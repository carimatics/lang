use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{
    allocation as a, buffer, cleanup as c, inspection as i, layout as l, map, shared::managed as g,
    vector as v,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

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
            path: "inspection.lang".into(),
            text: "x".repeat(256),
        }],
        constructed: [
            o::Constructor::Builtin(o::Builtin::Bytes),
            o::Constructor::Builtin(o::Builtin::Text),
            o::Constructor::Vec(nominal(0)),
            o::Constructor::Map(ty(1), ty(2)),
            o::Constructor::Shared(o::Type::I64),
            o::Constructor::Weak(o::Type::I64),
            o::Constructor::Vec(ty(4)),
            o::Constructor::Vec(o::Type::I64),
            o::Constructor::Vec(nominal(1)),
            o::Constructor::Option(ty(0)),
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
                kind: o::DefinitionKind::Record(vec![field("bytes", ty(0)), field("text", ty(1))]),
            },
            o::Definition {
                name: "Node".into(),
                public: false,
                span: span(0),
                kind: o::DefinitionKind::Record(vec![
                    field("leaf", ty(9)),
                    field("children", ty(8)),
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
}
struct Provider(Rc<RefCell<Trace>>);
impl a::Provider for Provider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<Self::Region> {
        self.0.borrow_mut().requests += 1;
        a::Storage::try_new(request).ok()
    }
    fn deallocate(&mut self, storage: Self::Region, _: a::Request) {
        self.0.borrow_mut().releases += 1;
        drop(storage);
    }
}
fn allocator() -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    (
        a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap(),
        trace,
    )
}
fn token(ty: o::Type, bytes: [u8; 32], start: u32) -> l::Token {
    l::Token {
        leaf: l::Leaf {
            ty,
            offset: 0,
            span: Some(span(start)),
        },
        bytes,
    }
}
fn put(
    value: &l::Reservation<'_, '_>,
    at: l::Location,
    bytes: [u8; 32],
    allocator: &mut a::Allocator<Provider>,
) {
    let start = at.offset() as usize;
    value.data_mut(allocator).unwrap()[start..start + 32].copy_from_slice(&bytes);
}
fn bytes(allocator: &mut a::Allocator<Provider>, value: &[u8]) -> [u8; 32] {
    buffer::Bytes::from_slice(allocator, value)
        .unwrap()
        .take_token(allocator)
        .unwrap()
}
fn text(allocator: &mut a::Allocator<Provider>, value: &str) -> [u8; 32] {
    let mut bytes = buffer::Bytes::from_slice(allocator, value.as_bytes()).unwrap();
    bytes
        .take_text(allocator)
        .unwrap()
        .take_token(allocator)
        .unwrap()
}
fn number<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    allocator: &mut a::Allocator<Provider>,
    n: i64,
) -> l::Reservation<'l, 't> {
    let value = l::Reservation::allocate(layouts, o::Type::I64, allocator).unwrap();
    value
        .write_immediate(value.root(), l::Immediate::I64(n), allocator)
        .unwrap();
    value
}
fn pair<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    allocator: &mut a::Allocator<Provider>,
    n: u8,
) -> l::Reservation<'l, 't> {
    let value = l::Reservation::allocate(layouts, nominal(0), allocator).unwrap();
    let left = value.child(value.root(), 0, allocator).unwrap();
    let right = value.child(value.root(), 1, allocator).unwrap();
    put(&value, left, bytes(allocator, &[n]), allocator);
    put(&value, right, text(allocator, "é"), allocator);
    value
        .initialize_order(
            value.root(),
            [(1, span(12)), (0, span(u32::from(n)))].into_iter(),
            allocator,
        )
        .unwrap();
    value
}
fn context(mut clean: impl FnMut(&mut c::Context<'_>) -> Result<(), u32>) {
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
    frame.initialize(c::Slot(0), (), Some(span(0))).unwrap();
    let report = frame.finish(c::Exit::Return(None)).report;
    assert!(report.primary().is_none(), "{report:?}");
}
fn clean_buffer(
    token: &mut l::Token,
    allocator: &mut a::Allocator<Provider>,
    _: c::Reason,
    _: &mut c::Context<'_>,
) -> Result<(), u32> {
    match token.leaf.ty {
        t if t == ty(0) => buffer::Bytes::from_token(&mut token.bytes, allocator)
            .unwrap()
            .release(allocator)
            .unwrap(),
        t if t == ty(1) => buffer::Text::from_token(&mut token.bytes, allocator)
            .unwrap()
            .release(allocator)
            .unwrap(),
        _ => panic!("unexpected leaf"),
    }
    Ok(())
}
fn empty(allocator: &a::Allocator<Provider>) {
    assert_eq!(allocator.stats().live_bytes, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
}
fn inspector<'l, 't>(layouts: &'l l::Layouts<'t>, cells: usize) -> i::Inspector<'l, 't> {
    i::Inspector::prepare(layouts, cells, Default::default(), layouts.usage()).unwrap()
}
fn fixture<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    allocator: &mut a::Allocator<Provider>,
) -> l::Token {
    let mut map = map::Map::new(layouts, ty(1), ty(2), allocator).unwrap();
    for (index, key) in ["", "a\0é"].into_iter().enumerate() {
        let mut vector = v::Vector::new(layouts, nominal(0), allocator).unwrap();
        for n in 1..=3 {
            let mut pair = pair(layouts, allocator, n + index as u8 * 3);
            vector
                .push_from(&mut pair, Some(span(30 + u32::from(n))), allocator)
                .unwrap();
            pair.release_storage(allocator).unwrap();
        }
        let mut key_value = l::Reservation::allocate(layouts, ty(1), allocator).unwrap();
        put(
            &key_value,
            key_value.root(),
            text(allocator, key),
            allocator,
        );
        let mut value = l::Reservation::allocate(layouts, ty(2), allocator).unwrap();
        put(
            &value,
            value.root(),
            vector.take_token(allocator).unwrap(),
            allocator,
        );
        map.publish_if_absent(
            &mut key_value,
            &mut value,
            map::Spans {
                key: Some(span(40 + index as u32)),
                value: Some(span(50)),
            },
            allocator,
        )
        .unwrap();
        key_value.release_storage(allocator).unwrap();
        value.release_storage(allocator).unwrap();
    }
    token(ty(3), map.take_token(allocator).unwrap(), 60)
}
// This fixture has a fixed two-container depth. Production cross-service
// retirement remains C09; no recursive test adapter stands in for that work.
fn discard_fixture(
    layouts: &l::Layouts<'_>,
    mut root: l::Token,
    allocator: &mut a::Allocator<Provider>,
) {
    let mut map = map::Map::from_token(layouts, ty(1), ty(2), &mut root.bytes, allocator).unwrap();
    context(|ctx| {
        map.cleanup(allocator, ctx, |token, allocator, reason, ctx| {
            if token.leaf.ty == ty(2) {
                let mut vector =
                    v::Vector::from_token(layouts, nominal(0), &mut token.bytes, allocator)
                        .unwrap();
                vector.cleanup(allocator, ctx, clean_buffer).unwrap();
                Ok(())
            } else {
                clean_buffer(token, allocator, reason, ctx)
            }
        })
        .map_err(|_| 99)
    });
}

#[test]
fn nested_map_vector_inline_traversal_matches_independent_leaf_contents_and_original_spans() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator();
    let root = fixture(&layouts, &mut allocator);
    let original = root.bytes;
    let before = allocator.stats();
    let requests = trace.borrow().requests;
    let mut inspector = inspector(&layouts, 8);
    let usage = inspector.usage();
    let mut read = inspector.token(&root, &allocator).unwrap();
    let mut found = BTreeMap::new();
    let mut work = i::Work::new(1000);
    while let Some(observation) = read.next(&mut work).unwrap() {
        let mut bytes = observation.bytes;
        let contents = if observation.ty == ty(0) {
            let value = buffer::Bytes::from_token(&mut bytes, &allocator).unwrap();
            vec![value.get(&allocator, 0).unwrap() as u8]
        } else {
            buffer::Text::from_token(&mut bytes, &allocator)
                .unwrap()
                .as_str(&allocator)
                .unwrap()
                .as_bytes()
                .to_vec()
        };
        *found
            .entry((contents, observation.span.unwrap().start))
            .or_insert(0) += 1;
    }
    let mut expected = BTreeMap::from([
        ((vec![], 40), 1),
        (("a\0é".as_bytes().to_vec(), 41), 1),
        (("é".as_bytes().to_vec(), 12), 6),
    ]);
    for n in 1..=6 {
        expected.insert((vec![n], u32::from(n)), 1);
    }
    assert_eq!(found, expected);
    assert_eq!(inspector.usage(), usage);
    assert_eq!(root.bytes, original);
    assert_eq!(allocator.stats(), before);
    assert_eq!(trace.borrow().requests, requests);
    discard_fixture(&layouts, root, &mut allocator);
    empty(&allocator);
}

