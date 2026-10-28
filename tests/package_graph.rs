use lang::package::graph::{Graph, Limits};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const TARGET: &str = "aarch64-apple-darwin";
const SOURCE: &[u8] = b"fn main()->i64{return 42;}\n";
static NEXT: AtomicU64 = AtomicU64::new(0);

struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lang package graph {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let files = Self(path);
        files.package("", "app", json!({}));
        files
    }
    fn root(&self) -> PathBuf {
        self.0.join("root")
    }
    fn manifest(&self) -> PathBuf {
        self.root().join("Lang.json")
    }
    fn package(&self, relative: &str, name: &str, dependencies: Value) {
        let root = self.root().join(relative);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("Lang.json"),
            serde_json::to_vec(&json!({
                "schema": 1,
                "package": {"name":name,"version":"0.1.0","edition":"2026","entry":"src/main.lang"},
                "dependencies": dependencies
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(root.join("src/main.lang"), SOURCE).unwrap();
    }
    fn resolve(&self) -> Result<Graph, lang::package::Diagnostic> {
        Graph::resolve(&self.manifest(), TARGET, Limits::default())
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn dep(path: &str) -> Value {
    json!({"path":path,"version":"0.1.0"})
}

#[cfg(unix)]
#[test]
fn graph_fifo_paths_do_not_block_resolution() {
    const WORKER: &str = "LANG_TEST_PACKAGE_GRAPH_FIFO_MANIFEST";
    if let Some(path) = std::env::var_os(WORKER) {
        assert_eq!(
            Graph::resolve(Path::new(&path), TARGET, Limits::default())
                .unwrap_err()
                .code,
            "PKG0005"
        );
        return;
    }
    fn refuses(path: &Path) {
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "graph_fifo_paths_do_not_block_resolution"])
            .env(WORKER, path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!("graph resolution blocked on FIFO path {path:?}: {output:?}");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{output:?}");
    }
    fn fifo(path: &Path) {
        assert!(
            std::process::Command::new("mkfifo")
                .arg(path)
                .status()
                .unwrap()
                .success()
        );
    }
    let f = Files::new();
    let manifest_fifo = f.root().join("pipe.json");
    fifo(&manifest_fifo);
    refuses(&manifest_fifo);
    let directory_fifo = f.root().join("pipe");
    fifo(&directory_fifo);
    refuses(&directory_fifo.join("Lang.json"));
    f.package("", "app", json!({"helper":dep("pipe")}));
    refuses(&f.manifest());
    f.package("", "app", json!({}));
    let entry_fifo = f.root().join("src/main.lang");
    fs::remove_file(&entry_fifo).unwrap();
    fifo(&entry_fifo);
    refuses(&f.manifest());
}

#[test]
fn sorted_diamond_edges_share_one_identity_and_keep_original_snapshots() {
    let f = Files::new();
    f.package("deps/shared", "shared", json!({}));
    f.package("deps/a", "a", json!({"shared":dep("../shared")}));
    f.package("deps/b", "b", json!({"shared":dep("../shared")}));
    // Deliberately reverse the source object order, not just a sorted JSON map.
    let root = br#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"b":{"path":"deps/b","version":"0.1.0"},"a":{"path":"deps/a","version":"0.1.0"}}}"#;
    fs::write(f.manifest(), root).unwrap();
    let g = f.resolve().unwrap();
    let names: Vec<_> = g
        .packages()
        .iter()
        .map(|p| p.manifest().package.as_ref().unwrap().name.as_str())
        .collect();
    assert_eq!(names, ["app", "a", "shared", "b"]);
    assert_eq!(g.target(), TARGET);
    let main = g.package(g.root()).unwrap();
    assert_eq!(main.root(), Path::new("."));
    assert_eq!(main.manifest_path(), Path::new("Lang.json"));
    assert_eq!(main.entry_path(), Path::new("src/main.lang"));
    let a = g.package(main.dependencies()["a"]).unwrap();
    let b = g.package(main.dependencies()["b"]).unwrap();
    assert_eq!(a.dependencies()["shared"], b.dependencies()["shared"]);
    assert_eq!(g.packages().len(), 4);
    fs::write(f.manifest(), b"changed after snapshot").unwrap();
    fs::write(f.root().join("src/main.lang"), b"changed source").unwrap();
    assert_eq!(main.manifest_bytes(), root);
    assert_eq!(main.entry_bytes(), SOURCE);
}

#[test]
fn exact_alias_and_version_requirements_are_checked_before_source_reads() {
    let f = Files::new();
    f.package("deps/helper", "helper", json!({}));
    f.package("", "app", json!({"wrong":dep("deps/helper")}));
    let error = f.resolve().unwrap_err();
    assert_eq!(error.code, "PKG0006");
    assert!(error.message.contains("alias wrong"));
    f.package(
        "",
        "app",
        json!({"helper":{"path":"deps/helper","version":"0.2.0"}}),
    );
    fs::remove_file(f.root().join("deps/helper/src/main.lang")).unwrap();
    let error = f.resolve().unwrap_err();
    assert_eq!(error.code, "PKG0006");
    assert!(error.message.contains("0.2.0, found 0.1.0"));
}

#[test]
fn cycles_and_cached_diamond_version_conflicts_are_refused() {
    let f = Files::new();
    f.package("", "app", json!({"helper":dep("deps/helper")}));
    f.package("deps/helper", "helper", json!({"app":dep("../..")}));
    let error = f.resolve().unwrap_err();
    assert_eq!(error.code, "PKG0006");
    assert!(error.message.contains("cycle"));

    f.package("deps/shared", "shared", json!({}));
    f.package("deps/a", "a", json!({"shared":dep("../shared")}));
    f.package(
        "deps/b",
        "b",
        json!({"shared":{"path":"../shared","version":"0.2.0"}}),
    );
    f.package("", "app", json!({"a":dep("deps/a"),"b":dep("deps/b")}));
    assert_eq!(f.resolve().unwrap_err().code, "PKG0006");
}

#[test]
fn inactive_edges_do_not_read_missing_packages_but_all_declarations_are_validated() {
    let f = Files::new();
    f.package(
        "",
        "app",
        json!({"helper":{"path":"missing","version":"0.1.0","targets":["x86_64-pc-windows-msvc"]}}),
    );
    let g = f.resolve().unwrap();
    assert_eq!(g.packages().len(), 1);
    assert!(g.package(g.root()).unwrap().dependencies().is_empty());
    assert_eq!(
        Graph::resolve(&f.manifest(), "x86_64-pc-windows-msvc", Limits::default())
            .unwrap_err()
            .code,
        "PKG0005"
    );
    f.package(
        "",
        "app",
        json!({"helper":{"path":"missing","version":"0.1.0","targets":["unknown"]}}),
    );
    assert_eq!(f.resolve().unwrap_err().code, "PKG0003");
    assert_eq!(
        Graph::resolve(
            &PathBuf::from("does-not-exist/Lang.json"),
            "unknown",
            Limits::default()
        )
        .unwrap_err()
        .code,
        "PKG0003"
    );
}

#[test]
fn package_edge_and_source_budgets_admit_exact_boundaries() {
    let f = Files::new();
    f.package("", "app", json!({"helper":dep("deps/helper")}));
    f.package("deps/helper", "helper", json!({}));
    let exact = Limits {
        packages: 2,
        edges: 1,
        source_bytes: SOURCE.len() * 2,
        ..Limits::default()
    };
    assert_eq!(
        Graph::resolve(&f.manifest(), TARGET, exact)
            .unwrap()
            .packages()
            .len(),
        2
    );
    for limits in [
        Limits {
            packages: 1,
            ..exact
        },
        Limits { edges: 0, ..exact },
        Limits {
            source_bytes: exact.source_bytes - 1,
            ..exact
        },
        Limits {
            packages: 0,
            ..exact
        },
    ] {
        assert_eq!(
            Graph::resolve(&f.manifest(), TARGET, limits)
                .unwrap_err()
                .code,
            "PKG0004"
        );
    }
}

#[test]
fn default_package_boundary_handles_a_deep_chain_without_unbounded_recursion() {
    let f = Files::new();
    f.package("", "app", json!({"p0":dep("nodes/p0")}));
    for i in 0..255 {
        let mut dependencies = serde_json::Map::new();
        if i < 254 {
            dependencies.insert(format!("p{}", i + 1), dep(&format!("../p{}", i + 1)));
        }
        f.package(
            &format!("nodes/p{i}"),
            &format!("p{i}"),
            Value::Object(dependencies),
        );
    }
    assert_eq!(f.resolve().unwrap().packages().len(), 256);
    f.package("nodes/p254", "p254", json!({"p255":dep("../p255")}));
    f.package("nodes/p255", "p255", json!({}));
    assert_eq!(f.resolve().unwrap_err().code, "PKG0004");
}

#[test]
fn parent_escapes_missing_entries_and_workspace_only_roots_are_refused() {
    let f = Files::new();
    f.package("../outside", "helper", json!({}));
    f.package("", "app", json!({"helper":dep("../outside")}));
    assert_eq!(f.resolve().unwrap_err().code, "PKG0005");
    f.package("", "app", json!({}));
    fs::remove_file(f.root().join("src/main.lang")).unwrap();
    assert_eq!(f.resolve().unwrap_err().code, "PKG0005");
    fs::write(
        f.manifest(),
        br#"{"schema":1,"workspace":{"members":["app"]}}"#,
    )
    .unwrap();
    assert_eq!(f.resolve().unwrap_err().code, "PKG0006");
}

#[cfg(unix)]
#[test]
fn symlink_escapes_are_refused_and_internal_aliases_keep_canonical_identity() {
    use std::os::unix::fs::symlink;
    let f = Files::new();
    f.package("../outside", "helper", json!({}));
    symlink("../outside", f.root().join("escape")).unwrap();
    f.package("", "app", json!({"helper":dep("escape")}));
    assert_eq!(f.resolve().unwrap_err().code, "PKG0005");
    f.package("deps/helper", "helper", json!({}));
    symlink("deps/helper", f.root().join("linked")).unwrap();
    f.package("", "app", json!({"helper":dep("linked")}));
    let g = f.resolve().unwrap();
    assert_eq!(g.packages()[1].root(), Path::new("deps/helper"));
    f.package("", "app", json!({}));
    fs::remove_file(f.root().join("src/main.lang")).unwrap();
    symlink(
        f.0.join("outside/src/main.lang"),
        f.root().join("src/main.lang"),
    )
    .unwrap();
    assert_eq!(f.resolve().unwrap_err().code, "PKG0005");
}
