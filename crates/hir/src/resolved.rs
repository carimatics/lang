//! Backend-independent, name-resolved input for RFC 0006 program typing.
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
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PatternId(pub usize);
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BlockId(pub usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Type {
    I64,
    Bool,
    Nominal(DefinitionId),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostImport {
    pub module: ModuleId,
    pub name: Name,
    pub qualified_name: String,
    /// RFC 0006 fixes host signatures to exactly `(i64) -> i64`.
    pub parameter: Name,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BindingKind {
    Parameter,
    Mutable,
    Pattern,
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
    Integer(i64),
    Boolean(bool),
    Local(LocalId),
    Add(ExpressionId, ExpressionId),
    LessThan(ExpressionId, ExpressionId),
    Call {
        callee: Callable,
        arguments: Vec<ExpressionId>,
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
    LetMut {
        local: LocalId,
        initializer: ExpressionId,
    },
    Assign {
        local: LocalId,
        value: ExpressionId,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    pub modules: Vec<Module>,
    pub definitions: Vec<Definition>,
    pub imports: Vec<HostImport>,
    pub functions: Vec<Function>,
}
