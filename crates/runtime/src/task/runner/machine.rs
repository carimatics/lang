use super::*;
use frame::ReturnTo;

pub struct Record<'l, 'ty> {
    pub id: t::TaskId,
    pub function: m::FunctionId,
    pub parent: Option<usize>,
    pub scope: u64,
    pub frames: Vec<Frame<'l, 'ty>>,
    pub result: Option<l::Reservation<'l, 'ty>>,
}
pub struct Fault {
    pub reason: Reason,
    pub function: m::FunctionId,
    pub span: m::Span,
}
pub struct Machine<'p, 'l, 'ty, 'host, 'ctrl, P: a::Provider> {
    pub proof: &'ty o::tasks::VerifiedTasks,
    pub plans: &'p [Option<l::FramePlan<'l, 'ty>>],
    pub allocator: &'host mut a::Allocator<P>,
    pub control: e::Control<'ctrl>,
    pub budget: m::Budget,
    pub scheduler: t::Scheduler,
    pub records: Vec<Record<'l, 'ty>>,
    pub faults: Vec<Option<Fault>>,
    pub events: Vec<Event>,
    pub trace: bool,
    pub trace_limit: usize,
    pub usage: e::Usage,
    pub live_frames: usize,
    pub live_cells: usize,
    pub active: Option<(t::TaskId, bool)>,
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
    pub fn event(&mut self, task: usize, kind: EventKind, span: m::Span) -> Result<(), Reason> {
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
    pub fn frame(&self, task: usize) -> Result<&Frame<'l, 'ty>, Reason> {
        self.records[task].frames.last().ok_or(Reason::Invariant)
    }
    pub fn frame_mut(&mut self, task: usize) -> Result<&mut Frame<'l, 'ty>, Reason> {
        self.records[task]
            .frames
            .last_mut()
            .ok_or(Reason::Invariant)
    }
    pub fn fault(&mut self, task: usize, reason: Reason, span: m::Span) -> Result<usize, Reason> {
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
    pub fn retired(&mut self, function: m::FunctionId) {
        self.live_frames -= 1;
        self.live_cells -= self.plans[function.0 as usize]
            .as_ref()
            .unwrap()
            .cleanup_slots()
            + 1;
    }
    pub fn prepare_frame(&mut self, function: m::FunctionId) -> Result<Frame<'l, 'ty>, Reason> {
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
    pub fn discard_result(&mut self, task: usize) -> Result<(), Reason> {
        if let Some(mut result) = self.records[task].result.take() {
            result.release_storage(self.allocator)?;
        }
        Ok(())
    }
    pub fn pending(&mut self, task: usize) -> Result<bool, Reason> {
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
    pub fn call(
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
    pub fn spawn(
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

    fn cleanup<B: Backend<P>>(
        &mut self,
        backend: &mut B,
        task: usize,
        outcome: t::Outcome,
    ) -> Result<(), Reason> {
        let function = self.records[task].function;
        let span = self.proof.typed().places().declarations().functions()[function.0 as usize].span;
        self.event(task, EventKind::Cleanup(outcome), span)?;
        if outcome != t::Outcome::Success
            && let Some(entry) = self.proof.plan().functions()[function.0 as usize].cancellation()
        {
            if !self.frame(task)?.closing {
                self.frame_mut(task)?.node = entry;
            }
            backend.cleanup(self, task)?;
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
    pub fn run<B: Backend<P>>(mut self, backend: &mut B) -> Result<Outcome, Failure> {
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
                        if self.pending(index)? {
                            while self.active.is_some() {
                                backend.resume(self.proof, &mut self, index)?;
                            }
                        }
                    }
                    t::Dispatch::Cleanup { task, outcome } => {
                        let index = task.index();
                        self.active = Some((task, true));
                        self.cleanup(backend, index, outcome)?;
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
