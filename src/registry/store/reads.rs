use super::*;
use std::collections::BTreeMap;

pub const MAX_INDEX_BYTES: usize = 2 * 1024 * 1024;

#[derive(Serialize)]
struct IndexRecord<'a> {
    format: &'static str,
    schema: u32,
    registry: &'a str,
    name: &'a str,
    revision: u64,
    releases: &'a [records::Release],
}

pub(super) fn encode_package(registry: &str, package: &records::Package) -> Result<Vec<u8>, Error> {
    records::encode(
        &IndexRecord {
            format: "lang-registry-index",
            schema: 1,
            registry,
            name: &package.name,
            revision: package.revision,
            releases: &package.releases,
        },
        MAX_INDEX_BYTES,
    )
}

fn unavailable() -> Error {
    error(ErrorKind::NotFound, "registry resource is unavailable")
}

/// A read-only service retains the exclusive registry lock and one fully
/// validated generation. No mutable authority or replay receipts are cached.
pub(crate) struct CapturedReads {
    registry: Registry,
    head: String,
    indexes: BTreeMap<String, Option<Box<[u8]>>>,
    archives: BTreeMap<String, usize>,
    memory: usize,
}

impl CapturedReads {
    pub(crate) fn capture(registry: Registry) -> Result<Self, Error> {
        let view = history::load(&registry.storage, &registry.identity)?;
        let mut indexes = BTreeMap::new();
        let mut archives = BTreeMap::new();
        let mut memory = 4096usize;
        for package in &view.state.packages {
            let bytes = match encode_package(registry.identity.registry(), package) {
                Ok(bytes) => Some(bytes.into_boxed_slice()),
                Err(e) if e.kind == ErrorKind::Limit => None,
                Err(e) => return Err(e),
            };
            // A conservative fixed allowance covers each bounded key, map
            // node, value and allocation rounding, in addition to exact data.
            memory = memory
                .checked_add(1024 + bytes.as_ref().map_or(0, |b| b.len()))
                .ok_or_else(capacity)?;
            indexes.insert(package.name.clone(), bytes);
            for release in &package.releases {
                if !release.quarantined {
                    let length = usize::try_from(release.archive_bytes).map_err(|_| capacity())?;
                    if let Some(prior) = archives.insert(release.archive_sha256.clone(), length) {
                        if prior != length {
                            return Err(corrupt());
                        }
                    } else {
                        memory = memory.checked_add(1024).ok_or_else(capacity)?;
                    }
                }
            }
        }
        Ok(Self {
            registry,
            head: view.head,
            indexes,
            archives,
            memory,
        })
    }

    pub(crate) fn identity(&self) -> &ServiceIdentity {
        &self.registry.identity
    }
    pub(crate) fn memory_bytes(&self) -> usize {
        self.memory
    }
    pub(crate) fn check_head(&self) -> Result<(), Error> {
        if self.registry.storage.head()? != self.head {
            return Err(corrupt());
        }
        Ok(())
    }
    pub(crate) fn index(&self, name: &str) -> Result<&[u8], Error> {
        self.check_head()?;
        self.indexes
            .get(name)
            .ok_or_else(unavailable)?
            .as_deref()
            .ok_or_else(capacity)
    }
    pub(crate) fn archive_length(&self, hash: &str) -> Result<usize, Error> {
        self.check_head()?;
        self.archives.get(hash).copied().ok_or_else(unavailable)
    }
    pub(crate) fn archive(&self, hash: &str) -> Result<Vec<u8>, Error> {
        let length = self.archive_length(hash)?;
        self.registry.storage.captured_object(hash, length)
    }
}

impl Registry {
    /// Canonical bounded index from one completely validated committed state.
    /// These bytes carry integrity metadata, not signing or execution authority.
    pub fn index(&self, name: &str) -> Result<Vec<u8>, Error> {
        records::name(name)
            .map_err(|_| error(ErrorKind::Malformed, "invalid registry package name"))?;
        let view = history::load(&self.storage, &self.identity)?;
        let package = &view.state.packages[view.state.package(name).map_err(|_| unavailable())?];
        encode_package(self.identity.registry(), package)
    }

    /// Exact admitted bytes referenced by a committed, non-quarantined release.
    /// Yanked releases remain available for authenticated exact-lock replay.
    pub fn archive(&self, sha256: &str) -> Result<AdmittedArchive, Error> {
        records::hex::<32>(sha256)
            .map_err(|_| error(ErrorKind::Malformed, "invalid registry archive digest"))?;
        let view = history::load(&self.storage, &self.identity)?;
        let served = view.state.packages.iter().any(|package| {
            package
                .releases
                .iter()
                .any(|release| release.archive_sha256 == sha256 && !release.quarantined)
        });
        if !served {
            return Err(unavailable());
        }
        let bytes = self.storage.immutable(storage::Space::Object, sha256)?;
        AdmittedArchive::admit(&bytes).map_err(|_| corrupt())
    }
}
