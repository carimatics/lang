use super::*;

#[derive(Clone, Copy)]
pub struct Location {
    pub slot: usize,
    pub at: l::Location,
}
#[derive(Clone, Copy)]
pub struct Scope {
    pub id: b::TaskScopeId,
    pub serial: u64,
    pub results: Option<usize>,
}
#[derive(Clone, Copy)]
pub enum Pending {
    Await {
        child: usize,
        destination: usize,
        next: f::NodeId,
        failure: f::NodeId,
    },
    Scope {
        scope: Scope,
        cursor: usize,
        next: f::NodeId,
        failure: f::NodeId,
    },
}
#[derive(Clone, Copy)]
pub struct ReturnTo {
    pub destination: usize,
    pub next: f::NodeId,
    pub failure: f::NodeId,
}
pub struct Frame<'l, 'ty> {
    pub function: m::FunctionId,
    pub node: f::NodeId,
    pub locals: usize,
    pub storage: Option<l::FrameStorage<'l, 'ty>>,
    pub live: Vec<bool>,
    pub claims: Vec<Option<t::TaskId>>,
    pub prepared: Vec<Option<Location>>,
    pub scopes: Vec<Scope>,
    pub serial: u64,
    pub pending: Option<Pending>,
    pub returned: Option<ReturnTo>,
    pub saved: bool,
    pub closing: bool,
    pub fault: Option<usize>,
    pub return_cell: usize,
}
impl<'l, 'ty> Frame<'l, 'ty> {
    pub fn new<P: a::Provider>(
        proof: &o::tasks::VerifiedTasks,
        plan: &l::FramePlan<'l, 'ty>,
        function: m::FunctionId,
        allocator: &mut a::Allocator<P>,
        budget: &mut m::Budget,
    ) -> Result<Self, Reason> {
        let body = &proof.typed().functions()[function.0 as usize];
        let mut live = budget.vector(plan.cell_count())?;
        live.resize(plan.cell_count(), false);
        let mut claims = budget.vector(plan.cell_count())?;
        claims.resize_with(plan.cell_count(), || None);
        let mut prepared = budget.vector(body.prepared)?;
        prepared.resize(body.prepared, None);
        let mut scopes = budget.vector(body.task_scopes.len())?;
        if !body.task_scopes.is_empty() {
            scopes.push(Scope {
                id: b::TaskScopeId(0),
                serial: 0,
                results: None,
            });
        }
        let storage = plan.reserve_after(allocator, usage(budget))?;
        *budget = m::Budget::with_usage(budget.limits(), storage.usage())?;
        Ok(Self {
            function,
            node: proof.plan().functions()[function.0 as usize].entry(),
            locals: proof.typed().places().declarations().functions()[function.0 as usize]
                .locals
                .len(),
            storage: Some(storage),
            live,
            claims,
            prepared,
            scopes,
            serial: 0,
            pending: None,
            returned: None,
            saved: false,
            closing: false,
            fault: None,
            return_cell: plan.return_cell(),
        })
    }
    pub fn value(&self, value: m::Register) -> usize {
        self.locals + value.0 as usize
    }
    pub fn data(&self) -> Result<&l::FrameStorage<'l, 'ty>, Reason> {
        self.storage.as_ref().ok_or(Reason::Invariant)
    }
    pub fn root(&self, slot: usize) -> Result<Location, Reason> {
        Ok(Location {
            slot,
            at: self.data()?.cell(slot)?.root(),
        })
    }
    pub fn available(&self, slot: usize) -> Result<(), Reason> {
        if self.live.get(slot) == Some(&true) {
            Ok(())
        } else {
            Err(Reason::Invariant)
        }
    }
    pub fn read<P: a::Provider>(
        &self,
        slot: usize,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Immediate, Reason> {
        self.available(slot)?;
        let cell = self.data()?.cell(slot)?;
        Ok(cell.read_immediate(cell.root(), allocator)?)
    }
    pub fn write<P: a::Provider>(
        &mut self,
        slot: usize,
        value: l::Immediate,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let cell = self.data()?.cell(slot)?;
        cell.write_immediate(cell.root(), value, allocator)?;
        self.live[slot] = true;
        Ok(())
    }
    pub fn is_claim(&self, slot: usize) -> Result<bool, Reason> {
        Ok(matches!(
            self.data()?
                .cell(slot)?
                .layouts()
                .get(self.root(slot)?.at.ty())
                .ok_or(Reason::Invariant)?
                .kind,
            l::Kind::TaskClaim
        ))
    }
    pub fn copy_location<P: a::Provider>(
        &mut self,
        from: Location,
        to: Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        self.available(from.slot)?;
        self.data()?.cell(to.slot)?.copy_from(
            to.at,
            self.data()?.cell(from.slot)?,
            from.at,
            allocator,
        )?;
        self.live[to.slot] = true;
        Ok(())
    }
    pub fn transfer<P: a::Provider>(
        &mut self,
        from: usize,
        to: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        self.available(from)?;
        if self.is_claim(from)? {
            let claim = self.claims[from].take().ok_or(Reason::Invariant)?;
            self.live[from] = false;
            self.claims[to] = Some(claim);
            self.live[to] = true;
        } else {
            self.copy_location(self.root(from)?, self.root(to)?, allocator)?;
        }
        Ok(())
    }
    pub fn copy_from_frame<P: a::Provider>(
        &mut self,
        to: usize,
        from: &Self,
        source: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        from.available(source)?;
        self.data()?.cell(to)?.copy_from(
            self.root(to)?.at,
            from.data()?.cell(source)?,
            from.root(source)?.at,
            allocator,
        )?;
        self.live[to] = true;
        Ok(())
    }
    pub fn place<P: a::Provider>(
        &self,
        place: &p::Place,
        allocator: &a::Allocator<P>,
    ) -> Result<Location, Reason> {
        let slot = match place.root {
            p::Root::Local(id) => id.0 as usize,
            p::Root::Value(id) => self.value(id),
        };
        self.available(slot)?;
        let mut at = self.root(slot)?.at;
        for projection in &place.projections {
            match projection.kind {
                p::ProjectionKind::Field(field) => {
                    at = self.data()?.cell(slot)?.child(at, field, allocator)?
                }
                _ => return Err(Reason::Unsupported),
            }
        }
        Ok(Location { slot, at })
    }
    pub fn forget(&mut self, slot: usize) {
        self.claims[slot] = None;
        self.live[slot] = false;
    }
    pub fn release<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        self.storage
            .as_mut()
            .ok_or(Reason::Invariant)?
            .release(allocator)?;
        self.storage = None;
        self.claims.fill(None);
        self.live.fill(false);
        Ok(())
    }
    pub fn result<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<l::Reservation<'l, 'ty>, Reason> {
        self.available(self.return_cell)?;
        let result = self
            .storage
            .as_mut()
            .ok_or(Reason::Invariant)?
            .release_except(self.return_cell, allocator)?;
        self.storage = None;
        self.claims.fill(None);
        self.live.fill(false);
        Ok(result)
    }
}
