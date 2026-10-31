use super::*;
use serde::{Deserialize, Deserializer, Serialize};
use std::io::Write;

pub(super) const MAX_STATE: usize = 16 * 1024 * 1024;
pub(super) const MAX_AUDIT: usize = 2048;
pub(super) const MAX_HEAD: usize = 128;
pub(super) const MAX_EVENTS: u64 = 65_536;
pub(super) const MAX_RELEASES: usize = 8192;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Head {
    pub schema: u32,
    pub state_sha256: String,
}

#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct State {
    pub format: String,
    pub schema: u32,
    pub registry: String,
    pub origin: String,
    pub revision: u64,
    pub time_floor: u64,
    pub authority: authority::codec::Records,
    #[serde(deserialize_with = "packages")]
    pub packages: Vec<Package>,
    pub last_audit_sha256: String,
}

#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Package {
    pub name: String,
    pub revision: u64,
    #[serde(deserialize_with = "releases")]
    pub releases: Vec<Release>,
}

#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Release {
    pub version: String,
    pub archive_sha256: String,
    pub archive_bytes: u64,
    pub descriptor_sha256: String,
    pub edition: String,
    #[serde(deserialize_with = "dependencies")]
    pub dependencies: Vec<archive::Dependency>,
    pub yanked: bool,
    pub quarantined: bool,
}
impl Release {
    pub fn from_archive(a: &AdmittedArchive) -> Self {
        Self {
            version: a.descriptor().package().version().into(),
            archive_sha256: a.sha256().into(),
            archive_bytes: a.as_bytes().len() as u64,
            descriptor_sha256: a.descriptor_sha256().into(),
            edition: a.descriptor().package().edition().into(),
            dependencies: a.descriptor().dependencies().to_vec(),
            yanked: false,
            quarantined: false,
        }
    }
    // A bounded fingerprint of all metadata, not just the archive hash. This
    // lets the history walker validate bindings without retaining source bodies.
    pub fn binding(&self, name: &str) -> Result<String, Error> {
        Ok(digest(&encode(
            &(name, self),
            archive::MAX_DESCRIPTOR_BYTES,
        )?))
    }
}

