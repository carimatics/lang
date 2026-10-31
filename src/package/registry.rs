//! Explicit host bindings and sealed package-consumption evidence for RFC 0049.
use super::{Diagnostic, manifest};
use crate::registry::{archive::AdmittedArchive, authority::ServiceIdentity};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub mod cache;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Binding {
    identity: ServiceIdentity,
    root_sha256: [u8; 32],
    policy_id: String,
}

impl Binding {
    /// The digest identifies host policy; supplying it is not authentication.
    pub fn new(identity: ServiceIdentity, root_sha256: [u8; 32]) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"LANGREGISTRYPOLICY1\0");
        for field in [identity.registry(), identity.origin(), &hex(&root_sha256)] {
            hash.update((field.len() as u64).to_le_bytes());
            hash.update(field.as_bytes());
        }
        Self {
            identity,
            root_sha256,
            policy_id: hex(&hash.finalize()),
        }
    }
    pub fn identity(&self) -> &ServiceIdentity {
        &self.identity
    }
    pub fn root_sha256(&self) -> &[u8; 32] {
        &self.root_sha256
    }
    pub fn policy_id(&self) -> &str {
        &self.policy_id
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub struct Request<'a> {
    pub binding: &'a Binding,
    pub name: &'a str,
    pub version: &'a str,
    pub archive_sha256: Option<&'a str>,
}

/// Implementations must obtain sealed evidence from the independent verifier.
/// There is deliberately no production constructor or unsigned TLS adapter.
pub trait Source {
    fn resolve(&self, request: Request<'_>) -> Result<ConsumedArchive, Diagnostic>;
}

#[derive(Debug)]
pub struct ConsumedArchive {
    binding: Binding,
    archive: AdmittedArchive,
}

impl ConsumedArchive {
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
    pub fn archive(&self) -> &AdmittedArchive {
        &self.archive
    }

    #[cfg(test)]
    pub(crate) fn isolated_unsigned(
        binding: Binding,
        index: &crate::registry::store::Index,
        version: &str,
        bytes: &[u8],
    ) -> Result<Self, Diagnostic> {
        if index.identity() != binding.identity() {
            return Err(refused("registry policy binding mismatch"));
        }
        let release = index
            .release(version)
            .ok_or_else(|| refused("registry release unavailable"))?;
        if release.yanked() || release.quarantined() {
            return Err(refused("registry release unavailable"));
        }
        let archive = release
            .admit_archive(bytes)
            .map_err(|_| refused("registry archive binding mismatch"))?;
        Ok(Self { binding, archive })
    }
}

pub struct Context<'a> {
    bindings: BTreeMap<String, Binding>,
    source: &'a dyn Source,
}

impl<'a> Context<'a> {
    pub fn new(
        bindings: BTreeMap<String, Binding>,
        source: &'a dyn Source,
    ) -> Result<Self, Diagnostic> {
        if bindings.len() > manifest::MAX_ENTRIES
            || bindings.iter().any(|(id, b)| id != b.identity.registry())
        {
            return Err(refused("invalid explicit registry bindings"));
        }
        Ok(Self { bindings, source })
    }

    pub(super) fn resolve(
        &self,
        registry: &str,
        name: &str,
        version: &str,
        archive_sha256: Option<&str>,
    ) -> Result<ConsumedArchive, Diagnostic> {
        let binding = self.binding(registry)?;
        let consumed = self.source.resolve(Request {
            binding,
            name,
            version,
            archive_sha256,
        })?;
        let package = consumed.archive.descriptor().package();
        if &consumed.binding != binding
            || package.name() != name
            || package.version() != version
            || archive_sha256.is_some_and(|hash| hash != consumed.archive.sha256())
        {
            return Err(refused(
                "registry consumption evidence does not match dependency",
            ));
        }
        Ok(consumed)
    }
    pub(super) fn binding(&self, registry: &str) -> Result<&Binding, Diagnostic> {
        self.bindings
            .get(registry)
            .ok_or_else(|| refused("registry has no explicit host binding"))
    }
}

pub(super) fn refused(message: &str) -> Diagnostic {
    Diagnostic::error("PKG0006", message)
}
