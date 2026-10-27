use lang_runtime::{allocation as a, execution as e};
use std::path::Path;
fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let analyzed = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("task-owned-integration.lang"),
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
    let lang_semantics::analysis::Program::Tasks(program) = analyzed.result.unwrap() else {
        panic!("task profile")
    };
    program
}
const SOURCE: &str = include_str!("fixtures/task_owned_integrated.lang");
#[test]
fn combined_owned_families_patterns_and_descendants_execute_and_retire_once() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let result = execute(&program, &native, compiled, &mut allocator, control()).unwrap();
        assert_eq!(result.value, 42);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, [77, 40, 41, 0, 0, 11, 9, 8]);
    }
}
#[allow(
    clippy::result_large_err,
    reason = "Compare full task failure evidence."
)]
fn execute<P: a::Provider>(
    program: &lang_semantics::analysis::TaskProgram,
    native: &lang_codegen_clif::tasks::CompiledProgram<'_>,
    compiled: bool,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
) -> Result<lang_runtime::task::runner::Outcome, lang_runtime::task::runner::Failure> {
    execute_with(
        program,
        native,
        compiled,
        allocator,
        control,
        Default::default(),
    )
}
#[allow(
    clippy::result_large_err,
    reason = "Compare full task failure evidence."
)]
fn execute_with<P: a::Provider>(
    program: &lang_semantics::analysis::TaskProgram,
    native: &lang_codegen_clif::tasks::CompiledProgram<'_>,
    compiled: bool,
    allocator: &mut a::Allocator<P>,
    control: e::Control<'_>,
    limits: lang_runtime::task::runner::Limits,
) -> Result<lang_runtime::task::runner::Outcome, lang_runtime::task::runner::Failure> {
    if compiled {
        native.execute(allocator, control, limits)
    } else {
        lang_vm::unit::tasks::execute_entry(program.verified(), allocator, control, limits)
    }
}

fn compare_reports(
    expected: Option<&lang_runtime::cleanup::Report>,
    actual: Option<&lang_runtime::cleanup::Report>,
) {
    assert_eq!(
        expected.map(|r| (
            r.primary(),
            r.secondary(),
            r.truncated(),
            r.quarantined(),
            r.cleanup_failures()
        )),
        actual.map(|r| (
            r.primary(),
            r.secondary(),
            r.truncated(),
            r.quarantined(),
            r.cleanup_failures()
        ))
    );
}
fn compare_failures(
    expected: &lang_runtime::task::runner::Failure,
    actual: &lang_runtime::task::runner::Failure,
) {
    assert_eq!(
        format!("{:?}", expected.reason),
        format!("{:?}", actual.reason)
    );
    assert_eq!(expected.function, actual.function);
    assert_eq!(expected.span, actual.span);
    assert_eq!(expected.usage, actual.usage);
    assert_eq!(expected.tasks, actual.tasks);
    assert_eq!(expected.events, actual.events);
    assert_eq!(expected.poisoned, actual.poisoned);
    compare_reports(expected.report.as_ref(), actual.report.as_ref());
}
const ORDER: &[u8] = &[77, 40, 41, 0, 0, 11, 9, 8];

#[test]
fn every_service_work_prefix_preserves_numeric_refusal_and_mixed_owner_retirement() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let (mut base, _) = allocator(usize::MAX);
    let work = execute(&program, &native, true, &mut base, control())
        .unwrap()
        .usage
        .service_work;
    assert!(work > 0);
    for prefix in 0..work {
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            let failure = execute(
                &program,
                &native,
                compiled,
                &mut allocator,
                control().and_service_work(Some(prefix)),
            )
            .err()
            .unwrap();
            let (limit, used, requested) = match failure.reason {
                lang_runtime::task::runner::Reason::Control(e::Failure::ServiceWorkLimit {
                    limit,
                    used,
                    requested,
                })
                | lang_runtime::task::runner::Reason::Execution(
                    lang_mir::unit::ownership::failure::ExecutionFailure::ServiceWorkLimit {
                        limit,
                        used,
                        requested,
                    },
                ) => (limit, used, requested),
                _ => panic!("prefix {prefix}, native {compiled}: {failure:?}"),
            };
            assert_eq!(limit, prefix);
            assert_eq!(used, failure.usage.service_work);
            assert!(used <= limit && used + requested > limit);
            assert!(!failure.poisoned);
            assert_eq!(failure.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
            if let Some((previous, buffers, stats)) = &expected {
                compare_failures(previous, &failure);
                assert_eq!(buffers, &trace.borrow().buffers);
                assert_eq!(*stats, allocator.stats());
            } else {
                expected = Some((failure, trace.borrow().buffers.clone(), allocator.stats()));
            }
        }
    }
}

