// Inserted into the standalone installed-SDK consumer with the real harness.
fn interruption_profiles() {
    let mut profiles = 0;
    for backend in 0..3 {
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
                let flag = Rc::new(e::InterruptionFlag::new());
                let mut child = Child::new(&mut h.values, backend == 1, 1, usize::MAX);
                if backend == 2 {
                    child.object_resolver = Some(resolve);
                    child.object_record = Some(ObjectRecord {
                        native: native_bytes("answer").into(),
                        semantic: SEMANTIC.into(),
                    });
                }
                child.interruption = Some(flag.clone());
                child.interrupt_at = point;
                let child = Rc::new(child);
                let (provider, watch) = h.registration_child(cleanup_failure, Some(child.clone()));
                if point == InterruptAt::BeforeAdmission {
                    flag.interrupt();
                }
                let control = e::Control::with_limits(Default::default()).and_interruption(&flag);
                let failure =
                    call_object_control(&mut h, provider.clone(), "create", None, control)
                        .err()
                        .unwrap_or_else(|| {
                            panic!(
                                "AOT success after interruption: backend={backend} point={point:?}"
                            )
                        });
                assert_eq!(
                    control_failure(&failure.reason),
                    Some(ChildControlFailure::Control(e::Failure::Interrupted)),
                    "backend={backend} point={point:?}: {:?}",
                    failure.reason
                );
                assert_eq!(failure.tasks.live, 0);
                assert_eq!(failure.poisoned, cleanup_failure);
                assert_eq!(h.state.poisoned(), cleanup_failure);
                assert_eq!(
                    watch.produced.get(),
                    usize::from(point == InterruptAt::NativeReturn)
                );
                assert_eq!(watch.produced.get(), watch.destroyed.get());
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
                    child.deferred_makes.set(1);
                    let created = call_object(&mut h, provider.clone(), "create", None).unwrap();
                    let execution::Value::Frame(frame) = created.value else {
                        panic!("resource frame")
                    };
                    let claim = frame.resources()[0];
                    drop(frame);
                    let consumed =
                        call_object(&mut h, provider.clone(), "consume", Some(claim)).unwrap();
                    let execution::Value::Frame(frame) = consumed.value else {
                        panic!("scalar frame")
                    };
                    assert!(matches!(
                        frame.values().nodes.as_slice(),
                        [wire::Value::I64(42)]
                    ));
                    drop(frame);
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
    assert_eq!(profiles, 15);
}
