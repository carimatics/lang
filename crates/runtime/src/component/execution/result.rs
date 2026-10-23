//! Read admitted results into bounded flat caller metadata. The original guest
//! table retains resource custody until publication; ReadView keeps root and
//! vector element access on the same validated allocator/layout path.
use super::*;
use crate::{allocation as a, buffer, vector};

pub(crate) fn read_parameters<'l, 't, P: a::Provider>(
    roots: &[ReadRoot<'l, 't>],
    layouts: &'l l::Layouts<'t>,
    types: &o::VerifiedTypes,
    allocator: &a::Allocator<P>,
    session: &mut wire::Session,
) -> Result<NativeArguments, Reason> {
    let mut values = wire::Values {
        roots: session
            .vector(roots.len(), 0)
            .map_err(Reason::ComponentFrame)?,
        nodes: Vec::new(),
    };
    let mut resources = session
        .vector(roots.len(), 0)
        .map_err(Reason::ComponentFrame)?;
    for root in roots {
        session.charge(1, 0).map_err(Reason::ComponentFrame)?;
        let id = match root {
            ReadRoot::Resource(resource) => {
                session
                    .push(&mut resources, *resource, 0)
                    .map_err(Reason::ComponentFrame)?;
                let index = u32::try_from(resources.len())
                    .ok()
                    .and_then(crate::component::adapter::BoundaryResourceIndex::new)
                    .ok_or(Reason::Invariant)?;
                let id = wire::ValueId(values.nodes.len());
                session
                    .push(&mut values.nodes, wire::Value::Resource(index), 0)
                    .map_err(Reason::ComponentFrame)?;
                id
            }
            ReadRoot::Value(root) => {
                let frame = read_value(*root, layouts, types, allocator, session)?;
                let resource_base = resources.len();
                for claim in frame.resources {
                    session
                        .push(
                            &mut resources,
                            ResourceArgument {
                                claim,
                                parent: None,
                            },
                            0,
                        )
                        .map_err(Reason::ComponentFrame)?;
                }
                let base = values.nodes.len();
                let id = wire::ValueId(
                    base.checked_add(frame.values.roots[0].0)
                        .ok_or(Reason::Invariant)?,
                );
                for mut node in frame.values.nodes {
                    let mut rebase = |id: &mut wire::ValueId| -> Result<(), Reason> {
                        session.charge(1, 0).map_err(Reason::ComponentFrame)?;
                        id.0 = id.0.checked_add(base).ok_or(Reason::Invariant)?;
                        Ok(())
                    };
                    match &mut node {
                        wire::Value::Resource(index) => {
                            *index = u32::try_from(resource_base)
                                .ok()
                                .and_then(|base| base.checked_add(index.get()))
                                .and_then(crate::component::adapter::BoundaryResourceIndex::new)
                                .ok_or(Reason::Invariant)?;
                        }
                        wire::Value::Record(fields) | wire::Value::List(fields) => {
                            for id in fields {
                                rebase(id)?;
                            }
                        }
                        wire::Value::Variant { payload, .. }
                        | wire::Value::Some(payload)
                        | wire::Value::Ok(payload)
                        | wire::Value::Err(payload) => rebase(payload)?,
                        _ => {}
                    }
                    session
                        .push(&mut values.nodes, node, 0)
                        .map_err(Reason::ComponentFrame)?;
                }
                id
            }
        };
        values.roots.push(id);
    }
    Ok(NativeArguments {
        values,
        resources,
        consumed: Vec::new(),
        cleanup: None,
    })
}
enum Aggregate {
    Record,
    List,
    Variant(u32),
    Some,
    Ok,
    Err,
}
enum Work<'l, 't> {
    Read(l::ReadView<'l, 't>, l::Location),
    Build(Aggregate, usize),
}
fn fault(error: buffer::Fault) -> Reason {
    match error {
        buffer::Fault::Allocator(error) => Reason::Layout(l::Error::Allocation(error)),
        _ => Reason::Invariant,
    }
}
pub(crate) fn read_result<P: a::Provider>(
    value: &l::Reservation<'_, '_>,
    types: &o::VerifiedTypes,
    allocator: &a::Allocator<P>,
    session: &mut wire::Session,
) -> Result<FrameResult, Reason> {
    read_value(value.observe()?, value.layouts(), types, allocator, session)
}
fn read_value<'l, 't, P: a::Provider>(
    root: l::ReadView<'l, 't>,
    layouts: &'l l::Layouts<'t>,
    types: &o::VerifiedTypes,
    allocator: &a::Allocator<P>,
    session: &mut wire::Session,
) -> Result<FrameResult, Reason> {
    let mut work = Vec::new();
    let mut stack = Vec::new();
    let mut nodes = Vec::new();
    let mut resources = Vec::new();
    session
        .push(&mut work, Work::Read(root, root.root()), 0)
        .map_err(Reason::ComponentFrame)?;
    while let Some(next) = work.pop() {
        session.charge(1, 0).map_err(Reason::ComponentFrame)?;
        let node = match next {
            Work::Build(kind, count) => {
                let start = stack.len().checked_sub(count).ok_or(Reason::Invariant)?;
                match kind {
                    Aggregate::Record | Aggregate::List => {
                        let mut fields =
                            session.vector(count, 0).map_err(Reason::ComponentFrame)?;
                        fields.extend(stack.drain(start..));
                        if matches!(kind, Aggregate::List) {
                            wire::Value::List(fields)
                        } else {
                            wire::Value::Record(fields)
                        }
                    }
                    Aggregate::Variant(case) => wire::Value::Variant {
                        case,
                        payload: stack.pop().ok_or(Reason::Invariant)?,
                    },
                    Aggregate::Some => wire::Value::Some(stack.pop().ok_or(Reason::Invariant)?),
                    Aggregate::Ok => wire::Value::Ok(stack.pop().ok_or(Reason::Invariant)?),
                    Aggregate::Err => wire::Value::Err(stack.pop().ok_or(Reason::Invariant)?),
                }
            }
            Work::Read(view, at) => {
                let layout = layouts.get(at.ty()).ok_or(Reason::Entry)?;
                match &layout.kind {
                    l::Kind::Scalar | l::Kind::Error { .. } => {
                        match view.immediate(at, allocator)? {
                            l::Immediate::I64(value) => wire::Value::I64(value),
                            l::Immediate::Bool(value) => wire::Value::Bool(value),
                            l::Immediate::Unit => wire::Value::Unit,
                            _ => return Err(Reason::Unsupported),
                        }
                    }
                    l::Kind::Token(o::Constructor::ComponentResource(_)) => {
                        let token = view.token(at, allocator)?;
                        let claim = Claim::from_token(token.bytes).map_err(super::fault)?;
                        session
                            .push(&mut resources, claim, 0)
                            .map_err(Reason::ComponentFrame)?;
                        let index = u32::try_from(resources.len())
                            .ok()
                            .and_then(crate::component::adapter::BoundaryResourceIndex::new)
                            .ok_or(Reason::Entry)?;
                        wire::Value::Resource(index)
                    }
                    l::Kind::Token(o::Constructor::Builtin(o::Builtin::Bytes)) => {
                        let mut token = view.token(at, allocator)?;
                        let bytes = buffer::Bytes::from_token(&mut token.bytes, allocator)
                            .map_err(fault)?;
                        let bytes = bytes.as_slice(allocator).map_err(fault)?;
                        session
                            .value_bytes(bytes.len(), 0)
                            .map_err(Reason::ComponentFrame)?;
                        session
                            .charge(bytes.len(), 0)
                            .map_err(Reason::ComponentFrame)?;
                        let mut copy = session
                            .vector(bytes.len(), 0)
                            .map_err(Reason::ComponentFrame)?;
                        copy.extend_from_slice(bytes);
                        wire::Value::Bytes(copy)
                    }
                    l::Kind::Token(o::Constructor::Builtin(o::Builtin::Text)) => {
                        let token = view.token(at, allocator)?;
                        let bytes =
                            buffer::text_token_bytes(token.bytes, allocator).map_err(fault)?;
                        session
                            .text_bytes(bytes.len(), 0)
                            .map_err(Reason::ComponentFrame)?;
                        session
                            .charge(bytes.len(), 0)
                            .map_err(Reason::ComponentFrame)?;
                        let mut copy = session
                            .vector(bytes.len(), 0)
                            .map_err(Reason::ComponentFrame)?;
                        copy.extend_from_slice(bytes);
                        wire::Value::Text(String::from_utf8(copy).map_err(|_| Reason::Invariant)?)
                    }
                    l::Kind::Token(o::Constructor::Vec(element)) => {
                        let mut token = view.token(at, allocator)?;
                        let vector = vector::Vector::from_token(
                            layouts,
                            *element,
                            &mut token.bytes,
                            allocator,
                        )
                        .map_err(|_| Reason::Invariant)?;
                        let count =
                            usize::try_from(vector.len(allocator).map_err(|_| Reason::Invariant)?)
                                .map_err(|_| Reason::Invariant)?;
                        session.count(count, 0).map_err(Reason::ComponentFrame)?;
                        session.charge(count, 0).map_err(Reason::ComponentFrame)?;
                        session
                            .push(&mut work, Work::Build(Aggregate::List, count), 0)
                            .map_err(Reason::ComponentFrame)?;
                        for index in (0..count).rev() {
                            let (element, _) = vector
                                .observation(index as u64, allocator)
                                .map_err(|_| Reason::Invariant)?;
                            session
                                .push(&mut work, Work::Read(element, element.root()), 0)
                                .map_err(Reason::ComponentFrame)?;
                        }
                        continue;
                    }
                    l::Kind::Record(fields) => {
                        view.initialized_aggregate(at, allocator)?;
                        session
                            .push(
                                &mut work,
                                Work::Build(Aggregate::Record, fields.fields.len()),
                                0,
                            )
                            .map_err(Reason::ComponentFrame)?;
                        for index in (0..fields.fields.len()).rev() {
                            session
                                .push(
                                    &mut work,
                                    Work::Read(view, view.child(at, index as u32, allocator)?),
                                    0,
                                )
                                .map_err(Reason::ComponentFrame)?;
                        }
                        continue;
                    }
                    l::Kind::Enum { cases, .. } => {
                        view.initialized_aggregate(at, allocator)?;
                        let case = view.case(at, allocator)?;
                        let fields = cases.get(case as usize).ok_or(Reason::Entry)?;
                        let constructor = match at.ty() {
                            o::Type::Constructed(id) => {
                                Some(types.table().constructed[id.0 as usize].constructor)
                            }
                            _ => None,
                        };
                        let aggregate = match constructor {
                            Some(o::Constructor::Option(_)) if case == 0 => {
                                let id = wire::ValueId(nodes.len());
                                session
                                    .push(&mut nodes, wire::Value::None, 0)
                                    .map_err(Reason::ComponentFrame)?;
                                session
                                    .push(&mut stack, id, 0)
                                    .map_err(Reason::ComponentFrame)?;
                                continue;
                            }
                            Some(o::Constructor::Option(_)) => Aggregate::Some,
                            Some(o::Constructor::Result(_, _)) if case == 0 => Aggregate::Ok,
                            Some(o::Constructor::Result(_, _)) => Aggregate::Err,
                            _ => Aggregate::Variant(case),
                        };
                        session
                            .push(&mut work, Work::Build(aggregate, 1), 0)
                            .map_err(Reason::ComponentFrame)?;
                        match fields.fields.len() {
                            0 => {
                                let id = wire::ValueId(nodes.len());
                                session
                                    .push(&mut nodes, wire::Value::Unit, 0)
                                    .map_err(Reason::ComponentFrame)?;
                                session
                                    .push(&mut stack, id, 0)
                                    .map_err(Reason::ComponentFrame)?;
                            }
                            1 => session
                                .push(
                                    &mut work,
                                    Work::Read(view, view.child(at, 0, allocator)?),
                                    0,
                                )
                                .map_err(Reason::ComponentFrame)?,
                            _ => return Err(Reason::Unsupported),
                        }
                        continue;
                    }
                    _ => return Err(Reason::Unsupported),
                }
            }
        };
        let id = wire::ValueId(nodes.len());
        session
            .push(&mut nodes, node, 0)
            .map_err(Reason::ComponentFrame)?;
        session
            .push(&mut stack, id, 0)
            .map_err(Reason::ComponentFrame)?;
    }
    if stack.len() != 1 {
        return Err(Reason::Invariant);
    }
    Ok(FrameResult {
        values: wire::Values {
            roots: stack,
            nodes,
        },
        resources,
        bytes: Vec::new(),
    })
}