#[test]
fn every_work_prefix_refuses_without_effects_and_the_same_inspector_can_retry() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator();
    let root = fixture(&layouts, &mut allocator);
    let mut inspector = inspector(&layouts, 8);
    let mut work = i::Work::new(1000);
    let mut read = inspector.token(&root, &allocator).unwrap();
    while read.next(&mut work).unwrap().is_some() {}
    let needed = 1000 - work.remaining();
    let before = allocator.stats();
    let requests = trace.borrow().requests;
    for fuel in 0..=needed {
        let mut work = i::Work::new(fuel);
        let mut read = inspector.token(&root, &allocator).unwrap();
        let result = loop {
            match read.next(&mut work) {
                Ok(Some(_)) => {}
                done => break done,
            }
        };
        if fuel == needed {
            assert_eq!(result, Ok(None));
        } else {
            assert_eq!(result, Err(i::Error::Work));
            assert_eq!(read.next(&mut i::Work::new(1000)), Ok(None));
        }
        assert_eq!(allocator.stats(), before);
        assert_eq!(trace.borrow().requests, requests);
    }
    discard_fixture(&layouts, root, &mut allocator);
    empty(&allocator);
}

#[test]
fn wide_scalar_vector_uses_constant_cursor_space_and_no_leaf_or_allocation() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator();
    let mut vector = v::Vector::new(&layouts, o::Type::I64, &mut allocator).unwrap();
    let mut value = number(&layouts, &mut allocator, 7);
    for _ in 0..10_000 {
        vector
            .push_copy(&mut value, Some(span(5)), &mut allocator)
            .unwrap();
    }
    value.release_storage(&mut allocator).unwrap();
    let root = token(ty(7), vector.take_token(&allocator).unwrap(), 0);
    let before = allocator.stats();
    let requests = trace.borrow().requests;
    let mut inspector = inspector(&layouts, 2);
    let mut read = inspector.token(&root, &allocator).unwrap();
    assert_eq!(read.next(&mut i::Work::new(100_000)), Ok(None));
    assert_eq!(allocator.stats(), before);
    assert_eq!(trace.borrow().requests, requests);
    let mut bytes = root.bytes;
    let mut vector = v::Vector::from_token(&layouts, o::Type::I64, &mut bytes, &allocator).unwrap();
    context(|ctx| {
        vector
            .cleanup(&mut allocator, ctx, |_, _, _, _| panic!("Copy leaf"))
            .map_err(|_| 99)
    });
    empty(&allocator);
}

