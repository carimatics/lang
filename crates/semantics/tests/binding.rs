use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lang_diagnostics::{SourceMap, codes};
use lang_hir::resolved as r;
use lang_semantics::binding::{BindingAnalysis, BindingError, BindingLimits, bind_program};
use lang_semantics::modules::{ModuleGraph, ModuleLimits, load_modules};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new(text: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "lang-binding-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let files = Self(path);
        files.write("main.lang", text);
        files
    }
    fn write(&self, name: &str, text: &str) {
        let file = self.0.join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
    }
    fn graph(&self) -> (SourceMap, ModuleGraph) {
        let mut sources = SourceMap::new();
        let analysis = load_modules(
            &mut sources,
            &self.0.join("main.lang"),
            &self.0,
            ModuleLimits::default(),
        )
        .unwrap();
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        (sources, analysis.graph.unwrap())
    }
    fn bind(&self, limits: BindingLimits) -> (SourceMap, Result<BindingAnalysis, BindingError>) {
        let (sources, graph) = self.graph();
        let analysis = bind_program(&graph, &sources, limits);
        match &analysis {
            Ok(a) => {
                for d in &a.diagnostics {
                    d.validate(&sources).unwrap();
                }
            }
            Err(BindingError::Limit(d)) => {
                d.validate(&sources).unwrap();
            }
            Err(error) => panic!("unexpected binding input error: {error}"),
        }
        (sources, analysis)
    }
    fn program(&self) -> (SourceMap, r::Program) {
        let (sources, analysis) = self.bind(BindingLimits::default());
        let analysis = analysis.unwrap();
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        let program = analysis.program.unwrap();
        validate(&sources, &program);
        (sources, program)
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn validate(sources: &SourceMap, program: &r::Program) {
    for module in &program.modules {
        sources.slice(module.span).unwrap();
    }
    for definition in &program.definitions {
        sources.slice(definition.span).unwrap();
        assert_eq!(
            sources.slice(definition.name.span).unwrap(),
            definition.name.text
        );
    }
    for function in &program.functions {
        sources.slice(function.span).unwrap();
        sources.slice(function.signature_span).unwrap();
        assert!(function.module.0 < program.modules.len());
        assert!(function.body.0 < function.blocks.len());
        for parameter in &function.parameters {
            assert_eq!(function.locals[parameter.0].kind, r::BindingKind::Parameter);
        }
        for local in &function.locals {
            assert_eq!(sources.slice(local.name.span).unwrap(), local.name.text);
        }
        for expression in &function.expressions {
            sources.slice(expression.span).unwrap();
            match &expression.kind {
                r::ExpressionKind::Local(id) => assert!(id.0 < function.locals.len()),
                r::ExpressionKind::Add(a, b) | r::ExpressionKind::LessThan(a, b) => {
                    assert!(a.0 < function.expressions.len());
                    assert!(b.0 < function.expressions.len());
                }
                r::ExpressionKind::Call { callee, arguments } => {
                    for a in arguments {
                        assert!(a.0 < function.expressions.len());
                    }
                    match callee {
                        r::Callable::Guest(id) => assert!(id.0 < program.functions.len()),
                        r::Callable::Host(id) => assert!(id.0 < program.imports.len()),
                        r::Callable::Case { definition, .. } => {
                            assert!(definition.0 < program.definitions.len())
                        }
                    }
                }
                _ => {}
            }
        }
        for pattern in &function.patterns {
            sources.slice(pattern.span).unwrap();
        }
        for block in &function.blocks {
            sources.slice(block.span).unwrap();
            assert_eq!(sources.slice(block.closing_brace).unwrap(), "}");
            for statement in &block.statements {
                sources.slice(statement.span).unwrap();
            }
        }
    }
}

#[test]
fn core_specializations_resolve_names_even_in_unreachable_source() {
    rejects(
        "fn f() -> i64 { return core::vec_len::<i64>(values); }",
        codes::UNKNOWN_NAME,
    );
    rejects(
        "fn f() -> i64 { return 1; let mut x: i64 = core::vec_len::<i64>(values); }",
        codes::UNKNOWN_NAME,
    );
}

#[test]
fn every_core_operation_binds_with_concrete_type_arguments_and_written_value_order() {
    use lang_hir::core::Operation;
    for operation in Operation::ALL {
        let signature = operation.signature();
        let specialization = match signature.type_arguments.len() {
            0 => "",
            1 => "::<i64>",
            2 => "::<i64, bool>",
            _ => panic!(),
        };
        let arguments = ["a", "b", "c"][..signature.parameters.len()].join(", ");
        // Most calls deliberately use invalid value types. Panic's concrete
        // i64 argument/result is valid and selects the ownership profile.
        let text = format!(
            "fn f(a: i64, b: i64, c: i64) -> i64 {{ return core::{}{specialization}({arguments}); }}",
            operation.name()
        );
        let (sources, program) = Files::new(&text).program();
        let f = &program.functions[0];
        let r::StatementKind::Return(result) = f.blocks[f.body.0].statements[0].kind else {
            panic!()
        };
        let r::ExpressionKind::CoreCall {
            operation: bound,
            ref type_arguments,
            ref arguments,
            callee_span,
        } = f.expressions[result.0].kind
        else {
            panic!()
        };
        assert_eq!(bound, operation);
        assert_eq!(type_arguments.len(), signature.type_arguments.len());
        for (i, argument) in type_arguments.iter().enumerate() {
            assert_eq!(sources.slice(argument.span).unwrap(), ["i64", "bool"][i]);
        }
        for (i, argument) in arguments.iter().enumerate() {
            assert!(
                matches!(f.expressions[argument.0].kind, r::ExpressionKind::Local(r::LocalId(index)) if index == i)
            );
        }
        assert_eq!(
            sources.slice(callee_span).unwrap(),
            format!("core::{}", operation.name())
        );
        let checked =
            lang_hir::program::analyze_program(program, &sources, Default::default()).unwrap();
        if operation == Operation::Panic {
            assert!(matches!(
                checked.program,
                Some(lang_hir::program::CheckedProgram::Ownership(_))
            ));
            assert!(checked.diagnostics.is_empty());
            continue;
        }
        assert!(checked.program.is_none());
        assert!(
            checked
                .diagnostics
                .iter()
                .any(|d| d.severity == lang_diagnostics::Severity::Error)
        );
        for d in checked.diagnostics {
            d.validate(&sources).unwrap();
        }
    }
}

#[test]
fn core_call_paths_and_arities_are_checked_without_user_generic_fallback() {
    for call in [
        "core::vec_len(x)",
        "core::vec_len::<i64, bool>(x)",
        "core::map_len::<i64>(x)",
        "core::text_len::<i64>(x)",
        "core::vec_clear::<i64>(x)",
        "core::vec_len::nested::<i64>(x)",
        "f::<i64>(x)",
    ] {
        rejects(
            &format!("fn f(x: i64) -> i64 {{ return {call}; }}"),
            codes::UNSUPPORTED_SPECIALIZATION,
        );
    }
    for call in [
        "core::allocator(x)",
        "core::text_len()",
        "core::vec_push::<i64>(x)",
        "core::map_insert::<i64, bool>(x, x)",
    ] {
        rejects(
            &format!("fn f(x: i64) -> i64 {{ return {call}; }}"),
            codes::TYPE_MISMATCH,
        );
    }
    Files::new("fn f(x: i64) -> i64 { return root::core::vec_len::<i64>(x); }").program();
}

#[test]
fn core_argument_instantiation_keeps_nominal_identity_and_compiler_limits() {
    let (sources, program) = Files::new("struct Item { n: i64 } fn f(x: i64) -> i64 { return core::vec_len::<core::Option<Item>>(x); }").program();
    let f = &program.functions[0];
    let call = f
        .expressions
        .iter()
        .find_map(|e| {
            if let r::ExpressionKind::CoreCall { type_arguments, .. } = &e.kind {
                Some(type_arguments)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(sources.slice(call[0].span).unwrap(), "core::Option<Item>");
    let r::Type::Constructed(id) = call[0].value_type else {
        panic!()
    };
    assert_eq!(
        program.types[id.0].constructor,
        r::TypeConstructor::Option(r::Type::Nominal(r::DefinitionId(0)))
    );
    for (text, limits) in [
        (
            "fn f(x: i64) -> i64 { return core::vec_push::<i64>(x, x); }",
            BindingLimits {
                expressions: 2,
                ..Default::default()
            },
        ),
        (
            "fn f(x: i64) -> i64 { return core::vec_len::<core::Vec<core::Text>>(x); }",
            BindingLimits {
                types: 1,
                ..Default::default()
            },
        ),
    ] {
        assert!(matches!(
            Files::new(text).bind(limits).1,
            Err(BindingError::Limit(_))
        ));
    }
}

#[test]
fn every_core_case_binds_in_values_and_patterns_without_synthetic_nominal_ids() {
    use lang_hir::core::Case;
    for case in Case::ALL {
        let path = format!("core::{}::{}", case.family().name(), case.name());
        let explicit = !case.payload().is_empty();
        let expression = format!("{path}{}", if explicit { "(x)" } else { "" });
        let (sources, program) =
            Files::new(&format!("fn f(x: i64) -> i64 {{ return {expression}; }}")).program();
        assert!(program.definitions.is_empty() && program.types.is_empty());
        let f = &program.functions[0];
        let r::StatementKind::Return(result) = f.blocks[f.body.0].statements[0].kind else {
            panic!()
        };
        let r::ExpressionKind::CoreCase {
            case: bound,
            ref payload,
            explicit_payload,
            case_span,
        } = f.expressions[result.0].kind
        else {
            panic!()
        };
        assert_eq!(bound, case);
        assert_eq!(explicit_payload, explicit);
        assert_eq!(payload.len(), usize::from(explicit));
        assert_eq!(sources.slice(case_span).unwrap(), path);
        if let Some(value) = payload.first() {
            assert!(matches!(
                f.expressions[value.0].kind,
                r::ExpressionKind::Local(r::LocalId(0))
            ));
        }
        let checked =
            lang_hir::program::check_program(program, &sources, Default::default()).unwrap();
        assert!(checked.program.is_none());
        assert!(
            checked
                .diagnostics
                .iter()
                .any(|d| d.severity == lang_diagnostics::Severity::Error)
        );
        for d in checked.diagnostics {
            d.validate(&sources).unwrap();
        }
        let pattern = format!("{path}{}", if explicit { "(value)" } else { "" });
        let (sources, program) = Files::new(&format!("fn f(x: i64) -> i64 {{ match x {{ {pattern} => {{ return 0; }}, _ => {{ return 1; }}, }} }}")).program();
        let f = &program.functions[0];
        let core = f
            .patterns
            .iter()
            .find(|p| matches!(p.kind, r::PatternKind::CoreCase { .. }))
            .unwrap();
        let r::PatternKind::CoreCase {
            case: bound,
            ref payload,
            explicit_payload,
            case_span,
        } = core.kind
        else {
            panic!()
        };
        assert_eq!(bound, case);
        assert_eq!(explicit_payload, explicit);
        assert_eq!(payload.len(), usize::from(explicit));
        assert_eq!(sources.slice(case_span).unwrap(), path);
        let checked =
            lang_hir::program::check_program(program, &sources, Default::default()).unwrap();
        assert!(checked.program.is_none());
        assert!(
            checked
                .diagnostics
                .iter()
                .any(|d| d.severity == lang_diagnostics::Severity::Error)
        );
        for d in checked.diagnostics {
            d.validate(&sources).unwrap();
        }
    }
}

#[test]
fn unit_is_an_empty_core_record_in_values_and_patterns() {
    let (sources, program) = Files::new("fn f(x: core::Unit) -> core::Unit { match x { root::core::Unit {} => { return core::Unit {}; }, } }").program();
    let f = &program.functions[0];
    assert!(
        f.expressions
            .iter()
            .any(|e| matches!(e.kind, r::ExpressionKind::CoreUnit)
                && sources.slice(e.span).unwrap() == "core::Unit {}")
    );
    assert!(
        f.patterns
            .iter()
            .any(|p| matches!(p.kind, r::PatternKind::CoreUnit)
                && sources.slice(p.span).unwrap() == "root::core::Unit {}")
    );
    assert!(program.definitions.is_empty());
    for text in [
        "fn f() -> i64 { return core::Unit {}; }",
        "fn f(x: i64) -> i64 { match x { core::Unit {} => { return 0; }, } }",
    ] {
        let (sources, program) = Files::new(text).program();
        assert!(program.types.is_empty());
        let checked =
            lang_hir::program::check_program(program, &sources, Default::default()).unwrap();
        assert!(checked.program.is_none());
        assert!(
            checked
                .diagnostics
                .iter()
                .any(|d| d.severity == lang_diagnostics::Severity::Error)
        );
        for d in checked.diagnostics {
            d.validate(&sources).unwrap();
        }
    }
}

#[test]
fn core_values_and_patterns_enforce_exact_case_and_unit_shape() {
    for expression in [
        "core::Option::None()",
        "core::Option::Some",
        "core::Option::Some()",
        "core::Result::Ok(1, 2)",
        "core::Unit",
        "core::Unit()",
        "core::Unit { n: 1 }",
        "core::Option::Some {}",
    ] {
        rejects(
            &format!("fn f() -> i64 {{ return {expression}; }}"),
            codes::INVALID_SHAPE,
        );
    }
    for pattern in [
        "core::Option::Some",
        "core::Result::Err(x, y)",
        "core::Unit { n: value }",
        "core::Unit",
        "core::text_len(x)",
    ] {
        rejects(
            &format!("fn f(x: i64) -> i64 {{ match x {{ {pattern} => {{ return 0; }}, }} }}"),
            codes::INVALID_SHAPE,
        );
    }
    // Empty payload pattern lists are rejected by the accepted parser before
    // binding; expression-call lists remain valid syntax and fail shape above.
    for pattern in ["core::Option::None()", "core::Option::Some()"] {
        let files = Files::new(&format!(
            "fn f(x: i64) -> i64 {{ match x {{ {pattern} => {{ return 0; }}, }} }}"
        ));
        let mut sources = SourceMap::new();
        let parsed = load_modules(
            &mut sources,
            &files.0.join("main.lang"),
            &files.0,
            Default::default(),
        )
        .unwrap();
        assert!(parsed.graph.is_none());
        assert!(
            parsed
                .diagnostics
                .iter()
                .any(|d| d.code == codes::EXPECTED_SYNTAX)
        );
    }
    rejects(
        "fn f() -> i64 { return core::Option::Some::<i64>(1); }",
        codes::UNSUPPORTED_SPECIALIZATION,
    );
    rejects(
        "fn f() -> i64 { return core::Option::Ok; }",
        codes::UNSUPPORTED_SPECIALIZATION,
    );
    rejects(
        "fn f() -> i64 { return core::BoundsError::Bounds; }",
        codes::UNSUPPORTED_SPECIALIZATION,
    );
}

#[test]
fn core_cases_retain_nested_payloads_and_pattern_scope() {
    let (sources, program) = Files::new("fn f(x: core::Result<core::Option<i64>, core::BufferError>) -> i64 { match x { core::Result::Ok(core::Option::Some(value)) => { return value; }, core::Result::Err(core::BufferError::Allocation(core::AllocError::Limit)) => { return 1; }, _ => { return 0; }, } }").program();
    let f = &program.functions[0];
    assert_eq!(
        f.patterns
            .iter()
            .filter(|p| matches!(p.kind, r::PatternKind::CoreCase { .. }))
            .count(),
        5
    );
    let value = f
        .locals
        .iter()
        .position(|l| l.name.text == "value")
        .unwrap();
    assert!(
        f.expressions
            .iter()
            .any(|e| matches!(e.kind, r::ExpressionKind::Local(r::LocalId(id)) if id == value))
    );
    for p in &f.patterns {
        sources.slice(p.span).unwrap();
    }
    rejects(
        "fn f(x: i64) -> i64 { match x { core::Option::Some(value) => { return 0; }, } return value; }",
        codes::UNKNOWN_NAME,
    );
    Files::new("fn f() -> core::Result<i64, core::BufferError> { return core::Result::Err(core::BufferError::Allocation(core::AllocError::Limit)); }").program();
}

#[test]
fn user_and_core_cases_have_distinct_identity_and_source_budgets() {
    let (_, program) = Files::new("enum Option { Some(i64), None } fn f(x: i64) -> i64 { let mut user: Option = Option::Some(x); let mut builtin: core::Option<i64> = core::Option::Some(x); return 0; }").program();
    let f = &program.functions[0];
    assert!(f.expressions.iter().any(|e| matches!(
        e.kind,
        r::ExpressionKind::Call {
            callee: r::Callable::Case { .. },
            ..
        }
    )));
    assert!(
        f.expressions
            .iter()
            .any(|e| matches!(e.kind, r::ExpressionKind::CoreCase { .. }))
    );
    for (text, limits) in [
        (
            "fn f() -> i64 { return core::Option::Some(1); }",
            BindingLimits {
                expressions: 1,
                ..Default::default()
            },
        ),
        (
            "fn f(x: i64) -> i64 { match x { core::Option::Some(value) => { return 0; }, } }",
            BindingLimits {
                patterns: 1,
                ..Default::default()
            },
        ),
        (
            "fn f(x: i64) -> i64 { return core::Result::Ok(core::Option::Some(x)); }",
            BindingLimits {
                depth: 4,
                ..Default::default()
            },
        ),
    ] {
        assert!(matches!(
            Files::new(text).bind(limits).1,
            Err(BindingError::Limit(_))
        ));
    }
}

#[test]
fn realistic_core_source_binds_through_calls_cases_and_read_places() {
    let (sources, program) = Files::new(include_str!("fixtures/core-binding.lang")).program();
    let f = &program.functions[0];
    assert_eq!(
        f.expressions
            .iter()
            .filter(|e| matches!(e.kind, r::ExpressionKind::CoreCall { .. }))
            .count(),
        3
    );
    assert_eq!(
        f.patterns
            .iter()
            .filter(|p| matches!(p.kind, r::PatternKind::CoreCase { .. }))
            .count(),
        4
    );
    assert!(
        f.expressions
            .iter()
            .any(|e| matches!(e.kind, r::ExpressionKind::Dereference(_)))
    );
    let types = lang_hir::types::analyze(&program, &sources, Default::default()).unwrap();
    assert!(types.facts.is_some(), "{:?}", types.diagnostics);
    assert!(matches!(
        lang_hir::program::check_program(program, &sources, Default::default()),
        Err(lang_hir::program::CheckError::OwnershipProfile { .. })
    ));
}

fn body_analysis(text: &str) -> (SourceMap, lang_hir::program::BodyAnalysis) {
    let (sources, program) = Files::new(text).program();
    let analysis =
        lang_hir::program::analyze_bodies(program, &sources, Default::default()).unwrap();
    for diagnostic in &analysis.diagnostics {
        diagnostic.validate(&sources).unwrap();
    }
    (sources, analysis)
}

#[test]
fn bidirectional_core_context_reaches_returns_locals_arguments_and_record_fields() {
    let (_, analysis) = body_analysis(
        "struct Box { value: core::Option<i64> } fn consume(value: core::Result<core::Option<i64>, core::AllocError>) -> i64 { return 0; } fn f() -> core::Result<Box, core::AllocError> { let mut local: core::Option<i64> = core::Option::None; local = core::Option::Some(1); let mut record: Box = Box { value: core::Option::Some(2) }; let mut call: i64 = consume(core::Result::Ok(core::Option::Some(3))); return core::Result::Ok(Box { value: core::Option::None }); }",
    );
    assert!(analysis.bodies.is_some(), "{:?}", analysis.diagnostics);
    let bodies = analysis.bodies.unwrap();
    assert!(
        bodies.functions()[1]
            .locals()
            .iter()
            .all(|ty| bodies.properties(*ty).unwrap().is_copy)
    );
    let (_, unit) = body_analysis(
        "fn f(x: core::Unit) -> core::Unit { match x { core::Unit {} => { return core::Unit {}; }, } }",
    );
    assert!(unit.bodies.is_some(), "{:?}", unit.diagnostics);
}

#[test]
fn real_core_body_types_keep_provisional_read_loans_out_of_resolved_source() {
    let (_, analysis) = body_analysis(include_str!("fixtures/core-binding.lang"));
    assert!(analysis.bodies.is_some(), "{:?}", analysis.diagnostics);
    let bodies = analysis.bodies.unwrap();
    assert!(bodies.types().len() > bodies.source().types.len());
    let function = &bodies.source().functions[0];
    for name in ["values", "read"] {
        let index = function
            .locals
            .iter()
            .position(|local| local.name.text == name)
            .unwrap();
        let r::Type::Constructed(id) = bodies.functions()[0].locals()[index] else {
            panic!()
        };
        assert!(
            matches!(
                bodies.types()[id.0].constructor,
                r::TypeConstructor::Loan(r::LoanKind::Shared, _)
            ),
            "{name}"
        );
    }
    for (index, expression) in function.expressions.iter().enumerate() {
        if let r::ExpressionKind::CoreCall { operation, .. } = expression.kind {
            assert_eq!(
                bodies.functions()[0]
                    .call(r::ExpressionId(index))
                    .unwrap()
                    .result_binding,
                operation.signature().result_binding
            );
        }
    }
    let sources = Files::new(include_str!("fixtures/core-binding.lang"));
    let (snapshots, program) = sources.program();
    assert!(matches!(
        lang_hir::program::check_program(program, &snapshots, Default::default()),
        Err(lang_hir::program::CheckError::OwnershipProfile { .. })
    ));
}

#[test]
fn type_only_analysis_handles_explicit_and_inferred_loans_and_place_types() {
    for text in [
        "fn length(x: &core::Text) -> i64 { return core::text_len(x); } fn f(text: core::Text) -> i64 { let mut read: &core::Text = &text; return length(read); }",
        "fn length(x: &core::Text) -> i64 { return core::text_len(x); } fn f(text: core::Text) -> i64 { return length(text); }",
        "fn f(bytes: &mut core::Bytes) -> core::Result<core::Unit, core::BufferError> { return core::bytes_set(bytes, 0, 255); }",
        "fn f(values: &mut core::Vec<i64>) -> i64 { match core::vec_get_mut::<i64>(values, 0) { core::Result::Ok(read) => { (*read) = 1; return (*read); }, core::Result::Err(_) => { return 0; }, } }",
        "struct R { value: i64 } fn f(value: &mut R) -> i64 { value.value = 2; return value.value; }",
    ] {
        let (_, analysis) = body_analysis(text);
        assert!(
            analysis.bodies.is_some(),
            "{text}: {:?}",
            analysis.diagnostics
        );
    }
}

#[test]
fn core_body_type_errors_produce_no_type_facts() {
    for (text, code) in [
        (
            "fn f() -> core::Option<i64> { return core::Option::Some(true); }",
            codes::TYPE_MISMATCH,
        ),
        (
            "fn f() -> core::Result<i64, core::AllocError> { return core::Option::Some(1); }",
            codes::TYPE_MISMATCH,
        ),
        (
            "fn f() -> i64 { match core::Option::Some(1) { _ => { return 0; }, } }",
            codes::TYPE_MISMATCH,
        ),
        (
            "fn f(x: i64) -> i64 { return core::text_len(x); }",
            codes::TYPE_MISMATCH,
        ),
        (
            "fn f(x: i64) -> i64 { return core::map_len::<bool, i64>(x); }",
            codes::UNSUPPORTED_SPECIALIZATION,
        ),
        (
            "fn f(x: i64) -> i64 { return core::vec_len::<&i64>(x); }",
            codes::INVALID_OWNERSHIP_TYPE,
        ),
        (
            "fn f(x: i64) -> i64 { return core::vec_len::<core::Allocator>(x); }",
            codes::INVALID_OWNERSHIP_TYPE,
        ),
        (
            "struct N { token: arena i64 } fn f(a: &core::Allocator, n: N) -> i64 { let mut result: core::Result<shared i64, core::OwnershipError> = core::shared_new::<N>(a, n); return 0; }",
            codes::REGION_ESCAPE,
        ),
        ("fn f() -> i64 { return (*1); }", codes::INVALID_LOAN),
    ] {
        let (_, analysis) = body_analysis(text);
        assert!(analysis.bodies.is_none(), "{text}");
        assert!(
            analysis.diagnostics.iter().any(|d| d.code == code),
            "{text}: {:?}",
            analysis.diagnostics
        );
    }
}

#[test]
fn core_body_exhaustiveness_and_usefulness_share_the_existing_search() {
    for (text, missing) in [
        (
            "fn f(x: core::Option<i64>) -> i64 { match x { core::Option::Some(_) => { return 1; }, } }",
            "core::Option::None",
        ),
        (
            "fn f(x: core::Result<core::Unit, core::BoundsError>) -> i64 { match x { core::Result::Ok(core::Unit {}) => { return 1; }, } }",
            "core::Result::Err(core::BoundsError::OutOfBounds)",
        ),
        (
            "fn f(x: core::BufferError) -> i64 { match x { core::BufferError::Bounds => { return 1; }, core::BufferError::InvalidByte => { return 2; }, } }",
            "core::BufferError::Allocation(core::AllocError::Limit)",
        ),
    ] {
        let (_, analysis) = body_analysis(text);
        assert!(analysis.bodies.is_none());
        let diagnostic = analysis
            .diagnostics
            .iter()
            .find(|d| d.code == codes::NON_EXHAUSTIVE_MATCH)
            .unwrap();
        assert!(
            diagnostic.notes[0].contains(missing),
            "{:?}",
            diagnostic.notes
        );
    }
    let (_, analysis) = body_analysis(
        "fn f(x: core::Option<i64>) -> i64 { match x { _ => { return 1; }, core::Option::None => { return 0; }, } }",
    );
    assert!(analysis.bodies.is_some());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::UNREACHABLE_ARM)
    );
}

fn signature_source(pattern: lang_hir::core::TypePattern) -> String {
    use lang_hir::core::TypePattern as P;
    match pattern {
        P::I64 | P::Argument(0) => "i64".into(),
        P::Bool | P::Argument(1) => "bool".into(),
        P::Argument(_) => panic!(),
        P::Builtin(b) => format!("core::{}", b.name()),
        P::Option(t) => format!("core::Option<{}>", signature_source(*t)),
        P::Result(t, e) => format!(
            "core::Result<{}, {}>",
            signature_source(*t),
            signature_source(*e)
        ),
        P::Vec(t) => format!("core::Vec<{}>", signature_source(*t)),
        P::Map(k, v) => format!(
            "core::Map<{}, {}>",
            signature_source(*k),
            signature_source(*v)
        ),
        P::Shared(t) => format!("shared {}", signature_source(*t)),
        P::Weak(t) => format!("weak {}", signature_source(*t)),
        P::Arena(t) => format!("arena {}", signature_source(*t)),
        P::Loan(r::LoanKind::Shared, t) => format!("&{}", signature_source(*t)),
        P::Loan(r::LoanKind::Exclusive, t) => format!("&mut {}", signature_source(*t)),
        P::ArenaOwner => panic!("arena-owner body typing remains a separate scope"),
    }
}

#[test]
fn body_typing_instantiates_all_non_arena_core_signatures_from_real_source() {
    use lang_hir::core::{Operation, ResultBinding};
    for operation in Operation::ALL {
        if matches!(
            operation,
            Operation::ArenaNew | Operation::ArenaRead | Operation::ArenaReplace
        ) {
            continue;
        }
        let sig = operation.signature();
        let parameters = sig
            .parameters
            .iter()
            .enumerate()
            .map(|(i, p)| format!("p{i}: {}", signature_source(*p)))
            .collect::<Vec<_>>()
            .join(", ");
        let arguments = (0..sig.parameters.len())
            .map(|i| format!("p{i}"))
            .collect::<Vec<_>>()
            .join(", ");
        let generics = match sig.type_arguments.len() {
            0 => "",
            1 => "::<i64>",
            2 => "::<i64, bool>",
            _ => panic!(),
        };
        let call = format!("core::{}{generics}({arguments})", operation.name());
        let (result, body) = match sig.result_binding {
            ResultBinding::InstanceAllocator => (
                "i64".into(),
                format!("let mut authority: &core::Allocator = {call}; return 0;"),
            ),
            ResultBinding::ProvisionalLoan { .. } => (
                "i64".into(),
                format!("match {call} {{ _ => {{ return 0; }}, }}"),
            ),
            _ => (signature_source(sig.result), format!("return {call};")),
        };
        let text = format!("fn f({parameters}) -> {result} {{ {body} }}");
        let (_, analysis) = body_analysis(&text);
        assert!(
            analysis.bodies.is_some(),
            "{operation:?}: {:?}",
            analysis.diagnostics
        );
    }
    let (sources, program) = Files::new(
        "fn f(allocator: &core::Allocator) -> i64 { arena graph(allocator) {} return 0; }",
    )
    .program();
    assert!(
        lang_hir::program::analyze_bodies(program, &sources, Default::default())
            .unwrap()
            .bodies
            .is_some()
    );
}

#[test]
fn body_typing_checks_every_core_case_payload_with_a_concrete_expected_family() {
    use lang_hir::core::{Case, Family, TypePattern as P};
    for case in Case::ALL {
        let result = match case.family() {
            Family::Option => "core::Option<i64>".into(),
            Family::Result => "core::Result<i64, bool>".into(),
            family => format!("core::{}", family.name()),
        };
        let mut expression = format!("core::{}::{}", case.family().name(), case.name());
        if let Some(payload) = case.payload().first() {
            let value = match payload {
                P::Argument(0) | P::I64 => "1",
                P::Argument(1) => "true",
                P::Builtin(r::BuiltinType::AllocError) => "core::AllocError::Limit",
                _ => panic!(),
            };
            expression.push_str(&format!("({value})"));
        }
        let (_, analysis) =
            body_analysis(&format!("fn f() -> {result} {{ return {expression}; }}"));
        assert!(
            analysis.bodies.is_some(),
            "{case:?}: {:?}",
            analysis.diagnostics
        );
    }
    let (_, analysis) = body_analysis(
        "fn f() -> i64 { match core::AllocError::Limit { core::AllocError::Limit => { return 1; }, core::AllocError::OutOfMemory => { return 2; }, core::AllocError::Overflow => { return 3; }, } }",
    );
    assert!(analysis.bodies.is_some(), "{:?}", analysis.diagnostics);
}

#[test]
fn body_typing_retains_copy_checker_results_and_original_resolved_source() {
    let (sources, program) = Files::new("struct R { x: i64, b: bool } enum E { None, Some(R) } fn value(x: i64) -> E { return E::Some(R { b: true, x: x }); } fn f(x: i64) -> i64 { match value(x) { E::None => { return 0; }, E::Some(R { x: result, b: _ }) => { return result; }, } }").program();
    let copy =
        lang_hir::program::check_program(program.clone(), &sources, Default::default()).unwrap();
    let body =
        lang_hir::program::analyze_bodies(program.clone(), &sources, Default::default()).unwrap();
    assert_eq!(copy.diagnostics, body.diagnostics);
    let body = body.bodies.unwrap();
    assert_eq!(copy.program.unwrap().functions(), body.functions());
    assert_eq!(body.source(), &program);
    let again =
        lang_hir::program::analyze_bodies(body.source().clone(), &sources, Default::default())
            .unwrap();
    assert_eq!(again.bodies.unwrap(), body);
}

#[test]
fn type_only_facts_do_not_authorize_move_validity_or_executable_lowering() {
    let text = "fn f(text: core::Text) -> i64 { let mut moved: core::Text = text; return core::text_len(text); }";
    let (sources, program) = Files::new(text).program();
    let types =
        lang_hir::program::analyze_bodies(program.clone(), &sources, Default::default()).unwrap();
    assert!(types.bodies.is_some(), "{:?}", types.diagnostics);
    let checked = lang_hir::program::check_program(program, &sources, Default::default()).unwrap();
    assert!(checked.program.is_none());
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|d| d.code == codes::INVALID_OWNER_USE)
    );
}

