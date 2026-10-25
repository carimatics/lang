use super::*;
use crate::channel::managed as g;

pub(super) fn failure(error: g::Error) -> Reason {
    match error {
        g::Error::Protocol(crate::channel::Error::Work(e)) => Reason::Control(e),
        g::Error::Layout(l::Error::ServiceWork(e)) => Reason::Control(e),
        error => Reason::Channel(error),
    }
}
pub(super) fn allocation(error: &g::Error) -> Option<crate::buffer::AllocError> {
    let error = match error {
        g::Error::Allocation(e) | g::Error::Layout(l::Error::Allocation(e)) => e,
        _ => return None,
    };
    Some(match error {
        a::Error::Limit(_) => crate::buffer::AllocError::Limit,
        a::Error::OutOfMemory => crate::buffer::AllocError::OutOfMemory,
        a::Error::Overflow => crate::buffer::AllocError::Overflow,
        _ => return None,
    })
}
impl Access<'_, '_, '_, '_> {
    pub fn capture_channel<P: a::Provider>(
        &mut self,
        from: usize,
        destination: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let token = self.endpoint(from, allocator)?;
        let capture = self
            .services
            .channels
            .as_mut()
            .ok_or(Reason::Invariant)?
            .capture(&token, allocator)
            .map_err(failure)?;
        let slot = self
            .channel_prepared
            .get_mut(destination)
            .ok_or(Reason::Invariant)?;
        if slot.is_some() {
            return Err(Reason::Invariant);
        }
        *slot = Some(capture);
        self.forget(from);
        Ok(())
    }
    pub fn release_channel_capture<P: a::Provider>(
        &mut self,
        capture: g::Capture,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let services = &mut self.services;
        let types = self.frame.types;
        let managed = self.frame.managed.as_mut().ok_or(Reason::Invariant)?;
        let meter = services.work.clone();
        meter.unmetered(|| {
            managed
                .session
                .retire(
                    c::Slot(0),
                    capture,
                    Some(span),
                    c::Reason::ScopeExit,
                    &mut c::Contextual::new(
                        |capture: &mut g::Capture, _, context: &mut c::Context<'_>| {
                            services.retire(types, capture.token_mut(), allocator, context)
                        },
                    ),
                )
                .map_err(|_| Reason::Cleanup)
        })
    }
    pub fn discard_channels<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let mut failed = false;
        for index in (0..self.channel_prepared.len()).rev() {
            if let Some(capture) = self.channel_prepared[index].take() {
                let span = self.origin;
                failed |= self
                    .release_channel_capture(capture, span, allocator)
                    .is_err();
            }
        }
        if failed { Err(Reason::Cleanup) } else { Ok(()) }
    }
    pub fn endpoint<P: a::Provider>(
        &self,
        from: usize,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Token, Reason> {
        let ty = self.data()?.cell(from)?.root().ty();
        let o::Type::Constructed(id) = ty else {
            return Err(Reason::Invariant);
        };
        let o::Constructor::Loan(_, target) =
            self.types.table().constructed[id.0 as usize].constructor
        else {
            return Err(Reason::Invariant);
        };
        let bytes = self.token(self.reference(from)?, allocator)?;
        Ok(l::Token {
            leaf: l::Leaf {
                ty: target,
                offset: 0,
                span: Some(self.origin),
            },
            bytes,
        })
    }
    pub(super) fn channel_core<P: a::Provider>(
        &mut self,
        operation: o::core::Operation,
        first: usize,
        mut arguments: impl ExactSizeIterator<Item = usize>,
        to: usize,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        use o::core::Operation as O;
        let result = self.data()?.cell(to)?.root().ty();
        match operation {
            O::ChannelNew => {
                if !matches!(self.reference(first)?, Target::Allocator) {
                    return Err(Reason::Invariant);
                }
                let l::Immediate::I64(capacity) =
                    self.read(arguments.next().ok_or(Reason::Invariant)?, allocator)?
                else {
                    return Err(Reason::Invariant);
                };
                let o::Type::Constructed(id) = result else {
                    return Err(Reason::Invariant);
                };
                let o::Constructor::Result(endpoint, _) =
                    self.types.table().constructed[id.0 as usize].constructor
                else {
                    return Err(Reason::Invariant);
                };
                let outcome = self
                    .services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .create(endpoint, capacity, Some(span), allocator);
                let outcome = match outcome {
                    Ok(token) => Ok(token.bytes),
                    Err(g::Error::Protocol(crate::channel::Error::InvalidCapacity)) => {
                        Err(l::Immediate::Channel(l::ChannelError::InvalidCapacity))
                    }
                    Err(error) => {
                        if let Some(e) = allocation(&error) {
                            Err(l::Immediate::Channel(l::ChannelError::Allocation(e)))
                        } else {
                            return Err(failure(error));
                        }
                    }
                };
                self.token_result(to, outcome, span, allocator)
            }
            O::ChannelSender | O::ChannelClone => {
                let source = self.endpoint(first, allocator)?;
                let token = self
                    .services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .sender(&source, result, Some(span), allocator)
                    .map_err(failure)?;
                self.store_token(self.root(to)?, token.bytes, allocator)?;
                self.mark(to, span)
            }
            O::ChannelReceiver => {
                self.available(first)?;
                let source = self.data()?.cell(first)?;
                let token = l::Token {
                    leaf: l::Leaf {
                        ty: source.root().ty(),
                        offset: 0,
                        span: Some(span),
                    },
                    bytes: self.token(self.root(first)?, allocator)?,
                };
                let result = self
                    .services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .receiver(&token, result, allocator)
                    .map_err(failure)?;
                self.store_token(self.root(to)?, result.bytes, allocator)?;
                self.forget(first);
                self.mark(to, span)
            }
            O::ChannelSeal => {
                let receiver = self.endpoint(first, allocator)?;
                self.services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .seal(&mut self.services.scheduler, &receiver, allocator)
                    .map_err(failure)?;
                self.write(to, l::Immediate::Unit, allocator)
            }
            O::ChannelUnsent => {
                let case = self.case(first, allocator)?;
                if case == 0 {
                    return Err(Reason::Panic(Some(-4)));
                }
                if case > 2 {
                    return Err(Reason::Invariant);
                }
                self.extract(first, to, case, -4, allocator)
            }
            _ => Err(Reason::Invariant),
        }
    }
}