#[test]
fn real_shared_and_weak_tokens_remain_opaque_observations_without_retains_or_consumption() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, _) = allocator();
    let mut heap = g::Heap::prepare(
        &layouts,
        g::Limits {
            nodes: 1,
            slots: 1,
            tickets: 4,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    heap.open(&mut allocator).unwrap();
    let mut scalar = number(&layouts, &mut allocator, 19);
    let strong = heap
        .allocate_from(&mut scalar, Some(span(1)), &mut allocator)
        .unwrap();
    scalar.release_storage(&mut allocator).unwrap();
    let weak = heap
        .downgrade(&strong, o::Type::I64, &mut allocator)
        .unwrap();
    let mut inspector = inspector(&layouts, 1);
    for root in [
        token(ty(4), strong.into_token(), 4),
        token(ty(5), weak.into_token(), 5),
    ] {
        let before = allocator.stats();
        for _ in 0..3 {
            let mut read = inspector.token(&root, &allocator).unwrap();
            let observation = read.next(&mut i::Work::new(1)).unwrap().unwrap();
            assert_eq!(observation.bytes, root.bytes);
            assert_eq!(observation.span, root.leaf.span);
            assert_eq!(read.next(&mut i::Work::new(0)), Ok(None));
        }
        assert_eq!(allocator.stats(), before);
        let mut bytes = root.bytes;
        if root.leaf.ty == ty(4) {
            let mut strong = Some(g::Strong::from_token(&mut bytes).unwrap());
            context(|ctx| {
                heap.release_strong(
                    strong.take().unwrap(),
                    o::Type::I64,
                    &mut allocator,
                    ctx,
                    |_, _, _, _| panic!("Copy leaf"),
                )
                .map_err(|_| 99)
            });
        } else {
            heap.release_weak(
                g::Weak::from_token(&mut bytes).unwrap(),
                o::Type::I64,
                &mut allocator,
            )
            .unwrap();
        }
    }
    heap.close(&mut allocator).unwrap();
    empty(&allocator);
}

#[test]
fn exact_host_admission_and_cell_limits_are_checked_before_any_guest_callback() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let expected = inspector(&layouts, 8).usage();
    for short in [false, true] {
        let result = i::Inspector::prepare(
            &layouts,
            8,
            m::Limits {
                allocations: expected.allocation_bytes - usize::from(short),
                elements: expected.elements,
                work: expected.work,
                ..Default::default()
            },
            layouts.usage(),
        );
        assert_eq!(result.is_ok(), !short);
    }
    assert!(
        i::Inspector::prepare(&layouts, usize::MAX, Default::default(), layouts.usage()).is_err()
    );
    assert!(i::Inspector::prepare(&layouts, 1, Default::default(), m::Usage::default()).is_err());
    let (mut allocator, trace) = allocator();
    let root = fixture(&layouts, &mut allocator);
    let before = allocator.stats();
    let requests = trace.borrow().requests;
    let mut zero = inspector(&layouts, 0);
    assert!(matches!(
        zero.token(&root, &allocator),
        Err(i::Error::Cells)
    ));
    let mut one = inspector(&layouts, 1);
    assert_eq!(
        one.token(&root, &allocator)
            .unwrap()
            .next(&mut i::Work::new(100)),
        Err(i::Error::Cells)
    );
    assert_eq!(allocator.stats(), before);
    assert_eq!(trace.borrow().requests, requests);
    discard_fixture(&layouts, root, &mut allocator);
    empty(&allocator);
}

