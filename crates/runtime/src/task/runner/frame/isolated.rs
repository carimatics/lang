//! Source service calls use the invocation's existing channel backing.
use super::channels::{allocation, failure};
use super::*;
use crate::channel::managed as g;

fn service_error(error: g::Error) -> Result<l::ServiceError, Reason> {
    if let Some(error) = allocation(&error) {
        return Ok(l::ServiceError::Allocation(error));
    }
    match error {
        g::Error::Service(error) => Ok(error),
        g::Error::Protocol(crate::channel::Error::InvalidCapacity) => {
            Ok(l::ServiceError::InvalidCapacity)
        }
        error => Err(failure(error)),
    }
}
impl<'l, 'ty> Access<'_, 'l, 'ty, '_> {
    fn retire_service<P: a::Provider>(
        &mut self,
        value: Option<g::Retired<'l, 'ty>>,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        if value.is_none() {
            return Ok(());
        }
        let services = &mut self.services;
        let types = self.frame.types;
        let managed = self.frame.managed.as_mut().ok_or(Reason::Invariant)?;
        let meter = services.work.clone();
        meter.unmetered(|| {
            managed
                .session
                .retire(
                    c::Slot(0),
                    value,
                    Some(span),
                    c::Reason::ScopeExit,
                    &mut c::Contextual::new(
                        |value: &mut Option<g::Retired<'l, 'ty>>,
                         _,
                         context: &mut c::Context<'_>| {
                            services.retire_channel(types, value, allocator, context)
                        },
                    ),
                )
                .map_err(|_| Reason::Cleanup)
        })
    }
    pub fn finish_service_endpoint<P: a::Provider>(
        &mut self,
        owner: &l::Token,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let value = self
            .services
            .channels
            .as_mut()
            .ok_or(Reason::Invariant)?
            .service_before_receive(owner, &mut self.services.scheduler, allocator)
            .map_err(failure)?;
        self.retire_service(value, span, allocator)
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Keep the verified operands, result loan and allocator explicit at the service boundary."
    )]
    pub(super) fn service_core<P: a::Provider>(
        &mut self,
        operation: o::core::Operation,
        result_loan: Option<b::LoanId>,
        first: usize,
        mut arguments: impl ExactSizeIterator<Item = usize>,
        to: usize,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        use o::core::Operation as O;
        let result = self.data()?.cell(to)?.root().ty();
        match operation {
            O::ServiceNew => {
                if !matches!(self.reference(first)?, Target::Allocator) {
                    return Err(Reason::Invariant);
                }
                let l::Immediate::I64(capacity) =
                    self.read(arguments.next().ok_or(Reason::Invariant)?, allocator)?
                else {
                    return Err(Reason::Invariant);
                };
                let source = arguments.next().ok_or(Reason::Invariant)?;
                self.available(source)?;
                let copy = self.is_copy(source)?;
                let o::Type::Constructed(id) = result else {
                    return Err(Reason::Invariant);
                };
                let o::Constructor::Result(endpoint, _) =
                    self.types.table().constructed[id.0 as usize].constructor
                else {
                    return Err(Reason::Invariant);
                };
                let input = self
                    .frame
                    .storage
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .cell_mut(source)?;
                let outcome = self
                    .services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .create_service(
                        endpoint,
                        capacity,
                        input,
                        Some(span),
                        self.services.inspection.as_mut().ok_or(Reason::Invariant)?,
                        self.services.heap.as_ref(),
                        allocator,
                    );
                let success = outcome.is_ok();
                if success && !copy {
                    input.recycle(allocator)?;
                    self.forget(source);
                }
                let outcome = match outcome {
                    Ok(token) => Ok(token.bytes),
                    Err(error) => Err(l::Immediate::Service(service_error(error)?)),
                };
                self.token_result(to, outcome, span, allocator)?;
                if !success && !copy {
                    self.cleanup_slot(source, c::Reason::ScopeExit, allocator)?;
                }
                Ok(())
            }
            O::ServiceHandle | O::ServiceClone => {
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
            O::ServiceOwner => {
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
                let owner = self
                    .services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .receiver(&token, result, allocator)
                    .map_err(failure)?;
                self.store_token(self.root(to)?, owner.bytes, allocator)?;
                self.forget(first);
                self.mark(to, span)
            }
            O::ServiceRead => {
                let owner = self.endpoint(first, allocator)?;
                let id = result_loan.ok_or(Reason::Invariant)?;
                let origin = self.next_reference(id)?;
                let channels = self.services.channels.as_mut().ok_or(Reason::Invariant)?;
                match channels.service_read(&owner, allocator) {
                    Ok(read) => {
                        let at = channels
                            .service_view(read, allocator)
                            .map_err(failure)?
                            .root();
                        self.frame
                            .borrow_target(id, Target::Service { origin, at }, to)?;
                        self.frame.managed.as_mut().ok_or(Reason::Invariant)?.loans
                            [id.0 as usize]
                            .service = Some(read);
                        self.loan_sum(to, 0, true, span, allocator)
                    }
                    Err(error) => self.immediate_result(
                        to,
                        Err(l::Immediate::Service(service_error(error)?)),
                        span,
                        allocator,
                    ),
                }
            }
            O::ServiceReplace => {
                let owner = self.endpoint(first, allocator)?;
                let source = arguments.next().ok_or(Reason::Invariant)?;
                self.available(source)?;
                let copy = self.is_copy(source)?;
                let input = self
                    .frame
                    .storage
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .cell_mut(source)?;
                let outcome = self
                    .services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .service_replace(
                        &owner,
                        input,
                        span,
                        self.services.inspection.as_mut().ok_or(Reason::Invariant)?,
                        self.services.heap.as_ref(),
                        allocator,
                    );
                let old = match outcome {
                    Ok(old) => old,
                    Err(error) => {
                        self.immediate_result(
                            to,
                            Err(l::Immediate::Service(service_error(error)?)),
                            span,
                            allocator,
                        )?;
                        if !copy {
                            self.cleanup_slot(source, c::Reason::ScopeExit, allocator)?;
                        }
                        return Ok(());
                    }
                };
                self.immediate_result(to, Ok(l::Immediate::Unit), span, allocator)?;
                if !copy {
                    self.frame.managed.as_mut().ok_or(Reason::Invariant)?.spans[source] = old;
                    self.cleanup_slot(source, c::Reason::Replacement, allocator)?;
                }
                Ok(())
            }
            O::ServiceFinish => {
                let owner = self.endpoint(first, allocator)?;
                let value = self
                    .services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .service_finish(&owner, &mut self.services.scheduler, allocator)
                    .map_err(failure)?;
                self.retire_service(value, span, allocator)?;
                self.write(to, l::Immediate::Unit, allocator)
            }
            O::ServiceCancel => {
                let handle = self.endpoint(first, allocator)?;
                let value = self
                    .services
                    .channels
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .service_cancel(&handle, &mut self.services.scheduler, allocator)
                    .map_err(failure)?;
                self.retire_service(value, span, allocator)?;
                self.write(to, l::Immediate::Unit, allocator)
            }
            _ => Err(Reason::Unsupported),
        }
    }
}
