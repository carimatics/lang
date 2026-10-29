use super::inspect::{self, Preview, Query};
use super::mixed::{self, NativeHosts, PhysicalFrame};
use super::{Coordinates, uri};
use lang_semantics::program_lowering::DebugInfo;
mod inspection;
use crate::{
    frontend,
    package::editor::{Limits, workspace::Config},
};
use lang_bytecode::unit::Program;
use lang_mir::unit as m;
use lang_runtime::{HostFunctionRegistry, execution::InterruptionFlag};
use lang_vm::unit::{Control, DebugEvent, DebugPoint, DebugSession, HostBindings, StackFrame};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
    sync::{Arc, mpsc},
    thread::JoinHandle,
};

const INDEX_BYTES: usize = 32 * 1024 * 1024;
const WORK: usize = 10_000_000;
const REFERENCES: u32 = 65_536;
type Key = (m::FunctionId, m::BlockId, usize);
fn key(point: &DebugPoint) -> Key {
    (point.function, point.block, point.instruction)
}

struct Source {
    path: PathBuf,
    identity: String,
    text: String,
    lines: Vec<usize>,
}
impl Source {
    fn position_work(&self, offset: u32) -> usize {
        let offset = offset as usize;
        self.lines
            .partition_point(|start| *start <= offset)
            .checked_sub(1)
            .map_or(WORK + 1, |line| offset.saturating_sub(self.lines[line]))
    }
    fn position(&self, offset: u32, c: Coordinates) -> Option<(u32, u32)> {
        let offset = offset as usize;
        if !self.text.is_char_boundary(offset) {
            return None;
        }
        let line = self
            .lines
            .partition_point(|start| *start <= offset)
            .checked_sub(1)?;
        let column = self.text[self.lines[line]..offset].encode_utf16().count();
        Some((
            u32::try_from(line).ok()?.checked_add(c.lines)?,
            u32::try_from(column).ok()?.checked_add(c.columns)?,
        ))
    }
    fn descriptor(&self, id: usize, c: Coordinates) -> Result<Value, String> {
        let path = if c.uri {
            uri::file(&self.path).map_err(|_| "invalid source URI")?
        } else {
            self.path
                .to_str()
                .ok_or("non-UTF-8 source path")?
                .to_owned()
        };
        Ok(
            json!({"name":self.path.file_name().and_then(|s|s.to_str()),"path":path,"sourceReference":id+1}),
        )
    }
}

pub(super) struct Run {
    breakpoints: BTreeSet<Key>,
    stop: Stop,
    skip: Option<DebugPoint>,
}
pub(super) enum Command {
    Run(Run),
    Inspect {
        frame: usize,
        query: Query,
        reply: mpsc::SyncSender<Result<Vec<Preview>, String>>,
    },
}
#[derive(Clone)]
enum Reference {
    Locals(usize),
    Value(usize, Vec<String>),
}
impl Reference {
    fn bytes(&self) -> usize {
        match self {
            Self::Locals(_) => size_of::<Self>(),
            Self::Value(_, path) => path.iter().fold(size_of::<Self>(), |n, s| {
                n.saturating_add(size_of::<String>())
                    .saturating_add(s.len())
            }),
        }
    }
}
#[derive(Clone, Copy)]
pub(super) enum StepKind {
    Into,
    Over,
    Out,
}
enum Stop {
    Continue,
    Entry,
    Step(StepKind, DebugPoint),
}
#[derive(Clone, Copy)]
pub(super) enum StopReason {
    Entry,
    Breakpoint,
    Step,
    Native,
}
impl StopReason {
    pub fn name(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Breakpoint => "breakpoint",
            Self::Step => "step",
            Self::Native => "pause",
        }
    }
}
struct Point {
    key: Key,
    span: m::Span,
    position: (u32, u32),
}
pub(super) enum Event {
    Paused {
        point: DebugPoint,
        stack: Vec<StackFrame>,
        reason: StopReason,
        physical: Vec<PhysicalFrame>,
    },
    Exited {
        value: i64,
        fuel: u64,
    },
    Failed(String),
}
#[derive(Clone, Copy, PartialEq)]
enum State {
    Configuring,
    Running,
    Paused,
    Exited,
}

