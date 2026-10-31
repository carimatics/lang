//! RFC 0048's private host registry. A held exclusive lock serializes all writes.
//! No bearer, saved actor, receipt or archive grants ambient filesystem access.

mod history;
mod records;
mod storage;
#[cfg(test)]
mod tests;

use super::{archive, authority};
use archive::AdmittedArchive;
use authority::{Authority, Bearer, Operation, ServiceIdentity};
use cap_std::fs::Dir;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::{Display, Formatter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ErrorKind {
    Malformed,
    Limit,
    Authentication,
    Authorization,
    Conflict,
    Integrity,
    State,
    Busy,
    Io,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationOutcome {
    NotPublished,
    Unknown,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub kind: ErrorKind,
    pub outcome: PublicationOutcome,
    pub message: &'static str,
}
impl Error {
    pub const fn code(&self) -> &'static str {
        match self.kind {
            ErrorKind::Malformed => "REG0001",
            ErrorKind::Limit => "REG0002",
            ErrorKind::Authentication => "REG0003",
            ErrorKind::Authorization => "REG0004",
            ErrorKind::Conflict => "REG0005",
            ErrorKind::Integrity => "REG0006",
            ErrorKind::State => "REG0007",
            ErrorKind::Busy | ErrorKind::Io => "REG0008",
        }
    }
}
impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.message)
    }
}
impl std::error::Error for Error {}
fn error(kind: ErrorKind, message: &'static str) -> Error {
    Error {
        kind,
        outcome: PublicationOutcome::NotPublished,
        message,
    }
}
fn corrupt() -> Error {
    error(
        ErrorKind::State,
        "invalid registry state; verified recovery required",
    )
}
fn capacity() -> Error {
    error(ErrorKind::Limit, "registry capacity exceeded")
}
fn conflict() -> Error {
    error(ErrorKind::Conflict, "registry request or revision conflict")
}
fn io() -> Error {
    error(ErrorKind::Io, "registry storage operation failed")
}
impl From<authority::Error> for Error {
    fn from(e: authority::Error) -> Self {
        let kind = match e.kind {
            authority::ErrorKind::Malformed => ErrorKind::Malformed,
            authority::ErrorKind::Limit => ErrorKind::Limit,
            authority::ErrorKind::Authentication => ErrorKind::Authentication,
            authority::ErrorKind::Authorization => ErrorKind::Authorization,
            authority::ErrorKind::Identity => ErrorKind::Integrity,
            authority::ErrorKind::State => ErrorKind::State,
            authority::ErrorKind::Io => ErrorKind::Io,
        };
        error(kind, e.message)
    }
}
fn digest(bytes: &[u8]) -> String {
    authority::codec::hex(&Sha256::digest(bytes))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExpectedRevision {
    Absent,
    Revision(u64),
}

/// Trusted host time source. Never implement this from request timestamps.
pub trait Clock {
    fn now(&self) -> Result<u64, Error>;
}
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Result<u64, Error> {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .map_err(|_| corrupt())
    }
}

/// Informational committed result. No operation accepts a receipt as authority.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Receipt {
    schema: u32,
    audit_sha256: String,
    result: records::AuditResult,
}
impl Receipt {
    pub fn audit_sha256(&self) -> &str {
        &self.audit_sha256
    }
    pub fn state_revision(&self) -> u64 {
        match &self.result {
            records::AuditResult::Initialize(r) => r.state_revision,
            records::AuditResult::Publish(r) => r.state_revision,
        }
    }
    pub fn as_bytes(&self) -> Result<Vec<u8>, Error> {
        records::encode(self, records::MAX_AUDIT)
    }
}

