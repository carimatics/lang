use super::*;
use lang_runtime::shared::managed as sh;
use m::ownership::{self as o, core::Operation as Op};

fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
fn outcome<T>(value: Result<T, sh::Error>) -> Result<Result<T, l::OwnershipError>, c::Cause> {
    if let Err(error) = &value
        && let Some(failure) = lang_runtime::execution::service_failure(error)
    {
        return Err(c::Cause::Execution(failure.into()));
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
        }) => Err(c::Cause::Execution(c::ExecutionFailure::Fault)),
        Err(_) => Err(bad()),
    }
}
pub(super) fn supports(op: Op) -> bool {
    matches!(
        op,
        Op::SharedNew
            | Op::SharedClone
            | Op::SharedRead
            | Op::SharedReplace
            | Op::WeakNew
            | Op::WeakClone
            | Op::WeakUpgrade
    )
}
pub(super) fn view<'s, 'l>(
    store: &'s Store<'_, 'l>,
    read: usize,
    ty: o::Type,
) -> Result<lang_runtime::shared::storage::View<'s, 'l, 'l>, c::Cause> {
    let read = store
        .loans
        .get(read)
        .and_then(|loan| loan.shared_read)
        .ok_or_else(bad)?;
    store
        .heap
        .as_ref()
        .ok_or_else(bad)?
        .view(read, ty, store.allocator)
        .map_err(service_invariant)
}
pub(super) fn custody(
    unit: &State<'_, '_, '_, '_>,
    target: places::Target,
) -> Result<Option<sh::Custody>, c::Cause> {
    let store = unit.store.borrow();
    match target {
        places::Target::Shared { read, ty, .. } => {
            let read = store.loans[read].shared_read.ok_or_else(bad)?;
            store
                .heap
                .as_ref()
                .ok_or_else(bad)?
                .custody(read, ty, store.allocator)
                .map(Some)
                .map_err(service_invariant)
        }
        places::Target::Map { read, .. } | places::Target::Vector { read, .. } => {
            Ok(store.loans[read].custody)
        }
        _ => Ok(None),
    }
}
fn result(
    unit: &mut State<'_, '_, '_, '_>,
    site: &n::CoreSite,
    value: Result<Option<[u8; 32]>, l::OwnershipError>,
) -> n::ResultStep {
    core::result(
        unit,
        site,
        value
            .map(|token| match token {
                Some(token) => core::Payload::Token(token),
                None => core::Payload::Immediate(l::Immediate::Unit),
            })
            .map_err(l::Immediate::Ownership),
    )
}
pub(super) fn call(unit: &mut State<'_, '_, '_, '_>, site: &n::CoreSite) -> n::ResultStep {
    if matches!(site.operation, Op::SharedNew | Op::SharedReplace) {
        return store(unit, site);
    }
    let target = places::referent(unit, site.arguments[0])?;
    let observed = places::observation(unit, target)?;
    let custody = custody(unit, target)?;
    match site.operation {
        Op::SharedClone | Op::WeakNew | Op::WeakClone | Op::WeakUpgrade => {
            let action = match site.operation {
                Op::SharedClone => sh::FieldAction::CloneStrong,
                Op::WeakNew => sh::FieldAction::NewWeak,
                Op::WeakClone => sh::FieldAction::CloneWeak,
                Op::WeakUpgrade => sh::FieldAction::UpgradeWeak,
                _ => return Err(bad()),
            };
            let value = {
                let mut state = unit.store.borrow_mut();
                let Store {
                    heap, allocator, ..
                } = &mut *state;
                outcome(
                    heap.as_mut()
                        .ok_or_else(bad)?
                        .retain_observed(&observed, custody, action, allocator),
                )?
                .map(|value| {
                    Some(match value {
                        sh::Retained::Strong(value) => value.into_token(),
                        sh::Retained::Weak(value) => value.into_token(),
                    })
                })
            };
            result(unit, site, value)
        }
        Op::SharedRead => {
            let loan = site.loan.ok_or_else(bad)?;
            let id = places::available(unit, loan)?;
            {
                let state = unit.store.borrow();
                let entry = &state.loans[id];
                if entry.shared_read.is_some()
                    || entry.vector_read.is_some()
                    || entry.map_read.is_some()
                {
                    return Err(bad());
                }
            }
            let value = {
                let mut state = unit.store.borrow_mut();
                let Store {
                    heap, allocator, ..
                } = &mut *state;
                outcome(
                    heap.as_mut()
                        .ok_or_else(bad)?
                        .read_observed(&observed, custody, allocator),
                )?
            };
            let value = match value {
                Ok(read) => {
                    let ty = site.type_arguments[0];
                    let location = {
                        let state = unit.store.borrow();
                        state
                            .heap
                            .as_ref()
                            .ok_or_else(bad)?
                            .view(read, ty, state.allocator)
                            .map_err(service_invariant)?
                            .root()
                    };
                    unit.store.borrow_mut().loans[id].shared_read = Some(read);
                    Ok(Some(places::register_token(
                        unit,
                        loan,
                        places::Target::Shared {
                            read: id,
                            ty,
                            location,
                        },
                        site.span,
                    )?))
                }
                Err(error) => Err(error),
            };
            result(unit, site, value)
        }
        _ => Err(bad()),
    }
}

