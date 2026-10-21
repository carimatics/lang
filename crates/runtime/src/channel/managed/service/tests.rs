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
