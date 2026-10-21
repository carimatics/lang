use super::*;

impl<T: Payload> Hub<T> {
    /// Cleanup can recover a registration from its checked task identity even
    /// when a failed adapter continuation has no local completion claim left.
    pub fn pending_operation(
        &self,
        scheduler: &t::Scheduler,
        task: &t::TaskId,
    ) -> Result<Option<Operation>, Error> {
        self.scheduler(scheduler)?;
        scheduler.status(task)?;
        Ok(self
            .pending
            .iter()
            .flatten()
            .find(|p| p.operation.task() == task)
            .map(|p| p.operation.clone()))
    }
    pub(super) fn available(&self, task: &t::TaskId) -> Result<usize, Error> {
        if self
            .pending
            .iter()
            .flatten()
            .any(|p| p.operation.task() == task)
        {
            return Err(Error::Busy);
        }
        self.pending
            .iter()
            .position(Option::is_none)
            .ok_or(Error::WaiterLimit {
                limit: self.limits.waiters,
            })
    }

    pub(super) fn operation(&self, operation: &Operation) -> Result<usize, Error> {
        if !Rc::ptr_eq(&self.identity, &operation.identity) {
            return Err(Error::ForeignEndpoint);
        }
        self.pending
            .iter()
            .position(|p| p.as_ref().is_some_and(|p| p.operation == *operation))
            .ok_or(Error::StaleOperation)
    }

    pub(super) fn append_sender(&mut self, channel: usize, slot: usize) {
        let previous = self.channels[channel].send_tail;
        self.pending[slot].as_mut().unwrap().previous = previous;
        if let Some(previous) = previous {
            self.pending[previous].as_mut().unwrap().next = Some(slot);
        } else {
            self.channels[channel].send_head = Some(slot);
        }
        self.channels[channel].send_tail = Some(slot);
    }

    pub(super) fn unlink(&mut self, slot: usize) {
        let p = self.pending[slot].as_ref().unwrap();
        let (channel, previous, next, send) = (
            p.channel,
            p.previous,
            p.next,
            matches!(p.kind, Kind::Send(_)),
        );
        if send {
            if let Some(previous) = previous {
                self.pending[previous].as_mut().unwrap().next = next;
            } else if self.channels[channel].send_head == Some(slot) {
                self.channels[channel].send_head = next;
            }
            if let Some(next) = next {
                self.pending[next].as_mut().unwrap().previous = previous;
            } else if self.channels[channel].send_tail == Some(slot) {
                self.channels[channel].send_tail = previous;
            }
        } else if self.channels[channel].receiver_wait == Some(slot) {
            self.channels[channel].receiver_wait = None;
        }
        let p = self.pending[slot].as_mut().unwrap();
        p.previous = None;
        p.next = None;
    }

    pub(super) fn remove(&mut self, slot: usize) -> Pending<T> {
        if !self.pending[slot].as_ref().unwrap().revoked {
            self.unlink(slot);
        }
        self.usage.pending -= 1;
        self.pending[slot].take().unwrap()
    }

    fn wake_slot(&mut self, scheduler: &mut t::Scheduler, slot: usize) -> Result<(), Error> {
        let p = self.pending[slot].as_ref().unwrap();
        if p.eligible || p.revoked {
            return Ok(());
        }
        match scheduler.wake(&p.operation.wait) {
            Ok(()) => self.pending[slot].as_mut().unwrap().eligible = true,
            Err(t::Error::StaleWait) => {
                self.unlink(slot);
                self.pending[slot].as_mut().unwrap().revoked = true;
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    pub(super) fn notify(
        &mut self,
        scheduler: &mut t::Scheduler,
        channel: usize,
    ) -> Result<(), Error> {
        if self.channels[channel].sealed {
            let mut next = self.channels[channel].send_head;
            while let Some(slot) = next {
                next = self.pending[slot].as_ref().unwrap().next;
                self.wake_slot(scheduler, slot)?;
            }
        } else if self.channels[channel].values.has_space() {
            while let Some(head) = self.channels[channel].send_head {
                self.wake_slot(scheduler, head)?;
                if self.channels[channel].send_head == Some(head) {
                    break;
                }
            }
        }
        if let Some(receiver) = self.channels[channel].receiver_wait
            && (!self.channels[channel].values.is_empty() || self.channels[channel].sealed)
        {
            self.wake_slot(scheduler, receiver)?;
        }
        Ok(())
    }

    /// Revoke cancelled registrations without dropping their pending payloads.
    /// The task adapter still owns cancellation retirement via `cancel`.
    pub fn refresh(&mut self, scheduler: &mut t::Scheduler) -> Result<(), Error> {
        self.scheduler(scheduler)?;
        for slot in 0..self.pending.len() {
            let Some(p) = self.pending[slot].as_ref() else {
                continue;
            };
            if p.revoked {
                continue;
            }
            let status = scheduler.status(p.operation.task())?;
            if scheduler.cancellation_requested(p.operation.task())?
                || matches!(
                    status,
                    t::Status::Joining
                        | t::Status::CleanupReady(_)
                        | t::Status::Cleaning(_)
                        | t::Status::Complete(_)
                )
            {
                self.unlink(slot);
                self.pending[slot].as_mut().unwrap().revoked = true;
            }
        }
        for channel in 0..self.channels.len() {
            self.notify(scheduler, channel)?;
        }
        Ok(())
    }

    /// Unlink before returning the pending owner to the adapter's cleanup.
    pub fn cancel(
        &mut self,
        scheduler: &mut t::Scheduler,
        operation: &Operation,
    ) -> Result<Cancelled<T>, Error> {
        self.scheduler(scheduler)?;
        let slot = self.operation(operation)?;
        if matches!(
            scheduler.status(operation.task())?,
            t::Status::Running | t::Status::Ready | t::Status::Parked | t::Status::Waiting { .. }
        ) {
            scheduler.request_cancel(operation.task())?;
        }
        let p = self.remove(slot);
        if matches!(p.kind, Kind::Receive) {
            self.channels[p.channel].sealed = true;
        }
        self.refresh(scheduler)?;
        Ok(match p.kind {
            Kind::Send(value) => Cancelled::Send(value.value),
            Kind::Receive => Cancelled::Receive,
        })
    }

    /// Closed drivers record this origin before requesting tree cancellation.
    /// No external event/timer capability is inferred from a parked task.
    pub fn deadlock(&self, scheduler: &t::Scheduler) -> Result<Option<Deadlock>, Error> {
        self.scheduler(scheduler)?;
        if !scheduler.is_quiescent() {
            return Ok(None);
        }
        let mut oldest: Option<&Operation> = None;
        for pending in self.pending.iter().flatten() {
            if !pending.revoked
                && scheduler.status(pending.operation.task())? == t::Status::Parked
                && oldest.is_none_or(|old| pending.operation.ordinal() < old.ordinal())
            {
                oldest = Some(&pending.operation);
            }
        }
        Ok(oldest.map(|operation| Deadlock {
            operation: operation.clone(),
        }))
    }
}
