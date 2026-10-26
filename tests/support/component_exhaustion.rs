use super::*;
#[path = "component_exhaustion_checks.rs"]
mod checks;
use checks::*;

const SOURCE: &[u8] = include_bytes!("../fixtures/component_scoped_roots_exhaustion.lang");
fn unit(stored: bool) -> (bindings::Unit, Option<Vec<u8>>) {
    unit_callback_results(SOURCE, false, false, false, false, false, stored)
}
fn observed(h: &Harness, usage: e::Usage) -> Observed {
    Observed {
        allocation: h.allocator.stats(),
        requests: h.provider_requests.borrow().clone(),
        usage,
    }
}
fn refuse(
    compiled: bool,
    stored: bool,
    bounds: Bounds,
    refusal: Refusal,
    cleanup_failure: bool,
) -> Observed {
    let mut h = Harness::with_allocator_limits(bounds.allocator);
    (h.unit, h.source_idl) = unit(stored);
    h.task_limits = lang_runtime::task::Limits {
        live_tasks: 1,
        total_tasks: 1,
    };
    let control =
        e::Control::with_limits(bounds.execution).and_service_work(Some(bounds.service_work));
    let (provider, retained) = h.registration(cleanup_failure);
    let created = h.call_control(provider, "create", None, compiled, control);
    let claim = match created {
        Ok(outcome) => {
            let execution::Value::Resource(claim) = outcome.value else {
                panic!("resource")
            };
            claim
        }
        Err(failure) => {
            assert!(!cleanup_failure);
            refusal.check(&failure.reason, bounds);
            assert!(!failure.poisoned);
            assert!(h.values.is_empty());
            assert_eq!(h.allocator.stats().live_allocations, 0);
            let observed = observed(&h, failure.usage);
            drop(failure);
            assert_eq!(retained.produced.get(), 0);
            assert_eq!(retained.destroyed.get(), 0);
            assert_eq!(retained.released.get(), 1);
            return observed;
        }
    };
    if let Refusal::Provider(index) = refusal {
        assert!(index >= h.provider_calls.get());
        h.provider_failure.set(Some(index));
    }
    let mut child = Child::new(&mut h.values, compiled, 1, usize::MAX);
    child.unit = unit(true).0;
    let child = Rc::new(child);
    let (provider, native) = h.registration_child(false, Some(child.clone()));
    let failure = h
        .call_control(provider, "consume", Some(claim), compiled, control)
        .err()
        .unwrap_or_else(|| panic!("quota unexpectedly admitted {refusal:?}"));
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
    if let Refusal::Provider(index) = refusal {
        assert_eq!(h.provider_calls.get(), index + 1);
    }
    let result = observed(&h, failure.usage);
    if retained.destroyed.get() == 0 {
        println!("caller custody after {refusal:?}");
        // Rejected argument admission keeps caller custody. A smaller root can
        // retire it under the same allocator/control ceilings and history.
        assert!(!cleanup_failure && !h.values.is_empty());
        h.unit = stored_unit();
        h.source_idl = None;
        let (provider, watch) = h.registration(false);
        h.call_control(provider, "consume", Some(claim), compiled, control)
            .expect("smaller cleanup under unchanged ceilings");
        assert_eq!(watch.produced.get(), 0);
        assert_eq!(watch.released.get(), 1);
    }
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
    drop(failure);
    assert_eq!(retained.released.get(), 1);
    assert_eq!(native.released.get(), 1);
    result
}

