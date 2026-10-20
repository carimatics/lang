use lang::lsp::{self, Limits, Server, uri, wire};
use serde_json::{Value, json};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lang lsp {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(std::fs::canonicalize(path).unwrap())
    }
    fn file(&self, name: &str, text: &str) -> String {
        std::fs::write(self.0.join(name), text).unwrap();
        uri::file(&self.0.join(name)).unwrap()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn request(id: i32, method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":params})
}
fn note(method: &str, params: Value) -> Value {
    json!({"jsonrpc":"2.0","method":method,"params":params})
}
fn initialize(root: Option<&Path>, capabilities: Value) -> Value {
    request(
        1,
        "initialize",
        json!({"processId":null,"rootUri":root.map(|p|uri::file(p).unwrap()),"capabilities":capabilities}),
    )
}
fn ready(root: Option<&Path>, capabilities: Value) -> Server {
    let mut server = Server::new(Limits::default());
    assert!(
        server.handle(initialize(root, capabilities))[0]
            .get("result")
            .is_some()
    );
    server.handle(note("initialized", json!({})));
    server
}
fn open(server: &mut Server, uri: &str, text: &str) -> Vec<Value> {
    server.handle(note(
        "textDocument/didOpen",
        json!({"textDocument":{"uri":uri,"languageId":"lang","version":1,"text":text}}),
    ))
}
fn change(server: &mut Server, uri: &str, version: i32, text: &str) -> Vec<Value> {
    server.handle(note(
        "textDocument/didChange",
        json!({"textDocument":{"uri":uri,"version":version},"contentChanges":[{"text":text}]}),
    ))
}
fn diagnostics<'a>(messages: &'a [Value], uri: &str) -> &'a Vec<Value> {
    messages.iter().find(|m|m["method"]=="textDocument/publishDiagnostics" && m["params"]["uri"]==uri)
        .unwrap_or_else(||panic!("missing publication for {uri}: {messages:?}"))["params"]["diagnostics"].as_array().unwrap()
}
fn code(messages: &[Value]) -> i64 {
    messages[0]["error"]["code"].as_i64().unwrap()
}
fn frames(messages: &[Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for message in messages {
        wire::write(&mut bytes, message, wire::Limits::default()).unwrap();
    }
    bytes
}
fn decode(bytes: &[u8]) -> Vec<Value> {
    let mut cursor = Cursor::new(bytes);
    let mut messages = Vec::new();
    while let Some(bytes) = wire::read(&mut cursor, wire::Limits::default()).unwrap() {
        messages.push(serde_json::from_slice(&bytes).unwrap());
    }
    messages
}

#[test]
fn initialization_lifecycle_and_capabilities_match_the_supported_profile() {
    let mut server = Server::new(Limits::default());
    assert_eq!(
        code(&server.handle(request(4, "unknown", json!({})))),
        -32002
    );
    assert!(
        server
            .handle(note("textDocument/didOpen", json!({})))
            .is_empty()
    );
    let reply = server.handle(initialize(
        None,
        json!({"general":{"positionEncodings":["utf-8"]}}),
    ));
    let caps = &reply[0]["result"]["capabilities"];
    assert_eq!(caps["positionEncoding"], "utf-16");
    assert_eq!(caps["textDocumentSync"]["change"], 1);
    assert_eq!(caps["documentFormattingProvider"], true);
    assert!(caps.get("completionProvider").is_none());
    assert_eq!(code(&server.handle(initialize(None, json!({})))), -32600);
    server.handle(note("initialized", json!({})));
    assert_eq!(
        code(&server.handle(request(5, "unknown", json!({})))),
        -32601
    );
    assert_eq!(code(&server.handle(json!([]))), -32600);
    assert_eq!(
        code(&server.handle(json!({"jsonrpc":"2.0","id":1.5,"method":"x"}))),
        -32600
    );
    assert_eq!(
        code(&server.handle(request(6, "exit", json!(null)))),
        -32600
    );
    assert!(server.handle(request(7, "shutdown", json!(null)))[0]["result"].is_null());
    assert_eq!(
        code(&server.handle(request(8, "unknown", json!(null)))),
        -32600
    );
    server.handle(note("exit", json!(null)));
    assert_eq!(server.exit_status(), Some(0));
    let mut server = Server::new(Limits::default());
    server.handle(note("exit", json!(null)));
    assert_eq!(server.exit_status(), Some(1));
}

#[test]
fn utf16_incomplete_edits_versions_close_and_reopen_use_current_source() {
    let files = Files::new();
    let name = files.file("unicode.lang", "");
    let mut server = ready(
        None,
        json!({"textDocument":{"publishDiagnostics":{"versionSupport":true}}}),
    );
    let text = "fn main() -> i64 { /*😀*/ return missing; }";
    let messages = open(&mut server, &name, text);
    let diagnostic = diagnostics(&messages, &name)
        .iter()
        .find(|d| d["code"] == "SRC0002")
        .unwrap();
    let expected = text[..text.find("missing").unwrap()].encode_utf16().count();
    assert_eq!(diagnostic["range"]["start"]["character"], expected);
    assert!(diagnostic.get("relatedInformation").is_none());
    assert_eq!(messages[0]["params"]["version"], 1);
    let messages = change(&mut server, &name, 2, "fn main() -> i64 { return 42; }");
    assert!(diagnostics(&messages, &name).is_empty());
    assert!(change(&mut server, &name, 1, text).is_empty());
    assert!(change(&mut server, &name, 2, text).is_empty());
    let before = server.stats().unwrap();
    let invalid=server.handle(note("textDocument/didChange",json!({"textDocument":{"uri":name,"version":3},"contentChanges":[{"range":{},"text":text}]})));
    assert_eq!(invalid[0]["method"], "window/logMessage");
    assert_eq!(server.stats().unwrap(), before);
    let incomplete = change(&mut server, &name, 3, "fn main() -> i64 { return");
    assert!(!diagnostics(&incomplete, &name).is_empty());
    let closed = server.handle(note(
        "textDocument/didClose",
        json!({"textDocument":{"uri":name}}),
    ));
    assert!(diagnostics(&closed, &name).is_empty());
    assert!(
        diagnostics(
            &open(&mut server, &name, "fn main() -> i64 { return 7; }"),
            &name
        )
        .is_empty()
    );
}

#[test]
fn child_overlays_use_parent_context_and_closed_dependencies_route_to_open_root() {
    let files = Files::new();
    let root_text = "mod math; fn main() -> i64 { return math::answer(); }";
    let root = files.file("root.lang", root_text);
    let child = files.file("math.lang", "pub fn answer() -> i64 { return missing; }");
    let mut server = ready(
        Some(&files.0),
        json!({"textDocument":{"publishDiagnostics":{"relatedInformation":true}}}),
    );
    let messages = open(&mut server, &root, root_text);
    let d = diagnostics(&messages, &root)
        .iter()
        .find(|d| d["code"] == "SRC0002")
        .unwrap();
    assert_eq!(d["relatedInformation"][0]["location"]["uri"], child);
    assert_eq!(d["range"]["start"]["character"], 4);
    let messages = open(&mut server, &child, "pub fn answer() -> i64 { return 42; }");
    assert!(diagnostics(&messages, &root).is_empty());
    assert!(diagnostics(&messages, &child).is_empty());
    let messages = change(
        &mut server,
        &child,
        2,
        "pub fn answer() -> i64 { return absent; }",
    );
    assert!(diagnostics(&messages, &root).is_empty());
    assert!(
        diagnostics(&messages, &child)
            .iter()
            .any(|d| d["code"] == "SRC0002")
    );
    let messages = server.handle(note(
        "textDocument/didClose",
        json!({"textDocument":{"uri":child}}),
    ));
    assert_eq!(
        messages
            .iter()
            .filter(|m| m["params"]["uri"] == child)
            .count(),
        1
    );
    assert!(diagnostics(&messages, &child).is_empty());
    assert!(
        diagnostics(&messages, &root)
            .iter()
            .any(|d| d["code"] == "SRC0002")
    );
    assert!(
        messages
            .iter()
            .all(|m| m["params"].get("version").is_none())
    );
}

#[test]
fn watched_files_recover_missing_modules_without_overwriting_open_buffers() {
    let files = Files::new();
    let text = "mod math; fn main() -> i64 { return math::answer(); }";
    let root = files.file("root.lang", text);
    let mut server = Server::new(Limits::default());
    server.handle(initialize(
        Some(&files.0),
        json!({"workspace":{"didChangeWatchedFiles":{"dynamicRegistration":true}}}),
    ));
    let registration = server.handle(note("initialized", json!({})));
    assert_eq!(registration[0]["method"], "client/registerCapability");
    server.handle(json!({"jsonrpc":"2.0","id":"lang-watch","result":null}));
    assert!(!diagnostics(&open(&mut server, &root, text), &root).is_empty());
    let child = files.file("math.lang", "pub fn answer() -> i64 { return 42; }");
    let event = note(
        "workspace/didChangeWatchedFiles",
        json!({"changes":[{"uri":child,"type":1}]}),
    );
    assert!(diagnostics(&server.handle(event.clone()), &root).is_empty());
    open(
        &mut server,
        &child,
        "pub fn answer() -> i64 { return missing; }",
    );
    let messages = server.handle(event);
    assert!(!diagnostics(&messages, &child).is_empty());
}

#[test]
fn canonical_formatter_returns_utf16_whole_document_edits_and_preserves_invalid_input() {
    let files = Files::new();
    let name = files.file("format.lang", "");
    let text = "//😀\r\nfn main()->i64{return 42;}";
    let mut server = ready(None, json!({}));
    open(&mut server, &name, text);
    let params = json!({"textDocument":{"uri":name},"options":{"tabSize":8,"insertSpaces":false}});
    let result = server.handle(request(2, "textDocument/formatting", params.clone()));
    let edit = &result[0]["result"][0];
    assert_eq!(edit["range"]["start"], json!({"line":0,"character":0}));
    assert_eq!(edit["range"]["end"], json!({"line":1,"character":26}));
    let formatted = edit["newText"].as_str().unwrap();
    assert!(formatted.contains("    return 42;"), "{formatted}");
    change(&mut server, &name, 2, formatted);
    assert_eq!(
        server.handle(request(3, "textDocument/formatting", params.clone()))[0]["result"],
        json!([])
    );
    change(&mut server, &name, 3, "fn main(");
    assert_eq!(
        server.handle(request(4, "textDocument/formatting", params))[0]["result"],
        json!([])
    );
}

#[test]
fn task_source_checks_and_formats_with_independent_mir_evidence() {
    let files = Files::new();
    let name = files.file("task.lang", "");
    let mut server = ready(None, json!({}));
    let text = "//😀\nasync fn main()->i64{task_scope{yield;return 42;}}";
    let messages = open(&mut server, &name, text);
    let codes = diagnostics(&messages, &name)
        .iter()
        .map(|d| d["code"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(codes.is_empty(), "{codes:?}");
    let params = json!({"textDocument":{"uri":name},"options":{"tabSize":4,"insertSpaces":true}});
    let reply = server.handle(request(2, "textDocument/formatting", params.clone()));
    let formatted = reply[0]["result"][0]["newText"].as_str().unwrap();
    assert!(formatted.contains("async fn main() -> i64"));
    assert!(formatted.contains("        yield;"));
    let changed = change(&mut server, &name, 2, formatted);
    assert!(diagnostics(&changed, &name).is_empty());
    assert_eq!(
        server.handle(request(3, "textDocument/formatting", params))[0]["result"],
        json!([])
    );
}

#[test]
fn local_file_uris_round_trip_and_reject_unsupported_authorities_and_encodings() {
    let files = Files::new();
    let path = files.0.join("hello 😀 #%.lang");
    std::fs::write(&path, "").unwrap();
    let name = uri::file(&path).unwrap();
    assert!(name.contains("%F0%9F%98%80"));
    assert!(name.contains("%23%25"));
    for encoded in [&name, &name.replacen("file:///", "file://localhost/", 1)] {
        let decoded = uri::path(encoded, 8192).unwrap();
        // Windows canonicalize returns a verbatim disk prefix, while a file URI
        // decodes to a normal disk path. Compare the target and stable URI form.
        assert_eq!(std::fs::canonicalize(&decoded).unwrap(), path);
        assert_eq!(uri::file(&decoded).unwrap(), name);
    }
    for name in [
        "https://host/a",
        "file://remote/a",
        "file:////remote/a",
        "file:///a?x",
        "file:///a#x",
        "file:///a/%00",
        "file:///a/%FF",
        "file:///a/%",
        "file:///a/../b",
        "file:///a/%5C",
    ] {
        assert!(uri::path(name, 8192).is_err(), "{name}");
    }
    assert!(uri::path(&name, 3).is_err());
    let mut server = ready(Some(&files.0), json!({}));
    let outside = Files::new();
    let rejected = open(
        &mut server,
        &outside.file("outside.lang", ""),
        "fn x() -> i64 { return 0; }",
    );
    assert_eq!(rejected[0]["method"], "window/logMessage");
    assert_eq!(server.stats().unwrap().disk_reads, 0);
}

#[cfg(windows)]
#[test]
fn windows_file_uris_normalize_verbatim_disk_paths_and_reject_unc_paths() {
    let normal = Path::new(r"C:\source space\hello 😀 #%.lang");
    let verbatim = Path::new(r"\\?\C:\source space\hello 😀 #%.lang");
    let encoded = "file:///C:/source%20space/hello%20%F0%9F%98%80%20%23%25.lang";
    assert_eq!(uri::file(normal).unwrap(), encoded);
    assert_eq!(uri::file(verbatim).unwrap(), encoded);
    assert_eq!(uri::path(encoded, 8192).unwrap(), normal);
    for path in [
        r"\\server\share\file.lang",
        r"\\?\UNC\server\share\file.lang",
    ] {
        assert_eq!(uri::file(Path::new(path)), Err(uri::UriError::NonLocal));
    }
}

#[cfg(unix)]
#[test]
fn workspace_symlinks_cannot_grant_source_authority_outside_the_root() {
    let files = Files::new();
    let outside = Files::new();
    outside.file("secret.lang", "fn secret() -> i64 { return 42; }");
    std::os::unix::fs::symlink(outside.0.join("secret.lang"), files.0.join("escape.lang")).unwrap();
    let mut server = ready(Some(&files.0), json!({}));
    let messages = open(
        &mut server,
        &uri::file(&files.0.join("escape.lang")).unwrap(),
        "",
    );
    assert_eq!(messages[0]["method"], "window/logMessage");
    assert_eq!(server.stats().unwrap().disk_reads, 0);
}

#[test]
fn framing_counts_utf8_bytes_and_rejects_truncation_and_ambiguous_headers() {
    let value = json!({"message":"😀日本語"});
    let bytes = frames(std::slice::from_ref(&value));
    assert_eq!(decode(&bytes), vec![value]);
    for end in 1..bytes.len() {
        assert!(
            wire::read(&mut Cursor::new(&bytes[..end]), wire::Limits::default()).is_err(),
            "{end}"
        );
    }
    for invalid in [
        "Content-Length: 0\n\n",
        "Content-Length: -1\r\n\r\n",
        "Content-Length: 1\r\ncontent-length: 1\r\n\r\nx",
        "X: y\r\n\r\n",
        "Content-Length: 99999999999999999999999999\r\n\r\n",
    ] {
        assert!(
            wire::read(&mut Cursor::new(invalid), wire::Limits::default()).is_err(),
            "{invalid:?}"
        );
    }
    let limits = wire::Limits {
        header_bytes: 32,
        message_bytes: 4,
    };
    assert!(matches!(
        wire::read(&mut Cursor::new(b"Content-Length: 5\r\n\r\n"), limits),
        Err(wire::TransportError::MessageLimit)
    ));
    assert!(matches!(
        wire::read(&mut Cursor::new(vec![b'x'; 100]), limits),
        Err(wire::TransportError::HeaderLimit)
    ));
    let mut output = Vec::new();
    assert!(matches!(
        wire::write(&mut output, &json!("oversize"), limits),
        Err(wire::TransportError::MessageLimit)
    ));
    assert!(output.is_empty());
}

#[test]
fn malformed_json_and_unsupported_charset_recover_at_the_next_frame() {
    let mut input = b"Content-Length: 1\r\n\r\n{".to_vec();
    input.extend_from_slice(
        b"Content-Length: 2\r\nContent-Type: application/vscode-jsonrpc; charset=latin1\r\n\r\n{}",
    );
    input.extend(frames(&[
        initialize(None, json!({})),
        request(2, "shutdown", json!(null)),
        note("exit", json!(null)),
    ]));
    let mut output = Vec::new();
    assert_eq!(
        lsp::serve(&mut Cursor::new(input), &mut output, Limits::default()).unwrap(),
        0
    );
    let messages = decode(&output);
    assert_eq!(code(&messages), -32700);
    assert_eq!(messages[1]["error"]["code"], -32700);
    assert_eq!(
        messages[2]["result"]["capabilities"]["positionEncoding"],
        "utf-16"
    );
    let mut seed = 42u64;
    for _ in 0..1000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let bytes: Vec<u8> = (0..(seed as usize % 128))
            .map(|n| (seed.rotate_left(n as u32) & 255) as u8)
            .collect();
        let _ = wire::read(
            &mut Cursor::new(bytes),
            wire::Limits {
                header_bytes: 64,
                message_bytes: 64,
            },
        );
    }
}

#[test]
fn stdio_process_emits_only_framed_protocol_and_uses_lifecycle_exit_status() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_lang"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&frames(&[
            initialize(None, json!({})),
            note("initialized", json!({})),
            request(2, "shutdown", json!(null)),
            note("exit", json!(null)),
        ]))
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
    assert_eq!(decode(&output.stdout).len(), 2);
    let mut child = Command::new(env!("CARGO_BIN_EXE_lang"))
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&frames(&[note("exit", json!(null))]))
        .unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(1));
}

