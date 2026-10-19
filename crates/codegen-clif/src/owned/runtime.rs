//! Concrete ordinary ownership entry and the shared sealed frame-program view.
use super::*;
mod sealed {
    pub trait Sealed {}
}
/// Prepared layout/usage view used by production managed primitives.
/// This backend-only trait is sealed and grants no execution authority.
pub trait FrameProgram<'a>: sealed::Sealed {
    fn frame_layout(&self, function: m::FunctionId) -> Option<&FunctionLayout<'a>>;
    fn frame_usage(&self) -> m::Usage;
}
/// Ordinary ownership code owners that invoke the original native continuations.
pub trait EntryProgram<'a>: FrameProgram<'a> {
    fn invoke_entry<R: Runtime>(
        &self,
        function: m::FunctionId,
        runtime: &mut RuntimeInstance<R>,
    ) -> Result<(), Failure>;
}
impl sealed::Sealed for CompiledProgram<'_> {}
impl<'a> FrameProgram<'a> for CompiledProgram<'a> {
    fn frame_layout(&self, function: m::FunctionId) -> Option<&FunctionLayout<'a>> {
        self.layout(function)
    }
    fn frame_usage(&self) -> m::Usage {
        self.usage()
    }
}
impl<'a> EntryProgram<'a> for CompiledProgram<'a> {
    fn invoke_entry<R: Runtime>(
        &self,
        function: m::FunctionId,
        runtime: &mut RuntimeInstance<R>,
    ) -> Result<(), Failure> {
        self.invoke(function, runtime)
    }
}
impl<Image> sealed::Sealed for LinkedProgram<'_, Image> {}
impl<'a, Image> FrameProgram<'a> for LinkedProgram<'a, Image> {
    fn frame_layout(&self, function: m::FunctionId) -> Option<&FunctionLayout<'a>> {
        self.layout(function)
    }
    fn frame_usage(&self) -> m::Usage {
        self.usage()
    }
}
impl<'a, Image> EntryProgram<'a> for LinkedProgram<'a, Image> {
    fn invoke_entry<R: Runtime>(
        &self,
        function: m::FunctionId,
        runtime: &mut RuntimeInstance<R>,
    ) -> Result<(), Failure> {
        self.invoke(function, runtime)
    }
}
impl sealed::Sealed for CCompiledProgram<'_> {}
impl<'a> FrameProgram<'a> for CCompiledProgram<'a> {
    fn frame_layout(&self, function: m::FunctionId) -> Option<&FunctionLayout<'a>> {
        <Self as NativeProgram<'a>>::layout(self, function)
    }
    fn frame_usage(&self) -> m::Usage {
        <Self as NativeProgram<'a>>::usage(self)
    }
}
impl<Image> sealed::Sealed for CLinkedProgram<'_, Image> {}
impl<'a, Image> FrameProgram<'a> for CLinkedProgram<'a, Image> {
    fn frame_layout(&self, function: m::FunctionId) -> Option<&FunctionLayout<'a>> {
        <Self as NativeProgram<'a>>::layout(self, function)
    }
    fn frame_usage(&self) -> m::Usage {
        <Self as NativeProgram<'a>>::usage(self)
    }
}

use lang_runtime::{allocation as a, cleanup as c, execution as e, layout as l};

#[derive(Debug)]
pub enum PreparationError {
    Capacity,
    Control(e::Failure),
    ServiceWork(e::UnsupportedServiceWork),
    Resource(m::Error),
    Layout(l::Error),
    Shared(lang_runtime::shared::managed::Error),
    Arena(lang_runtime::arena::managed::Error),
    Inspection(lang_runtime::inspection::Error),
    Retirement(lang_runtime::retirement::Error),
    Instance(c::EnterError),
}
impl std::fmt::Display for PreparationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "owned runtime preparation failed: {self:?}")
    }
}
impl std::error::Error for PreparationError {}
/// Ordinary entry requires finite logical-frame/cell limits. Defaults are
/// trusted embedding policy, not serialized guest authority.
#[derive(Clone, Copy, Debug)]
pub struct Config<'a> {
    pub control: e::Control<'a>,
    pub limits: m::Limits,
    pub cleanup_records: usize,
}
impl Default for Config<'_> {
    fn default() -> Self {
        Self {
            control: e::Control::with_limits(e::Limits {
                fuel: None,
                host_calls: Some(0),
                frames: Some(64),
                live_cells: Some(65536),
            }),
            limits: m::Limits::default(),
            cleanup_records: c::Limits::default().failure_records,
        }
    }
}
#[derive(Debug)]
pub struct Outcome {
    pub value: i64,
    pub usage: e::Usage,
    pub report: c::Report,
}
#[derive(Debug)]
pub enum ErrorKind {
    Entry,
    Poisoned,
    CInterface,
    Preparation(PreparationError),
    Admission(c::ExecutionFailure),
    Native(super::Failure),
    Cleanup,
    HostPanic,
    Invariant,
}
#[derive(Debug)]
pub struct EntryError {
    pub kind: ErrorKind,
    pub usage: e::Usage,
    pub report: Option<c::Report>,
    pub stack: Vec<(m::FunctionId, m::Span)>,
    /// Supplemental host-storage completion failure after the guest report.
    pub cleanup: Option<lang_runtime::c_abi::context::Fault>,
    pub function: Option<m::FunctionId>,
    pub fault: Option<m::Span>,
    pub panic_code: Option<i64>,
}
impl std::fmt::Display for EntryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "owned native entry failed: {:?}", self.kind)
    }
}
impl std::error::Error for EntryError {}

/// Execute an ordinary zero-parameter i64 entry through actual native code.
/// The borrowed program owns its executable code; no C authority is created.
#[allow(
    clippy::result_large_err,
    reason = "Retain original failure and cleanup report without a fallible error allocation."
)]
pub fn execute_entry<'a, 'l: 'a, P: a::Provider, S: EntryProgram<'l>>(
    program: &'l S,
    entry: m::FunctionId,
    instance: &'a mut c::Instance,
    allocator: &'a mut a::Allocator<P>,
    config: Config<'a>,
) -> Result<Outcome, EntryError> {
    crate::c_abi::runtime::source::execute_entry(program, entry, instance, allocator, config)
}