#[test]
fn body_type_instantiation_and_coverage_limits_fail_without_partial_facts() {
    use lang_hir::program::{CheckError, CheckLimits, analyze_bodies};
    let (sources, program) = Files::new(include_str!("fixtures/core-binding.lang")).program();
    let baseline = analyze_bodies(program.clone(), &sources, Default::default())
        .unwrap()
        .bodies
        .unwrap();
    let mut exact = CheckLimits::default();
    exact.types.work = baseline.type_work_used();
    exact.types.allocations = baseline.requested_type_bytes();
    assert_eq!(
        analyze_bodies(program.clone(), &sources, exact)
            .unwrap()
            .bodies
            .unwrap(),
        baseline
    );
    for limits in [
        CheckLimits {
            types: lang_hir::types::Limits {
                work: baseline.type_work_used() - 1,
                ..Default::default()
            },
            ..Default::default()
        },
        CheckLimits {
            types: lang_hir::types::Limits {
                allocations: baseline.requested_type_bytes() - 1,
                ..Default::default()
            },
            ..Default::default()
        },
        CheckLimits {
            types: lang_hir::types::Limits {
                constructed: program.types.len(),
                ..Default::default()
            },
            ..Default::default()
        },
        CheckLimits {
            work: 0,
            ..Default::default()
        },
        CheckLimits {
            exhaustiveness: 0,
            ..Default::default()
        },
    ] {
        let error = analyze_bodies(program.clone(), &sources, limits).unwrap_err();
        let (CheckError::Limit(d) | CheckError::Types(lang_hir::types::Error::Limit(d))) = error
        else {
            panic!("{error:?}")
        };
        d.validate(&sources).unwrap();
    }
}

