use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{allocation as a, arena::managed as g, buffer, cleanup as c, layout as l};
use std::{cell::RefCell, rc::Rc};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1,
    }
}
fn model() -> o::VerifiedTypes {
    let mut table = o::TypeTable {
        sources: vec![m::Source {
            path: "arena.lang".into(),
            text: "x".into(),
        }],
        ..Default::default()
    };
    for constructor in [
        o::Constructor::Builtin(o::Builtin::Bytes),
        o::Constructor::Arena(o::Type::Nominal(m::DefinitionId(0))),
        o::Constructor::Option(o::Type::Constructed(o::ConstructedId(1))),
        o::Constructor::Vec(o::Type::Constructed(o::ConstructedId(1))),
        o::Constructor::Shared(o::Type::I64),
        o::Constructor::Weak(o::Type::I64),
    ] {
        table.constructed.push(o::Constructed {
            constructor,
            span: span(),
        });
    }
    table.definitions.push(o::Definition {
        name: "Node".into(),
        public: false,
        span: span(),
        kind: o::DefinitionKind::Record(vec![
            o::Field {
                name: "bytes".into(),
                span: span(),
                ty: o::TypeUse {
                    ty: ct(0),
                    span: span(),
                },
            },
            o::Field {
                name: "next".into(),
                span: span(),
                ty: o::TypeUse {
                    ty: ct(2),
                    span: span(),
                },
            },
        ]),
    });
    table.verify(Default::default()).unwrap()
}
fn ct(id: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(id))
}
fn node_type() -> o::Type {
    o::Type::Nominal(m::DefinitionId(0))
}
fn prepare<'l, 't>(layouts: &'l l::Layouts<'t>, limits: g::Limits) -> g::Arena<'l, 't> {
    g::Arena::prepare(layouts, limits, Default::default(), layouts.usage()).unwrap()
}
fn no_containers<P: a::Provider>(
    token: &l::Token,
    _: &g::Arena<'_, '_>,
    _: &a::Allocator<P>,
    _: &mut g::PublicationBudget,
) -> Result<(), g::Error> {
    Err(g::Error {
        kind: g::Kind::Unsupported(token.leaf.ty),
        span: None,
    })
}
fn make_node<'l, 't, P: a::Provider>(
    layouts: &'l l::Layouts<'t>,
    allocator: &mut a::Allocator<P>,
    value: u8,
    next: Option<g::Reference>,
) -> l::Reservation<'l, 't> {
    let cell = l::Reservation::allocate(layouts, node_type(), allocator).unwrap();
    let field = cell.child(cell.root(), 0, allocator).unwrap();
    let mut bytes = buffer::Bytes::from_slice(allocator, &[value]).unwrap();
    let token = bytes.take_token(allocator).unwrap();
    let offset = field.offset() as usize;
    cell.data_mut(allocator).unwrap()[offset..offset + 32].copy_from_slice(&token);
    let link = cell.child(cell.root(), 1, allocator).unwrap();
    cell.prepare_case(link, u32::from(next.is_some()), allocator)
        .unwrap();
    if let Some(reference) = next {
        let payload = cell.child(link, 0, allocator).unwrap();
        let start = payload.offset() as usize;
        cell.data_mut(allocator).unwrap()[start..start + 32].copy_from_slice(&reference.token());
    }
    cell.initialize_order(
        cell.root(),
        [(1, span()), (0, span())].into_iter(),
        allocator,
    )
    .unwrap();
    cell
}
fn retire<P: a::Provider>(
    arena: &mut g::Arena<'_, '_>,
    allocator: &mut a::Allocator<P>,
    exit: g::Exit,
    mut cleaner: impl FnMut(
        &mut l::Token,
        &mut a::Allocator<P>,
        c::Reason,
        &mut c::Context<'_>,
    ) -> Result<(), u32>,
) -> c::Report {
    let mut instance = c::Instance::default();
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |_: &mut (), _, context: &mut c::Context<'_>| {
                arena
                    .cleanup(exit, allocator, context, &mut cleaner)
                    .map_err(|_| 99)
            },
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    frame.initialize(c::Slot(0), (), Some(span())).unwrap();
    frame.finish(c::Exit::Return(None)).report
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

