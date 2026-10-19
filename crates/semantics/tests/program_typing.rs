use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lang_diagnostics::{DiagnosticCode, Severity, SourceMap, codes};
use lang_hir::{
    program::{
        ArenaKind, CheckError, CheckInputErrorKind, CheckLimits, Flow, ProgramAnalysis,
        check_program,
    },
    resolved as r,
};
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_modules},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new(text: &str) -> Self {
        let directory = std::env::temp_dir().join(format!(
            "lang-program-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let files = Self(directory);
        files.write("main.lang", text);
        files
    }
    fn write(&self, file: &str, text: &str) {
        let path = self.0.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn resolved(&self) -> (SourceMap, r::Program) {
        let mut sources = SourceMap::new();
        let loaded = load_modules(
            &mut sources,
            &self.0.join("main.lang"),
            &self.0,
            ModuleLimits::default(),
        )
        .unwrap();
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
        let bound =
            bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
        assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
        (sources, bound.program.unwrap())
    }
    fn check(&self) -> (SourceMap, ProgramAnalysis) {
        let (sources, source) = self.resolved();
        let analysis = checked(source, &sources, CheckLimits::default()).unwrap();
        (sources, analysis)
    }
    fn pass(&self) -> (SourceMap, ProgramAnalysis) {
        let (sources, analysis) = self.check();
        assert!(analysis.program.is_some(), "{:?}", analysis.diagnostics);
        assert!(
            analysis
                .diagnostics
                .iter()
                .all(|d| d.severity == Severity::Warning)
        );
        (sources, analysis)
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn c_exports_require_the_root_module_even_when_the_module_is_public() {
    let files = Files::new("pub mod child; fn main() -> i64 { return child::exported(); }");
    files.write(
        "child.lang",
        "pub extern \"C\" fn exported() -> i64 { return 1; }",
    );
    let (sources, source) = files.resolved();
    let analysis =
        lang_hir::program::analyze_program(source, &sources, Default::default()).unwrap();
    assert!(analysis.program.is_none());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::FOREIGN_BOUNDARY
                && sources.slice(d.primary.span).unwrap() == "exported")
    );
}

fn checked(
    source: r::Program,
    sources: &SourceMap,
    limits: CheckLimits,
) -> Result<ProgramAnalysis, CheckError> {
    let analysis = check_program(source, sources, limits);
    match &analysis {
        Ok(analysis) => {
            for diagnostic in &analysis.diagnostics {
                diagnostic.validate(sources).unwrap();
            }
        }
        Err(CheckError::Limit(d) | CheckError::Layout(lang_hir::layout::LayoutError::Limit(d))) => {
            assert_eq!(d.code, codes::RESOURCE_LIMIT);
            d.validate(sources).unwrap();
        }
        Err(_) => {}
    }
    analysis
}

fn reject(text: &str, code: DiagnosticCode) -> ProgramAnalysis {
    let (_, analysis) = Files::new(text).check();
    assert!(analysis.program.is_none(), "accepted {text}");
    assert!(
        analysis.diagnostics.iter().any(|d| d.code == code),
        "{text}: {:?}",
        analysis.diagnostics
    );
    analysis
}

#[test]
fn source_functions_bool_control_flow_and_forward_calls_produce_typed_facts() {
    let files = Files::new(
        "fn first(flag: bool, value: i64) -> i64 { let mut running: bool = flag; while running { running = false; } if running { return later(value); } else { return -9223372036854775808 + 9223372036854775807; } } fn later(value: i64) -> i64 { return value + 1; }",
    );
    let (_, analysis) = files.pass();
    let program = analysis.program.unwrap();
    assert_eq!(
        program.functions()[0].locals(),
        [r::Type::Bool, r::Type::I64, r::Type::Bool]
    );
    let first = &program.source().functions[0];
    assert_eq!(program.functions()[0].blocks()[first.body.0], Flow::Returns);
    assert_eq!(
        program.functions()[0].expressions().len(),
        first.expressions.len()
    );
}

#[test]
fn module_calls_records_enums_fields_and_payloads_use_exact_nominal_types() {
    let files = Files::new(
        "pub mod data; use data::Pair; use data::Maybe; fn main() -> i64 { let mut p: Pair = Pair { right: true, left: 42 }; let mut m: Maybe = data::wrap(p); match m { Maybe::None => { return 0; }, Maybe::Some(Pair { right: _, left: value }) => { return value; }, } }",
    );
    files.write("data.lang", "pub struct Pair { left: i64, right: bool } pub enum Maybe { None, Some(Pair) } pub fn wrap(p: Pair) -> Maybe { return Maybe::Some(p); }");
    let (_, analysis) = files.pass();
    let program = analysis.program.unwrap();
    let main = &program.source().functions[0];
    let record = main
        .expressions
        .iter()
        .position(|e| matches!(e.kind, r::ExpressionKind::Record { .. }))
        .unwrap();
    assert_eq!(
        program.functions()[0].record_order(r::ExpressionId(record)),
        Some([1, 0].as_slice())
    );
    let local = main
        .locals
        .iter()
        .position(|l| l.name.text == "value")
        .unwrap();
    assert_eq!(program.functions()[0].locals()[local], r::Type::I64);
}

#[test]
fn nested_record_projections_bind_tighter_and_have_resolved_field_indices() {
    let (_, analysis) = Files::new("struct Pair { left: i64, right: bool } struct Outer { pair: Pair } fn f() -> bool { return Outer { pair: Pair { right: true, left: 1 } }.pair.right; }").pass();
    let program = analysis.program.unwrap();
    let function = &program.source().functions[0];
    let fields: Vec<_> = function
        .expressions
        .iter()
        .enumerate()
        .filter_map(|(index, e)| {
            if matches!(e.kind, r::ExpressionKind::Field { .. }) {
                Some(
                    program.functions()[0]
                        .field_index(r::ExpressionId(index))
                        .unwrap(),
                )
            } else {
                None
            }
        })
        .collect();
    assert_eq!(fields, [0, 1]);
}

#[test]
fn existing_integer_and_condition_contexts_keep_their_accepted_codes() {
    for text in [
        "fn f() -> i64 { return true; }",
        "fn f() -> i64 { return true + 1; }",
        "fn f() -> bool { return true < 1; }",
        "fn f() -> i64 { let mut x: i64 = true; return x; }",
        "fn f() -> i64 { let mut x: i64 = 1; x = false; return x; }",
    ] {
        reject(text, codes::EXPECTED_I64);
    }
    for text in [
        "fn f() -> i64 { if 1 { return 1; } return 0; }",
        "fn f() -> i64 { while 1 { return 1; } return 0; }",
    ] {
        reject(text, codes::EXPECTED_BOOL);
    }
    reject(
        "fn f() -> bool { let mut x: bool = 1; return x; }",
        codes::TYPE_MISMATCH,
    );
}

#[test]
fn calls_enforce_arity_host_signature_and_nominal_identity() {
    for text in [
        "fn g(x: i64) -> i64 { return x; } fn f() -> i64 { return g(); }",
        "fn g(x: i64) -> i64 { return x; } fn f() -> i64 { return g(true); }",
        "extern fn next(x: i64) -> i64; fn f() -> i64 { return next(false); }",
        "struct A {} struct B {} fn g(x: A) -> i64 { return 0; } fn f() -> i64 { return g(B {}); }",
    ] {
        let analysis = reject(text, codes::TYPE_MISMATCH);
        assert!(analysis.diagnostics.iter().any(|d| !d.secondary.is_empty()));
    }
    Files::new("struct A {} fn g(x: A) -> A { return x; } fn f() -> A { return g(A {}); }").pass();
}

#[test]
fn record_values_require_exact_fields_and_types() {
    for value in [
        "R { a: 1 }",
        "R { a: 1, b: true, c: 2 }",
        "R { a: 1, a: 2, b: true }",
    ] {
        reject(
            &format!("struct R {{ a: i64, b: bool }} fn f() -> R {{ return {value}; }}"),
            codes::INVALID_SHAPE,
        );
    }
    reject(
        "struct R { a: i64 } fn f() -> R { return R { a: false }; }",
        codes::TYPE_MISMATCH,
    );
    reject(
        "struct R {} fn f() -> i64 { return R {}.missing; }",
        codes::INVALID_SHAPE,
    );
    reject(
        "fn f() -> i64 { return true.missing; }",
        codes::TYPE_MISMATCH,
    );
    reject(
        "enum E { None } fn f() -> E { return E {}; }",
        codes::INVALID_SHAPE,
    );
}

#[test]
fn enum_payload_syntax_arity_and_types_are_checked() {
    for value in ["E::None()", "E::Some"] {
        reject(
            &format!("enum E {{ None, Some(i64) }} fn f() -> E {{ return {value}; }}"),
            codes::INVALID_SHAPE,
        );
    }
    for value in ["E::Some()", "E::Some(false)", "E::Some(1, 2)"] {
        reject(
            &format!("enum E {{ None, Some(i64) }} fn f() -> E {{ return {value}; }}"),
            codes::TYPE_MISMATCH,
        );
    }
    Files::new("enum E { None, Some(bool) } fn f() -> E { return E::Some(true); }").pass();
}

#[test]
fn patterns_check_nominal_shapes_field_sets_payloads_and_literal_types() {
    for pattern in ["R { a: _, b: _ }", "R { a: _ }", "R { a: _, a: _, b: _ }"] {
        let text = format!(
            "struct R {{ a: bool, b: bool }} fn f(value: R) -> i64 {{ match value {{ {pattern} => {{ return 1; }}, }} }}"
        );
        if pattern == "R { a: _, b: _ }" {
            Files::new(&text).pass();
        } else {
            reject(&text, codes::INVALID_SHAPE);
        }
    }
    reject(
        "struct R {} struct S {} fn f(value: R) -> i64 { match value { S {} => { return 1; }, } }",
        codes::TYPE_MISMATCH,
    );
    reject(
        "enum E { Some(bool) } fn f(value: E) -> i64 { match value { E::Some => { return 1; }, } }",
        codes::INVALID_SHAPE,
    );
    reject(
        "enum E { Some(bool) } fn f(value: E) -> i64 { match value { E::Some(_, _) => { return 1; }, } }",
        codes::INVALID_SHAPE,
    );
    let analysis = reject(
        "fn f(value: bool) -> i64 { match value { 1 => { return 1; }, } }",
        codes::TYPE_MISMATCH,
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.message.contains("expected `bool`, found `i64`"))
    );
}

#[test]
fn record_product_coverage_uses_all_columns_and_written_field_orders() {
    Files::new("struct P { a: bool, b: bool } fn f(p: P) -> i64 { match p { P { a: false, b: _ } => { return 0; }, P { b: false, a: true } => { return 1; }, P { a: true, b: true } => { return 2; }, } }").pass();
    let analysis = reject(
        "struct P { a: bool, b: bool } fn f(p: P) -> i64 { match p { P { a: false, b: _ } => { return 0; }, P { a: true, b: false } => { return 1; }, } }",
        codes::NON_EXHAUSTIVE_MATCH,
    );
    let missing = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == codes::NON_EXHAUSTIVE_MATCH)
        .unwrap();
    assert_eq!(missing.notes, ["missing pattern: P { a: true, b: true }"]);
}

