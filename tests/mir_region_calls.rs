//! Source-independent MIR caller-region effects, including overwritten stores.
use lang::{frontend, frontend::CompileLimits};
use lang_diagnostics::SourceMap;
use lang_mir::unit::ownership::{
    self as o, bodies as b, loan_activity, loans, moves, places as p, regions as g,
};
use lang_semantics::analysis::Program;
use std::path::Path;

const TRANSIENT: &str = "fn store(out: &mut core::Option<arena i64>, input: arena i64) -> i64 { (*out) = core::Option::Some(input); (*out) = core::Option::None; return 0; } fn f() -> i64 { let mut outside: core::Option<arena i64> = core::Option::None; arena one(core::allocator()) { let mut token: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut inside: core::Option<arena i64> = core::Option::None; let mut called: i64 = store(inside,token); return called; } }";

fn table(source: &str) -> b::BodyTable {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("region-calls.lang"),
        source.as_bytes(),
        None,
        CompileLimits::default(),
        None,
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let Program::Ownership(program) = analysis.result.unwrap() else {
        panic!("ownership source")
    };
    program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table()
}
fn loaned(table: b::BodyTable) -> loans::LoanCheckedBodies {
    let typed = table.verify(Default::default()).unwrap();
    let moved = moves::analyze(typed, Default::default()).unwrap();
    let active = loan_activity::analyze(moved, Default::default()).unwrap();
    loans::analyze(active, Default::default()).unwrap()
}
fn forged_destination(source: &str) -> loans::LoanCheckedBodies {
    let mut table = table(source);
    let index = table
        .places
        .declarations
        .functions
        .iter()
        .position(|f| f.name == "f")
        .unwrap();
    let locals = &table.places.declarations.functions[index].locals;
    let find =
        |name| o::declarations::LocalId(locals.iter().position(|l| l.name == name).unwrap() as u32);
    let (inside, outside) = (find("inside"), find("outside"));
    let places = &mut table.places.functions[index].places;
    let mut edits = 0;
    for block in &table.functions[index].blocks {
        for i in &block.instructions {
            if let b::Op::Borrow { place, .. } = i.op
                && places[place.0 as usize].root == p::Root::Local(inside)
            {
                places[place.0 as usize].root = p::Root::Local(outside);
                edits += 1;
            }
        }
    }
    assert_eq!(edits, 1);
    loaned(table)
}
#[test]
fn overwritten_store_still_requires_the_callers_storage_lifetime() {
    let invalid = TRANSIENT.replace("store(inside,token)", "store(outside,token)");
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("bad-region-call.lang"),
        invalid.as_bytes(),
        None,
        CompileLimits::default(),
        None,
    );
    assert!(analysis.result.is_err());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == lang_diagnostics::codes::REGION_ESCAPE)
    );
    let input = forged_destination(TRANSIENT);
    let plan = g::prepare(&input, Default::default()).unwrap();
    let local = g::local::analyze(&plan, Default::default()).unwrap();
    assert_eq!(local.functions()[1].pending_calls().len(), 1);
    let error = g::calls::analyze(&plan, Default::default()).err().unwrap();
    assert!(
        matches!(error.kind, g::local::ErrorKind::CallStore { .. }),
        "{error:?}"
    );
    assert!(error.primary.is_some() && error.secondary.is_some());
    let primary = error.primary.unwrap();
    let secondary = error.secondary.unwrap();
    assert_eq!(
        &TRANSIENT[primary.start as usize..primary.end as usize],
        "store(inside,token)"
    );
    assert!(TRANSIENT[secondary.start as usize..secondary.end as usize].contains("Some(input)"));
    let g::local::ErrorKind::CallStore { callee, store, .. } = &error.kind else {
        unreachable!()
    };
    assert_eq!(callee.0, 0);
    assert_eq!(*store, secondary);
    drop(local);
    drop(plan);
    // R01 must make this complete proof mandatory in the existing admission
    // path; success here records the remaining integration boundary.
    o::failure::elaborate(input, Default::default()).unwrap();
}

