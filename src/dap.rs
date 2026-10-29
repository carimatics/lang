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
const REFERENCE_BLOCK: u32 = 65_536;

fn incoming(args: &Value, epoch: u32) -> Result<Value, String> {
    let mut args = args.clone();
    for key in [
        "threadId",
        "frameId",
        "sourceReference",
        "variablesReference",
    ] {
        if let Some(value) = args.get_mut(key) {
            let n = value.as_u64().ok_or("invalid debug reference")?;
            if n != 0 {
                let local = n
                    .checked_sub(u64::from(epoch))
                    .ok_or("stale debug reference")?;
                if local == 0 || local > u64::from(REFERENCE_BLOCK) {
                    return Err("stale debug reference".into());
                }
                *value = json!(local);
            }
        }
    }
    Ok(args)
}

fn outgoing(value: &mut Value, epoch: u32) {
    match value {
        Value::Array(values) => values.iter_mut().for_each(|v| outgoing(v, epoch)),
        Value::Object(values) => {
            for (key, value) in values {
                if matches!(
                    key.as_str(),
                    "id" | "threadId" | "sourceReference" | "variablesReference"
                ) {
                    if let Some(n) = value.as_u64().filter(|n| *n > 0) {
                        *value = json!(n + u64::from(epoch));
                    }
                } else {
                    outgoing(value, epoch);
                }
            }
        }
        _ => {}
    }
}

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
    launch: Option<Value>,
    epoch: u32,
    next_epoch: u32,
    terminated: bool,
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
    fn event(&mut self, name: &str, mut body: Value) -> Value {
        outgoing(&mut body, self.epoch);
        if name == "terminated" {
            self.terminated = true;
        }
        json!({"seq":self.number(),"type":"event","event":name,"body":body})
    }
    fn stop(&mut self) -> Result<(), String> {
        self.target = None; // interrupts and joins the original worker
        match self.native.take() {
            Some(mut native) => native.disconnect(),
            None => Ok(()),
        }
    }
    fn sequence_exhausted(&self) -> bool {
        self.sequence >= i32::MAX as u32 - 4
    }
    pub fn closed(&self) -> bool {
        self.closed
    }
    pub fn handle(&mut self, request: Value) -> Vec<Value> {
        if self.sequence_exhausted() {
            if self.closed {
                return vec![];
            }
            let _ = self.stop();
            self.closed = true;
            let response = json!({"seq":self.number(),"type":"response",
                "request_seq":request["seq"].as_u64().unwrap_or(0),
                "command":request["command"].as_str().filter(|s| s.len() <= 128).unwrap_or(""),
                "success":false,"message":"adapter sequence limit exceeded"});
            let mut output = vec![response];
            if !self.terminated {
                output.push(self.event("terminated", json!({})));
            }
            return output;
        }
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
            && request.get("jsonrpc").is_none()
            && request.get("arguments").is_none_or(Value::is_object);
        let args = &request["arguments"];
        let result = if !valid {
            Err("invalid DAP request".into())
        } else if self.closed {
            Err("adapter is disconnected".into())
        } else {
            self.dispatch(command, args)
        };
        let result = result.and_then(|mut body| {
            outgoing(&mut body, self.epoch);
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
        if success && matches!(command, "launch" | "restart") {
            let hover = self.launch.as_ref().is_some_and(|a| a["mode"] == "vm");
            if command == "restart" || !hover {
                output.push(self.event(
                    "capabilities",
                    json!({"capabilities":{"supportsEvaluateForHovers":hover}}),
                ));
            }
            output.push(self.event("initialized", json!({})));
        }
        if matches!(command, "disconnect" | "terminate")
            && !self.terminated
            && (success || (valid && self.initialized))
        {
            output.push(self.event("terminated", json!({})));
        }
        if command == "restart"
            && !success
            && !self.terminated
            && self.launch.is_some()
            && self.target.is_none()
            && self.native.is_none()
        {
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
            let uri = match args.get("pathFormat") {
                None => false,
                Some(Value::String(value)) if value == "path" => false,
                Some(Value::String(value)) if value == "uri" => true,
                _ => return Err("unsupported pathFormat".into()),
            };
            self.coordinates = Coordinates {
                lines: u32::from(boolean("linesStartAt1")?),
                columns: u32::from(boolean("columnsStartAt1")?),
                uri,
            };
            self.initialized = true;
            return Ok(
                json!({"supportsConfigurationDoneRequest":true,"supportsEvaluateForHovers":true,
                    "supportsRestartRequest":true,"supportsTerminateRequest":true,"supportsCancelRequest":true}),
            );
        }
        if !self.initialized {
            return Err("initialize is required".into());
        }
        if command == "cancel" {
            if args
                .get("requestId")
                .is_some_and(|n| n.as_u64().is_none_or(|n| n == 0 || n > i32::MAX as u64))
                || args.get("progressId").is_some_and(|s| {
                    s.as_str().is_none_or(|s| {
                        s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control)
                    })
                })
            {
                return Err("invalid cancellation identity".into());
            }
            // Requests complete synchronously and no progress sequence exists.
            // Cancelling a completed/unknown request never stops the debuggee.
            return Ok(json!({}));
        }
        if matches!(command, "disconnect" | "terminate") {
            self.closed = command == "disconnect";
            self.stop()?;
            return Ok(json!({}));
        }
        if matches!(command, "launch" | "restart") {
            if command == "launch" && (self.target.is_some() || self.native.is_some()) {
                return Err("target already launched".into());
            }
            let args = if command == "restart" {
                let previous = self.launch.as_ref().ok_or("no previous launch")?;
                let replacement = args.get("arguments").unwrap_or(previous);
                if !replacement.is_object() {
                    return Err("invalid restart launch arguments".into());
                }
                replacement.clone()
            } else {
                args.clone()
            };
            if self.next_epoch > i32::MAX as u32 - REFERENCE_BLOCK {
                return Err("debug target identity limit exceeded".into());
            }
            self.stop()?;
            self.epoch = self.next_epoch;
            self.next_epoch += REFERENCE_BLOCK;
            self.terminated = false;
            self.launch = Some(args.clone());
            match args["mode"].as_str() {
                Some("native") => self.native = Some(native::Target::launch(&args)?),
                Some("vm") => self.target = Some(Target::launch(&args, self.coordinates)?),
                Some("mixed") => {
                    self.target = Some(Target::launch_mixed(
                        &args,
                        self.coordinates,
                        self.native_hosts.clone(),
                    )?)
                }
                _ => return Err("unsupported debug mode".into()),
            }
            return Ok(json!({}));
        }
        let args = incoming(args, self.epoch)?;
        let args = &args;
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
        if self.sequence_exhausted() && !self.closed {
            let _ = self.stop();
            self.closed = true;
            return if self.terminated {
                vec![]
            } else {
                vec![self.event("terminated", json!({}))]
            };
        }
        if self.closed || self.terminated {
            return vec![];
        }
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

#[cfg(test)]
mod lifecycle_tests {
    use super::*;

    #[test]
    fn exhausted_sequence_refuses_and_closes_without_wrapping() {
        let mut server = Server {
            sequence: i32::MAX as u32 - 4,
            ..Server::default()
        };
        let output =
            server.handle(json!({"seq":7,"type":"request","command":"initialize","arguments":{}}));
        assert_eq!(output.len(), 2);
        assert_eq!(output[0]["request_seq"], 7);
        assert_eq!(output[0]["success"], false);
        assert_eq!(output[1]["event"], "terminated");
        assert!(output[0]["seq"].as_u64().unwrap() < output[1]["seq"].as_u64().unwrap());
        assert!(output[1]["seq"].as_u64().unwrap() <= i32::MAX as u64);
        assert!(server.closed());
        assert!(server.poll().is_empty());
        assert!(server.handle(Value::Null).is_empty());
    }

    #[test]
    fn exhausted_target_identity_refuses_before_new_preparation() {
        let mut server = Server {
            initialized: true,
            next_epoch: 2_147_418_112,
            ..Server::default()
        };
        let output = server
            .handle(json!({"seq":1,"type":"request","command":"launch","arguments":{"mode":"vm"}}));
        assert_eq!(output[0]["success"], false);
        assert_eq!(output[0]["message"], "debug target identity limit exceeded");
        assert!(server.target.is_none());
        assert!(server.native.is_none());
        assert!(server.launch.is_none());
    }
}
