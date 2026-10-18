use super::*;

const TEXT: &str = r#"fn text(count:i64,byte:i64)->core::Text {
    let mut data:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
    let mut index:i64=0;while index < count {let mut pushed:core::Result<core::Unit,core::BufferError>=core::bytes_push(data,byte);index=index+1;}
    return core::result_unwrap::<core::Text,core::Utf8Error>(core::text_from_utf8(data));
}"#;
fn source(remove: bool) -> String {
    let mutation = if remove {
        "let mut query:core::Text=text(1,97);let mut removed:core::Option<core::Text>=core::map_remove::<core::Text,core::Text>(values,query);"
    } else {
        "let mut replaced:core::Result<core::Option<core::Text>,core::AllocError>=core::map_insert::<core::Text,core::Text>(values,text(1,97),text(3,67));"
    };
    format!(
        "{TEXT} fn main()->i64 {{let mut values:core::Map<core::Text,core::Text>=core::result_unwrap::<core::Map<core::Text,core::Text>,core::AllocError>(core::map_new::<core::Text,core::Text>(core::allocator()));let mut first:core::Result<core::Option<core::Text>,core::AllocError>=core::map_insert::<core::Text,core::Text>(values,text(1,97),text(2,66));{mutation}let mut later:core::Text=text(9,90);return core::map_len::<core::Text,core::Text>(values);}}"
    )
}
fn snippet(source: &str, event: &Event) -> String {
    let span = event.span.unwrap();
    source[span.start as usize..span.end as usize].to_owned()
}

#[test]
fn map_key_retirement_and_returned_values_keep_their_original_source_initializers() {
    for removal in [false, true] {
        let source = source(removal);
        let program = checked(&source);
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
            i64::from(!removal)
        );
        if removal {
            assert_eq!(lengths(), [1, 9, 2, 1]);
            assert_eq!(events()[0].reason, c::Reason::Explicit);
            assert!(snippet(&source, &events()[0]).contains("text(2,66)"));
            assert!(snippet(&source, &events()[2]).contains("text(2,66)"));
        } else {
            assert_eq!(lengths(), [1, 9, 2, 3, 1]);
            assert_eq!(events()[0].reason, c::Reason::Replacement);
            for (index, fragment) in [
                (0, "text(3,67)"),
                (2, "text(2,66)"),
                (3, "text(3,67)"),
                (4, "text(2,66)"),
            ] {
                assert!(
                    snippet(&source, &events()[index]).contains(fragment),
                    "{index}: {}",
                    snippet(&source, &events()[index])
                );
            }
        }
        assert_empty(&allocator);
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn map_key_cleanup_failure_or_panic_protects_old_values_stops_later_effects_and_poison_reentry() {
    for removal in [false, true] {
        for (fail, panic) in [(Some(0), None), (None, Some(0)), (Some(0), Some(1))] {
            let source = source(removal);
            let program = checked(&source);
            let _guard = install(Injection {
                fail: fail.into_iter().collect(),
                panic,
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
            assert_eq!(
                lengths(),
                if removal {
                    vec![1, 2, 1]
                } else {
                    vec![1, 2, 3, 1]
                }
            );
            let report = error.report.as_ref().unwrap();
            assert_eq!(
                report.primary().unwrap().kind,
                if fail.is_some() {
                    c::FailureKind::Cleanup(1001)
                } else {
                    c::FailureKind::CleanupPanic
                }
            );
            assert_eq!(report.primary().unwrap().span, events()[0].span);
            assert!(snippet(&source, &events()[1]).contains("text(2,66)"));
            if panic == Some(1) {
                assert_eq!(report.secondary().len(), 1);
                assert_eq!(report.secondary()[0].kind, c::FailureKind::CleanupPanic);
                assert_eq!(report.secondary()[0].span, events()[1].span);
            } else {
                assert!(report.secondary().is_empty());
            }
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
fn map_body_panic_keeps_priority_over_later_key_and_value_cleanup_errors() {
    let source = source(false).replace(
        "return core::map_len::<core::Text,core::Text>(values);",
        "return core::option_unwrap::<i64>(core::Option::None);",
    );
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
    assert_eq!(lengths(), [1, 9, 2, 3, 1]);
    assert_records(report.secondary(), &events()[1..]);
    assert!(instance.is_poisoned());
    assert_empty(&allocator);
}

#[test]
fn map_key_retirement_interruption_prevents_later_effects_and_retires_the_protected_output() {
    for removal in [false, true] {
        let program = checked(&source(removal));
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
        assert_eq!(
            lengths(),
            if removal {
                vec![1, 2, 1]
            } else {
                vec![1, 2, 3, 1]
            }
        );
        assert_eq!(events()[1].reason, c::Reason::Failure);
        assert!(!instance.is_poisoned());
        assert_empty(&allocator);
    }
}

#[test]
fn map_cleanup_report_truncation_never_skips_any_remaining_value() {
    let program = checked(
        r#"fn main()->i64 {
        let mut values:core::Map<i64,core::Bytes>=core::result_unwrap::<core::Map<i64,core::Bytes>,core::AllocError>(core::map_new::<i64,core::Bytes>(core::allocator()));
        let mut index:i64=0;while index < 260 {let mut data:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));let mut added:core::Result<core::Option<core::Bytes>,core::AllocError>=core::map_insert::<i64,core::Bytes>(values,index,data);index=index+1;}return 0;
    }"#,
    );
    let _guard = install(Injection {
        all: true,
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
        c::FailureKind::Cleanup(1000)
    );
    assert_eq!(report.secondary().len(), 255);
    assert_eq!(report.truncated(), 4);
    assert_eq!(lengths(), vec![0; 260]);
    assert!(instance.is_poisoned());
    assert_empty(&allocator);
}
