use lang::repl::{Event, Readiness, ReplError, ReplLimits, Report, Session, SubmissionBuffer};
use lang_diagnostics::codes;
use lang_mir::HostFunctionId;
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit::{Control, HostBindings, Limits, Reason};
use std::cell::RefCell;
use std::ffi::OsString;
use std::io::Write;
use std::process::{Command, Output, Stdio};

#[path = "support/repl_owned.rs"]
mod owned;

fn submit(session: &mut Session, text: &str) -> Report {
    let report = session.submit(
        text.as_bytes(),
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::default(),
    );
    for diagnostic in &report.diagnostics {
        diagnostic.validate(&report.sources).unwrap();
    }
    report
}
fn commit(session: &mut Session, text: &str) {
    let r = submit(session, text);
    assert_eq!(
        r.result.as_ref().unwrap(),
        &Event::ItemCommitted,
        "{:?}",
        r.diagnostics
    );
}
fn value(session: &mut Session, text: &str, expected: &str) {
    let r = submit(session, text);
    assert_eq!(
        r.result.as_ref().unwrap(),
        &Event::Value(expected.into()),
        "{:?}",
        r.diagnostics
    );
}
fn rendered(report: &Report) -> String {
    report
        .diagnostics
        .iter()
        .map(|d| d.render(&report.sources).unwrap())
        .collect()
}

#[test]
fn ownership_items_commit_without_granting_legacy_submission_allocator_authority() {
    let mut session = Session::default();
    commit(&mut session, "fn keep() -> i64 { return 7; }");
    let report = submit(
        &mut session,
        "fn owned(v: core::Text) -> core::Text { return v; }",
    );
    assert_eq!(report.result.unwrap(), Event::ItemCommitted);
    assert!(report.diagnostics.is_empty());
    assert_eq!(session.definition_count(), 2);
    let report = submit(&mut session, "keep()");
    assert!(
        matches!(report.result, Err(ReplError::OwnedRuntime(ref e)) if matches!(e.reason, lang_vm::unit::owned::Reason::MissingAllocator))
    );
    let mut allocator = lang_runtime::allocation::Allocator::new(
        lang_runtime::allocation::System,
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        session
            .submit_with_allocator(
                b"keep()",
                &HostFunctionRegistry::new(),
                &HostBindings::new(),
                &mut allocator,
                Control::default()
            )
            .result
            .unwrap(),
        Event::Value("7".into())
    );
    assert_eq!(allocator.stats().live_allocations, 0);
    assert!(
        submit(&mut session, "fn owned() -> i64 { return 9; }")
            .result
            .is_err()
    );
    assert_eq!(submit(&mut session, ":reset").result.unwrap(), Event::Reset);
    commit(&mut session, "fn owned() -> i64 { return 9; }");
    value(&mut session, "owned()", "9");
}

#[test]
fn definitions_commit_only_after_successful_complete_analysis() {
    let mut s = Session::default();
    commit(&mut s, "fn add_one(x: i64) -> i64 { return x + 1; }");
    let bad = submit(&mut s, "fn bad() -> i64 { return false; }");
    assert!(matches!(bad.result, Err(ReplError::Diagnostics)));
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.code == codes::EXPECTED_I64)
    );
    assert_eq!(s.definition_count(), 1);
    value(&mut s, "add_one(41)", "42");
    assert_eq!(s.definition_count(), 1);
    let bad = submit(&mut s, "bad()");
    assert!(bad.result.is_err());
    assert!(
        bad.diagnostics
            .iter()
            .any(|d| d.code == codes::UNKNOWN_NAME)
    );
}

#[test]
fn duplicate_items_retain_original_declaration_locations_until_reset() {
    let mut s = Session::default();
    let text = "\nfn answer() -> i64 { return 42; }";
    commit(&mut s, text);
    let report = submit(&mut s, "fn answer() -> i64 { return 0; }");
    assert!(report.result.is_err());
    assert_eq!(s.definition_count(), 1);
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|d| d.code == codes::MODULE_ERROR)
        .unwrap();
    assert_eq!(
        report
            .sources
            .file(diagnostic.primary.span.source())
            .unwrap()
            .name(),
        "repl/2.lang"
    );
    assert_eq!(
        report
            .sources
            .file(diagnostic.secondary[0].span.source())
            .unwrap()
            .name(),
        "repl/1.lang"
    );
    assert_eq!(
        report
            .sources
            .file(diagnostic.secondary[0].span.source())
            .unwrap()
            .position(diagnostic.secondary[0].span.range().start)
            .unwrap()
            .line,
        2
    );
    assert!(!rendered(&report).contains("repl.lang"));
    assert_eq!(submit(&mut s, ":reset").result.unwrap(), Event::Reset);
    assert_eq!(s.definition_count(), 0);
    commit(&mut s, "fn answer() -> i64 { return 7; }");
    value(&mut s, "answer()", "7");
}

