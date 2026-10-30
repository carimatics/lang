use lang::package::{compatibility::SourceEdition, manifest::Manifest};
use serde_json::{Value, json};
use std::process::Command;

fn cli(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lang"))
        .arg("compatibility")
        .args(arguments)
        .output()
        .unwrap()
}

#[test]
fn report_lists_exact_supported_editions_and_independent_contracts() {
    let output = cli(&["--message-format", "json"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report.as_object().unwrap().len(), 5);
    assert_eq!(report["schema"], 1);
    assert_eq!(report["compiler_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["source_editions"], json!(["2026"]));
    assert_eq!(report["default_source_edition"], "2026");
    let domains = report["domains"].as_array().unwrap();
    assert_eq!(
        domains
            .iter()
            .map(|d| d["domain"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "source",
            "package",
            "bytecode",
            "native-abi",
            "component-abi",
            "tooling",
            "bootstrap"
        ]
    );
    assert_eq!(
        domains
            .iter()
            .filter(|d| d["status"] == "stabilized")
            .map(|d| &d["domain"])
            .collect::<Vec<_>>(),
        [&json!("component-abi")]
    );
    assert_eq!(domains[6]["status"], "unavailable");
    for domain in domains {
        assert_eq!(domain.as_object().unwrap().len(), 3);
        assert!(!domain["policy"].as_str().unwrap().is_empty());
    }
    // A reported edition must actually pass the ordinary manifest boundary.
    for edition in report["source_editions"].as_array().unwrap() {
        let manifest = json!({"schema":1,"package":{
            "name":"consumer","version":"0.1.0","edition":edition,"entry":"main.lang"
        }});
        Manifest::parse(&serde_json::to_vec(&manifest).unwrap()).unwrap();
    }
}

#[test]
fn human_report_is_default_and_preserves_public_private_distinction() {
    let implicit = cli(&[]);
    let explicit = cli(&["--message-format", "human"]);
    assert!(implicit.status.success() && explicit.status.success());
    assert!(implicit.stderr.is_empty() && explicit.stderr.is_empty());
    assert_eq!(implicit.stdout, explicit.stdout);
    let text = String::from_utf8(implicit.stdout).unwrap();
    assert!(text.contains("Source editions: 2026; standalone/REPL default: 2026"));
    assert!(text.contains("Only public C v1 is stabilized"));
    assert!(text.contains("native-abi [experimental]"));
    assert!(text.contains("bootstrap [unavailable]"));
}

#[test]
fn usage_errors_never_emit_a_success_report_and_json_stays_structured() {
    for arguments in [
        vec!["--unknown"],
        vec!["Lang.json"],
        vec!["--message-format"],
        vec!["--message-format", "yaml"],
        vec!["--message-format", "json", "--unknown"],
        vec!["--message-format", "json", "--message-format", "human"],
        vec!["--message-format", "json", "--message-format", "json"],
    ] {
        let output = cli(&arguments);
        assert_eq!(output.status.code(), Some(2), "{arguments:?}");
        assert!(output.stdout.is_empty());
        if arguments.contains(&"json") {
            let error: Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["code"], "CLI0001");
            assert_eq!(error["schema"], 1);
            assert_eq!(error["severity"], "error");
        } else {
            assert!(
                String::from_utf8(output.stderr)
                    .unwrap()
                    .contains("CLI0001")
            );
        }
    }
}

#[test]
fn unsupported_editions_are_not_guessed_or_coerced() {
    assert_eq!(
        SourceEdition::parse("2026").unwrap(),
        SourceEdition::CURRENT
    );
    for edition in [
        "",
        "2025",
        "2027",
        "02026",
        "2026.0",
        "latest",
        "2026 ",
        "２０２６",
    ] {
        assert_eq!(SourceEdition::parse(edition).unwrap_err().code, "PKG0002");
    }
}
