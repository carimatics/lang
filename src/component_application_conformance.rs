//! Representative multi-component application conformance fixture.
//!
//! The fixture composes two independently selected provider interfaces, exact
//! lock replay, instance publication, scalar and owned-resource execution, and
//! guest-trap cleanup into one deterministic, host-orchestrated prototype
//! application run. Selected bindings gate the calls, but production generated
//! dispatch stubs remain downstream work.

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_runtime::component::CapabilityGrants;
use lang_runtime::component::adapter::AdapterLimits;
use lang_runtime::component::lockfile::{
    ComponentLockfile, ComponentPackageSelectionError, select_component_packages,
};
use lang_runtime::component::negotiation::{
    ContentDigest, InterfaceFunctionType, InterfaceKey, InterfaceMember, InterfaceProvider,
    InterfaceValueType, PackageKey, ResourceTypeKey, SemanticVersion, VersionRequirement,
};
use lang_runtime::component::serialization::{
    ComponentContractRequiredMember, ComponentContractRequirement, ComponentContractValueType,
    VersionedComponentContract,
};

use crate::component_conformance::{
    ScalarComponentError, ScalarComponentOutcomes, run_prototype_scalar_component,
};
use crate::component_resource_conformance::{
    CrossModeTypedResourceOutcomes, ResourceFailureConformanceError, ResourceFailureOutcome,
    run_prototype_resource_failure_adapter, run_prototype_typed_resource_component,
};

const TRANSFORM_CAPABILITY: &str = "application.transform";
const COUNTER_CAPABILITY: &str = "application.counter";
const REPRESENTATIVE_INPUT: i64 = 21;
const REPRESENTATIVE_OUTPUT: i64 = 42;
const EXPECTED_BINDINGS: usize = 2;

#[derive(Clone, Debug)]
pub struct RepresentativeComponentApplication {
    contract: VersionedComponentContract,
    providers: Vec<InterfaceProvider>,
    grants: CapabilityGrants,
    lockfile: ComponentLockfile,
}

impl RepresentativeComponentApplication {
    pub fn new() -> Result<Self, RepresentativeApplicationError> {
        let transform_interface = InterfaceKey::new(
            PackageKey::new("example", "transform-provider"),
            "transform",
        );
        let counter_interface =
            InterfaceKey::new(PackageKey::new("example", "counter-provider"), "counter");
        let counter_resource = ResourceTypeKey::new(counter_interface.clone(), "counter");

        let transform_member = InterfaceMember::new(
            "double",
            InterfaceFunctionType::new(
                vec![InterfaceValueType::I64],
                vec![InterfaceValueType::I64],
            ),
            Some(TRANSFORM_CAPABILITY.to_owned()),
        );
        let counter_member = InterfaceMember::new(
            "consume",
            InterfaceFunctionType::new(
                vec![InterfaceValueType::Own(counter_resource.clone())],
                vec![InterfaceValueType::I64],
            ),
            Some(COUNTER_CAPABILITY.to_owned()),
        );
        let providers = vec![
            provider(
                transform_interface.clone(),
                SemanticVersion::new(1, 0, 0),
                1,
                transform_member.clone(),
            ),
            provider(
                transform_interface.clone(),
                SemanticVersion::new(1, 1, 0),
                2,
                transform_member,
            ),
            provider(
                counter_interface.clone(),
                SemanticVersion::new(1, 0, 0),
                3,
                counter_member,
            ),
        ];
        let contract = VersionedComponentContract::new(
            PackageKey::new("example", "representative-application"),
            SemanticVersion::new(1, 0, 0),
            vec![
                ComponentContractRequirement::new(
                    transform_interface,
                    VersionRequirement::Interval {
                        min_inclusive: SemanticVersion::new(1, 0, 0),
                        max_exclusive: SemanticVersion::new(2, 0, 0),
                        allow_prerelease: false,
                    },
                )
                .with_required_members(vec![ComponentContractRequiredMember::new(
                    "double",
                    vec![ComponentContractValueType::I64],
                    vec![ComponentContractValueType::I64],
                    TRANSFORM_CAPABILITY,
                )]),
                ComponentContractRequirement::new(
                    counter_interface,
                    VersionRequirement::Exact(SemanticVersion::new(1, 0, 0)),
                )
                .with_required_members(vec![ComponentContractRequiredMember::new(
                    "consume",
                    vec![ComponentContractValueType::Own(counter_resource.clone())],
                    vec![ComponentContractValueType::I64],
                    COUNTER_CAPABILITY,
                )]),
            ],
            Vec::new(),
            vec![counter_resource],
        )
        .expect("the static representative application contract is valid");
        let mut grants = CapabilityGrants::new();
        grants.grant(TRANSFORM_CAPABILITY);
        grants.grant(COUNTER_CAPABILITY);
        let lockfile = select_component_packages(contract.clone(), &providers, &grants, None)
            .map_err(RepresentativeApplicationError::PackageSelection)?
            .lockfile()
            .clone();

        Ok(Self {
            contract,
            providers,
            grants,
            lockfile,
        })
    }

