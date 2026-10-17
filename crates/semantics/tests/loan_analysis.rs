use lang_diagnostics::{SourceMap, codes};
use lang_hir::{
    loans::{self, Limits, ViolationKind as V},
    moves::{self, MoveCheckedBodies},
    program::{CheckError, CheckLimits, analyze_bodies, check_program},
};
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_virtual},
};
use std::path::Path;

fn prepare(text: &str) -> (SourceMap, MoveCheckedBodies) {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("loans.lang"),
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
    let checked = analyze_bodies(bound.program.unwrap(), &sources, CheckLimits::default()).unwrap();
    assert!(
        checked.bodies.is_some(),
        "{text}: {:?}",
        checked.diagnostics
    );
    let moved =
        moves::analyze(checked.bodies.unwrap(), &sources, moves::Limits::default()).unwrap();
    assert!(moved.bodies.is_some(), "{text}: {:?}", moved.diagnostics);
    (sources, moved.bodies.unwrap())
}
fn analyze(text: &str) -> (SourceMap, loans::Analysis) {
    let (sources, moved) = prepare(text);
    let result = loans::analyze(moved, &sources, Limits::default()).unwrap();
    for diagnostic in &result.diagnostics {
        diagnostic.validate(&sources).unwrap();
    }
    (sources, result)
}
fn accepted(text: &str) {
    let (_, checked) = analyze(text);
    assert!(checked.bodies.is_some(), "{text}: {:?}", checked.violations);
}
fn rejected(text: &str, kind: V) {
    let (_, checked) = analyze(text);
    assert!(checked.bodies.is_none(), "{text}");
    assert!(
        checked.violations.iter().any(|v| v.kind == kind),
        "{text}: {:?}",
        checked.violations
    );
}

#[test]
fn unused_named_read_loans_prevent_moves_and_keep_origin_identity() {
    let (sources, result) = analyze(
        "fn f(text: core::Text) -> core::Text { let mut read: &core::Text = &text; return text; }",
    );
    let v = result.violations[0];
    assert_eq!(v.kind, V::Conflict);
    assert_eq!(v.function.0, 0);
    assert_eq!(
        v.owner,
        Some(loans::Owner::Local(lang_hir::resolved::LocalId(0)))
    );
    assert_eq!(sources.slice(v.span).unwrap(), "text");
    assert_eq!(sources.slice(v.related.unwrap()).unwrap(), "&text");
    assert_eq!(result.diagnostics[0].code, codes::INVALID_LOAN);
    assert_eq!(result.diagnostics[0].secondary[0].span, v.related.unwrap());
}

#[test]
fn shared_loans_allow_reads_and_exclude_replacement_and_exclusive_access() {
    accepted(
        "fn f() -> i64 { let mut x: i64 = 1; let mut a: &i64 = &x; let mut b: &i64 = &x; return x + (*a) + (*b); }",
    );
    rejected(
        "fn f() -> i64 { let mut x: i64 = 1; let mut read: &i64 = &x; x = 2; return 0; }",
        V::Conflict,
    );
    rejected(
        "fn f() -> i64 { let mut x: i64 = 1; let mut read: &i64 = &x; let mut write: &mut i64 = &mut x; return 0; }",
        V::Conflict,
    );
    rejected(
        "fn f(read: &i64) -> i64 { let mut write: &mut i64 = &mut (*read); return 0; }",
        V::Immutable,
    );
}

#[test]
fn exclusive_loans_exclude_other_access_but_authorize_their_own_reads_and_writes() {
    accepted(
        "fn f() -> i64 { let mut x: i64 = 1; let mut write: &mut i64 = &mut x; (*write) = 2; return (*write); }",
    );
    rejected(
        "fn f() -> i64 { let mut x: i64 = 1; let mut write: &mut i64 = &mut x; return x; }",
        V::Conflict,
    );
    rejected(
        "fn f() -> i64 { let mut x: i64 = 1; let mut write: &mut i64 = &mut x; let mut read: &i64 = &x; return 0; }",
        V::Conflict,
    );
    rejected(
        "fn f() -> i64 { let mut x: i64 = 1; let mut write: &mut i64 = &mut x; x = 3; return 0; }",
        V::Conflict,
    );
}

