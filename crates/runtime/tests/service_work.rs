use lang_runtime::execution::{Failure, ServiceWork};

#[test]
fn nested_unmetered_scopes_restore_after_cleanup_panics_and_preserve_usage() {
    let meter = ServiceWork::new(Some(1));
    meter.charge(1).unwrap();
    meter.unmetered(|| {
        meter.charge_batch(&[u64::MAX, 1]).unwrap();
        meter.clone().unmetered(|| meter.charge(9).unwrap());
        meter.charge(2).unwrap();
    });
    assert_eq!(meter.used(), 1);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            meter.unmetered(|| panic!("cleanup callback"));
        }))
        .is_err()
    );
    assert_eq!(
        meter.charge(1),
        Err(Failure::ServiceWorkLimit {
            limit: 1,
            used: 1,
            requested: 1
        })
    );
    assert_eq!(meter.used(), 1);
}

#[test]
fn nested_runtime_errors_keep_the_structured_refusal_identity() {
    use lang_runtime::{inspection, layout, shared::managed};
    let failure = Failure::ServiceWorkLimit {
        limit: 7,
        used: 5,
        requested: 3,
    };
    let error = managed::Error {
        kind: managed::Kind::Inspection(inspection::Error::Layout(layout::Error::ServiceWork(
            failure,
        ))),
        primary: None,
        secondary: None,
    };
    assert_eq!(
        lang_runtime::execution::service_failure(&error),
        Some(failure)
    );
}

#[test]
fn local_and_invocation_quotas_commit_together_without_charging_metadata() {
    use lang_runtime::inspection::{Error, Work};
    let meter = ServiceWork::new(Some(1));
    let mut work = Work::new(3).with_service_work(meter.clone());
    assert_eq!(work.charge_event(4, 1), Err(Error::Work));
    assert_eq!(meter.used(), 0);
    assert_eq!(work.remaining(), 3);
    assert_eq!(
        work.charge_event(2, 2),
        Err(Error::ServiceWork(Failure::ServiceWorkLimit {
            limit: 1,
            used: 0,
            requested: 2
        }))
    );
    assert_eq!(meter.used(), 0);
    assert_eq!(work.remaining(), 3);
    work.charge_event(1, 1).unwrap();
    work.charge(1).unwrap();
    assert_eq!(meter.used(), 1);
    assert_eq!(work.remaining(), 1);
    assert_eq!(
        work.charge_event(1, 1),
        Err(Error::ServiceWork(Failure::ServiceWorkLimit {
            limit: 1,
            used: 1,
            requested: 1
        }))
    );
    assert_eq!(work.remaining(), 1);
}

#[test]
fn service_views_share_admission_and_independent_invocations_do_not() {
    let meter = ServiceWork::new(Some(9));
    let nested = meter.clone();
    meter.charge(2).unwrap();
    nested.charge_batch(&[3, 4]).unwrap();
    assert_eq!(meter.used(), 9);
    assert_eq!(
        nested.charge(1),
        Err(Failure::ServiceWorkLimit {
            limit: 9,
            used: 9,
            requested: 1
        })
    );
    assert_eq!(meter.used(), 9);
    let independent = ServiceWork::new(Some(9));
    assert_eq!(independent.used(), 0);
    independent.charge(1).unwrap();
    assert_eq!(meter.used(), 9);
}

#[test]
fn shared_counter_overflow_and_batch_refusal_preserve_the_admitted_prefix() {
    let meter = ServiceWork::new(None);
    meter.charge(u64::MAX - 1).unwrap();
    assert_eq!(
        meter.clone().charge_batch(&[1, 1]),
        Err(Failure::CounterOverflow)
    );
    assert_eq!(meter.used(), u64::MAX - 1);
    meter.charge(1).unwrap();
    assert_eq!(meter.used(), u64::MAX);
    assert_eq!(
        meter.charge_batch(&[u64::MAX, 1]),
        Err(Failure::CounterOverflow)
    );
    assert_eq!(meter.used(), u64::MAX);
}
