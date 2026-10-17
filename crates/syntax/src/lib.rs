//! Lossless source syntax for accepted RFCs 0002 and 0006.
//!
//! UTF-8 ingestion precedes tokenization; malformed byte input carries raw byte
//! ranges, never invented spans into a different string. Tokens retain trivia
//! and malformed text as validated ranges into immutable source snapshots.

pub mod format;
mod lexer;
mod parser;

pub use lexer::{
    InputError, InputErrorKind, LexError, LexLimits, Lexed, Token, TokenKind, add_source, lex,
};
pub use parser::{
    ExpectedSyntax, NodeId, ParseError, ParseLimits, SyntaxElement, SyntaxKind, SyntaxNode,
    SyntaxTree, parse, parse_expression,
};
