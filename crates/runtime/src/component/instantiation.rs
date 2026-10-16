//! General component-instantiation prototype for schema-2 contracts.
//!
//! Instantiation negotiates every required interface before publishing an
//! instance. A successful instance owns immutable validated bindings and a
//! fresh canonical-resource adapter table; no partial state escapes on error.

use std::error::Error;
use std::fmt::{Display, Formatter};

use super::CapabilityGrants;
use super::adapter::{AdapterLimits, CanonicalResourceTable};
use super::negotiation::{
    ContentDigest, ExactInterfaceLock, InterfaceConsumer, InterfaceFunctionType, InterfaceKey,
    InterfaceMember, InterfaceProvider, InterfaceValueType, NegotiationError,
    ValidatedInterfaceBinding, negotiate_interface,
};
use super::serialization::{
    ComponentContractLock, ComponentContractRequiredMember, ComponentContractRequirement,
    ComponentContractValueType, VersionedComponentContract,
};

#[derive(Debug)]
pub struct ComponentInstance {
    contract: VersionedComponentContract,
    bindings: Vec<ValidatedInterfaceBinding>,
    resources: CanonicalResourceTable,
}

impl ComponentInstance {
    pub const fn contract(&self) -> &VersionedComponentContract {
        &self.contract
    }

    pub fn bindings(&self) -> &[ValidatedInterfaceBinding] {
        &self.bindings
    }

    pub fn binding(&self, key: &InterfaceKey) -> Option<&ValidatedInterfaceBinding> {
        self.bindings.iter().find(|binding| binding.key() == key)
    }

    pub const fn resources(&self) -> &CanonicalResourceTable {
        &self.resources
    }

    pub const fn resources_mut(&mut self) -> &mut CanonicalResourceTable {
        &mut self.resources
    }
}

pub fn instantiate_component(
    contract: VersionedComponentContract,
    providers: &[InterfaceProvider],
    grants: &CapabilityGrants,
    adapter_limits: AdapterLimits,
) -> Result<ComponentInstance, ComponentInstantiationError> {
    let mut bindings = Vec::new();
    bindings
        .try_reserve_exact(contract.requirements().len())
        .map_err(|_| ComponentInstantiationError::ReservationFailed)?;

    for requirement in contract.requirements() {
        let consumer = interface_consumer(requirement);
        let exact_lock = contract
            .locks()
            .iter()
            .find(|lock| lock.key() == requirement.key())
            .map(exact_interface_lock);
        let binding = negotiate_interface(&consumer, providers, grants, exact_lock.as_ref())
            .map_err(|error| ComponentInstantiationError::Negotiation {
                interface: Box::new(requirement.key().clone()),
                error: Box::new(error),
            })?;
        bindings.push(binding);
    }

    Ok(ComponentInstance {
        contract,
        bindings,
        resources: CanonicalResourceTable::new(adapter_limits),
    })
}

fn interface_consumer(requirement: &ComponentContractRequirement) -> InterfaceConsumer {
    InterfaceConsumer::new(
        requirement.key().clone(),
        requirement.requirement().clone(),
        requirement
            .required_members()
            .iter()
            .map(interface_member)
            .collect(),
    )
}

fn interface_member(member: &ComponentContractRequiredMember) -> InterfaceMember {
    InterfaceMember::new(
        member.name(),
        InterfaceFunctionType::new(
            member.parameters().iter().map(interface_value).collect(),
            member.results().iter().map(interface_value).collect(),
        ),
        Some(member.capability().to_owned()),
    )
}

fn interface_value(value: &ComponentContractValueType) -> InterfaceValueType {
    match value {
        ComponentContractValueType::I64 => InterfaceValueType::I64,
        ComponentContractValueType::Bool => InterfaceValueType::Bool,
        ComponentContractValueType::Own(resource) => InterfaceValueType::Own(resource.clone()),
        ComponentContractValueType::Borrow(resource) => {
            InterfaceValueType::Borrow(resource.clone())
        }
    }
}

fn exact_interface_lock(lock: &ComponentContractLock) -> ExactInterfaceLock {
    let digest = ContentDigest::new(lock.digest().as_bytes().to_vec())
        .expect("validated versioned digests are non-empty");
    ExactInterfaceLock::new(lock.key().clone(), lock.version().clone(), digest)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentInstantiationError {
    ReservationFailed,
    Negotiation {
        interface: Box<InterfaceKey>,
        error: Box<NegotiationError>,
    },
}

impl Display for ComponentInstantiationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReservationFailed => formatter.write_str("component binding reservation failed"),
            Self::Negotiation { interface, error } => {
                write!(
                    formatter,
                    "component interface {interface} cannot bind: {error}"
                )
            }
        }
    }
}