#[test]
fn nested_enum_product_coverage_returns_a_concrete_missing_witness() {
    Files::new("struct P { flag: bool } enum E { None, Pair(bool, P) } fn f(e: E) -> i64 { match e { E::None => { return 0; }, E::Pair(false, _) => { return 1; }, E::Pair(true, P { flag: false }) => { return 2; }, E::Pair(true, P { flag: true }) => { return 3; }, } }").pass();
    let analysis = reject(
        "enum E { None, Some(bool) } fn f(e: E) -> i64 { match e { E::None => { return 0; }, E::Some(false) => { return 1; }, } }",
        codes::NON_EXHAUSTIVE_MATCH,
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.notes == ["missing pattern: E::Some(true)"])
    );
}

#[test]
fn integer_coverage_uses_a_complement_class_and_empty_records_are_inhabited() {
    let analysis = reject(
        "fn f(x: i64) -> i64 { match x { 0 => { return 1; }, } }",
        codes::NON_EXHAUSTIVE_MATCH,
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.notes == ["missing pattern: 1"])
    );
    Files::new("fn f(x: i64) -> i64 { match x { -9223372036854775808 => { return 1; }, 9223372036854775807 => { return 2; }, _ => { return 3; }, } }").pass();
    Files::new("struct Empty {} fn f(e: Empty) -> i64 { match e { Empty {} => { return 1; }, } }")
        .pass();
    let analysis = reject(
        "struct Empty {} fn f(e: Empty) -> i64 { match e {} }",
        codes::NON_EXHAUSTIVE_MATCH,
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.notes == ["missing pattern: Empty {}"])
    );
}

