//! Test-only fault injection after real leaf retirement. The provider contract,
//! actual payload cleanup and generated execution path remain unchanged.
use super::*;
use lang_diagnostics::SourceMap;
use lang_semantics::{
    analysis::{self, OwnershipProgram, Program},
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_virtual},
};
use std::path::Path;

mod maps;
mod nested;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Event {
    length: i64,
    span: Option<m::Span>,
    slot: c::Slot,
    reason: c::Reason,
}
#[derive(Default)]
struct Injection {
    events: Vec<Event>,
    fail: Vec<usize>,
    panic: Option<usize>,
    all: bool,
    only_failure: bool,
    interrupt: Option<Rc<crate::InterruptionFlag>>,
}
thread_local! {
    static INJECTION: RefCell<Option<Injection>> = const { RefCell::new(None) };
}
struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        INJECTION.with(|state| {
            state.borrow_mut().take();
        });
    }
}
fn install(injection: Injection) -> Guard {
    INJECTION.with(|state| {
        assert!(state.borrow_mut().replace(injection).is_none());
    });
    Guard
}
fn events() -> Vec<Event> {
    INJECTION.with(|state| state.borrow().as_ref().unwrap().events.clone())
}
pub(super) fn before_leaf<P: a::Provider>(
    token: &l::Token,
    allocator: &a::Allocator<P>,
    context: &c::Context<'_>,
) -> Option<Event> {
    INJECTION.with(|state| {
        state.borrow().as_ref()?;
        let mut bytes = token.bytes;
        let length = if let Ok(value) = buffer::Bytes::from_token(&mut bytes, allocator) {
            value.len(allocator).unwrap()
        } else {
            let mut text = token.bytes;
            buffer::Text::from_token(&mut text, allocator)
                .unwrap()
                .len(allocator)
                .unwrap()
        };
        Some(Event {
            length,
            span: context.span(),
            slot: context.slot(),
            reason: context.reason(),
        })
    })
}
pub(super) fn after_leaf(event: Option<Event>) -> Result<(), u32> {
    let Some(event) = event else {
        return Ok(());
    };
    let (fail, panic) = INJECTION.with(|state| {
        let mut state = state.borrow_mut();
        let state = state.as_mut().unwrap();
        let index = state.events.len();
        state.events.push(event);
        if let Some(flag) = &state.interrupt {
            flag.interrupt();
        }
        (
            (state.all || state.fail.contains(&index))
                && (!state.only_failure || event.reason == c::Reason::Failure),
            state.panic == Some(index),
        )
    });
    assert!(!panic, "injected cleanup panic after real leaf release");
    if fail {
        Err(1000 + u32::try_from(event.length).unwrap())
    } else {
        Ok(())
    }
}
fn checked(source: &str) -> Box<OwnershipProgram> {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("failure.lang"),
        source.as_bytes(),
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let bound = bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
    assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
    let analyzed = analysis::analyze(
        bound.program.unwrap(),
        &sources,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    assert!(
        analyzed.diagnostics.is_empty(),
        "{:?}",
        analyzed.diagnostics
    );
    let Program::Ownership(program) = analyzed.program.unwrap() else {
        panic!("ownership source")
    };
    program
}
const CALLS: &str = include_str!("../../../../../examples/scripts/cleanup_calls.lang");
fn control() -> Control<'static> {
    Control::with_limits(super::super::Limits {
        fuel: Some(100000),
        frames: Some(32),
        ..Default::default()
    })
}

fn lengths() -> Vec<i64> {
    events().iter().map(|event| event.length).collect()
}
fn cleanup_code(event: &Event) -> c::FailureKind {
    c::FailureKind::Cleanup(1000 + u32::try_from(event.length).unwrap())
}
fn assert_records(records: &[c::FailureRecord], expected: &[Event]) {
    assert_eq!(records.len(), expected.len());
    for (record, event) in records.iter().zip(expected) {
        assert_eq!(record.kind, cleanup_code(event));
        assert_eq!(record.span, event.span);
        assert_eq!(record.slot, Some(event.slot));
    }
}
fn names(program: &OwnershipProgram, failure: &Failure) -> Vec<String> {
    let declarations = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    failure
        .stack
        .iter()
        .map(|frame| {
            declarations.functions()[frame.function.0 as usize]
                .name
                .clone()
        })
        .collect()
}
fn assert_empty<P: a::Provider>(allocator: &a::Allocator<P>) {
    let stats = allocator.stats();
    assert_eq!(stats.live_allocations, 0);
    assert_eq!(stats.live_bytes, 0);
    assert_eq!(stats.allocations, stats.deallocations);
}

fn preview_function(program: &OwnershipProgram) -> m::FunctionId {
    m::FunctionId(
        program
            .mir()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .functions()
            .iter()
            .position(|f| f.name == "value")
            .unwrap() as u32,
    )
}

