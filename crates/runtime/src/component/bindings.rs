//! Independent component/MIR correspondence. This module grants no runtime authority.

use super::{
    idl,
    negotiation::{InterfaceKey, ResourceTypeKey},
};
use lang_mir::unit::{self as m, ownership as o};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberKey {
    pub interface: InterfaceKey,
    pub member: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceBinding {
    pub tag: m::DefinitionId,
    pub resource: ResourceTypeKey,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportBinding {
    pub source: m::ImportId,
    pub member: MemberKey,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExportBinding {
    pub source: m::FunctionId,
    pub member: MemberKey,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Map {
    pub version: u32,
    pub resources: Vec<ResourceBinding>,
    pub imports: Vec<ImportBinding>,
    pub exports: Vec<ExportBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Code {
    Owned(o::failure::Elaborated),
    Tasks(o::tasks::VerifiedTasks),
}
impl Code {
    pub fn declarations(&self) -> &o::declarations::VerifiedDeclarations {
        match self {
            Self::Owned(flow) => flow
                .source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations(),
            Self::Tasks(proof) => proof.typed().places().declarations(),
        }
    }
    pub fn usage(&self) -> m::Usage {
        let flow = match self {
            Self::Owned(flow) => flow,
            Self::Tasks(proof) => proof.plan(),
        };
        m::Usage {
            work: flow.work_used(),
            allocation_bytes: flow.requested_bytes(),
            elements: flow.elements_used(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub bindings: usize,
    pub depth: usize,
    pub budget: m::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bindings: 65_536,
            depth: 128,
            budget: Default::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Section {
    Resources,
    Imports,
    Exports,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Budget(m::ErrorKind),
    Version(u32),
    BindingLimit,
    Depth,
    InvalidSource,
    UnknownResource,
    UnknownMember,
    Duplicate,
    Missing,
    Abi,
    Effect,
    Arity,
    Type { actual: o::Type },
    Nominal,
    Name,
    Loan,
    TaskTransfer,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub source: Option<m::Span>,
    pub contract: Option<m::Span>,
    pub binding: Option<(Section, usize)>,
}
impl From<m::Error> for Error {
    fn from(error: m::Error) -> Self {
        Self {
            kind: ErrorKind::Budget(error.kind),
            source: None,
            contract: None,
            binding: None,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "component binding proof failed: {self:?}")
    }
}
impl std::error::Error for Error {}
impl Error {
    pub const fn code(&self) -> &'static str {
        match self.kind {
            ErrorKind::Budget(_) | ErrorKind::BindingLimit | ErrorKind::Depth => "CBND0001",
            ErrorKind::Version(_) => "CBND0002",
            ErrorKind::InvalidSource | ErrorKind::UnknownResource | ErrorKind::UnknownMember => {
                "CBND0003"
            }
            ErrorKind::Duplicate => "CBND0004",
            ErrorKind::Missing => "CBND0005",
            ErrorKind::Abi | ErrorKind::Effect | ErrorKind::TaskTransfer => "CBND0006",
            ErrorKind::Arity
            | ErrorKind::Type { .. }
            | ErrorKind::Nominal
            | ErrorKind::Name
            | ErrorKind::Loan => "CBND0007",
        }
    }
}
type Result<T> = std::result::Result<T, Error>;
fn error(kind: ErrorKind) -> Error {
    Error {
        kind,
        source: None,
        contract: None,
        binding: None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Function {
    pub interface: usize,
    pub member: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Resource {
    pub interface: usize,
    pub resource: usize,
}
#[derive(Debug)]
pub struct Unit {
    document: idl::CheckedDocument,
    code: Code,
    mappings: Map,
    resources: Vec<Option<Resource>>,
    definitions: Vec<Option<Definition>>,
    imports: Vec<Function>,
    exports: Vec<(m::FunctionId, Function)>,
    usage: m::Usage,
}
impl Unit {
    pub fn new(
        document: idl::CheckedDocument,
        code: Code,
        mappings: Map,
        limits: Limits,
    ) -> Result<Self> {
        Self::with_usage(
            document,
            code,
            mappings,
            limits,
            m::Usage {
                work: 0,
                allocation_bytes: 0,
                elements: 0,
            },
        )
    }
    /// Additional preparation usage is never subtracted or refunded.
    pub fn with_usage(
        document: idl::CheckedDocument,
        code: Code,
        mappings: Map,
        limits: Limits,
        prior: m::Usage,
    ) -> Result<Self> {
        let facts = validate(&document, &code, &mappings, limits, prior)?;
        Ok(Self {
            document,
            code,
            mappings,
            resources: facts.resources,
            definitions: facts.definitions,
            imports: facts.imports,
            exports: facts.exports,
            usage: facts.usage,
        })
    }
    pub fn document(&self) -> &idl::CheckedDocument {
        &self.document
    }
    pub fn code(&self) -> &Code {
        &self.code
    }
    pub fn mappings(&self) -> &Map {
        &self.mappings
    }
    pub fn resource(&self, tag: m::DefinitionId) -> Option<Resource> {
        self.resources.get(tag.0 as usize).copied().flatten()
    }
    pub fn definition(&self, source: m::DefinitionId) -> Option<Definition> {
        self.definitions.get(source.0 as usize).copied().flatten()
    }
    pub fn imports(&self) -> &[Function] {
        &self.imports
    }
    pub fn exports(&self) -> &[(m::FunctionId, Function)] {
        &self.exports
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
}
struct Facts {
    resources: Vec<Option<Resource>>,
    definitions: Vec<Option<Definition>>,
    imports: Vec<Function>,
    exports: Vec<(m::FunctionId, Function)>,
    usage: m::Usage,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Definition {
    pub interface: usize,
    pub definition: usize,
}
type Named = Definition;
struct Context<'a> {
    document: &'a idl::Document,
    declarations: &'a o::declarations::VerifiedDeclarations,
    budget: m::Budget,
    limits: Limits,
    resources: Vec<Option<Resource>>,
    named: Vec<Option<Named>>,
}
fn filled<T: Clone>(budget: &mut m::Budget, n: usize, value: T) -> Result<Vec<T>> {
    budget.charge(n)?;
    let mut out = budget.vector(n)?;
    out.resize(n, value);
    Ok(out)
}
fn push<T>(budget: &mut m::Budget, values: &mut Vec<T>, value: T) -> Result<()> {
    if values.len() == values.capacity() {
        let n = values
            .capacity()
            .max(4)
            .checked_mul(2)
            .ok_or_else(|| error(ErrorKind::BindingLimit))?;
        budget.charge(values.len())?;
        let mut next = budget.vector(n)?;
        next.append(values);
        *values = next;
    }
    budget.charge(1)?;
    values.push(value);
    Ok(())
}
fn interface_cost(key: &InterfaceKey) -> Result<usize> {
    key.package()
        .namespace()
        .len()
        .checked_add(key.package().package().len())
        .and_then(|n| n.checked_add(key.interface().len()))
        .and_then(|n| n.checked_add(1))
        .ok_or_else(|| error(ErrorKind::BindingLimit))
}
impl Context<'_> {
    fn interface(&mut self, key: &InterfaceKey) -> Result<usize> {
        let document = self.document;
        for (i, item) in document.interfaces.iter().enumerate() {
            self.budget
                .charge(interface_cost(key)?.saturating_add(interface_cost(&item.key.value)?))?;
            if item.key.value == *key {
                return Ok(i);
            }
        }
        Err(error(ErrorKind::UnknownMember))
    }
    fn member(&mut self, key: &MemberKey) -> Result<Function> {
        self.member_parts(&key.interface, &key.member)
    }
    fn member_parts(&mut self, key: &InterfaceKey, name: &str) -> Result<Function> {
        let interface = self.interface(key)?;
        for (member, function) in self.document.interfaces[interface]
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
                return Ok(Function { interface, member });
            }
        }
        Err(error(ErrorKind::UnknownMember))
    }
    fn resource(&mut self, key: &ResourceTypeKey) -> Result<Resource> {
        let interface = self.interface(key.interface()).map_err(|mut e| {
            e.kind = ErrorKind::UnknownResource;
            e
        })?;
        for (resource, item) in self.document.interfaces[interface]
            .resources
            .iter()
            .enumerate()
        {
            self.budget.charge(
                key.resource()
                    .len()
                    .saturating_add(item.value.resource().len())
                    .saturating_add(1),
            )?;
            if item.value == *key {
                return Ok(Resource {
                    interface,
                    resource,
                });
            }
        }
        Err(error(ErrorKind::UnknownResource))
    }
    fn constructor(&self, ty: o::Type) -> Option<o::Constructor> {
        let o::Type::Constructed(id) = ty else {
            return None;
        };
        self.declarations
            .types()
            .table()
            .constructed
            .get(id.0 as usize)
            .map(|t| t.constructor)
    }
    fn resource_type(&mut self, ty: o::Type, key: &ResourceTypeKey) -> Result<()> {
        let Some(o::Constructor::ComponentResource(tag)) = self.constructor(ty) else {
            return Err(error(ErrorKind::Type { actual: ty }));
        };
        let resource = self.resource(key)?;
        if self.resources.get(tag.0 as usize).copied().flatten() != Some(resource) {
            return Err(error(ErrorKind::UnknownResource));
        }
        Ok(())
    }
    fn named(&mut self, key: &idl::ValueKey) -> Result<Named> {
        let interface = self.interface(&key.interface)?;
        for (definition, item) in self.document.interfaces[interface]
            .definitions
            .iter()
            .enumerate()
        {
            self.budget.charge(
                key.name
                    .len()
                    .saturating_add(item.key.value.name.len())
                    .saturating_add(1),
            )?;
            if item.key.value == *key {
                return Ok(Named {
                    interface,
                    definition,
                });
            }
        }
        Err(error(ErrorKind::UnknownMember))
    }
    fn field_name(&mut self, source: &str, expected: &str) -> Result<()> {
        self.budget.charge(
            source
                .len()
                .saturating_add(expected.len())
                .saturating_add(1),
        )?;
        if source == expected {
            return Ok(());
        }
        let Some(encoded) = source.strip_prefix("component_") else {
            return Err(error(ErrorKind::Name));
        };
        let Some(length) = expected.len().checked_mul(2) else {
            return Err(error(ErrorKind::Name));
        };
        if encoded.len() != length {
            return Err(error(ErrorKind::Name));
        }
        let hex = b"0123456789abcdef";
        for (pair, byte) in encoded
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .zip(expected.bytes())
        {
            if *pair != [hex[(byte >> 4) as usize], hex[(byte & 15) as usize]] {
                return Err(error(ErrorKind::Name));
            }
        }
        Ok(())
    }
    fn unit_type(&mut self, mut ty: idl::TypeId) -> Result<bool> {
        for _ in 0..=self.limits.depth {
            self.budget.charge(1)?;
            match &self.document.type_nodes[ty.0].kind {
                idl::TypeKind::Unit => return Ok(true),
                idl::TypeKind::Named(key) => {
                    let named = self.named(&key.value)?;
                    match self.document.interfaces[named.interface].definitions[named.definition]
                        .kind
                    {
                        idl::DefinitionKind::Alias(next) => ty = next,
                        _ => return Ok(false),
                    }
                }
                _ => return Ok(false),
            }
        }
        Err(error(ErrorKind::Depth))
    }
    fn ty(&mut self, source: o::TypeUse, expected: idl::TypeId, parameter: bool) -> Result<()> {
        let document = self.document;
        let declarations = self.declarations;
        let mut pending = self.budget.vector(1)?;
        pending.push((source, expected, parameter, 1usize));
        while let Some((source, expected, parameter, depth)) = pending.pop() {
            self.budget.charge(1)?;
            if depth > self.limits.depth {
                return Err(error(ErrorKind::Depth));
            }
            let node = &document.type_nodes[expected.0];
            let child = |ty| o::TypeUse {
                ty,
                span: source.span,
            };
            let step = |pending: &mut Vec<_>, budget: &mut m::Budget, s, e| {
                push(budget, pending, (s, e, false, depth + 1))
            };
            let mismatch = || Error {
                kind: ErrorKind::Type { actual: source.ty },
                source: Some(source.span),
                contract: Some(node.span),
                binding: None,
            };
            match &node.kind {
                idl::TypeKind::I64 if source.ty == o::Type::I64 => {}
                idl::TypeKind::Bool if source.ty == o::Type::Bool => {}
                idl::TypeKind::Unit | idl::TypeKind::Text | idl::TypeKind::Bytes => {
                    let builtin = match node.kind {
                        idl::TypeKind::Unit => o::Builtin::Unit,
                        idl::TypeKind::Text => o::Builtin::Text,
                        _ => o::Builtin::Bytes,
                    };
                    if self.constructor(source.ty) != Some(o::Constructor::Builtin(builtin)) {
                        return Err(mismatch());
                    }
                }
                idl::TypeKind::List(expected) | idl::TypeKind::Option(expected) => {
                    let actual = match (self.constructor(source.ty), &node.kind) {
                        (Some(o::Constructor::Vec(t)), idl::TypeKind::List(_))
                        | (Some(o::Constructor::Option(t)), idl::TypeKind::Option(_)) => t,
                        _ => return Err(mismatch()),
                    };
                    step(&mut pending, &mut self.budget, child(actual), *expected)?;
                }
                idl::TypeKind::Result(ok, err) => {
                    let Some(o::Constructor::Result(a, b)) = self.constructor(source.ty) else {
                        return Err(mismatch());
                    };
                    step(&mut pending, &mut self.budget, child(b), *err)?;
                    step(&mut pending, &mut self.budget, child(a), *ok)?;
                }
                idl::TypeKind::Own(key) => {
                    self.resource_type(source.ty, &key.value).map_err(|mut e| {
                        e.source = Some(source.span);
                        e.contract = Some(node.span);
                        e
                    })?
                }
                idl::TypeKind::Borrow { access, resource } => {
                    if !parameter {
                        return Err(error(ErrorKind::Loan));
                    }
                    let kind = match access {
                        idl::Access::Shared => o::LoanKind::Shared,
                        idl::Access::Exclusive => o::LoanKind::Exclusive,
                    };
                    let Some(o::Constructor::Loan(actual, target)) = self.constructor(source.ty)
                    else {
                        return Err(mismatch());
                    };
                    if actual != kind {
                        return Err(mismatch());
                    }
                    self.resource_type(target, &resource.value)?;
                }
                idl::TypeKind::Named(key) => {
                    let named = self.named(&key.value)?;
                    let definition =
                        &document.interfaces[named.interface].definitions[named.definition];
                    if let idl::DefinitionKind::Alias(next) = definition.kind {
                        push(
                            &mut self.budget,
                            &mut pending,
                            (source, next, parameter, depth + 1),
                        )?;
                        continue;
                    }
                    let o::Type::Nominal(id) = source.ty else {
                        return Err(mismatch());
                    };
                    if let Some(previous) = self.named[id.0 as usize] {
                        if previous != named {
                            return Err(error(ErrorKind::Nominal));
                        }
                        continue;
                    }
                    for previous in &self.named {
                        self.budget.charge(1)?;
                        if *previous == Some(named) {
                            return Err(error(ErrorKind::Nominal));
                        }
                    }
                    self.named[id.0 as usize] = Some(named);
                    let actual = &declarations.types().table().definitions[id.0 as usize];
                    match (&actual.kind, &definition.kind) {
                        (
                            o::DefinitionKind::Record(fields),
                            idl::DefinitionKind::Record(expected),
                        ) => {
                            if fields.len() != expected.len() {
                                return Err(error(ErrorKind::Arity));
                            }
                            for (a, b) in fields.iter().zip(expected) {
                                self.field_name(&a.name, &b.name.value).map_err(|mut e| {
                                    e.source = Some(a.span);
                                    e.contract = Some(b.name.span);
                                    e
                                })?;
                                step(&mut pending, &mut self.budget, a.ty, b.ty)?;
                            }
                        }
                        (
                            o::DefinitionKind::Enum(cases),
                            idl::DefinitionKind::Variant(expected),
                        ) => {
                            if cases.len() != expected.len() {
                                return Err(error(ErrorKind::Arity));
                            }
                            for (a, b) in cases.iter().zip(expected) {
                                self.field_name(&a.name, &b.name.value).map_err(|mut e| {
                                    e.source = Some(a.span);
                                    e.contract = Some(b.name.span);
                                    e
                                })?;
                                if a.payload.is_empty() && self.unit_type(b.ty)? {
                                    continue;
                                }
                                if a.payload.len() != 1 {
                                    return Err(error(ErrorKind::Arity));
                                }
                                step(&mut pending, &mut self.budget, a.payload[0], b.ty)?;
                            }
                        }
                        _ => return Err(mismatch()),
                    }
                }
                _ => return Err(mismatch()),
            }
        }
        Ok(())
    }
}

fn validate(
    document: &idl::CheckedDocument,
    code: &Code,
    map: &Map,
    limits: Limits,
    prior: m::Usage,
) -> Result<Facts> {
    if map.version != 1 {
        return Err(error(ErrorKind::Version(map.version)));
    }
    let count = map
        .resources
        .len()
        .checked_add(map.imports.len())
        .and_then(|n| n.checked_add(map.exports.len()))
        .ok_or_else(|| error(ErrorKind::BindingLimit))?;
    if count > limits.bindings {
        return Err(error(ErrorKind::BindingLimit));
    }
    if document.usage().data_depth > limits.depth {
        return Err(error(ErrorKind::Depth));
    }
    let base = preparation_usage(document, code)?;
    let a = base;
    let b = prior;
    let add = |a: usize, b: usize| {
        a.checked_add(b)
            .ok_or_else(|| error(ErrorKind::BindingLimit))
    };
    let mut budget = m::Budget::with_usage(
        limits.budget,
        m::Usage {
            work: add(a.work, b.work)?,
            allocation_bytes: add(a.allocation_bytes, b.allocation_bytes)?,
            elements: add(a.elements, b.elements)?,
        },
    )?;
    let declarations = code.declarations();
    if matches!(code, Code::Owned(_))
        && let Some(span) = declarations.task_feature_span()
    {
        return Err(Error {
            kind: ErrorKind::Effect,
            source: Some(span),
            contract: None,
            binding: None,
        });
    }
    for function in declarations.functions() {
        budget.charge(1)?;
        if function.abi != o::declarations::FunctionAbi::Guest {
            return Err(Error {
                kind: ErrorKind::Abi,
                source: Some(function.signature_span),
                contract: None,
                binding: None,
            });
        }
    }
    let resources = filled(
        &mut budget,
        declarations.types().table().definitions.len(),
        None,
    )?;
    let named = filled(
        &mut budget,
        declarations.types().table().definitions.len(),
        None,
    )?;
    let mut c = Context {
        document: document.document(),
        declarations,
        budget,
        limits,
        resources,
        named,
    };
    for (index, binding) in map.resources.iter().enumerate() {
        c.budget.charge(1)?;
        let checked = (|| {
            let slot = binding.tag.0 as usize;
            if slot >= c.resources.len() {
                return Err(error(ErrorKind::InvalidSource));
            }
            if c.resources[slot].is_some() {
                return Err(error(ErrorKind::Duplicate));
            }
            let target = c.resource(&binding.resource)?;
            for previous in &c.resources {
                c.budget.charge(1)?;
                if *previous == Some(target) {
                    return Err(error(ErrorKind::Duplicate));
                }
            }
            c.resources[slot] = Some(target);
            Ok(())
        })();
        checked.map_err(|mut e| {
            e.binding = Some((Section::Resources, index));
            if e.source.is_none() {
                e.source = declarations
                    .types()
                    .table()
                    .definitions
                    .get(binding.tag.0 as usize)
                    .map(|d| d.span);
            }
            e
        })?;
    }
    for item in &declarations.types().table().constructed {
        c.budget.charge(1)?;
        if let o::Constructor::ComponentResource(tag) = item.constructor
            && c.resources[tag.0 as usize].is_none()
        {
            return Err(Error {
                kind: ErrorKind::Missing,
                source: Some(item.span),
                contract: None,
                binding: None,
            });
        }
    }
    let mut imports = filled(&mut c.budget, declarations.imports().len(), None)?;
    let mut exports = c.budget.vector(map.exports.len())?;
    for (index, binding) in map.imports.iter().enumerate() {
        let checked = (|| {
            c.budget.charge(1)?;
            let source = declarations
                .imports()
                .get(binding.source.0 as usize)
                .ok_or_else(|| error(ErrorKind::InvalidSource))?;
            if source.abi != o::declarations::ImportAbi::Component {
                return Err(error(ErrorKind::Abi));
            }
            if imports[binding.source.0 as usize].is_some() {
                return Err(error(ErrorKind::Duplicate));
            }
            let target = c.member(&binding.member)?;
            for previous in &imports {
                c.budget.charge(1)?;
                if *previous == Some(target) {
                    return Err(error(ErrorKind::Duplicate));
                }
            }
            let mut required = false;
            for requirement in &c.document.imports {
                c.budget
                    .charge(interface_cost(&requirement.interface.value)?)?;
                if requirement.interface.value == binding.member.interface {
                    for member in &requirement.members {
                        c.budget.charge(
                            member
                                .value
                                .len()
                                .saturating_add(binding.member.member.len())
                                .saturating_add(1),
                        )?;
                        required |= member.value == binding.member.member;
                    }
                }
            }
            if !required {
                return Err(error(ErrorKind::UnknownMember));
            }
            let expected = &c.document.interfaces[target.interface].functions[target.member];
            if expected.effect.value != idl::Effect::Sync {
                return Err(error(ErrorKind::Effect));
            }
            signature(
                &mut c,
                &source.parameters,
                source.result,
                expected,
                false,
                source.span,
            )?;
            imports[binding.source.0 as usize] = Some(target);
            Ok(())
        })();
        checked.map_err(|mut e| {
            e.binding = Some((Section::Imports, index));
            if e.source.is_none() {
                e.source = declarations
                    .imports()
                    .get(binding.source.0 as usize)
                    .map(|d| d.span);
            }
            e
        })?;
    }
    for (index, binding) in map.exports.iter().enumerate() {
        let checked = (|| {
            c.budget.charge(1)?;
            let source = declarations
                .functions()
                .get(binding.source.0 as usize)
                .ok_or_else(|| error(ErrorKind::InvalidSource))?;
            if !source.public || source.abi != o::declarations::FunctionAbi::Guest {
                return Err(error(ErrorKind::Abi));
            }
            let target = c.member(&binding.member)?;
            for (_, previous) in &exports {
                c.budget.charge(1)?;
                if *previous == target {
                    return Err(error(ErrorKind::Duplicate));
                }
            }
            let mut required = false;
            for key in &c.document.exports {
                c.budget.charge(interface_cost(&key.value)?)?;
                required |= key.value == binding.member.interface;
            }
            if !required {
                return Err(error(ErrorKind::UnknownMember));
            }
            let expected = &c.document.interfaces[target.interface].functions[target.member];
            if source.effect.is_async() != (expected.effect.value == idl::Effect::Task) {
                return Err(error(ErrorKind::Effect));
            }
            let mut parameters = c.budget.vector(source.parameters.len())?;
            for local in &source.parameters {
                c.budget.charge(1)?;
                parameters.push(source.locals[local.0 as usize].ty);
            }
            signature(
                &mut c,
                &parameters,
                source.result,
                expected,
                source.effect.is_async(),
                source.signature_span,
            )?;
            exports.push((binding.source, target));
            Ok(())
        })();
        checked.map_err(|mut e| {
            e.binding = Some((Section::Exports, index));
            if e.source.is_none() {
                e.source = declarations
                    .functions()
                    .get(binding.source.0 as usize)
                    .map(|d| d.signature_span);
            }
            e
        })?;
    }
    for (index, source) in declarations.imports().iter().enumerate() {
        c.budget.charge(1)?;
        if imports[index].is_none() {
            return Err(Error {
                kind: ErrorKind::Missing,
                source: Some(source.span),
                contract: None,
                binding: None,
            });
        }
    }
    for requirement in &c.document.imports {
        for member in &requirement.members {
            let target = c.member_parts(&requirement.interface.value, &member.value)?;
            let mut found = false;
            for previous in &imports {
                c.budget.charge(1)?;
                found |= *previous == Some(target);
            }
            if !found {
                return Err(Error {
                    kind: ErrorKind::Missing,
                    source: None,
                    contract: Some(member.span),
                    binding: None,
                });
            }
        }
    }
    for key in &c.document.exports {
        let interface = c.interface(&key.value)?;
        for member in 0..c.document.interfaces[interface].functions.len() {
            let mut found = false;
            for (_, previous) in &exports {
                c.budget.charge(1)?;
                found |= *previous == Function { interface, member };
            }
            if !found {
                return Err(Error {
                    kind: ErrorKind::Missing,
                    source: None,
                    contract: Some(key.span),
                    binding: None,
                });
            }
        }
    }
    let mut checked = c.budget.vector(imports.len())?;
    for entry in imports {
        c.budget.charge(1)?;
        checked.push(entry.ok_or_else(|| error(ErrorKind::Missing))?);
    }
    let usage = m::Usage {
        work: c.budget.work_used(),
        allocation_bytes: c.budget.allocation_bytes(),
        elements: c.budget.elements_used(),
    };
    Ok(Facts {
        resources: c.resources,
        definitions: c.named,
        imports: checked,
        exports,
        usage,
    })
}
pub fn preparation_usage(document: &idl::CheckedDocument, code: &Code) -> Result<m::Usage> {
    let a = document.usage().budget;
    let b = code.usage();
    let add = |a: usize, b: usize| {
        a.checked_add(b)
            .ok_or_else(|| error(ErrorKind::BindingLimit))
    };
    Ok(m::Usage {
        work: add(a.work, b.work)?,
        allocation_bytes: add(a.allocation_bytes, b.allocation_bytes)?,
        elements: add(a.elements, b.elements)?,
    })
}
fn signature(
    c: &mut Context<'_>,
    parameters: &[o::TypeUse],
    result: o::TypeUse,
    expected: &idl::Function,
    task: bool,
    span: m::Span,
) -> Result<()> {
    if parameters.len() != expected.params.len() {
        return Err(Error {
            kind: ErrorKind::Arity,
            source: Some(span),
            contract: Some(expected.span),
            binding: None,
        });
    }
    for (source, target) in parameters.iter().zip(&expected.params) {
        c.budget.charge(1)?;
        if task
            && !c
                .declarations
                .types()
                .properties(source.ty)
                .is_some_and(|p| p.is_send())
        {
            return Err(error(ErrorKind::TaskTransfer));
        }
        c.ty(*source, target.ty, true).map_err(|mut e| {
            e.source.get_or_insert(source.span);
            e.contract
                .get_or_insert(c.document.type_nodes[target.ty.0].span);
            e
        })?;
    }
    if task
        && !c
            .declarations
            .types()
            .properties(result.ty)
            .is_some_and(|p| p.is_send())
    {
        return Err(error(ErrorKind::TaskTransfer));
    }
    c.ty(result, expected.result, false).map_err(|mut e| {
        e.source.get_or_insert(result.span);
        e.contract
            .get_or_insert(c.document.type_nodes[expected.result.0].span);
        e
    })
}
