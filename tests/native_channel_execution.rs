use lang_runtime::{allocation as a, execution as e, task::runner as r};
use lang_semantics::analysis::{Program, TaskProgram};
use std::path::Path;

#[path = "support/channel_fixtures.rs"]
mod channel_fixtures;
use channel_fixtures::{MANAGED_CHANNEL_FIXTURES, channel_allocator};

fn prepare(text: &str) -> Box<TaskProgram> {
    let output = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("native-channels.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    let Program::Tasks(program) = output.result.unwrap() else {
        panic!("task profile")
    };
    program
}

#[test]
fn source_channel_fixtures_match_vm_on_generated_native_continuations() {
    for (name, text) in [
        ("backpressure", include_str!("fixtures/task_channels.lang")),
        (
            "self-cycle",
            include_str!("fixtures/task_channel_self_cycle.lang"),
        ),
        (
            "reinspection",
            include_str!("fixtures/task_channel_reinspect.lang"),
        ),
        ("mixed", include_str!("fixtures/task_channel_mixed.lang")),
        (
            "shared-sender",
            include_str!("fixtures/task_channel_shared_sender.lang"),
        ),
        (
            "mixed-cancel",
            include_str!("fixtures/task_channel_mixed_cancel.lang"),
        ),
        ("text", include_str!("fixtures/task_channel_text.lang")),
    ] {
        let program = prepare(text);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let mut expected = None;
        for compiled in [false, true] {
            let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
            let control = e::Control::with_limits(Default::default());
            let limits = r::Limits {
                trace_events: 4096,
                ..Default::default()
            };
            let result = if compiled {
                native.execute(&mut allocator, control, limits)
            } else {
                lang_vm::unit::tasks::execute_entry(
                    program.verified(),
                    &mut allocator,
                    control,
                    limits,
                )
            }
            .unwrap();
            assert_eq!(result.value, 42, "{name} native={compiled}");
            assert_eq!(result.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            if let Some((usage, tasks, events)) = &expected {
                assert_eq!(&result.usage, usage, "{name}");
                assert_eq!(&result.tasks, tasks, "{name}");
                assert_eq!(&result.events, events, "{name}");
            } else {
                expected = Some((result.usage, result.tasks, result.events));
            }
        }
    }
}

#[expect(
    clippy::result_large_err,
    reason = "Compare complete native channel failure and cleanup evidence."
)]
fn execute<P: a::Provider>(
    program: &TaskProgram,
    native: &lang_codegen_clif::tasks::CompiledProgram<'_>,
    compiled: bool,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
    limits: r::Limits,
) -> Result<r::Outcome, r::Failure> {
    if compiled {
        native.execute(allocator, control, limits)
    } else {
        lang_vm::unit::tasks::execute_entry(program.verified(), allocator, control, limits)
    }
}
fn compare(
    left: &Result<r::Outcome, r::Failure>,
    right: &Result<r::Outcome, r::Failure>,
    name: &str,
) {
    match (left, right) {
        (Ok(left), Ok(right)) => {
            assert_eq!(left.value, right.value, "{name}");
            assert_eq!(left.usage, right.usage, "{name}");
            assert_eq!(left.tasks, right.tasks, "{name}");
            assert_eq!(left.allocations, right.allocations, "{name}");
            assert_eq!(left.events, right.events, "{name}");
            assert_eq!(left.poisoned, right.poisoned, "{name}");
            assert_eq!(
                format!("{:?}", left.report),
                format!("{:?}", right.report),
                "{name}"
            );
        }
        (Err(left), Err(right)) => {
            assert_eq!(
                format!("{:?}", left.reason),
                format!("{:?}", right.reason),
                "{name}"
            );
            assert_eq!(left.function, right.function, "{name}");
            assert_eq!(left.span, right.span, "{name}");
            assert_eq!(left.usage, right.usage, "{name}");
            assert_eq!(left.tasks, right.tasks, "{name}");
            assert_eq!(left.events, right.events, "{name}");
            assert_eq!(left.poisoned, right.poisoned, "{name}");
            assert_eq!(
                format!("{:?}", left.report),
                format!("{:?}", right.report),
                "{name}"
            );
        }
        _ => panic!("{name}: VM/native success/failure disagrees"),
    }
}
fn fixtures() -> impl Iterator<Item = (&'static str, &'static str)> {
    std::iter::once(("backpressure", include_str!("fixtures/task_channels.lang")))
        .chain(MANAGED_CHANNEL_FIXTURES.iter().copied())
}

