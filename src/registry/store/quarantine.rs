use super::*;
use serde::{Deserialize, Serialize};

const MAX_ACTION_BYTES: usize = 2048;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum QuarantineActionKind {
    Quarantine,
    Unquarantine,
}
impl QuarantineActionKind {
    pub(super) fn quarantined(self) -> bool {
        self == Self::Quarantine
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    request_id: String,
    expected_revision: u64,
    action: QuarantineActionKind,
    reason: String,
    version: String,
}

/// A bounded canonical request, never authentication or authority evidence.
pub struct QuarantineAction {
    name: String,
    record: Record,
    bytes: Vec<u8>,
}
impl std::fmt::Debug for QuarantineAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QuarantineAction")
            .field("action", &self.record.action)
            .field("request_id", &self.record.request_id)
            .field("expected_revision", &self.record.expected_revision)
            .finish_non_exhaustive()
    }
}
fn malformed() -> Error {
    error(ErrorKind::Malformed, "invalid registry quarantine action")
}
pub(super) fn valid_reason(reason: &str) -> bool {
    !reason.is_empty() && reason.len() <= 256 && !reason.chars().any(char::is_control)
}
fn validate(name: &str, version: &str, id: &str, revision: u64, reason: &str) -> Result<(), Error> {
    records::name(name).map_err(|_| malformed())?;
    records::version(version).map_err(|_| malformed())?;
    records::hex::<16>(id).map_err(|_| malformed())?;
    if revision == 0 || !valid_reason(reason) {
        return Err(malformed());
    }
    Ok(())
}
impl QuarantineAction {
    pub fn new(
        name: &str,
        version: &str,
        request_id: &str,
        expected_revision: u64,
        action: QuarantineActionKind,
        reason: &str,
    ) -> Result<Self, Error> {
        // Validate borrowed input before copying caller-sized strings.
        validate(name, version, request_id, expected_revision, reason)?;
        Self::finish(
            name,
            Record {
                schema: 1,
                request_id: request_id.into(),
                expected_revision,
                action,
                reason: reason.into(),
                version: version.into(),
            },
        )
    }
    pub fn admit(name: &str, bytes: &[u8]) -> Result<Self, Error> {
        records::name(name).map_err(|_| malformed())?;
        let record: Record = records::decode(bytes, MAX_ACTION_BYTES).map_err(|e| {
            if e.kind == ErrorKind::Limit {
                e
            } else {
                malformed()
            }
        })?;
        if record.schema != 1 {
            return Err(malformed());
        }
        validate(
            name,
            &record.version,
            &record.request_id,
            record.expected_revision,
            &record.reason,
        )?;
        Self::finish(name, record)
    }
    fn finish(name: &str, record: Record) -> Result<Self, Error> {
        let bytes = records::encode(&record, MAX_ACTION_BYTES)?;
        Ok(Self {
            name: name.into(),
            record,
            bytes,
        })
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn version(&self) -> &str {
        &self.record.version
    }
    pub fn request_id(&self) -> &str {
        &self.record.request_id
    }
    pub fn expected_revision(&self) -> u64 {
        self.record.expected_revision
    }
    pub fn kind(&self) -> QuarantineActionKind {
        self.record.action
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub(super) fn request_digest(&self) -> Result<String, Error> {
        records::http_request_digest(
            "POST",
            &format!("/v1/packages/{}/actions", self.name),
            Some(self.expected_revision()),
            &digest(&self.bytes),
            self.bytes.len() as u64,
        )
    }
}

impl Registry {
    /// Restrict or restore exact downloads without replacing any source archive.
    pub fn quarantine(
        &mut self,
        bearer: &Bearer,
        action: &QuarantineAction,
        clock: &impl Clock,
    ) -> Result<Receipt, Error> {
        let view = self.recover()?;
        let operation = Operation::Admin(action.name());
        let authority = Authority::from_records(view.state.authority.clone(), &self.identity)?;
        let actor = authority.authorize(bearer, clock.now()?, operation)?;
        let request_hash = action.request_digest()?;
        if let Some(replay) = view
            .requests
            .get(&(actor.principal().into(), action.request_id().into()))
        {
            authority.authorize(bearer, clock.now()?, operation)?;
            return if replay.digest == request_hash {
                Ok(replay.receipt.clone())
            } else {
                Err(conflict())
            };
        }
        let i = view.state.package(action.name()).map_err(|_| conflict())?;
        if view.state.packages[i].revision != action.expected_revision() {
            return Err(conflict());
        }
        let principal = actor.principal();
        let credential = actor.credential_id();
        let draft = prepare(
            &view,
            action,
            principal,
            credential,
            &request_hash,
            authority::MAX_TIME,
        )?;
        view.inventory.reserve(
            &[
                (storage::Space::Audit, &draft.audit_hash, &draft.audit_bytes),
                (storage::Space::State, &draft.state_hash, &draft.state_bytes),
            ],
            draft.head_bytes.len(),
        )?;
        let time = clock.now()?;
        authority.authorize(bearer, time, operation)?;
        let publication = prepare(&view, action, principal, credential, &request_hash, time)?;
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
}

fn prepare(
    view: &history::View,
    action: &QuarantineAction,
    principal: &str,
    credential: &str,
    request_hash: &str,
    time: u64,
) -> Result<Publication, Error> {
    let (mut state, result) = history::advance_quarantine(
        &view.state,
        action.name(),
        action.version(),
        action.kind().quarantined(),
        "",
        time,
    )?;
    reads::encode_package(
        &state.registry,
        &state.packages[state.package(action.name()).map_err(|_| corrupt())?],
    )?;
    let operation = match action.kind() {
        QuarantineActionKind::Quarantine => records::AuditOperation::Quarantine {
            name: result.name.clone(),
            version: result.version.clone(),
            archive_sha256: result.archive_sha256.clone(),
        },
        QuarantineActionKind::Unquarantine => records::AuditOperation::Unquarantine {
            name: result.name.clone(),
            version: result.version.clone(),
            archive_sha256: result.archive_sha256.clone(),
        },
    };
    let audit = records::Audit {
        format: "lang-registry-audit".into(),
        schema: 1,
        sequence: state.revision,
        previous_audit_sha256: Some(view.state.last_audit_sha256.clone()),
        prior_state_sha256: Some(view.head.clone()),
        time,
        principal: principal.into(),
        credential_id: Some(credential.into()),
        operation,
        request_id: Some(action.request_id().into()),
        request_sha256: Some(request_hash.into()),
        reason: action.record.reason.clone(),
        result: records::AuditResult::Quarantine(result),
    };
    let audit_bytes = records::encode(&audit, records::MAX_AUDIT)?;
    let audit_hash = digest(&audit_bytes);
    state.last_audit_sha256 = audit_hash.clone();
    let state_bytes = records::encode(&state, view.state_limit)?;
    let state_hash = digest(&state_bytes);
    let head_bytes = head_bytes(&state_hash)?;
    let receipt = Receipt {
        schema: 1,
        audit_sha256: audit_hash.clone(),
        result: audit.result,
    };
    Ok(Publication {
        audit_hash,
        audit_bytes,
        state_hash,
        state_bytes,
        head_bytes,
        receipt,
    })
}
