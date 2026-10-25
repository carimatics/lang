//! Backend-neutral owning-value cleanup for RFC 0008.
//!
//! Metadata and failure records are reserved before entry. The frame owns real
//! payloads, with an intrusive initialization list per lexical scope: moves
//! unlink the source and reinitialization appends a new obligation. Generated
//! adapters must declare parameters in order, preserve aggregate field order,
//! and validate loans, prepared destinations and shared publication before
//! calling this engine. The MIR cleanup mapping does not make complete
//! ownership bodies executable.

use lang_mir::unit::Span;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::mem::ManuallyDrop;
use std::{any::Any, rc::Rc};

mod context;
mod mir;
mod session;
pub use context::{Cleaner, Context, Contextual};
pub use lang_mir::unit::ownership::failure::{Cause, ExecutionFailure};
pub use mir::{MirLayout, RecordDrain, RecordOrder};
pub use session::Session;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub slots: usize,
    /// Includes the outer parameter scope.
    pub scopes: usize,
    /// Includes the primary record; zero is invalid.
    pub failure_records: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            slots: 4096,
            scopes: 256,
            failure_records: 256,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnterError {
    Poisoned,
    InvalidLimits,
    Allocation,
}
impl Display for EnterError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Poisoned => "execution instance is poisoned",
            Self::InvalidLimits => "cleanup requires a scope and a primary failure record",
            Self::Allocation => "cleanup metadata reservation failed before entry",
        })
    }
}
impl Error for EnterError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Slot(pub usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Owner,
    /// A compiler-known borrow registration, ended before scope owners.
    Borrow,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateError {
    InvalidSlot,
    AlreadyDeclared,
    Undeclared,
    Initialized,
    Uninitialized,
    KindMismatch,
    ScopeLimit,
    OuterScope,
    CallBoundary,
    AggregateArity { expected: usize, actual: usize },
    ExitPending,
    ExitNotStaged,
    Failed,
}
impl Display for StateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "cleanup frame state error: {self:?}")
    }
}
impl Error for StateError {}

/// Rejected initialization keeps its incoming owner outside publication.
#[derive(Debug)]
pub struct Rejected<T> {
    pub error: StateError,
    pub value: T,
}

