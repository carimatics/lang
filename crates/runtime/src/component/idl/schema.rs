use super::super::negotiation::{InterfaceKey, PackageKey, ResourceTypeKey, VersionRequirement};
use super::json::{Id, Json, Kind};
use super::*;

pub(super) fn parse(json: &Json<'_>, c: &mut Context) -> Result<Document, Error> {
    let mut r = Reader {
        json,
        c,
        types: Vec::new(),
    };
    r.closed(
        0,
        &[
            "idl",
            "package",
            "version",
            "interfaces",
            "imports",
            "exports",
        ],
    )?;
    let version = r.required(0, "idl")?;
    let location = r.span(version);
    if !matches!(r.node(version).kind, Kind::Number)
        || &json.input[location.start as usize..location.end as usize] != b"1"
    {
        return Err(r.c.error(Code::Version, Reason::InvalidVersion, location));
    }
    let id = r.required(0, "package")?;
    let package = Spanned {
        value: r.package(id)?,
        span: r.span(id),
    };
    let id = r.required(0, "version")?;
    let version = Spanned {
        value: versions::parse(r.string(id)?, r.span(id), r.c)?,
        span: r.span(id),
    };
    let id = r.required(0, "interfaces")?;
    let array = r.array(id)?;
    r.c.count(array.len(), LimitKind::Declarations, r.span(id))?;
    let mut interfaces = r.c.vector(array.len(), r.span(id))?;
    for id in array {
        interfaces.push(r.interface(*id)?);
    }
    let id = r.required(0, "imports")?;
    let array = r.array(id)?;
    r.c.count(array.len(), LimitKind::Declarations, r.span(id))?;
    let mut imports = r.c.vector(array.len(), r.span(id))?;
    for id in array {
        imports.push(r.import(*id)?);
    }
    let id = r.required(0, "exports")?;
    let array = r.array(id)?;
    r.c.count(array.len(), LimitKind::Elements, r.span(id))?;
    let mut exports = r.c.vector(array.len(), r.span(id))?;
    for id in array {
        exports.push(Spanned {
            value: r.interface_key(r.string(*id)?, r.span(*id))?,
            span: r.span(*id),
        });
    }
    Ok(Document {
        source: r.c.source,
        source_bytes: json.input.len() as u32,
        package,
        version,
        interfaces,
        imports,
        exports,
        type_nodes: r.types,
    })
}
struct Reader<'j, 'a, 'c> {
    json: &'j Json<'a>,
    c: &'c mut Context,
    types: Vec<TypeNode>,
}
impl<'j> Reader<'j, '_, '_> {
    fn node(&self, id: Id) -> &'j json::Node {
        &self.json.nodes[id]
    }
    fn span(&self, id: Id) -> m::Span {
        self.node(id).span
    }
    fn expected(&self, id: Id, what: &'static str) -> Error {
        self.c
            .error(Code::Field, Reason::Expected(what), self.span(id))
    }
    fn object(&self, id: Id) -> Result<&'j [json::Member], Error> {
        match &self.node(id).kind {
            Kind::Object(v) => Ok(v),
            _ => Err(self.expected(id, "object")),
        }
    }
    fn array(&self, id: Id) -> Result<&'j [Id], Error> {
        match &self.node(id).kind {
            Kind::Array(v) => Ok(v),
            _ => Err(self.expected(id, "array")),
        }
    }
    fn string(&self, id: Id) -> Result<&'j str, Error> {
        match &self.node(id).kind {
            Kind::String(v) => Ok(v),
            _ => Err(self.expected(id, "string")),
        }
    }
    fn closed(&mut self, id: Id, allowed: &[&str]) -> Result<(), Error> {
        for field in self.object(id)? {
            self.c
                .charge(field.key.value.len() + allowed.len(), field.key.span)?;
            if !allowed.contains(&field.key.value.as_str()) {
                return Err(self
                    .c
                    .error(Code::Field, Reason::UnknownField, field.key.span));
            }
        }
        Ok(())
    }
    fn optional(&self, id: Id, key: &str) -> Result<Option<Id>, Error> {
        Ok(self
            .object(id)?
            .iter()
            .find(|m| m.key.value == key)
            .map(|m| m.value))
    }
    fn required(&self, id: Id, key: &'static str) -> Result<Id, Error> {
        self.optional(id, key)?.ok_or_else(|| {
            self.c
                .error(Code::Field, Reason::MissingField(key), self.span(id))
        })
    }
    fn name(&mut self, id: Id) -> Result<Spanned<String>, Error> {
        let value = self.string(id)?;
        let span = self.span(id);
        self.c.charge(value.len(), span)?;
        if !super::super::is_component_identifier(value) {
            return Err(self.c.error(Code::Name, Reason::InvalidName, span));
        }
        Ok(Spanned {
            value: self.c.text(value, span)?,
            span,
        })
    }
    fn package_text(&mut self, value: &str, span: m::Span) -> Result<PackageKey, Error> {
        self.c.charge(value.len(), span)?;
        let (ns, name) = value
            .split_once(':')
            .ok_or_else(|| self.c.error(Code::Name, Reason::InvalidName, span))?;
        if !super::super::is_component_identifier(ns)
            || !super::super::is_component_identifier(name)
        {
            return Err(self.c.error(Code::Name, Reason::InvalidName, span));
        }
        Ok(PackageKey::new(
            self.c.text(ns, span)?,
            self.c.text(name, span)?,
        ))
    }
    fn package(&mut self, id: Id) -> Result<PackageKey, Error> {
        self.package_text(self.string(id)?, self.span(id))
    }
    fn interface_key(&mut self, value: &str, span: m::Span) -> Result<InterfaceKey, Error> {
        self.c.charge(value.len(), span)?;
        let (package, name) = value
            .split_once('/')
            .ok_or_else(|| self.c.error(Code::Name, Reason::InvalidName, span))?;
        if !super::super::is_component_identifier(name) {
            return Err(self.c.error(Code::Name, Reason::InvalidName, span));
        }
        Ok(InterfaceKey::new(
            self.package_text(package, span)?,
            self.c.text(name, span)?,
        ))
    }
    fn key_copy(&mut self, key: &InterfaceKey, span: m::Span) -> Result<InterfaceKey, Error> {
        Ok(InterfaceKey::new(
            PackageKey::new(
                self.c.text(key.package().namespace(), span)?,
                self.c.text(key.package().package(), span)?,
            ),
            self.c.text(key.interface(), span)?,
        ))
    }
    fn reference(
        &mut self,
        id: Id,
        interface: &InterfaceKey,
    ) -> Result<(InterfaceKey, String), Error> {
        let text = self.string(id)?;
        let span = self.span(id);
        self.c.charge(text.len(), span)?;
        let (key, name) = if let Some((key, name)) = text.rsplit_once('.') {
            (self.interface_key(key, span)?, name)
        } else {
            (self.key_copy(interface, span)?, text)
        };
        if !super::super::is_component_identifier(name) {
            return Err(self.c.error(Code::Name, Reason::InvalidName, span));
        }
        Ok((key, self.c.text(name, span)?))
    }
    fn ty(&mut self, root: Id, interface: &InterfaceKey) -> Result<TypeId, Error> {
        enum Constructor {
            List,
            Option,
            Result,
        }
        enum Work {
            Visit(Id),
            Build(Constructor, m::Span),
        }
        let mut work = Vec::new();
        let mut results = Vec::new();
        self.c.push(&mut work, Work::Visit(root), self.span(root))?;
        while let Some(next) = work.pop() {
            match next {
                Work::Build(kind, span) => {
                    let last = results.pop().expect("type child");
                    let ty = match kind {
                        Constructor::List => TypeKind::List(last),
                        Constructor::Option => TypeKind::Option(last),
                        Constructor::Result => {
                            TypeKind::Result(results.pop().expect("result child"), last)
                        }
                    };
                    let id = TypeId(self.types.len());
                    self.c
                        .push(&mut self.types, TypeNode { kind: ty, span }, span)?;
                    self.c.push(&mut results, id, span)?;
                }
                Work::Visit(id) => {
                    let span = self.span(id);
                    self.c.charge(1, span)?;
                    let kind = match &self.node(id).kind {
                        Kind::String(text) => match text.as_str() {
                            "unit" => TypeKind::Unit,
                            "i64" => TypeKind::I64,
                            "bool" => TypeKind::Bool,
                            "text" => TypeKind::Text,
                            "bytes" => TypeKind::Bytes,
                            _ => {
                                let (key, name) = self.reference(id, interface)?;
                                TypeKind::Named(Spanned {
                                    value: ValueKey {
                                        interface: key,
                                        name,
                                    },
                                    span,
                                })
                            }
                        },
                        Kind::Object(fields) => {
                            if fields.len() != 1 {
                                return Err(self.c.error(
                                    Code::Type,
                                    Reason::Expected("one type constructor"),
                                    span,
                                ));
                            }
                            let field = &fields[0];
                            match field.key.value.as_str() {
                                "list" | "option" => {
                                    let constructor = if field.key.value == "option" {
                                        Constructor::Option
                                    } else {
                                        Constructor::List
                                    };
                                    self.c
                                        .push(&mut work, Work::Build(constructor, span), span)?;
                                    self.c.push(&mut work, Work::Visit(field.value), span)?;
                                    continue;
                                }
                                "result" => {
                                    self.closed(field.value, &["ok", "err"])?;
                                    let ok = self.required(field.value, "ok")?;
                                    let err = self.required(field.value, "err")?;
                                    self.c.push(
                                        &mut work,
                                        Work::Build(Constructor::Result, span),
                                        span,
                                    )?;
                                    self.c.push(&mut work, Work::Visit(err), span)?;
                                    self.c.push(&mut work, Work::Visit(ok), span)?;
                                    continue;
                                }
                                "own" | "borrow" | "borrow-mut" => {
                                    let (key, name) = self.reference(field.value, interface)?;
                                    let resource = Spanned {
                                        value: ResourceTypeKey::new(key, name),
                                        span: self.span(field.value),
                                    };
                                    if field.key.value == "own" {
                                        TypeKind::Own(resource)
                                    } else {
                                        TypeKind::Borrow {
                                            access: if field.key.value == "borrow" {
                                                Access::Shared
                                            } else {
                                                Access::Exclusive
                                            },
                                            resource,
                                        }
                                    }
                                }
                                _ => {
                                    return Err(self.c.error(
                                        Code::Field,
                                        Reason::UnknownField,
                                        field.key.span,
                                    ));
                                }
                            }
                        }
                        _ => return Err(self.c.error(Code::Type, Reason::Expected("type"), span)),
                    };
                    let value = TypeId(self.types.len());
                    self.c
                        .push(&mut self.types, TypeNode { kind, span }, span)?;
                    self.c.push(&mut results, value, span)?;
                }
            }
        }
        Ok(results.pop().expect("one type root"))
    }
    fn fields(&mut self, id: Id, interface: &InterfaceKey) -> Result<Vec<Field>, Error> {
        let array = self.array(id)?;
        self.c
            .count(array.len(), LimitKind::Elements, self.span(id))?;
        let mut fields = self.c.vector(array.len(), self.span(id))?;
        for id in array {
            self.closed(*id, &["name", "type"])?;
            let name = self.required(*id, "name")?;
            let ty = self.required(*id, "type")?;
            fields.push(Field {
                name: self.name(name)?,
                ty: self.ty(ty, interface)?,
            });
        }
        Ok(fields)
    }
    fn interface(&mut self, id: Id) -> Result<Interface, Error> {
        self.closed(id, &["key", "resources", "types", "functions"])?;
        let key = self.required(id, "key")?;
        let key = Spanned {
            value: self.interface_key(self.string(key)?, self.span(key))?,
            span: self.span(key),
        };
        let resources = self.required(id, "resources")?;
        let array = self.array(resources)?;
        self.c
            .count(array.len(), LimitKind::Declarations, self.span(resources))?;
        let mut resources = self.c.vector(array.len(), self.span(resources))?;
        for id in array {
            let name = self.name(*id)?;
            resources.push(Spanned {
                value: ResourceTypeKey::new(self.key_copy(&key.value, name.span)?, name.value),
                span: name.span,
            });
        }
        let types = self.required(id, "types")?;
        let array = self.array(types)?;
        self.c
            .count(array.len(), LimitKind::Declarations, self.span(types))?;
        let mut definitions = self.c.vector(array.len(), self.span(types))?;
        for id in array {
            self.closed(*id, &["name", "record", "variant", "alias"])?;
            let name = self.required(*id, "name")?;
            let name = self.name(name)?;
            let record = self.optional(*id, "record")?;
            let variant = self.optional(*id, "variant")?;
            let alias = self.optional(*id, "alias")?;
            let kind = match (record, variant, alias) {
                (Some(v), None, None) => DefinitionKind::Record(self.fields(v, &key.value)?),
                (None, Some(v), None) => {
                    let fields = self.fields(v, &key.value)?;
                    if fields.is_empty() {
                        return Err(self.c.error(
                            Code::Type,
                            Reason::Expected("nonempty variant"),
                            self.span(v),
                        ));
                    }
                    DefinitionKind::Variant(fields)
                }
                (None, None, Some(v)) => DefinitionKind::Alias(self.ty(v, &key.value)?),
                _ => {
                    return Err(self.c.error(
                        Code::Type,
                        Reason::Expected("one definition kind"),
                        self.span(*id),
                    ));
                }
            };
            definitions.push(Definition {
                key: Spanned {
                    value: ValueKey {
                        interface: self.key_copy(&key.value, name.span)?,
                        name: name.value,
                    },
                    span: name.span,
                },
                kind,
                span: self.span(*id),
            });
        }
        let functions = self.required(id, "functions")?;
        let array = self.array(functions)?;
        self.c
            .count(array.len(), LimitKind::Declarations, self.span(functions))?;
        let mut functions = self.c.vector(array.len(), self.span(functions))?;
        for id in array {
            self.closed(*id, &["name", "effect", "params", "result", "capability"])?;
            let name = self.required(*id, "name")?;
            let name = self.name(name)?;
            let effect = self.required(*id, "effect")?;
            let effect = Spanned {
                value: match self.string(effect)? {
                    "sync" => Effect::Sync,
                    "task" => Effect::Task,
                    _ => {
                        return Err(self.c.error(
                            Code::Type,
                            Reason::InvalidEffect,
                            self.span(effect),
                        ));
                    }
                },
                span: self.span(effect),
            };
            let params = self.required(*id, "params")?;
            self.c.cap(
                self.array(params)?.len(),
                256,
                LimitKind::Arity,
                self.span(params),
            )?;
            let params = self.fields(params, &key.value)?;
            let result = self.required(*id, "result")?;
            let result = self.ty(result, &key.value)?;
            let capability = self.required(*id, "capability")?;
            let text = self.string(capability)?;
            self.c.charge(text.len(), self.span(capability))?;
            if !super::super::is_capability_identifier(text) {
                return Err(self
                    .c
                    .error(Code::Name, Reason::InvalidName, self.span(capability)));
            }
            let capability = Spanned {
                value: self.c.text(text, self.span(capability))?,
                span: self.span(capability),
            };
            functions.push(Function {
                name,
                effect,
                params,
                result,
                capability,
                span: self.span(*id),
            });
        }
        Ok(Interface {
            key,
            resources,
            definitions,
            functions,
            span: self.span(id),
        })
    }
    fn import(&mut self, id: Id) -> Result<Import, Error> {
        self.closed(id, &["interface", "version", "members"])?;
        let key = self.required(id, "interface")?;
        let interface = Spanned {
            value: self.interface_key(self.string(key)?, self.span(key))?,
            span: self.span(key),
        };
        let req = self.required(id, "version")?;
        self.closed(req, &["exact", "interval"])?;
        let value = match (
            self.optional(req, "exact")?,
            self.optional(req, "interval")?,
        ) {
            (Some(exact), None) => VersionRequirement::Exact(versions::parse(
                self.string(exact)?,
                self.span(exact),
                self.c,
            )?),
            (None, Some(interval)) => {
                self.closed(interval, &["min", "max", "prerelease"])?;
                let min = self.required(interval, "min")?;
                let max = self.required(interval, "max")?;
                let pre = self.required(interval, "prerelease")?;
                let min = versions::parse(self.string(min)?, self.span(min), self.c)?;
                let max = versions::parse(self.string(max)?, self.span(max), self.c)?;
                if min.precedence_cmp(&max) != std::cmp::Ordering::Less {
                    return Err(self.c.error(
                        Code::Version,
                        Reason::InvalidVersion,
                        self.span(interval),
                    ));
                }
                let Kind::Bool(allow_prerelease) = self.node(pre).kind else {
                    return Err(self.expected(pre, "boolean"));
                };
                VersionRequirement::Interval {
                    min_inclusive: min,
                    max_exclusive: max,
                    allow_prerelease,
                }
            }
            _ => {
                return Err(self
                    .c
                    .error(Code::Version, Reason::InvalidVersion, self.span(req)));
            }
        };
        let members = self.required(id, "members")?;
        let array = self.array(members)?;
        self.c
            .count(array.len(), LimitKind::Elements, self.span(members))?;
        if array.is_empty() {
            return Err(self.c.error(
                Code::Projection,
                Reason::InvalidProjection,
                self.span(members),
            ));
        }
        let mut members = self.c.vector(array.len(), self.span(members))?;
        for id in array {
            members.push(self.name(*id)?);
        }
        Ok(Import {
            interface,
            requirement: Spanned {
                value,
                span: self.span(req),
            },
            members,
            span: self.span(id),
        })
    }
}
