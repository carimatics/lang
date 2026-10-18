use super::*;
use o::places::{Place, ProjectionKind, Root};

mod arenas;
mod buffers;
mod replacement;
mod shared;

#[derive(Clone, Copy)]
pub(super) enum Capture {
    Root(usize),
    Path(Target),
}
pub(super) enum ArenaTransfer {
    Copy,
    Moved,
    Rejected,
    Replaced(Option<m::Span>),
}

#[derive(Clone, Copy)]
pub(super) enum Target {
    Allocator,
    Data {
        cell: usize,
        location: l::Location,
    },
    Arena {
        owner: usize,
        read: usize,
        location: l::Location,
    },
    Shared {
        read: usize,
        ty: o::Type,
        location: l::Location,
    },
}
pub(super) struct Cell {
    pub physical: usize,
    pub ty: o::Type,
    pub live: bool,
    pub slot: Option<c::Slot>,
}
pub(super) struct Loan {
    pub generation: u64,
    pub target: Option<Target>,
    pub arena_read: Option<(usize, g::Read)>,
    pub shared_read: Option<sh::Read>,
}
pub(super) struct State<'a, 'layouts, 'types, P: a::Provider> {
    pub allocator: &'a mut a::Allocator<P>,
    pub storage: Option<StackStorage<'layouts, 'types>>,
    pub types: &'types o::VerifiedTypes,
    pub cells: Vec<Cell>,
    pub inverse: Vec<usize>,
    pub loans: Vec<Loan>,
    pub prepared: Vec<Option<Capture>>,
    pub arenas: Vec<Option<g::Arena<'layouts, 'types>>>,
    pub shared_heap: Option<sh::Heap<'layouts, 'types>>,
    pub panic_code: Option<i64>,
    pub current: Address,
}
pub(super) type Step<T> = Result<T, c::ExecutionFailure>;
pub(super) fn invariant<T>(_: T) -> c::ExecutionFailure {
    c::ExecutionFailure::Invariant
}
fn cleanup_leaf<P: a::Provider>(
    types: &o::VerifiedTypes,
    token: &mut l::Token,
    allocator: &mut a::Allocator<P>,
    context: &mut c::Context<'_>,
) -> Result<(), u32> {
    #[cfg(test)]
    let event = super::failure_tests::before_leaf(token, allocator, context);
    buffer::cleanup_token(types, token, allocator).map_err(|_| 3u32)?;
    #[cfg(test)]
    super::failure_tests::after_leaf(event)?;
    let _ = context;
    Ok(())
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
        if let Target::Shared { read, ty, location } = target {
            return self
                .shared_view(read, ty)?
                .read_immediate(location, self.allocator)
                .map_err(invariant);
        }
        if let Target::Arena {
            owner,
            read,
            location,
        } = target
        {
            return self
                .arena_view(owner, read)?
                .read_immediate(location, self.allocator)
                .map_err(invariant);
        }
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
            Target::Shared { read, ty, location } => {
                self.shared_view(read, ty).is_ok()
                    && self.types.equivalent(location.ty(), pointee) == Some(true)
            }
            Target::Arena {
                owner,
                read,
                location,
            } => {
                self.arena_view(owner, read).is_ok()
                    && self.types.equivalent(location.ty(), pointee) == Some(true)
            }
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
        let mut target = self.root(
            self.current.cell
                + match place.root {
                    Root::Local(id) => id.0 as usize,
                    Root::Value(value) => locals + value.0 as usize,
                },
        )?;
        for projection in &place.projections {
            target = match projection.kind {
                ProjectionKind::Dereference => self.reference(target)?,
                ProjectionKind::Field(field) => self.field(target, field)?,
            };
        }
        Ok(target)
    }
    pub fn field(&self, target: Target, field: u32) -> Step<Target> {
        if let Target::Shared { read, ty, location } = target {
            let location = self
                .shared_view(read, ty)?
                .child(location, field, self.allocator)
                .map_err(invariant)?;
            return Ok(Target::Shared { read, ty, location });
        }
        if let Target::Arena {
            owner,
            read,
            location,
        } = target
        {
            let location = self
                .arena_view(owner, read)?
                .child(location, field, self.allocator)
                .map_err(invariant)?;
            return Ok(Target::Arena {
                owner,
                read,
                location,
            });
        }
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
        if let Target::Shared { read, ty, location } = target {
            return self
                .shared_view(read, ty)?
                .case(location, self.allocator)
                .map_err(invariant);
        }
        if let Target::Arena {
            owner,
            read,
            location,
        } = target
        {
            return self
                .arena_view(owner, read)?
                .case(location, self.allocator)
                .map_err(invariant);
        }
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
        let id = self.current.loan + id;
        let token = self.loan_token(id, target)?;
        let physical = self.cells[destination].physical;
        self.storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(physical)
            .map_err(invariant)?
            .data_mut(self.allocator)
            .map_err(invariant)?
            .copy_from_slice(&token);
        self.cells[destination].live = true;
        Ok(())
    }
    pub(super) fn loan_token(&mut self, id: usize, target: Target) -> Step<[u8; 32]> {
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
        loan.target = Some(target);
        Ok(token)
    }
    pub fn copy(&mut self, from: Target, destination: usize) -> Step<()> {
        if let Target::Shared { read, ty, location } = from {
            let value = self.loans[read]
                .shared_read
                .ok_or(c::ExecutionFailure::Invariant)?;
            let target = self
                .storage
                .as_ref()
                .unwrap()
                .cell(self.cells[destination].physical)
                .map_err(invariant)?;
            self.shared_heap
                .as_ref()
                .unwrap()
                .view(value, ty, self.allocator)
                .map_err(invariant)?
                .copy_to(location, target, target.root(), self.allocator)
                .map_err(invariant)?;
            self.cells[destination].live = true;
            return Ok(());
        }
        if let Target::Arena {
            owner,
            read,
            location,
        } = from
        {
            let storage = self
                .storage
                .as_ref()
                .ok_or(c::ExecutionFailure::Invariant)?;
            let to = storage
                .cell(self.cells[destination].physical)
                .map_err(invariant)?;
            let (source, token) = self.loans[read]
                .arena_read
                .as_ref()
                .ok_or(c::ExecutionFailure::Invariant)?;
            if *source != owner {
                return Err(c::ExecutionFailure::Invariant);
            }
            self.arenas[owner]
                .as_ref()
                .ok_or(c::ExecutionFailure::Invariant)?
                .payload(token, self.allocator)
                .map_err(invariant)?
                .copy_into(location, to, to.root(), self.allocator)
                .map_err(invariant)?;
            self.cells[destination].live = true;
            return Ok(());
        }
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
    /// Relocate into another activation/return escrow, keeping physical frame
    /// regions in their original pool. The caller retargets the ledger ticket.
    pub fn relocate(&mut self, from: usize, to: usize) -> Step<()> {
        self.root(from)?;
        if self.cells[to].live
            || self
                .types
                .equivalent(self.cells[from].ty, self.cells[to].ty)
                != Some(true)
        {
            return Err(c::ExecutionFailure::Invariant);
        }
        let (input, output) = self
            .storage
            .as_mut()
            .ok_or(c::ExecutionFailure::Invariant)?
            .two_cells_mut(self.cells[from].physical, self.cells[to].physical)
            .map_err(invariant)?;
        input
            .move_root_into(output, output.root(), self.allocator)
            .map_err(invariant)?;
        input.recycle(self.allocator).map_err(invariant)?;
        self.cells[from].live = false;
        self.cells[to].live = true;
        Ok(())
    }
    pub fn is_copy(&self, index: usize) -> bool {
        self.types.properties(self.cells[index].ty).unwrap().is_copy
    }
    pub fn extract(
        &mut self,
        source: usize,
        destination: usize,
        contract: o::core::Extraction,
    ) -> Step<()> {
        if self.cells[destination].live && !self.is_copy(destination) {
            return Err(c::ExecutionFailure::Invariant);
        }
        let root = self.root(source)?;
        if self.case(root)? != contract.case {
            self.panic_code = Some(contract.panic_code);
            return Err(c::ExecutionFailure::Panic);
        }
        if self.is_copy(source) {
            self.copy(self.field(root, 0)?, destination)?;
        } else {
            let from = self.cells[source].physical;
            let to = self.cells[destination].physical;
            let (input, output) = self
                .storage
                .as_mut()
                .ok_or(c::ExecutionFailure::Invariant)?
                .two_cells_mut(from, to)
                .map_err(invariant)?;
            input
                .extract_case_into(contract.case, output, self.allocator)
                .map_err(invariant)?;
            input.recycle(self.allocator).map_err(invariant)?;
            self.cells[source].live = false;
            self.cells[destination].live = true;
        }
        Ok(())
    }
    pub fn aggregate(
        &mut self,
        destination: usize,
        case: Option<u32>,
        fields: impl ExactSizeIterator<Item = usize> + Clone,
        order: impl ExactSizeIterator<Item = (u32, m::Span)>,
    ) -> Step<()> {
        if self.cells[destination].live && !self.is_copy(destination) {
            return Err(c::ExecutionFailure::Invariant);
        }
        if let (Some(case), o::Type::Constructed(id)) = (case, self.cells[destination].ty)
            && let o::Constructor::Builtin(builtin) =
                self.types.table().constructed[id.0 as usize].constructor
            && matches!(
                builtin,
                o::Builtin::AllocError
                    | o::Builtin::BoundsError
                    | o::Builtin::BufferError
                    | o::Builtin::Utf8Error
                    | o::Builtin::OwnershipError
                    | o::Builtin::ArenaError
            )
        {
            if fields.len() > 1 {
                return Err(c::ExecutionFailure::Invariant);
            }
            let payload = fields
                .clone()
                .next()
                .map(|index| self.root(index).and_then(|target| self.read(target)))
                .transpose()?;
            return self.write(
                destination,
                l::Immediate::from_case(builtin, case, payload).map_err(invariant)?,
            );
        }
        let target = self.cells[destination].physical;
        let output = self
            .storage
            .as_ref()
            .ok_or(c::ExecutionFailure::Invariant)?
            .cell(target)
            .map_err(invariant)?;
        let root = output.root();
        if let Some(case) = case {
            output
                .prepare_case(root, case, self.allocator)
                .map_err(invariant)?;
        }
        for (field, source) in fields.clone().enumerate() {
            let input = self.root(source)?;
            let Target::Data { location, .. } = input else {
                return Err(c::ExecutionFailure::Invariant);
            };
            let child = output
                .child(root, field as u32, self.allocator)
                .map_err(invariant)?;
            if self.types.equivalent(location.ty(), child.ty()) != Some(true) {
                return Err(c::ExecutionFailure::Invariant);
            }
        }
        // Validate complete written order/spans before transferring obligations.
        // All payload/order/work storage was admitted before guest execution.
        output
            .initialize_order(root, order, self.allocator)
            .map_err(invariant)?;
        for (field, source) in fields.enumerate() {
            let from = self.cells[source].physical;
            let output = self
                .storage
                .as_ref()
                .unwrap()
                .cell(target)
                .map_err(invariant)?;
            let child = output
                .child(root, field as u32, self.allocator)
                .map_err(invariant)?;
            if self.is_copy(source) {
                let input = self
                    .storage
                    .as_ref()
                    .unwrap()
                    .cell(from)
                    .map_err(invariant)?;
                output
                    .copy_from(child, input, input.root(), self.allocator)
                    .map_err(invariant)?;
            } else {
                let (input, output) = self
                    .storage
                    .as_mut()
                    .unwrap()
                    .two_cells_mut(from, target)
                    .map_err(invariant)?;
                input
                    .move_root_into(output, child, self.allocator)
                    .map_err(invariant)?;
                input.recycle(self.allocator).map_err(invariant)?;
                self.cells[source].live = false;
            }
        }
        self.cells[destination].live = true;
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
        let target = self.reference(self.root(reference)?)?;
        let mut token = self.token(target)?;
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
                let target = self.loans[id].target.take();
                if let Some((owner, mut read)) = self.loans[id].arena_read.take() {
                    self.arenas[owner]
                        .as_mut()
                        .ok_or(6u32)?
                        .end_read(&mut read, self.allocator)
                        .map_err(|_| 7u32)?;
                }
                if let Some(read) = self.loans[id].shared_read.take() {
                    let Some(Target::Shared { ty, .. }) = target else {
                        return Err(9u32);
                    };
                    self.shared_heap
                        .as_mut()
                        .ok_or(9u32)?
                        .end_read(
                            read,
                            ty,
                            self.allocator,
                            context,
                            |token, allocator, _, context| {
                                cleanup_leaf(self.types, token, allocator, context)
                            },
                        )
                        .map_err(|_| 10u32)?;
                }
            }
            Ticket::Cell(physical) => {
                let logical = self.inverse[physical];
                self.cells[logical].live = false;
                let arena_owner = matches!(self.cells[logical].ty, o::Type::Constructed(id)
                    if matches!(self.types.table().constructed[id.0 as usize].constructor, o::Constructor::ArenaOwner(_)));
                if arena_owner {
                    let arena = self
                        .arenas
                        .get_mut(physical)
                        .and_then(Option::as_mut)
                        .ok_or(8u32)?;
                    arena
                        .cleanup(
                            if context.reason() == c::Reason::Failure {
                                g::Exit::Abort
                            } else {
                                g::Exit::Normal
                            },
                            self.allocator,
                            context,
                            |token, allocator, _, context| {
                                shared::cleanup_services(
                                    self.types,
                                    &mut self.shared_heap,
                                    token,
                                    allocator,
                                    context,
                                )
                            },
                        )
                        .map_err(|_| 4u32)?;
                    return Ok(());
                }
                let cell = self
                    .storage
                    .as_mut()
                    .ok_or(1u32)?
                    .cell_mut(physical)
                    .map_err(|_| 2u32)?;
                cell.retire(self.allocator, context, |token, allocator, _, context| {
                    shared::cleanup_services(
                        self.types,
                        &mut self.shared_heap,
                        token,
                        allocator,
                        context,
                    )
                })
                .map_err(|_| 4u32)?;
                cell.recycle(self.allocator).map_err(|_| 5u32)?;
            }
        }
        Ok(())
    }
}
