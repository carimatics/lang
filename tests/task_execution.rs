use lang_runtime::{allocation as a, execution as e, task as t};
use lang_vm::unit::tasks::{self as vm, EventKind};
use std::path::Path;
fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("tasks.lang"),
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
fn control() -> e::Control<'static> {
    e::Control::with_limits(Default::default())
}
const WORKER: &str = "async fn worker(n:i64)->i64{yield;cancel_check;return n;}";

#[test]
fn task_layout_is_explicit_and_cleanup_keeps_only_typed_return_storage() {
    use lang_runtime::layout::{self as l, Immediate};
    let proof = prepare(include_str!("../crates/syntax/tests/fixtures/tasks.lang"));
    let verified = proof.verified();
    let typed = verified.typed();
    let types = typed.places().declarations().types();
    assert!(matches!(
        l::Layouts::compile(types, Default::default()),
        Err(l::Error::UnsupportedTaskProfile { .. })
    ));
    let layouts = l::Layouts::compile_tasks(verified, Default::default()).unwrap();
    let claim = types
        .table()
        .constructed
        .iter()
        .position(|t| {
            matches!(
                t.constructor,
                lang_mir::unit::ownership::Constructor::Task(_)
            )
        })
        .unwrap();
    let claim = lang_mir::unit::ownership::Type::Constructed(
        lang_mir::unit::ownership::ConstructedId(claim as u32),
    );
    let layout = layouts.get(claim).unwrap();
    assert_eq!(layout.size, 0);
    assert_eq!(layout.work_slots, 0);
    assert!(matches!(layout.kind, l::Kind::TaskClaim));
    let main = lang_mir::unit::FunctionId(1);
    assert!(matches!(
        l::FramePlan::compile(&layouts, typed, main, Default::default(), 1024),
        Err(l::Error::UnsupportedTaskProfile { .. })
    ));
    let other = verified.clone();
    assert!(matches!(
        l::FramePlan::compile_tasks(&layouts, &other, main, Default::default(), 1024),
        Err(l::Error::ForeignLocation)
    ));
    let plan =
        l::FramePlan::compile_tasks(&layouts, verified, main, Default::default(), 1024).unwrap();
    let mut allocator = allocator();
    let mut frame = plan.reserve(&mut allocator).unwrap();
    let at = frame.cell(plan.return_cell()).unwrap();
    at.write_immediate(at.root(), Immediate::I64(42), &mut allocator)
        .unwrap();
    let mut result = frame
        .release_except(plan.return_cell(), &mut allocator)
        .unwrap();
    assert_eq!(
        result.read_immediate(result.root(), &allocator).unwrap(),
        Immediate::I64(42)
    );
    assert_eq!(allocator.stats().live_bytes, 8);
    result.release_storage(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn unsupported_execution_and_artifact_modes_refuse_before_effects() {
    let source = "extern fn host(value:i64)->i64; async fn main()->i64{yield;return host(42);}";
    let proof = prepare(source);
    let mut allocator = allocator();
    let error = vm::execute_entry(
        proof.verified(),
        &mut allocator,
        control(),
        Default::default(),
    )
    .err()
    .unwrap();
    assert!(matches!(error.reason, vm::Reason::Unsupported));
    assert_eq!(allocator.stats().requests, 0);
    let proof = prepare(include_str!("../crates/syntax/tests/fixtures/tasks.lang"));
    let output = vm::execute_entry(
        proof.verified(),
        &mut allocator,
        control().and_service_work(Some(100)),
        Default::default(),
    )
    .unwrap();
    assert_eq!(output.value, 42);
    assert_eq!(output.usage.service_work, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    let source = "async fn main()->i64{yield;return 42;}";
    let analysis = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("artifact-task.lang"),
        source.as_bytes(),
        None,
        Default::default(),
        None,
    );
    let program = analysis.result.unwrap();
    let result = lang::artifact::compile(
        program.clone(),
        &analysis.sources,
        Default::default(),
        Default::default(),
    );
    assert!(matches!(
        result,
        Err(lang::artifact::CompileError::UnsupportedTaskProfile { .. })
    ));
    if cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(
            target_os = "windows",
            target_arch = "x86_64",
            target_env = "msvc"
        )
    )) {
        assert!(matches!(
            lang::native::compile(
                program,
                &analysis.sources,
                Default::default(),
                lang_codegen_clif::Triple::host()
            ),
            Err(lang::native::Error::UnsupportedTaskProfile { .. })
        ));
    }
}

