//! Detached compiler records for trusted linked code, not a wire format.
use super::super::program::FunctionCode;
use super::*;

/// Original object bytes and code metadata without borrows of the producer IR.
/// This is a Rust-only trusted compiler record; it is not a package decoder.
pub struct RecordedObject {
    bytes: Vec<u8>,
    target: crate::Triple,
    scope: CompileScope,
    functions: Vec<FunctionCode>,
}
#[derive(Debug)]
pub enum RecordedError {
    Preparation(Error),
    FunctionCount {
        expected: usize,
        recorded: usize,
    },
    FunctionIdentity {
        expected: m::FunctionId,
        recorded: m::FunctionId,
    },
    Symbol {
        function: m::FunctionId,
    },
    CodeRange {
        function: m::FunctionId,
    },
    SourceSpan {
        function: m::FunctionId,
    },
}
impl Display for RecordedError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "linked metadata preparation failed: {self:?}")
    }
}
impl std::error::Error for RecordedError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Preparation(e) => Some(e),
            _ => None,
        }
    }
}
impl From<Error> for RecordedError {
    fn from(error: Error) -> Self {
        Self::Preparation(error)
    }
}
impl ObjectArtifact<'_> {
    /// Detach owned code/records so producer IR/layouts may be released.
    /// The records retain exact code sizes/source ranges and private ABI scope.
    pub fn into_recorded(self, limits: m::Limits) -> Result<RecordedObject, Error> {
        let mut budget = m::Budget::with_usage(limits, self.usage)?;
        let mut functions = budget.vector(self.functions.len())?;
        for function in self.functions {
            functions.push(function.metadata);
        }
        Ok(RecordedObject {
            bytes: self.bytes,
            target: self.target,
            scope: self.scope,
            functions,
        })
    }
}
impl RecordedObject {
    pub(super) fn compilation_scope(&self) -> CompileScope {
        self.scope
    }
    pub(super) fn compilation_target(&self) -> &crate::Triple {
        &self.target
    }
    /// Package trusted compiler output as RFC 0018 data. No code is loaded.
    pub fn into_record(self) -> crate::native_record::Record {
        use crate::native_record::{Kind, Record};
        let kind = match self.scope {
            CompileScope::Function => Kind::OwnershipFunction,
            CompileScope::Unit => Kind::OwnershipUnit,
            CompileScope::CUnit => Kind::CUnit,
            CompileScope::CObject(digest) => Kind::CObject(digest),
        };
        Record::new(self.target, kind, self.bytes, self.functions)
    }
    /// Recover records from validated data; preparation and the full unsafe
    /// exact-code/image binding contract remain necessary before execution.
    pub fn from_record(
        record: crate::native_record::Record,
    ) -> Result<Self, crate::native_record::Error> {
        use crate::native_record::{Error, ErrorKind, Kind};
        let scope = match record.kind {
            Kind::Copy => {
                return Err(Error {
                    offset: 17,
                    kind: ErrorKind::Profile,
                });
            }
            Kind::OwnershipFunction => CompileScope::Function,
            Kind::OwnershipUnit => CompileScope::Unit,
            Kind::CUnit => CompileScope::CUnit,
            Kind::CObject(digest) => CompileScope::CObject(digest),
        };
        Ok(Self {
            target: record.target,
            scope,
            bytes: record.bytes,
            functions: record.functions,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn metadata(&self, function: m::FunctionId) -> Option<&FunctionCode> {
        self.functions.get(function.0 as usize)
    }
    /// Rebuild runtime plans without ISA lookup, code emission, JIT construction,
    /// source analysis, or a symbol resolver.
    ///
    /// The flow must describe the same program that produced this trusted record.
    /// Structural checks do not prove semantic equivalence or authenticate native
    /// bytes. ObjectArtifact::bind still requires its full unsafe exact-code,
    /// toolchain and image contract. No serialized metadata is admitted.
    pub fn prepare<'a>(
        self,
        flow: &'a f::Elaborated,
        layouts: &'a Layouts<'a>,
        limits: m::Limits,
        cells: usize,
    ) -> Result<ObjectArtifact<'a>, RecordedError> {
        if !std::ptr::eq(layouts.types(), typed(flow).places().declarations().types()) {
            return Err(Error::Layout(l::Error::ForeignLocation).into());
        }
        let Self {
            bytes,
            target,
            scope,
            functions: records,
        } = self;
        let expected = flow.functions().len();
        if records.len() != expected {
            return Err(RecordedError::FunctionCount {
                expected,
                recorded: records.len(),
            });
        }
        let mut budget = m::Budget::with_usage(limits, FunctionLayout::base_usage(flow, layouts)?)
            .map_err(Error::Resource)?;
        budget
            .allocate::<u8>(bytes.capacity())
            .map_err(Error::Resource)?;
        budget
            .allocate::<FunctionCode>(records.capacity())
            .map_err(Error::Resource)?;
        for record in &records {
            budget
                .allocate::<u8>(record.symbol.capacity())
                .map_err(Error::Resource)?;
            budget
                .allocate::<super::super::program::SourceRange>(record.source_ranges.capacity())
                .map_err(Error::Resource)?;
        }
        let mut functions = budget.vector(expected).map_err(Error::Resource)?;
        let mut usage = m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        };
        for (index, metadata) in records.into_iter().enumerate() {
            let function = m::FunctionId(index as u32);
            if metadata.function != function {
                return Err(RecordedError::FunctionIdentity {
                    expected: function,
                    recorded: metadata.function,
                });
            }
            let Prepared {
                layout,
                nodes,
                continuations,
                mut budget,
            } = prepare(flow, layouts, function, limits, cells, Some(usage), scope)?;
            let symbol = function_symbol(flow, function, scope);
            budget
                .allocate::<u8>(symbol.capacity())
                .map_err(Error::Resource)?;
            if metadata.symbol != symbol {
                return Err(RecordedError::Symbol { function });
            }
            if metadata.code_size == 0 {
                return Err(RecordedError::CodeRange { function });
            }
            budget
                .charge(metadata.source_ranges.len())
                .map_err(Error::Resource)?;
            for range in &metadata.source_ranges {
                if range.start >= range.end || range.end as usize > metadata.code_size {
                    return Err(RecordedError::CodeRange { function });
                }
                let valid = layouts
                    .types()
                    .table()
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
                    return Err(RecordedError::SourceSpan { function });
                }
            }
            usage = m::Usage {
                work: budget.work_used(),
                allocation_bytes: budget.allocation_bytes(),
                elements: budget.elements_used(),
            };
            functions.push(ObjectFunction {
                layout,
                nodes,
                continuations,
                metadata,
                failure_node: flow.functions()[index].failure().0,
            });
        }
        Ok(ObjectArtifact {
            bytes,
            target,
            scope,
            functions,
            usage,
        })
    }
}