    pub fn run(&self) -> Result<RepresentativeApplicationOutcome, RepresentativeApplicationError> {
        let unlocked =
            select_component_packages(self.contract.clone(), &self.providers, &self.grants, None)
                .map_err(RepresentativeApplicationError::PackageSelection)?;
        if unlocked.lockfile() != &self.lockfile {
            return Err(RepresentativeApplicationError::GeneratedLockChanged);
        }
        let lock_entry_count = unlocked.lockfile().entries().len();
        if lock_entry_count != EXPECTED_BINDINGS {
            return Err(RepresentativeApplicationError::UnexpectedLockEntryCount {
                expected: EXPECTED_BINDINGS,
                actual: lock_entry_count,
            });
        }

        let instance = select_component_packages(
            self.contract.clone(),
            &self.providers,
            &self.grants,
            Some(unlocked.lockfile()),
        )
        .map_err(RepresentativeApplicationError::PackageSelection)?
        .into_instance(AdapterLimits::default());
        let binding_count = instance.bindings().len();
        if binding_count != EXPECTED_BINDINGS {
            return Err(RepresentativeApplicationError::UnexpectedBindingCount {
                expected: EXPECTED_BINDINGS,
                actual: binding_count,
            });
        }

        let scalar = run_prototype_scalar_component(REPRESENTATIVE_INPUT)
            .map_err(RepresentativeApplicationError::ScalarExecution)?;
        if scalar.vm != scalar.jit {
            return Err(RepresentativeApplicationError::ScalarBackendMismatch {
                vm: scalar.vm,
                jit: scalar.jit,
            });
        }
        let typed_resource = run_prototype_typed_resource_component()
            .map_err(RepresentativeApplicationError::ResourceExecution)?;
        if typed_resource.vm != typed_resource.jit {
            return Err(RepresentativeApplicationError::ResourceBackendMismatch);
        }
        let failure = run_prototype_resource_failure_adapter(scalar.vm)
            .map_err(RepresentativeApplicationError::ResourceExecution)?;
        if scalar.vm != REPRESENTATIVE_OUTPUT
            || typed_resource.vm.host_value != scalar.vm
            || failure.host_value != scalar.vm
        {
            return Err(RepresentativeApplicationError::PipelineValueMismatch {
                scalar: scalar.vm,
                resource: typed_resource.vm.host_value,
                cleanup: failure.host_value,
            });
        }

        Ok(RepresentativeApplicationOutcome {
            binding_count,
            lock_entry_count,
            scalar,
            typed_resource,
            failure,
        })
    }

    pub const fn lockfile(&self) -> &ComponentLockfile {
        &self.lockfile
    }
}

