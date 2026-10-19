use super::config::InvalidConfig;
use crate::c_abi::validation;
use crate::cleanup::{FailureKind, Report};
use crate::execution;
use lang_mir::unit::{Span, ownership::failure::ExecutionFailure};
use std::fmt::{Display, Formatter};

/// Converted explicitly to C u32. This Rust enum is never a wire layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Success,
    Invalid,
    GuestPanic,
    Runtime,
    Cleanup,
    Capability,
    Poisoned,
}
impl Status {
    pub const fn code(self) -> u32 {
        match self {
            Self::Success => 0,
            Self::Invalid => 1,
            Self::GuestPanic => 2,
            Self::Runtime => 3,
            Self::Cleanup => 4,
            Self::Capability => 5,
            Self::Poisoned => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Config(InvalidConfig),
    WrongProgram,
    Reentry,
    Closed,
    Poisoned,
    Function(usize),
    Value(validation::Error),
    ForeignValue(validation::Error),
    ForeignStatus { code: u32, known: bool },
    Control(execution::Failure),
    Execution(ExecutionFailure),
    Cleanup,
    HostPanic,
    ResultShape,
    Capacity,
    Allocation,
    IdentityExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Fault {
    pub kind: Kind,
    pub span: Option<Span>,
    pub panic_code: i64,
    pub cleanup_count: u32,
    pub truncated: bool,
    pub quarantined: usize,
    poison: bool,
}
impl Fault {
    pub const fn new(kind: Kind) -> Self {
        Self {
            kind,
            span: None,
            panic_code: 0,
            cleanup_count: if matches!(kind, Kind::Cleanup) { 1 } else { 0 },
            truncated: false,
            quarantined: 0,
            poison: matches!(
                kind,
                Kind::HostPanic
                    | Kind::Cleanup
                    | Kind::Execution(ExecutionFailure::HostPanic | ExecutionFailure::Invariant)
            ),
        }
    }
    pub const fn at(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
    pub const fn guest_panic(code: i64, span: Option<Span>) -> Self {
        Self {
            panic_code: code,
            span,
            ..Self::new(Kind::Execution(ExecutionFailure::Panic))
        }
    }
    pub const fn status(self) -> Status {
        match self.kind {
            Kind::Config(InvalidConfig::Grant(_)) => Status::Capability,
            Kind::Config(_)
            | Kind::WrongProgram
            | Kind::Reentry
            | Kind::Closed
            | Kind::Function(_)
            | Kind::ResultShape => Status::Invalid,
            Kind::Value(validation::Error::Control(_)) => Status::Runtime,
            Kind::Value(_) => Status::Invalid,
            Kind::Execution(ExecutionFailure::Panic) => Status::GuestPanic,
            Kind::Cleanup => Status::Cleanup,
            Kind::Poisoned => Status::Poisoned,
            _ => Status::Runtime,
        }
    }
    pub const fn poisons(self) -> bool {
        self.poison || self.cleanup_count != 0 || self.quarantined != 0
    }
    pub fn from_report(report: &Report, panic_code: i64) -> Option<Self> {
        let primary = report.primary()?;
        let kind = match primary.kind {
            FailureKind::Execution(failure) => Kind::Execution(failure),
            _ => Kind::Cleanup,
        };
        let mut fault = Self::new(kind);
        fault.span = primary.span;
        if matches!(kind, Kind::Execution(ExecutionFailure::Panic)) {
            fault.panic_code = panic_code;
        }
        fault.cleanup_count = u32::try_from(report.cleanup_failures()).unwrap_or(u32::MAX);
        fault.truncated = report.truncated() != 0;
        fault.quarantined = report.quarantined();
        Some(fault)
    }
    pub(super) fn with_later_cleanup(mut self, later: Self) -> Self {
        self.cleanup_count = self.cleanup_count.saturating_add(later.cleanup_count);
        self.truncated |= later.truncated;
        self.quarantined = self.quarantined.saturating_add(later.quarantined);
        self.poison |= later.poisons();
        self
    }
    pub(super) fn host_panic(payload: Box<dyn std::any::Any + Send>) -> Self {
        let mut fault = Self::new(Kind::HostPanic);
        // A malicious panic payload can panic in Drop. Never recursively drop
        // the second panic payload; preserve the primary and bound containment.
        if let Err(secondary) =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(payload)))
        {
            std::mem::forget(secondary);
            fault.quarantined = 1;
            fault.cleanup_count = 1;
        }
        fault
    }
}
impl From<validation::Error> for Fault {
    fn from(error: validation::Error) -> Self {
        Self::new(Kind::Value(error))
    }
}
impl From<execution::Failure> for Fault {
    fn from(error: execution::Failure) -> Self {
        Self::new(Kind::Control(error))
    }
}
impl Display for Fault {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "C context failure: {:?}", self.kind)
    }
}
impl std::error::Error for Fault {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            Kind::Value(error) | Kind::ForeignValue(error) => Some(error),
            Kind::Control(error) => Some(error),
            _ => None,
        }
    }
}

/// Integer-only copied C summary. Native wrappers validate out storage before
/// copying; this type contains no borrowed diagnostic data or Rust enum.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ErrorSummary {
    pub panic_code: i64,
    pub status: u32,
    pub source: u32,
    pub start: u32,
    pub end: u32,
    pub cleanup_count: u32,
    pub flags: u32,
}
impl ErrorSummary {
    pub const fn success() -> Self {
        Self {
            panic_code: 0,
            status: 0,
            source: u32::MAX,
            start: 0,
            end: 0,
            cleanup_count: 0,
            flags: 0,
        }
    }
    pub fn from_fault(fault: Fault) -> Self {
        let mut summary = Self {
            status: fault.status().code(),
            panic_code: if fault.status() == Status::GuestPanic {
                fault.panic_code
            } else {
                0
            },
            cleanup_count: fault.cleanup_count,
            flags: u32::from(fault.truncated),
            ..Self::success()
        };
        if let Some(span) = fault.span {
            summary.source = span.source.0;
            summary.start = span.start;
            summary.end = span.end;
        }
        summary
    }
}
const _: () =
    assert!(std::mem::size_of::<ErrorSummary>() == 32 && std::mem::align_of::<ErrorSummary>() == 8);
