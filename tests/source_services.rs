// The enclosing test binary chooses its crate-root service backend. Each
// verified profile is prepared once; every execution has a fresh invocation.
#[allow(
    dead_code,
    reason = "The native test binary supplies its own crate-root adapter."
)]
mod service_backend {
    pub(super) struct Prepared<'p>(&'p lang_mir::unit::ownership::tasks::VerifiedTasks);
    pub(super) fn prepare(proof: &lang_mir::unit::ownership::tasks::VerifiedTasks) -> Prepared<'_> {
        Prepared(proof)
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
            lang_vm::unit::tasks::execute_entry(self.0, allocator, control, limits)
        }
    }
}

use lang_runtime::{allocation as a, execution as e, task::runner as r};
use lang_semantics::analysis::{Program, TaskProgram};
use std::path::Path;
#[path = "support/channel_fixtures.rs"]
#[expect(
    dead_code,
    reason = "Reuse the shared allocator/trace adapter; its channel-only source catalog is exercised by source_channels."
)]
pub(crate) mod channel_fixtures;
use channel_fixtures::channel_allocator;

#[path = "support/service_fixtures.rs"]
#[allow(
    dead_code,
    reason = "Source-only cases use selected catalogs; native and linked suites use the complete catalog."
)]
pub(crate) mod service_fixtures;
use service_fixtures::{SERVICE_COPY_FIXTURES, SERVICE_SHUTDOWN_FIXTURES, SERVICE_STATE_FIXTURES};

fn prepare(text: &str) -> Box<TaskProgram> {
    let out = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("source-services.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let Program::Tasks(program) = out.result.unwrap() else {
        panic!("service task proof")
    };
    program
}
fn fixture() -> Box<TaskProgram> {
    prepare(include_str!("fixtures/task_service.lang"))
}
fn allocator() -> a::Allocator<a::System> {
    a::Allocator::new(a::System, Default::default()).unwrap()
}

#[test]
fn receiver_owned_state_executes_fifo_reads_and_retires_its_nested_queue() {
    let program = prepare(include_str!("fixtures/task_service_owned_state.lang"));
    let execution = crate::service_backend::prepare(program.verified());
    let (mut heap, trace) = channel_allocator(usize::MAX);
    let result = execution.execute(
        &mut heap,
        e::Control::with_limits(Default::default()),
        r::Limits {
            trace_events: 2048,
            ..Default::default()
        },
    );
    assert_eq!(
        heap.stats().live_allocations,
        0,
        "{:?}",
        result.as_ref().err()
    );
    assert!(trace.borrow().live.is_empty());
    assert_eq!(result.unwrap().value, 42);
    let mut buffers = trace.borrow().buffers.clone();
    buffers.sort_unstable();
    assert_eq!(buffers, [3, 7, 20, 22]);
}

#[test]
fn publication_follows_actual_service_state_and_returns_a_rejected_recursive_owner() {
    let program = prepare(include_str!("fixtures/task_service_state_cycle.lang"));
    let execution = crate::service_backend::prepare(program.verified());
    let (mut heap, trace) = channel_allocator(usize::MAX);
    let result = execution.execute(
        &mut heap,
        e::Control::with_limits(Default::default()),
        Default::default(),
    );
    assert_eq!(
        heap.stats().live_allocations,
        0,
        "{:?}",
        result.as_ref().err()
    );
    assert!(trace.borrow().live.is_empty());
    assert_eq!(result.unwrap().value, 42);
    assert_eq!(trace.borrow().buffers, [7]);
}

#[test]
fn resumed_sends_reinspect_receivers_queued_under_service_state_after_wakeup() {
    let program = prepare(include_str!("fixtures/task_service_state_reinspect.lang"));
    let execution = crate::service_backend::prepare(program.verified());
    let (mut heap, trace) = channel_allocator(usize::MAX);
    let result = execution.execute(
        &mut heap,
        e::Control::with_limits(Default::default()),
        r::Limits {
            trace_events: 4096,
            ..Default::default()
        },
    );
    assert_eq!(
        heap.stats().live_allocations,
        0,
        "{:?}",
        result.as_ref().err()
    );
    assert!(trace.borrow().live.is_empty());
    let result = result.unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(trace.borrow().buffers, [7]);
    assert!(
        result
            .events
            .iter()
            .filter(|event| event.task == 1 && event.kind == r::EventKind::Resume)
            .count()
            >= 2
    );
}

#[test]
fn whole_replacement_moves_nested_service_owners_and_vector_held_receivers_once() {
    let program = prepare(include_str!("fixtures/task_service_state_replace.lang"));
    let execution = crate::service_backend::prepare(program.verified());
    let (mut heap, trace) = channel_allocator(usize::MAX);
    let result = execution.execute(
        &mut heap,
        e::Control::with_limits(Default::default()),
        Default::default(),
    );
    assert_eq!(
        heap.stats().live_allocations,
        0,
        "{:?}",
        result.as_ref().err()
    );
    assert!(trace.borrow().live.is_empty());
    assert_eq!(result.unwrap().value, 42);
    assert_eq!(trace.borrow().buffers, [7, 107, 22, 20, 9, 109]);
}

#[test]
fn shared_weak_service_handles_keep_checked_authority_and_phantom_receiver_parameters() {
    let program = prepare(include_str!(
        "fixtures/task_service_state_shared_handle.lang"
    ));
    let execution = crate::service_backend::prepare(program.verified());
    let (mut heap, trace) = channel_allocator(usize::MAX);
    let result = execution.execute(
        &mut heap,
        e::Control::with_limits(Default::default()),
        Default::default(),
    );
    assert_eq!(
        heap.stats().live_allocations,
        0,
        "{:?}",
        result.as_ref().err()
    );
    assert!(trace.borrow().live.is_empty());
    assert_eq!(result.unwrap().value, 42);
    assert!(trace.borrow().buffers.is_empty());
}

