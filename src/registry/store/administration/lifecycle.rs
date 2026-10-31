use super::*;

const MAX_PROVISION_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum LocalKind {
    Enroll,
    Provision,
    Deactivate,
    Recover,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PrincipalKind {
    EnrollPrincipal,
    DeactivatePrincipal,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ProvisionKind {
    ProvisionCredential,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RecoveryKind {
    RecoverOwner,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PrincipalRecord {
    pub schema: u32,
    pub request_id: String,
    pub expected_state_revision: u64,
    pub action: PrincipalKind,
    pub reason: String,
    pub principal: String,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProvisionRecord {
    pub schema: u32,
    pub request_id: String,
    pub expected_state_revision: u64,
    pub action: ProvisionKind,
    pub reason: String,
    pub credential: authority::codec::Token,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RecoveryRecord {
    pub schema: u32,
    pub request_id: String,
    pub expected_state_revision: u64,
    pub expected_package_revision: u64,
    pub action: RecoveryKind,
    pub reason: String,
    pub name: String,
    pub principal: String,
}
#[derive(Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum LocalRecord {
    Principal(PrincipalRecord),
    Provision(ProvisionRecord),
    Recovery(RecoveryRecord),
}

pub(super) struct LocalAction {
    pub record: LocalRecord,
    bytes: Vec<u8>,
}
fn invalid() -> Error {
    error(ErrorKind::Malformed, "invalid local registry action")
}
fn common(id: &str, revision: u64, reason: &str) -> Result<(), Error> {
    records::hex::<16>(id).map_err(|_| invalid())?;
    if revision == 0 || !actions::valid_reason(reason) {
        return Err(invalid());
    }
    Ok(())
}
impl LocalAction {
    pub(super) fn kind(&self) -> LocalKind {
        match &self.record {
            LocalRecord::Principal(r) => match r.action {
                PrincipalKind::EnrollPrincipal => LocalKind::Enroll,
                PrincipalKind::DeactivatePrincipal => LocalKind::Deactivate,
            },
            LocalRecord::Provision(_) => LocalKind::Provision,
            LocalRecord::Recovery(_) => LocalKind::Recover,
        }
    }
    fn fields(&self) -> (u32, &str, u64, &str) {
        match &self.record {
            LocalRecord::Principal(r) => (
                r.schema,
                &r.request_id,
                r.expected_state_revision,
                &r.reason,
            ),
            LocalRecord::Provision(r) => (
                r.schema,
                &r.request_id,
                r.expected_state_revision,
                &r.reason,
            ),
            LocalRecord::Recovery(r) => (
                r.schema,
                &r.request_id,
                r.expected_state_revision,
                &r.reason,
            ),
        }
    }
    pub(super) fn request_id(&self) -> &str {
        self.fields().1
    }
    pub(super) fn revision(&self) -> u64 {
        self.fields().2
    }
    pub(super) fn reason(&self) -> &str {
        self.fields().3
    }
    pub(super) fn from_record(record: LocalRecord) -> Result<Self, Error> {
        let mut action = Self {
            record,
            bytes: Vec::new(),
        };
        let (schema, id, revision, reason) = action.fields();
        if schema != 1 {
            return Err(invalid());
        }
        common(id, revision, reason)?;
        match &action.record {
            LocalRecord::Principal(r) => records::hex::<16>(&r.principal).map_err(|_| invalid())?,
            LocalRecord::Provision(r) => {
                r.credential.validate_record().map_err(Error::from)?;
                if r.credential.revoked {
                    return Err(invalid());
                }
            }
            LocalRecord::Recovery(r) => {
                records::hex::<16>(&r.principal).map_err(|_| invalid())?;
                records::name(&r.name).map_err(|_| invalid())?;
                if r.expected_package_revision == 0 {
                    return Err(invalid());
                }
            }
        }
        let limit = if action.kind() == LocalKind::Provision {
            MAX_PROVISION_BYTES
        } else {
            MAX_ACTION_BYTES
        };
        action.bytes = records::encode(&action.record, limit)?;
        Ok(action)
    }
    fn admit(bytes: &[u8], kind: LocalKind) -> Result<Self, Error> {
        let limit = if kind == LocalKind::Provision {
            MAX_PROVISION_BYTES
        } else {
            MAX_ACTION_BYTES
        };
        let record = records::decode(bytes, limit).map_err(|e| {
            if e.kind == ErrorKind::Limit {
                e
            } else {
                invalid()
            }
        })?;
        let action = Self::from_record(record)?;
        if action.kind() != kind {
            return Err(invalid());
        }
        Ok(action)
    }
    pub(super) fn request_digest(&self) -> Result<String, Error> {
        let path = match &self.record {
            LocalRecord::Principal(r) => format!(
                "/v1/admin/principals/{}/{}",
                r.principal,
                match r.action {
                    PrincipalKind::EnrollPrincipal => "enroll",
                    PrincipalKind::DeactivatePrincipal => "deactivate",
                }
            ),
            LocalRecord::Provision(r) => {
                format!("/v1/admin/credentials/{}/provision", r.credential.id)
            }
            LocalRecord::Recovery(r) => format!("/v1/admin/packages/{}/recover-owner", r.name),
        };
        records::http_request_digest(
            "LOCAL",
            &path,
            Some(self.revision()),
            &digest(&self.bytes),
            self.bytes.len() as u64,
        )
    }
    pub(super) fn check_time(&self, now: u64) -> Result<(), Error> {
        if let LocalRecord::Provision(r) = &self.record
            && (now < r.credential.issued || now >= r.credential.expires)
        {
            return Err(error(
                ErrorKind::Authorization,
                "registry credential is not currently provisionable",
            ));
        }
        Ok(())
    }
}

macro_rules! action_type {
    ($name:ident, $kind:ident) => {
        /// A bounded canonical local request; this value grants no capability.
        pub struct $name(pub(super) LocalAction);
        impl $name {
            pub fn admit(bytes: &[u8]) -> Result<Self, Error> {
                LocalAction::admit(bytes, LocalKind::$kind).map(Self)
            }
            pub fn as_bytes(&self) -> &[u8] {
                &self.0.bytes
            }
            pub fn request_id(&self) -> &str {
                self.0.request_id()
            }
            pub fn expected_state_revision(&self) -> u64 {
                self.0.revision()
            }
            #[cfg(test)]
            pub(in crate::registry::store) fn request_digest(&self) -> Result<String, Error> {
                self.0.request_digest()
            }
        }
        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name))
                    .field("request_id", &self.request_id())
                    .field("expected_state_revision", &self.expected_state_revision())
                    .finish_non_exhaustive()
            }
        }
    };
}
action_type!(Enrollment, Enroll);
action_type!(CredentialProvision, Provision);
action_type!(Deactivation, Deactivate);
action_type!(OwnerRecovery, Recover);

fn principal_record(
    principal: &str,
    request: &str,
    revision: u64,
    reason: &str,
    action: PrincipalKind,
) -> Result<LocalAction, Error> {
    common(request, revision, reason)?;
    records::hex::<16>(principal).map_err(|_| invalid())?;
    LocalAction::from_record(LocalRecord::Principal(PrincipalRecord {
        schema: 1,
        request_id: request.into(),
        expected_state_revision: revision,
        action,
        reason: reason.into(),
        principal: principal.into(),
    }))
}
impl Enrollment {
    pub fn new(principal: &str, request: &str, revision: u64, reason: &str) -> Result<Self, Error> {
        principal_record(
            principal,
            request,
            revision,
            reason,
            PrincipalKind::EnrollPrincipal,
        )
        .map(Self)
    }
}
impl Deactivation {
    pub fn new(principal: &str, request: &str, revision: u64, reason: &str) -> Result<Self, Error> {
        principal_record(
            principal,
            request,
            revision,
            reason,
            PrincipalKind::DeactivatePrincipal,
        )
        .map(Self)
    }
}
impl CredentialProvision {
    /// The canonical token record contains a domain-separated authenticator hash,
    /// never the raw bearer. Current principal/time checks occur after replay.
    pub fn new(
        credential: &[u8],
        request: &str,
        revision: u64,
        reason: &str,
    ) -> Result<Self, Error> {
        common(request, revision, reason)?;
        let credential = records::decode(credential, MAX_PROVISION_BYTES)?;
        LocalAction::from_record(LocalRecord::Provision(ProvisionRecord {
            schema: 1,
            request_id: request.into(),
            expected_state_revision: revision,
            action: ProvisionKind::ProvisionCredential,
            reason: reason.into(),
            credential,
        }))
        .map(Self)
    }
}
impl OwnerRecovery {
    pub fn new(
        name: &str,
        principal: &str,
        request: &str,
        revision: u64,
        package_revision: u64,
        reason: &str,
    ) -> Result<Self, Error> {
        common(request, revision, reason)?;
        records::name(name).map_err(|_| invalid())?;
        records::hex::<16>(principal).map_err(|_| invalid())?;
        if package_revision == 0 {
            return Err(invalid());
        }
        LocalAction::from_record(LocalRecord::Recovery(RecoveryRecord {
            schema: 1,
            request_id: request.into(),
            expected_state_revision: revision,
            expected_package_revision: package_revision,
            action: RecoveryKind::RecoverOwner,
            reason: reason.into(),
            name: name.into(),
            principal: principal.into(),
        }))
        .map(Self)
    }
}

impl Administrator {
    pub fn enroll(&mut self, action: &Enrollment, clock: &impl Clock) -> Result<Receipt, Error> {
        execute(&mut self.registry, &self.operator, None, &action.0, clock)
    }
    pub fn provision(
        &mut self,
        action: &CredentialProvision,
        clock: &impl Clock,
    ) -> Result<Receipt, Error> {
        execute(&mut self.registry, &self.operator, None, &action.0, clock)
    }
    pub fn deactivate(
        &mut self,
        action: &Deactivation,
        clock: &impl Clock,
    ) -> Result<Receipt, Error> {
        execute(&mut self.registry, &self.operator, None, &action.0, clock)
    }
}

/// Separately supplied local directory authority, restricted to one package.
/// It cannot be constructed from a bearer, receipt or another registry handle.
pub struct RecoveryAdministrator {
    registry: Registry,
    operator: String,
    name: String,
}
impl RecoveryAdministrator {
    pub fn open(
        parent: &Dir,
        child: &str,
        identity: &ServiceIdentity,
        operator: &str,
        name: &str,
    ) -> Result<Self, Error> {
        records::hex::<16>(operator).map_err(|_| invalid())?;
        records::name(name).map_err(|_| invalid())?;
        let registry = Registry::open(parent, child, identity)?;
        let view = registry.recover()?;
        check_operator(&view.state, operator, view.state.time_floor)?;
        Ok(Self {
            registry,
            operator: operator.into(),
            name: name.into(),
        })
    }
    pub fn revision(&self) -> Result<u64, Error> {
        let view = self.registry.recover()?;
        check_operator(&view.state, &self.operator, view.state.time_floor)?;
        Ok(view.state.revision)
    }
    pub fn package_revision(&self) -> Result<u64, Error> {
        let view = self.registry.recover()?;
        check_operator(&view.state, &self.operator, view.state.time_floor)?;
        let i = view
            .state
            .package(&self.name)
            .map_err(|_| error(ErrorKind::NotFound, "registry package is unavailable"))?;
        Ok(view.state.packages[i].revision)
    }
    pub fn recover_owner(
        &mut self,
        action: &OwnerRecovery,
        clock: &impl Clock,
    ) -> Result<Receipt, Error> {
        execute(
            &mut self.registry,
            &self.operator,
            Some(&self.name),
            &action.0,
            clock,
        )
    }
}

fn execute(
    registry: &mut Registry,
    operator: &str,
    scope: Option<&str>,
    action: &LocalAction,
    clock: &impl Clock,
) -> Result<Receipt, Error> {
    let view = registry.recover()?;
    let initial_time = clock.now()?;
    check_operator(&view.state, operator, initial_time)?;
    match (&action.record, scope) {
        (LocalRecord::Recovery(r), Some(name)) if r.name == name => {}
        (LocalRecord::Recovery(_), _) | (_, Some(_)) => {
            return Err(error(
                ErrorKind::Authorization,
                "local registry recovery scope mismatch",
            ));
        }
        (_, None) => {}
    }
    let request_hash = action.request_digest()?;
    if let Some(replay) = view
        .requests
        .get(&(operator.into(), action.request_id().into()))
    {
        check_operator(&view.state, operator, clock.now()?)?;
        return if replay.digest == request_hash {
            Ok(replay.receipt.clone())
        } else {
            Err(conflict())
        };
    }
    if view.state.revision != action.revision() {
        return Err(conflict());
    }
    let draft = prepare(
        &view,
        action,
        operator,
        &request_hash,
        initial_time,
        Preparation::Quota,
    )?;
    view.inventory.reserve(
        &[
            (storage::Space::Audit, &draft.audit_hash, &draft.audit_bytes),
            (storage::Space::State, &draft.state_hash, &draft.state_bytes),
        ],
        draft.head_bytes.len(),
    )?;
    let time = clock.now()?;
    check_operator(&view.state, operator, time)?;
    if time < initial_time {
        return Err(corrupt());
    }
    let publication = prepare(
        &view,
        action,
        operator,
        &request_hash,
        time,
        Preparation::Commit,
    )?;
    registry.storage.install(
        storage::Space::Audit,
        &publication.audit_hash,
        &publication.audit_bytes,
    )?;
    registry.storage.install(
        storage::Space::State,
        &publication.state_hash,
        &publication.state_bytes,
    )?;
    registry.commit(
        Some(&view.head),
        &publication.state_hash,
        &publication.head_bytes,
        publication.receipt,
        || {
            let final_time = clock.now()?;
            if final_time < time {
                return Err(corrupt());
            }
            check_operator(&view.state, operator, final_time)?;
            action.check_time(final_time)
        },
    )
}

enum Preparation {
    Quota,
    Commit,
}
fn prepare(
    view: &history::View,
    action: &LocalAction,
    operator: &str,
    request_hash: &str,
    time: u64,
    preparation: Preparation,
) -> Result<Publication, Error> {
    let (mut state, result) = transitions::advance(&view.state, action, operator, "", time)?;
    if let LocalRecord::Recovery(r) = &action.record {
        reads::encode_package(
            &state.registry,
            &state.packages[state.package(&r.name).map_err(|_| corrupt())?],
        )?;
    }
    // Validate with the actual sampled time first. Only the uninstalled size
    // reservation uses maximal timestamp widths; no expired token is admitted.
    let time = match preparation {
        Preparation::Quota => authority::MAX_TIME,
        Preparation::Commit => time,
    };
    state.time_floor = time;
    state.authority.time_floor = time;
    let audit = records::Audit {
        format: "lang-registry-audit".into(),
        schema: 1,
        sequence: state.revision,
        previous_audit_sha256: Some(view.state.last_audit_sha256.clone()),
        prior_state_sha256: Some(view.head.clone()),
        time,
        principal: operator.into(),
        credential_id: None,
        operation: transitions::operation(action)?,
        request_id: Some(action.request_id().into()),
        request_sha256: Some(request_hash.into()),
        reason: action.reason().into(),
        result,
    };
    audit.validate()?;
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