#[test]
fn a_valid_overwritten_store_has_a_normal_mutation_and_a_transient_requirement() {
    let input = loaned(table(TRANSIENT));
    let plan = g::prepare(&input, Default::default()).unwrap();
    let result = g::calls::analyze(&plan, Default::default()).unwrap();
    assert!(
        result
            .functions()
            .iter()
            .all(|f| f.pending_calls().is_empty())
    );
    assert!(result.summaries()[0].may_return());
    assert_eq!(result.summaries()[0].mutation_count(), 1);
    assert_eq!(result.summaries()[0].transient_store_count(), 1);
    assert!(result.summaries()[0].input_projection_count() >= 2);
}

fn recursive_source(divergent: bool) -> String {
    let prefix = if divergent {
        "fn spin(out: &mut core::Option<arena i64>, input: arena i64) -> i64 { (*out) = core::Option::Some(input); (*out) = core::Option::None; return spin(out,input); }"
    } else {
        "fn left(out: &mut core::Option<arena i64>, input: arena i64, flag: bool) -> i64 { if flag { (*out) = core::Option::Some(input); (*out) = core::Option::None; return 0; } return right(out,input); } fn right(out: &mut core::Option<arena i64>, input: arena i64) -> i64 { return left(out,input,true); }"
    };
    let caller = TRANSIENT.split_once("fn f()").unwrap().1.replace(
        "store(inside,token)",
        if divergent {
            "spin(inside,token)"
        } else {
            "left(inside,token,false)"
        },
    );
    format!("{prefix} fn f(){caller}")
}
#[test]
fn recursive_and_divergent_calls_preserve_every_store_before_normal_return() {
    for divergent in [false, true] {
        let source = recursive_source(divergent);
        let input = loaned(table(&source));
        let plan = g::prepare(&input, Default::default()).unwrap();
        let result = g::calls::analyze(&plan, Default::default()).unwrap();
        assert_eq!(result.summaries()[0].may_return(), !divergent);
        assert!(result.summaries()[0].transient_store_count() > 0);
        assert!(result.rounds() >= 2);
        let input = forged_destination(&source);
        let plan = g::prepare(&input, Default::default()).unwrap();
        let error = g::calls::analyze(&plan, Default::default()).err().unwrap();
        assert!(
            matches!(error.kind, g::local::ErrorKind::CallStore { .. }),
            "{error:?}"
        );
    }
}

#[test]
fn normal_return_mutations_preserve_the_other_record_field() {
    let source = "struct Pair { left: arena i64, right: arena i64 } fn set_left(out: &mut Pair, input: arena i64) -> i64 { (*out).left = input; return 0; } fn f() -> i64 { arena one(core::allocator()) { let mut a: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); arena two(core::allocator()) { let mut b: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(two,2)); let mut pair: Pair = Pair {left:a,right:a}; let mut called: i64 = set_left(pair,b); let mut changed_left: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(two,pair.left,3); let mut changed_right: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(one,pair.right,4); return called; } } }";
    let input = loaned(table(source));
    let plan = g::prepare(&input, Default::default()).unwrap();
    let result = g::calls::analyze(&plan, Default::default()).unwrap();
    assert_eq!(result.summaries()[0].mutation_count(), 1);
}

#[test]
fn nested_reference_cells_use_the_actual_caller_storage() {
    let source = "fn put(out: &mut &mut core::Option<arena i64>, input: arena i64) -> i64 { (*(*out)) = core::Option::Some(input); return 0; } fn f() -> i64 { let mut old: core::Option<arena i64> = core::Option::None; let mut outside: &mut core::Option<arena i64> = &mut old; arena one(core::allocator()) { let mut token: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut local: core::Option<arena i64> = core::Option::None; let mut inside: &mut core::Option<arena i64> = &mut local; let mut called: i64 = put(inside,token); return called; } }";
    let input = loaned(table(source));
    let plan = g::prepare(&input, Default::default()).unwrap();
    g::calls::analyze(&plan, Default::default()).unwrap();
    let input = forged_destination(source);
    let plan = g::prepare(&input, Default::default()).unwrap();
    let error = g::calls::analyze(&plan, Default::default()).err().unwrap();
    assert!(
        matches!(error.kind, g::local::ErrorKind::CallStore { .. }),
        "{error:?}"
    );
}

