use cap_std::{ambient_authority, fs::Dir};
use lang::package::migration::{Limits, Plan};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const OLD: &str = "fn main()->i64{let mut shared:i64=40;let mut weak:i64=2;return shared+weak;}";
const OPTIONS: &[&str] = &[
    "--from-profile",
    "rfc0006-scalar",
    "--to-edition",
    "2026",
    "--source-root",
    ".",
];
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang-migration-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("old.lang"), OLD).unwrap();
        Self(path)
    }
    fn authority(&self) -> Dir {
        Dir::open_ambient_dir(&self.0, ambient_authority()).unwrap()
    }
    fn plan(&self) -> Plan {
        Plan::capture(&self.authority(), Path::new("old.lang"), Limits::default()).unwrap()
    }
    fn bytes(&self) -> Vec<u8> {
        fs::read(self.0.join("old.lang")).unwrap()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cli(files: &Files, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lang"))
        .current_dir(&files.0)
        .args(["migrate", "--message-format", "json"])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn migration_cli_previews_applies_and_reports_a_repeat_noop() {
    let files = Files::new();
    let mut args = OPTIONS.to_vec();
    args.push("old.lang");
    let preview = cli(&files, &args);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(report["applied"], 0);
    assert_eq!(report["roots"][0], "old.lang");
    assert_eq!(report["edits"].as_array().unwrap().len(), 4);
    assert_eq!(files.bytes(), OLD.as_bytes());
    let human = Command::new(env!("CARGO_BIN_EXE_lang"))
        .current_dir(&files.0)
        .arg("migrate")
        .args(&args)
        .output()
        .unwrap();
    assert!(human.status.success());
    assert!(
        String::from_utf8(human.stdout)
            .unwrap()
            .contains("preview, checked")
    );
    args.push("--apply");
    let applied = cli(&files, &args);
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&applied.stdout).unwrap();
    assert_eq!(report["applied"], 1);
    assert_eq!(report["outcome"], "applied");
    assert_ne!(files.bytes(), OLD.as_bytes());
    let run = Command::new(env!("CARGO_BIN_EXE_lang"))
        .current_dir(&files.0)
        .args(["run", "--source-root", ".", "old.lang"])
        .output()
        .unwrap();
    assert!(run.status.success());
    assert_eq!(run.stdout, b"42\n");
    let noop = cli(&files, &args);
    assert!(noop.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&noop.stdout).unwrap()["changed"],
        0
    );
}

#[test]
fn cli_file_is_cwd_relative_and_absolute_inside_root_paths_work() {
    let files = Files::new();
    let other = Files::new();
    let run = |file: &Path| {
        Command::new(env!("CARGO_BIN_EXE_lang"))
            .current_dir(&other.0)
            .args([
                "migrate",
                "--from-profile",
                "rfc0006-scalar",
                "--to-edition",
                "2026",
                "--source-root",
            ])
            .arg(&files.0)
            .args(["--message-format", "json"])
            .arg(file)
            .output()
            .unwrap()
    };
    let outside = run(Path::new("old.lang"));
    assert_eq!(outside.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&outside.stderr).unwrap()["code"],
        "MIG0001"
    );
    let inside = run(&files.0.join("old.lang"));
    assert!(
        inside.status.success(),
        "{}",
        String::from_utf8_lossy(&inside.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&inside.stdout).unwrap()["files"][0],
        "old.lang"
    );
    assert_eq!(files.bytes(), OLD.as_bytes());
    assert_eq!(other.bytes(), OLD.as_bytes());
}

#[test]
fn cli_requires_explicit_supported_options_and_preserves_bytes_on_refusal() {
    let files = Files::new();
    let mut cases = vec![vec![], vec!["old.lang"]];
    for suffix in [
        vec!["--apply", "--apply", "old.lang"],
        vec!["--unknown", "old.lang"],
        vec!["old.lang", "other.lang"],
        vec!["--source-root"],
        vec!["--manifest-path", "Lang.json", "old.lang"],
    ] {
        let mut args = OPTIONS.to_vec();
        args.extend(suffix);
        cases.push(args);
    }
    for (index, value) in [(1, "unknown"), (3, "2027")] {
        let mut args = OPTIONS.to_vec();
        args[index] = value;
        args.push("old.lang");
        cases.push(args);
    }
    for args in cases {
        let out = cli(&files, &args);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&out.stderr).unwrap()["code"],
            "CLI0001"
        );
        assert!(out.stdout.is_empty());
        assert_eq!(files.bytes(), OLD.as_bytes());
    }
    let mut outside = OPTIONS.to_vec();
    outside.extend(["--apply", "../outside.lang"]);
    let out = cli(&files, &outside);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&out.stderr).unwrap()["code"],
        "MIG0001"
    );
    let invalid = "pub fn main()->i64{return 42;}";
    fs::write(files.0.join("old.lang"), invalid).unwrap();
    let mut args = OPTIONS.to_vec();
    args.extend(["--apply", "old.lang"]);
    let out = cli(&files, &args);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&out.stderr).unwrap()["code"],
        "MIG0001"
    );
    assert_eq!(files.bytes(), invalid.as_bytes());
}

