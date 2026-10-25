use super::*;

mod wait;

fn scheduler() -> Scheduler {
    Scheduler::new(Limits {
        live_tasks: 16,
        total_tasks: 32,
    })
    .unwrap()
}

fn resume(scheduler: &mut Scheduler, task: &TaskId) {
    assert_eq!(
        scheduler.dispatch(),
        Ok(Some(Dispatch::Resume(task.clone())))
    );
}

fn cleanup(scheduler: &mut Scheduler, task: &TaskId, outcome: Outcome) {
    assert_eq!(
        scheduler.dispatch(),
        Ok(Some(Dispatch::Cleanup {
            task: task.clone(),
            outcome,
        }))
    );
    assert_eq!(scheduler.status(task), Ok(Status::Cleaning(outcome)));
    assert_eq!(scheduler.finish_cleanup(task, None), Ok(outcome));
}

#[test]
fn current_task_borrow_keeps_non_root_identity_cancellation_and_parent_lifecycle() {
    let mut s = Scheduler::new(Limits {
        live_tasks: 2,
        total_tasks: 2,
    })
    .unwrap();
    let root = s.start(None).unwrap();
    assert!(matches!(s.current(&root), Err(Error::NoActiveTask)));
    resume(&mut s, &root);
    let origin = Span {
        source: lang_mir::unit::SourceId(0),
        start: 3,
        end: 7,
    };
    let child = s.spawn(&root, Some(origin)).unwrap();
    s.join(&root, &child).unwrap();
    resume(&mut s, &child);
    assert_eq!(child.index(), 1);
    assert!(matches!(s.current(&root), Err(Error::NotActiveTask)));
    let mut foreign = scheduler();
    let other = foreign.start(None).unwrap();
    assert!(matches!(s.current(&other), Err(Error::ForeignTask)));
    let invalid = TaskId {
        invocation: child.invocation.clone(),
        index: usize::MAX,
    };
    assert!(matches!(s.current(&invalid), Err(Error::InvalidTask)));
    let before = s.usage();
    {
        let mut current = s.current(&child).unwrap();
        assert_eq!(current.id(), &child);
        assert_eq!(current.parent(), Ok(Some(root.clone())));
        assert_eq!(current.origin(), Ok(Some(origin)));
        assert_eq!(current.status(), Ok(Status::Running));
        assert_eq!(current.usage(), before);
        assert_eq!(current.limits().total_tasks, 2);
        assert_eq!(current.cancellation_requested(), Ok(false));
        {
            let mut nested = current.reborrow().unwrap();
            assert_eq!(nested.id(), &child);
            nested.request_cancel().unwrap();
            assert_eq!(nested.cancellation_requested(), Ok(true));
            assert_eq!(nested.status(), Ok(Status::Running));
            assert_eq!(nested.usage(), before);
        }
        assert_eq!(current.cancellation_requested(), Ok(true));
        assert_eq!(current.status(), Ok(Status::Running));
    }
    assert_eq!(s.usage(), before);
    assert_eq!(s.status(&child), Ok(Status::Running));
    assert_eq!(
        s.status(&root),
        Ok(Status::Waiting {
            child: child.clone()
        })
    );
    assert!(matches!(s.dispatch(), Err(Error::Busy)));
    assert_eq!(s.cancellation_point(&child), Ok(Boundary::Closing));
    assert_eq!(
        s.dispatch(),
        Ok(Some(Dispatch::Cleanup {
            task: child.clone(),
            outcome: Outcome::Cancelled
        }))
    );
    assert!(matches!(s.current(&child), Err(Error::WrongPhase)));
    assert_eq!(s.finish_cleanup(&child, None), Ok(Outcome::Cancelled));
    assert!(matches!(s.current(&child), Err(Error::NoActiveTask)));
    resume(&mut s, &root);
    assert_eq!(s.cancellation_requested(&root), Ok(false));
    s.finish(&root, Outcome::Success).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
    assert!(s.is_finished());
    assert_eq!(
        s.usage(),
        Usage {
            admitted: 2,
            live: 0,
            peak_live: 2
        }
    );
}

