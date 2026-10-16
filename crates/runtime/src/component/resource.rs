//! In-memory component-resource handle prototype.
//!
//! Handles are intentionally copyable because an untrusted component can
//! duplicate their integer representation. The table, rather than Rust move
//! semantics, enforces generation, borrow, and exactly-once release rules.

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt::{Display, Formatter};

const FIRST_GENERATION: u64 = 1;
const FIRST_BORROW_ID: u64 = 1;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct OwnedResourceHandle {
    slot: u32,
    generation: u64,
}

impl OwnedResourceHandle {
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
    shared_borrows: BTreeSet<u64>,
    exclusive_borrow: Option<u64>,
}

impl<T> ResourceSlot<T> {
    fn occupied(generation: u64, value: T) -> Self {
        Self {
            generation,
            value: Some(value),
            shared_borrows: BTreeSet::new(),
            exclusive_borrow: None,
        }
    }

    fn active_shared(&self) -> usize {
        self.shared_borrows.len()
    }

    fn active_exclusive(&self) -> bool {
        self.exclusive_borrow.is_some()
    }
}

#[derive(Debug)]
pub struct ResourceTable<T> {
    slots: Vec<ResourceSlot<T>>,
    free_slots: BTreeSet<u32>,
    next_borrow_id: u64,
    active_resources: usize,
}

impl<T> Default for ResourceTable<T> {
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free_slots: BTreeSet::new(),
            next_borrow_id: FIRST_BORROW_ID,
            active_resources: 0,
        }
    }
}

impl<T> ResourceTable<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, value: T) -> Result<OwnedResourceHandle, ResourceTableError> {
        let handle = if let Some(slot_index) = self.free_slots.pop_first() {
            let slot = &mut self.slots[slot_index as usize];
            debug_assert!(slot.value.is_none());
            debug_assert!(slot.shared_borrows.is_empty());
            debug_assert!(slot.exclusive_borrow.is_none());
            slot.value = Some(value);
            OwnedResourceHandle {
                slot: slot_index,
                generation: slot.generation,
            }
        } else {
            let slot_index = u32::try_from(self.slots.len())
                .map_err(|_| ResourceTableError::CapacityExhausted)?;
            self.slots
                .push(ResourceSlot::occupied(FIRST_GENERATION, value));
            OwnedResourceHandle {
                slot: slot_index,
                generation: FIRST_GENERATION,
            }
        };
        self.active_resources += 1;
        Ok(handle)
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

        let borrow_id = self.allocate_borrow_id()?;
        self.resolve_owned_mut(owner)?
            .shared_borrows
            .insert(borrow_id);
        Ok(BorrowedResourceHandle {
            owner,
            borrow_id,
            kind: ResourceBorrowKind::Shared,
        })
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

        let borrow_id = self.allocate_borrow_id()?;
        self.resolve_owned_mut(owner)?.exclusive_borrow = Some(borrow_id);
        Ok(BorrowedResourceHandle {
            owner,
            borrow_id,
            kind: ResourceBorrowKind::Exclusive,
        })
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
        let slot = self
            .resolve_owned_mut(borrowed.owner)
            .map_err(|_| ResourceTableError::StaleBorrowedHandle(borrowed))?;
        let was_active = match borrowed.kind {
            ResourceBorrowKind::Shared => slot.shared_borrows.remove(&borrowed.borrow_id),
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
            self.free_slots.insert(owner.slot);
        }
        Ok(value)
    }

    pub const fn len(&self) -> usize {
        self.active_resources
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

    fn resolve_owned(
        &self,
        owner: OwnedResourceHandle,
    ) -> Result<&ResourceSlot<T>, ResourceTableError> {
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
        if !Self::borrow_is_active(slot, borrowed) {
            return Err(ResourceTableError::StaleBorrowedHandle(borrowed));
        }
        Ok(slot)
    }

    fn resolve_borrowed_mut(
        &mut self,
        borrowed: BorrowedResourceHandle,
    ) -> Result<&mut ResourceSlot<T>, ResourceTableError> {
        let slot = self
            .resolve_owned_mut(borrowed.owner)
            .map_err(|_| ResourceTableError::StaleBorrowedHandle(borrowed))?;
        if !Self::borrow_is_active(slot, borrowed) {
            return Err(ResourceTableError::StaleBorrowedHandle(borrowed));
        }
        Ok(slot)
    }

    fn borrow_is_active(slot: &ResourceSlot<T>, borrowed: BorrowedResourceHandle) -> bool {
        match borrowed.kind {
            ResourceBorrowKind::Shared => slot.shared_borrows.contains(&borrowed.borrow_id),
            ResourceBorrowKind::Exclusive => slot.exclusive_borrow == Some(borrowed.borrow_id),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceTableError {
    CapacityExhausted,
    BorrowIdExhausted,
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
            Self::CapacityExhausted => formatter.write_str("component resource table is full"),
            Self::BorrowIdExhausted => {
                formatter.write_str("component resource borrow identifiers are exhausted")
            }
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

#[cfg(test)]
mod tests {
    use super::{OwnedResourceHandle, ResourceBorrowKind, ResourceTable, ResourceTableError};

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
