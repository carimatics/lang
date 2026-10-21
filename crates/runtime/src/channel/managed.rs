//! Guest-backed channel storage. The Hub remains the sole owner of queued and
//! parked values; the cache below contains only empty physical reservations.
//! Endpoint tokens are private, checked allocator/role/type identities.
use super::*;
use crate::{allocation as a, layout as l};
use lang_mir::unit::{self as m, ownership as o};

const HEADER: u64 = 0x4348_414e_4e45_4c31;
const HEADER_BYTES: u64 = 48;
const TICKET: u64 = 0x4348_5449_434b_4554;
mod operations;
#[cfg(test)]
mod tests;

#[derive(Debug)]
pub enum Error {
    Protocol(super::Error),
    Layout(l::Error),
    Allocation(a::Error),
    Preparation(m::Error),
    Cells { requested: usize, limit: usize },
    MessageType,
    State,
}
impl From<super::Error> for Error {
    fn from(e: super::Error) -> Self {
        Self::Protocol(e)
    }
}
impl From<l::Error> for Error {
    fn from(e: l::Error) -> Self {
        Self::Layout(e)
    }
}
impl From<a::Error> for Error {
    fn from(e: a::Error) -> Self {
        Self::Allocation(e)
    }
}
impl From<m::Error> for Error {
    fn from(e: m::Error) -> Self {
        Self::Preparation(e)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "guest channel: {self:?}")
    }
}
impl std::error::Error for Error {}

struct Backing<'l, 'ty> {
    header: Option<a::Handle>,
    message: o::Type,
    free: Vec<l::Reservation<'l, 'ty>>,
    retired: bool,
}
pub struct Message<'l, 'ty> {
    pub storage: Option<l::Reservation<'l, 'ty>>,
    pub span: Option<Span>,
    channel: usize,
    extra: bool,
}
impl std::fmt::Debug for Message<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GuestMessage")
            .field("channel", &self.channel)
            .field("span", &self.span)
            .field("extra", &self.extra)
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub struct SendFailure<T> {
    pub error: Error,
    pub value: T,
}
pub enum SendReadiness {
    Open,
    Closed,
    Closing,
}
pub struct Capture {
    token: l::Token,
}
impl Capture {
    pub fn token(&self) -> &l::Token {
        &self.token
    }
}
impl Payload for Message<'_, '_> {
    fn owner_span(&self) -> Option<Span> {
        self.span
    }
    fn receivers(
        &self,
        _: &mut dyn FnMut(&Receiver) -> Result<(), super::Error>,
    ) -> Result<(), super::Error> {
        Err(super::Error::InspectionContextRequired)
    }
}

