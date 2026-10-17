//! Lexical whole-owner loans over move-checked source bodies.
//! Nested reference cells and value arena-brand flow remain separate gates;
//! these facts cannot construct an executable program.

use crate::{core, moves::MoveCheckedBodies, program::FunctionFacts, resolved as r};
use lang_diagnostics::{Diagnostic, Label, Severity, SourceError, SourceMap, Span, codes};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub functions: usize,
    pub locals: usize,
    pub loans: usize,
    pub depth: usize,
    pub work: usize,
    /// Cumulative requested buffers; excludes input, allocator overhead, and
    /// the fixed emergency error returned when a budget is exhausted.
    pub allocations: usize,
    pub diagnostics: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            functions: 4096,
            locals: 1_000_000,
            loans: 1_000_000,
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
        write!(f, "source loan analysis failed: {self:?}")
    }
}
impl StdError for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct LoanId(pub usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Owner {
    Local(r::LocalId),
    Parameter(r::LocalId),
    Temporary(r::ExpressionId),
    InstanceAllocator,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Loan {
    pub owner: Owner,
    pub kind: r::LoanKind,
    pub parent: Option<LoanId>,
    pub span: Span,
    /// Lexical nesting level, including temporary call/match frames. This is
    /// analysis metadata, not a runtime generation or a serialized region ID.
    pub scope: usize,
    owner_scope: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViolationKind {
    Conflict,
    Immutable,
    FrozenParent,
    Escape,
    LoopState,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Violation {
    pub function: r::FunctionId,
    pub kind: ViolationKind,
    pub owner: Option<Owner>,
    pub span: Span,
    pub related: Option<Span>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingKind {
    NestedReferenceCell,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pending {
    pub function: r::FunctionId,
    pub kind: PendingKind,
    pub span: Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionLoans {
    loans: Vec<Loan>,
}
impl FunctionLoans {
    pub fn loans(&self) -> &[Loan] {
        &self.loans
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanCheckedBodies {
    moved: MoveCheckedBodies,
    functions: Vec<FunctionLoans>,
    work: usize,
    allocations: usize,
}
impl LoanCheckedBodies {
    pub fn moved(&self) -> &MoveCheckedBodies {
        &self.moved
    }
    pub fn functions(&self) -> &[FunctionLoans] {
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
    pub bodies: Option<LoanCheckedBodies>,
    pub diagnostics: Vec<Diagnostic>,
    pub violations: Vec<Violation>,
    /// Unsupported provenance is a remaining analysis gate, never a claim of
    /// invalid source semantics and never successful checked-body evidence.
    pub pending: Vec<Pending>,
}

struct Budget {
    limits: Limits,
    work: usize,
    allocations: usize,
    loans: usize,
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
            .ok_or_else(|| self.limit(span, "loan analysis work limit exceeded"))?;
        Ok(())
    }
    fn enter(&mut self, span: Span, depth: usize) -> Result<(), Error> {
        self.charge(span, 1)?;
        if depth >= self.limits.depth.min(256) {
            return Err(self.limit(span, "loan analysis depth limit exceeded"));
        }
        Ok(())
    }
    fn allocate<T>(&mut self, span: Span, count: usize) -> Result<(), Error> {
        self.allocations = count
            .checked_mul(std::mem::size_of::<T>())
            .and_then(|n| self.allocations.checked_add(n))
            .filter(|n| *n <= self.limits.allocations)
            .ok_or_else(|| self.limit(span, "loan analysis allocation limit exceeded"))?;
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
                .ok_or_else(|| self.limit(span, "loan analysis buffer overflow"))?;
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
        let mut out = self.vector(span, values.len())?;
        out.extend_from_slice(values);
        Ok(out)
    }
    fn text(&mut self, span: Span, value: &str) -> Result<String, Error> {
        self.charge(span, value.len())?;
        self.allocate::<u8>(span, value.len())?;
        let mut out = String::new();
        out.try_reserve_exact(value.len())
            .map_err(|_| Error::AllocationFailed { span })?;
        out.push_str(value);
        Ok(out)
    }
    fn union(&mut self, span: Span, left: &mut Vec<LoanId>, right: &[LoanId]) -> Result<(), Error> {
        for id in right {
            self.charge(span, left.len() + 1)?;
            if !left.contains(id) {
                self.push(span, left, *id)?;
            }
        }
        // IDs are created monotonically. Sorting makes state equality stable
        // independently of branch visitation and charges its upper bound.
        self.charge(
            span,
            left.len()
                .saturating_mul(left.len().checked_ilog2().unwrap_or(0) as usize + 1),
        )?;
        left.sort_unstable();
        Ok(())
    }
}

#[derive(Default)]
struct State {
    references: Vec<Vec<LoanId>>,
    active: Vec<LoanId>,
}
impl State {
    fn copy(&self, budget: &mut Budget, span: Span) -> Result<Self, Error> {
        budget.charge(span, self.references.len())?;
        let mut references = budget.vector(span, self.references.len())?;
        for value in &self.references {
            references.push(budget.copy(span, value)?);
        }
        Ok(Self {
            references,
            active: budget.copy(span, &self.active)?,
        })
    }
    fn merge(mut self, other: &Self, budget: &mut Budget, span: Span) -> Result<Self, Error> {
        budget.charge(span, self.references.len())?;
        budget.union(span, &mut self.active, &other.active)?;
        for (a, b) in self.references.iter_mut().zip(&other.references) {
            budget.union(span, a, b)?;
        }
        Ok(self)
    }
}
#[derive(Clone, Copy)]
struct Place {
    owner: Owner,
    via: Option<LoanId>,
    mutable: bool,
    scope: usize,
}
struct Value {
    place: Option<Place>,
    references: Vec<LoanId>,
    provisional: bool,
}
impl Value {
    fn plain(place: Place) -> Self {
        Self {
            place: Some(place),
            references: Vec::new(),
            provisional: false,
        }
    }
    fn references(references: Vec<LoanId>) -> Self {
        Self {
            place: None,
            references,
            provisional: false,
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Access {
    Read,
    Write,
    Move,
}

struct Context {
    budget: Budget,
    diagnostics: Vec<Diagnostic>,
    violations: Vec<Violation>,
    pending: Vec<Pending>,
}
impl Context {
    fn report(&mut self, violation: Violation) -> Result<(), Error> {
        let span = violation.span;
        if self.violations.len() >= self.budget.limits.diagnostics {
            return Err(self.budget.limit(span, "loan diagnostic limit exceeded"));
        }
        let message = match violation.kind {
            ViolationKind::Conflict => "access conflicts with a live lexical loan",
            ViolationKind::Immutable => {
                "exclusive access requires a mutable place or exclusive loan"
            }
            ViolationKind::FrozenParent => "parent loan is frozen by a live reborrow",
            ViolationKind::Escape => "loan outlives its owner or parent loan",
            ViolationKind::LoopState => "loop back-edge must restore the header loan state",
        };
        let code = if violation.kind == ViolationKind::Escape {
            codes::REGION_ESCAPE
        } else {
            codes::INVALID_LOAN
        };
        let text = self.budget.text(span, message)?;
        let label = Label::new(span, self.budget.text(span, message)?);
        let mut diagnostic = Diagnostic::new(code, Severity::Error, text, label);
        if let Some(origin) = violation.related {
            let label = Label::new(
                origin,
                self.budget
                    .text(origin, "contributing loan or owner declared here")?,
            );
            self.budget.push(span, &mut diagnostic.secondary, label)?;
        }
        self.budget.push(span, &mut self.diagnostics, diagnostic)?;
        self.budget.push(span, &mut self.violations, violation)
    }
}

pub fn analyze(
    moved: MoveCheckedBodies,
    sources: &SourceMap,
    limits: Limits,
) -> Result<Analysis, Error> {
    let bodies = moved.bodies();
    let root = bodies.source().modules[0].span;
    for module in &bodies.source().modules {
        sources.slice(module.span).map_err(Error::Source)?;
    }
    let mut context = Context {
        budget: Budget {
            limits,
            work: 0,
            allocations: 0,
            loans: 0,
        },
        diagnostics: Vec::new(),
        violations: Vec::new(),
        pending: Vec::new(),
    };
    if bodies.source().functions.len() > limits.functions {
        return Err(context.budget.limit(root, "loan function limit exceeded"));
    }
    let mut functions = context
        .budget
        .vector(root, bodies.source().functions.len())?;
    let mut locals = 0usize;
    for (index, function) in bodies.source().functions.iter().enumerate() {
        locals = locals
            .checked_add(function.locals.len())
            .filter(|n| *n <= limits.locals)
            .ok_or_else(|| {
                context
                    .budget
                    .limit(function.span, "loan local limit exceeded")
            })?;
        let facts = &bodies.functions()[index];
        let mut nested = false;
        for ty in facts.locals().iter().chain(facts.expressions()) {
            context.budget.charge(function.span, 1)?;
            if let r::Type::Constructed(id) = ty
                && let r::TypeConstructor::Loan(_, target) = bodies.types()[id.0].constructor
                && bodies.properties(target).unwrap().contains_loan
            {
                nested = true;
            }
        }
        if nested {
            context.budget.push(
                function.span,
                &mut context.pending,
                Pending {
                    function: r::FunctionId(index),
                    kind: PendingKind::NestedReferenceCell,
                    span: function.span,
                },
            )?;
            functions.push(FunctionLoans { loans: Vec::new() });
            continue;
        }
        let mut references = context
            .budget
            .vector(function.span, function.locals.len())?;
        references.resize_with(function.locals.len(), Vec::new);
        let mut depths = context
            .budget
            .vector(function.span, function.locals.len())?;
        depths.resize(function.locals.len(), 0);
        let mut checker = Checker {
            context: &mut context,
            bodies,
            function,
            facts,
            id: r::FunctionId(index),
            loans: Vec::new(),
            state: State {
                references,
                active: Vec::new(),
            },
            depths,
            scopes: Vec::new(),
        };
        for local in &function.parameters {
            if let Some(kind) = checker.loan(facts.locals()[local.0]) {
                let span = function.locals[local.0].name.span;
                let id = checker.create(
                    Place {
                        owner: Owner::Parameter(*local),
                        via: None,
                        mutable: kind == r::LoanKind::Exclusive,
                        scope: 0,
                    },
                    kind,
                    0,
                    span,
                )?;
                checker
                    .context
                    .budget
                    .push(span, &mut checker.state.references[local.0], id)?;
            }
        }
        checker.block(function.body, 0)?;
        functions.push(FunctionLoans {
            loans: checker.loans,
        });
    }
    let work = context.budget.work;
    let allocations = context.budget.allocations;
    let successful = context.violations.is_empty() && context.pending.is_empty();
    Ok(Analysis {
        bodies: successful.then_some(LoanCheckedBodies {
            moved,
            functions,
            work,
            allocations,
        }),
        diagnostics: context.diagnostics,
        violations: context.violations,
        pending: context.pending,
    })
}

struct Checker<'a, 'b> {
    context: &'b mut Context,
    bodies: &'a crate::program::BodyTypes,
    function: &'a r::Function,
    facts: &'a FunctionFacts,
    id: r::FunctionId,
    loans: Vec<Loan>,
    state: State,
    depths: Vec<usize>,
    scopes: Vec<Vec<r::LocalId>>,
}
impl Checker<'_, '_> {
    fn loan(&self, ty: r::Type) -> Option<r::LoanKind> {
        if let r::Type::Constructed(id) = ty
            && let r::TypeConstructor::Loan(kind, _) = self.bodies.types()[id.0].constructor
        {
            Some(kind)
        } else {
            None
        }
    }
    fn report(
        &mut self,
        kind: ViolationKind,
        owner: Option<Owner>,
        span: Span,
        related: Option<Span>,
    ) -> Result<(), Error> {
        self.context.report(Violation {
            function: self.id,
            kind,
            owner,
            span,
            related,
        })
    }
    fn begin(&mut self, span: Span) -> Result<(), Error> {
        self.context.budget.enter(span, self.scopes.len())?;
        self.context.budget.push(span, &mut self.scopes, Vec::new())
    }
    fn end(&mut self, span: Span) -> Result<(), Error> {
        let depth = self.scopes.len();
        self.context.budget.charge(span, self.state.active.len())?;
        self.state
            .active
            .retain(|id| self.loans[id.0].scope < depth);
        for local in self.scopes.pop().unwrap() {
            self.context.budget.charge(span, 1)?;
            self.state.references[local.0].clear();
        }
        Ok(())
    }
    fn declare(&mut self, local: r::LocalId, value: Vec<LoanId>, span: Span) -> Result<(), Error> {
        self.depths[local.0] = self.scopes.len();
        self.state.references[local.0] = value;
        self.context
            .budget
            .push(span, self.scopes.last_mut().unwrap(), local)
    }
    fn ancestor(&mut self, ancestor: LoanId, mut child: LoanId, span: Span) -> Result<bool, Error> {
        loop {
            self.context.budget.charge(span, 1)?;
            if ancestor == child {
                return Ok(true);
            }
            if let Some(parent) = self.loans[child.0].parent {
                child = parent;
            } else {
                return Ok(false);
            }
        }
    }
    fn access(&mut self, place: Place, access: Access, span: Span) -> Result<bool, Error> {
        if access == Access::Write && !place.mutable {
            let related = match place.owner {
                Owner::Local(id) | Owner::Parameter(id) => {
                    Some(self.function.locals[id.0].name.span)
                }
                _ => None,
            };
            self.report(ViolationKind::Immutable, Some(place.owner), span, related)?;
            return Ok(false);
        }
        let mut valid = true;
        for index in 0..self.state.active.len() {
            self.context.budget.charge(span, 1)?;
            let id = self.state.active[index];
            let loan = self.loans[id.0];
            if loan.owner != place.owner {
                continue;
            }
            if let Some(via) = place.via
                && self.ancestor(id, via, span)?
            {
                continue;
            }
            if access == Access::Read && loan.kind == r::LoanKind::Shared {
                continue;
            }
            let kind = if let Some(via) = place.via
                && self.ancestor(via, id, span)?
            {
                ViolationKind::FrozenParent
            } else {
                ViolationKind::Conflict
            };
            self.report(kind, Some(place.owner), span, Some(loan.span))?;
            valid = false;
        }
        Ok(valid)
    }
    fn create(
        &mut self,
        place: Place,
        kind: r::LoanKind,
        scope: usize,
        span: Span,
    ) -> Result<LoanId, Error> {
        self.context.budget.loans = self
            .context
            .budget
            .loans
            .checked_add(1)
            .filter(|n| *n <= self.context.budget.limits.loans)
            .ok_or_else(|| self.context.budget.limit(span, "loan count limit exceeded"))?;
        let id = LoanId(self.loans.len());
        self.context.budget.push(
            span,
            &mut self.loans,
            Loan {
                owner: place.owner,
                kind,
                parent: place.via,
                span,
                scope,
                owner_scope: place.scope,
            },
        )?;
        self.context.budget.push(span, &mut self.state.active, id)?;
        Ok(id)
    }
    fn borrow(
        &mut self,
        place: Place,
        kind: r::LoanKind,
        scope: usize,
        span: Span,
    ) -> Result<Option<LoanId>, Error> {
        let parent = place.via.map(|id| self.loans[id.0]);
        if place.scope > scope || parent.is_some_and(|p| p.scope > scope) {
            let related = parent.map(|p| p.span).or_else(|| match place.owner {
                Owner::Local(id) => Some(self.function.locals[id.0].name.span),
                _ => None,
            });
            self.report(ViolationKind::Escape, Some(place.owner), span, related)?;
            return Ok(None);
        }
        let access = if kind == r::LoanKind::Shared {
            Access::Read
        } else {
            Access::Write
        };
        if self.access(place, access, span)? {
            self.create(place, kind, scope, span).map(Some)
        } else {
            Ok(None)
        }
    }
    fn loan_place(&self, id: LoanId) -> Place {
        let loan = self.loans[id.0];
        Place {
            owner: loan.owner,
            via: Some(id),
            mutable: loan.kind == r::LoanKind::Exclusive,
            scope: loan.owner_scope,
        }
    }
    fn singleton(&mut self, place: Place, span: Span) -> Result<Vec<Place>, Error> {
        let mut values = self.context.budget.vector(span, 1)?;
        values.push(place);
        Ok(values)
    }
    fn places(&mut self, id: r::ExpressionId, depth: usize) -> Result<Vec<Place>, Error> {
        let expression = &self.function.expressions[id.0];
        let span = expression.span;
        self.context.budget.enter(span, depth)?;
        match &expression.kind {
            r::ExpressionKind::Local(local) => self.singleton(
                Place {
                    owner: Owner::Local(*local),
                    via: None,
                    mutable: matches!(
                        self.function.locals[local.0].kind,
                        r::BindingKind::Mutable | r::BindingKind::ArenaOwner
                    ),
                    scope: self.depths[local.0],
                },
                span,
            ),
            r::ExpressionKind::Dereference(value) => self.referents(*value, depth + 1),
            r::ExpressionKind::Field { receiver, .. } => {
                if self.loan(self.facts.expressions()[receiver.0]).is_some() {
                    self.referents(*receiver, depth + 1)
                } else {
                    self.places(*receiver, depth + 1)
                }
            }
            _ => {
                let value = self.eval(id, self.scopes.len(), depth + 1)?;
                self.singleton(value.place.unwrap(), span)
            }
        }
    }
    fn referents(&mut self, id: r::ExpressionId, depth: usize) -> Result<Vec<Place>, Error> {
        let span = self.function.expressions[id.0].span;
        let refs = if let r::ExpressionKind::Local(local) = self.function.expressions[id.0].kind {
            self.context
                .budget
                .copy(span, &self.state.references[local.0])?
        } else {
            self.eval(id, self.scopes.len(), depth + 1)?.references
        };
        let mut places = self.context.budget.vector(span, refs.len())?;
        for id in refs {
            self.context.budget.charge(span, 1)?;
            places.push(self.loan_place(id));
        }
        Ok(places)
    }
    fn reference(
        &mut self,
        id: r::ExpressionId,
        kind: r::LoanKind,
        scope: usize,
        depth: usize,
    ) -> Result<Vec<LoanId>, Error> {
        let expression = &self.function.expressions[id.0];
        let span = expression.span;
        self.context.budget.enter(span, depth)?;
        if let r::ExpressionKind::Borrow {
            kind: explicit,
            place,
        } = expression.kind
        {
            let mut out = Vec::new();
            for place in self.places(r::ExpressionId(place.0), depth + 1)? {
                if let Some(id) = self.borrow(place, explicit, scope, span)? {
                    self.context.budget.push(span, &mut out, id)?;
                }
            }
            return Ok(out);
        }
        let places = if self.loan(self.facts.expressions()[id.0]).is_some() {
            self.referents(id, depth + 1)?
        } else {
            self.places(id, depth + 1)?
        };
        let mut out = Vec::new();
        for place in places {
            if let Some(id) = self.borrow(place, kind, scope, span)? {
                self.context.budget.push(span, &mut out, id)?;
            }
        }
        Ok(out)
    }
    fn eval(
        &mut self,
        id: r::ExpressionId,
        result_scope: usize,
        depth: usize,
    ) -> Result<Value, Error> {
        let expression = &self.function.expressions[id.0];
        let span = expression.span;
        self.context.budget.enter(span, depth)?;
        let ty = self.facts.expressions()[id.0];
        if let Some(kind) = self.loan(ty)
            && !matches!(expression.kind, r::ExpressionKind::CoreCall { .. })
        {
            return self
                .reference(id, kind, result_scope, depth + 1)
                .map(Value::references);
        }
        match &expression.kind {
            r::ExpressionKind::Local(_)
            | r::ExpressionKind::Field { .. }
            | r::ExpressionKind::Dereference(_) => {
                let access = if self.bodies.properties(ty).unwrap().is_copy {
                    Access::Read
                } else {
                    Access::Move
                };
                let places = self.places(id, depth + 1)?;
                for place in &places {
                    self.access(*place, access, span)?;
                }
                return Ok(Value::plain(places.first().copied().unwrap_or(Place {
                    owner: Owner::Temporary(id),
                    via: None,
                    mutable: false,
                    scope: result_scope,
                })));
            }
            r::ExpressionKind::Call { arguments, .. }
            | r::ExpressionKind::CoreCall { arguments, .. } => {
                let call = self.facts.call(id).unwrap();
                self.begin(span)?;
                let call_scope = self.scopes.len();
                let mut returned = Vec::new();
                for (index, argument) in arguments.iter().enumerate() {
                    let value = if let Some(kind) = self.loan(call.parameters[index]) {
                        self.reference(*argument, kind, call_scope, depth + 1)?
                    } else {
                        self.eval(*argument, call_scope, depth + 1)?.references
                    };
                    if call.result_binding
                        == (core::ResultBinding::ProvisionalLoan { parameter: index })
                    {
                        for id in value {
                            let mut place = self.loan_place(id);
                            while let Some(via) = place.via {
                                self.context.budget.charge(span, 1)?;
                                let loan = self.loans[via.0];
                                if loan.scope <= result_scope {
                                    break;
                                }
                                place.via = loan.parent;
                            }
                            self.context.budget.push(span, &mut returned, place)?;
                        }
                    }
                }
                self.end(span)?;
                match call.result_binding {
                    core::ResultBinding::ProvisionalLoan { .. } => {
                        let mut refs = Vec::new();
                        for place in returned {
                            if let Some(id) =
                                self.borrow(place, r::LoanKind::Shared, result_scope, span)?
                            {
                                self.context.budget.push(span, &mut refs, id)?;
                            }
                        }
                        return Ok(Value {
                            place: None,
                            references: refs,
                            provisional: true,
                        });
                    }
                    core::ResultBinding::InstanceAllocator => {
                        let place = Place {
                            owner: Owner::InstanceAllocator,
                            via: None,
                            mutable: false,
                            scope: 0,
                        };
                        let mut refs = Vec::new();
                        if let Some(id) =
                            self.borrow(place, r::LoanKind::Shared, result_scope, span)?
                        {
                            self.context.budget.push(span, &mut refs, id)?;
                        }
                        return Ok(Value::references(refs));
                    }
                    _ => {}
                }
            }
            r::ExpressionKind::CoreCase { payload, .. } => {
                for id in payload {
                    self.eval(*id, result_scope, depth + 1)?;
                }
            }
            r::ExpressionKind::Record { fields, .. } => {
                for field in fields {
                    self.eval(field.value, result_scope, depth + 1)?;
                }
            }
            r::ExpressionKind::Add(a, b) | r::ExpressionKind::LessThan(a, b) => {
                self.eval(*a, result_scope, depth + 1)?;
                self.eval(*b, result_scope, depth + 1)?;
            }
            r::ExpressionKind::Integer(_)
            | r::ExpressionKind::Boolean(_)
            | r::ExpressionKind::Case { .. }
            | r::ExpressionKind::CoreUnit => {}
            r::ExpressionKind::Borrow { .. } => unreachable!("typed borrow has a loan type"),
        }
        Ok(Value::plain(Place {
            owner: Owner::Temporary(id),
            via: None,
            mutable: false,
            scope: result_scope,
        }))
    }
    fn block(&mut self, id: r::BlockId, depth: usize) -> Result<bool, Error> {
        let block = &self.function.blocks[id.0];
        self.context.budget.enter(block.span, depth)?;
        self.begin(block.span)?;
        let mut reaches = true;
        for statement in &block.statements {
            if !reaches {
                break;
            }
            reaches = self.statement(statement, depth + 1)?;
        }
        self.end(block.span)?;
        Ok(reaches)
    }
    fn temporary(&mut self, id: r::ExpressionId, depth: usize) -> Result<Value, Error> {
        let span = self.function.expressions[id.0].span;
        self.begin(span)?;
        let value = self.eval(id, self.scopes.len(), depth + 1)?;
        self.end(span)?;
        Ok(value)
    }
    fn statement(&mut self, statement: &r::Statement, depth: usize) -> Result<bool, Error> {
        let span = statement.span;
        self.context.budget.enter(span, depth)?;
        match &statement.kind {
            r::StatementKind::LetMut { local, initializer } => {
                let refs = if let Some(kind) = self.loan(self.facts.locals()[local.0]) {
                    self.reference(*initializer, kind, self.scopes.len(), depth + 1)?
                } else {
                    self.temporary(*initializer, depth + 1)?.references
                };
                self.declare(*local, refs, span)?;
            }
            r::StatementKind::Assign { local, value } => {
                let refs = if let Some(kind) = self.loan(self.facts.locals()[local.0]) {
                    // Rebinding a reference cell does not access its old
                    // referent. Earlier lexical loans/reborrows stay live.
                    self.reference(*value, kind, self.depths[local.0], depth + 1)?
                } else {
                    self.temporary(*value, depth + 1)?.references
                };
                let place = Place {
                    owner: Owner::Local(*local),
                    via: None,
                    mutable: self.function.locals[local.0].kind == r::BindingKind::Mutable,
                    scope: self.depths[local.0],
                };
                self.access(place, Access::Write, span)?;
                self.state.references[local.0] = refs;
            }
            r::StatementKind::PlaceAssign { place, value } => {
                self.begin(span)?;
                let places = self.places(r::ExpressionId(place.0), depth + 1)?;
                self.eval(*value, self.scopes.len(), depth + 1)?;
                for place in places {
                    self.access(place, Access::Write, span)?;
                }
                self.end(span)?;
            }
            r::StatementKind::Return(value) => {
                self.temporary(*value, depth + 1)?;
                return Ok(false);
            }
            r::StatementKind::Arena {
                owner,
                allocator,
                body,
            } => {
                self.begin(span)?;
                self.reference(
                    *allocator,
                    r::LoanKind::Shared,
                    self.scopes.len(),
                    depth + 1,
                )?;
                self.declare(*owner, Vec::new(), span)?;
                let reaches = self.block(*body, depth + 1)?;
                self.end(span)?;
                return Ok(reaches);
            }
            r::StatementKind::If {
                condition,
                then_block,
                else_block,
            } => {
                self.temporary(*condition, depth + 1)?;
                let base = std::mem::take(&mut self.state);
                self.state = base.copy(&mut self.context.budget, span)?;
                let yes = self.block(*then_block, depth + 1)?;
                let yes_state = std::mem::take(&mut self.state);
                self.state = base.copy(&mut self.context.budget, span)?;
                let no = if let Some(block) = else_block {
                    self.block(*block, depth + 1)?
                } else {
                    true
                };
                let no_state = std::mem::take(&mut self.state);
                self.state = match (yes, no) {
                    (true, true) => yes_state.merge(&no_state, &mut self.context.budget, span)?,
                    (true, false) => yes_state,
                    (false, true) => no_state,
                    (false, false) => base,
                };
                return Ok(yes || no);
            }
            r::StatementKind::While { condition, body } => {
                let header = self.state.copy(&mut self.context.budget, span)?;
                self.temporary(*condition, depth + 1)?;
                let exit = self.state.copy(&mut self.context.budget, span)?;
                if self.block(*body, depth + 1)? {
                    self.context
                        .budget
                        .charge(span, self.state.active.len() + self.state.references.len())?;
                    for (a, b) in self.state.references.iter().zip(&header.references) {
                        self.context
                            .budget
                            .charge(span, a.len().saturating_add(b.len()))?;
                    }
                    if self.state.active != header.active
                        || self.state.references != header.references
                    {
                        self.report(ViolationKind::LoopState, None, span, None)?;
                    }
                }
                self.state = exit;
            }
            r::StatementKind::Match { scrutinee, arms } => {
                self.begin(span)?;
                let ty = self.facts.expressions()[scrutinee.0];
                let value = if let Some(kind) = self.loan(ty) {
                    Value::references(self.reference(
                        *scrutinee,
                        kind,
                        self.scopes.len(),
                        depth + 1,
                    )?)
                } else if !self.bodies.properties(ty).unwrap().is_copy && self.is_place(*scrutinee)
                {
                    Value::references(self.reference(
                        *scrutinee,
                        r::LoanKind::Shared,
                        self.scopes.len(),
                        depth + 1,
                    )?)
                } else {
                    let mut value = self.eval(*scrutinee, self.scopes.len(), depth + 1)?;
                    if !self.bodies.properties(ty).unwrap().is_copy
                        && let Some(place) = value.place
                        && let Some(id) =
                            self.borrow(place, r::LoanKind::Shared, self.scopes.len(), span)?
                    {
                        self.context.budget.push(span, &mut value.references, id)?;
                    }
                    value
                };
                let base = std::mem::take(&mut self.state);
                let mut joined = None;
                for arm in arms {
                    self.context.budget.charge(arm.span, 1)?;
                    if self.facts.arm_is_unreachable(arm.pattern) {
                        continue;
                    }
                    self.state = base.copy(&mut self.context.budget, arm.span)?;
                    if value.provisional && !self.loan_binding(arm.pattern, depth + 1)? {
                        self.context.budget.charge(
                            arm.span,
                            self.state
                                .active
                                .len()
                                .saturating_mul(value.references.len() + 1),
                        )?;
                        self.state
                            .active
                            .retain(|id| !value.references.contains(id));
                    }
                    self.begin(arm.span)?;
                    self.pattern(arm.pattern, &value.references, depth + 1)?;
                    let reaches = self.block(arm.body, depth + 1)?;
                    self.end(arm.span)?;
                    if reaches {
                        let state = std::mem::take(&mut self.state);
                        joined = Some(if let Some(previous) = joined {
                            state.merge(&previous, &mut self.context.budget, arm.span)?
                        } else {
                            state
                        });
                    }
                }
                let reaches = joined.is_some();
                self.state = joined.unwrap_or(base);
                self.end(span)?;
                return Ok(reaches);
            }
        }
        Ok(true)
    }
    fn is_place(&self, id: r::ExpressionId) -> bool {
        matches!(
            self.function.expressions[id.0].kind,
            r::ExpressionKind::Local(_)
                | r::ExpressionKind::Field { .. }
                | r::ExpressionKind::Dereference(_)
        )
    }
    fn loan_binding(&mut self, id: r::PatternId, depth: usize) -> Result<bool, Error> {
        let pattern = &self.function.patterns[id.0];
        self.context.budget.enter(pattern.span, depth)?;
        match &pattern.kind {
            r::PatternKind::Binding(local) => Ok(self.loan(self.facts.locals()[local.0]).is_some()),
            r::PatternKind::Record { fields, .. } => {
                for field in fields {
                    if self.loan_binding(field.pattern, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            r::PatternKind::Case { payload, .. } | r::PatternKind::CoreCase { payload, .. } => {
                for id in payload {
                    if self.loan_binding(*id, depth + 1)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => Ok(false),
        }
    }
    fn pattern(&mut self, id: r::PatternId, source: &[LoanId], depth: usize) -> Result<(), Error> {
        let pattern = &self.function.patterns[id.0];
        let span = pattern.span;
        self.context.budget.enter(span, depth)?;
        match &pattern.kind {
            r::PatternKind::Binding(local) => {
                let mut refs = Vec::new();
                if let Some(kind) = self.loan(self.facts.locals()[local.0]) {
                    for id in source {
                        if let Some(id) =
                            self.borrow(self.loan_place(*id), kind, self.scopes.len(), span)?
                        {
                            self.context.budget.push(span, &mut refs, id)?;
                        }
                    }
                }
                self.declare(*local, refs, span)?;
            }
            r::PatternKind::Record { fields, .. } => {
                for field in fields {
                    self.pattern(field.pattern, source, depth + 1)?;
                }
            }
            r::PatternKind::Case { payload, .. } | r::PatternKind::CoreCase { payload, .. } => {
                for id in payload {
                    self.pattern(*id, source, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
