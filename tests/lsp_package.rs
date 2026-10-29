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

fn complete(s: &mut Server, uri: &str, source: &str, offset: usize) -> Value {
    let before = &source[..offset];
    let line = before.bytes().filter(|b| *b == b'\n').count();
    let character = before.rsplit('\n').next().unwrap().encode_utf16().count();
    s.handle(request(
        "textDocument/completion",
        json!({"textDocument":{"uri":uri},"position":{"line":line,"character":character}}),
    ))
    .remove(0)
}

#[test]
fn completion_uses_public_module_names_and_current_overlay_utf16_replacement() {
    let f = Files::new();
    f.write(
        "helper/src/main.lang",
        "pub fn answer()->i64{return 42;}fn hidden()->i64{return 0;}",
    );
    let root = f.uri("src/main.lang");
    let mut s = f.ready(Default::default(), json!({}));
    let source = "/*😀*/mod helper;fn main()->i64{return helper::ans;}";
    open(&mut s, &root, source);
    let offset = source.find("ans;").unwrap() + 3;
    let result = complete(&mut s, &root, source, offset);
    let items = result["result"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("{result:?}"));
    assert_eq!(items.len(), 1, "{result:?}");
    assert_eq!(items[0]["label"], "answer");
    assert_eq!(items[0]["kind"], 3);
    assert_eq!(items[0]["data"]["version"], 1);
    assert_eq!(
        items[0]["textEdit"]["range"]["start"]["character"],
        source[..offset - 3].encode_utf16().count()
    );
    assert_eq!(
        items[0]["textEdit"]["range"]["end"]["character"],
        source[..offset].encode_utf16().count()
    );
    let empty = source.replace("::ans", "::");
    change(&mut s, &root, 2, &empty);
    let all = complete(&mut s, &root, &empty, empty.find("::;").unwrap() + 2);
    let names: Vec<_> = all["result"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("{all:?}"))
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"answer"), "{all:?}");
    assert!(!names.contains(&"hidden"), "{all:?}");
    let edit = &items[0]["textEdit"];
    let boundary = |key: &str| {
        lang_lsp::text_offset(
            source,
            edit["range"][key]["line"].as_u64().unwrap() as u32,
            edit["range"][key]["character"].as_u64().unwrap() as u32,
            Limits::default().mapping.source_bytes,
        )
        .unwrap()
    };
    let mut fixed = source.to_string();
    fixed.replace_range(
        boundary("start")..boundary("end"),
        edit["newText"].as_str().unwrap(),
    );
    fixed = fixed.replace("::answer;", "::answer();");
    assert!(diagnostics(&change(&mut s, &root, 3, &fixed), &root).is_empty());
    assert_eq!(fs::read_to_string(f.0.join("src/main.lang")).unwrap(), ROOT);
    assert!(!f.0.join("Lang.lock").exists());
    assert!(!f.0.join("target").exists());
}

#[test]
fn completion_recovers_declared_record_fields_and_lexical_bindings() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let mut s = f.ready(Default::default(), json!({}));
    let source = "struct Pair{value:i64,other:i64}fn main()->i64{let mut pair:Pair=Pair{value:42,other:0};return pair.;}";
    open(&mut s, &root, source);
    let r = complete(&mut s, &root, source, source.find(".;").unwrap() + 1);
    let items = r["result"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("{r:?}"));
    let names: Vec<_> = items.iter().map(|i| i["label"].as_str().unwrap()).collect();
    assert_eq!(names, ["other", "value"], "{r:?}");
    assert!(items.iter().all(|i| i["kind"] == 5));
    let source = "fn choose(parameter:i64)->i64{let mut previous:i64=parameter;if true{let mut nested:i64=1;}return pre;}fn main()->i64{return choose(42);}";
    change(&mut s, &root, 2, source);
    let r = complete(&mut s, &root, source, source.find("pre;").unwrap() + 3);
    assert_eq!(r["result"]["items"][0]["label"], "previous", "{r:?}");
    let source = source.replace("return pre;", "return ;");
    change(&mut s, &root, 3, &source);
    let r = complete(&mut s, &root, &source, source.find("return ;").unwrap() + 7);
    let names: Vec<_> = r["result"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("{r:?}"))
        .iter()
        .map(|i| i["label"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"parameter"), "{r:?}");
    assert!(names.contains(&"previous"), "{r:?}");
    assert!(!names.contains(&"nested"), "{r:?}");
}