#[test]
fn recursive_service_state_retirement_is_iterative_and_keeps_nested_headers_live() {
    let program = prepare(include_str!("fixtures/task_service_state_deep.lang"));
    let execution = crate::service_backend::prepare(program.verified());
    let (mut heap, trace) = channel_allocator(usize::MAX);
    let result = execution.execute(
        &mut heap,
        e::Control::with_limits(Default::default()),
        r::Limits {
            channels: lang_runtime::channel::Limits {
                channels: 64,
                slots: 64,
                handles: 0,
                waiters: 0,
            },
            ..Default::default()
        },
    );
    assert_eq!(
        heap.stats().live_allocations,
        0,
        "{:?}",
        result.as_ref().err()
    );
    assert!(trace.borrow().live.is_empty());
    assert_eq!(result.unwrap().value, 42);
    assert_eq!(trace.borrow().buffers, (0..64).rev().collect::<Vec<_>>());
}

#[test]
fn owned_receiver_state_quotas_refuse_one_below_exact_and_keep_real_inventory() {
    let program = prepare(include_str!("fixtures/task_service_owned_state.lang"));
    let execution = crate::service_backend::prepare(program.verified());
    let exact = lang_runtime::channel::Limits {
        channels: 2,
        slots: 2,
        handles: 2,
        waiters: 2,
    };
    for kind in 0..=4 {
        let mut channels = exact;
        match kind {
            0 => channels.channels -= 1,
            1 => channels.slots -= 1,
            2 => channels.handles -= 1,
            3 => channels.waiters -= 1,
            _ => {}
        }
        let (mut heap, trace) = channel_allocator(usize::MAX);
        let result = execution.execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            r::Limits {
                channels,
                ..Default::default()
            },
        );
        assert_eq!(
            heap.stats().live_allocations,
            0,
            "quota {kind}: {:?}",
            result.as_ref().err()
        );
        assert!(trace.borrow().live.is_empty(), "quota {kind}");
        if kind == 4 {
            assert_eq!(result.unwrap().value, 42);
        } else {
            use lang_runtime::channel::{Error as C, managed::Error as G};
            let failure = result.err().expect("one below quota");
            assert!(
                matches!(
                    (kind, &failure.reason),
                    (
                        0,
                        r::Reason::Channel(G::Protocol(C::ChannelLimit { limit: 1 }))
                    ) | (
                        1,
                        r::Reason::Channel(G::Protocol(C::SlotLimit { limit: 1 }))
                    ) | (
                        2,
                        r::Reason::Channel(G::Protocol(C::HandleLimit { limit: 1 }))
                    ) | (
                        3,
                        r::Reason::Channel(G::Protocol(C::WaiterLimit { limit: 1 }))
                    )
                ),
                "quota {kind}: {failure:?}"
            );
        }
    }
}

#[test]
fn nested_owner_state_cleanup_keeps_primary_panics_and_continues_after_truncation() {
    let fixture = include_str!("fixtures/task_service_state_replace.lang");
    for primary in [false, true] {
        let source = if primary {
            format!(
                "{}return core::panic::<i64>(91);}}\nasync fn main{}",
                fixture
                    .split("    let mut second:core::Bytes=")
                    .next()
                    .unwrap(),
                fixture.split("async fn main").nth(1).unwrap()
            )
        } else {
            fixture.to_owned()
        };
        let program = prepare(&source);
        let execution = crate::service_backend::prepare(program.verified());
        for records in [1, 8] {
            let (mut heap, trace) = channel_allocator(usize::MAX);
            trace.borrow_mut().panic_buffers = vec![9, 109];
            let failure = execution
                .execute(
                    &mut heap,
                    e::Control::with_limits(Default::default()),
                    r::Limits {
                        cleanup_failure_records: records,
                        ..Default::default()
                    },
                )
                .err()
                .expect("nested cleanup panics");
            if primary {
                assert!(
                    matches!(failure.reason, r::Reason::Panic(Some(91))),
                    "{failure:?}"
                );
            } else {
                assert!(matches!(failure.reason, r::Reason::Cleanup), "{failure:?}");
            }
            let report = failure.report.as_ref().unwrap();
            assert!(failure.poisoned);
            assert_eq!(report.cleanup_failures(), 2);
            if records == 1 {
                assert!(report.secondary().is_empty());
                assert!(report.truncated() > 0);
            }
            let buffers = &trace.borrow().buffers;
            let at = |byte| buffers.iter().position(|value| *value == byte).unwrap();
            assert!(at(7) < at(107));
            assert!(at(9) < at(109));
            let mut sorted = buffers.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, [7, 9, 20, 22, 107, 109]);
            assert_eq!(heap.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
            assert_eq!(failure.tasks.live, 0);
        }
    }
}

#[test]
fn copy_service_message_quotas_keep_queued_and_uncommitted_storage_accounted() {
    let program = prepare(&channel_fixtures::copy_quota_source(true));
    let execution = crate::service_backend::prepare(program.verified());
    let exact = lang_runtime::channel::Limits {
        channels: 1,
        slots: 1,
        handles: 2,
        waiters: 1,
    };
    for kind in 0..=4 {
        let mut channels = exact;
        match kind {
            0 => channels.channels = 0,
            1 => channels.slots = 0,
            2 => channels.handles = 1,
            3 => channels.waiters = 0,
            _ => {}
        }
        let (mut heap, trace) = channel_allocator(usize::MAX);
        let result = execution.execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            r::Limits {
                channels,
                ..Default::default()
            },
        );
        assert_eq!(
            result.is_ok(),
            kind == 4,
            "quota {kind}: {:?}",
            result.as_ref().err()
        );
        if kind < 4 {
            use lang_runtime::channel::{Error as C, managed::Error as G};
            let failure = result.as_ref().err().expect("quota refusal");
            assert!(
                matches!(
                    (kind, &failure.reason),
                    (
                        0,
                        r::Reason::Channel(G::Protocol(C::ChannelLimit { limit: 0 }))
                    ) | (
                        1,
                        r::Reason::Channel(G::Protocol(C::SlotLimit { limit: 0 }))
                    ) | (
                        2,
                        r::Reason::Channel(G::Protocol(C::HandleLimit { limit: 1 }))
                    ) | (
                        3,
                        r::Reason::Channel(G::Protocol(C::WaiterLimit { limit: 0 }))
                    )
                ),
                "quota {kind}: {failure:?}"
            );
        }
        assert_eq!(heap.stats().live_allocations, 0, "quota {kind}");
        assert!(trace.borrow().live.is_empty(), "quota {kind}");
        if let Ok(outcome) = result {
            assert_eq!(outcome.value, 42);
        }
    }
}

