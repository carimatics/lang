use lang_runtime::allocation::Region as _;
use lang_runtime::{allocation as a, execution as e};
use std::{cell::RefCell, collections::BTreeSet, path::Path, rc::Rc};

fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("owned-task.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(
        analyzed.diagnostics.is_empty(),
        "{:?}",
        analyzed.diagnostics
    );
    let lang_semantics::analysis::Program::Tasks(p) = analyzed.result.unwrap() else {
        panic!("task profile")
    };
    p
}

#[test]
fn owned_source_task_suspends_cancels_and_retires_all_storage() {
    let program = prepare(include_str!("fixtures/task_owned.lang"));
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
        assert_eq!(result.value, 7);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

const OWNED_CALLS: &str = r#"
struct Pair { first:core::Bytes, second:core::Bytes }
fn bytes(value:i64)->core::Bytes {
    let mut result:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));
    let mut pushed:core::Unit=core::result_unwrap::<core::Unit,core::BufferError>(core::bytes_push(result,value));
    return result;
}
fn carry(value:Pair)->Pair {return value;}
async fn main()->i64 {
    let mut pair:Pair=carry(Pair {second:bytes(2),first:bytes(1)});
    yield;
    pair.first=bytes(3);
    return core::bytes_len(pair.first)+core::bytes_len(pair.second);
}
"#;