#[test]
fn completion_uses_resolved_imports_enum_cases_and_type_context() {
    let f = Files::new();
    f.write(
        "helper/src/main.lang",
        "pub fn answer()->i64{return 42;}pub enum Choice{Yes,No}pub struct Pair{value:i64}",
    );
    let root = f.uri("src/main.lang");
    let mut s = f.ready(Default::default(), json!({}));
    for (version, source, needle, expected) in [
        (
            1,
            "mod helper;use helper::answer;fn main()->i64{return ans;}",
            "ans;",
            vec!["answer"],
        ),
        (
            2,
            "mod helper;fn main()->i64{let mut c:helper::Choice=helper::Choice::;return 42;}",
            "::;",
            vec!["No", "Yes"],
        ),
        (
            3,
            "mod helper;fn main()->i64{let mut c:helper::=0;return 42;}",
            "::=",
            vec!["Choice", "Pair"],
        ),
    ] {
        if version == 1 {
            open(&mut s, &root, source);
        } else {
            change(&mut s, &root, version, source);
        }
        let offset = source.find(needle).unwrap()
            + if needle.starts_with("::") {
                2
            } else {
                needle.len() - 1
            };
        let r = complete(&mut s, &root, source, offset);
        let names: Vec<_> = r["result"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("{r:?}"))
            .iter()
            .map(|i| i["label"].as_str().unwrap())
            .collect();
        assert_eq!(names, expected, "source={source}: {r:?}");
    }
}

#[test]
fn completion_refuses_unsupported_context_stale_text_and_bounded_output() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let mut s = f.ready(Default::default(), json!({}));
    let source = "/*😀 helper:: */fn main()->i64{return 42;}";
    open(&mut s, &root, source);
    let r = complete(&mut s, &root, source, source.find("::").unwrap() + 2);
    assert_eq!(r["result"]["items"], json!([]));
    for source in [
        "fn main()->i64{return unknown.;}",
        "struct Pair{value:i64,value:i64}fn main()->i64{let mut p:Pair=Pair{value:42};return p.;}",
    ] {
        let version = if source.contains("unknown") { 2 } else { 3 };
        change(&mut s, &root, version, source);
        let r = complete(&mut s, &root, source, source.find(".;").unwrap() + 1);
        assert_eq!(r["result"]["items"], json!([]), "{r:?}");
    }
    let current = "mod helper;fn main()->i64{return helper::ans;}";
    change(&mut s, &root, 4, current);
    assert!(change(&mut s, &root, 3, "broken").is_empty());
    let r = complete(&mut s, &root, current, current.find("ans;").unwrap() + 3);
    assert_eq!(r["result"]["items"][0]["data"]["version"], 4, "{r:?}");
    let mut bounded = f.ready(
        Limits {
            mapping: lang_lsp::Limits {
                output_bytes: 32,
                ..Limits::default().mapping
            },
            ..Default::default()
        },
        json!({}),
    );
    open(&mut bounded, &root, current);
    let r = complete(
        &mut bounded,
        &root,
        current,
        current.find("ans;").unwrap() + 3,
    );
    assert_eq!(r["error"]["code"], -32603, "{r:?}");
    assert!(r.get("result").is_none());
    change(&mut s, &root, 5, source);
    let r = s.handle(request(
        "textDocument/completion",
        json!({"textDocument":{"uri":root},"position":{"line":0,"character":3}}),
    ));
    assert_eq!(r[0]["error"]["code"], -32602);
}

