//! Cross-mode host-value resource and boundary-failure conformance fixture.
//!
//! Each execution backend supplies the scalar result produced by the same
//! validated component function. The backend-neutral adapter then transfers
//! that value as an owned host resource, performs guest-trap cleanup, and
//! projects the resulting boundary failure into the schema-2 report.

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_bytecode::{LowerError, lower};
use lang_codegen_clif::{CompileError, CompiledFunction, InvokeError};
use lang_mir::{Function, ResourceTypeId, Signature, Terminator, Type, ValueId};
use lang_runtime::component::CapabilityGrants;
use lang_runtime::component::adapter::{
    AdapterError, AdapterLimits, CallOutcome, CanonicalOwnership, InstanceState, TransferArgument,
    preflight_call,
};
use lang_runtime::component::host_value::{HostResourcePublishError, HostResourceValues};
use lang_runtime::component::instantiation::{
    ComponentInstance, ComponentInstantiationError, instantiate_component,
};
use lang_runtime::component::negotiation::{
    ContentDigest, InterfaceFunctionType, InterfaceKey, InterfaceMember, InterfaceProvider,
    InterfaceValueType, PackageKey, ResourceTypeKey, SemanticVersion, VersionRequirement,
};
use lang_runtime::component::serialization::{
    BoundaryFailureSerializationError, ComponentContractRequiredMember,
    ComponentContractRequirement, ComponentContractValueType, VersionedComponentContract,
    VersionedComponentFailureReport,
};
use lang_vm::{VmError, VmValue, VmValueError, execute_typed};

use crate::component_conformance::{ScalarComponentError, run_prototype_scalar_component};