#[test]
fn current_task_error_and_unwind_release_only_the_borrow() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let before = s.usage();
    let result = (|| -> Result<(), Error> {
        let mut current = s.current(&root)?;
        let nested = current.reborrow()?;
        assert_eq!(nested.id(), &root);
        Err(Error::Capacity)
    })();
    assert_eq!(result, Err(Error::Capacity));
    assert_eq!(s.status(&root), Ok(Status::Running));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut current = s.current(&root).unwrap();
        let nested = current.reborrow().unwrap();
        assert_eq!(nested.id(), &root);
        panic!("contained current-task borrower");
    }));
    assert!(panic.is_err());
    assert_eq!(s.status(&root), Ok(Status::Running));
    assert_eq!(s.cancellation_requested(&root), Ok(false));
    assert_eq!(s.usage(), before);
    s.finish(&root, Outcome::Success).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
}

#[test]
fn call_scopes_finish_only_the_scope_on_the_original_non_root_task() {
    let mut s = Scheduler::new(Limits {
        live_tasks: 2,
        total_tasks: 2,
    })
    .unwrap();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let child = s.spawn(&root, None).unwrap();
    s.join(&root, &child).unwrap();
    resume(&mut s, &child);
    let before = s.usage();
    for outcome in [Outcome::Success, Outcome::Failed(FailureId(7))] {
        let mut scope = driver::Driver::scope(s.current(&child).unwrap()).unwrap();
        assert_eq!(scope.start(None), Ok(child.clone()));
        assert_eq!(scope.record_index(&child), Ok(0));
        assert_eq!(child.index(), 1);
        assert_eq!(scope.record_index(&root), Err(Error::ForeignTask));
        assert_eq!(scope.dispatch(), Ok(Some(Dispatch::Resume(child.clone()))));
        assert_eq!(scope.spawn(&child, None), Err(Error::WrongPhase));
        assert_eq!(scope.yield_now(&child), Err(Error::WrongPhase));
        {
            let current = scope.current(&child).unwrap();
            let mut nested = driver::Driver::scope(current).unwrap();
            assert_eq!(nested.start(None), Ok(child.clone()));
            assert_eq!(nested.dispatch(), Ok(Some(Dispatch::Resume(child.clone()))));
            nested.finish(&child, Outcome::Success).unwrap();
            assert!(matches!(
                nested.dispatch(),
                Ok(Some(Dispatch::Cleanup { .. }))
            ));
            assert_eq!(nested.finish_cleanup(&child, None), Ok(Outcome::Success));
            assert_eq!(nested.usage(), before);
        }
        assert_eq!(scope.current(&child).unwrap().status(), Ok(Status::Running));
        scope.finish(&child, outcome).unwrap();
        assert_eq!(
            scope.dispatch(),
            Ok(Some(Dispatch::Cleanup {
                task: child.clone(),
                outcome
            }))
        );
        assert_eq!(scope.finish_cleanup(&child, None), Ok(outcome));
        assert!(scope.is_finished());
        drop(scope);
        assert_eq!(s.status(&child), Ok(Status::Running));
        assert_eq!(s.usage(), before);
        assert_eq!(
            s.status(&root),
            Ok(Status::Waiting {
                child: child.clone()
            })
        );
    }
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut scope = driver::Driver::scope(s.current(&child).unwrap()).unwrap();
        scope.start(None).unwrap();
        scope.dispatch().unwrap();
        panic!("contained call scope");
    }));
    assert!(panic.is_err());
    assert_eq!(s.status(&child), Ok(Status::Running));
    {
        let mut scope = driver::Driver::scope(s.current(&child).unwrap()).unwrap();
        scope.start(None).unwrap();
        scope.dispatch().unwrap();
        scope.current(&child).unwrap().request_cancel().unwrap();
        assert_eq!(scope.cancellation_point(&child), Ok(Boundary::Closing));
        assert_eq!(
            scope.dispatch(),
            Ok(Some(Dispatch::Cleanup {
                task: child.clone(),
                outcome: Outcome::Cancelled
            }))
        );
        assert_eq!(
            scope.finish_cleanup(&child, Some(FailureId(8))),
            Ok(Outcome::Cancelled)
        );
    }
    assert_eq!(s.status(&child), Ok(Status::Running));
    assert_eq!(s.usage(), before);
    assert_eq!(s.cancellation_point(&child), Ok(Boundary::Closing));
    cleanup(&mut s, &child, Outcome::Cancelled);
    resume(&mut s, &root);
    s.finish(&root, Outcome::Success).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
    assert_eq!(
        s.usage(),
        Usage {
            admitted: 2,
            live: 0,
            peak_live: 2
        }
    );
}