#[test]
fn completion_candidate_limit_refuses_the_entire_response() {
    let f = Files::new();
    let helper = (0..257)
        .map(|i| format!("pub fn candidate{i}()->i64{{return 42;}}"))
        .collect::<String>();
    f.write("helper/src/main.lang", &helper);
    let root = f.uri("src/main.lang");
    let mut s = f.ready(Default::default(), json!({}));
    let source = "mod helper;fn main()->i64{return helper::;}";
    open(&mut s, &root, source);
    let r = complete(&mut s, &root, source, source.find("::;").unwrap() + 2);
    assert_eq!(r["error"]["code"], -32603, "{r:?}");
    assert!(r.get("result").is_none());
}

fn token_query(s: &mut Server, uri: &str) -> Value {
    s.handle(request(
        "textDocument/semanticTokens/full",
        json!({"textDocument":{"uri":uri}}),
    ))
    .remove(0)
}

#[test]
fn checked_rename_reanalyzes_a_real_dependency_and_returns_versioned_original_ranges() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let helper = f.uri("helper/src/main.lang");
    let mut s = Server::new(Default::default());
    let init=s.handle(request("initialize",json!({"rootUri":uri::file(&f.0).unwrap(),"capabilities":{"workspace":{"workspaceEdit":{"documentChanges":true,"failureHandling":"textOnlyTransactional"}}}})));
    assert_eq!(
        init[0]["result"]["capabilities"]["renameProvider"]["prepareProvider"],
        true
    );
    s.handle(note("initialized", json!({})));
    open(&mut s, &root, ROOT);
    open(&mut s, &helper, HELPER);
    let prepare = s.handle(request(
        "textDocument/prepareRename",
        json!({"textDocument":{"uri":root},"position":position()}),
    ));
    assert_eq!(prepare[0]["result"]["placeholder"], "answer", "{prepare:?}");
    let renamed = s.handle(request(
        "textDocument/rename",
        json!({"textDocument":{"uri":root},"position":position(),"newName":"solution"}),
    ));
    let changes = renamed[0]["result"]["documentChanges"]
        .as_array()
        .unwrap_or_else(|| panic!("{renamed:?}"));
    assert_eq!(changes.len(), 2, "{renamed:?}");
    let mut updated = std::collections::BTreeMap::new();
    for change in changes {
        let uri = change["textDocument"]["uri"].as_str().unwrap();
        assert_eq!(change["textDocument"]["version"], 1);
        let original = if uri == root {
            ROOT
        } else {
            assert_eq!(uri, helper);
            HELPER
        };
        let mut text = original.to_string();
        for edit in change["edits"].as_array().unwrap().iter().rev() {
            let offset = |end: &str| {
                lang_lsp::text_offset(
                    original,
                    edit["range"][end]["line"].as_u64().unwrap() as u32,
                    edit["range"][end]["character"].as_u64().unwrap() as u32,
                    original.len(),
                )
                .unwrap()
            };
            let start = offset("start");
            let end = offset("end");
            assert_eq!(&original[start..end], "answer");
            text.replace_range(start..end, edit["newText"].as_str().unwrap());
        }
        updated.insert(uri.to_string(), text);
    }
    change(&mut s, &root, 2, &updated[&root]);
    let diagnostics_after = change(&mut s, &helper, 2, &updated[&helper]);
    assert!(
        diagnostics(&diagnostics_after, &root).is_empty(),
        "{diagnostics_after:?}"
    );
    assert!(
        diagnostics(&diagnostics_after, &helper).is_empty(),
        "{diagnostics_after:?}"
    );
    assert_eq!(fs::read_to_string(f.0.join("src/main.lang")).unwrap(), ROOT);
    assert_eq!(
        fs::read_to_string(f.0.join("helper/src/main.lang")).unwrap(),
        HELPER
    );
    assert!(!f.0.join("Lang.lock").exists());
    assert!(!f.0.join("target").exists());
}

