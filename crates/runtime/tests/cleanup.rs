use lang_mir::unit::{SourceId, Span};
use lang_runtime::cleanup::{
    EnterError, ExecutionFailure, Exit, FailureKind, Frame, Instance, Kind, Limits, Reason, Slot,
    StateError,
};
use lang_runtime::shared::{Payload, SharedHeap, Strong};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

type Trace = Rc<RefCell<Vec<(u32, Reason)>>>;
type Drops = Rc<RefCell<Vec<u32>>>;

#[derive(Debug)]
struct Resource {
    value: Box<u32>,
    drops: Drops,
    panic_on_drop: bool,
}
impl Drop for Resource {
    fn drop(&mut self) {
        self.drops.borrow_mut().push(*self.value);
        assert!(!self.panic_on_drop, "trusted payload drop panicked");
    }
}
fn resource(id: u32, drops: &Drops) -> Resource {
    Resource {
        value: Box::new(id),
        drops: drops.clone(),
        panic_on_drop: false,
    }
}
fn cleaner(trace: &Trace) -> impl FnMut(&mut Resource, Reason) -> Result<(), u32> {
    let trace = trace.clone();
    move |value, reason| {
        trace.borrow_mut().push((*value.value, reason));
        Ok(())
    }
}
fn span(start: u32) -> Span {
    Span {
        source: SourceId(4),
        start,
        end: start + 1,
    }
}
fn initialize<C: FnMut(&mut Resource, Reason) -> Result<(), u32>>(
    frame: &mut Frame<'_, Resource, C>,
    slot: usize,
    id: u32,
    drops: &Drops,
) {
    frame.declare(Slot(slot), Kind::Owner).unwrap();
    frame
        .initialize(Slot(slot), resource(id, drops), Some(span(id)))
        .unwrap();
}
fn ids(trace: &Trace) -> Vec<u32> {
    trace.borrow().iter().map(|(id, _)| *id).collect()
}

#[test]
fn actual_initialization_order_moves_reinitialization_and_parameters() {
    let (drops, trace) = (Drops::default(), Trace::default());
    let mut instance = Instance::default();
    let mut frame = instance.enter(Limits::default(), cleaner(&trace)).unwrap();
    initialize(&mut frame, 0, 10, &drops);
    initialize(&mut frame, 1, 11, &drops);
    frame.open_scope().unwrap();
    // Declaration order is deliberately different from initialization order.
    frame.declare(Slot(2), Kind::Owner).unwrap();
    initialize(&mut frame, 3, 30, &drops);
    frame
        .initialize(Slot(2), resource(20, &drops), Some(span(20)))
        .unwrap();
    frame.declare(Slot(4), Kind::Owner).unwrap();
    frame.move_value(Slot(2), Slot(4)).unwrap();
    frame
        .initialize(Slot(2), resource(21, &drops), Some(span(21)))
        .unwrap();
    frame.close_scope().unwrap();
    assert_eq!(ids(&trace), [21, 20, 30]);
    assert_eq!(frame.value(Slot(2)).unwrap_err(), StateError::Undeclared);
    let done = frame.finish(Exit::Return(None));
    assert!(done.report.primary().is_none());
    assert!(!done.poisoned);
    assert_eq!(ids(&trace), [21, 20, 30, 11, 10]);
    assert_eq!(*drops.borrow(), ids(&trace));
}

#[test]
fn branch_flags_and_loop_scopes_do_not_keep_old_obligations() {
    let (drops, trace) = (Drops::default(), Trace::default());
    let mut instance = Instance::default();
    let mut frame = instance.enter(Limits::default(), cleaner(&trace)).unwrap();
    frame.declare(Slot(0), Kind::Owner).unwrap();
    for iteration in 0..100 {
        frame.open_scope().unwrap();
        frame.declare(Slot(1), Kind::Owner).unwrap();
        frame.declare(Slot(2), Kind::Owner).unwrap();
        if iteration % 2 == 0 {
            frame
                .initialize(Slot(1), resource(iteration, &drops), None)
                .unwrap();
            frame.move_value(Slot(1), Slot(0)).unwrap();
            frame.drop_value(Slot(0)).unwrap();
        }
        frame.close_scope().unwrap();
    }
    let done = frame.finish(Exit::Return(None));
    assert!(done.report.primary().is_none());
    assert_eq!(ids(&trace), (0..100).step_by(2).collect::<Vec<_>>());
    assert_eq!(*drops.borrow(), ids(&trace));
}