#[test]
fn fields_share_owner_granularity_and_owning_parameters_are_immutable() {
    rejected(
        "struct R { a: i64, b: i64 } fn f(a: R) -> i64 { let mut r: R = a; let mut write: &mut i64 = &mut r.a; return r.b; }",
        V::Conflict,
    );
    rejected(
        "struct R { a: i64 } fn f(r: R) -> i64 { r.a = 2; return 0; }",
        V::Immutable,
    );
    rejected(
        "fn f(x: i64) -> i64 { let mut write: &mut i64 = &mut x; return 0; }",
        V::Immutable,
    );
    accepted("struct R { a: i64 } fn f(r: &mut R) -> i64 { r.a = 2; return r.a; }");
}

#[test]
fn call_loans_end_after_each_call_and_mutable_arguments_need_authority() {
    accepted(
        "fn inc(x: &mut i64) -> i64 { (*x) = (*x) + 1; return (*x); } fn f() -> i64 { let mut x: i64 = 1; let mut a: i64 = inc(x); return inc(&mut x) + a; }",
    );
    accepted(
        "fn inc(x: &mut i64) -> i64 { (*x) = 1; return (*x); } fn f(x: &mut i64) -> i64 { let mut a: i64 = inc(x); return inc(x) + a; }",
    );
    rejected(
        "fn inc(x: &mut i64) -> i64 { return 0; } fn f(x: i64) -> i64 { return inc(x); }",
        V::Immutable,
    );
    accepted(
        "fn read(x: &i64) -> i64 { return (*x); } fn f() -> i64 { let mut x: i64 = 1; let mut a: i64 = read(&x); x = 2; return a + read(x); }",
    );
}

#[test]
fn all_call_arguments_hold_their_loans_in_written_evaluation_order() {
    for args in ["&x, &mut x", "&mut x, &x"] {
        let signature = if args.starts_with("&x") {
            "a: &i64, b: &mut i64"
        } else {
            "a: &mut i64, b: &i64"
        };
        let text = format!(
            "fn both({signature}) -> i64 {{ return 0; }} fn f() -> i64 {{ let mut x: i64 = 1; return both({args}); }}"
        );
        let (_, result) = analyze(&text);
        let v = result.violations[0];
        assert_eq!(v.kind, V::Conflict);
        assert!(v.related.unwrap().range().start < v.span.range().start);
    }
    rejected(
        "fn take(a: &core::Text, b: core::Text) -> i64 { return 0; } fn f(text: core::Text) -> i64 { return take(text, text); }",
        V::Conflict,
    );
}

#[test]
fn shared_reborrows_allow_parent_reads_until_their_lexical_end() {
    accepted(
        "fn f(condition: bool) -> i64 { let mut x: i64 = 1; let mut parent: &mut i64 = &mut x; if condition { let mut child: &i64 = &(*parent); let mut seen: i64 = (*parent) + (*child); } (*parent) = 3; return (*parent); }",
    );
    rejected(
        "fn f() -> i64 { let mut x: i64 = 1; let mut parent: &mut i64 = &mut x; let mut child: &i64 = &(*parent); (*parent) = 2; return 0; }",
        V::FrozenParent,
    );
}

#[test]
fn exclusive_reborrows_freeze_parents_and_release_them_at_scope_exit() {
    rejected(
        "fn f() -> i64 { let mut x: i64 = 1; let mut parent: &mut i64 = &mut x; let mut child: &mut i64 = parent; return (*parent); }",
        V::FrozenParent,
    );
    accepted(
        "fn f(condition: bool) -> i64 { let mut x: i64 = 1; let mut parent: &mut i64 = &mut x; if condition { let mut child: &mut i64 = parent; (*child) = 2; } (*parent) = 3; return (*parent); }",
    );
}