#[test]
fn state_and_message_owners_survive_fifo_backpressure_and_helper_read_loans() {
    let program = fixture();
    let execution = crate::service_backend::prepare(program.verified());
    let mut allocator = allocator();
    let outcome = execution
        .execute(
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 2048,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.value, 42);
    assert_eq!(outcome.tasks.live, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(
        outcome
            .events
            .iter()
            .filter(|event| event.task == 0 && event.kind == r::EventKind::Resume)
            .count()
            >= 2,
        "the second send must park and resume"
    );
}

#[test]
fn every_finite_work_and_fuel_prefix_drains_service_state_and_inflight_payloads() {
    let program = fixture();
    let execution = crate::service_backend::prepare(program.verified());
    let mut heap = allocator();
    let baseline = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()).and_service_work(Some(u64::MAX)),
            Default::default(),
        )
        .unwrap();
    assert_eq!(baseline.value, 42);
    assert_eq!(heap.stats().live_allocations, 0);
    for work in 0..=baseline.usage.service_work {
        let mut heap = allocator();
        let result = execution.execute(
            &mut heap,
            e::Control::with_limits(Default::default()).and_service_work(Some(work)),
            Default::default(),
        );
        assert_eq!(
            heap.stats().live_allocations,
            0,
            "work {work}: {:?}",
            result.as_ref().err()
        );
        if work < baseline.usage.service_work {
            let error = result.err().expect("work prefix must fail");
            assert!(
                matches!(error.reason,r::Reason::Control(e::Failure::ServiceWorkLimit {limit,..}) if limit==work),
                "{work}: {error:?}"
            );
        } else {
            assert_eq!(result.unwrap().value, 42);
        }
    }
    for fuel in 0..=baseline.usage.fuel {
        let mut heap = allocator();
        let result = execution.execute(
            &mut heap,
            e::Control::with_limits(e::Limits {
                fuel: Some(fuel),
                ..Default::default()
            }),
            Default::default(),
        );
        assert_eq!(
            heap.stats().live_allocations,
            0,
            "fuel {fuel}: {:?}",
            result.as_ref().err()
        );
        if fuel < baseline.usage.fuel {
            assert!(matches!(
                result.err().expect("fuel prefix must fail").reason,
                r::Reason::Control(e::Failure::FuelExhausted)
            ));
        } else {
            assert_eq!(result.unwrap().value, 42);
        }
    }
}

#[test]
fn state_storage_counts_against_allocation_and_live_cell_limits() {
    let program = fixture();
    let execution = crate::service_backend::prepare(program.verified());
    let mut heap = allocator();
    let baseline = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
    let peak = heap.stats().peak_live_allocations;
    for allocations in 0..=peak {
        let mut heap = a::Allocator::new(
            a::System,
            a::Limits {
                live_allocations: allocations,
                ..Default::default()
            },
        )
        .unwrap();
        let result = execution.execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            Default::default(),
        );
        assert_eq!(
            heap.stats().live_allocations,
            0,
            "allocations {allocations}: {:?}",
            result.as_ref().err()
        );
        if allocations < peak {
            assert!(result.is_err(), "allocation prefix {allocations}");
        } else {
            assert_eq!(result.unwrap().value, 42);
        }
    }
    for cells in 0..=baseline.usage.peak_live_cells {
        let mut heap = allocator();
        let result = execution.execute(
            &mut heap,
            e::Control::with_limits(e::Limits {
                live_cells: Some(cells),
                ..Default::default()
            }),
            Default::default(),
        );
        assert_eq!(
            heap.stats().live_allocations,
            0,
            "cells {cells}: {:?}",
            result.as_ref().err()
        );
        if cells < baseline.usage.peak_live_cells {
            assert!(result.is_err(), "cells {cells}");
        } else {
            assert_eq!(result.unwrap().value, 42);
        }
    }
}

#[test]
fn trace_prefixes_release_retained_capture_and_state_authority() {
    let program = fixture();
    let execution = crate::service_backend::prepare(program.verified());
    let mut heap = allocator();
    let baseline = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 2048,
                ..Default::default()
            },
        )
        .unwrap();
    for trace in 1..=baseline.events.len() {
        let mut heap = allocator();
        let result = execution.execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: trace,
                ..Default::default()
            },
        );
        assert_eq!(
            heap.stats().live_allocations,
            0,
            "trace {trace}: {:?}",
            result.as_ref().err()
        );
        if trace < baseline.events.len() {
            assert!(matches!(
                result.err().expect("trace prefix must fail").reason,
                r::Reason::TraceLimit
            ));
        } else {
            assert_eq!(result.unwrap().value, 42);
        }
    }
}

#[test]
fn non_share_state_metadata_is_admitted_by_private_service_task_adapters() {
    for text in [
        "fn carry(owner:core::ServiceOwner<core::Receiver<i64>,i64>)->core::ServiceOwner<core::Receiver<i64>,i64>{return owner;}async fn main()->i64{yield;return 42;}",
        "fn carry(owner:core::ServiceOwner<core::Vec<core::Receiver<i64>>,i64>)->core::ServiceOwner<core::Vec<core::Receiver<i64>>,i64>{return owner;}async fn main()->i64{yield;return 42;}",
    ] {
        let program = prepare(text);
        let execution = crate::service_backend::prepare(program.verified());
        let mut heap = allocator();
        let outcome = execution
            .execute(
                &mut heap,
                e::Control::with_limits(Default::default()),
                Default::default(),
            )
            .unwrap();
        assert_eq!(outcome.value, 42);
        assert_eq!(heap.stats().live_allocations, 0);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let outcome = native
            .execute(
                &mut heap,
                e::Control::with_limits(Default::default()),
                Default::default(),
            )
            .unwrap();
        assert_eq!(outcome.value, 42);
        assert_eq!(heap.stats().live_allocations, 0);
    }
}