#[test]
fn preview_result_retirement_suppresses_output_and_preserves_failure_precedence() {
    let source = format!(
        "{} fn value() -> Pair {{ return Pair {{ left: make(1), right: make(2) }}; }}",
        CALLS.split("fn leaf").next().unwrap()
    );
    let program = checked(&source);
    for (limit, inject) in [(65536, false), (65536, true), (0, true)] {
        let guard = install(Injection {
            all: inject,
            ..Default::default()
        });
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut instance = c::Instance::default();
        let outcome = execute_preview(
            program.failure_flow(),
            preview_function(&program),
            &mut instance,
            Some(&mut allocator),
            control(),
            limit,
        );
        assert_eq!(lengths(), [2, 1]);
        let events = events();
        assert!(events.iter().all(|event| event.reason
            == if limit == 0 {
                c::Reason::UnpublishedReturn
            } else {
                c::Reason::ScopeExit
            }));
        if !inject {
            assert_eq!(
                outcome.unwrap().value,
                "Pair { left: core::Bytes [65], right: core::Bytes [65, 65] }"
            );
        } else {
            let failure = outcome.err().unwrap();
            let report = failure.report.as_ref().unwrap();
            if limit == 0 {
                assert!(matches!(
                    failure.reason,
                    Reason::Preview(l::PreviewError::Limit)
                ));
                assert_eq!(
                    report.primary().unwrap().kind,
                    c::FailureKind::Execution(c::ExecutionFailure::AllocationLimit)
                );
                assert_records(report.secondary(), &events);
            } else {
                assert!(matches!(
                    failure.reason,
                    Reason::Execution(c::FailureKind::Cleanup(1002))
                ));
                assert_records(
                    std::slice::from_ref(report.primary().unwrap()),
                    &events[..1],
                );
                assert_records(report.secondary(), &events[1..]);
            }
            assert_eq!(report.truncated(), 0);
            assert!(instance.is_poisoned());
            let before = allocator.stats();
            let refused = execute_preview(
                program.failure_flow(),
                preview_function(&program),
                &mut instance,
                Some(&mut allocator),
                control(),
                65536,
            )
            .err()
            .unwrap();
            assert!(matches!(refused.reason, Reason::Poisoned));
            assert_eq!(allocator.stats(), before);
        }
        assert_eq!(instance.is_poisoned(), inject);
        assert_empty(&allocator);
        drop(guard);
    }
}