#[test]
fn task_analysis_preserves_source_diagnostics_and_mir_resource_caps() {
    use lang_diagnostics::codes;
    let source = "async fn main()->i64{let mut n:i64=1;let mut loan:&i64=&n;yield;return 0;}";
    let analysis = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("loan-task.lang"),
        source.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(analysis.result.is_err());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::BORROW_ACROSS_SUSPENSION)
    );
    for d in analysis.diagnostics {
        d.validate(&analysis.sources).unwrap();
    }
    let mut limits = lang::frontend::CompileLimits::default();
    limits.ownership.bodies.places.ownership.unit.work = 1;
    let source = "async fn main()->i64{yield;return 42;}";
    let analysis = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("limited-task.lang"),
        source.as_bytes(),
        None,
        limits,
        None,
    );
    assert!(analysis.result.is_err());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::RESOURCE_LIMIT)
    );
    for d in analysis.diagnostics {
        d.validate(&analysis.sources).unwrap();
    }
}

#[test]
fn every_fuel_prefix_retires_frames_and_retained_results() {
    let proof = prepare(include_str!("../crates/syntax/tests/fixtures/tasks.lang"));
    let mut baseline = allocator();
    let output = vm::execute_entry(
        proof.verified(),
        &mut baseline,
        control(),
        Default::default(),
    )
    .unwrap();
    for fuel in 0..=output.usage.fuel {
        let mut allocator = allocator();
        let control = e::Control::with_limits(e::Limits {
            fuel: Some(fuel),
            ..Default::default()
        });
        match vm::execute_entry(
            proof.verified(),
            &mut allocator,
            control,
            Default::default(),
        ) {
            Ok(result) => {
                assert_eq!(fuel, output.usage.fuel);
                assert_eq!(result.value, 42);
            }
            Err(error) => {
                assert!(
                    matches!(error.reason, vm::Reason::Control(e::Failure::FuelExhausted)),
                    "fuel {fuel}: {error:?}"
                );
                assert_eq!(error.tasks.live, 0, "fuel {fuel}");
            }
        }
        assert_eq!(allocator.stats().live_allocations, 0, "fuel {fuel}");
    }
}