#[test]
fn idle_read_and_replacement_preserve_state_and_consume_rejected_candidates() {
    let helpers = include_str!("fixtures/task_service.lang")
        .split("fn add(")
        .next()
        .unwrap();
    let text = format!(
        "{helpers}{}",
        r#"
fn code(error:core::ServiceError)->i64 {
    match error {
        core::ServiceError::InvalidCapacity=>{return 2;},
        core::ServiceError::Allocation(detail)=>{return 3;},
        core::ServiceError::Unavailable=>{return 4;},
        core::ServiceError::NoActiveHandler=>{return 1;},
        core::ServiceError::InvalidPublication=>{return 5;},
    }
}
async fn main()->i64 {
    let mut score:i64=0;
    match core::service_new::<core::Bytes,i64>(core::allocator(),0,bytes(99)) {
        core::Result::Ok(invalid_setup)=>{return core::panic::<i64>(-7);},
        core::Result::Err(capacity_error)=>{score=score+code(capacity_error);},
    }
    let mut setup:core::ServiceSetup<core::Bytes,i64>=core::result_unwrap::<core::ServiceSetup<core::Bytes,i64>,core::ServiceError>(core::service_new::<core::Bytes,i64>(core::allocator(),1,bytes(9)));
    let mut handle:core::ServiceHandle<i64>=core::service_handle::<core::Bytes,i64>(&setup);
    let mut clone:core::ServiceHandle<i64>=core::service_clone::<i64>(&handle);
    let mut owner:core::ServiceOwner<core::Bytes,i64>=core::service_owner::<core::Bytes,i64>(setup);
    match core::service_read::<core::Bytes,i64>(&owner) {
        core::Result::Ok(idle_state)=>{return core::panic::<i64>(-7);},
        core::Result::Err(idle_error)=>{score=score+code(idle_error);},
    }
    match core::service_replace::<core::Bytes,i64>(&mut owner,bytes(77)) {
        core::Result::Ok(replaced_unit)=>{return core::panic::<i64>(-7);},
        core::Result::Err(replacement_error)=>{score=score+code(replacement_error);},
    }
    let mut sent:core::SendResult<i64>=await core::service_send::<i64>(&clone,28);
    let mut received:i64=core::option_unwrap::<i64>(await core::service_receive::<core::Bytes,i64>(&mut owner));
    match core::service_read::<core::Bytes,i64>(&owner) {
        core::Result::Ok(state)=>{score=score+core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(state,0));},
        core::Result::Err(read_error)=>{return core::panic::<i64>(-7);},
    }
    let mut finished:core::Unit=core::service_finish::<core::Bytes,i64>(&mut owner);
    match core::service_read::<core::Bytes,i64>(&owner) {
        core::Result::Ok(finished_state)=>{return core::panic::<i64>(-7);},
        core::Result::Err(finish_error)=>{score=score+code(finish_error);},
    }
    return received+score;
}
"#
    );
    let program = prepare(&text);
    let execution = crate::service_backend::prepare(program.verified());
    let mut heap = allocator();
    let result = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(heap.stats().live_allocations, 0);
}

#[test]
fn idle_cancellation_closes_state_and_recovers_owned_messages_with_live_identities() {
    let program = prepare(include_str!("fixtures/task_service_cancel.lang"));
    let execution = crate::service_backend::prepare(program.verified());
    let mut heap = allocator();
    let outcome = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
    assert_eq!(outcome.value, 42);
    assert_eq!(heap.stats().live_allocations, 0);
}

#[test]
fn active_cancellation_preserves_reads_and_replacement_until_boundary_and_recovers_parked_send() {
    let program = prepare(include_str!("fixtures/task_service_active_cancel.lang"));
    let execution = crate::service_backend::prepare(program.verified());
    let mut heap = allocator();
    let outcome = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 2048,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.value, 42);
    assert!(
        outcome
            .events
            .iter()
            .filter(|event| event.task == 1 && event.kind == r::EventKind::Resume)
            .count()
            >= 3,
        "the producer must park twice and recover the last payload"
    );
    assert_eq!(heap.stats().live_allocations, 0);
}

#[test]
fn idle_cancellation_wakes_parked_receive_and_keeps_closed_owner_header_alive() {
    let helpers = include_str!("fixtures/task_service_cancel.lang")
        .split("async fn main")
        .next()
        .unwrap();
    let text = format!(
        "{helpers}{}",
        r#"
async fn worker(input:core::ServiceOwner<State,core::Bytes>)->i64 {
    let mut owner:core::ServiceOwner<State,core::Bytes>=input;
    match await core::service_receive::<State,core::Bytes>(&mut owner) {
        core::Option::Some(unexpected)=>{return core::panic::<i64>(-10);},
        core::Option::None=>{},
    }
    match core::service_read::<State,core::Bytes>(&owner) {
        core::Result::Ok(unexpected_state)=>{return core::panic::<i64>(-10);},
        core::Result::Err(closed_error)=>{let mut checked:core::Unit=unavailable(closed_error);},
    }
    let mut finished:core::Unit=core::service_finish::<State,core::Bytes>(&mut owner);
    return 42;
}
async fn main()->i64 {
    let mut setup:core::ServiceSetup<State,core::Bytes>=core::result_unwrap::<core::ServiceSetup<State,core::Bytes>,core::ServiceError>(core::service_new::<State,core::Bytes>(core::allocator(),1,State {marker:bytes(7)}));
    let mut handle:core::ServiceHandle<core::Bytes>=core::service_handle::<State,core::Bytes>(&setup);
    let mut owner:core::ServiceOwner<State,core::Bytes>=core::service_owner::<State,core::Bytes>(setup);
    let mut serving:core::Task<i64>=spawn worker(owner);
    yield;
    let mut requested:core::Unit=core::service_cancel::<core::Bytes>(&handle);
    return core::result_unwrap::<i64,core::TaskError>(await serving);
}
"#
    );
    let program = prepare(&text);
    let execution = crate::service_backend::prepare(program.verified());
    let mut heap = allocator();
    let outcome = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 2048,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.value, 42);
    assert!(
        outcome
            .events
            .iter()
            .filter(|event| event.task == 1 && event.kind == r::EventKind::Resume)
            .count()
            >= 2
    );
    assert_eq!(heap.stats().live_allocations, 0);
}