#[test]
fn native_channel_control_prefixes_match_complete_vm_failure_evidence() {
    for (name, text) in fixtures() {
        let program = prepare(text);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let (mut allocator, trace) = channel_allocator(usize::MAX);
        let baseline = execute(
            &program,
            &native,
            false,
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 4096,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(baseline.value, 42, "{name}");
        assert!(trace.borrow().live.is_empty());
        for (kind, total) in [
            (0, baseline.usage.fuel),
            (1, baseline.usage.service_work),
            (2, baseline.usage.peak_live_cells as u64),
            (3, baseline.events.len() as u64),
        ] {
            for cap in u64::from(kind == 3)..=total {
                let label = format!("{name} control {kind} cap {cap}/{total}");
                let mut expected = None;
                for compiled in [false, true] {
                    let (mut allocator, trace) = channel_allocator(usize::MAX);
                    let control = e::Control::with_limits(e::Limits {
                        fuel: (kind == 0).then_some(cap),
                        live_cells: (kind == 2).then_some(cap as usize),
                        ..Default::default()
                    })
                    .and_service_work((kind == 1).then_some(cap));
                    let result = execute(
                        &program,
                        &native,
                        compiled,
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
                        "{label} native={compiled}"
                    );
                    assert!(trace.borrow().live.is_empty(), "{label} native={compiled}");
                    if cap < total {
                        assert!(result.is_err(), "{label}");
                    } else {
                        assert_eq!(result.as_ref().unwrap().value, 42, "{label}");
                    }
                    if let Some(expected) = &expected {
                        compare(expected, &result, &label);
                    } else {
                        expected = Some(result);
                    }
                }
            }
        }
    }
}

#[test]
fn native_channel_provider_and_interruption_positions_match_vm_inventory_and_failure() {
    for (name, text) in fixtures() {
        let program = prepare(text);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let (mut allocator, trace) = channel_allocator(usize::MAX);
        let baseline = execute(
            &program,
            &native,
            false,
            &mut allocator,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
        let calls = trace.borrow().calls;
        for at in 1..=calls {
            for interrupt in [false, true] {
                let label = format!("{name} allocation {at}/{calls} interrupt={interrupt}");
                let mut expected = None;
                let mut expected_buffers = None;
                for compiled in [false, true] {
                    let (mut allocator, trace) =
                        channel_allocator(if interrupt { usize::MAX } else { at });
                    let flag = std::rc::Rc::new(e::InterruptionFlag::new());
                    if interrupt {
                        trace.borrow_mut().interrupt = Some((at, flag.clone()));
                    }
                    let result = execute(
                        &program,
                        &native,
                        compiled,
                        &mut allocator,
                        e::Control::with_limits(Default::default())
                            .and_interruption(&flag)
                            .and_service_work(Some(baseline.usage.service_work)),
                        Default::default(),
                    );
                    assert!(result.is_err(), "{label}");
                    assert_eq!(
                        allocator.stats().live_allocations,
                        0,
                        "{label} native={compiled}"
                    );
                    assert!(trace.borrow().live.is_empty(), "{label} native={compiled}");
                    let buffers = trace.borrow().buffers.clone();
                    if let Some(previous) = &expected_buffers {
                        assert_eq!(previous, &buffers, "{label}");
                    } else {
                        expected_buffers = Some(buffers);
                    }
                    if let Some(previous) = &expected {
                        compare(previous, &result, &label);
                    } else {
                        expected = Some(result);
                    }
                }
            }
        }
    }
}

#[test]
fn native_channel_cleanup_panic_primary_and_truncated_reports_match_vm() {
    for primary in [false, true] {
        let text = include_str!("fixtures/task_channel_mixed.lang");
        let text = if primary {
            text.replace("return result;", "return core::panic::<i64>(91);")
        } else {
            text.to_owned()
        };
        let program = prepare(&text);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        for records in [1, 8] {
            let mut expected = None;
            for compiled in [false, true] {
                let (mut allocator, trace) = channel_allocator(usize::MAX);
                trace.borrow_mut().panic_buffers = vec![9, 42];
                let result = execute(
                    &program,
                    &native,
                    compiled,
                    &mut allocator,
                    e::Control::with_limits(Default::default()),
                    r::Limits {
                        cleanup_failure_records: records,
                        ..Default::default()
                    },
                );
                let failure = result.as_ref().err().unwrap();
                assert!(failure.poisoned);
                assert_eq!(failure.report.as_ref().unwrap().cleanup_failures(), 2);
                assert_eq!(trace.borrow().buffers, [9, 42]);
                assert!(trace.borrow().live.is_empty());
                assert_eq!(allocator.stats().live_allocations, 0);
                if let Some(previous) = &expected {
                    compare(previous, &result, "cleanup panic");
                } else {
                    expected = Some(result);
                }
            }
        }
    }
}

#[test]
fn native_channel_deadlock_and_wrong_send_case_preserve_the_vm_primary_origin() {
    let producer = include_str!("fixtures/task_channels.lang")
        .split("async fn consumer")
        .next()
        .unwrap();
    let deadlock = format!(
        "{producer}async fn main()->i64{{let mut setup:core::ChannelSetup<core::Bytes>=core::result_unwrap::<core::ChannelSetup<core::Bytes>,core::ChannelError>(core::channel_new::<core::Bytes>(core::allocator(),1));let mut sender:core::Sender<core::Bytes>=core::channel_sender::<core::Bytes>(&setup);let mut receiver:core::Receiver<core::Bytes>=core::channel_receiver::<core::Bytes>(setup);let mut sending:core::Task<i64>=spawn producer(sender);return core::result_unwrap::<i64,core::TaskError>(await sending);}}"
    );
    for (name, text) in [
        ("deadlock", deadlock.as_str()),
        (
            "sent-unsent",
            "fn main()->i64{let mut sent:core::SendResult<i64>=core::SendResult::Sent;return core::channel_unsent::<i64>(sent);}",
        ),
    ] {
        let program = prepare(text);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = channel_allocator(usize::MAX);
            let result = execute(
                &program,
                &native,
                compiled,
                &mut allocator,
                e::Control::with_limits(Default::default()),
                r::Limits {
                    trace_events: 2048,
                    ..Default::default()
                },
            );
            assert!(result.is_err());
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
            if let Some(previous) = &expected {
                compare(previous, &result, name);
            } else {
                expected = Some(result);
            }
        }
    }
}

#[test]
fn native_seal_wakes_a_parked_sender_and_returns_its_uncommitted_owner() {
    let program = prepare(
        r#"
fn bytes(value:i64)->core::Bytes {
 let mut data:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
 let mut pushed:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(data,value));
 return data;
}
async fn producer(sender:core::Sender<core::Bytes>)->i64 {
 let mut first:core::SendResult<core::Bytes>=await core::channel_send::<core::Bytes>(&sender,bytes(20));
 let mut second:core::SendResult<core::Bytes>=await core::channel_send::<core::Bytes>(&sender,bytes(22));
 let mut returned:core::Bytes=core::channel_unsent::<core::Bytes>(second);
 return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(returned,0));
}
async fn main()->i64 {
 let mut setup:core::ChannelSetup<core::Bytes>=core::result_unwrap::<core::ChannelSetup<core::Bytes>,core::ChannelError>(core::channel_new::<core::Bytes>(core::allocator(),1));
 let mut sender:core::Sender<core::Bytes>=core::channel_sender::<core::Bytes>(&setup);
 let mut receiver:core::Receiver<core::Bytes>=core::channel_receiver::<core::Bytes>(setup);
 let mut sending:core::Task<i64>=spawn producer(sender);
 yield;
 let mut sealed:core::Unit=core::channel_seal::<core::Bytes>(&mut receiver);
 let mut received:core::Option<core::Bytes>=await core::channel_receive::<core::Bytes>(&mut receiver);
 let mut value:core::Bytes=core::option_unwrap::<core::Bytes>(received);
 let mut first:i64=core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(value,0));
 let mut second:i64=core::result_unwrap::<i64,core::TaskError>(await sending);
 let mut ended:core::Option<core::Bytes>=await core::channel_receive::<core::Bytes>(&mut receiver);
 match ended {core::Option::None=>{return first+second;},core::Option::Some(_)=>{return -1;},}
}
"#,
    );
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let mut expected = None;
    for compiled in [false, true] {
        let (mut allocator, trace) = channel_allocator(usize::MAX);
        let result = execute(
            &program,
            &native,
            compiled,
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 2048,
                ..Default::default()
            },
        );
        assert_eq!(result.as_ref().unwrap().value, 42);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(allocator.stats().live_allocations, 0);
        if let Some(previous) = &expected {
            compare(previous, &result, "seal parked sender");
        } else {
            expected = Some(result);
        }
    }
}