impl Error for ComponentInstantiationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ReservationFailed => None,
            Self::Negotiation { error, .. } => Some(error.as_ref()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ComponentInstantiationError, instantiate_component};
    use crate::component::CapabilityGrants;
    use crate::component::adapter::{
        AdapterErrorKind, AdapterLimits, InstanceState, ResourceIdentity,
    };
    use crate::component::negotiation::{
        ContentDigest, InterfaceFunctionType, InterfaceKey, InterfaceMember, InterfaceProvider,
        InterfaceValueType, NegotiationError, PackageKey, ResourceTypeKey, SemanticVersion,
        VersionRequirement,
    };
    use crate::component::serialization::{
        ComponentContractLock, ComponentContractRequiredMember, ComponentContractRequirement,
        ComponentContractValueType, DigestAlgorithm, VersionedComponentContract, VersionedDigest,
    };

    fn interface(name: &str) -> InterfaceKey {
        InterfaceKey::new(PackageKey::new("example", "services"), name)
    }

    fn resource(name: &str) -> ResourceTypeKey {
        ResourceTypeKey::new(interface("counters"), name)
    }

    fn required_member(name: &str, capability: &str) -> ComponentContractRequiredMember {
        ComponentContractRequiredMember::new(
            name,
            vec![ComponentContractValueType::I64],
            vec![ComponentContractValueType::I64],
            capability,
        )
    }

    fn provider_member(name: &str, capability: &str) -> InterfaceMember {
        InterfaceMember::new(
            name,
            InterfaceFunctionType::new(
                vec![InterfaceValueType::I64],
                vec![InterfaceValueType::I64],
            ),
            Some(capability.to_owned()),
        )
    }

    fn digest(byte: u8) -> ContentDigest {
        ContentDigest::new(vec![byte; 32]).expect("fixture digest is non-empty")
    }

    fn versioned_digest(byte: u8) -> VersionedDigest {
        VersionedDigest::new(DigestAlgorithm::Sha256, vec![byte; 32])
            .expect("fixture digest has the SHA-256 length")
    }

    fn provider(
        key: InterfaceKey,
        version: SemanticVersion,
        digest_byte: u8,
        member: InterfaceMember,
    ) -> InterfaceProvider {
        InterfaceProvider::new(key, version, digest(digest_byte), vec![member])
    }

    fn contract() -> VersionedComponentContract {
        let alpha = interface("alpha");
        let zeta = interface("zeta");
        VersionedComponentContract::new(
            PackageKey::new("example", "consumer"),
            SemanticVersion::new(1, 0, 0),
            vec![
                ComponentContractRequirement::new(
                    zeta.clone(),
                    VersionRequirement::Interval {
                        min_inclusive: SemanticVersion::new(1, 0, 0),
                        max_exclusive: SemanticVersion::new(3, 0, 0),
                        allow_prerelease: false,
                    },
                )
                .with_required_members(vec![required_member("read", "counter.read")]),
                ComponentContractRequirement::new(
                    alpha.clone(),
                    VersionRequirement::Exact(SemanticVersion::new(1, 2, 0)),
                )
                .with_required_members(vec![required_member("tick", "clock.tick")]),
            ],
            vec![ComponentContractLock::new(
                alpha,
                SemanticVersion::new(1, 2, 0),
                versioned_digest(7),
            )],
            vec![resource("counter")],
        )
        .expect("fixture contract should validate")
    }

    fn providers() -> Vec<InterfaceProvider> {
        vec![
            provider(
                interface("zeta"),
                SemanticVersion::new(1, 5, 0),
                3,
                provider_member("read", "counter.read"),
            ),
            provider(
                interface("alpha"),
                SemanticVersion::new(1, 2, 0),
                7,
                provider_member("tick", "clock.tick"),
            ),
            provider(
                interface("zeta"),
                SemanticVersion::new(2, 0, 0),
                4,
                provider_member("read", "counter.read"),
            ),
        ]
    }

    fn grants() -> CapabilityGrants {
        let mut grants = CapabilityGrants::new();
        assert!(grants.grant("clock.tick"));
        assert!(grants.grant("counter.read"));
        grants
    }

    #[test]
    fn instantiates_all_requirements_in_canonical_order() {
        let instance = instantiate_component(
            contract(),
            &providers(),
            &grants(),
            AdapterLimits::new(4, 8),
        )
        .expect("all requirements should negotiate");

        assert_eq!(instance.contract().package().package(), "consumer");
        assert_eq!(instance.bindings().len(), 2);
        assert_eq!(instance.bindings()[0].key(), &interface("alpha"));
        assert_eq!(
            instance.bindings()[0].version(),
            &SemanticVersion::new(1, 2, 0)
        );
        assert!(instance.bindings()[0].is_locked());
        assert_eq!(instance.bindings()[1].key(), &interface("zeta"));
        assert_eq!(
            instance.bindings()[1].version(),
            &SemanticVersion::new(2, 0, 0)
        );
        assert!(!instance.bindings()[1].is_locked());
        assert_eq!(instance.resources().state(), InstanceState::Reusable);
        assert!(instance.resources().is_empty());
    }

    #[test]
    fn identifies_the_requirement_that_prevents_instantiation() {
        let error = instantiate_component(
            contract(),
            &providers(),
            &CapabilityGrants::new(),
            AdapterLimits::default(),
        )
        .expect_err("missing capability must reject the whole instance");

        assert_eq!(
            error,
            ComponentInstantiationError::Negotiation {
                interface: Box::new(interface("alpha")),
                error: Box::new(NegotiationError::MissingCapability {
                    member: "tick".to_owned(),
                    capability: "clock.tick".to_owned(),
                }),
            }
        );
    }

    #[test]
    fn instances_own_independent_bounded_adapter_state() {
        let mut first = instantiate_component(
            contract(),
            &providers(),
            &grants(),
            AdapterLimits::new(1, 1),
        )
        .expect("first instance should bind");
        let second = instantiate_component(
            contract(),
            &providers(),
            &grants(),
            AdapterLimits::new(1, 1),
        )
        .expect("second instance should bind");

        first
            .resources_mut()
            .insert_owned(resource("counter"), ResourceIdentity::new(1))
            .expect("first resource should fit");
        let error = first
            .resources_mut()
            .insert_owned(resource("counter"), ResourceIdentity::new(2))
            .expect_err("first instance should enforce its own limit");

        assert!(matches!(
            error.kind(),
            AdapterErrorKind::LiveLimitExceeded { limit: 1 }
        ));
        assert_eq!(first.resources().len(), 1);
        assert!(second.resources().is_empty());
    }
}
