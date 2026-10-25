use lang_runtime::{allocation as a, execution as e, layout as l, task::runner as r};
use std::path::Path;
fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("task-controls.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analyzed.diagnostics.is_empty(),
        "{:?}",
        analyzed.diagnostics
    );
    let lang_semantics::analysis::Program::Tasks(program) = analyzed.result.unwrap() else {
        panic!("task profile")
    };
    program
}
fn allocator() -> a::Allocator<a::System> {
    a::Allocator::new(a::System, Default::default()).unwrap()
}
#[allow(
    clippy::result_large_err,
    reason = "Compare complete control failure evidence."
)]
fn execute(
    program: &lang_semantics::analysis::TaskProgram,
    native: &lang_codegen_clif::tasks::CompiledProgram<'_>,
    compiled: bool,
    control: e::Control<'_>,
    limits: r::Limits,
) -> Result<r::Outcome, r::Failure> {
    let mut allocator = allocator();
    let result = if compiled {
        native.execute(&mut allocator, control, limits)
    } else {
        lang_vm::unit::tasks::execute_entry(program.verified(), &mut allocator, control, limits)
    };
    assert_eq!(allocator.stats().live_allocations, 0);
    result
}
fn work_failure(reason: &r::Reason) -> Option<(u64, u64, u64)> {
    match *reason {
        r::Reason::Control(e::Failure::ServiceWorkLimit {
            limit,
            used,
            requested,
        })
        | r::Reason::Execution(
            lang_mir::unit::ownership::failure::ExecutionFailure::ServiceWorkLimit {
                limit,
                used,
                requested,
            },
        ) => Some((limit, used, requested)),
        _ => None,
    }
}
#[test]
fn task_service_costs_match_synchronous_execution_without_marshalling_events() {
    let text = include_str!("fixtures/task_owned_buffers.lang");
    let program = prepare(text);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let synchronous = text
        .replace("async fn", "fn")
        .replace("yield;", "")
        .replace("cancel_check;", "")
        .replace("await worker()", "worker()")
        .replace(
            "core::result_unwrap::<i64, core::TaskError>(worker())",
            "worker()",
        );
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("sync-controls.lang"),
        synchronous.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analyzed.diagnostics.is_empty(),
        "{:?}",
        analyzed.diagnostics
    );
    let lang_semantics::analysis::Program::Ownership(sync) = analyzed.result.unwrap() else {
        panic!("owned profile")
    };
    let mut allocator = allocator();
    let mut instance = lang_runtime::cleanup::Instance::default();
    let baseline = lang_vm::unit::owned::execute_entry(
        sync.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        e::Control::with_limits(Default::default()),
    )
    .unwrap();
    for compiled in [false, true] {
        let actual = execute(
            &program,
            &native,
            compiled,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
        assert_eq!(actual.value, 71);
        assert_eq!(actual.usage.service_work, baseline.usage.service_work);
        assert!(actual.usage.service_work > 0);
    }
}
#[test]
fn exact_and_one_below_service_limits_cover_all_owned_families_and_task_actors() {
    for source in [
        include_str!("fixtures/task_owned_buffers.lang"),
        include_str!("fixtures/task_owned_pipeline.lang"),
        include_str!("fixtures/task_owned_arena.lang"),
        include_str!("fixtures/task_owned_integrated.lang"),
    ] {
        let program = prepare(source);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let baseline = execute(
            &program,
            &native,
            false,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
        let exact = baseline.usage.service_work;
        assert!(exact > 0);
        for compiled in [false, true] {
            let full = execute(
                &program,
                &native,
                compiled,
                e::Control::with_limits(Default::default()).and_service_work(Some(exact)),
                Default::default(),
            )
            .unwrap();
            assert_eq!(full.value, baseline.value);
            assert_eq!(full.usage, baseline.usage);
            assert!(!full.poisoned);
            let failure = execute(
                &program,
                &native,
                compiled,
                e::Control::with_limits(Default::default()).and_service_work(Some(exact - 1)),
                Default::default(),
            )
            .err()
            .unwrap();
            let (limit, used, requested) =
                work_failure(&failure.reason).unwrap_or_else(|| panic!("{failure:?}"));
            assert_eq!(limit, exact - 1);
            assert_eq!(used, failure.usage.service_work);
            assert!(used <= limit && used + requested > limit);
            assert!(!failure.poisoned);
            assert!(failure.span.is_some());
            assert!(failure.function.is_some());
            assert_eq!(failure.tasks.live, 0);
        }
    }
}
#[test]
fn zero_service_work_refuses_the_event_and_still_retires_all_tasks() {
    let program = prepare(include_str!("fixtures/task_owned_buffers.lang"));
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let mut allocator = allocator();
        let control = e::Control::with_limits(Default::default()).and_service_work(Some(0));
        let failure = if compiled {
            native.execute(&mut allocator, control, Default::default())
        } else {
            lang_vm::unit::tasks::execute_entry(
                program.verified(),
                &mut allocator,
                control,
                Default::default(),
            )
        }
        .err()
        .unwrap();
        assert!(
            matches!(
                failure.reason,
                r::Reason::Control(e::Failure::ServiceWorkLimit {
                    limit: 0,
                    used: 0,
                    ..
                }) | r::Reason::Execution(
                    lang_mir::unit::ownership::failure::ExecutionFailure::ServiceWorkLimit {
                        limit: 0,
                        used: 0,
                        ..
                    }
                )
            ),
            "{failure:?}"
        );
        assert!(!failure.poisoned);
        assert_eq!(failure.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn finite_work_rejects_unused_foreign_metadata_before_any_guest_allocation() {
    for import in [
        "extern fn host(value:i64)->i64;",
        "extern \"C\" fn host(value:i64)->i64;",
    ] {
        let program = prepare(&format!("{import}async fn main()->i64{{yield;return 42;}}"));
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        for compiled in [false, true] {
            let mut allocator = allocator();
            let control = e::Control::with_limits(Default::default()).and_service_work(Some(100));
            let failure = if compiled {
                native.execute(&mut allocator, control, Default::default())
            } else {
                lang_vm::unit::tasks::execute_entry(
                    program.verified(),
                    &mut allocator,
                    control,
                    Default::default(),
                )
            }
            .err()
            .unwrap();
            assert!(
                matches!(failure.reason, r::Reason::Unsupported),
                "{failure:?}"
            );
            assert_eq!(failure.usage, e::Usage::default());
            assert_eq!(failure.tasks.admitted, 0);
            assert_eq!(allocator.stats().requests, 0);
            assert!(!failure.poisoned);
        }
    }
}
#[test]
fn retained_child_result_stays_in_the_cell_limit_until_it_is_retired() {
    use lang_mir::unit::FunctionId;
    let program = prepare(
        "async fn worker()->i64{return 21;}async fn main()->i64{let mut first:core::Task<i64>=spawn worker();yield;yield;yield;let mut second:core::Task<i64>=spawn worker();yield;yield;yield;return 42;}",
    );
    let proof = program.verified();
    let layouts = l::Layouts::compile_tasks(proof, Default::default()).unwrap();
    let functions = proof.typed().places().declarations().functions();
    let cells = |name: &str| {
        l::FramePlan::compile_tasks(
            &layouts,
            proof,
            FunctionId(functions.iter().position(|f| f.name == name).unwrap() as u32),
            Default::default(),
            usize::MAX,
        )
        .unwrap()
        .cell_count()
    };
    let before_result = cells("main") + cells("worker");
    let exact = before_result + 1;
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(proof, Default::default()).unwrap();
    for limit in [before_result, exact] {
        for compiled in [false, true] {
            let mut allocator = allocator();
            let control = e::Control::with_limits(e::Limits {
                live_cells: Some(limit),
                ..Default::default()
            });
            let result = if compiled {
                native.execute(&mut allocator, control, Default::default())
            } else {
                lang_vm::unit::tasks::execute_entry(
                    proof,
                    &mut allocator,
                    control,
                    Default::default(),
                )
            };
            if limit == exact {
                let result = result.unwrap();
                assert_eq!(result.value, 42);
                assert_eq!(result.usage.peak_live_cells, exact);
            } else {
                let failure = result.err().unwrap();
                assert!(
                    matches!(failure.reason, r::Reason::CellLimit { .. }),
                    "{failure:?}"
                );
                assert_eq!(failure.tasks.admitted, 2);
                assert_eq!(failure.tasks.live, 0);
            }
            assert_eq!(allocator.stats().live_allocations, 0);
        }
    }
}

#[test]
fn finite_service_work_composes_with_task_frame_cell_trace_and_poll_controls() {
    let program = prepare(include_str!("fixtures/task_owned_integrated.lang"));
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let baseline = execute(
        &program,
        &native,
        false,
        e::Control::with_limits(Default::default()),
        r::Limits {
            trace_events: 1024,
            ..Default::default()
        },
    )
    .unwrap();
    let work = baseline.usage.service_work;
    let flag = e::InterruptionFlag::new();
    flag.interrupt();
    let controls = [
        e::Control::with_limits(e::Limits {
            fuel: Some(baseline.usage.fuel / 2),
            ..Default::default()
        }),
        e::Control::with_limits(e::Limits {
            frames: Some(2),
            ..Default::default()
        }),
        e::Control::with_limits(e::Limits {
            live_cells: Some(1),
            ..Default::default()
        }),
        e::Control::with_limits(Default::default()),
        e::Control::with_limits(Default::default()).and_interruption(&flag),
        e::Control::with_limits(Default::default()).and_deadline(std::time::Instant::now()),
    ];
    for (index, control) in controls.into_iter().enumerate() {
        let limits = if index == 3 {
            r::Limits {
                trace_events: 3,
                ..Default::default()
            }
        } else {
            Default::default()
        };
        let mut expected = None;
        for compiled in [false, true] {
            let failure = execute(
                &program,
                &native,
                compiled,
                control.and_service_work(Some(work)),
                limits,
            )
            .err()
            .unwrap();
            assert!(work_failure(&failure.reason).is_none());
            assert!(!failure.poisoned);
            assert_eq!(failure.tasks.live, 0);
            let evidence = format!("{failure:?}");
            if let Some(previous) = &expected {
                assert_eq!(previous, &evidence);
            } else {
                expected = Some(evidence);
            }
        }
    }
    for tasks in [
        lang_runtime::task::Limits {
            live_tasks: 1,
            total_tasks: 20,
        },
        lang_runtime::task::Limits {
            live_tasks: 20,
            total_tasks: 2,
        },
    ] {
        for compiled in [false, true] {
            let failure = execute(
                &program,
                &native,
                compiled,
                e::Control::with_limits(Default::default()).and_service_work(Some(work)),
                r::Limits {
                    tasks,
                    ..Default::default()
                },
            )
            .err()
            .unwrap();
            assert!(matches!(failure.reason, r::Reason::Schedule(_)));
            assert!(!failure.poisoned);
            assert_eq!(failure.tasks.live, 0);
        }
    }
}
