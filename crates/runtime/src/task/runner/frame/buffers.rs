//! The existing RFC 0007 services keep their allocator/token representation.
use super::*;
use crate::buffer;
impl Access<'_, '_, '_> {
    pub(super) fn buffer_core<P: a::Provider>(
        &mut self,
        operation: o::core::Operation,
        first: usize,
        mut arguments: impl ExactSizeIterator<Item = usize>,
        to: usize,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        use o::core::Operation as O;
        match operation {
            O::BytesGet | O::BytesSet => {
                let target = self.referenced_cell(first)?;
                let mut token = self.token(target, allocator)?;
                let l::Immediate::I64(index) =
                    self.read(arguments.next().ok_or(Reason::Invariant)?, allocator)?
                else {
                    return Err(Reason::Invariant);
                };
                let mut bytes = buffer::Bytes::from_token(&mut token, allocator)
                    .map_err(|_| Reason::Invariant)?;
                let outcome = if operation == O::BytesGet {
                    match bytes.get(allocator, index) {
                        Ok(value) => Ok(l::Immediate::I64(value)),
                        Err(buffer::Failure::Error(error)) => Err(l::Immediate::Bounds(error)),
                        Err(buffer::Failure::Fault(_)) => return Err(Reason::Invariant),
                    }
                } else {
                    let l::Immediate::I64(value) =
                        self.read(arguments.next().ok_or(Reason::Invariant)?, allocator)?
                    else {
                        return Err(Reason::Invariant);
                    };
                    match bytes.set(allocator, index, value) {
                        Ok(()) => Ok(l::Immediate::Unit),
                        Err(buffer::Failure::Error(error)) => Err(l::Immediate::Buffer(error)),
                        Err(buffer::Failure::Fault(_)) => return Err(Reason::Invariant),
                    }
                };
                self.immediate_result(to, outcome, span, allocator)
            }
            O::TextLen | O::TextEqual => {
                let target = self.referenced_cell(first)?;
                let mut token = self.token(target, allocator)?;
                let text = buffer::Text::from_token(&mut token, allocator)
                    .map_err(|_| Reason::Invariant)?;
                let value = if operation == O::TextLen {
                    l::Immediate::I64(text.len(allocator).map_err(|_| Reason::Invariant)?)
                } else {
                    let other = self.referenced_cell(arguments.next().ok_or(Reason::Invariant)?)?;
                    let mut token = self.token(other, allocator)?;
                    let other = buffer::Text::from_token(&mut token, allocator)
                        .map_err(|_| Reason::Invariant)?;
                    l::Immediate::Bool(
                        text.equal(&other, allocator)
                            .map_err(|_| Reason::Invariant)?,
                    )
                };
                self.write(to, value, allocator)
            }
            O::BytesClone | O::TextClone | O::TextBytes | O::TextConcat => {
                if !matches!(self.reference(first)?, Target::Allocator) {
                    return Err(Reason::Invariant);
                }
                let target = self.referenced_cell(arguments.next().ok_or(Reason::Invariant)?)?;
                let mut token = self.token(target, allocator)?;
                let outcome = if operation == O::BytesClone {
                    buffer::Bytes::from_token(&mut token, allocator)
                        .map_err(|_| Reason::Invariant)?
                        .try_clone(allocator)
                        .and_then(|mut value| {
                            value.take_token(allocator).map_err(buffer::Failure::Fault)
                        })
                } else {
                    let text = buffer::Text::from_token(&mut token, allocator)
                        .map_err(|_| Reason::Invariant)?;
                    match operation {
                        O::TextClone => text.try_clone(allocator).and_then(|mut value| {
                            value.take_token(allocator).map_err(buffer::Failure::Fault)
                        }),
                        O::TextBytes => text.to_bytes(allocator).and_then(|mut value| {
                            value.take_token(allocator).map_err(buffer::Failure::Fault)
                        }),
                        _ => {
                            let target =
                                self.referenced_cell(arguments.next().ok_or(Reason::Invariant)?)?;
                            let mut token = self.token(target, allocator)?;
                            let other = buffer::Text::from_token(&mut token, allocator)
                                .map_err(|_| Reason::Invariant)?;
                            text.concat(&other, allocator).and_then(|mut value| {
                                value.take_token(allocator).map_err(buffer::Failure::Fault)
                            })
                        }
                    }
                };
                let outcome = match outcome {
                    Ok(token) => Ok(token),
                    Err(buffer::Failure::Error(error)) => Err(l::Immediate::Allocation(error)),
                    Err(buffer::Failure::Fault(_)) => return Err(Reason::Invariant),
                };
                self.token_result(to, outcome, span, allocator)
            }
            O::TextFromUtf8 => {
                let source = self.root(first)?;
                let mut token = self.token(source, allocator)?;
                let mut bytes = buffer::Bytes::from_token(&mut token, allocator)
                    .map_err(|_| Reason::Invariant)?;
                let outcome = match bytes.take_text(allocator) {
                    Ok(mut text) => {
                        Ok(text.take_token(allocator).map_err(|_| Reason::Invariant)?)
                    }
                    Err(buffer::Failure::Error(error)) => Err(l::Immediate::Utf8(error)),
                    Err(buffer::Failure::Fault(_)) => {
                        let token = bytes.take_token(allocator).map_err(|_| Reason::Invariant)?;
                        self.store_token(source, token, allocator)?;
                        return Err(Reason::Invariant);
                    }
                };
                self.store_token(source, [0; 32], allocator)?;
                self.forget(first);
                self.token_result(to, outcome, span, allocator)
            }
            _ => Err(Reason::Unsupported),
        }
    }
}
