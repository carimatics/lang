//! Reference provenance and lexical access checking over ownership MIR.
//! These facts do not establish arena/call effects, cleanup, or execution.

use super::super as m;
use super::{Budget, LoanKind, Span, Type, bodies as b, loan_activity, moves};
use std::error::Error as StdError;
use std::fmt::{Display, Formatter};

mod analysis;
mod check;
mod state;
mod topology;

pub use analysis::analyze;
pub(crate) use analysis::analyze_in;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReferenceId(pub u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CellId(pub u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Owner {
    Local(super::declarations::LocalId),
    Value(m::Register),
    Parameter {
        local: super::declarations::LocalId,
        level: u32,
    },
    InstanceAllocator,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Resource(m::ErrorKind),
    Nodes {
        requested: usize,
        limit: usize,
    },
    StateWords {
        requested: usize,
        limit: usize,
    },
    MissingReference,
    Suspension(ReferenceId),
    EndedReference(ReferenceId),
    Conflict {
        owner: Owner,
        loan: ReferenceId,
    },
    FrozenParent {
        parent: ReferenceId,
        child: ReferenceId,
    },
    EndParent {
        parent: ReferenceId,
        child: ReferenceId,
    },
    CallerCellEscape(Owner),
    InvalidPrepared(b::PreparedPlaceId),
    LoopState {
        header: m::BlockId,
        owner: Option<Owner>,
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
        write!(f, "ownership MIR loan analysis failed: {:?}", self.kind)
    }
}
impl StdError for Error {}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub work: usize,
    pub allocations: usize,
    pub elements: usize,
    pub nodes: usize,
    pub depth: usize,
    /// Cumulative words in both block-entry state matrices.
    pub state_words: usize,
}
impl Default for Limits {
    fn default() -> Self {
        let l = m::Limits::default();
        Self {
            work: l.work,
            allocations: l.allocations,
            elements: l.elements,
            nodes: 1_000_000,
            depth: 64,
            state_words: 8_000_000,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FunctionLoans {
    topology: topology::Topology,
    layout: state::Layout,
    blocks: usize,
    entries: Vec<u64>,
}
impl FunctionLoans {
    pub fn cell_owner(&self, cell: CellId) -> Option<Owner> {
        self.topology.cells.get(cell.0 as usize).map(|c| c.owner)
    }
    pub fn root_cell(&self, slot: moves::Slot) -> Option<CellId> {
        self.topology
            .root(slot)
            .and_then(|n| u32::try_from(n).ok())
            .map(CellId)
    }
    pub fn references(
        &self,
        block: m::BlockId,
        slot: moves::Slot,
        reference: ReferenceId,
    ) -> Option<bool> {
        let node = self.topology.root(slot)?;
        let (block, id) = (block.0 as usize, reference.0 as usize);
        if block >= self.blocks || id >= self.topology.kinds.len() {
            return None;
        }
        let row = &self.entries[block * self.layout.words..(block + 1) * self.layout.words];
        Some(
            self.topology.cells[node]
                .leaves
                .clone()
                .any(|leaf| self.layout.has_ref(row, leaf, id)),
        )
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoanCheckedBodies {
    activity: loan_activity::ActivityBodies,
    functions: Vec<FunctionLoans>,
    work: usize,
    bytes: usize,
    elements: usize,
}
impl LoanCheckedBodies {
    pub fn activity(&self) -> &loan_activity::ActivityBodies {
        &self.activity
    }
    pub fn functions(&self) -> &[FunctionLoans] {
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
    pub fn into_activity(self) -> loan_activity::ActivityBodies {
        self.activity
    }
}

fn add(a: usize, b: usize) -> Result<usize, m::Error> {
    a.checked_add(b)
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))
}
fn mul(a: usize, b: usize) -> Result<usize, m::Error> {
    a.checked_mul(b)
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow))
}
fn push<T>(budget: &mut Budget, values: &mut Vec<T>, value: T) -> Result<(), m::Error> {
    budget.charge(1)?;
    if values.len() == values.capacity() {
        let next = mul(values.capacity().max(1), 2)?;
        budget.allocate::<T>(next)?;
        values
            .try_reserve_exact(next - values.len())
            .map_err(|_| m::Error::new(m::ErrorKind::AllocationFailed))?;
    }
    values.push(value);
    Ok(())
}
