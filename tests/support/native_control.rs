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
#[ignore = "observational adapter timings, without a performance threshold"]
fn managed_service_budget_timing_observation() {
    let source = source(include_str!(
        "../../examples/analysis/native_managed_text_work.lang"
    ));
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
    let base = e::Control::with_limits(limits());
    let expected = native(flow, &layouts, &program, base, None).usage;
    for backend in ["vm", "native"] {
        for budget in [None, Some(expected.service_work)] {
            let control = base.and_service_work(budget);
            let mut samples = Vec::new();
            for sample in 0..7 {
                let start = Instant::now();
                for _ in 0..16 {
                    let usage = if backend == "native" {
                        native(flow, &layouts, &program, control, None).usage
                    } else {
                        vm(flow, control, None).0.unwrap().usage
                    };
                    assert_eq!(usage, expected);
                }
                if sample >= 2 {
                    samples.push(start.elapsed().as_nanos() / 16);
                }
            }
            samples.sort_unstable();
            eprintln!(
                "managed-text {backend} budget={budget:?}: median={} ns/invocation, range={}..{}, 5 x 16 samples; prepared adapter execution including per-invocation storage, excluding source/native compilation",
                samples[2], samples[0], samples[4]
            );
        }
    }
}

#[test]
fn finite_managed_work_keeps_guest_panic_and_owner_cleanup() {
    let text = include_str!("../../examples/scripts/shared.lang").replace(
        "return (*value);",
        "return core::option_unwrap::<i64>(core::Option::None);",
    );
    let source = source(&text);
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
    let control = e::Control::with_limits(limits()).and_service_work(Some(9));
    let actual = native(flow, &layouts, &program, control, None);
    let (expected, requests) = vm(flow, control, None);
    same(&actual, &expected);
    assert_eq!(actual.total_requests, requests);
    assert_eq!(actual.usage.service_work, 9);
    assert!(actual.failure.unwrap().panic_code.is_some());
}

#[test]
fn managed_work_refusal_stays_primary_when_cleanup_also_fails() {
    for text in &EXAMPLES[1..] {
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
        let control =
            e::Control::with_limits(limits()).and_service_work(Some(normal.usage.service_work / 2));
        let actual = execute_controlled(
            flow,
            &layouts,
            &program,
            Run {
                inputs: &[],
                fuel: None,
                max_frames: 8,
                cells: 1000,
                refuse: None,
                refuse_cleanup: true,
            },
            control,
            None,
        );
        let failure = actual.failure.unwrap();
        assert!(matches!(
            failure.reason,
            n::Reason::Runtime(c::Cause::Execution(
                c::ExecutionFailure::ServiceWorkLimit { .. }
            ))
        ));
        assert!(matches!(
            actual.report.primary().unwrap().kind,
            c::FailureKind::Execution(c::ExecutionFailure::ServiceWorkLimit { .. })
        ));
        assert!(!actual.report.secondary().is_empty());
        assert!(
            actual
                .report
                .secondary()
                .iter()
                .all(|record| matches!(record.kind, c::FailureKind::Cleanup(_)))
        );
    }
}

pub(super) fn work_profiles() -> Vec<(&'static str, String)> {
    let unused = format!(
        "{}\n{}",
        EXAMPLES[0],
        EXAMPLES[1].replacen("fn main()", "fn unused()", 1)
    );
    [
        ("utf8", EXAMPLES[0]),
        ("vector", EXAMPLES[1]),
        ("map", EXAMPLES[2]),
        ("shared", EXAMPLES[3]),
        ("arena", EXAMPLES[4]),
        ("unused", unused.as_str()),
        (
            "vector-copy",
            include_str!("../../examples/scripts/vec.lang"),
        ),
        (
            "vector-owned",
            include_str!("../../examples/scripts/vec_owned.lang"),
        ),
        (
            "vector-errors",
            include_str!("../../examples/analysis/native_vec_errors.lang"),
        ),
        ("map-copy", include_str!("../../examples/scripts/map.lang")),
        (
            "map-owned",
            include_str!("../../examples/scripts/map_owned.lang"),
        ),
        (
            "map-text",
            include_str!("../../examples/analysis/native_map_text_probe.lang"),
        ),
        (
            "shared-copy",
            include_str!("../../examples/scripts/shared.lang"),
        ),
        (
            "shared-owned",
            include_str!("../../examples/scripts/shared_owned.lang"),
        ),
        (
            "shared-cycle",
            include_str!("../../examples/scripts/nested_cycle.lang"),
        ),
        (
            "weak-expired",
            include_str!("../../examples/analysis/native_weak_expired.lang"),
        ),
        (
            "arena-copy",
            include_str!("../../examples/scripts/arena.lang"),
        ),
        (
            "arena-owned",
            include_str!("../../examples/scripts/arena_owned.lang"),
        ),
        (
            "arena-nested",
            include_str!("../../examples/scripts/nested_arena.lang"),
        ),
        (
            "arena-reuse",
            include_str!("../../examples/analysis/native_arena_reuse.lang"),
        ),
        (
            "managed-text",
            include_str!("../../examples/analysis/native_managed_text_work.lang"),
        ),
        (
            "buffer-bytes",
            include_str!("../../examples/analysis/native_core_bytes.lang"),
        ),
        (
            "buffer-errors",
            include_str!("../../examples/analysis/native_core_errors.lang"),
        ),
        (
            "explicit-drop",
            include_str!("../../examples/analysis/explicit_drop.lang"),
        ),
        (
            "explicit-drop-shared",
            include_str!("../../examples/analysis/explicit_drop_shared.lang"),
        ),
        ("text", include_str!("../../examples/scripts/text.lang")),
    ]
    .into_iter()
    .map(|(label, text)| (label, text.to_owned()))
    .collect()
}

