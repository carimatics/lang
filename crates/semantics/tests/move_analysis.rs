use lang_diagnostics::{SourceMap, codes};
use lang_hir::{
    moves::{self, AccessKind, Limits, ViolationKind as V},
    program::{BodyTypes, CheckError, CheckLimits, analyze_bodies, check_program},
};
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_virtual},
};
use std::path::Path;

fn typed(text: &str) -> (SourceMap, BodyTypes) {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("moves.lang"),
        text.as_bytes(),
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let bound = bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
    assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
    let checked = analyze_bodies(bound.program.unwrap(), &sources, CheckLimits::default()).unwrap();
    assert!(
        checked.bodies.is_some(),
        "{text}: {:?}",
        checked.diagnostics
    );
    (sources, checked.bodies.unwrap())
}
fn analyze(text: &str) -> (SourceMap, moves::Analysis) {
    let (sources, bodies) = typed(text);
    let analysis = moves::analyze(bodies, &sources, Limits::default()).unwrap();
    for d in &analysis.diagnostics {
        d.validate(&sources).unwrap();
    }
    (sources, analysis)
}
fn rejected(text: &str, kind: V) {
    let (_, analysis) = analyze(text);
    assert!(analysis.bodies.is_none(), "{text}");
    assert!(
        analysis.violations.iter().any(|v| v.kind == kind),
        "{text}: {:?}",
        analysis.violations
    );
}

#[test]
fn whole_value_moves_report_the_original_move_and_later_use() {
    let text = "fn take(value: core::Text) -> core::Text { return value; } fn f(text: core::Text) -> i64 { let mut moved: core::Text = take(text); return core::text_len(text); }";
    let (sources, analysis) = analyze(text);
    let violation = &analysis.violations[0];
    assert_eq!(violation.kind, V::Moved);
    assert_eq!(violation.function.0, 1);
    assert_eq!(violation.local.unwrap().0, 0);
    assert_eq!(sources.slice(violation.span).unwrap(), "text");
    assert_eq!(sources.slice(violation.origin.unwrap()).unwrap(), "text");
    assert!(violation.origin.unwrap().range().start < violation.span.range().start);
    assert_eq!(analysis.diagnostics[0].code, codes::INVALID_OWNER_USE);
    assert_eq!(
        analysis.diagnostics[0].secondary[0].span,
        violation.origin.unwrap()
    );
}

#[test]
fn mutable_whole_locals_can_be_reinitialized_after_a_move_or_self_consumption() {
    for text in [
        "fn f(a: core::Text, b: core::Text) -> i64 { let mut text: core::Text = a; let mut moved: core::Text = text; text = b; return core::text_len(text); }",
        "fn id(x: core::Text) -> core::Text { return x; } fn f(a: core::Text) -> i64 { let mut text: core::Text = a; text = id(text); return core::text_len(text); }",
    ] {
        let (_, analysis) = analyze(text);
        assert!(analysis.bodies.is_some(), "{:?}", analysis.violations);
    }
}

#[test]
fn copy_values_loans_and_inferred_call_borrows_are_not_consumed() {
    for text in [
        "struct R { value: i64 } fn f(a: R) -> i64 { let mut b: R = a; return a.value + b.value; }",
        "fn length(text: &core::Text) -> i64 { return core::text_len(text); } fn f(text: core::Text) -> i64 { let mut length_a: i64 = length(text); return length_a + length(text); }",
        include_str!("fixtures/core-binding.lang"),
        include_str!("fixtures/arena-body.lang"),
    ] {
        let (_, analysis) = analyze(text);
        assert!(
            analysis.bodies.is_some(),
            "{text}: {:?}",
            analysis.violations
        );
    }
    let (_, analysis) = analyze(
        "fn f(text: core::Text) -> i64 { let mut a: i64 = core::text_len(text); return a + core::text_len(text); }",
    );
    assert!(
        analysis.bodies.unwrap().functions()[0]
            .accesses()
            .iter()
            .all(|a| a.kind == AccessKind::Read)
    );
}

#[test]
fn branches_merge_only_states_on_paths_reaching_the_join() {
    rejected(
        "fn f(text: core::Text, condition: bool) -> i64 { if condition { let mut moved: core::Text = text; } return core::text_len(text); }",
        V::MaybeMoved,
    );
    let (_, analysis) = analyze(
        "fn f(text: core::Text, condition: bool) -> i64 { if condition { let mut moved: core::Text = text; return 1; } return core::text_len(text); }",
    );
    assert!(analysis.bodies.is_some(), "{:?}", analysis.violations);
    rejected(
        "fn f(text: core::Text, condition: bool) -> i64 { if condition { let mut left: core::Text = text; } else { let mut right: core::Text = text; } return core::text_len(text); }",
        V::Moved,
    );
}

