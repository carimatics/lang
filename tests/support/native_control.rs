use super::*;
use std::time::{Duration, Instant};

const EXAMPLES: [&str; 5] = [
    include_str!("../../examples/scripts/utf8.lang"),
    include_str!("../../examples/analysis/native_vec_nested.lang"),
    include_str!("../../examples/analysis/native_map_nested.lang"),
    include_str!("../../examples/analysis/native_shared_custody.lang"),
    include_str!("../../examples/analysis/native_arena_regions.lang"),
];
pub(super) fn limits() -> e::Limits {
    e::Limits {
        fuel: None,
        host_calls: Some(0),
        frames: Some(8),
        live_cells: Some(1000),
    }
}
pub(super) fn native(
    flow: &m::ownership::failure::Elaborated,
    layouts: &l::Layouts<'_>,
    program: &CompiledProgram<'_>,
    control: e::Control<'_>,
    interrupt: Option<(usize, Rc<e::InterruptionFlag>)>,
) -> Seen {
    execute_controlled(
        flow,
        layouts,
        program,
        Run {
            inputs: &[],
            fuel: control.limits().fuel,
            max_frames: 8,
            cells: 1000,
            refuse: None,
            refuse_cleanup: false,
        },
        control,
        interrupt,
    )
}
type VmResult = Result<lang_vm::unit::owned::Outcome, lang_vm::unit::owned::Failure>;

#[test]
fn unmetered_service_profiles_reject_before_provider_effects() {
    let unused = format!(
        "{}\n{}",
        EXAMPLES[0],
        EXAMPLES[1].replacen("fn main()", "fn unused()", 1)
    );
    for text in [EXAMPLES[1], EXAMPLES[2], EXAMPLES[3], EXAMPLES[4], &unused] {
        let source = source(text);
        let flow = source.failure_flow();
        assert!(!e::service_work_supported(flow));
        let types = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let program = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
        for limit in [0, 1, u64::MAX] {
            let control = e::Control::with_limits(limits()).and_service_work(Some(limit));
            let actual = native(flow, &layouts, &program, control, None);
            let (expected, requests) = vm(flow, control, None);
            let expected = expected.err().unwrap();
            let error = e::UnsupportedServiceWork { limit };
            assert!(
                matches!(expected.reason, lang_vm::unit::owned::Reason::UnsupportedServiceWork(value) if value == error)
            );
            let failure = actual.failure.unwrap();
            assert_eq!(failure.reason, n::Reason::UnsupportedServiceWork(error));
            assert_eq!(Some(failure.span), expected.fault);
            assert_eq!(Some(failure.function), expected.function);
            assert_eq!(actual.usage, e::Usage::default());
            assert_eq!(expected.usage, e::Usage::default());
            assert_eq!(actual.total_requests, 0);
            assert_eq!(requests, 0);
            assert!(actual.report.primary().is_none());
            let trace = Rc::new(RefCell::new(Trace::default()));
            let mut allocator =
                a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap();
            let mut instance = c::Instance::default();
            let preview = lang_vm::unit::owned::execute_preview(
                flow,
                failure.function,
                &mut instance,
                Some(&mut allocator),
                control,
                32,
            )
            .err()
            .unwrap();
            assert!(
                matches!(preview.reason, lang_vm::unit::owned::Reason::UnsupportedServiceWork(value) if value == error)
            );
            assert_eq!(preview.usage, e::Usage::default());
            assert_eq!(trace.borrow().requests, 0);
            assert!(!instance.is_poisoned());
        }
    }
}
pub(super) fn vm(
    flow: &m::ownership::failure::Elaborated,
    control: e::Control<'_>,
    interrupt: Option<(usize, Rc<e::InterruptionFlag>)>,
) -> (VmResult, usize) {
    let trace = Rc::new(RefCell::new(Trace {
        interrupt,
        ..Default::default()
    }));
    let mut allocator = a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let result =
        lang_vm::unit::owned::execute_entry(flow, &mut instance, Some(&mut allocator), control);
    assert!(!instance.is_poisoned());
    assert_eq!(allocator.stats().live_allocations, 0);
    let trace = trace.borrow();
    assert_eq!(trace.allocated, trace.released);
    (result, trace.requests)
}
pub(super) fn same(native: &Seen, vm: &VmResult) {
    match vm {
        Ok(vm) => {
            assert!(native.failure.is_none(), "{:?}", native.failure);
            assert_eq!(native.scalar, Some(vm.value));
            assert_eq!(native.usage, vm.usage);
        }
        Err(vm) => {
            let failure = native.failure.as_ref().unwrap();
            let expected = match &vm.reason {
                lang_vm::unit::owned::Reason::Execution(c::FailureKind::Execution(reason)) => {
                    *reason
                }
                lang_vm::unit::owned::Reason::Control(reason) => match reason {
                    lang_vm::unit::Reason::Interrupted => c::ExecutionFailure::Interrupted,
                    lang_vm::unit::Reason::DeadlineExceeded => {
                        c::ExecutionFailure::DeadlineExceeded
                    }
                    lang_vm::unit::Reason::FuelExhausted => c::ExecutionFailure::FuelLimit,
                    lang_vm::unit::Reason::FrameLimit => c::ExecutionFailure::FrameLimit,
                    lang_vm::unit::Reason::LiveCellLimit => c::ExecutionFailure::CellLimit,
                    _ => panic!("unexpected VM control failure: {vm:?}"),
                },
                lang_vm::unit::owned::Reason::Preparation(l::Error::CellLimit { .. }) => {
                    c::ExecutionFailure::CellLimit
                }
                _ => panic!("unexpected VM failure: {vm:?}"),
            };
            assert_eq!(
                failure.reason,
                n::Reason::Runtime(c::Cause::Execution(expected)),
                "{vm:?}"
            );
            assert!(native.scalar.is_none());
            assert_eq!(native.usage, vm.usage);
            assert_eq!(failure.panic_code, vm.panic_code);
            if let Some(span) = vm.fault {
                assert_eq!(failure.span, span);
            }
            if let Some(function) = vm.function {
                assert_eq!(failure.function, function);
            }
            assert_eq!(
                native.report.primary().map(|r| (&r.kind, r.span)),
                vm.report
                    .as_ref()
                    .and_then(|r| r.primary())
                    .map(|r| (&r.kind, r.span))
            );
            assert_eq!(
                native.report.secondary().len(),
                vm.report.as_ref().map_or(0, |r| r.secondary().len())
            );
        }
    }
    assert_eq!(
        native.usage.host_calls, 0,
        "runtime helpers are not user host calls"
    );
}

