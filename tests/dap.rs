use lang::frontend::{self, CompileLimits};
use lang_bytecode::unit::Program;
use lang_diagnostics::SourceMap;
use lang_mir::HostFunctionId;
use lang_runtime::{HostFunctionRegistry, execution::InterruptionFlag};
use lang_vm::unit::{
    self as vm, Control, DebugError, DebugEvent, DebugSession, HostBindings, Limits, Reason,
};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

struct Package(std::path::PathBuf);
impl Package {
    fn new(source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang dap {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        std::fs::write(path.join("Lang.json"),r#"{"schema":1,"package":{"name":"debug","version":"0.1.0","edition":"2026","entry":"main.lang"}}"#).unwrap();
        std::fs::write(path.join("main.lang"), source).unwrap();
        Self(path)
    }
}
impl Drop for Package {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn dap(
    server: &mut lang::dap::Server,
    command: &str,
    args: serde_json::Value,
) -> serde_json::Value {
    let reply = server
        .handle(serde_json::json!({"seq":1,"type":"request","command":command,"arguments":args}));
    assert_eq!(reply[0]["request_seq"], 1);
    assert_eq!(reply[0]["command"], command);
    assert_eq!(reply[0]["success"], true, "{reply:?}");
    reply[0]["body"].clone()
}
fn events(server: &mut lang::dap::Server) -> Vec<serde_json::Value> {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let events = server.poll();
        if !events.is_empty() {
            return events;
        }
        assert!(std::time::Instant::now() < end, "debug event timed out");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
#[test]
fn dap_vm_breakpoints_retain_nested_stack_and_captured_source() {
    use serde_json::json;
    let source = "/*😀*/fn child(n:i64)->i64 {\r\n  return n+1;\r\n}\r\nfn main()->i64 {\r\n  return child(41);\r\n}\r\n";
    let package = Package::new(source);
    let mut server = lang::dap::Server::default();
    let caps = dap(
        &mut server,
        "initialize",
        json!({"pathFormat":"uri","linesStartAt1":false,"columnsStartAt1":false}),
    );
    assert_eq!(caps["supportsConfigurationDoneRequest"], true);
    let reply = server.handle(
        json!({"seq":2,"type":"request","command":"launch","arguments":{
        "root":package.0,"manifestPath":"Lang.json","mode":"vm"}}),
    );
    assert_eq!(reply[0]["success"], true, "{reply:?}");
    assert_eq!(reply[1]["event"], "initialized");
    let path = lang::lsp::uri::file(&package.0.join("main.lang")).unwrap();
    let local_uri = path.replacen("file://", "file://localhost", 1);
    let bp = dap(
        &mut server,
        "setBreakpoints",
        json!({"source":{"path":local_uri},"breakpoints":[{"line":1}]}),
    );
    assert_eq!(bp["breakpoints"][0]["verified"], true, "{bp:?}");
    assert_eq!(bp["breakpoints"][0]["line"], 1);
    let source_ref = bp["breakpoints"][0]["source"]["sourceReference"].clone();
    for args in [
        json!({"source":{"path":path},"breakpoints":vec![json!({"line":1});257]}),
        json!({"source":{"path":path},"breakpoints":[{"line":1,"condition":"effect()"}]}),
        json!({"source":{"path":lang::lsp::uri::file(&package.0.join("unadmitted.lang")).unwrap()},"breakpoints":[]}),
    ] {
        let failed = server
            .handle(json!({"seq":3,"type":"request","command":"setBreakpoints","arguments":args}));
        assert_eq!(failed[0]["success"], false, "{failed:?}");
    }
    // Invalid replacements must preserve the original executable breakpoint.
    dap(&mut server, "configurationDone", json!({}));
    assert_eq!(events(&mut server)[0]["event"], "stopped");
    let limited = server.handle(json!({"seq":4,"type":"request","command":"stackTrace","arguments":{"threadId":1,"levels":257}}));
    assert_eq!(limited[0]["success"], false);
    let stack = dap(&mut server, "stackTrace", json!({"threadId":1}));
    let frames = stack["stackFrames"].as_array().unwrap();
    assert_eq!(frames.len(), 2, "{stack:?}");
    assert!(frames[0]["name"].as_str().unwrap().contains("child"));
    assert_eq!(frames[0]["line"], 1);
    assert_eq!(frames[1]["line"], 4);
    assert_eq!(frames[1]["column"], 9);
    assert_eq!(frames[1]["source"]["path"], path);
    std::fs::write(package.0.join("main.lang"), "changed on disk").unwrap();
    assert_eq!(
        dap(&mut server, "source", json!({"sourceReference":source_ref}))["content"],
        source
    );
    // Remove the breakpoint using its captured identity, without disk reads.
    dap(
        &mut server,
        "setBreakpoints",
        json!({"source":{"path":path},"breakpoints":[]}),
    );
    dap(&mut server, "continue", json!({"threadId":1}));
    let exit = events(&mut server);
    assert_eq!(exit[0]["body"]["output"], "42\n", "{exit:?}");
    assert_eq!(exit[1]["event"], "exited");
    assert_eq!(exit[2]["event"], "terminated");
    assert!(
        dap(&mut server, "threads", json!({}))["threads"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let stale = server.handle(json!({"seq":3,"type":"request","command":"source","arguments":{"sourceReference":source_ref}}));
    assert_eq!(stale[0]["success"], false);
    dap(&mut server, "disconnect", json!({}));
}

fn compile(source: &str) -> Program {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("debug.lang"),
        source.as_bytes(),
        None,
        CompileLimits::default(),
        None,
    );
    let typed = frontend::copy_executable(analysis.result.as_ref().unwrap()).unwrap();
    frontend::compile_script(typed, &analysis.sources, CompileLimits::default()).unwrap()
}

struct DapClient {
    child: std::process::Child,
    input: Option<std::process::ChildStdin>,
    messages: std::sync::mpsc::Receiver<serde_json::Value>,
    reader: Option<std::thread::JoinHandle<()>>,
    sequence: u32,
    last_sequence: u64,
}
impl DapClient {
    fn new() -> Self {
        use std::{
            io::{BufRead, Read},
            process::{Command, Stdio},
        };
        let mut child = Command::new(env!("CARGO_BIN_EXE_lang"))
            .arg("dap")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (send, messages) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut input = std::io::BufReader::new(stdout);
            for _ in 0..128 {
                let mut length = None;
                let mut used = 0usize;
                loop {
                    let mut line = Vec::new();
                    let read = input
                        .by_ref()
                        .take(8193)
                        .read_until(b'\n', &mut line)
                        .unwrap();
                    if read == 0 {
                        return;
                    }
                    used += read;
                    assert!(used <= 8192 && line.ends_with(b"\r\n") && line.is_ascii());
                    if line == b"\r\n" {
                        break;
                    }
                    let header = std::str::from_utf8(&line).unwrap();
                    let (name, value) = header.trim_end().split_once(':').unwrap();
                    assert!(name.eq_ignore_ascii_case("Content-Length"));
                    assert!(length.is_none());
                    let value = value.trim();
                    assert!(!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()));
                    length = Some(value.parse::<usize>().unwrap());
                }
                let length = length.unwrap();
                assert!(length <= 4 * 1024 * 1024);
                let mut bytes = vec![0; length];
                input.read_exact(&mut bytes).unwrap();
                if send.send(serde_json::from_slice(&bytes).unwrap()).is_err() {
                    return;
                }
            }
            panic!("excessive DAP messages");
        });
        Self {
            child,
            input,
            messages,
            reader: Some(reader),
            sequence: 0,
            last_sequence: 0,
        }
    }
    fn receive(&mut self) -> serde_json::Value {
        let message = self
            .messages
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        let seq = message["seq"].as_u64().unwrap();
        assert!(seq > self.last_sequence);
        self.last_sequence = seq;
        assert!(message.get("jsonrpc").is_none());
        message
    }
    fn request(&mut self, command: &str, args: serde_json::Value) -> serde_json::Value {
        use std::io::Write;
        self.sequence += 1;
        let bytes = serde_json::to_vec(&serde_json::json!({"seq":self.sequence,"type":"request",
            "command":command,"arguments":args}))
        .unwrap();
        let input = self.input.as_mut().unwrap();
        write!(input, "Content-Length: {}\r\n\r\n", bytes.len()).unwrap();
        input.write_all(&bytes).unwrap();
        input.flush().unwrap();
        let message = self.receive();
        assert_eq!(message["type"], "response", "{message:?}");
        assert_eq!(message["request_seq"], self.sequence);
        assert_eq!(message["command"], command);
        assert_eq!(message["success"], true, "{message:?}");
        message["body"].clone()
    }
    fn finish(mut self) {
        self.request("disconnect", serde_json::json!({}));
        assert_eq!(self.receive()["event"], "terminated");
        self.input.take();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        self.reader.take().unwrap().join().unwrap();
        use std::io::Read;
        let mut errors = String::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut errors)
            .unwrap();
        assert!(errors.is_empty(), "{errors}");
    }
}
impl Drop for DapClient {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
#[test]
fn independent_stdio_dap_client_configures_before_execution_and_maps_utf16() {
    use serde_json::json;
    let source = "/*😀*/fn main()->i64{return 42;}";
    let package = Package::new(source);
    let mut client = DapClient::new();
    client.request("initialize", json!({"adapterID":"lang"}));
    client.request(
        "launch",
        json!({"root":package.0,"manifestPath":"Lang.json","mode":"vm","stopOnEntry":true}),
    );
    assert_eq!(client.receive()["event"], "initialized");
    client.request("configurationDone", json!({}));
    assert_eq!(client.receive()["body"]["reason"], "entry");
    let frames = client.request("stackTrace", json!({"threadId":1}));
    let frame = &frames["stackFrames"][0];
    assert_eq!(frame["line"], 1);
    assert_eq!(
        frame["column"],
        source[..source.find("42").unwrap()].encode_utf16().count() + 1
    );
    let reference = frame["source"]["sourceReference"].clone();
    assert_eq!(
        client.request("source", json!({"sourceReference":reference}))["content"],
        source
    );
    client.request("continue", json!({"threadId":1}));
    assert_eq!(client.receive()["body"]["output"], "42\n");
    assert_eq!(client.receive()["event"], "exited");
    assert_eq!(client.receive()["event"], "terminated");
    client.finish();
}
#[test]
fn dap_refuses_invalid_envelopes_escaping_manifests_and_ungranted_imports() {
    use serde_json::json;
    let package = Package::new("extern fn effect(n:i64)->i64;fn main()->i64{return effect(1);}");
    let mut server = lang::dap::Server::default();
    assert_eq!(
        server.handle(json!({"jsonrpc":"2.0","seq":1,"type":"request","command":"initialize"}))[0]
            ["success"],
        false
    );
    dap(&mut server, "initialize", json!({}));
    for args in [
        json!({"root":package.0,"manifestPath":"../Lang.json","mode":"vm"}),
        json!({"root":package.0,"manifestPath":"Lang.json","mode":"native"}),
        json!({"root":package.0,"manifestPath":"Lang.json","mode":"vm"}),
    ] {
        let reply =
            server.handle(json!({"seq":2,"type":"request","command":"launch","arguments":args}));
        assert_eq!(reply.len(), 1);
        assert_eq!(reply[0]["success"], false, "{reply:?}");
        assert!(
            !reply[0]["message"]
                .as_str()
                .unwrap()
                .contains(package.0.to_str().unwrap())
        );
    }
    dap(&mut server, "disconnect", json!({}));
}

#[test]
fn vm_debug_session_retains_cells_stack_controls_and_exact_host_effects() {
    static CALLS: AtomicU64 = AtomicU64::new(0);
    extern "C" fn tick(value: i64) -> i64 {
        CALLS.fetch_add(1, Ordering::SeqCst);
        value + 1
    }
    let source = "extern fn tick(value:i64)->i64;fn child(n:i64)->i64{return tick(n)+1;}fn main()->i64{return child(40);}";
    let program = compile(source);
    let mut hosts = HostFunctionRegistry::new();
    let mut bindings = HostBindings::new();
    let id = HostFunctionId::new(7);
    hosts.register_i64(id, tick).unwrap();
    bindings.bind("tick", id).unwrap();
    CALLS.store(0, Ordering::SeqCst);
    let expected = vm::execute_entry(&program, &hosts, &bindings, Control::default()).unwrap();
    assert_eq!(expected.value.as_i64(), Some(42));
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);
    CALLS.store(0, Ordering::SeqCst);
    let mut session = DebugSession::entry(&program, &hosts, &bindings, Control::default()).unwrap();
    assert!(matches!(
        session.resume_until(|_| true).unwrap(),
        DebugEvent::Paused(_)
    ));
    assert_eq!(session.usage().fuel, 0);
    assert_eq!(CALLS.load(Ordering::SeqCst), 0);
    let mut peak = 0;
    let mut completed = false;
    for _ in 0..1024 {
        let fuel = session.usage().fuel;
        let mut skip = true;
        match session
            .resume_until(|_| !std::mem::take(&mut skip))
            .unwrap()
        {
            DebugEvent::Paused(point) => {
                assert_eq!(session.usage().fuel, fuel + 1);
                let frames = session.stack().collect::<Vec<_>>();
                assert_eq!(frames.len(), point.depth);
                assert_eq!(frames[0].function, point.function);
                assert_eq!(frames[0].span, point.span);
                peak = peak.max(frames.len());
                if frames.len() == 2 {
                    let caller = frames[1].span.unwrap();
                    assert_eq!(
                        &source[caller.start as usize..caller.end as usize],
                        "child(40)"
                    );
                }
            }
            DebugEvent::Exited(actual) => {
                assert_eq!(actual.value, expected.value);
                assert_eq!(actual.usage, expected.usage);
                assert_eq!(CALLS.load(Ordering::SeqCst), 1);
                completed = true;
                break;
            }
        }
    }
    assert!(completed);
    assert_eq!(peak, 2);
    assert_eq!(session.stack().count(), 0);
    assert!(matches!(
        session.resume_until(|_| false),
        Err(DebugError::Finished)
    ));
}

#[test]
fn vm_debug_session_preserves_fuel_failure_and_interruption_precedence() {
    let program = compile("fn leaf(n:i64)->i64{return n+1;}fn main()->i64{return leaf(41);}");
    let hosts = HostFunctionRegistry::new();
    let bindings = HostBindings::new();
    let control = Control::with_limits(Limits {
        fuel: Some(2),
        ..Default::default()
    });
    let expected = vm::execute_entry(&program, &hosts, &bindings, control).unwrap_err();
    let mut session = DebugSession::entry(&program, &hosts, &bindings, control).unwrap();
    assert!(matches!(
        session.resume_until(|_| true).unwrap(),
        DebugEvent::Paused(_)
    ));
    let Err(DebugError::Execution(actual)) = session.resume_until(|_| false) else {
        panic!("fuel failure");
    };
    assert!(matches!(actual.reason, Reason::FuelExhausted));
    assert_eq!(actual.usage, expected.usage);
    assert_eq!(actual.fault, expected.fault);
    assert_eq!(actual.stack.len(), expected.stack.len());
    for (a, b) in actual.stack.iter().zip(&expected.stack) {
        assert_eq!(a.function, b.function);
        assert_eq!(a.span, b.span);
    }
    assert!(matches!(
        session.resume_until(|_| false),
        Err(DebugError::Finished)
    ));
    let flag = InterruptionFlag::new();
    let mut session = DebugSession::entry(
        &program,
        &hosts,
        &bindings,
        Control::default().and_interruption(&flag),
    )
    .unwrap();
    assert!(matches!(
        session.resume_until(|_| true).unwrap(),
        DebugEvent::Paused(_)
    ));
    flag.interrupt();
    let Err(DebugError::Execution(failure)) = session.resume_until(|_| true) else {
        panic!("interruption precedes pause");
    };
    assert!(matches!(failure.reason, Reason::Interrupted));
    assert_eq!(failure.usage.fuel, 0);
}
