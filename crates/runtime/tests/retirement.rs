use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{
    allocation as a, arena::managed as arena, buffer, cleanup as c, layout as l, map,
    retirement as r, shared::managed as g, vector as v,
};
use std::{cell::RefCell, rc::Rc};

fn ty(n: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(n))
}
fn nominal(n: u32) -> o::Type {
    o::Type::Nominal(m::DefinitionId(n))
}
fn span(n: u32) -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: n,
        end: n + 1,
    }
}
fn model() -> o::VerifiedTypes {
    let field = |name: &str, ty| o::Field {
        name: name.into(),
        span: span(0),
        ty: o::TypeUse { ty, span: span(0) },
    };
    o::TypeTable {
        sources: vec![m::Source {
            path: "retirement.lang".into(),
            text: "x".repeat(256),
        }],
        constructed: [
            o::Constructor::Builtin(o::Builtin::Bytes),
            o::Constructor::Builtin(o::Builtin::Text),
            o::Constructor::Shared(nominal(0)),
            o::Constructor::Vec(ty(2)),
            o::Constructor::Map(ty(1), ty(3)),
            o::Constructor::Vec(nominal(1)),
            o::Constructor::Vec(ty(0)),
            o::Constructor::Map(o::Type::I64, ty(6)),
        ]
        .into_iter()
        .map(|constructor| o::Constructed {
            constructor,
            span: span(0),
        })
        .collect(),
        definitions: vec![
            o::Definition {
                name: "Pair".into(),
                public: false,
                span: span(0),
                kind: o::DefinitionKind::Record(vec![field("a", ty(0)), field("b", ty(0))]),
            },
            o::Definition {
                name: "Node".into(),
                public: false,
                span: span(0),
                kind: o::DefinitionKind::Record(vec![
                    field("bytes", ty(0)),
                    field("children", ty(5)),
                ]),
            },
        ],
    }
    .verify(Default::default())
    .unwrap()
}
#[derive(Default)]
struct Trace {
    requests: usize,
    releases: usize,
    refuse: bool,
}
struct Provider(Rc<RefCell<Trace>>);
impl a::Provider for Provider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<Self::Region> {
        let mut trace = self.0.borrow_mut();
        trace.requests += 1;
        if trace.refuse {
            None
        } else {
            a::Storage::try_new(request).ok()
        }
    }
    fn deallocate(&mut self, storage: Self::Region, _: a::Request) {
        self.0.borrow_mut().releases += 1;
        drop(storage);
    }
}
fn allocator() -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    (
        a::Allocator::new(
            Provider(trace.clone()),
            a::Limits {
                live_allocations: 4096,
                ..Default::default()
            },
        )
        .unwrap(),
        trace,
    )
}
fn put(
    value: &l::Reservation<'_, '_>,
    at: l::Location,
    bytes: [u8; 32],
    allocator: &mut a::Allocator<Provider>,
) {
    let offset = at.offset() as usize;
    value.data_mut(allocator).unwrap()[offset..offset + 32].copy_from_slice(&bytes);
}
fn bytes(allocator: &mut a::Allocator<Provider>, n: u8) -> [u8; 32] {
    buffer::Bytes::from_slice(allocator, &[n])
        .unwrap()
        .take_token(allocator)
        .unwrap()
}
fn text(allocator: &mut a::Allocator<Provider>, s: &str) -> [u8; 32] {
    let mut bytes = buffer::Bytes::from_slice(allocator, s.as_bytes()).unwrap();
    bytes
        .take_text(allocator)
        .unwrap()
        .take_token(allocator)
        .unwrap()
}
fn input<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    ty: o::Type,
    bytes: [u8; 32],
    allocator: &mut a::Allocator<Provider>,
) -> l::Reservation<'l, 't> {
    let value = l::Reservation::allocate(layouts, ty, allocator).unwrap();
    put(&value, value.root(), bytes, allocator);
    value
}
fn token(ty: o::Type, bytes: [u8; 32]) -> l::Token {
    l::Token {
        leaf: l::Leaf {
            ty,
            offset: 0,
            span: None,
        },
        bytes,
    }
}
fn context(
    mut clean: impl FnMut(&mut c::Context<'_>) -> Result<(), u32>,
    fail: bool,
) -> (c::Instance, c::Report) {
    let mut instance = c::Instance::default();
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |_: &mut (), _, ctx: &mut c::Context<'_>| clean(ctx),
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    frame.initialize(c::Slot(0), (), Some(span(200))).unwrap();
    let report = frame
        .finish(if fail {
            c::Exit::Failure {
                failure: c::ExecutionFailure::Panic,
                span: Some(span(201)),
            }
        } else {
            c::Exit::Return(None)
        })
        .report;
    (instance, report)
}
fn clean(
    token: &mut l::Token,
    allocator: &mut a::Allocator<Provider>,
    ctx: &c::Context<'_>,
) -> (u8, Option<m::Span>) {
    let n = if token.leaf.ty == ty(0) {
        let mut value = buffer::Bytes::from_token(&mut token.bytes, allocator).unwrap();
        let n = value.get(allocator, 0).unwrap() as u8;
        value.release(allocator).unwrap();
        n
    } else {
        assert_eq!(token.leaf.ty, ty(1));
        let mut value = buffer::Text::from_token(&mut token.bytes, allocator).unwrap();
        let n = value.as_str(allocator).unwrap().as_bytes()[0];
        value.release(allocator).unwrap();
        n
    };
    (n, ctx.span())
}
fn empty(allocator: &a::Allocator<Provider>, trace: &Rc<RefCell<Trace>>) {
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
    assert_eq!(trace.borrow().requests, trace.borrow().releases);
}
fn heap<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    allocator: &mut a::Allocator<Provider>,
) -> g::Heap<'l, 't> {
    let mut heap = g::Heap::prepare(
        layouts,
        g::Limits {
            nodes: 512,
            slots: 512,
            tickets: 1024,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    heap.open(allocator).unwrap();
    heap
}
fn pair<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    n: u8,
    allocator: &mut a::Allocator<Provider>,
) -> l::Reservation<'l, 't> {
    let value = l::Reservation::allocate(layouts, nominal(0), allocator).unwrap();
    for field in 0..2 {
        let at = value.child(value.root(), field, allocator).unwrap();
        put(&value, at, bytes(allocator, n + field as u8), allocator);
    }
    value
        .initialize_order(
            value.root(),
            [(1, span(u32::from(n + 1))), (0, span(u32::from(n)))].into_iter(),
            allocator,
        )
        .unwrap();
    value
}
fn fixture<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    heap: &mut g::Heap<'l, 't>,
    allocator: &mut a::Allocator<Provider>,
) -> l::Token {
    let mut map = map::Map::new(layouts, ty(1), ty(3), allocator).unwrap();
    for (i, key) in ["A", "B"].into_iter().enumerate() {
        let mut vector = v::Vector::new(layouts, ty(2), allocator).unwrap();
        for j in 0..2 {
            let mut pair = pair(layouts, (i * 4 + j * 2 + 1) as u8, allocator);
            let owner = heap
                .allocate_from(&mut pair, Some(span(20)), allocator)
                .unwrap();
            pair.release_storage(allocator).unwrap();
            let mut incoming = input(layouts, ty(2), owner.into_token(), allocator);
            vector
                .push_from(&mut incoming, Some(span(30)), allocator)
                .unwrap();
            incoming.release_storage(allocator).unwrap();
        }
        let mut key_value = input(layouts, ty(1), text(allocator, key), allocator);
        let mut value = input(
            layouts,
            ty(3),
            vector.take_token(allocator).unwrap(),
            allocator,
        );
        map.publish_if_absent(
            &mut key_value,
            &mut value,
            map::Spans {
                key: Some(span(40 + i as u32)),
                value: Some(span(50)),
            },
            allocator,
        )
        .unwrap();
        key_value.release_storage(allocator).unwrap();
        value.release_storage(allocator).unwrap();
    }
    token(ty(4), map.take_token(allocator).unwrap())
}

