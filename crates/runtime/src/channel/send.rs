use super::*;

impl<T: Payload> Hub<T> {
    pub fn send(
        &mut self,
        scheduler: &mut t::Scheduler,
        task: &t::TaskId,
        sender: &Sender,
        value: T,
        span: Option<Span>,
    ) -> Result<Send<T>, Failure<T>> {
        let mut inspect = |value: &T, visit: &mut dyn FnMut(&Receiver) -> Result<(), Error>| {
            value.receivers(visit)
        };
        self.send_inspected(scheduler, task, sender, value, span, &mut inspect)
    }

    pub fn send_inspected(
        &mut self,
        scheduler: &mut t::Scheduler,
        task: &t::TaskId,
        sender: &Sender,
        value: T,
        span: Option<Span>,
        inspect: &mut PayloadInspection<'_, T>,
    ) -> Result<Send<T>, Failure<T>> {
        self.send_with_publication(scheduler, task, sender, value, span, &mut |root, visit| {
            match root {
                PublicationRoot::Message(value) => inspect(value, visit),
                PublicationRoot::State(_) => Ok(()),
            }
        })
    }

    pub(crate) fn send_with_publication(
        &mut self,
        scheduler: &mut t::Scheduler,
        task: &t::TaskId,
        sender: &Sender,
        value: T,
        span: Option<Span>,
        inspect: &mut PublicationInspection<'_, T>,
    ) -> Result<Send<T>, Failure<T>> {
        let owner_span = value.owner_span().or(span);
        let checked = (|| {
            self.running(scheduler, task)?;
            let channel = self.sender_index(sender)?;
            if self
                .pending
                .iter()
                .flatten()
                .any(|p| p.operation.task() == task)
            {
                return Err(Error::Busy);
            }
            Ok(channel)
        })();
        let channel = match checked {
            Ok(channel) => channel,
            Err(error) => return Err(Failure { error, value }),
        };
        match scheduler.cancellation_point(task) {
            Ok(t::Boundary::Closing) => return Ok(Send::Closing(value)),
            Err(error) => {
                return Err(Failure {
                    error: error.into(),
                    value,
                });
            }
            Ok(t::Boundary::Continue) => {}
        }
        let admission = (|| {
            self.work.charge(1)?;
            self.refresh(scheduler)?;
            Ok::<(), Error>(())
        })();
        if let Err(error) = admission {
            return Err(Failure { error, value });
        }
        if self.channels[channel].sealed {
            return Ok(Send::Rejected {
                reason: Rejection::Closed,
                value,
            });
        }
        if self.channels[channel].values.has_space() && self.channels[channel].send_head.is_none() {
            match self.publication(channel, &value, inspect) {
                Ok(false) => {
                    return Ok(Send::Rejected {
                        reason: Rejection::InvalidPublication,
                        value,
                    });
                }
                Err(error) => return Err(Failure { error, value }),
                Ok(true) => {}
            }
            if let Err(error) = self.work.charge(1) {
                return Err(Failure {
                    error: error.into(),
                    value,
                });
            }
            let position = match self.channels[channel].values.push(Message {
                value,
                span: owner_span,
            }) {
                Ok(position) => position,
                Err((refusal, message)) => {
                    return Err(Failure {
                        error: match refusal {
                            queue::Refusal::Full => Error::NotReady,
                            queue::Refusal::PositionExhausted => Error::PositionExhausted,
                        },
                        value: message.value,
                    });
                }
            };
            self.usage.queued += 1;
            // Refresh validated every private task ticket. Notification is
            // allocation-free bookkeeping, outside the ownership transition.
            self.notify(scheduler, channel)
                .expect("validated channel readiness");
            return Ok(Send::Sent { position });
        }
        let slot = match self.available(task).and_then(|slot| {
            scheduler
                .check_park(task)
                .map(|()| slot)
                .map_err(Error::from)
        }) {
            Ok(slot) => slot,
            Err(error) => return Err(Failure { error, value }),
        };
        let wait = match scheduler.park(task) {
            Ok(t::Park::Waiting(wait)) => wait,
            Ok(t::Park::Closing) => return Ok(Send::Closing(value)),
            Err(error) => {
                return Err(Failure {
                    error: error.into(),
                    value,
                });
            }
        };
        let operation = Operation {
            identity: Rc::clone(&self.identity),
            wait,
            origin: span,
        };
        self.pending[slot] = Some(Pending {
            operation: operation.clone(),
            channel,
            kind: Kind::Send(Message {
                value,
                span: owner_span,
            }),
            previous: None,
            next: None,
            eligible: false,
            revoked: false,
        });
        self.usage.pending += 1;
        self.append_sender(channel, slot);
        Ok(Send::Waiting(operation))
    }

