use super::*;

fn outcome<T>(value: Result<T, sh::Error>) -> Step<Result<T, l::OwnershipError>> {
    match value {
        Ok(v) => Ok(Ok(v)),
        Err(sh::Error {
            kind: sh::Kind::Ownership(e),
            ..
        }) => Ok(Err(e)),
        Err(sh::Error {
            kind: sh::Kind::Allocator(_) | sh::Kind::Buffer(_),
            ..
        }) => Err(c::ExecutionFailure::Fault),
        Err(_) => Err(c::ExecutionFailure::Invariant),
    }
}
pub(super) fn cleanup_services<'l, 't, P: a::Provider>(
    layouts: &'l l::Layouts<'t>,
    heap: &mut Option<sh::Heap<'l, 't>>,
    retirement: &mut Option<r::Drain<'l, 't>>,
    token: &mut l::Token,
    allocator: &mut a::Allocator<P>,
    context: &mut c::Context<'_>,
) -> Result<(), u32> {
    if let Some(drain) = retirement {
        drain.token(
            token,
            heap.as_mut(),
            allocator,
            context,
            |token, allocator, _, context| cleanup_leaf(layouts.types(), token, allocator, context),
        )
    } else {
        cleanup_leaf(layouts.types(), token, allocator, context)
    }
}
impl<P: a::Provider> State<'_, '_, '_, P> {
    pub fn shared_view(
        &self,
        id: usize,
        ty: o::Type,
    ) -> Step<lang_runtime::shared::storage::View<'_, '_, '_>> {
        let read = self.loans[id]
            .shared_read
            .ok_or(c::ExecutionFailure::Invariant)?;
        self.shared_heap
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .view(read, ty, self.allocator)
            .map_err(invariant)
    }
    fn shared_result(
        &mut self,
        destination: usize,
        result: Result<Option<[u8; 32]>, l::OwnershipError>,
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
        let payload = cell
            .child(cell.root(), 0, self.allocator)
            .map_err(invariant)?;
        match result {
            Ok(Some(token)) => {
                let start = payload.offset() as usize;
                cell.data_mut(self.allocator).map_err(invariant)?[start..start + 32]
                    .copy_from_slice(&token);
            }
            Ok(None) => cell
                .write_immediate(payload, l::Immediate::Unit, self.allocator)
                .map_err(invariant)?,
            Err(error) => cell
                .write_immediate(payload, l::Immediate::Ownership(error), self.allocator)
                .map_err(invariant)?,
        };
        cell.initialize_order(cell.root(), [(0, span)].into_iter(), self.allocator)
            .map_err(invariant)?;
        self.cells[destination].live = true;
        Ok(())
    }
    pub fn shared_store(
        &mut self,
        operation: o::core::Operation,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<ArenaTransfer> {
        let value = |r: m::Register| self.current.cell + values + r.0 as usize;
        let source = value(*call.arguments.last().unwrap());
        let copy = self.is_copy(source);
        let target = if operation == o::core::Operation::SharedReplace {
            Some(self.reference(self.root(value(call.arguments[0]))?)?)
        } else {
            self.require_allocator(value(call.arguments[0]))?;
            None
        };
        let observed = target.map(|target| self.observation(target)).transpose()?;
        let custody = target
            .map(|target| self.custody(target))
            .transpose()?
            .flatten();
        let heap = self
            .shared_heap
            .as_mut()
            .ok_or(c::ExecutionFailure::Invariant)?;
        let ready = if heap.is_open() {
            Ok(())
        } else {
            outcome(heap.open(self.allocator))?
        };
        let input = self
            .storage
            .as_mut()
            .unwrap()
            .cell_mut(self.cells[source].physical)
            .map_err(invariant)?;
        let result = match ready {
            Err(error) => Err(error),
            Ok(()) => {
                if target.is_some() {
                    if copy {
                        outcome(heap.replace_observed_copy(
                            observed.as_ref().unwrap(),
                            custody,
                            input,
                            span,
                            self.allocator,
                        ))?
                        .map(|_| (None, ArenaTransfer::Copy))
                    } else {
                        outcome(heap.replace_observed(
                            observed.as_ref().unwrap(),
                            custody,
                            input,
                            span,
                            self.allocator,
                        ))?
                        .map(|old| (None, ArenaTransfer::Replaced(old)))
                    }
                } else {
                    outcome(if copy {
                        heap.allocate_copy(input, Some(span), self.allocator)
                    } else {
                        heap.allocate_from(input, Some(span), self.allocator)
                    })?
                    .map(|owner| {
                        (
                            Some(owner.into_token()),
                            if copy {
                                ArenaTransfer::Copy
                            } else {
                                ArenaTransfer::Moved
                            },
                        )
                    })
                }
            }
        };
        let (result, transfer) = match result {
            Ok((token, transfer)) => (Ok(token), transfer),
            Err(e) => (Err(e), ArenaTransfer::Rejected),
        };
        if matches!(transfer, ArenaTransfer::Moved) {
            input.recycle(self.allocator).map_err(invariant)?;
            self.cells[source].live = false;
        }
        self.shared_result(destination, result, span)?;
        Ok(transfer)
    }
    pub fn shared_owner_call(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<()> {
        let argument = self.current.cell + values + call.arguments[0].0 as usize;
        let target = self.reference(self.root(argument)?)?;
        let action = match call.operation {
            o::core::Operation::SharedClone => sh::FieldAction::CloneStrong,
            o::core::Operation::WeakNew => sh::FieldAction::NewWeak,
            o::core::Operation::WeakClone => sh::FieldAction::CloneWeak,
            o::core::Operation::WeakUpgrade => sh::FieldAction::UpgradeWeak,
            _ => return Err(c::ExecutionFailure::Invariant),
        };
        let token = self.observation(target)?;
        let custody = self.custody(target)?;
        let result = outcome(self.shared_heap.as_mut().unwrap().retain_observed(
            &token,
            custody,
            action,
            self.allocator,
        ))?;
        self.shared_result(
            destination,
            result.map(|owner| {
                Some(match owner {
                    sh::Retained::Strong(v) => v.into_token(),
                    sh::Retained::Weak(v) => v.into_token(),
                })
            }),
            span,
        )
    }
    pub fn shared_read(
        &mut self,
        call: &o::core::Call,
        values: usize,
        destination: usize,
        span: m::Span,
    ) -> Step<bool> {
        let argument = self.current.cell + values + call.arguments[0].0 as usize;
        let target = self.reference(self.root(argument)?)?;
        let ty = call.type_arguments[0].ty;
        let id =
            self.current.loan + call.result_loan.ok_or(c::ExecutionFailure::Invariant)?.0 as usize;
        if self.loans[id].target.is_some()
            || self.loans[id].shared_read.is_some()
            || self.loans[id].arena_read.is_some()
        {
            return Err(c::ExecutionFailure::Invariant);
        }
        self.loans[id]
            .generation
            .checked_add(1)
            .ok_or(c::ExecutionFailure::CounterOverflow)?;
        let token = self.observation(target)?;
        let custody = self.custody(target)?;
        let result = outcome(self.shared_heap.as_mut().unwrap().read_observed(
            &token,
            custody,
            self.allocator,
        ))?;
        let success = result.is_ok();
        let result = match result {
            Ok(read) => {
                let location = self
                    .shared_heap
                    .as_ref()
                    .unwrap()
                    .view(read, ty, self.allocator)
                    .map_err(invariant)?
                    .root();
                let token = self.loan_token(
                    id,
                    Target::Shared {
                        read: id,
                        ty,
                        location,
                    },
                )?;
                self.loans[id].shared_read = Some(read);
                Ok(Some(token))
            }
            Err(e) => Err(e),
        };
        self.shared_result(destination, result, span)?;
        Ok(success)
    }
}
