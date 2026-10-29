use lang::{
    frontend,
    package::{
        graph::{Graph, Limits},
        lock::{Lock, validate_encoding},
        source,
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

const TARGET: &str = "aarch64-apple-darwin";
const ROOT: &str = "{\"schema\":1,\"workspace\":{\"members\":[\"app\",\"helper\"]}}\n";
const APP: &str = "{\"schema\":1,\"package\":{\"name\":\"app\",\"version\":\"0.1.0\",\"edition\":\"2026\",\"entry\":\"src/main.lang\"},\"dependencies\":{\"helper\":{\"path\":\"../helper\",\"version\":\"0.1.0\"}}}\n";
const HELPER: &str = "{\"schema\":1,\"package\":{\"name\":\"helper\",\"version\":\"0.1.0\",\"edition\":\"2026\",\"entry\":\"src/main.lang\"}}\n";
const APP_SOURCE: &str =
    "mod helper; pub fn answer()->i64{return helper::answer();} fn main()->i64{return answer();}\n";
const HELPER_SOURCE: &str = "pub fn answer()->i64{return 42;}\n";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lang workspace {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let files = Self(path);
        for (path, text) in [
            ("Lang.json", ROOT),
            ("app/Lang.json", APP),
            ("helper/Lang.json", HELPER),
            ("app/src/main.lang", APP_SOURCE),
            ("helper/src/main.lang", HELPER_SOURCE),
        ] {
            files.write(path, text);
        }
        files
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn graph(&self, name: &str) -> Graph {
        Graph::resolve_selected(
            &self.0.join("Lang.json"),
            TARGET,
            Limits::default(),
            Some(name),
        )
        .unwrap()
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_lang"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }
    fn catalogue_with_idle(&self) {
        self.write(
            "Lang.json",
            r#"{"schema":1,"workspace":{"members":["idle","helper","app"]}}"#,
        );
        self.write("idle/Lang.json", &HELPER.replace("\"helper\"", "\"idle\""));
        // No idle entry exists: catalogue capture must not read inactive sources.
    }
    fn target_conditions(&self) {
        self.write(
            "Lang.json",
            r#"{"schema":1,"workspace":{"members":["win_support","app","mac_support","helper"]}}"#,
        );
        let mut app: Value = serde_json::from_str(APP).unwrap();
        for (name, target) in [
            ("mac_support", TARGET),
            ("win_support", "x86_64-pc-windows-msvc"),
        ] {
            app["dependencies"][name] =
                json!({"path":format!("../{name}"),"version":"0.1.0","targets":[target]});
            self.write(
                &format!("{name}/Lang.json"),
                &HELPER.replace("\"helper\"", &format!("\"{name}\"")),
            );
        }
        app["tests"] = json!(["tests/smoke.lang"]);
        self.write("app/Lang.json", &app.to_string());
        self.write(
            "app/tests/smoke.lang",
            "mod _package; fn main()->i64{return _package::answer() + -42;}\n",
        );
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn run(graph: &Graph) -> i64 {
    let analysis = source::analyze(graph, frontend::CompileLimits::default()).unwrap();
    assert!(analysis.result.is_ok(), "{:?}", analysis.diagnostics);
    let lang_semantics::analysis::Program::Copy(program) = analysis.result.unwrap() else {
        panic!("Copy fixture")
    };
    let program =
        frontend::compile_script(&program, &analysis.sources, Default::default()).unwrap();
    lang_vm::unit::execute_entry(
        &program,
        &lang_runtime::HostFunctionRegistry::new(),
        &lang_vm::unit::HostBindings::new(),
        Default::default(),
    )
    .unwrap()
    .value
    .as_i64()
    .unwrap()
}

#[test]
fn explicit_workspace_authority_executes_a_real_sibling_dependency() {
    let files = Files::new();
    assert_eq!(
        Graph::resolve(&files.0.join("app/Lang.json"), TARGET, Default::default())
            .unwrap_err()
            .code,
        "PKG0005"
    );
    assert_eq!(
        Graph::resolve(&files.0.join("Lang.json"), TARGET, Default::default())
            .unwrap_err()
            .code,
        "PKG0006"
    );
    let graph = files.graph("app");
    assert_eq!(
        graph.package(graph.root()).unwrap().root(),
        std::path::Path::new("app")
    );
    assert_eq!(graph.packages().len(), 2);
    let workspace = graph.workspace().unwrap();
    assert_eq!(
        workspace
            .members()
            .iter()
            .map(|member| member.package().name.as_str())
            .collect::<Vec<_>>(),
        ["app", "helper"]
    );
    assert_eq!(run(&graph), 42);
}

#[test]
fn named_selection_and_the_root_package_have_explicit_defaults() {
    let files = Files::new();
    let mut root: Value = serde_json::from_str(ROOT).unwrap();
    root["package"] =
        json!({"name":"root_app","version":"0.1.0","edition":"2026","entry":"src/main.lang"});
    files.write("Lang.json", &root.to_string());
    files.write("src/main.lang", "fn main()->i64{return 7;}\n");
    let graph = Graph::resolve(&files.0.join("Lang.json"), TARGET, Default::default()).unwrap();
    assert_eq!(run(&graph), 7);
    assert_eq!(
        graph.workspace().unwrap().selected_root(),
        std::path::Path::new(".")
    );
    assert_eq!(run(&files.graph("app")), 42);
    assert_eq!(
        Graph::resolve_selected(
            &files.0.join("Lang.json"),
            TARGET,
            Default::default(),
            Some("missing")
        )
        .unwrap_err()
        .code,
        "PKG0006"
    );
}

#[test]
fn schema_two_matches_independently_computed_golden_bytes() {
    let files = Files::new();
    let graph = files.graph("app");
    let lock = Lock::capture(&graph).unwrap();
    let expected = include_str!("fixtures/package-lock/schema2-workspace.json").replace(
        "<compiler_build_id>",
        &lang_codegen_clif::sdk::build_id(&TARGET.parse().unwrap()),
    );
    assert_eq!(std::str::from_utf8(lock.as_bytes()).unwrap(), expected);
    validate_encoding(lock.as_bytes()).unwrap();
    lock.verify(lock.as_bytes()).unwrap();
}

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn refused(output: Output, code: &str) {
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let value: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(value["code"], code);
}

#[test]
fn sibling_build_run_test_and_locked_offline_use_workspace_outputs() {
    let files = Files::new();
    let mut app: Value = serde_json::from_str(APP).unwrap();
    app["tests"] = json!(["tests/smoke.lang"]);
    files.write("app/Lang.json", &app.to_string());
    files.write(
        "app/tests/smoke.lang",
        "mod _package; fn main()->i64{return _package::answer() + -42;}\n",
    );
    for action in ["build", "run", "test"] {
        let output = success(files.run(&[action, "--package", "app"]));
        assert_eq!(
            output.stdout,
            if action == "run" {
                b"42\n".as_slice()
            } else {
                b""
            }
        );
    }
    let lock = fs::read(files.0.join("Lang.lock")).unwrap();
    let modified = fs::metadata(files.0.join("Lang.lock"))
        .unwrap()
        .modified()
        .unwrap();
    let generation = files
        .0
        .join("target/lang")
        .join(lang_codegen_clif::sdk::digest(&lock));
    for path in [
        "build/main.lbc",
        "test/tests/0.lbc",
        "build/inputs.lock",
        "test/inputs.lock",
    ] {
        assert!(generation.join(path).is_file(), "{path}");
    }
    for action in ["build", "run", "test"] {
        success(files.run(&[action, "--package", "app", "--locked", "--offline"]));
    }
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), lock);
    assert_eq!(
        fs::metadata(files.0.join("Lang.lock"))
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
    assert!(!files.0.join("app/Lang.lock").exists());
    assert!(!files.0.join("app/target").exists());
    refused(
        files.run(&["run", "--package", "missing", "--message-format", "json"]),
        "PKG0006",
    );
    refused(files.run(&["run", "--message-format", "json"]), "PKG0006");
    refused(
        files.run(&[
            "run",
            "--manifest-path",
            "app/Lang.json",
            "--package",
            "app",
            "--message-format",
            "json",
        ]),
        "PKG0005",
    );
}

#[test]
fn unselected_metadata_is_frozen_and_changed_replay_preserves_outputs() {
    let mut files = Files::new();
    files.catalogue_with_idle();
    success(files.run(&["build", "--package", "app"]));
    let graph = files.graph("app");
    let lock = Lock::capture(&graph).unwrap();
    let old = fs::read(files.0.join("Lang.lock")).unwrap();
    let artifact = files
        .0
        .join("target/lang")
        .join(lang_codegen_clif::sdk::digest(&old))
        .join("build/main.lbc");
    let old_artifact = fs::read(&artifact).unwrap();
    files.write(
        "idle/Lang.json",
        &HELPER
            .replace("\"helper\"", "\"idle\"")
            .replace("0.1.0", "0.1.1"),
    );
    for action in ["build", "run", "test"] {
        refused(
            files.run(&[
                action,
                "--package",
                "app",
                "--locked",
                "--offline",
                "--message-format",
                "json",
            ]),
            "PKG0007",
        );
    }
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), old);
    assert_eq!(fs::read(&artifact).unwrap(), old_artifact);
    assert_eq!(Lock::capture(&graph).unwrap().as_bytes(), lock.as_bytes());
    files.write("app/src/main.lang", "not valid source");
    fs::remove_file(files.0.join("helper/Lang.json")).unwrap();
    let relocated = files.0.with_extension("relocated");
    fs::rename(&files.0, &relocated).unwrap();
    files.0 = relocated;
    assert_eq!(run(&graph), 42);
    assert_eq!(Lock::capture(&graph).unwrap().as_bytes(), old);
}

#[test]
fn root_scope_excludes_unselected_members_and_all_catalogue_manifests() {
    let files = Files::new();
    let mut root: Value = serde_json::from_str(ROOT).unwrap();
    root["package"] =
        json!({"name":"root_app","version":"0.1.0","edition":"2026","entry":"main.lang"});
    files.write("Lang.json", &root.to_string());
    files.write("main.lang", "fn main()->i64{return 7;}\n");
    files.write("helper/src/main.lang", "invalid inactive member source");
    let graph = files.graph("root_app");
    let lock = Lock::capture(&graph).unwrap();
    let value: Value = serde_json::from_slice(lock.as_bytes()).unwrap();
    assert_eq!(value["workspace"]["members"].as_array().unwrap().len(), 3);
    assert_eq!(value["packages"].as_array().unwrap().len(), 1);
    assert_eq!(value["packages"][0]["sources"].as_array().unwrap().len(), 1);
    assert_eq!(run(&graph), 7);
    assert_eq!(success(files.run(&["run"])).stdout, b"7\n");
}

#[test]
fn catalogue_rejects_duplicate_identities_missing_packages_and_escapes() {
    let files = Files::new();
    for (manifest, code) in [
        (
            r#"{"schema":1,"workspace":{"members":["app","app/."]}}"#,
            "PKG0003",
        ),
        (
            r#"{"schema":1,"workspace":{"members":["app","missing"]}}"#,
            "PKG0005",
        ),
        (
            r#"{"schema":1,"workspace":{"members":["../outside"]}}"#,
            "PKG0003",
        ),
    ] {
        files.write("Lang.json", manifest);
        assert_eq!(
            Graph::resolve_selected(
                &files.0.join("Lang.json"),
                TARGET,
                Default::default(),
                Some("app")
            )
            .unwrap_err()
            .code,
            code
        );
    }
    files.write("Lang.json", ROOT);
    files.write("helper/Lang.json", &HELPER.replace("\"helper\"", "\"app\""));
    assert_eq!(
        Graph::resolve_selected(
            &files.0.join("Lang.json"),
            TARGET,
            Default::default(),
            Some("app")
        )
        .unwrap_err()
        .code,
        "PKG0006"
    );
    files.write(
        "helper/Lang.json",
        r#"{"schema":1,"workspace":{"members":["missing"]}}"#,
    );
    assert_eq!(
        Graph::resolve_selected(
            &files.0.join("Lang.json"),
            TARGET,
            Default::default(),
            Some("app")
        )
        .unwrap_err()
        .code,
        "PKG0006"
    );
}

#[test]
fn schema_dispatch_is_strict_and_never_grants_replay_authority() {
    let files = Files::new();
    let lock = Lock::capture(&files.graph("app")).unwrap();
    let original: Value = serde_json::from_slice(lock.as_bytes()).unwrap();
    for location in ["root", "workspace", "member"] {
        let mut value = original.clone();
        match location {
            "root" => value["extra"] = json!(true),
            "workspace" => value["workspace"]["extra"] = json!(true),
            _ => value["workspace"]["members"][0]["extra"] = json!(true),
        }
        assert_eq!(
            validate_encoding(&serde_json::to_vec(&value).unwrap())
                .unwrap_err()
                .code,
            "PKG0007"
        );
    }
    let mut value = original.clone();
    value.as_object_mut().unwrap().remove("workspace");
    assert_eq!(
        validate_encoding(&serde_json::to_vec(&value).unwrap())
            .unwrap_err()
            .code,
        "PKG0007"
    );
    let text = std::str::from_utf8(lock.as_bytes())
        .unwrap()
        .replace("\"schema\": 2", "\"schema\": 2, \"schema\": 2");
    assert_eq!(
        validate_encoding(text.as_bytes()).unwrap_err().code,
        "PKG0007"
    );
    value = original.clone();
    value["schema"] = json!(99);
    assert_eq!(
        validate_encoding(&serde_json::to_vec(&value).unwrap())
            .unwrap_err()
            .code,
        "PKG0002"
    );
    value = original;
    value["workspace"]["selected_root"] = json!("helper");
    let bytes = serde_json::to_vec(&value).unwrap();
    validate_encoding(&bytes).unwrap();
    assert_eq!(lock.verify(&bytes).unwrap_err().code, "PKG0007");
}

#[test]
fn unselected_manifest_hardlinks_cannot_be_replaced_by_lock_output() {
    let files = Files::new();
    files.catalogue_with_idle();
    let original = fs::read(files.0.join("idle/Lang.json")).unwrap();
    fs::hard_link(files.0.join("idle/Lang.json"), files.0.join("Lang.lock")).unwrap();
    refused(
        files.run(&["build", "--package", "app", "--message-format", "json"]),
        "PKG0003",
    );
    assert_eq!(fs::read(files.0.join("idle/Lang.json")).unwrap(), original);
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), original);
    assert!(!files.0.join("target").exists());
}

