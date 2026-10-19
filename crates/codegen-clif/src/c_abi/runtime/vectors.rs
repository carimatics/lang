use super::*;
use lang_runtime::vector as v;
use m::ownership::{self as o, core::Operation as Op};

fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
pub(super) fn fault(error: v::Error) -> c::Cause {
    if let Some(failure) = lang_runtime::execution::service_failure(&error) {
        return c::Cause::Execution(failure.into());
    }
    c::Cause::Execution(match error {
        v::Error::LoanLimit | v::Error::Allocation(a::Error::Limit(_) | a::Error::OutOfMemory) => {
            c::ExecutionFailure::AllocationLimit
        }
        v::Error::Allocation(a::Error::Overflow) => c::ExecutionFailure::CounterOverflow,
        v::Error::BorrowConflict => c::ExecutionFailure::Fault,
        _ => c::ExecutionFailure::Invariant,
    })
}
fn allocation<T>(value: Result<T, v::Error>) -> Result<Result<T, buffer::AllocError>, c::Cause> {
    match value {
        Ok(value) => Ok(Ok(value)),
        Err(v::Error::Allocation(a::Error::Limit(_))) => Ok(Err(buffer::AllocError::Limit)),
        Err(v::Error::Allocation(a::Error::OutOfMemory)) => {
            Ok(Err(buffer::AllocError::OutOfMemory))
        }
        Err(v::Error::Allocation(a::Error::Overflow)) => Ok(Err(buffer::AllocError::Overflow)),
        Err(error) => Err(fault(error)),
    }
}
pub(super) fn supports(op: Op) -> bool {
    matches!(
        op,
        Op::VecNew | Op::VecLen | Op::VecPush | Op::VecPop | Op::VecGet | Op::VecGetMut
    )
}
pub(super) fn view<'s, 'l>(
    store: &'s Store<'_, 'l>,
    read: usize,
) -> Result<v::Borrowed<'s, 'l, 'l>, c::Cause> {
    store
        .loans
        .get(read)
        .and_then(|loan| loan.vector_read.as_ref())
        .ok_or_else(bad)?
        .view(store.layouts, store.allocator)
        .map_err(fault)
}
fn owner<'l>(
    unit: &State<'_, '_, '_, 'l>,
    slot: usize,
    ty: o::Type,
) -> Result<v::Vector<'l, 'l>, c::Cause> {
    let (_, mut token) = places::buffer_token(unit, slot)?;
    let store = unit.store.borrow();
    v::Vector::from_token(store.layouts, ty, &mut token, store.allocator)
        .map_err(fault)
        .map(|value| value.with_service_work(unit.service_work.clone()))
}
fn physical(unit: &State<'_, '_, '_, '_>, slot: usize) -> Result<Physical, c::Cause> {
    unit.ledger
        .as_ref()
        .unwrap()
        .value(unit.slot(slot))
        .map_err(invariant)?
        .physical()
}
pub(super) fn call(unit: &mut State<'_, '_, '_, '_>, site: &n::CoreSite) -> n::ResultStep {
    let arg = |i: usize| site.arguments[i];
    let ty = *site.type_arguments.first().ok_or_else(bad)?;
    match site.operation {
        Op::VecNew => {
            if !matches!(places::referent(unit, arg(0))?, places::Target::Allocator) {
                return Err(bad());
            }
            let outcome = {
                let mut store = unit.store.borrow_mut();
                let layouts = store.layouts;
                match allocation(v::Vector::new(layouts, ty, store.allocator))? {
                    Ok(mut value) => Ok(core::Payload::Token(
                        value.take_token(store.allocator).map_err(fault)?,
                    )),
                    Err(error) => Err(l::Immediate::Allocation(error)),
                }
            };
            core::result(unit, site, outcome)
        }
        Op::VecLen => {
            let owner = owner(unit, arg(0), ty)?;
            let length = owner.len(unit.store.borrow().allocator).map_err(fault)?;
            values::write_scalar(unit, site.result, length, site.span)
        }
        Op::VecPush => {
            let mut owner = owner(unit, arg(0), ty)?;
            let source = physical(unit, arg(1))?;
            let copied = values::is_copy(unit, arg(1));
            let outcome = {
                let mut store = unit.store.borrow_mut();
                let Store {
                    pools, allocator, ..
                } = &mut *store;
                let input = pools[source.frame]
                    .cell_mut(source.cell)
                    .map_err(|_| bad())?;
                let outcome = allocation(if copied {
                    owner.push_copy(input, Some(site.span), allocator)
                } else {
                    owner.push_from(input, Some(site.span), allocator)
                })?;
                if outcome.is_ok() && !copied {
                    input.recycle(allocator).map_err(|_| bad())?;
                }
                outcome
            };
            let moved = outcome.is_ok();
            core::result(
                unit,
                site,
                outcome
                    .map(|()| core::Payload::Immediate(l::Immediate::Unit))
                    .map_err(l::Immediate::Allocation),
            )?;
            if !copied {
                let slot = unit.slot(arg(1));
                let ledger = unit.ledger.as_mut().unwrap();
                if moved {
                    ledger.take_owner(slot).map_err(invariant)?;
                } else {
                    ledger.drop_value(slot).map_err(invariant)?;
                }
            }
            Ok(())
        }
        Op::VecPop => {
            let mut owner = owner(unit, arg(0), ty)?;
            let output = Physical {
                frame: unit.depth(),
                cell: site.result,
            };
            {
                let mut store = unit.store.borrow_mut();
                let Store {
                    pools, allocator, ..
                } = &mut *store;
                let empty = owner.is_empty(allocator).map_err(fault)?;
                let cell = pools[output.frame].cell(output.cell).map_err(|_| bad())?;
                let root = cell.root();
                cell.prepare_case(root, u32::from(!empty), allocator)
                    .map_err(|_| bad())?;
                let target = if empty {
                    root
                } else {
                    cell.child(root, 0, allocator).map_err(|_| bad())?
                };
                let moved = owner.pop_into(cell, target, allocator).map_err(fault)?;
                cell.initialize_order(root, moved.then_some((0, site.span)).into_iter(), allocator)
                    .map_err(|_| bad())?;
            }
            values::publish(unit, site.result, output, site.span)
        }
        Op::VecGet | Op::VecGetMut => {
            let loan = site.loan.ok_or_else(bad)?;
            let id = places::available(unit, loan)?;
            if unit.store.borrow().loans[id].vector_read.is_some() {
                return Err(bad());
            }
            let custody = shared::custody(unit, places::referent(unit, arg(0))?)?;
            let mut owner = owner(unit, arg(0), ty)?;
            let index = values::read_scalar(unit, arg(1))?;
            let kind = if site.operation == Op::VecGetMut {
                o::LoanKind::Exclusive
            } else {
                o::LoanKind::Shared
            };
            let borrowed = owner.borrow(index, kind, unit.store.borrow_mut().allocator);
            let outcome = match borrowed {
                Ok(read) => {
                    let location = {
                        let store = unit.store.borrow();
                        read.view(store.layouts, store.allocator)
                            .map_err(fault)?
                            .root()
                    };
                    unit.store.borrow_mut().loans[id].vector_read = Some(read);
                    unit.store.borrow_mut().loans[id].custody = custody;
                    let token = places::register_token(
                        unit,
                        loan,
                        places::Target::Vector { read: id, location },
                        site.span,
                    )?;
                    Ok(core::Payload::Token(token))
                }
                Err(v::Error::Bounds(error)) => Err(l::Immediate::Bounds(error)),
                Err(error) => return Err(fault(error)),
            };
            core::result(unit, site, outcome)
        }
        _ => Err(bad()),
    }
}
pub(super) fn replace(
    unit: &mut State<'_, '_, '_, '_>,
    read: usize,
    location: l::Location,
    value: usize,
    span: m::Span,
) -> n::ResultStep {
    let source = physical(unit, value)?;
    let copied = values::is_copy(unit, value);
    let old = {
        let mut store = unit.store.borrow_mut();
        let Store {
            pools,
            layouts,
            loans,
            allocator,
            ..
        } = &mut *store;
        let loan = loans[read].vector_read.as_ref().ok_or_else(bad)?;
        let input = pools[source.frame]
            .cell_mut(source.cell)
            .map_err(|_| bad())?;
        if copied {
            loan.replace_copy(layouts, location, input, input.root(), span, allocator)
                .map_err(fault)?;
            None
        } else {
            loan.replace_from(layouts, location, input, span, allocator)
                .map_err(fault)?
        }
    };
    if !copied {
        let slot = unit.slot(value);
        unit.ledger
            .as_mut()
            .unwrap()
            .retire_replaced(slot, old)
            .map_err(invariant)?;
    }
    Ok(())
}