#[test]
fn input_failures_and_bounded_publications_recover_after_valid_edits() {
    let files = Files::new();
    let name = files.file("bom.lang", "");
    let mut server = ready(None, json!({}));
    let messages = open(&mut server, &name, "\u{feff}fn main() -> i64 { return 1; }");
    assert!(
        diagnostics(&messages, &name)
            .iter()
            .any(|d| d["code"] == "SRC0010")
    );
    assert!(
        diagnostics(
            &change(&mut server, &name, 2, "fn main() -> i64 { return 1; }"),
            &name
        )
        .is_empty()
    );
    let mut limits = Limits::default();
    limits.wire.message_bytes = 512;
    let text = (0..24)
        .map(|n| format!("fn f{n}() -> i64 {{ return absent; }}\n"))
        .collect::<String>();
    let messages = open(
        &mut server,
        &uri::file(&files.0.join("many.lang")).unwrap(),
        &text,
    );
    let mut output = Vec::new();
    let publication = messages
        .iter()
        .find(|m| !m["params"]["diagnostics"].as_array().unwrap().is_empty())
        .unwrap();
    assert!(matches!(
        wire::write(&mut output, publication, limits.wire),
        Err(wire::TransportError::MessageLimit)
    ));
    assert!(output.is_empty());
    let nested = format!("{}0{}", "[".repeat(200), "]".repeat(200));
    let mut input = format!("Content-Length: {}\r\n\r\n{nested}", nested.len()).into_bytes();
    input.extend(frames(&[note("exit", json!(null))]));
    assert_eq!(
        lsp::serve(&mut Cursor::new(input), &mut output, limits).unwrap(),
        1
    );
    assert_eq!(code(&decode(&output)), -32700);
}

