use lang::{
    frontend::{self, CompileLimits},
    package::{
        graph::{Graph, Limits},
        source,
    },
};
use lang_bytecode::unit::{self as wire, Program};
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit::{self as vm, Control, HostBindings};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const TARGET: &str = "aarch64-apple-darwin";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lang package source {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn package(&self, path: &str, name: &str, dependencies: Value, text: &str) {
        let root = self.0.join(path);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("Lang.json"), serde_json::to_vec(&json!({"schema":1,"package":{"name":name,"version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":dependencies})).unwrap()).unwrap();
        fs::write(root.join("src/main.lang"), text).unwrap();
    }
    fn graph(&self) -> Graph {
        Graph::resolve(&self.0.join("Lang.json"), TARGET, Limits::default()).unwrap()
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
fn compile(graph: &Graph) -> Program {
    let a = source::analyze(graph, CompileLimits::default()).unwrap();
    assert!(a.result.is_ok(), "{:?}: {:?}", a.result, a.diagnostics);
    let checked = a.result.unwrap();
    let lang_semantics::analysis::Program::Copy(typed) = checked else {
        panic!("Copy fixture");
    };
    let p = frontend::compile_script(&typed, &a.sources, CompileLimits::default()).unwrap();
    let bytes = wire::encode(&p, wire::CodecLimits::default()).unwrap();
    wire::decode(&bytes, wire::CodecLimits::default()).unwrap()
}
fn run(program: &Program) -> i64 {
    vm::execute_entry(
        program,
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
fn real_dependency_fixture_preserves_package_root_entry_and_native_equivalence() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/packages/local-dependency/Lang.json");
    let g = Graph::resolve(&path, TARGET, Limits::default()).unwrap();
    let p = compile(&g);
    assert_eq!(run(&p), 42);
    let unit = p.verified();
    let entry = unit.unit().entry.unwrap();
    let jit = lang_codegen_clif::program::CompiledProgram::compile(
        unit,
        &[],
        &HostFunctionRegistry::new(),
        lang_mir::unit::Limits::default(),
    )
    .unwrap();
    assert_eq!(
        jit.invoke(entry, &[]).unwrap(),
        lang_codegen_clif::program::Value {
            value_type: lang_mir::unit::Type::I64,
            cells: vec![42],
        }
    );
    assert!(
        unit.unit()
            .functions
            .iter()
            .any(|f| f.name == "root::package1::main")
    );
    assert!(
        unit.unit()
            .sources
            .iter()
            .any(|s| s.text.contains("root::hidden()"))
    );
}

#[test]
fn entry_snapshots_survive_disk_changes_and_each_package_loads_its_own_file_modules() {
    let mut f = Files::new();
    f.package(
        "",
        "app",
        json!({"helper":dep("deps/helper")}),
        "mod helper; mod math; fn main()->i64{return helper::answer()+math::value();}",
    );
    f.package(
        "deps/helper",
        "helper",
        json!({}),
        "mod math; pub fn answer()->i64{return root::math::value();}",
    );
    fs::write(f.0.join("src/math.lang"), "pub fn value()->i64{return 2;}").unwrap();
    fs::write(
        f.0.join("deps/helper/src/math.lang"),
        "pub fn value()->i64{return 40;}",
    )
    .unwrap();
    let g = f.graph();
    fs::write(f.0.join("src/main.lang"), "invalid later source").unwrap();
    fs::remove_file(f.0.join("deps/helper/src/main.lang")).unwrap();
    assert_eq!(run(&compile(&g)), 42);
    fs::remove_dir_all(f.0.join("src")).unwrap();
    fs::remove_dir_all(f.0.join("deps/helper/src")).unwrap();
    let mut limits = CompileLimits::default();
    limits.modules.modules = 4;
    assert!(source::analyze(&g, limits).unwrap().result.is_ok());
    limits.modules.modules = 3;
    assert!(source::analyze(&g, limits).unwrap().result.is_err());
    let moved = f.0.with_extension("moved");
    fs::rename(&f.0, &moved).unwrap();
    f.0 = moved;
    assert_eq!(run(&compile(&g)), 42);
}

#[test]
fn separate_transitive_aliases_and_shared_diamond_types_keep_distinct_identities() {
    let f = Files::new();
    f.package(
        "",
        "app",
        json!({"a":dep("deps/a"),"b":dep("deps/b")}),
        "mod a; mod b; fn main()->i64{return b::take(a::make());}",
    );
    f.package(
        "deps/common",
        "common",
        json!({}),
        "pub struct Shared {value:i64} pub fn make()->Shared{return Shared{value:42};}",
    );
    f.package(
        "deps/a",
        "a",
        json!({"common":dep("../common")}),
        "mod common; pub fn make()->common::Shared{return common::make();}",
    );
    f.package(
        "deps/b",
        "b",
        json!({"common":dep("../common")}),
        "mod common; pub fn take(x:common::Shared)->i64{return x.value;}",
    );
    let p = compile(&f.graph());
    assert_eq!(run(&p), 42);
    assert_eq!(p.verified().unit().definitions.len(), 1);

    f.package(
        "deps/a/deps/helper",
        "helper",
        json!({}),
        "pub fn value()->i64{return 40;}",
    );
    f.package(
        "deps/b/deps/helper",
        "helper",
        json!({}),
        "pub fn value()->i64{return 2;}",
    );
    f.package(
        "deps/a",
        "a",
        json!({"helper":dep("deps/helper")}),
        "mod helper; pub fn value()->i64{return root::helper::value();}",
    );
    f.package(
        "deps/b",
        "b",
        json!({"helper":dep("deps/helper")}),
        "mod helper; pub fn value()->i64{return root::helper::value();}",
    );
    f.package(
        "",
        "app",
        json!({"a":dep("deps/a"),"b":dep("deps/b")}),
        "mod a; mod b; fn main()->i64{return a::value()+b::value();}",
    );
    assert_eq!(run(&compile(&f.graph())), 42);
}

#[test]
fn private_undeclared_and_duplicate_names_are_refused_with_original_spans() {
    let f = Files::new();
    f.package(
        "deps/helper",
        "helper",
        json!({}),
        "// 日本語\nfn secret()->i64{return 42;}",
    );
    f.package(
        "",
        "app",
        json!({"helper":dep("deps/helper")}),
        "mod helper; fn main()->i64{return helper::secret();}",
    );
    let a = source::analyze(&f.graph(), CompileLimits::default()).unwrap();
    assert!(a.result.is_err());
    let d = a
        .diagnostics
        .iter()
        .find(|d| d.message.contains("private"))
        .unwrap();
    assert_eq!(a.sources.slice(d.primary.span).unwrap(), "secret");
    assert_eq!(a.sources.slice(d.secondary[0].span).unwrap(), "secret");
    f.package(
        "",
        "app",
        json!({}),
        "mod helper; fn main()->i64{return 0;}",
    );
    assert!(
        source::analyze(&f.graph(), CompileLimits::default())
            .unwrap()
            .result
            .is_err()
    );
    f.package(
        "",
        "app",
        json!({"helper":dep("deps/helper")}),
        "mod helper; fn helper()->i64{return 0;} fn main()->i64{return 0;}",
    );
    let a = source::analyze(&f.graph(), CompileLimits::default()).unwrap();
    assert!(
        a.diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate module name"))
    );
}

#[cfg(unix)]
#[test]
fn opened_file_identity_refuses_symlink_and_hardlink_module_duplicates_and_escapes() {
    use std::os::unix::fs::symlink;
    let f = Files::new();
    f.package(
        "",
        "app",
        json!({}),
        "mod first; mod second; fn main()->i64{return 0;}",
    );
    fs::write(
        f.0.join("src/first.lang"),
        "pub fn value()->i64{return 42;}",
    )
    .unwrap();
    symlink("first.lang", f.0.join("src/second.lang")).unwrap();
    let a = source::analyze(&f.graph(), CompileLimits::default()).unwrap();
    assert!(
        a.diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate canonical"))
    );
    fs::remove_file(f.0.join("src/second.lang")).unwrap();
    fs::hard_link(f.0.join("src/first.lang"), f.0.join("src/second.lang")).unwrap();
    let a = source::analyze(&f.graph(), CompileLimits::default()).unwrap();
    assert!(
        a.diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate canonical"))
    );
    fs::remove_file(f.0.join("src/second.lang")).unwrap();
    // The directory is authorized for metadata, but lies outside this source root.
    fs::create_dir(f.0.join("outside_source")).unwrap();
    fs::write(
        f.0.join("outside_source/second.lang"),
        "pub fn value()->i64{return 99;}",
    )
    .unwrap();
    symlink("../outside_source", f.0.join("src/alias")).unwrap();
    f.package("", "app", json!({}), "mod alias; fn main()->i64{return 0;}");
    fs::write(f.0.join("src/alias.lang"), "pub mod second;").unwrap();
    assert_eq!(
        source::analyze(&f.graph(), CompileLimits::default())
            .unwrap_err()
            .code,
        "PKG0005"
    );
    fs::remove_file(f.0.join("src/alias.lang")).unwrap();
    symlink("../outside_source/second.lang", f.0.join("src/alias.lang")).unwrap();
    assert_eq!(
        source::analyze(&f.graph(), CompileLimits::default())
            .unwrap_err()
            .code,
        "PKG0005"
    );
    fs::remove_file(f.0.join("src/main.lang")).unwrap();
    symlink("../outside_source/second.lang", f.0.join("src/main.lang")).unwrap();
    assert_eq!(
        Graph::resolve(&f.0.join("Lang.json"), TARGET, Limits::default())
            .unwrap_err()
            .code,
        "PKG0005"
    );
}

#[test]
fn hardlinked_source_modules_are_refused_on_both_hosts() {
    let f = Files::new();
    f.package(
        "",
        "app",
        json!({}),
        "mod first; mod second; fn main()->i64{return 0;}",
    );
    fs::write(
        f.0.join("src/first.lang"),
        "pub fn value()->i64{return 42;}",
    )
    .unwrap();
    fs::hard_link(f.0.join("src/first.lang"), f.0.join("src/second.lang")).unwrap();
    let a = source::analyze(&f.graph(), CompileLimits::default()).unwrap();
    assert!(a.result.is_err());
    assert!(
        a.diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate canonical"))
    );
}

#[cfg(unix)]
#[test]
fn reachable_fifo_module_is_refused_without_blocking() {
    const WORKER: &str = "LANG_TEST_PACKAGE_SOURCE_FIFO_MANIFEST";
    if let Some(path) = std::env::var_os(WORKER) {
        let g = Graph::resolve(Path::new(&path), TARGET, Limits::default()).unwrap();
        let error = source::analyze(&g, CompileLimits::default()).unwrap_err();
        assert_eq!(error.code, "PKG0005");
        assert!(error.message.contains("regular file"));
        return;
    }
    let f = Files::new();
    f.package("", "app", json!({}), "mod pipe; fn main()->i64{return 0;}");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(f.0.join("src/pipe.lang"))
            .status()
            .unwrap()
            .success()
    );
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "reachable_fifo_module_is_refused_without_blocking",
        ])
        .env(WORKER, f.0.join("Lang.json"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if std::time::Instant::now() >= end {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!("FIFO module blocked: {output:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn later_analyses_use_frozen_modules_and_new_graphs_observe_edits() {
    let f = Files::new();
    f.package(
        "",
        "app",
        json!({}),
        "mod math; fn main()->i64{return math::answer();}",
    );
    fs::write(
        f.0.join("src/math.lang"),
        "pub fn answer()->i64{return 42;}",
    )
    .unwrap();
    let graph = f.graph();
    assert_eq!(run(&compile(&graph)), 42);
    fs::write(
        f.0.join("src/math.lang"),
        "pub fn answer()->i64{return 99;}",
    )
    .unwrap();
    assert_eq!(run(&compile(&graph)), 42);
    assert_eq!(run(&compile(&f.graph())), 99);
}

#[test]
fn unused_inputs_count_toward_limits_and_failed_capture_releases_handles() {
    let mut f = Files::new();
    f.package("", "app", json!({}), "fn main()->i64{return 42;}");
    fs::write(
        f.0.join("src/unused.lang"),
        "pub fn unused()->i64{return 0;}",
    )
    .unwrap();
    let limits = Limits {
        source_files: 1,
        ..Limits::default()
    };
    let graph = Graph::resolve(&f.0.join("Lang.json"), TARGET, limits).unwrap();
    assert_eq!(
        source::analyze(&graph, CompileLimits::default())
            .unwrap_err()
            .code,
        "PKG0004"
    );
    fs::remove_file(f.0.join("src/unused.lang")).unwrap();
    assert_eq!(
        source::analyze(&graph, CompileLimits::default())
            .unwrap_err()
            .code,
        "PKG0004"
    );
    let moved = f.0.with_extension("moved");
    fs::rename(&f.0, &moved).unwrap();
    f.0 = moved;
    assert_eq!(run(&compile(&f.graph())), 42);
}

#[test]
fn inventory_file_byte_and_entry_budgets_admit_exact_boundaries() {
    let f = Files::new();
    let main = "fn main()->i64{return 42;}";
    let unused = "pub fn unused()->i64{return 0;}";
    f.package("", "app", json!({}), main);
    fs::write(f.0.join("src/unused.lang"), unused).unwrap();
    let exact = Limits {
        source_files: 2,
        directory_entries: 2,
        source_bytes: main.len() + unused.len(),
        ..Limits::default()
    };
    let graph = Graph::resolve(&f.0.join("Lang.json"), TARGET, exact).unwrap();
    assert_eq!(run(&compile(&graph)), 42);
    for limits in [
        Limits {
            source_files: 1,
            ..exact
        },
        Limits {
            directory_entries: 1,
            ..exact
        },
        Limits {
            source_bytes: exact.source_bytes - 1,
            ..exact
        },
    ] {
        let graph = Graph::resolve(&f.0.join("Lang.json"), TARGET, limits).unwrap();
        assert_eq!(
            source::analyze(&graph, CompileLimits::default())
                .unwrap_err()
                .code,
            "PKG0004"
        );
    }
}

#[test]
fn declared_test_directories_are_captured_and_released_with_package_inputs() {
    let f = Files::new();
    f.package("", "app", json!({}), "fn main()->i64{return 42;}");
    let path = f.0.join("Lang.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    manifest["tests"] = json!(["tests/smoke.lang"]);
    fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    fs::create_dir(f.0.join("tests")).unwrap();
    fs::write(f.0.join("tests/smoke.lang"), "fn main()->i64{return 0;}").unwrap();
    let graph = f.graph();
    assert_eq!(run(&compile(&graph)), 42);
    fs::remove_dir_all(f.0.join("tests")).unwrap();
    assert_eq!(run(&compile(&graph)), 42);
    assert_eq!(
        source::analyze(&f.graph(), CompileLimits::default())
            .unwrap_err()
            .code,
        "PKG0005"
    );
}

#[test]
fn nested_dependency_sources_are_not_local_module_authority() {
    let f = Files::new();
    f.package(
        "",
        "app",
        json!({"helper":dep("src/deps/helper")}),
        "mod helper; fn main()->i64{return helper::answer();}",
    );
    f.package(
        "src/deps/helper",
        "helper",
        json!({}),
        "pub fn answer()->i64{return 42;}",
    );
    let manifest_path = f.0.join("src/deps/helper/Lang.json");
    let mut manifest: Value = serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
    manifest["package"]["entry"] = json!("main.lang");
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    fs::write(
        f.0.join("src/deps/helper/main.lang"),
        "pub fn answer()->i64{return 42;}",
    )
    .unwrap();
    fs::write(
        f.0.join("src/deps/helper/src.lang"),
        "pub fn peek()->i64{return 999;}",
    )
    .unwrap();
    assert_eq!(run(&compile(&f.graph())), 42);
    fs::write(
        f.0.join("src/main.lang"),
        "mod deps; fn main()->i64{return deps::helper::src::peek();}",
    )
    .unwrap();
    fs::write(f.0.join("src/deps.lang"), "pub mod helper;").unwrap();
    fs::write(f.0.join("src/deps/helper.lang"), "pub mod src;").unwrap();
    let a = source::analyze(&f.graph(), CompileLimits::default()).unwrap();
    assert!(a.result.is_err());
    assert!(
        a.diagnostics
            .iter()
            .any(|d| d.message.contains("frozen package inventory"))
    );
}

#[cfg(unix)]
#[test]
fn captured_directory_aliases_survive_removal_and_cycles_are_refused() {
    use std::os::unix::fs::symlink;
    let f = Files::new();
    f.package(
        "",
        "app",
        json!({}),
        "mod alias; fn main()->i64{return alias::leaf::answer();}",
    );
    fs::write(f.0.join("src/alias.lang"), "pub mod leaf;").unwrap();
    fs::create_dir(f.0.join("src/real")).unwrap();
    fs::write(
        f.0.join("src/real/leaf.lang"),
        "pub fn answer()->i64{return 42;}",
    )
    .unwrap();
    symlink("real", f.0.join("src/alias")).unwrap();
    let graph = f.graph();
    assert_eq!(run(&compile(&graph)), 42);
    fs::remove_file(f.0.join("src/alias")).unwrap();
    assert_eq!(run(&compile(&graph)), 42);
    symlink("..", f.0.join("src/real/loop")).unwrap();
    let error = source::analyze(&f.graph(), CompileLimits::default()).unwrap_err();
    assert_eq!(error.code, "PKG0005");
    assert!(error.message.contains("cycle"));
}
