use super::*;

#[test]
fn original_interruption_survives_callback_boundaries_and_mandatory_cleanup() {
    let mut profiles = 0;
    for stored in [false, true] {
        for compiled in [false, true] {
            for point in [
                InterruptAt::BeforeAdmission,
                InterruptAt::ChildEntry,
                InterruptAt::ChildReturn,
                InterruptAt::NativeReturn,
            ] {
                for cleanup_failure in [false, true] {
                    if cleanup_failure && point != InterruptAt::NativeReturn {
                        continue;
                    }
                    let mut h = Harness::new();
                    let semantic = if stored {
                        let unit = stored_unit();
                        let snapshot = unit.code().declarations().types().table().sources
                            [unit.document().document().source.0 as usize]
                            .clone();
                        Some(
                            lang::artifact::component::encode(
                                unit,
                                snapshot,
                                None,
                                Default::default(),
                            )
                            .unwrap(),
                        )
                    } else {
                        None
                    };
                    if let Some(bytes) = &semantic {
                        h.unit = lang::artifact::component::decode(bytes, Default::default())
                            .unwrap()
                            .into_unit();
                    }
                    let flag = Rc::new(e::InterruptionFlag::new());
                    let mut child = Child::new(&mut h.values, compiled, 1, usize::MAX);
                    if let Some(bytes) = &semantic {
                        child.unit = lang::artifact::component::decode(bytes, Default::default())
                            .unwrap()
                            .into_unit();
                    }
                    child.interruption = Some(flag.clone());
                    child.interrupt_at = point;
                    let child = Rc::new(child);
                    let (provider, watch) =
                        h.registration_child(cleanup_failure, Some(child.clone()));
                    if point == InterruptAt::BeforeAdmission {
                        flag.interrupt();
                    }
                    let control =
                        e::Control::with_limits(Default::default()).and_interruption(&flag);
                    let failure = h.call_control(provider.clone(), "create", None, compiled, control).err().unwrap_or_else(|| panic!("interruption returned success: stored={stored} compiled={compiled} point={point:?}"));
                    assert_eq!(
                        control_failure(&failure.reason),
                        Some(ChildControlFailure::Control(e::Failure::Interrupted)),
                        "stored={stored} compiled={compiled} point={point:?}: {:?}",
                        failure.reason
                    );
                    assert!(flag.is_interrupted());
                    assert_eq!(failure.poisoned, cleanup_failure);
                    assert_eq!(h.state.poisoned(), cleanup_failure);
                    assert_eq!(
                        watch.produced.get(),
                        usize::from(point == InterruptAt::NativeReturn)
                    );
                    assert_eq!(watch.destroyed.get(), watch.produced.get());
                    assert!(h.values.is_empty());
                    assert_eq!(h.allocator.stats().live_allocations, 0);
                    if point != InterruptAt::BeforeAdmission {
                        assert_eq!(child.task.borrow().as_ref().unwrap().index(), 0);
                        assert_eq!(child.task_usage.get().unwrap().admitted, 1);
                        assert_eq!(child.task_observations.get(), 1);
                        assert_eq!(h.engine.check(&child.parent.get().unwrap()), 5);
                    }
                    if cleanup_failure {
                        assert_eq!(failure.report.as_ref().unwrap().cleanup_failures(), 1);
                        let mut entered = false;
                        prepared_unit(&h.unit, provider.clone(), |program, _| {
                            assert!(h.state.with_program(program, |_| entered = true).is_err());
                        });
                        assert!(!entered);
                    } else {
                        // A fresh root has its own control. The old flag stays
                        // interrupted and cannot be reset by the callback.
                        // Do not replay the callback's diagnostic cache branded
                        // with the previous root's TaskId.
                        child.deferred_makes.set(1);
                        let result = h.call(provider.clone(), "create", None, compiled).unwrap();
                        let execution::Value::Resource(claim) = result.value else {
                            panic!("resource")
                        };
                        let result = h
                            .call(provider.clone(), "consume", Some(claim), compiled)
                            .unwrap();
                        assert!(matches!(
                            result.value,
                            execution::Value::Copy(lang_runtime::layout::Immediate::I64(42))
                        ));
                        assert!(flag.is_interrupted());
                        assert!(h.values.is_empty());
                        assert_eq!(h.allocator.stats().live_allocations, 0);
                    }
                    drop(provider);
                    if point != InterruptAt::BeforeAdmission {
                        assert_eq!(watch.released.get(), 0);
                    }
                    drop(failure);
                    assert_eq!(watch.released.get(), 1);
                    profiles += 1;
                }
            }
        }
    }
    assert_eq!(profiles, 20);
}
