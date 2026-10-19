use super::*;

#[test]
fn explicit_panic_diverges_without_fabricating_generic_results() {
    for text in [
        include_str!("../../examples/analysis/explicit_panic.lang").to_owned(),
        include_str!("../../examples/analysis/explicit_panic_owned.lang").to_owned(),
        "fn main() -> i64 { let mut never: &i64 = core::panic::<&i64>(42); }".to_owned(),
        "fn main() -> i64 { let mut never: &mut i64 = core::panic::<&mut i64>(42); }".to_owned(),
        "fn main() -> i64 { let mut never: &core::Allocator = core::panic::<&core::Allocator>(42); }".to_owned(),
        "fn main() -> i64 { (*core::panic::<&mut i64>(42)) = 1; }".to_owned(),
        "fn main() -> i64 { let mut x: i64 = 0; x = core::panic::<i64>(42); }".to_owned(),
        "fn read(p: &i64) -> i64 { return (*p); } fn main() -> i64 { return read(core::panic::<&i64>(42)); }".to_owned(),
        "fn main() -> i64 { return core::panic::<i64>(core::panic::<i64>(42)); }".to_owned(),
        "fn main() -> i64 { let mut d: core::Unit = core::drop::<core::Bytes>(core::panic::<core::Bytes>(42)); }".to_owned(),
        "fn main() -> i64 { if core::panic::<bool>(42) { return 1; } }".to_owned(),
        "fn main() -> i64 { while core::panic::<bool>(42) { } }".to_owned(),
        "fn main() -> i64 { match core::panic::<bool>(42) { true => { }, false => { }, } }".to_owned(),
    ] {
        let program = checked(&text);
        let typed = program.mir().activity().moved().typed();
        assert!(typed.has_explicit_panic());
        for block in typed.functions().iter().flat_map(|f| &f.blocks) {
            if let Some(last) = block.instructions.last()
                && matches!(last.op, lang_mir::unit::ownership::bodies::Op::Panic(_))
            {
                assert!(last.results.is_empty());
                assert!(matches!(block.terminator.kind, lang_mir::unit::ownership::bodies::Term::Unreachable));
            }
        }
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        let failure = vm::execute_entry(program.failure_flow(), &mut instance, Some(&mut allocator), control()).err().unwrap();
        assert!(matches!(failure.reason, vm::Reason::Execution(c::FailureKind::Execution(ExecutionFailure::Panic))));
        assert_eq!(failure.panic_code, Some(if text.contains("fail(-42)") { -42 } else { 42 }));
        let span = failure.fault.unwrap();
        assert!(text[span.start as usize..span.end as usize].starts_with("core::panic"));
        assert!(!instance.is_poisoned());
        assert_all_released(&allocator, &trace.borrow());
    }
}

#[test]
fn explicit_panic_type_checks_unreachable_source_and_stops_owner_analysis() {
    let prefix = "fn take(a: core::Bytes, n: i64, b: core::Bytes) -> i64 { return n; } fn main() -> i64 { let mut x: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); let mut d: core::Unit = core::drop::<core::Bytes>(x); return take(core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())), core::panic::<i64>(42), x); }";
    let program = checked(prefix);
    let (mut allocator, trace) = allocator();
    let failure = vm::execute_entry(
        program.failure_flow(),
        &mut c::Instance::default(),
        Some(&mut allocator),
        control(),
    )
    .err()
    .unwrap();
    assert_eq!(failure.panic_code, Some(42));
    assert_all_released(&allocator, &trace.borrow());
    for (text, code) in [
        (
            "fn main() -> i64 { return core::panic::<i64>(true); }",
            lang_diagnostics::codes::TYPE_MISMATCH,
        ),
        (
            "fn main() -> i64 { return core::panic(42); }",
            lang_diagnostics::codes::UNSUPPORTED_SPECIALIZATION,
        ),
        (
            "fn main() -> i64 { let mut stopped: core::Unit = core::panic::<core::Unit>(42); return true; }",
            lang_diagnostics::codes::EXPECTED_I64,
        ),
    ] {
        let analysis = frontend::analyze_virtual(
            SourceMap::new(),
            Path::new("panic.lang"),
            text.as_bytes(),
            None,
            Default::default(),
            None,
        );
        assert!(analysis.result.is_err());
        assert!(
            analysis.diagnostics.iter().any(|d| d.code == code),
            "{:?}",
            analysis.diagnostics
        );
    }
}
