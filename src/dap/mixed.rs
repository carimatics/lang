//! Explicit trusted Rust host bindings for cooperative mixed debugging.
//! Source and protocol input cannot register a callback or acquire this scope.
use super::target::{Command, Event, Run, StopReason};
use lang_runtime::{I64HostFunction, execution::InterruptionFlag};
use lang_vm::unit::{DebugPoint, StackFrame};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt,
    rc::Rc,
    sync::{Arc, mpsc},
};

/// Callback code must remain alive for the embedded server's lifetime.
/// The default stdio server has no such bindings. This is trusted native code,
/// not an in-process sandbox or an authority supplied by a source declaration.
#[derive(Clone, Default)]
pub struct NativeHosts {
    functions: BTreeMap<String, I64HostFunction>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidName,
    DuplicateName,
    BindingLimit,
    OutsideNativeCall,
    CaptureLimit,
    MissingBoundary,
    Interrupted,
    Disconnected,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "mixed debug host: {self:?}")
    }
}
impl std::error::Error for Error {}
impl NativeHosts {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, name: &str, callback: I64HostFunction) -> Result<(), Error> {
        if name.is_empty() || name.len() > 4096 || name.chars().any(char::is_control) {
            return Err(Error::InvalidName);
        }
        if self.functions.contains_key(name) {
            return Err(Error::DuplicateName);
        }
        if self.functions.len() >= 256 {
            return Err(Error::BindingLimit);
        }
        self.functions.insert(name.into(), callback);
        Ok(())
    }
    pub(super) fn get(&self, name: &str) -> Option<I64HostFunction> {
        self.functions.get(name).copied()
    }
    pub(super) fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }
}

#[derive(Clone)]
pub(super) struct PhysicalFrame {
    pub address: usize,
}
pub(super) type Commands = Rc<RefCell<mpsc::Receiver<Command>>>;
pub(super) struct Scope {
    callback: I64HostFunction,
    point: DebugPoint,
    stack: Vec<StackFrame>,
    commands: Commands,
    events: mpsc::Sender<Event>,
    interruption: Arc<InterruptionFlag>,
    baseline: RefCell<Vec<usize>>,
    active: Cell<bool>,
    pending: RefCell<Option<Run>>,
    failure: Cell<Option<Error>>,
}
thread_local! {
    static CURRENT: RefCell<Option<Rc<Scope>>> = const { RefCell::new(None) };
    static ORPHAN: Cell<bool> = const { Cell::new(false) };
}
pub(super) struct Guard(Option<Rc<Scope>>);
impl Drop for Guard {
    fn drop(&mut self) {
        CURRENT.with(|slot| *slot.borrow_mut() = self.0.take());
    }
}
pub(super) fn enter(scope: Rc<Scope>) -> Guard {
    Guard(CURRENT.with(|slot| slot.borrow_mut().replace(scope)))
}
pub(super) fn orphan_failure() -> bool {
    ORPHAN.with(|flag| flag.replace(false))
}
fn capture() -> Result<Vec<usize>, Error> {
    let mut frames = Vec::new();
    backtrace::trace(|frame| {
        frames.push(frame.ip() as usize);
        frames.len() <= 256
    });
    if frames.len() > 256 || frames.contains(&0) {
        return Err(Error::CaptureLimit);
    }
    Ok(frames)
}
impl Scope {
    pub(super) fn new(
        callback: I64HostFunction,
        point: DebugPoint,
        stack: Vec<StackFrame>,
        commands: Commands,
        events: mpsc::Sender<Event>,
        interruption: Arc<InterruptionFlag>,
    ) -> Rc<Self> {
        Rc::new(Self {
            callback,
            point,
            stack,
            commands,
            events,
            interruption,
            baseline: RefCell::new(Vec::new()),
            active: Cell::new(false),
            pending: RefCell::new(None),
            failure: Cell::new(None),
        })
    }
    fn fail(&self, error: Error) -> Error {
        if self.failure.get().is_none() {
            self.failure.set(Some(error));
        }
        self.interruption.interrupt();
        error
    }
    pub(super) fn take_run(&self) -> Option<Run> {
        self.pending.borrow_mut().take()
    }
    pub(super) fn failure(&self) -> Option<Error> {
        self.failure.get()
    }
    fn pause(&self) -> Result<(), Error> {
        if !self.active.get() {
            return Err(Error::OutsideNativeCall);
        }
        if self.interruption.is_interrupted() {
            return Err(self.fail(Error::Interrupted));
        }
        let live = capture().map_err(|e| self.fail(e))?;
        let baseline = self.baseline.borrow();
        let shared = baseline
            .iter()
            .rev()
            .zip(live.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        if shared < 2 || shared == live.len() || shared == baseline.len() {
            return Err(self.fail(Error::MissingBoundary));
        }
        let native_count = live.len() - shared;
        if native_count.saturating_add(self.stack.len()) > 256 {
            return Err(self.fail(Error::CaptureLimit));
        }
        let physical = live[..native_count]
            .iter()
            .map(|address| PhysicalFrame { address: *address })
            .collect();
        drop(baseline);
        if self.interruption.is_interrupted() {
            return Err(self.fail(Error::Interrupted));
        }
        self.events
            .send(Event::Paused {
                point: self.point,
                stack: self.stack.clone(),
                reason: StopReason::Native,
                physical,
            })
            .map_err(|_| self.fail(Error::Disconnected))?;
        loop {
            if self.interruption.is_interrupted() {
                return Err(self.fail(Error::Interrupted));
            }
            let command = self
                .commands
                .borrow()
                .recv()
                .map_err(|_| self.fail(Error::Disconnected))?;
            match command {
                Command::Run(run) => {
                    if self.interruption.is_interrupted() {
                        return Err(self.fail(Error::Interrupted));
                    }
                    *self.pending.borrow_mut() = Some(run);
                    return Ok(());
                }
                Command::Inspect { reply, .. } => {
                    let _ = reply.send(Err(
                        "value inspection is unavailable inside a native callback".into(),
                    ));
                }
            }
        }
    }
}

/// Cooperatively stop the current explicitly granted callback. The embedding
/// host must handle interruption and return promptly; guest code gets no hook.
/// Continues the same native call after the debugger resumes it.
pub fn pause() -> Result<(), Error> {
    let scope = CURRENT
        .with(|slot| slot.borrow().clone())
        .ok_or(Error::OutsideNativeCall)?;
    scope.pause()
}

// A scoped cleanup action after the actual native call prevents a tail call
// from erasing this boundary, and expires its live receipt on every return.
#[inline(never)]
pub(super) extern "C" fn dispatch(argument: i64) -> i64 {
    let Some(scope) = CURRENT.with(|slot| slot.borrow().clone()) else {
        ORPHAN.with(|flag| flag.set(true));
        return 0;
    };
    if scope.interruption.is_interrupted() {
        scope.fail(Error::Interrupted);
        return 0;
    }
    let frames = match capture() {
        Ok(frames) => frames,
        Err(error) => {
            scope.fail(error);
            return 0;
        }
    };
    if scope.interruption.is_interrupted() {
        scope.fail(Error::Interrupted);
        return 0;
    }
    *scope.baseline.borrow_mut() = frames;
    scope.active.set(true);
    struct Active(Rc<Scope>);
    impl Drop for Active {
        fn drop(&mut self) {
            self.0.active.set(false);
            self.0.baseline.borrow_mut().clear();
        }
    }
    let _active = Active(scope.clone());
    (scope.callback)(argument)
}
