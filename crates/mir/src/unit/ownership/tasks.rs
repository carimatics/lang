//! Independent task lifetime/escrow proof over ownership MIR. No HIR facts,
//! scheduler grant, physical task layout or execution authority is inferred.
use super::{bodies as b, failure, loan_activity, loans, moves, places as p, *};
use crate::unit as m;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub bodies: b::Limits,
    pub scopes: usize,
    pub bindings: usize,
    pub state_cells: usize,
    pub loan_nodes: usize,
    pub loan_words: usize,
    pub failure_nodes: usize,
    pub regions: failure::RegionLimits,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            bodies: Default::default(),
            scopes: 65_536,
            bindings: 1_000_000,
            state_cells: 16_000_000,
            loan_nodes: 1_000_000,
            loan_words: 8_000_000,
            failure_nodes: 1_000_000,
            regions: Default::default(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Resource(m::ErrorKind),
    Types(super::ErrorKind),
    Moves(moves::ErrorKind),
    Activity(loan_activity::ErrorKind),
    Loans(loans::ErrorKind),
    Failure(failure::ErrorKind),
    Scope(u32),
    Binding(b::CleanupTarget),
    Stack,
    Claim,
    Escape,
    ReturnEscrow,
    UnsupportedTransfer(Type),
    Limit,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub primary: Option<Span>,
    pub secondary: Option<Span>,
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid task MIR: {:?}", self.kind)
    }
}
impl std::error::Error for Error {}
impl From<m::Error> for Error {
    fn from(e: m::Error) -> Self {
        Self {
            kind: ErrorKind::Resource(e.kind),
            primary: None,
            secondary: None,
        }
    }
}
fn fail(kind: ErrorKind, span: Span) -> Error {
    Error {
        kind,
        primary: Some(span),
        secondary: None,
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedTasks {
    plan: failure::Elaborated,
}
impl VerifiedTasks {
    pub fn plan(&self) -> &failure::Elaborated {
        &self.plan
    }
    /// Consume the task proof and recover its checked ownership/failure plan.
    pub fn into_plan(self) -> failure::Elaborated {
        self.plan
    }
    pub fn typed(&self) -> &b::TypedBodies {
        self.plan.source().activity().moved().typed()
    }
}

pub fn verify(table: b::BodyTable, limits: Limits) -> Result<VerifiedTasks, Error> {
    verify_with_usage(table, limits, m::Usage::default())
}

/// Preserve source-adapter work and storage charges across independent MIR proof.
pub fn verify_with_usage(
    table: b::BodyTable,
    limits: Limits,
    usage: m::Usage,
) -> Result<VerifiedTasks, Error> {
    let global = limits.bodies.places.ownership.unit;
    let mut budget = m::Budget::with_usage(global, usage)?;
    let mut typed = table
        .verify_profile(
            &mut budget,
            limits.bodies.places.ownership.constructed,
            limits.bodies.places.places,
            limits.bodies.loans,
            limits.bodies.prepared,
            b::Profile::Tasks,
        )
        .map_err(|e| Error {
            primary: e.span,
            secondary: None,
            kind: ErrorKind::Types(e.kind),
        })?;
    let mut scopes = 0usize;
    let mut bindings = 0usize;
    let mut cells = 0usize;
    for f in 0..typed.functions().len() {
        let span = typed.places().declarations().functions()[f].span;
        let body = &typed.functions()[f];
        scopes = scopes
            .checked_add(body.task_scopes.len())
            .ok_or_else(|| fail(ErrorKind::Limit, span))?;
        bindings = bindings
            .checked_add(body.task_bindings.len())
            .ok_or_else(|| fail(ErrorKind::Limit, span))?;
        let slots = typed.places().declarations().functions()[f]
            .locals
            .len()
            .checked_add(typed.places().functions()[f].values.len())
            .ok_or_else(|| fail(ErrorKind::Limit, span))?;
        cells = cells
            .checked_add(
                slots
                    .checked_mul(body.blocks.len())
                    .ok_or_else(|| fail(ErrorKind::Limit, span))?,
            )
            .ok_or_else(|| fail(ErrorKind::Limit, span))?;
        if scopes > limits.scopes || bindings > limits.bindings || cells > limits.state_cells {
            return Err(fail(ErrorKind::Limit, span));
        }
        let meta = metadata(&typed, f, &mut budget)?;
        analyze(&typed, f, &meta, &mut budget)?;
    }
    typed.record_usage(&budget);
    let moved = moves::analyze(
        typed,
        moves::Limits {
            work: global.work,
            allocations: global.allocations,
            elements: global.elements,
            state_cells: limits.state_cells,
        },
    )
    .map_err(|e| Error {
        primary: e.primary,
        secondary: e.secondary,
        kind: ErrorKind::Moves(e.kind),
    })?;
    let activity = loan_activity::analyze(
        moved,
        loan_activity::Limits {
            work: global.work,
            allocations: global.allocations,
            elements: global.elements,
            state_cells: limits.state_cells,
        },
    )
    .map_err(|e| Error {
        primary: e.primary,
        secondary: e.secondary,
        kind: ErrorKind::Activity(e.kind),
    })?;
    let loaned = loans::analyze(
        activity,
        loans::Limits {
            work: global.work,
            allocations: global.allocations,
            elements: global.elements,
            nodes: limits.loan_nodes,
            depth: global.depth,
            state_words: limits.loan_words,
        },
    )
    .map_err(|e| Error {
        primary: e.primary,
        secondary: e.secondary,
        kind: ErrorKind::Loans(e.kind),
    })?;
    let plan = failure::elaborate(
        loaned,
        failure::Limits {
            work: global.work,
            allocations: global.allocations,
            elements: global.elements,
            nodes: limits.failure_nodes,
            regions: limits.regions,
        },
    )
    .map_err(|e| Error {
        primary: e.primary,
        secondary: None,
        kind: ErrorKind::Failure(e.kind),
    })?;
    Ok(VerifiedTasks { plan })
}

fn append<T>(budget: &mut m::Budget, values: &mut Vec<T>, value: T) -> Result<(), Error> {
    budget.charge(1)?;
    if values.len() == values.capacity() {
        let next = values
            .capacity()
            .max(2)
            .checked_mul(2)
            .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))?;
        budget.allocate::<T>(next)?;
        values
            .try_reserve_exact(next - values.len())
            .map_err(|_| m::Error::new(m::ErrorKind::AllocationFailed))?;
    }
    values.push(value);
    Ok(())
}
fn copy<T: Copy>(budget: &mut m::Budget, values: &[T]) -> Result<Vec<T>, Error> {
    budget.charge(values.len())?;
    let mut out = budget.vector(values.len())?;
    out.extend_from_slice(values);
    Ok(out)
}
struct Metadata {
    scopes: Vec<Option<b::TaskScopeId>>,
    prepared: Vec<Option<p::PlaceId>>,
    locals: usize,
}
fn slot(target: b::CleanupTarget, locals: usize, values: usize) -> Option<usize> {
    match target {
        b::CleanupTarget::Local(id) if (id.0 as usize) < locals => Some(id.0 as usize),
        b::CleanupTarget::Value(id) if (id.0 as usize) < values => {
            locals.checked_add(id.0 as usize)
        }
        _ => None,
    }
}
fn is_task(types: &VerifiedTypes, ty: Type) -> bool {
    matches!(ty,Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,Constructor::Task(_)))
}
fn metadata(typed: &b::TypedBodies, f: usize, budget: &mut m::Budget) -> Result<Metadata, Error> {
    let declarations = typed.places().declarations();
    let types = declarations.types();
    let function = &declarations.functions()[f];
    let body = &typed.functions()[f];
    let values = &typed.places().functions()[f].values;
    let span = function.span;
    if function.effect.is_async() {
        if body.task_scopes.is_empty() {
            return Err(fail(ErrorKind::Scope(0), span));
        }
        for use_ in std::iter::once(function.result).chain(
            function
                .parameters
                .iter()
                .map(|id| function.locals[id.0 as usize].ty),
        ) {
            budget.charge(1)?;
            let p = types.properties(use_.ty).unwrap();
            if !p.is_send() {
                return Err(fail(ErrorKind::UnsupportedTransfer(use_.ty), use_.span));
            }
        }
    } else if !body.task_scopes.is_empty() || !body.task_bindings.is_empty() {
        return Err(fail(ErrorKind::Stack, span));
    }
    let mut depths = budget.vector(body.task_scopes.len())?;
    for (id, scope) in body.task_scopes.iter().enumerate() {
        budget.charge(1)?;
        validation::source_span(&types.table().sources, Some(scope.span), budget)?;
        if scope.span.source != span.source
            || scope.span.start < span.start
            || scope.span.end > span.end
        {
            return Err(fail(ErrorKind::Scope(id as u32), span));
        }
        let depth = match scope.parent {
            None if id == 0 => 1,
            Some(parent) if (parent.0 as usize) < id => {
                let p = &body.task_scopes[parent.0 as usize];
                if scope.span.source != p.span.source
                    || scope.span.start < p.span.start
                    || scope.span.end > p.span.end
                {
                    return Err(fail(ErrorKind::Scope(id as u32), scope.span));
                }
                depths[parent.0 as usize] + 1
            }
            _ => return Err(fail(ErrorKind::Scope(id as u32), scope.span)),
        };
        if depth > budget.limits().depth {
            return Err(fail(ErrorKind::Limit, scope.span));
        }
        depths.push(depth);
    }
    let n = function.locals.len() + values.len();
    let mut scopes = budget.vector(n)?;
    budget.charge(n)?;
    scopes.resize(n, None);
    for binding in &body.task_bindings {
        budget.charge(1)?;
        let index = slot(binding.target, function.locals.len(), values.len())
            .ok_or_else(|| fail(ErrorKind::Binding(binding.target), span))?;
        let ty = if index < function.locals.len() {
            function.locals[index].ty.ty
        } else {
            values[index - function.locals.len()].ty
        };
        if !is_task(types, ty)
            || scopes[index].is_some()
            || (binding.scope.0 as usize) >= body.task_scopes.len()
        {
            return Err(fail(ErrorKind::Binding(binding.target), span));
        }
        scopes[index] = Some(binding.scope);
    }
    for (index, ty) in function
        .locals
        .iter()
        .map(|l| l.ty.ty)
        .chain(values.iter().map(|v| v.ty))
        .enumerate()
    {
        budget.charge(1)?;
        if is_task(types, ty) != scopes[index].is_some() {
            return Err(fail(ErrorKind::Claim, span));
        }
    }
    let mut prepared = budget.vector(body.prepared)?;
    prepared.resize(body.prepared, None);
    for block in &body.blocks {
        for i in &block.instructions {
            budget.charge(1)?;
            match i.op {
                b::Op::PreparePlace { destination, place } => {
                    prepared[destination.0 as usize] = Some(place)
                }
                b::Op::TaskScopeEnter(id) | b::Op::TaskScopeExit(id)
                    if (id.0 as usize) >= body.task_scopes.len()
                        || matches!(i.op, b::Op::TaskScopeEnter(b::TaskScopeId(0))) =>
                {
                    return Err(fail(ErrorKind::Scope(id.0), i.span));
                }
                _ => {}
            }
        }
    }
    Ok(Metadata {
        scopes,
        prepared,
        locals: function.locals.len(),
    })
}

