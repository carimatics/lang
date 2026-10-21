//! Region validation must be independent of the source frontend's HIR proof.
use lang::{frontend, frontend::CompileLimits};
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
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
    let error = match g::local::analyze(&plan, Default::default()) {
        Ok(_) => panic!("different arena token admitted by local transfer"),
        Err(error) => error,
    };
    assert!(
        matches!(error.kind, g::local::ErrorKind::CrossArena { .. }),
        "{error:?}"
    );
    assert!(error.primary.is_some() && error.secondary.is_some());
    // The complete proof is mandatory at the failure/execution boundary too.
    drop(plan);
    let error = o::failure::elaborate(loaned, Default::default()).unwrap_err();
    assert!(matches!(
        error.kind,
        o::failure::ErrorKind::Regions(g::local::Error {
            kind: g::local::ErrorKind::CrossArena { .. },
            ..
        })
    ));
}

#[test]
fn local_region_transfer_accepts_same_arena_tokens_and_reports_call_obligations() {
    let input = loaned(table());
    let plan = g::prepare(&input, Default::default()).unwrap();
    let analysis = g::local::analyze(&plan, Default::default()).unwrap();
    assert!(analysis.functions()[0].pending_calls().is_empty());
    let input = loaned(checked_table(
        "fn inspect(value: arena i64) -> i64 { return 0; } fn f(value: arena i64) -> i64 { return inspect(value); }",
    ));
    let plan = g::prepare(&input, Default::default()).unwrap();
    let analysis = g::local::analyze(&plan, Default::default()).unwrap();
    assert_eq!(analysis.functions()[1].pending_calls().len(), 1);
    let site = analysis.functions()[1].pending_calls()[0];
    assert!(matches!(
        input.activity().moved().typed().functions()[1].blocks[site.block.0 as usize].instructions
            [site.order - 1]
            .op,
        b::Op::Call { .. }
    ));
    let input = loaned(checked_table(include_str!(
        "../examples/analysis/arena.lang"
    )));
    let plan = g::prepare(&input, Default::default()).unwrap();
    g::local::analyze(&plan, Default::default()).unwrap();
}

#[test]
fn local_transfer_preserves_aggregate_case_reference_and_loop_precision() {
    for source in [
        "struct Pair { left: arena i64, right: arena i64 } fn f() -> i64 { arena one(core::allocator()) { let mut a: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); arena two(core::allocator()) { let mut b: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(two,2)); let mut p: Pair = Pair {left:a,right:b}; let mut r: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(two, p.right, 3); let mut s: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(one, p.left, 4); return 0; } } }",
        "enum Choice { Left(arena i64), Right(arena i64) } fn f(flag: bool) -> i64 { arena one(core::allocator()) { let mut a: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); arena two(core::allocator()) { let mut b: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(two,2)); let mut choice: Choice = Choice::Left(a); if flag { choice = Choice::Right(b); } match choice { Choice::Left(x) => { let mut r: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(one, x, 3); }, Choice::Right(y) => { let mut s: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(two, y, 3); }, } return 0; } } }",
        "fn f() -> i64 { let mut count: i64 = 0; while count < 2 { arena one(core::allocator()) { let mut a: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut r: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(one,a,2); } count = count + 1; } return count; }",
        "fn f(pair: &mut core::Option<arena i64>, input: arena i64) -> i64 { (*pair) = core::Option::Some(input); return 0; }",
        "struct Entry { token: arena i64, number: i64 } fn f() -> i64 { arena one(core::allocator()) { let mut token: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut entries: core::Vec<Entry> = core::result_unwrap::<core::Vec<Entry>,core::AllocError>(core::vec_new::<Entry>(core::allocator())); let mut pushed: core::Result<core::Unit,core::AllocError> = core::vec_push::<Entry>(entries,Entry {token:token,number:7}); match core::vec_get_mut::<Entry>(entries,0) { core::Result::Ok(item) => { item.number = 8; let mut changed: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(one,item.token,2); }, core::Result::Err(_) => {}, } let mut popped: core::Option<Entry> = core::vec_pop::<Entry>(entries); return 0; } }",
        "fn f() -> i64 { arena one(core::allocator()) { let mut token: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut entries: core::Map<i64,arena i64> = core::result_unwrap::<core::Map<i64,arena i64>,core::AllocError>(core::map_new::<i64,arena i64>(core::allocator())); let mut inserted: core::Result<core::Option<arena i64>,core::AllocError> = core::map_insert::<i64,arena i64>(entries,1,token); let mut key: i64 = 1; match core::map_get::<i64,arena i64>(entries,key) { core::Option::Some(item) => { let mut changed: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(one,(*item),2); }, core::Option::None => {}, } let mut removed: core::Option<arena i64> = core::map_remove::<i64,arena i64>(entries,key); return 0; } }",
        "fn f() -> i64 { let mut error: core::ArenaError = core::ArenaError::Allocation(core::AllocError::Overflow); match error { core::ArenaError::Allocation(allocation) => { match allocation { core::AllocError::Limit => {return 1;}, core::AllocError::OutOfMemory => {return 2;}, core::AllocError::Overflow => {return 3;}, } }, core::ArenaError::CrossRegion => {return 4;}, core::ArenaError::Unavailable => {return 5;}, core::ArenaError::BorrowConflict => {return 6;}, } }",
    ] {
        let input = loaned(checked_table(source));
        let plan = g::prepare(&input, Default::default()).unwrap();
        g::local::analyze(&plan, Default::default()).unwrap();
        for fact in plan.functions()[0].arena_calls() {
            let instruction = &input.activity().moved().typed().functions()[0].blocks
                [fact.site.block.0 as usize]
                .instructions[fact.site.order - 1];
            assert_eq!(fact.span, instruction.span);
            assert!(matches!(instruction.op, b::Op::CoreCall(_)));
        }
    }
}

