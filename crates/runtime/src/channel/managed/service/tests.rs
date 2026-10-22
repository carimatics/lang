use super::*;
use lang_mir::unit::ownership::{bodies as b, tasks};
fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1,
    }
}
fn ct(index: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(index))
}
fn proof() -> tasks::VerifiedTasks {
    let mut table = b::BodyTable::default();
    table.places.declarations.types.sources.push(m::Source {
        path: "services.lang".into(),
        text: "x".into(),
    });
    let state = o::Type::Nominal(m::DefinitionId(0));
    table
        .places
        .declarations
        .types
        .definitions
        .push(o::Definition {
            name: "RecursiveState".into(),
            public: false,
            span: span(),
            kind: o::DefinitionKind::Record(vec![o::Field {
                name: "child".into(),
                span: span(),
                ty: o::TypeUse {
                    ty: ct(12),
                    span: span(),
                },
            }]),
        });
    for constructor in [
        o::Constructor::Builtin(o::Builtin::Bytes),
        o::Constructor::ServiceSetup(ct(0), ct(0)),
        o::Constructor::ServiceHandle(ct(0)),
        o::Constructor::ServiceOwner(ct(0), ct(0)),
        o::Constructor::ServiceOwner(o::Type::I64, ct(0)),
        o::Constructor::Builtin(o::Builtin::ServiceError),
        o::Constructor::ChannelSetup(ct(0)),
        o::Constructor::Sender(ct(0)),
        o::Constructor::Receiver(ct(0)),
        o::Constructor::ServiceSetup(ct(8), ct(0)),
        o::Constructor::ServiceOwner(ct(8), ct(0)),
        o::Constructor::ServiceOwner(state, ct(0)),
        o::Constructor::Option(ct(11)),
        o::Constructor::ServiceSetup(state, ct(0)),
    ] {
        table
            .places
            .declarations
            .types
            .constructed
            .push(o::Constructed {
                constructor,
                span: span(),
            });
    }
    tasks::verify(table, Default::default()).unwrap()
}
fn value<'l, 'ty>(
    layouts: &'l l::Layouts<'ty>,
    byte: u8,
    allocator: &mut a::Allocator<a::System>,
) -> l::Reservation<'l, 'ty> {
    let value = l::Reservation::allocate(layouts, ct(0), allocator).unwrap();
    let mut bytes = crate::buffer::Bytes::from_slice(allocator, &[byte]).unwrap();
    let token = bytes.take_token(allocator).unwrap();
    value.data_mut(allocator).unwrap().copy_from_slice(&token);
    value
}

