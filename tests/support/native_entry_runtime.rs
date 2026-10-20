//! Observe the concrete runtime; this file supplies no native Runtime implementation.
use super::*;
use lang_codegen_clif::owned::runtime as entry;
type Actual = Result<entry::Outcome, entry::EntryError>;

const SOURCES: [&str; 10] = [
    include_str!("../../examples/scripts/owned_bytes.lang"),
    include_str!("../../examples/scripts/utf8.lang"),
    include_str!("../../examples/scripts/vec_owned.lang"),
    include_str!("../../examples/scripts/map_owned.lang"),
    include_str!("../../examples/scripts/shared_owned.lang"),
    include_str!("../../examples/scripts/arena_owned.lang"),
    include_str!("../../examples/scripts/owned_calls.lang"),
    include_str!("../../examples/analysis/native_managed_text_work.lang"),
    include_str!("../../examples/analysis/explicit_panic_owned.lang"),
    include_str!("../../examples/analysis/explicit_drop_shared.lang"),
];
const COMBINED: &str = include_str!("../../examples/conformance/owned_pipeline.lang");

#[test]
fn production_owned_entry_combines_collections_shared_arena_calls_and_panic() {
    for panic in [false, true] {
        let text = if panic {
            COMBINED.replace(
                "fn main() -> i64 { return scenario(false); }",
                "fn main() -> i64 { return scenario(true); }",
            )
        } else {
            COMBINED.to_owned()
        };
        let source = source(&text);
        let flow = source.failure_flow();
        let layouts = l::Layouts::compile(
            flow.source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations()
                .types(),
            Default::default(),
        )
        .unwrap();
        let program =
            n::CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
        let controls = e::Control::with_limits(control::limits());
        let (actual, requests) = run(&program, main_id(flow), controls, None);
        let (expected, expected_requests) = control::vm(flow, controls, None);
        compare(&actual, &expected);
        assert_eq!(requests, expected_requests);
        let usage = actual.as_ref().map(|v| v.usage).unwrap_or_else(|e| e.usage);
        assert!(usage.fuel > 0 && usage.service_work > 0 && requests > 0);
        if panic {
            assert_eq!(actual.unwrap_err().panic_code, Some(-42));
        } else {
            assert_eq!(actual.unwrap().value, 42);
        }
        eprintln!(
            "combined panic={panic}: fuel={}, service-work={}, provider-requests={requests}",
            usage.fuel, usage.service_work
        );
        for fuel in 0..=usage.fuel + 1 {
            let control = e::Control::with_limits(e::Limits {
                fuel: Some(fuel),
                ..control::limits()
            });
            let (actual, requests) = run(&program, main_id(flow), control, None);
            let (expected, expected_requests) = control::vm(flow, control, None);
            compare(&actual, &expected);
            assert_eq!(requests, expected_requests);
        }
        for work in 0..=usage.service_work + 1 {
            let control = controls.and_service_work(Some(work));
            let (actual, requests) = run(&program, main_id(flow), control, None);
            let (expected, expected_requests) = control::vm(flow, control, None);
            compare(&actual, &expected);
            assert_eq!(requests, expected_requests);
        }
        for at in 1..=requests {
            let (actual, requests) = run(&program, main_id(flow), controls, Some(at));
            let (expected, expected_requests) =
                control::vm_with_refusal(flow, controls, None, Some(at));
            compare(&actual, &expected);
            assert_eq!(requests, expected_requests);
        }
        for frames in [0, usage.peak_frames - 1, usage.peak_frames] {
            let control = e::Control::with_limits(e::Limits {
                frames: Some(frames),
                ..control::limits()
            });
            compare(
                &run(&program, main_id(flow), control, None).0,
                &control::vm(flow, control, None).0,
            );
        }
        for cells in [0, usage.peak_live_cells - 1, usage.peak_live_cells] {
            let control = e::Control::with_limits(e::Limits {
                live_cells: Some(cells),
                ..control::limits()
            });
            compare(
                &run(&program, main_id(flow), control, None).0,
                &control::vm(flow, control, None).0,
            );
        }
    }
}

