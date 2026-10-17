//! Exercise generated failure control flow with a limited transfer backend and
//! real shared owners. This is not the complete ownership instruction VM.
use lang::frontend;
use lang_diagnostics::{SourceMap, codes};
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, failure as f, places as p},
};
use lang_runtime::cleanup::{
    Cause, Completion, ExecutionFailure, Instance, Kind, Limits, MirLayout, Reason, StateError,
};
use lang_runtime::shared::{Payload, SharedHeap, Strong};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::{cell::RefCell, path::Path, rc::Rc};

const RETURN: &str = "fn f(x: shared i64, y: shared i64) -> shared i64 { return x; }";
fn checked(text: &str) -> Box<OwnershipProgram> {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("failure.lang"),
        text.as_bytes(),
        None,
        Default::default(),
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
}
#[derive(Clone, Copy, Default)]
struct Config {
    fuel: usize,
    fail_site: Option<b::Site>,
    reported: bool,
    cleanup_fails: bool,
    entry_fails: bool,
    force_block: Option<m::BlockId>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Event {
    Execute(b::Site),
    Stage,
    ClearPrepared,
    Retire,
    DiscardResult,
    Drop(u8, Reason),
}
struct Run {
    done: Completion<Strong>,
    heap: SharedHeap,
    incoming: Vec<Option<Strong>>,
    events: Vec<Event>,
    consumed: usize,
}
fn run(program: &OwnershipProgram, config: Config) -> Run {
    let typed = program.mir().activity().moved().typed();
    let flow = &program.failure_flow().functions()[0];
    let declaration = &typed.places().declarations().functions()[0];
    let body = &typed.functions()[0];
    let layout = MirLayout::new(typed, m::FunctionId(0), 3).unwrap();
    let heap = SharedHeap::default();
    let mut incoming: Vec<_> = (1..=declaration.parameters.len())
        .map(|id| {
            Some(
                heap.allocate(
                    Payload {
                        bytes: vec![id as u8],
                        edges: vec![],
                    },
                    None,
                )
                .unwrap(),
            )
        })
        .collect();
    let events = Rc::new(RefCell::new(Vec::new()));
    let trace = events.clone();
    let mut instance = Instance::default();
    let mut frame = instance
        .enter(
            Limits {
                slots: layout.end(),
                ..Limits::default()
            },
            move |owner: &mut Strong, reason| {
                let id = owner.read().unwrap().bytes()[0];
                trace.borrow_mut().push(Event::Drop(id, reason));
                if config.cleanup_fails {
                    Err(u32::from(id))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
    for target in (0..declaration.locals.len())
        .map(|i| b::CleanupTarget::Local(o::declarations::LocalId(i as u32)))
        .chain(
            (0..typed.places().functions()[0].values.len())
                .map(|i| b::CleanupTarget::Value(m::Register(i as u32))),
        )
    {
        if let Some((slot, kind)) = layout.slot(target).unwrap() {
            assert_eq!(kind, Kind::Owner);
            frame.declare(slot, kind).unwrap();
        }
    }
    let slot = |target| layout.slot(target).unwrap().unwrap().0;
    let local = |place: p::PlaceId| {
        let place = &typed.places().functions()[0].places[place.0 as usize];
        assert!(place.projections.is_empty());
        let p::Root::Local(local) = place.root else {
            panic!("whole local transfer");
        };
        slot(b::CleanupTarget::Local(local))
    };
    let mut prepared = vec![None; body.prepared];
    let mut booleans = vec![None; typed.places().functions()[0].values.len()];
    let mut pc = flow.entry();
    let mut fuel = config.fuel;
    let mut consumed = 0;
    let mut cause = None;
    for _ in 0..2000 {
        let node = flow.nodes()[pc.0 as usize];
        match node.kind {
            f::Kind::Entry { next, failure } => {
                if config.entry_fails {
                    cause = Some((Cause::Execution(ExecutionFailure::FrameLimit), node.span));
                    pc = failure;
                } else {
                    for (parameter, value) in declaration.parameters.iter().zip(&mut incoming) {
                        frame
                            .initialize(
                                slot(b::CleanupTarget::Local(*parameter)),
                                value.take().unwrap(),
                                Some(declaration.locals[parameter.0 as usize].span),
                            )
                            .unwrap();
                    }
                    pc = config
                        .force_block
                        .map_or(next, |block| flow.source_entries()[block.0 as usize]);
                }
            }
            f::Kind::Fuel { next, failure, .. } => {
                if fuel == 0 {
                    cause = Some((Cause::Execution(ExecutionFailure::FuelLimit), node.span));
                    pc = failure;
                } else {
                    fuel -= 1;
                    consumed += 1;
                    pc = next;
                }
            }
            f::Kind::Execute {
                site,
                next,
                failure,
            } => {
                events.borrow_mut().push(Event::Execute(site));
                if config.fail_site == Some(site) {
                    cause = Some((
                        if config.reported {
                            Cause::Reported
                        } else {
                            Cause::Execution(ExecutionFailure::Fault)
                        },
                        node.span,
                    ));
                    pc = failure;
                    continue;
                }
                let instruction = &body.blocks[site.block.0 as usize].instructions[site.order - 1];
                let status = match &instruction.op {
                    b::Op::MovePlace(place) => frame.move_value(
                        local(*place),
                        slot(b::CleanupTarget::Value(instruction.results[0])),
                    ),
                    b::Op::Initialize { local, value } => frame.move_value(
                        slot(b::CleanupTarget::Value(*value)),
                        slot(b::CleanupTarget::Local(*local)),
                    ),
                    b::Op::PreparePlace { destination, place } => {
                        prepared[destination.0 as usize] = Some(local(*place));
                        Ok(())
                    }
                    b::Op::ReplacePrepared { destination, value } => {
                        let target = prepared[destination.0 as usize].take().unwrap();
                        frame.replace(slot(b::CleanupTarget::Value(*value)), target)
                    }
                    b::Op::Bool(value) => {
                        booleans[instruction.results[0].0 as usize] = Some(*value);
                        Ok(())
                    }
                    b::Op::Cleanup(cleanup) => frame.cleanup_mir(cleanup, &layout),
                    other => panic!("outside the tested transfer backend: {other:?}"),
                };
                match status {
                    Ok(()) => pc = next,
                    Err(StateError::Failed) => {
                        cause = Some((Cause::Reported, node.span));
                        pc = failure;
                    }
                    Err(error) => panic!("invalid checked transfer: {error}"),
                }
            }
            f::Kind::Transfer {
                block, arm, target, ..
            } => {
                let edge = body.blocks[block.0 as usize]
                    .terminator
                    .kind
                    .targets()
                    .nth(arm as usize)
                    .unwrap();
                assert!(edge.arguments.is_empty());
                assert_eq!(target, flow.source_entries()[edge.block.0 as usize]);
                pc = target;
            }
            f::Kind::Branch {
                condition,
                then_node,
                else_node,
                ..
            } => {
                pc = if booleans[condition.0 as usize].unwrap() {
                    then_node
                } else {
                    else_node
                }
            }
            f::Kind::StageReturn { value, next, .. } => {
                events.borrow_mut().push(Event::Stage);
                frame
                    .stage_return(
                        layout
                            .slot(b::CleanupTarget::Value(value))
                            .unwrap()
                            .map(|(slot, _)| slot),
                    )
                    .unwrap();
                pc = next;
            }
            f::Kind::DiscardPrepared { next } => {
                prepared.fill(None);
                events.borrow_mut().push(Event::ClearPrepared);
                pc = next;
            }
            f::Kind::RetireReturn { next, failure } => {
                events.borrow_mut().push(Event::Retire);
                if frame.retire_exit().is_ok() {
                    pc = next;
                } else {
                    cause = Some((Cause::Reported, node.span));
                    pc = failure;
                }
            }
            f::Kind::PublishReturn => {
                let done = frame.finish_staged_return();
                let events = events.borrow().clone();
                return Run {
                    done,
                    heap,
                    incoming,
                    events,
                    consumed,
                };
            }
            f::Kind::Raise { failure, next } => {
                cause = Some((Cause::Execution(failure), node.span));
                pc = next;
            }
            f::Kind::RecordFailure { next } => {
                let (cause, span) = cause.take().unwrap();
                frame.begin_failure(cause, Some(span)).unwrap();
                pc = next;
            }
            f::Kind::RetireFailure { next } => {
                events.borrow_mut().push(Event::Retire);
                assert!(matches!(
                    frame.retire_exit(),
                    Ok(()) | Err(StateError::Failed)
                ));
                pc = next;
            }
            f::Kind::DiscardResult { next } => {
                events.borrow_mut().push(Event::DiscardResult);
                frame.discard_result().unwrap();
                pc = next;
            }
            f::Kind::PropagateFailure => {
                let done = frame.finish_reported_failure();
                let events = events.borrow().clone();
                return Run {
                    done,
                    heap,
                    incoming,
                    events,
                    consumed,
                };
            }
        }
    }
    panic!("generated control flow did not terminate");
}
fn dropped(run: &Run) -> Vec<(u8, Reason)> {
    run.events
        .iter()
        .filter_map(|e| {
            if let Event::Drop(id, reason) = e {
                Some((*id, *reason))
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn generated_return_stages_before_cleanup_and_failure_paths_are_explicit() {
    let program = checked(RETURN);
    let flow = &program.failure_flow().functions()[0];
    let typed = program.mir().activity().moved().typed();
    for node in flow.nodes() {
        assert!(
            node.kind
                .targets()
                .all(|target| (target.0 as usize) < flow.nodes().len())
        );
        let source =
            &typed.places().declarations().types().table().sources[node.span.source.0 as usize];
        assert!(
            source
                .text
                .get(node.span.start as usize..node.span.end as usize)
                .is_some()
        );
    }
    let stage = flow
        .nodes()
        .iter()
        .position(|n| matches!(n.kind, f::Kind::StageReturn { .. }))
        .unwrap();
    let cleanup = flow.nodes().iter().position(|n| matches!(n.kind, f::Kind::Execute { site, .. } if matches!(typed.functions()[0].blocks[site.block.0 as usize].instructions[site.order - 1].op, b::Op::Cleanup(_)))).unwrap();
    assert!(stage < cleanup);
    assert!(matches!(
        flow.nodes()[stage - 1].kind,
        f::Kind::Fuel {
            position: f::Position::Terminator(_),
            ..
        }
    ));
    assert!(matches!(
        flow.nodes()[1].kind,
        f::Kind::RecordFailure { .. }
    ));
    assert!(matches!(
        flow.nodes()[2].kind,
        f::Kind::DiscardPrepared { .. }
    ));
    assert!(matches!(
        flow.nodes()[3].kind,
        f::Kind::RetireFailure { .. }
    ));
    assert!(matches!(
        flow.nodes()[4].kind,
        f::Kind::DiscardResult { .. }
    ));
    assert_eq!(flow.nodes()[5].kind, f::Kind::PropagateFailure);
}

#[test]
fn fuel_failures_keep_live_owners_and_cleanup_consumes_no_guest_fuel() {
    let program = checked(RETURN);
    let before = run(&program, Config::default());
    assert_eq!(before.consumed, 0);
    assert_eq!(
        dropped(&before),
        [(2, Reason::Failure), (1, Reason::Failure)]
    );
    assert_eq!(
        before.done.report.primary().unwrap().kind,
        lang_runtime::cleanup::FailureKind::Execution(ExecutionFailure::FuelLimit)
    );
    assert!(!before.done.poisoned);
    let after_expression = run(
        &program,
        Config {
            fuel: 1,
            ..Default::default()
        },
    );
    assert_eq!(
        dropped(&after_expression),
        [(1, Reason::Failure), (2, Reason::Failure)]
    );
    assert!(!after_expression.events.contains(&Event::Stage));
    let success = run(
        &program,
        Config {
            fuel: 2,
            ..Default::default()
        },
    );
    assert_eq!(success.consumed, 2);
    assert_eq!(dropped(&success), [(2, Reason::ScopeExit)]);
    assert!(success.done.report.primary().is_none());
    assert_eq!(
        success.done.value.as_ref().unwrap().read().unwrap().bytes(),
        [1]
    );
    drop(success.done.value);
    assert_eq!(success.heap.stats().unwrap().live_nodes, 0);
}

#[test]
fn generated_cleanup_failure_preserves_primary_and_discards_escrow_last() {
    let program = checked(RETURN);
    let result = run(
        &program,
        Config {
            fuel: 2,
            cleanup_fails: true,
            ..Default::default()
        },
    );
    assert!(result.done.value.is_none());
    assert_eq!(
        dropped(&result),
        [(2, Reason::ScopeExit), (1, Reason::UnpublishedReturn)]
    );
    assert_eq!(
        result.done.report.primary().unwrap().kind,
        lang_runtime::cleanup::FailureKind::Cleanup(2)
    );
    assert_eq!(
        result
            .done
            .report
            .secondary()
            .iter()
            .map(|r| r.kind)
            .collect::<Vec<_>>(),
        [lang_runtime::cleanup::FailureKind::Cleanup(1)]
    );
    assert!(result.done.poisoned);
    assert_eq!(result.heap.stats().unwrap().live_nodes, 0);
}

#[test]
fn failed_replacement_discards_capture_before_cleaning_old_and_incoming_values() {
    let program = checked(
        "fn f(x: shared i64, y: shared i64) -> shared i64 { let mut item: shared i64 = x; item = y; return item; }",
    );
    let typed = program.mir().activity().moved().typed();
    let site = typed.functions()[0].blocks[0]
        .instructions
        .iter()
        .enumerate()
        .find_map(|(i, instruction)| {
            matches!(instruction.op, b::Op::ReplacePrepared { .. }).then_some(b::Site {
                block: m::BlockId(0),
                order: i + 1,
            })
        })
        .unwrap();
    let result = run(
        &program,
        Config {
            fuel: 100,
            fail_site: Some(site),
            ..Default::default()
        },
    );
    assert_eq!(
        dropped(&result),
        [(2, Reason::Failure), (1, Reason::Failure)]
    );
    let cleared = result
        .events
        .iter()
        .position(|e| *e == Event::ClearPrepared)
        .unwrap();
    let first_drop = result
        .events
        .iter()
        .position(|e| matches!(e, Event::Drop(..)))
        .unwrap();
    assert!(cleared < first_drop);
    assert_eq!(
        result.done.report.primary().unwrap().kind,
        lang_runtime::cleanup::FailureKind::Execution(ExecutionFailure::Fault)
    );
    assert!(!result.done.poisoned);
    assert_eq!(result.heap.stats().unwrap().live_nodes, 0);
}

#[test]
fn entry_preflight_retains_caller_arguments_and_reported_without_report_is_invariant() {
    let program = checked(RETURN);
    let refused = run(
        &program,
        Config {
            fuel: 100,
            entry_fails: true,
            ..Default::default()
        },
    );
    assert_eq!(refused.consumed, 0);
    assert!(refused.incoming.iter().all(Option::is_some));
    assert!(
        refused
            .events
            .iter()
            .all(|e| !matches!(e, Event::Execute(_) | Event::Drop(..)))
    );
    assert_eq!(refused.heap.stats().unwrap().live_nodes, 2);
    drop(refused.incoming);
    assert_eq!(refused.heap.stats().unwrap().live_nodes, 0);
    let result = run(
        &program,
        Config {
            fuel: 100,
            fail_site: Some(b::Site {
                block: m::BlockId(0),
                order: 1,
            }),
            reported: true,
            ..Default::default()
        },
    );
    assert_eq!(
        result.done.report.primary().unwrap().kind,
        lang_runtime::cleanup::FailureKind::Execution(ExecutionFailure::Invariant)
    );
    assert!(result.done.poisoned);
}

#[test]
fn generated_branches_retain_source_edges_and_exact_guest_step_charges() {
    for condition in ["true", "false"] {
        let program = checked(&format!(
            "fn f(x: shared i64, y: shared i64) -> shared i64 {{ if {condition} {{ return x; }} else {{ return x; }} }}"
        ));
        let result = run(
            &program,
            Config {
                fuel: 4,
                ..Default::default()
            },
        );
        assert_eq!(result.consumed, 4);
        assert!(result.done.report.primary().is_none());
        assert_eq!(
            result.done.value.as_ref().unwrap().read().unwrap().bytes(),
            [1]
        );
    }
    let program =
        checked("fn f(x: shared i64, y: shared i64) -> shared i64 { while false {} return x; }");
    let result = run(
        &program,
        Config {
            fuel: 5,
            ..Default::default()
        },
    );
    assert_eq!(result.consumed, 5);
    assert!(result.done.report.primary().is_none());
    let program =
        checked("fn f(x: shared i64, y: shared i64) -> shared i64 { while true {} return x; }");
    let result = run(
        &program,
        Config {
            fuel: 7,
            ..Default::default()
        },
    );
    assert_eq!(result.consumed, 7);
    assert_eq!(
        dropped(&result),
        [(2, Reason::Failure), (1, Reason::Failure)]
    );
    assert_eq!(
        result.done.report.primary().unwrap().kind,
        lang_runtime::cleanup::FailureKind::Execution(ExecutionFailure::FuelLimit)
    );
}

#[test]
fn unreachable_paths_raise_contained_invariants_instead_of_publishing_a_value() {
    let program = checked(
        "fn f(x: shared i64, y: shared i64) -> shared i64 { match true { true => { return x; }, false => { return x; }, } }",
    );
    let flow = &program.failure_flow().functions()[0];
    assert!(flow.nodes().iter().any(|node| node.kind
        == f::Kind::Raise {
            failure: ExecutionFailure::Invariant,
            next: flow.failure()
        }));
    let typed = program.mir().activity().moved().typed();
    let unreachable = typed.functions()[0]
        .blocks
        .iter()
        .position(|block| block.terminator.kind == b::Term::Unreachable)
        .unwrap();
    let result = run(
        &program,
        Config {
            fuel: 1,
            cleanup_fails: true,
            force_block: Some(m::BlockId(unreachable as u32)),
            ..Default::default()
        },
    );
    assert_eq!(
        result.done.report.primary().unwrap().kind,
        lang_runtime::cleanup::FailureKind::Execution(ExecutionFailure::Invariant)
    );
    assert_eq!(
        result
            .done
            .report
            .secondary()
            .iter()
            .map(|record| record.kind)
            .collect::<Vec<_>>(),
        [
            lang_runtime::cleanup::FailureKind::Cleanup(2),
            lang_runtime::cleanup::FailureKind::Cleanup(1)
        ]
    );
    assert_eq!(
        dropped(&result),
        [(2, Reason::Failure), (1, Reason::Failure)]
    );
    assert!(result.done.poisoned);
}

#[test]
fn flow_budgets_are_cumulative_exact_replayable_and_diagnose_original_source() {
    let program = checked(RETURN);
    let generated = program.failure_flow();
    assert_eq!(
        *generated,
        f::elaborate(program.mir().clone(), Default::default()).unwrap()
    );
    for (kind, used) in [
        (m::LimitKind::Work, generated.work_used()),
        (m::LimitKind::Allocations, generated.requested_bytes()),
        (m::LimitKind::Elements, generated.elements_used()),
    ] {
        let mut limits = f::Limits::default();
        match kind {
            m::LimitKind::Work => limits.work = used,
            m::LimitKind::Allocations => limits.allocations = used,
            _ => limits.elements = used,
        }
        f::elaborate(program.mir().clone(), limits).unwrap();
        match kind {
            m::LimitKind::Work => limits.work -= 1,
            m::LimitKind::Allocations => limits.allocations -= 1,
            _ => limits.elements -= 1,
        }
        assert!(
            matches!(f::elaborate(program.mir().clone(), limits).unwrap_err().kind, f::ErrorKind::Resource(m::ErrorKind::Limit { kind: actual, .. }) if actual == kind)
        );
    }
    let count = generated
        .functions()
        .iter()
        .map(|function| function.nodes().len())
        .sum();
    f::elaborate(
        program.mir().clone(),
        f::Limits {
            nodes: count,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(matches!(
        f::elaborate(
            program.mir().clone(),
            f::Limits {
                nodes: count - 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind,
        f::ErrorKind::Nodes { .. }
    ));
    let mut limits = frontend::CompileLimits::default();
    limits.ownership.failure.nodes = 0;
    let mut sources = SourceMap::new();
    lang_syntax::add_source(&mut sources, "unused.lang", b"unused", Default::default()).unwrap();
    let analysis = frontend::analyze_virtual(
        sources,
        Path::new("failure.lang"),
        RETURN.as_bytes(),
        None,
        limits,
        None,
    );
    assert!(analysis.result.is_err());
    assert_eq!(analysis.diagnostics.len(), 1);
    assert_eq!(analysis.diagnostics[0].code, codes::RESOURCE_LIMIT);
    analysis.diagnostics[0].validate(&analysis.sources).unwrap();
    let copy = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("copy.lang"),
        b"fn f() -> i64 { return 1; }",
        None,
        limits,
        None,
    );
    assert!(matches!(copy.result, Ok(Program::Copy(_))));
}
