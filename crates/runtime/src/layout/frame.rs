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
    scratch: Vec<Option<usize>>,
    limits: m::Limits,
    usage: m::Usage,
}
pub struct FrameStorage<'layouts, 'types> {
    cells: Vec<Reservation<'layouts, 'types>>,
    layouts: &'layouts Layouts<'types>,
    capacity: usize,
    preparation: m::Usage,
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
fn covers(outer: m::Usage, inner: m::Usage) -> bool {
    outer.work >= inner.work
        && outer.allocation_bytes >= inner.allocation_bytes
        && outer.elements >= inner.elements
}
impl<'layouts, 'types> FramePlan<'layouts, 'types> {
    /// Include locals, SSA values, block-argument snapshots, explicit loan
    /// registrations and return escrow in the cell limit. Loans have no
    /// separate payload storage. Host compiler
    /// and wrapper pools use cumulative fallible MIR allocation/work budgets.
    pub fn compile(
        layouts: &'layouts Layouts<'types>,
        typed: &TypedBodies,
        function: m::FunctionId,
        limits: m::Limits,
        cell_limit: usize,
    ) -> Result<Self, Error> {
        Self::compile_inner(layouts, typed, function, limits, cell_limit, None, false)
    }
    /// Add this function's plan to cumulative program preparation, whose
    /// verified type/layout/body graph has already been counted once.
    pub fn compile_after(
        layouts: &'layouts Layouts<'types>,
        typed: &TypedBodies,
        function: m::FunctionId,
        limits: m::Limits,
        cell_limit: usize,
        initial: m::Usage,
    ) -> Result<Self, Error> {
        Self::compile_inner(
            layouts,
            typed,
            function,
            limits,
            cell_limit,
            Some(initial),
            false,
        )
    }
    /// Physical task cells require independent task evidence and matching task layouts.
    pub fn compile_tasks(
        layouts: &'layouts Layouts<'types>,
        proof: &o::tasks::VerifiedTasks,
        function: m::FunctionId,
        limits: m::Limits,
        cell_limit: usize,
    ) -> Result<Self, Error> {
        if !layouts.tasks {
            return Err(Error::State);
        }
        Self::compile_inner(
            layouts,
            proof.typed(),
            function,
            limits,
            cell_limit,
            None,
            true,
        )
    }
    pub fn compile_tasks_after(
        layouts: &'layouts Layouts<'types>,
        proof: &o::tasks::VerifiedTasks,
        function: m::FunctionId,
        limits: m::Limits,
        cell_limit: usize,
        initial: m::Usage,
    ) -> Result<Self, Error> {
        if !layouts.tasks {
            return Err(Error::State);
        }
        Self::compile_inner(
            layouts,
            proof.typed(),
            function,
            limits,
            cell_limit,
            Some(initial),
            true,
        )
    }
    fn compile_inner(
        layouts: &'layouts Layouts<'types>,
        typed: &TypedBodies,
        function: m::FunctionId,
        limits: m::Limits,
        cell_limit: usize,
        prefix: Option<m::Usage>,
        tasks: bool,
    ) -> Result<Self, Error> {
        let types = typed.places().declarations().types();
        if !std::ptr::eq(layouts.types, types) {
            return Err(Error::ForeignLocation);
        }
        if let Some(span) = typed.places().declarations().task_feature_span()
            && !tasks
        {
            return Err(Error::UnsupportedTaskProfile { span });
        }
        let index = function.0 as usize;
        let declaration = typed
            .places()
            .declarations()
            .functions()
            .get(index)
            .ok_or(Error::InvalidFunction(function))?;
        let values = &typed.places().functions()[index].values;
        let parameters = typed.functions()[index]
            .blocks
            .iter()
            .flat_map(|b| &b.parameters);
        let scratch_count = parameters.clone().count();
        let count = declaration
            .locals
            .len()
            .checked_add(values.len())
            .and_then(|n| n.checked_add(1))
            .and_then(|n| n.checked_add(scratch_count))
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
        let initial = if let Some(prefix) = prefix {
            if !covers(prefix, initial) {
                return Err(Error::State);
            }
            prefix
        } else {
            initial
        };
        let mut budget = m::Budget::with_usage(limits, initial)?;
        let mut cells = budget.vector(count)?;
        let mut requests = budget.vector(count.checked_mul(2).ok_or(Error::Overflow)?)?;
        let mut scratch = budget.vector(if scratch_count == 0 { 0 } else { values.len() })?;
        if scratch_count != 0 {
            scratch.resize(values.len(), None);
            let start = declaration.locals.len() + values.len() + 1;
            for (offset, parameter) in parameters.clone().enumerate() {
                budget.charge(1)?;
                scratch[parameter.0 as usize] = Some(start + offset);
            }
        }
        for ty in declaration
            .locals
            .iter()
            .map(|l| l.ty)
            .chain(values.iter().copied())
            .chain([declaration.result])
            .chain(parameters.map(|parameter| values[parameter.0 as usize]))
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
            scratch,
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
    /// Snapshot reserved for this block parameter, after the return escrow.
    pub fn scratch_cell(&self, parameter: m::Register) -> Option<usize> {
        self.scratch.get(parameter.0 as usize).copied().flatten()
    }
    /// Scratch obligations follow the original MIR owner/loan slots, so the
    /// cleanup mapping for source values and loan registrations stays stable.
    pub fn scratch_slot(&self, parameter: m::Register) -> Option<usize> {
        let cell = self.scratch_cell(parameter)?;
        let count = self.cells.len() - self.return_cell() - 1;
        let offset = cell - self.return_cell() - 1;
        Some(self.cleanup_slots - count + offset)
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
        self.reserve_after(allocator, self.usage)
    }
    /// Resume accounting after an execution adapter's bounded pre-entry pools.
    pub fn reserve_after<P: Provider>(
        &self,
        allocator: &mut Allocator<P>,
        initial: m::Usage,
    ) -> Result<FrameStorage<'layouts, 'types>, Error> {
        if !covers(initial, self.usage) {
            return Err(Error::State);
        }
        allocator.check(&self.requests)?;
        let mut pool = self.prepare_pool_after(initial)?;
        pool.admit(self, allocator)?;
        Ok(pool)
    }

    /// Prepare only bounded host wrappers, without an allocator callback or
    /// guest region. Prepare before outermost guest entry; admission can then
    /// reuse this capacity for plans from the same layouts/type graph.
    pub fn prepare_pool(&self) -> Result<FrameStorage<'layouts, 'types>, Error> {
        self.prepare_pool_after(self.usage)
    }
    /// Resume a caller's cumulative compiler/host-pool preparation budget.
    pub fn prepare_pool_after(
        &self,
        initial: m::Usage,
    ) -> Result<FrameStorage<'layouts, 'types>, Error> {
        if !covers(initial, self.usage) {
            return Err(Error::State);
        }
        let mut budget = m::Budget::with_usage(self.limits, initial)?;
        budget.charge(self.cells.len())?;
        let cells = budget.vector(self.cells.len())?;
        Ok(FrameStorage {
            cells,
            layouts: self.layouts,
            capacity: self.cells.len(),
            preparation: initial,
            usage: usage(&budget),
            released: true,
        })
    }
}
impl<'layouts, 'types> FrameStorage<'layouts, 'types> {
    pub const fn usage(&self) -> m::Usage {
        self.usage
    }
    /// Logical wrapper capacity charged at preparation, not allocator rounding.
    pub const fn cell_capacity(&self) -> usize {
        self.capacity
    }
    /// Admit guest regions into a prepared/released pool without growing its
    /// host vector. The caller supplies entry/parameter ownership authority.
    /// Failure releases the completed prefix and leaves the pool reusable.
    pub fn admit<P: Provider>(
        &mut self,
        plan: &FramePlan<'layouts, 'types>,
        allocator: &mut Allocator<P>,
    ) -> Result<(), Error> {
        if !self.released {
            return Err(Error::State);
        }
        if !std::ptr::eq(self.layouts, plan.layouts) {
            return Err(Error::ForeignLocation);
        }
        if plan.cell_count() > self.capacity {
            return Err(Error::FrameCapacity {
                cells: plan.cell_count(),
                capacity: self.capacity,
            });
        }
        if !covers(self.preparation, plan.usage) {
            return Err(Error::State);
        }
        allocator.check(&plan.requests)?;
        // Old wrappers contain only released regions. Keep their allocation;
        // every push below fits the separately budgeted logical capacity.
        self.cells.clear();
        for ty in &plan.cells {
            match Reservation::allocate(plan.layouts, ty.ty, allocator) {
                Ok(cell) => self.cells.push(cell),
                Err(error) => {
                    for cell in self.cells.iter_mut().rev() {
                        cell.release_storage(allocator)?;
                    }
                    self.cells.clear();
                    return Err(error);
                }
            }
        }
        self.released = false;
        Ok(())
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
    /// Borrow distinct admitted cells for an allocation-free ownership transfer.
    pub fn two_cells_mut(
        &mut self,
        first: usize,
        second: usize,
    ) -> Result<
        (
            &mut Reservation<'layouts, 'types>,
            &mut Reservation<'layouts, 'types>,
        ),
        Error,
    > {
        if self.released || first == second {
            return Err(Error::State);
        }
        for index in [first, second] {
            if index >= self.cells.len() {
                return Err(Error::InvalidCell(index));
            }
        }
        if first < second {
            let (left, right) = self.cells.split_at_mut(second);
            Ok((&mut left[first], &mut right[0]))
        } else {
            let (left, right) = self.cells.split_at_mut(first);
            Ok((&mut right[0], &mut left[second]))
        }
    }
    /// Borrow three distinct admitted cells without allocating wrapper storage.
    pub fn three_cells_mut(
        &mut self,
        ids: [usize; 3],
    ) -> Result<[&mut Reservation<'layouts, 'types>; 3], Error> {
        if self.released {
            return Err(Error::State);
        }
        for id in ids {
            if id >= self.cells.len() {
                return Err(Error::InvalidCell(id));
            }
        }
        self.cells.get_disjoint_mut(ids).map_err(|_| Error::State)
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

    /// Terminal frame cleanup: release all other cells and detach protected escrow.
    /// Owners in discarded cells must already be retired; the retained reservation
    /// keeps its typed storage until a scope retrieves or retires the result.
    pub fn release_except<P: Provider>(
        &mut self,
        keep: usize,
        allocator: &mut Allocator<P>,
    ) -> Result<Reservation<'layouts, 'types>, Error> {
        if self.released || keep >= self.cells.len() {
            return Err(Error::State);
        }
        for (index, cell) in self.cells.iter().enumerate() {
            if index != keep {
                cell.check_release(allocator)?;
            }
        }
        for (index, cell) in self.cells.iter_mut().enumerate().rev() {
            if index != keep {
                cell.release_storage(allocator)?;
            }
        }
        let result = self.cells.swap_remove(keep);
        self.released = true;
        Ok(result)
    }
}
