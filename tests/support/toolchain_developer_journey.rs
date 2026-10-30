//! A continuous new-developer workflow using the actual signed installed tools.
#[path = "dap_client.rs"]
mod dap_client;
#[path = "lsp_client.rs"]
mod lsp_client;

use lang::lsp::uri;
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, process::Command};

fn command(compiler: &Path, root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(compiler)
        .current_dir(root)
        .args(args)
        .output()
        .unwrap()
}
fn success(output: std::process::Output) -> std::process::Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

pub(super) fn run(base: &Path, compiler: &Path, sdk: &Path) {
    success(command(compiler, base, &["new", "app"]));
    // DAP captures canonical authority; use its physical spelling for source paths.
    let app = fs::canonicalize(base.join("app")).unwrap();
    success(command(compiler, &app, &["new", "helper"]));
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(app.join("Lang.json")).unwrap()).unwrap();
    manifest["dependencies"] = json!({"helper":{"path":"helper","version":"0.1.0"}});
    fs::write(
        app.join("Lang.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let initial = "// 😀 installed developer journey\nmod helper;\npub fn compute()->i64{return helper::answer();}\nfn main()->i64{let mut unused:i64=7;let mut value:i64=compute();return value;}\n";
    let helper = "/// The dependency's public answer.\npub fn answer()->i64{return 42;}\n";
    fs::write(app.join("src/main.lang"), initial).unwrap();
    fs::write(app.join("helper/src/main.lang"), helper).unwrap();
    fs::write(
        app.join("tests/smoke.lang"),
        "mod _package;fn main()->i64{return _package::compute() + -42;}\n",
    )
    .unwrap();
    success(command(compiler, &app, &["build"]));
    let original_lock = fs::read(app.join("Lang.lock")).unwrap();
    assert_eq!(
        success(command(compiler, &app, &["run", "--locked", "--offline"])).stdout,
        b"42\n"
    );
    editor(compiler, &app, initial, helper);
    let stale = command(
        compiler,
        &app,
        &["run", "--locked", "--offline", "--message-format", "json"],
    );
    assert_eq!(stale.status.code(), Some(1));
    assert!(stale.stdout.is_empty());
    let diagnostic: Value = serde_json::from_slice(&stale.stderr).unwrap();
    assert_eq!(diagnostic["code"], "PKG0007");
    assert_eq!(fs::read(app.join("Lang.lock")).unwrap(), original_lock);
    success(command(compiler, &app, &["fmt"]));
    success(command(compiler, &app, &["fmt", "--check"]));
    success(command(compiler, &app, &["lint", "--deny-warnings"]));
    success(command(compiler, &app, &["build"]));
    let edited_lock = fs::read(app.join("Lang.lock")).unwrap();
    assert_ne!(edited_lock, original_lock);
    success(command(compiler, &app, &["test", "--locked", "--offline"]));
    assert_eq!(
        success(command(compiler, &app, &["run", "--locked", "--offline"])).stdout,
        b"42\n"
    );
    success(command(compiler, &app, &["doc", "--locked", "--offline"]));
    success(command(compiler, &app, &["package", "--list"]));
    success(command(
        compiler,
        &app,
        &["package", "--locked", "--offline"],
    ));
    debug(compiler, &app);
    let first = native(compiler, sdk, &app);
    let relocated = base.join("relocated-app");
    for name in [
        "Lang.json",
        "Lang.lock",
        "src/main.lang",
        "tests/smoke.lang",
        "helper/Lang.json",
        "helper/src/main.lang",
        "helper/tests/smoke.lang",
    ] {
        let to = relocated.join(name);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(app.join(name), to).unwrap();
    }
    assert_eq!(
        native(compiler, sdk, &relocated),
        first,
        "installed-tool native bytes must reproduce after relocation"
    );
    assert_eq!(fs::read(app.join("Lang.lock")).unwrap(), edited_lock);
    assert_eq!(fs::read(relocated.join("Lang.lock")).unwrap(), edited_lock);
}

fn editor(compiler: &Path, app: &Path, initial: &str, helper: &str) {
    use lsp_client::{ApplyError, Client, Document, apply, position};
    let root_uri = uri::file(&app.join("src/main.lang")).unwrap();
    let helper_uri = uri::file(&app.join("helper/src/main.lang")).unwrap();
    let mut client = Client::start(compiler);
    let init = client.call("initialize",json!({"rootUri":uri::file(app).unwrap(),"capabilities":{
        "textDocument":{"publishDiagnostics":{"versionSupport":true},"codeAction":{"codeActionLiteralSupport":{"codeActionKind":{"valueSet":["quickfix"]}}}},
        "workspace":{"semanticTokens":{"refreshSupport":true},"workspaceEdit":{"documentChanges":true,"failureHandling":"textOnlyTransactional"}}}}));
    assert_eq!(init["result"]["capabilities"]["definitionProvider"], true);
    client.note("initialized", json!({}));
    let mut documents = BTreeMap::from([
        (
            root_uri.clone(),
            Document {
                version: 1,
                text: initial.into(),
            },
        ),
        (
            helper_uri.clone(),
            Document {
                version: 1,
                text: helper.into(),
            },
        ),
    ]);
    for (uri, document) in &documents {
        client.note("textDocument/didOpen",json!({"textDocument":{"uri":uri,"languageId":"lang","version":document.version,"text":document.text}}));
    }
    let usage = initial.find("helper::answer").unwrap() + "helper::".len();
    let location = json!({"textDocument":{"uri":root_uri},"position":position(initial,usage)});
    assert_eq!(
        client.call("textDocument/definition", location.clone())["result"]["uri"],
        helper_uri
    );
    let references = client.call("textDocument/references",json!({"textDocument":{"uri":root_uri},"position":position(initial,usage),"context":{"includeDeclaration":true}}));
    assert_eq!(references["result"].as_array().unwrap().len(), 2);
    let completion = client.call(
        "textDocument/completion",
        json!({"textDocument":{"uri":root_uri},"position":position(initial,usage+2)}),
    );
    assert_eq!(completion["result"]["items"][0]["label"], "answer");
    let tokens = client.call(
        "textDocument/semanticTokens/full",
        json!({"textDocument":{"uri":root_uri}}),
    );
    let data = tokens["result"]["data"].as_array().unwrap();
    assert!(!data.is_empty());
    assert_eq!(data.len() % 5, 0);
    assert_eq!(
        client.call("textDocument/prepareRename", location)["result"]["placeholder"],
        "answer"
    );
    let proposed = client.call("textDocument/rename",json!({"textDocument":{"uri":root_uri},"position":position(initial,usage),"newName":"solution"}));
    let mut stale = documents.clone();
    stale.get_mut(&root_uri).unwrap().version += 1;
    let before = stale.clone();
    assert_eq!(
        apply(&mut stale, &proposed["result"]),
        Err(ApplyError::Stale)
    );
    assert_eq!(
        stale, before,
        "a stale edit cannot partially rename a dependency"
    );
    apply(&mut documents, &proposed["result"]).unwrap();
    for (uri, document) in &documents {
        client.note("textDocument/didChange",json!({"textDocument":{"uri":uri,"version":document.version},"contentChanges":[{"text":document.text}]}));
    }
    assert!(documents[&root_uri].text.contains("helper::solution"));
    assert!(documents[&helper_uri].text.contains("fn solution"));
    let current = &documents[&root_uri].text;
    let unused = current.find("unused").unwrap();
    let action = client.call("textDocument/codeAction",json!({"textDocument":{"uri":root_uri},"range":{"start":position(current,unused),"end":position(current,unused+6)},"context":{"diagnostics":[],"only":["quickfix"]}}));
    assert_eq!(action["result"].as_array().unwrap().len(), 1, "{action:?}");
    apply(&mut documents, &action["result"][0]["edit"]).unwrap();
    assert!(documents[&root_uri].text.contains("_unused"));
    client.note("textDocument/didChange",json!({"textDocument":{"uri":root_uri,"version":documents[&root_uri].version},"contentChanges":[{"text":documents[&root_uri].text}]}));
    let updated = client.call(
        "textDocument/semanticTokens/full",
        json!({"textDocument":{"uri":root_uri}}),
    );
    assert_ne!(updated["result"]["resultId"], tokens["result"]["resultId"]);
    client.finish();
    assert_eq!(
        fs::read_to_string(app.join("src/main.lang")).unwrap(),
        initial
    );
    assert_eq!(
        fs::read_to_string(app.join("helper/src/main.lang")).unwrap(),
        helper
    );
    fs::write(app.join("src/main.lang"), &documents[&root_uri].text).unwrap();
    fs::write(
        app.join("helper/src/main.lang"),
        &documents[&helper_uri].text,
    )
    .unwrap();
}

fn debug(compiler: &Path, app: &Path) {
    let source_path = app.join("src/main.lang");
    let source = fs::read_to_string(&source_path).unwrap();
    let line = source
        .lines()
        .position(|l| l.trim() == "return value;")
        .unwrap()
        + 1;
    let mut client = dap_client::DapClient::new(compiler);
    client.request("initialize", json!({"adapterID":"lang"}));
    client.request(
        "launch",
        json!({"root":app,"manifestPath":"Lang.json","mode":"vm"}),
    );
    assert_eq!(client.receive()["event"], "initialized");
    let breakpoints = client.request(
        "setBreakpoints",
        json!({"source":{"path":source_path},"breakpoints":[{"line":line}]}),
    );
    assert_eq!(breakpoints["breakpoints"][0]["verified"], true);
    client.request("configurationDone", json!({}));
    assert_eq!(client.receive()["body"]["reason"], "breakpoint");
    let frames = client.request("stackTrace", json!({"threadId":1}));
    let frame = &frames["stackFrames"][0];
    assert!(frame["name"].as_str().unwrap().contains("main"));
    assert_eq!(frame["line"], line);
    assert_eq!(
        fs::canonicalize(frame["source"]["path"].as_str().unwrap()).unwrap(),
        fs::canonicalize(&source_path).unwrap()
    );
    assert_eq!(
        client.request(
            "source",
            json!({"sourceReference":frame["source"]["sourceReference"]})
        )["content"],
        source
    );
    let scope =
        client.request("scopes", json!({"frameId":frame["id"]}))["scopes"][0]["variablesReference"]
            .clone();
    let variables = client.request("variables", json!({"variablesReference":scope}));
    assert!(
        variables["variables"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["name"] == "value" && v["value"] == "42")
    );
    assert_eq!(
        client.request(
            "evaluate",
            json!({"frameId":frame["id"],"expression":"value","context":"hover"})
        )["result"],
        "42"
    );
    client.request("continue", json!({"threadId":1}));
    assert_eq!(client.receive()["body"]["output"], "42\n");
    assert_eq!(client.receive()["event"], "exited");
    assert_eq!(client.receive()["event"], "terminated");
    client.finish();
}

fn native(compiler: &Path, sdk: &Path, app: &Path) -> Vec<u8> {
    let output = success(
        Command::new(compiler)
            .current_dir(app)
            .args(["build", "--native", "--sdk"])
            .arg(sdk)
            .args(["--locked", "--offline", "--message-format", "json"])
            .env("CARGO", app.join("absent-cargo"))
            .env("CARGO_HOME", app.join("absent-cargo-home"))
            .output()
            .unwrap(),
    );
    let artifact: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(artifact["schema"], 1);
    assert_eq!(artifact["kind"], "artifact");
    let executable = app.join(artifact["path"].as_str().unwrap());
    let bytes = fs::read(&executable).unwrap();
    let receipt: Value =
        serde_json::from_slice(&fs::read(executable.parent().unwrap().join("build.json")).unwrap())
            .unwrap();
    assert_eq!(
        receipt["executable_sha256"],
        lang_codegen_clif::sdk::digest(&bytes)
    );
    assert_eq!(
        receipt["sdk_manifest_sha256"],
        lang_codegen_clif::sdk::digest(&fs::read(sdk.join("sdk.json")).unwrap())
    );
    assert_eq!(
        success(Command::new(executable).output().unwrap()).stdout,
        b"42\n"
    );
    bytes
}
