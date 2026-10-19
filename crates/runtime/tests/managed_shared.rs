use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{allocation as a, buffer, cleanup as c, layout as l, shared::managed as g};
use std::{cell::RefCell, rc::Rc};

#[path = "support/shared_work.rs"]
mod work;

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1,
    }
}
fn types() -> o::VerifiedTypes {
    o::TypeTable {
        sources: vec![m::Source {
            path: "shared.lang".into(),
            text: "x".into(),
        }],
        ..Default::default()
    }
    .verify(Default::default())
    .unwrap()
}
fn context(mut clean: impl FnMut(&mut c::Context<'_>) -> Result<(), u32>) -> c::Report {
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
    frame.initialize(c::Slot(0), (), Some(span())).unwrap();
    frame.finish(c::Exit::Return(None)).report
}
#[test]
fn real_registered_tokens_clone_upgrade_retire_and_reject_a_consumed_ticket() {
    let types = types();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 2,
            slots: 2,
            tickets: 5,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    heap.open(&mut allocator).unwrap();
    let mut input = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    input
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&9i64.to_le_bytes());
    let strong = heap
        .allocate_from(&mut input, Some(span()), &mut allocator)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let weak = heap
        .downgrade(&strong, o::Type::I64, &mut allocator)
        .unwrap();
    let clone = heap
        .clone_strong(&strong, o::Type::I64, &mut allocator)
        .unwrap();
    let mut bytes = clone.into_token();
    let mut stale = bytes;
    let clone = g::Strong::from_token(&mut bytes).unwrap();
    assert_eq!(bytes, [0; 32]);
    assert_eq!(
        heap.strong_count(&strong, o::Type::I64, &allocator)
            .unwrap(),
        2
    );
    let mut clone = Some(clone);
    let report = context(|ctx| {
        heap.release_strong(
            clone.take().unwrap(),
            o::Type::I64,
            &mut allocator,
            ctx,
            |_, _, _, _| panic!("scalar cleanup"),
        )
        .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    let stale = g::Strong::from_token(&mut stale).unwrap();
    assert!(heap.strong_count(&stale, o::Type::I64, &allocator).is_err());
    assert_eq!(
        heap.strong_count(&strong, o::Type::I64, &allocator)
            .unwrap(),
        1
    );
    let mut strong = Some(strong);
    let report = context(|ctx| {
        heap.release_strong(
            strong.take().unwrap(),
            o::Type::I64,
            &mut allocator,
            ctx,
            |_, _, _, _| panic!("scalar cleanup"),
        )
        .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    assert!(matches!(
        heap.upgrade(&weak, o::Type::I64, &mut allocator)
            .err()
            .unwrap()
            .kind,
        g::Kind::Ownership(l::OwnershipError::ExpiredWeak)
    ));
    heap.release_weak(weak, o::Type::I64, &mut allocator)
        .unwrap();
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
}

fn at(start: u32) -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start,
        end: start + 1,
    }
}
fn ct(index: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(index))
}
fn node_type() -> o::Type {
    o::Type::Nominal(m::DefinitionId(0))
}
fn node_model() -> o::VerifiedTypes {
    node_table().verify(Default::default()).unwrap()
}
fn node_table() -> o::TypeTable {
    o::TypeTable {
        sources: vec![m::Source {
            path: "shared.lang".into(),
            text: "abcdefgh".into(),
        }],
        constructed: [
            o::Constructor::Builtin(o::Builtin::Bytes),
            o::Constructor::Shared(node_type()),
            o::Constructor::Weak(node_type()),
            o::Constructor::Option(ct(1)),
            o::Constructor::Option(ct(2)),
            o::Constructor::Vec(ct(1)),
        ]
        .into_iter()
        .map(|constructor| o::Constructed {
            constructor,
            span: at(0),
        })
        .collect(),
        definitions: vec![o::Definition {
            name: "Node".into(),
            public: false,
            span: m::Span {
                source: m::SourceId(0),
                start: 0,
                end: 8,
            },
            kind: o::DefinitionKind::Record(
                [(0, "bytes", ct(0)), (1, "next", ct(3)), (2, "back", ct(4))]
                    .into_iter()
                    .map(|(i, name, ty)| o::Field {
                        name: name.into(),
                        span: at(i),
                        ty: o::TypeUse { ty, span: at(i) },
                    })
                    .collect(),
            ),
        }],
    }
}
fn node<'l, 't, P: a::Provider>(
    layouts: &'l l::Layouts<'t>,
    allocator: &mut a::Allocator<P>,
    value: u8,
    next: Option<g::Strong>,
    back: Option<g::Weak>,
) -> l::Reservation<'l, 't> {
    let out = l::Reservation::allocate(layouts, node_type(), allocator).unwrap();
    let location = out.child(out.root(), 0, allocator).unwrap();
    let mut bytes = buffer::Bytes::from_slice(allocator, &[value]).unwrap();
    let token = bytes.take_token(allocator).unwrap();
    let offset = location.offset() as usize;
    out.data_mut(allocator).unwrap()[offset..offset + 32].copy_from_slice(&token);
    for (field, token) in [
        (1, next.map(g::Strong::into_token)),
        (2, back.map(g::Weak::into_token)),
    ] {
        let location = out.child(out.root(), field, allocator).unwrap();
        out.prepare_case(location, u32::from(token.is_some()), allocator)
            .unwrap();
        if let Some(token) = token {
            let payload = out.child(location, 0, allocator).unwrap();
            let offset = payload.offset() as usize;
            out.data_mut(allocator).unwrap()[offset..offset + 32].copy_from_slice(&token);
            out.initialize_order(location, [(0, at(field))].into_iter(), allocator)
                .unwrap();
        } else {
            out.initialize_order(location, [].into_iter(), allocator)
                .unwrap();
        }
    }
    out.initialize_order(
        out.root(),
        [(0, at(0)), (1, at(1)), (2, at(2))].into_iter(),
        allocator,
    )
    .unwrap();
    out
}
#[derive(Default)]
struct Trace {
    requests: Vec<a::Request>,
    releases: Vec<a::Request>,
    fail: Option<usize>,
}
struct Provider(Rc<RefCell<Trace>>);
impl a::Provider for Provider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<Self::Region> {
        let mut trace = self.0.borrow_mut();
        trace.requests.push(request);
        if trace.fail == Some(trace.requests.len()) {
            return None;
        }
        a::Storage::try_new(request).ok()
    }
    fn deallocate(&mut self, region: Self::Region, request: a::Request) {
        self.0.borrow_mut().releases.push(request);
        drop(region);
    }
}
fn allocator(limits: a::Limits) -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    (
        a::Allocator::new(Provider(trace.clone()), limits).unwrap(),
        trace,
    )
}
fn clean<P: a::Provider>(
    token: &mut l::Token,
    allocator: &mut a::Allocator<P>,
    _: c::Reason,
    _: &mut c::Context<'_>,
) -> Result<(), u32> {
    let mut bytes = buffer::Bytes::from_token(&mut token.bytes, allocator).unwrap();
    bytes.release(allocator).unwrap();
    Ok(())
}
fn release<P: a::Provider>(
    heap: &mut g::Heap<'_, '_>,
    strong: g::Strong,
    ty: o::Type,
    allocator: &mut a::Allocator<P>,
) {
    let mut strong = Some(strong);
    let report = context(|ctx| {
        heap.release_strong(strong.take().unwrap(), ty, allocator, ctx, clean)
            .map_err(|_| 99)
    });
    assert!(report.primary().is_none(), "{report:?}");
}

