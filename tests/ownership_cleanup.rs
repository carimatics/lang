//! Execute only the straight-line ownership transfer/cleanup subset below.
//! Actual shared resources test the source→MIR→cleanup connection; this helper
//! is not the production ownership VM or a general expression interpreter.
use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, places as p},
};
use lang_runtime::cleanup::{
    Completion, Exit, FailureKind, Instance, Kind, Limits, MirLayout, Reason, StateError,
};
use lang_runtime::shared::{Payload, SharedHeap, Strong};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

fn checked(owning_return: bool) -> Box<OwnershipProgram> {
    let text = format!(
        "fn f(a: shared i64, b: shared i64, c: shared i64) -> {} {{
            let mut x: shared i64 = a;
            let mut y: shared i64 = b;
            let mut moved: shared i64 = x;
            x = c;
            return {};
        }}",
        if owning_return { "shared i64" } else { "i64" },
        if owning_return { "moved" } else { "0" },
    );
    let mut sources = SourceMap::new();
    lang_syntax::add_source(&mut sources, "unused.lang", b"unused", Default::default()).unwrap();
    let analyzed = frontend::analyze_virtual(
        sources,
        Path::new("cleanup.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analyzed.diagnostics.is_empty(),
        "{:?}",
        analyzed.diagnostics
    );
    let Program::Ownership(program) = analyzed.result.unwrap() else {
        panic!("ownership profile");
    };
    program
}
fn local(place: p::PlaceId, typed: &b::TypedBodies) -> d::LocalId {
    let place = &typed.places().functions()[0].places[place.0 as usize];
    assert!(place.projections.is_empty());
    let p::Root::Local(local) = place.root else {
        panic!("whole local required");
    };
    local
}
fn execute(
    owning_return: bool,
    fail: Option<u8>,
) -> (Completion<Strong>, Vec<(u8, Reason)>, SharedHeap) {
    let program = checked(owning_return);
    let typed = program.mir().activity().moved().typed();
    let layout = MirLayout::new(typed, m::FunctionId(0), 5).unwrap();
    let declaration = &typed.places().declarations().functions()[0];
    let values = &typed.places().functions()[0].values;
    let heap = SharedHeap::default();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let events = trace.clone();
    let mut instance = Instance::default();
    let mut frame = instance
        .enter(
            Limits {
                slots: layout.end(),
                ..Limits::default()
            },
            move |owner: &mut Strong, reason| {
                let id = owner.read().unwrap().bytes()[0];
                events.borrow_mut().push((id, reason));
                if fail == Some(id) {
                    Err(u32::from(id))
                } else {
                    Ok(())
                }
            },
        )
        .unwrap();
    for target in (0..declaration.locals.len())
        .map(|i| b::CleanupTarget::Local(d::LocalId(i as u32)))
        .chain((0..values.len()).map(|i| b::CleanupTarget::Value(m::Register(i as u32))))
    {
        if let Some((slot, kind)) = layout.slot(target).unwrap() {
            frame.declare(slot, kind).unwrap();
        }
    }
    for (i, local) in declaration.parameters.iter().enumerate() {
        let owner = heap
            .allocate(
                Payload {
                    bytes: vec![(i + 1) as u8],
                    edges: vec![],
                },
                None,
            )
            .unwrap();
        let (slot, _) = layout
            .slot(b::CleanupTarget::Local(*local))
            .unwrap()
            .unwrap();
        frame
            .initialize(slot, owner, Some(declaration.locals[local.0 as usize].span))
            .unwrap();
    }
    let slot = |target| layout.slot(target).unwrap().unwrap().0;
    let body = &typed.functions()[0];
    assert_eq!(body.blocks.len(), 1);
    let block = &body.blocks[0];
    let mut prepared = vec![None; body.prepared];
    let mut scalar = vec![None; values.len()];
    for instruction in &block.instructions {
        match &instruction.op {
            b::Op::I64(value) => scalar[instruction.results[0].0 as usize] = Some(*value),
            b::Op::MovePlace(place) => {
                let from = slot(b::CleanupTarget::Local(local(*place, typed)));
                let to = slot(b::CleanupTarget::Value(instruction.results[0]));
                frame.move_value(from, to).unwrap();
            }
            b::Op::Move(value) => {
                frame
                    .move_value(
                        slot(b::CleanupTarget::Value(*value)),
                        slot(b::CleanupTarget::Value(instruction.results[0])),
                    )
                    .unwrap();
            }
            b::Op::Initialize { local, value } => {
                frame
                    .move_value(
                        slot(b::CleanupTarget::Value(*value)),
                        slot(b::CleanupTarget::Local(*local)),
                    )
                    .unwrap();
            }
            b::Op::PreparePlace { destination, place } => {
                prepared[destination.0 as usize] =
                    Some(slot(b::CleanupTarget::Local(local(*place, typed))))
            }
            b::Op::ReplacePrepared { destination, value } => {
                let from = slot(b::CleanupTarget::Value(*value));
                let to = prepared[destination.0 as usize].take().unwrap();
                match frame.value(to) {
                    Err(StateError::Uninitialized) => frame.move_value(from, to).unwrap(),
                    Ok(_) => frame.replace(from, to).unwrap(),
                    Err(error) => panic!("unexpected destination state: {error}"),
                }
            }
            b::Op::Cleanup(cleanup) => {
                if let Err(error) = frame.cleanup_mir(cleanup, &layout) {
                    assert_eq!(error, StateError::Failed);
                    break;
                }
            }
            other => panic!("outside the tested transfer/cleanup subset: {other:?}"),
        }
    }
    let b::Term::Return(result) = block.terminator.kind else {
        panic!("straight-line return");
    };
    let returned = layout
        .slot(b::CleanupTarget::Value(result))
        .unwrap()
        .map(|(slot, _)| slot);
    if !owning_return {
        assert_eq!(scalar[result.0 as usize], Some(0));
    }
    let done = frame.finish(Exit::Return(returned));
    let trace = trace.borrow().clone();
    (done, trace, heap)
}

