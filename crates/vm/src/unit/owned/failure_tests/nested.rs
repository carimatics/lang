use super::*;

const COLLECTIONS: &str =
    include_str!("../../../../../../examples/scripts/nested_collections.lang");
const ARENA: &str = include_str!("../../../../../../examples/scripts/nested_arena.lang");

#[test]
fn nested_source_cleanup_preserves_reverse_order_spans_and_drains_after_leaf_error_or_panic() {
    for (source, expected, order) in [(COLLECTIONS, 8, vec![1]), (ARENA, 4, vec![1, 2])] {
        let program = checked(source);
        {
            let _guard = install(Injection::default());
            let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
            let mut instance = c::Instance::default();
            assert_eq!(
                execute_entry(
                    program.failure_flow(),
                    &mut instance,
                    Some(&mut allocator),
                    control()
                )
                .unwrap()
                .value,
                expected
            );
            assert_eq!(lengths(), order);
            assert!(events().iter().all(|event| event.span.is_some()));
            if source == ARENA {
                assert_eq!(events()[0].reason, c::Reason::Replacement);
            }
            assert_empty(&allocator);
        }
        for panic in [false, true] {
            let _guard = install(Injection {
                fail: if panic { vec![] } else { vec![0] },
                panic: panic.then_some(0),
                ..Default::default()
            });
            let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
            let mut instance = c::Instance::default();
            let error = execute_entry(
                program.failure_flow(),
                &mut instance,
                Some(&mut allocator),
                control(),
            )
            .err()
            .unwrap();
            assert_eq!(lengths(), order);
            let report = error.report.as_ref().unwrap();
            assert_eq!(
                report.primary().unwrap().kind,
                if panic {
                    c::FailureKind::CleanupPanic
                } else {
                    c::FailureKind::Cleanup(1001)
                }
            );
            assert_eq!(report.primary().unwrap().span, events()[0].span);
            assert_empty(&allocator);
            assert!(instance.is_poisoned());
            let before = allocator.stats();
            assert!(matches!(
                execute_entry(
                    program.failure_flow(),
                    &mut instance,
                    Some(&mut allocator),
                    control()
                )
                .err()
                .unwrap()
                .reason,
                Reason::Poisoned
            ));
            assert_eq!(allocator.stats(), before);
        }
    }
}

#[test]
fn nested_shared_body_panic_keeps_priority_and_frees_all_parent_container_frames() {
    let source = COLLECTIONS.replace("return inspect(value);", "let mut seen: i64 = inspect(value); return core::option_unwrap::<i64>(core::Option::None);");
    let program = checked(&source);
    let _guard = install(Injection {
        all: true,
        only_failure: true,
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let error = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    let report = error.report.as_ref().unwrap();
    assert_eq!(
        report.primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Panic)
    );
    assert_eq!(error.panic_code, Some(-1));
    assert_eq!(lengths(), [1]);
    assert_records(report.secondary(), &events());
    assert_empty(&allocator);
    assert!(instance.is_poisoned());
}

#[test]
fn nested_arena_replacement_interruption_drains_the_published_value_without_poison() {
    let program = checked(ARENA);
    let flag = Rc::new(crate::InterruptionFlag::new());
    let _guard = install(Injection {
        interrupt: Some(flag.clone()),
        ..Default::default()
    });
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let error = execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control().and_interruption(&flag),
    )
    .err()
    .unwrap();
    assert_eq!(
        error.report.as_ref().unwrap().primary().unwrap().kind,
        c::FailureKind::Execution(c::ExecutionFailure::Interrupted)
    );
    assert_eq!(lengths(), [1, 2]);
    assert_eq!(events()[1].reason, c::Reason::Failure);
    assert_empty(&allocator);
    assert!(!instance.is_poisoned());
}
