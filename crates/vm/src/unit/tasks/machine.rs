use super::*;
use frame::{ReturnTo, Scope};

struct Record<'l, 'ty> {
    id: t::TaskId,
    function: m::FunctionId,
    parent: Option<usize>,
    scope: u64,
    frames: Vec<Frame<'l, 'ty>>,
    result: Option<l::Reservation<'l, 'ty>>,
}
struct Fault {
    reason: Reason,
    function: m::FunctionId,
    span: m::Span,
}
pub(super) struct Machine<'p, 'l, 'ty, 'host, 'ctrl, P: a::Provider> {
    proof: &'ty o::tasks::VerifiedTasks,
    plans: &'p [Option<l::FramePlan<'l, 'ty>>],
    allocator: &'host mut a::Allocator<P>,
    control: e::Control<'ctrl>,
    budget: m::Budget,
    scheduler: t::Scheduler,
    records: Vec<Record<'l, 'ty>>,
    faults: Vec<Option<Fault>>,
    events: Vec<Event>,
    trace: bool,
    trace_limit: usize,
    usage: e::Usage,
    live_frames: usize,
    live_cells: usize,
    active: Option<(t::TaskId, bool)>,
}
impl<'p, 'l, 'ty, 'host, 'ctrl, P: a::Provider> Machine<'p, 'l, 'ty, 'host, 'ctrl, P> {
    pub fn new(
        proof: &'ty o::tasks::VerifiedTasks,
        plans: &'p [Option<l::FramePlan<'l, 'ty>>],
        allocator: &'host mut a::Allocator<P>,
        control: e::Control<'ctrl>,
        limits: Limits,
        mut budget: m::Budget,
        entry: m::FunctionId,
    ) -> Result<Self, Reason> {
        let plan = plans[entry.0 as usize].as_ref().ok_or(Reason::Invariant)?;
        let cells = plan.cleanup_slots() + 1;
        admit(control, 0, 0, cells)?;
        let mut records = budget.vector(limits.tasks.total_tasks)?;
        let mut faults = budget.vector(limits.tasks.total_tasks)?;
        faults.resize_with(limits.tasks.total_tasks, || None);
        let events = budget.vector(limits.trace_events)?;
        let mut frames = budget.vector(1)?;
        let mut scheduler = t::Scheduler::new(limits.tasks)?;
        let frame = Frame::new(proof, plan, entry, allocator, &mut budget)?;
        let span = proof.typed().places().declarations().functions()[entry.0 as usize].span;
        let id = scheduler.start(Some(span))?;
        frames.push(frame);
        records.push(Record {
            id,
            function: entry,
            parent: None,
            scope: 0,
            frames,
            result: None,
        });
        Ok(Self {
            proof,
            plans,
            allocator,
            control,
            budget,
            scheduler,
            records,
            faults,
            events,
            trace: limits.trace_events != 0,
            trace_limit: limits.trace_events,
            usage: e::Usage {
                peak_frames: 1,
                peak_live_cells: cells,
                ..Default::default()
            },
            live_frames: 1,
            live_cells: cells,
            active: None,
        })
    }
    fn event(&mut self, task: usize, kind: EventKind, span: m::Span) -> Result<(), Reason> {
        if !self.trace {
            return Ok(());
        }
        if self.events.len() == self.trace_limit {
            self.trace = false;
            return Err(Reason::TraceLimit);
        }
        let function = self.records[task]
            .frames
            .last()
            .map(|f| f.function)
            .unwrap_or(self.records[task].function);
        self.events.push(Event {
            task,
            function,
            kind,
            span,
        });
        Ok(())
    }
    fn frame(&self, task: usize) -> Result<&Frame<'l, 'ty>, Reason> {
        self.records[task].frames.last().ok_or(Reason::Invariant)
    }
    fn frame_mut(&mut self, task: usize) -> Result<&mut Frame<'l, 'ty>, Reason> {
        self.records[task]
            .frames
            .last_mut()
            .ok_or(Reason::Invariant)
    }
    fn fault(&mut self, task: usize, reason: Reason, span: m::Span) -> Result<usize, Reason> {
        if let Some(index) = self.frame(task)?.fault {
            return Ok(index);
        }
        let function = self.frame(task)?.function;
        self.faults[task] = Some(Fault {
            reason,
            function,
            span,
        });
        self.frame_mut(task)?.fault = Some(task);
        Ok(task)
    }
    fn retired(&mut self, function: m::FunctionId) {
        self.live_frames -= 1;
        self.live_cells -= self.plans[function.0 as usize]
            .as_ref()
            .unwrap()
            .cleanup_slots()
            + 1;
    }
    fn prepare_frame(&mut self, function: m::FunctionId) -> Result<Frame<'l, 'ty>, Reason> {
        let plan = self.plans[function.0 as usize]
            .as_ref()
            .ok_or(Reason::Invariant)?;
        let cells = plan.cleanup_slots() + 1;
        admit(self.control, self.live_frames, self.live_cells, cells)?;
        let frame = Frame::new(self.proof, plan, function, self.allocator, &mut self.budget)?;
        self.live_frames += 1;
        self.live_cells += cells;
        self.usage.peak_frames = self.usage.peak_frames.max(self.live_frames);
        self.usage.peak_live_cells = self.usage.peak_live_cells.max(self.live_cells);
        Ok(frame)
    }
    fn discard_result(&mut self, task: usize) -> Result<(), Reason> {
        if let Some(mut result) = self.records[task].result.take() {
            result.release_storage(self.allocator)?;
        }
        Ok(())
    }
    fn pending(&mut self, task: usize) -> Result<bool, Reason> {
        let Some(pending) = self.frame_mut(task)?.pending.take() else {
            return Ok(true);
        };
        match pending {
            Pending::Await {
                child,
                destination,
                next,
                failure,
            } => {
                let outcome = match self.scheduler.status(&self.records[child].id)? {
                    t::Status::Complete(outcome) => outcome,
                    _ => return Err(Reason::Invariant),
                };
                match outcome {
                    t::Outcome::Failed(id) => {
                        let frame = self.frame_mut(task)?;
                        frame.fault = Some(id.0 as usize);
                        frame.node = failure;
                    }
                    t::Outcome::Success | t::Outcome::Cancelled => {
                        let [parent, child] = self
                            .records
                            .get_disjoint_mut([task, child])
                            .map_err(|_| Reason::Invariant)?;
                        let frame = parent.frames.last_mut().ok_or(Reason::Invariant)?;
                        let cell = frame.data()?.cell(destination)?;
                        let root = cell.root();
                        let case = if outcome == t::Outcome::Success { 0 } else { 1 };
                        cell.prepare_case(root, case, self.allocator)?;
                        let at = cell.child(root, 0, self.allocator)?;
                        if case == 0 {
                            let mut result = child.result.take().ok_or(Reason::Invariant)?;
                            cell.copy_from(at, &result, result.root(), self.allocator)?;
                            result.release_storage(self.allocator)?;
                        } else {
                            cell.write_immediate(at, l::Immediate::TaskCancelled, self.allocator)?;
                        }
                        frame.live[destination] = true;
                        frame.node = next;
                    }
                }
                Ok(true)
            }
            Pending::Scope {
                scope,
                mut cursor,
                next,
                failure,
            } => {
                while cursor < self.records.len() {
                    let index = cursor;
                    cursor += 1;
                    if self.records[index].parent != Some(task)
                        || self.records[index].scope != scope.serial
                    {
                        continue;
                    }
                    match self.scheduler.status(&self.records[index].id)? {
                        t::Status::Complete(t::Outcome::Failed(id)) => {
                            let frame = self.frame_mut(task)?;
                            frame.fault = Some(id.0 as usize);
                            frame.node = failure;
                            return Ok(true);
                        }
                        t::Status::Complete(_) => self.discard_result(index)?,
                        _ => {
                            self.frame_mut(task)?.pending = Some(Pending::Scope {
                                scope,
                                cursor: index,
                                next,
                                failure,
                            });
                            self.scheduler
                                .join(&self.records[task].id, &self.records[index].id)?;
                            self.active = None;
                            return Ok(false);
                        }
                    }
                }
                let function = self.frame(task)?.function;
                for binding in &self.proof.typed().functions()[function.0 as usize].task_bindings {
                    if binding.scope == scope.id {
                        let frame = self.frame_mut(task)?;
                        let slot = match binding.target {
                            b::CleanupTarget::Local(id) => id.0 as usize,
                            b::CleanupTarget::Value(id) => frame.value(id),
                            _ => return Err(Reason::Invariant),
                        };
                        frame.forget(slot);
                    }
                }
                let frame = self.frame_mut(task)?;
                if frame.scopes.pop().map(|s| s.serial) != Some(scope.serial) {
                    return Err(Reason::Invariant);
                }
                frame.node = next;
                Ok(true)
            }
        }
    }
    fn call(
        &mut self,
        task: usize,
        function: m::FunctionId,
        arguments: &[m::Register],
        destination: usize,
        next: f::NodeId,
        failure: f::NodeId,
    ) -> Result<(), Reason> {
        reserve_push(&mut self.budget, &mut self.records[task].frames)?;
        let mut child = self.prepare_frame(function)?;
        let transferred = (|| {
            let parent = self.records[task].frames.last().ok_or(Reason::Invariant)?;
            for (argument, parameter) in arguments.iter().zip(
                &self.proof.typed().places().declarations().functions()[function.0 as usize]
                    .parameters,
            ) {
                child.copy_from_frame(
                    parameter.0 as usize,
                    parent,
                    parent.value(*argument),
                    self.allocator,
                )?;
            }
            Ok::<_, Reason>(())
        })();
        if let Err(error) = transferred {
            child.release(self.allocator)?;
            self.retired(function);
            return Err(error);
        }
        child.returned = Some(ReturnTo {
            destination,
            next,
            failure,
        });
        self.records[task].frames.push(child);
        Ok(())
    }
    fn spawn(
        &mut self,
        task: usize,
        function: m::FunctionId,
        arguments: &[m::Register],
        destination: usize,
        span: m::Span,
    ) -> Result<(), Reason> {
        self.scheduler.check_spawn(&self.records[task].id)?;
        let mut frames = self.budget.vector(1)?;
        let scope = self
            .frame(task)?
            .scopes
            .last()
            .copied()
            .ok_or(Reason::Invariant)?;
        let mut frame = self.prepare_frame(function)?;
        let transferred = (|| {
            let parent = self.records[task].frames.last().ok_or(Reason::Invariant)?;
            for (argument, parameter) in arguments.iter().zip(
                &self.proof.typed().places().declarations().functions()[function.0 as usize]
                    .parameters,
            ) {
                frame.copy_from_frame(
                    parameter.0 as usize,
                    parent,
                    parent.value(*argument),
                    self.allocator,
                )?;
            }
            self.control.check()?;
            Ok::<_, Reason>(())
        })();
        if let Err(error) = transferred {
            frame.release(self.allocator)?;
            self.retired(function);
            return Err(error);
        }
        let child = self.scheduler.spawn(&self.records[task].id, Some(span))?;
        let index = child.index();
        frames.push(frame);
        self.records.push(Record {
            id: child.clone(),
            function,
            parent: Some(task),
            scope: scope.serial,
            frames,
            result: None,
        });
        let parent = self.frame_mut(task)?;
        parent.claims[destination] = Some(child);
        parent.live[destination] = true;
        self.event(task, EventKind::Spawn { child: index }, span)
    }

