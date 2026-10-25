//! Persistent locators remain receipts for the original resource table.
use super::*;

pub struct PersistentReservation {
    holder: Holder,
}
impl PersistentReservation {
    pub fn prepare(resources: &mut Resources<'_>) -> Result<Self, Reason> {
        Ok(Self {
            holder: resources.new_holder().map_err(fault)?,
        })
    }
    pub fn bind(
        self,
        provider: super::super::super::native::Provider<'static>,
        maximum: u32,
    ) -> PersistentResources {
        PersistentResources {
            holder: self.holder,
            provider,
            maximum,
            indices: Vec::new(),
        }
    }
}
pub struct PersistentResources {
    holder: Holder,
    provider: super::super::super::native::Provider<'static>,
    maximum: u32,
    indices: Vec<Claim>,
}
impl PersistentResources {
    pub fn holder(&self) -> Holder {
        self.holder
    }
    pub fn issued(&self) -> u64 {
        self.indices.len() as u64
    }
}
impl CallContext<'_, '_, '_, '_, '_> {
    fn preparation_failure(&self, error: m::Error) -> u32 {
        self.remember_failure(Reason::Resource(error));
        12
    }
    fn persistent_matches(&self, state: &PersistentResources) -> Result<(), u32> {
        if !state.provider.is_owned() || !self.matches_provider(&state.provider) {
            return Err(7);
        }
        self.resources.check_holder(state.holder).map_err(|_| 7u32)
    }
    pub(super) fn keep(&mut self, index: u32, state: &mut PersistentResources) -> Result<u32, u32> {
        self.persistent_matches(state)?;
        let view = self.view(index)?;
        let NativeAuthority::Own(claim, holder) = view.authority else {
            return Err(7);
        };
        if holder != self.holder
            || !self
                .owners
                .iter()
                .any(|owner| owner.claim == claim && owner.held.get())
        {
            return Err(7);
        }
        self.resources
            .check_transfer(std::iter::once(claim), holder, state.holder, self.memory)
            .map_err(|_| 7u32)?;
        let limit = state.maximum.min(self.index_limit);
        if state.indices.len() >= limit as usize {
            self.remember_failure(Reason::ComponentIndices {
                limit: u64::from(limit),
                attempted: state.indices.len() as u64 + 1,
            });
            return Err(12);
        }
        self.budget
            .charge(1)
            .map_err(|error| self.preparation_failure(error))?;
        let mut indices = self
            .budget
            .vector(state.indices.len() + 1)
            .map_err(|error| self.preparation_failure(error))?;
        self.budget
            .charge(
                state
                    .indices
                    .len()
                    .saturating_add(self.owners.len().saturating_mul(2)),
            )
            .map_err(|error| self.preparation_failure(error))?;
        indices.extend_from_slice(&state.indices);
        let moved = self.move_persistent(claim, holder, state.holder)?;
        for owner in self.owners.iter().filter(|owner| owner.claim == claim) {
            owner.held.set(false);
        }
        indices.push(moved);
        state.indices = indices;
        Ok(state.indices.len() as u32)
    }
    pub(super) fn take(&mut self, index: u32, state: &mut PersistentResources) -> Result<u32, u32> {
        self.persistent_matches(state)?;
        let claim = index
            .checked_sub(1)
            .and_then(|index| state.indices.get(index as usize))
            .copied()
            .ok_or(7u32)?;
        let kind = self
            .resources
            .retirement_kind(claim, state.holder, self.memory)
            .map_err(|_| 7u32)?;
        if !kind.provider().same_registration(&state.provider) {
            return Err(7);
        }
        let bytes = self
            .resources
            .owned_view(claim, state.holder, kind.clone(), self.memory)
            .map_err(|_| 7u32)?;
        self.resources
            .check_transfer(
                std::iter::once(claim),
                state.holder,
                self.holder,
                self.memory,
            )
            .map_err(|_| 7u32)?;
        if self.views.len() >= self.index_limit as usize {
            self.remember_failure(Reason::ComponentIndices {
                limit: u64::from(self.index_limit),
                attempted: self.views.len() as u64 + 1,
            });
            return Err(12);
        }
        self.budget
            .charge(1)
            .map_err(|error| self.preparation_failure(error))?;
        if self.views.len() == self.views.capacity() {
            let mut views = self
                .budget
                .vector(self.views.len() + 1)
                .map_err(|error| self.preparation_failure(error))?;
            self.budget
                .charge(self.views.len())
                .map_err(|error| self.preparation_failure(error))?;
            views.append(self.views);
            *self.views = views;
        }
        if self.owners.len() == self.owners.capacity() {
            let mut owners = self
                .budget
                .vector(self.owners.len() + 1)
                .map_err(|error| self.preparation_failure(error))?;
            self.budget
                .charge(self.owners.len())
                .map_err(|error| self.preparation_failure(error))?;
            owners.append(self.owners);
            *self.owners = owners;
        }
        let moved = self.move_persistent(claim, state.holder, self.holder)?;
        self.owners.push(NativeOwner::new(moved, self.views.len()));
        self.views.push(NativeView {
            authority: NativeAuthority::Own(moved, self.holder),
            kind,
            access: super::super::super::idl::Access::Exclusive,
            bytes,
        });
        // The consumed persistent claim remains a stale ticket. Its ordinal
        // cannot resolve to a later owner, even after keep/take repetition.
        Ok(self.views.len() as u32)
    }
    fn move_persistent(&mut self, claim: Claim, from: Holder, to: Holder) -> Result<Claim, u32> {
        let ticket = self
            .resources
            .prepare_transfer(&[claim], from, to, self.memory)
            .map_err(|_| 7u32)?;
        let moved = self
            .resources
            .prepared_claim(ticket, 0)
            .expect("one admitted persistent transfer has one prepared claim");
        if self.resources.commit_transfer(ticket, self.memory).is_err() {
            self.resources
                .abort_transfer(ticket)
                .expect("failed pre-commit validation retains its transfer");
            return Err(7);
        }
        Ok(moved)
    }
}
