use lang_runtime::{allocation as a, execution as e, task::runner as r};
use lang_semantics::analysis::{Program, TaskProgram};
use std::path::Path;

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
fn state_and_message_owners_survive_fifo_backpressure_and_helper_read_loans() {
    let program = fixture();
    let mut allocator = allocator();
    let outcome = lang_vm::unit::tasks::execute_entry(
        program.verified(),
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
    let mut heap = allocator();
    let baseline = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut heap,
        e::Control::with_limits(Default::default()).and_service_work(Some(u64::MAX)),
        Default::default(),
    )
    .unwrap();
    assert_eq!(baseline.value, 42);
    assert_eq!(heap.stats().live_allocations, 0);
    for work in 0..=baseline.usage.service_work {
        let mut heap = allocator();
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
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
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
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
    let mut heap = allocator();
    let baseline = lang_vm::unit::tasks::execute_entry(
        program.verified(),
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
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
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
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
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
    let mut heap = allocator();
    let baseline = lang_vm::unit::tasks::execute_entry(
        program.verified(),
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
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
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
fn non_share_state_and_unimplemented_cancel_refuse_before_allocator_effects() {
    for text in [
        "fn carry(owner:core::ServiceOwner<core::Receiver<i64>,i64>)->core::ServiceOwner<core::Receiver<i64>,i64>{return owner;}async fn main()->i64{yield;return 42;}",
        "async fn main()->i64{let mut setup:core::ServiceSetup<i64,i64>=core::result_unwrap::<core::ServiceSetup<i64,i64>,core::ServiceError>(core::service_new::<i64,i64>(core::allocator(),1,42));let mut handle:core::ServiceHandle<i64>=core::service_handle::<i64,i64>(&setup);let mut cancelled:core::Unit=core::service_cancel::<i64>(&handle);return 42;}",
    ] {
        let program = prepare(text);
        let mut heap = allocator();
        let before = heap.stats();
        assert!(
            lang_vm::unit::tasks::execute_entry(
                program.verified(),
                &mut heap,
                e::Control::with_limits(Default::default()),
                Default::default()
            )
            .is_err()
        );
        assert_eq!(heap.stats(), before);
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
    let mut heap = allocator();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut heap,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(heap.stats().live_allocations, 0);
}
