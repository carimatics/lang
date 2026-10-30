//! Authenticated, bounded host toolchain inputs (RFC 0042).
//! No verification operation opens payloads or changes installation state.
pub mod command;
pub mod installation;
use ed25519_dalek::{Signature, VerifyingKey};
pub use installation::distribution;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_ROOT_BYTES: usize = 64 * 1024;
pub const MAX_METADATA_BYTES: usize = 1024 * 1024;
pub const MAX_TIME: u64 = 253_402_300_799;
pub const MAX_RELEASE_BYTES: u64 = 1024 * 1024 * 1024;
pub const SIGNING_DOMAIN: &[u8] = b"lang-toolchain-channel-v1\0";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Format,
    Limit,
    Identity,
    Signature,
    Expired,
    Clock,
    Rollback,
    Payload,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub message: &'static str,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for Error {}
fn err(kind: ErrorKind, message: &'static str) -> Error {
    Error { kind, message }
}
fn format(message: &'static str) -> Error {
    err(ErrorKind::Format, message)
}
pub fn digest(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut result, "{byte:02x}").expect("writing to String");
    }
    result
}
fn hex<const N: usize>(value: &str) -> Result<[u8; N], Error> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(format("invalid lowercase hexadecimal identity"));
    }
    let mut bytes = [0; N];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
            .map_err(|_| format("invalid hexadecimal byte"))?;
    }
    Ok(bytes)
}
fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T, Error> {
    if bytes.len() > limit {
        return Err(err(
            ErrorKind::Limit,
            "toolchain metadata byte limit exceeded",
        ));
    }
    serde_json::from_slice(bytes).map_err(|_| format("invalid or unsupported toolchain record"))
}
fn repository(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Stable,
    Beta,
    Nightly,
}
impl Channel {
    pub fn text(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Nightly => "nightly",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Target {
    #[serde(rename = "aarch64-apple-darwin")]
    Macos,
    #[serde(rename = "x86_64-pc-windows-msvc")]
    Windows,
}
impl Target {
    pub fn text(self) -> &'static str {
        match self {
            Self::Macos => "aarch64-apple-darwin",
            Self::Windows => "x86_64-pc-windows-msvc",
        }
    }
    pub fn compiler(self) -> &'static str {
        match self {
            Self::Macos => "bin/lang",
            Self::Windows => "bin/lang.exe",
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RootRecord {
    schema: u32,
    repository: String,
    expires: u64,
    threshold: usize,
    keys: Vec<KeyRecord>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyRecord {
    id: String,
    public_key: String,
}

/// An explicitly supplied, structurally validated trust root; never fetched implicitly.
pub struct TrustedRoot {
    identity: String,
    repository: String,
    expires: u64,
    threshold: usize,
    keys: BTreeMap<String, VerifyingKey>,
}
impl TrustedRoot {
    pub fn decode(bytes: &[u8], now: u64) -> Result<Self, Error> {
        let root: RootRecord = decode(bytes, MAX_ROOT_BYTES)?;
        if root.schema != 1 || !repository(&root.repository) || root.expires > MAX_TIME {
            return Err(format("invalid trust root identity"));
        }
        if now > MAX_TIME {
            return Err(err(ErrorKind::Clock, "unsupported host clock"));
        }
        if root.expires <= now {
            return Err(err(ErrorKind::Expired, "trust root expired"));
        }
        if root.keys.is_empty()
            || root.keys.len() > 8
            || root.threshold == 0
            || root.threshold > root.keys.len()
        {
            return Err(err(
                ErrorKind::Limit,
                "invalid trust key count or threshold",
            ));
        }
        let mut keys = BTreeMap::new();
        for key in root.keys {
            let bytes = hex::<32>(&key.public_key)?;
            if key.id != digest(&bytes) {
                return Err(err(
                    ErrorKind::Identity,
                    "trust key ID does not match public bytes",
                ));
            }
            let public = VerifyingKey::from_bytes(&bytes)
                .map_err(|_| format("invalid Ed25519 public key"))?;
            if public.is_weak() || keys.insert(key.id, public).is_some() {
                return Err(err(ErrorKind::Signature, "weak or duplicate trust key"));
            }
        }
        Ok(Self {
            identity: digest(bytes),
            repository: root.repository,
            expires: root.expires,
            threshold: root.threshold,
            keys,
        })
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    pub fn repository(&self) -> &str {
        &self.repository
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: u32,
    payload: String,
    signatures: Vec<SignedBy>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedBy {
    key_id: String,
    signature: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Payload {
    pub schema: u32,
    pub repository: String,
    pub channel: Channel,
    pub target: Target,
    pub sequence: u64,
    pub issued: u64,
    pub expires: u64,
    pub current_version: String,
    pub releases: Vec<Release>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub version: String,
    pub compiler_build_id: String,
    pub files: Vec<File>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub executable: bool,
}
fn path(value: &str) -> bool {
    if value.is_empty() || value.len() > 240 {
        return false;
    }
    value.split('/').all(|part| {
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with('.')
            || !part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
        {
            return false;
        }
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        !matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            && !(stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && (b'1'..=b'9').contains(&stem.as_bytes()[3]))
    })
}
impl Release {
    fn validate(&self, target: Target) -> Result<(), Error> {
        if !crate::package::manifest::valid_version(&self.version) {
            return Err(format("invalid release version"));
        }
        hex::<32>(&self.compiler_build_id)?;
        if self.files.is_empty() || self.files.len() > 4096 {
            return Err(err(ErrorKind::Limit, "invalid release file count"));
        }
        let sdk_compiler = format!("sdk/{}", target.compiler());
        let mut names = BTreeSet::new();
        let mut total = 0_u64;
        for file in &self.files {
            if !path(&file.path) || !names.insert(file.path.to_ascii_lowercase()) {
                return Err(format("invalid or duplicate release file path"));
            }
            hex::<32>(&file.sha256)?;
            total = total
                .checked_add(file.bytes)
                .filter(|n| *n <= MAX_RELEASE_BYTES)
                .ok_or(err(ErrorKind::Limit, "release payload byte limit exceeded"))?;
            if file.executable != (file.path == target.compiler() || file.path == sdk_compiler) {
                return Err(format("invalid release executable designation"));
            }
        }
        for name in &names {
            for (index, _) in name.match_indices('/') {
                if names.contains(&name[..index]) {
                    return Err(format("release file/directory path collision"));
                }
            }
        }
        let compiler = self
            .files
            .iter()
            .find(|f| f.path == target.compiler())
            .ok_or(format("release lacks its compiler"))?;
        let sdk = self
            .files
            .iter()
            .find(|f| f.path == sdk_compiler)
            .ok_or(format("release lacks its SDK compiler"))?;
        if compiler.sha256 != sdk.sha256
            || compiler.bytes != sdk.bytes
            || compiler.bytes == 0
            || !self.files.iter().any(|f| f.path == "sdk/sdk.json")
        {
            return Err(err(
                ErrorKind::Identity,
                "release compiler and SDK inputs do not reconcile",
            ));
        }
        Ok(())
    }
    pub fn identity(&self) -> Result<String, Error> {
        serde_json::to_vec(self)
            .map(|bytes| digest(&bytes))
            .map_err(|_| format("release identity encoding failed"))
    }
}

/// Prior authenticated admission, scoped to a root/channel/target. No public
/// constructor permits an unverified envelope to supply a rollback floor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Admission {
    root: String,
    channel: Channel,
    target: Target,
    sequence: u64,
    payload_digest: String,
    time_floor: u64,
}
impl Admission {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn payload_digest(&self) -> &str {
        &self.payload_digest
    }
    pub fn time_floor(&self) -> u64 {
        self.time_floor
    }
}
pub struct Request<'a> {
    pub channel: Channel,
    pub target: Target,
    pub now: u64,
    pub previous: Option<&'a Admission>,
}
/// Only successful strict verification can construct this input authority.
pub struct Admitted {
    payload: Payload,
    admission: Admission,
}
impl Admitted {
    pub fn payload(&self) -> &Payload {
        &self.payload
    }
    pub fn admission(&self) -> &Admission {
        &self.admission
    }
    pub fn release(&self, version: Option<&str>) -> Result<&Release, Error> {
        let version = version.unwrap_or(&self.payload.current_version);
        self.payload
            .releases
            .iter()
            .find(|r| r.version == version)
            .ok_or(err(
                ErrorKind::Identity,
                "version is absent from the admitted catalogue",
            ))
    }
}
pub fn verify(root: &TrustedRoot, bytes: &[u8], request: Request<'_>) -> Result<Admitted, Error> {
    if request.now > MAX_TIME {
        return Err(err(ErrorKind::Clock, "unsupported host clock"));
    }
    if root.expires <= request.now {
        return Err(err(ErrorKind::Expired, "trust root expired"));
    }
    let envelope: Envelope = decode(bytes, MAX_METADATA_BYTES)?;
    if envelope.schema != 1 {
        return Err(format("unsupported signed envelope schema"));
    }
    if envelope.signatures.len() < root.threshold || envelope.signatures.len() > 8 {
        return Err(err(
            ErrorKind::Signature,
            "signature threshold or count refused",
        ));
    }
    let mut signed = Vec::with_capacity(SIGNING_DOMAIN.len() + envelope.payload.len());
    signed.extend_from_slice(SIGNING_DOMAIN);
    signed.extend_from_slice(envelope.payload.as_bytes());
    let mut signers = BTreeSet::new();
    for signature in envelope.signatures {
        let key = root
            .keys
            .get(&signature.key_id)
            .ok_or(err(ErrorKind::Signature, "unknown release signer"))?;
        if !signers.insert(signature.key_id) {
            return Err(err(ErrorKind::Signature, "duplicate release signer"));
        }
        let signature = Signature::from_bytes(&hex::<64>(&signature.signature)?);
        key.verify_strict(&signed, &signature)
            .map_err(|_| err(ErrorKind::Signature, "invalid release signature"))?;
    }
    let payload: Payload = decode(envelope.payload.as_bytes(), MAX_METADATA_BYTES)?;
    if payload.schema != 1
        || payload.sequence == 0
        || payload.issued > MAX_TIME
        || payload.expires > MAX_TIME
    {
        return Err(format("invalid release metadata identity"));
    }
    if payload.repository != root.repository
        || payload.channel != request.channel
        || payload.target != request.target
    {
        return Err(err(
            ErrorKind::Identity,
            "repository, channel or target mismatch",
        ));
    }
    if payload.issued > request.now {
        return Err(err(ErrorKind::Clock, "release metadata is from the future"));
    }
    if payload.expires <= request.now {
        return Err(err(ErrorKind::Expired, "release metadata expired"));
    }
    if payload
        .expires
        .checked_sub(payload.issued)
        .is_none_or(|n| n > 90 * 24 * 60 * 60)
    {
        return Err(format("release metadata lifetime exceeds 90 days"));
    }
    if payload.releases.is_empty() || payload.releases.len() > 64 {
        return Err(err(ErrorKind::Limit, "invalid release catalogue count"));
    }
    let mut versions = BTreeSet::new();
    let mut files = 0_usize;
    for release in &payload.releases {
        release.validate(payload.target)?;
        files = files
            .checked_add(release.files.len())
            .filter(|n| *n <= 4096)
            .ok_or(err(ErrorKind::Limit, "catalogue file count exceeded"))?;
        if !versions.insert(&release.version) {
            return Err(format("duplicate release version"));
        }
    }
    if !versions.contains(&payload.current_version) {
        return Err(format("current version is absent from catalogue"));
    }
    let payload_digest = digest(envelope.payload.as_bytes());
    if let Some(prior) = request.previous {
        if prior.root != root.identity
            || prior.channel != payload.channel
            || prior.target != payload.target
        {
            return Err(err(
                ErrorKind::Identity,
                "prior admission belongs to another trust context",
            ));
        }
        if request.now < prior.time_floor {
            return Err(err(
                ErrorKind::Clock,
                "clock moved behind the admission floor",
            ));
        }
        if payload.sequence < prior.sequence
            || (payload.sequence == prior.sequence && payload_digest != prior.payload_digest)
        {
            return Err(err(
                ErrorKind::Rollback,
                "release metadata rolls back or changes an admitted sequence",
            ));
        }
    }
    let admission = Admission {
        root: root.identity.clone(),
        channel: payload.channel,
        target: payload.target,
        sequence: payload.sequence,
        payload_digest,
        time_floor: request.now,
    };
    Ok(Admitted { payload, admission })
}

/// Verify an already admitted file's exact bytes without publishing anything.
pub fn verify_file(file: &File, bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() as u64 != file.bytes
        || file.bytes > MAX_RELEASE_BYTES
        || digest(bytes) != file.sha256
    {
        return Err(err(
            ErrorKind::Payload,
            "release payload length or digest mismatch",
        ));
    }
    Ok(())
}