const RESOURCE_CAPABILITY: &str = "counter.consume";
pub const PROTOTYPE_COUNTER_RESOURCE_TYPE: ResourceTypeId = ResourceTypeId::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceFailureOutcome {
    pub host_value: i64,
    pub state: InstanceState,
    pub report: VersionedComponentFailureReport,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossModeResourceFailureOutcomes {
    pub vm: ResourceFailureOutcome,
    pub jit: ResourceFailureOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossModeTypedResourceOutcomes {
    pub vm: ResourceFailureOutcome,
    pub jit: ResourceFailureOutcome,
}

pub fn prototype_owned_resource_function() -> Function {
    Function::new(
        "component_transfer_owned_resource",
        Signature::new(
            vec![Type::Own(PROTOTYPE_COUNTER_RESOURCE_TYPE)],
            Type::Own(PROTOTYPE_COUNTER_RESOURCE_TYPE),
        ),
        Vec::new(),
        Terminator::Return(ValueId::new(0)),
    )
}

pub fn run_prototype_typed_resource_component()
-> Result<CrossModeTypedResourceOutcomes, ResourceFailureConformanceError> {
    let function = prototype_owned_resource_function();
    let program = lower(&function).map_err(ResourceFailureConformanceError::Lowering)?;
    let compiled = CompiledFunction::compile(&function)
        .map_err(ResourceFailureConformanceError::NativeCompilation)?;

    let vm = run_typed_resource_adapter(|index| {
        let argument = VmValue::own_resource(PROTOTYPE_COUNTER_RESOURCE_TYPE, index)
            .map_err(ResourceFailureConformanceError::VmValue)?;
        execute_typed(&program, &[argument])
            .map_err(ResourceFailureConformanceError::VmExecution)?
            .owned_resource_index()
            .ok_or(ResourceFailureConformanceError::MissingBackendResourceIndex)
    })?;
    let jit = run_typed_resource_adapter(|index| {
        compiled
            .invoke_own_resource_1(PROTOTYPE_COUNTER_RESOURCE_TYPE, index)
            .map_err(ResourceFailureConformanceError::NativeInvocation)
    })?;

    Ok(CrossModeTypedResourceOutcomes { vm, jit })
}

pub fn run_prototype_resource_failure_component()
-> Result<CrossModeResourceFailureOutcomes, ResourceFailureConformanceError> {
    let scalar = run_prototype_scalar_component(21)
        .map_err(ResourceFailureConformanceError::ScalarExecution)?;
    let vm = run_prototype_resource_failure_adapter(scalar.vm)?;
    let jit = run_prototype_resource_failure_adapter(scalar.jit)?;
    Ok(CrossModeResourceFailureOutcomes { vm, jit })
}

pub fn run_prototype_resource_failure_adapter(
    backend_value: i64,
) -> Result<ResourceFailureOutcome, ResourceFailureConformanceError> {
    let resource_type = counter_resource_type();
    let (mut caller, mut callee) = prototype_instances()?;
    let mut host_values = HostResourceValues::new();
    let source = host_values
        .publish_owned(caller.resources_mut(), resource_type.clone(), backend_value)
        .map_err(ResourceFailureConformanceError::Publication)?;
    let prepared = preflight_call(
        caller.resources_mut(),
        callee.resources_mut(),
        &[TransferArgument::new(
            source,
            resource_type,
            CanonicalOwnership::Own,
        )],
    )
    .map_err(ResourceFailureConformanceError::Adapter)?;

    let mut destroyed = None;
    let completion = prepared
        .commit()
        .finish(CallOutcome::GuestTrap, |identity| {
            let value = host_values.release(identity)?;
            destroyed = Some(value);
            Ok::<(), lang_runtime::component::host_value::HostResourceError>(())
        });
    let host_value = destroyed.ok_or(ResourceFailureConformanceError::MissingDestroyedValue)?;
    if !host_values.is_empty() {
        return Err(ResourceFailureConformanceError::LeakedHostValue);
    }
    let failure = completion
        .failure()
        .ok_or(ResourceFailureConformanceError::MissingBoundaryFailure)?;
    let report = VersionedComponentFailureReport::from_boundary_failure_redacted(failure)
        .map_err(ResourceFailureConformanceError::FailureSerialization)?;

    Ok(ResourceFailureOutcome {
        host_value,
        state: completion.state(),
        report,
    })
}

pub fn run_prototype_typed_resource_adapter(
    backend_index: u32,
) -> Result<ResourceFailureOutcome, ResourceFailureConformanceError> {
    run_typed_resource_adapter(|_| Ok(backend_index))
}

fn run_typed_resource_adapter<F>(
    backend: F,
) -> Result<ResourceFailureOutcome, ResourceFailureConformanceError>
where
    F: FnOnce(u32) -> Result<u32, ResourceFailureConformanceError>,
{
    let resource_type = counter_resource_type();
    let (mut caller, mut callee) = prototype_instances()?;
    let mut host_values = HostResourceValues::new();
    let source = host_values
        .publish_owned(caller.resources_mut(), resource_type.clone(), 42)
        .map_err(ResourceFailureConformanceError::Publication)?;
    let prepared = preflight_call(
        caller.resources_mut(),
        callee.resources_mut(),
        &[TransferArgument::new(
            source,
            resource_type,
            CanonicalOwnership::Own,
        )],
    )
    .map_err(ResourceFailureConformanceError::Adapter)?;

    let active = prepared.commit();
    let destination = active
        .arguments()
        .first()
        .expect("one preflight argument commits one destination")
        .destination();
    let backend_index = backend(destination.get())?;
    if backend_index != destination.get() {
        return Err(
            ResourceFailureConformanceError::BackendResourceIndexMismatch {
                expected: destination.get(),
                actual: backend_index,
            },
        );
    }

    let mut destroyed = None;
    let completion = active.finish(CallOutcome::GuestTrap, |identity| {
        let value = host_values.release(identity)?;
        destroyed = Some(value);
        Ok::<(), lang_runtime::component::host_value::HostResourceError>(())
    });
    let host_value = destroyed.ok_or(ResourceFailureConformanceError::MissingDestroyedValue)?;
    if !host_values.is_empty() {
        return Err(ResourceFailureConformanceError::LeakedHostValue);
    }
    let failure = completion
        .failure()
        .ok_or(ResourceFailureConformanceError::MissingBoundaryFailure)?;
    let report = VersionedComponentFailureReport::from_boundary_failure_redacted(failure)
        .map_err(ResourceFailureConformanceError::FailureSerialization)?;

    Ok(ResourceFailureOutcome {
        host_value,
        state: completion.state(),
        report,
    })
}

fn prototype_instances()
-> Result<(ComponentInstance, ComponentInstance), ResourceFailureConformanceError> {
    let contract = prototype_contract();
    let providers = [prototype_provider()];
    let mut grants = CapabilityGrants::new();
    grants.grant(RESOURCE_CAPABILITY);
    let caller = instantiate_component(
        contract.clone(),
        &providers,
        &grants,
        AdapterLimits::new(4, 8),
    )
    .map_err(ResourceFailureConformanceError::Instantiation)?;
    let callee = instantiate_component(contract, &providers, &grants, AdapterLimits::new(4, 8))
        .map_err(ResourceFailureConformanceError::Instantiation)?;
    Ok((caller, callee))
}

fn prototype_contract() -> VersionedComponentContract {
    let interface = counter_interface();
    let resource = counter_resource_type();
    VersionedComponentContract::new(
        PackageKey::new("example", "resource-consumer"),
        SemanticVersion::new(1, 0, 0),
        vec![
            ComponentContractRequirement::new(
                interface,
                VersionRequirement::Exact(SemanticVersion::new(1, 0, 0)),
            )
            .with_required_members(vec![ComponentContractRequiredMember::new(
                "consume",
                vec![ComponentContractValueType::Own(resource.clone())],
                vec![ComponentContractValueType::I64],
                RESOURCE_CAPABILITY,
            )]),
        ],
        Vec::new(),
        vec![resource],
    )
    .expect("the static resource conformance contract is valid")
}

fn prototype_provider() -> InterfaceProvider {
    InterfaceProvider::new(
        counter_interface(),
        SemanticVersion::new(1, 0, 0),
        ContentDigest::new(vec![7; 32]).expect("the fixture digest is non-empty"),
        vec![InterfaceMember::new(
            "consume",
            InterfaceFunctionType::new(
                vec![InterfaceValueType::Own(counter_resource_type())],
                vec![InterfaceValueType::I64],
            ),
            Some(RESOURCE_CAPABILITY.to_owned()),
        )],
    )
}

fn counter_interface() -> InterfaceKey {
    InterfaceKey::new(PackageKey::new("example", "counter"), "counters")
}

fn counter_resource_type() -> ResourceTypeKey {
    ResourceTypeKey::new(counter_interface(), "counter")
}

#[derive(Debug)]
pub enum ResourceFailureConformanceError {
    ScalarExecution(ScalarComponentError),
    Lowering(LowerError),
    VmValue(VmValueError),
    VmExecution(VmError),
    NativeCompilation(CompileError),
    NativeInvocation(InvokeError),
    Instantiation(ComponentInstantiationError),
    Publication(HostResourcePublishError),
    Adapter(AdapterError),
    MissingDestroyedValue,
    LeakedHostValue,
    MissingBoundaryFailure,
    MissingBackendResourceIndex,
    BackendResourceIndexMismatch { expected: u32, actual: u32 },
    FailureSerialization(BoundaryFailureSerializationError),
}

impl Display for ResourceFailureConformanceError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ScalarExecution(error) => write!(formatter, "scalar backend failed: {error}"),
            Self::Lowering(error) => write!(formatter, "resource lowering failed: {error}"),
            Self::VmValue(error) => write!(formatter, "VM resource value failed: {error}"),
            Self::VmExecution(error) => write!(formatter, "VM resource execution failed: {error}"),
            Self::NativeCompilation(error) => {
                write!(formatter, "native resource compilation failed: {error}")
            }
            Self::NativeInvocation(error) => {
                write!(formatter, "native resource invocation failed: {error}")
            }
            Self::Instantiation(error) => write!(formatter, "instantiation failed: {error}"),
            Self::Publication(error) => write!(formatter, "resource publication failed: {error}"),
            Self::Adapter(error) => write!(formatter, "resource transfer failed: {error}"),
            Self::MissingDestroyedValue => {
                formatter.write_str("resource cleanup did not return the host value")
            }
            Self::LeakedHostValue => formatter.write_str("resource cleanup left a live host value"),
            Self::MissingBoundaryFailure => {
                formatter.write_str("guest trap did not produce a boundary failure")
            }
            Self::MissingBackendResourceIndex => {
                formatter.write_str("backend result did not contain an owned resource index")
            }
            Self::BackendResourceIndexMismatch { expected, actual } => write!(
                formatter,
                "backend returned resource index {actual}, expected {expected}"
            ),
            Self::FailureSerialization(error) => {
                write!(formatter, "boundary failure projection failed: {error}")
            }
        }
    }
}