enum Transfer {
    Copy,
    Moved,
    Replaced(Option<m::Span>),
    Rejected,
}
fn store(unit: &mut State<'_, '_, '_, '_>, site: &n::CoreSite) -> n::ResultStep {
    let source_slot = *site.arguments.last().ok_or_else(bad)?;
    let physical = unit
        .ledger
        .as_ref()
        .unwrap()
        .value(unit.slot(source_slot))
        .map_err(invariant)?
        .physical()?;
    let copied = values::is_copy(unit, source_slot);
    let target = if site.operation == Op::SharedReplace {
        Some(places::referent(unit, site.arguments[0])?)
    } else {
        if !matches!(
            places::referent(unit, site.arguments[0])?,
            places::Target::Allocator
        ) {
            return Err(bad());
        }
        None
    };
    let observed = target
        .map(|target| places::observation(unit, target))
        .transpose()?;
    let custody = target
        .map(|target| custody(unit, target))
        .transpose()?
        .flatten();
    let (value, transfer) = {
        let mut state = unit.store.borrow_mut();
        let Store {
            pools,
            heap,
            allocator,
            ..
        } = &mut *state;
        let heap = heap.as_mut().ok_or_else(bad)?;
        let ready = if heap.is_open() {
            Ok(())
        } else {
            outcome(heap.open(allocator))?
        };
        let input = pools[physical.frame]
            .cell_mut(physical.cell)
            .map_err(service_invariant)?;
        let value = match ready {
            Err(error) => Err(error),
            Ok(()) if target.is_some() => {
                if copied {
                    outcome(heap.replace_observed_copy(
                        observed.as_ref().unwrap(),
                        custody,
                        input,
                        site.span,
                        allocator,
                    ))?
                    .map(|()| (None, Transfer::Copy))
                } else {
                    outcome(heap.replace_observed(
                        observed.as_ref().unwrap(),
                        custody,
                        input,
                        site.span,
                        allocator,
                    ))?
                    .map(|span| (None, Transfer::Replaced(span)))
                }
            }
            Ok(()) => outcome(if copied {
                heap.allocate_copy(input, Some(site.span), allocator)
            } else {
                heap.allocate_from(input, Some(site.span), allocator)
            })?
            .map(|owner| {
                (
                    Some(owner.into_token()),
                    if copied {
                        Transfer::Copy
                    } else {
                        Transfer::Moved
                    },
                )
            }),
        };
        let (value, transfer) = match value {
            Ok((token, transfer)) => (Ok(token), transfer),
            Err(error) => (Err(error), Transfer::Rejected),
        };
        if matches!(transfer, Transfer::Moved) {
            input.recycle(allocator).map_err(service_invariant)?;
        }
        (value, transfer)
    };
    result(unit, site, value)?;
    if !copied {
        let slot = unit.slot(source_slot);
        let ledger = unit.ledger.as_mut().unwrap();
        match transfer {
            Transfer::Moved => {
                ledger.take_owner(slot).map_err(invariant)?;
            }
            Transfer::Replaced(span) => ledger.retire_replaced(slot, span).map_err(invariant)?,
            Transfer::Rejected => ledger.drop_value(slot).map_err(invariant)?,
            Transfer::Copy => return Err(bad()),
        }
    }
    Ok(())
}