#[test]
fn malformed_foreign_body_type_arguments_and_places_are_rejected_structurally() {
    use lang_hir::core::Operation;
    use lang_hir::program::{CheckError, CheckInputErrorKind as K, analyze_bodies};
    let (sources, mut program) = Files::new(include_str!("fixtures/core-binding.lang")).program();
    let source_len = program.types.len();
    // This index can already name an inferred loan by this call site; it is
    // still outside the original source arena and cannot be a source argument.
    for expression in &mut program.functions[0].expressions {
        if let r::ExpressionKind::CoreCall {
            operation: Operation::VecGet,
            type_arguments,
            ..
        } = &mut expression.kind
        {
            type_arguments[0].value_type = r::Type::Constructed(r::ConstructedTypeId(source_len));
        }
    }
    assert!(
        matches!(analyze_bodies(program, &sources, Default::default()), Err(CheckError::Input { kind: K::InvalidConstructed(id), .. }) if id == source_len)
    );
    let (sources, mut program) =
        Files::new("fn f(x: &i64) -> i64 { let mut read: &i64 = &(*x); return 0; }").program();
    let literal = program.functions[0]
        .expressions
        .iter()
        .position(|e| matches!(e.kind, r::ExpressionKind::Integer(0)))
        .unwrap();
    for expression in &mut program.functions[0].expressions {
        if let r::ExpressionKind::Borrow { place, .. } = &mut expression.kind {
            *place = r::PlaceId(literal);
        }
    }
    assert!(matches!(
        analyze_bodies(program, &sources, Default::default()),
        Err(CheckError::Input {
            kind: K::InvalidPlace,
            ..
        })
    ));
}

