//! Result receipts never own a second resource store. The original table holds
//! native custody until complete guest storage has been staged and committed.
use super::*;

impl Invocation<'_, '_, '_, '_, '_, '_> {
    pub(super) fn prepare_native_result(
        &mut self,
        result: &mut NativeResult,
        memory: &dyn Memory,
    ) -> Result<(), Reason> {
        if result.originals.is_empty() {
            return Ok(());
        }
        let holder = result.holder.ok_or(Reason::Invariant)?;
        let ticket = self
            .resources
            .prepare_transfer(&result.originals, holder, self.guest, memory)
            .map_err(fault)?;
        result.transfer = Some(ticket);
        for (index, claim) in result.claims.iter_mut().enumerate() {
            *claim = self
                .resources
                .prepared_claim(ticket, index)
                .map_err(fault)?;
        }
        Ok(())
    }
    pub(super) fn abort_native_result(
        &mut self,
        result: &mut NativeResult,
        cleanup: &mut c::Session,
        span: m::Span,
        memory: &mut dyn Memory,
    ) -> Result<(), Reason> {
        if result.committed {
            return Ok(());
        }
        let mut failed = false;
        if let Some(ticket) = result.transfer.take() {
            failed |= self.resources.abort_transfer(ticket).is_err();
        }
        if result.originals.is_empty() {
            return if failed { Err(Reason::Cleanup) } else { Ok(()) };
        }
        let holder = result.holder.ok_or(Reason::Invariant)?;
        for (index, claim) in result.originals.drain(..).enumerate().rev() {
            let mut cleaner = c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
                self.retire_from_holder(claim, holder, Some(span), memory, context)
            });
            failed |= cleanup
                .retire(
                    c::Slot(index),
                    (),
                    Some(span),
                    c::Reason::Failure,
                    &mut cleaner,
                )
                .is_err();
        }
        if failed { Err(Reason::Cleanup) } else { Ok(()) }
    }
}
