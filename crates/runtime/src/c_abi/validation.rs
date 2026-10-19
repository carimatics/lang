//! Shared profile-1 C address/value preflight. Native adapters must first
//! establish actual object lifetime/access validity under RFC 0008, then use
//! this address-only pass before creating mutable references or reading input.
//! `Reader` requests only initialized fields and required canonical storage;
//! ordinary C padding can remain indeterminate and is never requested.
use super::{Artifact, Kind, Type, TypeId};
use crate::execution::{Control, Failure, ServiceWork};
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Address {
    pub start: usize,
    pub extent: usize,
}
impl Address {
    pub fn from_slice(bytes: &[u8]) -> Self {
        Self {
            start: bytes.as_ptr() as usize,
            extent: bytes.len(),
        }
    }
    fn checked(self, size: usize, align: usize, object: Object) -> Result<Range> {
        if self.start == 0 {
            return Err(Error::Storage {
                object,
                kind: Storage::Null,
            });
        }
        if !self.start.is_multiple_of(align) {
            return Err(Error::Storage {
                object,
                kind: Storage::Misaligned { align },
            });
        }
        if self.extent < size {
            return Err(Error::Storage {
                object,
                kind: Storage::Short {
                    required: size,
                    actual: self.extent,
                },
            });
        }
        if size > isize::MAX as usize {
            return Err(Error::Storage {
                object,
                kind: Storage::Overflow,
            });
        }
        let end = self.start.checked_add(size).ok_or(Error::Storage {
            object,
            kind: Storage::Overflow,
        })?;
        Ok(Range {
            start: self.start,
            end,
        })
    }
}
#[derive(Clone, Copy)]
struct Range {
    start: usize,
    end: usize,
}
impl Range {
    fn overlaps(self, other: Self) -> bool {
        self.start < other.end && other.start < self.end
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Object {
    Context,
    Argument(usize),
    Result,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Storage {
    Null,
    Misaligned { align: usize },
    Short { required: usize, actual: usize },
    Overflow,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Control(Failure),
    Function(usize),
    Type(TypeId),
    Arity { expected: usize, actual: usize },
    Shape(Object),
    Storage { object: Object, kind: Storage },
    OutputOverlap(Object),
    Read { offset: usize, bytes: usize },
    InvalidBool { offset: usize, value: u8 },
    InvalidTag { offset: usize, value: u32 },
    Reserved { offset: usize },
    UnusedUnion { offset: usize },
    Overflow,
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid C value boundary: {self:?}")
    }
}
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Control(error) => Some(error),
            _ => None,
        }
    }
}
impl From<Failure> for Error {
    fn from(error: Failure) -> Self {
        Self::Control(error)
    }
}
type Result<T> = std::result::Result<T, Error>;

/// A synchronous read of the same stable C object throughout validation.
/// All bytes requested by `read` must be initialized. Native implementations
/// must be constructed under the caller's actual-object/lifetime contract;
/// they must not first form a u8 slice over potentially indeterminate padding.
/// Successful reads fill the entire requested destination; no guest body,
/// allocator callback, or foreign callback is invoked by this interface.
pub trait Reader {
    fn address(&self) -> Address;
    fn read(&self, offset: usize, destination: &mut [u8]) -> Result<()>;
}
pub struct Bytes<'a>(pub &'a [u8]);
impl Reader for Bytes<'_> {
    fn address(&self) -> Address {
        Address::from_slice(self.0)
    }
    fn read(&self, offset: usize, destination: &mut [u8]) -> Result<()> {
        let end = offset
            .checked_add(destination.len())
            .ok_or(Error::Overflow)?;
        let bytes = self.0.get(offset..end).ok_or(Error::Read {
            offset,
            bytes: destination.len(),
        })?;
        destination.copy_from_slice(bytes);
        Ok(())
    }
}
pub enum Argument<'a> {
    I64(i64),
    Bool(u8),
    Object(&'a dyn Reader),
}

