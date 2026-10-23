//! In-memory component-resource handle prototype.
//!
//! Handles are intentionally copyable because an untrusted component can
//! duplicate their integer representation. The table, rather than Rust move
//! semantics, enforces generation, borrow, and exactly-once release rules.

use lang_mir::unit as m;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};

const FIRST_GENERATION: u64 = 1;
const FIRST_BORROW_ID: u64 = 1;
static NEXT_TABLE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OwnedResourceHandle {
    // Host-only provenance; the component wire lane remains instance-local u32.
    table: u64,
    slot: u32,
    generation: u64,
}

impl OwnedResourceHandle {
    pub(crate) fn token(self) -> [u8; 24] {
        let mut bytes = [0; 24];
        bytes[..8].copy_from_slice(&self.table.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.slot.to_le_bytes());
        bytes[16..].copy_from_slice(&self.generation.to_le_bytes());
        bytes
    }
    pub(crate) fn from_token(bytes: [u8; 24]) -> Result<Self, ResourceTableError> {
        let table = u64::from_le_bytes(bytes[..8].try_into().expect("fixed token"));
        let generation = u64::from_le_bytes(bytes[16..].try_into().expect("fixed token"));
        if bytes[12..16] != [0; 4] || table == 0 || generation == 0 {
            return Err(ResourceTableError::InvalidToken);
        }
        Ok(Self {
            table,
            slot: u32::from_le_bytes(bytes[8..12].try_into().expect("fixed token")),
            generation,
        })
    }
    pub const fn slot(self) -> u32 {
        self.slot
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }
}

impl Display for OwnedResourceHandle {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "resource[{}@{}]", self.slot, self.generation)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceBorrowKind {
    Shared,
    Exclusive,
}

impl Display for ResourceBorrowKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shared => formatter.write_str("shared"),
            Self::Exclusive => formatter.write_str("exclusive"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BorrowedResourceHandle {
    owner: OwnedResourceHandle,
    borrow_id: u64,
    kind: ResourceBorrowKind,
}

impl BorrowedResourceHandle {
    pub const fn owner(self) -> OwnedResourceHandle {
        self.owner
    }

    pub const fn borrow_id(self) -> u64 {
        self.borrow_id
    }

    pub const fn kind(self) -> ResourceBorrowKind {
        self.kind
    }
}

impl Display for BorrowedResourceHandle {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} borrow {} of {}",
            self.kind, self.borrow_id, self.owner
        )
    }
}

#[derive(Debug)]
struct ResourceSlot<T> {
    generation: u64,
    value: Option<T>,
    shared_borrows: usize,
    exclusive_borrow: Option<u64>,
}

impl<T> ResourceSlot<T> {
    fn occupied(generation: u64, value: T) -> Self {
        Self {
            generation,
            value: Some(value),
            shared_borrows: 0,
            exclusive_borrow: None,
        }
    }

    fn active_shared(&self) -> usize {
        self.shared_borrows
    }

    fn active_exclusive(&self) -> bool {
        self.exclusive_borrow.is_some()
    }
}

#[derive(Debug)]
pub struct ResourceTable<T> {
    identity: Option<u64>,
    slots: Vec<ResourceSlot<T>>,
    free_slots: Vec<u32>,
    borrows: Vec<BorrowedResourceHandle>,
    capacity: Option<(usize, usize)>,
    next_borrow_id: u64,
    active_resources: usize,
}

impl<T> Default for ResourceTable<T> {
    fn default() -> Self {
        Self {
            identity: NEXT_TABLE
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
                .ok(),
            slots: Vec::new(),
            free_slots: Vec::new(),
            borrows: Vec::new(),
            capacity: None,
            next_borrow_id: FIRST_BORROW_ID,
            active_resources: 0,
        }
    }
}

