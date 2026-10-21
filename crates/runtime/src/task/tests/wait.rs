use super::*;

fn park(s: &mut Scheduler, task: &TaskId) -> WaitId {
    s.check_park(task).unwrap();
    let Park::Waiting(wait) = s.park(task).unwrap() else {
        panic!("uncancelled task should park");
    };
    assert_eq!(wait.task(), task);
    assert_eq!(s.status(task), Ok(Status::Parked));
    wait
}

#[test]
fn parked_root_is_quiescent_but_live_and_keeps_its_origin() {
    let mut s = scheduler();
    let origin = Span {
        source: lang_mir::unit::SourceId(7),
        start: 5,
        end: 11,
    };
    let root = s.start(Some(origin)).unwrap();
    resume(&mut s, &root);
    let wait = park(&mut s, &root);
    assert_eq!(wait.ordinal(), 1);
    assert_eq!(s.dispatch(), Ok(None));
    assert!(!s.is_finished());
    assert_eq!(s.usage().live, 1);
    assert_eq!(s.origin(&root), Ok(Some(origin)));
    assert_eq!(s.parent(&root), Ok(None));
    assert_eq!(s.check_park(&root), Err(Error::NoActiveTask));
    s.wake(&wait).unwrap();
    assert_eq!(s.status(&root), Ok(Status::Ready));
    resume(&mut s, &root);
    s.finish(&root, Outcome::Success).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
    assert_eq!(s.wake(&wait), Err(Error::StaleWait));
    assert_eq!(s.usage().live, 0);
    assert!(s.is_finished());
    assert_eq!(s.origin(&root), Ok(Some(origin)));
}

#[test]
fn wake_appends_fifo_once_without_replacing_an_active_dispatch() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let left = s.spawn(&root, None).unwrap();
    let right = s.spawn(&root, None).unwrap();
    let already_ready = s.spawn(&root, None).unwrap();
    let root_wait = park(&mut s, &root);
    resume(&mut s, &left);
    let left_wait = park(&mut s, &left);
    resume(&mut s, &right);
    let before = s.usage();
    s.wake(&left_wait).unwrap();
    s.wake(&root_wait).unwrap();
    assert_eq!(s.wake(&left_wait.clone()), Err(Error::StaleWait));
    assert_eq!(s.wake(&root_wait), Err(Error::StaleWait));
    assert_eq!(s.dispatch(), Err(Error::Busy));
    assert_eq!(s.status(&right), Ok(Status::Running));
    assert_eq!(s.usage(), before);
    s.finish(&right, Outcome::Success).unwrap();
    resume(&mut s, &already_ready);
    s.finish(&already_ready, Outcome::Success).unwrap();
    resume(&mut s, &left);
    s.finish(&left, Outcome::Success).unwrap();
    resume(&mut s, &root);
    s.finish(&root, Outcome::Success).unwrap();
    assert_eq!(s.status(&root), Ok(Status::Joining));
    cleanup(&mut s, &right, Outcome::Success);
    cleanup(&mut s, &already_ready, Outcome::Success);
    cleanup(&mut s, &left, Outcome::Success);
    cleanup(&mut s, &root, Outcome::Success);
    assert_eq!(s.dispatch(), Ok(None));
    assert_eq!(s.usage().live, 0);
}

#[test]
fn repark_uses_a_new_ordinal_without_growing_admitted_storage() {
    let mut s = Scheduler::new(Limits {
        live_tasks: 1,
        total_tasks: 1,
    })
    .unwrap();
    let storage = (s.tasks.as_ptr(), s.tasks.capacity(), s.queue.capacity());
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let mut old = None;
    for ordinal in 1..=256 {
        let wait = park(&mut s, &root);
        assert_eq!(wait.ordinal(), ordinal);
        if let Some(old) = old.as_ref() {
            assert_eq!(s.wake(old), Err(Error::StaleWait));
            assert_eq!(s.status(&root), Ok(Status::Parked));
            assert!(s.queue.is_empty());
        }
        s.wake(&wait).unwrap();
        assert_eq!(s.wake(&wait), Err(Error::StaleWait));
        resume(&mut s, &root);
        assert_eq!(s.usage().admitted, 1);
        assert_eq!(s.usage().live, 1);
        assert_eq!(s.tasks.as_ptr(), storage.0);
        assert_eq!(s.tasks.capacity(), storage.1);
        assert_eq!(s.queue.capacity(), storage.2);
        old = Some(wait);
    }
    // The ring's front may rotate, but its reserved backing storage never grows.
    assert_eq!(s.queue.capacity(), storage.2);
    s.finish(&root, Outcome::Success).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
    assert_eq!(s.usage().peak_live, 1);
}

