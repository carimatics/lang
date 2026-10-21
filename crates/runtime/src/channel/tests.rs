use super::*;
use std::cell::RefCell;

fn session(records: usize) -> c::Session {
    c::Session::prepare(
        std::num::NonZeroUsize::new(records).unwrap(),
        &mut lang_mir::unit::Budget::new(Default::default()),
    )
    .unwrap()
}

#[derive(Debug)]
struct Value {
    data: Vec<u8>,
    dropped: Rc<RefCell<Vec<u8>>>,
    receiver: Option<Receiver>,
}
impl Value {
    fn new(id: u8, dropped: &Rc<RefCell<Vec<u8>>>) -> Self {
        Self {
            data: vec![id],
            dropped: Rc::clone(dropped),
            receiver: None,
        }
    }
}
impl Payload for Value {
    fn receivers(
        &self,
        visit: &mut dyn FnMut(&Receiver) -> Result<(), Error>,
    ) -> Result<(), Error> {
        if let Some(receiver) = &self.receiver {
            visit(receiver)?;
        }
        Ok(())
    }
}
impl Drop for Value {
    fn drop(&mut self) {
        self.dropped.borrow_mut().push(self.data[0]);
    }
}

fn scheduler() -> (t::Scheduler, t::TaskId) {
    let mut s = t::Scheduler::new(t::Limits {
        live_tasks: 8,
        total_tasks: 8,
    })
    .unwrap();
    let root = s.start(None).unwrap();
    assert_eq!(
        s.dispatch().unwrap(),
        Some(t::Dispatch::Resume(root.clone()))
    );
    (s, root)
}
fn resume(s: &mut t::Scheduler, task: &t::TaskId) {
    assert_eq!(
        s.dispatch().unwrap(),
        Some(t::Dispatch::Resume(task.clone()))
    );
}
fn cleanup(s: &mut t::Scheduler, task: &t::TaskId) {
    assert_eq!(
        s.dispatch().unwrap(),
        Some(t::Dispatch::Cleanup {
            task: task.clone(),
            outcome: t::Outcome::Success
        })
    );
    s.finish_cleanup(task, None).unwrap();
}

#[test]
fn hub_moves_owned_payload_through_a_parked_receiver_and_keeps_its_allocation() {
    let (mut s, root) = scheduler();
    let mut hub =
        Hub::<Vec<u8>>::prepare(&s, &root, Limits::default(), e::ServiceWork::new(None)).unwrap();
    let setup = hub.create(1).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let mut receiver = hub.receiver(setup).unwrap();
    let producer = s.spawn(&root, None).unwrap();
    let Receive::Waiting(wait) = hub.receive(&mut s, &root, &mut receiver, None).unwrap() else {
        panic!("receiver must park")
    };
    resume(&mut s, &producer);
    let value = vec![42];
    let allocation = value.as_ptr();
    assert!(matches!(
        hub.send(&mut s, &producer, &sender, value, None).unwrap(),
        Send::Sent { position: 0 }
    ));
    hub.release_sender(&mut s, &sender).unwrap();
    s.finish(&producer, t::Outcome::Success).unwrap();
    resume(&mut s, &root);
    let Receive::Value { value, position } = hub.resume_receive(&mut s, &wait).unwrap() else {
        panic!("expected committed value")
    };
    assert_eq!(value, [42]);
    assert_eq!(value.as_ptr(), allocation);
    assert_eq!(position, 0);
    assert_eq!(hub.usage().pending, 0);
    assert_eq!(hub.usage().queued, 0);
    assert!(matches!(
        hub.receive(&mut s, &root, &mut receiver, None).unwrap(),
        Receive::Closed
    ));
    drop(value);
    hub.abort_receiver(
        &mut s,
        &receiver,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut |_: &mut Vec<u8>, _| Ok(()),
    )
    .unwrap();
    assert_eq!(hub.usage().reserved_slots, 0);
    s.finish(&root, t::Outcome::Success).unwrap();
    cleanup(&mut s, &producer);
    cleanup(&mut s, &root);
    assert_eq!(s.usage().live, 0);
}

#[test]
fn late_ready_sender_cannot_overtake_the_woken_head_and_values_sum_to_42() {
    let (mut s, root) = scheduler();
    let dropped = Rc::new(RefCell::new(Vec::new()));
    let work = e::ServiceWork::new(None);
    let mut hub = Hub::<Value>::prepare(&s, &root, Limits::default(), work.clone()).unwrap();
    let setup = hub.create(1).unwrap();
    let initial = hub.sender(&setup).unwrap();
    let older_sender = hub.clone_sender(&initial).unwrap();
    let later_sender = hub.clone_sender(&initial).unwrap();
    let mut receiver = hub.receiver(setup).unwrap();
    let older = s.spawn(&root, None).unwrap();
    let later = s.spawn(&root, None).unwrap();
    assert!(matches!(
        hub.send(&mut s, &root, &initial, Value::new(0, &dropped), None)
            .unwrap(),
        Send::Sent { position: 0 }
    ));
    hub.release_sender(&mut s, &initial).unwrap();
    s.yield_now(&root).unwrap();
    resume(&mut s, &older);
    let Send::Waiting(old) = hub
        .send(
            &mut s,
            &older,
            &older_sender,
            Value::new(20, &dropped),
            None,
        )
        .unwrap()
    else {
        panic!("full send must park")
    };
    resume(&mut s, &later);
    s.yield_now(&later).unwrap();
    resume(&mut s, &root);
    let Receive::Value { value, position: 0 } =
        hub.receive(&mut s, &root, &mut receiver, None).unwrap()
    else {
        panic!("first value")
    };
    drop(value);
    s.yield_now(&root).unwrap();
    // Later was ready before older's wake. The vacant physical slot remains
    // reserved for older, so later must join behind it rather than commit.
    resume(&mut s, &later);
    let Send::Waiting(late) = hub
        .send(
            &mut s,
            &later,
            &later_sender,
            Value::new(22, &dropped),
            None,
        )
        .unwrap()
    else {
        panic!("late send must not overtake")
    };
    resume(&mut s, &older);
    assert!(matches!(
        hub.resume_send(&mut s, &old).unwrap(),
        Send::Sent { position: 1 }
    ));
    assert_eq!(
        hub.resume_send(&mut s, &old).unwrap_err(),
        Error::StaleOperation
    );
    hub.release_sender(&mut s, &older_sender).unwrap();
    s.finish(&older, t::Outcome::Success).unwrap();
    resume(&mut s, &root);
    let Receive::Value { value, position: 1 } =
        hub.receive(&mut s, &root, &mut receiver, None).unwrap()
    else {
        panic!("older value")
    };
    let mut sum = value.data[0] as i64;
    drop(value);
    s.yield_now(&root).unwrap();
    cleanup(&mut s, &older);
    resume(&mut s, &later);
    assert!(matches!(
        hub.resume_send(&mut s, &late).unwrap(),
        Send::Sent { position: 2 }
    ));
    hub.release_sender(&mut s, &later_sender).unwrap();
    s.finish(&later, t::Outcome::Success).unwrap();
    resume(&mut s, &root);
    let Receive::Value { value, position: 2 } =
        hub.receive(&mut s, &root, &mut receiver, None).unwrap()
    else {
        panic!("later value")
    };
    sum += value.data[0] as i64;
    drop(value);
    assert_eq!(sum, 42);
    assert!(matches!(
        hub.receive(&mut s, &root, &mut receiver, None).unwrap(),
        Receive::Closed
    ));
    assert_eq!(work.used(), 20);
    hub.abort_receiver(
        &mut s,
        &receiver,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut |_: &mut Value, _| Ok(()),
    )
    .unwrap();
    assert_eq!(*dropped.borrow(), [0, 20, 22]);
    assert_eq!(hub.usage().pending, 0);
    assert_eq!(hub.usage().queued, 0);
    assert_eq!(hub.usage().handles, 0);
    assert_eq!(hub.usage().reserved_slots, 0);
    s.finish(&root, t::Outcome::Success).unwrap();
    cleanup(&mut s, &later);
    cleanup(&mut s, &root);
}