#[test]
fn ordinary_return_cleanup_failure_precedes_formatting_and_discards_unpublished_result() {
    let source = format!(
        "{} fn value() -> core::Bytes {{ let mut result: core::Bytes = make(1); let mut local: core::Bytes = make(2); return result; }}",
        CALLS.split("fn leaf").next().unwrap()
    );
    let program = checked(&source);
    let _guard = install(Injection {
        all: true,
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let failure = execute_preview(
        program.failure_flow(),
        preview_function(&program),
        &mut instance,
        Some(&mut allocator),
        control(),
        0,
    )
    .err()
    .unwrap();
    assert!(matches!(
        failure.reason,
        Reason::Execution(c::FailureKind::Cleanup(1002))
    ));
    assert_eq!(lengths(), [2, 1]);
    let events = events();
    assert_eq!(events[0].reason, c::Reason::ScopeExit);
    assert_eq!(events[1].reason, c::Reason::UnpublishedReturn);
    let report = failure.report.as_ref().unwrap();
    assert_records(
        std::slice::from_ref(report.primary().unwrap()),
        &events[..1],
    );
    assert_records(report.secondary(), &events[1..]);
    assert!(instance.is_poisoned());
    assert_empty(&allocator);
}

#[test]
fn preview_limit_keeps_primary_and_retires_every_owner_after_report_truncation() {
    let fields = (0..131)
        .map(|n| format!("p{n}: Pair"))
        .collect::<Vec<_>>()
        .join(",");
    let values = (0..131)
        .map(|n| format!("p{n}: Pair {{ left: make(1), right: make(1) }}"))
        .collect::<Vec<_>>()
        .join(",");
    let source = format!(
        "{} struct Many {{ {fields} }} fn value() -> Many {{ return Many {{ {values} }}; }}",
        CALLS.split("fn leaf").next().unwrap()
    );
    let program = checked(&source);
    let _guard = install(Injection {
        all: true,
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(
        a::System,
        a::Limits {
            live_allocations: 65536,
            ..Default::default()
        },
    )
    .unwrap();
    let mut instance = c::Instance::default();
    let failure = execute_preview(
        program.failure_flow(),
        preview_function(&program),
        &mut instance,
        Some(&mut allocator),
        control(),
        0,
    )
    .err()
    .unwrap();
    assert!(
        matches!(failure.reason, Reason::Preview(l::PreviewError::Limit)),
        "{failure:?}"
    );
    assert_eq!(events().len(), 262);
    assert!(
        events()
            .iter()
            .all(|e| e.reason == c::Reason::UnpublishedReturn)
    );
    let report = failure.report.unwrap();
    assert_eq!(
        report.primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::AllocationLimit)
    );
    assert_records(report.secondary(), &events()[..255]);
    assert_eq!(report.truncated(), 7);
    assert!(instance.is_poisoned());
    assert_empty(&allocator);
}

#[test]
fn normal_calls_preserve_reverse_written_nested_order_and_publish_the_result() {
    let program = checked(CALLS);
    let _guard = install(Injection::default());
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let outcome = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 13);
    assert_eq!(lengths(), [4, 1, 2, 3, 8, 5, 7, 6]);
    for event in events() {
        assert_eq!(event.reason, c::Reason::ScopeExit);
        if (1..=3).contains(&event.length) {
            let span = event.span.unwrap();
            assert_eq!(
                &CALLS[span.start as usize..span.end as usize],
                format!("make({})", event.length)
            );
        }
    }
    assert!(!instance.is_poisoned());
    assert_empty(&allocator);
}

#[test]
fn nested_return_cleanup_failure_discards_escrow_and_preserves_one_shared_report() {
    let program = checked(CALLS);
    let _guard = install(Injection {
        all: true,
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let failure = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    let events = events();
    assert_eq!(lengths(), [4, 1, 2, 3, 7, 5, 6]);
    assert_eq!(
        events.iter().map(|e| e.reason).collect::<Vec<_>>(),
        [
            c::Reason::ScopeExit,
            c::Reason::ScopeExit,
            c::Reason::ScopeExit,
            c::Reason::ScopeExit,
            c::Reason::UnpublishedReturn,
            c::Reason::Failure,
            c::Reason::Failure
        ]
    );
    let report = failure.report.as_ref().unwrap();
    assert_records(
        std::slice::from_ref(report.primary().unwrap()),
        &events[..1],
    );
    assert_records(report.secondary(), &events[1..]);
    assert_eq!(report.truncated(), 0);
    assert_eq!(report.quarantined(), 0);
    assert_eq!(failure.fault, events[0].span);
    assert_eq!(names(&program, &failure), ["leaf", "outer", "main"]);
    assert!(instance.is_poisoned());
    assert_empty(&allocator);
    let requests = allocator.stats().requests;
    let refused = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert!(matches!(refused.reason, Reason::Poisoned));
    assert_eq!(allocator.stats().requests, requests);
    assert_eq!(lengths(), [4, 1, 2, 3, 7, 5, 6]);
}

#[test]
fn cleanup_panic_is_contained_after_leaf_release_and_does_not_skip_siblings() {
    let program = checked(CALLS);
    let _guard = install(Injection {
        all: true,
        panic: Some(1),
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let failure = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert_eq!(lengths(), [4, 1, 2, 3, 7, 5, 6]);
    let events = events();
    let report = failure.report.unwrap();
    assert_eq!(
        report.primary().unwrap().kind,
        c::FailureKind::Cleanup(1004)
    );
    assert_eq!(report.secondary()[0].kind, c::FailureKind::CleanupPanic);
    assert_eq!(report.secondary()[0].span, events[1].span);
    assert_records(&report.secondary()[1..], &events[2..]);
    assert_eq!(report.quarantined(), 1);
    assert!(instance.is_poisoned());
    assert_empty(&allocator);
}

#[test]
fn body_panic_stays_primary_ahead_of_nested_cleanup_failures_and_can_otherwise_recover() {
    let source = CALLS.replace("outer(make(7), false)", "outer(make(7), true)");
    let program = checked(&source);
    for inject in [false, true] {
        let guard = install(Injection {
            all: inject,
            ..Default::default()
        });
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut instance = c::Instance::default();
        let failure = execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        assert_eq!(lengths(), [4, 1, 2, 3, 7, 5, 6]);
        assert!(
            events()
                .iter()
                .all(|event| event.reason == c::Reason::Failure)
        );
        assert_eq!(failure.panic_code, Some(-1));
        let fault = failure.fault.unwrap();
        assert!(source[fault.start as usize..fault.end as usize].contains("option_unwrap"));
        assert_eq!(names(&program, &failure), ["leaf", "outer", "main"]);
        let report = failure.report.unwrap();
        assert_eq!(
            report.primary().unwrap().kind,
            c::FailureKind::Execution(c::ExecutionFailure::Panic)
        );
        if inject {
            assert_records(report.secondary(), &events());
        } else {
            assert!(report.secondary().is_empty());
        }
        assert_eq!(instance.is_poisoned(), inject);
        assert_empty(&allocator);
        drop(guard);
        if !inject {
            let success = checked(CALLS);
            assert_eq!(
                execute_entry(
                    success.failure_flow(),
                    &mut instance,
                    Some(&mut allocator),
                    control()
                )
                .unwrap()
                .value,
                13
            );
            assert_empty(&allocator);
        }
    }
}

#[derive(Default)]
struct AllocationTrace {
    attempts: usize,
    fail_at: usize,
    live: std::collections::BTreeSet<usize>,
}
struct Provider(Rc<RefCell<AllocationTrace>>);
struct Region {
    id: usize,
    storage: a::Storage,
}
impl a::Region for Region {
    fn bytes(&self) -> &[u8] {
        self.storage.bytes()
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        self.storage.bytes_mut()
    }
}
impl a::Provider for Provider {
    type Region = Region;
    fn allocate(&mut self, request: a::Request) -> Option<Region> {
        let mut trace = self.0.borrow_mut();
        trace.attempts += 1;
        let id = trace.attempts;
        if trace.fail_at == id {
            return None;
        }
        let storage = a::Storage::try_new(request).ok()?;
        assert!(trace.live.insert(id));
        Some(Region { id, storage })
    }
    fn deallocate(&mut self, region: Region, _: a::Request) {
        assert!(self.0.borrow_mut().live.remove(&region.id));
    }
}
fn allocator() -> (a::Allocator<Provider>, Rc<RefCell<AllocationTrace>>) {
    let trace = Rc::new(RefCell::new(AllocationTrace::default()));
    (
        a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap(),
        trace,
    )
}
fn assert_execution_first(failure: &Failure) {
    let report = failure.report.as_ref().unwrap();
    assert!(matches!(
        report.primary().unwrap().kind,
        c::FailureKind::Execution(_)
    ));
    let failed = events()
        .into_iter()
        .filter(|event| event.reason == c::Reason::Failure)
        .collect::<Vec<_>>();
    assert_records(report.secondary(), &failed);
    assert_eq!(report.truncated(), 0);
    assert_eq!(report.quarantined(), 0);
}

#[test]
fn every_provider_oom_keeps_execution_primary_and_drains_real_owners_with_secondary_failures() {
    let program = checked(CALLS);
    let (mut baseline, trace) = allocator();
    assert_eq!(
        execute_entry(
            program.failure_flow(),
            &mut c::Instance::default(),
            Some(&mut baseline),
            control()
        )
        .unwrap()
        .value,
        13
    );
    let attempts = trace.borrow().attempts;
    for fail_at in 1..=attempts {
        let _guard = install(Injection {
            all: true,
            only_failure: true,
            ..Default::default()
        });
        let (mut allocator, trace) = allocator();
        trace.borrow_mut().fail_at = fail_at;
        let mut instance = c::Instance::default();
        let failure = execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        assert_eq!(trace.borrow().attempts, fail_at);
        assert!(trace.borrow().live.is_empty(), "OOM {fail_at}");
        assert_empty(&allocator);
        assert_execution_first(&failure);
        assert_eq!(
            instance.is_poisoned(),
            events()
                .iter()
                .any(|event| event.reason == c::Reason::Failure)
        );
    }
}

#[test]
fn every_fuel_prefix_keeps_the_original_failure_ahead_of_cleanup_errors() {
    let program = checked(CALLS);
    let (mut baseline, _) = allocator();
    let fuel = execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        control(),
    )
    .unwrap()
    .usage
    .fuel;
    for prefix in 0..fuel {
        let _guard = install(Injection {
            all: true,
            only_failure: true,
            ..Default::default()
        });
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        let control = Control::with_limits(super::super::Limits {
            fuel: Some(prefix),
            ..control().limits()
        });
        let failure = execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control,
        )
        .err()
        .unwrap();
        assert_eq!(failure.usage.fuel, prefix);
        assert_eq!(
            failure.report.as_ref().unwrap().primary().unwrap().kind,
            c::FailureKind::Execution(c::ExecutionFailure::FuelLimit)
        );
        assert_execution_first(&failure);
        assert!(trace.borrow().live.is_empty(), "fuel {prefix}");
        assert_empty(&allocator);
    }
}

#[test]
fn recursive_nested_failure_report_is_bounded_without_skipping_retirement() {
    let make = CALLS.split("fn leaf").next().unwrap();
    let source = format!(
        "{make} fn dive(depth: i64) -> i64 {{ let mut pair: Pair = Pair {{ right: make(2), left: make(1) }}; if depth < 1 {{ let mut missing: core::Option<core::Bytes> = core::Option::None; let mut unreachable: core::Bytes = core::option_unwrap::<core::Bytes>(missing); return 0; }} return dive(depth + (-1)); }} fn main() -> i64 {{ return dive(130); }}"
    );
    let program = checked(&source);
    let _guard = install(Injection {
        all: true,
        only_failure: true,
        ..Default::default()
    });
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let control = Control::with_limits(super::super::Limits {
        frames: Some(256),
        ..control().limits()
    });
    let failure = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control,
    )
    .err()
    .unwrap();
    let events = events();
    assert_eq!(
        events.iter().map(|e| e.length).collect::<Vec<_>>(),
        [1, 2].repeat(131)
    );
    assert!(events.iter().all(|e| e.reason == c::Reason::Failure));
    assert_eq!(failure.stack.len(), 132);
    assert_eq!(failure.panic_code, Some(-1));
    let report = failure.report.unwrap();
    assert_eq!(
        report.primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Panic)
    );
    assert_eq!(report.secondary().len(), 255);
    assert_records(report.secondary(), &events[..255]);
    assert_eq!(report.truncated(), 7);
    assert_eq!(report.quarantined(), 0);
    assert!(instance.is_poisoned());
    assert!(trace.borrow().live.is_empty());
    assert_empty(&allocator);
}

#[test]
fn single_frame_exits_preserve_body_or_cleanup_primary_and_drain_each_owner_once() {
    let mut setup = String::new();
    for (name, length) in [("first", 1), ("second", 2)] {
        setup.push_str(&format!("let mut {name}: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));"));
        for index in 0..length {
            setup.push_str(&format!("let mut pushed_{name}_{index}: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(&mut {name},65));"));
        }
    }
    for body_failure in [false, true] {
        let body = if body_failure {
            "let mut missing: core::Option<core::Bytes> = core::Option::None; let mut unreachable: core::Bytes = core::option_unwrap::<core::Bytes>(missing);"
        } else {
            ""
        };
        let program = checked(&format!("fn main() -> i64 {{ {setup} {body} return 7; }}"));
        for mode in 0..3 {
            let _guard = install(Injection {
                all: mode != 0,
                panic: (mode == 2).then_some(0),
                ..Default::default()
            });
            let (mut allocator, trace) = allocator();
            let mut instance = c::Instance::default();
            let result = execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control(),
            );
            assert_eq!(lengths(), [2, 1]);
            assert_empty(&allocator);
            assert!(trace.borrow().live.is_empty());
            if !body_failure && mode == 0 {
                assert_eq!(result.unwrap().value, 7);
                assert!(!instance.is_poisoned());
                continue;
            }
            let failure = result.err().unwrap();
            assert_eq!(names(&program, &failure), ["main"]);
            let report = failure.report.unwrap();
            let kinds = report
                .secondary()
                .iter()
                .map(|r| r.kind)
                .collect::<Vec<_>>();
            let first = if mode == 2 {
                c::FailureKind::CleanupPanic
            } else {
                c::FailureKind::Cleanup(1002)
            };
            if body_failure {
                assert_eq!(failure.panic_code, Some(-1));
                assert_eq!(
                    report.primary().unwrap().kind,
                    c::FailureKind::Execution(c::ExecutionFailure::Panic)
                );
                assert_eq!(
                    kinds,
                    if mode == 0 {
                        vec![]
                    } else {
                        vec![first, c::FailureKind::Cleanup(1001)]
                    }
                );
                assert!(events().iter().all(|e| e.reason == c::Reason::Failure));
            } else {
                assert_eq!(report.primary().unwrap().kind, first);
                assert_eq!(kinds, [c::FailureKind::Cleanup(1001)]);
            }
            assert_eq!(report.quarantined(), usize::from(mode == 2));
            assert_eq!(instance.is_poisoned(), mode != 0);
        }
    }
}

#[test]
fn interruption_during_cleanup_waits_for_the_drain_and_stops_before_caller_effects() {
    let program = checked(CALLS);
    let flag = Rc::new(crate::InterruptionFlag::default());
    let _guard = install(Injection {
        interrupt: Some(flag.clone()),
        ..Default::default()
    });
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let failure = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control().and_interruption(&flag),
    )
    .err()
    .unwrap();
    assert_eq!(lengths(), [4, 1, 2, 3, 7, 5, 6]);
    let events = events();
    assert!(
        events[..4]
            .iter()
            .all(|event| event.reason == c::Reason::ScopeExit)
    );
    assert!(
        events[4..]
            .iter()
            .all(|event| event.reason == c::Reason::Failure)
    );
    assert_eq!(names(&program, &failure), ["outer", "main"]);
    let report = failure.report.unwrap();
    assert_eq!(
        report.primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Interrupted)
    );
    assert!(report.secondary().is_empty());
    assert!(!instance.is_poisoned());
    assert!(trace.borrow().live.is_empty());
    assert_empty(&allocator);
}

