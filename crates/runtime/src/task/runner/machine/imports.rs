//! Typed native imports share one path for interpreted/generated guest bodies.
use super::*;
use crate::component::execution::{self, ReadRoot};

impl<P: a::Provider> Machine<'_, '_, '_, '_, '_, P> {
    pub fn import_native(
        &mut self,
        task: usize,
        import: m::ImportId,
        arguments: &[m::Register],
        to: usize,
        span: m::Span,
    ) -> Result<(), Reason> {
        self.control.check()?;
        self.control.charge_host_call(&mut self.usage)?;
        let mut roots = self.budget.vector(arguments.len())?;
        {
            let frame = frame::Access::new(&mut self.records[task].frames, &mut self.services)?;
            for argument in arguments {
                let slot = frame.value(*argument);
                frame.available(slot)?;
                if frame.is_reference(slot)? {
                    roots.push(ReadRoot::Resource(
                        frame.native_resource(slot, self.allocator)?,
                    ));
                } else {
                    roots.push(ReadRoot::Value(frame.data()?.cell(slot)?.observe()?));
                }
            }
        }
        let types = self.proof.typed().places().declarations().types();
        let layouts = self.services.layouts;
        let values = {
            let allocator = &*self.allocator;
            let mut read = |session: &mut crate::component::wire::Session| {
                execution::read_parameters(&roots, layouts, types, allocator, session)
            };
            self.services
                .component
                .as_deref_mut()
                .ok_or(Reason::Unsupported)?
                .read_native_arguments(&mut read)?
        };
        let result = self
            .services
            .component
            .as_deref_mut()
            .ok_or(Reason::Unsupported)?
            .invoke_native(import, values, self.allocator, &mut self.budget)?;
        self.control.check()?;
        if result.roots.len() != 1 {
            return Err(Reason::Invariant);
        }
        let mut ready = self.budget.vector(1)?;
        let record = &mut self.records[task];
        let frame = record.frames.last_mut().ok_or(Reason::Invariant)?;
        let session = record.session.as_mut().ok_or(Reason::Invariant)?;
        if let Err(reason) = frame.stage_values(
            to,
            result.roots[0],
            &result,
            None,
            self.allocator,
            &mut self.budget,
            &mut ready,
            &mut self.services.drain,
            session,
        ) {
            if !ready.is_empty() {
                let _ = frame.discard_partial(to, self.allocator, &mut self.services, session);
            }
            return Err(reason);
        }
        let mut frame = frame::Access::new(&mut self.records[task].frames, &mut self.services)?;
        frame.mark(to, span)?;
        for argument in arguments {
            let slot = frame.value(*argument);
            if frame.is_owner(slot)? {
                frame.cleanup_slot(slot, c::Reason::ScopeExit, self.allocator)?;
            }
        }
        Ok(())
    }
}
