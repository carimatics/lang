use lang_runtime::{allocation as a, execution as e, task::runner as r};
use lang_semantics::analysis::{Program, TaskProgram};
use std::path::Path;

#[path = "support/channel_fixtures.rs"]
mod channel_fixtures;
use channel_fixtures::{MANAGED_CHANNEL_FIXTURES, channel_allocator};

fn prepare(text: &str) -> Box<TaskProgram> {
    let out = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("source-channels.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let Program::Tasks(program) = out.result.unwrap() else {
        panic!("channel task profile")
    };
    program
}

#[test]
fn owned_source_channels_execute_backpressure_and_retire_every_guest_allocation() {
    let program = prepare(include_str!("fixtures/task_channels.lang"));
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        r::Limits {
            trace_events: 2048,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(result.tasks.live, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    for task in [1, 2] {
        assert!(
            result
                .events
                .iter()
                .filter(|event| event.task == task && event.kind == r::EventKind::Resume)
                .count()
                >= 2,
            "both producer backpressure and receiver parking must actually resume"
        );
    }
}

#[test]
fn sealed_send_returns_the_same_owned_payload_through_whole_result_extraction() {
    let program = prepare(
        r#"
async fn main()->i64 {
    let mut setup:core::ChannelSetup<core::Bytes>=core::result_unwrap::<core::ChannelSetup<core::Bytes>,core::ChannelError>(core::channel_new::<core::Bytes>(core::allocator(),1));
    let mut sender:core::Sender<core::Bytes>=core::channel_sender::<core::Bytes>(&setup);
    let mut receiver:core::Receiver<core::Bytes>=core::channel_receiver::<core::Bytes>(setup);
    let mut sealed:core::Unit=core::channel_seal::<core::Bytes>(&mut receiver);
    let mut value:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed:core::Result<core::Unit,core::BufferError>=core::bytes_push(&mut value,42);
    let mut outcome:core::SendResult<core::Bytes>=await core::channel_send::<core::Bytes>(&sender,value);
    let mut unsent:core::Bytes=core::channel_unsent::<core::Bytes>(outcome);
    return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(&unsent,0));
}
"#,
    );
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn extracting_sent_has_the_recorded_panic_code_and_drains_the_invocation() {
    let program = prepare(
        "fn main()->i64{let mut sent:core::SendResult<i64>=core::SendResult::Sent;return core::channel_unsent::<i64>(sent);}",
    );
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    );
    let Err(error) = result else {
        panic!("Sent has no payload")
    };
    assert!(
        matches!(error.reason, r::Reason::Panic(Some(-4))),
        "{error:?}"
    );
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn cancelling_a_parked_sender_retires_uncommitted_and_queued_payloads() {
    let producer = include_str!("fixtures/task_channels.lang")
        .split("async fn consumer")
        .next()
        .unwrap();
    let text = format!(
        "{producer}\nasync fn main()->i64{{let mut setup:core::ChannelSetup<core::Bytes>=core::result_unwrap::<core::ChannelSetup<core::Bytes>,core::ChannelError>(core::channel_new::<core::Bytes>(core::allocator(),1));let mut sender:core::Sender<core::Bytes>=core::channel_sender::<core::Bytes>(&setup);let mut receiver:core::Receiver<core::Bytes>=core::channel_receiver::<core::Bytes>(setup);let mut sending:core::Task<i64>=spawn producer(sender);yield;cancel sending;return 42;}}"
    );
    let program = prepare(&text);
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        r::Limits {
            trace_events: 1024,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(result.tasks.live, 0);
    assert_eq!(
        allocator.stats().live_allocations,
        0,
        "parked payload and private capture must be retired before the queued owner"
    );
}

#[test]
fn a_closed_invocation_stall_has_the_oldest_typed_deadlock_origin() {
    let producer = include_str!("fixtures/task_channels.lang")
        .split("async fn consumer")
        .next()
        .unwrap();
    let text = format!(
        "{producer}\nasync fn main()->i64{{let mut setup:core::ChannelSetup<core::Bytes>=core::result_unwrap::<core::ChannelSetup<core::Bytes>,core::ChannelError>(core::channel_new::<core::Bytes>(core::allocator(),1));let mut sender:core::Sender<core::Bytes>=core::channel_sender::<core::Bytes>(&setup);let mut receiver:core::Receiver<core::Bytes>=core::channel_receiver::<core::Bytes>(setup);let mut sending:core::Task<i64>=spawn producer(sender);return core::result_unwrap::<i64,core::TaskError>(await sending);}}"
    );
    let program = prepare(&text);
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    );
    let Err(error) = result else {
        panic!("no receiver can make the full send ready")
    };
    let r::Reason::ChannelDeadlock(ref deadlock) = error.reason else {
        panic!("{error:?}")
    };
    let origin = deadlock.operation.origin().unwrap();
    assert_eq!(
        &text[origin.start as usize..origin.end as usize],
        "await core::channel_send::<core::Bytes>(&sender, second)"
    );
    assert_eq!(error.span, Some(origin));
    assert_eq!(
        error.report.as_ref().unwrap().primary().unwrap().span,
        Some(origin)
    );
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn each_service_work_prefix_drains_parked_and_committed_source_owners() {
    let program = prepare(include_str!("fixtures/task_channels.lang"));
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()).and_service_work(Some(u64::MAX)),
        Default::default(),
    )
    .unwrap();
    let total = result.usage.service_work;
    assert!(total > 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    for limit in 0..=total {
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            e::Control::with_limits(Default::default()).and_service_work(Some(limit)),
            Default::default(),
        );
        assert_eq!(
            allocator.stats().live_allocations,
            0,
            "service work {limit}/{total}"
        );
        if limit < total {
            let Err(error) = result else {
                panic!("work prefix {limit}")
            };
            assert!(
                matches!(error.reason,r::Reason::Control(e::Failure::ServiceWorkLimit{limit:l,..}) if l==limit),
                "{limit}: {error:?}"
            );
        } else {
            assert_eq!(result.unwrap().value, 42);
        }
    }
}

#[test]
fn captures_survive_ended_argument_loans_and_synchronous_helpers() {
    let program = prepare(
        r#"
fn mint(setup:&core::ChannelSetup<i64>)->core::Sender<i64>{return core::channel_sender::<i64>(setup);}
async fn main()->i64 {
    let mut setup:core::ChannelSetup<i64>=core::result_unwrap::<core::ChannelSetup<i64>,core::ChannelError>(core::channel_new::<i64>(core::allocator(),1));
    let mut sender:core::Sender<i64>=mint(&setup);
    let mut sent:core::SendResult<i64>=await core::channel_send::<i64>(&sender,42);
    let mut receiver:core::Receiver<i64>=core::channel_receiver::<i64>(setup);
    return core::option_unwrap::<i64>(await core::channel_receive::<i64>(&mut receiver));
}


"#,
    );
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn cloned_source_senders_preserve_fifo_across_multiple_producers() {
    let program = prepare(
        r#"
async fn send(sender:core::Sender<i64>,value:i64)->i64{let mut outcome:core::SendResult<i64>=await core::channel_send::<i64>(&sender,value);return value;}
async fn receive(input:core::Receiver<i64>)->i64{let mut receiver:core::Receiver<i64>=input;let mut left:i64=core::option_unwrap::<i64>(await core::channel_receive::<i64>(&mut receiver));let mut right:i64=core::option_unwrap::<i64>(await core::channel_receive::<i64>(&mut receiver));if left<right{return left+right;}return -1;}
async fn main()->i64{
    let mut setup:core::ChannelSetup<i64>=core::result_unwrap::<core::ChannelSetup<i64>,core::ChannelError>(core::channel_new::<i64>(core::allocator(),1));
    let mut first:core::Sender<i64>=core::channel_sender::<i64>(&setup);
    let mut second:core::Sender<i64>=core::channel_clone::<i64>(&first);
    let mut receiver:core::Receiver<i64>=core::channel_receiver::<i64>(setup);
    let mut a:core::Task<i64>=spawn send(first,20);
    let mut b:core::Task<i64>=spawn send(second,22);
    let mut c:core::Task<i64>=spawn receive(receiver);
    let mut result:i64=core::result_unwrap::<i64,core::TaskError>(await c);
    let mut x:i64=core::result_unwrap::<i64,core::TaskError>(await a);
    let mut y:i64=core::result_unwrap::<i64,core::TaskError>(await b);
    return result;
}
"#,
    );
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn each_fuel_and_trace_prefix_retires_private_channel_custody() {
    let program = prepare(include_str!("fixtures/task_channels.lang"));
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let baseline = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        r::Limits {
            trace_events: 2048,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(allocator.stats().live_allocations, 0);
    for fuel in 0..=baseline.usage.fuel {
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            e::Control::with_limits(e::Limits {
                fuel: Some(fuel),
                ..Default::default()
            }),
            Default::default(),
        );
        assert_eq!(
            allocator.stats().live_allocations,
            0,
            "fuel {fuel}/{}",
            baseline.usage.fuel
        );
        if fuel < baseline.usage.fuel {
            let Err(error) = result else {
                panic!("fuel prefix {fuel}")
            };
            assert!(
                matches!(error.reason, r::Reason::Control(e::Failure::FuelExhausted)),
                "{fuel}: {error:?}"
            );
        } else {
            assert_eq!(result.unwrap().value, 42);
        }
    }
    for trace in 1..=baseline.events.len() {
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: trace,
                ..Default::default()
            },
        );
        assert_eq!(
            allocator.stats().live_allocations,
            0,
            "trace {trace}/{}",
            baseline.events.len()
        );
        if trace < baseline.events.len() {
            let Err(error) = result else {
                panic!("trace prefix {trace}")
            };
            assert!(
                matches!(error.reason, r::Reason::TraceLimit),
                "{trace}: {error:?}"
            );
        } else {
            assert_eq!(result.unwrap().value, 42);
        }
    }
}

#[test]
fn queue_and_blocked_storage_count_against_the_invocations_live_cell_limit() {
    let program = prepare(include_str!("fixtures/task_channels.lang"));
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let baseline = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    for cells in 0..=baseline.usage.peak_live_cells {
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            e::Control::with_limits(e::Limits {
                live_cells: Some(cells),
                ..Default::default()
            }),
            Default::default(),
        );
        assert_eq!(allocator.stats().live_allocations, 0, "cell prefix {cells}");
        if cells < baseline.usage.peak_live_cells {
            assert!(
                result.is_err(),
                "cell prefix {cells}/{} must include queue and pending reservations",
                baseline.usage.peak_live_cells
            );
        } else {
            assert_eq!(result.unwrap().value, 42);
        }
    }
}

