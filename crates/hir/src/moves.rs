//! Definite initialization and whole-local consumption over typed source HIR.
//! This is one ownership pass: loan conflicts, mutability, region provenance,
//! drops, layouts, and executable promotion require their own validation.

use crate::{
    program::{BodyTypes, FunctionFacts},
    resolved as r,
};
use lang_diagnostics::{Diagnostic, Label, Severity, SourceError, SourceMap, Span, codes};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub functions: usize,
    pub locals: usize,
    pub depth: usize,
    pub work: usize,
    /// Cumulative requested analysis buffers, excluding the typed input,
    /// emergency error, and allocator bookkeeping/fragmentation.
    pub allocations: usize,
    pub diagnostics: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            functions: 4096,
            locals: 1_000_000,
            depth: 256,
            work: 10_000_000,
            allocations: 128 * 1024 * 1024,
            diagnostics: 4096,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Source(SourceError),
    Limit(Box<Diagnostic>),
    AllocationFailed { span: Span },
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "source move analysis failed: {self:?}")
    }
}
impl StdError for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViolationKind {
    Uninitialized,
    Moved,
    MaybeMoved,
    PartialMove,
    BorrowedMove,
    ArenaOwnerMove,
    LoopState,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Violation {
    pub function: r::FunctionId,
    pub local: Option<r::LocalId>,
    pub kind: ViolationKind,
    pub span: Span,
    pub origin: Option<Span>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessKind {
    Read,
    Move,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Access {
    pub expression: r::ExpressionId,
    pub local: r::LocalId,
    pub kind: AccessKind,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionMoves {
    accesses: Vec<Access>,
}
impl FunctionMoves {
    pub fn accesses(&self) -> &[Access] {
        &self.accesses
    }
}

/// Constructed only after this pass succeeds. It grants no execution/loan or
/// region validity. The original BodyTypes and source snapshots stay intact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveCheckedBodies {
    bodies: BodyTypes,
    functions: Vec<FunctionMoves>,
    work: usize,
    allocations: usize,
}
impl MoveCheckedBodies {
    pub fn bodies(&self) -> &BodyTypes {
        &self.bodies
    }
    pub fn functions(&self) -> &[FunctionMoves] {
        &self.functions
    }
    pub const fn work_used(&self) -> usize {
        self.work
    }
    pub const fn requested_bytes(&self) -> usize {
        self.allocations
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Analysis {
    pub bodies: Option<MoveCheckedBodies>,
    pub diagnostics: Vec<Diagnostic>,
    pub violations: Vec<Violation>,
}

struct Budget {
    limits: Limits,
    work: usize,
    allocations: usize,
}
impl Budget {
    fn limit(&self, span: Span, message: &str) -> Error {
        Error::Limit(Box::new(Diagnostic::new(
            codes::RESOURCE_LIMIT,
            Severity::Error,
            message,
            Label::new(span, message),
        )))
    }
    fn charge(&mut self, span: Span, count: usize) -> Result<(), Error> {
        self.work = self
            .work
            .checked_add(count)
            .filter(|n| *n <= self.limits.work)
            .ok_or_else(|| self.limit(span, "move analysis work limit exceeded"))?;
        Ok(())
    }
    fn enter(&mut self, span: Span, depth: usize) -> Result<(), Error> {
        self.charge(span, 1)?;
        if depth >= self.limits.depth.min(256) {
            return Err(self.limit(span, "move analysis depth limit exceeded"));
        }
        Ok(())
    }
    fn allocate<T>(&mut self, span: Span, count: usize) -> Result<(), Error> {
        self.allocations = count
            .checked_mul(std::mem::size_of::<T>())
            .and_then(|n| self.allocations.checked_add(n))
            .filter(|n| *n <= self.limits.allocations)
            .ok_or_else(|| self.limit(span, "move analysis allocation limit exceeded"))?;
        Ok(())
    }
    fn vector<T>(&mut self, span: Span, count: usize) -> Result<Vec<T>, Error> {
        self.allocate::<T>(span, count)?;
        let mut v = Vec::new();
        v.try_reserve_exact(count)
            .map_err(|_| Error::AllocationFailed { span })?;
        Ok(v)
    }
    fn push<T>(&mut self, span: Span, values: &mut Vec<T>, value: T) -> Result<(), Error> {
        self.charge(span, 1)?;
        if values.len() == values.capacity() {
            let next = values
                .capacity()
                .max(2)
                .checked_mul(2)
                .ok_or_else(|| self.limit(span, "move analysis buffer overflow"))?;
            self.allocate::<T>(span, next)?;
            values
                .try_reserve_exact(next - values.len())
                .map_err(|_| Error::AllocationFailed { span })?;
        }
        values.push(value);
        Ok(())
    }
    fn copy<T: Copy>(&mut self, span: Span, values: &[T]) -> Result<Vec<T>, Error> {
        self.charge(span, values.len())?;
        let mut copy = self.vector(span, values.len())?;
        copy.extend_from_slice(values);
        Ok(copy)
    }
    fn text(&mut self, span: Span, value: &str) -> Result<String, Error> {
        self.charge(span, value.len())?;
        self.allocate::<u8>(span, value.len())?;
        let mut text = String::new();
        text.try_reserve_exact(value.len())
            .map_err(|_| Error::AllocationFailed { span })?;
        text.push_str(value);
        Ok(text)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Absent,
    Ready,
    Moved(Span),
    MaybeMoved(Span),
}
impl State {
    fn origin(self) -> Option<Span> {
        match self {
            Self::Moved(span) | Self::MaybeMoved(span) => Some(span),
            _ => None,
        }
    }
    fn same_initialization(self, other: Self) -> bool {
        std::mem::discriminant(&self) == std::mem::discriminant(&other)
    }
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Ready, Self::Ready) => Self::Ready,
            (Self::Absent, Self::Absent) => Self::Absent,
            (Self::Moved(span), Self::Moved(_)) => Self::Moved(span),
            (Self::Moved(span) | Self::MaybeMoved(span), _)
            | (_, Self::Moved(span) | Self::MaybeMoved(span)) => Self::MaybeMoved(span),
            // Typed lexical scopes prevent Ready/Absent joins for visible
            // locals. An absent branch must never grant initialization.
            _ => Self::Absent,
        }
    }
}
#[derive(Clone, Copy)]
enum Mode {
    Value,
    Observe,
}

struct Context<'a> {
    bodies: &'a BodyTypes,
    budget: Budget,
    diagnostics: Vec<Diagnostic>,
    violations: Vec<Violation>,
}
impl Context<'_> {
    fn report(&mut self, violation: Violation) -> Result<(), Error> {
        let span = violation.span;
        if self.violations.len() >= self.budget.limits.diagnostics {
            return Err(self.budget.limit(span, "move diagnostic limit exceeded"));
        }
        let (code, message) = match violation.kind {
            ViolationKind::Uninitialized => (
                codes::INVALID_OWNER_USE,
                "value is not initialized on this path",
            ),
            ViolationKind::Moved => (codes::INVALID_OWNER_USE, "value was moved before this use"),
            ViolationKind::MaybeMoved => (
                codes::INVALID_OWNER_USE,
                "value is moved on a reaching path",
            ),
            ViolationKind::LoopState => (
                codes::INVALID_OWNER_USE,
                "loop back-edge must restore the header initialization and move state",
            ),
            ViolationKind::PartialMove => (
                codes::INVALID_OWNERSHIP_TYPE,
                "moving a non-Copy field is a forbidden partial move",
            ),
            ViolationKind::BorrowedMove => (
                codes::INVALID_OWNERSHIP_TYPE,
                "a non-Copy value cannot be moved out of a loan",
            ),
            ViolationKind::ArenaOwnerMove => (
                codes::INVALID_OWNERSHIP_TYPE,
                "a lexical arena owner cannot be moved",
            ),
        };
        let message_text = self.budget.text(span, message)?;
        let primary = Label::new(span, self.budget.text(span, message)?);
        let mut diagnostic = Diagnostic::new(code, Severity::Error, message_text, primary);
        if let Some(origin) = violation.origin {
            let label = Label::new(
                origin,
                self.budget
                    .text(origin, "move contributing to this state")?,
            );
            self.budget.push(span, &mut diagnostic.secondary, label)?;
        }
        self.budget.push(span, &mut self.diagnostics, diagnostic)?;
        self.budget.push(span, &mut self.violations, violation)
    }
}

pub fn analyze(bodies: BodyTypes, sources: &SourceMap, limits: Limits) -> Result<Analysis, Error> {
    let root = bodies.source().modules[0].span;
    for module in &bodies.source().modules {
        sources.slice(module.span).map_err(Error::Source)?;
    }
    let mut context = Context {
        bodies: &bodies,
        budget: Budget {
            limits,
            work: 0,
            allocations: 0,
        },
        diagnostics: Vec::new(),
        violations: Vec::new(),
    };
    if bodies.source().functions.len() > limits.functions {
        return Err(context.budget.limit(root, "move function limit exceeded"));
    }
    let mut total_locals = 0usize;
    let mut functions = context
        .budget
        .vector(root, bodies.source().functions.len())?;
    for (index, function) in bodies.source().functions.iter().enumerate() {
        total_locals = total_locals
            .checked_add(function.locals.len())
            .filter(|n| *n <= limits.locals)
            .ok_or_else(|| {
                context
                    .budget
                    .limit(function.span, "move local limit exceeded")
            })?;
        let mut state = context
            .budget
            .vector(function.span, function.locals.len())?;
        context
            .budget
            .charge(function.span, function.locals.len())?;
        state.resize(function.locals.len(), State::Absent);
        for local in &function.parameters {
            context.budget.charge(function.span, 1)?;
            state[local.0] = State::Ready;
        }
        let mut checker = Checker {
            context: &mut context,
            function,
            types: &bodies.functions()[index],
            id: r::FunctionId(index),
            state,
            scopes: Vec::new(),
            accesses: Vec::new(),
        };
        checker.block(function.body, 0)?;
        functions.push(FunctionMoves {
            accesses: checker.accesses,
        });
    }
    let successful = context.violations.is_empty();
    let work = context.budget.work;
    let allocations = context.budget.allocations;
    let diagnostics = context.diagnostics;
    let violations = context.violations;
    Ok(Analysis {
        bodies: successful.then_some(MoveCheckedBodies {
            bodies,
            functions,
            work,
            allocations,
        }),
        diagnostics,
        violations,
    })
}

struct Checker<'a, 'b> {
    context: &'b mut Context<'a>,
    function: &'a r::Function,
    types: &'a FunctionFacts,
    id: r::FunctionId,
    state: Vec<State>,
    scopes: Vec<Vec<r::LocalId>>,
    accesses: Vec<Access>,
}
impl Checker<'_, '_> {
    fn constructor(&self, ty: r::Type) -> Option<r::TypeConstructor> {
        if let r::Type::Constructed(id) = ty {
            Some(self.context.bodies.types()[id.0].constructor)
        } else {
            None
        }
    }
    fn loan(&self, ty: r::Type) -> bool {
        matches!(self.constructor(ty), Some(r::TypeConstructor::Loan(..)))
    }
    fn copy_type(&self, ty: r::Type) -> bool {
        self.context.bodies.properties(ty).unwrap().is_copy
    }
    fn violation(
        &mut self,
        local: Option<r::LocalId>,
        kind: ViolationKind,
        span: Span,
        origin: Option<Span>,
    ) -> Result<(), Error> {
        self.context.report(Violation {
            function: self.id,
            local,
            kind,
            span,
            origin,
        })
    }
    fn ready(&mut self, local: r::LocalId, span: Span) -> Result<bool, Error> {
        self.context.budget.charge(span, 1)?;
        let kind = match self.state[local.0] {
            State::Ready => return Ok(true),
            State::Absent => ViolationKind::Uninitialized,
            State::Moved(_) => ViolationKind::Moved,
            State::MaybeMoved(_) => ViolationKind::MaybeMoved,
        };
        self.violation(Some(local), kind, span, self.state[local.0].origin())?;
        Ok(false)
    }
    fn declare(&mut self, local: r::LocalId, span: Span) -> Result<(), Error> {
        self.state[local.0] = State::Ready;
        self.context
            .budget
            .push(span, self.scopes.last_mut().unwrap(), local)
    }
    fn leave(&mut self, span: Span) -> Result<(), Error> {
        for local in self.scopes.pop().unwrap() {
            self.context.budget.charge(span, 1)?;
            self.state[local.0] = State::Absent;
        }
        Ok(())
    }
    fn merge(
        &mut self,
        mut left: Vec<State>,
        right: &[State],
        span: Span,
    ) -> Result<Vec<State>, Error> {
        for (left, right) in left.iter_mut().zip(right) {
            self.context.budget.charge(span, 1)?;
            *left = left.join(*right);
        }
        Ok(left)
    }
    fn block(&mut self, id: r::BlockId, depth: usize) -> Result<bool, Error> {
        let block = &self.function.blocks[id.0];
        self.context.budget.enter(block.span, depth)?;
        self.context
            .budget
            .push(block.span, &mut self.scopes, Vec::new())?;
        let mut reaches = true;
        for statement in &block.statements {
            if !reaches {
                break;
            }
            reaches = self.statement(statement, depth + 1)?;
        }
        self.leave(block.span)?;
        Ok(reaches)
    }
    fn statement(&mut self, statement: &r::Statement, depth: usize) -> Result<bool, Error> {
        let span = statement.span;
        self.context.budget.enter(span, depth)?;
        match &statement.kind {
            r::StatementKind::Unsafe { body } => return self.block(*body, depth + 1),
            r::StatementKind::LetMut { local, initializer } => {
                self.eval(*initializer, Mode::Value, depth + 1)?;
                self.declare(*local, span)?;
            }
            r::StatementKind::Assign { local, value } => {
                self.eval(*value, Mode::Value, depth + 1)?;
                self.state[local.0] = State::Ready;
            }
            r::StatementKind::PlaceAssign { place, value } => {
                let root = self.eval(r::ExpressionId(place.0), Mode::Observe, depth + 1)?;
                self.eval(*value, Mode::Value, depth + 1)?;
                if let Some(root) = root {
                    self.ready(root, span)?;
                }
            }
            r::StatementKind::Return(value) => {
                self.eval(*value, Mode::Value, depth + 1)?;
                return Ok(false);
            }
            r::StatementKind::Arena {
                owner,
                allocator,
                body,
            } => {
                self.eval(*allocator, Mode::Observe, depth + 1)?;
                self.context
                    .budget
                    .push(span, &mut self.scopes, Vec::new())?;
                self.declare(*owner, span)?;
                let reaches = self.block(*body, depth + 1)?;
                self.leave(span)?;
                return Ok(reaches);
            }
            r::StatementKind::If {
                condition,
                then_block,
                else_block,
            } => {
                self.eval(*condition, Mode::Value, depth + 1)?;
                let base = std::mem::take(&mut self.state);
                self.state = self.context.budget.copy(span, &base)?;
                let then_reaches = self.block(*then_block, depth + 1)?;
                let then_state = std::mem::take(&mut self.state);
                self.state = self.context.budget.copy(span, &base)?;
                let else_reaches = if let Some(block) = else_block {
                    self.block(*block, depth + 1)?
                } else {
                    true
                };
                let else_state = std::mem::take(&mut self.state);
                self.state = match (then_reaches, else_reaches) {
                    (true, true) => self.merge(then_state, &else_state, span)?,
                    (true, false) => then_state,
                    (false, true) => else_state,
                    (false, false) => base,
                };
                return Ok(then_reaches || else_reaches);
            }
            r::StatementKind::While { condition, body } => {
                let header = self.context.budget.copy(span, &self.state)?;
                self.eval(*condition, Mode::Value, depth + 1)?;
                let exit = self.context.budget.copy(span, &self.state)?;
                if self.block(*body, depth + 1)? {
                    for (index, before) in header.iter().enumerate() {
                        self.context.budget.charge(span, 1)?;
                        if !before.same_initialization(self.state[index]) {
                            self.violation(
                                Some(r::LocalId(index)),
                                ViolationKind::LoopState,
                                span,
                                self.state[index].origin().or(before.origin()),
                            )?;
                        }
                    }
                }
                self.state = exit;
            }
            r::StatementKind::Match { scrutinee, arms } => {
                self.eval(*scrutinee, Mode::Observe, depth + 1)?;
                let base = std::mem::take(&mut self.state);
                let mut joined = None;
                for arm in arms {
                    self.context.budget.charge(arm.span, 1)?;
                    if self.types.arm_is_unreachable(arm.pattern) {
                        continue;
                    }
                    self.state = self.context.budget.copy(arm.span, &base)?;
                    self.context
                        .budget
                        .push(arm.span, &mut self.scopes, Vec::new())?;
                    self.pattern(arm.pattern, depth + 1)?;
                    let reaches = self.block(arm.body, depth + 1)?;
                    self.leave(arm.span)?;
                    if reaches {
                        let state = std::mem::take(&mut self.state);
                        joined = Some(if let Some(previous) = joined {
                            self.merge(previous, &state, arm.span)?
                        } else {
                            state
                        });
                    }
                }
                let reaches = joined.is_some();
                self.state = joined.unwrap_or(base);
                return Ok(reaches);
            }
        }
        Ok(true)
    }
    fn pattern(&mut self, id: r::PatternId, depth: usize) -> Result<(), Error> {
        let pattern = &self.function.patterns[id.0];
        self.context.budget.enter(pattern.span, depth)?;
        match &pattern.kind {
            r::PatternKind::Binding(local) => self.declare(*local, pattern.span)?,
            r::PatternKind::Record { fields, .. } => {
                for field in fields {
                    self.pattern(field.pattern, depth + 1)?;
                }
            }
            r::PatternKind::Case { payload, .. } | r::PatternKind::CoreCase { payload, .. } => {
                for pattern in payload {
                    self.pattern(*pattern, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn eval(
        &mut self,
        id: r::ExpressionId,
        mode: Mode,
        depth: usize,
    ) -> Result<Option<r::LocalId>, Error> {
        let expression = &self.function.expressions[id.0];
        let span = expression.span;
        self.context.budget.enter(span, depth)?;
        let ty = self.types.expressions()[id.0];
        match &expression.kind {
            r::ExpressionKind::Local(local) => {
                let ready = self.ready(*local, span)?;
                let moves = matches!(mode, Mode::Value) && !self.copy_type(ty) && !self.loan(ty);
                if moves
                    && matches!(
                        self.constructor(ty),
                        Some(r::TypeConstructor::ArenaOwner(_))
                    )
                {
                    self.violation(Some(*local), ViolationKind::ArenaOwnerMove, span, None)?;
                } else if ready && moves {
                    self.state[local.0] = State::Moved(span);
                }
                self.context.budget.push(
                    span,
                    &mut self.accesses,
                    Access {
                        expression: id,
                        local: *local,
                        kind: if moves {
                            AccessKind::Move
                        } else {
                            AccessKind::Read
                        },
                    },
                )?;
                return Ok(Some(*local));
            }
            r::ExpressionKind::Borrow { place, .. } => {
                return self.eval(r::ExpressionId(place.0), Mode::Observe, depth + 1);
            }
            r::ExpressionKind::Field { receiver, .. } => {
                let root = self.eval(*receiver, Mode::Observe, depth + 1)?;
                if matches!(mode, Mode::Value) && !self.copy_type(ty) && !self.loan(ty) {
                    self.violation(root, ViolationKind::PartialMove, span, None)?;
                }
                return Ok(root);
            }
            r::ExpressionKind::Dereference(value) => {
                let root = self.eval(*value, Mode::Observe, depth + 1)?;
                if matches!(mode, Mode::Value) && !self.copy_type(ty) && !self.loan(ty) {
                    self.violation(root, ViolationKind::BorrowedMove, span, None)?;
                }
                return Ok(root);
            }
            r::ExpressionKind::Call { arguments, .. }
            | r::ExpressionKind::CoreCall { arguments, .. } => {
                let parameters = &self.types.call(id).unwrap().parameters;
                for (index, argument) in arguments.iter().enumerate() {
                    let mode = if self.loan(parameters[index]) {
                        Mode::Observe
                    } else {
                        Mode::Value
                    };
                    self.eval(*argument, mode, depth + 1)?;
                }
            }
            r::ExpressionKind::CoreCase { payload, .. } => {
                for value in payload {
                    self.eval(*value, Mode::Value, depth + 1)?;
                }
            }
            r::ExpressionKind::Record { fields, .. } => {
                for field in fields {
                    self.eval(field.value, Mode::Value, depth + 1)?;
                }
            }
            r::ExpressionKind::Add(left, right) | r::ExpressionKind::LessThan(left, right) => {
                self.eval(*left, Mode::Value, depth + 1)?;
                self.eval(*right, Mode::Value, depth + 1)?;
            }
            r::ExpressionKind::Integer(_)
            | r::ExpressionKind::Boolean(_)
            | r::ExpressionKind::Case { .. }
            | r::ExpressionKind::CoreUnit => {}
        }
        Ok(None)
    }
}
