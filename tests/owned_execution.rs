use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_mir::unit::ownership::{core::Operation, failure::ExecutionFailure};
use lang_runtime::{allocation as a, cleanup as c};
use lang_semantics::analysis::{OwnershipProgram, Program};
use lang_vm::{
    InterruptionFlag,
    unit::{Control, Limits, owned as vm},
};
use std::{cell::RefCell, path::Path, rc::Rc, time::Instant};

const SOURCE: &str = include_str!("../examples/scripts/bytes.lang");
fn checked(source: &str) -> Box<OwnershipProgram> {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("owned.lang"),
        source.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let Program::Ownership(program) = analysis.result.unwrap() else {
        panic!("ownership source")
    };
    program
}
#[derive(Default)]
struct Trace {
    requests: Vec<a::Request>,
    released: Vec<a::Request>,
    deny_buffer: bool,
    fail_at: usize,
}
struct Provider(Rc<RefCell<Trace>>);
impl a::Provider for Provider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<a::Storage> {
        let mut trace = self.0.borrow_mut();
        trace.requests.push(request);
        if (trace.deny_buffer && request.bytes() == 24) || trace.fail_at == trace.requests.len() {
            None
        } else {
            a::Storage::try_new(request).ok()
        }
    }
    fn deallocate(&mut self, _: a::Storage, request: a::Request) {
        self.0.borrow_mut().released.push(request);
    }
}
fn allocator() -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let allocator = a::Allocator::new(
        Provider(Rc::clone(&trace)),
        a::Limits {
            live_allocations: 128,
            ..Default::default()
        },
    )
    .unwrap();
    (allocator, trace)
}
fn control() -> Control<'static> {
    Control::with_limits(Limits::default())
}

#[test]
fn frame_admission_oom_prevents_entry_and_allows_a_later_retry() {
    let program = checked(SOURCE);
    let (mut allocator, trace) = allocator();
    trace.borrow_mut().fail_at = 1;
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        vm::Reason::Preparation(lang_runtime::layout::Error::Allocation(
            a::Error::OutOfMemory
        ))
    ));
    assert_eq!(error.usage.fuel, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(!instance.is_poisoned());
    assert_eq!(trace.borrow().requests.len(), 1);
    let retried = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(retried.value, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn repeated_bytes_loans_and_moves_reuse_frame_cells_until_fuel_exhaustion() {
    let program = checked(
        "fn main() -> i64 { let mut repeat: bool = true; while repeat { let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); match made { core::Result::Ok(value) => { let mut length: i64 = core::bytes_len(value); }, core::Result::Err(_) => {}, } } return 0; }",
    );
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        Control::with_limits(Limits {
            fuel: Some(200),
            ..Default::default()
        }),
    )
    .err()
    .unwrap();
    assert!(
        matches!(
            error.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
        ),
        "{error:?}"
    );
    assert_eq!(error.usage.fuel, 200);
    assert!(
        trace
            .borrow()
            .requests
            .iter()
            .filter(|r| r.bytes() == 24)
            .count()
            > 1
    );
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    assert!(!instance.is_poisoned());
}

#[test]
fn copy_scalar_reads_and_arithmetic_coexist_with_a_live_owned_result() {
    let program = checked(
        "fn main() -> i64 { let mut baseline: i64 = 7; let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); match made { core::Result::Ok(value) => { let mut length: i64 = core::bytes_len(value); return baseline + length; }, core::Result::Err(_) => { return -1; }, } }",
    );
    let (mut allocator, _) = allocator();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 7);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn bytes_source_uses_real_storage_and_releases_every_region_exactly_once() {
    let program = checked(SOURCE);
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 0);
    assert_eq!(outcome.allocations.live_allocations, 0);
    assert_eq!(outcome.allocations.live_bytes, 0);
    assert!(outcome.usage.fuel > 0);
    assert_eq!(outcome.usage.peak_frames, 1);
    assert_eq!(outcome.usage.host_calls, 0);
    assert!(!instance.is_poisoned());
    let trace = trace.borrow();
    assert_eq!(trace.requests.iter().filter(|r| r.bytes() == 24).count(), 1);
    assert_eq!(trace.released.iter().filter(|r| r.bytes() == 24).count(), 1);
    assert_eq!(trace.requests.len(), trace.released.len());
    assert_eq!(
        outcome.allocations.allocations,
        outcome.allocations.deallocations
    );
}

