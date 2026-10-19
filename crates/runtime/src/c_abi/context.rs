//! Shared context admission/state kernel for generated VM/native C adapters.
//!
//! This safe layer never dereferences raw C descriptors, restores erased
//! callback types, invokes allocator pointers, or publishes native output.
//! Those trusted adapter operations must consume this gate, retain actual
//! object/code/user lifetimes, and stage results until guest cleanup succeeds.
use super::{Artifact, Direction, Kind as TypeKind, validation};
use crate::{cleanup, execution};
use lang_mir::unit::ownership::failure::ExecutionFailure;
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU64, Ordering};

pub mod config;
mod error;
use config::{Allocator, BoundImport, Capacities, CheckedLimits, Config, Snapshot};
pub use error::{ErrorSummary, Fault, Kind, Status};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
fn identity() -> Result<u64, Fault> {
    NEXT_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| Fault::new(Kind::IdentityExhausted))
}
/// Retain the real JIT/linked/VM code owner. ABI digests describe signatures,
/// not executable identity; registering two same-ABI programs keeps them apart.
pub trait CodeOwner {}
impl<T> CodeOwner for T {}
pub struct Program<'a> {
    artifact: &'a Artifact,
    _owner: &'a dyn CodeOwner,
    id: u64,
    capacities: Capacities,
}
impl<'a> Program<'a> {
    pub fn new(
        artifact: &'a Artifact,
        owner: &'a dyn CodeOwner,
        capacities: Capacities,
    ) -> Result<Self, Fault> {
        Ok(Self {
            artifact,
            _owner: owner,
            id: identity()?,
            capacities,
        })
    }
    pub fn create(&'a self, config: Config<'_>) -> Result<Context<'a>, Fault> {
        let snapshot = Snapshot::check(self.artifact, config, self.capacities)?;
        Ok(Context {
            program: self,
            allocator_identity: AllocatorIdentity(identity()?),
            snapshot,
            state: RefCell::new(State {
                instance: cleanup::Instance::default(),
                last: None,
                closed: false,
            }),
        })
    }
    pub const fn artifact(&self) -> &Artifact {
        self.artifact
    }
}
/// Process-local allocator-instance provenance, never a serialized handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AllocatorIdentity(u64);

