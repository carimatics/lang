use super::*;
use lang_runtime::{allocation as a, cleanup as c};
use lang_vm::{InterruptionFlag, unit::owned as engine};
use std::rc::Rc;

#[path = "provisional_reads.rs"]
mod reads;

const FRESH: &str = "fn fresh_bytes() -> core::Bytes { return core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); }";
const BYTE: &str = "fn byte() -> core::Bytes { let mut bytes: core::Bytes = fresh_bytes(); let mut pushed: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(&mut bytes, 65)); return bytes; }";
const FAIL: &str = "fn fail() -> core::Bytes { let mut kept: core::Bytes = fresh_bytes(); let mut none: core::Option<core::Bytes> = core::Option::None; return core::option_unwrap::<core::Bytes>(none); }";

#[derive(Default)]
struct Trace {
    requests: usize,
    allocated: Vec<usize>,
    released: Vec<usize>,
    fail_at: usize,
    interrupt: Option<Rc<InterruptionFlag>>,
}
struct Provider(Rc<RefCell<Trace>>);
struct Region {
    storage: a::Storage,
    id: usize,
}
impl a::Region for Region {
    fn bytes(&self) -> &[u8] {
        self.storage.bytes()
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        self.storage.bytes_mut()
    }
}
impl a::Provider for Provider {
    type Region = Region;
    fn allocate(&mut self, request: a::Request) -> Option<Region> {
        let mut trace = self.0.borrow_mut();
        trace.requests += 1;
        if trace.fail_at == trace.requests {
            return None;
        }
        if request.bytes() == 25
            && let Some(flag) = &trace.interrupt
        {
            flag.interrupt();
        }
        let storage = a::Storage::try_new(request).ok()?;
        let id = trace.requests;
        trace.allocated.push(id);
        Some(Region { storage, id })
    }
    fn deallocate(&mut self, region: Region, _: a::Request) {
        self.0.borrow_mut().released.push(region.id);
    }
}
fn allocator(limits: a::Limits) -> (a::Allocator<Provider>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    (
        a::Allocator::new(Provider(Rc::clone(&trace)), limits).unwrap(),
        trace,
    )
}
fn released(allocator: &a::Allocator<Provider>, trace: &RefCell<Trace>) {
    let stats = allocator.stats();
    assert_eq!(stats.live_allocations, 0);
    assert_eq!(stats.live_bytes, 0);
    assert_eq!(stats.allocations, stats.deallocations);
    let t = trace.borrow();
    let mut ids = t.released.clone();
    ids.sort_unstable();
    assert_eq!(ids, t.allocated, "every real region released exactly once");
}
fn eval(
    session: &mut Session,
    text: &str,
    allocator: &mut a::Allocator<Provider>,
    control: Control<'_>,
) -> Report {
    let report = session.submit_with_allocator(
        text.as_bytes(),
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        allocator,
        control,
    );
    for d in &report.diagnostics {
        d.validate(&report.sources).unwrap();
    }
    report
}
fn expect_value(report: Report, expected: &str) {
    assert_eq!(
        report
            .result
            .as_ref()
            .unwrap_or_else(|e| panic!("{e:?}: {:?}", report.diagnostics)),
        &Event::Value(expected.into())
    );
}

#[test]
fn owned_values_use_canonical_previews_and_retain_only_source_definitions() {
    let mut session = Session::default();
    for item in [
        FRESH,
        BYTE,
        "fn text() -> core::Text { return core::result_unwrap::<core::Text, core::Utf8Error>(core::text_from_utf8(byte())); }",
        "struct Message { bytes: core::Bytes, text: core::Text, count: i64 }",
        "fn message() -> core::Result<Message, core::AllocError> { return core::Result::Ok(Message { count: 7, text: text(), bytes: byte() }); }",
        "fn main(x: i64) -> bool { return x < 1; }",
    ] {
        commit(&mut session, item);
    }
    let definitions = session.definition_count();
    let (mut allocator, trace) = allocator(Default::default());
    for (expression, expected) in [
        ("byte()", "core::Bytes [65]"),
        ("text()", "core::Text \"A\""),
        (
            "message()",
            "core::Result::Ok(Message { bytes: core::Bytes [65], text: core::Text \"A\", count: 7 })",
        ),
        (
            "core::bytes_new(core::allocator())",
            "core::Result::Ok(core::Bytes [])",
        ),
        ("core::Unit {}", "core::Unit {}"),
        ("core::AllocError::Limit", "core::AllocError::Limit"),
        ("main(0)", "true"),
        ("1 + 2", "3"),
    ] {
        expect_value(
            eval(&mut session, expression, &mut allocator, Control::default()),
            expected,
        );
        released(&allocator, &trace);
        assert_eq!(session.definition_count(), definitions);
    }
    for expression in [
        "core::Option::None",
        "core::allocator()",
        "let mut saved: core::Bytes = byte();",
    ] {
        let report = eval(&mut session, expression, &mut allocator, Control::default());
        assert!(report.result.is_err());
        assert!(!report.diagnostics.is_empty());
        assert!(!rendered(&report).contains("__repl_eval_"));
        assert!(!rendered(&report).contains("repl.lang"));
        assert_eq!(session.definition_count(), definitions);
        released(&allocator, &trace);
    }
}