#[test]
fn exactly_one_item_and_no_top_level_mutable_state_are_accepted() {
    let mut s = Session::default();
    for text in [
        "fn a() -> i64 { return 1; } fn b() -> i64 { return 2; }",
        "let mut global: i64 = 1;",
        "fn incomplete() -> i64 { return 1;",
        "/* unclosed",
    ] {
        let report = submit(&mut s, text);
        assert!(report.result.is_err(), "{text}");
        assert_eq!(s.definition_count(), 0);
        assert!(!rendered(&report).contains("__repl_eval_"));
    }
    assert_eq!(
        submit(&mut s, "// comment only\n").result.unwrap(),
        Event::Empty
    );
}

#[test]
fn original_expression_ranges_and_fixes_never_include_generated_preludes() {
    let mut s = Session::default();
    commit(&mut s, "// 😀\nfn answer() -> i64 { return 42; }");
    let report = submit(&mut s, "\n  missing");
    let diagnostic = &report.diagnostics[0];
    let source = report
        .sources
        .file(diagnostic.primary.span.source())
        .unwrap();
    assert_eq!(source.name(), "repl/2.lang");
    assert_eq!(source.text(), "\n  missing");
    assert_eq!(
        report.sources.slice(diagnostic.primary.span).unwrap(),
        "missing"
    );
    let position = source
        .position(diagnostic.primary.span.range().start)
        .unwrap();
    assert_eq!((position.line, position.column), (2, 3));
    let report = submit(&mut s, "fn bad() -> i64 { return 1 }");
    assert!(report.diagnostics.iter().any(|d| !d.fixes.is_empty()));
    for diagnostic in &report.diagnostics {
        for fix in &diagnostic.fixes {
            for edit in &fix.edits {
                assert_eq!(
                    report.sources.file(edit.span.source()).unwrap().name(),
                    "repl/3.lang"
                );
            }
        }
    }
}

#[test]
fn canonical_scalar_record_enum_and_empty_record_values_are_displayed() {
    let mut s = Session::default();
    value(&mut s, "-9223372036854775808", "-9223372036854775808");
    value(&mut s, "true", "true");
    value(&mut s, "false", "false");
    commit(&mut s, "struct Empty {}");
    value(&mut s, "Empty {}", "Empty {}");
    commit(&mut s, "struct Pair { a: bool, b: i64 }");
    commit(&mut s, "enum E { None, Some(Pair, bool) }");
    value(&mut s, "Pair { b: 42, a: true }", "Pair { a: true, b: 42 }");
    value(&mut s, "E::None", "E::None");
    value(
        &mut s,
        "E::Some(Pair { b: 42, a: false }, true)",
        "E::Some(Pair { a: false, b: 42 }, true)",
    );
    value(&mut s, "1 // trailing comment", "1");
}

#[test]
fn main_named_library_functions_and_user_wrapper_like_names_keep_ordinary_types() {
    let mut s = Session::default();
    commit(&mut s, "fn main(x: i64) -> bool { return x < 1; }");
    value(&mut s, "main(0)", "true");
    commit(&mut s, "fn __repl_eval_0() -> i64 { return 42; }");
    value(&mut s, "__repl_eval_0()", "42");
    let report = submit(&mut s, "__repl_eval_1()");
    assert!(report.result.is_err());
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == codes::UNKNOWN_NAME)
    );
}

#[test]
fn each_expression_has_fresh_fuel_and_items_do_not_execute() {
    let mut s = Session::default();
    let report = s.submit(
        b"fn answer() -> i64 { return 42; }",
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::with_limits(Limits {
            fuel: Some(0),
            ..Limits::default()
        }),
    );
    assert_eq!(report.result.unwrap(), Event::ItemCommitted);
    for _ in 0..3 {
        let report = s.submit(
            b"1 + 2",
            &HostFunctionRegistry::new(),
            &HostBindings::new(),
            Control::with_limits(Limits {
                fuel: Some(4),
                ..Limits::default()
            }),
        );
        assert_eq!(report.result.unwrap(), Event::Value("3".into()));
    }
    let report = s.submit(
        b"1 + 2",
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::with_limits(Limits {
            fuel: Some(3),
            ..Limits::default()
        }),
    );
    let Err(ReplError::Runtime(error)) = report.result else {
        panic!("expected runtime failure")
    };
    assert_eq!(error.reason, Reason::FuelExhausted);
    assert_eq!(error.usage.fuel, 3);
    value(&mut s, "answer()", "42");
}