#[test]
fn a_captured_destination_cannot_store_an_inner_region_in_an_outer_local() {
    let source = "fn f() -> i64 { let mut outside: core::Option<arena i64> = core::Option::None; outside = core::Option::None; arena one(core::allocator()) { let mut token: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut inside: core::Option<arena i64> = core::Option::None; inside = core::Option::Some(token); return 0; } }";
    let mut table = checked_table(source);
    let locals = &table.places.declarations.functions[0].locals;
    let find =
        |name| o::declarations::LocalId(locals.iter().position(|l| l.name == name).unwrap() as u32);
    let (inside, outside) = (find("inside"), find("outside"));
    let places = &table.places.functions[0].places;
    let outside = p::PlaceId(
        places
            .iter()
            .position(|p| p.root == p::Root::Local(outside) && p.projections.is_empty())
            .unwrap() as u32,
    );
    let mut edits = 0;
    for block in &mut table.functions[0].blocks {
        for i in &mut block.instructions {
            if let b::Op::PreparePlace { place, .. } = &mut i.op
                && places[place.0 as usize].root == p::Root::Local(inside)
            {
                *place = outside;
                edits += 1;
            }
        }
    }
    assert_eq!(edits, 1);
    let input = loaned(table);
    let plan = g::prepare(&input, Default::default()).unwrap();
    let error = g::local::analyze(&plan, Default::default()).err().unwrap();
    assert!(
        matches!(error.kind, g::local::ErrorKind::Escape { .. }),
        "{error:?}"
    );
}