#[derive(Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum AuditOperation {
    Initialize,
    Publish {
        name: String,
        version: String,
        archive_sha256: String,
    },
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct InitialResult {
    pub state_revision: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct PublishResult {
    pub state_revision: u64,
    pub package_revision: u64,
    pub name: String,
    pub version: String,
    pub archive_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(untagged)]
pub(super) enum AuditResult {
    Initialize(InitialResult),
    Publish(PublishResult),
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Audit {
    pub format: String,
    pub schema: u32,
    pub sequence: u64,
    #[serde(deserialize_with = "required_option")]
    pub previous_audit_sha256: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub prior_state_sha256: Option<String>,
    pub time: u64,
    pub principal: String,
    #[serde(deserialize_with = "required_option")]
    pub credential_id: Option<String>,
    pub operation: AuditOperation,
    #[serde(deserialize_with = "required_option")]
    pub request_id: Option<String>,
    #[serde(deserialize_with = "required_option")]
    pub request_sha256: Option<String>,
    pub reason: String,
    pub result: AuditResult,
}

fn required_option<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    Option::deserialize(d)
}
fn packages<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Package>, D::Error> {
    authority::codec::bounded::<D, _, 1024>(d)
}
fn releases<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Release>, D::Error> {
    authority::codec::bounded::<D, _, 128>(d)
}
fn dependencies<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<archive::Dependency>, D::Error> {
    authority::codec::bounded::<D, _, 128>(d)
}

pub(super) fn hex<const N: usize>(s: &str) -> Result<(), Error> {
    authority::codec::unhex::<N>(s)
        .map(|_| ())
        .map_err(|_| corrupt())
}
pub(super) fn version(s: &str) -> Result<[u32; 3], Error> {
    if s.len() > 32 || !crate::package::manifest::valid_version(s) {
        return Err(corrupt());
    }
    let mut result = [0; 3];
    for (out, part) in result.iter_mut().zip(s.split('.')) {
        *out = part.parse().map_err(|_| corrupt())?;
    }
    Ok(result)
}
pub(super) fn name(s: &str) -> Result<(), Error> {
    if !crate::package::manifest::valid_name(s) {
        return Err(corrupt());
    }
    Ok(())
}
impl State {
    pub fn validate(&self, identity: &ServiceIdentity) -> Result<(), Error> {
        if self.format != "lang-registry-state"
            || self.schema != 1
            || self.registry != identity.registry()
            || self.origin != identity.origin()
            || self.revision == 0
            || self.revision > MAX_EVENTS
            || self.time_floor > authority::MAX_TIME
            || self.authority.time_floor != self.time_floor
            || self.authority.registry != self.registry
            || self.authority.origin != self.origin
            || self.packages.len() > 1024
            || self.packages.len() != self.authority.packages.len()
        {
            return Err(corrupt());
        }
        authority::codec::validate(&self.authority).map_err(|_| corrupt())?;
        hex::<32>(&self.last_audit_sha256)?;
        let mut total = 0usize;
        for (p, owners) in self.packages.iter().zip(&self.authority.packages) {
            if p.name != owners.name
                || p.revision == 0
                || p.revision > self.revision
                || p.releases.is_empty()
                || p.releases.len() > 128
            {
                return Err(corrupt());
            }
            total = total.checked_add(p.releases.len()).ok_or_else(capacity)?;
            if total > MAX_RELEASES {
                return Err(capacity());
            }
            let mut previous = None;
            for r in &p.releases {
                let v = version(&r.version)?;
                if previous.is_some_and(|prev| prev >= v) {
                    return Err(corrupt());
                }
                previous = Some(v);
                hex::<32>(&r.archive_sha256)?;
                hex::<32>(&r.descriptor_sha256)?;
                if r.archive_bytes > archive::MAX_ARCHIVE_BYTES as u64
                    || r.dependencies.len() > 128
                    || r.yanked
                    || r.quarantined
                {
                    return Err(corrupt());
                }
            }
        }
        Ok(())
    }
    pub fn package(&self, name: &str) -> Result<usize, usize> {
        self.packages
            .binary_search_by(|p| p.name.as_str().cmp(name))
    }
    pub fn operation<'a>(&self, name: &'a str) -> Operation<'a> {
        if self.package(name).is_ok() {
            Operation::Publish(name)
        } else {
            Operation::Claim(name)
        }
    }
}
impl Audit {
    pub fn validate(&self) -> Result<(), Error> {
        if self.format != "lang-registry-audit"
            || self.schema != 1
            || self.sequence == 0
            || self.sequence > MAX_EVENTS
            || self.time > authority::MAX_TIME
        {
            return Err(corrupt());
        }
        hex::<16>(&self.principal)?;
        match (&self.operation, &self.result) {
            (AuditOperation::Initialize, AuditResult::Initialize(r)) => {
                if self.sequence != 1
                    || r.state_revision != 1
                    || self.reason != "initialize"
                    || self.previous_audit_sha256.is_some()
                    || self.prior_state_sha256.is_some()
                    || self.credential_id.is_some()
                    || self.request_id.is_some()
                    || self.request_sha256.is_some()
                {
                    return Err(corrupt());
                }
            }
            (
                AuditOperation::Publish {
                    name: n,
                    version: v,
                    archive_sha256: h,
                },
                AuditResult::Publish(r),
            ) => {
                name(n)?;
                version(v)?;
                hex::<32>(h)?;
                hex::<32>(self.previous_audit_sha256.as_deref().ok_or_else(corrupt)?)?;
                hex::<32>(self.prior_state_sha256.as_deref().ok_or_else(corrupt)?)?;
                hex::<16>(self.credential_id.as_deref().ok_or_else(corrupt)?)?;
                hex::<16>(self.request_id.as_deref().ok_or_else(corrupt)?)?;
                hex::<32>(self.request_sha256.as_deref().ok_or_else(corrupt)?)?;
                if self.sequence <= 1
                    || r.state_revision != self.sequence
                    || r.package_revision == 0
                    || r.package_revision >= self.sequence
                    || r.name != *n
                    || r.version != *v
                    || r.archive_sha256 != *h
                    || self.reason != "publish"
                {
                    return Err(corrupt());
                }
            }
            _ => return Err(corrupt()),
        }
        Ok(())
    }
}

struct Writer {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("record limit"));
        }
        self.bytes
            .try_reserve(bytes.len())
            .map_err(std::io::Error::other)?;
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, Error> {
    let mut writer = Writer {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer_pretty(&mut writer, value).map_err(|_| capacity())?;
    writer.write_all(b"\n").map_err(|_| capacity())?;
    Ok(writer.bytes)
}
pub(super) fn decode<T: for<'de> Deserialize<'de> + Serialize>(
    bytes: &[u8],
    limit: usize,
) -> Result<T, Error> {
    if bytes.len() > limit {
        return Err(capacity());
    }
    let (mut depth, mut string, mut escape) = (0usize, false, false);
    for &b in bytes {
        if string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                string = false;
            }
        } else {
            match b {
                b'"' => string = true,
                b'[' | b'{' => {
                    depth += 1;
                    if depth > 16 {
                        return Err(capacity());
                    }
                }
                b']' | b'}' => depth = depth.checked_sub(1).ok_or_else(corrupt)?,
                _ => (),
            }
        }
    }
    let value: T = serde_json::from_slice(bytes).map_err(|_| corrupt())?;
    if bytes != encode(&value, limit)? {
        return Err(corrupt());
    }
    Ok(value)
}

pub(super) fn request_digest(
    name: &str,
    version: &str,
    condition: ExpectedRevision,
    hash: &str,
    bytes: u64,
) -> Result<String, Error> {
    #[derive(Serialize)]
    struct Request<'a> {
        schema: u32,
        method: &'a str,
        path: String,
        conditional_revision: Option<u64>,
        body_sha256: &'a str,
        body_bytes: u64,
    }
    let request = Request {
        schema: 1,
        method: "PUT",
        path: format!("/v1/packages/{name}/{version}"),
        conditional_revision: match condition {
            ExpectedRevision::Absent => None,
            ExpectedRevision::Revision(r) => Some(r),
        },
        body_sha256: hash,
        body_bytes: bytes,
    };
    Ok(digest(&encode(&request, MAX_AUDIT)?))
}
