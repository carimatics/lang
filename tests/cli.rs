use lang::frontend::{self, AnalysisError, CompileLimits};
use lang_diagnostics::{SourceMap, codes};
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Output};
#[path = "support/source.rs"]
mod source;
use source::Files;

fn cli(args: impl IntoIterator<Item = OsString>) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lang"))
        .args(args)
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap()
}
fn run(files: &Files, options: &[&str]) -> Output {
    cli(std::iter::once(OsString::from("run"))
        .chain(options.iter().map(OsString::from))
        .chain(std::iter::once(
            files.0.join("source.lang").into_os_string(),
        )))
}
fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[test]
fn ownership_source_analysis_preserves_limit_and_type_error_snapshots() {
    let text = b"fn f(x: &i64) -> i64 { return 0; }";
    let mut limits = CompileLimits::default();
    limits.typing.types.work = 0;
    let limited = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("ownership.lang"),
        text,
        None,
        limits,
        None,
    );
    assert!(matches!(limited.result, Err(AnalysisError::Diagnostics)));
    assert_eq!(limited.diagnostics[0].code, codes::RESOURCE_LIMIT);
    limited.diagnostics[0].validate(&limited.sources).unwrap();
    let rejected = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("ownership.lang"),
        b"fn f() -> &i64 { return 0; }",
        None,
        Default::default(),
        None,
    );
    assert!(matches!(rejected.result, Err(AnalysisError::Diagnostics)));
    assert_eq!(rejected.diagnostics[0].code, codes::REGION_ESCAPE);
    rejected.diagnostics[0].validate(&rejected.sources).unwrap();
    let pending = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("ownership.lang"),
        text,
        None,
        Default::default(),
        None,
    );
    let Err(AnalysisError::Typing(lang_hir::program::CheckError::OwnershipProfile { span })) =
        pending.result
    else {
        panic!()
    };
    assert_eq!(pending.sources.slice(span).unwrap(), "&i64");
}

#[test]
fn ownership_body_type_errors_are_rendered_before_the_pending_execution_boundary() {
    let files = Files::new(
        "fn main() -> i64 { let mut value: core::Option<i64> = core::Option::Some(true); return 0; }",
    );
    let output = run(&files, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("SRC0014"), "{}", stderr(&output));
    assert!(stderr(&output).contains("bool"));
    assert!(stderr(&output).contains("i64"));
}

#[test]
fn move_diagnostics_reach_cli_and_move_limits_keep_source_snapshots() {
    let text = "fn bad(text: core::Text) -> i64 { let mut moved: core::Text = text; return core::text_len(text); } fn main() -> i64 { return 0; }";
    let files = Files::new(text);
    let output = run(&files, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("SRC0020"), "{}", stderr(&output));
    let mut limits = CompileLimits::default();
    limits.typing.moves.work = 0;
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("moves.lang"),
        text.as_bytes(),
        None,
        limits,
        None,
    );
    assert!(matches!(analysis.result, Err(AnalysisError::Diagnostics)));
    assert_eq!(analysis.diagnostics[0].code, codes::RESOURCE_LIMIT);
    analysis.diagnostics[0].validate(&analysis.sources).unwrap();
}

