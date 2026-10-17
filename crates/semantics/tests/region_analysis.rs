use lang_diagnostics::{SourceMap, codes};
use lang_hir::{
    loans::{self, LoanCheckedBodies},
    moves,
    program::{CheckLimits, analyze_bodies},
    regions::{self, BrandKind, Limits, ViolationKind as V},
};
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_virtual},
};
use std::path::Path;

fn prepare(text: &str) -> (SourceMap, LoanCheckedBodies) {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("regions.lang"),
        text.as_bytes(),
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let bound = bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
    assert!(
        bound.diagnostics.is_empty(),
        "{text}: {:?}",
        bound.diagnostics
    );
    let typed = analyze_bodies(bound.program.unwrap(), &sources, CheckLimits::default()).unwrap();
    assert!(typed.bodies.is_some(), "{text}: {:?}", typed.diagnostics);
    let moved = moves::analyze(typed.bodies.unwrap(), &sources, Default::default()).unwrap();
    assert!(moved.bodies.is_some(), "{text}: {:?}", moved.diagnostics);
    let loaned = loans::analyze(moved.bodies.unwrap(), &sources, Default::default()).unwrap();
    assert!(loaned.bodies.is_some(), "{text}: {:?}", loaned.diagnostics);
    (sources, loaned.bodies.unwrap())
}
fn analyze(text: &str) -> (SourceMap, regions::Analysis) {
    let (sources, loaned) = prepare(text);
    let analysis = regions::analyze(loaned, &sources, Default::default()).unwrap();
    for d in &analysis.diagnostics {
        d.validate(&sources).unwrap();
    }
    (sources, analysis)
}
fn accepted(text: &str) {
    let (_, a) = analyze(text);
    assert!(
        a.bodies.is_some(),
        "{text}: {:?} {:?}",
        a.violations,
        a.pending
    );
}
fn rejected(text: &str, kind: V) {
    let (_, a) = analyze(text);
    assert!(a.bodies.is_none(), "{text}");
    assert!(
        a.violations.iter().any(|v| v.kind == kind),
        "{text}: {:?} {:?}",
        a.violations,
        a.pending
    );
}
fn two_nodes(middle: &str, body: &str) -> String {
    format!(
        "fn f(a: &core::Allocator, condition: bool) -> i64 {{ arena one(a) {{ let mut made1: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(one, 1); match made1 {{ core::Result::Ok(node1) => {{ {middle} arena two(a) {{ let mut made2: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(two, 2); match made2 {{ core::Result::Ok(node2) => {{ {body} }}, core::Result::Err(_) => {{ return 0; }}, }} }} }}, core::Result::Err(_) => {{ return 0; }}, }} }} return 0; }}"
    )
}
const ESCAPE: &str = "fn f(a: &core::Allocator) -> i64 { let mut outside: core::Option<arena i64> = core::Option::None; arena graph(a) { let mut made: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(graph, 7); match made { core::Result::Ok(token) => { outside = core::Option::Some(token); }, core::Result::Err(_) => {}, } } return 0; }";

#[test]
fn same_region_cycles_reads_and_replacements_keep_source_brand_facts() {
    let text = "struct Node { next: core::Option<arena Node> } fn f(a: &core::Allocator) -> i64 { arena graph(a) { let mut made: core::Result<arena Node, core::ArenaError> = core::arena_new::<Node>(graph, Node { next: core::Option::None }); match made { core::Result::Ok(token) => { let mut replaced: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<Node>(graph, token, Node { next: core::Option::Some(token) }); return 1; }, core::Result::Err(_) => { return 0; }, } } }";
    let (sources, a) = analyze(text);
    let checked = a.bodies.unwrap();
    let facts = &checked.functions()[0];
    assert_eq!(facts.brands().len(), 1);
    assert!(matches!(facts.brands()[0].kind, BrandKind::Local(_)));
    assert_eq!(sources.slice(facts.brands()[0].span).unwrap(), "graph");
    let source = checked.loaned().moved().bodies().source();
    for (id, expression) in source.functions[0].expressions.iter().enumerate() {
        if matches!(
            expression.kind,
            lang_hir::resolved::ExpressionKind::CoreCall {
                operation: lang_hir::core::Operation::ArenaNew,
                ..
            }
        ) {
            assert_eq!(
                facts
                    .expression_brands(lang_hir::resolved::ExpressionId(id))
                    .unwrap(),
                &[regions::BrandId(0)]
            );
        }
    }
    accepted(include_str!("fixtures/arena-body.lang"));
}

#[test]
fn escaping_options_report_the_arena_declaration_and_store() {
    let (sources, a) = analyze(ESCAPE);
    let v = a.violations[0];
    assert_eq!(v.kind, V::Escape);
    assert_eq!(v.function.0, 0);
    assert_eq!(sources.slice(v.brand.span).unwrap(), "graph");
    assert!(sources.slice(v.span).unwrap().starts_with("outside ="));
    assert_eq!(a.diagnostics[0].code, codes::REGION_ESCAPE);
    assert_eq!(a.diagnostics[0].secondary[0].span, v.brand.span);
}

