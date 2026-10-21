use super::*;
use lang_mir::unit::ownership::{bodies as b, tasks};

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1,
    }
}
fn ct(id: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(id))
}
fn proof() -> tasks::VerifiedTasks {
    let mut table = b::BodyTable::default();
    table.places.declarations.types.sources.push(m::Source {
        path: "channels.lang".into(),
        text: "x".into(),
    });
    for constructor in [
        o::Constructor::Builtin(o::Builtin::Bytes),
        o::Constructor::ChannelSetup(ct(0)),
        o::Constructor::Sender(ct(0)),
        o::Constructor::Receiver(ct(0)),
        o::Constructor::SendResult(ct(0)),
        o::Constructor::Builtin(o::Builtin::ChannelError),
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
fn retire_setup(
    channels: &mut Channels<'_, '_>,
    token: &mut l::Token,
    scheduler: &mut t::Scheduler,
    allocator: &mut a::Allocator<a::System>,
) {
    let mut session = c::Session::prepare(
        std::num::NonZeroUsize::new(2).unwrap(),
        &mut m::Budget::new(Default::default()),
    )
    .unwrap();
    session
        .retire(
            c::Slot(0),
            token,
            Some(span()),
            c::Reason::ScopeExit,
            &mut c::Contextual::new(
                |token: &mut &mut l::Token, _, context: &mut c::Context<'_>| {
                    channels
                        .retire(token, scheduler, allocator, context, |_, _, _, _| {
                            panic!("empty slots contain no value owner")
                        })
                        .map_err(|_| 1u32)
                },
            ),
        )
        .unwrap();
    assert_eq!(session.report().cleanup_failures(), 0);
}

#[test]
fn only_explicit_channel_proof_selects_private_storage_and_finite_errors() {
    let proof = proof();
    assert!(matches!(
        l::Layouts::compile(
            proof.typed().places().declarations().types(),
            Default::default()
        ),
        Err(l::Error::UnsupportedChannelProfile { .. })
    ));
    assert!(matches!(
        l::Layouts::compile_tasks(&proof, Default::default()),
        Err(l::Error::UnsupportedChannelProfile { .. })
    ));
    let layouts = l::Layouts::compile_channel_tasks(&proof, Default::default()).unwrap();
    for ty in [ct(1), ct(2), ct(3)] {
        let layout = layouts.get(ty).unwrap();
        assert_eq!((layout.size, layout.alignment), (32, 8));
        assert!(matches!(layout.kind, l::Kind::Token(_)));
    }
    assert!(matches!(&layouts.get(ct(4)).unwrap().kind,l::Kind::Enum{cases,..} if cases.len()==3));
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut value = l::Reservation::allocate(&layouts, ct(5), &mut allocator).unwrap();
    for error in [
        l::ChannelError::InvalidCapacity,
        l::ChannelError::Allocation(crate::buffer::AllocError::Limit),
        l::ChannelError::Allocation(crate::buffer::AllocError::OutOfMemory),
        l::ChannelError::Allocation(crate::buffer::AllocError::Overflow),
    ] {
        value
            .write_immediate(value.root(), l::Immediate::Channel(error), &mut allocator)
            .unwrap();
        assert_eq!(
            value.read_immediate(value.root(), &allocator).unwrap(),
            l::Immediate::Channel(error)
        );
    }
    value.release_storage(&mut allocator).unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn every_guest_queue_allocation_prefix_rolls_back_before_setup_publication() {
    let proof = proof();
    let layouts = l::Layouts::compile_channel_tasks(&proof, Default::default()).unwrap();
    for live_limit in 0..=5 {
        let mut allocator = a::Allocator::new(
            a::System,
            a::Limits {
                live_allocations: live_limit,
                ..Default::default()
            },
        )
        .unwrap();
        let mut scheduler = t::Scheduler::new(Default::default()).unwrap();
        let root = scheduler.start(None).unwrap();
        let mut budget = m::Budget::new(Default::default());
        let mut channels = Channels::prepare(
            &layouts,
            &allocator,
            &scheduler,
            &root,
            Limits {
                channels: 1,
                slots: 2,
                handles: 2,
                waiters: 1,
            },
            2,
            e::ServiceWork::new(None),
            &mut budget,
        )
        .unwrap();
        let result = channels.create(ct(1), 2, Some(span()), &mut allocator);
        if live_limit < 5 {
            assert!(result.is_err());
            assert_eq!(channels.usage().channels, 0);
            assert_eq!(channels.live_cells(), 0);
        } else {
            let mut token = result.unwrap();
            assert_eq!(channels.usage().reserved_slots, 2);
            assert_eq!(channels.live_cells(), 2);
            assert_eq!(allocator.stats().live_allocations, 5);
            retire_setup(&mut channels, &mut token, &mut scheduler, &mut allocator);
            assert_eq!(channels.live_cells(), 0);
        }
        assert_eq!(
            allocator.stats().live_allocations,
            0,
            "guest allocation prefix {live_limit}"
        );
    }
}

fn make_message<'l, 'ty>(
    channels: &mut Channels<'l, 'ty>,
    sender: &l::Token,
    value: u8,
    allocator: &mut a::Allocator<a::System>,
) -> Message<'l, 'ty> {
    let mut message = channels.stage(sender, Some(span()), allocator).unwrap();
    let mut bytes = crate::buffer::Bytes::from_slice(allocator, &[value]).unwrap();
    let token = bytes.take_token(allocator).unwrap();
    message
        .storage
        .as_mut()
        .unwrap()
        .data_mut(allocator)
        .unwrap()
        .copy_from_slice(&token);
    message
}
fn receive_one<'l, 'ty>(
    channels: &mut Channels<'l, 'ty>,
    scheduler: &mut t::Scheduler,
    task: &t::TaskId,
    receiver: &l::Token,
    allocator: &mut a::Allocator<a::System>,
    session: &mut c::Session,
) -> u8 {
    let Receive::Value { value: message, .. } = channels
        .receive(scheduler, task, receiver, Some(span()), allocator)
        .unwrap()
    else {
        panic!("committed guest owner")
    };
    let storage = message.storage.as_ref().unwrap();
    let mut token = storage.read_token(storage.root(), allocator).unwrap();
    let bytes = crate::buffer::Bytes::from_token(&mut token.bytes, allocator).unwrap();
    let value = bytes.as_slice(allocator).unwrap()[0];
    let types = channels.layouts.types();
    session
        .retire(
            c::Slot(0),
            message,
            Some(span()),
            c::Reason::ScopeExit,
            &mut c::Contextual::new(
                |message: &mut Message<'l, 'ty>, _, context: &mut c::Context<'_>| {
                    channels
                        .retire_message(message, allocator, context, |token, allocator, _, _| {
                            crate::buffer::cleanup_token(types, token, allocator).map_err(|_| 1u32)
                        })
                        .map_err(|_| 2u32)
                },
            ),
        )
        .unwrap();
    value
}
#[test]
fn guest_backed_bytes_cross_capacity_one_backpressure_and_retire_all_storage() {
    let proof = proof();
    let layouts = l::Layouts::compile_channel_tasks(&proof, Default::default()).unwrap();
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut scheduler = t::Scheduler::new(t::Limits {
        live_tasks: 2,
        total_tasks: 2,
    })
    .unwrap();
    let root = scheduler.start(None).unwrap();
    assert!(matches!(
        scheduler.dispatch().unwrap(),
        Some(t::Dispatch::Resume(_))
    ));
    let child = scheduler.spawn(&root, None).unwrap();
    let mut budget = m::Budget::new(Default::default());
    let mut channels = Channels::prepare(
        &layouts,
        &allocator,
        &scheduler,
        &root,
        Limits {
            channels: 1,
            slots: 1,
            handles: 2,
            waiters: 2,
        },
        2,
        e::ServiceWork::new(None),
        &mut budget,
    )
    .unwrap();
    let setup = channels
        .create(ct(1), 1, Some(span()), &mut allocator)
        .unwrap();
    let mut sender = channels
        .sender(&setup, ct(2), Some(span()), &mut allocator)
        .unwrap();
    let mut receiver = channels.receiver(&setup, ct(3), &mut allocator).unwrap();
    let mut inspector =
        crate::inspection::Inspector::prepare(&layouts, 32, Default::default(), layouts.usage())
            .unwrap();
    let first = make_message(&mut channels, &sender, 20, &mut allocator);
    assert!(matches!(
        channels
            .send(
                &mut scheduler,
                &root,
                &sender,
                first,
                Some(span()),
                &mut inspector,
                &allocator
            )
            .unwrap(),
        Send::Sent { position: 0 }
    ));
    let second = make_message(&mut channels, &sender, 22, &mut allocator);
    let Send::Waiting(operation) = channels
        .send(
            &mut scheduler,
            &root,
            &sender,
            second,
            Some(span()),
            &mut inspector,
            &allocator,
        )
        .unwrap()
    else {
        panic!("capacity-one backpressure")
    };
    assert_eq!(channels.live_cells(), 2);
    assert!(matches!(
        scheduler.dispatch().unwrap(),
        Some(t::Dispatch::Resume(_))
    ));
    let mut session = c::Session::prepare(
        std::num::NonZeroUsize::new(4).unwrap(),
        &mut m::Budget::new(Default::default()),
    )
    .unwrap();
    let first = receive_one(
        &mut channels,
        &mut scheduler,
        &child,
        &receiver,
        &mut allocator,
        &mut session,
    );
    scheduler.yield_now(&child).unwrap();
    assert!(matches!(
        scheduler.dispatch().unwrap(),
        Some(t::Dispatch::Resume(_))
    ));
    assert!(matches!(
        channels
            .resume_send(&mut scheduler, &operation, &mut inspector, &allocator)
            .unwrap(),
        Send::Sent { position: 1 }
    ));
    session
        .retire(
            c::Slot(1),
            &mut sender,
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
                            |_, _, _, _| unreachable!(),
                        )
                        .map_err(|_| 3u32)
                },
            ),
        )
        .unwrap();
    scheduler.finish(&root, t::Outcome::Success).unwrap();
    assert!(matches!(
        scheduler.dispatch().unwrap(),
        Some(t::Dispatch::Resume(_))
    ));
    let second = receive_one(
        &mut channels,
        &mut scheduler,
        &child,
        &receiver,
        &mut allocator,
        &mut session,
    );
    assert!(matches!(
        channels
            .receive(&mut scheduler, &child, &receiver, None, &allocator)
            .unwrap(),
        Receive::Closed
    ));
    assert_eq!(channels.live_cells(), 1);
    retire_setup(&mut channels, &mut receiver, &mut scheduler, &mut allocator);
    assert_eq!([first, second], [20, 22]);
    assert_eq!(i64::from(first) + i64::from(second), 42);
    assert_eq!(channels.live_cells(), 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(session.report().cleanup_failures(), 0);
    scheduler.finish(&child, t::Outcome::Success).unwrap();
    while let Some(dispatch) = scheduler.dispatch().unwrap() {
        let t::Dispatch::Cleanup { task, .. } = dispatch else {
            panic!("finished tree")
        };
        scheduler.finish_cleanup(&task, None).unwrap();
    }
    assert!(scheduler.is_finished());
}