fn poisons(failure: ExecutionFailure) -> bool {
    matches!(
        failure,
        ExecutionFailure::HostPanic | ExecutionFailure::Invariant
    )
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeCleanupKind {
    Destructor,
    HostContract,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeOrigin {
    pub provider: [u8; 32],
    pub member: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    Execution(ExecutionFailure),
    /// A trusted cleanup function's structured, allocation-free error code.
    Cleanup(u32),
    NativeCleanup {
        kind: NativeCleanupKind,
        origin: Option<NativeOrigin>,
    },
    CleanupPanic,
    PayloadDropPanic,
    PanicPayloadDropPanic,
}
/// Passive owner retention for checked native diagnostic metadata. The lease
/// exposes no code, user pointer or resource authority.
#[derive(Clone)]
pub struct CodeOwnerLease(Rc<dyn Any>);
impl CodeOwnerLease {
    pub(crate) fn new(owner: Rc<dyn Any>) -> Self {
        Self(owner)
    }
}
impl std::fmt::Debug for CodeOwnerLease {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("CodeOwnerLease")
    }
}
impl PartialEq for CodeOwnerLease {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for CodeOwnerLease {}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureRecord {
    pub kind: FailureKind,
    pub span: Option<Span>,
    pub slot: Option<Slot>,
    pub code_owner: Option<CodeOwnerLease>,
}

#[derive(Debug)]
pub struct Report {
    primary: Option<FailureRecord>,
    secondary: Vec<FailureRecord>,
    limit: usize,
    truncated: usize,
    quarantined: usize,
    cleanup_failures: usize,
}
impl Report {
    pub(crate) fn append_invariant(&mut self, span: Option<Span>) {
        self.record(FailureRecord {
            kind: FailureKind::Execution(ExecutionFailure::Invariant),
            span,
            slot: None,
            code_owner: None,
        });
    }
    pub const fn primary(&self) -> Option<&FailureRecord> {
        self.primary.as_ref()
    }
    pub fn secondary(&self) -> &[FailureRecord] {
        &self.secondary
    }
    pub const fn truncated(&self) -> usize {
        self.truncated
    }
    /// Host values/panic payloads deliberately leaked after containment failure.
    pub const fn quarantined(&self) -> usize {
        self.quarantined
    }
    /// Includes cleanup failures omitted from the bounded record buffer.
    pub const fn cleanup_failures(&self) -> usize {
        self.cleanup_failures
    }
    fn record(&mut self, record: FailureRecord) {
        if !matches!(record.kind, FailureKind::Execution(_)) {
            self.cleanup_failures = self.cleanup_failures.saturating_add(1);
        }
        if self.primary.is_none() {
            self.primary = Some(record);
        } else if self.secondary.len() < self.limit - 1 {
            self.secondary.push(record);
        } else {
            self.truncated = self.truncated.saturating_add(1);
        }
    }
    fn execution(&mut self, failure: ExecutionFailure, span: Option<Span>) {
        let old = self.primary.replace(FailureRecord {
            kind: FailureKind::Execution(failure),
            span,
            slot: None,
            code_owner: None,
        });
        if let Some(old) = old {
            // Preserve an original execution failure ahead of cleanup errors
            // already observed on a normal scope exit. Never grow the buffer.
            if self.limit == 1 {
                self.truncated = self.truncated.saturating_add(1);
            } else {
                if self.secondary.len() == self.limit - 1 {
                    self.secondary.pop();
                    self.truncated = self.truncated.saturating_add(1);
                }
                self.secondary.insert(0, old);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Reason {
    ScopeExit,
    Explicit,
    Replacement,
    Failure,
    UnpublishedReturn,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Exit {
    /// None is a normal Copy/Unit result, held outside this owning-value frame.
    Return(Option<Slot>),
    Failure {
        failure: ExecutionFailure,
        span: Option<Span>,
    },
}
#[derive(Debug)]
pub struct Completion<T> {
    /// Ownership is published only after all exiting cleanup succeeds.
    pub value: Option<T>,
    pub report: Report,
    pub poisoned: bool,
}

#[derive(Debug, Default)]
pub struct Instance {
    poisoned: bool,
}
impl Instance {
    pub const fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    /// Permanently reject reuse after an unrecoverable trusted-adapter failure.
    /// This does not retire outstanding owners; the adapter must still finish
    /// its cleanup ledger and contain independent cleanup failures.
    pub fn poison(&mut self) {
        self.poisoned = true;
    }
    /// Reservation failure prevents entry; no guest cleanup callback runs.
    pub fn enter<T, C>(&mut self, limits: Limits, cleaner: C) -> Result<Frame<'_, T, C>, EnterError>
    where
        C: FnMut(&mut T, Reason) -> Result<(), u32>,
    {
        self.enter_cleaner(limits, cleaner)
    }
    /// Generated aggregate cleanup shares the entry's bounded report buffer.
    pub fn enter_with_context<T, C>(
        &mut self,
        limits: Limits,
        cleaner: C,
    ) -> Result<Frame<'_, T, Contextual<C>>, EnterError>
    where
        C: FnMut(&mut T, Reason, &mut Context<'_>) -> Result<(), u32>,
    {
        self.enter_cleaner(limits, Contextual(cleaner))
    }
    pub fn enter_cleaner<T, C: Cleaner<T>>(
        &mut self,
        limits: Limits,
        cleaner: C,
    ) -> Result<Frame<'_, T, C>, EnterError> {
        if self.poisoned {
            return Err(EnterError::Poisoned);
        }
        if limits.scopes == 0 || limits.failure_records == 0 {
            return Err(EnterError::InvalidLimits);
        }
        let secondary = reserve(limits.failure_records - 1)?;
        let mut slots = reserve(limits.slots)?;
        slots.resize_with(limits.slots, Value::default);
        let mut scopes = reserve(limits.scopes)?;
        scopes.push(Scope {
            call: true,
            ..Scope::default()
        });
        Ok(Frame {
            instance: self,
            cleaner,
            slots,
            scopes,
            limits,
            report: Report {
                primary: None,
                secondary,
                limit: limits.failure_records,
                truncated: 0,
                quarantined: 0,
                cleanup_failures: 0,
            },
            finished: false,
            pending: None,
        })
    }
}
fn reserve<T>(count: usize) -> Result<Vec<T>, EnterError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| EnterError::Allocation)?;
    Ok(values)
}

struct Value<T> {
    payload: Option<ManuallyDrop<T>>,
    scope: Option<usize>,
    kind: Kind,
    span: Option<Span>,
    older: Option<Slot>,
    newer: Option<Slot>,
    next_declaration: Option<Slot>,
    selected: bool,
}
impl<T> Default for Value<T> {
    fn default() -> Self {
        Self {
            payload: None,
            scope: None,
            kind: Kind::Owner,
            span: None,
            older: None,
            newer: None,
            next_declaration: None,
            selected: false,
        }
    }
}
#[derive(Default)]
struct Scope {
    newest: Option<Slot>,
    declarations: Option<Slot>,
    call: bool,
}
struct PendingExit<T> {
    depth: usize,
    returned: Option<(Slot, ManuallyDrop<T>, Option<Span>)>,
    retired: bool,
}

/// A frame holds real owners and reserves all bookkeeping before guest entry.
/// The trusted cleaner must mark its internal cleanup attempts before returning;
/// subsequent Rust payload disposal must not retry those attempts. Payload
/// disposal and generated/registered cleanup are contained separately.
/// Abandoning it drains remaining values. A failed trusted callback's payload
/// is deliberately leaked: invoking its possibly panicking destructor during
/// unwinding could otherwise abort before independent owners are cleaned.
#[must_use = "finish the frame to inspect cleanup failures and publish its result"]
pub struct Frame<'a, T, C: Cleaner<T>> {
    instance: &'a mut Instance,
    cleaner: C,
    slots: Vec<Value<T>>,
    scopes: Vec<Scope>,
    limits: Limits,
    report: Report,
    finished: bool,
    pending: Option<PendingExit<T>>,
}
impl<T, C: Cleaner<T>> Frame<'_, T, C> {
    fn running(&self) -> Result<(), StateError> {
        if self.report.primary.is_some() {
            Err(StateError::Failed)
        } else {
            Ok(())
        }
    }
    fn declared(&self, slot: Slot) -> Result<&Value<T>, StateError> {
        let value = self.slots.get(slot.0).ok_or(StateError::InvalidSlot)?;
        value.scope.ok_or(StateError::Undeclared)?;
        Ok(value)
    }
    pub fn report(&self) -> &Report {
        &self.report
    }
    /// Observe the shared instance while its exclusive frame borrow is live.
    pub const fn is_poisoned(&self) -> bool {
        self.instance.poisoned
    }
    pub fn value(&self, slot: Slot) -> Result<&T, StateError> {
        self.declared(slot)?
            .payload
            .as_deref()
            .ok_or(StateError::Uninitialized)
    }
    pub fn open_scope(&mut self) -> Result<(), StateError> {
        self.push_scope(false)
    }
    /// Trusted execution adapters may retarget private owner bookkeeping after
    /// checked storage relocation, without changing its cleanup obligation/span.
    pub fn value_mut(&mut self, slot: Slot) -> Result<&mut T, StateError> {
        self.running()?;
        self.declared(slot)?;
        self.slots[slot.0]
            .payload
            .as_deref_mut()
            .ok_or(StateError::Uninitialized)
    }
    /// Open a nested guest activation's parameter scope in this instance.
    /// Slot numbers identify cells across the bounded activation stack.
    pub fn open_call(&mut self) -> Result<(), StateError> {
        self.push_scope(true)
    }
    fn push_scope(&mut self, call: bool) -> Result<(), StateError> {
        self.running()?;
        if self.pending.is_some() {
            return Err(StateError::ExitPending);
        }
        if self.scopes.len() == self.limits.scopes {
            return Err(StateError::ScopeLimit);
        }
        self.scopes.push(Scope {
            call,
            ..Scope::default()
        });
        Ok(())
    }
    pub fn declare(&mut self, slot: Slot, kind: Kind) -> Result<(), StateError> {
        self.running()?;
        if self.pending.is_some() {
            return Err(StateError::ExitPending);
        }
        let value = self.slots.get_mut(slot.0).ok_or(StateError::InvalidSlot)?;
        if value.scope.is_some() {
            return Err(StateError::AlreadyDeclared);
        }
        let depth = self.scopes.len() - 1;
        value.scope = Some(depth);
        value.kind = kind;
        value.next_declaration = self.scopes[depth].declarations.replace(slot);
        Ok(())
    }
    pub fn initialize(
        &mut self,
        slot: Slot,
        value: T,
        span: Option<Span>,
    ) -> Result<(), Rejected<T>> {
        let valid = self.running().and_then(|()| {
            let target = self.declared(slot)?;
            if target.payload.is_some() {
                Err(StateError::Initialized)
            } else {
                Ok(())
            }
        });
        if let Err(error) = valid {
            return Err(Rejected { error, value });
        }
        self.insert(slot, ManuallyDrop::new(value), span);
        Ok(())
    }
    fn insert(&mut self, slot: Slot, payload: ManuallyDrop<T>, span: Option<Span>) {
        let depth = self.slots[slot.0].scope.expect("validated declaration");
        let older = self.scopes[depth].newest.replace(slot);
        if let Some(older) = older {
            self.slots[older.0].newer = Some(slot);
        }
        let value = &mut self.slots[slot.0];
        value.payload = Some(payload);
        value.span = span;
        value.older = older;
        value.newer = None;
    }
    /// Remove the obligation before any callback can run.
    fn remove(&mut self, slot: Slot) -> (ManuallyDrop<T>, Option<Span>) {
        let value = &mut self.slots[slot.0];
        let payload = value.payload.take().expect("validated initialized slot");
        let span = value.span.take();
        let depth = value.scope.expect("validated declaration");
        let older = value.older.take();
        let newer = value.newer.take();
        value.selected = false;
        if let Some(older) = older {
            self.slots[older.0].newer = newer;
        }
        if let Some(newer) = newer {
            self.slots[newer.0].older = older;
        } else {
            self.scopes[depth].newest = older;
        }
        (payload, span)
    }
    fn transfer_valid(&self, source: Slot, target: Slot) -> Result<(), StateError> {
        self.running()?;
        let source = self.declared(source)?;
        let target = self.declared(target)?;
        if source.payload.is_none() {
            return Err(StateError::Uninitialized);
        }
        if source.kind != target.kind {
            return Err(StateError::KindMismatch);
        }
        Ok(())
    }
    pub fn move_value(&mut self, source: Slot, target: Slot) -> Result<(), StateError> {
        self.transfer_valid(source, target)?;
        if self.slots[target.0].payload.is_some() {
            return Err(StateError::Initialized);
        }
        let (payload, span) = self.remove(source);
        self.insert(target, payload, span);
        Ok(())
    }
    /// Transfer an owner into already reserved aggregate/call storage. The
    /// caller assumes its obligation before further fallible work; generated
    /// adapters must validate the complete transfer before removing any owner.
    pub fn take_owner(&mut self, slot: Slot) -> Result<T, StateError> {
        self.running()?;
        if self.declared(slot)?.kind != Kind::Owner {
            return Err(StateError::KindMismatch);
        }
        self.value(slot)?;
        let (payload, _) = self.remove(slot);
        Ok(ManuallyDrop::into_inner(payload))
    }
    /// Call after destination/RHS evaluation and replacement validation.
    /// No callback runs until the incoming owner has been committed.
    pub fn replace(&mut self, source: Slot, target: Slot) -> Result<(), StateError> {
        self.transfer_valid(source, target)?;
        if source == target || self.slots[target.0].kind != Kind::Owner {
            return Err(StateError::KindMismatch);
        }
        if self.slots[target.0].payload.is_none() {
            return Err(StateError::Uninitialized);
        }
        let (incoming, span) = self.remove(source);
        let (old, old_span) = self.remove(target);
        self.insert(target, incoming, span);
        self.destroy(target, old, old_span, Reason::Replacement);
        self.running()
    }
    /// The adapter has already published a field replacement. This source slot
    /// now owns the detached old field and its original initialization span.
    pub fn retire_replaced(&mut self, slot: Slot, span: Option<Span>) -> Result<(), StateError> {
        self.running()?;
        if self.declared(slot)?.kind != Kind::Owner {
            return Err(StateError::KindMismatch);
        }
        self.value(slot)?;
        let (payload, _) = self.remove(slot);
        self.destroy(slot, payload, span, Reason::Replacement);
        self.running()
    }
    /// Consume an already detached child through the same bounded report and
    /// poison state, without allocating another ledger slot. `slot` is its
    /// diagnostic anchor; protect any output owner in the ledger first. Even
    /// after a prior failure, this consumes the child before reporting failure.
    pub fn retire_detached<U, D: Cleaner<U>>(
        &mut self,
        slot: Slot,
        value: U,
        span: Option<Span>,
        reason: Reason,
        cleaner: &mut D,
    ) -> Result<(), StateError> {
        let span = span.or_else(|| self.slots.get(slot.0).and_then(|v| v.span));
        let mut context = Context {
            instance: &mut *self.instance,
            report: &mut self.report,
            slot,
            span,
            reason,
        };
        context.field(value, span, cleaner);
        self.running()
    }
    pub fn drop_value(&mut self, slot: Slot) -> Result<(), StateError> {
        self.running()?;
        self.value(slot)?;
        let (payload, span) = self.remove(slot);
        self.destroy(slot, payload, span, Reason::Explicit);
        self.running()
    }
    pub fn close_scope(&mut self) -> Result<(), StateError> {
        self.running()?;
        if self.pending.is_some() {
            return Err(StateError::ExitPending);
        }
        if self.scopes.len() == 1 {
            return Err(StateError::OuterScope);
        }
        let depth = self.scopes.len() - 1;
        if self.scopes[depth].call {
            return Err(StateError::CallBoundary);
        }
        self.drain_kind(depth, Kind::Borrow, Reason::ScopeExit);
        self.drain_kind(depth, Kind::Owner, Reason::ScopeExit);
        self.forget_scope();
        self.running()
    }
    /// Drain a generated cleanup range in actual initialization order. Vacant
    /// cells and duplicate targets are harmless. Validate every target before
    /// effects, and allocate no metadata while selecting or draining values.
    pub fn cleanup_slots<I>(&mut self, slots: I) -> Result<(), StateError>
    where
        I: Iterator<Item = Slot> + Clone,
    {
        self.running()?;
        for slot in slots.clone() {
            self.declared(slot)?;
        }
        for slot in slots.clone() {
            self.slots[slot.0].selected = true;
        }
        for kind in [Kind::Borrow, Kind::Owner] {
            for depth in (0..self.scopes.len()).rev() {
                self.drain_kind_if(depth, kind, Reason::ScopeExit, true);
            }
        }
        for slot in slots {
            self.slots[slot.0].selected = false;
        }
        self.running()
    }
    /// Hold a callee result outside its exiting scopes, then publish into an
    /// uninitialized caller cell only after successful callee cleanup. For a
    /// Copy/Unit result pass None/None and publish it separately after success.
    pub fn return_to(
        &mut self,
        source: Option<Slot>,
        target: Option<Slot>,
    ) -> Result<(), StateError> {
        self.running()?;
        if self.pending.is_some() {
            return Err(StateError::ExitPending);
        }
        let depth = self
            .scopes
            .iter()
            .rposition(|scope| scope.call)
            .expect("entry parameter scope");
        if depth == 0 {
            return Err(StateError::OuterScope);
        }
        match (source, target) {
            (Some(source), Some(target)) => {
                self.transfer_valid(source, target)?;
                let from = self.declared(source)?;
                let to = self.declared(target)?;
                if from.kind != Kind::Owner
                    || from.scope.is_none_or(|scope| scope < depth)
                    || to.scope.is_none_or(|scope| scope >= depth)
                {
                    return Err(StateError::CallBoundary);
                }
                if to.payload.is_some() {
                    return Err(StateError::Initialized);
                }
            }
            (None, None) => {}
            _ => return Err(StateError::CallBoundary),
        }
        let returned = source.map(|slot| {
            let (payload, span) = self.remove(slot);
            (slot, payload, span)
        });
        self.drain_from(depth, Reason::ScopeExit);
        if let Some((slot, payload, span)) = returned {
            if self.report.primary.is_none() {
                self.insert(target.expect("validated caller cell"), payload, span);
            } else {
                self.destroy(slot, payload, span, Reason::UnpublishedReturn);
            }
        }
        self.running()
    }
    fn forget_scope(&mut self) {
        let scope = self.scopes.pop().expect("an active scope");
        let mut slot = scope.declarations;
        while let Some(current) = slot {
            let value = &mut self.slots[current.0];
            slot = value.next_declaration.take();
            value.scope = None;
        }
    }
    fn drain_kind(&mut self, depth: usize, kind: Kind, reason: Reason) {
        self.drain_kind_if(depth, kind, reason, false);
    }
    fn drain_kind_if(&mut self, depth: usize, kind: Kind, reason: Reason, selected: bool) {
        let mut slot = self.scopes[depth].newest;
        while let Some(current) = slot {
            let value = &self.slots[current.0];
            slot = value.older;
            if value.kind == kind && (!selected || value.selected) {
                let (payload, span) = self.remove(current);
                self.destroy(current, payload, span, reason);
            }
        }
    }
    fn drain_all(&mut self, reason: Reason) {
        self.drain_from(0, reason);
    }
    fn drain_from(&mut self, first: usize, reason: Reason) {
        // End all compiler-known registrations before any owner teardown on
        // function/failure exits, including registrations in outer scopes.
        for depth in (first..self.scopes.len()).rev() {
            self.drain_kind(depth, Kind::Borrow, reason);
        }
        while self.scopes.len() > first {
            let depth = self.scopes.len() - 1;
            self.drain_kind(depth, Kind::Owner, reason);
            self.forget_scope();
        }
    }
    fn destroy(
        &mut self,
        slot: Slot,
        payload: ManuallyDrop<T>,
        span: Option<Span>,
        reason: Reason,
    ) {
        let cleaner = &mut self.cleaner;
        let mut context = Context {
            instance: &mut *self.instance,
            report: &mut self.report,
            slot,
            span,
            reason,
        };
        context.run(payload, |value, context| {
            cleaner.cleanup(value, reason, context)
        });
    }
    /// Protect an unpublished result before generated return cleanup starts.
    /// Copy/Unit results remain in the adapter; use None for their ledger slot.
    pub fn stage_return(&mut self, value: Option<Slot>) -> Result<(), StateError> {
        self.running()?;
        if self.pending.is_some() {
            return Err(StateError::ExitPending);
        }
        let depth = self
            .scopes
            .iter()
            .rposition(|scope| scope.call)
            .ok_or(StateError::ExitNotStaged)?;
        let returned = if let Some(slot) = value {
            if self.declared(slot)?.kind != Kind::Owner {
                return Err(StateError::KindMismatch);
            }
            if self.declared(slot)?.scope.is_none_or(|scope| scope < depth) {
                return Err(StateError::CallBoundary);
            }
            self.value(slot)?;
            let (payload, span) = self.remove(slot);
            Some((slot, payload, span))
        } else {
            None
        };
        self.pending = Some(PendingExit {
            depth,
            returned,
            retired: false,
        });
        Ok(())
    }
    /// Record a fresh execution failure, or preserve a callee/cleanup report.
    /// A Reported status without any report is a contained invariant failure.
    pub fn begin_failure(&mut self, cause: Cause, span: Option<Span>) -> Result<(), StateError> {
        if self.pending.is_none() {
            let depth = self
                .scopes
                .iter()
                .rposition(|scope| scope.call)
                .ok_or(StateError::ExitNotStaged)?;
            self.pending = Some(PendingExit {
                depth,
                returned: None,
                retired: false,
            });
        }
        let failure = match cause {
            Cause::Execution(failure) => Some(failure),
            Cause::Reported if self.report.primary.is_none() => Some(ExecutionFailure::Invariant),
            Cause::Reported => None,
        };
        if let Some(failure) = failure {
            self.instance.poisoned |= poisons(failure);
            self.report.execution(failure, span);
        }
        Ok(())
    }
    /// Retire one activation once. Failure does not stop independent cleanup.
    /// Reentering the common failure path never retries consumed obligations.
    pub fn retire_exit(&mut self) -> Result<(), StateError> {
        let pending = self.pending.as_mut().ok_or(StateError::ExitNotStaged)?;
        if !pending.retired {
            pending.retired = true;
            let depth = pending.depth;
            self.drain_from(
                depth,
                if self.report.primary.is_some() {
                    Reason::Failure
                } else {
                    Reason::ScopeExit
                },
            );
        }
        self.running()
    }
    /// A trusted entry adapter has consumed a read-only preview. Retire the
    /// protected result normally before publishing any external output.
    pub fn consume_entry_result(&mut self) -> Result<(), StateError> {
        self.running()?;
        let pending = self.pending.as_mut().ok_or(StateError::ExitNotStaged)?;
        if !pending.retired {
            return Err(StateError::ExitPending);
        }
        if pending.depth != 0 {
            return Err(StateError::CallBoundary);
        }
        if let Some((slot, payload, span)) = pending.returned.take() {
            self.destroy(slot, payload, span, Reason::ScopeExit);
        }
        self.running()
    }
    pub fn discard_result(&mut self) -> Result<(), StateError> {
        let pending = self.pending.as_mut().ok_or(StateError::ExitNotStaged)?;
        if !pending.retired {
            return Err(StateError::ExitPending);
        }
        if let Some((slot, payload, span)) = pending.returned.take() {
            self.destroy(slot, payload, span, Reason::UnpublishedReturn);
        }
        Ok(())
    }
    /// Publish into a caller only after the callee activation retired cleanly.
    pub fn publish_staged_to(&mut self, target: Option<Slot>) -> Result<(), StateError> {
        self.running()?;
        let pending = self.pending.as_ref().ok_or(StateError::ExitNotStaged)?;
        if !pending.retired {
            return Err(StateError::ExitPending);
        }
        if pending.depth == 0 {
            return Err(StateError::OuterScope);
        }
        match (&pending.returned, target) {
            (Some(_), Some(target)) => {
                let value = self.declared(target)?;
                if value.kind != Kind::Owner
                    || value.scope.is_none_or(|scope| scope >= pending.depth)
                {
                    return Err(StateError::CallBoundary);
                }
                if value.payload.is_some() {
                    return Err(StateError::Initialized);
                }
            }
            (None, None) => {}
            _ => return Err(StateError::KindMismatch),
        }
        let pending = self.pending.take().expect("validated staged exit");
        if let Some((_, payload, span)) = pending.returned {
            self.insert(target.expect("validated caller result cell"), payload, span);
        }
        Ok(())
    }
    /// Retarget an unpublished owning result only after successful cleanup.
    /// The adapter must preserve ownership before attempting publication.
    pub fn staged_value_mut(&mut self) -> Result<Option<&mut T>, StateError> {
        self.running()?;
        let pending = self.pending.as_mut().ok_or(StateError::ExitNotStaged)?;
        if !pending.retired {
            return Err(StateError::ExitPending);
        }
        Ok(pending.returned.as_mut().map(|(_, value, _)| &mut **value))
    }
    /// Return failure to the caller without publishing a result or replacing
    /// the shared report. The caller follows its own generated failure edge.
    pub fn propagate_call_failure(&mut self) -> Result<(), StateError> {
        let pending = self.pending.as_ref().ok_or(StateError::ExitNotStaged)?;
        if pending.depth == 0 {
            return Err(StateError::OuterScope);
        }
        if !pending.retired || pending.returned.is_some() {
            return Err(StateError::ExitPending);
        }
        if self.report.primary.is_none() {
            return Err(StateError::Failed);
        }
        self.pending = None;
        Ok(())
    }
    pub fn finish_staged_return(mut self) -> Completion<T> {
        if !self
            .pending
            .as_ref()
            .is_some_and(|exit| exit.depth == 0 && exit.retired)
        {
            self.instance.poisoned = true;
            self.report.execution(ExecutionFailure::Invariant, None);
        }
        self.finish(Exit::Return(None))
    }
    pub fn finish_reported_failure(mut self) -> Completion<T> {
        if self.report.primary.is_none() {
            self.instance.poisoned = true;
            self.report.execution(ExecutionFailure::Invariant, None);
        }
        self.finish(Exit::Return(None))
    }
    pub fn finish(mut self, exit: Exit) -> Completion<T> {
        let had_pending = self.pending.is_some();
        let pending = self.pending.take();
        let nested_depth = pending
            .as_ref()
            .filter(|pending| pending.depth > 0)
            .map(|pending| pending.depth);
        let mut returned = pending.and_then(|pending| pending.returned);
        match exit {
            Exit::Failure { failure, span } => {
                self.instance.poisoned |= poisons(failure);
                self.report.execution(failure, span);
            }
            Exit::Return(Some(slot)) => {
                if !had_pending
                    && self
                        .declared(slot)
                        .is_ok_and(|v| v.kind == Kind::Owner && v.payload.is_some())
                {
                    let (payload, span) = self.remove(slot);
                    returned = Some((slot, payload, span));
                } else {
                    self.instance.poisoned = true;
                    self.report.execution(ExecutionFailure::Invariant, None);
                }
            }
            Exit::Return(None) => {}
        }
        if let Some(depth) = nested_depth {
            // An entry-wide abort still completes the callee's exit before
            // retiring its callers. A callee result cannot become an entry result.
            if self.report.primary.is_none() {
                self.instance.poisoned = true;
                self.report.execution(ExecutionFailure::Invariant, None);
            }
            self.drain_from(depth, Reason::Failure);
            if let Some((slot, payload, span)) = returned.take() {
                self.destroy(slot, payload, span, Reason::UnpublishedReturn);
            }
        }
        self.drain_all(if self.report.primary.is_some() {
            Reason::Failure
        } else {
            Reason::ScopeExit
        });
        let value = match returned {
            Some((_, payload, _)) if self.report.primary.is_none() => {
                Some(ManuallyDrop::into_inner(payload))
            }
            Some((slot, payload, span)) => {
                self.destroy(slot, payload, span, Reason::UnpublishedReturn);
                None
            }
            None => None,
        };
        self.finished = true;
        Completion {
            value,
            report: Report {
                primary: self.report.primary.take(),
                secondary: std::mem::take(&mut self.report.secondary),
                limit: self.report.limit,
                truncated: self.report.truncated,
                quarantined: self.report.quarantined,
                cleanup_failures: self.report.cleanup_failures,
            },
            poisoned: self.instance.poisoned,
        }
    }
}
impl<T, C: Cleaner<T>> Drop for Frame<'_, T, C> {
    fn drop(&mut self) {
        if !self.finished {
            if let Some(pending) = self.pending.take() {
                self.drain_from(pending.depth, Reason::Failure);
                if let Some((slot, payload, span)) = pending.returned {
                    self.destroy(slot, payload, span, Reason::UnpublishedReturn);
                }
            }
            self.drain_all(Reason::Failure);
        }
    }
}

#[cfg(test)]
mod report_tests {
    use super::*;

    #[test]
    fn final_invariant_preserves_the_primary_and_the_original_record_bound() {
        for records in [1, 2] {
            let mut budget = lang_mir::unit::Budget::new(Default::default());
            let mut session =
                Session::prepare(std::num::NonZeroUsize::new(records).unwrap(), &mut budget)
                    .unwrap();
            session.execution(ExecutionFailure::FuelLimit, None);
            let mut report = session.into_report();
            report.append_invariant(None);
            assert_eq!(
                report.primary().unwrap().kind,
                FailureKind::Execution(ExecutionFailure::FuelLimit)
            );
            if records == 1 {
                assert!(report.secondary().is_empty());
                assert_eq!(report.truncated(), 1);
            } else {
                assert_eq!(
                    report.secondary()[0].kind,
                    FailureKind::Execution(ExecutionFailure::Invariant)
                );
                assert_eq!(report.truncated(), 0);
            }
        }
    }
}
