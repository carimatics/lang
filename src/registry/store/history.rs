use super::*;
use records::{Audit, AuditOperation, AuditResult, PublishResult, Release, State};
use std::collections::{BTreeMap, BTreeSet};
use storage::{Inventory, Space, Storage};

pub(super) struct Replay {
    pub digest: String,
    pub receipt: Receipt,
}
pub(super) struct View {
    pub head: String,
    pub state: State,
    pub inventory: Inventory,
    pub reachable: BTreeSet<(Space, String)>,
    pub requests: BTreeMap<(String, String), Replay>,
}

pub(super) fn state(
    store: &Storage,
    hash: &str,
    identity: &ServiceIdentity,
) -> Result<State, Error> {
    let s: State = records::decode(&store.immutable(Space::State, hash)?, records::MAX_STATE)?;
    s.validate(identity)?;
    Ok(s)
}
fn bind(
    s: &State,
    inventory: &Inventory,
    reachable: &mut BTreeSet<(Space, String)>,
) -> Result<(), Error> {
    for p in &s.packages {
        for r in &p.releases {
            if inventory.bindings.get(&r.archive_sha256) != Some(&r.binding(&p.name)?) {
                return Err(corrupt());
            }
            reachable.insert((Space::Object, r.archive_sha256.clone()));
        }
    }
    Ok(())
}
pub(super) fn load(store: &Storage, identity: &ServiceIdentity) -> Result<View, Error> {
    let inventory = store.inventory(identity)?;
    let head = store.head()?;
    let mut current_hash = head.clone();
    let mut current = state(store, &current_hash, identity)?;
    let mut reachable = BTreeSet::new();
    let mut requests = BTreeMap::new();
    loop {
        if !reachable.insert((Space::State, current_hash.clone()))
            || !reachable.insert((Space::Audit, current.last_audit_sha256.clone()))
        {
            return Err(corrupt());
        }
        bind(&current, &inventory, &mut reachable)?;
        let a: Audit = records::decode(
            &store.immutable(Space::Audit, &current.last_audit_sha256)?,
            records::MAX_AUDIT,
        )?;
        a.validate()?;
        if a.sequence != current.revision || a.time != current.time_floor {
            return Err(corrupt());
        }
        match &a.operation {
            AuditOperation::Initialize => {
                if !current.packages.is_empty()
                    || !current
                        .authority
                        .principals
                        .iter()
                        .any(|p| p.active && p.id == a.principal)
                {
                    return Err(corrupt());
                }
                break;
            }
            AuditOperation::Publish {
                name,
                version,
                archive_sha256,
            } => {
                let prior_hash = a.prior_state_sha256.as_deref().ok_or_else(corrupt)?;
                let prior = state(store, prior_hash, identity)?;
                if prior.revision.checked_add(1) != Some(current.revision)
                    || prior.time_floor > a.time
                    || Some(&prior.last_audit_sha256) != a.previous_audit_sha256.as_ref()
                {
                    return Err(corrupt());
                }
                let authority = Authority::from_records(prior.authority.clone(), identity)
                    .map_err(|_| corrupt())?;
                authority
                    .validate_recorded_actor(
                        &a.principal,
                        a.credential_id.as_deref().ok_or_else(corrupt)?,
                        a.time,
                        prior.operation(name),
                    )
                    .map_err(|_| corrupt())?;
                let package = &current.packages[current.package(name).map_err(|_| corrupt())?];
                let release = package
                    .releases
                    .iter()
                    .find(|r| r.version == *version && r.archive_sha256 == *archive_sha256)
                    .ok_or_else(corrupt)?;
                let condition = match prior.package(name) {
                    Ok(i) => ExpectedRevision::Revision(prior.packages[i].revision),
                    Err(_) => ExpectedRevision::Absent,
                };
                let request_digest = records::request_digest(
                    name,
                    version,
                    condition,
                    archive_sha256,
                    release.archive_bytes,
                )?;
                if a.request_sha256.as_deref() != Some(&request_digest) {
                    return Err(corrupt());
                }
                let (expected, result) = advance(
                    &prior,
                    &a.principal,
                    name,
                    release.clone(),
                    &current.last_audit_sha256,
                    a.time,
                )
                .map_err(|_| corrupt())?;
                if current != expected || a.result != AuditResult::Publish(result) {
                    return Err(corrupt());
                }
                let key = (
                    a.principal.clone(),
                    a.request_id.clone().ok_or_else(corrupt)?,
                );
                let replay = Replay {
                    digest: request_digest,
                    receipt: Receipt {
                        schema: 1,
                        audit_sha256: current.last_audit_sha256.clone(),
                        result: a.result.clone(),
                    },
                };
                if requests.insert(key, replay).is_some() {
                    return Err(corrupt());
                }
                current_hash = prior_hash.into();
                current = prior;
            }
            operation @ (AuditOperation::Yank {
                name,
                version,
                archive_sha256,
            }
            | AuditOperation::Unyank {
                name,
                version,
                archive_sha256,
            }) => {
                let prior_hash = a.prior_state_sha256.as_deref().ok_or_else(corrupt)?;
                let prior = state(store, prior_hash, identity)?;
                if prior.revision.checked_add(1) != Some(current.revision)
                    || prior.time_floor > a.time
                    || Some(&prior.last_audit_sha256) != a.previous_audit_sha256.as_ref()
                {
                    return Err(corrupt());
                }
                Authority::from_records(prior.authority.clone(), identity)
                    .map_err(|_| corrupt())?
                    .validate_recorded_actor(
                        &a.principal,
                        a.credential_id.as_deref().ok_or_else(corrupt)?,
                        a.time,
                        Operation::Yank(name),
                    )
                    .map_err(|_| corrupt())?;
                let kind = if matches!(operation, AuditOperation::Yank { .. }) {
                    YankActionKind::Yank
                } else {
                    YankActionKind::Unyank
                };
                let action = YankAction::new(
                    name,
                    version,
                    a.request_id.as_deref().ok_or_else(corrupt)?,
                    prior.packages[prior.package(name).map_err(|_| corrupt())?].revision,
                    kind,
                    &a.reason,
                )
                .map_err(|_| corrupt())?;
                let request_digest = action.request_digest()?;
                if a.request_sha256.as_deref() != Some(&request_digest) {
                    return Err(corrupt());
                }
                let (expected, result) = advance_yank(
                    &prior,
                    name,
                    version,
                    kind.yanked(),
                    &current.last_audit_sha256,
                    a.time,
                )
                .map_err(|_| corrupt())?;
                if result.archive_sha256 != *archive_sha256
                    || current != expected
                    || a.result != AuditResult::Yank(result)
                {
                    return Err(corrupt());
                }
                let key = (
                    a.principal.clone(),
                    a.request_id.clone().ok_or_else(corrupt)?,
                );
                let replay = Replay {
                    digest: request_digest,
                    receipt: Receipt {
                        schema: 1,
                        audit_sha256: current.last_audit_sha256.clone(),
                        result: a.result.clone(),
                    },
                };
                if requests.insert(key, replay).is_some() {
                    return Err(corrupt());
                }
                current_hash = prior_hash.into();
                current = prior;
            }
        }
    }
    if reachable
        .iter()
        .any(|key| !inventory.immutable.contains_key(key))
    {
        return Err(corrupt());
    }
    // Walk with only adjacent generations, then reread the original captured head.
    // No Vec<State> proportional to retained 4 GiB history is constructed.
    let state = state(store, &head, identity)?;
    Ok(View {
        head,
        state,
        inventory,
        reachable,
        requests,
    })
}

