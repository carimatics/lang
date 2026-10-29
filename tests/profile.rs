use lang::{
    artifact, frontend,
    package::{graph::Graph, lock::Lock, source},
    profile::Report,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lang"))
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/packages/local-dependency/Lang.json")
}
fn compile(manifest: &Path) -> Report {
    let result = bin()
        .args(["profile", "compile", "--manifest-path"])
        .arg(manifest)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    Report::decode(&result.stdout).unwrap()
}

#[test]
fn real_package_profile_observes_the_ordinary_dependency_artifact_without_publication() {
    let manifest = fixture();
    let before = std::fs::read(&manifest).unwrap();
    let report = compile(&manifest);
    let graph = Graph::resolve(&manifest, &report.target, Default::default()).unwrap();
    let lock = Lock::capture(&graph).unwrap();
    assert_eq!(
        report.input_digest,
        lang_codegen_clif::sdk::digest(lock.as_bytes())
    );
    let limits = frontend::CompileLimits::default();
    let analysis = source::analyze(&graph, limits).unwrap();
    let expected_files = analysis.sources.iter().count();
    let expected_bytes: usize = analysis
        .sources
        .iter()
        .map(|(_, file)| file.text().len())
        .sum();
    let bytes = artifact::compile(
        analysis.result.unwrap(),
        &analysis.sources,
        limits,
        Default::default(),
    )
    .unwrap();
    assert_eq!(report.source_files as usize, expected_files);
    assert_eq!(report.source_bytes, expected_bytes as u64);
    assert_eq!(report.artifact_bytes, bytes.len() as u64);
    assert_eq!(
        report.elapsed_ns,
        report
            .phases
            .iter()
            .map(|phase| phase.elapsed_ns)
            .sum::<u64>()
    );
    assert_eq!(std::fs::read(manifest).unwrap(), before);
    let again = compile(&fixture());
    assert_eq!(again.input_digest, report.input_digest);
    assert_eq!(again.compiler_id, report.compiler_id);
    assert_eq!(Report::decode(&report.encode().unwrap()).unwrap(), report);
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang profile {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn profile_uses_explicit_workspace_selection_and_preserves_existing_build_state() {
    let temp = Temp::new();
    let app = temp.0.join("app");
    assert!(bin().arg("new").arg(&app).status().unwrap().success());
    assert!(
        bin()
            .arg("build")
            .arg("--manifest-path")
            .arg(app.join("Lang.json"))
            .status()
            .unwrap()
            .success()
    );
    let lock = std::fs::read(app.join("Lang.lock")).unwrap();
    std::fs::write(
        temp.0.join("Lang.json"),
        r#"{"schema":1,"workspace":{"members":["app"]}}"#,
    )
    .unwrap();
    let result = bin()
        .args(["profile", "compile", "--manifest-path"])
        .arg(temp.0.join("Lang.json"))
        .args(["--package", "app"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report = Report::decode(&result.stdout).unwrap();
    assert_eq!(report.source_files, 1);
    assert_eq!(std::fs::read(app.join("Lang.lock")).unwrap(), lock);
    assert!(!temp.0.join("Lang.lock").exists());
    std::fs::write(
        app.join("src/main.lang"),
        "fn main()->i64{return missing;}\n",
    )
    .unwrap();
    let failed = bin()
        .args(["profile", "compile", "--manifest-path"])
        .arg(app.join("Lang.json"))
        .args(["--message-format", "json"])
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stdout.is_empty());
    let lines = String::from_utf8(failed.stderr).unwrap();
    let diagnostics: Vec<serde_json::Value> = lines
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"].as_str().unwrap().starts_with("SRC") && d.get("span").is_some())
    );
    assert_eq!(std::fs::read(app.join("Lang.lock")).unwrap(), lock);
    assert!(app.join("target/lang").is_dir());
}

#[test]
fn compile_profile_reader_rejects_foreign_formats_overflow_and_incomplete_observations() {
    let report = compile(&fixture());
    let good = serde_json::to_value(&report).unwrap();
    let mut failures = Vec::new();
    for (key, value) in [
        ("schema", serde_json::json!(2)),
        ("kind", serde_json::json!("runtime")),
        ("clock", serde_json::json!("cpu")),
        ("source_files", serde_json::json!(4097)),
        ("source_bytes", serde_json::json!(33 * 1024 * 1024)),
        ("artifact_bytes", serde_json::json!(65 * 1024 * 1024)),
        ("input_digest", serde_json::json!("forged")),
        ("elapsed_ns", serde_json::json!(u64::MAX)),
        ("target", serde_json::json!("../../outside")),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        failures.push(bad);
    }
    let mut bad = good.clone();
    bad["phases"][0]["elapsed_ns"] = serde_json::json!(u64::MAX);
    bad["phases"][1]["elapsed_ns"] = serde_json::json!(1);
    failures.push(bad);
    let mut bad = good.clone();
    bad["phases"].as_array_mut().unwrap().pop();
    failures.push(bad);
    let mut bad = good.clone();
    bad["phases"][0]["name"] = serde_json::json!("frontend");
    failures.push(bad);
    for bad in failures {
        assert!(
            Report::decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{bad}"
        );
    }
    assert!(Report::decode(&vec![b' '; lang::profile::MAX_REPORT_BYTES + 1]).is_err());
    let duplicate = String::from_utf8(report.encode().unwrap())
        .unwrap()
        .replacen("{", "{\"schema\":1,", 1);
    assert!(Report::decode(duplicate.as_bytes()).is_err());
    for args in [
        vec!["profile", "compile", "--unknown"],
        vec!["profile", "compile", "--target", "invalid"],
        vec!["profile", "compile", "--message-format", "unsupported"],
    ] {
        let result = bin().args(args).output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
    }
}