#[test]
fn forged_container_tokens_spans_and_stale_generations_refuse_without_mutation() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, trace) = allocator();
    let root = fixture(&layouts, &mut allocator);
    let mut inspector = inspector(&layouts, 8);
    let before = allocator.stats();
    let requests = trace.borrow().requests;
    for byte in [0, 8, 16, 24, 31] {
        let mut bad = token(root.leaf.ty, root.bytes, 60);
        bad.bytes[byte] ^= 0x40;
        assert!(
            inspector
                .token(&bad, &allocator)
                .unwrap()
                .next(&mut i::Work::new(1000))
                .is_err()
        );
        assert_eq!(allocator.stats(), before);
    }
    let mut bad_span = token(root.leaf.ty, root.bytes, 60);
    bad_span.leaf.span.as_mut().unwrap().end = 999;
    assert!(matches!(
        inspector.token(&bad_span, &allocator),
        Err(i::Error::Layout(l::Error::InvalidSpan))
    ));
    assert_eq!(trace.borrow().requests, requests);
    let stale = token(root.leaf.ty, root.bytes, 60);
    discard_fixture(&layouts, root, &mut allocator);
    // Reuse a freed allocator slot; its old generation must remain unavailable.
    let mut other = map::Map::new(&layouts, ty(1), ty(2), &mut allocator).unwrap();
    let before = allocator.stats();
    assert!(
        inspector
            .token(&stale, &allocator)
            .unwrap()
            .next(&mut i::Work::new(1000))
            .is_err()
    );
    assert_eq!(allocator.stats(), before);
    context(|ctx| {
        other
            .cleanup(&mut allocator, ctx, clean_buffer)
            .map_err(|_| 99)
    });
    empty(&allocator);
}