#[test]
fn root_and_field_replacement_publish_before_old_cleanup_and_update_initialization_order() {
    let make = CALLS.split("fn leaf").next().unwrap();
    for field in [false, true] {
        let body = if field {
            "fn change(value: &mut Pair, replacement: core::Bytes) -> core::Unit { (*value).left = replacement; return core::Unit {}; } fn main() -> i64 { let mut value: Pair = Pair { left: make(1), right: make(2) }; let mut done: core::Unit = change(&mut value,make(3)); return core::bytes_len(value.left) + core::bytes_len(value.right); }"
        } else {
            "fn change(value: &mut core::Bytes, replacement: core::Bytes) -> core::Unit { (*value) = replacement; return core::Unit {}; } fn main() -> i64 { let mut first: core::Bytes = make(1); let mut second: core::Bytes = make(2); let mut done: core::Unit = change(&mut first,make(3)); return core::bytes_len(first) + core::bytes_len(second); }"
        };
        let source = format!("{make}{body}");
        let program = checked(&source);
        for fail in [false, true] {
            let _guard = install(Injection {
                all: fail,
                ..Default::default()
            });
            let (mut allocator, trace) = allocator();
            let mut instance = c::Instance::default();
            let result = execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control(),
            );
            assert_eq!(lengths(), [1, 3, 2]);
            let events = events();
            assert_eq!(events[0].reason, c::Reason::Replacement);
            assert!(events[1..].iter().all(|event| event.reason
                == if fail {
                    c::Reason::Failure
                } else {
                    c::Reason::ScopeExit
                }));
            if field {
                let span = events[0].span.unwrap();
                assert_eq!(&source[span.start as usize..span.end as usize], "make(1)");
                let span = events[1].span.unwrap();
                assert_eq!(
                    &source[span.start as usize..span.end as usize],
                    "replacement"
                );
            }
            if fail {
                let error = result.err().unwrap();
                let report = error.report.unwrap();
                assert_records(
                    std::slice::from_ref(report.primary().unwrap()),
                    &events[..1],
                );
                assert_records(report.secondary(), &events[1..]);
                assert!(instance.is_poisoned());
            } else {
                assert_eq!(result.unwrap().value, 5);
                assert!(!instance.is_poisoned());
            }
            assert!(trace.borrow().live.is_empty());
            assert_empty(&allocator);
        }
    }
}