#[test]
fn map_vector_shared_descendants_finish_before_siblings_with_original_field_and_key_spans() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator();
    let mut heap = heap(&layouts, &mut allocator);
    let mut drain =
        r::Drain::prepare(&layouts, &allocator, Default::default(), heap.usage()).unwrap();
    let usage = drain.usage();
    let mut root = fixture(&layouts, &mut heap, &mut allocator);
    let requests = trace.borrow().requests;
    trace.borrow_mut().refuse = true;
    let mut found = vec![];
    let (instance, report) = context(
        |ctx| {
            drain.token(
                &mut root,
                Some(&mut heap),
                &mut allocator,
                ctx,
                |token, allocator, _, ctx| {
                    found.push(clean(token, allocator, ctx));
                    Ok(())
                },
            )
        },
        false,
    );
    assert!(report.primary().is_none());
    assert!(!instance.is_poisoned());
    assert_eq!(root.bytes, [0; 32]);
    assert_eq!(
        found,
        [
            (7, 7),
            (8, 8),
            (5, 5),
            (6, 6),
            (b'B', 41),
            (3, 3),
            (4, 4),
            (1, 1),
            (2, 2),
            (b'A', 40)
        ]
        .map(|(n, s)| (n, Some(span(s))))
    );
    assert_eq!(trace.borrow().requests, requests);
    assert_eq!(drain.usage(), usage);
    heap.close(&mut allocator).unwrap();
    empty(&allocator, &trace);
}