fn rename_server(f: &Files, handling: &str) -> Server {
    rename_server_limits(f, handling, Default::default())
}
fn rename_server_limits(f: &Files, handling: &str, limits: Limits) -> Server {
    let mut s = Server::new(limits);
    let r=s.handle(request("initialize",json!({"rootUri":uri::file(&f.0).unwrap(),"capabilities":{"workspace":{"workspaceEdit":{"documentChanges":true,"failureHandling":handling}}}})));
    assert!(r[0]["error"].is_null(), "{r:?}");
    s.handle(note("initialized", json!({})));
    s
}
fn rename_query(s: &mut Server, uri: &str, source: &str, offset: usize, name: &str) -> Value {
    let before = &source[..offset];
    let line = before.bytes().filter(|b| *b == b'\n').count();
    let character = before.rsplit('\n').next().unwrap().encode_utf16().count();
    s.handle(request("textDocument/rename",json!({"textDocument":{"uri":uri},"position":{"line":line,"character":character},"newName":name}))).remove(0)
}

#[test]
fn checked_rename_preserves_parameters_assignments_fields_cases_types_loans_and_async_calls() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    for (source, needle, new_name, count) in [
        (
            "fn id(value:i64)->i64{return value;}fn main()->i64{return id(42);}",
            "return value",
            "parameter",
            2,
        ),
        (
            "fn main()->i64{let mut value:i64=1;value=42;return value;}",
            "return value",
            "amount",
            3,
        ),
        (
            "struct Pair{value:i64}fn main()->i64{let mut p:Pair=Pair{value:41};p.value=42;match p{Pair{value:copy}=>{return copy;},}}",
            "p.value",
            "amount",
            4,
        ),
        (
            "enum E{Yes,No}fn main()->i64{let mut e:E=E::Yes;match e{E::Yes=>{return 42;},E::No=>{return 0;},}}",
            "E::Yes",
            "Accepted",
            3,
        ),
        (
            "struct Pair{value:i64}fn main()->i64{let mut p:Pair=Pair{value:42};return p.value;}",
            "p:Pair",
            "Tuple",
            3,
        ),
        (
            "fn main()->i64{let mut value:i64=41;if true{let mut pointer:&mut i64=&mut value;(*pointer)=42;}return value;}",
            "return value",
            "amount",
            3,
        ),
        (
            "async fn answer()->i64{return 42;}async fn main()->i64{return core::result_unwrap::<i64,core::TaskError>(await answer());}",
            "await answer",
            "solution",
            2,
        ),
    ] {
        let mut s = rename_server(&f, "transactional");
        let messages = open(&mut s, &root, source);
        assert!(
            diagnostics(&messages, &root).is_empty(),
            "source={source}: {messages:?}"
        );
        let start = source.find(needle).unwrap();
        let offset = start
            + if needle.starts_with("return ") {
                7
            } else if needle.starts_with("await ") {
                6
            } else if needle.starts_with("E::") {
                3
            } else {
                2
            };
        let r = rename_query(&mut s, &root, source, offset, new_name);
        let changes = r["result"]["documentChanges"]
            .as_array()
            .unwrap_or_else(|| panic!("source={source}: {r:?}"));
        assert_eq!(changes.len(), 1, "{r:?}");
        assert_eq!(
            changes[0]["edits"].as_array().unwrap().len(),
            count,
            "source={source}: {r:?}"
        );
    }
}