#[test]
fn owned_panic_reports_map_cleanup_and_call_sites_and_survive_reset() {
    let mut session = Session::default();
    for item in [FRESH, FAIL, "fn outer() -> core::Bytes { return fail(); }"] {
        commit(&mut session, item);
    }
    let (mut allocator, trace) = allocator(Default::default());
    let report = eval(
        &mut session,
        "\n outer()",
        &mut allocator,
        Control::default(),
    );
    let Err(ReplError::OwnedRuntime(error)) = &report.result else {
        panic!("{report:?}");
    };
    assert_eq!(error.panic_code, Some(-1));
    assert!(!error.poisoned);
    assert!(error.trace_complete);
    assert_eq!(
        error
            .stack
            .iter()
            .map(|f| error.function_name(*f))
            .collect::<Vec<_>>(),
        ["fail", "outer", "<expression>"]
    );
    assert_eq!(
        report.sources.slice(error.fault.unwrap()).unwrap(),
        "core::option_unwrap::<core::Bytes>(none)"
    );
    assert_eq!(
        report
            .sources
            .file(error.fault.unwrap().source())
            .unwrap()
            .name(),
        "repl/2.lang"
    );
    assert_eq!(
        report.sources.slice(error.stack[2].span.unwrap()).unwrap(),
        "outer()"
    );
    assert_eq!(
        report
            .sources
            .file(error.stack[2].span.unwrap().source())
            .unwrap()
            .name(),
        "repl/4.lang"
    );
    let cleanup = error.report.as_ref().unwrap();
    assert_eq!(
        cleanup.primary.unwrap().kind,
        c::FailureKind::Execution(lang_mir::unit::ownership::failure::ExecutionFailure::Panic)
    );
    assert_eq!(cleanup.primary.unwrap().span, error.fault);
    assert!(cleanup.secondary.is_empty());
    released(&allocator, &trace);
    expect_value(
        eval(
            &mut session,
            "fresh_bytes()",
            &mut allocator,
            Control::default(),
        ),
        "core::Bytes []",
    );
    assert_eq!(submit(&mut session, ":reset").result.unwrap(), Event::Reset);
    commit(&mut session, "fn other() -> i64 { return 42; }");
    value(&mut session, "other()", "42");
    assert_eq!(error.function_name(error.stack[0]), "fail");
    assert_eq!(
        report
            .sources
            .slice(cleanup.primary.unwrap().span.unwrap())
            .unwrap(),
        "core::option_unwrap::<core::Bytes>(none)"
    );
    released(&allocator, &trace);
}