#[test]
fn source_cleanup_ranges_use_actual_reinitialization_order_for_real_shared_values() {
    let (done, trace, heap) = execute(false, None);
    // Declaration-reverse individual drops would produce [1, 2, 3].
    assert_eq!(
        trace.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [3, 1, 2]
    );
    assert!(done.report.primary().is_none());
    assert!(!done.poisoned);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
    assert_eq!(heap.stats().unwrap().reclaimed_nodes, 3);
}

#[test]
fn source_owning_return_is_published_only_after_mir_cleanup_succeeds() {
    let (done, trace, heap) = execute(true, None);
    assert_eq!(trace.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [3, 2]);
    assert_eq!(done.value.as_ref().unwrap().read().unwrap().bytes(), [1]);
    assert_eq!(heap.stats().unwrap().live_nodes, 1);
    drop(done.value);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);

    let (done, trace, heap) = execute(true, Some(3));
    assert!(done.value.is_none());
    assert_eq!(done.report.primary().unwrap().kind, FailureKind::Cleanup(3));
    assert!(done.poisoned);
    assert_eq!(
        trace,
        [
            (3, Reason::ScopeExit),
            (2, Reason::ScopeExit),
            (1, Reason::UnpublishedReturn)
        ]
    );
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
}

#[test]
fn cleanup_metadata_and_runtime_mapping_reject_malformed_targets_before_effects() {
    let program = checked(false);
    let typed = program.mir().activity().moved().typed();
    assert!(MirLayout::new(typed, m::FunctionId(999), 0).is_none());
    assert!(MirLayout::new(typed, m::FunctionId(0), usize::MAX).is_none());
    let layout = MirLayout::new(typed, m::FunctionId(0), 0).unwrap();
    let (slot, _) = layout
        .slot(b::CleanupTarget::Local(d::LocalId(0)))
        .unwrap()
        .unwrap();
    let span = typed.places().declarations().functions()[0].span;
    let mut instance = Instance::default();
    let calls = RefCell::new(0);
    let mut frame = instance
        .enter(
            Limits {
                slots: layout.end(),
                ..Limits::default()
            },
            |_: &mut u32, _| {
                *calls.borrow_mut() += 1;
                Ok(())
            },
        )
        .unwrap();
    frame.declare(slot, Kind::Owner).unwrap();
    frame.initialize(slot, 7, Some(span)).unwrap();
    for target in [
        b::CleanupTarget::Local(d::LocalId(999)),
        b::CleanupTarget::Value(m::Register(999)),
        b::CleanupTarget::Loan(b::LoanId(999)),
        b::CleanupTarget::Arena(d::LocalId(0)),
    ] {
        let cleanup = b::Cleanup {
            steps: vec![
                b::CleanupStep {
                    target: b::CleanupTarget::Local(d::LocalId(0)),
                    span,
                },
                b::CleanupStep { target, span },
            ],
        };
        assert!(frame.cleanup_mir(&cleanup, &layout).is_err());
        assert_eq!(*frame.value(slot).unwrap(), 7);
        assert_eq!(*calls.borrow(), 0);
        let mut table = typed.clone().into_table();
        table.functions[0].blocks[0]
            .instructions
            .push(b::Instruction {
                results: vec![],
                op: b::Op::Cleanup(cleanup),
                span,
            });
        assert!(table.verify(Default::default()).is_err());
    }
    frame.finish(Exit::Return(None));
    assert_eq!(*calls.borrow(), 1);
}

