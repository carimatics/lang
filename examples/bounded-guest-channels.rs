//! C03 integration experiment: real guest Bytes reservations over the C02 Hub.
//! This does not yet admit source channel operations or endpoint layouts.
use lang_mir::unit::{self as m, ownership as o};
use lang_runtime::{
    allocation as a, buffer::Bytes, channel as h, cleanup as c, execution as e, layout as l,
    task as t,
};
use std::num::NonZeroUsize;

struct Message<'l, 'ty>(l::Reservation<'l, 'ty>);
impl h::Payload for Message<'_, '_> {
    fn receivers(
        &self,
        _: &mut dyn FnMut(&h::Receiver) -> Result<(), h::Error>,
    ) -> Result<(), h::Error> {
        // This wrapper is constructed only for the exact verified Bytes type;
        // no endpoint, container or shared value can enter this experiment.
        Ok(())
    }
}
fn message<'l, 'ty>(
    layouts: &'l l::Layouts<'ty>,
    ty: o::Type,
    value: u8,
    allocator: &mut a::Allocator<a::System>,
) -> Message<'l, 'ty> {
    assert!(
        matches!(ty, o::Type::Constructed(id) if layouts.types().table().constructed[id.0 as usize].constructor == o::Constructor::Builtin(o::Builtin::Bytes))
    );
    let reservation = l::Reservation::allocate(layouts, ty, allocator).unwrap();
    let mut bytes = Bytes::from_slice(allocator, &[value]).unwrap();
    let token = bytes.take_token(allocator).unwrap();
    reservation
        .data_mut(allocator)
        .unwrap()
        .copy_from_slice(&token);
    Message(reservation)
}
fn retire(
    message: &mut Message<'_, '_>,
    allocator: &mut a::Allocator<a::System>,
    context: &mut c::Context<'_>,
    retired: &mut Vec<u8>,
) -> Result<(), u32> {
    message
        .0
        .retire(allocator, context, |token, allocator, _, _| {
            let mut bytes = Bytes::from_token(&mut token.bytes, allocator).map_err(|_| 1_u32)?;
            retired.push(bytes.as_slice(allocator).map_err(|_| 2_u32)?[0]);
            bytes.release(allocator).map_err(|_| 3_u32)
        })
        .map_err(|_| 4_u32)?;
    message.0.release_storage(allocator).map_err(|_| 5_u32)
}
fn main() {
    let ty = o::Type::Constructed(o::ConstructedId(0));
    let types = o::TypeTable {
        sources: vec![m::Source {
            path: "guest-channel.lang".into(),
            text: String::new(),
        }],
        constructed: vec![o::Constructed {
            constructor: o::Constructor::Builtin(o::Builtin::Bytes),
            span: m::Span {
                source: m::SourceId(0),
                start: 0,
                end: 0,
            },
        }],
        ..Default::default()
    }
    .verify(Default::default())
    .unwrap();
    let layouts = l::Layouts::compile(&types, Default::default()).unwrap();
    for cancel in [false, true] {
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
        let work = e::ServiceWork::new(Some(64));
        let mut hub =
            h::Hub::<Message<'_, '_>>::prepare(&scheduler, &root, h::Limits::default(), work)
                .unwrap();
        let setup = hub.create(1).unwrap();
        let sender = hub.sender(&setup).unwrap();
        let mut receiver = hub.receiver(setup).unwrap();
        let first = message(&layouts, ty, 20, &mut allocator);
        let owner = first.0.root();
        assert!(matches!(
            hub.send(&mut scheduler, &root, &sender, first, None)
                .map_err(|e| e.error)
                .unwrap(),
            h::Send::Sent { position: 0 }
        ));
        let second = message(&layouts, ty, 22, &mut allocator);
        let h::Send::Waiting(wait) = hub
            .send(&mut scheduler, &root, &sender, second, None)
            .map_err(|e| e.error)
            .unwrap()
        else {
            panic!("full send must park")
        };
        assert_eq!(hub.usage().queued, 1);
        assert_eq!(hub.usage().pending, 1);
        let before = allocator.stats().live_allocations;
        let mut session = c::Session::prepare(
            NonZeroUsize::new(4).unwrap(),
            &mut m::Budget::new(Default::default()),
        )
        .unwrap();
        let mut retired = Vec::new();
        assert!(matches!(
            scheduler.dispatch().unwrap(),
            Some(t::Dispatch::Resume(_))
        ));
        if cancel {
            let h::Cancelled::Send(value) = hub.cancel(&mut scheduler, &wait).unwrap() else {
                panic!("pending owner")
            };
            session
                .retire(
                    c::Slot(0),
                    value,
                    None,
                    c::Reason::Failure,
                    &mut c::Contextual::new(
                        |v: &mut Message<'_, '_>, _, context: &mut c::Context<'_>| {
                            retire(v, &mut allocator, context, &mut retired)
                        },
                    ),
                )
                .unwrap();
        } else {
            let h::Receive::Value { value, position: 0 } = hub
                .receive(&mut scheduler, &child, &mut receiver, None)
                .unwrap()
            else {
                panic!("queued owner")
            };
            assert_eq!(value.0.root(), owner);
            session
                .retire(
                    c::Slot(0),
                    value,
                    None,
                    c::Reason::ScopeExit,
                    &mut c::Contextual::new(
                        |v: &mut Message<'_, '_>, _, context: &mut c::Context<'_>| {
                            retire(v, &mut allocator, context, &mut retired)
                        },
                    ),
                )
                .unwrap();
            scheduler.yield_now(&child).unwrap();
            assert!(matches!(
                scheduler.dispatch().unwrap(),
                Some(t::Dispatch::Resume(_))
            ));
            assert!(matches!(
                hub.resume_send(&mut scheduler, &wait).unwrap(),
                h::Send::Sent { position: 1 }
            ));
            hub.release_sender(&mut scheduler, &sender).unwrap();
            scheduler.finish(&root, t::Outcome::Success).unwrap();
            assert!(matches!(
                scheduler.dispatch().unwrap(),
                Some(t::Dispatch::Resume(_))
            ));
            let h::Receive::Value { value, position: 1 } = hub
                .receive(&mut scheduler, &child, &mut receiver, None)
                .unwrap()
            else {
                panic!("resumed owner")
            };
            session
                .retire(
                    c::Slot(0),
                    value,
                    None,
                    c::Reason::ScopeExit,
                    &mut c::Contextual::new(
                        |v: &mut Message<'_, '_>, _, context: &mut c::Context<'_>| {
                            retire(v, &mut allocator, context, &mut retired)
                        },
                    ),
                )
                .unwrap();
        }
        if cancel {
            hub.release_sender(&mut scheduler, &sender).unwrap();
        }
        hub.abort_receiver(
            &mut scheduler,
            &receiver,
            &mut session,
            if cancel {
                c::Reason::Failure
            } else {
                c::Reason::ScopeExit
            },
            &mut c::Contextual::new(|v: &mut Message<'_, '_>, _, context: &mut c::Context<'_>| {
                retire(v, &mut allocator, context, &mut retired)
            }),
        )
        .unwrap();
        scheduler
            .finish(
                &child,
                if cancel {
                    t::Outcome::Cancelled
                } else {
                    t::Outcome::Success
                },
            )
            .unwrap();
        while let Some(dispatch) = scheduler.dispatch().unwrap() {
            match dispatch {
                t::Dispatch::Cleanup { task, .. } => {
                    scheduler.finish_cleanup(&task, None).unwrap();
                }
                t::Dispatch::Resume(_) => panic!("cancelled root cannot resume"),
            }
        }
        assert!(scheduler.is_finished());
        assert_eq!(allocator.stats().live_allocations, 0);
        assert_eq!(
            allocator.stats().allocations,
            allocator.stats().deallocations
        );
        assert_eq!(retired, if cancel { vec![22, 20] } else { vec![20, 22] });
        assert_eq!(hub.usage().reserved_slots, 0);
        println!(
            "cancel={cancel} sum={} retired={retired:?} peak_live_allocations={before} live=0",
            retired.iter().map(|v| u32::from(*v)).sum::<u32>()
        );
    }
}
