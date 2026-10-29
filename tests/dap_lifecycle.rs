use lang::dap::{Server, mixed};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

struct Package(std::path::PathBuf);
impl Package {
    fn new(source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang dap lifecycle {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let path = std::fs::canonicalize(path).unwrap();
        std::fs::write(path.join("Lang.json"), r#"{"schema":1,"package":{"name":"debug","version":"0.1.0","edition":"2026","entry":"main.lang"}}"#).unwrap();
        std::fs::write(path.join("main.lang"), source).unwrap();
        Self(path)
    }
    fn launch(&self, mode: &str) -> Value {
        json!({"mode":mode,"root":self.0,"manifestPath":"Lang.json","stopOnEntry":true})
    }
}
impl Drop for Package {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn request(server: &mut Server, command: &str, args: Value, success: bool) -> Vec<Value> {
    let output =
        server.handle(json!({"seq":1,"type":"request","command":command,"arguments":args}));
    assert_eq!(output[0]["success"], success, "{output:?}");
    assert_eq!(output[0]["request_seq"], 1);
    output
}
fn body(server: &mut Server, command: &str, args: Value) -> Value {
    request(server, command, args, true).remove(0)["body"].take()
}
fn events(server: &mut Server) -> Vec<Value> {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let output = server.poll();
        if !output.is_empty() {
            return output;
        }
        assert!(std::time::Instant::now() < end, "DAP event timeout");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn restart_recaptures_source_and_rejects_every_old_target_identity() {
    let source = "fn main()->i64{let mut value:i64=41;return value+1;}";
    let package = Package::new(source);
    let mut server = Server::default();
    let caps = body(&mut server, "initialize", json!({}));
    assert_eq!(caps["supportsRestartRequest"], true);
    assert_eq!(caps["supportsTerminateRequest"], true);
    assert_eq!(caps["supportsCancelRequest"], true);
    body(&mut server, "launch", package.launch("vm"));
    body(&mut server, "configurationDone", json!({}));
    let thread = events(&mut server)[0]["body"]["threadId"].clone();
    let frame =
        body(&mut server, "stackTrace", json!({"threadId":thread}))["stackFrames"][0].clone();
    let locals = body(&mut server, "scopes", json!({"frameId":frame["id"]}))["scopes"][0]["variablesReference"].clone();
    body(
        &mut server,
        "cancel",
        json!({"requestId":1,"progressId":"already-finished"}),
    );
    assert_eq!(
        body(&mut server, "stackTrace", json!({"threadId":thread}))["stackFrames"][0],
        frame
    );
    let replacement = "fn main()->i64{return 43;}";
    std::fs::write(package.0.join("main.lang"), replacement).unwrap();
    let restarted = request(&mut server, "restart", json!({}), true);
    assert_eq!(restarted.last().unwrap()["event"], "initialized");
    for (command, args) in [
        ("stackTrace", json!({"threadId":thread})),
        ("scopes", json!({"frameId":frame["id"]})),
        ("variables", json!({"variablesReference":locals})),
        (
            "source",
            json!({"sourceReference":frame["source"]["sourceReference"]}),
        ),
    ] {
        request(&mut server, command, args, false);
    }
    body(&mut server, "configurationDone", json!({}));
    let fresh_thread = events(&mut server)[0]["body"]["threadId"].clone();
    assert_ne!(fresh_thread, thread);
    let fresh =
        body(&mut server, "stackTrace", json!({"threadId":fresh_thread}))["stackFrames"][0].clone();
    assert_ne!(fresh["id"], frame["id"]);
    let content = body(
        &mut server,
        "source",
        json!({"sourceReference":fresh["source"]["sourceReference"]}),
    );
    assert_eq!(content["content"], replacement);
    body(&mut server, "continue", json!({"threadId":fresh_thread}));
    let exit = events(&mut server);
    assert_eq!(exit[0]["body"]["output"], "43\n");
    assert_eq!(
        exit.iter().filter(|v| v["event"] == "terminated").count(),
        1
    );
    assert!(server.poll().is_empty());
    let disconnect = request(&mut server, "disconnect", json!({}), true);
    assert_eq!(disconnect.len(), 1, "termination must not be repeated");
    assert!(server.closed());
}

#[test]
fn failed_restart_releases_old_execution_and_does_not_reuse_its_ids() {
    let package = Package::new("fn main()->i64{return 42;}");
    let mut server = Server::default();
    body(&mut server, "initialize", json!({}));
    body(&mut server, "launch", package.launch("vm"));
    body(&mut server, "configurationDone", json!({}));
    let old = events(&mut server)[0]["body"]["threadId"].clone();
    let mut bad = package.launch("vm");
    bad["manifestPath"] = json!("../Lang.json");
    let failed = request(&mut server, "restart", json!({"arguments":bad}), false);
    assert_eq!(failed[1]["event"], "terminated");
    request(&mut server, "stackTrace", json!({"threadId":old}), false);
    assert!(server.poll().is_empty());
    request(
        &mut server,
        "restart",
        json!({"arguments":package.launch("vm")}),
        true,
    );
    body(&mut server, "configurationDone", json!({}));
    let fresh = events(&mut server)[0]["body"]["threadId"].clone();
    assert!(fresh.as_u64().unwrap() > 2 * 65_536);
    let stop = request(&mut server, "terminate", json!({}), true);
    assert_eq!(stop[1]["event"], "terminated");
    assert!(!server.closed());
    assert!(server.poll().is_empty());
    assert_eq!(request(&mut server, "disconnect", json!({}), true).len(), 1);
}

#[test]
fn restart_and_terminate_join_each_original_cooperative_native_callback() {
    static ENTERED: AtomicU64 = AtomicU64::new(0);
    static RETURNED: AtomicU64 = AtomicU64::new(0);
    static DROPPED: AtomicU64 = AtomicU64::new(0);
    struct Lease;
    impl Drop for Lease {
        fn drop(&mut self) {
            DROPPED.fetch_add(1, Ordering::SeqCst);
        }
    }
    extern "C" fn callback(value: i64) -> i64 {
        let _lease = Lease;
        ENTERED.fetch_add(1, Ordering::SeqCst);
        if mixed::pause().is_err() {
            return 0;
        }
        RETURNED.fetch_add(1, Ordering::SeqCst);
        value
    }
    let package = Package::new(
        "extern fn native_bridge(n:i64)->i64; fn main()->i64{return native_bridge(42);}",
    );
    let mut hosts = mixed::NativeHosts::new();
    hosts.register("native_bridge", callback).unwrap();
    let mut server = Server::with_native_hosts(hosts);
    body(&mut server, "initialize", json!({}));
    let mut args = package.launch("mixed");
    args["stopOnEntry"] = json!(false);
    body(&mut server, "launch", args);
    for ordinal in 1..=2 {
        body(&mut server, "configurationDone", json!({}));
        let stopped = events(&mut server);
        assert_eq!(stopped[0]["body"]["reason"], "pause");
        assert_eq!(ENTERED.load(Ordering::SeqCst), ordinal);
        body(&mut server, "cancel", json!({"requestId":1}));
        assert_eq!(DROPPED.load(Ordering::SeqCst), ordinal - 1);
        if ordinal == 1 {
            body(&mut server, "restart", json!({}));
        } else {
            body(&mut server, "terminate", json!({}));
        }
        assert_eq!(DROPPED.load(Ordering::SeqCst), ordinal);
        assert_eq!(RETURNED.load(Ordering::SeqCst), 0);
    }
    assert_eq!(mixed::pause(), Err(mixed::Error::OutsideNativeCall));
    assert!(server.poll().is_empty());
    assert_eq!(request(&mut server, "disconnect", json!({}), true).len(), 1);
}

#[test]
fn malformed_lifecycle_requests_do_not_close_or_start_an_execution() {
    let mut server = Server::default();
    request(
        &mut server,
        "initialize",
        json!({"pathFormat":false}),
        false,
    );
    body(&mut server, "initialize", json!({}));
    request(&mut server, "restart", json!({}), false);
    for args in [
        json!({"requestId":0}),
        json!({"requestId":-1}),
        json!({"requestId":2147483648_u64}),
        json!({"progressId":[]}),
    ] {
        request(&mut server, "cancel", args, false);
    }
    let malformed =
        server.handle(json!({"seq":2,"type":"request","command":"terminate","arguments":[]}));
    assert_eq!(malformed.len(), 1);
    assert_eq!(malformed[0]["success"], false);
    assert!(!server.closed());
    assert!(server.poll().is_empty());
    body(&mut server, "disconnect", json!({}));
}