#[test]
fn covered_arms_warn_and_every_arm_body_is_still_type_checked() {
    let (_, analysis) = Files::new("fn f(x: bool) -> i64 { match x { false => { return 0; }, true => { return 1; }, _ => { return 2; }, } }").pass();
    let covered = analysis
        .diagnostics
        .iter()
        .find(|d| d.code == codes::UNREACHABLE_ARM)
        .unwrap();
    assert_eq!(covered.severity, Severity::Warning);
    assert_eq!(covered.secondary.len(), 2);
    let analysis = reject(
        "fn f(x: bool) -> i64 { match x { false => { return 0; }, true => { return 1; }, _ => { return false; }, } }",
        codes::EXPECTED_I64,
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::UNREACHABLE_ARM)
    );
}

#[test]
fn return_flow_requires_all_paths_and_unreachable_statements_keep_stable_warnings() {
    for text in [
        "fn f() -> i64 {}",
        "fn f(x: bool) -> i64 { if x { return 1; } }",
        "fn f() -> i64 { while true { return 1; } }",
    ] {
        reject(text, codes::MISSING_RETURN);
    }
    let (_, analysis) = Files::new("fn f() -> i64 { return 1; return 2; }").pass();
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::UNREACHABLE_STATEMENT)
    );
    let analysis = reject(
        "fn f() -> i64 { return 1; return false; }",
        codes::EXPECTED_I64,
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::UNREACHABLE_STATEMENT)
    );
}

