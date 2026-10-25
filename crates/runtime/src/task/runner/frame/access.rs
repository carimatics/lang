//! Ephemeral access to one synchronous activation and its live ancestors.
//! Coordinates survive vector growth; no borrowed Rust pointer is retained.
use super::*;
use crate::task::runner::services::Services;
use std::ops::{Deref, DerefMut};

pub struct Access<'a, 'l, 'ty, 'scope> {
    pub frame: &'a mut Frame<'l, 'ty>,
    pub ancestors: &'a mut [Frame<'l, 'ty>],
    pub services: &'a mut Services<'l, 'ty, 'scope>,
}
impl<'l, 'ty> Deref for Access<'_, 'l, 'ty, '_> {
    type Target = Frame<'l, 'ty>;
    fn deref(&self) -> &Self::Target {
        self.frame
    }
}
impl DerefMut for Access<'_, '_, '_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.frame
    }
}
impl<'a, 'l, 'ty, 'scope> Access<'a, 'l, 'ty, 'scope> {
    pub fn end_loan<P: a::Provider>(
        &mut self,
        id: b::LoanId,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        self.frame.end_loan(id, allocator, self.services)
    }
    pub fn end_loans<P: a::Provider>(
        &mut self,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        self.frame.end_loans(allocator, self.services)
    }
    pub fn new(
        frames: &'a mut [Frame<'l, 'ty>],
        services: &'a mut Services<'l, 'ty, 'scope>,
    ) -> Result<Self, Reason> {
        let (frame, ancestors) = frames.split_last_mut().ok_or(Reason::Invariant)?;
        Ok(Self {
            frame,
            ancestors,
            services,
        })
    }
    pub fn cleanup_slot<P: a::Provider>(
        &mut self,
        slot: usize,
        reason: c::Reason,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        self.frame
            .cleanup_slot(slot, reason, allocator, self.services)
    }
    pub fn cleanup_selected<P: a::Provider>(
        &mut self,
        slots: impl IntoIterator<Item = usize>,
        reason: c::Reason,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        self.frame
            .cleanup_selected(slots, reason, allocator, self.services)
    }
    pub fn owner(&self, id: ActivationId) -> Result<&Frame<'l, 'ty>, Reason> {
        if id == self.frame.activation {
            return Ok(self.frame);
        }
        self.ancestors
            .iter()
            .find(|f| f.activation == id)
            .ok_or(Reason::Invariant)
    }
    fn owner_mut(&mut self, id: ActivationId) -> Result<&mut Frame<'l, 'ty>, Reason> {
        if id == self.frame.activation {
            return Ok(self.frame);
        }
        self.ancestors
            .iter_mut()
            .find(|f| f.activation == id)
            .ok_or(Reason::Invariant)
    }
    pub fn cell(&self, location: Location) -> Result<&l::Reservation<'l, 'ty>, Reason> {
        Ok(self
            .owner(location.activation)?
            .data()?
            .cell(location.slot)?)
    }
    fn available_at(&self, location: Location) -> Result<(), Reason> {
        self.owner(location.activation)?.available(location.slot)
    }
    pub fn claim_slot(&self, target: Target) -> Result<Option<usize>, Reason> {
        match target {
            Target::Cell(location)
                if self.owner(location.activation)?.is_claim(location.slot)? =>
            {
                Ok(Some(location.slot))
            }
            _ => Ok(None),
        }
    }
    fn reference_at(&self, activation: ActivationId, slot: usize) -> Result<Target, Reason> {
        let owner = self.owner(activation)?;
        owner.available(slot)?;
        let reference = owner.managed.as_ref().ok_or(Reason::Invariant)?.references[slot]
            .ok_or(Reason::Invariant)?;
        let origin = self.owner(reference.activation)?;
        let loan = origin
            .managed
            .as_ref()
            .ok_or(Reason::Invariant)?
            .loans
            .get(reference.loan)
            .ok_or(Reason::Invariant)?;
        if loan.generation != reference.generation {
            return Err(Reason::Invariant);
        }
        loan.target.ok_or(Reason::Invariant)
    }
    pub fn reference(&self, slot: usize) -> Result<Target, Reason> {
        self.reference_at(self.frame.activation, slot)
    }
    fn referent(&self, slot: usize) -> Result<Target, Reason> {
        if self.is_reference(slot)? {
            self.reference(slot)
        } else {
            self.available(slot)?;
            Ok(Target::Cell(self.root(slot)?))
        }
    }

