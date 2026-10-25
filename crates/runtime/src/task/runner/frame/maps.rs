//! Map publication and detached-key retirement share the frame cleanup session.
use super::*;
use crate::{buffer, map as mm};
fn fault(error: mm::Error) -> Reason {
    if let Some(failure) = crate::execution::service_failure(&error) {
        return Reason::Execution(failure.into());
    }
    match error {
        mm::Error::LoanLimit
        | mm::Error::Allocation(a::Error::Limit(_) | a::Error::OutOfMemory) => {
            Reason::Execution(f::ExecutionFailure::AllocationLimit)
        }
        mm::Error::Allocation(a::Error::Overflow) => {
            Reason::Execution(f::ExecutionFailure::CounterOverflow)
        }
        mm::Error::WorkLimit | mm::Error::BorrowConflict => {
            Reason::Execution(f::ExecutionFailure::Fault)
        }
        _ => Reason::Invariant,
    }
}
fn allocation<T>(value: Result<T, mm::Error>) -> Result<Result<T, buffer::AllocError>, Reason> {
    match value {
        Ok(v) => Ok(Ok(v)),
        Err(mm::Error::Allocation(a::Error::Limit(_))) => Ok(Err(buffer::AllocError::Limit)),
        Err(mm::Error::Allocation(a::Error::OutOfMemory)) => {
            Ok(Err(buffer::AllocError::OutOfMemory))
        }
        Err(mm::Error::Allocation(a::Error::Overflow)) => Ok(Err(buffer::AllocError::Overflow)),
        Err(e) => Err(fault(e)),
    }
}
impl Access<'_, '_, '_, '_> {
    fn map_key<P: a::Provider>(
        &self,
        slot: usize,
        ty: o::Type,
        allocator: &a::Allocator<P>,
    ) -> Result<mm::Key, Reason> {
        let target = self.reference(slot)?;
        if ty == o::Type::I64 {
            let l::Immediate::I64(value) = self.target_read(target, allocator)? else {
                return Err(Reason::Invariant);
            };
            Ok(mm::Key::I64(value))
        } else {
            Ok(mm::Key::Text(self.token(target, allocator)?))
        }
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep verified operands, publication context and allocator explicit at the shared backend boundary."
    )]
    pub(super) fn map_core<P: a::Provider>(
        &mut self,
        operation: o::core::Operation,
        key_ty: o::Type,
        value_ty: o::Type,
        result_loan: Option<b::LoanId>,
        first: usize,
        mut arguments: impl ExactSizeIterator<Item = usize>,
        to: usize,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        use o::core::Operation as O;
        if operation == O::MapNew {
            if !matches!(self.reference(first)?, Target::Allocator) {
                return Err(Reason::Invariant);
            }
            let result = match allocation(mm::Map::new(
                self.services.layouts,
                key_ty,
                value_ty,
                allocator,
            ))? {
                Ok(mut map) => Ok(map.take_token(allocator).map_err(fault)?),
                Err(e) => Err(l::Immediate::Allocation(e)),
            };
            return self.token_result(to, result, span, allocator);
        }
        let target = self.reference(first)?;
        let mut token = self.token(target, allocator)?;
        let mut owner = mm::Map::from_token(
            self.services.layouts,
            key_ty,
            value_ty,
            &mut token,
            allocator,
        )
        .map_err(fault)?
        .with_service_work(self.services.work.clone());
        if operation == O::MapLen {
            let length = owner.len(allocator).map_err(fault)?;
            return self.write(to, l::Immediate::I64(length), allocator);
        }
        let key_slot = arguments.next().ok_or(Reason::Invariant)?;
        if operation == O::MapGet {
            let key = self.map_key(key_slot, key_ty, allocator)?;
            let id = result_loan.ok_or(Reason::Invariant)?;
            let origin = self.next_reference(id)?;
            let custody = self.custody(target, allocator)?;
            let present = match owner.borrow(key, allocator).map_err(fault)? {
                Some(read) => {
                    let at = read
                        .view(self.services.layouts, allocator)
                        .map_err(fault)?
                        .root();
                    self.frame
                        .borrow_target(id, Target::Map { origin, at }, to)?;
                    let loan = &mut self.frame.managed.as_mut().ok_or(Reason::Invariant)?.loans
                        [id.0 as usize];
                    loan.map = Some(read);
                    loan.custody = custody;
                    true
                }
                None => false,
            };
            return self.loan_sum(to, u32::from(present), present, span, allocator);
        }
        if operation == O::MapRemove {
            let key = self.map_key(key_slot, key_ty, allocator)?;
            let output = self.data()?.cell(to)?;
            let root = output.root();
            output.prepare_case(root, 1, allocator)?;
            let at = output.child(root, 0, allocator)?;
            let old = owner
                .remove_into(key, output, at, allocator)
                .map_err(fault)?;
            let detached = match old {
                Some(old) => {
                    output.initialize_order(
                        root,
                        [(0, old.value_span.unwrap_or(span))].into_iter(),
                        allocator,
                    )?;
                    old.key
                }
                None => {
                    output.prepare_case(root, 0, allocator)?;
                    output.initialize_order(root, std::iter::empty(), allocator)?;
                    None
                }
            };
            self.mark(to, span)?;
            if let Some(token) = detached {
                self.retire_token(token, to, span, allocator)?;
            }
            return Ok(());
        }
        if operation != O::MapInsert {
            return Err(Reason::Unsupported);
        }
        let value_slot = arguments.next().ok_or(Reason::Invariant)?;
        self.available(key_slot)?;
        self.available(value_slot)?;
        let key_copy = self.is_copy(key_slot)?;
        let value_copy = self.is_copy(value_slot)?;
        let output = self.data()?.cell(to)?;
        let root = output.root();
        output.prepare_case(root, 0, allocator)?;
        let option = output.child(root, 0, allocator)?;
        output.prepare_case(option, 1, allocator)?;
        let at = output.child(option, 0, allocator)?;
        let spans = mm::Spans {
            key: Some(span),
            value: Some(span),
        };
        let storage = self.frame.storage.as_mut().ok_or(Reason::Invariant)?;
        let result = if key_slot == value_slot {
            let (input, output) = storage.two_cells_mut(key_slot, to)?;
            allocation(owner.insert_repeated_integer_into(input, spans, output, at, allocator))?
        } else {
            let [key, value, output] = storage.three_cells_mut([key_slot, value_slot, to])?;
            allocation(owner.insert_into(key, value, spans, output, at, allocator))?
        };
        let published = result.is_ok();
        if published {
            for (slot, copy) in [(key_slot, key_copy), (value_slot, value_copy)] {
                if !copy {
                    self.frame
                        .storage
                        .as_mut()
                        .ok_or(Reason::Invariant)?
                        .cell_mut(slot)?
                        .recycle(allocator)?;
                    self.forget(slot);
                }
            }
        }
        let output = self.data()?.cell(to)?;
        let detached = match result {
            Ok(Some(old)) => {
                output.initialize_order(
                    option,
                    [(0, old.value_span.unwrap_or(span))].into_iter(),
                    allocator,
                )?;
                old.key
            }
            Ok(None) => {
                output.prepare_case(option, 0, allocator)?;
                output.initialize_order(option, std::iter::empty(), allocator)?;
                None
            }
            Err(error) => {
                output.prepare_case(root, 1, allocator)?;
                output.write_immediate(
                    output.child(root, 0, allocator)?,
                    l::Immediate::Allocation(error),
                    allocator,
                )?;
                None
            }
        };
        output.initialize_order(root, [(0, span)].into_iter(), allocator)?;
        self.mark(to, span)?;
        if !published {
            self.cleanup_selected([key_slot, value_slot], c::Reason::ScopeExit, allocator)?;
        }
        if let Some(token) = detached {
            self.retire_token(token, to, span, allocator)?;
        }
        Ok(())
    }
}