    fn resume(&mut self, task: usize) -> Result<(), Reason> {
        if !self.pending(task)? {
            return Ok(());
        }
        loop {
            let frame = self.frame(task)?;
            let function = frame.function;
            let node =
                self.proof.plan().functions()[function.0 as usize].nodes()[frame.node.0 as usize];
            let next = match node.kind {
                f::Kind::Entry { next, .. } => Some(next),
                f::Kind::Fuel { next, failure, .. } => match self.control.charge(&mut self.usage) {
                    Ok(()) => Some(next),
                    Err(reason) => {
                        self.fault(task, reason.into(), node.span)?;
                        Some(failure)
                    }
                },
                f::Kind::Execute {
                    site,
                    next,
                    failure,
                } => {
                    let i = &self.proof.typed().functions()[function.0 as usize].blocks
                        [site.block.0 as usize]
                        .instructions[site.order - 1];
                    let output = i
                        .results
                        .first()
                        .map(|r| self.frame(task).map(|f| f.value(*r)))
                        .transpose()?;
                    let applied = (|| -> Result<bool, Reason> {
                        match &i.op {
                            b::Op::Call {
                                function,
                                arguments,
                            } => {
                                self.call(
                                    task,
                                    *function,
                                    arguments,
                                    output.ok_or(Reason::Invariant)?,
                                    next,
                                    failure,
                                )?;
                                return Ok(false);
                            }
                            b::Op::TaskSpawn {
                                function,
                                arguments,
                            } => self.spawn(
                                task,
                                *function,
                                arguments,
                                output.ok_or(Reason::Invariant)?,
                                node.span,
                            )?,
                            b::Op::TaskAwait(value) => {
                                let frame = self.frame_mut(task)?;
                                let slot = frame.value(*value);
                                frame.available(slot)?;
                                let child = frame.claims[slot].take().ok_or(Reason::Invariant)?;
                                frame.live[slot] = false;
                                frame.pending = Some(Pending::Await {
                                    child: child.index(),
                                    destination: output.ok_or(Reason::Invariant)?,
                                    next,
                                    failure,
                                });
                                self.event(
                                    task,
                                    EventKind::Await {
                                        child: child.index(),
                                    },
                                    node.span,
                                )?;
                                self.scheduler.join(&self.records[task].id, &child)?;
                                self.active = None;
                                return Ok(false);
                            }
                            b::Op::TaskCancel(place) => {
                                let place = &self.proof.typed().places().functions()
                                    [function.0 as usize]
                                    .places[place.0 as usize];
                                let frame = self.frame(task)?;
                                let location = frame.place(place, self.allocator)?;
                                let child = frame.claims[location.slot]
                                    .as_ref()
                                    .ok_or(Reason::Invariant)?
                                    .clone();
                                self.scheduler.request_cancel(&child)?;
                            }
                            b::Op::TaskYield => {
                                self.event(task, EventKind::Yield, node.span)?;
                                self.frame_mut(task)?.node = next;
                                self.scheduler.yield_now(&self.records[task].id)?;
                                self.active = None;
                                return Ok(false);
                            }
                            b::Op::TaskCheck => {
                                self.event(task, EventKind::Check, node.span)?;
                                self.frame_mut(task)?.node = next;
                                if self.scheduler.cancellation_point(&self.records[task].id)?
                                    == t::Boundary::Closing
                                {
                                    self.active = None;
                                    return Ok(false);
                                }
                            }
                            b::Op::TaskScopeEnter(id) => {
                                let frame = self.frame_mut(task)?;
                                frame.serial =
                                    frame.serial.checked_add(1).ok_or(Reason::Invariant)?;
                                frame.scopes.push(Scope {
                                    id: *id,
                                    serial: frame.serial,
                                });
                                self.event(task, EventKind::ScopeEnter(id.0), node.span)?;
                            }
                            b::Op::TaskScopeExit(id) => {
                                let scope = self
                                    .frame(task)?
                                    .scopes
                                    .last()
                                    .copied()
                                    .ok_or(Reason::Invariant)?;
                                if scope.id != *id {
                                    return Err(Reason::Invariant);
                                }
                                self.event(task, EventKind::ScopeExit(id.0), node.span)?;
                                self.frame_mut(task)?.pending = Some(Pending::Scope {
                                    scope,
                                    cursor: 0,
                                    next,
                                    failure,
                                });
                                return self.pending(task);
                            }
                            b::Op::TaskSaveReturn(value) => {
                                let frame = self.records[task]
                                    .frames
                                    .last_mut()
                                    .ok_or(Reason::Invariant)?;
                                frame.transfer(
                                    frame.value(*value),
                                    frame.return_cell,
                                    self.allocator,
                                )?;
                                frame.saved = true;
                            }
                            _ => {
                                let frame = self.records[task]
                                    .frames
                                    .last_mut()
                                    .ok_or(Reason::Invariant)?;
                                instructions::execute(
                                    self.proof.typed(),
                                    frame,
                                    site,
                                    self.allocator,
                                )?;
                            }
                        }
                        Ok(true)
                    })();
                    match applied {
                        Ok(true) => Some(next),
                        Ok(false) => {
                            if self.active.is_none() {
                                return Ok(());
                            }
                            continue;
                        }
                        Err(reason) => {
                            self.fault(task, reason, node.span)?;
                            Some(failure)
                        }
                    }
                }
                f::Kind::Transfer {
                    block, arm, target, ..
                } => {
                    let body = &self.proof.typed().functions()[function.0 as usize];
                    let edge = body.blocks[block.0 as usize]
                        .terminator
                        .kind
                        .targets()
                        .nth(arm as usize)
                        .ok_or(Reason::Invariant)?;
                    let parameters = &body.blocks[edge.block.0 as usize].parameters;
                    let plan = self.plans[function.0 as usize]
                        .as_ref()
                        .ok_or(Reason::Invariant)?;
                    let frame = self.records[task]
                        .frames
                        .last_mut()
                        .ok_or(Reason::Invariant)?;
                    for (argument, parameter) in edge.arguments.iter().zip(parameters) {
                        frame.transfer(
                            frame.value(*argument),
                            plan.scratch_cell(*parameter).ok_or(Reason::Invariant)?,
                            self.allocator,
                        )?;
                    }
                    for parameter in parameters {
                        frame.transfer(
                            plan.scratch_cell(*parameter).ok_or(Reason::Invariant)?,
                            frame.value(*parameter),
                            self.allocator,
                        )?;
                    }
                    Some(target)
                }
                f::Kind::Branch {
                    condition,
                    then_node,
                    else_node,
                    ..
                } => {
                    let value = self
                        .frame(task)?
                        .read(self.frame(task)?.value(condition), self.allocator)?;
                    match value {
                        l::Immediate::Bool(true) => Some(then_node),
                        l::Immediate::Bool(false) => Some(else_node),
                        _ => return Err(Reason::Invariant),
                    }
                }
                f::Kind::StageReturn { value, next, .. } => {
                    let frame = self.records[task]
                        .frames
                        .last_mut()
                        .ok_or(Reason::Invariant)?;
                    frame.transfer(frame.value(value), frame.return_cell, self.allocator)?;
                    frame.saved = true;
                    Some(next)
                }
                f::Kind::DiscardPrepared { next } => {
                    self.frame_mut(task)?.prepared.fill(None);
                    Some(next)
                }
                f::Kind::RetireReturn { next, .. } => Some(next),
                f::Kind::PublishReturn => {
                    if self.records[task].frames.len() == 1 {
                        self.scheduler
                            .finish(&self.records[task].id, t::Outcome::Success)?;
                        self.active = None;
                        return Ok(());
                    }
                    let mut child = self.records[task].frames.pop().ok_or(Reason::Invariant)?;
                    let returned = child.returned.ok_or(Reason::Invariant)?;
                    let parent = self.records[task]
                        .frames
                        .last_mut()
                        .ok_or(Reason::Invariant)?;
                    parent.copy_from_frame(
                        returned.destination,
                        &child,
                        child.return_cell,
                        self.allocator,
                    )?;
                    parent.node = returned.next;
                    child.release(self.allocator)?;
                    self.retired(child.function);
                    continue;
                }
                f::Kind::Raise { failure, next } => {
                    self.fault(task, Reason::Execution(failure), node.span)?;
                    Some(next)
                }
                f::Kind::RecordFailure { next } => {
                    if self.frame(task)?.fault.is_none() {
                        self.fault(task, Reason::Invariant, node.span)?;
                    }
                    Some(next)
                }
                f::Kind::RetireFailure { next } => Some(next),
                f::Kind::DiscardResult { next } => {
                    let frame = self.frame_mut(task)?;
                    frame.live[frame.return_cell] = false;
                    frame.saved = false;
                    Some(next)
                }
                f::Kind::EndTaskLoans { next } => Some(next),
                f::Kind::CancelTaskScopes { next } => {
                    self.frame_mut(task)?.node = next;
                    let failure = self.frame(task)?.fault.ok_or(Reason::Invariant)?;
                    self.scheduler.finish(
                        &self.records[task].id,
                        t::Outcome::Failed(t::FailureId(failure as u64)),
                    )?;
                    self.active = None;
                    return Ok(());
                }
                f::Kind::PropagateFailure => {
                    let failure = self.frame(task)?.fault.ok_or(Reason::Invariant)?;
                    if self.records[task].frames.len() == 1 {
                        self.scheduler.finish(
                            &self.records[task].id,
                            t::Outcome::Failed(t::FailureId(failure as u64)),
                        )?;
                        self.active = None;
                        return Ok(());
                    }
                    let mut child = self.records[task].frames.pop().ok_or(Reason::Invariant)?;
                    let returned = child.returned.ok_or(Reason::Invariant)?;
                    child.release(self.allocator)?;
                    self.retired(child.function);
                    let parent = self.frame_mut(task)?;
                    parent.fault = Some(failure);
                    parent.node = returned.failure;
                    continue;
                }
                f::Kind::JoinTaskScopes { .. } | f::Kind::FinishTask => {
                    return Err(Reason::Invariant);
                }
            };
            if let Some(next) = next {
                self.frame_mut(task)?.node = next;
            }
        }
    }

