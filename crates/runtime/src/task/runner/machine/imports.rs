//! Typed native imports share one path for interpreted/generated guest bodies.
use super::*;
use crate::component::execution::{self, ReadRoot};

impl<P: a::Provider> Machine<'_, '_, '_, '_, '_, P> {
    pub(super) fn capture_callers(
        &mut self,
        task: usize,
        span: m::Span,
    ) -> Option<execution::GuestCallFrames> {
        let activation = self.frame(task).ok()?.activation;
        let logical = self.records.get(task)?.id.clone();
        let (total, ancestors) = caller_count(&self.records, task).ok()?;
        if ancestors != self.records[task].trace_depth {
            return None;
        }
        let frames = std::mem::take(&mut self.records[task].callers.frames);
        let mut callers =
            write_guest_callers(&self.records, self.proof, task, &logical, total, frames).ok()?;
        if let Some(leaf) = callers.frames.last_mut()
            && leaf.activation == activation
            && leaf.task == logical
        {
            leaf.span = span;
        }
        Some(callers)
    }
    pub fn import_native(
        &mut self,
        task: usize,
        import: m::ImportId,
        arguments: &[m::Register],
        to: usize,
        span: m::Span,
    ) -> Result<(), Reason> {
        self.control.check()?;
        self.charge_host_call()?;
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
                .read_native_arguments(self.budget, &mut read)?
        };
        let parent_live = e::LiveUsage {
            frames: self.live_frames,
            cells: self.value_cells()?,
        };
        let id = self.records[task].id.clone();
        let caller = self.records[task]
            .frames
            .last()
            .ok_or(Reason::Invariant)?
            .function;
        let current = self.services.scheduler.current(&id)?;
        let accounting = self.accounting.as_ref().ok_or(Reason::Invariant)?.clone();
        let logical = current.id().clone();
        let callers = guest_callers(&self.records, self.proof, task, &logical, self.budget)?;
        let result = accounting.with_task(logical, || {
            self.services
                .component
                .as_deref_mut()
                .ok_or(Reason::Unsupported)?
                .invoke_native(
                    import,
                    &mut values,
                    span,
                    caller,
                    callers,
                    self.allocator,
                    self.budget,
                    parent_live,
                    current,
                )
        });
        if let Err(reason) = &result
            && !matches!(reason.primary_cause(), Reason::Cancelled)
        {
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
                        self.budget,
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

pub(super) fn prepare_failure_callers(
    depth: usize,
    budget: &mut m::Budget,
) -> Result<execution::GuestCallFrames, Reason> {
    // One count and one bounded copy pass per admitted ancestor. This work
    // and the storage remain available when the fault exhausts the ledger.
    budget.charge(depth.checked_mul(66).ok_or(Reason::Invariant)?)?;
    Ok(execution::GuestCallFrames {
        frames: budget.vector(64)?,
        omitted: 0,
    })
}

fn guest_callers(
    records: &[Record<'_, '_>],
    proof: &o::tasks::VerifiedTasks,
    task: usize,
    logical: &t::TaskId,
    budget: &mut m::Budget,
) -> Result<execution::GuestCallFrames, Reason> {
    let (total, ancestors) = caller_count(records, task)?;
    budget.charge(total.saturating_add(ancestors))?;
    let frames = budget.vector(total.min(64))?;
    write_guest_callers(records, proof, task, logical, total, frames)
}

fn caller_count(records: &[Record<'_, '_>], task: usize) -> Result<(usize, usize), Reason> {
    let mut at = task;
    let mut total = 0usize;
    let mut ancestors = 0usize;
    loop {
        let record = records.get(at).ok_or(Reason::Invariant)?;
        total = total
            .checked_add(record.frames.len())
            .ok_or(Reason::Invariant)?;
        ancestors = ancestors.checked_add(1).ok_or(Reason::Invariant)?;
        match record.parent {
            Some(parent) if parent < at => at = parent,
            Some(_) => return Err(Reason::Invariant),
            None => break,
        }
    }
    if total == 0 {
        return Err(Reason::Invariant);
    }
    Ok((total, ancestors))
}

fn write_guest_callers(
    records: &[Record<'_, '_>],
    proof: &o::tasks::VerifiedTasks,
    task: usize,
    logical: &t::TaskId,
    total: usize,
    mut frames: Vec<execution::GuestCallFrame>,
) -> Result<execution::GuestCallFrames, Reason> {
    let capacity = total.min(64);
    if capacity == 0 || frames.capacity() < capacity {
        return Err(Reason::Invariant);
    }
    frames.clear();
    let mut cursor = 0usize;
    let mut at = task;
    loop {
        let record = &records[at];
        let id = if at == task { logical } else { &record.id };
        // Inner frames beyond this prefix cannot enter the outermost 64,
        // even when this task has ancestors with no remaining live frames.
        for frame in record.frames.iter().take(capacity).rev() {
            let span = proof
                .plan()
                .functions()
                .get(frame.function.0 as usize)
                .and_then(|function| function.nodes().get(frame.node.0 as usize))
                .ok_or(Reason::Invariant)?
                .span;
            let frame = execution::GuestCallFrame {
                activation: frame.activation,
                task: id.clone(),
                function: frame.function,
                span,
            };
            if frames.len() < capacity {
                frames.push(frame);
            } else {
                frames[cursor] = frame;
                cursor = (cursor + 1) % capacity;
            }
        }
        match record.parent {
            Some(parent) => at = parent,
            None => break,
        }
    }
    if total > capacity {
        frames.rotate_left(cursor);
    }
    frames.reverse();
    Ok(execution::GuestCallFrames {
        frames,
        omitted: total.saturating_sub(capacity) as u64,
    })
}
