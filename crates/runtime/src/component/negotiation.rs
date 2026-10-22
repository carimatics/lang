//! In-memory package and interface compatibility negotiation.
//!
//! This module prototypes the normalized compatibility model from RFC 0003. It
//! deliberately does not select a serialized metadata schema, digest algorithm,
//! package-manager API, or executable adapter representation.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::fmt::{Display, Formatter, Write};

use super::{CapabilityGrants, is_capability_identifier, is_component_identifier};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PrereleaseIdentifier {
    Numeric(String),
    AlphaNumeric(String),
}

impl PrereleaseIdentifier {
    pub fn numeric(value: impl Into<String>) -> Result<Self, VersionIdentifierError> {
        let value = value.into();
        validate_identifier_characters(&value)?;
        if !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(VersionIdentifierError::ExpectedNumeric(value));
        }
        if value.len() > 1 && value.starts_with('0') {
            return Err(VersionIdentifierError::NumericLeadingZero(value));
        }
        Ok(Self::Numeric(value))
    }

    pub fn alphanumeric(value: impl Into<String>) -> Result<Self, VersionIdentifierError> {
        let value = value.into();
        validate_identifier_characters(&value)?;
        if value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(VersionIdentifierError::ExpectedAlphaNumeric(value));
        }
        Ok(Self::AlphaNumeric(value))
    }
}

impl Display for PrereleaseIdentifier {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Numeric(value) | Self::AlphaNumeric(value) => formatter.write_str(value),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BuildIdentifier(String);

impl BuildIdentifier {
    pub fn new(value: impl Into<String>) -> Result<Self, VersionIdentifierError> {
        let value = value.into();
        validate_identifier_characters(&value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for BuildIdentifier {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn validate_identifier_characters(value: &str) -> Result<(), VersionIdentifierError> {
    if value.is_empty() {
        return Err(VersionIdentifierError::Empty);
    }
    if value
        .bytes()
        .any(|byte| !byte.is_ascii_alphanumeric() && byte != b'-')
    {
        return Err(VersionIdentifierError::InvalidCharacter(value.to_owned()));
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VersionIdentifierError {
    Empty,
    InvalidCharacter(String),
    ExpectedNumeric(String),
    ExpectedAlphaNumeric(String),
    NumericLeadingZero(String),
}

impl Display for VersionIdentifierError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("semantic-version identifier cannot be empty"),
            Self::InvalidCharacter(value) => write!(
                formatter,
                "semantic-version identifier `{value}` contains an invalid character"
            ),
            Self::ExpectedNumeric(value) => {
                write!(
                    formatter,
                    "semantic-version identifier `{value}` is not numeric"
                )
            }
            Self::ExpectedAlphaNumeric(value) => write!(
                formatter,
                "numeric semantic-version identifier `{value}` must use the numeric form"
            ),
            Self::NumericLeadingZero(value) => write!(
                formatter,
                "numeric semantic-version identifier `{value}` has a leading zero"
            ),
        }
    }
}

impl Error for VersionIdentifierError {}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SemanticVersion {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Vec<PrereleaseIdentifier>,
    build: Vec<BuildIdentifier>,
}

impl SemanticVersion {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: Vec::new(),
            build: Vec::new(),
        }
    }

    pub fn with_prerelease(mut self, prerelease: Vec<PrereleaseIdentifier>) -> Self {
        self.prerelease = prerelease;
        self
    }

    pub fn with_build(mut self, build: Vec<BuildIdentifier>) -> Self {
        self.build = build;
        self
    }

    pub const fn major(&self) -> u64 {
        self.major
    }

    pub const fn minor(&self) -> u64 {
        self.minor
    }

    pub const fn patch(&self) -> u64 {
        self.patch
    }

    pub fn prerelease(&self) -> &[PrereleaseIdentifier] {
        &self.prerelease
    }

    pub fn build(&self) -> &[BuildIdentifier] {
        &self.build
    }

    pub fn is_prerelease(&self) -> bool {
        !self.prerelease.is_empty()
    }

    pub(super) fn validate(&self) -> Result<(), VersionIdentifierError> {
        // Public enum variants can bypass the convenience constructors.
        for identifier in &self.prerelease {
            match identifier {
                PrereleaseIdentifier::Numeric(value) => {
                    PrereleaseIdentifier::numeric(value.clone())?;
                }
                PrereleaseIdentifier::AlphaNumeric(value) => {
                    PrereleaseIdentifier::alphanumeric(value.clone())?;
                }
            }
        }
        Ok(())
    }

    pub fn precedence_cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| compare_prerelease(&self.prerelease, &other.prerelease))
    }
}

fn compare_prerelease(left: &[PrereleaseIdentifier], right: &[PrereleaseIdentifier]) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }

    for (left, right) in left.iter().zip(right) {
        let ordering = match (left, right) {
            (PrereleaseIdentifier::Numeric(left), PrereleaseIdentifier::Numeric(right)) => {
                left.len().cmp(&right.len()).then_with(|| left.cmp(right))
            }
            (PrereleaseIdentifier::Numeric(_), PrereleaseIdentifier::AlphaNumeric(_)) => {
                Ordering::Less
            }
            (PrereleaseIdentifier::AlphaNumeric(_), PrereleaseIdentifier::Numeric(_)) => {
                Ordering::Greater
            }
            (
                PrereleaseIdentifier::AlphaNumeric(left),
                PrereleaseIdentifier::AlphaNumeric(right),
            ) => left.cmp(right),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }

    left.len().cmp(&right.len())
}

impl Display for SemanticVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.prerelease.is_empty() {
            formatter.write_char('-')?;
            write_joined(formatter, &self.prerelease)?;
        }
        if !self.build.is_empty() {
            formatter.write_char('+')?;
            write_joined(formatter, &self.build)?;
        }
        Ok(())
    }
}

