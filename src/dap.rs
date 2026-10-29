//! Bounded, source-snapshot DAP host for retained VM execution (RFC 0040).
mod inspect;
pub mod mixed;
mod native;
mod target;

use crate::lsp::{uri, wire};
use serde_json::{Value, json};
use std::{io, sync::mpsc, time::Duration};
use target::{Event, StepKind, Target};

const WIRE: wire::Limits = wire::Limits {
    header_bytes: 8192,
    message_bytes: 4 * 1024 * 1024,
};

#[derive(Clone, Copy)]
struct Coordinates {
    lines: u32,
    columns: u32,
    uri: bool,
}
impl Default for Coordinates {
    fn default() -> Self {
        Self {
            lines: 1,
            columns: 1,
            uri: false,
        }
    }
}

/// A protocol endpoint. Execution progresses on its owning worker; call `poll`
/// while waiting for requests to deliver asynchronous stop/exit events.
#[derive(Default)]
pub struct Server {
    sequence: u32,
    initialized: bool,
    closed: bool,
    coordinates: Coordinates,
    target: Option<Target>,
    native: Option<native::Target>,
    native_hosts: mixed::NativeHosts,
}
impl Server {
    /// Configure trusted native imports in the embedding host, never in source
    /// or protocol input. Callback code must live for the server lifetime.
    pub fn with_native_hosts(native_hosts: mixed::NativeHosts) -> Self {
        Self {
            native_hosts,
            ..Self::default()
        }
    }
    fn number(&mut self) -> u32 {
        self.sequence += 1;
        self.sequence
    }
    fn event(&mut self, name: &str, body: Value) -> Value {
        json!({"seq":self.number(),"type":"event","event":name,"body":body})
    }
    pub fn closed(&self) -> bool {
        self.closed
    }
    pub fn handle(&mut self, request: Value) -> Vec<Value> {
        // DAP sequences and commands have finite representations. Reject a
        // foreign JSON-RPC envelope rather than interpreting it as a request.
        let seq = request
            .get("seq")
            .and_then(Value::as_u64)
            .filter(|n| *n > 0 && *n <= i32::MAX as u64);
        let command = request
            .get("command")
            .and_then(Value::as_str)
            .filter(|s| s.len() <= 128)
            .unwrap_or("");
        let valid = seq.is_some()
            && request["type"] == "request"
            && !command.is_empty()
            && request.get("jsonrpc").is_none();
        let args = &request["arguments"];
        let result = if !valid {
            Err("invalid DAP request".into())
        } else if self.closed {
            Err("adapter is disconnected".into())
        } else {
            self.dispatch(command, args)
        };
        let result = result.and_then(|body| {
            // Leave space for the response envelope and JSON escaping. This
            // also makes oversized captured-source requests normal refusals.
            let size = serde_json::to_vec(&body)
                .map_err(|_| "cannot encode DAP response".to_owned())?
                .len();
            if size > WIRE.message_bytes.saturating_sub(1024) {
                Err("DAP response limit exceeded".into())
            } else {
                Ok(body)
            }
        });
        let success = result.is_ok();
        let mut response = json!({"seq":self.number(),"type":"response",
            "request_seq":seq.unwrap_or(0),"command":command,"success":success});
        match result {
            Ok(body) => response["body"] = body,
            Err(message) => response["message"] = Value::String(message),
        }
        let mut output = vec![response];
        if success && command == "launch" {
            if self.native.is_some() || args["mode"] == "mixed" {
                output.push(self.event(
                    "capabilities",
                    json!({"capabilities":{"supportsEvaluateForHovers":false}}),
                ));
            }
            output.push(self.event("initialized", json!({})));
        }
        if success && command == "disconnect" {
            output.push(self.event("terminated", json!({})));
        }
        output
    }
    fn dispatch(&mut self, command: &str, args: &Value) -> Result<Value, String> {
        if command == "initialize" {
            if self.initialized {
                return Err("already initialized".into());
            }
            let boolean = |key: &str| -> Result<bool, String> {
                match args.get(key) {
                    None => Ok(true),
                    Some(v) => v.as_bool().ok_or_else(|| format!("invalid {key}")),
                }
            };
            let uri = match args
                .get("pathFormat")
                .and_then(Value::as_str)
                .unwrap_or("path")
            {
                "path" => false,
                "uri" => true,
                _ => return Err("unsupported pathFormat".into()),
            };
            self.coordinates = Coordinates {
                lines: u32::from(boolean("linesStartAt1")?),
                columns: u32::from(boolean("columnsStartAt1")?),
                uri,
            };
            self.initialized = true;
            return Ok(
                json!({"supportsConfigurationDoneRequest":true,"supportsEvaluateForHovers":true}),
            );
        }
        if !self.initialized {
            return Err("initialize is required".into());
        }
        if command == "disconnect" {
            self.target = None; // interrupts and joins the original worker
            self.closed = true;
            if let Some(mut native) = self.native.take() {
                native.disconnect()?;
            }
            return Ok(json!({}));
        }
        if command == "launch" {
            if self.target.is_some() || self.native.is_some() {
                return Err("target already launched".into());
            }
            match args["mode"].as_str() {
                Some("native") => self.native = Some(native::Target::launch(args)?),
                Some("vm") => self.target = Some(Target::launch(args, self.coordinates)?),
                Some("mixed") => {
                    self.target = Some(Target::launch_mixed(
                        args,
                        self.coordinates,
                        self.native_hosts.clone(),
                    )?)
                }
                _ => return Err("unsupported debug mode".into()),
            }
            return Ok(json!({}));
        }
        if let Some(native) = self.native.as_mut() {
            return native.dispatch(command, args, self.coordinates);
        }
        let target = self.target.as_mut().ok_or("launch is required")?;
        if target.native_paused() && matches!(command, "scopes" | "variables" | "evaluate") {
            return Err("value inspection is unavailable inside a native callback".into());
        }
        match command {
            "setBreakpoints" => target.breakpoints(args, self.coordinates),
            "configurationDone" => {
                target.start()?;
                Ok(json!({}))
            }
            "continue" => {
                target.thread(args)?;
                target.resume()?;
                Ok(json!({"allThreadsContinued":true}))
            }
            "stepIn" | "next" | "stepOut" => {
                target.step(
                    args,
                    match command {
                        "stepIn" => StepKind::Into,
                        "next" => StepKind::Over,
                        _ => StepKind::Out,
                    },
                )?;
                Ok(json!({}))
            }
            "threads" => Ok(json!({"threads":target.threads()})),
            "stackTrace" => target.stack(args, self.coordinates),
            "source" => target.source(args),
            "scopes" => target.scopes(args),
            "variables" => target.variables(args),
            "evaluate" => target.evaluate(args),
            _ => Err("unsupported DAP command".into()),
        }
    }
    pub fn poll(&mut self) -> Vec<Value> {
        if let Some(native) = self.native.as_mut() {
            return match native.poll() {
                Ok(Some(value)) => vec![self.event(
                    value["event"].as_str().expect("native event"),
                    value["body"].clone(),
                )],
                Ok(None) => vec![],
                Err(_) => {
                    self.native.take();
                    vec![self.event("output", json!({"category":"stderr","output":"native debugger session failed\n"})),
                        self.event("exited", json!({"exitCode":1})), self.event("terminated", json!({}))]
                }
            };
        }
        let Some(event) = self.target.as_mut().and_then(Target::poll) else {
            return vec![];
        };
        match event {
            Event::Paused { reason, .. } => vec![self.event("stopped", json!({
                "reason":reason.name(),"threadId":1,"allThreadsStopped":true}))],
            Event::Exited { value, fuel } => vec![
                self.event("output", json!({"category":"console","output":format!("{value}\n"),"data":{"fuel":fuel}})),
                self.event("exited", json!({"exitCode":0})),
                self.event("terminated", json!({})),
            ],
            Event::Failed(message) => vec![
                self.event("output", json!({"category":"stderr","output":format!("{message}\n")})),
                self.event("exited", json!({"exitCode":1})),
                self.event("terminated", json!({})),
            ],
        }
    }
}

