use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
struct Files(PathBuf);
impl Files {
    fn new(source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lang package lint {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let files = Self(root);
        files.write("Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
        files.write("src/main.lang", source);
        files
    }
    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lang"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn lint(&self, deny: bool) -> Output {
        let mut args = vec![
            "lint",
            "--target",
            "aarch64-apple-darwin",
            "--offline",
            "--message-format",
            "json",
        ];
        if deny {
            args.push("--deny-warnings");
        }
        self.run(&args)
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn diagnostics(output: &Output) -> Vec<Value> {
    assert!(output.stdout.is_empty());
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}
#[test]
fn unread_bindings_use_resolved_ids_original_spans_and_warning_denial() {
    let source = "// 未読 unused\nfn f(unused:i64,_quiet:i64)->i64{let mut written:i64=0;written=1;return 42;}fn g(unused:i64)->i64{return unused;}";
    let files = Files::new(source);
    let output = files.lint(false);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let d = diagnostics(&output);
    assert_eq!(d.len(), 2);
    for (diagnostic, name) in d.iter().zip(["unused", "written"]) {
        assert_eq!(diagnostic["schema"], 1);
        assert_eq!(diagnostic["code"], "LNT0001");
        assert_eq!(diagnostic["severity"], "warning");
        assert_eq!(
            std::path::Path::new(diagnostic["path"].as_str().unwrap()),
            std::path::Path::new("src/main.lang")
        );
        let start = diagnostic["span"]["start"].as_u64().unwrap() as usize;
        let end = diagnostic["span"]["end"].as_u64().unwrap() as usize;
        assert_eq!(&source[start..end], name);
    }
    let denied = files.lint(true);
    assert_eq!(denied.status.code(), Some(1));
    assert_eq!(diagnostics(&denied), d);
    assert!(!files.0.join("Lang.lock").exists());
    assert!(!files.0.join("target").exists());
}
#[test]
fn fields_comments_and_other_function_locals_do_not_count_as_local_reads() {
    let files = Files::new(
        "struct Item{missing:i64}fn f(missing:i64,item:Item)->i64{return item.missing;}fn g(missing:i64)->i64{return missing;} // missing\n",
    );
    let output = files.lint(true);
    assert_eq!(output.status.code(), Some(1));
    let d = diagnostics(&output);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0]["code"], "LNT0001");
    assert!(d[0]["message"].as_str().unwrap().contains("missing"));
}
#[test]
fn existing_unreachable_code_keeps_its_identity_and_can_be_denied() {
    let files = Files::new("fn main()->i64{return 42;return 0;}");
    let output = files.lint(false);
    assert_eq!(output.status.code(), Some(0));
    let d = diagnostics(&output);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0]["code"], "SRC0019");
    assert_eq!(files.lint(true).status.code(), Some(1));
}
#[test]
fn pattern_bindings_share_the_resolved_rule_and_underscore_policy() {
    let files = Files::new(
        "enum E{Some(i64,i64)}fn f(e:E)->i64{match e{E::Some(unread,_quiet)=>{return 42;},}}",
    );
    let output = files.lint(true);
    assert_eq!(output.status.code(), Some(1));
    let d = diagnostics(&output);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0]["code"], "LNT0001");
    assert!(d[0]["message"].as_str().unwrap().contains("unread"));
}
#[test]
fn package_tests_and_dependencies_are_checked_once_without_execution_or_publication() {
    let files = Files::new(
        "mod helper;extern fn never_call(value:i64)->i64;pub fn answer()->i64{return helper::answer();}fn main()->i64{return never_call(42);}",
    );
    files.write("Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"helper","version":"0.1.0"}},"tests":["tests/check.lang"]}"#);
    files.write("helper/Lang.json", r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
    files.write(
        "helper/src/main.lang",
        "pub fn answer()->i64{let mut ignored:i64=7;return 42;}",
    );
    files.write(
        "tests/check.lang",
        "mod _package;fn main()->i64{let mut test_unused:i64=0;return _package::answer();}",
    );
    let built = files.run(&["build", "--target", "aarch64-apple-darwin"]);
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let lock = fs::read(files.0.join("Lang.lock")).unwrap();
    let artifact = files.0.join(format!(
        "target/lang/{}/build/main.lbc",
        lang_codegen_clif::sdk::digest(&lock)
    ));
    let old_artifact = fs::read(&artifact).unwrap();
    let output = files.run(&[
        "lint",
        "--target",
        "aarch64-apple-darwin",
        "--locked",
        "--offline",
        "--message-format",
        "json",
    ]);
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let d = diagnostics(&output);
    assert_eq!(d.len(), 2);
    assert_eq!(
        std::path::Path::new(d[0]["path"].as_str().unwrap()),
        std::path::Path::new("helper/src/main.lang")
    );
    assert_eq!(
        std::path::Path::new(d[1]["path"].as_str().unwrap()),
        std::path::Path::new("tests/check.lang")
    );
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), lock);
    assert_eq!(fs::read(&artifact).unwrap(), old_artifact);
    files.write("tests/check.lang", "fn main()->i64{return unknown;}");
    let stale = files.run(&[
        "lint",
        "--target",
        "aarch64-apple-darwin",
        "--locked",
        "--message-format",
        "json",
    ]);
    assert_eq!(stale.status.code(), Some(1));
    assert_eq!(diagnostics(&stale)[0]["code"], "PKG0007");
    let invalid = files.lint(false);
    assert_eq!(invalid.status.code(), Some(1));
    assert!(
        diagnostics(&invalid)
            .iter()
            .any(|d| d["path"].as_str().map(std::path::Path::new)
                == Some(std::path::Path::new("tests/check.lang"))
                && d["severity"] == "error")
    );
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), lock);
}
#[test]
fn warning_free_library_needs_no_main_and_options_are_action_specific() {
    let files = Files::new("pub fn answer()->i64{return 42;}");
    let output = files.lint(true);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(diagnostics(&output).is_empty());
    for args in [
        vec!["lint", "--check"],
        vec!["fmt", "--deny-warnings"],
        vec!["build", "--deny-warnings"],
    ] {
        assert_eq!(files.run(&args).status.code(), Some(2));
    }
}