#[test]
fn caller_input_projections_preserve_product_and_sum_precision() {
    let product = "struct Pair { left: arena i64, right: arena i64 } fn put(out: &mut core::Option<arena i64>, pair: Pair) -> i64 { (*out) = core::Option::Some(pair.left); return 0; } fn f() -> i64 { arena one(core::allocator()) { let mut a: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut outside: core::Option<arena i64> = core::Option::None; arena two(core::allocator()) { let mut b: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(two,2)); let mut pair: Pair = Pair {left:a,right:b}; let mut called: i64 = put(outside,pair); return called; } } }";
    let input = loaned(table(product));
    let plan = g::prepare(&input, Default::default()).unwrap();
    g::calls::analyze(&plan, Default::default()).unwrap();
    let mut table = table(product);
    let mut edits = 0;
    for place in &mut table.places.functions[0].places {
        if place.root == p::Root::Local(o::declarations::LocalId(1))
            && place.projections.len() == 1
            && place.projections[0].kind == p::ProjectionKind::Field(0)
        {
            place.projections[0].kind = p::ProjectionKind::Field(1);
            edits += 1;
        }
    }
    assert_eq!(edits, 1);
    let input = loaned(table);
    let plan = g::prepare(&input, Default::default()).unwrap();
    let error = g::calls::analyze(&plan, Default::default()).err().unwrap();
    assert!(
        matches!(error.kind, g::local::ErrorKind::CallStore { .. }),
        "{error:?}"
    );
    let sum = "enum Choice { A(arena i64), B(arena i64) } fn put(out: &mut core::Option<arena i64>, choice: Choice) -> i64 { match choice { Choice::A(token) => { (*out) = core::Option::Some(token); }, Choice::B(_) => { (*out) = core::Option::None; }, } return 0; } fn f(flag: bool) -> i64 { arena one(core::allocator()) { let mut a: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut outside: core::Option<arena i64> = core::Option::None; arena two(core::allocator()) { let mut b: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(two,2)); let mut choice: Choice = Choice::A(a); if flag { choice = Choice::B(b); } let mut called: i64 = put(outside,choice); return called; } } }";
    let input = loaned(self::table(sum));
    let plan = g::prepare(&input, Default::default()).unwrap();
    g::calls::analyze(&plan, Default::default()).unwrap();
}

#[test]
fn multiple_normal_outputs_read_one_snapshot_and_recursive_clears_are_definite() {
    let swap = "fn swap(a: &mut core::Option<arena i64>, b: &mut core::Option<arena i64>) -> i64 { let mut old: core::Option<arena i64> = (*a); (*a) = (*b); (*b) = old; return 0; } fn f() -> i64 { arena one(core::allocator()) { let mut a: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut outside: core::Option<arena i64> = core::Option::None; arena two(core::allocator()) { let mut b: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(two,2)); let mut left: core::Option<arena i64> = core::Option::Some(a); let mut right: core::Option<arena i64> = core::Option::Some(b); let mut called: i64 = swap(left,right); outside = right; return called; } } }";
    let input = loaned(table(swap));
    let plan = g::prepare(&input, Default::default()).unwrap();
    let result = g::calls::analyze(&plan, Default::default()).unwrap();
    assert_eq!(result.summaries()[0].mutation_count(), 2);
    let clear = "fn clear(out: &mut core::Option<arena i64>, count: i64) -> i64 { if count < 1 { (*out) = core::Option::None; return 0; } return clear(out,count + -1); } fn f() -> i64 { let mut outside: core::Option<arena i64> = core::Option::None; arena one(core::allocator()) { let mut token: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut inside: core::Option<arena i64> = core::Option::Some(token); let mut called: i64 = clear(inside,3); outside = inside; return called; } }";
    let input = loaned(table(clear));
    let plan = g::prepare(&input, Default::default()).unwrap();
    g::calls::analyze(&plan, Default::default()).unwrap();
}