#[test]
fn arena_reentry_does_not_revive_tokens_from_the_previous_activation() {
    let source = "fn f() -> i64 { arena one(core::allocator()) { let mut token: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); let mut r: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(one,token,2); return 0; } }";
    let mut table = checked_table(source);
    let body = &mut table.functions[0];
    let (owner, allocator) = body
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let b::Op::ArenaEnter { owner, allocator } = i.op {
                Some((owner, allocator))
            } else {
                None
            }
        })
        .unwrap();
    // Reentry needs a fresh allocator argument: source lowering ends the
    // original arena-entry temporaries immediately after authority capture.
    let mut allocator_call = body
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| match &i.op {
            b::Op::CoreCall(call) if call.operation == o::core::Operation::Allocator => {
                Some(call.clone())
            }
            _ => None,
        })
        .unwrap();
    let fresh_loan = b::LoanId(body.loans as u32);
    body.loans += 1;
    allocator_call.result_loan = Some(fresh_loan);
    let values = &mut table.places.functions[0].values;
    let fresh_allocator = m::Register(values.len() as u32);
    values.push(values[allocator.0 as usize]);
    let argument = body
        .blocks
        .iter()
        .flat_map(|b| &b.instructions)
        .find_map(|i| {
            if let b::Op::CoreCall(ref call) = i.op
                && call.operation == o::core::Operation::ArenaReplace
            {
                Some(call.arguments[0])
            } else {
                None
            }
        })
        .unwrap();
    let mut edits = 0;
    for block in &mut body.blocks {
        if let Some(position) = block
            .instructions
            .iter()
            .position(|i| i.results.contains(&argument))
        {
            let span = block.instructions[position].span;
            block.instructions.splice(
                position..position,
                [
                    b::Instruction {
                        results: vec![],
                        op: b::Op::ArenaExit { owner },
                        span,
                    },
                    b::Instruction {
                        results: vec![fresh_allocator],
                        op: b::Op::CoreCall(allocator_call.clone()),
                        span,
                    },
                    b::Instruction {
                        results: vec![],
                        op: b::Op::ArenaEnter {
                            owner,
                            allocator: fresh_allocator,
                        },
                        span,
                    },
                    b::Instruction {
                        results: vec![],
                        op: b::Op::EndBorrow {
                            loan: fresh_loan,
                            mode: b::EndMode::IfActive,
                        },
                        span,
                    },
                ],
            );
            edits += 1;
        }
    }
    assert_eq!(edits, 1);
    let input = loaned(table);
    let plan = g::prepare(&input, Default::default()).unwrap();
    let error = g::local::analyze(&plan, Default::default()).err().unwrap();
    assert!(
        matches!(error.kind, g::local::ErrorKind::ExpiredArena(_)),
        "{error:?}"
    );
}

#[test]
fn local_transfer_uses_exact_cumulative_budgets_and_reuses_the_prepared_input() {
    let input = loaned(table());
    let plan = g::prepare(&input, Default::default()).unwrap();
    let analysis = g::local::analyze(&plan, Default::default()).unwrap();
    let used = analysis.usage();
    let exact = g::local::Limits {
        work: used.work,
        allocations: used.allocation_bytes,
        elements: used.elements,
        nodes: analysis.nodes_used(),
        state_cells: analysis.state_cells(),
        ..Default::default()
    };
    assert!(g::local::analyze(&plan, exact).is_ok());
    for limits in [
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
        assert!(g::local::analyze(&plan, limits).is_err());
    }
    assert!(g::local::analyze(&plan, exact).is_ok());
    assert!(
        analysis.functions()[0]
            .direct_brands(
                lang_mir::unit::BlockId(u32::MAX),
                moves::Slot::Value(lang_mir::unit::Register(u32::MAX))
            )
            .is_none()
    );
}