#[test]
fn instantiated_type_diagnostics_name_arguments_and_obey_output_bounds() {
    let text = "fn f(value: core::Vec<bool>) -> i64 { return core::vec_len::<i64>(value); }";
    let (sources, analysis) = body_analysis(text);
    let diagnostic = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == codes::TYPE_MISMATCH)
        .unwrap();
    assert!(
        diagnostic.message.contains("&core::Vec<i64>"),
        "{}",
        diagnostic.message
    );
    assert!(
        diagnostic.message.contains("core::Vec<bool>"),
        "{}",
        diagnostic.message
    );
    diagnostic.validate(&sources).unwrap();
    let (sources, program) = Files::new(text).program();
    let limits = lang_hir::program::CheckLimits {
        type_display_bytes: 4,
        ..Default::default()
    };
    assert!(matches!(
        lang_hir::program::analyze_bodies(program, &sources, limits),
        Err(lang_hir::program::CheckError::Limit(_))
    ));
}

#[test]
fn all_arena_operations_instantiate_the_same_lexical_owner_and_return_types() {
    use lang_hir::core::{Operation, ResultBinding};
    let (_, analysis) = body_analysis(include_str!("fixtures/arena-body.lang"));
    assert!(analysis.bodies.is_some(), "{:?}", analysis.diagnostics);
    let bodies = analysis.bodies.unwrap();
    let function = &bodies.source().functions[0];
    let owner = function
        .locals
        .iter()
        .position(|l| l.kind == r::BindingKind::ArenaOwner)
        .unwrap();
    let owner_type = bodies.functions()[0].locals()[owner];
    let r::Type::Constructed(owner_id) = owner_type else {
        panic!()
    };
    assert_eq!(
        bodies.types()[owner_id.0].constructor,
        r::TypeConstructor::ArenaOwner(r::ArenaOwnerId {
            function: r::FunctionId(0),
            local: r::LocalId(owner)
        })
    );
    let properties = bodies.properties(owner_type).unwrap();
    assert!(!properties.is_copy && properties.requires_drop && properties.contains_arena);
    let mut seen = std::collections::BTreeSet::new();
    for (id, expression) in function.expressions.iter().enumerate() {
        if let r::ExpressionKind::CoreCall { operation, .. } = expression.kind {
            seen.insert(operation);
            let facts = bodies.functions()[0].call(r::ExpressionId(id)).unwrap();
            let r::Type::Constructed(parameter) = facts.parameters[0] else {
                panic!()
            };
            let kind = if operation == Operation::ArenaRead {
                r::LoanKind::Shared
            } else {
                r::LoanKind::Exclusive
            };
            assert_eq!(
                bodies.types()[parameter.0].constructor,
                r::TypeConstructor::Loan(kind, owner_type)
            );
            assert_eq!(
                facts.result_binding,
                match operation {
                    Operation::ArenaNew => ResultBinding::ArenaReference { owner: 0 },
                    Operation::ArenaRead => ResultBinding::ProvisionalLoan { parameter: 0 },
                    _ => ResultBinding::Value,
                }
            );
        }
    }
    assert_eq!(
        seen,
        [
            Operation::ArenaNew,
            Operation::ArenaRead,
            Operation::ArenaReplace
        ]
        .into_iter()
        .collect()
    );
    assert!(
        bodies
            .source()
            .types
            .iter()
            .all(|t| !matches!(t.constructor, r::TypeConstructor::ArenaOwner(_)))
    );
}

#[test]
fn lexical_owner_identity_is_distinct_across_nested_regions_and_functions() {
    let (_, analysis) = body_analysis(
        "fn f(a: &core::Allocator) -> i64 { arena graph(a) { arena nested(a) {} } return 0; } fn g(a: &core::Allocator) -> i64 { arena graph(a) {} return 0; }",
    );
    let bodies = analysis.bodies.unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for (function_id, function) in bodies.source().functions.iter().enumerate() {
        for (local_id, local) in function.locals.iter().enumerate() {
            if local.kind == r::BindingKind::ArenaOwner {
                let ty = bodies.functions()[function_id].locals()[local_id];
                assert!(seen.insert(ty));
                let r::Type::Constructed(id) = ty else {
                    panic!()
                };
                assert_eq!(
                    bodies.types()[id.0].constructor,
                    r::TypeConstructor::ArenaOwner(r::ArenaOwnerId {
                        function: r::FunctionId(function_id),
                        local: r::LocalId(local_id)
                    })
                );
            }
        }
    }
    assert_eq!(seen.len(), 3);
}

#[test]
fn arena_calls_accept_explicit_loans_and_owner_aliases_with_preserved_identity() {
    let text = "fn f(a: &core::Allocator) -> i64 { arena graph(a) { let mut created: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(&mut graph, 1); match created { core::Result::Ok(node) => { match graph { alias => { match core::arena_read::<i64>(alias, node) { core::Result::Ok(read) => { return (*read); }, core::Result::Err(_) => { return 0; }, } }, } }, core::Result::Err(_) => { return 0; }, } } }";
    let (_, analysis) = body_analysis(text);
    assert!(analysis.bodies.is_some(), "{:?}", analysis.diagnostics);
    let bodies = analysis.bodies.unwrap();
    let function = &bodies.source().functions[0];
    let graph = function
        .locals
        .iter()
        .position(|l| l.name.text == "graph")
        .unwrap();
    let alias = function
        .locals
        .iter()
        .position(|l| l.name.text == "alias")
        .unwrap();
    let r::Type::Constructed(id) = bodies.functions()[0].locals()[alias] else {
        panic!()
    };
    assert_eq!(
        bodies.types()[id.0].constructor,
        r::TypeConstructor::Loan(r::LoanKind::Shared, bodies.functions()[0].locals()[graph])
    );
}

#[test]
fn arena_owner_allocator_and_loan_kind_mismatches_are_source_diagnostics() {
    for (text, code) in [
        (
            "fn f(a: i64) -> i64 { arena graph(a) {} return 0; }",
            codes::TYPE_MISMATCH,
        ),
        (
            "fn f(a: &core::Allocator) -> i64 { let mut n: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(a, 1); return 0; }",
            codes::INVALID_OWNERSHIP_TYPE,
        ),
        (
            "fn f(a: &core::Allocator, n: arena i64) -> i64 { let mut r: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<i64>(n, n, 1); return 0; }",
            codes::INVALID_OWNERSHIP_TYPE,
        ),
        (
            "fn f(a: &core::Allocator) -> i64 { arena graph(a) { let mut n: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(&graph, 1); } return 0; }",
            codes::INVALID_LOAN,
        ),
    ] {
        let (_, analysis) = body_analysis(text);
        assert!(analysis.bodies.is_none(), "{text}");
        assert!(
            analysis.diagnostics.iter().any(|d| d.code == code),
            "{text}: {:?}",
            analysis.diagnostics
        );
    }
}

