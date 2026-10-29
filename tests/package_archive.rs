use lang::package::{
    archive::{Archive, Kind},
    graph::Graph,
};
use serde_json::Value;
use std::{
    fs,
    io::{Cursor, Read},
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
const TARGET: &str = "aarch64-apple-darwin";
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang source archive {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let files = Self(path);
        files.write(
            "Lang.json",
            r#"{"schema":1,"workspace":{"members":["app","helper"]}}"#,
        );
        files.write("app/Lang.json", r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"../helper","version":"0.1.0"}},"tests":["tests/smoke.lang"]}"#);
        files.write("helper/Lang.json", r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
        files.write("app/src/main.lang", "mod helper; pub fn answer()->i64{return helper::answer();} fn main()->i64{return answer();}\n");
        files.write("helper/src/main.lang", "pub fn answer()->i64{return 42;}\n");
        files.write(
            "app/tests/smoke.lang",
            "mod _package; fn main()->i64{return _package::answer() + -42;}\n",
        );
        files
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn graph(&self) -> Graph {
        Graph::resolve_selected(
            &self.0.join("Lang.json"),
            TARGET,
            Default::default(),
            Some("app"),
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
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn frozen_tar_has_deterministic_headers_bytes_and_real_extracted_sibling_execution() {
    let mut files = Files::new();
    let graph = files.graph();
    let archive = Archive::capture(&graph).unwrap();
    let mut paths = Vec::new();
    let mut reader = tar::Archive::new(Cursor::new(archive.as_bytes()));
    for entry in reader.entries().unwrap() {
        let mut entry = entry.unwrap();
        assert!(entry.header().entry_type().is_file());
        assert_eq!(entry.header().mtime().unwrap(), 0);
        assert_eq!(entry.header().uid().unwrap(), 0);
        assert_eq!(entry.header().gid().unwrap(), 0);
        assert_eq!(entry.header().mode().unwrap(), 0o644);
        assert!(entry.header().username().unwrap().is_none_or(str::is_empty));
        assert!(
            entry
                .header()
                .groupname()
                .unwrap()
                .is_none_or(str::is_empty)
        );
        let path = entry.path().unwrap().to_str().unwrap().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        let description = archive.entries().iter().find(|e| e.path == path).unwrap();
        assert_eq!(lang_codegen_clif::sdk::digest(&bytes), description.sha256);
        paths.push(path);
    }
    assert_eq!(
        paths,
        archive
            .entries()
            .iter()
            .map(|e| e.path.clone())
            .collect::<Vec<_>>()
    );
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted);
    assert!(archive.entries().iter().any(|e| e.kind == Kind::Descriptor));
    let extracted = files.0.join("extracted");
    fs::create_dir(&extracted).unwrap();
    tar::Archive::new(Cursor::new(archive.as_bytes()))
        .unpack(&extracted)
        .unwrap();
    for action in ["run", "test"] {
        let output = success(
            Command::new(env!("CARGO_BIN_EXE_lang"))
                .current_dir(&extracted)
                .args([
                    action,
                    "--package",
                    "app",
                    "--target",
                    TARGET,
                    "--locked",
                    "--offline",
                ])
                .output()
                .unwrap(),
        );
        assert_eq!(
            output.stdout,
            if action == "run" {
                b"42\n".as_slice()
            } else {
                b""
            }
        );
    }
    files.write("helper/src/main.lang", "changed");
    fs::remove_file(files.0.join("app/Lang.json")).unwrap();
    let renamed = files.0.with_extension("relocated");
    fs::rename(&files.0, &renamed).unwrap();
    files.0 = renamed;
    assert_eq!(
        Archive::capture(&graph).unwrap().as_bytes(),
        archive.as_bytes()
    );
    archive.verify(archive.as_bytes()).unwrap();
    let mut changed = archive.as_bytes().to_vec();
    changed[512] ^= 1;
    assert_eq!(archive.verify(&changed).unwrap_err().code, "PKG0007");
}

#[test]
fn package_cli_lists_without_writes_then_publishes_and_replays_exact_archive() {
    let files = Files::new();
    let listing = success(files.run(&[
        "package",
        "--package",
        "app",
        "--target",
        TARGET,
        "--list",
        "--message-format",
        "json",
    ]));
    let value: Value = serde_json::from_slice(&listing.stdout).unwrap();
    assert_eq!(value["kind"], "package-files");
    assert!(!files.0.join("Lang.lock").exists());
    assert!(!files.0.join("target").exists());
    let output = success(files.run(&["package", "--package", "app", "--target", TARGET]));
    let path = String::from_utf8(output.stdout).unwrap();
    let path = files.0.join(path.trim());
    let bytes = fs::read(&path).unwrap();
    let lock = fs::read(files.0.join("Lang.lock")).unwrap();
    success(files.run(&[
        "package",
        "--package",
        "app",
        "--target",
        TARGET,
        "--locked",
        "--offline",
    ]));
    assert_eq!(fs::read(&path).unwrap(), bytes);
    files.write("helper/src/main.lang", "pub fn answer()->i64{return 43;}\n");
    let output = files.run(&[
        "package",
        "--package",
        "app",
        "--target",
        TARGET,
        "--locked",
        "--offline",
        "--message-format",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let diagnostic: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(diagnostic["code"], "PKG0007");
    assert_eq!(fs::read(path).unwrap(), bytes);
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), lock);
}

#[test]
fn utf8_order_long_names_and_unused_original_bytes_are_preserved() {
    let files = Files::new();
    let long = format!("app/src/{}/unused.lang", "長".repeat(80));
    for path in [&long, "app/src/\u{e000}.lang", "app/src/\u{10000}.lang"] {
        files.write(path, "// unchanged unused bytes\r\n");
    }
    let archive = Archive::capture(&files.graph()).unwrap();
    let paths: Vec<_> = archive.entries().iter().map(|e| e.path.as_str()).collect();
    assert!(
        paths
            .iter()
            .position(|p| *p == "app/src/\u{e000}.lang")
            .unwrap()
            < paths
                .iter()
                .position(|p| *p == "app/src/\u{10000}.lang")
                .unwrap()
    );
    let mut found = false;
    for entry in tar::Archive::new(Cursor::new(archive.as_bytes()))
        .entries()
        .unwrap()
    {
        let mut entry = entry.unwrap();
        if entry.path().unwrap().to_str().unwrap() == long {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            assert_eq!(bytes, b"// unchanged unused bytes\r\n");
            found = true;
        }
    }
    assert!(found);
    assert_eq!(
        Archive::capture(&files.graph()).unwrap().as_bytes(),
        archive.as_bytes()
    );
}

#[test]
fn reserved_metadata_resource_and_cache_tampering_are_refused() {
    let files = Files::new();
    let archive = Archive::capture(&files.graph()).unwrap();
    assert_eq!(
        archive
            .verify(&vec![0; 64 * 1024 * 1024 + 1])
            .unwrap_err()
            .code,
        "PKG0004"
    );
    let output = success(files.run(&[
        "package",
        "--package",
        "app",
        "--target",
        TARGET,
        "--message-format",
        "json",
    ]));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let path = files.0.join(report["path"].as_str().unwrap());
    let mut altered = fs::read(&path).unwrap();
    altered[512] ^= 1;
    fs::write(&path, &altered).unwrap();
    let old_lock = fs::read(files.0.join("Lang.lock")).unwrap();
    let output = files.run(&[
        "package",
        "--package",
        "app",
        "--target",
        TARGET,
        "--locked",
        "--offline",
        "--message-format",
        "json",
    ]);
    let diagnostic: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(diagnostic["code"], "PKG0007");
    assert_eq!(fs::read(&path).unwrap(), altered);
    assert_eq!(fs::read(files.0.join("Lang.lock")).unwrap(), old_lock);
    for name in [".lang-snapshot.json", ".LANG-SNAPSHOT.JSON"] {
        fs::copy(files.0.join("Lang.json"), files.0.join(name)).unwrap();
        let graph =
            Graph::resolve_selected(&files.0.join(name), TARGET, Default::default(), Some("app"))
                .unwrap();
        assert_eq!(Archive::capture(&graph).unwrap_err().code, "PKG0003");
    }
}

#[cfg(unix)]
#[test]
fn directory_alias_profiles_and_actual_symlink_escapes_are_refused() {
    use std::os::unix::fs::symlink;
    let files = Files::new();
    symlink("src", files.0.join("app/alias")).unwrap();
    let original = fs::read_to_string(files.0.join("app/Lang.json")).unwrap();
    files.write(
        "app/Lang.json",
        &original.replace("src/main.lang", "alias/main.lang"),
    );
    assert_eq!(
        Archive::capture(&files.graph()).unwrap_err().code,
        "PKG0003"
    );
    files.write("app/Lang.json", &original);
    fs::remove_file(files.0.join("app/alias")).unwrap();
    symlink("helper", files.0.join("chosen")).unwrap();
    files.write(
        "Lang.json",
        r#"{"schema":1,"workspace":{"members":["app","chosen"]}}"#,
    );
    assert_eq!(
        Archive::capture(&files.graph()).unwrap_err().code,
        "PKG0003"
    );
    files.write(
        "Lang.json",
        r#"{"schema":1,"workspace":{"members":["app","helper"]}}"#,
    );
    files.write("app/Lang.json", &original.replace("../helper", "../chosen"));
    assert_eq!(
        Archive::capture(&files.graph()).unwrap_err().code,
        "PKG0003"
    );
    files.write("app/Lang.json", &original);
    symlink(std::env::temp_dir(), files.0.join("app/src/outside")).unwrap();
    assert_eq!(
        Archive::capture(&files.graph()).unwrap_err().code,
        "PKG0005"
    );
    assert!(!files.0.join("Lang.lock").exists());
    assert!(!files.0.join("target").exists());
}