#[test]
fn service_refusal_remains_primary_when_cleanup_panics_and_report_truncates() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let (mut base, _) = allocator(usize::MAX);
    let work = execute(&program, &native, true, &mut base, control())
        .unwrap()
        .usage
        .service_work;
    for records in [1, 32] {
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            trace.borrow_mut().panic_buffers = vec![77, 40, 41, 0, 11, 9, 8];
            let failure = execute_with(
                &program,
                &native,
                compiled,
                &mut allocator,
                control().and_service_work(Some(work - 1)),
                lang_runtime::task::runner::Limits {
                    cleanup_failure_records: records,
                    trace_events: 1024,
                    ..Default::default()
                },
            )
            .err()
            .unwrap();
            assert!(
                matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Control(
                        e::Failure::ServiceWorkLimit { .. }
                    ) | lang_runtime::task::runner::Reason::Execution(
                        lang_mir::unit::ownership::failure::ExecutionFailure::ServiceWorkLimit { .. }
                    )
                ),
                "{failure:?}"
            );
            assert!(failure.poisoned);
            assert_eq!(failure.tasks.live, 0);
            let report = failure.report.as_ref().unwrap();
            assert!(report.cleanup_failures() > 0);
            if records == 1 {
                assert!(report.secondary().is_empty());
                assert!(report.truncated() > 0);
            }
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
fn every_combined_fuel_prefix_matches_failures_reports_and_exact_retirement() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let (mut base, _) = allocator(usize::MAX);
    let baseline = execute(&program, &native, true, &mut base, control())
        .unwrap()
        .usage;
    for prefix in 0..baseline.fuel {
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            let control = e::Control::with_limits(e::Limits {
                fuel: Some(prefix),
                ..Default::default()
            })
            .and_service_work(Some(baseline.service_work));
            let failure = execute(&program, &native, compiled, &mut allocator, control)
                .err()
                .unwrap();
            assert!(
                matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Control(e::Failure::FuelExhausted)
                ),
                "prefix {prefix}, native {compiled}: {failure:?}"
            );
            assert!(!failure.poisoned);
            assert_eq!(failure.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(
                trace.borrow().live.is_empty(),
                "prefix {prefix}, native {compiled}"
            );
            if let Some((previous, buffers, stats)) = &expected {
                compare_failures(previous, &failure);
                assert_eq!(buffers, &trace.borrow().buffers);
                assert_eq!(*stats, allocator.stats());
            } else {
                expected = Some((failure, trace.borrow().buffers.clone(), allocator.stats()));
            }
        }
    }
}

#[test]
fn every_combined_provider_refusal_matches_reports_and_retires_every_obligation() {
    let program = prepare(SOURCE);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let (mut base, _) = allocator(usize::MAX);
    let work = execute(&program, &native, true, &mut base, control())
        .unwrap()
        .usage
        .service_work;
    let attempts = base.provider().unwrap().at;
    for fail in 1..=attempts {
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(fail);
            let failure = execute(
                &program,
                &native,
                compiled,
                &mut allocator,
                control().and_service_work(Some(work)),
            )
            .err()
            .unwrap();
            assert_eq!(
                failure.tasks.live, 0,
                "request {fail}, native {compiled}: {failure:?}"
            );
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(
                trace.borrow().live.is_empty(),
                "request {fail}, native {compiled}"
            );
            if let Some((previous, buffers, stats)) = &expected {
                compare_failures(previous, &failure);
                assert_eq!(buffers, &trace.borrow().buffers);
                assert_eq!(*stats, allocator.stats());
            } else {
                expected = Some((failure, trace.borrow().buffers.clone(), allocator.stats()));
            }
        }
    }
}