#[test]
fn private_roles_read_generations_and_queue_before_state_retirement_are_checked() {
    let proof = proof();
    let layouts = l::Layouts::compile_service_tasks(&proof, Default::default()).unwrap();
    let types = layouts.types();
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut scheduler = t::Scheduler::new(Default::default()).unwrap();
    let root = scheduler.start(None).unwrap();
    assert!(matches!(
        scheduler.dispatch().unwrap(),
        Some(t::Dispatch::Resume(_))
    ));
    let mut channels = Channels::prepare(
        &layouts,
        &allocator,
        &scheduler,
        &root,
        Limits {
            channels: 1,
            slots: 2,
            handles: 2,
            waiters: 2,
        },
        4,
        e::ServiceWork::new(None),
        &mut m::Budget::new(Default::default()),
    )
    .unwrap();
    let mut inspector =
        crate::inspection::Inspector::prepare(&layouts, 32, Default::default(), layouts.usage())
            .unwrap();
    let mut initial = value(&layouts, 9, &mut allocator);
    let setup = channels
        .create_service(
            ct(1),
            2,
            &mut initial,
            Some(span()),
            &mut inspector,
            None,
            &mut allocator,
        )
        .unwrap();
    initial.recycle(&mut allocator).unwrap();
    initial.release_storage(&mut allocator).unwrap();
    let mut handle = channels
        .sender(&setup, ct(2), Some(span()), &mut allocator)
        .unwrap();
    let mut owner = channels.receiver(&setup, ct(3), &mut allocator).unwrap();
    assert!(matches!(
        channels.service_cancel(&owner, &mut scheduler, &allocator),
        Err(Error::MessageType)
    ));
    assert!(matches!(
        channels.service_read(&owner, &allocator),
        Err(Error::Service(l::ServiceError::NoActiveHandler))
    ));
    for (token, ty) in [(&owner, ct(4)), (&owner, ct(8)), (&handle, ct(7))] {
        let forged = l::Token {
            leaf: l::Leaf {
                ty,
                offset: 0,
                span: Some(span()),
            },
            bytes: token.bytes,
        };
        assert!(channels.index(&forged, &allocator).is_err());
    }
    // Exercise the dynamic lease boundary independently of the source proof.
    channels.service_received(0, true).unwrap();
    let read = channels.service_read(&owner, &allocator).unwrap();
    let foreign = a::Allocator::new(a::System, Default::default()).unwrap();
    assert!(channels.service_view(read, &foreign).is_err());
    let mut candidate = value(&layouts, 10, &mut allocator);
    assert!(matches!(
        channels.service_replace(
            &owner,
            &mut candidate,
            span(),
            &mut inspector,
            None,
            &mut allocator
        ),
        Err(Error::State)
    ));
    channels.end_service_read(read, &allocator).unwrap();
    assert_eq!(
        channels
            .service_replace(
                &owner,
                &mut candidate,
                span(),
                &mut inspector,
                None,
                &mut allocator
            )
            .unwrap(),
        Some(span())
    );
    assert!(channels.service_view(read, &allocator).is_err());
    assert!(
        channels
            .service_finish(&owner, &mut scheduler, &allocator)
            .unwrap()
            .is_none()
    );
    let mut session = c::Session::prepare(
        std::num::NonZeroUsize::new(8).unwrap(),
        &mut m::Budget::new(Default::default()),
    )
    .unwrap();
    session
        .retire(
            c::Slot(0),
            (),
            Some(span()),
            c::Reason::Replacement,
            &mut c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
                candidate
                    .retire(&mut allocator, context, |token, allocator, _, _| {
                        crate::buffer::cleanup_token(types, token, allocator).map_err(|_| 1u32)
                    })
                    .map_err(|_| 2u32)
            }),
        )
        .unwrap();
    candidate.release_storage(&mut allocator).unwrap();
    for byte in [20, 22] {
        let mut message = channels
            .stage(&handle, Some(span()), &mut allocator)
            .unwrap();
        let mut bytes = crate::buffer::Bytes::from_slice(&mut allocator, &[byte]).unwrap();
        let token = bytes.take_token(&allocator).unwrap();
        message
            .storage
            .as_mut()
            .unwrap()
            .data_mut(&mut allocator)
            .unwrap()
            .copy_from_slice(&token);
        assert!(matches!(
            channels
                .send(
                    &mut scheduler,
                    &root,
                    &handle,
                    message,
                    Some(span()),
                    &mut inspector,
                    None,
                    &allocator
                )
                .unwrap(),
            Send::Sent { .. }
        ));
    }
    let mut order = Vec::new();
    for token in [&mut owner, &mut handle] {
        session
            .retire(
                c::Slot(0),
                token,
                Some(span()),
                c::Reason::ScopeExit,
                &mut c::Contextual::new(
                    |token: &mut &mut l::Token, _, context: &mut c::Context<'_>| {
                        channels
                            .retire(
                                token,
                                &mut scheduler,
                                &mut allocator,
                                context,
                                |token, allocator, _, _| {
                                    let mut bytes = token.bytes;
                                    order.push(
                                        crate::buffer::Bytes::from_token(&mut bytes, allocator)
                                            .unwrap()
                                            .as_slice(allocator)
                                            .unwrap()[0],
                                    );
                                    crate::buffer::cleanup_token(types, token, allocator)
                                        .map_err(|_| 1u32)
                                },
                            )
                            .map_err(|_| 2u32)
                    },
                ),
            )
            .unwrap();
    }
    assert_eq!(order, [20, 22, 10]);
    assert_eq!(session.report().cleanup_failures(), 0);
    assert_eq!(channels.live_cells(), 0);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn all_finite_service_errors_keep_canonical_tags_and_allocation_payloads() {
    let proof = proof();
    let layouts = l::Layouts::compile_service_tasks(&proof, Default::default()).unwrap();
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut value = l::Reservation::allocate(&layouts, ct(5), &mut allocator).unwrap();
    for (case, error) in [
        (0, l::ServiceError::InvalidCapacity),
        (
            1,
            l::ServiceError::Allocation(crate::buffer::AllocError::Limit),
        ),
        (
            1,
            l::ServiceError::Allocation(crate::buffer::AllocError::OutOfMemory),
        ),
        (
            1,
            l::ServiceError::Allocation(crate::buffer::AllocError::Overflow),
        ),
        (2, l::ServiceError::Unavailable),
        (3, l::ServiceError::NoActiveHandler),
        (4, l::ServiceError::InvalidPublication),
    ] {
        let immediate = l::Immediate::Service(error);
        assert_eq!(
            l::Immediate::from_case(o::Builtin::ServiceError, case, immediate.payload()).unwrap(),
            immediate
        );
        value
            .write_immediate(value.root(), immediate, &mut allocator)
            .unwrap();
        assert_eq!(
            value.read_immediate(value.root(), &allocator).unwrap(),
            immediate
        );
        assert_eq!(value.case(value.root(), &allocator).unwrap(), case);
    }
    value.release_storage(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn prospective_replacement_checks_only_new_state_and_refusal_preserves_both_owners() {
    fn run(queued: usize, remaining: Option<u64>) -> u64 {
        let proof = proof();
        let layouts = l::Layouts::compile_service_tasks(&proof, Default::default()).unwrap();
        let types = layouts.types();
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut scheduler = t::Scheduler::new(Default::default()).unwrap();
        let root = scheduler.start(None).unwrap();
        assert!(matches!(
            scheduler.dispatch().unwrap(),
            Some(t::Dispatch::Resume(_))
        ));
        let work = e::ServiceWork::new(Some(10_000));
        let mut channels = Channels::prepare(
            &layouts,
            &allocator,
            &scheduler,
            &root,
            Limits {
                channels: 3,
                slots: queued + 2,
                handles: 2,
                waiters: 1,
            },
            queued + 3,
            work.clone(),
            &mut m::Budget::new(Default::default()),
        )
        .unwrap();
        let mut inspector = crate::inspection::Inspector::prepare(
            &layouts,
            32,
            Default::default(),
            layouts.usage(),
        )
        .unwrap();
        let old_setup = channels
            .create(ct(6), queued as i64, Some(span()), &mut allocator)
            .unwrap();
        let mut old_sender = channels
            .sender(&old_setup, ct(7), Some(span()), &mut allocator)
            .unwrap();
        let old_receiver = channels
            .receiver(&old_setup, ct(8), &mut allocator)
            .unwrap();
        for _ in 0..queued {
            let mut message = channels
                .stage(&old_sender, Some(span()), &mut allocator)
                .unwrap();
            let mut bytes = crate::buffer::Bytes::from_slice(&mut allocator, &[1]).unwrap();
            let token = bytes.take_token(&allocator).unwrap();
            message
                .storage
                .as_mut()
                .unwrap()
                .data_mut(&mut allocator)
                .unwrap()
                .copy_from_slice(&token);
            assert!(matches!(
                channels
                    .send(
                        &mut scheduler,
                        &root,
                        &old_sender,
                        message,
                        Some(span()),
                        &mut inspector,
                        None,
                        &allocator,
                    )
                    .unwrap(),
                Send::Sent { .. }
            ));
        }
        let new_setup = channels
            .create(ct(6), 1, Some(span()), &mut allocator)
            .unwrap();
        let new_receiver = channels
            .receiver(&new_setup, ct(8), &mut allocator)
            .unwrap();
        let new_bytes = new_receiver.bytes;
        let mut input = l::Reservation::allocate(&layouts, ct(8), &mut allocator).unwrap();
        input
            .data_mut(&mut allocator)
            .unwrap()
            .copy_from_slice(&old_receiver.bytes);
        let setup = channels
            .create_service(
                ct(9),
                1,
                &mut input,
                Some(span()),
                &mut inspector,
                None,
                &mut allocator,
            )
            .unwrap();
        input.recycle(&mut allocator).unwrap();
        input.release_storage(&mut allocator).unwrap();
        let mut handle = channels
            .sender(&setup, ct(2), Some(span()), &mut allocator)
            .unwrap();
        let mut owner = channels.receiver(&setup, ct(10), &mut allocator).unwrap();
        // The adapter's active-handler proof is tested separately by source
        // execution. Here, isolate publication and its dynamic refusal boundary.
        channels.service_received(2, true).unwrap();
        let mut candidate = l::Reservation::allocate(&layouts, ct(8), &mut allocator).unwrap();
        candidate
            .data_mut(&mut allocator)
            .unwrap()
            .copy_from_slice(&new_bytes);
        let old_state = channels.backing[2]
            .service
            .as_ref()
            .unwrap()
            .storage
            .as_ref()
            .unwrap()
            .data(&allocator)
            .unwrap()
            .to_vec();
        let generation = channels.backing[2].service.as_ref().unwrap().generation;
        if let Some(remaining) = remaining {
            work.charge(10_000 - work.used() - remaining).unwrap();
        }
        let before = work.used();
        let result = channels.service_replace(
            &owner,
            &mut candidate,
            span(),
            &mut inspector,
            None,
            &mut allocator,
        );
        let used = work.used() - before;
        let stored = channels.backing[2].service.as_ref().unwrap();
        match result {
            Ok(_) => {
                assert_eq!(stored.generation, generation + 1);
                assert_eq!(
                    stored.storage.as_ref().unwrap().data(&allocator).unwrap(),
                    new_bytes
                );
                assert_eq!(candidate.data(&allocator).unwrap(), old_state);
                if let Some(remaining) = remaining {
                    assert_eq!(used, remaining);
                }
            }
            Err(error) => {
                assert!(
                    matches!(
                        error,
                        Error::Protocol(crate::channel::Error::Work(
                            e::Failure::ServiceWorkLimit { .. }
                        )) | Error::Layout(l::Error::ServiceWork(
                            e::Failure::ServiceWorkLimit { .. }
                        ))
                    ),
                    "{error:?}"
                );
                assert!(remaining.is_some());
                assert_eq!(stored.generation, generation);
                assert_eq!(
                    stored.storage.as_ref().unwrap().data(&allocator).unwrap(),
                    old_state
                );
                assert_eq!(candidate.data(&allocator).unwrap(), new_bytes);
            }
        }
        let cleanup_work = work.used();
        let mut session = c::Session::prepare(
            std::num::NonZeroUsize::new(8).unwrap(),
            &mut m::Budget::new(Default::default()),
        )
        .unwrap();
        let mut drain = crate::retirement::Drain::prepare(
            &layouts,
            &allocator,
            Default::default(),
            layouts.usage(),
        )
        .unwrap();
        let mut retired_buffers = 0;
        session
            .retire(
                c::Slot(0),
                (),
                Some(span()),
                c::Reason::Replacement,
                &mut c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
                    candidate
                        .retire(&mut allocator, context, |token, allocator, _, context| {
                            drain.token_with_services(
                                token,
                                crate::retirement::Services {
                                    heap: None,
                                    channels: Some((&mut channels, &mut scheduler)),
                                },
                                allocator,
                                context,
                                |token, allocator, _, _| {
                                    retired_buffers += 1;
                                    crate::buffer::cleanup_token(types, token, allocator)
                                        .map_err(|_| 1u32)
                                },
                            )
                        })
                        .map_err(|_| 2u32)
                }),
            )
            .unwrap();
        candidate.release_storage(&mut allocator).unwrap();
        for token in [&mut owner, &mut handle, &mut old_sender] {
            session
                .retire(
                    c::Slot(0),
                    token,
                    Some(span()),
                    c::Reason::ScopeExit,
                    &mut c::Contextual::new(
                        |token: &mut &mut l::Token, _, context: &mut c::Context<'_>| {
                            drain.token_with_services(
                                token,
                                crate::retirement::Services {
                                    heap: None,
                                    channels: Some((&mut channels, &mut scheduler)),
                                },
                                &mut allocator,
                                context,
                                |token, allocator, _, _| {
                                    retired_buffers += 1;
                                    crate::buffer::cleanup_token(types, token, allocator)
                                        .map_err(|_| 1u32)
                                },
                            )
                        },
                    ),
                )
                .unwrap();
        }
        assert_eq!(retired_buffers, queued);
        assert_eq!(
            work.used(),
            cleanup_work,
            "exhaustion cannot meter mandatory cleanup"
        );
        assert_eq!(session.report().cleanup_failures(), 0);
        assert_eq!(channels.live_cells(), 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        used
    }
    let work = run(1, None);
    assert!(work > 0);
    assert_eq!(
        run(4, None),
        work,
        "the old state's queue is not a prospective root"
    );
    assert_eq!(run(4, Some(work)), work);
    run(4, Some(work - 1));
}