fn closed_read(name: &str) -> String {
    format!(
        "match core::service_read::<State,core::Bytes>(&owner) {{ core::Result::Ok(unexpected_{name})=>{{return core::panic::<i64>(-10);}}, core::Result::Err({name})=>{{let mut checked_{name}:core::Unit=unavailable({name});}}, }}"
    )
}

#[test]
fn last_producer_closure_drains_accepted_messages_then_state_at_finish_or_next_receive() {
    let helpers = include_str!("fixtures/task_service_cancel.lang")
        .split("async fn main")
        .next()
        .unwrap();
    for explicit in [false, true] {
        let boundary = if explicit {
            format!(
                "let mut finished:core::Unit=core::service_finish::<State,core::Bytes>(&mut owner);{}",
                closed_read("finish_error")
            )
        } else {
            String::new()
        };
        let after = closed_read("closed_error");
        let text = format!(
            r#"{helpers}
async fn producer(handle:core::ServiceHandle<core::Bytes>)->i64 {{
    let mut sent:core::SendResult<core::Bytes>=await core::service_send::<core::Bytes>(&handle,bytes(42));
    return 0;
}}
async fn main()->i64 {{
    let mut setup:core::ServiceSetup<State,core::Bytes>=core::result_unwrap::<core::ServiceSetup<State,core::Bytes>,core::ServiceError>(core::service_new::<State,core::Bytes>(core::allocator(),1,State {{marker:bytes(7)}}));
    let mut handle:core::ServiceHandle<core::Bytes>=core::service_handle::<State,core::Bytes>(&setup);
    let mut owner:core::ServiceOwner<State,core::Bytes>=core::service_owner::<State,core::Bytes>(setup);
    let mut sending:core::Task<i64>=spawn producer(handle);
    let mut joined:i64=core::result_unwrap::<i64,core::TaskError>(await sending);
    let mut received:core::Bytes=core::option_unwrap::<core::Bytes>(await core::service_receive::<State,core::Bytes>(&mut owner));
    let mut marker:i64=0;
    match core::service_read::<State,core::Bytes>(&owner) {{
        core::Result::Ok(state)=>{{marker=core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(state.marker,0));}},
        core::Result::Err(read_error)=>{{return core::panic::<i64>(-10);}},
    }}
    {boundary}
    match await core::service_receive::<State,core::Bytes>(&mut owner) {{
        core::Option::Some(unexpected)=>{{return core::panic::<i64>(-10);}},
        core::Option::None=>{{}},
    }}
    {after}
    let mut repeated_finish:core::Unit=core::service_finish::<State,core::Bytes>(&mut owner);
    return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(&received,0))+marker+ -7;
}}
"#
        );
        let program = prepare(&text);
        let execution = crate::service_backend::prepare(program.verified());
        let mut heap = allocator();
        let outcome = execution
            .execute(
                &mut heap,
                e::Control::with_limits(Default::default()),
                Default::default(),
            )
            .unwrap();
        assert_eq!(outcome.value, 42, "explicit finish {explicit}");
        assert_eq!(heap.stats().live_allocations, 0);
    }
}

#[test]
fn last_producer_closure_wakes_empty_receive_and_retains_owner_until_drop() {
    let helpers = include_str!("fixtures/task_service_cancel.lang")
        .split("async fn main")
        .next()
        .unwrap();
    let after = closed_read("closed_error");
    let text = format!(
        r#"{helpers}
async fn producer(handle:core::ServiceHandle<core::Bytes>)->i64 {{return 42;}}
async fn main()->i64 {{
    let mut setup:core::ServiceSetup<State,core::Bytes>=core::result_unwrap::<core::ServiceSetup<State,core::Bytes>,core::ServiceError>(core::service_new::<State,core::Bytes>(core::allocator(),1,State {{marker:bytes(7)}}));
    let mut handle:core::ServiceHandle<core::Bytes>=core::service_handle::<State,core::Bytes>(&setup);
    let mut owner:core::ServiceOwner<State,core::Bytes>=core::service_owner::<State,core::Bytes>(setup);
    let mut sending:core::Task<i64>=spawn producer(handle);
    match await core::service_receive::<State,core::Bytes>(&mut owner) {{
        core::Option::Some(unexpected)=>{{return core::panic::<i64>(-10);}},
        core::Option::None=>{{}},
    }}
    {after}
    let mut finished:core::Unit=core::service_finish::<State,core::Bytes>(&mut owner);
    return core::result_unwrap::<i64,core::TaskError>(await sending);
}}
"#
    );
    let program = prepare(&text);
    let execution = crate::service_backend::prepare(program.verified());
    let mut heap = allocator();
    let outcome = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 2048,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.value, 42);
    assert!(
        outcome
            .events
            .iter()
            .filter(|event| event.task == 0 && event.kind == r::EventKind::Resume)
            .count()
            >= 2
    );
    assert_eq!(heap.stats().live_allocations, 0);
}

#[test]
fn cancel_before_claiming_owner_and_after_owner_drop_keeps_handle_identity_valid() {
    let helpers = include_str!("fixtures/task_service_cancel.lang")
        .split("async fn main")
        .next()
        .unwrap();
    let after = closed_read("closed_error");
    let text = format!(
        r#"{helpers}
fn discard(owner:core::ServiceOwner<State,core::Bytes>)->core::Unit {{return core::Unit {{}};}}
async fn main()->i64 {{
    let mut setup:core::ServiceSetup<State,core::Bytes>=core::result_unwrap::<core::ServiceSetup<State,core::Bytes>,core::ServiceError>(core::service_new::<State,core::Bytes>(core::allocator(),1,State {{marker:bytes(7)}}));
    let mut handle:core::ServiceHandle<core::Bytes>=core::service_handle::<State,core::Bytes>(&setup);
    let mut requested:core::Unit=core::service_cancel::<core::Bytes>(&handle);
    let mut owner:core::ServiceOwner<State,core::Bytes>=core::service_owner::<State,core::Bytes>(setup);
    {after}
    let mut discarded:core::Unit=discard(owner);
    let mut repeated:core::Unit=core::service_cancel::<core::Bytes>(&handle);
    let mut clone:core::ServiceHandle<core::Bytes>=core::service_clone::<core::Bytes>(&handle);
    let mut clone_request:core::Unit=core::service_cancel::<core::Bytes>(&clone);
    return 42;
}}
"#
    );
    let program = prepare(&text);
    let execution = crate::service_backend::prepare(program.verified());
    let mut heap = allocator();
    let outcome = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
    assert_eq!(outcome.value, 42);
    assert_eq!(heap.stats().live_allocations, 0);
}

