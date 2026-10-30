use lang::profile::rebuild::{Category, Report, Snapshot};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lang"))
}
struct Temp(PathBuf);
impl Temp {
    fn fixture() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "lang rebuild {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let original =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/packages/local-dependency");
        for file in [
            "Lang.json",
            "src/main.lang",
            "deps/helper/Lang.json",
            "deps/helper/src/lib.lang",
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::copy(original.join(file), path).unwrap();
        }
        Self(root)
    }
    fn command(&self, action: &[&str]) -> Command {
        let mut command = bin();
        command
            .args(action)
            .arg("--manifest-path")
            .arg(self.0.join("Lang.json"));
        command
    }
    fn profile(&self, extra: &[&str]) -> Report {
        let output = self
            .command(&["profile", "rebuild"])
            .args(extra)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        Report::decode(&output.stdout).unwrap()
    }
    fn build(&self) {
        assert!(self.command(&["build"]).status().unwrap().success());
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn rebuild_cli_observes_actual_source_dependency_and_target_inputs_without_publication() {
    let temp = Temp::fixture();
    let first = temp.profile(&[]);
    assert!(first.before.is_none() && first.inputs_changed);
    assert!(!temp.0.join("Lang.lock").exists());
    assert!(!temp.0.join("target").exists());
    temp.build();
    let lock = std::fs::read(temp.0.join("Lang.lock")).unwrap();
    let equal = temp.profile(&[]);
    assert!(!equal.inputs_changed);
    assert!(equal.changes.is_empty());
    assert_eq!(equal.before.as_ref().unwrap(), &equal.current);
    std::fs::write(
        temp.0.join("src/main.lang"),
        "mod helper; fn main()->i64 { return helper::answer() + 1; }\n",
    )
    .unwrap();
    let changed = temp.profile(&[]);
    assert!(
        changed
            .changes
            .iter()
            .any(|c| c.category == Category::Source && c.key[0] == ".")
    );
    assert!(
        !changed
            .changes
            .iter()
            .any(|c| c.category == Category::Source && c.key[0] == "deps/helper")
    );
    std::fs::write(
        temp.0.join("deps/helper/src/lib.lang"),
        "pub fn answer()->i64{return 43;}\nfn main()->i64{return -1;}\n",
    )
    .unwrap();
    let changed = temp.profile(&[]);
    assert!(
        changed
            .changes
            .iter()
            .any(|c| c.category == Category::Source && c.key[0] == "deps/helper")
    );
    let target = if cfg!(windows) {
        "aarch64-apple-darwin"
    } else {
        "x86_64-pc-windows-msvc"
    };
    let changed = temp.profile(&["--target", target]);
    assert!(
        changed
            .changes
            .iter()
            .any(|c| c.category == Category::Target)
    );
    assert!(
        changed
            .changes
            .iter()
            .any(|c| c.category == Category::Compiler && c.key == ["build_id"])
    );
    assert_eq!(Report::decode(&changed.encode().unwrap()).unwrap(), changed);
    assert_eq!(std::fs::read(temp.0.join("Lang.lock")).unwrap(), lock);
    assert_eq!(
        std::fs::read_dir(temp.0.join("target/lang"))
            .unwrap()
            .count(),
        1
    );
    let explicit = temp.0.join("baseline.lock");
    std::fs::write(&explicit, lock).unwrap();
    let output = temp
        .command(&["profile", "rebuild"])
        .arg("--previous-lock")
        .arg(explicit)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(Report::decode(&output.stdout).unwrap().inputs_changed);
}

#[test]
fn actual_historical_compiler_input_is_reported_without_inventing_source_changes() {
    // Captured by the real 546ae69 CLI on 2026-10-30 JST from the unchanged
    // public local-dependency example, targeting aarch64-apple-darwin.
    let historical = include_bytes!("fixtures/package-lock/rebuild-546ae69.json");
    let temp = Temp::fixture();
    let current = temp.profile(&["--target", "aarch64-apple-darwin"]).current;
    let report =
        Report::compare(Some(Snapshot::capture(historical, None).unwrap()), current).unwrap();
    assert!(report.inputs_changed);
    assert!(
        report
            .changes
            .iter()
            .any(|c| c.category == Category::Compiler && c.key == ["build_id"])
    );
    assert!(!report.changes.iter().any(|c| matches!(
        c.category,
        Category::Source | Category::Manifest | Category::Dependency | Category::Target
    )));

    // Checkout must preserve the golden capture's LF bytes. An actual CRLF
    // source edit is a real input change and must still be reported.
    let path = temp.0.join("src/main.lang");
    let source = std::fs::read_to_string(&path).unwrap();
    assert!(!source.contains('\r'));
    std::fs::write(&path, source.replace('\n', "\r\n")).unwrap();
    let current = temp.profile(&["--target", "aarch64-apple-darwin"]).current;
    let changed =
        Report::compare(Some(Snapshot::capture(historical, None).unwrap()), current).unwrap();
    assert!(
        changed
            .changes
            .iter()
            .any(|c| c.category == Category::Source && c.key == [".", "src/main.lang"])
    );
}

#[test]
fn rebuild_reader_requires_exact_snapshot_digests_and_complete_ordered_changes() {
    let temp = Temp::fixture();
    temp.build();
    let before = temp.profile(&[]).current;
    std::fs::write(temp.0.join("src/main.lang"), "fn main()->i64{return 17;}\n").unwrap();
    let current = temp.profile(&[]).current;
    let report = Report::compare(Some(before), current).unwrap();
    assert!(report.changes.len() >= 2);
    let mut bad = report.clone();
    bad.changes.pop();
    assert!(bad.validate().is_err());
    let mut bad = report.clone();
    bad.changes.reverse();
    assert!(bad.validate().is_err());
    let mut bad = report.clone();
    bad.changes[0].after = Some("invented".into());
    assert!(bad.validate().is_err());
    let mut bad = report.clone();
    bad.inputs_changed = false;
    assert!(bad.validate().is_err());
    let mut bad = report.clone();
    bad.current.lock_sha256 = "0".repeat(64);
    assert!(bad.validate().is_err());
    let duplicate = report.current.lock.replacen("{", "{\"schema\":1,", 1);
    assert!(Snapshot::capture(duplicate.as_bytes(), None).is_err());
    let traversal = report
        .current
        .lock
        .replacen("\"root\": \".\"", "\"root\": \"..\"", 1);
    assert!(Snapshot::capture(traversal.as_bytes(), None).is_err());
    let mut forged = serde_json::to_value(&report).unwrap();
    forged["extra"] = serde_json::json!(true);
    assert!(Report::decode(&serde_json::to_vec(&forged).unwrap()).is_err());
    let duplicate = String::from_utf8(report.encode().unwrap())
        .unwrap()
        .replacen("{", "{\"schema\":1,", 1);
    assert!(Report::decode(duplicate.as_bytes()).is_err());
    assert!(Report::decode(&vec![b' '; lang::profile::MAX_REPORT_BYTES + 1]).is_err());
}

#[test]
fn rebuild_cli_rejects_invalid_explicit_inputs_and_options_without_changing_the_lock() {
    let temp = Temp::fixture();
    temp.build();
    let lock = std::fs::read(temp.0.join("Lang.lock")).unwrap();
    for args in [
        vec!["--native-sdk", "missing"],
        vec!["--previous-native", "missing"],
        vec!["--fuel", "1"],
        vec!["--package", "a", "--package", "a"],
    ] {
        let output = temp
            .command(&["profile", "rebuild"])
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
    let missing = temp.0.join("missing.lock");
    let output = temp
        .command(&["profile", "rebuild"])
        .arg("--previous-lock")
        .arg(&missing)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    std::fs::write(&missing, "{}").unwrap();
    let output = temp
        .command(&["profile", "rebuild"])
        .arg("--previous-lock")
        .arg(&missing)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(temp.0.join("Lang.lock")).unwrap(), lock);
}
