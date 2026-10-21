use lang_codegen_clif::tasks::CompiledProgram;
use lang_runtime::{
    allocation as a, execution as e,
    task::{self as t, runner as r},
};
use std::path::Path;
#[path = "support/task_static.rs"]
mod task_static;
fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("native-tasks.lang"),
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
fn limits() -> r::Limits {
    r::Limits {
        trace_events: 512,
        ..Default::default()
    }
}
#[allow(
    clippy::result_large_err,
    reason = "Compare the production failure evidence without changing its allocation behavior."
)]
fn compare(
    proof: &lang_mir::unit::ownership::tasks::VerifiedTasks,
    native: &CompiledProgram<'_>,
    control: e::Control<'_>,
    limits: r::Limits,
) -> Result<r::Outcome, r::Failure> {
    let mut vm_allocator = allocator();
    let mut native_allocator = allocator();
    let vm = lang_vm::unit::tasks::execute_entry(proof, &mut vm_allocator, control, limits);
    let actual = native.execute(&mut native_allocator, control, limits);
    assert_eq!(vm_allocator.stats().live_allocations, 0);
    assert_eq!(native_allocator.stats().live_allocations, 0);
    assert_eq!(vm_allocator.stats(), native_allocator.stats());
    match (&vm, &actual) {
        (Ok(a), Ok(b)) => {
            assert_eq!(a.value, b.value);
            assert_eq!(a.usage, b.usage);
            assert_eq!(a.tasks, b.tasks);
            assert_eq!(a.events, b.events);
        }
        (Err(a), Err(b)) => {
            assert_eq!(format!("{:?}", a.reason), format!("{:?}", b.reason));
            assert_eq!(a.function, b.function);
            assert_eq!(a.span, b.span);
            assert_eq!(a.usage, b.usage);
            assert_eq!(a.tasks, b.tasks);
            assert_eq!(a.events, b.events);
            assert_eq!(b.tasks.live, 0);
        }
        _ => panic!(
            "VM/native outcome differs: vm={} native={}",
            vm.is_ok(),
            actual.is_ok()
        ),
    }
    actual
}
const WORKER: &str = "async fn worker(n:i64)->i64{yield;cancel_check;return n;}";
const AGGREGATES: &str = "struct Pair{left:i64,right:i64}enum Choice{None,Pair(Pair)}async fn worker(n:i64)->Choice{let mut p:Pair=Pair{right:n,left:19};p.left=p.left+1;yield;return Choice::Pair(p);}async fn main()->i64{let mut r:core::Result<Choice,core::TaskError>=await worker(22);let mut choice:Choice=core::result_unwrap::<Choice,core::TaskError>(r);match choice{Choice::Pair(Pair{left:a,right:b})=>{return a+b;},Choice::None=>{return 0;},}}";

#[test]
fn native_finite_aggregate_storage_and_case_projections_match_vm() {
    let proof = prepare(AGGREGATES);
    let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
    assert_eq!(
        compare(proof.verified(), &native, control(), limits())
            .unwrap()
            .value,
        42
    );
}

#[test]
fn native_unwrap_and_synchronous_callee_failures_match_vm() {
    for source in [
        "async fn main()->i64{return core::option_unwrap::<i64>(core::Option::None);}",
        "fn bad()->i64{return core::panic::<i64>(8);}async fn main()->i64{return bad();}",
    ] {
        let proof = prepare(source);
        let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
        assert!(compare(proof.verified(), &native, control(), limits()).is_err());
    }
}