/// Stdio uses a bounded input queue so execution events do not depend on the
/// client sending another request. Only the reader thread touches stdin.
pub fn serve_stdio() -> Result<u8, wire::TransportError> {
    serve_stdio_with_native_hosts(mixed::NativeHosts::new())
}

/// Stdio endpoint with explicit native bindings supplied by its trusted Rust
/// embedding host. The ordinary CLI calls this with an empty configuration.
pub fn serve_stdio_with_native_hosts(
    hosts: mixed::NativeHosts,
) -> Result<u8, wire::TransportError> {
    let (send, receive) = mpsc::sync_channel(2);
    std::thread::spawn(move || {
        let mut input = io::stdin().lock();
        loop {
            let message = wire::read(&mut input, WIRE);
            let end = !matches!(message, Ok(Some(_)));
            if send.send(message).is_err() || end {
                break;
            }
        }
    });
    let mut output = io::stdout().lock();
    let mut server = Server::with_native_hosts(hosts);
    loop {
        for event in server.poll() {
            wire::write(&mut output, &event, WIRE)?;
        }
        match receive.recv_timeout(Duration::from_millis(10)) {
            Ok(Ok(Some(bytes))) => {
                let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
                for response in server.handle(value) {
                    wire::write(&mut output, &response, WIRE)?;
                }
                if server.closed() {
                    return Ok(0);
                }
            }
            Ok(Ok(None)) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(1),
            Ok(Err(error)) => return Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}