#[test]
fn read_sessions_check_scalar_values_empty_case_initialization_and_layout_identity() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let foreign = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, _) = allocator();
    let mut inspector = inspector(&layouts, 4);
    let mut option = l::Reservation::allocate(&layouts, ty(9), &mut allocator).unwrap();
    // None has no child fields, but its order header must still be initialized.
    assert_eq!(
        inspector
            .read(&option, None, &allocator)
            .unwrap()
            .next(&mut i::Work::new(10)),
        Err(i::Error::Layout(l::Error::Uninitialized))
    );
    option
        .prepare_case(option.root(), 0, &mut allocator)
        .unwrap();
    option
        .initialize_order(option.root(), [].into_iter(), &mut allocator)
        .unwrap();
    assert_eq!(
        inspector
            .read(&option, None, &allocator)
            .unwrap()
            .next(&mut i::Work::new(10)),
        Ok(None)
    );
    option.data_mut(&mut allocator).unwrap()[..4].copy_from_slice(&999u32.to_le_bytes());
    assert!(
        inspector
            .read(&option, None, &allocator)
            .unwrap()
            .next(&mut i::Work::new(10))
            .is_err()
    );
    option
        .prepare_case(option.root(), 0, &mut allocator)
        .unwrap();
    option
        .initialize_order(option.root(), [].into_iter(), &mut allocator)
        .unwrap();
    option.begin_retirement(None, &mut allocator).unwrap();
    assert!(option.next_leaf(&mut allocator).unwrap().is_none());
    option.release_storage(&mut allocator).unwrap();
    let scalar = l::Reservation::allocate(&layouts, o::Type::Bool, &mut allocator).unwrap();
    scalar.data_mut(&mut allocator).unwrap()[0] = 2;
    assert!(
        inspector
            .read(&scalar, None, &allocator)
            .unwrap()
            .next(&mut i::Work::new(10))
            .is_err()
    );
    scalar.data_mut(&mut allocator).unwrap()[0] = 1;
    assert_eq!(
        inspector
            .read(&scalar, None, &allocator)
            .unwrap()
            .next(&mut i::Work::new(10)),
        Ok(None)
    );
    let mut foreign_inspector = self::inspector(&foreign, 4);
    assert!(matches!(
        foreign_inspector.read(&scalar, None, &allocator),
        Err(i::Error::Layout(l::Error::State))
    ));
    let mut scalar = scalar;
    scalar.release_storage(&mut allocator).unwrap();
    empty(&allocator);
}

#[test]
fn exclusive_vector_loan_blocks_observation_and_ending_it_permits_retry() {
    let types = model();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let (mut allocator, _) = allocator();
    let mut vector = v::Vector::new(&layouts, o::Type::I64, &mut allocator).unwrap();
    let mut value = number(&layouts, &mut allocator, 3);
    vector
        .push_copy(&mut value, Some(span(3)), &mut allocator)
        .unwrap();
    value.release_storage(&mut allocator).unwrap();
    let root = token(ty(7), vector.take_token(&allocator).unwrap(), 0);
    let mut bytes = root.bytes;
    let mut vector = v::Vector::from_token(&layouts, o::Type::I64, &mut bytes, &allocator).unwrap();
    let mut loan = vector
        .borrow(0, o::LoanKind::Exclusive, &mut allocator)
        .unwrap();
    let before = allocator.stats();
    let mut inspector = inspector(&layouts, 2);
    assert_eq!(
        inspector
            .token(&root, &allocator)
            .unwrap()
            .next(&mut i::Work::new(10)),
        Err(i::Error::Vector(v::Error::BorrowConflict))
    );
    assert_eq!(allocator.stats(), before);
    loan.end(&layouts, &mut allocator).unwrap();
    assert_eq!(
        inspector
            .token(&root, &allocator)
            .unwrap()
            .next(&mut i::Work::new(10)),
        Ok(None)
    );
    context(|ctx| {
        vector
            .cleanup(&mut allocator, ctx, |_, _, _, _| panic!("Copy leaf"))
            .map_err(|_| 99)
    });
    empty(&allocator);
}