#[test]
fn native_source_spawn_yield_join_matches_vm_and_retains_source_ranges() {
    let proof = prepare(include_str!("../examples/scripts/tasks.lang"));
    let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
    let output = compare(proof.verified(), &native, control(), limits()).unwrap();
    assert_eq!(output.value, 42);
    assert_eq!(output.tasks.admitted, 3);
    assert!(
        native
            .metadata()
            .all(|f| f.code_size > 0 && !f.source_ranges.is_empty())
    );
    let yields: Vec<_> = output
        .events
        .iter()
        .filter(|e| e.kind == r::EventKind::Yield)
        .map(|e| e.task)
        .collect();
    assert_eq!(yields, vec![1, 2]);
}
#[test]
fn native_cancelled_join_and_completed_claim_match_vm() {
    for (yields, expected) in [("", 7), ("yield;", 7), ("yield;yield;yield;", 42)] {
        let source = format!(
            "{WORKER}async fn main()->i64{{let mut h:core::Task<i64>=spawn worker(42);{yields}cancel h;let mut r:core::Result<i64,core::TaskError>=await h;match r{{core::Result::Ok(n)=>{{return n;}},core::Result::Err(core::TaskError::Cancelled)=>{{return 7;}},}}}}"
        );
        let proof = prepare(&source);
        let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
        assert_eq!(
            compare(proof.verified(), &native, control(), limits())
                .unwrap()
                .value,
            expected
        );
    }
}
#[test]
fn native_loop_scope_instances_join_and_discard_unclaimed_results() {
    let proof = prepare(&format!(
        "{WORKER}async fn main()->i64{{let mut n:i64=0;while n<3{{task_scope{{let mut h:core::Task<i64>=spawn worker(n);}}n=n+1;}}return n;}}"
    ));
    let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
    let output = compare(proof.verified(), &native, control(), limits()).unwrap();
    assert_eq!(output.value, 3);
    assert_eq!(output.tasks.admitted, 4);
}
#[test]
fn native_guest_calls_direct_await_and_branch_transfer_match_vm() {
    let source = "fn add(n:i64)->i64{if n<20{return n+1;}return n+1;}async fn worker(n:i64)->i64{yield;return add(n);}async fn main()->i64{let mut a:core::Result<i64,core::TaskError>=await worker(19);let mut b:core::Result<i64,core::TaskError>=await worker(21);return core::result_unwrap::<i64,core::TaskError>(a)+core::result_unwrap::<i64,core::TaskError>(b);}";
    let proof = prepare(source);
    let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
    assert_eq!(
        compare(proof.verified(), &native, control(), limits())
            .unwrap()
            .value,
        42
    );
}
#[test]
fn native_child_failure_keeps_origin_and_cancels_siblings() {
    let source = format!(
        "{WORKER}async fn bad()->i64{{return core::panic::<i64>(91);}}async fn main()->i64{{let mut failed:core::Task<i64>=spawn bad();let mut sibling:core::Task<i64>=spawn worker(42);let mut r:core::Result<i64,core::TaskError>=await failed;return 0;}}"
    );
    let proof = prepare(&source);
    let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
    let error = compare(proof.verified(), &native, control(), limits())
        .err()
        .unwrap();
    assert!(matches!(error.reason, r::Reason::Panic(Some(91))));
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
    assert!(
        error
            .events
            .iter()
            .any(|e| e.task == 2 && e.kind == r::EventKind::Complete(t::Outcome::Cancelled))
    );
}
#[test]
fn every_native_fuel_and_trace_prefix_matches_vm_cleanup() {
    let proof = prepare(include_str!("../examples/scripts/tasks.lang"));
    let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
    let output = compare(proof.verified(), &native, control(), limits()).unwrap();
    for fuel in 0..=output.usage.fuel {
        let result = compare(
            proof.verified(),
            &native,
            e::Control::with_limits(e::Limits {
                fuel: Some(fuel),
                ..Default::default()
            }),
            limits(),
        );
        match result {
            Ok(out) => {
                assert_eq!(fuel, output.usage.fuel);
                assert_eq!(out.value, 42);
            }
            Err(error) => assert!(matches!(
                error.reason,
                r::Reason::Control(e::Failure::FuelExhausted)
            )),
        }
    }
    for trace_events in 1..output.events.len() {
        let error = compare(
            proof.verified(),
            &native,
            control(),
            r::Limits {
                trace_events,
                ..Default::default()
            },
        )
        .err()
        .unwrap();
        assert!(matches!(error.reason, r::Reason::TraceLimit));
    }
}
#[test]
fn native_task_frame_cell_and_preparation_limits_refuse_and_cleanup() {
    let proof = prepare(include_str!("../examples/scripts/tasks.lang"));
    let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
    for (tasks, frames, cells) in [
        (
            t::Limits {
                live_tasks: 1,
                total_tasks: 10,
            },
            256,
            usize::MAX,
        ),
        (
            t::Limits {
                live_tasks: 10,
                total_tasks: 2,
            },
            256,
            usize::MAX,
        ),
        (t::Limits::default(), 0, usize::MAX),
        (t::Limits::default(), 1, usize::MAX),
        (t::Limits::default(), 2, usize::MAX),
        (t::Limits::default(), 256, 1),
    ] {
        assert!(
            compare(
                proof.verified(),
                &native,
                e::Control::with_limits(e::Limits {
                    frames: Some(frames),
                    live_cells: Some(cells),
                    ..Default::default()
                }),
                r::Limits { tasks, ..limits() }
            )
            .is_err()
        );
    }
    let usage = native.usage();
    assert!(usage.work > 0);
    let compile_limits = lang_mir::unit::Limits {
        work: usage.work - 1,
        ..Default::default()
    };
    assert!(matches!(
        CompiledProgram::compile(proof.verified(), compile_limits),
        Err(lang_codegen_clif::tasks::Error::Resource(_))
    ));
    let mut runtime_limits = limits();
    runtime_limits.preparation.work = 1;
    let mut allocator = allocator();
    assert!(
        native
            .execute(&mut allocator, control(), runtime_limits)
            .is_err()
    );
    assert_eq!(allocator.stats().requests, 0);
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
fn every_native_provider_refusal_retires_unpublished_and_admitted_storage() {
    let proof = prepare(include_str!("../examples/scripts/tasks.lang"));
    let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
    let mut baseline = a::Allocator::new(
        Provider {
            at: 0,
            fail: usize::MAX,
        },
        Default::default(),
    )
    .unwrap();
    native.execute(&mut baseline, control(), limits()).unwrap();
    let callbacks = baseline.provider().unwrap().at;
    assert!(callbacks > 0);
    for fail in 1..=callbacks {
        let mut allocator =
            a::Allocator::new(Provider { at: 0, fail }, Default::default()).unwrap();
        let error = native
            .execute(&mut allocator, control(), limits())
            .err()
            .unwrap();
        assert_eq!(error.tasks.live, 0, "request {fail}: {error:?}");
        assert_eq!(allocator.stats().live_allocations, 0, "request {fail}");
    }
}
#[test]
fn unsupported_native_task_operations_and_controls_refuse_before_guest_effects() {
    let foreign =
        prepare("extern fn host(value:i64)->i64; async fn main()->i64{yield;return host(42);}");
    assert!(matches!(
        CompiledProgram::compile(foreign.verified(), Default::default()),
        Err(lang_codegen_clif::tasks::Error::Unsupported { .. })
    ));
    let proof = prepare(include_str!("../examples/scripts/tasks.lang"));
    let native = CompiledProgram::compile(proof.verified(), Default::default()).unwrap();
    let mut allocator = allocator();
    assert!(matches!(
        native
            .execute(
                &mut allocator,
                control().and_service_work(Some(100)),
                limits()
            )
            .err()
            .unwrap()
            .reason,
        r::Reason::Unsupported
    ));
    assert_eq!(allocator.stats().requests, 0);
}
