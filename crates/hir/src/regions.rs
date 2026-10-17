//! Arena-brand dataflow over loan-checked source bodies. Guest-call final
//! values and transient store requirements use separate bounded fixed points.
//! No result grants execution authority.

mod summary;

use crate::{
    core,
    loans::LoanCheckedBodies,
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
    pub values: usize,
    pub places: usize,
    pub depth: usize,
    pub work: usize,
    pub allocations: usize,
    pub diagnostics: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            functions: 4096,
            locals: 1_000_000,
            values: 1_000_000,
            places: 1_000_000,
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
        write!(f, "source region analysis failed: {self:?}")
    }
}
impl StdError for Error {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct BrandId(pub usize);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrandKind {
    Local(r::ArenaOwnerId),
    /// Opaque caller provenance, potentially containing multiple runtime
    /// regions. It never authorizes access through a new local arena owner.
    Caller {
        function: r::FunctionId,
        parameter: r::LocalId,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Brand {
    pub kind: BrandKind,
    pub span: Span,
    pub scope: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViolationKind {
    Escape,
    CrossArena,
    SharedStorage,
    ReturnEscape,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Violation {
    pub function: r::FunctionId,
    pub kind: ViolationKind,
    pub brand: Brand,
    pub span: Span,
    pub related: Option<Span>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Mutation {
    pub parameter: r::LocalId,
    pub dereferences: usize,
    pub span: Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallerOrigin {
    pub parameter: r::LocalId,
    pub dereferences: usize,
    path: Vec<Projection>,
}
impl CallerOrigin {
    pub fn projections(&self) -> &[Projection] {
        &self.path
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionRegions {
    brands: Vec<Brand>,
    expressions: Vec<Option<Vec<BrandId>>>,
    origins: Vec<Option<CallerOrigin>>,
    mutations: Vec<Mutation>,
}
impl FunctionRegions {
    pub fn brands(&self) -> &[Brand] {
        &self.brands
    }
    /// Conservative union over every analysis visit, including loop edges.
    /// Unreachable expressions have no facts. IDs are local to this function.
    pub fn expression_brands(&self, id: r::ExpressionId) -> Option<&[BrandId]> {
        self.expressions.get(id.0).and_then(|v| v.as_deref())
    }
    pub fn caller_origin(&self, id: BrandId) -> Option<&CallerOrigin> {
        self.origins.get(id.0).and_then(Option::as_ref)
    }
    pub fn mutations(&self) -> &[Mutation] {
        &self.mutations
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionCheckedBodies {
    loaned: LoanCheckedBodies,
    functions: Vec<FunctionRegions>,
    work: usize,
    allocations: usize,
}
impl RegionCheckedBodies {
    pub fn loaned(&self) -> &LoanCheckedBodies {
        &self.loaned
    }
    pub fn functions(&self) -> &[FunctionRegions] {
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
    pub bodies: Option<RegionCheckedBodies>,
    pub diagnostics: Vec<Diagnostic>,
    pub violations: Vec<Violation>,
}

struct Budget {
    limits: Limits,
    work: usize,
    allocations: usize,
    values: usize,
    places: usize,
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
            .ok_or_else(|| self.limit(span, "region analysis work limit exceeded"))?;
        Ok(())
    }
    fn enter(&mut self, span: Span, depth: usize) -> Result<(), Error> {
        self.charge(span, 1)?;
        if depth >= self.limits.depth.min(256) {
            return Err(self.limit(span, "region analysis depth limit exceeded"));
        }
        Ok(())
    }
    fn vector<T>(&mut self, span: Span, count: usize) -> Result<Vec<T>, Error> {
        self.allocations = count
            .checked_mul(std::mem::size_of::<T>())
            .and_then(|n| self.allocations.checked_add(n))
            .filter(|n| *n <= self.limits.allocations)
            .ok_or_else(|| self.limit(span, "region analysis allocation limit exceeded"))?;
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
                .ok_or_else(|| self.limit(span, "region buffer overflow"))?;
            // Charge the requested replacement buffer, not just net growth.
            self.allocations = next
                .checked_mul(std::mem::size_of::<T>())
                .and_then(|n| self.allocations.checked_add(n))
                .filter(|n| *n <= self.limits.allocations)
                .ok_or_else(|| self.limit(span, "region analysis allocation limit exceeded"))?;
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
    fn union<T: Copy + Ord>(
        &mut self,
        span: Span,
        left: &mut Vec<T>,
        right: &[T],
    ) -> Result<(), Error> {
        for value in right {
            self.charge(span, left.len() + 1)?;
            if !left.contains(value) {
                self.push(span, left, *value)?;
            }
        }
        self.charge(
            span,
            left.len()
                .saturating_mul(left.len().checked_ilog2().unwrap_or(0) as usize + 1),
        )?;
        left.sort_unstable();
        Ok(())
    }
    fn text(&mut self, span: Span, text: &str) -> Result<String, Error> {
        let bytes = self.copy(span, text.as_bytes())?;
        // The bytes came directly from a UTF-8 str, with no transformation.
        Ok(String::from_utf8(bytes).expect("copied UTF-8"))
    }
}
struct Context {
    budget: Budget,
    diagnostics: Vec<Diagnostic>,
    violations: Vec<Violation>,
    reporting: bool,
}
impl Context {
    fn report(&mut self, v: Violation) -> Result<(), Error> {
        self.budget.charge(v.span, 1)?;
        if !self.reporting {
            return Ok(());
        }
        // Repeated fixed-point visits must not duplicate the same violation.
        self.budget.charge(v.span, self.violations.len())?;
        if self.violations.contains(&v) {
            return Ok(());
        }
        if self.violations.len() >= self.budget.limits.diagnostics {
            return Err(self
                .budget
                .limit(v.span, "region diagnostic limit exceeded"));
        }
        let message = match v.kind {
            ViolationKind::Escape => "arena-bearing value outlives its region",
            ViolationKind::CrossArena => "value does not belong to this lexical arena",
            ViolationKind::SharedStorage => {
                "arena-bearing values cannot enter shared or weak storage"
            }
            ViolationKind::ReturnEscape => "arena-bearing values cannot escape as function results",
        };
        let code = if v.kind == ViolationKind::SharedStorage {
            codes::INVALID_OWNERSHIP_TYPE
        } else {
            codes::REGION_ESCAPE
        };
        let message_text = self.budget.text(v.span, message)?;
        let label = Label::new(v.span, self.budget.text(v.span, message)?);
        let mut d = Diagnostic::new(code, Severity::Error, message_text, label);
        let related = Label::new(
            v.brand.span,
            self.budget
                .text(v.brand.span, "arena or caller provenance introduced here")?,
        );
        self.budget.push(v.span, &mut d.secondary, related)?;
        if let Some(span) = v.related {
            let related = Label::new(
                span,
                self.budget
                    .text(span, "callee mutation contributing to this store")?,
            );
            self.budget.push(v.span, &mut d.secondary, related)?;
        }
        self.budget.push(v.span, &mut self.diagnostics, d)?;
        self.budget.push(v.span, &mut self.violations, v)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct ValueId(usize);
const EMPTY: ValueId = ValueId(0);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SumCase {
    Core(core::Case),
    User(r::DefinitionId, usize),
}
type Tag = SumCase;
#[derive(Debug, Eq, PartialEq)]
enum Value {
    Empty,
    Brands(Vec<BrandId>),
    Record(r::DefinitionId, Vec<ValueId>),
    Case(Tag, Vec<ValueId>),
    Cases(Vec<ValueId>),
    References(Vec<usize>),
    Authority(BrandId),
}
impl Value {
    fn cost(&self) -> usize {
        1 + match self {
            Self::Brands(v) => v.len(),
            Self::Record(_, v) | Self::Case(_, v) | Self::Cases(v) => v.len(),
            Self::References(v) => v.len(),
            _ => 0,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionKind {
    Field(usize),
    Payload(Tag, usize),
    Element,
}
type StepKind = ProjectionKind;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Projection {
    pub kind: ProjectionKind,
    pub ty: r::Type,
}
type Step = Projection;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Path {
    parent: Option<usize>,
    step: Step,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Place {
    root: usize,
    path: Option<usize>,
}
#[derive(Clone, Copy)]
struct Root {
    ty: r::Type,
    scope: usize,
}
struct Scope {
    locals: Vec<usize>,
}

pub fn analyze(
    loaned: LoanCheckedBodies,
    sources: &SourceMap,
    limits: Limits,
) -> Result<Analysis, Error> {
    let bodies = loaned.moved().bodies();
    let root = bodies.source().modules[0].span;
    for module in &bodies.source().modules {
        sources.slice(module.span).map_err(Error::Source)?;
    }
    let mut context = Context {
        budget: Budget {
            limits,
            work: 0,
            allocations: 0,
            values: 0,
            places: 0,
        },
        diagnostics: Vec::new(),
        violations: Vec::new(),
        reporting: false,
    };
    if bodies.source().functions.len() > limits.functions {
        return Err(context.budget.limit(root, "region function limit exceeded"));
    }
    let mut total_locals = 0usize;
    for function in &bodies.source().functions {
        total_locals = total_locals
            .checked_add(function.locals.len())
            .filter(|n| *n <= limits.locals)
            .ok_or_else(|| {
                context
                    .budget
                    .limit(function.span, "region local limit exceeded")
            })?;
    }
    let count = bodies.source().functions.len();
    let mut summaries = context.budget.vector(root, count)?;
    summaries.resize_with(count, summary::Summary::bottom);
    // Normal-return outputs first, then all transient stores. Mixing these
    // domains would lose stores followed by a clear or invent recursive ones.
    for requirements in [false, true] {
        loop {
            context.budget.charge(root, 1)?;
            let mut next = context.budget.vector(root, count)?;
            let mut stable = true;
            for index in 0..count {
                let function = &bodies.source().functions[index];
                let mut checker =
                    new_checker(&mut context, bodies, index, &summaries, requirements);
                checker.initialize()?;
                checker.block(function.body, 0)?;
                let summary = checker.summarize()?;
                checker.context.budget.charge(
                    function.span,
                    summary.cost().saturating_add(summaries[index].cost()),
                )?;
                stable &= summary == summaries[index];
                next.push(summary);
            }
            summaries = next;
            if stable {
                break;
            }
        }
    }
    context.reporting = true;
    let mut functions = context.budget.vector(root, count)?;
    for index in 0..count {
        let function = &bodies.source().functions[index];
        let mut checker = new_checker(&mut context, bodies, index, &summaries, true);
        checker.initialize()?;
        checker.block(function.body, 0)?;
        let mutations = checker.mutation_facts()?;
        functions.push(FunctionRegions {
            brands: checker.brands,
            expressions: checker.expressions,
            origins: checker.origins,
            mutations,
        });
    }
    let work = context.budget.work;
    let allocations = context.budget.allocations;
    let successful = context.violations.is_empty();
    Ok(Analysis {
        bodies: successful.then_some(RegionCheckedBodies {
            loaned,
            functions,
            work,
            allocations,
        }),
        diagnostics: context.diagnostics,
        violations: context.violations,
    })
}

fn new_checker<'a, 'b>(
    context: &'b mut Context,
    bodies: &'a BodyTypes,
    index: usize,
    summaries: &'b [summary::Summary],
    requirements: bool,
) -> Checker<'a, 'b> {
    Checker {
        context,
        bodies,
        function: &bodies.source().functions[index],
        facts: &bodies.functions()[index],
        id: r::FunctionId(index),
        values: Vec::new(),
        paths: Vec::new(),
        places: Vec::new(),
        roots: Vec::new(),
        state: Vec::new(),
        brands: Vec::new(),
        origins: Vec::new(),
        expressions: Vec::new(),
        temporaries: Vec::new(),
        matches: Vec::new(),
        scopes: Vec::new(),
        summaries,
        requirements,
        parameter_roots: Vec::new(),
        outputs: Vec::new(),
        has_exit: false,
        diverges: false,
        call_origin: None,
    }
}

struct Checker<'a, 'b> {
    context: &'b mut Context,
    bodies: &'a BodyTypes,
    function: &'a r::Function,
    facts: &'a FunctionFacts,
    id: r::FunctionId,
    values: Vec<Value>,
    paths: Vec<Path>,
    places: Vec<Place>,
    roots: Vec<Root>,
    state: Vec<ValueId>,
    brands: Vec<Brand>,
    origins: Vec<Option<CallerOrigin>>,
    expressions: Vec<Option<Vec<BrandId>>>,
    /// Stable expression scratch slots, reused on fixed-point visits.
    temporaries: Vec<Option<usize>>,
    matches: Vec<Option<usize>>,
    scopes: Vec<Scope>,
    summaries: &'b [summary::Summary],
    requirements: bool,
    parameter_roots: Vec<summary::ParameterRoot>,
    outputs: Vec<ValueId>,
    has_exit: bool,
    diverges: bool,
    call_origin: Option<Span>,
}
impl Checker<'_, '_> {
    fn constructor(&self, ty: r::Type) -> Option<r::TypeConstructor> {
        if let r::Type::Constructed(id) = ty {
            Some(self.bodies.types()[id.0].constructor)
        } else {
            None
        }
    }
    fn loan(&self, ty: r::Type) -> Option<r::Type> {
        if let Some(r::TypeConstructor::Loan(_, target)) = self.constructor(ty) {
            Some(target)
        } else {
            None
        }
    }
    fn branded(&self, ty: r::Type) -> bool {
        self.bodies.properties(ty).unwrap().contains_arena
    }
    fn node(&mut self, id: ValueId, span: Span) -> Result<Value, Error> {
        self.context.budget.charge(span, 1)?;
        Ok(match &self.values[id.0] {
            Value::Empty => Value::Empty,
            Value::Brands(v) => Value::Brands(self.context.budget.copy(span, v)?),
            Value::Record(d, v) => Value::Record(*d, self.context.budget.copy(span, v)?),
            Value::Case(t, v) => Value::Case(*t, self.context.budget.copy(span, v)?),
            Value::Cases(v) => Value::Cases(self.context.budget.copy(span, v)?),
            Value::References(v) => Value::References(self.context.budget.copy(span, v)?),
            Value::Authority(b) => Value::Authority(*b),
        })
    }
    fn intern(&mut self, value: Value, span: Span) -> Result<ValueId, Error> {
        for (index, previous) in self.values.iter().enumerate() {
            self.context
                .budget
                .charge(span, value.cost().saturating_add(previous.cost()))?;
            if *previous == value {
                return Ok(ValueId(index));
            }
        }
        self.context.budget.values = self
            .context
            .budget
            .values
            .checked_add(1)
            .filter(|n| *n <= self.context.budget.limits.values)
            .ok_or_else(|| {
                self.context
                    .budget
                    .limit(span, "region value limit exceeded")
            })?;
        let id = ValueId(self.values.len());
        self.context.budget.push(span, &mut self.values, value)?;
        Ok(id)
    }
    fn set(&mut self, brands: Vec<BrandId>, span: Span) -> Result<ValueId, Error> {
        if brands.is_empty() {
            Ok(EMPTY)
        } else {
            self.intern(Value::Brands(brands), span)
        }
    }
    fn brand(&mut self, kind: BrandKind, scope: usize, span: Span) -> Result<BrandId, Error> {
        for (index, brand) in self.brands.iter().enumerate() {
            self.context.budget.charge(span, 1)?;
            if brand.kind == kind && self.origins[index].is_none() {
                return Ok(BrandId(index));
            }
        }
        let id = BrandId(self.brands.len());
        self.context
            .budget
            .push(span, &mut self.brands, Brand { kind, span, scope })?;
        self.context.budget.push(span, &mut self.origins, None)?;
        Ok(id)
    }
    fn singleton_brand(&mut self, brand: BrandId, span: Span) -> Result<ValueId, Error> {
        let mut v = self.context.budget.vector(span, 1)?;
        v.push(brand);
        self.set(v, span)
    }
    fn root(
        &mut self,
        ty: r::Type,
        scope: usize,
        value: ValueId,
        span: Span,
    ) -> Result<usize, Error> {
        let id = self.roots.len();
        self.context
            .budget
            .push(span, &mut self.roots, Root { ty, scope })?;
        self.put(id, value, span)?;
        Ok(id)
    }
    fn get(&self, root: usize) -> ValueId {
        self.state.get(root).copied().unwrap_or(EMPTY)
    }
    fn put(&mut self, root: usize, value: ValueId, span: Span) -> Result<(), Error> {
        while self.state.len() <= root {
            self.context.budget.push(span, &mut self.state, EMPTY)?;
        }
        self.state[root] = value;
        Ok(())
    }
    fn place(&mut self, place: Place, span: Span) -> Result<usize, Error> {
        for (index, previous) in self.places.iter().enumerate() {
            self.context.budget.charge(span, 1)?;
            if *previous == place {
                return Ok(index);
            }
        }
        self.context.budget.places = self
            .context
            .budget
            .places
            .checked_add(1)
            .filter(|n| *n <= self.context.budget.limits.places)
            .ok_or_else(|| {
                self.context
                    .budget
                    .limit(span, "region place limit exceeded")
            })?;
        let id = self.places.len();
        self.context.budget.push(span, &mut self.places, place)?;
        Ok(id)
    }
    fn reference(&mut self, places: Vec<usize>, span: Span) -> Result<ValueId, Error> {
        self.intern(Value::References(places), span)
    }
    fn root_reference(&mut self, root: usize, span: Span) -> Result<ValueId, Error> {
        let place = self.place(Place { root, path: None }, span)?;
        let mut places = self.context.budget.vector(span, 1)?;
        places.push(place);
        self.reference(places, span)
    }
    fn append(&mut self, place: usize, step: Step, span: Span) -> Result<usize, Error> {
        let p = self.places[place];
        let node = Path {
            parent: p.path,
            step,
        };
        let mut found = None;
        for (index, previous) in self.paths.iter().enumerate() {
            self.context.budget.charge(span, 1)?;
            if *previous == node {
                found = Some(index);
                break;
            }
        }
        let path = if let Some(index) = found {
            index
        } else {
            let index = self.paths.len();
            self.context.budget.push(span, &mut self.paths, node)?;
            index
        };
        self.place(
            Place {
                root: p.root,
                path: Some(path),
            },
            span,
        )
    }
    fn path(&mut self, mut path: Option<usize>, span: Span) -> Result<Vec<Step>, Error> {
        let mut out = Vec::new();
        while let Some(id) = path {
            self.context.budget.enter(span, out.len())?;
            self.context
                .budget
                .push(span, &mut out, self.paths[id].step)?;
            path = self.paths[id].parent;
        }
        self.context.budget.charge(span, out.len())?;
        out.reverse();
        Ok(out)
    }
    fn initialize(&mut self) -> Result<(), Error> {
        let span = self.function.span;
        assert_eq!(self.intern(Value::Empty, span)?, EMPTY);
        for ty in self.facts.locals() {
            self.root(*ty, 0, EMPTY, span)?;
        }
        self.expressions = self
            .context
            .budget
            .vector(span, self.function.expressions.len())?;
        self.expressions
            .resize_with(self.function.expressions.len(), || None);
        self.temporaries = self
            .context
            .budget
            .vector(span, self.function.expressions.len())?;
        self.temporaries
            .resize(self.function.expressions.len(), None);
        self.matches = self
            .context
            .budget
            .vector(span, self.function.expressions.len())?;
        self.matches.resize(self.function.expressions.len(), None);
        self.context
            .budget
            .charge(span, self.function.expressions.len())?;
        for local in &self.function.parameters {
            let value = self.parameter(*local, self.facts.locals()[local.0], 0)?;
            self.put(local.0, value, span)?;
        }
        Ok(())
    }
    fn collect(&mut self, value: ValueId, span: Span, depth: usize) -> Result<Vec<BrandId>, Error> {
        self.context.budget.enter(span, depth)?;
        let mut out = Vec::new();
        match self.node(value, span)? {
            Value::Empty => {}
            Value::Brands(v) => return Ok(v),
            Value::Authority(brand) => self.context.budget.push(span, &mut out, brand)?,
            Value::Record(_, values) | Value::Case(_, values) | Value::Cases(values) => {
                for value in values {
                    let v = self.collect(value, span, depth + 1)?;
                    self.context.budget.union(span, &mut out, &v)?;
                }
            }
            Value::References(places) => {
                for place in places {
                    let value = self.read(place, span, depth + 1)?;
                    let v = self.collect(value, span, depth + 1)?;
                    self.context.budget.union(span, &mut out, &v)?;
                }
            }
        }
        Ok(out)
    }
    fn project(
        &mut self,
        value: ValueId,
        step: Step,
        span: Span,
        depth: usize,
    ) -> Result<ValueId, Error> {
        self.context.budget.enter(span, depth)?;
        match self.node(value, span)? {
            Value::Empty => Ok(EMPTY),
            Value::Brands(_) => Ok(if self.branded(step.ty) { value } else { EMPTY }),
            Value::Record(_, values) => {
                if let StepKind::Field(i) = step.kind {
                    Ok(values[i])
                } else {
                    Ok(EMPTY)
                }
            }
            Value::Case(tag, values) => {
                if let StepKind::Payload(expected, i) = step.kind {
                    Ok(if tag == expected { values[i] } else { EMPTY })
                } else {
                    Ok(EMPTY)
                }
            }
            Value::Cases(values) => {
                let mut out = EMPTY;
                for v in values {
                    let v = self.project(v, step, span, depth + 1)?;
                    out = self.join(out, v, step.ty, span, depth + 1)?;
                }
                Ok(out)
            }
            _ => Ok(EMPTY),
        }
    }
    fn read(&mut self, place: usize, span: Span, depth: usize) -> Result<ValueId, Error> {
        self.context.budget.enter(span, depth)?;
        let p = self.places[place];
        let mut value = self.get(p.root);
        for step in self.path(p.path, span)? {
            value = self.project(value, step, span, depth + 1)?;
        }
        Ok(value)
    }
    fn record(
        &mut self,
        definition: r::DefinitionId,
        fields: Vec<ValueId>,
        span: Span,
    ) -> Result<ValueId, Error> {
        self.context.budget.charge(span, fields.len())?;
        if fields.iter().all(|v| *v == EMPTY) {
            Ok(EMPTY)
        } else {
            self.intern(Value::Record(definition, fields), span)
        }
    }
    fn case(&mut self, tag: Tag, payload: Vec<ValueId>, span: Span) -> Result<ValueId, Error> {
        self.context.budget.charge(span, payload.len())?;
        if payload.iter().all(|v| *v == EMPTY) {
            Ok(EMPTY)
        } else {
            self.intern(Value::Case(tag, payload), span)
        }
    }
    fn core_payload(&self, ty: r::Type, tag: core::Case) -> Option<r::Type> {
        match (self.constructor(ty), tag) {
            (Some(r::TypeConstructor::Option(t)), core::Case::Some) => Some(t),
            (Some(r::TypeConstructor::Result(t, _)), core::Case::Ok) => Some(t),
            (Some(r::TypeConstructor::Result(_, e)), core::Case::Err) => Some(e),
            _ => None,
        }
    }
    fn join(
        &mut self,
        a: ValueId,
        b: ValueId,
        ty: r::Type,
        span: Span,
        depth: usize,
    ) -> Result<ValueId, Error> {
        self.context.budget.enter(span, depth)?;
        if a == b || b == EMPTY {
            return Ok(a);
        }
        if a == EMPTY {
            return Ok(b);
        }
        if self.loan(ty).is_some() {
            let Value::References(mut left) = self.node(a, span)? else {
                return Ok(b);
            };
            let Value::References(right) = self.node(b, span)? else {
                return Ok(a);
            };
            self.context.budget.union(span, &mut left, &right)?;
            return self.reference(left, span);
        }
        if let r::Type::Nominal(id) = ty
            && let r::DefinitionKind::Record(fields) = &self.bodies.source().definitions[id.0].kind
        {
            let mut values = self.context.budget.vector(span, fields.len())?;
            for (index, field) in fields.iter().enumerate() {
                let ty = field.value_type.value_type;
                let step = Step {
                    kind: StepKind::Field(index),
                    ty,
                };
                let a = self.project(a, step, span, depth + 1)?;
                let b = self.project(b, step, span, depth + 1)?;
                values.push(self.join(a, b, ty, span, depth + 1)?);
            }
            return self.record(id, values, span);
        }
        let mut cases = Vec::new();
        if let r::Type::Nominal(id) = ty
            && let r::DefinitionKind::Enum(variants) = &self.bodies.source().definitions[id.0].kind
        {
            for (index, variant) in variants.iter().enumerate() {
                let tag = Tag::User(id, index);
                let mut payload = self.context.budget.vector(span, variant.payload.len())?;
                for (i, field) in variant.payload.iter().enumerate() {
                    let ty = field.value_type;
                    let step = Step {
                        kind: StepKind::Payload(tag, i),
                        ty,
                    };
                    let a = self.project(a, step, span, depth + 1)?;
                    let b = self.project(b, step, span, depth + 1)?;
                    payload.push(self.join(a, b, ty, span, depth + 1)?);
                }
                let value = self.case(tag, payload, span)?;
                if value != EMPTY {
                    self.context.budget.push(span, &mut cases, value)?;
                }
            }
        } else if matches!(
            self.constructor(ty),
            Some(r::TypeConstructor::Option(_) | r::TypeConstructor::Result(..))
        ) {
            for tag in [core::Case::Some, core::Case::Ok, core::Case::Err] {
                if let Some(payload_ty) = self.core_payload(ty, tag) {
                    let step = Step {
                        kind: StepKind::Payload(Tag::Core(tag), 0),
                        ty: payload_ty,
                    };
                    let a = self.project(a, step, span, depth + 1)?;
                    let b = self.project(b, step, span, depth + 1)?;
                    let joined = self.join(a, b, payload_ty, span, depth + 1)?;
                    let mut payload = self.context.budget.vector(span, 1)?;
                    payload.push(joined);
                    let value = self.case(Tag::Core(tag), payload, span)?;
                    if value != EMPTY {
                        self.context.budget.push(span, &mut cases, value)?;
                    }
                }
            }
        } else {
            let mut left = self.collect(a, span, depth + 1)?;
            let right = self.collect(b, span, depth + 1)?;
            self.context.budget.union(span, &mut left, &right)?;
            return self.set(left, span);
        }
        if cases.is_empty() {
            Ok(EMPTY)
        } else if cases.len() == 1 {
            Ok(cases[0])
        } else {
            self.intern(Value::Cases(cases), span)
        }
    }
    fn update(
        &mut self,
        old: ValueId,
        ty: r::Type,
        path: &[Step],
        value: ValueId,
        span: Span,
        depth: usize,
    ) -> Result<ValueId, Error> {
        self.context.budget.enter(span, depth)?;
        let Some((step, rest)) = path.split_first() else {
            return Ok(value);
        };
        match step.kind {
            StepKind::Field(index) => {
                let r::Type::Nominal(id) = ty else {
                    unreachable!("typed record place")
                };
                let r::DefinitionKind::Record(fields) =
                    &self.bodies.source().definitions[id.0].kind
                else {
                    unreachable!("typed record place")
                };
                let mut values = self.context.budget.vector(span, fields.len())?;
                for (i, field) in fields.iter().enumerate() {
                    let ty = field.value_type.value_type;
                    let old = self.project(
                        old,
                        Step {
                            kind: StepKind::Field(i),
                            ty,
                        },
                        span,
                        depth + 1,
                    )?;
                    values.push(if i == index {
                        self.update(old, ty, rest, value, span, depth + 1)?
                    } else {
                        old
                    });
                }
                self.record(id, values, span)
            }
            StepKind::Element => {
                let child = self.project(old, *step, span, depth + 1)?;
                let value = self.update(child, step.ty, rest, value, span, depth + 1)?;
                let mut brands = self.collect(old, span, depth + 1)?;
                let incoming = self.collect(value, span, depth + 1)?;
                self.context.budget.union(span, &mut brands, &incoming)?;
                self.set(brands, span)
            }
            StepKind::Payload(_, _) => {
                // Pattern payloads are read loans; mutable core projections
                // point at collection elements, not an owning sum payload.
                self.join(old, value, ty, span, depth + 1)
            }
        }
    }
    fn store(
        &mut self,
        place: usize,
        value: ValueId,
        weak: bool,
        span: Span,
        depth: usize,
    ) -> Result<(), Error> {
        let p = self.places[place];
        self.check_store(value, self.roots[p.root].scope, span, depth + 1)?;
        self.record_store(p.root, value, span)?;
        let path = self.path(p.path, span)?;
        let old = self.get(p.root);
        let new = self.update(old, self.roots[p.root].ty, &path, value, span, depth + 1)?;
        let new = if weak {
            self.join(old, new, self.roots[p.root].ty, span, depth + 1)?
        } else {
            new
        };
        self.put(p.root, new, span)
    }
    fn check_store(
        &mut self,
        value: ValueId,
        scope: usize,
        span: Span,
        depth: usize,
    ) -> Result<(), Error> {
        for id in self.collect(value, span, depth + 1)? {
            let brand = self.brands[id.0];
            if brand.scope > scope {
                self.context.report(Violation {
                    function: self.id,
                    kind: ViolationKind::Escape,
                    brand,
                    span,
                    related: self.call_origin,
                })?;
            }
        }
        Ok(())
    }
    fn same_arena(
        &mut self,
        owner: BrandId,
        value: ValueId,
        span: Span,
        depth: usize,
    ) -> Result<(), Error> {
        for id in self.collect(value, span, depth + 1)? {
            if id != owner {
                self.context.report(Violation {
                    function: self.id,
                    kind: ViolationKind::CrossArena,
                    brand: self.brands[id.0],
                    span,
                    related: None,
                })?;
            }
        }
        Ok(())
    }
    fn ref_places(&mut self, value: ValueId, span: Span) -> Result<Vec<usize>, Error> {
        if let Value::References(places) = self.node(value, span)? {
            Ok(places)
        } else {
            Ok(Vec::new())
        }
    }
    fn locations(&mut self, id: r::ExpressionId, depth: usize) -> Result<Vec<usize>, Error> {
        if self.diverges {
            return Ok(Vec::new());
        }
        let expression = &self.function.expressions[id.0];
        let span = expression.span;
        self.context.budget.enter(span, depth)?;
        match &expression.kind {
            r::ExpressionKind::Local(local) => {
                let p = self.place(
                    Place {
                        root: local.0,
                        path: None,
                    },
                    span,
                )?;
                let mut places = self.context.budget.vector(span, 1)?;
                places.push(p);
                Ok(places)
            }
            r::ExpressionKind::Dereference(value) => {
                let value = self.eval(*value, depth + 1)?;
                self.ref_places(value, span)
            }
            r::ExpressionKind::Field { receiver, .. } => {
                let places = if self.loan(self.facts.expressions()[receiver.0]).is_some() {
                    let value = self.eval(*receiver, depth + 1)?;
                    self.ref_places(value, span)?
                } else {
                    self.locations(*receiver, depth + 1)?
                };
                let mut out = self.context.budget.vector(span, places.len())?;
                let step = Step {
                    kind: StepKind::Field(self.facts.field_index(id).unwrap()),
                    ty: self.facts.expressions()[id.0],
                };
                for place in places {
                    out.push(self.append(place, step, span)?);
                }
                Ok(out)
            }
            _ => {
                let value = self.eval(id, depth + 1)?;
                let root = self.temporary_root(id, self.facts.expressions()[id.0], value, span)?;
                let p = self.place(Place { root, path: None }, span)?;
                let mut out = self.context.budget.vector(span, 1)?;
                out.push(p);
                Ok(out)
            }
        }
    }
    fn load_locations(
        &mut self,
        places: &[usize],
        ty: r::Type,
        span: Span,
        depth: usize,
    ) -> Result<ValueId, Error> {
        let mut out = EMPTY;
        for place in places {
            let value = self.read(*place, span, depth + 1)?;
            out = self.join(out, value, ty, span, depth + 1)?;
        }
        Ok(out)
    }
    fn temporary_root(
        &mut self,
        id: r::ExpressionId,
        ty: r::Type,
        value: ValueId,
        span: Span,
    ) -> Result<usize, Error> {
        if let Some(root) = self.temporaries[id.0] {
            self.put(root, value, span)?;
            Ok(root)
        } else {
            let root = self.root(ty, self.scopes.len(), value, span)?;
            self.temporaries[id.0] = Some(root);
            Ok(root)
        }
    }
    fn argument(
        &mut self,
        id: r::ExpressionId,
        parameter: r::Type,
        depth: usize,
    ) -> Result<ValueId, Error> {
        if self.loan(parameter) == Some(self.facts.expressions()[id.0]) {
            let places = self.locations(id, depth + 1)?;
            self.reference(places, self.function.expressions[id.0].span)
        } else {
            self.eval(id, depth + 1)
        }
    }
    fn eval(&mut self, id: r::ExpressionId, depth: usize) -> Result<ValueId, Error> {
        if self.diverges {
            return Ok(EMPTY);
        }
        let expression = &self.function.expressions[id.0];
        let span = expression.span;
        self.context.budget.enter(span, depth)?;
        let ty = self.facts.expressions()[id.0];
        let value = match &expression.kind {
            r::ExpressionKind::Local(local) => self.get(local.0),
            r::ExpressionKind::Borrow { place, .. } => {
                let places = self.locations(r::ExpressionId(place.0), depth + 1)?;
                self.reference(places, span)?
            }
            r::ExpressionKind::Field { .. } | r::ExpressionKind::Dereference(_) => {
                let places = self.locations(id, depth + 1)?;
                self.load_locations(&places, ty, span, depth + 1)?
            }
            r::ExpressionKind::Record { definition, fields } => {
                let mut values = self.context.budget.vector(span, fields.len())?;
                values.resize(fields.len(), EMPTY);
                for (i, field) in fields.iter().enumerate() {
                    let value = self.eval(field.value, depth + 1)?;
                    values[self.facts.record_order(id).unwrap()[i]] = value;
                }
                self.record(*definition, values, span)?
            }
            r::ExpressionKind::CoreCase { case, payload, .. } => {
                let mut values = self.context.budget.vector(span, payload.len())?;
                for value in payload {
                    values.push(self.eval(*value, depth + 1)?);
                }
                self.case(Tag::Core(*case), values, span)?
            }
            r::ExpressionKind::Call { callee, arguments } => {
                let mut values = self.context.budget.vector(span, arguments.len())?;
                for (index, argument) in arguments.iter().enumerate() {
                    values.push(self.argument(
                        *argument,
                        self.facts.call(id).unwrap().parameters[index],
                        depth + 1,
                    )?);
                }
                if self.diverges {
                    EMPTY
                } else if let r::Callable::Case { definition, index } = callee {
                    self.case(Tag::User(*definition, *index), values, span)?
                } else if let r::Callable::Guest(callee) = callee {
                    self.apply_call(*callee, &values, span, depth + 1)?
                } else {
                    EMPTY
                }
            }
            r::ExpressionKind::CoreCall {
                operation,
                arguments,
                ..
            } => {
                let mut values = self.context.budget.vector(span, arguments.len())?;
                for (index, argument) in arguments.iter().enumerate() {
                    values.push(self.argument(
                        *argument,
                        self.facts.call(id).unwrap().parameters[index],
                        depth + 1,
                    )?);
                }
                if self.diverges {
                    EMPTY
                } else {
                    self.core_call(id, *operation, arguments, &values, depth + 1)?
                }
            }
            r::ExpressionKind::Add(a, b) | r::ExpressionKind::LessThan(a, b) => {
                self.eval(*a, depth + 1)?;
                self.eval(*b, depth + 1)?;
                EMPTY
            }
            _ => EMPTY,
        };
        if self.diverges {
            return Ok(EMPTY);
        }
        let brands = self.collect(value, span, depth + 1)?;
        if let Some(previous) = &mut self.expressions[id.0] {
            self.context.budget.union(span, previous, &brands)?;
        } else {
            self.expressions[id.0] = Some(brands);
        }
        Ok(value)
    }
    fn payload_result(
        &mut self,
        tag: core::Case,
        value: ValueId,
        span: Span,
    ) -> Result<ValueId, Error> {
        let mut payload = self.context.budget.vector(span, 1)?;
        payload.push(value);
        self.case(Tag::Core(tag), payload, span)
    }
    fn authority(&mut self, value: ValueId, span: Span, depth: usize) -> Result<BrandId, Error> {
        self.context.budget.enter(span, depth)?;
        match self.node(value, span)? {
            Value::Authority(brand) => Ok(brand),
            Value::References(places) => {
                let value = self.read(places[0], span, depth + 1)?;
                self.authority(value, span, depth + 1)
            }
            _ => unreachable!("typed compiler-created arena authority"),
        }
    }
    fn core_call(
        &mut self,
        id: r::ExpressionId,
        op: core::Operation,
        arguments: &[r::ExpressionId],
        values: &[ValueId],
        depth: usize,
    ) -> Result<ValueId, Error> {
        use core::Operation as O;
        let span = self.function.expressions[id.0].span;
        self.context.budget.enter(span, depth)?;
        match op {
            O::OptionUnwrap | O::ResultUnwrap | O::ResultUnwrapErr => {
                let case = match op {
                    O::OptionUnwrap => core::Case::Some,
                    O::ResultUnwrap => core::Case::Ok,
                    O::ResultUnwrapErr => core::Case::Err,
                    _ => unreachable!(),
                };
                self.project(
                    values[0],
                    Step {
                        kind: StepKind::Payload(Tag::Core(case), 0),
                        ty: self.facts.expressions()[id.0],
                    },
                    span,
                    depth + 1,
                )
            }
            O::ArenaNew | O::ArenaRead | O::ArenaReplace => {
                let owner = self.authority(values[0], span, depth + 1)?;
                for i in 1..values.len() {
                    self.same_arena(
                        owner,
                        values[i],
                        self.function.expressions[arguments[i].0].span,
                        depth + 1,
                    )?;
                }
                if op == O::ArenaNew {
                    let value = self.singleton_brand(owner, span)?;
                    self.payload_result(core::Case::Ok, value, span)
                } else if op == O::ArenaRead {
                    let r::ExpressionKind::CoreCall { type_arguments, .. } =
                        &self.function.expressions[id.0].kind
                    else {
                        unreachable!()
                    };
                    let ty = type_arguments[0].value_type;
                    let value = if self.branded(ty) {
                        self.singleton_brand(owner, span)?
                    } else {
                        EMPTY
                    };
                    let root = self.temporary_root(id, ty, value, span)?;
                    let value = self.root_reference(root, span)?;
                    self.payload_result(core::Case::Ok, value, span)
                } else {
                    Ok(EMPTY)
                }
            }
            O::VecPush | O::MapInsert => {
                let value = *values.last().unwrap();
                let incoming = self.collect(value, span, depth + 1)?;
                let places = self.ref_places(values[0], span)?;
                let mut previous = EMPTY;
                for place in places {
                    let old = self.read(place, span, depth + 1)?;
                    let ty = self.place_type(place);
                    previous = self.join(previous, old, ty, span, depth + 1)?;
                    let mut brands = self.collect(old, span, depth + 1)?;
                    self.context.budget.union(span, &mut brands, &incoming)?;
                    let new = self.set(brands, span)?;
                    self.store(place, new, false, span, depth + 1)?;
                }
                if op == O::MapInsert {
                    let ty = self.element_type(self.facts.call(id).unwrap().parameters[0]);
                    let brands = self.collect(previous, span, depth + 1)?;
                    let value = if self.branded(ty) {
                        self.set(brands, span)?
                    } else {
                        EMPTY
                    };
                    let value = self.payload_result(core::Case::Some, value, span)?;
                    self.payload_result(core::Case::Ok, value, span)
                } else {
                    Ok(EMPTY)
                }
            }
            O::VecGet | O::VecGetMut | O::MapGet => {
                let ty = self.element_type(self.facts.call(id).unwrap().parameters[0]);
                let places = self.ref_places(values[0], span)?;
                let mut elements = self.context.budget.vector(span, places.len())?;
                for place in places {
                    elements.push(self.append(
                        place,
                        Step {
                            kind: StepKind::Element,
                            ty,
                        },
                        span,
                    )?);
                }
                let value = self.reference(elements, span)?;
                self.payload_result(
                    if op == O::MapGet {
                        core::Case::Some
                    } else {
                        core::Case::Ok
                    },
                    value,
                    span,
                )
            }
            O::VecPop | O::MapRemove => {
                let ty = self.element_type(self.facts.call(id).unwrap().parameters[0]);
                let brands = self.collect(values[0], span, depth + 1)?;
                let value = if self.branded(ty) {
                    self.set(brands, span)?
                } else {
                    EMPTY
                };
                self.payload_result(core::Case::Some, value, span)
            }
            O::SharedNew | O::SharedReplace => {
                for brand in self.collect(*values.last().unwrap(), span, depth + 1)? {
                    self.context.report(Violation {
                        function: self.id,
                        kind: ViolationKind::SharedStorage,
                        brand: self.brands[brand.0],
                        span,
                        related: None,
                    })?;
                }
                Ok(EMPTY)
            }
            O::SharedRead => {
                let r::ExpressionKind::CoreCall { type_arguments, .. } =
                    &self.function.expressions[id.0].kind
                else {
                    unreachable!()
                };
                let root = self.temporary_root(id, type_arguments[0].value_type, EMPTY, span)?;
                let value = self.root_reference(root, span)?;
                self.payload_result(core::Case::Ok, value, span)
            }
            O::Allocator => {
                let target = self.loan(self.facts.expressions()[id.0]).unwrap();
                let root = self.temporary_root(id, target, EMPTY, span)?;
                self.root_reference(root, span)
            }
            _ => Ok(EMPTY),
        }
    }
    fn element_type(&self, mut ty: r::Type) -> r::Type {
        if let Some(target) = self.loan(ty) {
            ty = target;
        }
        match self.constructor(ty).unwrap() {
            r::TypeConstructor::Vec(t) | r::TypeConstructor::Map(_, t) => t,
            _ => unreachable!("typed collection call"),
        }
    }
    fn place_type(&self, place: usize) -> r::Type {
        let p = self.places[place];
        if let Some(path) = p.path {
            self.paths[path].step.ty
        } else {
            self.roots[p.root].ty
        }
    }
    fn begin(&mut self, span: Span) -> Result<(), Error> {
        self.context.budget.enter(span, self.scopes.len())?;
        self.context
            .budget
            .push(span, &mut self.scopes, Scope { locals: Vec::new() })
    }
    fn end(&mut self, span: Span) -> Result<(), Error> {
        let depth = self.scopes.len();
        self.scopes.pop().unwrap();
        for root in 0..self.state.len() {
            self.context.budget.charge(span, 1)?;
            if self.roots[root].scope >= depth {
                self.state[root] = EMPTY;
            }
        }
        Ok(())
    }
    fn declare(&mut self, local: r::LocalId, value: ValueId, span: Span) -> Result<(), Error> {
        self.roots[local.0].scope = self.scopes.len();
        self.check_store(value, self.scopes.len(), span, 0)?;
        self.put(local.0, value, span)?;
        self.context
            .budget
            .push(span, &mut self.scopes.last_mut().unwrap().locals, local.0)
    }
    fn merge(
        &mut self,
        left: Vec<ValueId>,
        right: &[ValueId],
        span: Span,
    ) -> Result<Vec<ValueId>, Error> {
        let count = left.len().max(right.len());
        let mut values = self.context.budget.vector(span, count)?;
        for index in 0..count {
            // Avoid holding a metadata borrow across recursive joins.
            let ty = self.roots[index].ty;
            let a = left.get(index).copied().unwrap_or(EMPTY);
            let b = right.get(index).copied().unwrap_or(EMPTY);
            values.push(self.join(a, b, ty, span, 0)?);
        }
        Ok(values)
    }
    fn block(&mut self, id: r::BlockId, depth: usize) -> Result<bool, Error> {
        self.diverges = false;
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
    fn statement(&mut self, statement: &r::Statement, depth: usize) -> Result<bool, Error> {
        let span = statement.span;
        self.context.budget.enter(span, depth)?;
        match &statement.kind {
            r::StatementKind::LetMut { local, initializer } => {
                let value = self.eval(*initializer, depth + 1)?;
                if self.diverges {
                    return Ok(false);
                }
                self.declare(*local, value, span)?;
            }
            r::StatementKind::Assign { local, value } => {
                let value = self.eval(*value, depth + 1)?;
                if self.diverges {
                    return Ok(false);
                }
                self.check_store(value, self.roots[local.0].scope, span, depth + 1)?;
                self.put(local.0, value, span)?;
            }
            r::StatementKind::PlaceAssign { place, value } => {
                let places = self.locations(r::ExpressionId(place.0), depth + 1)?;
                let value = self.eval(*value, depth + 1)?;
                if self.diverges {
                    return Ok(false);
                }
                let weak = places.len() != 1;
                for place in places {
                    self.store(place, value, weak, span, depth + 1)?;
                }
            }
            r::StatementKind::Return(value) => {
                let value = self.eval(*value, depth + 1)?;
                if self.diverges {
                    return Ok(false);
                }
                for brand in self.collect(value, span, depth + 1)? {
                    self.context.report(Violation {
                        function: self.id,
                        kind: ViolationKind::ReturnEscape,
                        brand: self.brands[brand.0],
                        span,
                        related: None,
                    })?;
                }
                self.capture_outputs(span)?;
                return Ok(false);
            }
            r::StatementKind::Arena {
                owner,
                allocator,
                body,
            } => {
                self.eval(*allocator, depth + 1)?;
                if self.diverges {
                    return Ok(false);
                }
                self.begin(span)?;
                let brand = self.brand(
                    BrandKind::Local(r::ArenaOwnerId {
                        function: self.id,
                        local: *owner,
                    }),
                    self.scopes.len(),
                    self.function.locals[owner.0].name.span,
                )?;
                let value = self.intern(Value::Authority(brand), span)?;
                self.declare(*owner, value, span)?;
                let reaches = self.block(*body, depth + 1)?;
                self.end(span)?;
                return Ok(reaches);
            }
            r::StatementKind::If {
                condition,
                then_block,
                else_block,
            } => {
                self.eval(*condition, depth + 1)?;
                if self.diverges {
                    return Ok(false);
                }
                let base = std::mem::take(&mut self.state);
                self.state = self.context.budget.copy(span, &base)?;
                self.diverges = false;
                let yes = self.block(*then_block, depth + 1)?;
                let yes_state = std::mem::take(&mut self.state);
                self.state = self.context.budget.copy(span, &base)?;
                let no = if let Some(block) = else_block {
                    self.block(*block, depth + 1)?
                } else {
                    true
                };
                let no_state = std::mem::take(&mut self.state);
                self.state = match (yes, no) {
                    (true, true) => self.merge(yes_state, &no_state, span)?,
                    (true, false) => yes_state,
                    (false, true) => no_state,
                    (false, false) => base,
                };
                self.diverges = false;
                return Ok(yes || no);
            }
            r::StatementKind::While { condition, body } => {
                let mut header = self.context.budget.copy(span, &self.state)?;
                loop {
                    self.context.budget.charge(span, 1)?;
                    self.state = self.context.budget.copy(span, &header)?;
                    self.eval(*condition, depth + 1)?;
                    if self.diverges {
                        return Ok(false);
                    }
                    let exit = self.context.budget.copy(span, &self.state)?;
                    if !self.block(*body, depth + 1)? {
                        self.state = exit;
                        break;
                    }
                    let back = std::mem::take(&mut self.state);
                    let previous = self.context.budget.copy(span, &header)?;
                    let next = self.merge(header, &back, span)?;
                    self.context.budget.charge(span, next.len())?;
                    let stable = next
                        .iter()
                        .enumerate()
                        .all(|(i, v)| *v == previous.get(i).copied().unwrap_or(EMPTY));
                    if stable {
                        self.state = exit;
                        break;
                    }
                    header = next;
                }
                self.diverges = false;
            }
            r::StatementKind::Match { scrutinee, arms } => {
                self.begin(span)?;
                let value = self.eval(*scrutinee, depth + 1)?;
                if self.diverges {
                    self.end(span)?;
                    return Ok(false);
                }
                let ty = self.facts.expressions()[scrutinee.0];
                // Non-Copy patterns only read their scrutinee. A snapshot
                // retains field precision; provisional loan payloads retain
                // their original reference locations.
                let root = if let Some(root) = self.matches[scrutinee.0] {
                    self.put(root, value, span)?;
                    root
                } else {
                    let root = self.root(ty, self.scopes.len(), value, span)?;
                    self.matches[scrutinee.0] = Some(root);
                    root
                };
                let place = self.place(Place { root, path: None }, span)?;
                let base = std::mem::take(&mut self.state);
                let mut joined = None;
                for arm in arms {
                    self.context.budget.charge(arm.span, 1)?;
                    if self.facts.arm_is_unreachable(arm.pattern) {
                        continue;
                    }
                    self.state = self.context.budget.copy(arm.span, &base)?;
                    self.begin(arm.span)?;
                    self.pattern(arm.pattern, value, &[place], ty, depth + 1)?;
                    let reaches = self.block(arm.body, depth + 1)?;
                    self.end(arm.span)?;
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
                self.end(span)?;
                self.diverges = false;
                return Ok(reaches);
            }
        }
        Ok(true)
    }
    fn pattern(
        &mut self,
        id: r::PatternId,
        mut value: ValueId,
        locations: &[usize],
        mut ty: r::Type,
        depth: usize,
    ) -> Result<(), Error> {
        let pattern = &self.function.patterns[id.0];
        let span = pattern.span;
        self.context.budget.enter(span, depth)?;
        if let r::PatternKind::Binding(local) = pattern.kind {
            let value =
                if self.loan(self.facts.locals()[local.0]).is_some() && self.loan(ty).is_none() {
                    let locations = self.context.budget.copy(span, locations)?;
                    self.reference(locations, span)?
                } else {
                    value
                };
            return self.declare(local, value, span);
        }
        let mut current = self.context.budget.copy(span, locations)?;
        if let Some(target) = self.loan(ty) {
            current = self.ref_places(value, span)?;
            value = self.load_locations(&current, target, span, depth + 1)?;
            ty = target;
        }
        match &pattern.kind {
            r::PatternKind::Record { definition, fields } => {
                let r::DefinitionKind::Record(declared) =
                    &self.bodies.source().definitions[definition.0].kind
                else {
                    unreachable!()
                };
                for field in fields {
                    let index = declared
                        .iter()
                        .position(|d| d.name.text == field.name.text)
                        .unwrap();
                    self.context.budget.charge(span, declared.len())?;
                    let ty = declared[index].value_type.value_type;
                    let step = Step {
                        kind: StepKind::Field(index),
                        ty,
                    };
                    let child = self.project(value, step, span, depth + 1)?;
                    let mut places = self.context.budget.vector(span, current.len())?;
                    for p in &current {
                        places.push(self.append(*p, step, span)?);
                    }
                    self.pattern(field.pattern, child, &places, ty, depth + 1)?;
                }
            }
            r::PatternKind::Case {
                definition,
                index,
                payload,
                ..
            } => {
                let r::DefinitionKind::Enum(cases) =
                    &self.bodies.source().definitions[definition.0].kind
                else {
                    unreachable!()
                };
                for (i, id) in payload.iter().enumerate() {
                    let ty = cases[*index].payload[i].value_type;
                    let step = Step {
                        kind: StepKind::Payload(Tag::User(*definition, *index), i),
                        ty,
                    };
                    let child = self.project(value, step, span, depth + 1)?;
                    let mut places = self.context.budget.vector(span, current.len())?;
                    for p in &current {
                        places.push(self.append(*p, step, span)?);
                    }
                    self.pattern(*id, child, &places, ty, depth + 1)?;
                }
            }
            r::PatternKind::CoreCase { case, payload, .. } => {
                for (i, id) in payload.iter().enumerate() {
                    let child_ty = self
                        .core_payload(ty, *case)
                        .unwrap_or(self.facts.patterns()[id.0]);
                    let step = Step {
                        kind: StepKind::Payload(Tag::Core(*case), i),
                        ty: child_ty,
                    };
                    let child = self.project(value, step, span, depth + 1)?;
                    let mut places = self.context.budget.vector(span, current.len())?;
                    for p in &current {
                        places.push(self.append(*p, step, span)?);
                    }
                    self.pattern(*id, child, &places, child_ty, depth + 1)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}
