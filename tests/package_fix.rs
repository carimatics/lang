use lang::{
    frontend,
    package::{
        editor::{Overlay, Snapshot},
        fix::{Limits, Preview},
        graph::Graph,
    },
};
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit::{Control, HostBindings, execute_entry};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
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
        serde_json::json!(["src/main.lang", "tests/check.lang"])
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