#[test]
fn replacement_commits_before_old_cleanup_failure_and_never_retries_old_value() {
    let (drops, trace) = (Drops::default(), Trace::default());
    let calls = trace.clone();
    let mut instance = Instance::default();
    let mut frame = instance
        .enter(Limits::default(), move |value: &mut Resource, reason| {
            calls.borrow_mut().push((*value.value, reason));
            if *value.value == 1 { Err(41) } else { Ok(()) }
        })
        .unwrap();
    initialize(&mut frame, 0, 1, &drops);
    initialize(&mut frame, 1, 2, &drops);
    assert_eq!(frame.replace(Slot(1), Slot(0)), Err(StateError::Failed));
    assert_eq!(*frame.value(Slot(0)).unwrap().value, 2);
    assert_eq!(frame.value(Slot(1)).unwrap_err(), StateError::Uninitialized);
    assert_eq!(frame.open_scope(), Err(StateError::Failed));
    let done = frame.finish(Exit::Return(None));
    let first = done.report.primary().unwrap();
    assert_eq!(first.kind, FailureKind::Cleanup(41));
    assert_eq!(first.span, Some(span(1)));
    assert_eq!(first.slot, Some(Slot(0)));
    assert_eq!(
        *trace.borrow(),
        [(1, Reason::Replacement), (2, Reason::Failure)]
    );
    assert_eq!(*drops.borrow(), [1, 2]);
    assert!(done.poisoned);
    assert!(matches!(
        instance.enter(Limits::default(), cleaner(&trace)),
        Err(EnterError::Poisoned)
    ));
}

#[test]
fn rejected_operations_preserve_destination_and_incoming_owners() {
    let (drops, trace) = (Drops::default(), Trace::default());
    let mut instance = Instance::default();
    let mut frame = instance.enter(Limits::default(), cleaner(&trace)).unwrap();
    initialize(&mut frame, 0, 10, &drops);
    initialize(&mut frame, 1, 20, &drops);
    frame.declare(Slot(2), Kind::Borrow).unwrap();
    frame.declare(Slot(3), Kind::Owner).unwrap();
    assert_eq!(
        frame.move_value(Slot(1), Slot(0)),
        Err(StateError::Initialized)
    );
    assert_eq!(
        frame.replace(Slot(1), Slot(3)),
        Err(StateError::Uninitialized)
    );
    assert_eq!(
        frame.replace(Slot(1), Slot(1)),
        Err(StateError::KindMismatch)
    );
    assert_eq!(
        frame.move_value(Slot(1), Slot(2)),
        Err(StateError::KindMismatch)
    );
    assert_eq!(
        frame.replace(Slot(1), Slot(99999)),
        Err(StateError::InvalidSlot)
    );
    let rejected = frame
        .initialize(Slot(0), resource(30, &drops), None)
        .unwrap_err();
    assert_eq!(rejected.error, StateError::Initialized);
    assert_eq!(*rejected.value.value, 30);
    assert!(drops.borrow().is_empty());
    assert!(trace.borrow().is_empty());
    drop(rejected.value); // incoming owner remains outside publication
    assert_eq!(*frame.value(Slot(0)).unwrap().value, 10);
    assert_eq!(*frame.value(Slot(1)).unwrap().value, 20);
    frame.finish(Exit::Return(None));
    assert_eq!(ids(&trace), [20, 10]);
    assert_eq!(*drops.borrow(), [30, 20, 10]);
}

#[test]
fn successful_return_ends_borrows_before_owners_and_publishes_last() {
    let (drops, trace) = (Drops::default(), Trace::default());
    let mut instance = Instance::default();
    let mut frame = instance.enter(Limits::default(), cleaner(&trace)).unwrap();
    initialize(&mut frame, 0, 10, &drops);
    frame.declare(Slot(1), Kind::Borrow).unwrap();
    frame
        .initialize(Slot(1), resource(99, &drops), None)
        .unwrap();
    frame.open_scope().unwrap();
    initialize(&mut frame, 2, 20, &drops);
    initialize(&mut frame, 3, 30, &drops);
    let done = frame.finish(Exit::Return(Some(Slot(3))));
    assert_eq!(ids(&trace), [99, 20, 10]);
    assert_eq!(*drops.borrow(), [99, 20, 10]);
    assert_eq!(*done.value.as_ref().unwrap().value, 30);
    assert!(done.report.primary().is_none());
    drop(done.value);
    assert_eq!(*drops.borrow(), [99, 20, 10, 30]);
}