#[test]
fn owned_limits_oom_interruption_and_cumulative_quotas_release_and_recover() {
    let mut session = Session::new(
        None,
        ReplLimits {
            display_bytes: 14,
            ..Default::default()
        },
    );
    commit(&mut session, FRESH);
    let (mut alloc, trace) = allocator(Default::default());
    let report = eval(
        &mut session,
        "core::bytes_new(core::allocator())",
        &mut alloc,
        Control::default(),
    );
    let Err(ReplError::OwnedRuntime(error)) = report.result else {
        panic!("display limit");
    };
    assert!(matches!(
        error.reason,
        engine::Reason::Preview(lang_runtime::layout::PreviewError::Limit)
    ));
    assert!(!error.poisoned);
    released(&alloc, &trace);
    expect_value(
        eval(
            &mut session,
            "fresh_bytes()",
            &mut alloc,
            Control::default(),
        ),
        "core::Bytes []",
    );
    let one = alloc.stats().cumulative_bytes;
    assert!(one > 0);

    let mut session = Session::default();
    commit(&mut session, FRESH);
    let (mut baseline, t) = allocator(Default::default());
    expect_value(
        eval(
            &mut session,
            "fresh_bytes()",
            &mut baseline,
            Control::default(),
        ),
        "core::Bytes []",
    );
    let charge = baseline.stats().cumulative_bytes;
    released(&baseline, &t);
    let (mut quota, q) = allocator(a::Limits {
        cumulative_bytes: charge,
        ..Default::default()
    });
    expect_value(
        eval(
            &mut session,
            "fresh_bytes()",
            &mut quota,
            Control::default(),
        ),
        "core::Bytes []",
    );
    assert!(
        eval(
            &mut session,
            "fresh_bytes()",
            &mut quota,
            Control::default()
        )
        .result
        .is_err()
    );
    assert_eq!(quota.stats().cumulative_bytes, charge);
    released(&quota, &q);

    let (mut alloc, trace) = allocator(Default::default());
    trace.borrow_mut().fail_at = 1;
    assert!(
        eval(
            &mut session,
            "fresh_bytes()",
            &mut alloc,
            Control::default()
        )
        .result
        .is_err()
    );
    released(&alloc, &trace);
    trace.borrow_mut().fail_at = 0;
    let exact = (0..128)
        .find(|fuel| {
            let report = eval(
                &mut session,
                "fresh_bytes()",
                &mut alloc,
                Control::with_limits(Limits {
                    fuel: Some(*fuel),
                    ..Default::default()
                }),
            );
            released(&alloc, &trace);
            report.result.is_ok()
        })
        .unwrap();
    for _ in 0..3 {
        expect_value(
            eval(
                &mut session,
                "fresh_bytes()",
                &mut alloc,
                Control::with_limits(Limits {
                    fuel: Some(exact),
                    ..Default::default()
                }),
            ),
            "core::Bytes []",
        );
        released(&alloc, &trace);
    }
    commit(&mut session, BYTE);
    let flag = Rc::new(InterruptionFlag::default());
    trace.borrow_mut().interrupt = Some(Rc::clone(&flag));
    let interrupted = eval(
        &mut session,
        "byte()",
        &mut alloc,
        Control::default().and_interruption(&flag),
    );
    assert!(interrupted.result.is_err());
    assert!(flag.is_interrupted());
    released(&alloc, &trace);
    trace.borrow_mut().interrupt = None;
    expect_value(
        eval(&mut session, "byte()", &mut alloc, Control::default()),
        "core::Bytes [65]",
    );
    released(&alloc, &trace);
}

#[test]
fn ownership_transactions_and_unsupported_reachable_operations_have_no_provider_effects() {
    let mut session = Session::default();
    commit(&mut session, FRESH);
    let bad = submit(&mut session, "fn bad() -> i64 { return fresh_bytes(); }");
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.code == codes::EXPECTED_I64)
    );
    assert_eq!(session.definition_count(), 1);
    assert!(!rendered(&bad).contains("repl.lang"));
    commit(
        &mut session,
        "fn vector() -> core::Vec<i64> { return core::result_unwrap::<core::Vec<i64>, core::AllocError>(core::vec_new::<i64>(core::allocator())); }",
    );
    let (mut allocator, trace) = allocator(Default::default());
    let report = eval(&mut session, "vector()", &mut allocator, Control::default());
    assert!(
        matches!(report.result, Err(ReplError::OwnedRuntime(ref e)) if matches!(e.reason, engine::Reason::Unsupported { .. }))
    );
    assert_eq!(trace.borrow().requests, 0);
    expect_value(
        eval(
            &mut session,
            "fresh_bytes()",
            &mut allocator,
            Control::default(),
        ),
        "core::Bytes []",
    );
    released(&allocator, &trace);
}

