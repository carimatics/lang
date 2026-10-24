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
        let mut values = {
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
            .invoke_native(import, &mut values, span, self.allocator, &mut self.budget);
        if let Err(reason) = &result {
            self.record_execution(task, reason, span)?;
        }
        if let Some(cleanup) = values.cleanup.take() {
            let record = &mut self.records[task];
            if let Some(managed) = record
                .frames
                .last_mut()
                .and_then(|frame| frame.managed.as_mut())
            {
                managed.session.append(cleanup);
            } else {
                record
                    .session
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .append(cleanup);
            }
        }
        match result {
            Err(reason) => {
                if !values.consumed.is_empty() {
                    let frame = self.records[task]
                        .frames
                        .last_mut()
                        .ok_or(Reason::Invariant)?;
                    for argument in arguments {
                        let slot = frame.value(*argument);
                        if frame.is_owner(slot)? {
                            let _ = frame.cleanup_slot_except(
                                slot,
                                c::Reason::Failure,
                                self.allocator,
                                &mut self.services,
                                &values.consumed,
                            );
                        }
                    }
                }
                Err(reason)
            }
            Ok(mut result) => {
                let mut ready = Vec::new();
                let mut marked = false;
                let staged = (|| -> Result<(), Reason> {
                    let frame = self.records[task]
                        .frames
                        .last_mut()
                        .ok_or(Reason::Invariant)?;
                    let mut failure = None;
                    for argument in arguments {
                        let slot = frame.value(*argument);
                        if frame.is_owner(slot)?
                            && let Err(reason) = frame.cleanup_slot_except(
                                slot,
                                c::Reason::ScopeExit,
                                self.allocator,
                                &mut self.services,
                                &values.consumed,
                            )
                        {
                            failure.get_or_insert(reason);
                        }
                    }
                    if let Some(reason) = failure {
                        return Err(reason);
                    }
                    self.control.check()?;
                    if result.values.roots.len() != 1 {
                        return Err(Reason::Invariant);
                    }
                    ready = self.budget.vector(1)?;
                    let record = &mut self.records[task];
                    let frame = record.frames.last_mut().ok_or(Reason::Invariant)?;
                    frame.stage_values(
                        to,
                        result.values.roots[0],
                        &result.values,
                        Some(frame::ResourceValues::Native(&result)),
                        &self.services.work,
                        self.allocator,
                        &mut self.budget,
                        &mut ready,
                        &mut self.services.drain,
                        record.session.as_mut().ok_or(Reason::Invariant)?,
                    )?;
                    self.control.check()?;
                    frame.mark(to, span)?;
                    marked = true;
                    self.services
                        .component
                        .as_deref_mut()
                        .ok_or(Reason::Invariant)?
                        .commit_native_result(&mut result, self.allocator)?;
                    Ok(())
                })();
                if let Err(reason) = &staged {
                    self.record_execution(task, reason, span)?;
                    let record = &mut self.records[task];
                    let frame = record.frames.last_mut().ok_or(Reason::Invariant)?;
                    if !ready.is_empty() {
                        let _ = frame.discard_partial_except(
                            to,
                            self.allocator,
                            &mut self.services,
                            record.session.as_mut().ok_or(Reason::Invariant)?,
                            &result.claims,
                        );
                    }
                    if marked {
                        frame.forget(to);
                    }
                    let cleanup = if let Some(managed) = frame.managed.as_mut() {
                        &mut managed.session
                    } else {
                        record.session.as_mut().ok_or(Reason::Invariant)?
                    };
                    let _ = self
                        .services
                        .component
                        .as_deref_mut()
                        .ok_or(Reason::Invariant)?
                        .discard_native_result(&mut result, cleanup, span, self.allocator);
                }
                staged
            }
        }
    }
}