pub struct Registry {
    storage: storage::Storage,
    identity: ServiceIdentity,
}
impl Registry {
    /// Exclusively create a private child of an explicit directory capability.
    /// A failed bootstrap leaves an incomplete destination for operator recovery.
    pub fn initialize(
        parent: &Dir,
        child: &str,
        identity: &ServiceIdentity,
        authority_bytes: &[u8],
        operator: &str,
        clock: &impl Clock,
    ) -> Result<Self, Error> {
        let mut authority = Authority::admit(authority_bytes, identity)?.into_records();
        let time = clock.now()?;
        if time > authority::MAX_TIME
            || time < authority.time_floor
            || !authority.packages.is_empty()
            || !authority
                .principals
                .iter()
                .any(|p| p.id == operator && p.active)
        {
            return Err(corrupt());
        }
        authority.time_floor = time;
        let audit = records::Audit {
            format: "lang-registry-audit".into(),
            schema: 1,
            sequence: 1,
            previous_audit_sha256: None,
            prior_state_sha256: None,
            time,
            principal: operator.into(),
            credential_id: None,
            operation: records::AuditOperation::Initialize,
            request_id: None,
            request_sha256: None,
            reason: "initialize".into(),
            result: records::AuditResult::Initialize(records::InitialResult { state_revision: 1 }),
        };
        let audit_bytes = records::encode(&audit, records::MAX_AUDIT)?;
        let audit_hash = digest(&audit_bytes);
        let state = records::State {
            format: "lang-registry-state".into(),
            schema: 1,
            registry: identity.registry().into(),
            origin: identity.origin().into(),
            revision: 1,
            time_floor: time,
            authority,
            packages: Vec::new(),
            last_audit_sha256: audit_hash.clone(),
        };
        state.validate(identity)?;
        let state_bytes = records::encode(&state, records::MAX_STATE)?;
        let state_hash = digest(&state_bytes);
        let head_bytes = head_bytes(&state_hash)?;
        let store = Self {
            storage: storage::Storage::create(parent, child)?,
            identity: identity.clone(),
        };
        store
            .storage
            .install(storage::Space::Audit, &audit_hash, &audit_bytes)?;
        store
            .storage
            .install(storage::Space::State, &state_hash, &state_bytes)?;
        let receipt = Receipt {
            schema: 1,
            audit_sha256: audit_hash,
            result: audit.result,
        };
        store.commit(None, &state_hash, &head_bytes, receipt, || Ok(()))?;
        Ok(store)
    }
    /// Acquires the nonblocking lock, proves the whole committed history, then
    /// removes only validated unpublished candidates. The lock lasts until drop.
    pub fn open(parent: &Dir, child: &str, identity: &ServiceIdentity) -> Result<Self, Error> {
        let store = Self {
            storage: storage::Storage::open(parent, child)?,
            identity: identity.clone(),
        };
        store.recover()?;
        Ok(store)
    }
    fn recover(&self) -> Result<history::View, Error> {
        let mut view = history::load(&self.storage, &self.identity)?;
        self.storage.cleanup(&mut view.inventory, &view.reachable)?;
        Ok(view)
    }
    pub fn revision(&self) -> Result<u64, Error> {
        Ok(history::load(&self.storage, &self.identity)?.state.revision)
    }

