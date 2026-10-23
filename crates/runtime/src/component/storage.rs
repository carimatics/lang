//! Allocator-backed component owners. Private claims are not wire authority.
//! Locator storage is reserved before entry; moving ownership changes the real
//! provider-backed header and never allocates or invokes native code.
use super::{
    native::ResourceKind,
    negotiation::ResourceTypeKey,
    resource::{self, OwnedResourceHandle, ResourceTable},
};
use crate::allocation as a;
use lang_mir::unit as m;
use std::fmt;

const MAGIC: u64 = 0x636f_6d70_6f77_6e31;
const HEADER_BYTES: u64 = 48;

/// Access to the same actual instance allocator from the shared component
/// service. Erasing its provider type creates neither another allocator nor
/// shadow accounting; every operation delegates to the original storage.
pub trait Memory {
    fn identity(&self) -> a::Identity;
    fn check(&self, requests: &[a::Request]) -> Result<(), a::Error>;
    fn allocate(&mut self, request: a::Request) -> Result<a::Handle, a::Error>;
    fn request(&self, handle: a::Handle) -> Result<a::Request, a::Error>;
    fn bytes(&self, handle: a::Handle) -> Result<&[u8], a::Error>;
    fn bytes_mut(&mut self, handle: a::Handle) -> Result<&mut [u8], a::Error>;
    fn release(&mut self, handle: a::Handle) -> Result<(), a::Error>;
}
impl<P: a::Provider> Memory for a::Allocator<P> {
    fn identity(&self) -> a::Identity {
        self.identity()
    }
    fn check(&self, requests: &[a::Request]) -> Result<(), a::Error> {
        self.check(requests)
    }
    fn allocate(&mut self, request: a::Request) -> Result<a::Handle, a::Error> {
        self.allocate(request)
    }
    fn request(&self, handle: a::Handle) -> Result<a::Request, a::Error> {
        self.request(handle)
    }
    fn bytes(&self, handle: a::Handle) -> Result<&[u8], a::Error> {
        self.bytes(handle)
    }
    fn bytes_mut(&mut self, handle: a::Handle) -> Result<&mut [u8], a::Error> {
        self.bytes_mut(handle)
    }
    fn release(&mut self, handle: a::Handle) -> Result<(), a::Error> {
        self.release(handle)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Holder {
    table: u64,
    scope: u64,
}
/// Copyable native/internal claim. Every use rechecks the actual owner table,
/// original allocator and current physical ticket/holder before granting access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Claim {
    owner: OwnedResourceHandle,
    ticket: u64,
}
impl Claim {
    pub fn token(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[..24].copy_from_slice(&self.owner.token());
        bytes[24..].copy_from_slice(&self.ticket.to_le_bytes());
        bytes
    }
    pub fn from_token(bytes: [u8; 32]) -> Result<Self, Error> {
        let ticket = u64::from_le_bytes(bytes[24..].try_into().expect("fixed token"));
        if ticket == 0 {
            return Err(Error::StaleClaim);
        }
        Ok(Self {
            owner: OwnedResourceHandle::from_token(bytes[..24].try_into().expect("fixed token"))?,
            ticket,
        })
    }
}
#[derive(Clone, Copy)]
struct Backing<'a> {
    kind: ResourceKind<'a>,
    data: a::Handle,
    header: a::Handle,
}
#[derive(Clone, Copy)]
struct Plan {
    original: Claim,
    next: Claim,
    header: a::Handle,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Transfer {
    table: u64,
    serial: u64,
}
#[derive(Clone, Copy)]
struct Pending {
    ticket: Transfer,
    from: Holder,
    to: Holder,
}
#[derive(Debug)]
pub enum Error {
    Allocation(a::Error),
    Table(resource::ResourceTableError),
    Preparation(resource::PreparationError),
    ForeignAllocator,
    ForeignHolder,
    StaleClaim,
    WrongHolder,
    StaticType,
    Implementation,
    AliasedOwner,
    Exhausted,
    Capacity,
    State,
}
impl From<a::Error> for Error {
    fn from(e: a::Error) -> Self {
        Self::Allocation(e)
    }
}
impl From<resource::ResourceTableError> for Error {
    fn from(e: resource::ResourceTableError) -> Self {
        Self::Table(e)
    }
}
impl From<resource::PreparationError> for Error {
    fn from(e: resource::PreparationError) -> Self {
        Self::Preparation(e)
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "component storage: {self:?}")
    }
}
impl std::error::Error for Error {}