#[test]
fn distinct_arena_read_replace_and_payload_storage_are_rejected() {
    for body in [
        "match core::arena_read::<i64>(one, node2) { core::Result::Ok(read) => { return (*read); }, core::Result::Err(_) => { return 0; }, }",
        "let mut replaced: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<i64>(one, node2, 3); return 0;",
    ] {
        rejected(&two_nodes("", body), V::CrossArena);
    }
    let text = format!(
        "struct Holder {{ token: arena i64 }} {}",
        two_nodes(
            "",
            "let mut made3: core::Result<arena Holder, core::ArenaError> = core::arena_new::<Holder>(one, Holder { token: node2 }); return 0;"
        )
    );
    rejected(&text, V::CrossArena);
}

#[test]
fn caller_provenance_is_opaque_and_does_not_authorize_a_fresh_arena() {
    rejected(
        "fn f(a: &core::Allocator, token: arena i64) -> i64 { arena graph(a) { match core::arena_read::<i64>(graph, token) { core::Result::Ok(read) => { return (*read); }, core::Result::Err(_) => { return 0; }, } } }",
        V::CrossArena,
    );
    let (_, a) =
        analyze("fn f(token: arena i64) -> i64 { let mut copy: arena i64 = token; return 0; }");
    assert!(matches!(
        a.bodies.unwrap().functions()[0].brands()[0].kind,
        BrandKind::Caller { .. }
    ));
}

#[test]
fn field_projections_keep_outer_and_inner_brands_separate() {
    for (field, valid) in [("left", true), ("right", false)] {
        let text = format!(
            "struct Pair {{ left: arena i64, right: arena i64 }} {}",
            two_nodes(
                "let mut outside: core::Option<arena i64> = core::Option::None;",
                &format!(
                    "let mut pair: Pair = Pair {{ right: node2, left: node1 }}; outside = core::Option::Some(pair.{field}); return 0;"
                )
            )
        );
        let (_, a) = analyze(&text);
        assert_eq!(a.bodies.is_some(), valid, "{text}: {:?}", a.violations);
    }
}

#[test]
fn record_patterns_preserve_field_provenance() {
    let text = format!(
        "struct Pair {{ left: arena i64, right: arena i64 }} {}",
        two_nodes(
            "let mut outside: core::Option<arena i64> = core::Option::None;",
            "let mut pair: Pair = Pair { left: node1, right: node2 }; match pair { Pair { right: inner, left: outer } => { outside = core::Option::Some(outer); return 0; }, }"
        )
    );
    accepted(&text);
}

#[test]
fn record_field_and_loan_alias_stores_use_the_destination_owner_lifetime() {
    for destination in ["outside.token", "alias.token"] {
        let alias = if destination.starts_with("alias") {
            "let mut alias: &mut Holder = &mut outside;"
        } else {
            ""
        };
        let text = format!(
            "struct Holder {{ token: core::Option<arena i64> }} fn f(a: &core::Allocator) -> i64 {{ let mut outside: Holder = Holder {{ token: core::Option::None }}; {alias} arena graph(a) {{ let mut made: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(graph, 7); match made {{ core::Result::Ok(token) => {{ {destination} = core::Option::Some(token); }}, core::Result::Err(_) => {{}}, }} }} return 0; }}"
        );
        rejected(&text, V::Escape);
    }
}

#[test]
fn collection_growth_rejects_tokens_that_outlive_the_destination() {
    for (ty, operation) in [
        (
            "core::Vec<arena i64>",
            "core::vec_push::<arena i64>(values, token)",
        ),
        (
            "core::Map<i64, arena i64>",
            "core::map_insert::<i64, arena i64>(values, 1, token)",
        ),
    ] {
        let result = if ty.contains("Vec") {
            "core::Result<core::Unit, core::AllocError>"
        } else {
            "core::Result<core::Option<arena i64>, core::AllocError>"
        };
        let text = format!(
            "fn f(a: &core::Allocator, original: {ty}) -> i64 {{ let mut values: {ty} = original; arena graph(a) {{ let mut made: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(graph, 7); match made {{ core::Result::Ok(token) => {{ let mut result: {result} = {operation}; return 0; }}, core::Result::Err(_) => {{ return 0; }}, }} }} }}"
        );
        rejected(&text, V::Escape);
    }
}

#[test]
fn an_ordinary_collection_inside_the_region_can_hold_its_tokens() {
    accepted(
        "fn f(a: &core::Allocator, original: core::Vec<arena i64>) -> i64 { arena graph(a) { let mut values: core::Vec<arena i64> = original; let mut made: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(graph, 7); match made { core::Result::Ok(token) => { let mut result: core::Result<core::Unit, core::AllocError> = core::vec_push::<arena i64>(values, token); return core::vec_len::<arena i64>(values); }, core::Result::Err(_) => { return 0; }, } } }",
    );
}

