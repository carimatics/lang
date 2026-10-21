//! Indirect locations carry a checked lease identity, never an owning token.
use super::managed::{Loan, Reference};
use super::*;
impl From<Location> for Target {
    fn from(value: Location) -> Self {
        Self::Cell(value)
    }
}
impl Target {
    pub fn at(self) -> Result<l::Location, Reason> {
        match self {
            Self::Cell(v) => Ok(v.at),
            Self::Vector { at, .. }
            | Self::Map { at, .. }
            | Self::Shared { at, .. }
            | Self::Service { at, .. }
            | Self::Arena { at, .. } => Ok(at),
            _ => Err(Reason::Invariant),
        }
    }
    fn with_at(self, at: l::Location) -> Result<Self, Reason> {
        Ok(match self {
            Self::Cell(mut v) => {
                v.at = at;
                Self::Cell(v)
            }
            Self::Vector { origin, .. } => Self::Vector { origin, at },
            Self::Map { origin, .. } => Self::Map { origin, at },
            Self::Shared { origin, ty, .. } => Self::Shared { origin, ty, at },
            Self::Service { origin, .. } => Self::Service { origin, at },
            Self::Arena { origin, .. } => Self::Arena { origin, at },
            _ => return Err(Reason::Invariant),
        })
    }
}
macro_rules! observe {
    ($s:expr,$target:expr,$allocator:expr,|$view:ident,$at:ident|$body:expr) => {{
        match $target {
            Target::Cell(location) => {
                let $view = $s.cell(location)?;
                let $at = location.at;
                $body.map_err(|error| Reason::service(&error))
            }
            Target::Vector { origin, at } => {
                let loan = $s.lease(origin)?.vector.as_ref().ok_or(Reason::Invariant)?;
                let $view = loan
                    .view($s.services.layouts, $allocator)
                    .map_err(|error| Reason::service(&error))?;
                let $at = at;
                $body.map_err(|error| Reason::service(&error))
            }
            Target::Map { origin, at } => {
                let loan = $s.lease(origin)?.map.as_ref().ok_or(Reason::Invariant)?;
                let $view = loan
                    .view($s.services.layouts, $allocator)
                    .map_err(|error| Reason::service(&error))?;
                let $at = at;
                $body.map_err(|error| Reason::service(&error))
            }
            Target::Shared { origin, ty, at } => {
                let (read, read_ty) = $s.lease(origin)?.shared.ok_or(Reason::Invariant)?;
                if ty != read_ty {
                    return Err(Reason::Invariant);
                }
                let $view = $s
                    .services
                    .heap
                    .as_ref()
                    .ok_or(Reason::Invariant)?
                    .view(read, ty, $allocator)
                    .map_err(|error| Reason::service(&error))?;
                let $at = at;
                $body.map_err(|error| Reason::service(&error))
            }
            Target::Service { origin, at } => {
                let read = $s.lease(origin)?.service.ok_or(Reason::Invariant)?;
                let $view = $s
                    .services
                    .channels
                    .as_ref()
                    .ok_or(Reason::Invariant)?
                    .service_view(read, $allocator)
                    .map_err(Reason::Channel)?;
                let $at = at;
                $body.map_err(|error| Reason::service(&error))
            }
            Target::Arena { origin, at } => {
                let $view = $s.arena_view(origin, $allocator)?;
                let $at = at;
                $body.map_err(|error| Reason::service(&error))
            }
            _ => Err(Reason::Invariant),
        }
    }};
}
impl Access<'_, '_, '_> {
    pub(super) fn replace_indirect<P: a::Provider>(
        &mut self,
        from: usize,
        to: Target,
        span: m::Span,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        self.available(from)?;
        let copy = self.is_copy(from)?;
        let Target::Vector { origin, at } = to else {
            return Err(Reason::Invariant);
        };
        self.lease(origin)?;
        let managed = if origin.activation == self.frame.activation {
            self.frame.managed.as_ref().ok_or(Reason::Invariant)?
        } else {
            self.ancestors
                .iter()
                .find(|f| f.activation == origin.activation)
                .and_then(|f| f.managed.as_ref())
                .ok_or(Reason::Invariant)?
        };
        let read = managed.loans[origin.loan]
            .vector
            .as_ref()
            .ok_or(Reason::Invariant)?;
        let input = self
            .frame
            .storage
            .as_mut()
            .ok_or(Reason::Invariant)?
            .cell_mut(from)?;
        let old = if copy {
            read.replace_copy(
                self.services.layouts,
                at,
                input,
                input.root(),
                span,
                allocator,
            )
            .map_err(|error| Reason::service(&error))?;
            None
        } else {
            read.replace_from(self.services.layouts, at, input, span, allocator)
                .map_err(|error| Reason::service(&error))?
        };
        if !copy {
            self.frame.managed.as_mut().ok_or(Reason::Invariant)?.spans[from] = old;
            self.cleanup_slot(from, c::Reason::Replacement, allocator)?;
        }
        Ok(())
    }
    pub(super) fn next_reference(&self, id: b::LoanId) -> Result<Reference, Reason> {
        let loan = self
            .managed
            .as_ref()
            .ok_or(Reason::Invariant)?
            .loans
            .get(id.0 as usize)
            .ok_or(Reason::Invariant)?;
        if loan.target.is_some()
            || loan.vector.is_some()
            || loan.map.is_some()
            || loan.shared.is_some()
            || loan.service.is_some()
            || loan.arena.is_some()
        {
            return Err(Reason::Invariant);
        }
        Ok(Reference {
            activation: self.activation,
            loan: id.0 as usize,
            generation: loan.generation.checked_add(1).ok_or(Reason::Invariant)?,
        })
    }
    pub(super) fn custody<P: a::Provider>(
        &self,
        target: Target,
        allocator: &a::Allocator<P>,
    ) -> Result<Option<crate::shared::managed::Custody>, Reason> {
        match target {
            Target::Cell(_) => Ok(None),
            Target::Vector { origin, .. } | Target::Map { origin, .. } => {
                Ok(self.lease(origin)?.custody)
            }
            Target::Shared { origin, ty, .. } => {
                let (read, read_ty) = self.lease(origin)?.shared.ok_or(Reason::Invariant)?;
                if read_ty != ty {
                    return Err(Reason::Invariant);
                }
                Ok(Some(
                    self.services
                        .heap
                        .as_ref()
                        .ok_or(Reason::Invariant)?
                        .custody(read, ty, allocator)
                        .map_err(|error| Reason::service(&error))?,
                ))
            }
            _ => Ok(None),
        }
    }
    pub(super) fn lease(&self, origin: Reference) -> Result<&Loan, Reason> {
        let loan = self
            .owner(origin.activation)?
            .managed
            .as_ref()
            .ok_or(Reason::Invariant)?
            .loans
            .get(origin.loan)
            .ok_or(Reason::Invariant)?;
        if loan.generation != origin.generation || loan.target.is_none() {
            return Err(Reason::Invariant);
        }
        Ok(loan)
    }
    pub fn target_child<P: a::Provider>(
        &self,
        target: Target,
        field: u32,
        allocator: &a::Allocator<P>,
    ) -> Result<Target, Reason> {
        let child = observe!(self, target, allocator, |view, at| view
            .child(at, field, allocator))?;
        target.with_at(child)
    }
    pub fn target_case<P: a::Provider>(
        &self,
        target: Target,
        allocator: &a::Allocator<P>,
    ) -> Result<u32, Reason> {
        observe!(self, target, allocator, |view, at| view.case(at, allocator))
    }
    pub fn target_read<P: a::Provider>(
        &self,
        target: Target,
        allocator: &a::Allocator<P>,
    ) -> Result<l::Immediate, Reason> {
        observe!(self, target, allocator, |view, at| view
            .read_immediate(at, allocator))
    }
    pub(super) fn observed_token<P: a::Provider>(
        &self,
        target: Target,
        allocator: &a::Allocator<P>,
    ) -> Result<[u8; 32], Reason> {
        Ok(observe!(self, target, allocator, |view, at| view
            .read_token(at, allocator))?
        .bytes)
    }
    pub(super) fn copy_observed<P: a::Provider>(
        &self,
        target: Target,
        to: Location,
        allocator: &mut a::Allocator<P>,
    ) -> Result<(), Reason> {
        if !self
            .types
            .properties(target.at()?.ty())
            .ok_or(Reason::Invariant)?
            .is_copy
        {
            return Err(Reason::Invariant);
        }
        let destination = self.cell(to)?;
        match target {
            Target::Service { origin, at } => {
                let read = self.lease(origin)?.service.ok_or(Reason::Invariant)?;
                let source = self
                    .services
                    .channels
                    .as_ref()
                    .ok_or(Reason::Invariant)?
                    .service_view(read, allocator)
                    .map_err(Reason::Channel)?;
                source
                    .copy_into(at, destination, to.at, allocator)
                    .map_err(|error| Reason::service(&error))
            }
            Target::Arena { origin, at } => self
                .arena_view(origin, allocator)?
                .copy_into(at, destination, to.at, allocator)
                .map_err(|error| Reason::service(&error)),
            Target::Vector { origin, at } => self
                .lease(origin)?
                .vector
                .as_ref()
                .ok_or(Reason::Invariant)?
                .view(self.services.layouts, allocator)
                .map_err(|error| Reason::service(&error))?
                .copy_into(at, destination, to.at, allocator)
                .map_err(|error| Reason::service(&error)),
            Target::Map { origin, at } => self
                .lease(origin)?
                .map
                .as_ref()
                .ok_or(Reason::Invariant)?
                .view(self.services.layouts, allocator)
                .map_err(|error| Reason::service(&error))?
                .copy_into(at, destination, to.at, allocator)
                .map_err(|error| Reason::service(&error)),
            Target::Shared { origin, ty, at } => {
                let (read, read_ty) = self.lease(origin)?.shared.ok_or(Reason::Invariant)?;
                if ty != read_ty {
                    return Err(Reason::Invariant);
                }
                self.services
                    .heap
                    .as_ref()
                    .ok_or(Reason::Invariant)?
                    .view(read, ty, allocator)
                    .map_err(|error| Reason::service(&error))?
                    .copy_to(at, destination, to.at, allocator)?;
                Ok(())
            }
            _ => Err(Reason::Invariant),
        }
    }
}
