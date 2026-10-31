//! RFC 0050 byte cache. It never constructs authenticated consumption evidence.
use super::{Binding, ConsumedArchive, Request};
use crate::package::lock::{
    Lock as PackageLock,
    registry::cache::{Locator, ObjectSummary, Reference, references},
};
use crate::registry::archive::AdmittedArchive;
pub use crate::registry::store::{Error, ErrorKind, PublicationOutcome};
use cap_std::fs::Dir;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

mod storage;
#[cfg(test)]
mod tests;

const FORMAT: &[u8] = b"{\n  \"format\": \"lang-registry-cache\",\n  \"schema\": 1\n}\n";
const MAX_GENERATION_ARCHIVES: usize = 64 * 1024 * 1024;

fn error(kind: ErrorKind, message: &'static str) -> Error {
    Error {
        kind,
        message,
        outcome: PublicationOutcome::NotPublished,
    }
}
fn capacity() -> Error {
    error(ErrorKind::Limit, "registry cache capacity exceeded")
}
fn corrupt() -> Error {
    error(
        ErrorKind::State,
        "invalid registry cache; verified recovery required",
    )
}
fn integrity() -> Error {
    error(
        ErrorKind::Integrity,
        "registry cache input identity mismatch",
    )
}
fn conflict() -> Error {
    error(
        ErrorKind::Conflict,
        "immutable cached registry version conflict",
    )
}
fn io() -> Error {
    error(ErrorKind::Io, "registry cache operation failed")
}
fn digest(bytes: &[u8]) -> String {
    super::hex(&Sha256::digest(bytes))
}
fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn checkpoint(_stage: &'static str) {
    #[cfg(test)]
    tests::checkpoint(_stage);
}
fn check_generation_bytes(references: &[Reference]) -> Result<(), Error> {
    references.iter().try_fold(0usize, |n, r| {
        n.checked_add(r.archive_bytes)
            .filter(|n| *n <= MAX_GENERATION_ARCHIVES)
            .ok_or_else(capacity)
    })?;
    Ok(())
}

/// Explicit write/recovery authority. The nonblocking exclusive lock is retained.
pub struct Cache {
    storage: storage::Storage,
}
impl Cache {
    pub fn initialize(parent: &Dir, child: &str) -> Result<Self, Error> {
        let storage = storage::Storage::create(parent, child)?;
        storage.inventory()?;
        Ok(Self { storage })
    }
    pub fn open(parent: &Dir, child: &str) -> Result<Self, Error> {
        let storage = storage::Storage::open(parent, child, true)?;
        let view = storage.inventory()?;
        storage.cleanup(&view)?;
        Ok(Self { storage })
    }
    /// All inputs must be sealed evidence for exactly this captured lock.
    /// Returns the complete generation digest after root-lock publication.
    pub fn publish(
        &self,
        lock: &PackageLock,
        inputs: &[&ConsumedArchive],
        root_output: &Dir,
    ) -> Result<String, Error> {
        self.storage.require_external_output(root_output)?;
        let refs = references(lock.as_bytes()).map_err(|_| integrity())?;
        check_generation_bytes(&refs)?;
        if refs.len() != inputs.len() {
            return Err(integrity());
        }
        let mut supplied = BTreeMap::new();
        for input in inputs {
            let package = input.archive().descriptor().package();
            let locator = Locator {
                policy: input.binding().policy_id().to_owned(),
                name: package.name().to_owned(),
                version: package.version().to_owned(),
            };
            if supplied.insert(locator, *input).is_some() {
                return Err(integrity());
            }
        }
        // Recovery is bounded and must validate everything before any cleanup.
        let previous = self.storage.inventory()?;
        self.storage.cleanup(&previous)?;
        let mut view = self.storage.inventory()?;
        let mut objects = BTreeMap::new();
        for reference in &refs {
            let input = supplied.get(&reference.locator).ok_or_else(integrity)?;
            let archive = input.archive();
            if input.binding() != &reference.binding
                || archive.sha256() != reference.archive_sha256
                || !reference.matches(&ObjectSummary::capture(archive).map_err(|_| integrity())?)
            {
                return Err(integrity());
            }
            view.bind(reference)?;
            objects.insert(archive.sha256(), archive);
        }
        let generation = digest(lock.as_bytes());
        let mut additional_bytes = 0u64;
        let mut additional_entries = 0usize;
        for (hash, archive) in &objects {
            if !view.objects.contains_key(*hash) {
                additional_bytes = additional_bytes
                    .checked_add(archive.as_bytes().len() as u64)
                    .ok_or_else(capacity)?;
                additional_entries = additional_entries.checked_add(1).ok_or_else(capacity)?;
            }
        }
        if !view.generations.contains(&generation) {
            additional_bytes = additional_bytes
                .checked_add(lock.as_bytes().len() as u64)
                .ok_or_else(capacity)?;
            additional_entries = additional_entries.checked_add(1).ok_or_else(capacity)?;
        }
        view.reserve(additional_bytes, additional_entries)?;
        for (hash, archive) in objects {
            self.storage
                .install(storage::Kind::Object, hash, archive.as_bytes())?;
        }
        self.storage
            .install(storage::Kind::Generation, &generation, lock.as_bytes())?;
        checkpoint("before-root-publication");
        let replacing = std::cell::Cell::new(false);
        lock.publish_before_replace(root_output, || replacing.set(true))
            .map_err(|_| Error {
                outcome: if replacing.get() {
                    PublicationOutcome::Unknown
                } else {
                    PublicationOutcome::NotPublished
                },
                ..io()
            })?;
        checkpoint("after-root-publication");
        #[cfg(test)]
        tests::after_root()?;
        Ok(generation)
    }
}

