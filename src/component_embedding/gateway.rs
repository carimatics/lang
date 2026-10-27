//! Component C operation implementations. No API query/table is published yet.
mod admission;
mod callbacks;
#[cfg(test)]
mod failure_tests;
mod native_image;
mod persistent;
mod registration;
mod resources;
mod selection;
use super::{Limits as ModuleLimits, Module, ffi, load};
pub use callbacks::context_call;
use lang_codegen_clif::{c_abi::allocator::ForeignProvider, component};
use lang_mir::unit as m;
use lang_runtime::{
    allocation as a,
    component::{
        CapabilityGrants, execution, grants, native, program,
        serialization::{DigestAlgorithm, VersionedDigest},
        storage, wire,
    },
    execution as e,
    task::runner as r,
};
pub use native_image::module_bind_native;
pub use persistent::{context_resource_keep, context_resource_take};
pub use resources::{resource_drop, resource_publish};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    ptr::{self, NonNull},
    rc::Rc,
};

const OK: u32 = 0;
const INVALID: u32 = 2;
const UNSUPPORTED: u32 = 3;
const BUSY: u32 = 4;
const OOM: u32 = 6;
const MAGIC: u64 = 0x636f_6d70_6777_7631;
struct Data {
    config: ffi::Config,
    bridge: component::Engine,
    allocator: a::Allocator<ForeignProvider<'static>>,
    values: storage::Resources<'static>,
    limits: ModuleLimits,
    inventory: Vec<native::Provider<'static>>,
    registrations: Vec<Rc<Registered>>,
}
struct Metadata {
    magic: u64,
    engine: *mut ffi::Engine,
    handles: Cell<usize>,
    active: Cell<bool>,
    current: RefCell<Option<CurrentCall>>,
    data: NonNull<Data>,
}
struct Loaded {
    engine: NonNull<Metadata>,
    module: Rc<Module>,
    digest: [u8; 32],
    native: Option<Rc<native_image::Lease>>,
}
struct Registered {
    provider: Option<native::Provider<'static>>,
    bindings: Cell<usize>,
    persistent: RefCell<Option<execution::PersistentResources>>,
}
struct ImportLease(Rc<Registered>);
impl Drop for ImportLease {
    fn drop(&mut self) {
        let count = self.0.bindings.get();
        debug_assert!(count > 0);
        self.0.bindings.set(count - 1);
    }
}
struct ProviderHandle {
    engine: NonNull<Metadata>,
    registration: Rc<Registered>,
}
struct InstanceData {
    engine: NonNull<Metadata>,
    module: Rc<Module>,
    state: RefCell<execution::InstanceState>,
    closed: Cell<bool>,
    ceiling: ffi::Limits,
    digest: [u8; 32],
    grants: Vec<registration::Grant>,
    providers: Vec<native::Provider<'static>>,
    leases: RefCell<Vec<ImportLease>>,
    indices: RefCell<Vec<storage::Claim>>,
    // Retire executable ownership after state, values and provider leases.
    native: Option<Rc<native_image::Lease>>,
}
struct InstanceHandle(Rc<InstanceData>);
#[derive(Clone)]
struct CurrentCall {
    instance: Rc<InstanceData>,
    mode: u32,
    limits: ModuleLimits,
    ceiling: ffi::Limits,
}
struct CurrentScope<'a> {
    slot: &'a RefCell<Option<CurrentCall>>,
    previous: Option<CurrentCall>,
}
impl<'a> CurrentScope<'a> {
    fn enter(owner: &'a Metadata, current: CurrentCall) -> Self {
        Self {
            slot: &owner.current,
            previous: owner.current.replace(Some(current)),
        }
    }
}
impl Drop for CurrentScope<'_> {
    fn drop(&mut self) {
        self.slot.replace(self.previous.take());
    }
}
struct Binding {
    instance: Rc<InstanceData>,
    entry: m::FunctionId,
}
struct Control {
    engine: NonNull<Metadata>,
    ceiling: ffi::Limits,
    flag: e::InterruptionFlag,
    used: Cell<bool>,
}
enum ResultBody {
    Value(execution::FrameResult),
    Failure(Rc<r::Failure>),
    Cleanup {
        report: Box<lang_runtime::cleanup::Report>,
        _code: Vec<native::ResourceKind<'static>>,
    },
}
enum ResultOwner {
    Instance(Rc<InstanceData>),
    Provider {
        engine: NonNull<Metadata>,
        registration: Rc<Registered>,
    },
}
impl ResultOwner {
    fn engine(&self) -> &NonNull<Metadata> {
        match self {
            Self::Instance(instance) => &instance.engine,
            Self::Provider { engine, .. } => engine,
        }
    }
}
struct CallResult {
    body: ResultBody,
    usage: e::Usage,
    committed: bool,
    poisoned: bool,
    allocations: a::Stats,
    canonical_indices: u64,
    tasks: lang_runtime::task::Usage,
    report: Option<(wire::Report, Vec<u8>)>,
    // Keep the actual instance/code owner through every result body's drop.
    owner: ResultOwner,
}

