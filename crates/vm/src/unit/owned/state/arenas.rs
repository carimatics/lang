use super::*;

fn inspect<P: a::Provider>(
    token: &l::Token,
    _: &g::Arena<'_, '_>,
    _: &a::Allocator<P>,
    _: &mut g::PublicationBudget,
) -> Result<(), g::Error> {
    Err(g::Error {
        kind: g::Kind::Unsupported(token.leaf.ty),
        span: None,
    })
}
fn outcome<T>(value: Result<T, g::Error>) -> Step<Result<T, l::ArenaError>> {
    match value {
        Ok(value) => Ok(Ok(value)),
        Err(g::Error {
            kind: g::Kind::Arena(error),
            ..
        }) => Ok(Err(error)),
        Err(g::Error {
            kind: g::Kind::Allocator(_) | g::Kind::Buffer(_),
            ..
        }) => Err(c::ExecutionFailure::Fault),
        Err(_) => Err(c::ExecutionFailure::Invariant),
    }
}
impl<P: a::Provider> State<'_, '_, '_, P> {
    pub fn arena_enter(&mut self, local: usize, reference: usize) -> Step<()> {
        self.require_allocator(reference)?;
        let physical = self.cells[local].physical;
        self.arenas[physical]
            .as_mut()
            .ok_or(c::ExecutionFailure::Invariant)?
            .open(self.allocator)
            .map_err(|_| c::ExecutionFailure::AllocationLimit)?;
        self.cells[local].live = true;
        Ok(())
    }
    fn arena_owner(&self, argument: usize) -> Step<usize> {
        let Target::Data { cell, location } = self.reference(self.root(argument)?)? else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let owner_type = matches!(location.ty(), o::Type::Constructed(id)
            if matches!(self.types.table().constructed[id.0 as usize].constructor, o::Constructor::ArenaOwner(_)));
        if !owner_type
            || location.offset() != 0
            || self.arenas.get(cell).and_then(Option::as_ref).is_none()
        {
            return Err(c::ExecutionFailure::Invariant);
        }
        Ok(cell)
    }
    pub fn arena_store(
        &mut self,
        operation: o::core::Operation,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<ArenaTransfer> {
        let value = |r: m::Register| self.current.cell + values + r.0 as usize;
        let owner = self.arena_owner(value(call.arguments[0]))?;
        let source = value(*call.arguments.last().unwrap());
        let copy = self.is_copy(source);
        let reference = if operation == o::core::Operation::ArenaReplace {
            Some(
                g::Reference::from_token(self.token(self.root(value(call.arguments[1]))?)?)
                    .map_err(invariant)?,
            )
        } else {
            None
        };
        let input = self
            .storage
            .as_mut()
            .unwrap()
            .cell_mut(self.cells[source].physical)
            .map_err(invariant)?;
        let arena = self.arenas[owner]
            .as_mut()
            .ok_or(c::ExecutionFailure::Invariant)?;
        let result = if let Some(reference) = reference {
            if copy {
                outcome(arena.replace_copy(reference, input, span, self.allocator, inspect))?
                    .map(|_| (None, ArenaTransfer::Copy))
            } else {
                outcome(arena.replace_owned(reference, input, span, self.allocator, inspect))?
                    .map(|span| (None, ArenaTransfer::Replaced(span)))
            }
        } else {
            outcome(if copy {
                arena.allocate_copy(input, Some(span), self.allocator, inspect)
            } else {
                arena.allocate_from(input, Some(span), self.allocator, inspect)
            })?
            .map(|value| {
                (
                    Some(value),
                    if copy {
                        ArenaTransfer::Copy
                    } else {
                        ArenaTransfer::Moved
                    },
                )
            })
        };
        let (result, transfer) = match result {
            Ok((value, transfer)) => (Ok(value), transfer),
            Err(error) => (Err(error), ArenaTransfer::Rejected),
        };
        if matches!(transfer, ArenaTransfer::Moved) {
            input.recycle(self.allocator).map_err(invariant)?;
            self.cells[source].live = false;
        }
        let target = self
            .storage
            .as_ref()
            .unwrap()
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        let (case, result) = match result {
            Ok(value) => (0, Ok(value)),
            Err(error) => (1, Err(error)),
        };
        target
            .prepare_case(target.root(), case, self.allocator)
            .map_err(invariant)?;
        let payload = target
            .child(target.root(), 0, self.allocator)
            .map_err(invariant)?;
        match result {
            Ok(Some(reference)) => {
                let start = payload.offset() as usize;
                target.data_mut(self.allocator).map_err(invariant)?[start..start + 32]
                    .copy_from_slice(&reference.token());
            }
            Ok(None) => target
                .write_immediate(payload, l::Immediate::Unit, self.allocator)
                .map_err(invariant)?,
            Err(error) => target
                .write_immediate(payload, l::Immediate::Arena(error), self.allocator)
                .map_err(invariant)?,
        }
        target
            .initialize_order(target.root(), [(0, span)].into_iter(), self.allocator)
            .map_err(invariant)?;
        self.cells[destination].live = true;
        Ok(transfer)
    }
    pub fn arena_view(&self, owner: usize, id: usize) -> Step<g::View<'_, '_, '_>> {
        let (expected, read) = self.loans[id]
            .arena_read
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?;
        if *expected != owner {
            return Err(c::ExecutionFailure::Invariant);
        }
        self.arenas[owner]
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .payload(read, self.allocator)
            .map_err(invariant)
    }
    pub fn arena_read(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<bool> {
        let value = |r: m::Register| self.current.cell + values + r.0 as usize;
        let owner = self.arena_owner(value(call.arguments[0]))?;
        let reference = g::Reference::from_token(self.token(self.root(value(call.arguments[1]))?)?)
            .map_err(invariant)?;
        let id =
            self.current.loan + call.result_loan.ok_or(c::ExecutionFailure::Invariant)?.0 as usize;
        if self.loans[id].target.is_some() || self.loans[id].arena_read.is_some() {
            return Err(c::ExecutionFailure::Invariant);
        }
        self.loans[id]
            .generation
            .checked_add(1)
            .ok_or(c::ExecutionFailure::CounterOverflow)?;
        let outcome = outcome(self.arenas[owner].as_mut().unwrap().read(
            reference,
            call.type_arguments[0].ty,
            self.allocator,
        ))?;
        let (case, token, error) = match outcome {
            Ok(read) => {
                let location = self.arenas[owner]
                    .as_ref()
                    .unwrap()
                    .payload(&read, self.allocator)
                    .map_err(invariant)?
                    .root();
                let token = self.loan_token(
                    id,
                    Target::Arena {
                        owner,
                        read: id,
                        location,
                    },
                )?;
                self.loans[id].arena_read = Some((owner, read));
                (0, Some(token), None)
            }
            Err(error) => (1, None, Some(error)),
        };
        let target = self
            .storage
            .as_ref()
            .unwrap()
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        target
            .prepare_case(target.root(), case, self.allocator)
            .map_err(invariant)?;
        let payload = target
            .child(target.root(), 0, self.allocator)
            .map_err(invariant)?;
        if let Some(token) = token {
            let start = payload.offset() as usize;
            target.data_mut(self.allocator).map_err(invariant)?[start..start + 32]
                .copy_from_slice(&token);
        } else {
            target
                .write_immediate(payload, l::Immediate::Arena(error.unwrap()), self.allocator)
                .map_err(invariant)?;
        }
        target
            .initialize_order(target.root(), [(0, span)].into_iter(), self.allocator)
            .map_err(invariant)?;
        self.cells[destination].live = true;
        Ok(token.is_some())
    }
}