#[test]
fn managed_work_profiles_match_vm_at_finite_boundaries() {
    for (label, text) in work_profiles() {
        let source = source(&text);
        let flow = source.failure_flow();
        assert!(e::service_work_supported(flow));
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
        let configured = e::Control::with_limits(limits());
        let actual = native(flow, &layouts, &program, configured, None);
        let (expected, requests) = vm(flow, configured, None);
        let expected_usage = match &expected {
            Ok(value) => value.usage,
            Err(error) => error.usage,
        };
        assert_eq!(actual.usage, expected_usage, "{label}");
        same(&actual, &expected);
        assert_eq!(actual.total_requests, requests, "{label}");
        assert!(actual.usage.service_work > 0, "{label}");
        let total = actual.usage.service_work;
        for limit in 0..=total + 1 {
            let configured = e::Control::with_limits(limits()).and_service_work(Some(limit));
            let actual = native(flow, &layouts, &program, configured, None);
            let (expected, requests) = vm(flow, configured, None);
            let expected_usage = match &expected {
                Ok(value) => value.usage,
                Err(error) => error.usage,
            };
            assert_eq!(
                actual.usage,
                expected_usage,
                "{label}, limit {limit}: {:?} / {:?}",
                actual.failure,
                expected.as_ref().err()
            );
            same(&actual, &expected);
            assert_eq!(actual.total_requests, requests, "{label}, limit {limit}");
            if let Some(failure) = actual.failure {
                assert!(
                    matches!(
                        failure.reason,
                        n::Reason::Runtime(c::Cause::Execution(
                            c::ExecutionFailure::ServiceWorkLimit { .. }
                        ))
                    ),
                    "{label}, limit {limit}: {failure:?}"
                );
            } else {
                assert!(limit >= total, "{label}, limit {limit}");
            }
        }
        assert_eq!(actual.usage, expected_usage, "{label}");
    }
}
#[test]
fn managed_work_preserves_other_control_and_provider_failure_precedence() {
    for text in &EXAMPLES[1..] {
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
        for work in [normal.usage.service_work / 2, normal.usage.service_work] {
            for fuel in [0, normal.usage.fuel / 2, normal.usage.fuel] {
                let mut bounds = limits();
                bounds.fuel = Some(fuel);
                let control = e::Control::with_limits(bounds).and_service_work(Some(work));
                let actual = native(flow, &layouts, &program, control, None);
                let (expected, requests) = vm(flow, control, None);
                same(&actual, &expected);
                assert_eq!(actual.total_requests, requests);
            }
            // The native fixture numbers guest allocation requests after its
            // prepared storage. Align that index with VM's absolute trace.
            // Admitted work survives OOM; refusal suppresses later effects.
            for request in 1..=normal.total_requests {
                let control = e::Control::with_limits(limits()).and_service_work(Some(work));
                if request <= normal.requests {
                    let actual = execute_controlled(
                        flow,
                        &layouts,
                        &program,
                        Run {
                            inputs: &[],
                            fuel: None,
                            max_frames: 8,
                            cells: 1000,
                            refuse: Some(request),
                            refuse_cleanup: false,
                        },
                        control,
                        None,
                    );
                    let absolute_request = normal.total_requests - normal.requests + request;
                    let (expected, requests) =
                        vm_with_refusal(flow, control, None, Some(absolute_request));
                    same(&actual, &expected);
                    assert_eq!(actual.total_requests, requests);
                }
                let native_flag = Rc::new(e::InterruptionFlag::new());
                let vm_flag = Rc::new(e::InterruptionFlag::new());
                let actual = native(
                    flow,
                    &layouts,
                    &program,
                    control.and_interruption(&native_flag),
                    Some((request, native_flag.clone())),
                );
                let (expected, requests) = vm(
                    flow,
                    control.and_interruption(&vm_flag),
                    Some((request, vm_flag.clone())),
                );
                same(&actual, &expected);
                assert_eq!(actual.total_requests, requests);
            }
        }
    }
}
pub(super) fn vm(
    flow: &m::ownership::failure::Elaborated,
    control: e::Control<'_>,
    interrupt: Option<(usize, Rc<e::InterruptionFlag>)>,
) -> (VmResult, usize) {
    vm_with_refusal(flow, control, interrupt, None)
}
pub(super) fn vm_with_refusal(
    flow: &m::ownership::failure::Elaborated,
    control: e::Control<'_>,
    interrupt: Option<(usize, Rc<e::InterruptionFlag>)>,
    refuse: Option<usize>,
) -> (VmResult, usize) {
    let trace = Rc::new(RefCell::new(Trace {
        interrupt,
        refuse,
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