pub struct Resources<'a> {
    values: ResourceTable<Backing<'a>>,
    allocator: a::Identity,
    next_ticket: Option<u64>,
    next_holder: Option<u64>,
    next_transfer: Option<u64>,
    pending: Option<Pending>,
    plans: Vec<Plan>,
    capacity: usize,
}
impl<'a> Resources<'a> {
    pub fn prepare(
        allocator: &dyn Memory,
        capacity: usize,
        borrows: usize,
        budget: &mut m::Budget,
    ) -> Result<Self, Error> {
        budget
            .charge(capacity)
            .map_err(resource::PreparationError::Budget)?;
        let values = ResourceTable::prepare(capacity, borrows, budget)?;
        let plans = budget
            .vector(capacity)
            .map_err(resource::PreparationError::Budget)?;
        Ok(Self {
            values,
            allocator: allocator.identity(),
            next_ticket: Some(1),
            next_holder: Some(1),
            next_transfer: Some(1),
            pending: None,
            plans,
            capacity,
        })
    }
    pub fn host(&self) -> Holder {
        Holder {
            table: self.values.identity().expect("prepared identity"),
            scope: 0,
        }
    }
    pub fn new_holder(&mut self) -> Result<Holder, Error> {
        self.idle()?;
        let scope = self.next_holder.ok_or(Error::Exhausted)?;
        self.next_holder = scope.checked_add(1);
        Ok(Holder {
            scope,
            ..self.host()
        })
    }
    pub fn len(&self) -> usize {
        self.values.len()
    }
    pub(crate) const fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    fn idle(&self) -> Result<(), Error> {
        if self.pending.is_some() {
            Err(Error::State)
        } else {
            Ok(())
        }
    }
    fn check_allocator(&self, allocator: &dyn Memory) -> Result<(), Error> {
        if allocator.identity() != self.allocator {
            Err(Error::ForeignAllocator)
        } else {
            Ok(())
        }
    }
    fn check_holder(&self, holder: Holder) -> Result<(), Error> {
        if holder.table != self.host().table {
            Err(Error::ForeignHolder)
        } else {
            Ok(())
        }
    }
    fn tickets(&mut self, count: usize) -> Result<u64, Error> {
        if count == 0 {
            return Ok(0);
        }
        let first = self.next_ticket.ok_or(Error::Exhausted)?;
        let last = first
            .checked_add(u64::try_from(count - 1).map_err(|_| Error::Exhausted)?)
            .ok_or(Error::Exhausted)?;
        self.next_ticket = last.checked_add(1);
        Ok(first)
    }
    pub fn publish(
        &mut self,
        kind: ResourceKind<'a>,
        holder: Holder,
        bytes: &[u8],
        allocator: &mut dyn Memory,
    ) -> Result<Claim, Error> {
        self.idle()?;
        self.check_allocator(allocator)?;
        self.check_holder(holder)?;
        self.values.check_insert()?;
        let requests = [
            a::Request::new(HEADER_BYTES, 8)?,
            a::Request::new(bytes.len() as u64, 1)?,
        ];
        allocator.check(&requests)?;
        let ticket = self.tickets(1)?;
        let header = allocator.allocate(requests[0])?;
        let data = match allocator.allocate(requests[1]) {
            Ok(data) => data,
            Err(e) => {
                allocator.release(header)?;
                return Err(e.into());
            }
        };
        allocator.bytes_mut(data)?.copy_from_slice(bytes);
        let owner = match self.values.insert(Backing { kind, data, header }) {
            Ok(owner) => owner,
            Err(e) => {
                allocator.release(data)?;
                allocator.release(header)?;
                return Err(e.into());
            }
        };
        let claim = Claim { owner, ticket };
        let out = allocator.bytes_mut(header)?;
        out[..8].copy_from_slice(&MAGIC.to_le_bytes());
        out[8..32].copy_from_slice(&owner.token());
        out[32..40].copy_from_slice(&ticket.to_le_bytes());
        out[40..48].copy_from_slice(&holder.scope.to_le_bytes());
        Ok(claim)
    }
    fn backing(
        &self,
        claim: Claim,
        holder: Holder,
        allocator: &dyn Memory,
    ) -> Result<Backing<'a>, Error> {
        self.check_allocator(allocator)?;
        self.check_holder(holder)?;
        let backing = *self.values.owned_value(claim.owner)?;
        let bytes = allocator.bytes(backing.header)?;
        if bytes.len() != HEADER_BYTES as usize
            || bytes[..8] != MAGIC.to_le_bytes()
            || bytes[8..32] != claim.owner.token()
        {
            return Err(Error::State);
        }
        if bytes[32..40] != claim.ticket.to_le_bytes() {
            return Err(Error::StaleClaim);
        }
        if bytes[40..48] != holder.scope.to_le_bytes() {
            return Err(Error::WrongHolder);
        }
        allocator.request(backing.data)?;
        Ok(backing)
    }
    pub fn validate(
        &self,
        claim: Claim,
        holder: Holder,
        key: &ResourceTypeKey,
        allocator: &dyn Memory,
    ) -> Result<(), Error> {
        if self.backing(claim, holder, allocator)?.kind.key() != key {
            return Err(Error::StaticType);
        }
        Ok(())
    }
    pub fn bytes<'b>(
        &self,
        claim: Claim,
        holder: Holder,
        implementation: ResourceKind<'_>,
        allocator: &'b dyn Memory,
    ) -> Result<&'b [u8], Error> {
        let backing = self.backing(claim, holder, allocator)?;
        if !backing.kind.same_implementation(implementation) {
            return Err(Error::Implementation);
        }
        Ok(allocator.bytes(backing.data)?)
    }
    /// Preflight every owner before reserving fresh authority tickets. Dropping
    /// the prepared transfer preserves input ownership; reserved tickets expire.
    pub fn preflight<'r, 'host>(
        &'r mut self,
        claims: &[Claim],
        from: Holder,
        to: Holder,
        allocator: &'host mut dyn Memory,
    ) -> Result<Prepared<'r, 'host, 'a>, Error> {
        let ticket = self.prepare_transfer(claims, from, to, allocator)?;
        Ok(Prepared {
            resources: self,
            allocator,
            ticket,
            committed: false,
        })
    }
    /// Read-only whole-batch admission before any caller/callee representation
    /// allocates. The iterator borrows prepared caller transport; no ledger or
    /// ticket is changed, and no native/provider callback is invoked.
    pub(crate) fn check_transfer(
        &self,
        claims: impl Iterator<Item = Claim> + Clone,
        from: Holder,
        to: Holder,
        allocator: &dyn Memory,
    ) -> Result<(), Error> {
        self.idle()?;
        self.check_allocator(allocator)?;
        self.check_holder(from)?;
        self.check_holder(to)?;
        if claims.clone().count() > self.capacity {
            return Err(Error::Capacity);
        }
        for (index, claim) in claims.clone().enumerate() {
            self.backing(claim, from, allocator)?;
            self.values.check_release(claim.owner)?;
            if claims
                .clone()
                .take(index)
                .any(|prior| prior.owner == claim.owner)
            {
                return Err(Error::AliasedOwner);
            }
        }
        Ok(())
    }

    pub fn prepare_transfer(
        &mut self,
        claims: &[Claim],
        from: Holder,
        to: Holder,
        allocator: &dyn Memory,
    ) -> Result<Transfer, Error> {
        self.check_transfer(claims.iter().copied(), from, to, allocator)?;
        self.plans.clear();
        for claim in claims {
            let backing = self.backing(*claim, from, allocator)?;
            self.plans.push(Plan {
                original: *claim,
                next: *claim,
                header: backing.header,
            });
        }
        let first = self.tickets(claims.len())?;
        for (index, plan) in self.plans.iter_mut().enumerate() {
            plan.next.ticket = first + index as u64;
        }
        let serial = self.next_transfer.ok_or(Error::Exhausted)?;
        self.next_transfer = serial.checked_add(1);
        let ticket = Transfer {
            table: self.host().table,
            serial,
        };
        self.pending = Some(Pending { ticket, from, to });
        Ok(ticket)
    }
    pub fn prepared_claim(&self, ticket: Transfer, index: usize) -> Result<Claim, Error> {
        if self.pending.is_none_or(|p| p.ticket != ticket) {
            return Err(Error::State);
        }
        self.plans.get(index).map(|p| p.next).ok_or(Error::State)
    }
    pub fn abort_transfer(&mut self, ticket: Transfer) -> Result<(), Error> {
        if self.pending.is_none_or(|p| p.ticket != ticket) {
            return Err(Error::State);
        }
        self.pending = None;
        self.plans.clear();
        Ok(())
    }
    pub fn commit_transfer(
        &mut self,
        ticket: Transfer,
        allocator: &mut dyn Memory,
    ) -> Result<(), Error> {
        self.check_allocator(allocator)?;
        let pending = self
            .pending
            .filter(|p| p.ticket == ticket)
            .ok_or(Error::State)?;
        // Staging caller representation may allocate; revalidate every source
        // before the first write. The write phase has no fallible operation.
        for plan in &self.plans {
            self.backing(plan.original, pending.from, allocator)?;
            self.values.check_release(plan.original.owner)?;
        }
        for plan in &self.plans {
            let bytes = allocator
                .bytes_mut(plan.header)
                .expect("preflight holds live exact header bounds");
            bytes[32..40].copy_from_slice(&plan.next.ticket.to_le_bytes());
            bytes[40..48].copy_from_slice(&pending.to.scope.to_le_bytes());
        }
        self.pending = None;
        self.plans.clear();
        Ok(())
    }
    /// Unlink before the registered destructor is attempted. The caller must
    /// invoke that actual descriptor under its live original invocation context,
    /// then release both allocations even if destruction reports a failure.
    pub fn detach(
        &mut self,
        claim: Claim,
        holder: Holder,
        allocator: &dyn Memory,
    ) -> Result<Detached<'a>, Error> {
        self.idle()?;
        self.backing(claim, holder, allocator)?;
        let backing = self.values.release(claim.owner)?;
        Ok(Detached {
            backing: Some(backing),
            allocator: self.allocator,
        })
    }
}