struct State {
    instance: cleanup::Instance,
    last: Option<Fault>,
    closed: bool,
}
pub struct Context<'a> {
    program: &'a Program<'a>,
    allocator_identity: AllocatorIdentity,
    snapshot: Snapshot,
    state: RefCell<State>,
}
/// Addresses are checked before any input read; output may be uninitialized.
#[derive(Clone, Copy)]
pub struct Call<'a> {
    /// Source call site for guest imports; native callers have no source span.
    pub span: Option<lang_mir::unit::Span>,
    pub context: validation::Address,
    pub arguments: &'a [validation::Argument<'a>],
    pub output: Option<validation::Address>,
}
pub enum EntryPoint<'a> {
    Guest,
    Export {
        function: usize,
        call: Call<'a>,
        /// Initialized call-local C storage, distinct from caller output.
        /// The adapter publishes it only after this gate returns success.
        result: Option<&'a dyn validation::Reader>,
    },
}
impl Context<'_> {
    pub const fn allocator(&self) -> Allocator {
        self.snapshot.allocator
    }
    pub const fn allocator_identity(&self) -> AllocatorIdentity {
        self.allocator_identity
    }
    pub const fn limits(&self) -> CheckedLimits {
        self.snapshot.limits
    }
    pub fn last_error(&self) -> Result<ErrorSummary, Fault> {
        let state = self
            .state
            .try_borrow()
            .map_err(|_| Fault::new(Kind::Reentry))?;
        if state.closed {
            return Err(Fault::new(Kind::Closed));
        }
        Ok(state
            .last
            .map(ErrorSummary::from_fault)
            .unwrap_or_else(ErrorSummary::success))
    }
    pub fn is_poisoned(&self) -> Result<bool, Fault> {
        self.state
            .try_borrow()
            .map(|state| state.instance.is_poisoned())
            .map_err(|_| Fault::new(Kind::Reentry))
    }
    /// The shared borrow is intentional: nested entry reaches try_borrow_mut
    /// before anyone constructs a mutable state reference. Native handles must
    /// enforce the synchronous thread/lifetime contract before obtaining &Self.
    pub fn run(
        &self,
        program: &Program<'_>,
        control: execution::Control<'_>,
        work: &execution::ServiceWork,
        usage: &mut execution::Usage,
        point: EntryPoint<'_>,
        body: &mut impl FnMut(&mut Entry<'_, '_>) -> Result<(), Fault>,
    ) -> Result<(), Fault> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| Fault::new(Kind::Reentry))?;
        if state.closed {
            return Err(Fault::new(Kind::Closed));
        }
        // Reentry and poison rejection preserve the existing primary summary.
        if state.instance.is_poisoned() {
            return Err(Fault::new(Kind::Poisoned));
        }
        let control = control.constrained(self.snapshot.limits.execution);
        let admission = contain(&mut || {
            if program.id != self.program.id {
                return Err(Fault::new(Kind::WrongProgram));
            }
            control.check()?;
            if control.limits().frames == Some(0) {
                return Err(Fault::new(Kind::Execution(ExecutionFailure::FrameLimit)));
            }
            if self.snapshot.limits.cleanup_records == 0 {
                return Err(Fault::new(Kind::Capacity));
            }
            if let EntryPoint::Export {
                function,
                call,
                result,
            } = &point
            {
                let function_metadata = self
                    .program
                    .artifact
                    .functions()
                    .get(*function)
                    .filter(|function| function.direction == Direction::Export)
                    .ok_or_else(|| Fault::new(Kind::Function(*function)))?;
                validation::call(
                    self.program.artifact,
                    *function,
                    call.context,
                    call.arguments,
                    call.output,
                    work,
                    control,
                )?;
                result_shape(self.program.artifact, function_metadata.result, *result)?;
            }
            Ok(())
        });
        if let Err(fault) = admission {
            if fault.poisons() {
                state.instance.poison();
            }
            state.last = Some(fault);
            return Err(fault);
        }
        state.last = None;
        let mut session = Session {
            artifact: self.program.artifact,
            snapshot: &self.snapshot,
            control,
            work,
            usage,
            failure: None,
        };
        // Borrow the callable; neither an owning callback capture nor a guest
        // return value is dropped while crossing this containment boundary.
        let body_result = catch_unwind(AssertUnwindSafe(|| {
            body(&mut Entry {
                instance: &mut state.instance,
                session: &mut session,
            })
        }));
        let body_result = match body_result {
            Ok(result) => result,
            Err(payload) => Err(Fault::host_panic(payload)),
        };
        let mut outcome = match (session.failure, body_result) {
            (Some(primary), Err(later)) => Err(if primary == later {
                primary
            } else {
                primary.with_later_cleanup(later)
            }),
            (Some(primary), Ok(())) => Err(primary),
            (None, result) => result,
        };
        if outcome.is_ok() {
            outcome = contain(&mut || {
                control.check()?;
                if let EntryPoint::Export {
                    function,
                    result: Some(result),
                    ..
                } = point
                {
                    let ty = self.program.artifact.functions()[function].result;
                    validation::object(self.program.artifact, ty, result, work, control)?;
                }
                Ok(())
            });
        }
        // A backend must propagate its Report. If it discards a cleanup error,
        // the shared Instance still prevents successful result publication.
        if outcome.is_ok() && state.instance.is_poisoned() {
            outcome = Err(Fault::new(Kind::Cleanup));
        }
        if let Err(fault) = outcome {
            if fault.poisons() {
                state.instance.poison();
            }
            state.last = Some(fault);
        }
        outcome
    }
    /// Revoke first, then best-effort teardown. A native destroy consumes the
    /// opaque pointer even if this returns an error; it cannot publish reuse.
    pub fn close(&self, teardown: &mut impl FnMut() -> Result<(), Fault>) -> Result<(), Fault> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| Fault::new(Kind::Reentry))?;
        if state.closed {
            return Err(Fault::new(Kind::Closed));
        }
        state.closed = true;
        match catch_unwind(AssertUnwindSafe(teardown)) {
            Ok(result) => result,
            Err(payload) => Err(Fault::host_panic(payload)),
        }
    }
}
fn contain(operation: &mut impl FnMut() -> Result<(), Fault>) -> Result<(), Fault> {
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(result) => result,
        Err(payload) => Err(Fault::host_panic(payload)),
    }
}
fn result_shape(
    artifact: &Artifact,
    ty: super::TypeId,
    result: Option<&dyn validation::Reader>,
) -> Result<(), Fault> {
    let unit = matches!(artifact.types()[ty.0].kind, TypeKind::Unit);
    if unit != result.is_none() {
        return Err(Fault::new(Kind::ResultShape));
    }
    Ok(())
}

