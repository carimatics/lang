use lang::lsp::{Limits, Server, uri};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
const ROOT: &str = "/*😀*/mod helper;fn main()->i64{return helper::answer();}";
const HELPER: &str = "pub fn answer()->i64{return 42;}";
const MANIFEST: &str = r#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"src/main.lang"},"dependencies":{"helper":{"path":"helper","version":"0.1.0"}}}"#;
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "lang lsp package {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        let f = Self(fs::canonicalize(p).unwrap());
        f.write("Lang.json", MANIFEST);
        f.write("src/main.lang", ROOT);
        f.write("helper/Lang.json",r#"{"schema":1,"package":{"name":"helper","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#);
        f.write("helper/src/main.lang", HELPER);
        f
    }
    fn write(&self, p: &str, text: &str) {
        let p = self.0.join(p);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }
    fn uri(&self, p: &str) -> String {
        uri::file(&self.0.join(p)).unwrap()
    }
    fn ready(&self, limits: Limits, options: Value) -> Server {
        let mut s = Server::new(limits);
        let r=s.handle(request("initialize",json!({"rootUri":uri::file(&self.0).unwrap(),"capabilities":{"textDocument":{"publishDiagnostics":{"versionSupport":true,"relatedInformation":true}}},"initializationOptions":options})));
        assert!(r[0]["result"].is_object(), "{r:?}");
        assert_eq!(r[0]["result"]["capabilities"]["definitionProvider"], true);
        assert_eq!(r[0]["result"]["capabilities"]["referencesProvider"], true);
        s.handle(note("initialized", json!({})));
        s
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn request(method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
}
fn note(method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","method":method,"params":params})
}
fn open(s: &mut Server, uri: &str, text: &str) -> Vec<Value> {
    s.handle(note(
        "textDocument/didOpen",
        json!({"textDocument":{"uri":uri,"languageId":"lang","version":1,"text":text}}),
    ))
}
fn change(s: &mut Server, uri: &str, version: i32, text: &str) -> Vec<Value> {
    s.handle(note(
        "textDocument/didChange",
        json!({"textDocument":{"uri":uri,"version":version},"contentChanges":[{"text":text}]}),
    ))
}
fn position() -> Value {
    let offset = ROOT.find("helper::answer").unwrap() + "helper::".len();
    json!({"line":0,"character":ROOT[..offset].encode_utf16().count()})
}
fn query(s: &mut Server, uri: &str) -> Value {
    s.handle(request(
        "textDocument/definition",
        json!({"textDocument":{"uri":uri},"position":position()}),
    ))
    .remove(0)
}
fn diagnostics<'a>(m: &'a [Value], uri: &str) -> &'a Vec<Value> {
    m.iter()
        .find(|v| v["params"]["uri"] == uri)
        .unwrap_or_else(|| panic!("missing {uri}: {m:?}"))["params"]["diagnostics"]
        .as_array()
        .unwrap()
}
fn watch(s: &mut Server, uri: &str) -> Vec<Value> {
    s.handle(note(
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":uri,"type":2}]}),
    ))
}

#[test]
fn real_package_definition_references_and_overlay_lifecycle_keep_original_utf16_locations() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let helper = f.uri("helper/src/main.lang");
    let mut s = f.ready(Default::default(), json!({}));
    assert!(diagnostics(&open(&mut s, &root, ROOT), &root).is_empty());
    let result = query(&mut s, &root);
    assert_eq!(result["result"]["uri"], helper);
    assert_eq!(
        result["result"]["range"],
        json!({"start":{"line":0,"character":7},"end":{"line":0,"character":13}})
    );
    for (include, count) in [(false, 1), (true, 2)] {
        let r=s.handle(request("textDocument/references",json!({"textDocument":{"uri":root},"position":position(),"context":{"includeDeclaration":include}})));
        assert_eq!(r[0]["result"].as_array().unwrap().len(), count);
    }
    let overlay = format!("// moved\n{HELPER}");
    open(&mut s, &helper, &overlay);
    assert_eq!(query(&mut s, &root)["result"]["range"]["start"]["line"], 1);
    let broken = change(&mut s, &helper, 2, "fn answer()->i64{return 42;}");
    assert!(!diagnostics(&broken, &root).is_empty());
    assert!(query(&mut s, &root)["result"].is_null());
    change(&mut s, &helper, 3, &overlay);
    assert!(change(&mut s, &helper, 2, "broken").is_empty());
    assert_eq!(query(&mut s, &root)["result"]["range"]["start"]["line"], 1);
    s.handle(note(
        "textDocument/didClose",
        json!({"textDocument":{"uri":helper}}),
    ));
    assert_eq!(query(&mut s, &root)["result"]["range"]["start"]["line"], 0);
    assert!(!f.0.join("Lang.lock").exists());
    assert!(!f.0.join("target").exists());
}