fn cleanup_description(
    kind: lang_runtime::cleanup::FailureKind,
) -> Option<(wire::Category, wire::Diagnostic)> {
    use lang_runtime::cleanup::{ExecutionFailure, FailureKind, NativeCleanupKind};
    match kind {
        FailureKind::Cleanup(_) => Some((wire::Category::Destructor, wire::Diagnostic::Destructor)),
        FailureKind::NativeCleanup {
            kind: NativeCleanupKind::Destructor,
            ..
        } => Some((wire::Category::Destructor, wire::Diagnostic::Destructor)),
        FailureKind::NativeCleanup {
            kind: NativeCleanupKind::HostContract,
            ..
        } => Some((wire::Category::HostContract, wire::Diagnostic::HostContract)),
        FailureKind::CleanupPanic
        | FailureKind::PayloadDropPanic
        | FailureKind::PanicPayloadDropPanic
        | FailureKind::Execution(ExecutionFailure::HostPanic | ExecutionFailure::Invariant) => {
            Some((wire::Category::HostContract, wire::Diagnostic::HostContract))
        }
        _ => None,
    }
}
struct FailureCode<'a> {
    digest: [u8; 32],
    mode: u32,
    entry: m::FunctionId,
    functions: &'a [m::ownership::declarations::Function],
}
fn native_cleanup_origin(kind: lang_runtime::cleanup::FailureKind) -> Option<wire::Frame> {
    match kind {
        lang_runtime::cleanup::FailureKind::NativeCleanup {
            origin: Some(origin),
            ..
        } => Some(wire::Frame {
            mode: wire::Mode::Host,
            digest: origin.provider,
            member: origin.member,
            span: None,
        }),
        _ => None,
    }
}
fn project_failure(
    failure: &r::Failure,
    ceiling: ffi::Limits,
    code: FailureCode<'_>,
    allocations: a::Stats,
    canonical_indices: u64,
) -> Option<(wire::Report, Vec<u8>)> {
    let FailureCode {
        digest,
        mode,
        entry,
        functions,
    } = code;
    // Preserve the actual primary; cleanup records never replace it.
    let mut limit = None;
    let mut phase = wire::Phase::Call;
    let (category, diagnostic) = match *failure.reason.primary_cause() {
        r::Reason::ComponentIndices {
            limit: maximum,
            attempted,
        } => {
            limit = Some(wire::LimitDetail {
                kind: wire::ReportLimitKind::CanonicalIndices,
                limit: maximum,
                attempted,
            });
            (wire::Category::Limit, wire::Diagnostic::Limit)
        }
        r::Reason::Panic(_)
        | r::Reason::Execution(lang_mir::unit::ownership::failure::ExecutionFailure::Panic) => {
            (wire::Category::Panic, wire::Diagnostic::Panic)
        }
        r::Reason::Cancelled => (wire::Category::Cancellation, wire::Diagnostic::Cancelled),
        r::Reason::Control(e::Failure::Interrupted | e::Failure::DeadlineExceeded)
        | r::Reason::Layout(lang_runtime::layout::Error::ServiceWork(
            e::Failure::Interrupted | e::Failure::DeadlineExceeded,
        ))
        | r::Reason::Execution(
            lang_mir::unit::ownership::failure::ExecutionFailure::Interrupted
            | lang_mir::unit::ownership::failure::ExecutionFailure::DeadlineExceeded,
        ) => (wire::Category::Interruption, wire::Diagnostic::Interrupted),
        r::Reason::Control(e::Failure::FuelExhausted)
        | r::Reason::Layout(lang_runtime::layout::Error::ServiceWork(e::Failure::FuelExhausted))
        | r::Reason::Execution(lang_mir::unit::ownership::failure::ExecutionFailure::FuelLimit) => {
            limit = Some(wire::LimitDetail {
                kind: wire::ReportLimitKind::Fuel,
                limit: ceiling.fuel,
                attempted: failure.usage.fuel.saturating_add(1),
            });
            (wire::Category::Limit, wire::Diagnostic::Limit)
        }
        r::Reason::Control(e::Failure::HostCallLimit)
        | r::Reason::Layout(lang_runtime::layout::Error::ServiceWork(e::Failure::HostCallLimit))
        | r::Reason::Execution(
            lang_mir::unit::ownership::failure::ExecutionFailure::HostCallLimit,
        ) => {
            limit = Some(wire::LimitDetail {
                kind: wire::ReportLimitKind::HostCalls,
                limit: ceiling.host_calls,
                attempted: failure.usage.host_calls.saturating_add(1),
            });
            (wire::Category::Limit, wire::Diagnostic::Limit)
        }
        r::Reason::Control(e::Failure::ServiceWorkLimit {
            limit: maximum,
            used,
            requested,
        })
        | r::Reason::Layout(lang_runtime::layout::Error::ServiceWork(
            e::Failure::ServiceWorkLimit {
                limit: maximum,
                used,
                requested,
            },
        ))
        | r::Reason::Execution(
            lang_mir::unit::ownership::failure::ExecutionFailure::ServiceWorkLimit {
                limit: maximum,
                used,
                requested,
            },
        ) => {
            limit = Some(wire::LimitDetail {
                kind: wire::ReportLimitKind::ServiceWork,
                limit: maximum,
                attempted: used.saturating_add(requested),
            });
            (wire::Category::Limit, wire::Diagnostic::Limit)
        }
        r::Reason::Resource(m::Error {
            kind:
                m::ErrorKind::Limit {
                    kind,
                    limit: maximum,
                    requested,
                },
            ..
        })
        | r::Reason::Layout(lang_runtime::layout::Error::Resource {
            error:
                m::Error {
                    kind:
                        m::ErrorKind::Limit {
                            kind,
                            limit: maximum,
                            requested,
                        },
                    ..
                },
            ..
        })
        | r::Reason::ComponentFrame(wire::Error {
            kind:
                wire::ErrorKind::Budget(m::ErrorKind::Limit {
                    kind,
                    limit: maximum,
                    requested,
                }),
            ..
        }) => {
            let kind = match kind {
                m::LimitKind::Work => wire::ReportLimitKind::PreparationWork,
                m::LimitKind::Allocations
                | m::LimitKind::SourceBytes
                | m::LimitKind::TotalSourceBytes => wire::ReportLimitKind::MetadataBytes,
                m::LimitKind::Depth => wire::ReportLimitKind::Depth,
                _ => wire::ReportLimitKind::ValueElements,
            };
            limit = Some(wire::LimitDetail {
                kind,
                limit: maximum as u64,
                attempted: requested as u64,
            });
            (wire::Category::Limit, wire::Diagnostic::Limit)
        }
        r::Reason::ComponentFrame(wire::Error {
            kind:
                wire::ErrorKind::Limit {
                    kind,
                    limit: maximum,
                    requested,
                },
            ..
        }) => {
            let kind = match kind {
                wire::LimitKind::Bytes | wire::LimitKind::StringBytes => {
                    wire::ReportLimitKind::MetadataBytes
                }
                wire::LimitKind::SerializationBytes => wire::ReportLimitKind::SerializationBytes,
                wire::LimitKind::DecoderAllocationBytes => {
                    wire::ReportLimitKind::DecoderAllocationBytes
                }
                wire::LimitKind::Depth => wire::ReportLimitKind::Depth,
                _ => wire::ReportLimitKind::ValueElements,
            };
            limit = Some(wire::LimitDetail {
                kind,
                limit: maximum as u64,
                attempted: requested as u64,
            });
            (wire::Category::Limit, wire::Diagnostic::Limit)
        }
        r::Reason::Layout(lang_runtime::layout::Error::CellLimit {
            cells,
            limit: maximum,
        }) => {
            limit = Some(wire::LimitDetail {
                kind: wire::ReportLimitKind::LiveCells,
                limit: maximum as u64,
                attempted: cells as u64,
            });
            (wire::Category::Limit, wire::Diagnostic::Limit)
        }
        r::Reason::FrameLimit {
            limit: maximum,
            attempted,
        }
        | r::Reason::CellLimit {
            limit: maximum,
            attempted,
        } => {
            let kind = if matches!(failure.reason, r::Reason::FrameLimit { .. }) {
                wire::ReportLimitKind::Frames
            } else {
                wire::ReportLimitKind::LiveCells
            };
            limit = Some(wire::LimitDetail {
                kind,
                limit: maximum as u64,
                attempted: attempted as u64,
            });
            (wire::Category::Limit, wire::Diagnostic::Limit)
        }
        r::Reason::Schedule(lang_runtime::task::Error::LiveTaskLimit { limit: maximum }) => {
            // The immutable scheduler admits one task at a time and never
            // exceeds this bound; the refused reservation is exactly cap + 1.
            limit = Some(wire::LimitDetail {
                kind: wire::ReportLimitKind::LiveTasks,
                limit: maximum as u64,
                attempted: (maximum as u64).saturating_add(1),
            });
            (wire::Category::Limit, wire::Diagnostic::Limit)
        }
        r::Reason::Layout(lang_runtime::layout::Error::Allocation(a::Error::Limit(violation))) => {
            let kind = match violation.kind {
                a::Limit::RequestBytes | a::Limit::LiveBytes => {
                    Some(wire::ReportLimitKind::HeapBytes)
                }
                a::Limit::LiveAllocations => Some(wire::ReportLimitKind::LiveAllocations),
                // These are internal provider-profile guards, not additional
                // public live-heap/count quotas. The raw failure keeps its kind.
                a::Limit::CumulativeBytes | a::Limit::Alignment => None,
            };
            if let Some(kind) = kind {
                limit = Some(wire::LimitDetail {
                    kind,
                    limit: violation.limit,
                    attempted: violation.attempted,
                });
                (wire::Category::Limit, wire::Diagnostic::Limit)
            } else {
                (wire::Category::HostContract, wire::Diagnostic::HostContract)
            }
        }
        r::Reason::Execution(
            lang_mir::unit::ownership::failure::ExecutionFailure::Fault
            | lang_mir::unit::ownership::failure::ExecutionFailure::HostCall
            | lang_mir::unit::ownership::failure::ExecutionFailure::CounterOverflow,
        )
        | r::Reason::Control(e::Failure::CounterOverflow)
        | r::Reason::ChannelDeadlock(_)
        | r::Reason::Layout(
            lang_runtime::layout::Error::Overflow
            | lang_runtime::layout::Error::ServiceWork(e::Failure::CounterOverflow)
            | lang_runtime::layout::Error::Allocation(a::Error::OutOfMemory | a::Error::Overflow),
        )
        | r::Reason::Resource(m::Error {
            kind: m::ErrorKind::AllocationFailed | m::ErrorKind::ArithmeticOverflow,
            ..
        })
        | r::Reason::Layout(lang_runtime::layout::Error::Resource {
            error:
                m::Error {
                    kind: m::ErrorKind::AllocationFailed | m::ErrorKind::ArithmeticOverflow,
                    ..
                },
            ..
        })
        | r::Reason::ComponentFrame(wire::Error {
            kind:
                wire::ErrorKind::Budget(
                    m::ErrorKind::AllocationFailed | m::ErrorKind::ArithmeticOverflow,
                ),
            ..
        }) => (wire::Category::Trap, wire::Diagnostic::Trap),
        r::Reason::Execution(
            lang_mir::unit::ownership::failure::ExecutionFailure::HostPanic
            | lang_mir::unit::ownership::failure::ExecutionFailure::Invariant,
        ) => (wire::Category::HostContract, wire::Diagnostic::HostContract),
        r::Reason::Instance(execution::InstanceError::Poisoned) => {
            phase = wire::Phase::Admission;
            (
                wire::Category::Admission,
                wire::Diagnostic::InstancePoisoned,
            )
        }
        r::Reason::Instance(execution::InstanceError::Active) => {
            phase = wire::Phase::Admission;
            (wire::Category::Admission, wire::Diagnostic::Reentry)
        }
        r::Reason::Native(
            execution::NativeFailure::Report(_)
            | execution::NativeFailure::Latched(_)
            | execution::NativeFailure::Propagated { .. },
        ) => (wire::Category::Trap, wire::Diagnostic::Trap),
        r::Reason::Native(_)
        | r::Reason::Invariant
        | r::Reason::Instance(execution::InstanceError::ScopeNotClosed) => {
            (wire::Category::HostContract, wire::Diagnostic::HostContract)
        }
        r::Reason::Cleanup => match failure
            .report
            .as_ref()
            .and_then(|report| report.primary())
            .map(|r| r.kind)
        {
            Some(
                lang_runtime::cleanup::FailureKind::Cleanup(_)
                | lang_runtime::cleanup::FailureKind::NativeCleanup {
                    kind: lang_runtime::cleanup::NativeCleanupKind::Destructor,
                    ..
                },
            ) => {
                phase = wire::Phase::Cleanup;
                (wire::Category::Destructor, wire::Diagnostic::Destructor)
            }
            Some(
                lang_runtime::cleanup::FailureKind::CleanupPanic
                | lang_runtime::cleanup::FailureKind::PayloadDropPanic
                | lang_runtime::cleanup::FailureKind::PanicPayloadDropPanic
                | lang_runtime::cleanup::FailureKind::Execution(
                    lang_runtime::cleanup::ExecutionFailure::HostPanic
                    | lang_runtime::cleanup::ExecutionFailure::Invariant,
                ),
            ) => {
                phase = wire::Phase::Cleanup;
                (wire::Category::HostContract, wire::Diagnostic::HostContract)
            }
            _ => {
                phase = wire::Phase::Cleanup;
                (wire::Category::HostContract, wire::Diagnostic::HostContract)
            }
        },
        // Invalid verified state, missing boundary evidence or an internal
        // profile guard is a contract failure. Preserve the raw reason in the
        // result and publish the mandatory typed envelope instead of None.
        _ => (wire::Category::HostContract, wire::Diagnostic::HostContract),
    };
    let frame = wire::Frame {
        mode: match mode {
            1 => wire::Mode::Jit,
            2 => wire::Mode::Aot,
            _ => wire::Mode::Vm,
        },
        digest,
        member: failure.function.unwrap_or(entry).0,
        span: failure.span.or_else(|| {
            functions
                .get(failure.function.unwrap_or(entry).0 as usize)
                .map(|function| function.signature_span)
        }),
    };
    let native_frame = failure.reason.native_frame();
    let repeated = match (failure.reason.without_native_frames(), native_frame) {
        (r::Reason::Native(execution::NativeFailure::Propagated { failure, .. }), Some(frame)) => {
            failure
                .trace_frame()
                .is_some_and(|original| original.same_activation(frame))
        }
        _ => false,
    };
    let mut frames = Vec::new();
    let caller_trace = native_frame
        .filter(|native| !native.callers().is_empty())
        .map(|native| (native.callers(), native.omitted_callers()))
        .or_else(|| {
            failure
                .callers
                .as_ref()
                .map(|callers| (callers.frames.as_slice(), callers.omitted))
        });
    let mut omitted_frames = if repeated {
        0
    } else {
        caller_trace.map_or(0, |(_, omitted)| omitted)
    };
    if !repeated {
        let mut push = |frame: wire::Frame| {
            if frames.len() < ceiling.trace_frames.min(64) as usize {
                frames.push(frame);
            } else {
                omitted_frames = omitted_frames.saturating_add(1);
            }
        };
        if let Some((callers, _)) = caller_trace {
            for caller in callers {
                push(wire::Frame {
                    member: caller.function.0,
                    span: Some(caller.span),
                    ..frame.clone()
                });
            }
        } else {
            push(frame.clone());
        }
        if let Some(native) = native_frame {
            push(native.origin().clone());
        }
    }
    let mut omitted = failure
        .report
        .as_ref()
        .map_or(0, |r| r.truncated() as u64)
        .saturating_add(omitted_frames);
    let mut origin_work = 4096usize;
    let mut cleanup_origin = |span: m::Span| {
        if Some(span) == failure.span {
            return Some(frame.clone());
        }
        let mut member = None;
        for (index, function) in functions.iter().enumerate() {
            if origin_work == 0 {
                return None;
            }
            origin_work -= 1;
            if function.span.source == span.source
                && function.span.start <= span.start
                && span.end <= function.span.end
            {
                if member.is_some() {
                    return None;
                }
                member = u32::try_from(index).ok();
            }
        }
        member.map(|member| wire::Frame {
            member,
            span: Some(span),
            ..frame.clone()
        })
    };
    let primary_origin = if matches!(failure.reason.primary_cause(), r::Reason::Cleanup) {
        match failure.report.as_ref().and_then(|report| report.primary()) {
            Some(record)
                if matches!(
                    record.kind,
                    lang_runtime::cleanup::FailureKind::NativeCleanup { .. }
                ) =>
            {
                native_cleanup_origin(record.kind)
            }
            Some(record) if cleanup_description(record.kind).is_some() => record
                .span
                .map_or_else(|| Some(frame.clone()), &mut cleanup_origin),
            _ => Some(frame.clone()),
        }
    } else {
        Some(frame.clone())
    };
    let mut suppressed = Vec::new();
    if let Some(cleanup) = &failure.report {
        let maximum = ceiling.suppressed_records.min(64) as usize;
        for record in cleanup.secondary() {
            let description = cleanup_description(record.kind);
            if let Some((category, diagnostic)) = description
                && suppressed.len() < maximum
            {
                suppressed.push(wire::Record {
                    category,
                    phase: wire::Phase::Cleanup,
                    diagnostic,
                    message: None,
                    limit: None,
                    origin: if matches!(
                        record.kind,
                        lang_runtime::cleanup::FailureKind::NativeCleanup { .. }
                    ) {
                        native_cleanup_origin(record.kind)
                    } else {
                        record.span.and_then(&mut cleanup_origin)
                    },
                });
            } else {
                omitted = omitted.saturating_add(1);
            }
        }
    }
    let mut report = wire::Report {
        policy: wire::MessagePolicy::Redacted,
        primary: wire::Record {
            category,
            phase,
            diagnostic,
            message: None,
            limit,
            origin: primary_origin,
        },
        suppressed,
        poisoned: failure.poisoned,
        omitted,
        usage: wire::ReportUsage {
            fuel: failure.usage.fuel,
            service_work: failure.usage.service_work,
            host_calls: failure.usage.host_calls,
            peak_heap_bytes: allocations.peak_live_bytes,
            peak_live_allocations: allocations.peak_live_allocations as u64,
            peak_frames: failure.usage.peak_frames as u64,
            peak_live_cells: failure.usage.peak_live_cells as u64,
            peak_live_tasks: failure.tasks.peak_live as u64,
            canonical_indices: match failure.reason.without_native_frames() {
                r::Reason::Native(execution::NativeFailure::Context {
                    canonical_indices, ..
                }) => *canonical_indices,
                _ => canonical_indices,
            },
        },
        frames,
    };
    let native = match failure.reason.without_native_frames() {
        r::Reason::Native(execution::NativeFailure::Report(native)) => Some(native.report()),
        r::Reason::Native(execution::NativeFailure::Latched(native)) => Some(native.report()),
        r::Reason::Native(execution::NativeFailure::Propagated { failure, .. }) => {
            Some(failure.report())
        }
        _ => None,
    };
    if let Some(native) = native {
        // Foreign bytes were admitted and attributed on the original Session.
        // This mandatory projection only caps already bounded immutable data.
        let mut native = native.clone();
        let maximum = ceiling.suppressed_records.min(64) as usize;
        // Canonical indices are scoped locators. A parent namespace cannot
        // overwrite the immutable observed prefix of the failed child scope.
        let failed_indices = native.usage.canonical_indices;
        native.usage = report.usage;
        native.usage.canonical_indices = failed_indices;
        native.poisoned |= report.poisoned;
        native.omitted = native.omitted.saturating_add(report.omitted);
        native.omitted = native
            .omitted
            .saturating_add(native.suppressed.len().saturating_sub(maximum) as u64);
        native.suppressed.truncate(maximum);
        for record in report.suppressed {
            if native.suppressed.len() < maximum {
                native.suppressed.push(record);
            } else {
                native.omitted = native.omitted.saturating_add(1);
            }
        }
        report.frames.append(&mut native.frames);
        let traces = ceiling.trace_frames.min(64) as usize;
        native.omitted = native
            .omitted
            .saturating_add(report.frames.len().saturating_sub(traces) as u64);
        report.frames.truncate(traces);
        native.frames = report.frames;
        report = native;
    }
    let requires_poison = |record: &wire::Record| {
        matches!(
            record.category,
            wire::Category::Destructor | wire::Category::HostContract
        ) || record.diagnostic == wire::Diagnostic::InstancePoisoned
            || (record.category == wire::Category::Panic
                && record
                    .origin
                    .as_ref()
                    .is_some_and(|frame| frame.mode == wire::Mode::Host))
    };
    report.poisoned |=
        requires_poison(&report.primary) || report.suppressed.iter().any(requires_poison);
    // Mandatory projection is outside guest control charging, bounded to this
    // fixed primary/top-frame representation. It cannot refund/reset a root.
    let mut session = wire::Session::new(wire::Limits {
        bytes: 16384,
        serialization_bytes: 16384,
        allocation_bytes: 65536,
        work: 65536,
        elements: 1024,
        ..Default::default()
    });
    let bytes = wire::encode_report(&report, &mut session).ok()?;
    Some((report, bytes))
}
macro_rules! checked_context_operation {
    ($name:ident, $target:ident, ($($argument:ident: $kind:ty),* $(,)?) ) => {
        /// # Safety
        /// Context, outputs and initialized error storage obey the exact native
        /// callback contract and remain valid for this complete operation.
        pub unsafe extern "C" fn $name($($argument: $kind,)* error: *mut ffi::Error) -> u32 {
            // SAFETY: the caller retains the initialized native error object.
            if !unsafe { error_ready(error) } { return INVALID; }
            // SAFETY: the original bridge validates header/thread/invocation/
            // scope/control before restoring any borrowed native capability.
            match unsafe { component::$target($($argument),*) } {
                status @ 0..=6 => status,
                12 | 17 => OOM,
                _ => INVALID,
            }
        }
    };
}
checked_context_operation!(context_check, context_check, (context: *const ffi::Context));
checked_context_operation!(context_resource_write, context_resource_write,
    (context: *const ffi::Context, index: u32, output: *mut ffi::MutBytes));
