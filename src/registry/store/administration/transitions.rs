use super::lifecycle::*;
use super::*;

fn active(state: &records::State, principal: &str) -> Result<usize, Error> {
    let i = state
        .authority
        .principals
        .binary_search_by(|p| p.id.as_str().cmp(principal))
        .map_err(|_| error(ErrorKind::NotFound, "registry principal is unavailable"))?;
    if !state.authority.principals[i].active {
        return Err(error(
            ErrorKind::Authorization,
            "registry principal is inactive",
        ));
    }
    Ok(i)
}

pub(super) fn operation(action: &LocalAction) -> Result<records::AuditOperation, Error> {
    use records::AuditOperation as O;
    Ok(match &action.record {
        LocalRecord::Principal(r) => match r.action {
            PrincipalKind::EnrollPrincipal => O::EnrollPrincipal {
                principal: r.principal.clone(),
            },
            PrincipalKind::DeactivatePrincipal => O::DeactivatePrincipal {
                principal: r.principal.clone(),
            },
        },
        LocalRecord::Provision(r) => O::ProvisionCredential {
            credential_id: r.credential.id.clone(),
            record_sha256: digest(&records::encode(&r.credential, 64 * 1024)?),
        },
        LocalRecord::Recovery(r) => O::RecoverOwner {
            name: r.name.clone(),
            principal: r.principal.clone(),
        },
    })
}

pub(super) fn advance(
    prior: &records::State,
    action: &LocalAction,
    operator: &str,
    audit: &str,
    time: u64,
) -> Result<(records::State, records::AuditResult), Error> {
    use records::AuditResult as R;
    check_operator(prior, operator, time)?;
    if action.revision() != prior.revision {
        return Err(conflict());
    }
    action.check_time(time)?;
    let mut next = prior.clone();
    next.revision = prior.revision.checked_add(1).ok_or_else(capacity)?;
    if next.revision > records::MAX_EVENTS {
        return Err(capacity());
    }
    next.time_floor = time;
    next.authority.time_floor = time;
    next.last_audit_sha256 = audit.into();
    let result = match &action.record {
        LocalRecord::Principal(r) => {
            let enabled = matches!(r.action, PrincipalKind::EnrollPrincipal);
            if enabled {
                let i = next
                    .authority
                    .principals
                    .binary_search_by(|p| p.id.cmp(&r.principal))
                    .err()
                    .ok_or_else(conflict)?;
                if next.authority.principals.len() == authority::MAX_PRINCIPALS {
                    return Err(capacity());
                }
                next.authority.principals.insert(
                    i,
                    authority::codec::Principal {
                        id: r.principal.clone(),
                        active: true,
                    },
                );
            } else {
                if r.principal == operator {
                    return Err(conflict());
                }
                let i = prior
                    .authority
                    .principals
                    .binary_search_by(|p| p.id.cmp(&r.principal))
                    .map_err(|_| error(ErrorKind::NotFound, "registry principal is unavailable"))?;
                if !prior.authority.principals[i].active {
                    return Err(conflict());
                }
                if prior.authority.packages.iter().any(|p| {
                    p.owners.contains(&r.principal)
                        && !p
                            .owners
                            .iter()
                            .any(|other| other != &r.principal && active(prior, other).is_ok())
                }) {
                    return Err(conflict());
                }
                next.authority.principals[i].active = false;
            }
            R::Principal(records::PrincipalResult {
                state_revision: next.revision,
                principal: r.principal.clone(),
                active: enabled,
            })
        }
        LocalRecord::Provision(r) => {
            let token = &r.credential;
            active(prior, &token.principal)?;
            let i = prior
                .authority
                .tokens
                .binary_search_by(|t| t.id.cmp(&token.id))
                .err()
                .ok_or_else(conflict)?;
            if prior
                .authority
                .tokens
                .iter()
                .any(|t| t.authenticator_sha256 == token.authenticator_sha256)
            {
                return Err(conflict());
            }
            if prior.authority.tokens.len() == authority::MAX_TOKENS {
                return Err(capacity());
            }
            next.authority.tokens.insert(i, token.clone());
            R::Provision(records::ProvisionResult {
                state_revision: next.revision,
                credential_id: token.id.clone(),
                principal: token.principal.clone(),
            })
        }
        LocalRecord::Recovery(r) => {
            active(prior, &r.principal)?;
            let i = prior
                .package(&r.name)
                .map_err(|_| error(ErrorKind::NotFound, "registry package is unavailable"))?;
            if prior.packages[i].revision != r.expected_package_revision
                || prior.authority.packages[i].owners == [r.principal.clone()]
            {
                return Err(conflict());
            }
            next.authority.packages[i].owners = vec![r.principal.clone()];
            next.packages[i].revision = next.packages[i]
                .revision
                .checked_add(1)
                .ok_or_else(capacity)?;
            R::Recovery(records::RecoveryResult {
                state_revision: next.revision,
                package_revision: next.packages[i].revision,
                name: r.name.clone(),
                recovered_principal: r.principal.clone(),
            })
        }
    };
    // Includes exact configured audience/origin, full retained collection limits,
    // unique authenticator hashes and an active owner for every package.
    authority::codec::validate(&next.authority).map_err(Error::from)?;
    Ok((next, result))
}

pub(in crate::registry::store) fn validate_local_history(
    prior: &records::State,
    current: &records::State,
    audit: &records::Audit,
) -> Result<String, Error> {
    use records::AuditOperation as O;
    let request = audit.request_id.as_deref().ok_or_else(corrupt)?;
    let action = match &audit.operation {
        O::EnrollPrincipal { principal } => {
            Enrollment::new(principal, request, prior.revision, &audit.reason)?.0
        }
        O::DeactivatePrincipal { principal } => {
            Deactivation::new(principal, request, prior.revision, &audit.reason)?.0
        }
        O::ProvisionCredential {
            credential_id,
            record_sha256,
        } => {
            let i = current
                .authority
                .tokens
                .binary_search_by(|t| t.id.cmp(credential_id))
                .map_err(|_| corrupt())?;
            let bytes = records::encode(&current.authority.tokens[i], 64 * 1024)?;
            if digest(&bytes) != *record_sha256 {
                return Err(corrupt());
            }
            CredentialProvision::new(&bytes, request, prior.revision, &audit.reason)?.0
        }
        O::RecoverOwner { name, principal } => {
            let i = prior.package(name).map_err(|_| corrupt())?;
            OwnerRecovery::new(
                name,
                principal,
                request,
                prior.revision,
                prior.packages[i].revision,
                &audit.reason,
            )?
            .0
        }
        _ => return Err(corrupt()),
    };
    let hash = action.request_digest()?;
    if audit.request_sha256.as_deref() != Some(&hash) {
        return Err(corrupt());
    }
    let (expected, result) = advance(
        prior,
        &action,
        &audit.principal,
        &current.last_audit_sha256,
        audit.time,
    )?;
    if *current != expected || audit.result != result {
        return Err(corrupt());
    }
    Ok(hash)
}
