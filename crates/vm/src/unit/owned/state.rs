use super::*;
use o::places::{Place, ProjectionKind, Root};

#[derive(Clone, Copy)]
pub(super) enum Target {
    Allocator,
    Data { cell: usize, location: l::Location },
}
pub(super) struct Cell {
    pub physical: usize,
    pub ty: o::Type,
    pub live: bool,
}
pub(super) struct Loan {
    pub generation: u64,
    pub target: Option<Target>,
}
pub(super) struct State<'a, 'layouts, 'types, P: a::Provider> {
    pub allocator: &'a mut a::Allocator<P>,
    pub storage: Option<l::FrameStorage<'layouts, 'types>>,
    pub types: &'types o::VerifiedTypes,
    pub cells: Vec<Cell>,
    pub inverse: Vec<usize>,
    pub loans: Vec<Loan>,
}
pub(super) type Step<T> = Result<T, c::ExecutionFailure>;
pub(super) fn invariant<T>(_: T) -> c::ExecutionFailure {
    c::ExecutionFailure::Invariant
}
impl<P: a::Provider> State<'_, '_, '_, P> {
    pub fn root(&self, index: usize) -> Step<Target> {
        let cell = self
            .cells
            .get(index)
            .ok_or(c::ExecutionFailure::Invariant)?;
        if !cell.live {
            return Err(c::ExecutionFailure::Invariant);
        }
        let location = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(cell.physical)
            .map_err(invariant)?
            .root();
        Ok(Target::Data {
            cell: cell.physical,
            location,
        })
    }
    pub fn read(&self, target: Target) -> Step<l::Immediate> {
        let Target::Data { cell, location } = target else {
            return Err(c::ExecutionFailure::Invariant);
        };
        self.storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(cell)
            .map_err(invariant)?
            .read_immediate(location, self.allocator)
            .map_err(invariant)
    }
    pub fn write(&mut self, index: usize, value: l::Immediate) -> Step<()> {
        let cell = &self.cells[index];
        let value_cell = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(cell.physical)
            .map_err(invariant)?;
        value_cell
            .write_immediate(value_cell.root(), value, self.allocator)
            .map_err(invariant)?;
        self.cells[index].live = true;
        Ok(())
    }
    fn loan_type(&self, ty: o::Type) -> Option<o::Type> {
        let o::Type::Constructed(id) = ty else {
            return None;
        };
        match self.types.table().constructed[id.0 as usize].constructor {
            o::Constructor::Loan(_, ty) => Some(ty),
            _ => None,
        }
    }
    pub fn reference(&self, target: Target) -> Step<Target> {
        let Target::Data { cell, location } = target else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let pointee = self
            .loan_type(location.ty())
            .ok_or(c::ExecutionFailure::Invariant)?;
        let data = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(cell)
            .map_err(invariant)?
            .data(self.allocator)
            .map_err(invariant)?;
        let start = location.offset() as usize;
        let bytes = data
            .get(
                start
                    ..start
                        .checked_add(16)
                        .ok_or(c::ExecutionFailure::Invariant)?,
            )
            .ok_or(c::ExecutionFailure::Invariant)?;
        let id = usize::try_from(u64::from_le_bytes(
            bytes[..8].try_into().map_err(invariant)?,
        ))
        .map_err(invariant)?;
        let generation = u64::from_le_bytes(bytes[8..].try_into().map_err(invariant)?);
        let loan = self.loans.get(id).ok_or(c::ExecutionFailure::Invariant)?;
        if loan.generation != generation {
            return Err(c::ExecutionFailure::Invariant);
        }
        let target = loan.target.ok_or(c::ExecutionFailure::Invariant)?;
        let valid = match target {
            Target::Data { cell, location } => {
                self.cells[self.inverse[cell]].live
                    && self.types.equivalent(location.ty(), pointee) == Some(true)
            }
            Target::Allocator => match pointee {
                o::Type::Constructed(id) => {
                    self.types.table().constructed[id.0 as usize].constructor
                        == o::Constructor::Builtin(o::Builtin::Allocator)
                }
                _ => false,
            },
        };
        if !valid {
            return Err(c::ExecutionFailure::Invariant);
        }
        Ok(target)
    }
    pub fn referent(&self, index: usize) -> Step<Target> {
        let target = self.root(index)?;
        if self.loan_type(self.cells[index].ty).is_some() {
            self.reference(target)
        } else {
            Ok(target)
        }
    }
    pub fn place(&self, place: &Place, locals: usize) -> Step<Target> {
        let mut target = self.root(match place.root {
            Root::Local(id) => id.0 as usize,
            Root::Value(value) => locals + value.0 as usize,
        })?;
        for projection in &place.projections {
            target = match projection.kind {
                ProjectionKind::Dereference => self.reference(target)?,
                ProjectionKind::Field(field) => self.field(target, field)?,
            };
        }
        Ok(target)
    }
    pub fn field(&self, target: Target, field: u32) -> Step<Target> {
        let Target::Data { cell, location } = target else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let location = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(cell)
            .map_err(invariant)?
            .child(location, field, self.allocator)
            .map_err(invariant)?;
        Ok(Target::Data { cell, location })
    }
    pub fn case(&self, target: Target) -> Step<u32> {
        let Target::Data { cell, location } = target else {
            return Err(c::ExecutionFailure::Invariant);
        };
        self.storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(cell)
            .map_err(invariant)?
            .case(location, self.allocator)
            .map_err(invariant)
    }
    pub fn borrow(&mut self, destination: usize, id: usize, target: Target) -> Step<()> {
        let loan = &mut self.loans[id];
        if loan.target.is_some() {
            return Err(c::ExecutionFailure::Invariant);
        }
        loan.generation = loan
            .generation
            .checked_add(1)
            .ok_or(c::ExecutionFailure::CounterOverflow)?;
        let mut token = [0u8; 32];
        token[..8].copy_from_slice(&(id as u64).to_le_bytes());
        token[8..16].copy_from_slice(&loan.generation.to_le_bytes());
        let physical = self.cells[destination].physical;
        self.storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(physical)
            .map_err(invariant)?
            .data_mut(self.allocator)
            .map_err(invariant)?
            .copy_from_slice(&token);
        loan.target = Some(target);
        self.cells[destination].live = true;
        Ok(())
    }
    pub fn copy(&mut self, from: Target, destination: usize) -> Step<()> {
        let Target::Data { cell, location } = from else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let storage = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?;
        let to = storage
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        to.copy_from(
            to.root(),
            storage.cell(cell).map_err(invariant)?,
            location,
            self.allocator,
        )
        .map_err(invariant)?;
        self.cells[destination].live = true;
        Ok(())
    }
    pub fn swap_move(&mut self, from: usize, to: usize) -> Step<()> {
        self.root(from)?;
        if self.cells[to].live
            || self
                .types
                .equivalent(self.cells[from].ty, self.cells[to].ty)
                != Some(true)
        {
            return Err(c::ExecutionFailure::Invariant);
        }
        let a = self.cells[from].physical;
        let b = self.cells[to].physical;
        self.cells[from].physical = b;
        self.cells[to].physical = a;
        self.inverse[a] = to;
        self.inverse[b] = from;
        self.cells[from].live = false;
        self.cells[to].live = true;
        Ok(())
    }
    pub fn bytes_new(&mut self, destination: usize, span: m::Span) -> Step<()> {
        if self.cells[destination].live {
            return Err(c::ExecutionFailure::Invariant);
        }
        let outcome = buffer::Bytes::new(self.allocator);
        let cell = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(self.cells[destination].physical)
            .map_err(invariant)?;
        let root = cell.root();
        match outcome {
            Ok(mut bytes) => {
                cell.prepare_case(root, 0, self.allocator)
                    .map_err(invariant)?;
                let payload = cell.child(root, 0, self.allocator).map_err(invariant)?;
                let token = bytes.take_token(self.allocator).map_err(invariant)?;
                let start = payload.offset() as usize;
                cell.data_mut(self.allocator)
                    .map_err(invariant)?
                    .get_mut(start..start + 32)
                    .ok_or(c::ExecutionFailure::Invariant)?
                    .copy_from_slice(&token);
            }
            Err(buffer::Failure::Error(error)) => {
                cell.prepare_case(root, 1, self.allocator)
                    .map_err(invariant)?;
                let payload = cell.child(root, 0, self.allocator).map_err(invariant)?;
                cell.write_immediate(payload, l::Immediate::Allocation(error), self.allocator)
                    .map_err(invariant)?;
            }
            Err(buffer::Failure::Fault(_)) => return Err(c::ExecutionFailure::Fault),
        }
        cell.initialize_order(root, [(0, span)].into_iter(), self.allocator)
            .map_err(invariant)?;
        self.cells[destination].live = true;
        Ok(())
    }
    pub fn bytes_len(&self, reference: usize) -> Step<i64> {
        let Target::Data { cell, location } = self.reference(self.root(reference)?)? else {
            return Err(c::ExecutionFailure::Invariant);
        };
        let start = location.offset() as usize;
        let mut token = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(cell)
            .map_err(invariant)?
            .data(self.allocator)
            .map_err(invariant)?
            .get(start..start + 32)
            .ok_or(c::ExecutionFailure::Invariant)?
            .try_into()
            .map_err(invariant)?;
        let bytes = buffer::Bytes::from_token(&mut token, self.allocator).map_err(invariant)?;
        bytes.len(self.allocator).map_err(invariant)
    }
    pub fn cleanup(
        &mut self,
        ticket: &mut Ticket,
        context: &mut c::Context<'_>,
    ) -> Result<(), u32> {
        match *ticket {
            Ticket::Loan(id) => {
                self.loans[id].target = None;
            }
            Ticket::Cell(physical) => {
                self.cells[self.inverse[physical]].live = false;
                let cell = self
                    .storage
                    .as_mut()
                    .ok_or(1u32)?
                    .cell_mut(physical)
                    .map_err(|_| 2u32)?;
                cell.retire(self.allocator, context, |token, allocator, _, _| {
                    buffer::cleanup_token(self.types, token, allocator).map_err(|_| 3u32)
                })
                .map_err(|_| 4u32)?;
                cell.recycle(self.allocator).map_err(|_| 5u32)?;
            }
        }
        Ok(())
    }
}
