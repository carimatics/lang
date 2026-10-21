//! In-memory RFC 0004/0007 ownership type graph. This is a type-only contract,
//! not an executable unit, heap layout, source loan proof, or wire format.
//! Logical arena-owner symbols confer no runtime or allocator authority.

use super::{Budget, DefinitionId, FunctionId, Source, Span, validation};
use std::collections::hash_map::DefaultHasher;
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};

pub mod bodies;
pub mod core;
pub mod declarations;
pub mod failure;
mod flow;
pub mod loan_activity;
pub mod loans;
pub mod moves;
pub mod places;
pub mod regions;
pub mod tasks;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstructedId(pub u32);
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Type {
    I64,
    Bool,
    Nominal(DefinitionId),
    Constructed(ConstructedId),
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Builtin {
    Unit,
    Text,
    Bytes,
    Allocator,
    AllocError,
    BoundsError,
    BufferError,
    Utf8Error,
    OwnershipError,
    ArenaError,
    TaskError,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LoanKind {
    Shared,
    Exclusive,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArenaOwner {
    pub function: FunctionId,
    pub local: u32,
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Constructor {
    Builtin(Builtin),
    Option(Type),
    Result(Type, Type),
    Vec(Type),
    Map(Type, Type),
    Shared(Type),
    Weak(Type),
    Arena(Type),
    Loan(LoanKind, Type),
    ArenaOwner(ArenaOwner),
    Task(Type),
}
impl Constructor {
    pub fn arguments(self) -> impl Iterator<Item = Type> {
        let args = match self {
            Self::Option(t)
            | Self::Vec(t)
            | Self::Shared(t)
            | Self::Weak(t)
            | Self::Arena(t)
            | Self::Task(t)
            | Self::Loan(_, t) => [Some(t), None],
            Self::Result(t, e) | Self::Map(t, e) => [Some(t), Some(e)],
            Self::Builtin(_) | Self::ArenaOwner(_) => [None, None],
        };
        args.into_iter().flatten()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Constructed {
    pub constructor: Constructor,
    pub span: Span,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeUse {
    pub ty: Type,
    pub span: Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Field {
    pub name: String,
    pub span: Span,
    pub ty: TypeUse,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Case {
    pub name: String,
    pub span: Span,
    pub payload: Vec<TypeUse>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DefinitionKind {
    Record(Vec<Field>),
    Enum(Vec<Case>),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Definition {
    pub name: String,
    pub public: bool,
    pub span: Span,
    pub kind: DefinitionKind,
}
/// Indices name graph entries. Use verified equivalence to compare types.
/// Constructed arguments precede their parents; nominal indirection may recur.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TypeTable {
    pub sources: Vec<Source>,
    pub definitions: Vec<Definition>,
    pub constructed: Vec<Constructed>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub constructed: usize,
    pub unit: super::Limits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            constructed: 65_536,
            unit: super::Limits::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CProfileLimit {
    Depth,
    Parameters,
    Fields,
    Cases,
    Payloads,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Mir(super::ErrorKind),
    ConstructedLimit {
        requested: usize,
        limit: usize,
    },
    InvalidType(Type),
    ForwardConstructed(ConstructedId),
    SpanOutsideOwner,
    InvalidPanicContinuation,
    EmptyEnum,
    RecursiveInline(Type),
    InvalidStorage(Type),
    InvalidTaskType(Type),
    InvalidAsyncEffect(FunctionId),
    UnsupportedTaskProfile,
    ArenaInShared(Type),
    PrivateInPublic(Type),
    InvalidMapKey(Type),
    InvalidFunction(FunctionId),
    InvalidImport(super::ImportId),
    InvalidLocal {
        function: FunctionId,
        local: u32,
    },
    InvalidParameterList(FunctionId),
    InvalidSignatureType(Type),
    PrivateSignatureType(Type),
    InvalidLocalType(Type),
    InvalidOwner(ArenaOwner),
    InvalidHostSignature,
    InvalidCSignatureType(Type),
    InvalidCExport(FunctionId),
    CProfileLimit {
        kind: CProfileLimit,
        requested: usize,
        limit: usize,
    },
    CallArity {
        expected: usize,
        actual: usize,
    },
    CallType {
        expected: Type,
        actual: Type,
    },
    FunctionBodyCount {
        expected: usize,
        actual: usize,
    },
    PlaceLimit {
        requested: usize,
        limit: usize,
    },
    InvalidValue(super::Register),
    InvalidPlace(u32),
    DuplicatePlace(super::Register),
    InvalidDereference(Type),
    ExpectedRecord(Type),
    InvalidField {
        definition: DefinitionId,
        field: u32,
    },
    PlaceType {
        expected: Type,
        actual: Type,
    },
    ImmutablePlace,
    NonCopyPlace(Type),
    PartialMove(Type),
    MoveThroughLoan(Type),
    MoveAuthority(Type),
    ReplaceAuthority(Type),
    LoanLimit {
        requested: usize,
        limit: usize,
    },
    InvalidLoan(u32),
    DuplicateLoan(u32),
    UndefinedLoan(u32),
    PreparedLimit {
        requested: usize,
        limit: usize,
    },
    InvalidPrepared(u32),
    DuplicatePrepared(u32),
    UndefinedPrepared(u32),
    PreparedDoesNotDominate(u32),
    InvalidInitialization(u32),
    InvalidInitializationOrder(u32),
    CoreTypeArity {
        expected: usize,
        actual: usize,
    },
    CoreTypeArgument {
        index: usize,
        rule: core::TypeArgumentRule,
        actual: Type,
    },
    CoreType {
        operation: core::Operation,
        position: core::Position,
        actual: Type,
    },
    CoreLoan {
        operation: core::Operation,
        expected: bool,
        actual: Option<u32>,
    },
    CoreProvisionalUse(super::Register),
    ExpectedBuiltin {
        expected: Builtin,
        actual: Type,
    },
    ExpectedLoan {
        kind: LoanKind,
        referent: Type,
        actual: Type,
    },
    ExpectedVariant(Type),
    ExpectedOwnedVariant(Type),
    InvalidCase {
        ty: Type,
        case: u32,
    },
    InvalidPayloadField {
        ty: Type,
        case: u32,
        field: u32,
    },
    CaseGuard,
    LoanResult {
        kind: LoanKind,
        actual: Type,
    },
    ValueType {
        expected: Type,
        actual: Type,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub span: Option<Span>,
}
impl Error {
    fn new(kind: ErrorKind, span: Option<Span>) -> Self {
        Self { kind, span }
    }
}
impl From<super::Error> for Error {
    fn from(error: super::Error) -> Self {
        Self::new(ErrorKind::Mir(error.kind), None)
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ownership MIR type validation failed: {:?}", self.kind)
    }
}
impl StdError for Error {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Properties {
    pub is_copy: bool,
    pub requires_drop: bool,
    pub contains_loan: bool,
    pub contains_arena: bool,
    pub contains_allocator: bool,
    pub contains_owner: bool,
    pub contains_task: bool,
}
impl Properties {
    /// RFC 0024 eligibility, derived independently from the verified MIR graph.
    pub const fn is_send(self) -> bool {
        !self.contains_loan
            && !self.contains_arena
            && !self.contains_allocator
            && !self.contains_owner
            && !self.contains_task
    }
    /// Current checked shared operations have the same transitive exclusions.
    /// New capability-bearing constructors require their own accepted policy.
    pub const fn is_share(self) -> bool {
        self.is_send()
    }
    fn from_bits(bits: u8) -> Self {
        Self {
            is_copy: bits & NONCOPY == 0,
            requires_drop: bits & DROP != 0,
            contains_loan: bits & LOAN != 0,
            contains_arena: bits & ARENA != 0,
            contains_allocator: bits & ALLOCATOR != 0,
            contains_owner: bits & OWNER != 0,
            contains_task: bits & TASK != 0,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
/// Proves graph shape, storage constraints, and transitive properties only.
/// Body declarations, signatures, control flow, loans, and allocator authority
/// still require validation before this graph can be part of executable MIR.
pub struct VerifiedTypes {
    table: TypeTable,
    bits: Vec<u8>,
    canonical: Vec<Type>,
    work: usize,
    bytes: usize,
    elements: usize,
}
impl VerifiedTypes {
    /// An unsupported execution profile can be present in unused metadata.
    pub fn task_feature_span(&self) -> Option<Span> {
        self.table.constructed.iter().find_map(|t| {
            matches!(
                t.constructor,
                Constructor::Task(_) | Constructor::Builtin(Builtin::TaskError)
            )
            .then_some(t.span)
        })
    }
    pub fn table(&self) -> &TypeTable {
        &self.table
    }
    pub fn properties(&self, ty: Type) -> Option<Properties> {
        let index = node(&self.table, ty).ok()?;
        self.bits.get(index).copied().map(Properties::from_bits)
    }
    /// First equivalent entry in this table. Nominal types remain distinct;
    /// constructed types compare recursively, including loan kind/owner symbol.
    /// The returned logical reference has meaning only within this table.
    pub fn canonical(&self, ty: Type) -> Option<Type> {
        node(&self.table, ty).ok()?;
        Some(match ty {
            Type::Constructed(id) => self.canonical[id.0 as usize],
            other => other,
        })
    }
    pub fn equivalent(&self, left: Type, right: Type) -> Option<bool> {
        Some(self.canonical(left)? == self.canonical(right)?)
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
    pub fn into_table(self) -> TypeTable {
        self.table
    }
}
impl TypeTable {
    pub fn verify(self, limits: Limits) -> Result<VerifiedTypes, Error> {
        self.verify_with_budget(&mut Budget::new(limits.unit), limits.constructed)
    }
    /// Construction and verification may share the same cumulative budget.
    pub fn verify_with_budget(
        self,
        budget: &mut Budget,
        constructed_limit: usize,
    ) -> Result<VerifiedTypes, Error> {
        let bits = validate(&self, budget, constructed_limit)?;
        let canonical = canonicalize(&self, budget)?;
        Ok(VerifiedTypes {
            table: self,
            bits,
            canonical,
            work: budget.work_used(),
            bytes: budget.allocation_bytes(),
            elements: budget.elements_used(),
        })
    }
}

fn canonicalize(table: &TypeTable, budget: &mut Budget) -> Result<Vec<Type>, Error> {
    let n = table.constructed.len();
    let mut canonical = budget.vector(n)?;
    if n == 0 {
        return Ok(canonical);
    }
    let capacity = n
        .checked_mul(2)
        .and_then(usize::checked_next_power_of_two)
        .ok_or_else(|| super::Error::new(super::ErrorKind::ArithmeticOverflow))?;
    // Fixed fallibly reserved open addressing: hashing allocates nothing, every
    // probe is charged, and equality compares keys rather than hash values.
    // At most n entries occupy at least 2*n slots. No growth or full-table loop.
    let mut slots = filled(budget, capacity, None::<(Constructor, Type)>)?;
    for (i, t) in table.constructed.iter().enumerate() {
        budget.charge(1)?;
        let child = |ty| match ty {
            Type::Constructed(id) => canonical[id.0 as usize],
            other => other,
        };
        let key = match t.constructor {
            Constructor::Option(t) => Constructor::Option(child(t)),
            Constructor::Result(t, e) => Constructor::Result(child(t), child(e)),
            Constructor::Vec(t) => Constructor::Vec(child(t)),
            Constructor::Map(k, v) => Constructor::Map(child(k), child(v)),
            Constructor::Shared(t) => Constructor::Shared(child(t)),
            Constructor::Weak(t) => Constructor::Weak(child(t)),
            Constructor::Arena(t) => Constructor::Arena(child(t)),
            Constructor::Loan(k, t) => Constructor::Loan(k, child(t)),
            Constructor::Task(t) => Constructor::Task(child(t)),
            Constructor::Builtin(_) | Constructor::ArenaOwner(_) => t.constructor,
        };
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let mut slot = (hasher.finish() as usize) & (capacity - 1);
        loop {
            at(budget.charge(1), t.span)?;
            match slots[slot] {
                Some((existing, representative)) if existing == key => {
                    canonical.push(representative);
                    break;
                }
                None => {
                    let representative = Type::Constructed(ConstructedId(i as u32));
                    slots[slot] = Some((key, representative));
                    canonical.push(representative);
                    break;
                }
                _ => slot = (slot + 1) & (capacity - 1),
            }
        }
    }
    Ok(canonical)
}

const NONCOPY: u8 = 1;
const DROP: u8 = 2;
const LOAN: u8 = 4;
const ARENA: u8 = 8;
const ALLOCATOR: u8 = 16;
const OWNER: u8 = 32;
const PRIVATE: u8 = 64;
const TASK: u8 = 128;
const INSPECT: u8 = LOAN | ARENA | ALLOCATOR | OWNER | PRIVATE | TASK;
#[derive(Clone, Copy)]
struct Edge {
    child: usize,
    inline: bool,
    span: Span,
}
#[derive(Clone, Copy)]
struct Reverse {
    parent: usize,
    inline: bool,
}

fn node(table: &TypeTable, ty: Type) -> Result<usize, Error> {
    match ty {
        Type::I64 => Ok(0),
        Type::Bool => Ok(1),
        Type::Nominal(id) if (id.0 as usize) < table.definitions.len() => Ok(2 + id.0 as usize),
        Type::Constructed(id) if (id.0 as usize) < table.constructed.len() => {
            Ok(2 + table.definitions.len() + id.0 as usize)
        }
        _ => Err(Error::new(ErrorKind::InvalidType(ty), None)),
    }
}
fn type_at(table: &TypeTable, index: usize) -> Type {
    match index {
        0 => Type::I64,
        1 => Type::Bool,
        i if i < 2 + table.definitions.len() => Type::Nominal(DefinitionId((i - 2) as u32)),
        i => Type::Constructed(ConstructedId((i - 2 - table.definitions.len()) as u32)),
    }
}
fn at<T>(result: Result<T, super::Error>, span: Span) -> Result<T, Error> {
    result.map_err(|e| Error::new(ErrorKind::Mir(e.kind), Some(span)))
}
fn cap(n: usize, max: usize, kind: super::LimitKind) -> Result<(), Error> {
    if n > max {
        Err(super::Error::new(super::ErrorKind::Limit {
            kind,
            requested: n,
            limit: max,
        })
        .into())
    } else {
        Ok(())
    }
}
fn push<T>(budget: &mut Budget, values: &mut Vec<T>, value: T) -> Result<(), Error> {
    budget.charge(1)?;
    if values.len() == values.capacity() {
        let next = values
            .capacity()
            .max(2)
            .checked_mul(2)
            .ok_or_else(|| super::Error::new(super::ErrorKind::ArithmeticOverflow))?;
        budget.allocate::<T>(next)?;
        values
            .try_reserve_exact(next - values.len())
            .map_err(|_| super::Error::new(super::ErrorKind::AllocationFailed))?;
    }
    values.push(value);
    Ok(())
}
fn filled<T: Clone>(budget: &mut Budget, n: usize, value: T) -> Result<Vec<T>, Error> {
    budget.charge(n)?;
    let mut result = budget.vector(n)?;
    result.resize(n, value);
    Ok(result)
}
struct Graph<'a> {
    table: &'a TypeTable,
    budget: &'a mut Budget,
    edges: Vec<Vec<Edge>>,
    reverse: Vec<Vec<Reverse>>,
}
impl Graph<'_> {
    fn span(&mut self, span: Span) -> Result<(), Error> {
        at(
            validation::source_span(&self.table.sources, Some(span), self.budget),
            span,
        )
    }
    fn within(&mut self, owner: Span, span: Span) -> Result<(), Error> {
        self.span(span)?;
        if owner.source != span.source || owner.start > span.start || owner.end < span.end {
            Err(Error::new(ErrorKind::SpanOutsideOwner, Some(span)))
        } else {
            Ok(())
        }
    }
    fn edge(&mut self, parent: usize, ty: TypeUse, inline: bool) -> Result<(), Error> {
        self.span(ty.span)?;
        let child = node(self.table, ty.ty).map_err(|mut e| {
            e.span = Some(ty.span);
            e
        })?;
        push(
            self.budget,
            &mut self.edges[parent],
            Edge {
                child,
                inline,
                span: ty.span,
            },
        )?;
        push(
            self.budget,
            &mut self.reverse[child],
            Reverse { parent, inline },
        )
    }
    fn names(&mut self, names: &mut [&str], owner: Span) -> Result<(), Error> {
        for name in names.iter() {
            at(validation::name(name, false, self.budget), owner)?;
        }
        at(validation::unique(names, self.budget), owner)
    }
    fn inline_depths(&mut self) -> Result<(), Error> {
        #[derive(Clone, Copy)]
        struct Frame {
            id: usize,
            next: usize,
            depth: usize,
        }
        let n = self.edges.len();
        let limit = self.budget.limits().depth;
        let mut state = filled(self.budget, n, 0u8)?;
        let mut depths = filled(self.budget, n, 0usize)?;
        let mut stack = self.budget.vector::<Frame>(limit.min(n))?;
        for start in 0..n {
            self.budget.charge(1)?;
            if state[start] != 0 {
                continue;
            }
            cap(1, limit, super::LimitKind::Depth)?;
            state[start] = 1;
            stack.push(Frame {
                id: start,
                next: 0,
                depth: 0,
            });
            while let Some(frame) = stack.last_mut() {
                self.budget.charge(1)?;
                if let Some(edge) = self.edges[frame.id].get(frame.next) {
                    frame.next += 1;
                    if !edge.inline {
                        continue;
                    }
                    match state[edge.child] {
                        0 => {
                            cap(stack.len() + 1, limit, super::LimitKind::Depth)?;
                            state[edge.child] = 1;
                            stack.push(Frame {
                                id: edge.child,
                                next: 0,
                                depth: 0,
                            });
                        }
                        1 => {
                            return Err(Error::new(
                                ErrorKind::RecursiveInline(type_at(self.table, edge.child)),
                                Some(edge.span),
                            ));
                        }
                        _ => frame.depth = frame.depth.max(depths[edge.child]),
                    }
                } else {
                    let frame = stack.pop().expect("active traversal");
                    let depth = frame.depth + 1;
                    cap(depth, limit, super::LimitKind::Depth)?;
                    depths[frame.id] = depth;
                    state[frame.id] = 2;
                    if let Some(parent) = stack.last_mut() {
                        parent.depth = parent.depth.max(depth);
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate(
    table: &TypeTable,
    budget: &mut Budget,
    constructed_limit: usize,
) -> Result<Vec<u8>, Error> {
    if table.constructed.len() > constructed_limit {
        return Err(Error::new(
            ErrorKind::ConstructedLimit {
                requested: table.constructed.len(),
                limit: constructed_limit,
            },
            None,
        ));
    }
    cap(
        table.definitions.len(),
        budget.limits().definitions,
        super::LimitKind::Definitions,
    )?;
    u32::try_from(table.definitions.len())
        .map_err(|_| super::Error::new(super::ErrorKind::ArithmeticOverflow))?;
    u32::try_from(table.constructed.len())
        .map_err(|_| super::Error::new(super::ErrorKind::ArithmeticOverflow))?;
    validation::source_metadata(&table.sources, budget)?;
    let base = 2usize
        .checked_add(table.definitions.len())
        .ok_or_else(|| super::Error::new(super::ErrorKind::ArithmeticOverflow))?;
    let n = base
        .checked_add(table.constructed.len())
        .ok_or_else(|| super::Error::new(super::ErrorKind::ArithmeticOverflow))?;
    let mut g = Graph {
        table,
        edges: filled(budget, n, Vec::new())?,
        reverse: filled(budget, n, Vec::new())?,
        budget,
    };
    let mut bits = filled(g.budget, n, 0u8)?;
    let mut depths = g.budget.vector::<usize>(table.constructed.len())?;
    let mut names = g.budget.vector(table.definitions.len())?;
    for (i, definition) in table.definitions.iter().enumerate() {
        g.span(definition.span)?;
        at(
            validation::name(&definition.name, true, g.budget),
            definition.span,
        )?;
        names.push(definition.name.as_str());
        if !definition.public {
            bits[2 + i] = PRIVATE;
        }
        let mut member_names;
        match &definition.kind {
            DefinitionKind::Record(fields) => {
                cap(
                    fields.len(),
                    g.budget.limits().fields,
                    super::LimitKind::Fields,
                )?;
                member_names = g.budget.vector(fields.len())?;
                for field in fields {
                    g.within(definition.span, field.span)?;
                    g.within(definition.span, field.ty.span)?;
                    member_names.push(field.name.as_str());
                    g.edge(2 + i, field.ty, true)?;
                }
            }
            DefinitionKind::Enum(cases) => {
                cap(
                    cases.len(),
                    g.budget.limits().fields,
                    super::LimitKind::Fields,
                )?;
                if cases.is_empty() {
                    return Err(Error::new(ErrorKind::EmptyEnum, Some(definition.span)));
                }
                member_names = g.budget.vector(cases.len())?;
                for case in cases {
                    g.within(definition.span, case.span)?;
                    member_names.push(case.name.as_str());
                    cap(
                        case.payload.len(),
                        g.budget.limits().fields,
                        super::LimitKind::Fields,
                    )?;
                    for ty in &case.payload {
                        g.within(definition.span, ty.span)?;
                        g.edge(2 + i, *ty, true)?;
                    }
                }
            }
        }
        g.names(&mut member_names, definition.span)?;
    }
    validation::unique(&mut names, g.budget)?;
    let mut has_task = false;
    for (i, ty) in table.constructed.iter().enumerate() {
        has_task |= matches!(ty.constructor, Constructor::Task(_));
        g.span(ty.span)?;
        let mut depth = 1;
        for argument in ty.constructor.arguments() {
            g.budget.charge(1)?;
            if let Type::Constructed(id) = argument {
                if id.0 as usize >= i {
                    return Err(Error::new(ErrorKind::ForwardConstructed(id), Some(ty.span)));
                }
                depth = depth.max(depths[id.0 as usize] + 1);
            } else {
                depth = depth.max(2);
            }
            g.edge(
                base + i,
                TypeUse {
                    ty: argument,
                    span: ty.span,
                },
                matches!(
                    ty.constructor,
                    Constructor::Option(_) | Constructor::Result(..)
                ),
            )?;
        }
        cap(depth, g.budget.limits().depth, super::LimitKind::Depth)?;
        depths.push(depth);
        bits[base + i] = match ty.constructor {
            Constructor::Builtin(Builtin::Text | Builtin::Bytes)
            | Constructor::Vec(_)
            | Constructor::Map(..)
            | Constructor::Shared(_)
            | Constructor::Weak(_) => NONCOPY | DROP,
            Constructor::Builtin(Builtin::Allocator) => NONCOPY | ALLOCATOR,
            Constructor::Loan(..) => NONCOPY | LOAN,
            Constructor::Arena(_) => ARENA,
            Constructor::ArenaOwner(_) => NONCOPY | DROP | ARENA | OWNER,
            Constructor::Task(_) => NONCOPY | DROP | TASK,
            _ => 0,
        };
    }
    g.inline_depths()?;
    // Every queued delta introduces at least one of eight previously absent
    // bits at its node, including the initial seeds. No queue growth is needed.
    let queue_capacity = n
        .checked_mul(if has_task { 8 } else { 7 })
        .ok_or_else(|| super::Error::new(super::ErrorKind::ArithmeticOverflow))?;
    let mut queue = g.budget.vector(queue_capacity)?;
    for (id, flags) in bits.iter().copied().enumerate() {
        g.budget.charge(1)?;
        if flags != 0 {
            queue.push((id, flags));
        }
    }
    let mut head = 0;
    while let Some(&(id, delta)) = queue.get(head) {
        head += 1;
        g.budget.charge(1)?;
        for edge in &g.reverse[id] {
            g.budget.charge(1)?;
            let next = delta & if edge.inline { u8::MAX } else { INSPECT } & !bits[edge.parent];
            if next != 0 {
                bits[edge.parent] |= next;
                queue.push((edge.parent, next));
            }
        }
    }
    for ty in &table.constructed {
        g.budget.charge(1)?;
        for argument in ty.constructor.arguments() {
            let flags = bits[node(table, argument)?];
            if flags & TASK != 0
                || (matches!(ty.constructor, Constructor::Task(_))
                    && flags & (LOAN | ARENA | ALLOCATOR | OWNER) != 0)
            {
                return Err(Error::new(
                    ErrorKind::InvalidTaskType(argument),
                    Some(ty.span),
                ));
            }
            let forbidden = match ty.constructor {
                Constructor::Loan(..) => 0,
                // Core read results may contain loans in stack sums. Source
                // annotation spellability is the frontend's separate rule.
                Constructor::Option(_) | Constructor::Result(..) => ALLOCATOR | OWNER,
                _ => LOAN | ALLOCATOR | OWNER,
            };
            if flags & forbidden != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidStorage(argument),
                    Some(ty.span),
                ));
            }
            if matches!(
                ty.constructor,
                Constructor::Shared(_) | Constructor::Weak(_)
            ) && flags & ARENA != 0
            {
                return Err(Error::new(
                    ErrorKind::ArenaInShared(argument),
                    Some(ty.span),
                ));
            }
        }
        if let Constructor::Map(key, _) = ty.constructor {
            let text = matches!(key, Type::Constructed(id) if table.constructed[id.0 as usize].constructor == Constructor::Builtin(Builtin::Text));
            if key != Type::I64 && !text {
                return Err(Error::new(ErrorKind::InvalidMapKey(key), Some(ty.span)));
            }
        }
    }
    for (i, definition) in table.definitions.iter().enumerate() {
        for edge in &g.edges[2 + i] {
            g.budget.charge(1)?;
            if bits[edge.child] & TASK != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidTaskType(type_at(table, edge.child)),
                    Some(definition.span),
                ));
            }
            if bits[edge.child] & (LOAN | ALLOCATOR | OWNER) != 0 {
                return Err(Error::new(
                    ErrorKind::InvalidStorage(type_at(table, edge.child)),
                    Some(edge.span),
                ));
            }
            if definition.public && bits[edge.child] & PRIVATE != 0 {
                return Err(Error::new(
                    ErrorKind::PrivateInPublic(type_at(table, edge.child)),
                    Some(edge.span),
                ));
            }
        }
    }
    Ok(bits)
}
