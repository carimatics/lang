//! RFC 0011/0014/0017 ownership schemas 2, 3 and 4. Raw tables carry no authority; decoding reruns
//! the complete independent ownership and region/failure verification pipeline.
#![allow(
    clippy::result_large_err,
    reason = "Preserve structured errors without allocating after OOM."
)]

use lang_mir::unit::{self as m, ownership as o};
use std::fmt::{Display, Formatter};

mod reader;
mod tags;
mod writer;

#[cfg(test)]
mod tests;

const MAGIC: &[u8; 8] = b"LANGBC\0\0";
const HEADER: usize = 16;

#[derive(Clone, Copy, Debug, Default)]
pub struct VerificationLimits {
    pub bodies: o::bodies::Limits,
    pub moves: o::moves::Limits,
    pub activity: o::loan_activity::Limits,
    pub loans: o::loans::Limits,
    pub failure: o::failure::Limits,
}
#[derive(Clone, Copy, Debug)]
pub struct CodecLimits {
    pub bytes: usize,
    pub verification: VerificationLimits,
}
impl Default for CodecLimits {
    fn default() -> Self {
        Self {
            bytes: 64 * 1024 * 1024,
            verification: VerificationLimits::default(),
        }
    }
}
impl CodecLimits {
    // A tighter phase cap tightens the whole load; a later phase cannot reset
    // counters or restore a larger work/allocation/element allowance.
    pub fn budget_limits(self) -> m::Limits {
        let v = self.verification;
        let mut common = v.bodies.places.ownership.unit;
        for (work, allocations, elements) in [
            (v.moves.work, v.moves.allocations, v.moves.elements),
            (v.activity.work, v.activity.allocations, v.activity.elements),
            (v.loans.work, v.loans.allocations, v.loans.elements),
            (v.failure.work, v.failure.allocations, v.failure.elements),
        ] {
            common.work = common.work.min(work);
            common.allocations = common.allocations.min(allocations);
            common.elements = common.elements.min(elements);
        }
        common
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Table {
    Sources,
    Definitions,
    Constructed,
    Imports,
    Functions,
    Fields,
    Locals,
    Values,
    Places,
    Blocks,
    Instructions,
    Loans,
    Prepared,
    Elements,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tag {
    Definition,
    ImportAbi,
    FunctionAbi,
    Type,
    Constructor,
    Builtin,
    LocalKind,
    Root,
    Projection,
    Instruction,
    CoreOperation,
    CleanupTarget,
    LoanKind,
    EndMode,
    Terminator,
    OptionalId,
}
#[derive(Debug)]
pub enum CodecErrorKind {
    TooLarge {
        limit: usize,
        requested: usize,
    },
    Truncated,
    BadMagic,
    Version(u16),
    ProfileVersion(u16),
    Flags(u16),
    LengthMismatch,
    TrailingBytes,
    InvalidUtf8,
    InvalidBoolean(u8),
    InvalidTag {
        tag: Tag,
        value: u8,
    },
    Limit {
        table: Table,
        limit: usize,
        requested: usize,
    },
    IntegerOverflow,
    AllocationFailed,
    InvalidEntry,
    UnsupportedCInterface {
        span: m::Span,
    },
    UnsupportedChannelProfile {
        span: lang_mir::unit::Span,
    },
    UnsupportedServiceProfile {
        span: m::Span,
    },
    UnsupportedComponentProfile {
        span: m::Span,
    },
    UnsupportedTaskProfile {
        span: m::Span,
    },
    Budget(m::Error),
    Types(o::Error),
    Moves(o::moves::Error),
    Activity(o::loan_activity::Error),
    Loans(o::loans::Error),
    Failure(o::failure::Error),
}
#[derive(Debug)]
pub struct CodecError {
    pub offset: usize,
    pub kind: CodecErrorKind,
}
impl Display for CodecError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ownership artifact error at byte {}: {:?}",
            self.offset, self.kind
        )
    }
}
impl std::error::Error for CodecError {}
fn error(offset: usize, kind: CodecErrorKind) -> CodecError {
    CodecError { offset, kind }
}

