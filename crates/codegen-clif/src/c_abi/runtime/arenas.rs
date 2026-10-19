use super::*;
use lang_runtime::{arena::managed as g, inspection as ins};
use m::ownership::{self as o, core::Operation as Op};

fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
fn outcome<T>(value: Result<T, g::Error>) -> Result<Result<T, l::ArenaError>, c::Cause> {
    if let Err(error) = &value
        && let Some(failure) = lang_runtime::execution::service_failure(error)
    {
        return Err(c::Cause::Execution(failure.into()));
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
        }) => Err(c::Cause::Execution(c::ExecutionFailure::Fault)),
        Err(_) => Err(bad()),
    }
}
fn inspect<P: a::Provider>(
    layouts: &l::Layouts<'_>,
    inspector: &mut Option<ins::Inspector<'_, '_>>,
    token: &l::Token,
    arena: &g::Arena<'_, '_>,
    allocator: &a::Allocator<P>,
    budget: &mut g::PublicationBudget,
) -> Result<(), g::Error> {
    let mut read = inspector
        .as_mut()
        .ok_or(g::Error {
            kind: g::Kind::State,
            span: None,
        })?
        .token(token, allocator)?;
    while let Some(observed) = budget.inspect_next(&mut read)? {
        let mut bytes = observed.bytes;
        let Some(l::Layout {
            kind: l::Kind::Token(constructor),
            ..
        }) = layouts.get(observed.ty)
        else {
            return Err(g::Error {
                kind: g::Kind::State,
                span: observed.span,
            });
        };
        match constructor {
            o::Constructor::Arena(ty) => {
                arena.resolve(g::Reference::from_token(bytes)?, *ty, allocator)?
            }
            o::Constructor::Shared(_) | o::Constructor::Weak(_) => {
                return Err(g::Error {
                    kind: g::Kind::Arena(l::ArenaError::CrossRegion),
                    span: observed.span,
                });
            }
            o::Constructor::Builtin(o::Builtin::Bytes) => {
                buffer::Bytes::from_token(&mut bytes, allocator)
                    .and_then(|v| v.len(allocator))
                    .map_err(|e| g::Error {
                        kind: g::Kind::Buffer(e),
                        span: observed.span,
                    })?;
            }
            o::Constructor::Builtin(o::Builtin::Text) => {
                let text = buffer::Text::from_token_metered(&mut bytes, allocator, &mut |n| {
                    budget.service(n)
                })
                .map_err(|e| g::Error {
                    kind: g::Kind::Buffer(e),
                    span: observed.span,
                })?;
                let length = usize::try_from(text.len(allocator).map_err(|e| g::Error {
                    kind: g::Kind::Buffer(e),
                    span: observed.span,
                })?)
                .map_err(|_| g::Error {
                    kind: g::Kind::State,
                    span: observed.span,
                })?;
                budget.observe(length)?;
                text.as_str(allocator).map_err(|e| g::Error {
                    kind: g::Kind::Buffer(e),
                    span: observed.span,
                })?;
            }
            _ => {
                return Err(g::Error {
                    kind: g::Kind::Unsupported(observed.ty),
                    span: observed.span,
                });
            }
        }
    }
    Ok(())
}

