//! Controlled record/transfer interpreter for cleanup integration, using real
//! shared owners. This does not provide general ownership VM execution.
use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_mir::unit::{
    self as m,
    ownership::{self as o, bodies as b, declarations as d, places as p},
};
use lang_runtime::cleanup::{
    Exit, Instance, Kind, Limits, MirLayout, RecordOrder, Slot, StateError,
};
use lang_runtime::shared::{Payload, SharedHeap, Strong};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

type Trace = Rc<RefCell<Vec<u8>>>;
fn checked(text: &str) -> Box<OwnershipProgram> {
    let mut sources = SourceMap::new();
    lang_syntax::add_source(&mut sources, "unused.lang", b"unused", Default::default()).unwrap();
    let analysis = frontend::analyze_virtual(
        sources,
        Path::new("record.lang"),
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
        panic!("ownership profile");
    };
    program
}
fn source(order: &[usize]) -> String {
    let fields = ["first: a", "second: b", "third: c"];
    format!(
        "struct Bundle {{ first: shared i64, second: shared i64, third: shared i64 }}
        fn f(a: shared i64, b: shared i64, c: shared i64) -> Bundle {{ return Bundle {{ {} }}; }}",
        order
            .iter()
            .map(|&i| fields[i])
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn sites(typed: &b::TypedBodies) -> Vec<b::Site> {
    typed.functions()[0].blocks[0]
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(i, instruction)| {
            matches!(instruction.op, b::Op::Record { .. }).then_some(b::Site {
                block: m::BlockId(0),
                order: i + 1,
            })
        })
        .collect()
}
#[derive(Debug)]
enum Value<'a> {
    Leaf {
        owner: Strong,
        trace: Trace,
    },
    Record {
        fields: Vec<Option<Value<'a>>>,
        order: RecordOrder<'a>,
    },
}
impl Drop for Value<'_> {
    fn drop(&mut self) {
        match self {
            Self::Leaf { owner, trace } => {
                trace.borrow_mut().push(owner.read().unwrap().bytes()[0])
            }
            Self::Record { fields, order } => {
                for (_, field) in order.drain(fields).unwrap() {
                    drop(field);
                }
            }
        }
    }
}
fn node(heap: &SharedHeap, id: u8) -> Strong {
    heap.allocate(
        Payload {
            bytes: vec![id],
            edges: vec![],
        },
        None,
    )
    .unwrap()
}
fn materialize<'a>(typed: &'a b::TypedBodies, trace: &Trace) -> (Value<'a>, SharedHeap) {
    let layout = MirLayout::new(typed, m::FunctionId(0), 0).unwrap();
    let declaration = &typed.places().declarations().functions()[0];
    let body = &typed.functions()[0];
    assert_eq!(body.blocks.len(), 1);
    let instructions = &body.blocks[0].instructions;
    // All test payload storage is reserved before entry. The constructor below
    // transfers completed temporaries without allocating during publication.
    let mut storage: Vec<Option<Vec<Option<Value<'a>>>>> = instructions
        .iter()
        .map(|i| {
            if let b::Op::Record { fields, .. } = &i.op {
                Some(std::iter::repeat_with(|| None).take(fields.len()).collect())
            } else {
                None
            }
        })
        .collect();
    let heap = SharedHeap::default();
    let mut instance = Instance::default();
    let mut frame = instance
        .enter(
            Limits {
                slots: layout.end(),
                ..Limits::default()
            },
            |_: &mut Value<'_>, _| Ok(()),
        )
        .unwrap();
    for target in (0..declaration.locals.len())
        .map(|i| b::CleanupTarget::Local(d::LocalId(i as u32)))
        .chain(
            (0..typed.places().functions()[0].values.len())
                .map(|i| b::CleanupTarget::Value(m::Register(i as u32))),
        )
    {
        if let Some((slot, kind)) = layout.slot(target).unwrap() {
            frame.declare(slot, kind).unwrap();
        }
    }
    let slot = |target| layout.slot(target).unwrap().unwrap().0;
    for (index, parameter) in declaration.parameters.iter().enumerate() {
        frame
            .initialize(
                slot(b::CleanupTarget::Local(*parameter)),
                Value::Leaf {
                    owner: node(&heap, (index + 1) as u8),
                    trace: trace.clone(),
                },
                None,
            )
            .unwrap();
    }
    for (index, instruction) in instructions.iter().enumerate() {
        match &instruction.op {
            b::Op::MovePlace(place) => {
                let place = &typed.places().functions()[0].places[place.0 as usize];
                assert!(place.projections.is_empty());
                let p::Root::Local(local) = place.root else {
                    panic!("whole local");
                };
                frame
                    .move_value(
                        slot(b::CleanupTarget::Local(local)),
                        slot(b::CleanupTarget::Value(instruction.results[0])),
                    )
                    .unwrap();
            }
            b::Op::Record { fields, .. } => {
                let order = RecordOrder::new(
                    typed,
                    m::FunctionId(0),
                    b::Site {
                        block: m::BlockId(0),
                        order: index + 1,
                    },
                )
                .unwrap();
                let mut stored = storage[index].take().unwrap();
                // Physical placement uses declaration order. The separate MIR
                // permutation must survive, including nested aggregate moves.
                for (field, register) in fields.iter().enumerate() {
                    if let Some((from, _)) =
                        layout.slot(b::CleanupTarget::Value(*register)).unwrap()
                    {
                        stored[field] = Some(frame.take_owner(from).unwrap());
                    }
                }
                frame
                    .initialize(
                        slot(b::CleanupTarget::Value(instruction.results[0])),
                        Value::Record {
                            fields: stored,
                            order,
                        },
                        Some(instruction.span),
                    )
                    .unwrap();
            }
            b::Op::Cleanup(cleanup) => frame.cleanup_mir(cleanup, &layout).unwrap(),
            b::Op::Bool(_) | b::Op::I64(_) => {} // Copy fields have no owning obligation.
            other => panic!("outside the tested record/transfer subset: {other:?}"),
        }
    }
    let b::Term::Return(result) = body.blocks[0].terminator.kind else {
        panic!("record return");
    };
    let done = frame.finish(Exit::Return(Some(slot(b::CleanupTarget::Value(result)))));
    assert!(done.report.primary().is_none());
    (done.value.unwrap(), heap)
}