#[test]
fn no_entry_is_required_for_typed_libraries_and_empty_source_is_valid() {
    Files::new("").pass();
    Files::new("fn library(value: bool) -> bool { return value; }").pass();
}

#[test]
fn coverage_and_witness_limits_return_resource_errors_instead_of_false_results() {
    let files = Files::new("fn f(x: bool) -> i64 { match x { true => { return 1; }, } }");
    let (sources, source) = files.resolved();
    for limits in [
        CheckLimits {
            exhaustiveness: 0,
            ..CheckLimits::default()
        },
        CheckLimits {
            witness_bytes: 0,
            ..CheckLimits::default()
        },
    ] {
        assert!(matches!(
            checked(source.clone(), &sources, limits),
            Err(CheckError::Limit(_))
        ));
    }
}

#[test]
fn coverage_work_is_shared_across_matches_and_charges_repeated_specialization() {
    let one = Files::new("fn f() -> i64 { match true { _ => {}, } return 1; }");
    let (sources, source) = one.resolved();
    let minimum = (0..1000)
        .find(|work| {
            matches!(
                checked(
                    source.clone(),
                    &sources,
                    CheckLimits {
                        exhaustiveness: *work,
                        ..CheckLimits::default()
                    }
                ),
                Ok(ProgramAnalysis {
                    program: Some(_),
                    ..
                })
            )
        })
        .unwrap();
    assert!(minimum > 1);
    let two =
        Files::new("fn f() -> i64 { match true { _ => {}, } match true { _ => {}, } return 1; }");
    let (sources, source) = two.resolved();
    assert!(matches!(
        checked(
            source.clone(),
            &sources,
            CheckLimits {
                exhaustiveness: minimum,
                ..CheckLimits::default()
            }
        ),
        Err(CheckError::Limit(_))
    ));
    assert!(
        checked(
            source,
            &sources,
            CheckLimits {
                exhaustiveness: minimum * 2,
                ..CheckLimits::default()
            }
        )
        .unwrap()
        .program
        .is_some()
    );
}

