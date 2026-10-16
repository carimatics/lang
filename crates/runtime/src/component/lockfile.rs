//! Deterministic in-memory component lockfile and package-selection prototype.
//!
//! This module deliberately defines no on-disk syntax. It integrates exact
//! version/digest replay with the in-memory resolver while the production
//! package manager and stable lockfile representation remain later work.

use std::error::Error;
use std::fmt::{Display, Formatter};

use super::CapabilityGrants;
use super::adapter::AdapterLimits;
use super::instantiation::{ComponentInstance, exact_interface_lock, interface_consumer};
use super::negotiation::{
    ContentDigest, ExactInterfaceLock, InterfaceKey, InterfaceProvider, NegotiationError,
    PackageKey, SemanticVersion, ValidatedInterfaceBinding, negotiate_interface, validate_consumer,
};
use super::serialization::{
    ComponentContractLock, DigestAlgorithm, VersionedComponentContract, VersionedDigest,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentLockEntry {
    key: InterfaceKey,
    version: SemanticVersion,
    digest: VersionedDigest,
}

impl ComponentLockEntry {
    pub fn new(key: InterfaceKey, version: SemanticVersion, digest: VersionedDigest) -> Self {
        Self {
            key,
            version,
            digest,
        }
    }

    pub fn key(&self) -> &InterfaceKey {
        &self.key
    }

    pub fn version(&self) -> &SemanticVersion {
        &self.version
    }

    pub fn digest(&self) -> &VersionedDigest {
        &self.digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentLockfile {
    package: PackageKey,
    version: SemanticVersion,
    entries: Vec<ComponentLockEntry>,
}

impl ComponentLockfile {
    pub fn new(
        package: PackageKey,
        version: SemanticVersion,
        mut entries: Vec<ComponentLockEntry>,
    ) -> Result<Self, ComponentLockfileError> {
        entries.sort_by(|left, right| left.key.cmp(&right.key));
        for pair in entries.windows(2) {
            if pair[0].key == pair[1].key {
                return Err(ComponentLockfileError::DuplicateEntry(Box::new(
                    pair[0].key.clone(),
                )));
            }
        }
        Ok(Self {
            package,
            version,
            entries,
        })
    }

    pub fn package(&self) -> &PackageKey {
        &self.package
    }

    pub fn version(&self) -> &SemanticVersion {
        &self.version
    }

    pub fn entries(&self) -> &[ComponentLockEntry] {
        &self.entries
    }

    pub fn entry(&self, key: &InterfaceKey) -> Option<&ComponentLockEntry> {
        self.entries
            .binary_search_by(|entry| entry.key.cmp(key))
            .ok()
            .map(|index| &self.entries[index])
    }
}

#[derive(Debug)]
pub struct LockedComponentSelection {
    contract: VersionedComponentContract,
    lockfile: ComponentLockfile,
    bindings: Vec<ValidatedInterfaceBinding>,
}

impl LockedComponentSelection {
    pub const fn contract(&self) -> &VersionedComponentContract {
        &self.contract
    }

    pub const fn lockfile(&self) -> &ComponentLockfile {
        &self.lockfile
    }

    pub fn bindings(&self) -> &[ValidatedInterfaceBinding] {
        &self.bindings
    }

    pub fn into_instance(self, adapter_limits: AdapterLimits) -> ComponentInstance {
        ComponentInstance::from_validated_parts(self.contract, self.bindings, adapter_limits)
    }
}

pub fn select_component_packages(
    contract: VersionedComponentContract,
    providers: &[InterfaceProvider],
    grants: &CapabilityGrants,
    lockfile: Option<&ComponentLockfile>,
) -> Result<LockedComponentSelection, ComponentPackageSelectionError> {
    for requirement in contract.requirements() {
        validate_consumer(&interface_consumer(requirement), grants).map_err(|error| {
            ComponentPackageSelectionError::Negotiation {
                interface: Box::new(requirement.key().clone()),
                error: Box::new(error),
            }
        })?;
    }
    if let Some(lockfile) = lockfile {
        validate_lockfile_identity(&contract, lockfile)?;
        validate_lockfile_coverage(&contract, lockfile)?;
    }

    let mut bindings = Vec::new();
    bindings
        .try_reserve_exact(contract.requirements().len())
        .map_err(|_| ComponentPackageSelectionError::ReservationFailed)?;

    for requirement in contract.requirements() {
        let contract_lock = contract
            .locks()
            .iter()
            .find(|candidate| candidate.key() == requirement.key());
        let lockfile_entry = lockfile.and_then(|lockfile| lockfile.entry(requirement.key()));
        if let (Some(contract_lock), Some(lockfile_entry)) = (contract_lock, lockfile_entry) {
            validate_matching_locks(contract_lock, lockfile_entry)?;
        }

        let lock = if let Some(entry) = lockfile_entry {
            Some(exact_lock_from_entry(entry))
        } else {
            contract_lock.map(exact_interface_lock)
        };
        let consumer = interface_consumer(requirement);
        let binding =
            negotiate_interface(&consumer, providers, grants, lock.as_ref()).map_err(|error| {
                ComponentPackageSelectionError::Negotiation {
                    interface: Box::new(requirement.key().clone()),
                    error: Box::new(error),
                }
            })?;
        bindings.push(binding);
    }

    let generated_lockfile = lockfile_from_bindings(&contract, &bindings)?;
    Ok(LockedComponentSelection {
        contract,
        lockfile: generated_lockfile,
        bindings,
    })
}

fn validate_lockfile_identity(
    contract: &VersionedComponentContract,
    lockfile: &ComponentLockfile,
) -> Result<(), ComponentPackageSelectionError> {
    if contract.package() != lockfile.package() || contract.version() != lockfile.version() {
        return Err(ComponentPackageSelectionError::ContractIdentityMismatch {
            expected_package: Box::new(contract.package().clone()),
            expected_version: Box::new(contract.version().clone()),
            actual_package: Box::new(lockfile.package().clone()),
            actual_version: Box::new(lockfile.version().clone()),
        });
    }
    Ok(())
}

fn validate_lockfile_coverage(
    contract: &VersionedComponentContract,
    lockfile: &ComponentLockfile,
) -> Result<(), ComponentPackageSelectionError> {
    for requirement in contract.requirements() {
        if lockfile.entry(requirement.key()).is_none() {
            return Err(ComponentPackageSelectionError::MissingEntry(Box::new(
                requirement.key().clone(),
            )));
        }
    }
    for entry in lockfile.entries() {
        if contract
            .requirements()
            .binary_search_by(|requirement| requirement.key().cmp(entry.key()))
            .is_err()
        {
            return Err(ComponentPackageSelectionError::OrphanEntry(Box::new(
                entry.key().clone(),
            )));
        }
    }
    Ok(())
}

fn validate_matching_locks(
    contract: &ComponentContractLock,
    lockfile: &ComponentLockEntry,
) -> Result<(), ComponentPackageSelectionError> {
    if contract.version() != lockfile.version() || contract.digest() != lockfile.digest() {
        return Err(ComponentPackageSelectionError::ContractLockMismatch {
            interface: Box::new(contract.key().clone()),
            contract_version: Box::new(contract.version().clone()),
            contract_digest: Box::new(contract.digest().clone()),
            lockfile_version: Box::new(lockfile.version().clone()),
            lockfile_digest: Box::new(lockfile.digest().clone()),
        });
    }
    Ok(())
}

fn exact_lock_from_entry(entry: &ComponentLockEntry) -> ExactInterfaceLock {
    ExactInterfaceLock::new(
        entry.key().clone(),
        entry.version().clone(),
        ContentDigest::new(entry.digest().as_bytes().to_vec())
            .expect("validated versioned digests are non-empty"),
    )
}

fn lockfile_from_bindings(
    contract: &VersionedComponentContract,
    bindings: &[ValidatedInterfaceBinding],
) -> Result<ComponentLockfile, ComponentPackageSelectionError> {
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(bindings.len())
        .map_err(|_| ComponentPackageSelectionError::ReservationFailed)?;
    for binding in bindings {
        let digest = VersionedDigest::new(DigestAlgorithm::Sha256, binding.digest().as_bytes())
            .map_err(|error| ComponentPackageSelectionError::UnsupportedDigest {
                interface: Box::new(binding.key().clone()),
                actual: match error {
                    super::serialization::VersionedDigestError::LengthMismatch {
                        actual, ..
                    } => actual,
                },
            })?;
        entries.push(ComponentLockEntry::new(
            binding.key().clone(),
            binding.version().clone(),
            digest,
        ));
    }
    ComponentLockfile::new(
        contract.package().clone(),
        contract.version().clone(),
        entries,
    )
    .map_err(ComponentPackageSelectionError::Lockfile)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentLockfileError {
    DuplicateEntry(Box<InterfaceKey>),
}

impl Display for ComponentLockfileError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateEntry(interface) => {
                write!(
                    formatter,
                    "component lockfile repeats interface {interface}"
                )
            }
        }
    }
}

impl Error for ComponentLockfileError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentPackageSelectionError {
    ReservationFailed,
    Lockfile(ComponentLockfileError),
    ContractIdentityMismatch {
        expected_package: Box<PackageKey>,
        expected_version: Box<SemanticVersion>,
        actual_package: Box<PackageKey>,
        actual_version: Box<SemanticVersion>,
    },
    MissingEntry(Box<InterfaceKey>),
    OrphanEntry(Box<InterfaceKey>),
    ContractLockMismatch {
        interface: Box<InterfaceKey>,
        contract_version: Box<SemanticVersion>,
        contract_digest: Box<VersionedDigest>,
        lockfile_version: Box<SemanticVersion>,
        lockfile_digest: Box<VersionedDigest>,
    },
    UnsupportedDigest {
        interface: Box<InterfaceKey>,
        actual: usize,
    },
    Negotiation {
        interface: Box<InterfaceKey>,
        error: Box<NegotiationError>,
    },
}

impl Display for ComponentPackageSelectionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReservationFailed => {
                formatter.write_str("component package selection reservation failed")
            }
            Self::Lockfile(error) => error.fmt(formatter),
            Self::ContractIdentityMismatch {
                expected_package,
                expected_version,
                actual_package,
                actual_version,
            } => write!(
                formatter,
                "component lockfile identifies {actual_package}@{actual_version}; expected {expected_package}@{expected_version}"
            ),
            Self::MissingEntry(interface) => {
                write!(
                    formatter,
                    "component lockfile is missing interface {interface}"
                )
            }
            Self::OrphanEntry(interface) => {
                write!(
                    formatter,
                    "component lockfile has orphan interface {interface}"
                )
            }
            Self::ContractLockMismatch { interface, .. } => write!(
                formatter,
                "component lockfile conflicts with the contract lock for {interface}"
            ),
            Self::UnsupportedDigest { interface, actual } => write!(
                formatter,
                "selected provider for {interface} has a {actual}-byte digest; schema-2 locks require SHA-256"
            ),
            Self::Negotiation { interface, error } => {
                write!(
                    formatter,
                    "component interface {interface} cannot select: {error}"
                )
            }
        }
    }
}