#[test]
fn each_payload_oom_keeps_the_operand_and_published_node_and_refunds_only_live_storage() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for fail_at in [1, 2, 3] {
        let (mut allocator, trace) = allocator(Default::default());
        let mut arena = prepare(&layouts, Default::default());
        arena.open(&mut allocator).unwrap();
        let mut input = make_node(&layouts, &mut allocator, 7, None);
        let first = arena
            .allocate_from(&mut input, Some(span()), &mut allocator, no_containers)
            .unwrap();
        input.release_storage(&mut allocator).unwrap();
        let mut incoming = make_node(&layouts, &mut allocator, 9, Some(first));
        let bytes = incoming.data(&allocator).unwrap().to_vec();
        let before = allocator.stats();
        let calls = trace.borrow().requests.len();
        trace.borrow_mut().fail = Some(calls + fail_at);
        let error = arena
            .allocate_from(&mut incoming, Some(span()), &mut allocator, no_containers)
            .unwrap_err();
        assert_eq!(
            error.kind,
            g::Kind::Arena(l::ArenaError::Allocation(buffer::AllocError::OutOfMemory))
        );
        assert_eq!(error.span, Some(span()));
        assert_eq!(incoming.data(&allocator).unwrap(), bytes);
        incoming.validate_retirement(&mut allocator).unwrap();
        assert_eq!(allocator.stats().live_bytes, before.live_bytes);
        assert_eq!(allocator.stats().live_allocations, before.live_allocations);
        let authorized: u64 = trace.borrow().requests[calls..]
            .iter()
            .map(|r| r.bytes())
            .sum();
        assert_eq!(
            allocator.stats().cumulative_bytes,
            before.cumulative_bytes + authorized
        );
        assert_eq!(trace.borrow().requests.len() - calls, fail_at);
        arena.resolve(first, node_type(), &allocator).unwrap();
        trace.borrow_mut().fail = None;
        let second = arena
            .allocate_from(&mut incoming, Some(span()), &mut allocator, no_containers)
            .unwrap();
        incoming.release_storage(&mut allocator).unwrap();
        arena.resolve(second, node_type(), &allocator).unwrap();
        let report = retire(
            &mut arena,
            &mut allocator,
            g::Exit::Normal,
            |token, allocator, _, _| buffer::cleanup_token(&types, token, allocator).map_err(|_| 1),
        );
        assert!(report.primary().is_none());
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
    }
}

#[test]
fn cycles_replace_atomically_and_descendants_retire_once_in_reverse_node_order() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut arena = prepare(&layouts, Default::default());
    arena.open(&mut allocator).unwrap();
    let mut input = make_node(&layouts, &mut allocator, 1, None);
    let first = arena
        .allocate_from(&mut input, Some(span()), &mut allocator, no_containers)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let mut input = make_node(&layouts, &mut allocator, 2, Some(first));
    let second = arena
        .allocate_from(&mut input, Some(span()), &mut allocator, no_containers)
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    let mut input = make_node(&layouts, &mut allocator, 3, Some(second));
    let calls = trace.borrow().requests.len();
    arena
        .replace_from(first, &mut input, span(), &mut allocator, no_containers)
        .unwrap();
    assert_eq!(trace.borrow().requests.len(), calls);
    // The node now owns 3 and points to the second node, which points back.
    let mut seen = Vec::new();
    let report = retire(
        &mut arena,
        &mut allocator,
        g::Exit::Normal,
        |token, allocator, _, context| {
            assert_eq!(context.span(), Some(span()));
            let mut bytes = token.bytes;
            let view = buffer::Bytes::from_token(&mut bytes, allocator).unwrap();
            seen.push(view.as_slice(allocator).unwrap()[0]);
            buffer::cleanup_token(&types, token, allocator).map_err(|_| 1)
        },
    );
    assert!(report.primary().is_none());
    assert_eq!(seen, [2, 3]);
    // The old first payload is still owned by the replacement operand.
    let before = allocator.stats().live_allocations;
    assert_eq!(before, 3);
    let mut instance = c::Instance::default();
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |value: &mut l::Reservation<'_, '_>, _, context: &mut c::Context<'_>| {
                value
                    .cleanup(&mut allocator, context, |token, allocator, _, _| {
                        buffer::cleanup_token(&types, token, allocator).map_err(|_| 1)
                    })
                    .map_err(|_| 2)
            },
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    assert!(frame.initialize(c::Slot(0), input, Some(span())).is_ok());
    assert!(
        frame
            .finish(c::Exit::Return(None))
            .report
            .primary()
            .is_none()
    );
    assert_eq!(allocator.stats().live_allocations, 0);
}

