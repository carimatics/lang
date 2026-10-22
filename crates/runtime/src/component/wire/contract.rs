use super::io::{Reader, Writer};
use super::*;
use crate::component::idl::{
    self, Access, CheckedDocument, Definition, DefinitionKind, Document, Effect, Field, Function,
    Import, Interface, Spanned, TypeId, TypeKind, TypeNode, ValueKey,
};
use crate::component::negotiation::{
    BuildIdentifier, InterfaceKey, PackageKey, PrereleaseIdentifier, ResourceTypeKey,
    SemanticVersion, VersionRequirement,
};
use crate::component::serialization::{ComponentContractLock, DigestAlgorithm, VersionedDigest};

/// Immutable normalized metadata. It grants no executable binding or resource.
#[derive(Debug)]
pub struct Contract {
    document: CheckedDocument,
    locks: Vec<ComponentContractLock>,
}
pub(super) fn proof_limits(
    usage: idl::Usage,
    limits: idl::Limits,
    session: &Session,
) -> Result<(), Error> {
    session.cap(
        usage.declarations,
        limits.declarations,
        LimitKind::Declarations,
        0,
    )?;
    session.cap(
        usage.elements,
        limits.elements,
        LimitKind::MetadataElements,
        0,
    )?;
    session.cap(usage.data_depth, limits.depth, LimitKind::Depth, 0)
}
impl Contract {
    pub fn new(
        document: CheckedDocument,
        mut locks: Vec<ComponentContractLock>,
        session: &mut Session,
    ) -> Result<Self, Error> {
        let n = locks.len();
        session.cap(
            n,
            session.limits.metadata.declarations,
            LimitKind::Declarations,
            0,
        )?;
        session.charge(n, 0)?;
        let key_bytes = locks
            .iter()
            .map(|l| interface_bytes(l.key()))
            .max()
            .unwrap_or(0);
        sorting(session, n, key_bytes, 0)?;
        locks.sort_unstable_by(|a, b| a.key().cmp(b.key()));
        validate_locks(document.document(), &locks, session)?;
        Ok(Self { document, locks })
    }
    pub fn checked(&self) -> &CheckedDocument {
        &self.document
    }
    pub fn document(&self) -> &Document {
        self.document.document()
    }
    pub fn locks(&self) -> &[ComponentContractLock] {
        &self.locks
    }
}
pub(super) fn interface_bytes(k: &InterfaceKey) -> usize {
    k.package().namespace().len() + k.package().package().len() + k.interface().len() + 2
}
pub(super) fn lookup(s: &mut Session, n: usize, bytes: usize, at: usize) -> Result<(), Error> {
    let comparisons = if n == 0 {
        1
    } else {
        usize::BITS as usize - n.leading_zeros() as usize + 1
    };
    s.charge(
        Session::multiply(comparisons, Session::add(bytes, 1, at)?, at)?,
        at,
    )
}
fn sorting(s: &mut Session, n: usize, bytes: usize, at: usize) -> Result<(), Error> {
    let comparisons = if n < 2 {
        1
    } else {
        usize::BITS as usize - n.leading_zeros() as usize + 1
    };
    s.charge(
        Session::multiply(
            Session::multiply(n, comparisons, at)?,
            Session::add(bytes, 1, at)?,
            at,
        )?,
        at,
    )
}
fn strict<T: Ord>(
    previous: Option<&T>,
    next: &T,
    s: &mut Session,
    bytes: usize,
    at: usize,
) -> Result<(), Error> {
    s.charge(Session::add(bytes, 1, at)?, at)?;
    if previous.is_some_and(|p| p >= next) {
        Err(s.error(at, ErrorKind::Noncanonical))
    } else {
        Ok(())
    }
}
fn valid_version(v: &SemanticVersion, s: &mut Session, at: usize) -> Result<(), Error> {
    for p in v.prerelease() {
        let (text, numeric) = match p {
            PrereleaseIdentifier::Numeric(t) => (t, true),
            PrereleaseIdentifier::AlphaNumeric(t) => (t, false),
        };
        version_part(text, Some(numeric), s, at)?;
    }
    for p in v.build() {
        version_part(p.as_str(), None, s, at)?;
    }
    Ok(())
}
fn version_part(t: &str, numeric: Option<bool>, s: &mut Session, at: usize) -> Result<(), Error> {
    s.charge(t.len(), at)?;
    let digits = t.bytes().all(|b| b.is_ascii_digit());
    if t.is_empty()
        || !t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || numeric.is_some_and(|n| n != digits || (n && t.len() > 1 && t.starts_with('0')))
    {
        return Err(s.error(at, ErrorKind::InvalidSchema));
    }
    Ok(())
}
fn valid_key(k: &InterfaceKey, s: &mut Session, at: usize) -> Result<(), Error> {
    s.charge(interface_bytes(k), at)?;
    if !crate::component::is_component_identifier(k.package().namespace())
        || !crate::component::is_component_identifier(k.package().package())
        || !crate::component::is_component_identifier(k.interface())
    {
        Err(s.error(at, ErrorKind::InvalidSchema))
    } else {
        Ok(())
    }
}
fn validate_locks(
    d: &Document,
    locks: &[ComponentContractLock],
    s: &mut Session,
) -> Result<(), Error> {
    for (i, lock) in locks.iter().enumerate() {
        valid_key(lock.key(), s, 0)?;
        valid_version(lock.version(), s, 0)?;
        strict(
            i.checked_sub(1).map(|i| locks[i].key()),
            lock.key(),
            s,
            interface_bytes(lock.key()),
            0,
        )?;
        lookup(s, d.imports.len(), interface_bytes(lock.key()), 0)?;
        let index = d
            .imports
            .binary_search_by(|r| r.interface.value.cmp(lock.key()))
            .map_err(|_| s.error(0, ErrorKind::Unresolved))?;
        let valid = match &d.imports[index].requirement.value {
            VersionRequirement::Exact(v) => v == lock.version(),
            VersionRequirement::Interval {
                min_inclusive,
                max_exclusive,
                allow_prerelease,
            } => {
                (*allow_prerelease || lock.version().prerelease().is_empty())
                    && min_inclusive.precedence_cmp(lock.version()).is_le()
                    && lock.version().precedence_cmp(max_exclusive).is_lt()
            }
        };
        if !valid {
            return Err(s.error(0, ErrorKind::InvalidSchema));
        }
    }
    Ok(())
}
pub(super) fn write_package(w: &mut Writer<'_>, k: &PackageKey) -> Result<(), Error> {
    w.string(k.namespace())?;
    w.string(k.package())
}
pub(super) fn write_interface(w: &mut Writer<'_>, k: &InterfaceKey) -> Result<(), Error> {
    write_package(w, k.package())?;
    w.string(k.interface())
}
pub(super) fn write_resource(w: &mut Writer<'_>, k: &ResourceTypeKey) -> Result<(), Error> {
    write_interface(w, k.interface())?;
    w.string(k.resource())
}
pub(super) fn write_value_key(w: &mut Writer<'_>, k: &ValueKey) -> Result<(), Error> {
    write_interface(w, &k.interface)?;
    w.string(&k.name)
}
fn write_version(w: &mut Writer<'_>, v: &SemanticVersion) -> Result<(), Error> {
    w.u64(v.major())?;
    w.u64(v.minor())?;
    w.u64(v.patch())?;
    w.count(v.prerelease().len())?;
    for p in v.prerelease() {
        match p {
            PrereleaseIdentifier::Numeric(t) => {
                w.u8(0)?;
                w.string(t)?;
            }
            PrereleaseIdentifier::AlphaNumeric(t) => {
                w.u8(1)?;
                w.string(t)?;
            }
        }
    }
    w.count(v.build().len())?;
    for p in v.build() {
        w.string(p.as_str())?;
    }
    Ok(())
}
fn write_requirement(w: &mut Writer<'_>, v: &VersionRequirement) -> Result<(), Error> {
    match v {
        VersionRequirement::Exact(v) => {
            w.u8(0)?;
            write_version(w, v)
        }
        VersionRequirement::Interval {
            min_inclusive,
            max_exclusive,
            allow_prerelease,
        } => {
            w.u8(1)?;
            write_version(w, min_inclusive)?;
            write_version(w, max_exclusive)?;
            w.u8(u8::from(*allow_prerelease))
        }
    }
}
pub(super) fn write_type(w: &mut Writer<'_>, d: &Document, root: TypeId) -> Result<(), Error> {
    let mut work = Vec::new();
    let at = w.at();
    w.session.push(&mut work, (root, 1usize), at)?;
    while let Some((id, depth)) = work.pop() {
        let at = w.at();
        w.session
            .cap(depth, w.session.limits.depth, LimitKind::Depth, at)?;
        w.session.charge(1, at)?;
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
            TypeKind::List(child) | TypeKind::Option(child) => {
                w.u8(if matches!(d.type_nodes[id.0].kind, TypeKind::List(_)) {
                    8
                } else {
                    9
                })?;
                w.session.push(&mut work, (*child, depth + 1), at)?;
            }
            TypeKind::Result(ok, err) => {
                w.u8(10)?;
                w.session.push(&mut work, (*err, depth + 1), at)?;
                w.session.push(&mut work, (*ok, depth + 1), at)?;
            }
        }
    }
    Ok(())
}
pub(super) fn write_fields(
    w: &mut Writer<'_>,
    d: &Document,
    fields: &[Field],
) -> Result<(), Error> {
    w.count(fields.len())?;
    for f in fields {
        w.string(&f.name.value)?;
        write_type(w, d, f.ty)?;
    }
    Ok(())
}
pub(super) fn write_definition(
    w: &mut Writer<'_>,
    d: &Document,
    def: &Definition,
) -> Result<(), Error> {
    write_value_key(w, &def.key.value)?;
    match &def.kind {
        DefinitionKind::Record(f) => {
            w.u8(0)?;
            write_fields(w, d, f)
        }
        DefinitionKind::Variant(f) => {
            w.u8(1)?;
            write_fields(w, d, f)
        }
        DefinitionKind::Alias(t) => {
            w.u8(2)?;
            write_type(w, d, *t)
        }
    }
}
pub fn encode_contract(contract: &Contract, session: &mut Session) -> Result<Vec<u8>, Error> {
    session.metadata(|session, limits| {
        session.cap(1, session.limits.depth, LimitKind::Depth, 0)?;
        let d = contract.document();
        let usage = contract.checked().usage();
        proof_limits(usage, limits, session)?;
        let mut w = Writer::new(session);
        w.header(b"LCMP", 3, 0)?;
        write_package(&mut w, &d.package.value)?;
        write_version(&mut w, &d.version.value)?;
        w.session
            .charge(Session::multiply(d.interfaces.len(), 2, 0)?, 0)?;
        w.count(d.interfaces.iter().map(|i| i.resources.len()).sum())?;
        for i in &d.interfaces {
            for r in &i.resources {
                write_resource(&mut w, &r.value)?;
            }
        }
        w.count(d.interfaces.iter().map(|i| i.definitions.len()).sum())?;
        for i in &d.interfaces {
            for def in &i.definitions {
                write_definition(&mut w, d, def)?;
            }
        }
        w.count(d.interfaces.len())?;
        for i in &d.interfaces {
            write_interface(&mut w, &i.key.value)?;
            w.count(i.functions.len())?;
            for f in &i.functions {
                w.string(&f.name.value)?;
                w.u8(u8::from(f.effect.value == Effect::Task))?;
                write_fields(&mut w, d, &f.params)?;
                write_type(&mut w, d, f.result)?;
                w.string(&f.capability.value)?;
            }
        }
        w.count(d.imports.len())?;
        for r in &d.imports {
            write_interface(&mut w, &r.interface.value)?;
            write_requirement(&mut w, &r.requirement.value)?;
            w.count(r.members.len())?;
            for n in &r.members {
                w.string(&n.value)?;
            }
        }
        w.count(contract.locks.len())?;
        for l in &contract.locks {
            write_interface(&mut w, l.key())?;
            write_version(&mut w, l.version())?;
            w.u8(match l.digest().algorithm() {
                DigestAlgorithm::Sha256 => 0,
            })?;
            w.raw(l.digest().as_bytes())?;
        }
        w.count(d.exports.len())?;
        for k in &d.exports {
            write_interface(&mut w, &k.value)?;
        }
        Ok(w.bytes)
    })
}

