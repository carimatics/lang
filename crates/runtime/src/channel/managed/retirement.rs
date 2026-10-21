use super::*;

/// A suspended queue or detached message. Its current Reservation is the sole
/// payload owner, transferred from the Hub before yielding any descendant.
pub(crate) struct Retired<'l, 'ty> {
    channel: Option<usize>,
    message: Option<Message<'l, 'ty>>,
}
impl<'l, 'ty> Retired<'l, 'ty> {
    pub(crate) fn message<P: a::Provider>(
        message: &mut Message<'l, 'ty>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Self, Error> {
        message
            .storage
            .as_mut()
            .ok_or(Error::State)?
            .begin_retirement(message.span, allocator)?;
        Ok(Self {
            channel: None,
            message: Some(Message {
                storage: message.storage.take(),
                span: message.span,
                channel: message.channel,
                extra: message.extra,
            }),
        })
    }
    pub(crate) fn next<P: a::Provider>(
        &mut self,
        channels: &mut Channels<'l, 'ty>,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<l::Token>, Error> {
        loop {
            if let Some(message) = self.message.as_mut() {
                let storage = message.storage.as_mut().ok_or(Error::State)?;
                if let Some(leaf) = storage.next_leaf(allocator)? {
                    let offset = leaf.offset as usize;
                    let bytes = storage.data(allocator)?[offset..offset + 32]
                        .try_into()
                        .map_err(|_| Error::State)?;
                    // The outer iterative drain now owns this detached token.
                    storage.complete_leaf()?;
                    return Ok(Some(l::Token { leaf, bytes }));
                }
                channels.recycle(self.message.take().ok_or(Error::State)?, allocator)?;
            }
            let Some(channel) = self.channel else {
                return Ok(None);
            };
            if let Some(entry) = channels.hub.channels[channel].values.pop() {
                channels.hub.usage.queued -= 1;
                let mut message = entry.value.value;
                message
                    .storage
                    .as_mut()
                    .ok_or(Error::State)?
                    .begin_retirement(message.span.or(entry.value.span), allocator)?;
                self.message = Some(message);
            } else {
                channels.finish_retirement(channel, allocator)?;
                self.channel = None;
                return Ok(None);
            }
        }
    }
}
impl<'l, 'ty> Channels<'l, 'ty> {
    /// Detach and revoke/wake before any payload callback. A returned frame
    /// keeps its guest header live until all queued descendants finish.
    pub(crate) fn begin_retirement<P: a::Provider>(
        &mut self,
        token: &mut l::Token,
        scheduler: &mut t::Scheduler,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Option<Retired<'l, 'ty>>, Error> {
        let channel = self.index(token, allocator)?;
        let o::Type::Constructed(id) = token.leaf.ty else {
            return Err(Error::MessageType);
        };
        let constructor = self.layouts.types().table().constructed[id.0 as usize].constructor;
        let handle = a::Handle::from_token(token.bytes[..24].try_into().unwrap())?;
        let count = u64::from_le_bytes(allocator.bytes(handle)?[32..40].try_into().unwrap());
        if count > 1 {
            allocator.bytes_mut(handle)?[32..40].copy_from_slice(&(count - 1).to_le_bytes());
            token.bytes.fill(0);
            return Ok(None);
        }
        if matches!(constructor, o::Constructor::Sender(_)) {
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
                && !self.backing[channel].draining
            {
                allocator.release(self.backing[channel].header.take().ok_or(Error::State)?)?;
            }
            return Ok(None);
        }
        let role = match constructor {
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
        self.backing[channel].draining = true;
        self.hub.refresh(scheduler)?;
        Ok(Some(Retired {
            channel: Some(channel),
            message: None,
        }))
    }
    fn finish_retirement<P: a::Provider>(
        &mut self,
        channel: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        if !self.backing[channel].draining || !self.hub.channels[channel].values.is_empty() {
            return Err(Error::State);
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
        self.backing[channel].draining = false;
        if self.hub.channels[channel].producers == 0 {
            allocator.release(self.backing[channel].header.take().ok_or(Error::State)?)?;
        }
        Ok(())
    }
    fn retire_frame<P: a::Provider, F>(
        &mut self,
        mut retired: Retired<'l, 'ty>,
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
        while let Some(token) = retired.next(self, allocator)? {
            let span = token.leaf.span.or(context.span());
            context.field(
                token,
                span,
                &mut c::Contextual::new(
                    |token: &mut l::Token, reason, context: &mut c::Context<'_>| {
                        cleaner(token, allocator, reason, context)
                    },
                ),
            );
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
        let retired = Retired::message(message, allocator)?;
        self.retire_frame(retired, allocator, context, cleaner)
    }
    pub fn retire<P: a::Provider, F>(
        &mut self,
        token: &mut l::Token,
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
        if let Some(retired) = self.begin_retirement(token, scheduler, allocator)? {
            self.retire_frame(retired, allocator, context, cleaner)?;
        }
        Ok(())
    }
}