#[test]
fn owned_aggregate_calls_return_and_replace_without_copying_owners() {
    let program = prepare(OWNED_CALLS);
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
        assert_eq!(result.value, 2);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn every_owned_execution_fuel_prefix_retires_remaining_owners() {
    use lang_runtime::cleanup::{ExecutionFailure, FailureKind};
    for source in [
        OWNED_CALLS,
        include_str!("fixtures/task_owned_buffers.lang"),
    ] {
        let program = prepare(source);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let baseline = native
            .execute(
                &mut allocator,
                e::Control::with_limits(Default::default()),
                Default::default(),
            )
            .unwrap();
        assert_eq!(allocator.stats().live_allocations, 0);
        for prefix in 0..baseline.usage.fuel {
            for compiled in [false, true] {
                let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
                let control = e::Control::with_limits(e::Limits {
                    fuel: Some(prefix),
                    ..Default::default()
                });
                let result = if compiled {
                    native.execute(&mut allocator, control, Default::default())
                } else {
                    lang_vm::unit::tasks::execute_entry(
                        program.verified(),
                        &mut allocator,
                        control,
                        Default::default(),
                    )
                };
                let failure = result.err().unwrap();
                assert!(
                    matches!(
                        failure.reason,
                        lang_runtime::task::runner::Reason::Control(e::Failure::FuelExhausted)
                    ),
                    "prefix {prefix}, native {compiled}: {failure:?}"
                );
                assert_eq!(
                    failure
                        .report
                        .as_ref()
                        .and_then(|r| r.primary())
                        .map(|r| r.kind),
                    Some(FailureKind::Execution(ExecutionFailure::FuelLimit))
                );
                assert_eq!(failure.tasks.live, 0, "prefix {prefix}, native {compiled}");
                assert_eq!(
                    allocator.stats().live_allocations,
                    0,
                    "prefix {prefix}, native {compiled}"
                );
            }
        }
    }
}

#[test]
fn owned_sums_and_flat_buffer_services_survive_task_suspension() {
    let program = prepare(include_str!("fixtures/task_owned_buffers.lang"));
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
        assert_eq!(result.value, 71);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[derive(Default)]
struct Allocations {
    active: BTreeSet<usize>,
    buffers: Vec<u8>,
    panic_buffers: bool,
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
    trace: Rc<RefCell<Allocations>>,
}
impl a::Provider for Provider {
    type Region = Region;
    fn allocate(&mut self, request: a::Request) -> Option<Region> {
        self.at += 1;
        if self.at == self.fail {
            return None;
        }
        let storage = a::Storage::try_new(request).ok()?;
        assert!(self.trace.borrow_mut().active.insert(self.at));
        Some(Region {
            id: self.at,
            storage,
        })
    }
    fn deallocate(&mut self, region: Region, _: a::Request) {
        let mut trace = self.trace.borrow_mut();
        assert!(trace.active.remove(&region.id), "duplicate retirement");
        // Observe the existing private flat-buffer header, not a source ABI.
        let data = region.storage.bytes();
        if data.len() > 24
            && data[..8] == 1u64.to_le_bytes()
            && data[8..16] == ((data.len() - 24) as u64).to_le_bytes()
            && data[16..24] == [0; 8]
        {
            trace.buffers.push(data[24]);
            assert!(
                !trace.panic_buffers,
                "injected trusted buffer cleanup panic"
            );
        }
    }
}
fn tracked(fail: usize) -> (a::Allocator<Provider>, Rc<RefCell<Allocations>>) {
    let trace = Rc::new(RefCell::new(Allocations::default()));
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

#[test]
fn aggregate_field_replacement_retires_once_in_actual_initialization_order() {
    let program = prepare(OWNED_CALLS);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = tracked(usize::MAX);
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
        assert_eq!(result.value, 2);
        assert!(trace.borrow().active.is_empty());
        assert_eq!(trace.borrow().buffers, [1, 3, 2]);
    }
}

#[test]
fn every_owned_provider_refusal_retires_admitted_and_unpublished_storage_once() {
    for source in [
        OWNED_CALLS,
        include_str!("fixtures/task_owned_buffers.lang"),
    ] {
        let program = prepare(source);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let (mut baseline, _) = tracked(usize::MAX);
        native
            .execute(
                &mut baseline,
                e::Control::with_limits(Default::default()),
                Default::default(),
            )
            .unwrap();
        let requests = baseline.provider().unwrap().at;
        for fail in 1..=requests {
            for compiled in [false, true] {
                let (mut allocator, trace) = tracked(fail);
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
                };
                let failure = result.err().unwrap();
                assert_eq!(
                    failure.tasks.live, 0,
                    "request {fail}, native {compiled}: {failure:?}"
                );
                assert_eq!(
                    allocator.stats().live_allocations,
                    0,
                    "request {fail}, native {compiled}"
                );
                assert!(
                    trace.borrow().active.is_empty(),
                    "request {fail}, native {compiled}"
                );
            }
        }
    }
}

#[test]
fn owned_child_failure_report_keeps_origin_and_retires_cancelled_siblings() {
    let source = OWNED_CALLS
        .split("async fn main")
        .next()
        .unwrap()
        .to_owned()
        + r#"
async fn bad()->i64 {let mut b:core::Bytes=bytes(65);yield;return core::panic::<i64>(91);}
async fn sibling()->i64 {let mut b:core::Bytes=bytes(66);yield;cancel_check;return core::bytes_len(b);}
async fn main()->i64 {
    let mut b:core::Bytes=bytes(67);
    let mut failed:core::Task<i64>=spawn bad();
    let mut other:core::Task<i64>=spawn sibling();
    let mut result:core::Result<i64,core::TaskError>=await failed;
    return 0;
}
"#;
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = tracked(usize::MAX);
        let control = e::Control::with_limits(Default::default());
        let failure = if compiled {
            native.execute(&mut allocator, control, Default::default())
        } else {
            lang_vm::unit::tasks::execute_entry(
                program.verified(),
                &mut allocator,
                control,
                Default::default(),
            )
        }
        .err()
        .unwrap();
        assert!(
            matches!(
                failure.reason,
                lang_runtime::task::runner::Reason::Panic(Some(91))
            ),
            "{failure:?}"
        );
        let report = failure.report.as_ref().unwrap();
        assert_eq!(report.primary().unwrap().span, failure.span);
        assert_eq!(
            report.primary().unwrap().kind,
            lang_runtime::cleanup::FailureKind::Execution(
                lang_runtime::cleanup::ExecutionFailure::Panic
            )
        );
        assert!(!failure.poisoned);
        assert_eq!(failure.tasks.live, 0);
        assert!(trace.borrow().active.is_empty());
        assert_eq!(trace.borrow().buffers, [66, 65, 67]);
    }
}
#[test]
fn cleanup_panics_do_not_retry_or_skip_owners_after_report_truncation() {
    // Deliberately violate the trusted provider's no-unwind contract only
    // inside contained leaf cleanup. This does not authorize host unwinding.
    let program = prepare(OWNED_CALLS);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = tracked(usize::MAX);
        trace.borrow_mut().panic_buffers = true;
        let control = e::Control::with_limits(Default::default());
        let limits = lang_runtime::task::runner::Limits {
            cleanup_failure_records: 1,
            ..Default::default()
        };
        let failure = if compiled {
            native.execute(&mut allocator, control, limits)
        } else {
            lang_vm::unit::tasks::execute_entry(program.verified(), &mut allocator, control, limits)
        }
        .err()
        .unwrap();
        assert!(
            matches!(failure.reason, lang_runtime::task::runner::Reason::Cleanup),
            "{failure:?}"
        );
        assert!(failure.poisoned);
        let report = failure.report.as_ref().unwrap();
        assert_eq!(
            report.primary().unwrap().kind,
            lang_runtime::cleanup::FailureKind::CleanupPanic
        );
        assert_eq!(report.cleanup_failures(), 3);
        assert_eq!(report.truncated(), 2);
        assert_eq!(report.quarantined(), 3);
        assert_eq!(failure.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().active.is_empty());
        assert_eq!(trace.borrow().buffers, [1, 3, 2]);
    }
}

