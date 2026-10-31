use super::*;
use serde::{Deserialize, Serialize};

const MAX_ACTION_BYTES: usize = 2048;
pub const MAX_OWNER_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum OwnerActionKind {
    AddOwner,
    RemoveOwner,
}
impl OwnerActionKind {
    pub(super) fn member(self) -> bool {
        self == Self::AddOwner
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    request_id: String,
    expected_revision: u64,
    action: OwnerActionKind,
    reason: String,
    principal: String,
}

/// A bounded canonical request, never authentication or authority evidence.
pub struct OwnerAction {
    name: String,
    record: Record,
    bytes: Vec<u8>,
}
impl std::fmt::Debug for OwnerAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OwnerAction")
            .field("action", &self.record.action)
            .field("request_id", &self.record.request_id)
            .field("expected_revision", &self.record.expected_revision)
            .finish_non_exhaustive()
    }
}
fn malformed() -> Error {
    error(ErrorKind::Malformed, "invalid registry owner action")
}
fn validate(
    name: &str,
    principal: &str,
    id: &str,
    revision: u64,
    reason: &str,
) -> Result<(), Error> {
    records::name(name).map_err(|_| malformed())?;
    records::hex::<16>(principal).map_err(|_| malformed())?;
    records::hex::<16>(id).map_err(|_| malformed())?;
    if revision == 0 || !actions::valid_reason(reason) {
        return Err(malformed());
    }
    Ok(())
}
impl OwnerAction {
    pub fn new(
        name: &str,
        principal: &str,
        request_id: &str,
        expected_revision: u64,
        action: OwnerActionKind,
        reason: &str,
    ) -> Result<Self, Error> {
        // Validate borrowed input before copying caller-sized strings.
        validate(name, principal, request_id, expected_revision, reason)?;
        Self::finish(
            name,
            Record {
                schema: 1,
                request_id: request_id.into(),
                expected_revision,
                action,
                reason: reason.into(),
                principal: principal.into(),
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
            &record.principal,
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
    pub fn principal(&self) -> &str {
        &self.record.principal
    }
    pub fn request_id(&self) -> &str {
        &self.record.request_id
    }
    pub fn expected_revision(&self) -> u64 {
        self.record.expected_revision
    }
    pub fn kind(&self) -> OwnerActionKind {
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
    /// Return a freshly authorized private membership snapshot, never authority.
    pub fn owners(
        &self,
        bearer: &Bearer,
        name: &str,
        clock: &impl Clock,
    ) -> Result<Vec<u8>, Error> {
        #[derive(Serialize)]
        struct OwnerList<'a> {
            schema: u32,
            registry: &'a str,
            name: &'a str,
            revision: u64,
            owners: &'a [String],
        }
        records::name(name).map_err(|_| malformed())?;
        let view = self.recover()?;
        let authority = Authority::from_records(view.state.authority.clone(), &self.identity)?;
        let operation = Operation::ReadOwners(name);
        authority.authorize(bearer, clock.now()?, operation)?;
        let i = view.state.package(name).map_err(|_| conflict())?;
        let package = &view.state.authority.packages[i];
        let bytes = records::encode(
            &OwnerList {
                schema: 1,
                registry: &view.state.registry,
                name,
                revision: view.state.packages[i].revision,
                owners: &package.owners,
            },
            MAX_OWNER_BYTES,
        )?;
        authority.authorize(bearer, clock.now()?, operation)?;
        Ok(bytes)
    }

    /// Change current membership without changing credentials or source archives.
    pub fn change_owner(
        &mut self,
        bearer: &Bearer,
        action: &OwnerAction,
        clock: &impl Clock,
    ) -> Result<Receipt, Error> {
        let view = self.recover()?;
        let operation = Operation::ChangeOwners(action.name());
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
    action: &OwnerAction,
    principal: &str,
    credential: &str,
    request_hash: &str,
    time: u64,
) -> Result<Publication, Error> {
    let (mut state, result) = history::advance_owner(
        &view.state,
        action.name(),
        action.principal(),
        action.kind().member(),
        "",
        time,
    )?;
    reads::encode_package(
        &state.registry,
        &state.packages[state.package(action.name()).map_err(|_| corrupt())?],
    )?;
    let operation = match action.kind() {
        OwnerActionKind::AddOwner => records::AuditOperation::AddOwner {
            name: result.name.clone(),
            principal: result.principal.clone(),
        },
        OwnerActionKind::RemoveOwner => records::AuditOperation::RemoveOwner {
            name: result.name.clone(),
            principal: result.principal.clone(),
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
        result: records::AuditResult::Owner(result),
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