#[test]
fn fifo_children_join_only_after_actual_cleanup_acknowledgement() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let left = s.spawn(&root, None).unwrap();
    let right = s.spawn(&root, None).unwrap();
    assert_eq!(s.parent(&left), Ok(Some(root.clone())));
    s.join(&root, &left).unwrap();
    resume(&mut s, &left);
    s.yield_now(&left).unwrap();
    resume(&mut s, &right);
    s.finish(&right, Outcome::Success).unwrap();
    resume(&mut s, &left);
    s.finish(&left, Outcome::Success).unwrap();
    assert_eq!(
        s.status(&root),
        Ok(Status::Waiting {
            child: left.clone()
        })
    );
    cleanup(&mut s, &right, Outcome::Success);
    assert_eq!(s.usage().live, 2);
    cleanup(&mut s, &left, Outcome::Success);
    resume(&mut s, &root);
    s.finish(&root, Outcome::Success).unwrap();
    assert!(!s.is_finished());
    cleanup(&mut s, &root, Outcome::Success);
    assert!(s.is_finished());
    assert_eq!(s.dispatch(), Ok(None));
    assert_eq!(
        s.usage(),
        Usage {
            admitted: 3,
            live: 0,
            peak_live: 3
        }
    );
}

#[test]
fn normal_scope_exit_joins_instead_of_cancelling_children() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let child = s.spawn(&root, None).unwrap();
    s.finish(&root, Outcome::Success).unwrap();
    assert_eq!(s.status(&root), Ok(Status::Joining));
    assert_eq!(s.cancellation_requested(&child), Ok(false));
    resume(&mut s, &child);
    s.yield_now(&child).unwrap();
    resume(&mut s, &child);
    s.finish(&child, Outcome::Success).unwrap();
    cleanup(&mut s, &child, Outcome::Success);
    cleanup(&mut s, &root, Outcome::Success);
}

#[test]
fn nested_cancel_retires_descendants_before_parent_exactly_once() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let child = s.spawn(&root, None).unwrap();
    s.join(&root, &child).unwrap();
    resume(&mut s, &child);
    let leaf = s.spawn(&child, None).unwrap();
    s.yield_now(&child).unwrap();
    s.request_cancel(&root).unwrap();
    s.request_cancel(&root).unwrap();
    for task in [&root, &child, &leaf] {
        assert_eq!(s.cancellation_requested(task), Ok(true));
    }
    cleanup(&mut s, &leaf, Outcome::Cancelled);
    cleanup(&mut s, &child, Outcome::Cancelled);
    cleanup(&mut s, &root, Outcome::Cancelled);
    s.request_cancel(&root).unwrap();
    assert_eq!(s.dispatch(), Ok(None));
    assert_eq!(s.finish_cleanup(&root, None), Err(Error::NoActiveTask));
    assert_eq!(s.usage().live, 0);
}

#[test]
fn running_task_observes_cancel_at_boundary_without_preemption() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    assert_eq!(s.cancellation_point(&root), Ok(Boundary::Continue));
    s.request_cancel(&root).unwrap();
    assert_eq!(s.status(&root), Ok(Status::Running));
    assert_eq!(s.dispatch(), Err(Error::Busy));
    assert_eq!(s.spawn(&root, None), Err(Error::CancellationPending));
    assert_eq!(s.usage().admitted, 1);
    assert_eq!(s.cancellation_point(&root), Ok(Boundary::Closing));
    cleanup(&mut s, &root, Outcome::Cancelled);
}

#[test]
fn failure_in_running_instruction_remains_primary_over_pending_cancel() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    s.request_cancel(&root).unwrap();
    s.finish(&root, Outcome::Failed(FailureId(7))).unwrap();
    cleanup(&mut s, &root, Outcome::Failed(FailureId(7)));
}

#[test]
fn cancelled_success_and_joining_parent_observe_cancellation() {
    for finish_before_cancel in [true, false] {
        let mut s = scheduler();
        let root = s.start(None).unwrap();
        resume(&mut s, &root);
        let child = s.spawn(&root, None).unwrap();
        if finish_before_cancel {
            s.finish(&root, Outcome::Success).unwrap();
            s.request_cancel(&root).unwrap();
        } else {
            s.request_cancel(&root).unwrap();
            s.finish(&root, Outcome::Success).unwrap();
        }
        cleanup(&mut s, &child, Outcome::Cancelled);
        cleanup(&mut s, &root, Outcome::Cancelled);
    }
}