checked_context_operation!(context_resource_read, context_resource_read,
    (context: *const ffi::Context, index: u32, output: *mut ffi::Bytes));
checked_context_operation!(context_resource_publish, context_resource_publish,
    (context: *const ffi::Context, key: ffi::Bytes, buffer: *mut ffi::Buffer, output: *mut u32));
checked_context_operation!(context_buffer_create, context_buffer_create,
    (context: *const ffi::Context, length: u64, alignment: u64, output: *mut *mut ffi::Buffer));
checked_context_operation!(context_buffer_drop, context_buffer_drop,
    (context: *const ffi::Context, buffer: *mut ffi::Buffer));
checked_context_operation!(buffer_mut_bytes, buffer_mut_bytes,
    (context: *const ffi::Context, buffer: *mut ffi::Buffer, output: *mut ffi::MutBytes));

struct Active<'a>(&'a Cell<bool>);
impl Drop for Active<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
struct ResultReservation<'a> {
    owner: &'a Metadata,
    held: bool,
}
impl<'a> ResultReservation<'a> {
    fn new(owner: &'a Metadata) -> Result<Self, u32> {
        retain(owner)?;
        Ok(Self { owner, held: true })
    }
    fn commit(mut self) {
        self.held = false;
    }
}
impl Drop for ResultReservation<'_> {
    fn drop(&mut self) {
        if self.held {
            release(self.owner).expect("the unexposed result retains its handle reservation");
        }
    }
}
fn bounds(a: ffi::Limits, b: ffi::Limits) -> ffi::Limits {
    macro_rules! merge { ($($name:ident),* $(,)?) => { ffi::Limits {
        header:a.header,reserved:[0;2],$($name:a.$name.min(b.$name)),*
    } }; }
    merge!(
        fuel,
        service_work,
        host_calls,
        heap_bytes,
        live_allocations,
        frames,
        live_cells,
        live_tasks,
        canonical_indices,
        preparation_work,
        metadata_bytes,
        value_elements,
        serialization_bytes,
        depth,
        decoder_allocation_bytes,
        suppressed_records,
        trace_frames
    )
}
unsafe fn owner(reference: &NonNull<Metadata>) -> Result<&Metadata, u32> {
    // SAFETY: a live dependent keeps metadata alive. Read only immutable engine
    // identity until its original bridge verifies the calling owner thread.
    let handle = unsafe { reference.as_ref().engine };
    // SAFETY: this exact engine remains retained by that dependent handle.
    let checked = unsafe { metadata(handle) }?;
    if checked != *reference {
        return Err(INVALID);
    }
    // SAFETY: callers keep this borrow within the current native operation.
    Ok(unsafe { &*reference.as_ptr() })
}
fn retain(owner: &Metadata) -> Result<(), u32> {
    owner
        .handles
        .set(owner.handles.get().checked_add(1).ok_or(OOM)?);
    Ok(())
}
fn release(owner: &Metadata) -> Result<(), u32> {
    owner
        .handles
        .set(owner.handles.get().checked_sub(1).ok_or(INVALID)?);
    Ok(())
}
fn meter(
    s: &mut wire::Session,
    limits: ModuleLimits,
    work: usize,
    bytes: usize,
    elements: usize,
) -> Result<(), u32> {
    let mut budget =
        m::Budget::with_usage(limits.preparation, s.preparation_usage()).map_err(|_| INVALID)?;
    budget.charge(work).map_err(|_| INVALID)?;
    budget.bytes(bytes).map_err(|_| INVALID)?;
    budget.allocate::<()>(elements).map_err(|_| INVALID)?;
    *s = wire::Session::with_usage(
        limits.metadata,
        m::Usage {
            work: budget.work_used(),
            allocation_bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        },
    )
    .map_err(|_| INVALID)?;
    Ok(())
}
unsafe fn header<T>(value: *const T) -> bool {
    if value.is_null() {
        return false;
    }
    // SAFETY: callers supply an initialized readable object of the exact C
    // type; its first named fields are always the fixed size/version header.
    let header = unsafe { value.cast::<ffi::Header>().read() };
    header.size == std::mem::size_of::<T>() as u32 && header.version == 1
}
unsafe fn error_ready(error: *mut ffi::Error) -> bool {
    // SAFETY: the caller supplies a readable initialized error slot.
    unsafe { header(error) }
}
unsafe fn metadata(engine: *mut ffi::Engine) -> Result<NonNull<Metadata>, u32> {
    // SAFETY: caller supplies this gateway's exact live opaque engine. The
    // bridge checks owner thread before this separate metadata is followed.
    let owner = unsafe { component::gateway_owner(engine) }?;
    let owner = owner.cast::<Metadata>();
    // SAFETY: gateway creation attached precisely this stable live Metadata.
    if unsafe { owner.as_ref().magic } != MAGIC {
        return Err(INVALID);
    }
    Ok(owner)
}
unsafe fn bytes<'a>(view: ffi::Bytes) -> Result<&'a [u8], u32> {
    let length = usize::try_from(view.length).map_err(|_| INVALID)?;
    if length > isize::MAX as usize || (length != 0 && view.data.is_null()) {
        return Err(INVALID);
    }
    if length == 0 {
        return Ok(&[]);
    }
    // SAFETY: native callers retain the entire readable range through return.
    // The module loader snapshots every byte it retains.
    Ok(unsafe { std::slice::from_raw_parts(view.data, length) })
}
fn prepare(config: ffi::Config) -> Result<Box<Data>, u32> {
    if config.flags & !1 != 0
        || config.reserved != [0; 2]
        || config.allocator.reserved != [0; 2]
        || config.ceiling.reserved != [0; 2]
    {
        return Err(INVALID);
    }
    if config.mode > 2 {
        return Err(INVALID);
    }
    let (Some(allocate), Some(deallocate)) =
        (config.allocator.allocate, config.allocator.deallocate)
    else {
        return Err(INVALID);
    };
    let size = |n| usize::try_from(n).map_err(|_| INVALID);
    let budget_limits = m::Limits {
        work: size(config.ceiling.preparation_work)?,
        allocations: size(config.ceiling.metadata_bytes)?,
        elements: size(config.ceiling.value_elements)?,
        ..Default::default()
    };
    let capacity = size(config.ceiling.canonical_indices)?;
    if capacity > u32::MAX as usize {
        return Err(INVALID);
    }
    // SAFETY: engine_create's native contract retains allocator code/user
    // through engine_destroy and all original regions; no callback is called
    // during this metadata admission. Exact typed callbacks are retained.
    let provider =
        unsafe { ForeignProvider::component(allocate, deallocate, config.allocator.user) };
    let allocator = a::Allocator::new(
        provider,
        a::Limits {
            request_bytes: config.ceiling.heap_bytes,
            live_bytes: config.ceiling.heap_bytes,
            cumulative_bytes: u64::MAX,
            live_allocations: size(config.ceiling.live_allocations)?,
            ..Default::default()
        },
    )
    .map_err(|_| INVALID)?;
    let mut budget = m::Budget::new(budget_limits);
    budget.allocate::<Data>(1).map_err(|_| OOM)?;
    budget.allocate::<Metadata>(1).map_err(|_| OOM)?;
    let values = storage::Resources::prepare(&allocator, capacity, capacity, &mut budget)
        .map_err(|_| OOM)?;
    let bridge = component::Engine::prepare(&mut budget).map_err(|_| OOM)?;
    let mut limits = ModuleLimits {
        input_bytes: size(config.ceiling.metadata_bytes)?,
        preparation: budget_limits,
        ..Default::default()
    };
    limits.metadata.bytes = limits.input_bytes;
    limits.metadata.serialization_bytes = size(config.ceiling.serialization_bytes)?;
    limits.metadata.depth = size(config.ceiling.depth)?;
    limits.metadata.work = limits.metadata.work.min(budget_limits.work);
    limits.metadata.allocation_bytes = limits
        .metadata
        .allocation_bytes
        .min(size(config.ceiling.decoder_allocation_bytes)?);
    limits.metadata.elements = limits.metadata.elements.min(budget_limits.elements);
    Ok(Box::new(Data {
        config,
        bridge,
        allocator,
        values,
        limits,
        inventory: Vec::new(),
        registrations: Vec::new(),
    }))
}