#[test]
fn match_joins_and_unreachable_arms_use_coverage_facts() {
    rejected(
        "fn f(text: core::Text, key: bool) -> i64 { match key { true => { let mut moved: core::Text = text; }, false => {}, } return core::text_len(text); }",
        V::MaybeMoved,
    );
    let (_, analysis) = analyze(
        "fn f(text: core::Text, key: bool) -> i64 { match key { _ => { return 7; }, false => { let mut moved: core::Text = text; let mut again: core::Text = text; }, } }",
    );
    let checked = analysis.bodies.unwrap();
    assert!(
        checked.functions()[0]
            .accesses()
            .iter()
            .all(|a| a.kind != AccessKind::Move)
    );
    let (_, analysis) =
        analyze("fn f(text: core::Text) -> core::Text { match text { _ => {}, } return text; }");
    assert!(analysis.bodies.is_some(), "{:?}", analysis.violations);
}

#[test]
fn loop_back_edges_must_restore_initialization_and_move_state() {
    rejected(
        "fn f(text: core::Text, condition: bool) -> i64 { while condition { let mut moved: core::Text = text; } return 0; }",
        V::LoopState,
    );
    rejected(
        "fn pred(text: core::Text) -> bool { return false; } fn f(text: core::Text) -> i64 { while pred(text) {} return 0; }",
        V::LoopState,
    );
    let (_, analysis) = analyze(
        "fn id(text: core::Text) -> core::Text { return text; } fn f(a: core::Text, condition: bool) -> i64 { let mut text: core::Text = a; while condition { text = id(text); } return core::text_len(text); }",
    );
    assert!(analysis.bodies.is_some(), "{:?}", analysis.violations);
}

#[test]
fn zero_iteration_exits_still_keep_condition_effects() {
    rejected(
        "fn pred(text: core::Text) -> bool { return false; } fn f(text: core::Text) -> i64 { while pred(text) { return 1; } return core::text_len(text); }",
        V::Moved,
    );
}

#[test]
fn noncopy_fields_and_borrowed_values_cannot_be_moved_partially() {
    rejected(
        "struct R { text: core::Text, value: i64 } fn f(value: R) -> core::Text { return value.text; }",
        V::PartialMove,
    );
    rejected(
        "fn f(value: &core::Text) -> core::Text { return (*value); }",
        V::BorrowedMove,
    );
    let (_, analysis) = analyze(
        "struct R { text: core::Text, value: i64 } fn f(value: R) -> i64 { return value.value + core::text_len(value.text); }",
    );
    assert!(analysis.bodies.is_some(), "{:?}", analysis.violations);
}

#[test]
fn field_replacement_requires_the_destination_owner_after_rhs_evaluation() {
    rejected(
        "struct R { text: core::Text } fn consume(owner: R, text: core::Text) -> core::Text { return text; } fn f(a: R, replacement: core::Text) -> i64 { let mut owner: R = a; owner.text = consume(owner, replacement); return 0; }",
        V::Moved,
    );
}

#[test]
fn fallible_operations_consume_owning_arguments_on_error_paths() {
    rejected(
        "fn f(a: &core::Allocator, text: core::Text) -> i64 { let mut made: core::Result<shared core::Text, core::OwnershipError> = core::shared_new::<core::Text>(a, text); match made { core::Result::Ok(_) => { return 1; }, core::Result::Err(_) => { return core::text_len(text); }, } }",
        V::Moved,
    );
}

#[test]
fn argument_and_record_field_evaluation_preserve_written_move_order() {
    let (sources, analysis) = analyze(
        "fn take(a: core::Text, b: core::Text) -> i64 { return 0; } fn f(text: core::Text) -> i64 { return take(text, text); }",
    );
    assert_eq!(analysis.violations[0].kind, V::Moved);
    assert!(
        analysis.violations[0].origin.unwrap().range().start
            < analysis.violations[0].span.range().start
    );
    analysis.diagnostics[0].validate(&sources).unwrap();
    let (_, analysis) = analyze(
        "struct R { a: core::Text, b: core::Text } fn f(text: core::Text) -> R { return R { b: text, a: text }; }",
    );
    assert_eq!(analysis.violations[0].kind, V::Moved);
    assert!(
        analysis.violations[0].origin.unwrap().range().start
            < analysis.violations[0].span.range().start
    );
}

#[test]
fn source_frontend_reports_moves_but_success_still_requires_loan_region_and_mir_passes() {
    let (sources, bodies) = typed(
        "fn f(text: core::Text) -> i64 { let mut moved: core::Text = text; return core::text_len(text); }",
    );
    let checked = check_program(bodies.source().clone(), &sources, CheckLimits::default()).unwrap();
    assert!(checked.program.is_none());
    assert_eq!(checked.diagnostics[0].code, codes::INVALID_OWNER_USE);
    let (sources, bodies) = typed("fn f(text: core::Text) -> i64 { return core::text_len(text); }");
    let source = bodies.source().clone();
    assert!(
        moves::analyze(bodies, &sources, Limits::default())
            .unwrap()
            .bodies
            .is_some()
    );
    assert!(matches!(
        check_program(source, &sources, CheckLimits::default()),
        Err(CheckError::OwnershipProfile { .. })
    ));
}