pub struct Prepared<'r, 'host, 'code> {
    resources: &'r mut Resources<'code>,
    allocator: &'host mut dyn Memory,
    ticket: Transfer,
    committed: bool,
}
impl Prepared<'_, '_, '_> {
    pub fn claim(&self, index: usize) -> Option<Claim> {
        self.resources.prepared_claim(self.ticket, index).ok()
    }
    pub fn allocator(&mut self) -> &mut dyn Memory {
        self.allocator
    }
    pub fn commit(mut self) {
        self.resources
            .commit_transfer(self.ticket, self.allocator)
            .expect("exclusive prepared transfer remains valid");
        self.committed = true;
    }
}
impl Drop for Prepared<'_, '_, '_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.resources.abort_transfer(self.ticket);
        }
    }
}

pub struct Detached<'a> {
    backing: Option<Backing<'a>>,
    allocator: a::Identity,
}
impl<'a> Detached<'a> {
    pub fn kind(&self) -> Result<ResourceKind<'a>, Error> {
        Ok(self.backing.ok_or(Error::State)?.kind)
    }
    pub fn bytes_mut<'b>(&self, allocator: &'b mut dyn Memory) -> Result<&'b mut [u8], Error> {
        if allocator.identity() != self.allocator {
            return Err(Error::ForeignAllocator);
        }
        Ok(allocator.bytes_mut(self.backing.ok_or(Error::State)?.data)?)
    }
    pub fn release(&mut self, allocator: &mut dyn Memory) -> Result<(), Error> {
        if allocator.identity() != self.allocator {
            return Err(Error::ForeignAllocator);
        }
        let backing = self.backing.ok_or(Error::State)?;
        allocator.request(backing.data)?;
        allocator.request(backing.header)?;
        allocator.release(backing.data)?;
        allocator.release(backing.header)?;
        self.backing = None;
        Ok(())
    }
}