pub struct Entry<'a, 'b> {
    instance: &'a mut cleanup::Instance,
    session: &'a mut Session<'b>,
}
impl<'b> Entry<'_, 'b> {
    /// Disjoint state lets generated cleanup Frames borrow the Instance while
    /// callbacks use the same Session. Never create a second cleanup instance.
    pub fn parts(&mut self) -> (&mut cleanup::Instance, &mut Session<'b>) {
        (self.instance, self.session)
    }
    pub const fn cleanup_records(&self) -> usize {
        self.session.snapshot.limits.cleanup_records
    }
}
pub struct Session<'a> {
    artifact: &'a Artifact,
    snapshot: &'a Snapshot,
    control: execution::Control<'a>,
    work: &'a execution::ServiceWork,
    usage: &'a mut execution::Usage,
    failure: Option<Fault>,
}
impl Session<'_> {
    pub const fn control(&self) -> execution::Control<'_> {
        self.control
    }
    pub fn work(&self) -> &execution::ServiceWork {
        self.work
    }
    pub fn usage(&mut self) -> &mut execution::Usage {
        self.usage
    }
    /// Charge once, after complete call preflight and before the foreign call.
    /// The trusted backend uses this same Usage/ServiceWork for its invocation.
    pub fn invoke(
        &mut self,
        function: usize,
        call: Call<'_>,
        result: Option<&dyn validation::Reader>,
        invoke: &mut impl FnMut(&BoundImport) -> u32,
    ) -> Result<(), Fault> {
        if let Some(fault) = self.failure {
            return Err(fault);
        }
        let checked = (|| {
            let metadata = self
                .artifact
                .functions()
                .get(function)
                .filter(|metadata| metadata.direction == Direction::Import)
                .ok_or_else(|| Fault::new(Kind::Function(function)))?;
            let index = self
                .snapshot
                .imports
                .binary_search_by_key(&function, |binding| binding.index)
                .map_err(|_| Fault::new(Kind::Function(function)))?;
            validation::call(
                self.artifact,
                function,
                call.context,
                call.arguments,
                call.output,
                self.work,
                self.control,
            )?;
            result_shape(self.artifact, metadata.result, result)?;
            if result.map(validation::Reader::address) != call.output {
                return Err(Fault::new(Kind::ResultShape));
            }
            self.control.charge_host_call(self.usage)?;
            let status =
                match catch_unwind(AssertUnwindSafe(|| invoke(&self.snapshot.imports[index]))) {
                    Ok(status) => status,
                    Err(payload) => return Err(Fault::host_panic(payload).at(metadata.span)),
                };
            if status != 0 {
                return Err(Fault::new(Kind::ForeignStatus {
                    code: status,
                    known: status <= 6,
                })
                .at(metadata.span));
            }
            self.control.check()?;
            if let Some(result) = result {
                validation::object(
                    self.artifact,
                    metadata.result,
                    result,
                    self.work,
                    self.control,
                )
                .map_err(|error| Fault::new(Kind::ForeignValue(error)).at(metadata.span))?;
            }
            Ok(())
        })();
        if let Err(mut fault) = checked {
            fault.span = call.span.or(fault.span);
            self.failure = Some(fault);
            return Err(fault);
        }
        Ok(())
    }
}
