use super::*;
mod access;
mod arenas;
mod buffers;
mod channels;
mod component;
pub(super) use component::ResourceValues;
mod isolated;
pub use access::Access;
mod managed;
mod maps;
mod services;
mod shared;
mod targets;
mod vectors;
pub use managed::Managed;
pub use managed::Target;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivationId(pub u64);
#[derive(Clone, Copy)]
pub struct Location {
    pub activation: ActivationId,
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
    Channel,
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
pub struct ChannelWait {
    pub operation: crate::channel::Operation,
    pub capture: crate::channel::managed::Capture,
    pub destination: usize,
    pub send: bool,
    pub payload: Option<usize>,
    pub next: f::NodeId,
    pub failure: f::NodeId,
    pub span: m::Span,
}
#[derive(Clone, Copy)]
pub struct ReturnTo {
    pub destination: usize,
    pub next: f::NodeId,
    pub failure: f::NodeId,
}
/// A detached scope result retains its owner provenance with its reservation.
pub struct RetainedResult<'l, 'ty> {
    pub value: l::Reservation<'l, 'ty>,
    pub span: m::Span,
}
pub struct Frame<'l, 'ty> {
    pub activation: ActivationId,
    pub types: &'ty o::VerifiedTypes,
    pub origin: m::Span,
    pub managed: Option<Managed>,
    component_inputs: Vec<component::Referent<'l, 'ty>>,
    pub arenas: Vec<Option<crate::arena::managed::Arena<'l, 'ty>>>,
    pub function: m::FunctionId,
    pub node: f::NodeId,
    pub locals: usize,
    pub storage: Option<l::FrameStorage<'l, 'ty>>,
    pub live: Vec<bool>,
    pub claims: Vec<Option<t::TaskId>>,
    pub prepared: Vec<Option<Target>>,
    pub channel_prepared: Vec<Option<crate::channel::managed::Capture>>,
    pub channel_wait: Option<ChannelWait>,
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
    #[allow(
        clippy::too_many_arguments,
        reason = "Prepared frame identity, layouts and allocator remain explicit."
    )]
    pub fn new<P: a::Provider>(
        proof: &'ty o::tasks::VerifiedTasks,
        plan: &l::FramePlan<'l, 'ty>,
        layouts: &'l l::Layouts<'ty>,
        work: &e::ServiceWork,
        function: m::FunctionId,
        activation: ActivationId,
        failure_records: std::num::NonZeroUsize,
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
        budget.charge(body.prepared)?;
        let count = if proof.typed().facts()[function.0 as usize]
            .prepared()
            .iter()
            .flatten()
            .any(|p| p.channel().is_some() || p.service().is_some())
        {
            body.prepared
        } else {
            0
        };
        let mut channel_prepared = budget.vector(count)?;
        channel_prepared.resize_with(count, || None);
        let mut scopes = budget.vector(body.task_scopes.len())?;
        if !body.task_scopes.is_empty() {
            scopes.push(Scope {
                id: b::TaskScopeId(0),
                serial: 0,
                results: None,
            });
        }
        let types = proof.typed().places().declarations().types();
        budget.charge(plan.cell_count())?;
        let needs=body.loans!=0||(0..plan.cell_count()).any(|slot| {
            let ty=plan.cell_type(slot).expect("prepared cell").ty;
            (types.properties(ty).expect("verified type").contains_loan||!types.properties(ty).expect("verified type").is_copy) && !matches!(ty,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::Task(_)))
        });
        let managed = if needs {
            Some(Managed::prepare(
                plan.cell_count(),
                body.loans,
                failure_records,
                budget,
            )?)
        } else {
            None
        };
        budget.charge(plan.cell_count())?;
        let has_arenas=(0..plan.cell_count()).any(|slot|matches!(plan.cell_type(slot).map(|t|t.ty),Some(o::Type::Constructed(id))if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::ArenaOwner(_))));
        let mut arenas = budget.vector(if has_arenas { plan.cell_count() } else { 0 })?;
        if has_arenas {
            for slot in 0..plan.cell_count() {
                budget.charge(1)?;
                let arena = if matches!(plan.cell_type(slot).map(|t|t.ty),Some(o::Type::Constructed(id))if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::ArenaOwner(_)))
                {
                    let arena = crate::arena::managed::Arena::prepare_with_budget(
                        layouts,
                        Default::default(),
                        budget,
                    )
                    .map_err(arenas::preparation)?
                    .with_service_work(work.clone());
                    Some(arena)
                } else {
                    None
                };
                arenas.push(arena);
            }
        }
        let storage = plan.reserve_with_budget(allocator, budget)?;
        Ok(Self {
            activation,
            types,
            origin: proof.typed().places().declarations().functions()[function.0 as usize].span,
            managed,
            component_inputs: Vec::new(),
            arenas,
            function,
            node: proof.plan().functions()[function.0 as usize].entry(),
            locals: proof.typed().places().declarations().functions()[function.0 as usize]
                .locals
                .len(),
            storage: Some(storage),
            live,
            claims,
            prepared,
            channel_prepared,
            channel_wait: None,
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
            activation: self.activation,
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
        self.mark(slot, self.origin)
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
        if from.activation != self.activation || to.activation != self.activation {
            return Err(Reason::Invariant);
        }
        self.available(from.slot)?;
        let source_root = self.root(from.slot)?.at;
        if self.is_reference(from.slot)? {
            if from.at != source_root || to.at != self.root(to.slot)?.at {
                return Err(Reason::Unsupported);
            }
            let reference = self.managed.as_ref().ok_or(Reason::Unsupported)?.references[from.slot];
            self.managed.as_mut().ok_or(Reason::Unsupported)?.references[to.slot] = reference;
            if !self.is_copy(from.slot)? {
                self.forget(from.slot);
            }
        } else if self.is_allocator(from.slot)? {
            if from.at != source_root || to.at != self.root(to.slot)?.at {
                return Err(Reason::Unsupported);
            }
            if !self.is_copy(from.slot)? {
                self.forget(from.slot);
            }
        } else if self
            .types
            .properties(from.at.ty())
            .ok_or(Reason::Invariant)?
            .is_copy
        {
            self.data()?.cell(to.slot)?.copy_from(
                to.at,
                self.data()?.cell(from.slot)?,
                from.at,
                allocator,
            )?;
        } else {
            if from.at != source_root || from.slot == to.slot {
                return Err(Reason::Unsupported);
            }
            let span = self
                .managed
                .as_ref()
                .and_then(|m| m.spans[from.slot])
                .unwrap_or(self.origin);
            let (source, target) = self
                .storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .two_cells_mut(from.slot, to.slot)?;
            source.move_root_into(target, to.at, allocator)?;
            source.recycle(allocator)?;
            if self
                .types
                .properties(source_root.ty())
                .ok_or(Reason::Invariant)?
                .contains_loan
            {
                let managed = self.managed.as_mut().ok_or(Reason::Invariant)?;
                managed.references[to.slot] = managed.references[from.slot];
            }
            self.forget(from.slot);
            return self.mark(to.slot, span);
        }
        self.mark(to.slot, self.origin)
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
        from: &mut Self,
        source: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        from.available(source)?;
        if from.is_reference(source)? {
            let reference = from.managed.as_ref().ok_or(Reason::Invariant)?.references[source]
                .ok_or(Reason::Invariant)?;
            self.managed.as_mut().ok_or(Reason::Invariant)?.references[to] = Some(reference);
            if !from.is_copy(source)? {
                from.forget(source);
            }
            return self.mark(to, self.origin);
        }
        if from.is_copy(source)? {
            self.data()?.cell(to)?.copy_from(
                self.root(to)?.at,
                from.data()?.cell(source)?,
                from.root(source)?.at,
                allocator,
            )?;
            self.mark(to, self.origin)
        } else {
            if from.is_reference(source)? || from.is_claim(source)? {
                return Err(Reason::Unsupported);
            }
            let span = from
                .managed
                .as_ref()
                .and_then(|m| m.spans[source])
                .unwrap_or(from.origin);
            let target = self.storage.as_ref().ok_or(Reason::Invariant)?.cell(to)?;
            let cell = from
                .storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .cell_mut(source)?;
            cell.move_root_into(target, target.root(), allocator)?;
            cell.recycle(allocator)?;
            from.forget(source);
            self.mark(to, span)
        }
    }
    pub fn replace<P: a::Provider>(
        &mut self,
        from: usize,
        to: Location,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
        services: &mut super::services::Services<'_, '_, '_>,
    ) -> Result<(), Reason> {
        self.available(from)?;
        if !self.live[to.slot] && to.at == self.root(to.slot)?.at {
            return self.transfer(from, to.slot, allocator);
        }
        if self.is_claim(to.slot)? {
            return self.transfer(from, to.slot, allocator);
        }
        if self.is_reference(from)? {
            return self.copy_location(self.root(from)?, to, allocator);
        }
        if self
            .types
            .properties(to.at.ty())
            .ok_or(Reason::Invariant)?
            .is_copy
        {
            return self.copy_location(self.root(from)?, to, allocator);
        }
        if from == to.slot {
            return Err(Reason::Invariant);
        }
        let old_span = {
            let (target, incoming) = self
                .storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .two_cells_mut(to.slot, from)?;
            target.exchange_root_at(to.at, incoming, span, allocator)?
        };
        let root = self.root(to.slot)?.at;
        let managed = self.managed.as_mut().ok_or(Reason::Invariant)?;
        if to.at == root {
            let previous = managed.spans[to.slot];
            managed.remove(to.slot);
            managed.insert(to.slot, span);
            managed.spans[from] = previous;
        } else {
            managed.spans[from] = old_span;
        }
        self.cleanup_slot(
            from,
            crate::cleanup::Reason::Replacement,
            allocator,
            services,
        )
    }
    pub fn place<P: a::Provider>(
        &self,
        place: &p::Place,
        allocator: &a::Allocator<P>,
    ) -> Result<Location, Reason> {
        match self.place_target(place, allocator)? {
            Target::Cell(location) => Ok(location),
            _ => Err(Reason::Invariant),
        }
    }
    pub fn place_target<P: a::Provider>(
        &self,
        place: &p::Place,
        allocator: &a::Allocator<P>,
    ) -> Result<Target, Reason> {
        let mut slot = match place.root {
            p::Root::Local(id) => id.0 as usize,
            p::Root::Value(id) => self.value(id),
        };
        self.available(slot)?;
        let mut at = self.root(slot)?.at;
        for (index, projection) in place.projections.iter().enumerate() {
            match projection.kind {
                p::ProjectionKind::Field(field) => {
                    at = self.data()?.cell(slot)?.child(at, field, allocator)?
                }
                p::ProjectionKind::Dereference => {
                    if at != self.root(slot)?.at {
                        return Err(Reason::Unsupported);
                    }
                    match self.reference(slot)? {
                        Target::Cell(target) => {
                            slot = target.slot;
                            at = target.at;
                        }
                        Target::Allocator if index + 1 == place.projections.len() => {
                            return Ok(Target::Allocator);
                        }
                        _ => return Err(Reason::Invariant),
                    }
                }
            }
        }
        Ok(Target::Cell(Location {
            activation: self.activation,
            slot,
            at,
        }))
    }
    pub fn forget(&mut self, slot: usize) {
        self.claims[slot] = None;
        if let Some(m) = &mut self.managed {
            m.references[slot] = None;
            m.remove(slot);
        }
        self.live[slot] = false;
    }
    pub fn release<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
        services: &mut super::services::Services<'_, '_, '_>,
    ) -> Result<(), Reason> {
        let cleaned = self.cleanup_all(None, crate::cleanup::Reason::Failure, allocator, services);
        self.storage
            .as_mut()
            .ok_or(Reason::Invariant)?
            .release(allocator)?;
        self.storage = None;
        self.claims.fill(None);
        self.live.fill(false);
        cleaned
    }
    pub fn result<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
        services: &mut super::services::Services<'_, '_, '_>,
    ) -> Result<RetainedResult<'l, 'ty>, Reason> {
        self.available(self.return_cell)?;
        let span = self
            .managed
            .as_ref()
            .and_then(|m| m.spans[self.return_cell])
            .unwrap_or(self.origin);
        if let Err(reason) = self.cleanup_all(
            Some(self.return_cell),
            crate::cleanup::Reason::ScopeExit,
            allocator,
            services,
        ) {
            let _ = self.cleanup_slot(
                self.return_cell,
                crate::cleanup::Reason::UnpublishedReturn,
                allocator,
                services,
            );
            self.storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .release(allocator)?;
            self.storage = None;
            self.live.fill(false);
            self.claims.fill(None);
            return Err(reason);
        }
        if let Some(m) = &mut self.managed {
            m.remove(self.return_cell);
        }
        let result = self
            .storage
            .as_mut()
            .ok_or(Reason::Invariant)?
            .release_except(self.return_cell, allocator)?;
        self.storage = None;
        self.claims.fill(None);
        self.live.fill(false);
        Ok(RetainedResult {
            value: result,
            span,
        })
    }
}
