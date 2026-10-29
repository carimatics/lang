//! Private bounded DAP transport; protocol clients cannot select the tool.
use super::super::{WIRE, wire};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    io::BufReader,
    path::Path,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread::JoinHandle,
    time::{Duration, Instant},
};

pub(super) struct Connection {
    child: Child,
    input: Option<ChildStdin>,
    receiver: Option<Receiver<Result<Value, String>>>,
    reader: Option<JoinHandle<()>>,
    sequence: u32,
    outstanding: BTreeMap<u32, String>,
    responses: BTreeMap<u32, Value>,
    events: VecDeque<Value>,
    event_bytes: usize,
}
impl Connection {
    pub fn open(cwd: &Path) -> Result<Self, String> {
        let mut command = if cfg!(windows) {
            let adapter = std::env::var_os("LLDB_DAP_EXECUTABLE")
                .filter(|s| !s.is_empty())
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::PathBuf::from(
                        std::env::var_os("ProgramFiles")
                            .unwrap_or_else(|| r"C:\Program Files".into()),
                    )
                    .join("LLVM/bin/lldb-dap.exe")
                });
            if !adapter.is_absolute() || !adapter.is_file() {
                return Err("native DAP tool is unavailable".into());
            }
            let mut command = Command::new(adapter);
            command.env("LLDB_USE_NATIVE_PDB_READER", "1");
            if let Some(runtime) = std::env::var_os("LLDB_PYTHON_RUNTIME").filter(|s| !s.is_empty())
            {
                let runtime = std::path::PathBuf::from(runtime);
                let home = runtime
                    .parent()
                    .filter(|_| runtime.is_absolute() && runtime.is_file())
                    .ok_or("invalid native debugger Python runtime")?;
                let dll =
                    std::env::var("LLDB_PYTHON_DLL").unwrap_or_else(|_| "python310.dll".into());
                if Path::new(&dll).file_name().and_then(|s| s.to_str()) != Some(dll.as_str())
                    || !home.join(dll).is_file()
                {
                    return Err("native debugger Python ABI is unavailable".into());
                }
                let mut paths = vec![home.to_owned()];
                paths.extend(std::env::split_paths(
                    &std::env::var_os("PATH").unwrap_or_default(),
                ));
                command.env("PYTHONHOME", home).env(
                    "PATH",
                    std::env::join_paths(paths)
                        .map_err(|_| "invalid native debugger runtime path")?,
                );
            }
            command
        } else if cfg!(target_os = "macos") {
            let mut command = Command::new("xcrun");
            command.arg("lldb-dap");
            command
        } else {
            return Err("native debugging requires a tier-1 host".into());
        };
        let mut child = command
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| "cannot start native debugger")?;
        let input = child.stdin.take();
        let stdout = child.stdout.take().expect("piped debugger output");
        let (send, receiver) = mpsc::sync_channel(4);
        let reader = std::thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                let message = match wire::read(&mut stdout, WIRE) {
                    Ok(Some(bytes)) => serde_json::from_slice(&bytes)
                        .map_err(|_| "invalid native debugger JSON".to_owned()),
                    Ok(None) => Err("native debugger closed its output".into()),
                    Err(_) => Err("invalid native debugger framing".into()),
                };
                let end = message.is_err();
                if send.send(message).is_err() || end {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            receiver: Some(receiver),
            reader: Some(reader),
            sequence: 0,
            outstanding: BTreeMap::new(),
            responses: BTreeMap::new(),
            events: VecDeque::new(),
            event_bytes: 0,
        })
    }
    pub fn send(&mut self, command: &str, arguments: Value) -> Result<u32, String> {
        if self.outstanding.len() >= 2 {
            return Err("native debugger request limit exceeded".into());
        }
        self.sequence = self
            .sequence
            .checked_add(1)
            .filter(|n| *n <= i32::MAX as u32)
            .ok_or("native debugger sequence exhausted")?;
        wire::write(
            self.input.as_mut().ok_or("native debugger disconnected")?,
            &json!({"seq":self.sequence,"type":"request","command":command,"arguments":arguments}),
            WIRE,
        )
        .map_err(|_| "cannot write native debugger request")?;
        self.outstanding.insert(self.sequence, command.into());
        Ok(self.sequence)
    }
    fn accept(&mut self, value: Value) -> Result<(), String> {
        if value["seq"].as_u64().is_none_or(|n| n > i32::MAX as u64) {
            return Err("invalid native debugger sequence".into());
        }
        match value["type"].as_str() {
            Some("response") => {
                let seq = value["request_seq"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or("invalid native debugger response")?;
                if self
                    .outstanding
                    .get(&seq)
                    .is_none_or(|c| value["command"] != c.as_str())
                    || self.responses.contains_key(&seq)
                    || !value["success"].is_boolean()
                {
                    return Err("uncorrelated native debugger response".into());
                }
                self.responses.insert(seq, value);
            }
            Some("event") if value["event"].as_str().is_some_and(|s| s.len() <= 128) => {
                let bytes = serde_json::to_vec(&value)
                    .map_err(|_| "native event encoding failed")?
                    .len();
                if self.events.len() >= 128
                    || bytes.saturating_add(self.event_bytes) > WIRE.message_bytes
                {
                    return Err("native debugger event limit exceeded".into());
                }
                self.event_bytes += bytes;
                self.events.push_back(value);
            }
            _ => return Err("unsupported native debugger message".into()),
        }
        Ok(())
    }
    fn receive(&mut self, deadline: Instant) -> Result<(), String> {
        let value = self
            .receiver
            .as_ref()
            .ok_or("native debugger disconnected")?
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .map_err(|_| "native debugger response timed out")??;
        self.accept(value)
    }
    pub fn response(&mut self, sequence: u32) -> Result<Value, String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !self.responses.contains_key(&sequence) {
            self.receive(deadline)?;
        }
        let value = self
            .responses
            .remove(&sequence)
            .expect("correlated response");
        self.outstanding.remove(&sequence);
        if value["success"] != true {
            return Err("native debugger refused the request".into());
        }
        Ok(value.get("body").cloned().unwrap_or_else(|| json!({})))
    }
    pub fn request(&mut self, command: &str, args: Value) -> Result<Value, String> {
        let sequence = self.send(command, args)?;
        self.response(sequence)
    }
    fn remove_event(&mut self, index: usize) -> Value {
        let value = self.events.remove(index).expect("selected event");
        self.event_bytes -= serde_json::to_vec(&value).expect("validated event").len();
        value
    }
    pub fn initialized(&mut self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(i) = self.events.iter().position(|e| e["event"] == "initialized") {
                self.remove_event(i);
                return Ok(());
            }
            // A refused launch can arrive before the initialized event.
            if self.responses.values().any(|r| r["success"] == false) {
                return Err("native debugger launch failed".into());
            }
            self.receive(deadline)?;
        }
    }
    pub fn poll(&mut self) -> Result<Option<Value>, String> {
        if !self.events.is_empty() {
            return Ok(Some(self.remove_event(0)));
        }
        match self
            .receiver
            .as_ref()
            .ok_or("native debugger disconnected")?
            .try_recv()
        {
            Ok(value) => {
                self.accept(value?)?;
                Ok((!self.events.is_empty()).then(|| self.remove_event(0)))
            }
            Err(mpsc::TryRecvError::Empty) => Ok(None),
            Err(_) => Err("native debugger disconnected".into()),
        }
    }
    pub fn disconnect(&mut self) -> Result<(), String> {
        self.request("disconnect", json!({"terminateDebuggee":true}))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self
                .child
                .try_wait()
                .map_err(|_| "cannot wait for native debugger")?
            {
                Some(status) => {
                    return if status.success() {
                        Ok(())
                    } else {
                        Err("native debugger exited abnormally".into())
                    };
                }
                None if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                None => return Err("native debugger shutdown timed out".into()),
            }
        }
    }
}
impl Drop for Connection {
    fn drop(&mut self) {
        // Release a reader blocked on the bounded queue before joining it.
        self.receiver.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        self.input.take();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