#[test]
fn runtime_stacks_map_both_definitions_and_current_expression_to_original_sources() {
    let mut s = Session::default();
    commit(&mut s, "\nfn recur(x: i64) -> i64 { return recur(x); }");
    let report = s.submit(
        b"  recur(1)",
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::with_limits(Limits {
            frames: Some(3),
            ..Limits::default()
        }),
    );
    let Err(ReplError::Runtime(error)) = &report.result else {
        panic!("expected runtime failure")
    };
    assert_eq!(error.reason, Reason::FrameLimit);
    assert_eq!(error.stack.len(), 3);
    assert_eq!(
        report.sources.slice(error.fault.unwrap()).unwrap(),
        "recur(x)"
    );
    assert_eq!(
        report
            .sources
            .file(error.stack[0].span.unwrap().source())
            .unwrap()
            .name(),
        "repl/1.lang"
    );
    assert_eq!(error.function_name(error.stack[2]), "<expression>");
    assert_eq!(
        report.sources.slice(error.stack[2].span.unwrap()).unwrap(),
        "recur(1)"
    );
    assert_eq!(
        report
            .sources
            .file(error.stack[2].span.unwrap().source())
            .unwrap()
            .name(),
        "repl/2.lang"
    );
}

thread_local! {static EVENTS:RefCell<Vec<i64>>=const {RefCell::new(Vec::new())};}
extern "C" fn visit(value: i64) -> i64 {
    EVENTS.with_borrow_mut(|events| events.push(value));
    value
}
#[test]
fn trusted_host_effects_are_ordered_and_not_rolled_back_after_execution_failure() {
    EVENTS.with_borrow_mut(Vec::clear);
    let mut s = Session::default();
    commit(&mut s, "extern fn visit(x: i64) -> i64;");
    let mut hosts = HostFunctionRegistry::new();
    hosts.register_i64(HostFunctionId::new(7), visit).unwrap();
    let mut bindings = HostBindings::new();
    bindings.bind("visit", HostFunctionId::new(7)).unwrap();
    let report = s.submit(
        b"visit(1) + visit(2)",
        &hosts,
        &bindings,
        Control::with_limits(Limits {
            host_calls: Some(1),
            ..Limits::default()
        }),
    );
    let Err(ReplError::Runtime(error)) = &report.result else {
        panic!("expected runtime failure")
    };
    assert_eq!(error.reason, Reason::HostCallLimit);
    assert_eq!(error.usage.host_calls, 1);
    assert_eq!(
        report.sources.slice(error.fault.unwrap()).unwrap(),
        "visit(2)"
    );
    EVENTS.with_borrow(|events| assert_eq!(events, &[1]));
    assert_eq!(s.definition_count(), 1);
    let report = s.submit(b"visit(3)", &hosts, &bindings, Control::default());
    assert_eq!(report.result.unwrap(), Event::Value("3".into()));
    EVENTS.with_borrow(|events| assert_eq!(events, &[1, 3]));
}