#[test]
fn rhs_failure_discards_the_capture_without_replacing_the_old_field() {
    let make = CALLS.split("fn leaf").next().unwrap();
    let source = format!(
        "{make} fn fail() -> core::Bytes {{ let mut none: core::Option<core::Bytes> = core::Option::None; return core::option_unwrap::<core::Bytes>(none); }} fn main() -> i64 {{ let mut value: Pair = Pair {{ left: make(1), right: make(2) }}; value.left = fail(); return 0; }}"
    );
    let program = checked(&source);
    let _guard = install(Injection {
        all: true,
        ..Default::default()
    });
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let failure = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert_eq!(lengths(), [2, 1]);
    assert!(
        events()
            .iter()
            .all(|event| event.reason == c::Reason::Failure)
    );
    assert_eq!(failure.panic_code, Some(-1));
    assert_execution_first(&failure);
    assert!(instance.is_poisoned());
    assert!(trace.borrow().live.is_empty());
    assert_empty(&allocator);
}

#[test]
fn cleanup_failure_after_real_retirement_runs_the_generated_call_exit() {
    let make = CALLS.split("fn leaf").next().unwrap();
    let source = format!(
        "{make} fn main() -> i64 {{ let mut first: core::Bytes = make(1); let mut second: core::Bytes = make(2); return 7; }}"
    );
    let program = checked(&source);
    let _guard = install(Injection {
        fail: vec![0],
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let failure = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert_eq!(
        events().iter().map(|e| e.length).collect::<Vec<_>>(),
        [2, 1],
        "{failure:?}"
    );
    assert_eq!(
        failure.report.as_ref().unwrap().primary().unwrap().kind,
        c::FailureKind::Cleanup(1002)
    );
    assert!(instance.is_poisoned());
    assert_eq!(allocator.stats().live_allocations, 0);
}

const ARENA: &str = include_str!("../../../../../examples/scripts/arena_owned.lang");
const SHARED: &str = include_str!("../../../../../examples/scripts/shared_owned.lang");
const VECTOR: &str = include_str!("../../../../../examples/scripts/vec_owned.lang");

#[test]
fn vec_source_replacement_pop_and_reverse_elements_keep_real_leaf_initializer_spans() {
    let program = checked(VECTOR);
    let _guard = install(Injection::default());
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let outcome = execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 12);
    assert_eq!(lengths(), [2, 1, 2, 1, 2, 1, 2, 1, 2, 3, 2]);
    assert_eq!(events()[0].reason, c::Reason::Replacement);
    for (event, expected) in events().into_iter().zip([
        "make(2)", "make(1)", "make(2)", "make(1)", "make(2)", "make(1)", "make(2)", "make(1)",
        "make(2)", "make(3)", "make(1)",
    ]) {
        let span = event.span.unwrap();
        assert_eq!(&VECTOR[span.start as usize..span.end as usize], expected);
    }
    assert_empty(&allocator);
}

#[test]
fn vec_source_child_failure_and_panic_drain_all_elements_and_poison_before_reentry() {
    let program = checked(VECTOR);
    for (fail, panic) in [(Some(3), None), (Some(3), Some(7)), (None, Some(3))] {
        let _guard = install(Injection {
            fail: fail.into_iter().collect(),
            panic,
            ..Default::default()
        });
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut instance = c::Instance::default();
        let error = execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        assert_eq!(lengths(), [2, 1, 2, 1, 2, 1, 2, 1, 2, 3, 2]);
        let report = error.report.as_ref().unwrap();
        assert_eq!(
            report.primary().unwrap().kind,
            if fail.is_some() {
                c::FailureKind::Cleanup(1001)
            } else {
                c::FailureKind::CleanupPanic
            }
        );
        if panic == Some(7) {
            assert_eq!(report.secondary()[0].kind, c::FailureKind::CleanupPanic);
        }
        assert!(instance.is_poisoned());
        assert_empty(&allocator);
        let before = allocator.stats();
        assert!(matches!(
            execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control()
            )
            .err()
            .unwrap()
            .reason,
            Reason::Poisoned
        ));
        assert_eq!(allocator.stats(), before);
    }
}

