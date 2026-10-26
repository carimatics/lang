// Shared expectations, actual installed component entry points and original
// resource/provider state exercise the complete finite-bound matrix.
fn quota_call(
    h: &mut Harness,
    provider: native::Provider<'static>,
    name: &str,
    input: Option<storage::Claim>,
    control: e::Control<'_>,
) -> Result<r::Outcome<execution::Value>, r::Failure> {
    let mut outcome = call_object_control(h, provider, name, input, control)?;
    if let execution::Value::Frame(frame) = outcome.value {
        outcome.value = match frame.values().nodes.as_slice() {
            [wire::Value::Resource(_)] if frame.resources().len() == 1 => {
                execution::Value::Resource(frame.resources()[0])
            }
            [wire::Value::I64(value)] => {
                execution::Value::Copy(lang_runtime::layout::Immediate::I64(*value))
            }
            _ => panic!("quota result frame"),
        };
    }
    Ok(outcome)
}
fn quota_observed(h: &Harness, usage: e::Usage) -> quota::Observed {
    quota::Observed {
        allocation: h.allocator.stats(),
        requests: h.provider_requests.borrow().clone(),
        usage,
    }
}
fn quota_run(
    backend: usize,
    bounds: quota::Bounds,
    expected: Option<quota::Refusal>,
    cleanup_failure: bool,
) -> quota::Observed {
    let mut h = Harness::with_allocator_limits(bounds.allocator);
    h.task_limits = lang_runtime::task::Limits {
        live_tasks: 1,
        total_tasks: 1,
    };
    let control =
        e::Control::with_limits(bounds.execution).and_service_work(Some(bounds.service_work));
    let (provider, retained) = h.registration(cleanup_failure);
    let claim = match quota_call(&mut h, provider, "create", None, control) {
        Ok(outcome) => {
            let execution::Value::Resource(claim) = outcome.value else {
                panic!("retained resource")
            };
            claim
        }
        Err(failure) => {
            assert!(!cleanup_failure);
            expected
                .expect("initial quota")
                .check(&failure.reason, bounds);
            assert!(!failure.poisoned);
            assert!(h.values.is_empty());
            assert_eq!(h.allocator.stats().live_allocations, 0);
            let observed = quota_observed(&h, failure.usage);
            drop(failure);
            assert_eq!(retained.produced.get(), 0);
            assert_eq!(retained.destroyed.get(), 0);
            assert_eq!(retained.released.get(), 1);
            return observed;
        }
    };
    if let Some(quota::Refusal::Provider(index)) = expected {
        assert!(index >= h.provider_calls.get());
        h.provider_failure.set(Some(index));
    }
    let mut child = Child::new(&mut h.values, backend == 1, 1, usize::MAX);
    if backend == 2 {
        child.object_resolver = Some(resolve);
        child.object_record = Some(ObjectRecord {
            native: native_bytes("answer").into(),
            semantic: SEMANTIC.into(),
        });
    }
    let child = Rc::new(child);
    let (provider, native) = h.registration_child(false, Some(child.clone()));
    let observed = match quota_call(&mut h, provider, "consume", Some(claim), control) {
        Ok(outcome) => {
            assert!(
                expected.is_none(),
                "quota unexpectedly admitted {expected:?}"
            );
            assert!(matches!(
                outcome.value,
                execution::Value::Copy(lang_runtime::layout::Immediate::I64(42))
            ));
            assert_eq!(outcome.tasks.live, 0);
            assert_eq!(outcome.tasks.admitted, 1);
            assert_eq!(child.task.borrow().as_ref().unwrap().index(), 0);
            assert_eq!(child.task_usage.get().unwrap().admitted, 1);
            quota_observed(&h, outcome.usage)
        }
        Err(failure) => {
            let refusal = expected.expect("unexpected quota failure");
            refusal.check(&failure.reason, bounds);
            assert_eq!(failure.tasks.live, 0);
            assert_eq!(failure.poisoned, cleanup_failure);
            assert_eq!(h.state.poisoned(), cleanup_failure);
            assert_eq!(
                failure.report.as_ref().map_or(0, |r| r.cleanup_failures()),
                usize::from(cleanup_failure)
            );
            if cleanup_failure {
                assert_eq!(retained.released.get(), 0);
            }
            if let Some(task) = child.task.borrow().as_ref() {
                assert_eq!(task.index(), 0);
                assert_eq!(child.task_usage.get().unwrap().admitted, 1);
            }
            if let quota::Refusal::Provider(index) = refusal {
                assert_eq!(h.provider_calls.get(), index + 1);
            }
            let observed = quota_observed(&h, failure.usage);
            if retained.destroyed.get() == 0 {
                assert!(!cleanup_failure && !h.values.is_empty());
                let (provider, watch) = h.registration(false);
                quota_call(&mut h, provider, "cleanup", Some(claim), control)
                    .expect("smaller linked entry under unchanged ceilings");
                assert_eq!(watch.produced.get(), 0);
                assert_eq!(watch.released.get(), 1);
            }
            drop(failure);
            observed
        }
    };
    assert!(h.values.is_empty());
    assert_eq!(h.allocator.stats().live_allocations, 0);
    assert_eq!(h.allocator.limits(), bounds.allocator);
    assert_eq!(retained.produced.get(), 1);
    assert_eq!(retained.destroyed.get(), 1);
    assert_eq!(native.produced.get(), native.destroyed.get());
    let (provider, watch) = h.registration(false);
    prepared(provider, |program, _| {
        let entered = Cell::new(false);
        let result = h.state.with_program(program, |_| entered.set(true));
        if cleanup_failure {
            assert!(matches!(
                result,
                Err(r::Reason::Instance(execution::InstanceError::Poisoned))
            ));
            assert!(!entered.get());
        } else {
            result.unwrap();
            assert!(entered.get());
        }
    });
    assert_eq!(watch.produced.get(), 0);
    assert_eq!(watch.released.get(), 1);
    assert_eq!(retained.released.get(), 1);
    assert_eq!(native.released.get(), 1);
    observed
}
fn exhaustion_profiles() {
    let baseline = quota_run(0, quota::Bounds::generous(), None, false);
    verify_exhaustion_reference(&baseline);
    let exact = quota::Bounds::exact(&baseline);
    let mut profiles = 0;
    for backend in 0..3 {
        assert_eq!(
            quota_run(backend, quota::Bounds::generous(), None, false),
            baseline
        );
        assert_eq!(quota_run(backend, exact, None, false), baseline);
        profiles += 2;
    }
    for refusal in quota::cases(baseline.requests.len()) {
        for cleanup_failure in [false, true] {
            if cleanup_failure && !refusal.cleanup_failure() {
                continue;
            }
            let bounds = refusal.bounds(exact);
            let mut expected = None;
            for backend in 0..3 {
                let observed = quota_run(backend, bounds, Some(refusal), cleanup_failure);
                assert_eq!(
                    observed.requests,
                    baseline.requests[..observed.requests.len()]
                );
                assert!(
                    observed.allocation.peak_live_bytes <= bounds.allocator.live_bytes
                        && observed.allocation.cumulative_bytes
                            <= bounds.allocator.cumulative_bytes
                        && observed.allocation.peak_live_allocations
                            <= bounds.allocator.live_allocations
                );
                assert!(
                    observed
                        .requests
                        .iter()
                        .all(|r| r.bytes() <= bounds.allocator.request_bytes
                            && r.alignment() <= bounds.allocator.alignment)
                );
                assert!(
                    observed.usage.fuel <= bounds.execution.fuel.unwrap()
                        && observed.usage.host_calls <= bounds.execution.host_calls.unwrap()
                        && observed.usage.service_work <= bounds.service_work
                );
                if let Some(previous) = &expected {
                    assert_eq!(previous, &observed, "{refusal:?} cleanup={cleanup_failure}");
                } else {
                    expected = Some(observed);
                }
                profiles += 1;
            }
        }
    }
    assert_eq!(profiles, 72);
}
