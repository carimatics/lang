//! Trusted native launch over the same admitted immutable source snapshot.
mod connection;
use super::{Coordinates, target, uri};
use crate::{frontend, native};
use connection::Connection;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

struct Source {
    path: PathBuf,
    relative: String,
    identity: String,
    text: String,
    lines: Vec<usize>,
}
impl Source {
    fn descriptor(&self, id: usize, c: Coordinates) -> Result<Value, String> {
        let path = if c.uri {
            self.identity.clone()
        } else {
            self.path.to_str().ok_or("non-UTF-8 source path")?.into()
        };
        Ok(
            json!({"name":self.path.file_name().and_then(|s|s.to_str()),"path":path,"sourceReference":id+1}),
        )
    }
}
#[derive(PartialEq)]
enum State {
    Configuring,
    Running,
    Paused,
    Exited,
}
pub(super) struct Target {
    // Close the debugger before dropping the actual code/snapshot owners.
    connection: Connection,
    _image: native::link::DebugImage,
    _stage: native::Stage,
    authority: PathBuf,
    sources: Vec<Source>,
    breakpoints: BTreeMap<usize, usize>,
    launch: u32,
    state: State,
    thread: Option<u64>,
    next_reference: u32,
    disconnected: bool,
}
impl Target {
    pub fn launch(args: &Value) -> Result<Self, String> {
        let began = std::time::Instant::now();
        let checkpoint = |name: &str| {
            if std::env::var_os("LANG_DAP_TRACE").is_some() {
                eprintln!("native DAP launch (+{:?}): {name}", began.elapsed());
            }
        };
        checkpoint("begin admission");
        let host = lang_codegen_clif::Triple::host();
        if args
            .get("target")
            .is_some_and(|value| value.as_str() != Some(host.to_string().as_str()))
        {
            return Err("native debug target must match the host".into());
        }
        let sdk = args["sdk"]
            .as_str()
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or("native launch requires an absolute sdk directory")?;
        let stop = args
            .get("stopOnEntry")
            .map(|v| v.as_bool().ok_or("invalid stopOnEntry"))
            .transpose()?
            .unwrap_or(false);
        let view = target::capture(args)?;
        let analysis = view.snapshot.analysis();
        let checked = analysis
            .result
            .as_ref()
            .map_err(|_| "package has source errors")?;
        let typed = frontend::copy_executable(checked)
            .map_err(|_| "native debugger requires the Copy profile")?;
        let program = frontend::compile_script(typed, &analysis.sources, Default::default())
            .map_err(|_| "native debug lowering failed")?;
        let unit = program.verified().unit();
        if unit.entry.is_none() || !unit.imports.is_empty() {
            return Err("native debug entry cannot acquire host imports".into());
        }
        if unit.sources.len() >= 65_536 {
            return Err("native source reference limit exceeded".into());
        }
        let mut sources = Vec::new();
        let mut bytes = 0usize;
        let mut work = 0usize;
        for source in &unit.sources {
            if source.text.len().saturating_add(work) > 10_000_000
                || source.text.len().saturating_add(bytes) > 32 * 1024 * 1024
            {
                return Err("native source index limit exceeded".into());
            }
            let relative = Path::new(&source.path);
            if relative.as_os_str().is_empty()
                || !relative
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)))
            {
                return Err("invalid native source identity".into());
            }
            let path = view.authority.join(relative);
            let (_, captured) = view.source(&path).map_err(|_| "unadmitted native source")?;
            if captured.text() != source.text {
                return Err("native source snapshot mismatch".into());
            }
            let identity = uri::file(&path).map_err(|_| "invalid native source identity")?;
            let mut lines = vec![0];
            for (i, b) in source.text.bytes().enumerate() {
                if b == b'\n' {
                    lines.push(i + 1);
                }
                if lines
                    .len()
                    .saturating_mul(size_of::<usize>())
                    .saturating_add(source.text.len())
                    .saturating_add(bytes)
                    > 32 * 1024 * 1024
                {
                    return Err("native source index limit exceeded".into());
                }
            }
            bytes = bytes
                .saturating_add(source.text.len())
                .saturating_add(source.path.len())
                .saturating_add(identity.len())
                .saturating_add(lines.len() * size_of::<usize>());
            work = work.saturating_add(source.text.len());
            if bytes > 32 * 1024 * 1024 || work > 10_000_000 {
                return Err("native source index limit exceeded".into());
            }
            sources.push(Source {
                path,
                relative: source.path.clone(),
                identity,
                text: source.text.clone(),
                lines,
            });
        }
        let package = native::compile(checked.clone(), &analysis.sources, Default::default(), host)
            .map_err(|_| "native debug compilation failed")?;
        checkpoint("compiled admitted source");
        let stage = native::Stage::new(
            &std::env::temp_dir().join(format!("lang-dap-{}", std::process::id())),
        )
        .map_err(|_| "cannot prepare private native debug directory")?;
        let package_path = stage.0.join("program");
        package
            .publish(&package_path)
            .map_err(|_| "cannot prepare native debug package")?;
        let image = native::link::link_debug(
            &native::link::Request {
                sdk,
                package: package_path,
                host: None,
                objects: vec![],
                output: stage.0.join("unused-output"),
                rustc: None,
            },
            Default::default(),
        )
        .map_err(|_| "native debug SDK/link admission failed")?;
        checkpoint("linked verified SDK snapshots");
        let semantic = lang_bytecode::unit::encode(&program, Default::default())
            .map_err(|_| "native semantic encoding failed")?;
        if image.semantic() != semantic {
            return Err("native debug semantic identity mismatch".into());
        }
        // canonicalize uses internal verbatim prefixes on Windows. Pass the
        // same local identity in ordinary tool syntax across the OS boundary.
        let cwd = tool_path(&view.authority)?;
        let executable = tool_path(image.executable())?;
        let mut connection = Connection::open(&cwd)?;
        checkpoint("spawned debugger");
        connection.request(
            "initialize",
            json!({"adapterID":"lang-native","linesStartAt1":true,
            "columnsStartAt1":true,"pathFormat":"path","supportsRunInTerminalRequest":false}),
        )?;
        checkpoint("initialized debugger");
        let launch = connection.send(
            "launch",
            json!({"program":executable,"cwd":cwd,
            "stopOnEntry":stop,"sourceMap":[[".",cwd]]}),
        )?;
        connection.initialized()?;
        checkpoint("prepared native launch");
        Ok(Self {
            connection,
            _image: image,
            _stage: stage,
            authority: view.authority,
            next_reference: sources.len() as u32 + 1,
            sources,
            breakpoints: BTreeMap::new(),
            launch,
            state: State::Configuring,
            thread: None,
            disconnected: false,
        })
    }
    fn source_id(&self, path: &str, c: Coordinates) -> Result<usize, String> {
        if path.len() > 8192 || self.sources.len().saturating_mul(path.len()) > 10_000_000 {
            return Err("native source lookup limit exceeded".into());
        }
        let path = if c.uri {
            uri::path(path, 8192).map_err(|_| "invalid source URI")?
        } else {
            PathBuf::from(path)
        };
        let identity = uri::file(&path).map_err(|_| "invalid native source path")?;
        self.sources
            .iter()
            .position(|s| s.identity == identity)
            .ok_or_else(|| "source is outside the captured target".into())
    }
    fn thread(&self, args: &Value) -> Result<u64, String> {
        if self.state != State::Paused || args["threadId"].as_u64() != Some(1) {
            return Err("native target is not paused on this thread".into());
        }
        self.thread
            .ok_or_else(|| "native debug thread is unavailable".into())
    }
    pub fn dispatch(
        &mut self,
        command: &str,
        args: &Value,
        c: Coordinates,
    ) -> Result<Value, String> {
        match command {
            "source" => {
                if self.state == State::Exited {
                    return Err("native target exited".into());
                }
                let id = args["sourceReference"]
                    .as_u64()
                    .and_then(|n| n.checked_sub(1))
                    .and_then(|n| usize::try_from(n).ok())
                    .ok_or("invalid sourceReference")?;
                let source = self.sources.get(id).ok_or("unknown sourceReference")?;
                Ok(json!({"content":source.text,"mimeType":"text/x-lang"}))
            }
            "setBreakpoints" => self.breakpoints(args, c),
            "configurationDone" => {
                if self.state != State::Configuring {
                    return Err("native target already configured".into());
                }
                self.connection.request(command, json!({}))?;
                self.connection.response(self.launch)?;
                self.state = State::Running;
                Ok(json!({}))
            }
            "threads" => Ok(
                json!({"threads":if self.state==State::Exited {json!([])} else {json!([{"id":1,"name":"Lang native"}])}}),
            ),
            "stackTrace" => self.stack(args, c),
            "continue" | "stepIn" | "next" | "stepOut" => {
                let thread = self.thread(args)?;
                if args.get("targetId").is_some()
                    || args.get("singleThread") == Some(&Value::Bool(true))
                    || args.get("granularity").is_some_and(|g| g != "statement")
                {
                    return Err("unsupported native step control".into());
                }
                self.connection
                    .request(command, json!({"threadId":thread}))?;
                self.state = State::Running;
                self.thread = None;
                Ok(if command == "continue" {
                    json!({"allThreadsContinued":true})
                } else {
                    json!({})
                })
            }
            "scopes" | "variables" | "evaluate" => {
                Err("native value inspection is unavailable".into())
            }
            _ => Err("unsupported native DAP command".into()),
        }
    }
    fn breakpoints(&mut self, args: &Value, c: Coordinates) -> Result<Value, String> {
        if self.state == State::Exited {
            return Err("native target exited".into());
        }
        let id = self.source_id(
            args["source"]["path"]
                .as_str()
                .ok_or("missing source path")?,
            c,
        )?;
        let requested = args["breakpoints"]
            .as_array()
            .ok_or("invalid breakpoints")?;
        let other: usize = self
            .breakpoints
            .iter()
            .filter(|(i, _)| **i != id)
            .map(|(_, n)| n)
            .sum();
        if requested.len().saturating_add(other) > 256 {
            return Err("breakpoint limit exceeded".into());
        }
        let source = &self.sources[id];
        let mut native = Vec::new();
        for b in requested {
            if ["condition", "hitCondition", "logMessage"]
                .iter()
                .any(|key| b.get(key).is_some())
            {
                return Err("conditional breakpoints are unsupported".into());
            }
            let line = b["line"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .filter(|n| *n >= c.lines)
                .ok_or("invalid breakpoint line")?;
            let col = b
                .get("column")
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .filter(|n| *n >= c.columns)
                        .ok_or("invalid breakpoint column")
                })
                .transpose()?
                .unwrap_or(c.columns);
            if col != c.columns {
                return Err("native column breakpoints are unavailable".into());
            }
            let line = line
                .checked_sub(c.lines)
                .and_then(|n| n.checked_add(1))
                .ok_or("invalid breakpoint line")?;
            native.push(json!({"line":line}));
        }
        let body = self.connection.request(
            "setBreakpoints",
            json!({"source":{"path":source.relative},"breakpoints":native}),
        )?;
        let results = body["breakpoints"]
            .as_array()
            .filter(|a| a.len() == requested.len())
            .ok_or("invalid native breakpoint response")?;
        let mut output = Vec::new();
        for b in results {
            if b["verified"] == true {
                let path = b["source"]["path"]
                    .as_str()
                    .filter(|s| s.len() <= 8192)
                    .ok_or("missing native breakpoint source")?;
                let path = Path::new(path);
                let path = if path.is_absolute() {
                    path.to_owned()
                } else {
                    self.authority.join(path)
                };
                if uri::file(&path).map_err(|_| "invalid native breakpoint source")?
                    != source.identity
                {
                    return Err("native breakpoint source mismatch".into());
                }
                let line = b["line"]
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| *n > 0 && *n as usize <= source.lines.len())
                    .ok_or("invalid native breakpoint location")?;
                output.push(json!({"verified":true,"line":line-1+c.lines,"column":c.columns,"source":source.descriptor(id,c)?}));
            } else {
                output.push(
                    json!({"verified":false,"message":"no executable native source location"}),
                );
            }
        }
        self.breakpoints.insert(id, requested.len());
        Ok(json!({"breakpoints":output}))
    }
    fn stack(&mut self, args: &Value, c: Coordinates) -> Result<Value, String> {
        let thread = self.thread(args)?;
        let index = |key: &str, default: u32| -> Result<u32, String> {
            args.get(key)
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or_else(|| format!("invalid {key}"))
                })
                .unwrap_or(Ok(default))
        };
        let start = index("startFrame", 0)?;
        let levels = index("levels", 256)?;
        let levels = if levels == 0 { 256 } else { levels };
        if levels > 256 {
            return Err("stack response limit exceeded".into());
        }
        let body = self.connection.request(
            "stackTrace",
            json!({"threadId":thread,"startFrame":start,"levels":levels}),
        )?;
        let frames = body["stackFrames"]
            .as_array()
            .filter(|f| f.len() <= levels as usize)
            .ok_or("invalid native stack response")?;
        if self.next_reference.saturating_add(frames.len() as u32) > 65_536 {
            return Err("debug reference limit exceeded".into());
        }
        let mut output = Vec::new();
        let mut work = 0usize;
        for frame in frames {
            let name = frame["name"]
                .as_str()
                .filter(|n| n.len() <= 4096)
                .ok_or("invalid native frame name")?;
            let mut value =
                json!({"id":self.next_reference,"name":name,"line":c.lines,"column":c.columns});
            self.next_reference += 1;
            if let Some(path) = frame["source"]["path"].as_str().filter(|s| s.len() <= 8192) {
                let path = Path::new(path);
                let path = if path.is_absolute() {
                    path.to_owned()
                } else {
                    self.authority.join(path)
                };
                if let Ok(identity) = uri::file(&path) {
                    work = work.saturating_add(self.sources.len().saturating_mul(identity.len()));
                    if work > 10_000_000 {
                        return Err("native stack lookup limit exceeded".into());
                    }
                }
                if let Ok(identity) = uri::file(&path)
                    && let Some((id, source)) = self
                        .sources
                        .iter()
                        .enumerate()
                        .find(|(_, s)| s.identity == identity)
                {
                    let line = frame["line"]
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .filter(|n| *n > 0 && *n <= source.lines.len())
                        .ok_or("invalid native frame location")?;
                    value["line"] = json!(line - 1 + c.lines as usize);
                    value["source"] = source.descriptor(id, c)?;
                }
            }
            output.push(value);
        }
        Ok(
            json!({"stackFrames":output,"totalFrames":body["totalFrames"].as_u64().unwrap_or(start as u64+frames.len() as u64)}),
        )
    }
    pub fn poll(&mut self) -> Result<Option<Value>, String> {
        if self.state == State::Exited {
            return Ok(None);
        }
        for _ in 0..128 {
            let Some(event) = self.connection.poll()? else {
                return Ok(None);
            };
            match event["event"].as_str() {
                Some("stopped") => {
                    let thread = event["body"]["threadId"]
                        .as_u64()
                        .filter(|n| *n > 0)
                        .ok_or("invalid native stopped thread")?;
                    let reason = match event["body"]["reason"].as_str() {
                        Some("breakpoint") => "breakpoint",
                        Some("step") => "step",
                        Some("entry") => "entry",
                        _ => "exception",
                    };
                    self.thread = Some(thread);
                    self.state = State::Paused;
                    return Ok(Some(
                        json!({"event":"stopped","body":{"reason":reason,"threadId":1,"allThreadsStopped":true}}),
                    ));
                }
                Some("output") => {
                    if let Some(output) = event["body"]["output"]
                        .as_str()
                        .filter(|s| s.len() <= 1024 * 1024)
                    {
                        if output.len().saturating_mul(6)
                            > super::WIRE.message_bytes.saturating_sub(1024)
                        {
                            return Err("native output response limit exceeded".into());
                        }
                        return Ok(Some(
                            json!({"event":"output","body":{"category":"console","output":output}}),
                        ));
                    }
                }
                Some("exited") => {
                    let code = event["body"]["exitCode"]
                        .as_i64()
                        .ok_or("invalid native exit code")?;
                    return Ok(Some(json!({"event":"exited","body":{"exitCode":code}})));
                }
                Some("terminated") => {
                    self.state = State::Exited;
                    self.thread = None;
                    return Ok(Some(json!({"event":"terminated","body":{}})));
                }
                _ => {}
            }
        }
        Ok(None)
    }
    pub fn disconnect(&mut self) -> Result<(), String> {
        if self.disconnected {
            return Ok(());
        }
        self.disconnected = true;
        self.connection.disconnect()
    }
}
impl Drop for Target {
    fn drop(&mut self) {
        let _ = self.disconnect();
    }
}

fn tool_path(path: &Path) -> Result<PathBuf, String> {
    let identity = uri::file(path).map_err(|_| "invalid native tool path")?;
    uri::path(&identity, 8192).map_err(|_| "invalid native tool path".into())
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn native_tools_receive_ordinary_windows_paths_with_the_same_identity() {
        let captured = std::path::Path::new(r"\\?\D:\package dir\application.exe");
        let tool = super::tool_path(captured).unwrap();
        assert_eq!(
            tool,
            std::path::Path::new(r"D:\package dir\application.exe")
        );
        assert_eq!(
            super::uri::file(captured).unwrap(),
            super::uri::file(&tool).unwrap()
        );
    }
}
