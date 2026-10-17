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
const OWNED_SOURCE: &str = include_str!("../examples/scripts/owned_bytes.lang");
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
fn all_three_extractions_transfer_one_source_buffer_without_new_allocation() {
    let program = checked(OWNED_SOURCE);
    let typed = program.mir().activity().moved().typed();
    let layouts = lang_runtime::layout::Layouts::compile(
        typed.places().declarations().types(),
        Default::default(),
    )
    .unwrap();
    let plan = lang_runtime::layout::FramePlan::compile(
        &layouts,
        typed,
        lang_mir::unit::FunctionId(0),
        Default::default(),
        usize::MAX,
    )
    .unwrap();
    let admitted: Vec<_> = plan
        .requests()
        .iter()
        .copied()
        .filter(|r| r.bytes() != 0)
        .collect();
    for deny_buffer in [false, true] {
        let (mut allocator, trace) = allocator();
        trace.borrow_mut().deny_buffer = deny_buffer;
        let mut instance = c::Instance::default();
        let outcome = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .unwrap();
        assert_eq!(outcome.value, if deny_buffer { -1 } else { 0 });
        let trace = trace.borrow();
        assert_eq!(&trace.requests[..admitted.len()], admitted.as_slice());
        assert_eq!(trace.requests.len(), admitted.len() + 1);
        assert_eq!(trace.requests.last().unwrap().bytes(), 24);
        assert_eq!(
            outcome.allocations.requests as usize,
            plan.requests().len() + 1
        );
        assert_eq!(
            trace.requests.len(),
            trace.released.len() + usize::from(deny_buffer)
        );
        assert_eq!(outcome.allocations.live_allocations, 0);
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn extraction_frame_oom_rolls_back_every_admission_prefix_before_guest_entry() {
    let program = checked(OWNED_SOURCE);
    let (mut baseline, trace) = allocator();
    vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        control(),
    )
    .unwrap();
    let requests = trace.borrow().requests.len();
    for fail_at in 1..=requests {
        let (mut allocator, trace) = allocator();
        trace.borrow_mut().fail_at = fail_at;
        let mut instance = c::Instance::default();
        let result = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        );
        if fail_at == requests {
            assert_eq!(result.unwrap().value, -1);
        } else {
            let error = result.err().unwrap();
            assert!(matches!(error.reason, vm::Reason::Preparation(_)));
            assert_eq!(error.usage.fuel, 0);
        }
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(allocator.stats().live_bytes, 0);
        assert_eq!(
            trace.borrow().requests.len(),
            trace.borrow().released.len() + 1
        );
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn repeated_sum_construction_and_extraction_recycle_admitted_cells_until_fuel_exit() {
    let program = checked(
        "fn main() -> i64 { let mut running: bool = true; while running { let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(made); let mut wrapped: core::Option<core::Bytes> = core::Option::Some(bytes); let mut taken: core::Bytes = core::option_unwrap::<core::Bytes>(wrapped); } return 0; }",
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
    assert!(matches!(
        error.reason,
        vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::FuelLimit))
    ));
    assert_eq!(error.panic_code, None);
    assert!(
        trace
            .borrow()
            .requests
            .iter()
            .filter(|r| r.bytes() == 24)
            .count()
            > 1
    );
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(!instance.is_poisoned());
}

#[test]
fn copy_sum_construction_and_extraction_preserve_source_reuse() {
    let program = checked(
        "fn main() -> i64 { let mut optional: core::Option<i64> = core::Option::Some(7); let mut a: i64 = core::option_unwrap::<i64>(optional); let mut b: i64 = core::option_unwrap::<i64>(optional); let mut ok: core::Result<i64, i64> = core::Result::Ok(a + b); let mut value: i64 = core::result_unwrap::<i64, i64>(ok); let mut err: core::Result<i64, i64> = core::Result::Err(3); return value + core::result_unwrap_err::<i64, i64>(err); }",
    );
    let (mut allocator, _) = allocator();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 17);
    assert_eq!(outcome.allocations.live_allocations, 0);
}

#[test]
fn wrong_cases_keep_panic_codes_and_spans_and_clean_owning_payloads_without_poisoning() {
    for (body, operation, code) in [
        (
            "let mut missing: core::Option<core::Bytes> = core::Option::None; let mut bytes: core::Bytes = core::option_unwrap::<core::Bytes>(missing); return 0;",
            Operation::OptionUnwrap,
            -1,
        ),
        (
            "let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(made); let mut err: core::Result<i64, core::Bytes> = core::Result::Err(bytes); return core::result_unwrap::<i64, core::Bytes>(err);",
            Operation::ResultUnwrap,
            -2,
        ),
        (
            "let mut err: core::AllocError = core::result_unwrap_err::<core::Bytes, core::AllocError>(made); return 0;",
            Operation::ResultUnwrapErr,
            -3,
        ),
    ] {
        let source = format!(
            "fn main() -> i64 {{ let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); {body} }}"
        );
        let program = checked(&source);
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        assert_eq!(error.panic_code, Some(code));
        assert!(matches!(
            error.reason,
            vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::Panic))
        ));
        let span = error.fault.unwrap();
        assert!(
            source[span.start as usize..span.end as usize]
                .starts_with(&format!("core::{}::<", operation.name()))
        );
        assert_eq!(
            error.report.as_ref().unwrap().primary().unwrap().span,
            Some(span)
        );
        assert!(error.report.as_ref().unwrap().secondary().is_empty());
        assert!(!instance.is_poisoned());
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
        assert_eq!(
            trace
                .borrow()
                .released
                .iter()
                .filter(|r| r.bytes() == 24)
                .count(),
            1
        );
        let again = checked(SOURCE);
        assert_eq!(
            vm::execute_entry(
                again.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control()
            )
            .unwrap()
            .value,
            0
        );
    }
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
    for source in [SOURCE, OWNED_SOURCE] {
        let program = checked(source);
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
                let text = &typed.places().declarations().types().table().sources
                    [span.source.0 as usize]
                    .text;
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
}

#[test]
fn unsupported_core_operations_are_rejected_before_earlier_source_allocations() {
    for (later, expected, spelling) in [
        (
            "let mut later: core::Result<core::Vec<i64>, core::AllocError> = core::vec_new::<i64>(core::allocator());",
            Some(Operation::VecNew),
            "vec_new",
        ),
        (
            "let mut later: core::Result<core::Text, core::Utf8Error> = core::text_from_utf8(core::result_unwrap::<core::Bytes, core::AllocError>(first));",
            Some(Operation::TextFromUtf8),
            "text_from_utf8",
        ),
        (
            "let mut later: core::Option<core::Text> = core::Option::None;",
            None,
            "Option::None",
        ),
    ] {
        let program = checked(&format!(
            "fn main() -> i64 {{ let mut first: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator()); {later} return 0; }}"
        ));
        let (mut allocator, trace) = allocator();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        assert!(matches!(error.reason, vm::Reason::Unsupported { core } if core == expected));
        assert_eq!(allocator.stats(), a::Stats::default());
        assert!(trace.borrow().requests.is_empty());
        let span = error.fault.unwrap();
        let typed = program.mir().activity().moved().typed();
        let text =
            &typed.places().declarations().types().table().sources[span.source.0 as usize].text;
        assert!(text[span.start as usize..span.end as usize].contains(spelling));
    }
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
