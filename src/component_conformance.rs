//! Executable scalar component-contract prototype shared by VM, JIT, and AOT tests.

use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_bytecode::{LowerError, lower};
use lang_codegen_clif::{CompileError, CompiledFunction, InvokeError};
use lang_mir::{Function, HostFunctionId, Instruction, Signature, Terminator, Type, ValueId};
use lang_runtime::component::{
    COMPONENT_METADATA_SCHEMA_VERSION, CapabilityGrants, ComponentBindingError, ComponentExport,
    ComponentFunctionType, ComponentImport, ComponentImportBindings, ComponentMetadata,
    ComponentValidationError, ComponentVersion, ValidatedComponentMetadata,
    validate_component_metadata,
};
use lang_runtime::{HostFunctionRegistry, RegistrationError};
use lang_vm::{VmError, execute_with_hosts};

pub const PROTOTYPE_SCALAR_IMPORT: &str = "transform";
pub const PROTOTYPE_SCALAR_EXPORT: &str = "run";
pub const PROTOTYPE_SCALAR_CAPABILITY: &str = "example.transform";
pub const PROTOTYPE_SCALAR_HOST: HostFunctionId = HostFunctionId::new(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScalarComponentOutcomes {
    pub vm: i64,
    pub jit: i64,
}

#[derive(Debug)]
pub enum ScalarComponentError {
    HostRegistration(RegistrationError),
    ImportBinding(ComponentBindingError),
    ContractValidation(ComponentValidationError),
    MissingImport(String),
    MissingExport(String),
    UnsupportedImportType {
        import: String,
        function_type: ComponentFunctionType,
    },
    UnsupportedExportType {
        export: String,
        function_type: ComponentFunctionType,
    },
    UnresolvedImport(String),
    Bytecode(LowerError),
    Vm(VmError),
    JitCompile(CompileError),
    JitInvoke(InvokeError),
}

impl Display for ScalarComponentError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostRegistration(error) => {
                write!(formatter, "host function registration failed: {error}")
            }
            Self::ImportBinding(error) => write!(formatter, "import binding failed: {error}"),
            Self::ContractValidation(error) => {
                write!(formatter, "component contract validation failed: {error}")
            }
            Self::MissingImport(import) => {
                write!(formatter, "component import `{import}` is not declared")
            }
            Self::MissingExport(export) => {
                write!(formatter, "component export `{export}` is not declared")
            }
            Self::UnsupportedImportType {
                import,
                function_type,
            } => write!(
                formatter,
                "component import `{import}` has unsupported type {function_type}"
            ),
            Self::UnsupportedExportType {
                export,
                function_type,
            } => write!(
                formatter,
                "component export `{export}` has unsupported type {function_type}"
            ),
            Self::UnresolvedImport(import) => {
                write!(
                    formatter,
                    "component import `{import}` has no validated binding"
                )
            }
            Self::Bytecode(error) => write!(formatter, "bytecode lowering failed: {error}"),
            Self::Vm(error) => write!(formatter, "VM execution failed: {error}"),
            Self::JitCompile(error) => write!(formatter, "JIT compilation failed: {error}"),
            Self::JitInvoke(error) => write!(formatter, "JIT invocation failed: {error}"),
        }
    }
}

impl Error for ScalarComponentError {}

pub fn prototype_scalar_component_metadata() -> ComponentMetadata {
    ComponentMetadata::new(
        COMPONENT_METADATA_SCHEMA_VERSION,
        "example",
        "scalar-component",
        ComponentVersion::new(1, 0, 0),
        vec![ComponentImport::new(
            PROTOTYPE_SCALAR_IMPORT,
            ComponentFunctionType::i64_to_i64(),
            PROTOTYPE_SCALAR_CAPABILITY,
        )],
        vec![ComponentExport::new(
            PROTOTYPE_SCALAR_EXPORT,
            ComponentFunctionType::i64_to_i64(),
        )],
    )
}

pub fn prototype_scalar_component_function() -> Result<Function, ScalarComponentError> {
    let host_functions = prototype_host_functions()?;
    let validated = validate_prototype_contract(&host_functions)?;
    scalar_component_function(&validated, PROTOTYPE_SCALAR_IMPORT, PROTOTYPE_SCALAR_EXPORT)
}

