use super::*;
use m::ownership::core::Operation as Op;

fn bad() -> c::Cause {
    c::Cause::Execution(c::ExecutionFailure::Invariant)
}
fn fault<T: std::error::Error + 'static>(error: T) -> c::Cause {
    c::Cause::Execution(
        e::service_failure(&error)
            .map(Into::into)
            .unwrap_or(c::ExecutionFailure::Fault),
    )
}
fn buffer_fault(error: buffer::Fault) -> c::Cause {
    match error {
        buffer::Fault::ServiceWork(error) => control_cause(error),
        _ => fault(error),
    }
}

pub(super) enum Payload {
    Token([u8; 32]),
    Immediate(l::Immediate),
}
pub(super) fn result(
    unit: &mut State<'_, '_, '_, '_>,
    site: &n::CoreSite,
    outcome: Result<Payload, l::Immediate>,
) -> n::ResultStep {
    let (case, payload) = match outcome {
        Ok(value) => (0, value),
        Err(error) => (1, Payload::Immediate(error)),
    };
    let physical = Physical {
        frame: unit.depth(),
        cell: site.result,
    };
    {
        let mut store = unit.store.borrow_mut();
        let Store {
            pools, allocator, ..
        } = &mut *store;
        let cell = pools[physical.frame].cell(physical.cell).map_err(fault)?;
        let root = cell.root();
        cell.prepare_case(root, case, allocator).map_err(fault)?;
        let at = cell.child(root, 0, allocator).map_err(fault)?;
        match payload {
            Payload::Immediate(value) => {
                cell.write_immediate(at, value, allocator).map_err(fault)?
            }
            Payload::Token(token) => {
                let start = at.offset() as usize;
                cell.data_mut(allocator).map_err(fault)?[start..start + 32].copy_from_slice(&token);
            }
        }
        cell.initialize_order(root, [(0, site.span)].into_iter(), allocator)
            .map_err(fault)?;
    }
    values::publish(unit, site.result, physical, site.span)
}
fn bytes(
    unit: &State<'_, '_, '_, '_>,
    slot: usize,
) -> Result<(places::Target, buffer::Bytes), c::Cause> {
    let (target, mut token) = places::buffer_token(unit, slot)?;
    let value =
        buffer::Bytes::from_token(&mut token, unit.store.borrow().allocator).map_err(fault)?;
    Ok((target, value))
}
fn text(unit: &mut State<'_, '_, '_, '_>, slot: usize) -> Result<buffer::Text, c::Cause> {
    let (_, mut token) = places::buffer_token(unit, slot)?;
    let store = &unit.store;
    let mut work = unit.work();
    buffer::Text::from_token_metered(&mut token, store.borrow().allocator, &mut work)
        .map_err(buffer_fault)
}
fn allocated(
    unit: &mut State<'_, '_, '_, '_>,
    site: &n::CoreSite,
    outcome: buffer::Outcome<[u8; 32], buffer::AllocError>,
) -> n::ResultStep {
    match outcome {
        Ok(token) => result(unit, site, Ok(Payload::Token(token))),
        Err(buffer::Failure::Error(error)) => {
            result(unit, site, Err(l::Immediate::Allocation(error)))
        }
        Err(buffer::Failure::Fault(error)) => Err(buffer_fault(error)),
    }
}
pub(super) fn call(unit: &mut State<'_, '_, '_, '_>, site: &n::CoreSite) -> n::ResultStep {
    let arg = |index: usize| site.arguments[index];
    match site.operation {
        Op::Allocator => places::register(
            unit,
            site.loan.ok_or_else(bad)?,
            places::Target::Allocator,
            site.result,
            site.span,
        ),
        Op::BytesNew => {
            if !matches!(places::referent(unit, arg(0))?, places::Target::Allocator) {
                return Err(bad());
            }
            let outcome = {
                let mut store = unit.store.borrow_mut();
                buffer::Bytes::new(store.allocator).and_then(|mut value| {
                    value
                        .take_token(store.allocator)
                        .map_err(buffer::Failure::Fault)
                })
            };
            allocated(unit, site, outcome)
        }
        Op::BytesLen => {
            let (_, bytes) = bytes(unit, arg(0))?;
            let length = bytes.len(unit.store.borrow().allocator).map_err(fault)?;
            values::write_scalar(unit, site.result, length, site.span)
        }
        Op::BytesGet => {
            let index = values::read_scalar(unit, arg(1))?;
            let (_, bytes) = bytes(unit, arg(0))?;
            let outcome = {
                let store = &unit.store;
                let mut work = unit.work();
                bytes.get_metered(store.borrow().allocator, index, &mut work)
            };
            match outcome {
                Ok(value) => result(unit, site, Ok(Payload::Immediate(l::Immediate::I64(value)))),
                Err(buffer::Failure::Error(error)) => {
                    result(unit, site, Err(l::Immediate::Bounds(error)))
                }
                Err(buffer::Failure::Fault(error)) => Err(buffer_fault(error)),
            }
        }
        Op::BytesPush | Op::BytesSet => {
            let index = values::read_scalar(unit, arg(1))?;
            let value = if site.operation == Op::BytesSet {
                values::read_scalar(unit, arg(2))?
            } else {
                index
            };
            let (target, mut bytes) = bytes(unit, arg(0))?;
            let (outcome, token) = {
                let store = &unit.store;
                let mut work = unit.work();
                let mut store = store.borrow_mut();
                let outcome = if site.operation == Op::BytesSet {
                    bytes.set_metered(store.allocator, index, value, &mut work)
                } else {
                    bytes.push_metered(store.allocator, value, &mut work)
                };
                let token = bytes.take_token(store.allocator).map_err(fault)?;
                (outcome, token)
            };
            places::store_token(unit, target, token)?;
            match outcome {
                Ok(()) => result(unit, site, Ok(Payload::Immediate(l::Immediate::Unit))),
                Err(buffer::Failure::Error(error)) => {
                    result(unit, site, Err(l::Immediate::Buffer(error)))
                }
                Err(buffer::Failure::Fault(error)) => Err(buffer_fault(error)),
            }
        }
        Op::BytesClone | Op::TextClone | Op::TextBytes | Op::TextConcat => {
            if !matches!(places::referent(unit, arg(0))?, places::Target::Allocator) {
                return Err(bad());
            }
            let outcome = match site.operation {
                Op::BytesClone => {
                    let (_, bytes) = bytes(unit, arg(1))?;
                    let store = &unit.store;
                    let mut work = unit.work();
                    let mut store = store.borrow_mut();
                    bytes
                        .try_clone_metered(store.allocator, &mut work)
                        .and_then(|mut value| {
                            value
                                .take_token(store.allocator)
                                .map_err(buffer::Failure::Fault)
                        })
                }
                Op::TextClone => {
                    let text = text(unit, arg(1))?;
                    let store = &unit.store;
                    let mut work = unit.work();
                    let mut store = store.borrow_mut();
                    text.try_clone_metered(store.allocator, &mut work)
                        .and_then(|mut value| {
                            value
                                .take_token(store.allocator)
                                .map_err(buffer::Failure::Fault)
                        })
                }
                Op::TextBytes => {
                    let text = text(unit, arg(1))?;
                    let store = &unit.store;
                    let mut work = unit.work();
                    let mut store = store.borrow_mut();
                    text.to_bytes_metered(store.allocator, &mut work)
                        .and_then(|mut value| {
                            value
                                .take_token(store.allocator)
                                .map_err(buffer::Failure::Fault)
                        })
                }
                Op::TextConcat => {
                    let left = text(unit, arg(1))?;
                    let right = text(unit, arg(2))?;
                    let store = &unit.store;
                    let mut work = unit.work();
                    let mut store = store.borrow_mut();
                    left.concat_metered(&right, store.allocator, &mut work)
                        .and_then(|mut value| {
                            value
                                .take_token(store.allocator)
                                .map_err(buffer::Failure::Fault)
                        })
                }
                _ => unreachable!(),
            };
            allocated(unit, site, outcome)
        }
        Op::TextLen => {
            let text = text(unit, arg(0))?;
            let length = text.len(unit.store.borrow().allocator).map_err(fault)?;
            values::write_scalar(unit, site.result, length, site.span)
        }
        Op::TextEqual => {
            let left = text(unit, arg(0))?;
            let right = text(unit, arg(1))?;
            let equal = {
                let store = &unit.store;
                let mut work = unit.work();
                left.equal_metered(&right, store.borrow().allocator, &mut work)
                    .map_err(buffer_fault)?
            };
            values::write_scalar(unit, site.result, i64::from(equal), site.span)
        }
        Op::TextFromUtf8 => {
            let slot = unit.slot(arg(0));
            let physical = unit
                .ledger
                .as_ref()
                .unwrap()
                .value(slot)
                .map_err(invariant)?
                .physical()?;
            let outcome = {
                let store = &unit.store;
                let mut work = unit.work();
                let mut store = store.borrow_mut();
                let Store {
                    pools, allocator, ..
                } = &mut *store;
                let cell = pools[physical.frame].cell(physical.cell).map_err(fault)?;
                let mut token = cell
                    .read_token(cell.root(), allocator)
                    .map_err(fault)?
                    .bytes;
                let mut bytes = buffer::Bytes::from_token(&mut token, allocator).map_err(fault)?;
                let outcome = bytes.take_text_metered(allocator, &mut work);
                match outcome {
                    Ok(mut text) => Ok(Payload::Token(text.take_token(allocator).map_err(fault)?)),
                    Err(buffer::Failure::Error(error)) => Err(l::Immediate::Utf8(error)),
                    Err(buffer::Failure::Fault(error)) => return Err(buffer_fault(error)),
                }
            };
            {
                let mut store = unit.store.borrow_mut();
                let Store {
                    pools, allocator, ..
                } = &mut *store;
                pools[physical.frame]
                    .cell(physical.cell)
                    .map_err(fault)?
                    .data_mut(allocator)
                    .map_err(fault)?
                    .fill(0);
            }
            unit.ledger
                .as_mut()
                .unwrap()
                .take_owner(slot)
                .map_err(invariant)?;
            result(unit, site, outcome)
        }
        _ => Err(bad()),
    }
}