fn main_id(flow: &m::ownership::failure::Elaborated) -> m::FunctionId {
    m::FunctionId(
        flow.source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .functions()
            .iter()
            .position(|f| f.name == "main")
            .unwrap() as u32,
    )
}
fn compare(
    actual: &Actual,
    expected: &Result<lang_vm::unit::owned::Outcome, lang_vm::unit::owned::Failure>,
) {
    match (actual, expected) {
        (Ok(a), Ok(b)) => {
            assert_eq!(a.value, b.value);
            assert_eq!(a.usage, b.usage);
            assert!(a.report.primary().is_none());
        }
        (Err(a), Err(b)) => {
            assert_eq!(a.usage, b.usage, "native error {a:?}; VM {b:?}");
            assert!(a.cleanup.is_none(), "{a:?}");
            let native_report = a.report.as_ref();
            assert_eq!(
                native_report
                    .and_then(|r| r.primary())
                    .map(|r| (r.kind, r.span)),
                b.report
                    .as_ref()
                    .and_then(|r| r.primary())
                    .map(|r| (r.kind, r.span)),
                "native {a:?}; VM {b:?}"
            );
            assert_eq!(
                native_report.map_or(0, |r| r.secondary().len()),
                b.report.as_ref().map_or(0, |r| r.secondary().len())
            );
            assert_eq!(a.panic_code, b.panic_code);
            if let Some(fault) = b.fault {
                assert_eq!(a.fault, Some(fault));
            }
            if let Some(function) = b.function {
                assert_eq!(a.function, Some(function));
            }
            assert_eq!(
                a.stack
                    .iter()
                    .map(|(f, s)| (*f, Some(*s)))
                    .collect::<Vec<_>>(),
                b.stack
                    .iter()
                    .map(|f| (f.function, f.span))
                    .collect::<Vec<_>>(),
                "native {a:?}; VM {b:?}"
            );
        }
        (a, b) => panic!(
            "different result categories: native {a:?}, VM failure {:?}",
            b.as_ref().err()
        ),
    }
}
fn run<'a, S: entry::EntryProgram<'a>>(
    program: &'a S,
    function: m::FunctionId,
    control: e::Control<'_>,
    refuse: Option<usize>,
) -> (Actual, usize) {
    let trace = Rc::new(RefCell::new(Trace {
        refuse,
        ..Default::default()
    }));
    let mut allocator = a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let actual = entry::execute_entry(
        program,
        function,
        &mut instance,
        &mut allocator,
        entry::Config {
            control,
            ..Default::default()
        },
    );
    assert!(!instance.is_poisoned(), "{actual:?}");
    assert_eq!(allocator.stats().live_allocations, 0, "{actual:?}");
    let trace = trace.borrow();
    assert_eq!(trace.allocated, trace.released, "{actual:?}");
    (actual, trace.requests)
}
#[test]
fn production_owned_entry_covers_all_managed_families_and_explicit_panic() {
    for (index, text) in SOURCES.into_iter().enumerate() {
        let source = source(text);
        let flow = source.failure_flow();
        let layouts = l::Layouts::compile(
            flow.source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations()
                .types(),
            Default::default(),
        )
        .unwrap();
        let program =
            n::CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
        let control = e::Control::with_limits(control::limits());
        let (actual, requests) = run(&program, main_id(flow), control, None);
        let (expected, expected_requests) = control::vm(flow, control, None);
        compare(&actual, &expected);
        assert_eq!(requests, expected_requests, "source {index}");
    }
}
#[test]
fn production_owned_entry_matches_each_fuel_work_and_provider_refusal_prefix() {
    for text in SOURCES {
        let source = source(text);
        let flow = source.failure_flow();
        let layouts = l::Layouts::compile(
            flow.source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations()
                .types(),
            Default::default(),
        )
        .unwrap();
        let program =
            n::CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
        let function = main_id(flow);
        let base = e::Control::with_limits(control::limits());
        let (normal, requests) = run(&program, function, base, None);
        let usage = normal.as_ref().map(|v| v.usage).unwrap_or_else(|e| e.usage);
        for fuel in 0..=usage.fuel + 1 {
            let control = e::Control::with_limits(e::Limits {
                fuel: Some(fuel),
                ..control::limits()
            });
            let (actual, requests) = run(&program, function, control, None);
            let (expected, expected_requests) = control::vm(flow, control, None);
            compare(&actual, &expected);
            assert_eq!(requests, expected_requests);
        }
        for work in 0..=usage.service_work + 1 {
            let control = base.and_service_work(Some(work));
            let (actual, requests) = run(&program, function, control, None);
            let (expected, expected_requests) = control::vm(flow, control, None);
            compare(&actual, &expected);
            assert_eq!(requests, expected_requests);
        }
        for at in 1..=requests {
            let (actual, requests) = run(&program, function, base, Some(at));
            let (expected, expected_requests) =
                control::vm_with_refusal(flow, base, None, Some(at));
            compare(&actual, &expected);
            assert_eq!(requests, expected_requests);
        }
    }
}