    pub fn publish(
        &mut self,
        bearer: &Bearer,
        archive: &AdmittedArchive,
        request_id: &str,
        expected: ExpectedRevision,
        clock: &impl Clock,
    ) -> Result<Receipt, Error> {
        authority::codec::unhex::<16>(request_id)
            .map_err(|_| error(ErrorKind::Malformed, "invalid registry request identifier"))?;
        if expected == ExpectedRevision::Revision(0) {
            return Err(error(
                ErrorKind::Malformed,
                "invalid expected registry revision",
            ));
        }
        let view = self.recover()?;
        let name = archive.descriptor().package().name();
        let version = archive.descriptor().package().version();
        let operation = view.state.operation(name);
        let authority = Authority::from_records(view.state.authority.clone(), &self.identity)?;
        let actor = authority.authorize(bearer, clock.now()?, operation)?;
        let request_digest = records::request_digest(
            name,
            version,
            expected,
            archive.sha256(),
            archive.as_bytes().len() as u64,
        )?;
        if let Some(replay) = view
            .requests
            .get(&(actor.principal().into(), request_id.into()))
        {
            authority.authorize(bearer, clock.now()?, operation)?;
            return if replay.digest == request_digest {
                Ok(replay.receipt.clone())
            } else {
                Err(conflict())
            };
        }
        let current = match view.state.package(name) {
            Ok(i) => ExpectedRevision::Revision(view.state.packages[i].revision),
            Err(_) => ExpectedRevision::Absent,
        };
        if expected != current {
            return Err(conflict());
        }
        let principal = actor.principal();
        let credential = actor.credential_id();
        // Reserve using the longest admitted timestamp before any candidate is
        // written. Actual metadata is rebuilt after staging with fresh time/auth.
        let draft = Publication::build(
            &view,
            archive,
            principal,
            credential,
            request_id,
            &request_digest,
            authority::MAX_TIME,
        )?;
        view.inventory.reserve(
            &[
                (storage::Space::Object, archive.sha256(), archive.as_bytes()),
                (storage::Space::Audit, &draft.audit_hash, &draft.audit_bytes),
                (storage::Space::State, &draft.state_hash, &draft.state_bytes),
            ],
            draft.head_bytes.len(),
        )?;
        self.storage
            .install(storage::Space::Object, archive.sha256(), archive.as_bytes())?;
        let time = clock.now()?;
        authority.authorize(bearer, time, operation)?;
        let publication = Publication::build(
            &view,
            archive,
            principal,
            credential,
            request_id,
            &request_digest,
            time,
        )?;
        self.storage.install(
            storage::Space::Audit,
            &publication.audit_hash,
            &publication.audit_bytes,
        )?;
        self.storage.install(
            storage::Space::State,
            &publication.state_hash,
            &publication.state_bytes,
        )?;
        self.commit(
            Some(&view.head),
            &publication.state_hash,
            &publication.head_bytes,
            publication.receipt,
            || {
                let final_time = clock.now()?;
                if final_time < time {
                    return Err(corrupt());
                }
                authority.authorize(bearer, final_time, operation)?;
                Ok(())
            },
        )
    }
    fn commit(
        &self,
        old: Option<&str>,
        new: &str,
        bytes: &[u8],
        receipt: Receipt,
        authorize: impl FnOnce() -> Result<(), Error>,
    ) -> Result<Receipt, Error> {
        match self.storage.replace_head(new, bytes, authorize) {
            Ok(()) => Ok(receipt),
            Err(failure) => match history::load(&self.storage, &self.identity) {
                Ok(view) if view.head == new => Ok(receipt),
                Ok(view) if Some(view.head.as_str()) == old => Err(failure),
                _ => Err(Error {
                    kind: ErrorKind::Io,
                    outcome: PublicationOutcome::Unknown,
                    message: "registry publication outcome unknown; reconcile the same request identity",
                }),
            },
        }
    }
}
fn head_bytes(hash: &str) -> Result<Vec<u8>, Error> {
    records::encode(
        &records::Head {
            schema: 1,
            state_sha256: hash.into(),
        },
        records::MAX_HEAD,
    )
}
struct Publication {
    audit_hash: String,
    audit_bytes: Vec<u8>,
    state_hash: String,
    state_bytes: Vec<u8>,
    head_bytes: Vec<u8>,
    receipt: Receipt,
}
impl Publication {
    fn build(
        view: &history::View,
        archive: &AdmittedArchive,
        principal: &str,
        credential: &str,
        request_id: &str,
        request_hash: &str,
        time: u64,
    ) -> Result<Self, Error> {
        let name = archive.descriptor().package().name();
        let (mut state, result) = history::advance(
            &view.state,
            principal,
            name,
            records::Release::from_archive(archive),
            "",
            time,
        )?;
        let audit = records::Audit {
            format: "lang-registry-audit".into(),
            schema: 1,
            sequence: state.revision,
            previous_audit_sha256: Some(view.state.last_audit_sha256.clone()),
            prior_state_sha256: Some(view.head.clone()),
            time,
            principal: principal.into(),
            credential_id: Some(credential.into()),
            operation: records::AuditOperation::Publish {
                name: name.into(),
                version: archive.descriptor().package().version().into(),
                archive_sha256: archive.sha256().into(),
            },
            request_id: Some(request_id.into()),
            request_sha256: Some(request_hash.into()),
            reason: "publish".into(),
            result: records::AuditResult::Publish(result),
        };
        let audit_bytes = records::encode(&audit, records::MAX_AUDIT)?;
        let audit_hash = digest(&audit_bytes);
        state.last_audit_sha256 = audit_hash.clone();
        let state_bytes = records::encode(&state, records::MAX_STATE)?;
        let state_hash = digest(&state_bytes);
        let head_bytes = head_bytes(&state_hash)?;
        let receipt = Receipt {
            schema: 1,
            audit_sha256: audit_hash.clone(),
            result: audit.result,
        };
        Ok(Self {
            audit_hash,
            audit_bytes,
            state_hash,
            state_bytes,
            head_bytes,
            receipt,
        })
    }
}

fn checkpoint(_label: &'static str) {
    #[cfg(test)]
    tests::checkpoint(_label);
}