#[test]
fn sending_after_receiver_retirement_returns_closed_without_new_guest_storage() {
    struct StopAfterRelease {
        system: a::System,
        stopped: std::rc::Rc<std::cell::Cell<bool>>,
    }
    impl a::Provider for StopAfterRelease {
        type Region = a::Storage;
        fn allocate(&mut self, request: a::Request) -> Option<a::Storage> {
            if self.stopped.get() {
                None
            } else {
                self.system.allocate(request)
            }
        }
        fn deallocate(&mut self, region: a::Storage, request: a::Request) {
            if request.bytes() != 0 {
                self.stopped.set(true);
            }
            self.system.deallocate(region, request);
        }
    }
    let program = prepare(
        r#"
async fn main()->i64{
 let mut setup:core::ChannelSetup<i64>=core::result_unwrap::<core::ChannelSetup<i64>,core::ChannelError>(core::channel_new::<i64>(core::allocator(),1));
 let mut sender:core::Sender<i64>=core::channel_sender::<i64>(&setup);
 let mut receiver:core::Receiver<i64>=core::channel_receiver::<i64>(setup);
 let mut dropped:core::Unit=core::drop::<core::Receiver<i64>>(receiver);
 let mut result:core::SendResult<i64>=await core::channel_send::<i64>(&sender,42);
 return core::channel_unsent::<i64>(result);
}


"#,
    );
    let stopped = std::rc::Rc::new(std::cell::Cell::new(false));
    let mut allocator = a::Allocator::new(
        StopAfterRelease {
            system: a::System,
            stopped: stopped.clone(),
        },
        Default::default(),
    )
    .unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert!(stopped.get());
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn every_guest_allocation_prefix_retires_frames_queue_metadata_and_payloads() {
    let program = prepare(include_str!("fixtures/task_channels.lang"));
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    let peak = allocator.stats().peak_live_allocations;
    assert_eq!(allocator.stats().live_allocations, 0);
    for allocations in 0..=peak {
        let mut allocator = a::Allocator::new(
            a::System,
            a::Limits {
                live_allocations: allocations,
                ..Default::default()
            },
        )
        .unwrap();
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            e::Control::with_limits(Default::default()),
            Default::default(),
        );
        assert_eq!(
            allocator.stats().live_allocations,
            0,
            "allocation prefix {allocations}/{peak}"
        );
        if allocations < peak {
            assert!(result.is_err(), "allocation prefix {allocations}");
        } else {
            assert_eq!(result.unwrap().value, 42);
        }
    }
}