#[test]
fn cleanup_is_nonpreemptible_and_retains_primary_and_secondary_failures() {
    for (initial, cancelled, expected) in [
        (Outcome::Success, false, Outcome::Failed(FailureId(9))),
        (Outcome::Success, true, Outcome::Cancelled),
        (
            Outcome::Failed(FailureId(7)),
            false,
            Outcome::Failed(FailureId(7)),
        ),
        (Outcome::Cancelled, false, Outcome::Cancelled),
    ] {
        let mut s = scheduler();
        let root = s.start(None).unwrap();
        resume(&mut s, &root);
        s.finish(&root, initial).unwrap();
        assert!(matches!(
            s.dispatch().unwrap(),
            Some(Dispatch::Cleanup { .. })
        ));
        if cancelled {
            s.request_cancel(&root).unwrap();
        }
        assert_eq!(s.status(&root), Ok(Status::Cleaning(initial)));
        assert_eq!(s.dispatch(), Err(Error::Busy));
        assert_eq!(s.yield_now(&root), Err(Error::WrongPhase));
        assert_eq!(s.finish_cleanup(&root, Some(FailureId(9))), Ok(expected));
        assert_eq!(s.status(&root), Ok(Status::Complete(expected)));
    }
}

#[test]
fn pending_cancellation_is_visible_to_the_cleanup_adapter_at_entry() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    s.finish(&root, Outcome::Success).unwrap();
    s.request_cancel(&root).unwrap();
    cleanup(&mut s, &root, Outcome::Cancelled);
}

#[test]
fn terminal_child_failure_cancels_siblings_and_propagates_after_cleanup() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let left = s.spawn(&root, None).unwrap();
    let right = s.spawn(&root, None).unwrap();
    s.join(&root, &left).unwrap();
    resume(&mut s, &left);
    s.finish(&left, Outcome::Failed(FailureId(42))).unwrap();
    resume(&mut s, &right);
    s.yield_now(&right).unwrap();
    cleanup(&mut s, &left, Outcome::Failed(FailureId(42)));
    assert_eq!(s.status(&root), Ok(Status::Joining));
    cleanup(&mut s, &right, Outcome::Cancelled);
    cleanup(&mut s, &root, Outcome::Failed(FailureId(42)));
}

#[test]
fn failure_selection_uses_spawn_order_and_explicit_parent_failure_wins() {
    for parent_failure in [None, Some(FailureId(99))] {
        let mut s = scheduler();
        let root = s.start(None).unwrap();
        resume(&mut s, &root);
        let left = s.spawn(&root, None).unwrap();
        let right = s.spawn(&root, None).unwrap();
        s.finish(
            &root,
            parent_failure.map_or(Outcome::Success, Outcome::Failed),
        )
        .unwrap();
        if parent_failure.is_none() {
            resume(&mut s, &left);
            s.yield_now(&left).unwrap();
            resume(&mut s, &right);
            s.finish(&right, Outcome::Failed(FailureId(2))).unwrap();
            resume(&mut s, &left);
            s.finish(&left, Outcome::Failed(FailureId(1))).unwrap();
            // The later-spawned task cleans first, but does not win selection.
            cleanup(&mut s, &right, Outcome::Failed(FailureId(2)));
            cleanup(&mut s, &left, Outcome::Failed(FailureId(1)));
        } else {
            cleanup(&mut s, &right, Outcome::Cancelled);
            cleanup(&mut s, &left, Outcome::Cancelled);
        }
        cleanup(
            &mut s,
            &root,
            Outcome::Failed(parent_failure.unwrap_or(FailureId(1))),
        );
    }
}

#[test]
fn completed_child_join_yields_and_does_not_reclean_the_child() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let child = s.spawn(&root, None).unwrap();
    s.yield_now(&root).unwrap();
    resume(&mut s, &child);
    s.finish(&child, Outcome::Success).unwrap();
    resume(&mut s, &root);
    s.yield_now(&root).unwrap();
    cleanup(&mut s, &child, Outcome::Success);
    resume(&mut s, &root);
    s.join(&root, &child).unwrap();
    assert_eq!(s.status(&root), Ok(Status::Ready));
    assert_eq!(s.finish(&root, Outcome::Success), Err(Error::NoActiveTask));
    resume(&mut s, &root);
    s.finish(&root, Outcome::Success).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
}

