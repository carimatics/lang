//! Bridge between canonical component resource identities and host values.
//!
//! A single store is shared by the component instances that transfer a
//! resource. Canonical adapter tables retain only opaque identities; this
//! store resolves those identities to generation-checked runtime handles and
//! owns the corresponding host values.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use super::adapter::{
    AdapterError, BoundaryResourceIndex, CanonicalResourceTable, ResourceIdentity,
};
use super::negotiation::ResourceTypeKey;
use super::resource::{
    BorrowedResourceHandle, OwnedResourceHandle, ResourceTable, ResourceTableError,
};

const FIRST_RESOURCE_IDENTITY: u64 = 1;

#[derive(Debug)]
pub struct HostResourceValues<T> {
    values: ResourceTable<T>,
    owners: BTreeMap<ResourceIdentity, OwnedResourceHandle>,
    next_identity: Option<u64>,
}

impl<T> Default for HostResourceValues<T> {
    fn default() -> Self {
        Self {
            values: ResourceTable::new(),
            owners: BTreeMap::new(),
            next_identity: Some(FIRST_RESOURCE_IDENTITY),
        }
    }
}

impl<T> HostResourceValues<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub const fn len(&self) -> usize {
        self.values.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn contains(&self, identity: ResourceIdentity) -> bool {
        self.owners.contains_key(&identity)
    }

    pub fn insert(&mut self, value: T) -> Result<ResourceIdentity, HostResourceError> {
        let identity = self
            .next_identity
            .map(ResourceIdentity::new)
            .ok_or(HostResourceError::IdentityExhausted)?;
        let owner = self
            .values
            .insert(value)
            .map_err(HostResourceError::Table)?;

        self.next_identity = identity.get().checked_add(1);
        let replaced = self.owners.insert(identity, owner);
        debug_assert!(replaced.is_none(), "resource identities never repeat");
        Ok(identity)
    }

    pub fn publish_owned(
        &mut self,
        adapter: &mut CanonicalResourceTable,
        resource_type: ResourceTypeKey,
        value: T,
    ) -> Result<BoundaryResourceIndex, HostResourcePublishError> {
        let identity = self.insert(value).map_err(HostResourcePublishError::Host)?;
        match adapter.insert_owned(resource_type, identity) {
            Ok(index) => Ok(index),
            Err(adapter) => match self.release(identity) {
                Ok(_) => Err(HostResourcePublishError::Adapter(adapter)),
                Err(rollback) => Err(HostResourcePublishError::Rollback { adapter, rollback }),
            },
        }
    }

    pub fn borrow_shared(
        &mut self,
        identity: ResourceIdentity,
    ) -> Result<BorrowedResourceHandle, HostResourceError> {
        let owner = self.owner(identity)?;
        self.values
            .borrow_shared(owner)
            .map_err(HostResourceError::Table)
    }

    pub fn borrow_exclusive(
        &mut self,
        identity: ResourceIdentity,
    ) -> Result<BorrowedResourceHandle, HostResourceError> {
        let owner = self.owner(identity)?;
        self.values
            .borrow_exclusive(owner)
            .map_err(HostResourceError::Table)
    }

    pub fn get_borrowed(&self, borrowed: BorrowedResourceHandle) -> Result<&T, HostResourceError> {
        self.values
            .get_borrowed(borrowed)
            .map_err(HostResourceError::Table)
    }

    pub fn get_borrowed_mut(
        &mut self,
        borrowed: BorrowedResourceHandle,
    ) -> Result<&mut T, HostResourceError> {
        self.values
            .get_borrowed_mut(borrowed)
            .map_err(HostResourceError::Table)
    }

    pub fn end_borrow(
        &mut self,
        borrowed: BorrowedResourceHandle,
    ) -> Result<(), HostResourceError> {
        self.values
            .end_borrow(borrowed)
            .map_err(HostResourceError::Table)
    }

    pub fn release(&mut self, identity: ResourceIdentity) -> Result<T, HostResourceError> {
        let owner = self.owner(identity)?;
        let value = self
            .values
            .release(owner)
            .map_err(HostResourceError::Table)?;
        let removed = self.owners.remove(&identity);
        debug_assert_eq!(removed, Some(owner));
        Ok(value)
    }