#[test]
fn cleanup_metadata_preserves_source_spans_and_exact_resource_bounds() {
    let program = checked(false);
    let typed = program.mir().activity().moved().typed();
    for instruction in &typed.functions()[0].blocks[0].instructions {
        if let b::Op::Cleanup(cleanup) = &instruction.op {
            for step in &cleanup.steps {
                let source = &typed.places().declarations().types().table().sources
                    [step.span.source.0 as usize];
                assert!(source.path.ends_with("cleanup.lang"));
                assert!(
                    source
                        .text
                        .get(step.span.start as usize..step.span.end as usize)
                        .is_some()
                );
            }
        }
    }
    let table = typed.clone().into_table();
    let replay = table.clone().verify(Default::default()).unwrap();
    for (kind, used) in [
        (m::LimitKind::Work, replay.work_used()),
        (m::LimitKind::Allocations, replay.requested_bytes()),
        (m::LimitKind::Elements, replay.elements_used()),
    ] {
        let mut limits = b::Limits::default();
        let bound = match kind {
            m::LimitKind::Work => &mut limits.places.ownership.unit.work,
            m::LimitKind::Allocations => &mut limits.places.ownership.unit.allocations,
            _ => &mut limits.places.ownership.unit.elements,
        };
        *bound = used;
        table.clone().verify(limits).unwrap();
        match kind {
            m::LimitKind::Work => limits.places.ownership.unit.work -= 1,
            m::LimitKind::Allocations => limits.places.ownership.unit.allocations -= 1,
            _ => limits.places.ownership.unit.elements -= 1,
        }
        assert!(matches!(table.clone().verify(limits).unwrap_err().kind,
            o::ErrorKind::Mir(m::ErrorKind::Limit { kind: actual, .. }) if actual == kind));
    }
    for bad_span in [
        m::Span {
            source: m::SourceId(99),
            start: 0,
            end: 1,
        },
        m::Span {
            source: m::SourceId(0),
            start: 1,
            end: 0,
        },
        m::Span {
            source: m::SourceId(0),
            start: 0,
            end: u32::MAX,
        },
    ] {
        let mut malformed = table.clone();
        let cleanup = malformed.functions[0].blocks[0]
            .instructions
            .iter_mut()
            .find_map(|i| {
                if let b::Op::Cleanup(cleanup) = &mut i.op {
                    Some(cleanup)
                } else {
                    None
                }
            })
            .unwrap();
        cleanup.steps[0].span = bad_span;
        assert!(malformed.verify(Default::default()).is_err());
    }
}
