//! Task-local failure evidence and mandatory retirement shared by both backends.
use super::*;
enum FrameEntry {
    Prepared,
    Entered,
}
impl<'p, 'l, 'ty, 'host, 'ctrl, P: a::Provider> Machine<'p, 'l, 'ty, 'host, 'ctrl, P> {
    pub(super) fn record_execution(
        &mut self,
        task: usize,
        reason: &Reason,
        span: m::Span,
    ) -> Result<(), Reason> {
        let poisoned_cause = match reason.without_native_frames() {
            Reason::Native(
                crate::component::execution::NativeFailure::Child(failure)
                | crate::component::execution::NativeFailure::Context { failure, .. },
            ) => failure.poisoned,
            Reason::Native(
                crate::component::execution::NativeFailure::Latched(failure)
                | crate::component::execution::NativeFailure::Propagated { failure, .. },
            ) => failure.report().poisoned,
            _ => false,
        };
        let failure = if poisoned_cause {
            c::ExecutionFailure::Invariant
        } else {
            match reason.primary_cause() {
                Reason::Cleanup => return Ok(()),
                Reason::Panic(_) => c::ExecutionFailure::Panic,
                Reason::Execution(failure) => *failure,
                Reason::FrameLimit { .. } => c::ExecutionFailure::FrameLimit,
                Reason::CellLimit { .. } => c::ExecutionFailure::CellLimit,
                Reason::Control(failure) | Reason::Layout(l::Error::ServiceWork(failure)) => {
                    match failure {
                        e::Failure::Interrupted => c::ExecutionFailure::Interrupted,
                        e::Failure::DeadlineExceeded => c::ExecutionFailure::DeadlineExceeded,
                        e::Failure::FuelExhausted => c::ExecutionFailure::FuelLimit,
                        e::Failure::HostCallLimit => c::ExecutionFailure::HostCallLimit,
                        e::Failure::CounterOverflow => c::ExecutionFailure::CounterOverflow,
                        e::Failure::ServiceWorkLimit {
                            limit,
                            used,
                            requested,
                        } => c::ExecutionFailure::ServiceWorkLimit {
                            limit: *limit,
                            used: *used,
                            requested: *requested,
                        },
                    }
                }
                Reason::Resource(_)
                | Reason::Layout(
                    l::Error::Resource { .. }
                    | l::Error::Overflow
                    | l::Error::Allocation(
                        a::Error::Limit(_) | a::Error::OutOfMemory | a::Error::Overflow,
                    ),
                ) => c::ExecutionFailure::AllocationLimit,
                Reason::Layout(l::Error::CellLimit { .. }) => c::ExecutionFailure::CellLimit,
                Reason::Invariant
                | Reason::Layout(_)
                | Reason::Instance(crate::component::execution::InstanceError::ScopeNotClosed) => {
                    c::ExecutionFailure::Invariant
                }
                Reason::Native(crate::component::execution::NativeFailure::Child(failure))
                    if failure.poisoned =>
                {
                    c::ExecutionFailure::Invariant
                }
                Reason::Native(crate::component::execution::NativeFailure::Report(report))
                    if report.report().poisoned =>
                {
                    c::ExecutionFailure::Invariant
                }
                Reason::Native(crate::component::execution::NativeFailure::Latched(report))
                    if report.report().poisoned =>
                {
                    c::ExecutionFailure::Invariant
                }
                Reason::Native(
                    crate::component::execution::NativeFailure::Invocation(_)
                    | crate::component::execution::NativeFailure::Status { .. }
                    | crate::component::execution::NativeFailure::MissingResult
                    | crate::component::execution::NativeFailure::InvalidResult(_)
                    | crate::component::execution::NativeFailure::Resource { .. },
                ) => c::ExecutionFailure::Invariant,
                _ => c::ExecutionFailure::Fault,
            }
        };
        let session = self.records[task]
            .session
            .as_mut()
            .ok_or(Reason::Invariant)?;
        if !matches!(
            session.report().primary().map(|r| r.kind),
            Some(c::FailureKind::Execution(_))
        ) {
            session.execution(failure, Some(span));
        }
        Ok(())
    }
    pub fn merge_frame(&mut self, task: usize, frame: &mut Frame<'l, 'ty>) -> Result<(), Reason> {
        if let Some(managed) = frame.managed.take() {
            self.records[task]
                .session
                .as_mut()
                .ok_or(Reason::Invariant)?
                .append(managed.session);
        }
        Ok(())
    }
    pub fn cleanup_fault(
        &mut self,
        task: usize,
        function: m::FunctionId,
        span: m::Span,
        detached: Option<&Frame<'_, '_>>,
    ) {
        if self.faults[task].is_none() {
            let callers = self.capture_cleanup_callers(task, detached);
            self.faults[task] = Some(Fault {
                reason: Reason::Cleanup,
                function,
                span,
                callers,
            });
        }
    }
    /// Continue independent owners after a callback error. Structural storage
    /// faults remain fatal; the abort path still drains the remaining frames.
    pub fn release_frame(&mut self, task: usize, frame: Frame<'l, 'ty>) -> Result<bool, Reason> {
        self.retire_frame(task, frame, FrameEntry::Entered)
    }
    pub fn release_prepared_frame(
        &mut self,
        task: usize,
        frame: Frame<'l, 'ty>,
    ) -> Result<bool, Reason> {
        self.retire_frame(task, frame, FrameEntry::Prepared)
    }
    fn retire_frame(
        &mut self,
        task: usize,
        mut frame: Frame<'l, 'ty>,
        entry: FrameEntry,
    ) -> Result<bool, Reason> {
        let result = frame.release(self.allocator, &mut self.services);
        self.merge_frame(task, &mut frame)?;
        if matches!(&result, Err(Reason::Cleanup)) && matches!(entry, FrameEntry::Entered) {
            self.cleanup_fault(task, frame.function, frame.origin, Some(&frame));
        }
        // An unentered call/spawn already has its original refusal. Preserve
        // the caller's reserved trace storage for that fault's capture.
        self.retired(frame.function, false, frame.component_cells());
        match result {
            Ok(()) => Ok(false),
            Err(Reason::Cleanup) => Ok(true),
            Err(reason) => Err(reason),
        }
    }
    pub fn publish_sync(&mut self, task: usize) -> Result<bool, Reason> {
        if self.records[task].frames.len() == 1 {
            self.services
                .scheduler
                .finish(&self.records[task].id, t::Outcome::Success)?;
            self.active = None;
            return Ok(false);
        }
        let mut child = self.records[task].frames.pop().ok_or(Reason::Invariant)?;
        let returned = child.returned.ok_or(Reason::Invariant)?;
        let mut cleaned = child.cleanup_all(
            Some(child.return_cell),
            c::Reason::ScopeExit,
            self.allocator,
            &mut self.services,
        );
        if cleaned.is_ok() {
            let from = child.return_cell;
            cleaned = self.records[task]
                .frames
                .last_mut()
                .ok_or(Reason::Invariant)?
                .copy_from_frame(returned.destination, &mut child, from, self.allocator);
        }
        if matches!(&cleaned, Err(Reason::Cleanup)) {
            self.cleanup_fault(task, child.function, child.origin, Some(&child));
        }
        let failed = self.release_frame(task, child)?;
        let cleanup_failed = cleaned.is_err();
        if let Err(reason) = cleaned
            && !matches!(reason, Reason::Cleanup)
        {
            return Err(reason);
        }
        let parent = self.frame_mut(task)?;
        if failed || cleanup_failed {
            parent.fault = Some(task);
            parent.node = returned.failure;
        } else {
            parent.node = returned.next;
        }
        Ok(true)
    }
    pub fn propagate_sync(&mut self, task: usize) -> Result<bool, Reason> {
        let failure = self.frame(task)?.fault.ok_or(Reason::Invariant)?;
        if self.records[task].frames.len() == 1 {
            self.services.scheduler.finish(
                &self.records[task].id,
                t::Outcome::Failed(t::FailureId(failure as u64)),
            )?;
            self.active = None;
            return Ok(false);
        }
        let child = self.records[task].frames.pop().ok_or(Reason::Invariant)?;
        let returned = child.returned.ok_or(Reason::Invariant)?;
        self.release_frame(task, child)?;
        let parent = self.frame_mut(task)?;
        parent.fault = Some(failure);
        parent.node = returned.failure;
        Ok(true)
    }
    pub(super) fn poisoned_task(&self) -> Option<usize> {
        self.records
            .iter()
            .position(|record| record.session.as_ref().is_some_and(c::Session::is_poisoned))
    }
    pub(super) fn failure_task(&self) -> usize {
        match self.services.scheduler.status(&self.records[0].id) {
            Ok(t::Status::Complete(t::Outcome::Failed(id))) => id.0 as usize,
            _ if self.faults[0].is_some() => 0,
            _ => self.poisoned_task().unwrap_or(0),
        }
    }
    pub(super) fn failure_report(
        &mut self,
        primary: usize,
    ) -> Result<(Option<c::Report>, bool), Reason> {
        let mut session = self
            .records
            .get_mut(primary)
            .and_then(|r| r.session.take())
            .ok_or(Reason::Invariant)?;
        for &task in &self.completed {
            if let Some(other) = self.records[task].session.take() {
                session.append(other);
            }
        }
        for record in &mut self.records {
            if let Some(other) = record.session.take() {
                session.append(other);
            }
        }
        let poisoned = session.is_poisoned();
        Ok((Some(session.into_report()), poisoned))
    }
}
