//! Pre-effect frame storage admission. The compiler plan and fallibly reserved
//! host wrapper pool are bounded separately from guest payload/drop requests.
//! This reserves storage, not initialization, loan authority or a cleanup ledger.
use super::*;
use allocation::{Allocator, Provider, Request};
use o::{bodies::TypedBodies, declarations::LocalId};

pub struct FramePlan<'layouts, 'types> {
    layouts: &'layouts Layouts<'types>,
    cells: Vec<o::TypeUse>,
    requests: Vec<Request>,
    locals: usize,
    values: usize,
    cleanup_slots: usize,
    limits: m::Limits,
    usage: m::Usage,
}
pub struct FrameStorage<'layouts, 'types> {
    cells: Vec<Reservation<'layouts, 'types>>,
    usage: m::Usage,
    released: bool,
}
fn usage(budget: &m::Budget) -> m::Usage {
    m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    }
}
fn combined(a: usize, b: usize, common: usize) -> Result<usize, Error> {
    a.checked_sub(common)
        .and_then(|extra| extra.checked_add(b))
        .ok_or(Error::Overflow)
}
impl<'layouts, 'types> FramePlan<'layouts, 'types> {
    /// Include locals, SSA values, explicit loan registrations and return escrow
    /// in the cell limit. Loans have no separate payload storage. Host compiler
    /// and wrapper pools use cumulative fallible MIR allocation/work budgets.
    pub fn compile(
        layouts: &'layouts Layouts<'types>,
        typed: &TypedBodies,
        function: m::FunctionId,
        limits: m::Limits,
        cell_limit: usize,
    ) -> Result<Self, Error> {
        let types = typed.places().declarations().types();
        if !std::ptr::eq(layouts.types, types) {
            return Err(Error::ForeignLocation);
        }
        let index = function.0 as usize;
        let declaration = typed
            .places()
            .declarations()
            .functions()
            .get(index)
            .ok_or(Error::InvalidFunction(function))?;
        let values = &typed.places().functions()[index].values;
        let count = declaration
            .locals
            .len()
            .checked_add(values.len())
            .and_then(|n| n.checked_add(1))
            .ok_or(Error::Overflow)?;
        let cleanup_slots = count
            .checked_sub(1)
            .and_then(|n| n.checked_add(typed.functions()[index].loans))
            .ok_or(Error::Overflow)?;
        let needed = cleanup_slots.checked_add(1).ok_or(Error::Overflow)?;
        if needed > cell_limit {
            return Err(Error::CellLimit {
                cells: needed,
                limit: cell_limit,
            });
        }
        // Both branches already include the verified type graph. Count it once.
        let base = layouts.usage();
        let initial = m::Usage {
            work: combined(base.work, typed.work_used(), types.work_used())?,
            allocation_bytes: combined(
                base.allocation_bytes,
                typed.requested_bytes(),
                types.requested_bytes(),
            )?,
            elements: combined(base.elements, typed.elements_used(), types.elements_used())?,
        };
        let mut budget = m::Budget::with_usage(limits, initial)?;
        let mut cells = budget.vector(count)?;
        let mut requests = budget.vector(count.checked_mul(2).ok_or(Error::Overflow)?)?;
        for ty in declaration
            .locals
            .iter()
            .map(|l| l.ty)
            .chain(values.iter().copied())
            .chain([declaration.result])
        {
            budget.charge(1)?;
            let layout = layouts.get(ty.ty).ok_or(Error::InvalidType(ty.ty))?;
            cells.push(ty);
            requests.extend(layout.storage_requests()?);
        }
        Ok(Self {
            layouts,
            cells,
            requests,
            locals: declaration.locals.len(),
            values: values.len(),
            cleanup_slots,
            limits,
            usage: usage(&budget),
        })
    }
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }
    pub const fn cleanup_slots(&self) -> usize {
        self.cleanup_slots
    }
    pub const fn return_cell(&self) -> usize {
        self.locals + self.values
    }
    pub fn local(&self, local: LocalId) -> Result<usize, Error> {
        let index = local.0 as usize;
        if index < self.locals {
            Ok(index)
        } else {
            Err(Error::InvalidCell(index))
        }
    }
    pub fn value(&self, value: m::Register) -> Result<usize, Error> {
        let index = value.0 as usize;
        if index < self.values {
            Ok(self.locals + index)
        } else {
            Err(Error::InvalidCell(index))
        }
    }
    pub fn cell_type(&self, index: usize) -> Option<o::TypeUse> {
        self.cells.get(index).copied()
    }
    pub fn requests(&self) -> &[Request] {
        &self.requests
    }
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }

    /// Reserve every inline value/drop region before parameters or guest effects
    /// are admitted. All configured guest limits are checked before the first
    /// provider callback. OOM cancels completed reservations in reverse order;
    /// cumulative attempted request accounting remains in the allocator.
    pub fn reserve<P: Provider>(
        &self,
        allocator: &mut Allocator<P>,
    ) -> Result<FrameStorage<'layouts, 'types>, Error> {
        allocator.check(&self.requests)?;
        let mut budget = m::Budget::with_usage(self.limits, self.usage)?;
        budget.charge(self.cells.len())?;
        let mut cells: Vec<Reservation<'layouts, 'types>> = budget.vector(self.cells.len())?;
        for ty in &self.cells {
            match Reservation::allocate(self.layouts, ty.ty, allocator) {
                Ok(cell) => cells.push(cell),
                Err(error) => {
                    for cell in cells.iter_mut().rev() {
                        cell.release_storage(allocator)?;
                    }
                    return Err(error);
                }
            }
        }
        Ok(FrameStorage {
            cells,
            usage: usage(&budget),
            released: false,
        })
    }
}
impl<'layouts, 'types> FrameStorage<'layouts, 'types> {
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    pub fn cell(&self, index: usize) -> Result<&Reservation<'layouts, 'types>, Error> {
        if self.released {
            return Err(Error::State);
        }
        self.cells.get(index).ok_or(Error::InvalidCell(index))
    }
    pub fn cell_mut(&mut self, index: usize) -> Result<&mut Reservation<'layouts, 'types>, Error> {
        if self.released {
            return Err(Error::State);
        }
        self.cells.get_mut(index).ok_or(Error::InvalidCell(index))
    }
    /// Cancel unpublished cells or release fully retired storage. The execution
    /// ledger must discharge live owners first. Preflight every cell/handle so a
    /// pending retirement or foreign allocator cannot release a valid prefix.
    pub fn release<P: Provider>(&mut self, allocator: &mut Allocator<P>) -> Result<(), Error> {
        if self.released {
            return Err(Error::State);
        }
        for cell in &self.cells {
            cell.check_release(allocator)?;
        }
        for cell in self.cells.iter_mut().rev() {
            cell.release_storage(allocator)?;
        }
        self.released = true;
        Ok(())
    }
}