#[test]
fn custom_manifest_is_metadata_and_selection_is_part_of_lock_identity() {
    let files = Files::new();
    let mut root: Value = serde_json::from_str(ROOT).unwrap();
    root["package"] =
        json!({"name":"root_app","version":"0.1.0","edition":"2026","entry":"main.lang"});
    files.write("Workspace.lang", &root.to_string());
    files.write("main.lang", "fn main()->i64{return 7;}\n");
    let graph =
        Graph::resolve(&files.0.join("Workspace.lang"), TARGET, Default::default()).unwrap();
    let lock = Lock::capture(&graph).unwrap();
    assert_eq!(run(&graph), 7);
    let value: Value = serde_json::from_slice(lock.as_bytes()).unwrap();
    assert_eq!(value["workspace"]["manifest_path"], "Workspace.lang");
    assert_eq!(value["packages"][0]["sources"].as_array().unwrap().len(), 1);
    let graph = Graph::resolve_selected(
        &files.0.join("Workspace.lang"),
        TARGET,
        Default::default(),
        Some("app"),
    )
    .unwrap();
    let selected = Lock::capture(&graph).unwrap();
    assert_eq!(
        selected.verify(lock.as_bytes()).unwrap_err().code,
        "PKG0007"
    );
    assert_eq!(run(&graph), 42);
    let standalone = Graph::resolve_selected(
        &files.0.join("helper/Lang.json"),
        TARGET,
        Default::default(),
        Some("missing"),
    )
    .unwrap_err();
    assert_eq!(standalone.code, "PKG0006");
    let standalone = Graph::resolve_selected(
        &files.0.join("helper/Lang.json"),
        TARGET,
        Default::default(),
        Some("helper"),
    )
    .unwrap();
    let value: Value =
        serde_json::from_slice(Lock::capture(&standalone).unwrap().as_bytes()).unwrap();
    assert_eq!(value["schema"], 1);
    assert!(value.get("workspace").is_none());
}