#[test]
fn native_protocol_quota_and_invalid_capacity_boundaries_match_vm() {
    let program = prepare(include_str!("fixtures/task_channels.lang"));
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let exact = lang_runtime::channel::Limits {
        channels: 1,
        slots: 1,
        handles: 1,
        waiters: 2,
    };
    for kind in 0..=4 {
        let mut channels = exact;
        match kind {
            0 => channels.channels = 0,
            1 => channels.slots = 0,
            2 => channels.handles = 0,
            3 => channels.waiters = 1,
            _ => {}
        }
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = channel_allocator(usize::MAX);
            let result = execute(
                &program,
                &native,
                compiled,
                &mut allocator,
                e::Control::with_limits(Default::default()),
                r::Limits {
                    channels,
                    ..Default::default()
                },
            );
            assert_eq!(result.is_ok(), kind == 4);
            assert!(trace.borrow().live.is_empty());
            assert_eq!(allocator.stats().live_allocations, 0);
            if let Some(previous) = &expected {
                compare(previous, &result, "protocol quota");
            } else {
                expected = Some(result);
            }
        }
    }
    for capacity in [0, -1] {
        let program = prepare(&format!(
            "fn main()->i64{{match core::channel_new::<i64>(core::allocator(),{capacity}){{core::Result::Err(core::ChannelError::InvalidCapacity)=>{{return 42;}},core::Result::Err(_)=>{{return -1;}},core::Result::Ok(_)=>{{return -2;}},}}}}"
        ));
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = channel_allocator(usize::MAX);
            let result = execute(
                &program,
                &native,
                compiled,
                &mut allocator,
                e::Control::with_limits(Default::default()),
                Default::default(),
            );
            assert_eq!(result.as_ref().unwrap().value, 42);
            assert!(trace.borrow().live.is_empty());
            assert_eq!(allocator.stats().live_allocations, 0);
            if let Some(previous) = &expected {
                compare(previous, &result, "invalid capacity");
            } else {
                expected = Some(result);
            }
        }
    }
}