#[test]
fn seal_returns_uncommitted_payload_and_drains_committed_values_fifo() {
    let (mut s, root) = scheduler();
    let dropped = Rc::new(RefCell::new(Vec::new()));
    let mut hub =
        Hub::<Value>::prepare(&s, &root, Limits::default(), e::ServiceWork::new(None)).unwrap();
    let setup = hub.create(2).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let waiting_sender = hub.clone_sender(&sender).unwrap();
    let mut receiver = hub.receiver(setup).unwrap();
    for id in [1, 2] {
        assert!(matches!(
            hub.send(&mut s, &root, &sender, Value::new(id, &dropped), None)
                .unwrap(),
            Send::Sent { .. }
        ));
    }
    let child = s.spawn(&root, None).unwrap();
    s.yield_now(&root).unwrap();
    resume(&mut s, &child);
    let Send::Waiting(operation) = hub
        .send(
            &mut s,
            &child,
            &waiting_sender,
            Value::new(3, &dropped),
            None,
        )
        .unwrap()
    else {
        panic!("full send")
    };
    resume(&mut s, &root);
    hub.seal(&mut s, &receiver).unwrap();
    hub.seal(&mut s, &receiver).unwrap();
    for expected in [1, 2] {
        let Receive::Value { value, .. } = hub.receive(&mut s, &root, &mut receiver, None).unwrap()
        else {
            panic!("drain")
        };
        assert_eq!(value.data[0], expected);
        drop(value);
    }
    assert!(matches!(
        hub.receive(&mut s, &root, &mut receiver, None).unwrap(),
        Receive::Closed
    ));
    s.yield_now(&root).unwrap();
    resume(&mut s, &child);
    let Send::Rejected {
        reason: Rejection::Closed,
        value,
    } = hub.resume_send(&mut s, &operation).unwrap()
    else {
        panic!("closed pending send must return its owner")
    };
    assert_eq!(value.data, [3]);
    drop(value);
    hub.release_sender(&mut s, &waiting_sender).unwrap();
    s.finish(&child, t::Outcome::Success).unwrap();
    resume(&mut s, &root);
    hub.release_sender(&mut s, &sender).unwrap();
    hub.abort_receiver(
        &mut s,
        &receiver,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut |_: &mut Value, _| Ok(()),
    )
    .unwrap();
    assert_eq!(*dropped.borrow(), [1, 2, 3]);
    assert_eq!(hub.usage().queued, 0);
    assert_eq!(hub.usage().pending, 0);
    s.finish(&root, t::Outcome::Success).unwrap();
    cleanup(&mut s, &child);
    cleanup(&mut s, &root);
}

#[test]
fn abort_fifo_continues_after_cleanup_panic_and_report_truncation_without_work() {
    let (mut s, root) = scheduler();
    let dropped = Rc::new(RefCell::new(Vec::new()));
    let work = e::ServiceWork::new(Some(13));
    let mut hub = Hub::<Value>::prepare(&s, &root, Limits::default(), work.clone()).unwrap();
    let setup = hub.create(3).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let mut receiver = hub.receiver(setup).unwrap();
    for id in 1..=3 {
        assert!(matches!(
            hub.send(&mut s, &root, &sender, Value::new(id, &dropped), None)
                .unwrap(),
            Send::Sent { .. }
        ));
    }
    assert_eq!(work.used(), 13);
    let mut report = session(1);
    let mut attempts = Vec::new();
    hub.abort_receiver(
        &mut s,
        &receiver,
        &mut report,
        c::Reason::Failure,
        &mut |value: &mut Value, reason| {
            assert_eq!(reason, c::Reason::Failure);
            work.charge(u64::MAX).unwrap();
            attempts.push(value.data[0]);
            if value.data[0] == 2 {
                panic!("trusted cleanup panic")
            }
            Err(value.data[0] as u32)
        },
    )
    .unwrap();
    assert_eq!(attempts, [1, 2, 3]);
    // Session quarantines the unknown callback-panic payload; it must not run
    // its destructor a second time. Independent values still retire FIFO.
    assert_eq!(*dropped.borrow(), [1, 3]);
    assert!(report.is_poisoned());
    assert_eq!(report.report().cleanup_failures(), 3);
    assert_eq!(report.report().quarantined(), 1);
    assert_eq!(report.report().truncated(), 2);
    assert_eq!(work.used(), 13);
    assert_eq!(hub.usage().queued, 0);
    assert_eq!(hub.usage().reserved_slots, 0);
    assert_eq!(
        hub.receive(&mut s, &root, &mut receiver, None).unwrap_err(),
        Error::WrongRole
    );
    hub.release_sender(&mut s, &sender).unwrap();
    s.finish(&root, t::Outcome::Success).unwrap();
    cleanup(&mut s, &root);
}