pub fn scalar_component_function(
    validated: &ValidatedComponentMetadata,
    import_name: &str,
    export_name: &str,
) -> Result<Function, ScalarComponentError> {
    let import = validated
        .metadata()
        .imports()
        .iter()
        .find(|import| import.name() == import_name)
        .ok_or_else(|| ScalarComponentError::MissingImport(import_name.to_owned()))?;
    if import.function_type() != ComponentFunctionType::i64_to_i64() {
        return Err(ScalarComponentError::UnsupportedImportType {
            import: import_name.to_owned(),
            function_type: import.function_type(),
        });
    }

    let export = validated
        .metadata()
        .exports()
        .iter()
        .find(|export| export.name() == export_name)
        .ok_or_else(|| ScalarComponentError::MissingExport(export_name.to_owned()))?;
    if export.function_type() != ComponentFunctionType::i64_to_i64() {
        return Err(ScalarComponentError::UnsupportedExportType {
            export: export_name.to_owned(),
            function_type: export.function_type(),
        });
    }

    let host_function = validated
        .host_function(import_name)
        .ok_or_else(|| ScalarComponentError::UnresolvedImport(import_name.to_owned()))?;

    Ok(Function::new(
        export.name(),
        Signature::new(vec![Type::I64], Type::I64),
        vec![Instruction::CallHostI64 {
            result: ValueId::new(1),
            function: host_function,
            argument: ValueId::new(0),
        }],
        Terminator::Return(ValueId::new(1)),
    ))
}

pub fn run_prototype_scalar_component(
    value: i64,
) -> Result<ScalarComponentOutcomes, ScalarComponentError> {
    let host_functions = prototype_host_functions()?;
    let validated = validate_prototype_contract(&host_functions)?;
    let function =
        scalar_component_function(&validated, PROTOTYPE_SCALAR_IMPORT, PROTOTYPE_SCALAR_EXPORT)?;

    let program = lower(&function).map_err(ScalarComponentError::Bytecode)?;
    let vm = execute_with_hosts(&program, &[value], &host_functions)
        .map_err(ScalarComponentError::Vm)?;
    let compiled = CompiledFunction::compile_with_hosts(&function, &host_functions)
        .map_err(ScalarComponentError::JitCompile)?;
    let jit = compiled
        .invoke_i64_1(value)
        .map_err(ScalarComponentError::JitInvoke)?;

    Ok(ScalarComponentOutcomes { vm, jit })
}

fn prototype_host_functions() -> Result<HostFunctionRegistry, ScalarComponentError> {
    let mut host_functions = HostFunctionRegistry::new();
    host_functions
        .register_i64(PROTOTYPE_SCALAR_HOST, wrapping_double)
        .map_err(ScalarComponentError::HostRegistration)?;
    Ok(host_functions)
}

fn validate_prototype_contract(
    host_functions: &HostFunctionRegistry,
) -> Result<ValidatedComponentMetadata, ScalarComponentError> {
    let mut bindings = ComponentImportBindings::new();
    bindings
        .bind(PROTOTYPE_SCALAR_IMPORT, PROTOTYPE_SCALAR_HOST)
        .map_err(ScalarComponentError::ImportBinding)?;

    let mut capabilities = CapabilityGrants::new();
    capabilities.grant(PROTOTYPE_SCALAR_CAPABILITY);

    validate_component_metadata(
        prototype_scalar_component_metadata(),
        &bindings,
        host_functions,
        &capabilities,
    )
    .map_err(ScalarComponentError::ContractValidation)
}

extern "C" fn wrapping_double(value: i64) -> i64 {
    value.wrapping_mul(2)
}

#[cfg(test)]
mod tests {
    use lang_mir::{Instruction, Terminator, ValueId};

    use super::{
        PROTOTYPE_SCALAR_EXPORT, PROTOTYPE_SCALAR_HOST, ScalarComponentOutcomes,
        prototype_scalar_component_function, run_prototype_scalar_component,
    };

    #[test]
    fn validated_contract_drives_the_export_and_host_binding() {
        let function = prototype_scalar_component_function().expect("validates the fixture");

        assert_eq!(function.name(), PROTOTYPE_SCALAR_EXPORT);
        assert_eq!(
            function.blocks()[0].instructions(),
            &[Instruction::CallHostI64 {
                result: ValueId::new(1),
                function: PROTOTYPE_SCALAR_HOST,
                argument: ValueId::new(0),
            }]
        );
        assert_eq!(
            function.blocks()[0].terminator(),
            &Terminator::Return(ValueId::new(1))
        );
    }

    #[test]
    fn vm_and_jit_execute_the_same_validated_scalar_contract() {
        for value in [0_i64, 21, -7, i64::MAX, i64::MIN] {
            let expected = value.wrapping_mul(2);
            assert_eq!(
                run_prototype_scalar_component(value).expect("executes the component contract"),
                ScalarComponentOutcomes {
                    vm: expected,
                    jit: expected,
                }
            );
        }
    }
}