#[test]
fn task_and_frame_limits_refuse_before_child_admission_and_cleanup_the_tree() {
    let proof = prepare(include_str!("../crates/syntax/tests/fixtures/tasks.lang"));
    for (tasks, frames) in [
        (
            t::Limits {
                live_tasks: 1,
                total_tasks: 10,
            },
            256,
        ),
        (
            t::Limits {
                live_tasks: 10,
                total_tasks: 2,
            },
            256,
        ),
        (t::Limits::default(), 1),
        (t::Limits::default(), 2),
    ] {
        let mut allocator = allocator();
        let error = vm::execute_entry(
            proof.verified(),
            &mut allocator,
            e::Control::with_limits(e::Limits {
                frames: Some(frames),
                ..Default::default()
            }),
            vm::Limits {
                tasks,
                ..Default::default()
            },
        )
        .err()
        .unwrap();
        assert!(
            matches!(
                error.reason,
                vm::Reason::Schedule(
                    t::Error::LiveTaskLimit { .. } | t::Error::TotalTaskLimit { .. }
                ) | vm::Reason::Execution(
                    lang_mir::unit::ownership::failure::ExecutionFailure::FrameLimit
                )
            ),
            "{error:?}"
        );
        assert_eq!(error.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
    let mut allocator = allocator();
    let error = vm::execute_entry(
        proof.verified(),
        &mut allocator,
        e::Control::with_limits(e::Limits {
            frames: Some(0),
            ..Default::default()
        }),
        Default::default(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        vm::Reason::Execution(lang_mir::unit::ownership::failure::ExecutionFailure::FrameLimit)
    ));
    assert_eq!(allocator.stats().requests, 0);
}

#[test]
fn trace_exhaustion_keeps_mandatory_cleanup_unmetered() {
    let proof = prepare(include_str!("../crates/syntax/tests/fixtures/tasks.lang"));
    let mut baseline = allocator();
    let output = vm::execute_entry(
        proof.verified(),
        &mut baseline,
        control(),
        vm::Limits {
            trace_events: 256,
            ..Default::default()
        },
    )
    .unwrap();
    for limit in 1..output.events.len() {
        let mut allocator = allocator();
        let error = vm::execute_entry(
            proof.verified(),
            &mut allocator,
            control(),
            vm::Limits {
                trace_events: limit,
                ..Default::default()
            },
        )
        .err()
        .unwrap();
        assert!(
            matches!(error.reason, vm::Reason::TraceLimit),
            "limit {limit}: {error:?}"
        );
        assert_eq!(error.tasks.live, 0, "limit {limit}");
        assert_eq!(allocator.stats().live_allocations, 0, "limit {limit}");
    }
}

struct Provider {
    at: usize,
    fail: usize,
}
impl a::Provider for Provider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<Self::Region> {
        self.at += 1;
        if self.at == self.fail {
            None
        } else {
            a::Storage::try_new(request).ok()
        }
    }
    fn deallocate(&mut self, _: Self::Region, _: a::Request) {}
}
#[test]
fn every_provider_refusal_rolls_back_unpublished_frames_or_cleans_admitted_children() {
    let proof = prepare(include_str!("../crates/syntax/tests/fixtures/tasks.lang"));
    let mut baseline = a::Allocator::new(
        Provider {
            at: 0,
            fail: usize::MAX,
        },
        Default::default(),
    )
    .unwrap();
    let output = vm::execute_entry(
        proof.verified(),
        &mut baseline,
        control(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(output.value, 42);
    let callbacks = baseline.provider().unwrap().at;
    assert!(callbacks > 0);
    for fail in 1..=callbacks {
        let mut allocator =
            a::Allocator::new(Provider { at: 0, fail }, Default::default()).unwrap();
        let result = vm::execute_entry(
            proof.verified(),
            &mut allocator,
            control(),
            Default::default(),
        );
        assert!(result.is_err(), "request {fail}");
        if let Err(error) = result {
            assert_eq!(error.tasks.live, 0, "request {fail}: {error:?}");
        }
        assert_eq!(allocator.stats().live_allocations, 0, "request {fail}");
    }
}

#[test]
fn cancelled_join_is_typed_and_completed_cancellation_is_inert() {
    for (yields, expected) in [("", 7), ("yield;", 7), ("yield;yield;yield;", 42)] {
        let source = format!(
            "{WORKER}async fn main()->i64{{let mut h:core::Task<i64>=spawn worker(42);{yields}cancel h;let mut r:core::Result<i64,core::TaskError>=await h;match r{{core::Result::Ok(n)=>{{return n;}},core::Result::Err(core::TaskError::Cancelled)=>{{return 7;}},}}}}"
        );
        let proof = prepare(&source);
        let mut allocator = allocator();
        let output = vm::execute_entry(
            proof.verified(),
            &mut allocator,
            control(),
            Default::default(),
        )
        .unwrap();
        assert_eq!(output.value, expected, "{yields}");
        assert_eq!(output.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn loop_scope_instances_join_and_dispose_unclaimed_results() {
    let proof = prepare(&format!(
        "{WORKER}async fn main()->i64{{let mut n:i64=0;while n<3{{task_scope{{let mut h:core::Task<i64>=spawn worker(n);}}n=n+1;}}return n;}}"
    ));
    let mut allocator = allocator();
    let output = vm::execute_entry(
        proof.verified(),
        &mut allocator,
        control(),
        vm::Limits {
            trace_events: 128,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(output.value, 3);
    assert_eq!(output.tasks.admitted, 4);
    assert_eq!(output.tasks.live, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    let exited: Vec<_> = output
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::ScopeExit(1) => Some(e.task),
            _ => None,
        })
        .collect();
    assert_eq!(exited, vec![0, 0, 0]);
}

#[test]
fn guest_calls_and_direct_async_await_keep_their_continuations() {
    let proof = prepare(
        "fn add(n:i64)->i64{return n+1;}async fn worker(n:i64)->i64{yield;return add(n);}async fn main()->i64{let mut a:core::Result<i64,core::TaskError>=await worker(19);let mut b:core::Result<i64,core::TaskError>=await worker(21);return core::result_unwrap::<i64,core::TaskError>(a)+core::result_unwrap::<i64,core::TaskError>(b);}",
    );
    let mut allocator = allocator();
    let output = vm::execute_entry(
        proof.verified(),
        &mut allocator,
        control(),
        vm::Limits {
            trace_events: 128,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(output.value, 42);
    assert_eq!(output.tasks.admitted, 3);
    assert_eq!(
        output
            .events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Spawn { .. }))
            .count(),
        2
    );
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn child_panic_preserves_origin_and_cleans_siblings_without_awaiting_them() {
    let source = format!(
        "{WORKER}async fn bad()->i64{{return core::panic::<i64>(91);}}async fn main()->i64{{let mut failed:core::Task<i64>=spawn bad();let mut sibling:core::Task<i64>=spawn worker(42);let mut r:core::Result<i64,core::TaskError>=await failed;return 0;}}"
    );
    let proof = prepare(&source);
    let mut allocator = allocator();
    let error = vm::execute_entry(
        proof.verified(),
        &mut allocator,
        control(),
        vm::Limits {
            trace_events: 128,
            ..Default::default()
        },
    )
    .err()
    .unwrap();
    assert!(matches!(error.reason, vm::Reason::Panic(Some(91))));
    let span = error.span.unwrap();
    let source = &proof
        .verified()
        .typed()
        .places()
        .declarations()
        .types()
        .table()
        .sources[span.source.0 as usize]
        .text;
    assert_eq!(
        &source[span.start as usize..span.end as usize],
        "core::panic::<i64>(91)"
    );
    assert_eq!(error.tasks.live, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(
        error
            .events
            .iter()
            .any(|e| e.task == 2 && e.kind == EventKind::Complete(t::Outcome::Cancelled))
    );
}
#[test]
fn source_spawn_yield_join_executes_through_the_shared_task_driver() {
    let proof = prepare(include_str!("../crates/syntax/tests/fixtures/tasks.lang"));
    let mut allocator = lang_runtime::allocation::Allocator::new(
        lang_runtime::allocation::System,
        Default::default(),
    )
    .unwrap();
    let output = lang_vm::unit::tasks::execute_entry(
        proof.verified(),
        &mut allocator,
        lang_runtime::execution::Control::with_limits(Default::default()),
        lang_vm::unit::tasks::Limits {
            trace_events: 256,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(output.value, 42);
    assert_eq!(output.tasks.admitted, 3);
    assert_eq!(output.tasks.live, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    let yields: Vec<_> = output
        .events
        .iter()
        .filter(|e| e.kind == lang_vm::unit::tasks::EventKind::Yield)
        .map(|e| e.task)
        .collect();
    assert_eq!(yields, vec![1, 2]);
}