#[test]
fn cancelling_head_before_or_after_wake_releases_its_turn_and_preserves_later_payload() {
    for after_wake in [false, true] {
        let (mut s, root) = scheduler();
        let dropped = Rc::new(RefCell::new(Vec::new()));
        let mut hub =
            Hub::<Value>::prepare(&s, &root, Limits::default(), e::ServiceWork::new(None)).unwrap();
        let setup = hub.create(1).unwrap();
        let sender = hub.sender(&setup).unwrap();
        let first_sender = hub.clone_sender(&sender).unwrap();
        let second_sender = hub.clone_sender(&sender).unwrap();
        let mut receiver = hub.receiver(setup).unwrap();
        let first = s.spawn(&root, None).unwrap();
        let second = s.spawn(&root, None).unwrap();
        assert!(matches!(
            hub.send(&mut s, &root, &sender, Value::new(0, &dropped), None)
                .unwrap(),
            Send::Sent { .. }
        ));
        s.yield_now(&root).unwrap();
        resume(&mut s, &first);
        let Send::Waiting(a) = hub
            .send(&mut s, &first, &first_sender, Value::new(1, &dropped), None)
            .unwrap()
        else {
            panic!("first wait")
        };
        resume(&mut s, &second);
        let Send::Waiting(b) = hub
            .send(
                &mut s,
                &second,
                &second_sender,
                Value::new(2, &dropped),
                None,
            )
            .unwrap()
        else {
            panic!("second wait")
        };
        resume(&mut s, &root);
        if after_wake {
            let Receive::Value { value, .. } =
                hub.receive(&mut s, &root, &mut receiver, None).unwrap()
            else {
                panic!("vacate")
            };
            drop(value);
        }
        let Cancelled::Send(value) = hub.cancel(&mut s, &a).unwrap() else {
            panic!("recover cancelled owner")
        };
        assert_eq!(value.data, [1]);
        drop(value);
        assert_eq!(hub.cancel(&mut s, &a).unwrap_err(), Error::StaleOperation);
        if !after_wake {
            let Receive::Value { value, .. } =
                hub.receive(&mut s, &root, &mut receiver, None).unwrap()
            else {
                panic!("vacate")
            };
            drop(value);
        }
        assert_eq!(s.status(&second), Ok(t::Status::Ready));
        hub.release_sender(&mut s, &first_sender).unwrap();
        s.yield_now(&root).unwrap();
        assert_eq!(
            s.dispatch().unwrap(),
            Some(t::Dispatch::Cleanup {
                task: first.clone(),
                outcome: t::Outcome::Cancelled
            })
        );
        s.finish_cleanup(&first, None).unwrap();
        resume(&mut s, &second);
        assert!(matches!(
            hub.resume_send(&mut s, &b).unwrap(),
            Send::Sent { position: 1 }
        ));
        hub.release_sender(&mut s, &second_sender).unwrap();
        s.finish(&second, t::Outcome::Success).unwrap();
        // Cancellation after commit cannot retract the already queued owner.
        s.request_cancel(&second).unwrap();
        resume(&mut s, &root);
        let Receive::Value { value, .. } = hub.receive(&mut s, &root, &mut receiver, None).unwrap()
        else {
            panic!("committed value survives")
        };
        assert_eq!(value.data, [2]);
        drop(value);
        hub.release_sender(&mut s, &sender).unwrap();
        hub.abort_receiver(
            &mut s,
            &receiver,
            &mut session(1),
            c::Reason::ScopeExit,
            &mut |_: &mut Value, _| Ok(()),
        )
        .unwrap();
        s.finish(&root, t::Outcome::Success).unwrap();
        assert_eq!(
            s.dispatch().unwrap(),
            Some(t::Dispatch::Cleanup {
                task: second.clone(),
                outcome: t::Outcome::Cancelled
            })
        );
        s.finish_cleanup(&second, None).unwrap();
        cleanup(&mut s, &root);
        let mut inventory = dropped.borrow().clone();
        inventory.sort();
        assert_eq!(inventory, [0, 1, 2]);
        assert_eq!(hub.usage().pending, 0);
        assert_eq!(hub.usage().queued, 0);
        assert_eq!(hub.usage().reserved_slots, 0);
        assert_eq!(s.usage().live, 0);
    }
}

#[test]
fn receiver_cancel_and_closed_deadlock_keep_origins_and_never_publish_a_fake_value() {
    let (mut s, root) = scheduler();
    let mut hub =
        Hub::<i64>::prepare(&s, &root, Limits::default(), e::ServiceWork::new(None)).unwrap();
    let setup = hub.create(1).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let mut receiver = hub.receiver(setup).unwrap();
    let span = Span {
        source: lang_mir::unit::SourceId(8),
        start: 17,
        end: 29,
    };
    assert!(hub.deadlock(&s).unwrap().is_none());
    let Receive::Waiting(operation) = hub
        .receive(&mut s, &root, &mut receiver, Some(span))
        .unwrap()
    else {
        panic!("open empty receive")
    };
    assert!(s.is_quiescent());
    let deadlock = hub.deadlock(&s).unwrap().unwrap();
    assert_eq!(deadlock.operation, operation);
    assert_eq!(deadlock.operation.origin(), Some(span));
    assert_eq!(hub.usage().queued, 0);
    assert!(matches!(
        hub.cancel(&mut s, &operation).unwrap(),
        Cancelled::Receive
    ));
    hub.release_sender(&mut s, &sender).unwrap();
    hub.abort_receiver(
        &mut s,
        &receiver,
        &mut session(1),
        c::Reason::Failure,
        &mut |_: &mut i64, _| Ok(()),
    )
    .unwrap();
    assert_eq!(
        s.dispatch().unwrap(),
        Some(t::Dispatch::Cleanup {
            task: root.clone(),
            outcome: t::Outcome::Cancelled
        })
    );
    s.finish_cleanup(&root, None).unwrap();
    assert!(hub.deadlock(&s).unwrap().is_none());
    assert_eq!(hub.usage().pending, 0);
    assert_eq!(hub.usage().reserved_slots, 0);
}

