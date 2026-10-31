use super::*;
use crate::package::compatibility::SourceEdition;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: String,
    schema: u32,
    registry: String,
    name: String,
    revision: u64,
    #[serde(deserialize_with = "releases")]
    releases: Vec<records::Release>,
}

fn releases<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<records::Release>, D::Error> {
    authority::codec::bounded::<D, _, 128>(d)
}

fn malformed_index() -> Error {
    error(ErrorKind::Malformed, "invalid registry index")
}

fn binding_error() -> Error {
    error(
        ErrorKind::Integrity,
        "registry release identity or digest mismatch",
    )
}

/// A bounded canonical index associated with an explicitly selected service.
/// This proves encoding and metadata consistency only. It supplies no signature,
/// freshness, network-origin proof, filesystem or execution authority.
pub struct Index {
    identity: ServiceIdentity,
    record: Record,
}

impl Index {
    pub fn admit(bytes: &[u8], expected: &ServiceIdentity, name: &str) -> Result<Self, Error> {
        records::name(name).map_err(|_| malformed_index())?;
        let record: Record = records::decode(bytes, MAX_INDEX_BYTES).map_err(|e| {
            if e.kind == ErrorKind::Limit {
                e
            } else {
                malformed_index()
            }
        })?;
        if record.format != "lang-registry-index"
            || record.schema != 1
            || record.revision == 0
            || record.releases.is_empty()
        {
            return Err(malformed_index());
        }
        if record.registry != expected.registry() || record.name != name {
            return Err(binding_error());
        }
        let mut previous = None;
        for release in &record.releases {
            let version = records::version(&release.version).map_err(|_| malformed_index())?;
            if previous.is_some_and(|prior| prior >= version)
                || release.archive_bytes == 0
                || SourceEdition::parse(&release.edition).is_err()
            {
                return Err(malformed_index());
            }
            if release.archive_bytes > archive::MAX_ARCHIVE_BYTES as u64 {
                return Err(capacity());
            }
            previous = Some(version);
            records::hex::<32>(&release.archive_sha256).map_err(|_| malformed_index())?;
            records::hex::<32>(&release.descriptor_sha256).map_err(|_| malformed_index())?;
            archive::validate_dependencies(&release.dependencies).map_err(|_| malformed_index())?;
        }
        Ok(Self {
            identity: expected.clone(),
            record,
        })
    }

    pub fn identity(&self) -> &ServiceIdentity {
        &self.identity
    }
    pub fn name(&self) -> &str {
        &self.record.name
    }
    pub fn revision(&self) -> u64 {
        self.record.revision
    }
    pub fn as_bytes(&self) -> Result<Vec<u8>, Error> {
        records::encode(&self.record, MAX_INDEX_BYTES)
    }
    pub fn releases(&self) -> impl ExactSizeIterator<Item = IndexedRelease<'_>> {
        self.record.releases.iter().map(|record| IndexedRelease {
            index: self,
            record,
        })
    }
    pub fn release(&self, version: &str) -> Option<IndexedRelease<'_>> {
        let wanted = records::version(version).ok()?;
        let index = self
            .record
            .releases
            .binary_search_by_key(&wanted, |r| {
                records::version(&r.version).expect("admitted immutable version")
            })
            .ok()?;
        Some(IndexedRelease {
            index: self,
            record: &self.record.releases[index],
        })
    }
}

/// An exact release borrowed from a validated index, without signing evidence.
pub struct IndexedRelease<'a> {
    index: &'a Index,
    record: &'a records::Release,
}

impl IndexedRelease<'_> {
    pub fn version(&self) -> &str {
        &self.record.version
    }
    pub fn archive_sha256(&self) -> &str {
        &self.record.archive_sha256
    }
    pub fn archive_bytes(&self) -> u64 {
        self.record.archive_bytes
    }
    pub fn descriptor_sha256(&self) -> &str {
        &self.record.descriptor_sha256
    }
    pub fn edition(&self) -> &str {
        &self.record.edition
    }
    pub fn dependencies(&self) -> &[archive::Dependency] {
        &self.record.dependencies
    }
    pub fn yanked(&self) -> bool {
        self.record.yanked
    }
    pub fn quarantined(&self) -> bool {
        self.record.quarantined
    }

    /// Verify immutable archive identity and every duplicated metadata field.
    /// Production consumption additionally requires independent trust/freshness.
    pub fn admit_archive(&self, bytes: &[u8]) -> Result<AdmittedArchive, Error> {
        if self.quarantined() {
            return Err(error(
                ErrorKind::NotFound,
                "registry resource is unavailable",
            ));
        }
        if bytes.len() as u64 != self.archive_bytes() || digest(bytes) != self.archive_sha256() {
            return Err(binding_error());
        }
        let archive = AdmittedArchive::admit(bytes).map_err(|_| binding_error())?;
        let mut metadata = records::Release::from_archive(&archive);
        metadata.yanked = self.yanked();
        metadata.quarantined = self.quarantined();
        if archive.descriptor().package().name() != self.index.name() || metadata != *self.record {
            return Err(binding_error());
        }
        Ok(archive)
    }
}