#[test]
fn type_check_work_structure_depth_and_diagnostic_limits_have_valid_spans() {
    let (sources, source) = Files::new("fn f() -> i64 { return 1; }").resolved();
    for limits in [
        CheckLimits {
            work: 0,
            ..CheckLimits::default()
        },
        CheckLimits {
            modules: 0,
            ..CheckLimits::default()
        },
        CheckLimits {
            functions: 0,
            ..CheckLimits::default()
        },
        CheckLimits {
            expressions: 0,
            ..CheckLimits::default()
        },
        CheckLimits {
            blocks: 0,
            ..CheckLimits::default()
        },
        CheckLimits {
            depth: 0,
            ..CheckLimits::default()
        },
    ] {
        assert!(matches!(
            checked(source.clone(), &sources, limits),
            Err(CheckError::Limit(_))
        ));
    }
    let (sources, source) = Files::new("fn f() -> i64 { return true; }").resolved();
    assert!(matches!(
        checked(
            source,
            &sources,
            CheckLimits {
                diagnostics: 0,
                ..CheckLimits::default()
            }
        ),
        Err(CheckError::Limit(_))
    ));
}

#[test]
fn forged_expression_ids_cycles_shared_nodes_and_orphans_are_rejected() {
    let (sources, source) = Files::new("fn f() -> i64 { return 1 + 2; }").resolved();
    let last = source.functions[0].expressions.len() - 1;
    for (kind, expected) in [
        (
            r::ExpressionKind::Add(r::ExpressionId(99), r::ExpressionId(0)),
            CheckInputErrorKind::InvalidId {
                arena: ArenaKind::Expression,
                index: 99,
            },
        ),
        (
            r::ExpressionKind::Add(r::ExpressionId(last), r::ExpressionId(0)),
            CheckInputErrorKind::Cycle(ArenaKind::Expression),
        ),
        (
            r::ExpressionKind::Add(r::ExpressionId(0), r::ExpressionId(0)),
            CheckInputErrorKind::SharedNode(ArenaKind::Expression),
        ),
    ] {
        let mut forged = source.clone();
        forged.functions[0].expressions[last].kind = kind;
        assert!(
            matches!(checked(forged, &sources, CheckLimits::default()), Err(CheckError::Input { kind, .. }) if kind == expected)
        );
    }
    let mut forged = source.clone();
    let orphan = forged.functions[0].expressions[0].clone();
    forged.functions[0].expressions.push(orphan);
    assert!(matches!(
        checked(forged, &sources, CheckLimits::default()),
        Err(CheckError::Input {
            kind: CheckInputErrorKind::Orphan(ArenaKind::Expression),
            ..
        })
    ));
}