#[test]
fn foreign_forged_and_wrong_phase_wait_operations_preserve_the_active_task() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let child = s.spawn(&root, None).unwrap();
    let root_wait = park(&mut s, &root);
    resume(&mut s, &child);
    let mut foreign = scheduler();
    let other = foreign.start(None).unwrap();
    resume(&mut foreign, &other);
    let other_wait = park(&mut foreign, &other);
    let before = (s.usage(), s.last_wait);
    assert_eq!(s.check_park(&root), Err(Error::NotActiveTask));
    assert_eq!(s.park(&root), Err(Error::NotActiveTask));
    assert_eq!(s.check_park(&other), Err(Error::ForeignTask));
    assert_eq!(s.park(&other), Err(Error::ForeignTask));
    assert_eq!(s.wake(&other_wait), Err(Error::ForeignTask));
    assert_eq!(
        s.wake(&WaitId {
            task: TaskId {
                invocation: Rc::clone(&s.invocation),
                index: usize::MAX,
            },
            ordinal: root_wait.ordinal(),
        }),
        Err(Error::InvalidTask)
    );
    for ordinal in [0, root_wait.ordinal() + 1] {
        assert_eq!(
            s.wake(&WaitId {
                task: root.clone(),
                ordinal,
            }),
            Err(Error::StaleWait)
        );
    }
    assert_eq!((s.usage(), s.last_wait), before);
    assert_eq!(s.status(&root), Ok(Status::Parked));
    assert_eq!(s.status(&child), Ok(Status::Running));
    assert!(s.queue.is_empty());
    s.wake(&root_wait).unwrap();
    s.finish(&child, Outcome::Success).unwrap();
    resume(&mut s, &root);
    s.finish(&root, Outcome::Success).unwrap();
    assert_eq!(
        s.dispatch(),
        Ok(Some(Dispatch::Cleanup {
            task: child.clone(),
            outcome: Outcome::Success,
        }))
    );
    assert_eq!(s.check_park(&child), Err(Error::WrongPhase));
    assert_eq!(s.park(&child), Err(Error::WrongPhase));
    assert_eq!(s.wake(&root_wait), Err(Error::StaleWait));
    s.finish_cleanup(&child, None).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
    foreign.request_cancel(&other).unwrap();
    cleanup(&mut foreign, &other, Outcome::Cancelled);
}

#[derive(Clone, Copy)]
enum CancelAt {
    Parked,
    Woken,
    Running,
}

#[test]
fn cancellation_revokes_parked_and_woken_tickets_before_postorder_cleanup() {
    for at in [CancelAt::Parked, CancelAt::Woken, CancelAt::Running] {
        let mut s = scheduler();
        let root = s.start(None).unwrap();
        resume(&mut s, &root);
        let child = s.spawn(&root, None).unwrap();
        let root_wait = park(&mut s, &root);
        resume(&mut s, &child);
        let leaf = s.spawn(&child, None).unwrap();
        let child_wait = park(&mut s, &child);
        resume(&mut s, &leaf);
        let leaf_wait = park(&mut s, &leaf);
        assert_eq!(s.dispatch(), Ok(None));
        assert!(!s.is_finished());
        assert_eq!(s.usage().live, 3);
        if matches!(at, CancelAt::Woken | CancelAt::Running) {
            s.wake(&root_wait).unwrap();
        }
        if matches!(at, CancelAt::Running) {
            resume(&mut s, &root);
        }
        s.request_cancel(&root).unwrap();
        if matches!(at, CancelAt::Running) {
            let ordinal = s.last_wait;
            assert_eq!(s.check_park(&root), Err(Error::CancellationPending));
            assert_eq!(s.park(&root), Ok(Park::Closing));
            assert_eq!(s.last_wait, ordinal);
        }
        for wait in [&root_wait, &child_wait, &leaf_wait] {
            assert_eq!(s.wake(wait), Err(Error::StaleWait));
        }
        assert_eq!(s.status(&root), Ok(Status::Joining));
        assert_eq!(s.status(&child), Ok(Status::Joining));
        // No frame is considered retired until the real adapter acknowledges it.
        let mut frames = [Some(vec![0]), Some(vec![1]), Some(vec![2])];
        let mut retired = Vec::new();
        while let Some(dispatch) = s.dispatch().unwrap() {
            let Dispatch::Cleanup { task, outcome } = dispatch else {
                panic!("cancelled parked tasks must never resume guest code");
            };
            assert_eq!(outcome, Outcome::Cancelled);
            let index = task.index();
            assert_eq!(s.usage().live, 3 - retired.len());
            drop(frames[index].take().unwrap());
            retired.push(index);
            for wait in [&root_wait, &child_wait, &leaf_wait] {
                assert_eq!(s.wake(wait), Err(Error::StaleWait));
            }
            s.finish_cleanup(&task, None).unwrap();
        }
        assert_eq!(retired, [2, 1, 0]);
        assert!(frames.iter().all(Option::is_none));
        assert!(s.is_finished());
        assert_eq!(s.usage().live, 0);
    }
}