#[test]
fn typed_cycle_rejection_keeps_values_counts_and_input_before_weak_graph_cleanup() {
    let types = node_model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 8,
            slots: 8,
            tickets: 32,
            value_cells: 32,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    heap.open(&mut allocator).unwrap();
    let mut input = node(&layouts, &mut allocator, 1, None, None);
    let left = heap
        .allocate_from(&mut input, Some(at(4)), &mut allocator)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let weak = heap.downgrade(&left, node_type(), &mut allocator).unwrap();
    let left_edge = heap
        .clone_strong(&left, node_type(), &mut allocator)
        .unwrap();
    let back = heap.clone_weak(&weak, node_type(), &mut allocator).unwrap();
    let mut input = node(&layouts, &mut allocator, 2, Some(left_edge), Some(back));
    let right = heap
        .allocate_from(&mut input, Some(at(5)), &mut allocator)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let right_edge = heap
        .clone_strong(&right, node_type(), &mut allocator)
        .unwrap();
    let mut incoming = node(&layouts, &mut allocator, 3, Some(right_edge), None);
    let original = incoming.data(&allocator).unwrap().to_vec();
    let before = allocator.stats();
    let error = heap
        .replace_from(&left, node_type(), &mut incoming, at(6), &mut allocator)
        .unwrap_err();
    assert_eq!(
        error.kind,
        g::Kind::Ownership(l::OwnershipError::StrongCycle)
    );
    assert_eq!(error.primary, Some(at(6)));
    assert_eq!(error.secondary, Some(at(1)));
    assert_eq!(allocator.stats(), before);
    assert_eq!(incoming.data(&allocator).unwrap(), original);
    assert_eq!(
        heap.strong_count(&left, node_type(), &allocator).unwrap(),
        2
    );
    assert_eq!(
        heap.strong_count(&right, node_type(), &allocator).unwrap(),
        2
    );
    let report = context(|ctx| {
        heap.retire_value(&mut incoming, Some(at(6)), &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    incoming.release_storage(&mut allocator).unwrap();
    release(&mut heap, left, node_type(), &mut allocator);
    let mut seen = Vec::new();
    let mut right = Some(right);
    let calls = trace.borrow().requests.len();
    let report = context(|ctx| {
        heap.release_strong(
            right.take().unwrap(),
            node_type(),
            &mut allocator,
            ctx,
            |token, alloc, _, _| {
                let mut bytes = buffer::Bytes::from_token(&mut token.bytes, alloc).unwrap();
                seen.push(bytes.as_slice(alloc).unwrap()[0]);
                bytes.release(alloc).unwrap();
                Ok(())
            },
        )
        .map_err(|_| 99)
    });
    assert!(report.primary().is_none(), "{report:?}");
    assert_eq!(seen, [1, 2]);
    assert_eq!(trace.borrow().requests.len(), calls);
    assert!(matches!(
        heap.upgrade(&weak, node_type(), &mut allocator)
            .err()
            .unwrap()
            .kind,
        g::Kind::Ownership(l::OwnershipError::ExpiredWeak)
    ));
    heap.release_weak(weak, node_type(), &mut allocator)
        .unwrap();
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn read_registration_keeps_the_last_payload_alive_and_stale_read_cannot_end_a_reused_slot() {
    let types = types();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 2,
            slots: 2,
            tickets: 2,
            readers: 1,
            value_cells: 4,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let (mut allocator, _) = allocator(Default::default());
    heap.open(&mut allocator).unwrap();
    let mut input = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    input
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&9i64.to_le_bytes());
    let strong = heap
        .allocate_copy(&mut input, Some(span()), &mut allocator)
        .unwrap();
    let read = heap.read(&strong, o::Type::I64, &mut allocator).unwrap();
    let before = allocator.stats();
    assert!(matches!(
        heap.replace_copy(&strong, o::Type::I64, &mut input, span(), &mut allocator)
            .unwrap_err()
            .kind,
        g::Kind::Ownership(l::OwnershipError::BorrowConflict)
    ));
    assert_eq!(allocator.stats(), before);
    release(&mut heap, strong, o::Type::I64, &mut allocator);
    assert_eq!(
        heap.view(read, o::Type::I64, &allocator)
            .unwrap()
            .read_immediate(
                heap.view(read, o::Type::I64, &allocator).unwrap().root(),
                &allocator
            )
            .unwrap(),
        l::Immediate::I64(9)
    );
    let report = context(|ctx| {
        heap.end_read(read, o::Type::I64, &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    assert!(heap.view(read, o::Type::I64, &allocator).is_err());
    let strong = heap
        .allocate_copy(&mut input, Some(span()), &mut allocator)
        .unwrap();
    let new = heap.read(&strong, o::Type::I64, &mut allocator).unwrap();
    let report = context(|ctx| {
        assert!(
            heap.end_read(read, o::Type::I64, &mut allocator, ctx, clean)
                .is_err()
        );
        Ok(())
    });
    assert!(report.primary().is_none());
    assert!(heap.view(new, o::Type::I64, &allocator).is_ok());
    context(|ctx| {
        heap.end_read(new, o::Type::I64, &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    release(&mut heap, strong, o::Type::I64, &mut allocator);
    input.release_storage(&mut allocator).unwrap();
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn deep_typed_graph_retires_children_before_siblings_without_provider_requests_or_rust_recursion() {
    let types = node_model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 1024,
            slots: 1024,
            tickets: 2048,
            value_cells: 32,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    heap.open(&mut allocator).unwrap();
    let mut root = None;
    for i in 0..1024 {
        let mut input = node(&layouts, &mut allocator, (i % 256) as u8, root.take(), None);
        root = Some(
            heap.allocate_from(&mut input, Some(at(4)), &mut allocator)
                .unwrap(),
        );
        input.release_storage(&mut allocator).unwrap();
    }
    let mut seen = Vec::new();
    let calls = trace.borrow().requests.len();
    let report = context(|ctx| {
        heap.release_strong(
            root.take().unwrap(),
            node_type(),
            &mut allocator,
            ctx,
            |token, alloc, _, _| {
                let mut bytes = buffer::Bytes::from_token(&mut token.bytes, alloc).unwrap();
                seen.push(bytes.as_slice(alloc).unwrap()[0]);
                bytes.release(alloc).unwrap();
                Ok(())
            },
        )
        .map_err(|_| 99)
    });
    assert!(report.primary().is_none(), "{report:?}");
    assert_eq!(seen, (0..1024).map(|i| (i % 256) as u8).collect::<Vec<_>>());
    assert_eq!(trace.borrow().requests.len(), calls);
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
}

#[test]
fn borrowed_stored_fields_retain_only_through_the_matching_live_read_and_location() {
    let types = node_model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 4,
            slots: 4,
            tickets: 32,
            value_cells: 32,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    heap.open(&mut allocator).unwrap();
    let mut input = node(&layouts, &mut allocator, 1, None, None);
    let child = heap
        .allocate_from(&mut input, Some(at(4)), &mut allocator)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let weak = heap.downgrade(&child, node_type(), &mut allocator).unwrap();
    let next = heap
        .clone_strong(&child, node_type(), &mut allocator)
        .unwrap();
    let back = heap.clone_weak(&weak, node_type(), &mut allocator).unwrap();
    let mut input = node(&layouts, &mut allocator, 2, Some(next), Some(back));
    let parent = heap
        .allocate_from(&mut input, Some(at(5)), &mut allocator)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let read = heap.read(&parent, node_type(), &mut allocator).unwrap();
    let view = heap.view(read, node_type(), &allocator).unwrap();
    let next = view
        .child(
            view.child(view.root(), 1, &allocator).unwrap(),
            0,
            &allocator,
        )
        .unwrap();
    let back = view
        .child(
            view.child(view.root(), 2, &allocator).unwrap(),
            0,
            &allocator,
        )
        .unwrap();
    let calls = trace.borrow().requests.len();
    let g::Retained::Strong(cloned) = heap
        .retain_field(
            read,
            node_type(),
            next,
            g::FieldAction::CloneStrong,
            &mut allocator,
        )
        .unwrap()
    else {
        panic!("strong")
    };
    let g::Retained::Weak(new_weak) = heap
        .retain_field(
            read,
            node_type(),
            next,
            g::FieldAction::NewWeak,
            &mut allocator,
        )
        .unwrap()
    else {
        panic!("weak")
    };
    let g::Retained::Weak(cloned_weak) = heap
        .retain_field(
            read,
            node_type(),
            back,
            g::FieldAction::CloneWeak,
            &mut allocator,
        )
        .unwrap()
    else {
        panic!("weak")
    };
    let g::Retained::Strong(upgraded) = heap
        .retain_field(
            read,
            node_type(),
            back,
            g::FieldAction::UpgradeWeak,
            &mut allocator,
        )
        .unwrap()
    else {
        panic!("strong")
    };
    assert!(
        heap.retain_field(
            read,
            node_type(),
            next,
            g::FieldAction::CloneWeak,
            &mut allocator
        )
        .is_err()
    );
    assert_eq!(
        heap.strong_count(&child, node_type(), &allocator).unwrap(),
        4
    );
    let other = heap.read(&child, node_type(), &mut allocator).unwrap();
    let view = heap.view(other, node_type(), &allocator).unwrap();
    let foreign = view.child(view.root(), 0, &allocator).unwrap();
    assert!(matches!(
        heap.retain_field(
            read,
            node_type(),
            foreign,
            g::FieldAction::CloneStrong,
            &mut allocator
        )
        .unwrap_err()
        .kind,
        g::Kind::Layout(l::Error::ForeignLocation)
    ));
    context(|ctx| {
        heap.end_read(other, node_type(), &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    context(|ctx| {
        heap.end_read(read, node_type(), &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    assert!(
        heap.retain_field(
            read,
            node_type(),
            next,
            g::FieldAction::CloneStrong,
            &mut allocator
        )
        .is_err()
    );
    assert_eq!(trace.borrow().requests.len(), calls);
    release(&mut heap, cloned, node_type(), &mut allocator);
    release(&mut heap, upgraded, node_type(), &mut allocator);
    release(&mut heap, parent, node_type(), &mut allocator);
    release(&mut heap, child, node_type(), &mut allocator);
    let after_dead = heap.clone_weak(&weak, node_type(), &mut allocator).unwrap();
    for weak in [weak, new_weak, cloned_weak, after_dead] {
        heap.release_weak(weak, node_type(), &mut allocator)
            .unwrap();
    }
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn preparation_pools_have_exact_cumulative_limits_and_no_trial_allocations() {
    let types = node_model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let limits = g::Limits {
        nodes: 4,
        slots: 5,
        tickets: 8,
        value_cells: 12,
        ..Default::default()
    };
    let usage =
        g::Heap::preparation_usage(&layouts, limits, Default::default(), layouts.usage()).unwrap();
    let exact = m::Limits {
        work: usage.work,
        allocations: usage.allocation_bytes,
        elements: usage.elements,
        ..Default::default()
    };
    let heap = g::Heap::prepare(&layouts, limits, exact, layouts.usage()).unwrap();
    assert_eq!(heap.usage(), usage);
    for resource in 0..3 {
        let mut below = exact;
        match resource {
            0 => below.work -= 1,
            1 => below.allocations -= 1,
            _ => below.elements -= 1,
        };
        assert!(g::Heap::prepare(&layouts, limits, below, layouts.usage()).is_err());
    }
    assert!(matches!(
        g::Heap::prepare(
            &layouts,
            g::Limits {
                nodes: 0,
                slots: 0,
                tickets: usize::MAX,
                value_cells: 0,
                ..Default::default()
            },
            Default::default(),
            layouts.usage()
        )
        .err()
        .unwrap()
        .kind,
        g::Kind::Ownership(l::OwnershipError::Allocation(buffer::AllocError::Overflow))
    ));
}

#[test]
fn every_shared_publication_oom_preserves_external_tickets_and_can_retry_the_same_input() {
    let types = node_model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for position in 1..=3 {
        let mut heap = g::Heap::prepare(
            &layouts,
            g::Limits {
                nodes: 4,
                slots: 4,
                tickets: 12,
                value_cells: 16,
                ..Default::default()
            },
            Default::default(),
            layouts.usage(),
        )
        .unwrap();
        let (mut allocator, trace) = allocator(Default::default());
        heap.open(&mut allocator).unwrap();
        let mut input = node(&layouts, &mut allocator, 1, None, None);
        let child = heap
            .allocate_from(&mut input, Some(at(4)), &mut allocator)
            .unwrap();
        input.release_storage(&mut allocator).unwrap();
        let edge = heap
            .clone_strong(&child, node_type(), &mut allocator)
            .unwrap();
        let mut incoming = node(&layouts, &mut allocator, 2, Some(edge), None);
        let original = incoming.data(&allocator).unwrap().to_vec();
        let before = allocator.stats();
        let calls = trace.borrow().requests.len();
        trace.borrow_mut().fail = Some(calls + position);
        let error = heap
            .allocate_from(&mut incoming, Some(at(5)), &mut allocator)
            .err()
            .unwrap();
        assert_eq!(
            error.kind,
            g::Kind::Ownership(l::OwnershipError::Allocation(
                buffer::AllocError::OutOfMemory
            ))
        );
        assert_eq!(incoming.data(&allocator).unwrap(), original);
        assert_eq!(allocator.stats().live_bytes, before.live_bytes);
        assert_eq!(allocator.stats().live_allocations, before.live_allocations);
        assert_eq!(
            heap.strong_count(&child, node_type(), &allocator).unwrap(),
            2
        );
        trace.borrow_mut().fail = None;
        let parent = heap
            .allocate_from(&mut incoming, Some(at(5)), &mut allocator)
            .unwrap();
        incoming.release_storage(&mut allocator).unwrap();
        release(&mut heap, child, node_type(), &mut allocator);
        release(&mut heap, parent, node_type(), &mut allocator);
        heap.close(&mut allocator).unwrap();
        assert_eq!(allocator.stats().live_bytes, 0);
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
    }
}

#[test]
fn opening_oom_leaves_the_service_closed_and_does_not_consume_a_generation_on_retry() {
    let types = types();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 1,
            slots: 1,
            tickets: 1,
            value_cells: 1,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    trace.borrow_mut().fail = Some(1);
    assert_eq!(
        heap.open(&mut allocator).unwrap_err().kind,
        g::Kind::Ownership(l::OwnershipError::Allocation(
            buffer::AllocError::OutOfMemory
        ))
    );
    assert_eq!(allocator.stats().live_bytes, 0);
    trace.borrow_mut().fail = None;
    heap.open(&mut allocator).unwrap();
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn successful_replacement_detaches_old_owners_and_self_weak_cleanup_survives_failures_and_panics() {
    let types = node_model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for panic in [false, true] {
        let mut heap = g::Heap::prepare(
            &layouts,
            g::Limits {
                nodes: 4,
                slots: 4,
                tickets: 16,
                value_cells: 16,
                ..Default::default()
            },
            Default::default(),
            layouts.usage(),
        )
        .unwrap();
        let (mut allocator, trace) = allocator(Default::default());
        heap.open(&mut allocator).unwrap();
        let mut input = node(&layouts, &mut allocator, 1, None, None);
        let left = heap
            .allocate_from(&mut input, Some(at(4)), &mut allocator)
            .unwrap();
        input.release_storage(&mut allocator).unwrap();
        let mut input = node(&layouts, &mut allocator, 2, None, None);
        let right = heap
            .allocate_from(&mut input, Some(at(5)), &mut allocator)
            .unwrap();
        input.release_storage(&mut allocator).unwrap();
        let back = heap.downgrade(&left, node_type(), &mut allocator).unwrap();
        let next = heap
            .clone_strong(&right, node_type(), &mut allocator)
            .unwrap();
        let mut input = node(&layouts, &mut allocator, 3, Some(next), Some(back));
        let calls = trace.borrow().requests.len();
        let old = heap
            .replace_from(&left, node_type(), &mut input, at(6), &mut allocator)
            .unwrap();
        assert_eq!(old, Some(at(4)));
        assert_eq!(trace.borrow().requests.len(), calls);
        let report = context(|ctx| {
            heap.retire_value(
                &mut input,
                old,
                &mut allocator,
                ctx,
                |token, alloc, _, _| {
                    let mut bytes = buffer::Bytes::from_token(&mut token.bytes, alloc).unwrap();
                    assert_eq!(bytes.as_slice(alloc).unwrap(), [1]);
                    bytes.release(alloc).unwrap();
                    Ok(())
                },
            )
            .map_err(|_| 99)
        });
        assert!(report.primary().is_none());
        input.release_storage(&mut allocator).unwrap();
        release(&mut heap, right, node_type(), &mut allocator);
        let mut left = Some(left);
        let mut seen = Vec::new();
        let calls = trace.borrow().requests.len();
        let report = context(|ctx| {
            heap.release_strong(
                left.take().unwrap(),
                node_type(),
                &mut allocator,
                ctx,
                |token, alloc, _, _| {
                    let mut bytes = buffer::Bytes::from_token(&mut token.bytes, alloc).unwrap();
                    seen.push(bytes.as_slice(alloc).unwrap()[0]);
                    bytes.release(alloc).unwrap();
                    if panic && seen.len() == 1 {
                        panic!("contained shared descendant");
                    }
                    Err(7)
                },
            )
            .map_err(|_| 99)
        });
        assert_eq!(seen, [2, 3]);
        assert_eq!(
            report.primary().unwrap().kind,
            if panic {
                c::FailureKind::CleanupPanic
            } else {
                c::FailureKind::Cleanup(7)
            }
        );
        assert_eq!(report.secondary().len(), 1);
        assert_eq!(trace.borrow().requests.len(), calls);
        heap.close(&mut allocator).unwrap();
        assert_eq!(allocator.stats().live_bytes, 0);
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
    }
}

#[test]
fn ticket_reader_node_slot_value_and_work_refusals_precede_provider_calls_or_count_changes() {
    let types = types();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let base = g::Limits {
        nodes: 2,
        slots: 2,
        tickets: 8,
        readers: 1,
        value_cells: 2,
        ..Default::default()
    };
    for resource in 0..6 {
        let mut limits = base;
        match resource {
            0 => limits.nodes = 0,
            1 => limits.slots = 0,
            2 => limits.tickets = 0,
            3 => limits.value_cells = 0,
            4 => limits.publication_work = 0,
            _ => limits.reference_count = 0,
        };
        let mut heap =
            g::Heap::prepare(&layouts, limits, Default::default(), layouts.usage()).unwrap();
        let (mut allocator, _) = allocator(Default::default());
        heap.open(&mut allocator).unwrap();
        let mut input = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
        input
            .data_mut(&mut allocator)
            .unwrap()
            .copy_from_slice(&9i64.to_le_bytes());
        let before = allocator.stats();
        let error = heap
            .allocate_copy(&mut input, None, &mut allocator)
            .err()
            .unwrap();
        assert_eq!(
            error.kind,
            if resource == 5 {
                g::Kind::Ownership(l::OwnershipError::CountOverflow)
            } else {
                g::Kind::Ownership(l::OwnershipError::Allocation(buffer::AllocError::Limit))
            }
        );
        assert_eq!(allocator.stats(), before);
        assert_eq!(input.data(&allocator).unwrap(), 9i64.to_le_bytes());
        input.release_storage(&mut allocator).unwrap();
        heap.close(&mut allocator).unwrap();
    }
    for tickets in [1, 8] {
        let mut heap = g::Heap::prepare(
            &layouts,
            g::Limits {
                tickets,
                readers: 0,
                reference_count: 1,
                ..base
            },
            Default::default(),
            layouts.usage(),
        )
        .unwrap();
        let (mut allocator, _) = allocator(Default::default());
        heap.open(&mut allocator).unwrap();
        let mut input = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
        let strong = heap
            .allocate_copy(&mut input, None, &mut allocator)
            .unwrap();
        let before = allocator.stats();
        assert!(
            heap.clone_strong(&strong, o::Type::I64, &mut allocator)
                .is_err()
        );
        assert!(heap.read(&strong, o::Type::I64, &mut allocator).is_err());
        assert_eq!(
            heap.strong_count(&strong, o::Type::I64, &allocator)
                .unwrap(),
            1
        );
        assert_eq!(allocator.stats(), before);
        release(&mut heap, strong, o::Type::I64, &mut allocator);
        input.release_storage(&mut allocator).unwrap();
        heap.close(&mut allocator).unwrap();
    }
}

#[test]
fn duplicate_foreign_wrong_kind_and_malformed_container_payloads_are_rejected_before_publication() {
    let mut table = node_table();
    table.definitions.push(o::Definition {
        name: "Pair".into(),
        public: false,
        span: m::Span {
            source: m::SourceId(0),
            start: 0,
            end: 8,
        },
        kind: o::DefinitionKind::Record(
            (0..2)
                .map(|i| o::Field {
                    name: format!("edge{i}"),
                    span: at(i),
                    ty: o::TypeUse {
                        ty: ct(1),
                        span: at(i),
                    },
                })
                .collect(),
        ),
    });
    let types = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let limits = g::Limits {
        nodes: 4,
        slots: 4,
        tickets: 16,
        value_cells: 16,
        ..Default::default()
    };
    let mut heap = g::Heap::prepare(&layouts, limits, Default::default(), layouts.usage()).unwrap();
    let (mut allocator, _) = allocator(Default::default());
    heap.open(&mut allocator).unwrap();
    let mut input = node(&layouts, &mut allocator, 1, None, None);
    let child = heap
        .allocate_from(&mut input, Some(at(4)), &mut allocator)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let edge = heap
        .clone_strong(&child, node_type(), &mut allocator)
        .unwrap();
    let raw = edge.into_token();
    let mut duplicate = l::Reservation::allocate(
        &layouts,
        o::Type::Nominal(m::DefinitionId(1)),
        &mut allocator,
    )
    .unwrap();
    for i in 0..2 {
        let offset = duplicate
            .child(duplicate.root(), i, &allocator)
            .unwrap()
            .offset() as usize;
        duplicate.data_mut(&mut allocator).unwrap()[offset..offset + 32].copy_from_slice(&raw);
    }
    duplicate
        .initialize_order(
            duplicate.root(),
            [(0, at(0)), (1, at(1))].into_iter(),
            &mut allocator,
        )
        .unwrap();
    let before = allocator.stats();
    assert!(matches!(
        heap.allocate_from(&mut duplicate, Some(at(5)), &mut allocator)
            .err()
            .unwrap()
            .kind,
        g::Kind::Ownership(l::OwnershipError::Unavailable)
    ));
    assert_eq!(allocator.stats(), before);
    assert_eq!(
        heap.strong_count(&child, node_type(), &allocator).unwrap(),
        2
    );
    duplicate.release_storage(&mut allocator).unwrap();
    let mut raw = raw;
    release(
        &mut heap,
        g::Strong::from_token(&mut raw).unwrap(),
        node_type(),
        &mut allocator,
    );
    let mut other =
        g::Heap::prepare(&layouts, limits, Default::default(), layouts.usage()).unwrap();
    other.open(&mut allocator).unwrap();
    let before = allocator.stats();
    assert!(other.strong_count(&child, node_type(), &allocator).is_err());
    assert!(heap.strong_count(&child, o::Type::I64, &allocator).is_err());
    assert_eq!(allocator.stats(), before);
    other.close(&mut allocator).unwrap();
    let weak = heap.downgrade(&child, node_type(), &mut allocator).unwrap();
    let raw = weak.into_token();
    let mut wrong = raw;
    let wrong = g::Strong::from_token(&mut wrong).unwrap();
    let before = allocator.stats();
    assert!(heap.strong_count(&wrong, node_type(), &allocator).is_err());
    assert_eq!(allocator.stats(), before);
    let mut raw = raw;
    heap.release_weak(
        g::Weak::from_token(&mut raw).unwrap(),
        node_type(),
        &mut allocator,
    )
    .unwrap();
    let mut unsupported = l::Reservation::allocate(&layouts, ct(5), &mut allocator).unwrap();
    let before = allocator.stats();
    assert!(matches!(
        heap.allocate_from(&mut unsupported, Some(at(5)), &mut allocator)
            .err()
            .unwrap()
            .kind,
        g::Kind::Inspection(_)
    ));
    assert_eq!(allocator.stats(), before);
    unsupported.release_storage(&mut allocator).unwrap();
    release(&mut heap, child, node_type(), &mut allocator);
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn typed_publication_matches_an_independent_reachability_and_reference_count_oracle() {
    let types = node_model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 6,
            slots: 6,
            tickets: 64,
            value_cells: 32,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let (mut allocator, _) = allocator(Default::default());
    heap.open(&mut allocator).unwrap();
    let mut owners = Vec::new();
    for i in 0..6 {
        let mut input = node(&layouts, &mut allocator, i, None, None);
        owners.push(
            heap.allocate_from(&mut input, Some(at(4)), &mut allocator)
                .unwrap(),
        );
        input.release_storage(&mut allocator).unwrap();
    }
    let mut edges = [None; 6];
    let mut seed = 7u64;
    for step in 0..128 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let from = (seed % 6) as usize;
        let to = if step % 5 == 0 {
            None
        } else {
            Some(((seed >> 8) % 6) as usize)
        };
        let mut cursor = to;
        let mut cycle = false;
        while let Some(next) = cursor {
            if next == from {
                cycle = true;
                break;
            }
            cursor = edges[next];
        }
        let next = to.map(|i| {
            heap.clone_strong(&owners[i], node_type(), &mut allocator)
                .unwrap()
        });
        let back = Some(
            heap.downgrade(&owners[(from + 1) % 6], node_type(), &mut allocator)
                .unwrap(),
        );
        let mut input = node(&layouts, &mut allocator, step as u8, next, back);
        let result = heap.replace_from(
            &owners[from],
            node_type(),
            &mut input,
            at(6),
            &mut allocator,
        );
        if cycle {
            assert!(matches!(
                result.unwrap_err().kind,
                g::Kind::Ownership(l::OwnershipError::StrongCycle)
            ));
        } else {
            result.unwrap();
            edges[from] = to;
        }
        let report = context(|ctx| {
            heap.retire_value(&mut input, Some(at(6)), &mut allocator, ctx, clean)
                .map_err(|_| 99)
        });
        assert!(report.primary().is_none());
        input.release_storage(&mut allocator).unwrap();
        for (i, owner) in owners.iter().enumerate() {
            assert_eq!(
                heap.strong_count(owner, node_type(), &allocator).unwrap(),
                1 + edges.iter().filter(|edge| **edge == Some(i)).count() as u64
            );
        }
    }
    for owner in owners {
        release(&mut heap, owner, node_type(), &mut allocator);
    }
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
}

#[test]
fn shared_and_arena_preflight_charge_text_bytes_before_a_utf8_scan_or_provider_callback() {
    use lang_runtime::arena::managed as ar;
    let types = o::TypeTable {
        sources: vec![m::Source {
            path: "text.lang".into(),
            text: "x".into(),
        }],
        constructed: vec![o::Constructed {
            constructor: o::Constructor::Builtin(o::Builtin::Text),
            span: span(),
        }],
        ..Default::default()
    }
    .verify(Default::default())
    .unwrap();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 2,
            slots: 2,
            tickets: 4,
            value_cells: 8,
            publication_work: 16,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let mut arena = ar::Arena::prepare(
        &layouts,
        ar::Limits {
            nodes: 2,
            value_cells: 8,
            readers: 2,
            publication_work: 16,
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let (mut allocator, _) = allocator(Default::default());
    heap.open(&mut allocator).unwrap();
    arena.open(&mut allocator).unwrap();
    let mut bytes = buffer::Bytes::from_slice(&mut allocator, &[b'x'; 64]).unwrap();
    let mut text = bytes.take_text(&mut allocator).unwrap();
    let token = text.take_token(&allocator).unwrap();
    let mut input = l::Reservation::allocate(&layouts, ct(0), &mut allocator).unwrap();
    input
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&token);
    let before = allocator.stats();
    assert_eq!(
        heap.allocate_from(&mut input, Some(span()), &mut allocator)
            .err()
            .unwrap()
            .kind,
        g::Kind::Ownership(l::OwnershipError::Allocation(buffer::AllocError::Limit))
    );
    assert_eq!(allocator.stats(), before);
    assert_eq!(
        arena
            .allocate_from(
                &mut input,
                Some(span()),
                &mut allocator,
                |_, _, _, _| panic!("no container")
            )
            .err()
            .unwrap()
            .kind,
        ar::Kind::Arena(l::ArenaError::Allocation(buffer::AllocError::Limit))
    );
    assert_eq!(allocator.stats(), before);
    assert_eq!(input.data(&allocator).unwrap(), token);
    let report = context(|ctx| {
        input
            .cleanup(&mut allocator, ctx, |token, alloc, _, _| {
                buffer::cleanup_token(&types, token, alloc).map_err(|_| 99)
            })
            .map_err(|_| 99)
    });
    assert!(report.primary().is_none());
    heap.close(&mut allocator).unwrap();
    context(|ctx| {
        arena
            .cleanup(ar::Exit::Normal, &mut allocator, ctx, |_, _, _, _| {
                panic!("no node")
            })
            .map_err(|_| 99)
    });
    assert_eq!(allocator.stats().live_bytes, 0);
}

fn read_byte<P: a::Provider>(
    heap: &g::Heap<'_, '_>,
    read: g::Read,
    allocator: &a::Allocator<P>,
) -> u8 {
    let view = heap.view(read, node_type(), allocator).unwrap();
    let mut token = view
        .read_token(view.child(view.root(), 0, allocator).unwrap(), allocator)
        .unwrap();
    let value = buffer::Bytes::from_token(&mut token.bytes, allocator).unwrap();
    value.as_slice(allocator).unwrap()[0]
}
#[test]
fn a_stored_shared_field_can_be_read_and_replaced_without_an_extra_clone_or_parent_mutation() {
    let types = node_model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 2,
            slots: 2,
            tickets: 12,
            value_cells: 16,
            reference_count: 2,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let (mut allocator, _) = allocator(Default::default());
    heap.open(&mut allocator).unwrap();
    let mut input = node(&layouts, &mut allocator, 7, None, None);
    let child = heap
        .allocate_from(&mut input, Some(at(4)), &mut allocator)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let mut input = node(&layouts, &mut allocator, 8, Some(child), None);
    let parent = heap
        .allocate_from(&mut input, Some(at(5)), &mut allocator)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let parent_read = heap.read(&parent, node_type(), &mut allocator).unwrap();
    let view = heap.view(parent_read, node_type(), &allocator).unwrap();
    let field = view
        .child(
            view.child(view.root(), 1, &allocator).unwrap(),
            0,
            &allocator,
        )
        .unwrap();
    let child_read = heap
        .read_field(parent_read, node_type(), field, &mut allocator)
        .unwrap();
    assert_eq!(read_byte(&heap, child_read, &allocator), 7);
    let mut incoming = node(&layouts, &mut allocator, 9, None, None);
    let original = incoming.data(&allocator).unwrap().to_vec();
    let before = allocator.stats();
    assert_eq!(
        heap.replace_field_from(
            parent_read,
            node_type(),
            field,
            &mut incoming,
            at(6),
            &mut allocator
        )
        .unwrap_err()
        .kind,
        g::Kind::Ownership(l::OwnershipError::BorrowConflict)
    );
    assert_eq!(allocator.stats(), before);
    assert_eq!(incoming.data(&allocator).unwrap(), original);
    context(|ctx| {
        heap.end_read(child_read, node_type(), &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    let old = heap
        .replace_field_from(
            parent_read,
            node_type(),
            field,
            &mut incoming,
            at(6),
            &mut allocator,
        )
        .unwrap();
    assert_eq!(old, Some(at(4)));
    context(|ctx| {
        heap.retire_value(&mut incoming, old, &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    incoming.release_storage(&mut allocator).unwrap();
    let child_read = heap
        .read_field(parent_read, node_type(), field, &mut allocator)
        .unwrap();
    assert_eq!(read_byte(&heap, child_read, &allocator), 9);
    context(|ctx| {
        heap.end_read(child_read, node_type(), &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    context(|ctx| {
        heap.end_read(parent_read, node_type(), &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    assert!(
        heap.read_field(parent_read, node_type(), field, &mut allocator)
            .is_err()
    );
    release(&mut heap, parent, node_type(), &mut allocator);
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
}

#[test]
fn a_copy_shared_field_replacement_preserves_the_rhs_and_read_lease_counts() {
    let mut table = node_table();
    table.constructed.push(o::Constructed {
        constructor: o::Constructor::Shared(o::Type::I64),
        span: at(0),
    });
    table.definitions.push(o::Definition {
        name: "Scalar".into(),
        public: false,
        span: m::Span {
            source: m::SourceId(0),
            start: 0,
            end: 8,
        },
        kind: o::DefinitionKind::Record(vec![o::Field {
            name: "value".into(),
            span: at(0),
            ty: o::TypeUse {
                ty: ct(6),
                span: at(0),
            },
        }]),
    });
    let types = table.verify(Default::default()).unwrap();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let holder = o::Type::Nominal(m::DefinitionId(1));
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 2,
            slots: 2,
            tickets: 12,
            value_cells: 16,
            reference_count: 2,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    heap.open(&mut allocator).unwrap();
    let mut scalar = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    scalar
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&5i64.to_le_bytes());
    let strong = heap
        .allocate_copy(&mut scalar, Some(at(4)), &mut allocator)
        .unwrap();
    let mut input = l::Reservation::allocate(&layouts, holder, &mut allocator).unwrap();
    input
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&strong.into_token());
    input
        .initialize_order(input.root(), [(0, at(0))].into_iter(), &mut allocator)
        .unwrap();
    let parent = heap
        .allocate_from(&mut input, Some(at(5)), &mut allocator)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let read = heap.read(&parent, holder, &mut allocator).unwrap();
    let view = heap.view(read, holder, &allocator).unwrap();
    let field = view.child(view.root(), 0, &allocator).unwrap();
    let guard = heap
        .read_field(read, holder, field, &mut allocator)
        .unwrap();
    let calls = trace.borrow().requests.len();
    scalar
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&9i64.to_le_bytes());
    assert!(matches!(
        heap.replace_field_copy(read, holder, field, &mut scalar, at(6), &mut allocator)
            .unwrap_err()
            .kind,
        g::Kind::Ownership(l::OwnershipError::BorrowConflict)
    ));
    context(|ctx| {
        heap.end_read(guard, o::Type::I64, &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    heap.replace_field_copy(read, holder, field, &mut scalar, at(6), &mut allocator)
        .unwrap();
    assert_eq!(scalar.data(&allocator).unwrap(), 9i64.to_le_bytes());
    let guard = heap
        .read_field(read, holder, field, &mut allocator)
        .unwrap();
    let view = heap.view(guard, o::Type::I64, &allocator).unwrap();
    assert_eq!(
        view.read_immediate(view.root(), &allocator).unwrap(),
        l::Immediate::I64(9)
    );
    context(|ctx| {
        heap.end_read(guard, o::Type::I64, &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    assert_eq!(trace.borrow().requests.len(), calls);
    context(|ctx| {
        heap.end_read(read, holder, &mut allocator, ctx, clean)
            .map_err(|_| 99)
    });
    release(&mut heap, parent, holder, &mut allocator);
    scalar.release_storage(&mut allocator).unwrap();
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_bytes, 0);
}
