//! Typed storage and scheduling services for compiled continuations. These
//! operations never evaluate a MIR instruction, arithmetic expression or branch.
use super::*;
use frame::{Location, Scope};
use machine::Machine;

#[derive(Clone, Copy, Debug)]
pub enum Scalar {
    I64,
    Bool,
    Unit,
}
#[derive(Debug)]
pub enum Primitive {
    Nop,
    Write {
        to: usize,
        kind: Scalar,
    },
    Copy {
        from: usize,
        to: usize,
    },
    Load {
        place: p::PlaceId,
        to: usize,
    },
    Prepare {
        place: p::PlaceId,
        destination: usize,
    },
    Replace {
        place: p::PlaceId,
        from: usize,
    },
    ReplacePrepared {
        destination: usize,
        from: usize,
    },
    DiscardPrepared(usize),
    Forget(Vec<usize>),
    Record {
        to: usize,
        fields: Vec<usize>,
        order: Vec<(u32, m::Span)>,
    },
    Variant {
        to: usize,
        case: u32,
        payload: Vec<usize>,
    },
    Field {
        from: usize,
        case: u32,
        field: u32,
        to: usize,
    },
    Fields {
        from: usize,
        case: u32,
        to: Vec<usize>,
    },
    Call {
        function: m::FunctionId,
        arguments: Vec<m::Register>,
        to: usize,
        next: f::NodeId,
        failure: f::NodeId,
    },
    Spawn {
        function: m::FunctionId,
        arguments: Vec<m::Register>,
        to: usize,
    },
    Await {
        from: usize,
        to: usize,
        next: f::NodeId,
        failure: f::NodeId,
    },
    Cancel(p::PlaceId),
    Yield(f::NodeId),
    Check(f::NodeId),
    Enter(b::TaskScopeId),
    Exit {
        scope: b::TaskScopeId,
        next: f::NodeId,
        failure: f::NodeId,
    },
    Save(usize),
    Transfer(Vec<(usize, usize, usize)>),
    Publish,
    Propagate,
    RecordFailure,
    DiscardAllPrepared,
    DiscardResult,
    Close(f::NodeId),
    Children,
    Finish,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flow {
    Continue,
    Switch,
    Suspended,
    Finished,
}

impl<P: a::Provider> Machine<'_, '_, '_, '_, '_, P> {
    pub fn apply(
        &mut self,
        task: usize,
        primitive: &Primitive,
        bits: u64,
        span: m::Span,
    ) -> Result<Flow, Reason> {
        let function = self.frame(task)?.function;
        let frame = self.records[task]
            .frames
            .last_mut()
            .ok_or(Reason::Invariant)?;
        let paths = &self.proof.typed().places().functions()[function.0 as usize].places;
        match primitive {
            Primitive::Nop => {}
            Primitive::Write { to, kind } => frame.write(
                *to,
                match kind {
                    Scalar::I64 => l::Immediate::I64(bits as i64),
                    Scalar::Bool => match bits {
                        0 => l::Immediate::Bool(false),
                        1 => l::Immediate::Bool(true),
                        _ => return Err(Reason::Invariant),
                    },
                    Scalar::Unit => l::Immediate::Unit,
                },
                self.allocator,
            )?,
            Primitive::Copy { from, to } => frame.transfer(*from, *to, self.allocator)?,
            Primitive::Load { place, to } => {
                let from = frame.place(&paths[place.0 as usize], self.allocator)?;
                if frame.is_claim(from.slot)? {
                    frame.transfer(from.slot, *to, self.allocator)?;
                } else {
                    frame.copy_location(from, frame.root(*to)?, self.allocator)?;
                }
            }
            Primitive::Prepare { place, destination } => {
                frame.prepared[*destination] =
                    Some(frame.place(&paths[place.0 as usize], self.allocator)?)
            }
            Primitive::Replace { place, from } => {
                let to = frame.place(&paths[place.0 as usize], self.allocator)?;
                if frame.is_claim(to.slot)? {
                    frame.transfer(*from, to.slot, self.allocator)?;
                } else {
                    frame.copy_location(frame.root(*from)?, to, self.allocator)?;
                }
            }
            Primitive::ReplacePrepared { destination, from } => {
                let to = frame.prepared[*destination]
                    .take()
                    .ok_or(Reason::Invariant)?;
                if frame.is_claim(to.slot)? {
                    frame.transfer(*from, to.slot, self.allocator)?;
                } else {
                    frame.copy_location(frame.root(*from)?, to, self.allocator)?;
                }
            }
            Primitive::DiscardPrepared(destination) => frame.prepared[*destination] = None,
            Primitive::Forget(slots) => {
                for slot in slots {
                    frame.forget(*slot);
                }
            }
            Primitive::Record { to, fields, order } => {
                let root = frame.root(*to)?;
                frame.data()?.cell(*to)?.initialize_order(
                    root.at,
                    order.iter().copied(),
                    self.allocator,
                )?;
                for (index, from) in fields.iter().enumerate() {
                    let at =
                        frame
                            .data()?
                            .cell(*to)?
                            .child(root.at, index as u32, self.allocator)?;
                    frame.copy_location(
                        frame.root(*from)?,
                        Location { slot: *to, at },
                        self.allocator,
                    )?;
                }
                frame.live[*to] = true;
            }
            Primitive::Variant { to, case, payload } => {
                let root = frame.root(*to)?;
                let builtin = match root.at.ty() {
                    o::Type::Constructed(id) => match self
                        .proof
                        .typed()
                        .places()
                        .declarations()
                        .types()
                        .table()
                        .constructed[id.0 as usize]
                        .constructor
                    {
                        o::Constructor::Builtin(builtin) => Some(builtin),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(builtin) = builtin {
                    let payload = payload
                        .first()
                        .map(|from| frame.read(*from, self.allocator))
                        .transpose()?;
                    frame.write(
                        *to,
                        l::Immediate::from_case(builtin, *case, payload)?,
                        self.allocator,
                    )?;
                } else {
                    frame
                        .data()?
                        .cell(*to)?
                        .prepare_case(root.at, *case, self.allocator)?;
                    for (index, from) in payload.iter().enumerate() {
                        let at = frame.data()?.cell(*to)?.child(
                            root.at,
                            index as u32,
                            self.allocator,
                        )?;
                        frame.copy_location(
                            frame.root(*from)?,
                            Location { slot: *to, at },
                            self.allocator,
                        )?;
                    }
                    frame.live[*to] = true;
                }
            }
            Primitive::Field {
                from,
                case,
                field,
                to,
            } => copy_field(frame, *from, *case, *field, *to, self.allocator)?,
            Primitive::Fields { from, case, to } => {
                for (field, to) in to.iter().enumerate() {
                    copy_field(frame, *from, *case, field as u32, *to, self.allocator)?;
                }
            }
            Primitive::Call {
                function,
                arguments,
                to,
                next,
                failure,
            } => {
                self.call(task, *function, arguments, *to, *next, *failure)?;
                return Ok(Flow::Switch);
            }
            Primitive::Spawn {
                function,
                arguments,
                to,
            } => self.spawn(task, *function, arguments, *to, span)?,
            Primitive::Await {
                from,
                to,
                next,
                failure,
            } => {
                frame.available(*from)?;
                let child = frame.claims[*from].take().ok_or(Reason::Invariant)?;
                frame.live[*from] = false;
                frame.pending = Some(Pending::Await {
                    child: child.index(),
                    destination: *to,
                    next: *next,
                    failure: *failure,
                });
                self.event(
                    task,
                    EventKind::Await {
                        child: child.index(),
                    },
                    span,
                )?;
                self.scheduler.join(&self.records[task].id, &child)?;
                self.active = None;
                return Ok(Flow::Suspended);
            }
            Primitive::Cancel(place) => {
                let location = frame.place(&paths[place.0 as usize], self.allocator)?;
                let child = frame.claims[location.slot]
                    .as_ref()
                    .ok_or(Reason::Invariant)?
                    .clone();
                self.scheduler.request_cancel(&child)?;
            }
            Primitive::Yield(next) => {
                frame.node = *next;
                self.event(task, EventKind::Yield, span)?;
                self.scheduler.yield_now(&self.records[task].id)?;
                self.active = None;
                return Ok(Flow::Suspended);
            }
            Primitive::Check(next) => {
                frame.node = *next;
                self.event(task, EventKind::Check, span)?;
                if self.scheduler.cancellation_point(&self.records[task].id)?
                    == t::Boundary::Closing
                {
                    self.active = None;
                    return Ok(Flow::Suspended);
                }
            }
            Primitive::Enter(id) => {
                frame.serial = frame.serial.checked_add(1).ok_or(Reason::Invariant)?;
                frame.scopes.push(Scope {
                    id: *id,
                    serial: frame.serial,
                    results: None,
                });
                self.event(task, EventKind::ScopeEnter(id.0), span)?;
            }
            Primitive::Exit {
                scope: id,
                next,
                failure,
            } => {
                let scope = frame.scopes.last().copied().ok_or(Reason::Invariant)?;
                if scope.id != *id {
                    return Err(Reason::Invariant);
                }
                frame.pending = Some(Pending::Scope {
                    scope,
                    cursor: 0,
                    next: *next,
                    failure: *failure,
                });
                self.event(task, EventKind::ScopeExit(id.0), span)?;
                return Ok(if self.pending(task)? {
                    Flow::Switch
                } else {
                    Flow::Suspended
                });
            }
            Primitive::Save(from) => {
                frame.transfer(*from, frame.return_cell, self.allocator)?;
                frame.saved = true;
            }
            Primitive::Transfer(slots) => {
                for (from, scratch, _) in slots {
                    frame.transfer(*from, *scratch, self.allocator)?;
                }
                for (_, scratch, to) in slots {
                    frame.transfer(*scratch, *to, self.allocator)?;
                }
            }
            Primitive::Publish => {
                if self.records[task].frames.len() == 1 {
                    self.scheduler
                        .finish(&self.records[task].id, t::Outcome::Success)?;
                    self.active = None;
                    return Ok(Flow::Suspended);
                }
                let mut child = self.records[task].frames.pop().ok_or(Reason::Invariant)?;
                let returned = child.returned.ok_or(Reason::Invariant)?;
                let parent = self.records[task]
                    .frames
                    .last_mut()
                    .ok_or(Reason::Invariant)?;
                parent.copy_from_frame(
                    returned.destination,
                    &child,
                    child.return_cell,
                    self.allocator,
                )?;
                parent.node = returned.next;
                child.release(self.allocator)?;
                self.retired(child.function);
                return Ok(Flow::Switch);
            }
            Primitive::Propagate => {
                let failure = frame.fault.ok_or(Reason::Invariant)?;
                if self.records[task].frames.len() == 1 {
                    self.scheduler.finish(
                        &self.records[task].id,
                        t::Outcome::Failed(t::FailureId(failure as u64)),
                    )?;
                    self.active = None;
                    return Ok(Flow::Suspended);
                }
                let mut child = self.records[task].frames.pop().ok_or(Reason::Invariant)?;
                let returned = child.returned.ok_or(Reason::Invariant)?;
                child.release(self.allocator)?;
                self.retired(child.function);
                let parent = self.frame_mut(task)?;
                parent.fault = Some(failure);
                parent.node = returned.failure;
                return Ok(Flow::Switch);
            }
            Primitive::RecordFailure => {
                if frame.fault.is_none() {
                    self.fault(task, Reason::Invariant, span)?;
                }
            }
            Primitive::DiscardAllPrepared => frame.prepared.fill(None),
            Primitive::DiscardResult => {
                frame.live[frame.return_cell] = false;
                frame.saved = false;
            }
            Primitive::Close(next) => {
                if self.active.as_ref().is_some_and(|(_, cleaning)| *cleaning) {
                    for child in &self.records {
                        if child.parent == Some(task) {
                            self.scheduler.request_cancel(&child.id)?;
                        }
                    }
                } else {
                    frame.node = *next;
                    frame.closing = true;
                    let failure = frame.fault.ok_or(Reason::Invariant)?;
                    self.scheduler.finish(
                        &self.records[task].id,
                        t::Outcome::Failed(t::FailureId(failure as u64)),
                    )?;
                    self.active = None;
                    return Ok(Flow::Suspended);
                }
            }
            Primitive::Children => {
                for child in &self.records {
                    if child.parent == Some(task)
                        && !matches!(self.scheduler.status(&child.id)?, t::Status::Complete(_))
                    {
                        return Err(Reason::Invariant);
                    }
                }
                self.retire_children_results(task)?;
            }
            Primitive::Finish => return Ok(Flow::Finished),
        }
        Ok(Flow::Continue)
    }
}

fn copy_field<P: a::Provider>(
    frame: &mut Frame<'_, '_>,
    from: usize,
    case: u32,
    field: u32,
    to: usize,
    allocator: &mut a::Allocator<P>,
) -> Result<(), Reason> {
    frame.available(from)?;
    let source = frame.data()?.cell(from)?;
    let root = source.root();
    if source.case(root, allocator)? != case {
        return Err(Reason::Invariant);
    }
    if matches!(
        source
            .layouts()
            .get(root.ty())
            .ok_or(Reason::Invariant)?
            .kind,
        l::Kind::Error { .. }
    ) {
        let payload = source
            .read_immediate(root, allocator)?
            .payload()
            .ok_or(Reason::Invariant)?;
        if field != 0 {
            return Err(Reason::Invariant);
        }
        frame.write(to, payload, allocator)
    } else {
        let at = source.child(root, field, allocator)?;
        frame.copy_location(Location { slot: from, at }, frame.root(to)?, allocator)
    }
}