#[test]
fn call_analysis_has_exact_cumulative_limits_and_reusable_inputs() {
    let source = recursive_source(false);
    let input = loaned(table(&source));
    let plan = g::prepare(&input, Default::default()).unwrap();
    let result = g::calls::analyze(&plan, Default::default()).unwrap();
    let used = result.usage();
    let exact = g::local::Limits {
        work: used.work,
        allocations: used.allocation_bytes,
        elements: used.elements,
        nodes: result.nodes_used(),
        state_cells: result.state_cells(),
        ..Default::default()
    };
    assert!(g::calls::analyze(&plan, exact).is_ok());
    for limit in [
        g::local::Limits {
            work: exact.work - 1,
            ..exact
        },
        g::local::Limits {
            allocations: exact.allocations - 1,
            ..exact
        },
        g::local::Limits {
            elements: exact.elements - 1,
            ..exact
        },
        g::local::Limits {
            nodes: exact.nodes - 1,
            ..exact
        },
        g::local::Limits {
            state_cells: exact.state_cells - 1,
            ..exact
        },
        g::local::Limits { depth: 0, ..exact },
        g::local::Limits {
            work: plan.usage().work - 1,
            ..exact
        },
    ] {
        assert!(g::calls::analyze(&plan, limit).is_err());
    }
    assert!(g::calls::analyze(&plan, exact).is_ok());
}

#[test]
fn collection_call_effects_check_the_actual_container_lifetime() {
    for (ty, new, body) in [
        (
            "core::Vec<arena i64>",
            "core::result_unwrap::<core::Vec<arena i64>,core::AllocError>(core::vec_new::<arena i64>(core::allocator()))",
            "let mut changed: core::Result<core::Unit,core::AllocError> = core::vec_push::<arena i64>(out,input);",
        ),
        (
            "core::Map<i64,arena i64>",
            "core::result_unwrap::<core::Map<i64,arena i64>,core::AllocError>(core::map_new::<i64,arena i64>(core::allocator()))",
            "let mut changed: core::Result<core::Option<arena i64>,core::AllocError> = core::map_insert::<i64,arena i64>(out,1,input);",
        ),
    ] {
        let source = format!(
            "fn put(out: &mut {ty}, input: arena i64) -> i64 {{ {body} return 0; }} fn f() -> i64 {{ let mut outside: {ty} = {new}; arena one(core::allocator()) {{ let mut token: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut inside: {ty} = {new}; let mut called: i64 = put(inside,token); return called; }} }}"
        );
        let input = loaned(table(&source));
        let plan = g::prepare(&input, Default::default()).unwrap();
        g::calls::analyze(&plan, Default::default()).unwrap();
        let input = forged_destination(&source);
        let plan = g::prepare(&input, Default::default()).unwrap();
        let error = g::calls::analyze(&plan, Default::default()).err().unwrap();
        assert!(
            matches!(error.kind, g::local::ErrorKind::CallStore { .. }),
            "{error:?}"
        );
    }
}

#[test]
fn provisional_element_bindings_can_be_forwarded_to_a_guest_mutation() {
    let source = "struct Entry { token: arena i64, number: i64 } fn put(out: &mut Entry, input: arena i64) -> i64 { out.token = input; return out.number; } fn f() -> i64 { arena one(core::allocator()) { let mut token: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut entries: core::Vec<Entry> = core::result_unwrap::<core::Vec<Entry>,core::AllocError>(core::vec_new::<Entry>(core::allocator())); let mut pushed: core::Result<core::Unit,core::AllocError> = core::vec_push::<Entry>(entries,Entry {token:token,number:7}); match core::vec_get_mut::<Entry>(entries,0) { core::Result::Ok(item) => { return put(item,token); }, core::Result::Err(_) => {return 0;}, } } }";
    let input = loaned(table(source));
    let plan = g::prepare(&input, Default::default()).unwrap();
    g::calls::analyze(&plan, Default::default()).unwrap();
}

