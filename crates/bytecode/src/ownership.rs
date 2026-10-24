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
    TaskScopes,
    TaskBindings,
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
    Profile,
    FunctionEffect,
    ChannelRole,
    ServiceRole,
}
/// Task-only contract failures. Common verification phases reuse their
/// existing error variants, preserving both spans without enlarging legacy
/// codec errors or allocating an error box after resource exhaustion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskErrorKind {
    Scope(u32),
    Binding(o::bodies::CleanupTarget),
    Stack,
    Claim,
    Escape,
    ReturnEscrow,
    UnsupportedTransfer(o::Type),
    Limit,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskError {
    pub kind: TaskErrorKind,
    pub primary: Option<m::Span>,
    pub secondary: Option<m::Span>,
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
    InvalidUsage,
    UnsupportedProfile(u8),
    Tasks(TaskError),
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

fn verify_tasks(
    table: o::bodies::BodyTable,
    entry: Option<m::FunctionId>,
    mut budget: m::Budget,
    limits: crate::tasks::CodecLimits,
    offset: usize,
) -> Result<crate::tasks::Program, CodecError> {
    if entry.is_some() {
        budget
            .charge(1)
            .map_err(|e| error(offset, CodecErrorKind::Budget(e)))?;
    }
    let usage = m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    };
    let proof = o::tasks::verify_with_usage(table, limits.verification, usage)
        .map_err(|e| error(offset, task_error(e)))?;
    let declarations = proof.typed().places().declarations();
    if let Some(span) = declarations.component_feature_span() {
        return Err(error(
            offset,
            CodecErrorKind::UnsupportedComponentProfile { span },
        ));
    }
    if let Some(id) = entry {
        let function = declarations
            .functions()
            .get(id.0 as usize)
            .ok_or_else(|| error(offset, CodecErrorKind::InvalidEntry))?;
        if !function.parameters.is_empty() || function.result.ty != o::Type::I64 {
            return Err(error(offset, CodecErrorKind::InvalidEntry));
        }
    }
    Ok(crate::tasks::Program { proof, entry })
}

pub(crate) fn decode_tasks(
    bytes: &[u8],
    limits: crate::tasks::CodecLimits,
) -> Result<crate::tasks::Program, CodecError> {
    let raw = reader::decode_tables(bytes, limits.shared(), Some(limits.verification))?;
    verify_tasks(raw.table, raw.entry, raw.budget, limits, raw.offset)
}

pub(crate) fn encode_tasks(
    table: o::bodies::BodyTable,
    entry: Option<m::FunctionId>,
    limits: crate::tasks::CodecLimits,
) -> Result<Vec<u8>, CodecError> {
    let shared = limits.shared();
    if limits.bytes < HEADER + 1 {
        return Err(error(
            0,
            CodecErrorKind::TooLarge {
                limit: limits.bytes,
                requested: HEADER + 1,
            },
        ));
    }
    let program = verify_tasks(
        table,
        entry,
        m::Budget::new(shared.budget_limits()),
        limits,
        0,
    )?;
    let entry = program.entry;
    writer::encode_tasks(
        Program {
            flow: program.proof.into_plan(),
            entry,
        },
        shared,
    )
}

fn task_error(e: o::tasks::Error) -> CodecErrorKind {
    use o::tasks::ErrorKind as K;
    let kind = match e.kind {
        K::Resource(kind) => return CodecErrorKind::Budget(m::Error::new(kind)),
        K::Types(kind) => {
            return CodecErrorKind::Types(o::Error {
                kind,
                span: e.primary,
            });
        }
        K::Moves(kind) => {
            return CodecErrorKind::Moves(o::moves::Error {
                kind,
                primary: e.primary,
                secondary: e.secondary,
            });
        }
        K::Activity(kind) => {
            return CodecErrorKind::Activity(o::loan_activity::Error {
                kind,
                primary: e.primary,
                secondary: e.secondary,
            });
        }
        K::Loans(kind) => {
            return CodecErrorKind::Loans(o::loans::Error {
                kind,
                primary: e.primary,
                secondary: e.secondary,
            });
        }
        K::Failure(kind) => {
            return CodecErrorKind::Failure(o::failure::Error {
                kind,
                primary: e.primary,
            });
        }
        K::Scope(id) => TaskErrorKind::Scope(id),
        K::Binding(target) => TaskErrorKind::Binding(target),
        K::Stack => TaskErrorKind::Stack,
        K::Claim => TaskErrorKind::Claim,
        K::Escape => TaskErrorKind::Escape,
        K::ReturnEscrow => TaskErrorKind::ReturnEscrow,
        K::UnsupportedTransfer(ty) => TaskErrorKind::UnsupportedTransfer(ty),
        K::Limit => TaskErrorKind::Limit,
    };
    CodecErrorKind::Tasks(TaskError {
        kind,
        primary: e.primary,
        secondary: e.secondary,
    })
}

pub(crate) fn decode_component(
    bytes: &[u8],
    limits: crate::tasks::CodecLimits,
) -> Result<crate::component::Unverified<'_>, CodecError> {
    let raw = reader::decode_component(bytes, limits.shared(), limits.verification)?;
    Ok(crate::component::Unverified {
        table: raw.table,
        entry: raw.entry,
        metadata: &bytes[raw.offset..],
        metadata_offset: raw.offset,
        usage: m::Usage {
            work: raw.budget.work_used(),
            allocation_bytes: raw.budget.allocation_bytes(),
            elements: raw.budget.elements_used(),
        },
    })
}
pub(crate) fn encode_component(
    proof: o::tasks::VerifiedTasks,
    entry: Option<m::FunctionId>,
    metadata: &[u8],
    limits: crate::tasks::CodecLimits,
    prior: m::Usage,
) -> Result<Vec<u8>, CodecError> {
    let p = proof.plan();
    if prior.work < p.work_used()
        || prior.allocation_bytes < p.requested_bytes()
        || prior.elements < p.elements_used()
    {
        return Err(error(0, CodecErrorKind::InvalidUsage));
    }
    writer::encode_component(
        Program {
            flow: proof.into_plan(),
            entry,
        },
        limits.shared(),
        metadata,
        prior,
    )
}
