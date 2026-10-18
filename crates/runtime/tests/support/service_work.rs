use lang_mir::unit::{SourceId, Span};
use lang_runtime::{cleanup as c, execution as e};
use std::cell::RefCell;
use std::rc::Rc;

fn budget(limit: Option<u64>) -> e::Control<'static> {
    e::Control::with_limits(e::Limits::unbounded()).and_service_work(limit)
}

#[test]
fn service_work_known_batches_and_exact_boundaries_are_atomic() {
    let control = budget(Some(5));
    let mut usage = e::Usage {
        fuel: 7,
        host_calls: 2,
        peak_frames: 3,
        peak_live_cells: 11,
        ..Default::default()
    };
    control.charge_service_batch(&mut usage, &[1, 2]).unwrap();
    let before = usage;
    assert_eq!(
        control.charge_service_batch(&mut usage, &[1, 2]),
        Err(e::Failure::ServiceWorkLimit {
            limit: 5,
            used: 3,
            requested: 3,
        })
    );
    assert_eq!(
        usage, before,
        "a fitting prefix must not partially consume the batch"
    );
    control.charge_service_work(&mut usage, 2).unwrap();
    control.charge_service_batch(&mut usage, &[]).unwrap();
    let full = usage;
    assert_eq!(
        control.charge_service_work(&mut usage, 1),
        Err(e::Failure::ServiceWorkLimit {
            limit: 5,
            used: 5,
            requested: 1,
        })
    );
    assert_eq!(usage, full);
    assert_eq!(
        (
            usage.fuel,
            usage.host_calls,
            usage.peak_frames,
            usage.peak_live_cells
        ),
        (7, 2, 3, 11)
    );
    assert_eq!(usage.service_work, 5);
    assert_eq!(
        control.check_service_work_support(false),
        Err(e::UnsupportedServiceWork { limit: 5 })
    );
    assert_eq!(control.check_service_work_support(true), Ok(()));
    let zero = budget(Some(0));
    let mut usage = e::Usage::default();
    zero.charge_service_work(&mut usage, 0).unwrap();
    assert_eq!(
        zero.charge_service_work(&mut usage, 1),
        Err(e::Failure::ServiceWorkLimit {
            limit: 0,
            used: 0,
            requested: 1
        })
    );
    assert_eq!(usage, e::Usage::default());
}

#[test]
fn service_work_unbounded_and_batch_overflow_do_not_wrap_or_partially_charge() {
    let control = budget(None);
    assert_eq!(control.service_work_limit(), None);
    assert_eq!(control.check_service_work_support(false), Ok(()));
    let mut usage = e::Usage::default();
    assert_eq!(
        control.charge_service_batch(&mut usage, &[1, u64::MAX]),
        Err(e::Failure::CounterOverflow)
    );
    assert_eq!(usage, e::Usage::default());
    control.charge_service_work(&mut usage, u64::MAX).unwrap();
    let before = usage;
    assert_eq!(
        control.charge_service_work(&mut usage, 1),
        Err(e::Failure::CounterOverflow)
    );
    assert_eq!(usage, before);
    assert_eq!(usage.service_work, u64::MAX);
}

#[test]
fn service_work_adds_no_polling_or_guest_fuel_position() {
    let flag = e::InterruptionFlag::new();
    let control = budget(Some(2)).and_interruption(&flag);
    let mut usage = e::Usage::default();
    control.charge(&mut usage).unwrap();
    flag.interrupt();
    control.charge_service_work(&mut usage, 1).unwrap();
    assert_eq!(usage.fuel, 1);
    assert_eq!(usage.service_work, 1);
    let before = usage;
    assert_eq!(control.charge(&mut usage), Err(e::Failure::Interrupted));
    assert_eq!(usage, before);
}

#[test]
fn service_work_numeric_failure_survives_cleanup_and_secondary_failures() {
    for refuse_cleanup in [false, true] {
        let control = budget(Some(3));
        let mut usage = e::Usage::default();
        control.charge_service_work(&mut usage, 3).unwrap();
        let error = control.charge_service_work(&mut usage, 1).unwrap_err();
        let failure: c::ExecutionFailure = error.into();
        let expected = c::ExecutionFailure::ServiceWorkLimit {
            limit: 3,
            used: 3,
            requested: 1,
        };
        assert_eq!(failure, expected);
        let trace = Rc::new(RefCell::new(Vec::new()));
        let drops = trace.clone();
        let span = Span {
            source: SourceId(0),
            start: 3,
            end: 7,
        };
        let mut instance = c::Instance::default();
        let mut frame = instance
            .enter(
                c::Limits {
                    slots: 2,
                    scopes: 1,
                    failure_records: 4,
                },
                move |id: &mut u8, _: c::Reason| {
                    drops.borrow_mut().push(*id);
                    if refuse_cleanup && *id == 1 {
                        Err(0x51)
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap();
        for id in 0..2u8 {
            frame.declare(c::Slot(id as usize), c::Kind::Owner).unwrap();
            frame
                .initialize(c::Slot(id as usize), id, Some(span))
                .unwrap();
        }
        let done = frame.finish(c::Exit::Failure {
            failure,
            span: Some(span),
        });
        assert_eq!(
            done.report.primary().unwrap().kind,
            c::FailureKind::Execution(expected)
        );
        assert_eq!(done.report.primary().unwrap().span, Some(span));
        assert_eq!(&*trace.borrow(), &[1, 0]);
        assert_eq!(done.report.secondary().len(), usize::from(refuse_cleanup));
        assert_eq!(done.poisoned, refuse_cleanup);
        assert_eq!(
            usage.service_work, 3,
            "mandatory retirement does not consult the exhausted meter"
        );
    }
}