#[test]
fn scripts_print_one_signed_value_and_success_is_not_the_return_exit_code() {
    for (source, expected) in [
        ("fn main() -> i64 { return 42; }", "42\n"),
        ("fn main() -> i64 { return -7; }", "-7\n"),
        (
            "fn main() -> i64 { return 9223372036854775807 + 1; }",
            "-9223372036854775808\n",
        ),
    ] {
        let files = Files::new(source);
        assert!(files.unit().unit().entry.is_some());
        let output = run(&files, &[]);
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn multi_file_aggregate_scripts_resolve_from_source_root_instead_of_process_cwd() {
    let files = Files::new(
        "mod math; struct P { a: i64, b: i64 } enum E { Some(P) } fn main() -> i64 { match E::Some(P { b: 21, a: math::increment(20) }) { E::Some(P { b: second, a: first }) => { return first + second; }, } }",
    );
    std::fs::write(
        files.0.join("math.lang"),
        "pub fn increment(x: i64) -> i64 { return x + 1; }",
    )
    .unwrap();
    let output = run(&files, &[]);
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(output.stdout, b"42\n");
    let output = cli([
        OsString::from("run"),
        OsString::from("--source-root"),
        files.0.clone().into_os_string(),
        files.0.join("source.lang").into_os_string(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"42\n");
}

#[test]
fn missing_and_invalid_root_entries_have_source_diagnostic_0018() {
    for text in [
        "",
        "fn library() -> i64 { return 1; }",
        "fn main(x: i64) -> i64 { return x; }",
        "fn main() -> bool { return true; }",
    ] {
        let output = run(&Files::new(text), &[]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let error = stderr(&output);
        assert!(error.contains("SRC0018"), "{error}");
        assert!(error.contains("source.lang:"));
    }
}

#[test]
fn compilation_failures_do_not_execute_and_keep_stable_source_codes() {
    for (text, code) in [
        ("fn main() -> i64 { return false; }", "SRC0005"),
        ("fn main() -> i64 { return missing; }", "SRC0002"),
        ("fn main() -> i64 { return 1 }", "SRC0012"),
    ] {
        let output = run(&Files::new(text), &[]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(stderr(&output).contains(code), "{}", stderr(&output));
    }
}

#[test]
fn raw_invalid_encoding_is_reported_without_a_fabricated_source_location() {
    let files = Files::new("");
    std::fs::write(files.0.join("source.lang"), [0xff]).unwrap();
    let output = run(&files, &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("SRC0010"));
    assert!(stderr(&output).contains("bytes 0..1"));
    assert!(!stderr(&output).contains("-->"));
    assert!(output.stdout.is_empty());
}

#[test]
fn cli_runtime_failures_report_fuel_and_only_entered_source_frames() {
    let files =
        Files::new("fn main() -> i64 { return leaf(1); } fn leaf(x: i64) -> i64 { return x; }");
    let output = run(&files, &["--frames", "1"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = stderr(&output);
    assert!(
        error.contains("FrameLimit (fuel 2, host calls 0)"),
        "{error}"
    );
    assert!(error.contains("at main"));
    assert!(!error.contains("at leaf"));
    assert!(error.contains("source.lang\":1:"));
    let output = run(&files, &["--fuel", "0"]);
    assert!(stderr(&output).contains("FuelExhausted (fuel 0"));
    let output = run(&files, &["--cells", "0"]);
    assert!(stderr(&output).contains("LiveCellLimit (fuel 0"));
}

#[test]
fn cli_never_binds_declared_host_imports_implicitly() {
    let output = run(
        &Files::new("extern fn environment(x: i64) -> i64; fn main() -> i64 { return 42; }"),
        &[],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("MissingImport"));
}

#[test]
fn invalid_usage_exits_two_and_help_succeeds() {
    for args in [
        vec![],
        vec!["run"],
        vec!["run", "--unknown"],
        vec!["run", "--fuel", "bad", "file.lang"],
        vec!["run", "--fuel", "1", "--fuel", "2", "file.lang"],
        vec!["run", "first.lang", "second.lang"],
        vec!["abc"],
    ] {
        let output = cli(args.into_iter().map(OsString::from));
        assert_eq!(output.status.code(), Some(2), "{}", stderr(&output));
        assert!(stderr(&output).contains("usage:"));
        assert!(output.stdout.is_empty());
    }
    let output = cli([OsString::from("--help")]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.starts_with(b"usage:"));
    assert!(output.stderr.is_empty());
}

#[test]
fn original_two_integer_prototype_remains_available() {
    let output = cli([OsString::from("20"), OsString::from("22")]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"bytecode: 42\nnative:   42\n");
}

#[test]
fn shared_frontend_retains_resource_diagnostics_and_snapshots_on_failure() {
    let mut limits = CompileLimits::default();
    limits.modules.parse.work = 0;
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("test.lang"),
        b"fn main() -> i64 { return 1; }",
        None,
        limits,
        None,
    );
    assert!(matches!(analysis.result, Err(AnalysisError::Diagnostics)));
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::RESOURCE_LIMIT)
    );
    for d in analysis.diagnostics {
        assert!(d.render(&analysis.sources).unwrap().contains("test.lang:"));
    }
}
