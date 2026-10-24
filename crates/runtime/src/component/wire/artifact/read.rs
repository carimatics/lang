use super::super::contract as c;
use super::super::io::Reader;
use super::*;
use crate::component::negotiation::{
    BuildIdentifier, InterfaceKey, PackageKey, PrereleaseIdentifier, ResourceTypeKey,
    SemanticVersion, VersionRequirement,
};
use idl::{
    Access, Definition, Effect, Field, Function, Import, Interface, TypeId, TypeNode, ValueKey,
};
#[derive(Clone, Copy)]
enum Class {
    Declaration,
    Element,
    Binding,
}
struct Decoder<'b, 's> {
    r: Reader<'b, 's>,
    declarations: usize,
    elements: usize,
    bindings: usize,
    binding_limit: usize,
}
impl Decoder<'_, '_> {
    fn bad(&self) -> Error {
        self.r.session.error(self.r.at, ErrorKind::InvalidSchema)
    }
    fn list<T>(
        &mut self,
        minimum: usize,
        class: Class,
        mut f: impl FnMut(&mut Self) -> Result<T, Error>,
    ) -> Result<Vec<T>, Error> {
        let at = self.r.at;
        let n = self.r.count(minimum)?;
        match class {
            Class::Declaration => {
                self.declarations = Session::add(self.declarations, n, at)?;
                self.r.session.cap(
                    self.declarations,
                    self.r.session.limits.metadata.declarations,
                    LimitKind::Declarations,
                    at,
                )?;
            }
            Class::Element => {
                self.elements = Session::add(self.elements, n, at)?;
                self.r.session.cap(
                    self.elements,
                    self.r.session.limits.metadata.elements,
                    LimitKind::MetadataElements,
                    at,
                )?;
            }
            Class::Binding => {
                self.bindings = Session::add(self.bindings, n, at)?;
                self.r.session.cap(
                    self.bindings,
                    self.binding_limit,
                    LimitKind::Declarations,
                    at,
                )?;
            }
        }
        let mut values = self.r.session.vector(n, at)?;
        for _ in 0..n {
            values.push(f(self)?);
        }
        Ok(values)
    }
    fn span(&mut self) -> Result<m::Span, Error> {
        Ok(m::Span {
            source: m::SourceId(self.r.u32()?),
            start: self.r.u32()?,
            end: self.r.u32()?,
        })
    }
    fn spanned<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<Spanned<T>, Error> {
        Ok(Spanned {
            value: f(self)?,
            span: self.span()?,
        })
    }
    fn package(&mut self) -> Result<PackageKey, Error> {
        Ok(PackageKey::new(self.r.string()?, self.r.string()?))
    }
    fn interface(&mut self) -> Result<InterfaceKey, Error> {
        Ok(InterfaceKey::new(self.package()?, self.r.string()?))
    }
    fn resource(&mut self) -> Result<ResourceTypeKey, Error> {
        Ok(ResourceTypeKey::new(self.interface()?, self.r.string()?))
    }
    fn value_key(&mut self) -> Result<ValueKey, Error> {
        Ok(ValueKey {
            interface: self.interface()?,
            name: self.r.string()?,
        })
    }
    fn version(&mut self) -> Result<SemanticVersion, Error> {
        let major = self.r.u64()?;
        let minor = self.r.u64()?;
        let patch = self.r.u64()?;
        let pre = self.list(5, Class::Element, |s| {
            let at = s.r.at;
            let tag = s.r.u8()?;
            if tag > 1 {
                return Err(s.r.session.error(at, ErrorKind::Tag));
            }
            let value = s.r.string()?;
            c::version_part(&value, Some(tag == 0), s.r.session, at)?;
            Ok(if tag == 0 {
                PrereleaseIdentifier::Numeric(value)
            } else {
                PrereleaseIdentifier::AlphaNumeric(value)
            })
        })?;
        let build = self.list(4, Class::Element, |s| {
            let at = s.r.at;
            let value = s.r.string()?;
            c::version_part(&value, None, s.r.session, at)?;
            BuildIdentifier::new(value).map_err(|_| s.bad())
        })?;
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
                valid_interval(&min_inclusive, &max_exclusive, self.r.session, at)?;
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
        Ok(TypeId(self.r.u32()? as usize))
    }
    fn field(&mut self) -> Result<Field, Error> {
        Ok(Field {
            name: self.spanned(|s| s.r.string())?,
            ty: self.ty()?,
        })
    }
    fn definition(&mut self) -> Result<Definition, Error> {
        let key = self.spanned(Self::value_key)?;
        let at = self.r.at;
        let kind = match self.r.u8()? {
            0 => DefinitionKind::Record(self.list(20, Class::Element, Self::field)?),
            1 => DefinitionKind::Variant(self.list(20, Class::Element, Self::field)?),
            2 => DefinitionKind::Alias(self.ty()?),
            _ => return Err(self.r.session.error(at, ErrorKind::Tag)),
        };
        Ok(Definition {
            key,
            kind,
            span: self.span()?,
        })
    }
    fn effect(&mut self) -> Result<Effect, Error> {
        let at = self.r.at;
        match self.r.u8()? {
            0 => Ok(Effect::Sync),
            1 => Ok(Effect::Task),
            _ => Err(self.r.session.error(at, ErrorKind::Tag)),
        }
    }
    fn function(&mut self) -> Result<Function, Error> {
        Ok(Function {
            name: self.spanned(|s| s.r.string())?,
            effect: self.spanned(Self::effect)?,
            params: self.list(20, Class::Element, Self::field)?,
            result: self.ty()?,
            capability: self.spanned(|s| s.r.string())?,
            span: self.span()?,
        })
    }
    fn interface_row(&mut self) -> Result<Interface, Error> {
        Ok(Interface {
            key: self.spanned(Self::interface)?,
            resources: self.list(28, Class::Declaration, |s| s.spanned(Self::resource))?,
            definitions: self.list(45, Class::Declaration, Self::definition)?,
            functions: self.list(61, Class::Declaration, Self::function)?,
            span: self.span()?,
        })
    }
    fn import_row(&mut self) -> Result<Import, Error> {
        Ok(Import {
            interface: self.spanned(Self::interface)?,
            requirement: self.spanned(Self::requirement)?,
            members: self.list(16, Class::Element, |s| s.spanned(|s| s.r.string()))?,
            span: self.span()?,
        })
    }
    fn node(&mut self) -> Result<TypeNode, Error> {
        let at = self.r.at;
        let kind = match self.r.u8()? {
            0 => TypeKind::Unit,
            1 => TypeKind::I64,
            2 => TypeKind::Bool,
            3 => TypeKind::Text,
            4 => TypeKind::Bytes,
            5 => TypeKind::List(self.ty()?),
            6 => TypeKind::Option(self.ty()?),
            7 => TypeKind::Result(self.ty()?, self.ty()?),
            8 => TypeKind::Named(self.spanned(Self::value_key)?),
            9 => TypeKind::Own(self.spanned(Self::resource)?),
            10 => {
                let at = self.r.at;
                let access = match self.r.u8()? {
                    0 => Access::Shared,
                    1 => Access::Exclusive,
                    _ => return Err(self.r.session.error(at, ErrorKind::Tag)),
                };
                TypeKind::Borrow {
                    access,
                    resource: self.spanned(Self::resource)?,
                }
            }
            _ => return Err(self.r.session.error(at, ErrorKind::Tag)),
        };
        Ok(TypeNode {
            kind,
            span: self.span()?,
        })
    }
    fn map(&mut self) -> Result<b::Map, Error> {
        let version = self.r.u32()?;
        if version != 1 {
            return Err(self.bad());
        }
        Ok(b::Map {
            version,
            resources: self.list(20, Class::Binding, |s| {
                Ok(b::ResourceBinding {
                    tag: m::DefinitionId(s.r.u32()?),
                    resource: s.resource()?,
                })
            })?,
            imports: self.list(20, Class::Binding, |s| {
                Ok(b::ImportBinding {
                    source: m::ImportId(s.r.u32()?),
                    member: b::MemberKey {
                        interface: s.interface()?,
                        member: s.r.string()?,
                    },
                })
            })?,
            exports: self.list(20, Class::Binding, |s| {
                Ok(b::ExportBinding {
                    source: m::FunctionId(s.r.u32()?),
                    member: b::MemberKey {
                        interface: s.interface()?,
                        member: s.r.string()?,
                    },
                })
            })?,
        })
    }
}
pub(super) fn decode(
    bytes: &[u8],
    sources: &[m::Source],
    session: &mut Session,
    binding_limit: usize,
) -> Result<(Document, b::Map), Error> {
    let mut d = Decoder {
        r: Reader::new(bytes, session)?,
        declarations: 0,
        elements: 0,
        bindings: 0,
        binding_limit,
    };
    let source = m::SourceId(d.r.u32()?);
    let source_bytes = d.r.u32()?;
    let snapshot = sources.get(source.0 as usize).ok_or_else(|| d.bad())?;
    if snapshot.text.len() != source_bytes as usize {
        return Err(d.bad());
    }
    d.r.session.cap(
        source_bytes as usize,
        d.r.session.limits.metadata.source_bytes,
        LimitKind::Bytes,
        0,
    )?;
    let document = Document {
        source,
        source_bytes,
        package: d.spanned(Decoder::package)?,
        version: d.spanned(Decoder::version)?,
        interfaces: d.list(48, Class::Declaration, Decoder::interface_row)?,
        imports: d.list(85, Class::Declaration, Decoder::import_row)?,
        exports: d.list(24, Class::Element, |s| s.spanned(Decoder::interface))?,
        type_nodes: d.list(13, Class::Element, Decoder::node)?,
    };
    let mappings = d.map()?;
    d.r.finish()?;
    Ok((document, mappings))
}