#[cfg(unix)]
#[test]
fn member_alias_retargeting_changes_lock_and_duplicate_roots_are_refused() {
    use std::os::unix::fs::symlink;
    let files = Files::new();
    files.write(
        "Lang.json",
        r#"{"schema":1,"workspace":{"members":["app","chosen"]}}"#,
    );
    files.write("other/Lang.json", HELPER);
    symlink("helper", files.0.join("chosen")).unwrap();
    success(files.run(&["build", "--package", "app"]));
    let old = fs::read(files.0.join("Lang.lock")).unwrap();
    fs::remove_file(files.0.join("chosen")).unwrap();
    symlink("other", files.0.join("chosen")).unwrap();
    refused(
        files.run(&[
            "build",
            "--package",
            "app",
            "--locked",
            "--offline",
            "--message-format",
            "json",
        ]),
        "PKG0007",
    );
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), old);
    files.write(
        "Lang.json",
        r#"{"schema":1,"workspace":{"members":["app","chosen","other"]}}"#,
    );
    refused(
        files.run(&["build", "--package", "app", "--message-format", "json"]),
        "PKG0006",
    );
    files.write(
        "Lang.json",
        r#"{"schema":1,"workspace":{"members":["app","chosen"]}}"#,
    );
    fs::remove_file(files.0.join("chosen")).unwrap();
    symlink(std::env::temp_dir(), files.0.join("chosen")).unwrap();
    refused(
        files.run(&["build", "--package", "app", "--message-format", "json"]),
        "PKG0005",
    );
}

