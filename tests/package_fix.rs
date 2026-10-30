use lang::{
    frontend,
    package::{
        editor::{Overlay, Snapshot},
        fix::{Limits, Plan, Preview},
        graph::Graph,
    },
};
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit::{Control, HostBindings, execute_entry};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

struct Files(PathBuf);
impl Files {
    fn new(main: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lang checked fix {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let files = Self(root);
        files.write("Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
        files.write("src/main.lang", main);
        files
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn graph(&self) -> Arc<Graph> {
        Arc::new(
            Graph::resolve(
                &self.0.join("Lang.json"),
                "aarch64-apple-darwin",
                Default::default(),
            )
            .unwrap(),
        )
    }
    fn directory(&self) -> cap_std::fs::Dir {
        cap_std::fs::Dir::open_ambient_dir(&self.0, cap_std::ambient_authority()).unwrap()
    }
    fn cli(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lang"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn no_stages(&self) {
        for path in ["src", "tests", "helper/src"] {
            if let Ok(entries) = fs::read_dir(self.0.join(path)) {
                assert!(
                    entries
                        .map(|e| e.unwrap())
                        .all(|e| !e.file_name().to_string_lossy().starts_with(".lang-format-"))
                );
            }
        }
    }
    fn dependency_and_test(&self) {
        self.write("Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"helper","version":"0.1.0"}},"tests":["tests/check.lang"]}"#);
        self.write("helper/Lang.json", r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
        self.write(
            "helper/src/main.lang",
            "pub fn answer()->i64{let mut dep_unused:i64=1;return 42;}",
        );
        self.write(
            "tests/check.lang",
            "mod _package;fn main()->i64{let mut unused_test:i64=1;return _package::answer()+-42;}",
        );
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn run(snapshot: &Snapshot) -> i64 {
    let analysis = snapshot.analysis();
    let lang_semantics::analysis::Program::Copy(typed) = analysis.result.as_ref().unwrap() else {
        panic!("Copy fixture")
    };
    let program = frontend::compile_script(typed, &analysis.sources, Default::default()).unwrap();
    execute_entry(
        &program,
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::default(),
    )
    .unwrap()
    .value
    .as_i64()
    .unwrap()
}

fn cli_success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    output
}

#[test]
fn cli_previews_then_applies_checked_roots_preserving_execution_and_lock() {
    let main = "// 😀 keep CRLF\r\nmod helper;pub fn answer()->i64{let mut unused:i64=3;unused=4;return helper::answer();}fn main()->i64{return answer();}\r\n";
    let files = Files::new(main);
    files.dependency_and_test();
    assert_eq!(cli_success(files.cli(&["run"])).stdout, b"42\n");
    cli_success(files.cli(&["test"]));
    let original_test = fs::read(files.0.join("tests/check.lang")).unwrap();
    let lock = fs::read(files.0.join("Lang.lock")).unwrap();
    let dependency = fs::read(files.0.join("helper/src/main.lang")).unwrap();
    let preview: Value = serde_json::from_slice(
        &cli_success(files.cli(&["fix", "--message-format", "json"])).stdout,
    )
    .unwrap();
    assert_eq!(preview["outcome"], "preview");
    assert_eq!(preview["changed"], 2);
    assert_eq!(preview["applied"], 0);
    assert_eq!(
        fs::read(files.0.join("src/main.lang")).unwrap(),
        main.as_bytes()
    );
    assert_eq!(
        fs::read(files.0.join("tests/check.lang")).unwrap(),
        original_test
    );
    let human = String::from_utf8(cli_success(files.cli(&["fix"])).stdout).unwrap();
    assert!(human.contains("fix preview: 2 files changed, 0 applied"));
    assert!(human.contains("LNT0001 -> _unused"));
    let applied: Value = serde_json::from_slice(
        &cli_success(files.cli(&[
            "fix",
            "--apply",
            "--locked",
            "--offline",
            "--message-format",
            "json",
        ]))
        .stdout,
    )
    .unwrap();
    assert_eq!(applied["outcome"], "applied");
    assert_eq!(applied["applied"], 2);
    assert_eq!(applied["edits"], preview["edits"]);
    assert_eq!(
        fs::read_to_string(files.0.join("src/main.lang")).unwrap(),
        main.replace("unused", "_unused")
    );
    assert_eq!(
        fs::read(files.0.join("helper/src/main.lang")).unwrap(),
        dependency
    );
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), lock);
    let stale = files.cli(&["run", "--locked", "--message-format", "json"]);
    assert_eq!(stale.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&stale.stderr).unwrap()["code"],
        "PKG0007"
    );
    assert_eq!(cli_success(files.cli(&["run"])).stdout, b"42\n");
    cli_success(files.cli(&["test"]));
    let repeat: Value = serde_json::from_slice(
        &cli_success(files.cli(&["fix", "--apply", "--message-format", "json"])).stdout,
    )
    .unwrap();
    assert_eq!(repeat["applied"], 0);
    assert_eq!(repeat["edits"], serde_json::json!([]));
    files.no_stages();
}

#[test]
fn fresh_fix_plan_refuses_stale_proof_inputs_and_foreign_authority() {
    let main = "mod helper;pub fn answer()->i64{let mut unused:i64=1;return helper::answer();}fn main()->i64{return answer();}";
    for changed in [
        "src/main.lang",
        "tests/check.lang",
        "helper/src/main.lang",
        "helper/Lang.json",
        "Lang.json",
    ] {
        let files = Files::new(main);
        files.dependency_and_test();
        let plan = Plan::capture(files.graph(), Limits::default()).unwrap();
        let expected: Vec<_> = plan
            .preview()
            .changes()
            .iter()
            .map(|c| {
                (
                    c.path().to_path_buf(),
                    fs::read(files.0.join(c.path())).unwrap(),
                )
            })
            .collect();
        let mut bytes = fs::read(files.0.join(changed)).unwrap();
        bytes.push(b' ');
        fs::write(files.0.join(changed), &bytes).unwrap();
        assert_eq!(plan.apply(&files.directory()).unwrap_err().code, "PKG0007");
        assert_eq!(fs::read(files.0.join(changed)).unwrap(), bytes);
        for (path, original) in expected {
            if path != std::path::Path::new(changed) {
                assert_eq!(fs::read(files.0.join(path)).unwrap(), original);
            }
        }
        files.no_stages();
    }
    let a = Files::new("fn main()->i64{let mut unused:i64=1;return 42;}");
    let b = Files::new("fn main()->i64{let mut unused:i64=1;return 42;}");
    let plan = Plan::capture(a.graph(), Limits::default()).unwrap();
    assert_eq!(plan.apply(&b.directory()).unwrap_err().code, "PKG0007");
    assert_eq!(
        fs::read(a.0.join("src/main.lang")).unwrap(),
        fs::read(b.0.join("src/main.lang")).unwrap()
    );
}

#[test]
fn fix_refuses_readonly_and_replaced_identity_before_any_publication() {
    let main = "mod helper;pub fn answer()->i64{let mut unused:i64=1;return helper::answer();}fn main()->i64{return answer();}";
    let files = Files::new(main);
    files.dependency_and_test();
    let test = files.0.join("tests/check.lang");
    let old = fs::metadata(&test).unwrap().permissions();
    let mut readonly = old.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&test, readonly).unwrap();
    let result = Plan::capture(files.graph(), Limits::default())
        .unwrap()
        .apply(&files.directory());
    fs::set_permissions(&test, old).unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.code, "PKG0005");
    assert!(error.message.contains("after 0 replacements"));
    assert_eq!(
        fs::read_to_string(files.0.join("src/main.lang")).unwrap(),
        main
    );
    let plan = Plan::capture(files.graph(), Limits::default()).unwrap();
    let bytes = fs::read(&test).unwrap();
    fs::rename(&test, files.0.join("tests/retained.txt")).unwrap();
    fs::write(&test, &bytes).unwrap();
    assert_eq!(plan.apply(&files.directory()).unwrap_err().code, "PKG0007");
    assert_eq!(
        fs::read_to_string(files.0.join("src/main.lang")).unwrap(),
        main
    );
    files.no_stages();
}

#[test]
fn fix_cli_refuses_invalid_options_and_semantic_conflicts_without_writes() {
    let main = "fn main()->i64{let mut unused:i64=1;let mut _unused:i64=2;return _unused;}";
    let files = Files::new(main);
    for flags in [
        vec!["--apply", "--apply"],
        vec!["--check"],
        vec!["--fuel", "1"],
        vec!["report.json"],
        vec!["--native"],
        vec!["--message-format", "yaml"],
    ] {
        let mut args = vec!["fix", "--message-format", "json"];
        args.extend(flags);
        let out = files.cli(&args);
        assert_eq!(out.status.code(), Some(2));
        assert!(out.stdout.is_empty());
        assert_eq!(
            serde_json::from_slice::<Value>(&out.stderr).unwrap()["code"],
            "CLI0001"
        );
    }
    let out = files.cli(&["fix", "--apply", "--message-format", "json"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stderr).unwrap()["code"],
        "PKG0003"
    );
    assert_eq!(
        fs::read_to_string(files.0.join("src/main.lang")).unwrap(),
        main
    );
    assert!(!files.0.join("Lang.lock").exists());
    files.no_stages();
}

#[cfg(unix)]
#[test]
fn fix_staging_failure_aliases_and_permissions_use_the_shared_publication_guard() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let main = "mod helper;pub fn answer()->i64{let mut unused:i64=1;return helper::answer();}fn main()->i64{return answer();}";
    let files = Files::new(main);
    files.dependency_and_test();
    let dir = files.0.join("tests");
    let old = fs::metadata(&dir).unwrap().permissions();
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o555)).unwrap();
    let result = Plan::capture(files.graph(), Limits::default())
        .unwrap()
        .apply(&files.directory());
    fs::set_permissions(&dir, old).unwrap();
    let error = result.unwrap_err();
    assert_eq!(error.code, "PKG0005");
    assert!(error.message.contains("after 0 replacements"));
    assert_eq!(
        fs::read_to_string(files.0.join("src/main.lang")).unwrap(),
        main
    );
    files.no_stages();
    let path = files.0.join("src/main.lang");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
    Plan::capture(files.graph(), Limits::default())
        .unwrap()
        .apply(&files.directory())
        .unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o750
    );
    fs::rename(&path, files.0.join("src/original.txt")).unwrap();
    symlink("original.txt", &path).unwrap();
    let out = files.cli(&["fix", "--apply", "--message-format", "json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stderr).unwrap()["code"],
        "PKG0003"
    );
    files.no_stages();
}

