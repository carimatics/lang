//! Activation-owned arena scopes use the established region/publication runtime.
use super::*;
use crate::arena::managed as g;
pub(super) fn preparation(error: g::Error) -> Reason {
    if let Some(failure) = crate::execution::service_failure(&error) {
        return Reason::Control(failure);
    }
    match error.kind {
        g::Kind::Layout(e) => Reason::Layout(e),
        _ => Reason::Invariant,
    }
}
fn outcome<T>(value: Result<T, g::Error>) -> Result<Result<T, l::ArenaError>, Reason> {
    if let Err(error) = &value
        && let Some(failure) = crate::execution::service_failure(error)
    {
        return Err(Reason::Execution(failure.into()));
    }
    match value {
        Ok(value) => Ok(Ok(value)),
        Err(g::Error {
            kind: g::Kind::Arena(error),
            ..
        }) => Ok(Err(error)),
        Err(g::Error {
            kind: g::Kind::Allocator(_) | g::Kind::Buffer(_),
            ..
        }) => Err(Reason::Execution(f::ExecutionFailure::Fault)),
        Err(_) => Err(Reason::Invariant),
    }
}
impl<'a, 'l, 'ty> Access<'a, 'l, 'ty, '_> {
    pub fn arena_enter<P: a::Provider>(
        &mut self,
        owner: usize,
        reference: usize,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        if !matches!(self.reference(reference)?, Target::Allocator) {
            return Err(Reason::Invariant);
        }
        self.frame
            .arenas
            .get_mut(owner)
            .and_then(Option::as_mut)
            .ok_or(Reason::Invariant)?
            .open(allocator)
            .map_err(|_| Reason::Execution(f::ExecutionFailure::AllocationLimit))?;
        self.mark(owner, span)
    }
    fn arena_owner(&self, argument: usize) -> Result<usize, Reason> {
        let Target::Cell(location) = self.reference(argument)? else {
            return Err(Reason::Invariant);
        };
        if location.activation != self.activation
            || location.at != self.root(location.slot)?.at
            || !self.is_arena_owner(location.slot)?
            || self
                .arenas
                .get(location.slot)
                .and_then(Option::as_ref)
                .is_none()
        {
            return Err(Reason::Invariant);
        }
        Ok(location.slot)
    }
    pub(super) fn arena_view<P: a::Provider>(
        &self,
        origin: super::managed::Reference,
        allocator: &a::Allocator<P>,
    ) -> Result<g::View<'_, 'l, 'ty>, Reason> {
        let (slot, read) = self
            .lease(origin)?
            .arena
            .as_ref()
            .ok_or(Reason::Invariant)?;
        self.owner(origin.activation)?
            .arenas
            .get(*slot)
            .and_then(Option::as_ref)
            .ok_or(Reason::Invariant)?
            .payload(read, allocator)
            .map_err(preparation)
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep verified operands, publication context and allocator explicit at the shared backend boundary."
    )]
    pub(super) fn arena_core<P: a::Provider>(
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
        let owner = self.arena_owner(first)?;
        if operation == O::ArenaRead {
            let second = arguments.next().ok_or(Reason::Invariant)?;
            let reference = g::Reference::from_token(self.token(self.root(second)?, allocator)?)
                .map_err(preparation)?;
            let id = result_loan.ok_or(Reason::Invariant)?;
            let origin = self.next_reference(id)?;
            let arena = self.frame.arenas[owner].as_mut().ok_or(Reason::Invariant)?;
            return match outcome(arena.read(reference, ty, allocator))? {
                Ok(read) => {
                    let at = arena.payload(&read, allocator).map_err(preparation)?.root();
                    self.frame
                        .borrow_target(id, Target::Arena { origin, at }, to)?;
                    self.frame.managed.as_mut().ok_or(Reason::Invariant)?.loans[id.0 as usize]
                        .arena = Some((owner, read));
                    self.loan_sum(to, 0, true, span, allocator)
                }
                Err(error) => {
                    self.immediate_result(to, Err(l::Immediate::Arena(error)), span, allocator)
                }
            };
        }
        let second = arguments.next().ok_or(Reason::Invariant)?;
        let (reference, source) = if operation == O::ArenaReplace {
            (
                Some(
                    g::Reference::from_token(self.token(self.root(second)?, allocator)?)
                        .map_err(preparation)?,
                ),
                arguments.next().ok_or(Reason::Invariant)?,
            )
        } else if operation == O::ArenaNew {
            (None, second)
        } else {
            return Err(Reason::Unsupported);
        };
        self.available(source)?;
        let copy = self.is_copy(source)?;
        let input = self
            .frame
            .storage
            .as_mut()
            .ok_or(Reason::Invariant)?
            .cell_mut(source)?;
        let arena = self.frame.arenas[owner].as_mut().ok_or(Reason::Invariant)?;
        let layouts = self.services.layouts;
        let inspector = self.services.inspection.as_mut().ok_or(Reason::Invariant)?;
        let inspect = |token: &l::Token,
                       arena: &g::Arena<'_, '_>,
                       allocator: &a::Allocator<P>,
                       budget: &mut g::PublicationBudget| {
            g::inspect(layouts, inspector, token, arena, allocator, budget)
        };
        let mut replaced = None;
        let result = if let Some(reference) = reference {
            if copy {
                outcome(arena.replace_copy(reference, input, span, allocator, inspect))?
                    .map(|_| None)
            } else {
                outcome(arena.replace_owned(reference, input, span, allocator, inspect))?.map(
                    |old| {
                        replaced = Some(old);
                        None
                    },
                )
            }
        } else {
            outcome(if copy {
                arena.allocate_copy(input, Some(span), allocator, inspect)
            } else {
                arena.allocate_from(input, Some(span), allocator, inspect)
            })?
            .map(Some)
        };
        let success = result.is_ok();
        if success && !copy && reference.is_none() {
            input.recycle(allocator)?;
            self.forget(source);
        }
        match result {
            Ok(Some(reference)) => self.token_result(to, Ok(reference.token()), span, allocator)?,
            Ok(None) => self.immediate_result(to, Ok(l::Immediate::Unit), span, allocator)?,
            Err(error) => {
                self.immediate_result(to, Err(l::Immediate::Arena(error)), span, allocator)?
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
        Ok(())
    }
}
impl Frame<'_, '_> {
    pub fn is_arena_owner(&self, slot: usize) -> Result<bool, Reason> {
        Ok(
            matches!(self.root(slot)?.at.ty(),o::Type::Constructed(id)if matches!(self.types.table().constructed[id.0 as usize].constructor,o::Constructor::ArenaOwner(_))),
        )
    }
}
