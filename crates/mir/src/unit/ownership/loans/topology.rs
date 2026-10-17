use super::*;
use crate::unit::ownership::{Constructor, VerifiedTypes, declarations as d};
use std::ops::Range;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Cell {
    pub ty: Type,
    pub owner: Owner,
    pub foreign: bool,
    pub leaves: Range<usize>,
    pub children: [Option<usize>; 2],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Parameter {
    pub leaf: usize,
    pub loan: usize,
    pub target: usize,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Topology {
    pub cells: Vec<Cell>,
    pub roots: Vec<usize>,
    pub locals: usize,
    pub values: usize,
    pub leaves: usize,
    pub kinds: Vec<LoanKind>,
    pub origins: Vec<Option<Span>>,
    pub parameters: Vec<Parameter>,
}
pub(super) fn constructor(types: &VerifiedTypes, ty: Type) -> Option<Constructor> {
    if let Type::Constructed(id) = ty {
        Some(types.table().constructed[id.0 as usize].constructor)
    } else {
        None
    }
}
impl Topology {
    pub fn root(&self, slot: moves::Slot) -> Option<usize> {
        let index = match slot {
            moves::Slot::Local(id) if (id.0 as usize) < self.locals => id.0 as usize,
            moves::Slot::Value(id) if (id.0 as usize) < self.values => self.locals + id.0 as usize,
            _ => return None,
        };
        self.roots.get(index).copied()
    }
    pub fn value(&self, id: m::Register) -> usize {
        self.roots[self.locals + id.0 as usize]
    }
    pub fn local(&self, id: d::LocalId) -> usize {
        self.roots[id.0 as usize]
    }
    pub fn build(
        typed: &b::TypedBodies,
        index: usize,
        limits: Limits,
        used: &mut usize,
        budget: &mut Budget,
    ) -> Result<Self, Error> {
        let body = &typed.functions()[index];
        let declaration = &typed.places().declarations().functions()[index];
        let frame = &typed.places().functions()[index];
        let types = typed.places().declarations().types();
        let mut this = Self {
            cells: Vec::new(),
            roots: budget.vector(add(declaration.locals.len(), frame.values.len())?)?,
            locals: declaration.locals.len(),
            values: frame.values.len(),
            leaves: 0,
            kinds: super::super::flow::filled(budget, body.loans, LoanKind::Shared)?,
            origins: super::super::flow::filled(budget, body.loans, None)?,
            parameters: Vec::new(),
        };
        for (i, local) in declaration.locals.iter().enumerate() {
            let node = this.node(
                types,
                local.ty.ty,
                Owner::Local(d::LocalId(i as u32)),
                false,
                0,
                limits,
                used,
                budget,
            )?;
            this.roots.push(node);
        }
        for (i, value) in frame.values.iter().enumerate() {
            let node = this.node(
                types,
                value.ty,
                Owner::Value(m::Register(i as u32)),
                false,
                0,
                limits,
                used,
                budget,
            )?;
            this.roots.push(node);
        }
        for block in &body.blocks {
            for i in &block.instructions {
                budget.charge(1)?;
                if let Some(id) = i.op.created_loan() {
                    this.kinds[id.0 as usize] = match i.op {
                        b::Op::Borrow { kind, .. } => kind,
                        _ => LoanKind::Shared,
                    };
                    this.origins[id.0 as usize] = Some(i.span);
                }
            }
        }
        for parameter in &declaration.parameters {
            let mut cell = this.local(*parameter);
            let mut ty = declaration.locals[parameter.0 as usize].ty.ty;
            let mut level = 0usize;
            while let Some(Constructor::Loan(kind, target)) = constructor(types, ty) {
                budget.charge(1)?;
                if level >= limits.depth.min(256) {
                    return Err(m::Error::new(m::ErrorKind::Limit {
                        kind: m::LimitKind::Depth,
                        requested: level + 1,
                        limit: limits.depth.min(256),
                    })
                    .into());
                }
                let node = this.node(
                    types,
                    target,
                    Owner::Parameter {
                        local: *parameter,
                        level: level as u32,
                    },
                    true,
                    0,
                    limits,
                    used,
                    budget,
                )?;
                let loan = this.kinds.len();
                push(budget, &mut this.kinds, kind)?;
                push(
                    budget,
                    &mut this.origins,
                    Some(declaration.locals[parameter.0 as usize].span),
                )?;
                push(
                    budget,
                    &mut this.parameters,
                    Parameter {
                        leaf: this.cells[cell].leaves.start,
                        loan,
                        target: node,
                    },
                )?;
                cell = node;
                ty = target;
                level += 1;
            }
        }
        u32::try_from(this.kinds.len())
            .map_err(|_| m::Error::new(m::ErrorKind::ArithmeticOverflow))?;
        Ok(this)
    }
    #[allow(clippy::too_many_arguments)]
    fn node(
        &mut self,
        types: &VerifiedTypes,
        ty: Type,
        owner: Owner,
        foreign: bool,
        depth: usize,
        limits: Limits,
        used: &mut usize,
        budget: &mut Budget,
    ) -> Result<usize, Error> {
        budget.charge(1)?;
        if depth >= limits.depth.min(256) {
            return Err(m::Error::new(m::ErrorKind::Limit {
                kind: m::LimitKind::Depth,
                requested: depth + 1,
                limit: limits.depth.min(256),
            })
            .into());
        }
        *used = add(*used, 1)?;
        if *used > limits.nodes {
            return Err(Error {
                kind: ErrorKind::Nodes {
                    requested: *used,
                    limit: limits.nodes,
                },
                primary: None,
                secondary: None,
            });
        }
        let index = self.cells.len();
        let first = self.leaves;
        push(
            budget,
            &mut self.cells,
            Cell {
                ty,
                owner,
                foreign,
                leaves: first..first,
                children: [None; 2],
            },
        )?;
        let mut children = [None; 2];
        if types.properties(ty).unwrap().contains_loan {
            match constructor(types, ty) {
                Some(Constructor::Loan(..)) => self.leaves = add(self.leaves, 1)?,
                Some(Constructor::Option(t)) => {
                    children[1] = Some(self.node(
                        types,
                        t,
                        owner,
                        foreign,
                        depth + 1,
                        limits,
                        used,
                        budget,
                    )?)
                }
                Some(Constructor::Result(t, e)) => {
                    children[0] = Some(self.node(
                        types,
                        t,
                        owner,
                        foreign,
                        depth + 1,
                        limits,
                        used,
                        budget,
                    )?);
                    children[1] = Some(self.node(
                        types,
                        e,
                        owner,
                        foreign,
                        depth + 1,
                        limits,
                        used,
                        budget,
                    )?);
                }
                _ => {
                    unreachable!("verified nominal and heap storage cannot contain ordinary loans")
                }
            }
        }
        self.cells[index].children = children;
        self.cells[index].leaves = first..self.leaves;
        Ok(index)
    }
}