#[test]
fn temporary_dereference_borrows_end_with_the_statement() {
    accepted(
        "fn f() -> i64 { let mut x: i64 = 1; let mut a: i64 = (*(&mut x)); x = 2; return a + x; }",
    );
    accepted("fn f(a: &i64, b: &i64) -> i64 { let mut read: &i64 = a; read = b; return (*read); }");
}

#[test]
fn rebinding_loan_variables_keeps_old_loans_without_accessing_the_old_referent() {
    accepted(
        "fn f(a: &i64, b: &i64) -> i64 { let mut parent: &i64 = a; let mut child: &i64 = parent; parent = b; return (*child) + (*parent); }",
    );
    accepted(
        "fn f() -> i64 { let mut x: i64 = 1; let mut y: i64 = 2; let mut parent: &mut i64 = &mut x; let mut child: &mut i64 = parent; parent = &mut y; (*child) = 3; (*parent) = 4; return (*child) + (*parent); }",
    );
    rejected(
        "fn f(a: &i64) -> i64 { let mut x: i64 = 1; let mut read: &i64 = &x; read = a; x = 2; return 0; }",
        V::Conflict,
    );
}

#[test]
fn loans_cannot_be_assigned_to_a_scope_outliving_the_owner_or_parent() {
    rejected(
        "fn f(a: &i64, condition: bool) -> i64 { let mut read: &i64 = a; if condition { let mut x: i64 = 1; read = &x; } return 0; }",
        V::Escape,
    );
    rejected(
        "fn f(a: &i64, condition: bool) -> i64 { let mut x: i64 = 1; let mut read: &i64 = a; if condition { let mut inner: &i64 = &x; read = inner; } return 0; }",
        V::Escape,
    );
}

#[test]
fn branch_joins_keep_reaching_aliases_and_discard_returning_paths() {
    rejected(
        "fn f(a: &i64, condition: bool) -> i64 { let mut x: i64 = 1; let mut read: &i64 = a; if condition { read = &x; } x = 2; return 0; }",
        V::Conflict,
    );
    accepted(
        "fn f(a: &i64, condition: bool) -> i64 { let mut x: i64 = 1; let mut read: &i64 = a; if condition { read = &x; return 0; } x = 2; return x; }",
    );
}

#[test]
fn loop_local_loans_end_but_persistent_loan_changes_fail_the_backedge() {
    accepted(
        "fn f(condition: bool) -> i64 { let mut x: i64 = 1; while condition { let mut write: &mut i64 = &mut x; (*write) = 2; } return x; }",
    );
    rejected(
        "fn f(a: &i64, condition: bool) -> i64 { let mut x: i64 = 1; let mut read: &i64 = a; while condition { read = &x; } return 0; }",
        V::LoopState,
    );
}

#[test]
fn noncopy_matches_hold_a_read_loan_and_end_it_after_the_match() {
    rejected(
        "struct R { text: core::Text, n: i64 } fn f(a: R) -> i64 { let mut r: R = a; match r { R { text: _, n: _ } => { r.n = 1; }, } return 0; }",
        V::Conflict,
    );
    accepted("fn f(text: core::Text) -> core::Text { match text { _ => {}, } return text; }");
    accepted(
        "struct R { text: core::Text, n: i64 } fn f(r: R) -> i64 { match r { R { text: read, n: n } => { return core::text_len(read) + n; }, } }",
    );
}

#[test]
fn provisional_core_reads_block_mutation_only_in_bound_success_arms() {
    rejected(
        "fn f(v: &mut core::Vec<i64>) -> i64 { match core::vec_get::<i64>(v, 0) { core::Result::Ok(read) => { let mut pushed: core::Result<core::Unit, core::AllocError> = core::vec_push::<i64>(v, 1); return (*read); }, core::Result::Err(_) => { return 0; }, } }",
        V::FrozenParent,
    );
    accepted(
        "fn f(v: &mut core::Vec<i64>) -> i64 { match core::vec_get::<i64>(v, 0) { core::Result::Ok(_) => { let mut pushed: core::Result<core::Unit, core::AllocError> = core::vec_push::<i64>(v, 1); return 1; }, core::Result::Err(_) => { let mut failed: core::Result<core::Unit, core::AllocError> = core::vec_push::<i64>(v, 2); return 0; }, } }",
    );
}

