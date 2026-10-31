//! Finite RFC 0006 scalar admission; never used by ordinary source compilation.
use super::{Limits, Work, invalid};
use crate::package::Diagnostic;
use lang_diagnostics::{SourceId, SourceMap, Span};
use lang_hir::{resolved as r, source::Name};
use lang_syntax::{Token, TokenKind as K};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn historical(token: Token, source: &str) -> K {
    let text = &source[token.span.range()];
    let old_keyword = matches!(
        text,
        "fn" | "let"
            | "mut"
            | "i64"
            | "bool"
            | "if"
            | "else"
            | "while"
            | "return"
            | "mod"
            | "use"
            | "pub"
            | "root"
            | "extern"
            | "struct"
            | "enum"
            | "match"
            | "true"
            | "false"
            | "_"
    );
    let identifier = text
        .as_bytes()
        .first()
        .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if identifier && !old_keyword {
        K::Identifier
    } else {
        token.kind
    }
}

pub(super) fn read(
    sources: &SourceMap,
    source: SourceId,
    text: &str,
    tokens: &[Token],
    limits: Limits,
    work: &mut Work,
) -> Result<r::Program, Diagnostic> {
    work.take(tokens.len())?;
    work.take(text.len())?;
    let mut reader = Reader {
        sources,
        source,
        text,
        tokens: tokens
            .iter()
            .filter(|t| !t.kind.is_trivia())
            .map(|t| Token {
                kind: historical(*t, text),
                span: t.span,
            })
            .collect(),
        position: 0,
        limits,
        nodes: limits.nodes,
        work,
    };
    let mut functions = Vec::new();
    let mut names = BTreeMap::new();
    let mut calls = Vec::new();
    while reader.peek().is_some() {
        if functions.len() == limits.functions {
            return Err(super::capacity());
        }
        let start = reader.expect(K::Fn)?.span.range().start;
        let name = reader.name()?;
        if names
            .insert(name.text.clone(), r::FunctionId(functions.len()))
            .is_some()
        {
            return Err(invalid("duplicate historical function", Some(name.span)));
        }
        let mut body = Body::new();
        let mut parameters = Vec::new();
        reader.expect(K::LeftParen)?;
        while reader.peek() != Some(K::RightParen) {
            let name = reader.name()?;
            reader.expect(K::Colon)?;
            let ty = reader.ty()?;
            let local = reader.local(&mut body, name.clone(), ty, r::BindingKind::Parameter)?;
            body.scopes[0].insert(name.text, local);
            parameters.push(local);
            if !reader.eat(K::Comma)? {
                break;
            }
        }
        reader.expect(K::RightParen)?;
        reader.expect(K::Arrow)?;
        let result = reader.ty()?;
        let signature_span = reader.span(start, result.span.range().end);
        let block = reader.block(&mut body, 0)?;
        let span = reader.span(start, body.blocks[block.0].span.range().end);
        calls.extend(
            body.calls
                .into_iter()
                .map(|(expression, name)| (functions.len(), expression, name)),
        );
        functions.push(r::Function {
            effect: r::FunctionEffect::Synchronous,
            abi: r::FunctionAbi::Guest,
            module: r::ModuleId(0),
            qualified_name: name.text.clone(),
            name,
            public: false,
            parameters,
            result,
            locals: body.locals,
            expressions: body.expressions,
            patterns: Vec::new(),
            blocks: body.blocks,
            body: block,
            span,
            signature_span,
        });
    }
    for (function, expression, name) in calls {
        reader.work.take(name.text.len().saturating_add(1))?;
        let callee = names
            .get(&name.text)
            .copied()
            .ok_or_else(|| invalid("unknown historical function", Some(name.span)))?;
        let r::ExpressionKind::Call { callee: target, .. } =
            &mut functions[function].expressions[expression.0].kind
        else {
            unreachable!("private pending call table")
        };
        *target = r::Callable::Guest(callee);
    }
    Ok(r::Program {
        modules: vec![r::Module {
            qualified_name: String::new(),
            span: reader.span(0, text.len()),
        }],
        definitions: Vec::new(),
        types: Vec::new(),
        imports: Vec::new(),
        functions,
    })
}

struct Body {
    locals: Vec<r::Local>,
    expressions: Vec<r::Expression>,
    blocks: Vec<r::Block>,
    scopes: Vec<BTreeMap<String, r::LocalId>>,
    used: BTreeSet<String>,
    calls: Vec<(r::ExpressionId, Name)>,
}
impl Body {
    fn new() -> Self {
        Self {
            locals: Vec::new(),
            expressions: Vec::new(),
            blocks: Vec::new(),
            scopes: vec![BTreeMap::new()],
            used: BTreeSet::new(),
            calls: Vec::new(),
        }
    }
    fn find(&self, name: &str) -> Option<r::LocalId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}