#[test]
fn rejected_payload_cleanup_keeps_its_owner_origin_after_unsent_extraction() {
    struct PanicPayload(a::System);
    impl a::Provider for PanicPayload {
        type Region = a::Storage;
        fn allocate(&mut self, request: a::Request) -> Option<a::Storage> {
            self.0.allocate(request)
        }
        fn deallocate(&mut self, region: a::Storage, request: a::Request) {
            use a::Region as _;
            let data = region.bytes();
            let payload = data.len() == 25
                && data[..8] == 1u64.to_le_bytes()
                && data[8..16] == 1u64.to_le_bytes()
                && data[16..24] == [0; 8]
                && data[24] == 42;
            self.0.deallocate(region, request);
            assert!(!payload, "contained channel payload cleanup panic");
        }
    }
    let text = r#"
async fn main()->i64 {
 let mut setup:core::ChannelSetup<core::Bytes>=core::result_unwrap::<core::ChannelSetup<core::Bytes>,core::ChannelError>(core::channel_new::<core::Bytes>(core::allocator(),1));
 let mut sender:core::Sender<core::Bytes>=core::channel_sender::<core::Bytes>(&setup);
 let mut receiver:core::Receiver<core::Bytes>=core::channel_receiver::<core::Bytes>(setup);
 let mut sealed:core::Unit=core::channel_seal::<core::Bytes>(&mut receiver);
 let mut value:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
 let mut pushed:core::Result<core::Unit,core::BufferError>=core::bytes_push(&mut value,42);
 let mut outcome:core::SendResult<core::Bytes>=await core::channel_send::<core::Bytes>(&sender,value);
 let mut unsent:core::Bytes=core::channel_unsent::<core::Bytes>(outcome);
 return 42;
}
"#;
    let program = prepare(text);
    let mut allocator = a::Allocator::new(PanicPayload(a::System), Default::default()).unwrap();
    let failure = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .err()
    .unwrap();
    assert!(matches!(failure.reason, r::Reason::Cleanup), "{failure:?}");
    assert!(failure.poisoned);
    let report = failure.report.unwrap();
    assert_eq!(report.cleanup_failures(), 1);
    let span = report.primary().unwrap().span.unwrap();
    assert!(
        (span.end as usize) <= text.find("await core::channel_send").unwrap(),
        "the recovered payload must retain its pre-send owner origin: {:?}",
        &text[span.start as usize..span.end as usize]
    );
    assert_eq!(failure.tasks.live, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn shared_sender_publication_and_replacement_keep_checked_opaque_authority() {
    let program = prepare(include_str!("fixtures/task_channel_shared_sender.lang"));
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(result.tasks.live, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn a_queued_last_sender_retires_before_its_parent_channel_header() {
    let program = prepare(
        r#"
struct Message { route:core::Sender<Message> }
async fn main()->i64 {
 let mut setup:core::ChannelSetup<Message>=core::result_unwrap::<core::ChannelSetup<Message>,core::ChannelError>(core::channel_new::<Message>(core::allocator(),1));
 let mut sender:core::Sender<Message>=core::channel_sender::<Message>(&setup);
 let mut receiver:core::Receiver<Message>=core::channel_receiver::<Message>(setup);
 let mut queued:core::Sender<Message>=core::channel_clone::<Message>(&sender);
 let mut sent:core::SendResult<Message>=await core::channel_send::<Message>(&sender,Message {route:queued});
 let mut dropped:core::Unit=core::drop::<core::Sender<Message>>(sender);
 return 42;
}
"#,
    );
    let (mut allocator, trace) = channel_allocator(usize::MAX);
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(trace.borrow().live.is_empty());
}

#[test]
fn shared_sender_message_parameters_do_not_become_physical_receiver_edges() {
    let program = prepare(
        r#"
struct Packet { route:shared core::Sender<core::Receiver<i64>> }
async fn main()->i64 {
 let mut leaf:core::ChannelSetup<core::Receiver<i64>>=core::result_unwrap::<core::ChannelSetup<core::Receiver<i64>>,core::ChannelError>(core::channel_new::<core::Receiver<i64>>(core::allocator(),1));
 let mut route:core::Sender<core::Receiver<i64>>=core::channel_sender::<core::Receiver<i64>>(&leaf);
 let mut leaf_receiver:core::Receiver<core::Receiver<i64>>=core::channel_receiver::<core::Receiver<i64>>(leaf);
 let mut stored:shared core::Sender<core::Receiver<i64>>=core::result_unwrap::<shared core::Sender<core::Receiver<i64>>,core::OwnershipError>(core::shared_new::<core::Sender<core::Receiver<i64>>>(core::allocator(),route));
 let mut setup:core::ChannelSetup<Packet>=core::result_unwrap::<core::ChannelSetup<Packet>,core::ChannelError>(core::channel_new::<Packet>(core::allocator(),1));
 let mut sender:core::Sender<Packet>=core::channel_sender::<Packet>(&setup);
 let mut receiver:core::Receiver<Packet>=core::channel_receiver::<Packet>(setup);
 let mut sent:core::SendResult<Packet>=await core::channel_send::<Packet>(&sender,Packet {route:stored});
 match sent {core::SendResult::Sent=>{},_=>{return -1;},}
 let mut incoming:core::Option<Packet>=await core::channel_receive::<Packet>(&mut receiver);
 let mut packet:Packet=core::option_unwrap::<Packet>(incoming);
 return 42;
}
"#,
    );
    let (mut allocator, trace) = channel_allocator(usize::MAX);
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(trace.borrow().live.is_empty());
}

#[test]
fn deep_receiver_queue_retirement_is_iterative_and_keeps_each_parent_live() {
    let program = prepare(
        r#"
enum Message { Empty, Prior(core::Receiver<Message>) }
async fn main()->i64 {
 let mut seed:core::ChannelSetup<Message>=core::result_unwrap::<core::ChannelSetup<Message>,core::ChannelError>(core::channel_new::<Message>(core::allocator(),1));
 let mut seed_sender:core::Sender<Message>=core::channel_sender::<Message>(&seed);
 let mut seed_receiver:core::Receiver<Message>=core::channel_receiver::<Message>(seed);
 let mut seeded:core::SendResult<Message>=await core::channel_send::<Message>(&seed_sender,Message::Empty);
 let mut previous:core::Option<core::Receiver<Message>>=core::Option::Some(seed_receiver);
 let mut count:i64=0;
 while count<512 {
  let mut setup:core::ChannelSetup<Message>=core::result_unwrap::<core::ChannelSetup<Message>,core::ChannelError>(core::channel_new::<Message>(core::allocator(),1));
  let mut sender:core::Sender<Message>=core::channel_sender::<Message>(&setup);
  let mut receiver:core::Receiver<Message>=core::channel_receiver::<Message>(setup);
  let mut old:core::Receiver<Message>=core::option_unwrap::<core::Receiver<Message>>(previous);
  let mut sent:core::SendResult<Message>=await core::channel_send::<Message>(&sender,Message::Prior(old));
  previous=core::Option::Some(receiver);
  count=count+1;
 }
 return 42;
}
"#,
    );
    let (mut allocator, trace) = channel_allocator(usize::MAX);
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        r::Limits {
            channels: lang_runtime::channel::Limits {
                channels: 513,
                slots: 513,
                handles: 513,
                waiters: 0,
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(result.tasks.live, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(trace.borrow().live.is_empty());
}

#[test]
fn source_channel_quotas_refuse_one_below_exact_without_losing_custody() {
    let program = prepare(include_str!("fixtures/task_channels.lang"));
    let exact = lang_runtime::channel::Limits {
        channels: 1,
        slots: 1,
        handles: 1,
        waiters: 2,
    };
    for kind in 0..=4 {
        let mut limits = exact;
        match kind {
            0 => limits.channels = 0,
            1 => limits.slots = 0,
            2 => limits.handles = 0,
            3 => limits.waiters = 1,
            _ => {}
        }
        let (mut allocator, trace) = channel_allocator(usize::MAX);
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                channels: limits,
                ..Default::default()
            },
        );
        assert_eq!(allocator.stats().live_allocations, 0, "quota {kind}");
        assert!(trace.borrow().live.is_empty(), "quota {kind}");
        if kind == 4 {
            assert_eq!(result.unwrap().value, 42);
        } else {
            let failure = result.err().unwrap();
            use lang_runtime::channel::{Error as C, managed::Error as G};
            assert!(
                matches!(
                    (kind, &failure.reason),
                    (
                        0,
                        r::Reason::Channel(G::Protocol(C::ChannelLimit { limit: 0 }))
                    ) | (
                        1,
                        r::Reason::Channel(G::Protocol(C::SlotLimit { limit: 0 }))
                    ) | (
                        2,
                        r::Reason::Channel(G::Protocol(C::HandleLimit { limit: 0 }))
                    ) | (
                        3,
                        r::Reason::Channel(G::Protocol(C::WaiterLimit { limit: 1 }))
                    )
                ),
                "quota {kind}: {failure:?}"
            );
            assert_eq!(failure.tasks.live, 0);
        }
    }
}

#[test]
fn channel_captures_and_argument_loan_slots_are_reused_across_loop_activations() {
    let program = prepare(
        r#"
async fn main()->i64 {
 let mut setup:core::ChannelSetup<i64>=core::result_unwrap::<core::ChannelSetup<i64>,core::ChannelError>(core::channel_new::<i64>(core::allocator(),1));
 let mut sender:core::Sender<i64>=core::channel_sender::<i64>(&setup);
 let mut receiver:core::Receiver<i64>=core::channel_receiver::<i64>(setup);
 let mut count:i64=0;
 let mut total:i64=0;
 while count<42 {
  let mut sent:core::SendResult<i64>=await core::channel_send::<i64>(&sender,1);
  let mut received:core::Option<i64>=await core::channel_receive::<i64>(&mut receiver);
  total=total+core::option_unwrap::<i64>(received);
  count=count+1;
 }
 let mut sealed:core::Unit=core::channel_seal::<i64>(&mut receiver);
 let mut last:core::Option<i64>=await core::channel_receive::<i64>(&mut receiver);
 match last {core::Option::None=>{return total;},core::Option::Some(_)=>{return -1;},}
}
"#,
    );
    let (mut allocator, trace) = channel_allocator(usize::MAX);
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(trace.borrow().live.is_empty());
}

#[test]
fn invalid_capacity_is_an_ordinary_source_error_before_queue_storage_creation() {
    for capacity in [0, -1] {
        let program = prepare(&format!(
            "fn main()->i64{{match core::channel_new::<i64>(core::allocator(),{capacity}){{core::Result::Err(core::ChannelError::InvalidCapacity)=>{{return 42;}},core::Result::Err(_)=>{{return -1;}},core::Result::Ok(_)=>{{return -2;}},}}}}"
        ));
        let (mut allocator, trace) = channel_allocator(usize::MAX);
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
        assert_eq!(result.value, 42);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
    }
}

#[test]
fn advanced_channel_control_prefixes_preserve_all_owners() {
    for &(name, text) in MANAGED_CHANNEL_FIXTURES {
        let program = prepare(text);
        let (mut allocator, trace) = channel_allocator(usize::MAX);
        let baseline = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 4096,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(baseline.value, 42, "{name}");
        assert!(trace.borrow().live.is_empty(), "{name}");
        for (kind, total) in [
            (0, baseline.usage.fuel),
            (1, baseline.usage.service_work),
            (2, baseline.usage.peak_live_cells as u64),
            (3, baseline.events.len() as u64),
        ] {
            let first = u64::from(kind == 3); // Zero disables trace recording.
            for cap in first..=total {
                let (mut allocator, trace) = channel_allocator(usize::MAX);
                let control = e::Control::with_limits(e::Limits {
                    fuel: (kind == 0).then_some(cap),
                    live_cells: (kind == 2).then_some(cap as usize),
                    ..Default::default()
                })
                .and_service_work((kind == 1).then_some(cap));
                let result = lang_vm::unit::tasks::execute_entry(
                    program.verified(),
                    &mut allocator,
                    control,
                    r::Limits {
                        trace_events: if kind == 3 { cap as usize } else { 0 },
                        ..Default::default()
                    },
                );
                assert_eq!(
                    allocator.stats().live_allocations,
                    0,
                    "{name} control {kind} cap {cap}/{total}"
                );
                assert!(
                    trace.borrow().live.is_empty(),
                    "{name} control {kind} cap {cap}/{total}"
                );
                if cap == total {
                    assert_eq!(result.unwrap().value, 42, "{name} control {kind}");
                } else {
                    let failure = result.err().unwrap();
                    assert_eq!(failure.tasks.live, 0, "{name} {failure:?}");
                    match kind {
                        0=>assert!(matches!(failure.reason,r::Reason::Control(e::Failure::FuelExhausted)),"{name} {cap}: {failure:?}"),
                        1=>assert!(matches!(failure.reason,r::Reason::Control(e::Failure::ServiceWorkLimit{..})|r::Reason::Execution(lang_mir::unit::ownership::failure::ExecutionFailure::ServiceWorkLimit{..})),"{name} {cap}: {failure:?}"),
                        2=>{},
                        _=>assert!(matches!(failure.reason,r::Reason::TraceLimit),"{name} {cap}: {failure:?}"),
                    }
                }
            }
        }
    }
}

#[test]
fn every_advanced_provider_refusal_and_interruption_position_drains_the_inventory() {
    for &(name, text) in MANAGED_CHANNEL_FIXTURES {
        let program = prepare(text);
        let (mut allocator, trace) = channel_allocator(usize::MAX);
        let baseline = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
        assert_eq!(baseline.value, 42, "{name}");
        let calls = trace.borrow().calls;
        assert!(trace.borrow().live.is_empty());
        for at in 1..=calls {
            for interrupt in [false, true] {
                let (mut allocator, trace) =
                    channel_allocator(if interrupt { usize::MAX } else { at });
                let flag = std::rc::Rc::new(e::InterruptionFlag::new());
                if interrupt {
                    trace.borrow_mut().interrupt = Some((at, flag.clone()));
                }
                let result = lang_vm::unit::tasks::execute_entry(
                    program.verified(),
                    &mut allocator,
                    e::Control::with_limits(Default::default())
                        .and_interruption(&flag)
                        .and_service_work(Some(baseline.usage.service_work)),
                    Default::default(),
                );
                let failure = result.err().unwrap_or_else(|| {
                    panic!("{name}: refusal {at}/{calls} interrupt={interrupt}")
                });
                if interrupt {
                    assert!(
                        matches!(failure.reason, r::Reason::Control(e::Failure::Interrupted)),
                        "{name} {at}: {failure:?}"
                    );
                }
                assert_eq!(failure.tasks.live, 0, "{name} {at}");
                assert_eq!(
                    allocator.stats().live_allocations,
                    0,
                    "{name} {at}: {failure:?}"
                );
                assert!(
                    trace.borrow().live.is_empty(),
                    "{name} {at}: {:?}",
                    trace.borrow().live
                );
            }
        }
    }
}

#[test]
fn cancelled_mixed_messages_retire_unpublished_before_queued_payloads() {
    let program = prepare(include_str!("fixtures/task_channel_mixed_cancel.lang"));
    let (mut allocator, trace) = channel_allocator(usize::MAX);
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(trace.borrow().buffers, [122, 22, 120, 20]);
    assert!(trace.borrow().live.is_empty());
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn mixed_channel_cleanup_continues_after_panic_and_report_truncation() {
    for primary in [false, true] {
        let text = include_str!("fixtures/task_channel_mixed.lang");
        let text = if primary {
            text.replace("return result;", "return core::panic::<i64>(91);")
        } else {
            text.to_owned()
        };
        let program = prepare(&text);
        for records in [1, 8] {
            let (mut allocator, trace) = channel_allocator(usize::MAX);
            trace.borrow_mut().panic_buffers = vec![9, 42];
            let failure = lang_vm::unit::tasks::execute_entry(
                program.verified(),
                &mut allocator,
                e::Control::with_limits(Default::default()),
                r::Limits {
                    cleanup_failure_records: records,
                    ..Default::default()
                },
            )
            .err()
            .unwrap();
            if primary {
                assert!(
                    matches!(failure.reason, r::Reason::Panic(Some(91))),
                    "{failure:?}"
                );
            } else {
                assert!(matches!(failure.reason, r::Reason::Cleanup), "{failure:?}");
            }
            assert!(failure.poisoned);
            let report = failure.report.as_ref().unwrap();
            assert_eq!(report.cleanup_failures(), 2, "{failure:?}");
            assert!(report.primary().unwrap().span.is_some());
            if records == 1 {
                assert!(report.secondary().is_empty());
                assert!(report.truncated() > 0);
            }
            assert_eq!(trace.borrow().buffers, [9, 42]);
            assert!(trace.borrow().live.is_empty(), "{failure:?}");
            assert_eq!(allocator.stats().live_allocations, 0);
            assert_eq!(failure.tasks.live, 0);
        }
    }
}

#[test]
fn source_receiver_cycles_are_rejected_before_commit_and_again_after_wakeup() {
    for text in [
        include_str!("fixtures/task_channel_self_cycle.lang"),
        include_str!("fixtures/task_channel_reinspect.lang"),
    ] {
        let program = prepare(text);
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 2048,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result.value, 42);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn shared_weak_and_vec_held_receiver_payloads_use_one_mixed_retirement_drain() {
    let program = prepare(include_str!("fixtures/task_channel_mixed.lang"));
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let result = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value, 42);
    assert_eq!(result.tasks.live, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
}
