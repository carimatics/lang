//! Activation/end state of explicit MIR loan identities. This is not a proof
//! of reference provenance, alias exclusion, parent/cell dependencies, or safe
//! reactivation of references retained from an earlier loop iteration.

use super::super as m;
use super::{Budget, Span, bodies as b, flow, moves};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Activity {
    Inactive,
    Active,
    MaybeActive,
}
impl Activity {
    fn join(self, other: Self) -> Self {
        if self == other {
            self
        } else {
            Self::MaybeActive
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Resource(m::ErrorKind),
    StateCells {
        requested: usize,
        limit: usize,
    },
    AlreadyActive {
        loan: b::LoanId,
        state: Activity,
    },
    NotActive {
        loan: b::LoanId,
        state: Activity,
    },
    Unended {
        loan: b::LoanId,
        state: Activity,
    },
    LoopState {
        header: m::BlockId,
        loan: b::LoanId,
        expected: Activity,
        actual: Activity,
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
        write!(
            f,
            "ownership MIR loan activity analysis failed: {:?}",
            self.kind
        )
    }
}
impl StdError for Error {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub work: usize,
    pub allocations: usize,
    pub elements: usize,
    /// Cumulative cells in the baseline and fixed-point block-entry matrices.
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
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionActivity {
    loans: usize,
    blocks: usize,
    entries: Vec<Activity>,
}
impl FunctionActivity {
    pub fn entry(&self, block: m::BlockId, loan: b::LoanId) -> Option<Activity> {
        let (block, loan) = (block.0 as usize, loan.0 as usize);
        if block >= self.blocks || loan >= self.loans {
            return None;
        }
        self.entries
            .get(block.checked_mul(self.loans)?.checked_add(loan)?)
            .copied()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActivityBodies {
    moved: moves::MoveCheckedBodies,
    functions: Vec<FunctionActivity>,
    work: usize,
    bytes: usize,
    elements: usize,
}
impl ActivityBodies {
    pub fn moved(&self) -> &moves::MoveCheckedBodies {
        &self.moved
    }
    pub fn functions(&self) -> &[FunctionActivity] {
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
    pub fn into_moved(self) -> moves::MoveCheckedBodies {
        self.moved
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Mode {
    Propagate,
    Check,
}

fn add(a: usize, b: usize) -> Result<usize, m::Error> {
    a.checked_add(b)
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))
}
fn copy(
    budget: &mut Budget,
    destination: &mut [Activity],
    source: &[Activity],
) -> Result<(), m::Error> {
    budget.charge(source.len())?;
    destination.copy_from_slice(source);
    Ok(())
}
fn join(
    budget: &mut Budget,
    destination: &mut [Activity],
    source: &[Activity],
) -> Result<bool, m::Error> {
    let mut changed = false;
    for (to, from) in destination.iter_mut().zip(source) {
        budget.charge(1)?;
        let next = to.join(*from);
        changed |= next != *to;
        *to = next;
    }
    Ok(changed)
}
fn block(
    body: &b::Block,
    state: &mut [Activity],
    origins: &[Option<Span>],
    mode: Mode,
    budget: &mut Budget,
) -> Result<(), Error> {
    for i in &body.instructions {
        budget.charge(1)?;
        if let b::Op::Cleanup(cleanup) = &i.op {
            budget.charge(cleanup.steps.len())?;
        }
        if let Some(loan) = i.op.created_loan() {
            let slot = &mut state[loan.0 as usize];
            if mode == Mode::Check && *slot != Activity::Inactive {
                return Err(Error {
                    kind: ErrorKind::AlreadyActive { loan, state: *slot },
                    primary: Some(i.span),
                    secondary: origins[loan.0 as usize],
                });
            }
            *slot = Activity::Active;
        }
        for (loan, end, span) in i.op.ended_loans(i.span) {
            budget.charge(1)?;
            let slot = &mut state[loan.0 as usize];
            if mode == Mode::Check && end == b::EndMode::Required && *slot != Activity::Active {
                return Err(Error {
                    kind: ErrorKind::NotActive { loan, state: *slot },
                    primary: Some(span),
                    secondary: origins[loan.0 as usize],
                });
            }
            *slot = Activity::Inactive;
        }
    }
    // Typed MIR requires Panic to be the last instruction with an Unreachable
    // terminator. It has no success exit: mandatory failure retirement ends
    // its active registrations after recording the panic, including arena
    // allocator loans. Normal returns and bare unreachable exits still need
    // explicit ends here; do not retire owners before recording the panic.
    let panic = body
        .instructions
        .last()
        .is_some_and(|i| matches!(i.op, b::Op::Panic(_)));
    if mode == Mode::Check && body.terminator.kind.is_exit() && !panic {
        for (index, activity) in state.iter().copied().enumerate() {
            budget.charge(1)?;
            if activity != Activity::Inactive {
                return Err(Error {
                    kind: ErrorKind::Unended {
                        loan: b::LoanId(index as u32),
                        state: activity,
                    },
                    primary: Some(body.terminator.span),
                    secondary: origins[index],
                });
            }
        }
    }
    Ok(())
}

/// Resume cumulative move-stage accounting and validate explicit activation/end
/// state. ActivityBodies cannot execute and confers no alias/provenance proof.
pub fn analyze(moved: moves::MoveCheckedBodies, limits: Limits) -> Result<ActivityBodies, Error> {
    let mut budget = Budget::with_usage(
        m::Limits {
            work: limits.work,
            allocations: limits.allocations,
            elements: limits.elements,
            ..Default::default()
        },
        m::Usage {
            work: moved.work_used(),
            allocation_bytes: moved.requested_bytes(),
            elements: moved.elements_used(),
        },
    )?;
    analyze_in(moved, limits, &mut budget)
}

pub(crate) fn analyze_in(
    moved: moves::MoveCheckedBodies,
    limits: Limits,
    budget: &mut Budget,
) -> Result<ActivityBodies, Error> {
    let mut functions = budget.vector(moved.typed().functions().len())?;
    let mut cells = 0usize;
    for (index, body) in moved.typed().functions().iter().enumerate() {
        let declaration = &moved.typed().places().declarations().functions()[index];
        let count = body
            .loans
            .checked_mul(body.blocks.len())
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
        functions.push(
            function(body, &moved.typed().facts()[index], budget).map_err(|mut error| {
                if error.primary.is_none() {
                    error.primary = Some(declaration.span);
                }
                error
            })?,
        );
    }
    Ok(ActivityBodies {
        moved,
        functions,
        work: budget.work_used(),
        bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    })
}

fn function(
    body: &b::FunctionBody,
    facts: &b::FunctionFacts,
    budget: &mut Budget,
) -> Result<FunctionActivity, Error> {
    let (loans, blocks) = (body.loans, body.blocks.len());
    let cells = loans
        .checked_mul(blocks)
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))?;
    let mut origins = budget.vector(loans)?;
    for site in facts.loans() {
        budget.charge(1)?;
        origins.push(site.map(|s| body.blocks[s.block.0 as usize].instructions[s.order - 1].span));
    }
    let graph = flow::cycles(body, budget)?;
    let mut baseline = flow::filled(budget, cells, Activity::Inactive)?;
    let mut found = flow::filled(budget, blocks, false)?;
    found[0] = true;
    let mut state = flow::filled(budget, loans, Activity::Inactive)?;
    for node in &graph.order {
        let node = *node;
        copy(
            budget,
            &mut state,
            &baseline[node * loans..(node + 1) * loans],
        )?;
        block(
            &body.blocks[node],
            &mut state,
            &origins,
            Mode::Propagate,
            budget,
        )?;
        for (position, target) in body.blocks[node].terminator.kind.targets().enumerate() {
            budget.charge(1)?;
            if graph.feedback[node][position] {
                continue;
            }
            let to = target.block.0 as usize;
            let row = &mut baseline[to * loans..(to + 1) * loans];
            if found[to] {
                join(budget, row, &state)?;
            } else {
                copy(budget, row, &state)?;
                found[to] = true;
            }
        }
    }
    let mut entries = budget.vector(cells)?;
    budget.charge(cells)?;
    entries.extend_from_slice(&baseline);
    let mut queue = flow::filled(budget, blocks, 0usize)?;
    let mut queued = flow::filled(budget, blocks, true)?;
    for (i, node) in graph.order.iter().copied().enumerate() {
        budget.charge(1)?;
        queue[i] = node;
    }
    let (mut head, mut tail, mut pending) = (0usize, 0usize, blocks);
    while pending > 0 {
        budget.charge(1)?;
        let node = queue[head];
        head = (head + 1) % blocks;
        pending -= 1;
        queued[node] = false;
        copy(
            budget,
            &mut state,
            &entries[node * loans..(node + 1) * loans],
        )?;
        block(
            &body.blocks[node],
            &mut state,
            &origins,
            Mode::Propagate,
            budget,
        )?;
        for target in body.blocks[node].terminator.kind.targets() {
            budget.charge(1)?;
            let to = target.block.0 as usize;
            if join(budget, &mut entries[to * loans..(to + 1) * loans], &state)? && !queued[to] {
                queue[tail] = to;
                tail = (tail + 1) % blocks;
                pending += 1;
                queued[to] = true;
            }
        }
    }
    for (node, body_block) in body.blocks.iter().enumerate() {
        copy(
            budget,
            &mut state,
            &entries[node * loans..(node + 1) * loans],
        )?;
        block(body_block, &mut state, &origins, Mode::Propagate, budget)?;
        for (position, target) in body_block.terminator.kind.targets().enumerate() {
            budget.charge(1)?;
            if !graph.feedback[node][position] {
                continue;
            }
            let to = target.block.0 as usize;
            for (index, (expected, actual)) in baseline[to * loans..(to + 1) * loans]
                .iter()
                .zip(&state)
                .enumerate()
            {
                budget.charge(1)?;
                if expected != actual {
                    return Err(Error {
                        kind: ErrorKind::LoopState {
                            header: target.block,
                            loan: b::LoanId(index as u32),
                            expected: *expected,
                            actual: *actual,
                        },
                        primary: Some(body_block.terminator.span),
                        secondary: Some(body.blocks[to].span),
                    });
                }
            }
        }
    }
    for (node, body_block) in body.blocks.iter().enumerate() {
        copy(
            budget,
            &mut state,
            &entries[node * loans..(node + 1) * loans],
        )?;
        block(body_block, &mut state, &origins, Mode::Check, budget)?;
    }
    Ok(FunctionActivity {
        loans,
        blocks,
        entries,
    })
}