#[test]
fn all_source_field_permutations_survive_physical_placement_moves_and_cleanup() {
    for first in 0..3 {
        for second in 0..3 {
            if second == first {
                continue;
            }
            let third = 3 - first - second;
            let permutation = [first, second, third];
            let program = checked(&source(&permutation));
            let typed = program.mir().activity().moved().typed();
            let order = RecordOrder::new(typed, m::FunctionId(0), sites(typed)[0]).unwrap();
            assert_eq!(
                order.reverse().collect::<Vec<_>>(),
                permutation
                    .into_iter()
                    .rev()
                    .map(|i| i as u32)
                    .collect::<Vec<_>>()
            );
            for (field, name) in ["a", "b", "c"].into_iter().enumerate() {
                let span = order.field_span(field as u32).unwrap();
                let source =
                    &typed.places().declarations().types().table().sources[span.source.0 as usize];
                assert_eq!(&source.text[span.start as usize..span.end as usize], name);
            }
            let trace = Trace::default();
            let (value, heap) = materialize(typed, &trace);
            assert!(trace.borrow().is_empty());
            assert_eq!(heap.stats().unwrap().live_nodes, 3);
            let mut instance = Instance::default();
            let mut caller = instance
                .enter(Limits::default(), |_: &mut Value<'_>, _| Ok(()))
                .unwrap();
            caller.declare(Slot(0), Kind::Owner).unwrap();
            caller.initialize(Slot(0), value, None).unwrap();
            assert!(caller.finish(Exit::Return(None)).report.primary().is_none());
            assert_eq!(
                *trace.borrow(),
                permutation
                    .into_iter()
                    .rev()
                    .map(|i| (i + 1) as u8)
                    .collect::<Vec<_>>()
            );
            assert_eq!(heap.stats().unwrap().live_nodes, 0);
            assert_eq!(heap.stats().unwrap().reclaimed_nodes, 3);
        }
    }
}

#[test]
fn nested_aggregate_order_and_copy_field_holes_do_not_follow_layout_order() {
    let program = checked(
        "struct Inner { first: shared i64, flag: bool, second: shared i64 }
        struct Outer { left: Inner, right: shared i64 }
        fn f(a: shared i64, b: shared i64, c: shared i64) -> Outer {
            return Outer { right: c, left: Inner { second: b, flag: true, first: a } };
        }",
    );
    let typed = program.mir().activity().moved().typed();
    let records = sites(typed);
    assert_eq!(records.len(), 2);
    assert_eq!(
        RecordOrder::new(typed, m::FunctionId(0), records[0])
            .unwrap()
            .reverse()
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        RecordOrder::new(typed, m::FunctionId(0), records[1])
            .unwrap()
            .reverse()
            .collect::<Vec<_>>(),
        [0, 1]
    );
    let trace = Trace::default();
    let (value, heap) = materialize(typed, &trace);
    drop(value);
    assert_eq!(*trace.borrow(), [1, 2, 3]);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);

    // Two values of one nominal type must retain different constructor orders.
    let program = checked(
        "struct Bundle { first: shared i64, second: shared i64, third: shared i64 }
        struct Pair { left: Bundle, right: Bundle }
        fn make(a: shared i64, b: shared i64, c: shared i64,
                d: shared i64, e: shared i64, f: shared i64) -> Pair {
            return Pair {
                left: Bundle { third: c, first: a, second: b },
                right: Bundle { first: d, second: e, third: f }
            };
        }",
    );
    let typed = program.mir().activity().moved().typed();
    let records = sites(typed);
    assert_eq!(records.len(), 3);
    let trace = Trace::default();
    let (value, heap) = materialize(typed, &trace);
    drop(value);
    assert_eq!(*trace.borrow(), [6, 5, 4, 2, 1, 3]);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
}

