use crate::{execution as e, task as t};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub channels: usize,
    /// Sum of live reserved queue capacities, including empty cells.
    pub slots: usize,
    /// Cumulative sender-ticket slots; identities are never recycled.
    pub handles: usize,
    pub waiters: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            channels: 64,
            slots: 4096,
            handles: 4096,
            waiters: 4096,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Usage {
    pub channels: usize,
    pub handles: usize,
    pub reserved_slots: usize,
    pub pending: usize,
    pub queued: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Capacity,
    InvalidCapacity,
    ChannelLimit { limit: usize },
    SlotLimit { limit: usize },
    HandleLimit { limit: usize },
    WaiterLimit { limit: usize },
    ForeignEndpoint,
    InspectionContextRequired,
    InvalidPayload,
    InspectionLimit,
    InvalidEndpoint,
    WrongRole,
    WrongTask,
    Busy,
    NotReady,
    StaleOperation,
    PositionExhausted,
    CountExhausted,
    Task(t::Error),
    Work(e::Failure),
}
impl From<t::Error> for Error {
    fn from(value: t::Error) -> Self {
        Self::Task(value)
    }
}
impl From<e::Failure> for Error {
    fn from(value: e::Failure) -> Self {
        Self::Work(value)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "channel: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rejection {
    Closed,
    InvalidPublication,
}

#[derive(Debug)]
pub struct Failure<T> {
    pub error: Error,
    pub value: T,
}

#[derive(Debug)]
pub enum Send<T> {
    Sent { position: u64 },
    Waiting(Operation),
    Rejected { reason: Rejection, value: T },
    Closing(T),
}

#[derive(Debug)]
pub enum Receive<T> {
    Value { value: T, position: u64 },
    Waiting(Operation),
    Closed,
    Closing,
}

#[derive(Debug)]
pub enum Cancelled<T> {
    Send(T),
    Receive,
}

/// Adapter-held completion claim; readiness remains private to the Hub.
#[derive(Clone, Debug)]
pub struct Operation {
    pub(super) identity: Rc<()>,
    pub(super) wait: t::WaitId,
    pub(super) origin: Option<lang_mir::unit::Span>,
}
impl PartialEq for Operation {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.identity, &other.identity) && self.wait == other.wait
    }
}
impl Eq for Operation {}
impl Operation {
    pub fn task(&self) -> &t::TaskId {
        self.wait.task()
    }
    pub fn ordinal(&self) -> u64 {
        self.wait.ordinal()
    }
    pub fn origin(&self) -> Option<lang_mir::unit::Span> {
        self.origin
    }
}

#[derive(Clone, Debug)]
pub(super) struct Key {
    pub identity: Rc<()>,
    pub index: usize,
}
#[derive(Debug)]
pub struct Setup(pub(super) Key);
#[derive(Debug)]
pub struct Sender {
    pub(super) key: Key,
    pub(super) ticket: usize,
}
#[derive(Debug)]
pub struct Receiver(pub(super) Key);

/// Explicit invocation storage context for publication inspection. It may read
/// actual guest storage but must not mutate values or reenter the Hub.
pub type PayloadInspection<'a, T> =
    dyn FnMut(&T, &mut dyn FnMut(&Receiver) -> Result<(), Error>) -> Result<(), Error> + 'a;

/// Trusted native adapters enumerate actual strong receive-authority edges.
/// Safe source requires independent type/move/loan proof before constructing
/// its payload wrapper; this interface is not a user-asserted Send capability.
/// The visitor must be finite and must not mutate payloads or reenter the Hub.
pub trait Payload {
    /// Original payload ownership provenance, independently from wait origin.
    fn owner_span(&self) -> Option<lang_mir::unit::Span> {
        None
    }
    fn receivers(&self, visit: &mut dyn FnMut(&Receiver) -> Result<(), Error>)
    -> Result<(), Error>;
}
impl Payload for i64 {
    fn receivers(&self, _: &mut dyn FnMut(&Receiver) -> Result<(), Error>) -> Result<(), Error> {
        Ok(())
    }
}
impl Payload for Vec<u8> {
    fn receivers(&self, _: &mut dyn FnMut(&Receiver) -> Result<(), Error>) -> Result<(), Error> {
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Deadlock {
    pub operation: Operation,
}