#[cfg(unix)]
#[test]
fn inactive_member_fifo_metadata_is_refused_without_blocking() {
    let files = Files::new();
    files.catalogue_with_idle();
    let path = files.0.join("idle/Lang.json");
    fs::remove_file(&path).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    refused(
        files.run(&["build", "--package", "app", "--message-format", "json"]),
        "PKG0005",
    );
}

#[test]
fn both_target_graphs_capture_exact_active_edges_without_inactive_source_reads() {
    for (target, active, inactive) in [
        (TARGET, "mac_support", "win_support"),
        ("x86_64-pc-windows-msvc", "win_support", "mac_support"),
    ] {
        let files = Files::new();
        files.target_conditions();
        files.write(&format!("{active}/src/main.lang"), HELPER_SOURCE);
        let resolve = || {
            Graph::resolve_selected(
                &files.0.join("Lang.json"),
                target,
                Default::default(),
                Some("app"),
            )
            .unwrap()
        };
        let graph = resolve();
        assert_eq!(graph.packages().len(), 3);
        let aliases: Vec<_> = graph
            .package(graph.root())
            .unwrap()
            .dependencies()
            .keys()
            .map(String::as_str)
            .collect();
        let mut expected = vec!["helper", active];
        expected.sort();
        assert_eq!(aliases, expected);
        let lock = Lock::capture(&graph).unwrap();
        let value: Value = serde_json::from_slice(lock.as_bytes()).unwrap();
        assert_eq!(value["target"], target);
        assert_eq!(value["workspace"]["members"].as_array().unwrap().len(), 4);
        assert_eq!(value["packages"].as_array().unwrap().len(), 3);
        assert_eq!(run(&graph), 42);
        files.write(
            &format!("{inactive}/src/main.lang"),
            "invalid inactive source",
        );
        assert_eq!(
            Lock::capture(&resolve()).unwrap().as_bytes(),
            lock.as_bytes()
        );
        files.write(&format!("{active}/src/main.lang"), "changed active source");
        let changed = Lock::capture(&resolve()).unwrap();
        assert_eq!(changed.verify(lock.as_bytes()).unwrap_err().code, "PKG0007");
    }
}