#[test]
fn compiler_only_owner_types_cannot_enter_resolved_input_or_generic_arguments() {
    use lang_hir::types::{Error, InputError};
    let (sources, mut program) = Files::new("fn f() -> i64 { return 0; }").program();
    program.types.push(r::ConstructedType {
        constructor: r::TypeConstructor::ArenaOwner(r::ArenaOwnerId {
            function: r::FunctionId(99),
            local: r::LocalId(99),
        }),
        span: program.functions[0].span,
    });
    assert!(matches!(
        lang_hir::types::analyze(&program, &sources, Default::default()),
        Err(Error::Input {
            kind: InputError::CompilerOnlyType(0),
            ..
        })
    ));
    assert!(matches!(
        lang_hir::program::analyze_bodies(program, &sources, Default::default()),
        Err(lang_hir::program::CheckError::Types(Error::Input {
            kind: InputError::CompilerOnlyType(0),
            ..
        }))
    ));
    rejects(
        "fn f(a: &core::Allocator) -> i64 { arena graph(a) { let mut n: core::Result<arena i64, core::ArenaError> = core::arena_new::<core::ArenaOwner>(graph, graph); } return 0; }",
        codes::UNSUPPORTED_SPECIALIZATION,
    );
}

#[test]
fn arena_body_types_preserve_the_type_only_and_resource_boundaries() {
    use lang_hir::program::{CheckError, CheckLimits, analyze_bodies};
    let (sources, program) = Files::new(include_str!("fixtures/arena-body.lang")).program();
    let baseline = analyze_bodies(program.clone(), &sources, Default::default())
        .unwrap()
        .bodies
        .unwrap();
    let mut exact = CheckLimits::default();
    exact.types.work = baseline.type_work_used();
    exact.types.allocations = baseline.requested_type_bytes();
    assert_eq!(
        analyze_bodies(program.clone(), &sources, exact)
            .unwrap()
            .bodies
            .unwrap(),
        baseline
    );
    exact.types.work -= 1;
    assert!(matches!(
        analyze_bodies(program.clone(), &sources, exact),
        Err(CheckError::Types(lang_hir::types::Error::Limit(_)))
    ));
    assert!(matches!(
        lang_hir::program::check_program(program, &sources, Default::default()),
        Err(CheckError::OwnershipProfile { .. })
    ));
}

#[test]
fn malformed_owner_bindings_cannot_become_parameters_or_bypass_local_metadata() {
    use lang_hir::program::{CheckError, CheckInputErrorKind as K, analyze_bodies};
    let (sources, program) =
        Files::new("fn f(a: &core::Allocator) -> i64 { arena graph(a) {} return 0; }").program();
    let owner = program.functions[0]
        .locals
        .iter()
        .position(|l| l.kind == r::BindingKind::ArenaOwner)
        .unwrap();
    let mut bad = program.clone();
    bad.functions[0].parameters.push(r::LocalId(owner));
    assert!(matches!(
        analyze_bodies(bad, &sources, Default::default()),
        Err(CheckError::Input {
            kind: K::InvalidSignature,
            ..
        })
    ));
    let mut bad = program.clone();
    for block in &mut bad.functions[0].blocks {
        for statement in &mut block.statements {
            if let r::StatementKind::Arena { owner, .. } = &mut statement.kind {
                *owner = r::LocalId(0);
            }
        }
    }
    assert!(matches!(
        analyze_bodies(bad, &sources, Default::default()),
        Err(CheckError::Input {
            kind: K::InvalidLocalMetadata,
            ..
        })
    ));
    let mut bad = program;
    for block in &mut bad.functions[0].blocks {
        for statement in &mut block.statements {
            if let r::StatementKind::Arena { owner, .. } = &mut statement.kind {
                *owner = r::LocalId(99);
            }
        }
    }
    assert!(matches!(
        analyze_bodies(bad, &sources, Default::default()),
        Err(CheckError::Input {
            kind: K::InvalidId { .. },
            ..
        })
    ));
}

#[test]
fn owner_type_identity_does_not_substitute_for_value_region_flow() {
    let text = "fn f(a: &core::Allocator) -> i64 { arena outer(a) { let mut created: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(outer, 1); match created { core::Result::Ok(node) => { arena inner(a) { match core::arena_read::<i64>(inner, node) { core::Result::Ok(read) => { return (*read); }, core::Result::Err(_) => { return 0; }, } } }, core::Result::Err(_) => { return 0; }, } } }";
    let (sources, program) = Files::new(text).program();
    let types =
        lang_hir::program::analyze_bodies(program.clone(), &sources, Default::default()).unwrap();
    assert!(types.bodies.is_some(), "{:?}", types.diagnostics);
    // Type identity alone does not prove value provenance. The following
    // region pass now rejects the cross-region read before execution.
    let checked = lang_hir::program::check_program(program, &sources, Default::default()).unwrap();
    assert!(checked.program.is_none());
    assert_eq!(checked.diagnostics[0].code, codes::REGION_ESCAPE);
}

#[test]
fn borrow_place_and_dereference_binding_preserves_roots_kinds_and_spans() {
    let (sources, program) = Files::new("struct Inner { n: i64 } struct Outer { child: Inner } fn f(x: &mut Outer) -> i64 { let mut read: &i64 = &x.child.n; let mut write: &mut i64 = &mut (*x).child.n; (*write) = 7; (*x).child.n = (*read); return 0; }").program();
    let f = &program.functions[0];
    let body = &f.blocks[f.body.0];
    for (statement, expected_kind, expected_span) in [
        (&body.statements[0], r::LoanKind::Shared, "&x.child.n"),
        (
            &body.statements[1],
            r::LoanKind::Exclusive,
            "&mut (*x).child.n",
        ),
    ] {
        let r::StatementKind::LetMut { initializer, .. } = statement.kind else {
            panic!()
        };
        let expression = &f.expressions[initializer.0];
        assert_eq!(sources.slice(expression.span).unwrap(), expected_span);
        let r::ExpressionKind::Borrow { kind, place } = expression.kind else {
            panic!()
        };
        assert_eq!(kind, expected_kind);
        let r::ExpressionKind::Field { receiver, ref name } = f.expressions[place.0].kind else {
            panic!()
        };
        assert_eq!(name.text, "n");
        assert!(matches!(
            f.expressions[receiver.0].kind,
            r::ExpressionKind::Field { .. }
        ));
    }
    let r::StatementKind::PlaceAssign { place, value } = body.statements[2].kind else {
        panic!()
    };
    assert_eq!(
        sources.slice(f.expressions[place.0].span).unwrap(),
        "(*write)"
    );
    assert!(matches!(
        f.expressions[place.0].kind,
        r::ExpressionKind::Dereference(_)
    ));
    assert!(matches!(
        f.expressions[value.0].kind,
        r::ExpressionKind::Integer(7)
    ));
    let r::StatementKind::PlaceAssign { place, value } = body.statements[3].kind else {
        panic!()
    };
    assert_eq!(
        sources.slice(f.expressions[place.0].span).unwrap(),
        "(*x).child.n"
    );
    assert_eq!(
        sources.slice(f.expressions[value.0].span).unwrap(),
        "(*read)"
    );
}

#[test]
fn arena_owner_binding_has_a_distinct_identity_and_lexical_scope() {
    let (sources, program) = Files::new("fn f(allocator: &core::Allocator) -> i64 { arena outer(allocator) { arena inner(outer) { let mut copy: i64 = inner; } let mut copy_outer: i64 = outer; } return 0; }").program();
    let f = &program.functions[0];
    let r::StatementKind::Arena {
        owner: outer,
        allocator,
        body,
    } = f.blocks[f.body.0].statements[0].kind
    else {
        panic!()
    };
    assert_eq!(f.locals[outer.0].kind, r::BindingKind::ArenaOwner);
    assert!(f.locals[outer.0].annotation.is_none());
    assert_eq!(sources.slice(f.locals[outer.0].name.span).unwrap(), "outer");
    assert!(matches!(
        f.expressions[allocator.0].kind,
        r::ExpressionKind::Local(r::LocalId(0))
    ));
    let r::StatementKind::Arena {
        owner: inner,
        allocator,
        body: inner_body,
    } = f.blocks[body.0].statements[0].kind
    else {
        panic!()
    };
    assert_ne!(inner, outer);
    assert!(matches!(f.expressions[allocator.0].kind, r::ExpressionKind::Local(id) if id == outer));
    let r::StatementKind::LetMut { initializer, .. } = f.blocks[inner_body.0].statements[0].kind
    else {
        panic!()
    };
    assert!(
        matches!(f.expressions[initializer.0].kind, r::ExpressionKind::Local(id) if id == inner)
    );
}

#[test]
fn temporary_values_and_module_items_are_not_places() {
    for expression in ["&1", "&(x + 1)", "&make()", "&R { n: 1 }.n", "&make", "&R"] {
        rejects(
            &format!(
                "struct R {{ n: i64 }} fn make() -> i64 {{ return 1; }} fn f(x: i64) -> i64 {{ return {expression}; }}"
            ),
            codes::INVALID_LOAN,
        );
    }
    rejects(
        "struct R { n: i64 } fn f() -> i64 { R { n: 1 }.n = 2; return 0; }",
        codes::INVALID_LOAN,
    );
    rejects("fn f() -> i64 { return &missing; }", codes::UNKNOWN_NAME);
    rejects(
        "fn f() -> i64 { return 1; let mut x: &i64 = &missing; }",
        codes::UNKNOWN_NAME,
    );
}

#[test]
fn arena_names_obey_visibility_name_reuse_and_reserved_root_rules() {
    for text in [
        "fn f(a: i64) -> i64 { arena graph(graph) {} return 0; }",
        "fn f(a: i64) -> i64 { arena graph(a) {} return graph; }",
        "fn f(a: i64) -> i64 { arena outer(a) { arena inner(outer) {} let mut x: i64 = inner; } return 0; }",
    ] {
        rejects(text, codes::UNKNOWN_NAME);
    }
    rejects(
        "fn f(a: i64) -> i64 { arena a(a) {} return 0; }",
        codes::DUPLICATE_NAME,
    );
    rejects(
        "fn f(a: i64) -> i64 { arena core(a) {} return 0; }",
        codes::UNSUPPORTED_SPECIALIZATION,
    );
}