#[test]
fn direct_and_transitive_receiver_cycles_refuse_before_enqueue_and_keep_the_owner() {
    let (mut s, root) = scheduler();
    let dropped = Rc::new(RefCell::new(Vec::new()));
    let mut hub =
        Hub::<Value>::prepare(&s, &root, Limits::default(), e::ServiceWork::new(None)).unwrap();
    let a = hub.create(2).unwrap();
    let sa = hub.sender(&a).unwrap();
    let ra = hub.receiver(a).unwrap();
    let mut value = Value::new(1, &dropped);
    value.receiver = Some(ra);
    let Send::Rejected {
        reason: Rejection::InvalidPublication,
        mut value,
    } = hub.send(&mut s, &root, &sa, value, None).unwrap()
    else {
        panic!("direct cycle")
    };
    assert_eq!(hub.usage().queued, 0);
    let ra = value.receiver.take().unwrap();
    drop(value);
    let b = hub.create(2).unwrap();
    let sb = hub.sender(&b).unwrap();
    let rb = hub.receiver(b).unwrap();
    let mut a_to_b = Value::new(2, &dropped);
    a_to_b.receiver = Some(rb);
    assert!(matches!(
        hub.send(&mut s, &root, &sa, a_to_b, None).unwrap(),
        Send::Sent { position: 0 }
    ));
    let mut b_to_a = Value::new(3, &dropped);
    b_to_a.receiver = Some(ra);
    let Send::Rejected {
        reason: Rejection::InvalidPublication,
        mut value,
    } = hub.send(&mut s, &root, &sb, b_to_a, None).unwrap()
    else {
        panic!("transitive cycle")
    };
    assert_eq!(hub.usage().queued, 1);
    let mut ra = value.receiver.take().unwrap();
    drop(value);
    let Receive::Value { mut value, .. } = hub.receive(&mut s, &root, &mut ra, None).unwrap()
    else {
        panic!("acyclic owning handle transfer")
    };
    let rb = value.receiver.take().unwrap();
    drop(value);
    hub.release_sender(&mut s, &sa).unwrap();
    hub.release_sender(&mut s, &sb).unwrap();
    let mut cleaner = |_: &mut Value, _| Ok(());
    hub.abort_receiver(
        &mut s,
        &ra,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut cleaner,
    )
    .unwrap();
    hub.abort_receiver(
        &mut s,
        &rb,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut cleaner,
    )
    .unwrap();
    assert_eq!(*dropped.borrow(), [1, 3, 2]);
    assert_eq!(hub.usage().queued, 0);
    assert_eq!(hub.usage().reserved_slots, 0);
    s.finish(&root, t::Outcome::Success).unwrap();
    cleanup(&mut s, &root);
}

#[test]
fn every_service_work_prefix_keeps_each_owner_and_does_not_meter_mandatory_abort() {
    for limit in 0..=7 {
        let (mut s, root) = scheduler();
        let dropped = Rc::new(RefCell::new(Vec::new()));
        let work = e::ServiceWork::new(Some(limit));
        let mut hub = Hub::<Value>::prepare(&s, &root, Limits::default(), work.clone()).unwrap();
        let mut report = session(1);
        let mut cleaner = |_: &mut Value, _| Ok(());
        let result = match hub.create(1) {
            Err(error) => Err(error),
            Ok(setup) => match hub.sender(&setup) {
                Err(error) => {
                    hub.abort_setup(
                        &mut s,
                        &setup,
                        &mut report,
                        c::Reason::Failure,
                        &mut cleaner,
                    )
                    .unwrap();
                    Err(error)
                }
                Ok(sender) => {
                    let mut receiver = hub.receiver(setup).unwrap();
                    let result =
                        match hub.send(&mut s, &root, &sender, Value::new(42, &dropped), None) {
                            Err(Failure { error, value }) => {
                                drop(value);
                                Err(error)
                            }
                            Ok(Send::Sent { .. }) => {
                                match hub.receive(&mut s, &root, &mut receiver, None) {
                                    Err(error) => Err(error),
                                    Ok(Receive::Value { value, .. }) => {
                                        let result = value.data[0];
                                        drop(value);
                                        Ok(result)
                                    }
                                    _ => panic!("committed value cannot vanish"),
                                }
                            }
                            _ => panic!("open channel has room"),
                        };
                    let used = work.used();
                    hub.abort_receiver(
                        &mut s,
                        &receiver,
                        &mut report,
                        c::Reason::Failure,
                        &mut cleaner,
                    )
                    .unwrap();
                    hub.release_sender(&mut s, &sender).unwrap();
                    assert_eq!(work.used(), used);
                    result
                }
            },
        };
        let outcome = if limit == 7 {
            assert_eq!(result, Ok(42));
            t::Outcome::Success
        } else {
            assert_eq!(
                result,
                Err(Error::Work(e::Failure::ServiceWorkLimit {
                    limit,
                    used: limit,
                    requested: 1
                }))
            );
            t::Outcome::Failed(t::FailureId(1))
        };
        assert_eq!(work.used(), limit);
        assert_eq!(dropped.borrow().len(), usize::from(limit >= 2));
        assert!(!report.is_poisoned());
        assert_eq!(hub.usage().queued, 0);
        assert_eq!(hub.usage().pending, 0);
        assert_eq!(hub.usage().reserved_slots, 0);
        assert_eq!(hub.usage().handles, 0);
        s.finish(&root, outcome).unwrap();
        assert_eq!(
            s.dispatch().unwrap(),
            Some(t::Dispatch::Cleanup {
                task: root.clone(),
                outcome
            })
        );
        s.finish_cleanup(&root, None).unwrap();
        assert!(s.is_finished());
    }
}

