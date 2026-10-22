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
    Tasks(Box<TaskProgram>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskProgram {
    hir: lang_hir::tasks::TaskLoans,
    mir: o::tasks::VerifiedTasks,
    feature_span: Span,
}
impl TaskProgram {
    pub fn hir(&self) -> &lang_hir::tasks::TaskLoans {
        &self.hir
    }
    pub fn verified(&self) -> &o::tasks::VerifiedTasks {
        &self.mir
    }
    pub fn into_verified(self) -> o::tasks::VerifiedTasks {
        self.mir
    }
    pub const fn feature_span(&self) -> Span {
        self.feature_span
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnershipProgram {
    hir: Box<h::OwnershipProgram>,
    flow: o::failure::Elaborated,
}
impl OwnershipProgram {
    /// Consume frontend-only data while preserving the shared, independently
    /// verified ownership contract and its original source snapshots.
    pub fn into_failure_flow(self) -> o::failure::Elaborated {
        self.flow
    }
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
            Self::Tasks(p) => p.hir.loaned().moved().bodies().source(),
        }
    }
    pub fn functions(&self) -> &[h::FunctionFacts] {
        match self {
            Self::Copy(p) => p.functions(),
            Self::Ownership(p) => p.hir().loaned().moved().bodies().functions(),
            Self::Tasks(p) => p.hir.loaned().moved().bodies().functions(),
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
    /// Use the exact resolved program that ownership lowering consumed.
    /// MIR source ordinals follow first distinct module snapshots, never paths
    /// or the current contents of an editor buffer with the same file name.
    pub fn region_diagnostic(
        &self,
        program: &r::Program,
        sources: &SourceMap,
    ) -> Option<Diagnostic> {
        let Self::Failure(error) = self else {
            return None;
        };
        let o::failure::ErrorKind::Regions(region) = &error.kind else {
            return None;
        };
        use o::regions::local::ErrorKind as K;
        let message = match region.kind {
            K::CrossArena { .. } => "arena value belongs to a different region",
            K::ExpiredArena(_) => "arena value is used after its region ended",
            K::Escape { .. } | K::CallStore { .. } => "arena value would outlive its region",
            K::ReturnEscape { .. } => "arena values cannot escape as function results",
            K::SharedStorage { .. } => "arena values cannot be stored in shared storage",
            K::LocalReferenceEscape => "callee references cannot escape into caller storage",
            K::ArenaState(_) => "arena lifetime is not valid on every path",
            _ => return None,
        };
        let primary = original_span(program, sources, region.primary?)?;
        let mut diagnostic = Diagnostic::new(
            codes::REGION_ESCAPE,
            Severity::Error,
            message,
            Label::new(primary, message),
        );
        if let Some(related) = region.secondary {
            let span = original_span(program, sources, related)?;
            diagnostic.secondary.push(Label::new(
                span,
                if matches!(region.kind, K::CallStore { .. }) {
                    "callee store requires this value to outlive the destination"
                } else {
                    "related arena or caller declaration"
                },
            ));
        }
        Some(diagnostic)
    }
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
            Self::Failure(e) => e.is_resource(),
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
fn original_span(program: &r::Program, sources: &SourceMap, span: m::Span) -> Option<Span> {
    let mut ordinal = 0usize;
    for (index, module) in program.modules.iter().enumerate() {
        let source = module.span.source();
        if program.modules[..index]
            .iter()
            .any(|m| m.span.source() == source)
        {
            continue;
        }
        if ordinal == span.source.0 as usize {
            return sources
                .span(source, span.start as usize..span.end as usize)
                .ok();
        }
        ordinal += 1;
    }
    None
}
pub fn analyze(
    source: r::Program,
    sources: &SourceMap,
    typing: h::CheckLimits,
    limits: OwnershipLimits,
) -> Result<Analysis, Error> {
    analyze_using(source, sources, typing, limits, h::analyze_program)
}
pub fn analyze_ownership(
    source: r::Program,
    sources: &SourceMap,
    typing: h::CheckLimits,
    limits: OwnershipLimits,
) -> Result<Analysis, Error> {
    analyze_using(
        source,
        sources,
        typing,
        limits,
        h::analyze_ownership_program,
    )
}
fn analyze_using(
    source: r::Program,
    sources: &SourceMap,
    typing: h::CheckLimits,
    limits: OwnershipLimits,
    check: impl FnOnce(
        r::Program,
        &SourceMap,
        h::CheckLimits,
    ) -> Result<h::SourceAnalysis, h::CheckError>,
) -> Result<Analysis, Error> {
    if let Some(span) =
        lang_hir::tasks::feature_span(&source, sources, typing).map_err(Error::Hir)?
    {
        return analyze_tasks(source, sources, typing, limits, span);
    }
    let root = source.modules.first().map(|m| m.span);
    let checked = check(source, sources, typing).map_err(Error::Hir)?;
    finish(checked, sources, limits, root)
}

fn analyze_tasks(
    source: r::Program,
    sources: &SourceMap,
    typing: h::CheckLimits,
    limits: OwnershipLimits,
    span: Span,
) -> Result<Analysis, Error> {
    let checked = lang_hir::tasks::analyze(source, sources, typing).map_err(Error::Hir)?;
    let mut diagnostics = checked.diagnostics;
    let Some(typed) = checked.program else {
        return Ok(Analysis {
            program: None,
            diagnostics,
        });
    };
    let checked = lang_hir::tasks::check_loans(typed, sources, typing.loans)
        .map_err(|e| Error::Hir(h::CheckError::Loans(e)))?;
    diagnostics.extend(checked.diagnostics);
    let Some(hir) = checked.program else {
        return Ok(Analysis {
            program: None,
            diagnostics,
        });
    };
    let mut task = o::tasks::Limits {
        bodies: limits.bodies,
        state_cells: limits.moves.state_cells.min(limits.activity.state_cells),
        loan_nodes: limits.loans.nodes,
        loan_words: limits.loans.state_words,
        failure_nodes: limits.failure.nodes,
        regions: limits.failure.regions,
        ..Default::default()
    };
    for (work, allocations, elements) in [
        (
            limits.moves.work,
            limits.moves.allocations,
            limits.moves.elements,
        ),
        (
            limits.activity.work,
            limits.activity.allocations,
            limits.activity.elements,
        ),
        (
            limits.loans.work,
            limits.loans.allocations,
            limits.loans.elements,
        ),
        (
            limits.failure.work,
            limits.failure.allocations,
            limits.failure.elements,
        ),
    ] {
        let global = &mut task.bodies.places.ownership.unit;
        global.work = global.work.min(work);
        global.allocations = global.allocations.min(allocations);
        global.elements = global.elements.min(elements);
    }
    let result = ownership_types::bodies::lower_tasks(&hir, sources, task).map_err(task_error);
    match result {
        Ok(mir) => Ok(Analysis {
            program: Some(Program::Tasks(Box::new(TaskProgram {
                hir,
                mir,
                feature_span: span,
            }))),
            diagnostics,
        }),
        Err(error) => {
            if let Some(diagnostic) = error.resource_diagnostic(span).or_else(|| {
                error.region_diagnostic(hir.loaned().moved().bodies().source(), sources)
            }) {
                diagnostics.push(diagnostic);
                Ok(Analysis {
                    program: None,
                    diagnostics,
                })
            } else {
                Err(error)
            }
        }
    }
}
fn task_error(error: ownership_types::Error) -> Error {
    let ownership_types::Error::Tasks(error) = error else {
        return Error::Lower(error);
    };
    let primary = error.primary;
    let secondary = error.secondary;
    match error.kind {
        o::tasks::ErrorKind::Resource(kind) => {
            Error::Lower(ownership_types::Error::Mir(m::Error::new(kind)))
        }
        o::tasks::ErrorKind::Types(kind) => Error::Lower(ownership_types::Error::Types(o::Error {
            kind,
            span: primary,
        })),
        o::tasks::ErrorKind::Moves(kind) => Error::Moves(o::moves::Error {
            kind,
            primary,
            secondary,
        }),
        o::tasks::ErrorKind::Activity(kind) => Error::Activity(o::loan_activity::Error {
            kind,
            primary,
            secondary,
        }),
        o::tasks::ErrorKind::Loans(kind) => Error::Loans(o::loans::Error {
            kind,
            primary,
            secondary,
        }),
        o::tasks::ErrorKind::Failure(kind) => Error::Failure(o::failure::Error { kind, primary }),
        o::tasks::ErrorKind::Limit => Error::Lower(ownership_types::Error::Capacity),
        kind => Error::Lower(ownership_types::Error::Tasks(o::tasks::Error {
            kind,
            primary,
            secondary,
        })),
    }
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
                    } else if let Some(d) = e
                        .region_diagnostic(hir.bodies().loaned().moved().bodies().source(), sources)
                    {
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