impl<T> ResourceTable<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Reserve trusted locator storage before entry. No insert, loan or release
    /// may grow these vectors in the prepared profile. Payload backing remains
    /// separately owned/accounted by the original instance allocator.
    pub fn prepare(
        resources: usize,
        borrows: usize,
        budget: &mut m::Budget,
    ) -> Result<Self, PreparationError> {
        u32::try_from(resources)
            .map_err(|_| PreparationError::Table(ResourceTableError::CapacityExhausted))?;
        let slots = budget.vector(resources).map_err(PreparationError::Budget)?;
        let free_slots = budget.vector(resources).map_err(PreparationError::Budget)?;
        let loans = budget.vector(borrows).map_err(PreparationError::Budget)?;
        let table = Self {
            slots,
            free_slots,
            borrows: loans,
            capacity: Some((resources, borrows)),
            ..Self::new()
        };
        if table.identity.is_none() {
            return Err(PreparationError::Table(
                ResourceTableError::TableIdExhausted,
            ));
        }
        Ok(table)
    }

    pub(crate) const fn identity(&self) -> Option<u64> {
        self.identity
    }
    pub(crate) fn check_insert(&self) -> Result<(), ResourceTableError> {
        self.identity.ok_or(ResourceTableError::TableIdExhausted)?;
        if self.free_slots.is_empty()
            && self
                .capacity
                .is_some_and(|(count, _)| self.slots.len() >= count)
        {
            return Err(ResourceTableError::CapacityExhausted);
        }
        Ok(())
    }
    pub(crate) fn check_release(
        &self,
        owner: OwnedResourceHandle,
    ) -> Result<(), ResourceTableError> {
        let slot = self.resolve_owned(owner)?;
        if slot.active_shared() != 0 || slot.active_exclusive() {
            return Err(ResourceTableError::ResourceInUse {
                owner,
                active_shared: slot.active_shared(),
                active_exclusive: slot.active_exclusive(),
            });
        }
        Ok(())
    }
    pub(crate) fn owned_value(&self, owner: OwnedResourceHandle) -> Result<&T, ResourceTableError> {
        Ok(self
            .resolve_owned(owner)?
            .value
            .as_ref()
            .expect("live resource"))
    }

    pub fn insert(&mut self, value: T) -> Result<OwnedResourceHandle, ResourceTableError> {
        let table = self.identity.ok_or(ResourceTableError::TableIdExhausted)?;
        let handle = if let Some(slot_index) = self.free_slots.pop() {
            let slot = &mut self.slots[slot_index as usize];
            debug_assert!(slot.value.is_none());
            debug_assert_eq!(slot.shared_borrows, 0);
            debug_assert!(slot.exclusive_borrow.is_none());
            slot.value = Some(value);
            OwnedResourceHandle {
                table,
                slot: slot_index,
                generation: slot.generation,
            }
        } else {
            if self
                .capacity
                .is_some_and(|(resources, _)| self.slots.len() >= resources)
            {
                return Err(ResourceTableError::CapacityExhausted);
            }
            let slot_index = u32::try_from(self.slots.len())
                .map_err(|_| ResourceTableError::CapacityExhausted)?;
            self.slots
                .try_reserve(1)
                .map_err(|_| ResourceTableError::AllocationFailed)?;
            // Release must never allocate, including for the legacy prototype.
            let count = self
                .slots
                .len()
                .checked_add(1)
                .ok_or(ResourceTableError::CapacityExhausted)?;
            if self.free_slots.capacity() < count {
                self.free_slots
                    .try_reserve_exact(count - self.free_slots.len())
                    .map_err(|_| ResourceTableError::AllocationFailed)?;
            }
            self.slots
                .push(ResourceSlot::occupied(FIRST_GENERATION, value));
            OwnedResourceHandle {
                table,
                slot: slot_index,
                generation: FIRST_GENERATION,
            }
        };
        self.active_resources += 1;
        Ok(handle)
    }

    /// Read-only admission for a complete prepared invocation. Reserve all
    /// identifiers and locator capacity before any borrow state is changed.
    pub(crate) fn check_borrow_reservation(&self, count: usize) -> Result<(), ResourceTableError> {
        let total = self
            .borrows
            .len()
            .checked_add(count)
            .ok_or(ResourceTableError::BorrowCapacityExhausted)?;
        if self.capacity.is_none_or(|(_, cap)| total > cap) || total > self.borrows.capacity() {
            return Err(ResourceTableError::BorrowCapacityExhausted);
        }
        self.next_borrow_id
            .checked_add(u64::try_from(count).map_err(|_| ResourceTableError::BorrowIdExhausted)?)
            .ok_or(ResourceTableError::BorrowIdExhausted)?;
        Ok(())
    }
    pub(crate) fn check_borrow(
        &self,
        owner: OwnedResourceHandle,
        requested: ResourceBorrowKind,
        shared_count: usize,
    ) -> Result<(), ResourceTableError> {
        let slot = self.resolve_owned(owner)?;
        if slot.active_exclusive()
            || (requested == ResourceBorrowKind::Exclusive && slot.active_shared() != 0)
        {
            return Err(ResourceTableError::BorrowConflict {
                owner,
                requested,
                active_shared: slot.active_shared(),
                active_exclusive: slot.active_exclusive(),
            });
        }
        slot.shared_borrows
            .checked_add(shared_count)
            .ok_or(ResourceTableError::BorrowCapacityExhausted)?;
        Ok(())
    }
    pub fn borrow_shared(
        &mut self,
        owner: OwnedResourceHandle,
    ) -> Result<BorrowedResourceHandle, ResourceTableError> {
        {
            let slot = self.resolve_owned(owner)?;
            if slot.active_exclusive() {
                return Err(ResourceTableError::BorrowConflict {
                    owner,
                    requested: ResourceBorrowKind::Shared,
                    active_shared: slot.active_shared(),
                    active_exclusive: true,
                });
            }
        }

        self.reserve_borrow()?;
        let shared = self
            .resolve_owned(owner)?
            .shared_borrows
            .checked_add(1)
            .ok_or(ResourceTableError::BorrowCapacityExhausted)?;
        let borrow_id = self.allocate_borrow_id()?;
        let borrowed = BorrowedResourceHandle {
            owner,
            borrow_id,
            kind: ResourceBorrowKind::Shared,
        };
        self.resolve_owned_mut(owner)?.shared_borrows = shared;
        self.borrows.push(borrowed);
        Ok(borrowed)
    }

    pub fn borrow_exclusive(
        &mut self,
        owner: OwnedResourceHandle,
    ) -> Result<BorrowedResourceHandle, ResourceTableError> {
        {
            let slot = self.resolve_owned(owner)?;
            if slot.active_shared() != 0 || slot.active_exclusive() {
                return Err(ResourceTableError::BorrowConflict {
                    owner,
                    requested: ResourceBorrowKind::Exclusive,
                    active_shared: slot.active_shared(),
                    active_exclusive: slot.active_exclusive(),
                });
            }
        }

        self.reserve_borrow()?;
        let borrow_id = self.allocate_borrow_id()?;
        self.resolve_owned_mut(owner)?.exclusive_borrow = Some(borrow_id);
        let borrowed = BorrowedResourceHandle {
            owner,
            borrow_id,
            kind: ResourceBorrowKind::Exclusive,
        };
        self.borrows.push(borrowed);
        Ok(borrowed)
    }

    pub fn get_borrowed(&self, borrowed: BorrowedResourceHandle) -> Result<&T, ResourceTableError> {
        let slot = self.resolve_borrowed(borrowed)?;
        Ok(slot.value.as_ref().expect("resolved slots are occupied"))
    }

    pub fn get_borrowed_mut(
        &mut self,
        borrowed: BorrowedResourceHandle,
    ) -> Result<&mut T, ResourceTableError> {
        if borrowed.kind != ResourceBorrowKind::Exclusive {
            self.resolve_borrowed(borrowed)?;
            return Err(ResourceTableError::SharedBorrowCannotMutate(borrowed));
        }
        let slot = self.resolve_borrowed_mut(borrowed)?;
        Ok(slot.value.as_mut().expect("resolved slots are occupied"))
    }

    pub fn end_borrow(
        &mut self,
        borrowed: BorrowedResourceHandle,
    ) -> Result<(), ResourceTableError> {
        let index = self
            .borrows
            .iter()
            .position(|b| *b == borrowed)
            .ok_or(ResourceTableError::StaleBorrowedHandle(borrowed))?;
        let slot = self
            .resolve_owned_mut(borrowed.owner)
            .map_err(|_| ResourceTableError::StaleBorrowedHandle(borrowed))?;
        let was_active = match borrowed.kind {
            ResourceBorrowKind::Shared if slot.shared_borrows != 0 => {
                slot.shared_borrows -= 1;
                true
            }
            ResourceBorrowKind::Shared => false,
            ResourceBorrowKind::Exclusive => {
                if slot.exclusive_borrow == Some(borrowed.borrow_id) {
                    slot.exclusive_borrow = None;
                    true
                } else {
                    false
                }
            }
        };
        if !was_active {
            return Err(ResourceTableError::StaleBorrowedHandle(borrowed));
        }
        self.borrows.swap_remove(index);
        Ok(())
    }

    pub fn release(&mut self, owner: OwnedResourceHandle) -> Result<T, ResourceTableError> {
        let (value, reusable) = {
            let slot = self.resolve_owned_mut(owner)?;
            if slot.active_shared() != 0 || slot.active_exclusive() {
                return Err(ResourceTableError::ResourceInUse {
                    owner,
                    active_shared: slot.active_shared(),
                    active_exclusive: slot.active_exclusive(),
                });
            }

            let value = slot.value.take().expect("resolved slots are occupied");
            let reusable = if let Some(next_generation) = slot.generation.checked_add(1) {
                slot.generation = next_generation;
                true
            } else {
                false
            };
            (value, reusable)
        };

        self.active_resources -= 1;
        if reusable {
            let index = self.free_slots.partition_point(|slot| *slot > owner.slot);
            self.free_slots.insert(index, owner.slot);
        }
        Ok(value)
    }

    pub const fn len(&self) -> usize {
        self.active_resources
    }

    pub(crate) fn entries(&self) -> impl Iterator<Item = (OwnedResourceHandle, &T)> {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.value.as_ref().map(|value| {
                (
                    OwnedResourceHandle {
                        table: self
                            .identity
                            .expect("occupied prepared table has an identity"),
                        slot: u32::try_from(index).expect("resource slots are bounded by u32"),
                        generation: slot.generation,
                    },
                    value,
                )
            })
        })
    }

    pub const fn is_empty(&self) -> bool {
        self.active_resources == 0
    }

    fn allocate_borrow_id(&mut self) -> Result<u64, ResourceTableError> {
        let borrow_id = self.next_borrow_id;
        self.next_borrow_id = self
            .next_borrow_id
            .checked_add(1)
            .ok_or(ResourceTableError::BorrowIdExhausted)?;
        Ok(borrow_id)
    }

    fn reserve_borrow(&mut self) -> Result<(), ResourceTableError> {
        if self
            .capacity
            .is_some_and(|(_, borrows)| self.borrows.len() >= borrows)
        {
            return Err(ResourceTableError::BorrowCapacityExhausted);
        }
        self.borrows
            .try_reserve(1)
            .map_err(|_| ResourceTableError::AllocationFailed)
    }

    fn resolve_owned(
        &self,
        owner: OwnedResourceHandle,
    ) -> Result<&ResourceSlot<T>, ResourceTableError> {
        if self.identity != Some(owner.table) {
            return Err(ResourceTableError::StaleOwnedHandle(owner));
        }
        let Some(slot) = self.slots.get(owner.slot as usize) else {
            return Err(ResourceTableError::StaleOwnedHandle(owner));
        };
        if slot.generation != owner.generation || slot.value.is_none() {
            return Err(ResourceTableError::StaleOwnedHandle(owner));
        }
        Ok(slot)
    }

    fn resolve_owned_mut(
        &mut self,
        owner: OwnedResourceHandle,
    ) -> Result<&mut ResourceSlot<T>, ResourceTableError> {
        if self.identity != Some(owner.table) {
            return Err(ResourceTableError::StaleOwnedHandle(owner));
        }
        let Some(slot) = self.slots.get_mut(owner.slot as usize) else {
            return Err(ResourceTableError::StaleOwnedHandle(owner));
        };
        if slot.generation != owner.generation || slot.value.is_none() {
            return Err(ResourceTableError::StaleOwnedHandle(owner));
        }
        Ok(slot)
    }

    fn resolve_borrowed(
        &self,
        borrowed: BorrowedResourceHandle,
    ) -> Result<&ResourceSlot<T>, ResourceTableError> {
        let slot = self
            .resolve_owned(borrowed.owner)
            .map_err(|_| ResourceTableError::StaleBorrowedHandle(borrowed))?;
        if !self.borrow_is_active(slot, borrowed) {
            return Err(ResourceTableError::StaleBorrowedHandle(borrowed));
        }
        Ok(slot)
    }

    fn resolve_borrowed_mut(
        &mut self,
        borrowed: BorrowedResourceHandle,
    ) -> Result<&mut ResourceSlot<T>, ResourceTableError> {
        self.resolve_borrowed(borrowed)?;
        self.resolve_owned_mut(borrowed.owner)
            .map_err(|_| ResourceTableError::StaleBorrowedHandle(borrowed))
    }

    fn borrow_is_active(&self, slot: &ResourceSlot<T>, borrowed: BorrowedResourceHandle) -> bool {
        self.borrows.contains(&borrowed)
            && match borrowed.kind {
                ResourceBorrowKind::Shared => slot.shared_borrows != 0,
                ResourceBorrowKind::Exclusive => slot.exclusive_borrow == Some(borrowed.borrow_id),
            }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceTableError {
    TableIdExhausted,
    CapacityExhausted,
    BorrowIdExhausted,
    BorrowCapacityExhausted,
    AllocationFailed,
    InvalidToken,
    StaleOwnedHandle(OwnedResourceHandle),
    StaleBorrowedHandle(BorrowedResourceHandle),
    BorrowConflict {
        owner: OwnedResourceHandle,
        requested: ResourceBorrowKind,
        active_shared: usize,
        active_exclusive: bool,
    },
    ResourceInUse {
        owner: OwnedResourceHandle,
        active_shared: usize,
        active_exclusive: bool,
    },
    SharedBorrowCannotMutate(BorrowedResourceHandle),
}

impl Display for ResourceTableError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TableIdExhausted => {
                formatter.write_str("component resource table identities are exhausted")
            }
            Self::CapacityExhausted => formatter.write_str("component resource table is full"),
            Self::BorrowIdExhausted => {
                formatter.write_str("component resource borrow identifiers are exhausted")
            }
            Self::BorrowCapacityExhausted => {
                formatter.write_str("component resource borrow table is full")
            }
            Self::AllocationFailed => {
                formatter.write_str("component resource locator allocation failed")
            }
            Self::InvalidToken => formatter.write_str("invalid private resource token"),
            Self::StaleOwnedHandle(owner) => {
                write!(formatter, "owned component handle {owner} is stale")
            }
            Self::StaleBorrowedHandle(borrowed) => {
                write!(formatter, "component {borrowed} is stale")
            }
            Self::BorrowConflict {
                owner,
                requested,
                active_shared,
                active_exclusive,
            } => write!(
                formatter,
                "cannot create {requested} borrow of {owner}; active shared borrows: {active_shared}, active exclusive borrow: {active_exclusive}"
            ),
            Self::ResourceInUse {
                owner,
                active_shared,
                active_exclusive,
            } => write!(
                formatter,
                "cannot release {owner}; active shared borrows: {active_shared}, active exclusive borrow: {active_exclusive}"
            ),
            Self::SharedBorrowCannotMutate(borrowed) => {
                write!(formatter, "component {borrowed} cannot mutate its resource")
            }
        }
    }
}

