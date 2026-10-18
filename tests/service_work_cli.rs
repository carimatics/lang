use std::ffi::OsString;
use std::io::Write;
use std::process::{Command, Output, Stdio};

#[path = "support/source.rs"]
#[allow(
    dead_code,
    reason = "Shared fixture helper also exposes MIR-only conveniences."
)]
mod source;
use source::Files;

const BUFFER: &str = include_str!("../examples/analysis/native_buffer_work.lang");

fn cli(args: impl IntoIterator<Item = OsString>, input: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lang"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    } else {
        drop(child.stdin.take());
    }
    child.wait_with_output().unwrap()
}
fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}
fn compile(files: &Files) {
    let result = cli(
        [
            "compile".into(),
            "--output".into(),
            files.0.join("program.lbc").into_os_string(),
            files.0.join("source.lang").into_os_string(),
        ],
        None,
    );
    assert!(result.status.success(), "{}", stderr(&result));
}
fn run(files: &Files, options: &[&str], artifact: bool) -> Output {
    let mut args = vec![OsString::from("run")];
    if artifact {
        args.push("--artifact".into());
    }
    args.extend(options.iter().map(OsString::from));
    args.push(
        files
            .0
            .join(if artifact {
                "program.lbc"
            } else {
                "source.lang"
            })
            .into_os_string(),
    );
    cli(args, None)
}

#[test]
fn source_and_loaded_buffers_preserve_numeric_budgets_and_guest_faults() {
    let files = Files::new(BUFFER);
    compile(&files);
    for (limit, used, requested) in [(0, 0, 1), (1, 1, 2), (11, 11, 2), (36, 36, 1)] {
        let option = limit.to_string();
        let source = run(&files, &["--service-work", &option], false);
        let loaded = run(&files, &["--service-work", &option], true);
        assert_eq!(source.status.code(), Some(1), "{}", stderr(&source));
        assert_eq!(source.stdout, b"");
        assert_eq!(source.status.code(), loaded.status.code());
        assert_eq!(source.stdout, loaded.stdout);
        assert_eq!(source.stderr, loaded.stderr);
        let message = stderr(&source);
        assert!(
            message.contains(&format!(
                "ServiceWorkLimit {{ limit: {limit}, used: {used}, requested: {requested} }}"
            )),
            "{message}"
        );
        assert!(
            message.contains(&format!("service work {used}")),
            "{message}"
        );
        assert!(message.contains("source.lang\":"), "{message}");
        if limit == 11 {
            assert!(message.find("at clone_text").unwrap() < message.find("at main").unwrap());
        }
    }
    for options in [
        vec![],
        vec!["--service-work", "37"],
        vec!["--service-work", "18446744073709551615"],
    ] {
        for artifact in [false, true] {
            let result = run(&files, &options, artifact);
            assert!(result.status.success(), "{}", stderr(&result));
            assert_eq!(result.stdout, b"67\n");
            assert_eq!(result.stderr, b"");
        }
    }
}

#[test]
fn unmetered_unused_functions_refuse_finite_source_and_artifact_execution() {
    let text = format!(
        "{BUFFER}\n{}",
        include_str!("../examples/analysis/native_vec_nested.lang").replacen(
            "fn main()",
            "fn unused()",
            1
        )
    );
    let files = Files::new(&text);
    compile(&files);
    for limit in ["0", "18446744073709551615"] {
        for artifact in [false, true] {
            let result = run(&files, &["--service-work", limit], artifact);
            assert_eq!(result.status.code(), Some(1));
            assert_eq!(result.stdout, b"");
            let message = stderr(&result);
            assert!(message.contains("UnsupportedServiceWork"), "{message}");
            assert!(message.contains(&format!("limit: {limit}")), "{message}");
            assert!(message.contains("service work 0"), "{message}");
        }
    }
    let result = run(&files, &[], false);
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(result.stdout, b"67\n");
}

#[test]
fn option_validation_and_zero_work_copy_programs_keep_cli_exit_contracts() {
    let files = Files::new("fn main() -> i64 { return 7; }");
    compile(&files);
    for artifact in [false, true] {
        let result = run(&files, &["--service-work", "0"], artifact);
        assert!(result.status.success(), "{}", stderr(&result));
        assert_eq!(result.stdout, b"7\n");
    }
    for options in [
        vec!["--service-work", "-1"],
        vec!["--service-work", "18446744073709551616"],
        vec!["--service-work", "abc"],
        vec!["--service-work", "0", "--service-work", "1"],
    ] {
        let result = run(&files, &options, false);
        assert_eq!(result.status.code(), Some(2), "{}", stderr(&result));
        assert_eq!(result.stdout, b"");
        assert!(stderr(&result).contains("--service-work"));
    }
    for args in [
        vec!["run", "--service-work"],
        vec!["repl", "--service-work"],
        vec!["repl", "--service-work", "-1"],
        vec!["check", "--service-work", "0"],
        vec!["compile", "--service-work", "0"],
    ] {
        let result = cli(args.into_iter().map(OsString::from), None);
        assert_eq!(result.status.code(), Some(2));
        assert!(stderr(&result).contains("--service-work"));
    }
    let help = cli(["--help".into()], None);
    assert!(help.status.success());
    assert_eq!(
        String::from_utf8(help.stdout)
            .unwrap()
            .matches("[--service-work N]")
            .count(),
        3
    );
}

#[test]
fn repl_budgets_reset_per_submission_and_failures_preserve_original_sources() {
    let input = "fn payload() -> i64 {\nlet mut b: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));\nlet mut p: core::Unit = core::result_unwrap::<core::Unit, core::BufferError>(core::bytes_push(b, 65));\nreturn core::bytes_len(b);\n}\npayload()\npayload()\n7\n:quit\n";
    let result = cli(
        ["repl".into(), "--service-work".into(), "1".into()],
        Some(input),
    );
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(result.stdout, b"1\n1\n7\n");
    assert_eq!(result.stderr, b"");
    let result = cli(
        ["repl".into(), "--service-work".into(), "0".into()],
        Some(input),
    );
    assert!(result.status.success(), "{}", stderr(&result));
    assert_eq!(result.stdout, b"7\n");
    let message = stderr(&result);
    assert!(
        message.contains("ServiceWorkLimit { limit: 0, used: 0, requested: 1 }"),
        "{message}"
    );
    assert!(message.contains("service work 0"), "{message}");
    assert!(message.contains("repl/1.lang\":"), "{message}");
    assert_eq!(message.matches("(fuel ").count(), 2, "{message}");
    assert!(message.contains("at payload"), "{message}");
    assert!(!message.contains("__repl_eval_"), "{message}");
}
