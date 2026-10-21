use super::*;

impl<T: Payload> Hub<T> {
    pub fn receive(
        &mut self,
        scheduler: &mut t::Scheduler,
        task: &t::TaskId,
        receiver: &mut Receiver,
        span: Option<Span>,
    ) -> Result<Receive<T>, Error> {
        self.running(scheduler, task)?;
        let channel = self.role(&receiver.0, Role::Receiver)?;
        if self
            .pending
            .iter()
            .flatten()
            .any(|p| p.operation.task() == task)
            || self.channels[channel].receiver_wait.is_some()
        {
            return Err(Error::Busy);
        }
        if scheduler.cancellation_point(task)? == t::Boundary::Closing {
            return Ok(Receive::Closing);
        }
        self.work.charge(1)?;
        self.refresh(scheduler)?;
        if !self.channels[channel].values.is_empty() {
            self.work.charge(1)?;
            let entry = self.channels[channel].values.pop().unwrap();
            self.usage.queued -= 1;
            self.notify(scheduler, channel)?;
            return Ok(Receive::Value {
                value: entry.value.value,
                position: entry.position,
            });
        }
        if self.channels[channel].sealed {
            return Ok(Receive::Closed);
        }
        let slot = self.available(task)?;
        scheduler.check_park(task)?;
        let t::Park::Waiting(wait) = scheduler.park(task)? else {
            return Ok(Receive::Closing);
        };
        let operation = Operation {
            identity: Rc::clone(&self.identity),
            wait,
            origin: span,
        };
        self.pending[slot] = Some(Pending {
            operation: operation.clone(),
            channel,
            kind: Kind::Receive,
            previous: None,
            next: None,
            eligible: false,
            revoked: false,
        });
        self.channels[channel].receiver_wait = Some(slot);
        self.usage.pending += 1;
        Ok(Receive::Waiting(operation))
    }

    pub fn resume_receive(
        &mut self,
        scheduler: &mut t::Scheduler,
        operation: &Operation,
    ) -> Result<Receive<T>, Error> {
        let slot = self.operation(operation)?;
        self.running(scheduler, operation.task())?;
        if !matches!(self.pending[slot].as_ref().unwrap().kind, Kind::Receive) {
            return Err(Error::WrongRole);
        }
        if scheduler.cancellation_point(operation.task())? == t::Boundary::Closing {
            let p = self.remove(slot);
            self.notify(scheduler, p.channel)?;
            return Ok(Receive::Closing);
        }
        self.refresh(scheduler)?;
        let p = self.pending[slot].as_ref().unwrap();
        if p.revoked {
            return Err(Error::StaleOperation);
        }
        if !p.eligible {
            return Err(Error::NotReady);
        }
        let channel = p.channel;
        if !self.channels[channel].values.is_empty() {
            self.work.charge(1)?;
            self.remove(slot);
            let entry = self.channels[channel].values.pop().unwrap();
            self.usage.queued -= 1;
            self.notify(scheduler, channel)?;
            return Ok(Receive::Value {
                value: entry.value.value,
                position: entry.position,
            });
        }
        if self.channels[channel].sealed {
            self.remove(slot);
            self.notify(scheduler, channel)?;
            return Ok(Receive::Closed);
        }
        Err(Error::NotReady)
    }

    pub fn release_sender(
        &mut self,
        scheduler: &mut t::Scheduler,
        sender: &Sender,
    ) -> Result<(), Error> {
        self.scheduler(scheduler)?;
        let channel = self.sender_index(sender)?;
        self.tickets[sender.ticket] = None;
        self.channels[channel].producers -= 1;
        self.usage.handles -= 1;
        if self.channels[channel].producers == 0 {
            self.channels[channel].sealed = true;
        }
        self.refresh(scheduler)
    }

    pub fn seal(&mut self, scheduler: &mut t::Scheduler, receiver: &Receiver) -> Result<(), Error> {
        self.scheduler(scheduler)?;
        let channel = self.role(&receiver.0, Role::Receiver)?;
        self.work.charge(1)?;
        self.channels[channel].sealed = true;
        self.refresh(scheduler)
    }

    pub fn abort_receiver<C: c::Cleaner<T>>(
        &mut self,
        scheduler: &mut t::Scheduler,
        receiver: &Receiver,
        session: &mut c::Session,
        reason: c::Reason,
        cleaner: &mut C,
    ) -> Result<(), Error> {
        let channel = self.role(&receiver.0, Role::Receiver)?;
        self.abort(scheduler, channel, session, reason, cleaner)
    }

    pub fn abort_setup<C: c::Cleaner<T>>(
        &mut self,
        scheduler: &mut t::Scheduler,
        setup: &Setup,
        session: &mut c::Session,
        reason: c::Reason,
        cleaner: &mut C,
    ) -> Result<(), Error> {
        self.scheduler(scheduler)?;
        let channel = self.role(&setup.0, Role::Setup)?;
        self.channels[channel].producers -= 1;
        self.abort(scheduler, channel, session, reason, cleaner)
    }

    fn abort<C: c::Cleaner<T>>(
        &mut self,
        scheduler: &mut t::Scheduler,
        channel: usize,
        session: &mut c::Session,
        reason: c::Reason,
        cleaner: &mut C,
    ) -> Result<(), Error> {
        self.scheduler(scheduler)?;
        self.channels[channel].role = Role::Retired;
        self.channels[channel].sealed = true;
        self.refresh(scheduler)?;
        let work = self.work.clone();
        work.unmetered(|| {
            while let Some(entry) = self.channels[channel].values.pop() {
                self.usage.queued -= 1;
                let _ = session.retire(
                    c::Slot(channel),
                    entry.value.value,
                    entry.value.span,
                    reason,
                    cleaner,
                );
            }
        });
        let capacity = self.channels[channel].capacity;
        self.channels[channel].values.release();
        self.channels[channel].capacity = 0;
        self.usage.reserved_slots -= capacity;
        Ok(())
    }
}
