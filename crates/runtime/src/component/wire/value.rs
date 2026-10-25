use super::contract::{interface_bytes, lookup, write_resource, write_value_key};
use super::io::{Reader, Writer};
use super::*;
use crate::component::idl::{
    Access, Definition, DefinitionKind, Document, Effect, Field, Function, TypeId, TypeKind,
    ValueKey,
};
use crate::component::negotiation::{InterfaceKey, ResourceTypeKey};
use sha2::{Digest, Sha256};

pub(super) fn definition<'a>(
    d: &'a Document,
    k: &ValueKey,
    s: &mut Session,
) -> Result<&'a Definition, Error> {
    lookup(s, d.interfaces.len(), interface_bytes(&k.interface), 0)?;
    let i = d
        .interfaces
        .binary_search_by(|i| i.key.value.cmp(&k.interface))
        .map_err(|_| s.error(0, ErrorKind::Unresolved))?;
    let defs = &d.interfaces[i].definitions;
    lookup(s, defs.len(), k.name.len(), 0)?;
    let i = defs
        .binary_search_by(|d| d.key.value.name.cmp(&k.name))
        .map_err(|_| s.error(0, ErrorKind::Unresolved))?;
    Ok(&defs[i])
}
fn resolve(d: &Document, mut ty: TypeId, s: &mut Session) -> Result<TypeId, Error> {
    loop {
        s.charge(1, 0)?;
        let TypeKind::Named(k) = &d.type_nodes[ty.0].kind else {
            return Ok(ty);
        };
        let def = definition(d, &k.value, s)?;
        let DefinitionKind::Alias(child) = def.kind else {
            return Ok(ty);
        };
        ty = child;
    }
}
struct Children<'a> {
    small: [Option<TypeId>; 2],
    fields: std::slice::Iter<'a, Field>,
}
impl Iterator for Children<'_> {
    type Item = TypeId;
    fn next(&mut self) -> Option<TypeId> {
        self.small[0]
            .take()
            .or_else(|| self.small[1].take())
            .or_else(|| self.fields.next().map(|f| f.ty))
    }
}
fn children<'a>(d: &'a Document, id: TypeId, s: &mut Session) -> Result<Children<'a>, Error> {
    let mut small = [None, None];
    let mut fields: &[Field] = &[];
    match &d.type_nodes[id.0].kind {
        TypeKind::List(c) | TypeKind::Option(c) => small[0] = Some(*c),
        TypeKind::Result(a, b) => small = [Some(*a), Some(*b)],
        TypeKind::Named(k) => match &definition(d, &k.value, s)?.kind {
            DefinitionKind::Alias(c) => small[0] = Some(*c),
            DefinitionKind::Record(f) | DefinitionKind::Variant(f) => fields = f,
        },
        _ => {}
    }
    Ok(Children {
        small,
        fields: fields.iter(),
    })
}
struct Projection<'a> {
    definitions: Vec<&'a Definition>,
    resources: Vec<&'a ResourceTypeKey>,
    minimum: Vec<Option<usize>>,
}
fn projection<'a>(d: &'a Document, f: &Function, s: &mut Session) -> Result<Projection<'a>, Error> {
    projection_roots(
        d,
        f.params
            .iter()
            .map(|p| p.ty)
            .chain(std::iter::once(f.result)),
        s,
    )
}
fn projection_roots<'a>(
    d: &'a Document,
    roots: impl Iterator<Item = TypeId>,
    s: &mut Session,
) -> Result<Projection<'a>, Error> {
    s.charge(d.type_nodes.len(), 0)?;
    let mut minimum = s.vector(d.type_nodes.len(), 0)?;
    minimum.resize(d.type_nodes.len(), None);
    let mut work = Vec::new();
    let mut definitions = Vec::new();
    let mut resources = Vec::new();
    for ty in roots {
        s.push(&mut work, (ty, false), 0)?;
    }
    while let Some((id, finish)) = work.pop() {
        s.charge(1, 0)?;
        if minimum[id.0].is_some() {
            continue;
        }
        if !finish {
            s.push(&mut work, (id, true), 0)?;
            for c in children(d, id, s)? {
                if minimum[c.0].is_none() {
                    s.push(&mut work, (c, false), 0)?;
                }
            }
            continue;
        }
        // These are lower bounds, not allocation sizes: saturation remains a
        // safe bound for an exponentially large record inside an empty sum/list.
        let size = match &d.type_nodes[id.0].kind {
            TypeKind::Unit => 0,
            TypeKind::I64 => 8,
            TypeKind::Bool => 1,
            TypeKind::Text | TypeKind::Bytes | TypeKind::List(_) => 4,
            TypeKind::Option(_) | TypeKind::Result(_, _) => 1,
            TypeKind::Own(k) => {
                s.push(&mut resources, &k.value, 0)?;
                4
            }
            TypeKind::Borrow { resource: k, .. } => {
                s.push(&mut resources, &k.value, 0)?;
                4
            }
            TypeKind::Named(k) => {
                let def = definition(d, &k.value, s)?;
                match &def.kind {
                    DefinitionKind::Alias(c) => minimum[c.0].expect("postorder"),
                    DefinitionKind::Record(fields) => {
                        s.push(&mut definitions, def, 0)?;
                        s.charge(fields.len(), 0)?;
                        fields.iter().fold(0usize, |n, f| {
                            n.saturating_add(minimum[f.ty.0].expect("postorder"))
                        })
                    }
                    DefinitionKind::Variant(fields) => {
                        s.push(&mut definitions, def, 0)?;
                        s.charge(fields.len(), 0)?;
                        4usize.saturating_add(
                            fields
                                .iter()
                                .map(|f| minimum[f.ty.0].expect("postorder"))
                                .min()
                                .expect("nonempty variant"),
                        )
                    }
                }
            }
        };
        minimum[id.0] = Some(size);
    }
    let max_key = definitions
        .iter()
        .map(|d| interface_bytes(&d.key.value.interface) + d.key.value.name.len() + 1)
        .chain(
            resources
                .iter()
                .map(|r| interface_bytes(r.interface()) + r.resource().len() + 1),
        )
        .max()
        .unwrap_or(0);
    let n = Session::add(definitions.len(), resources.len(), 0)?;
    let log = usize::BITS as usize - n.max(1).leading_zeros() as usize + 1;
    s.charge(
        Session::multiply(
            Session::multiply(n, log, 0)?,
            Session::add(max_key, 1, 0)?,
            0,
        )?,
        0,
    )?;
    definitions.sort_unstable_by(|a, b| a.key.value.cmp(&b.key.value));
    definitions.dedup_by(|a, b| a.key.value == b.key.value);
    resources.sort_unstable();
    resources.dedup();
    Ok(Projection {
        definitions,
        resources,
        minimum,
    })
}
fn schema_type(w: &mut Writer<'_>, d: &Document, root: TypeId) -> Result<(), Error> {
    let mut work = Vec::new();
    w.session.push(&mut work, (root, 1usize), 0)?;
    while let Some((id, depth)) = work.pop() {
        let at = w.at();
        w.session
            .cap(depth, w.session.limits.depth, LimitKind::Depth, at)?;
        let id = resolve(d, id, w.session)?;
        match &d.type_nodes[id.0].kind {
            TypeKind::I64 => w.u8(0)?,
            TypeKind::Bool => w.u8(1)?,
            TypeKind::Unit => w.u8(5)?,
            TypeKind::Text => w.u8(6)?,
            TypeKind::Bytes => w.u8(7)?,
            TypeKind::Own(k) => {
                w.u8(2)?;
                write_resource(w, &k.value)?;
            }
            TypeKind::Borrow { access, resource } => {
                w.u8(if *access == Access::Shared { 3 } else { 4 })?;
                write_resource(w, &resource.value)?;
            }
            TypeKind::Named(k) => {
                w.u8(11)?;
                write_value_key(w, &k.value)?;
            }
            TypeKind::List(c) | TypeKind::Option(c) => {
                w.u8(if matches!(d.type_nodes[id.0].kind, TypeKind::List(_)) {
                    8
                } else {
                    9
                })?;
                w.session.push(&mut work, (*c, depth + 1), at)?;
            }
            TypeKind::Result(a, b) => {
                w.u8(10)?;
                w.session.push(&mut work, (*b, depth + 1), at)?;
                w.session.push(&mut work, (*a, depth + 1), at)?;
            }
        }
    }
    Ok(())
}
fn schema_fields(w: &mut Writer<'_>, d: &Document, fields: &[Field]) -> Result<(), Error> {
    w.count(fields.len())?;
    for f in fields {
        w.string(&f.name.value)?;
        schema_type(w, d, f.ty)?;
    }
    Ok(())
}
fn preimage(
    d: &Document,
    key: &InterfaceKey,
    f: &Function,
    p: &Projection<'_>,
    s: &mut Session,
) -> Result<Vec<u8>, Error> {
    let mut w = Writer::new(s);
    w.raw(b"LCSG")?;
    w.u32(1)?;
    let n = Session::add(
        Session::add(interface_bytes(key), f.name.value.len(), 0)?,
        1,
        0,
    )?;
    w.session
        .cap(n, w.session.limits.string_bytes, LimitKind::StringBytes, 0)?;
    w.u32(u32::try_from(n).map_err(|_| Error {
        offset: 0,
        kind: ErrorKind::InvalidSchema,
    })?)?;
    w.raw(key.package().namespace().as_bytes())?;
    w.raw(b":")?;
    w.raw(key.package().package().as_bytes())?;
    w.raw(b"/")?;
    w.raw(key.interface().as_bytes())?;
    w.raw(b".")?;
    w.raw(f.name.value.as_bytes())?;
    w.u8(u8::from(f.effect.value == Effect::Task))?;
    w.string(&f.capability.value)?;
    w.count(f.params.len())?;
    for param in &f.params {
        schema_type(&mut w, d, param.ty)?;
    }
    schema_type(&mut w, d, f.result)?;
    w.count(p.resources.len())?;
    for k in &p.resources {
        write_resource(&mut w, k)?;
    }
    w.count(p.definitions.len())?;
    for def in &p.definitions {
        write_value_key(&mut w, &def.key.value)?;
        match &def.kind {
            DefinitionKind::Record(f) => {
                w.u8(0)?;
                schema_fields(&mut w, d, f)?;
            }
            DefinitionKind::Variant(f) => {
                w.u8(1)?;
                schema_fields(&mut w, d, f)?;
            }
            DefinitionKind::Alias(_) => unreachable!("aliases resolved before projection"),
        }
    }
    Ok(w.bytes)
}
/// A projection checked against immutable metadata. It is not an invocation grant.
#[derive(Debug)]
pub struct Signature<'a> {
    document: &'a Document,
    function: &'a Function,
    key: &'a InterfaceKey,
    digest: [u8; 32],
    data_only: bool,
    minimum: Vec<Option<usize>>,
    metadata_usage: crate::component::idl::Usage,
}
impl<'a> Signature<'a> {
    pub fn new(
        contract: &'a Contract,
        key: &InterfaceKey,
        member: &str,
        session: &mut Session,
    ) -> Result<Self, Error> {
        session.metadata(|session, limits| {
            super::contract::proof_limits(contract.checked().usage(), limits, session)?;
            let d = contract.document();
            lookup(session, d.interfaces.len(), interface_bytes(key), 0)?;
            let i = d
                .interfaces
                .binary_search_by(|i| i.key.value.cmp(key))
                .map_err(|_| session.error(0, ErrorKind::Unresolved))?;
            let interface = &d.interfaces[i];
            lookup(session, interface.functions.len(), member.len(), 0)?;
            let i = interface
                .functions
                .binary_search_by(|f| f.name.value.as_str().cmp(member))
                .map_err(|_| session.error(0, ErrorKind::Unresolved))?;
            let function = &interface.functions[i];
            let p = projection(d, function, session)?;
            let bytes = preimage(d, &interface.key.value, function, &p, session)?;
            session.charge(bytes.len(), 0)?;
            let digest = Sha256::digest(&bytes).into();
            Ok(Self {
                document: d,
                function,
                key: &interface.key.value,
                digest,
                data_only: p.resources.is_empty(),
                minimum: p.minimum,
                metadata_usage: contract.checked().usage(),
            })
        })
    }
    pub const fn digest(&self) -> [u8; 32] {
        self.digest
    }
    pub const fn is_data_only(&self) -> bool {
        self.data_only
    }
    pub fn preimage(&self, session: &mut Session) -> Result<Vec<u8>, Error> {
        session.metadata(|session, limits| {
            super::contract::proof_limits(self.metadata_usage, limits, session)?;
            let p = projection(self.document, self.function, session)?;
            preimage(self.document, self.key, self.function, &p, session)
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameKind {
    Parameters,
    Result,
}
impl FrameKind {
    fn tag(self) -> u8 {
        match self {
            Self::Parameters => 0,
            Self::Result => 1,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValueId(pub usize);
/// Flat storage keeps parsing, graph validation and destruction off the native stack.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Unit,
    I64(i64),
    Bool(bool),
    Text(String),
    Bytes(Vec<u8>),
    List(Vec<ValueId>),
    Record(Vec<ValueId>),
    Variant {
        case: u32,
        payload: ValueId,
    },
    None,
    Some(ValueId),
    Ok(ValueId),
    Err(ValueId),
    /// An untrusted call-local index. Only the live component adapter may
    /// validate it against its original table and commit ownership or a loan.
    Resource(crate::component::adapter::BoundaryResourceIndex),
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Values {
    pub roots: Vec<ValueId>,
    pub nodes: Vec<Value>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResourceAccess {
    Own,
    Borrow(Access),
}
/// Structural evidence only. The live adapter must admit every use against
/// its original table before it stages or publishes an owning value.
pub(crate) struct ResourceUse<'a> {
    pub value: ValueId,
    pub index: crate::component::adapter::BoundaryResourceIndex,
    pub key: &'a ResourceTypeKey,
    pub access: ResourceAccess,
    pub offset: usize,
}
fn roots<'a>(
    sig: &'a Signature<'_>,
    kind: FrameKind,
) -> impl DoubleEndedIterator<Item = TypeId> + 'a {
    let params = if kind == FrameKind::Parameters {
        sig.function.params.as_slice()
    } else {
        &[]
    };
    params
        .iter()
        .map(|p| p.ty)
        .chain((kind == FrameKind::Result).then_some(sig.function.result))
}
pub fn encode_values(
    sig: &Signature<'_>,
    kind: FrameKind,
    values: &Values,
    session: &mut Session,
) -> Result<Vec<u8>, Error> {
    if !sig.data_only {
        return Err(session.error(0, ErrorKind::ResourceContextRequired));
    }
    encode_value_frame(sig, kind, values, session, None)
}

/// Structural encoding for the live adapter only. Indices remain claims;
/// the adapter must validate the complete call before granting any authority.
pub(crate) fn encode_contextual_values<'a>(
    sig: &Signature<'a>,
    kind: FrameKind,
    values: &Values,
    session: &mut Session,
) -> Result<(Vec<u8>, Vec<ResourceUse<'a>>), Error> {
    let mut uses = Vec::new();
    let bytes = encode_value_frame(sig, kind, values, session, Some(&mut uses))?;
    Ok((bytes, uses))
}

fn encode_value_frame<'a>(
    sig: &Signature<'a>,
    kind: FrameKind,
    values: &Values,
    session: &mut Session,
    mut uses: Option<&mut Vec<ResourceUse<'a>>>,
) -> Result<Vec<u8>, Error> {
    let expected = if kind == FrameKind::Parameters {
        sig.function.params.len()
    } else {
        1
    };
    if values.roots.len() != expected {
        return Err(session.error(0, ErrorKind::ValueType));
    }
    session.count(expected, 0)?;
    let mut w = Writer::new(session);
    w.header(b"LCVL", 1, kind.tag())?;
    w.raw(&sig.digest)?;
    w.u32(0)?;
    let start = w.at();
    let mut work = Vec::new();
    for index in (0..expected).rev() {
        let ty = if kind == FrameKind::Parameters {
            sig.function.params[index].ty
        } else {
            sig.function.result
        };
        w.session
            .push(&mut work, (ty, values.roots[index], 1usize), start)?;
    }
    while let Some((ty, value, depth)) = work.pop() {
        let at = w.at();
        w.session
            .cap(depth, w.session.limits.depth, LimitKind::Depth, at)?;
        w.session.value_work(1, at)?;
        let ty = resolve(sig.document, ty, w.session)?;
        let value_id = value;
        let value = values
            .nodes
            .get(value.0)
            .ok_or_else(|| w.session.error(at, ErrorKind::ValueType))?;
        let mut child = |w: &mut Writer<'_>, t, v| w.session.push(&mut work, (t, v, depth + 1), at);
        match (&sig.document.type_nodes[ty.0].kind, value) {
            (TypeKind::Unit, Value::Unit) => {}
            (TypeKind::I64, Value::I64(n)) => w.raw(&n.to_le_bytes())?,
            (TypeKind::Bool, Value::Bool(b)) => w.u8(u8::from(*b))?,
            (kind @ (TypeKind::Own(_) | TypeKind::Borrow { .. }), Value::Resource(index)) => {
                if let Some(uses) = uses.as_deref_mut() {
                    let (key, access) = match kind {
                        TypeKind::Own(key) => (&key.value, ResourceAccess::Own),
                        TypeKind::Borrow { resource, access } => {
                            (&resource.value, ResourceAccess::Borrow(*access))
                        }
                        _ => unreachable!("resource kind"),
                    };
                    w.session.push(
                        uses,
                        ResourceUse {
                            value: value_id,
                            index: *index,
                            key,
                            access,
                            offset: at,
                        },
                        at,
                    )?;
                }
                w.u32(index.get())?;
            }
            (TypeKind::Text, Value::Text(t)) => w.value_string(t)?,
            (TypeKind::Bytes, Value::Bytes(b)) => {
                let n = u32::try_from(b.len())
                    .map_err(|_| w.session.error(at, ErrorKind::ValueType))?;
                w.u32(n)?;
                w.payload(b)?;
            }
            (TypeKind::List(t), Value::List(v)) => {
                w.u32(
                    u32::try_from(v.len())
                        .map_err(|_| w.session.error(at, ErrorKind::ValueType))?,
                )?;
                w.session.count(v.len(), at)?;
                for v in v.iter().rev() {
                    child(&mut w, *t, *v)?;
                }
            }
            (TypeKind::Option(_), Value::None) => w.u8(0)?,
            (TypeKind::Option(t), Value::Some(v)) => {
                w.u8(1)?;
                w.session.count(1, at)?;
                child(&mut w, *t, *v)?;
            }
            (TypeKind::Result(ok, _), Value::Ok(v)) => {
                w.u8(0)?;
                w.session.count(1, at)?;
                child(&mut w, *ok, *v)?;
            }
            (TypeKind::Result(_, err), Value::Err(v)) => {
                w.u8(1)?;
                w.session.count(1, at)?;
                child(&mut w, *err, *v)?;
            }
            (TypeKind::Named(k), v) => {
                match (&definition(sig.document, &k.value, w.session)?.kind, v) {
                    (DefinitionKind::Record(fields), Value::Record(items))
                        if fields.len() == items.len() =>
                    {
                        w.session.count(fields.len(), at)?;
                        for (f, v) in fields.iter().zip(items).rev() {
                            child(&mut w, f.ty, *v)?;
                        }
                    }
                    (DefinitionKind::Variant(cases), Value::Variant { case, payload }) => {
                        let f = cases
                            .get(*case as usize)
                            .ok_or_else(|| w.session.error(at, ErrorKind::ValueType))?;
                        w.u32(*case)?;
                        w.session.count(1, at)?;
                        child(&mut w, f.ty, *payload)?;
                    }
                    _ => return Err(w.session.error(at, ErrorKind::ValueType)),
                }
            }
            _ => return Err(w.session.error(at, ErrorKind::ValueType)),
        }
    }
    let n =
        u32::try_from(w.at() - start).map_err(|_| w.session.error(start, ErrorKind::ValueType))?;
    w.session.charge(4, 41)?;
    w.bytes[41..45].copy_from_slice(&n.to_le_bytes());
    Ok(w.bytes)
}
pub fn decode_values(
    sig: &Signature<'_>,
    kind: FrameKind,
    bytes: &[u8],
    session: &mut Session,
) -> Result<Values, Error> {
    if !sig.data_only {
        return Err(session.error(0, ErrorKind::ResourceContextRequired));
    }
    decode_value_frame(sig, kind, bytes, session, None)
}

pub(crate) fn decode_contextual_values<'a>(
    sig: &Signature<'a>,
    kind: FrameKind,
    bytes: &[u8],
    session: &mut Session,
) -> Result<(Values, Vec<ResourceUse<'a>>), Error> {
    let mut uses = Vec::new();
    let values = decode_value_frame(sig, kind, bytes, session, Some(&mut uses))?;
    Ok((values, uses))
}

fn decode_value_frame<'a>(
    sig: &Signature<'a>,
    kind: FrameKind,
    bytes: &[u8],
    session: &mut Session,
    mut uses: Option<&mut Vec<ResourceUse<'a>>>,
) -> Result<Values, Error> {
    let mut r = Reader::new(bytes, session)?;
    r.header(b"LCVL", 1, kind.tag())?;
    if r.array::<32>()? != sig.digest {
        return Err(r.session.error(9, ErrorKind::Signature));
    }
    let length = r.u32()? as usize;
    if length != bytes.len() - r.at {
        return Err(r.session.error(
            41,
            if length > bytes.len() - r.at {
                ErrorKind::Truncated
            } else {
                ErrorKind::Trailing
            },
        ));
    }
    #[derive(Clone, Copy)]
    enum Aggregate {
        List,
        Record,
        Variant(u32),
        Some,
        Ok,
        Err,
    }
    enum Work {
        Read(TypeId, usize),
        Build(Aggregate, usize),
    }
    let mut work = Vec::new();
    let mut stack = Vec::new();
    let mut nodes = Vec::new();
    let expected = if kind == FrameKind::Parameters {
        sig.function.params.len()
    } else {
        1
    };
    r.session.count(expected, r.at)?;
    for ty in roots(sig, kind).rev() {
        r.session.push(&mut work, Work::Read(ty, 1), r.at)?;
    }
    while let Some(next) = work.pop() {
        let at = r.at;
        let value = match next {
            Work::Build(kind, n) => {
                let start = stack.len().checked_sub(n).expect("decoded children");
                match kind {
                    Aggregate::List | Aggregate::Record => {
                        let mut items = r.session.vector(n, at)?;
                        items.extend(stack.drain(start..));
                        if matches!(kind, Aggregate::List) {
                            Value::List(items)
                        } else {
                            Value::Record(items)
                        }
                    }
                    Aggregate::Variant(case) => Value::Variant {
                        case,
                        payload: stack.pop().expect("variant payload"),
                    },
                    Aggregate::Some => Value::Some(stack.pop().expect("option payload")),
                    Aggregate::Ok => Value::Ok(stack.pop().expect("result payload")),
                    Aggregate::Err => Value::Err(stack.pop().expect("result payload")),
                }
            }
            Work::Read(ty, depth) => {
                r.session
                    .cap(depth, r.session.limits.depth, LimitKind::Depth, at)?;
                r.session.value_work(1, at)?;
                let ty = resolve(sig.document, ty, r.session)?;
                if sig.minimum[ty.0].expect("reachable type") > r.bytes.len() - r.at {
                    return Err(r.session.error(at, ErrorKind::Truncated));
                }
                match &sig.document.type_nodes[ty.0].kind {
                    TypeKind::Unit => Value::Unit,
                    TypeKind::I64 => Value::I64(i64::from_le_bytes(r.array()?)),
                    TypeKind::Bool => Value::Bool(r.boolean()?),
                    kind @ (TypeKind::Own(_) | TypeKind::Borrow { .. }) => {
                        let index = crate::component::adapter::BoundaryResourceIndex::new(r.u32()?)
                            .ok_or_else(|| r.session.error(at, ErrorKind::ValueType))?;
                        if let Some(uses) = uses.as_deref_mut() {
                            let (key, access) = match kind {
                                TypeKind::Own(key) => (&key.value, ResourceAccess::Own),
                                TypeKind::Borrow { resource, access } => {
                                    (&resource.value, ResourceAccess::Borrow(*access))
                                }
                                _ => unreachable!("resource kind"),
                            };
                            r.session.push(
                                uses,
                                ResourceUse {
                                    value: ValueId(nodes.len()),
                                    index,
                                    key,
                                    access,
                                    offset: at,
                                },
                                at,
                            )?;
                        }
                        Value::Resource(index)
                    }
                    TypeKind::Text => Value::Text(r.value_string()?),
                    TypeKind::Bytes => {
                        let n = r.u32()? as usize;
                        let b = r.payload(n)?;
                        let mut v = r.session.vector(n, at)?;
                        v.extend_from_slice(b);
                        Value::Bytes(v)
                    }
                    TypeKind::List(t) => {
                        let n = r.u32()? as usize;
                        let min = sig.minimum[t.0].expect("reachable type");
                        let needed = n
                            .checked_mul(min)
                            .ok_or_else(|| r.session.error(at, ErrorKind::InvalidSchema))?;
                        if needed > r.bytes.len() - r.at {
                            return Err(r.session.error(at, ErrorKind::Truncated));
                        }
                        r.session.count(n, at)?;
                        r.session
                            .push(&mut work, Work::Build(Aggregate::List, n), at)?;
                        for _ in 0..n {
                            r.session.push(&mut work, Work::Read(*t, depth + 1), at)?;
                        }
                        continue;
                    }
                    TypeKind::Option(t) => {
                        if !r.boolean()? {
                            Value::None
                        } else {
                            r.session.count(1, at)?;
                            r.session
                                .push(&mut work, Work::Build(Aggregate::Some, 1), at)?;
                            r.session.push(&mut work, Work::Read(*t, depth + 1), at)?;
                            continue;
                        }
                    }
                    TypeKind::Result(ok, err) => {
                        let err_case = r.boolean()?;
                        r.session.count(1, at)?;
                        r.session.push(
                            &mut work,
                            Work::Build(
                                if err_case {
                                    Aggregate::Err
                                } else {
                                    Aggregate::Ok
                                },
                                1,
                            ),
                            at,
                        )?;
                        r.session.push(
                            &mut work,
                            Work::Read(if err_case { *err } else { *ok }, depth + 1),
                            at,
                        )?;
                        continue;
                    }
                    TypeKind::Named(k) => {
                        match &definition(sig.document, &k.value, r.session)?.kind {
                            DefinitionKind::Record(fields) => {
                                r.session.count(fields.len(), at)?;
                                r.session.push(
                                    &mut work,
                                    Work::Build(Aggregate::Record, fields.len()),
                                    at,
                                )?;
                                for f in fields.iter().rev() {
                                    r.session.push(&mut work, Work::Read(f.ty, depth + 1), at)?;
                                }
                                continue;
                            }
                            DefinitionKind::Variant(cases) => {
                                let case = r.u32()?;
                                let field = cases
                                    .get(case as usize)
                                    .ok_or_else(|| r.session.error(at, ErrorKind::Tag))?;
                                r.session.count(1, at)?;
                                r.session.push(
                                    &mut work,
                                    Work::Build(Aggregate::Variant(case), 1),
                                    at,
                                )?;
                                r.session
                                    .push(&mut work, Work::Read(field.ty, depth + 1), at)?;
                                continue;
                            }
                            DefinitionKind::Alias(_) => unreachable!("resolved alias"),
                        }
                    }
                }
            }
        };
        let id = ValueId(nodes.len());
        r.session.push(&mut nodes, value, at)?;
        r.session.push(&mut stack, id, at)?;
    }
    r.finish()?;
    Ok(Values {
        roots: stack,
        nodes,
    })
}
