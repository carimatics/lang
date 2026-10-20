//! Typed ownership instructions and CFG shape. Successful verification does
//! not prove move/loan/region state, cleanup, layouts, or executable semantics.
//! These Rust enums and logical case ordinals are not a wire encoding.

use super::super as m;
use super::{declarations as d, places as p, *};

mod term;
pub use term::Term;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Profile {
    Synchronous,
    Tasks,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LoanId(pub u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TaskScopeId(pub u32);
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskScope {
    pub parent: Option<TaskScopeId>,
    pub span: Span,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskBinding {
    pub target: CleanupTarget,
    pub scope: TaskScopeId,
}
/// Function-local identity of a destination capture site, not a source value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedPlaceId(pub u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EndMode {
    Required,
    IfActive,
}
/// A generated lexical/temporary cleanup range. Targets describe obligations,
/// not their physical destruction order; runtime order follows initialization.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Cleanup {
    pub steps: Vec<CleanupStep>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CleanupTarget {
    Local(d::LocalId),
    Value(m::Register),
    Loan(LoanId),
    Arena(d::LocalId),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CleanupStep {
    pub target: CleanupTarget,
    pub span: Span,
}
impl CleanupStep {
    /// Static flow transfer only. Execution drains the whole range in dynamic
    /// order, ending registrations before cleaning owners.
    pub fn instruction(self) -> Instruction {
        Instruction {
            results: Vec::new(),
            op: match self.target {
                CleanupTarget::Local(local) => Op::DropLocalIfInitialized(local),
                CleanupTarget::Value(value) => Op::DropValueIfAvailable(value),
                CleanupTarget::Loan(loan) => Op::EndBorrow {
                    loan,
                    mode: EndMode::IfActive,
                },
                CleanupTarget::Arena(owner) => Op::ArenaExit { owner },
            },
            span: self.span,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Op {
    TaskScopeEnter(TaskScopeId),
    TaskScopeExit(TaskScopeId),
    TaskSpawn {
        function: FunctionId,
        arguments: Vec<m::Register>,
    },
    TaskAwait(m::Register),
    TaskCancel(p::PlaceId),
    TaskYield,
    TaskCheck,
    TaskSaveReturn(m::Register),
    I64(i64),
    Bool(bool),
    Unit,
    /// Terminal guest failure; requires an i64 code and an Unreachable block.
    Panic(m::Register),
    Add(m::Register, m::Register),
    LessThan(m::Register, m::Register),
    EqualI64(m::Register, m::Register),
    Copy(m::Register),
    Move(m::Register),
    CopyPlace(p::PlaceId),
    MovePlace(p::PlaceId),
    Initialize {
        local: d::LocalId,
        value: m::Register,
    },
    Replace {
        place: p::PlaceId,
        value: m::Register,
    },
    /// Evaluate and retain a destination path before RHS effects. This does not
    /// activate an exclusive loan; publication must revalidate its dependencies.
    PreparePlace {
        destination: PreparedPlaceId,
        place: p::PlaceId,
    },
    /// Publish through the captured path, without rereading reference cells.
    ReplacePrepared {
        destination: PreparedPlaceId,
        value: m::Register,
    },
    /// Abandon a captured path, for example on an RHS failure edge.
    DiscardPrepared(PreparedPlaceId),
    DropValue(m::Register),
    DropLocal(d::LocalId),
    /// Drop an initialized local, or do nothing when its value was moved out.
    DropLocalIfInitialized(d::LocalId),
    /// Drop a remaining SSA owner, or do nothing when it was consumed.
    DropValueIfAvailable(m::Register),
    Cleanup(Cleanup),
    Borrow {
        loan: LoanId,
        kind: LoanKind,
        place: p::PlaceId,
    },
    EndBorrow {
        loan: LoanId,
        mode: EndMode,
    },
    ArenaEnter {
        owner: d::LocalId,
        allocator: m::Register,
    },
    ArenaExit {
        owner: d::LocalId,
    },
    Call {
        function: FunctionId,
        arguments: Vec<m::Register>,
    },
    Import {
        import: m::ImportId,
        arguments: Vec<m::Register>,
    },
    CoreCall(core::Call),
    Record {
        definition: DefinitionId,
        fields: Vec<m::Register>,
        /// Declaration-field ordinals in actual initialization order. This is
        /// a complete permutation, independent of physical field placement.
        initialization_order: Vec<u32>,
    },
    Variant {
        case: u32,
        payload: Vec<m::Register>,
    },
    IsCase {
        value: m::Register,
        case: u32,
    },
    CopyCaseField {
        value: m::Register,
        case: u32,
        field: u32,
        guard: m::Register,
    },
    BorrowCaseField {
        loan: LoanId,
        value: m::Register,
        case: u32,
        field: u32,
        guard: m::Register,
    },
    /// Consume the entire owned variant, producing every payload field.
    /// Source non-Copy matching still borrows; this is not partial extraction.
    UnpackVariant {
        value: m::Register,
        case: u32,
        guard: m::Register,
    },
}
impl Op {
    pub const fn is_task(&self) -> bool {
        matches!(
            self,
            Self::TaskScopeEnter(_)
                | Self::TaskScopeExit(_)
                | Self::TaskSpawn { .. }
                | Self::TaskAwait(_)
                | Self::TaskCancel(_)
                | Self::TaskYield
                | Self::TaskCheck
                | Self::TaskSaveReturn(_)
        )
    }
    pub fn operands(&self) -> impl Iterator<Item = m::Register> + '_ {
        let (fixed, variable): ([Option<m::Register>; 2], &[m::Register]) = match self {
            Self::Add(a, b) | Self::LessThan(a, b) | Self::EqualI64(a, b) => {
                ([Some(*a), Some(*b)], &[])
            }
            Self::Copy(v)
            | Self::TaskAwait(v)
            | Self::TaskSaveReturn(v)
            | Self::Move(v)
            | Self::Panic(v)
            | Self::DropValue(v)
            | Self::DropValueIfAvailable(v)
            | Self::Initialize { value: v, .. }
            | Self::Replace { value: v, .. }
            | Self::ReplacePrepared { value: v, .. }
            | Self::ArenaEnter { allocator: v, .. }
            | Self::IsCase { value: v, .. } => ([Some(*v), None], &[]),
            Self::CopyCaseField { value, guard, .. }
            | Self::BorrowCaseField { value, guard, .. }
            | Self::UnpackVariant { value, guard, .. } => ([Some(*value), Some(*guard)], &[]),
            Self::Call { arguments, .. }
            | Self::Import { arguments, .. }
            | Self::TaskSpawn { arguments, .. } => ([None, None], arguments),
            Self::CoreCall(call) => ([None, None], &call.arguments),
            Self::Record { fields, .. } => ([None, None], fields),
            Self::Variant { payload, .. } => ([None, None], payload),
            _ => ([None, None], &[]),
        };
        let steps: &[CleanupStep] = match self {
            Self::Cleanup(cleanup) => &cleanup.steps,
            _ => &[],
        };
        fixed
            .into_iter()
            .flatten()
            .chain(variable.iter().copied())
            .chain(steps.iter().filter_map(|step| match step.target {
                CleanupTarget::Value(value) => Some(value),
                _ => None,
            }))
    }
    pub fn ended_loans(&self, span: Span) -> impl Iterator<Item = (LoanId, EndMode, Span)> + '_ {
        let (direct, steps): (Option<_>, &[CleanupStep]) = match self {
            Self::EndBorrow { loan, mode } => (Some((*loan, *mode, span)), &[]),
            Self::Cleanup(cleanup) => (None, &cleanup.steps),
            _ => (None, &[]),
        };
        direct
            .into_iter()
            .chain(steps.iter().filter_map(|step| match step.target {
                CleanupTarget::Loan(loan) => Some((loan, EndMode::IfActive, step.span)),
                _ => None,
            }))
    }
    pub const fn place(&self) -> Option<p::PlaceId> {
        match self {
            Self::CopyPlace(p)
            | Self::TaskCancel(p)
            | Self::MovePlace(p)
            | Self::Replace { place: p, .. }
            | Self::PreparePlace { place: p, .. }
            | Self::Borrow { place: p, .. } => Some(*p),
            _ => None,
        }
    }
    pub const fn created_loan(&self) -> Option<LoanId> {
        match self {
            Self::Borrow { loan, .. } | Self::BorrowCaseField { loan, .. } => Some(*loan),
            Self::CoreCall(call) => call.result_loan,
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Instruction {
    pub results: Vec<m::Register>,
    pub op: Op,
    pub span: Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Terminator {
    pub kind: Term,
    pub span: Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub parameters: Vec<m::Register>,
    pub instructions: Vec<Instruction>,
    pub terminator: Terminator,
    pub span: Span,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FunctionBody {
    pub task_scopes: Vec<TaskScope>,
    pub task_bindings: Vec<TaskBinding>,
    pub loans: usize,
    pub prepared: usize,
    pub blocks: Vec<Block>,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BodyTable {
    pub places: p::PlaceTable,
    pub functions: Vec<FunctionBody>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub loans: usize,
    pub prepared: usize,
    pub places: p::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            loans: 1_000_000,
            prepared: 1_000_000,
            places: p::Limits::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Site {
    pub block: m::BlockId,
    pub order: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedPlace {
    pub site: Site,
    pub place: p::PlaceId,
    pub span: Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionFacts {
    definitions: Vec<Option<Site>>,
    loans: Vec<Option<Site>>,
    prepared: Vec<Option<PreparedPlace>>,
    idom: Vec<u32>,
    core_calls: Vec<core::Fact>,
    needs_allocator: bool,
}
impl FunctionFacts {
    pub fn definitions(&self) -> &[Option<Site>] {
        &self.definitions
    }
    pub fn loans(&self) -> &[Option<Site>] {
        &self.loans
    }
    pub fn prepared(&self) -> &[Option<PreparedPlace>] {
        &self.prepared
    }
    pub fn immediate_dominators(&self) -> &[u32] {
        &self.idom
    }
    pub fn core_calls(&self) -> &[core::Fact] {
        &self.core_calls
    }
    /// Transitive guest-call requirement, including recursive calls. Runtime
    /// entry preflight must supply authority before publishing guest effects.
    pub const fn needs_instance_allocator(&self) -> bool {
        self.needs_allocator
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedBodies {
    has_panic: bool,
    places: p::VerifiedPlaces,
    functions: Vec<FunctionBody>,
    facts: Vec<FunctionFacts>,
    work: usize,
    bytes: usize,
    elements: usize,
}
impl TypedBodies {
    pub(crate) fn record_usage(&mut self, budget: &Budget) {
        self.work = budget.work_used();
        self.bytes = budget.allocation_bytes();
        self.elements = budget.elements_used();
    }
    pub const fn has_explicit_panic(&self) -> bool {
        self.has_panic
    }
    pub fn places(&self) -> &p::VerifiedPlaces {
        &self.places
    }
    pub fn functions(&self) -> &[FunctionBody] {
        &self.functions
    }
    pub fn facts(&self) -> &[FunctionFacts] {
        &self.facts
    }
    pub const fn work_used(&self) -> usize {
        self.work
    }
    pub const fn requested_bytes(&self) -> usize {
        self.bytes
    }
    pub const fn elements_used(&self) -> usize {
        self.elements
    }
    pub fn into_table(self) -> BodyTable {
        BodyTable {
            places: self.places.into_table(),
            functions: self.functions,
        }
    }
}
impl BodyTable {
    pub fn verify(self, limits: Limits) -> Result<TypedBodies, Error> {
        self.verify_with_budget(
            &mut Budget::new(limits.places.ownership.unit),
            limits.places.ownership.constructed,
            limits.places.places,
            limits.loans,
            limits.prepared,
        )
    }
    pub fn verify_with_budget(
        self,
        budget: &mut Budget,
        constructed_limit: usize,
        place_limit: usize,
        loan_limit: usize,
        prepared_limit: usize,
    ) -> Result<TypedBodies, Error> {
        self.verify_profile(
            budget,
            constructed_limit,
            place_limit,
            loan_limit,
            prepared_limit,
            Profile::Synchronous,
        )
    }
    pub(crate) fn verify_profile(
        self,
        budget: &mut Budget,
        constructed_limit: usize,
        place_limit: usize,
        loan_limit: usize,
        prepared_limit: usize,
        profile: Profile,
    ) -> Result<TypedBodies, Error> {
        if self.functions.len() != self.places.functions.len() {
            return Err(Error::new(
                ErrorKind::FunctionBodyCount {
                    expected: self.places.functions.len(),
                    actual: self.functions.len(),
                },
                None,
            ));
        }
        let places = self
            .places
            .verify_profile(budget, constructed_limit, place_limit, profile)?;
        let mut facts = budget.vector(self.functions.len())?;
        let mut has_panic = false;
        let (mut blocks, mut instructions, mut loans, mut prepared) =
            (0usize, 0usize, 0usize, 0usize);
        for (f, body) in self.functions.iter().enumerate() {
            if profile == Profile::Synchronous
                && (!body.task_scopes.is_empty() || !body.task_bindings.is_empty())
            {
                return Err(Error::new(
                    ErrorKind::UnsupportedTaskProfile,
                    Some(places.declarations().functions()[f].span),
                ));
            }
            blocks = add(blocks, body.blocks.len())?;
            cap(blocks, budget.limits().blocks, m::LimitKind::Blocks)?;
            loans = add(loans, body.loans)?;
            if loans > loan_limit {
                return Err(Error::new(
                    ErrorKind::LoanLimit {
                        requested: loans,
                        limit: loan_limit,
                    },
                    Some(places.declarations().functions()[f].span),
                ));
            }
            prepared = add(prepared, body.prepared)?;
            if prepared > prepared_limit {
                return Err(Error::new(
                    ErrorKind::PreparedLimit {
                        requested: prepared,
                        limit: prepared_limit,
                    },
                    Some(places.declarations().functions()[f].span),
                ));
            }
            for b in &body.blocks {
                budget.charge(1)?;
                instructions = add(instructions, b.instructions.len())?;
                for i in &b.instructions {
                    budget.charge(1)?;
                    has_panic |= matches!(i.op, Op::Panic(_));
                }
            }
            cap(
                instructions,
                budget.limits().instructions,
                m::LimitKind::Instructions,
            )?;
            facts.push(function(&places, f, body, budget, profile)?);
        }
        allocator_requirements(&self.functions, &mut facts, budget)?;
        Ok(TypedBodies {
            has_panic,
            places,
            functions: self.functions,
            facts,
            work: budget.work_used(),
            bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        })
    }
}
fn add(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_add(b)
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow).into())
}
fn fail(kind: m::ErrorKind, span: Span) -> Error {
    Error::new(ErrorKind::Mir(kind), Some(span))
}
fn within(
    types: &VerifiedTypes,
    owner: Span,
    span: Span,
    budget: &mut Budget,
) -> Result<(), Error> {
    at(
        validation::source_span(&types.table.sources, Some(span), budget),
        span,
    )?;
    if owner.source != span.source || span.start < owner.start || span.end > owner.end {
        Err(Error::new(ErrorKind::SpanOutsideOwner, Some(span)))
    } else {
        Ok(())
    }
}
fn ctor(types: &VerifiedTypes, t: Type) -> Option<Constructor> {
    if let Type::Constructed(id) = t {
        types
            .table
            .constructed
            .get(id.0 as usize)
            .map(|t| t.constructor)
    } else {
        None
    }
}
#[derive(Clone, Copy)]
enum Expected {
    Exact(Type),
    Builtin(Builtin),
}
impl Expected {
    fn check(self, types: &VerifiedTypes, actual: Type, span: Span) -> Result<(), Error> {
        match self {
            Self::Exact(expected) if types.equivalent(expected, actual) != Some(true) => Err(
                Error::new(ErrorKind::ValueType { expected, actual }, Some(span)),
            ),
            Self::Builtin(expected)
                if ctor(types, actual) != Some(Constructor::Builtin(expected)) =>
            {
                Err(Error::new(
                    ErrorKind::ExpectedBuiltin { expected, actual },
                    Some(span),
                ))
            }
            _ => Ok(()),
        }
    }
}
enum Payload<'a> {
    Empty,
    One(Expected),
    User(&'a [TypeUse]),
}
impl Payload<'_> {
    fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::One(_) => 1,
            Self::User(v) => v.len(),
        }
    }
    fn get(&self, index: usize) -> Option<Expected> {
        match self {
            Self::Empty => None,
            Self::One(t) => (index == 0).then_some(*t),
            Self::User(v) => v.get(index).map(|t| Expected::Exact(t.ty)),
        }
    }
}
fn variant(types: &VerifiedTypes, ty: Type, shared: bool) -> Result<Type, ErrorKind> {
    let ty = if shared {
        match ctor(types, ty) {
            Some(Constructor::Loan(LoanKind::Shared, t)) => t,
            _ => ty,
        }
    } else {
        ty
    };
    let valid = match ty {
        Type::Nominal(d) => matches!(
            types.table.definitions[d.0 as usize].kind,
            DefinitionKind::Enum(_)
        ),
        _ => matches!(
            ctor(types, ty),
            Some(
                Constructor::Option(_)
                    | Constructor::Result(..)
                    | Constructor::Builtin(
                        Builtin::AllocError
                            | Builtin::BoundsError
                            | Builtin::BufferError
                            | Builtin::Utf8Error
                            | Builtin::OwnershipError
                            | Builtin::ArenaError
                            | Builtin::TaskError
                    )
            )
        ),
    };
    if valid {
        Ok(ty)
    } else {
        Err(ErrorKind::ExpectedVariant(ty))
    }
}
fn payload(types: &VerifiedTypes, ty: Type, case: u32) -> Result<Payload<'_>, ErrorKind> {
    let invalid = || ErrorKind::InvalidCase { ty, case };
    if let Type::Nominal(id) = ty
        && let DefinitionKind::Enum(cases) = &types.table.definitions[id.0 as usize].kind
    {
        return cases
            .get(case as usize)
            .map(|c| Payload::User(&c.payload))
            .ok_or_else(invalid);
    }
    Ok(match ctor(types, ty) {
        Some(Constructor::Option(t)) => match case {
            0 => Payload::Empty,
            1 => Payload::One(Expected::Exact(t)),
            _ => return Err(invalid()),
        },
        Some(Constructor::Result(t, e)) => match case {
            0 => Payload::One(Expected::Exact(t)),
            1 => Payload::One(Expected::Exact(e)),
            _ => return Err(invalid()),
        },
        Some(Constructor::Builtin(b)) => {
            let count = match b {
                Builtin::AllocError | Builtin::BufferError => 3,
                Builtin::BoundsError | Builtin::Utf8Error | Builtin::TaskError => 1,
                Builtin::OwnershipError => 6,
                Builtin::ArenaError => 4,
                _ => 0,
            };
            if case >= count {
                return Err(invalid());
            }
            if b == Builtin::Utf8Error {
                Payload::One(Expected::Exact(Type::I64))
            } else if (b == Builtin::BufferError && case == 2)
                || (matches!(b, Builtin::OwnershipError | Builtin::ArenaError) && case == 0)
            {
                Payload::One(Expected::Builtin(Builtin::AllocError))
            } else {
                Payload::Empty
            }
        }
        _ => return Err(ErrorKind::ExpectedVariant(ty)),
    })
}
struct Check<'a, 'b> {
    profile: Profile,
    places: &'a p::VerifiedPlaces,
    types: &'a VerifiedTypes,
    frame: &'a p::FunctionPlaces,
    declaration: &'a d::Function,
    prepared: &'a [Option<PreparedPlace>],
    function: FunctionId,
    budget: &'b mut Budget,
    core_calls: Vec<core::Fact>,
    needs_allocator: bool,
}
impl Check<'_, '_> {
    fn value(&mut self, v: m::Register, span: Span) -> Result<Type, Error> {
        at(self.budget.charge(1), span)?;
        self.frame
            .values
            .get(v.0 as usize)
            .map(|t| t.ty)
            .ok_or_else(|| Error::new(ErrorKind::InvalidValue(v), Some(span)))
    }
    fn expect(&mut self, v: m::Register, t: Expected, span: Span) -> Result<(), Error> {
        let actual = self.value(v, span)?;
        t.check(self.types, actual, span)
    }
    fn arity(&self, actual: usize, expected: usize, span: Span) -> Result<(), Error> {
        if actual == expected {
            Ok(())
        } else {
            Err(fail(m::ErrorKind::Arity { expected, actual }, span))
        }
    }
    fn one(&mut self, i: &Instruction, t: Expected) -> Result<(), Error> {
        self.arity(i.results.len(), 1, i.span)?;
        self.expect(i.results[0], t, i.span)
    }
    fn void(&self, i: &Instruction) -> Result<(), Error> {
        self.arity(i.results.len(), 0, i.span)
    }
    fn local(&self, l: d::LocalId, span: Span) -> Result<&d::Local, Error> {
        self.declaration.locals.get(l.0 as usize).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidLocal {
                    function: self.function,
                    local: l.0,
                },
                Some(span),
            )
        })
    }
    fn authority(&self, ty: Type) -> bool {
        matches!(
            ctor(self.types, ty),
            Some(Constructor::ArenaOwner(_) | Constructor::Builtin(Builtin::Allocator))
        )
    }
    fn borrow_result(
        &mut self,
        i: &Instruction,
        kind: LoanKind,
        referent: Expected,
    ) -> Result<(), Error> {
        self.arity(i.results.len(), 1, i.span)?;
        let actual = self.value(i.results[0], i.span)?;
        let Some(Constructor::Loan(k, t)) = ctor(self.types, actual) else {
            return Err(Error::new(
                ErrorKind::LoanResult { kind, actual },
                Some(i.span),
            ));
        };
        if k != kind {
            return Err(Error::new(
                ErrorKind::LoanResult { kind, actual },
                Some(i.span),
            ));
        }
        referent.check(self.types, t, i.span)
    }
    fn place(&self, p: p::PlaceId, span: Span) -> Result<&p::Facts, Error> {
        self.places
            .facts(self.function, p)
            .ok_or_else(|| Error::new(ErrorKind::InvalidPlace(p.0), Some(span)))
    }
    fn access(&mut self, p: p::PlaceId, a: p::Access, span: Span) -> Result<Type, Error> {
        self.places
            .check_access(self.function, p, a, span, self.budget)?;
        Ok(self.place(p, span)?.value_type)
    }
    fn destination(&self, id: PreparedPlaceId, span: Span) -> Result<PreparedPlace, Error> {
        self.prepared
            .get(id.0 as usize)
            .ok_or_else(|| Error::new(ErrorKind::InvalidPrepared(id.0), Some(span)))?
            .ok_or_else(|| Error::new(ErrorKind::UndefinedPrepared(id.0), Some(span)))
    }
    fn operation(&mut self, i: &Instruction, site: Site) -> Result<(), Error> {
        if i.op.is_task()
            && (self.profile != Profile::Tasks
                || !self.places.declarations().functions()[self.function.0 as usize]
                    .effect
                    .is_async())
        {
            return Err(Error::new(ErrorKind::UnsupportedTaskProfile, Some(i.span)));
        }
        let s = i.span;
        use Expected::{Builtin as B, Exact as E};
        match &i.op {
            Op::TaskScopeEnter(_) | Op::TaskScopeExit(_) | Op::TaskYield | Op::TaskCheck => {
                self.void(i)
            }
            Op::TaskCancel(place) => {
                let actual = self.access(*place, p::Access::Read, s)?;
                if !matches!(ctor(self.types, actual), Some(Constructor::Task(_))) {
                    return Err(Error::new(ErrorKind::InvalidTaskType(actual), Some(s)));
                }
                self.void(i)
            }
            Op::TaskAwait(value) => {
                let actual = self.value(*value, s)?;
                let Some(Constructor::Task(payload)) = ctor(self.types, actual) else {
                    return Err(Error::new(ErrorKind::InvalidTaskType(actual), Some(s)));
                };
                self.arity(i.results.len(), 1, s)?;
                let result = self.value(i.results[0], s)?;
                let Some(Constructor::Result(ok, error)) = ctor(self.types, result) else {
                    return Err(Error::new(ErrorKind::InvalidTaskType(result), Some(s)));
                };
                E(payload).check(self.types, ok, s)?;
                B(Builtin::TaskError).check(self.types, error, s)?;
                Ok(())
            }
            Op::TaskSaveReturn(value) => {
                self.expect(
                    *value,
                    E(
                        self.places.declarations().functions()[self.function.0 as usize]
                            .result
                            .ty,
                    ),
                    s,
                )?;
                self.void(i)
            }
            Op::TaskSpawn {
                function,
                arguments,
            } => {
                cap(
                    arguments.len(),
                    self.budget.limits().fields,
                    m::LimitKind::Fields,
                )?;
                let mut types = self.budget.vector(arguments.len())?;
                for value in arguments {
                    types.push(self.value(*value, s)?);
                }
                let payload =
                    self.places
                        .declarations()
                        .check_spawn(*function, &types, s, self.budget)?;
                self.arity(i.results.len(), 1, s)?;
                let result = self.value(i.results[0], s)?;
                let Some(Constructor::Task(actual)) = ctor(self.types, result) else {
                    return Err(Error::new(ErrorKind::InvalidTaskType(result), Some(s)));
                };
                E(payload).check(self.types, actual, s)
            }
            Op::Cleanup(cleanup) => {
                self.void(i)?;
                self.budget.allocate::<CleanupStep>(cleanup.steps.len())?;
                // End registrations first, regardless of untrusted list order.
                for loans in [true, false] {
                    for step in &cleanup.steps {
                        self.budget.charge(1)?;
                        if matches!(step.target, CleanupTarget::Loan(_)) == loans {
                            at(
                                validation::source_span(
                                    &self.types.table.sources,
                                    Some(step.span),
                                    self.budget,
                                ),
                                step.span,
                            )?;
                            self.operation(&step.instruction(), site)?;
                        }
                    }
                }
                Ok(())
            }
            Op::Panic(code) => {
                self.expect(*code, E(Type::I64), s)?;
                self.void(i)
            }
            Op::I64(_) => self.one(i, E(Type::I64)),
            Op::Bool(_) => self.one(i, E(Type::Bool)),
            Op::Unit => self.one(i, B(Builtin::Unit)),
            Op::Add(a, b) | Op::LessThan(a, b) | Op::EqualI64(a, b) => {
                self.expect(*a, E(Type::I64), s)?;
                self.expect(*b, E(Type::I64), s)?;
                self.one(
                    i,
                    E(if matches!(i.op, Op::Add(..)) {
                        Type::I64
                    } else {
                        Type::Bool
                    }),
                )
            }
            Op::Copy(v) | Op::Move(v) => {
                let ty = self.value(*v, s)?;
                if matches!(i.op, Op::Copy(_)) && !self.types.properties(ty).unwrap().is_copy {
                    return Err(Error::new(ErrorKind::NonCopyPlace(ty), Some(s)));
                }
                if self.authority(ty) {
                    return Err(Error::new(ErrorKind::MoveAuthority(ty), Some(s)));
                }
                self.one(i, E(ty))
            }
            Op::CopyPlace(p) | Op::MovePlace(p) => {
                let ty = self.access(
                    *p,
                    if matches!(i.op, Op::CopyPlace(_)) {
                        p::Access::Copy
                    } else {
                        p::Access::Move
                    },
                    s,
                )?;
                self.one(i, E(ty))
            }
            Op::Initialize { local, value } => {
                let l = self.local(*local, s)?;
                if !matches!(l.kind, d::LocalKind::Mutable | d::LocalKind::Pattern) {
                    return Err(Error::new(
                        ErrorKind::InvalidInitialization(local.0),
                        Some(s),
                    ));
                }
                let ty = l.ty.ty;
                self.expect(*value, E(ty), s)?;
                self.void(i)
            }
            Op::Replace { place, value } => {
                let ty = self.value(*value, s)?;
                self.access(*place, p::Access::Replace(ty), s)?;
                self.void(i)
            }
            Op::PreparePlace { place, .. } => {
                let ty = self.place(*place, s)?.value_type;
                self.access(*place, p::Access::Replace(ty), s)?;
                self.void(i)
            }
            Op::ReplacePrepared { destination, value } => {
                let captured = self.destination(*destination, s)?;
                let ty = self.value(*value, s)?;
                self.access(captured.place, p::Access::Replace(ty), s)?;
                self.void(i)
            }
            Op::DiscardPrepared(id) => {
                self.destination(*id, s)?;
                self.void(i)
            }
            Op::DropValue(v) | Op::DropValueIfAvailable(v) => {
                let ty = self.value(*v, s)?;
                if self.authority(ty) {
                    return Err(Error::new(ErrorKind::MoveAuthority(ty), Some(s)));
                }
                self.void(i)
            }
            Op::DropLocal(l) | Op::DropLocalIfInitialized(l) => {
                let ty = self.local(*l, s)?.ty.ty;
                if self.authority(ty) {
                    return Err(Error::new(ErrorKind::MoveAuthority(ty), Some(s)));
                }
                self.void(i)
            }
            Op::Borrow { kind, place, .. } => {
                let ty = self.access(*place, p::Access::Borrow(*kind), s)?;
                self.borrow_result(i, *kind, E(ty))
            }
            Op::EndBorrow { .. } => self.void(i),
            Op::CoreCall(call) => {
                self.arity(i.results.len(), 1, s)?;
                let result = self.value(i.results[0], s)?;
                let mut arguments = self.budget.vector(call.arguments.len())?;
                for value in &call.arguments {
                    arguments.push(self.value(*value, s)?);
                }
                let checked = core::check(
                    self.places.declarations(),
                    self.function,
                    call,
                    &arguments,
                    result,
                    s,
                    self.budget,
                )?;
                self.needs_allocator |=
                    checked.result_binding == core::ResultBinding::InstanceAllocator;
                self.core_calls.push(core::Fact {
                    site,
                    operation: call.operation,
                    checked,
                });
                Ok(())
            }
            Op::ArenaEnter { owner, allocator } => {
                if self.local(*owner, s)?.kind != d::LocalKind::ArenaOwner {
                    return Err(Error::new(
                        ErrorKind::InvalidInitialization(owner.0),
                        Some(s),
                    ));
                }
                let ty = self.value(*allocator, s)?;
                let Some(Constructor::Loan(_, t)) = ctor(self.types, ty) else {
                    return Err(Error::new(
                        ErrorKind::ExpectedBuiltin {
                            expected: Builtin::Allocator,
                            actual: ty,
                        },
                        Some(s),
                    ));
                };
                B(Builtin::Allocator).check(self.types, t, s)?;
                self.void(i)
            }
            Op::ArenaExit { owner } => {
                if self.local(*owner, s)?.kind != d::LocalKind::ArenaOwner {
                    return Err(Error::new(
                        ErrorKind::InvalidInitialization(owner.0),
                        Some(s),
                    ));
                }
                self.void(i)
            }
            Op::Call {
                function: FunctionId(function),
                arguments,
            }
            | Op::Import {
                import: m::ImportId(function),
                arguments,
            } => {
                cap(
                    arguments.len(),
                    self.budget.limits().fields,
                    m::LimitKind::Fields,
                )?;
                let mut types = self.budget.vector(arguments.len())?;
                for v in arguments {
                    types.push(self.value(*v, s)?);
                }
                let ty = if matches!(i.op, Op::Call { .. }) {
                    self.places.declarations().check_call(
                        FunctionId(*function),
                        &types,
                        s,
                        self.budget,
                    )?
                } else {
                    self.places.declarations().check_import(
                        m::ImportId(*function),
                        &types,
                        s,
                        self.budget,
                    )?
                };
                self.one(i, E(ty))
            }
            Op::Record {
                definition,
                fields,
                initialization_order,
            } => {
                let d = self
                    .types
                    .table
                    .definitions
                    .get(definition.0 as usize)
                    .ok_or_else(|| {
                        Error::new(ErrorKind::InvalidType(Type::Nominal(*definition)), Some(s))
                    })?;
                let DefinitionKind::Record(expected) = &d.kind else {
                    return Err(Error::new(
                        ErrorKind::ExpectedRecord(Type::Nominal(*definition)),
                        Some(s),
                    ));
                };
                self.arity(fields.len(), expected.len(), s)?;
                self.arity(initialization_order.len(), fields.len(), s)?;
                let mut seen = self.budget.vector(fields.len())?;
                seen.resize(fields.len(), false);
                for &field in initialization_order {
                    self.budget.charge(1)?;
                    let Some(seen) = seen.get_mut(field as usize) else {
                        return Err(Error::new(
                            ErrorKind::InvalidInitializationOrder(field),
                            Some(s),
                        ));
                    };
                    if std::mem::replace(seen, true) {
                        return Err(Error::new(
                            ErrorKind::InvalidInitializationOrder(field),
                            Some(s),
                        ));
                    }
                }
                for (v, t) in fields.iter().zip(expected) {
                    self.expect(*v, E(t.ty.ty), s)?;
                }
                self.one(i, E(Type::Nominal(*definition)))
            }
            Op::Variant {
                case,
                payload: values,
            } => {
                self.arity(i.results.len(), 1, s)?;
                let ty = self.value(i.results[0], s)?;
                let ty = variant(self.types, ty, false).map_err(|k| Error::new(k, Some(s)))?;
                let expected =
                    payload(self.types, ty, *case).map_err(|k| Error::new(k, Some(s)))?;
                self.arity(values.len(), expected.len(), s)?;
                for (j, v) in values.iter().enumerate() {
                    self.expect(*v, expected.get(j).unwrap(), s)?;
                }
                Ok(())
            }
            Op::IsCase { value, case } => {
                let actual = self.value(*value, s)?;
                let ty = variant(self.types, actual, true).map_err(|k| Error::new(k, Some(s)))?;
                payload(self.types, ty, *case).map_err(|k| Error::new(k, Some(s)))?;
                self.one(i, E(Type::Bool))
            }
            Op::CopyCaseField {
                value,
                case,
                field,
                guard,
            }
            | Op::BorrowCaseField {
                value,
                case,
                field,
                guard,
                ..
            } => {
                let actual = self.value(*value, s)?;
                let ty = variant(self.types, actual, true).map_err(|k| Error::new(k, Some(s)))?;
                let fields = payload(self.types, ty, *case).map_err(|k| Error::new(k, Some(s)))?;
                let expected = fields.get(*field as usize).ok_or_else(|| {
                    Error::new(
                        ErrorKind::InvalidPayloadField {
                            ty,
                            case: *case,
                            field: *field,
                        },
                        Some(s),
                    )
                })?;
                self.expect(*guard, E(Type::Bool), s)?;
                if matches!(i.op, Op::BorrowCaseField { .. }) {
                    self.borrow_result(i, LoanKind::Shared, expected)
                } else {
                    self.one(i, expected)?;
                    let t = self.value(i.results[0], s)?;
                    if !self.types.properties(t).unwrap().is_copy {
                        return Err(Error::new(ErrorKind::NonCopyPlace(t), Some(s)));
                    }
                    Ok(())
                }
            }
            Op::UnpackVariant { value, case, guard } => {
                let ty = self.value(*value, s)?;
                if matches!(ctor(self.types, ty), Some(Constructor::Loan(..))) {
                    return Err(Error::new(ErrorKind::ExpectedOwnedVariant(ty), Some(s)));
                }
                let ty = variant(self.types, ty, false).map_err(|k| Error::new(k, Some(s)))?;
                let fields = payload(self.types, ty, *case).map_err(|k| Error::new(k, Some(s)))?;
                self.expect(*guard, E(Type::Bool), s)?;
                self.arity(i.results.len(), fields.len(), s)?;
                for (j, v) in i.results.iter().enumerate() {
                    self.expect(*v, fields.get(j).unwrap(), s)?;
                }
                Ok(())
            }
        }
    }
}

fn define(sites: &mut [Option<Site>], v: m::Register, site: Site, span: Span) -> Result<(), Error> {
    let slot = sites
        .get_mut(v.0 as usize)
        .ok_or_else(|| Error::new(ErrorKind::InvalidValue(v), Some(span)))?;
    if slot.is_some() {
        return Err(fail(m::ErrorKind::DuplicateRegister(v.0), span));
    }
    *slot = Some(site);
    Ok(())
}
fn dominates(
    idom: &[u32],
    ancestor: u32,
    mut block: u32,
    budget: &mut Budget,
) -> Result<bool, Error> {
    loop {
        budget.charge(1)?;
        if ancestor == block {
            return Ok(true);
        }
        if block == 0 {
            return Ok(false);
        }
        block = idom[block as usize];
    }
}
fn use_value(
    sites: &[Option<Site>],
    idom: &[u32],
    v: m::Register,
    at_site: Site,
    span: Span,
    budget: &mut Budget,
) -> Result<(), Error> {
    at(budget.charge(1), span)?;
    let Some(site) = sites.get(v.0 as usize).copied().flatten() else {
        return Err(fail(m::ErrorKind::UndefinedRegister(v.0), span));
    };
    if (site.block == at_site.block && site.order >= at_site.order)
        || (site.block != at_site.block && !dominates(idom, site.block.0, at_site.block.0, budget)?)
    {
        return Err(fail(m::ErrorKind::DoesNotDominate(v.0), span));
    }
    Ok(())
}
struct Predecessors<'a> {
    starts: &'a [usize],
    blocks: &'a [u32],
}
fn case_guard(
    body: &FunctionBody,
    sites: &[Option<Site>],
    idom: &[u32],
    predecessors: Predecessors<'_>,
    block: usize,
    i: &Instruction,
    budget: &mut Budget,
) -> Result<(), Error> {
    let (value, case, guard) = match i.op {
        Op::CopyCaseField {
            value, case, guard, ..
        }
        | Op::BorrowCaseField {
            value, case, guard, ..
        }
        | Op::UnpackVariant { value, case, guard } => (value, case, guard),
        _ => return Ok(()),
    };
    let Some(site) = sites[guard.0 as usize] else {
        return Err(Error::new(ErrorKind::CaseGuard, Some(i.span)));
    };
    if site.order == 0
        || !matches!(body.blocks[site.block.0 as usize].instructions[site.order-1].op,Op::IsCase{value:v,case:c} if v==value&&c==case)
    {
        return Err(Error::new(ErrorKind::CaseGuard, Some(i.span)));
    }
    for (source, b) in body.blocks.iter().enumerate() {
        budget.charge(1)?;
        if let Term::Branch {
            condition,
            then_target,
            else_target,
        } = &b.terminator.kind
        {
            let target = then_target.block.0 as usize;
            if *condition == guard
                && then_target.block != else_target.block
                && predecessors.starts[target + 1] - predecessors.starts[target] == 1
                && predecessors.blocks[predecessors.starts[target]] == source as u32
                && dominates(idom, target as u32, block as u32, budget)?
            {
                return Ok(());
            }
        }
    }
    Err(Error::new(ErrorKind::CaseGuard, Some(i.span)))
}
fn function(
    places: &p::VerifiedPlaces,
    index: usize,
    body: &FunctionBody,
    budget: &mut Budget,
    profile: Profile,
) -> Result<FunctionFacts, Error> {
    let declaration = &places.declarations().functions()[index];
    let span = declaration.span;
    if body.blocks.is_empty() {
        return Err(fail(m::ErrorKind::EmptyFunction, span));
    }
    if !body.blocks[0].parameters.is_empty() {
        return Err(fail(m::ErrorKind::EntryParameters, body.blocks[0].span));
    }
    u32::try_from(body.blocks.len())
        .map_err(|_| m::Error::new(m::ErrorKind::ArithmeticOverflow))?;
    u32::try_from(body.loans).map_err(|_| m::Error::new(m::ErrorKind::ArithmeticOverflow))?;
    u32::try_from(body.prepared).map_err(|_| m::Error::new(m::ErrorKind::ArithmeticOverflow))?;
    let mut prepared = filled(budget, body.prepared, None)?;
    let mut core_count = 0usize;
    for (b, block) in body.blocks.iter().enumerate() {
        for (position, i) in block.instructions.iter().enumerate() {
            budget.charge(1)?;
            if matches!(i.op, Op::CoreCall(_)) {
                core_count = add(core_count, 1)?;
            }
            if let Op::PreparePlace { destination, place } = i.op {
                let slot = prepared.get_mut(destination.0 as usize).ok_or_else(|| {
                    Error::new(ErrorKind::InvalidPrepared(destination.0), Some(i.span))
                })?;
                if slot.is_some() {
                    return Err(Error::new(
                        ErrorKind::DuplicatePrepared(destination.0),
                        Some(i.span),
                    ));
                }
                *slot = Some(PreparedPlace {
                    site: Site {
                        block: m::BlockId(b as u32),
                        order: position + 1,
                    },
                    place,
                    span: i.span,
                });
            }
        }
    }
    let mut c = Check {
        profile,
        places,
        types: places.declarations().types(),
        frame: &places.functions()[index],
        declaration,
        prepared: &prepared,
        function: FunctionId(index as u32),
        budget,
        core_calls: Vec::new(),
        needs_allocator: false,
    };
    c.core_calls = c.budget.vector(core_count)?;
    let mut definitions = filled(c.budget, c.frame.values.len(), None)?;
    let mut loans = filled(c.budget, body.loans, None)?;
    let mut starts = filled(c.budget, body.blocks.len() + 1, 0usize)?;
    for (b, block) in body.blocks.iter().enumerate() {
        within(c.types, span, block.span, c.budget)?;
        for v in &block.parameters {
            c.value(*v, block.span)?;
            define(
                &mut definitions,
                *v,
                Site {
                    block: m::BlockId(b as u32),
                    order: 0,
                },
                block.span,
            )?;
        }
        for (position, i) in block.instructions.iter().enumerate() {
            within(c.types, block.span, i.span, c.budget)?;
            let site = Site {
                block: m::BlockId(b as u32),
                order: position + 1,
            };
            if matches!(i.op, Op::Panic(_))
                && (position + 1 != block.instructions.len()
                    || !matches!(block.terminator.kind, Term::Unreachable))
            {
                return Err(Error::new(
                    ErrorKind::InvalidPanicContinuation,
                    Some(i.span),
                ));
            }
            c.operation(i, site)?;
            for v in &i.results {
                c.value(*v, i.span)?;
                define(&mut definitions, *v, site, i.span)?;
            }
            if let Some(id) = i.op.created_loan() {
                let slot = loans
                    .get_mut(id.0 as usize)
                    .ok_or_else(|| Error::new(ErrorKind::InvalidLoan(id.0), Some(i.span)))?;
                if slot.is_some() {
                    return Err(Error::new(ErrorKind::DuplicateLoan(id.0), Some(i.span)));
                }
                *slot = Some(site);
            }
        }
        within(c.types, block.span, block.terminator.span, c.budget)?;
        match block.terminator.kind {
            Term::ReturnSaved => {
                if profile != Profile::Tasks || !declaration.effect.is_async() {
                    return Err(Error::new(
                        ErrorKind::UnsupportedTaskProfile,
                        Some(block.terminator.span),
                    ));
                }
            }
            Term::Return(v) => c.expect(
                v,
                Expected::Exact(declaration.result.ty),
                block.terminator.span,
            )?,
            Term::Branch { condition, .. } => c.expect(
                condition,
                Expected::Exact(Type::Bool),
                block.terminator.span,
            )?,
            Term::Jump(_) | Term::Unreachable => {}
        }
        for target in block.terminator.kind.targets() {
            at(c.budget.charge(1), block.terminator.span)?;
            if target.block.0 == 0 {
                return Err(fail(m::ErrorKind::EntryPredecessor, block.terminator.span));
            }
            let destination = body.blocks.get(target.block.0 as usize).ok_or_else(|| {
                fail(
                    m::ErrorKind::InvalidBlock(target.block.0),
                    block.terminator.span,
                )
            })?;
            c.arity(
                target.arguments.len(),
                destination.parameters.len(),
                block.terminator.span,
            )?;
            for (a, p) in target.arguments.iter().zip(&destination.parameters) {
                let ty = c.value(*p, block.terminator.span)?;
                c.expect(*a, Expected::Exact(ty), block.terminator.span)?;
            }
            starts[target.block.0 as usize + 1] = add(starts[target.block.0 as usize + 1], 1)?;
        }
    }
    for i in 1..starts.len() {
        c.budget.charge(1)?;
        starts[i] = add(starts[i], starts[i - 1])?;
    }
    let mut predecessors = filled(c.budget, *starts.last().unwrap(), 0u32)?;
    let mut cursors = c.budget.vector(body.blocks.len())?;
    for start in &starts[..body.blocks.len()] {
        c.budget.charge(1)?;
        cursors.push(*start);
    }
    for (b, block) in body.blocks.iter().enumerate() {
        for target in block.terminator.kind.targets() {
            c.budget.charge(1)?;
            let cursor = &mut cursors[target.block.0 as usize];
            predecessors[*cursor] = b as u32;
            *cursor += 1;
        }
    }
    let idom = at(
        validation::dominators(
            body.blocks.len(),
            |b, next| {
                body.blocks[b]
                    .terminator
                    .kind
                    .targets()
                    .nth(next)
                    .map(|t| t.block.0)
            },
            &starts,
            &predecessors,
            c.budget,
        ),
        span,
    )?;
    for (b, block) in body.blocks.iter().enumerate() {
        for (position, i) in block.instructions.iter().enumerate() {
            let here = Site {
                block: m::BlockId(b as u32),
                order: position + 1,
            };
            for v in i.op.operands() {
                use_value(&definitions, &idom, v, here, i.span, c.budget)?;
                if provisional(body, &definitions, v, c.budget)? && !provisional_use(&i.op, v) {
                    return Err(Error::new(ErrorKind::CoreProvisionalUse(v), Some(i.span)));
                }
            }
            if let Some(p) = i.op.place()
                && let p::Root::Value(v) = c.frame.places[p.0 as usize].root
            {
                use_value(&definitions, &idom, v, here, i.span, c.budget)?;
                if provisional(body, &definitions, v, c.budget)? {
                    return Err(Error::new(ErrorKind::CoreProvisionalUse(v), Some(i.span)));
                }
            }
            for (loan, mode, span) in i.op.ended_loans(i.span) {
                let Some(site) = loans.get(loan.0 as usize).copied().flatten() else {
                    return Err(Error::new(ErrorKind::UndefinedLoan(loan.0), Some(span)));
                };
                if mode == EndMode::Required
                    && ((site.block == here.block && site.order >= here.order)
                        || (site.block != here.block
                            && !dominates(&idom, site.block.0, here.block.0, c.budget)?))
                {
                    return Err(Error::new(ErrorKind::UndefinedLoan(loan.0), Some(span)));
                }
            }
            if let Op::ReplacePrepared { destination, .. } | Op::DiscardPrepared(destination) = i.op
            {
                let site = c.destination(destination, i.span)?.site;
                if (site.block == here.block && site.order >= here.order)
                    || (site.block != here.block
                        && !dominates(&idom, site.block.0, here.block.0, c.budget)?)
                {
                    return Err(Error::new(
                        ErrorKind::PreparedDoesNotDominate(destination.0),
                        Some(i.span),
                    ));
                }
            }
            case_guard(
                body,
                &definitions,
                &idom,
                Predecessors {
                    starts: &starts,
                    blocks: &predecessors,
                },
                b,
                i,
                c.budget,
            )?;
        }
        for v in block.terminator.kind.operands() {
            use_value(
                &definitions,
                &idom,
                v,
                Site {
                    block: m::BlockId(b as u32),
                    order: block.instructions.len() + 1,
                },
                block.terminator.span,
                c.budget,
            )?;
            if provisional(body, &definitions, v, c.budget)? {
                return Err(Error::new(
                    ErrorKind::CoreProvisionalUse(v),
                    Some(block.terminator.span),
                ));
            }
        }
    }
    let core_calls = c.core_calls;
    let needs_allocator = c.needs_allocator;
    Ok(FunctionFacts {
        definitions,
        loans,
        prepared,
        idom,
        core_calls,
        needs_allocator,
    })
}

fn provisional(
    body: &FunctionBody,
    definitions: &[Option<Site>],
    value: m::Register,
    budget: &mut Budget,
) -> Result<bool, Error> {
    budget.charge(1)?;
    let Some(site) = definitions[value.0 as usize] else {
        return Ok(false);
    };
    if site.order == 0 {
        return Ok(false);
    }
    Ok(
        matches!(&body.blocks[site.block.0 as usize].instructions[site.order - 1].op,
        Op::CoreCall(call) if matches!(call.operation.signature().result_binding, core::ResultBinding::ProvisionalLoan { .. })),
    )
}
fn provisional_use(op: &Op, value: m::Register) -> bool {
    match op {
        Op::Cleanup(_) => true, // Every register operand here is a conditional discard.
        Op::IsCase { value: v, .. }
        | Op::CopyCaseField { value: v, .. }
        | Op::BorrowCaseField { value: v, .. }
        | Op::UnpackVariant { value: v, .. }
        | Op::DropValue(v)
        | Op::DropValueIfAvailable(v) => *v == value,
        _ => false,
    }
}
fn allocator_requirements(
    bodies: &[FunctionBody],
    facts: &mut [FunctionFacts],
    budget: &mut Budget,
) -> Result<(), Error> {
    loop {
        let mut changed = false;
        for (index, body) in bodies.iter().enumerate() {
            budget.charge(1)?;
            if facts[index].needs_allocator {
                continue;
            }
            'blocks: for block in &body.blocks {
                for i in &block.instructions {
                    budget.charge(1)?;
                    if let Op::Call { function, .. } | Op::TaskSpawn { function, .. } = i.op
                        && facts[function.0 as usize].needs_allocator
                    {
                        facts[index].needs_allocator = true;
                        changed = true;
                        break 'blocks;
                    }
                }
            }
        }
        if !changed {
            return Ok(());
        }
    }
}
