//! Bounded initialization and affine consumption over typed ownership CFGs.
//! This stage does not prove active loans, indirect alias/region flow, cleanup,
//! prepared destination validity, or executable VM/native behavior.

use super::super as m;
use super::{bodies as b, declarations as d, flow::cycles, places as p, *};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Availability {
    Vacant,
    Available,
    MaybeAvailable,
}
impl Availability {
    fn join(self, other: Self) -> Self {
        if self == other {
            self
        } else {
            Self::MaybeAvailable
        }
    }
    fn may_live(self) -> bool {
        self != Self::Vacant
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Slot {
    Local(d::LocalId),
    Value(m::Register),
    Prepared(b::PreparedPlaceId),
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Resource(m::ErrorKind),
    StateCells {
        requested: usize,
        limit: usize,
    },
    Unavailable {
        slot: Slot,
        state: Availability,
    },
    InitializeLive(Slot),
    OverwriteOwner(Slot),
    OverwritePrepared(b::PreparedPlaceId),
    UnfinishedPrepared(b::PreparedPlaceId),
    LoopState {
        header: m::BlockId,
        slot: Slot,
        expected: Availability,
        actual: Availability,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub primary: Option<Span>,
    pub secondary: Option<Span>,
}
impl From<m::Error> for Error {
    fn from(e: m::Error) -> Self {
        Self {
            kind: ErrorKind::Resource(e.kind),
            primary: None,
            secondary: None,
        }
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "ownership MIR move analysis failed: {:?}", self.kind)
    }
}
impl StdError for Error {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub work: usize,
    pub allocations: usize,
    pub elements: usize,
    pub state_cells: usize,
}
impl Default for Limits {
    fn default() -> Self {
        let l = m::Limits::default();
        Self {
            work: l.work,
            allocations: l.allocations,
            elements: l.elements,
            state_cells: 16_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Obligation {
    pub slot: Slot,
    pub state: Availability,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Exit {
    pub block: m::BlockId,
    owners: Vec<Obligation>,
}
impl Exit {
    pub fn owners(&self) -> &[Obligation] {
        &self.owners
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionMoves {
    locals: usize,
    values: usize,
    slots: usize,
    blocks: usize,
    entries: Vec<Availability>,
    causes: Vec<Option<Span>>,
    exits: Vec<Exit>,
}
impl FunctionMoves {
    fn index(&self, slot: Slot) -> Option<usize> {
        let s = match slot {
            Slot::Local(l) if (l.0 as usize) < self.locals => l.0 as usize,
            Slot::Value(v) if (v.0 as usize) < self.values => {
                self.locals.checked_add(v.0 as usize)?
            }
            Slot::Prepared(p) => self
                .locals
                .checked_add(self.values)?
                .checked_add(p.0 as usize)?,
            _ => return None,
        };
        (s < self.slots).then_some(s)
    }
    pub fn entry(&self, block: m::BlockId, slot: Slot) -> Option<Availability> {
        let b = block.0 as usize;
        if b >= self.blocks {
            return None;
        }
        let s = self.index(slot)?;
        self.entries
            .get(b.checked_mul(self.slots)?.checked_add(s)?)
            .copied()
    }
    pub fn exits(&self) -> &[Exit] {
        &self.exits
    }
    /// A possible earlier consumption site for diagnostics, not path identity.
    pub fn consumption_span(&self, slot: Slot) -> Option<Span> {
        let s = self.index(slot)?;
        self.causes.get(s).copied().flatten()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoveCheckedBodies {
    typed: b::TypedBodies,
    functions: Vec<FunctionMoves>,
    work: usize,
    bytes: usize,
    elements: usize,
}
impl MoveCheckedBodies {
    pub fn typed(&self) -> &b::TypedBodies {
        &self.typed
    }
    pub fn functions(&self) -> &[FunctionMoves] {
        &self.functions
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
    pub fn into_typed(self) -> b::TypedBodies {
        self.typed
    }
}
#[derive(Clone, Copy, Eq, PartialEq)]
enum Mode {
    Propagate,
    Check,
}
#[derive(Clone, Copy)]
struct Properties {
    copy: bool,
    drop: bool,
}
fn add(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_add(b)
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow).into())
}
fn filled<T: Clone>(budget: &mut Budget, n: usize, value: T) -> Result<Vec<T>, Error> {
    budget.charge(n)?;
    let mut out = budget.vector(n)?;
    out.resize(n, value);
    Ok(out)
}
fn copy<T: Copy>(budget: &mut Budget, destination: &mut [T], source: &[T]) -> Result<(), Error> {
    budget.charge(source.len())?;
    destination.copy_from_slice(source);
    Ok(())
}
fn slot(locals: usize, values: usize, index: usize) -> Slot {
    if index < locals {
        Slot::Local(d::LocalId(index as u32))
    } else if index - locals < values {
        Slot::Value(m::Register((index - locals) as u32))
    } else {
        Slot::Prepared(b::PreparedPlaceId((index - locals - values) as u32))
    }
}
fn join(
    budget: &mut Budget,
    destination: &mut [Availability],
    source: &[Availability],
) -> Result<bool, Error> {
    let mut changed = false;
    for (to, from) in destination.iter_mut().zip(source) {
        budget.charge(1)?;
        let next = to.join(*from);
        changed |= next != *to;
        *to = next;
    }
    Ok(changed)
}

struct Context<'a, 'b> {
    typed: &'a b::TypedBodies,
    function: usize,
    locals: usize,
    values: usize,
    properties: &'a [Properties],
    causes: &'b mut [Option<Span>],
    budget: &'b mut Budget,
}
impl Context<'_, '_> {
    fn require(
        &mut self,
        state: &[Availability],
        index: usize,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        self.budget.charge(1)?;
        if mode == Mode::Check && state[index] != Availability::Available {
            return Err(Error {
                kind: ErrorKind::Unavailable {
                    slot: slot(self.locals, self.values, index),
                    state: state[index],
                },
                primary: Some(span),
                secondary: self.causes[index],
            });
        }
        Ok(())
    }
    fn consume(
        &mut self,
        state: &mut [Availability],
        index: usize,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        let available = state[index] == Availability::Available;
        self.require(state, index, span, mode)?;
        if !self.properties[index].copy {
            state[index] = Availability::Vacant;
            if available && self.causes[index].is_none() {
                self.causes[index] = Some(span);
            }
        }
        Ok(())
    }
    fn value(&self, v: m::Register) -> usize {
        self.locals + v.0 as usize
    }
    fn prepared(&self, id: b::PreparedPlaceId) -> usize {
        self.locals + self.values + id.0 as usize
    }
    fn read(
        &mut self,
        state: &[Availability],
        v: m::Register,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        self.require(state, self.value(v), span, mode)
    }
    fn take(
        &mut self,
        state: &mut [Availability],
        v: m::Register,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        self.consume(state, self.value(v), span, mode)
    }
    fn define(
        &mut self,
        state: &mut [Availability],
        index: usize,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        self.budget.charge(1)?;
        if mode == Mode::Check && self.properties[index].drop && state[index].may_live() {
            return Err(Error {
                kind: ErrorKind::OverwriteOwner(slot(self.locals, self.values, index)),
                primary: Some(span),
                secondary: self.causes[index],
            });
        }
        state[index] = Availability::Available;
        Ok(())
    }
    fn place_root(&self, id: p::PlaceId) -> p::Root {
        self.typed.places().functions()[self.function].places[id.0 as usize].root
    }
    fn root_index(&self, root: p::Root) -> usize {
        match root {
            p::Root::Local(l) => l.0 as usize,
            p::Root::Value(v) => self.value(v),
        }
    }
    fn read_place(
        &mut self,
        state: &[Availability],
        id: p::PlaceId,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        self.require(state, self.root_index(self.place_root(id)), span, mode)
    }
    fn take_place(
        &mut self,
        state: &mut [Availability],
        id: p::PlaceId,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        self.read_place(state, id, span, mode)?;
        let fact = self
            .typed
            .places()
            .facts(FunctionId(self.function as u32), id)
            .unwrap();
        if !self
            .typed
            .places()
            .declarations()
            .types()
            .properties(fact.value_type)
            .unwrap()
            .is_copy
        {
            self.consume(state, self.root_index(fact.root), span, mode)?;
        }
        Ok(())
    }
    fn operation(
        &mut self,
        state: &mut [Availability],
        i: &b::Instruction,
        mode: Mode,
    ) -> Result<(), Error> {
        use b::Op;
        let s = i.span;
        self.budget.charge(1)?;
        match &i.op {
            Op::I64(_) | Op::Bool(_) | Op::Unit | Op::EndBorrow { .. } => {}
            Op::Add(a, z) | Op::LessThan(a, z) | Op::EqualI64(a, z) => {
                self.read(state, *a, s, mode)?;
                self.read(state, *z, s, mode)?;
            }
            Op::Copy(v) | Op::IsCase { value: v, .. } => self.read(state, *v, s, mode)?,
            Op::Move(v) | Op::DropValue(v) => self.take(state, *v, s, mode)?,
            Op::CopyPlace(p) | Op::Borrow { place: p, .. } => {
                self.read_place(state, *p, s, mode)?
            }
            Op::MovePlace(p) => self.take_place(state, *p, s, mode)?,
            Op::Initialize { local, value } => {
                let index = local.0 as usize;
                self.take(state, *value, s, mode)?;
                if mode == Mode::Check && state[index].may_live() {
                    return Err(Error {
                        kind: ErrorKind::InitializeLive(Slot::Local(*local)),
                        primary: Some(s),
                        secondary: None,
                    });
                }
                state[index] = Availability::Available;
            }
            Op::Replace { place, value } => {
                let p = &self.typed.places().functions()[self.function].places[place.0 as usize];
                if p.projections.is_empty() {
                    if let p::Root::Local(l) = p.root {
                        self.take(state, *value, s, mode)?;
                        state[l.0 as usize] = Availability::Available;
                    } else {
                        self.read_place(state, *place, s, mode)?;
                        self.take(state, *value, s, mode)?;
                    }
                } else {
                    self.read_place(state, *place, s, mode)?;
                    self.take(state, *value, s, mode)?;
                }
            }
            Op::PreparePlace { destination, place } => {
                let p = &self.typed.places().functions()[self.function].places[place.0 as usize];
                // A whole local destination is an address, so reinitializing
                // moved storage needs no read. Projections evaluate their root.
                if !p.projections.is_empty() || matches!(p.root, p::Root::Value(_)) {
                    self.read_place(state, *place, s, mode)?;
                }
                let index = self.prepared(*destination);
                if mode == Mode::Check && state[index].may_live() {
                    return Err(Error {
                        kind: ErrorKind::OverwritePrepared(*destination),
                        primary: Some(s),
                        secondary: None,
                    });
                }
                state[index] = Availability::Available;
            }
            Op::ReplacePrepared { destination, value } => {
                let index = self.prepared(*destination);
                self.require(state, index, s, mode)?;
                self.take(state, *value, s, mode)?;
                let captured =
                    self.typed.facts()[self.function].prepared()[destination.0 as usize].unwrap();
                let p = &self.typed.places().functions()[self.function].places
                    [captured.place.0 as usize];
                if !p.projections.is_empty()
                    && !self
                        .typed
                        .places()
                        .facts(FunctionId(self.function as u32), captured.place)
                        .unwrap()
                        .through_loan
                {
                    self.read_place(state, captured.place, s, mode)?;
                }
                if p.projections.is_empty()
                    && let p::Root::Local(l) = p.root
                {
                    state[l.0 as usize] = Availability::Available;
                }
                // Underlying alias validity belongs to loan analysis. Do not
                // reread a reference cell that RHS effects may have rebound.
                self.consume(state, index, s, mode)?;
            }
            Op::DiscardPrepared(id) => self.consume(state, self.prepared(*id), s, mode)?,
            Op::DropLocal(l) | Op::ArenaExit { owner: l } => {
                let index = l.0 as usize;
                let available = state[index] == Availability::Available;
                self.require(state, index, s, mode)?;
                state[index] = Availability::Vacant;
                if available && self.causes[index].is_none() {
                    self.causes[index] = Some(s);
                }
            }
            Op::ArenaEnter { owner, allocator } => {
                self.read(state, *allocator, s, mode)?;
                let index = owner.0 as usize;
                if mode == Mode::Check && state[index].may_live() {
                    return Err(Error {
                        kind: ErrorKind::InitializeLive(Slot::Local(*owner)),
                        primary: Some(s),
                        secondary: None,
                    });
                }
                state[index] = Availability::Available;
            }
            Op::Call { arguments, .. }
            | Op::Import { arguments, .. }
            | Op::Record {
                fields: arguments, ..
            }
            | Op::Variant {
                payload: arguments, ..
            } => {
                for value in arguments {
                    self.take(state, *value, s, mode)?;
                }
            }
            Op::CopyCaseField { value, guard, .. } | Op::BorrowCaseField { value, guard, .. } => {
                self.read(state, *value, s, mode)?;
                self.read(state, *guard, s, mode)?;
            }
            Op::UnpackVariant { value, guard, .. } => {
                self.read(state, *guard, s, mode)?;
                self.take(state, *value, s, mode)?;
            }
        }
        for result in &i.results {
            self.define(state, self.value(*result), s, mode)?;
        }
        Ok(())
    }
    fn block(&mut self, state: &mut [Availability], block: usize, mode: Mode) -> Result<(), Error> {
        let body = &self.typed.functions()[self.function].blocks[block];
        for i in &body.instructions {
            self.operation(state, i, mode)?;
        }
        match body.terminator.kind {
            m::Term::Return(v) => {
                self.take(state, v, body.terminator.span, mode)?;
                if mode == Mode::Check {
                    for (index, availability) in
                        state.iter().enumerate().skip(self.locals + self.values)
                    {
                        self.budget.charge(1)?;
                        if availability.may_live() {
                            return Err(Error {
                                kind: ErrorKind::UnfinishedPrepared(b::PreparedPlaceId(
                                    (index - self.locals - self.values) as u32,
                                )),
                                primary: Some(body.terminator.span),
                                secondary: None,
                            });
                        }
                    }
                }
            }
            m::Term::Branch { condition, .. } => {
                self.read(state, condition, body.terminator.span, mode)?
            }
            m::Term::Jump(_) => {}
        }
        Ok(())
    }
    fn edge(
        &mut self,
        state: &mut [Availability],
        target: &m::Target,
        span: Span,
        mode: Mode,
    ) -> Result<(), Error> {
        for value in &target.arguments {
            self.take(state, *value, span, mode)?;
        }
        let block = &self.typed.functions()[self.function].blocks[target.block.0 as usize];
        for parameter in &block.parameters {
            self.define(state, self.value(*parameter), span, mode)?;
        }
        Ok(())
    }
}

/// Resume the typed stage's cumulative resource counters. The resulting proof
/// is still nonexecutable and carries live-owner cleanup obligations at returns.
pub fn analyze(typed: b::TypedBodies, limits: Limits) -> Result<MoveCheckedBodies, Error> {
    let mut budget = Budget::with_usage(
        m::Limits {
            work: limits.work,
            allocations: limits.allocations,
            elements: limits.elements,
            ..m::Limits::default()
        },
        m::Usage {
            work: typed.work_used(),
            allocation_bytes: typed.requested_bytes(),
            elements: typed.elements_used(),
        },
    )?;
    let mut functions = budget.vector(typed.functions().len())?;
    let mut cells = 0usize;
    for f in 0..typed.functions().len() {
        let declaration = &typed.places().declarations().functions()[f];
        let slots = add(
            add(
                declaration.locals.len(),
                typed.places().functions()[f].values.len(),
            )?,
            typed.functions()[f].prepared,
        )?;
        let count = slots
            .checked_mul(typed.functions()[f].blocks.len())
            .and_then(|n| n.checked_mul(2))
            .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))?;
        cells = add(cells, count)?;
        if cells > limits.state_cells {
            return Err(Error {
                kind: ErrorKind::StateCells {
                    requested: cells,
                    limit: limits.state_cells,
                },
                primary: Some(declaration.span),
                secondary: None,
            });
        }
        functions.push(function(&typed, f, &mut budget).map_err(|mut error| {
            if error.primary.is_none() {
                error.primary = Some(declaration.span);
            }
            error
        })?);
    }
    Ok(MoveCheckedBodies {
        typed,
        functions,
        work: budget.work_used(),
        bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    })
}
fn function(
    typed: &b::TypedBodies,
    function: usize,
    budget: &mut Budget,
) -> Result<FunctionMoves, Error> {
    let body = &typed.functions()[function];
    let declaration = &typed.places().declarations().functions()[function];
    let locals = declaration.locals.len();
    let values = &typed.places().functions()[function].values;
    let slots = add(add(locals, values.len())?, body.prepared)?;
    let blocks = body.blocks.len();
    let cells = slots
        .checked_mul(blocks)
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))?;
    let mut properties = budget.vector(slots)?;
    for ty in declaration
        .locals
        .iter()
        .map(|l| l.ty.ty)
        .chain(values.iter().map(|v| v.ty))
    {
        budget.charge(1)?;
        let p = typed
            .places()
            .declarations()
            .types()
            .properties(ty)
            .unwrap();
        properties.push(Properties {
            copy: p.is_copy,
            drop: p.requires_drop,
        });
    }
    for _ in 0..body.prepared {
        budget.charge(1)?;
        properties.push(Properties {
            copy: false,
            drop: false,
        });
    }
    let mut causes = filled(budget, slots, None)?;
    let graph = cycles(body, budget)?;
    let mut baseline = filled(budget, cells, Availability::Vacant)?;
    let mut found = filled(budget, blocks, false)?;
    found[0] = true;
    for l in &declaration.parameters {
        budget.charge(1)?;
        baseline[l.0 as usize] = Availability::Available;
    }
    let mut state = filled(budget, slots, Availability::Vacant)?;
    let mut edge = filled(budget, slots, Availability::Vacant)?;
    let mut c = Context {
        typed,
        function,
        locals,
        values: values.len(),
        properties: &properties,
        causes: &mut causes,
        budget,
    };
    for block in &graph.order {
        let block = *block;
        copy(
            c.budget,
            &mut state,
            &baseline[block * slots..(block + 1) * slots],
        )?;
        c.block(&mut state, block, Mode::Propagate)?;
        for (position, target) in body.blocks[block].terminator.kind.targets().enumerate() {
            if graph.feedback[block][position] {
                continue;
            }
            copy(c.budget, &mut edge, &state)?;
            c.edge(
                &mut edge,
                target,
                body.blocks[block].terminator.span,
                Mode::Propagate,
            )?;
            let to = target.block.0 as usize;
            let row = &mut baseline[to * slots..(to + 1) * slots];
            if found[to] {
                join(c.budget, row, &edge)?;
            } else {
                copy(c.budget, row, &edge)?;
                found[to] = true;
            }
        }
    }
    let mut entries = c.budget.vector(cells)?;
    c.budget.charge(cells)?;
    entries.extend_from_slice(&baseline);
    let mut queue = filled(c.budget, blocks, 0usize)?;
    let mut queued = filled(c.budget, blocks, true)?;
    for (i, block) in graph.order.iter().copied().enumerate() {
        c.budget.charge(1)?;
        queue[i] = block;
    }
    let (mut head, mut tail, mut pending) = (0usize, 0usize, blocks);
    while pending > 0 {
        c.budget.charge(1)?;
        let block = queue[head];
        head = (head + 1) % blocks;
        pending -= 1;
        queued[block] = false;
        copy(
            c.budget,
            &mut state,
            &entries[block * slots..(block + 1) * slots],
        )?;
        c.block(&mut state, block, Mode::Propagate)?;
        for target in body.blocks[block].terminator.kind.targets() {
            copy(c.budget, &mut edge, &state)?;
            c.edge(
                &mut edge,
                target,
                body.blocks[block].terminator.span,
                Mode::Propagate,
            )?;
            let to = target.block.0 as usize;
            if join(c.budget, &mut entries[to * slots..(to + 1) * slots], &edge)? && !queued[to] {
                queue[tail] = to;
                tail = (tail + 1) % blocks;
                pending += 1;
                queued[to] = true;
            }
        }
    }
    // Removing DFS feedback edges gives a first-iteration DAG. Restore every
    // local and droppable SSA slot on feedback, including irreducible cycles.
    for (block, b) in body.blocks.iter().enumerate() {
        copy(
            c.budget,
            &mut state,
            &entries[block * slots..(block + 1) * slots],
        )?;
        c.block(&mut state, block, Mode::Propagate)?;
        for (position, target) in b.terminator.kind.targets().enumerate() {
            if !graph.feedback[block][position] {
                continue;
            }
            copy(c.budget, &mut edge, &state)?;
            c.edge(&mut edge, target, b.terminator.span, Mode::Propagate)?;
            let to = target.block.0 as usize;
            let expected = &baseline[to * slots..(to + 1) * slots];
            for (index, (before, after)) in expected.iter().zip(&edge).enumerate() {
                c.budget.charge(1)?;
                if (index < locals || properties[index].drop || index >= locals + values.len())
                    && before != after
                {
                    return Err(Error {
                        kind: ErrorKind::LoopState {
                            header: target.block,
                            slot: slot(locals, values.len(), index),
                            expected: *before,
                            actual: *after,
                        },
                        primary: Some(b.terminator.span),
                        secondary: Some(body.blocks[to].span),
                    });
                }
            }
        }
    }
    let exits_count = body
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator.kind, m::Term::Return(_)))
        .count();
    c.budget.charge(blocks)?;
    let mut exits = c.budget.vector(exits_count)?;
    for (block, b) in body.blocks.iter().enumerate() {
        copy(
            c.budget,
            &mut state,
            &entries[block * slots..(block + 1) * slots],
        )?;
        c.block(&mut state, block, Mode::Check)?;
        if matches!(b.terminator.kind, m::Term::Return(_)) {
            let count = state
                .iter()
                .zip(&properties)
                .filter(|(s, p)| s.may_live() && p.drop)
                .count();
            c.budget.charge(slots)?;
            let mut owners = c.budget.vector(count)?;
            for (index, (s, p)) in state.iter().zip(&properties).enumerate() {
                c.budget.charge(1)?;
                if s.may_live() && p.drop {
                    owners.push(Obligation {
                        slot: slot(locals, values.len(), index),
                        state: *s,
                    });
                }
            }
            exits.push(Exit {
                block: m::BlockId(block as u32),
                owners,
            });
        }
        for target in b.terminator.kind.targets() {
            copy(c.budget, &mut edge, &state)?;
            c.edge(&mut edge, target, b.terminator.span, Mode::Check)?;
        }
    }
    Ok(FunctionMoves {
        locals,
        values: values.len(),
        slots,
        blocks,
        entries,
        causes,
        exits,
    })
}