#[test]
fn foreign_reference_cells_cannot_receive_a_callees_reference() {
    let source = "fn f(target: &mut &i64, input: &i64) -> i64 { let mut scratch: &i64 = &(*input); scratch = input; return 0; }";
    let invalid = source.replace("scratch = input;", "(*target) = input;");
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("reference-store.lang"),
        invalid.as_bytes(),
        None,
        CompileLimits::default(),
        None,
    );
    assert!(analysis.result.is_err());
    let mut table = table(source);
    let locals = &table.places.declarations.functions[0].locals;
    let scratch =
        o::declarations::LocalId(locals.iter().position(|l| l.name == "scratch").unwrap() as u32);
    let mut edits = 0;
    for block in &table.functions[0].blocks {
        for i in &block.instructions {
            if let b::Op::PreparePlace { place, .. } = i.op
                && table.places.functions[0].places[place.0 as usize].root
                    == p::Root::Local(scratch)
            {
                let place = &mut table.places.functions[0].places[place.0 as usize];
                place.root = p::Root::Local(o::declarations::LocalId(0));
                place.projections.push(p::Projection {
                    kind: p::ProjectionKind::Dereference,
                    span: place.span,
                });
                edits += 1;
            }
        }
    }
    assert_eq!(edits, 1);
    let typed = table.verify(Default::default()).unwrap();
    let moved = moves::analyze(typed, Default::default()).unwrap();
    let active = loan_activity::analyze(moved, Default::default()).unwrap();
    let error = loans::analyze(active, Default::default()).err().unwrap();
    assert!(
        matches!(error.kind, loans::ErrorKind::CallerCellEscape(_)),
        "{error:?}"
    );
}

#[test]
fn a_plain_nonreturning_call_defers_local_diagnostics_and_cuts_cfg_successors() {
    let source = "fn stop() -> i64 { return stop(); } fn f(token: arena i64) -> i64 { let mut called: i64 = stop(); if true { return 0; } return 0; }";
    let input = loaned(table(source));
    let plan = g::prepare(&input, Default::default()).unwrap();
    assert_eq!(plan.functions()[1].calls()[0].effects, g::Effects::Plain);
    let local = g::local::analyze(&plan, Default::default()).unwrap();
    assert_eq!(local.functions()[1].pending_calls().len(), 1);
    let result = g::calls::analyze(&plan, Default::default()).unwrap();
    assert!(!result.summaries()[0].may_return());
    assert!(!result.summaries()[1].may_return());
    assert!(
        result.functions()[1]
            .direct_brands(
                lang_mir::unit::BlockId(0),
                moves::Slot::Local(o::declarations::LocalId(0))
            )
            .is_some()
    );
    assert!(
        result.functions()[1]
            .direct_brands(
                lang_mir::unit::BlockId(1),
                moves::Slot::Local(o::declarations::LocalId(0))
            )
            .is_none()
    );
}

#[test]
fn a_local_brand_stored_into_a_caller_cell_retains_its_operation_span() {
    let source = "fn f(out: &mut core::Option<arena i64>) -> i64 { arena one(core::allocator()) { let mut token: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut inside: core::Option<arena i64> = core::Option::None; inside = core::Option::Some(token); return 0; } }";
    let mut table = table(source);
    let inside = o::declarations::LocalId(
        table.places.declarations.functions[0]
            .locals
            .iter()
            .position(|l| l.name == "inside")
            .unwrap() as u32,
    );
    let mut statement = None;
    for block in &table.functions[0].blocks {
        for i in &block.instructions {
            if let b::Op::PreparePlace { place, .. } = i.op
                && table.places.functions[0].places[place.0 as usize].root == p::Root::Local(inside)
            {
                let place = &mut table.places.functions[0].places[place.0 as usize];
                place.root = p::Root::Local(o::declarations::LocalId(0));
                place.projections.push(p::Projection {
                    kind: p::ProjectionKind::Dereference,
                    span: place.span,
                });
            }
            if matches!(i.op, b::Op::ReplacePrepared { .. }) {
                statement = Some(i.span);
            }
        }
    }
    let input = loaned(table);
    let plan = g::prepare(&input, Default::default()).unwrap();
    let error = g::calls::analyze(&plan, Default::default()).err().unwrap();
    assert!(
        matches!(error.kind, g::local::ErrorKind::Escape { .. }),
        "{error:?}"
    );
    assert_eq!(error.primary, statement);
    assert!(error.secondary.is_some());
}
