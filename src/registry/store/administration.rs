use super::*;
use serde::{Deserialize, Serialize};

const MAX_ACTION_BYTES: usize = 2048;

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    RevokeCredential,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    request_id: String,
    expected_state_revision: u64,
    action: Kind,
    reason: String,
    credential_id: String,
}

/// A bounded local request. Possession of this value grants no capability.
pub struct Revocation {
    record: Record,
    bytes: Vec<u8>,
}
impl std::fmt::Debug for Revocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Revocation")
            .field("request_id", &self.record.request_id)
            .field(
                "expected_state_revision",
                &self.record.expected_state_revision,
            )
            .finish_non_exhaustive()
    }
}
fn malformed() -> Error {
    error(ErrorKind::Malformed, "invalid local registry revocation")
}
fn validate(credential: &str, request: &str, revision: u64, reason: &str) -> Result<(), Error> {
    records::hex::<16>(credential).map_err(|_| malformed())?;
    records::hex::<16>(request).map_err(|_| malformed())?;
    if revision == 0 || !actions::valid_reason(reason) {
        return Err(malformed());
    }
    Ok(())
}
impl Revocation {
    pub fn new(
        credential_id: &str,
        request_id: &str,
        expected_state_revision: u64,
        reason: &str,
    ) -> Result<Self, Error> {
        validate(credential_id, request_id, expected_state_revision, reason)?;
        Self::finish(Record {
            schema: 1,
            request_id: request_id.into(),
            expected_state_revision,
            action: Kind::RevokeCredential,
            reason: reason.into(),
            credential_id: credential_id.into(),
        })
    }
    pub fn admit(bytes: &[u8]) -> Result<Self, Error> {
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
            &record.credential_id,
            &record.request_id,
            record.expected_state_revision,
            &record.reason,
        )?;
        Self::finish(record)
    }
    fn finish(record: Record) -> Result<Self, Error> {
        let bytes = records::encode(&record, MAX_ACTION_BYTES)?;
        Ok(Self { record, bytes })
    }
    pub fn credential_id(&self) -> &str {
        &self.record.credential_id
    }
    pub fn request_id(&self) -> &str {
        &self.record.request_id
    }
    pub fn expected_state_revision(&self) -> u64 {
        self.record.expected_state_revision
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub(super) fn request_digest(&self) -> Result<String, Error> {
        // LOCAL is an internal domain separator, never an HTTP route/method.
        records::http_request_digest(
            "LOCAL",
            &format!("/v1/admin/credentials/{}/revoke", self.credential_id()),
            Some(self.expected_state_revision()),
            &digest(&self.bytes),
            self.bytes.len() as u64,
        )
    }
}

/// Local administrative authority comes from a freshly supplied directory
/// capability. There is no conversion from a bearer or an ordinary Registry.
pub struct Administrator {
    registry: Registry,
    operator: String,
}
impl Administrator {
    pub fn open(
        parent: &Dir,
        child: &str,
        identity: &ServiceIdentity,
        operator: &str,
    ) -> Result<Self, Error> {
        records::hex::<16>(operator).map_err(|_| malformed())?;
        let registry = Registry::open(parent, child, identity)?;
        let view = registry.recover()?;
        check_operator(&view.state, operator, view.state.time_floor)?;
        Ok(Self {
            registry,
            operator: operator.into(),
        })
    }
    pub fn revision(&self) -> Result<u64, Error> {
        let view = self.registry.recover()?;
        check_operator(&view.state, &self.operator, view.state.time_floor)?;
        Ok(view.state.revision)
    }
    pub fn revoke(&mut self, action: &Revocation, clock: &impl Clock) -> Result<Receipt, Error> {
        let view = self.registry.recover()?;
        check_operator(&view.state, &self.operator, clock.now()?)?;
        let request_hash = action.request_digest()?;
        if let Some(replay) = view
            .requests
            .get(&(self.operator.clone(), action.request_id().into()))
        {
            check_operator(&view.state, &self.operator, clock.now()?)?;
            return if replay.digest == request_hash {
                Ok(replay.receipt.clone())
            } else {
                Err(conflict())
            };
        }
        if view.state.revision != action.expected_state_revision() {
            return Err(conflict());
        }
        let draft = prepare(
            &view,
            action,
            &self.operator,
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
        check_operator(&view.state, &self.operator, time)?;
        let publication = prepare(&view, action, &self.operator, &request_hash, time)?;
        self.registry.storage.install(
            storage::Space::Audit,
            &publication.audit_hash,
            &publication.audit_bytes,
        )?;
        self.registry.storage.install(
            storage::Space::State,
            &publication.state_hash,
            &publication.state_bytes,
        )?;
        self.registry.commit(
            Some(&view.head),
            &publication.state_hash,
            &publication.head_bytes,
            publication.receipt,
            || {
                let final_time = clock.now()?;
                if final_time < time {
                    return Err(corrupt());
                }
                check_operator(&view.state, &self.operator, final_time)
            },
        )
    }
}

/// Validates configured attribution and time, not possession of a capability.
/// Only Administrator construction grants the current local host authority.
pub(super) fn check_operator(
    state: &records::State,
    operator: &str,
    now: u64,
) -> Result<(), Error> {
    if now > authority::MAX_TIME || now < state.time_floor {
        return Err(error(
            ErrorKind::State,
            "invalid or regressed registry time",
        ));
    }
    if !state
        .authority
        .principals
        .iter()
        .any(|p| p.id == operator && p.active)
    {
        return Err(error(
            ErrorKind::Authorization,
            "local registry operator is unavailable",
        ));
    }
    Ok(())
}

fn prepare(
    view: &history::View,
    action: &Revocation,
    operator: &str,
    request_hash: &str,
    time: u64,
) -> Result<Publication, Error> {
    let (mut state, result) =
        history::advance_revocation(&view.state, action.credential_id(), "", time)?;
    let audit = records::Audit {
        format: "lang-registry-audit".into(),
        schema: 1,
        sequence: state.revision,
        previous_audit_sha256: Some(view.state.last_audit_sha256.clone()),
        prior_state_sha256: Some(view.head.clone()),
        time,
        principal: operator.into(),
        credential_id: None,
        operation: records::AuditOperation::RevokeCredential {
            credential_id: action.credential_id().into(),
        },
        request_id: Some(action.request_id().into()),
        request_sha256: Some(request_hash.into()),
        reason: action.record.reason.clone(),
        result: records::AuditResult::Revocation(result),
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
    Ok(Publication {
        audit_hash,
        audit_bytes,
        state_hash,
        state_bytes,
        head_bytes,
        receipt,
    })
}
