//! Candidate schema-2 serialization for versioned component contracts.
//!
//! This module is design evidence for RFC 0003. The format remains unstable
//! while the RFC is proposed; production IDL and adapter implementation remain
//! later milestones.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str;

use super::codec::ComponentCodecLimits;
use super::is_component_identifier;
use super::negotiation::{
    BuildIdentifier, InterfaceKey, PackageKey, PrereleaseIdentifier, ResourceTypeKey,
    SemanticVersion, VersionIdentifierError, VersionRequirement,
};

/// Magic shared with the experimental schema-1 metadata fixture.
pub const VERSIONED_COMPONENT_MAGIC: [u8; 4] = *b"LCMP";

/// Candidate schema version for normalized requirements, locks, and resources.
pub const VERSIONED_COMPONENT_SCHEMA_VERSION: u32 = 2;

const CONTRACT_DOCUMENT_KIND: u8 = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DigestAlgorithm {
    Sha256,
}

impl DigestAlgorithm {
    const fn tag(self) -> u8 {
        match self {
            Self::Sha256 => 0,
        }
    }

    const fn byte_length(self) -> usize {
        match self {
            Self::Sha256 => 32,
        }
    }
}

impl Display for DigestAlgorithm {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sha256 => formatter.write_str("sha256"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionedDigest {
    algorithm: DigestAlgorithm,
    bytes: Vec<u8>,
}

impl VersionedDigest {
    pub fn new(
        algorithm: DigestAlgorithm,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<Self, VersionedDigestError> {
        let bytes = bytes.into();
        let expected = algorithm.byte_length();
        if bytes.len() != expected {
            return Err(VersionedDigestError::LengthMismatch {
                algorithm,
                expected,
                actual: bytes.len(),
            });
        }
        Ok(Self { algorithm, bytes })
    }

    pub const fn algorithm(&self) -> DigestAlgorithm {
        self.algorithm
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VersionedDigestError {
    LengthMismatch {
        algorithm: DigestAlgorithm,
        expected: usize,
        actual: usize,
    },
}

impl Display for VersionedDigestError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LengthMismatch {
                algorithm,
                expected,
                actual,
            } => write!(
                formatter,
                "{algorithm} digest requires {expected} bytes, but received {actual}"
            ),
        }
    }
}

impl Error for VersionedDigestError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentContractRequirement {
    key: InterfaceKey,
    requirement: VersionRequirement,
}

impl ComponentContractRequirement {
    pub fn new(key: InterfaceKey, requirement: VersionRequirement) -> Self {
        Self { key, requirement }
    }

    pub fn key(&self) -> &InterfaceKey {
        &self.key
    }

    pub fn requirement(&self) -> &VersionRequirement {
        &self.requirement
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentContractLock {
    key: InterfaceKey,
    version: SemanticVersion,
    digest: VersionedDigest,
}

impl ComponentContractLock {
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
pub struct VersionedComponentContract {
    package: PackageKey,
    version: SemanticVersion,
    requirements: Vec<ComponentContractRequirement>,
    locks: Vec<ComponentContractLock>,
    resources: Vec<ResourceTypeKey>,
}

impl VersionedComponentContract {
    pub fn new(
        package: PackageKey,
        version: SemanticVersion,
        mut requirements: Vec<ComponentContractRequirement>,
        mut locks: Vec<ComponentContractLock>,
        mut resources: Vec<ResourceTypeKey>,
    ) -> Result<Self, ComponentContractValidationError> {
        validate_package_key(&package)?;

        for requirement in &requirements {
            validate_interface_key(requirement.key())?;
            validate_requirement(requirement)?;
        }
        requirements.sort_by(|left, right| left.key().cmp(right.key()));
        reject_duplicate_requirements(&requirements)?;

        for lock in &locks {
            validate_interface_key(lock.key())?;
        }
        locks.sort_by(|left, right| left.key().cmp(right.key()));
        reject_duplicate_locks(&locks)?;
        validate_locks(&requirements, &locks)?;

        for resource in &resources {
            validate_resource_type_key(resource)?;
        }
        resources.sort();
        reject_duplicate_resources(&resources)?;

        Ok(Self {
            package,
            version,
            requirements,
            locks,
            resources,
        })
    }

    pub fn package(&self) -> &PackageKey {
        &self.package
    }

    pub fn version(&self) -> &SemanticVersion {
        &self.version
    }

    pub fn requirements(&self) -> &[ComponentContractRequirement] {
        &self.requirements
    }

    pub fn locks(&self) -> &[ComponentContractLock] {
        &self.locks
    }

    pub fn resources(&self) -> &[ResourceTypeKey] {
        &self.resources
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComponentContractIdentifierField {
    PackageNamespace,
    PackageName,
    InterfaceName,
    ResourceName,
}

impl Display for ComponentContractIdentifierField {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PackageNamespace => formatter.write_str("package namespace"),
            Self::PackageName => formatter.write_str("package name"),
            Self::InterfaceName => formatter.write_str("interface name"),
            Self::ResourceName => formatter.write_str("resource name"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComponentContractValidationError {
    InvalidIdentifier {
        field: ComponentContractIdentifierField,
        value: String,
    },
    InvalidVersionInterval {
        key: Box<InterfaceKey>,
        min_inclusive: Box<SemanticVersion>,
        max_exclusive: Box<SemanticVersion>,
    },
    DuplicateRequirement(InterfaceKey),
    DuplicateLock(InterfaceKey),
    DuplicateResource(ResourceTypeKey),
    MissingRequirementForLock(InterfaceKey),
    LockVersionMismatch {
        key: Box<InterfaceKey>,
        version: Box<SemanticVersion>,
    },
}

impl Display for ComponentContractValidationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdentifier { field, value } => {
                write!(formatter, "invalid {field} `{value}`")
            }
            Self::InvalidVersionInterval {
                key,
                min_inclusive,
                max_exclusive,
            } => write!(
                formatter,
                "interface `{key}` has invalid version interval [{min_inclusive}, {max_exclusive})"
            ),
            Self::DuplicateRequirement(key) => {
                write!(formatter, "duplicate interface requirement `{key}`")
            }
            Self::DuplicateLock(key) => write!(formatter, "duplicate interface lock `{key}`"),
            Self::DuplicateResource(resource) => write!(
                formatter,
                "duplicate resource identity `{}#{}`",
                resource.interface(),
                resource.resource()
            ),
            Self::MissingRequirementForLock(key) => write!(
                formatter,
                "interface lock `{key}` has no matching requirement"
            ),
            Self::LockVersionMismatch { key, version } => write!(
                formatter,
                "locked version `{version}` does not satisfy interface requirement `{key}`"
            ),
        }
    }
}

impl Error for ComponentContractValidationError {}

fn validate_package_key(package: &PackageKey) -> Result<(), ComponentContractValidationError> {
    validate_identifier(
        ComponentContractIdentifierField::PackageNamespace,
        package.namespace(),
    )?;
    validate_identifier(
        ComponentContractIdentifierField::PackageName,
        package.package(),
    )
}

fn validate_interface_key(key: &InterfaceKey) -> Result<(), ComponentContractValidationError> {
    validate_package_key(key.package())?;
    validate_identifier(
        ComponentContractIdentifierField::InterfaceName,
        key.interface(),
    )
}

fn validate_resource_type_key(
    resource: &ResourceTypeKey,
) -> Result<(), ComponentContractValidationError> {
    validate_interface_key(resource.interface())?;
    validate_identifier(
        ComponentContractIdentifierField::ResourceName,
        resource.resource(),
    )
}

fn validate_identifier(
    field: ComponentContractIdentifierField,
    value: &str,
) -> Result<(), ComponentContractValidationError> {
    if is_component_identifier(value) {
        Ok(())
    } else {
        Err(ComponentContractValidationError::InvalidIdentifier {
            field,
            value: value.to_owned(),
        })
    }
}

fn validate_requirement(
    requirement: &ComponentContractRequirement,
) -> Result<(), ComponentContractValidationError> {
    if let VersionRequirement::Interval {
        min_inclusive,
        max_exclusive,
        ..
    } = requirement.requirement()
        && min_inclusive.precedence_cmp(max_exclusive) != std::cmp::Ordering::Less
    {
        return Err(ComponentContractValidationError::InvalidVersionInterval {
            key: Box::new(requirement.key().clone()),
            min_inclusive: Box::new(min_inclusive.clone()),
            max_exclusive: Box::new(max_exclusive.clone()),
        });
    }
    Ok(())
}

fn reject_duplicate_requirements(
    requirements: &[ComponentContractRequirement],
) -> Result<(), ComponentContractValidationError> {
    for adjacent in requirements.windows(2) {
        if adjacent[0].key() == adjacent[1].key() {
            return Err(ComponentContractValidationError::DuplicateRequirement(
                adjacent[0].key().clone(),
            ));
        }
    }
    Ok(())
}

fn reject_duplicate_locks(
    locks: &[ComponentContractLock],
) -> Result<(), ComponentContractValidationError> {
    for adjacent in locks.windows(2) {
        if adjacent[0].key() == adjacent[1].key() {
            return Err(ComponentContractValidationError::DuplicateLock(
                adjacent[0].key().clone(),
            ));
        }
    }
    Ok(())
}

fn reject_duplicate_resources(
    resources: &[ResourceTypeKey],
) -> Result<(), ComponentContractValidationError> {
    for adjacent in resources.windows(2) {
        if adjacent[0] == adjacent[1] {
            return Err(ComponentContractValidationError::DuplicateResource(
                adjacent[0].clone(),
            ));
        }
    }
    Ok(())
}

fn validate_locks(
    requirements: &[ComponentContractRequirement],
    locks: &[ComponentContractLock],
) -> Result<(), ComponentContractValidationError> {
    for lock in locks {
        let Ok(index) = requirements.binary_search_by(|entry| entry.key().cmp(lock.key())) else {
            return Err(ComponentContractValidationError::MissingRequirementForLock(
                lock.key().clone(),
            ));
        };
        if !requirement_matches(requirements[index].requirement(), lock.version()) {
            return Err(ComponentContractValidationError::LockVersionMismatch {
                key: Box::new(lock.key().clone()),
                version: Box::new(lock.version().clone()),
            });
        }
    }
    Ok(())
}

fn requirement_matches(requirement: &VersionRequirement, version: &SemanticVersion) -> bool {
    match requirement {
        VersionRequirement::Exact(expected) => expected == version,
        VersionRequirement::Interval {
            min_inclusive,
            max_exclusive,
            allow_prerelease,
        } => {
            (!version.is_prerelease() || *allow_prerelease)
                && version.precedence_cmp(min_inclusive) != std::cmp::Ordering::Less
                && version.precedence_cmp(max_exclusive) == std::cmp::Ordering::Less
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionedComponentSection {
    PrereleaseIdentifiers,
    BuildIdentifiers,
    Requirements,
    Locks,
    Resources,
}

impl Display for VersionedComponentSection {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PrereleaseIdentifiers => formatter.write_str("prerelease identifiers"),
            Self::BuildIdentifiers => formatter.write_str("build identifiers"),
            Self::Requirements => formatter.write_str("interface requirements"),
            Self::Locks => formatter.write_str("interface locks"),
            Self::Resources => formatter.write_str("resource identities"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionedComponentStringField {
    PackageNamespace,
    PackageName,
    InterfaceName,
    ResourceName,
    PrereleaseIdentifier,
    BuildIdentifier,
}

impl Display for VersionedComponentStringField {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PackageNamespace => formatter.write_str("package namespace"),
            Self::PackageName => formatter.write_str("package name"),
            Self::InterfaceName => formatter.write_str("interface name"),
            Self::ResourceName => formatter.write_str("resource name"),
            Self::PrereleaseIdentifier => formatter.write_str("prerelease identifier"),
            Self::BuildIdentifier => formatter.write_str("build identifier"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VersionedComponentCodecErrorKind {
    ByteLimitExceeded {
        limit: usize,
        actual: usize,
    },
    ItemCountExceeded {
        section: VersionedComponentSection,
        limit: usize,
        actual: usize,
    },
    NestingDepthExceeded {
        limit: usize,
        required: usize,
    },
    StringLengthExceeded {
        field: VersionedComponentStringField,
        limit: usize,
        actual: usize,
    },
    LengthOverflow {
        actual: usize,
    },
    InvalidMagic {
        found: [u8; 4],
    },
    UnsupportedSchemaVersion {
        expected: u32,
        actual: u32,
    },
    UnexpectedDocumentKind {
        expected: u8,
        actual: u8,
    },
    UnexpectedEnd {
        needed: usize,
        remaining: usize,
    },
    InvalidUtf8 {
        field: VersionedComponentStringField,
    },
    UnknownPrereleaseIdentifierTag {
        tag: u8,
    },
    UnknownVersionRequirementTag {
        tag: u8,
    },
    InvalidBoolean {
        value: u8,
    },
    UnknownDigestAlgorithm {
        tag: u8,
    },
    InvalidVersionIdentifier(VersionIdentifierError),
    InvalidDigest(VersionedDigestError),
    InvalidContract(ComponentContractValidationError),
    TrailingBytes {
        remaining: usize,
    },
}

impl Display for VersionedComponentCodecErrorKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ByteLimitExceeded { limit, actual } => {
                write!(formatter, "byte limit {limit} exceeded by {actual} bytes")
            }
            Self::ItemCountExceeded {
                section,
                limit,
                actual,
            } => write!(
                formatter,
                "{section} item limit {limit} exceeded by count {actual}"
            ),
            Self::NestingDepthExceeded { limit, required } => write!(
                formatter,
                "nesting-depth limit {limit} is below required depth {required}"
            ),
            Self::StringLengthExceeded {
                field,
                limit,
                actual,
            } => write!(
                formatter,
                "{field} byte limit {limit} exceeded by length {actual}"
            ),
            Self::LengthOverflow { actual } => {
                write!(formatter, "length {actual} cannot be represented as u32")
            }
            Self::InvalidMagic { found } => {
                write!(formatter, "invalid component-contract magic {:02x?}", found)
            }
            Self::UnsupportedSchemaVersion { expected, actual } => write!(
                formatter,
                "unsupported component-contract schema {actual}; expected {expected}"
            ),
            Self::UnexpectedDocumentKind { expected, actual } => write!(
                formatter,
                "unexpected component document kind {actual}; expected {expected}"
            ),
            Self::UnexpectedEnd { needed, remaining } => write!(
                formatter,
                "unexpected end: needed {needed} bytes, but {remaining} remain"
            ),
            Self::InvalidUtf8 { field } => write!(formatter, "invalid UTF-8 in {field}"),
            Self::UnknownPrereleaseIdentifierTag { tag } => {
                write!(formatter, "unknown prerelease identifier tag {tag}")
            }
            Self::UnknownVersionRequirementTag { tag } => {
                write!(formatter, "unknown version requirement tag {tag}")
            }
            Self::InvalidBoolean { value } => {
                write!(formatter, "invalid canonical boolean value {value}")
            }
            Self::UnknownDigestAlgorithm { tag } => {
                write!(formatter, "unknown digest algorithm tag {tag}")
            }
            Self::InvalidVersionIdentifier(error) => Display::fmt(error, formatter),
            Self::InvalidDigest(error) => Display::fmt(error, formatter),
            Self::InvalidContract(error) => Display::fmt(error, formatter),
            Self::TrailingBytes { remaining } => {
                write!(formatter, "{remaining} trailing bytes remain")
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionedComponentCodecError {
    offset: usize,
    kind: VersionedComponentCodecErrorKind,
}

impl VersionedComponentCodecError {
    const fn new(offset: usize, kind: VersionedComponentCodecErrorKind) -> Self {
        Self { offset, kind }
    }

    pub const fn offset(&self) -> usize {
        self.offset
    }

    pub const fn kind(&self) -> &VersionedComponentCodecErrorKind {
        &self.kind
    }
}

impl Display for VersionedComponentCodecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "component-contract codec error at byte {}: {}",
            self.offset, self.kind
        )
    }
}

impl Error for VersionedComponentCodecError {}

/// Encodes a candidate schema-2 component contract deterministically.
pub fn encode_versioned_component_contract(
    contract: &VersionedComponentContract,
    limits: ComponentCodecLimits,
) -> Result<Vec<u8>, VersionedComponentCodecError> {
    let mut writer = Writer::new(limits);
    writer.require_depth(1)?;
    writer.write_bytes(&VERSIONED_COMPONENT_MAGIC)?;
    writer.write_u32(VERSIONED_COMPONENT_SCHEMA_VERSION)?;
    writer.write_u8(CONTRACT_DOCUMENT_KIND)?;

    writer.require_depth(2)?;
    writer.write_package_key(contract.package())?;
    writer.write_semantic_version(contract.version())?;

    writer.write_count(
        VersionedComponentSection::Requirements,
        contract.requirements().len(),
    )?;
    for requirement in contract.requirements() {
        writer.require_depth(3)?;
        writer.write_interface_key(requirement.key())?;
        writer.write_version_requirement(requirement.requirement())?;
    }

    writer.write_count(VersionedComponentSection::Locks, contract.locks().len())?;
    for lock in contract.locks() {
        writer.require_depth(3)?;
        writer.write_interface_key(lock.key())?;
        writer.write_semantic_version(lock.version())?;
        writer.write_digest(lock.digest())?;
    }

    writer.write_count(
        VersionedComponentSection::Resources,
        contract.resources().len(),
    )?;
    for resource in contract.resources() {
        writer.require_depth(3)?;
        writer.write_resource_type_key(resource)?;
    }

    Ok(writer.finish())
}

/// Decodes untrusted candidate schema-2 component-contract bytes under bounds.
pub fn decode_versioned_component_contract(
    bytes: &[u8],
    limits: ComponentCodecLimits,
) -> Result<VersionedComponentContract, VersionedComponentCodecError> {
    if bytes.len() > limits.max_bytes() {
        return Err(VersionedComponentCodecError::new(
            0,
            VersionedComponentCodecErrorKind::ByteLimitExceeded {
                limit: limits.max_bytes(),
                actual: bytes.len(),
            },
        ));
    }

    let mut reader = Reader::new(bytes, limits);
    reader.require_depth(1)?;
    let magic_offset = reader.offset();
    let magic = reader.read_array::<4>()?;
    if magic != VERSIONED_COMPONENT_MAGIC {
        return Err(VersionedComponentCodecError::new(
            magic_offset,
            VersionedComponentCodecErrorKind::InvalidMagic { found: magic },
        ));
    }
    let schema_offset = reader.offset();
    let schema = reader.read_u32()?;
    if schema != VERSIONED_COMPONENT_SCHEMA_VERSION {
        return Err(VersionedComponentCodecError::new(
            schema_offset,
            VersionedComponentCodecErrorKind::UnsupportedSchemaVersion {
                expected: VERSIONED_COMPONENT_SCHEMA_VERSION,
                actual: schema,
            },
        ));
    }
    let kind_offset = reader.offset();
    let kind = reader.read_u8()?;
    if kind != CONTRACT_DOCUMENT_KIND {
        return Err(VersionedComponentCodecError::new(
            kind_offset,
            VersionedComponentCodecErrorKind::UnexpectedDocumentKind {
                expected: CONTRACT_DOCUMENT_KIND,
                actual: kind,
            },
        ));
    }

    reader.require_depth(2)?;
    let contract_offset = reader.offset();
    let package = reader.read_package_key()?;
    let version = reader.read_semantic_version()?;

    let requirement_count = reader.read_count(VersionedComponentSection::Requirements, 45)?;
    let mut requirements = Vec::with_capacity(requirement_count);
    for _ in 0..requirement_count {
        reader.require_depth(3)?;
        requirements.push(ComponentContractRequirement::new(
            reader.read_interface_key()?,
            reader.read_version_requirement()?,
        ));
    }

    let lock_count = reader.read_count(VersionedComponentSection::Locks, 77)?;
    let mut locks = Vec::with_capacity(lock_count);
    for _ in 0..lock_count {
        reader.require_depth(3)?;
        locks.push(ComponentContractLock::new(
            reader.read_interface_key()?,
            reader.read_semantic_version()?,
            reader.read_digest()?,
        ));
    }

    let resource_count = reader.read_count(VersionedComponentSection::Resources, 16)?;
    let mut resources = Vec::with_capacity(resource_count);
    for _ in 0..resource_count {
        reader.require_depth(3)?;
        resources.push(reader.read_resource_type_key()?);
    }

    reader.finish()?;
    VersionedComponentContract::new(package, version, requirements, locks, resources).map_err(
        |error| {
            VersionedComponentCodecError::new(
                contract_offset,
                VersionedComponentCodecErrorKind::InvalidContract(error),
            )
        },
    )
}

struct Writer {
    bytes: Vec<u8>,
    limits: ComponentCodecLimits,
}

impl Writer {
    fn new(limits: ComponentCodecLimits) -> Self {
        Self {
            bytes: Vec::new(),
            limits,
        }
    }

    fn offset(&self) -> usize {
        self.bytes.len()
    }

    fn require_depth(&self, required: usize) -> Result<(), VersionedComponentCodecError> {
        if required > self.limits.max_nesting_depth() {
            return Err(VersionedComponentCodecError::new(
                self.offset(),
                VersionedComponentCodecErrorKind::NestingDepthExceeded {
                    limit: self.limits.max_nesting_depth(),
                    required,
                },
            ));
        }
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), VersionedComponentCodecError> {
        let end = self.offset().checked_add(bytes.len()).ok_or_else(|| {
            VersionedComponentCodecError::new(
                self.offset(),
                VersionedComponentCodecErrorKind::LengthOverflow { actual: usize::MAX },
            )
        })?;
        if end > self.limits.max_bytes() {
            return Err(VersionedComponentCodecError::new(
                self.offset(),
                VersionedComponentCodecErrorKind::ByteLimitExceeded {
                    limit: self.limits.max_bytes(),
                    actual: end,
                },
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    fn write_u8(&mut self, value: u8) -> Result<(), VersionedComponentCodecError> {
        self.write_bytes(&[value])
    }

    fn write_u32(&mut self, value: u32) -> Result<(), VersionedComponentCodecError> {
        self.write_bytes(&value.to_le_bytes())
    }

    fn write_u64(&mut self, value: u64) -> Result<(), VersionedComponentCodecError> {
        self.write_bytes(&value.to_le_bytes())
    }

    fn write_count(
        &mut self,
        section: VersionedComponentSection,
        count: usize,
    ) -> Result<(), VersionedComponentCodecError> {
        let offset = self.offset();
        if count > self.limits.max_items_per_section() {
            return Err(VersionedComponentCodecError::new(
                offset,
                VersionedComponentCodecErrorKind::ItemCountExceeded {
                    section,
                    limit: self.limits.max_items_per_section(),
                    actual: count,
                },
            ));
        }
        let count = u32::try_from(count).map_err(|_| {
            VersionedComponentCodecError::new(
                offset,
                VersionedComponentCodecErrorKind::LengthOverflow { actual: count },
            )
        })?;
        self.write_u32(count)
    }

    fn write_string(
        &mut self,
        field: VersionedComponentStringField,
        value: &str,
    ) -> Result<(), VersionedComponentCodecError> {
        let offset = self.offset();
        if value.len() > self.limits.max_string_bytes() {
            return Err(VersionedComponentCodecError::new(
                offset,
                VersionedComponentCodecErrorKind::StringLengthExceeded {
                    field,
                    limit: self.limits.max_string_bytes(),
                    actual: value.len(),
                },
            ));
        }
        let length = u32::try_from(value.len()).map_err(|_| {
            VersionedComponentCodecError::new(
                offset,
                VersionedComponentCodecErrorKind::LengthOverflow {
                    actual: value.len(),
                },
            )
        })?;
        self.write_u32(length)?;
        self.write_bytes(value.as_bytes())
    }

    fn write_package_key(&mut self, key: &PackageKey) -> Result<(), VersionedComponentCodecError> {
        self.write_string(
            VersionedComponentStringField::PackageNamespace,
            key.namespace(),
        )?;
        self.write_string(VersionedComponentStringField::PackageName, key.package())
    }

    fn write_interface_key(
        &mut self,
        key: &InterfaceKey,
    ) -> Result<(), VersionedComponentCodecError> {
        self.write_package_key(key.package())?;
        self.write_string(
            VersionedComponentStringField::InterfaceName,
            key.interface(),
        )
    }

    fn write_resource_type_key(
        &mut self,
        key: &ResourceTypeKey,
    ) -> Result<(), VersionedComponentCodecError> {
        self.write_interface_key(key.interface())?;
        self.write_string(VersionedComponentStringField::ResourceName, key.resource())
    }

    fn write_semantic_version(
        &mut self,
        version: &SemanticVersion,
    ) -> Result<(), VersionedComponentCodecError> {
        self.write_u64(version.major())?;
        self.write_u64(version.minor())?;
        self.write_u64(version.patch())?;
        self.write_count(
            VersionedComponentSection::PrereleaseIdentifiers,
            version.prerelease().len(),
        )?;
        for identifier in version.prerelease() {
            match identifier {
                PrereleaseIdentifier::Numeric(value) => {
                    self.write_u8(0)?;
                    self.write_string(VersionedComponentStringField::PrereleaseIdentifier, value)?;
                }
                PrereleaseIdentifier::AlphaNumeric(value) => {
                    self.write_u8(1)?;
                    self.write_string(VersionedComponentStringField::PrereleaseIdentifier, value)?;
                }
            }
        }
        self.write_count(
            VersionedComponentSection::BuildIdentifiers,
            version.build().len(),
        )?;
        for identifier in version.build() {
            self.write_string(
                VersionedComponentStringField::BuildIdentifier,
                identifier.as_str(),
            )?;
        }
        Ok(())
    }

    fn write_version_requirement(
        &mut self,
        requirement: &VersionRequirement,
    ) -> Result<(), VersionedComponentCodecError> {
        match requirement {
            VersionRequirement::Exact(version) => {
                self.write_u8(0)?;
                self.write_semantic_version(version)
            }
            VersionRequirement::Interval {
                min_inclusive,
                max_exclusive,
                allow_prerelease,
            } => {
                self.write_u8(1)?;
                self.write_semantic_version(min_inclusive)?;
                self.write_semantic_version(max_exclusive)?;
                self.write_u8(u8::from(*allow_prerelease))
            }
        }
    }

    fn write_digest(
        &mut self,
        digest: &VersionedDigest,
    ) -> Result<(), VersionedComponentCodecError> {
        self.write_u8(digest.algorithm().tag())?;
        self.write_bytes(digest.as_bytes())
    }

    fn finish(self) -> Vec<u8> {
        self.bytes
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
    limits: ComponentCodecLimits,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8], limits: ComponentCodecLimits) -> Self {
        Self {
            bytes,
            offset: 0,
            limits,
        }
    }

    const fn offset(&self) -> usize {
        self.offset
    }

    fn require_depth(&self, required: usize) -> Result<(), VersionedComponentCodecError> {
        if required > self.limits.max_nesting_depth() {
            return Err(VersionedComponentCodecError::new(
                self.offset(),
                VersionedComponentCodecErrorKind::NestingDepthExceeded {
                    limit: self.limits.max_nesting_depth(),
                    required,
                },
            ));
        }
        Ok(())
    }

    fn read_exact(&mut self, length: usize) -> Result<&'a [u8], VersionedComponentCodecError> {
        let remaining = self.bytes.len().saturating_sub(self.offset);
        if length > remaining {
            return Err(VersionedComponentCodecError::new(
                self.offset,
                VersionedComponentCodecErrorKind::UnexpectedEnd {
                    needed: length,
                    remaining,
                },
            ));
        }
        let start = self.offset;
        self.offset += length;
        Ok(&self.bytes[start..self.offset])
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], VersionedComponentCodecError> {
        let mut array = [0_u8; N];
        array.copy_from_slice(self.read_exact(N)?);
        Ok(array)
    }

    fn read_u8(&mut self) -> Result<u8, VersionedComponentCodecError> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32, VersionedComponentCodecError> {
        Ok(u32::from_le_bytes(self.read_array()?))
    }

    fn read_u64(&mut self) -> Result<u64, VersionedComponentCodecError> {
        Ok(u64::from_le_bytes(self.read_array()?))
    }

    fn read_count(
        &mut self,
        section: VersionedComponentSection,
        minimum_item_bytes: usize,
    ) -> Result<usize, VersionedComponentCodecError> {
        let offset = self.offset();
        let count = self.read_u32()? as usize;
        if count > self.limits.max_items_per_section() {
            return Err(VersionedComponentCodecError::new(
                offset,
                VersionedComponentCodecErrorKind::ItemCountExceeded {
                    section,
                    limit: self.limits.max_items_per_section(),
                    actual: count,
                },
            ));
        }
        let minimum_bytes = count.saturating_mul(minimum_item_bytes);
        let remaining = self.bytes.len().saturating_sub(self.offset());
        if minimum_bytes > remaining {
            return Err(VersionedComponentCodecError::new(
                offset,
                VersionedComponentCodecErrorKind::UnexpectedEnd {
                    needed: minimum_bytes,
                    remaining,
                },
            ));
        }
        Ok(count)
    }

    fn read_string(
        &mut self,
        field: VersionedComponentStringField,
    ) -> Result<String, VersionedComponentCodecError> {
        let length_offset = self.offset();
        let length = self.read_u32()? as usize;
        if length > self.limits.max_string_bytes() {
            return Err(VersionedComponentCodecError::new(
                length_offset,
                VersionedComponentCodecErrorKind::StringLengthExceeded {
                    field,
                    limit: self.limits.max_string_bytes(),
                    actual: length,
                },
            ));
        }
        let string_offset = self.offset();
        let bytes = self.read_exact(length)?;
        str::from_utf8(bytes).map(str::to_owned).map_err(|error| {
            VersionedComponentCodecError::new(
                string_offset + error.valid_up_to(),
                VersionedComponentCodecErrorKind::InvalidUtf8 { field },
            )
        })
    }

    fn read_package_key(&mut self) -> Result<PackageKey, VersionedComponentCodecError> {
        Ok(PackageKey::new(
            self.read_string(VersionedComponentStringField::PackageNamespace)?,
            self.read_string(VersionedComponentStringField::PackageName)?,
        ))
    }

    fn read_interface_key(&mut self) -> Result<InterfaceKey, VersionedComponentCodecError> {
        Ok(InterfaceKey::new(
            self.read_package_key()?,
            self.read_string(VersionedComponentStringField::InterfaceName)?,
        ))
    }

    fn read_resource_type_key(&mut self) -> Result<ResourceTypeKey, VersionedComponentCodecError> {
        Ok(ResourceTypeKey::new(
            self.read_interface_key()?,
            self.read_string(VersionedComponentStringField::ResourceName)?,
        ))
    }

    fn read_semantic_version(&mut self) -> Result<SemanticVersion, VersionedComponentCodecError> {
        let major = self.read_u64()?;
        let minor = self.read_u64()?;
        let patch = self.read_u64()?;
        let prerelease_count =
            self.read_count(VersionedComponentSection::PrereleaseIdentifiers, 5)?;
        let mut prerelease = Vec::with_capacity(prerelease_count);
        for _ in 0..prerelease_count {
            let tag_offset = self.offset();
            let tag = self.read_u8()?;
            let value = self.read_string(VersionedComponentStringField::PrereleaseIdentifier)?;
            let identifier = match tag {
                0 => PrereleaseIdentifier::numeric(value),
                1 => PrereleaseIdentifier::alphanumeric(value),
                _ => {
                    return Err(VersionedComponentCodecError::new(
                        tag_offset,
                        VersionedComponentCodecErrorKind::UnknownPrereleaseIdentifierTag { tag },
                    ));
                }
            }
            .map_err(|error| {
                VersionedComponentCodecError::new(
                    tag_offset,
                    VersionedComponentCodecErrorKind::InvalidVersionIdentifier(error),
                )
            })?;
            prerelease.push(identifier);
        }

        let build_count = self.read_count(VersionedComponentSection::BuildIdentifiers, 4)?;
        let mut build = Vec::with_capacity(build_count);
        for _ in 0..build_count {
            let offset = self.offset();
            let value = self.read_string(VersionedComponentStringField::BuildIdentifier)?;
            build.push(BuildIdentifier::new(value).map_err(|error| {
                VersionedComponentCodecError::new(
                    offset,
                    VersionedComponentCodecErrorKind::InvalidVersionIdentifier(error),
                )
            })?);
        }

        Ok(SemanticVersion::new(major, minor, patch)
            .with_prerelease(prerelease)
            .with_build(build))
    }

    fn read_version_requirement(
        &mut self,
    ) -> Result<VersionRequirement, VersionedComponentCodecError> {
        let tag_offset = self.offset();
        match self.read_u8()? {
            0 => Ok(VersionRequirement::Exact(self.read_semantic_version()?)),
            1 => {
                let min_inclusive = self.read_semantic_version()?;
                let max_exclusive = self.read_semantic_version()?;
                let boolean_offset = self.offset();
                let allow_prerelease = match self.read_u8()? {
                    0 => false,
                    1 => true,
                    value => {
                        return Err(VersionedComponentCodecError::new(
                            boolean_offset,
                            VersionedComponentCodecErrorKind::InvalidBoolean { value },
                        ));
                    }
                };
                Ok(VersionRequirement::Interval {
                    min_inclusive,
                    max_exclusive,
                    allow_prerelease,
                })
            }
            tag => Err(VersionedComponentCodecError::new(
                tag_offset,
                VersionedComponentCodecErrorKind::UnknownVersionRequirementTag { tag },
            )),
        }
    }

    fn read_digest(&mut self) -> Result<VersionedDigest, VersionedComponentCodecError> {
        let algorithm_offset = self.offset();
        let algorithm = match self.read_u8()? {
            0 => DigestAlgorithm::Sha256,
            tag => {
                return Err(VersionedComponentCodecError::new(
                    algorithm_offset,
                    VersionedComponentCodecErrorKind::UnknownDigestAlgorithm { tag },
                ));
            }
        };
        let bytes = self.read_exact(algorithm.byte_length())?.to_vec();
        VersionedDigest::new(algorithm, bytes).map_err(|error| {
            VersionedComponentCodecError::new(
                algorithm_offset,
                VersionedComponentCodecErrorKind::InvalidDigest(error),
            )
        })
    }

    fn finish(self) -> Result<(), VersionedComponentCodecError> {
        let remaining = self.bytes.len() - self.offset;
        if remaining == 0 {
            Ok(())
        } else {
            Err(VersionedComponentCodecError::new(
                self.offset,
                VersionedComponentCodecErrorKind::TrailingBytes { remaining },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ComponentContractLock, ComponentContractRequirement, ComponentContractValidationError,
        DigestAlgorithm, VERSIONED_COMPONENT_MAGIC, VERSIONED_COMPONENT_SCHEMA_VERSION,
        VersionedComponentCodecErrorKind, VersionedComponentContract, VersionedComponentSection,
        VersionedComponentStringField, VersionedDigest, VersionedDigestError,
        decode_versioned_component_contract, encode_versioned_component_contract,
    };
    use crate::component::codec::{
        ComponentCodecErrorKind as LegacyComponentCodecErrorKind, ComponentCodecLimits,
        decode_component_metadata,
    };
    use crate::component::negotiation::{
        BuildIdentifier, InterfaceKey, PackageKey, PrereleaseIdentifier, ResourceTypeKey,
        SemanticVersion, VersionRequirement,
    };

    fn interface(name: &str) -> InterfaceKey {
        InterfaceKey::new(PackageKey::new("example", "platform"), name)
    }

    fn version(major: u64, minor: u64, patch: u64) -> SemanticVersion {
        SemanticVersion::new(major, minor, patch)
    }

    fn contract() -> VersionedComponentContract {
        let clocks = interface("clocks");
        let storage = interface("storage");
        VersionedComponentContract::new(
            PackageKey::new("example", "application"),
            version(2, 1, 0)
                .with_prerelease(vec![
                    PrereleaseIdentifier::alphanumeric("beta").expect("valid identifier"),
                    PrereleaseIdentifier::numeric("1").expect("valid identifier"),
                ])
                .with_build(vec![
                    BuildIdentifier::new("linux").expect("valid identifier"),
                ]),
            vec![
                ComponentContractRequirement::new(
                    storage.clone(),
                    VersionRequirement::Exact(version(3, 2, 1)),
                ),
                ComponentContractRequirement::new(
                    clocks.clone(),
                    VersionRequirement::Interval {
                        min_inclusive: version(1, 0, 0),
                        max_exclusive: version(2, 0, 0),
                        allow_prerelease: false,
                    },
                ),
            ],
            vec![
                ComponentContractLock::new(
                    storage.clone(),
                    version(3, 2, 1),
                    VersionedDigest::new(DigestAlgorithm::Sha256, vec![0x22; 32])
                        .expect("valid digest"),
                ),
                ComponentContractLock::new(
                    clocks.clone(),
                    version(1, 4, 0),
                    VersionedDigest::new(DigestAlgorithm::Sha256, vec![0x11; 32])
                        .expect("valid digest"),
                ),
            ],
            vec![
                ResourceTypeKey::new(storage, "object"),
                ResourceTypeKey::new(clocks, "timer"),
            ],
        )
        .expect("fixture contract should validate")
    }

    fn minimal_interval_contract() -> VersionedComponentContract {
        let clocks = interface("clocks");
        VersionedComponentContract::new(
            PackageKey::new("example", "application"),
            version(1, 0, 0),
            vec![ComponentContractRequirement::new(
                clocks.clone(),
                VersionRequirement::Interval {
                    min_inclusive: version(1, 0, 0),
                    max_exclusive: version(2, 0, 0),
                    allow_prerelease: false,
                },
            )],
            vec![ComponentContractLock::new(
                clocks,
                version(1, 4, 0),
                VersionedDigest::new(DigestAlgorithm::Sha256, vec![0x33; 32])
                    .expect("valid digest"),
            )],
            Vec::new(),
        )
        .expect("minimal interval contract should validate")
    }

    fn limits(
        max_bytes: usize,
        max_items: usize,
        max_depth: usize,
        max_string_bytes: usize,
    ) -> ComponentCodecLimits {
        ComponentCodecLimits::new(max_bytes, max_items, max_depth, max_string_bytes)
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn round_trip_is_deterministic_and_canonicalizes_collections() {
        let contract = contract();
        assert_eq!(contract.requirements()[0].key().interface(), "clocks");
        assert_eq!(contract.locks()[0].key().interface(), "clocks");
        assert_eq!(contract.resources()[0].resource(), "timer");

        let encoded =
            encode_versioned_component_contract(&contract, ComponentCodecLimits::default())
                .expect("contract should encode");
        assert_eq!(&encoded[..4], &VERSIONED_COMPONENT_MAGIC);
        assert_eq!(
            u32::from_le_bytes(encoded[4..8].try_into().expect("schema bytes")),
            VERSIONED_COMPONENT_SCHEMA_VERSION
        );
        assert_eq!(encoded[8], 0);
        let legacy_error = decode_component_metadata(&encoded, ComponentCodecLimits::default())
            .expect_err("schema-1 decoder must not reinterpret schema 2");
        assert_eq!(legacy_error.offset(), 4);
        assert!(matches!(
            legacy_error.kind(),
            LegacyComponentCodecErrorKind::UnsupportedSchemaVersion {
                expected: 1,
                actual: VERSIONED_COMPONENT_SCHEMA_VERSION,
            }
        ));
        assert_eq!(
            hex(&encoded),
            "4c434d500200000000070000006578616d706c650b0000006170706c69636174696f6e0200000000000000010000000000000000000000000000000200000001040000006265746100010000003101000000050000006c696e757802000000070000006578616d706c6508000000706c6174666f726d06000000636c6f636b73010100000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000070000006578616d706c6508000000706c6174666f726d0700000073746f7261676500030000000000000002000000000000000100000000000000000000000000000002000000070000006578616d706c6508000000706c6174666f726d06000000636c6f636b730100000000000000040000000000000000000000000000000000000000000000001111111111111111111111111111111111111111111111111111111111111111070000006578616d706c6508000000706c6174666f726d0700000073746f72616765030000000000000002000000000000000100000000000000000000000000000000222222222222222222222222222222222222222222222222222222222222222202000000070000006578616d706c6508000000706c6174666f726d06000000636c6f636b730500000074696d6572070000006578616d706c6508000000706c6174666f726d0700000073746f72616765060000006f626a656374"
        );

        let decoded =
            decode_versioned_component_contract(&encoded, ComponentCodecLimits::default())
                .expect("contract should decode");
        assert_eq!(decoded, contract);
        assert_eq!(
            encode_versioned_component_contract(&decoded, ComponentCodecLimits::default())
                .expect("decoded contract should re-encode"),
            encoded
        );
    }

    #[test]
    fn rejects_duplicate_and_unsatisfied_locks() {
        let clocks = interface("clocks");
        let requirement = ComponentContractRequirement::new(
            clocks.clone(),
            VersionRequirement::Interval {
                min_inclusive: version(1, 0, 0),
                max_exclusive: version(2, 0, 0),
                allow_prerelease: false,
            },
        );
        let duplicate = VersionedComponentContract::new(
            PackageKey::new("example", "application"),
            version(1, 0, 0),
            vec![requirement.clone(), requirement.clone()],
            Vec::new(),
            Vec::new(),
        );
        assert_eq!(
            duplicate,
            Err(ComponentContractValidationError::DuplicateRequirement(
                clocks.clone()
            ))
        );

        let mismatch = VersionedComponentContract::new(
            PackageKey::new("example", "application"),
            version(1, 0, 0),
            vec![requirement],
            vec![ComponentContractLock::new(
                clocks.clone(),
                version(2, 0, 0),
                VersionedDigest::new(DigestAlgorithm::Sha256, vec![0; 32]).expect("valid digest"),
            )],
            Vec::new(),
        );
        assert_eq!(
            mismatch,
            Err(ComponentContractValidationError::LockVersionMismatch {
                key: Box::new(clocks),
                version: Box::new(version(2, 0, 0)),
            })
        );
        assert_eq!(
            VersionedDigest::new(DigestAlgorithm::Sha256, vec![0; 31]),
            Err(VersionedDigestError::LengthMismatch {
                algorithm: DigestAlgorithm::Sha256,
                expected: 32,
                actual: 31,
            })
        );
    }

    #[test]
    fn enforces_bounds_before_allocating_sections() {
        let contract = contract();
        let encoded =
            encode_versioned_component_contract(&contract, ComponentCodecLimits::default())
                .expect("fixture should encode");

        let byte_error =
            decode_versioned_component_contract(&encoded, limits(encoded.len() - 1, 8, 3, 64))
                .expect_err("byte limit should reject input");
        assert!(matches!(
            byte_error.kind(),
            VersionedComponentCodecErrorKind::ByteLimitExceeded { .. }
        ));

        let requirements_only = VersionedComponentContract::new(
            PackageKey::new("example", "application"),
            version(1, 0, 0),
            vec![
                ComponentContractRequirement::new(
                    interface("clocks"),
                    VersionRequirement::Exact(version(1, 0, 0)),
                ),
                ComponentContractRequirement::new(
                    interface("storage"),
                    VersionRequirement::Exact(version(1, 0, 0)),
                ),
            ],
            Vec::new(),
            Vec::new(),
        )
        .expect("requirements-only contract should validate");
        let requirements_encoded = encode_versioned_component_contract(
            &requirements_only,
            ComponentCodecLimits::default(),
        )
        .expect("requirements-only contract should encode");
        let item_error = decode_versioned_component_contract(
            &requirements_encoded,
            limits(requirements_encoded.len(), 1, 3, 64),
        )
        .expect_err("two requirements should exceed the limit");
        assert!(matches!(
            item_error.kind(),
            VersionedComponentCodecErrorKind::ItemCountExceeded {
                section: VersionedComponentSection::Requirements,
                actual: 2,
                ..
            }
        ));

        let depth_error =
            encode_versioned_component_contract(&contract, limits(encoded.len(), 8, 2, 64))
                .expect_err("contract entries require depth three");
        assert!(matches!(
            depth_error.kind(),
            VersionedComponentCodecErrorKind::NestingDepthExceeded { required: 3, .. }
        ));

        let string_error =
            decode_versioned_component_contract(&encoded, limits(encoded.len(), 8, 3, 6))
                .expect_err("package namespace should exceed the string limit");
        assert_eq!(string_error.offset(), 9);
        assert!(matches!(
            string_error.kind(),
            VersionedComponentCodecErrorKind::StringLengthExceeded {
                field: VersionedComponentStringField::PackageNamespace,
                limit: 6,
                actual: 7,
            }
        ));
    }

    #[test]
    fn reports_header_and_payload_corruption_at_offsets() {
        let encoded =
            encode_versioned_component_contract(&contract(), ComponentCodecLimits::default())
                .expect("fixture should encode");

        let mut invalid_magic = encoded.clone();
        invalid_magic[0] = 0;
        let magic_error =
            decode_versioned_component_contract(&invalid_magic, ComponentCodecLimits::default())
                .expect_err("invalid magic should fail");
        assert_eq!(magic_error.offset(), 0);
        assert!(matches!(
            magic_error.kind(),
            VersionedComponentCodecErrorKind::InvalidMagic { .. }
        ));

        let mut schema_one = encoded.clone();
        schema_one[4..8].copy_from_slice(&1_u32.to_le_bytes());
        let schema_error =
            decode_versioned_component_contract(&schema_one, ComponentCodecLimits::default())
                .expect_err("schema one belongs to the legacy decoder");
        assert_eq!(schema_error.offset(), 4);
        assert!(matches!(
            schema_error.kind(),
            VersionedComponentCodecErrorKind::UnsupportedSchemaVersion {
                expected: VERSIONED_COMPONENT_SCHEMA_VERSION,
                actual: 1,
            }
        ));

        let mut wrong_kind = encoded.clone();
        wrong_kind[8] = 7;
        let kind_error =
            decode_versioned_component_contract(&wrong_kind, ComponentCodecLimits::default())
                .expect_err("unknown document kind should fail");
        assert_eq!(kind_error.offset(), 8);
        assert!(matches!(
            kind_error.kind(),
            VersionedComponentCodecErrorKind::UnexpectedDocumentKind { actual: 7, .. }
        ));

        let mut trailing = encoded;
        let offset = trailing.len();
        trailing.push(0);
        let trailing_error =
            decode_versioned_component_contract(&trailing, ComponentCodecLimits::default())
                .expect_err("trailing bytes should fail");
        assert_eq!(trailing_error.offset(), offset);
        assert!(matches!(
            trailing_error.kind(),
            VersionedComponentCodecErrorKind::TrailingBytes { remaining: 1 }
        ));
    }

    #[test]
    fn reports_malformed_schema_2_payload_offsets() {
        let encoded = encode_versioned_component_contract(
            &minimal_interval_contract(),
            ComponentCodecLimits::default(),
        )
        .expect("minimal contract should encode");

        let mut impossible_count = encoded.clone();
        impossible_count[67..71].copy_from_slice(&u32::MAX.to_le_bytes());
        let count_error = decode_versioned_component_contract(
            &impossible_count,
            limits(impossible_count.len(), usize::MAX, 3, 64),
        )
        .expect_err("count requiring more bytes than remain must fail before allocation");
        assert_eq!(count_error.offset(), 67);
        assert!(matches!(
            count_error.kind(),
            VersionedComponentCodecErrorKind::UnexpectedEnd { needed, remaining }
                if needed > remaining
        ));

        let mut invalid_utf8 = encoded.clone();
        invalid_utf8[13] = 0xff;
        let utf8_error =
            decode_versioned_component_contract(&invalid_utf8, ComponentCodecLimits::default())
                .expect_err("invalid package namespace UTF-8 should fail");
        assert_eq!(utf8_error.offset(), 13);
        assert!(matches!(
            utf8_error.kind(),
            VersionedComponentCodecErrorKind::InvalidUtf8 {
                field: VersionedComponentStringField::PackageNamespace,
            }
        ));

        let mut unknown_requirement = encoded.clone();
        unknown_requirement[104] = 9;
        let requirement_error = decode_versioned_component_contract(
            &unknown_requirement,
            ComponentCodecLimits::default(),
        )
        .expect_err("unknown requirement tag should fail");
        assert_eq!(requirement_error.offset(), 104);
        assert!(matches!(
            requirement_error.kind(),
            VersionedComponentCodecErrorKind::UnknownVersionRequirementTag { tag: 9 }
        ));

        let mut invalid_boolean = encoded.clone();
        invalid_boolean[169] = 2;
        let boolean_error =
            decode_versioned_component_contract(&invalid_boolean, ComponentCodecLimits::default())
                .expect_err("non-canonical boolean should fail");
        assert_eq!(boolean_error.offset(), 169);
        assert!(matches!(
            boolean_error.kind(),
            VersionedComponentCodecErrorKind::InvalidBoolean { value: 2 }
        ));

        let mut unknown_digest = encoded.clone();
        unknown_digest[239] = 7;
        let digest_error =
            decode_versioned_component_contract(&unknown_digest, ComponentCodecLimits::default())
                .expect_err("unknown digest algorithm should fail");
        assert_eq!(digest_error.offset(), 239);
        assert!(matches!(
            digest_error.kind(),
            VersionedComponentCodecErrorKind::UnknownDigestAlgorithm { tag: 7 }
        ));

        let truncation_error =
            decode_versioned_component_contract(&encoded[..240], ComponentCodecLimits::default())
                .expect_err("truncated lock should fail before allocation");
        assert_eq!(truncation_error.offset(), 170);
        assert!(matches!(
            truncation_error.kind(),
            VersionedComponentCodecErrorKind::UnexpectedEnd {
                needed: 77,
                remaining: 66,
            }
        ));
    }

    #[test]
    fn arbitrary_byte_corpus_never_panics_or_exceeds_input_bounds() {
        let limits = limits(512, 16, 3, 64);
        let mut state = 0x517c_c1b7_u32;
        for length in 0..=512 {
            let mut bytes = vec![0_u8; length];
            for byte in &mut bytes {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                *byte = state.to_le_bytes()[3];
            }
            let _ = decode_versioned_component_contract(&bytes, limits);
        }
    }
}