#[test]
fn prospective_state_cycle_is_rejected_before_swapping_or_consuming_the_candidate() {
    fn state<'l, 'ty>(
        layouts: &'l l::Layouts<'ty>,
        child: Option<l::Token>,
        allocator: &mut a::Allocator<a::System>,
    ) -> l::Reservation<'l, 'ty> {
        let value =
            l::Reservation::allocate(layouts, o::Type::Nominal(m::DefinitionId(0)), allocator)
                .unwrap();
        let root = value.root();
        let option = value.child(root, 0, allocator).unwrap();
        value
            .prepare_case(option, u32::from(child.is_some()), allocator)
            .unwrap();
        if let Some(child) = child {
            let mut input = l::Reservation::allocate(layouts, ct(11), allocator).unwrap();
            input
                .data_mut(allocator)
                .unwrap()
                .copy_from_slice(&child.bytes);
            let target = value.child(option, 0, allocator).unwrap();
            input.move_root_into(&value, target, allocator).unwrap();
            input.recycle(allocator).unwrap();
            input.release_storage(allocator).unwrap();
            value
                .initialize_order(option, [(0, span())].into_iter(), allocator)
                .unwrap();
        } else {
            value
                .initialize_order(option, [].into_iter(), allocator)
                .unwrap();
        }
        value
            .initialize_order(root, [(0, span())].into_iter(), allocator)
            .unwrap();
        value
    }
    let proof = proof();
    let layouts = l::Layouts::compile_service_tasks(&proof, Default::default()).unwrap();
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut scheduler = t::Scheduler::new(Default::default()).unwrap();
    let task = scheduler.start(None).unwrap();
    assert!(matches!(
        scheduler.dispatch().unwrap(),
        Some(t::Dispatch::Resume(_))
    ));
    let mut channels = Channels::prepare(
        &layouts,
        &allocator,
        &scheduler,
        &task,
        Limits {
            channels: 2,
            slots: 2,
            handles: 1,
            waiters: 0,
        },
        4,
        e::ServiceWork::new(None),
        &mut m::Budget::new(Default::default()),
    )
    .unwrap();
    let mut inspector =
        crate::inspection::Inspector::prepare(&layouts, 32, Default::default(), layouts.usage())
            .unwrap();
    let mut empty = state(&layouts, None, &mut allocator);
    let first = channels
        .create_service(
            ct(13),
            1,
            &mut empty,
            Some(span()),
            &mut inspector,
            None,
            &mut allocator,
        )
        .unwrap();
    empty.recycle(&mut allocator).unwrap();
    empty.release_storage(&mut allocator).unwrap();
    let mut handle = channels
        .sender(&first, ct(2), Some(span()), &mut allocator)
        .unwrap();
    let first_owner = channels.receiver(&first, ct(11), &mut allocator).unwrap();
    channels.service_received(0, true).unwrap();
    // Preserve an authenticated, non-owning operation argument before moving
    // A into B. B then moves into the candidate. The actual graph is B -> A;
    // installing that candidate as A's state would introduce A -> B -> A.
    // Source loan proof prevents this ancestor move while borrowing A; this
    // probe deliberately isolates the additional dynamic publication boundary.
    let mut observed = l::Token {
        leaf: l::Leaf {
            ty: ct(11),
            offset: 0,
            span: Some(span()),
        },
        bytes: first_owner.bytes,
    };
    let mut enclosing = state(&layouts, Some(first_owner), &mut allocator);
    let second = channels
        .create_service(
            ct(13),
            1,
            &mut enclosing,
            Some(span()),
            &mut inspector,
            None,
            &mut allocator,
        )
        .unwrap();
    enclosing.recycle(&mut allocator).unwrap();
    enclosing.release_storage(&mut allocator).unwrap();
    let second_owner = channels.receiver(&second, ct(11), &mut allocator).unwrap();
    let mut candidate = state(&layouts, Some(second_owner), &mut allocator);
    let old = channels.backing[0]
        .service
        .as_ref()
        .unwrap()
        .storage
        .as_ref()
        .unwrap()
        .data(&allocator)
        .unwrap()
        .to_vec();
    let generation = channels.backing[0].service.as_ref().unwrap().generation;
    let incoming = candidate.data(&allocator).unwrap().to_vec();
    assert!(matches!(
        channels.service_replace(
            &observed,
            &mut candidate,
            span(),
            &mut inspector,
            None,
            &mut allocator,
        ),
        Err(Error::Service(l::ServiceError::InvalidPublication))
    ));
    assert_eq!(
        channels.backing[0].service.as_ref().unwrap().generation,
        generation
    );
    assert_eq!(
        channels.backing[0]
            .service
            .as_ref()
            .unwrap()
            .storage
            .as_ref()
            .unwrap()
            .data(&allocator)
            .unwrap(),
        old
    );
    assert_eq!(candidate.data(&allocator).unwrap(), incoming);
    observed.bytes.fill(0);
    let mut session = c::Session::prepare(
        std::num::NonZeroUsize::new(8).unwrap(),
        &mut m::Budget::new(Default::default()),
    )
    .unwrap();
    let mut drain = crate::retirement::Drain::prepare(
        &layouts,
        &allocator,
        Default::default(),
        layouts.usage(),
    )
    .unwrap();
    session
        .retire(
            c::Slot(0),
            (),
            Some(span()),
            c::Reason::Replacement,
            &mut c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
                candidate
                    .retire(&mut allocator, context, |token, allocator, _, context| {
                        drain.token_with_services(
                            token,
                            crate::retirement::Services {
                                heap: None,
                                channels: Some((&mut channels, &mut scheduler)),
                            },
                            allocator,
                            context,
                            |_, _, _, _| Err(1u32),
                        )
                    })
                    .map_err(|_| 2u32)
            }),
        )
        .unwrap();
    candidate.release_storage(&mut allocator).unwrap();
    session
        .retire(
            c::Slot(0),
            &mut handle,
            Some(span()),
            c::Reason::ScopeExit,
            &mut c::Contextual::new(
                |token: &mut &mut l::Token, _, context: &mut c::Context<'_>| {
                    drain.token_with_services(
                        token,
                        crate::retirement::Services {
                            heap: None,
                            channels: Some((&mut channels, &mut scheduler)),
                        },
                        &mut allocator,
                        context,
                        |_, _, _, _| Err(1u32),
                    )
                },
            ),
        )
        .unwrap();
    assert_eq!(session.report().cleanup_failures(), 0);
    assert_eq!(channels.live_cells(), 0);
    assert_eq!(allocator.stats().live_allocations, 0);
}