#[test]
fn native_control_fuel_frame_and_cell_boundaries_match_vm() {
    for text in EXAMPLES {
        let source = source(text);
        let flow = source.failure_flow();
        let types = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let program = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
        let normal = native(
            flow,
            &layouts,
            &program,
            e::Control::with_limits(limits()),
            None,
        );
        let (expected, requests) = vm(flow, e::Control::with_limits(limits()), None);
        same(&normal, &expected);
        assert_eq!(normal.total_requests, requests);
        for fuel in 0..=normal.fuel {
            let control = e::Control::with_limits(e::Limits {
                fuel: Some(fuel),
                ..limits()
            });
            let actual = native(flow, &layouts, &program, control, None);
            let (expected, requests) = vm(flow, control, None);
            same(&actual, &expected);
            assert_eq!(actual.total_requests, requests);
        }
        for frames in [normal.peak - 1, normal.peak] {
            let control = e::Control::with_limits(e::Limits {
                frames: Some(frames),
                ..limits()
            });
            same(
                &native(flow, &layouts, &program, control, None),
                &vm(flow, control, None).0,
            );
        }
        for cells in [normal.peak_cells - 1, normal.peak_cells] {
            let control = e::Control::with_limits(e::Limits {
                live_cells: Some(cells),
                ..limits()
            });
            same(
                &native(flow, &layouts, &program, control, None),
                &vm(flow, control, None).0,
            );
        }
    }
}