#[test]
fn failed_return_discards_escrow_after_independent_cleanup() {
    let (drops, trace) = (Drops::default(), Trace::default());
    let calls = trace.clone();
    let mut instance = Instance::default();
    let mut frame = instance
        .enter(Limits::default(), move |value: &mut Resource, reason| {
            calls.borrow_mut().push((*value.value, reason));
            Err(*value.value)
        })
        .unwrap();
    initialize(&mut frame, 0, 10, &drops);
    initialize(&mut frame, 1, 20, &drops);
    initialize(&mut frame, 2, 30, &drops);
    let done = frame.finish(Exit::Return(Some(Slot(2))));
    assert!(done.value.is_none());
    assert_eq!(
        done.report.primary().unwrap().kind,
        FailureKind::Cleanup(20)
    );
    assert_eq!(
        done.report
            .secondary()
            .iter()
            .map(|r| r.kind)
            .collect::<Vec<_>>(),
        [FailureKind::Cleanup(10), FailureKind::Cleanup(30)]
    );
    assert_eq!(
        trace.borrow().last(),
        Some(&(30, Reason::UnpublishedReturn))
    );
    assert_eq!(*drops.borrow(), [20, 10, 30]);
    assert!(done.poisoned);
}

#[test]
fn every_execution_failure_keeps_primary_and_only_host_panic_or_invariant_poison() {
    for failure in [
        ExecutionFailure::Panic,
        ExecutionFailure::Fault,
        ExecutionFailure::FuelLimit,
        ExecutionFailure::FrameLimit,
        ExecutionFailure::CellLimit,
        ExecutionFailure::AllocationLimit,
        ExecutionFailure::HostCall,
        ExecutionFailure::HostPanic,
        ExecutionFailure::Invariant,
    ] {
        for cleanup_fails in [false, true] {
            let (drops, trace) = (Drops::default(), Trace::default());
            let mut instance = Instance::default();
            let mut frame = instance
                .enter(Limits::default(), |value: &mut Resource, _| {
                    if cleanup_fails {
                        Err(*value.value)
                    } else {
                        Ok(())
                    }
                })
                .unwrap();
            initialize(&mut frame, 0, 1, &drops);
            initialize(&mut frame, 1, 2, &drops);
            let done = frame.finish(Exit::Failure {
                failure,
                span: Some(span(100)),
            });
            assert!(done.value.is_none());
            assert_eq!(
                done.report.primary().unwrap().kind,
                FailureKind::Execution(failure)
            );
            assert_eq!(done.report.primary().unwrap().span, Some(span(100)));
            assert_eq!(
                done.report.secondary().len(),
                if cleanup_fails { 2 } else { 0 }
            );
            assert_eq!(*drops.borrow(), [2, 1]);
            let poisoned = cleanup_fails
                || matches!(
                    failure,
                    ExecutionFailure::HostPanic | ExecutionFailure::Invariant
                );
            assert_eq!(done.poisoned, poisoned);
            let next = instance.enter(Limits::default(), cleaner(&trace));
            assert_eq!(next.is_err(), poisoned);
        }
    }
}

#[test]
fn bounded_reports_keep_order_and_drain_every_owner_after_overflow() {
    let drops = Drops::default();
    let mut instance = Instance::default();
    let mut frame = instance
        .enter(Limits::default(), |value: &mut Resource, _| {
            Err(*value.value)
        })
        .unwrap();
    for id in 0..300 {
        initialize(&mut frame, id as usize, id, &drops);
    }
    let done = frame.finish(Exit::Return(None));
    assert_eq!(
        done.report.primary().unwrap().kind,
        FailureKind::Cleanup(299)
    );
    assert_eq!(done.report.secondary().len(), 255);
    assert_eq!(
        done.report.secondary().last().unwrap().kind,
        FailureKind::Cleanup(44)
    );
    assert_eq!(done.report.truncated(), 44);
    assert_eq!(*drops.borrow(), (0..300).rev().collect::<Vec<_>>());

    let mut instance = Instance::default();
    let mut frame = instance
        .enter(
            Limits {
                failure_records: 1,
                ..Limits::default()
            },
            |_: &mut Resource, _| Err(1),
        )
        .unwrap();
    for id in 0..3 {
        initialize(&mut frame, id, id as u32, &drops);
    }
    let done = frame.finish(Exit::Failure {
        failure: ExecutionFailure::Panic,
        span: None,
    });
    assert_eq!(
        done.report.primary().unwrap().kind,
        FailureKind::Execution(ExecutionFailure::Panic)
    );
    assert!(done.report.secondary().is_empty());
    assert_eq!(done.report.truncated(), 3);
}

