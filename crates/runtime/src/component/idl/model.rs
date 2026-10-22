use super::super::negotiation::{
    InterfaceKey, PackageKey, ResourceTypeKey, SemanticVersion, VersionRequirement,
};
use super::{Spanned, m};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct ValueKey {
    pub interface: InterfaceKey,
    pub name: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeId(pub usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Access {
    Shared,
    Exclusive,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    Sync,
    Task,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeKind {
    Unit,
    I64,
    Bool,
    Text,
    Bytes,
    List(TypeId),
    Option(TypeId),
    Result(TypeId, TypeId),
    Named(Spanned<ValueKey>),
    Own(Spanned<ResourceTypeKey>),
    Borrow {
        access: Access,
        resource: Spanned<ResourceTypeKey>,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeNode {
    pub kind: TypeKind,
    pub span: m::Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    pub name: Spanned<String>,
    pub ty: TypeId,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DefinitionKind {
    Record(Vec<Field>),
    Variant(Vec<Field>),
    Alias(TypeId),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Definition {
    pub key: Spanned<ValueKey>,
    pub kind: DefinitionKind,
    pub span: m::Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub name: Spanned<String>,
    pub effect: Spanned<Effect>,
    pub params: Vec<Field>,
    pub result: TypeId,
    pub capability: Spanned<String>,
    pub span: m::Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Interface {
    pub key: Spanned<InterfaceKey>,
    pub resources: Vec<Spanned<ResourceTypeKey>>,
    pub definitions: Vec<Definition>,
    pub functions: Vec<Function>,
    pub span: m::Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Import {
    pub interface: Spanned<InterfaceKey>,
    pub requirement: Spanned<VersionRequirement>,
    pub members: Vec<Spanned<String>>,
    pub span: m::Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Document {
    pub source: m::SourceId,
    pub source_bytes: u32,
    pub package: Spanned<PackageKey>,
    pub version: Spanned<SemanticVersion>,
    pub interfaces: Vec<Interface>,
    pub imports: Vec<Import>,
    pub exports: Vec<Spanned<InterfaceKey>>,
    pub type_nodes: Vec<TypeNode>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Usage {
    pub budget: m::Usage,
    pub declarations: usize,
    pub elements: usize,
}

/// Immutable declaration proof. It is neither an executable nor a capability grant.
#[derive(Debug)]
pub struct CheckedDocument {
    pub(super) document: Document,
    pub(super) usage: Usage,
}
impl CheckedDocument {
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub const fn usage(&self) -> Usage {
        self.usage
    }
}