#[test]
fn mandatory_admission_uses_one_global_budget_and_exact_region_graph_limits() {
    use o::failure as f;
    let input = loaned(table());
    let plan = g::prepare(&input, Default::default()).unwrap();
    let proof = g::calls::analyze(&plan, Default::default()).unwrap();
    let counts = plan.counts();
    let region_limits = f::RegionLimits {
        type_nodes: counts.nodes,
        paths: counts.paths,
        edges: counts.edges,
        analysis_nodes: proof.nodes_used(),
        state_cells: proof.state_cells(),
        depth: 64,
    };
    let flow = f::elaborate(input.clone(), Default::default()).unwrap();
    assert_eq!(flow.region_usage(), proof.usage());
    assert!(flow.region_usage().work > input.work_used());
    assert!(flow.work_used() > flow.region_usage().work);
    assert!(flow.requested_bytes() > flow.region_usage().allocation_bytes);
    assert!(flow.elements_used() > flow.region_usage().elements);
    let exact = f::Limits {
        work: flow.work_used(),
        allocations: flow.requested_bytes(),
        elements: flow.elements_used(),
        nodes: flow.functions().iter().map(|f| f.nodes().len()).sum(),
        regions: region_limits,
    };
    assert_eq!(f::elaborate(input.clone(), exact).unwrap(), flow);
    for limits in [
        f::Limits {
            work: exact.work - 1,
            ..exact
        },
        f::Limits {
            allocations: exact.allocations - 1,
            ..exact
        },
        f::Limits {
            elements: exact.elements - 1,
            ..exact
        },
    ] {
        assert!(
            f::elaborate(input.clone(), limits)
                .unwrap_err()
                .is_resource()
        );
    }
    for regions in [
        f::RegionLimits {
            type_nodes: region_limits.type_nodes - 1,
            ..region_limits
        },
        f::RegionLimits {
            paths: region_limits.paths - 1,
            ..region_limits
        },
        f::RegionLimits {
            edges: region_limits.edges - 1,
            ..region_limits
        },
        f::RegionLimits {
            analysis_nodes: region_limits.analysis_nodes - 1,
            ..region_limits
        },
        f::RegionLimits {
            state_cells: region_limits.state_cells - 1,
            ..region_limits
        },
        f::RegionLimits {
            depth: 0,
            ..region_limits
        },
    ] {
        let error = f::elaborate(input.clone(), f::Limits { regions, ..exact }).unwrap_err();
        assert!(error.is_resource(), "{error:?}");
        assert!(matches!(
            error.kind,
            f::ErrorKind::RegionPreparation(_) | f::ErrorKind::Regions(_)
        ));
    }
    assert_eq!(f::elaborate(input, exact).unwrap(), flow);
}

#[test]
fn all_region_admission_quotas_become_resource_diagnostics_on_the_original_snapshot() {
    use o::failure as f;
    let source = "fn f(out: &mut core::Option<arena i64>) -> i64 { let mut copied: core::Option<arena i64> = (*out); return 0; }";
    for regions in [
        f::RegionLimits {
            type_nodes: 0,
            ..Default::default()
        },
        f::RegionLimits {
            paths: 0,
            ..Default::default()
        },
        f::RegionLimits {
            edges: 0,
            ..Default::default()
        },
        f::RegionLimits {
            analysis_nodes: 0,
            ..Default::default()
        },
        f::RegionLimits {
            state_cells: 0,
            ..Default::default()
        },
        f::RegionLimits {
            depth: 0,
            ..Default::default()
        },
    ] {
        let mut limits = CompileLimits::default();
        limits.ownership.failure.regions = regions;
        let mut sources = SourceMap::new();
        let old = sources.add("limits.lang", "old snapshot").unwrap();
        let analysis = frontend::analyze_virtual(
            sources,
            Path::new("limits.lang"),
            source.as_bytes(),
            None,
            limits,
            None,
        );
        assert!(
            matches!(analysis.result, Err(frontend::AnalysisError::Diagnostics)),
            "{:?}",
            analysis.result
        );
        let diagnostic = analysis
            .diagnostics
            .iter()
            .find(|d| d.code == lang_diagnostics::codes::RESOURCE_LIMIT)
            .unwrap();
        assert_ne!(diagnostic.primary.span.source(), old);
        assert_eq!(
            analysis
                .sources
                .file(diagnostic.primary.span.source())
                .unwrap()
                .text(),
            source
        );
        assert_eq!(
            analysis.sources.slice(diagnostic.primary.span).unwrap(),
            source
        );
    }
}