#[test]
fn allocator_and_arena_read_authority_have_lexical_duration() {
    accepted(include_str!("fixtures/core-binding.lang"));
    accepted(include_str!("fixtures/arena-body.lang"));
    rejected(
        "fn f(a: &core::Allocator, node: arena i64) -> i64 { arena graph(a) { match core::arena_read::<i64>(graph, node) { core::Result::Ok(read) => { let mut r: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<i64>(graph, node, 3); return (*read); }, core::Result::Err(_) => { return 0; }, } } }",
        V::Conflict,
    );
}

#[test]
fn dead_statements_and_redundant_match_arms_have_no_loan_effects() {
    accepted(
        "fn f(x: core::Text, key: bool) -> i64 { match key { _ => { return 0; }, false => { let mut a: &core::Text = &x; let mut b: core::Text = x; }, } }",
    );
    accepted(
        "fn f(x: core::Text) -> i64 { return 0; let mut a: &core::Text = &x; let mut b: core::Text = x; }",
    );
}

#[test]
fn nested_reference_cells_have_checked_provenance_without_executable_promotion() {
    let (sources, moved) = prepare("fn f(x: &&i64) -> i64 { return (*(*x)); }");
    let source = moved.bodies().source().clone();
    let result = loans::analyze(moved, &sources, Limits::default()).unwrap();
    assert!(result.bodies.is_some());
    assert!(result.diagnostics.is_empty());
    assert!(matches!(
        check_program(source, &sources, CheckLimits::default()),
        Err(CheckError::OwnershipProfile { .. })
    ));
}

#[test]
fn nested_parameter_paths_preserve_read_and_write_permissions() {
    accepted("fn f(p: &mut &mut i64) -> i64 { (*(*p)) = (*(*p)) + 1; return (*(*p)); }");
    accepted(
        "fn f(p: &mut &mut &mut i64) -> i64 { (*(*(*p))) = (*(*(*p))) + 1; return (*(*(*p))); }",
    );
    rejected(
        "fn f(p: &&mut i64) -> i64 { (*(*p)) = 2; return 0; }",
        V::Immutable,
    );
    rejected(
        "fn f(p: &mut &i64) -> i64 { (*(*p)) = 2; return 0; }",
        V::Immutable,
    );
    rejected(
        "fn f(p: &&mut i64) -> i64 { let mut write: &mut i64 = (*p); return 0; }",
        V::Immutable,
    );
}

#[test]
fn local_reference_cell_updates_publish_new_provenance() {
    accepted(
        "fn f() -> i64 { let mut x: i64 = 1; let mut y: i64 = 2; let mut read: &i64 = &x; let mut cell: &mut &i64 = &mut read; (*cell) = &y; return (*(*cell)); }",
    );
    rejected(
        "fn f(condition: bool) -> i64 { let mut x: i64 = 1; let mut y: i64 = 2; let mut read: &i64 = &x; if condition { let mut cell: &mut &i64 = &mut read; (*cell) = &y; } y = 3; return 0; }",
        V::Conflict,
    );
    rejected(
        "fn f() -> i64 { let mut x: i64 = 1; let mut y: i64 = 2; let mut read: &i64 = &x; let mut cell: &mut &i64 = &mut read; (*cell) = &y; return (*read); }",
        V::Conflict,
    );
}

#[test]
fn loans_to_reference_cells_prevent_direct_rebinding() {
    rejected(
        "fn f(a: &i64, b: &i64) -> i64 { let mut read: &i64 = a; let mut cell: &&i64 = &read; read = b; return 0; }",
        V::Conflict,
    );
    accepted(
        "fn f(a: &i64) -> i64 { let mut read: &i64 = a; let mut cell: &&i64 = &read; return (*read) + (*(*cell)); }",
    );
}