    fn owner(&self, identity: ResourceIdentity) -> Result<OwnedResourceHandle, HostResourceError> {
        self.owners
            .get(&identity)
            .copied()
            .ok_or(HostResourceError::UnknownIdentity(identity))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostResourceError {
    IdentityExhausted,
    UnknownIdentity(ResourceIdentity),
    Table(ResourceTableError),
}

impl Display for HostResourceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IdentityExhausted => {
                formatter.write_str("host resource identities are exhausted")
            }
            Self::UnknownIdentity(identity) => {
                write!(
                    formatter,
                    "host resource identity {} is unknown",
                    identity.get()
                )
            }
            Self::Table(error) => error.fmt(formatter),
        }
    }
}

impl Error for HostResourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Table(error) => Some(error),
            Self::IdentityExhausted | Self::UnknownIdentity(_) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostResourcePublishError {
    Host(HostResourceError),
    Adapter(AdapterError),
    Rollback {
        adapter: AdapterError,
        rollback: HostResourceError,
    },
}

impl Display for HostResourcePublishError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(error) => error.fmt(formatter),
            Self::Adapter(error) => error.fmt(formatter),
            Self::Rollback { adapter, rollback } => write!(
                formatter,
                "canonical resource publication failed ({adapter}) and host-value rollback failed ({rollback})"
            ),
        }
    }
}

impl Error for HostResourcePublishError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Host(error) => Some(error),
            Self::Adapter(error) | Self::Rollback { adapter: error, .. } => Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{HostResourceError, HostResourcePublishError, HostResourceValues};
    use crate::component::adapter::{
        AdapterLimits, CallOutcome, CanonicalOwnership, CanonicalResourceTable, TransferArgument,
        preflight_call,
    };
    use crate::component::negotiation::{InterfaceKey, PackageKey, ResourceTypeKey};
    use crate::component::resource::ResourceTableError;

    fn resource_type(name: &str) -> ResourceTypeKey {
        ResourceTypeKey::new(
            InterfaceKey::new(PackageKey::new("example", "counter"), "values"),
            name,
        )
    }

    #[test]
    fn canonical_cleanup_releases_the_actual_host_value_once() {
        let counter = resource_type("counter");
        let mut values = HostResourceValues::new();
        let mut caller = CanonicalResourceTable::new(AdapterLimits::default());
        let source = values
            .publish_owned(&mut caller, counter.clone(), String::from("host counter"))
            .expect("host value should be published");
        let identity = caller
            .resource_identity(source)
            .expect("published entry should carry its host identity");
        let mut callee = CanonicalResourceTable::new(AdapterLimits::default());

        let prepared = preflight_call(
            &mut caller,
            &mut callee,
            &[TransferArgument::new(
                source,
                counter,
                CanonicalOwnership::Own,
            )],
        )
        .expect("owned transfer should preflight");
        let mut destroyed = Vec::new();
        let completion = prepared.commit().finish(CallOutcome::Returned, |resource| {
            values.release(resource).map(|value| destroyed.push(value))
        });

        assert!(completion.failure().is_none());
        assert_eq!(destroyed, ["host counter"]);
        assert!(values.is_empty());
        assert_eq!(
            values.release(identity),
            Err(HostResourceError::UnknownIdentity(identity))
        );
    }

    #[test]
    fn adapter_rejection_rolls_back_without_reusing_the_identity() {
        let mut values = HostResourceValues::new();
        let mut full_adapter = CanonicalResourceTable::new(AdapterLimits::new(0, 1));

        assert!(matches!(
            values.publish_owned(&mut full_adapter, resource_type("counter"), 41),
            Err(HostResourcePublishError::Adapter(_))
        ));
        assert!(values.is_empty());

        let identity = values.insert(42).expect("identity should remain available");
        assert_eq!(identity.get(), 2);
        assert_eq!(values.release(identity), Ok(42));
    }

    #[test]
    fn host_borrows_resolve_through_the_canonical_identity() {
        let mut values = HostResourceValues::new();
        let identity = values.insert(vec![1]).expect("value should fit");
        let borrowed = values
            .borrow_exclusive(identity)
            .expect("exclusive borrow should start");

        values
            .get_borrowed_mut(borrowed)
            .expect("exclusive borrow should resolve")
            .push(2);
        assert!(matches!(
            values.release(identity),
            Err(HostResourceError::Table(
                ResourceTableError::ResourceInUse { .. }
            ))
        ));
        values.end_borrow(borrowed).expect("borrow should end");
        assert_eq!(values.release(identity), Ok(vec![1, 2]));
    }
}