#[test]
fn cancellation_retires_committed_queue_before_state_and_never_duplicates_pending_or_received_owners()
 {
    for (name, text, expected) in [
        (
            "idle",
            include_str!("fixtures/task_service_cancel.lang"),
            &[20, 7, 99, 42][..],
        ),
        (
            "active",
            include_str!("fixtures/task_service_active_cancel.lang"),
            &[7, 42, 22, 9, 20][..],
        ),
    ] {
        let program = prepare(text);
        let execution = crate::service_backend::prepare(program.verified());
        let (mut heap, trace) = channel_allocator(usize::MAX);
        let outcome = execution
            .execute(
                &mut heap,
                e::Control::with_limits(Default::default()),
                Default::default(),
            )
            .unwrap();
        assert_eq!(outcome.value, 42, "{name}");
        assert_eq!(trace.borrow().buffers, expected, "{name}");
        assert!(trace.borrow().live.is_empty());
        assert_eq!(heap.stats().live_allocations, 0);
    }
}

#[test]
fn cancellation_cleanup_continues_in_order_after_panics_and_report_truncation() {
    for active in [false, true] {
        for primary in [false, true] {
            let text = if active {
                include_str!("fixtures/task_service_active_cancel.lang")
            } else {
                include_str!("fixtures/task_service_cancel.lang")
            };
            let text = if primary {
                if active {
                    format!("{}return core::panic::<i64>(91);}}\nasync fn main{}",text.split("    match await core::service_receive::<State,core::Bytes>(&mut owner) {").next().unwrap(),text.split("async fn main").nth(1).unwrap())
                } else {
                    format!(
                        "{}return core::panic::<i64>(91);}}",
                        text.split("let mut cancelled:").next().unwrap()
                    )
                }
            } else {
                text.to_owned()
            };
            let program = prepare(&text);
            let execution = crate::service_backend::prepare(program.verified());
            for records in [1, 8] {
                let (mut heap, trace) = channel_allocator(usize::MAX);
                trace.borrow_mut().panic_buffers = if active { vec![22, 9] } else { vec![20, 7] };
                let failure = execution
                    .execute(
                        &mut heap,
                        e::Control::with_limits(Default::default()),
                        r::Limits {
                            cleanup_failure_records: records,
                            ..Default::default()
                        },
                    )
                    .err()
                    .expect("two cleanup panics");
                if primary {
                    assert!(
                        matches!(failure.reason, r::Reason::Panic(Some(91))),
                        "{failure:?}"
                    );
                } else {
                    assert!(matches!(failure.reason, r::Reason::Cleanup), "{failure:?}");
                }
                assert!(failure.poisoned);
                let report = failure.report.as_ref().unwrap();
                assert_eq!(report.cleanup_failures(), 2, "{failure:?}");
                assert!(report.primary().unwrap().span.is_some());
                if records == 1 {
                    assert!(report.secondary().is_empty());
                    assert!(report.truncated() > 0);
                }
                let expected = if active {
                    if primary {
                        &[7, 42, 20, 22, 9][..]
                    } else {
                        &[7, 42, 22, 9, 20][..]
                    }
                } else {
                    &[20, 7][..]
                };
                assert_eq!(
                    trace.borrow().buffers,
                    expected,
                    "active={active} primary={primary}"
                );
                assert!(trace.borrow().live.is_empty(), "{failure:?}");
                assert_eq!(heap.stats().live_allocations, 0, "{failure:?}");
                assert_eq!(failure.tasks.live, 0);
            }
        }
    }
}