#[test]
fn metadata_limits_are_preflighted_and_sender_ticket_indices_are_not_reused() {
    let (mut s, root) = scheduler();
    for (limits, expected) in [
        (
            Limits {
                channels: 0,
                ..Limits::default()
            },
            Error::ChannelLimit { limit: 0 },
        ),
        (
            Limits {
                slots: 0,
                ..Limits::default()
            },
            Error::SlotLimit { limit: 0 },
        ),
    ] {
        let work = e::ServiceWork::new(None);
        let mut hub = Hub::<i64>::prepare(&s, &root, limits, work.clone()).unwrap();
        assert_eq!(hub.create(1).unwrap_err(), expected);
        assert_eq!(work.used(), 0);
        assert_eq!(hub.usage(), Usage::default());
    }
    assert!(matches!(
        Hub::<i64>::prepare(
            &s,
            &root,
            Limits {
                channels: usize::MAX,
                ..Limits::default()
            },
            e::ServiceWork::new(None)
        ),
        Err(Error::Capacity)
    ));
    let mut hub = Hub::<i64>::prepare(
        &s,
        &root,
        Limits {
            channels: 1,
            slots: 1,
            handles: 1,
            waiters: 0,
        },
        e::ServiceWork::new(None),
    )
    .unwrap();
    for capacity in [-1, 0] {
        assert_eq!(hub.create(capacity).unwrap_err(), Error::InvalidCapacity);
    }
    assert_eq!(
        hub.create(i64::MAX).unwrap_err(),
        Error::SlotLimit { limit: 1 }
    );
    let setup = hub.create(1).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let before = hub.usage();
    assert_eq!(
        hub.clone_sender(&sender).unwrap_err(),
        Error::HandleLimit { limit: 1 }
    );
    assert_eq!(hub.usage(), before);
    hub.release_sender(&mut s, &sender).unwrap();
    assert_eq!(
        hub.release_sender(&mut s, &sender),
        Err(Error::InvalidEndpoint)
    );
    assert_eq!(
        hub.clone_sender(&sender).unwrap_err(),
        Error::InvalidEndpoint
    );
    assert_eq!(
        hub.sender(&setup).unwrap_err(),
        Error::HandleLimit { limit: 1 }
    );
    let mut receiver = hub.receiver(setup).unwrap();
    assert!(matches!(
        hub.receive(&mut s, &root, &mut receiver, None).unwrap(),
        Receive::Closed
    ));
    hub.abort_receiver(
        &mut s,
        &receiver,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut |_: &mut i64, _| Ok(()),
    )
    .unwrap();
    assert_eq!(hub.create(1).unwrap_err(), Error::ChannelLimit { limit: 1 });
    assert_eq!(hub.usage().reserved_slots, 0);
    assert_eq!(hub.usage().handles, 0);
    s.finish(&root, t::Outcome::Success).unwrap();
    cleanup(&mut s, &root);
}

#[test]
fn foreign_endpoint_and_scheduler_refusals_preserve_roles_and_payloads() {
    let (mut s, root) = scheduler();
    let (mut other, other_root) = scheduler();
    let mut a =
        Hub::<i64>::prepare(&s, &root, Limits::default(), e::ServiceWork::new(None)).unwrap();
    let mut b = Hub::<i64>::prepare(
        &other,
        &other_root,
        Limits::default(),
        e::ServiceWork::new(None),
    )
    .unwrap();
    let setup = a.create(1).unwrap();
    let foreign = b.create(1).unwrap();
    let sender = a.sender(&setup).unwrap();
    let foreign_sender = b.sender(&foreign).unwrap();
    let before = a.usage();
    assert_eq!(a.sender(&foreign).unwrap_err(), Error::ForeignEndpoint);
    let Failure { error, value } = a
        .send(&mut s, &root, &foreign_sender, 42, None)
        .unwrap_err();
    assert_eq!((error, value), (Error::ForeignEndpoint, 42));
    assert_eq!(
        a.abort_setup(
            &mut other,
            &setup,
            &mut session(1),
            c::Reason::Failure,
            &mut |_: &mut i64, _| Ok(())
        ),
        Err(Error::Task(t::Error::ForeignTask))
    );
    assert_eq!(a.usage(), before);
    // The refused abort must not consume setup's producer keepalive.
    assert_eq!(a.channels[0].producers, 2);
    let receiver = a.receiver(setup).unwrap();
    a.release_sender(&mut s, &sender).unwrap();
    a.abort_receiver(
        &mut s,
        &receiver,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut |_: &mut i64, _| Ok(()),
    )
    .unwrap();
    b.release_sender(&mut other, &foreign_sender).unwrap();
    b.abort_setup(
        &mut other,
        &foreign,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut |_: &mut i64, _| Ok(()),
    )
    .unwrap();
    for (scheduler, task) in [(&mut s, root), (&mut other, other_root)] {
        scheduler.finish(&task, t::Outcome::Success).unwrap();
        cleanup(scheduler, &task);
    }
}

