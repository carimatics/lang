use lang_mir::unit::{Budget, Span};
use lang_runtime::cleanup as c;
use std::{cell::Cell, num::NonZeroUsize, rc::Rc};

fn session(records: usize) -> c::Session {
    c::Session::prepare(
        NonZeroUsize::new(records).unwrap(),
        &mut Budget::new(Default::default()),
    )
    .unwrap()
}

#[test]
fn execution_primary_survives_truncation_and_remaining_callbacks() {
    let mut session = session(1);
    session.execution(c::ExecutionFailure::Panic, None);
    let mut attempts = Vec::new();
    for slot in 0..4 {
        let mut cleaner = |_: &mut (), reason| {
            assert_eq!(reason, c::Reason::Failure);
            attempts.push(slot);
            Err(slot as u32)
        };
        assert_eq!(
            session.retire(c::Slot(slot), (), None, c::Reason::Failure, &mut cleaner),
            Err(c::StateError::Failed)
        );
    }
    assert_eq!(attempts, [0, 1, 2, 3]);
    assert!(session.is_poisoned());
    let report = session.into_report();
    assert_eq!(
        report.primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Panic)
    );
    assert!(report.secondary().is_empty());
    assert_eq!(report.cleanup_failures(), 4);
    assert_eq!(report.truncated(), 4);
}

#[test]
fn callback_panic_is_contained_once_and_independent_retirement_continues() {
    let mut session = session(3);
    let attempts = Cell::new(0);
    let mut panic = |_: &mut (), _| -> Result<(), u32> {
        attempts.set(attempts.get() + 1);
        panic!("trusted cleanup panic")
    };
    assert_eq!(
        session.retire(c::Slot(2), (), None, c::Reason::ScopeExit, &mut panic),
        Err(c::StateError::Failed)
    );
    let mut success = |_: &mut (), _| {
        attempts.set(attempts.get() + 1);
        Ok(())
    };
    session
        .retire(c::Slot(1), (), None, c::Reason::ScopeExit, &mut success)
        .unwrap();
    assert_eq!(attempts.get(), 2);
    assert!(session.is_poisoned());
    let report = session.into_report();
    assert_eq!(report.primary().unwrap().kind, c::FailureKind::CleanupPanic);
    assert_eq!(report.quarantined(), 1);
}

#[test]
fn ordinary_cleanup_error_disposes_consumed_payload_once() {
    struct Ticket(Rc<Cell<usize>>);
    impl Drop for Ticket {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let mut session = session(2);
    let mut error = |_: &mut Ticket, _| Err(42);
    assert!(
        session
            .retire(
                c::Slot(0),
                Ticket(drops.clone()),
                None,
                c::Reason::Explicit,
                &mut error
            )
            .is_err()
    );
    assert_eq!(drops.get(), 1);
    assert_eq!(session.report().cleanup_failures(), 1);
}

#[test]
fn ordered_task_reports_merge_without_replacing_the_chosen_primary() {
    let span = Span {
        source: lang_mir::unit::SourceId(0),
        start: 10,
        end: 12,
    };
    let mut primary = session(3);
    primary.execution(c::ExecutionFailure::Panic, Some(span));
    let mut completed = session(2);
    for code in 10..13 {
        let mut error = |_: &mut (), _| Err(code);
        assert!(
            completed
                .retire(
                    c::Slot(code as usize),
                    (),
                    Some(span),
                    c::Reason::Failure,
                    &mut error
                )
                .is_err()
        );
    }
    primary.append(completed);
    let report = primary.into_report();
    assert_eq!(
        report.primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Panic)
    );
    assert_eq!(
        report
            .secondary()
            .iter()
            .map(|r| r.kind)
            .collect::<Vec<_>>(),
        [c::FailureKind::Cleanup(10), c::FailureKind::Cleanup(11)]
    );
    assert_eq!(report.cleanup_failures(), 3);
    assert_eq!(report.truncated(), 1);
    assert!(report.secondary().iter().all(|r| r.span == Some(span)));
}
