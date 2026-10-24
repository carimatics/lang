use super::super::contract as c;
use super::super::io::Writer;
use super::*;
use crate::component::negotiation::{
    InterfaceKey, PackageKey, ResourceTypeKey, SemanticVersion, VersionRequirement,
};
use idl::{Access, Effect, Field, TypeId, ValueKey};

#[derive(Clone, Copy, Default)]
pub(super) struct Counts {
    pub declarations: usize,
    pub elements: usize,
}
#[derive(Clone, Copy)]
enum Class {
    Declaration,
    Element,
    Binding,
}
struct Encoder<'d, 's> {
    w: Writer<'s>,
    source: m::SourceId,
    text: &'d str,
    root: m::Span,
    types: usize,
    counts: Counts,
    bindings: usize,
    binding_limit: usize,
}
impl Encoder<'_, '_> {
    fn bad(&self) -> Error {
        self.w.session.error(self.w.at(), ErrorKind::InvalidSchema)
    }
    fn count(&mut self, n: usize, class: Class) -> Result<(), Error> {
        let at = self.w.at();
        match class {
            Class::Declaration => {
                self.counts.declarations = Session::add(self.counts.declarations, n, at)?;
                self.w.session.cap(
                    self.counts.declarations,
                    self.w.session.limits.metadata.declarations,
                    LimitKind::Declarations,
                    at,
                )?;
            }
            Class::Element => {
                self.counts.elements = Session::add(self.counts.elements, n, at)?;
                self.w.session.cap(
                    self.counts.elements,
                    self.w.session.limits.metadata.elements,
                    LimitKind::MetadataElements,
                    at,
                )?;
            }
            Class::Binding => {
                self.bindings = Session::add(self.bindings, n, at)?;
                self.w.session.cap(
                    self.bindings,
                    self.binding_limit,
                    LimitKind::Declarations,
                    at,
                )?;
            }
        }
        self.w.count(n)
    }
    fn list<T>(
        &mut self,
        values: &[T],
        class: Class,
        mut f: impl FnMut(&mut Self, &T) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.count(values.len(), class)?;
        for value in values {
            f(self, value)?;
        }
        Ok(())
    }
    fn ordered<'a, V, T: Ord + 'a>(
        &mut self,
        values: &'a [V],
        key: impl Fn(&'a V) -> &'a T,
        bytes: impl Fn(&V) -> usize,
    ) -> Result<(), Error> {
        for pair in values.windows(2) {
            c::strict(
                Some(key(&pair[0])),
                key(&pair[1]),
                self.w.session,
                bytes(&pair[1]),
                self.w.at(),
            )?;
        }
        Ok(())
    }
    fn span(&mut self, value: m::Span, owner: m::Span) -> Result<(), Error> {
        if !valid_span(value, self.source, self.text)
            || !valid_span(owner, self.source, self.text)
            || value.start < owner.start
            || value.end > owner.end
        {
            return Err(self.bad());
        }
        self.w.u32(value.source.0)?;
        self.w.u32(value.start)?;
        self.w.u32(value.end)
    }
    fn spanned<T>(
        &mut self,
        value: &Spanned<T>,
        owner: m::Span,
        f: impl FnOnce(&mut Self, &T) -> Result<(), Error>,
    ) -> Result<(), Error> {
        f(self, &value.value)?;
        self.span(value.span, owner)
    }
    fn name(&mut self, name: &str) -> Result<(), Error> {
        let at = self.w.at();
        self.w.session.charge(name.len(), at)?;
        if !crate::component::is_component_identifier(name) {
            return Err(self.bad());
        }
        self.w.string(name)
    }
    fn package(&mut self, key: &PackageKey) -> Result<(), Error> {
        self.name(key.namespace())?;
        self.name(key.package())
    }
    fn interface(&mut self, key: &InterfaceKey) -> Result<(), Error> {
        self.package(key.package())?;
        self.name(key.interface())
    }
    fn resource(&mut self, key: &ResourceTypeKey) -> Result<(), Error> {
        self.interface(key.interface())?;
        self.name(key.resource())
    }
    fn value_key(&mut self, key: &ValueKey) -> Result<(), Error> {
        self.interface(&key.interface)?;
        self.name(&key.name)
    }
    fn version(&mut self, value: &SemanticVersion) -> Result<(), Error> {
        use crate::component::negotiation::PrereleaseIdentifier;
        let at = self.w.at();
        c::valid_version(value, self.w.session, at)?;
        self.w.u64(value.major())?;
        self.w.u64(value.minor())?;
        self.w.u64(value.patch())?;
        self.list(value.prerelease(), Class::Element, |s, v| {
            let (tag, text) = match v {
                PrereleaseIdentifier::Numeric(t) => (0, t),
                PrereleaseIdentifier::AlphaNumeric(t) => (1, t),
            };
            s.w.u8(tag)?;
            s.w.string(text)
        })?;
        self.list(value.build(), Class::Element, |s, v| s.w.string(v.as_str()))
    }
    fn requirement(&mut self, value: &VersionRequirement) -> Result<(), Error> {
        match value {
            VersionRequirement::Exact(v) => {
                self.w.u8(0)?;
                self.version(v)
            }
            VersionRequirement::Interval {
                min_inclusive,
                max_exclusive,
                allow_prerelease,
            } => {
                let at = self.w.at();
                valid_interval(min_inclusive, max_exclusive, self.w.session, at)?;
                self.w.u8(1)?;
                self.version(min_inclusive)?;
                self.version(max_exclusive)?;
                self.w.u8(u8::from(*allow_prerelease))
            }
        }
    }
    fn ty(&mut self, id: TypeId) -> Result<(), Error> {
        if id.0 >= self.types {
            return Err(self.bad());
        }
        self.w.u32(u32::try_from(id.0).map_err(|_| self.bad())?)
    }
    fn field(&mut self, value: &Field, owner: m::Span) -> Result<(), Error> {
        self.spanned(&value.name, owner, |s, n| s.name(n))?;
        self.ty(value.ty)
    }
    fn definition(&mut self, value: &idl::Definition, owner: m::Span) -> Result<(), Error> {
        self.spanned(&value.key, value.span, Self::value_key)?;
        match &value.kind {
            DefinitionKind::Record(fields) | DefinitionKind::Variant(fields) => {
                self.w
                    .u8(u8::from(matches!(value.kind, DefinitionKind::Variant(_))))?;
                self.list(fields, Class::Element, |s, v| s.field(v, value.span))?;
            }
            DefinitionKind::Alias(ty) => {
                self.w.u8(2)?;
                self.ty(*ty)?;
            }
        }
        self.span(value.span, owner)
    }
    fn function(&mut self, value: &idl::Function, owner: m::Span) -> Result<(), Error> {
        self.spanned(&value.name, value.span, |s, n| s.name(n))?;
        self.spanned(&value.effect, value.span, |s, e| {
            s.w.u8(u8::from(*e == Effect::Task))
        })?;
        self.list(&value.params, Class::Element, |s, v| s.field(v, value.span))?;
        self.ty(value.result)?;
        self.spanned(&value.capability, value.span, |s, n| {
            let at = s.w.at();
            s.w.session.charge(n.len(), at)?;
            if !crate::component::is_capability_identifier(n) {
                return Err(s.bad());
            }
            s.w.string(n)
        })?;
        self.span(value.span, owner)
    }
    fn interface_row(&mut self, value: &idl::Interface) -> Result<(), Error> {
        self.spanned(&value.key, value.span, Self::interface)?;
        self.ordered(
            &value.resources,
            |v| &v.value,
            |v| c::interface_bytes(v.value.interface()) + v.value.resource().len() + 1,
        )?;
        self.list(&value.resources, Class::Declaration, |s, v| {
            s.spanned(v, value.span, Self::resource)
        })?;
        self.ordered(
            &value.definitions,
            |v| &v.key.value,
            |v| c::interface_bytes(&v.key.value.interface) + v.key.value.name.len() + 1,
        )?;
        self.list(&value.definitions, Class::Declaration, |s, v| {
            s.definition(v, value.span)
        })?;
        self.ordered(&value.functions, |v| &v.name.value, |v| v.name.value.len())?;
        self.list(&value.functions, Class::Declaration, |s, v| {
            s.function(v, value.span)
        })?;
        self.span(value.span, self.root)
    }
    fn import_row(&mut self, value: &idl::Import) -> Result<(), Error> {
        self.spanned(&value.interface, value.span, Self::interface)?;
        self.spanned(&value.requirement, value.span, Self::requirement)?;
        self.ordered(&value.members, |v| &v.value, |v| v.value.len())?;
        self.list(&value.members, Class::Element, |s, v| {
            s.spanned(v, value.span, |s, n| s.name(n))
        })?;
        self.span(value.span, self.root)
    }
    fn node(&mut self, value: &idl::TypeNode) -> Result<(), Error> {
        match &value.kind {
            TypeKind::Unit => self.w.u8(0)?,
            TypeKind::I64 => self.w.u8(1)?,
            TypeKind::Bool => self.w.u8(2)?,
            TypeKind::Text => self.w.u8(3)?,
            TypeKind::Bytes => self.w.u8(4)?,
            TypeKind::List(id) => {
                self.w.u8(5)?;
                self.ty(*id)?;
            }
            TypeKind::Option(id) => {
                self.w.u8(6)?;
                self.ty(*id)?;
            }
            TypeKind::Result(ok, err) => {
                self.w.u8(7)?;
                self.ty(*ok)?;
                self.ty(*err)?;
            }
            TypeKind::Named(key) => {
                self.w.u8(8)?;
                self.spanned(key, value.span, Self::value_key)?;
            }
            TypeKind::Own(key) => {
                self.w.u8(9)?;
                self.spanned(key, value.span, Self::resource)?;
            }
            TypeKind::Borrow { access, resource } => {
                self.w.u8(10)?;
                self.w.u8(u8::from(*access == Access::Exclusive))?;
                self.spanned(resource, value.span, Self::resource)?;
            }
        }
        self.span(value.span, self.root)
    }
    fn unit(&mut self, d: &Document, map: &b::Map) -> Result<(), Error> {
        self.w.u32(d.source.0)?;
        self.w.u32(d.source_bytes)?;
        self.spanned(&d.package, self.root, Self::package)?;
        self.spanned(&d.version, self.root, Self::version)?;
        self.ordered(
            &d.interfaces,
            |v| &v.key.value,
            |v| c::interface_bytes(&v.key.value),
        )?;
        self.list(&d.interfaces, Class::Declaration, Self::interface_row)?;
        self.ordered(
            &d.imports,
            |v| &v.interface.value,
            |v| c::interface_bytes(&v.interface.value),
        )?;
        self.list(&d.imports, Class::Declaration, Self::import_row)?;
        self.ordered(&d.exports, |v| &v.value, |v| c::interface_bytes(&v.value))?;
        self.list(&d.exports, Class::Element, |s, v| {
            s.spanned(v, s.root, Self::interface)
        })?;
        self.list(&d.type_nodes, Class::Element, Self::node)?;
        if map.version != 1 {
            return Err(self.bad());
        }
        self.w.u32(map.version)?;
        self.list(&map.resources, Class::Binding, |s, v| {
            s.w.u32(v.tag.0)?;
            s.resource(&v.resource)
        })?;
        self.list(&map.imports, Class::Binding, |s, v| {
            s.w.u32(v.source.0)?;
            s.interface(&v.member.interface)?;
            s.name(&v.member.member)
        })?;
        self.list(&map.exports, Class::Binding, |s, v| {
            s.w.u32(v.source.0)?;
            s.interface(&v.member.interface)?;
            s.name(&v.member.member)
        })
    }
}
fn encoder<'d, 's>(
    d: &'d Document,
    sources: &'d [m::Source],
    session: &'s mut Session,
    measure: bool,
    binding_limit: usize,
) -> Result<Encoder<'d, 's>, Error> {
    let source = sources
        .get(d.source.0 as usize)
        .ok_or_else(|| session.error(0, ErrorKind::InvalidSchema))?;
    if source.text.len() != d.source_bytes as usize {
        return Err(session.error(0, ErrorKind::InvalidSchema));
    }
    session.cap(
        source.text.len(),
        session.limits.metadata.source_bytes,
        LimitKind::Bytes,
        0,
    )?;
    Ok(Encoder {
        w: if measure {
            Writer::measuring(session)
        } else {
            Writer::new(session)
        },
        source: d.source,
        text: &source.text,
        root: m::Span {
            source: d.source,
            start: 0,
            end: d.source_bytes,
        },
        types: d.type_nodes.len(),
        counts: Counts::default(),
        bindings: 0,
        binding_limit,
    })
}
pub(super) fn measure(
    d: &Document,
    map: &b::Map,
    sources: &[m::Source],
    session: &mut Session,
    binding_limit: usize,
) -> Result<Counts, Error> {
    let mut w = encoder(d, sources, session, true, binding_limit)?;
    w.unit(d, map)?;
    Ok(w.counts)
}
pub(super) fn encode(
    d: &Document,
    map: &b::Map,
    sources: &[m::Source],
    session: &mut Session,
    binding_limit: usize,
) -> Result<Vec<u8>, Error> {
    let mut w = encoder(d, sources, session, false, binding_limit)?;
    w.unit(d, map)?;
    Ok(w.w.bytes)
}