#[test]
fn source_panic_inside_the_combined_arena_keeps_its_origin_and_drains_descendants() {
    let source = SOURCE.replace("await scenario(false)", "await scenario(true)");
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        let failure = execute(&program, &native, compiled, &mut allocator, control())
            .err()
            .unwrap();
        assert!(
            matches!(
                failure.reason,
                lang_runtime::task::runner::Reason::Panic(Some(-42))
            ),
            "{failure:?}"
        );
        assert_eq!(
            failure.span.unwrap().start as usize,
            source.find("core::panic::<i64>(-42)").unwrap()
        );
        assert_eq!(
            failure.report.as_ref().unwrap().primary().unwrap().span,
            failure.span
        );
        assert!(!failure.poisoned);
        assert_eq!(failure.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, ORDER);
    }
}

#[test]
fn cleanup_failure_after_saving_a_return_discards_escrow_and_drains_all_families() {
    let source = early_return_source();
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        trace.borrow_mut().panic_buffers = vec![40, 0];
        let limits = lang_runtime::task::runner::Limits {
            cleanup_failure_records: 1,
            ..Default::default()
        };
        let failure = execute_with(
            &program,
            &native,
            compiled,
            &mut allocator,
            control(),
            limits,
        )
        .err()
        .unwrap();
        assert!(
            matches!(failure.reason, lang_runtime::task::runner::Reason::Cleanup),
            "{failure:?}"
        );
        assert!(failure.poisoned);
        assert_eq!(failure.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, ORDER);
        let report = failure.report.unwrap();
        assert_eq!(
            report.primary().unwrap().kind,
            lang_runtime::cleanup::FailureKind::CleanupPanic
        );
        assert_eq!(report.cleanup_failures(), 3);
        assert_eq!(report.quarantined(), 3);
        assert_eq!(report.truncated(), 2);
    }
}