#[test]
fn native_control_preentry_polling_and_precedence_match_vm_without_effects() {
    let source = source(EXAMPLES[4]);
    let flow = source.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let program = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    let flag = e::InterruptionFlag::new();
    flag.interrupt();
    let expired = Instant::now() - Duration::from_secs(1);
    let zero = e::Control::with_limits(e::Limits {
        fuel: Some(0),
        frames: Some(0),
        ..limits()
    });
    for control in [
        zero.and_deadline(expired).and_interruption(&flag),
        zero.and_deadline(expired),
        zero,
    ] {
        let actual = native(flow, &layouts, &program, control, None);
        let (expected, requests) = vm(flow, control, None);
        same(&actual, &expected);
        assert_eq!(actual.total_requests, 0);
        assert_eq!(requests, 0);
        assert_eq!(actual.usage, e::Usage::default());
    }
}

#[test]
fn native_control_provider_interruption_prefixes_match_vm_and_retire_owners() {
    for text in [EXAMPLES[3], EXAMPLES[4]] {
        let source = source(text);
        let flow = source.failure_flow();
        let types = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let program = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
        let normal = native(
            flow,
            &layouts,
            &program,
            e::Control::with_limits(limits()),
            None,
        );
        for request in 1..=normal.total_requests {
            let native_flag = Rc::new(e::InterruptionFlag::new());
            let vm_flag = Rc::new(e::InterruptionFlag::new());
            let actual = native(
                flow,
                &layouts,
                &program,
                e::Control::with_limits(limits()).and_interruption(&native_flag),
                Some((request, native_flag.clone())),
            );
            let (expected, requests) = vm(
                flow,
                e::Control::with_limits(limits()).and_interruption(&vm_flag),
                Some((request, vm_flag.clone())),
            );
            same(&actual, &expected);
            assert_eq!(actual.total_requests, requests);
            assert!(actual.report.secondary().is_empty());
        }
    }
}

#[test]
fn native_control_unsupported_prepared_bounds_are_rejected_before_effects() {
    let source = source(EXAMPLES[4]);
    let flow = source.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let program = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    for (setting, limit, requested, capacity) in [
        (
            e::Limits {
                frames: None,
                ..limits()
            },
            n::ControlLimit::Frames,
            None,
            8,
        ),
        (
            e::Limits {
                frames: Some(9),
                ..limits()
            },
            n::ControlLimit::Frames,
            Some(9),
            8,
        ),
        (
            e::Limits {
                live_cells: None,
                ..limits()
            },
            n::ControlLimit::LiveCells,
            None,
            1000,
        ),
        (
            e::Limits {
                live_cells: Some(1001),
                ..limits()
            },
            n::ControlLimit::LiveCells,
            Some(1001),
            1000,
        ),
    ] {
        let actual = native(
            flow,
            &layouts,
            &program,
            e::Control::with_limits(setting),
            None,
        );
        assert_eq!(
            actual.failure.unwrap().reason,
            n::Reason::UnsupportedControl(n::UnsupportedControl {
                limit,
                requested,
                capacity
            })
        );
        assert_eq!(actual.total_requests, 0);
        assert_eq!(actual.usage, e::Usage::default());
        assert!(actual.report.primary().is_none());
    }
}

#[test]
fn shared_control_counter_overflow_and_refused_charges_preserve_usage() {
    let mut usage = e::Usage {
        fuel: u64::MAX,
        host_calls: u64::MAX,
        ..Default::default()
    };
    let initial = usage;
    let unbounded = e::Control::with_limits(e::Limits::unbounded());
    assert_eq!(
        unbounded.charge(&mut usage),
        Err(e::Failure::CounterOverflow)
    );
    assert_eq!(
        unbounded.charge_host_call(&mut usage),
        Err(e::Failure::CounterOverflow)
    );
    assert_eq!(usage, initial);
    let denied = e::Control::with_limits(e::Limits {
        fuel: Some(0),
        host_calls: Some(0),
        ..limits()
    });
    assert_eq!(denied.charge(&mut usage), Err(e::Failure::FuelExhausted));
    assert_eq!(
        denied.charge_host_call(&mut usage),
        Err(e::Failure::HostCallLimit)
    );
    let flag = e::InterruptionFlag::new();
    flag.interrupt();
    let denied = denied
        .and_deadline(Instant::now() - Duration::from_secs(1))
        .and_interruption(&flag);
    assert_eq!(denied.charge(&mut usage), Err(e::Failure::Interrupted));
    assert_eq!(
        denied.charge_host_call(&mut usage),
        Err(e::Failure::Interrupted)
    );
    assert_eq!(usage, initial);
}