impl Error for ComponentPackageSelectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Lockfile(error) => Some(error),
            Self::Negotiation { error, .. } => Some(error.as_ref()),
            Self::ReservationFailed
            | Self::ContractIdentityMismatch { .. }
            | Self::MissingEntry(_)
            | Self::OrphanEntry(_)
            | Self::ContractLockMismatch { .. }
            | Self::UnsupportedDigest { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ComponentLockEntry, ComponentLockfile, ComponentPackageSelectionError,
        select_component_packages,
    };
    use crate::component::CapabilityGrants;
    use crate::component::adapter::{AdapterLimits, InstanceState};
    use crate::component::negotiation::{
        ContentDigest, InterfaceFunctionType, InterfaceKey, InterfaceMember, InterfaceProvider,
        InterfaceValueType, NegotiationError, PackageKey, SemanticVersion, VersionRequirement,
    };
    use crate::component::serialization::{
        ComponentContractLock, ComponentContractRequiredMember, ComponentContractRequirement,
        ComponentContractValueType, DigestAlgorithm, VersionedComponentContract, VersionedDigest,
    };

    fn interface(name: &str) -> InterfaceKey {
        InterfaceKey::new(PackageKey::new("example", "services"), name)
    }

    fn digest(byte: u8) -> ContentDigest {
        ContentDigest::new(vec![byte; 32]).expect("fixture digest is non-empty")
    }

    fn versioned_digest(byte: u8) -> VersionedDigest {
        VersionedDigest::new(DigestAlgorithm::Sha256, vec![byte; 32])
            .expect("fixture digest has the SHA-256 length")
    }

    fn member(name: &str, capability: &str) -> InterfaceMember {
        InterfaceMember::new(
            name,
            InterfaceFunctionType::new(
                vec![InterfaceValueType::I64],
                vec![InterfaceValueType::I64],
            ),
            Some(capability.to_owned()),
        )
    }

    fn required(name: &str, capability: &str) -> ComponentContractRequiredMember {
        ComponentContractRequiredMember::new(
            name,
            vec![ComponentContractValueType::I64],
            vec![ComponentContractValueType::I64],
            capability,
        )
    }

    fn provider(
        key: InterfaceKey,
        version: SemanticVersion,
        digest_byte: u8,
        member: InterfaceMember,
    ) -> InterfaceProvider {
        InterfaceProvider::new(key, version, digest(digest_byte), vec![member])
    }

    #[test]
    fn package_selection_checks_all_capabilities_before_inventory() {
        let mut grants = CapabilityGrants::new();
        grants.grant("clock.tick");
        let error = select_component_packages(contract(false), &[], &grants, None).unwrap_err();
        assert!(
            matches!(error, ComponentPackageSelectionError::Negotiation { interface: key, error } if *key == interface("zeta") && matches!(*error, NegotiationError::MissingCapability { .. }))
        );
    }

    fn contract(embedded_lock: bool) -> VersionedComponentContract {
        let alpha = interface("alpha");
        let zeta = interface("zeta");
        let locks = if embedded_lock {
            vec![ComponentContractLock::new(
                alpha.clone(),
                SemanticVersion::new(1, 2, 0),
                versioned_digest(7),
            )]
        } else {
            Vec::new()
        };
        VersionedComponentContract::new(
            PackageKey::new("example", "consumer"),
            SemanticVersion::new(1, 0, 0),
            vec![
                ComponentContractRequirement::new(
                    zeta,
                    VersionRequirement::Interval {
                        min_inclusive: SemanticVersion::new(1, 0, 0),
                        max_exclusive: SemanticVersion::new(3, 0, 0),
                        allow_prerelease: false,
                    },
                )
                .with_required_members(vec![required("read", "counter.read")]),
                ComponentContractRequirement::new(
                    alpha,
                    VersionRequirement::Exact(SemanticVersion::new(1, 2, 0)),
                )
                .with_required_members(vec![required("tick", "clock.tick")]),
            ],
            locks,
            Vec::new(),
        )
        .expect("fixture contract should validate")
    }

    fn providers() -> Vec<InterfaceProvider> {
        vec![
            provider(
                interface("zeta"),
                SemanticVersion::new(1, 5, 0),
                3,
                member("read", "counter.read"),
            ),
            provider(
                interface("alpha"),
                SemanticVersion::new(1, 2, 0),
                7,
                member("tick", "clock.tick"),
            ),
            provider(
                interface("zeta"),
                SemanticVersion::new(2, 0, 0),
                4,
                member("read", "counter.read"),
            ),
        ]
    }

    fn grants() -> CapabilityGrants {
        let mut grants = CapabilityGrants::new();
        grants.grant("clock.tick");
        grants.grant("counter.read");
        grants
    }

    #[test]
    fn unlocked_selection_generates_a_canonical_complete_lockfile() {
        let forward = select_component_packages(contract(false), &providers(), &grants(), None)
            .expect("selection should succeed");
        let mut reversed_providers = providers();
        reversed_providers.reverse();
        let reversed =
            select_component_packages(contract(false), &reversed_providers, &grants(), None)
                .expect("registration order must not affect selection");

        assert_eq!(forward.lockfile(), reversed.lockfile());
        assert_eq!(forward.lockfile().entries().len(), 2);
        assert_eq!(forward.lockfile().entries()[0].key(), &interface("alpha"));
        assert_eq!(forward.lockfile().entries()[1].key(), &interface("zeta"));
        assert_eq!(
            forward.lockfile().entries()[1].version(),
            &SemanticVersion::new(2, 0, 0)
        );
    }

    #[test]
    fn lockfile_replay_is_exact_and_never_falls_back() {
        let selected = select_component_packages(contract(false), &providers(), &grants(), None)
            .expect("initial selection should succeed");
        let lockfile = selected.lockfile().clone();
        let replay =
            select_component_packages(contract(false), &providers(), &grants(), Some(&lockfile))
                .expect("locked providers should replay");
        assert_eq!(replay.lockfile(), &lockfile);
        assert!(replay.bindings().iter().all(|binding| binding.is_locked()));

        let providers_without_locked_zeta = providers()
            .into_iter()
            .filter(|provider| {
                provider.key() != &interface("zeta")
                    || provider.version() != &SemanticVersion::new(2, 0, 0)
            })
            .collect::<Vec<_>>();
        let error = select_component_packages(
            contract(false),
            &providers_without_locked_zeta,
            &grants(),
            Some(&lockfile),
        )
        .expect_err("an older compatible provider must not replace the lock");
        assert!(matches!(
            error,
            ComponentPackageSelectionError::Negotiation {
                error,
                ..
            } if matches!(*error, NegotiationError::LockedProviderUnavailable { .. })
        ));
    }

    #[test]
    fn rejects_incomplete_or_conflicting_lockfiles_deterministically() {
        let incomplete = ComponentLockfile::new(
            PackageKey::new("example", "consumer"),
            SemanticVersion::new(1, 0, 0),
            vec![ComponentLockEntry::new(
                interface("zeta"),
                SemanticVersion::new(2, 0, 0),
                versioned_digest(4),
            )],
        )
        .expect("fixture lockfile is structurally valid");
        assert_eq!(
            select_component_packages(contract(false), &providers(), &grants(), Some(&incomplete),)
                .expect_err("missing alpha lock should fail"),
            ComponentPackageSelectionError::MissingEntry(Box::new(interface("alpha")))
        );

        let conflicting = ComponentLockfile::new(
            PackageKey::new("example", "consumer"),
            SemanticVersion::new(1, 0, 0),
            vec![
                ComponentLockEntry::new(
                    interface("alpha"),
                    SemanticVersion::new(1, 2, 0),
                    versioned_digest(9),
                ),
                ComponentLockEntry::new(
                    interface("zeta"),
                    SemanticVersion::new(2, 0, 0),
                    versioned_digest(4),
                ),
            ],
        )
        .expect("fixture lockfile is structurally valid");
        assert!(matches!(
            select_component_packages(contract(true), &providers(), &grants(), Some(&conflicting),),
            Err(ComponentPackageSelectionError::ContractLockMismatch { .. })
        ));
    }

    #[test]
    fn selection_moves_directly_into_fresh_instance_state() {
        let selection = select_component_packages(contract(true), &providers(), &grants(), None)
            .expect("embedded and unlocked requirements should select");
        assert_eq!(selection.bindings().len(), 2);
        assert!(selection.bindings()[0].is_locked());
        assert!(!selection.bindings()[1].is_locked());

        let instance = selection.into_instance(AdapterLimits::new(2, 4));
        assert_eq!(instance.bindings().len(), 2);
        assert_eq!(instance.resources().state(), InstanceState::Reusable);
        assert!(instance.resources().is_empty());
    }
}
