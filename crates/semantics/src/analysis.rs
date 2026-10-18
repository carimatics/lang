//! Shared source-analysis profiles for the compiler driver and editor database.
use crate::ownership_types;
use lang_diagnostics::{Diagnostic, Label, Severity, SourceMap, Span, codes};
use lang_hir::{program as h, resolved as r};
use lang_mir::unit::{self as m, ownership as o};
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Default)]
pub struct OwnershipLimits {
    pub bodies: o::bodies::Limits,
    pub moves: o::moves::Limits,
    pub activity: o::loan_activity::Limits,
    pub loans: o::loans::Limits,
    pub failure: o::failure::Limits,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Program {
    Copy(h::TypedProgram),
    Ownership(Box<OwnershipProgram>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnershipProgram {
    hir: Box<h::OwnershipProgram>,
    flow: o::failure::Elaborated,
}
impl OwnershipProgram {
    pub fn hir(&self) -> &lang_hir::regions::RegionCheckedBodies {
        self.hir.bodies()
    }
    pub fn mir(&self) -> &o::loans::LoanCheckedBodies {
        self.flow.source()
    }
    pub fn failure_flow(&self) -> &o::failure::Elaborated {
        &self.flow
    }
    pub const fn feature_span(&self) -> Span {
        self.hir.feature_span()
    }
}
impl Program {
    pub fn source(&self) -> &r::Program {
        match self {
            Self::Copy(p) => p.source(),
            Self::Ownership(p) => p.hir().loaned().moved().bodies().source(),
        }
    }
    pub fn functions(&self) -> &[h::FunctionFacts] {
        match self {
            Self::Copy(p) => p.functions(),
            Self::Ownership(p) => p.hir().loaned().moved().bodies().functions(),
        }
    }
    pub fn as_copy(&self) -> Option<&h::TypedProgram> {
        if let Self::Copy(p) = self {
            Some(p)
        } else {
            None
        }
    }
}
#[derive(Debug)]
pub struct Analysis {
    pub program: Option<Program>,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Debug)]
pub enum Error {
    Hir(h::CheckError),
    Lower(ownership_types::Error),
    Moves(o::moves::Error),
    Activity(o::loan_activity::Error),
    Loans(o::loans::Error),
    Failure(o::failure::Error),
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Hir(e) => Display::fmt(e, f),
            Self::Lower(e) => Display::fmt(e, f),
            Self::Moves(e) => Display::fmt(e, f),
            Self::Activity(e) => Display::fmt(e, f),
            Self::Loans(e) => Display::fmt(e, f),
            Self::Failure(e) => Display::fmt(e, f),
        }
    }
}
impl std::error::Error for Error {}
fn resource(kind: &m::ErrorKind) -> bool {
    matches!(
        kind,
        m::ErrorKind::Limit { .. }
            | m::ErrorKind::AllocationFailed
            | m::ErrorKind::ArithmeticOverflow
    )
}
impl Error {
    /// MIR resource bounds describe this compilation, rather than a source
    /// semantic failure. Use the original root span, never remapped MIR IDs.
    pub fn resource_diagnostic(&self, root: Span) -> Option<Diagnostic> {
        let exhausted = match self {
            Self::Hir(_) => false,
            Self::Lower(ownership_types::Error::Capacity) => true,
            Self::Lower(ownership_types::Error::Mir(e)) => resource(&e.kind),
            Self::Lower(ownership_types::Error::Types(e)) => match &e.kind {
                o::ErrorKind::Mir(k) => resource(k),
                o::ErrorKind::ConstructedLimit { .. }
                | o::ErrorKind::PlaceLimit { .. }
                | o::ErrorKind::LoanLimit { .. }
                | o::ErrorKind::PreparedLimit { .. } => true,
                _ => false,
            },
            Self::Lower(_) => false,
            Self::Moves(e) => {
                matches!(&e.kind, o::moves::ErrorKind::StateCells { .. })
                    || matches!(&e.kind, o::moves::ErrorKind::Resource(k) if resource(k))
            }
            Self::Activity(e) => {
                matches!(&e.kind, o::loan_activity::ErrorKind::StateCells { .. })
                    || matches!(&e.kind, o::loan_activity::ErrorKind::Resource(k) if resource(k))
            }
            Self::Loans(e) => {
                matches!(
                    &e.kind,
                    o::loans::ErrorKind::Nodes { .. } | o::loans::ErrorKind::StateWords { .. }
                ) || matches!(&e.kind, o::loans::ErrorKind::Resource(k) if resource(k))
            }
            Self::Failure(e) => {
                matches!(&e.kind, o::failure::ErrorKind::Nodes { .. })
                    || matches!(&e.kind, o::failure::ErrorKind::Resource(k) if resource(k))
            }
        };
        exhausted.then(|| {
            Diagnostic::new(
                codes::RESOURCE_LIMIT,
                Severity::Error,
                "ownership MIR analysis resource bound exhausted",
                Label::new(
                    root,
                    "resource bound exhausted while analyzing this program",
                ),
            )
        })
    }
}
pub fn analyze(
    source: r::Program,
    sources: &SourceMap,
    typing: h::CheckLimits,
    limits: OwnershipLimits,
) -> Result<Analysis, Error> {
    let root = source.modules.first().map(|m| m.span);
    let checked = h::analyze_program(source, sources, typing).map_err(Error::Hir)?;
    finish(checked, sources, limits, root)
}

/// Infer only a validated host-generated expression wrapper, then apply the
/// same independent ownership MIR pipeline as annotated source functions.
pub fn analyze_expression_function(
    source: r::Program,
    sources: &SourceMap,
    typing: h::CheckLimits,
    limits: OwnershipLimits,
    wrapper: r::FunctionId,
) -> Result<Analysis, Error> {
    let root = source.modules.first().map(|m| m.span);
    let checked =
        h::analyze_expression_function(source, sources, typing, wrapper).map_err(Error::Hir)?;
    finish(checked, sources, limits, root)
}

fn finish(
    checked: h::SourceAnalysis,
    sources: &SourceMap,
    limits: OwnershipLimits,
    root: Option<Span>,
) -> Result<Analysis, Error> {
    let mut diagnostics = checked.diagnostics;
    let program = match checked.program {
        None => None,
        Some(h::CheckedProgram::Copy(p)) => Some(Program::Copy(p)),
        Some(h::CheckedProgram::Ownership(hir)) => {
            let result = (|| {
                let typed = ownership_types::bodies::lower(hir.bodies(), sources, limits.bodies)
                    .map_err(Error::Lower)?;
                let moved = o::moves::analyze(typed, limits.moves).map_err(Error::Moves)?;
                let activity =
                    o::loan_activity::analyze(moved, limits.activity).map_err(Error::Activity)?;
                let loaned = o::loans::analyze(activity, limits.loans).map_err(Error::Loans)?;
                o::failure::elaborate(loaned, limits.failure).map_err(Error::Failure)
            })();
            match result {
                Ok(flow) => Some(Program::Ownership(Box::new(OwnershipProgram { hir, flow }))),
                Err(e) => {
                    if let Some(d) = root.and_then(|root| e.resource_diagnostic(root)) {
                        diagnostics.push(d);
                        None
                    } else {
                        return Err(e);
                    }
                }
            }
        }
    };
    Ok(Analysis {
        program,
        diagnostics,
    })
}
