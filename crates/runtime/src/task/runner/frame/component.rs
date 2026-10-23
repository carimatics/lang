//! Component input construction over existing typed reservations and Vector.
//! Resource claims stay unpublished until the original table's atomic commit.
use super::*;
use crate::{
    buffer,
    component::{
        execution::{FrameArgument, Host},
        wire,
    },
    retirement as r, vector,
};
enum Inline {
    Read(l::Location, wire::ValueId),
    Finish(l::Location, u32),
}
enum Task {
    Data(usize, l::Location, wire::ValueId),
    List(usize, l::Location, wire::ValueId, usize),
    Push {
        parent: usize,
        at: l::Location,
        list: wire::ValueId,
        index: usize,
        temporary: usize,
    },
}
struct Temporary<'l, 't> {
    value: l::Reservation<'l, 't>,
    initialized: bool,
}
fn vector_fault(error: vector::Error) -> Reason {
    match error {
        vector::Error::Allocation(error) => Reason::Layout(l::Error::Allocation(error)),
        vector::Error::Layout(error) => Reason::Layout(error),
        _ => Reason::Invariant,
    }
}
fn buffer_fault(error: buffer::Failure<buffer::AllocError>) -> Reason {
    match error {
        buffer::Failure::Error(buffer::AllocError::OutOfMemory) => {
            Reason::Layout(l::Error::Allocation(a::Error::OutOfMemory))
        }
        buffer::Failure::Error(buffer::AllocError::Overflow) => {
            Reason::Layout(l::Error::Allocation(a::Error::Overflow))
        }
        buffer::Failure::Error(buffer::AllocError::Limit) => {
            Reason::Execution(f::ExecutionFailure::AllocationLimit)
        }
        buffer::Failure::Fault(buffer::Fault::Allocator(error)) => {
            Reason::Layout(l::Error::Allocation(error))
        }
        _ => Reason::Invariant,
    }
}
fn store<P: a::Provider>(
    cell: &l::Reservation<'_, '_>,
    at: l::Location,
    token: [u8; 32],
    allocator: &mut a::Allocator<P>,
) -> Result<(), Reason> {
    let start = usize::try_from(at.offset()).map_err(|_| Reason::Invariant)?;
    let end = start.checked_add(32).ok_or(Reason::Invariant)?;
    cell.data_mut(allocator)?
        .get_mut(start..end)
        .ok_or(Reason::Invariant)?
        .copy_from_slice(&token);
    Ok(())
}
fn cell<'a, 'l, 't>(
    root: &'a l::Reservation<'l, 't>,
    temporaries: &'a [Option<Temporary<'l, 't>>],
    index: usize,
) -> Result<&'a l::Reservation<'l, 't>, Reason> {
    if index == 0 {
        Ok(root)
    } else {
        Ok(&temporaries
            .get(index - 1)
            .and_then(Option::as_ref)
            .ok_or(Reason::Invariant)?
            .value)
    }
}
#[allow(
    clippy::too_many_arguments,
    reason = "Reuse one bounded inline workspace for every unpublished typed element."
)]
fn shape<P: a::Provider>(
    cell: &l::Reservation<'_, '_>,
    root: wire::ValueId,
    values: &wire::Values,
    host: &dyn Host,
    allocator: &mut a::Allocator<P>,
    budget: &mut m::Budget,
    span: m::Span,
    work: &mut Vec<Inline>,
    data: &mut Vec<(l::Location, wire::ValueId)>,
) -> Result<(), Reason> {
    work.clear();
    data.clear();
    work.push(Inline::Read(cell.root(), root));
    while let Some(next) = work.pop() {
        budget.charge(1)?;
        let (at, id) = match next {
            Inline::Finish(at, count) => {
                cell.initialize_order(at, (0..count).map(|field| (field, span)), allocator)?;
                continue;
            }
            Inline::Read(at, id) => (at, id),
        };
        let value = values.nodes.get(id.0).ok_or(Reason::Entry)?;
        match value {
            wire::Value::I64(value) => {
                cell.write_immediate(at, l::Immediate::I64(*value), allocator)?
            }
            wire::Value::Bool(value) => {
                cell.write_immediate(at, l::Immediate::Bool(*value), allocator)?
            }
            wire::Value::Unit => cell.write_immediate(at, l::Immediate::Unit, allocator)?,
            wire::Value::Resource(_) => {
                if !matches!(
                    cell.layouts().get(at.ty()).ok_or(Reason::Entry)?.kind,
                    l::Kind::Token(o::Constructor::ComponentResource(_))
                ) {
                    return Err(Reason::Entry);
                }
                store(cell, at, host.argument(id.0)?.token(), allocator)?;
            }
            wire::Value::Text(_) | wire::Value::Bytes(_) | wire::Value::List(_) => {
                let kind = &cell.layouts().get(at.ty()).ok_or(Reason::Entry)?.kind;
                let valid = match value {
                    wire::Value::Text(_) => matches!(
                        kind,
                        l::Kind::Token(o::Constructor::Builtin(o::Builtin::Text))
                    ),
                    wire::Value::Bytes(_) => matches!(
                        kind,
                        l::Kind::Token(o::Constructor::Builtin(o::Builtin::Bytes))
                    ),
                    wire::Value::List(_) => matches!(kind, l::Kind::Token(o::Constructor::Vec(_))),
                    _ => false,
                };
                if !valid {
                    return Err(Reason::Entry);
                }
                store(cell, at, [0; 32], allocator)?;
                data.push((at, id));
            }
            wire::Value::Record(fields) => {
                let l::Kind::Record(aggregate) =
                    &cell.layouts().get(at.ty()).ok_or(Reason::Entry)?.kind
                else {
                    return Err(Reason::Entry);
                };
                if aggregate.fields.len() != fields.len() {
                    return Err(Reason::Entry);
                }
                work.push(Inline::Finish(
                    at,
                    u32::try_from(fields.len()).map_err(|_| Reason::Entry)?,
                ));
                for (index, field) in fields.iter().enumerate().rev() {
                    work.push(Inline::Read(
                        cell.child(at, index as u32, allocator)?,
                        *field,
                    ));
                }
            }
            wire::Value::Variant { case, payload } => {
                cell.prepare_case(at, *case, allocator)?;
                let l::Kind::Enum { cases, .. } =
                    &cell.layouts().get(at.ty()).ok_or(Reason::Entry)?.kind
                else {
                    return Err(Reason::Entry);
                };
                match cases[*case as usize].fields.len() {
                    0 if matches!(values.nodes.get(payload.0), Some(wire::Value::Unit)) => {
                        work.push(Inline::Finish(at, 0))
                    }
                    1 => {
                        work.push(Inline::Finish(at, 1));
                        work.push(Inline::Read(cell.child(at, 0, allocator)?, *payload));
                    }
                    _ => return Err(Reason::Entry),
                }
            }
            wire::Value::None => {
                cell.prepare_case(at, 0, allocator)?;
                work.push(Inline::Finish(at, 0));
            }
            wire::Value::Some(payload) | wire::Value::Ok(payload) | wire::Value::Err(payload) => {
                let case = u32::from(matches!(value, wire::Value::Some(_) | wire::Value::Err(_)));
                cell.prepare_case(at, case, allocator)?;
                work.push(Inline::Finish(at, 1));
                work.push(Inline::Read(cell.child(at, 0, allocator)?, *payload));
            }
        }
    }
    Ok(())
}
fn unpublished<P: a::Provider>(
    types: &o::VerifiedTypes,
    token: &mut l::Token,
    allocator: &mut a::Allocator<P>,
) -> Result<(), u32> {
    if token.bytes == [0; 32] {
        return Ok(());
    }
    if matches!(token.leaf.ty,o::Type::Constructed(id)if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::ComponentResource(_)))
    {
        token.bytes.fill(0);
        return Ok(());
    }
    buffer::cleanup_token(types, token, allocator).map_err(|_| 3u32)
}
impl<'l, 't> Frame<'l, 't> {
    #[allow(
        clippy::too_many_arguments,
        reason = "Keep original allocator, bounded workspaces and the existing cleanup services explicit."
    )]
    pub fn stage_component<P: a::Provider>(
        &mut self,
        slot: usize,
        argument: FrameArgument,
        host: &dyn Host,
        allocator: &mut a::Allocator<P>,
        budget: &mut m::Budget,
        ready: &mut Vec<usize>,
        drain: &mut Option<r::Drain<'l, 't>>,
        session: &mut c::Session,
    ) -> Result<(), Reason> {
        let values = host.frame(argument)?;
        let count = values.nodes.len();
        let mut inline = budget.vector(count.checked_mul(2).ok_or(Reason::Invariant)?)?;
        let mut data = budget.vector(count)?;
        let mut tasks = budget.vector(count.checked_mul(2).ok_or(Reason::Invariant)?)?;
        let mut temporaries: Vec<Option<Temporary<'l, 't>>> = budget.vector(count)?;
        let mut free = budget.vector(count)?;
        let root = self.data()?.cell(slot)?;
        let layouts = root.layouts();
        let span = self.origin;
        shape(
            root,
            argument.root(),
            values,
            host,
            allocator,
            budget,
            span,
            &mut inline,
            &mut data,
        )?;
        ready.push(slot);
        for (at, id) in data.drain(..).rev() {
            tasks.push(Task::Data(0, at, id));
        }
        let staged = (|| -> Result<(), Reason> {
            while let Some(task) = tasks.pop() {
                budget.charge(1)?;
                match task {
                    Task::Data(storage, at, id) => {
                        let target = cell(root, &temporaries, storage)?;
                        match &values.nodes[id.0] {
                            wire::Value::List(_) => {
                                let l::Kind::Token(o::Constructor::Vec(element)) =
                                    layouts.get(at.ty()).ok_or(Reason::Entry)?.kind
                                else {
                                    return Err(Reason::Entry);
                                };
                                let mut vector = vector::Vector::new(layouts, element, allocator)
                                    .map_err(vector_fault)?;
                                let token = vector.take_token(allocator).map_err(vector_fault)?;
                                store(target, at, token, allocator)?;
                                tasks.push(Task::List(storage, at, id, 0));
                            }
                            value => {
                                let bytes = match value {
                                    wire::Value::Text(text) => text.as_bytes(),
                                    wire::Value::Bytes(bytes) => bytes.as_slice(),
                                    _ => return Err(Reason::Invariant),
                                };
                                budget.charge(bytes.len())?;
                                let mut buffer = buffer::Bytes::from_slice(allocator, bytes)
                                    .map_err(buffer_fault)?;
                                let token = if matches!(value, wire::Value::Text(_)) {
                                    match buffer.take_text(allocator) {
                                        Ok(mut text) => text
                                            .take_token(allocator)
                                            .map_err(|_| Reason::Invariant)?,
                                        Err(_) => {
                                            let _ = buffer.release(allocator);
                                            return Err(Reason::Invariant);
                                        }
                                    }
                                } else {
                                    buffer
                                        .take_token(allocator)
                                        .map_err(|_| Reason::Invariant)?
                                };
                                store(target, at, token, allocator)?;
                            }
                        }
                    }
                    Task::List(parent, at, list, index) => {
                        let wire::Value::List(children) = &values.nodes[list.0] else {
                            return Err(Reason::Invariant);
                        };
                        let Some(child) = children.get(index) else {
                            continue;
                        };
                        let l::Kind::Token(o::Constructor::Vec(element)) =
                            layouts.get(at.ty()).ok_or(Reason::Entry)?.kind
                        else {
                            return Err(Reason::Entry);
                        };
                        let value = l::Reservation::allocate(layouts, element, allocator)?;
                        let temporary = if let Some(index) = free.pop() {
                            temporaries[index] = Some(Temporary {
                                value,
                                initialized: false,
                            });
                            index
                        } else {
                            let index = temporaries.len();
                            temporaries.push(Some(Temporary {
                                value,
                                initialized: false,
                            }));
                            index
                        };
                        let temporary_value =
                            &mut temporaries[temporary].as_mut().ok_or(Reason::Invariant)?;
                        shape(
                            &temporary_value.value,
                            *child,
                            values,
                            host,
                            allocator,
                            budget,
                            span,
                            &mut inline,
                            &mut data,
                        )?;
                        temporary_value.initialized = true;
                        tasks.push(Task::Push {
                            parent,
                            at,
                            list,
                            index,
                            temporary,
                        });
                        for (at, id) in data.drain(..).rev() {
                            tasks.push(Task::Data(temporary + 1, at, id));
                        }
                    }
                    Task::Push {
                        parent,
                        at,
                        list,
                        index,
                        temporary,
                    } => {
                        let mut token =
                            cell(root, &temporaries, parent)?.read_token(at, allocator)?;
                        let l::Kind::Token(o::Constructor::Vec(element)) =
                            layouts.get(at.ty()).ok_or(Reason::Entry)?.kind
                        else {
                            return Err(Reason::Entry);
                        };
                        let mut vector = vector::Vector::from_token(
                            layouts,
                            element,
                            &mut token.bytes,
                            allocator,
                        )
                        .map_err(vector_fault)?;
                        let value = temporaries[temporary].as_mut().ok_or(Reason::Invariant)?;
                        vector
                            .push_from(&mut value.value, Some(span), allocator)
                            .map_err(vector_fault)?;
                        value.initialized = false;
                        value.value.release_storage(allocator)?;
                        temporaries[temporary] = None;
                        free.push(temporary);
                        tasks.push(Task::List(parent, at, list, index + 1));
                    }
                }
            }
            Ok(())
        })();
        if staged.is_err() {
            for mut temporary in temporaries.into_iter().rev().flatten() {
                if temporary.initialized {
                    let mut cleanup = c::Contextual::new(
                        |value: &mut &mut l::Reservation<'_, '_>,
                         _,
                         context: &mut c::Context<'_>| {
                            value
                                .retire(allocator, context, |token, allocator, _, context| {
                                    if token.bytes == [0; 32] {
                                        return Ok(());
                                    }
                                    if let Some(drain) = drain.as_mut() {
                                        drain.token(
                                            token,
                                            None,
                                            allocator,
                                            context,
                                            |token, allocator, _, _| {
                                                unpublished(layouts.types(), token, allocator)
                                            },
                                        )
                                    } else {
                                        unpublished(layouts.types(), token, allocator)
                                    }
                                })
                                .map_err(|_| 4u32)
                        },
                    );
                    let _ = session.retire(
                        c::Slot(slot),
                        &mut temporary.value,
                        Some(span),
                        c::Reason::Failure,
                        &mut cleanup,
                    );
                }
                temporary.value.release_storage(allocator)?;
            }
        }
        staged
    }
}
