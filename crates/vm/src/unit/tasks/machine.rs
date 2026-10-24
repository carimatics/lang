use super::*;
use frame::Scope;
use lang_runtime::task::runner::{self as runner, machine};
pub(super) struct Interpreter;
impl<P: a::Provider> runner::Backend<P> for Interpreter {
    fn services_supported(&self) -> bool {
        true
    }
    fn supported(&self, op: &b::Op) -> bool {
        instructions::supported(op)
    }
    fn resume(
        &mut self,
        _proof: &o::tasks::VerifiedTasks,
        machine: &mut machine::Machine<'_, '_, '_, '_, '_, P>,
        task: usize,
    ) -> Result<(), Reason> {
        loop {
            let frame = machine.frame(task)?;
            let function = frame.function;
            let node = machine.proof.plan().functions()[function.0 as usize].nodes()
                [frame.node.0 as usize];
            let next = match node.kind {
                f::Kind::Entry { next, .. } => Some(next),
                f::Kind::Fuel { next, failure, .. } => match machine.charge() {
                    Ok(()) => Some(next),
                    Err(reason) => {
                        machine.fault(task, reason.into(), node.span)?;
                        Some(failure)
                    }
                },
                f::Kind::Execute {
                    site,
                    next,
                    failure,
                } => {
                    let i = &machine.proof.typed().functions()[function.0 as usize].blocks
                        [site.block.0 as usize]
                        .instructions[site.order - 1];
                    let output = i
                        .results
                        .first()
                        .map(|r| machine.frame(task).map(|f| f.value(*r)))
                        .transpose()?;
                    let applied = (|| -> Result<bool, Reason> {
                        match &i.op {
                            b::Op::ChannelSend { authority, payload }
                            | b::Op::ServiceSend { authority, payload } => {
                                let payload = machine.frame(task)?.value(*payload);
                                return machine.channel_send(
                                    task,
                                    authority.0 as usize,
                                    payload,
                                    output.ok_or(Reason::Invariant)?,
                                    next,
                                    failure,
                                    node.span,
                                );
                            }
                            b::Op::ChannelReceive { authority }
                            | b::Op::ServiceReceive { authority } => {
                                return machine.channel_receive(
                                    task,
                                    authority.0 as usize,
                                    output.ok_or(Reason::Invariant)?,
                                    next,
                                    failure,
                                    node.span,
                                );
                            }
                            b::Op::Import { import, arguments } => machine.import_native(
                                task,
                                *import,
                                arguments,
                                output.ok_or(Reason::Invariant)?,
                                node.span,
                            )?,
                            b::Op::Call {
                                function,
                                arguments,
                            } => {
                                machine.call(
                                    task,
                                    *function,
                                    arguments,
                                    output.ok_or(Reason::Invariant)?,
                                    next,
                                    failure,
                                )?;
                                return Ok(false);
                            }
                            b::Op::TaskSpawn {
                                function,
                                arguments,
                            } => machine.spawn(
                                task,
                                *function,
                                arguments,
                                output.ok_or(Reason::Invariant)?,
                                node.span,
                            )?,
                            b::Op::TaskAwait(value) => {
                                let frame = machine.frame_mut(task)?;
                                let slot = frame.value(*value);
                                frame.available(slot)?;
                                let child = frame.claims[slot].take().ok_or(Reason::Invariant)?;
                                frame.live[slot] = false;
                                frame.pending = Some(Pending::Await {
                                    child: child.index(),
                                    destination: output.ok_or(Reason::Invariant)?,
                                    next,
                                    failure,
                                });
                                machine.event(
                                    task,
                                    EventKind::Await {
                                        child: child.index(),
                                    },
                                    node.span,
                                )?;
                                machine
                                    .services
                                    .scheduler
                                    .join(&machine.records[task].id, &child)?;
                                machine.active = None;
                                return Ok(false);
                            }
                            b::Op::TaskCancel(place) => {
                                let place = &machine.proof.typed().places().functions()
                                    [function.0 as usize]
                                    .places[place.0 as usize];
                                let frame = machine.frame(task)?;
                                let location = frame.place(place, machine.allocator)?;
                                let child = frame.claims[location.slot]
                                    .as_ref()
                                    .ok_or(Reason::Invariant)?
                                    .clone();
                                machine.services.scheduler.request_cancel(&child)?;
                            }
                            b::Op::TaskYield => {
                                machine.event(task, EventKind::Yield, node.span)?;
                                machine.frame_mut(task)?.node = next;
                                machine
                                    .services
                                    .scheduler
                                    .yield_now(&machine.records[task].id)?;
                                machine.active = None;
                                return Ok(false);
                            }
                            b::Op::TaskCheck => {
                                machine.event(task, EventKind::Check, node.span)?;
                                machine.frame_mut(task)?.node = next;
                                if machine
                                    .services
                                    .scheduler
                                    .cancellation_point(&machine.records[task].id)?
                                    == t::Boundary::Closing
                                {
                                    machine.active = None;
                                    return Ok(false);
                                }
                            }
                            b::Op::TaskScopeEnter(id) => {
                                let frame = machine.frame_mut(task)?;
                                frame.serial =
                                    frame.serial.checked_add(1).ok_or(Reason::Invariant)?;
                                frame.scopes.push(Scope {
                                    id: *id,
                                    serial: frame.serial,
                                    results: None,
                                });
                                machine.event(task, EventKind::ScopeEnter(id.0), node.span)?;
                            }
                            b::Op::TaskScopeExit(id) => {
                                let scope = machine
                                    .frame(task)?
                                    .scopes
                                    .last()
                                    .copied()
                                    .ok_or(Reason::Invariant)?;
                                if scope.id != *id {
                                    return Err(Reason::Invariant);
                                }
                                machine.event(task, EventKind::ScopeExit(id.0), node.span)?;
                                machine.frame_mut(task)?.pending = Some(Pending::Scope {
                                    scope,
                                    cursor: 0,
                                    next,
                                    failure,
                                });
                                return machine.pending(task);
                            }
                            b::Op::TaskSaveReturn(value) => {
                                let frame = machine.records[task]
                                    .frames
                                    .last_mut()
                                    .ok_or(Reason::Invariant)?;
                                frame.transfer(
                                    frame.value(*value),
                                    frame.return_cell,
                                    machine.allocator,
                                )?;
                                frame.saved = true;
                            }
                            _ => {
                                if let b::Op::CoreCall(call) = &i.op
                                    && matches!(
                                        call.operation,
                                        o::core::Operation::ChannelNew
                                            | o::core::Operation::ServiceNew
                                    )
                                {
                                    let capacity = machine.frame(task)?.value(call.arguments[1]);
                                    if call.operation == o::core::Operation::ServiceNew {
                                        machine.admit_service_create(task, capacity)?;
                                    } else {
                                        machine.admit_channel_create(task, capacity)?;
                                    }
                                }
                                let mut frame = frame::Access::new(
                                    &mut machine.records[task].frames,
                                    &mut machine.services,
                                )?;
                                instructions::execute(
                                    machine.proof.typed(),
                                    &mut frame,
                                    site,
                                    machine.allocator,
                                )?;
                                machine.channel_usage()?;
                            }
                        }
                        Ok(true)
                    })();
                    match applied {
                        Ok(true) => Some(next),
                        Ok(false) => {
                            if machine.active.is_none() {
                                return Ok(());
                            }
                            continue;
                        }
                        Err(reason) => {
                            machine.fault(task, reason, node.span)?;
                            Some(failure)
                        }
                    }
                }
                f::Kind::Transfer {
                    block, arm, target, ..
                } => {
                    let body = &machine.proof.typed().functions()[function.0 as usize];
                    let edge = body.blocks[block.0 as usize]
                        .terminator
                        .kind
                        .targets()
                        .nth(arm as usize)
                        .ok_or(Reason::Invariant)?;
                    let parameters = &body.blocks[edge.block.0 as usize].parameters;
                    let plan = machine.plans[function.0 as usize]
                        .as_ref()
                        .ok_or(Reason::Invariant)?;
                    let frame = machine.records[task]
                        .frames
                        .last_mut()
                        .ok_or(Reason::Invariant)?;
                    for (argument, parameter) in edge.arguments.iter().zip(parameters) {
                        frame.transfer(
                            frame.value(*argument),
                            plan.scratch_cell(*parameter).ok_or(Reason::Invariant)?,
                            machine.allocator,
                        )?;
                    }
                    for parameter in parameters {
                        frame.transfer(
                            plan.scratch_cell(*parameter).ok_or(Reason::Invariant)?,
                            frame.value(*parameter),
                            machine.allocator,
                        )?;
                    }
                    Some(target)
                }
                f::Kind::Branch {
                    condition,
                    then_node,
                    else_node,
                    ..
                } => {
                    let value = machine
                        .frame(task)?
                        .read(machine.frame(task)?.value(condition), machine.allocator)?;
                    match value {
                        l::Immediate::Bool(true) => Some(then_node),
                        l::Immediate::Bool(false) => Some(else_node),
                        _ => return Err(Reason::Invariant),
                    }
                }
                f::Kind::StageReturn { value, next, .. } => {
                    let frame = machine.records[task]
                        .frames
                        .last_mut()
                        .ok_or(Reason::Invariant)?;
                    frame.transfer(frame.value(value), frame.return_cell, machine.allocator)?;
                    frame.saved = true;
                    Some(next)
                }
                f::Kind::DiscardPrepared { next } => {
                    machine.frame_mut(task)?.prepared.fill(None);
                    frame::Access::new(&mut machine.records[task].frames, &mut machine.services)?
                        .discard_channels(machine.allocator)?;
                    Some(next)
                }
                f::Kind::RetireReturn { next, .. } => Some(next),
                f::Kind::PublishReturn => {
                    if !machine.publish_sync(task)? {
                        return Ok(());
                    }
                    continue;
                }
                f::Kind::Raise { failure, next } => {
                    machine.fault(task, Reason::Execution(failure), node.span)?;
                    Some(next)
                }
                f::Kind::RecordFailure { next } => {
                    if machine.frame(task)?.fault.is_none() {
                        machine.fault(task, Reason::Invariant, node.span)?;
                    }
                    Some(next)
                }
                f::Kind::RetireFailure { next } => Some(next),
                f::Kind::DiscardResult { next } => {
                    let frame = machine.frame_mut(task)?;
                    // Retain the obligation until mandatory retirement after joins.
                    frame.saved = false;
                    Some(next)
                }
                f::Kind::EndTaskLoans { next } => {
                    machine.end_loans(task)?;
                    Some(next)
                }
                f::Kind::CancelTaskScopes { next } => {
                    machine.frame_mut(task)?.node = next;
                    machine.frame_mut(task)?.closing = true;
                    let failure = machine.frame(task)?.fault.ok_or(Reason::Invariant)?;
                    machine.services.scheduler.finish(
                        &machine.records[task].id,
                        t::Outcome::Failed(t::FailureId(failure as u64)),
                    )?;
                    machine.active = None;
                    return Ok(());
                }
                f::Kind::PropagateFailure => {
                    if !machine.propagate_sync(task)? {
                        return Ok(());
                    }
                    continue;
                }
                f::Kind::JoinTaskScopes { .. } | f::Kind::FinishTask => {
                    return Err(Reason::Invariant);
                }
            };
            if let Some(next) = next {
                machine.frame_mut(task)?.node = next;
            }
        }
    }

