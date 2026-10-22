//! Deterministic declarations and passive binding data for checked component IDL.
use crate::frontend::component as source;
use lang_mir::unit as m;
use lang_runtime::component::{
    bindings::MemberKey,
    idl,
    negotiation::{InterfaceKey, ResourceTypeKey},
};

pub mod abi;
pub mod c;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub output_bytes: usize,
    pub depth: usize,
    pub budget: m::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            output_bytes: 64 * 1024 * 1024,
            depth: 128,
            budget: Default::default(),
        }
    }
}
#[derive(Debug)]
pub enum Error {
    Budget(m::Error),
    Wire(lang_runtime::component::wire::Error),
    OutputLimit,
    Depth,
    Reference,
    Allocation,
}
impl From<m::Error> for Error {
    fn from(error: m::Error) -> Self {
        Self::Budget(error)
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "component binding generation failed: {self:?}")
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Export {
    pub member: MemberKey,
    pub source: String,
    /// A public guest signature. The implementation supplies its body.
    pub declaration: String,
    pub contract_span: m::Span,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Origin {
    pub start: usize,
    pub end: usize,
    pub contract: m::Span,
}
#[derive(Debug)]
pub struct Guest {
    pub declarations: String,
    pub bindings: source::Input,
    pub exports: Vec<Export>,
    pub usage: m::Usage,
    /// Cumulative appended bytes, including intermediate and retained strings.
    pub text_bytes: usize,
    pub origins: Vec<Origin>,
}

struct Output {
    limits: Limits,
    budget: m::Budget,
    bytes: usize,
}
impl Output {
    fn new(limits: Limits) -> Self {
        Self {
            limits,
            budget: m::Budget::new(limits.budget),
            bytes: 0,
        }
    }
    fn usage(&self) -> m::Usage {
        m::Usage {
            work: self.budget.work_used(),
            allocation_bytes: self.budget.allocation_bytes(),
            elements: self.budget.elements_used(),
        }
    }
    fn number(&mut self, out: &mut String, value: usize) -> Result<()> {
        let mut buffer = [0; 20];
        self.text(out, decimal(value, &mut buffer))
    }
    fn text(&mut self, out: &mut String, text: &str) -> Result<()> {
        self.budget.charge(text.len().saturating_add(1))?;
        let next = self
            .bytes
            .checked_add(text.len())
            .ok_or(Error::OutputLimit)?;
        if next > self.limits.output_bytes {
            return Err(Error::OutputLimit);
        }
        let needed = out
            .len()
            .checked_add(text.len())
            .ok_or(Error::OutputLimit)?;
        if needed > out.capacity() {
            let capacity = out
                .capacity()
                .max(4)
                .checked_mul(2)
                .ok_or(Error::OutputLimit)?
                .max(needed);
            self.budget.bytes(capacity)?;
            self.budget.charge(out.len())?;
            out.try_reserve_exact(capacity - out.len())
                .map_err(|_| Error::Allocation)?;
        }
        out.push_str(text);
        self.bytes = next;
        Ok(())
    }
}
struct Writer<'a> {
    document: &'a idl::Document,
    output: Output,
}
impl std::ops::Deref for Writer<'_> {
    type Target = Output;
    fn deref(&self) -> &Output {
        &self.output
    }
}
impl std::ops::DerefMut for Writer<'_> {
    fn deref_mut(&mut self) -> &mut Output {
        &mut self.output
    }
}
impl Writer<'_> {
    fn hex(&mut self, out: &mut String, text: &str) -> Result<()> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in text.bytes() {
            let pair = [HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]];
            self.text(
                out,
                std::str::from_utf8(&pair).map_err(|_| Error::Reference)?,
            )?;
        }
        Ok(())
    }
    fn symbol(&mut self, kind: &str, interface: &InterfaceKey, name: &str) -> Result<String> {
        let mut out = String::new();
        self.text(&mut out, "component_")?;
        self.text(&mut out, kind)?;
        self.text(&mut out, "_")?;
        self.hex(&mut out, interface.package().namespace())?;
        self.hex(&mut out, ":")?;
        self.hex(&mut out, interface.package().package())?;
        self.hex(&mut out, "/")?;
        self.hex(&mut out, interface.interface())?;
        self.hex(&mut out, "#")?;
        self.hex(&mut out, name)?;
        Ok(out)
    }
    fn field(&mut self, out: &mut String, name: &str) -> Result<()> {
        if lang_syntax::is_declaration_identifier(name) && !name.starts_with("component_") {
            self.text(out, name)
        } else {
            self.text(out, "component_")?;
            self.hex(out, name)
        }
    }
    fn interface(&mut self, key: &InterfaceKey) -> Result<usize> {
        for (index, interface) in self.document.interfaces.iter().enumerate() {
            for k in [key, &interface.key.value] {
                self.budget.charge(
                    k.package()
                        .namespace()
                        .len()
                        .saturating_add(k.package().package().len())
                        .saturating_add(k.interface().len())
                        .saturating_add(1),
                )?;
            }
            if interface.key.value == *key {
                return Ok(index);
            }
        }
        Err(Error::Reference)
    }
    fn definition(&mut self, key: &idl::ValueKey) -> Result<(usize, usize)> {
        let interface = self.interface(&key.interface)?;
        for (index, definition) in self.document.interfaces[interface]
            .definitions
            .iter()
            .enumerate()
        {
            self.budget.charge(
                key.name
                    .len()
                    .saturating_add(definition.key.value.name.len())
                    .saturating_add(1),
            )?;
            if definition.key.value == *key {
                return Ok((interface, index));
            }
        }
        Err(Error::Reference)
    }
    fn function(&mut self, interface: usize, name: &str) -> Result<usize> {
        for (i, function) in self.document.interfaces[interface]
            .functions
            .iter()
            .enumerate()
        {
            self.budget.charge(
                name.len()
                    .saturating_add(function.name.value.len())
                    .saturating_add(1),
            )?;
            if function.name.value == name {
                return Ok(i);
            }
        }
        Err(Error::Reference)
    }
    fn key(&mut self, key: &InterfaceKey) -> Result<InterfaceKey> {
        for text in [
            key.package().namespace(),
            key.package().package(),
            key.interface(),
        ] {
            self.budget.charge(text.len().saturating_add(1))?;
            self.budget.bytes(text.len())?;
        }
        Ok(key.clone())
    }
    fn member(&mut self, key: &InterfaceKey, name: &str) -> Result<MemberKey> {
        let interface = self.key(key)?;
        let mut member = String::new();
        self.text(&mut member, name)?;
        Ok(MemberKey { interface, member })
    }
    fn ty(&mut self, out: &mut String, ty: idl::TypeId) -> Result<()> {
        enum Part {
            Type(idl::TypeId, usize),
            Text(&'static str),
        }
        let mut pending = self.budget.vector(1)?;
        pending.push(Part::Type(ty, 1));
        while let Some(part) = pending.pop() {
            self.budget.charge(1)?;
            let (ty, depth) = match part {
                Part::Text(text) => {
                    self.text(out, text)?;
                    continue;
                }
                Part::Type(ty, depth) => (ty, depth),
            };
            if depth > self.limits.depth {
                return Err(Error::Depth);
            }
            let kind = &self
                .document
                .type_nodes
                .get(ty.0)
                .ok_or(Error::Reference)?
                .kind;
            match kind {
                idl::TypeKind::I64 => self.text(out, "i64")?,
                idl::TypeKind::Bool => self.text(out, "bool")?,
                idl::TypeKind::Unit => self.text(out, "core::Unit")?,
                idl::TypeKind::Text => self.text(out, "core::Text")?,
                idl::TypeKind::Bytes => self.text(out, "core::Bytes")?,
                idl::TypeKind::List(child) | idl::TypeKind::Option(child) => {
                    self.text(
                        out,
                        if matches!(kind, idl::TypeKind::List(_)) {
                            "core::Vec<"
                        } else {
                            "core::Option<"
                        },
                    )?;
                    push(&mut self.budget, &mut pending, Part::Text(">"))?;
                    push(
                        &mut self.budget,
                        &mut pending,
                        Part::Type(*child, depth + 1),
                    )?;
                }
                idl::TypeKind::Result(ok, err) => {
                    self.text(out, "core::Result<")?;
                    push(&mut self.budget, &mut pending, Part::Text(">"))?;
                    push(&mut self.budget, &mut pending, Part::Type(*err, depth + 1))?;
                    push(&mut self.budget, &mut pending, Part::Text(","))?;
                    push(&mut self.budget, &mut pending, Part::Type(*ok, depth + 1))?;
                }
                idl::TypeKind::Named(key) => {
                    let (i, d) = self.definition(&key.value)?;
                    let definition = &self.document.interfaces[i].definitions[d];
                    if let idl::DefinitionKind::Alias(child) = definition.kind {
                        push(&mut self.budget, &mut pending, Part::Type(child, depth + 1))?;
                    } else {
                        let name = self.symbol("t", &key.value.interface, &key.value.name)?;
                        self.text(out, &name)?;
                    }
                }
                idl::TypeKind::Own(key) => {
                    self.text(out, "core::ComponentResource<")?;
                    let name = self.symbol("r", key.value.interface(), key.value.resource())?;
                    self.text(out, &name)?;
                    self.text(out, ">")?;
                }
                idl::TypeKind::Borrow { access, resource } => {
                    self.text(
                        out,
                        if *access == idl::Access::Shared {
                            "&core::ComponentResource<"
                        } else {
                            "&mut core::ComponentResource<"
                        },
                    )?;
                    let name =
                        self.symbol("r", resource.value.interface(), resource.value.resource())?;
                    self.text(out, &name)?;
                    self.text(out, ">")?;
                }
            }
        }
        Ok(())
    }
    fn signature(
        &mut self,
        out: &mut String,
        name: &str,
        function: &idl::Function,
        export: bool,
    ) -> Result<()> {
        if export {
            self.text(out, "pub ")?;
            if function.effect.value == idl::Effect::Task {
                self.text(out, "async ")?;
            }
        } else {
            self.text(out, "extern \"component\" ")?;
        }
        self.text(out, "fn ")?;
        self.text(out, name)?;
        self.text(out, "(")?;
        for (i, field) in function.params.iter().enumerate() {
            self.budget.charge(1)?;
            if i != 0 {
                self.text(out, ",")?;
            }
            self.text(out, "p")?;
            let mut digits = [0u8; 20];
            let value = decimal(i, &mut digits);
            self.text(out, value)?;
            self.text(out, ":")?;
            self.ty(out, field.ty)?;
        }
        self.text(out, ")->")?;
        self.ty(out, function.result)
    }
}
fn decimal(mut value: usize, buffer: &mut [u8; 20]) -> &str {
    let mut start = buffer.len();
    loop {
        start -= 1;
        buffer[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    std::str::from_utf8(&buffer[start..]).expect("ASCII digits")
}
fn push<T>(budget: &mut m::Budget, values: &mut Vec<T>, value: T) -> Result<()> {
    if values.len() == values.capacity() {
        let next = values
            .capacity()
            .max(4)
            .checked_mul(2)
            .ok_or(Error::OutputLimit)?;
        let mut output = budget.vector(next)?;
        budget.charge(values.len())?;
        output.append(values);
        *values = output;
    }
    budget.charge(1)?;
    values.push(value);
    Ok(())
}

pub fn guest(document: &idl::CheckedDocument, limits: Limits) -> Result<Guest> {
    if document.usage().data_depth > limits.depth {
        return Err(Error::Depth);
    }
    let mut writer = Writer {
        document: document.document(),
        output: Output {
            limits,
            budget: m::Budget::with_usage(limits.budget, document.usage().budget)?,
            bytes: 0,
        },
    };
    let mut declarations = String::new();
    let mut resources = writer.budget.vector(0)?;
    let mut origins = writer.budget.vector(0)?;
    let mut imports = writer.budget.vector(0)?;
    let mut exports = writer.budget.vector(0)?;
    let mut exported = writer.budget.vector(0)?;
    let doc = document.document();
    for interface in &doc.interfaces {
        for resource in &interface.resources {
            let start = declarations.len();
            let tag = writer.symbol("r", &interface.key.value, resource.value.resource())?;
            writer.text(&mut declarations, "pub struct ")?;
            writer.text(&mut declarations, &tag)?;
            writer.text(&mut declarations, " {}\n")?;
            let key = writer.key(resource.value.interface())?;
            let mut name = String::new();
            writer.text(&mut name, resource.value.resource())?;
            let mut retained = String::new();
            writer.text(&mut retained, &tag)?;
            push(
                &mut writer.budget,
                &mut resources,
                source::Resource {
                    tag: retained,
                    resource: ResourceTypeKey::new(key, name),
                },
            )?;
            push(
                &mut writer.budget,
                &mut origins,
                Origin {
                    start,
                    end: declarations.len(),
                    contract: resource.span,
                },
            )?;
        }
        for definition in &interface.definitions {
            let fields = match &definition.kind {
                idl::DefinitionKind::Record(fields) | idl::DefinitionKind::Variant(fields) => {
                    fields
                }
                idl::DefinitionKind::Alias(_) => continue,
            };
            let variant = matches!(definition.kind, idl::DefinitionKind::Variant(_));
            let start = declarations.len();
            let name = writer.symbol(
                "t",
                &definition.key.value.interface,
                &definition.key.value.name,
            )?;
            writer.text(
                &mut declarations,
                if variant { "pub enum " } else { "pub struct " },
            )?;
            writer.text(&mut declarations, &name)?;
            writer.text(&mut declarations, " {\n")?;
            for field in fields {
                let start = declarations.len();
                writer.budget.charge(1)?;
                writer.field(&mut declarations, &field.name.value)?;
                writer.text(&mut declarations, if variant { "(" } else { ":" })?;
                writer.ty(&mut declarations, field.ty)?;
                writer.text(&mut declarations, if variant { "),\n" } else { ",\n" })?;
                push(
                    &mut writer.budget,
                    &mut origins,
                    Origin {
                        start,
                        end: declarations.len(),
                        contract: field.name.span,
                    },
                )?;
            }
            writer.text(&mut declarations, "}\n")?;
            push(
                &mut writer.budget,
                &mut origins,
                Origin {
                    start,
                    end: declarations.len(),
                    contract: definition.span,
                },
            )?;
        }
    }
    for projection in &doc.imports {
        let i = writer.interface(&projection.interface.value)?;
        for member in &projection.members {
            let index = writer.function(i, &member.value)?;
            let function = &doc.interfaces[i].functions[index];
            let name = writer.symbol("i", &projection.interface.value, &member.value)?;
            let start = declarations.len();
            writer.signature(&mut declarations, &name, function, false)?;
            writer.text(&mut declarations, ";\n")?;
            let member = writer.member(&projection.interface.value, &member.value)?;
            push(
                &mut writer.budget,
                &mut imports,
                source::Import {
                    source: name,
                    member,
                },
            )?;
            push(
                &mut writer.budget,
                &mut origins,
                Origin {
                    start,
                    end: declarations.len(),
                    contract: function.span,
                },
            )?;
        }
    }
    for interface in &doc.exports {
        let i = writer.interface(&interface.value)?;
        for function in &doc.interfaces[i].functions {
            let name = writer.symbol("e", &interface.value, &function.name.value)?;
            let mut declaration = String::new();
            writer.signature(&mut declaration, &name, function, true)?;
            let member = writer.member(&interface.value, &function.name.value)?;
            let mut retained = String::new();
            writer.text(&mut retained, &name)?;
            let key = writer.member(&interface.value, &function.name.value)?;
            push(
                &mut writer.budget,
                &mut exports,
                source::Export {
                    source: retained,
                    member: key,
                },
            )?;
            push(
                &mut writer.budget,
                &mut exported,
                Export {
                    member,
                    source: name,
                    declaration,
                    contract_span: function.span,
                },
            )?;
        }
    }
    let usage = m::Usage {
        work: writer.budget.work_used(),
        allocation_bytes: writer.budget.allocation_bytes(),
        elements: writer.budget.elements_used(),
    };
    Ok(Guest {
        declarations,
        bindings: source::Input {
            version: 1,
            resources,
            imports,
            exports,
        },
        exports: exported,
        usage,
        text_bytes: writer.bytes,
        origins,
    })
}
