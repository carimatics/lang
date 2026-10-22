use lang_runtime::{allocation as a, execution as e, task::runner as r};
use lang_semantics::analysis::{Program, TaskProgram};
use std::path::Path;

// Compile the same lifecycle, quota, failure and ownership assertions against
// the native adapter, including inline source variants beyond the catalog.
#[path = "source_services.rs"]
mod source_cases;
use channel_fixtures::channel_allocator;
use source_cases::{channel_fixtures, service_fixtures};

mod service_backend {
    pub(super) struct Prepared<'p>(lang_codegen_clif::tasks::CompiledProgram<'p>);
    pub(super) fn prepare(proof: &lang_mir::unit::ownership::tasks::VerifiedTasks) -> Prepared<'_> {
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(proof, Default::default())
            .expect("the source conformance proof must compile to native continuations");
        Prepared(native)
    }
    impl Prepared<'_> {
        #[expect(
            clippy::result_large_err,
            reason = "Preserve complete task failure and cleanup evidence."
        )]
        pub(super) fn execute<P: lang_runtime::allocation::Provider>(
            &self,
            allocator: &mut lang_runtime::allocation::Allocator<P>,
            control: lang_runtime::execution::Control<'_>,
            limits: lang_runtime::task::runner::Limits,
        ) -> Result<lang_runtime::task::runner::Outcome, lang_runtime::task::runner::Failure>
        {
            self.0.execute(allocator, control, limits)
        }
    }
}

fn prepare(text: &str) -> Box<TaskProgram> {
    let output = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("native-services.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let Program::Tasks(program) = output.result.unwrap() else {
        panic!("service task profile")
    };
    program
}

#[test]
fn generated_service_continuations_match_vm_fifo_state_and_disposal() {
    for (name, source) in service_fixtures::fixtures() {
        let program = prepare(source);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let mut expected = None;
        for compiled in [false, true] {
            let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
            let control = e::Control::with_limits(Default::default());
            let limits = r::Limits {
                trace_events: 4096,
                ..Default::default()
            };
            let outcome = if compiled {
                native.execute(&mut allocator, control, limits)
            } else {
                lang_vm::unit::tasks::execute_entry(
                    program.verified(),
                    &mut allocator,
                    control,
                    limits,
                )
            }
            .unwrap();
            assert_eq!(outcome.value, 42, "{name} native={compiled}");
            assert_eq!(outcome.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            if let Some((usage, tasks, events)) = &expected {
                assert_eq!(&outcome.usage, usage);
                assert_eq!(&outcome.tasks, tasks);
                assert_eq!(&outcome.events, events);
            } else {
                expected = Some((outcome.usage, outcome.tasks, outcome.events));
            }
        }
    }
}

#[expect(
    clippy::result_large_err,
    reason = "Compare complete native service failure and cleanup evidence."
)]
fn execute<P: a::Provider>(
    program: &TaskProgram,
    native: &lang_codegen_clif::tasks::CompiledProgram<'_>,
    compiled: bool,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
    limits: r::Limits,
) -> Result<r::Outcome, r::Failure> {
    if compiled {
        native.execute(allocator, control, limits)
    } else {
        lang_vm::unit::tasks::execute_entry(program.verified(), allocator, control, limits)
    }
}
fn compare(
    left: &Result<r::Outcome, r::Failure>,
    right: &Result<r::Outcome, r::Failure>,
    name: &str,
) {
    match (left, right) {
        (Ok(left), Ok(right)) => {
            assert_eq!(left.value, right.value, "{name}");
            assert_eq!(left.usage, right.usage, "{name}");
            assert_eq!(left.tasks, right.tasks, "{name}");
            assert_eq!(left.allocations, right.allocations, "{name}");
            assert_eq!(left.events, right.events, "{name}");
            assert_eq!(left.poisoned, right.poisoned, "{name}");
            assert_eq!(
                format!("{:?}", left.report),
                format!("{:?}", right.report),
                "{name}"
            );
        }
        (Err(left), Err(right)) => {
            assert_eq!(
                format!("{:?}", left.reason),
                format!("{:?}", right.reason),
                "{name}"
            );
            assert_eq!(left.function, right.function, "{name}");
            assert_eq!(left.span, right.span, "{name}");
            assert_eq!(left.usage, right.usage, "{name}");
            assert_eq!(left.tasks, right.tasks, "{name}");
            assert_eq!(left.events, right.events, "{name}");
            assert_eq!(left.poisoned, right.poisoned, "{name}");
            assert_eq!(
                format!("{:?}", left.report),
                format!("{:?}", right.report),
                "{name}"
            );
        }
        _ => panic!("{name}: VM/native success/failure disagrees"),
    }
}
fn fixtures() -> impl Iterator<Item = (&'static str, String)> {
    service_fixtures::fixtures()
        .filter(|(name, _)| *name != "deep-state")
        .map(|(name, source)| (name, source.to_owned()))
        .chain(std::iter::once(service_fixtures::recursive_prefix_fixture()))
}