/// # Safety
/// Input/output/error objects are initialized valid C objects. Allocator
/// code/user remains live until successful engine destruction; callbacks obey
/// the unique storage, exact retirement and no-unwind/no-reentry contract.
pub unsafe extern "C" fn engine_create(
    config: *const ffi::Config,
    output: *mut *mut ffi::Engine,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: the caller retains these initialized readable descriptor slots.
    if output.is_null() || !unsafe { error_ready(error) } || !unsafe { header(config) } {
        return INVALID;
    }
    // SAFETY: the exact Config header was checked and caller retains all fields.
    let config = unsafe { config.read() };
    // SAFETY: these nested initialized descriptors are local field copies.
    if !unsafe { header(ptr::from_ref(&config.allocator)) }
        || !unsafe { header(ptr::from_ref(&config.ceiling)) }
    {
        return INVALID;
    }
    let mut data = match prepare(config) {
        Ok(data) => data,
        Err(status) => return status,
    };
    let mut owner = Box::new(Metadata {
        magic: MAGIC,
        engine: data.bridge.native_handle(),
        handles: Cell::new(0),
        active: Cell::new(false),
        current: RefCell::new(None),
        data: NonNull::from(&mut *data),
    });
    let handle = match data
        .bridge
        .attach_gateway(NonNull::from(&mut *owner).cast())
    {
        Ok(handle) => handle,
        Err(status) => return status,
    };
    let _ = Box::into_raw(data);
    let _ = Box::into_raw(owner);
    // SAFETY: success publishes this exact retained engine only at the end.
    unsafe { output.write(handle) };
    OK
}

/// # Safety
/// Engine and error are exact live handles/initialized C objects from this
/// gateway. No dependent handle may be used after successful destruction.
pub unsafe extern "C" fn engine_destroy(engine: *mut ffi::Engine, error: *mut ffi::Error) -> u32 {
    // SAFETY: the caller retains this initialized error slot.
    if !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: engine is the exact gateway handle under the stated contract.
    let owner = match unsafe { metadata(engine) } {
        Ok(owner) => owner,
        Err(status) => return status,
    };
    // SAFETY: metadata lives until this operation's final successful drop.
    let reference = unsafe { owner.as_ref() };
    if reference.active.get() || reference.handles.get() != 0 {
        return BUSY;
    }
    // SAFETY: no active/dependent operation borrows this exact Data allocation.
    let data = unsafe { reference.data.as_ref() };
    if !data.values.is_empty() {
        return BUSY;
    }
    // Keep the original allocator alive until every managed region is gone.
    let _identity = data.allocator.identity();
    // SAFETY: only successful creation published these exact Box allocations;
    // after all dependents close they are uniquely consumed exactly once.
    unsafe {
        drop(Box::from_raw(reference.data.as_ptr()));
        drop(Box::from_raw(owner.as_ptr()));
    }
    OK
}

