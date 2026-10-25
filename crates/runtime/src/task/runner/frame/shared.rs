//! Shared operations use observed authority and the invocation's managed heap.
use super::*;
use crate::shared::managed as sh;
fn outcome<T>(value: Result<T, sh::Error>) -> Result<Result<T, l::OwnershipError>, Reason> {
    if let Err(error) = &value
        && let Some(failure) = crate::execution::service_failure(error)
    {
        return Err(Reason::Execution(failure.into()));
    }
    match value {
        Ok(value) => Ok(Ok(value)),
        Err(sh::Error {
            kind: sh::Kind::Ownership(error),
            ..
        }) => Ok(Err(error)),
        Err(sh::Error {
            kind: sh::Kind::Allocator(_) | sh::Kind::Buffer(_),
            ..
        }) => Err(Reason::Execution(f::ExecutionFailure::Fault)),
        Err(_) => Err(Reason::Invariant),
    }
}
impl Access<'_, '_, '_, '_> {
    fn observation<P: a::Provider>(
        &self,
        target: Target,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Token, Reason> {
        let at = target.at()?;
        Ok(l::Token {
            leaf: l::Leaf {
                ty: at.ty(),
                offset: at.offset(),
                span: None,
            },
            bytes: self.token(target, allocator)?,
        })
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep verified operands, publication context and allocator explicit at the shared backend boundary."
    )]
    pub(super) fn shared_core<P: a::Provider>(
        &mut self,
        operation: o::core::Operation,
        ty: o::Type,
        result_loan: Option<b::LoanId>,
        first: usize,
        mut arguments: impl ExactSizeIterator<Item = usize>,
        to: usize,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        use o::core::Operation as O;
        if matches!(operation, O::SharedNew | O::SharedReplace) {
            let source = arguments.next().ok_or(Reason::Invariant)?;
            self.available(source)?;
            let copy = self.is_copy(source)?;
            let observed = if operation == O::SharedReplace {
                Some(self.observation(self.reference(first)?, allocator)?)
            } else {
                if !matches!(self.reference(first)?, Target::Allocator) {
                    return Err(Reason::Invariant);
                }
                None
            };
            let custody = if observed.is_some() {
                self.custody(self.reference(first)?, allocator)?
            } else {
                None
            };
            let heap = self.services.heap.as_mut().ok_or(Reason::Invariant)?;
            let ready = if self.services.opened {
                Ok(())
            } else {
                outcome(heap.open(allocator))?
            };
            let mut replaced = None;
            let input = self
                .frame
                .storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .cell_mut(source)?;
            let result =
                match ready {
                    Err(error) => Err(error),
                    Ok(()) => {
                        self.services.opened = true;
                        if let Some(observed) = &observed {
                            if copy {
                                outcome(heap.replace_observed_copy(
                                    observed, custody, input, span, allocator,
                                ))?
                                .map(|_| None)
                            } else {
                                outcome(heap.replace_observed_with_channels(
                                    observed,
                                    custody,
                                    input,
                                    span,
                                    allocator,
                                    self.services.channels.as_ref(),
                                ))?
                                .map(|old| {
                                    replaced = Some(old);
                                    None
                                })
                            }
                        } else {
                            outcome(if copy {
                                heap.allocate_copy(input, Some(span), allocator)
                            } else {
                                heap.allocate_from_with_channels(
                                    input,
                                    Some(span),
                                    allocator,
                                    self.services.channels.as_ref(),
                                )
                            })?
                            .map(|owner| Some(owner.into_token()))
                        }
                    }
                };
            let success = result.is_ok();
            if success && !copy && observed.is_none() {
                input.recycle(allocator)?;
                self.forget(source);
            }
            match result {
                Ok(Some(token)) => self.token_result(to, Ok(token), span, allocator)?,
                Ok(None) => self.immediate_result(to, Ok(l::Immediate::Unit), span, allocator)?,
                Err(error) => {
                    self.immediate_result(to, Err(l::Immediate::Ownership(error)), span, allocator)?
                }
            }
            if !copy {
                if let Some(old) = replaced {
                    self.frame.managed.as_mut().ok_or(Reason::Invariant)?.spans[source] = old;
                    self.cleanup_slot(source, c::Reason::Replacement, allocator)?;
                } else if !success {
                    self.cleanup_slot(source, c::Reason::ScopeExit, allocator)?;
                }
            }
            return Ok(());
        }
        let target = self.reference(first)?;
        let token = self.observation(target, allocator)?;
        let custody = self.custody(target, allocator)?;
        if operation == O::SharedRead {
            let id = result_loan.ok_or(Reason::Invariant)?;
            let origin = self.next_reference(id)?;
            let heap = self.services.heap.as_mut().ok_or(Reason::Invariant)?;
            return match outcome(heap.read_observed(&token, custody, allocator))? {
                Ok(read) => {
                    let at = heap
                        .view(read, ty, allocator)
                        .map_err(|error| Reason::service(&error))?
                        .root();
                    self.frame
                        .borrow_target(id, Target::Shared { origin, ty, at }, to)?;
                    self.frame.managed.as_mut().ok_or(Reason::Invariant)?.loans[id.0 as usize]
                        .shared = Some((read, ty));
                    self.loan_sum(to, 0, true, span, allocator)
                }
                Err(error) => {
                    self.immediate_result(to, Err(l::Immediate::Ownership(error)), span, allocator)
                }
            };
        }
        let action = match operation {
            O::SharedClone => sh::FieldAction::CloneStrong,
            O::WeakNew => sh::FieldAction::NewWeak,
            O::WeakClone => sh::FieldAction::CloneWeak,
            O::WeakUpgrade => sh::FieldAction::UpgradeWeak,
            _ => return Err(Reason::Unsupported),
        };
        let result = outcome(
            self.services
                .heap
                .as_mut()
                .ok_or(Reason::Invariant)?
                .retain_observed(&token, custody, action, allocator),
        )?;
        let result = result
            .map(|owner| match owner {
                sh::Retained::Strong(v) => v.into_token(),
                sh::Retained::Weak(v) => v.into_token(),
            })
            .map_err(l::Immediate::Ownership);
        self.token_result(to, result, span, allocator)
    }
}
