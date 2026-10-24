//! Allocator-backed component owners. Private claims are not wire authority.
//! Locator storage is reserved before entry; moving ownership changes the real
//! provider-backed header and never allocates or invokes native code.
use super::{
    native::ResourceKind,
    negotiation::ResourceTypeKey,
    resource::{
        self, BorrowedResourceHandle, OwnedResourceHandle, ResourceBorrowKind, ResourceTable,
    },
};
use crate::allocation as a;
use lang_mir::unit as m;
use std::fmt;

const MAGIC: u64 = 0x636f_6d70_6f77_6e31;
const HEADER_BYTES: u64 = 48;

/// The complete erased allocator access still delegates to original storage.
pub use crate::allocation::Memory;

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
    pub(crate) fn same_owner(self, other: Self) -> bool {
        self.owner == other.owner
    }
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
#[derive(Clone)]
struct Backing<'a> {
    kind: ResourceKind<'a>,
    data: a::Handle,
    header: a::Handle,
}
#[derive(Clone, Copy)]
pub(crate) enum Access {
    Own,
    Shared,
    Exclusive,
}
#[derive(Clone, Copy)]
pub(crate) struct Input {
    pub claim: Claim,
    pub access: Access,
}
#[derive(Clone, Copy)]
pub(crate) struct Loan {
    borrowed: BorrowedResourceHandle,
    claim: Claim,
    caller: Holder,
    guest: Holder,
}
#[derive(Clone, Copy)]
struct Plan {
    original: Claim,
    next: Claim,
    header: a::Handle,
    access: Access,
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
    borrows: usize,
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
            .vector(capacity.checked_add(borrows).ok_or(Error::Capacity)?)
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
            borrows,
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
    pub(crate) fn input_capacity(&self) -> usize {
        self.capacity + self.borrows
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
    pub(super) fn check_holder(&self, holder: Holder) -> Result<(), Error> {
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
        self.publish_aligned(kind, holder, bytes, 1, allocator)
    }
    /// Copy caller bytes with their requested alignment through this table's
    /// original allocator. Every request is admitted before allocation/custody.
    pub fn publish_aligned(
        &mut self,
        kind: ResourceKind<'a>,
        holder: Holder,
        bytes: &[u8],
        alignment: u64,
        allocator: &mut dyn Memory,
    ) -> Result<Claim, Error> {
        self.idle()?;
        self.check_allocator(allocator)?;
        self.check_holder(holder)?;
        self.values.check_insert()?;
        let requests = [
            a::Request::new(HEADER_BYTES, 8)?,
            a::Request::new(bytes.len() as u64, alignment)?,
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
    /// Publish an existing context buffer without copying or changing its
    /// allocation request. Refusal leaves that original buffer with its caller.
    pub(crate) fn publish_backing(
        &mut self,
        kind: ResourceKind<'a>,
        holder: Holder,
        data: a::Handle,
        allocator: &mut dyn Memory,
    ) -> Result<Claim, Error> {
        self.idle()?;
        self.check_allocator(allocator)?;
        self.check_holder(holder)?;
        self.values.check_insert()?;
        allocator.request(data)?;
        let request = a::Request::new(HEADER_BYTES, 8)?;
        allocator.check(&[request])?;
        let ticket = self.tickets(1)?;
        let header = allocator.allocate(request)?;
        let out = match allocator.bytes_mut(header) {
            Ok(bytes) if bytes.len() == HEADER_BYTES as usize => bytes,
            _ => {
                allocator.release(header)?;
                return Err(Error::State);
            }
        };
        let owner = match self.values.insert(Backing { kind, data, header }) {
            Ok(owner) => owner,
            Err(error) => {
                allocator.release(header)?;
                return Err(error.into());
            }
        };
        let claim = Claim { owner, ticket };
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
    ) -> Result<&Backing<'a>, Error> {
        self.check_allocator(allocator)?;
        self.check_holder(holder)?;
        let backing = self.values.owned_value(claim.owner)?;
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
    /// Preflight retirement and retain its original implementation, validating
    /// the live table, ticket, holder, allocator and absence of active loans.
    pub fn retirement_kind(
        &self,
        claim: Claim,
        holder: Holder,
        allocator: &dyn Memory,
    ) -> Result<ResourceKind<'a>, Error> {
        self.idle()?;
        let backing = self.backing(claim, holder, allocator)?;
        self.values.check_release(claim.owner)?;
        Ok(backing.kind.clone())
    }
    /// Preflight only this holder's live owners, newest caller publication
    /// first. This neither detaches owners nor invokes implementation code.
    pub fn held_claims(
        &self,
        holder: Holder,
        allocator: &dyn Memory,
        budget: &mut m::Budget,
    ) -> Result<Vec<Claim>, Error> {
        self.idle()?;
        self.check_allocator(allocator)?;
        self.check_holder(holder)?;
        budget
            .charge(self.values.len())
            .map_err(|_| Error::Capacity)?;
        let mut claims = budget
            .vector(self.values.len())
            .map_err(|_| Error::Capacity)?;
        for (owner, backing) in self.values.entries() {
            let bytes = allocator.bytes(backing.header)?;
            if bytes.len() != HEADER_BYTES as usize {
                return Err(Error::State);
            }
            if bytes[40..48] != holder.scope.to_le_bytes() {
                continue;
            }
            let ticket = u64::from_le_bytes(bytes[32..40].try_into().expect("checked header"));
            if ticket == 0 || self.next_ticket.is_some_and(|next| ticket >= next) {
                return Err(Error::State);
            }
            let claim = Claim { owner, ticket };
            self.backing(claim, holder, allocator)?;
            self.values.check_release(owner)?;
            claims.push(claim);
        }
        let work = claims
            .len()
            .saturating_mul(claims.len().checked_ilog2().unwrap_or(0) as usize + 1)
            .saturating_mul(4);
        budget.charge(work).map_err(|_| Error::Capacity)?;
        claims.sort_unstable_by_key(|claim| std::cmp::Reverse(claim.ticket));
        Ok(claims)
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
        if !backing.kind.same_implementation(&implementation) {
            return Err(Error::Implementation);
        }
        Ok(allocator.bytes(backing.data)?)
    }
    /// Validate the original table without allocating, calling native code or
    /// changing ownership. Only the preflighted return transfer may retain a
    /// guest owner before publication; completion permits none.
    pub(crate) fn scope_closed(
        &self,
        caller: Holder,
        guest: Holder,
        inputs: &[Input],
        transfer: Option<Transfer>,
        memory: &dyn Memory,
    ) -> Result<(), Error> {
        self.check_allocator(memory)?;
        self.check_holder(caller)?;
        self.check_holder(guest)?;
        for input in inputs
            .iter()
            .filter(|input| !matches!(input.access, Access::Own))
        {
            self.backing(input.claim, caller, memory)?;
        }
        match (self.pending, transfer) {
            (None, None) => {}
            (Some(pending), Some(ticket))
                if pending.ticket == ticket && pending.from == guest && pending.to == caller => {}
            _ => return Err(Error::State),
        }
        for (owner, backing) in self.values.entries() {
            let bytes = memory.bytes(backing.header)?;
            if bytes.len() != HEADER_BYTES as usize
                || bytes[..8] != MAGIC.to_le_bytes()
                || bytes[8..32] != owner.token()
            {
                return Err(Error::State);
            }
            let ticket = u64::from_le_bytes(bytes[32..40].try_into().expect("checked header"));
            let scope = u64::from_le_bytes(bytes[40..48].try_into().expect("checked header"));
            if ticket == 0
                || self.next_ticket.is_some_and(|next| ticket >= next)
                || self.next_holder.is_some_and(|next| scope >= next)
            {
                return Err(Error::State);
            }
            memory.bytes(backing.data)?;
            if scope == guest.scope
                && !(transfer.is_some()
                    && self.plans.iter().any(|plan| {
                        plan.original == Claim { owner, ticket }
                            && matches!(plan.access, Access::Own)
                    }))
            {
                return Err(Error::State);
            }
        }
        Ok(())
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
    /// Read-only whole-call admission. Shared aliases are permitted; every
    /// owner/exclusive collision is refused before staging provider storage.
    pub(crate) fn check_call(
        &self,
        inputs: impl Iterator<Item = Input> + Clone,
        from: Holder,
        to: Holder,
        allocator: &dyn Memory,
    ) -> Result<(), Error> {
        self.idle()?;
        self.check_inputs(inputs, from, to, allocator)
    }
    fn check_inputs(
        &self,
        inputs: impl Iterator<Item = Input> + Clone,
        from: Holder,
        to: Holder,
        allocator: &dyn Memory,
    ) -> Result<(), Error> {
        self.check_allocator(allocator)?;
        self.check_holder(from)?;
        self.check_holder(to)?;
        let owners = inputs
            .clone()
            .filter(|i| matches!(i.access, Access::Own))
            .count();
        let borrows = inputs
            .clone()
            .filter(|i| !matches!(i.access, Access::Own))
            .count();
        if owners > self.capacity || borrows > self.borrows {
            return Err(Error::Capacity);
        }
        self.values.check_borrow_reservation(borrows)?;
        for (index, input) in inputs.clone().enumerate() {
            self.backing(input.claim, from, allocator)?;
            if inputs.clone().take(index).any(|prior| {
                prior.claim.owner == input.claim.owner
                    && !(matches!(prior.access, Access::Shared)
                        && matches!(input.access, Access::Shared))
            }) {
                return Err(Error::AliasedOwner);
            }
            match input.access {
                Access::Own => self.values.check_release(input.claim.owner)?,
                access => {
                    let shared = inputs
                        .clone()
                        .filter(|i| {
                            i.claim.owner == input.claim.owner && matches!(i.access, Access::Shared)
                        })
                        .count();
                    self.values.check_borrow(
                        input.claim.owner,
                        if matches!(access, Access::Shared) {
                            ResourceBorrowKind::Shared
                        } else {
                            ResourceBorrowKind::Exclusive
                        },
                        shared,
                    )?;
                }
            }
        }
        Ok(())
    }
    pub(crate) fn check_transfer(
        &self,
        claims: impl Iterator<Item = Claim> + Clone,
        from: Holder,
        to: Holder,
        allocator: &dyn Memory,
    ) -> Result<(), Error> {
        self.check_call(
            claims.map(|claim| Input {
                claim,
                access: Access::Own,
            }),
            from,
            to,
            allocator,
        )
    }
    pub fn prepare_transfer(
        &mut self,
        claims: &[Claim],
        from: Holder,
        to: Holder,
        allocator: &dyn Memory,
    ) -> Result<Transfer, Error> {
        self.prepare_call(
            claims.iter().copied().map(|claim| Input {
                claim,
                access: Access::Own,
            }),
            from,
            to,
            allocator,
        )
    }
    pub(crate) fn prepare_call(
        &mut self,
        inputs: impl Iterator<Item = Input> + Clone,
        from: Holder,
        to: Holder,
        allocator: &dyn Memory,
    ) -> Result<Transfer, Error> {
        self.check_call(inputs.clone(), from, to, allocator)?;
        self.plans.clear();
        for input in inputs {
            let backing = self.backing(input.claim, from, allocator)?;
            self.plans.push(Plan {
                original: input.claim,
                next: input.claim,
                header: backing.header,
                access: input.access,
            });
        }
        let owners = self
            .plans
            .iter()
            .filter(|p| matches!(p.access, Access::Own))
            .count();
        let mut next = self.tickets(owners)?;
        for plan in &mut self.plans {
            if matches!(plan.access, Access::Own) {
                plan.next.ticket = next;
                next = next.saturating_add(1);
            }
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
        if self.plans.iter().any(|p| !matches!(p.access, Access::Own)) {
            return Err(Error::State);
        }
        self.commit_call_inner(ticket, allocator, None)
    }
    pub(crate) fn commit_call(
        &mut self,
        ticket: Transfer,
        allocator: &mut dyn Memory,
        loans: &mut [Option<Loan>],
    ) -> Result<(), Error> {
        self.commit_call_inner(ticket, allocator, Some(loans))
    }
    fn commit_call_inner(
        &mut self,
        ticket: Transfer,
        allocator: &mut dyn Memory,
        mut loans: Option<&mut [Option<Loan>]>,
    ) -> Result<(), Error> {
        let pending = self
            .pending
            .filter(|p| p.ticket == ticket)
            .ok_or(Error::State)?;
        if loans.as_ref().is_some_and(|l| l.len() != self.plans.len()) {
            return Err(Error::State);
        }
        self.check_inputs(
            self.plans.iter().map(|p| Input {
                claim: p.original,
                access: p.access,
            }),
            pending.from,
            pending.to,
            allocator,
        )?;
        // All bounds, identifiers, conflicts and vector capacity were checked.
        // These operations invoke no provider/native callback or allocation.
        for (index, plan) in self.plans.iter().enumerate() {
            match plan.access {
                Access::Own => {
                    let bytes = allocator
                        .bytes_mut(plan.header)
                        .expect("preflight holds live exact header bounds");
                    bytes[32..40].copy_from_slice(&plan.next.ticket.to_le_bytes());
                    bytes[40..48].copy_from_slice(&pending.to.scope.to_le_bytes());
                }
                access => {
                    let borrowed = if matches!(access, Access::Shared) {
                        self.values.borrow_shared(plan.original.owner)
                    } else {
                        self.values.borrow_exclusive(plan.original.owner)
                    }
                    .expect("complete batch preflight reserves compatible table borrows");
                    loans
                        .as_deref_mut()
                        .expect("borrow call reserves its result slots")[index] = Some(Loan {
                        borrowed,
                        claim: plan.original,
                        caller: pending.from,
                        guest: pending.to,
                    });
                }
            }
        }
        self.pending = None;
        self.plans.clear();
        Ok(())
    }
    pub(crate) fn check_loan(
        &self,
        loan: Loan,
        claim: Claim,
        guest: Holder,
        allocator: &dyn Memory,
    ) -> Result<(), Error> {
        if loan.claim != claim || loan.guest != guest {
            return Err(Error::WrongHolder);
        }
        self.values.get_borrowed(loan.borrowed)?;
        self.backing(claim, loan.caller, allocator)?;
        Ok(())
    }
    pub(crate) fn end_loan(&mut self, loan: Loan) -> Result<(), Error> {
        self.values.end_borrow(loan.borrowed)?;
        Ok(())
    }
    pub(crate) fn check_view(
        &self,
        loan: Loan,
        kind: ResourceKind<'_>,
        access: super::idl::Access,
    ) -> Result<(), Error> {
        let backing = self.values.get_borrowed(loan.borrowed)?;
        if !backing.kind.same_implementation(&kind) {
            return Err(Error::Implementation);
        }
        if access == super::idl::Access::Exclusive
            && loan.borrowed.kind() != ResourceBorrowKind::Exclusive
        {
            return Err(
                resource::ResourceTableError::SharedBorrowCannotMutate(loan.borrowed).into(),
            );
        }
        Ok(())
    }
    pub(crate) fn view(
        &self,
        loan: Loan,
        kind: ResourceKind<'_>,
        access: super::idl::Access,
        memory: &mut dyn Memory,
    ) -> Result<super::native::MutBytes, Error> {
        self.check_loan(loan, loan.claim, loan.guest, memory)?;
        self.check_view(loan, kind, access)?;
        let backing = self.values.get_borrowed(loan.borrowed)?;
        if access == super::idl::Access::Exclusive {
            let bytes = memory.bytes_mut(backing.data)?;
            Ok(super::native::MutBytes {
                data: bytes.as_mut_ptr(),
                length: bytes.len() as u64,
            })
        } else {
            let bytes = memory.bytes(backing.data)?;
            Ok(super::native::MutBytes {
                data: bytes.as_ptr().cast_mut(),
                length: bytes.len() as u64,
            })
        }
    }
    pub(crate) fn owned_view(
        &self,
        claim: Claim,
        holder: Holder,
        kind: ResourceKind<'_>,
        memory: &mut dyn Memory,
    ) -> Result<super::native::MutBytes, Error> {
        self.bytes(claim, holder, kind, memory)?;
        let backing = self.backing(claim, holder, memory)?;
        let bytes = memory.bytes_mut(backing.data)?;
        Ok(super::native::MutBytes {
            data: bytes.as_mut_ptr(),
            length: bytes.len() as u64,
        })
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
        Ok(self.backing.as_ref().ok_or(Error::State)?.kind.clone())
    }
    pub fn bytes_mut<'b>(&self, allocator: &'b mut dyn Memory) -> Result<&'b mut [u8], Error> {
        if allocator.identity() != self.allocator {
            return Err(Error::ForeignAllocator);
        }
        Ok(allocator.bytes_mut(self.backing.as_ref().ok_or(Error::State)?.data)?)
    }
    pub fn release(&mut self, allocator: &mut dyn Memory) -> Result<(), Error> {
        if allocator.identity() != self.allocator {
            return Err(Error::ForeignAllocator);
        }
        let backing = self.backing.as_ref().ok_or(Error::State)?;
        allocator.request(backing.data)?;
        allocator.request(backing.header)?;
        allocator.release(backing.data)?;
        allocator.release(backing.header)?;
        self.backing = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::negotiation::{InterfaceKey, PackageKey};
    use super::*;
    use crate::component::{
        idl, native,
        serialization::{DigestAlgorithm, VersionedDigest},
        wire,
    };
    use std::{ffi::c_void, ptr};

    extern "C" fn invoke(
        _: *mut c_void,
        _: *const native::Context,
        _: native::Bytes,
        _: *mut *mut native::Buffer,
    ) -> u32 {
        0
    }
    extern "C" fn destroy(
        _: *mut c_void,
        _: *const native::Context,
        _: native::MutBytes,
        _: *mut *mut native::Buffer,
    ) -> u32 {
        0
    }

    #[test]
    fn scope_validation_checks_physical_headers_and_the_exact_return_transfer() {
        let document = idl::check(
            m::SourceId(1),
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../tests/fixtures/component_counter.idl.json"
            )),
            Default::default(),
        )
        .unwrap();
        let mut session = wire::Session::new(Default::default());
        let contract = wire::Contract::new(document, vec![], &mut session).unwrap();
        let digest = VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap();
        let methods = [native::Method {
            interface: 0,
            member: 0,
            invoke,
            user: ptr::null_mut(),
            flags: 0,
        }];
        let descriptors = [native::ResourceType {
            interface: 0,
            resource: 0,
            destroy,
            user: ptr::null_mut(),
        }];
        let provider = native::Provider::with_resources(
            wire::Provider {
                contract: &contract,
                digest: &digest,
            },
            &methods,
            &descriptors,
            &(),
            &mut session,
        )
        .unwrap();
        let key = ResourceTypeKey::new(
            InterfaceKey::new(PackageKey::new("demo", "counter"), "api"),
            "counter",
        );
        let kind = provider.resource_kind(&key, &mut session).unwrap();
        let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut budget = m::Budget::new(Default::default());
        let mut resources = Resources::prepare(&allocator, 4, 2, &mut budget).unwrap();
        let guest = resources.new_holder().unwrap();
        let before_invalid = allocator.stats();
        assert!(
            resources
                .publish_aligned(
                    kind.clone(),
                    guest,
                    &41_i64.to_le_bytes(),
                    3,
                    &mut allocator
                )
                .is_err()
        );
        assert!(resources.is_empty());
        assert_eq!(allocator.stats(), before_invalid);
        let claim = resources
            .publish_aligned(
                kind.clone(),
                guest,
                &41_i64.to_le_bytes(),
                64,
                &mut allocator,
            )
            .unwrap();
        assert_eq!(
            resources
                .bytes(claim, guest, kind.clone(), &allocator)
                .unwrap()
                .as_ptr() as usize
                % 64,
            0
        );
        let caller = resources.host();
        assert!(matches!(
            resources.scope_closed(caller, guest, &[], None, &allocator),
            Err(Error::State)
        ));
        let transfer = resources
            .prepare_transfer(&[claim], guest, caller, &allocator)
            .unwrap();
        let returned = resources.prepared_claim(transfer, 0).unwrap();
        let header = resources.values.owned_value(claim.owner).unwrap().header;
        let before = allocator.stats();
        resources
            .scope_closed(caller, guest, &[], Some(transfer), &allocator)
            .unwrap();
        for (offset, invalid, original) in [
            (32, 0_u64.to_le_bytes(), claim.ticket.to_le_bytes()),
            (32, u64::MAX.to_le_bytes(), claim.ticket.to_le_bytes()),
            (
                8,
                0_u64.to_le_bytes(),
                claim.owner.token()[..8].try_into().unwrap(),
            ),
            (
                24,
                (u64::from_le_bytes(claim.owner.token()[16..24].try_into().unwrap()) ^ 1)
                    .to_le_bytes(),
                claim.owner.token()[16..24].try_into().unwrap(),
            ),
            (40, u64::MAX.to_le_bytes(), guest.scope.to_le_bytes()),
        ] {
            allocator.bytes_mut(header).unwrap()[offset..offset + 8].copy_from_slice(&invalid);
            assert!(matches!(
                resources.scope_closed(caller, guest, &[], Some(transfer), &allocator),
                Err(Error::State)
            ));
            allocator.bytes_mut(header).unwrap()[offset..offset + 8].copy_from_slice(&original);
            resources
                .scope_closed(caller, guest, &[], Some(transfer), &allocator)
                .unwrap();
        }
        assert_eq!(allocator.stats(), before);
        resources.commit_transfer(transfer, &mut allocator).unwrap();
        resources
            .scope_closed(caller, guest, &[], None, &allocator)
            .unwrap();
        let borrowed = [Input {
            claim: returned,
            access: Access::Shared,
        }];
        allocator.bytes_mut(header).unwrap()[32..40].copy_from_slice(&claim.ticket.to_le_bytes());
        assert!(matches!(
            resources.scope_closed(caller, guest, &borrowed, None, &allocator),
            Err(Error::StaleClaim)
        ));
        allocator.bytes_mut(header).unwrap()[32..40]
            .copy_from_slice(&returned.ticket.to_le_bytes());
        resources
            .scope_closed(caller, guest, &borrowed, None, &allocator)
            .unwrap();
        assert!(resources.validate(claim, guest, &key, &allocator).is_err());
        resources
            .validate(returned, caller, &key, &allocator)
            .unwrap();
        resources
            .detach(returned, caller, &allocator)
            .unwrap()
            .release(&mut allocator)
            .unwrap();
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(resources.is_empty());

        let other = resources.new_holder().unwrap();
        let first = resources
            .publish(kind.clone(), caller, &[1], &mut allocator)
            .unwrap();
        let foreign = resources
            .publish(kind.clone(), other, &[2], &mut allocator)
            .unwrap();
        let latest = resources
            .publish(kind, caller, &[3], &mut allocator)
            .unwrap();
        let before = allocator.stats();
        let mut refused = m::Budget::new(m::Limits {
            work: 0,
            ..Default::default()
        });
        assert!(matches!(
            resources.held_claims(caller, &allocator, &mut refused),
            Err(Error::Capacity)
        ));
        let foreign_allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        assert!(matches!(
            resources.held_claims(caller, &foreign_allocator, &mut budget),
            Err(Error::ForeignAllocator)
        ));
        assert_eq!(
            resources
                .held_claims(caller, &allocator, &mut budget)
                .unwrap(),
            [latest, first]
        );
        let inputs = [Input {
            claim: first,
            access: Access::Shared,
        }];
        let transfer = resources
            .prepare_call(inputs.iter().copied(), caller, guest, &allocator)
            .unwrap();
        let mut loans = [None];
        resources
            .commit_call(transfer, &mut allocator, &mut loans)
            .unwrap();
        assert!(
            resources
                .held_claims(caller, &allocator, &mut budget)
                .is_err()
        );
        assert_eq!(allocator.stats(), before);
        resources.end_loan(loans[0].unwrap()).unwrap();
        for claim in resources
            .held_claims(caller, &allocator, &mut budget)
            .unwrap()
        {
            resources
                .detach(claim, caller, &allocator)
                .unwrap()
                .release(&mut allocator)
                .unwrap();
        }
        resources
            .validate(foreign, other, &key, &allocator)
            .unwrap();
        assert_eq!(
            resources
                .held_claims(other, &allocator, &mut budget)
                .unwrap(),
            [foreign]
        );
        resources
            .detach(foreign, other, &allocator)
            .unwrap()
            .release(&mut allocator)
            .unwrap();
        assert!(resources.is_empty());
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}
