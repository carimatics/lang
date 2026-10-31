use super::*;
mod admission;

const B: &str = "00000000000000000000000000000002";
const INACTIVE: &str = "00000000000000000000000000000003";
const NEW: &str = "00000000000000000000000000000004";
const MODES: [&str; 4] = ["enroll", "provision", "deactivate", "recover"];
fn second_bearer() -> Bearer {
    Bearer::from_input("22".repeat(32).as_bytes()).unwrap()
}
fn token(principal: &str, id: &str, name: &str) -> Vec<u8> {
    use authority::codec::{Action, Grant};
    let r: authority::codec::Records =
        records::decode(&authority_bytes(), authority::MAX_BYTES).unwrap();
    let mut token = r.tokens[0].clone();
    token.id = id.into();
    token.principal = principal.into();
    token.authenticator_sha256 = second_bearer().authenticator_sha256();
    token.grants = vec![
        Grant {
            action: Action::Owners,
            name: name.into(),
        },
        Grant {
            action: Action::Publish,
            name: name.into(),
        },
    ];
    records::encode(&token, 64 * 1024).unwrap()
}

fn prepared(parent: &Dir) {
    prepared_with(parent, |_| {});
}
fn prepared_with(parent: &Dir, configure: impl FnOnce(&mut authority::codec::Records)) {
    use authority::codec::{Action, Grant, Principal};
    let mut r: authority::codec::Records =
        records::decode(&authority_bytes(), authority::MAX_BYTES).unwrap();
    r.principals.push(Principal {
        id: B.into(),
        active: true,
    });
    r.principals.push(Principal {
        id: INACTIVE.into(),
        active: false,
    });
    r.tokens[0].grants = [
        Action::Admin,
        Action::Claim,
        Action::Owners,
        Action::Publish,
        Action::Yank,
    ]
    .into_iter()
    .map(|action| Grant {
        action,
        name: "answer".into(),
    })
    .collect();
    configure(&mut r);
    let bytes = records::encode(&r, authority::MAX_BYTES).unwrap();
    let mut registry =
        Registry::initialize(parent, "registry", &identity(), &bytes, PRINCIPAL, &At(100)).unwrap();
    registry
        .publish(
            &bearer(),
            &archive("0.1.0"),
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
}
fn apply_mode(parent: &Dir, mode: &str, clock: &impl Clock) -> Result<Receipt, Error> {
    apply_mode_with(parent, mode, clock, || {})
}
fn apply_mode_with(
    parent: &Dir,
    mode: &str,
    clock: &impl Clock,
    before: impl FnOnce(),
) -> Result<Receipt, Error> {
    let request = "9".repeat(32);
    if mode == "recover" {
        let mut recovery =
            RecoveryAdministrator::open(parent, "registry", &identity(), PRINCIPAL, "answer")?;
        before();
        return recovery.recover_owner(
            &OwnerRecovery::new("answer", B, &request, 2, 1, "local lifecycle")?,
            clock,
        );
    }
    let mut admin = Administrator::open(parent, "registry", &identity(), PRINCIPAL)?;
    before();
    match mode {
        "enroll" => admin.enroll(
            &Enrollment::new(NEW, &request, 2, "local lifecycle")?,
            clock,
        ),
        "provision" => admin.provision(
            &CredentialProvision::new(
                &token(B, &"02".repeat(16), "answer"),
                &request,
                2,
                "local lifecycle",
            )?,
            clock,
        ),
        "deactivate" => admin.deactivate(
            &Deactivation::new(B, &request, 2, "local lifecycle")?,
            clock,
        ),
        _ => panic!("unknown local fixture"),
    }
}
fn changed_token(change: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
    let mut value: serde_json::Value =
        serde_json::from_slice(&token(B, &"02".repeat(16), "answer")).unwrap();
    change(&mut value);
    let record: authority::codec::Token = serde_json::from_value(value).unwrap();
    records::encode(&record, 64 * 1024).unwrap()
}

#[test]
fn local_administration_refuses_invalid_targets_identity_time_and_noops_before_writes() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    prepared(&parent);
    let before = snapshot(tmp.path());
    let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
    for target in [PRINCIPAL, B, INACTIVE] {
        assert_eq!(
            admin
                .enroll(
                    &Enrollment::new(target, &"a".repeat(32), 2, "duplicate principal").unwrap(),
                    &At(110)
                )
                .unwrap_err()
                .kind,
            ErrorKind::Conflict
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
    for target in [PRINCIPAL, INACTIVE, "ffffffffffffffffffffffffffffffff"] {
        assert!(
            admin
                .deactivate(
                    &Deactivation::new(target, &"b".repeat(32), 2, "invalid target").unwrap(),
                    &At(110)
                )
                .is_err()
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
    for bytes in [
        changed_token(|t| t["principal"] = INACTIVE.into()),
        changed_token(|t| t["principal"] = NEW.into()),
        changed_token(|t| t["registry"] = "another".into()),
        changed_token(|t| t["origin"] = "https://elsewhere.example".into()),
        changed_token(|t| t["issued"] = 111.into()),
        changed_token(|t| t["expires"] = 110.into()),
        changed_token(|t| t["id"] = "01".repeat(16).into()),
        changed_token(|t| t["authenticator_sha256"] = bearer().authenticator_sha256().into()),
        changed_token(|t| t["revoked"] = true.into()),
        changed_token(|t| t["expires"] = (100 + authority::MAX_LIFETIME + 1).into()),
        changed_token(|t| t["grants"] = serde_json::json!([])),
        changed_token(
            |t| t["grants"] = serde_json::json!([{"action":"publish","name":"answer"},{"action":"owners","name":"answer"}]),
        ),
    ] {
        let result = CredentialProvision::new(&bytes, &"c".repeat(32), 2, "invalid provision")
            .and_then(|action| admin.provision(&action, &At(110)));
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().outcome,
            PublicationOutcome::NotPublished
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
    drop(admin);
    let mut scoped =
        RecoveryAdministrator::open(&parent, "registry", &identity(), PRINCIPAL, "answer").unwrap();
    for (name, target, revision, package_revision) in [
        ("elsewhere", B, 2, 1),
        ("answer", PRINCIPAL, 2, 1),
        ("answer", B, 1, 1),
        ("answer", B, 2, 2),
        ("answer", INACTIVE, 2, 1),
        ("answer", NEW, 2, 1),
    ] {
        assert!(
            scoped
                .recover_owner(
                    &OwnerRecovery::new(
                        name,
                        target,
                        &"d".repeat(32),
                        revision,
                        package_revision,
                        "invalid recovery"
                    )
                    .unwrap(),
                    &At(110)
                )
                .is_err()
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
    drop(scoped);
    assert_eq!(
        Registry::open(&parent, "registry", &identity())
            .unwrap()
            .revision()
            .unwrap(),
        2
    );
}

struct Times {
    values: Vec<u64>,
    at: Cell<usize>,
}
impl Clock for Times {
    fn now(&self) -> Result<u64, Error> {
        let i = self.at.get();
        self.at.set(i + 1);
        Ok(self.values[i.min(self.values.len() - 1)])
    }
}
#[test]
fn final_local_time_and_unknown_outcome_preserve_reconciliation() {
    for mode in MODES {
        for values in [
            vec![110, 120, 119],
            vec![120, 110, 120],
            vec![100, 110, 120],
        ] {
            let tmp = Temp::new();
            let parent = parent(tmp.path());
            prepared(&parent);
            assert!(
                apply_mode(
                    &parent,
                    mode,
                    &Times {
                        values,
                        at: Cell::new(0)
                    }
                )
                .is_err()
            );
            let registry = Registry::open(&parent, "registry", &identity()).unwrap();
            assert_eq!(registry.revision().unwrap(), 2);
        }
        for (fault, committed) in [
            (Fault::Before, false),
            (Fault::After, true),
            (Fault::Unreadable, true),
        ] {
            let tmp = Temp::new();
            let parent = parent(tmp.path());
            prepared(&parent);
            FAULT.with(|f| f.set(Some(fault)));
            let result = apply_mode(&parent, mode, &At(110));
            match fault {
                Fault::Before => assert_eq!(
                    result.unwrap_err().outcome,
                    PublicationOutcome::NotPublished
                ),
                Fault::After => assert_eq!(result.unwrap().state_revision(), 3),
                Fault::Unreadable => {
                    assert_eq!(result.unwrap_err().outcome, PublicationOutcome::Unknown);
                    assert!(Registry::open(&parent, "registry", &identity()).is_err());
                    let bytes = FAULT_HEAD.with(|h| h.borrow_mut().take().unwrap());
                    parent
                        .open_dir("registry")
                        .unwrap()
                        .write("HEAD", &bytes)
                        .unwrap();
                }
            }
            let registry = Registry::open(&parent, "registry", &identity()).unwrap();
            assert_eq!(registry.revision().unwrap(), if committed { 3 } else { 2 });
            drop(registry);
            let receipt = apply_mode(&parent, mode, &At(111)).unwrap();
            assert_eq!(receipt.state_revision(), 3);
            assert_eq!(apply_mode(&parent, mode, &At(112)).unwrap(), receipt);
        }
    }
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    prepared(&parent);
    assert_eq!(
        apply_mode(
            &parent,
            "provision",
            &Times {
                values: vec![110, 120, 200],
                at: Cell::new(0)
            }
        )
        .unwrap_err()
        .kind,
        ErrorKind::Authorization
    );
    let registry = Registry::open(&parent, "registry", &identity()).unwrap();
    assert_eq!(registry.revision().unwrap(), 2);
    assert_eq!(registry.recover().unwrap().state.authority.tokens.len(), 1);
}

#[test]
fn local_process_child() {
    let Some(root) = std::env::var_os("LANG_LOCAL_ADMIN_TEST_ROOT") else {
        return;
    };
    let parent = parent(std::path::Path::new(&root));
    let mode = std::env::var("LANG_LOCAL_ADMIN_TEST_MODE").unwrap();
    let cut = std::env::var("LANG_LOCAL_ADMIN_TEST_CUT")
        .unwrap()
        .parse()
        .unwrap();
    apply_mode_with(&parent, &mode, &At(110), || {
        CUT.with(|c| c.set(Some((cut, 0))))
    })
    .unwrap();
    println!(
        "LOCAL_ADMIN_CHECKPOINTS {}",
        CUT.with(|c| c.get().unwrap().1)
    );
}

#[test]
fn local_request_identity_is_global_and_replay_never_reapplies_another_operation() {
    for first in MODES {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        prepared(&parent);
        let original = apply_mode(&parent, first, &At(110)).unwrap();
        let before = snapshot(tmp.path());
        for second in MODES {
            let result = apply_mode(&parent, second, &At(111));
            if first == second {
                assert_eq!(result.unwrap(), original);
            } else {
                assert_eq!(result.unwrap_err().kind, ErrorKind::Conflict);
            }
            assert_eq!(snapshot(tmp.path()), before);
        }
    }
}
fn interrupted(root: &std::path::Path, mode: &str, cut: usize) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "registry::store::tests::local_admin::local_process_child",
            "--nocapture",
        ])
        .env("LANG_LOCAL_ADMIN_TEST_ROOT", root)
        .env("LANG_LOCAL_ADMIN_TEST_MODE", mode)
        .env("LANG_LOCAL_ADMIN_TEST_CUT", cut.to_string())
        .output()
        .unwrap()
}
#[test]
fn every_local_lifecycle_process_cut_preserves_complete_generation_and_exact_source() {
    for mode in MODES {
        let tmp = Temp::new();
        prepared(&parent(tmp.path()));
        let complete = interrupted(tmp.path(), mode, usize::MAX);
        assert!(
            complete.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&complete.stdout)
        );
        assert!(String::from_utf8_lossy(&complete.stdout).contains("LOCAL_ADMIN_CHECKPOINTS 18"));
        for cut in 1..=18 {
            let tmp = Temp::new();
            let parent = parent(tmp.path());
            prepared(&parent);
            let result = interrupted(tmp.path(), mode, cut);
            assert_eq!(
                result.status.code(),
                Some(86),
                "{mode} cut {cut}: {}",
                String::from_utf8_lossy(&result.stdout)
            );
            let registry = Registry::open(&parent, "registry", &identity()).unwrap();
            let committed = cut == 18;
            let state = registry.recover().unwrap().state;
            assert_eq!(state.revision, 2 + u64::from(committed));
            match mode {
                "enroll" => assert_eq!(
                    state
                        .authority
                        .principals
                        .iter()
                        .any(|p| p.id == NEW && p.active),
                    committed
                ),
                "provision" => assert_eq!(state.authority.tokens.len(), 1 + usize::from(committed)),
                "deactivate" => assert_eq!(state.authority.principals[1].active, !committed),
                "recover" => {
                    assert_eq!(
                        state.authority.packages[0].owners,
                        [if committed { B } else { PRINCIPAL }]
                    );
                    assert_eq!(state.packages[0].revision, 1 + u64::from(committed));
                }
                _ => unreachable!(),
            }
            let original = archive("0.1.0");
            let admitted = registry.archive(original.sha256()).unwrap();
            assert_eq!(admitted.as_bytes(), original.as_bytes());
            yank::execute(&admitted);
            drop(registry);
            let receipt = apply_mode(&parent, mode, &At(111)).unwrap();
            assert_eq!(receipt.state_revision(), 3);
            assert_eq!(apply_mode(&parent, mode, &At(112)).unwrap(), receipt);
            assert_eq!(
                std::fs::read_dir(tmp.path().join("registry/staging"))
                    .unwrap()
                    .count(),
                0
            );
        }
    }
    println!("LOCAL_ADMIN_PROCESS_CUTS 72; old 68; new 4");
}

#[test]
fn rehashed_local_histories_refuse_hidden_changes_and_fully_reconstructed_noops() {
    use records::{AuditOperation as O, AuditResult as R};
    for mode in MODES {
        for case in 0..10 {
            let tmp = Temp::new();
            let parent = parent(tmp.path());
            prepared(&parent);
            apply_mode(&parent, mode, &At(110)).unwrap();
            let registry = Registry::open(&parent, "registry", &identity()).unwrap();
            let mut state = registry.recover().unwrap().state;
            let mut audit: records::Audit = records::decode(
                &registry
                    .storage
                    .immutable(storage::Space::Audit, &state.last_audit_sha256)
                    .unwrap(),
                records::MAX_AUDIT,
            )
            .unwrap();
            match case {
                0 => audit.reason = "changed explanation".into(),
                1 => audit.request_sha256 = Some("ab".repeat(32)),
                2 => audit.principal = INACTIVE.into(),
                3 => audit.credential_id = Some("01".repeat(16)),
                4 => state.packages[0].releases[0].quarantined = true,
                5 => state.authority.tokens[0].revoked = true,
                6 => {
                    state.time_floor += 1;
                    state.authority.time_floor += 1;
                }
                7 => match &mut audit.result {
                    R::Principal(r) => r.principal = "f".repeat(32),
                    R::Provision(r) => r.principal = "f".repeat(32),
                    R::Recovery(r) => r.recovered_principal = "f".repeat(32),
                    _ => unreachable!(),
                },
                8 => match mode {
                    "enroll" => state.authority.principals.last_mut().unwrap().active = false,
                    "provision" => state.authority.tokens[1].authenticator_sha256 = "ff".repeat(32),
                    "deactivate" => state.authority.principals[1].active = true,
                    "recover" => state.authority.packages[0]
                        .owners
                        .insert(0, PRINCIPAL.into()),
                    _ => unreachable!(),
                },
                9 => {
                    // Rebuild every operation/result/body hash around an already
                    // satisfied target. Hash-chain validity is not a transition.
                    let request = "9".repeat(32);
                    let hash = match (&mut audit.result, mode) {
                        (R::Principal(r), "enroll") => {
                            state.authority.principals.retain(|p| p.id != NEW);
                            audit.operation = O::EnrollPrincipal {
                                principal: B.into(),
                            };
                            r.principal = B.into();
                            Enrollment::new(B, &request, 2, &audit.reason)
                                .unwrap()
                                .request_digest()
                                .unwrap()
                        }
                        (R::Provision(r), "provision") => {
                            state.authority.tokens.truncate(1);
                            let bytes =
                                records::encode(&state.authority.tokens[0], 64 * 1024).unwrap();
                            audit.operation = O::ProvisionCredential {
                                credential_id: "01".repeat(16),
                                record_sha256: digest(&bytes),
                            };
                            r.credential_id = "01".repeat(16);
                            r.principal = PRINCIPAL.into();
                            CredentialProvision::new(&bytes, &request, 2, &audit.reason)
                                .unwrap()
                                .request_digest()
                                .unwrap()
                        }
                        (R::Principal(r), "deactivate") => {
                            state.authority.principals[1].active = true;
                            audit.operation = O::DeactivatePrincipal {
                                principal: INACTIVE.into(),
                            };
                            r.principal = INACTIVE.into();
                            Deactivation::new(INACTIVE, &request, 2, &audit.reason)
                                .unwrap()
                                .request_digest()
                                .unwrap()
                        }
                        (R::Recovery(r), "recover") => {
                            state.authority.packages[0].owners = vec![PRINCIPAL.into()];
                            audit.operation = O::RecoverOwner {
                                name: "answer".into(),
                                principal: PRINCIPAL.into(),
                            };
                            r.recovered_principal = PRINCIPAL.into();
                            OwnerRecovery::new("answer", PRINCIPAL, &request, 2, 1, &audit.reason)
                                .unwrap()
                                .request_digest()
                                .unwrap()
                        }
                        _ => unreachable!(),
                    };
                    audit.request_sha256 = Some(hash);
                }
                _ => unreachable!(),
            }
            let bytes = records::encode(&audit, records::MAX_AUDIT).unwrap();
            let hash = digest(&bytes);
            registry
                .storage
                .install(storage::Space::Audit, &hash, &bytes)
                .unwrap();
            state.last_audit_sha256 = hash;
            let bytes = records::encode(&state, records::MAX_STATE).unwrap();
            let hash = digest(&bytes);
            registry
                .storage
                .install(storage::Space::State, &hash, &bytes)
                .unwrap();
            registry
                .storage
                .replace_head(&hash, &head_bytes(&hash).unwrap(), || Ok(()))
                .unwrap();
            drop(registry);
            std::fs::write(
                tmp.path()
                    .join("registry/staging")
                    .join(format!("audit-{}", "f".repeat(64))),
                b"partial",
            )
            .unwrap();
            refuse_without_cleanup(&tmp, &parent);
        }
    }
}

#[test]
fn retained_principal_and_credential_limits_admit_equality_and_refuse_reuse() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    prepared_with(&parent, |r| {
        for i in 4..4096 {
            r.principals.push(authority::codec::Principal {
                id: format!("{i:032x}"),
                active: false,
            });
        }
        assert_eq!(r.principals.len(), 4095);
    });
    let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
    admin
        .enroll(
            &Enrollment::new(&"e".repeat(32), &"a".repeat(32), 2, "last principal slot").unwrap(),
            &At(110),
        )
        .unwrap();
    let before = snapshot(tmp.path());
    assert_eq!(
        admin
            .enroll(
                &Enrollment::new(&"f".repeat(32), &"b".repeat(32), 3, "principal capacity")
                    .unwrap(),
                &At(111)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Limit
    );
    assert_eq!(snapshot(tmp.path()), before);
    drop(admin);
    let full = Registry::open(&parent, "registry", &identity()).unwrap();
    assert_eq!(
        full.recover().unwrap().state.authority.principals.len(),
        4096
    );
    drop(full);

    let tmp = Temp::new();
    let parent = super::parent(tmp.path());
    prepared_with(&parent, |r| {
        let template = r.tokens[0].clone();
        for i in 1000u32..5094 {
            let mut token = template.clone();
            token.id = format!("{i:032x}");
            token.authenticator_sha256 = digest(&i.to_le_bytes());
            r.tokens.push(token);
        }
        r.tokens.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(r.tokens.len(), 4095);
    });
    let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
    let provision = CredentialProvision::new(
        &token(B, &"02".repeat(16), "answer"),
        &"a".repeat(32),
        2,
        "last credential slot",
    )
    .unwrap();
    admin.provision(&provision, &At(110)).unwrap();
    let bytes = changed_token(|t| {
        t["id"] = "03".repeat(16).into();
        t["authenticator_sha256"] = Bearer::from_input("33".repeat(32).as_bytes())
            .unwrap()
            .authenticator_sha256()
            .into();
    });
    let before = snapshot(tmp.path());
    assert_eq!(
        admin
            .provision(
                &CredentialProvision::new(&bytes, &"b".repeat(32), 3, "credential capacity")
                    .unwrap(),
                &At(111)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Limit
    );
    assert_eq!(snapshot(tmp.path()), before);
    admin
        .revoke(
            &Revocation::new(&"02".repeat(16), &"c".repeat(32), 3, "retained credential").unwrap(),
            &At(112),
        )
        .unwrap();
    let before = snapshot(tmp.path());
    let reused = changed_token(|t| t["id"] = "03".repeat(16).into());
    assert_eq!(
        admin
            .provision(
                &CredentialProvision::new(&reused, &"d".repeat(32), 4, "reused revoked hash")
                    .unwrap(),
                &At(113)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    assert_eq!(
        admin
            .provision(
                &CredentialProvision::new(
                    &token(B, &"02".repeat(16), "answer"),
                    &"e".repeat(32),
                    4,
                    "reused revoked identity"
                )
                .unwrap(),
                &At(113)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    assert_eq!(snapshot(tmp.path()), before);
    drop(admin);
    assert_eq!(
        Registry::open(&parent, "registry", &identity())
            .unwrap()
            .recover()
            .unwrap()
            .state
            .authority
            .tokens
            .len(),
        4096
    );
}

fn large_token() -> Vec<u8> {
    let mut record: authority::codec::Token =
        records::decode(&token(B, &"02".repeat(16), "answer"), 64 * 1024).unwrap();
    record.grants = (0..128)
        .map(|i| authority::codec::Grant {
            action: authority::codec::Action::Publish,
            name: format!("g{i:03}{}", "x".repeat(59)),
        })
        .collect();
    records::encode(&record, 64 * 1024).unwrap()
}
#[test]
fn large_provision_history_reuses_metered_workspace_at_refusal_equality_and_plus_one() {
    use crate::registry::{budget::Budget, transport::MAX_REQUEST_MEMORY};
    for adjustment in [-1isize, 0, 1] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        prepared(&parent);
        let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
        let action =
            CredentialProvision::new(&large_token(), &"a".repeat(32), 2, "full grant record")
                .unwrap();
        assert!(action.as_bytes().len() > records::MAX_AUDIT);
        let receipt = admin.provision(&action, &At(110)).unwrap();
        assert!(receipt.as_bytes().unwrap().len() < records::MAX_AUDIT);
        drop(admin);
        let registry = Registry::open(&parent, "registry", &identity()).unwrap();
        let view = registry.recover().unwrap();
        assert_eq!(view.state.authority.tokens[1].grants.len(), 128);
        assert!(
            registry
                .storage
                .immutable(storage::Space::Audit, &view.state.last_audit_sha256)
                .unwrap()
                .len()
                < records::MAX_AUDIT
        );
        drop(view);
        let budget = Budget::new(MAX_REQUEST_MEMORY);
        let mut metered = MeteredRegistry::with_budget(registry, budget.clone(), &At(110)).unwrap();
        let root = tmp.path().join("registry");
        let state_bytes = std::fs::read_dir(root.join("states"))
            .unwrap()
            .map(|e| e.unwrap().metadata().unwrap().len() as usize)
            .max()
            .unwrap();
        let entries = std::fs::read_dir(&root).unwrap().count()
            + ["states", "audits", "objects", "staging"]
                .iter()
                .map(|n| std::fs::read_dir(root.join(n)).unwrap().count())
                .sum::<usize>();
        let source = archive("0.1.0");
        let request = QuarantineAction::new(
            "answer",
            "0.1.0",
            &"b".repeat(32),
            1,
            QuarantineActionKind::Quarantine,
            "reconstruct provision history",
        )
        .unwrap();
        let candidate = state_bytes + 4096;
        let workspace =
            96 * candidate + 6 * source.as_bytes().len() + 8192 * (entries + 8) + 2 * 1024 * 1024;
        let generation = 2 * candidate + (1024 + 8192) * 1024 + 4096;
        let required = workspace + generation + request.as_bytes().len() + records::MAX_AUDIT;
        let pressure = budget
            .reserve(
                MAX_REQUEST_MEMORY
                    - budget.used()
                    - required.checked_add_signed(adjustment).unwrap(),
            )
            .unwrap();
        let before = snapshot(tmp.path());
        let result = metered.quarantine(&bearer(), "answer", request.as_bytes(), &At(111));
        let receipt = if adjustment < 0 {
            assert_eq!(result.unwrap_err().kind, ErrorKind::Busy);
            assert_eq!(snapshot(tmp.path()), before);
            None
        } else {
            let receipt = result.unwrap();
            assert_eq!(receipt.state_revision(), 4);
            assert!(
                Index::admit(metered.index("answer").unwrap(), &identity(), "answer")
                    .unwrap()
                    .release("0.1.0")
                    .unwrap()
                    .quarantined()
            );
            Some(receipt)
        };
        drop(pressure);
        drop(metered);
        assert_eq!(
            budget.used(),
            if receipt.is_some() {
                records::MAX_AUDIT
            } else {
                0
            }
        );
        drop(receipt);
        assert_eq!(budget.used(), 0);
    }
}

#[test]
fn local_writer_child() {
    let Some(root) = std::env::var_os("LANG_LOCAL_WRITER_ROOT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let parent = parent(root);
    let mode = std::env::var("LANG_LOCAL_WRITER_MODE").unwrap();
    if std::env::var("LANG_LOCAL_WRITER_BUSY").unwrap() == "true" {
        assert_eq!(
            apply_mode(&parent, &mode, &At(110)).unwrap_err().kind,
            ErrorKind::Busy
        );
        return;
    }
    apply_mode_with(&parent, &mode, &At(110), || {
        std::fs::write(root.join("ready"), b"ready").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !root.join("release").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "writer release timeout"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    })
    .unwrap();
}
fn writer(root: &std::path::Path, mode: &str, busy: bool) -> std::process::Child {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "registry::store::tests::local_admin::local_writer_child",
            "--nocapture",
        ])
        .env("LANG_LOCAL_WRITER_ROOT", root)
        .env("LANG_LOCAL_WRITER_MODE", mode)
        .env("LANG_LOCAL_WRITER_BUSY", busy.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap()
}
#[test]
fn local_and_registry_writers_refuse_contention_in_both_orders() {
    for mode in MODES {
        for local_first in [false, true] {
            let tmp = Temp::new();
            let parent = parent(tmp.path());
            prepared(&parent);
            let before = snapshot(&tmp.path().join("registry"));
            if local_first {
                let mut child = writer(tmp.path(), mode, false);
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
                while !tmp.path().join("ready").exists() {
                    assert!(
                        child.try_wait().unwrap().is_none(),
                        "writer exited before acquiring lock"
                    );
                    assert!(
                        std::time::Instant::now() < deadline,
                        "writer startup timeout"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                assert_eq!(
                    Registry::open(&parent, "registry", &identity())
                        .err()
                        .unwrap()
                        .kind,
                    ErrorKind::Busy
                );
                assert_eq!(
                    Administrator::open(&parent, "registry", &identity(), PRINCIPAL)
                        .err()
                        .unwrap()
                        .kind,
                    ErrorKind::Busy
                );
                assert_eq!(
                    RecoveryAdministrator::open(
                        &parent,
                        "registry",
                        &identity(),
                        PRINCIPAL,
                        "answer"
                    )
                    .err()
                    .unwrap()
                    .kind,
                    ErrorKind::Busy
                );
                assert_eq!(snapshot(&tmp.path().join("registry")), before);
                std::fs::write(tmp.path().join("release"), b"release").unwrap();
                let output = child.wait_with_output().unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            } else {
                let registry = Registry::open(&parent, "registry", &identity()).unwrap();
                let output = writer(tmp.path(), mode, true).wait_with_output().unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(snapshot(&tmp.path().join("registry")), before);
                drop(registry);
                apply_mode(&parent, mode, &At(110)).unwrap();
            }
            let registry = Registry::open(&parent, "registry", &identity()).unwrap();
            assert_eq!(registry.revision().unwrap(), 3);
            yank::execute(&registry.archive(archive("0.1.0").sha256()).unwrap());
        }
    }
}

#[test]
fn full_owner_recovery_replaces_all_members_and_preserves_independent_release_flags() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    prepared_with(&parent, |r| {
        r.principals[2].active = true;
        for i in 4..=129 {
            r.principals.push(authority::codec::Principal {
                id: format!("{i:032x}"),
                active: true,
            });
        }
    });
    let mut registry = Registry::open(&parent, "registry", &identity()).unwrap();
    for i in 3u64..=129 {
        registry
            .change_owner(
                &bearer(),
                &OwnerAction::new(
                    "answer",
                    &format!("{i:032x}"),
                    &format!("{:032x}", i + 1000),
                    i - 2,
                    OwnerActionKind::AddOwner,
                    "fill owner capacity",
                )
                .unwrap(),
                &At(110),
            )
            .unwrap();
    }
    assert_eq!(
        registry.recover().unwrap().state.authority.packages[0]
            .owners
            .len(),
        128
    );
    registry
        .yank(
            &bearer(),
            &YankAction::new(
                "answer",
                "0.1.0",
                &"a".repeat(32),
                128,
                YankActionKind::Yank,
                "independent yank",
            )
            .unwrap(),
            &At(111),
        )
        .unwrap();
    registry
        .quarantine(
            &bearer(),
            &QuarantineAction::new(
                "answer",
                "0.1.0",
                &"b".repeat(32),
                129,
                QuarantineActionKind::Quarantine,
                "independent quarantine",
            )
            .unwrap(),
            &At(112),
        )
        .unwrap();
    let prior = registry.recover().unwrap().state;
    assert_eq!(prior.packages[0].revision, 130);
    drop(registry);
    let mut recovery =
        RecoveryAdministrator::open(&parent, "registry", &identity(), PRINCIPAL, "answer").unwrap();
    recovery
        .recover_owner(
            &OwnerRecovery::new(
                "answer",
                B,
                &"c".repeat(32),
                prior.revision,
                130,
                "recover full ownership",
            )
            .unwrap(),
            &At(113),
        )
        .unwrap();
    drop(recovery);
    let registry = Registry::open(&parent, "registry", &identity()).unwrap();
    let next = registry.recover().unwrap().state;
    assert_eq!(next.authority.packages[0].owners, [B]);
    assert_eq!(next.packages[0].revision, 131);
    assert!(next.packages[0].releases == prior.packages[0].releases);
    assert!(next.authority.tokens == prior.authority.tokens);
    assert!(next.packages[0].releases[0].yanked && next.packages[0].releases[0].quarantined);
    let source = archive("0.1.0");
    assert_eq!(
        registry.archive(source.sha256()).err().unwrap().kind,
        ErrorKind::NotFound
    );
    assert_eq!(
        registry
            .storage
            .immutable(storage::Space::Object, source.sha256())
            .unwrap(),
        source.as_bytes()
    );
    drop(registry);
    let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
    let before = snapshot(tmp.path());
    assert_eq!(
        admin
            .deactivate(
                &Deactivation::new(B, &"d".repeat(32), next.revision, "last owner").unwrap(),
                &At(114)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    assert_eq!(snapshot(tmp.path()), before);
}

#[test]
fn local_actions_match_independent_canonical_request_and_record_vectors() {
    let principal = "2".repeat(32);
    let request = "a".repeat(32);
    let enrollment = Enrollment::new(&principal, &request, 2, "local enrollment").unwrap();
    let credential = token(&principal, &"3".repeat(32), "math");
    assert_eq!(
        digest(&credential),
        "88d0536b4c072a7539a2f47033cfb506bf882cae733689fb2ddd6373948e80a0"
    );
    let provision =
        CredentialProvision::new(&credential, &request, 2, "local provisioning").unwrap();
    let deactivation = Deactivation::new(&principal, &request, 2, "local deactivation").unwrap();
    let recovery =
        OwnerRecovery::new("math", &principal, &request, 2, 1, "lost owner recovery").unwrap();
    for (bytes, size, body_hash, actual_request, request_hash) in [
        (
            enrollment.as_bytes(),
            217,
            "221287f9daa126e304d8db8f934ded0dbec9f68ca22dafa610aa1127ad3d2899",
            enrollment.request_digest().unwrap(),
            "9ff59e30149ab10274ba68329b3a47e47a4461679efd40e9afb08af282b8df5e",
        ),
        (
            provision.as_bytes(),
            678,
            "c8377374ff7dd54782c5c159098b59792718b5adcb34841366de365fe706ff1d",
            provision.request_digest().unwrap(),
            "3245b2cc2e4099b16cb30fd190eec75224cfc03b75091abb71e6a485260eb119",
        ),
        (
            deactivation.as_bytes(),
            223,
            "e6613864fa0a88792b3871fa656974e19ac78e24342ec327b987d0caadeb9ed6",
            deactivation.request_digest().unwrap(),
            "fc818213fd0a60c5019f86a4a98dc1318647528e1a2ac89c10fa8aae2e15b324",
        ),
        (
            recovery.as_bytes(),
            269,
            "3a6173608d7c27cf7ba4a2dc24c797544fa63fe69e657cc7200ada2f0b612d33",
            recovery.request_digest().unwrap(),
            "7caba5896de97845111791b062cf48b351db5db9d330caa50f11953db12d6e8d",
        ),
    ] {
        assert_eq!(bytes.len(), size);
        assert_eq!(digest(bytes), body_hash);
        assert_eq!(actual_request, request_hash);
    }
    assert_eq!(
        Enrollment::admit(enrollment.as_bytes()).unwrap().as_bytes(),
        enrollment.as_bytes()
    );
    assert_eq!(
        CredentialProvision::admit(provision.as_bytes())
            .unwrap()
            .as_bytes(),
        provision.as_bytes()
    );
    assert_eq!(
        Deactivation::admit(deactivation.as_bytes())
            .unwrap()
            .as_bytes(),
        deactivation.as_bytes()
    );
    assert_eq!(
        OwnerRecovery::admit(recovery.as_bytes())
            .unwrap()
            .as_bytes(),
        recovery.as_bytes()
    );
    assert!(Enrollment::admit(deactivation.as_bytes()).is_err());
    assert!(Deactivation::admit(enrollment.as_bytes()).is_err());
    let unknown = std::str::from_utf8(provision.as_bytes())
        .unwrap()
        .replace("\"schema\": 1,", "\"schema\": 1,\"bearer\":\"forbidden\",");
    assert!(CredentialProvision::admit(unknown.as_bytes()).is_err());
    let debug = format!("{provision:?}");
    assert!(!debug.contains(&second_bearer().authenticator_sha256()));
    assert!(!debug.contains("credential"));
}

#[test]
fn local_enrollment_provision_recovery_and_deactivation_preserve_source_and_later_state() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut registry = Registry::initialize(
        &parent,
        "registry",
        &identity(),
        &authority_bytes(),
        PRINCIPAL,
        &At(100),
    )
    .unwrap();
    let original = archive("0.1.0");
    registry
        .publish(
            &bearer(),
            &original,
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    yank::execute(&registry.archive(original.sha256()).unwrap());
    drop(registry);
    let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
    let enrollment = Enrollment::new(B, &"a".repeat(32), 2, "local enrollment").unwrap();
    let enrolled = admin.enroll(&enrollment, &At(102)).unwrap();
    assert_eq!(enrolled.state_revision(), 3);
    assert_eq!(enrolled.package_result(), None);
    let token_id = "02".repeat(16);
    let provision = CredentialProvision::new(
        &token(B, &token_id, "answer"),
        &"b".repeat(32),
        3,
        "local provisioning",
    )
    .unwrap();
    let provisioned = admin.provision(&provision, &At(103)).unwrap();
    assert_eq!(provisioned.state_revision(), 4);
    assert_eq!(provisioned.package_result(), None);
    drop(admin);
    let recovery =
        OwnerRecovery::new("answer", B, &"c".repeat(32), 4, 1, "lost owner recovery").unwrap();
    let mut scoped =
        RecoveryAdministrator::open(&parent, "registry", &identity(), PRINCIPAL, "answer").unwrap();
    assert_eq!(scoped.revision().unwrap(), 4);
    assert_eq!(scoped.package_revision().unwrap(), 1);
    let recovered = scoped.recover_owner(&recovery, &At(104)).unwrap();
    assert_eq!(recovered.state_revision(), 5);
    assert_eq!(recovered.package_result(), None);
    drop(scoped);
    let mut registry = Registry::open(&parent, "registry", &identity()).unwrap();
    assert_eq!(
        registry
            .publish(
                &bearer(),
                &original,
                REQUEST,
                ExpectedRevision::Absent,
                &At(105)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Authorization
    );
    let second = archive("0.2.0");
    registry
        .publish(
            &second_bearer(),
            &second,
            &"d".repeat(32),
            ExpectedRevision::Revision(2),
            &At(105),
        )
        .unwrap();
    assert_eq!(
        registry.archive(original.sha256()).unwrap().as_bytes(),
        original.as_bytes()
    );
    yank::execute(&registry.archive(second.sha256()).unwrap());
    registry
        .change_owner(
            &second_bearer(),
            &OwnerAction::new(
                "answer",
                PRINCIPAL,
                &"e".repeat(32),
                3,
                OwnerActionKind::AddOwner,
                "restore second owner",
            )
            .unwrap(),
            &At(106),
        )
        .unwrap();
    assert_eq!(registry.revision().unwrap(), 7);
    drop(registry);
    let mut scoped =
        RecoveryAdministrator::open(&parent, "registry", &identity(), PRINCIPAL, "answer").unwrap();
    assert_eq!(
        scoped.recover_owner(&recovery, &At(107)).unwrap(),
        recovered
    );
    assert_eq!(scoped.revision().unwrap(), 7);
    assert_eq!(scoped.package_revision().unwrap(), 4);
    drop(scoped);
    let mut wrong_scope =
        RecoveryAdministrator::open(&parent, "registry", &identity(), PRINCIPAL, "elsewhere")
            .unwrap();
    assert_eq!(
        wrong_scope
            .recover_owner(&recovery, &At(107))
            .unwrap_err()
            .kind,
        ErrorKind::Authorization
    );
    drop(wrong_scope);
    let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
    admin
        .revoke(
            &Revocation::new(&token_id, &"f".repeat(32), 7, "rotate credential").unwrap(),
            &At(108),
        )
        .unwrap();
    assert_eq!(admin.provision(&provision, &At(109)).unwrap(), provisioned);
    assert_eq!(admin.revision().unwrap(), 8);
    let deactivation = Deactivation::new(B, &"1".repeat(32), 8, "deactivate former owner").unwrap();
    let deactivated = admin.deactivate(&deactivation, &At(110)).unwrap();
    assert_eq!(deactivated.state_revision(), 9);
    assert_eq!(deactivated.package_result(), None);
    assert_eq!(admin.enroll(&enrollment, &At(111)).unwrap(), enrolled);
    assert_eq!(admin.provision(&provision, &At(201)).unwrap(), provisioned);
    assert_eq!(
        admin.deactivate(&deactivation, &At(201)).unwrap(),
        deactivated
    );
    assert_eq!(admin.revision().unwrap(), 9);
    assert_eq!(
        admin
            .enroll(
                &Enrollment::new(B, &"2".repeat(32), 9, "duplicate identity").unwrap(),
                &At(112)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    assert_eq!(
        admin
            .deactivate(
                &Deactivation::new(PRINCIPAL, &"3".repeat(32), 9, "self deactivation").unwrap(),
                &At(112)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    drop(admin);
    assert!(Administrator::open(&parent, "registry", &identity(), B).is_err());
    let mut registry = Registry::open(&parent, "registry", &identity()).unwrap();
    assert_eq!(
        registry
            .publish(
                &second_bearer(),
                &archive("0.3.0"),
                &"4".repeat(32),
                ExpectedRevision::Revision(4),
                &At(112)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Authentication
    );
    let view = registry.recover().unwrap();
    assert_eq!(view.state.authority.packages[0].owners, [PRINCIPAL, B]);
    assert!(!view.state.authority.principals[1].active);
    assert!(view.state.authority.tokens[1].revoked);
    assert_eq!(view.state.packages[0].revision, 4);
    yank::execute(&registry.archive(original.sha256()).unwrap());
}
