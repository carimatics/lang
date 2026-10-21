//! RFC 0025 bounded channel services shared by trusted execution adapters.
//!
//! Source admission and concrete guest endpoint layouts remain independent
//! source/MIR duties. No caller-supplied Sendable bit authorizes a payload.

mod operations;
mod publication;
mod queue;
mod receive;
mod send;
mod types;
pub use types::*;
#[doc(hidden)]
pub mod managed;

#[cfg(test)]
mod tests;

use crate::{cleanup as c, execution as e, task as t};
use lang_mir::unit::Span;
use std::rc::Rc;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Role {
    Setup,
    Receiver,
    Retired,
}
struct Message<T> {
    value: T,
    span: Option<Span>,
}
struct Channel<T> {
    values: queue::Queue<Message<T>>,
    role: Role,
    producers: usize,
    sealed: bool,
    capacity: usize,
    send_head: Option<usize>,
    send_tail: Option<usize>,
    receiver_wait: Option<usize>,
}
enum Kind<T> {
    Send(Message<T>),
    Receive,
}
struct Pending<T> {
    operation: Operation,
    channel: usize,
    kind: Kind<T>,
    previous: Option<usize>,
    next: Option<usize>,
    eligible: bool,
    revoked: bool,
}

/// One prepared, typed trusted-adapter service. Source adapters must supply
/// independently verified payload wrappers and allocator/cell admission before
/// exposing this protocol to guest code. Kernel calls never execute guest code.
pub struct Hub<T: Payload> {
    identity: Rc<()>,
    root: t::TaskId,
    limits: Limits,
    usage: Usage,
    work: e::ServiceWork,
    channels: Vec<Channel<T>>,
    tickets: Vec<Option<usize>>,
    pending: Vec<Option<Pending<T>>>,
    visited: Vec<bool>,
    inspection: Vec<usize>,
}
impl<T: Payload> Hub<T> {
    pub fn prepare(
        scheduler: &t::Scheduler,
        root: &t::TaskId,
        limits: Limits,
        work: e::ServiceWork,
    ) -> Result<Self, Error> {
        if scheduler.parent(root)?.is_some() || scheduler.is_finished() {
            return Err(Error::WrongTask);
        }
        let mut channels = Vec::new();
        channels
            .try_reserve_exact(limits.channels)
            .map_err(|_| Error::Capacity)?;
        let mut tickets = Vec::new();
        tickets
            .try_reserve_exact(limits.handles)
            .map_err(|_| Error::Capacity)?;
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(limits.waiters)
            .map_err(|_| Error::Capacity)?;
        pending.resize_with(limits.waiters, || None);
        let mut visited = Vec::new();
        visited
            .try_reserve_exact(limits.channels)
            .map_err(|_| Error::Capacity)?;
        visited.resize(limits.channels, false);
        let mut inspection = Vec::new();
        inspection
            .try_reserve_exact(limits.channels)
            .map_err(|_| Error::Capacity)?;
        Ok(Self {
            identity: Rc::new(()),
            root: root.clone(),
            limits,
            usage: Usage::default(),
            work,
            channels,
            tickets,
            pending,
            visited,
            inspection,
        })
    }

    pub fn usage(&self) -> Usage {
        self.usage
    }

    pub fn create(&mut self, capacity: i64) -> Result<Setup, Error> {
        let capacity = usize::try_from(capacity).map_err(|_| Error::InvalidCapacity)?;
        if capacity == 0 {
            return Err(Error::InvalidCapacity);
        }
        if self.channels.len() == self.limits.channels {
            return Err(Error::ChannelLimit {
                limit: self.limits.channels,
            });
        }
        let slots = self
            .usage
            .reserved_slots
            .checked_add(capacity)
            .ok_or(Error::SlotLimit {
                limit: self.limits.slots,
            })?;
        if slots > self.limits.slots {
            return Err(Error::SlotLimit {
                limit: self.limits.slots,
            });
        }
        self.work.charge(capacity as u64)?;
        let values = queue::Queue::prepare(capacity).map_err(|_| Error::Capacity)?;
        let index = self.channels.len();
        self.channels.push(Channel {
            values,
            role: Role::Setup,
            producers: 1,
            sealed: false,
            capacity,
            send_head: None,
            send_tail: None,
            receiver_wait: None,
        });
        self.usage.channels += 1;
        self.usage.reserved_slots = slots;
        Ok(Setup(Key {
            identity: Rc::clone(&self.identity),
            index,
        }))
    }

    pub fn sender(&mut self, setup: &Setup) -> Result<Sender, Error> {
        let index = self.role(&setup.0, Role::Setup)?;
        self.mint(index)
    }

    pub fn clone_sender(&mut self, sender: &Sender) -> Result<Sender, Error> {
        let index = self.sender_index(sender)?;
        self.mint(index)
    }

    fn mint(&mut self, index: usize) -> Result<Sender, Error> {
        if self.tickets.len() == self.limits.handles {
            return Err(Error::HandleLimit {
                limit: self.limits.handles,
            });
        }
        let producers = self.channels[index]
            .producers
            .checked_add(1)
            .ok_or(Error::CountExhausted)?;
        self.work.charge(1)?;
        let ticket = self.tickets.len();
        self.tickets.push(Some(index));
        self.channels[index].producers = producers;
        self.usage.handles += 1;
        Ok(Sender {
            key: Key {
                identity: Rc::clone(&self.identity),
                index,
            },
            ticket,
        })
    }

    pub fn receiver(&mut self, setup: Setup) -> Result<Receiver, Failure<Setup>> {
        let index = match self.role(&setup.0, Role::Setup) {
            Ok(index) => index,
            Err(error) => {
                return Err(Failure {
                    error,
                    value: setup,
                });
            }
        };
        self.channels[index].role = Role::Receiver;
        self.channels[index].producers -= 1;
        self.channels[index].sealed = self.channels[index].producers == 0;
        Ok(Receiver(setup.0))
    }

    fn index(&self, key: &Key) -> Result<usize, Error> {
        if !Rc::ptr_eq(&self.identity, &key.identity) {
            return Err(Error::ForeignEndpoint);
        }
        if key.index >= self.channels.len() {
            return Err(Error::InvalidEndpoint);
        }
        Ok(key.index)
    }
    fn role(&self, key: &Key, role: Role) -> Result<usize, Error> {
        let index = self.index(key)?;
        if self.channels[index].role != role {
            return Err(Error::WrongRole);
        }
        Ok(index)
    }
    fn sender_index(&self, sender: &Sender) -> Result<usize, Error> {
        let index = self.index(&sender.key)?;
        if self.tickets.get(sender.ticket) != Some(&Some(index)) {
            return Err(Error::InvalidEndpoint);
        }
        Ok(index)
    }
    fn scheduler(&self, scheduler: &t::Scheduler) -> Result<(), Error> {
        scheduler.status(&self.root)?;
        Ok(())
    }
    fn running(&self, scheduler: &t::Scheduler, task: &t::TaskId) -> Result<(), Error> {
        self.scheduler(scheduler)?;
        if scheduler.status(task)? != t::Status::Running {
            return Err(Error::WrongTask);
        }
        Ok(())
    }
}