#[test]
fn cancelling_a_parked_child_does_not_spuriously_wake_its_parent() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let child = s.spawn(&root, None).unwrap();
    let root_wait = park(&mut s, &root);
    resume(&mut s, &child);
    let child_wait = park(&mut s, &child);
    s.request_cancel(&child).unwrap();
    assert_eq!(s.wake(&child_wait), Err(Error::StaleWait));
    cleanup(&mut s, &child, Outcome::Cancelled);
    assert_eq!(s.status(&root), Ok(Status::Parked));
    assert_eq!(s.dispatch(), Ok(None));
    s.wake(&root_wait).unwrap();
    resume(&mut s, &root);
    s.finish(&root, Outcome::Success).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
}

#[test]
fn child_failure_revokes_parked_parent_and_sibling_before_cleanup() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let bad = s.spawn(&root, None).unwrap();
    let sibling = s.spawn(&root, None).unwrap();
    let root_wait = park(&mut s, &root);
    resume(&mut s, &bad);
    s.finish(&bad, Outcome::Failed(FailureId(91))).unwrap();
    resume(&mut s, &sibling);
    let sibling_wait = park(&mut s, &sibling);
    // Failure is propagated only after bad's actual cleanup acknowledgement.
    assert_eq!(s.status(&root), Ok(Status::Parked));
    cleanup(&mut s, &bad, Outcome::Failed(FailureId(91)));
    assert_eq!(s.wake(&root_wait), Err(Error::StaleWait));
    assert_eq!(s.wake(&sibling_wait), Err(Error::StaleWait));
    assert_eq!(s.status(&root), Ok(Status::Joining));
    cleanup(&mut s, &sibling, Outcome::Cancelled);
    cleanup(&mut s, &root, Outcome::Failed(FailureId(91)));
    assert_eq!(s.usage().live, 0);
}

#[test]
fn last_wait_identity_is_usable_once_and_exhaustion_is_atomic() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    s.last_wait = u64::MAX - 1;
    let wait = park(&mut s, &root);
    assert_eq!(wait.ordinal(), u64::MAX);
    s.wake(&wait).unwrap();
    resume(&mut s, &root);
    let before = (s.usage(), s.last_wait, s.queue.clone());
    assert_eq!(s.check_park(&root), Err(Error::WaitIdentityExhausted));
    assert_eq!(s.park(&root), Err(Error::WaitIdentityExhausted));
    assert_eq!((s.usage(), s.last_wait, s.queue.clone()), before);
    assert_eq!(s.status(&root), Ok(Status::Running));
    assert_eq!(s.active, Some(root.index()));
    assert_eq!(s.wake(&wait), Err(Error::StaleWait));
    s.request_cancel(&root).unwrap();
    // Cancellation is observed before trying to reserve another wait identity.
    assert_eq!(s.check_park(&root), Err(Error::CancellationPending));
    assert_eq!(s.park(&root), Ok(Park::Closing));
    assert_eq!(s.last_wait, u64::MAX);
    cleanup(&mut s, &root, Outcome::Cancelled);
}