/// All independent semantic proofs are complete. Import/allocator bindings and
/// runtime preparation are still explicit inputs; this owns no guest values.
pub struct Program {
    flow: o::failure::Elaborated,
    entry: Option<m::FunctionId>,
}
impl Program {
    pub fn flow(&self) -> &o::failure::Elaborated {
        &self.flow
    }
    pub const fn entry(&self) -> Option<m::FunctionId> {
        self.entry
    }
    pub fn usage(&self) -> m::Usage {
        m::Usage {
            work: self.flow.work_used(),
            allocation_bytes: self.flow.requested_bytes(),
            elements: self.flow.elements_used(),
        }
    }
    /// Recover raw semantic input without cloning any tables or retaining proof
    /// flags. Re-encoding revalidates it under the caller's new limits.
    pub fn into_parts(self) -> (o::bodies::BodyTable, Option<m::FunctionId>) {
        (
            self.flow
                .into_source()
                .into_activity()
                .into_moved()
                .into_typed()
                .into_table(),
            self.entry,
        )
    }
}

fn verify(
    table: o::bodies::BodyTable,
    entry: Option<m::FunctionId>,
    mut budget: m::Budget,
    limits: CodecLimits,
    offset: usize,
) -> Result<Program, CodecError> {
    let cfg = limits.verification;
    let common = budget.limits();
    if entry.is_some() {
        budget
            .charge(1)
            .map_err(|e| error(offset, CodecErrorKind::Budget(e)))?;
    }
    let typed = table
        .verify_with_budget(
            &mut budget,
            cfg.bodies.places.ownership.constructed,
            cfg.bodies.places.places,
            cfg.bodies.loans,
            cfg.bodies.prepared,
        )
        .map_err(|e| error(offset, CodecErrorKind::Types(e)))?;
    if let Some(id) = entry {
        let declarations = typed.places().declarations();
        let Some(function) = declarations.functions().get(id.0 as usize) else {
            return Err(error(offset, CodecErrorKind::InvalidEntry));
        };
        if !function.parameters.is_empty() || function.result.ty != o::Type::I64 {
            return Err(error(offset, CodecErrorKind::InvalidEntry));
        }
    }
    macro_rules! capped {
        ($value:expr) => {{
            let mut v = $value;
            v.work = common.work;
            v.allocations = common.allocations;
            v.elements = common.elements;
            v
        }};
    }
    let moved = o::moves::analyze(typed, capped!(cfg.moves))
        .map_err(|e| error(offset, CodecErrorKind::Moves(e)))?;
    let activity = o::loan_activity::analyze(moved, capped!(cfg.activity))
        .map_err(|e| error(offset, CodecErrorKind::Activity(e)))?;
    let loans = o::loans::analyze(activity, capped!(cfg.loans))
        .map_err(|e| error(offset, CodecErrorKind::Loans(e)))?;
    let flow = o::failure::elaborate(loans, capped!(cfg.failure))
        .map_err(|e| error(offset, CodecErrorKind::Failure(e)))?;
    Ok(Program { flow, entry })
}

pub fn decode(bytes: &[u8], limits: CodecLimits) -> Result<Program, CodecError> {
    reader::decode(bytes, limits)
}

/// Consume and independently verify raw tables before publishing schema-2/3/4
/// bytes. This avoids unchecked/infallible table clones during revalidation.
pub fn encode(
    table: o::bodies::BodyTable,
    entry: Option<m::FunctionId>,
    limits: CodecLimits,
) -> Result<Vec<u8>, CodecError> {
    if limits.bytes < HEADER {
        return Err(error(
            0,
            CodecErrorKind::TooLarge {
                limit: limits.bytes,
                requested: HEADER,
            },
        ));
    }
    let program = verify(
        table,
        entry,
        m::Budget::new(limits.budget_limits()),
        limits,
        0,
    )?;
    writer::encode(program, limits)
}
