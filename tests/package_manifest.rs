use lang::package::manifest::{MAX_BYTES, MAX_ENTRIES, Manifest};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lang package manifest {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn package() -> Value {
    json!({"schema":1,"package":{"name":"hello","version":"0.1.0","edition":"2026","entry":"src/main.lang"}})
}
fn parse(value: &Value) -> Result<Manifest, lang::package::Diagnostic> {
    Manifest::parse(&serde_json::to_vec(value).unwrap())
}

#[test]
fn package_and_workspace_contracts_are_admitted_without_dependency_io() {
    let mut value = package();
    value["dependencies"] =
        json!({"helper":{"path":"../helper","version":"1.2.3","targets":["aarch64-apple-darwin"]}});
    value["tests"] = json!(["tests/smoke.lang"]);
    value["workspace"] = json!({"members":["app","helper"]});
    let manifest = parse(&value).unwrap();
    assert_eq!(manifest.dependencies["helper"].path, "../helper");
    assert_eq!(manifest.tests, ["tests/smoke.lang"]);
    let root = parse(&json!({"schema":1,"workspace":{"members":["app"]}})).unwrap();
    assert!(root.package.is_none());
}

#[test]
fn versions_names_editions_and_required_fields_fail_closed() {
    for (field, values, code) in [
        (
            "name",
            vec!["", "Hello", "a-b", "../a", "あ", "0a"],
            "PKG0003",
        ),
        (
            "version",
            vec![
                "1",
                "1.2",
                "1.2.3.4",
                "01.2.3",
                "+1.2.3",
                "1.2.3-beta",
                "4294967296.0.0",
            ],
            "PKG0003",
        ),
        ("edition", vec!["2024", "", "2027"], "PKG0002"),
    ] {
        for invalid in values {
            let mut value = package();
            value["package"][field] = json!(invalid);
            assert_eq!(parse(&value).unwrap_err().code, code, "{value}");
        }
    }
    for field in ["name", "version", "edition", "entry"] {
        let mut value = package();
        value["package"].as_object_mut().unwrap().remove(field);
        assert_eq!(parse(&value).unwrap_err().code, "PKG0001");
    }
    assert_eq!(parse(&json!({"schema":2})).unwrap_err().code, "PKG0002");
    assert_eq!(parse(&json!({"schema":1})).unwrap_err().code, "PKG0003");
}

#[test]
fn portable_paths_and_all_target_conditions_are_validated() {
    for path in [
        "",
        "/abs.lang",
        "../x.lang",
        "a/../x.lang",
        "./x.lang",
        "a//x.lang",
        "C:/x.lang",
        "a\\x.lang",
        "a\0.lang",
        "x.txt",
    ] {
        let mut value = package();
        value["package"]["entry"] = json!(path);
        assert_eq!(parse(&value).unwrap_err().code, "PKG0003", "{path:?}");
    }
    for targets in [
        json!([]),
        json!(["unknown"]),
        json!(["x86_64-pc-windows-msvc", "x86_64-pc-windows-msvc"]),
    ] {
        let mut value = package();
        value["dependencies"] =
            json!({"helper":{"path":"deps/helper","version":"0.1.0","targets":targets}});
        assert_eq!(parse(&value).unwrap_err().code, "PKG0003");
    }
    for field in ["tests", "members"] {
        let mut value = package();
        if field == "members" {
            value["workspace"] = json!({"members":["../escape"]});
        } else {
            value[field] = json!(["tests/a.lang", "tests/a.lang"]);
        }
        assert_eq!(parse(&value).unwrap_err().code, "PKG0003");
    }
}

#[test]
fn duplicate_unknown_malformed_and_trailing_json_are_refused() {
    for input in [
        r#"{"schema":1,"schema":1}"#,
        r#"{"schema":1,"package":null,"package":null}"#,
        r#"{"schema":1,"dependencies":{"x":{"path":"x","version":"0.1.0"},"x":{"path":"y","version":"0.1.0"}}}"#,
        r#"{"schema":1,"package":{"name":"x","name":"y"}}"#,
        r#"{"schema":1,"workspace":{"members":["a"],"members":["b"]}}"#,
        r#"{"schema":1,"dependencies":{"x":{"path":"x","path":"y","version":"0.1.0"}}}"#,
        r#"{"schema":1,"unknown":true}"#,
        r#"{"schema":1} {}"#,
        "[]",
        "{",
        "null",
    ] {
        assert_eq!(
            Manifest::parse(input.as_bytes()).unwrap_err().code,
            "PKG0001",
            "{input}"
        );
    }
    assert_eq!(Manifest::parse(&[255]).unwrap_err().code, "PKG0001");
    let mut value = package();
    value["package"]["extra"] = json!(true);
    assert_eq!(parse(&value).unwrap_err().code, "PKG0001");
}

#[test]
fn byte_nesting_and_list_budgets_are_bounded() {
    assert_eq!(
        Manifest::parse(&vec![b' '; MAX_BYTES + 1])
            .unwrap_err()
            .code,
        "PKG0004"
    );
    let nested = format!("{}0{}", "[".repeat(256), "]".repeat(256));
    assert_eq!(
        Manifest::parse(nested.as_bytes()).unwrap_err().code,
        "PKG0001"
    );
    let mut value = package();
    value["tests"] = json!(
        (0..=MAX_ENTRIES)
            .map(|i| format!("tests/t{i}.lang"))
            .collect::<Vec<_>>()
    );
    assert_eq!(parse(&value).unwrap_err().code, "PKG0004");
    value["tests"].as_array_mut().unwrap().pop();
    assert_eq!(parse(&value).unwrap().tests.len(), MAX_ENTRIES);
    value["dependencies"] = Value::Object(
        (0..=MAX_ENTRIES)
            .map(|i| (format!("d{i}"), json!({"path":"dep","version":"0.1.0"})))
            .collect(),
    );
    assert_eq!(parse(&value).unwrap_err().code, "PKG0004");
    value["dependencies"]
        .as_object_mut()
        .unwrap()
        .remove("d128");
    assert_eq!(parse(&value).unwrap().dependencies.len(), MAX_ENTRIES);
    let mut exact = serde_json::to_vec(&package()).unwrap();
    exact.resize(MAX_BYTES, b' ');
    Manifest::parse(&exact).unwrap();
}

#[test]
fn real_cli_emits_single_json_diagnostic_and_preserves_exit_codes() {
    let files = Files::new();
    let path = files.0.join("Lang.json");
    std::fs::write(&path, serde_json::to_vec(&package()).unwrap()).unwrap();
    let invoke = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lang"))
            .args(["package", "check-manifest", "--message-format", "json"])
            .arg(&path)
            .args(extra)
            .output()
            .unwrap()
    };
    let success = invoke(&[]);
    assert!(success.status.success());
    assert!(success.stdout.is_empty() && success.stderr.is_empty());
    std::fs::write(&path, b"{\n invalid}").unwrap();
    let failure = invoke(&[]);
    assert_eq!(failure.status.code(), Some(1));
    assert!(failure.stdout.is_empty());
    let text = String::from_utf8(failure.stderr).unwrap();
    assert_eq!(text.lines().count(), 1);
    let diagnostic: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(diagnostic["schema"], 1);
    assert_eq!(diagnostic["kind"], "diagnostic");
    assert_eq!(diagnostic["code"], "PKG0001");
    assert_eq!(diagnostic["path"], path.to_string_lossy().as_ref());
    assert!(diagnostic["span"].is_null());
    let usage = invoke(&["--bad"]);
    assert_eq!(usage.status.code(), Some(2));
    assert_eq!(
        serde_json::from_slice::<Value>(&usage.stderr).unwrap()["code"],
        "CLI0001"
    );
    std::fs::remove_file(&path).unwrap();
    let missing = invoke(&[]);
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&missing.stderr).unwrap()["code"],
        "PKG0005"
    );
}
