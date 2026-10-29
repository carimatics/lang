use lang::package::{format::Plan, graph::Graph};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang package format {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir(path.join("src")).unwrap();
        fs::write(path.join("Lang.json"), r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#).unwrap();
        fs::write(
            path.join("src/main.lang"),
            "// コメント\nfn main()->i64{return 42;}\n",
        )
        .unwrap();
        Self(path)
    }
    fn graph(&self) -> Graph {
        Graph::resolve(
            &self.0.join("Lang.json"),
            "aarch64-apple-darwin",
            Default::default(),
        )
        .unwrap()
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lang"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn directory(&self) -> cap_std::fs::Dir {
        cap_std::fs::Dir::open_ambient_dir(&self.0, cap_std::ambient_authority()).unwrap()
    }
    fn workspace(&self) {
        self.write("Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"workspace":{"members":["helper","idle"]},"dependencies":{"helper":{"path":"helper","version":"0.1.0"}},"tests":["tests/smoke.lang"]}"#);
        self.write("src/main.lang", "// コメント\nmod helper;pub fn answer()->i64{return helper::answer();}fn main()->i64{return answer();}\n");
        self.write("helper/Lang.json", r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
        self.write(
            "helper/src/main.lang",
            "// 依存\npub fn answer()->i64{return 42;}\n",
        );
        self.write("idle/Lang.json", r#"{"schema":1,"package":{"name":"idle","version":"0.1.0","edition":"2026","entry":"missing.lang"}}"#);
        self.write(
            "tests/smoke.lang",
            "mod _package;fn main()->i64{return _package::answer() + -42;}\n",
        );
    }
    fn originals(&self) -> Vec<(&'static str, Vec<u8>)> {
        ["src/main.lang", "helper/src/main.lang", "tests/smoke.lang"]
            .into_iter()
            .map(|p| (p, fs::read(self.0.join(p)).unwrap()))
            .collect()
    }
    fn no_stages(&self) {
        for directory in ["src", "helper/src", "tests"] {
            if let Ok(entries) = fs::read_dir(self.0.join(directory)) {
                assert!(
                    entries
                        .map(|e| e.unwrap())
                        .all(|e| !e.file_name().to_string_lossy().starts_with(".lang-format-"))
                );
            }
        }
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn cst_plan_preserves_comments_without_mutating_source_and_refuses_invalid_input() {
    let files = Files::new();
    let original = fs::read(files.0.join("src/main.lang")).unwrap();
    let plan = Plan::capture(&files.graph()).unwrap();
    assert_eq!(plan.changes().len(), 1);
    assert!(plan.changes()[0].text().contains("// コメント"));
    assert!(plan.changes()[0].text().contains("    return 42;"));
    assert_eq!(fs::read(files.0.join("src/main.lang")).unwrap(), original);
    fs::write(files.0.join("src/main.lang"), "fn main( { malformed").unwrap();
    let error = Plan::capture(&files.graph()).unwrap_err();
    assert!(!error.diagnostics().is_empty());
    assert!(
        error
            .diagnostics()
            .iter()
            .all(|d| d.path.as_deref() == Some("src/main.lang") && d.span.is_some())
    );
}
fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn code(output: &Output, expected: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["code"], expected);
}

#[test]
fn real_cli_formats_root_dependency_tests_then_replays_and_checks_without_writes() {
    let files = Files::new();
    files.workspace();
    success(files.run(&["build", "--target", "aarch64-apple-darwin"]));
    let lock = fs::read(files.0.join("Lang.lock")).unwrap();
    let originals = files.originals();
    let checked = files.run(&[
        "fmt",
        "--check",
        "--target",
        "aarch64-apple-darwin",
        "--message-format",
        "json",
    ]);
    assert_eq!(checked.status.code(), Some(1));
    assert!(checked.stdout.is_empty());
    let diagnostics: Vec<serde_json::Value> = String::from_utf8(checked.stderr)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(diagnostics.len(), 3);
    assert!(diagnostics.iter().all(|d| d["code"] == "FMT0001"));
    for (path, bytes) in &originals {
        assert_eq!(fs::read(files.0.join(path)).unwrap(), *bytes);
    }
    let formatted = success(files.run(&[
        "fmt",
        "--target",
        "aarch64-apple-darwin",
        "--locked",
        "--offline",
    ]));
    assert!(formatted.stdout.is_empty());
    assert!(formatted.stderr.is_empty());
    for (path, bytes) in &originals {
        assert_ne!(fs::read(files.0.join(path)).unwrap(), *bytes);
    }
    assert!(
        fs::read_to_string(files.0.join("src/main.lang"))
            .unwrap()
            .contains("// コメント")
    );
    assert!(
        fs::read_to_string(files.0.join("helper/src/main.lang"))
            .unwrap()
            .contains("// 依存")
    );
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), lock);
    code(
        &files.run(&[
            "run",
            "--target",
            "aarch64-apple-darwin",
            "--locked",
            "--offline",
            "--message-format",
            "json",
        ]),
        "PKG0007",
    );
    assert_eq!(
        success(files.run(&["run", "--target", "aarch64-apple-darwin"])).stdout,
        b"42\n"
    );
    success(files.run(&[
        "test",
        "--target",
        "aarch64-apple-darwin",
        "--locked",
        "--offline",
    ]));
    let before = files.originals();
    success(files.run(&["fmt", "--check"]));
    success(files.run(&["fmt"]));
    for (path, bytes) in before {
        assert_eq!(fs::read(files.0.join(path)).unwrap(), bytes);
    }
    assert!(!files.0.join("idle/missing.lang").exists());
    files.no_stages();
}

#[test]
fn changed_sources_and_any_manifest_refuse_the_complete_plan_before_publication() {
    for changed in ["src/main.lang", "helper/Lang.json", "idle/Lang.json"] {
        let files = Files::new();
        files.workspace();
        let originals = files.originals();
        let plan = Plan::capture(&files.graph()).unwrap();
        let mut bytes = fs::read(files.0.join(changed)).unwrap();
        bytes.extend_from_slice(b"\n// changed");
        fs::write(files.0.join(changed), &bytes).unwrap();
        assert_eq!(plan.apply(&files.directory()).unwrap_err().code, "PKG0007");
        assert_eq!(fs::read(files.0.join(changed)).unwrap(), bytes);
        for (path, before) in originals {
            if path != changed {
                assert_eq!(fs::read(files.0.join(path)).unwrap(), before);
            }
        }
        files.no_stages();
    }
}

#[test]
fn invalid_syntax_and_utf8_do_not_publish_other_valid_edits() {
    let files = Files::new();
    files.workspace();
    let original = fs::read(files.0.join("src/main.lang")).unwrap();
    files.write("tests/smoke.lang", "fn main( { malformed");
    let output = files.run(&["fmt", "--message-format", "json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(files.0.join("src/main.lang")).unwrap(), original);
    files.no_stages();
    fs::write(files.0.join("tests/smoke.lang"), [0xff]).unwrap();
    let output = files.run(&["fmt", "--message-format", "json"]);
    code(&output, "FMT0002");
    let value: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["span"]["start"], 0);
    assert_eq!(value["span"]["end"], 1);
    assert_eq!(fs::read(files.0.join("src/main.lang")).unwrap(), original);
    files.no_stages();
}

#[test]
fn readonly_source_refuses_before_any_replacement_and_cleans_stages() {
    let files = Files::new();
    files.workspace();
    let originals = files.originals();
    let path = files.0.join("tests/smoke.lang");
    let permissions = fs::metadata(&path).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&path, readonly).unwrap();
    let output = files.run(&["fmt", "--message-format", "json"]);
    fs::set_permissions(&path, permissions).unwrap();
    code(&output, "PKG0005");
    for (path, bytes) in originals {
        assert_eq!(fs::read(files.0.join(path)).unwrap(), bytes);
    }
    files.no_stages();
}

#[cfg(unix)]
#[test]
fn source_links_are_refused_and_original_permissions_and_root_authority_are_retained() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let mut files = Files::new();
    let root = files.directory();
    let plan = Plan::capture(&files.graph()).unwrap();
    let moved = files.0.with_extension("relocated");
    fs::rename(&files.0, &moved).unwrap();
    fs::create_dir(&files.0).unwrap();
    fs::write(files.0.join("sentinel"), b"other root").unwrap();
    plan.apply(&root).unwrap();
    assert!(
        fs::read_to_string(moved.join("src/main.lang"))
            .unwrap()
            .contains("    return 42;")
    );
    assert_eq!(fs::read(files.0.join("sentinel")).unwrap(), b"other root");
    fs::remove_dir_all(&files.0).unwrap();
    files.0 = moved;
    fs::set_permissions(
        files.0.join("src/main.lang"),
        fs::Permissions::from_mode(0o750),
    )
    .unwrap();
    files.write("src/main.lang", "fn main()->i64{return 42;}");
    success(files.run(&["fmt"]));
    assert_eq!(
        fs::metadata(files.0.join("src/main.lang"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o750
    );
    fs::rename(
        files.0.join("src/main.lang"),
        files.0.join("src/original.txt"),
    )
    .unwrap();
    symlink("original.txt", files.0.join("src/main.lang")).unwrap();
    code(&files.run(&["fmt", "--message-format", "json"]), "PKG0003");
    files.no_stages();
}

#[cfg(unix)]
#[test]
fn staging_failure_cleans_other_prepared_files_before_source_publication() {
    use std::os::unix::fs::PermissionsExt;
    let files = Files::new();
    files.workspace();
    let originals = files.originals();
    let directory = files.0.join("tests");
    let old = fs::metadata(&directory).unwrap().permissions();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o555)).unwrap();
    let output = files.run(&["fmt", "--message-format", "json"]);
    fs::set_permissions(&directory, old).unwrap();
    code(&output, "PKG0005");
    for (path, bytes) in originals {
        assert_eq!(fs::read(files.0.join(path)).unwrap(), bytes);
    }
    files.no_stages();
}
