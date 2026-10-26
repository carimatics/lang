use super::*;
use frame::ReturnTo;
mod channels;
mod imports;
mod retirement;

pub struct Record<'l, 'ty> {
    pub id: t::TaskId,
    pub function: m::FunctionId,
    pub parent: Option<usize>,
    pub scope: u64,
    pub frames: Vec<Frame<'l, 'ty>>,
    pub callers: crate::component::execution::GuestCallFrames,
    pub trace_depth: usize,
    pub result: Option<frame::RetainedResult<'l, 'ty>>,
    pub next_result: Option<usize>,
    pub session: Option<c::Session>,
}
pub struct Fault {
    pub reason: Reason,
    pub function: m::FunctionId,
    pub span: m::Span,
    pub callers: Option<crate::component::execution::GuestCallFrames>,
}
pub struct Machine<'p, 'l, 'ty, 'host, 'ctrl, P: a::Provider> {
    pub proof: &'ty o::tasks::VerifiedTasks,
    pub plans: &'p [Option<l::FramePlan<'l, 'ty>>],
    pub allocator: &'host mut a::Allocator<P>,
    pub control: e::Control<'ctrl>,
    pub budget: &'host mut m::Budget,
    pub records: Vec<Record<'l, 'ty>>,
    pub faults: Vec<Option<Fault>>,
    pub events: Vec<Event>,
    pub trace: bool,
    pub trace_limit: usize,
    pub usage: e::Usage,
    local_peak_frames: usize,
    local_peak_cells: usize,
    accounting: Option<e::InvocationAccounting>,
    pub live_frames: usize,
    pub live_cells: usize,
    pub active: Option<(t::TaskId, bool)>,
    pub failure_records: std::num::NonZeroUsize,
    pub completed: Vec<usize>,
    pub next_activation: u64,
    pub services: services::Services<'l, 'ty, 'host>,
}
impl<'p, 'l, 'ty, 'host, 'ctrl, P: a::Provider> Machine<'p, 'l, 'ty, 'host, 'ctrl, P> {
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep verified operands, publication context and allocator explicit at the shared backend boundary."
    )]
    pub fn new(
        proof: &'ty o::tasks::VerifiedTasks,
        plans: &'p [Option<l::FramePlan<'l, 'ty>>],
        layouts: &'l l::Layouts<'ty>,
        allocator: &'host mut a::Allocator<P>,
        control: e::Control<'ctrl>,
        limits: Limits,
        budget: &'host mut m::Budget,
        entry: m::FunctionId,
    ) -> Result<Self, Reason> {
        Self::new_with_arguments(
            proof,
            plans,
            layouts,
            allocator,
            control,
            limits,
            budget,
            entry,
            &[],
        )
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep root transport, verified program, allocator and original controls explicit."
    )]
    pub fn new_with_arguments(
        proof: &'ty o::tasks::VerifiedTasks,
        plans: &'p [Option<l::FramePlan<'l, 'ty>>],
        layouts: &'l l::Layouts<'ty>,
        allocator: &'host mut a::Allocator<P>,
        control: e::Control<'ctrl>,
        limits: Limits,
        budget: &'host mut m::Budget,
        entry: m::FunctionId,
        arguments: &[EntryArgument],
    ) -> Result<Self, Reason> {
        Self::new_inner(
            proof, plans, layouts, allocator, control, limits, budget, entry, arguments, None, None,
        )
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep component program, root transport and original invocation services explicit."
    )]
    pub fn new_component(
        proof: &'ty o::tasks::VerifiedTasks,
        plans: &'p [Option<l::FramePlan<'l, 'ty>>],
        layouts: &'l l::Layouts<'ty>,
        allocator: &'host mut a::Allocator<P>,
        control: e::Control<'ctrl>,
        limits: Limits,
        budget: &'host mut m::Budget,
        entry: m::FunctionId,
        arguments: &[EntryArgument],
        component: &'l mut dyn crate::component::execution::Host,
        current: Option<t::CurrentTask<'host>>,
    ) -> Result<Self, Reason> {
        Self::new_inner(
            proof,
            plans,
            layouts,
            allocator,
            control,
            limits,
            budget,
            entry,
            arguments,
            Some(component),
            current,
        )
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "One constructor preserves ordinary and component frame/control semantics."
    )]
    fn new_inner(
        proof: &'ty o::tasks::VerifiedTasks,
        plans: &'p [Option<l::FramePlan<'l, 'ty>>],
        layouts: &'l l::Layouts<'ty>,
        allocator: &'host mut a::Allocator<P>,
        control: e::Control<'ctrl>,
        limits: Limits,
        budget: &'host mut m::Budget,
        entry: m::FunctionId,
        arguments: &[EntryArgument],
        mut component: Option<&'l mut dyn crate::component::execution::Host>,
        current: Option<t::CurrentTask<'host>>,
    ) -> Result<Self, Reason> {
        let accounting = component.as_deref().map(|host| host.accounting().clone());
        let declaration = &proof.typed().places().declarations().functions()[entry.0 as usize];
        if declaration.parameters.len() != arguments.len()
            || (component.is_none()
                && declaration
                    .parameters
                    .iter()
                    .zip(arguments)
                    .any(|(id, argument)| {
                        Some(declaration.locals[id.0 as usize].ty.ty) != argument.ty()
                    }))
        {
            return Err(Reason::Entry);
        }
        if let Some(host) = component.as_deref() {
            if !host.corresponds(proof, entry)
                || !host.control_matches(control)
                || !host.accounting().matches(control)
            {
                return Err(Reason::Entry);
            }
            host.validate(arguments, allocator)?;
        }
        let plan = plans[entry.0 as usize].as_ref().ok_or(Reason::Invariant)?;
        let component_cells = if component.is_some() {
            let types = proof.typed().places().declarations().types();
            declaration.parameters.iter().filter(|slot|matches!(declaration.locals[slot.0 as usize].ty.ty,
                o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::Loan(..)))).count()
                .checked_mul(2).ok_or(Reason::Invariant)?
        } else {
            0
        };
        let cells = plan
            .cleanup_slots()
            .checked_add(1)
            .and_then(|n| n.checked_add(component_cells))
            .ok_or(Reason::Invariant)?;
        admit(control, accounting.as_ref(), 0, 0, cells)?;
        let storage_tasks = if current.is_some() {
            1
        } else {
            limits.tasks.total_tasks
        };
        let mut records = budget.vector(storage_tasks)?;
        let mut faults = budget.vector(storage_tasks)?;
        faults.resize_with(storage_tasks, || None);
        let events = budget.vector(limits.trace_events)?;
        let mut frames = budget.vector(1)?;
        let callers = imports::prepare_failure_callers(1, budget)?;
        let failure_records =
            std::num::NonZeroUsize::new(limits.cleanup_failure_records).ok_or(Reason::Invariant)?;
        let completed = budget.vector(storage_tasks)?;
        let mut session = c::Session::prepare(failure_records, budget)?;
        let mut input_ready = budget.vector(declaration.parameters.len())?;
        let mut scheduler = match current {
            Some(current) => t::driver::Driver::scope(current)?,
            None => t::driver::Driver::owned(limits.tasks)?,
        };
        let span = proof.typed().places().declarations().functions()[entry.0 as usize].span;
        let id = scheduler.start(Some(span))?;
        let mut services = services::Services::prepare(
            proof,
            plans,
            layouts,
            allocator,
            scheduler,
            &id,
            limits.channels,
            control.limits().live_cells.unwrap_or(usize::MAX),
            control.service_work_limit(),
            component.as_deref().map(|host| host.service_work().clone()),
            budget,
        )?;
        if services.drain.is_none()
            && component
                .as_deref()
                .is_some_and(|host| host.indirect_input())
        {
            let drain = crate::retirement::Drain::prepare_with_budget(layouts, allocator, budget)
                .map_err(|error| match error {
                crate::retirement::Error::Layout(error) => Reason::Layout(error),
                _ => Reason::Invariant,
            })?;
            services.drain = Some(drain);
        }
        let mut frame = Frame::new(
            proof,
            plan,
            services.layouts,
            &services.work,
            entry,
            frame::ActivationId(0),
            failure_records,
            allocator,
            budget,
        )?;
        let staged = (|| -> Result<(), Reason> {
            if let Some(host) = component.as_deref_mut() {
                frame.prepare_component_inputs(&declaration.parameters, budget)?;
                host.prepare_arguments(arguments, allocator)?;
            }
            for (index, (parameter, argument)) in
                declaration.parameters.iter().zip(arguments).enumerate()
            {
                if let Some(value) = argument.immediate() {
                    frame.write(parameter.0 as usize, value, allocator)?;
                } else if let EntryArgument::Frame(argument) = argument {
                    frame.stage_component(
                        parameter.0 as usize,
                        *argument,
                        component.as_deref().ok_or(Reason::Entry)?,
                        allocator,
                        budget,
                        &mut input_ready,
                        &mut services.drain,
                        &mut session,
                    )?;
                } else {
                    let claim = component.as_deref().ok_or(Reason::Entry)?.argument(index)?;
                    frame
                        .data()?
                        .cell(parameter.0 as usize)?
                        .data_mut(allocator)?
                        .copy_from_slice(&claim.token());
                }
            }
            control.check()?;
            if let Some(host) = component.as_deref_mut() {
                host.commit_arguments(allocator)?;
            }
            for (parameter, argument) in declaration.parameters.iter().zip(arguments) {
                if matches!(
                    argument,
                    EntryArgument::Resource(_) | EntryArgument::Frame(_)
                ) {
                    frame.mark(parameter.0 as usize, frame.origin)?;
                }
            }
            Ok(())
        })();
        if let Err(error) = staged {
            services.component = component;
            let _ = frame.end_loans(allocator, &mut services);
            for slot in input_ready.into_iter().rev() {
                let cell = frame
                    .storage
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .cell_mut(slot)?;
                let types = proof.typed().places().declarations().types();
                let mut cleanup = c::Contextual::new(
                    |cell: &mut &mut l::Reservation<'_, '_>, _, context: &mut c::Context<'_>| {
                        cell.retire(allocator, context, |token, allocator, _, context| {
                            if token.bytes == [0; 32] {
                                return Ok(());
                            }
                            services.retire(types, token, allocator, context)
                        })
                        .map_err(|_| 4u32)
                    },
                );
                let _ = session.retire(
                    c::Slot(slot),
                    cell,
                    Some(frame.origin),
                    c::Reason::Failure,
                    &mut cleanup,
                );
            }
            if let Some(host) = services.component.as_deref_mut() {
                host.abort_arguments();
            }
            frame
                .storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .release(allocator)?;
            services.close(allocator)?;
            return Err(error);
        }
        services.component = component;
        frames.push(frame);
        records.push(Record {
            id,
            function: entry,
            parent: None,
            scope: 0,
            frames,
            callers,
            trace_depth: 1,
            result: None,
            next_result: None,
            session: Some(session),
        });
        let mut initial_usage = e::Usage {
            peak_frames: 1,
            peak_live_cells: cells,
            ..Default::default()
        };
        if let Some(accounting) = &accounting {
            accounting.observe(initial_usage.peak_frames, initial_usage.peak_live_cells);
            initial_usage = accounting.snapshot();
        }
        Ok(Self {
            proof,
            plans,
            allocator,
            control,
            budget,
            records,
            faults,
            events,
            trace: limits.trace_events != 0,
            trace_limit: limits.trace_events,
            usage: initial_usage,
            local_peak_frames: 1,
            local_peak_cells: cells,
            accounting,
            live_frames: 1,
            live_cells: cells,
            active: None,
            failure_records,
            completed,
            next_activation: 1,
            services,
        })
    }
    /// Both guest dispatchers charge the actual original component counter.
    pub fn charge(&mut self) -> Result<(), e::Failure> {
        if let Some(accounting) = &self.accounting {
            accounting.charge(self.control)?;
            self.sync_usage();
            Ok(())
        } else {
            self.control.charge(&mut self.usage)
        }
    }
    fn charge_host_call(&mut self) -> Result<(), e::Failure> {
        if let Some(accounting) = &self.accounting {
            accounting.charge_host_call(self.control)?;
            self.sync_usage();
            Ok(())
        } else {
            self.control.charge_host_call(&mut self.usage)
        }
    }
    fn sync_usage(&mut self) {
        if let Some(accounting) = &self.accounting {
            accounting.observe(self.local_peak_frames, self.local_peak_cells);
            self.usage = accounting.snapshot();
        } else {
            self.usage.peak_frames = self.local_peak_frames;
            self.usage.peak_live_cells = self.local_peak_cells;
            self.usage.service_work = self.services.work.used();
        }
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
            task: self.records[task].id.index(),
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
    pub fn end_loans(&mut self, task: usize) -> Result<(), Reason> {
        let frame = self
            .records
            .get_mut(task)
            .and_then(|r| r.frames.last_mut())
            .ok_or(Reason::Invariant)?;
        frame.end_loans(self.allocator, &mut self.services)
    }
    pub fn fault(&mut self, task: usize, reason: Reason, span: m::Span) -> Result<usize, Reason> {
        if let Some(index) = self.frame(task)?.fault {
            return Ok(index);
        }
        let function = self.frame(task)?.function;
        let callers = self.capture_callers(task, span);
        if !matches!(reason.primary_cause(), Reason::Cancelled) {
            self.record_execution(task, &reason, span)?;
        }
        self.faults[task] = Some(Fault {
            reason,
            function,
            span,
            callers,
        });
        self.frame_mut(task)?.fault = Some(task);
        Ok(task)
    }
    pub fn finish_faulted_task(&mut self, task: usize) -> Result<(), Reason> {
        let failure = self.frame(task)?.fault.ok_or(Reason::Invariant)?;
        let outcome = if self
            .faults
            .get(failure)
            .and_then(Option::as_ref)
            .is_some_and(|fault| matches!(fault.reason.primary_cause(), Reason::Cancelled))
        {
            // The original cancellation flag determines the terminal outcome.
            t::Outcome::Success
        } else {
            t::Outcome::Failed(t::FailureId(failure as u64))
        };
        self.services
            .scheduler
            .finish(&self.records[task].id, outcome)?;
        self.active = None;
        Ok(())
    }
    pub fn retired(
        &mut self,
        function: m::FunctionId,
        retained_result: bool,
        component_cells: usize,
    ) {
        self.live_frames -= 1;
        self.live_cells -= self.plans[function.0 as usize]
            .as_ref()
            .unwrap()
            .cleanup_slots()
            + usize::from(!retained_result)
            + component_cells;
    }
    pub fn prepare_frame(&mut self, function: m::FunctionId) -> Result<Frame<'l, 'ty>, Reason> {
        let plan = self.plans[function.0 as usize]
            .as_ref()
            .ok_or(Reason::Invariant)?;
        let cells = plan.cleanup_slots() + 1;
        admit(
            self.control,
            self.accounting.as_ref(),
            self.live_frames,
            self.value_cells()?,
            cells,
        )?;
        let activation = frame::ActivationId(self.next_activation);
        self.next_activation = self
            .next_activation
            .checked_add(1)
            .ok_or(Reason::Invariant)?;
        let frame = Frame::new(
            self.proof,
            plan,
            self.services.layouts,
            &self.services.work,
            function,
            activation,
            self.failure_records,
            self.allocator,
            self.budget,
        )?;
        self.live_frames += 1;
        self.live_cells += cells;
        self.local_peak_frames = self.local_peak_frames.max(self.live_frames);
        self.channel_usage()?;
        Ok(frame)
    }
    pub fn discard_result(
        &mut self,
        task: usize,
        owner: usize,
        reason: c::Reason,
    ) -> Result<bool, Reason> {
        let mut failed = false;
        if let Some(mut result) = self.records[task].result.take() {
            let meter = self.services.work.clone();
            let types = self.proof.typed().places().declarations().types();
            let services = &mut self.services;
            let allocator = &mut self.allocator;
            let session = self.records[owner]
                .session
                .as_mut()
                .ok_or(Reason::Invariant)?;
            failed = meter.unmetered(|| {
                let mut cleaner = c::Contextual::new(
                    |value: &mut &mut l::Reservation<'_, '_>, _, context: &mut c::Context<'_>| {
                        value
                            .retire(allocator, context, |token, allocator, _, context| {
                                services.retire(types, token, allocator, context)
                            })
                            .map_err(|_| 4u32)
                    },
                );
                session
                    .retire(
                        c::Slot(task),
                        &mut result.value,
                        Some(result.span),
                        reason,
                        &mut cleaner,
                    )
                    .is_err()
            });
            result.value.release_storage(self.allocator)?;
            self.live_cells -= 1;
            if failed {
                self.cleanup_fault(owner, self.records[task].function, result.span, None);
            }
        }
        Ok(failed)
    }
    fn publish_result(&mut self, task: usize) -> Result<(), Reason> {
        let Some(parent) = self.records[task].parent else {
            return Ok(());
        };
        if self.records[task].result.is_none() {
            return Ok(());
        }
        let serial = self.records[task].scope;
        let scope = self.records[parent]
            .frames
            .iter_mut()
            .flat_map(|f| &mut f.scopes)
            .find(|s| s.serial == serial)
            .ok_or(Reason::Invariant)?;
        let previous = scope.results.replace(task);
        self.records[task].next_result = previous;
        Ok(())
    }
    fn retire_scope_results(
        &mut self,
        task: usize,
        serial: u64,
        reason: c::Reason,
    ) -> Result<bool, Reason> {
        let mut failed = false;
        let (frame, scope) = self.records[task]
            .frames
            .iter()
            .enumerate()
            .find_map(|(f, frame)| {
                frame
                    .scopes
                    .iter()
                    .position(|s| s.serial == serial)
                    .map(|s| (f, s))
            })
            .ok_or(Reason::Invariant)?;
        while let Some(child) = self.records[task].frames[frame].scopes[scope].results {
            if self.records[child].parent != Some(task) || self.records[child].scope != serial {
                return Err(Reason::Invariant);
            }
            self.records[task].frames[frame].scopes[scope].results =
                self.records[child].next_result.take();
            failed |= self.discard_result(child, task, reason)?;
        }
        Ok(failed)
    }
    pub fn retire_children_results(
        &mut self,
        task: usize,
        reason: c::Reason,
    ) -> Result<bool, Reason> {
        let mut failed = false;
        for frame in (0..self.records[task].frames.len()).rev() {
            for index in (0..self.records[task].frames[frame].scopes.len()).rev() {
                let serial = self.records[task].frames[frame].scopes[index].serial;
                failed |= self.retire_scope_results(task, serial, reason)?;
            }
        }
        Ok(failed)
    }
    pub fn pending(&mut self, task: usize) -> Result<bool, Reason> {
        let Some(pending) = self.frame_mut(task)?.pending.take() else {
            return Ok(true);
        };
        match pending {
            Pending::Channel => self.pending_channel(task),
            Pending::Await {
                child,
                destination,
                next,
                failure,
            } => {
                let outcome = match self.services.scheduler.status(&self.records[child].id)? {
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
                        let span = self.proof.plan().functions()[frame.function.0 as usize].nodes()
                            [frame.node.0 as usize]
                            .span;
                        let cell = frame.data()?.cell(destination)?;
                        let root = cell.root();
                        let case = if outcome == t::Outcome::Success { 0 } else { 1 };
                        cell.prepare_case(root, case, self.allocator)?;
                        let at = cell.child(root, 0, self.allocator)?;
                        if case == 0 {
                            let mut result = child.result.take().ok_or(Reason::Invariant)?;
                            result.value.move_root_into(cell, at, self.allocator)?;
                            cell.initialize_order(
                                root,
                                [(0, result.span)].into_iter(),
                                self.allocator,
                            )?;
                            result.value.release_storage(self.allocator)?;
                            self.live_cells -= 1;
                        } else {
                            cell.write_immediate(at, l::Immediate::TaskCancelled, self.allocator)?;
                            cell.initialize_order(root, [(0, span)].into_iter(), self.allocator)?;
                        }
                        frame.mark(destination, span)?;
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
                    match self.services.scheduler.status(&self.records[index].id)? {
                        t::Status::Complete(t::Outcome::Failed(id)) => {
                            let frame = self.frame_mut(task)?;
                            frame.fault = Some(id.0 as usize);
                            frame.node = failure;
                            return Ok(true);
                        }
                        t::Status::Complete(_) => {}
                        _ => {
                            self.frame_mut(task)?.pending = Some(Pending::Scope {
                                scope,
                                cursor: index,
                                next,
                                failure,
                            });
                            self.services
                                .scheduler
                                .join(&self.records[task].id, &self.records[index].id)?;
                            self.active = None;
                            return Ok(false);
                        }
                    }
                }
                let reason = if self.frame(task)?.closing {
                    c::Reason::Failure
                } else {
                    c::Reason::ScopeExit
                };
                if self.retire_scope_results(task, scope.serial, reason)? {
                    let frame = self.frame_mut(task)?;
                    frame.fault.get_or_insert(task);
                    frame.node = failure;
                    return Ok(true);
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
        reserve_push(self.budget, &mut self.records[task].frames)?;
        let mut child = self.prepare_frame(function)?;
        let transferred = (|| {
            let parent = self.records[task]
                .frames
                .last_mut()
                .ok_or(Reason::Invariant)?;
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
            self.release_prepared_frame(task, child)?;
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
        self.services
            .scheduler
            .check_spawn(&self.records[task].id)?;
        let mut frames = self.budget.vector(1)?;
        let trace_depth = self.records[task]
            .trace_depth
            .checked_add(1)
            .ok_or(Reason::Invariant)?;
        let callers = imports::prepare_failure_callers(trace_depth, self.budget)?;
        let session = c::Session::prepare(self.failure_records, self.budget)?;
        let scope = self
            .frame(task)?
            .scopes
            .last()
            .copied()
            .ok_or(Reason::Invariant)?;
        let mut frame = self.prepare_frame(function)?;
        let transferred = (|| {
            let parent = self.records[task]
                .frames
                .last_mut()
                .ok_or(Reason::Invariant)?;
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
            self.release_prepared_frame(task, frame)?;
            return Err(error);
        }
        let child = self
            .services
            .scheduler
            .spawn(&self.records[task].id, Some(span))?;
        let index = child.index();
        frames.push(frame);
        self.records.push(Record {
            id: child.clone(),
            function,
            parent: Some(task),
            scope: scope.serial,
            frames,
            callers,
            trace_depth,
            result: None,
            next_result: None,
            session: Some(session),
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
        self.capture_retirement_boundary(task);
        self.event(task, EventKind::Cleanup(outcome), span)?;
        let channel_cleanup_failed = self.cancel_channel_waits(task).is_err();
        if outcome != t::Outcome::Success
            && let Some(entry) = self.proof.plan().functions()[function.0 as usize].cancellation()
        {
            if !self.frame(task)?.closing {
                self.frame_mut(task)?.node = entry;
            }
            backend.cleanup(self, task)?;
        }

        let mut cleanup_failure = (self.retire_children_results(
            task,
            if outcome == t::Outcome::Success {
                c::Reason::ScopeExit
            } else {
                c::Reason::Failure
            },
        )? || channel_cleanup_failed)
            .then_some(t::FailureId(task as u64));
        while let Some(mut frame) = self.records[task].frames.pop() {
            if self.records[task].frames.is_empty() && outcome == t::Outcome::Success && frame.saved
            {
                let result = frame.result(self.allocator, &mut self.services);
                self.merge_frame(task, &mut frame)?;
                if matches!(&result, Err(Reason::Cleanup)) {
                    self.cleanup_fault(task, frame.function, frame.origin, Some(&frame));
                }
                self.retired(frame.function, result.is_ok(), frame.component_cells());
                match result {
                    Ok(result) => self.records[task].result = Some(result),
                    Err(Reason::Cleanup) => {
                        cleanup_failure = Some(t::FailureId(task as u64));
                    }
                    Err(reason) => return Err(reason),
                }
            } else {
                if self.release_frame(task, frame)? {
                    cleanup_failure = Some(t::FailureId(task as u64));
                }
            }
        }
        let actual = self
            .services
            .scheduler
            .finish_cleanup(&self.records[task].id, cleanup_failure)?;
        self.completed.push(task);
        self.active = None;
        if actual != t::Outcome::Success {
            self.discard_result(task, task, c::Reason::UnpublishedReturn)?;
        } else {
            self.publish_result(task)?;
        }
        self.event(task, EventKind::Complete(actual), span)
    }
    fn abort(&mut self, reason: Reason, task: usize) -> Result<(), Reason> {
        let function = self
            .frame(task)
            .map_or(self.records[task].function, |frame| frame.function);
        let span = self
            .frame(task)
            .ok()
            .and_then(|frame| {
                self.proof
                    .plan()
                    .functions()
                    .get(frame.function.0 as usize)
                    .and_then(|function| function.nodes().get(frame.node.0 as usize))
                    .map(|node| node.span)
            })
            .unwrap_or_else(|| {
                self.proof.typed().places().declarations().functions()[function.0 as usize].span
            });
        if self.faults[0].is_none() {
            let callers = self
                .capture_cleanup_callers(task, None)
                .or_else(|| self.take_retirement_callers(task));
            let recorded = self.record_execution(0, &reason, span);
            self.faults[0] = Some(Fault {
                reason,
                function,
                span,
                callers,
            });
            recorded?;
        }
        self.trace = false;
        if let Some((task, cleaning)) = self.active.take() {
            if cleaning {
                let index = self.services.scheduler.record_index(&task)?;
                self.cancel_channel_waits(index)?;
                self.retire_children_results(index, c::Reason::Failure)?;
                while let Some(frame) = self.records[index].frames.pop() {
                    self.release_frame(index, frame)?;
                }
                self.discard_result(index, index, c::Reason::Failure)?;
                self.services
                    .scheduler
                    .finish_cleanup(&task, Some(t::FailureId(0)))?;
                self.completed.push(index);
            } else {
                self.services
                    .scheduler
                    .finish(&task, t::Outcome::Failed(t::FailureId(0)))?;
            }
        }
        self.services
            .scheduler
            .request_cancel(&self.records[0].id)?;
        while let Some(dispatch) = self.services.scheduler.dispatch()? {
            match dispatch {
                t::Dispatch::Resume(task) => self
                    .services
                    .scheduler
                    .finish(&task, t::Outcome::Failed(t::FailureId(0)))?,
                t::Dispatch::Cleanup { task, .. } => {
                    let index = self.services.scheduler.record_index(&task)?;
                    self.cancel_channel_waits(index)?;
                    self.retire_children_results(index, c::Reason::Failure)?;
                    let mut cleanup_failure = None;
                    while let Some(frame) = self.records[index].frames.pop() {
                        if self.release_frame(index, frame)? {
                            cleanup_failure = Some(t::FailureId(index as u64));
                        }
                    }
                    self.discard_result(index, index, c::Reason::Failure)?;
                    self.services
                        .scheduler
                        .finish_cleanup(&task, cleanup_failure)?;
                    self.completed.push(index);
                }
            }
        }
        for task in 0..self.records.len() {
            let owner = self.records[task].parent.unwrap_or(task);
            self.discard_result(task, owner, c::Reason::Failure)?;
        }
        Ok(())
    }
    fn abandon_frame_result(&mut self, span: m::Span) -> Result<(), Reason> {
        let host = self
            .services
            .component
            .as_deref_mut()
            .ok_or(Reason::Invariant)?;
        let allocator = &mut self.allocator;
        let session = self.records[0].session.as_mut().ok_or(Reason::Invariant)?;
        let mut cleanup = c::Contextual::new(|_: &mut (), _, context: &mut c::Context<'_>| {
            host.abandon_result(*allocator, context)
        });
        let _ = session.retire(
            c::Slot(0),
            (),
            Some(span),
            c::Reason::UnpublishedReturn,
            &mut cleanup,
        );
        Ok(())
    }

    fn finish_frame_root(&mut self) -> Result<crate::component::execution::Value, Reason> {
        use crate::component::execution::{self, Value};
        let types = self.proof.typed().places().declarations().types();
        let result = self.records[0].result.as_ref().ok_or(Reason::Invariant)?;
        let host = self
            .services
            .component
            .as_deref_mut()
            .ok_or(Reason::Invariant)?;
        let frame = {
            let session = host.result_session()?;
            let mut preparation = session
                .scope_preparation(self.budget)
                .map_err(Reason::from)?;
            execution::read_result(&result.value, types, self.allocator, &mut preparation)?
        };
        let value = host.prepare_result(Value::Frame(frame), self.allocator, self.budget)?;
        let mut result = self.records[0].result.take().ok_or(Reason::Invariant)?;
        // Resource leaves stay in the original table's escrow. Builtin data
        // owners are retired after their complete caller representation exists.
        let retired = {
            let services = &mut self.services;
            let allocator = &mut self.allocator;
            let session = self.records[0].session.as_mut().ok_or(Reason::Invariant)?;
            let mut cleanup = c::Contextual::new(
                |value: &mut &mut l::Reservation<'_, '_>, _, context: &mut c::Context<'_>| {
                    value
                        .retire(allocator, context, |token, allocator, _, context| {
                            services.retire(types, token, allocator, context)
                        })
                        .map_err(|_| 4u32)
                },
            );
            session.retire(
                c::Slot(0),
                &mut result.value,
                Some(result.span),
                c::Reason::ScopeExit,
                &mut cleanup,
            )
        };
        let released = result.value.release_storage(self.allocator);
        if released.is_ok() {
            self.live_cells -= 1;
        }
        if retired.is_err() || released.is_err() {
            self.abandon_frame_result(result.span)?;
            return Err(released.err().map(Reason::from).unwrap_or(Reason::Cleanup));
        }
        if let Err(reason) = self.control.check() {
            self.abandon_frame_result(result.span)?;
            return Err(reason.into());
        }
        if let Err(reason) = self
            .services
            .component
            .as_deref_mut()
            .ok_or(Reason::Invariant)?
            .commit_result(self.allocator)
        {
            self.abandon_frame_result(result.span)?;
            return Err(reason);
        }
        Ok(value)
    }

    fn finish_root(&mut self) -> Result<crate::component::execution::Value, Reason> {
        use crate::component::{execution::Value, storage::Claim};
        if self.services.component.is_some() {
            self.control.check()?;
        }
        let result = self.records[0].result.as_ref().ok_or(Reason::Invariant)?;
        let ty = result.value.root().ty();
        let types = self.proof.typed().places().declarations().types();
        let resource = matches!(ty, o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor, o::Constructor::ComponentResource(_)));
        if self.services.component.is_some()
            && !resource
            && ty != o::Type::I64
            && ty != o::Type::Bool
        {
            return self.finish_frame_root();
        }
        let mut token = if resource {
            Some(
                result
                    .value
                    .read_token(result.value.root(), self.allocator)?,
            )
        } else {
            None
        };
        let value = if let Some(token) = &token {
            Value::Resource(Claim::from_token(token.bytes).map_err(Reason::Component)?)
        } else {
            Value::Copy(
                result
                    .value
                    .read_immediate(result.value.root(), self.allocator)?,
            )
        };
        let value = if let Some(host) = self.services.component.as_deref_mut() {
            host.prepare_result(value, self.allocator, self.budget)?
        } else {
            value
        };
        let mut result = self.records[0].result.take().ok_or(Reason::Invariant)?;
        // The live component table retains the original guest obligation while
        // its caller representation is staged. Releasing this private return
        // cell therefore does not release the resource or publish its ticket.
        if let Err(error) = result.value.release_storage(self.allocator) {
            self.records[0].result = Some(result);
            if let Some(host) = self.services.component.as_deref_mut() {
                host.abort_arguments();
            }
            return Err(error.into());
        }
        self.live_cells -= 1;
        if let Some(host) = self.services.component.as_deref_mut()
            && let Err(reason) = host.commit_result(self.allocator)
        {
            host.abort_arguments();
            // A failed preflight has made no ownership write. The physical
            // resource still belongs to the guest even though its cell is gone.
            if let Some(token) = token.as_mut() {
                let services = &mut self.services;
                let allocator = &mut self.allocator;
                let session = self.records[0].session.as_mut().ok_or(Reason::Invariant)?;
                let mut cleaner = c::Contextual::new(
                    |token: &mut &mut l::Token, _, context: &mut c::Context<'_>| {
                        services.retire(types, token, allocator, context)
                    },
                );
                let _ = session.retire(
                    c::Slot(0),
                    token,
                    Some(result.span),
                    c::Reason::UnpublishedReturn,
                    &mut cleaner,
                );
            }
            return Err(reason);
        }
        Ok(value)
    }
    pub fn run<B: Backend<P>>(
        mut self,
        backend: &mut B,
    ) -> Result<Outcome<crate::component::execution::Value>, Failure> {
        let mut failed_task = None;
        let driven = (|| -> Result<(), Reason> {
            while let Some(dispatch) = self.services.scheduler.dispatch()? {
                match dispatch {
                    t::Dispatch::Resume(task) => {
                        let index = self.services.scheduler.record_index(&task)?;
                        failed_task = Some(index);
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
                        let index = self.services.scheduler.record_index(&task)?;
                        failed_task = Some(index);
                        self.active = Some((task, true));
                        self.cleanup(backend, index, outcome)?;
                    }
                }
                failed_task = None;
            }
            if !self.services.scheduler.is_finished() {
                if let Some(channels) = &self.services.channels
                    && let Some(deadlock) = channels
                        .deadlock(&self.services.scheduler)
                        .map_err(Reason::Channel)?
                {
                    let task = self
                        .services
                        .scheduler
                        .record_index(deadlock.operation.task())?;
                    let function = self.frame(task)?.function;
                    let span = deadlock
                        .operation
                        .origin()
                        .unwrap_or(self.frame(task)?.origin);
                    let reason = Reason::ChannelDeadlock(deadlock.clone());
                    let callers = self.capture_callers(task, span);
                    self.record_execution(0, &reason, span)?;
                    self.faults[0] = Some(Fault {
                        reason,
                        function,
                        span,
                        callers,
                    });
                    return Err(Reason::ChannelDeadlock(deadlock));
                }
                return Err(Reason::Invariant);
            }
            Ok(())
        })();
        let failed = driven.is_err();
        if let Err(reason) = driven
            && let Err(reason) = self.abort(reason, failed_task.unwrap_or(0))
        {
            let function = self.records[0].function;
            let span =
                self.proof.typed().places().declarations().functions()[function.0 as usize].span;
            if let Some(session) = self.records[0].session.as_mut() {
                session.append_invariant(Some(span));
            }
            if self.faults[0].is_none() {
                let callers = self
                    .capture_cleanup_callers(0, None)
                    .or_else(|| self.take_retirement_callers(0));
                self.faults[0] = Some(Fault {
                    reason,
                    function,
                    span,
                    callers,
                });
            }
        }
        self.sync_usage();
        let outcome = self.services.scheduler.status(&self.records[0].id);
        let mut finished = (|| -> Result<(), Reason> {
            if failed {
                return Err(Reason::Invariant);
            }
            match outcome.map_err(Reason::from)? {
                t::Status::Complete(t::Outcome::Success) => {
                    if self.poisoned_task().is_some() {
                        Err(Reason::Cleanup)
                    } else {
                        Ok(())
                    }
                }
                t::Status::Complete(t::Outcome::Cancelled) => Err(Reason::Cancelled),
                t::Status::Complete(t::Outcome::Failed(_)) => Err(Reason::Invariant),
                _ => Err(Reason::Invariant),
            }
        })();
        for task in 0..self.records.len() {
            if task == 0 && finished.is_ok() {
                continue;
            }
            let owner = self.records[task].parent.unwrap_or(task);
            let discarded = self.discard_result(task, owner, c::Reason::ScopeExit);
            if matches!(discarded, Ok(true)) && finished.is_ok() {
                finished = Err(Reason::Cleanup);
            }
            if let Err(reason) = discarded {
                let primary = self.failure_task();
                if let Some(session) = self.records[primary].session.as_mut() {
                    session.append_invariant(None);
                }
                let fault = self.faults.get_mut(primary).and_then(Option::take);
                let (reason, function, span, callers) = if let Some(fault) = fault {
                    (
                        fault.reason,
                        Some(fault.function),
                        Some(fault.span),
                        fault
                            .callers
                            .or_else(|| self.take_retirement_callers(primary)),
                    )
                } else {
                    (
                        reason,
                        Some(self.records[task].function),
                        None,
                        self.take_retirement_callers(task),
                    )
                };
                let (report, poisoned) = self.failure_report(primary).unwrap_or((None, true));
                return Err(Failure {
                    reason,
                    function,
                    span,
                    callers,
                    usage: self.usage,
                    tasks: self.services.scheduler.usage(),
                    events: self.events,
                    report,
                    poisoned,
                });
            }
        }
        if let Err(reason) = self.services.close(self.allocator)
            && finished.is_ok()
        {
            finished = Err(reason);
        }
        let mut final_failure_origin = None;
        let finished = finished.and_then(|()| {
            let result = self.finish_root();
            if let Err(reason) = &result {
                let function = self.records[0].function;
                let span = self.proof.typed().places().declarations().functions()
                    [function.0 as usize]
                    .span;
                let _ = self.record_execution(0, reason, span);
                final_failure_origin = Some((function, span));
            }
            result
        });
        if finished.is_err() {
            let function = self.records[0].function;
            let span =
                self.proof.typed().places().declarations().functions()[function.0 as usize].span;
            if let Some(host) = self.services.component.as_deref_mut() {
                host.abort_arguments();
            }
            // A failed final check or representation preflight never publishes
            // the escrowed result; mandatory cleanup still uses its guest ticket.
            if self
                .discard_result(0, 0, c::Reason::UnpublishedReturn)
                .is_err()
                && let Some(session) = self.records[0].session.as_mut()
            {
                session.execution(c::ExecutionFailure::Invariant, Some(span));
            }
        }
        match finished {
            Ok(value) => {
                let (report, poisoned) = self.failure_report(0).unwrap_or((None, true));
                Ok(Outcome {
                    value,
                    usage: self.usage,
                    tasks: self.services.scheduler.usage(),
                    allocations: self.allocator.stats(),
                    events: self.events,
                    report: report.filter(|r| r.primary().is_some()),
                    poisoned,
                })
            }
            Err(reason) => {
                let failure = self.failure_task();
                let mut fault = self.faults.get_mut(failure).and_then(Option::take);
                let (report, poisoned) = self.failure_report(failure).unwrap_or((None, true));
                // Final result admission precedes retirement of its escrow.
                // A later destructor failure must not replace that typed cause.
                let (reason, function, span, callers) =
                    if let Some((function, span)) = final_failure_origin.filter(|_| failure == 0) {
                        let callers = fault
                            .as_mut()
                            .and_then(|fault| fault.callers.take())
                            .or_else(|| self.take_retirement_callers(0));
                        (reason, Some(function), Some(span), callers)
                    } else if let Some(fault) = fault {
                        (
                            fault.reason,
                            Some(fault.function),
                            Some(fault.span),
                            fault
                                .callers
                                .or_else(|| self.take_retirement_callers(failure)),
                        )
                    } else {
                        (
                            reason,
                            Some(self.records[0].function),
                            Some(
                                self.proof.typed().places().declarations().functions()
                                    [self.records[0].function.0 as usize]
                                    .span,
                            ),
                            self.take_retirement_callers(failure),
                        )
                    };
                Err(Failure {
                    reason,
                    function,
                    span,
                    callers,
                    usage: self.usage,
                    tasks: self.services.scheduler.usage(),
                    events: self.events,
                    report,
                    poisoned,
                })
            }
        }
    }
}
fn admit(
    control: e::Control<'_>,
    accounting: Option<&e::InvocationAccounting>,
    frames: usize,
    cells: usize,
    add: usize,
) -> Result<(), Reason> {
    control.check()?;
    let local = e::LiveUsage { frames, cells };
    let live = accounting.map_or(Ok(local), |accounting| accounting.live_with_parent(local))?;
    let frames = live
        .frames
        .checked_add(1)
        .ok_or(e::Failure::CounterOverflow)?;
    if let Some(limit) = control.limits().frames
        && frames > limit
    {
        return Err(Reason::FrameLimit {
            limit,
            attempted: frames,
        });
    }
    let cells = live
        .cells
        .checked_add(add)
        .ok_or(e::Failure::CounterOverflow)?;
    if let Some(limit) = control.limits().live_cells
        && cells > limit
    {
        return Err(Reason::CellLimit {
            limit,
            attempted: cells,
        });
    }
    Ok(())
}

#[cfg(test)]
mod live_admission_tests {
    use super::*;

    #[test]
    fn parent_live_admission_is_exact_checked_and_scope_local() {
        let control = e::Control::with_limits(e::Limits {
            frames: Some(3),
            live_cells: Some(10),
            ..e::Limits::unbounded()
        });
        let mut budget = m::Budget::new(Default::default());
        let accounting = e::InvocationAccounting::prepare(control, &mut budget).unwrap();
        let empty = e::LiveUsage::default();
        for _ in 0..2 {
            accounting
                .with_parent(
                    e::LiveUsage {
                        frames: 1,
                        cells: 3,
                    },
                    || {
                        assert!(admit(control, Some(&accounting), 1, 4, 3).is_ok());
                        assert!(matches!(
                            admit(control, Some(&accounting), 1, 4, 4),
                            Err(Reason::CellLimit {
                                limit: 10,
                                attempted: 11
                            })
                        ));
                        assert!(matches!(
                            admit(control, Some(&accounting), 2, 4, 0),
                            Err(Reason::FrameLimit {
                                limit: 3,
                                attempted: 4
                            })
                        ));
                        accounting
                            .with_parent(
                                e::LiveUsage {
                                    frames: 1,
                                    cells: 2,
                                },
                                || {
                                    assert!(admit(control, Some(&accounting), 0, 0, 5).is_ok());
                                    assert!(matches!(
                                        admit(control, Some(&accounting), 0, 1, 5),
                                        Err(Reason::CellLimit {
                                            limit: 10,
                                            attempted: 11
                                        })
                                    ));
                                },
                            )
                            .unwrap();
                        assert_eq!(
                            accounting.live_with_parent(empty).unwrap(),
                            e::LiveUsage {
                                frames: 1,
                                cells: 3
                            }
                        );
                    },
                )
                .unwrap();
            assert_eq!(accounting.live_with_parent(empty).unwrap(), empty);
        }
        let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            accounting
                .with_parent(
                    e::LiveUsage {
                        frames: 2,
                        cells: 8,
                    },
                    || panic!("contained admission scope"),
                )
                .unwrap()
        }));
        assert!(unwind.is_err());
        assert_eq!(accounting.live_with_parent(empty).unwrap(), empty);
        let unbounded = e::Control::with_limits(e::Limits::unbounded());
        for live in [
            e::LiveUsage {
                frames: usize::MAX,
                cells: 0,
            },
            e::LiveUsage {
                frames: 0,
                cells: usize::MAX,
            },
        ] {
            accounting
                .with_parent(live, || {
                    assert!(matches!(
                        admit(unbounded, Some(&accounting), 0, 0, 1),
                        Err(Reason::Control(e::Failure::CounterOverflow))
                    ));
                })
                .unwrap();
        }
        assert_eq!(accounting.live_with_parent(empty).unwrap(), empty);
        assert_eq!(accounting.snapshot(), e::Usage::default());
        assert_eq!(budget.limits(), m::Limits::default());
    }
}
