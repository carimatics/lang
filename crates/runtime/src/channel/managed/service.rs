//! Isolated state lives beside its queue in the existing managed backing.
//! The private VM profile admits Share state, so it cannot hide receiver edges.
use super::*;
#[cfg(test)]
mod tests;

pub(super) struct State<'l, 'ty> {
    pub ty: o::Type,
    pub storage: Option<l::Reservation<'l, 'ty>>,
    pub span: Option<Span>,
    generation: u64,
    readers: usize,
    phase: Phase,
    requested: bool,
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    Idle,
    Active,
    Closed,
}
#[derive(Clone, Copy)]
pub(crate) struct Read {
    allocator: a::Identity,
    channel: usize,
    generation: u64,
}
/// A read coordinate exposes observation only, with the invocation's meter.
pub(crate) struct View<'a, 'l, 'ty> {
    payload: &'a l::Reservation<'l, 'ty>,
    work: &'a e::ServiceWork,
}
impl<'l, 'ty> View<'_, 'l, 'ty> {
    pub(crate) fn root(&self) -> l::Location {
        self.payload.root()
    }
    pub(crate) fn child<P: a::Provider>(
        &self,
        at: l::Location,
        field: u32,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Location, l::Error> {
        self.payload.child(at, field, allocator)
    }
    pub(crate) fn case<P: a::Provider>(
        &self,
        at: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<u32, l::Error> {
        self.payload.case(at, allocator)
    }
    pub(crate) fn read_token<P: a::Provider>(
        &self,
        at: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Token, l::Error> {
        self.payload
            .read_token_with_service_work(at, allocator, Some(self.work))
    }
    pub(crate) fn read_immediate<P: a::Provider>(
        &self,
        at: l::Location,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Immediate, l::Error> {
        self.payload
            .read_immediate_with_service_work(at, allocator, Some(self.work))
    }
    pub(crate) fn copy_into<P: a::Provider>(
        &self,
        at: l::Location,
        destination: &l::Reservation<'l, 'ty>,
        target: l::Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), l::Error> {
        destination.copy_from_with_service_work(
            target,
            self.payload,
            at,
            allocator,
            Some(self.work),
        )
    }
}
impl<'l, 'ty> Channels<'l, 'ty> {
    #[expect(
        clippy::too_many_arguments,
        reason = "Creation checks the real state using the invocation's existing bounded inspection and heap contexts."
    )]
    pub(crate) fn create_service<P: a::Provider>(
        &mut self,
        endpoint: o::Type,
        capacity: i64,
        input: &mut l::Reservation<'l, 'ty>,
        span: Option<Span>,
        inspector: &mut crate::inspection::Inspector<'l, 'ty>,
        heap: Option<&crate::shared::managed::Heap<'l, 'ty>>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<l::Token, Error> {
        self.check(allocator)?;
        let o::Type::Constructed(id) = endpoint else {
            return Err(Error::MessageType);
        };
        let o::Constructor::ServiceSetup(ty, _) =
            self.layouts.types().table().constructed[id.0 as usize].constructor
        else {
            return Err(Error::MessageType);
        };
        let properties = self
            .layouts
            .types()
            .properties(ty)
            .ok_or(Error::MessageType)?;
        if !properties.is_share()
            || !properties.is_send()
            || self.layouts.types().equivalent(ty, input.root().ty()) != Some(true)
            || !std::ptr::eq(input.layouts(), self.layouts)
        {
            return Err(Error::MessageType);
        }
        if capacity <= 0 {
            return Err(super::super::Error::InvalidCapacity.into());
        }
        // State is an actual charged cell, including during a failed attempt.
        self.admit_cells(
            usize::try_from(capacity)
                .map_err(|_| super::super::Error::InvalidCapacity)?
                .checked_add(1)
                .ok_or(Error::State)?,
        )?;
        self.inspect_state(input, span, inspector, heap, allocator)?;
        let mut storage = l::Reservation::allocate(self.layouts, ty, allocator)?;
        let transfer = if properties.is_copy {
            storage.copy_from_with_service_work(
                storage.root(),
                input,
                input.root(),
                allocator,
                Some(&self.hub.work),
            )
        } else {
            input.move_root_into_with_service_work(
                &storage,
                storage.root(),
                allocator,
                Some(&self.hub.work),
            )
        };
        if let Err(error) = transfer {
            storage.release_storage(allocator)?;
            return Err(error.into());
        }
        let mut state = Some(State {
            ty,
            storage: Some(storage),
            span,
            generation: 0,
            readers: 0,
            phase: Phase::Idle,
            requested: false,
        });
        let result = self.create_inner(endpoint, capacity, span, allocator, &mut state);
        if result.is_ok() {
            self.cells += 1;
        } else {
            let mut storage = state.take().and_then(|s| s.storage).ok_or(Error::State)?;
            if !properties.is_copy {
                input.recycle(allocator)?;
                storage.move_root_into(input, input.root(), allocator)?;
            }
            storage.release_storage(allocator)?;
        }
        result
    }
    fn state_index<P: a::Provider>(
        &self,
        owner: &l::Token,
        allocator: &a::Allocator<P>,
    ) -> Result<usize, Error> {
        let index = self.index(owner, allocator)?;
        if !matches!(owner.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::ServiceOwner(..)))
        {
            return Err(Error::MessageType);
        }
        Ok(index)
    }
    fn inspect_state<P: a::Provider>(
        &self,
        input: &l::Reservation<'l, 'ty>,
        span: Option<Span>,
        inspector: &mut crate::inspection::Inspector<'l, 'ty>,
        heap: Option<&crate::shared::managed::Heap<'l, 'ty>>,
        allocator: &a::Allocator<P>,
    ) -> Result<(), Error> {
        Self::inspect_value(
            self.layouts,
            &self.backing,
            &self.tickets,
            &self.hub.identity,
            inspector,
            heap,
            input,
            span,
            allocator,
            &self.hub.work,
            &mut |_| Err(super::super::Error::InvalidPayload),
        )
        .map_err(Error::Protocol)
    }
    pub(crate) fn service_read<P: a::Provider>(
        &mut self,
        owner: &l::Token,
        allocator: &a::Allocator<P>,
    ) -> Result<Read, Error> {
        let index = self.state_index(owner, allocator)?;
        let state = self.backing[index]
            .service
            .as_mut()
            .ok_or(Error::MessageType)?;
        if state.phase == Phase::Closed {
            return Err(Error::Service(l::ServiceError::Unavailable));
        }
        if state.phase != Phase::Active {
            return Err(Error::Service(l::ServiceError::NoActiveHandler));
        }
        let readers = state.readers.checked_add(1).ok_or(Error::State)?;
        self.hub.work.charge(1).map_err(super::super::Error::from)?;
        state.readers = readers;
        Ok(Read {
            allocator: self.allocator,
            channel: index,
            generation: state.generation,
        })
    }
    pub(crate) fn service_view<P: a::Provider>(
        &self,
        read: Read,
        allocator: &a::Allocator<P>,
    ) -> Result<View<'_, 'l, 'ty>, Error> {
        self.check(allocator)?;
        let state = self
            .backing
            .get(read.channel)
            .and_then(|b| b.service.as_ref())
            .ok_or(Error::State)?;
        if read.allocator != self.allocator
            || read.generation != state.generation
            || state.readers == 0
            || state.phase != Phase::Active
        {
            return Err(Error::State);
        }
        Ok(View {
            payload: state.storage.as_ref().ok_or(Error::State)?,
            work: &self.hub.work,
        })
    }
    pub(crate) fn end_service_read<P: a::Provider>(
        &mut self,
        read: Read,
        allocator: &a::Allocator<P>,
    ) -> Result<(), Error> {
        self.service_view(read, allocator)?;
        self.backing[read.channel]
            .service
            .as_mut()
            .ok_or(Error::State)?
            .readers -= 1;
        Ok(())
    }
    pub(crate) fn service_replace<P: a::Provider>(
        &mut self,
        owner: &l::Token,
        incoming: &mut l::Reservation<'l, 'ty>,
        span: Span,
        inspector: &mut crate::inspection::Inspector<'l, 'ty>,
        heap: Option<&crate::shared::managed::Heap<'l, 'ty>>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<Span>, Error> {
        let index = self.state_index(owner, allocator)?;
        let state = self.backing[index]
            .service
            .as_ref()
            .ok_or(Error::MessageType)?;
        if state.phase == Phase::Closed {
            return Err(Error::Service(l::ServiceError::Unavailable));
        }
        if state.phase != Phase::Active {
            return Err(Error::Service(l::ServiceError::NoActiveHandler));
        }
        if state.readers != 0 {
            return Err(Error::State);
        }
        let generation = state.generation.checked_add(1).ok_or(Error::State)?;
        self.inspect_state(incoming, Some(span), inspector, heap, allocator)?;
        let state = self.backing[index]
            .service
            .as_mut()
            .ok_or(Error::MessageType)?;
        let storage = state.storage.as_mut().ok_or(Error::State)?;
        storage.exchange_root_at_with_service_work(
            storage.root(),
            incoming,
            span,
            allocator,
            Some(&self.hub.work),
        )?;
        state.generation = generation;
        Ok(state.span.replace(span))
    }
    pub(crate) fn service_finish<P: a::Provider>(
        &mut self,
        owner: &l::Token,
        scheduler: &mut t::Scheduler,
        allocator: &a::Allocator<P>,
    ) -> Result<Option<Retired<'l, 'ty>>, Error> {
        let index = self.state_index(owner, allocator)?;
        self.service_boundary(index)?;
        let state = self.backing[index].service.as_ref().ok_or(Error::State)?;
        if state.phase != Phase::Closed
            && (state.requested
                || (self.hub.channels[index].sealed && self.hub.channels[index].values.is_empty()))
        {
            self.begin_service_disposal(index, scheduler).map(Some)
        } else {
            Ok(None)
        }
    }
    pub(crate) fn service_before_receive<P: a::Provider>(
        &mut self,
        owner: &l::Token,
        scheduler: &mut t::Scheduler,
        allocator: &a::Allocator<P>,
    ) -> Result<Option<Retired<'l, 'ty>>, Error> {
        if !matches!(owner.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::ServiceOwner(..)))
        {
            return Ok(None);
        }
        self.service_finish(owner, scheduler, allocator)
    }
    pub(crate) fn service_cancel<P: a::Provider>(
        &mut self,
        handle: &l::Token,
        scheduler: &mut t::Scheduler,
        allocator: &a::Allocator<P>,
    ) -> Result<Option<Retired<'l, 'ty>>, Error> {
        let index = self.index(handle, allocator)?;
        if !matches!(handle.leaf.ty,o::Type::Constructed(id) if matches!(self.layouts.types().table().constructed[id.0 as usize].constructor,o::Constructor::ServiceHandle(_)))
        {
            return Err(Error::MessageType);
        }
        let state = self.backing[index]
            .service
            .as_mut()
            .ok_or(Error::MessageType)?;
        if state.requested || state.phase == Phase::Closed {
            return Ok(None);
        }
        state.requested = true;
        self.hub.channels[index].sealed = true;
        self.hub.refresh(scheduler)?;
        self.service_unlink(index);
        if self.backing[index]
            .service
            .as_ref()
            .ok_or(Error::State)?
            .phase
            == Phase::Active
        {
            return Ok(None);
        }
        self.begin_service_disposal(index, scheduler).map(Some)
    }
    fn begin_service_disposal(
        &mut self,
        index: usize,
        scheduler: &mut t::Scheduler,
    ) -> Result<Retired<'l, 'ty>, Error> {
        if self.backing[index].draining {
            return Err(Error::State);
        }
        let state = self.backing[index].service.as_mut().ok_or(Error::State)?;
        if state.readers != 0 || state.phase == Phase::Active || state.phase == Phase::Closed {
            return Err(Error::State);
        }
        state.phase = Phase::Closed;
        self.hub.channels[index].sealed = true;
        self.backing[index].retired = true;
        self.backing[index].draining = true;
        self.hub.refresh(scheduler)?;
        self.service_unlink(index);
        Ok(Retired::channel(index))
    }
    pub(super) fn service_unlink(&mut self, index: usize) {
        if self.backing[index].service.is_none() {
            return;
        }
        for slot in 0..self.hub.pending.len() {
            if self.hub.pending[slot]
                .as_ref()
                .is_some_and(|p| p.channel == index && !p.revoked)
            {
                self.hub.unlink(slot);
            }
        }
    }
    pub(super) fn service_boundary(&mut self, index: usize) -> Result<(), Error> {
        if let Some(state) = self.backing[index].service.as_mut() {
            if state.readers != 0 {
                return Err(Error::State);
            }
            if state.phase != Phase::Closed {
                state.phase = Phase::Idle;
            }
        }
        Ok(())
    }
    pub(super) fn service_close(&mut self, index: usize) -> Result<(), Error> {
        if let Some(state) = self.backing[index].service.as_mut() {
            if state.readers != 0 {
                return Err(Error::State);
            }
            state.phase = Phase::Closed;
        }
        Ok(())
    }
    pub(super) fn service_received(&mut self, index: usize, received: bool) -> Result<(), Error> {
        if let Some(state) = self.backing[index].service.as_mut() {
            if state.readers != 0
                || state.phase == Phase::Active
                || (received && (state.phase == Phase::Closed || state.requested))
            {
                return Err(Error::State);
            }
            if received {
                state.phase = Phase::Active;
            }
        }
        Ok(())
    }
}