#[test]
fn oversized_diagnostics_report_a_bounded_log_and_later_publications_still_work() {
    let files = Files::new();
    let name = files.file("many.lang", "");
    let text = (0..8)
        .map(|n| format!("fn f{n}() -> i64 {{ return absent; }}\n"))
        .collect::<String>();
    let messages = [
        initialize(None, json!({})),
        note("initialized", json!({})),
        note(
            "textDocument/didOpen",
            json!({"textDocument":{"uri":name,"languageId":"lang","version":1,"text":text}}),
        ),
        note(
            "textDocument/didChange",
            json!({"textDocument":{"uri":name,"version":2},"contentChanges":[{"text":""}]}),
        ),
        request(2, "shutdown", json!(null)),
        note("exit", json!(null)),
    ];
    let mut limits = Limits::default();
    limits.wire.message_bytes = 1024;
    for message in &messages {
        assert!(serde_json::to_vec(message).unwrap().len() <= limits.wire.message_bytes);
    }
    let mut output = Vec::new();
    assert_eq!(
        lsp::serve(&mut Cursor::new(frames(&messages)), &mut output, limits).unwrap(),
        0
    );
    let messages = decode(&output);
    assert!(messages.iter().any(|m| {
        m["method"] == "window/logMessage"
            && m["params"]["message"]
                .as_str()
                .unwrap()
                .contains("byte limit")
    }));
    assert!(diagnostics(&messages, &name).is_empty());
}
