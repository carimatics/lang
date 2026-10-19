use super::*;

fn fault(error: v::Error) -> c::ExecutionFailure {
    if let Some(failure) = lang_runtime::execution::service_failure(&error) {
        return failure.into();
    }
    match error {
        v::Error::LoanLimit | v::Error::Allocation(a::Error::Limit(_) | a::Error::OutOfMemory) => {
            c::ExecutionFailure::AllocationLimit
        }
        v::Error::Allocation(a::Error::Overflow) => c::ExecutionFailure::CounterOverflow,
        v::Error::BorrowConflict => c::ExecutionFailure::Fault,
        _ => c::ExecutionFailure::Invariant,
    }
}
fn allocation<T>(value: Result<T, v::Error>) -> Step<Result<T, buffer::AllocError>> {
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
impl<'l, 't, P: a::Provider> State<'_, 'l, 't, P> {
    pub fn vector_view(&self, id: usize) -> Step<v::Borrowed<'_, 'l, 't>> {
        self.loans[id]
            .vector_read
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .view(self.layouts, self.allocator)
            .map_err(fault)
    }
    fn vector_owner(&self, argument: usize, ty: o::Type) -> Step<v::Vector<'l, 't>> {
        let target = self.reference(self.root(argument)?)?;
        let mut token = self.token(target)?;
        v::Vector::from_token(self.layouts, ty, &mut token, self.allocator)
            .map_err(fault)
            .map(|value| value.with_service_work(self.service_work.clone()))
    }
    fn vector_result(
        &mut self,
        destination: usize,
        result: Result<Option<[u8; 32]>, l::Immediate>,
        span: m::Span,
    ) -> Step<()> {
        let cell = self
            .storage
            .as_ref()
            .unwrap()
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        cell.prepare_case(cell.root(), u32::from(result.is_err()), self.allocator)
            .map_err(invariant)?;
        let at = cell
            .child(cell.root(), 0, self.allocator)
            .map_err(invariant)?;
        match result {
            Ok(Some(token)) => {
                let start = at.offset() as usize;
                cell.data_mut(self.allocator).map_err(invariant)?[start..start + 32]
                    .copy_from_slice(&token);
            }
            Ok(None) => cell
                .write_immediate(at, l::Immediate::Unit, self.allocator)
                .map_err(invariant)?,
            Err(e) => cell
                .write_immediate(at, e, self.allocator)
                .map_err(invariant)?,
        };
        cell.initialize_order(cell.root(), [(0, span)].into_iter(), self.allocator)
            .map_err(invariant)?;
        self.cells[destination].live = true;
        Ok(())
    }
    pub fn vector_new(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<()> {
        let arg = self.current.cell + values + call.arguments[0].0 as usize;
        self.require_allocator(arg)?;
        let result = allocation(v::Vector::new(
            self.layouts,
            call.type_arguments[0].ty,
            self.allocator,
        ))?;
        let result = match result {
            Ok(mut v) => Ok(Some(v.take_token(self.allocator).map_err(fault)?)),
            Err(e) => Err(l::Immediate::Allocation(e)),
        };
        self.vector_result(destination, result, span)
    }
    pub fn vector_len(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
    ) -> Step<()> {
        let owner = self.vector_owner(
            self.current.cell + values + call.arguments[0].0 as usize,
            call.type_arguments[0].ty,
        )?;
        let len = owner.len(self.allocator).map_err(fault)?;
        self.write(destination, l::Immediate::I64(len))
    }
    pub fn vector_push(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<ArenaTransfer> {
        let arg = |r: m::Register| self.current.cell + values + r.0 as usize;
        let mut owner = self.vector_owner(arg(call.arguments[0]), call.type_arguments[0].ty)?;
        let source = arg(call.arguments[1]);
        let copy = self.is_copy(source);
        let input = self
            .storage
            .as_mut()
            .unwrap()
            .cell_mut(self.cells[source].physical)
            .map_err(invariant)?;
        let result = allocation(if copy {
            owner.push_copy(input, Some(span), self.allocator)
        } else {
            owner.push_from(input, Some(span), self.allocator)
        })?;
        let transfer = match result {
            Ok(()) if copy => ArenaTransfer::Copy,
            Ok(()) => {
                input.recycle(self.allocator).map_err(invariant)?;
                self.cells[source].live = false;
                ArenaTransfer::Moved
            }
            Err(_) => ArenaTransfer::Rejected,
        };
        self.immediate_result(
            destination,
            result
                .map(|_| l::Immediate::Unit)
                .map_err(l::Immediate::Allocation),
            span,
        )?;
        Ok(transfer)
    }
    pub fn vector_pop(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<()> {
        let mut owner = self.vector_owner(
            self.current.cell + values + call.arguments[0].0 as usize,
            call.type_arguments[0].ty,
        )?;
        let empty = owner.is_empty(self.allocator).map_err(fault)?;
        let cell = self
            .storage
            .as_ref()
            .unwrap()
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        cell.prepare_case(cell.root(), u32::from(!empty), self.allocator)
            .map_err(invariant)?;
        let target = if empty {
            cell.root()
        } else {
            cell.child(cell.root(), 0, self.allocator)
                .map_err(invariant)?
        };
        let moved = owner
            .pop_into(cell, target, self.allocator)
            .map_err(fault)?;
        if moved {
            cell.initialize_order(cell.root(), [(0, span)].into_iter(), self.allocator)
                .map_err(invariant)?;
        } else {
            cell.initialize_order(cell.root(), std::iter::empty(), self.allocator)
                .map_err(invariant)?;
        }
        self.cells[destination].live = true;
        Ok(())
    }
    pub fn vector_get(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<bool> {
        let arg = |r: m::Register| self.current.cell + values + r.0 as usize;
        let mut owner = self.vector_owner(arg(call.arguments[0]), call.type_arguments[0].ty)?;
        let l::Immediate::I64(index) = self.read(self.root(arg(call.arguments[1]))?)? else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let custody = self.custody(self.reference(self.root(arg(call.arguments[0]))?)?)?;
        let id =
            self.current.loan + call.result_loan.ok_or(c::ExecutionFailure::Invariant)?.0 as usize;
        if self.loans[id].target.is_some()
            || self.loans[id].vector_read.is_some()
            || self.loans[id].arena_read.is_some()
            || self.loans[id].shared_read.is_some()
        {
            return Err(c::ExecutionFailure::Invariant);
        }
        self.loans[id]
            .generation
            .checked_add(1)
            .ok_or(c::ExecutionFailure::CounterOverflow)?;
        let kind = if call.operation == o::core::Operation::VecGetMut {
            o::LoanKind::Exclusive
        } else {
            o::LoanKind::Shared
        };
        let (result, success) = match owner.borrow(index, kind, self.allocator) {
            Ok(read) => {
                let location = read
                    .view(self.layouts, self.allocator)
                    .map_err(fault)?
                    .root();
                let token = self.loan_token(id, Target::Vector { read: id, location })?;
                self.loans[id].vector_read = Some(read);
                self.loans[id].custody = custody;
                (Ok(Some(token)), true)
            }
            Err(v::Error::Bounds(e)) => (Err(l::Immediate::Bounds(e)), false),
            Err(e) => return Err(fault(e)),
        };
        self.vector_result(destination, result, span)?;
        Ok(success)
    }
    pub fn vector_replace(
        &mut self,
        target: Target,
        source: usize,
        span: m::Span,
    ) -> Step<Option<m::Span>> {
        let Target::Vector { read, location } = target else {
            return Err(c::ExecutionFailure::Invariant);
        };
        self.root(source)?;
        let copy = self.is_copy(source);
        let loan = self.loans[read]
            .vector_read
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?;
        let input = self
            .storage
            .as_mut()
            .unwrap()
            .cell_mut(self.cells[source].physical)
            .map_err(invariant)?;
        if copy {
            loan.replace_copy(
                self.layouts,
                location,
                input,
                input.root(),
                span,
                self.allocator,
            )
            .map_err(fault)?;
            Ok(None)
        } else {
            loan.replace_from(self.layouts, location, input, span, self.allocator)
                .map_err(fault)
        }
    }
}