#[test]
fn checked_rename_refuses_capture_collisions_invalid_names_aliases_and_missing_client_guarantees() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let helper = f.uri("helper/src/main.lang");
    let source = "fn main()->i64{let mut first:i64=1;if true{let mut second:i64=2;return first+second;}return first;}";
    let mut s = rename_server(&f, "transactional");
    open(&mut s, &root, source);
    for name in ["second", "fn", "_", "two words", ""] {
        let r = rename_query(
            &mut s,
            &root,
            source,
            source.find("return first").unwrap() + 7,
            name,
        );
        assert!(r.get("error").is_some(), "{name}: {r:?}");
        assert!(r.get("result").is_none());
        if name == "second" {
            assert!(
                r["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("would invalidate"),
                "{r:?}"
            );
        }
    }
    let imported = "mod helper;use helper::answer;fn main()->i64{return answer();}";
    change(&mut s, &root, 2, imported);
    open(&mut s, &helper, HELPER);
    let r = rename_query(
        &mut s,
        &root,
        imported,
        imported.find("return answer").unwrap() + 7,
        "solution",
    );
    assert!(
        r["error"]["message"].as_str().unwrap().contains("alias"),
        "{r:?}"
    );
    let mut closed = rename_server(&f, "transactional");
    open(&mut closed, &root, ROOT);
    let r = rename_query(
        &mut closed,
        &root,
        ROOT,
        ROOT.find("helper::answer").unwrap() + 8,
        "solution",
    );
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("must be open"),
        "{r:?}"
    );
    let mut non_atomic = rename_server(&f, "undo");
    open(&mut non_atomic, &root, ROOT);
    open(&mut non_atomic, &helper, HELPER);
    let r = rename_query(
        &mut non_atomic,
        &root,
        ROOT,
        ROOT.find("helper::answer").unwrap() + 8,
        "solution",
    );
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("transactional"),
        "{r:?}"
    );
    let mut unsupported = f.ready(Default::default(), json!({}));
    open(&mut unsupported, &root, ROOT);
    open(&mut unsupported, &helper, HELPER);
    let r = rename_query(
        &mut unsupported,
        &root,
        ROOT,
        ROOT.find("helper::answer").unwrap() + 8,
        "solution",
    );
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("documentChanges"),
        "{r:?}"
    );
}

#[test]
fn checked_rename_rechecks_closed_inputs_metadata_versions_and_response_limits() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let helper = f.uri("helper/src/main.lang");
    let source = "fn main()->i64{let mut value:i64=42;return value;}";
    let offset = source.find("return value").unwrap() + 7;
    let mut s = rename_server(&f, "transactional");
    open(&mut s, &root, source);
    f.write("helper/src/main.lang", "pub fn answer()->i64{return 43;}");
    let stale = rename_query(&mut s, &root, source, offset, "amount");
    assert!(
        stale["error"]["message"]
            .as_str()
            .unwrap()
            .contains("refresh"),
        "{stale:?}"
    );
    watch(&mut s, &helper);
    let r = rename_query(&mut s, &root, source, offset, "amount");
    assert_eq!(
        r["result"]["documentChanges"][0]["textDocument"]["version"], 1,
        "{r:?}"
    );
    change(&mut s, &root, 5, source);
    assert!(change(&mut s, &root, 4, "broken").is_empty());
    let r = rename_query(&mut s, &root, source, offset, "amount");
    assert_eq!(
        r["result"]["documentChanges"][0]["textDocument"]["version"], 5,
        "{r:?}"
    );
    f.write("Lang.json", &format!("{MANIFEST}\n"));
    let stale = rename_query(&mut s, &root, source, offset, "amount");
    assert!(
        stale["error"]["message"]
            .as_str()
            .unwrap()
            .contains("refresh"),
        "{stale:?}"
    );
    let mut bounded = rename_server_limits(
        &f,
        "transactional",
        Limits {
            mapping: lang_lsp::Limits {
                output_bytes: 96,
                ..Limits::default().mapping
            },
            ..Default::default()
        },
    );
    open(&mut bounded, &root, source);
    let r = rename_query(&mut bounded, &root, source, offset, "amount");
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("byte limit"),
        "{r:?}"
    );
    assert!(r.get("result").is_none());
    assert_eq!(fs::read_to_string(f.0.join("src/main.lang")).unwrap(), ROOT);
    assert!(!f.0.join("Lang.lock").exists());
    assert!(!f.0.join("target").exists());
}

#[test]
fn checked_rename_refuses_module_and_external_abi_declarations() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    for (source, needle) in [
        (ROOT, "helper"),
        (
            "extern \"C\" fn external(x:i64)->i64;fn main()->i64{return 42;}",
            "external",
        ),
        (
            "pub extern \"C\" fn exported(x:i64)->i64{return x;}fn main()->i64{return 42;}",
            "exported",
        ),
    ] {
        let mut s = rename_server(&f, "transactional");
        let messages = open(&mut s, &root, source);
        assert!(diagnostics(&messages, &root).is_empty(), "{messages:?}");
        let r = rename_query(
            &mut s,
            &root,
            source,
            source.find(needle).unwrap(),
            "changed",
        );
        assert!(r.get("error").is_some(), "{r:?}");
        assert!(r.get("result").is_none());
    }
}

