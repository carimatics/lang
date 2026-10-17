//! Backend-independent source HIR for accepted RFCs 0002/0006.
//!
//! The original RFC 0002 analyzer accepts structured semantic syntax and yields
//! a typed primitive function only after successful checking. `resolved` holds
//! name-resolved RFC 0006 program input; `layout` checks bounded copy-only nominal
//! layouts. Source parsing/module binding live outside this crate. Program-wide
//! expression/pattern/call typing and bounded exhaustiveness are checked by
//! `program`, which produces a read-only typed program on success.

pub mod layout;
pub mod program;
pub mod resolved;
pub mod source;
pub mod types;

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_diagnostics::{Diagnostic, Label, Severity, SourceError, SourceMap, Span, codes};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalId(usize);

impl LocalId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExpressionId(usize);

impl ExpressionId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlockId(usize);

impl BlockId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Type {
    I64,
    Bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalKind {
    Parameter,
    Mutable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Local {
    pub name: String,
    pub span: Span,
    pub kind: LocalKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expression {
    pub kind: ExpressionKind,
    pub value_type: Type,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpressionKind {
    Integer(i64),
    Local(LocalId),
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub statements: Vec<Statement>,
    pub span: Span,
    pub closing_brace: Span,
    pub falls_through: bool,
}

/// Arena contents are read-only: only successful analysis constructs this type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    name: String,
    span: Span,
    signature_span: Span,
    locals: Vec<Local>,
    parameters: Vec<LocalId>,
    expressions: Vec<Expression>,
    blocks: Vec<Block>,
    body: BlockId,
}

impl Function {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub const fn span(&self) -> Span {
        self.span
    }
    pub const fn signature_span(&self) -> Span {
        self.signature_span
    }
    pub fn locals(&self) -> &[Local] {
        &self.locals
    }
    pub fn parameters(&self) -> &[LocalId] {
        &self.parameters
    }
    pub fn expressions(&self) -> &[Expression] {
        &self.expressions
    }
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }
    pub const fn body(&self) -> BlockId {
        self.body
    }
}

/// Caller-selected internal analysis limits, not a source-language profile.
/// Recursion is additionally capped at 128 to protect the Rust call stack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisLimits {
    pub work: usize,
    pub locals: usize,
    pub depth: usize,
    pub diagnostics: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Work,
    Locals,
    Depth,
    Diagnostics,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputErrorKind {
    InvalidSpan(SourceError),
    SpanOutsideFunction,
    NameTextMismatch,
    InvalidExpressionId(usize),
    InvalidBlockId(usize),
    CyclicExpression,
    CyclicBlock,
    LimitExceeded(LimitKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputError {
    pub kind: InputErrorKind,
    pub span: Span,
}

impl Display for InputError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid or over-budget HIR input: {:?}",
            self.kind
        )
    }
}

impl Error for InputError {}

/// RFC 0002 requires this warning but does not assign its diagnostic code.
/// Keep it structured without preempting RFC 0006's proposed code assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Warning {
    UnreachableStatement { span: Span },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Analysis {
    pub function: Option<Function>,
    pub diagnostics: Vec<Diagnostic>,
    pub warnings: Vec<Warning>,
}

/// Resolve/type-check one RFC 0002 function after syntactic validation.
/// Input nodes are borrowed arenas, so cycles/depth bombs are safely rejected
/// without recursive input ownership or drop. Literal range errors belong to
/// the preceding syntax stage; this API receives already checked i64 values.
pub fn analyze(
    input: &source::Function,
    sources: &SourceMap,
    limits: AnalysisLimits,
) -> Result<Analysis, InputError> {
    let mut analyzer = Analyzer {
        input,
        sources,
        limits,
        work: 0,
        locals: Vec::new(),
        parameters: Vec::new(),
        declarations: BTreeMap::new(),
        scopes: vec![BTreeMap::new()],
        expressions: Vec::new(),
        blocks: Vec::new(),
        active_expressions: BTreeSet::new(),
        active_blocks: BTreeSet::new(),
        diagnostics: Vec::new(),
        warnings: Vec::new(),
    };
    analyzer.check_span(input.span, 0)?;
    analyzer.check_span(input.signature_span, 0)?;
    analyzer.check_name(&input.name, 0)?;
    for parameter in &input.parameters {
        if let Some(local) = analyzer.declare(parameter, LocalKind::Parameter, 0)? {
            analyzer.parameters.push(local);
        }
    }
    let body = analyzer.block(input.body, 0)?;
    if analyzer.blocks[body.index()].falls_through {
        let mut diagnostic = Diagnostic::new(
            codes::MISSING_RETURN,
            Severity::Error,
            "a reachable function exit lacks a return",
            Label::new(
                analyzer.blocks[body.index()].closing_brace,
                "function can end here",
            ),
        );
        diagnostic.secondary.push(Label::new(
            input.signature_span,
            "this function must return i64",
        ));
        analyzer.report(diagnostic)?;
    }
    let function = analyzer.diagnostics.is_empty().then(|| Function {
        name: input.name.text.clone(),
        span: input.span,
        signature_span: input.signature_span,
        locals: analyzer.locals,
        parameters: analyzer.parameters,
        expressions: analyzer.expressions,
        blocks: analyzer.blocks,
        body,
    });
    Ok(Analysis {
        function,
        diagnostics: analyzer.diagnostics,
        warnings: analyzer.warnings,
    })
}

struct Analyzer<'a> {
    input: &'a source::Function,
    sources: &'a SourceMap,
    limits: AnalysisLimits,
    work: usize,
    locals: Vec<Local>,
    parameters: Vec<LocalId>,
    declarations: BTreeMap<String, LocalId>,
    scopes: Vec<BTreeMap<String, LocalId>>,
    expressions: Vec<Expression>,
    blocks: Vec<Block>,
    active_expressions: BTreeSet<source::ExpressionId>,
    active_blocks: BTreeSet<source::BlockId>,
    diagnostics: Vec<Diagnostic>,
    warnings: Vec<Warning>,
}