#[test]
fn ownership_body_binding_uses_existing_expression_local_block_and_depth_budgets() {
    for (text, limits) in [
        (
            "fn f(x: i64) -> i64 { return &x; }",
            BindingLimits {
                expressions: 1,
                ..Default::default()
            },
        ),
        (
            "fn f(x: i64) -> i64 { arena graph(x) {} return 0; }",
            BindingLimits {
                locals: 1,
                ..Default::default()
            },
        ),
        (
            "fn f(x: i64) -> i64 { arena graph(x) {} return 0; }",
            BindingLimits {
                blocks: 1,
                ..Default::default()
            },
        ),
        (
            "fn f(x: i64) -> i64 { return &x.field.field.field; }",
            BindingLimits {
                depth: 4,
                ..Default::default()
            },
        ),
    ] {
        assert!(matches!(
            Files::new(text).bind(limits).1,
            Err(BindingError::Limit(_))
        ));
    }
}

#[test]
fn extended_forms_reach_body_analysis_even_without_constructed_annotations() {
    for text in [
        "fn f(x: i64) -> i64 { return &x; }",
        "fn f(x: i64) -> i64 { return (*x); }",
        "fn f(x: i64) -> i64 { x.field = 1; return 0; }",
        "fn f(x: i64) -> i64 { arena graph(x) {} return 0; }",
        "fn f(x: i64) -> i64 { return 0; x.field = 1; }",
    ] {
        let (sources, program) = Files::new(text).program();
        assert!(program.types.is_empty());
        let checked =
            lang_hir::program::check_program(program, &sources, Default::default()).unwrap();
        assert!(checked.program.is_none());
        assert!(
            checked
                .diagnostics
                .iter()
                .any(|d| d.severity == lang_diagnostics::Severity::Error)
        );
        for d in checked.diagnostics {
            d.validate(&sources).unwrap();
        }
    }
}

#[test]
fn core_cannot_be_shadowed_by_parameters_locals_or_pattern_bindings() {
    for text in [
        "fn f(core: i64) -> i64 { return core; }",
        "fn f() -> i64 { let mut core: i64 = 1; return core; }",
        "fn f(x: i64) -> i64 { match x { core => { return core; }, } }",
    ] {
        let (_, result) = Files::new(text).bind(BindingLimits::default());
        let result = result.unwrap();
        assert!(result.program.is_none());
        assert_eq!(
            result.diagnostics[0].code,
            codes::UNSUPPORTED_SPECIALIZATION,
            "{text}"
        );
    }
    // Record field names are not bindings in the root namespace.
    Files::new("struct R { core: i64 } fn f() -> i64 { return R { core: 1 }.core; }").program();
}

fn rejects(text: &str, code: lang_diagnostics::DiagnosticCode) {
    let files = Files::new(text);
    let (_, analysis) = files.bind(BindingLimits::default());
    let analysis = analysis.unwrap();
    assert!(analysis.program.is_none(), "{text}");
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == code),
        "{text}: {:?}",
        analysis.diagnostics
    );
}

fn type_analysis(text: &str) -> (SourceMap, r::Program, lang_hir::types::Analysis) {
    let (sources, program) = Files::new(text).program();
    let analysis = lang_hir::types::analyze(&program, &sources, Default::default()).unwrap();
    for diagnostic in &analysis.diagnostics {
        diagnostic.validate(&sources).unwrap();
    }
    (sources, program, analysis)
}

#[test]
fn source_types_are_interned_structurally_and_keep_each_use_span() {
    let (sources, program, analysis) = type_analysis(
        "struct Item { n: i64 } fn f(a: &mut core::Vec<shared Item>, b: &mut core::Vec<shared Item>, c: &Item, d: weak Item, e: arena Item) -> core::Unit { return 0; }",
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let f = &program.functions[0];
    let a = f.locals[0].annotation.unwrap();
    let b = f.locals[1].annotation.unwrap();
    assert_eq!(a.value_type, b.value_type);
    assert_ne!(a.span, b.span);
    assert_eq!(
        sources.slice(a.span).unwrap(),
        "&mut core::Vec<shared Item>"
    );
    let r::Type::Constructed(id) = a.value_type else {
        panic!()
    };
    assert_eq!(program.types[id.0].span, a.span);
    assert!(matches!(
        program.types[id.0].constructor,
        r::TypeConstructor::Loan(r::LoanKind::Exclusive, _)
    ));
    for (index, ty) in program.types.iter().enumerate() {
        sources.slice(ty.span).unwrap();
        for arg in ty.constructor.arguments() {
            if let r::Type::Constructed(child) = arg {
                assert!(child.0 < index);
            }
        }
    }
}

#[test]
fn builtin_copy_drop_and_region_properties_follow_the_source_profile() {
    let (_, program, analysis) = type_analysis(
        "struct C { x: core::Option<i64>, y: core::Result<bool, core::BoundsError> } struct O { x: core::Option<core::Text> } fn f(a: core::Bytes, b: core::Vec<i64>, c: core::Map<core::Text, bool>, d: shared C, e: weak C, f: arena core::Text, g: &core::Allocator) -> core::Unit { return 0; }",
    );
    let facts = analysis.facts.unwrap();
    let copy = facts.get(r::Type::Nominal(r::DefinitionId(0))).unwrap();
    let owned = facts.get(r::Type::Nominal(r::DefinitionId(1))).unwrap();
    assert!(copy.is_copy && !copy.requires_drop);
    assert!(!owned.is_copy && owned.requires_drop);
    let f = &program.functions[0];
    for local in &f.locals[..5] {
        let properties = facts.get(local.annotation.unwrap().value_type).unwrap();
        assert!(!properties.is_copy && properties.requires_drop);
    }
    let arena = facts
        .get(f.locals[5].annotation.unwrap().value_type)
        .unwrap();
    assert!(arena.is_copy && arena.contains_arena && !arena.requires_drop);
    let loan = facts
        .get(f.locals[6].annotation.unwrap().value_type)
        .unwrap();
    assert!(loan.contains_loan && loan.contains_allocator && !loan.is_copy && !loan.requires_drop);
    assert!(facts.get(f.result.value_type).unwrap().is_copy);
    for name in [
        "AllocError",
        "BoundsError",
        "BufferError",
        "Utf8Error",
        "OwnershipError",
        "ArenaError",
    ] {
        let (_, p, a) = type_analysis(&format!("fn f(x: core::{name}) -> i64 {{ return 0; }}"));
        let properties = a
            .facts
            .unwrap()
            .get(p.functions[0].locals[0].annotation.unwrap().value_type)
            .unwrap();
        assert!(properties.is_copy && !properties.requires_drop, "{name}");
    }
}

#[test]
fn unsupported_builtin_specializations_are_source_diagnostics() {
    for ty in [
        "core::Unknown",
        "core::Text<i64>",
        "core::Vec",
        "core::Result<i64>",
        "core::Option<i64, bool>",
        "core::Map<i64, bool, i64>",
        "Item<i64>",
        "core::Vec::Nested<i64>",
    ] {
        rejects(
            &format!("struct Item {{ n: i64 }} fn f(x: {ty}) -> i64 {{ return 0; }}"),
            codes::UNSUPPORTED_SPECIALIZATION,
        );
    }
}

#[test]
fn recursive_indirection_is_finite_but_inline_cycles_are_rejected() {
    for ty in [
        "core::Vec<Node>",
        "core::Map<i64, Node>",
        "shared Node",
        "weak Node",
        "arena Node",
    ] {
        let (_, _, analysis) = type_analysis(&format!("struct Node {{ next: {ty} }}"));
        assert!(analysis.facts.is_some(), "{ty}: {:?}", analysis.diagnostics);
    }
    for ty in ["Node", "core::Option<Node>", "core::Result<Node, i64>"] {
        let (_, _, analysis) = type_analysis(&format!("struct Node {{ next: {ty} }}"));
        assert!(analysis.facts.is_none());
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|d| d.code == codes::INVALID_SHAPE)
        );
    }
}

#[test]
fn type_storage_and_return_restrictions_precede_body_typing() {
    for (text, code) in [
        ("struct Bad { x: &i64 }", codes::INVALID_OWNERSHIP_TYPE),
        (
            "struct Bad { x: core::Vec<&i64> }",
            codes::INVALID_OWNERSHIP_TYPE,
        ),
        (
            "struct Bad { x: core::Allocator }",
            codes::INVALID_OWNERSHIP_TYPE,
        ),
        (
            "fn f(x: core::Allocator) -> i64 { return 0; }",
            codes::INVALID_OWNERSHIP_TYPE,
        ),
        ("fn f() -> &i64 { return 0; }", codes::REGION_ESCAPE),
        (
            "fn f() -> core::Option<arena i64> { return 0; }",
            codes::REGION_ESCAPE,
        ),
        (
            "struct Node { children: core::Vec<Node>, token: arena i64 } struct Bad { x: shared Node }",
            codes::REGION_ESCAPE,
        ),
        (
            "struct Node { children: core::Vec<Node>, token: arena i64 } struct Bad { x: weak Node }",
            codes::REGION_ESCAPE,
        ),
        (
            "struct Bad { x: core::Map<bool, i64> }",
            codes::UNSUPPORTED_SPECIALIZATION,
        ),
    ] {
        let (sources, program, analysis) = type_analysis(text);
        assert!(analysis.facts.is_none(), "{text}");
        assert!(
            analysis.diagnostics.iter().any(|d| d.code == code),
            "{text}: {:?}",
            analysis.diagnostics
        );
        let checked =
            lang_hir::program::check_program(program, &sources, Default::default()).unwrap();
        assert!(checked.program.is_none());
    }
    // Region-bearing owning containers are permitted locally; region-flow
    // analysis must still prevent escaping a particular arena at body level.
    let (_, _, good) = type_analysis(
        "fn f(x: core::Vec<arena i64>, allocator: &core::Allocator) -> i64 { return 0; }",
    );
    assert!(good.facts.is_some());
}

#[test]
fn nested_private_types_cannot_leak_through_public_signatures() {
    rejects(
        "struct Hidden { n: i64 } pub fn f(x: core::Vec<shared Hidden>) -> i64 { return 0; }",
        codes::MODULE_ERROR,
    );
    let (sources, mut program, _) =
        type_analysis("struct Hidden { n: i64 } struct Public { x: core::Vec<shared Hidden> }");
    program.definitions[1].public = true;
    let analysis = lang_hir::types::analyze(&program, &sources, Default::default()).unwrap();
    assert!(analysis.facts.is_none());
    let diagnostic = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == codes::MODULE_ERROR)
        .unwrap();
    assert_eq!(
        diagnostic.secondary[0].span,
        program.definitions[0].name.span
    );
}