#[test]
fn publication_is_rechecked_after_wake_when_another_queue_acquires_a_receiver() {
    let (mut s, root) = scheduler();
    let dropped = Rc::new(RefCell::new(Vec::new()));
    let mut hub =
        Hub::<Value>::prepare(&s, &root, Limits::default(), e::ServiceWork::new(None)).unwrap();
    let a = hub.create(1).unwrap();
    let sa = hub.sender(&a).unwrap();
    let mut ra = hub.receiver(a).unwrap();
    let b = hub.create(1).unwrap();
    let sb = hub.sender(&b).unwrap();
    let rb = hub.receiver(b).unwrap();
    assert!(matches!(
        hub.send(&mut s, &root, &sa, Value::new(0, &dropped), None)
            .unwrap(),
        Send::Sent { .. }
    ));
    let child = s.spawn(&root, None).unwrap();
    s.yield_now(&root).unwrap();
    resume(&mut s, &child);
    let mut incoming = Value::new(1, &dropped);
    incoming.receiver = Some(rb);
    let Send::Waiting(operation) = hub.send(&mut s, &child, &sa, incoming, None).unwrap() else {
        panic!("full A")
    };
    resume(&mut s, &root);
    let Receive::Value { value, .. } = hub.receive(&mut s, &root, &mut ra, None).unwrap() else {
        panic!("vacate A")
    };
    drop(value);
    let mut change = Value::new(2, &dropped);
    change.receiver = Some(ra);
    assert!(matches!(
        hub.send(&mut s, &root, &sb, change, None).unwrap(),
        Send::Sent { .. }
    ));
    s.yield_now(&root).unwrap();
    resume(&mut s, &child);
    let Send::Rejected {
        reason: Rejection::InvalidPublication,
        mut value,
    } = hub.resume_send(&mut s, &operation).unwrap()
    else {
        panic!("new B -> A edge makes pending A -> B cyclic")
    };
    let mut rb = value.receiver.take().unwrap();
    drop(value);
    let Receive::Value { mut value, .. } = hub.receive(&mut s, &child, &mut rb, None).unwrap()
    else {
        panic!("recover acyclic receiver")
    };
    let ra = value.receiver.take().unwrap();
    drop(value);
    hub.release_sender(&mut s, &sa).unwrap();
    hub.release_sender(&mut s, &sb).unwrap();
    for receiver in [&ra, &rb] {
        hub.abort_receiver(
            &mut s,
            receiver,
            &mut session(1),
            c::Reason::ScopeExit,
            &mut |_: &mut Value, _| Ok(()),
        )
        .unwrap();
    }
    s.finish(&child, t::Outcome::Success).unwrap();
    resume(&mut s, &root);
    s.finish(&root, t::Outcome::Success).unwrap();
    cleanup(&mut s, &child);
    cleanup(&mut s, &root);
    assert_eq!(*dropped.borrow(), [0, 1, 2]);
    assert_eq!(hub.usage().pending, 0);
    assert_eq!(hub.usage().queued, 0);
    assert_eq!(hub.usage().reserved_slots, 0);
}

#[test]
fn one_waiter_slot_is_reused_without_accepting_an_old_completion_claim() {
    let (mut s, root) = scheduler();
    let mut hub = Hub::<i64>::prepare(
        &s,
        &root,
        Limits {
            waiters: 1,
            ..Limits::default()
        },
        e::ServiceWork::new(None),
    )
    .unwrap();
    let setup = hub.create(1).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let mut receiver = hub.receiver(setup).unwrap();
    let storage = (
        hub.channels.as_ptr(),
        hub.tickets.as_ptr(),
        hub.pending.as_ptr(),
        hub.pending.capacity(),
        hub.inspection.capacity(),
    );
    let producer = s.spawn(&root, None).unwrap();
    let Receive::Waiting(old) = hub.receive(&mut s, &root, &mut receiver, None).unwrap() else {
        panic!("first wait")
    };
    resume(&mut s, &producer);
    assert!(matches!(
        hub.send(&mut s, &producer, &sender, 41, None).unwrap(),
        Send::Sent { .. }
    ));
    s.yield_now(&producer).unwrap();
    resume(&mut s, &root);
    assert!(matches!(
        hub.resume_receive(&mut s, &old).unwrap(),
        Receive::Value { value: 41, .. }
    ));
    let Receive::Waiting(new) = hub.receive(&mut s, &root, &mut receiver, None).unwrap() else {
        panic!("second wait")
    };
    assert!(new.ordinal() > old.ordinal());
    resume(&mut s, &producer);
    assert_eq!(
        hub.resume_receive(&mut s, &old).unwrap_err(),
        Error::StaleOperation
    );
    assert!(matches!(
        hub.send(&mut s, &producer, &sender, 42, None).unwrap(),
        Send::Sent { .. }
    ));
    hub.release_sender(&mut s, &sender).unwrap();
    s.finish(&producer, t::Outcome::Success).unwrap();
    resume(&mut s, &root);
    assert!(matches!(
        hub.resume_receive(&mut s, &new).unwrap(),
        Receive::Value { value: 42, .. }
    ));
    assert_eq!(
        (
            hub.channels.as_ptr(),
            hub.tickets.as_ptr(),
            hub.pending.as_ptr(),
            hub.pending.capacity(),
            hub.inspection.capacity()
        ),
        storage
    );
    hub.abort_receiver(
        &mut s,
        &receiver,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut |_: &mut i64, _| Ok(()),
    )
    .unwrap();
    s.finish(&root, t::Outcome::Success).unwrap();
    cleanup(&mut s, &producer);
    cleanup(&mut s, &root);
}

#[test]
fn waiter_refusal_preserves_input_and_position_and_count_exhaustion_publish_nothing() {
    let (mut s, root) = scheduler();
    let mut hub = Hub::<i64>::prepare(
        &s,
        &root,
        Limits {
            waiters: 0,
            ..Limits::default()
        },
        e::ServiceWork::new(None),
    )
    .unwrap();
    let setup = hub.create(1).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let mut receiver = hub.receiver(setup).unwrap();
    assert!(matches!(
        hub.send(&mut s, &root, &sender, 1, None).unwrap(),
        Send::Sent { .. }
    ));
    let before = hub.usage();
    let Failure { error, value } = hub.send(&mut s, &root, &sender, 2, None).unwrap_err();
    assert_eq!((error, value), (Error::WaiterLimit { limit: 0 }, 2));
    assert_eq!(hub.usage(), before);
    assert_eq!(s.status(&root), Ok(t::Status::Running));
    assert!(matches!(
        hub.receive(&mut s, &root, &mut receiver, None).unwrap(),
        Receive::Value { value: 1, .. }
    ));
    hub.channels[0].values.exhaust_positions();
    let before = hub.usage();
    let Failure { error, value } = hub.send(&mut s, &root, &sender, 42, None).unwrap_err();
    assert_eq!((error, value), (Error::PositionExhausted, 42));
    assert_eq!(hub.usage(), before);
    let producers = hub.channels[0].producers;
    hub.channels[0].producers = usize::MAX;
    assert_eq!(
        hub.clone_sender(&sender).unwrap_err(),
        Error::CountExhausted
    );
    assert_eq!(hub.usage(), before);
    hub.channels[0].producers = producers;
    hub.release_sender(&mut s, &sender).unwrap();
    hub.abort_receiver(
        &mut s,
        &receiver,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut |_: &mut i64, _| Ok(()),
    )
    .unwrap();
    s.finish(&root, t::Outcome::Success).unwrap();
    cleanup(&mut s, &root);
}

