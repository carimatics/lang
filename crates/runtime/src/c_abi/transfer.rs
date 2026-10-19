//! Field-wise finite Copy transfers. C offsets never stand in for managed
//! offsets. One call-local wire allocation uses the actual guest allocator.
use super::{
    Artifact, Kind, TypeId,
    context::{
        self as c,
        scalar::{Input, Invoker, ObjectOutput, Output},
    },
};
use crate::{allocation as a, layout as l};
use lang_mir::unit::{Span, ownership as o};

/// Unpublished scratch retained by its allocator. Adapters explicitly release
/// it on success and failure; dropping this token does not invoke a provider.
pub struct CallStorage {
    artifact: super::Digest,
    function: usize,
    handle: a::Handle,
    offsets: [usize; 257],
    count: usize,
    result: usize,
}
fn bad() -> l::Error {
    l::Error::State
}
fn number(n: u64) -> Result<usize, l::Error> {
    usize::try_from(n).map_err(|_| l::Error::Overflow)
}
fn add(a: usize, b: usize) -> Result<usize, l::Error> {
    a.checked_add(b).ok_or(l::Error::Overflow)
}
fn aligned(n: usize, alignment: usize) -> Result<usize, l::Error> {
    Ok(add(n, alignment - 1)? & !(alignment - 1))
}
impl CallStorage {
    pub fn reserve<P: a::Provider>(
        artifact: &Artifact,
        function: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Self, l::Error> {
        let metadata = artifact.functions().get(function).ok_or_else(bad)?;
        if metadata.parameters.len() > 256 {
            return Err(bad());
        }
        let mut offsets = [0; 257];
        let mut size = 0;
        let mut alignment = 1;
        for (index, id) in metadata.parameters.iter().enumerate() {
            let ty = &artifact.types()[id.0];
            size = aligned(size, number(ty.align)?)?;
            alignment = alignment.max(ty.align);
            offsets[index] = size;
            size = add(size, number(ty.size)?)?;
        }
        let result_type = &artifact.types()[metadata.result.0];
        size = aligned(size, number(result_type.align)?)?;
        alignment = alignment.max(result_type.align);
        offsets[metadata.parameters.len()] = size;
        let result = size;
        size = add(size, number(result_type.size)?)?;
        let handle = allocator.allocate(a::Request::new(size as u64, alignment)?)?;
        allocator.bytes_mut(handle)?.fill(0);
        Ok(Self {
            artifact: artifact.digest(),
            function,
            handle,
            offsets,
            count: metadata.parameters.len(),
            result,
        })
    }
    pub fn encode<P: a::Provider>(
        &self,
        index: usize,
        artifact: &Artifact,
        function: usize,
        source: &l::Reservation<'_, '_>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), l::Error> {
        if artifact.digest() != self.artifact
            || function != self.function
            || index >= self.count
            || !source
                .layouts()
                .types()
                .properties(source.root().ty())
                .is_some_and(|p| p.is_copy)
        {
            return Err(bad());
        }
        let id = *artifact
            .functions()
            .get(function)
            .and_then(|f| f.parameters.get(index))
            .ok_or_else(bad)?;
        encode(
            artifact,
            id,
            source,
            source.root(),
            self.handle,
            self.offsets[index],
            allocator,
        )
    }
    pub fn decode<P: a::Provider>(
        &self,
        artifact: &Artifact,
        function: usize,
        target: &l::Reservation<'_, '_>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), l::Error> {
        if artifact.digest() != self.artifact
            || function != self.function
            || !target
                .layouts()
                .types()
                .properties(target.root().ty())
                .is_some_and(|p| p.is_copy)
        {
            return Err(bad());
        }
        let id = artifact.functions().get(function).ok_or_else(bad)?.result;
        decode(
            artifact,
            id,
            target,
            target.root(),
            self.handle,
            self.result,
            allocator,
        )
    }
    pub fn invoke<P: a::Provider>(
        &self,
        function: usize,
        span: Span,
        session: &mut c::Session<'_>,
        invoker: &mut dyn Invoker,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), c::Fault> {
        let fault = || {
            c::Fault::new(c::Kind::Execution(
                crate::cleanup::ExecutionFailure::Invariant,
            ))
            .at(span)
        };
        if session.artifact().digest() != self.artifact || function != self.function {
            return Err(fault());
        }
        let bytes = allocator.bytes_mut(self.handle).map_err(|_| fault())?;
        let (prefix, result) = bytes.split_at_mut(self.result);
        let metadata = session
            .artifact()
            .functions()
            .get(function)
            .ok_or_else(fault)?;
        let inputs: [Input<'_>; 256] = std::array::from_fn(|i| {
            if i >= self.count {
                return Input::I64(0);
            }
            let ty = &session.artifact().types()[metadata.parameters[i].0];
            let bytes = &prefix[self.offsets[i]..self.offsets[i] + ty.size as usize];
            match ty.kind {
                Kind::I64 => Input::I64(i64::from_ne_bytes(bytes.try_into().expect("C i64 size"))),
                Kind::Bool => Input::Bool(bytes[0]),
                _ => Input::Object(bytes),
            }
        });
        let unit = matches!(
            session.artifact().types()[metadata.result.0].kind,
            Kind::Unit
        );
        let object = ObjectOutput::new(result);
        let output = if unit {
            Output::Unit
        } else {
            Output::Object(&object)
        };
        session.invoke_copy(function, Some(span), &inputs[..self.count], output, invoker)
    }
    pub fn release<P: a::Provider>(self, allocator: &mut a::Allocator<P>) -> Result<(), l::Error> {
        allocator.release(self.handle).map_err(Into::into)
    }
}

