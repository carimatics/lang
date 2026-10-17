//! Explicit status/failure control flow derived from fully loan-checked MIR.
//! Source instruction identities and spans are retained. This is in-memory
//! backend-neutral IR, not an ownership wire encoding or executable layout.
//! Typed guest Err values follow success edges; execution status is separate.
use super::{bodies as b, loans};
use crate::unit as m;
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutionFailure {
    Panic,
    Fault,
    FuelLimit,
    Interrupted,
    DeadlineExceeded,
    HostCallLimit,
    CounterOverflow,
    FrameLimit,
    CellLimit,
    AllocationLimit,
    HostCall,
    HostPanic,
    Invariant,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cause {
    Execution(ExecutionFailure),
    /// A callee/cleanup already recorded its primary and ordered secondaries.
    Reported,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeId(pub u32);
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Position {
    Instruction(b::Site),
    Terminator(m::BlockId),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// Preflight authority/frame/cell/metadata bounds before admitting parameter
    /// ownership. Success initializes parameters in declaration order.
    Entry {
        next: NodeId,
        failure: NodeId,
    },
    /// Check control/fuel, then charge exactly one guest step.
    Fuel {
        position: Position,
        next: NodeId,
        failure: NodeId,
    },
    /// Results become initialized only on success. Failure carries a Cause;
    /// call/publication semantics determine which input owners transferred.
    Execute {
        site: b::Site,
        next: NodeId,
        failure: NodeId,
    },
    /// Transfer original edge arguments simultaneously into its parameters.
    Transfer {
        block: m::BlockId,
        arm: u8,
        target: NodeId,
        failure: NodeId,
    },
    Branch {
        condition: m::Register,
        then_node: NodeId,
        else_node: NodeId,
        failure: NodeId,
    },
    StageReturn {
        value: m::Register,
        next: NodeId,
        failure: NodeId,
    },
    DiscardPrepared {
        next: NodeId,
    },
    /// End registrations and retire activation owners, protecting the result.
    RetireReturn {
        next: NodeId,
        failure: NodeId,
    },
    PublishReturn,
    Raise {
        failure: ExecutionFailure,
        next: NodeId,
    },
    RecordFailure {
        next: NodeId,
    },
    /// Continue all cleanup regardless of failures; never consume guest fuel.
    RetireFailure {
        next: NodeId,
    },
    DiscardResult {
        next: NodeId,
    },
    PropagateFailure,
}
impl Kind {
    pub fn targets(self) -> impl Iterator<Item = NodeId> {
        let targets = match self {
            Self::Entry { next, failure }
            | Self::Fuel { next, failure, .. }
            | Self::Execute { next, failure, .. }
            | Self::StageReturn { next, failure, .. }
            | Self::RetireReturn { next, failure } => [Some(next), Some(failure), None],
            Self::Transfer {
                target, failure, ..
            } => [Some(target), Some(failure), None],
            Self::Branch {
                then_node,
                else_node,
                failure,
                ..
            } => [Some(then_node), Some(else_node), Some(failure)],
            Self::DiscardPrepared { next }
            | Self::Raise { next, .. }
            | Self::RecordFailure { next }
            | Self::RetireFailure { next }
            | Self::DiscardResult { next } => [Some(next), None, None],
            Self::PublishReturn | Self::PropagateFailure => [None, None, None],
        };
        targets.into_iter().flatten()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Node {
    pub kind: Kind,
    pub span: m::Span,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    nodes: Vec<Node>,
    entries: Vec<NodeId>,
}
impl Function {
    pub const fn entry(&self) -> NodeId {
        NodeId(0)
    }
    pub const fn failure(&self) -> NodeId {
        NodeId(1)
    }
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }
    pub fn source_entries(&self) -> &[NodeId] {
        &self.entries
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Elaborated {
    source: loans::LoanCheckedBodies,
    functions: Vec<Function>,
    work: usize,
    bytes: usize,
    elements: usize,
}
impl Elaborated {
    pub fn source(&self) -> &loans::LoanCheckedBodies {
        &self.source
    }
    pub fn functions(&self) -> &[Function] {
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
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    pub work: usize,
    pub allocations: usize,
    pub elements: usize,
    pub nodes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        let limits = m::Limits::default();
        Self {
            work: limits.work,
            allocations: limits.allocations,
            elements: limits.elements,
            nodes: 1_000_000,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Resource(m::ErrorKind),
    Nodes { requested: usize, limit: usize },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub primary: Option<m::Span>,
}
impl From<m::Error> for Error {
    fn from(error: m::Error) -> Self {
        Self {
            kind: ErrorKind::Resource(error.kind),
            primary: None,
        }
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ownership MIR failure elaboration failed: {:?}",
            self.kind
        )
    }
}
impl std::error::Error for Error {}
fn add(a: usize, b: usize) -> Result<usize, Error> {
    a.checked_add(b)
        .ok_or_else(|| m::Error::new(m::ErrorKind::ArithmeticOverflow).into())
}
fn id(value: usize) -> Result<NodeId, Error> {
    u32::try_from(value)
        .map(NodeId)
        .map_err(|_| m::Error::new(m::ErrorKind::ArithmeticOverflow).into())
}
/// These are generated storage/registration cleanup, not additional guest steps.
pub fn bookkeeping(op: &b::Op) -> bool {
    matches!(
        op,
        b::Op::Cleanup(_)
            | b::Op::EndBorrow { .. }
            | b::Op::ArenaExit { .. }
            | b::Op::DropValueIfAvailable(_)
            | b::Op::DropLocalIfInitialized(_)
            | b::Op::DiscardPrepared(_)
    )
}
fn suffix(block: &b::Block) -> usize {
    if !matches!(
        block.terminator.kind,
        b::Term::Return(_) | b::Term::Unreachable
    ) {
        return block.instructions.len();
    }
    block
        .instructions
        .iter()
        .rposition(|i| !bookkeeping(&i.op))
        .map_or(0, |i| i + 1)
}
fn block_count(block: &b::Block, budget: &mut m::Budget) -> Result<usize, Error> {
    budget.charge(block.instructions.len())?;
    let mut count = match block.terminator.kind {
        b::Term::Return(_) => 5,
        b::Term::Branch { .. } => 4,
        b::Term::Jump(_) | b::Term::Unreachable => 2,
    };
    let instructions = if block.terminator.kind == b::Term::Unreachable {
        &block.instructions[..suffix(block)]
    } else {
        &block.instructions[..]
    };
    for instruction in instructions {
        budget.charge(1)?;
        count = add(count, if bookkeeping(&instruction.op) { 1 } else { 2 })?;
    }
    Ok(count)
}
pub fn elaborate(source: loans::LoanCheckedBodies, limits: Limits) -> Result<Elaborated, Error> {
    let mut budget = m::Budget::with_usage(
        m::Limits {
            work: limits.work,
            allocations: limits.allocations,
            elements: limits.elements,
            ..Default::default()
        },
        m::Usage {
            work: source.work_used(),
            allocation_bytes: source.requested_bytes(),
            elements: source.elements_used(),
        },
    )?;
    let typed = source.activity().moved().typed();
    let mut functions = budget.vector(typed.functions().len())?;
    let mut total = 0usize;
    for (index, body) in typed.functions().iter().enumerate() {
        let declaration = &typed.places().declarations().functions()[index];
        let result = (|| {
            let mut entries = budget.vector(body.blocks.len())?;
            let mut count = 6;
            for block in &body.blocks {
                budget.charge(1)?;
                entries.push(id(count)?);
                count = add(count, block_count(block, &mut budget)?)?;
            }
            id(count)?;
            total = add(total, count)?;
            if total > limits.nodes {
                return Err(Error {
                    kind: ErrorKind::Nodes {
                        requested: total,
                        limit: limits.nodes,
                    },
                    primary: Some(declaration.span),
                });
            }
            let mut nodes = budget.vector(count)?;
            let fail = NodeId(1);
            {
                let mut emit = |kind, span| -> Result<(), Error> {
                    budget.charge(1)?;
                    nodes.push(Node { kind, span });
                    Ok(())
                };
                emit(
                    Kind::Entry {
                        next: entries[0],
                        failure: fail,
                    },
                    declaration.signature_span,
                )?;
                emit(Kind::RecordFailure { next: NodeId(2) }, declaration.span)?;
                emit(Kind::DiscardPrepared { next: NodeId(3) }, declaration.span)?;
                emit(Kind::RetireFailure { next: NodeId(4) }, declaration.span)?;
                emit(Kind::DiscardResult { next: NodeId(5) }, declaration.span)?;
                emit(Kind::PropagateFailure, declaration.span)?;
            }
            for (block_index, block) in body.blocks.iter().enumerate() {
                let original = m::BlockId(block_index as u32);
                let split = suffix(block);
                for (order, instruction) in block.instructions[..split].iter().enumerate() {
                    instruction_nodes(
                        &mut nodes,
                        &mut budget,
                        instruction,
                        b::Site {
                            block: original,
                            order: order + 1,
                        },
                        fail,
                    )?;
                }
                let next = id(add(nodes.len(), 1)?)?;
                push(
                    &mut nodes,
                    &mut budget,
                    Kind::Fuel {
                        position: Position::Terminator(original),
                        next,
                        failure: fail,
                    },
                    block.terminator.span,
                )?;
                let span = block.terminator.span;
                match &block.terminator.kind {
                    b::Term::Return(value) => {
                        let next = id(add(nodes.len(), 1)?)?;
                        push(
                            &mut nodes,
                            &mut budget,
                            Kind::StageReturn {
                                value: *value,
                                next,
                                failure: fail,
                            },
                            span,
                        )?;
                        for (offset, instruction) in block.instructions[split..].iter().enumerate()
                        {
                            instruction_nodes(
                                &mut nodes,
                                &mut budget,
                                instruction,
                                b::Site {
                                    block: original,
                                    order: split + offset + 1,
                                },
                                fail,
                            )?;
                        }
                        let next = id(add(nodes.len(), 1)?)?;
                        push(
                            &mut nodes,
                            &mut budget,
                            Kind::DiscardPrepared { next },
                            span,
                        )?;
                        let next = id(add(nodes.len(), 1)?)?;
                        push(
                            &mut nodes,
                            &mut budget,
                            Kind::RetireReturn {
                                next,
                                failure: fail,
                            },
                            span,
                        )?;
                        push(&mut nodes, &mut budget, Kind::PublishReturn, span)?;
                    }
                    b::Term::Jump(target) => push(
                        &mut nodes,
                        &mut budget,
                        Kind::Transfer {
                            block: original,
                            arm: 0,
                            target: entries[target.block.0 as usize],
                            failure: fail,
                        },
                        span,
                    )?,
                    b::Term::Branch {
                        condition,
                        then_target,
                        else_target,
                    } => {
                        let then_node = id(add(nodes.len(), 1)?)?;
                        let else_node = id(add(nodes.len(), 2)?)?;
                        push(
                            &mut nodes,
                            &mut budget,
                            Kind::Branch {
                                condition: *condition,
                                then_node,
                                else_node,
                                failure: fail,
                            },
                            span,
                        )?;
                        push(
                            &mut nodes,
                            &mut budget,
                            Kind::Transfer {
                                block: original,
                                arm: 0,
                                target: entries[then_target.block.0 as usize],
                                failure: fail,
                            },
                            span,
                        )?;
                        push(
                            &mut nodes,
                            &mut budget,
                            Kind::Transfer {
                                block: original,
                                arm: 1,
                                target: entries[else_target.block.0 as usize],
                                failure: fail,
                            },
                            span,
                        )?;
                    }
                    b::Term::Unreachable => push(
                        &mut nodes,
                        &mut budget,
                        Kind::Raise {
                            failure: ExecutionFailure::Invariant,
                            next: fail,
                        },
                        span,
                    )?,
                }
            }
            debug_assert_eq!(nodes.len(), count);
            Ok(Function { nodes, entries })
        })();
        functions.push(result.map_err(|mut error: Error| {
            if error.primary.is_none() {
                error.primary = Some(declaration.span);
            }
            error
        })?);
    }
    Ok(Elaborated {
        source,
        functions,
        work: budget.work_used(),
        bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    })
}
fn push(
    nodes: &mut Vec<Node>,
    budget: &mut m::Budget,
    kind: Kind,
    span: m::Span,
) -> Result<(), Error> {
    budget.charge(1)?;
    nodes.push(Node { kind, span });
    Ok(())
}
fn instruction_nodes(
    nodes: &mut Vec<Node>,
    budget: &mut m::Budget,
    instruction: &b::Instruction,
    site: b::Site,
    failure: NodeId,
) -> Result<(), Error> {
    if !bookkeeping(&instruction.op) {
        let next = id(add(nodes.len(), 1)?)?;
        push(
            nodes,
            budget,
            Kind::Fuel {
                position: Position::Instruction(site),
                next,
                failure,
            },
            instruction.span,
        )?;
    }
    let next = id(add(nodes.len(), 1)?)?;
    push(
        nodes,
        budget,
        Kind::Execute {
            site,
            next,
            failure,
        },
        instruction.span,
    )
}