/// # Safety
/// All native views/output/error objects are valid for the operation; engine
/// is this gateway's live handle. No foreign byte reference is retained.
pub unsafe extern "C" fn module_load(
    engine: *mut ffi::Engine,
    format: u32,
    program: ffi::Bytes,
    contract: ffi::Bytes,
    output: *mut *mut ffi::Module,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: caller retains this initialized error/output storage.
    if output.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: supplied engine must be this gateway's live handle.
    let owner = match unsafe { metadata(engine) } {
        Ok(owner) => owner,
        Err(status) => return status,
    };
    // SAFETY: engine's stable metadata is retained by its live handle.
    let reference = unsafe { owner.as_ref() };
    if reference.active.get() {
        return BUSY;
    }
    // SAFETY: only this owner thread accesses immutable Data preparation limits.
    let data = unsafe { reference.data.as_ref() };
    if data.config.mode > 1 {
        return UNSUPPORTED;
    }
    // SAFETY: input views remain readable for this complete operation.
    let (program, contract) = match (unsafe { bytes(program) }, unsafe { bytes(contract) }) {
        (Ok(p), Ok(c)) => (p, c),
        _ => return INVALID,
    };
    let input_bytes = match program.len().checked_add(contract.len()) {
        Some(bytes) => bytes,
        None => return INVALID,
    };
    let mut hash_budget = m::Budget::new(data.limits.preparation);
    let preflight = if input_bytes > data.limits.input_bytes {
        Some(admission::Failure::limit(
            wire::ReportLimitKind::MetadataBytes,
            data.limits.input_bytes,
            input_bytes,
        ))
    } else {
        hash_budget
            .charge(input_bytes)
            .err()
            .map(|reason| admission::module(&super::Error::Budget(reason), None))
    };
    if let Some(failure) = preflight {
        // SAFETY: the initialized error slot was checked before preparation.
        unsafe { error.write(failure.error) };
        return failure.status;
    }
    // Hash traversal is admitted before reading either complete snapshot. The
    // loader receives only the remaining work; errors do not refund this cost.
    let digest: [u8; 32] = Sha256::digest(program).into();
    let idl_digest: [u8; 32] = Sha256::digest(contract).into();
    let mut load_limits = data.limits;
    load_limits.preparation.work -= input_bytes;
    let module = match load(format, program, contract, load_limits) {
        Ok(module) => module,
        Err(reason) => {
            let failure = admission::module_with_allocation(
                &reason,
                Some((digest, idl_digest)),
                super::common(load_limits).1,
            );
            // SAFETY: fixed output replaces every old field only after refusal.
            unsafe { error.write(failure.error) };
            return failure.status;
        }
    };
    let mut budget = match m::Budget::with_usage(data.limits.preparation, module.usage()) {
        Ok(v) => v,
        Err(reason) => {
            let failure = admission::module(&super::Error::Budget(reason), None);
            // SAFETY: this operation retains the checked initialized slot.
            unsafe { error.write(failure.error) };
            return failure.status;
        }
    };
    if let Err(reason) = budget
        .charge(input_bytes)
        .and_then(|()| budget.allocate::<Loaded>(1))
    {
        let failure = admission::module(&super::Error::Budget(reason), None);
        // SAFETY: admission has not published any module or dependent handle.
        unsafe { error.write(failure.error) };
        return failure.status;
    }
    let Some(count) = reference.handles.get().checked_add(1) else {
        return OOM;
    };
    let module = Box::new(Loaded {
        engine: owner,
        module: Rc::new(module),
        digest,
        native: None,
    });
    reference.handles.set(count);
    // SAFETY: publication follows the entire checked preparation transaction.
    unsafe { output.write(Box::into_raw(module).cast()) };
    OK
}