impl Analyzer<'_> {
    fn error(&self, kind: InputErrorKind, span: Span) -> InputError {
        InputError { kind, span }
    }

    fn charge(&mut self, amount: usize, span: Span) -> Result<(), InputError> {
        self.work = self
            .work
            .checked_add(amount)
            .filter(|work| *work <= self.limits.work)
            .ok_or_else(|| self.error(InputErrorKind::LimitExceeded(LimitKind::Work), span))?;
        Ok(())
    }

    fn check_span(&mut self, span: Span, depth: usize) -> Result<(), InputError> {
        self.charge(1, span)?;
        if depth > self.limits.depth.min(128) {
            return Err(self.error(InputErrorKind::LimitExceeded(LimitKind::Depth), span));
        }
        self.sources
            .slice(span)
            .map_err(|error| self.error(InputErrorKind::InvalidSpan(error), span))?;
        let range = span.range();
        let function = self.input.span.range();
        if span.source() != self.input.span.source()
            || range.start < function.start
            || range.end > function.end
        {
            return Err(self.error(InputErrorKind::SpanOutsideFunction, span));
        }
        Ok(())
    }

    fn check_name(&mut self, name: &source::Name, depth: usize) -> Result<(), InputError> {
        self.check_span(name.span, depth)?;
        self.charge(name.text.len(), name.span)?;
        if self.sources.slice(name.span).expect("span was validated") != name.text {
            return Err(self.error(InputErrorKind::NameTextMismatch, name.span));
        }
        Ok(())
    }

    fn check_diagnostic_budget(&self, span: Span) -> Result<(), InputError> {
        if self.diagnostics.len() >= self.limits.diagnostics.saturating_sub(self.warnings.len()) {
            return Err(self.error(InputErrorKind::LimitExceeded(LimitKind::Diagnostics), span));
        }
        Ok(())
    }

    fn report(&mut self, diagnostic: Diagnostic) -> Result<(), InputError> {
        self.check_diagnostic_budget(diagnostic.primary.span)?;
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn declare(
        &mut self,
        name: &source::Name,
        kind: LocalKind,
        depth: usize,
    ) -> Result<Option<LocalId>, InputError> {
        self.check_name(name, depth)?;
        if let Some(original) = self.declarations.get(&name.text) {
            let mut diagnostic = Diagnostic::new(
                codes::DUPLICATE_NAME,
                Severity::Error,
                format!("name '{}' is already declared in this function", name.text),
                Label::new(name.span, "duplicate declaration"),
            );
            diagnostic.secondary.push(Label::new(
                self.locals[original.index()].span,
                "first declaration",
            ));
            self.report(diagnostic)?;
            return Ok(None);
        }
        if self.locals.len() >= self.limits.locals {
            return Err(self.error(InputErrorKind::LimitExceeded(LimitKind::Locals), name.span));
        }
        let local = LocalId(self.locals.len());
        self.locals.push(Local {
            name: name.text.clone(),
            span: name.span,
            kind,
        });
        self.declarations.insert(name.text.clone(), local);
        self.scopes
            .last_mut()
            .expect("function scope exists")
            .insert(name.text.clone(), local);
        Ok(Some(local))
    }

    fn resolve(
        &mut self,
        name: &source::Name,
        depth: usize,
    ) -> Result<Option<LocalId>, InputError> {
        self.check_name(name, depth)?;
        for index in (0..self.scopes.len()).rev() {
            self.charge(1, name.span)?;
            if let Some(local) = self.scopes[index].get(&name.text) {
                return Ok(Some(*local));
            }
        }
        self.report(Diagnostic::new(
            codes::UNKNOWN_NAME,
            Severity::Error,
            format!("unknown name '{}'", name.text),
            Label::new(name.span, "not declared in this scope"),
        ))?;
        Ok(None)
    }

    fn require(
        &mut self,
        expression: Option<ExpressionId>,
        expected: Type,
    ) -> Result<Option<ExpressionId>, InputError> {
        if let Some(id) = expression {
            let expression = &self.expressions[id.index()];
            if expression.value_type != expected {
                let (code, message) = match expected {
                    Type::I64 => (codes::EXPECTED_I64, "expected i64 expression"),
                    Type::Bool => (codes::EXPECTED_BOOL, "condition must have type bool"),
                };
                self.report(Diagnostic::new(
                    code,
                    Severity::Error,
                    message,
                    Label::new(expression.span, message),
                ))?;
                return Ok(None);
            }
        }
        Ok(expression)
    }

    fn expression(
        &mut self,
        id: source::ExpressionId,
        depth: usize,
    ) -> Result<Option<ExpressionId>, InputError> {
        let input = self.input;
        let expression = input
            .expressions
            .get(id.0)
            .ok_or_else(|| self.error(InputErrorKind::InvalidExpressionId(id.0), input.span))?;
        self.check_span(expression.span, depth)?;
        if !self.active_expressions.insert(id) {
            return Err(self.error(InputErrorKind::CyclicExpression, expression.span));
        }
        let value = match &expression.kind {
            source::ExpressionKind::Integer(value) => {
                Some((ExpressionKind::Integer(*value), Type::I64))
            }
            source::ExpressionKind::Name(name) => self
                .resolve(name, depth)?
                .map(|local| (ExpressionKind::Local(local), Type::I64)),
            source::ExpressionKind::Add(left, right)
            | source::ExpressionKind::LessThan(left, right) => {
                let left = self.expression(*left, depth + 1)?;
                let left = self.require(left, Type::I64)?;
                let right = self.expression(*right, depth + 1)?;
                let right = self.require(right, Type::I64)?;
                left.zip(right).map(|(left, right)| match expression.kind {
                    source::ExpressionKind::Add(_, _) => {
                        (ExpressionKind::Add(left, right), Type::I64)
                    }
                    _ => (ExpressionKind::LessThan(left, right), Type::Bool),
                })
            }
        };
        self.active_expressions.remove(&id);
        Ok(value.map(|(kind, value_type)| {
            let id = ExpressionId(self.expressions.len());
            self.expressions.push(Expression {
                kind,
                value_type,
                span: expression.span,
            });
            id
        }))
    }

    fn statement(
        &mut self,
        statement: &source::Statement,
        depth: usize,
    ) -> Result<(Option<Statement>, bool), InputError> {
        self.check_span(statement.span, depth)?;
        let (kind, falls_through) = match &statement.kind {
            source::StatementKind::LetMut { name, initializer } => {
                let initializer = self.expression(*initializer, depth + 1)?;
                let initializer = self.require(initializer, Type::I64)?;
                // The name is deliberately not visible inside its initializer.
                let local = self.declare(name, LocalKind::Mutable, depth)?;
                (
                    local
                        .zip(initializer)
                        .map(|(local, initializer)| StatementKind::LetMut { local, initializer }),
                    true,
                )
            }
            source::StatementKind::Assign { target, value } => {
                let local = self.resolve(target, depth)?;
                if let Some(local) =
                    local.filter(|id| self.locals[id.index()].kind == LocalKind::Parameter)
                {
                    let mut diagnostic = Diagnostic::new(
                        codes::IMMUTABLE_ASSIGNMENT,
                        Severity::Error,
                        "cannot assign to an immutable parameter",
                        Label::new(target.span, "assignment target"),
                    );
                    diagnostic.secondary.push(Label::new(
                        self.locals[local.index()].span,
                        "parameter declared here",
                    ));
                    self.report(diagnostic)?;
                }
                let value = self.expression(*value, depth + 1)?;
                let value = self.require(value, Type::I64)?;
                (
                    local
                        .zip(value)
                        .map(|(local, value)| StatementKind::Assign { local, value }),
                    true,
                )
            }
            source::StatementKind::Return(value) => {
                let value = self.expression(*value, depth + 1)?;
                (
                    self.require(value, Type::I64)?.map(StatementKind::Return),
                    false,
                )
            }
            source::StatementKind::If {
                condition,
                then_block,
                else_block,
            } => {
                let condition = self.expression(*condition, depth + 1)?;
                let condition = self.require(condition, Type::Bool)?;
                let then_block = self.block(*then_block, depth + 1)?;
                let else_block = else_block
                    .map(|block| self.block(block, depth + 1))
                    .transpose()?;
                let falls_through = self.blocks[then_block.index()].falls_through
                    || else_block.is_none_or(|block| self.blocks[block.index()].falls_through);
                (
                    condition.map(|condition| StatementKind::If {
                        condition,
                        then_block,
                        else_block,
                    }),
                    falls_through,
                )
            }
            source::StatementKind::While { condition, body } => {
                let condition = self.expression(*condition, depth + 1)?;
                let condition = self.require(condition, Type::Bool)?;
                let body = self.block(*body, depth + 1)?;
                (
                    condition.map(|condition| StatementKind::While { condition, body }),
                    true,
                )
            }
        };
        Ok((
            kind.map(|kind| Statement {
                kind,
                span: statement.span,
            }),
            falls_through,
        ))
    }

    fn block(&mut self, id: source::BlockId, depth: usize) -> Result<BlockId, InputError> {
        let input = self.input;
        let block = input
            .blocks
            .get(id.0)
            .ok_or_else(|| self.error(InputErrorKind::InvalidBlockId(id.0), input.span))?;
        self.check_span(block.span, depth)?;
        self.check_span(block.closing_brace, depth)?;
        if !self.active_blocks.insert(id) {
            return Err(self.error(InputErrorKind::CyclicBlock, block.span));
        }
        self.scopes.push(BTreeMap::new());
        let mut statements = Vec::new();
        let mut falls_through = true;
        for statement in &block.statements {
            if !falls_through {
                self.check_diagnostic_budget(statement.span)?;
                self.warnings.push(Warning::UnreachableStatement {
                    span: statement.span,
                });
            }
            let (typed, statement_falls_through) = self.statement(statement, depth + 1)?;
            if let Some(typed) = typed {
                statements.push(typed);
            }
            falls_through &= statement_falls_through;
        }
        self.scopes.pop();
        self.active_blocks.remove(&id);
        let result = BlockId(self.blocks.len());
        self.blocks.push(Block {
            statements,
            span: block.span,
            closing_brace: block.closing_brace,
            falls_through,
        });
        Ok(result)
    }
}
