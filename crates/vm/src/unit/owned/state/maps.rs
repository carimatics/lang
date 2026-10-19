use super::*;

fn fault(error: mm::Error) -> c::ExecutionFailure {
    if let Some(failure) = lang_runtime::execution::service_failure(&error) {
        return failure.into();
    }
    match error {
        mm::Error::LoanLimit
        | mm::Error::Allocation(a::Error::Limit(_) | a::Error::OutOfMemory) => {
            c::ExecutionFailure::AllocationLimit
        }
        mm::Error::Allocation(a::Error::Overflow) => c::ExecutionFailure::CounterOverflow,
        mm::Error::WorkLimit | mm::Error::BorrowConflict => c::ExecutionFailure::Fault,
        _ => c::ExecutionFailure::Invariant,
    }
}
fn allocation<T>(result: Result<T, mm::Error>) -> Step<Result<T, buffer::AllocError>> {
    match result {
        Ok(v) => Ok(Ok(v)),
        Err(mm::Error::Allocation(a::Error::Limit(_))) => Ok(Err(buffer::AllocError::Limit)),
        Err(mm::Error::Allocation(a::Error::OutOfMemory)) => {
            Ok(Err(buffer::AllocError::OutOfMemory))
        }
        Err(mm::Error::Allocation(a::Error::Overflow)) => Ok(Err(buffer::AllocError::Overflow)),
        Err(e) => Err(fault(e)),
    }
}
impl<'l, 't, P: a::Provider> State<'_, 'l, 't, P> {
    pub fn map_view(&self, id: usize) -> Step<mm::Borrowed<'_, 'l, 't>> {
        self.loans[id]
            .map_read
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .view(self.layouts, self.allocator)
            .map_err(fault)
    }
    fn map_owner(&self, call: &o::core::Call, values: usize) -> Step<mm::Map<'l, 't>> {
        let argument = self.current.cell + values + call.arguments[0].0 as usize;
        let mut token = self.token(self.reference(self.root(argument)?)?)?;
        mm::Map::from_token(
            self.layouts,
            call.type_arguments[0].ty,
            call.type_arguments[1].ty,
            &mut token,
            self.allocator,
        )
        .map_err(fault)
        .map(|value| value.with_service_work(self.service_work.clone()))
    }
    fn map_key(&self, argument: usize, ty: o::Type) -> Step<mm::Key> {
        let target = self.reference(self.root(argument)?)?;
        if ty == o::Type::I64 {
            let l::Immediate::I64(n) = self.read(target)? else {
                return Err(c::ExecutionFailure::Invariant);
            };
            Ok(mm::Key::I64(n))
        } else {
            Ok(mm::Key::Text(self.token(target)?))
        }
    }
    pub fn map_new(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<()> {
        self.require_allocator(self.current.cell + values + call.arguments[0].0 as usize)?;
        let result = allocation(mm::Map::new(
            self.layouts,
            call.type_arguments[0].ty,
            call.type_arguments[1].ty,
            self.allocator,
        ))?;
        let result = match result {
            Ok(mut map) => Ok(map.take_token(self.allocator).map_err(fault)?),
            Err(e) => Err(e),
        };
        let cell = self
            .storage
            .as_ref()
            .unwrap()
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        cell.prepare_case(cell.root(), u32::from(result.is_err()), self.allocator)
            .map_err(invariant)?;
        let target = cell
            .child(cell.root(), 0, self.allocator)
            .map_err(invariant)?;
        match result {
            Ok(token) => {
                let offset = target.offset() as usize;
                cell.data_mut(self.allocator).map_err(invariant)?[offset..offset + 32]
                    .copy_from_slice(&token);
            }
            Err(e) => cell
                .write_immediate(target, l::Immediate::Allocation(e), self.allocator)
                .map_err(invariant)?,
        };
        cell.initialize_order(cell.root(), [(0, span)].into_iter(), self.allocator)
            .map_err(invariant)?;
        self.cells[destination].live = true;
        Ok(())
    }
    pub fn map_len(&mut self, call: &o::core::Call, values: usize, destination: usize) -> Step<()> {
        let owner = self.map_owner(call, values)?;
        let len = owner.len(self.allocator).map_err(fault)?;
        self.write(destination, l::Immediate::I64(len))
    }
    pub fn map_get(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<bool> {
        let mut owner = self.map_owner(call, values)?;
        let key = self.map_key(
            self.current.cell + values + call.arguments[1].0 as usize,
            call.type_arguments[0].ty,
        )?;
        let id =
            self.current.loan + call.result_loan.ok_or(c::ExecutionFailure::Invariant)?.0 as usize;
        let loan = &self.loans[id];
        if loan.target.is_some()
            || loan.map_read.is_some()
            || loan.vector_read.is_some()
            || loan.shared_read.is_some()
            || loan.arena_read.is_some()
        {
            return Err(c::ExecutionFailure::Invariant);
        }
        loan.generation
            .checked_add(1)
            .ok_or(c::ExecutionFailure::CounterOverflow)?;
        let argument = self.current.cell + values + call.arguments[0].0 as usize;
        let custody = self.custody(self.reference(self.root(argument)?)?)?;
        let token = match owner.borrow(key, self.allocator).map_err(fault)? {
            Some(read) => {
                let location = read
                    .view(self.layouts, self.allocator)
                    .map_err(fault)?
                    .root();
                let token = self.loan_token(id, Target::Map { read: id, location })?;
                self.loans[id].map_read = Some(read);
                self.loans[id].custody = custody;
                Some(token)
            }
            None => None,
        };
        let cell = self
            .storage
            .as_ref()
            .unwrap()
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        cell.prepare_case(cell.root(), u32::from(token.is_some()), self.allocator)
            .map_err(invariant)?;
        if let Some(token) = token {
            let at = cell
                .child(cell.root(), 0, self.allocator)
                .map_err(invariant)?
                .offset() as usize;
            cell.data_mut(self.allocator).map_err(invariant)?[at..at + 32].copy_from_slice(&token);
            cell.initialize_order(cell.root(), [(0, span)].into_iter(), self.allocator)
                .map_err(invariant)?;
        } else {
            cell.initialize_order(cell.root(), std::iter::empty(), self.allocator)
                .map_err(invariant)?;
        }
        self.cells[destination].live = true;
        Ok(token.is_some())
    }
    pub fn map_insert(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<MapTransfer> {
        let arg = |r: m::Register| self.current.cell + values + r.0 as usize;
        let k = arg(call.arguments[1]);
        let v = arg(call.arguments[2]);
        self.root(k)?;
        self.root(v)?;
        let kc = self.is_copy(k);
        let vc = self.is_copy(v);
        let mut owner = self.map_owner(call, values)?;
        let ids = [
            self.cells[k].physical,
            self.cells[v].physical,
            self.cells[destination].physical,
        ];
        let output = self
            .storage
            .as_ref()
            .unwrap()
            .cell(ids[2])
            .map_err(invariant)?;
        let root = output.root();
        output
            .prepare_case(root, 0, self.allocator)
            .map_err(invariant)?;
        let option = output.child(root, 0, self.allocator).map_err(invariant)?;
        output
            .prepare_case(option, 1, self.allocator)
            .map_err(invariant)?;
        let target = output.child(option, 0, self.allocator).map_err(invariant)?;
        let spans = mm::Spans {
            key: Some(span),
            value: Some(span),
        };
        let result = if ids[0] == ids[1] {
            let (input, output) = self
                .storage
                .as_mut()
                .unwrap()
                .two_cells_mut(ids[0], ids[2])
                .map_err(invariant)?;
            allocation(owner.insert_repeated_integer_into(
                input,
                spans,
                output,
                target,
                self.allocator,
            ))?
        } else {
            let [key, value, output] = self
                .storage
                .as_mut()
                .unwrap()
                .three_cells_mut(ids)
                .map_err(invariant)?;
            allocation(owner.insert_into(key, value, spans, output, target, self.allocator))?
        };
        let published = result.is_ok();
        if published {
            if !kc {
                self.storage
                    .as_mut()
                    .unwrap()
                    .cell_mut(ids[0])
                    .map_err(invariant)?
                    .recycle(self.allocator)
                    .map_err(invariant)?;
                self.cells[k].live = false;
            }
            if !vc {
                self.storage
                    .as_mut()
                    .unwrap()
                    .cell_mut(ids[1])
                    .map_err(invariant)?
                    .recycle(self.allocator)
                    .map_err(invariant)?;
                self.cells[v].live = false;
            }
        }
        let output = self
            .storage
            .as_ref()
            .unwrap()
            .cell(ids[2])
            .map_err(invariant)?;
        let mut detached = None;
        match result {
            Ok(old) => match old {
                Some(old) => {
                    output
                        .initialize_order(
                            option,
                            [(0, old.value_span.unwrap_or(span))].into_iter(),
                            self.allocator,
                        )
                        .map_err(invariant)?;
                    detached = old.key;
                }
                None => {
                    output
                        .prepare_case(option, 0, self.allocator)
                        .map_err(invariant)?;
                    output
                        .initialize_order(option, std::iter::empty(), self.allocator)
                        .map_err(invariant)?;
                }
            },
            Err(e) => {
                output
                    .prepare_case(root, 1, self.allocator)
                    .map_err(invariant)?;
                let target = output.child(root, 0, self.allocator).map_err(invariant)?;
                output
                    .write_immediate(target, l::Immediate::Allocation(e), self.allocator)
                    .map_err(invariant)?;
            }
        }
        output
            .initialize_order(root, [(0, span)].into_iter(), self.allocator)
            .map_err(invariant)?;
        self.cells[destination].live = true;
        Ok(MapTransfer {
            published,
            key: detached,
        })
    }
    pub fn map_remove(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<Option<l::Token>> {
        let mut owner = self.map_owner(call, values)?;
        let key = self.map_key(
            self.current.cell + values + call.arguments[1].0 as usize,
            call.type_arguments[0].ty,
        )?;
        let output = self
            .storage
            .as_ref()
            .unwrap()
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        let root = output.root();
        output
            .prepare_case(root, 1, self.allocator)
            .map_err(invariant)?;
        let target = output.child(root, 0, self.allocator).map_err(invariant)?;
        let old = owner
            .remove_into(key, output, target, self.allocator)
            .map_err(fault)?;
        let key = match old {
            Some(old) => {
                output
                    .initialize_order(
                        root,
                        [(0, old.value_span.unwrap_or(span))].into_iter(),
                        self.allocator,
                    )
                    .map_err(invariant)?;
                old.key
            }
            None => {
                output
                    .prepare_case(root, 0, self.allocator)
                    .map_err(invariant)?;
                output
                    .initialize_order(root, std::iter::empty(), self.allocator)
                    .map_err(invariant)?;
                None
            }
        };
        self.cells[destination].live = true;
        Ok(key)
    }
}