#[test]
fn every_backpressure_work_prefix_aborts_pending_and_committed_owners_once() {
    for limit in 0..=13 {
        let (mut s, root) = scheduler();
        let dropped = Rc::new(RefCell::new(Vec::new()));
        let work = e::ServiceWork::new(Some(limit));
        let mut hub = Hub::<Value>::prepare(&s, &root, Limits::default(), work.clone()).unwrap();
        let mut setup = None;
        let mut sender = None;
        let mut receiver = None;
        let mut producer = None;
        let mut sender_live = false;
        let mut active = root.clone();
        let mut report = session(2);
        let result = (|| -> Result<i64, Error> {
            setup = Some(hub.create(1)?);
            sender = Some(hub.sender(setup.as_ref().unwrap())?);
            sender_live = true;
            receiver = Some(hub.receiver(setup.take().unwrap()).unwrap());
            producer = Some(s.spawn(&root, None)?);
            let Receive::Waiting(mut root_wait) =
                hub.receive(&mut s, &root, receiver.as_mut().unwrap(), None)?
            else {
                panic!("initial wait")
            };
            let mut producer_wait = None;
            let mut root_stage = 0;
            let mut producer_stage = 0;
            let mut sum = 0;
            while let Some(dispatch) = s.dispatch()? {
                match dispatch {
                    t::Dispatch::Resume(task) => {
                        active = task.clone();
                        if task == root {
                            let Receive::Value { value, .. } =
                                hub.resume_receive(&mut s, &root_wait)?
                            else {
                                panic!("received value")
                            };
                            sum += value.data[0] as i64;
                            drop(value);
                            if root_stage == 0 {
                                let Receive::Waiting(next) =
                                    hub.receive(&mut s, &root, receiver.as_mut().unwrap(), None)?
                                else {
                                    panic!("second wait")
                                };
                                root_wait = next;
                            } else {
                                assert!(matches!(
                                    hub.receive(&mut s, &root, receiver.as_mut().unwrap(), None)?,
                                    Receive::Closed
                                ));
                                s.finish(&root, t::Outcome::Success)?;
                            }
                            root_stage += 1;
                        } else if producer_stage == 0 {
                            let outcome = hub
                                .send(
                                    &mut s,
                                    &task,
                                    sender.as_ref().unwrap(),
                                    Value::new(20, &dropped),
                                    None,
                                )
                                .map_err(|Failure { error, value }| {
                                    drop(value);
                                    error
                                })?;
                            assert!(matches!(outcome, Send::Sent { .. }));
                            let Send::Waiting(operation) = hub
                                .send(
                                    &mut s,
                                    &task,
                                    sender.as_ref().unwrap(),
                                    Value::new(22, &dropped),
                                    None,
                                )
                                .map_err(|Failure { error, value }| {
                                    drop(value);
                                    error
                                })?
                            else {
                                panic!("full send")
                            };
                            producer_wait = Some(operation);
                            producer_stage = 1;
                        } else {
                            assert!(matches!(
                                hub.resume_send(&mut s, producer_wait.as_ref().unwrap())?,
                                Send::Sent { .. }
                            ));
                            hub.release_sender(&mut s, sender.as_ref().unwrap())?;
                            sender_live = false;
                            s.finish(&task, t::Outcome::Success)?;
                        }
                    }
                    t::Dispatch::Cleanup { task, outcome } => {
                        assert_eq!(outcome, t::Outcome::Success);
                        if task == root {
                            hub.abort_receiver(
                                &mut s,
                                receiver.as_ref().unwrap(),
                                &mut report,
                                c::Reason::ScopeExit,
                                &mut |_: &mut Value, _| Ok(()),
                            )?;
                        }
                        s.finish_cleanup(&task, None)?;
                    }
                }
            }
            Ok(sum)
        })();
        if limit == 13 {
            assert_eq!(result, Ok(42));
        } else {
            assert_eq!(
                result,
                Err(Error::Work(e::Failure::ServiceWorkLimit {
                    limit,
                    used: limit,
                    requested: 1
                }))
            );
            assert_eq!(s.status(&active), Ok(t::Status::Running));
            s.finish(&active, t::Outcome::Failed(t::FailureId(99)))
                .unwrap();
            s.request_cancel(&root).unwrap();
            while let Some(dispatch) = s.dispatch().unwrap() {
                let t::Dispatch::Cleanup { task, .. } = dispatch else {
                    panic!("global control failure cannot resume guest work")
                };
                if let Some(operation) = hub.pending_operation(&s, &task).unwrap()
                    && let Cancelled::Send(value) = hub.cancel(&mut s, &operation).unwrap()
                {
                    report
                        .retire(
                            c::Slot(task.index()),
                            value,
                            None,
                            c::Reason::Failure,
                            &mut |_: &mut Value, _| Ok(()),
                        )
                        .unwrap();
                }
                if producer.as_ref() == Some(&task) && sender_live {
                    hub.release_sender(&mut s, sender.as_ref().unwrap())
                        .unwrap();
                    sender_live = false;
                }
                if task == root {
                    if let Some(receiver) = &receiver {
                        hub.abort_receiver(
                            &mut s,
                            receiver,
                            &mut report,
                            c::Reason::Failure,
                            &mut |_: &mut Value, _| Ok(()),
                        )
                        .unwrap();
                    }
                    if let Some(setup) = &setup {
                        hub.abort_setup(
                            &mut s,
                            setup,
                            &mut report,
                            c::Reason::Failure,
                            &mut |_: &mut Value, _| Ok(()),
                        )
                        .unwrap();
                    }
                }
                s.finish_cleanup(&task, None).unwrap();
            }
        }
        assert_eq!(work.used(), limit);
        assert!(s.is_finished());
        assert_eq!(s.usage().live, 0);
        assert_eq!(
            (
                hub.usage().queued,
                hub.usage().pending,
                hub.usage().reserved_slots,
                hub.usage().handles
            ),
            (0, 0, 0, 0)
        );
        assert!(!report.is_poisoned());
        let mut inventory = dropped.borrow().clone();
        inventory.sort();
        let expected = if limit <= 2 {
            vec![]
        } else if limit <= 5 {
            vec![20]
        } else {
            vec![20, 22]
        };
        assert_eq!(inventory, expected, "work prefix {limit}");
    }
}