fn node<'l, 't>(
    layouts: &'l l::Layouts<'t>,
    allocator: &mut a::Allocator<Provider>,
    child: Option<l::Reservation<'l, 't>>,
) -> l::Reservation<'l, 't> {
    let mut children = v::Vector::new(layouts, nominal(1), allocator).unwrap();
    let terminal = child.is_none();
    if let Some(mut child) = child {
        children
            .push_from(&mut child, Some(span(20)), allocator)
            .unwrap();
        child.release_storage(allocator).unwrap();
    }
    let node = l::Reservation::allocate(layouts, nominal(1), allocator).unwrap();
    let leaf = node.child(node.root(), 0, allocator).unwrap();
    node.prepare_case(leaf, u32::from(terminal), allocator)
        .unwrap();
    if terminal {
        let payload = node.child(leaf, 0, allocator).unwrap();
        put(&node, payload, bytes(allocator, &[99]), allocator);
        node.initialize_order(leaf, [(0, span(99))].into_iter(), allocator)
            .unwrap();
    } else {
        node.initialize_order(leaf, [].into_iter(), allocator)
            .unwrap();
    }
    let at = node.child(node.root(), 1, allocator).unwrap();
    put(
        &node,
        at,
        children.take_token(allocator).unwrap(),
        allocator,
    );
    node.initialize_order(
        node.root(),
        [(0, span(21)), (1, span(22))].into_iter(),
        allocator,
    )
    .unwrap();
    node
}
#[test]
fn recursive_collection_shapes_walk_on_a_small_host_stack_with_a_checked_depth_limit() {
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(|| {
            let types = model();
            let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
            let (mut allocator, trace) = allocator();
            let mut root = node(&layouts, &mut allocator, None);
            for _ in 0..512 {
                root = node(&layouts, &mut allocator, Some(root));
            }
            let before = allocator.stats();
            let requests = trace.borrow().requests;
            let mut shallow = inspector(&layouts, 16);
            assert_eq!(
                shallow
                    .read(&root, None, &allocator)
                    .unwrap()
                    .next(&mut i::Work::new(100_000)),
                Err(i::Error::Cells)
            );
            let mut inspector = inspector(&layouts, 2048);
            let mut read = inspector.read(&root, None, &allocator).unwrap();
            let mut work = i::Work::new(100_000);
            let leaf = read.next(&mut work).unwrap().unwrap();
            assert_eq!(leaf.ty, ty(0));
            assert_eq!(leaf.span, Some(span(99)));
            assert_eq!(read.next(&mut work), Ok(None));
            assert_eq!(allocator.stats(), before);
            assert_eq!(trace.borrow().requests, requests);
            // Unpack one level at a time to retire the test fixture. This is a
            // dedicated test teardown, not the production C09 service drain.
            loop {
                let at = root.child(root.root(), 1, &allocator).unwrap();
                let mut token = root.read_token(at, &allocator).unwrap();
                let mut children =
                    v::Vector::from_token(&layouts, nominal(1), &mut token.bytes, &allocator)
                        .unwrap();
                let child = if children.is_empty(&allocator).unwrap() {
                    None
                } else {
                    let out =
                        l::Reservation::allocate(&layouts, nominal(1), &mut allocator).unwrap();
                    let target = out.root();
                    assert!(children.pop_into(&out, target, &mut allocator).unwrap());
                    Some(out)
                };
                context(|ctx| {
                    root.cleanup(&mut allocator, ctx, |token, allocator, reason, ctx| {
                        if token.leaf.ty == ty(8) {
                            children
                                .cleanup(allocator, ctx, |_, _, _, _| panic!("emptied children"))
                                .unwrap();
                            Ok(())
                        } else {
                            clean_buffer(token, allocator, reason, ctx)
                        }
                    })
                    .map_err(|_| 99)
                });
                match child {
                    Some(next) => root = next,
                    None => break,
                }
            }
            empty(&allocator);
        })
        .unwrap()
        .join()
        .unwrap();
}