impl Error for ResourceTableError {}

#[derive(Debug)]
pub enum PreparationError {
    Budget(m::Error),
    Table(ResourceTableError),
}
impl Display for PreparationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "component resource preparation: {self:?}")
    }
}
impl Error for PreparationError {}

#[cfg(test)]
mod tests {
    use super::{OwnedResourceHandle, ResourceBorrowKind, ResourceTable, ResourceTableError};

    #[test]
    fn prepared_batch_refusals_preserve_existing_borrows_and_identifiers() {
        let mut budget = lang_mir::unit::Budget::new(Default::default());
        let mut table = ResourceTable::prepare(2, 2, &mut budget).unwrap();
        let first = table.insert(41).unwrap();
        let second = table.insert(42).unwrap();
        let loan = table.borrow_shared(first).unwrap();
        let next = table.next_borrow_id;
        assert!(matches!(
            table.check_borrow(first, ResourceBorrowKind::Exclusive, 0),
            Err(ResourceTableError::BorrowConflict { .. })
        ));
        assert_eq!(
            table.check_borrow_reservation(2),
            Err(ResourceTableError::BorrowCapacityExhausted)
        );
        table.check_borrow_reservation(1).unwrap();
        table
            .check_borrow(first, ResourceBorrowKind::Shared, 1)
            .unwrap();
        assert_eq!(table.next_borrow_id, next);
        assert_eq!(table.borrows.len(), 1);
        assert_eq!(*table.get_borrowed(loan).unwrap(), 41);
        table.end_borrow(loan).unwrap();
        table.next_borrow_id = u64::MAX;
        assert_eq!(
            table.check_borrow_reservation(1),
            Err(ResourceTableError::BorrowIdExhausted)
        );
        table.check_borrow_reservation(0).unwrap();
        assert!(table.borrows.is_empty());
        assert_eq!(table.next_borrow_id, u64::MAX);
        assert_eq!(table.release(first).unwrap(), 41);
        assert_eq!(table.release(second).unwrap(), 42);
    }