#[test]
fn move_checked_bodies_still_require_lexical_loan_validation() {
    let (sources, bodies) = typed(
        "fn f(text: core::Text) -> core::Text { let mut read: &core::Text = &text; return text; }",
    );
    let source = bodies.source().clone();
    let checked = moves::analyze(bodies, &sources, Limits::default())
        .unwrap()
        .bodies
        .unwrap();
    assert!(
        checked.functions()[0]
            .accesses()
            .iter()
            .any(|a| a.kind == AccessKind::Move)
    );
    // Initialization facts alone cannot enable execution. The following loan
    // pass rejects the move even though the named loan is unused.
    let checked = check_program(source, &sources, CheckLimits::default()).unwrap();
    assert!(checked.program.is_none());
    assert_eq!(checked.diagnostics[0].code, codes::INVALID_LOAN);
}

#[test]
fn move_analysis_limits_are_exact_and_foreign_snapshots_are_rejected() {
    let (sources, bodies) = typed(
        "fn f(text: core::Text, key: bool) -> i64 { if key { return core::text_len(text); } else { return 0; } }",
    );
    let baseline = moves::analyze(bodies.clone(), &sources, Limits::default())
        .unwrap()
        .bodies
        .unwrap();
    let exact = Limits {
        work: baseline.work_used(),
        allocations: baseline.requested_bytes(),
        ..Limits::default()
    };
    assert_eq!(
        moves::analyze(bodies.clone(), &sources, exact)
            .unwrap()
            .bodies
            .unwrap(),
        baseline
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
        Limits { depth: 1, ..exact },
        Limits { locals: 0, ..exact },
        Limits {
            functions: 0,
            ..exact
        },
    ] {
        let moves::Error::Limit(d) = moves::analyze(bodies.clone(), &sources, limits).unwrap_err()
        else {
            panic!()
        };
        d.validate(&sources).unwrap();
    }
    assert!(matches!(
        moves::analyze(bodies, &SourceMap::new(), Limits::default()),
        Err(moves::Error::Source(_))
    ));
    let (sources, bodies) = typed(
        "fn f(text: core::Text) -> i64 { let mut moved: core::Text = text; return core::text_len(text); }",
    );
    assert!(matches!(
        moves::analyze(
            bodies,
            &sources,
            Limits {
                diagnostics: 0,
                ..Limits::default()
            }
        ),
        Err(moves::Error::Limit(_))
    ));
}

#[test]
fn generated_branch_programs_match_an_independent_boolean_initialization_oracle() {
    #[derive(Clone, Copy)]
    enum Op {
        Read(usize),
        Move(usize),
        Assign(usize, usize),
    }
    fn apply(ops: &[Op], state: &mut [bool; 3]) -> bool {
        let mut valid = true;
        for op in ops {
            match *op {
                Op::Read(i) => valid &= state[i],
                Op::Move(i) => {
                    valid &= state[i];
                    state[i] = false;
                }
                Op::Assign(i, j) => {
                    valid &= state[j];
                    state[j] = false;
                    state[i] = true;
                }
            }
        }
        valid
    }
    fn source(ops: &[Op], index: &mut usize) -> String {
        let mut text = String::new();
        for op in ops {
            let n = *index;
            *index += 1;
            text.push_str(&match *op {
                Op::Read(i) => format!("let mut n{n}: i64 = core::text_len(x{i});"),
                Op::Move(i) => format!("let mut t{n}: core::Text = x{i};"),
                Op::Assign(i, j) => format!("x{i} = id(x{j});"),
            });
        }
        text
    }
    let mut random = 137u64;
    let mut outcomes = [false; 2];
    for sample in 0..80 {
        let mut groups = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
        for group in &mut groups {
            for _ in 0..5 {
                random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                let a = (random as usize) % 3;
                let b = ((random >> 24) as usize) % 3;
                group.push(
                    match if sample % 2 == 0 {
                        if random & 1 == 0 { 0 } else { 3 }
                    } else {
                        (random >> 32) % 3
                    } {
                        0 => Op::Read(a),
                        1 => Op::Move(a),
                        2 => Op::Assign(a, b),
                        _ => Op::Assign(a, a),
                    },
                );
            }
        }
        let mut expected = true;
        for branch in [1, 2] {
            let mut state = [true; 3];
            expected &= apply(&groups[0], &mut state);
            expected &= apply(&groups[branch], &mut state);
            expected &= apply(&groups[3], &mut state);
        }
        let mut index = 0;
        let prefix = source(&groups[0], &mut index);
        let left = source(&groups[1], &mut index);
        let right = source(&groups[2], &mut index);
        let suffix = source(&groups[3], &mut index);
        let text = format!(
            "fn id(value: core::Text) -> core::Text {{ return value; }} fn f(a: core::Text, b: core::Text, d: core::Text, key: bool) -> i64 {{ let mut x0: core::Text = a; let mut x1: core::Text = b; let mut x2: core::Text = d; {prefix} if key {{ {left} }} else {{ {right} }} {suffix} return 0; }}"
        );
        let (_, analysis) = analyze(&text);
        outcomes[usize::from(expected)] = true;
        assert_eq!(
            analysis.bodies.is_some(),
            expected,
            "{text}: {:?}",
            analysis.violations
        );
    }
    assert_eq!(outcomes, [true, true]);
}