#[test]
fn loaded_loans_freeze_the_cell_path_for_their_lexical_duration() {
    rejected(
        "fn f() -> i64 { let mut x: i64 = 1; let mut y: i64 = 2; let mut read: &i64 = &x; let mut cell: &mut &i64 = &mut read; let mut child: &i64 = (*cell); (*cell) = &y; return (*child); }",
        V::FrozenParent,
    );
    accepted(
        "fn f(condition: bool) -> i64 { let mut x: i64 = 1; let mut y: i64 = 2; let mut read: &i64 = &x; let mut cell: &mut &i64 = &mut read; if condition { let mut child: &i64 = (*cell); } (*cell) = &y; return (*(*cell)); }",
    );
}

#[test]
fn nested_arguments_can_infer_cell_borrows_and_reborrow_mutable_contents() {
    accepted(
        "fn read(p: &&i64) -> i64 { return (*(*p)); } fn f(a: &i64) -> i64 { let mut value: &i64 = a; return read(value) + read(&value); }",
    );
    accepted(
        "fn inc(p: &mut &mut i64) -> i64 { (*(*p)) = (*(*p)) + 1; return (*(*p)); } fn f() -> i64 { let mut x: i64 = 1; let mut value: &mut i64 = &mut x; let mut first: i64 = inc(value); return inc(&mut value) + first; }",
    );
}

#[test]
fn nested_call_arguments_reserve_reachable_owners_and_detect_aliases() {
    rejected(
        "fn read(p: &&mut i64) -> i64 { return (*(*p)); } fn f() -> i64 { let mut x: i64 = 1; let mut parent: &mut i64 = &mut x; let mut child: &mut i64 = parent; return read(parent); }",
        V::FrozenParent,
    );
    rejected(
        "fn both(a: &mut &mut i64, b: &mut i64) -> i64 { return 0; } fn f() -> i64 { let mut x: i64 = 1; let mut value: &mut i64 = &mut x; return both(value, value); }",
        V::Conflict,
    );
    accepted(
        "fn both(a: &&i64, b: &&i64) -> i64 { return (*(*a)) + (*(*b)); } fn f() -> i64 { let mut x: i64 = 1; let mut left: &i64 = &x; let mut right: &i64 = &x; return both(left, right); }",
    );
}

#[test]
fn user_functions_cannot_export_ordinary_loans_through_caller_owned_cells() {
    let (sources, result) =
        analyze("fn f(cell: &mut &i64, value: &i64) -> i64 { (*cell) = value; return 0; }");
    assert!(result.bodies.is_none());
    let v = result
        .violations
        .iter()
        .find(|v| v.kind == V::CallerCellEscape)
        .unwrap();
    assert_eq!(sources.slice(v.span).unwrap(), "value");
    assert_eq!(result.diagnostics[0].code, codes::REGION_ESCAPE);
    rejected(
        "fn f(cell: &mut &mut &i64, value: &i64) -> i64 { (*(*cell)) = value; return 0; }",
        V::CallerCellEscape,
    );
}

#[test]
fn nested_loads_and_stores_cannot_outlive_their_cells_or_owners() {
    rejected(
        "fn f(a: &i64, condition: bool) -> i64 { let mut read: &i64 = a; let mut cell: &mut &i64 = &mut read; if condition { let mut short: i64 = 1; (*cell) = &short; } return 0; }",
        V::Escape,
    );
    rejected(
        "fn f(a: &i64, condition: bool) -> i64 { let mut read: &i64 = a; let mut result: &i64 = a; if condition { let mut cell: &&i64 = &read; result = (*cell); } return 0; }",
        V::Escape,
    );
}