#[test]
fn forged_block_cycles_and_out_of_scope_locals_cannot_reach_lowering() {
    let (sources, mut source) = Files::new("fn f() -> bool { return true; }").resolved();
    let body = source.functions[0].body;
    source.functions[0].blocks[body.0].statements[0].kind = r::StatementKind::While {
        condition: r::ExpressionId(0),
        body,
    };
    assert!(matches!(
        checked(source, &sources, CheckLimits::default()),
        Err(CheckError::Input {
            kind: CheckInputErrorKind::Cycle(ArenaKind::Block),
            ..
        })
    ));
    let (sources, mut source) =
        Files::new("fn f() -> i64 { if true { let mut x: i64 = 1; } return 0; }").resolved();
    source.functions[0].expressions.last_mut().unwrap().kind =
        r::ExpressionKind::Local(r::LocalId(0));
    assert!(matches!(
        checked(source, &sources, CheckLimits::default()),
        Err(CheckError::Input {
            kind: CheckInputErrorKind::InvisibleLocal(0),
            ..
        })
    ));
}

#[test]
fn forged_call_signatures_names_and_foreign_snapshots_are_rejected() {
    let (sources, source) =
        Files::new("fn g(x: i64) -> i64 { return x; } fn f() -> i64 { return g(1); }").resolved();
    let mut forged = source.clone();
    let last = forged.functions[1].expressions.len() - 1;
    forged.functions[1].expressions[last].kind = r::ExpressionKind::Call {
        callee: r::Callable::Guest(r::FunctionId(99)),
        arguments: vec![r::ExpressionId(0)],
    };
    assert!(matches!(
        checked(forged, &sources, CheckLimits::default()),
        Err(CheckError::Input {
            kind: CheckInputErrorKind::InvalidFunction(99),
            ..
        })
    ));
    let mut forged = source.clone();
    forged.functions[0].parameters.push(r::LocalId(0));
    assert!(matches!(
        checked(forged, &sources, CheckLimits::default()),
        Err(CheckError::Input {
            kind: CheckInputErrorKind::InvalidSignature,
            ..
        })
    ));
    let mut forged = source.clone();
    forged.functions[0].name.text = "wrong".into();
    assert!(matches!(
        checked(forged, &sources, CheckLimits::default()),
        Err(CheckError::Input {
            kind: CheckInputErrorKind::NameTextMismatch,
            ..
        })
    ));
    assert!(matches!(
        check_program(source, &SourceMap::new(), CheckLimits::default()),
        Err(CheckError::Input {
            kind: CheckInputErrorKind::Source(_),
            ..
        })
    ));
}

#[test]
fn pattern_cycles_are_rejected_before_recursive_specialization() {
    let (sources, mut source) = Files::new(
        "enum E { Some(bool) } fn f(e: E) -> i64 { match e { E::Some(_) => { return 1; }, } }",
    )
    .resolved();
    let function = &mut source.functions[0];
    let root = function.patterns.len() - 1;
    let r::PatternKind::Case { payload, .. } = &mut function.patterns[root].kind else {
        panic!("case pattern")
    };
    *payload = vec![r::PatternId(root)];
    assert!(matches!(
        checked(source, &sources, CheckLimits::default()),
        Err(CheckError::Input {
            kind: CheckInputErrorKind::Cycle(ArenaKind::Pattern),
            ..
        })
    ));
}