#[test]
fn apply_report_limit_and_hardlink_alias_are_refused_before_writes() {
    let mut main = String::new();
    for i in 0..9 {
        main.push_str(&format!("mod item{i};"));
    }
    main.push_str("fn main()->i64{let mut unused:i64=1;return 42;}");
    let files = Files::new(&main);
    for i in 0..9 {
        files.write(
            &format!("src/item{i}.lang"),
            "pub fn value()->i64{let mut unused:i64=1;return 0;}",
        );
    }
    let preview = Preview::capture(files.graph(), Limits::default()).unwrap();
    assert_eq!(preview.changes().len(), 10);
    let size = serde_json::to_vec(&preview).unwrap().len();
    let mut limits = Limits {
        report_bytes: size,
        ..Limits::default()
    };
    assert_eq!(
        Plan::capture(files.graph(), limits).unwrap_err().code,
        "FIX0002"
    );
    assert_eq!(
        fs::read_to_string(files.0.join("src/main.lang")).unwrap(),
        main
    );
    limits.report_bytes += 1;
    Plan::capture(files.graph(), limits).unwrap();
    fs::hard_link(files.0.join("src/main.lang"), files.0.join("alias.txt")).unwrap();
    let out = files.cli(&["fix", "--apply", "--message-format", "json"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stderr).unwrap()["code"],
        "PKG0003"
    );
    assert_eq!(fs::read_to_string(files.0.join("alias.txt")).unwrap(), main);
    files.no_stages();
}

