//! Non-executable RFC 0022 source snapshots. Typing/move/scope snapshots and
//! suspension-loan proofs are separate; neither constructs executable MIR.

use crate::{
    loans, moves,
    program::{self, BodyTypes, CheckError, CheckLimits},
    resolved as r, types,
};
use lang_diagnostics::{Diagnostic, Label, Severity, SourceMap, Span, codes};

pub(crate) fn source_feature(source: &r::Program) -> Option<Span> {
    for ty in &source.types {
        if matches!(
            ty.constructor,
            r::TypeConstructor::Task(_) | r::TypeConstructor::Builtin(r::BuiltinType::TaskError)
        ) {
            return Some(ty.span);
        }
    }
    for f in &source.functions {
        if let r::FunctionEffect::Asynchronous { span } = f.effect {
            return Some(span);
        }
        for e in &f.expressions {
            if matches!(e.kind, r::ExpressionKind::CoreCall{operation,..} if operation.is_channel() || operation.is_service())
            {
                return Some(e.span);
            }
            if matches!(
                e.kind,
                r::ExpressionKind::Spawn(_)
                    | r::ExpressionKind::Await(_)
                    | r::ExpressionKind::CoreCase {
                        case: crate::core::Case::TaskCancelled,
                        ..
                    }
            ) {
                return Some(e.span);
            }
        }
        for p in &f.patterns {
            if matches!(
                p.kind,
                r::PatternKind::CoreCase {
                    case: crate::core::Case::TaskCancelled,
                    ..
                }
            ) {
                return Some(p.span);
            }
        }
        for b in &f.blocks {
            for s in &b.statements {
                if matches!(
                    s.kind,
                    r::StatementKind::TaskScope { .. }
                        | r::StatementKind::Yield
                        | r::StatementKind::Cancel(_)
                        | r::StatementKind::CancelCheck
                ) {
                    return Some(s.span);
                }
            }
        }
    }
    None
}

fn preflight_error(error: types::Error) -> CheckError {
    match error {
        types::Error::Limit(d) => CheckError::Limit(d),
        error => CheckError::Types(error),
    }
}

