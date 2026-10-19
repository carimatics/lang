//! Call-local C storage shared by VM and native import adapters. Scalar/Unit
//! calls retain their original API; finite Copy objects use the same Session
//! F10/control and failure latch, without adding fuel positions.
use super::{AllocatorIdentity, BoundImport, Call, Fault, Kind, Session};
use crate::c_abi::{
    Digest, Kind as TypeKind,
    validation::{self, Address, Argument, Reader},
};
use lang_mir::unit::Span;
use std::cell::{Cell, RefCell};

/// One actual call-local profile-1 Unit C object.
#[repr(C)]
#[derive(Default)]
pub struct Unit {
    reserved: u8,
}
impl Reader for Unit {
    fn address(&self) -> Address {
        Address {
            start: (self as *const Self).addr(),
            extent: 1,
        }
    }
    fn read(&self, offset: usize, destination: &mut [u8]) -> Result<(), validation::Error> {
        let end = offset
            .checked_add(destination.len())
            .ok_or(validation::Error::Overflow)?;
        let bytes = [self.reserved];
        let source = bytes.get(offset..end).ok_or(validation::Error::Read {
            offset,
            bytes: destination.len(),
        })?;
        destination.copy_from_slice(source);
        Ok(())
    }
}
#[derive(Clone, Copy)]
pub enum Input<'a> {
    I64(i64),
    Bool(u8),
    Unit(&'a Unit),
    Object(&'a [u8]),
}
impl Reader for Input<'_> {
    fn address(&self) -> Address {
        match self {
            Self::Unit(unit) => unit.address(),
            Self::Object(bytes) => Address::from_slice(bytes),
            _ => Address {
                start: 0,
                extent: 0,
            },
        }
    }
    fn read(&self, offset: usize, destination: &mut [u8]) -> Result<(), validation::Error> {
        match self {
            Self::Unit(unit) => unit.read(offset, destination),
            Self::Object(bytes) => validation::Bytes(bytes).read(offset, destination),
            _ => Err(validation::Error::Read {
                offset,
                bytes: destination.len(),
            }),
        }
    }
}