#[test]
fn leaf_failures_and_panics_keep_body_failure_priority_and_drain_every_remaining_owner() {
    for body in [false, true] {
        for panic in [false, true] {
            let types = model();
            let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
            let (mut allocator, trace) = allocator();
            let mut heap = heap(&layouts, &mut allocator);
            let mut drain =
                r::Drain::prepare(&layouts, &allocator, Default::default(), heap.usage()).unwrap();
            let mut root = fixture(&layouts, &mut heap, &mut allocator);
            let requests = trace.borrow().requests;
            let mut found = vec![];
            let (mut instance, report) = context(
                |ctx| {
                    drain.token(
                        &mut root,
                        Some(&mut heap),
                        &mut allocator,
                        ctx,
                        |token, allocator, _, ctx| {
                            found.push(clean(token, allocator, ctx));
                            if panic && found.len() == 2 {
                                panic!("after release");
                            }
                            Err(1000 + found.len() as u32)
                        },
                    )
                },
                body,
            );
            assert_eq!(found.len(), 10);
            assert_eq!(report.secondary().len(), 9 + usize::from(body));
            assert_eq!(
                report.primary().unwrap().kind,
                if body {
                    c::FailureKind::Execution(c::ExecutionFailure::Panic)
                } else {
                    c::FailureKind::Cleanup(1001)
                }
            );
            assert!(instance.is_poisoned());
            assert!(
                instance
                    .enter::<(), _>(Default::default(), |_, _| Ok(()))
                    .is_err()
            );
            assert_eq!(trace.borrow().requests, requests);
            heap.close(&mut allocator).unwrap();
            empty(&allocator, &trace);
        }
    }
}

#[test]
fn retained_read_is_the_last_owner_and_weak_cleanup_finishes_after_payload_expiration() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator();
    let mut heap = heap(&layouts, &mut allocator);
    let mut drain =
        r::Drain::prepare(&layouts, &allocator, Default::default(), heap.usage()).unwrap();
    let mut value = pair(&layouts, 1, &mut allocator);
    let strong = heap
        .allocate_from(&mut value, Some(span(10)), &mut allocator)
        .unwrap();
    value.release_storage(&mut allocator).unwrap();
    let weak = heap.downgrade(&strong, nominal(0), &mut allocator).unwrap();
    let read = heap.read(&strong, nominal(0), &mut allocator).unwrap();
    let mut root = token(ty(2), strong.into_token());
    let mut found = vec![];
    let (_, report) = context(
        |ctx| {
            drain.token(
                &mut root,
                Some(&mut heap),
                &mut allocator,
                ctx,
                |token, allocator, _, ctx| {
                    found.push(clean(token, allocator, ctx));
                    Ok(())
                },
            )
        },
        false,
    );
    assert!(report.primary().is_none());
    assert!(found.is_empty());
    let (_, report) = context(
        |ctx| {
            drain
                .end_read(
                    read,
                    nominal(0),
                    &mut heap,
                    &mut allocator,
                    ctx,
                    |token, allocator, _, ctx| {
                        found.push(clean(token, allocator, ctx));
                        Ok(())
                    },
                )
                .map_err(|_| 99)
        },
        false,
    );
    assert!(report.primary().is_none());
    assert_eq!(found.iter().map(|x| x.0).collect::<Vec<_>>(), [1, 2]);
    assert!(matches!(
        heap.upgrade(&weak, nominal(0), &mut allocator)
            .err()
            .unwrap()
            .kind,
        g::Kind::Ownership(l::OwnershipError::ExpiredWeak)
    ));
    heap.release_weak(weak, nominal(0), &mut allocator).unwrap();
    heap.close(&mut allocator).unwrap();
    empty(&allocator, &trace);
}

