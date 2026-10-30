use super::*;
use crate::{frontend, package::graph::Graph};
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit::{Control, HostBindings, execute_entry};
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};

struct Files(PathBuf);
impl Files {
    fn new(test: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang checked test rename {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::create_dir(path.join("src")).unwrap();
        fs::create_dir(path.join("tests")).unwrap();
        fs::write(path.join("Lang.json"), r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"tests":["tests/check.lang"]}"#).unwrap();
        fs::write(
            path.join("src/main.lang"),
            "pub fn answer()->i64{return 42;}fn main()->i64{return 99;}",
        )
        .unwrap();
        fs::write(path.join("tests/check.lang"), test).unwrap();
        Self(path)
    }
    fn view(&self) -> View {
        let graph = Graph::resolve(
            &self.0.join("Lang.json"),
            "aarch64-apple-darwin",
            Default::default(),
        )
        .unwrap();
        let limits = Default::default();
        let snapshot = Snapshot::capture_test(&graph, 0, 7, &[], limits).unwrap();
        View {
            authority: graph.authority_root().to_path_buf(),
            graph,
            snapshot,
            overlays: Vec::new(),
            limits,
        }
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
        panic!("scalar fixture")
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
fn checked_rename_preserves_the_declared_test_root_and_initializer() {
    let original = "// 😀 test-only input\r\nmod _package;fn main()->i64{let mut unused:i64=_package::answer();unused=7;return _package::answer();}";
    let files = Files::new(original);
    let view = files.view();
    assert_eq!(run(&view.snapshot), 42);
    assert_eq!(
        run(&Snapshot::capture(&view.graph, 7, &[], view.limits).unwrap()),
        99
    );
    let plan = rename(
        &view,
        &view.authority.join("tests/check.lang"),
        original.find("unused").unwrap(),
        "_unused",
        7,
    )
    .unwrap();
    assert_eq!(plan.files.len(), 1);
    let spans = &plan.files[Path::new("tests/check.lang")];
    assert_eq!(spans.len(), 2);
    let mut replacement = original.to_owned();
    for span in spans.iter().rev() {
        replacement.replace_range(span.start..span.end, &plan.replacement);
    }
    assert_eq!(replacement, original.replace("unused", "_unused"));
    let overlays = [Overlay {
        path: "tests/check.lang".into(),
        version: 8,
        bytes: Arc::from(replacement.as_bytes()),
    }];
    let next = view
        .snapshot
        .recapture(&view.graph, 8, &overlays, view.limits)
        .unwrap();
    assert_eq!(run(&next), 42);
    assert_eq!(
        next.definition(
            Path::new("tests/check.lang"),
            replacement.find("_unused").unwrap(),
            8
        )
        .unwrap()
        .unwrap()
        .name,
        "_unused"
    );
    assert_eq!(
        fs::read_to_string(files.0.join("tests/check.lang")).unwrap(),
        original
    );
}

#[test]
fn test_rename_refuses_collisions_and_never_falls_back_to_the_package_entry() {
    let original = "mod _package;fn main()->i64{let mut unused:i64=1;let mut _unused:i64=2;return _package::answer();}";
    let files = Files::new(original);
    let view = files.view();
    let failure = rename(
        &view,
        &view.authority.join("tests/check.lang"),
        original.find("unused").unwrap(),
        "_unused",
        7,
    )
    .err()
    .expect("colliding rename must fail its proof");
    assert_eq!(failure.code, "PKG0003");
    assert_eq!(
        Snapshot::capture_test(&view.graph, 1, 7, &[], view.limits)
            .unwrap_err()
            .code,
        "PKG0003"
    );
    let invalid = [Overlay {
        path: "tests/check.lang".into(),
        version: 8,
        bytes: Arc::from(b"fn broken(".as_slice()),
    }];
    let snapshot = view
        .snapshot
        .recapture(&view.graph, 8, &invalid, view.limits)
        .unwrap();
    assert!(snapshot.analysis().result.is_err());
    assert!(snapshot.symbols().is_empty());
    assert_eq!(
        fs::read_to_string(files.0.join("tests/check.lang")).unwrap(),
        original
    );
}