#[test]
fn watched_closed_dependency_and_dirty_manifest_invalidate_and_recover_the_package_epoch() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let helper = f.uri("helper/src/main.lang");
    let manifest = f.uri("Lang.json");
    let mut s = f.ready(Default::default(), json!({}));
    open(&mut s, &root, ROOT);
    f.write("helper/src/main.lang", &format!("// watched\n{HELPER}"));
    watch(&mut s, &helper);
    assert_eq!(query(&mut s, &root)["result"]["range"]["start"]["line"], 1);
    f.write(
        "helper/src/main.lang",
        "pub fn answer()->i64{return unknown;}",
    );
    let bad = watch(&mut s, &helper);
    assert!(!diagnostics(&bad, &helper).is_empty());
    assert!(query(&mut s, &root)["result"].is_null());
    f.write("helper/src/main.lang", HELPER);
    let repaired = watch(&mut s, &helper);
    assert!(diagnostics(&repaired, &helper).is_empty());
    assert!(
        watch(&mut s, &helper)
            .iter()
            .all(|m| m["params"]["uri"] != helper)
    );
    assert_eq!(query(&mut s, &root)["result"]["uri"], helper);
    open(&mut s, &manifest, MANIFEST);
    let dirty = change(&mut s, &manifest, 2, "{");
    assert!(dirty.iter().any(|m| {
        m["params"]["diagnostics"]
            .as_array()
            .is_some_and(|ds| ds.iter().any(|d| d["code"] == "PKG0007"))
    }));
    assert!(query(&mut s, &root).get("error").is_some());
    let saved = format!("{MANIFEST}\n");
    f.write("Lang.json", &saved);
    change(&mut s, &manifest, 3, &saved);
    watch(&mut s, &manifest);
    assert_eq!(query(&mut s, &root)["result"]["uri"], helper);
}
#[test]
fn protocol_positions_options_and_response_limits_refuse_without_partial_locations() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let mut s = f.ready(Default::default(), json!({}));
    open(&mut s, &root, ROOT);
    for position in [
        json!({"line":0,"character":3}),
        json!({"line":99,"character":0}),
        json!({"line":0,"character":-1}),
    ] {
        let r = s.handle(request(
            "textDocument/definition",
            json!({"textDocument":{"uri":root},"position":position}),
        ));
        assert_eq!(r[0]["error"]["code"], -32602);
        assert!(r[0].get("result").is_none());
    }
    let eol = s.handle(request(
        "textDocument/definition",
        json!({"textDocument":{"uri":root},"position":{"line":0,"character":i32::MAX}}),
    ));
    assert!(eol[0]["error"].is_null());
    assert!(eol[0]["result"].is_null());
    let bad=s.handle(request("textDocument/references",json!({"textDocument":{"uri":root},"position":position(),"context":{"includeDeclaration":"yes"}})));
    assert_eq!(bad[0]["error"]["code"], -32602);
    let limits = Limits {
        mapping: lang_lsp::Limits {
            output_bytes: 32,
            ..Limits::default().mapping
        },
        ..Default::default()
    };
    let mut bounded = f.ready(limits, json!({}));
    open(&mut bounded, &root, ROOT);
    let r = query(&mut bounded, &root);
    assert_eq!(r["error"]["code"], -32603);
    assert!(r.get("result").is_none());
    let mut invalid = Server::new(Default::default());
    let r=invalid.handle(request("initialize",json!({"rootUri":uri::file(&f.0).unwrap(),"capabilities":{},"initializationOptions":{"manifestPath":"../Lang.json"}})));
    assert_eq!(r[0]["error"]["code"], -32602);
    assert!(!f.0.join("Lang.lock").exists());
}
#[test]
fn explicit_member_manifest_uses_the_selected_namespace_without_a_runnable_main() {
    let f = Files::new();
    let helper = f.uri("helper/src/main.lang");
    let mut s=f.ready(Default::default(),json!({"manifestPath":"helper/Lang.json","package":"helper","target":"aarch64-apple-darwin"}));
    assert!(diagnostics(&open(&mut s, &helper, HELPER), &helper).is_empty());
    let r = s.handle(request(
        "textDocument/definition",
        json!({"textDocument":{"uri":helper},"position":{"line":0,"character":8}}),
    ));
    assert_eq!(r[0]["result"]["uri"], helper);
    assert_eq!(r[0]["result"]["range"]["start"]["character"], 7);
    assert!(!f.0.join("helper/Lang.lock").exists());
}
#[test]
fn local_parameter_and_record_fields_keep_distinct_bindings_and_reference_sets() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let source = "struct Pair{value:i64}fn value()->i64{return 0;}fn id(value:i64)->i64{return value;}fn main()->i64{let mut value:i64=1;value=2;let mut pair:Pair=Pair{value:40};pair.value=41;match pair{Pair{value:part}=>{let mut copy:i64=part;},}return id(pair.value)+value;}";
    f.write("src/main.lang", source);
    let mut s = f.ready(Default::default(), json!({}));
    let opened = open(&mut s, &root, source);
    assert!(diagnostics(&opened, &root).is_empty(), "{opened:?}");
    for (usage, declaration, count) in [
        (
            source.find("return value;").unwrap() + 7,
            source.find("fn id(value").unwrap() + 6,
            2,
        ),
        (
            source.rfind("+value").unwrap() + 1,
            source.find("let mut value").unwrap() + 8,
            3,
        ),
        (
            source.rfind("pair.value").unwrap() + 5,
            source.find("Pair{value").unwrap() + 5,
            5,
        ),
        (
            source.find("=part;").unwrap() + 1,
            source.find("value:part").unwrap() + 6,
            2,
        ),
    ] {
        let params = json!({"textDocument":{"uri":root},"position":{"line":0,"character":usage}});
        let reply = s.handle(request("textDocument/definition", params.clone()));
        assert_eq!(reply[0]["result"]["uri"], root, "{reply:?}");
        assert_eq!(
            reply[0]["result"]["range"]["start"]["character"], declaration,
            "{reply:?}"
        );
        let mut refs = params;
        refs["context"] = json!({"includeDeclaration":true});
        let reply = s.handle(request("textDocument/references", refs));
        assert_eq!(
            reply[0]["result"].as_array().unwrap().len(),
            count,
            "{reply:?}"
        );
    }
}
#[cfg(unix)]
#[test]
fn package_source_aliases_keep_the_original_uri_and_cannot_expand_workspace_authority() {
    use std::os::unix::fs::symlink;
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let helper = f.uri("helper/src/main.lang");
    fs::rename(f.0.join("src/main.lang"), f.0.join("src/actual.lang")).unwrap();
    symlink("actual.lang", f.0.join("src/main.lang")).unwrap();
    let mut s = f.ready(Default::default(), json!({}));
    let messages = open(&mut s, &root, ROOT);
    assert!(diagnostics(&messages, &root).is_empty(), "{messages:?}");
    let reply = query(&mut s, &root);
    assert_eq!(reply["result"]["uri"], helper, "{reply:?}");
    let outside = Files::new();
    let link = f.0.join("external.lang");
    symlink(outside.0.join("src/main.lang"), &link).unwrap();
    let messages=s.handle(request("textDocument/definition",json!({"textDocument":{"uri":uri::file(&link).unwrap()},"position":{"line":0,"character":0}})));
    assert!(messages[0].get("error").is_some());
    assert!(
        !messages[0]["error"]["message"]
            .as_str()
            .unwrap()
            .contains(outside.0.to_str().unwrap()),
        "outside authority path leaked"
    );
}
#[test]
fn open_document_uri_spelling_and_version_are_preserved_for_diagnostics_and_definitions() {
    let f = Files::new();
    let root = f
        .uri("src/main.lang")
        .replacen("file://", "file://localhost", 1);
    let mut s = f.ready(Default::default(), json!({}));
    open(&mut s, &root, ROOT);
    let offset = ROOT.find("main()").unwrap();
    let position = json!({"line":0,"character":ROOT[..offset].encode_utf16().count()});
    let r = s.handle(request(
        "textDocument/definition",
        json!({"textDocument":{"uri":root},"position":position}),
    ));
    assert_eq!(r[0]["result"]["uri"], root);
    let invalid = ROOT.replace("helper::answer", "helper::missing");
    let messages = change(&mut s, &root, 2, &invalid);
    assert!(!diagnostics(&messages, &root).is_empty());
    assert!(
        messages
            .iter()
            .any(|m| m["params"]["uri"] == root && m["params"]["version"] == 2)
    );
}
#[test]
fn actual_stdio_package_navigation_returns_only_valid_framed_protocol() {
    use lang::lsp::wire;
    use std::{
        io::{Cursor, Write},
        process::{Command, Stdio},
    };
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let helper = f.uri("helper/src/main.lang");
    let init = request(
        "initialize",
        json!({"rootUri":uri::file(&f.0).unwrap(),"capabilities":{}}),
    );
    let mut definition = request(
        "textDocument/definition",
        json!({"textDocument":{"uri":root},"position":position()}),
    );
    definition["id"] = json!(2);
    let mut shutdown = request("shutdown", Value::Null);
    shutdown["id"] = json!(3);
    let input = [
        init,
        note("initialized", json!({})),
        note(
            "textDocument/didOpen",
            json!({"textDocument":{"uri":root,"languageId":"lang","version":1,"text":ROOT}}),
        ),
        definition,
        shutdown,
        note("exit", Value::Null),
    ];
    let mut bytes = Vec::new();
    for message in input {
        wire::write(&mut bytes, &message, Default::default()).unwrap();
    }
    let mut child = Command::new(env!("CARGO_BIN_EXE_lang"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&bytes).unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut cursor = Cursor::new(output.stdout);
    let mut replies = Vec::<Value>::new();
    while let Some(body) = wire::read(&mut cursor, Default::default()).unwrap() {
        replies.push(serde_json::from_slice(&body).unwrap());
    }
    assert_eq!(
        replies.iter().find(|r| r["id"] == 2).unwrap()["result"]["uri"],
        helper
    );
    assert!(replies.iter().all(|r| r["jsonrpc"] == "2.0"));
}
#[test]
fn explicit_package_selection_reports_a_missing_manifest_and_recovers_after_creation() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let manifest = f.uri("Lang.json");
    fs::remove_file(f.0.join("Lang.json")).unwrap();
    let mut s = f.ready(Default::default(), json!({"package":"app"}));
    let messages = open(&mut s, &root, ROOT);
    assert!(
        diagnostics(&messages, &root)
            .iter()
            .any(|d| d["code"] == "PKG0005")
    );
    assert!(query(&mut s, &root).get("error").is_some());
    f.write("Lang.json", MANIFEST);
    watch(&mut s, &manifest);
    assert_eq!(
        query(&mut s, &root)["result"]["uri"],
        f.uri("helper/src/main.lang")
    );
}