#[test]
fn foreign_and_illegal_operations_leave_active_task_unchanged() {
    let mut s = scheduler();
    let root = s.start(None).unwrap();
    let mut other = scheduler();
    let foreign = other.start(None).unwrap();
    assert_ne!(root, foreign);
    resume(&mut s, &root);
    let child = s.spawn(&root, None).unwrap();
    let before = s.usage();
    assert_eq!(s.status(&foreign), Err(Error::ForeignTask));
    assert_eq!(s.request_cancel(&foreign), Err(Error::ForeignTask));
    assert_eq!(s.join(&root, &foreign), Err(Error::ForeignTask));
    assert_eq!(s.join(&root, &root), Err(Error::NotChild));
    assert_eq!(s.spawn(&child, None), Err(Error::NotActiveTask));
    assert_eq!(s.finish_cleanup(&root, None), Err(Error::WrongPhase));
    assert_eq!(s.start(None), Err(Error::RootExists));
    assert_eq!(s.dispatch(), Err(Error::Busy));
    assert_eq!(s.usage(), before);
    assert_eq!(s.status(&root), Ok(Status::Running));
    s.join(&root, &child).unwrap();
    resume(&mut s, &child);
    assert_eq!(s.join(&child, &root), Err(Error::NotChild));
    s.finish(&child, Outcome::Success).unwrap();
    cleanup(&mut s, &child, Outcome::Success);
    resume(&mut s, &root);
    s.finish(&root, Outcome::Success).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
}

#[test]
fn live_limit_is_released_only_after_cleanup_and_total_slots_are_not_reused() {
    let mut s = Scheduler::new(Limits {
        live_tasks: 2,
        total_tasks: 3,
    })
    .unwrap();
    let root = s.start(None).unwrap();
    resume(&mut s, &root);
    let child = s.spawn(&root, None).unwrap();
    assert_eq!(s.spawn(&root, None), Err(Error::LiveTaskLimit { limit: 2 }));
    s.yield_now(&root).unwrap();
    resume(&mut s, &child);
    s.finish(&child, Outcome::Success).unwrap();
    resume(&mut s, &root);
    assert_eq!(s.spawn(&root, None), Err(Error::LiveTaskLimit { limit: 2 }));
    s.yield_now(&root).unwrap();
    cleanup(&mut s, &child, Outcome::Success);
    resume(&mut s, &root);
    let next = s.spawn(&root, None).unwrap();
    assert_eq!(next.index(), 2);
    s.join(&root, &next).unwrap();
    resume(&mut s, &next);
    s.finish(&next, Outcome::Success).unwrap();
    cleanup(&mut s, &next, Outcome::Success);
    resume(&mut s, &root);
    assert_eq!(
        s.spawn(&root, None),
        Err(Error::TotalTaskLimit { limit: 3 })
    );
    assert_eq!(s.usage().admitted, 3);
    assert_eq!(s.status(&child), Ok(Status::Complete(Outcome::Success)));
    s.finish(&root, Outcome::Success).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
}

#[test]
fn empty_and_unrepresentable_limits_refuse_before_root_publication() {
    let mut s = Scheduler::new(Limits {
        live_tasks: 0,
        total_tasks: 0,
    })
    .unwrap();
    assert_eq!(s.start(None), Err(Error::LiveTaskLimit { limit: 0 }));
    assert_eq!(s.usage(), Usage::default());
    let mut s = Scheduler::new(Limits {
        live_tasks: 1,
        total_tasks: 0,
    })
    .unwrap();
    assert_eq!(s.start(None), Err(Error::TotalTaskLimit { limit: 0 }));
    assert!(matches!(
        Scheduler::new(Limits {
            live_tasks: 1,
            total_tasks: usize::MAX
        }),
        Err(Error::Capacity)
    ));
}

#[test]
fn source_origins_survive_completion() {
    let mut s = scheduler();
    let span = Span {
        source: lang_mir::unit::SourceId(0),
        start: 4,
        end: 9,
    };
    let root = s.start(Some(span)).unwrap();
    resume(&mut s, &root);
    s.finish(&root, Outcome::Success).unwrap();
    cleanup(&mut s, &root, Outcome::Success);
    assert_eq!(s.origin(&root), Ok(Some(span)));
    assert_eq!(s.parent(&root), Ok(None));
}