    #[test]
    fn prepared_table_has_exact_budget_and_fixed_owner_and_loan_capacity() {
        use lang_mir::unit as m;
        let mut budget = m::Budget::new(Default::default());
        ResourceTable::<i64>::prepare(2, 1, &mut budget).unwrap();
        let limits = m::Limits {
            work: budget.work_used(),
            allocations: budget.allocation_bytes(),
            elements: budget.elements_used(),
            ..Default::default()
        };
        let mut table = ResourceTable::prepare(2, 1, &mut m::Budget::new(limits)).unwrap();
        for below in [
            m::Limits {
                allocations: limits.allocations - 1,
                ..limits
            },
            m::Limits {
                elements: limits.elements - 1,
                ..limits
            },
        ] {
            assert!(ResourceTable::<i64>::prepare(2, 1, &mut m::Budget::new(below)).is_err());
        }
        let capacities = (
            table.slots.capacity(),
            table.free_slots.capacity(),
            table.borrows.capacity(),
        );
        let zero = table.insert(0).unwrap();
        let one = table.insert(1).unwrap();
        assert_eq!(table.insert(2), Err(ResourceTableError::CapacityExhausted));
        let borrowed = table.borrow_shared(zero).unwrap();
        assert_eq!(
            table.borrow_shared(zero),
            Err(ResourceTableError::BorrowCapacityExhausted)
        );
        table.end_borrow(borrowed).unwrap();
        table.release(one).unwrap();
        table.release(zero).unwrap();
        let replacement = table.insert(3).unwrap();
        assert_eq!(replacement.slot(), zero.slot());
        let borrowed = table.borrow_exclusive(replacement).unwrap();
        table.end_borrow(borrowed).unwrap();
        table.release(replacement).unwrap();
        assert_eq!(
            (
                table.slots.capacity(),
                table.free_slots.capacity(),
                table.borrows.capacity()
            ),
            capacities
        );
    }

