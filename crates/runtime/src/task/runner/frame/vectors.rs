//! Collection operations retain the runtime's validated ownership transfers.
use super::*;
use crate::{buffer, vector as v};
fn fault(error: v::Error) -> Reason {
    if let Some(failure) = crate::execution::service_failure(&error) {
        return Reason::Execution(failure.into());
    }
    match error {
        v::Error::LoanLimit | v::Error::Allocation(a::Error::Limit(_) | a::Error::OutOfMemory) => {
            Reason::Execution(f::ExecutionFailure::AllocationLimit)
        }
        v::Error::Allocation(a::Error::Overflow) => {
            Reason::Execution(f::ExecutionFailure::CounterOverflow)
        }
        v::Error::BorrowConflict => Reason::Execution(f::ExecutionFailure::Fault),
        _ => Reason::Invariant,
    }
}
fn allocation<T>(value: Result<T, v::Error>) -> Result<Result<T, buffer::AllocError>, Reason> {
    match value {
        Ok(v) => Ok(Ok(v)),
        Err(v::Error::Allocation(a::Error::Limit(_))) => Ok(Err(buffer::AllocError::Limit)),
        Err(v::Error::Allocation(a::Error::OutOfMemory)) => {
            Ok(Err(buffer::AllocError::OutOfMemory))
        }
        Err(v::Error::Allocation(a::Error::Overflow)) => Ok(Err(buffer::AllocError::Overflow)),
        Err(e) => Err(fault(e)),
    }
}
impl Access<'_, '_, '_, '_> {
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep verified operands, publication context and allocator explicit at the shared backend boundary."
    )]
    pub(super) fn vector_core<P: a::Provider>(
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
        if operation == O::VecNew {
            if !matches!(self.reference(first)?, Target::Allocator) {
                return Err(Reason::Invariant);
            }
            let result = match allocation(v::Vector::new(self.services.layouts, ty, allocator))? {
                Ok(mut value) => Ok(value.take_token(allocator).map_err(fault)?),
                Err(error) => Err(l::Immediate::Allocation(error)),
            };
            return self.token_result(to, result, span, allocator);
        }
        let target = self.referenced_cell(first)?;
        let mut token = self.token(target, allocator)?;
        let mut owner = v::Vector::from_token(self.services.layouts, ty, &mut token, allocator)
            .map_err(fault)?
            .with_service_work(self.services.work.clone());
        match operation {
            O::VecGet | O::VecGetMut => {
                let l::Immediate::I64(index) =
                    self.read(arguments.next().ok_or(Reason::Invariant)?, allocator)?
                else {
                    return Err(Reason::Invariant);
                };
                let id = result_loan.ok_or(Reason::Invariant)?;
                let origin = self.next_reference(id)?;
                let custody = self.custody(target, allocator)?;
                let kind = if operation == O::VecGetMut {
                    o::LoanKind::Exclusive
                } else {
                    o::LoanKind::Shared
                };
                match owner.borrow(index, kind, allocator) {
                    Ok(read) => {
                        let at = read
                            .view(self.services.layouts, allocator)
                            .map_err(fault)?
                            .root();
                        self.frame
                            .borrow_target(id, Target::Vector { origin, at }, to)?;
                        let loan = &mut self.frame.managed.as_mut().ok_or(Reason::Invariant)?.loans
                            [id.0 as usize];
                        loan.vector = Some(read);
                        loan.custody = custody;
                        self.loan_sum(to, 0, true, span, allocator)
                    }
                    Err(v::Error::Bounds(error)) => {
                        self.immediate_result(to, Err(l::Immediate::Bounds(error)), span, allocator)
                    }
                    Err(error) => Err(fault(error)),
                }
            }
            O::VecLen => {
                let length = owner.len(allocator).map_err(fault)?;
                self.write(to, l::Immediate::I64(length), allocator)
            }
            O::VecPush => {
                let source = arguments.next().ok_or(Reason::Invariant)?;
                self.available(source)?;
                let copy = self.is_copy(source)?;
                let input = self
                    .frame
                    .storage
                    .as_mut()
                    .ok_or(Reason::Invariant)?
                    .cell_mut(source)?;
                let result = allocation(if copy {
                    owner.push_copy(input, Some(span), allocator)
                } else {
                    owner.push_from(input, Some(span), allocator)
                })?;
                match result {
                    Ok(()) if !copy => {
                        input.recycle(allocator)?;
                        self.forget(source);
                    }
                    Err(_) if !copy => {
                        self.cleanup_slot(source, c::Reason::ScopeExit, allocator)?
                    }
                    _ => {}
                }
                self.immediate_result(
                    to,
                    result
                        .map(|_| l::Immediate::Unit)
                        .map_err(l::Immediate::Allocation),
                    span,
                    allocator,
                )
            }
            O::VecPop => {
                let empty = owner.is_empty(allocator).map_err(fault)?;
                let cell = self.data()?.cell(to)?;
                let root = cell.root();
                cell.prepare_case(root, u32::from(!empty), allocator)?;
                let target = if empty {
                    root
                } else {
                    cell.child(root, 0, allocator)?
                };
                let moved = owner.pop_into(cell, target, allocator).map_err(fault)?;
                cell.initialize_order(root, moved.then_some((0, span)).into_iter(), allocator)?;
                self.mark(to, span)
            }
            _ => Err(Reason::Unsupported),
        }
    }
}