#[test]
fn vec_source_body_panic_keeps_priority_over_descendant_failures() {
    let source=VECTOR.replace("return core::vec_len::<Pair>(values) + width(first) + width(last);","let mut absent: core::Option<i64> = core::Option::None; return core::option_unwrap::<i64>(absent);");
    let program = checked(&source);
    let _guard = install(Injection {
        all: true,
        only_failure: true,
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let error = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    let report = error.report.as_ref().unwrap();
    assert_eq!(
        report.primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Panic)
    );
    assert_eq!(error.panic_code, Some(-1));
    assert_eq!(lengths(), [2, 1, 2, 1, 2, 1, 2, 1, 2, 3, 2]);
    assert_records(report.secondary(), &events()[1..]);
    assert!(instance.is_poisoned());
    assert_empty(&allocator);
}

#[test]
fn vec_source_replacement_cleanup_interruption_stops_later_effects_and_still_drains_all_nodes() {
    let program = checked(VECTOR);
    let flag = Rc::new(crate::InterruptionFlag::new());
    let _guard = install(Injection {
        interrupt: Some(flag.clone()),
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let error = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control().and_interruption(&flag),
    )
    .err()
    .unwrap();
    assert_eq!(
        error.report.as_ref().unwrap().primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Interrupted)
    );
    assert_eq!(lengths(), [2, 1, 2, 1, 2, 1, 2, 1, 2, 3, 2]);
    assert_eq!(events()[1].reason, c::Reason::Failure);
    assert!(!instance.is_poisoned());
    assert_empty(&allocator);
}

#[test]
fn vec_source_cleanup_report_truncation_never_skips_retirement_of_remaining_elements() {
    let program = checked(
        r#"fn main() -> i64 {
        let mut values: core::Vec<core::Bytes> = core::result_unwrap::<core::Vec<core::Bytes>, core::AllocError>(core::vec_new::<core::Bytes>(core::allocator()));
        let mut index: i64 = 0; while index < 260 {
            let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
            let mut added: core::Result<core::Unit, core::AllocError> = core::vec_push::<core::Bytes>(values, bytes); index = index + 1;
        } return 0;
    }"#,
    );
    let _guard = install(Injection {
        all: true,
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let error = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    let report = error.report.as_ref().unwrap();
    assert_eq!(
        report.primary().unwrap().kind,
        c::FailureKind::Cleanup(1000)
    );
    assert_eq!(report.secondary().len(), 255);
    assert_eq!(report.truncated(), 4);
    assert_eq!(lengths(), vec![0; 260]);
    assert!(instance.is_poisoned());
    assert_empty(&allocator);
}

#[test]
fn shared_replacement_rejection_and_reverse_descendant_cleanup_keep_real_initializer_spans() {
    let program = checked(SHARED);
    let _guard = install(Injection::default());
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let outcome = execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 13);
    assert_eq!(lengths(), [1, 0, 2, 1]);
    assert_eq!(events()[0].reason, c::Reason::Replacement);
    for (event, expected) in events()
        .into_iter()
        .zip(["bytes", "rejected_bytes", "bytes", "bytes"])
    {
        let span = event.span.unwrap();
        assert_eq!(&SHARED[span.start as usize..span.end as usize], expected);
    }
    assert_empty(&allocator);
}

#[test]
fn shared_child_cleanup_failure_and_panic_drain_all_tickets_and_poison_before_reentry() {
    let program = checked(SHARED);
    for (fail, panic) in [(Some(2), None), (Some(2), Some(3)), (None, Some(2))] {
        let _guard = install(Injection {
            fail: fail.into_iter().collect(),
            panic,
            ..Default::default()
        });
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut instance = c::Instance::default();
        let failure = execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        assert_eq!(lengths(), [1, 0, 2, 1]);
        let report = failure.report.as_ref().unwrap();
        assert_eq!(
            report.primary().unwrap().kind,
            if fail.is_some() {
                c::FailureKind::Cleanup(1002)
            } else {
                c::FailureKind::CleanupPanic
            }
        );
        if panic == Some(3) {
            assert_eq!(report.secondary()[0].kind, c::FailureKind::CleanupPanic);
        }
        assert!(instance.is_poisoned());
        assert_empty(&allocator);
        let before = allocator.stats();
        assert!(matches!(
            execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control()
            )
            .err()
            .unwrap()
            .reason,
            Reason::Poisoned
        ));
        assert_eq!(allocator.stats(), before);
    }
}