pub struct Channels<'l, 'ty> {
    hub: Hub<Message<'l, 'ty>>,
    layouts: &'l l::Layouts<'ty>,
    allocator: a::Identity,
    backing: Vec<Backing<'l, 'ty>>,
    tickets: Vec<Option<a::Handle>>,
    cells: usize,
    cell_limit: usize,
}
impl<'l, 'ty> Channels<'l, 'ty> {
    #[expect(
        clippy::too_many_arguments,
        reason = "Prepare the bounded channel pools from the existing invocation allocator, scheduler, work and metadata budgets."
    )]
    pub fn prepare<P: a::Provider>(
        layouts: &'l l::Layouts<'ty>,
        allocator: &a::Allocator<P>,
        scheduler: &t::Scheduler,
        root: &t::TaskId,
        limits: Limits,
        cell_limit: usize,
        work: e::ServiceWork,
        budget: &mut m::Budget,
    ) -> Result<Self, Error> {
        // Account the exact bounded host pools used by the existing Hub.
        budget.allocate::<Channel<Message<'l, 'ty>>>(limits.channels)?;
        budget.allocate::<Option<usize>>(limits.handles)?;
        budget.allocate::<Option<Pending<Message<'l, 'ty>>>>(limits.waiters)?;
        budget.allocate::<bool>(limits.channels)?;
        budget.allocate::<usize>(limits.channels)?;
        // Queue entries and the empty-reservation cache allocate at creation,
        // but their maximum simultaneous host storage is admitted here.
        budget.allocate::<queue::Entry<Message<'l, 'ty>>>(limits.slots)?;
        budget.allocate::<l::Reservation<'l, 'ty>>(limits.slots)?;
        let backing = budget.vector(limits.channels)?;
        let tickets = budget.vector(limits.handles)?;
        let hub = Hub::prepare(scheduler, root, limits, work)?;
        Ok(Self {
            hub,
            layouts,
            allocator: allocator.identity(),
            backing,
            tickets,
            cells: 0,
            cell_limit,
        })
    }
    pub const fn live_cells(&self) -> usize {
        self.cells
    }
    pub fn usage(&self) -> Usage {
        self.hub.usage()
    }
    pub fn staging_cells<P: a::Provider>(
        &self,
        endpoint: &l::Token,
        allocator: &a::Allocator<P>,
    ) -> Result<usize, Error> {
        let index = self.index(endpoint, allocator)?;
        Ok(usize::from(self.backing[index].free.is_empty()))
    }
    /// Pin only protocol lifetime. No guest value or ordinary loan is cloned.
    pub fn capture<P: a::Provider>(
        &mut self,
        token: &l::Token,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Capture, Error> {
        self.index(token, allocator)?;
        if !matches!(token.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::Sender(_)|o::Constructor::Receiver(_)))
        {
            return Err(Error::MessageType);
        }
        let handle = a::Handle::from_token(token.bytes[..24].try_into().unwrap())?;
        let count = u64::from_le_bytes(allocator.bytes(handle)?[32..40].try_into().unwrap())
            .checked_add(1)
            .ok_or(super::Error::CountExhausted)?;
        self.hub.work.charge(1).map_err(super::Error::from)?;
        allocator.bytes_mut(handle)?[32..40].copy_from_slice(&count.to_le_bytes());
        Ok(Capture {
            token: l::Token {
                leaf: l::Leaf {
                    ty: token.leaf.ty,
                    offset: token.leaf.offset,
                    span: token.leaf.span,
                },
                bytes: token.bytes,
            },
        })
    }
    pub fn release_capture<P: a::Provider, F>(
        &mut self,
        capture: &mut Capture,
        scheduler: &mut t::Scheduler,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        cleaner: F,
    ) -> Result<(), Error>
    where
        F: FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    {
        self.retire(&mut capture.token, scheduler, allocator, context, cleaner)
    }
    fn check<P: a::Provider>(&self, allocator: &a::Allocator<P>) -> Result<(), Error> {
        if allocator.identity() != self.allocator {
            return Err(Error::State);
        }
        Ok(())
    }
    fn type_index(&self, ty: o::Type) -> Result<u64, Error> {
        let types = self.layouts.types();
        let canonical = types.canonical(ty).ok_or(Error::MessageType)?;
        Ok(match canonical {
            o::Type::I64 => 0,
            o::Type::Bool => 1,
            o::Type::Nominal(id) => 2 + u64::from(id.0),
            o::Type::Constructed(id) => {
                2 + types.table().definitions.len() as u64 + u64::from(id.0)
            }
        })
    }
    fn message(&self, endpoint: o::Type) -> Result<o::Type, Error> {
        let o::Type::Constructed(id) = endpoint else {
            return Err(Error::MessageType);
        };
        match self
            .layouts
            .types()
            .table()
            .constructed
            .get(id.0 as usize)
            .map(|c| c.constructor)
        {
            Some(
                o::Constructor::ChannelSetup(t)
                | o::Constructor::Sender(t)
                | o::Constructor::Receiver(t),
            ) => Ok(t),
            _ => Err(Error::MessageType),
        }
    }
    fn admit_cells(&self, amount: usize) -> Result<usize, Error> {
        let requested = self.cells.checked_add(amount).ok_or(Error::Cells {
            requested: usize::MAX,
            limit: self.cell_limit,
        })?;
        if requested > self.cell_limit {
            return Err(Error::Cells {
                requested,
                limit: self.cell_limit,
            });
        }
        Ok(requested)
    }
    /// Reserve every empty slot and its actual guest header before publishing
    /// setup. A failed attempt releases all physical reservations.
    pub fn create<P: a::Provider>(
        &mut self,
        endpoint: o::Type,
        capacity: i64,
        span: Option<Span>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<l::Token, Error> {
        self.check(allocator)?;
        let message = self.message(endpoint)?;
        if !matches!(endpoint,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::ChannelSetup(_)))
        {
            return Err(Error::MessageType);
        }
        if self.layouts.types().properties(message).is_none_or(|p| {
            !p.is_send()
                || p.contains_loan
                || p.contains_arena
                || p.contains_allocator
                || p.contains_owner
                || p.contains_task
        }) {
            return Err(Error::MessageType);
        }
        let capacity = usize::try_from(capacity)
            .ok()
            .filter(|c| *c != 0)
            .ok_or(super::Error::InvalidCapacity)?;
        if self.hub.channels.len() == self.hub.limits.channels {
            return Err(super::Error::ChannelLimit {
                limit: self.hub.limits.channels,
            }
            .into());
        }
        if self
            .hub
            .usage
            .reserved_slots
            .checked_add(capacity)
            .is_none_or(|n| n > self.hub.limits.slots)
        {
            return Err(super::Error::SlotLimit {
                limit: self.hub.limits.slots,
            }
            .into());
        }
        let cells = self.admit_cells(capacity)?;
        self.hub
            .work
            .check(capacity as u64)
            .map_err(super::Error::from)?;
        let mut free = Vec::new();
        free.try_reserve_exact(capacity)
            .map_err(|_| super::Error::Capacity)?;
        let header = allocator.allocate(a::Request::new(HEADER_BYTES, 8)?)?;
        let prepared = (|| -> Result<(), Error> {
            for _ in 0..capacity {
                free.push(l::Reservation::allocate(self.layouts, message, allocator)?);
            }
            Ok(())
        })();
        if let Err(error) = prepared {
            for mut slot in free {
                slot.release_storage(allocator)?;
            }
            allocator.release(header)?;
            return Err(error);
        }
        let setup = match self.hub.create(capacity as i64) {
            Ok(setup) => setup,
            Err(error) => {
                for mut slot in free {
                    slot.release_storage(allocator)?;
                }
                allocator.release(header)?;
                return Err(error.into());
            }
        };
        let index = setup.0.index;
        let words = [
            HEADER,
            self.type_index(message)?,
            capacity as u64,
            index as u64,
            1,
            0,
        ];
        for (part, word) in allocator
            .bytes_mut(header)?
            .as_chunks_mut::<8>()
            .0
            .iter_mut()
            .zip(words)
        {
            part.copy_from_slice(&word.to_le_bytes());
        }
        self.backing.push(Backing {
            header: Some(header),
            message,
            free,
            retired: false,
        });
        self.cells = cells;
        let mut bytes = [0; 32];
        bytes[..24].copy_from_slice(&header.token());
        bytes[24..].copy_from_slice(&(index as u64).to_le_bytes());
        Ok(l::Token {
            leaf: l::Leaf {
                ty: endpoint,
                offset: 0,
                span,
            },
            bytes,
        })
    }
    fn index<P: a::Provider>(
        &self,
        token: &l::Token,
        allocator: &a::Allocator<P>,
    ) -> Result<usize, Error> {
        self.check(allocator)?;
        let endpoint = self.message(token.leaf.ty)?;
        let o::Type::Constructed(id) = token.leaf.ty else {
            return Err(Error::MessageType);
        };
        let constructor = self.layouts.types().table().constructed[id.0 as usize].constructor;
        let handle = a::Handle::from_token(token.bytes[..24].try_into().unwrap())?;
        let number = usize::try_from(u64::from_le_bytes(token.bytes[24..].try_into().unwrap()))
            .map_err(|_| Error::State)?;
        let sender = matches!(constructor, o::Constructor::Sender(_));
        let index = if sender {
            if self.tickets.get(number).copied().flatten() != Some(handle) {
                return Err(Error::State);
            }
            let data = allocator.bytes(handle)?;
            if data.len() != 40 {
                return Err(Error::State);
            }
            let word = |i: usize| u64::from_le_bytes(data[i * 8..i * 8 + 8].try_into().unwrap());
            if word(0) != TICKET
                || word(1) != self.type_index(endpoint)?
                || word(3) != number as u64
                || word(4) == 0
            {
                return Err(Error::State);
            }
            let index = usize::try_from(word(2)).map_err(|_| Error::State)?;
            self.hub.sender_index(&Sender {
                key: Key {
                    identity: Rc::clone(&self.hub.identity),
                    index,
                },
                ticket: number,
            })?;
            index
        } else {
            number
        };
        let backing = self.backing.get(index).ok_or(Error::State)?;
        let header = backing.header.ok_or(Error::State)?;
        if !sender && header != handle {
            return Err(Error::State);
        }
        let bytes = allocator.bytes(header)?;
        if bytes.len() != HEADER_BYTES as usize {
            return Err(Error::State);
        }
        let word = |i: usize| u64::from_le_bytes(bytes[i * 8..i * 8 + 8].try_into().unwrap());
        if word(0) != HEADER
            || word(1) != self.type_index(endpoint)?
            || word(3) != index as u64
            || (!sender && word(4) == 0)
        {
            return Err(Error::State);
        }
        if !sender {
            let expected = if matches!(constructor, o::Constructor::ChannelSetup(_)) {
                Role::Setup
            } else {
                Role::Receiver
            };
            self.hub.role(
                &Key {
                    identity: Rc::clone(&self.hub.identity),
                    index,
                },
                expected,
            )?;
            if word(5) != u64::from(expected == Role::Receiver) {
                return Err(Error::State);
            }
        }
        if self.layouts.types().equivalent(backing.message, endpoint) != Some(true) {
            return Err(Error::MessageType);
        }
        Ok(index)
    }
    pub fn sender<P: a::Provider>(
        &mut self,
        source: &l::Token,
        result: o::Type,
        span: Option<Span>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<l::Token, Error> {
        let channel = self.index(source, allocator)?;
        let o::Type::Constructed(id) = result else {
            return Err(Error::MessageType);
        };
        if !matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::Sender(t) if self.layouts.types().equivalent(t,self.backing[channel].message)==Some(true))
        {
            return Err(Error::MessageType);
        }
        let source_sender = matches!(source.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::Sender(_)));
        if !source_sender {
            self.hub.role(
                &Key {
                    identity: Rc::clone(&self.hub.identity),
                    index: channel,
                },
                Role::Setup,
            )?;
        }
        if self.hub.tickets.len() == self.hub.limits.handles {
            return Err(super::Error::HandleLimit {
                limit: self.hub.limits.handles,
            }
            .into());
        }
        self.hub.channels[channel]
            .producers
            .checked_add(1)
            .ok_or(super::Error::CountExhausted)?;
        self.hub.work.check(1).map_err(super::Error::from)?;
        let descriptor = allocator.allocate(a::Request::new(40, 8)?)?;
        let sender = match self.hub.mint(channel) {
            Ok(sender) => sender,
            Err(error) => {
                allocator.release(descriptor)?;
                return Err(error.into());
            }
        };
        let words = [
            TICKET,
            self.type_index(self.backing[channel].message)?,
            channel as u64,
            sender.ticket as u64,
            1,
        ];
        for (part, word) in allocator
            .bytes_mut(descriptor)?
            .as_chunks_mut::<8>()
            .0
            .iter_mut()
            .zip(words)
        {
            part.copy_from_slice(&word.to_le_bytes());
        }
        self.tickets.push(Some(descriptor));
        let mut bytes = [0; 32];
        bytes[..24].copy_from_slice(&descriptor.token());
        bytes[24..].copy_from_slice(&(sender.ticket as u64).to_le_bytes());
        Ok(l::Token {
            leaf: l::Leaf {
                ty: result,
                offset: 0,
                span,
            },
            bytes,
        })
    }
    pub fn receiver<P: a::Provider>(
        &mut self,
        source: &l::Token,
        result: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<l::Token, Error> {
        if !matches!(source.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::ChannelSetup(_)))
        {
            return Err(Error::MessageType);
        }
        let channel = self.index(source, allocator)?;
        let o::Type::Constructed(id) = result else {
            return Err(Error::MessageType);
        };
        if !matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::Receiver(t) if self.layouts.types().equivalent(t,self.backing[channel].message)==Some(true))
        {
            return Err(Error::MessageType);
        }
        self.hub
            .receiver(Setup(Key {
                identity: Rc::clone(&self.hub.identity),
                index: channel,
            }))
            .map_err(|f| Error::Protocol(f.error))?;
        let header = self.backing[channel].header.ok_or(Error::State)?;
        allocator.bytes_mut(header)?[40..48].copy_from_slice(&1u64.to_le_bytes());
        Ok(l::Token {
            leaf: l::Leaf {
                ty: result,
                offset: 0,
                span: source.leaf.span,
            },
            bytes: source.bytes,
        })
    }
    /// Take empty, already charged storage. The caller moves a verified value
    /// into it before passing the Message to the Hub; it is not a second queue.
    pub fn stage<P: a::Provider>(
        &mut self,
        endpoint: &l::Token,
        span: Option<Span>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Message<'l, 'ty>, Error> {
        if !matches!(endpoint.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::Sender(_)))
        {
            return Err(Error::MessageType);
        }
        let channel = self.index(endpoint, allocator)?;
        let (storage, extra) = if let Some(storage) = self.backing[channel].free.pop() {
            (storage, false)
        } else {
            let cells = self.admit_cells(1)?;
            let storage =
                l::Reservation::allocate(self.layouts, self.backing[channel].message, allocator)?;
            self.cells = cells;
            (storage, true)
        };
        Ok(Message {
            storage: Some(storage),
            span,
            channel,
            extra,
        })
    }
    /// Return a reservation only after its payload was transferred or retired.
    /// Extra blocked/result storage stays charged until physical release.
    pub fn recycle<P: a::Provider>(
        &mut self,
        mut message: Message<'l, 'ty>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        self.check(allocator)?;
        self.recycle_storage(
            message.storage.take().ok_or(Error::State)?,
            message.channel,
            message.extra,
            allocator,
        )
    }
    fn recycle_storage<P: a::Provider>(
        &mut self,
        mut storage: l::Reservation<'l, 'ty>,
        channel: usize,
        extra: bool,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        storage.recycle(allocator)?;
        if extra || self.backing.get(channel).ok_or(Error::State)?.retired {
            storage.release_storage(allocator)?;
            self.cells = self.cells.checked_sub(1).ok_or(Error::State)?;
        } else {
            let free = &mut self.backing.get_mut(channel).ok_or(Error::State)?.free;
            if free.len() == free.capacity() {
                return Err(Error::State);
            }
            free.push(storage);
        }
        Ok(())
    }
    pub fn retire_message<P: a::Provider, F>(
        &mut self,
        message: &mut Message<'l, 'ty>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        cleaner: F,
    ) -> Result<(), Error>
    where
        F: FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    {
        message
            .storage
            .as_mut()
            .ok_or(Error::State)?
            .retire(allocator, context, cleaner)?;
        let storage = message.storage.take().ok_or(Error::State)?;
        self.recycle_storage(storage, message.channel, message.extra, allocator)
    }
    /// Detach the endpoint and revoke/wake waiters before any payload callback.
    /// Child retirement shares the caller's existing report and panic boundary.
    pub fn retire<P: a::Provider, F>(
        &mut self,
        token: &mut l::Token,
        scheduler: &mut t::Scheduler,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: F,
    ) -> Result<(), Error>
    where
        F: FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    {
        let channel = self.index(token, allocator)?;
        let o::Type::Constructed(id) = token.leaf.ty else {
            return Err(Error::MessageType);
        };
        let sender = matches!(
            self.layouts.types().table().constructed[id.0 as usize].constructor,
            o::Constructor::Sender(_)
        );
        let handle = a::Handle::from_token(token.bytes[..24].try_into().unwrap())?;
        let count = u64::from_le_bytes(allocator.bytes(handle)?[32..40].try_into().unwrap());
        if count > 1 {
            allocator.bytes_mut(handle)?[32..40].copy_from_slice(&(count - 1).to_le_bytes());
            token.bytes.fill(0);
            return Ok(());
        }
        if sender {
            let ticket = usize::try_from(u64::from_le_bytes(token.bytes[24..].try_into().unwrap()))
                .map_err(|_| Error::State)?;
            self.hub.release_sender(
                scheduler,
                &Sender {
                    key: Key {
                        identity: Rc::clone(&self.hub.identity),
                        index: channel,
                    },
                    ticket,
                },
            )?;
            token.bytes.fill(0);
            allocator.release(self.tickets[ticket].take().ok_or(Error::State)?)?;
            if self.hub.channels[channel].role == Role::Retired
                && self.hub.channels[channel].producers == 0
            {
                allocator.release(self.backing[channel].header.take().ok_or(Error::State)?)?;
            }
            return Ok(());
        }
        let role = match self.layouts.types().table().constructed[id.0 as usize].constructor {
            o::Constructor::ChannelSetup(_) => Role::Setup,
            o::Constructor::Receiver(_) => Role::Receiver,
            _ => return Err(Error::MessageType),
        };
        self.hub.role(
            &Key {
                identity: Rc::clone(&self.hub.identity),
                index: channel,
            },
            role,
        )?;
        token.bytes.fill(0);
        allocator.bytes_mut(handle)?[32..40].copy_from_slice(&0u64.to_le_bytes());
        if role == Role::Setup {
            self.hub.channels[channel].producers -= 1;
        }
        self.hub.channels[channel].role = Role::Retired;
        self.hub.channels[channel].sealed = true;
        self.backing[channel].retired = true;
        self.hub.refresh(scheduler)?;
        while let Some(entry) = self.hub.channels[channel].values.pop() {
            self.hub.usage.queued -= 1;
            context.field(
                entry.value.value,
                entry.value.span,
                &mut c::Contextual::new(
                    |message: &mut Message<'l, 'ty>, _, context: &mut c::Context<'_>| {
                        message
                            .storage
                            .as_mut()
                            .ok_or(0x4348_0001u32)?
                            .retire(allocator, context, &mut cleaner)
                            .map_err(|_| 0x4348_0001u32)?;
                        let storage = message.storage.take().ok_or(0x4348_0001u32)?;
                        self.recycle_storage(storage, message.channel, message.extra, allocator)
                            .map_err(|_| 0x4348_0001u32)
                    },
                ),
            );
        }
        let capacity = self.hub.channels[channel].capacity;
        self.hub.channels[channel].values.release();
        self.hub.channels[channel].capacity = 0;
        self.hub.usage.reserved_slots -= capacity;
        for mut slot in self.backing[channel].free.drain(..) {
            slot.release_storage(allocator)?;
            self.cells -= 1;
        }
        self.backing[channel].free = Vec::new();
        if self.hub.channels[channel].producers == 0 {
            allocator.release(self.backing[channel].header.take().ok_or(Error::State)?)?;
        }
        Ok(())
    }
}
