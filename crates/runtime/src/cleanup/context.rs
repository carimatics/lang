//! Shared bounded failure reporting for generated aggregate cleanup.
//! Callers reserve iterative retirement work storage during normal allocation
//! or frame construction. The context invokes no guest body and allocates no
//! bookkeeping; its individual trusted cleanup calls are contained separately.

use super::{
    CodeOwnerLease, FailureKind, FailureRecord, Instance, NativeCleanupKind, NativeOrigin, Reason,
    Report, Slot,
};
use lang_mir::unit::Span;
use std::mem::ManuallyDrop;
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Trusted generated/registered cleanup, never an implicit guest destructor.
/// Mark internal attempts consumed before returning; Rust payload disposal must
/// not retry them. Aggregate traversal must continue after individual failures
/// and use previously reserved iterative work storage for unbounded shapes.
pub trait Cleaner<T> {
    fn cleanup(
        &mut self,
        value: &mut T,
        reason: Reason,
        context: &mut Context<'_>,
    ) -> Result<(), u32>;
}
impl<T, F: FnMut(&mut T, Reason) -> Result<(), u32>> Cleaner<T> for F {
    fn cleanup(&mut self, value: &mut T, reason: Reason, _: &mut Context<'_>) -> Result<(), u32> {
        self(value, reason)
    }
}
/// Adapter for contextual generated cleanup; ordinary two-argument callbacks
/// remain supported through the blanket `Cleaner` implementation.
pub struct Contextual<F>(pub(super) F);
impl<F> Contextual<F> {
    pub const fn new(cleaner: F) -> Self {
        Self(cleaner)
    }
}
impl<T, F> Cleaner<T> for Contextual<F>
where
    F: FnMut(&mut T, Reason, &mut Context<'_>) -> Result<(), u32>,
{
    fn cleanup(
        &mut self,
        value: &mut T,
        reason: Reason,
        context: &mut Context<'_>,
    ) -> Result<(), u32> {
        (self.0)(value, reason, context)
    }
}

pub struct Context<'a> {
    pub(super) instance: &'a mut Instance,
    pub(super) report: &'a mut Report,
    pub(super) slot: Slot,
    pub(super) span: Option<Span>,
    pub(super) reason: Reason,
}
impl Context<'_> {
    pub const fn slot(&self) -> Slot {
        self.slot
    }
    pub const fn span(&self) -> Option<Span> {
        self.span
    }
    pub const fn reason(&self) -> Reason {
        self.reason
    }
    /// Account for optional cleanup evidence that could not be retained.
    /// This does not introduce a failure or change instance poison state.
    pub fn omit(&mut self, count: usize) {
        self.report.truncated = self.report.truncated.saturating_add(count);
    }
    /// Record a generated container-cleanup failure without stopping siblings.
    pub fn fail(&mut self, code: u32) {
        self.failure(FailureKind::Cleanup(code));
    }
    /// Record an attributed native cleanup failure in the original bounded
    /// ledger, retaining its supplied owned registration lease. Borrowed
    /// registrations keep the existing Rust caller lifetime contract.
    pub fn native_failure(
        &mut self,
        kind: NativeCleanupKind,
        origin: Option<NativeOrigin>,
        owner: Option<CodeOwnerLease>,
    ) {
        self.failure_with_owner(FailureKind::NativeCleanup { kind, origin }, owner);
    }
    /// Take an already detached child obligation before invoking its cleanup.
    /// All failures share the entry report and poison state. No new report or
    /// diagnostic string is allocated, including after the buffer overflows.
    pub fn field<T, C: Cleaner<T>>(&mut self, value: T, span: Option<Span>, cleaner: &mut C) {
        let span = span.or(self.span);
        let previous = std::mem::replace(&mut self.span, span);
        let reason = self.reason;
        self.run(ManuallyDrop::new(value), |value, context| {
            cleaner.cleanup(value, reason, context)
        });
        self.span = previous;
    }
    fn failure(&mut self, kind: FailureKind) {
        self.failure_with_owner(kind, None);
    }
    fn failure_with_owner(&mut self, kind: FailureKind, owner: Option<CodeOwnerLease>) {
        self.instance.poisoned = true;
        self.report.record(FailureRecord {
            kind,
            slot: Some(self.slot),
            span: self.span,
            code_owner: owner,
        });
    }
    fn panic_payload(&mut self, payload: Box<dyn std::any::Any + Send>) {
        if let Err(secondary) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
            // Bound disposal even when a panic payload's destructor also fails.
            std::mem::forget(secondary);
            self.report.quarantined = self.report.quarantined.saturating_add(1);
            self.failure(FailureKind::PanicPayloadDropPanic);
        }
    }
    pub(super) fn run<T, F>(&mut self, mut payload: ManuallyDrop<T>, cleaner: F)
    where
        F: FnOnce(&mut T, &mut Self) -> Result<(), u32>,
    {
        match catch_unwind(AssertUnwindSafe(|| cleaner(&mut payload, self))) {
            Ok(Ok(())) => {}
            Ok(Err(code)) => self.fail(code),
            Err(panic) => {
                self.failure(FailureKind::CleanupPanic);
                self.panic_payload(panic);
                self.report.quarantined = self.report.quarantined.saturating_add(1);
                // The partially processed host value is quarantined. Its Drop
                // must not panic a second time or retry the failed callback.
                return;
            }
        }
        if let Err(panic) =
            catch_unwind(AssertUnwindSafe(|| drop(ManuallyDrop::into_inner(payload))))
        {
            self.failure(FailureKind::PayloadDropPanic);
            self.panic_payload(panic);
        }
    }
}