#[test]
fn owned_modules_require_source_authority_and_keep_imported_fault_snapshots() {
    let directory =
        std::env::temp_dir().join(format!("lang-repl-owned-modules-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let text = "pub struct Box { bytes: core::Bytes } pub fn fresh() -> Box { return Box { bytes: core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())) }; } pub fn fail() -> core::Bytes { let mut none: core::Option<core::Bytes> = core::Option::None; return core::option_unwrap::<core::Bytes>(none); }";
    std::fs::write(directory.join("data.lang"), text).unwrap();
    let mut denied = Session::default();
    assert!(submit(&mut denied, "mod data;").result.is_err());
    assert_eq!(denied.definition_count(), 0);
    let mut session = Session::new(Some(directory.clone()), Default::default());
    commit(&mut session, "mod data;");
    let (mut allocator, trace) = allocator(Default::default());
    expect_value(
        eval(
            &mut session,
            "data::fresh()",
            &mut allocator,
            Control::default(),
        ),
        "data::Box { bytes: core::Bytes [] }",
    );
    let report = eval(
        &mut session,
        "data::fail()",
        &mut allocator,
        Control::default(),
    );
    let Err(ReplError::OwnedRuntime(error)) = &report.result else {
        panic!("{report:?}");
    };
    assert_eq!(error.function_name(error.stack[0]), "data::fail");
    assert!(
        report
            .sources
            .file(error.fault.unwrap().source())
            .unwrap()
            .name()
            .ends_with("data.lang")
    );
    submit(&mut session, ":reset");
    std::fs::remove_dir_all(directory).unwrap();
    assert_eq!(
        report.sources.slice(error.fault.unwrap()).unwrap(),
        "core::option_unwrap::<core::Bytes>(none)"
    );
    released(&allocator, &trace);
}

#[test]
fn explicit_allocator_preserves_copy_host_calls_and_owned_preflight_refuses_host_effects() {
    EVENTS.with_borrow_mut(Vec::clear);
    let mut session = Session::default();
    commit(&mut session, "extern fn visit(x: i64) -> i64;");
    let mut hosts = HostFunctionRegistry::new();
    hosts.register_i64(HostFunctionId::new(7), visit).unwrap();
    let mut bindings = HostBindings::new();
    bindings.bind("visit", HostFunctionId::new(7)).unwrap();
    let (mut allocator, trace) = allocator(Default::default());
    expect_value(
        session.submit_with_allocator(
            b"visit(41)",
            &hosts,
            &bindings,
            &mut allocator,
            Control::default(),
        ),
        "41",
    );
    assert_eq!(trace.borrow().requests, 0);
    commit(&mut session, FRESH);
    let report = session.submit_with_allocator(
        b"visit(5)",
        &hosts,
        &bindings,
        &mut allocator,
        Control::default(),
    );
    assert!(
        matches!(report.result, Err(ReplError::OwnedRuntime(ref e)) if matches!(e.reason, engine::Reason::Unsupported { .. }))
    );
    EVENTS.with_borrow(|e| assert_eq!(e, &[41]));
    assert_eq!(trace.borrow().requests, 0);
    expect_value(
        eval(
            &mut session,
            "fresh_bytes()",
            &mut allocator,
            Control::default(),
        ),
        "core::Bytes []",
    );
    released(&allocator, &trace);
}

#[test]
fn piped_owned_repl_transcript_recovers_from_runtime_and_frame_failures_without_prompts() {
    let output = cli(include_bytes!("../../examples/repl/owned.repl"), &[]);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "core::Bytes []\ncore::Result::Ok(core::Bytes [])\ncore::Text \"\"\ncore::Result::Ok(PreviewMessage { bytes: core::Bytes [], text: core::Text \"\", count: 7 })\n3\n3\n"
    );
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = cli(
        format!("{FRESH}\n{FAIL}\nfail()\nfresh_bytes()\n:quit\n").as_bytes(),
        &[],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"core::Bytes []\n");
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("panic code -1"));
    assert!(error.contains("repl/2.lang"));
    assert!(error.contains("<expression>"));
    assert!(!error.contains("__repl_eval_"));
    assert!(!error.contains("repl.lang"));
    let output = cli(
        format!("{FRESH}\nfresh_bytes()\n1 + 2\n:quit\n").as_bytes(),
        &["--frames".into(), "1".into()],
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"3\n");
    assert!(String::from_utf8_lossy(&output.stderr).contains("FrameLimit"));
}