#[test]
fn provisional_reads_preserve_nested_cell_guards_after_call_temporaries_end() {
    rejected(
        "fn f(a: core::Vec<i64>, b: core::Vec<i64>) -> i64 { let mut x: core::Vec<i64> = a; let mut y: core::Vec<i64> = b; let mut value: &mut core::Vec<i64> = &mut x; let mut cell: &mut &mut core::Vec<i64> = &mut value; match core::vec_get::<i64>((*cell), 0) { core::Result::Ok(read) => { (*cell) = &mut y; return (*read); }, core::Result::Err(_) => { return 0; }, } }",
        V::FrozenParent,
    );
    accepted(
        "fn f(a: core::Vec<i64>, b: core::Vec<i64>) -> i64 { let mut x: core::Vec<i64> = a; let mut y: core::Vec<i64> = b; let mut value: &mut core::Vec<i64> = &mut x; let mut cell: &mut &mut core::Vec<i64> = &mut value; match core::vec_get::<i64>((*cell), 0) { core::Result::Ok(_) => { (*cell) = &mut y; return 1; }, core::Result::Err(_) => { return 0; }, } }",
    );
}

#[test]
fn nested_dependency_graphs_and_budgets_are_validated_independently() {
    let (sources, moved) = prepare(
        "fn f(p: &mut &mut &mut i64) -> i64 { let mut read: &i64 = &(*(*(*p))); return (*read); }",
    );
    let baseline = loans::analyze(moved.clone(), &sources, Limits::default())
        .unwrap()
        .bodies
        .unwrap();
    let f = &baseline.functions()[0];
    assert!(f.loans().iter().any(|l| l.projection));
    assert!(
        f.loans()
            .iter()
            .enumerate()
            .any(|(i, _)| f.dependencies(loans::LoanId(i)).unwrap().len() >= 3)
    );
    for (index, loan) in f.loans().iter().enumerate() {
        let dependencies = f.dependencies(loans::LoanId(index)).unwrap();
        assert!(dependencies.windows(2).all(|w| w[0] < w[1]));
        assert!(dependencies.iter().all(|id| id.0 < index));
        if let Some(parent) = loan.parent {
            assert!(dependencies.contains(&parent));
        }
    }
    assert!(f.dependencies(loans::LoanId(f.loans().len())).is_none());
    let exact = Limits {
        work: baseline.work_used(),
        allocations: baseline.requested_bytes(),
        loans: f.loans().len(),
        ..Limits::default()
    };
    assert!(
        loans::analyze(moved.clone(), &sources, exact)
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
            loans: exact.loans - 1,
            ..exact
        },
        Limits { depth: 2, ..exact },
    ] {
        let Err(loans::Error::Limit(d)) = loans::analyze(moved.clone(), &sources, limits) else {
            panic!("{limits:?}");
        };
        d.validate(&sources).unwrap();
    }
}

#[test]
fn mutable_core_projections_keep_exclusive_authority() {
    accepted(
        "fn f(v: &mut core::Vec<i64>) -> i64 { match core::vec_get_mut::<i64>(v, 0) { core::Result::Ok(cell) => { (*cell) = 3; return (*cell); }, core::Result::Err(_) => { return 0; }, } }",
    );
    rejected(
        "fn f(v: &mut core::Vec<i64>) -> i64 { match core::vec_get_mut::<i64>(v, 0) { core::Result::Ok(cell) => { return core::vec_len::<i64>(v); }, core::Result::Err(_) => { return 0; }, } }",
        V::FrozenParent,
    );
}

#[test]
fn generated_nested_paths_agree_with_a_permission_intersection_oracle() {
    let mut successful_writes = 0;
    for depth in 1..=5 {
        for mask in 0..(1 << depth) {
            let mut ty = "i64".to_owned();
            let mut expression = "p".to_owned();
            let mut mutable = true;
            for level in 0..depth {
                let exclusive = mask & (1 << level) != 0;
                ty = format!("&{}{}", if exclusive { "mut " } else { "" }, ty);
                expression = format!("(*{expression})");
                mutable &= exclusive;
            }
            accepted(&format!("fn f(p: {ty}) -> i64 {{ return {expression}; }}"));
            let text =
                format!("fn f(p: {ty}) -> i64 {{ {expression} = {expression} + 1; return 0; }}");
            let (_, result) = analyze(&text);
            assert_eq!(
                result.bodies.is_some(),
                mutable,
                "{text}: {:?}",
                result.violations
            );
            successful_writes += usize::from(result.bodies.is_some());
        }
    }
    assert_eq!(successful_writes, 5);
}