#[test]
fn cfg_arguments_carry_caller_brands_through_a_loop_backedge() {
    use lang_mir::unit as m;
    let mut table = checked_table(
        "fn f(token: arena i64) -> i64 { let mut copied: arena i64 = token; return 0; }",
    );
    let body = &mut table.functions[0];
    assert_eq!(body.blocks.len(), 1);
    let span = body.blocks[0].span;
    let source = body.blocks[0]
        .instructions
        .iter()
        .find(|i| matches!(i.op, b::Op::CopyPlace(_)))
        .unwrap()
        .results[0];
    let b::Term::Return(result) = body.blocks[0].terminator.kind else {
        panic!("return")
    };
    let values = &mut table.places.functions[0].values;
    let phi = m::Register(values.len() as u32);
    values.push(values[source.0 as usize]);
    let condition = m::Register(values.len() as u32);
    values.push(o::TypeUse {
        ty: o::Type::Bool,
        span,
    });
    body.blocks[0].instructions.push(b::Instruction {
        results: vec![condition],
        op: b::Op::Bool(true),
        span,
    });
    body.blocks[0].terminator.kind = b::Term::Jump(m::Target {
        block: m::BlockId(1),
        arguments: vec![source],
    });
    body.blocks.push(b::Block {
        parameters: vec![phi],
        instructions: vec![],
        terminator: b::Terminator {
            kind: b::Term::Branch {
                condition,
                then_target: m::Target {
                    block: m::BlockId(1),
                    arguments: vec![phi],
                },
                else_target: m::Target {
                    block: m::BlockId(2),
                    arguments: vec![],
                },
            },
            span,
        },
        span,
    });
    body.blocks.push(b::Block {
        parameters: vec![],
        instructions: vec![],
        terminator: b::Terminator {
            kind: b::Term::Return(result),
            span,
        },
        span,
    });
    let input = loaned(table);
    let plan = g::prepare(&input, Default::default()).unwrap();
    let local = g::local::analyze(&plan, Default::default()).unwrap();
    let brands = local.functions()[0]
        .direct_brands(m::BlockId(1), moves::Slot::Value(phi))
        .unwrap();
    assert!(matches!(
        brands,
        [g::local::Brand {
            origin: g::local::Origin::Caller {
                parameter: o::declarations::LocalId(0),
                ..
            },
            expired: false
        }]
    ));
}

#[test]
fn pending_mutation_effects_cannot_turn_stale_pre_call_facts_into_a_rejection() {
    let source = "fn set(target: &mut arena i64, input: arena i64) -> i64 { (*target) = input; return 0; } fn f() -> i64 { arena one(core::allocator()) { let mut a: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); arena two(core::allocator()) { let mut b: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(two,2)); let mut selected: arena i64 = a; let mut called: i64 = set(selected,b); let mut changed: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(two,selected,3); return called; } } }";
    let input = loaned(checked_table(source));
    let plan = g::prepare(&input, Default::default()).unwrap();
    let local = g::local::analyze(&plan, Default::default()).unwrap();
    assert!(local.functions()[0].pending_calls().is_empty());
    assert_eq!(local.functions()[1].pending_calls().len(), 1);
}

#[test]
fn a_cfg_join_keeps_the_possibility_of_a_different_arena_brand() {
    let source = "fn f(flag: bool) -> i64 { arena one(core::allocator()) { let mut a: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(one,1)); arena two(core::allocator()) { let mut b: arena i64 = core::result_unwrap::<arena i64,core::ArenaError>(core::arena_new::<i64>(two,2)); let mut selected: arena i64 = b; if flag { selected = b; } let mut result: core::Result<core::Unit,core::ArenaError> = core::arena_replace::<i64>(two,selected,3); return 0; } } }";
    let mut table = checked_table(source);
    let locals = &table.places.declarations.functions[0].locals;
    let find =
        |name| o::declarations::LocalId(locals.iter().position(|l| l.name == name).unwrap() as u32);
    let (a, b) = (find("a"), find("b"));
    let places = &mut table.places.functions[0].places;
    let start = source.find("if flag").unwrap();
    let end = source.find("let mut result").unwrap();
    let mut edits = 0;
    for block in &mut table.functions[0].blocks {
        for i in &mut block.instructions {
            if let b::Op::CopyPlace(place) = &mut i.op
                && places[place.0 as usize].root == p::Root::Local(b)
                && (i.span.start as usize) >= start
                && (i.span.end as usize) <= end
            {
                places[place.0 as usize].root = p::Root::Local(a);
                edits += 1;
            }
        }
    }
    assert_eq!(edits, 1);
    let input = loaned(table);
    let plan = g::prepare(&input, Default::default()).unwrap();
    let error = g::local::analyze(&plan, Default::default()).err().unwrap();
    assert!(
        matches!(error.kind, g::local::ErrorKind::CrossArena { .. }),
        "{error:?}"
    );
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