struct Reader<'a> {
    sources: &'a SourceMap,
    source: SourceId,
    text: &'a str,
    tokens: Vec<Token>,
    position: usize,
    limits: Limits,
    nodes: usize,
    work: &'a mut Work,
}
impl Reader<'_> {
    fn span(&self, start: usize, end: usize) -> Span {
        self.sources
            .span(self.source, start..end)
            .expect("bounded historical token span")
    }
    fn here(&self) -> Span {
        self.tokens
            .get(self.position)
            .map_or_else(|| self.span(self.text.len(), self.text.len()), |t| t.span)
    }
    fn peek(&self) -> Option<K> {
        self.tokens.get(self.position).map(|t| t.kind)
    }
    fn bump(&mut self) -> Result<Token, Diagnostic> {
        self.work.take(1)?;
        let token = self
            .tokens
            .get(self.position)
            .copied()
            .ok_or_else(|| invalid("unexpected end of historical source", Some(self.here())))?;
        self.position += 1;
        Ok(token)
    }
    fn expect(&mut self, kind: K) -> Result<Token, Diagnostic> {
        if self.peek() != Some(kind) {
            return Err(invalid(
                "unsupported or malformed historical scalar syntax",
                Some(self.here()),
            ));
        }
        self.bump()
    }
    fn eat(&mut self, kind: K) -> Result<bool, Diagnostic> {
        if self.peek() == Some(kind) {
            self.bump()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn node(&mut self) -> Result<(), Diagnostic> {
        self.work.take(1)?;
        self.nodes = self.nodes.checked_sub(1).ok_or_else(super::capacity)?;
        Ok(())
    }
    fn depth(&self, depth: usize) -> Result<(), Diagnostic> {
        if depth > self.limits.depth {
            Err(super::capacity())
        } else {
            Ok(())
        }
    }
    fn name(&mut self) -> Result<Name, Diagnostic> {
        let token = self.expect(K::Identifier)?;
        let text = &self.text[token.span.range()];
        self.work.take(text.len())?;
        Ok(Name {
            text: text.to_string(),
            span: token.span,
        })
    }
    fn ty(&mut self) -> Result<r::TypeUse, Diagnostic> {
        let token = self.bump()?;
        let value_type = match token.kind {
            K::I64 => r::Type::I64,
            K::Bool => r::Type::Bool,
            _ => {
                return Err(invalid(
                    "historical profile requires an explicit i64 or bool type",
                    Some(token.span),
                ));
            }
        };
        Ok(r::TypeUse {
            value_type,
            span: token.span,
        })
    }
    fn local(
        &mut self,
        body: &mut Body,
        name: Name,
        ty: r::TypeUse,
        kind: r::BindingKind,
    ) -> Result<r::LocalId, Diagnostic> {
        self.node()?;
        if !body.used.insert(name.text.clone()) {
            return Err(invalid(
                "historical local names must be unique throughout the function",
                Some(name.span),
            ));
        }
        let id = r::LocalId(body.locals.len());
        body.locals.push(r::Local {
            name,
            kind,
            annotation: Some(ty),
        });
        Ok(id)
    }
    fn expression_node(
        &mut self,
        body: &mut Body,
        kind: r::ExpressionKind,
        span: Span,
    ) -> Result<r::ExpressionId, Diagnostic> {
        self.node()?;
        let id = r::ExpressionId(body.expressions.len());
        body.expressions.push(r::Expression { kind, span });
        Ok(id)
    }
    fn block(&mut self, body: &mut Body, depth: usize) -> Result<r::BlockId, Diagnostic> {
        self.depth(depth)?;
        self.node()?;
        let start = self.expect(K::LeftBrace)?.span.range().start;
        body.scopes.push(BTreeMap::new());
        let mut statements = Vec::new();
        while self.peek() != Some(K::RightBrace) {
            statements.push(self.statement(body, depth + 1)?);
        }
        let closing_brace = self.expect(K::RightBrace)?.span;
        body.scopes.pop();
        let id = r::BlockId(body.blocks.len());
        body.blocks.push(r::Block {
            statements,
            span: self.span(start, closing_brace.range().end),
            closing_brace,
        });
        Ok(id)
    }
    fn statement(&mut self, body: &mut Body, depth: usize) -> Result<r::Statement, Diagnostic> {
        self.depth(depth)?;
        self.node()?;
        let start = self.here().range().start;
        let kind = match self.peek() {
            Some(K::Let) => {
                self.bump()?;
                self.expect(K::Mut)?;
                let name = self.name()?;
                self.expect(K::Colon)?;
                let ty = self.ty()?;
                self.expect(K::Equal)?;
                let initializer = self.expression(body, depth)?;
                self.expect(K::Semicolon)?;
                let local = self.local(body, name.clone(), ty, r::BindingKind::Mutable)?;
                body.scopes
                    .last_mut()
                    .expect("active block")
                    .insert(name.text, local);
                r::StatementKind::LetMut { local, initializer }
            }
            Some(K::Identifier) => {
                let name = self.name()?;
                let local = body
                    .find(&name.text)
                    .ok_or_else(|| invalid("unknown historical local", Some(name.span)))?;
                if body.locals[local.0].kind != r::BindingKind::Mutable {
                    return Err(invalid(
                        "historical parameters are immutable",
                        Some(name.span),
                    ));
                }
                self.expect(K::Equal)?;
                let value = self.expression(body, depth)?;
                self.expect(K::Semicolon)?;
                r::StatementKind::Assign { local, value }
            }
            Some(K::Return) => {
                self.bump()?;
                let value = self.expression(body, depth)?;
                self.expect(K::Semicolon)?;
                r::StatementKind::Return(value)
            }
            Some(K::If) => {
                self.bump()?;
                let condition = self.expression(body, depth)?;
                let then_block = self.block(body, depth + 1)?;
                let else_block = if self.eat(K::Else)? {
                    Some(self.block(body, depth + 1)?)
                } else {
                    None
                };
                r::StatementKind::If {
                    condition,
                    then_block,
                    else_block,
                }
            }
            Some(K::While) => {
                self.bump()?;
                let condition = self.expression(body, depth)?;
                let nested = self.block(body, depth + 1)?;
                r::StatementKind::While {
                    condition,
                    body: nested,
                }
            }
            _ => {
                return Err(invalid(
                    "unsupported historical statement",
                    Some(self.here()),
                ));
            }
        };
        let end = self.tokens[self.position - 1].span.range().end;
        Ok(r::Statement {
            kind,
            span: self.span(start, end),
        })
    }
    fn expression(&mut self, body: &mut Body, depth: usize) -> Result<r::ExpressionId, Diagnostic> {
        self.depth(depth)?;
        let mut left = self.addition(body, depth)?;
        while self.eat(K::LessThan)? {
            let right = self.addition(body, depth)?;
            let span = self.span(
                body.expressions[left.0].span.range().start,
                body.expressions[right.0].span.range().end,
            );
            left = self.expression_node(body, r::ExpressionKind::LessThan(left, right), span)?;
        }
        Ok(left)
    }
    fn addition(&mut self, body: &mut Body, depth: usize) -> Result<r::ExpressionId, Diagnostic> {
        let mut left = self.primary(body, depth)?;
        while self.eat(K::Plus)? {
            let right = self.primary(body, depth)?;
            let span = self.span(
                body.expressions[left.0].span.range().start,
                body.expressions[right.0].span.range().end,
            );
            left = self.expression_node(body, r::ExpressionKind::Add(left, right), span)?;
        }
        Ok(left)
    }
    fn primary(&mut self, body: &mut Body, depth: usize) -> Result<r::ExpressionId, Diagnostic> {
        self.depth(depth)?;
        let token = self.bump()?;
        let kind = match token.kind {
            K::Integer(value) => r::ExpressionKind::Integer(value),
            K::True => r::ExpressionKind::Boolean(true),
            K::False => r::ExpressionKind::Boolean(false),
            K::LeftParen => {
                let expression = self.expression(body, depth + 1)?;
                let end = self.expect(K::RightParen)?.span.range().end;
                body.expressions[expression.0].span = self.span(token.span.range().start, end);
                return Ok(expression);
            }
            K::Identifier => {
                let text = &self.text[token.span.range()];
                self.work.take(text.len())?;
                let name = Name {
                    text: text.to_string(),
                    span: token.span,
                };
                if self.eat(K::LeftParen)? {
                    if body.find(&name.text).is_some() {
                        return Err(invalid(
                            "historical local shadows this callable",
                            Some(name.span),
                        ));
                    }
                    let mut arguments = Vec::new();
                    while self.peek() != Some(K::RightParen) {
                        arguments.push(self.expression(body, depth + 1)?);
                        if !self.eat(K::Comma)? {
                            break;
                        }
                    }
                    let end = self.expect(K::RightParen)?.span.range().end;
                    let expression = self.expression_node(
                        body,
                        r::ExpressionKind::Call {
                            callee: r::Callable::Guest(r::FunctionId(usize::MAX)),
                            arguments,
                        },
                        self.span(token.span.range().start, end),
                    )?;
                    body.calls.push((expression, name));
                    return Ok(expression);
                }
                r::ExpressionKind::Local(
                    body.find(&name.text)
                        .ok_or_else(|| invalid("unknown historical local", Some(name.span)))?,
                )
            }
            _ => {
                return Err(invalid(
                    "unsupported historical expression",
                    Some(token.span),
                ));
            }
        };
        self.expression_node(body, kind, token.span)
    }
}