#[test]
fn frontend_reports_loans_including_forms_without_constructed_annotations() {
    for text in [
        "fn f(text: core::Text) -> core::Text { let mut read: &core::Text = &text; return text; }",
        "struct R { a: i64 } fn f(r: R) -> i64 { r.a = 2; return 0; }",
    ] {
        let (sources, moved) = prepare(text);
        let checked = check_program(
            moved.bodies().source().clone(),
            &sources,
            CheckLimits::default(),
        )
        .unwrap();
        assert!(checked.program.is_none());
        assert_eq!(checked.diagnostics[0].code, codes::INVALID_LOAN);
    }
}

#[test]
fn limits_are_exact_and_source_snapshots_must_match() {
    let (sources, moved) = prepare(
        "fn f(a: &i64, condition: bool) -> i64 { let mut x: i64 = 1; if condition { let mut read: &i64 = &x; } return (*a); }",
    );
    let baseline = loans::analyze(moved.clone(), &sources, Limits::default())
        .unwrap()
        .bodies
        .unwrap();
    let count = baseline.functions().iter().map(|f| f.loans().len()).sum();
    let exact = Limits {
        functions: 1,
        locals: 4,
        loans: count,
        work: baseline.work_used(),
        allocations: baseline.requested_bytes(),
        ..Limits::default()
    };
    assert!(
        loans::analyze(moved.clone(), &sources, exact)
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
        Limits {
            loans: count - 1,
            ..exact
        },
        Limits { depth: 0, ..exact },
    ] {
        let Err(loans::Error::Limit(d)) = loans::analyze(moved.clone(), &sources, limits) else {
            panic!("{limits:?}");
        };
        assert_eq!(d.code, codes::RESOURCE_LIMIT);
        d.validate(&sources).unwrap();
    }
    for f in baseline.functions() {
        for (index, loan) in f.loans().iter().enumerate() {
            if let Some(parent) = loan.parent {
                assert!(parent.0 < index);
            }
            sources.slice(loan.span).unwrap();
        }
    }
    let mut wrong = SourceMap::new();
    wrong.add("wrong", "").unwrap();
    assert!(matches!(
        loans::analyze(moved, &wrong, exact),
        Err(loans::Error::Source(_))
    ));
    let (sources, moved) =
        prepare("fn f(x: core::Text) -> core::Text { let mut read: &core::Text = &x; return x; }");
    assert!(matches!(
        loans::analyze(
            moved,
            &sources,
            Limits {
                diagnostics: 0,
                ..Limits::default()
            }
        ),
        Err(loans::Error::Limit(_))
    ));
}

#[test]
fn generated_access_sequences_agree_with_an_independent_permission_oracle() {
    let mut successful = 0;
    for sequence in 0..64 {
        let mut text = String::from("fn f() -> i64 { let mut x: i64 = 1; ");
        let (mut readers, mut writer, mut valid) = (0, false, true);
        for step in 0..3 {
            let op = (sequence >> (step * 2)) & 3;
            let allowed = match op {
                0 => !writer,
                1 => !writer,
                2 => !writer && readers == 0,
                _ => !writer && readers == 0,
            };
            valid &= allowed;
            match op {
                0 => text.push_str(&format!("let mut n{step}: i64 = x; ")),
                1 => {
                    text.push_str(&format!("let mut r{step}: &i64 = &x; "));
                    readers += 1;
                }
                2 => {
                    text.push_str(&format!("let mut r{step}: &mut i64 = &mut x; "));
                    writer = true;
                }
                _ => text.push_str("x = 2; "),
            }
        }
        text.push_str("return 0; }");
        let (_, result) = analyze(&text);
        assert_eq!(
            result.bodies.is_some(),
            valid,
            "{text}: {:?}",
            result.violations
        );
        successful += usize::from(valid);
    }
    assert!(successful > 0 && successful < 64);
}
