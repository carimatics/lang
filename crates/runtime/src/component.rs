pub mod adapter;
pub mod bindings;
pub mod codec;
pub mod host_value;
pub mod idl;
pub mod instantiation;
pub mod lockfile;
pub mod negotiation;
pub mod resource;
pub mod serialization;
pub mod wire;

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter};

use lang_mir::HostFunctionId;

use crate::HostFunctionRegistry;

pub const COMPONENT_METADATA_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl ComponentVersion {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    pub const fn major(self) -> u32 {
        self.major
    }

    pub const fn minor(self) -> u32 {
        self.minor
    }

    pub const fn patch(self) -> u32 {
        self.patch
    }
}

impl Display for ComponentVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentScalarType {
    I64,
    Bool,
}

impl Display for ComponentScalarType {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I64 => formatter.write_str("i64"),
            Self::Bool => formatter.write_str("bool"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentFunctionType {
    parameter: ComponentScalarType,
    result: ComponentScalarType,
}

impl ComponentFunctionType {
    pub const fn new(parameter: ComponentScalarType, result: ComponentScalarType) -> Self {
        Self { parameter, result }
    }

    pub const fn i64_to_i64() -> Self {
        Self::new(ComponentScalarType::I64, ComponentScalarType::I64)
    }

    pub const fn parameter(self) -> ComponentScalarType {
        self.parameter
    }

    pub const fn result(self) -> ComponentScalarType {
        self.result
    }
}

impl Display for ComponentFunctionType {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "({}) -> {}", self.parameter, self.result)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentImport {
    name: String,
    function_type: ComponentFunctionType,
    capability: String,
}

impl ComponentImport {
    pub fn new(
        name: impl Into<String>,
        function_type: ComponentFunctionType,
        capability: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            function_type,
            capability: capability.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn function_type(&self) -> ComponentFunctionType {
        self.function_type
    }

    pub fn capability(&self) -> &str {
        &self.capability
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentExport {
    name: String,
    function_type: ComponentFunctionType,
}

impl ComponentExport {
    pub fn new(name: impl Into<String>, function_type: ComponentFunctionType) -> Self {
        Self {
            name: name.into(),
            function_type,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn function_type(&self) -> ComponentFunctionType {
        self.function_type
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentMetadata {
    schema_version: u32,
    package_namespace: String,
    package_name: String,
    version: ComponentVersion,
    imports: Vec<ComponentImport>,
    exports: Vec<ComponentExport>,
}

impl ComponentMetadata {
    pub fn new(
        schema_version: u32,
        package_namespace: impl Into<String>,
        package_name: impl Into<String>,
        version: ComponentVersion,
        imports: Vec<ComponentImport>,
        exports: Vec<ComponentExport>,
    ) -> Self {
        Self {
            schema_version,
            package_namespace: package_namespace.into(),
            package_name: package_name.into(),
            version,
            imports,
            exports,
        }
    }

    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn package_namespace(&self) -> &str {
        &self.package_namespace
    }

    pub fn package_name(&self) -> &str {
        &self.package_name
    }

    pub const fn version(&self) -> ComponentVersion {
        self.version
    }

    pub fn imports(&self) -> &[ComponentImport] {
        &self.imports
    }

    pub fn exports(&self) -> &[ComponentExport] {
        &self.exports
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CapabilityGrants {
    capabilities: HashSet<String>,
}

impl CapabilityGrants {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn grant(&mut self, capability: impl Into<String>) -> bool {
        self.capabilities.insert(capability.into())
    }

    pub fn contains(&self, capability: &str) -> bool {
        self.capabilities.contains(capability)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ComponentImportBindings {
    bindings: BTreeMap<String, HostFunctionId>,
}

impl ComponentImportBindings {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind(
        &mut self,
        import: impl Into<String>,
        host_function: HostFunctionId,
    ) -> Result<(), ComponentBindingError> {
        let import = import.into();
        match self.bindings.entry(import) {
            Entry::Vacant(entry) => {
                entry.insert(host_function);
                Ok(())
            }
            Entry::Occupied(entry) => Err(ComponentBindingError::DuplicateBinding(
                entry.key().to_owned(),
            )),
        }
    }

    pub fn resolve(&self, import: &str) -> Option<HostFunctionId> {
        self.bindings.get(import).copied()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedComponentMetadata {
    metadata: ComponentMetadata,
    host_functions: BTreeMap<String, HostFunctionId>,
}

impl ValidatedComponentMetadata {
    pub const fn metadata(&self) -> &ComponentMetadata {
        &self.metadata
    }

    pub fn host_function(&self, import: &str) -> Option<HostFunctionId> {
        self.host_functions.get(import).copied()
    }

    pub fn into_inner(self) -> ComponentMetadata {
        self.metadata
    }
}

pub fn validate_component_metadata(
    metadata: ComponentMetadata,
    bindings: &ComponentImportBindings,
    host_functions: &HostFunctionRegistry,
    capabilities: &CapabilityGrants,
) -> Result<ValidatedComponentMetadata, ComponentValidationError> {
    validate_structure(&metadata)?;

    for binding in bindings.bindings.keys() {
        if !metadata
            .imports()
            .iter()
            .any(|import| import.name() == binding)
        {
            return Err(ComponentValidationError::UnknownImportBinding(
                binding.to_owned(),
            ));
        }
    }

    for import in metadata.imports() {
        if !capabilities.contains(import.capability()) {
            return Err(ComponentValidationError::MissingCapability {
                import: import.name().to_owned(),
                capability: import.capability().to_owned(),
            });
        }
    }

    let mut resolved_host_functions = BTreeMap::new();
    for import in metadata.imports() {
        let function = bindings.resolve(import.name()).ok_or_else(|| {
            ComponentValidationError::MissingImportBinding(import.name().to_owned())
        })?;
        if host_functions.resolve_i64(function).is_none() {
            return Err(ComponentValidationError::UnknownHostFunction {
                import: import.name().to_owned(),
                function,
            });
        }
        resolved_host_functions.insert(import.name().to_owned(), function);
    }

    Ok(ValidatedComponentMetadata {
        metadata,
        host_functions: resolved_host_functions,
    })
}

fn validate_structure(metadata: &ComponentMetadata) -> Result<(), ComponentValidationError> {
    if metadata.schema_version() != COMPONENT_METADATA_SCHEMA_VERSION {
        return Err(ComponentValidationError::UnsupportedSchemaVersion {
            expected: COMPONENT_METADATA_SCHEMA_VERSION,
            actual: metadata.schema_version(),
        });
    }
    if !is_component_identifier(metadata.package_namespace()) {
        return Err(ComponentValidationError::InvalidPackageNamespace(
            metadata.package_namespace().to_owned(),
        ));
    }
    if !is_component_identifier(metadata.package_name()) {
        return Err(ComponentValidationError::InvalidPackageName(
            metadata.package_name().to_owned(),
        ));
    }

    let mut import_names = HashSet::new();
    for import in metadata.imports() {
        if !is_component_identifier(import.name()) {
            return Err(ComponentValidationError::InvalidImportName(
                import.name().to_owned(),
            ));
        }
        if !is_capability_identifier(import.capability()) {
            return Err(ComponentValidationError::InvalidCapabilityName(
                import.capability().to_owned(),
            ));
        }
        if import.function_type() != ComponentFunctionType::i64_to_i64() {
            return Err(ComponentValidationError::UnsupportedImportType {
                import: import.name().to_owned(),
                function_type: import.function_type(),
            });
        }
        if !import_names.insert(import.name()) {
            return Err(ComponentValidationError::DuplicateImportName(
                import.name().to_owned(),
            ));
        }
    }

    let mut export_names = HashSet::new();
    for export in metadata.exports() {
        if !is_component_identifier(export.name()) {
            return Err(ComponentValidationError::InvalidExportName(
                export.name().to_owned(),
            ));
        }
        if !export_names.insert(export.name()) {
            return Err(ComponentValidationError::DuplicateExportName(
                export.name().to_owned(),
            ));
        }
        if import_names.contains(export.name()) {
            return Err(ComponentValidationError::ImportExportNameCollision(
                export.name().to_owned(),
            ));
        }
    }

    Ok(())
}

pub(super) fn is_component_identifier(value: &str) -> bool {
    value.split('-').all(|part| {
        let mut characters = part.chars();
        characters
            .next()
            .is_some_and(|first| first.is_ascii_lowercase())
            && characters
                .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    })
}

pub(super) fn is_capability_identifier(value: &str) -> bool {
    value.split('.').all(is_component_identifier)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentValidationError {
    UnsupportedSchemaVersion {
        expected: u32,
        actual: u32,
    },
    InvalidPackageNamespace(String),
    InvalidPackageName(String),
    InvalidImportName(String),
    InvalidExportName(String),
    InvalidCapabilityName(String),
    DuplicateImportName(String),
    DuplicateExportName(String),
    ImportExportNameCollision(String),
    UnknownImportBinding(String),
    MissingImportBinding(String),
    UnsupportedImportType {
        import: String,
        function_type: ComponentFunctionType,
    },
    MissingCapability {
        import: String,
        capability: String,
    },
    UnknownHostFunction {
        import: String,
        function: HostFunctionId,
    },
}

impl Display for ComponentValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { expected, actual } => write!(
                formatter,
                "component metadata schema {actual} is unsupported; expected {expected}"
            ),
            Self::InvalidPackageNamespace(namespace) => {
                write!(
                    formatter,
                    "component package namespace `{namespace}` is invalid"
                )
            }
            Self::InvalidPackageName(name) => {
                write!(formatter, "component package name `{name}` is invalid")
            }
            Self::InvalidImportName(name) => {
                write!(formatter, "component import name `{name}` is invalid")
            }
            Self::InvalidExportName(name) => {
                write!(formatter, "component export name `{name}` is invalid")
            }
            Self::InvalidCapabilityName(capability) => {
                write!(formatter, "component capability `{capability}` is invalid")
            }
            Self::DuplicateImportName(name) => {
                write!(
                    formatter,
                    "component import `{name}` is declared more than once"
                )
            }
            Self::DuplicateExportName(name) => {
                write!(
                    formatter,
                    "component export `{name}` is declared more than once"
                )
            }
            Self::ImportExportNameCollision(name) => write!(
                formatter,
                "component item `{name}` cannot be both an import and an export"
            ),
            Self::UnknownImportBinding(import) => write!(
                formatter,
                "host binding `{import}` does not name a component import"
            ),
            Self::MissingImportBinding(import) => {
                write!(formatter, "component import `{import}` has no host binding")
            }
            Self::UnsupportedImportType {
                import,
                function_type,
            } => write!(
                formatter,
                "component import `{import}` has unsupported type {function_type}; expected (i64) -> i64"
            ),
            Self::MissingCapability { import, capability } => write!(
                formatter,
                "component import `{import}` requires capability `{capability}`"
            ),
            Self::UnknownHostFunction { import, function } => write!(
                formatter,
                "component import `{import}` references unregistered host function {function}"
            ),
        }
    }
}

impl Error for ComponentValidationError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentBindingError {
    DuplicateBinding(String),
}

impl Display for ComponentBindingError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateBinding(import) => {
                write!(formatter, "component import `{import}` is already bound")
            }
        }
    }
}

impl Error for ComponentBindingError {}

#[cfg(test)]
mod tests {
    use lang_mir::HostFunctionId;

    use super::{
        COMPONENT_METADATA_SCHEMA_VERSION, CapabilityGrants, ComponentBindingError,
        ComponentExport, ComponentFunctionType, ComponentImport, ComponentImportBindings,
        ComponentMetadata, ComponentScalarType, ComponentValidationError, ComponentVersion,
        validate_component_metadata,
    };
    use crate::HostFunctionRegistry;

    extern "C" fn identity(value: i64) -> i64 {
        value
    }

    fn metadata(component_import: ComponentImport) -> ComponentMetadata {
        ComponentMetadata::new(
            COMPONENT_METADATA_SCHEMA_VERSION,
            "example",
            "counter",
            ComponentVersion::new(1, 2, 3),
            vec![component_import],
            vec![ComponentExport::new(
                "run",
                ComponentFunctionType::i64_to_i64(),
            )],
        )
    }

    fn component_import() -> ComponentImport {
        ComponentImport::new(
            "read-clock",
            ComponentFunctionType::i64_to_i64(),
            "clock.monotonic",
        )
    }

    #[test]
    fn validates_versioned_metadata_with_explicit_capability() {
        let id = HostFunctionId::new(7);
        let mut registry = HostFunctionRegistry::new();
        registry.register_i64(id, identity).unwrap();
        let mut bindings = ComponentImportBindings::new();
        bindings.bind("read-clock", id).unwrap();
        let mut capabilities = CapabilityGrants::new();
        assert!(capabilities.grant("clock.monotonic"));

        let validated = validate_component_metadata(
            metadata(component_import()),
            &bindings,
            &registry,
            &capabilities,
        )
        .unwrap();

        assert_eq!(validated.metadata().package_namespace(), "example");
        assert_eq!(validated.metadata().package_name(), "counter");
        assert_eq!(
            validated.metadata().version(),
            ComponentVersion::new(1, 2, 3)
        );
        assert_eq!(validated.host_function("read-clock"), Some(id));
    }

    #[test]
    fn rejects_unknown_schema_before_other_fields() {
        let invalid = ComponentMetadata::new(
            COMPONENT_METADATA_SCHEMA_VERSION + 1,
            "Invalid",
            "counter",
            ComponentVersion::new(1, 0, 0),
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(
            validate_component_metadata(
                invalid,
                &ComponentImportBindings::new(),
                &HostFunctionRegistry::new(),
                &CapabilityGrants::new()
            ),
            Err(ComponentValidationError::UnsupportedSchemaVersion {
                expected: COMPONENT_METADATA_SCHEMA_VERSION,
                actual: COMPONENT_METADATA_SCHEMA_VERSION + 1,
            })
        );
    }

    #[test]
    fn rejects_invalid_and_duplicate_contract_names() {
        let invalid = ComponentMetadata::new(
            COMPONENT_METADATA_SCHEMA_VERSION,
            "example",
            "counter",
            ComponentVersion::new(1, 0, 0),
            vec![
                component_import(),
                ComponentImport::new(
                    "read-clock",
                    ComponentFunctionType::i64_to_i64(),
                    "clock.monotonic",
                ),
            ],
            Vec::new(),
        );

        assert_eq!(
            validate_component_metadata(
                invalid,
                &ComponentImportBindings::new(),
                &HostFunctionRegistry::new(),
                &CapabilityGrants::new()
            ),
            Err(ComponentValidationError::DuplicateImportName(
                "read-clock".to_owned()
            ))
        );

        let collision = ComponentMetadata::new(
            COMPONENT_METADATA_SCHEMA_VERSION,
            "example",
            "counter",
            ComponentVersion::new(1, 0, 0),
            vec![component_import()],
            vec![ComponentExport::new(
                "read-clock",
                ComponentFunctionType::i64_to_i64(),
            )],
        );

        assert_eq!(
            validate_component_metadata(
                collision,
                &ComponentImportBindings::new(),
                &HostFunctionRegistry::new(),
                &CapabilityGrants::new()
            ),
            Err(ComponentValidationError::ImportExportNameCollision(
                "read-clock".to_owned()
            ))
        );
    }

    #[test]
    fn rejects_an_import_type_the_registry_cannot_supply() {
        let unsupported = ComponentImport::new(
            "read-clock",
            ComponentFunctionType::new(ComponentScalarType::Bool, ComponentScalarType::Bool),
            "clock.monotonic",
        );

        assert_eq!(
            validate_component_metadata(
                metadata(unsupported),
                &ComponentImportBindings::new(),
                &HostFunctionRegistry::new(),
                &CapabilityGrants::new()
            ),
            Err(ComponentValidationError::UnsupportedImportType {
                import: "read-clock".to_owned(),
                function_type: ComponentFunctionType::new(
                    ComponentScalarType::Bool,
                    ComponentScalarType::Bool
                ),
            })
        );
    }

    #[test]
    fn checks_capability_before_host_registration() {
        let id = HostFunctionId::new(99);
        let component = metadata(component_import());
        let mut bindings = ComponentImportBindings::new();
        bindings.bind("read-clock", id).unwrap();

        assert_eq!(
            validate_component_metadata(
                component.clone(),
                &bindings,
                &HostFunctionRegistry::new(),
                &CapabilityGrants::new()
            ),
            Err(ComponentValidationError::MissingCapability {
                import: "read-clock".to_owned(),
                capability: "clock.monotonic".to_owned(),
            })
        );

        let mut capabilities = CapabilityGrants::new();
        capabilities.grant("clock.monotonic");
        assert_eq!(
            validate_component_metadata(
                component,
                &bindings,
                &HostFunctionRegistry::new(),
                &capabilities
            ),
            Err(ComponentValidationError::UnknownHostFunction {
                import: "read-clock".to_owned(),
                function: id,
            })
        );
    }

    #[test]
    fn validates_import_bindings_separately_from_portable_metadata() {
        let id = HostFunctionId::new(7);
        let mut capabilities = CapabilityGrants::new();
        capabilities.grant("clock.monotonic");

        assert_eq!(
            validate_component_metadata(
                metadata(component_import()),
                &ComponentImportBindings::new(),
                &HostFunctionRegistry::new(),
                &capabilities,
            ),
            Err(ComponentValidationError::MissingImportBinding(
                "read-clock".to_owned()
            ))
        );

        let mut bindings = ComponentImportBindings::new();
        assert_eq!(bindings.bind("read-clock", id), Ok(()));
        assert_eq!(
            bindings.bind("read-clock", id),
            Err(ComponentBindingError::DuplicateBinding(
                "read-clock".to_owned()
            ))
        );
        bindings.bind("zeta-undeclared", id).unwrap();
        bindings.bind("alpha-undeclared", id).unwrap();
        assert_eq!(
            validate_component_metadata(
                metadata(component_import()),
                &bindings,
                &HostFunctionRegistry::new(),
                &capabilities,
            ),
            Err(ComponentValidationError::UnknownImportBinding(
                "alpha-undeclared".to_owned()
            ))
        );
    }
}
