use lang::package::manifest::{DependencySource, MAX_BYTES, MAX_ENTRIES, Manifest};
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
    assert_eq!(
        manifest.dependencies["helper"].source,
        DependencySource::Local {
            path: "../helper".into()
        }
    );
    assert_eq!(manifest.tests, ["tests/smoke.lang"]);
    let root = parse(&json!({"schema":1,"workspace":{"members":["app"]}})).unwrap();
    assert!(root.package.is_none());
}

#[test]
fn schema_two_sources_are_explicit_and_do_not_grant_authority() {
    let mut value = package();
    value["schema"] = json!(2);
    value["dependencies"] = json!({
        "helper":{"source":"local","path":"../helper","version":"1.2.3"},
        "renamed":{"source":"registry","registry":"example","name":"math","version":"1.0.0","targets":["aarch64-apple-darwin"]}
    });
    let manifest = parse(&value).unwrap();
    assert_eq!(manifest.schema, 2);
    assert_eq!(
        manifest.dependencies["helper"].source,
        DependencySource::Local {
            path: "../helper".into()
        }
    );
    assert_eq!(
        manifest.dependencies["renamed"].source,
        DependencySource::Registry {
            registry: "example".into(),
            name: "math".into()
        }
    );
    assert_eq!(manifest.dependencies["renamed"].version, "1.0.0");
    assert_eq!(
        manifest.dependencies["renamed"].targets.as_deref(),
        Some(["aarch64-apple-darwin".to_owned()].as_slice())
    );
    value["schema"] = json!(1);
    assert_eq!(parse(&value).unwrap_err().code, "PKG0001");
    Manifest::parse(include_bytes!(
        "fixtures/package-lock/schema3-registry-inputs/app/Lang.json"
    ))
    .unwrap();
    parse(&json!({"schema":2,"workspace":{"members":["app"]}})).unwrap();
}

#[test]
fn schema_two_rejects_ambiguous_duplicate_and_authority_fields() {
    for declaration in [
        r#"{"path":"helper","version":"1.0.0"}"#,
        r#"{"source":"local","source":"registry","path":"helper","version":"1.0.0"}"#,
        r#"{"source":"local","path":"helper","path":"other","version":"1.0.0"}"#,
        r#"{"source":"local","path":"helper","version":"1.0.0","registry":"example"}"#,
        r#"{"source":"registry","registry":"example","name":"math","version":"1.0.0","path":"helper"}"#,
        r#"{"source":"registry","registry":"example","name":"math","name":"other","version":"1.0.0"}"#,
        r#"{"source":"registry","registry":"example","version":"1.0.0"}"#,
        r#"{"source":"Registry","registry":"example","name":"math","version":"1.0.0"}"#,
        r#"{"source":"local","path":"helper","version":"1.0.0","targets":null}"#,
        r#"{"source":"registry","registry":"example","name":"math","version":"1.0.0","targets":null}"#,
        r#"{"source":"registry","registry":"example","name":"math","version":"1.0.0","targets":[],"targets":[]}"#,
    ] {
        let input = format!(
            r#"{{"schema":2,"package":{},"dependencies":{{"helper":{declaration}}}}}"#,
            package()["package"]
        );
        assert_eq!(
            Manifest::parse(input.as_bytes()).unwrap_err().code,
            "PKG0001",
            "{declaration}"
        );
    }
    for field in ["url", "origin", "credentials", "root_sha256", "cache"] {
        let mut value = package();
        value["schema"] = json!(2);
        value["dependencies"] = json!({"helper":{"source":"registry","registry":"example","name":"math","version":"1.0.0"}});
        value["dependencies"]["helper"][field] = json!("untrusted");
        assert_eq!(parse(&value).unwrap_err().code, "PKG0001", "{field}");
    }
    let duplicate = br#"{"schema":2,"package":{"name":"app","version":"1.0.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"x":{"source":"local","path":"x","version":"1.0.0"},"x":{"source":"local","path":"y","version":"1.0.0"}}}"#;
    assert!(
        Manifest::parse(duplicate)
            .unwrap_err()
            .message
            .contains("duplicate dependency alias")
    );
    let tag_last = br#"{"schema":2,"package":{"name":"app","version":"1.0.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"x":{"registry":"example","name":"math","version":"1.0.0","source":"registry"}}}"#;
    Manifest::parse(tag_last).unwrap();
}

#[test]
fn schema_two_validates_inactive_registry_declarations_and_shared_limits() {
    let mut value = package();
    value["schema"] = json!(2);
    value["dependencies"] = json!({"helper":{"source":"registry","registry":"example","name":"math","version":"1.0.0","targets":["x86_64-pc-windows-msvc"]}});
    for (field, invalid) in [
        ("registry", json!("https://registry.example")),
        ("name", json!("../math")),
        ("version", json!("^1.0.0")),
        ("targets", json!([])),
        ("targets", json!(["unknown"])),
        (
            "targets",
            json!(["x86_64-pc-windows-msvc", "x86_64-pc-windows-msvc"]),
        ),
    ] {
        let mut invalid_value = value.clone();
        invalid_value["dependencies"]["helper"][field] = invalid;
        assert_eq!(
            parse(&invalid_value).unwrap_err().code,
            "PKG0003",
            "{field}"
        );
    }
    value["dependencies"] = Value::Object(
        (0..MAX_ENTRIES)
            .map(|i| (format!("d{i}"), value["dependencies"]["helper"].clone()))
            .collect(),
    );
    assert_eq!(parse(&value).unwrap().dependencies.len(), MAX_ENTRIES);
    value["dependencies"]["overflow"] = value["dependencies"]["d0"].clone();
    assert_eq!(parse(&value).unwrap_err().code, "PKG0004");
    value["dependencies"] = json!({});
    let mut bytes = serde_json::to_vec(&value).unwrap();
    bytes.resize(MAX_BYTES, b' ');
    Manifest::parse(&bytes).unwrap();
    bytes.push(b' ');
    assert_eq!(Manifest::parse(&bytes).unwrap_err().code, "PKG0004");
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
    assert_eq!(parse(&json!({"schema":3})).unwrap_err().code, "PKG0002");
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
    std::fs::write(
        &path,
        include_bytes!("fixtures/package-lock/schema3-registry-inputs/app/Lang.json"),
    )
    .unwrap();
    let registry = invoke(&[]);
    assert!(registry.status.success());
    assert!(registry.stdout.is_empty() && registry.stderr.is_empty());
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

#[test]
fn nonregular_manifest_inputs_refuse_without_waiting_for_a_writer() {
    let files = Files::new();
    assert_eq!(Manifest::read(&files.0).unwrap_err().code, "PKG0005");
    #[cfg(unix)]
    {
        let fifo = files.0.join("Lang.json");
        assert!(
            Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let mut child = Command::new(env!("CARGO_BIN_EXE_lang"))
            .args(["package", "check-manifest", "--message-format", "json"])
            .arg(&fifo)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!("manifest inspection blocked on a FIFO: {output:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let diagnostic: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(diagnostic["code"], "PKG0005");
        assert_eq!(diagnostic["path"], fifo.to_string_lossy().as_ref());
        assert!(
            diagnostic["message"]
                .as_str()
                .unwrap()
                .contains("regular file")
        );
    }
}