#[test]
fn deep_cancellation_never_recurses_on_the_host_stack() {
    const DEPTH: usize = 512;
    let mut s = Scheduler::new(Limits {
        live_tasks: DEPTH,
        total_tasks: DEPTH,
    })
    .unwrap();
    let root = s.start(None).unwrap();
    let mut parent = root.clone();
    for _ in 1..DEPTH {
        resume(&mut s, &parent);
        let child = s.spawn(&parent, None).unwrap();
        s.join(&parent, &child).unwrap();
        parent = child;
    }
    s.request_cancel(&root).unwrap();
    for expected_index in (0..DEPTH).rev() {
        let Some(Dispatch::Cleanup { task, outcome }) = s.dispatch().unwrap() else {
            panic!("every admitted task requires cleanup");
        };
        assert_eq!(task.index(), expected_index);
        assert_eq!(outcome, Outcome::Cancelled);
        s.finish_cleanup(&task, None).unwrap();
    }
    assert!(s.is_finished());
    assert_eq!(s.usage().live, 0);
}

#[test]
fn cancellation_at_every_dispatch_preserves_tree_retirement_and_capacity() {
    // Explore boundary positions in an executable branching tree. The oracle is
    // ancestry/retirement, independent of the kernel's internal state machine.
    for cancel_at in 0..40 {
        let mut s = Scheduler::new(Limits {
            live_tasks: 15,
            total_tasks: 15,
        })
        .unwrap();
        let capacities = (s.tasks.capacity(), s.queue.capacity());
        let root = s.start(None).unwrap();
        let mut depth = [0; 15];
        let mut resumed = [false; 15];
        let mut retired = [false; 15];
        let mut boundary = 0;
        while let Some(dispatch) = s.dispatch().unwrap() {
            if boundary == cancel_at {
                s.request_cancel(&root).unwrap();
            }
            boundary += 1;
            match dispatch {
                Dispatch::Resume(task) => {
                    if s.cancellation_point(&task).unwrap() == Boundary::Closing {
                        continue;
                    }
                    let index = task.index();
                    if !resumed[index] && depth[index] < 3 {
                        for _ in 0..2 {
                            let child = s.spawn(&task, None).unwrap();
                            depth[child.index()] = depth[index] + 1;
                        }
                        resumed[index] = true;
                        s.yield_now(&task).unwrap();
                    } else {
                        s.finish(&task, Outcome::Success).unwrap();
                    }
                }
                Dispatch::Cleanup { task, .. } => {
                    let index = task.index();
                    assert!(!retired[index], "cleanup exactly once");
                    for (child_index, child) in s.tasks.iter().enumerate() {
                        if child.parent == Some(index) {
                            assert!(retired[child_index], "children retire before parent");
                        }
                    }
                    retired[index] = true;
                    s.finish_cleanup(&task, None).unwrap();
                }
            }
            assert_eq!((s.tasks.capacity(), s.queue.capacity()), capacities);
            assert!(s.queue.len() <= s.usage().live);
            assert_eq!(
                s.queue
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len(),
                s.queue.len()
            );
        }
        assert!(s.is_finished());
        assert_eq!(s.usage().live, 0);
        assert_eq!(
            retired.iter().filter(|&&done| done).count(),
            s.usage().admitted
        );
    }
}

#[test]
fn child_cleanup_failure_overrides_cancellation_but_not_explicit_parent_failure() {
    for explicit_failure in [false, true] {
        let mut s = scheduler();
        let root = s.start(None).unwrap();
        resume(&mut s, &root);
        let child = s.spawn(&root, None).unwrap();
        s.yield_now(&root).unwrap();
        resume(&mut s, &child);
        s.finish(&child, Outcome::Success).unwrap();
        resume(&mut s, &root);
        // A failed parent's child is already cancelled when cleanup begins.
        // Otherwise the cleanup failure propagates before root cancellation.
        s.finish(
            &root,
            if explicit_failure {
                Outcome::Failed(FailureId(1))
            } else {
                Outcome::Success
            },
        )
        .unwrap();
        assert!(
            matches!(s.dispatch().unwrap(), Some(Dispatch::Cleanup { task, .. }) if task == child)
        );
        assert_eq!(
            s.finish_cleanup(&child, Some(FailureId(2))),
            Ok(if explicit_failure {
                Outcome::Cancelled
            } else {
                Outcome::Failed(FailureId(2))
            })
        );
        s.request_cancel(&root).unwrap();
        cleanup(
            &mut s,
            &root,
            Outcome::Failed(FailureId(if explicit_failure { 1 } else { 2 })),
        );
    }
}