#[test]
fn checked_rename_bounds_repeated_utf16_mapping_before_returning_any_edit() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let mut source = format!(
        "/*{}*/fn answer()->i64{{return 42;}}fn main()->i64{{",
        "x".repeat(40_000)
    );
    for i in 0..100 {
        source.push_str(&format!("let mut local{i}:i64=answer();"));
    }
    source.push_str("return 0;}");
    let mut s = rename_server(&f, "transactional");
    let messages = open(&mut s, &root, &source);
    assert!(diagnostics(&messages, &root).is_empty(), "{messages:?}");
    let r = rename_query(
        &mut s,
        &root,
        &source,
        source.find("fn answer").unwrap() + 3,
        "reply",
    );
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("mapping work limit"),
        "{r:?}"
    );
    assert!(r.get("result").is_none());
    assert_eq!(fs::read_to_string(f.0.join("src/main.lang")).unwrap(), ROOT);
}
fn decoded_tokens(source: &str, result: &Value) -> Vec<(String, u64)> {
    let data = result["result"]["data"]
        .as_array()
        .unwrap_or_else(|| panic!("{result:?}"));
    assert_eq!(data.len() % 5, 0);
    let (mut line, mut start, mut previous_end) = (0u32, 0u32, 0u32);
    let mut out = Vec::new();
    for token in data.as_chunks::<5>().0 {
        let delta_line = token[0].as_u64().unwrap() as u32;
        line += delta_line;
        if delta_line != 0 {
            start = 0;
            previous_end = 0;
        }
        start += token[1].as_u64().unwrap() as u32;
        assert!(start >= previous_end, "overlap: {result:?}");
        let len = token[2].as_u64().unwrap() as u32;
        assert!(len > 0);
        let a = lang_lsp::text_offset(source, line, start, source.len()).unwrap();
        let b = lang_lsp::text_offset(source, line, start + len, source.len()).unwrap();
        assert!(!source[a..b].contains(['\r', '\n']));
        assert_eq!(source[a..b].encode_utf16().count(), len as usize);
        assert_eq!(token[4], 0);
        out.push((source[a..b].to_string(), token[3].as_u64().unwrap()));
        previous_end = start + len;
    }
    out
}

#[test]
fn semantic_tokens_preserve_utf16_crlf_and_proven_symbol_kinds() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let source = "/*😀\r\nsecond*/\r\nstruct Pair{value:i64}\r\nfn id(parameter:i64)->i64{let mut local:i64=parameter;return local;}\r\nfn main()->i64{let mut pair:Pair=Pair{value:42};return id(pair.value);}\r\n";
    let mut s = f.ready(Default::default(), json!({}));
    let messages = open(&mut s, &root, source);
    assert!(diagnostics(&messages, &root).is_empty(), "{messages:?}");
    assert!(
        !messages
            .iter()
            .any(|m| m["method"] == "workspace/semanticTokens/refresh")
    );
    let r = token_query(&mut s, &root);
    let tokens = decoded_tokens(source, &r);
    for (text, kind, count) in [
        ("/*😀", 8, 1),
        ("second*/", 8, 1),
        ("Pair", 1, 3),
        ("value", 6, 3),
        ("id", 3, 2),
        ("parameter", 4, 2),
        ("local", 5, 2),
        ("42", 10, 1),
    ] {
        assert_eq!(
            tokens
                .iter()
                .filter(|(t, k)| t == text && *k == kind)
                .count(),
            count,
            "{text}: {tokens:?}"
        );
    }
    let broken = "/*😀\r\ninvalid*/\r\nfn main( { unknown+42;";
    change(&mut s, &root, 2, broken);
    let current = token_query(&mut s, &root);
    assert_ne!(current["result"]["resultId"], r["result"]["resultId"]);
    let fallback = decoded_tokens(broken, &current);
    assert!(fallback.iter().any(|(t, k)| t == "fn" && *k == 7));
    assert!(!fallback.iter().any(|(t, _)| t == "main" || t == "unknown"));
    assert!(fallback.iter().any(|(t, k)| t == "42" && *k == 10));
    assert!(change(&mut s, &root, 1, source).is_empty());
    assert_eq!(token_query(&mut s, &root), current);
    let bare_cr = source.replace("\r\n", "\r");
    change(&mut s, &root, 3, &bare_cr);
    assert_eq!(
        decoded_tokens(&bare_cr, &token_query(&mut s, &root)),
        tokens
    );
    s.handle(note(
        "textDocument/didClose",
        json!({"textDocument":{"uri":root}}),
    ));
    let disk = token_query(&mut s, &root);
    assert_ne!(disk["result"]["resultId"], current["result"]["resultId"]);
    assert!(!decoded_tokens(ROOT, &disk).is_empty());
}