#[test]
fn original_failure_precedes_already_observed_scope_cleanup_errors() {
    let drops = Drops::default();
    let mut instance = Instance::default();
    let mut frame = instance
        .enter(
            Limits {
                failure_records: 2,
                ..Limits::default()
            },
            |value: &mut Resource, _| Err(*value.value),
        )
        .unwrap();
    frame.open_scope().unwrap();
    initialize(&mut frame, 0, 10, &drops);
    initialize(&mut frame, 1, 20, &drops);
    assert_eq!(frame.close_scope(), Err(StateError::Failed));
    let done = frame.finish(Exit::Failure {
        failure: ExecutionFailure::Fault,
        span: None,
    });
    assert_eq!(
        done.report.primary().unwrap().kind,
        FailureKind::Execution(ExecutionFailure::Fault)
    );
    assert_eq!(done.report.secondary()[0].kind, FailureKind::Cleanup(20));
    assert_eq!(done.report.truncated(), 1);
    assert_eq!(*drops.borrow(), [20, 10]);
}

#[test]
fn metadata_reservation_and_scope_limits_fail_before_effects() {
    let effects = Cell::new(0);
    let mut instance = Instance::default();
    for (limits, expected) in [
        (
            Limits {
                failure_records: 0,
                ..Limits::default()
            },
            EnterError::InvalidLimits,
        ),
        (
            Limits {
                scopes: 0,
                ..Limits::default()
            },
            EnterError::InvalidLimits,
        ),
        (
            Limits {
                slots: usize::MAX,
                ..Limits::default()
            },
            EnterError::Allocation,
        ),
        (
            Limits {
                scopes: usize::MAX,
                ..Limits::default()
            },
            EnterError::Allocation,
        ),
        (
            Limits {
                failure_records: usize::MAX,
                ..Limits::default()
            },
            EnterError::Allocation,
        ),
    ] {
        let result = instance.enter(limits, |_: &mut Resource, _| {
            effects.set(effects.get() + 1);
            Ok(())
        });
        assert!(matches!(result, Err(error) if error == expected));
        drop(result);
        assert!(!instance.is_poisoned());
    }
    assert_eq!(effects.get(), 0);
    let mut frame = instance
        .enter(
            Limits {
                slots: 0,
                scopes: 1,
                failure_records: 1,
            },
            |_: &mut Resource, _| Ok(()),
        )
        .unwrap();
    assert_eq!(frame.open_scope(), Err(StateError::ScopeLimit));
    assert_eq!(frame.close_scope(), Err(StateError::OuterScope));
    assert_eq!(
        frame.declare(Slot(0), Kind::Owner),
        Err(StateError::InvalidSlot)
    );
    assert!(frame.finish(Exit::Return(None)).report.primary().is_none());
}

#[test]
fn callback_and_hostile_panic_payload_failures_are_contained_without_retry() {
    struct HostilePanic;
    impl Drop for HostilePanic {
        fn drop(&mut self) {
            panic!("hostile panic payload destructor");
        }
    }
    let (drops, trace) = (Drops::default(), Trace::default());
    let calls = trace.clone();
    let mut instance = Instance::default();
    let mut frame = instance
        .enter(Limits::default(), move |value: &mut Resource, reason| {
            calls.borrow_mut().push((*value.value, reason));
            if *value.value == 2 {
                std::panic::panic_any(HostilePanic);
            }
            Ok(())
        })
        .unwrap();
    initialize(&mut frame, 0, 1, &drops);
    initialize(&mut frame, 1, 2, &drops);
    initialize(&mut frame, 2, 3, &drops);
    let done = frame.finish(Exit::Return(None));
    assert_eq!(ids(&trace), [3, 2, 1]);
    assert_eq!(*drops.borrow(), [3, 1]); // failed callback payload deliberately quarantined
    assert_eq!(
        done.report.primary().unwrap().kind,
        FailureKind::CleanupPanic
    );
    assert_eq!(
        done.report.secondary()[0].kind,
        FailureKind::PanicPayloadDropPanic
    );
    assert_eq!(done.report.quarantined(), 2);
    assert!(done.poisoned);
}

