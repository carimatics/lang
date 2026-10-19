//! Checked preparation and retained linked-image ownership for Copy records.
use super::*;
use crate::native_record::{Kind, Record};
use std::{ffi::c_void, ptr::NonNull};

#[derive(Debug)]
pub enum BindError {
    Preparation(Error),
    Profile,
    Target,
    FunctionCount {
        expected: usize,
        recorded: usize,
    },
    Metadata {
        function: m::FunctionId,
    },
    SourceSpan {
        function: m::FunctionId,
    },
    MissingSymbol {
        function: m::FunctionId,
        symbol: String,
    },
}
impl Display for BindError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Copy object binding failed: {self:?}")
    }
}
impl StdError for BindError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Preparation(e) => Some(e),
            _ => None,
        }
    }
}
impl From<Error> for BindError {
    fn from(e: Error) -> Self {
        Self::Preparation(e)
    }
}
pub struct PreparedObject<'a> {
    target: Triple,
    verified: &'a m::VerifiedUnit,
    artifact: ObjectArtifact,
    usage: m::Usage,
}
impl Record {
    /// Revalidate the complete Copy domain and metadata without code generation.
    /// The domain must be independently decoded from the exact semantic bytes
    /// used to validate this record; native equivalence remains a bind obligation.
    /// Host imports must be explicitly supplied and linked by the trusted host.
    pub fn prepare_copy<'a>(
        self,
        verified: &'a m::VerifiedUnit,
        imports: &[HostFunctionId],
        limits: m::Limits,
    ) -> Result<PreparedObject<'a>, BindError> {
        if self.kind != Kind::Copy {
            return Err(BindError::Profile);
        }
        let mut budget = preflight(verified, imports, limits)?;
        let expected = verified.unit().functions.len();
        if self.functions.len() != expected {
            return Err(BindError::FunctionCount {
                expected,
                recorded: self.functions.len(),
            });
        }
        budget
            .allocate::<u8>(self.bytes.capacity())
            .map_err(Error::Verification)?;
        budget
            .allocate::<FunctionCode>(self.functions.capacity())
            .map_err(Error::Verification)?;
        for (index, metadata) in self.functions.iter().enumerate() {
            let function = m::FunctionId(index as u32);
            budget.charge(1).map_err(Error::Verification)?;
            budget
                .allocate::<u8>(metadata.symbol.capacity())
                .map_err(Error::Verification)?;
            budget
                .allocate::<SourceRange>(metadata.source_ranges.capacity())
                .map_err(Error::Verification)?;
            let valid = metadata
                .symbol
                .strip_prefix("lang_unit_f")
                .is_some_and(|tail| {
                    !tail.is_empty()
                        && tail.bytes().all(|b| b.is_ascii_digit())
                        && (tail == "0" || !tail.starts_with('0'))
                        && tail.parse::<usize>() == Ok(index)
                });
            if metadata.function != function || !valid || metadata.code_size == 0 {
                return Err(BindError::Metadata { function });
            }
            budget
                .charge(metadata.source_ranges.len())
                .map_err(Error::Verification)?;
            for range in &metadata.source_ranges {
                if range.start >= range.end || range.end as usize > metadata.code_size {
                    return Err(BindError::Metadata { function });
                }
                let valid = verified
                    .unit()
                    .sources
                    .get(range.span.source.0 as usize)
                    .is_some_and(|source| {
                        let start = range.span.start as usize;
                        let end = range.span.end as usize;
                        start <= end
                            && end <= source.text.len()
                            && source.text.is_char_boundary(start)
                            && source.text.is_char_boundary(end)
                    });
                if !valid {
                    return Err(BindError::SourceSpan { function });
                }
            }
        }
        let usage = m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        };
        Ok(PreparedObject {
            target: self.target,
            verified,
            artifact: ObjectArtifact {
                bytes: self.bytes,
                functions: self.functions,
            },
            usage,
        })
    }
}
impl<'a> PreparedObject<'a> {
    pub fn bytes(&self) -> &[u8] {
        &self.artifact.bytes
    }
    pub fn metadata(&self) -> &[FunctionCode] {
        &self.artifact.functions
    }
    pub fn usage(&self) -> m::Usage {
        self.usage
    }

    /// # Safety
    /// The image must retain exactly this object's code and all linked imports,
    /// built by the matching compiler/toolchain for this target. Every resolved
    /// address must be that symbol's live private Copy cell-buffer entry. The
    /// supplied import IDs/bindings must match the object's original imports.
    /// No untrusted native code, fabricated address or early unload is allowed.
    pub unsafe fn bind<Image>(
        mut self,
        image: Image,
        mut resolve: impl FnMut(&Image, &str) -> Option<NonNull<c_void>>,
        limits: m::Limits,
    ) -> Result<LinkedProgram<'a, Image>, BindError> {
        if self.target != Triple::host() {
            return Err(BindError::Target);
        }
        let mut budget = m::Budget::with_usage(limits, self.usage).map_err(Error::Verification)?;
        budget
            .charge(self.artifact.functions.len())
            .map_err(Error::Verification)?;
        let mut pointers = budget
            .vector(self.artifact.functions.len())
            .map_err(Error::Verification)?;
        for metadata in &mut self.artifact.functions {
            let pointer =
                resolve(&image, &metadata.symbol).ok_or_else(|| BindError::MissingSymbol {
                    function: metadata.function,
                    symbol: std::mem::take(&mut metadata.symbol),
                })?;
            pointers.push(pointer.as_ptr().cast_const().cast());
        }
        Ok(LinkedProgram {
            _image: image,
            verified: self.verified,
            pointers,
            artifact: self.artifact,
        })
    }
}
pub struct LinkedProgram<'a, Image> {
    _image: Image,
    verified: &'a m::VerifiedUnit,
    pointers: Vec<*const u8>,
    artifact: ObjectArtifact,
}
impl<Image> LinkedProgram<'_, Image> {
    pub fn metadata(&self) -> &[FunctionCode] {
        &self.artifact.functions
    }
    /// Same canonical value checks/private ABI as JIT Copy execution.
    /// This is trusted synchronous native execution, without VM fuel controls.
    pub fn invoke(
        &self,
        function: m::FunctionId,
        arguments: &[Value],
    ) -> Result<Value, InvokeError> {
        invoke(self.verified, &self.pointers, function, arguments)
    }
}
