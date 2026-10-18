use super::*;

#[test]
fn nested_container_admission_rejects_foreign_brands_shared_edges_and_work_refusal_before_publication()
 {
    let span = m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1,
    };
    let ty = |n| o::Type::Constructed(o::ConstructedId(n));
    let types = o::TypeTable {
        sources: vec![m::Source {
            path: "nested_boundary.lang".into(),
            text: "x".into(),
        }],
        constructed: [
            o::Constructor::Arena(o::Type::I64),
            o::Constructor::Vec(ty(0)),
            o::Constructor::Shared(o::Type::I64),
            o::Constructor::Vec(ty(2)),
        ]
        .into_iter()
        .map(|constructor| o::Constructed { constructor, span })
        .collect(),
        ..Default::default()
    }
    .verify(Default::default())
    .unwrap();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    let mut allocator = a::Allocator::new(
        a::System,
        a::Limits {
            live_allocations: 64,
            ..Default::default()
        },
    )
    .unwrap();
    let mut first = g::Arena::prepare(
        &layouts,
        g::Limits {
            nodes: 2,
            ..Default::default()
        },
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    let mut second = g::Arena::prepare(
        &layouts,
        g::Limits {
            nodes: 2,
            ..Default::default()
        },
        Default::default(),
        first.usage(),
    )
    .unwrap();
    let mut limited = g::Arena::prepare(
        &layouts,
        g::Limits {
            nodes: 1,
            publication_work: 1,
            ..Default::default()
        },
        Default::default(),
        second.usage(),
    )
    .unwrap();
    let mut heap = sh::Heap::prepare(
        &layouts,
        sh::Limits {
            nodes: 1,
            slots: 1,
            tickets: 4,
            ..Default::default()
        },
        Default::default(),
        limited.usage(),
    )
    .unwrap();
    let mut inspection =
        Some(ins::Inspector::prepare(&layouts, 8, Default::default(), heap.usage()).unwrap());
    let mut drain = r::Drain::prepare(
        &layouts,
        &allocator,
        Default::default(),
        inspection.as_ref().unwrap().usage(),
    )
    .unwrap();
    first.open(&mut allocator).unwrap();
    second.open(&mut allocator).unwrap();
    limited.open(&mut allocator).unwrap();
    heap.open(&mut allocator).unwrap();
    let mut scalar = l::Reservation::allocate(&layouts, o::Type::I64, &mut allocator).unwrap();
    scalar
        .write_immediate(scalar.root(), l::Immediate::I64(7), &mut allocator)
        .unwrap();
    let local = first
        .allocate_copy(&mut scalar, Some(span), &mut allocator, |_, _, _, _| {
            panic!("scalar")
        })
        .unwrap();
    let foreign = second
        .allocate_copy(&mut scalar, Some(span), &mut allocator, |_, _, _, _| {
            panic!("scalar")
        })
        .unwrap();
    let mut value = l::Reservation::allocate(&layouts, ty(0), &mut allocator).unwrap();
    value
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&foreign.token());
    let mut vector = v::Vector::new(&layouts, ty(0), &mut allocator).unwrap();
    vector
        .push_copy(&mut value, Some(span), &mut allocator)
        .unwrap();
    value.release_storage(&mut allocator).unwrap();
    let mut incoming = l::Reservation::allocate(&layouts, ty(1), &mut allocator).unwrap();
    let token = vector.take_token(&allocator).unwrap();
    incoming
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&token);
    let before = allocator.stats();
    let error = first
        .allocate_from(
            &mut incoming,
            Some(span),
            &mut allocator,
            |token, arena, allocator, budget| {
                inspect(&layouts, &mut inspection, token, arena, allocator, budget)
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, g::Kind::Arena(l::ArenaError::CrossRegion));
    assert_eq!(incoming.data(&allocator).unwrap(), token);
    assert_eq!(allocator.stats(), before);
    first.resolve(local, o::Type::I64, &allocator).unwrap();
    let error = limited
        .allocate_from(
            &mut incoming,
            Some(span),
            &mut allocator,
            |token, arena, allocator, budget| {
                inspect(&layouts, &mut inspection, token, arena, allocator, budget)
            },
        )
        .unwrap_err();
    assert_eq!(
        error.kind,
        g::Kind::Arena(l::ArenaError::Allocation(buffer::AllocError::Limit))
    );
    assert_eq!(incoming.data(&allocator).unwrap(), token);
    assert_eq!(allocator.stats(), before);
    let strong = heap
        .allocate_copy(&mut scalar, Some(span), &mut allocator)
        .unwrap();
    let mut value = l::Reservation::allocate(&layouts, ty(2), &mut allocator).unwrap();
    value
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&strong.into_token());
    let mut vector = v::Vector::new(&layouts, ty(2), &mut allocator).unwrap();
    vector
        .push_from(&mut value, Some(span), &mut allocator)
        .unwrap();
    value.release_storage(&mut allocator).unwrap();
    let mut shared_input = l::Reservation::allocate(&layouts, ty(3), &mut allocator).unwrap();
    let token = vector.take_token(&allocator).unwrap();
    shared_input
        .data_mut(&mut allocator)
        .unwrap()
        .copy_from_slice(&token);
    let before = allocator.stats();
    let error = first
        .allocate_from(
            &mut shared_input,
            Some(span),
            &mut allocator,
            |token, arena, allocator, budget| {
                inspect(&layouts, &mut inspection, token, arena, allocator, budget)
            },
        )
        .unwrap_err();
    assert_eq!(error.kind, g::Kind::Arena(l::ArenaError::CrossRegion));
    assert_eq!(shared_input.data(&allocator).unwrap(), token);
    assert_eq!(allocator.stats(), before);
    let mut instance = c::Instance::default();
    let mut frame = instance
        .enter_with_context(
            c::Limits {
                slots: 1,
                ..Default::default()
            },
            |_: &mut (), _, ctx: &mut c::Context<'_>| {
                for input in [&mut incoming, &mut shared_input] {
                    input
                        .cleanup(&mut allocator, ctx, |token, allocator, _, ctx| {
                            drain.token(token, Some(&mut heap), allocator, ctx, |_, _, _, _| {
                                panic!("Copy leaf")
                            })
                        })
                        .unwrap();
                }
                for arena in [&mut first, &mut second, &mut limited] {
                    arena
                        .cleanup(g::Exit::Normal, &mut allocator, ctx, |_, _, _, _| {
                            panic!("scalar")
                        })
                        .unwrap();
                }
                Ok(())
            },
        )
        .unwrap();
    frame.declare(c::Slot(0), c::Kind::Owner).unwrap();
    frame.initialize(c::Slot(0), (), Some(span)).unwrap();
    assert!(
        frame
            .finish(c::Exit::Return(None))
            .report
            .primary()
            .is_none()
    );
    scalar.release_storage(&mut allocator).unwrap();
    heap.close(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
}
