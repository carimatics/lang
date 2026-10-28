use cap_std::fs::Dir;
use lang::{
    frontend::{self, CompileLimits},
    package::{
        graph::{Graph, Limits},
        lock::{Lock, MAX_BYTES, NAME},
        source,
    },
};
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit::{self as vm, Control, HostBindings};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

const TARGET: &str = "aarch64-apple-darwin";
const MANIFEST: &str = "{\"schema\":1,\"package\":{\"name\":\"app\",\"version\":\"0.1.0\",\"edition\":\"2026\",\"entry\":\"src/main.lang\"}}\n";
const MAIN: &str = "fn main()->i64{return 42;}\n";
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "lang lock {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let f = Self(root);
        f.write("Lang.json", MANIFEST);
        f.write("src/main.lang", MAIN);
        f
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn graph(&self) -> Graph {
        self.with_limits(Limits::default()).unwrap()
    }
    fn with_limits(&self, limits: Limits) -> Result<Graph, lang::package::Diagnostic> {
        Graph::resolve(&self.0.join("Lang.json"), TARGET, limits)
    }
    fn directory(&self) -> Dir {
        Dir::open_ambient_dir(&self.0, cap_std::ambient_authority()).unwrap()
    }
    fn package(&self, entry: &str, tests: &[&str]) {
        self.write("Lang.json", &json!({"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":entry},"tests":tests}).to_string());
    }
    fn assert_no_stage(&self) {
        assert!(!fs::read_dir(&self.0).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".Lang.lock.stage-")
        }));
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn value(lock: &Lock) -> Value {
    serde_json::from_slice(lock.as_bytes()).unwrap()
}
fn run(graph: &Graph) -> i64 {
    let a = source::analyze(graph, CompileLimits::default()).unwrap();
    assert!(a.result.is_ok(), "{:?}", a.diagnostics);
    let lang_semantics::analysis::Program::Copy(p) = a.result.unwrap() else {
        panic!("copy fixture")
    };
    let p = frontend::compile_script(&p, &a.sources, CompileLimits::default()).unwrap();
    let bytes = lang_bytecode::unit::encode(&p, Default::default()).unwrap();
    let p = lang_bytecode::unit::decode(&bytes, Default::default()).unwrap();
    vm::execute_entry(
        &p,
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
fn canonical_schema_and_independent_sha256_fixture_replay_offline() {
    let f = Files::new();
    let graph = f.graph();
    let lock = Lock::capture(&graph).unwrap();
    let expected = include_str!("fixtures/package-lock/schema1.json").replace(
        "<compiler_build_id>",
        &lang_codegen_clif::sdk::build_id(&TARGET.parse().unwrap()),
    );
    assert_eq!(lock.as_bytes(), expected.as_bytes());
    assert_eq!(lock.check(&f.directory()).unwrap_err().code, "PKG0007");
    assert!(!f.0.join(NAME).exists());
    lock.publish(&f.directory()).unwrap();
    lock.check(&f.directory()).unwrap();
    assert_eq!(run(&graph), 42);
    let metadata = fs::metadata(f.0.join(NAME)).unwrap();
    lock.check(&f.directory()).unwrap();
    assert_eq!(
        fs::metadata(f.0.join(NAME)).unwrap().modified().unwrap(),
        metadata.modified().unwrap()
    );
    assert_eq!(fs::read(f.0.join(NAME)).unwrap(), expected.as_bytes());
    f.assert_no_stage();
}

#[test]
fn dependency_lock_relocates_and_replays_from_frozen_inputs() {
    let mut f = Files::new();
    f.write("Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"deps/helper","version":"0.1.0"}}}"#);
    f.write(
        "src/main.lang",
        "mod helper; fn main()->i64{return helper::answer();}",
    );
    f.write("deps/helper/Lang.json", r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/lib.lang"}}"#);
    f.write(
        "deps/helper/src/lib.lang",
        "pub fn answer()->i64{return 42;}",
    );
    let graph = f.graph();
    let lock = Lock::capture(&graph).unwrap();
    lock.publish(&f.directory()).unwrap();
    lock.check(&f.directory()).unwrap();
    assert_eq!(run(&graph), 42);
    let records = value(&lock);
    assert_eq!(
        records["packages"][0]["dependencies"],
        json!([{"alias":"helper","root":"deps/helper"}])
    );
    let relocated = f.0.with_extension("relocated");
    fs::rename(&f.0, &relocated).unwrap();
    f.0 = relocated;
    let moved = f.graph();
    let replay = Lock::capture(&moved).unwrap();
    assert_eq!(lock.as_bytes(), replay.as_bytes());
    replay.check(&f.directory()).unwrap();
    assert_eq!(run(&moved), 42);
    fs::remove_dir_all(f.0.join("src")).unwrap();
    fs::remove_dir_all(f.0.join("deps")).unwrap();
    assert_eq!(Lock::capture(&graph).unwrap().as_bytes(), lock.as_bytes());
    assert_eq!(run(&graph), 42);
}

#[test]
fn changed_unused_source_manifest_and_dependency_refuse_without_rewriting_outputs() {
    for changed in [
        "src/main.lang",
        "src/unused.lang",
        "Lang.json",
        "deps/helper/src/lib.lang",
        "deps/helper/Lang.json",
    ] {
        let f = Files::new();
        f.write("Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"deps/helper","version":"0.1.0"}}}"#);
        f.write(
            "src/main.lang",
            "mod helper; fn main()->i64{return helper::answer();}",
        );
        f.write("src/unused.lang", "unused bytes");
        f.write("deps/helper/Lang.json", r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/lib.lang"}}"#);
        f.write(
            "deps/helper/src/lib.lang",
            "pub fn answer()->i64{return 42;}",
        );
        f.write("target/prior.bin", "prior binary");
        let graph = f.graph();
        let old = Lock::capture(&graph).unwrap();
        old.publish(&f.directory()).unwrap();
        let mut bytes = fs::read(f.0.join(changed)).unwrap();
        bytes.push(b'\n');
        fs::write(f.0.join(changed), bytes).unwrap();
        let changed = Lock::capture(&f.graph()).unwrap();
        assert_eq!(changed.check(&f.directory()).unwrap_err().code, "PKG0007");
        assert_eq!(fs::read(f.0.join(NAME)).unwrap(), old.as_bytes());
        assert_eq!(
            fs::read(f.0.join("target/prior.bin")).unwrap(),
            b"prior binary"
        );
        old.check(&f.directory()).unwrap();
        assert_eq!(run(&graph), 42);
        f.assert_no_stage();
    }
}

#[test]
fn malformed_duplicate_unknown_noncanonical_and_incompatible_locks_are_refused() {
    let f = Files::new();
    let lock = Lock::capture(&f.graph()).unwrap();
    let text = String::from_utf8(lock.as_bytes().to_vec()).unwrap();
    let cases = [
        "{}".into(),
        "null".into(),
        format!("{text}{{}}"),
        text.replace("\"schema\": 1", "\"schema\": 1, \"schema\": 1"),
        text.replace("\"name\": \"app\"", "\"name\": \"app\", \"name\": \"app\""),
        text.replace(
            "\"path\": \"src/main.lang\"",
            "\"path\": \"src/main.lang\", \"path\": \"src/main.lang\"",
        ),
        text.replace(
            "\"kind\": \"entry\"",
            "\"kind\": \"entry\", \"kind\": \"entry\"",
        ),
        text.replace("\"schema\": 1", "\"unknown\": 0, \"schema\": 1"),
        text.replace(
            "\"edition\": \"2026\"",
            "\"unknown\": 0, \"edition\": \"2026\"",
        ),
        text.replace("\"sha256\":", "\"unknown\": 0, \"sha256\":"),
        text.replace("\"kind\":", "\"unknown\": 0, \"kind\":"),
        text.trim_end().into(),
        text.replace("  ", " "),
        text.replace(TARGET, "x86_64-pc-windows-msvc"),
        text.replace(
            "\"compiler_version\": \"0.1.0\"",
            "\"compiler_version\": \"9.0.0\"",
        ),
        text.replace(
            "\"compiler_build_id\": \"",
            "\"compiler_build_id\": \"changed",
        ),
        format!("{}0{}", "[".repeat(150), "]".repeat(150)),
    ];
    for bytes in cases {
        assert_eq!(
            lock.verify(bytes.as_bytes()).unwrap_err().code,
            "PKG0007",
            "{bytes}"
        );
    }
    assert_eq!(
        lock.verify(text.replace("\"schema\": 1", "\"schema\": 2").as_bytes())
            .unwrap_err()
            .code,
        "PKG0002"
    );
    let other = Graph::resolve(
        &f.0.join("Lang.json"),
        "x86_64-pc-windows-msvc",
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        Lock::capture(&other)
            .unwrap()
            .verify(lock.as_bytes())
            .unwrap_err()
            .code,
        "PKG0007"
    );
    for field in ["checksum", "manifest_sha256"] {
        let mut modified = value(&lock);
        modified["packages"][0][field] = json!("0".repeat(64));
        assert_eq!(
            lock.verify(&serde_json::to_vec_pretty(&modified).unwrap())
                .unwrap_err()
                .code,
            "PKG0007"
        );
    }
    assert_eq!(
        lock.verify(
            text.replace("\"kind\": \"entry\"", "\"kind\": \"unknown\"")
                .as_bytes()
        )
        .unwrap_err()
        .code,
        "PKG0007"
    );
    f.write("Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"helper","version":"0.1.0"}}}"#);
    f.write("helper/Lang.json", r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
    f.write("helper/src/main.lang", MAIN);
    let edges = Lock::capture(&f.graph()).unwrap();
    let text = std::str::from_utf8(edges.as_bytes()).unwrap();
    for replacement in [
        "\"alias\": \"helper\", \"alias\": \"helper\"",
        "\"unknown\": 0, \"alias\": \"helper\"",
    ] {
        assert_eq!(
            edges
                .verify(
                    text.replace("\"alias\": \"helper\"", replacement)
                        .as_bytes()
                )
                .unwrap_err()
                .code,
            "PKG0007"
        );
    }
}

#[test]
fn unicode_paths_use_utf8_lexical_checksum_order_on_every_host() {
    let f = Files::new();
    // UTF-16 orders these two names in the opposite order to UTF-8.
    f.write("src/\u{10000}.lang", "unused");
    f.write("src/\u{e000}.lang", "unused");
    let lock = Lock::capture(&f.graph()).unwrap();
    let record = value(&lock);
    let package = &record["packages"][0];
    let paths: Vec<_> = package["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        paths,
        ["src/main.lang", "src/\u{e000}.lang", "src/\u{10000}.lang"]
    );
    assert_eq!(
        package["checksum"],
        "112121a8cb55424dbfd5d1f993dba728722326eb6242f01395b033a3eae1f7e2"
    );
}

#[test]
fn ordinary_publication_replaces_a_prior_regular_lock_after_validation() {
    let f = Files::new();
    let old = Lock::capture(&f.graph()).unwrap();
    old.publish(&f.directory()).unwrap();
    f.write("target/prior.bin", "previous binary");
    f.write("src/main.lang", "fn main()->i64{return 99;}");
    let graph = f.graph();
    let new = Lock::capture(&graph).unwrap();
    assert_eq!(new.check(&f.directory()).unwrap_err().code, "PKG0007");
    assert_eq!(run(&graph), 99);
    new.publish(&f.directory()).unwrap();
    new.check(&f.directory()).unwrap();
    assert_eq!(fs::read(f.0.join(NAME)).unwrap(), new.as_bytes());
    assert_eq!(
        fs::read(f.0.join("target/prior.bin")).unwrap(),
        b"previous binary"
    );
    f.assert_no_stage();
}

#[test]
fn lock_and_source_resource_boundaries_and_nonregular_inputs_refuse_safely() {
    let f = Files::new();
    let lock = Lock::capture(&f.graph()).unwrap();
    assert_eq!(
        lock.verify(&vec![b' '; MAX_BYTES]).unwrap_err().code,
        "PKG0007"
    );
    assert_eq!(
        lock.verify(&vec![b' '; MAX_BYTES + 1]).unwrap_err().code,
        "PKG0004"
    );
    fs::write(f.0.join(NAME), vec![b' '; MAX_BYTES + 1]).unwrap();
    assert_eq!(lock.check(&f.directory()).unwrap_err().code, "PKG0004");
    fs::remove_file(f.0.join(NAME)).unwrap();
    fs::create_dir(f.0.join(NAME)).unwrap();
    assert_eq!(lock.check(&f.directory()).unwrap_err().code, "PKG0007");
    for limits in [
        Limits {
            source_files: 0,
            ..Limits::default()
        },
        Limits {
            directory_entries: 0,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            Lock::capture(&f.with_limits(limits).unwrap())
                .unwrap_err()
                .code,
            "PKG0004"
        );
    }
    let exact = Limits {
        source_bytes: MAIN.len(),
        source_files: 1,
        directory_entries: 1,
        ..Limits::default()
    };
    assert!(Lock::capture(&f.with_limits(exact).unwrap()).is_ok());
    assert_eq!(
        f.with_limits(Limits {
            source_bytes: MAIN.len() - 1,
            ..exact
        })
        .unwrap_err()
        .code,
        "PKG0004"
    );
    f.assert_no_stage();
}

#[test]
fn duplicate_physical_unused_inputs_are_refused_before_lock_publication() {
    let f = Files::new();
    let old = Lock::capture(&f.graph()).unwrap();
    old.publish(&f.directory()).unwrap();
    fs::hard_link(f.0.join("src/main.lang"), f.0.join("src/unused.lang")).unwrap();
    let error = Lock::capture(&f.graph()).unwrap_err();
    assert_eq!(error.code, "PKG0007");
    assert!(error.message.contains("duplicate physical source"));
    assert_eq!(fs::read(f.0.join(NAME)).unwrap(), old.as_bytes());
    f.assert_no_stage();
}

#[test]
fn declared_test_bindings_survive_overlapping_scopes_and_capture_deletion() {
    let f = Files::new();
    f.package(
        "src/main.lang",
        &["src/smoke.lang", "src/sub/two.lang", "tests/one.lang"],
    );
    f.write("src/smoke.lang", "fn main()->i64{return 0;}");
    f.write("src/sub/two.lang", "fn main()->i64{return 0;}");
    f.write("tests/one.lang", "fn main()->i64{return 0;}");
    let graph = f.graph();
    let lock = Lock::capture(&graph).unwrap();
    let package = &value(&lock)["packages"][0];
    assert_eq!(package["sources"].as_array().unwrap().len(), 4);
    assert_eq!(
        package["bindings"],
        json!([
            {"kind":"entry","scope":"src","path":"src/main.lang","target":"src/main.lang"},
            {"kind":"test","scope":"src","path":"src/smoke.lang","target":"src/smoke.lang"},
            {"kind":"test","scope":"src/sub","path":"src/sub/two.lang","target":"src/sub/two.lang"},
            {"kind":"test","scope":"tests","path":"tests/one.lang","target":"tests/one.lang"}
        ])
    );
    fs::remove_dir_all(f.0.join("src")).unwrap();
    fs::remove_dir_all(f.0.join("tests")).unwrap();
    assert_eq!(Lock::capture(&graph).unwrap().as_bytes(), lock.as_bytes());
    assert_eq!(run(&graph), 42);
}

#[test]
fn failed_publication_preserves_destination_and_cleans_exclusive_stage() {
    let f = Files::new();
    let lock = Lock::capture(&f.graph()).unwrap();
    f.write("Lang.lock/prior", "previous destination");
    f.write("target/prior.bin", "previous binary");
    assert_eq!(lock.publish(&f.directory()).unwrap_err().code, "PKG0005");
    assert_eq!(
        fs::read(f.0.join("Lang.lock/prior")).unwrap(),
        b"previous destination"
    );
    assert_eq!(
        fs::read(f.0.join("target/prior.bin")).unwrap(),
        b"previous binary"
    );
    f.assert_no_stage();
}

#[cfg(windows)]
#[test]
fn windows_busy_lock_retains_old_bytes_and_closes_failed_stage() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Files::new();
    let old = Lock::capture(&f.graph()).unwrap();
    old.publish(&f.directory()).unwrap();
    f.write("src/main.lang", "fn main()->i64{return 99;}");
    let new = Lock::capture(&f.graph()).unwrap();
    // FILE_SHARE_READ deliberately denies deletion/replacement of this handle.
    let busy = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(f.0.join(NAME))
        .unwrap();
    assert!(new.publish(&f.directory()).is_err());
    assert_eq!(fs::read(f.0.join(NAME)).unwrap(), old.as_bytes());
    f.assert_no_stage();
    drop(busy);
    new.publish(&f.directory()).unwrap();
    new.check(&f.directory()).unwrap();
}

#[cfg(unix)]
#[test]
fn alias_retarget_with_identical_source_checksums_is_stale_and_preserves_output() {
    use std::os::unix::fs::symlink;
    let f = Files::new();
    f.write(
        "src/main.lang",
        "mod route; fn main()->i64{return route::leaf::value();}",
    );
    f.write("src/route.lang", "pub mod leaf;");
    f.write("src/a/leaf.lang", "pub fn value()->i64{return 42;}");
    f.write("src/b/leaf.lang", "pub fn value()->i64{return 99;}");
    f.write("target/prior.bin", "previous binary");
    symlink("a", f.0.join("src/route")).unwrap();
    let graph = f.graph();
    let old = Lock::capture(&graph).unwrap();
    old.publish(&f.directory()).unwrap();
    assert_eq!(run(&graph), 42);
    fs::remove_file(f.0.join("src/route")).unwrap();
    symlink("b", f.0.join("src/route")).unwrap();
    let changed = f.graph();
    let new = Lock::capture(&changed).unwrap();
    let (a, b) = (value(&old), value(&new));
    assert_eq!(a["packages"][0]["sources"], b["packages"][0]["sources"]);
    assert_eq!(
        a["packages"][0]["manifest_sha256"],
        b["packages"][0]["manifest_sha256"]
    );
    assert_ne!(a["packages"][0]["bindings"], b["packages"][0]["bindings"]);
    assert_ne!(a["packages"][0]["checksum"], b["packages"][0]["checksum"]);
    assert_eq!(new.check(&f.directory()).unwrap_err().code, "PKG0007");
    assert_eq!(fs::read(f.0.join(NAME)).unwrap(), old.as_bytes());
    assert_eq!(
        fs::read(f.0.join("target/prior.bin")).unwrap(),
        b"previous binary"
    );
    assert_eq!(run(&graph), 42);
    assert_eq!(run(&changed), 99);
}

#[cfg(unix)]
#[test]
fn every_declared_entry_and_test_binding_is_pinned_when_inventories_overlap() {
    use std::os::unix::fs::symlink;
    let f = Files::new();
    fs::remove_dir_all(f.0.join("src")).unwrap();
    f.package(
        "selected/main.lang",
        &["a/one.lang", "b/one.lang", "selected/two.lang"],
    );
    for (dir, result) in [("a", 42), ("b", 99)] {
        f.write(
            &format!("{dir}/main.lang"),
            &format!("fn main()->i64{{return {result};}}"),
        );
        f.write(&format!("{dir}/one.lang"), "fn main()->i64{return 0;}");
        f.write(&format!("{dir}/two.lang"), "fn main()->i64{return 0;}");
    }
    symlink("a", f.0.join("selected")).unwrap();
    let old = Lock::capture(&f.graph()).unwrap();
    fs::remove_file(f.0.join("selected")).unwrap();
    symlink("b", f.0.join("selected")).unwrap();
    let new = Lock::capture(&f.graph()).unwrap();
    let (a, b) = (value(&old), value(&new));
    assert_eq!(a["packages"][0]["sources"], b["packages"][0]["sources"]);
    for (package, dir) in [(&a, "a"), (&b, "b")] {
        let bindings = package["packages"][0]["bindings"].as_array().unwrap();
        assert_eq!(bindings.len(), 4);
        for (kind, name) in [("entry", "main"), ("test", "two")] {
            let binding = bindings
                .iter()
                .find(|v| v["path"] == format!("selected/{name}.lang"))
                .unwrap();
            assert_eq!(binding["kind"], kind);
            assert_eq!(binding["target"], format!("{dir}/{name}.lang"));
        }
    }
    assert_eq!(new.verify(old.as_bytes()).unwrap_err().code, "PKG0007");
}