#[test]
fn ownership_type_facts_do_not_authorize_executable_programs() {
    let (sources, program, analysis) = type_analysis("fn f(x: &i64) -> i64 { return 0; }");
    assert!(analysis.facts.is_some());
    assert!(matches!(
        lang_hir::program::check_program(program.clone(), &sources, Default::default()),
        Err(lang_hir::program::CheckError::OwnershipProfile { .. })
    ));
    assert!(matches!(
        lang_hir::program::check_expression_function(
            program,
            &sources,
            Default::default(),
            r::FunctionId(0)
        ),
        Err(lang_hir::program::CheckError::OwnershipProfile { .. })
    ));
}

#[test]
fn source_type_work_allocation_instance_and_depth_limits_are_enforced() {
    use lang_hir::types::{Error, Limits, analyze};
    let (sources, program, analysis) = type_analysis(
        "struct Node { next: core::Vec<Node>, text: core::Text } fn f(x: &Node) -> core::Unit { return 0; }",
    );
    let facts = analysis.facts.unwrap();
    let exact = Limits {
        work: facts.work_used(),
        allocations: facts.allocation_bytes(),
        ..Limits::default()
    };
    assert_eq!(
        analyze(&program, &sources, exact).unwrap().facts.unwrap(),
        facts
    );
    for limits in [
        Limits {
            work: facts.work_used() - 1,
            ..Limits::default()
        },
        Limits {
            allocations: facts.allocation_bytes() - 1,
            ..Limits::default()
        },
        Limits {
            constructed: program.types.len() - 1,
            ..Limits::default()
        },
        Limits {
            members: 1,
            ..Limits::default()
        },
        Limits {
            depth: 1,
            ..Limits::default()
        },
    ] {
        assert!(matches!(
            analyze(&program, &sources, limits),
            Err(Error::Limit(_))
        ));
    }
    let (_, bound) =
        Files::new("fn f(x: core::Vec<core::Text>) -> i64 { return 0; }").bind(BindingLimits {
            types: 1,
            ..BindingLimits::default()
        });
    assert!(matches!(bound, Err(BindingError::Limit(_))));
    let (sources, bad) = Files::new("struct Bad { x: &i64 }").program();
    assert!(matches!(
        analyze(
            &bad,
            &sources,
            Limits {
                diagnostics: 0,
                ..Limits::default()
            }
        ),
        Err(Error::Limit(_))
    ));
}

#[test]
fn malformed_resolved_type_arenas_fail_structurally() {
    use lang_hir::types::{Error, InputError, analyze};
    let (sources, program, _) = type_analysis("fn f(x: core::Vec<i64>) -> i64 { return 0; }");
    let mut bad = program.clone();
    bad.types[0].constructor =
        r::TypeConstructor::Vec(r::Type::Constructed(r::ConstructedTypeId(0)));
    assert!(matches!(
        analyze(&bad, &sources, Default::default()),
        Err(Error::Input {
            kind: InputError::ForwardConstructed(0),
            ..
        })
    ));
    bad.types[0].constructor = r::TypeConstructor::Vec(r::Type::Nominal(r::DefinitionId(99)));
    assert!(matches!(
        analyze(&bad, &sources, Default::default()),
        Err(Error::Input {
            kind: InputError::InvalidDefinition(99),
            ..
        })
    ));
    bad = program.clone();
    bad.functions[0].parameters[0] = r::LocalId(99);
    assert!(matches!(
        analyze(&bad, &sources, Default::default()),
        Err(Error::Input {
            kind: InputError::InvalidParameter(99),
            ..
        })
    ));
    bad = program.clone();
    bad.functions[0].module = r::ModuleId(99);
    assert!(matches!(
        analyze(&bad, &sources, Default::default()),
        Err(Error::Input {
            kind: InputError::InvalidModule(99),
            ..
        })
    ));
    bad = program.clone();
    bad.functions[0].name.text = "wrong".into();
    assert!(matches!(
        analyze(&bad, &sources, Default::default()),
        Err(Error::Input {
            kind: InputError::NameTextMismatch,
            ..
        })
    ));
    bad = program.clone();
    bad.types.clear();
    assert!(matches!(
        analyze(&bad, &sources, Default::default()),
        Err(Error::Input {
            kind: InputError::InvalidConstructed(0),
            ..
        })
    ));
    assert!(matches!(
        lang_hir::program::check_program(bad, &sources, Default::default()),
        Err(lang_hir::program::CheckError::Input {
            kind: lang_hir::program::CheckInputErrorKind::InvalidConstructed(0),
            ..
        })
    ));
}

#[test]
fn long_nominal_property_chains_are_iterative_and_depth_bounded() {
    let mut text = String::new();
    for i in 0..400 {
        text.push_str(&format!(
            "struct N{i} {{ value: {} }}\n",
            if i == 399 {
                "core::Text".into()
            } else {
                format!("N{}", i + 1)
            }
        ));
    }
    let (sources, program) = Files::new(&text).program();
    assert!(matches!(
        lang_hir::types::analyze(&program, &sources, Default::default()),
        Err(lang_hir::types::Error::Limit(_))
    ));
    let analysis = lang_hir::types::analyze(
        &program,
        &sources,
        lang_hir::types::Limits {
            depth: 512,
            ..Default::default()
        },
    )
    .unwrap();
    let facts = analysis.facts.unwrap();
    assert!(
        facts
            .definitions()
            .iter()
            .all(|p| !p.is_copy && p.requires_drop)
    );
}

#[test]
fn generated_source_copy_properties_match_an_independent_structural_oracle() {
    let mut text = String::new();
    let mut expected = Vec::new();
    let mut seed = 17u64;
    for i in 0..200 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let previous = if i == 0 { 0 } else { (seed as usize) % i };
        let (ty, copy) = match if i == 0 { 0 } else { (seed >> 32) % 6 } {
            0 => ("bool".into(), true),
            1 => ("core::Text".into(), false),
            2 => (format!("core::Option<N{previous}>"), expected[previous]),
            3 => (
                format!("core::Result<N{previous}, core::BoundsError>"),
                expected[previous],
            ),
            4 => (format!("core::Vec<N{previous}>"), false),
            _ => (format!("shared N{previous}"), false),
        };
        text.push_str(&format!("struct N{i} {{ x: {ty} }}\n"));
        expected.push(copy);
    }
    let (_, _, analysis) = type_analysis(&text);
    let facts = analysis.facts.unwrap();
    for (properties, copy) in facts.definitions().iter().zip(expected) {
        assert_eq!(properties.is_copy, copy);
        assert_eq!(properties.requires_drop, !copy);
        assert!(
            !properties.contains_loan
                && !properties.contains_arena
                && !properties.contains_allocator
        );
    }
}