    /// On a precommit error the operation keeps its pending value for `cancel`;
    /// callers must not abandon that owner merely because resumption failed.
    pub fn resume_send(
        &mut self,
        scheduler: &mut t::Scheduler,
        operation: &Operation,
    ) -> Result<Send<T>, Error> {
        let mut inspect = |value: &T, visit: &mut dyn FnMut(&Receiver) -> Result<(), Error>| {
            value.receivers(visit)
        };
        self.resume_send_inspected(scheduler, operation, &mut inspect)
    }

    pub fn resume_send_inspected(
        &mut self,
        scheduler: &mut t::Scheduler,
        operation: &Operation,
        inspect: &mut PayloadInspection<'_, T>,
    ) -> Result<Send<T>, Error> {
        self.resume_send_with_publication(scheduler, operation, &mut |root, visit| match root {
            PublicationRoot::Message(value) => inspect(value, visit),
            PublicationRoot::State(_) => Ok(()),
        })
    }

    pub(crate) fn resume_send_with_publication(
        &mut self,
        scheduler: &mut t::Scheduler,
        operation: &Operation,
        inspect: &mut PublicationInspection<'_, T>,
    ) -> Result<Send<T>, Error> {
        let slot = self.operation(operation)?;
        self.running(scheduler, operation.task())?;
        if !matches!(self.pending[slot].as_ref().unwrap().kind, Kind::Send(_)) {
            return Err(Error::WrongRole);
        }
        if scheduler.cancellation_point(operation.task())? == t::Boundary::Closing {
            let p = self.remove(slot);
            self.notify(scheduler, p.channel)?;
            let Kind::Send(message) = p.kind else {
                unreachable!()
            };
            return Ok(Send::Closing(message.value));
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
        if self.channels[channel].sealed {
            let p = self.remove(slot);
            self.notify(scheduler, channel)?;
            let Kind::Send(message) = p.kind else {
                unreachable!()
            };
            return Ok(Send::Rejected {
                reason: Rejection::Closed,
                value: message.value,
            });
        }
        if self.channels[channel].send_head != Some(slot)
            || !self.channels[channel].values.has_space()
        {
            return Err(Error::NotReady);
        }
        // Inspect under shared ownership; refusal/unwinding leaves the pending
        // reservation in its ledger until cancellation/normal completion.
        if !self.pending_publication(slot, inspect)? {
            let p = self.remove(slot);
            self.notify(scheduler, channel)?;
            let Kind::Send(message) = p.kind else {
                unreachable!()
            };
            return Ok(Send::Rejected {
                reason: Rejection::InvalidPublication,
                value: message.value,
            });
        }
        self.work.charge(1)?;
        let p = self.remove(slot);
        let Kind::Send(message) = p.kind else {
            unreachable!()
        };
        let position = match self.channels[channel].values.push(message) {
            Ok(position) => position,
            Err((refusal, message)) => {
                // Restore the pending owner and head turn on a refused commit.
                self.pending[slot] = Some(Pending {
                    kind: Kind::Send(message),
                    previous: None,
                    next: self.channels[channel].send_head,
                    ..p
                });
                if let Some(next) = self.channels[channel].send_head {
                    self.pending[next].as_mut().unwrap().previous = Some(slot);
                } else {
                    self.channels[channel].send_tail = Some(slot);
                }
                self.channels[channel].send_head = Some(slot);
                self.usage.pending += 1;
                return Err(match refusal {
                    queue::Refusal::Full => Error::NotReady,
                    queue::Refusal::PositionExhausted => Error::PositionExhausted,
                });
            }
        };
        self.usage.queued += 1;
        self.notify(scheduler, channel)?;
        Ok(Send::Sent { position })
    }
}