fn write_joined<T: Display>(formatter: &mut Formatter<'_>, values: &[T]) -> std::fmt::Result {
    let mut separator = "";
    for value in values {
        formatter.write_str(separator)?;
        write!(formatter, "{value}")?;
        separator = ".";
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VersionRequirement {
    Exact(SemanticVersion),
    Interval {
        min_inclusive: SemanticVersion,
        max_exclusive: SemanticVersion,
        allow_prerelease: bool,
    },
}

impl VersionRequirement {
    fn validate(&self) -> Result<(), NegotiationError> {
        self.validate_versions()
            .map_err(NegotiationError::InvalidVersion)?;
        if let Self::Interval {
            min_inclusive,
            max_exclusive,
            ..
        } = self
            && min_inclusive.precedence_cmp(max_exclusive) != Ordering::Less
        {
            return Err(NegotiationError::InvalidVersionRequirement {
                min_inclusive: Box::new(min_inclusive.clone()),
                max_exclusive: Box::new(max_exclusive.clone()),
            });
        }
        Ok(())
    }

    pub(super) fn validate_versions(&self) -> Result<(), VersionIdentifierError> {
        match self {
            Self::Exact(version) => version.validate(),
            Self::Interval {
                min_inclusive,
                max_exclusive,
                ..
            } => {
                min_inclusive.validate()?;
                max_exclusive.validate()
            }
        }
    }

    pub(super) fn matches(&self, version: &SemanticVersion) -> bool {
        match self {
            Self::Exact(expected) => expected == version,
            Self::Interval {
                min_inclusive,
                max_exclusive,
                allow_prerelease,
            } => {
                (!version.is_prerelease() || *allow_prerelease)
                    && version.precedence_cmp(min_inclusive) != Ordering::Less
                    && version.precedence_cmp(max_exclusive) == Ordering::Less
            }
        }
    }

    pub(super) fn matches_bounds_ignoring_prerelease_policy(
        &self,
        version: &SemanticVersion,
    ) -> bool {
        match self {
            Self::Exact(expected) => expected == version,
            Self::Interval {
                min_inclusive,
                max_exclusive,
                ..
            } => {
                version.precedence_cmp(min_inclusive) != Ordering::Less
                    && version.precedence_cmp(max_exclusive) == Ordering::Less
            }
        }
    }
}

impl Display for VersionRequirement {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exact(version) => write!(formatter, "={version}"),
            Self::Interval {
                min_inclusive,
                max_exclusive,
                allow_prerelease,
            } => write!(
                formatter,
                "[{min_inclusive}, {max_exclusive}){}",
                if *allow_prerelease {
                    " with prereleases"
                } else {
                    ""
                }
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageKey {
    namespace: String,
    package: String,
}

impl PackageKey {
    pub fn new(namespace: impl Into<String>, package: impl Into<String>) -> Self {
        Self {
            namespace: namespace.into(),
            package: package.into(),
        }
    }

    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    pub fn package(&self) -> &str {
        &self.package
    }
}

impl Display for PackageKey {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}/{}", self.namespace, self.package)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceKey {
    package: PackageKey,
    interface: String,
}

impl InterfaceKey {
    pub fn new(package: PackageKey, interface: impl Into<String>) -> Self {
        Self {
            package,
            interface: interface.into(),
        }
    }

    pub fn package(&self) -> &PackageKey {
        &self.package
    }

    pub fn interface(&self) -> &str {
        &self.interface
    }
}

impl Display for InterfaceKey {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}#{}", self.package, self.interface)
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceTypeKey {
    interface: InterfaceKey,
    resource: String,
}

impl ResourceTypeKey {
    pub fn new(interface: InterfaceKey, resource: impl Into<String>) -> Self {
        Self {
            interface,
            resource: resource.into(),
        }
    }

    pub fn interface(&self) -> &InterfaceKey {
        &self.interface
    }

    pub fn resource(&self) -> &str {
        &self.resource
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContentDigest(Vec<u8>);

impl ContentDigest {
    pub fn new(bytes: impl Into<Vec<u8>>) -> Result<Self, ContentDigestError> {
        let bytes = bytes.into();
        if bytes.is_empty() {
            return Err(ContentDigestError::Empty);
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Display for ContentDigest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentDigestError {
    Empty,
}

impl Display for ContentDigestError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("content digest cannot be empty"),
        }
    }
}

impl Error for ContentDigestError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InterfaceValueType {
    I64,
    Bool,
    Own(ResourceTypeKey),
    Borrow(ResourceTypeKey),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceFunctionType {
    parameters: Vec<InterfaceValueType>,
    results: Vec<InterfaceValueType>,
}

impl InterfaceFunctionType {
    pub fn new(parameters: Vec<InterfaceValueType>, results: Vec<InterfaceValueType>) -> Self {
        Self {
            parameters,
            results,
        }
    }

    pub fn parameters(&self) -> &[InterfaceValueType] {
        &self.parameters
    }

    pub fn results(&self) -> &[InterfaceValueType] {
        &self.results
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceMember {
    name: String,
    function_type: InterfaceFunctionType,
    capability: Option<String>,
}

impl InterfaceMember {
    pub fn new(
        name: impl Into<String>,
        function_type: InterfaceFunctionType,
        capability: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            function_type,
            capability,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn function_type(&self) -> &InterfaceFunctionType {
        &self.function_type
    }

    pub fn capability(&self) -> Option<&str> {
        self.capability.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceProvider {
    key: InterfaceKey,
    version: SemanticVersion,
    digest: ContentDigest,
    members: Vec<InterfaceMember>,
}

impl InterfaceProvider {
    pub fn new(
        key: InterfaceKey,
        version: SemanticVersion,
        digest: ContentDigest,
        members: Vec<InterfaceMember>,
    ) -> Self {
        Self {
            key,
            version,
            digest,
            members,
        }
    }

    pub fn key(&self) -> &InterfaceKey {
        &self.key
    }

    pub fn version(&self) -> &SemanticVersion {
        &self.version
    }

    pub fn digest(&self) -> &ContentDigest {
        &self.digest
    }

    pub fn members(&self) -> &[InterfaceMember] {
        &self.members
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InterfaceConsumer {
    key: InterfaceKey,
    requirement: VersionRequirement,
    required_members: Vec<InterfaceMember>,
}

impl InterfaceConsumer {
    pub fn new(
        key: InterfaceKey,
        requirement: VersionRequirement,
        required_members: Vec<InterfaceMember>,
    ) -> Self {
        Self {
            key,
            requirement,
            required_members,
        }
    }

    pub fn key(&self) -> &InterfaceKey {
        &self.key
    }

    pub fn requirement(&self) -> &VersionRequirement {
        &self.requirement
    }

    pub fn required_members(&self) -> &[InterfaceMember] {
        &self.required_members
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactInterfaceLock {
    key: InterfaceKey,
    version: SemanticVersion,
    digest: ContentDigest,
}

impl ExactInterfaceLock {
    pub fn new(key: InterfaceKey, version: SemanticVersion, digest: ContentDigest) -> Self {
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

    pub fn digest(&self) -> &ContentDigest {
        &self.digest
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedInterfaceBinding {
    key: InterfaceKey,
    version: SemanticVersion,
    digest: ContentDigest,
    required_members: Vec<InterfaceMember>,
    locked: bool,
}

impl ValidatedInterfaceBinding {
    fn from_provider(
        provider: &InterfaceProvider,
        required_members: &[InterfaceMember],
        locked: bool,
    ) -> Self {
        Self {
            key: provider.key.clone(),
            version: provider.version.clone(),
            digest: provider.digest.clone(),
            required_members: required_members.to_vec(),
            locked,
        }
    }

    pub fn key(&self) -> &InterfaceKey {
        &self.key
    }

    pub fn version(&self) -> &SemanticVersion {
        &self.version
    }

    pub fn digest(&self) -> &ContentDigest {
        &self.digest
    }

    pub fn required_members(&self) -> &[InterfaceMember] {
        &self.required_members
    }

    pub const fn is_locked(&self) -> bool {
        self.locked
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectionMismatch {
    MissingMember,
    FunctionType {
        expected: InterfaceFunctionType,
        actual: InterfaceFunctionType,
    },
    Capability {
        expected: Option<String>,
        actual: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateRejection {
    version: Box<SemanticVersion>,
    digest: Box<ContentDigest>,
    member: String,
    mismatch: Box<ProjectionMismatch>,
}

impl CandidateRejection {
    pub fn version(&self) -> &SemanticVersion {
        &self.version
    }

    pub fn digest(&self) -> &ContentDigest {
        &self.digest
    }

    pub fn member(&self) -> &str {
        &self.member
    }

    pub fn mismatch(&self) -> &ProjectionMismatch {
        &self.mismatch
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NegotiationError {
    InvalidVersion(VersionIdentifierError),
    BorrowedResultUnsupported {
        member: String,
    },
    InvalidIdentifier {
        field: &'static str,
        value: String,
    },
    DuplicateRequiredMember(String),
    DuplicateProviderMember {
        version: SemanticVersion,
        member: String,
    },
    InvalidVersionRequirement {
        min_inclusive: Box<SemanticVersion>,
        max_exclusive: Box<SemanticVersion>,
    },
    MissingCapability {
        member: String,
        capability: String,
    },
    LockIdentityMismatch {
        expected: Box<InterfaceKey>,
        actual: Box<InterfaceKey>,
    },
    LockedVersionOutsideRequirement {
        version: Box<SemanticVersion>,
        requirement: Box<VersionRequirement>,
    },
    LockedProviderUnavailable {
        version: Box<SemanticVersion>,
        digest: Box<ContentDigest>,
    },
    LockDigestMismatch {
        version: Box<SemanticVersion>,
        expected: Box<ContentDigest>,
        available: Vec<ContentDigest>,
    },
    LockedProviderIncompatible(Box<CandidateRejection>),
    NoCompatibleVersion {
        key: Box<InterfaceKey>,
        requirement: Box<VersionRequirement>,
    },
    PrereleaseRejected {
        key: Box<InterfaceKey>,
        requirement: Box<VersionRequirement>,
    },
    StructuralMismatch {
        rejections: Vec<CandidateRejection>,
    },
    DuplicateProvider {
        version: Box<SemanticVersion>,
        digest: Box<ContentDigest>,
    },
    AmbiguousProvider {
        version: Box<SemanticVersion>,
        digests: Vec<ContentDigest>,
    },
}

impl Display for NegotiationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidVersion(error) => write!(formatter, "invalid semantic version: {error}"),
            Self::BorrowedResultUnsupported { member } => write!(
                formatter,
                "member `{member}` returns a borrow beyond its call scope"
            ),
            Self::InvalidIdentifier { field, value } => {
                write!(formatter, "invalid {field} identifier `{value}`")
            }
            Self::DuplicateRequiredMember(member) => {
                write!(
                    formatter,
                    "required member `{member}` is declared more than once"
                )
            }
            Self::DuplicateProviderMember { version, member } => write!(
                formatter,
                "provider {version} declares member `{member}` more than once"
            ),
            Self::InvalidVersionRequirement {
                min_inclusive,
                max_exclusive,
            } => write!(
                formatter,
                "invalid version interval [{min_inclusive}, {max_exclusive})"
            ),
            Self::MissingCapability { member, capability } => write!(
                formatter,
                "member `{member}` requires capability `{capability}`"
            ),
            Self::LockIdentityMismatch { expected, actual } => write!(
                formatter,
                "lock identifies interface {actual}, but consumer requires {expected}"
            ),
            Self::LockedVersionOutsideRequirement {
                version,
                requirement,
            } => write!(
                formatter,
                "locked version {version} does not satisfy requirement {requirement}"
            ),
            Self::LockedProviderUnavailable { version, digest } => write!(
                formatter,
                "locked provider {version} with digest {digest} is unavailable"
            ),
            Self::LockDigestMismatch {
                version, expected, ..
            } => write!(
                formatter,
                "provider {version} does not match locked digest {expected}"
            ),
            Self::LockedProviderIncompatible(rejection) => write!(
                formatter,
                "locked provider {} is structurally incompatible at member `{}`",
                rejection.version, rejection.member
            ),
            Self::NoCompatibleVersion { key, requirement } => write!(
                formatter,
                "no provider for {key} satisfies requirement {requirement}"
            ),
            Self::PrereleaseRejected { key, requirement } => write!(
                formatter,
                "prerelease providers for {key} are excluded by requirement {requirement}"
            ),
            Self::StructuralMismatch { rejections } => write!(
                formatter,
                "{} provider candidate(s) failed structural projection",
                rejections.len()
            ),
            Self::DuplicateProvider { version, digest } => write!(
                formatter,
                "provider {version} with digest {digest} is registered more than once"
            ),
            Self::AmbiguousProvider { version, digests } => write!(
                formatter,
                "{} providers have semantic-version precedence {version}",
                digests.len()
            ),
        }
    }
}

impl Error for NegotiationError {}

pub fn negotiate_interface(
    consumer: &InterfaceConsumer,
    providers: &[InterfaceProvider],
    grants: &CapabilityGrants,
    exact_lock: Option<&ExactInterfaceLock>,
) -> Result<ValidatedInterfaceBinding, NegotiationError> {
    validate_consumer(consumer, grants)?;

    for provider in providers {
        validate_provider(provider)?;
    }

    if let Some(exact_lock) = exact_lock {
        return negotiate_locked(consumer, providers, exact_lock);
    }

    negotiate_unlocked(consumer, providers)
}

pub(super) fn validate_consumer(
    consumer: &InterfaceConsumer,
    grants: &CapabilityGrants,
) -> Result<(), NegotiationError> {
    validate_interface_key(&consumer.key)?;
    consumer.requirement.validate()?;

    let mut names = HashSet::new();
    for member in &consumer.required_members {
        validate_member(member)?;
        if !names.insert(member.name.as_str()) {
            return Err(NegotiationError::DuplicateRequiredMember(
                member.name.clone(),
            ));
        }
        if let Some(capability) = member.capability()
            && !grants.contains(capability)
        {
            return Err(NegotiationError::MissingCapability {
                member: member.name.clone(),
                capability: capability.to_owned(),
            });
        }
    }
    Ok(())
}

fn validate_provider(provider: &InterfaceProvider) -> Result<(), NegotiationError> {
    validate_interface_key(&provider.key)?;
    provider
        .version
        .validate()
        .map_err(NegotiationError::InvalidVersion)?;
    let mut names = HashSet::new();
    for member in &provider.members {
        validate_member(member)?;
        if !names.insert(member.name.as_str()) {
            return Err(NegotiationError::DuplicateProviderMember {
                version: provider.version.clone(),
                member: member.name.clone(),
            });
        }
    }
    Ok(())
}

fn validate_interface_key(key: &InterfaceKey) -> Result<(), NegotiationError> {
    validate_identifier("package namespace", key.package.namespace())?;
    validate_identifier("package name", key.package.package())?;
    validate_identifier("interface name", key.interface())
}

fn validate_member(member: &InterfaceMember) -> Result<(), NegotiationError> {
    validate_identifier("member name", member.name())?;
    if member
        .function_type
        .results
        .iter()
        .any(|value| matches!(value, InterfaceValueType::Borrow(_)))
    {
        return Err(NegotiationError::BorrowedResultUnsupported {
            member: member.name.clone(),
        });
    }
    if let Some(capability) = member.capability()
        && !is_capability_identifier(capability)
    {
        return Err(NegotiationError::InvalidIdentifier {
            field: "capability",
            value: capability.to_owned(),
        });
    }
    for value_type in member
        .function_type
        .parameters
        .iter()
        .chain(&member.function_type.results)
    {
        if let InterfaceValueType::Own(resource) | InterfaceValueType::Borrow(resource) = value_type
        {
            validate_interface_key(resource.interface())?;
            validate_identifier("resource name", resource.resource())?;
        }
    }
    Ok(())
}

fn validate_identifier(field: &'static str, value: &str) -> Result<(), NegotiationError> {
    if is_component_identifier(value) {
        Ok(())
    } else {
        Err(NegotiationError::InvalidIdentifier {
            field,
            value: value.to_owned(),
        })
    }
}

fn negotiate_locked(
    consumer: &InterfaceConsumer,
    providers: &[InterfaceProvider],
    exact_lock: &ExactInterfaceLock,
) -> Result<ValidatedInterfaceBinding, NegotiationError> {
    exact_lock
        .version
        .validate()
        .map_err(NegotiationError::InvalidVersion)?;
    if exact_lock.key != consumer.key {
        return Err(NegotiationError::LockIdentityMismatch {
            expected: Box::new(consumer.key.clone()),
            actual: Box::new(exact_lock.key.clone()),
        });
    }
    if !consumer.requirement.matches(&exact_lock.version) {
        return Err(NegotiationError::LockedVersionOutsideRequirement {
            version: Box::new(exact_lock.version.clone()),
            requirement: Box::new(consumer.requirement.clone()),
        });
    }

    let same_version = providers
        .iter()
        .filter(|provider| provider.key == exact_lock.key && provider.version == exact_lock.version)
        .collect::<Vec<_>>();
    let matching = same_version
        .iter()
        .copied()
        .filter(|provider| provider.digest == exact_lock.digest)
        .collect::<Vec<_>>();

    let provider = match matching.as_slice() {
        [] if same_version.is_empty() => {
            return Err(NegotiationError::LockedProviderUnavailable {
                version: Box::new(exact_lock.version.clone()),
                digest: Box::new(exact_lock.digest.clone()),
            });
        }
        [] => {
            let mut available = same_version
                .iter()
                .map(|provider| provider.digest.clone())
                .collect::<Vec<_>>();
            available.sort();
            available.dedup();
            return Err(NegotiationError::LockDigestMismatch {
                version: Box::new(exact_lock.version.clone()),
                expected: Box::new(exact_lock.digest.clone()),
                available,
            });
        }
        [provider] => *provider,
        [provider, ..] => {
            return Err(NegotiationError::DuplicateProvider {
                version: Box::new(provider.version.clone()),
                digest: Box::new(provider.digest.clone()),
            });
        }
    };

    if let Err(rejection) = validate_projection(consumer, provider) {
        return Err(NegotiationError::LockedProviderIncompatible(Box::new(
            rejection,
        )));
    }

    Ok(ValidatedInterfaceBinding::from_provider(
        provider,
        &consumer.required_members,
        true,
    ))
}

fn negotiate_unlocked(
    consumer: &InterfaceConsumer,
    providers: &[InterfaceProvider],
) -> Result<ValidatedInterfaceBinding, NegotiationError> {
    let mut prerelease_rejected = false;
    let mut matching_versions = Vec::new();
    for provider in providers
        .iter()
        .filter(|provider| provider.key == consumer.key)
    {
        if consumer.requirement.matches(&provider.version) {
            matching_versions.push(provider);
        } else if provider.version.is_prerelease()
            && consumer
                .requirement
                .matches_bounds_ignoring_prerelease_policy(&provider.version)
        {
            prerelease_rejected = true;
        }
    }

    if matching_versions.is_empty() {
        return Err(if prerelease_rejected {
            NegotiationError::PrereleaseRejected {
                key: Box::new(consumer.key.clone()),
                requirement: Box::new(consumer.requirement.clone()),
            }
        } else {
            NegotiationError::NoCompatibleVersion {
                key: Box::new(consumer.key.clone()),
                requirement: Box::new(consumer.requirement.clone()),
            }
        });
    }

    let mut compatible = Vec::new();
    let mut rejections = Vec::new();
    for provider in matching_versions {
        match validate_projection(consumer, provider) {
            Ok(()) => compatible.push(provider),
            Err(rejection) => rejections.push(rejection),
        }
    }

    if compatible.is_empty() {
        rejections.sort_by(compare_rejections);
        return Err(NegotiationError::StructuralMismatch { rejections });
    }

    compatible.sort_by(|left, right| compare_providers(left, right));
    let highest = compatible[0];
    let same_precedence = compatible
        .iter()
        .copied()
        .take_while(|provider| provider.version.precedence_cmp(&highest.version) == Ordering::Equal)
        .collect::<Vec<_>>();

    if same_precedence.len() > 1 {
        let mut exact_identities = BTreeMap::<(String, ContentDigest), usize>::new();
        for provider in &same_precedence {
            *exact_identities
                .entry((provider.version.to_string(), provider.digest.clone()))
                .or_default() += 1;
        }
        if let Some(((version, digest), _)) = exact_identities.iter().find(|(_, count)| **count > 1)
            && let Some(provider) = same_precedence.iter().find(|provider| {
                provider.version.to_string() == *version && provider.digest == *digest
            })
        {
            return Err(NegotiationError::DuplicateProvider {
                version: Box::new(provider.version.clone()),
                digest: Box::new(digest.clone()),
            });
        }

        let mut digests = same_precedence
            .iter()
            .map(|provider| provider.digest.clone())
            .collect::<Vec<_>>();
        digests.sort();
        return Err(NegotiationError::AmbiguousProvider {
            version: Box::new(highest.version.clone()),
            digests,
        });
    }

    Ok(ValidatedInterfaceBinding::from_provider(
        highest,
        &consumer.required_members,
        false,
    ))
}

fn validate_projection(
    consumer: &InterfaceConsumer,
    provider: &InterfaceProvider,
) -> Result<(), CandidateRejection> {
    let members = provider
        .members
        .iter()
        .map(|member| (member.name.as_str(), member))
        .collect::<BTreeMap<_, _>>();

    for required in &consumer.required_members {
        let Some(actual) = members.get(required.name.as_str()) else {
            return Err(rejection(
                provider,
                required,
                ProjectionMismatch::MissingMember,
            ));
        };
        if required.function_type != actual.function_type {
            return Err(rejection(
                provider,
                required,
                ProjectionMismatch::FunctionType {
                    expected: required.function_type.clone(),
                    actual: actual.function_type.clone(),
                },
            ));
        }
        if required.capability != actual.capability {
            return Err(rejection(
                provider,
                required,
                ProjectionMismatch::Capability {
                    expected: required.capability.clone(),
                    actual: actual.capability.clone(),
                },
            ));
        }
    }
    Ok(())
}

fn rejection(
    provider: &InterfaceProvider,
    member: &InterfaceMember,
    mismatch: ProjectionMismatch,
) -> CandidateRejection {
    CandidateRejection {
        version: Box::new(provider.version.clone()),
        digest: Box::new(provider.digest.clone()),
        member: member.name.clone(),
        mismatch: Box::new(mismatch),
    }
}

fn compare_providers(left: &InterfaceProvider, right: &InterfaceProvider) -> Ordering {
    right
        .version
        .precedence_cmp(&left.version)
        .then_with(|| left.version.to_string().cmp(&right.version.to_string()))
        .then_with(|| left.digest.cmp(&right.digest))
}

fn compare_rejections(left: &CandidateRejection, right: &CandidateRejection) -> Ordering {
    right
        .version
        .precedence_cmp(&left.version)
        .then_with(|| left.version.to_string().cmp(&right.version.to_string()))
        .then_with(|| left.digest.cmp(&right.digest))
        .then_with(|| left.member.cmp(&right.member))
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use super::{
        BuildIdentifier, ContentDigest, ExactInterfaceLock, InterfaceConsumer,
        InterfaceFunctionType, InterfaceKey, InterfaceMember, InterfaceProvider,
        InterfaceValueType, NegotiationError, PackageKey, PrereleaseIdentifier, ProjectionMismatch,
        ResourceTypeKey, SemanticVersion, VersionRequirement, negotiate_interface,
    };
    use crate::component::CapabilityGrants;

    fn key() -> InterfaceKey {
        InterfaceKey::new(PackageKey::new("example", "storage"), "objects")
    }

    fn digest(value: u8) -> ContentDigest {
        ContentDigest::new(vec![value; 32]).expect("test digest is non-empty")
    }

    fn scalar_type(value_type: InterfaceValueType) -> InterfaceFunctionType {
        InterfaceFunctionType::new(vec![value_type.clone()], vec![value_type])
    }

    fn member(
        name: &str,
        function_type: InterfaceFunctionType,
        capability: Option<&str>,
    ) -> InterfaceMember {
        InterfaceMember::new(name, function_type, capability.map(str::to_owned))
    }

    fn scalar_member() -> InterfaceMember {
        member(
            "transform",
            scalar_type(InterfaceValueType::I64),
            Some("storage.read"),
        )
    }

    fn provider(
        version: SemanticVersion,
        digest_value: u8,
        members: Vec<InterfaceMember>,
    ) -> InterfaceProvider {
        InterfaceProvider::new(key(), version, digest(digest_value), members)
    }

    fn interval(allow_prerelease: bool) -> VersionRequirement {
        VersionRequirement::Interval {
            min_inclusive: SemanticVersion::new(1, 0, 0),
            max_exclusive: SemanticVersion::new(2, 0, 0),
            allow_prerelease,
        }
    }

    fn consumer(requirement: VersionRequirement) -> InterfaceConsumer {
        InterfaceConsumer::new(key(), requirement, vec![scalar_member()])
    }

    fn grants() -> CapabilityGrants {
        let mut grants = CapabilityGrants::new();
        grants.grant("storage.read");
        grants
    }

    #[test]
    fn rejects_unchecked_prerelease_variants_at_every_negotiation_input() {
        for identifier in [
            PrereleaseIdentifier::Numeric("01".to_owned()),
            PrereleaseIdentifier::Numeric("alpha".to_owned()),
            PrereleaseIdentifier::AlphaNumeric("1".to_owned()),
            PrereleaseIdentifier::AlphaNumeric("".to_owned()),
        ] {
            let invalid = SemanticVersion::new(1, 1, 0).with_prerelease(vec![identifier]);
            let valid = provider(SemanticVersion::new(1, 1, 0), 7, vec![scalar_member()]);
            assert!(matches!(
                negotiate_interface(
                    &consumer(VersionRequirement::Exact(invalid.clone())),
                    std::slice::from_ref(&valid),
                    &grants(),
                    None
                ),
                Err(NegotiationError::InvalidVersion(_))
            ));
            let bad_provider = provider(invalid.clone(), 8, vec![scalar_member()]);
            assert!(matches!(
                negotiate_interface(&consumer(interval(true)), &[bad_provider], &grants(), None),
                Err(NegotiationError::InvalidVersion(_))
            ));
            let lock = ExactInterfaceLock::new(key(), invalid, digest(7));
            assert!(matches!(
                negotiate_interface(&consumer(interval(true)), &[valid], &grants(), Some(&lock)),
                Err(NegotiationError::InvalidVersion(_))
            ));
        }
    }

    #[test]
    fn rejects_borrowed_results_in_consumers_and_providers() {
        let borrowed_result = member(
            "transform",
            InterfaceFunctionType::new(
                vec![InterfaceValueType::I64],
                vec![InterfaceValueType::Borrow(ResourceTypeKey::new(
                    key(),
                    "object",
                ))],
            ),
            Some("storage.read"),
        );
        let bad_consumer =
            InterfaceConsumer::new(key(), interval(false), vec![borrowed_result.clone()]);
        assert!(matches!(
            negotiate_interface(&bad_consumer, &[], &grants(), None),
            Err(NegotiationError::BorrowedResultUnsupported { .. })
        ));
        let bad_provider = provider(SemanticVersion::new(1, 1, 0), 7, vec![borrowed_result]);
        assert!(matches!(
            negotiate_interface(&consumer(interval(false)), &[bad_provider], &grants(), None),
            Err(NegotiationError::BorrowedResultUnsupported { .. })
        ));
    }

    #[test]
    fn selects_highest_compatible_precedence_independent_of_inventory_order() {
        let extra = member("additional", scalar_type(InterfaceValueType::Bool), None);
        let providers = vec![
            provider(SemanticVersion::new(1, 2, 0), 2, vec![scalar_member()]),
            provider(
                SemanticVersion::new(1, 1, 0),
                1,
                vec![scalar_member(), extra],
            ),
            provider(SemanticVersion::new(2, 0, 0), 3, vec![scalar_member()]),
        ];

        let binding = negotiate_interface(&consumer(interval(false)), &providers, &grants(), None)
            .expect("a compatible provider exists");

        assert_eq!(binding.version(), &SemanticVersion::new(1, 2, 0));
        assert_eq!(binding.digest(), &digest(2));
        assert!(!binding.is_locked());
        assert_eq!(binding.required_members(), &[scalar_member()]);
    }

    #[test]
    fn skips_a_higher_structurally_incompatible_provider() {
        let providers = vec![
            provider(
                SemanticVersion::new(1, 3, 0),
                3,
                vec![member(
                    "transform",
                    scalar_type(InterfaceValueType::Bool),
                    Some("storage.read"),
                )],
            ),
            provider(SemanticVersion::new(1, 2, 0), 2, vec![scalar_member()]),
        ];

        let binding = negotiate_interface(&consumer(interval(false)), &providers, &grants(), None)
            .expect("the lower compatible provider remains eligible");

        assert_eq!(binding.version(), &SemanticVersion::new(1, 2, 0));
    }

    #[test]
    fn reports_deterministically_ordered_structural_rejections() {
        let providers = vec![
            provider(SemanticVersion::new(1, 1, 0), 1, Vec::new()),
            provider(
                SemanticVersion::new(1, 2, 0),
                2,
                vec![member(
                    "transform",
                    scalar_type(InterfaceValueType::Bool),
                    Some("storage.read"),
                )],
            ),
        ];

        let error = negotiate_interface(&consumer(interval(false)), &providers, &grants(), None)
            .expect_err("all candidates are structurally incompatible");
        let NegotiationError::StructuralMismatch { rejections } = error else {
            panic!("expected structural mismatch");
        };

        assert_eq!(rejections.len(), 2);
        assert_eq!(rejections[0].version(), &SemanticVersion::new(1, 2, 0));
        assert!(matches!(
            rejections[0].mismatch(),
            ProjectionMismatch::FunctionType { .. }
        ));
        assert_eq!(rejections[1].version(), &SemanticVersion::new(1, 1, 0));
        assert_eq!(rejections[1].mismatch(), &ProjectionMismatch::MissingMember);
    }

    #[test]
    fn exact_lock_selects_only_the_named_digest_without_fallback() {
        let locked_version = SemanticVersion::new(1, 1, 0);
        let providers = vec![
            provider(locked_version.clone(), 1, vec![scalar_member()]),
            provider(SemanticVersion::new(1, 2, 0), 2, vec![scalar_member()]),
        ];
        let lock = ExactInterfaceLock::new(key(), locked_version.clone(), digest(1));

        let binding = negotiate_interface(
            &consumer(interval(false)),
            &providers,
            &grants(),
            Some(&lock),
        )
        .expect("the locked provider exists");
        assert_eq!(binding.version(), &locked_version);
        assert!(binding.is_locked());

        let wrong_digest = ExactInterfaceLock::new(key(), locked_version.clone(), digest(9));
        assert!(matches!(
            negotiate_interface(
                &consumer(interval(false)),
                &providers,
                &grants(),
                Some(&wrong_digest),
            ),
            Err(NegotiationError::LockDigestMismatch { version, .. }) if *version == locked_version
        ));
    }

    #[test]
    fn exact_lock_does_not_fallback_from_a_structurally_incompatible_provider() {
        let locked_version = SemanticVersion::new(1, 1, 0);
        let providers = vec![
            provider(
                locked_version.clone(),
                1,
                vec![member(
                    "transform",
                    scalar_type(InterfaceValueType::Bool),
                    Some("storage.read"),
                )],
            ),
            provider(SemanticVersion::new(1, 2, 0), 2, vec![scalar_member()]),
        ];
        let lock = ExactInterfaceLock::new(key(), locked_version, digest(1));

        assert!(matches!(
            negotiate_interface(
                &consumer(interval(false)),
                &providers,
                &grants(),
                Some(&lock),
            ),
            Err(NegotiationError::LockedProviderIncompatible(rejection))
                if matches!(
                    rejection.mismatch(),
                    ProjectionMismatch::FunctionType { .. }
                )
        ));
    }

    #[test]
    fn capability_is_checked_before_provider_inventory() {
        let invalid_provider = InterfaceProvider::new(
            InterfaceKey::new(PackageKey::new("INVALID", "storage"), "objects"),
            SemanticVersion::new(1, 0, 0),
            digest(1),
            vec![scalar_member()],
        );

        assert_eq!(
            negotiate_interface(
                &consumer(interval(false)),
                &[invalid_provider],
                &CapabilityGrants::new(),
                None,
            ),
            Err(NegotiationError::MissingCapability {
                member: "transform".to_owned(),
                capability: "storage.read".to_owned(),
            })
        );
    }

    #[test]
    fn provider_capability_must_match_the_required_projection() {
        let provider = provider(
            SemanticVersion::new(1, 0, 0),
            1,
            vec![member(
                "transform",
                scalar_type(InterfaceValueType::I64),
                Some("storage.write"),
            )],
        );

        assert!(matches!(
            negotiate_interface(
                &consumer(interval(false)),
                &[provider],
                &grants(),
                None,
            ),
            Err(NegotiationError::StructuralMismatch { rejections })
                if matches!(
                    rejections[0].mismatch(),
                    ProjectionMismatch::Capability { .. }
                )
        ));
    }

    #[test]
    fn prerelease_requires_explicit_interval_opt_in() {
        let prerelease = SemanticVersion::new(1, 1, 0).with_prerelease(vec![
            PrereleaseIdentifier::alphanumeric("alpha").expect("valid prerelease"),
        ]);
        let providers = vec![provider(prerelease.clone(), 1, vec![scalar_member()])];
        let requirement = VersionRequirement::Interval {
            min_inclusive: SemanticVersion::new(1, 0, 0),
            max_exclusive: SemanticVersion::new(2, 0, 0),
            allow_prerelease: false,
        };

        assert!(matches!(
            negotiate_interface(&consumer(requirement), &providers, &grants(), None),
            Err(NegotiationError::PrereleaseRejected { .. })
        ));

        let binding = negotiate_interface(&consumer(interval(true)), &providers, &grants(), None)
            .expect("explicit prerelease opt-in accepts the provider");
        assert_eq!(binding.version(), &prerelease);
    }

    #[test]
    fn same_precedence_build_or_digest_tie_is_ambiguous() {
        let linux = SemanticVersion::new(1, 2, 3).with_build(vec![
            BuildIdentifier::new("linux").expect("valid build identifier"),
        ]);
        let macos = SemanticVersion::new(1, 2, 3).with_build(vec![
            BuildIdentifier::new("macos").expect("valid build identifier"),
        ]);
        let providers = vec![
            provider(linux, 1, vec![scalar_member()]),
            provider(macos, 2, vec![scalar_member()]),
        ];

        assert!(matches!(
            negotiate_interface(&consumer(interval(false)), &providers, &grants(), None),
            Err(NegotiationError::AmbiguousProvider { digests, .. })
                if digests == vec![digest(1), digest(2)]
        ));
    }

    #[test]
    fn exact_requirement_includes_build_metadata() {
        let linux = SemanticVersion::new(1, 2, 3).with_build(vec![
            BuildIdentifier::new("linux").expect("valid build identifier"),
        ]);
        let macos = SemanticVersion::new(1, 2, 3).with_build(vec![
            BuildIdentifier::new("macos").expect("valid build identifier"),
        ]);
        let providers = vec![provider(macos, 1, vec![scalar_member()])];

        assert!(matches!(
            negotiate_interface(
                &consumer(VersionRequirement::Exact(linux)),
                &providers,
                &grants(),
                None,
            ),
            Err(NegotiationError::NoCompatibleVersion { .. })
        ));
    }

    #[test]
    fn prerelease_numeric_identifiers_use_arbitrary_precision_ordering() {
        let two = SemanticVersion::new(1, 0, 0).with_prerelease(vec![
            PrereleaseIdentifier::alphanumeric("alpha").expect("valid prerelease"),
            PrereleaseIdentifier::numeric("2").expect("valid numeric prerelease"),
        ]);
        let huge = SemanticVersion::new(1, 0, 0).with_prerelease(vec![
            PrereleaseIdentifier::alphanumeric("alpha").expect("valid prerelease"),
            PrereleaseIdentifier::numeric("184467440737095516160")
                .expect("numeric prerelease is not machine-word bounded"),
        ]);

        assert_eq!(two.precedence_cmp(&huge), Ordering::Less);
        assert_eq!(
            huge.precedence_cmp(&SemanticVersion::new(1, 0, 0)),
            Ordering::Less
        );
    }

    #[test]
    fn resource_ownership_mode_is_part_of_structural_compatibility() {
        let resource = ResourceTypeKey::new(key(), "file");
        let required = member(
            "consume",
            InterfaceFunctionType::new(vec![InterfaceValueType::Own(resource.clone())], Vec::new()),
            Some("storage.read"),
        );
        let provided = member(
            "consume",
            InterfaceFunctionType::new(vec![InterfaceValueType::Borrow(resource)], Vec::new()),
            Some("storage.read"),
        );
        let consumer = InterfaceConsumer::new(key(), interval(false), vec![required]);
        let providers = vec![provider(SemanticVersion::new(1, 0, 0), 1, vec![provided])];

        assert!(matches!(
            negotiate_interface(&consumer, &providers, &grants(), None),
            Err(NegotiationError::StructuralMismatch { rejections })
                if matches!(
                    rejections[0].mismatch(),
                    ProjectionMismatch::FunctionType { .. }
                )
        ));
    }

    #[test]
    fn rejects_empty_or_reversed_version_intervals() {
        let requirement = VersionRequirement::Interval {
            min_inclusive: SemanticVersion::new(2, 0, 0),
            max_exclusive: SemanticVersion::new(1, 0, 0),
            allow_prerelease: false,
        };

        assert!(matches!(
            negotiate_interface(&consumer(requirement), &[], &grants(), None),
            Err(NegotiationError::InvalidVersionRequirement { .. })
        ));
    }

    #[test]
    fn rejects_duplicate_required_and_provider_members() {
        let duplicate_consumer = InterfaceConsumer::new(
            key(),
            interval(false),
            vec![scalar_member(), scalar_member()],
        );
        assert_eq!(
            negotiate_interface(&duplicate_consumer, &[], &grants(), None),
            Err(NegotiationError::DuplicateRequiredMember(
                "transform".to_owned()
            ))
        );

        let duplicate_provider = provider(
            SemanticVersion::new(1, 0, 0),
            1,
            vec![scalar_member(), scalar_member()],
        );
        assert!(matches!(
            negotiate_interface(
                &consumer(interval(false)),
                &[duplicate_provider],
                &grants(),
                None,
            ),
            Err(NegotiationError::DuplicateProviderMember { member, .. })
                if member == "transform"
        ));
    }
}