fn types() -> o::VerifiedTypes {
    o::TypeTable::default().verify(Default::default()).unwrap()
}
#[test]
fn scalar_arena_storage_is_charged_and_close_revokes_before_reverse_drain() {
    let types = types();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut arena = g::Arena::prepare(
        &layouts,
        g::Limits {
            nodes: 2,
            readers: 0,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let mut allocator = a::Allocator::new(
        a::System,
        a::Limits {
            live_allocations: 16,
            ..Default::default()
        },
    )
    .unwrap();
    arena.open(&mut allocator).unwrap();
    let mut input = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    input
        .write_immediate(input.root(), l::Immediate::I64(7), &mut allocator)
        .unwrap();
    let first = arena
        .allocate_from(
            &mut input,
            None,
            &mut allocator,
            |_, _, _, _| unreachable!(),
        )
        .unwrap();
    input.recycle(&mut allocator).unwrap();
    input
        .write_immediate(input.root(), l::Immediate::I64(9), &mut allocator)
        .unwrap();
    let second = arena
        .allocate_from(
            &mut input,
            None,
            &mut allocator,
            |_, _, _, _| unreachable!(),
        )
        .unwrap();
    input.release_storage(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 7);
    assert_eq!(allocator.stats().live_bytes, 40 + 2 * (56 + 8));
    arena.begin_close(g::Exit::Normal, &mut allocator).unwrap();
    assert_eq!(
        arena
            .resolve(first, o::Type::I64, &allocator)
            .unwrap_err()
            .kind,
        g::Kind::Arena(l::ArenaError::Unavailable)
    );
    for (reference, value) in [(second, 9), (first, 7)] {
        let mut retired = arena.take_last(&mut allocator).unwrap().unwrap();
        assert_eq!(retired.reference, reference);
        assert_eq!(
            retired
                .payload
                .read_immediate(retired.payload.root(), &allocator)
                .unwrap(),
            l::Immediate::I64(value)
        );
        retired.payload.release_storage(&mut allocator).unwrap();
    }
    assert!(arena.take_last(&mut allocator).unwrap().is_none());
    arena.finish_close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
    arena.open(&mut allocator).unwrap();
    let mut value = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    value
        .write_immediate(value.root(), l::Immediate::I64(11), &mut allocator)
        .unwrap();
    let fresh = arena
        .allocate_from(&mut value, None, &mut allocator, no_containers)
        .unwrap();
    value.release_storage(&mut allocator).unwrap();
    arena.resolve(fresh, o::Type::I64, &allocator).unwrap();
    assert_eq!(
        arena
            .resolve(first, o::Type::I64, &allocator)
            .unwrap_err()
            .kind,
        g::Kind::Arena(l::ArenaError::Unavailable)
    );
    arena.begin_close(g::Exit::Normal, &mut allocator).unwrap();
    arena
        .take_last(&mut allocator)
        .unwrap()
        .unwrap()
        .payload
        .release_storage(&mut allocator)
        .unwrap();
    arena.finish_close(&mut allocator).unwrap();
}

fn clean_value<P: a::Provider>(
    value: &mut l::Reservation<'_, '_>,
    allocator: &mut a::Allocator<P>,
    types: &o::VerifiedTypes,
) {
    let mut instance = c::Instance::default();
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |_: &mut (), _, context: &mut c::Context<'_>| {
                value
                    .cleanup(allocator, context, |token, allocator, _, _| {
                        buffer::cleanup_token(types, token, allocator).map_err(|_| 1)
                    })
                    .map_err(|_| 2)
            },
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    frame.initialize(c::Slot(0), (), Some(span())).unwrap();
    assert!(
        frame
            .finish(c::Exit::Return(None))
            .report
            .primary()
            .is_none()
    );
}

#[test]
fn cross_region_stale_and_forged_nested_tokens_reject_before_callbacks_without_replacement() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut left = prepare(&layouts, Default::default());
    let mut right = prepare(&layouts, Default::default());
    left.open(&mut allocator).unwrap();
    right.open(&mut allocator).unwrap();
    let mut value = make_node(&layouts, &mut allocator, 1, None);
    let a = left
        .allocate_from(&mut value, Some(span()), &mut allocator, no_containers)
        .unwrap();
    value.release_storage(&mut allocator).unwrap();
    let mut value = make_node(&layouts, &mut allocator, 2, None);
    let b = right
        .allocate_from(&mut value, Some(span()), &mut allocator, no_containers)
        .unwrap();
    value.release_storage(&mut allocator).unwrap();
    let mut incoming = make_node(&layouts, &mut allocator, 3, Some(b));
    let bytes = incoming.data(&allocator).unwrap().to_vec();
    let before = allocator.stats();
    let calls = trace.borrow().requests.len();
    for replace in [false, true] {
        let error = if replace {
            left.replace_from(a, &mut incoming, span(), &mut allocator, no_containers)
        } else {
            left.allocate_from(&mut incoming, Some(span()), &mut allocator, no_containers)
                .map(|_| ())
        }
        .unwrap_err();
        assert_eq!(error.kind, g::Kind::Arena(l::ArenaError::CrossRegion));
        assert_eq!(incoming.data(&allocator).unwrap(), bytes);
        assert_eq!(allocator.stats(), before);
        assert_eq!(trace.borrow().requests.len(), calls);
    }
    right.begin_close(g::Exit::Normal, &mut allocator).unwrap();
    assert_eq!(
        left.allocate_from(&mut incoming, Some(span()), &mut allocator, no_containers)
            .unwrap_err()
            .kind,
        g::Kind::Arena(l::ArenaError::Unavailable)
    );
    let link = incoming.child(incoming.root(), 1, &allocator).unwrap();
    let link = incoming.child(link, 0, &allocator).unwrap();
    let mut forged = a.token();
    forged[24..].copy_from_slice(&u64::MAX.to_le_bytes());
    let offset = link.offset() as usize;
    incoming.data_mut(&mut allocator).unwrap()[offset..offset + 32].copy_from_slice(&forged);
    assert_eq!(
        left.allocate_from(&mut incoming, Some(span()), &mut allocator, no_containers)
            .unwrap_err()
            .kind,
        g::Kind::Arena(l::ArenaError::Unavailable)
    );
    assert_eq!(
        left.resolve(a, o::Type::I64, &allocator).unwrap_err().kind,
        g::Kind::Arena(l::ArenaError::Unavailable)
    );
    assert!(g::Reference::from_token([0; 32]).is_err());
    assert_eq!(allocator.stats(), before);
    clean_value(&mut incoming, &mut allocator, &types);
    for arena in [&mut left, &mut right] {
        assert!(
            retire(
                arena,
                &mut allocator,
                g::Exit::Normal,
                |token, allocator, _, _| buffer::cleanup_token(&types, token, allocator)
                    .map_err(|_| 1)
            )
            .primary()
            .is_none()
        );
    }
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn reads_exclude_all_node_mutations_and_abort_invalidates_abandoned_read_slots() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut arena = prepare(
        &layouts,
        g::Limits {
            readers: 1,
            ..Default::default()
        },
    );
    arena.open(&mut allocator).unwrap();
    let mut value = make_node(&layouts, &mut allocator, 1, None);
    let first = arena
        .allocate_from(&mut value, Some(span()), &mut allocator, no_containers)
        .unwrap();
    value.release_storage(&mut allocator).unwrap();
    let mut incoming = make_node(&layouts, &mut allocator, 2, Some(first));
    let mut read = arena.read(first, node_type(), &mut allocator).unwrap();
    assert_eq!(
        arena.payload(&read, &allocator).unwrap().root().ty(),
        node_type()
    );
    let before = allocator.stats();
    assert_eq!(
        arena
            .read(first, node_type(), &mut allocator)
            .err()
            .unwrap()
            .kind,
        g::Kind::Arena(l::ArenaError::Allocation(buffer::AllocError::Limit))
    );
    assert_eq!(
        arena
            .allocate_from(&mut incoming, Some(span()), &mut allocator, no_containers)
            .unwrap_err()
            .kind,
        g::Kind::Arena(l::ArenaError::BorrowConflict)
    );
    assert_eq!(
        arena
            .replace_from(first, &mut incoming, span(), &mut allocator, no_containers)
            .unwrap_err()
            .kind,
        g::Kind::Arena(l::ArenaError::BorrowConflict)
    );
    assert_eq!(
        arena
            .begin_close(g::Exit::Normal, &mut allocator)
            .unwrap_err()
            .kind,
        g::Kind::Arena(l::ArenaError::BorrowConflict)
    );
    assert_eq!(allocator.stats(), before);
    arena.end_read(&mut read, &mut allocator).unwrap();
    assert!(arena.end_read(&mut read, &mut allocator).is_err());
    let second = arena
        .allocate_from(&mut incoming, Some(span()), &mut allocator, no_containers)
        .unwrap();
    incoming.release_storage(&mut allocator).unwrap();
    let mut abandoned = arena.read(second, node_type(), &mut allocator).unwrap();
    let calls = trace.borrow().requests.len();
    assert!(
        retire(
            &mut arena,
            &mut allocator,
            g::Exit::Abort,
            |token, allocator, _, _| buffer::cleanup_token(&types, token, allocator).map_err(|_| 1)
        )
        .primary()
        .is_none()
    );
    assert_eq!(trace.borrow().requests.len(), calls);
    assert!(arena.payload(&abandoned, &allocator).is_err());
    arena.open(&mut allocator).unwrap();
    assert!(arena.end_read(&mut abandoned, &mut allocator).is_err());
    assert_eq!(
        arena
            .resolve(first, node_type(), &allocator)
            .unwrap_err()
            .kind,
        g::Kind::Arena(l::ArenaError::Unavailable)
    );
    arena.begin_close(g::Exit::Normal, &mut allocator).unwrap();
    arena.finish_close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn guest_limits_preflight_complete_storage_plans_and_exact_peak_is_sufficient() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let requests = layouts
        .get(node_type())
        .unwrap()
        .storage_requests()
        .unwrap();
    let payload = requests.iter().map(|r| r.bytes()).sum::<u64>();
    let extra = 56 + payload;
    // Real region/read metadata, input payload/order/work, Bytes header/data.
    let base = 40 + 16 * g::Limits::default().readers as u64 + payload + 25;
    for denied in 0..4 {
        let mut limits = a::Limits {
            live_allocations: 7,
            live_bytes: base + extra,
            cumulative_bytes: base + extra,
            ..Default::default()
        };
        match denied {
            0 => limits.live_allocations -= 1,
            1 => limits.live_bytes -= 1,
            2 => limits.cumulative_bytes -= 1,
            _ => {}
        }
        let (mut allocator, trace) = allocator(limits);
        let mut arena = prepare(&layouts, Default::default());
        arena.open(&mut allocator).unwrap();
        let mut incoming = make_node(&layouts, &mut allocator, 7, None);
        let before = allocator.stats();
        let bytes = incoming.data(&allocator).unwrap().to_vec();
        let calls = trace.borrow().requests.len();
        let result =
            arena.allocate_from(&mut incoming, Some(span()), &mut allocator, no_containers);
        if denied < 3 {
            assert_eq!(
                result.unwrap_err().kind,
                g::Kind::Arena(l::ArenaError::Allocation(buffer::AllocError::Limit))
            );
            assert_eq!(allocator.stats(), before);
            assert_eq!(incoming.data(&allocator).unwrap(), bytes);
            assert_eq!(trace.borrow().requests.len(), calls);
            clean_value(&mut incoming, &mut allocator, &types);
        } else {
            result.unwrap();
            assert_eq!(allocator.stats().peak_live_bytes, base + extra);
            assert_eq!(allocator.stats().peak_live_allocations, 7);
            assert_eq!(allocator.stats().cumulative_bytes, base + extra);
            incoming.release_storage(&mut allocator).unwrap();
        }
        let calls = trace.borrow().requests.len();
        assert!(
            retire(
                &mut arena,
                &mut allocator,
                g::Exit::Normal,
                |token, allocator, _, _| buffer::cleanup_token(&types, token, allocator)
                    .map_err(|_| 1)
            )
            .primary()
            .is_none()
        );
        assert_eq!(trace.borrow().requests.len(), calls);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn cleanup_failures_and_panics_continue_nodes_without_allocation_or_retry() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut arena = prepare(&layouts, Default::default());
    arena.open(&mut allocator).unwrap();
    for byte in 1..=3 {
        let mut input = make_node(&layouts, &mut allocator, byte, None);
        arena
            .allocate_from(&mut input, Some(span()), &mut allocator, no_containers)
            .unwrap();
        input.release_storage(&mut allocator).unwrap();
    }
    let calls = trace.borrow().requests.len();
    let mut seen = Vec::new();
    let report = retire(
        &mut arena,
        &mut allocator,
        g::Exit::Abort,
        |token, allocator, _, _| {
            let mut bytes = token.bytes;
            let value = buffer::Bytes::from_token(&mut bytes, allocator)
                .unwrap()
                .as_slice(allocator)
                .unwrap()[0];
            seen.push(value);
            buffer::cleanup_token(&types, token, allocator).unwrap();
            match value {
                3 => Err(73),
                2 => panic!("contained descendant failure"),
                _ => Ok(()),
            }
        },
    );
    assert_eq!(seen, [3, 2, 1]);
    assert_eq!(report.primary().unwrap().kind, c::FailureKind::Cleanup(73));
    assert_eq!(report.secondary().len(), 1);
    assert_eq!(report.secondary()[0].kind, c::FailureKind::CleanupPanic);
    assert_eq!(trace.borrow().requests.len(), calls);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
    let calls = trace.borrow().releases.len();
    assert!(
        retire(
            &mut arena,
            &mut allocator,
            g::Exit::Abort,
            |_, _, _, _| panic!("no retry")
        )
        .primary()
        .is_none()
    );
    assert_eq!(trace.borrow().releases.len(), calls);
}

#[test]
fn arena_graph_and_host_pool_limits_are_exact_and_do_not_hide_guest_allocations() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let small = g::Limits {
        nodes: 1,
        value_cells: 4,
        publication_work: 8,
        readers: 1,
    };
    let measured = prepare(&layouts, small).usage();
    let exact = m::Limits {
        work: measured.work,
        allocations: measured.allocation_bytes,
        elements: measured.elements,
        ..Default::default()
    };
    assert_eq!(
        g::Arena::prepare(&layouts, small, exact, layouts.usage())
            .unwrap()
            .usage(),
        measured
    );
    for limits in [
        m::Limits {
            work: exact.work - 1,
            ..exact
        },
        m::Limits {
            allocations: exact.allocations - 1,
            ..exact
        },
        m::Limits {
            elements: exact.elements - 1,
            ..exact
        },
    ] {
        assert!(g::Arena::prepare(&layouts, small, limits, layouts.usage()).is_err());
    }
    assert!(g::Arena::prepare(&layouts, small, Default::default(), m::Usage::default()).is_err());
    for limits in [
        g::Limits { nodes: 0, ..small },
        g::Limits {
            value_cells: 0,
            ..small
        },
        g::Limits {
            publication_work: 0,
            ..small
        },
    ] {
        let mut arena = prepare(&layouts, limits);
        let (mut allocator, trace) = allocator(Default::default());
        arena.open(&mut allocator).unwrap();
        let mut incoming = make_node(&layouts, &mut allocator, 1, None);
        let before = allocator.stats();
        let calls = trace.borrow().requests.len();
        assert_eq!(
            arena
                .allocate_from(&mut incoming, Some(span()), &mut allocator, no_containers)
                .unwrap_err()
                .kind,
            g::Kind::Arena(l::ArenaError::Allocation(buffer::AllocError::Limit))
        );
        assert_eq!(allocator.stats(), before);
        assert_eq!(trace.borrow().requests.len(), calls);
        clean_value(&mut incoming, &mut allocator, &types);
        arena.begin_close(g::Exit::Normal, &mut allocator).unwrap();
        arena.finish_close(&mut allocator).unwrap();
        assert_eq!(allocator.stats().live_allocations, 0);
    }
    let (mut allocator, trace) = allocator(Default::default());
    let mut arena = prepare(&layouts, small);
    arena.open(&mut allocator).unwrap();
    let mut input = make_node(&layouts, &mut allocator, 1, None);
    arena
        .allocate_from(&mut input, Some(span()), &mut allocator, no_containers)
        .unwrap();
    assert!(input.validate_retirement(&mut allocator).is_err());
    input.release_storage(&mut allocator).unwrap();
    let mut input = make_node(&layouts, &mut allocator, 2, None);
    let before = trace.borrow().requests.len();
    assert_eq!(
        arena
            .allocate_from(&mut input, Some(span()), &mut allocator, no_containers)
            .unwrap_err()
            .kind,
        g::Kind::Arena(l::ArenaError::Allocation(buffer::AllocError::Limit))
    );
    assert_eq!(trace.borrow().requests.len(), before);
    clean_value(&mut input, &mut allocator, &types);
    assert!(
        retire(
            &mut arena,
            &mut allocator,
            g::Exit::Normal,
            |token, allocator, _, _| buffer::cleanup_token(&types, token, allocator).map_err(|_| 1)
        )
        .primary()
        .is_none()
    );
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn owner_oom_remains_closed_and_can_retry_without_refunding_cumulative_requests() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut arena = prepare(&layouts, Default::default());
    let (mut allocator, trace) = allocator(Default::default());
    let opening = arena.opening_request().bytes();
    trace.borrow_mut().fail = Some(1);
    assert_eq!(
        arena.open(&mut allocator).unwrap_err().kind,
        g::Kind::Arena(l::ArenaError::Allocation(buffer::AllocError::OutOfMemory))
    );
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().cumulative_bytes, opening);
    trace.borrow_mut().fail = None;
    arena.open(&mut allocator).unwrap();
    assert_eq!(allocator.stats().cumulative_bytes, opening * 2);
    assert!(arena.open(&mut allocator).is_err());
    arena.begin_close(g::Exit::Normal, &mut allocator).unwrap();
    arena.finish_close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(trace.borrow().releases.len(), 1);
}