pub(super) fn advance(
    prior: &State,
    principal: &str,
    name: &str,
    release: Release,
    audit: &str,
    time: u64,
) -> Result<(State, PublishResult), Error> {
    // New publication cannot smuggle a mutable flag into the immutable append.
    if release.yanked || release.quarantined {
        return Err(corrupt());
    }
    let mut next = prior.clone();
    next.revision = next.revision.checked_add(1).ok_or_else(capacity)?;
    if next.revision > records::MAX_EVENTS {
        return Err(capacity());
    }
    next.time_floor = time;
    next.authority.time_floor = time;
    next.last_audit_sha256 = audit.into();
    let package = match next.package(name) {
        Ok(i) => &mut next.packages[i],
        Err(i) => {
            if next.packages.len() >= 1024 {
                return Err(capacity());
            }
            next.packages.insert(
                i,
                records::Package {
                    name: name.into(),
                    revision: 0,
                    releases: Vec::new(),
                },
            );
            next.authority.packages.insert(
                i,
                authority::codec::Package {
                    name: name.into(),
                    owners: vec![principal.into()],
                },
            );
            &mut next.packages[i]
        }
    };
    if package.releases.len() >= 128 {
        return Err(capacity());
    }
    let v = records::version(&release.version)?;
    let index = package
        .releases
        .partition_point(|r| records::version(&r.version).is_ok_and(|old| old < v));
    if package
        .releases
        .get(index)
        .is_some_and(|r| r.version == release.version)
    {
        return Err(conflict());
    }
    package.revision = package.revision.checked_add(1).ok_or_else(capacity)?;
    let result = PublishResult {
        state_revision: next.revision,
        package_revision: package.revision,
        name: name.into(),
        version: release.version.clone(),
        archive_sha256: release.archive_sha256.clone(),
    };
    package.releases.insert(index, release);
    if next
        .packages
        .iter()
        .map(|p| p.releases.len())
        .sum::<usize>()
        > records::MAX_RELEASES
    {
        return Err(capacity());
    }
    Ok((next, result))
}

pub(super) fn advance_yank(
    prior: &State,
    name: &str,
    version: &str,
    yanked: bool,
    audit: &str,
    time: u64,
) -> Result<(State, records::YankResult), Error> {
    let mut next = prior.clone();
    next.revision = next.revision.checked_add(1).ok_or_else(capacity)?;
    if next.revision > records::MAX_EVENTS {
        return Err(capacity());
    }
    next.time_floor = time;
    next.authority.time_floor = time;
    next.last_audit_sha256 = audit.into();
    let i = next.package(name).map_err(|_| conflict())?;
    let package = &mut next.packages[i];
    let release = package
        .releases
        .iter_mut()
        .find(|r| r.version == version)
        .ok_or_else(|| error(ErrorKind::NotFound, "registry release is unavailable"))?;
    if release.yanked == yanked {
        return Err(conflict());
    }
    release.yanked = yanked;
    package.revision = package.revision.checked_add(1).ok_or_else(capacity)?;
    let result = records::YankResult {
        state_revision: next.revision,
        package_revision: package.revision,
        name: name.into(),
        version: version.into(),
        archive_sha256: release.archive_sha256.clone(),
        yanked,
    };
    Ok((next, result))
}