#[test]
fn partial_field_drains_preserve_remaining_owners_and_reject_wrong_arity() {
    let program = checked(&source(&[2, 0, 1]));
    let typed = program.mir().activity().moved().typed();
    let order = RecordOrder::new(typed, m::FunctionId(0), sites(typed)[0]).unwrap();
    for prefix in 0..=3 {
        let heap = SharedHeap::default();
        let mut fields = vec![None, None, None];
        for &field in &[2, 0, 1][..prefix] {
            fields[field] = Some(node(&heap, (field + 1) as u8));
        }
        let visited: Vec<_> = order
            .drain(&mut fields)
            .unwrap()
            .map(|(field, owner)| {
                drop(owner);
                field
            })
            .collect();
        assert_eq!(
            visited,
            [2, 0, 1][..prefix]
                .iter()
                .rev()
                .map(|i| *i as u32)
                .collect::<Vec<_>>()
        );
        assert!(fields.iter().all(Option::is_none));
        assert_eq!(heap.stats().unwrap().live_nodes, 0);
    }
    let heap = SharedHeap::default();
    let mut fields = vec![
        Some(node(&heap, 1)),
        Some(node(&heap, 2)),
        Some(node(&heap, 3)),
    ];
    assert!(matches!(
        order.drain(&mut fields[..2]),
        Err(StateError::AggregateArity {
            expected: 3,
            actual: 2
        })
    ));
    assert!(fields.iter().all(Option::is_some));
    let mut drain = order.drain(&mut fields).unwrap();
    let (field, owner) = drain.next().unwrap();
    assert_eq!(field, 1);
    drop(owner);
    drop(drain);
    assert!(fields[1].is_none());
    assert!(fields[0].is_some() && fields[2].is_some());
    let remaining: Vec<_> = order
        .drain(&mut fields)
        .unwrap()
        .map(|(field, owner)| {
            drop(owner);
            field
        })
        .collect();
    assert_eq!(remaining, [0, 2]);
    assert_eq!(heap.stats().unwrap().live_nodes, 0);
}

#[test]
fn malformed_record_order_is_rejected_and_resource_bounds_are_exact() {
    let program = checked(&source(&[2, 0, 1]));
    let typed = program.mir().activity().moved().typed();
    let site = sites(typed)[0];
    let table = typed.clone().into_table();
    for order in [
        vec![],
        vec![0, 1],
        vec![0, 1, 2, 3],
        vec![0, 0, 2],
        vec![0, 1, 3],
        vec![u32::MAX, 1, 2],
    ] {
        let mut bad = table.clone();
        let b::Op::Record {
            initialization_order,
            ..
        } = &mut bad.functions[0].blocks[0].instructions[site.order - 1].op
        else {
            unreachable!();
        };
        *initialization_order = order;
        assert!(bad.verify(Default::default()).is_err());
    }
    let replay = table.clone().verify(Default::default()).unwrap();
    for (kind, used) in [
        (m::LimitKind::Work, replay.work_used()),
        (m::LimitKind::Allocations, replay.requested_bytes()),
        (m::LimitKind::Elements, replay.elements_used()),
    ] {
        let mut limits = b::Limits::default();
        match kind {
            m::LimitKind::Work => limits.places.ownership.unit.work = used,
            m::LimitKind::Allocations => limits.places.ownership.unit.allocations = used,
            _ => limits.places.ownership.unit.elements = used,
        }
        table.clone().verify(limits).unwrap();
        match kind {
            m::LimitKind::Work => limits.places.ownership.unit.work -= 1,
            m::LimitKind::Allocations => limits.places.ownership.unit.allocations -= 1,
            _ => limits.places.ownership.unit.elements -= 1,
        }
        assert!(
            matches!(table.clone().verify(limits).unwrap_err().kind, o::ErrorKind::Mir(m::ErrorKind::Limit { kind: actual, .. }) if actual == kind)
        );
    }
}

#[test]
fn empty_records_and_invalid_instruction_sites_have_no_field_obligations() {
    let program = checked("struct Empty {} fn f(owner: shared i64) -> Empty { return Empty {}; }");
    let typed = program.mir().activity().moved().typed();
    let order = RecordOrder::new(typed, m::FunctionId(0), sites(typed)[0]).unwrap();
    assert_eq!(order.field_count(), 0);
    assert!(order.field_span(0).is_none());
    assert_eq!(order.reverse().count(), 0);
    let mut fields: [Option<Strong>; 0] = [];
    let mut drain = order.drain(&mut fields).unwrap();
    assert_eq!(drain.size_hint(), (0, Some(0)));
    assert!(drain.next().is_none());
    assert!(drain.next().is_none());
    for site in [
        b::Site {
            block: m::BlockId(0),
            order: 0,
        },
        b::Site {
            block: m::BlockId(99),
            order: 1,
        },
        b::Site {
            block: m::BlockId(0),
            order: usize::MAX,
        },
    ] {
        assert!(RecordOrder::new(typed, m::FunctionId(0), site).is_none());
    }
    assert!(RecordOrder::new(typed, m::FunctionId(99), sites(typed)[0]).is_none());
}