fn write<P: a::Provider>(
    allocator: &mut a::Allocator<P>,
    handle: a::Handle,
    offset: usize,
    bytes: &[u8],
) -> Result<(), l::Error> {
    allocator
        .bytes_mut(handle)?
        .get_mut(offset..add(offset, bytes.len())?)
        .ok_or_else(bad)?
        .copy_from_slice(bytes);
    Ok(())
}
fn read<const N: usize, P: a::Provider>(
    allocator: &a::Allocator<P>,
    handle: a::Handle,
    offset: usize,
) -> Result<[u8; N], l::Error> {
    allocator
        .bytes(handle)?
        .get(offset..add(offset, N)?)
        .ok_or_else(bad)?
        .try_into()
        .map_err(|_| bad())
}
fn encode_immediate<P: a::Provider>(
    artifact: &Artifact,
    id: TypeId,
    value: l::Immediate,
    handle: a::Handle,
    offset: usize,
    allocator: &mut a::Allocator<P>,
) -> Result<(), l::Error> {
    match (&artifact.types()[id.0].kind, value) {
        (Kind::I64, l::Immediate::I64(value)) => {
            write(allocator, handle, offset, &value.to_ne_bytes())
        }
        (Kind::Bool, l::Immediate::Bool(value)) => {
            write(allocator, handle, offset, &[u8::from(value)])
        }
        (Kind::Unit, l::Immediate::Unit) => Ok(()),
        (
            Kind::Enum {
                cases,
                payload_offset,
                ..
            },
            value,
        ) => {
            let tag = value.case().ok_or_else(bad)?;
            let case = cases.get(tag as usize).ok_or_else(bad)?;
            write(allocator, handle, offset, &tag.to_ne_bytes())?;
            match (case.payload.as_slice(), value.payload()) {
                ([], None) => Ok(()),
                ([id], Some(value)) => encode_immediate(
                    artifact,
                    *id,
                    value,
                    handle,
                    add(offset, number(*payload_offset + case.offsets[0])?)?,
                    allocator,
                ),
                _ => Err(bad()),
            }
        }
        _ => Err(bad()),
    }
}
fn decode_immediate<P: a::Provider>(
    artifact: &Artifact,
    id: TypeId,
    builtin: o::Builtin,
    handle: a::Handle,
    offset: usize,
    allocator: &a::Allocator<P>,
) -> Result<l::Immediate, l::Error> {
    let Kind::Enum {
        cases,
        payload_offset,
        ..
    } = &artifact.types()[id.0].kind
    else {
        return Err(bad());
    };
    let tag = u32::from_ne_bytes(read(allocator, handle, offset)?);
    let case = cases.get(tag as usize).ok_or_else(bad)?;
    let payload = match case.payload.as_slice() {
        [] => None,
        [id] => {
            let at = add(offset, number(*payload_offset + case.offsets[0])?)?;
            Some(match &artifact.types()[id.0].kind {
                Kind::I64 => l::Immediate::I64(i64::from_ne_bytes(read(allocator, handle, at)?)),
                Kind::Enum { .. } => {
                    decode_immediate(artifact, *id, o::Builtin::AllocError, handle, at, allocator)?
                }
                _ => return Err(bad()),
            })
        }
        _ => return Err(bad()),
    };
    l::Immediate::from_case(builtin, tag, payload)
}
fn encode<P: a::Provider>(
    artifact: &Artifact,
    id: TypeId,
    source: &l::Reservation<'_, '_>,
    location: l::Location,
    handle: a::Handle,
    offset: usize,
    allocator: &mut a::Allocator<P>,
) -> Result<(), l::Error> {
    let layout = source.layouts().get(location.ty()).ok_or_else(bad)?;
    if matches!(layout.kind, l::Kind::Error { .. }) {
        return encode_immediate(
            artifact,
            id,
            source.read_immediate(location, allocator)?,
            handle,
            offset,
            allocator,
        );
    }
    match &artifact.types()[id.0].kind {
        Kind::I64 | Kind::Bool | Kind::Unit => encode_immediate(
            artifact,
            id,
            source.read_immediate(location, allocator)?,
            handle,
            offset,
            allocator,
        ),
        Kind::Record { fields, .. } => {
            for (i, field) in fields.iter().enumerate() {
                encode(
                    artifact,
                    field.ty,
                    source,
                    source.child(location, i as u32, allocator)?,
                    handle,
                    add(offset, number(field.offset)?)?,
                    allocator,
                )?;
            }
            Ok(())
        }
        Kind::Enum {
            cases,
            payload_offset,
            ..
        } => {
            let tag = source.case(location, allocator)?;
            let case = cases.get(tag as usize).ok_or_else(bad)?;
            write(allocator, handle, offset, &tag.to_ne_bytes())?;
            for (i, (id, at)) in case.payload.iter().zip(&case.offsets).enumerate() {
                encode(
                    artifact,
                    *id,
                    source,
                    source.child(location, i as u32, allocator)?,
                    handle,
                    add(offset, number(*payload_offset + *at)?)?,
                    allocator,
                )?;
            }
            Ok(())
        }
    }
}
fn decode<P: a::Provider>(
    artifact: &Artifact,
    id: TypeId,
    target: &l::Reservation<'_, '_>,
    location: l::Location,
    handle: a::Handle,
    offset: usize,
    allocator: &mut a::Allocator<P>,
) -> Result<(), l::Error> {
    let layout = target.layouts().get(location.ty()).ok_or_else(bad)?;
    if let l::Kind::Error { builtin, .. } = layout.kind {
        let value = decode_immediate(artifact, id, builtin, handle, offset, allocator)?;
        return target.write_immediate(location, value, allocator);
    }
    match &artifact.types()[id.0].kind {
        Kind::I64 => target.write_immediate(
            location,
            l::Immediate::I64(i64::from_ne_bytes(read(allocator, handle, offset)?)),
            allocator,
        ),
        Kind::Bool => {
            let [value] = read(allocator, handle, offset)?;
            if value > 1 {
                return Err(bad());
            }
            target.write_immediate(location, l::Immediate::Bool(value == 1), allocator)
        }
        Kind::Unit => target.write_immediate(location, l::Immediate::Unit, allocator),
        Kind::Record { fields, .. } => {
            for (i, field) in fields.iter().enumerate() {
                decode(
                    artifact,
                    field.ty,
                    target,
                    target.child(location, i as u32, allocator)?,
                    handle,
                    add(offset, number(field.offset)?)?,
                    allocator,
                )?;
            }
            Ok(())
        }
        Kind::Enum {
            cases,
            payload_offset,
            ..
        } => {
            let tag = u32::from_ne_bytes(read(allocator, handle, offset)?);
            let case = cases.get(tag as usize).ok_or_else(bad)?;
            target.prepare_case(location, tag, allocator)?;
            for (i, (id, at)) in case.payload.iter().zip(&case.offsets).enumerate() {
                decode(
                    artifact,
                    *id,
                    target,
                    target.child(location, i as u32, allocator)?,
                    handle,
                    add(offset, number(*payload_offset + *at)?)?,
                    allocator,
                )?;
            }
            Ok(())
        }
    }
}