#[test]
fn every_shutdown_work_fuel_trace_and_cell_prefix_drains_all_owners() {
    for &(name, text) in SERVICE_SHUTDOWN_FIXTURES
        .iter()
        .chain(SERVICE_COPY_FIXTURES)
        .chain(SERVICE_STATE_FIXTURES)
    {
        let program = prepare(text);
        let execution = crate::service_backend::prepare(program.verified());
        let mut heap = allocator();
        let baseline = execution
            .execute(
                &mut heap,
                e::Control::with_limits(Default::default()).and_service_work(Some(u64::MAX)),
                r::Limits {
                    trace_events: 2048,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(baseline.value, 42);
        assert_eq!(heap.stats().live_allocations, 0);
        for work in 0..=baseline.usage.service_work {
            let mut heap = allocator();
            let result = execution.execute(
                &mut heap,
                e::Control::with_limits(Default::default()).and_service_work(Some(work)),
                Default::default(),
            );
            assert_eq!(
                heap.stats().live_allocations,
                0,
                "{name} work {work}: {:?}",
                result.as_ref().err()
            );
            if work < baseline.usage.service_work {
                let failure = result.err().expect("work prefix");
                assert!(
                    matches!(failure.reason,
                        r::Reason::Control(e::Failure::ServiceWorkLimit {limit,..})
                        | r::Reason::Execution(lang_mir::unit::ownership::failure::ExecutionFailure::ServiceWorkLimit {limit,..}) if limit==work),
                    "{name} work {work}: {failure:?}"
                );
            } else {
                assert_eq!(result.unwrap().value, 42);
            }
        }
        for fuel in 0..=baseline.usage.fuel {
            let mut heap = allocator();
            let result = execution.execute(
                &mut heap,
                e::Control::with_limits(e::Limits {
                    fuel: Some(fuel),
                    ..Default::default()
                }),
                Default::default(),
            );
            assert_eq!(
                heap.stats().live_allocations,
                0,
                "{name} fuel {fuel}: {:?}",
                result.as_ref().err()
            );
            if fuel < baseline.usage.fuel {
                assert!(matches!(
                    result.err().expect("fuel prefix").reason,
                    r::Reason::Control(e::Failure::FuelExhausted)
                ));
            } else {
                assert_eq!(result.unwrap().value, 42);
            }
        }
        for cells in 0..=baseline.usage.peak_live_cells {
            let mut heap = allocator();
            let result = execution.execute(
                &mut heap,
                e::Control::with_limits(e::Limits {
                    live_cells: Some(cells),
                    ..Default::default()
                }),
                Default::default(),
            );
            assert_eq!(
                heap.stats().live_allocations,
                0,
                "{name} cells {cells}: {:?}",
                result.as_ref().err()
            );
            if cells < baseline.usage.peak_live_cells {
                assert!(result.is_err(), "{name} cells {cells}");
            } else {
                assert_eq!(result.unwrap().value, 42);
            }
        }
        for events in 1..=baseline.events.len() {
            let mut heap = allocator();
            let result = execution.execute(
                &mut heap,
                e::Control::with_limits(Default::default()),
                r::Limits {
                    trace_events: events,
                    ..Default::default()
                },
            );
            assert_eq!(
                heap.stats().live_allocations,
                0,
                "{name} trace {events}: {:?}",
                result.as_ref().err()
            );
            if events < baseline.events.len() {
                assert!(matches!(
                    result.err().expect("trace prefix").reason,
                    r::Reason::TraceLimit
                ));
            } else {
                assert_eq!(result.unwrap().value, 42);
            }
        }
    }
}

#[test]
fn every_shutdown_provider_refusal_and_interruption_position_drains_the_real_inventory() {
    for &(name, text) in SERVICE_SHUTDOWN_FIXTURES
        .iter()
        .chain(SERVICE_COPY_FIXTURES)
        .chain(SERVICE_STATE_FIXTURES)
    {
        let program = prepare(text);
        let execution = crate::service_backend::prepare(program.verified());
        let (mut heap, trace) = channel_allocator(usize::MAX);
        let baseline = execution
            .execute(
                &mut heap,
                e::Control::with_limits(Default::default()),
                Default::default(),
            )
            .unwrap();
        assert_eq!(baseline.value, 42);
        let calls = trace.borrow().calls;
        assert!(trace.borrow().live.is_empty());
        for at in 1..=calls {
            for interrupt in [false, true] {
                let (mut heap, trace) = channel_allocator(if interrupt { usize::MAX } else { at });
                let flag = std::rc::Rc::new(e::InterruptionFlag::new());
                if interrupt {
                    trace.borrow_mut().interrupt = Some((at, flag.clone()));
                }
                let failure = execution
                    .execute(
                        &mut heap,
                        e::Control::with_limits(Default::default())
                            .and_interruption(&flag)
                            .and_service_work(Some(baseline.usage.service_work)),
                        Default::default(),
                    )
                    .err()
                    .unwrap_or_else(|| {
                        panic!("{name} provider {at}/{calls} interrupt={interrupt}")
                    });
                if interrupt {
                    assert!(
                        matches!(failure.reason, r::Reason::Control(e::Failure::Interrupted)),
                        "{name} {at}: {failure:?}"
                    );
                }
                assert_eq!(failure.tasks.live, 0, "{name} {at}");
                assert_eq!(heap.stats().live_allocations, 0, "{name} {at}: {failure:?}");
                assert!(
                    trace.borrow().live.is_empty(),
                    "{name} {at}: {:?}",
                    trace.borrow().live
                );
            }
        }
    }
}

#[test]
fn requested_active_service_closes_at_explicit_finish_or_owner_return() {
    let text = include_str!("fixtures/task_service_active_cancel.lang");
    let boundary = "    match await core::service_receive::<State,core::Bytes>(&mut owner) {";
    for returned in [false, true] {
        let text = if returned {
            let before = text.split(boundary).next().unwrap();
            let main = text.split("async fn main").nth(1).unwrap();
            format!("{before}return answer;}}\nasync fn main{main}")
        } else {
            text.replace(boundary,&format!("    let mut explicit_finish:core::Unit=core::service_finish::<State,core::Bytes>(&mut owner);\n{boundary}"))
        };
        let program = prepare(&text);
        let execution = crate::service_backend::prepare(program.verified());
        let (mut heap, trace) = channel_allocator(usize::MAX);
        let outcome = execution
            .execute(
                &mut heap,
                e::Control::with_limits(Default::default()),
                Default::default(),
            )
            .unwrap();
        assert_eq!(outcome.value, 42);
        let buffers = &trace.borrow().buffers;
        assert!(
            buffers.iter().position(|b| *b == 22).unwrap()
                < buffers.iter().position(|b| *b == 9).unwrap(),
            "return={returned}: {buffers:?}"
        );
        assert!(trace.borrow().live.is_empty());
        assert_eq!(heap.stats().live_allocations, 0);
    }
}

#[test]
fn structured_task_cancellation_closes_active_state_after_received_locals_and_queued_payloads() {
    let helpers = include_str!("fixtures/task_service_cancel.lang")
        .split("async fn main")
        .next()
        .unwrap();
    for requested in [false, true] {
        let request = if requested {
            "let mut requested:core::Unit=core::service_cancel::<core::Bytes>(&handle);"
        } else {
            ""
        };
        let text = format!(
            r#"{helpers}
async fn worker(input:core::ServiceOwner<State,core::Bytes>)->i64 {{
    let mut owner:core::ServiceOwner<State,core::Bytes>=input;
    let mut received:core::Bytes=core::option_unwrap::<core::Bytes>(await core::service_receive::<State,core::Bytes>(&mut owner));
    yield;
    yield;
    return 0;
}}
async fn main()->i64 {{
    let mut setup:core::ServiceSetup<State,core::Bytes>=core::result_unwrap::<core::ServiceSetup<State,core::Bytes>,core::ServiceError>(core::service_new::<State,core::Bytes>(core::allocator(),1,State {{marker:bytes(7)}}));
    let mut handle:core::ServiceHandle<core::Bytes>=core::service_handle::<State,core::Bytes>(&setup);
    let mut owner:core::ServiceOwner<State,core::Bytes>=core::service_owner::<State,core::Bytes>(setup);
    let mut first:core::SendResult<core::Bytes>=await core::service_send::<core::Bytes>(&handle,bytes(20));
    let mut serving:core::Task<i64>=spawn worker(owner);
    yield;
    let mut second:core::SendResult<core::Bytes>=await core::service_send::<core::Bytes>(&handle,bytes(22));
    {request}
    cancel serving;
    return 42;
}}
"#
        );
        let program = prepare(&text);
        let execution = crate::service_backend::prepare(program.verified());
        let (mut heap, trace) = channel_allocator(usize::MAX);
        let outcome = execution
            .execute(
                &mut heap,
                e::Control::with_limits(Default::default()),
                Default::default(),
            )
            .unwrap();
        assert_eq!(outcome.value, 42);
        assert_eq!(trace.borrow().buffers, [20, 22, 7]);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(heap.stats().live_allocations, 0);
    }
}

#[test]
fn shutdown_protocol_quotas_refuse_one_below_exact_without_losing_payloads() {
    let program = prepare(include_str!("fixtures/task_service_active_cancel.lang"));
    let execution = crate::service_backend::prepare(program.verified());
    let exact = lang_runtime::channel::Limits {
        channels: 1,
        slots: 1,
        handles: 3,
        waiters: 1,
    };
    for kind in 0..=4 {
        let mut limits = exact;
        match kind {
            0 => limits.channels = 0,
            1 => limits.slots = 0,
            2 => limits.handles = 2,
            3 => limits.waiters = 0,
            _ => {}
        }
        let (mut heap, trace) = channel_allocator(usize::MAX);
        let result = execution.execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            r::Limits {
                channels: limits,
                ..Default::default()
            },
        );
        assert!(trace.borrow().live.is_empty(), "quota {kind}");
        assert_eq!(heap.stats().live_allocations, 0, "quota {kind}");
        if kind == 4 {
            assert_eq!(result.unwrap().value, 42);
        } else {
            let failure = result.err().expect("quota refusal");
            use lang_runtime::channel::{Error as C, managed::Error as G};
            assert!(
                matches!(
                    (kind, &failure.reason),
                    (
                        0,
                        r::Reason::Channel(G::Protocol(C::ChannelLimit { limit: 0 }))
                    ) | (
                        1,
                        r::Reason::Channel(G::Protocol(C::SlotLimit { limit: 0 }))
                    ) | (
                        2,
                        r::Reason::Channel(G::Protocol(C::HandleLimit { limit: 2 }))
                    ) | (
                        3,
                        r::Reason::Channel(G::Protocol(C::WaiterLimit { limit: 0 }))
                    )
                ),
                "quota {kind}: {failure:?}"
            );
            assert_eq!(failure.tasks.live, 0);
        }
    }
}

#[test]
fn cancellation_after_sender_wakeup_before_commit_recovers_both_unpublished_payloads() {
    let text = include_str!("fixtures/task_service_active_cancel.lang");
    let text = text.replacen(
        "    yield;\n    let mut marker:i64=0;",
        "    let mut marker:i64=0;",
        1,
    );
    let program = prepare(&text);
    let execution = crate::service_backend::prepare(program.verified());
    let (mut heap, trace) = channel_allocator(usize::MAX);
    let outcome = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 2048,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(outcome.value, 42);
    assert!(
        outcome
            .events
            .iter()
            .filter(|event| event.task == 1 && event.kind == r::EventKind::Resume)
            .count()
            >= 2
    );
    assert_eq!(trace.borrow().buffers, [7, 42, 22, 9, 20]);
    assert!(trace.borrow().live.is_empty());
    assert_eq!(heap.stats().live_allocations, 0);
}

#[test]
fn closed_owner_can_cross_an_owned_message_without_resurrecting_its_state() {
    let helpers = include_str!("fixtures/task_service_cancel.lang")
        .split("async fn main")
        .next()
        .unwrap();
    let text = format!(
        r#"{helpers}
async fn main()->i64 {{
    let mut setup:core::ServiceSetup<State,core::Bytes>=core::result_unwrap::<core::ServiceSetup<State,core::Bytes>,core::ServiceError>(core::service_new::<State,core::Bytes>(core::allocator(),1,State {{marker:bytes(7)}}));
    let mut handle:core::ServiceHandle<core::Bytes>=core::service_handle::<State,core::Bytes>(&setup);
    let mut owner:core::ServiceOwner<State,core::Bytes>=core::service_owner::<State,core::Bytes>(setup);
    let mut requested:core::Unit=core::service_cancel::<core::Bytes>(&handle);
    let mut relay:core::ChannelSetup<core::ServiceOwner<State,core::Bytes>>=core::result_unwrap::<core::ChannelSetup<core::ServiceOwner<State,core::Bytes>>,core::ChannelError>(core::channel_new::<core::ServiceOwner<State,core::Bytes>>(core::allocator(),1));
    let mut sender:core::Sender<core::ServiceOwner<State,core::Bytes>>=core::channel_sender::<core::ServiceOwner<State,core::Bytes>>(&relay);
    let mut receiver:core::Receiver<core::ServiceOwner<State,core::Bytes>>=core::channel_receiver::<core::ServiceOwner<State,core::Bytes>>(relay);
    let mut sent:core::SendResult<core::ServiceOwner<State,core::Bytes>>=await core::channel_send::<core::ServiceOwner<State,core::Bytes>>(&sender,owner);
    let mut recovered:core::ServiceOwner<State,core::Bytes>=core::option_unwrap::<core::ServiceOwner<State,core::Bytes>>(await core::channel_receive::<core::ServiceOwner<State,core::Bytes>>(&mut receiver));
    match core::service_read::<State,core::Bytes>(&recovered) {{
        core::Result::Ok(unexpected_state)=>{{return core::panic::<i64>(-10);}},
        core::Result::Err(closed_error)=>{{let mut checked:core::Unit=unavailable(closed_error);}},
    }}
    let mut finished:core::Unit=core::service_finish::<State,core::Bytes>(&mut recovered);
    return 42;
}}
"#
    );
    let program = prepare(&text);
    let execution = crate::service_backend::prepare(program.verified());
    let (mut heap, trace) = channel_allocator(usize::MAX);
    let outcome = execution
        .execute(
            &mut heap,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
    assert_eq!(outcome.value, 42);
    assert_eq!(trace.borrow().buffers, [7]);
    assert!(trace.borrow().live.is_empty());
    assert_eq!(heap.stats().live_allocations, 0);
}