/// # Safety
/// Exact live engine and initialized configuration/output/error storage are
/// retained through return. Native descriptor arrays/views are readable, and
/// owner/code/user storage satisfies registration::prepare's lifetime contract.
pub unsafe extern "C" fn provider_register(
    engine: *mut ffi::Engine,
    config: *const ffi::ProviderConfig,
    output: *mut *mut ffi::Provider,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: caller retains these initialized descriptor and output objects.
    if output.is_null() || !unsafe { error_ready(error) } || !unsafe { header(config) } {
        return INVALID;
    }
    // SAFETY: the engine is this gateway's exact live owner-thread handle.
    let identity = match unsafe { metadata(engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    // SAFETY: live engine metadata is retained until successful destruction.
    let owner = unsafe { identity.as_ref() };
    if owner.active.get() {
        return BUSY;
    }
    let Some(next_handles) = owner.handles.get().checked_add(1) else {
        return OOM;
    };
    // SAFETY: no active root or callback borrows this original execution data.
    let data = unsafe { &mut *owner.data.as_ptr() };
    // SAFETY: the caller supplies the initialized complete checked C record.
    let config = unsafe { config.read() };
    let prepared = (|| {
        let mut session = wire::Session::new(data.limits.metadata);
        let count = data.inventory.len().checked_add(1).ok_or(OOM)?;
        session.count(count, 0).map_err(|_| INVALID)?;
        let mut inventory = session.vector(count, 0).map_err(|_| INVALID)?;
        let mut registrations = session.vector(count, 0).map_err(|_| INVALID)?;
        for provider in &data.inventory {
            session.charge(1, 0).map_err(|_| INVALID)?;
            inventory.push(provider.clone());
        }
        for registration in &data.registrations {
            session.charge(1, 0).map_err(|_| INVALID)?;
            registrations.push(registration.clone());
        }
        session
            .retained::<ProviderHandle>(1, 0)
            .map_err(|_| INVALID)?;
        let mut registration = session
            .shared(
                || Registered {
                    provider: None,
                    bindings: Cell::new(0),
                    persistent: RefCell::new(None),
                },
                0,
            )
            .map_err(|_| INVALID)?;
        let persistent =
            execution::PersistentReservation::prepare(&mut data.values).map_err(|_| INVALID)?;
        // SAFETY: all descriptor/code/user lifetimes are the caller contract;
        // every fallible publication allocation/counter was admitted above.
        let provider = unsafe { registration::prepare(&config, &mut session) }?;
        let prepared = Rc::get_mut(&mut registration).expect("unpublished unique registration");
        prepared.provider = Some(provider.clone());
        *prepared.persistent.get_mut() = Some(persistent.bind(
            provider.clone(),
            u32::try_from(data.config.ceiling.canonical_indices).unwrap_or(u32::MAX),
        ));
        inventory.push(provider);
        registrations.push(registration.clone());
        let handle = Box::new(ProviderHandle {
            engine: identity,
            registration,
        });
        Ok::<_, u32>((inventory, registrations, handle))
    })();
    let (inventory, registrations, handle) = match prepared {
        Ok(value) => value,
        Err(status) => {
            let diagnostic = admission::Failure::new(wire::Diagnostic::InvalidMetadata);
            // SAFETY: a refusal replaces only the initialized error object.
            unsafe { error.write(diagnostic.error) };
            return status;
        }
    };
    data.inventory = inventory;
    data.registrations = registrations;
    owner.handles.set(next_handles);
    // SAFETY: transfer is complete; publish exactly this retained handle last.
    unsafe { output.write(Box::into_raw(handle).cast()) };
    OK
}

/// # Safety
/// Exact live provider and initialized result/error slots remain valid. The
/// provider must not be used after an admitted successful close.
pub unsafe extern "C" fn provider_drop(
    provider: *mut ffi::Provider,
    output: *mut *mut ffi::Result,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: caller retains initialized readable error and writable output.
    if provider.is_null() || output.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: the caller supplies this gateway's exact live provider handle.
    let handle = unsafe { &*provider.cast::<ProviderHandle>() };
    // SAFETY: the dependent handle keeps its engine metadata alive.
    let owner = match unsafe { owner(&handle.engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    if owner.active.get() || handle.registration.bindings.get() != 0 {
        return BUSY;
    }
    let Some(next_handles) = owner.handles.get().checked_sub(1) else {
        return INVALID;
    };
    // SAFETY: owner-thread close has no active mutable execution borrow.
    let data = unsafe { &mut *owner.data.as_ptr() };
    let Some(index) = data
        .registrations
        .iter()
        .position(|r| Rc::ptr_eq(r, &handle.registration))
    else {
        return INVALID;
    };
    let Some(original) = handle.registration.provider.as_ref() else {
        return INVALID;
    };
    if !data
        .inventory
        .get(index)
        .is_some_and(|p| p.same_registration(original))
    {
        return INVALID;
    }
    let mut budget = m::Budget::new(data.limits.preparation);
    let holder = match handle.registration.persistent.borrow().as_ref() {
        Some(persistent) => persistent.holder(),
        None => return INVALID,
    };
    let claims = match data
        .values
        .held_claims(holder, &data.allocator, &mut budget)
    {
        Ok(claims) => claims,
        Err(_) => return BUSY,
    };
    owner.active.set(true);
    let _active = Active(&owner.active);
    let result = match resources::retire(
        ResultOwner::Provider {
            engine: handle.engine,
            registration: handle.registration.clone(),
        },
        data,
        &claims,
        &mut budget,
    ) {
        Ok(result) => result,
        Err(status) => return status,
    };
    data.inventory.remove(index);
    data.registrations.remove(index);
    let status = if let Some(result) = result {
        owner.handles.set(next_handles + 1);
        // SAFETY: admitted close transfers the handle slot to its report.
        unsafe { output.write(Box::into_raw(Box::new(result)).cast()) };
        1
    } else {
        owner.handles.set(next_handles);
        // SAFETY: successful close publishes a null failure result.
        unsafe { output.write(ptr::null_mut()) };
        OK
    };
    // SAFETY: this consumes the exact unique public handle after unlink; any
    // resource/report lease retains the original code owner independently.
    unsafe {
        drop(Box::from_raw(provider.cast::<ProviderHandle>()));
    }
    status
}

/// # Safety
/// All handles/descriptors/output slots are initialized valid C objects from
/// this gateway, retained for this owner-thread operation.
pub unsafe extern "C" fn instance_create(
    module: *mut ffi::Module,
    config: *const ffi::InstanceConfig,
    output: *mut *mut ffi::Instance,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: initialized descriptors and writable slots outlive this operation.
    if module.is_null()
        || output.is_null()
        || !unsafe { error_ready(error) }
        || !unsafe { header(config) }
    {
        return INVALID;
    }
    // SAFETY: exact live module and complete initialized configuration.
    let (module, config) = unsafe { (&*module.cast::<Loaded>(), config.read()) };
    // SAFETY: module retains its original engine metadata.
    let owner = match unsafe { owner(&module.engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    if owner.active.get() {
        return BUSY;
    }
    // SAFETY: the nested local descriptor is a complete initialized object.
    if !unsafe { header(ptr::from_ref(&config.ceiling)) }
        || config.ceiling.reserved != [0; 2]
        || config.reserved != [0; 2]
    {
        return INVALID;
    }
    let Some(next_handles) = owner.handles.get().checked_add(1) else {
        return OOM;
    };
    // SAFETY: no active root borrows owner-thread execution data.
    let data = unsafe { &mut *owner.data.as_ptr() };
    let prepared = (|| {
        let mut session = wire::Session::new(data.limits.metadata);
        // SAFETY: native caller retains all initialized grant views and bytes.
        let grants =
            unsafe { registration::grants(config.grants, config.grant_count, &mut session) }?;
        let mut views = session.vector(grants.len(), 0).map_err(|_| INVALID)?;
        for grant in &grants {
            views.push(grant.view());
        }
        let bound = grants::prepare(
            module.module.unit(),
            module.module.contract(),
            &views,
            || &data.inventory,
            &mut session,
        )
        .map_err(|_| INVALID)?;
        let mut providers = session
            .vector(bound.imports().len(), 0)
            .map_err(|_| INVALID)?;
        let mut registrations: Vec<Rc<Registered>> = session
            .vector(bound.imports().len(), 0)
            .map_err(|_| INVALID)?;
        for import in bound.imports() {
            let selected = import.provider();
            session
                .charge(data.registrations.len(), 0)
                .map_err(|_| INVALID)?;
            let registration = data
                .registrations
                .iter()
                .find(|r| {
                    r.provider
                        .as_ref()
                        .is_some_and(|p| p.same_registration(selected))
                })
                .ok_or(INVALID)?;
            session
                .charge(registrations.len(), 0)
                .map_err(|_| INVALID)?;
            if registrations.iter().any(|r| Rc::ptr_eq(r, registration)) {
                continue;
            }
            let retained = selected.clone().into_owned().map_err(|_| INVALID)?;
            registration.bindings.get().checked_add(1).ok_or(OOM)?;
            providers.push(retained);
            registrations.push(registration.clone());
        }
        session
            .retained::<InstanceData>(1, 0)
            .map_err(|_| INVALID)?;
        session.retained::<usize>(2, 0).map_err(|_| INVALID)?;
        session
            .retained::<InstanceHandle>(1, 0)
            .map_err(|_| INVALID)?;
        let mut leases = session
            .vector(registrations.len(), 0)
            .map_err(|_| INVALID)?;
        let index_capacity =
            usize::try_from(bounds(data.config.ceiling, config.ceiling).canonical_indices)
                .map_err(|_| INVALID)?;
        let indices = session.vector(index_capacity, 0).map_err(|_| INVALID)?;
        // Every fallible step precedes fresh holder and lease acquisition.
        let state = execution::InstanceState::prepare(&mut data.values).map_err(|_| INVALID)?;
        for registration in registrations {
            registration.bindings.set(registration.bindings.get() + 1);
            leases.push(ImportLease(registration));
        }
        Ok::<_, u32>((grants, providers, leases, state, indices))
    })();
    let (grants, providers, leases, state, indices) = match prepared {
        Ok(v) => v,
        Err(s) => return s,
    };
    let instance = Box::new(InstanceHandle(Rc::new(InstanceData {
        engine: module.engine,
        module: module.module.clone(),
        native: module.native.clone(),
        state: RefCell::new(state),
        closed: Cell::new(false),
        ceiling: bounds(data.config.ceiling, config.ceiling),
        digest: module.digest,
        grants,
        providers,
        leases: RefCell::new(leases),
        indices: RefCell::new(indices),
    })));
    owner.handles.set(next_handles);
    // SAFETY: publish only after the complete checked transaction succeeds.
    unsafe { output.write(Box::into_raw(instance).cast()) };
    OK
}
/// # Safety
/// Instance, projection and initialized output/error slots remain live for the
/// operation. Projection bytes contain no callable or resource authority.
pub unsafe extern "C" fn bind_export(
    instance: *mut ffi::Instance,
    projection: ffi::Bytes,
    output: *mut *mut ffi::Binding,
    error: *mut ffi::Error,
) -> u32 {
    if instance.is_null() || output.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: caller retains the exact live instance handle.
    let instance = unsafe { &(*instance.cast::<InstanceHandle>()).0 };
    let owner = match unsafe { owner(&instance.engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    if owner.active.get() {
        return BUSY;
    }
    if instance.closed.get() {
        return INVALID;
    }
    // SAFETY: same owner thread accesses immutable preparation settings/views.
    let data = unsafe { owner.data.as_ref() };
    let projection = match unsafe { bytes(projection) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    let prepared = (|| -> Result<m::FunctionId, u32> {
        let mut session = wire::Session::new(data.limits.metadata);
        let consumer =
            wire::decode_contract(m::SourceId(0), projection, &mut session).map_err(|_| INVALID)?;
        let doc = consumer.document();
        if doc.imports.is_empty() {
            return selection::task_root(&consumer, &instance.module, &mut session);
        }
        if doc.imports.len() != 1 || doc.imports[0].members.len() != 1 {
            return Err(INVALID);
        }
        let request = &doc.imports[0];
        let interface = doc
            .interfaces
            .iter()
            .find(|i| i.key.value == request.interface.value)
            .ok_or(INVALID)?;
        let function = interface
            .functions
            .iter()
            .find(|f| f.name.value == request.members[0].value)
            .ok_or(INVALID)?;
        let encoded =
            wire::encode_contract(instance.module.contract(), &mut session).map_err(|_| INVALID)?;
        let cap = &function.capability.value;
        meter(
            &mut session,
            data.limits,
            encoded.len().saturating_add(cap.len()),
            32usize.saturating_add(cap.len()),
            33,
        )?;
        let digest =
            VersionedDigest::new(DigestAlgorithm::Sha256, Sha256::digest(&encoded).to_vec())
                .map_err(|_| INVALID)?;
        let mut capabilities = CapabilityGrants::new();
        capabilities.grant(cap.as_str());
        let providers = [wire::Provider {
            contract: instance.module.contract(),
            digest: &digest,
        }];
        let selected = wire::select_provider(&consumer, 0, &providers, &capabilities, &mut session)
            .map_err(|_| INVALID)?;
        if selected.members().len() != 1 {
            return Err(INVALID);
        }
        let member = selected.members()[0].actual;
        instance
            .module
            .unit()
            .exports()
            .iter()
            .find_map(|(id, f)| {
                (f.interface == selected.provider_interface() && f.member == member).then_some(*id)
            })
            .ok_or(INVALID)
    })();
    let entry = match prepared {
        Ok(v) => v,
        Err(s) => return s,
    };
    if instance
        .native
        .as_ref()
        .is_some_and(|native| native.image.0.entry != entry)
    {
        return UNSUPPORTED;
    }
    if retain(owner).is_err() {
        return OOM;
    }
    let binding = Box::new(Binding {
        instance: instance.clone(),
        entry,
    });
    // SAFETY: complete typed/versioned selection succeeded before publication.
    unsafe { output.write(Box::into_raw(binding).cast()) };
    OK
}

/// # Safety
/// Binding/error are exact live gateway objects; close the binding only once.
pub unsafe extern "C" fn binding_drop(binding: *mut ffi::Binding, error: *mut ffi::Error) -> u32 {
    if binding.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: caller retains this exact live dependent.
    let reference = unsafe { &*binding.cast::<Binding>() };
    let owner = match unsafe { owner(&reference.instance.engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    if owner.active.get() {
        return BUSY;
    }
    if release(owner).is_err() {
        return INVALID;
    }
    // SAFETY: consume the exact unique opaque binding after admission.
    unsafe { drop(Box::from_raw(binding.cast::<Binding>())) };
    OK
}

/// # Safety
/// Engine, limits and output/error slots are initialized live native objects.
pub unsafe extern "C" fn control_create(
    engine: *mut ffi::Engine,
    limits: *const ffi::Limits,
    output: *mut *mut ffi::Control,
    error: *mut ffi::Error,
) -> u32 {
    if output.is_null() || !unsafe { error_ready(error) } || !unsafe { header(limits) } {
        return INVALID;
    }
    let pointer = match unsafe { metadata(engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    // SAFETY: checked engine is retained and caller supplied complete limits.
    let (owner, limits) = unsafe { (pointer.as_ref(), limits.read()) };
    if owner.active.get() {
        return BUSY;
    }
    if limits.reserved != [0; 2] {
        return INVALID;
    }
    // SAFETY: owner thread reads immutable engine configuration.
    let ceiling = bounds(unsafe { owner.data.as_ref() }.config.ceiling, limits);
    if retain(owner).is_err() {
        return OOM;
    }
    let control = Box::new(Control {
        engine: pointer,
        ceiling,
        flag: e::InterruptionFlag::new(),
        used: Cell::new(false),
    });
    // SAFETY: fresh one-shot control publishes only after admission.
    unsafe { output.write(Box::into_raw(control).cast()) };
    OK
}

/// # Safety
/// The exact live control is retained through this potentially cross-thread
/// operation. Its owner must not destroy it concurrently.
pub unsafe extern "C" fn interrupt(control: *mut ffi::Control) {
    if control.is_null() {
        return;
    }
    // SAFETY: borrow only the immutable atomic flag field; no owner-thread
    // Cell or engine execution storage is followed by a foreign thread.
    let flag = unsafe { &*ptr::addr_of!((*control.cast::<Control>()).flag) };
    flag.interrupt();
}

/// # Safety
/// Exact live control/error are retained through this owner-thread operation.
pub unsafe extern "C" fn control_drop(control: *mut ffi::Control, error: *mut ffi::Error) -> u32 {
    if control.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: native caller supplies the exact live control.
    let control_ref = unsafe { &*control.cast::<Control>() };
    let owner = match unsafe { owner(&control_ref.engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    if owner.active.get() {
        return BUSY;
    }
    if release(owner).is_err() {
        return INVALID;
    }
    // SAFETY: consume the exact unique control after all use ended.
    unsafe { drop(Box::from_raw(control.cast::<Control>())) };
    OK
}

/// # Safety
/// Instance/result/error slots are exact live native objects. A successful
/// close invalidates the instance handle; outstanding bindings remain closed.
pub unsafe extern "C" fn instance_close(
    instance: *mut ffi::Instance,
    output: *mut *mut ffi::Result,
    error: *mut ffi::Error,
) -> u32 {
    // SAFETY: forward the caller's exact native lifetime/output contract.
    unsafe { resources::close(instance, output, error) }
}

/// # Safety
/// Exact live binding/control and initialized output/error slots remain valid
/// through the operation. Argument storage is a readable canonical byte view.
pub unsafe extern "C" fn call(
    binding: *mut ffi::Binding,
    control: *mut ffi::Control,
    arguments: ffi::Bytes,
    output: *mut *mut ffi::Result,
    error: *mut ffi::Error,
) -> u32 {
    if binding.is_null() || control.is_null() || output.is_null() || !unsafe { error_ready(error) }
    {
        return INVALID;
    }
    // SAFETY: native caller retains these exact live opaque objects.
    let (binding, control) = unsafe { (&*binding.cast::<Binding>(), &*control.cast::<Control>()) };
    let owner = match unsafe { owner(&binding.instance.engine) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    if owner.active.get() {
        return BUSY;
    }
    let reservation = match ResultReservation::new(owner) {
        Ok(value) => value,
        Err(status) => return status,
    };
    if binding.instance.engine != control.engine
        || binding.instance.closed.get()
        || control.used.get()
    {
        return INVALID;
    }
    // SAFETY: view remains readable for this complete operation.
    let arguments = match unsafe { bytes(arguments) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    // A valid control is one-shot even when later preparation refuses entry.
    control.used.set(true);
    owner.active.set(true);
    let active = Active(&owner.active);
    // SAFETY: owner-thread active guard excludes other mutable root operations.
    let data = unsafe { &mut *owner.data.as_ptr() };
    let ceiling = bounds(binding.instance.ceiling, control.ceiling);
    let _current = CurrentScope::enter(
        owner,
        CurrentCall {
            instance: binding.instance.clone(),
            mode: data.config.mode,
            limits: data.limits,
            ceiling,
        },
    );
    let as_size = |n| usize::try_from(n).map_err(|_| INVALID);
    let execute = (|| -> Result<CallResult, u32> {
        let limits = e::Limits {
            fuel: Some(ceiling.fuel),
            host_calls: Some(ceiling.host_calls),
            frames: Some(as_size(ceiling.frames)?),
            live_cells: Some(as_size(ceiling.live_cells)?),
        };
        let invocation = e::Control::with_limits(limits)
            .and_service_work(Some(ceiling.service_work))
            .and_interruption(&control.flag);
        let mut session = wire::Session::new(data.limits.metadata);
        let mut views = session
            .vector(binding.instance.grants.len(), 0)
            .map_err(|_| INVALID)?;
        for grant in &binding.instance.grants {
            views.push(grant.view());
        }
        let bound = grants::prepare(
            binding.instance.module.unit(),
            binding.instance.module.contract(),
            &views,
            || &binding.instance.providers,
            &mut session,
        )
        .map_err(|_| INVALID)?;
        let mut budget = if data.config.mode == 2 {
            m::Budget::with_usage(data.limits.preparation, session.preparation_usage())
                .map_err(|_| INVALID)?
        } else {
            m::Budget::new(data.limits.preparation)
        };
        let program = if data.config.mode == 2 {
            program::Program::prepare_with_budget(&bound, &mut budget)
        } else {
            program::Program::prepare(&bound, Default::default())
        }
        .map_err(|reason| {
            if data.config.mode == 2 {
                native_image::program_refused(reason, error)
            } else {
                INVALID
            }
        })?;
        let runtime = r::Limits {
            tasks: lang_runtime::task::Limits {
                live_tasks: as_size(ceiling.live_tasks)?,
                ..Default::default()
            },
            preparation: data.limits.preparation,
            cleanup_failure_records: as_size(ceiling.suppressed_records)?.saturating_add(1),
            ..Default::default()
        };
        let generated = if data.config.mode == 1 {
            Some(
                lang_codegen_clif::tasks::CompiledProgram::compile_component(
                    &program,
                    binding.entry,
                    data.limits.preparation,
                )
                .map_err(|_| INVALID)?,
            )
        } else {
            None
        };
        let aot = if data.config.mode == 2 {
            Some(
                binding
                    .instance
                    .native
                    .as_ref()
                    .ok_or(UNSUPPORTED)?
                    .image
                    .bind(&program, &mut budget)
                    .map_err(|reason| native_image::refused(reason, error))?,
            )
        } else {
            None
        };
        let mut invoker = data.bridge.begin(invocation)?;
        let mut state = binding.instance.state.try_borrow_mut().map_err(|_| BUSY)?;
        state
            .with_program(&program, |instance| {
                let early = |reason: r::Reason,
                             service_work: u64,
                             allocations: a::Stats|
                 -> Result<CallResult, u32> {
                    if !matches!(
                        reason,
                        r::Reason::Control(_) | r::Reason::ComponentIndices { .. }
                    ) {
                        return Err(INVALID);
                    }
                    let failure = r::Failure {
                        reason,
                        function: Some(binding.entry),
                        span: Some(
                            binding
                                .instance
                                .module
                                .unit()
                                .code()
                                .declarations()
                                .functions()[binding.entry.0 as usize]
                                .signature_span,
                        ),
                        callers: Some(execution::GuestCallFrames {
                            frames: Vec::new(),
                            omitted: 0,
                        }),
                        usage: e::Usage {
                            service_work,
                            ..Default::default()
                        },
                        tasks: Default::default(),
                        events: vec![],
                        report: None,
                        poisoned: false,
                    };
                    let report = project_failure(
                        &failure,
                        ceiling,
                        FailureCode {
                            digest: binding.instance.digest,
                            mode: data.config.mode,
                            entry: binding.entry,
                            functions: binding
                                .instance
                                .module
                                .unit()
                                .code()
                                .declarations()
                                .functions(),
                        },
                        allocations,
                        binding.instance.indices.borrow().len() as u64,
                    );
                    Ok(CallResult {
                        owner: ResultOwner::Instance(binding.instance.clone()),
                        body: ResultBody::Failure(Rc::new(failure)),
                        usage: e::Usage {
                            service_work,
                            ..Default::default()
                        },
                        committed: false,
                        poisoned: false,
                        allocations,
                        canonical_indices: binding.instance.indices.borrow().len() as u64,
                        tasks: Default::default(),
                        report,
                    })
                };
                let mut host = match execution::Invocation::prepare_owned(
                    instance,
                    binding.entry,
                    &mut data.values,
                    &mut invoker,
                    &mut budget,
                ) {
                    Ok(v) => v,
                    Err(reason) => return early(reason, 0, data.allocator.stats()),
                };
                let namespace = execution::ResourceNamespace::new(
                    u32::try_from(binding.instance.indices.borrow().len()).map_err(|_| INVALID)?,
                    u32::try_from(ceiling.canonical_indices).map_err(|_| INVALID)?,
                );
                if let Err(reason) = host.resource_namespace(namespace) {
                    return early(reason, host.service_work_used(), data.allocator.stats());
                }
                let arguments = match host.decode_arguments(
                    arguments,
                    &binding.instance.indices.borrow(),
                    &data.allocator,
                    &mut session,
                ) {
                    Ok(v) => v,
                    Err(reason) => {
                        return early(reason, host.service_work_used(), data.allocator.stats());
                    }
                };
                if let Err(reason) = host.prepare_output(&mut session) {
                    return early(reason, host.service_work_used(), data.allocator.stats());
                }
                let outcome = if let Some(aot) = aot {
                    aot.execute_component_with_budget(
                        &mut data.allocator,
                        invocation,
                        runtime,
                        &arguments,
                        &mut host,
                        &mut budget,
                    )
                } else if let Some(generated) = generated {
                    generated.execute_component(
                        &mut data.allocator,
                        invocation,
                        runtime,
                        &arguments,
                        &mut host,
                    )
                } else {
                    lang_vm::unit::tasks::execute_component(
                        &program,
                        binding.entry,
                        &mut data.allocator,
                        invocation,
                        runtime,
                        &arguments,
                        &mut host,
                    )
                };
                let committed = execution::Host::arguments_committed(&host);
                drop(host);
                let allocations = data.allocator.stats();
                match outcome {
                    Ok(outcome) => {
                        let execution::Value::Frame(frame) = outcome.value else {
                            return Err(INVALID);
                        };
                        {
                            let mut indices = binding.instance.indices.borrow_mut();
                            debug_assert!(
                                indices.len() + frame.resources().len() <= indices.capacity()
                            );
                            indices.extend_from_slice(frame.resources());
                        }
                        Ok(CallResult {
                            owner: ResultOwner::Instance(binding.instance.clone()),
                            body: ResultBody::Value(frame),
                            usage: outcome.usage,
                            committed,
                            poisoned: outcome.poisoned,
                            allocations,
                            canonical_indices: binding.instance.indices.borrow().len() as u64,
                            tasks: outcome.tasks,
                            report: None,
                        })
                    }
                    Err(mut failure) => {
                        let usage = failure.usage;
                        let tasks = failure.tasks;
                        let report = project_failure(
                            &failure,
                            ceiling,
                            FailureCode {
                                digest: binding.instance.digest,
                                mode: data.config.mode,
                                entry: binding.entry,
                                functions: binding
                                    .instance
                                    .module
                                    .unit()
                                    .code()
                                    .declarations()
                                    .functions(),
                            },
                            allocations,
                            binding.instance.indices.borrow().len() as u64,
                        );
                        if report.as_ref().is_some_and(|(report, _)| report.poisoned) {
                            failure.poisoned = true;
                            instance.poison();
                        }
                        let poisoned = failure.poisoned;
                        Ok(CallResult {
                            owner: ResultOwner::Instance(binding.instance.clone()),
                            body: ResultBody::Failure(Rc::new(failure)),
                            usage,
                            committed,
                            poisoned,
                            allocations,
                            canonical_indices: binding.instance.indices.borrow().len() as u64,
                            tasks,
                            report,
                        })
                    }
                }
            })
            .map_err(|_| INVALID)?
    })();
    drop(active);
    let value = match execute {
        Ok(v) => v,
        Err(s) => return s,
    };
    let status = if matches!(value.body, ResultBody::Failure(_)) {
        1
    } else {
        OK
    };
    reservation.commit();
    // SAFETY: admitted completion publishes one immutable owned result handle.
    unsafe { output.write(Box::into_raw(Box::new(value)).cast()) };
    status
}

/// # Safety
/// Result and initialized info/error slots are exact live native objects.
pub unsafe extern "C" fn result_info(
    result: *const ffi::Result,
    output: *mut ffi::ResultInfo,
    error: *mut ffi::Error,
) -> u32 {
    if result.is_null() || !unsafe { header(output) } || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: exact immutable result remains live through inspection.
    let value = unsafe { &*result.cast::<CallResult>() };
    if let Err(s) = unsafe { owner(value.owner.engine()) } {
        return s;
    }
    let (kind, records, omitted, frames) = match &value.body {
        ResultBody::Value(_) => (0, 0, 0, 0),
        ResultBody::Cleanup { report, .. } => {
            if let Some((wire, _)) = &value.report {
                (1, wire.suppressed.len(), wire.omitted, wire.frames.len())
            } else {
                (1, report.secondary().len(), report.truncated() as u64, 0)
            }
        }
        ResultBody::Failure(f) => {
            if let Some((report, _)) = &value.report {
                (
                    1,
                    report.suppressed.len(),
                    report.omitted,
                    report.frames.len(),
                )
            } else {
                (
                    1,
                    f.report.as_ref().map_or(0, |r| r.secondary().len()),
                    f.report.as_ref().map_or(0, |r| r.truncated() as u64),
                    0,
                )
            }
        }
    };
    // SAFETY: output was initialized and header checked; named fields only.
    unsafe {
        *output = ffi::ResultInfo {
            header: (*output).header,
            kind,
            flags: u32::from(value.committed) | (u32::from(value.poisoned) * 2),
            omitted_records: omitted,
            suppressed_count: records as u64,
            frame_count: frames as u64,
            reserved: [0; 2],
        }
    };
    OK
}

/// # Safety
/// Exact immutable result and initialized usage/error slots remain live.
pub unsafe extern "C" fn result_usage(
    result: *const ffi::Result,
    output: *mut ffi::Usage,
    error: *mut ffi::Error,
) -> u32 {
    if result.is_null() || !unsafe { header(output) } || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: exact result retains its original engine and immutable usage.
    let value = unsafe { &*result.cast::<CallResult>() };
    if let Err(s) = unsafe { owner(value.owner.engine()) } {
        return s;
    }
    // SAFETY: initialized output header was checked before publication.
    unsafe {
        *output = ffi::Usage {
            header: (*output).header,
            fuel: value.usage.fuel,
            service_work: value.usage.service_work,
            host_calls: value.usage.host_calls,
            peak_heap_bytes: value.allocations.peak_live_bytes,
            peak_live_allocations: value.allocations.peak_live_allocations as u64,
            peak_frames: value.usage.peak_frames as u64,
            peak_live_cells: value.usage.peak_live_cells as u64,
            peak_live_tasks: value.tasks.peak_live as u64,
            canonical_indices: value
                .report
                .as_ref()
                .map_or(value.canonical_indices, |(report, _)| {
                    report.usage.canonical_indices
                }),
            reserved: [0; 2],
        }
    };
    OK
}

/// # Safety
/// Result/output/error slots are valid and live. Returned immutable bytes
/// borrow the result handle and expire only when that handle is released.
pub unsafe extern "C" fn result_bytes(
    result: *const ffi::Result,
    output: *mut ffi::Bytes,
    error: *mut ffi::Error,
) -> u32 {
    if result.is_null() || output.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: exact result remains retained throughout this immutable view.
    let value = unsafe { &*result.cast::<CallResult>() };
    if let Err(s) = unsafe { owner(value.owner.engine()) } {
        return s;
    }
    let bytes = match &value.body {
        ResultBody::Value(frame) => frame.bytes(),
        ResultBody::Failure(_) | ResultBody::Cleanup { .. } => match &value.report {
            Some((_, bytes)) => bytes.as_slice(),
            None => return INVALID,
        },
    };
    // SAFETY: complete canonical bytes are retained by this result handle.
    unsafe {
        output.write(ffi::Bytes {
            data: bytes.as_ptr(),
            length: bytes.len() as u64,
        })
    };
    OK
}

/// # Safety
/// Exact unique live result/error objects are retained through release.
pub unsafe extern "C" fn result_drop(result: *mut ffi::Result, error: *mut ffi::Error) -> u32 {
    if result.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: immutable result still retains its original engine metadata.
    let value = unsafe { &*result.cast::<CallResult>() };
    let owner = match unsafe { owner(value.owner.engine()) } {
        Ok(v) => v,
        Err(s) => return s,
    };
    if release(owner).is_err() {
        return INVALID;
    }
    // SAFETY: only decoder-owned representation is released; independent
    // resource ownership is never inferred from or retired by these bytes.
    unsafe { drop(Box::from_raw(result.cast::<CallResult>())) };
    OK
}

/// # Safety
/// Module is the unique live module handle from this gateway, closed once;
/// the initialized error slot remains valid for the complete operation.
pub unsafe extern "C" fn module_drop(module: *mut ffi::Module, error: *mut ffi::Error) -> u32 {
    // SAFETY: native caller retains initialized error and exact module handle.
    if module.is_null() || !unsafe { error_ready(error) } {
        return INVALID;
    }
    // SAFETY: only successful module_load publishes this exact Loaded Box.
    let loaded = unsafe { &*module.cast::<Loaded>() };
    // SAFETY: the dependent handle keeps its engine live; resolve checks thread.
    let owner = unsafe { loaded.engine.as_ref() };
    let handle = owner.engine;
    // SAFETY: handle derives from the retained original engine.
    if let Err(status) = unsafe { metadata(handle) } {
        return status;
    }
    if owner.active.get() {
        return BUSY;
    }
    let Some(count) = owner.handles.get().checked_sub(1) else {
        return INVALID;
    };
    owner.handles.set(count);
    // SAFETY: the exact unique handle is consumed once after all admission.
    unsafe { drop(Box::from_raw(module.cast::<Loaded>())) };
    OK
}