#[test]
fn exact_host_admission_and_allocator_identity_refuse_before_consuming_a_token() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator();
    let expected = r::Drain::prepare(&layouts, &allocator, Default::default(), layouts.usage())
        .unwrap()
        .usage();
    for short in [false, true] {
        assert_eq!(
            r::Drain::prepare(
                &layouts,
                &allocator,
                m::Limits {
                    allocations: expected.allocation_bytes - usize::from(short),
                    elements: expected.elements,
                    work: expected.work,
                    ..Default::default()
                },
                layouts.usage()
            )
            .is_ok(),
            !short
        );
    }
    let (mut foreign, _) = self::allocator();
    let mut drain =
        r::Drain::prepare(&layouts, &foreign, Default::default(), layouts.usage()).unwrap();
    let mut root = token(ty(0), bytes(&mut allocator, 9));
    let original = root.bytes;
    let (_, report) = context(
        |ctx| {
            drain.token(&mut root, None, &mut allocator, ctx, |_, _, _, _| {
                panic!("refused")
            })
        },
        false,
    );
    assert!(report.primary().is_some());
    assert_eq!(root.bytes, original);
    let mut heap = heap(&layouts, &mut allocator);
    let foreign_layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut wrong_layout = r::Drain::prepare(
        &foreign_layouts,
        &allocator,
        Default::default(),
        foreign_layouts.usage(),
    )
    .unwrap();
    let (_, report) = context(
        |ctx| {
            wrong_layout.token(
                &mut root,
                Some(&mut heap),
                &mut allocator,
                ctx,
                |_, _, _, _| panic!("refused"),
            )
        },
        false,
    );
    assert!(report.primary().is_some());
    assert_eq!(root.bytes, original);
    heap.close(&mut allocator).unwrap();
    let mut drain =
        r::Drain::prepare(&layouts, &allocator, Default::default(), layouts.usage()).unwrap();
    let (_, report) = context(
        |ctx| {
            drain.token(
                &mut root,
                None,
                &mut allocator,
                ctx,
                |token, allocator, _, ctx| {
                    clean(token, allocator, ctx);
                    Ok(())
                },
            )
        },
        false,
    );
    assert!(report.primary().is_none());
    assert!(foreign.revoke().is_ok());
    empty(&allocator, &trace);
}

