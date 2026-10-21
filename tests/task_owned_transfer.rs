use lang_runtime::{allocation as a, execution as e, task::runner as r};
use std::path::Path;
fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let result = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("task-transfer.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let lang_semantics::analysis::Program::Tasks(program) = result.result.unwrap() else {
        panic!("task proof")
    };
    program
}
const SOURCE: &str = include_str!("fixtures/task_owned_transfer.lang");
const ALL: &str = include_str!("fixtures/task_owned_transfer_all.lang");

#[allow(
    clippy::result_large_err,
    reason = "Compare complete ownership failure evidence."
)]
fn execute<P: a::Provider>(
    program: &lang_semantics::analysis::TaskProgram,
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
fn compare_failures(left: &r::Failure, right: &r::Failure) {
    assert_eq!(format!("{:?}", left.reason), format!("{:?}", right.reason));
    assert_eq!(left.function, right.function);
    assert_eq!(left.span, right.span);
    assert_eq!(left.usage, right.usage);
    assert_eq!(left.tasks, right.tasks);
    assert_eq!(left.events, right.events);
    assert_eq!(left.poisoned, right.poisoned);
    let report = |r: Option<&lang_runtime::cleanup::Report>| r.map(|r| format!("{:?}", r));
    assert_eq!(report(left.report.as_ref()), report(right.report.as_ref()));
}
fn compare_outcomes(left: &r::Outcome, right: &r::Outcome) {
    assert_eq!(left.value, right.value);
    assert_eq!(left.usage, right.usage);
    assert_eq!(left.tasks, right.tasks);
    assert_eq!(left.allocations, right.allocations);
    assert_eq!(left.events, right.events);
    assert_eq!(left.poisoned, right.poisoned);
    assert_eq!(format!("{:?}", left.report), format!("{:?}", right.report));
}

#[test]
fn every_owned_transfer_fuel_work_and_provider_prefix_matches_and_retires_all_owners() {
    let program = prepare(ALL);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let limits = r::Limits {
        trace_events: 1024,
        ..Default::default()
    };
    let (mut baseline, _) = allocator(usize::MAX);
    let usage = execute(
        &program,
        &native,
        false,
        &mut baseline,
        e::Control::with_limits(Default::default()),
        limits,
    )
    .unwrap()
    .usage;
    let allocations = baseline.provider().unwrap().at;
    for (kind, count) in [
        (0, usage.fuel),
        (1, usage.service_work),
        (2, allocations as u64),
    ] {
        for prefix in 0..count {
            let mut expected = None;
            for compiled in [false, true] {
                let fail = if kind == 2 {
                    prefix as usize + 1
                } else {
                    usize::MAX
                };
                let (mut allocator, trace) = allocator(fail);
                let control = e::Control::with_limits(e::Limits {
                    fuel: if kind == 0 {
                        Some(prefix)
                    } else {
                        Some(usage.fuel)
                    },
                    ..Default::default()
                })
                .and_service_work(Some(if kind == 1 {
                    prefix
                } else {
                    usage.service_work
                }));
                let failure = execute(&program, &native, compiled, &mut allocator, control, limits)
                    .err()
                    .unwrap();
                assert_eq!(failure.tasks.live, 0);
                assert_eq!(allocator.stats().live_allocations, 0);
                assert!(trace.borrow().live.is_empty());
                assert!(!failure.poisoned, "{failure:?}");
                if kind == 0 {
                    assert!(
                        matches!(
                            failure.reason,
                            r::Reason::Control(e::Failure::FuelExhausted)
                        ),
                        "{failure:?}"
                    );
                }
                if kind == 1 {
                    let (limit,used,requested)=match failure.reason{r::Reason::Control(e::Failure::ServiceWorkLimit{limit,used,requested})|r::Reason::Execution(lang_mir::unit::ownership::failure::ExecutionFailure::ServiceWorkLimit{limit,used,requested})=>(limit,used,requested),_=>panic!("{failure:?}")};
                    assert_eq!(limit, prefix);
                    assert_eq!(used, failure.usage.service_work);
                    assert!(used <= limit && used + requested > limit);
                }
                if let Some((previous, buffers, stats)) = &expected {
                    compare_failures(previous, &failure);
                    assert_eq!(*buffers, trace.borrow().buffers);
                    assert_eq!(*stats, allocator.stats());
                } else {
                    expected = Some((failure, trace.borrow().buffers.clone(), allocator.stats()));
                }
            }
        }
    }
}

#[test]
fn cancellation_before_and_after_owned_publication_has_one_typed_join_and_cleanup() {
    for yields in 0..8 {
        let prefix = SOURCE.split("async fn main").next().unwrap();
        let source = format!(
            "{prefix}async fn main()->i64{{let mut child:core::Task<core::Bytes>=spawn worker(bytes(42));{}cancel child;let mut result:core::Result<core::Bytes,core::TaskError>=await child;match result{{core::Result::Ok(output)=>{{return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(output,0));}},core::Result::Err(core::TaskError::Cancelled)=>{{return 7;}},}}}}",
            "yield;".repeat(yields)
        );
        let program = prepare(&source);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            let output = execute(
                &program,
                &native,
                compiled,
                &mut allocator,
                e::Control::with_limits(Default::default()).and_service_work(Some(2)),
                r::Limits {
                    trace_events: 512,
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(matches!(output.value, 7 | 42));
            if yields == 0 {
                assert_eq!(output.value, 7);
            }
            if yields >= 4 {
                assert_eq!(output.value, 42);
            }
            assert!(!output.poisoned);
            assert_eq!(trace.borrow().buffers, [42]);
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
            if let Some(previous) = &expected {
                compare_outcomes(previous, &output);
            } else {
                expected = Some(output);
            }
        }
    }
}

#[test]
fn nested_generations_and_nested_owned_result_cases_preserve_each_ticket() {
    let relay = "async fn relay(value:Parcel)->Parcel{let mut child:core::Task<Parcel>=spawn forward(value);yield;return core::result_unwrap::<Parcel,core::TaskError>(await child);}";
    let nested = ALL
        .replace("async fn main", &format!("{relay}async fn main"))
        .replace("spawn forward(input)", "spawn relay(input)")
        .replace("spawn forward(intermediate)", "spawn relay(intermediate)");
    let prefix = ALL.split("async fn main").next().unwrap();
    let choose = "async fn choose(left:core::Bytes,right:core::Text,failed:bool)->core::Result<core::Bytes,core::Text>{yield;if failed{return core::Result::Err(right);}return core::Result::Ok(left);}";
    let success = format!(
        "{prefix}{choose}async fn main()->i64{{let mut result:core::Result<core::Bytes,core::Text>=core::result_unwrap::<core::Result<core::Bytes,core::Text>,core::TaskError>(await choose(bytes(42),word(),false));let mut output:core::Bytes=core::result_unwrap::<core::Bytes,core::Text>(result);return core::result_unwrap::<i64,core::BoundsError>(core::bytes_get(output,0));}}"
    );
    let failed = format!(
        "{prefix}{choose}async fn main()->i64{{let mut result:core::Result<core::Bytes,core::Text>=core::result_unwrap::<core::Result<core::Bytes,core::Text>,core::TaskError>(await choose(bytes(42),word(),true));let mut output:core::Text=core::result_unwrap_err::<core::Bytes,core::Text>(result);return core::text_len(output);}}"
    );
    for (source, value) in [(nested, 42), (success, 42), (failed, 1)] {
        let program = prepare(&source);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            let output = execute(
                &program,
                &native,
                compiled,
                &mut allocator,
                e::Control::with_limits(Default::default()),
                r::Limits {
                    trace_events: 1024,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(output.value, value);
            assert!(!output.poisoned);
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
            if let Some(previous) = &expected {
                compare_outcomes(previous, &output);
            } else {
                expected = Some(output);
            }
        }
    }
}

#[test]
fn expired_weak_result_keeps_identity_after_its_child_retires_the_strong_owner() {
    let prefix = SOURCE.split("async fn worker").next().unwrap();
    let source = format!(
        "{prefix}async fn make()->weak core::Bytes{{let mut strong:shared core::Bytes=core::result_unwrap::<shared core::Bytes,core::OwnershipError>(core::shared_new::<core::Bytes>(core::allocator(),bytes(42)));return core::result_unwrap::<weak core::Bytes,core::OwnershipError>(core::weak_new::<core::Bytes>(strong));}}async fn main()->i64{{let mut observer:weak core::Bytes=core::result_unwrap::<weak core::Bytes,core::TaskError>(await make());match core::weak_upgrade::<core::Bytes>(observer){{core::Result::Ok(_)=>{{return -1;}},core::Result::Err(core::OwnershipError::ExpiredWeak)=>{{return 7;}},core::Result::Err(_)=>{{return -2;}},}}}}"
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let output = execute(
            &program,
            &native,
            compiled,
            &mut allocator,
            e::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
        assert_eq!(output.value, 7);
        assert_eq!(trace.borrow().buffers, [42]);
        assert!(!output.poisoned);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
    }
}

#[test]
fn unpublished_owned_return_is_discarded_after_an_owner_cleanup_failure() {
    let prefix = SOURCE.split("async fn worker").next().unwrap();
    let source = format!(
        "{prefix}async fn worker()->core::Bytes{{let mut kept:core::Bytes=bytes(9);let mut output:core::Bytes=bytes(42);return output;}}async fn main()->i64{{let mut output:core::Bytes=core::result_unwrap::<core::Bytes,core::TaskError>(await worker());return core::bytes_len(output);}}"
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let mut expected = None;
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        trace.borrow_mut().panic_buffers = vec![9, 42];
        let failure = execute(
            &program,
            &native,
            compiled,
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 512,
                ..Default::default()
            },
        )
        .err()
        .unwrap();
        assert!(matches!(failure.reason, r::Reason::Cleanup), "{failure:?}");
        assert_eq!(trace.borrow().buffers, [9, 42]);
        assert!(failure.poisoned);
        assert_eq!(failure.tasks.live, 0);
        assert_eq!(failure.report.as_ref().unwrap().cleanup_failures(), 2);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        if let Some(previous) = &expected {
            compare_failures(previous, &failure);
        } else {
            expected = Some(failure);
        }
    }
}

#[test]
fn parent_panic_keeps_precedence_over_published_result_and_local_cleanup_failures() {
    let prefix = SOURCE.split("async fn main").next().unwrap();
    let source = format!(
        "{prefix}async fn main()->i64{{let mut kept:core::Bytes=bytes(9);let mut child:core::Task<core::Bytes>=spawn worker(bytes(1));yield;yield;yield;yield;return core::panic::<i64>(91);}}"
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for records in [1, 32] {
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            trace.borrow_mut().panic_buffers = vec![1, 9];
            let failure = execute(
                &program,
                &native,
                compiled,
                &mut allocator,
                e::Control::with_limits(Default::default()).and_service_work(Some(2)),
                r::Limits {
                    trace_events: 512,
                    cleanup_failure_records: records,
                    ..Default::default()
                },
            )
            .err()
            .unwrap();
            assert!(
                matches!(failure.reason, r::Reason::Panic(Some(91))),
                "{failure:?}"
            );
            assert_eq!(trace.borrow().buffers, [1, 9]);
            assert!(failure.poisoned);
            let report = failure.report.as_ref().unwrap();
            assert!(matches!(
                report.primary().unwrap().kind,
                lang_runtime::cleanup::FailureKind::Execution(
                    lang_runtime::cleanup::ExecutionFailure::Panic
                )
            ));
            assert_eq!(report.cleanup_failures(), 2);
            if records == 1 {
                assert!(report.secondary().is_empty());
                assert_eq!(report.truncated(), 2);
            } else {
                assert_eq!(
                    report.secondary()[0].slot,
                    Some(lang_runtime::cleanup::Slot(1))
                );
            }
            assert_eq!(failure.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
            if let Some(previous) = &expected {
                compare_failures(previous, &failure);
            } else {
                expected = Some(failure);
            }
        }
    }
}

#[test]
fn cancelled_owned_join_keeps_suppressed_cleanup_poison_without_a_payload() {
    let prefix = SOURCE.split("async fn main").next().unwrap();
    let source = format!(
        "{prefix}async fn main()->i64{{let mut child:core::Task<core::Bytes>=spawn worker(bytes(42));cancel child;let mut result:core::Result<core::Bytes,core::TaskError>=await child;match result{{core::Result::Ok(_)=>{{return -1;}},core::Result::Err(core::TaskError::Cancelled)=>{{return 7;}},}}}}"
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let mut expected = None;
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        trace.borrow_mut().panic_buffers = vec![42];
        let output = execute(
            &program,
            &native,
            compiled,
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 512,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(output.value, 7);
        assert!(output.poisoned);
        assert_eq!(trace.borrow().buffers, [42]);
        assert_eq!(output.report.as_ref().unwrap().cleanup_failures(), 1);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        if let Some(previous) = &expected {
            compare_outcomes(previous, &output);
        } else {
            expected = Some(output);
        }
    }
}

#[test]
fn exact_and_one_below_frame_cell_task_and_heap_limits_preserve_owned_retirement() {
    let program = prepare(ALL);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
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
        (1, baseline.usage.peak_live_cells as u64),
        (2, baseline.tasks.peak_live as u64),
        (3, baseline.tasks.admitted as u64),
        (4, base.stats().peak_live_bytes),
    ] {
        for enough in [false, true] {
            let bound = exact - u64::from(!enough);
            let mut expected = None;
            for compiled in [false, true] {
                let mut allocation = a::Limits::default();
                if kind == 4 {
                    allocation.live_bytes = bound;
                }
                let mut allocator = a::Allocator::new(a::System, allocation).unwrap();
                let mut control = e::Limits::default();
                if kind == 0 {
                    control.frames = Some(bound as usize);
                }
                if kind == 1 {
                    control.live_cells = Some(bound as usize);
                }
                let control = e::Control::with_limits(control)
                    .and_service_work(Some(baseline.usage.service_work));
                let mut limits = r::Limits::default();
                if kind == 2 {
                    limits.tasks.live_tasks = bound as usize;
                }
                if kind == 3 {
                    limits.tasks.total_tasks = bound as usize;
                }
                let result = execute(&program, &native, compiled, &mut allocator, control, limits);
                assert_eq!(allocator.stats().live_allocations, 0);
                if enough {
                    let result = result.unwrap();
                    assert_eq!(result.value, 42);
                    assert!(!result.poisoned);
                } else {
                    let failure = result.err().unwrap();
                    assert_eq!(failure.tasks.live, 0);
                    assert!(!failure.poisoned, "{failure:?}");
                    if let Some(previous) = &expected {
                        compare_failures(previous, &failure);
                    } else {
                        expected = Some(failure);
                    }
                }
            }
        }
    }
}

#[test]
fn interruption_at_every_provider_callback_drains_staged_and_published_owners() {
    let program = prepare(ALL);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let (mut base, _) = allocator(usize::MAX);
    let baseline = execute(
        &program,
        &native,
        false,
        &mut base,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    let attempts = base.provider().unwrap().at;
    for at in 1..=attempts {
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            let flag = std::rc::Rc::new(e::InterruptionFlag::new());
            trace.borrow_mut().interrupt = Some((at, flag.clone()));
            let control = e::Control::with_limits(Default::default())
                .and_interruption(&flag)
                .and_service_work(Some(baseline.usage.service_work));
            let failure = execute(
                &program,
                &native,
                compiled,
                &mut allocator,
                control,
                r::Limits {
                    trace_events: 1024,
                    ..Default::default()
                },
            )
            .err()
            .unwrap();
            assert!(
                matches!(failure.reason, r::Reason::Control(e::Failure::Interrupted)),
                "callback {at}: {failure:?}"
            );
            assert_eq!(failure.tasks.live, 0);
            assert!(!failure.poisoned);
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
            if let Some((previous, buffers)) = &expected {
                compare_failures(previous, &failure);
                assert_eq!(*buffers, trace.borrow().buffers);
            } else {
                expected = Some((failure, trace.borrow().buffers.clone()));
            }
        }
    }
}

#[test]
fn retained_owned_result_keeps_its_cell_charge_during_later_frame_admission() {
    let prefix = SOURCE.split("async fn worker").next().unwrap();
    let source = format!(
        "{prefix}async fn worker(value:i64)->core::Bytes{{return bytes(value);}}async fn main()->i64{{let mut first:core::Task<core::Bytes>=spawn worker(1);yield;yield;yield;yield;let mut second:core::Task<core::Bytes>=spawn worker(2);yield;yield;yield;yield;return 42;}}"
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let (mut base, _) = allocator(usize::MAX);
    let exact = execute(
        &program,
        &native,
        false,
        &mut base,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap()
    .usage
    .peak_live_cells;
    for enough in [false, true] {
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            let bound = exact - usize::from(!enough);
            let control = e::Control::with_limits(e::Limits {
                live_cells: Some(bound),
                ..Default::default()
            });
            let result = execute(
                &program,
                &native,
                compiled,
                &mut allocator,
                control,
                Default::default(),
            );
            if enough {
                let output = result.unwrap();
                assert_eq!(output.value, 42);
                assert_eq!(output.usage.peak_live_cells, exact);
                assert_eq!(trace.borrow().buffers, [2, 1]);
            } else {
                let failure = result.err().unwrap();
                assert!(
                    matches!(
                        failure.reason,
                        r::Reason::Execution(
                            lang_mir::unit::ownership::failure::ExecutionFailure::CellLimit
                        )
                    ),
                    "{failure:?}"
                );
                assert_eq!(trace.borrow().buffers, [1]);
                assert_eq!(failure.tasks.live, 0);
            }
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
        }
    }
}

#[test]
fn awaited_child_failure_precedes_cleanup_of_an_earlier_published_owned_result() {
    let prefix = SOURCE.split("async fn main").next().unwrap();
    let source = format!(
        "{prefix}async fn bad()->core::Bytes{{return core::panic::<core::Bytes>(91);}}async fn main()->i64{{let mut first:core::Task<core::Bytes>=spawn worker(bytes(1));yield;yield;yield;yield;let mut failed:core::Task<core::Bytes>=spawn bad();let mut output:core::Bytes=core::result_unwrap::<core::Bytes,core::TaskError>(await failed);return core::bytes_len(output);}}"
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let mut expected = None;
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        trace.borrow_mut().panic_buffers = vec![1];
        let failure = execute(
            &program,
            &native,
            compiled,
            &mut allocator,
            e::Control::with_limits(Default::default()),
            r::Limits {
                trace_events: 512,
                ..Default::default()
            },
        )
        .err()
        .unwrap();
        assert!(
            matches!(failure.reason, r::Reason::Panic(Some(91))),
            "{failure:?}"
        );
        assert_eq!(
            program
                .verified()
                .typed()
                .places()
                .declarations()
                .functions()[failure.function.unwrap().0 as usize]
                .name,
            "bad"
        );
        assert!(failure.poisoned);
        assert_eq!(trace.borrow().buffers, [1]);
        assert_eq!(failure.report.as_ref().unwrap().cleanup_failures(), 1);
        assert_eq!(failure.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        if let Some(previous) = &expected {
            compare_failures(previous, &failure);
        } else {
            expected = Some(failure);
        }
    }
}

#[derive(Default)]
struct Trace {
    buffers: Vec<u8>,
    panic_buffers: Vec<u8>,
    interrupt: Option<(usize, std::rc::Rc<e::InterruptionFlag>)>,
    live: std::collections::BTreeSet<usize>,
}
struct Region {
    id: usize,
    storage: a::Storage,
}
impl a::Region for Region {
    fn bytes(&self) -> &[u8] {
        self.storage.bytes()
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        self.storage.bytes_mut()
    }
}
struct Provider {
    at: usize,
    fail: usize,
    trace: std::rc::Rc<std::cell::RefCell<Trace>>,
}
impl a::Provider for Provider {
    type Region = Region;
    fn allocate(&mut self, request: a::Request) -> Option<Region> {
        self.at += 1;
        if self.at == self.fail {
            return None;
        }
        let storage = a::Storage::try_new(request).ok()?;
        assert!(self.trace.borrow_mut().live.insert(self.at));
        if let Some((at, flag)) = self.trace.borrow().interrupt.as_ref()
            && *at == self.at
        {
            flag.interrupt();
        }
        Some(Region {
            id: self.at,
            storage,
        })
    }
    fn deallocate(&mut self, region: Region, _: a::Request) {
        use a::Region as _;
        let mut trace = self.trace.borrow_mut();
        assert!(trace.live.remove(&region.id), "duplicate release");
        let data = region.storage.bytes();
        if data.len() == 25
            && data[..8] == 1u64.to_le_bytes()
            && data[8..16] == 1u64.to_le_bytes()
            && data[16..24] == [0; 8]
        {
            let byte = data[24];
            trace.buffers.push(byte);
            assert!(
                !trace.panic_buffers.contains(&byte),
                "contained buffer cleanup panic"
            );
        }
    }
}
fn allocator(
    fail: usize,
) -> (
    a::Allocator<Provider>,
    std::rc::Rc<std::cell::RefCell<Trace>>,
) {
    let trace = std::rc::Rc::new(std::cell::RefCell::new(Trace::default()));
    (
        a::Allocator::new(
            Provider {
                at: 0,
                fail,
                trace: trace.clone(),
            },
            Default::default(),
        )
        .unwrap(),
        trace,
    )
}

fn unclaimed() -> String {
    let prefix = SOURCE.split("async fn main").next().unwrap();
    format!(
        "{prefix}async fn main()->i64{{let mut kept:core::Bytes=bytes(9);let mut first:core::Task<core::Bytes>=spawn worker(bytes(1));let mut second:core::Task<core::Bytes>=spawn worker(bytes(2));yield;return 42;}}"
    )
}

#[test]
fn unclaimed_owned_results_retire_before_locals_in_reverse_publication_order() {
    let program = prepare(&unclaimed());
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let control = e::Control::with_limits(Default::default()).and_service_work(Some(3));
        let result = if compiled {
            native.execute(&mut allocator, control, Default::default())
        } else {
            lang_vm::unit::tasks::execute_entry(
                program.verified(),
                &mut allocator,
                control,
                Default::default(),
            )
        }
        .unwrap();
        assert_eq!(result.value, 42);
        assert!(!result.poisoned);
        assert_eq!(result.usage.service_work, 3);
        assert_eq!(trace.borrow().buffers, [2, 1, 9]);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
    }
}

#[test]
fn result_cleanup_failure_uses_the_scope_failure_edge_and_drains_other_results() {
    let program = prepare(&unclaimed());
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for records in [1, 32] {
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            trace.borrow_mut().panic_buffers = vec![1, 2, 9];
            let control = e::Control::with_limits(Default::default()).and_service_work(Some(3));
            let limits = r::Limits {
                cleanup_failure_records: records,
                trace_events: 512,
                ..Default::default()
            };
            let failure = if compiled {
                native.execute(&mut allocator, control, limits)
            } else {
                lang_vm::unit::tasks::execute_entry(
                    program.verified(),
                    &mut allocator,
                    control,
                    limits,
                )
            }
            .err()
            .unwrap();
            assert!(matches!(failure.reason, r::Reason::Cleanup), "{failure:?}");
            assert!(failure.poisoned);
            assert_eq!(failure.tasks.live, 0);
            assert_eq!(trace.borrow().buffers, [2, 1, 9]);
            let report = failure.report.as_ref().unwrap();
            assert_eq!(report.cleanup_failures(), 3);
            assert_eq!(report.quarantined(), 3);
            assert_eq!(
                report.primary().unwrap().slot,
                Some(lang_runtime::cleanup::Slot(2))
            );
            assert!(report.primary().unwrap().span.is_some());
            if records == 1 {
                assert!(report.secondary().is_empty());
                assert_eq!(report.truncated(), 2);
            }
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
        }
    }
}
#[test]
fn owned_buffer_argument_and_result_execute_on_vm_and_generated_native() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let control = e::Control::with_limits(Default::default());
        let result = if compiled {
            native.execute(
                &mut allocator,
                control,
                r::Limits {
                    trace_events: 512,
                    ..Default::default()
                },
            )
        } else {
            lang_vm::unit::tasks::execute_entry(
                program.verified(),
                &mut allocator,
                control,
                r::Limits {
                    trace_events: 512,
                    ..Default::default()
                },
            )
        }
        .unwrap();
        assert_eq!(result.value, 42);
        assert!(!result.poisoned);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn all_owned_families_move_through_multiple_tasks_and_whole_enum_results() {
    let program = prepare(include_str!("fixtures/task_owned_transfer_all.lang"));
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let control = e::Control::with_limits(Default::default());
        let result = if compiled {
            native.execute(&mut allocator, control, Default::default())
        } else {
            lang_vm::unit::tasks::execute_entry(
                program.verified(),
                &mut allocator,
                control,
                Default::default(),
            )
        }
        .unwrap();
        assert_eq!(result.value, 42);
        assert!(!result.poisoned);
        assert_eq!(result.tasks.admitted, 3);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}
