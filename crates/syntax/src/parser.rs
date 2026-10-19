use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_diagnostics::{
    Applicability, Diagnostic, DiagnosticCode, Fix, Label, Severity, SourceError, SourceId,
    SourceMap, Span, TextEdit, codes,
};

use crate::{LexError, LexLimits, Token, TokenKind as K, lex};

/// Lower limits are useful for embedding and boundary tests. The recursion
/// ceiling remains 256 even if a caller requests a larger depth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseLimits {
    pub lex: LexLimits,
    pub nodes: usize,
    pub depth: usize,
    pub diagnostics: usize,
    /// Parameters, fields, cases, payloads, and argument lists share this bound.
    pub list_elements: usize,
    /// Charges token indexing, node starts, real token consumption, and missing
    /// syntax insertion. Recovery revisits are charged in the same way.
    pub work: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            lex: LexLimits::default(),
            nodes: 2_000_000,
            depth: 256,
            diagnostics: 4_096,
            list_elements: 256,
            work: 10_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxKind {
    File,
    ExpressionRoot,
    Function,
    Parameters,
    Parameter,
    Record,
    RecordField,
    Enum,
    EnumCase,
    PayloadTypes,
    Module,
    Import,
    Host,
    ForeignImport,
    ForeignExport,
    Type,
    LoanType,
    SharedType,
    WeakType,
    ArenaType,
    TypeArguments,
    Path,
    Block,
    UnsafeBlock,
    Let,
    Assign,
    PlaceAssign,
    Arena,
    If,
    While,
    Return,
    Match,
    Arm,
    Literal,
    Name,
    Parenthesized,
    Call,
    SpecializedCall,
    Borrow,
    Dereference,
    Arguments,
    Construction,
    FieldValue,
    Field,
    Add,
    LessThan,
    WildcardPattern,
    BindingPattern,
    LiteralPattern,
    RecordPattern,
    PatternField,
    CasePattern,
    PayloadPatterns,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedSyntax {
    Token(K),
    Expression,
    Pattern,
    Item,
    Type,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeId(usize);

impl NodeId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyntaxElement {
    Node(NodeId),
    Token(usize),
    Missing {
        expected: ExpectedSyntax,
        span: Span,
    },
}

/// Nodes refer only to earlier arena nodes; cloning/dropping a deep left-
/// associative expression never recurses through Rust-owned child values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxNode {
    kind: SyntaxKind,
    span: Span,
    children: Vec<SyntaxElement>,
    depth: usize,
}

impl SyntaxNode {
    pub const fn kind(&self) -> SyntaxKind {
        self.kind
    }
    pub const fn span(&self) -> Span {
        self.span
    }
    pub fn children(&self) -> &[SyntaxElement] {
        &self.children
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxTree {
    source: SourceId,
    tokens: Vec<Token>,
    nodes: Vec<SyntaxNode>,
    root: NodeId,
    diagnostics: Vec<Diagnostic>,
}

impl SyntaxTree {
    pub const fn source(&self) -> SourceId {
        self.source
    }
    pub const fn root(&self) -> NodeId {
        self.root
    }
    pub fn tokens(&self) -> &[Token] {
        &self.tokens
    }
    pub fn nodes(&self) -> &[SyntaxNode] {
        &self.nodes
    }
    pub fn node(&self, id: NodeId) -> Option<&SyntaxNode> {
        self.nodes.get(id.0)
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// Traverses actual tree leaves, including invalid tokens and trivia.
    /// Missing syntax has no real text and is never silently printed as input.
    pub fn reconstruct(&self, sources: &SourceMap) -> std::result::Result<String, SourceError> {
        sources.file(self.source)?;
        let mut output = String::new();
        let mut pending = vec![SyntaxElement::Node(self.root)];
        while let Some(element) = pending.pop() {
            match element {
                SyntaxElement::Node(id) => {
                    pending.extend(self.nodes[id.0].children.iter().rev().copied())
                }
                SyntaxElement::Token(index) => {
                    output.push_str(sources.slice(self.tokens[index].span)?)
                }
                SyntaxElement::Missing { .. } => {}
            }
        }
        Ok(output)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    Lex(LexError),
    Limit(Box<Diagnostic>),
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lex(error) => Display::fmt(error, f),
            Self::Limit(diagnostic) => {
                write!(f, "{}: {}", diagnostic.code.as_str(), diagnostic.message)
            }
        }
    }
}
impl Error for ParseError {}

pub fn parse(sources: &SourceMap, source: SourceId, limits: ParseLimits) -> Result<SyntaxTree> {
    Parser::new(sources, source, limits)?.file()
}

/// The expression entry point is used by tools and transactional REPL analysis;
/// it accepts exactly one expression and retains/diagnoses any trailing input.
pub fn parse_expression(
    sources: &SourceMap,
    source: SourceId,
    limits: ParseLimits,
) -> Result<SyntaxTree> {
    Parser::new(sources, source, limits)?.expression_root()
}

struct Builder {
    kind: SyntaxKind,
    start: usize,
    children: Vec<SyntaxElement>,
}

struct Parser<'a> {
    sources: &'a SourceMap,
    source: SourceId,
    length: usize,
    tokens: Vec<Token>,
    significant: Vec<usize>,
    head: usize,
    cursor: usize,
    nodes: Vec<SyntaxNode>,
    stack: Vec<Builder>,
    diagnostics: Vec<Diagnostic>,
    limits: ParseLimits,
    work: usize,
}

type Result<T> = std::result::Result<T, ParseError>;

impl<'a> Parser<'a> {
    fn new(sources: &'a SourceMap, source: SourceId, limits: ParseLimits) -> Result<Self> {
        let lexed = lex(sources, source, limits.lex).map_err(ParseError::Lex)?;
        let mut parser = Self {
            sources,
            source,
            length: sources
                .file(source)
                .map_err(|e| ParseError::Lex(LexError::Source(e)))?
                .text()
                .len(),
            tokens: lexed.tokens,
            significant: Vec::new(),
            head: 0,
            cursor: 0,
            nodes: Vec::new(),
            stack: Vec::new(),
            diagnostics: lexed.diagnostics,
            limits,
            work: 0,
        };
        if parser.diagnostics.len() > limits.diagnostics {
            return Err(parser.limit("syntax diagnostic limit exceeded"));
        }
        for index in 0..parser.tokens.len() {
            parser.charge()?;
            if !parser.tokens[index].kind.is_trivia() {
                parser.significant.push(index);
            }
        }
        Ok(parser)
    }

    fn span(&self, start: usize, end: usize) -> Span {
        self.sources
            .span(self.source, start..end)
            .expect("parser offsets come from validated token boundaries")
    }

    fn offset(&self) -> usize {
        self.tokens
            .get(self.cursor)
            .map_or(self.length, |t| t.span.range().start)
    }
    fn current_span(&self) -> Span {
        self.significant.get(self.head).map_or_else(
            || self.span(self.length, self.length),
            |i| self.tokens[*i].span,
        )
    }
    fn peek(&self) -> Option<K> {
        self.significant
            .get(self.head)
            .map(|i| self.tokens[*i].kind)
    }

    fn limit(&self, message: &str) -> ParseError {
        ParseError::Limit(Box::new(Diagnostic::new(
            codes::RESOURCE_LIMIT,
            Severity::Error,
            message,
            Label::new(self.current_span(), message),
        )))
    }

    fn charge(&mut self) -> Result<()> {
        if self.work >= self.limits.work {
            return Err(self.limit("parser work limit exceeded"));
        }
        self.work += 1;
        Ok(())
    }

    fn start(&mut self, kind: SyntaxKind) -> Result<()> {
        self.charge()?;
        if self.nodes.len().saturating_add(self.stack.len()) >= self.limits.nodes {
            return Err(self.limit("syntax node limit exceeded"));
        }
        if self.stack.len() >= self.limits.depth.min(256) {
            return Err(self.limit("syntax depth limit exceeded"));
        }
        self.stack.push(Builder {
            kind,
            start: self.offset(),
            children: Vec::new(),
        });
        Ok(())
    }

    fn finish(&mut self) -> Result<NodeId> {
        let builder = self.stack.pop().expect("balanced parser builders");
        let mut depth = 1;
        let mut end = self.offset();
        for child in &builder.children {
            match child {
                SyntaxElement::Node(id) => {
                    depth = depth.max(self.nodes[id.0].depth + 1);
                    end = end.max(self.nodes[id.0].span.range().end);
                }
                SyntaxElement::Missing { span, .. } => end = end.max(span.range().end),
                SyntaxElement::Token(_) => {}
            }
        }
        if depth > self.limits.depth.min(256) {
            return Err(self.limit("syntax tree depth limit exceeded"));
        }
        let id = NodeId(self.nodes.len());
        self.nodes.push(SyntaxNode {
            kind: builder.kind,
            span: self.span(builder.start, end),
            children: builder.children,
            depth,
        });
        if let Some(parent) = self.stack.last_mut() {
            parent.children.push(SyntaxElement::Node(id));
        }
        Ok(id)
    }

    fn precede(&mut self, kind: SyntaxKind) -> Result<()> {
        let child = self
            .stack
            .last_mut()
            .expect("parent for expression")
            .children
            .pop()
            .expect("completed expression");
        let SyntaxElement::Node(id) = child else {
            unreachable!("expressions finish a node")
        };
        self.start(kind)?;
        let builder = self.stack.last_mut().unwrap();
        builder.start = self.nodes[id.0].span.range().start;
        builder.children.push(child);
        Ok(())
    }

    fn consume_to(&mut self, end: usize) -> Result<()> {
        while self.cursor < end {
            self.charge()?;
            self.stack
                .last_mut()
                .expect("root builder")
                .children
                .push(SyntaxElement::Token(self.cursor));
            self.cursor += 1;
        }
        Ok(())
    }

    fn bump(&mut self) -> Result<()> {
        if let Some(index) = self.significant.get(self.head).copied() {
            self.consume_to(index + 1)?;
            self.head += 1;
        }
        Ok(())
    }

    fn eat(&mut self, kind: K) -> Result<bool> {
        if self.peek() == Some(kind) {
            self.bump()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn missing(
        &mut self,
        expected: ExpectedSyntax,
        message: &str,
        fix: Option<&str>,
    ) -> Result<()> {
        self.missing_code(expected, message, fix, codes::EXPECTED_SYNTAX)
    }

    fn missing_code(
        &mut self,
        expected: ExpectedSyntax,
        message: &str,
        fix: Option<&str>,
        code: DiagnosticCode,
    ) -> Result<()> {
        self.charge()?;
        if self.diagnostics.len() >= self.limits.diagnostics {
            return Err(self.limit("syntax diagnostic limit exceeded"));
        }
        let primary = self.current_span();
        let insertion = self.span(primary.range().start, primary.range().start);
        let mut diagnostic =
            Diagnostic::new(code, Severity::Error, message, Label::new(primary, message));
        if let Some(text) = fix {
            diagnostic.fixes.push(Fix {
                message: format!("insert `{text}`"),
                applicability: Applicability::MachineApplicable,
                edits: vec![TextEdit {
                    span: insertion,
                    replacement: text.into(),
                }],
            });
        }
        self.diagnostics.push(diagnostic);
        self.stack
            .last_mut()
            .unwrap()
            .children
            .push(SyntaxElement::Missing {
                expected,
                span: insertion,
            });
        Ok(())
    }

    fn expect(&mut self, kind: K) -> Result<bool> {
        if self.eat(kind)? {
            return Ok(true);
        }
        // Only delimiter insertions with an established follow boundary are
        // machine-applicable. Missing names/expressions/types are never guessed.
        let next = self.peek();
        if kind == K::Identifier && next.is_some_and(reserved_keyword) {
            self.missing_code(
                ExpectedSyntax::Token(kind),
                "keyword is reserved; rename this identifier",
                None,
                codes::UNSUPPORTED_SPECIALIZATION,
            )?;
            self.bump()?;
            return Ok(false);
        }
        let fix = match kind {
            K::Semicolon
                if next.is_none()
                    || next == Some(K::RightBrace)
                    || next.is_some_and(statement_start)
                    || next.is_some_and(item_start) =>
            {
                Some(";")
            }
            K::RightParen
                if matches!(
                    next,
                    None | Some(K::Arrow | K::Semicolon | K::RightBrace | K::FatArrow)
                ) =>
            {
                Some(")")
            }
            K::RightBrace if next.is_none() || next.is_some_and(item_start) => Some("}"),
            _ => None,
        };
        self.missing(
            ExpectedSyntax::Token(kind),
            &format!("expected {kind:?}"),
            fix,
        )?;
        Ok(false)
    }

    fn file(mut self) -> Result<SyntaxTree> {
        self.start(SyntaxKind::File)?;
        while self.peek().is_some() {
            let before = self.head;
            if self.peek().is_some_and(item_start) {
                self.item()?;
            } else {
                self.start(SyntaxKind::Error)?;
                self.missing(ExpectedSyntax::Item, "expected a declaration", None)?;
                self.recover(false)?;
                self.finish()?;
            }
            if self.head == before {
                self.bump()?;
            }
        }
        self.complete()
    }

    fn expression_root(mut self) -> Result<SyntaxTree> {
        self.start(SyntaxKind::ExpressionRoot)?;
        self.expression(true)?;
        if self.peek().is_some() {
            self.start(SyntaxKind::Error)?;
            self.missing(
                ExpectedSyntax::Expression,
                "unexpected text after expression",
                None,
            )?;
            while self.peek().is_some() {
                self.bump()?;
            }
            self.finish()?;
        }
        self.complete()
    }

    fn complete(mut self) -> Result<SyntaxTree> {
        self.consume_to(self.tokens.len())?;
        let root = self.finish()?;
        debug_assert!(self.stack.is_empty());
        Ok(SyntaxTree {
            source: self.source,
            tokens: self.tokens,
            nodes: self.nodes,
            root,
            diagnostics: self.diagnostics,
        })
    }

    fn item(&mut self) -> Result<()> {
        let public = self.peek() == Some(K::Pub);
        let kind = self.lookahead(usize::from(public));
        let foreign =
            kind == Some(K::Extern) && self.lookahead(usize::from(public) + 1) == Some(K::CAbi);
        let node = match kind {
            Some(K::Fn) => SyntaxKind::Function,
            Some(K::Struct) => SyntaxKind::Record,
            Some(K::Enum) => SyntaxKind::Enum,
            Some(K::Mod) => SyntaxKind::Module,
            Some(K::Use) => SyntaxKind::Import,
            Some(K::Extern) if foreign && public => SyntaxKind::ForeignExport,
            Some(K::Extern) if foreign => SyntaxKind::ForeignImport,
            Some(K::Extern) => SyntaxKind::Host,
            _ => SyntaxKind::Error,
        };
        self.start(node)?;
        let public = self.eat(K::Pub)?;
        if public && !foreign && !matches!(kind, Some(K::Fn | K::Struct | K::Enum | K::Mod)) {
            self.missing(
                ExpectedSyntax::Item,
                "pub requires a function, struct, enum, or module",
                None,
            )?;
        }
        match kind {
            Some(K::Fn) => self.function()?,
            Some(K::Struct) => {
                self.bump()?;
                self.expect(K::Identifier)?;
                if self.expect(K::LeftBrace)? {
                    self.list(K::RightBrace, true, Self::record_field)?;
                    self.expect(K::RightBrace)?;
                }
            }
            Some(K::Enum) => {
                self.bump()?;
                self.expect(K::Identifier)?;
                if self.expect(K::LeftBrace)? {
                    self.list(K::RightBrace, false, Self::enum_case)?;
                    self.expect(K::RightBrace)?;
                }
            }
            Some(K::Mod) => {
                self.bump()?;
                self.expect(K::Identifier)?;
                self.expect(K::Semicolon)?;
            }
            Some(K::Use) => {
                self.bump()?;
                self.path()?;
                self.expect(K::Semicolon)?;
            }
            Some(K::Extern) if foreign => {
                self.bump()?;
                self.expect(K::CAbi)?;
                if public {
                    self.function()?;
                } else {
                    self.expect(K::Fn)?;
                    self.signature()?;
                    self.expect(K::Semicolon)?;
                }
            }
            Some(K::Extern) => self.host()?,
            _ => {
                self.missing(
                    ExpectedSyntax::Item,
                    "expected a declaration after pub",
                    None,
                )?;
                self.recover(false)?;
            }
        }
        self.finish()?;
        Ok(())
    }

    fn function(&mut self) -> Result<()> {
        self.expect(K::Fn)?;
        self.signature()?;
        self.block()
    }

    fn signature(&mut self) -> Result<()> {
        self.expect(K::Identifier)?;
        self.start(SyntaxKind::Parameters)?;
        if self.expect(K::LeftParen)? {
            self.list(K::RightParen, true, Self::parameter)?;
            self.expect(K::RightParen)?;
        }
        self.finish()?;
        self.expect(K::Arrow)?;
        self.ty()
    }

    fn host(&mut self) -> Result<()> {
        self.bump()?;
        self.expect(K::Fn)?;
        self.expect(K::Identifier)?;
        self.start(SyntaxKind::Parameters)?;
        self.expect(K::LeftParen)?;
        self.start(SyntaxKind::Parameter)?;
        self.expect(K::Identifier)?;
        self.expect(K::Colon)?;
        self.start(SyntaxKind::Type)?;
        self.expect(K::I64)?;
        self.finish()?;
        self.finish()?;
        self.expect(K::RightParen)?;
        self.finish()?;
        self.expect(K::Arrow)?;
        self.start(SyntaxKind::Type)?;
        self.expect(K::I64)?;
        self.finish()?;
        self.expect(K::Semicolon)?;
        Ok(())
    }

    fn parameter(&mut self) -> Result<()> {
        self.start(SyntaxKind::Parameter)?;
        self.expect(K::Identifier)?;
        self.expect(K::Colon)?;
        self.ty()?;
        self.finish()?;
        Ok(())
    }

    fn record_field(&mut self) -> Result<()> {
        self.start(SyntaxKind::RecordField)?;
        self.expect(K::Identifier)?;
        self.expect(K::Colon)?;
        self.ty()?;
        self.finish()?;
        Ok(())
    }

    fn enum_case(&mut self) -> Result<()> {
        self.start(SyntaxKind::EnumCase)?;
        self.expect(K::Identifier)?;
        if self.peek() == Some(K::LeftParen) {
            self.start(SyntaxKind::PayloadTypes)?;
            self.bump()?;
            self.list(K::RightParen, false, Self::ty)?;
            self.expect(K::RightParen)?;
            self.finish()?;
        }
        self.finish()?;
        Ok(())
    }

    fn ty(&mut self) -> Result<()> {
        self.start(SyntaxKind::Type)?;
        match self.peek() {
            Some(K::I64 | K::Bool) => self.bump()?,
            Some(K::Identifier | K::Root) => {
                self.path()?;
                if self.peek() == Some(K::LessThan) {
                    self.type_arguments()?;
                }
            }
            Some(K::Ampersand | K::Shared | K::Weak | K::Arena) => {
                let modifier = match self.peek().unwrap() {
                    K::Ampersand => SyntaxKind::LoanType,
                    K::Shared => SyntaxKind::SharedType,
                    K::Weak => SyntaxKind::WeakType,
                    K::Arena => SyntaxKind::ArenaType,
                    _ => unreachable!(),
                };
                self.start(modifier)?;
                self.bump()?;
                if modifier == SyntaxKind::LoanType {
                    self.eat(K::Mut)?;
                }
                self.ty()?;
                self.finish()?;
            }
            Some(K::Unsafe) => {
                self.missing_code(
                    ExpectedSyntax::Type,
                    "keyword is reserved; rename this identifier",
                    None,
                    codes::UNSUPPORTED_SPECIALIZATION,
                )?;
                self.bump()?;
            }
            _ => self.missing(ExpectedSyntax::Type, "expected a type", None)?,
        }
        self.finish()?;
        Ok(())
    }

    fn path(&mut self) -> Result<()> {
        self.start(SyntaxKind::Path)?;
        if self.eat(K::Root)? {
            self.expect(K::ColonColon)?;
        }
        self.expect(K::Identifier)?;
        while self.peek() == Some(K::ColonColon) && self.lookahead(1) != Some(K::LessThan) {
            self.bump()?;
            if !self.expect(K::Identifier)? {
                break;
            }
        }
        self.finish()?;
        Ok(())
    }

    fn lookahead(&self, distance: usize) -> Option<K> {
        self.significant
            .get(self.head + distance)
            .map(|index| self.tokens[*index].kind)
    }

    fn type_arguments(&mut self) -> Result<()> {
        self.start(SyntaxKind::TypeArguments)?;
        self.expect(K::LessThan)?;
        self.list(K::GreaterThan, false, Self::ty)?;
        self.expect(K::GreaterThan)?;
        self.finish()?;
        Ok(())
    }

    /// Comma-separated lists recover locally, leaving enclosing braces and new
    /// declarations untouched. A failed element must consume or stop.
    fn list(&mut self, close: K, empty: bool, element: fn(&mut Self) -> Result<()>) -> Result<()> {
        if self.peek() == Some(close) {
            if !empty {
                self.missing(
                    ExpectedSyntax::Expression,
                    "expected at least one list element",
                    None,
                )?;
            }
            return Ok(());
        }
        let mut count = 0;
        loop {
            if self.list_boundary(close) {
                break;
            }
            if count >= self.limits.list_elements {
                return Err(self.limit("syntax list element limit exceeded"));
            }
            count += 1;
            let before = self.head;
            element(self)?;
            if self.head == before {
                if self.list_boundary(close) {
                    break;
                }
                self.start(SyntaxKind::Error)?;
                self.recover_list(close)?;
                self.finish()?;
                if self.head == before {
                    break;
                }
            }
            if self.peek() == Some(close) {
                break;
            }
            if self.eat(K::Comma)? {
                if self.peek() == Some(close) {
                    break;
                }
            } else {
                self.expect(K::Comma)?;
                if self.peek().is_some_and(|kind| !list_element_start(kind))
                    && !self.list_boundary(close)
                {
                    self.start(SyntaxKind::Error)?;
                    self.recover_list(close)?;
                    self.finish()?;
                    self.eat(K::Comma)?;
                }
            }
            if self.list_boundary(close) {
                break;
            }
        }
        Ok(())
    }

    fn list_boundary(&self, close: K) -> bool {
        self.peek().is_none_or(|kind| {
            kind == close
                || kind == K::RightBrace
                || kind == K::RightParen
                || kind == K::Semicolon
                || kind == K::Arrow
                || kind == K::FatArrow
                || item_start(kind)
        })
    }

    fn recover_list(&mut self, close: K) -> Result<()> {
        let mut delimiters = Vec::new();
        while let Some(kind) = self.peek() {
            if delimiters.is_empty() && (self.list_boundary(close) || kind == K::Comma) {
                break;
            }
            // Even an unclosed nested parenthesis must not consume the block's brace.
            if kind == K::RightBrace && !delimiters.contains(&K::RightBrace) {
                break;
            }
            match kind {
                K::LeftParen => delimiters.push(K::RightParen),
                K::LeftBrace => delimiters.push(K::RightBrace),
                K::RightParen | K::RightBrace if delimiters.last() == Some(&kind) => {
                    delimiters.pop();
                }
                _ => {}
            }
            if delimiters.len() > self.limits.depth.min(256) {
                return Err(self.limit("recovery delimiter depth limit exceeded"));
            }
            self.bump()?;
        }
        Ok(())
    }

    fn block(&mut self) -> Result<()> {
        self.start(SyntaxKind::Block)?;
        if self.expect(K::LeftBrace)? {
            while let Some(kind) = self.peek() {
                if kind == K::RightBrace || item_start(kind) {
                    break;
                }
                let before = self.head;
                self.statement()?;
                if self.head == before {
                    self.bump()?;
                }
            }
            self.expect(K::RightBrace)?;
        }
        self.finish()?;
        Ok(())
    }

    fn statement(&mut self) -> Result<()> {
        let kind = match self.peek() {
            Some(K::Let) => SyntaxKind::Let,
            Some(K::Identifier) if self.lookahead(1) == Some(K::Equal) => SyntaxKind::Assign,
            Some(K::Identifier | K::LeftParen) => SyntaxKind::PlaceAssign,
            Some(K::Arena) if self.lookahead(1) != Some(K::Equal) => SyntaxKind::Arena,
            Some(K::Unsafe) if self.lookahead(1) == Some(K::LeftBrace) => SyntaxKind::UnsafeBlock,
            Some(K::If) => SyntaxKind::If,
            Some(K::While) => SyntaxKind::While,
            Some(K::Return) => SyntaxKind::Return,
            Some(K::Match) => SyntaxKind::Match,
            _ => SyntaxKind::Error,
        };
        self.start(kind)?;
        match kind {
            SyntaxKind::Let => {
                self.bump()?;
                self.expect(K::Mut)?;
                self.expect(K::Identifier)?;
                self.expect(K::Colon)?;
                self.ty()?;
                self.expect(K::Equal)?;
                self.expression(true)?;
                self.expect(K::Semicolon)?;
            }
            SyntaxKind::Assign => {
                self.bump()?;
                self.expect(K::Equal)?;
                self.expression(true)?;
                self.expect(K::Semicolon)?;
            }
            SyntaxKind::PlaceAssign => {
                self.primary(true)?;
                self.expect(K::Equal)?;
                self.expression(true)?;
                self.expect(K::Semicolon)?;
            }
            SyntaxKind::Arena => {
                self.bump()?;
                self.expect(K::Identifier)?;
                self.expect(K::LeftParen)?;
                self.expression(true)?;
                self.expect(K::RightParen)?;
                self.block()?;
            }
            SyntaxKind::Return => {
                self.bump()?;
                self.expression(true)?;
                self.expect(K::Semicolon)?;
            }
            SyntaxKind::If => {
                self.bump()?;
                self.expression(false)?;
                self.block()?;
                if self.eat(K::Else)? {
                    self.block()?;
                }
            }
            SyntaxKind::While => {
                self.bump()?;
                self.expression(false)?;
                self.block()?;
            }
            SyntaxKind::UnsafeBlock => {
                self.bump()?;
                self.block()?;
            }
            SyntaxKind::Match => self.match_statement()?,
            _ => {
                if self.peek().is_some_and(reserved_keyword) {
                    self.missing_code(
                        ExpectedSyntax::Expression,
                        "keyword is reserved; rename this identifier",
                        None,
                        codes::UNSUPPORTED_SPECIALIZATION,
                    )?;
                } else {
                    self.missing(ExpectedSyntax::Expression, "expected a statement", None)?;
                }
                self.recover(true)?;
            }
        }
        self.finish()?;
        Ok(())
    }

    fn match_statement(&mut self) -> Result<()> {
        self.bump()?;
        self.expression(false)?;
        if !self.expect(K::LeftBrace)? {
            return Ok(());
        }
        while let Some(kind) = self.peek() {
            if kind == K::RightBrace || item_start(kind) {
                break;
            }
            let before = self.head;
            self.start(SyntaxKind::Arm)?;
            self.pattern()?;
            self.expect(K::FatArrow)?;
            self.block()?;
            self.expect(K::Comma)?;
            self.finish()?;
            if self.head == before {
                self.recover(true)?;
                if self.head == before {
                    break;
                }
            }
        }
        self.expect(K::RightBrace)?;
        Ok(())
    }

    fn expression(&mut self, records: bool) -> Result<()> {
        self.sum(records)?;
        if self.peek() == Some(K::LessThan) {
            self.precede(SyntaxKind::LessThan)?;
            self.bump()?;
            self.sum(records)?;
            self.finish()?;
        }
        Ok(())
    }

    fn sum(&mut self, records: bool) -> Result<()> {
        self.unary(records)?;
        while self.peek() == Some(K::Plus) {
            self.precede(SyntaxKind::Add)?;
            self.bump()?;
            self.unary(records)?;
            self.finish()?;
        }
        Ok(())
    }

    fn unary(&mut self, records: bool) -> Result<()> {
        if self.peek() == Some(K::Ampersand) {
            self.start(SyntaxKind::Borrow)?;
            self.bump()?;
            self.eat(K::Mut)?;
            self.unary(records)?;
            self.finish()?;
            Ok(())
        } else {
            self.primary(records)
        }
    }

    fn primary(&mut self, records: bool) -> Result<()> {
        match self.peek() {
            Some(K::Integer(_) | K::IntegerOutOfRange | K::True | K::False) => {
                self.start(SyntaxKind::Literal)?;
                self.bump()?;
                self.finish()?;
            }
            Some(K::Identifier | K::Root) => {
                self.start(SyntaxKind::Name)?;
                self.path()?;
                self.finish()?;
                let specialization =
                    self.peek() == Some(K::ColonColon) && self.lookahead(1) == Some(K::LessThan);
                if specialization || self.peek() == Some(K::LeftParen) {
                    self.precede(if specialization {
                        SyntaxKind::SpecializedCall
                    } else {
                        SyntaxKind::Call
                    })?;
                    if specialization {
                        self.bump()?;
                        self.type_arguments()?;
                    }
                    self.start(SyntaxKind::Arguments)?;
                    if self.expect(K::LeftParen)? {
                        self.list(K::RightParen, true, Self::argument)?;
                        self.expect(K::RightParen)?;
                    }
                    self.finish()?;
                    self.finish()?;
                } else if records && self.peek() == Some(K::LeftBrace) {
                    self.precede(SyntaxKind::Construction)?;
                    self.bump()?;
                    self.list(K::RightBrace, true, Self::field_value)?;
                    self.expect(K::RightBrace)?;
                    self.finish()?;
                }
            }
            Some(K::LeftParen) => {
                self.start(SyntaxKind::Parenthesized)?;
                self.bump()?;
                if self.peek() == Some(K::Star) {
                    self.start(SyntaxKind::Dereference)?;
                    self.bump()?;
                    self.expression(true)?;
                    self.finish()?;
                } else {
                    self.expression(true)?;
                }
                self.expect(K::RightParen)?;
                self.finish()?;
            }
            _ => {
                self.start(SyntaxKind::Error)?;
                if self.peek().is_some_and(reserved_keyword) {
                    self.missing_code(
                        ExpectedSyntax::Expression,
                        "keyword is reserved; rename this identifier",
                        None,
                        codes::UNSUPPORTED_SPECIALIZATION,
                    )?;
                } else {
                    self.missing(ExpectedSyntax::Expression, "expected an expression", None)?;
                }
                if self.peek().is_some_and(|kind| !expression_boundary(kind)) {
                    self.bump()?;
                }
                self.finish()?;
            }
        }
        while self.peek() == Some(K::Dot) {
            self.precede(SyntaxKind::Field)?;
            self.bump()?;
            self.expect(K::Identifier)?;
            self.finish()?;
        }
        Ok(())
    }

    fn argument(&mut self) -> Result<()> {
        self.expression(true)
    }

    fn field_value(&mut self) -> Result<()> {
        self.start(SyntaxKind::FieldValue)?;
        self.expect(K::Identifier)?;
        self.expect(K::Colon)?;
        self.expression(true)?;
        self.finish()?;
        Ok(())
    }

    fn pattern(&mut self) -> Result<()> {
        match self.peek() {
            Some(K::Wildcard) => {
                self.start(SyntaxKind::WildcardPattern)?;
                self.bump()?;
                self.finish()?;
            }
            Some(K::Integer(_) | K::IntegerOutOfRange | K::True | K::False) => {
                self.start(SyntaxKind::LiteralPattern)?;
                self.bump()?;
                self.finish()?;
            }
            Some(K::Identifier | K::Root) => {
                let single = self.peek() == Some(K::Identifier)
                    && self
                        .significant
                        .get(self.head + 1)
                        .is_none_or(|i| self.tokens[*i].kind != K::ColonColon);
                self.start(SyntaxKind::Name)?;
                self.path()?;
                let base = self.finish()?;
                if self.peek() == Some(K::LeftBrace) {
                    self.precede(SyntaxKind::RecordPattern)?;
                    self.bump()?;
                    self.list(K::RightBrace, true, Self::pattern_field)?;
                    self.expect(K::RightBrace)?;
                    self.finish()?;
                } else if self.peek() == Some(K::LeftParen) {
                    self.precede(SyntaxKind::CasePattern)?;
                    self.start(SyntaxKind::PayloadPatterns)?;
                    self.bump()?;
                    self.list(K::RightParen, false, Self::pattern)?;
                    self.expect(K::RightParen)?;
                    self.finish()?;
                    self.finish()?;
                } else {
                    self.nodes[base.0].kind = if single {
                        SyntaxKind::BindingPattern
                    } else {
                        SyntaxKind::CasePattern
                    };
                }
            }
            _ => {
                self.start(SyntaxKind::Error)?;
                if self.peek().is_some_and(reserved_keyword) {
                    self.missing_code(
                        ExpectedSyntax::Pattern,
                        "keyword is reserved; rename this identifier",
                        None,
                        codes::UNSUPPORTED_SPECIALIZATION,
                    )?;
                } else {
                    self.missing(ExpectedSyntax::Pattern, "expected a pattern", None)?;
                }
                if self.peek().is_some_and(|kind| !expression_boundary(kind)) {
                    self.bump()?;
                }
                self.finish()?;
            }
        }
        Ok(())
    }

    fn pattern_field(&mut self) -> Result<()> {
        self.start(SyntaxKind::PatternField)?;
        self.expect(K::Identifier)?;
        self.expect(K::Colon)?;
        self.pattern()?;
        self.finish()?;
        Ok(())
    }

    /// Iterative delimiter-aware synchronization. An enclosing closing brace
    /// always belongs to its block, even after an unclosed parenthesis.
    fn recover(&mut self, block: bool) -> Result<()> {
        let mut braces = 0_usize;
        let mut parens = 0_usize;
        while let Some(kind) = self.peek() {
            if braces == 0 && (kind == K::RightBrace && block || item_start(kind)) {
                break;
            }
            if kind == K::Semicolon && braces == 0 && parens == 0 {
                self.bump()?;
                break;
            }
            match kind {
                K::LeftBrace => braces += 1,
                K::RightBrace if braces > 0 => braces -= 1,
                K::LeftParen => parens += 1,
                K::RightParen => parens = parens.saturating_sub(1),
                _ => {}
            }
            if braces.max(parens) > self.limits.depth.min(256) {
                return Err(self.limit("recovery delimiter depth limit exceeded"));
            }
            self.bump()?;
        }
        Ok(())
    }
}

fn item_start(kind: K) -> bool {
    matches!(
        kind,
        K::Pub | K::Fn | K::Struct | K::Enum | K::Mod | K::Use | K::Extern
    )
}
fn statement_start(kind: K) -> bool {
    matches!(
        kind,
        K::Let | K::If | K::While | K::Return | K::Match | K::Arena | K::Unsafe
    )
}
fn list_element_start(kind: K) -> bool {
    matches!(
        kind,
        K::Identifier
            | K::Root
            | K::I64
            | K::Bool
            | K::Integer(_)
            | K::IntegerOutOfRange
            | K::True
            | K::False
            | K::Wildcard
            | K::LeftParen
            | K::Ampersand
            | K::Shared
            | K::Weak
            | K::Arena
    )
}
fn reserved_keyword(kind: K) -> bool {
    matches!(kind, K::Shared | K::Weak | K::Arena | K::Unsafe)
}
fn expression_boundary(kind: K) -> bool {
    matches!(
        kind,
        K::RightBrace | K::LeftBrace | K::RightParen | K::Semicolon | K::Comma | K::FatArrow
    ) || item_start(kind)
        || statement_start(kind)
}