struct State {
    stack: Vec<b::TaskScopeId>,
    origins: Vec<Vec<b::TaskScopeId>>,
    saved: bool,
}
impl State {
    fn new(slots: usize, asynchronous: bool, budget: &mut m::Budget) -> Result<Self, Error> {
        let mut origins = budget.vector(slots)?;
        budget.charge(slots)?;
        origins.resize_with(slots, Vec::new);
        let mut stack = Vec::new();
        if asynchronous {
            append(budget, &mut stack, b::TaskScopeId(0))?;
        }
        Ok(Self {
            stack,
            origins,
            saved: false,
        })
    }
    fn copy(&self, budget: &mut m::Budget) -> Result<Self, Error> {
        let stack = copy(budget, &self.stack)?;
        let mut origins = budget.vector(self.origins.len())?;
        for origin in &self.origins {
            origins.push(copy(budget, origin)?);
        }
        Ok(Self {
            stack,
            origins,
            saved: self.saved,
        })
    }
    fn join(&mut self, other: &Self, span: Span, budget: &mut m::Budget) -> Result<bool, Error> {
        budget.charge(self.stack.len().saturating_add(self.origins.len()))?;
        if self.stack != other.stack || self.saved != other.saved {
            return Err(fail(ErrorKind::Stack, span));
        }
        let mut changed = false;
        for (to, from) in self.origins.iter_mut().zip(&other.origins) {
            for origin in from {
                budget.charge(to.len().saturating_add(1))?;
                if let Err(index) = to.binary_search(origin) {
                    append(budget, to, *origin)?;
                    to[index..].rotate_right(1);
                    changed = true;
                }
            }
        }
        Ok(changed)
    }
}
struct Check<'a, 'b> {
    typed: &'a b::TypedBodies,
    function: usize,
    meta: &'a Metadata,
    budget: &'b mut m::Budget,
}
impl Check<'_, '_> {
    fn place(&self, id: p::PlaceId) -> usize {
        match self.typed.places().functions()[self.function].places[id.0 as usize].root {
            p::Root::Local(id) => id.0 as usize,
            p::Root::Value(id) => self.meta.locals + id.0 as usize,
        }
    }
    fn value(&self, id: m::Register) -> usize {
        self.meta.locals + id.0 as usize
    }
    fn read(
        &mut self,
        state: &State,
        index: usize,
        span: Span,
        check: bool,
    ) -> Result<Vec<b::TaskScopeId>, Error> {
        if check && self.meta.scopes[index].is_some() && state.origins[index].is_empty() {
            return Err(fail(ErrorKind::Claim, span));
        }
        copy(self.budget, &state.origins[index])
    }
    fn take(
        &mut self,
        state: &mut State,
        index: usize,
        span: Span,
        check: bool,
    ) -> Result<Vec<b::TaskScopeId>, Error> {
        let origins = self.read(state, index, span, check)?;
        state.origins[index].clear();
        Ok(origins)
    }
    fn assign(
        &mut self,
        state: &mut State,
        index: usize,
        origins: Vec<b::TaskScopeId>,
        span: Span,
    ) -> Result<(), Error> {
        let Some(scope) = self.meta.scopes[index] else {
            return Ok(());
        };
        self.budget.charge(state.stack.len())?;
        if !state.stack.contains(&scope) {
            return Err(fail(ErrorKind::Stack, span));
        }
        for origin in &origins {
            let mut at = Some(scope);
            let mut inside = false;
            while let Some(id) = at {
                self.budget.charge(1)?;
                if id == *origin {
                    inside = true;
                    break;
                }
                at = self.typed.functions()[self.function].task_scopes[id.0 as usize].parent;
            }
            if !inside {
                return Err(Error {
                    kind: ErrorKind::Escape,
                    primary: Some(span),
                    secondary: Some(
                        self.typed.functions()[self.function].task_scopes[origin.0 as usize].span,
                    ),
                });
            }
        }
        state.origins[index] = origins;
        Ok(())
    }
    fn operation(
        &mut self,
        state: &mut State,
        i: &b::Instruction,
        check: bool,
    ) -> Result<(), Error> {
        use b::Op;
        let span = i.span;
        self.budget.charge(1)?;
        if self.typed.places().declarations().functions()[self.function]
            .effect
            .is_async()
            && (state.saved || state.stack.is_empty())
            && !failure::bookkeeping(&i.op)
            && !matches!(i.op, Op::TaskScopeExit(_))
        {
            return Err(fail(ErrorKind::ReturnEscrow, span));
        }
        match &i.op {
            Op::TaskScopeEnter(scope) => {
                let parent =
                    self.typed.functions()[self.function].task_scopes[scope.0 as usize].parent;
                if parent != state.stack.last().copied() {
                    return Err(fail(ErrorKind::Stack, span));
                }
                append(self.budget, &mut state.stack, *scope)?;
            }
            Op::TaskScopeExit(scope) => {
                if state.stack.pop() != Some(*scope) {
                    return Err(fail(ErrorKind::Stack, span));
                }
                for (index, binding) in self.meta.scopes.iter().enumerate() {
                    self.budget.charge(1)?;
                    if *binding == Some(*scope) {
                        state.origins[index].clear();
                    }
                }
                for origins in &state.origins {
                    self.budget.charge(origins.len())?;
                    if origins.contains(scope) {
                        return Err(fail(ErrorKind::Escape, span));
                    }
                }
            }
            Op::TaskSpawn { .. } => {
                let scope = state
                    .stack
                    .last()
                    .copied()
                    .ok_or_else(|| fail(ErrorKind::Stack, span))?;
                let mut origins = self.budget.vector(1)?;
                origins.push(scope);
                self.assign(state, self.value(i.results[0]), origins, span)?;
            }
            Op::Move(value) => {
                let from = self.value(*value);
                if self.meta.scopes[from].is_some() {
                    let origins = self.take(state, from, span, check)?;
                    self.assign(state, self.value(i.results[0]), origins, span)?;
                }
            }
            Op::MovePlace(place) => {
                let from = self.place(*place);
                if self.meta.scopes[from].is_some() {
                    let origins = self.take(state, from, span, check)?;
                    self.assign(state, self.value(i.results[0]), origins, span)?;
                }
            }
            Op::Initialize { local, value } => {
                let to = local.0 as usize;
                if self.meta.scopes[to].is_some() {
                    let origins = self.take(state, self.value(*value), span, check)?;
                    self.assign(state, to, origins, span)?;
                }
            }
            Op::Replace { place, value } => {
                let to = self.place(*place);
                if self.meta.scopes[to].is_some() {
                    let origins = self.take(state, self.value(*value), span, check)?;
                    self.assign(state, to, origins, span)?;
                }
            }
            Op::ReplacePrepared { destination, value } => {
                let place = self.meta.prepared[destination.0 as usize]
                    .ok_or_else(|| fail(ErrorKind::Claim, span))?;
                let to = self.place(place);
                if self.meta.scopes[to].is_some() {
                    let origins = self.take(state, self.value(*value), span, check)?;
                    self.assign(state, to, origins, span)?;
                }
            }
            Op::TaskAwait(value) => {
                self.take(state, self.value(*value), span, check)?;
            }
            Op::TaskCancel(place) => {
                self.read(state, self.place(*place), span, check)?;
            }
            Op::TaskSaveReturn(_) => {
                if state.saved {
                    return Err(fail(ErrorKind::ReturnEscrow, span));
                }
                state.saved = true;
            }
            Op::DropLocal(local) | Op::DropLocalIfInitialized(local) => {
                state.origins[local.0 as usize].clear()
            }
            Op::DropValue(value) | Op::DropValueIfAvailable(value) => {
                state.origins[self.value(*value)].clear()
            }
            Op::Cleanup(cleanup) => {
                for step in &cleanup.steps {
                    self.operation(state, &step.instruction(), check)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn block(&mut self, state: &mut State, index: usize, check: bool) -> Result<(), Error> {
        let block = &self.typed.functions()[self.function].blocks[index];
        for i in &block.instructions {
            self.operation(state, i, check)?;
        }
        let asynchronous = self.typed.places().declarations().functions()[self.function]
            .effect
            .is_async();
        match block.terminator.kind {
            b::Term::ReturnSaved if !asynchronous || !state.saved || !state.stack.is_empty() => {
                return Err(fail(ErrorKind::ReturnEscrow, block.terminator.span));
            }
            b::Term::Return(_) if asynchronous => {
                return Err(fail(ErrorKind::ReturnEscrow, block.terminator.span));
            }
            _ => {}
        }
        Ok(())
    }
    fn edge(
        &mut self,
        state: &mut State,
        target: &m::Target,
        span: Span,
        check: bool,
    ) -> Result<(), Error> {
        let parameters =
            &self.typed.functions()[self.function].blocks[target.block.0 as usize].parameters;
        let mut incoming = self.budget.vector(target.arguments.len())?;
        for value in &target.arguments {
            incoming.push(self.read(state, self.value(*value), span, check)?);
        }
        for value in &target.arguments {
            if self.meta.scopes[self.value(*value)].is_some() {
                state.origins[self.value(*value)].clear();
            }
        }
        for (parameter, origins) in parameters.iter().zip(incoming) {
            if self.meta.scopes[self.value(*parameter)].is_some() {
                self.assign(state, self.value(*parameter), origins, span)?;
            }
        }
        Ok(())
    }
}
fn analyze(
    typed: &b::TypedBodies,
    function: usize,
    meta: &Metadata,
    budget: &mut m::Budget,
) -> Result<(), Error> {
    let body = &typed.functions()[function];
    let n = body.blocks.len();
    let asynchronous = typed.places().declarations().functions()[function]
        .effect
        .is_async();
    let mut entries = budget.vector(n)?;
    entries.resize_with(n, || None);
    entries[0] = Some(State::new(meta.scopes.len(), asynchronous, budget)?);
    let mut queue = budget.vector(n)?;
    queue.resize(n, 0usize);
    let mut queued = budget.vector(n)?;
    queued.resize(n, false);
    queued[0] = true;
    let (mut head, mut tail, mut pending) = (0usize, 1 % n, 1usize);
    let mut check = Check {
        typed,
        function,
        meta,
        budget,
    };
    while pending > 0 {
        check.budget.charge(1)?;
        let block = queue[head];
        head = (head + 1) % n;
        pending -= 1;
        queued[block] = false;
        let mut state = entries[block].as_ref().unwrap().copy(check.budget)?;
        check.block(&mut state, block, false)?;
        for target in body.blocks[block].terminator.kind.targets() {
            let mut next = state.copy(check.budget)?;
            check.edge(&mut next, target, body.blocks[block].terminator.span, false)?;
            let to = target.block.0 as usize;
            let changed = if let Some(current) = entries[to].as_mut() {
                current.join(&next, body.blocks[block].terminator.span, check.budget)?
            } else {
                entries[to] = Some(next);
                true
            };
            if changed && !queued[to] {
                queue[tail] = to;
                tail = (tail + 1) % n;
                pending += 1;
                queued[to] = true;
            }
        }
    }
    for (block, entry) in entries.iter().enumerate() {
        if let Some(entry) = entry {
            let mut state = entry.copy(check.budget)?;
            check.block(&mut state, block, true)?;
            for target in body.blocks[block].terminator.kind.targets() {
                let mut edge = state.copy(check.budget)?;
                check.edge(&mut edge, target, body.blocks[block].terminator.span, true)?;
            }
        }
    }
    Ok(())
}
