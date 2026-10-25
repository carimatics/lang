//! Intra-function region transfer over checked MIR. This is not an admission
//! witness: guest calls still require interprocedural effects/return control.
use super::{super::flow, *};

pub mod calls;
mod transfer;
mod values;
use values::{Data, Engine, Projection, ProjectionKind, RootKind};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub work: usize,
    pub allocations: usize,
    pub elements: usize,
    pub depth: usize,
    /// Cumulative interned values/places/roots and call-summary/effect entries.
    pub nodes: usize,
    /// Cumulative block entries and scratch state cells.
    pub state_cells: usize,
}
impl Default for Limits {
    fn default() -> Self {
        let l = m::Limits::default();
        Self {
            work: l.work,
            allocations: l.allocations,
            elements: l.elements,
            depth: 64,
            nodes: 1_000_000,
            state_cells: 16_000_000,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Origin {
    Arena(declarations::LocalId),
    /// The place is an analysis-local caller projection, never an address.
    Caller {
        parameter: declarations::LocalId,
        place: usize,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Brand {
    pub origin: Origin,
    pub expired: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Resource(m::ErrorKind),
    Depth {
        requested: usize,
        limit: usize,
    },
    Nodes {
        requested: usize,
        limit: usize,
    },
    StateCells {
        requested: usize,
        limit: usize,
    },
    MissingReference,
    InvalidShape,
    ArenaState(declarations::LocalId),
    ExpiredArena(declarations::LocalId),
    CrossArena {
        owner: declarations::LocalId,
        incoming: Brand,
    },
    Escape {
        incoming: Brand,
    },
    ReturnEscape {
        incoming: Brand,
    },
    SharedStorage {
        incoming: Brand,
    },
    ImportEffectsRequired(m::ImportId),
    CallStore {
        callee: FunctionId,
        store: Span,
        incoming: Brand,
    },
    LocalReferenceEscape,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub function: Option<FunctionId>,
    pub primary: Option<Span>,
    pub secondary: Option<Span>,
}
impl From<m::Error> for Error {
    fn from(e: m::Error) -> Self {
        Self {
            kind: ErrorKind::Resource(e.kind),
            function: None,
            primary: None,
            secondary: None,
        }
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "MIR local region analysis failed: {:?}", self.kind)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;

pub struct FunctionAnalysis {
    engine: Engine,
    entries: Vec<usize>,
    width: usize,
    blocks: usize,
    reached: Vec<bool>,
    /// These sites have NOT had mutation/store/normal-return effects checked.
    pending: Vec<bodies::Site>,
}
impl FunctionAnalysis {
    pub fn pending_calls(&self) -> &[bodies::Site] {
        &self.pending
    }
    /// Direct token/container brands at a block entry. Aggregates and loans
    /// retain their structure internally; this query does not flatten them.
    /// Entries of a function with pending calls are provisional: unknown
    /// mutations have not been applied and its final diagnostics are deferred.
    /// Unreachable blocks and invalid indices have no entry facts.
    pub fn direct_brands(&self, block: m::BlockId, slot: moves::Slot) -> Option<&[Brand]> {
        let root = self.engine.slot(slot)?;
        if block.0 as usize >= self.blocks || !self.reached[block.0 as usize] {
            return None;
        }
        let id = self.entries[block.0 as usize * self.width + root];
        match self.engine.values.get(id)? {
            Data::Empty => Some(&[]),
            Data::Brands(v) => Some(v),
            _ => None,
        }
    }
}
pub struct Analysis<'p, 'a> {
    plan: &'p Plan<'a>,
    functions: Vec<FunctionAnalysis>,
    usage: m::Usage,
    nodes: usize,
    cells: usize,
}
impl<'p, 'a> Analysis<'p, 'a> {
    pub fn plan(&self) -> &'p Plan<'a> {
        self.plan
    }
    pub fn functions(&self) -> &[FunctionAnalysis] {
        &self.functions
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub const fn nodes_used(&self) -> usize {
        self.nodes
    }
    pub const fn state_cells(&self) -> usize {
        self.cells
    }
}
struct Admission<'a> {
    budget: &'a mut Budget,
    limits: Limits,
    nodes: usize,
    cells: usize,
}
impl Admission<'_> {
    fn node(&mut self) -> Result<()> {
        self.budget.charge(1)?;
        self.nodes = add(self.nodes, 1)?;
        if self.nodes > self.limits.nodes {
            return Err(self.error(ErrorKind::Nodes {
                requested: self.nodes,
                limit: self.limits.nodes,
            }));
        }
        Ok(())
    }
    fn cells(&mut self, count: usize) -> Result<()> {
        self.cells = add(self.cells, count)?;
        if self.cells > self.limits.state_cells {
            return Err(self.error(ErrorKind::StateCells {
                requested: self.cells,
                limit: self.limits.state_cells,
            }));
        }
        Ok(())
    }
    fn depth(&mut self, depth: usize) -> Result<()> {
        self.budget.charge(1)?;
        if depth > self.limits.depth {
            return Err(self.error(ErrorKind::Depth {
                requested: depth,
                limit: self.limits.depth,
            }));
        }
        Ok(())
    }
    fn error(&self, kind: ErrorKind) -> Error {
        Error {
            kind,
            function: None,
            primary: None,
            secondary: None,
        }
    }
    fn copy<T: Copy>(&mut self, values: &[T]) -> Result<Vec<T>> {
        self.budget.charge(values.len())?;
        let mut out = self.budget.vector(values.len())?;
        out.extend_from_slice(values);
        Ok(out)
    }
    fn push<T>(&mut self, values: &mut Vec<T>, value: T) -> Result<()> {
        self.budget.charge(1)?;
        if values.len() == values.capacity() {
            let next = roots_checked_mul(values.capacity().max(1), 2)?;
            self.budget.allocate::<T>(next)?;
            values
                .try_reserve_exact(next - values.len())
                .map_err(|_| m::Error::new(m::ErrorKind::AllocationFailed))?;
        }
        values.push(value);
        Ok(())
    }
}

/// Performs local transfer and reports outstanding call obligations separately.
/// Neither success nor an empty pending-call list authorizes execution.
pub fn analyze<'p, 'a>(plan: &'p Plan<'a>, limits: Limits) -> Result<Analysis<'p, 'a>> {
    let mut budget = Budget::with_usage(
        m::Limits {
            work: limits.work,
            allocations: limits.allocations,
            elements: limits.elements,
            depth: limits.depth,
            ..Default::default()
        },
        plan.usage(),
    )?;
    analyze_in(plan, limits, &mut budget)
}

pub(crate) fn analyze_in<'p, 'a>(
    plan: &'p Plan<'a>,
    limits: Limits,
    budget: &mut Budget,
) -> Result<Analysis<'p, 'a>> {
    let mut admit = Admission {
        budget,
        limits,
        nodes: 0,
        cells: 0,
    };
    let mut functions = admit.budget.vector(plan.functions().len())?;
    for index in 0..plan.functions().len() {
        let result = function(plan, index, &mut admit, None, false).map_err(|mut e| {
            e.function = Some(FunctionId(index as u32));
            if e.primary.is_none() {
                e.primary = Some(
                    plan.source
                        .activity()
                        .moved()
                        .typed()
                        .places()
                        .declarations()
                        .functions()[index]
                        .span,
                );
            }
            e
        })?;
        functions.push(result);
    }
    Ok(Analysis {
        plan,
        functions,
        usage: m::Usage {
            work: admit.budget.work_used(),
            allocation_bytes: admit.budget.allocation_bytes(),
            elements: admit.budget.elements_used(),
        },
        nodes: admit.nodes,
        cells: admit.cells,
    })
}

fn function(
    plan: &Plan<'_>,
    index: usize,
    admit: &mut Admission<'_>,
    summaries: Option<&[calls::Summary]>,
    final_check: bool,
) -> Result<FunctionAnalysis> {
    let typed = plan.source.activity().moved().typed();
    let body = &typed.functions()[index];
    let declaration = &typed.places().declarations().functions()[index];
    let mut engine = Engine::new(plan, index, admit)?;
    let mut seeds = Vec::new();
    for parameter in &declaration.parameters {
        let root = parameter.0 as usize;
        let place = engine.place(root, &[], admit)?;
        let value = engine.parameter(plan, engine.roots[root].node, *parameter, place, admit, 0)?;
        admit.push(&mut seeds, (root, value))?;
    }
    engine.read_roots(plan, index, admit)?;
    engine.record = summaries.is_some();
    if engine.record {
        admit.cells(roots_checked_mul(engine.roots.len(), 2)?)?;
        engine.outputs = flow::filled(admit.budget, engine.roots.len(), 0usize)?;
        engine.wrote = flow::filled(admit.budget, engine.roots.len(), false)?;
    }
    let roots = engine.roots.len();
    let width = add(
        add(roots, roots)?,
        add(body.prepared, declaration.locals.len())?,
    )?;
    let cells = roots_checked_mul(width, body.blocks.len())?;
    admit.cells(add(cells, roots_checked_mul(width, 2)?)?)?;
    let mut entries = flow::filled(admit.budget, cells, 0usize)?;
    let mut row = flow::filled(admit.budget, width, 0usize)?;
    let mut edge = flow::filled(admit.budget, width, 0usize)?;
    entries[roots..roots * 2].fill(usize::MAX);
    for (root, value) in seeds {
        entries[root] = value;
        entries[roots + root] = 0;
    }
    for (root, metadata) in engine.roots.iter().enumerate() {
        if matches!(metadata.kind, RootKind::Caller { .. }) {
            entries[roots + root] = 0;
            entries[root] = metadata.seed;
        }
    }
    let mut reached = flow::filled(admit.budget, body.blocks.len(), false)?;
    reached[0] = true;
    let mut queued = flow::filled(admit.budget, body.blocks.len(), false)?;
    let mut queue = flow::filled(admit.budget, body.blocks.len(), 0usize)?;
    queued[0] = true;
    let (mut head, mut tail, mut pending) = (0usize, 1 % queue.len(), 1usize);
    while pending > 0 {
        admit.budget.charge(1)?;
        let block = queue[head];
        head = (head + 1) % queue.len();
        pending -= 1;
        queued[block] = false;
        admit.budget.charge(width)?;
        row.copy_from_slice(&entries[block * width..(block + 1) * width]);
        if !engine.block(
            plan,
            index,
            &mut row,
            block,
            &body.blocks[block],
            admit,
            false,
            summaries,
        )? {
            continue;
        }
        for target in body.blocks[block].terminator.kind.targets() {
            admit.budget.charge(width)?;
            edge.copy_from_slice(&row);
            engine.edge(plan, index, &mut edge, target, admit)?;
            let to = target.block.0 as usize;
            let dest = &mut entries[to * width..(to + 1) * width];
            let changed = if reached[to] {
                engine.merge(plan, dest, &edge, admit)?
            } else {
                admit.budget.charge(width)?;
                dest.copy_from_slice(&edge);
                reached[to] = true;
                true
            };
            if changed && !queued[to] {
                queue[tail] = to;
                tail = (tail + 1) % queue.len();
                pending += 1;
                queued[to] = true;
            }
        }
    }
    let mut pending = admit.budget.vector(plan.functions[index].calls.len())?;
    for call in &plan.functions[index].calls {
        admit.budget.charge(1)?;
        if summaries.is_none()
            && (call.effects == Effects::Regions || matches!(call.target, Target::Guest(_)))
        {
            pending.push(call.site);
        }
    }
    // The provisional entries above do not apply unknown mutations. Never
    // reject a valid body using stale pre-call facts. R10 must apply summaries
    // and rerun the final check for the entire pending function.
    if (summaries.is_none() && pending.is_empty()) || final_check {
        for (block, source) in body.blocks.iter().enumerate() {
            if !reached[block] {
                continue;
            }
            admit.budget.charge(width)?;
            row.copy_from_slice(&entries[block * width..(block + 1) * width]);
            engine.block(plan, index, &mut row, block, source, admit, true, summaries)?;
        }
    }
    Ok(FunctionAnalysis {
        engine,
        entries,
        width,
        blocks: body.blocks.len(),
        reached,
        pending,
    })
}
fn roots_checked_mul(a: usize, b: usize) -> std::result::Result<usize, m::Error> {
    a.checked_mul(b)
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))
}
