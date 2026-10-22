//! Backend-independent, name-resolved input for RFC 0006/0007 program analysis.
//!
//! This is an in-memory compiler IR, not a serialized or executable artifact.
//! Types in signatures and annotations name declarations; expression, pattern,
//! and inferred binding types still require checking. IDs are arena indices
//! scoped to one compilation (or one function for local/body IDs). Consumers
//! that accept externally constructed input must validate IDs, spans, and limits.
//! No syntax-tree or code-generation type crosses this boundary.
//! Expression, pattern, and block arenas form source trees: each node is used
//! exactly once. Program typing rejects cycles, shared nodes, and orphan nodes.

use lang_diagnostics::Span;

use crate::source::Name;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ModuleId(pub usize);
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DefinitionId(pub usize);
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FunctionId(pub usize);
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ImportId(pub usize);
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct LocalId(pub usize);
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExpressionId(pub usize);
/// A place root in the function's expression arena. Binding admits only
/// locals, field chains, and explicit dereferences; type/loan validity is a
/// later check. This uses the expression budget, not a second storage arena.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PlaceId(pub usize);
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PatternId(pub usize);
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BlockId(pub usize);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Type {
    I64,
    Bool,
    Nominal(DefinitionId),
    Constructed(ConstructedTypeId),
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ConstructedTypeId(pub usize);
/// Compiler-created lexical authority. Local IDs alone are function-scoped;
/// this pair keeps owner identity distinct across the whole typed arena.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ArenaOwnerId {
    pub function: FunctionId,
    pub local: LocalId,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LoanKind {
    Shared,
    Exclusive,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BuiltinType {
    Unit,
    Text,
    Bytes,
    Allocator,
    AllocError,
    BoundsError,
    BufferError,
    Utf8Error,
    OwnershipError,
    ArenaError,
    TaskError,
    ChannelError,
    ServiceError,
}
impl BuiltinType {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unit => "Unit",
            Self::Text => "Text",
            Self::Bytes => "Bytes",
            Self::Allocator => "Allocator",
            Self::AllocError => "AllocError",
            Self::BoundsError => "BoundsError",
            Self::BufferError => "BufferError",
            Self::Utf8Error => "Utf8Error",
            Self::OwnershipError => "OwnershipError",
            Self::ArenaError => "ArenaError",
            Self::TaskError => "TaskError",
            Self::ChannelError => "ChannelError",
            Self::ServiceError => "ServiceError",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Unit,
            Self::Text,
            Self::Bytes,
            Self::Allocator,
            Self::AllocError,
            Self::BoundsError,
            Self::BufferError,
            Self::Utf8Error,
            Self::OwnershipError,
            Self::ArenaError,
            Self::TaskError,
            Self::ChannelError,
            Self::ServiceError,
        ]
        .into_iter()
        .find(|b| b.name() == name)
    }
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TypeConstructor {
    Builtin(BuiltinType),
    Option(Type),
    Result(Type, Type),
    SendResult(Type),
    Task(Type),
    ChannelSetup(Type),
    Sender(Type),
    Receiver(Type),
    ServiceSetup(Type, Type),
    ServiceOwner(Type, Type),
    ServiceHandle(Type),
    /// Opaque owner identity. The nominal tag is not a stored payload.
    ComponentResource(DefinitionId),
    Vec(Type),
    Map(Type, Type),
    Shared(Type),
    Weak(Type),
    Arena(Type),
    Loan(LoanKind, Type),
    /// Only the typed body arena may contain this constructor. Resolved source
    /// input cannot name, annotate, or specialize an arena-owner type.
    ArenaOwner(ArenaOwnerId),
}
impl TypeConstructor {
    pub fn arguments(self) -> impl Iterator<Item = Type> {
        let pair = match self {
            Self::Builtin(_) | Self::ArenaOwner(_) | Self::ComponentResource(_) => [None, None],
            Self::SendResult(t)
            | Self::Option(t)
            | Self::ChannelSetup(t)
            | Self::Sender(t)
            | Self::Receiver(t)
            | Self::ServiceHandle(t)
            | Self::Task(t)
            | Self::Vec(t)
            | Self::Shared(t)
            | Self::Weak(t)
            | Self::Arena(t)
            | Self::Loan(_, t) => [Some(t), None],
            Self::Result(t, e)
            | Self::Map(t, e)
            | Self::ServiceSetup(t, e)
            | Self::ServiceOwner(t, e) => [Some(t), Some(e)],
        };
        pair.into_iter().flatten()
    }
}
/// Interned by constructor identity. The first written occurrence supplies the
/// constructor span; every individual annotation retains its own TypeUse span.
/// Constructed arguments refer only to earlier entries; named definitions can
/// refer forward and may recurse through explicit indirection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstructedType {
    pub constructor: TypeConstructor,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeUse {
    pub value_type: Type,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    pub qualified_name: String,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    pub name: Name,
    pub value_type: TypeUse,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Case {
    pub name: Name,
    pub payload: Vec<TypeUse>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DefinitionKind {
    Record(Vec<Field>),
    Enum(Vec<Case>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Definition {
    pub module: ModuleId,
    pub name: Name,
    pub qualified_name: String,
    pub public: bool,
    pub kind: DefinitionKind,
    pub span: Span,
}

/// Binding identity is independent of its source name and scalar shape.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ImportAbi {
    #[default]
    Host,
    C,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FunctionAbi {
    #[default]
    Guest,
    CExport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportParameter {
    pub name: Name,
    pub ty: TypeUse,
}

/// Historical name retained for the resolved import table; abi distinguishes
/// RFC 0006 host bindings from RFC 0008 C declarations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostImport {
    pub abi: ImportAbi,
    pub module: ModuleId,
    pub name: Name,
    pub qualified_name: String,
    pub parameters: Vec<ImportParameter>,
    pub result: TypeUse,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingKind {
    Parameter,
    Mutable,
    Pattern,
    /// Compiler-created owner visible only inside its lexical arena body.
    ArenaOwner,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Local {
    pub name: Name,
    pub kind: BindingKind,
    /// Pattern binding types are inferred from a checked scrutinee.
    pub annotation: Option<TypeUse>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Callable {
    Guest(FunctionId),
    Host(ImportId),
    Case {
        definition: DefinitionId,
        index: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldValue {
    pub name: Name,
    pub value: ExpressionId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpressionKind {
    CoreUnit,
    CoreCase {
        case: crate::core::Case,
        payload: Vec<ExpressionId>,
        explicit_payload: bool,
        case_span: Span,
    },
    Integer(i64),
    Boolean(bool),
    Local(LocalId),
    Borrow {
        kind: LoanKind,
        place: PlaceId,
    },
    Dereference(ExpressionId),
    Spawn(ExpressionId),
    Await(ExpressionId),
    Add(ExpressionId, ExpressionId),
    LessThan(ExpressionId, ExpressionId),
    Call {
        callee: Callable,
        arguments: Vec<ExpressionId>,
    },
    CoreCall {
        operation: crate::core::Operation,
        type_arguments: Vec<TypeUse>,
        arguments: Vec<ExpressionId>,
        callee_span: Span,
    },
    /// A bare enum-case path, distinct from explicit call parentheses.
    Case {
        definition: DefinitionId,
        index: usize,
    },
    Record {
        definition: DefinitionId,
        fields: Vec<FieldValue>,
    },
    Field {
        receiver: ExpressionId,
        name: Name,
    },
}

impl ExpressionKind {
    /// Evaluated children in source order, independent of backend lowering.
    pub fn operands(&self) -> impl Iterator<Item = ExpressionId> + '_ {
        let (fixed, list, fields): ([Option<ExpressionId>; 2], &[ExpressionId], &[FieldValue]) =
            match self {
                Self::Borrow { place, .. } => ([Some(ExpressionId(place.0)), None], &[], &[]),
                Self::Dereference(value)
                | Self::Spawn(value)
                | Self::Await(value)
                | Self::Field {
                    receiver: value, ..
                } => ([Some(*value), None], &[], &[]),
                Self::Add(a, b) | Self::LessThan(a, b) => ([Some(*a), Some(*b)], &[], &[]),
                Self::Call { arguments, .. } | Self::CoreCall { arguments, .. } => {
                    ([None, None], arguments, &[])
                }
                Self::CoreCase { payload, .. } => ([None, None], payload, &[]),
                Self::Record { fields, .. } => ([None, None], &[], fields),
                _ => ([None, None], &[], &[]),
            };
        fixed
            .into_iter()
            .flatten()
            .chain(list.iter().copied())
            .chain(fields.iter().map(|field| field.value))
    }
}
impl StatementKind {
    /// Evaluated expressions in source order, including a captured destination.
    pub fn operands(&self) -> impl Iterator<Item = ExpressionId> {
        let values = match self {
            Self::LetMut { initializer, .. } => [Some(*initializer), None],
            Self::Assign { value, .. } | Self::Return(value) => [Some(*value), None],
            Self::PlaceAssign { place, value } => [Some(ExpressionId(place.0)), Some(*value)],
            Self::If { condition, .. } | Self::While { condition, .. } => [Some(*condition), None],
            Self::Match { scrutinee, .. } => [Some(*scrutinee), None],
            Self::Arena { allocator, .. } => [Some(*allocator), None],
            Self::Cancel(value) => [Some(*value), None],
            Self::Unsafe { .. } | Self::TaskScope { .. } | Self::Yield | Self::CancelCheck => {
                [None, None]
            }
        };
        values.into_iter().flatten()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternField {
    pub name: Name,
    pub pattern: PatternId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PatternKind {
    CoreUnit,
    CoreCase {
        case: crate::core::Case,
        payload: Vec<PatternId>,
        explicit_payload: bool,
        case_span: Span,
    },
    Wildcard,
    Binding(LocalId),
    Integer(i64),
    Boolean(bool),
    Record {
        definition: DefinitionId,
        fields: Vec<PatternField>,
    },
    Case {
        definition: DefinitionId,
        index: usize,
        payload: Vec<PatternId>,
        explicit_payload: bool,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Arm {
    pub pattern: PatternId,
    pub body: BlockId,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StatementKind {
    TaskScope {
        body: BlockId,
    },
    Yield,
    Cancel(ExpressionId),
    CancelCheck,
    /// A normal lexical scope with an additional foreign-call obligation marker.
    Unsafe {
        body: BlockId,
    },
    LetMut {
        local: LocalId,
        initializer: ExpressionId,
    },
    Assign {
        local: LocalId,
        value: ExpressionId,
    },
    PlaceAssign {
        place: PlaceId,
        value: ExpressionId,
    },
    Arena {
        owner: LocalId,
        allocator: ExpressionId,
        body: BlockId,
    },
    If {
        condition: ExpressionId,
        then_block: BlockId,
        else_block: Option<BlockId>,
    },
    While {
        condition: ExpressionId,
        body: BlockId,
    },
    Return(ExpressionId),
    Match {
        scrutinee: ExpressionId,
        arms: Vec<Arm>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
    pub closing_brace: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub effect: FunctionEffect,
    pub abi: FunctionAbi,
    pub module: ModuleId,
    pub name: Name,
    pub qualified_name: String,
    pub public: bool,
    pub parameters: Vec<LocalId>,
    pub result: TypeUse,
    pub locals: Vec<Local>,
    pub expressions: Vec<Expression>,
    pub patterns: Vec<Pattern>,
    pub blocks: Vec<Block>,
    pub body: BlockId,
    pub span: Span,
    pub signature_span: Span,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FunctionEffect {
    #[default]
    Synchronous,
    Asynchronous {
        span: Span,
    },
}
impl FunctionEffect {
    pub const fn is_async(self) -> bool {
        matches!(self, Self::Asynchronous { .. })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    pub modules: Vec<Module>,
    pub definitions: Vec<Definition>,
    pub types: Vec<ConstructedType>,
    pub imports: Vec<HostImport>,
    pub functions: Vec<Function>,
}