struct Decoder<'b, 's> {
    r: Reader<'b, 's>,
    source: m::SourceId,
    types: Vec<TypeNode>,
    limits: idl::Limits,
    declarations: usize,
    elements: usize,
}
impl Decoder<'_, '_> {
    fn span(&self, start: usize) -> m::Span {
        m::Span {
            source: self.source,
            start: start as u32,
            end: self.r.at as u32,
        }
    }
    fn invalid(&self, at: usize) -> Error {
        self.r.session.error(at, ErrorKind::InvalidSchema)
    }
    fn declared(&mut self, n: usize) -> Result<(), Error> {
        self.declarations = Session::add(self.declarations, n, self.r.at)?;
        self.r.session.cap(
            self.declarations,
            self.limits.declarations,
            LimitKind::Declarations,
            self.r.at,
        )
    }
    fn elements(&mut self, n: usize) -> Result<(), Error> {
        self.elements = Session::add(self.elements, n, self.r.at)?;
        self.r.session.cap(
            self.elements,
            self.limits.elements,
            LimitKind::MetadataElements,
            self.r.at,
        )
    }
    fn name(&mut self) -> Result<Spanned<String>, Error> {
        let at = self.r.at;
        let value = self.r.string()?;
        self.r.session.charge(value.len(), at)?;
        if !crate::component::is_component_identifier(&value) {
            return Err(self.invalid(at));
        }
        Ok(Spanned {
            value,
            span: self.span(at),
        })
    }
    fn package(&mut self) -> Result<PackageKey, Error> {
        Ok(PackageKey::new(self.name()?.value, self.name()?.value))
    }
    fn interface(&mut self) -> Result<InterfaceKey, Error> {
        Ok(InterfaceKey::new(self.package()?, self.name()?.value))
    }
    fn resource(&mut self) -> Result<ResourceTypeKey, Error> {
        Ok(ResourceTypeKey::new(self.interface()?, self.name()?.value))
    }
    fn value_key(&mut self) -> Result<ValueKey, Error> {
        Ok(ValueKey {
            interface: self.interface()?,
            name: self.name()?.value,
        })
    }
    fn version(&mut self) -> Result<SemanticVersion, Error> {
        let major = self.r.u64()?;
        let minor = self.r.u64()?;
        let patch = self.r.u64()?;
        let n = self.r.count(6)?;
        let mut pre = self.r.session.vector(n, self.r.at)?;
        for _ in 0..n {
            let at = self.r.at;
            let tag = self.r.u8()?;
            if tag > 1 {
                return Err(self.r.session.error(at, ErrorKind::Tag));
            }
            let value = self.r.string()?;
            version_part(&value, Some(tag == 0), self.r.session, at)?;
            pre.push(if tag == 0 {
                PrereleaseIdentifier::Numeric(value)
            } else {
                PrereleaseIdentifier::AlphaNumeric(value)
            });
        }
        let n = self.r.count(5)?;
        let mut build = self.r.session.vector(n, self.r.at)?;
        for _ in 0..n {
            let at = self.r.at;
            let value = self.r.string()?;
            version_part(&value, None, self.r.session, at)?;
            self.r.session.charge(value.len(), at)?;
            build.push(BuildIdentifier::new(value).map_err(|_| self.invalid(at))?);
        }
        Ok(SemanticVersion::new(major, minor, patch)
            .with_prerelease(pre)
            .with_build(build))
    }
    fn requirement(&mut self) -> Result<VersionRequirement, Error> {
        let at = self.r.at;
        match self.r.u8()? {
            0 => Ok(VersionRequirement::Exact(self.version()?)),
            1 => {
                let min_inclusive = self.version()?;
                let max_exclusive = self.version()?;
                let allow_prerelease = self.r.boolean()?;
                if !min_inclusive.precedence_cmp(&max_exclusive).is_lt() {
                    return Err(self.invalid(at));
                }
                Ok(VersionRequirement::Interval {
                    min_inclusive,
                    max_exclusive,
                    allow_prerelease,
                })
            }
            _ => Err(self.r.session.error(at, ErrorKind::Tag)),
        }
    }
    fn ty(&mut self) -> Result<TypeId, Error> {
        enum Constructor {
            List,
            Option,
            Result,
        }
        enum Work {
            Read(usize),
            Build(Constructor, usize),
        }
        let mut work = Vec::new();
        let mut results = Vec::new();
        self.r.session.push(&mut work, Work::Read(1), self.r.at)?;
        while let Some(step) = work.pop() {
            let at = self.r.at;
            let (kind, start) = match step {
                Work::Build(kind, start) => {
                    let last = results.pop().expect("type child");
                    (
                        match kind {
                            Constructor::List => TypeKind::List(last),
                            Constructor::Option => TypeKind::Option(last),
                            Constructor::Result => {
                                TypeKind::Result(results.pop().expect("result child"), last)
                            }
                        },
                        start,
                    )
                }
                Work::Read(depth) => {
                    self.r
                        .session
                        .cap(depth, self.r.session.limits.depth, LimitKind::Depth, at)?;
                    let kind = match self.r.u8()? {
                        0 => TypeKind::I64,
                        1 => TypeKind::Bool,
                        5 => TypeKind::Unit,
                        6 => TypeKind::Text,
                        7 => TypeKind::Bytes,
                        tag @ (2..=4) => {
                            let resource = Spanned {
                                value: self.resource()?,
                                span: self.span(at),
                            };
                            if tag == 2 {
                                TypeKind::Own(resource)
                            } else {
                                TypeKind::Borrow {
                                    access: if tag == 3 {
                                        Access::Shared
                                    } else {
                                        Access::Exclusive
                                    },
                                    resource,
                                }
                            }
                        }
                        tag @ (8..=10) => {
                            let kind = match tag {
                                8 => Constructor::List,
                                9 => Constructor::Option,
                                _ => Constructor::Result,
                            };
                            self.r.session.push(&mut work, Work::Build(kind, at), at)?;
                            self.r.session.push(&mut work, Work::Read(depth + 1), at)?;
                            if tag == 10 {
                                self.r.session.push(&mut work, Work::Read(depth + 1), at)?;
                            }
                            continue;
                        }
                        11 => TypeKind::Named(Spanned {
                            value: self.value_key()?,
                            span: self.span(at),
                        }),
                        _ => return Err(self.r.session.error(at, ErrorKind::Tag)),
                    };
                    (kind, at)
                }
            };
            let ty = TypeId(self.types.len());
            let span = self.span(start);
            self.r
                .session
                .push(&mut self.types, TypeNode { kind, span }, at)?;
            self.r.session.push(&mut results, ty, at)?;
        }
        Ok(results.pop().expect("one type root"))
    }
    fn fields(&mut self, params: bool) -> Result<Vec<Field>, Error> {
        let at = self.r.at;
        let n = self.r.count(6)?;
        if params && n > 256 {
            return Err(self.invalid(at));
        }
        self.elements(n)?;
        let mut fields = self.r.session.vector(n, self.r.at)?;
        for _ in 0..n {
            fields.push(Field {
                name: self.name()?,
                ty: self.ty()?,
            });
        }
        Ok(fields)
    }
    fn definition(&mut self) -> Result<Definition, Error> {
        let at = self.r.at;
        let key = Spanned {
            value: self.value_key()?,
            span: self.span(at),
        };
        let tag = self.r.at;
        let kind = match self.r.u8()? {
            0 => DefinitionKind::Record(self.fields(false)?),
            1 => {
                let f = self.fields(false)?;
                if f.is_empty() {
                    return Err(self.invalid(tag));
                }
                DefinitionKind::Variant(f)
            }
            2 => DefinitionKind::Alias(self.ty()?),
            _ => return Err(self.r.session.error(tag, ErrorKind::Tag)),
        };
        Ok(Definition {
            key,
            kind,
            span: self.span(at),
        })
    }
    fn function(&mut self) -> Result<Function, Error> {
        let at = self.r.at;
        let name = self.name()?;
        let pos = self.r.at;
        let effect = Spanned {
            value: match self.r.u8()? {
                0 => Effect::Sync,
                1 => Effect::Task,
                _ => return Err(self.r.session.error(pos, ErrorKind::Tag)),
            },
            span: self.span(pos),
        };
        let params = self.fields(true)?;
        let result = self.ty()?;
        let pos = self.r.at;
        let value = self.r.string()?;
        self.r.session.charge(value.len(), pos)?;
        if !crate::component::is_capability_identifier(&value) {
            return Err(self.invalid(pos));
        }
        let capability = Spanned {
            value,
            span: self.span(pos),
        };
        Ok(Function {
            name,
            effect,
            params,
            result,
            capability,
            span: self.span(at),
        })
    }
}
pub fn decode_contract(
    source: m::SourceId,
    bytes: &[u8],
    session: &mut Session,
) -> Result<Contract, Error> {
    session.metadata(|session, limits| {
        session.cap(1, session.limits.depth, LimitKind::Depth, 0)?;
        if bytes.len() > u32::MAX as usize {
            return Err(session.error(0, ErrorKind::InvalidSchema));
        }
        let r = Reader::new(bytes, session)?;
        let mut p = Decoder {
            r,
            source,
            types: Vec::new(),
            limits,
            declarations: 0,
            elements: 0,
        };
        p.r.header(b"LCMP", 3, 0)?;
        let at = p.r.at;
        let package = Spanned {
            value: p.package()?,
            span: p.span(at),
        };
        let at = p.r.at;
        let version = Spanned {
            value: p.version()?,
            span: p.span(at),
        };
        let n = p.r.count(20)?;
        p.declared(n)?;
        let mut resources: Vec<Spanned<ResourceTypeKey>> = p.r.session.vector(n, p.r.at)?;
        for _ in 0..n {
            let at = p.r.at;
            let value = p.resource()?;
            strict(
                resources.last().map(|v| &v.value),
                &value,
                p.r.session,
                interface_bytes(value.interface()) + value.resource().len() + 1,
                at,
            )?;
            resources.push(Spanned {
                value,
                span: p.span(at),
            });
        }
        let n = p.r.count(22)?;
        p.declared(n)?;
        let mut definitions: Vec<Definition> = p.r.session.vector(n, p.r.at)?;
        for _ in 0..n {
            let def = p.definition()?;
            strict(
                definitions.last().map(|v| &v.key.value),
                &def.key.value,
                p.r.session,
                interface_bytes(&def.key.value.interface) + def.key.value.name.len() + 1,
                def.span.start as usize,
            )?;
            definitions.push(def);
        }
        let n = p.r.count(19)?;
        p.declared(n)?;
        let mut interfaces: Vec<Interface> = p.r.session.vector(n, p.r.at)?;
        let mut resources = resources.into_iter().peekable();
        let mut definitions = definitions.into_iter().peekable();
        for _ in 0..n {
            let at = p.r.at;
            let value = p.interface()?;
            strict(
                interfaces.last().map(|v| &v.key.value),
                &value,
                p.r.session,
                interface_bytes(&value),
                at,
            )?;
            let key = Spanned {
                value,
                span: p.span(at),
            };
            let n = p.r.count(16)?;
            p.declared(n)?;
            let mut functions: Vec<Function> = p.r.session.vector(n, p.r.at)?;
            for _ in 0..n {
                let f = p.function()?;
                strict(
                    functions.last().map(|f| &f.name.value),
                    &f.name.value,
                    p.r.session,
                    f.name.value.len(),
                    f.span.start as usize,
                )?;
                functions.push(f);
            }
            let mut own_resources = Vec::new();
            let mut own_definitions = Vec::new();
            while resources
                .peek()
                .is_some_and(|r| r.value.interface() == &key.value)
            {
                p.r.session
                    .push(&mut own_resources, resources.next().expect("peeked"), at)?;
            }
            while definitions
                .peek()
                .is_some_and(|d| d.key.value.interface == key.value)
            {
                p.r.session.push(
                    &mut own_definitions,
                    definitions.next().expect("peeked"),
                    at,
                )?;
            }
            interfaces.push(Interface {
                key,
                resources: own_resources,
                definitions: own_definitions,
                functions,
                span: p.span(at),
            });
        }
        if resources.next().is_some() || definitions.next().is_some() {
            return Err(p.r.session.error(p.r.at, ErrorKind::Unresolved));
        }
        let n = p.r.count(57)?;
        p.declared(n)?;
        let mut imports: Vec<Import> = p.r.session.vector(n, p.r.at)?;
        for _ in 0..n {
            let at = p.r.at;
            let value = p.interface()?;
            strict(
                imports.last().map(|i| &i.interface.value),
                &value,
                p.r.session,
                interface_bytes(&value),
                at,
            )?;
            let interface = Spanned {
                value,
                span: p.span(at),
            };
            let pos = p.r.at;
            let requirement = Spanned {
                value: p.requirement()?,
                span: p.span(pos),
            };
            let n = p.r.count(5)?;
            p.elements(n)?;
            if n == 0 {
                return Err(p.invalid(pos));
            }
            let mut members: Vec<Spanned<String>> = p.r.session.vector(n, p.r.at)?;
            for _ in 0..n {
                let name = p.name()?;
                strict(
                    members.last().map(|n| &n.value),
                    &name.value,
                    p.r.session,
                    name.value.len(),
                    name.span.start as usize,
                )?;
                members.push(name);
            }
            imports.push(Import {
                interface,
                requirement,
                members,
                span: p.span(at),
            });
        }
        let n = p.r.count(80)?;
        p.r.session
            .cap(n, limits.declarations, LimitKind::Declarations, p.r.at)?;
        let mut locks: Vec<ComponentContractLock> = p.r.session.vector(n, p.r.at)?;
        for _ in 0..n {
            let at = p.r.at;
            let key = p.interface()?;
            strict(
                locks.last().map(|l| l.key()),
                &key,
                p.r.session,
                interface_bytes(&key),
                at,
            )?;
            let version = p.version()?;
            let at = p.r.at;
            if p.r.u8()? != 0 {
                return Err(p.r.session.error(at, ErrorKind::Tag));
            }
            let raw = p.r.raw(32)?;
            let mut digest = p.r.session.vector(32, at)?;
            digest.extend_from_slice(raw);
            let digest =
                VersionedDigest::new(DigestAlgorithm::Sha256, digest).expect("exact SHA256 length");
            locks.push(ComponentContractLock::new(key, version, digest));
        }
        let n = p.r.count(15)?;
        p.elements(n)?;
        let mut exports: Vec<Spanned<InterfaceKey>> = p.r.session.vector(n, p.r.at)?;
        for _ in 0..n {
            let at = p.r.at;
            let value = p.interface()?;
            strict(
                exports.last().map(|e| &e.value),
                &value,
                p.r.session,
                interface_bytes(&value),
                at,
            )?;
            exports.push(Spanned {
                value,
                span: p.span(at),
            });
        }
        p.r.finish()?;
        let document = Document {
            source,
            source_bytes: bytes.len() as u32,
            package,
            version,
            interfaces,
            imports,
            exports,
            type_nodes: p.types,
        };
        let budget = std::mem::replace(
            &mut p.r.session.budget,
            m::Budget::new(m::Limits::default()),
        );
        let (document, budget) =
            idl::check_decoded(document, limits, budget, p.declarations, p.elements);
        p.r.session.budget = budget;
        let document = document.map_err(|e| {
            p.r.session
                .error(e.span.start as usize, ErrorKind::Declaration(e))
        })?;
        validate_locks(document.document(), &locks, p.r.session)?;
        Ok(Contract { document, locks })
    })
}
