//! Syntax-independent input for the accepted RFC 0002 subset.
//!
//! A future parser supplies these nodes after lexical/syntactic validation.
//! This module does not tokenize text, choose identifier rules, infer omitted
//! type annotations, or admit the proposed RFC 0006 extensions. Parameters,
//! mutable declarations, and results are explicitly `i64` in this profile.
//! Arenas avoid recursive ownership/drop even for maliciously linked inputs.

use lang_diagnostics::Span;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ExpressionId(pub usize);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BlockId(pub usize);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Name {
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpressionKind {
    /// The parser must already have checked the signed decimal's i64 range.
    Integer(i64),
    Name(Name),
    Add(ExpressionId, ExpressionId),
    LessThan(ExpressionId, ExpressionId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Statement {
    pub kind: StatementKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StatementKind {
    LetMut {
        name: Name,
        initializer: ExpressionId,
    },
    Assign {
        target: Name,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
    pub closing_brace: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub name: Name,
    pub parameters: Vec<Name>,
    pub expressions: Vec<Expression>,
    pub blocks: Vec<Block>,
    pub body: BlockId,
    pub span: Span,
    pub signature_span: Span,
}
