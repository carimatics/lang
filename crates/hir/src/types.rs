//! Bounded RFC 0007 type properties over actual resolved source HIR.
//! This pass proves type/storage/Copy facts, not body move/loan/region flow,
//! runtime layout, an ABI, or a fully typed/executable program.

use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

use crate::{resolved as r, source::Name};
use lang_diagnostics::{
    Diagnostic, DiagnosticCode, Label, Severity, SourceError, SourceMap, Span, codes,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub constructed: usize,
    pub definitions: usize,
    pub functions: usize,
    pub modules: usize,
    pub members: usize,
    pub depth: usize,
    pub work: usize,
    /// Cumulative requested explicit analysis buffers, excluding caller-owned
    /// HIR/source snapshots, the fixed emergency limit error, and the
    /// allocator's own bookkeeping/fragmentation.
    pub allocations: usize,
    pub diagnostics: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            constructed: 65_536,
            definitions: 4096,
            functions: 4096,
            modules: 256,
            members: 256,
            depth: 64,
            work: 10_000_000,
            allocations: 128 * 1024 * 1024,
            diagnostics: 4096,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InputError {
    Source(SourceError),
    MissingRoot,
    InvalidModule(usize),
    InvalidDefinition(usize),
    InvalidConstructed(usize),
    ForwardConstructed(usize),
    InvalidParameter(usize),
    SpanOutsideOwner,
    NameTextMismatch,
    CompilerOnlyType(usize),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    Input {
        kind: InputError,
        span: Option<Span>,
    },
    Limit(Box<Diagnostic>),
    AllocationFailed {
        span: Span,
    },
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "source type analysis failed: {self:?}")
    }
}
impl StdError for Error {}
fn input(kind: InputError, span: Option<Span>) -> Error {
    Error::Input { kind, span }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Properties {
    pub is_copy: bool,
    pub requires_drop: bool,
    pub contains_loan: bool,
    pub contains_arena: bool,
    pub contains_allocator: bool,
    pub contains_task: bool,
    pub(crate) non_send: bool,
    pub(crate) non_share: bool,
}
impl Properties {
    /// Logical-task capability derived from the checked graph, independently
    /// of storage/region facts and of the other capability (RFCs 0024/0025).
    pub const fn is_send(self) -> bool {
        !self.non_send
    }
    pub const fn is_share(self) -> bool {
        !self.non_share
    }
    pub(crate) fn result_restriction(self) -> Option<(DiagnosticCode, &'static str)> {
        if self.contains_task {
            Some((
                codes::INVALID_TASK,
                "Task claims cannot be returned from a function",
            ))
        } else if self.contains_loan || self.contains_arena {
            Some((
                codes::REGION_ESCAPE,
                "ordinary loans and arena-bearing values cannot be returned",
            ))
        } else if self.contains_allocator {
            Some((
                codes::INVALID_OWNERSHIP_TYPE,
                "allocator authority is usable only through a loan",
            ))
        } else {
            None
        }
    }
    fn bits(bits: u16) -> Self {
        Self {
            is_copy: bits & NONCOPY == 0,
            requires_drop: bits & DROP != 0,
            contains_loan: bits & LOAN != 0,
            contains_arena: bits & ARENA != 0,
            contains_allocator: bits & ALLOCATOR != 0,
            contains_task: bits & TASK != 0,
            non_send: bits & NONSEND != 0,
            non_share: bits & NONSHARE != 0,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Facts {
    definitions: Vec<Properties>,
    constructed: Vec<Properties>,
    work: usize,
    allocations: usize,
}
impl Facts {
    pub(crate) fn into_parts(self) -> (Vec<Properties>, Vec<Properties>, usize, usize) {
        (
            self.definitions,
            self.constructed,
            self.work,
            self.allocations,
        )
    }
    pub fn get(&self, ty: r::Type) -> Option<Properties> {
        match ty {
            r::Type::I64 | r::Type::Bool => Some(Properties::bits(0)),
            r::Type::Nominal(id) => self.definitions.get(id.0).copied(),
            r::Type::Constructed(id) => self.constructed.get(id.0).copied(),
        }
    }
    pub fn definitions(&self) -> &[Properties] {
        &self.definitions
    }
    pub fn constructed(&self) -> &[Properties] {
        &self.constructed
    }
    pub const fn work_used(&self) -> usize {
        self.work
    }
    pub const fn allocation_bytes(&self) -> usize {
        self.allocations
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Analysis {
    pub facts: Option<Facts>,
    pub diagnostics: Vec<Diagnostic>,
}

const NONCOPY: u16 = 1;
const DROP: u16 = 2;
const LOAN: u16 = 4;
const ARENA: u16 = 8;
const ALLOCATOR: u16 = 16;
const PRIVATE: u16 = 32;
const TASK: u16 = 64;
const NONSEND: u16 = 128;
const NONSHARE: u16 = 256;
const CAPABILITIES: u16 = NONSEND | NONSHARE;
const INSPECT: u16 = CAPABILITIES | LOAN | ARENA | ALLOCATOR | PRIVATE | TASK;

// Capability dependencies need not coincide with physical referents.
fn capability_delta(constructor: Option<r::TypeConstructor>, delta: u16) -> u16 {
    let denied = match constructor {
        Some(r::TypeConstructor::Shared(_) | r::TypeConstructor::Weak(_)) => {
            delta & CAPABILITIES != 0
        }
        Some(
            r::TypeConstructor::Sender(_)
            | r::TypeConstructor::Receiver(_)
            | r::TypeConstructor::ChannelSetup(_),
        ) => delta & NONSEND != 0,
        _ => return delta & CAPABILITIES,
    };
    if denied { CAPABILITIES } else { 0 }
}

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
pub(crate) struct Budget {
    pub(crate) limits: Limits,
    pub(crate) work: usize,
    pub(crate) allocations: usize,
}
impl Budget {
    pub(crate) fn limit(&self, span: Span, message: &str) -> Error {
        Error::Limit(Box::new(Diagnostic::new(
            codes::RESOURCE_LIMIT,
            Severity::Error,
            message,
            Label::new(span, message),
        )))
    }
    pub(crate) fn charge(&mut self, span: Span, n: usize) -> Result<(), Error> {
        self.work = self
            .work
            .checked_add(n)
            .filter(|n| *n <= self.limits.work)
            .ok_or_else(|| self.limit(span, "source type work limit exceeded"))?;
        Ok(())
    }
    fn allocate<T>(&mut self, span: Span, n: usize) -> Result<(), Error> {
        self.allocations = n
            .checked_mul(std::mem::size_of::<T>())
            .and_then(|n| self.allocations.checked_add(n))
            .filter(|n| *n <= self.limits.allocations)
            .ok_or_else(|| self.limit(span, "source type allocation limit exceeded"))?;
        Ok(())
    }
    pub(crate) fn vector<T>(&mut self, span: Span, n: usize) -> Result<Vec<T>, Error> {
        self.allocate::<T>(span, n)?;
        let mut v = Vec::new();
        v.try_reserve_exact(n)
            .map_err(|_| Error::AllocationFailed { span })?;
        Ok(v)
    }
    fn filled<T: Clone>(&mut self, span: Span, n: usize, value: T) -> Result<Vec<T>, Error> {
        self.charge(span, n)?;
        let mut v = self.vector(span, n)?;
        v.resize(n, value);
        Ok(v)
    }
    pub(crate) fn text(&mut self, span: Span, value: &str) -> Result<String, Error> {
        self.charge(span, value.len())?;
        self.allocate::<u8>(span, value.len())?;
        let mut text = String::new();
        text.try_reserve_exact(value.len())
            .map_err(|_| Error::AllocationFailed { span })?;
        text.push_str(value);
        Ok(text)
    }
    pub(crate) fn push<T>(&mut self, span: Span, v: &mut Vec<T>, value: T) -> Result<(), Error> {
        self.charge(span, 1)?;
        if v.len() == v.capacity() {
            let next = v
                .capacity()
                .max(2)
                .checked_mul(2)
                .ok_or_else(|| self.limit(span, "source type buffer overflow"))?;
            self.allocate::<T>(span, next)?;
            v.try_reserve_exact(next - v.len())
                .map_err(|_| Error::AllocationFailed { span })?;
        }
        v.push(value);
        Ok(())
    }
}
struct Context<'a> {
    program: &'a r::Program,
    sources: &'a SourceMap,
    budget: Budget,
    diagnostics: Vec<Diagnostic>,
    root: Span,
}
impl Context<'_> {
    fn span(&self, span: Span) -> Result<(), Error> {
        self.sources
            .slice(span)
            .map(|_| ())
            .map_err(|e| input(InputError::Source(e), Some(span)))
    }
    fn within(&self, outer: Span, inner: Span) -> Result<(), Error> {
        self.span(inner)?;
        if outer.source() != inner.source()
            || outer.range().start > inner.range().start
            || outer.range().end < inner.range().end
        {
            return Err(input(InputError::SpanOutsideOwner, Some(inner)));
        }
        Ok(())
    }
    fn owner(&mut self, module: r::ModuleId, span: Span, name: &Name) -> Result<(), Error> {
        let parent = self
            .program
            .modules
            .get(module.0)
            .ok_or_else(|| input(InputError::InvalidModule(module.0), Some(span)))?;
        self.within(parent.span, span)?;
        self.name(span, name)
    }
    fn name(&mut self, owner: Span, name: &Name) -> Result<(), Error> {
        self.within(owner, name.span)?;
        self.budget.charge(name.span, name.text.len() + 1)?;
        if self.sources.slice(name.span).unwrap() != name.text {
            return Err(input(InputError::NameTextMismatch, Some(name.span)));
        }
        Ok(())
    }
    fn node(&self, ty: r::Type, span: Span) -> Result<usize, Error> {
        match ty {
            r::Type::I64 => Ok(0),
            r::Type::Bool => Ok(1),
            r::Type::Nominal(id) if id.0 < self.program.definitions.len() => Ok(2 + id.0),
            r::Type::Nominal(id) => Err(input(InputError::InvalidDefinition(id.0), Some(span))),
            r::Type::Constructed(id) if id.0 < self.program.types.len() => {
                Ok(2 + self.program.definitions.len() + id.0)
            }
            r::Type::Constructed(id) => {
                Err(input(InputError::InvalidConstructed(id.0), Some(span)))
            }
        }
    }
    fn use_type(&mut self, owner: Span, ty: r::TypeUse) -> Result<usize, Error> {
        self.within(owner, ty.span)?;
        self.budget.charge(ty.span, 1)?;
        self.node(ty.value_type, ty.span)
    }
    fn report(
        &mut self,
        code: DiagnosticCode,
        span: Span,
        message: &str,
        related: Option<Span>,
    ) -> Result<(), Error> {
        if self.diagnostics.len() >= self.budget.limits.diagnostics {
            return Err(self
                .budget
                .limit(span, "source type diagnostic limit exceeded"));
        }
        let message_text = self.budget.text(span, message)?;
        let primary = Label::new(span, self.budget.text(span, message)?);
        let mut d = Diagnostic::new(code, Severity::Error, message_text, primary);
        if let Some(span) = related {
            let label = Label::new(span, self.budget.text(span, "related declaration")?);
            self.budget.push(span, &mut d.secondary, label)?;
        }
        self.budget.push(span, &mut self.diagnostics, d)
    }
    fn count(&self, span: Span, count: usize) -> Result<(), Error> {
        if count > self.budget.limits.members {
            return Err(self.budget.limit(span, "source type member limit exceeded"));
        }
        Ok(())
    }
    fn unique(&mut self, owner: Span, names: &[&Name]) -> Result<(), Error> {
        for (i, name) in names.iter().enumerate() {
            self.name(owner, name)?;
            for previous in &names[..i] {
                self.budget.charge(
                    name.span,
                    name.text
                        .len()
                        .saturating_add(previous.text.len())
                        .saturating_add(1),
                )?;
                if name.text == previous.text {
                    self.report(
                        codes::INVALID_SHAPE,
                        name.span,
                        "duplicate type member name",
                        Some(previous.span),
                    )?;
                    break;
                }
            }
        }
        Ok(())
    }
    fn edge(
        &mut self,
        parent: usize,
        owner: Span,
        ty: r::TypeUse,
        inline: bool,
        graph: &mut [Vec<Edge>],
        reverse: &mut [Vec<Reverse>],
    ) -> Result<(), Error> {
        let child = self.use_type(owner, ty)?;
        self.budget.push(
            ty.span,
            &mut graph[parent],
            Edge {
                child,
                inline,
                span: ty.span,
            },
        )?;
        self.budget
            .push(ty.span, &mut reverse[child], Reverse { parent, inline })
    }
    fn top(
        &mut self,
        owner: Span,
        ty: r::TypeUse,
        position: Position,
        public: bool,
        bits: &[u16],
        private: &[Option<Span>],
    ) -> Result<(), Error> {
        let id = self.use_type(owner, ty)?;
        let flags = bits[id];
        if public && flags & PRIVATE != 0 {
            self.report(
                codes::MODULE_ERROR,
                ty.span,
                "public signature contains a private type",
                private[id],
            )?;
        }
        let outer_loan = matches!(ty.value_type, r::Type::Constructed(id) if matches!(self.program.types[id.0].constructor, r::TypeConstructor::Loan(..)));
        if flags & TASK != 0 && matches!(position, Position::Field | Position::Parameter) {
            self.report(
                codes::INVALID_TASK,
                ty.span,
                "Task claims are permitted only in async locals",
                None,
            )?;
        } else if position == Position::Result
            && let Some((code, message)) = Properties::bits(flags).result_restriction()
        {
            self.report(code, ty.span, message, None)?;
        } else if flags & ALLOCATOR != 0 && (!outer_loan || position == Position::Field) {
            self.report(
                codes::INVALID_OWNERSHIP_TYPE,
                ty.span,
                "allocator authority is usable only through a loan",
                None,
            )?;
        } else if position == Position::Field && flags & LOAN != 0 {
            self.report(
                codes::INVALID_OWNERSHIP_TYPE,
                ty.span,
                "ordinary loans cannot be stored in fields",
                None,
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Position {
    Field,
    Parameter,
    Local,
    Result,
}

pub fn analyze(
    program: &r::Program,
    sources: &SourceMap,
    limits: Limits,
) -> Result<Analysis, Error> {
    let root = program
        .modules
        .first()
        .ok_or_else(|| input(InputError::MissingRoot, None))?
        .span;
    let mut c = Context {
        program,
        sources,
        budget: Budget {
            limits,
            work: 0,
            allocations: 0,
        },
        diagnostics: Vec::new(),
        root,
    };
    c.span(root)?;
    if program.types.len() > limits.constructed
        || program.definitions.len() > limits.definitions
        || program.functions.len() > limits.functions
        || program.modules.len() > limits.modules
    {
        return Err(c
            .budget
            .limit(root, "source type declaration limit exceeded"));
    }
    for module in &program.modules {
        c.span(module.span)?;
        c.budget.charge(module.span, 1)?;
    }
    let n = 2usize
        .checked_add(program.definitions.len())
        .and_then(|n| n.checked_add(program.types.len()))
        .ok_or_else(|| c.budget.limit(root, "source type node overflow"))?;
    let base = 2 + program.definitions.len();
    let mut graph = c.budget.vector(root, n)?;
    c.budget.charge(root, n)?;
    graph.resize_with(n, Vec::new);
    let mut reverse = c.budget.vector(root, n)?;
    c.budget.charge(root, n)?;
    reverse.resize_with(n, Vec::new);
    let mut bits = c.budget.filled(root, n, 0u16)?;
    let mut private = c.budget.filled(root, n, None)?;
    let mut generic_depth: Vec<usize> = c.budget.vector(root, program.types.len())?;
    for (index, ty) in program.types.iter().enumerate() {
        c.span(ty.span)?;
        if matches!(ty.constructor, r::TypeConstructor::ArenaOwner(_)) {
            return Err(input(InputError::CompilerOnlyType(index), Some(ty.span)));
        }
        let mut found = false;
        for module in &program.modules {
            c.budget.charge(ty.span, 1)?;
            if module.span.source() == ty.span.source() && c.within(module.span, ty.span).is_ok() {
                found = true;
                break;
            }
        }
        if !found {
            return Err(input(InputError::SpanOutsideOwner, Some(ty.span)));
        }
        let mut depth = 1usize;
        for argument in ty.constructor.arguments() {
            c.node(argument, ty.span)?;
            if let r::Type::Constructed(id) = argument {
                if id.0 >= index {
                    return Err(input(InputError::ForwardConstructed(id.0), Some(ty.span)));
                }
                depth = depth.max(
                    generic_depth[id.0]
                        .checked_add(1)
                        .ok_or_else(|| c.budget.limit(ty.span, "source type depth overflow"))?,
                );
            } else {
                depth = depth.max(2);
            }
            c.edge(
                base + index,
                ty.span,
                r::TypeUse {
                    value_type: argument,
                    span: ty.span,
                },
                matches!(
                    ty.constructor,
                    r::TypeConstructor::Option(_) | r::TypeConstructor::Result(..)
                ),
                &mut graph,
                &mut reverse,
            )?;
        }
        if depth > limits.depth {
            return Err(c
                .budget
                .limit(ty.span, "constructed type depth limit exceeded"));
        }
        generic_depth.push(depth);
        bits[base + index] = match ty.constructor {
            r::TypeConstructor::Builtin(r::BuiltinType::Text | r::BuiltinType::Bytes) => {
                NONCOPY | DROP
            }
            r::TypeConstructor::Builtin(r::BuiltinType::Allocator) => {
                NONCOPY | ALLOCATOR | CAPABILITIES
            }
            r::TypeConstructor::Vec(_)
            | r::TypeConstructor::Map(..)
            | r::TypeConstructor::Shared(_)
            | r::TypeConstructor::Weak(_) => NONCOPY | DROP,
            r::TypeConstructor::Loan(..) => NONCOPY | LOAN | CAPABILITIES,
            r::TypeConstructor::Task(_) => NONCOPY | DROP | TASK | CAPABILITIES,
            r::TypeConstructor::ChannelSetup(_) => NONCOPY | DROP | CAPABILITIES,
            r::TypeConstructor::Sender(_) => NONCOPY | DROP,
            r::TypeConstructor::Receiver(_) => NONCOPY | DROP | NONSHARE,
            r::TypeConstructor::Arena(_) => ARENA | CAPABILITIES,
            _ => 0,
        };
    }
    for (index, definition) in program.definitions.iter().enumerate() {
        c.owner(definition.module, definition.span, &definition.name)?;
        c.budget
            .charge(definition.span, definition.qualified_name.len() + 1)?;
        if !definition.public {
            bits[2 + index] |= PRIVATE;
            private[2 + index] = Some(definition.name.span);
        }
        match &definition.kind {
            r::DefinitionKind::Record(fields) => {
                c.count(definition.span, fields.len())?;
                let mut names = c.budget.vector(definition.span, fields.len())?;
                for field in fields {
                    names.push(&field.name);
                    c.edge(
                        2 + index,
                        definition.span,
                        field.value_type,
                        true,
                        &mut graph,
                        &mut reverse,
                    )?;
                }
                c.unique(definition.span, &names)?;
            }
            r::DefinitionKind::Enum(cases) => {
                c.count(definition.span, cases.len())?;
                if cases.is_empty() {
                    c.report(
                        codes::INVALID_SHAPE,
                        definition.span,
                        "enum must have at least one case",
                        None,
                    )?;
                }
                let mut names = c.budget.vector(definition.span, cases.len())?;
                for case in cases {
                    names.push(&case.name);
                    c.count(case.name.span, case.payload.len())?;
                    for ty in &case.payload {
                        c.edge(
                            2 + index,
                            definition.span,
                            *ty,
                            true,
                            &mut graph,
                            &mut reverse,
                        )?;
                    }
                }
                c.unique(definition.span, &names)?;
            }
        }
    }
    check_inline(&mut c, &graph)?;
    // Positive properties propagate through every referent. Copy/drop flow
    // only through inline storage, so Vec<Node> recursion remains finite.
    let mut queue = c.budget.vector(root, n)?;
    for (id, flags) in bits.iter().copied().enumerate() {
        if flags != 0 {
            c.budget.push(root, &mut queue, (id, flags))?;
        }
    }
    let mut head = 0;
    while let Some(&(id, delta)) = queue.get(head) {
        head += 1;
        c.budget.charge(root, 1)?;
        for edge in &reverse[id] {
            c.budget.charge(root, 1)?;
            let constructor = edge
                .parent
                .checked_sub(base)
                .and_then(|i| program.types.get(i))
                .map(|t| t.constructor);
            let storage = delta & if edge.inline { u16::MAX } else { INSPECT };
            let next = ((storage & !CAPABILITIES) | capability_delta(constructor, delta))
                & !bits[edge.parent];
            if next != 0 {
                bits[edge.parent] |= next;
                if next & PRIVATE != 0 {
                    private[edge.parent] = private[id];
                }
                c.budget.push(root, &mut queue, (edge.parent, next))?;
            }
        }
    }
    for ty in &program.types {
        for argument in ty.constructor.arguments() {
            c.budget.charge(ty.span, 1)?;
            if matches!(
                ty.constructor,
                r::TypeConstructor::ChannelSetup(_)
                    | r::TypeConstructor::Sender(_)
                    | r::TypeConstructor::Receiver(_)
            ) && bits[c.node(argument, ty.span)?] & NONSEND != 0
            {
                c.report(
                    codes::INVALID_OWNERSHIP_TYPE,
                    ty.span,
                    "channel message types must be Send",
                    None,
                )?;
            }
            if matches!(ty.constructor, r::TypeConstructor::Task(_))
                && bits[c.node(argument, ty.span)?] & (LOAN | ARENA | ALLOCATOR) != 0
            {
                c.report(
                    codes::INVALID_TASK,
                    ty.span,
                    "Task results cannot contain loans, arenas or allocator authority",
                    None,
                )?;
            }
            if bits[c.node(argument, ty.span)?] & TASK != 0 {
                c.report(
                    codes::INVALID_TASK,
                    ty.span,
                    "Task claims cannot be nested, borrowed or stored in containers",
                    None,
                )?;
            }
        }
        if !matches!(ty.constructor, r::TypeConstructor::Loan(..)) {
            for argument in ty.constructor.arguments() {
                c.budget.charge(ty.span, 1)?;
                let flags = bits[c.node(argument, ty.span)?];
                if flags & (LOAN | ALLOCATOR) != 0 {
                    c.report(codes::INVALID_OWNERSHIP_TYPE, ty.span, "ordinary loans and allocator authority cannot be stored in constructed values", None)?;
                }
                if matches!(
                    ty.constructor,
                    r::TypeConstructor::Shared(_) | r::TypeConstructor::Weak(_)
                ) && flags & ARENA != 0
                {
                    c.report(
                        codes::REGION_ESCAPE,
                        ty.span,
                        "arena-bearing values cannot enter shared or weak storage",
                        None,
                    )?;
                }
            }
        }
        if let r::TypeConstructor::Map(key, _) = ty.constructor {
            let text = matches!(key, r::Type::Constructed(id) if program.types[id.0].constructor == r::TypeConstructor::Builtin(r::BuiltinType::Text));
            if key != r::Type::I64 && !text {
                c.report(
                    codes::UNSUPPORTED_SPECIALIZATION,
                    ty.span,
                    "Map keys must be exactly i64 or core::Text",
                    None,
                )?;
            }
        }
        c.budget.charge(ty.span, 1)?;
    }
    for definition in &program.definitions {
        match &definition.kind {
            r::DefinitionKind::Record(fields) => {
                for field in fields {
                    c.top(
                        definition.span,
                        field.value_type,
                        Position::Field,
                        definition.public,
                        &bits,
                        &private,
                    )?;
                }
            }
            r::DefinitionKind::Enum(cases) => {
                for case in cases {
                    for ty in &case.payload {
                        c.top(
                            definition.span,
                            *ty,
                            Position::Field,
                            definition.public,
                            &bits,
                            &private,
                        )?;
                    }
                }
            }
        }
    }
    for f in &program.functions {
        c.owner(f.module, f.span, &f.name)?;
        c.within(f.span, f.signature_span)?;
        c.top(
            f.span,
            f.result,
            Position::Result,
            f.public,
            &bits,
            &private,
        )?;
        c.count(f.span, f.parameters.len())?;
        for id in &f.parameters {
            c.budget.charge(f.span, 1)?;
            let local = f
                .locals
                .get(id.0)
                .ok_or_else(|| input(InputError::InvalidParameter(id.0), Some(f.span)))?;
            if let Some(ty) = local.annotation {
                c.top(f.span, ty, Position::Parameter, f.public, &bits, &private)?;
            }
        }
        for local in &f.locals {
            c.name(f.span, &local.name)?;
            if let Some(ty) = local.annotation {
                if !f.effect.is_async() && bits[c.node(ty.value_type, ty.span)?] & TASK != 0 {
                    c.report(
                        codes::INVALID_TASK,
                        ty.span,
                        "Task locals require an async function",
                        None,
                    )?;
                }
                c.top(f.span, ty, Position::Local, false, &bits, &private)?;
            }
        }
    }
    let facts = if c.diagnostics.is_empty() {
        let mut definitions = c.budget.vector(root, program.definitions.len())?;
        let mut constructed = c.budget.vector(root, program.types.len())?;
        for flags in &bits[2..base] {
            c.budget.charge(root, 1)?;
            definitions.push(Properties::bits(*flags));
        }
        for flags in &bits[base..] {
            c.budget.charge(root, 1)?;
            constructed.push(Properties::bits(*flags));
        }
        Some(Facts {
            definitions,
            constructed,
            work: c.budget.work,
            allocations: c.budget.allocations,
        })
    } else {
        None
    };
    Ok(Analysis {
        facts,
        diagnostics: c.diagnostics,
    })
}

fn check_inline(c: &mut Context<'_>, graph: &[Vec<Edge>]) -> Result<(), Error> {
    #[derive(Clone, Copy)]
    struct Frame {
        id: usize,
        next: usize,
        child_depth: usize,
    }
    let root = c.root;
    let mut state = c.budget.filled(root, graph.len(), 0u8)?;
    let mut depths = c.budget.filled(root, graph.len(), 0usize)?;
    let mut stack = c
        .budget
        .vector(root, c.budget.limits.depth.min(graph.len()))?;
    for start in 0..graph.len() {
        c.budget.charge(root, 1)?;
        if state[start] != 0 {
            continue;
        }
        if c.budget.limits.depth == 0 {
            return Err(c
                .budget
                .limit(root, "inline source type depth limit exceeded"));
        }
        state[start] = 1;
        stack.push(Frame {
            id: start,
            next: 0,
            child_depth: 0,
        });
        while let Some(frame) = stack.last_mut() {
            c.budget.charge(root, 1)?;
            if let Some(edge) = graph[frame.id].get(frame.next) {
                frame.next += 1;
                if !edge.inline {
                    continue;
                }
                match state[edge.child] {
                    0 => {
                        if stack.len() >= c.budget.limits.depth {
                            return Err(c
                                .budget
                                .limit(edge.span, "inline source type depth limit exceeded"));
                        }
                        state[edge.child] = 1;
                        stack.push(Frame {
                            id: edge.child,
                            next: 0,
                            child_depth: 0,
                        });
                    }
                    1 => c.report(
                        codes::INVALID_SHAPE,
                        edge.span,
                        "recursive inline type cycle; use explicit indirection",
                        None,
                    )?,
                    _ => frame.child_depth = frame.child_depth.max(depths[edge.child]),
                }
            } else {
                let frame = stack.pop().unwrap();
                let depth = frame
                    .child_depth
                    .checked_add(1)
                    .ok_or_else(|| c.budget.limit(root, "inline type depth overflow"))?;
                if depth > c.budget.limits.depth {
                    return Err(c
                        .budget
                        .limit(root, "inline source type depth limit exceeded"));
                }
                depths[frame.id] = depth;
                state[frame.id] = 2;
                if let Some(parent) = stack.last_mut() {
                    parent.child_depth = parent.child_depth.max(depth);
                }
            }
        }
    }
    Ok(())
}
