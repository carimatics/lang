//! Ownership results never escape the fresh execution instance. Reports keep
//! independent immutable source snapshots, mapped cleanup order and precedence.
use super::*;
use lang_runtime::cleanup as c;
use lang_semantics::analysis::OwnershipProgram;
use lang_vm::unit::owned as engine;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CleanupFailure {
    pub kind: c::FailureKind,
    pub span: Option<Span>,
    pub slot: Option<c::Slot>,
}
#[derive(Debug)]
pub struct CleanupReport {
    pub primary: Option<CleanupFailure>,
    pub secondary: Vec<CleanupFailure>,
    pub truncated: usize,
    pub quarantined: usize,
}
impl CleanupReport {
    fn mapped(report: &c::Report, map: impl Fn(m::Span) -> Span) -> Self {
        let record = |r: &c::FailureRecord| CleanupFailure {
            kind: r.kind,
            span: r.span.map(&map),
            slot: r.slot,
        };
        let mut secondary = Vec::new();
        let mut truncated = report.truncated();
        if secondary
            .try_reserve_exact(report.secondary().len())
            .is_ok()
        {
            secondary.extend(report.secondary().iter().map(record));
        } else {
            // Diagnostic transport must never replace the original primary.
            truncated = truncated.saturating_add(report.secondary().len());
        }
        Self {
            primary: report.primary().map(record),
            secondary,
            truncated,
            quarantined: report.quarantined(),
        }
    }
}
#[derive(Debug)]
pub struct OwnedRuntimeFailure {
    pub reason: engine::Reason,
    pub usage: vm::Usage,
    pub fault: Option<Span>,
    pub stack: Vec<Frame>,
    pub trace_complete: bool,
    pub report: Option<CleanupReport>,
    pub panic_code: Option<i64>,
    pub poisoned: bool,
    program: Box<OwnershipProgram>,
    wrapper: m::FunctionId,
}
impl OwnedRuntimeFailure {
    pub fn function_name(&self, frame: Frame) -> &str {
        if frame.function == self.wrapper {
            "<expression>"
        } else {
            &self
                .program
                .mir()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations()
                .functions()[frame.function.0 as usize]
                .name
        }
    }
}
impl Display for OwnedRuntimeFailure {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.reason)?;
        if let Some(code) = self.panic_code {
            write!(f, " (panic code {code})")?;
        }
        write!(
            f,
            " (fuel {}, host calls {}) (service work {})",
            self.usage.fuel, self.usage.host_calls, self.usage.service_work
        )
    }
}
pub(super) struct Evaluation {
    pub program: Box<OwnershipProgram>,
    pub sources: SourceMap,
    pub diagnostics: Vec<Diagnostic>,
    pub origins: Origins,
    pub wrapper: String,
    pub display_bytes: usize,
}
impl Evaluation {
    pub fn execute<P: a::Provider>(
        self,
        allocator: Option<&mut a::Allocator<P>>,
        control: Control<'_>,
    ) -> Report {
        let Self {
            program,
            sources,
            diagnostics,
            origins,
            wrapper,
            display_bytes,
        } = self;
        let declarations = program
            .mir()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations();
        let function = m::FunctionId(
            declarations
                .functions()
                .iter()
                .position(|f| f.name == wrapper)
                .expect("checked wrapper") as u32,
        );
        let mut instance = c::Instance::default();
        match engine::execute_preview(
            program.failure_flow(),
            function,
            &mut instance,
            allocator,
            control,
            display_bytes,
        ) {
            Ok(outcome) => Report {
                sources,
                diagnostics,
                result: Ok(Event::Value(outcome.value)),
            },
            Err(error) => {
                let mapped =
                    |span| origins.mir_span(span, &declarations.types().table().sources, &sources);
                let frames = error.stack.iter().copied().chain(
                    error
                        .function
                        .filter(|_| error.stack.is_empty())
                        .map(|function| vm::StackFrame {
                            function,
                            span: error.fault,
                        }),
                );
                let count = error.stack.len().max(usize::from(error.function.is_some()));
                let mut stack = Vec::new();
                let trace_complete = stack.try_reserve_exact(count).is_ok();
                if trace_complete {
                    stack.extend(frames.map(|f| Frame {
                        function: f.function,
                        span: f.span.map(mapped),
                    }));
                }
                let report = error
                    .report
                    .as_ref()
                    .map(|r| CleanupReport::mapped(r, mapped));
                let fault = error.fault.map(mapped);
                let failure = OwnedRuntimeFailure {
                    reason: error.reason,
                    usage: error.usage,
                    fault,
                    stack,
                    trace_complete,
                    report,
                    panic_code: error.panic_code,
                    poisoned: instance.is_poisoned(),
                    program,
                    wrapper: function,
                };
                Report::failure(
                    sources,
                    diagnostics,
                    ReplError::OwnedRuntime(Box::new(failure)),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_transport_preserves_primary_cleanup_order_truncation_and_quarantine() {
        let span = |start| m::Span {
            source: m::SourceId(0),
            start,
            end: start + 1,
        };
        let mut instance = c::Instance::default();
        let mut frame = instance
            .enter(
                c::Limits {
                    slots: 3,
                    failure_records: 2,
                    ..Default::default()
                },
                |value: &mut u32, _| {
                    assert_ne!(*value, 2, "contained cleanup panic");
                    Err(1000 + *value)
                },
            )
            .unwrap();
        for value in 1..=3 {
            let slot = c::Slot(value as usize - 1);
            frame.declare(slot, c::Kind::Owner).unwrap();
            frame.initialize(slot, value, Some(span(value))).unwrap();
        }
        let completion = frame.finish(c::Exit::Failure {
            failure: lang_mir::unit::ownership::failure::ExecutionFailure::Panic,
            span: Some(span(0)),
        });
        let mut sources = SourceMap::new();
        let source = sources.add("repl/7.lang", "abcdef").unwrap();
        let report = CleanupReport::mapped(&completion.report, |s| {
            sources
                .span(source, s.start as usize..s.end as usize)
                .unwrap()
        });
        assert_eq!(
            report.primary.unwrap().kind,
            c::FailureKind::Execution(lang_mir::unit::ownership::failure::ExecutionFailure::Panic)
        );
        assert_eq!(
            sources
                .slice(report.primary.unwrap().span.unwrap())
                .unwrap(),
            "a"
        );
        assert_eq!(report.secondary.len(), 1);
        assert_eq!(report.secondary[0].kind, c::FailureKind::Cleanup(1003));
        assert_eq!(
            sources.slice(report.secondary[0].span.unwrap()).unwrap(),
            "d"
        );
        assert_eq!(report.secondary[0].slot, Some(c::Slot(2)));
        assert_eq!(report.truncated, 2);
        assert_eq!(report.quarantined, 1);
        assert!(instance.is_poisoned());
    }
}