impl Error for ResourceFailureConformanceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ScalarExecution(error) => Some(error),
            Self::Lowering(error) => Some(error),
            Self::VmValue(error) => Some(error),
            Self::VmExecution(error) => Some(error),
            Self::NativeCompilation(error) => Some(error),
            Self::NativeInvocation(error) => Some(error),
            Self::Instantiation(error) => Some(error),
            Self::Publication(error) => Some(error),
            Self::Adapter(error) => Some(error),
            Self::FailureSerialization(error) => Some(error),
            Self::MissingDestroyedValue
            | Self::LeakedHostValue
            | Self::MissingBoundaryFailure
            | Self::MissingBackendResourceIndex
            | Self::BackendResourceIndexMismatch { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use lang_runtime::component::adapter::InstanceState;
    use lang_runtime::component::serialization::{
        VersionedComponentDiagnostic, VersionedComponentFailureCategory,
        VersionedComponentFailurePhase, VersionedComponentMessagePolicy,
    };

    use super::{
        run_prototype_resource_failure_adapter, run_prototype_resource_failure_component,
        run_prototype_typed_resource_component,
    };

    #[test]
    fn vm_and_jit_preserve_typed_ownership_through_cleanup() {
        let outcomes = run_prototype_typed_resource_component()
            .expect("VM and JIT should execute the typed resource fixture");

        assert_eq!(outcomes.vm, outcomes.jit);
        assert_eq!(outcomes.vm.host_value, 42);
        assert_eq!(outcomes.vm.state, InstanceState::Reusable);
        assert!(!outcomes.vm.report.poisoned());
    }

    #[test]
    fn vm_and_jit_match_for_host_value_cleanup_and_failure_projection() {
        let outcomes = run_prototype_resource_failure_component()
            .expect("VM and JIT should execute the resource fixture");

        assert_eq!(outcomes.vm, outcomes.jit);
        assert_eq!(outcomes.vm.host_value, 42);
        assert_eq!(outcomes.vm.state, InstanceState::Reusable);
        assert_eq!(
            outcomes.vm.report.message_policy(),
            VersionedComponentMessagePolicy::Redacted
        );
        assert_eq!(
            outcomes.vm.report.primary().category(),
            VersionedComponentFailureCategory::Trap
        );
        assert_eq!(
            outcomes.vm.report.primary().phase(),
            VersionedComponentFailurePhase::Call
        );
        assert_eq!(
            outcomes.vm.report.primary().diagnostic(),
            VersionedComponentDiagnostic::BoundaryTrap
        );
        assert!(outcomes.vm.report.suppressed().is_empty());
        assert!(!outcomes.vm.report.poisoned());
    }

    #[test]
    fn adapter_preserves_the_backend_value_through_trap_cleanup() {
        let outcome = run_prototype_resource_failure_adapter(-7)
            .expect("the adapter should clean up the supplied host value");

        assert_eq!(outcome.host_value, -7);
        assert_eq!(outcome.state, InstanceState::Reusable);
        assert!(!outcome.report.poisoned());
    }
}