#[test]
fn native_service_control_prefixes_match_complete_vm_failure_evidence() {
    for (name, text, exhaustive) in
        fixtures()
            .map(|(name, source)| (name, source, true))
            .chain(std::iter::once((
                "deep-state",
                include_str!("fixtures/task_service_state_deep.lang").to_owned(),
                false,
            )))
    {
        let program = prepare(&text);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let (mut allocator, trace) = channel_allocator(usize::MAX);
        let baseline = execute(
            &program,
            &native,
            false,
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 4096,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(baseline.value, 42, "{name}");
        assert!(trace.borrow().live.is_empty());
        for (kind, total) in [
            (0, baseline.usage.fuel),
            (1, baseline.usage.service_work),
            (2, baseline.usage.peak_live_cells as u64),
            (3, baseline.events.len() as u64),
        ] {
            let first = u64::from(kind == 3);
            let caps: Vec<_> = if exhaustive {
                (first..=total).collect()
            } else {
                let mut caps = vec![first, first.max(1), total.saturating_sub(1), total];
                caps.retain(|cap| *cap >= first && *cap <= total);
                caps.sort_unstable();
                caps.dedup();
                caps
            };
            for cap in caps {
                let label = format!("{name} control {kind} cap {cap}/{total}");
                let mut expected = None;
                for compiled in [false, true] {
                    let (mut allocator, trace) = channel_allocator(usize::MAX);
                    let control = e::Control::with_limits(e::Limits {
                        fuel: (kind == 0).then_some(cap),
                        live_cells: (kind == 2).then_some(cap as usize),
                        ..Default::default()
                    })
                    .and_service_work((kind == 1).then_some(cap));
                    let result = execute(
                        &program,
                        &native,
                        compiled,
                        &mut allocator,
                        control,
                        r::Limits {
                            trace_events: if kind == 3 { cap as usize } else { 0 },
                            ..Default::default()
                        },
                    );
                    assert_eq!(
                        allocator.stats().live_allocations,
                        0,
                        "{label} native={compiled}"
                    );
                    assert!(trace.borrow().live.is_empty(), "{label} native={compiled}");
                    if cap < total {
                        assert!(result.is_err(), "{label}");
                    } else {
                        assert_eq!(result.as_ref().unwrap().value, 42, "{label}");
                    }
                    if let Some(expected) = &expected {
                        compare(expected, &result, &label);
                    } else {
                        expected = Some(result);
                    }
                }
            }
        }
    }
}

#[test]
fn native_service_provider_and_interruption_positions_match_vm_inventory_and_failure() {
    for (name, text) in fixtures() {
        let program = prepare(&text);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let (mut allocator, trace) = channel_allocator(usize::MAX);
        let baseline = execute(
            &program,
            &native,
            false,
            &mut allocator,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
        let calls = trace.borrow().calls;
        for at in 1..=calls {
            for interrupt in [false, true] {
                let label = format!("{name} allocation {at}/{calls} interrupt={interrupt}");
                let mut expected = None;
                let mut expected_buffers = None;
                for compiled in [false, true] {
                    let (mut allocator, trace) =
                        channel_allocator(if interrupt { usize::MAX } else { at });
                    let flag = std::rc::Rc::new(e::InterruptionFlag::new());
                    if interrupt {
                        trace.borrow_mut().interrupt = Some((at, flag.clone()));
                    }
                    let result = execute(
                        &program,
                        &native,
                        compiled,
                        &mut allocator,
                        e::Control::with_limits(Default::default())
                            .and_interruption(&flag)
                            .and_service_work(Some(baseline.usage.service_work)),
                        Default::default(),
                    );
                    assert!(result.is_err(), "{label}");
                    assert_eq!(
                        allocator.stats().live_allocations,
                        0,
                        "{label} native={compiled}"
                    );
                    assert!(trace.borrow().live.is_empty(), "{label} native={compiled}");
                    let buffers = trace.borrow().buffers.clone();
                    if let Some(previous) = &expected_buffers {
                        assert_eq!(previous, &buffers, "{label}");
                    } else {
                        expected_buffers = Some(buffers);
                    }
                    if let Some(previous) = &expected {
                        compare(previous, &result, &label);
                    } else {
                        expected = Some(result);
                    }
                }
            }
        }
    }
}