#[test]
fn copy_value_shape_shared_edges_and_invalid_spans_fail_before_provider_calls() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut arena = prepare(&layouts, Default::default());
    let (mut allocator, trace) = allocator(Default::default());
    arena.open(&mut allocator).unwrap();
    for ty in [o::Type::Bool, ct(2), ct(4), ct(5)] {
        let mut input = l::Reservation::allocate(&layouts, ty, &mut allocator).unwrap();
        match ty {
            o::Type::Bool => input.data_mut(&mut allocator).unwrap()[0] = 2,
            _ if ty == ct(2) => {
                input.data_mut(&mut allocator).unwrap()[..4].copy_from_slice(&9u32.to_le_bytes())
            }
            _ => {}
        }
        let before = allocator.stats();
        let error = arena
            .allocate_from(&mut input, Some(span()), &mut allocator, |_, _, _, _| {
                panic!("shared/weak must not delegate validation")
            })
            .unwrap_err();
        if ty == ct(4) || ty == ct(5) {
            assert_eq!(error.kind, g::Kind::Arena(l::ArenaError::CrossRegion));
        } else {
            assert!(matches!(error.kind, g::Kind::Layout(_)));
        }
        assert_eq!(allocator.stats(), before);
        input.release_storage(&mut allocator).unwrap();
    }
    let mut input = make_node(&layouts, &mut allocator, 1, None);
    let before = trace.borrow().requests.len();
    let invalid = m::Span {
        source: m::SourceId(99),
        ..span()
    };
    assert_eq!(
        arena
            .allocate_from(&mut input, Some(invalid), &mut allocator, no_containers)
            .unwrap_err()
            .kind,
        g::Kind::Layout(l::Error::InvalidSpan)
    );
    assert_eq!(trace.borrow().requests.len(), before);
    clean_value(&mut input, &mut allocator, &types);
    arena.begin_close(g::Exit::Normal, &mut allocator).unwrap();
    arena.finish_close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn indirect_service_validation_must_share_the_walk_budget_and_cannot_skip_unknown_tokens() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut arena = prepare(
        &layouts,
        g::Limits {
            publication_work: 1,
            ..Default::default()
        },
    );
    arena.open(&mut allocator).unwrap();
    // Synthetic unpublished service input: no Vec implementation is claimed.
    // The service contract must reject rather than treating it as a scalar.
    let mut input = l::Reservation::allocate(&layouts, ct(3), &mut allocator).unwrap();
    let before = allocator.stats();
    let calls = trace.borrow().requests.len();
    let mut inspected = 0;
    let error = arena
        .allocate_from(
            &mut input,
            Some(span()),
            &mut allocator,
            |_, _, _, budget| {
                inspected += 1;
                budget.charge(1)
            },
        )
        .unwrap_err();
    assert_eq!(
        error.kind,
        g::Kind::Arena(l::ArenaError::Allocation(buffer::AllocError::Limit))
    );
    assert_eq!(inspected, 1);
    let error = arena
        .allocate_from(&mut input, Some(span()), &mut allocator, no_containers)
        .unwrap_err();
    assert_eq!(error.kind, g::Kind::Unsupported(ct(3)));
    assert_eq!(allocator.stats(), before);
    assert_eq!(trace.borrow().requests.len(), calls);
    input.release_storage(&mut allocator).unwrap();
    arena.begin_close(g::Exit::Normal, &mut allocator).unwrap();
    arena.finish_close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn copy_arena_operations_preserve_the_operand_and_reject_owning_copy_before_callbacks() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    let mut arena = prepare(&layouts, Default::default());
    arena.open(&mut allocator).unwrap();
    let mut value = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    value
        .write_immediate(value.root(), l::Immediate::I64(7), &mut allocator)
        .unwrap();
    let reference = arena
        .allocate_copy(&mut value, Some(span()), &mut allocator, no_containers)
        .unwrap();
    assert_eq!(
        value.read_immediate(value.root(), &allocator).unwrap(),
        l::Immediate::I64(7)
    );
    value
        .write_immediate(value.root(), l::Immediate::I64(11), &mut allocator)
        .unwrap();
    arena
        .replace_copy(reference, &mut value, span(), &mut allocator, no_containers)
        .unwrap();
    assert_eq!(
        value.read_immediate(value.root(), &allocator).unwrap(),
        l::Immediate::I64(11)
    );
    let mut read = arena.read(reference, o::Type::I64, &mut allocator).unwrap();
    let view = arena.payload(&read, &allocator).unwrap();
    assert_eq!(
        view.read_immediate(view.root(), &allocator).unwrap(),
        l::Immediate::I64(11)
    );
    view.copy_into(view.root(), &value, value.root(), &mut allocator)
        .unwrap();
    arena.end_read(&mut read, &mut allocator).unwrap();
    value.release_storage(&mut allocator).unwrap();
    let mut owning = make_node(&layouts, &mut allocator, 1, None);
    let before = trace.borrow().requests.len();
    assert_eq!(
        arena
            .allocate_copy(&mut owning, Some(span()), &mut allocator, no_containers)
            .unwrap_err()
            .kind,
        g::Kind::Layout(l::Error::InvalidType(node_type()))
    );
    assert_eq!(trace.borrow().requests.len(), before);
    clean_value(&mut owning, &mut allocator, &types);
    assert!(
        retire(
            &mut arena,
            &mut allocator,
            g::Exit::Normal,
            |_, _, _, _| panic!("Copy node has no descendants")
        )
        .primary()
        .is_none()
    );
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn arena_pool_estimation_matches_real_preparation_at_exact_limits() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let limits = g::Limits {
        nodes: 4,
        value_cells: 8,
        readers: 2,
        ..Default::default()
    };
    let usage =
        g::Arena::preparation_usage(&layouts, limits, Default::default(), layouts.usage()).unwrap();
    let exact = m::Limits {
        work: usage.work,
        allocations: usage.allocation_bytes,
        elements: usage.elements,
        ..Default::default()
    };
    assert_eq!(
        g::Arena::prepare(&layouts, limits, exact, layouts.usage())
            .unwrap()
            .usage(),
        usage
    );
    for below in [
        m::Limits {
            allocations: exact.allocations - 1,
            ..exact
        },
        m::Limits {
            elements: exact.elements - 1,
            ..exact
        },
    ] {
        assert!(g::Arena::preparation_usage(&layouts, limits, below, layouts.usage()).is_err());
        assert!(g::Arena::prepare(&layouts, limits, below, layouts.usage()).is_err());
    }
}