#[test]
fn writes_through_mutable_collection_projections_preserve_region_bounds() {
    rejected(
        "fn f(a: &core::Allocator, values: &mut core::Vec<arena i64>) -> i64 { arena graph(a) { let mut made: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(graph, 7); match made { core::Result::Ok(token) => { match core::vec_get_mut::<arena i64>(values, 0) { core::Result::Ok(cell) => { (*cell) = token; return 0; }, core::Result::Err(_) => { return 0; }, } }, core::Result::Err(_) => { return 0; }, } } }",
        V::Escape,
    );
}

#[test]
fn loop_fixed_points_check_values_reaching_later_iterations() {
    let body = "let mut saved: core::Option<arena i64> = core::Option::None; while condition { match saved { core::Option::Some(old) => { let mut made3: core::Result<arena Holder, core::ArenaError> = core::arena_new::<Holder>(one, Holder { token: old }); }, core::Option::None => {}, } saved = core::Option::Some(node2); } return 0;";
    rejected(
        &format!(
            "struct Holder {{ token: arena i64 }} {}",
            two_nodes("", body)
        ),
        V::CrossArena,
    );
}

#[test]
fn unreachable_region_effects_do_not_poison_facts() {
    accepted(&two_nodes(
        "",
        "match condition { _ => { return 0; }, false => { let mut bad: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<i64>(one, node2, 1); return 0; }, }",
    ));
}

#[test]
fn guest_region_calls_remain_an_explicit_interprocedural_gate() {
    let text = "fn inspect(token: arena i64) -> i64 { return 0; } fn f(token: arena i64) -> i64 { return inspect(token); }";
    let (_, a) = analyze(text);
    assert!(a.bodies.is_none());
    assert!(a.diagnostics.is_empty());
    assert_eq!(a.pending.len(), 1);
    assert_eq!(a.pending[0].function.0, 1);
    assert_eq!(a.pending[0].callee.0, 0);
    accepted("fn id(value: i64) -> i64 { return value; } fn f() -> i64 { return id(7); }");
}

#[test]
fn region_resource_limits_and_snapshot_identity_are_enforced() {
    let (sources, loaned) = prepare(include_str!("fixtures/arena-body.lang"));
    let baseline = regions::analyze(loaned.clone(), &sources, Default::default())
        .unwrap()
        .bodies
        .unwrap();
    let exact = Limits {
        work: baseline.work_used(),
        allocations: baseline.requested_bytes(),
        ..Limits::default()
    };
    assert!(
        regions::analyze(loaned.clone(), &sources, exact)
            .unwrap()
            .bodies
            .is_some()
    );
    for limits in [
        Limits {
            work: exact.work - 1,
            ..exact
        },
        Limits {
            allocations: exact.allocations - 1,
            ..exact
        },
        Limits {
            functions: 0,
            ..exact
        },
        Limits { locals: 0, ..exact },
        Limits { values: 0, ..exact },
        Limits { places: 0, ..exact },
        Limits { depth: 0, ..exact },
    ] {
        let Err(regions::Error::Limit(d)) = regions::analyze(loaned.clone(), &sources, limits)
        else {
            panic!("{limits:?}")
        };
        d.validate(&sources).unwrap();
    }
    let mut wrong = SourceMap::new();
    wrong.add("wrong", "").unwrap();
    assert!(matches!(
        regions::analyze(loaned, &wrong, exact),
        Err(regions::Error::Source(_))
    ));
    let (sources, loaned) = prepare(ESCAPE);
    assert!(matches!(
        regions::analyze(
            loaned,
            &sources,
            Limits {
                diagnostics: 0,
                ..Limits::default()
            }
        ),
        Err(regions::Error::Limit(_))
    ));
}

#[test]
fn generated_lexical_stores_agree_with_an_independent_scope_oracle() {
    let mut successful = 0;
    for count in 1..=3 {
        for sink in 0..=count {
            for origin in 0..count {
                let mut text = String::from("fn f(a: &core::Allocator) -> i64 { ");
                if sink == 0 {
                    text.push_str(
                        "let mut outside: core::Option<arena i64> = core::Option::None; ",
                    );
                }
                for level in 0..count {
                    text.push_str(&format!("arena graph{level}(a) {{ "));
                    if sink == level + 1 {
                        text.push_str(
                            "let mut outside: core::Option<arena i64> = core::Option::None; ",
                        );
                    }
                    text.push_str(&format!("let mut made{level}: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(graph{level}, 1); match made{level} {{ core::Result::Ok(token{level}) => {{ "));
                }
                text.push_str(&format!(
                    "outside = core::Option::Some(token{origin}); return 0; "
                ));
                for _ in 0..count {
                    text.push_str("}, core::Result::Err(_) => { return 0; }, } } ");
                }
                text.push('}');
                let (_, a) = analyze(&text);
                let valid = sink > origin;
                assert_eq!(a.bodies.is_some(), valid, "{text}: {:?}", a.violations);
                successful += usize::from(valid);
            }
        }
    }
    assert_eq!(successful, 10);
}
