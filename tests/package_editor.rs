use lang::package::{
    editor::{Limits, Overlay, Snapshot},
    graph::Graph,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
struct Files(PathBuf);
const ROOT: &str = "// 😀 snapshot\nmod helper;fn answer()->i64{return 0;}fn main()->i64{return helper::answer();}fn shadow()->i64{let mut answer:i64=0;answer=1;return answer;}";
impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let f = Self(std::env::temp_dir().join(format!(
            "lang package editor {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        f.write("Lang.json",r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"helper","version":"0.1.0"}}}"#);
        f.write("src/main.lang", ROOT);
        f.write("helper/Lang.json",r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
        f.write("helper/src/main.lang", "pub fn answer()->i64{return 42;}");
        f
    }
    fn write(&self, p: &str, text: &str) {
        let p = self.0.join(p);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn graph(&self) -> Graph {
        Graph::resolve(
            &self.0.join("Lang.json"),
            "aarch64-apple-darwin",
            Default::default(),
        )
        .unwrap()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn dependency_navigation_uses_the_frozen_epoch_and_excludes_shadowing_local_reads_and_writes() {
    let mut f = Files::new();
    let graph = f.graph();
    let snapshot = Snapshot::capture(&graph, 1, &[], Default::default()).unwrap();
    assert!(
        snapshot.analysis().result.is_ok(),
        "{:?}",
        snapshot.analysis().diagnostics
    );
    let offset = ROOT.find("helper::answer").unwrap() + "helper::".len();
    let target = snapshot
        .definition(Path::new("src/main.lang"), offset, 1)
        .unwrap()
        .unwrap();
    assert_eq!(target.name, "answer");
    assert_eq!(target.location.path, Path::new("helper/src/main.lang"));
    assert_eq!(target.location.span.start, 7);
    assert_eq!(target.location.span.end, 13);
    for offset in [
        ROOT.find("answer=1").unwrap(),
        ROOT.rfind("answer;").unwrap(),
    ] {
        let local = snapshot
            .definition(Path::new("src/main.lang"), offset, 1)
            .unwrap()
            .unwrap();
        assert_eq!(local.kind, lang::package::editor::SymbolKind::Variable);
        assert_eq!(local.location.span.start, ROOT.find("answer:i64").unwrap());
    }
    assert_eq!(
        snapshot
            .definition(Path::new("src/main.lang"), offset, 2)
            .unwrap_err()
            .code,
        "PKG0007"
    );
    assert_eq!(
        snapshot
            .definition(Path::new("src/main.lang"), ROOT.find('😀').unwrap() + 1, 1)
            .unwrap_err()
            .code,
        "PKG0003"
    );
    let moved = f.0.with_extension("relocated");
    fs::rename(&f.0, &moved).unwrap();
    f.0 = moved;
    f.write("helper/src/main.lang", "broken source");
    assert_eq!(
        snapshot
            .definition(Path::new("src/main.lang"), offset, 1)
            .unwrap()
            .unwrap()
            .location,
        target.location
    );
    let overlays = [
        Overlay {
            path: "src/main.lang".into(),
            version: 3,
            bytes: Arc::from(
                ROOT.replace("helper::answer()", "helper::renamed()")
                    .as_bytes(),
            ),
        },
        Overlay {
            path: "helper/src/main.lang".into(),
            version: 5,
            bytes: Arc::from(b"pub fn renamed()->i64{return 42;}".as_slice()),
        },
    ];
    let changed = Snapshot::capture(&graph, 2, &overlays, Default::default()).unwrap();
    assert!(
        changed.analysis().result.is_ok(),
        "{:?}",
        changed.analysis().diagnostics
    );
    assert_eq!(
        changed
            .definition(Path::new("src/main.lang"), offset, 2)
            .unwrap()
            .unwrap()
            .name,
        "renamed"
    );
    assert!(changed.matches(
        2,
        &BTreeMap::from([
            (PathBuf::from("src/main.lang"), 3),
            (PathBuf::from("helper/src/main.lang"), 5)
        ])
    ));
    assert!(!changed.matches(1, changed.versions()));
    assert_eq!(
        snapshot
            .definition(Path::new("src/main.lang"), offset, 1)
            .unwrap()
            .unwrap()
            .name,
        "answer"
    );
}

#[test]
fn invalid_overlay_never_publishes_old_semantic_bindings_and_admission_is_bounded() {
    let f = Files::new();
    let graph = f.graph();
    let invalid = [Overlay {
        path: "src/main.lang".into(),
        version: 1,
        bytes: Arc::from(b"fn broken( {".as_slice()),
    }];
    let s = Snapshot::capture(&graph, 1, &invalid, Default::default()).unwrap();
    assert!(s.analysis().result.is_err());
    assert!(s.symbols().is_empty());
    assert!(s.occurrences().is_empty());
    assert!(
        s.definition(Path::new("src/main.lang"), 0, 1)
            .unwrap()
            .is_none()
    );
    for path in ["Lang.json", "../outside.lang", "missing.lang"] {
        let o = [Overlay {
            path: path.into(),
            ..invalid[0].clone()
        }];
        assert_eq!(
            Snapshot::capture(&graph, 1, &o, Default::default())
                .unwrap_err()
                .code,
            "PKG0003"
        );
    }
    let duplicate = [invalid[0].clone(), invalid[0].clone()];
    assert_eq!(
        Snapshot::capture(&graph, 1, &duplicate, Default::default())
            .unwrap_err()
            .code,
        "PKG0003"
    );
    for limits in [
        Limits {
            overlays: 0,
            ..Default::default()
        },
        Limits {
            overlay_bytes: invalid[0].bytes.len() - 1,
            ..Default::default()
        },
    ] {
        assert_eq!(
            Snapshot::capture(&graph, 1, &invalid, limits)
                .unwrap_err()
                .code,
            "PKG0004"
        );
    }
    for limits in [
        Limits {
            symbols: 0,
            ..Default::default()
        },
        Limits {
            occurrences: 0,
            ..Default::default()
        },
        Limits {
            work: 0,
            ..Default::default()
        },
    ] {
        assert_eq!(
            Snapshot::capture(&graph, 1, &[], limits).unwrap_err().code,
            "PKG0004"
        );
    }
}
#[test]
fn imported_function_nominal_type_and_enum_case_use_resolved_declarations_and_privacy() {
    use lang::package::editor::SymbolKind;
    let f = Files::new();
    let root = "mod helper;use helper::answer;pub enum Flag{Ready}pub fn identity(value:helper::Shared)->helper::Shared{return value;}pub fn flag()->Flag{return Flag::Ready;}fn main()->i64{return answer();}";
    f.write("src/main.lang", root);
    f.write(
        "helper/src/main.lang",
        "pub fn answer()->i64{return 42;}pub struct Shared{value:i64}",
    );
    let s = Snapshot::capture(&f.graph(), 1, &[], Default::default()).unwrap();
    assert!(
        s.analysis().result.is_ok(),
        "{:?}",
        s.analysis().diagnostics
    );
    for (offset, name, kind, path) in [
        (
            root.rfind("answer()").unwrap(),
            "answer",
            SymbolKind::Function,
            "helper/src/main.lang",
        ),
        (
            root.find("helper::Shared").unwrap() + "helper::".len(),
            "Shared",
            SymbolKind::Record,
            "helper/src/main.lang",
        ),
        (
            root.find("Flag::Ready").unwrap() + "Flag::".len(),
            "Ready",
            SymbolKind::EnumCase,
            "src/main.lang",
        ),
    ] {
        let target = s
            .definition(Path::new("src/main.lang"), offset, 1)
            .unwrap()
            .unwrap();
        assert_eq!(target.name, name);
        assert_eq!(target.kind, kind);
        assert_eq!(target.location.path, Path::new(path));
    }
    f.write(
        "helper/src/main.lang",
        "fn answer()->i64{return 42;}pub struct Shared{value:i64}",
    );
    let refused = Snapshot::capture(&f.graph(), 2, &[], Default::default()).unwrap();
    assert!(refused.analysis().result.is_err());
    assert!(refused.symbols().is_empty());
    assert!(
        refused
            .definition(
                Path::new("src/main.lang"),
                root.rfind("answer()").unwrap(),
                2
            )
            .unwrap()
            .is_none()
    );
}
#[test]
fn logical_index_bytes_accept_the_exact_limit_and_refuse_one_below() {
    let f = Files::new();
    let graph = f.graph();
    let s = Snapshot::capture(&graph, 1, &[], Default::default()).unwrap();
    let bytes = s.indexed_bytes();
    assert!(bytes > 0);
    let exact = Snapshot::capture(
        &graph,
        1,
        &[],
        Limits {
            index_bytes: bytes,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(exact.indexed_bytes(), bytes);
    assert_eq!(
        Snapshot::capture(
            &graph,
            1,
            &[],
            Limits {
                index_bytes: bytes - 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        "PKG0004"
    );
}
#[test]
fn input_epoch_owns_unused_and_manifest_bytes_and_refuses_metadata_as_source() {
    let f = Files::new();
    f.write("src/unused.lang", "// unused input");
    let manifest = fs::read(f.0.join("Lang.json")).unwrap();
    let graph = f.graph();
    let snapshot = Snapshot::capture(&graph, 1, &[], Default::default()).unwrap();
    drop(graph);
    f.write("Lang.json", "changed metadata");
    f.write("src/unused.lang", "changed unused input");
    assert_eq!(snapshot.input(Path::new("Lang.json")).unwrap(), manifest);
    assert_eq!(
        snapshot.input(Path::new("src/unused.lang")).unwrap(),
        b"// unused input"
    );
    fs::write(f.0.join("Lang.json"), &manifest).unwrap();
    fs::remove_file(f.0.join("src/main.lang")).unwrap();
    fs::hard_link(f.0.join("Lang.json"), f.0.join("src/main.lang")).unwrap();
    let overlays = [Overlay {
        path: "src/main.lang".into(),
        version: 1,
        bytes: Arc::from(b"fn main()->i64{return 42;}".as_slice()),
    }];
    assert_eq!(
        Snapshot::capture(&f.graph(), 2, &overlays, Default::default())
            .unwrap_err()
            .code,
        "PKG0003"
    );
}