#[test]
fn production_owned_entry_preserves_frame_cell_and_async_control_boundaries() {
    let source = source(SOURCES[7]);
    let flow = source.failure_flow();
    let layouts = l::Layouts::compile(
        flow.source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types(),
        Default::default(),
    )
    .unwrap();
    let program = n::CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    let function = main_id(flow);
    let base = e::Control::with_limits(control::limits());
    let normal = run(&program, function, base, None).0.unwrap();
    for frames in [0, normal.usage.peak_frames - 1, normal.usage.peak_frames] {
        let c = e::Control::with_limits(e::Limits {
            frames: Some(frames),
            ..control::limits()
        });
        let (actual, requests) = run(&program, function, c, None);
        let (expected, n) = control::vm(flow, c, None);
        compare(&actual, &expected);
        assert_eq!(requests, n);
    }
    for cells in [
        0,
        normal.usage.peak_live_cells - 1,
        normal.usage.peak_live_cells,
    ] {
        let c = e::Control::with_limits(e::Limits {
            live_cells: Some(cells),
            ..control::limits()
        });
        let (actual, requests) = run(&program, function, c, None);
        let (expected, n) = control::vm(flow, c, None);
        compare(&actual, &expected);
        assert_eq!(requests, n);
    }
    let flag = Rc::new(e::InterruptionFlag::new());
    flag.interrupt();
    let c = base.and_interruption(&flag);
    compare(
        &run(&program, function, c, None).0,
        &control::vm(flow, c, None).0,
    );
    let c = base.and_deadline(std::time::Instant::now() - std::time::Duration::from_secs(1));
    compare(
        &run(&program, function, c, None).0,
        &control::vm(flow, c, None).0,
    );

    let native_flag = Rc::new(e::InterruptionFlag::new());
    let vm_flag = Rc::new(e::InterruptionFlag::new());
    let trace = Rc::new(RefCell::new(Trace {
        interrupt: Some((1, native_flag.clone())),
        ..Default::default()
    }));
    let mut allocator = a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let actual = entry::execute_entry(
        &program,
        function,
        &mut instance,
        &mut allocator,
        entry::Config {
            control: base.and_interruption(&native_flag),
            ..Default::default()
        },
    );
    let expected = control::vm(
        flow,
        base.and_interruption(&vm_flag),
        Some((1, vm_flag.clone())),
    )
    .0;
    compare(&actual, &expected);
    assert_eq!(allocator.stats().live_allocations, 0);
}
#[cfg(target_os = "macos")]
#[test]
fn production_owned_entry_runs_persisted_linked_records_without_producer_or_test_runtime() {
    for text in SOURCES {
        let files = {
            let (record, _, semantic) = record_artifact(text);
            let files = Directory::new("production owned startup");
            fs::write(files.0.join("program.lbc"), semantic).unwrap();
            fs::write(files.0.join("program.native"), record).unwrap();
            files
        };
        let semantic = fs::read(files.0.join("program.lbc")).unwrap();
        let record = fs::read(files.0.join("program.native")).unwrap();
        let decoded = lang_bytecode::ownership::decode(&semantic, Default::default()).unwrap();
        let entry_id = decoded.entry().unwrap();
        let flow = decoded.flow();
        let layouts = l::Layouts::compile(
            flow.source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations()
                .types(),
            Default::default(),
        )
        .unwrap();
        let native = lang_codegen_clif::native_record::Record::decode(
            &record,
            &semantic,
            Default::default(),
        )
        .unwrap();
        let artifact = n::RecordedObject::from_record(native)
            .unwrap()
            .prepare(flow, &layouts, Default::default(), 1000)
            .unwrap();
        let dropped = Rc::new(Cell::new(false));
        let image = Image::link(&artifact.bytes, dropped.clone());
        // SAFETY: this image links these exact compiler bytes and retains all
        // private continuation entries through the original checked gateway.
        let program = unsafe { artifact.bind(image, Image::symbol, Default::default()) }.unwrap();
        let control = e::Control::with_limits(control::limits());
        let (actual, requests) = run(&program, entry_id, control, None);
        let (expected, n) = control::vm(flow, control, None);
        compare(&actual, &expected);
        assert_eq!(requests, n);
        assert!(!dropped.get());
        drop(program);
        assert!(dropped.get());
    }
}