#[test]
fn simultaneous_bounds_preserve_each_refusal_provider_prefix_and_cleanup() {
    let baseline = baseline(false, false);
    let exact = Bounds::exact(&baseline);
    let count = baseline.requests.len();
    let cases = cases(count);
    let mut profiles = 0;
    for refusal in cases {
        for cleanup_failure in [false, true] {
            if cleanup_failure && !refusal.cleanup_failure() {
                continue;
            }
            let bounds = refusal.bounds(exact);
            let mut expected = None;
            for stored in [false, true] {
                for compiled in [false, true] {
                    let result = refuse(compiled, stored, bounds, refusal, cleanup_failure);
                    assert_eq!(result.requests, baseline.requests[..result.requests.len()]);
                    assert!(
                        result.allocation.peak_live_bytes <= bounds.allocator.live_bytes
                            && result.allocation.cumulative_bytes
                                <= bounds.allocator.cumulative_bytes
                            && result.allocation.peak_live_allocations
                                <= bounds.allocator.live_allocations
                    );
                    assert!(
                        result
                            .requests
                            .iter()
                            .all(|r| r.bytes() <= bounds.allocator.request_bytes
                                && r.alignment() <= bounds.allocator.alignment)
                    );
                    assert!(
                        result.usage.fuel <= bounds.execution.fuel.unwrap()
                            && result.usage.host_calls <= bounds.execution.host_calls.unwrap()
                            && result.usage.service_work <= bounds.service_work
                    );
                    if let Some(previous) = &expected {
                        assert_eq!(previous, &result, "{refusal:?} cleanup={cleanup_failure}");
                    } else {
                        expected = Some(result);
                    }
                    profiles += 1;
                }
            }
        }
    }
    assert_eq!(profiles, 88);
}
fn baseline(compiled: bool, stored: bool) -> Observed {
    run_success(compiled, stored, Bounds::generous())
}
pub(super) fn reference() -> (a::Stats, Vec<a::Request>, e::Usage) {
    let observed = baseline(false, true);
    (observed.allocation, observed.requests, observed.usage)
}
fn run_success(compiled: bool, stored: bool, bounds: Bounds) -> Observed {
    let mut h = Harness::with_allocator_limits(bounds.allocator);
    (h.unit, h.source_idl) = unit(stored);
    h.task_limits = lang_runtime::task::Limits {
        live_tasks: 1,
        total_tasks: 1,
    };
    let control =
        e::Control::with_limits(bounds.execution).and_service_work(Some(bounds.service_work));
    let (provider, retained) = h.registration(false);
    let first = h
        .call_control(provider, "create", None, compiled, control)
        .unwrap();
    let execution::Value::Resource(claim) = first.value else {
        panic!("retained owner")
    };
    let mut child = Child::new(&mut h.values, compiled, 1, usize::MAX);
    // The native callback carries its original IDL snapshot, as in the existing
    // allocator fixture; only the outer source/artifact admission varies.
    child.unit = unit(true).0;
    let child = Rc::new(child);
    let (provider, native) = h.registration_child(false, Some(child.clone()));
    let result = h
        .call_control(provider, "consume", Some(claim), compiled, control)
        .unwrap();
    assert!(matches!(
        result.value,
        execution::Value::Copy(lang_runtime::layout::Immediate::I64(42))
    ));
    assert_eq!(result.tasks.live, 0);
    assert_eq!(result.tasks.admitted, 1);
    assert_eq!(child.task.borrow().as_ref().unwrap().index(), 0);
    assert_eq!(child.task_usage.get().unwrap().admitted, 1);
    assert!(h.values.is_empty());
    assert_eq!(h.allocator.stats().live_allocations, 0);
    for watch in [&retained, &native] {
        assert_eq!(watch.produced.get(), 1);
        assert_eq!(watch.destroyed.get(), 1);
        assert_eq!(watch.released.get(), 1);
    }
    let requests = h.provider_requests.borrow().clone();
    Observed {
        allocation: h.allocator.stats(),
        requests,
        usage: result.usage,
    }
}
#[test]
fn combined_exhaustion_fixture_has_one_original_task_and_shared_live_storage() {
    let mut expected = None;
    for stored in [false, true] {
        for compiled in [false, true] {
            let observed = baseline(compiled, stored);
            println!(
                "exhaustion baseline stored={stored} compiled={compiled}: {:?}; max request={}; max alignment={}",
                observed.allocation,
                observed.requests.iter().map(|r| r.bytes()).max().unwrap(),
                observed
                    .requests
                    .iter()
                    .map(|r| r.alignment())
                    .max()
                    .unwrap()
            );
            if let Some(previous) = &expected {
                assert_eq!(previous, &observed);
            } else {
                expected = Some(observed);
            }
        }
    }
}

#[test]
fn simultaneous_exact_allocator_execution_and_task_limits_admit_the_same_run() {
    let expected = baseline(false, false);
    let bounds = Bounds::exact(&expected);
    println!("simultaneous exact bounds {bounds:?}");
    for stored in [false, true] {
        for compiled in [false, true] {
            assert_eq!(run_success(compiled, stored, bounds), expected);
        }
    }
}