#[test]
fn payload_destructor_panics_and_abandoned_frames_still_clean_independent_owners() {
    let (drops, trace) = (Drops::default(), Trace::default());
    let mut instance = Instance::default();
    {
        let mut frame = instance.enter(Limits::default(), cleaner(&trace)).unwrap();
        initialize(&mut frame, 0, 1, &drops);
        frame.open_scope().unwrap();
        frame.declare(Slot(1), Kind::Owner).unwrap();
        let mut panicking = resource(2, &drops);
        panicking.panic_on_drop = true;
        frame.initialize(Slot(1), panicking, None).unwrap();
        initialize(&mut frame, 2, 3, &drops);
        // Host abandonment follows the same drain path without publishing a result.
    }
    assert_eq!(*drops.borrow(), [3, 2, 1]);
    assert_eq!(ids(&trace), [3, 2, 1]);
    assert!(instance.is_poisoned());

    let mut instance = Instance::default();
    let mut frame = instance.enter(Limits::default(), cleaner(&trace)).unwrap();
    frame.declare(Slot(0), Kind::Owner).unwrap();
    let mut panicking = resource(4, &drops);
    panicking.panic_on_drop = true;
    frame.initialize(Slot(0), panicking, Some(span(4))).unwrap();
    let done = frame.finish(Exit::Return(None));
    assert_eq!(
        done.report.primary().unwrap().kind,
        FailureKind::PayloadDropPanic
    );
    assert_eq!(done.report.primary().unwrap().span, Some(span(4)));
}

#[test]
fn real_shared_owners_and_typed_err_results_keep_normal_return_semantics() {
    let heap = SharedHeap::default();
    let make = || {
        heap.allocate(
            Payload {
                bytes: b"real".to_vec(),
                edges: vec![],
            },
            None,
        )
        .unwrap()
    };
    let a = make();
    let weak_a = a.downgrade().unwrap();
    let b = make();
    let weak_b = b.downgrade().unwrap();
    let mut instance = Instance::default();
    let mut frame = instance
        .enter(Limits::default(), |_: &mut Strong, _| Ok(()))
        .unwrap();
    for id in 0..3 {
        frame.declare(Slot(id), Kind::Owner).unwrap();
    }
    frame.initialize(Slot(0), a, None).unwrap();
    frame.initialize(Slot(1), b, None).unwrap();
    frame.move_value(Slot(0), Slot(2)).unwrap();
    let done = frame.finish(Exit::Return(Some(Slot(2))));
    assert_eq!(heap.stats().unwrap().live_nodes, 1);
    assert!(weak_b.upgrade().is_err());
    assert!(weak_a.upgrade().is_ok());
    drop(done.value);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
    assert!(weak_a.upgrade().is_err());
    let mut frame = instance
        .enter(Limits::default(), |_: &mut Result<Strong, u32>, _| Ok(()))
        .unwrap();
    frame.declare(Slot(0), Kind::Owner).unwrap();
    frame.initialize(Slot(0), Err(42), None).unwrap();
    let done = frame.finish(Exit::Return(Some(Slot(0))));
    assert!(matches!(done.value, Some(Err(42))));
    assert!(done.report.primary().is_none());
    assert!(!done.poisoned);
}

#[test]
fn nested_calls_transfer_return_ownership_only_after_callee_cleanup() {
    let (drops, trace) = (Drops::default(), Trace::default());
    let mut instance = Instance::default();
    let mut frame = instance.enter(Limits::default(), cleaner(&trace)).unwrap();
    initialize(&mut frame, 0, 10, &drops);
    frame.declare(Slot(1), Kind::Owner).unwrap();
    frame.open_call().unwrap();
    frame.declare(Slot(2), Kind::Owner).unwrap();
    frame.move_value(Slot(0), Slot(2)).unwrap();
    initialize(&mut frame, 3, 30, &drops);
    assert_eq!(frame.close_scope(), Err(StateError::CallBoundary));
    assert_eq!(
        frame.return_to(Some(Slot(0)), Some(Slot(1))),
        Err(StateError::Uninitialized)
    );
    frame.open_scope().unwrap();
    initialize(&mut frame, 4, 40, &drops);
    assert_eq!(
        frame.return_to(Some(Slot(2)), Some(Slot(4))),
        Err(StateError::CallBoundary)
    );
    assert!(trace.borrow().is_empty());
    frame.return_to(Some(Slot(2)), Some(Slot(1))).unwrap();
    assert_eq!(ids(&trace), [40, 30]);
    assert_eq!(*frame.value(Slot(1)).unwrap().value, 10);
    assert_eq!(frame.value(Slot(2)).unwrap_err(), StateError::Undeclared);
    frame.open_call().unwrap();
    initialize(&mut frame, 2, 50, &drops);
    frame.return_to(None, None).unwrap(); // normal Copy/Unit/typed Err call
    frame.finish(Exit::Return(None));
    assert_eq!(*drops.borrow(), [40, 30, 50, 10]);
}