struct Input {
    binding: Binding,
    archive: Arc<AdmittedArchive>,
}

/// Owned immutable bytes only. A verifier must independently grant consumption.
pub struct Captured {
    lock: Vec<u8>,
    inputs: BTreeMap<Locator, Input>,
}
impl Captured {
    /// Read-only and local-only: no cleanup, network or credential-provider calls.
    pub fn open(parent: &Dir, child: &str, lock: &[u8]) -> Result<Self, Error> {
        let refs = references(lock).map_err(|_| integrity())?;
        check_generation_bytes(&refs)?;
        let storage = storage::Storage::open(parent, child, false)?;
        let view = storage.inventory()?;
        let generation = digest(lock);
        if !view.generations.contains(&generation)
            || storage.read(storage::Kind::Generation, &generation)? != lock
        {
            return Err(corrupt());
        }
        let mut objects: BTreeMap<String, Arc<AdmittedArchive>> = BTreeMap::new();
        let mut inputs = BTreeMap::new();
        for reference in refs {
            let archive = match objects.get(&reference.archive_sha256) {
                Some(archive) => Arc::clone(archive),
                None => {
                    let bytes = storage.read(storage::Kind::Object, &reference.archive_sha256)?;
                    let archive =
                        Arc::new(AdmittedArchive::admit(&bytes).map_err(|_| integrity())?);
                    objects.insert(reference.archive_sha256.clone(), Arc::clone(&archive));
                    archive
                }
            };
            if !reference.matches(&ObjectSummary::capture(&archive).map_err(|_| integrity())?) {
                return Err(integrity());
            }
            if inputs
                .insert(
                    reference.locator,
                    Input {
                        binding: reference.binding,
                        archive,
                    },
                )
                .is_some()
            {
                return Err(corrupt());
            }
        }
        Ok(Self {
            lock: lock.to_vec(),
            inputs,
        })
    }
    pub fn lock_bytes(&self) -> &[u8] {
        &self.lock
    }
    pub fn archive(&self, request: Request<'_>) -> Result<&AdmittedArchive, Error> {
        let key = Locator {
            policy: request.binding.policy_id().to_owned(),
            name: request.name.to_owned(),
            version: request.version.to_owned(),
        };
        let input = self.inputs.get(&key).ok_or_else(integrity)?;
        if &input.binding != request.binding
            || request
                .archive_sha256
                .is_some_and(|h| h != input.archive.sha256())
        {
            return Err(integrity());
        }
        Ok(&input.archive)
    }
}
