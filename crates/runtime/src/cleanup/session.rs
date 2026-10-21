//! Bounded cleanup context for an adapter whose activations do not form one
//! borrowed synchronous entry. The owner/loan ledger remains with that adapter.
use super::*;
use lang_mir::unit as m;
use std::num::NonZeroUsize;

pub struct Session {
    instance: Instance,
    report: Report,
}
impl Session {
    /// Reserve the complete failure buffer before admitting payload ownership.
    pub fn prepare(records: NonZeroUsize, budget: &mut m::Budget) -> Result<Self, m::Error> {
        let secondary = budget.vector(records.get() - 1)?;
        Ok(Self {
            instance: Instance::default(),
            report: Report {
                primary: None,
                secondary,
                limit: records.get(),
                truncated: 0,
                quarantined: 0,
                cleanup_failures: 0,
            },
        })
    }
    pub fn report(&self) -> &Report {
        &self.report
    }
    pub fn is_poisoned(&self) -> bool {
        self.instance.is_poisoned()
    }
    pub fn into_report(self) -> Report {
        self.report
    }
    pub fn execution(&mut self, failure: ExecutionFailure, span: Option<Span>) {
        self.instance.poisoned |= poisons(failure);
        self.report.execution(failure, span);
    }
    /// The caller consumes its obligation before entry. Independent retirement
    /// continues after failure, including a full/truncated diagnostic buffer.
    pub fn retire<T, C: Cleaner<T>>(
        &mut self,
        slot: Slot,
        value: T,
        span: Option<Span>,
        reason: Reason,
        cleaner: &mut C,
    ) -> Result<(), StateError> {
        let before = self.report.cleanup_failures;
        let mut context = Context {
            instance: &mut self.instance,
            report: &mut self.report,
            slot,
            span,
            reason,
        };
        context.field(value, span, cleaner);
        if self.report.cleanup_failures != before {
            Err(StateError::Failed)
        } else {
            Ok(())
        }
    }
    /// Append an already completed task's bounded evidence without allocation.
    /// Callers choose the primary task first, then the required secondary order.
    pub fn append(&mut self, other: Self) {
        self.instance.poisoned |= other.instance.poisoned;
        let cleanup = self
            .report
            .cleanup_failures
            .saturating_add(other.report.cleanup_failures);
        if let Some(record) = other.report.primary {
            self.report.record(record);
        }
        for record in other.report.secondary {
            self.report.record(record);
        }
        self.report.cleanup_failures = cleanup;
        self.report.truncated = self.report.truncated.saturating_add(other.report.truncated);
        self.report.quarantined = self
            .report
            .quarantined
            .saturating_add(other.report.quarantined);
    }
}