    pub fn place_target<P: a::Provider>(
        &self,
        place: &p::Place,
        allocator: &a::Allocator<P>,
    ) -> Result<Target, Reason> {
        let slot = match place.root {
            p::Root::Local(id) => id.0 as usize,
            p::Root::Value(id) => self.value(id),
        };
        self.available(slot)?;
        let mut target = Target::Cell(self.root(slot)?);
        for projection in &place.projections {
            target = match projection.kind {
                p::ProjectionKind::Field(field) => self.target_child(target, field, allocator)?,
                p::ProjectionKind::Dereference => {
                    let Target::Cell(location) = target else {
                        return Err(Reason::Invariant);
                    };
                    if location.at != self.owner(location.activation)?.root(location.slot)?.at {
                        return Err(Reason::Invariant);
                    }
                    self.reference_at(location.activation, location.slot)?
                }
            };
        }
        Ok(target)
    }
    pub fn place<P: a::Provider>(
        &self,
        place: &p::Place,
        allocator: &a::Allocator<P>,
    ) -> Result<Target, Reason> {
        self.place_target(place, allocator)
    }
    /// A whole local destination may be vacant after a proved move. Projection
    /// and read addressing still require a live owner and checked loan.
    pub fn write_place<P: a::Provider>(
        &self,
        place: &p::Place,
        allocator: &a::Allocator<P>,
    ) -> Result<Target, Reason> {
        if place.projections.is_empty()
            && let p::Root::Local(id) = place.root
        {
            return Ok(Target::Cell(self.root(id.0 as usize)?));
        }
        self.place_target(place, allocator)
    }

    pub fn borrow<P: a::Provider>(
        &mut self,
        loan: b::LoanId,
        place: &p::Place,
        to: usize,
        allocator: &a::Allocator<P>,
    ) -> Result<(), Reason> {
        let target = self.place_target(place, allocator)?;
        self.frame.borrow_target(loan, target, to)
    }
    pub fn case<P: a::Provider>(
        &self,
        from: usize,
        allocator: &a::Allocator<P>,
    ) -> Result<u32, Reason> {
        let location = self.referent(from)?;
        self.target_case(location, allocator)
    }
    pub fn borrow_case<P: a::Provider>(
        &mut self,
        from: usize,
        case: u32,
        field: u32,
        loan: b::LoanId,
        to: usize,
        allocator: &a::Allocator<P>,
    ) -> Result<(), Reason> {
        let mut location = self.referent(from)?;
        if self.target_case(location, allocator)? != case {
            return Err(Reason::Invariant);
        }
        location = self.target_child(location, field, allocator)?;
        self.frame.borrow_target(loan, location, to)
    }
    pub fn copy_field<P: a::Provider>(
        &mut self,
        from: usize,
        case: u32,
        field: u32,
        to: usize,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let mut location = self.referent(from)?;
        if self.target_case(location, allocator)? != case {
            return Err(Reason::Invariant);
        }
        if matches!(
            self.services
                .layouts
                .get(location.at()?.ty())
                .ok_or(Reason::Invariant)?
                .kind,
            l::Kind::Error { .. }
        ) {
            if field != 0 {
                return Err(Reason::Invariant);
            }
            let payload = self
                .target_read(location, allocator)?
                .payload()
                .ok_or(Reason::Invariant)?;
            self.write(to, payload, allocator)
        } else {
            location = self.target_child(location, field, allocator)?;
            self.copy_location(location, self.root(to)?, allocator)
        }
    }

    pub fn copy_location<P: a::Provider>(
        &mut self,
        from: impl Into<Target>,
        to: Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let from = match from.into() {
            Target::Cell(location) => location,
            target => {
                self.copy_observed(target, to, allocator)?;
                let span = self.frame.origin;
                return self.owner_mut(to.activation)?.mark(to.slot, span);
            }
        };
        if from.activation == self.frame.activation && to.activation == self.frame.activation {
            return self.frame.copy_location(from, to, allocator);
        }
        self.available_at(from)?;
        if !self
            .types
            .properties(from.at.ty())
            .ok_or(Reason::Invariant)?
            .is_copy
        {
            return Err(Reason::Invariant);
        }
        self.cell(to)?
            .copy_from(to.at, self.cell(from)?, from.at, allocator)?;
        let span = self.frame.origin;
        self.owner_mut(to.activation)?.mark(to.slot, span)
    }
    pub fn replace<P: a::Provider>(
        &mut self,
        from: usize,
        to: Target,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        let Target::Cell(to) = to else {
            return self.replace_indirect(from, to, span, allocator);
        };
        if to.activation == self.frame.activation {
            return self.frame.replace(from, to, span, allocator, self.services);
        }
        self.available(from)?;
        self.available_at(to)?;
        if self
            .types
            .properties(to.at.ty())
            .ok_or(Reason::Invariant)?
            .is_copy
        {
            return self.copy_location(self.root(from)?, to, allocator);
        }
        let owner = self
            .ancestors
            .iter_mut()
            .find(|f| f.activation == to.activation)
            .ok_or(Reason::Invariant)?;
        let old_span = {
            let target = owner
                .storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .cell_mut(to.slot)?;
            let incoming = self
                .frame
                .storage
                .as_mut()
                .ok_or(Reason::Invariant)?
                .cell_mut(from)?;
            target.exchange_root_at(to.at, incoming, span, allocator)?
        };
        let root = owner.root(to.slot)?.at;
        let managed = owner.managed.as_mut().ok_or(Reason::Invariant)?;
        let previous = if to.at == root {
            let previous = managed.spans[to.slot];
            managed.remove(to.slot);
            managed.insert(to.slot, span);
            previous
        } else {
            old_span
        };
        self.frame.managed.as_mut().ok_or(Reason::Invariant)?.spans[from] = previous;
        self.frame.cleanup_slot(
            from,
            crate::cleanup::Reason::Replacement,
            allocator,
            self.services,
        )
    }
}