#[test]
fn both_target_cli_locks_relocate_replay_and_refuse_cross_target_or_version_changes() {
    for (target, other, active) in [
        (TARGET, "x86_64-pc-windows-msvc", "mac_support"),
        ("x86_64-pc-windows-msvc", TARGET, "win_support"),
    ] {
        let mut files = Files::new();
        files.target_conditions();
        for name in ["mac_support", "win_support"] {
            files.write(&format!("{name}/src/main.lang"), HELPER_SOURCE);
        }
        success(files.run(&["build", "--package", "app", "--target", target]));
        let old = fs::read(files.0.join("Lang.lock")).unwrap();
        let generation = format!(
            "target/lang/{}/build/main.lbc",
            lang_codegen_clif::sdk::digest(&old)
        );
        let artifact = fs::read(files.0.join(&generation)).unwrap();
        let relocated = files.0.with_extension("relocated");
        fs::rename(&files.0, &relocated).unwrap();
        files.0 = relocated;
        for action in ["build", "run", "test"] {
            let output = success(files.run(&[
                action,
                "--package",
                "app",
                "--target",
                target,
                "--locked",
                "--offline",
            ]));
            assert_eq!(
                output.stdout,
                if action == "run" {
                    b"42\n".as_slice()
                } else {
                    b""
                }
            );
            refused(
                files.run(&[
                    action,
                    "--package",
                    "app",
                    "--target",
                    other,
                    "--locked",
                    "--offline",
                    "--message-format",
                    "json",
                ]),
                "PKG0007",
            );
        }
        assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), old);
        assert_eq!(fs::read(files.0.join(&generation)).unwrap(), artifact);
        let mut app: Value =
            serde_json::from_str(&fs::read_to_string(files.0.join("app/Lang.json")).unwrap())
                .unwrap();
        app["dependencies"][active]["version"] = json!("0.1.1");
        files.write("app/Lang.json", &app.to_string());
        refused(
            files.run(&[
                "build",
                "--package",
                "app",
                "--target",
                target,
                "--locked",
                "--offline",
                "--message-format",
                "json",
            ]),
            "PKG0006",
        );
        assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), old);
        assert_eq!(fs::read(files.0.join(&generation)).unwrap(), artifact);
    }
}