#[test]
fn resolves_forward_and_mutually_recursive_functions_with_host_and_bool_signatures() {
    let files = Files::new(
        "extern fn next(value: i64) -> i64; fn a(flag: bool) -> i64 { if flag { return b(next(1)); } return 0; } fn b(value: i64) -> i64 { return a(false); }",
    );
    let (_, program) = files.program();
    assert_eq!(program.imports[0].qualified_name, "next");
    assert_eq!(
        program.functions[0].locals[0]
            .annotation
            .map(|ty| ty.value_type),
        Some(r::Type::Bool)
    );
    let calls: Vec<_> = program.functions[0]
        .expressions
        .iter()
        .filter_map(|e| {
            if let r::ExpressionKind::Call { callee, .. } = &e.kind {
                Some(*callee)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        calls,
        [
            r::Callable::Host(r::ImportId(0)),
            r::Callable::Guest(r::FunctionId(1))
        ]
    );
    assert!(program.functions[1].expressions.iter().any(|e| matches!(
        e.kind,
        r::ExpressionKind::Call {
            callee: r::Callable::Guest(r::FunctionId(0)),
            ..
        }
    )));
}

#[test]
fn nominal_types_paths_and_written_field_order_survive_resolution() {
    let files = Files::new(
        "mod data; use data::Pair; use data::Maybe; fn main() -> i64 { let mut p: Pair = Pair { right: true, left: 42 }; let mut m: Maybe = Maybe::Some(p); match m { Maybe::Some(Pair { left: value, right: _ }) => { return value; }, Maybe::None => { return 0; }, } }",
    );
    files.write(
        "data.lang",
        "pub struct Pair { left: i64, right: bool } pub enum Maybe { None, Some(Pair) }",
    );
    let (_, program) = files.program();
    assert_eq!(program.definitions.len(), 2);
    let function = &program.functions[0];
    let fields = function
        .expressions
        .iter()
        .find_map(|e| match &e.kind {
            r::ExpressionKind::Record { fields, .. } => Some(fields),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        fields
            .iter()
            .map(|f| f.name.text.as_str())
            .collect::<Vec<_>>(),
        ["right", "left"]
    );
    let binding = function
        .locals
        .iter()
        .find(|l| l.name.text == "value")
        .unwrap();
    assert_eq!(binding.kind, r::BindingKind::Pattern);
    assert_eq!(binding.annotation, None);
    assert!(
        function
            .patterns
            .iter()
            .any(|p| matches!(p.kind, r::PatternKind::Record { .. }))
    );
    assert!(function.expressions.iter().any(|e| matches!(
        e.kind,
        r::ExpressionKind::Call {
            callee: r::Callable::Case { .. },
            ..
        }
    )));
}

#[test]
fn every_local_is_visible_after_its_initializer_only_and_inside_its_block() {
    rejects(
        "fn f() -> i64 { let mut x: i64 = x; return x; }",
        codes::UNKNOWN_NAME,
    );
    rejects(
        "fn f() -> i64 { return x; let mut x: i64 = 1; }",
        codes::UNKNOWN_NAME,
    );
    rejects(
        "fn f() -> i64 { if true { let mut x: i64 = 1; } return x; }",
        codes::UNKNOWN_NAME,
    );
    rejects("fn f() -> i64 { x = 1; return 0; }", codes::UNKNOWN_NAME);
    let files =
        Files::new("fn f() -> i64 { let mut x: i64 = 1; while x < 3 { x = x + 1; } return x; }");
    let (_, program) = files.program();
    assert_eq!(program.functions[0].locals.len(), 1);
}

#[test]
fn name_reuse_is_forbidden_across_parameters_branches_and_pattern_arms() {
    for text in [
        "fn f(x: i64, x: i64) -> i64 { return x; }",
        "fn f(x: i64) -> i64 { let mut x: i64 = 1; return x; }",
        "fn f() -> i64 { if true { let mut x: i64 = 1; } else { let mut x: i64 = 2; } return 0; }",
        "fn f() -> i64 { match true { x => { return 1; }, x => { return 2; }, } }",
        "fn f(x: bool) -> i64 { match x { x => { return 1; }, } }",
    ] {
        rejects(text, codes::DUPLICATE_NAME);
    }
}

#[test]
fn parameters_and_pattern_bindings_are_immutable() {
    rejects(
        "fn f(x: i64) -> i64 { x = 1; return x; }",
        codes::IMMUTABLE_ASSIGNMENT,
    );
    rejects(
        "fn f() -> i64 { match 1 { x => { x = 2; return x; }, } }",
        codes::IMMUTABLE_ASSIGNMENT,
    );
}

#[test]
fn pattern_bindings_are_arm_local_and_unreachable_arms_still_resolve() {
    rejects(
        "fn f() -> i64 { match 1 { x => { return x; }, _ => { return x; }, } }",
        codes::UNKNOWN_NAME,
    );
    rejects(
        "fn f() -> i64 { match 1 { _ => { return 1; }, _ => { return missing; }, } }",
        codes::UNKNOWN_NAME,
    );
    rejects(
        "fn f() -> i64 { match 1 { x => { return x; }, } return x; }",
        codes::UNKNOWN_NAME,
    );
}

#[test]
fn locals_shadow_module_values_and_cannot_be_called() {
    rejects(
        "fn value() -> i64 { return 1; } fn f() -> i64 { let mut value: i64 = 2; return value(); }",
        codes::TYPE_MISMATCH,
    );
    let files = Files::new("pub mod m; fn f() -> i64 { let mut m: i64 = 1; return m::g(); }");
    files.write("m.lang", "pub fn g() -> i64 { return 1; }");
    assert!(
        files
            .bind(BindingLimits::default())
            .1
            .unwrap()
            .program
            .is_none()
    );
    files.write(
        "main.lang",
        "pub mod m; fn f() -> i64 { let mut m: i64 = 1; return root::m::g() + m; }",
    );
    let (_, program) = files.program();
    assert!(
        program.functions[0]
            .expressions
            .iter()
            .any(|e| matches!(e.kind, r::ExpressionKind::Local(_)))
    );
}

#[test]
fn public_signatures_cannot_expose_private_nominal_types() {
    for text in [
        "struct Secret {} pub fn f(x: Secret) -> i64 { return 0; }",
        "struct Secret {} pub fn f() -> Secret { return Secret {}; }",
        "struct Secret {} pub struct Public { secret: Secret }",
        "struct Secret {} pub enum Public { Secret(Secret) }",
    ] {
        rejects(text, codes::MODULE_ERROR);
    }
    let files = Files::new("pub struct Visible {} pub fn f(x: Visible) -> Visible { return x; }");
    files.program();
}

#[test]
fn type_positions_require_types_and_construction_patterns_require_nominal_names() {
    rejects(
        "fn other() -> i64 { return 1; } fn f(x: other) -> i64 { return 0; }",
        codes::TYPE_MISMATCH,
    );
    rejects("fn f() -> i64 { return f {}; }", codes::INVALID_SHAPE);
    rejects(
        "struct R {} fn f() -> i64 { match 1 { R::No => { return 1; }, } }",
        codes::MODULE_ERROR,
    );
    rejects(
        "struct R {} fn f() -> i64 { return R(); }",
        codes::TYPE_MISMATCH,
    );
}

#[test]
fn zero_payload_syntax_is_preserved_for_later_shape_checking() {
    let files = Files::new(
        "enum E { None } fn f() -> E { return E::None; } fn g() -> E { return E::None(); }",
    );
    let (_, program) = files.program();
    assert!(matches!(
        program.functions[0].expressions[0].kind,
        r::ExpressionKind::Case { .. }
    ));
    assert!(matches!(
        program.functions[1].expressions[0].kind,
        r::ExpressionKind::Call {
            callee: r::Callable::Case { .. },
            ..
        }
    ));
}

#[test]
fn multiple_function_errors_keep_related_source_spans_without_an_artifact() {
    let files = Files::new(
        "fn a(x: i64) -> i64 { x = 1; return x; } fn b(y: i64) -> i64 { y = 2; return y; }",
    );
    let (sources, analysis) = files.bind(BindingLimits::default());
    let analysis = analysis.unwrap();
    assert!(analysis.program.is_none());
    assert_eq!(analysis.diagnostics.len(), 2);
    assert!(analysis.diagnostics.iter().all(|d| d.secondary.len() == 1));
    assert_eq!(
        sources.slice(analysis.diagnostics[1].primary.span).unwrap(),
        "y"
    );
}

#[test]
fn work_is_charged_at_exact_boundaries_including_module_path_resolution() {
    let minimum = |text: &str| {
        let files = Files::new(text);
        let first = (1..512)
            .find(|work| {
                files
                    .bind(BindingLimits {
                        work: *work,
                        ..BindingLimits::default()
                    })
                    .1
                    .is_ok()
            })
            .unwrap();
        assert!(
            files
                .bind(BindingLimits {
                    work: first,
                    ..BindingLimits::default()
                })
                .1
                .unwrap()
                .program
                .is_some()
        );
        assert!(matches!(
            files
                .bind(BindingLimits {
                    work: first - 1,
                    ..BindingLimits::default()
                })
                .1,
            Err(BindingError::Limit(_))
        ));
        first
    };
    let scalar = minimum("fn f() -> i64 { return 1; }");
    assert!(minimum("fn f() -> i64 { return f(); }") > scalar);
    assert!(minimum("/*a*/ /*b*/ /*c*/ fn f() -> i64 { return 1; }") > scalar);
}

#[test]
fn arenas_diagnostics_and_recursion_are_bounded_without_partial_success() {
    for (text, limits) in [
        (
            "fn f() -> i64 { return 1; }",
            BindingLimits {
                expressions: 0,
                ..BindingLimits::default()
            },
        ),
        (
            "fn f() -> i64 { return 1; }",
            BindingLimits {
                blocks: 0,
                ..BindingLimits::default()
            },
        ),
        (
            "fn f(x: i64) -> i64 { return x; }",
            BindingLimits {
                locals: 0,
                ..BindingLimits::default()
            },
        ),
        (
            "fn f() -> i64 { match 1 { _ => { return 1; }, } }",
            BindingLimits {
                patterns: 0,
                ..BindingLimits::default()
            },
        ),
        (
            "fn f() -> i64 { return missing; }",
            BindingLimits {
                diagnostics: 0,
                ..BindingLimits::default()
            },
        ),
        (
            "fn f() -> i64 { return 1; }",
            BindingLimits {
                depth: 0,
                ..BindingLimits::default()
            },
        ),
    ] {
        assert!(matches!(
            Files::new(text).bind(limits).1,
            Err(BindingError::Limit(_))
        ));
    }
    let expression = format!("{}1", "1 + ".repeat(180));
    let files = Files::new(&format!("fn f() -> i64 {{ return {expression}; }}"));
    files.program();
    assert!(matches!(
        files
            .bind(BindingLimits {
                depth: 32,
                ..BindingLimits::default()
            })
            .1,
        Err(BindingError::Limit(_))
    ));
}

#[test]
fn original_snapshot_identity_is_required() {
    let files = Files::new("fn f() -> i64 { return 1; }");
    let (_, graph) = files.graph();
    assert!(matches!(
        bind_program(&graph, &SourceMap::new(), BindingLimits::default()),
        Err(BindingError::Source(_))
    ));
}

#[test]
fn full_syntax_fixture_resolves_and_retains_exact_type_annotation_spans() {
    let files = Files::new(include_str!("../../syntax/tests/fixtures/accepted.lang"));
    files.write(
        "math.lang",
        "pub fn increment(value: i64) -> i64 { return value + 1; }",
    );
    let (sources, program) = files.program();
    for function in &program.functions {
        let result = sources.slice(function.result.span).unwrap().trim();
        assert!(matches!(result, "i64" | "Maybe"));
        for local in &function.locals {
            if let Some(annotation) = local.annotation {
                assert!(!sources.slice(annotation.span).unwrap().trim().is_empty());
            }
        }
    }
    assert_eq!(program.definitions.len(), 3);
    assert_eq!(program.imports.len(), 1);
    assert_eq!(program.functions.len(), 3);
}

#[test]
fn parsed_nominal_cycles_and_duplicate_record_fields_fail_layout_validation() {
    for text in [
        "pub struct A { next: A }",
        "pub struct A { next: B } pub enum B { Next(A) }",
        "pub struct R { same: i64, same: bool }",
    ] {
        let (sources, program) = Files::new(text).program();
        let analysis = lang_hir::layout::analyze_layouts(
            &program.definitions,
            &sources,
            lang_hir::layout::LayoutLimits::default(),
        )
        .unwrap();
        assert!(analysis.layouts.is_none());
        assert!(
            analysis
                .diagnostics
                .iter()
                .all(|d| d.code == codes::INVALID_SHAPE)
        );
        for d in &analysis.diagnostics {
            d.validate(&sources).unwrap();
        }
    }
}

#[test]
fn parsed_type_layouts_enforce_the_4096_cell_boundary() {
    let mut text = "pub struct R0 {}".to_owned();
    for i in 1..=10 {
        text.push_str(&format!(
            " pub struct R{i} {{ a: R{}, b: R{} }}",
            i - 1,
            i - 1
        ));
    }
    text.push_str(" pub struct Limit { a: R10, b: R10, scalar: i64 }");
    let (sources, program) = Files::new(&text).program();
    let layouts = lang_hir::layout::analyze_layouts(
        &program.definitions,
        &sources,
        lang_hir::layout::LayoutLimits::default(),
    )
    .unwrap()
    .layouts
    .unwrap();
    assert_eq!(layouts.last().unwrap().cells(), 4096);
    text.push_str(" pub struct TooLarge { limit: Limit }");
    let (sources, program) = Files::new(&text).program();
    let error = lang_hir::layout::analyze_layouts(
        &program.definitions,
        &sources,
        lang_hir::layout::LayoutLimits::default(),
    )
    .unwrap_err();
    let lang_hir::layout::LayoutError::Limit(diagnostic) = error else {
        panic!("expected cell limit")
    };
    diagnostic.validate(&sources).unwrap();
    assert!(diagnostic.message.contains("cell limit"));
}