#[test]
fn semantic_tokens_negotiate_full_only_refresh_and_refuse_output_overflow() {
    let f = Files::new();
    let root = f.uri("src/main.lang");
    let mut s = Server::new(Default::default());
    let init=s.handle(request("initialize",json!({"rootUri":uri::file(&f.0).unwrap(),"capabilities":{"workspace":{"semanticTokens":{"refreshSupport":true}}}})));
    let capability = &init[0]["result"]["capabilities"]["semanticTokensProvider"];
    assert_eq!(capability["full"], true);
    assert_eq!(capability["range"], false);
    assert_eq!(
        capability["legend"]["tokenTypes"],
        json!([
            "namespace",
            "type",
            "enumMember",
            "function",
            "parameter",
            "variable",
            "property",
            "keyword",
            "comment",
            "string",
            "number",
            "operator"
        ])
    );
    assert_eq!(capability["legend"]["tokenModifiers"], json!([]));
    s.handle(note("initialized", json!({})));
    let opened = open(&mut s, &root, ROOT);
    let refresh = opened
        .iter()
        .find(|m| m["method"] == "workspace/semanticTokens/refresh")
        .unwrap();
    assert!(
        s.handle(json!({"jsonrpc":"2.0","id":refresh["id"],"result":null}))
            .is_empty()
    );
    let before = token_query(&mut s, &root);
    f.write("helper/src/main.lang", "pub fn answer()->i64{return 43;}");
    let watched = watch(&mut s, &f.uri("helper/src/main.lang"));
    let next = watched
        .iter()
        .find(|m| m["method"] == "workspace/semanticTokens/refresh")
        .unwrap();
    assert_ne!(next["id"], refresh["id"]);
    assert_ne!(
        token_query(&mut s, &root)["result"]["resultId"],
        before["result"]["resultId"]
    );
    for method in [
        "textDocument/semanticTokens/range",
        "textDocument/semanticTokens/full/delta",
    ] {
        let r = s.handle(request(method, json!({"textDocument":{"uri":root}})));
        assert_eq!(r[0]["error"]["code"], -32601);
    }
    let mut bounded = f.ready(
        Limits {
            mapping: lang_lsp::Limits {
                output_bytes: 64,
                ..Limits::default().mapping
            },
            ..Default::default()
        },
        json!({}),
    );
    open(&mut bounded, &root, ROOT);
    let r = token_query(&mut bounded, &root);
    assert_eq!(r["error"]["code"], -32603, "{r:?}");
    assert!(r.get("result").is_none());
    assert!(!f.0.join("Lang.lock").exists());
    assert!(!f.0.join("target").exists());
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
    let mut completion = request(
        "textDocument/completion",
        json!({"textDocument":{"uri":root},"position":position()}),
    );
    completion["id"] = json!(4);
    let mut tokens = request(
        "textDocument/semanticTokens/full",
        json!({"textDocument":{"uri":root}}),
    );
    tokens["id"] = json!(5);
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
        completion,
        tokens,
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
    assert_eq!(
        replies.iter().find(|r| r["id"] == 4).unwrap()["result"]["items"][0]["label"],
        "answer"
    );
    assert!(!decoded_tokens(ROOT, replies.iter().find(|r| r["id"] == 5).unwrap()).is_empty());
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