#[test]
fn cancelled_child_keeps_cancelled_join_and_exposes_secondary_cleanup_report() {
    let program = prepare(include_str!("fixtures/task_owned.lang"));
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = tracked(usize::MAX);
        trace.borrow_mut().panic_buffers = true;
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
        assert_eq!(result.value, 7);
        assert!(result.poisoned);
        let report = result.report.as_ref().unwrap();
        assert_eq!(
            report.primary().unwrap().kind,
            lang_runtime::cleanup::FailureKind::CleanupPanic
        );
        assert_eq!(report.cleanup_failures(), 1);
        assert_eq!(report.quarantined(), 1);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().active.is_empty());
        assert_eq!(trace.borrow().buffers, [65]);
    }
}
#[test]
fn owned_utf8_domain_error_remains_a_successful_nested_task_result() {
    let source = OWNED_CALLS
        .split("async fn main")
        .next()
        .unwrap()
        .to_owned()
        + r#"
async fn worker()->core::Result<i64,core::Utf8Error> {
    let mut b:core::Bytes=bytes(255);
    yield;
    let mut error:core::Utf8Error=core::result_unwrap_err::<core::Text,core::Utf8Error>(core::text_from_utf8(b));
    return core::Result::Err(error);
}
async fn main()->i64 {
    let mut result:core::Result<i64,core::Utf8Error>=core::result_unwrap::<core::Result<i64,core::Utf8Error>,core::TaskError>(await worker());
    match result {
        core::Result::Ok(value)=>{return value;},
        core::Result::Err(core::Utf8Error::Invalid(offset))=>{return offset+42;},
    }
}
"#;
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = tracked(usize::MAX);
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
        assert!(result.report.is_none());
        assert!(!result.poisoned);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().active.is_empty());
        assert_eq!(trace.borrow().buffers, [255]);
    }
}

#[test]
fn unconnected_owned_task_profiles_refuse_before_provider_effects() {
    let source = "async fn main()->i64 {let mut values:core::Vec<i64>=core::result_unwrap::<core::Vec<i64>,core::AllocError>(core::vec_new::<i64>(core::allocator()));yield;return core::vec_len::<i64>(values);}";
    let program = prepare(source);
    assert!(matches!(
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default()),
        Err(lang_codegen_clif::tasks::Error::Unsupported { .. })
    ));
    let (mut allocator, trace) = tracked(usize::MAX);
    let failure = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        e::Control::with_limits(Default::default()),
        Default::default(),
    )
    .err()
    .unwrap();
    assert!(
        matches!(
            failure.reason,
            lang_runtime::task::runner::Reason::Unsupported
        ),
        "{failure:?}"
    );
    assert_eq!(allocator.stats().requests, 0);
    assert!(trace.borrow().active.is_empty());
}