pub(super) fn supports(op: Op) -> bool {
    matches!(op, Op::ArenaNew | Op::ArenaRead | Op::ArenaReplace)
}
pub(super) fn view<'s, 'l, P: a::Provider>(
    store: &'s Store<'_, 'l, P>,
    owner: usize,
    read: usize,
) -> Result<g::View<'s, 'l, 'l>, c::Cause> {
    let (expected, read) = store
        .loans
        .get(read)
        .and_then(|loan| loan.arena_read.as_ref())
        .ok_or_else(bad)?;
    if *expected != owner || !store.arena_live[owner] {
        return Err(bad());
    }
    store.arena[owner]
        .as_ref()
        .ok_or_else(bad)?
        .payload(read, store.allocator)
        .map_err(service_invariant)
}
pub(super) fn enter<P: a::Provider>(
    unit: &mut State<'_, '_, '_, '_, P>,
    site: &n::ArenaSite,
) -> n::ResultStep {
    if !matches!(
        places::referent(unit, site.allocator)?,
        places::Target::Allocator
    ) {
        return Err(bad());
    }
    let depth = unit.depth();
    let id = depth * unit.stride + site.owner;
    {
        let mut store = unit.store.borrow_mut();
        let Store {
            arena,
            arena_live,
            allocator,
            ..
        } = &mut *store;
        if arena_live[id] {
            return Err(bad());
        }
        arena[id]
            .as_mut()
            .ok_or_else(bad)?
            .open(allocator)
            .map_err(|_| c::Cause::Execution(c::ExecutionFailure::AllocationLimit))?;
        arena_live[id] = true;
    }
    values::publish(
        unit,
        site.owner,
        Physical {
            frame: depth,
            cell: site.owner,
        },
        site.span,
    )
}
fn result<P: a::Provider>(
    unit: &mut State<'_, '_, '_, '_, P>,
    site: &n::CoreSite,
    value: Result<Option<g::Reference>, l::ArenaError>,
) -> n::ResultStep {
    core::result(
        unit,
        site,
        value
            .map(|value| match value {
                Some(reference) => core::Payload::Token(reference.token()),
                None => core::Payload::Immediate(l::Immediate::Unit),
            })
            .map_err(l::Immediate::Arena),
    )
}
pub(super) fn call<P: a::Provider>(
    unit: &mut State<'_, '_, '_, '_, P>,
    site: &n::CoreSite,
) -> n::ResultStep {
    let owner = places::arena_owner(unit, site.arguments[0])?;
    if site.operation == Op::ArenaRead {
        let reference = g::Reference::from_token(places::buffer_token(unit, site.arguments[1])?.1)
            .map_err(service_invariant)?;
        let loan = site.loan.ok_or_else(bad)?;
        let id = places::available(unit, loan)?;
        {
            let store = unit.store.borrow();
            let entry = &store.loans[id];
            if entry.arena_read.is_some()
                || entry.shared_read.is_some()
                || entry.map_read.is_some()
                || entry.vector_read.is_some()
            {
                return Err(bad());
            }
        }
        let value = {
            let mut store = unit.store.borrow_mut();
            let Store {
                arena, allocator, ..
            } = &mut *store;
            outcome(arena[owner].as_mut().ok_or_else(bad)?.read(
                reference,
                site.type_arguments[0],
                allocator,
            ))?
        };
        let value = match value {
            Ok(read) => {
                let location = {
                    let store = unit.store.borrow();
                    store.arena[owner]
                        .as_ref()
                        .ok_or_else(bad)?
                        .payload(&read, store.allocator)
                        .map_err(service_invariant)?
                        .root()
                };
                unit.store.borrow_mut().loans[id].arena_read = Some((owner, read));
                Ok(core::Payload::Token(places::register_token(
                    unit,
                    loan,
                    places::Target::Arena {
                        owner,
                        read: id,
                        location,
                    },
                    site.span,
                )?))
            }
            Err(error) => Err(l::Immediate::Arena(error)),
        };
        return core::result(unit, site, value);
    }
    store(unit, site, owner)
}
enum Transfer {
    Copy,
    Moved,
    Replaced(Option<m::Span>),
    Rejected,
}
fn store<P: a::Provider>(
    unit: &mut State<'_, '_, '_, '_, P>,
    site: &n::CoreSite,
    owner: usize,
) -> n::ResultStep {
    let source_slot = *site.arguments.last().ok_or_else(bad)?;
    let source = unit
        .ledger
        .as_ref()
        .unwrap()
        .value(unit.slot(source_slot))
        .map_err(invariant)?
        .physical()?;
    let copied = values::is_copy(unit, source_slot);
    let reference = if site.operation == Op::ArenaReplace {
        Some(
            g::Reference::from_token(places::buffer_token(unit, site.arguments[1])?.1)
                .map_err(service_invariant)?,
        )
    } else {
        None
    };
    let (value, transfer) = {
        let mut state = unit.store.borrow_mut();
        let Store {
            pools,
            arena,
            allocator,
            layouts,
            inspection,
            ..
        } = &mut *state;
        let input = pools[source.frame]
            .cell_mut(source.cell)
            .map_err(service_invariant)?;
        let arena = arena[owner].as_mut().ok_or_else(bad)?;
        let inspect = |token: &l::Token,
                       arena: &g::Arena<'_, '_>,
                       allocator: &a::Allocator<P>,
                       budget: &mut g::PublicationBudget| {
            inspect(layouts, inspection, token, arena, allocator, budget)
        };
        let value = if let Some(reference) = reference {
            if copied {
                outcome(arena.replace_copy(reference, input, site.span, allocator, inspect))?
                    .map(|()| (None, Transfer::Copy))
            } else {
                outcome(arena.replace_owned(reference, input, site.span, allocator, inspect))?
                    .map(|span| (None, Transfer::Replaced(span)))
            }
        } else {
            outcome(if copied {
                arena.allocate_copy(input, Some(site.span), allocator, inspect)
            } else {
                arena.allocate_from(input, Some(site.span), allocator, inspect)
            })?
            .map(|reference| {
                (
                    Some(reference),
                    if copied {
                        Transfer::Copy
                    } else {
                        Transfer::Moved
                    },
                )
            })
        };
        let (value, transfer) = match value {
            Ok((value, transfer)) => (Ok(value), transfer),
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