#[test]
fn constructor_specialization_matches_a_finite_domain_coverage_oracle() {
    // Each pattern has a separately computed set of concrete domain values.
    // Product record order is deliberately reversed in half the patterns.
    let cells = ["_", "false", "true"];
    let mut products = Vec::new();
    for a in 0..3 {
        for b in 0..3 {
            let text = if (a + b) % 2 == 0 {
                format!("P {{ a: {}, b: {} }}", cells[a], cells[b])
            } else {
                format!("P {{ b: {}, a: {} }}", cells[b], cells[a])
            };
            let mut mask = 0_u32;
            for av in 0..2 {
                for bv in 0..2 {
                    if (a == 0 || a - 1 == av) && (b == 0 || b - 1 == bv) {
                        mask |= 1 << (av * 2 + bv);
                    }
                }
            }
            products.push((text, mask));
        }
    }
    let variants = vec![
        ("E::None".into(), 1),
        ("E::Pair(_, _)".into(), 30),
        ("E::Pair(false, _)".into(), 6),
        ("E::Pair(true, _)".into(), 24),
        ("E::Pair(_, false)".into(), 10),
        ("E::Pair(_, true)".into(), 20),
        ("E::Pair(false, false)".into(), 2),
        ("E::Pair(true, true)".into(), 16),
        ("_".into(), 31),
    ];
    for (declaration, ty, patterns, full) in [
        ("struct P { a: bool, b: bool }", "P", products, 15),
        ("enum E { None, Pair(bool, bool) }", "E", variants, 31),
    ] {
        for indices in 0..729 {
            let selected = [indices % 9, (indices / 9) % 9, (indices / 81) % 9];
            let mut covered = 0;
            let mut unreachable = 0;
            let mut arms = String::new();
            for (arm, index) in selected.into_iter().enumerate() {
                let (pattern, mask) = &patterns[index];
                if mask & !covered == 0 {
                    unreachable += 1;
                }
                covered |= mask;
                arms.push_str(&format!("{pattern} => {{ return {arm}; }},"));
            }
            let text =
                format!("{declaration} fn f(value: {ty}) -> i64 {{ match value {{ {arms} }} }}");
            let (_, analysis) = Files::new(&text).check();
            assert_eq!(analysis.program.is_some(), covered == full, "{text}");
            assert_eq!(
                analysis
                    .diagnostics
                    .iter()
                    .filter(|d| d.code == codes::UNREACHABLE_ARM)
                    .count(),
                unreachable,
                "{text}"
            );
            assert_eq!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|d| d.code == codes::NON_EXHAUSTIVE_MATCH),
                covered != full,
                "{text}"
            );
        }
    }
}

#[test]
fn wide_boolean_products_use_the_iterative_search_without_enumerating_all_values() {
    let fields = (0..256)
        .map(|i| format!("f{i}: bool"))
        .collect::<Vec<_>>()
        .join(", ");
    let patterns = (0..256)
        .map(|i| format!("f{i}: _"))
        .collect::<Vec<_>>()
        .join(", ");
    let text = format!(
        "struct Wide {{ {fields} }} fn f(value: Wide) -> i64 {{ match value {{ Wide {{ {patterns} }} => {{ return 1; }}, }} }}"
    );
    Files::new(&text).pass();
}

#[test]
fn missing_witness_paths_are_valid_from_the_function_module() {
    let files = Files::new("mod m;");
    files.write("m.lang", "struct P { flag: bool } fn f(value: P) -> i64 { match value { P { flag: true } => { return 1; }, } }");
    let (_, analysis) = files.check();
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.notes == ["missing pattern: P { flag: false }"])
    );
    files.write("main.lang", "pub mod other; pub mod m;");
    files.write("other.lang", "pub struct P { flag: bool }");
    files.write("m.lang", "use root::other::P; fn f(value: P) -> i64 { match value { P { flag: true } => { return 1; }, } }");
    let (_, analysis) = files.check();
    let note = &analysis
        .diagnostics
        .iter()
        .find(|d| d.code == codes::NON_EXHAUSTIVE_MATCH)
        .unwrap()
        .notes[0];
    assert_eq!(note, "missing pattern: root::other::P { flag: false }");
    let witness = note.strip_prefix("missing pattern: ").unwrap();
    files.write("m.lang", &format!("use root::other::P; fn f(value: P) -> i64 {{ match value {{ P {{ flag: true }} => {{ return 1; }}, {witness} => {{ return 0; }}, }} }}"));
    files.pass();
}
