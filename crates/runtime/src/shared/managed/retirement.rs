use super::*;

impl<'l, 't> Heap<'l, 't> {
    fn reclaimed(&mut self, index: usize) {
        if self.nodes[index]
            .as_ref()
            .is_some_and(|n| n.block.identity().is_err())
        {
            self.nodes[index] = None;
        }
    }
    pub(super) fn enqueue<P: a::Provider>(
        &mut self,
        index: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        if self.retiring.len() == self.limits.slots {
            return Err(Error::new(Kind::State));
        }
        let owner = self.owner(allocator)?;
        let bytes = allocator.bytes_mut(owner)?;
        let live = word(bytes, 8)
            .checked_sub(1)
            .ok_or_else(|| Error::new(Kind::State))?;
        put(bytes, 8, live);
        let mut value = self.nodes[index]
            .as_mut()
            .unwrap()
            .block
            .detach(allocator)?;
        value.begin(allocator)?;
        self.retiring.push(Retirement { index, value });
        Ok(())
    }
    pub(super) fn drop_reference<P: a::Provider>(
        &mut self,
        reference: Reference,
        kind: u64,
        ty: o::Type,
        customer: u64,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Error> {
        let index = Registry::new(self.owner, &self.nodes, self.layouts, self.limits)
            .resolve(reference, kind, ty, customer, allocator)?;
        self.consume(reference, allocator)?;
        if kind == WEAK {
            self.nodes[index]
                .as_mut()
                .unwrap()
                .block
                .release_weak(allocator)?;
            self.reclaimed(index);
        } else if kind == STRONG {
            if self.nodes[index]
                .as_mut()
                .unwrap()
                .block
                .release_strong(allocator)?
                == s::Release::Retire
            {
                self.enqueue(index, allocator)?;
            }
        } else {
            return Err(Error::new(Kind::State));
        }
        Ok(())
    }
    fn retire_token<P: a::Provider, F>(
        &mut self,
        token: &mut l::Token,
        customer: u64,
        allocator: &mut a::Allocator<P>,
        reason: c::Reason,
        context: &mut c::Context<'_>,
        cleaner: &mut F,
    ) -> Result<(), u32>
    where
        F: FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    {
        let edge = match token.leaf.ty {
            o::Type::Constructed(id) => {
                match self.layouts.types().table().constructed[id.0 as usize].constructor {
                    o::Constructor::Shared(ty) => Some((STRONG, ty)),
                    o::Constructor::Weak(ty) => Some((WEAK, ty)),
                    _ => None,
                }
            }
            _ => None,
        };
        if let Some((kind, ty)) = edge {
            let reference = Reference::parse(token.bytes).map_err(|_| 0x5352_0001u32)?;
            token.bytes.fill(0);
            self.drop_reference(reference, kind, ty, customer, allocator)
                .map_err(|_| 0x5352_0001u32)
        } else {
            cleaner(token, allocator, reason, context)
        }
    }
    /// Depth-first retirement uses admitted frames. A child completes before
    /// the next sibling; no Rust recursion or new retirement storage is used.
    pub(super) fn drain_pending<P: a::Provider, F>(
        &mut self,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        cleaner: &mut F,
    ) -> Result<(), Error>
    where
        F: FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    {
        while let Some(frame) = self.retiring.last_mut() {
            let index = frame.index;
            if let Some(token) = frame.value.next_token(allocator)? {
                frame.value.complete_leaf()?;
                let span = token.leaf.span;
                let mut callback =
                    c::Contextual::new(|token: &mut l::Token, reason, ctx: &mut c::Context<'_>| {
                        self.retire_token(token, index as u64 + 1, allocator, reason, ctx, cleaner)
                    });
                context.field(token, span, &mut callback);
            } else {
                let mut frame = self.retiring.pop().expect("pending frame");
                let completed = frame.value.finish(allocator)?;
                self.nodes[index]
                    .as_mut()
                    .unwrap()
                    .block
                    .complete_retirement(completed, allocator)?;
                self.reclaimed(index);
            }
        }
        Ok(())
    }
    /// Drain a rejected input or detached replacement outside publication, but
    /// retain its frame storage for the caller's recycle/release policy.
    pub fn retire_value<P: a::Provider>(
        &mut self,
        input: &mut l::Reservation<'l, 't>,
        span: Option<m::Span>,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        if !self.retiring.is_empty() {
            return Err(Error::new(Kind::State));
        }
        input.begin_retirement(span, allocator)?;
        while let Some(leaf) = input.next_leaf(allocator)? {
            let start = leaf.offset as usize;
            let bytes = input.data(allocator)?[start..start + 32]
                .try_into()
                .expect("token extent");
            input.complete_leaf()?;
            let mut callback =
                c::Contextual::new(|token: &mut l::Token, reason, ctx: &mut c::Context<'_>| {
                    self.retire_token(token, 0, allocator, reason, ctx, &mut cleaner)
                });
            context.field(l::Token { leaf, bytes }, leaf.span, &mut callback);
            self.drain_pending(allocator, context, &mut cleaner)?;
        }
        Ok(())
    }
    pub fn read<P: a::Provider>(
        &mut self,
        strong: &Strong,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Read, Error> {
        let index = self.resolve(strong.0, STRONG, ty, allocator)?;
        self.read_index(index, allocator)
    }
    pub(super) fn read_index<P: a::Provider>(
        &mut self,
        index: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<Read, Error> {
        let owner = self.owner(allocator)?;
        let readers = word(allocator.bytes(owner)?, 16);
        if readers >= self.limits.readers as u64 {
            return Err(limit());
        }
        let reference = self.vacancy(allocator, &mut self.work())?;
        self.nodes[index]
            .as_mut()
            .unwrap()
            .block
            .begin_read(self.limits.reference_count, allocator)?;
        self.register(reference, index, READ, allocator)?;
        put(allocator.bytes_mut(owner)?, 16, readers + 1);
        Ok(Read(reference))
    }
    pub fn view<P: a::Provider>(
        &self,
        read: Read,
        ty: o::Type,
        allocator: &a::Allocator<P>,
    ) -> Result<s::View<'_, 'l, 't>, Error> {
        let index = self.resolve(read.0, READ, ty, allocator)?;
        Ok(self.nodes[index]
            .as_ref()
            .unwrap()
            .block
            .payload(allocator)?)
    }
    pub fn end_read<P: a::Provider>(
        &mut self,
        read: Read,
        ty: o::Type,
        allocator: &mut a::Allocator<P>,
        context: &mut c::Context<'_>,
        mut cleaner: impl FnMut(
            &mut l::Token,
            &mut a::Allocator<P>,
            c::Reason,
            &mut c::Context<'_>,
        ) -> Result<(), u32>,
    ) -> Result<(), Error> {
        let index = self.resolve(read.0, READ, ty, allocator)?;
        let owner = self.owner(allocator)?;
        let readers = word(allocator.bytes(owner)?, 16)
            .checked_sub(1)
            .ok_or_else(|| Error::new(Kind::State))?;
        self.consume(read.0, allocator)?;
        put(allocator.bytes_mut(owner)?, 16, readers);
        if self.nodes[index]
            .as_mut()
            .unwrap()
            .block
            .end_read(allocator)?
            == s::Release::Retire
        {
            self.enqueue(index, allocator)?;
        }
        self.drain_pending(allocator, context, &mut cleaner)
    }
}