#[test]
fn native_channel_frame_task_heap_and_allocation_caps_match_vm_at_exact_boundaries() {
    for (name, text) in fixtures() {
        let program = prepare(text);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let mut base = a::Allocator::new(a::System, Default::default()).unwrap();
        let baseline = execute(
            &program,
            &native,
            false,
            &mut base,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
        for (kind, exact) in [
            (0, baseline.usage.peak_frames as u64),
            (1, baseline.tasks.peak_live as u64),
            (2, baseline.tasks.admitted as u64),
            (3, base.stats().peak_live_bytes),
            (4, base.stats().peak_live_allocations as u64),
        ] {
            for enough in [false, true] {
                let bound = exact - u64::from(!enough);
                let label = format!("{name} resource {kind} cap {bound}/{exact}");
                let mut expected = None;
                for compiled in [false, true] {
                    let mut allocation = a::Limits::default();
                    if kind == 3 {
                        allocation.live_bytes = bound;
                    }
                    if kind == 4 {
                        allocation.live_allocations = bound as usize;
                    }
                    let mut allocator = a::Allocator::new(a::System, allocation).unwrap();
                    let control = e::Control::with_limits(e::Limits {
                        frames: (kind == 0).then_some(bound as usize),
                        ..Default::default()
                    })
                    .and_service_work(Some(baseline.usage.service_work));
                    let mut limits = r::Limits::default();
                    if kind == 1 {
                        limits.tasks.live_tasks = bound as usize;
                    }
                    if kind == 2 {
                        limits.tasks.total_tasks = bound as usize;
                    }
                    let result =
                        execute(&program, &native, compiled, &mut allocator, control, limits);
                    assert_eq!(result.is_ok(), enough, "{label} native={compiled}");
                    assert_eq!(
                        allocator.stats().live_allocations,
                        0,
                        "{label} native={compiled}"
                    );
                    if let Some(previous) = &expected {
                        compare(previous, &result, &label);
                    } else {
                        expected = Some(result);
                    }
                }
            }
        }
    }
}