/// Bound the raw classifier before traversing caller-constructed arenas.
/// Bounded profile discovery only; source and independent MIR proofs remain required.
pub fn feature_span(
    source: &r::Program,
    sources: &SourceMap,
    limits: CheckLimits,
) -> Result<Option<Span>, CheckError> {
    let Some(root) = source.modules.first().map(|m| m.span) else {
        return Ok(None);
    };
    sources.slice(root).map_err(|e| CheckError::Input {
        kind: program::CheckInputErrorKind::Source(e),
        span: Some(root),
    })?;
    let mut preflight = limits.types;
    preflight.work = limits.work;
    let mut budget = types::Budget {
        limits: preflight,
        work: 0,
        allocations: 0,
    };
    if source.types.len() > limits.types.constructed
        || source.functions.len() > limits.functions
        || source.modules.len() > limits.modules
    {
        return Err(preflight_error(
            budget.limit(root, "task source declaration limit exceeded"),
        ));
    }
    let mut counts = [0usize; 4];
    for f in &source.functions {
        budget.charge(root, 1).map_err(preflight_error)?;
        for (i, n) in [
            f.locals.len(),
            f.expressions.len(),
            f.patterns.len(),
            f.blocks.len(),
        ]
        .into_iter()
        .enumerate()
        {
            counts[i] = counts[i]
                .checked_add(n)
                .ok_or_else(|| preflight_error(budget.limit(root, "task arena count overflow")))?;
        }
        for b in &f.blocks {
            budget
                .charge(root, b.statements.len() + 1)
                .map_err(preflight_error)?;
        }
    }
    if counts
        .iter()
        .zip([
            limits.locals,
            limits.expressions,
            limits.patterns,
            limits.blocks,
        ])
        .any(|(n, limit)| *n > limit)
    {
        return Err(preflight_error(
            budget.limit(root, "task source arena limit exceeded"),
        ));
    }
    budget
        .charge(
            root,
            source
                .types
                .len()
                .saturating_add(counts[1])
                .saturating_add(counts[2]),
        )
        .map_err(preflight_error)?;
    Ok(source_feature(source))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionClaims {
    origins: Vec<Vec<r::BlockId>>,
}
impl FunctionClaims {
    /// Possible creating task scopes for an expression, in stable ID order.
    pub fn origins(&self, expression: r::ExpressionId) -> &[r::BlockId] {
        &self.origins[expression.0]
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskTypes {
    moved: moves::MoveCheckedBodies,
    functions: Vec<FunctionClaims>,
}
impl TaskTypes {
    pub fn moved(&self) -> &moves::MoveCheckedBodies {
        &self.moved
    }
    pub fn bodies(&self) -> &BodyTypes {
        self.moved.bodies()
    }
    pub fn functions(&self) -> &[FunctionClaims] {
        &self.functions
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Analysis {
    pub program: Option<TaskTypes>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Combined source claim and suspension-loan evidence, not a MIR proof.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskLoans {
    loaned: loans::LoanCheckedBodies,
    functions: Vec<FunctionClaims>,
}
impl TaskLoans {
    pub fn loaned(&self) -> &loans::LoanCheckedBodies {
        &self.loaned
    }
    pub fn functions(&self) -> &[FunctionClaims] {
        &self.functions
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanAnalysis {
    pub program: Option<TaskLoans>,
    pub diagnostics: Vec<Diagnostic>,
}
pub fn check_loans(
    typed: TaskTypes,
    sources: &SourceMap,
    limits: loans::Limits,
) -> Result<LoanAnalysis, loans::Error> {
    let checked = loans::analyze(typed.moved, sources, limits)?;
    Ok(LoanAnalysis {
        program: checked.bodies.map(|loaned| TaskLoans {
            loaned,
            functions: typed.functions,
        }),
        diagnostics: checked.diagnostics,
    })
}

pub fn analyze(
    source: r::Program,
    sources: &SourceMap,
    limits: CheckLimits,
) -> Result<Analysis, CheckError> {
    feature_span(&source, sources, limits)?;
    let checked = program::analyze_bodies(source, sources, limits)?;
    let Some(bodies) = checked.bodies else {
        return Ok(Analysis {
            program: None,
            diagnostics: checked.diagnostics,
        });
    };
    let moved = moves::analyze(bodies, sources, limits.moves).map_err(CheckError::Moves)?;
    let mut diagnostics = checked.diagnostics;
    diagnostics.extend(moved.diagnostics);
    let Some(moved) = moved.bodies else {
        return Ok(Analysis {
            program: None,
            diagnostics,
        });
    };
    let mut context = Context {
        budget: types::Budget {
            limits: limits.types,
            work: 0,
            allocations: 0,
        },
        diagnostics,
        limit: limits.diagnostics,
    };
    let mut functions = Vec::new();
    let root = moved.bodies().source().modules[0].span;
    for (i, function) in moved.bodies().source().functions.iter().enumerate() {
        let facts = &moved.bodies().functions()[i];
        if function.effect.is_async() {
            for ty in std::iter::once(function.result).chain(
                function
                    .parameters
                    .iter()
                    .map(|id| function.locals[id.0].annotation.unwrap()),
            ) {
                context
                    .budget
                    .charge(ty.span, 1)
                    .map_err(CheckError::Types)?;
                let p = moved.bodies().properties(ty.value_type).unwrap();
                if !p.is_send() {
                    context.error(ty.span, "async transfers require Send values without loans, arenas, allocator authority, Task claims or component resources", None, codes::INVALID_TASK)?;
                }
            }
        }
        let origins = context.empty_state(function.span, function.expressions.len())?;
        let state = context.empty_state(function.span, function.locals.len())?;
        let mut parents = context
            .budget
            .vector(function.span, function.blocks.len())
            .map_err(CheckError::Types)?;
        parents.resize(function.blocks.len(), None);
        let mut declarations = context
            .budget
            .vector(function.span, function.locals.len())
            .map_err(CheckError::Types)?;
        declarations.resize(function.locals.len(), None);
        let mut scopes = Vec::new();
        context
            .budget
            .push(function.span, &mut scopes, function.body)
            .map_err(CheckError::Types)?;
        let mut checker = Checker {
            context: &mut context,
            bodies: moved.bodies(),
            function,
            facts,
            parents,
            declarations,
            origins,
            state,
            lexical: Vec::new(),
            scopes,
            depth: limits.depth.min(256),
        };
        checker.block(function.body, 0)?;
        let claims = FunctionClaims {
            origins: checker.origins,
        };
        context
            .budget
            .push(root, &mut functions, claims)
            .map_err(CheckError::Types)?;
    }
    let success = !context
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
    Ok(Analysis {
        program: success.then_some(TaskTypes { moved, functions }),
        diagnostics: context.diagnostics,
    })
}

type State = Vec<Vec<r::BlockId>>;
struct Context {
    budget: types::Budget,
    diagnostics: Vec<Diagnostic>,
    limit: usize,
}
impl Context {
    fn empty_state(&mut self, span: Span, n: usize) -> Result<State, CheckError> {
        let mut state = self.budget.vector(span, n).map_err(CheckError::Types)?;
        self.budget.charge(span, n).map_err(CheckError::Types)?;
        state.resize_with(n, Vec::new);
        Ok(state)
    }
    fn copy_origins(
        &mut self,
        span: Span,
        values: &[r::BlockId],
    ) -> Result<Vec<r::BlockId>, CheckError> {
        let mut result = self
            .budget
            .vector(span, values.len())
            .map_err(CheckError::Types)?;
        self.budget
            .charge(span, values.len())
            .map_err(CheckError::Types)?;
        result.extend_from_slice(values);
        Ok(result)
    }
    fn snapshot(&mut self, span: Span, values: &State) -> Result<State, CheckError> {
        let mut result = self.empty_state(span, values.len())?;
        for (out, input) in result.iter_mut().zip(values) {
            *out = self.copy_origins(span, input)?;
        }
        Ok(result)
    }
    fn union(
        &mut self,
        span: Span,
        to: &mut Vec<r::BlockId>,
        from: &[r::BlockId],
    ) -> Result<(), CheckError> {
        for value in from {
            self.budget
                .charge(span, to.len() + 1)
                .map_err(CheckError::Types)?;
            if let Err(at) = to.binary_search(value) {
                self.budget
                    .push(span, to, *value)
                    .map_err(CheckError::Types)?;
                to[at..].rotate_right(1);
            }
        }
        Ok(())
    }
    fn merge(&mut self, span: Span, a: &State, b: &State) -> Result<State, CheckError> {
        let mut result = self.snapshot(span, a)?;
        for (to, from) in result.iter_mut().zip(b) {
            self.union(span, to, from)?;
        }
        Ok(result)
    }
    fn error(
        &mut self,
        span: Span,
        message: &str,
        related: Option<Span>,
        code: lang_diagnostics::DiagnosticCode,
    ) -> Result<(), CheckError> {
        if self.diagnostics.len() >= self.limit {
            return Err(CheckError::Types(
                self.budget.limit(span, "task diagnostic limit exceeded"),
            ));
        }
        let mut d = Diagnostic::new(
            code,
            Severity::Error,
            self.budget.text(span, message).map_err(CheckError::Types)?,
            Label::new(
                span,
                self.budget.text(span, message).map_err(CheckError::Types)?,
            ),
        );
        if let Some(origin) = related {
            let label = self
                .budget
                .text(span, "creating task scope")
                .map_err(CheckError::Types)?;
            self.budget
                .push(span, &mut d.secondary, Label::new(origin, label))
                .map_err(CheckError::Types)?;
        }
        self.budget
            .push(span, &mut self.diagnostics, d)
            .map_err(CheckError::Types)
    }
}

struct Checker<'a, 'b> {
    context: &'b mut Context,
    bodies: &'a BodyTypes,
    function: &'a r::Function,
    facts: &'a program::FunctionFacts,
    parents: Vec<Option<r::BlockId>>,
    declarations: Vec<Option<r::BlockId>>,
    origins: State,
    state: State,
    lexical: Vec<r::BlockId>,
    scopes: Vec<r::BlockId>,
    depth: usize,
}
impl Checker<'_, '_> {
    fn enter(&mut self, span: Span, depth: usize) -> Result<(), CheckError> {
        self.context
            .budget
            .charge(span, 1)
            .map_err(CheckError::Types)?;
        if depth > self.depth {
            return Err(CheckError::Types(
                self.context.budget.limit(span, "task scope depth exceeded"),
            ));
        }
        Ok(())
    }
    fn block(&mut self, id: r::BlockId, depth: usize) -> Result<bool, CheckError> {
        let block = &self.function.blocks[id.0];
        self.enter(block.span, depth)?;
        self.parents[id.0] = self.lexical.last().copied();
        self.context
            .budget
            .push(block.span, &mut self.lexical, id)
            .map_err(CheckError::Types)?;
        let mut alive = true;
        for statement in &block.statements {
            if alive {
                alive = self.statement(statement, depth + 1)?;
            }
        }
        for (i, declared) in self.declarations.iter().enumerate() {
            self.context
                .budget
                .charge(block.span, 1)
                .map_err(CheckError::Types)?;
            if *declared == Some(id) {
                self.state[i].clear();
            }
        }
        self.lexical.pop();
        Ok(alive)
    }
    fn store(
        &mut self,
        local: r::LocalId,
        value: Vec<r::BlockId>,
        span: Span,
    ) -> Result<(), CheckError> {
        for scope in &value {
            let mut declaration = self.declarations[local.0];
            let mut inside = false;
            while let Some(block) = declaration {
                self.context
                    .budget
                    .charge(span, 1)
                    .map_err(CheckError::Types)?;
                if block == *scope {
                    inside = true;
                    break;
                }
                declaration = self.parents[block.0];
            }
            if !inside {
                self.context.error(
                    span,
                    "Task claim escapes its creating task scope",
                    Some(self.function.blocks[scope.0].span),
                    codes::INVALID_TASK,
                )?;
            }
        }
        self.state[local.0] = value;
        Ok(())
    }
    fn expression(
        &mut self,
        id: r::ExpressionId,
        depth: usize,
    ) -> Result<Vec<r::BlockId>, CheckError> {
        let expression = &self.function.expressions[id.0];
        self.enter(expression.span, depth)?;
        for child in expression.kind.operands() {
            self.expression(child, depth + 1)?;
            if self.facts.diverges(child) {
                return Ok(Vec::new());
            }
        }
        let values = if self
            .bodies
            .properties(self.facts.expressions()[id.0])
            .unwrap()
            .contains_task
        {
            match expression.kind {
                r::ExpressionKind::Local(local) => self
                    .context
                    .copy_origins(expression.span, &self.state[local.0])?,
                r::ExpressionKind::Spawn(value) => self
                    .context
                    .copy_origins(expression.span, &self.origins[value.0])?,
                r::ExpressionKind::Call {
                    callee: r::Callable::Guest(_),
                    ..
                } => {
                    let mut values = Vec::new();
                    self.context
                        .budget
                        .push(expression.span, &mut values, *self.scopes.last().unwrap())
                        .map_err(CheckError::Types)?;
                    values
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        self.context
            .union(expression.span, &mut self.origins[id.0], &values)?;
        Ok(values)
    }
    fn statement(&mut self, s: &r::Statement, depth: usize) -> Result<bool, CheckError> {
        self.enter(s.span, depth)?;
        match &s.kind {
            r::StatementKind::TaskScope { body } => {
                self.context
                    .budget
                    .push(s.span, &mut self.scopes, *body)
                    .map_err(CheckError::Types)?;
                let alive = self.block(*body, depth + 1)?;
                self.scopes.pop();
                return Ok(alive);
            }
            r::StatementKind::Unsafe { body } => return self.block(*body, depth + 1),
            r::StatementKind::LetMut { local, initializer } => {
                self.declarations[local.0] = self.lexical.last().copied();
                let values = self.expression(*initializer, depth + 1)?;
                self.store(*local, values, s.span)?;
            }
            r::StatementKind::Assign { local, value } => {
                let values = self.expression(*value, depth + 1)?;
                self.store(*local, values, s.span)?;
            }
            r::StatementKind::If {
                condition,
                then_block,
                else_block,
            } => {
                self.expression(*condition, depth + 1)?;
                let before = self.context.snapshot(s.span, &self.state)?;
                let yes_alive = self.block(*then_block, depth + 1)?;
                let yes =
                    std::mem::replace(&mut self.state, self.context.snapshot(s.span, &before)?);
                let no_alive = if let Some(no) = else_block {
                    self.block(*no, depth + 1)?
                } else {
                    true
                };
                self.state = match (yes_alive, no_alive) {
                    (true, true) => self.context.merge(s.span, &yes, &self.state)?,
                    (true, false) => yes,
                    _ => std::mem::take(&mut self.state),
                };
                return Ok(yes_alive || no_alive);
            }
            r::StatementKind::While { condition, body } => loop {
                let before = self.context.snapshot(s.span, &self.state)?;
                self.expression(*condition, depth + 1)?;
                let alive = self.block(*body, depth + 1)?;
                let next = if alive {
                    self.context.merge(s.span, &before, &self.state)?
                } else {
                    self.context.snapshot(s.span, &before)?
                };
                self.context
                    .budget
                    .charge(s.span, next.len())
                    .map_err(CheckError::Types)?;
                let stable = next == before;
                self.state = next;
                if stable {
                    break;
                }
            },
            r::StatementKind::Match { scrutinee, arms } => {
                self.expression(*scrutinee, depth + 1)?;
                let before = self.context.snapshot(s.span, &self.state)?;
                let mut merged = None;
                for arm in arms {
                    if self.facts.arm_is_unreachable(arm.pattern) {
                        continue;
                    }
                    self.state = self.context.snapshot(s.span, &before)?;
                    if self.block(arm.body, depth + 1)? {
                        merged = Some(if let Some(previous) = merged {
                            self.context.merge(s.span, &previous, &self.state)?
                        } else {
                            self.context.snapshot(s.span, &self.state)?
                        });
                    }
                }
                let alive = merged.is_some();
                self.state = merged.unwrap_or(before);
                return Ok(alive);
            }
            r::StatementKind::Arena {
                allocator, body, ..
            } => {
                self.expression(*allocator, depth + 1)?;
                return self.block(*body, depth + 1);
            }
            _ => {
                for expression in s.kind.operands() {
                    self.expression(expression, depth + 1)?;
                }
            }
        }
        Ok(!matches!(s.kind, r::StatementKind::Return(_))
            && !s.kind.operands().any(|id| self.facts.diverges(id)))
    }
}