#[test]
fn preview_and_apply_bind_one_source_without_a_manifest_and_execute_the_result() {
    let files = Files::new();
    let authority = files.authority();
    let plan = files.plan();
    assert_eq!(files.bytes(), OLD.as_bytes());
    assert_eq!(serde_json::to_value(plan.preview()).unwrap()["applied"], 0);
    let expected = plan.preview().replacement().to_string();
    // Ambient package data is neither consulted nor rewritten.
    fs::write(files.0.join("Lang.json"), "not a package").unwrap();
    fs::write(files.0.join("Lang.lock"), "leave unchanged").unwrap();
    let report = plan.apply(&authority).unwrap();
    assert_eq!(serde_json::to_value(&report).unwrap()["applied"], 1);
    assert_eq!(serde_json::to_value(&report).unwrap()["outcome"], "applied");
    assert_eq!(files.bytes(), expected.as_bytes());
    assert_eq!(
        fs::read_to_string(files.0.join("Lang.lock")).unwrap(),
        "leave unchanged"
    );
    assert_eq!(
        fs::read_to_string(files.0.join("Lang.json")).unwrap(),
        "not a package"
    );
    let run = Command::new(env!("CARGO_BIN_EXE_lang"))
        .args(["run", "--source-root"])
        .arg(&files.0)
        .arg(files.0.join("old.lang"))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(run.stdout, b"42\n");
    let again = files.plan().apply(&authority).unwrap();
    assert_eq!(serde_json::to_value(again).unwrap()["applied"], 0);
    assert_eq!(files.bytes(), expected.as_bytes());
}

#[test]
fn stale_bytes_identity_and_foreign_authority_refuse_before_publication() {
    let files = Files::new();
    let other = Files::new();
    assert_eq!(
        files.plan().apply(&other.authority()).unwrap_err().code,
        "PKG0007"
    );
    assert_eq!(other.bytes(), OLD.as_bytes());
    let plan = files.plan();
    let newer = format!("{OLD}\n// new edit\n");
    fs::write(files.0.join("old.lang"), &newer).unwrap();
    assert_eq!(plan.apply(&files.authority()).unwrap_err().code, "PKG0007");
    assert_eq!(files.bytes(), newer.as_bytes());
    let plan = files.plan();
    fs::write(files.0.join("replacement.lang"), &newer).unwrap();
    fs::rename(files.0.join("replacement.lang"), files.0.join("old.lang")).unwrap();
    assert_eq!(plan.apply(&files.authority()).unwrap_err().code, "PKG0007");
    assert_eq!(files.bytes(), newer.as_bytes());
}

#[test]
fn readonly_alias_paths_and_limits_refuse_without_changing_source() {
    let files = Files::new();
    let source = files.0.join("old.lang");
    let permissions = fs::metadata(&source).unwrap().permissions();
    let mut readonly = permissions.clone();
    readonly.set_readonly(true);
    fs::set_permissions(&source, readonly).unwrap();
    let result = files.plan().apply(&files.authority());
    fs::set_permissions(&source, permissions).unwrap();
    assert_eq!(result.unwrap_err().code, "PKG0005");
    assert_eq!(files.bytes(), OLD.as_bytes());
    for path in [Path::new(""), Path::new("../old.lang"), &source] {
        assert_eq!(
            Plan::capture(&files.authority(), path, Limits::default())
                .unwrap_err()
                .code,
            "MIG0001"
        );
    }
    for limits in [
        Limits {
            source_bytes: OLD.len() - 1,
            ..Limits::default()
        },
        Limits {
            report_bytes: 0,
            ..Limits::default()
        },
    ] {
        assert_eq!(
            Plan::capture(&files.authority(), Path::new("old.lang"), limits)
                .unwrap_err()
                .code,
            "MIG0002"
        );
    }
    fs::hard_link(&source, files.0.join("alias.lang")).unwrap();
    assert_eq!(
        Plan::capture(&files.authority(), Path::new("old.lang"), Limits::default())
            .unwrap_err()
            .code,
        "PKG0003"
    );
    assert_eq!(files.bytes(), OLD.as_bytes());
    assert!(!fs::read_dir(&files.0).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".lang-format-")
    }));
}

#[cfg(unix)]
#[test]
fn symlink_file_and_directory_aliases_are_not_hidden_by_canonicalization() {
    use std::os::unix::fs::symlink;
    let files = Files::new();
    symlink("old.lang", files.0.join("link.lang")).unwrap();
    assert_eq!(
        Plan::capture(
            &files.authority(),
            Path::new("link.lang"),
            Limits::default()
        )
        .unwrap_err()
        .code,
        "PKG0003"
    );
    fs::create_dir(files.0.join("real")).unwrap();
    fs::write(files.0.join("real/source.lang"), OLD).unwrap();
    symlink("real", files.0.join("alias")).unwrap();
    assert_eq!(
        Plan::capture(
            &files.authority(),
            Path::new("alias/source.lang"),
            Limits::default()
        )
        .unwrap_err()
        .code,
        "PKG0007"
    );
    assert_eq!(
        fs::read_to_string(files.0.join("real/source.lang")).unwrap(),
        OLD
    );
}
