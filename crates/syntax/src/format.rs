//! Whitespace-only CST formatting. Real token/comment bytes are preserved;
//! a malformed tree returns its original snapshot without dropping tokens.
use crate::{SyntaxElement, SyntaxKind as S, SyntaxTree, TokenKind as K};
use lang_diagnostics::{SourceError, SourceMap};
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub work: usize,
    pub output_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            work: 10_000_000,
            output_bytes: 16 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    Formatted,
    PreservedInvalid,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Formatted {
    pub text: String,
    pub status: Status,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Work,
    OutputBytes,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FormatError {
    Source(SourceError),
    Limit(LimitKind),
    AllocationFailed,
    InvalidTree,
}
impl Display for FormatError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "source formatting failed: {self:?}")
    }
}
impl Error for FormatError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Brace {
    Declaration,
    Block,
    Match,
    Value,
}
fn brace(owner: S) -> Result<Brace, FormatError> {
    match owner {
        S::Record | S::Enum => Ok(Brace::Declaration),
        S::Block => Ok(Brace::Block),
        S::Match => Ok(Brace::Match),
        S::Construction | S::RecordPattern => Ok(Brace::Value),
        _ => Err(FormatError::InvalidTree),
    }
}
enum Delimiter {
    Paren,
    TypeArguments,
    Brace { kind: Brace, empty: bool },
}
struct Writer {
    text: String,
    indent: usize,
    line_start: bool,
    space: bool,
    line: bool,
    work: usize,
    limits: Limits,
}
impl Writer {
    fn charge(&mut self, n: usize) -> Result<(), FormatError> {
        self.work = self
            .work
            .checked_add(n)
            .filter(|n| *n <= self.limits.work)
            .ok_or(FormatError::Limit(LimitKind::Work))?;
        Ok(())
    }
    fn raw(&mut self, text: &str) -> Result<(), FormatError> {
        self.charge(text.len().div_ceil(1024) + 1)?;
        if self
            .text
            .len()
            .checked_add(text.len())
            .is_none_or(|n| n > self.limits.output_bytes)
        {
            return Err(FormatError::Limit(LimitKind::OutputBytes));
        }
        self.text
            .try_reserve(text.len())
            .map_err(|_| FormatError::AllocationFailed)?;
        self.text.push_str(text);
        self.line_start = self.text.ends_with('\n');
        Ok(())
    }
    fn newline(&mut self) -> Result<(), FormatError> {
        if !self.text.is_empty() && !self.line_start {
            self.raw("\n")?;
        }
        self.space = false;
        self.line = false;
        Ok(())
    }
    fn token(&mut self, text: &str) -> Result<(), FormatError> {
        if self.line {
            self.newline()?;
        }
        if self.line_start {
            for _ in 0..self.indent {
                self.raw("    ")?;
            }
        } else if self.space {
            self.raw(" ")?;
        }
        self.space = false;
        self.raw(text)
    }
}
fn vector<T: Clone>(n: usize, value: T) -> Result<Vec<T>, FormatError> {
    let mut out = Vec::new();
    out.try_reserve_exact(n)
        .map_err(|_| FormatError::AllocationFailed)?;
    out.resize(n, value);
    Ok(out)
}
/// Uses the original immutable snapshot, CST token owners, and actual comments.
/// Work charges node/child/token visits, emitted indentation levels, and text
/// chunks of up to 1 KiB. Output has an independent byte cap before reservation.
pub fn format(
    tree: &SyntaxTree,
    sources: &SourceMap,
    limits: Limits,
) -> Result<Formatted, FormatError> {
    let original = sources
        .file(tree.source())
        .map_err(FormatError::Source)?
        .text();
    let mut writer = Writer {
        text: String::new(),
        indent: 0,
        line_start: true,
        space: false,
        line: false,
        work: 0,
        limits,
    };
    if tree.has_errors() {
        writer.raw(original)?;
        return Ok(Formatted {
            text: writer.text,
            status: Status::PreservedInvalid,
        });
    }
    writer.charge(tree.tokens().len())?;
    let mut owners = vector(tree.tokens().len(), S::File)?;
    for node in tree.nodes() {
        writer.charge(node.children().len() + 1)?;
        for element in node.children() {
            if let SyntaxElement::Token(index) = element {
                owners[*index] = node.kind();
            }
        }
    }
    // Next code token decides whether a closing brace ends a statement/item
    // or continues a field/call/pattern/else expression. Comments keep order.
    let mut following = vector(tree.tokens().len(), (None, None))?;
    let mut next = None;
    let mut content = None;
    for (i, token) in tree.tokens().iter().enumerate().rev() {
        following[i] = (next, content);
        if !token.kind.is_trivia() {
            next = Some(token.kind);
        }
        if token.kind != K::Whitespace {
            content = Some(token.kind);
        }
    }
    let mut delimiters = Vec::new();
    delimiters
        .try_reserve_exact(tree.nodes().len().min(256))
        .map_err(|_| FormatError::AllocationFailed)?;
    let mut previous = None;
    let mut comment = false;
    let mut gap_newline = false;
    for (index, token) in tree.tokens().iter().enumerate() {
        writer.charge(1)?;
        let text = sources.slice(token.span).map_err(FormatError::Source)?;
        if token.kind == K::Whitespace {
            writer.charge(text.len().div_ceil(1024))?;
            gap_newline |= text.contains(['\n', '\r']);
            continue;
        }
        if matches!(token.kind, K::LineComment | K::BlockComment) {
            let after_statement = writer.line;
            if gap_newline {
                writer.line = true;
            } else if !writer.text.is_empty() {
                writer.line = false;
                writer.space = true;
            }
            writer.token(text)?;
            writer.line = token.kind == K::LineComment || after_statement || gap_newline;
            writer.space = !writer.line;
            comment = true;
            gap_newline = false;
            continue;
        }
        if comment && gap_newline {
            writer.line = true;
        }
        gap_newline = false;
        comment = false;
        match token.kind {
            K::LeftBrace => {
                let kind = brace(owners[index])?;
                let empty = following[index].1 == Some(K::RightBrace);
                writer.space = previous.is_some();
                writer.token(text)?;
                if !empty {
                    writer.indent += 1;
                    writer.line = true;
                }
                if delimiters.len() == delimiters.capacity() {
                    delimiters
                        .try_reserve(1)
                        .map_err(|_| FormatError::AllocationFailed)?;
                }
                delimiters.push(Delimiter::Brace { kind, empty });
            }
            K::RightBrace => {
                let Some(Delimiter::Brace { kind, empty }) = delimiters.pop() else {
                    return Err(FormatError::InvalidTree);
                };
                if kind != brace(owners[index])? {
                    return Err(FormatError::InvalidTree);
                }
                if !empty {
                    writer.indent = writer
                        .indent
                        .checked_sub(1)
                        .ok_or(FormatError::InvalidTree)?;
                    writer.line = true;
                } else {
                    writer.space = false;
                }
                writer.token(text)?;
                writer.line = kind == Brace::Declaration
                    || !following[index].0.is_some_and(|k| {
                        matches!(
                            k,
                            K::Else
                                | K::Comma
                                | K::Semicolon
                                | K::RightParen
                                | K::Dot
                                | K::Plus
                                | K::LessThan
                                | K::FatArrow
                        )
                    });
            }
            K::LeftParen => {
                writer.space |=
                    previous.is_some_and(|k| matches!(k, K::If | K::While | K::Match | K::Return));
                writer.token(text)?;
                writer.indent += 1;
                if delimiters.len() == delimiters.capacity() {
                    delimiters
                        .try_reserve(1)
                        .map_err(|_| FormatError::AllocationFailed)?;
                }
                delimiters.push(Delimiter::Paren);
            }
            K::RightParen => {
                if !matches!(delimiters.pop(), Some(Delimiter::Paren)) {
                    return Err(FormatError::InvalidTree);
                }
                writer.indent = writer
                    .indent
                    .checked_sub(1)
                    .ok_or(FormatError::InvalidTree)?;
                writer.space = false;
                writer.token(text)?;
            }
            K::Semicolon => {
                writer.space = false;
                writer.token(text)?;
                writer.line = true;
            }
            K::Comma => {
                writer.space = false;
                writer.token(text)?;
                if matches!(delimiters.last(), Some(Delimiter::Brace { .. })) {
                    writer.line = true;
                } else {
                    writer.space = true;
                }
            }
            K::Colon => {
                writer.space = false;
                writer.token(text)?;
                writer.space = true;
            }
            K::ColonColon | K::Dot => {
                writer.space = false;
                writer.token(text)?;
            }
            K::LessThan if owners[index] == S::TypeArguments => {
                writer.space = false;
                writer.token(text)?;
                writer.indent += 1;
                if delimiters.len() == delimiters.capacity() {
                    delimiters
                        .try_reserve(1)
                        .map_err(|_| FormatError::AllocationFailed)?;
                }
                delimiters.push(Delimiter::TypeArguments);
            }
            K::GreaterThan => {
                if !matches!(delimiters.pop(), Some(Delimiter::TypeArguments)) {
                    return Err(FormatError::InvalidTree);
                }
                writer.indent = writer
                    .indent
                    .checked_sub(1)
                    .ok_or(FormatError::InvalidTree)?;
                writer.space = false;
                writer.token(text)?;
            }
            K::Ampersand | K::Star => {
                writer.space |= previous.is_some_and(|k| {
                    !matches!(
                        k,
                        K::LeftParen | K::ColonColon | K::Dot | K::Ampersand | K::Star
                    )
                });
                writer.token(text)?;
            }
            K::Equal | K::Plus | K::LessThan | K::Arrow | K::FatArrow => {
                writer.space = true;
                writer.token(text)?;
                writer.space = true;
            }
            _ => {
                writer.space |= previous.is_some_and(|k| {
                    !matches!(
                        k,
                        K::LeftParen | K::ColonColon | K::Dot | K::Ampersand | K::Star
                    ) && !(k == K::LessThan
                        && matches!(delimiters.last(), Some(Delimiter::TypeArguments)))
                });
                writer.token(text)?;
            }
        }
        previous = Some(token.kind);
    }
    if !delimiters.is_empty() {
        return Err(FormatError::InvalidTree);
    }
    writer.newline()?;
    Ok(Formatted {
        text: writer.text,
        status: Status::Formatted,
    })
}