#[test]
fn preview_composes_original_spans_and_preserves_entry_test_and_dependency_execution() {
    let main = "// 😀 byte spans\r\nmod helper;pub fn answer(unread:i64)->i64{let mut first:i64=helper::answer();first=7;let mut second:i64=3;return helper::answer();}fn main()->i64{return answer(9);}";
    let test = "mod _package;mod extra;fn main()->i64{let mut test_unused:i64=extra::touch();return _package::answer(0);}";
    let extra = "pub fn touch()->i64{let mut nested:i64=4;return 1;}";
    let dep = "pub fn answer()->i64{let mut dependency_unused:i64=1;return 42;}";
    let files = Files::new(main);
    files.write("Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"helper","version":"0.1.0"}},"tests":["tests/check.lang"]}"#);
    files.write("helper/Lang.json", r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
    files.write("helper/src/main.lang", dep);
    files.write("tests/check.lang", test);
    files.write("tests/extra.lang", extra);
    let graph = files.graph();
    let before = Snapshot::capture(&graph, 0, &[], Default::default()).unwrap();
    let before_test = Snapshot::capture_test(&graph, 0, 0, &[], Default::default()).unwrap();
    assert_eq!((run(&before), run(&before_test)), (42, 42));
    let preview = Preview::capture(graph.clone(), Limits::default()).unwrap();
    assert_eq!(preview.changes().len(), 3);
    assert_eq!(preview.edits().len(), 6);
    let report = serde_json::to_value(&preview).unwrap();
    assert_eq!(report["schema"], 1);
    assert_eq!(report["applied"], 0);
    assert_eq!(report["changed"], 3);
    assert_eq!(
        report["roots"],
        serde_json::json!([
            PathBuf::from("src").join("main.lang"),
            PathBuf::from("tests").join("check.lang")
        ])
    );
    for edit in preview.edits() {
        let original = fs::read_to_string(files.0.join(&edit.path)).unwrap();
        assert_eq!(
            edit.replacement,
            format!("_{}", &original[edit.span.start..edit.span.end])
        );
        assert!(!edit.path.starts_with("helper"));
    }
    assert!(
        preview
            .edits()
            .windows(2)
            .all(|p| (&p[0].path, p[0].span.start) < (&p[1].path, p[1].span.start))
    );
    let overlays: Vec<_> = preview
        .changes()
        .iter()
        .map(|change| Overlay {
            path: change.path().to_path_buf(),
            version: 1,
            bytes: Arc::from(change.text().as_bytes()),
        })
        .collect();
    assert_eq!(
        run(&Snapshot::capture(&graph, 1, &overlays, Default::default()).unwrap()),
        42
    );
    assert_eq!(
        run(&Snapshot::capture_test(&graph, 0, 1, &overlays, Default::default()).unwrap()),
        42
    );
    for (path, original) in [
        ("src/main.lang", main),
        ("tests/check.lang", test),
        ("tests/extra.lang", extra),
        ("helper/src/main.lang", dep),
    ] {
        assert_eq!(fs::read_to_string(files.0.join(path)).unwrap(), original);
    }
    assert_eq!(
        serde_json::to_value(Preview::capture(graph, Limits::default()).unwrap()).unwrap(),
        report
    );
    // Publish only inside this test fixture to exercise repeat-run idempotence.
    for change in preview.changes() {
        fs::write(files.0.join(change.path()), change.text()).unwrap();
    }
    assert!(
        Preview::capture(files.graph(), Limits::default())
            .unwrap()
            .edits()
            .is_empty()
    );
    assert_eq!(
        fs::read_to_string(files.0.join("helper/src/main.lang")).unwrap(),
        dep
    );
}

#[test]
fn aggregate_limits_and_exact_report_boundary_refuse_without_writes() {
    let main = "fn main()->i64{let mut first:i64=1;let mut second:i64=2;return 42;}";
    let files = Files::new(main);
    let preview = Preview::capture(files.graph(), Limits::default()).unwrap();
    let size = serde_json::to_vec(&preview).unwrap().len();
    Preview::capture(
        files.graph(),
        Limits {
            report_bytes: size,
            ..Limits::default()
        },
    )
    .unwrap();
    for limits in [
        Limits {
            roots: 0,
            ..Limits::default()
        },
        Limits {
            fixes: 1,
            ..Limits::default()
        },
        Limits {
            edits: 1,
            ..Limits::default()
        },
        Limits {
            analysis_bytes: 0,
            ..Limits::default()
        },
        Limits {
            work: 60_000_000,
            ..Limits::default()
        },
        Limits {
            rewritten_bytes: main.len() + 1,
            ..Limits::default()
        },
        Limits {
            report_bytes: size - 1,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            Preview::capture(files.graph(), limits).unwrap_err().code,
            "FIX0002"
        );
        assert_eq!(
            fs::read_to_string(files.0.join("src/main.lang")).unwrap(),
            main
        );
    }
    let one = Files::new("fn main()->i64{let mut unused:i64=1;return 42;}");
    Preview::capture(
        one.graph(),
        Limits {
            work: 60_000_000,
            ..Limits::default()
        },
    )
    .unwrap();
}

#[test]
fn collision_and_invalid_declared_test_never_publish_partial_fixes() {
    let main = "fn main()->i64{let mut first:i64=1;let mut collision:i64=2;let mut _collision:i64=3;return 42;}";
    let files = Files::new(main);
    assert_eq!(
        Preview::capture(files.graph(), Limits::default())
            .unwrap_err()
            .code,
        "PKG0003"
    );
    assert_eq!(
        fs::read_to_string(files.0.join("src/main.lang")).unwrap(),
        main
    );
    let mut manifest: Value =
        serde_json::from_str(&fs::read_to_string(files.0.join("Lang.json")).unwrap()).unwrap();
    manifest["tests"] = serde_json::json!(["tests/check.lang"]);
    files.write("Lang.json", &manifest.to_string());
    files.write("tests/check.lang", "fn broken(");
    assert_eq!(
        Preview::capture(files.graph(), Limits::default())
            .unwrap_err()
            .code,
        "FIX0001"
    );
    assert_eq!(
        fs::read_to_string(files.0.join("src/main.lang")).unwrap(),
        main
    );
}

#[test]
fn selected_workspace_member_is_fixed_without_touching_other_members() {
    let files = Files::new("fn main()->i64{return 0;}");
    files.write(
        "Lang.json",
        r#"{"schema":1,"workspace":{"members":["one","two"]}}"#,
    );
    for (directory, name) in [("one", "first"), ("two", "second")] {
        files.write(&format!("{directory}/Lang.json"), &format!(r#"{{"schema":1,"package":{{"name":"{name}","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}}}"#));
        files.write(
            &format!("{directory}/src/main.lang"),
            "fn main()->i64{let mut unused:i64=1;return 42;}",
        );
    }
    let graph = Arc::new(
        Graph::resolve_selected(
            &files.0.join("Lang.json"),
            "aarch64-apple-darwin",
            Default::default(),
            Some("first"),
        )
        .unwrap(),
    );
    let preview = Preview::capture(graph, Limits::default()).unwrap();
    assert_eq!(preview.changes().len(), 1);
    assert_eq!(
        preview.changes()[0].path(),
        std::path::Path::new("one/src/main.lang")
    );
    assert_eq!(
        fs::read_to_string(files.0.join("two/src/main.lang")).unwrap(),
        "fn main()->i64{let mut unused:i64=1;return 42;}"
    );
}

#[test]
fn unread_initializer_exhausts_fuel_before_and_after_the_fix() {
    let files = Files::new(
        "fn spin()->i64{while true{}return 0;}fn main()->i64{let mut unused:i64=spin();return 42;}",
    );
    let graph = files.graph();
    let preview = Preview::capture(graph.clone(), Limits::default()).unwrap();
    let overlays: Vec<_> = preview
        .changes()
        .iter()
        .map(|change| Overlay {
            path: change.path().to_path_buf(),
            version: 1,
            bytes: Arc::from(change.text().as_bytes()),
        })
        .collect();
    for inputs in [&[][..], overlays.as_slice()] {
        let snapshot = Snapshot::capture(&graph, 1, inputs, Default::default()).unwrap();
        let analysis = snapshot.analysis();
        let lang_semantics::analysis::Program::Copy(typed) = analysis.result.as_ref().unwrap()
        else {
            panic!("Copy fixture")
        };
        let program =
            frontend::compile_script(typed, &analysis.sources, Default::default()).unwrap();
        assert_eq!(
            execute_entry(
                &program,
                &HostFunctionRegistry::new(),
                &HostBindings::new(),
                Control::with_limits(lang_runtime::execution::Limits {
                    fuel: Some(100),
                    ..Default::default()
                })
            )
            .unwrap_err()
            .reason,
            lang_vm::unit::Reason::FuelExhausted
        );
    }
    assert!(
        preview.changes()[0]
            .text()
            .contains("let mut _unused:i64=spin();")
    );
}