/// Validate the complete call's shapes and addresses before requesting any
/// input bytes. The result is not read or written; it may be uninitialized.
/// Scalar arguments pass by value; all other arguments pass by const pointer.
/// Input/input overlap is allowed. Output must be disjoint from every input
/// object and the complete opaque context object.
pub fn call(
    artifact: &Artifact,
    function: usize,
    context: Address,
    arguments: &[Argument<'_>],
    output: Option<Address>,
    work: &ServiceWork,
    control: Control<'_>,
) -> Result<()> {
    control.check()?;
    let function = artifact
        .functions()
        .get(function)
        .ok_or(Error::Function(function))?;
    if function.parameters.len() != arguments.len() {
        return Err(Error::Arity {
            expected: function.parameters.len(),
            actual: arguments.len(),
        });
    }
    let context = context.checked(context.extent.max(1), 8, Object::Context)?;
    let result = ty(artifact, function.result)?;
    let out = match (&result.kind, output) {
        (Kind::Unit, None) => None,
        (Kind::Unit, Some(_)) | (_, None) => return Err(Error::Shape(Object::Result)),
        (_, Some(address)) => {
            Some(address.checked(number(result.size)?, number(result.align)?, Object::Result)?)
        }
    };
    if out.is_some_and(|out| out.overlaps(context)) {
        return Err(Error::OutputOverlap(Object::Context));
    }
    for (i, (id, arg)) in function.parameters.iter().zip(arguments).enumerate() {
        control.check()?;
        work.charge(1)?;
        let expected = ty(artifact, *id)?;
        match (&expected.kind, arg) {
            (Kind::I64, Argument::I64(_)) | (Kind::Bool, Argument::Bool(_)) => {}
            (Kind::I64 | Kind::Bool, _) | (_, Argument::I64(_) | Argument::Bool(_)) => {
                return Err(Error::Shape(Object::Argument(i)));
            }
            (_, Argument::Object(reader)) => {
                let input = reader.address().checked(
                    number(expected.size)?,
                    number(expected.align)?,
                    Object::Argument(i),
                )?;
                if out.is_some_and(|out| out.overlaps(input)) {
                    return Err(Error::OutputOverlap(Object::Argument(i)));
                }
            }
        }
    }
    // No Reader::read occurred above, including for later malformed arguments.
    for (id, arg) in function.parameters.iter().zip(arguments) {
        match arg {
            Argument::I64(_) => {
                control.check()?;
                work.charge(1)?;
            }
            Argument::Bool(value) => {
                control.check()?;
                work.charge(1)?;
                if *value > 1 {
                    return Err(Error::InvalidBool {
                        offset: 0,
                        value: *value,
                    });
                }
            }
            Argument::Object(reader) => object(artifact, *id, *reader, work, control)?,
        }
    }
    control.check()?;
    Ok(())
}

/// Check a single complete object, including an import callback's successful
/// result, before it can become a source value. No output is modified.
pub fn object(
    artifact: &Artifact,
    value: TypeId,
    reader: &dyn Reader,
    work: &ServiceWork,
    control: Control<'_>,
) -> Result<()> {
    let root = ty(artifact, value)?;
    let address = reader.address();
    address.checked(number(root.size)?, number(root.align)?, Object::Result)?;
    let check = Check {
        artifact,
        reader,
        work,
        control,
        extent: address.extent,
    };
    check.value(value, 0)?;
    control.check()?;
    Ok(())
}
fn number(value: u64) -> Result<usize> {
    usize::try_from(value).map_err(|_| Error::Overflow)
}
fn ty(artifact: &Artifact, id: TypeId) -> Result<&Type> {
    artifact.types().get(id.0).ok_or(Error::Type(id))
}
#[derive(Clone, Copy)]
enum Zero {
    Reserved,
    UnusedUnion,
}
struct Check<'a, 'c> {
    artifact: &'a Artifact,
    reader: &'a dyn Reader,
    work: &'a ServiceWork,
    control: Control<'c>,
    extent: usize,
}
impl Check<'_, '_> {
    fn charge(&self, count: usize) -> Result<()> {
        self.control.check()?;
        self.work
            .charge(u64::try_from(count).map_err(|_| Error::Overflow)?)?;
        Ok(())
    }
    fn read(&self, offset: usize, bytes: &mut [u8]) -> Result<()> {
        let end = offset.checked_add(bytes.len()).ok_or(Error::Overflow)?;
        if end > self.extent {
            return Err(Error::Read {
                offset,
                bytes: bytes.len(),
            });
        }
        self.charge(bytes.len())?;
        self.reader.read(offset, bytes)
    }
    fn zero(&self, mut offset: usize, end: usize, kind: Zero) -> Result<()> {
        let mut bytes = [0; 64];
        while offset < end {
            let count = (end - offset).min(bytes.len());
            self.read(offset, &mut bytes[..count])?;
            if let Some(i) = bytes[..count].iter().position(|b| *b != 0) {
                return Err(match kind {
                    Zero::UnusedUnion => Error::UnusedUnion { offset: offset + i },
                    Zero::Reserved => Error::Reserved { offset: offset + i },
                });
            }
            offset += count;
        }
        Ok(())
    }
    fn value(&self, id: TypeId, offset: usize) -> Result<()> {
        self.charge(1)?;
        let value = ty(self.artifact, id)?;
        match &value.kind {
            Kind::I64 => self.read(offset, &mut [0; 8]),
            Kind::Bool => {
                let mut byte = [0];
                self.read(offset, &mut byte)?;
                if byte[0] > 1 {
                    Err(Error::InvalidBool {
                        offset,
                        value: byte[0],
                    })
                } else {
                    Ok(())
                }
            }
            Kind::Unit => self.zero(
                offset,
                offset.checked_add(1).ok_or(Error::Overflow)?,
                Zero::Reserved,
            ),
            Kind::Record { fields, .. } => {
                if fields.is_empty() {
                    self.zero(
                        offset,
                        offset.checked_add(1).ok_or(Error::Overflow)?,
                        Zero::Reserved,
                    )?;
                }
                for field in fields {
                    self.value(
                        field.ty,
                        offset
                            .checked_add(number(field.offset)?)
                            .ok_or(Error::Overflow)?,
                    )?;
                }
                Ok(())
            }
            Kind::Enum {
                cases,
                payload_offset,
                payload_size,
                ..
            } => {
                let mut tag = [0; 4];
                self.read(offset, &mut tag)?;
                let tag = u32::from_le_bytes(tag);
                let case = cases
                    .get(tag as usize)
                    .ok_or(Error::InvalidTag { offset, value: tag })?;
                let base = offset
                    .checked_add(number(*payload_offset)?)
                    .ok_or(Error::Overflow)?;
                if case.payload.is_empty() {
                    self.zero(
                        base,
                        base.checked_add(1).ok_or(Error::Overflow)?,
                        Zero::Reserved,
                    )?;
                }
                for (ty, at) in case.payload.iter().zip(&case.offsets) {
                    self.value(*ty, base.checked_add(number(*at)?).ok_or(Error::Overflow)?)?;
                }
                // Full active-case sizeof includes its ordinary padding.
                // The enclosing enum's own trailing padding is not union data.
                self.zero(
                    base.checked_add(number(case.size)?)
                        .ok_or(Error::Overflow)?,
                    base.checked_add(number(*payload_size)?)
                        .ok_or(Error::Overflow)?,
                    Zero::UnusedUnion,
                )
            }
        }
    }
}