struct PanicOnDrop;
impl Drop for PanicOnDrop {
    fn drop(&mut self) {
        panic!("secondary payload drop");
    }
}
struct PanicProvider {
    inner: Provider,
    allocate_at: Option<usize>,
    deallocate_at: Option<usize>,
    releases: Rc<Cell<usize>>,
    payload_drop: bool,
}
impl a::Provider for PanicProvider {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<a::Storage> {
        let next = self.inner.0.borrow().requests + 1;
        if self.allocate_at == Some(next) {
            self.inner.0.borrow_mut().requests = next;
            if self.payload_drop {
                std::panic::panic_any(PanicOnDrop);
            }
            panic!("provider allocation panic");
        }
        self.inner.allocate(request)
    }
    fn deallocate(&mut self, region: a::Storage, request: a::Request) {
        self.inner.deallocate(region, request);
        let next = self.releases.get() + 1;
        self.releases.set(next);
        if self.deallocate_at == Some(next) {
            panic!("provider release panic");
        }
    }
}
#[test]
#[allow(
    clippy::result_large_err,
    reason = "keep the allocation-failure report inline while testing unwind containment"
)]
fn production_owned_entry_contains_provider_panics_and_keeps_instance_poison() {
    let source = source(SOURCES[0]);
    let flow = source.failure_flow();
    let layouts = l::Layouts::compile(
        flow.source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types(),
        Default::default(),
    )
    .unwrap();
    let program = n::CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    let function = main_id(flow);
    let config = entry::Config {
        control: e::Control::with_limits(control::limits()),
        ..Default::default()
    };
    let normal_releases = {
        let releases = Rc::new(Cell::new(0));
        let trace = Rc::new(RefCell::new(Trace::default()));
        let provider = PanicProvider {
            inner: Provider(trace),
            allocate_at: None,
            deallocate_at: None,
            releases: releases.clone(),
            payload_drop: false,
        };
        let mut allocator = a::Allocator::new(provider, Default::default()).unwrap();
        entry::execute_entry(
            &program,
            function,
            &mut c::Instance::default(),
            &mut allocator,
            config,
        )
        .unwrap();
        releases.get()
    };
    for (allocation, release, bad_payload) in [
        (Some(1), None, true),
        (None, Some(1), false),
        (None, Some(normal_releases), false),
    ] {
        let trace = Rc::new(RefCell::new(Trace::default()));
        let releases = Rc::new(Cell::new(0));
        let provider = PanicProvider {
            inner: Provider(trace.clone()),
            allocate_at: allocation,
            deallocate_at: release,
            releases: releases.clone(),
            payload_drop: bad_payload,
        };
        let mut allocator = a::Allocator::new(provider, Default::default()).unwrap();
        let mut instance = c::Instance::default();
        let actual = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            entry::execute_entry(&program, function, &mut instance, &mut allocator, config)
        }))
        .expect("provider unwind contained");
        let failure = actual.unwrap_err();
        assert!(instance.is_poisoned(), "{failure:?}");
        if bad_payload {
            assert_eq!(failure.cleanup.unwrap().quarantined, 1);
        }
        let before = trace.borrow().requests;
        assert!(matches!(
            entry::execute_entry(&program, function, &mut instance, &mut allocator, config),
            Err(entry::EntryError {
                kind: entry::ErrorKind::Poisoned,
                ..
            })
        ));
        assert_eq!(trace.borrow().requests, before);
        drop(allocator);
        assert_eq!(trace.borrow().allocated, trace.borrow().released);
    }
}
#[test]
fn production_owned_entry_refuses_invalid_entries_and_preparation_before_provider_effects() {
    // C program owners deliberately do not implement the sealed EntryProgram
    // interface; even unused C declarations cannot create an ordinary owner.
    let foreign = source("extern \"C\" fn foreign(value: i64)->i64; fn main()->i64 { return 42; }");
    let foreign_flow = foreign.failure_flow();
    let foreign_layouts = l::Layouts::compile(
        foreign_flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types(),
        Default::default(),
    )
    .unwrap();
    assert!(matches!(
        n::CompiledProgram::compile(foreign_flow, &foreign_layouts, Default::default(), 1000),
        Err(n::Error::UnsupportedCInterface { .. })
    ));
    let source = source(
        "fn other(x: i64)->i64 { return x; } fn main()->i64 { let mut b: core::Bytes = core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator())); return 1; }",
    );
    let flow = source.failure_flow();
    let layouts = l::Layouts::compile(
        flow.source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types(),
        Default::default(),
    )
    .unwrap();
    let program = n::CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut allocator = a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let config = entry::Config {
        control: e::Control::with_limits(control::limits()),
        ..Default::default()
    };
    for function in [m::FunctionId(u32::MAX), m::FunctionId(0)] {
        assert!(matches!(
            entry::execute_entry(&program, function, &mut instance, &mut allocator, config),
            Err(entry::EntryError {
                kind: entry::ErrorKind::Entry,
                ..
            })
        ));
    }
    let mut limited = config;
    limited.limits.work = 0;
    assert!(matches!(
        entry::execute_entry(
            &program,
            main_id(flow),
            &mut instance,
            &mut allocator,
            limited
        ),
        Err(entry::EntryError {
            kind: entry::ErrorKind::Preparation(entry::PreparationError::Resource(_)),
            ..
        })
    ));
    let mut unbounded = config;
    unbounded.control = e::Control::with_limits(e::Limits::unbounded());
    assert!(matches!(
        entry::execute_entry(
            &program,
            main_id(flow),
            &mut instance,
            &mut allocator,
            unbounded
        ),
        Err(entry::EntryError {
            kind: entry::ErrorKind::Preparation(entry::PreparationError::Capacity),
            ..
        })
    ));
    assert_eq!(trace.borrow().requests, 0);
    assert!(!instance.is_poisoned());
    let result = entry::execute_entry(
        &program,
        main_id(flow),
        &mut instance,
        &mut allocator,
        config,
    )
    .unwrap();
    assert_eq!(result.value, 1);
    assert_eq!(allocator.stats().live_allocations, 0);
}
#[test]
fn production_owned_entry_reuses_original_allocator_and_instance_after_clean_failures() {
    let source = source(SOURCES[7]);
    let flow = source.failure_flow();
    let layouts = l::Layouts::compile(
        flow.source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types(),
        Default::default(),
    )
    .unwrap();
    let program = n::CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    let trace = Rc::new(RefCell::new(Trace {
        refuse: Some(1),
        ..Default::default()
    }));
    let mut allocator = a::Allocator::new(Provider(trace.clone()), Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let config = entry::Config {
        control: e::Control::with_limits(control::limits()),
        ..Default::default()
    };
    assert!(
        entry::execute_entry(
            &program,
            main_id(flow),
            &mut instance,
            &mut allocator,
            config
        )
        .is_err()
    );
    assert!(!instance.is_poisoned());
    assert_eq!(allocator.stats().live_allocations, 0);
    trace.borrow_mut().refuse = None;
    let actual = entry::execute_entry(
        &program,
        main_id(flow),
        &mut instance,
        &mut allocator,
        config,
    )
    .unwrap();
    let expected = control::vm(flow, config.control, None).0.unwrap();
    assert_eq!(actual.value, expected.value);
    assert_eq!(actual.usage, expected.usage);
    assert!(!instance.is_poisoned());
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(trace.borrow().allocated, trace.borrow().released);
}