#[test]
fn shared_body_panic_stays_primary_while_real_payload_cleanup_failure_still_releases_every_region()
{
    let program = checked(
        r#"struct Payload { bytes: core::Bytes }
    fn fail(owner: shared Payload) -> i64 { let mut absent: core::Option<i64> = core::Option::None; return core::option_unwrap::<i64>(absent); }
    fn main() -> i64 { let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
        let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(bytes, 65);
        let mut owner: shared Payload = core::result_unwrap::<shared Payload, core::OwnershipError>(core::shared_new::<Payload>(core::allocator(), Payload { bytes: bytes })); return fail(owner); }"#,
    );
    let _guard = install(Injection {
        all: true,
        only_failure: true,
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let failure = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    let report = failure.report.as_ref().unwrap();
    assert_eq!(
        report.primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Panic)
    );
    assert_eq!(failure.panic_code, Some(-1));
    assert_eq!(lengths(), [1]);
    assert_records(report.secondary(), &events());
    assert!(instance.is_poisoned());
    assert_empty(&allocator);
}

#[test]
fn shared_replacement_cleanup_interruption_prevents_later_guest_effects_and_drains_published_nodes()
{
    let program = checked(SHARED);
    let flag = Rc::new(crate::InterruptionFlag::new());
    let _guard = install(Injection {
        interrupt: Some(flag.clone()),
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let failure = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control().and_interruption(&flag),
    )
    .err()
    .unwrap();
    assert_eq!(
        failure.report.as_ref().unwrap().primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Interrupted)
    );
    assert_eq!(lengths(), [1, 2, 1]);
    assert!(!instance.is_poisoned());
    assert_empty(&allocator);
}
#[test]
fn arena_cycle_replacement_and_reverse_teardown_preserve_real_leaf_spans() {
    let program = checked(ARENA);
    let _guard = install(Injection::default());
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let outcome = execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 5);
    assert_eq!(lengths(), [1, 2, 3]);
    assert_eq!(events()[0].reason, c::Reason::Replacement);
    for (event, source) in events().into_iter().zip(["make(1)", "make(2)", "make(3)"]) {
        let span = event.span.unwrap();
        assert_eq!(&ARENA[span.start as usize..span.end as usize], source);
    }
    assert_empty(&allocator);
}