#[test]
fn failed_callee_cleanup_destroys_unpublished_result_and_retains_caller_scope() {
    let (drops, trace) = (Drops::default(), Trace::default());
    let calls = trace.clone();
    let mut instance = Instance::default();
    let mut frame = instance
        .enter(Limits::default(), move |value: &mut Resource, reason| {
            calls.borrow_mut().push((*value.value, reason));
            if *value.value == 30 { Err(30) } else { Ok(()) }
        })
        .unwrap();
    initialize(&mut frame, 0, 10, &drops);
    frame.declare(Slot(1), Kind::Owner).unwrap();
    frame.open_call().unwrap();
    initialize(&mut frame, 2, 20, &drops);
    initialize(&mut frame, 3, 30, &drops);
    assert_eq!(
        frame.return_to(Some(Slot(2)), Some(Slot(1))),
        Err(StateError::Failed)
    );
    assert_eq!(frame.value(Slot(1)).unwrap_err(), StateError::Uninitialized);
    assert_eq!(*frame.value(Slot(0)).unwrap().value, 10);
    assert_eq!(
        *trace.borrow(),
        [(30, Reason::ScopeExit), (20, Reason::UnpublishedReturn)]
    );
    frame.finish(Exit::Return(None));
    assert_eq!(*drops.borrow(), [30, 20, 10]);
    assert!(instance.is_poisoned());
}

#[test]
fn generated_moves_replacements_and_explicit_drops_match_an_independent_order_oracle() {
    for seed in 1..=100u64 {
        let (drops, trace) = (Drops::default(), Trace::default());
        let mut instance = Instance::default();
        let mut frame = instance
            .enter(
                Limits {
                    slots: 16,
                    ..Limits::default()
                },
                cleaner(&trace),
            )
            .unwrap();
        for slot in 0..16 {
            frame.declare(Slot(slot), Kind::Owner).unwrap();
        }
        let mut state = seed;
        let mut values = [None; 16];
        let mut order = Vec::new();
        let mut expected = Vec::new();
        let mut id = 0;
        for _ in 0..256 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let from = (state >> 32) as usize % 16;
            let to = (state >> 40) as usize % 16;
            match (state >> 48) % 4 {
                0 if values[to].is_none() => {
                    frame
                        .initialize(Slot(to), resource(id, &drops), None)
                        .unwrap();
                    values[to] = Some(id);
                    order.push(to);
                    id += 1;
                }
                1 if from != to && values[from].is_some() && values[to].is_none() => {
                    frame.move_value(Slot(from), Slot(to)).unwrap();
                    values[to] = values[from].take();
                    order.retain(|slot| *slot != from);
                    order.push(to);
                }
                2 if from != to && values[from].is_some() && values[to].is_some() => {
                    frame.replace(Slot(from), Slot(to)).unwrap();
                    expected.push(values[to].replace(values[from].unwrap()).unwrap());
                    values[from] = None;
                    order.retain(|slot| *slot != from && *slot != to);
                    order.push(to);
                }
                3 if values[from].is_some() => {
                    frame.drop_value(Slot(from)).unwrap();
                    expected.push(values[from].take().unwrap());
                    order.retain(|slot| *slot != from);
                }
                _ => {}
            }
        }
        frame.finish(Exit::Return(None));
        expected.extend(order.into_iter().rev().map(|slot| values[slot].unwrap()));
        assert_eq!(ids(&trace), expected, "seed {seed}");
        assert_eq!(*drops.borrow(), expected, "seed {seed}");
        assert_eq!(
            expected.len(),
            id as usize,
            "each initialized owner cleans once"
        );
    }
}