#[test]
fn module_loading_needs_explicit_root_and_qualified_types_display_canonically() {
    let directory = std::env::temp_dir().join(format!("lang-repl-modules-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(
        directory.join("data.lang"),
        "pub struct R { a: bool, z: i64 } pub enum E { None, Some(R) }",
    )
    .unwrap();
    let mut s = Session::default();
    let report = submit(&mut s, "mod data;");
    assert!(report.result.is_err());
    assert_eq!(s.definition_count(), 0);
    let mut s = Session::new(Some(directory.clone()), ReplLimits::default());
    commit(&mut s, "mod data;");
    value(
        &mut s,
        "data::E::Some(data::R { z: 42, a: true })",
        "data::E::Some(data::R { a: true, z: 42 })",
    );
    let report = submit(&mut s, "mod missing;");
    assert!(report.result.is_err());
    assert!(rendered(&report).contains("repl/3.lang"));
    assert!(!rendered(&report).contains("repl.lang"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn preparation_compile_and_display_limits_do_not_commit_partial_items() {
    let mut limits = ReplLimits::default();
    limits.compile.modules.parse.lex.source_bytes = 64;
    let mut s = Session::new(None, limits);
    commit(&mut s, "fn a() -> i64 { return 1; }");
    commit(&mut s, "fn b() -> i64 { return 2; }");
    let report = submit(&mut s, "fn c() -> i64 { return 3; }");
    assert!(matches!(report.result, Err(ReplError::Diagnostics)));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == codes::RESOURCE_LIMIT)
    );
    assert_eq!(s.definition_count(), 2);
    let limits = ReplLimits {
        display_bytes: 2,
        ..ReplLimits::default()
    };
    let mut s = Session::new(None, limits);
    assert!(matches!(
        submit(&mut s, "123").result,
        Err(ReplError::DisplayLimit)
    ));
    assert_eq!(s.definition_count(), 0);
    assert_eq!(submit(&mut s, ":reset").result.unwrap(), Event::Reset);
}

#[test]
fn submission_buffer_scans_incrementally_through_nested_comments_and_delimiters() {
    let mut b = SubmissionBuffer::default();
    assert_eq!(b.push(b"fn f(\n", 1024, 256).unwrap(), Readiness::Continue);
    assert_eq!(
        b.push(b") -> i64 { /* outer { /* inner */ } */\n", 1024, 256)
            .unwrap(),
        Readiness::Continue
    );
    assert_eq!(
        b.push(b"return 1; }\n", 1024, 256).unwrap(),
        Readiness::Complete
    );
    let mut whole = SubmissionBuffer::default();
    assert_eq!(
        whole.push(b.bytes(), 1024, 256).unwrap(),
        Readiness::Complete
    );
    let text = b.bytes().to_vec();
    let mut chunks = SubmissionBuffer::default();
    for byte in &text {
        chunks.push(&[*byte], 1024, 256).unwrap();
    }
    assert_eq!(chunks.bytes(), whole.bytes());
    assert_eq!(chunks.push(b"", 1024, 256).unwrap(), Readiness::Complete);
    b.clear();
    assert_eq!(
        b.push(b"/* incomplete\n", 1024, 256).unwrap(),
        Readiness::Continue
    );
    b.clear();
    assert_eq!(b.push(b"(}", 1024, 256).unwrap(), Readiness::Complete);
    b.clear();
    assert_eq!(b.push(b"(((", 1024, 2).unwrap(), Readiness::Complete);
    b.clear();
    assert!(b.push(b"1234", 3, 256).is_err());
    assert!(b.bytes().is_empty());
}

#[test]
fn raw_encoding_and_comment_errors_never_commit_or_fabricate_spans() {
    let mut s = Session::default();
    for bytes in [&[0xff][..], &[0xef, 0xbb, 0xbf][..]] {
        let report = s.submit(
            bytes,
            &HostFunctionRegistry::new(),
            &HostBindings::new(),
            Control::default(),
        );
        assert!(matches!(report.result, Err(ReplError::Input(_))));
        assert_eq!(report.sources.iter().count(), 0);
        assert_eq!(s.definition_count(), 0);
    }
    let report = submit(&mut s, "// comment \0");
    assert!(matches!(report.result, Err(ReplError::Diagnostics)));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == codes::INVALID_TOKEN)
    );
    let report = submit(&mut s, "/* incomplete");
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == codes::UNTERMINATED_COMMENT)
    );
}

fn cli(input: &[u8], args: &[OsString]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lang"))
        .arg("repl")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn piped_repl_continues_after_errors_prints_values_and_never_prompts() {
    let output=cli(b"fn add_one(x: i64) -> i64 {\n return x + 1;\n}\nfn bad() -> i64 { return false; }\nadd_one(41)\ntrue\n:quit\n999\n",&[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"42\ntrue\n");
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("SRC0005"));
    assert!(!error.contains("__repl_eval_"));
    assert!(!error.contains("repl.lang"));
}

#[test]
fn repl_reset_removes_definitions_and_incomplete_eof_diagnoses_original_input() {
    let output = cli(
        b"fn answer() -> i64 { return 42; }\n:reset\nanswer()\n1 + 2\n",
        &[],
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"3\n");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("SRC0002")
    );
    let output = cli(b"fn incomplete() -> i64 {\n return 1;", &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("SRC0012"));
    assert!(error.contains("repl/1.lang"));
    assert!(!error.contains("__repl_eval_"));
    let output = cli(b"42", &[]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"42\n");
}

#[test]
fn repl_cli_requires_explicit_file_authority_and_rejects_positional_input() {
    let directory = std::env::temp_dir().join(format!("lang-repl-cli-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    std::fs::write(
        directory.join("math.lang"),
        "pub fn value() -> i64 { return 42; }",
    )
    .unwrap();
    let input = b"mod math;\nmath::value()\n";
    let output = cli(input, &[]);
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("SRC0013")
    );
    let output = cli(
        input,
        &[
            OsString::from("--source-root"),
            directory.clone().into_os_string(),
        ],
    );
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"42\n");
    assert!(output.stderr.is_empty());
    let output = cli(b"", &[OsString::from("file.lang")]);
    assert_eq!(output.status.code(), Some(2));
    std::fs::remove_dir_all(directory).unwrap();
}
