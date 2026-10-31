use super::*;

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