#[test]
fn arena_nodes_and_nested_map_vector_children_retire_in_reverse_order_with_span_fallbacks() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator();
    let mut region = arena::Arena::prepare(
        &layouts,
        arena::Limits {
            nodes: 2,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let mut inspector = lang_runtime::inspection::Inspector::prepare(
        &layouts,
        8,
        Default::default(),
        region.usage(),
    )
    .unwrap();
    let mut drain =
        r::Drain::prepare(&layouts, &allocator, Default::default(), inspector.usage()).unwrap();
    region.open(&mut allocator).unwrap();
    for n in 0..2 {
        let mut vector = v::Vector::new(&layouts, ty(0), &mut allocator).unwrap();
        for j in 0..2 {
            let mut value = input(
                &layouts,
                ty(0),
                bytes(&mut allocator, n * 2 + j),
                &mut allocator,
            );
            vector.push_from(&mut value, None, &mut allocator).unwrap();
            value.release_storage(&mut allocator).unwrap();
        }
        let mut map = map::Map::new(&layouts, o::Type::I64, ty(6), &mut allocator).unwrap();
        let mut key = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
        let mut value = input(
            &layouts,
            ty(6),
            vector.take_token(&allocator).unwrap(),
            &mut allocator,
        );
        map.publish_if_absent(
            &mut key,
            &mut value,
            map::Spans {
                key: None,
                value: Some(span(70 + u32::from(n))),
            },
            &mut allocator,
        )
        .unwrap();
        key.release_storage(&mut allocator).unwrap();
        value.release_storage(&mut allocator).unwrap();
        let mut incoming = input(
            &layouts,
            ty(7),
            map.take_token(&allocator).unwrap(),
            &mut allocator,
        );
        region
            .allocate_from(
                &mut incoming,
                Some(span(80)),
                &mut allocator,
                |token, _, allocator, budget| {
                    // Reserve this fixture's complete walk allowance before inspection.
                    budget.charge(32)?;
                    let mut work = lang_runtime::inspection::Work::new(32);
                    let mut read = inspector.token(token, allocator).unwrap();
                    while let Some(observed) = read.next(&mut work).unwrap() {
                        assert_eq!(observed.ty, ty(0));
                        let mut bytes = observed.bytes;
                        buffer::Bytes::from_token(&mut bytes, allocator)
                            .unwrap()
                            .len(allocator)
                            .unwrap();
                    }
                    Ok(())
                },
            )
            .unwrap();
        incoming.release_storage(&mut allocator).unwrap();
    }
    let requests = trace.borrow().requests;
    trace.borrow_mut().refuse = true;
    let mut found = vec![];
    let (_, report) = context(
        |ctx| {
            region
                .cleanup(
                    arena::Exit::Normal,
                    &mut allocator,
                    ctx,
                    |token, allocator, _, ctx| {
                        drain.token(token, None, allocator, ctx, |token, allocator, _, ctx| {
                            found.push(clean(token, allocator, ctx));
                            Ok(())
                        })
                    },
                )
                .map_err(|_| 99)
        },
        false,
    );
    assert!(report.primary().is_none(), "{report:?}");
    assert_eq!(
        found,
        [(3, 71), (2, 71), (1, 70), (0, 70)].map(|(n, s)| (n, Some(span(s))))
    );
    assert_eq!(trace.borrow().requests, requests);
    empty(&allocator, &trace);
}

fn node<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    n: u8,
    child: Option<l::Reservation<'l, 't>>,
    allocator: &mut a::Allocator<Provider>,
) -> l::Reservation<'l, 't> {
    let mut children = v::Vector::new(layouts, nominal(1), allocator).unwrap();
    if let Some(mut child) = child {
        children
            .push_from(&mut child, Some(span(10)), allocator)
            .unwrap();
        child.release_storage(allocator).unwrap();
    }
    let node = l::Reservation::allocate(layouts, nominal(1), allocator).unwrap();
    let at = node.child(node.root(), 0, allocator).unwrap();
    put(&node, at, bytes(allocator, n), allocator);
    let at = node.child(node.root(), 1, allocator).unwrap();
    put(
        &node,
        at,
        children.take_token(allocator).unwrap(),
        allocator,
    );
    node.initialize_order(
        node.root(),
        [(0, span(u32::from(n))), (1, span(10))].into_iter(),
        allocator,
    )
    .unwrap();
    node
}
#[test]
fn recursive_collection_destruction_uses_a_small_host_stack_without_retirement_allocation() {
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let types = model();
            let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
            let (mut allocator, trace) = allocator();
            let mut drain =
                r::Drain::prepare(&layouts, &allocator, Default::default(), layouts.usage())
                    .unwrap();
            let usage = drain.usage();
            let mut root = node(&layouts, 0, None, &mut allocator);
            for n in 1..=512 {
                root = node(&layouts, (n % 128) as u8, Some(root), &mut allocator);
            }
            let requests = trace.borrow().requests;
            trace.borrow_mut().refuse = true;
            let mut found = vec![];
            let (_, report) = context(
                |ctx| {
                    root.cleanup(&mut allocator, ctx, |token, allocator, _, ctx| {
                        drain.token(token, None, allocator, ctx, |token, allocator, _, ctx| {
                            found.push(clean(token, allocator, ctx));
                            Ok(())
                        })
                    })
                    .map_err(|_| 99)
                },
                false,
            );
            assert!(report.primary().is_none(), "{report:?}");
            assert_eq!(
                found,
                (0..=512)
                    .map(|n| ((n % 128) as u8, Some(span((n % 128) as u32))))
                    .collect::<Vec<_>>()
            );
            assert_eq!(trace.borrow().requests, requests);
            assert_eq!(drain.usage(), usage);
            empty(&allocator, &trace);
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn bounded_report_truncation_does_not_skip_any_shared_descendant() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator();
    let mut heap = heap(&layouts, &mut allocator);
    let mut drain =
        r::Drain::prepare(&layouts, &allocator, Default::default(), heap.usage()).unwrap();
    let mut vector = v::Vector::new(&layouts, ty(2), &mut allocator).unwrap();
    for _ in 0..130 {
        let mut pair = pair(&layouts, 1, &mut allocator);
        let strong = heap.allocate_from(&mut pair, None, &mut allocator).unwrap();
        pair.release_storage(&mut allocator).unwrap();
        let mut value = input(&layouts, ty(2), strong.into_token(), &mut allocator);
        vector.push_from(&mut value, None, &mut allocator).unwrap();
        value.release_storage(&mut allocator).unwrap();
    }
    let mut root = token(ty(3), vector.take_token(&allocator).unwrap());
    let requests = trace.borrow().requests;
    let mut count = 0;
    let (_, report) = context(
        |ctx| {
            drain.token(
                &mut root,
                Some(&mut heap),
                &mut allocator,
                ctx,
                |token, allocator, _, ctx| {
                    clean(token, allocator, ctx);
                    count += 1;
                    Err(1000 + count)
                },
            )
        },
        false,
    );
    assert_eq!(count, 260);
    assert_eq!(report.secondary().len(), 255);
    assert_eq!(report.truncated(), 4);
    assert_eq!(trace.borrow().requests, requests);
    heap.close(&mut allocator).unwrap();
    empty(&allocator, &trace);
}
