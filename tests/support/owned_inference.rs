use super::*;
use lang_diagnostics::codes;
use lang_hir::{program::CheckInputErrorKind, resolved as r};

const WRAPPER: &str = "__repl_eval_inferred";
const OWNED: &str = include_str!("../../examples/analysis/repl_preview.lang");
fn analyze(prefix: &str, expression: &str, limits: frontend::CompileLimits) -> frontend::Analysis {
    let source = format!("{prefix}\nfn {WRAPPER}() -> i64 {{ return {expression}; }}");
    analyze_source(&source, WRAPPER, limits)
}
fn analyze_source(
    source: &str,
    wrapper: &str,
    limits: frontend::CompileLimits,
) -> frontend::Analysis {
    frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("inferred.lang"),
        source.as_bytes(),
        None,
        limits,
        Some(wrapper),
    )
}

#[test]
fn inferred_source_results_reach_independent_mir_and_execute_without_guest_storage_leaks() {
    for (prefix, expression, expected) in [
        (
            "",
            "core::bytes_new(core::allocator())",
            "core::Result::Ok(core::Bytes [])",
        ),
        (
            "",
            "core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()))",
            "core::Bytes []",
        ),
        ("", "core::Unit {}", "core::Unit {}"),
        (
            "",
            "core::AllocError::OutOfMemory",
            "core::AllocError::OutOfMemory",
        ),
        (OWNED, "fresh_text()", "core::Text \"\""),
        (
            OWNED,
            "preview_message()",
            "core::Result::Ok(PreviewMessage { bytes: core::Bytes [], text: core::Text \"\", count: 7 })",
        ),
        (OWNED, "1 + 2", "3"),
        (OWNED, "true", "true"),
    ] {
        let analysis = analyze(prefix, expression, Default::default());
        assert!(
            analysis.diagnostics.is_empty(),
            "{expression}: {:?}",
            analysis.diagnostics
        );
        let program = analysis
            .result
            .unwrap_or_else(|e| panic!("{expression}: {e:?}"));
        let index = program
            .source()
            .functions
            .iter()
            .position(|f| f.qualified_name == WRAPPER)
            .unwrap();
        let signature = program.source().functions[index].result;
        assert_eq!(analysis.sources.slice(signature.span).unwrap(), "i64");
        if expected == "3" {
            assert_eq!(signature.value_type, r::Type::I64);
        } else {
            assert_ne!(signature.value_type, r::Type::I64);
        }
        let Program::Ownership(program) = program else {
            panic!("ownership profile: {expression}");
        };
        let (mut allocator, trace) = allocator_with_limits(a::Limits {
            live_allocations: 4096,
            ..Default::default()
        });
        let mut instance = c::Instance::default();
        let outcome = vm::execute_preview(
            program.failure_flow(),
            lang_mir::unit::FunctionId(index as u32),
            &mut instance,
            Some(&mut allocator),
            control(),
            65536,
        )
        .unwrap();
        assert_eq!(outcome.value, expected);
        assert_all_released(&allocator, &trace.borrow());
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn normal_annotations_moves_and_loans_are_still_checked_in_an_inferred_program() {
    for (declaration, expected) in [
        (
            "fn bad() -> i64 { return fresh_bytes(); }",
            codes::EXPECTED_I64,
        ),
        (
            "fn bad() -> i64 { let mut bytes: core::Bytes = fresh_bytes(); let mut moved: core::Bytes = bytes; return core::bytes_len(bytes); }",
            codes::INVALID_OWNER_USE,
        ),
        (
            "fn bad() -> i64 { let mut bytes: core::Bytes = fresh_bytes(); let mut loan: &core::Bytes = &bytes; let mut pushed: core::Result<core::Unit, core::BufferError> = core::bytes_push(&mut bytes, 0); return core::bytes_len(loan); }",
            codes::INVALID_LOAN,
        ),
        ("fn bad() -> &i64 { return 0; }", codes::REGION_ESCAPE),
    ] {
        let analysis = analyze(
            &format!("{OWNED}\n{declaration}"),
            "fresh_bytes()",
            Default::default(),
        );
        assert!(analysis.result.is_err(), "{declaration}");
        assert!(
            analysis.diagnostics.iter().any(|d| d.code == expected),
            "{declaration}: {:?}",
            analysis.diagnostics
        );
    }
    let source = format!("{OWNED}\nfn __repl_eval_user() -> i64 {{ return fresh_bytes(); }}");
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("ordinary.lang"),
        source.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analysis.result.is_err(),
        "a wrapper-like name does not opt into inference"
    );
}

#[test]
fn inferred_loans_and_ambiguous_sums_cannot_escape_normal_expression_checks() {
    for (expression, expected, label) in [
        (
            "core::allocator()",
            codes::REGION_ESCAPE,
            "core::allocator()",
        ),
        (
            "core::vec_get::<i64>(core::result_unwrap::<core::Vec<i64>, core::AllocError>(core::vec_new::<i64>(core::allocator())), 0)",
            codes::INVALID_LOAN,
            "core::result_unwrap::<core::Vec<i64>, core::AllocError>(core::vec_new::<i64>(core::allocator()))",
        ),
        (
            "core::Option::None",
            codes::TYPE_MISMATCH,
            "core::Option::None",
        ),
    ] {
        let analysis = analyze("", expression, Default::default());
        assert!(analysis.result.is_err(), "{expression}");
        let diagnostic = analysis
            .diagnostics
            .iter()
            .find(|d| d.code == expected)
            .unwrap_or_else(|| panic!("{expression}: {:?}", analysis.diagnostics));
        assert_eq!(
            analysis.sources.slice(diagnostic.primary.span).unwrap(),
            label
        );
    }
}

#[test]
fn wrapper_shape_and_no_caller_rules_apply_to_both_source_profiles() {
    for prefix in ["", OWNED] {
        for (declaration, name) in [
            ("fn ordinary() -> i64 { return 1; }", "ordinary"),
            (
                "pub fn __repl_eval_inferred() -> i64 { return 1; }",
                WRAPPER,
            ),
            (
                "fn __repl_eval_inferred(x: i64) -> i64 { return x; }",
                WRAPPER,
            ),
            (
                "fn __repl_eval_inferred() -> bool { return true; }",
                WRAPPER,
            ),
            (
                "fn __repl_eval_inferred() -> i64 { let mut x: i64 = 1; return x; }",
                WRAPPER,
            ),
            (
                "fn __repl_eval_inferred() -> i64 { if true { return 1; } return 0; }",
                WRAPPER,
            ),
            (
                "fn __repl_eval_inferred() -> i64 { return __repl_eval_inferred(); }",
                WRAPPER,
            ),
            (
                "fn caller() -> i64 { return __repl_eval_inferred(); } fn __repl_eval_inferred() -> i64 { return 1; }",
                WRAPPER,
            ),
        ] {
            let analysis = analyze_source(
                &format!("{prefix}\n{declaration}"),
                name,
                Default::default(),
            );
            assert!(
                matches!(
                    analysis.result,
                    Err(frontend::AnalysisError::Typing(
                        lang_hir::program::CheckError::Input {
                            kind: CheckInputErrorKind::InvalidExpressionFunction,
                            ..
                        }
                    ))
                ),
                "{declaration}: {analysis:?}"
            );
        }
    }
}

#[test]
fn inferred_analysis_preserves_hir_and_independent_mir_resource_diagnostics() {
    for stage in 0..3 {
        let mut limits = frontend::CompileLimits::default();
        match stage {
            0 => limits.typing.work = 0,
            1 => limits.typing.types.work = 0,
            _ => limits.ownership.bodies.places.ownership.unit.work = 0,
        }
        let analysis = analyze("", "core::bytes_new(core::allocator())", limits);
        assert!(analysis.result.is_err());
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|d| d.code == codes::RESOURCE_LIMIT),
            "{stage}: {analysis:?}"
        );
    }
}
