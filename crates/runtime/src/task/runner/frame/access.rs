//! Ephemeral access to one synchronous activation and its live ancestors.
//! Coordinates survive vector growth; no borrowed Rust pointer is retained.
use super::*;
use std::ops::{Deref, DerefMut};

pub struct Access<'a, 'l, 'ty> {
    pub frame: &'a mut Frame<'l, 'ty>,
    pub ancestors: &'a mut [Frame<'l, 'ty>],
}
impl<'l, 'ty> Deref for Access<'_, 'l, 'ty> {
    type Target = Frame<'l, 'ty>;
    fn deref(&self) -> &Self::Target {
        self.frame
    }
}
impl DerefMut for Access<'_, '_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.frame
    }
}
impl<'a, 'l, 'ty> Access<'a, 'l, 'ty> {
    pub fn new(frames: &'a mut [Frame<'l, 'ty>]) -> Result<Self, Reason> {
        let (frame, ancestors) = frames.split_last_mut().ok_or(Reason::Invariant)?;
        Ok(Self { frame, ancestors })
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
    pub fn claim_at(&self, location: Location) -> Result<bool, Reason> {
        self.owner(location.activation)?.is_claim(location.slot)
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
    fn referent(&self, slot: usize) -> Result<Location, Reason> {
        if self.is_reference(slot)? {
            match self.reference(slot)? {
                Target::Cell(location) => Ok(location),
                _ => Err(Reason::Invariant),
            }
        } else {
            self.available(slot)?;
            self.root(slot)
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
        let mut location = self.root(slot)?;
        for (index, projection) in place.projections.iter().enumerate() {
            match projection.kind {
                p::ProjectionKind::Field(field) => {
                    location.at = self.cell(location)?.child(location.at, field, allocator)?
                }
                p::ProjectionKind::Dereference => {
                    if location.at != self.owner(location.activation)?.root(location.slot)?.at {
                        return Err(Reason::Invariant);
                    }
                    match self.reference_at(location.activation, location.slot)? {
                        Target::Cell(target) => location = target,
                        Target::Allocator if index + 1 == place.projections.len() => {
                            return Ok(Target::Allocator);
                        }
                        _ => return Err(Reason::Invariant),
                    }
                }
            }
        }
        Ok(Target::Cell(location))
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
        Ok(self.cell(location)?.case(location.at, allocator)?)
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
        if self.cell(location)?.case(location.at, allocator)? != case {
            return Err(Reason::Invariant);
        }
        location.at = self.cell(location)?.child(location.at, field, allocator)?;
        self.frame.borrow_target(loan, Target::Cell(location), to)
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
        let source = self.cell(location)?;
        if source.case(location.at, allocator)? != case {
            return Err(Reason::Invariant);
        }
        if matches!(
            source
                .layouts()
                .get(location.at.ty())
                .ok_or(Reason::Invariant)?
                .kind,
            l::Kind::Error { .. }
        ) {
            if field != 0 {
                return Err(Reason::Invariant);
            }
            let payload = source
                .read_immediate(location.at, allocator)?
                .payload()
                .ok_or(Reason::Invariant)?;
            self.write(to, payload, allocator)
        } else {
            location.at = source.child(location.at, field, allocator)?;
            self.copy_location(location, self.root(to)?, allocator)
        }
    }
    pub fn copy_location<P: a::Provider>(
        &mut self,
        from: Location,
        to: Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
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
        to: Location,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        if to.activation == self.frame.activation {
            return self.frame.replace(from, to, span, allocator);
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
        self.frame
            .cleanup_slot(from, crate::cleanup::Reason::Replacement, allocator)
    }
}