    fn cleanup(&mut self, task: usize, outcome: t::Outcome) -> Result<(), Reason> {
        let function = self.records[task].function;
        let span = self.proof.typed().places().declarations().functions()[function.0 as usize].span;
        self.event(task, EventKind::Cleanup(outcome), span)?;
        if outcome != t::Outcome::Success
            && let Some(entry) = self.proof.plan().functions()[function.0 as usize].cancellation()
        {
            let current = self.frame(task)?.node;
            let start = if matches!(
                self.proof.plan().functions()[function.0 as usize].nodes()[current.0 as usize].kind,
                f::Kind::JoinTaskScopes { .. }
            ) {
                current
            } else {
                entry
            };
            let mut at = start;
            loop {
                let node =
                    self.proof.plan().functions()[function.0 as usize].nodes()[at.0 as usize];
                at = match node.kind {
                    f::Kind::DiscardPrepared { next } => {
                        self.frame_mut(task)?.prepared.fill(None);
                        next
                    }
                    f::Kind::EndTaskLoans { next } => next,
                    f::Kind::CancelTaskScopes { next } => {
                        for child in &self.records {
                            if child.parent == Some(task) {
                                self.scheduler.request_cancel(&child.id)?;
                            }
                        }
                        next
                    }
                    f::Kind::JoinTaskScopes { next } => {
                        for child in &self.records {
                            if child.parent == Some(task)
                                && !matches!(
                                    self.scheduler.status(&child.id)?,
                                    t::Status::Complete(_)
                                )
                            {
                                return Err(Reason::Invariant);
                            }
                        }
                        next
                    }
                    f::Kind::RetireFailure { next } => next,
                    f::Kind::DiscardResult { next } => {
                        let frame = self.frame_mut(task)?;
                        frame.saved = false;
                        frame.live[frame.return_cell] = false;
                        next
                    }
                    f::Kind::FinishTask => break,
                    _ => return Err(Reason::Invariant),
                };
            }
        }
        for child in 0..self.records.len() {
            if self.records[child].parent == Some(task) {
                self.discard_result(child)?;
            }
        }
        while let Some(mut frame) = self.records[task].frames.pop() {
            if self.records[task].frames.is_empty() && outcome == t::Outcome::Success && frame.saved
            {
                self.records[task].result = Some(frame.result(self.allocator)?);
            } else {
                frame.release(self.allocator)?;
            }
            self.retired(frame.function);
        }
        let actual = self
            .scheduler
            .finish_cleanup(&self.records[task].id, None)?;
        self.active = None;
        if actual != t::Outcome::Success {
            self.discard_result(task)?;
        }
        self.event(task, EventKind::Complete(actual), span)
    }
    fn abort(&mut self, reason: Reason) -> Result<(), Reason> {
        let function = self.records[0].function;
        let span = self.proof.typed().places().declarations().functions()[function.0 as usize].span;
        if self.faults[0].is_none() {
            self.faults[0] = Some(Fault {
                reason,
                function,
                span,
            });
        }
        self.trace = false;
        if let Some((task, cleaning)) = self.active.take() {
            if cleaning {
                let index = task.index();
                while let Some(mut frame) = self.records[index].frames.pop() {
                    frame.release(self.allocator)?;
                    self.retired(frame.function);
                }
                self.discard_result(index)?;
                self.scheduler
                    .finish_cleanup(&task, Some(t::FailureId(0)))?;
            } else {
                self.scheduler
                    .finish(&task, t::Outcome::Failed(t::FailureId(0)))?;
            }
        }
        self.scheduler.request_cancel(&self.records[0].id)?;
        while let Some(dispatch) = self.scheduler.dispatch()? {
            match dispatch {
                t::Dispatch::Resume(task) => self
                    .scheduler
                    .finish(&task, t::Outcome::Failed(t::FailureId(0)))?,
                t::Dispatch::Cleanup { task, .. } => {
                    let index = task.index();
                    while let Some(mut frame) = self.records[index].frames.pop() {
                        frame.release(self.allocator)?;
                        self.retired(frame.function);
                    }
                    self.discard_result(index)?;
                    self.scheduler.finish_cleanup(&task, None)?;
                }
            }
        }
        for task in 0..self.records.len() {
            self.discard_result(task)?;
        }
        Ok(())
    }
    pub fn run(mut self) -> Result<Outcome, Failure> {
        let driven = (|| -> Result<(), Reason> {
            while let Some(dispatch) = self.scheduler.dispatch()? {
                match dispatch {
                    t::Dispatch::Resume(task) => {
                        let index = task.index();
                        self.active = Some((task, false));
                        let frame = self.frame(index)?;
                        let span = self.proof.plan().functions()[frame.function.0 as usize].nodes()
                            [frame.node.0 as usize]
                            .span;
                        self.event(index, EventKind::Resume, span)?;
                        self.resume(index)?;
                    }
                    t::Dispatch::Cleanup { task, outcome } => {
                        let index = task.index();
                        self.active = Some((task, true));
                        self.cleanup(index, outcome)?;
                    }
                }
            }
            if !self.scheduler.is_finished() {
                return Err(Reason::Invariant);
            }
            Ok(())
        })();
        let failed = driven.is_err();
        if let Err(reason) = driven
            && let Err(reason) = self.abort(reason)
        {
            let function = self.records[0].function;
            let span =
                self.proof.typed().places().declarations().functions()[function.0 as usize].span;
            self.faults[0] = Some(Fault {
                reason,
                function,
                span,
            });
        }
        let outcome = self.scheduler.status(&self.records[0].id);
        let finished = (|| -> Result<i64, Reason> {
            if failed {
                return Err(Reason::Invariant);
            }
            match outcome.map_err(Reason::from)? {
                t::Status::Complete(t::Outcome::Success) => {
                    let mut result = self.records[0].result.take().ok_or(Reason::Invariant)?;
                    let value = result.read_immediate(result.root(), self.allocator)?;
                    result.release_storage(self.allocator)?;
                    match value {
                        l::Immediate::I64(value) => Ok(value),
                        _ => Err(Reason::Invariant),
                    }
                }
                t::Status::Complete(t::Outcome::Cancelled) => Err(Reason::Cancelled),
                t::Status::Complete(t::Outcome::Failed(_)) => Err(Reason::Invariant),
                _ => Err(Reason::Invariant),
            }
        })();
        for task in 0..self.records.len() {
            if let Err(reason) = self.discard_result(task) {
                return Err(Failure {
                    reason,
                    function: Some(self.records[task].function),
                    span: None,
                    usage: self.usage,
                    tasks: self.scheduler.usage(),
                    events: self.events,
                });
            }
        }
        match finished {
            Ok(value) => Ok(Outcome {
                value,
                usage: self.usage,
                tasks: self.scheduler.usage(),
                allocations: self.allocator.stats(),
                events: self.events,
            }),
            Err(reason) => {
                let failure = match self.scheduler.status(&self.records[0].id) {
                    Ok(t::Status::Complete(t::Outcome::Failed(id))) => id.0 as usize,
                    _ => 0,
                };
                let fault = self.faults.get_mut(failure).and_then(Option::take);
                let (reason, function, span) = if let Some(fault) = fault {
                    (fault.reason, Some(fault.function), Some(fault.span))
                } else {
                    (
                        reason,
                        Some(self.records[0].function),
                        Some(
                            self.proof.typed().places().declarations().functions()
                                [self.records[0].function.0 as usize]
                                .span,
                        ),
                    )
                };
                Err(Failure {
                    reason,
                    function,
                    span,
                    usage: self.usage,
                    tasks: self.scheduler.usage(),
                    events: self.events,
                })
            }
        }
    }
}
fn admit(control: e::Control<'_>, frames: usize, cells: usize, add: usize) -> Result<(), Reason> {
    control.check()?;
    if control.limits().frames.is_some_and(|limit| frames >= limit) {
        return Err(Reason::Execution(f::ExecutionFailure::FrameLimit));
    }
    let cells = cells.checked_add(add).ok_or(Reason::Invariant)?;
    if control
        .limits()
        .live_cells
        .is_some_and(|limit| cells > limit)
    {
        return Err(Reason::Execution(f::ExecutionFailure::CellLimit));
    }
    Ok(())
}