#[test]
fn cancelling_a_parent_joins_multiple_owned_generations_before_its_owners() {
    let definitions = SOURCE.split("async fn main").next().unwrap();
    let mut saw_leaf = false;
    let mut saw_graph = false;
    for yields in 1..=6 {
        let source = format!(
            "{definitions}async fn controller()->i64{{let mut local:Parcel=Parcel::Pair(item(90),item(91));task_scope{{let mut nested:core::Task<i64>=spawn scenario(false);yield;yield;yield;yield;return 0;}}}}async fn main()->i64{{let mut local:core::Bytes=bytes(99);let mut child:core::Task<i64>=spawn controller();{}cancel child;let mut joined:core::Result<i64,core::TaskError>=await child;match joined{{core::Result::Ok(_)=>{{return 42;}},core::Result::Err(core::TaskError::Cancelled)=>{{return 42;}},}}}}",
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
            let limits = lang_runtime::task::runner::Limits {
                trace_events: 512,
                ..Default::default()
            };
            let outcome = execute_with(
                &program,
                &native,
                compiled,
                &mut allocator,
                control(),
                limits,
            )
            .unwrap();
            assert_eq!(outcome.value, 42);
            assert_eq!(outcome.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(!outcome.poisoned);
            assert!(trace.borrow().live.is_empty());
            let buffers = trace.borrow().buffers.clone();
            saw_leaf |= buffers.contains(&77);
            saw_graph |= buffers.contains(&40);
            assert_eq!(buffers.last(), Some(&99));
            let parent = buffers.iter().position(|v| *v == 91).unwrap();
            assert_eq!(&buffers[parent..], &[91, 90, 99]);
            if let Some(leaf) = buffers.iter().position(|v| *v == 77) {
                assert!(leaf < parent);
            }
            if let Some(graph) = buffers.iter().position(|v| *v == 40) {
                assert!(graph < parent);
            }
            if let Some((events, previous, usage, tasks, stats)) = &expected {
                assert_eq!(events, &outcome.events);
                assert_eq!(previous, &buffers);
                assert_eq!(*usage, outcome.usage);
                assert_eq!(*tasks, outcome.tasks);
                assert_eq!(*stats, allocator.stats());
            } else {
                expected = Some((
                    outcome.events,
                    buffers,
                    outcome.usage,
                    outcome.tasks,
                    allocator.stats(),
                ));
            }
        }
    }
    assert!(saw_leaf && saw_graph);
}

#[test]
fn cancelled_combined_child_retains_typed_join_but_withholds_poisoned_root_result() {
    let definitions = SOURCE.split("async fn main").next().unwrap();
    let source = format!(
        "{definitions}async fn main()->i64{{let mut child:core::Task<i64>=spawn scenario(false);yield;yield;cancel child;let mut result:core::Result<i64,core::TaskError>=await child;match result{{core::Result::Ok(_)=>{{return -1;}},core::Result::Err(core::TaskError::Cancelled)=>{{return 42;}},}}}}"
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        trace.borrow_mut().panic_buffers = vec![40, 0];
        let result = execute_with(
            &program,
            &native,
            compiled,
            &mut allocator,
            control(),
            lang_runtime::task::runner::Limits {
                trace_events: 1024,
                ..Default::default()
            },
        )
        .err()
        .expect("poisoned cleanup withholds the recovered root result");
        assert!(matches!(
            result.reason,
            lang_runtime::task::runner::Reason::Cleanup
        ));
        assert!(result.events.iter().any(|event| {
            event.task == 1
                && event.kind
                    == lang_runtime::task::runner::EventKind::Complete(
                        lang_runtime::task::Outcome::Cancelled,
                    )
        }));
        assert!(result.events.iter().any(|event| {
            event.task == 0
                && event.kind
                    == lang_runtime::task::runner::EventKind::Complete(
                        lang_runtime::task::Outcome::Success,
                    )
        }));
        assert!(result.poisoned);
        assert_eq!(result.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, ORDER);
        let report = result.report.unwrap();
        assert_eq!(
            report.primary().unwrap().kind,
            lang_runtime::cleanup::FailureKind::CleanupPanic
        );
        assert_eq!(report.cleanup_failures(), 3);
        assert_eq!(report.quarantined(), 3);
    }
}

#[test]
fn explicitly_awaited_failure_precedes_an_earlier_completed_sibling_and_cleanup_errors() {
    let definitions = SOURCE.split("async fn main").next().unwrap();
    let source = format!(
        "{definitions}async fn sibling()->i64{{let mut owned:core::Bytes=bytes(88);yield;return core::panic::<i64>(91);}}async fn main()->i64{{let mut first:core::Task<i64>=spawn scenario(true);let mut other:core::Task<i64>=spawn sibling();let mut result:core::Result<i64,core::TaskError>=await first;return 0;}}"
    );
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for records in [1, 64] {
        let mut expected = None;
        for compiled in [false, true] {
            let (mut allocator, trace) = allocator(usize::MAX);
            trace.borrow_mut().panic_buffers = vec![77, 40, 0, 88];
            let limits = lang_runtime::task::runner::Limits {
                trace_events: 512,
                cleanup_failure_records: records,
                ..Default::default()
            };
            let failure = execute_with(
                &program,
                &native,
                compiled,
                &mut allocator,
                control(),
                limits,
            )
            .err()
            .unwrap();
            assert!(
                matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Panic(Some(-42))
                ),
                "{failure:?}"
            );
            assert_eq!(
                failure.span.unwrap().start as usize,
                source.find("core::panic::<i64>(-42)").unwrap()
            );
            assert!(failure.poisoned);
            assert_eq!(failure.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            assert!(trace.borrow().live.is_empty());
            let report = failure.report.as_ref().unwrap();
            assert_eq!(report.primary().unwrap().span, failure.span);
            assert_eq!(
                report.primary().unwrap().kind,
                lang_runtime::cleanup::FailureKind::Execution(
                    lang_runtime::cleanup::ExecutionFailure::Panic
                )
            );
            if records == 1 {
                assert!(report.secondary().is_empty());
                assert_eq!(report.truncated(), 6);
            } else {
                use lang_runtime::cleanup::{ExecutionFailure as F, FailureKind as K};
                let kinds: Vec<_> = report.secondary().iter().map(|r| r.kind).collect();
                let mut ordered = vec![K::CleanupPanic; 3];
                for event in &failure.events {
                    if matches!(
                        event.kind,
                        lang_runtime::task::runner::EventKind::Complete(_)
                    ) {
                        match event.task {
                            2 => ordered.extend([K::Execution(F::Panic), K::CleanupPanic]),
                            3 => ordered.push(K::CleanupPanic),
                            _ => {}
                        }
                    }
                }
                assert_eq!(kinds, ordered);
                let sibling = report
                    .secondary()
                    .iter()
                    .find(|r| r.kind == K::Execution(F::Panic))
                    .unwrap();
                assert_eq!(
                    sibling.span.unwrap().start as usize,
                    source.find("core::panic::<i64>(91)").unwrap()
                );
            }
            use lang_runtime::task::runner::EventKind;
            let completed = |id| {
                failure
                    .events
                    .iter()
                    .position(|e| e.task == id && matches!(e.kind, EventKind::Complete(_)))
                    .unwrap()
            };
            assert!(completed(2) < completed(1));
            assert_eq!(report.cleanup_failures(), 5);
            assert_eq!(report.quarantined(), 5);
            if let Some((previous, buffers)) = &expected {
                compare_failures(previous, &failure);
                assert_eq!(buffers, &trace.borrow().buffers);
            } else {
                expected = Some((failure, trace.borrow().buffers.clone()));
            }
        }
    }
}
#[derive(Default)]
struct Trace {
    panic_buffers: Vec<u8>,
    live: std::collections::BTreeSet<usize>,
    buffers: Vec<u8>,
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
        Some(Region {
            id: self.at,
            storage,
        })
    }
    fn deallocate(&mut self, region: Region, _: a::Request) {
        use a::Region as _;
        let mut trace = self.trace.borrow_mut();
        assert!(trace.live.remove(&region.id), "duplicate callback");
        let bytes = region.storage.bytes();
        if bytes.len() > 24
            && bytes[..8] == 1u64.to_le_bytes()
            && bytes[8..16] == ((bytes.len() - 24) as u64).to_le_bytes()
            && bytes[16..24] == [0; 8]
        {
            trace.buffers.push(bytes[24]);
            assert!(
                !trace.panic_buffers.contains(&bytes[24]),
                "injected contained leaf cleanup panic"
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
fn control() -> e::Control<'static> {
    e::Control::with_limits(Default::default())
}

fn early_return_source() -> String {
    let line = SOURCE
        .lines()
        .find(|l| l.contains("result = score(store, query)"))
        .unwrap();
    let early = SOURCE.replace(line, &line.replace("result =", "return"));
    let end = early.find("    match core::weak_upgrade").unwrap();
    let main = early.find("async fn main").unwrap();
    format!("{} }}\n{}", &early[..end], &early[main..])
}

#[test]
fn failed_child_discards_an_escrowed_return_before_parent_owner_cleanup_failures() {
    let source = early_return_source();
    let needle = "    return 0;";
    assert_eq!(source.matches(needle).count(), 1, "unique tail return");
    let source = source.replace(needle, "    return core::panic::<i64>(91);");
    assert!(source.contains("core::panic::<i64>(91)"));
    let program = prepare(&source);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    for compiled in [false, true] {
        let (mut allocator, trace) = allocator(usize::MAX);
        trace.borrow_mut().panic_buffers = vec![40, 0];
        let failure = execute(&program, &native, compiled, &mut allocator, control())
            .err()
            .unwrap();
        assert!(
            matches!(
                failure.reason,
                lang_runtime::task::runner::Reason::Panic(Some(91))
            ),
            "{failure:?}"
        );
        assert_eq!(
            failure.span.unwrap().start as usize,
            source.find("core::panic::<i64>(91)").unwrap()
        );
        assert_eq!(
            program
                .verified()
                .typed()
                .places()
                .declarations()
                .functions()[failure.function.unwrap().0 as usize]
                .name,
            "tail"
        );
        let report = failure.report.as_ref().unwrap();
        assert_eq!(report.primary().unwrap().span, failure.span);
        assert_eq!(
            report.primary().unwrap().kind,
            lang_runtime::cleanup::FailureKind::Execution(
                lang_runtime::cleanup::ExecutionFailure::Panic
            )
        );
        assert_eq!(
            report
                .secondary()
                .iter()
                .map(|r| r.kind)
                .collect::<Vec<_>>(),
            [lang_runtime::cleanup::FailureKind::CleanupPanic; 3]
        );
        assert_eq!(report.cleanup_failures(), 3);
        assert_eq!(report.quarantined(), 3);
        assert!(failure.poisoned);
        assert_eq!(failure.tasks.live, 0);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(trace.borrow().live.is_empty());
        assert_eq!(trace.borrow().buffers, ORDER);
    }
}