/// Initialized, call-local wire storage, separate from every const input.
/// Native adapters must restore initialized padding before returning access to
/// Rust. Only the selected type's initialized fields carry source meaning.
pub struct ObjectOutput<'a>(RefCell<&'a mut [u8]>);
impl<'a> ObjectOutput<'a> {
    pub fn new(bytes: &'a mut [u8]) -> Self {
        Self(RefCell::new(bytes))
    }
    pub fn with_bytes<T>(&self, f: impl FnOnce(&mut [u8]) -> T) -> T {
        f(&mut self.0.borrow_mut())
    }
}
impl Reader for ObjectOutput<'_> {
    fn address(&self) -> Address {
        Address::from_slice(&self.0.borrow())
    }
    fn read(&self, offset: usize, destination: &mut [u8]) -> Result<(), validation::Error> {
        validation::Bytes(&self.0.borrow()).read(offset, destination)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scalar {
    I64(i64),
    Bool(u8),
    Unit,
}
/// Actual initialized C-width objects, with interior mutability for the native
/// callback. Rust bool/enum layouts are never exposed or constructed from C.
#[derive(Clone, Copy)]
pub enum Output<'a> {
    I64(&'a Cell<i64>),
    Bool(&'a Cell<u8>),
    Unit,
    Object(&'a ObjectOutput<'a>),
}
impl Reader for Output<'_> {
    fn address(&self) -> Address {
        match self {
            Self::Object(value) => value.address(),
            Self::I64(value) => Address {
                start: value.as_ptr().addr(),
                extent: 8,
            },
            Self::Bool(value) => Address {
                start: value.as_ptr().addr(),
                extent: 1,
            },
            Self::Unit => Address {
                start: 0,
                extent: 0,
            },
        }
    }
    fn read(&self, offset: usize, destination: &mut [u8]) -> Result<(), validation::Error> {
        if let Self::Object(value) = self {
            return value.read(offset, destination);
        }
        let end = offset
            .checked_add(destination.len())
            .ok_or(validation::Error::Overflow)?;
        let bytes = match self {
            Self::I64(value) => value.get().to_ne_bytes(),
            Self::Bool(value) => [value.get(), 0, 0, 0, 0, 0, 0, 0],
            Self::Unit => return Err(validation::Error::Shape(validation::Object::Result)),
            Self::Object(_) => unreachable!(),
        };
        let size = self.address().extent;
        if end > size {
            return Err(validation::Error::Read {
                offset,
                bytes: destination.len(),
            });
        }
        destination.copy_from_slice(&bytes[offset..end]);
        Ok(())
    }
}
/// Trusted host primitive, explicitly installed for one context. Native
/// implementations validate their exact signatures and binding ownership;
/// implementations must not unwind or retain call-local inputs/outputs.
pub trait Invoker {
    fn artifact(&self) -> Digest;
    fn allocator_identity(&self) -> AllocatorIdentity;
    fn context(&self) -> Address;
    fn invoke(&mut self, binding: &BoundImport, arguments: &[Input<'_>], output: Output<'_>)
    -> u32;
}
/// Trusted allocator primitive installed from this context's descriptor. Source
/// execution also checks its actual accounting identity and configured limits.
pub trait ContextProvider: crate::allocation::Provider {
    fn context_identity(&self) -> AllocatorIdentity;
}
impl Session<'_> {
    pub fn invoke_scalars(
        &mut self,
        function: usize,
        span: Option<Span>,
        arguments: &[Scalar],
        invoker: &mut (impl Invoker + ?Sized),
    ) -> Result<Scalar, Fault> {
        if let Some(fault) = self.failure {
            return Err(fault);
        }
        let checked = (|| {
            if invoker.artifact() != self.artifact.digest()
                || invoker.allocator_identity() != self.allocator_identity
            {
                return Err(Fault::new(Kind::WrongProgram));
            }
            let metadata = self
                .artifact
                .functions()
                .get(function)
                .ok_or_else(|| Fault::new(Kind::Function(function)))?;
            if arguments.len() > 256
                || metadata
                    .parameters
                    .iter()
                    .chain(std::iter::once(&metadata.result))
                    .any(|ty| {
                        !matches!(
                            self.artifact.types()[ty.0].kind,
                            TypeKind::I64 | TypeKind::Bool | TypeKind::Unit
                        )
                    })
            {
                return Err(Fault::new(Kind::Function(function)));
            }
            let integer = Cell::new(0i64);
            let boolean = Cell::new(0u8);
            let output = match self.artifact.types()[metadata.result.0].kind {
                TypeKind::I64 => Output::I64(&integer),
                TypeKind::Bool => Output::Bool(&boolean),
                TypeKind::Unit => Output::Unit,
                _ => unreachable!("checked scalar result"),
            };
            let unit = Unit::default();
            let inputs: [Input<'_>; 256] = std::array::from_fn(|i| match arguments.get(i) {
                Some(Scalar::I64(value)) => Input::I64(*value),
                Some(Scalar::Bool(value)) => Input::Bool(*value),
                Some(Scalar::Unit) => Input::Unit(&unit),
                None => Input::I64(0),
            });
            let args: [Argument<'_>; 256] = std::array::from_fn(|i| match inputs[i] {
                Input::I64(value) => Argument::I64(value),
                Input::Bool(value) => Argument::Bool(value),
                Input::Unit(unit) => Argument::Object(unit),
                Input::Object(_) => Argument::Object(&inputs[i]),
            });
            let result = if matches!(output, Output::Unit) {
                None
            } else {
                Some(&output as &dyn Reader)
            };
            self.invoke(
                function,
                Call {
                    span,
                    context: invoker.context(),
                    arguments: &args[..arguments.len()],
                    output: result.map(Reader::address),
                },
                result,
                &mut |binding| invoker.invoke(binding, &inputs[..arguments.len()], output),
            )?;
            Ok(match output {
                Output::I64(value) => Scalar::I64(value.get()),
                Output::Bool(value) => Scalar::Bool(value.get()),
                Output::Unit => Scalar::Unit,
                Output::Object(_) => unreachable!(),
            })
        })();
        if let Err(mut fault) = checked {
            fault.span = span.or(fault.span);
            self.failure = Some(fault);
            return Err(fault);
        }
        checked
    }
}

impl Session<'_> {
    /// Invoke already marshalled finite Copy objects through the same context
    /// gate as scalars. The caller retains every input and staged result until
    /// validation finishes, then translates fields into unpublished typed cells.
    pub fn invoke_copy(
        &mut self,
        function: usize,
        span: Option<Span>,
        inputs: &[Input<'_>],
        output: Output<'_>,
        invoker: &mut (impl Invoker + ?Sized),
    ) -> Result<(), Fault> {
        if let Some(fault) = self.failure {
            return Err(fault);
        }
        let checked = (|| {
            if invoker.artifact() != self.artifact.digest()
                || invoker.allocator_identity() != self.allocator_identity
            {
                return Err(Fault::new(Kind::WrongProgram));
            }
            if inputs.len() > 256 {
                return Err(Fault::new(Kind::Function(function)));
            }
            let arguments: [Argument<'_>; 256] = std::array::from_fn(|i| match inputs.get(i) {
                Some(Input::I64(value)) => Argument::I64(*value),
                Some(Input::Bool(value)) => Argument::Bool(*value),
                Some(input) => Argument::Object(input),
                None => Argument::I64(0),
            });
            let result = if matches!(output, Output::Unit) {
                None
            } else {
                Some(&output as &dyn Reader)
            };
            self.invoke(
                function,
                Call {
                    span,
                    context: invoker.context(),
                    arguments: &arguments[..inputs.len()],
                    output: result.map(Reader::address),
                },
                result,
                &mut |binding| invoker.invoke(binding, inputs, output),
            )
        })();
        if let Err(mut fault) = checked {
            fault.span = span.or(fault.span);
            self.failure = Some(fault);
            return Err(fault);
        }
        checked
    }
}
