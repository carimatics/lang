//! Region validation must be independent of the source frontend's HIR proof.
use lang::{frontend, frontend::CompileLimits};
use lang_diagnostics::SourceMap;
use lang_mir::unit::ownership::{
    self as o, bodies as b, loan_activity, loans, moves, places as p, regions as g,
};
use lang_semantics::analysis::Program;
use std::path::Path;

const SOURCE: &str = "fn f() -> i64 { arena one(core::allocator()) { let mut node1: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(one, 1)); let mut copied: arena i64 = node1; arena two(core::allocator()) { let mut node2: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(core::arena_new::<i64>(two, 2)); let mut changed: core::Result<core::Unit, core::ArenaError> = core::arena_replace::<i64>(two, node2, 3); return 0; } } }";

fn table() -> b::BodyTable {
    checked_table(SOURCE)
}
fn checked_table(source: &str) -> b::BodyTable {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("mir-regions.lang"),
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
        panic!("ownership source");
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

#[test]
fn source_rejection_and_mir_region_preparation_have_distinct_contracts() {
    let invalid = SOURCE.replace("(two, node2, 3)", "(two, node1, 3)");
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("bad-regions.lang"),
        invalid.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(analysis.result.is_err());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == lang_diagnostics::codes::REGION_ESCAPE)
    );
    let mut table = table();
    let locals = &table.places.declarations.functions[0].locals;
    let index = |name| locals.iter().position(|local| local.name == name).unwrap() as u32;
    let (one, two) = (index("node1"), index("node2"));
    let places = &table.places.functions[0].places;
    let source = p::PlaceId(
        places
            .iter()
            .position(|p| {
                p.root == p::Root::Local(o::declarations::LocalId(one)) && p.projections.is_empty()
            })
            .unwrap() as u32,
    );
    let mut changed = 0;
    for block in &mut table.functions[0].blocks {
        for instruction in &mut block.instructions {
            if let b::Op::CopyPlace(place) = &mut instruction.op
                && places[place.0 as usize].root == p::Root::Local(o::declarations::LocalId(two))
                && places[place.0 as usize].projections.is_empty()
            {
                *place = source;
                changed += 1;
            }
        }
    }
    assert_eq!(changed, 1);
    let owner = locals.iter().position(|local| local.name == "two").unwrap() as u32;
    let loaned = loaned(table);
    let plan = g::prepare(&loaned, Default::default()).unwrap();
    assert!(std::ptr::eq(plan.source(), &loaned));
    assert_eq!(
        plan.functions()[0].path(source).unwrap().root().slot,
        moves::Slot::Local(o::declarations::LocalId(one))
    );
    assert!(
        plan.functions()[0]
            .arena_calls()
            .iter()
            .any(|call| call.owner.local == owner)
    );
    // R08 preserves both inputs but proves no relationship between them.
    // R09/R10/R01 must reject this forged MIR before arena admission is enabled.
    drop(plan);
    o::failure::elaborate(loaned, Default::default()).unwrap();
}

#[test]
fn region_paths_preserve_field_loan_case_and_guest_call_boundaries() {
    let source = "struct Pair { token: arena i64, count: i64 } enum Wrapped { None, Some(Pair) } fn inspect(value: arena i64) -> i64 { return 0; } fn scalar(value: i64) -> i64 { return value; } fn f(pair: &Pair) -> i64 { let mut value: i64 = inspect((*pair).token); return scalar(value); } fn unused(value: Wrapped) -> i64 { return 0; }";
    let input = loaned(checked_table(source));
    let plan = g::prepare(&input, Default::default()).unwrap();
    let node = plan
        .node_for(o::Type::Nominal(lang_mir::unit::DefinitionId(0)))
        .unwrap();
    let g::Kind::Record(fields) = &plan.node(node).unwrap().kind else {
        panic!("record");
    };
    assert!(matches!(
        plan.node(fields[0]).unwrap().kind,
        g::Kind::Arena(_)
    ));
    let wrapped = plan
        .node(
            plan.node_for(o::Type::Nominal(lang_mir::unit::DefinitionId(1)))
                .unwrap(),
        )
        .unwrap();
    assert_eq!(wrapped.case(0).unwrap(), []);
    assert_eq!(wrapped.case(1).unwrap(), [node]);
    assert!(wrapped.case(2).is_none());
    let f = &plan.functions()[2];
    let path = f
        .paths()
        .iter()
        .find(|path| path.steps().len() == 2)
        .unwrap();
    assert_eq!(path.steps()[0].kind, p::ProjectionKind::Dereference);
    assert_eq!(path.steps()[1].kind, p::ProjectionKind::Field(0));
    assert_eq!(path.result(), fields[0]);
    assert_eq!(
        f.calls().iter().map(|c| c.effects).collect::<Vec<_>>(),
        [g::Effects::Regions, g::Effects::Plain]
    );
    assert_eq!(f.calls()[0].arguments()[0].node, fields[0]);
    assert_eq!(
        f.calls()[0].target,
        g::Target::Guest(lang_mir::unit::FunctionId(0))
    );
    assert!(
        f.root(moves::Slot::Value(lang_mir::unit::Register(u32::MAX)))
            .is_none()
    );
    assert!(f.path(p::PlaceId(u32::MAX)).is_none());
}

#[test]
fn region_preparation_checks_exact_cumulative_budgets_and_keeps_the_input_reusable() {
    let input = loaned(table());
    let plan = g::prepare(&input, Default::default()).unwrap();
    let used = plan.usage();
    let counts = plan.counts();
    assert!(counts.paths > 0 && counts.edges > 0);
    let exact = g::Limits {
        work: used.work,
        allocations: used.allocation_bytes,
        elements: used.elements,
        nodes: counts.nodes,
        paths: counts.paths,
        edges: counts.edges,
    };
    assert!(g::prepare(&input, exact).is_ok());
    for limits in [
        g::Limits {
            work: exact.work - 1,
            ..exact
        },
        g::Limits {
            allocations: exact.allocations - 1,
            ..exact
        },
        g::Limits {
            elements: exact.elements - 1,
            ..exact
        },
        g::Limits {
            nodes: exact.nodes - 1,
            ..exact
        },
        g::Limits {
            paths: exact.paths - 1,
            ..exact
        },
        g::Limits {
            edges: exact.edges - 1,
            ..exact
        },
    ] {
        assert!(g::prepare(&input, limits).is_err());
    }
    assert!(
        g::prepare(
            &input,
            g::Limits {
                work: input.work_used() - 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    assert!(g::prepare(&input, exact).is_ok());
    assert!(plan.node(g::NodeId(usize::MAX)).is_none());
    assert!(
        plan.node_for(o::Type::Constructed(o::ConstructedId(u32::MAX)))
            .is_none()
    );
}