#[test]
fn two_freed_slots_still_advance_only_the_oldest_sender_turn() {
    let (mut s, root) = scheduler();
    let mut hub =
        Hub::<i64>::prepare(&s, &root, Limits::default(), e::ServiceWork::new(None)).unwrap();
    let setup = hub.create(2).unwrap();
    let sender = hub.sender(&setup).unwrap();
    let mut receiver = hub.receiver(setup).unwrap();
    for value in [1, 2] {
        assert!(matches!(
            hub.send(&mut s, &root, &sender, value, None).unwrap(),
            Send::Sent { .. }
        ));
    }
    let a = s.spawn(&root, None).unwrap();
    let b = s.spawn(&root, None).unwrap();
    s.yield_now(&root).unwrap();
    resume(&mut s, &a);
    let Send::Waiting(wa) = hub.send(&mut s, &a, &sender, 3, None).unwrap() else {
        panic!("first wait")
    };
    resume(&mut s, &b);
    let Send::Waiting(wb) = hub.send(&mut s, &b, &sender, 4, None).unwrap() else {
        panic!("second wait")
    };
    resume(&mut s, &root);
    for expected in [1, 2] {
        assert!(
            matches!(hub.receive(&mut s, &root, &mut receiver, None).unwrap(), Receive::Value { value, .. } if value == expected)
        );
    }
    assert_eq!(s.status(&a), Ok(t::Status::Ready));
    assert_eq!(s.status(&b), Ok(t::Status::Parked));
    s.yield_now(&root).unwrap();
    resume(&mut s, &a);
    assert!(matches!(
        hub.resume_send(&mut s, &wa).unwrap(),
        Send::Sent { position: 2 }
    ));
    assert_eq!(s.status(&b), Ok(t::Status::Ready));
    s.finish(&a, t::Outcome::Success).unwrap();
    resume(&mut s, &root);
    assert!(matches!(
        hub.receive(&mut s, &root, &mut receiver, None).unwrap(),
        Receive::Value { value: 3, .. }
    ));
    s.yield_now(&root).unwrap();
    resume(&mut s, &b);
    assert!(matches!(
        hub.resume_send(&mut s, &wb).unwrap(),
        Send::Sent { position: 3 }
    ));
    s.finish(&b, t::Outcome::Success).unwrap();
    cleanup(&mut s, &a);
    resume(&mut s, &root);
    assert!(matches!(
        hub.receive(&mut s, &root, &mut receiver, None).unwrap(),
        Receive::Value { value: 4, .. }
    ));
    hub.release_sender(&mut s, &sender).unwrap();
    hub.abort_receiver(
        &mut s,
        &receiver,
        &mut session(1),
        c::Reason::ScopeExit,
        &mut |_: &mut i64, _| Ok(()),
    )
    .unwrap();
    s.finish(&root, t::Outcome::Success).unwrap();
    cleanup(&mut s, &b);
    cleanup(&mut s, &root);
}

#[test]
fn deadlock_uses_registration_order_after_pool_reuse_not_slot_order() {
    let (mut s, root) = scheduler();
    let mut hub = Hub::<i64>::prepare(
        &s,
        &root,
        Limits {
            waiters: 2,
            ..Limits::default()
        },
        e::ServiceWork::new(None),
    )
    .unwrap();
    let setup_a = hub.create(1).unwrap();
    let sa = hub.sender(&setup_a).unwrap();
    let mut ra = hub.receiver(setup_a).unwrap();
    let setup_b = hub.create(1).unwrap();
    let sb = hub.sender(&setup_b).unwrap();
    let mut rb = hub.receiver(setup_b).unwrap();
    let child = s.spawn(&root, None).unwrap();
    let Receive::Waiting(old_root) = hub.receive(&mut s, &root, &mut ra, None).unwrap() else {
        panic!("root wait")
    };
    assert!(hub.deadlock(&s).unwrap().is_none());
    resume(&mut s, &child);
    let child_origin = Span {
        source: lang_mir::unit::SourceId(9),
        start: 4,
        end: 8,
    };
    let Receive::Waiting(child_wait) = hub
        .receive(&mut s, &child, &mut rb, Some(child_origin))
        .unwrap()
    else {
        panic!("child wait")
    };
    assert_eq!(hub.deadlock(&s).unwrap().unwrap().operation, old_root);
    hub.seal(&mut s, &ra).unwrap();
    resume(&mut s, &root);
    assert!(matches!(
        hub.resume_receive(&mut s, &old_root).unwrap(),
        Receive::Closed
    ));
    let setup_c = hub.create(1).unwrap();
    let sc = hub.sender(&setup_c).unwrap();
    let mut rc = hub.receiver(setup_c).unwrap();
    let Receive::Waiting(new_root) = hub.receive(&mut s, &root, &mut rc, None).unwrap() else {
        panic!("reused root slot")
    };
    assert!(new_root.ordinal() > child_wait.ordinal());
    let failure = hub.deadlock(&s).unwrap().unwrap();
    assert_eq!(failure.operation, child_wait);
    assert_eq!(failure.operation.origin(), Some(child_origin));
    assert!(matches!(
        hub.cancel(&mut s, &child_wait).unwrap(),
        Cancelled::Receive
    ));
    assert!(matches!(
        hub.cancel(&mut s, &new_root).unwrap(),
        Cancelled::Receive
    ));
    for sender in [&sa, &sb, &sc] {
        hub.release_sender(&mut s, sender).unwrap();
    }
    for receiver in [&ra, &rb, &rc] {
        hub.abort_receiver(
            &mut s,
            receiver,
            &mut session(1),
            c::Reason::Failure,
            &mut |_: &mut i64, _| Ok(()),
        )
        .unwrap();
    }
    for expected in [&child, &root] {
        assert_eq!(
            s.dispatch().unwrap(),
            Some(t::Dispatch::Cleanup {
                task: expected.clone(),
                outcome: t::Outcome::Cancelled
            })
        );
        s.finish_cleanup(expected, None).unwrap();
    }
    assert_eq!(hub.usage().pending, 0);
    assert_eq!(hub.usage().reserved_slots, 0);
}