fn provider(
    key: InterfaceKey,
    version: SemanticVersion,
    digest_byte: u8,
    member: InterfaceMember,
) -> InterfaceProvider {
    InterfaceProvider::new(
        key,
        version,
        ContentDigest::new(vec![digest_byte; 32]).expect("the static provider digest is non-empty"),
        vec![member],
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepresentativeApplicationOutcome {
    pub binding_count: usize,
    pub lock_entry_count: usize,
    pub scalar: ScalarComponentOutcomes,
    pub typed_resource: CrossModeTypedResourceOutcomes,
    pub failure: ResourceFailureOutcome,
}

#[derive(Debug)]
pub enum RepresentativeApplicationError {
    PackageSelection(ComponentPackageSelectionError),
    ScalarExecution(ScalarComponentError),
    ResourceExecution(ResourceFailureConformanceError),
    GeneratedLockChanged,
    UnexpectedLockEntryCount {
        expected: usize,
        actual: usize,
    },
    UnexpectedBindingCount {
        expected: usize,
        actual: usize,
    },
    ScalarBackendMismatch {
        vm: i64,
        jit: i64,
    },
    ResourceBackendMismatch,
    PipelineValueMismatch {
        scalar: i64,
        resource: i64,
        cleanup: i64,
    },
}

impl Display for RepresentativeApplicationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PackageSelection(error) => write!(formatter, "package selection failed: {error}"),
            Self::ScalarExecution(error) => write!(formatter, "scalar execution failed: {error}"),
            Self::ResourceExecution(error) => {
                write!(formatter, "resource execution failed: {error}")
            }
            Self::GeneratedLockChanged => {
                formatter.write_str("unlocked package selection changed the generated lock")
            }
            Self::UnexpectedLockEntryCount { expected, actual } => write!(
                formatter,
                "representative lock has {actual} entries, expected {expected}"
            ),
            Self::UnexpectedBindingCount { expected, actual } => write!(
                formatter,
                "representative instance has {actual} bindings, expected {expected}"
            ),
            Self::ScalarBackendMismatch { vm, jit } => {
                write!(formatter, "scalar VM/JIT mismatch: VM {vm}, JIT {jit}")
            }
            Self::ResourceBackendMismatch => {
                formatter.write_str("typed-resource VM/JIT outcomes differ")
            }
            Self::PipelineValueMismatch {
                scalar,
                resource,
                cleanup,
            } => write!(
                formatter,
                "application pipeline values differ: scalar {scalar}, resource {resource}, cleanup {cleanup}"
            ),
        }
    }
}

impl Error for RepresentativeApplicationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PackageSelection(error) => Some(error),
            Self::ScalarExecution(error) => Some(error),
            Self::ResourceExecution(error) => Some(error),
            Self::GeneratedLockChanged
            | Self::UnexpectedLockEntryCount { .. }
            | Self::UnexpectedBindingCount { .. }
            | Self::ScalarBackendMismatch { .. }
            | Self::ResourceBackendMismatch
            | Self::PipelineValueMismatch { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use lang_runtime::component::adapter::InstanceState;

    use super::RepresentativeComponentApplication;

    #[test]
    fn composes_selection_lock_replay_execution_and_cleanup() {
        let application =
            RepresentativeComponentApplication::new().expect("builds the application fixture");
        let outcome = application.run().expect("runs the application fixture");

        assert_eq!(application.lockfile().entries().len(), 2);
        assert_eq!(outcome.binding_count, 2);
        assert_eq!(outcome.lock_entry_count, 2);
        assert_eq!(outcome.scalar.vm, 42);
        assert_eq!(outcome.scalar.jit, 42);
        assert_eq!(outcome.typed_resource.vm.host_value, 42);
        assert_eq!(outcome.typed_resource.jit.host_value, 42);
        assert_eq!(outcome.failure.host_value, 42);
        assert_eq!(outcome.failure.state, InstanceState::Reusable);
        assert!(!outcome.failure.report.poisoned());
    }
}
