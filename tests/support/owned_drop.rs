use super::*;
use lang_diagnostics::{DiagnosticCode, codes};

#[test]
fn source_drop_preserves_copy_values_and_uses_existing_owner_cleanup_mir() {
    let cases = [
        (
            include_str!("../../examples/analysis/explicit_drop.lang"),
            42,
            1,
        ),
        (
            include_str!("../../examples/analysis/explicit_drop_shared.lang"),
            42,
            1,
        ),
        (
            "struct Pair { number: i64, flag: bool } fn main() -> i64 { let mut n: i64 = 17; let mut d: core::Unit = core::drop::<i64>(n); let mut pair: Pair = Pair { number: 25, flag: true }; let mut dropped: core::Unit = core::drop::<Pair>(pair); let mut unit: core::Unit = core::drop::<core::Unit>(core::Unit {}); if pair.flag { return n + pair.number; } return 0; }",
            42,
            0,
        ),
        (
            "fn main() -> i64 { let mut dropped: core::Unit = core::drop::<core::Result<core::Bytes, core::AllocError>>(core::bytes_new(core::allocator())); return 42; }",
            42,
            1,
        ),
    ];
    for (text, expected, drops) in cases {
        let program = checked(text);
        let instructions = program
            .mir()
            .activity()
            .moved()
            .typed()
            .functions()
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.instructions);
        let explicit: Vec<_> = instructions
            .filter(|i| matches!(i.op, lang_mir::unit::ownership::bodies::Op::DropValue(_)))
            .collect();
        assert_eq!(explicit.len(), drops);
        for instruction in explicit {
            let span = instruction.span;
            assert!(text[span.start as usize..span.end as usize].starts_with("core::drop"));
        }
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        let outcome = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .unwrap();
        assert_eq!(outcome.value, expected);
        assert!(!instance.is_poisoned());
        assert_all_released(&allocator, &trace.borrow());
    }
}

fn rejects(text: &str, expected: DiagnosticCode) {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("drop.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(analysis.result.is_err());
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == expected),
        "{text}: {:?}",
        analysis.diagnostics
    );
    for diagnostic in analysis.diagnostics {
        analysis.sources.slice(diagnostic.primary.span).unwrap();
    }
}
#[test]
fn source_drop_enforces_moves_lexical_loans_and_finite_specialization() {
    rejects(
        "fn f(bytes: core::Bytes) -> i64 { let mut d: core::Unit = core::drop::<core::Bytes>(bytes); return core::bytes_len(bytes); }",
        codes::INVALID_OWNER_USE,
    );
    rejects(
        "fn f(bytes: core::Bytes) -> i64 { let mut loan: &core::Bytes = &bytes; let mut d: core::Unit = core::drop::<core::Bytes>(bytes); return 0; }",
        codes::INVALID_LOAN,
    );
    for loan in ["&core::Bytes", "&mut core::Bytes", "&core::Allocator"] {
        rejects(
            &format!(
                "fn f(value: {loan}) -> i64 {{ let mut d: core::Unit = core::drop::<{loan}>(value); return 0; }}"
            ),
            codes::UNSUPPORTED_SPECIALIZATION,
        );
    }
    for call in ["core::drop(1)", "core::drop::<i64, bool>(1)"] {
        rejects(
            &format!("fn f() -> i64 {{ let mut d: core::Unit = {call}; return 0; }}"),
            codes::UNSUPPORTED_SPECIALIZATION,
        );
    }
    for call in [
        "core::drop::<i64>()",
        "core::drop::<bool>(1)",
        "core::drop::<i64>(1, 2)",
    ] {
        rejects(
            &format!("fn f() -> i64 {{ let mut d: core::Unit = {call}; return 0; }}"),
            codes::TYPE_MISMATCH,
        );
    }
    rejects(
        "struct Pair { first: core::Bytes, second: core::Bytes } fn f(pair: Pair) -> i64 { let mut d: core::Unit = core::drop::<core::Bytes>(pair.first); return 0; }",
        codes::INVALID_OWNERSHIP_TYPE,
    );
}