    #[test]
    fn foreign_table_tokens_cannot_read_mutate_end_or_release_resources() {
        let mut first = ResourceTable::new();
        let mut second = ResourceTable::new();
        let first_owner = first.insert(11).unwrap();
        let second_owner = second.insert(22).unwrap();
        assert_eq!(first_owner.slot(), second_owner.slot());
        assert_eq!(first_owner.generation(), second_owner.generation());
        assert!(matches!(
            second.release(first_owner),
            Err(ResourceTableError::StaleOwnedHandle(_))
        ));
        let first_borrow = first.borrow_exclusive(first_owner).unwrap();
        let second_borrow = second.borrow_exclusive(second_owner).unwrap();
        assert!(second.get_borrowed(first_borrow).is_err());
        assert!(second.get_borrowed_mut(first_borrow).is_err());
        assert!(second.end_borrow(first_borrow).is_err());
        assert_eq!(*second.get_borrowed(second_borrow).unwrap(), 22);
        first.end_borrow(first_borrow).unwrap();
        second.end_borrow(second_borrow).unwrap();
        assert_eq!(first.release(first_owner).unwrap(), 11);
        assert_eq!(second.release(second_owner).unwrap(), 22);
    }

    #[test]
    fn exhausted_table_identity_cannot_publish_a_handle() {
        let mut table = ResourceTable {
            identity: None,
            ..ResourceTable::new()
        };
        assert_eq!(table.insert(1), Err(ResourceTableError::TableIdExhausted));
        assert!(table.is_empty());
    }

