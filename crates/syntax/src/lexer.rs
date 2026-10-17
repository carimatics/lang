use std::error::Error;
use std::fmt::{Display, Formatter};
use std::ops::Range;

use lang_diagnostics::{
    Diagnostic, DiagnosticCode, Label, Severity, SourceError, SourceId, SourceMap, Span, codes,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LexLimits {
    pub source_bytes: usize,
    pub tokens: usize,
    pub comment_depth: usize,
    /// An extra implementation bound for invalid-source diagnostic storage.
    pub diagnostics: usize,
}

impl Default for LexLimits {
    fn default() -> Self {
        Self {
            source_bytes: 4 * 1024 * 1024,
            tokens: 1_000_000,
            comment_depth: 256,
            diagnostics: 4_096,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputErrorKind {
    InvalidUtf8,
    Bom,
    SourceBytes,
    Source(SourceError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputError {
    pub kind: InputErrorKind,
    pub byte_range: Range<usize>,
}

impl InputError {
    pub const fn code(&self) -> Option<DiagnosticCode> {
        match self.kind {
            InputErrorKind::InvalidUtf8 | InputErrorKind::Bom => Some(codes::INVALID_ENCODING),
            InputErrorKind::SourceBytes => Some(codes::RESOURCE_LIMIT),
            InputErrorKind::Source(_) => None,
        }
    }
}

impl Display for InputError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "source input error {:?} at bytes {}..{}",
            self.kind, self.byte_range.start, self.byte_range.end
        )
    }
}
impl Error for InputError {}

pub fn add_source(
    sources: &mut SourceMap,
    name: impl Into<String>,
    bytes: &[u8],
    limits: LexLimits,
) -> Result<SourceId, InputError> {
    if bytes.len() > limits.source_bytes {
        return Err(InputError {
            kind: InputErrorKind::SourceBytes,
            byte_range: 0..bytes.len(),
        });
    }
    let text = std::str::from_utf8(bytes).map_err(|error| InputError {
        kind: InputErrorKind::InvalidUtf8,
        byte_range: error.valid_up_to()
            ..error
                .error_len()
                .map_or(bytes.len(), |len| error.valid_up_to() + len),
    })?;
    if text.starts_with('\u{feff}') {
        return Err(InputError {
            kind: InputErrorKind::Bom,
            byte_range: 0..3,
        });
    }
    sources.add(name, text).map_err(|error| InputError {
        kind: InputErrorKind::Source(error),
        byte_range: 0..0,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TokenKind {
    Whitespace,
    LineComment,
    BlockComment,
    Invalid,
    Identifier,
    Wildcard,
    Integer(i64),
    IntegerOutOfRange,
    Fn,
    Let,
    Mut,
    I64,
    Bool,
    If,
    Else,
    While,
    Return,
    Mod,
    Use,
    Pub,
    Root,
    Extern,
    Struct,
    Enum,
    Match,
    True,
    False,
    Shared,
    Weak,
    Arena,
    LeftParen,
    RightParen,
    LeftBrace,
    RightBrace,
    Colon,
    Semicolon,
    Comma,
    Dot,
    Equal,
    Plus,
    LessThan,
    GreaterThan,
    Ampersand,
    Star,
    Arrow,
    ColonColon,
    FatArrow,
}

impl TokenKind {
    pub const fn is_trivia(self) -> bool {
        matches!(
            self,
            Self::Whitespace | Self::LineComment | Self::BlockComment
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lexed {
    pub source: SourceId,
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LexError {
    Source(SourceError),
    Limit(Box<Diagnostic>),
}

impl Display for LexError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(error) => Display::fmt(error, f),
            Self::Limit(diagnostic) => {
                write!(f, "{}: {}", diagnostic.code.as_str(), diagnostic.message)
            }
        }
    }
}
impl Error for LexError {}

pub fn lex(sources: &SourceMap, source: SourceId, limits: LexLimits) -> Result<Lexed, LexError> {
    let text = sources.file(source).map_err(LexError::Source)?.text();
    let mut lexer = Lexer {
        sources,
        source,
        text,
        limits,
        cursor: 0,
        tokens: Vec::new(),
        diagnostics: Vec::new(),
    };
    if text.len() > limits.source_bytes {
        return Err(lexer.limit(0, text.len(), "source byte limit exceeded"));
    }
    while lexer.cursor < text.len() {
        let start = lexer.cursor;
        if lexer.tokens.len() >= limits.tokens {
            return Err(lexer.limit(start, start, "token limit exceeded"));
        }
        let kind = lexer.token()?;
        lexer.tokens.push(Token {
            kind,
            span: lexer.span(start, lexer.cursor),
        });
    }
    Ok(Lexed {
        source,
        tokens: lexer.tokens,
        diagnostics: lexer.diagnostics,
    })
}

struct Lexer<'a> {
    sources: &'a SourceMap,
    source: SourceId,
    text: &'a str,
    limits: LexLimits,
    cursor: usize,
    tokens: Vec<Token>,
    diagnostics: Vec<Diagnostic>,
}

impl Lexer<'_> {
    fn span(&self, start: usize, end: usize) -> Span {
        self.sources
            .span(self.source, start..end)
            .expect("lexer spans are UTF-8 boundaries")
    }

    fn limit(&self, start: usize, end: usize, message: &str) -> LexError {
        LexError::Limit(Box::new(Diagnostic::new(
            codes::RESOURCE_LIMIT,
            Severity::Error,
            message,
            Label::new(self.span(start, end), message),
        )))
    }

    fn report(
        &mut self,
        code: DiagnosticCode,
        start: usize,
        end: usize,
        message: &str,
    ) -> Result<(), LexError> {
        if self.diagnostics.len() >= self.limits.diagnostics {
            return Err(self.limit(start, end, "lexical diagnostic limit exceeded"));
        }
        self.diagnostics.push(Diagnostic::new(
            code,
            Severity::Error,
            message,
            Label::new(self.span(start, end), message),
        ));
        Ok(())
    }

    fn token(&mut self) -> Result<TokenKind, LexError> {
        let bytes = self.text.as_bytes();
        let start = self.cursor;
        let byte = bytes[start];
        if matches!(byte, b' ' | b'\t' | b'\r' | b'\n') {
            self.cursor += 1;
            while bytes
                .get(self.cursor)
                .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
            {
                self.cursor += 1;
            }
            return Ok(TokenKind::Whitespace);
        }
        if bytes[start..].starts_with(b"//") {
            self.cursor += 2;
            while self.cursor < bytes.len() && bytes[self.cursor] != b'\n' {
                if bytes[self.cursor] == 0 {
                    self.report(
                        codes::INVALID_TOKEN,
                        self.cursor,
                        self.cursor + 1,
                        "NUL is not allowed in source",
                    )?;
                }
                self.cursor += 1;
            }
            return Ok(TokenKind::LineComment);
        }
        if bytes[start..].starts_with(b"/*") {
            return self.block_comment();
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            self.cursor += 1;
            while bytes
                .get(self.cursor)
                .is_some_and(|byte| identifier_part(*byte))
            {
                self.cursor += 1;
            }
            return Ok(keyword(&self.text[start..self.cursor]));
        }
        if byte.is_ascii_digit()
            || (byte == b'-' && bytes.get(start + 1).is_some_and(u8::is_ascii_digit))
        {
            self.cursor += 1;
            while bytes.get(self.cursor).is_some_and(u8::is_ascii_digit) {
                self.cursor += 1;
            }
            if bytes
                .get(self.cursor)
                .is_some_and(|byte| identifier_part(*byte))
            {
                while bytes
                    .get(self.cursor)
                    .is_some_and(|byte| identifier_part(*byte))
                {
                    self.cursor += 1;
                }
                self.report(
                    codes::INVALID_TOKEN,
                    start,
                    self.cursor,
                    "malformed decimal token",
                )?;
                return Ok(TokenKind::Invalid);
            }
            return match self.text[start..self.cursor].parse::<i64>() {
                Ok(value) => Ok(TokenKind::Integer(value)),
                Err(_) => {
                    self.report(
                        codes::INTEGER_OUT_OF_RANGE,
                        start,
                        self.cursor,
                        "decimal literal is outside the i64 range",
                    )?;
                    Ok(TokenKind::IntegerOutOfRange)
                }
            };
        }
        let pair = match bytes.get(start..start + 2) {
            Some(b"->") => Some(TokenKind::Arrow),
            Some(b"::") => Some(TokenKind::ColonColon),
            Some(b"=>") => Some(TokenKind::FatArrow),
            _ => None,
        };
        if let Some(kind) = pair {
            self.cursor += 2;
            return Ok(kind);
        }
        let kind = match byte {
            b'(' => Some(TokenKind::LeftParen),
            b')' => Some(TokenKind::RightParen),
            b'{' => Some(TokenKind::LeftBrace),
            b'}' => Some(TokenKind::RightBrace),
            b':' => Some(TokenKind::Colon),
            b';' => Some(TokenKind::Semicolon),
            b',' => Some(TokenKind::Comma),
            b'.' => Some(TokenKind::Dot),
            b'=' => Some(TokenKind::Equal),
            b'+' => Some(TokenKind::Plus),
            b'<' => Some(TokenKind::LessThan),
            b'>' => Some(TokenKind::GreaterThan),
            b'&' => Some(TokenKind::Ampersand),
            b'*' => Some(TokenKind::Star),
            _ => None,
        };
        if let Some(kind) = kind {
            self.cursor += 1;
            return Ok(kind);
        }
        let character = self.text[start..]
            .chars()
            .next()
            .expect("nonempty source remainder");
        self.cursor += character.len_utf8();
        let (code, message) = if start == 0 && character == '\u{feff}' {
            (
                codes::INVALID_ENCODING,
                "source must not begin with a UTF-8 BOM",
            )
        } else {
            (codes::INVALID_TOKEN, "invalid source character")
        };
        self.report(code, start, self.cursor, message)?;
        Ok(TokenKind::Invalid)
    }

    fn block_comment(&mut self) -> Result<TokenKind, LexError> {
        let start = self.cursor;
        let bytes = self.text.as_bytes();
        let mut depth = 0_usize;
        while self.cursor < bytes.len() {
            if bytes[self.cursor..].starts_with(b"/*") {
                if depth >= self.limits.comment_depth {
                    return Err(self.limit(
                        self.cursor,
                        self.cursor + 2,
                        "block comment depth limit exceeded",
                    ));
                }
                depth += 1;
                self.cursor += 2;
            } else if bytes[self.cursor..].starts_with(b"*/") {
                depth -= 1;
                self.cursor += 2;
                if depth == 0 {
                    return Ok(TokenKind::BlockComment);
                }
            } else {
                if bytes[self.cursor] == 0 {
                    self.report(
                        codes::INVALID_TOKEN,
                        self.cursor,
                        self.cursor + 1,
                        "NUL is not allowed in source",
                    )?;
                }
                self.cursor += 1;
            }
        }
        self.report(
            codes::UNTERMINATED_COMMENT,
            start,
            self.cursor,
            "unterminated block comment",
        )?;
        Ok(TokenKind::BlockComment)
    }
}

fn identifier_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn keyword(text: &str) -> TokenKind {
    match text {
        "fn" => TokenKind::Fn,
        "let" => TokenKind::Let,
        "mut" => TokenKind::Mut,
        "i64" => TokenKind::I64,
        "bool" => TokenKind::Bool,
        "if" => TokenKind::If,
        "else" => TokenKind::Else,
        "while" => TokenKind::While,
        "return" => TokenKind::Return,
        "mod" => TokenKind::Mod,
        "use" => TokenKind::Use,
        "pub" => TokenKind::Pub,
        "root" => TokenKind::Root,
        "extern" => TokenKind::Extern,
        "struct" => TokenKind::Struct,
        "enum" => TokenKind::Enum,
        "match" => TokenKind::Match,
        "true" => TokenKind::True,
        "false" => TokenKind::False,
        "shared" => TokenKind::Shared,
        "weak" => TokenKind::Weak,
        "arena" => TokenKind::Arena,
        "_" => TokenKind::Wildcard,
        _ => TokenKind::Identifier,
    }
}