    fn cleanup(
        &mut self,
        machine: &mut machine::Machine<'_, '_, '_, '_, '_, P>,
        task: usize,
    ) -> Result<(), Reason> {
        let function = machine.frame(task)?.function;
        let mut at = machine.frame(task)?.node;
        loop {
            let node = machine.proof.plan().functions()[function.0 as usize].nodes()[at.0 as usize];
            at = match node.kind {
                f::Kind::DiscardPrepared { next } => {
                    machine.frame_mut(task)?.prepared.fill(None);
                    frame::Access::new(&mut machine.records[task].frames, &mut machine.services)?
                        .discard_channels(machine.allocator)?;
                    next
                }
                f::Kind::EndTaskLoans { next } => {
                    machine.end_loans(task)?;
                    next
                }
                f::Kind::CancelTaskScopes { next } => {
                    for child in &machine.records {
                        if child.parent == Some(task) {
                            machine.services.scheduler.request_cancel(&child.id)?;
                        }
                    }
                    next
                }
                f::Kind::JoinTaskScopes { next } => {
                    for child in &machine.records {
                        if child.parent == Some(task)
                            && !matches!(
                                machine.services.scheduler.status(&child.id)?,
                                t::Status::Complete(_)
                            )
                        {
                            return Err(Reason::Invariant);
                        }
                    }
                    machine
                        .retire_children_results(task, lang_runtime::cleanup::Reason::Failure)?;
                    next
                }
                f::Kind::RetireFailure { next } => next,
                f::Kind::DiscardResult { next } => {
                    let frame = machine.frame_mut(task)?;
                    frame.saved = false;
                    // Retain the obligation until mandatory retirement after joins.
                    next
                }
                f::Kind::FinishTask => break,
                _ => return Err(Reason::Invariant),
            };
        }
        Ok(())
    }
}