    #[test]
    fn generation_tags_prevent_stale_handles_from_aliasing_reused_slots() {
        let mut table = ResourceTable::new();
        let first = table.insert(String::from("first")).unwrap();
        let ended_borrow = table.borrow_shared(first).unwrap();
        table.end_borrow(ended_borrow).unwrap();

        assert_eq!(table.release(first), Ok(String::from("first")));
        assert_eq!(
            table.release(first),
            Err(ResourceTableError::StaleOwnedHandle(first))
        );

        let replacement = table.insert(String::from("replacement")).unwrap();
        assert_eq!(replacement.slot(), first.slot());
        assert_eq!(replacement.generation(), first.generation() + 1);
        assert_eq!(
            table.borrow_shared(first),
            Err(ResourceTableError::StaleOwnedHandle(first))
        );
        assert_eq!(
            table.end_borrow(ended_borrow),
            Err(ResourceTableError::StaleBorrowedHandle(ended_borrow))
        );
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn shared_borrows_coexist_and_block_exclusive_borrows_and_release() {
        let mut table = ResourceTable::new();
        let owner = table.insert(41).unwrap();
        let first = table.borrow_shared(owner).unwrap();
        let second = table.borrow_shared(owner).unwrap();

        assert_eq!(table.get_borrowed(first), Ok(&41));
        assert_eq!(
            table.borrow_exclusive(owner),
            Err(ResourceTableError::BorrowConflict {
                owner,
                requested: ResourceBorrowKind::Exclusive,
                active_shared: 2,
                active_exclusive: false,
            })
        );
        assert_eq!(
            table.release(owner),
            Err(ResourceTableError::ResourceInUse {
                owner,
                active_shared: 2,
                active_exclusive: false,
            })
        );

        table.end_borrow(first).unwrap();
        table.end_borrow(second).unwrap();
        assert_eq!(table.release(owner), Ok(41));
        assert!(table.is_empty());
    }

    #[test]
    fn exclusive_borrow_allows_mutation_and_blocks_shared_borrows() {
        let mut table = ResourceTable::new();
        let owner = table.insert(vec![1, 2]).unwrap();
        let exclusive = table.borrow_exclusive(owner).unwrap();

        table.get_borrowed_mut(exclusive).unwrap().push(3);
        assert_eq!(table.get_borrowed(exclusive), Ok(&vec![1, 2, 3]));
        assert_eq!(
            table.borrow_shared(owner),
            Err(ResourceTableError::BorrowConflict {
                owner,
                requested: ResourceBorrowKind::Shared,
                active_shared: 0,
                active_exclusive: true,
            })
        );

        table.end_borrow(exclusive).unwrap();
        assert_eq!(table.release(owner), Ok(vec![1, 2, 3]));
    }

    #[test]
    fn ended_or_shared_borrows_cannot_be_used_for_mutation() {
        let mut table = ResourceTable::new();
        let owner = table.insert(7).unwrap();
        let shared = table.borrow_shared(owner).unwrap();

        assert_eq!(
            table.get_borrowed_mut(shared),
            Err(ResourceTableError::SharedBorrowCannotMutate(shared))
        );
        table.end_borrow(shared).unwrap();
        assert_eq!(
            table.end_borrow(shared),
            Err(ResourceTableError::StaleBorrowedHandle(shared))
        );
        assert_eq!(
            table.get_borrowed(shared),
            Err(ResourceTableError::StaleBorrowedHandle(shared))
        );
    }

    #[test]
    fn reuse_order_and_borrow_exhaustion_are_deterministic() {
        let mut table = ResourceTable::new();
        let zero = table.insert(0).unwrap();
        let one = table.insert(1).unwrap();
        let two = table.insert(2).unwrap();
        table.release(two).unwrap();
        table.release(zero).unwrap();

        let first_replacement = table.insert(10).unwrap();
        let second_replacement = table.insert(20).unwrap();
        assert_eq!(first_replacement.slot(), zero.slot());
        assert_eq!(second_replacement.slot(), two.slot());

        table.next_borrow_id = u64::MAX;
        assert_eq!(
            table.borrow_shared(one),
            Err(ResourceTableError::BorrowIdExhausted)
        );
    }

    #[test]
    fn generation_exhaustion_retires_the_slot_instead_of_wrapping() {
        let mut table = ResourceTable::new();
        let original = table.insert(5).unwrap();
        table.slots[original.slot() as usize].generation = u64::MAX;
        let exhausted = OwnedResourceHandle {
            table: original.table,
            slot: original.slot(),
            generation: u64::MAX,
        };

        assert_eq!(table.release(exhausted), Ok(5));
        let replacement = table.insert(6).unwrap();
        assert_ne!(replacement.slot(), exhausted.slot());
        assert_eq!(
            table.borrow_shared(exhausted),
            Err(ResourceTableError::StaleOwnedHandle(exhausted))
        );
    }
}