pub(super) struct Target {
    sources: Vec<Source>,
    functions: Vec<String>,
    points: Vec<Point>,
    breakpoints: BTreeMap<usize, BTreeSet<Key>>,
    sender: Option<mpsc::SyncSender<Command>>,
    receiver: mpsc::Receiver<Event>,
    interruption: Arc<InterruptionFlag>,
    worker: Option<JoinHandle<()>>,
    state: State,
    stop_on_entry: bool,
    point: Option<DebugPoint>,
    frames: Vec<(u32, StackFrame)>,
    physical: Vec<(u32, PhysicalFrame)>,
    next_reference: u32,
    references: BTreeMap<u32, Reference>,
    index_bytes: usize,
    reference_bytes: usize,
}
impl Drop for Target {
    fn drop(&mut self) {
        self.interruption.interrupt();
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn text<'a>(args: &'a Value, name: &str) -> Result<&'a str, String> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing or invalid {name}"))
}
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}
fn worker(
    program: Program,
    debug: DebugInfo,
    native_callbacks: Vec<lang_runtime::I64HostFunction>,
    flag: Arc<InterruptionFlag>,
    receive: mpsc::Receiver<Command>,
    send: mpsc::Sender<Event>,
    ready: mpsc::SyncSender<Result<(), String>>,
) {
    let mut hosts = HostFunctionRegistry::new();
    let mut bindings = HostBindings::new();
    let unit = program.verified().unit();
    if native_callbacks.len() != unit.imports.len() {
        let _ = ready.send(Err("missing native host grant".into()));
        return;
    }
    for (i, import) in unit.imports.iter().enumerate() {
        let id = lang_mir::HostFunctionId::new(i as u32 + 1);
        if hosts.register_i64(id, mixed::dispatch).is_err()
            || bindings.bind(&import.name, id).is_err()
        {
            let _ = ready.send(Err("invalid native host grant".into()));
            return;
        }
    }
    let mut session = match DebugSession::entry(
        &program,
        &hosts,
        &bindings,
        Control::default().and_interruption(&flag),
    ) {
        Ok(session) => session,
        Err(error) => {
            let _ = ready.send(Err(error.to_string()));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    let receive = std::rc::Rc::new(std::cell::RefCell::new(receive));
    while let Ok(command) = receive.borrow().recv() {
        let mut run = match command {
            Command::Run(run) => run,
            Command::Inspect {
                frame,
                query,
                reply,
            } => {
                let _ = reply.send(inspect::query(&program, &debug, &session, frame, query));
                continue;
            }
        };
        let mut first = true;
        let mut reason = StopReason::Breakpoint;
        let mut scope: Option<std::rc::Rc<mixed::Scope>> = None;
        let mut scope_guard: Option<mixed::Guard> = None;
        let mut internal_skip = None;
        let event = loop {
            let mut native_point = None;
            let mut orphan = false;
            let event = session.resume_until(|point| {
                if !native_callbacks.is_empty() && mixed::orphan_failure() {
                    orphan = true;
                    return true;
                }
                if let Some(next) = scope.as_ref().and_then(|s| s.take_run()) {
                    run = next;
                    first = true;
                }
                if internal_skip.take() == Some(*point) {
                    return false;
                }
                let initial = std::mem::replace(&mut first, false);
                let skip = initial && run.skip == Some(*point);
                if !skip {
                    if run.breakpoints.contains(&key(point)) {
                        reason = StopReason::Breakpoint;
                        return true;
                    }
                    match run.stop {
                        Stop::Continue => {}
                        Stop::Entry => {
                            reason = StopReason::Entry;
                            return true;
                        }
                        Stop::Step(kind, base) => {
                            let depth = match kind {
                                StepKind::Into => true,
                                StepKind::Over => point.depth <= base.depth,
                                StepKind::Out => point.depth < base.depth,
                            };
                            if depth
                                && point.span.is_some()
                                && (point.span != base.span
                                    || point.depth != base.depth
                                    || *point == base)
                            {
                                reason = StopReason::Step;
                                return true;
                            }
                        }
                    }
                }
                if !native_callbacks.is_empty()
                    && let Some(instruction) = unit.functions[point.function.0 as usize].blocks
                        [point.block.0 as usize]
                        .instructions
                        .get(point.instruction)
                    && let m::Op::HostCall { import, .. } = instruction.op
                {
                    native_point = Some((*point, import.0 as usize));
                    return true;
                }
                false
            });
            if orphan {
                flag.interrupt();
                break Event::Failed("native callback has no live debug scope".into());
            }
            if let Some(error) = scope.as_ref().and_then(|s| s.failure()) {
                break Event::Failed(error.to_string());
            }
            if let (Some((point, import)), Ok(DebugEvent::Paused(_))) = (native_point, &event) {
                scope_guard.take();
                let current = mixed::Scope::new(
                    native_callbacks[import],
                    point,
                    session.stack().collect(),
                    receive.clone(),
                    send.clone(),
                    flag.clone(),
                );
                scope_guard = Some(mixed::enter(current.clone()));
                scope = Some(current);
                internal_skip = Some(point);
                continue;
            }
            break match event {
                Ok(DebugEvent::Paused(point)) => Event::Paused {
                    point,
                    stack: session.stack().collect(),
                    reason,
                    physical: vec![],
                },
                Ok(DebugEvent::Exited(outcome)) => Event::Exited {
                    value: outcome.value.as_i64().unwrap_or_default(),
                    fuel: outcome.usage.fuel,
                },
                Err(error) => Event::Failed(error.to_string()),
            };
        };
        drop(scope_guard);
        let terminal = !matches!(event, Event::Paused { .. });
        if send.send(event).is_err() || terminal {
            break;
        }
    }
}
pub(super) fn capture(args: &Value) -> Result<crate::package::editor::workspace::View, String> {
    let root = Path::new(text(args, "root")?);
    if !root.is_absolute() {
        return Err("root must be absolute".into());
    }
    let root = std::fs::canonicalize(root).map_err(|_| "root is unavailable")?;
    let manifest = Path::new(text(args, "manifestPath")?);
    if !relative(manifest) {
        return Err("manifestPath must be relative to root".into());
    }
    let target = args
        .get("target")
        .map(|_| text(args, "target"))
        .transpose()?
        .unwrap_or(if cfg!(windows) {
            "x86_64-pc-windows-msvc"
        } else {
            "aarch64-apple-darwin"
        });
    let selected = args
        .get("package")
        .map(|_| text(args, "package").map(str::to_owned))
        .transpose()?;
    Config {
        manifest: root.join(manifest),
        root,
        selected,
        target: target.into(),
    }
    .capture(1, &[], Limits::default())
    .map_err(|_| "package debug admission failed".to_owned())
}
impl Target {
    pub fn launch(args: &Value, c: Coordinates) -> Result<Self, String> {
        if text(args, "mode")? != "vm" {
            return Err("unsupported debug mode".into());
        }
        Self::prepare(args, c, None)
    }
    pub fn launch_mixed(args: &Value, c: Coordinates, hosts: NativeHosts) -> Result<Self, String> {
        if hosts.is_empty() {
            return Err("mixed launch requires explicit Rust host grants".into());
        }
        Self::prepare(args, c, Some(hosts))
    }
    fn prepare(args: &Value, _c: Coordinates, hosts: Option<NativeHosts>) -> Result<Self, String> {
        let stop_on_entry = match args.get("stopOnEntry") {
            None => false,
            Some(v) => v.as_bool().ok_or("invalid stopOnEntry")?,
        };
        let view = capture(args)?;
        let analysis = view.snapshot.analysis();
        let checked = analysis
            .result
            .as_ref()
            .map_err(|_| "package has source errors")?;
        let typed = frontend::copy_executable(checked)
            .map_err(|_| "debugger requires the Copy source profile")?;
        let (program, debug) =
            frontend::compile_script_debug(typed, &analysis.sources, Default::default())
                .map_err(|_| "debug lowering failed")?;
        let unit = program.verified().unit();
        if unit.entry.is_none() || (hosts.is_none() && !unit.imports.is_empty()) {
            return Err("debug target needs an entry and cannot acquire host imports".into());
        }
        let native_callbacks = unit
            .imports
            .iter()
            .map(|import| {
                hosts
                    .as_ref()
                    .and_then(|h| h.get(&import.name))
                    .ok_or("missing explicit native host grant".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let entry = &unit.functions[unit.entry.unwrap().0 as usize];
        if entry.signature.result != m::Type::I64 || !entry.signature.parameters.is_empty() {
            return Err("debug entry must have no parameters and return i64".into());
        }
        if unit.sources.len() > REFERENCES as usize {
            return Err("debug source reference limit exceeded".into());
        }
        let mut bytes = debug.bytes();
        let mut work = 0usize;
        let mut sources = Vec::new();
        for source in &unit.sources {
            if source.text.len().saturating_add(bytes) > INDEX_BYTES
                || source.text.len().saturating_add(work) > WORK
            {
                return Err("debug index limit exceeded".into());
            }
            let relative = Path::new(&source.path);
            if !self::relative(relative) {
                return Err("invalid lowered source identity".into());
            }
            let path = view.authority.join(relative);
            let identity = uri::file(&path).map_err(|_| "invalid captured source identity")?;
            let (_, captured) = view
                .source(&path)
                .map_err(|_| "unadmitted lowered source")?;
            if captured.text() != source.text {
                return Err("lowered source snapshot mismatch".into());
            }
            let mut lines = vec![0];
            for (i, b) in source.text.bytes().enumerate() {
                if b == b'\n' {
                    if lines
                        .len()
                        .saturating_add(1)
                        .saturating_mul(size_of::<usize>())
                        .saturating_add(source.text.len())
                        .saturating_add(bytes)
                        > INDEX_BYTES
                    {
                        return Err("debug index limit exceeded".into());
                    }
                    lines.push(i + 1);
                }
            }
            bytes = bytes
                .saturating_add(source.text.len())
                .saturating_add(lines.len() * size_of::<usize>())
                .saturating_add(source.path.len())
                .saturating_add(identity.len());
            work = work.saturating_add(source.text.len());
            if bytes > INDEX_BYTES || work > WORK {
                return Err("debug index limit exceeded".into());
            }
            sources.push(Source {
                path,
                identity,
                text: source.text.clone(),
                lines,
            });
        }
        let mut points = Vec::new();
        let mut functions = Vec::new();
        for (fi, function) in unit.functions.iter().enumerate() {
            functions.push(function.name.clone());
            bytes = bytes.saturating_add(function.name.len());
            for (bi, block) in function.blocks.iter().enumerate() {
                for (ip, span) in block
                    .instructions
                    .iter()
                    .map(|i| i.span)
                    .chain(std::iter::once(block.terminator.span))
                    .enumerate()
                {
                    work = work.saturating_add(1);
                    if work > WORK {
                        return Err("debug index limit exceeded".into());
                    }
                    if let Some(span) = span {
                        bytes = bytes.saturating_add(size_of::<Point>());
                        if bytes > INDEX_BYTES || work > WORK {
                            return Err("debug index limit exceeded".into());
                        }
                        let source = sources
                            .get(span.source.0 as usize)
                            .ok_or("invalid debug source")?;
                        work = work.saturating_add(source.position_work(span.start));
                        if work > WORK {
                            return Err("debug index work limit exceeded".into());
                        }
                        let position = source
                            .position(
                                span.start,
                                Coordinates {
                                    lines: 0,
                                    columns: 0,
                                    uri: false,
                                },
                            )
                            .ok_or("invalid debug span")?;
                        points.push(Point {
                            key: (m::FunctionId(fi as u32), m::BlockId(bi as u32), ip),
                            span,
                            position,
                        });
                    }
                }
            }
        }
        let (sender, commands) = mpsc::sync_channel(1);
        let (events, receiver) = mpsc::channel();
        let (ready, prepared) = mpsc::sync_channel(1);
        let interruption = Arc::new(InterruptionFlag::new());
        let flag = interruption.clone();
        let worker = std::thread::Builder::new()
            .name("lang-dap-vm".into())
            .spawn(move || {
                worker(
                    program,
                    debug,
                    native_callbacks,
                    flag,
                    commands,
                    events,
                    ready,
                )
            })
            .map_err(|_| "cannot start debug worker")?;
        let next_reference = sources.len() as u32 + 1;
        let target = Self {
            sources,
            functions,
            points,
            breakpoints: BTreeMap::new(),
            sender: Some(sender),
            receiver,
            interruption,
            worker: Some(worker),
            state: State::Configuring,
            stop_on_entry,
            point: None,
            frames: vec![],
            physical: vec![],
            next_reference,
            references: BTreeMap::new(),
            index_bytes: bytes,
            reference_bytes: 0,
        };
        prepared
            .recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|_| "debug preparation did not complete")??;
        Ok(target)
    }
    pub fn thread(&self, args: &Value) -> Result<(), String> {
        if args["threadId"].as_u64() != Some(1) || self.state == State::Exited {
            return Err("unknown debug thread".into());
        }
        Ok(())
    }
    pub fn threads(&self) -> Value {
        if self.state == State::Exited {
            json!([])
        } else {
            json!([{"id":1,"name":"Lang VM"}])
        }
    }
    fn run(&mut self, stop: Stop) -> Result<(), String> {
        let run = Run {
            breakpoints: self.breakpoints.values().flatten().copied().collect(),
            stop,
            skip: self.point,
        };
        self.sender
            .as_ref()
            .ok_or("debug worker unavailable")?
            .try_send(Command::Run(run))
            .map_err(|_| "debug worker unavailable")?;
        self.frames.clear();
        self.physical.clear();
        self.references.clear();
        self.reference_bytes = 0;
        self.point = None;
        self.state = State::Running;
        Ok(())
    }
    pub fn start(&mut self) -> Result<(), String> {
        if self.state != State::Configuring {
            return Err("target already configured".into());
        }
        self.run(if self.stop_on_entry {
            Stop::Entry
        } else {
            Stop::Continue
        })
    }
    pub fn resume(&mut self) -> Result<(), String> {
        if self.state != State::Paused {
            return Err("target is not paused".into());
        }
        self.run(Stop::Continue)
    }
    pub fn step(&mut self, args: &Value, kind: StepKind) -> Result<(), String> {
        self.thread(args)?;
        if self.state != State::Paused {
            return Err("target is not paused".into());
        }
        if self.native_paused() {
            return Err("native callback instruction stepping is unavailable".into());
        }
        if args.get("targetId").is_some()
            || args.get("granularity").is_some_and(|v| v != "statement")
        {
            return Err("unsupported step target or granularity".into());
        }
        self.run(Stop::Step(
            kind,
            self.point.ok_or("missing paused location")?,
        ))
    }
    pub fn poll(&mut self) -> Option<Event> {
        if self.state == State::Exited {
            return None;
        }
        let event = match self.receiver.try_recv() {
            Ok(event) => event,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) if self.state != State::Exited => {
                Event::Failed("debug worker disconnected".into())
            }
            Err(_) => return None,
        };
        self.frames.clear();
        self.physical.clear();
        self.references.clear();
        self.reference_bytes = 0;
        match &event {
            Event::Paused {
                point,
                stack,
                physical,
                ..
            } => {
                let count = stack.len().saturating_add(physical.len());
                if count > 256
                    || self.next_reference.saturating_add(count as u32) > REFERENCES
                    || self
                        .index_bytes
                        .saturating_add(physical.len().saturating_mul(size_of::<PhysicalFrame>()))
                        > INDEX_BYTES
                {
                    self.state = State::Exited;
                    self.interruption.interrupt();
                    self.sender.take();
                    return Some(Event::Failed("debug reference limit exceeded".into()));
                }
                self.state = State::Paused;
                self.point = Some(*point);
                for frame in physical {
                    self.physical.push((self.next_reference, frame.clone()));
                    self.next_reference += 1;
                }
                for frame in stack {
                    self.frames.push((self.next_reference, *frame));
                    self.next_reference += 1;
                }
            }
            _ => {
                self.state = State::Exited;
                self.point = None;
            }
        }
        Some(event)
    }
    pub fn native_paused(&self) -> bool {
        !self.physical.is_empty()
    }
    pub fn breakpoints(&mut self, args: &Value, c: Coordinates) -> Result<Value, String> {
        if self.state == State::Exited {
            return Err("target exited".into());
        }
        let path = text(&args["source"], "path")?;
        if path.len() > 8192 {
            return Err("source path limit exceeded".into());
        }
        let path = if c.uri {
            uri::path(path, 8192).map_err(|_| "invalid source URI")?
        } else {
            PathBuf::from(path)
        };
        // Normalize lexical local URI identity, including Windows verbatim
        // disk prefixes and localhost spelling, without rereading the disk.
        let identity = uri::file(&path).map_err(|_| "invalid source path")?;
        if self.sources.len().saturating_mul(identity.len()) > WORK {
            return Err("source lookup work limit exceeded".into());
        }
        let id = self
            .sources
            .iter()
            .position(|s| s.identity == identity)
            .ok_or("source is outside the captured target")?;
        let requested = args["breakpoints"]
            .as_array()
            .ok_or("invalid breakpoints")?;
        let other = self
            .breakpoints
            .iter()
            .filter(|(i, _)| **i != id)
            .map(|(_, p)| p.len())
            .sum::<usize>();
        if requested.len().saturating_add(other) > 256 {
            return Err("breakpoint limit exceeded".into());
        }
        let source = &self.sources[id];
        if self.points.len().saturating_mul(requested.len()) > WORK {
            return Err("breakpoint work limit exceeded".into());
        }
        let mut selected = BTreeSet::new();
        let mut result = Vec::new();
        for request in requested {
            if request.get("condition").is_some()
                || request.get("hitCondition").is_some()
                || request.get("logMessage").is_some()
            {
                return Err("conditional breakpoints are unsupported".into());
            }
            let line = request["line"]
                .as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or("invalid breakpoint line")?;
            let column = request
                .get("column")
                .map(|v| {
                    v.as_u64()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or("invalid breakpoint column")
                })
                .transpose()?
                .unwrap_or(c.columns);
            if line < c.lines || column < c.columns {
                return Err("invalid breakpoint position".into());
            }
            let mut best = None;
            for point in &self.points {
                if point.span.source.0 as usize != id {
                    continue;
                }
                let position = (point.position.0 + c.lines, point.position.1 + c.columns);
                if position >= (line, column) && best.as_ref().is_none_or(|(_, p)| position < *p) {
                    best = Some((point.key, position));
                }
            }
            if let Some((key, (line, column))) = best {
                selected.insert(key);
                result.push(json!({"verified":true,"line":line,"column":column,"source":source.descriptor(id,c)?}));
            } else {
                result.push(json!({"verified":false,"message":"no executable source location"}));
            }
        }
        self.breakpoints.insert(id, selected);
        Ok(json!({"breakpoints":result}))
    }
    pub fn stack(&self, args: &Value, c: Coordinates) -> Result<Value, String> {
        self.thread(args)?;
        if self.state != State::Paused {
            return Err("target is not paused".into());
        }
        let index = |name: &str, default: usize| -> Result<usize, String> {
            args.get(name)
                .map(|n| {
                    n.as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .ok_or_else(|| format!("invalid {name}"))
                })
                .unwrap_or(Ok(default))
        };
        let start = index("startFrame", 0)?;
        let levels = index("levels", 256)?;
        let levels = if levels == 0 { 256 } else { levels };
        if levels > 256 {
            return Err("stack response limit exceeded".into());
        }
        let mut frames = vec![];
        let mut work = 0usize;
        for (id, frame) in self.physical.iter().skip(start).take(levels) {
            frames.push(json!({"id":id,"name":format!("native 0x{:x}",frame.address),
                "line":c.lines,"column":c.columns,"instructionPointerReference":format!("0x{:x}",frame.address)}));
        }
        let vm_start = start.saturating_sub(self.physical.len());
        let vm_count = levels.saturating_sub(frames.len());
        for (id, frame) in self.frames.iter().skip(vm_start).take(vm_count) {
            let mut value = json!({"id":id,"name":self.functions[frame.function.0 as usize],
                "line":c.lines,"column":c.columns});
            if let Some(span) = frame.span {
                let source = &self.sources[span.source.0 as usize];
                work = work.saturating_add(source.position_work(span.start));
                if work > WORK {
                    return Err("stack query work limit exceeded".into());
                }
                let (line, column) = source
                    .position(span.start, c)
                    .ok_or("invalid frame location")?;
                value["line"] = json!(line);
                value["column"] = json!(column);
                value["source"] = source.descriptor(span.source.0 as usize, c)?;
            }
            frames.push(value);
        }
        Ok(json!({"stackFrames":frames,"totalFrames":self.physical.len()+self.frames.len()}))
    }
    pub fn source(&self, args: &Value) -> Result<Value, String> {
        let id = args["sourceReference"]
            .as_u64()
            .and_then(|n| n.checked_sub(1))
            .and_then(|n| usize::try_from(n).ok())
            .ok_or("invalid sourceReference")?;
        if self.state == State::Exited {
            return Err("target exited".into());
        }
        let source = self.sources.get(id).ok_or("unknown sourceReference")?;
        Ok(json!({"content":source.text,"mimeType":"text/x-lang"}))
    }
}