#[test]
fn arena_cleanup_failures_and_panics_drain_the_published_cycle_without_retry() {
    let program = checked(ARENA);
    for (fail, panic) in [(Some(0), None), (Some(1), Some(2)), (None, Some(0))] {
        let _guard = install(Injection {
            fail: fail.into_iter().collect(),
            panic,
            ..Default::default()
        });
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut instance = c::Instance::default();
        let failure = execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .err()
        .unwrap();
        assert_eq!(lengths(), [1, 2, 3]);
        let report = failure.report.as_ref().unwrap();
        let expected = match fail {
            Some(0) => c::FailureKind::Cleanup(1001),
            Some(1) => c::FailureKind::Cleanup(1002),
            _ => c::FailureKind::CleanupPanic,
        };
        assert_eq!(report.primary().unwrap().kind, expected);
        assert!(instance.is_poisoned());
        if fail == Some(1) {
            assert_eq!(report.secondary()[0].kind, c::FailureKind::CleanupPanic);
        }
        assert_empty(&allocator);
    }
}

#[test]
fn every_arena_fuel_prefix_ends_reads_and_keeps_execution_failure_primary() {
    let program = checked(ARENA);
    let mut baseline = a::Allocator::new(a::System, Default::default()).unwrap();
    let fuel = execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut baseline),
        control(),
    )
    .unwrap()
    .usage
    .fuel;
    assert_empty(&baseline);
    for limit in 0..fuel {
        let _guard = install(Injection {
            all: true,
            only_failure: true,
            ..Default::default()
        });
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut instance = c::Instance::default();
        let failure = execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Control::with_limits(super::super::Limits {
                fuel: Some(limit),
                frames: Some(8),
                ..Default::default()
            }),
        )
        .err()
        .unwrap();
        assert_eq!(
            failure.report.as_ref().unwrap().primary().unwrap().kind,
            c::FailureKind::Execution(c::ExecutionFailure::FuelLimit),
            "fuel {limit}: {failure:?}"
        );
        assert_execution_first(&failure);
        assert_empty(&allocator);
    }
}

#[test]
fn arena_teardown_interruption_drains_before_the_callers_next_guest_effect() {
    let inner = ARENA
        .lines()
        .filter(|line| !line.contains("let mut changed:"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("fn main()", "fn inner()");
    let source = format!(
        "{inner}\nfn main() -> i64 {{ let mut value: i64 = inner(); let mut later: core::Bytes = make(5); return value; }}"
    );
    let program = checked(&source);
    let flag = Rc::new(crate::InterruptionFlag::default());
    let _guard = install(Injection {
        interrupt: Some(flag.clone()),
        ..Default::default()
    });
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let failure = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control().and_interruption(&flag),
    )
    .err()
    .unwrap();
    assert_eq!(lengths(), [2, 1]);
    assert_eq!(
        failure.report.as_ref().unwrap().primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Interrupted)
    );
    assert!(!instance.is_poisoned());
    assert!(trace.borrow().live.is_empty());
    assert_empty(&allocator);
}