#[test]
fn source_handles_bytes_oom_as_typed_err_without_allocating_an_error_result() {
    let program = checked(SOURCE);
    let (mut allocator, trace) = allocator();
    trace.borrow_mut().deny_buffer = true;
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, -1);
    assert!(!instance.is_poisoned());
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().live_bytes, 0);
    let trace = trace.borrow();
    assert_eq!(trace.requests.last().unwrap().bytes(), 24);
    assert_eq!(trace.requests.len(), trace.released.len() + 1);
    assert_eq!(
        allocator.stats().cumulative_bytes,
        trace.requests.iter().map(|r| r.bytes()).sum::<u64>()
    );
}

#[test]
fn every_fuel_prefix_cleans_source_owners_and_ends_loans_without_poisoning() {
    let program = checked(SOURCE);
    let (mut baseline, _) = allocator();
    let fuel = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        control(),
    )
    .unwrap()
    .usage
    .fuel;
    for available in 0..=fuel {
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Control::with_limits(Limits {
                fuel: Some(available),
                ..Default::default()
            }),
        );
        if available < fuel {
            let error = result.err().unwrap();
            assert!(matches!(
                error.reason,
                vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
            ));
            assert_eq!(error.usage.fuel, available);
            let report = error.report.unwrap();
            assert!(report.secondary().is_empty());
            let span = error.fault.unwrap();
            let typed = program.mir().activity().moved().typed();
            let text =
                &typed.places().declarations().types().table().sources[span.source.0 as usize].text;
            assert!(text.get(span.start as usize..span.end as usize).is_some());
        } else {
            assert_eq!(result.unwrap().value, 0);
        }
        assert!(!instance.is_poisoned());
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(allocator.stats().live_bytes, 0);
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    }
}

#[test]
fn unsupported_core_operations_are_rejected_before_earlier_source_allocations() {
    let program = checked(
        "fn main() -> i64 { let mut first: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); let mut later: core::Result<core::Vec<i64>, core::AllocError> = core::vec_new::<i64>(core::allocator()); return 0; }",
    );
    let (mut allocator, trace) = allocator();
    let error = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert!(matches!(
        error.reason,
        vm::Reason::Unsupported {
            core: Some(Operation::VecNew)
        }
    ));
    assert_eq!(allocator.stats(), a::Stats::default());
    assert!(trace.borrow().requests.is_empty());
    let span = error.fault.unwrap();
    let typed = program.mir().activity().moved().typed();
    let text = &typed.places().declarations().types().table().sources[span.source.0 as usize].text;
    assert!(text[span.start as usize..span.end as usize].contains("vec_new"));
}

#[test]
fn entry_authority_poison_control_and_frame_cell_limits_prevent_callbacks() {
    let program = checked(SOURCE);
    let mut instance = c::Instance::default();
    let missing =
        vm::execute_entry::<Provider>(program.failure_flow(), &mut instance, None, control())
            .err()
            .unwrap();
    assert!(matches!(missing.reason, vm::Reason::MissingAllocator));
    let flag = InterruptionFlag::new();
    flag.interrupt();
    for control in [
        control().and_interruption(&flag),
        control().and_deadline(Instant::now()),
        Control::with_limits(Limits {
            frames: Some(0),
            ..Default::default()
        }),
        Control::with_limits(Limits {
            live_cells: Some(0),
            ..Default::default()
        }),
    ] {
        let (mut allocator, trace) = allocator();
        assert!(
            vm::execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control
            )
            .is_err()
        );
        assert!(trace.borrow().requests.is_empty());
        assert_eq!(allocator.stats(), a::Stats::default());
    }
    let mut poisoned = c::Instance::default();
    let mut frame = poisoned
        .enter(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |_: &mut (), _| Err(1),
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    assert!(frame.initialize(c::Slot(0), (), None).is_ok());
    frame.finish(c::Exit::Return(None));
    let (mut allocator, trace) = allocator();
    let denied = vm::execute_entry(
        program.failure_flow(),
        &mut poisoned,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert!(matches!(denied.reason, vm::Reason::Poisoned));
    assert!(trace.borrow().requests.is_empty());
}
