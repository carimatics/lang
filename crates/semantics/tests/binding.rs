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
fn ownership_body_forms_fail_with_spans_before_legacy_hir_assumptions() {
    for text in [
        "fn f(x: i64) -> i64 { return &x; }",
        "fn f(x: i64) -> i64 { return (*x); }",
        "fn f(x: i64) -> i64 { x.field = 1; return 0; }",
        "fn f(x: i64) -> i64 { arena graph(x) {} return 0; }",
        "fn f() -> i64 { return core::vec_len::<i64>(values); }",
        "fn f() -> i64 { return 1; let mut x: &i64 = &missing; }",
    ] {
        let files = Files::new(text);
        let (sources, graph) = files.graph();
        let error = bind_program(&graph, &sources, BindingLimits::default()).unwrap_err();
        let BindingError::UnsupportedSyntax { span } = error else {
            panic!("{text}: {error:?}")
        };
        assert!(!sources.slice(span).unwrap().trim().is_empty(), "{text}");
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
