use super::*;

const B: &str = "00000000000000000000000000000002";
const ADMIN: &str = "00000000000000000000000000000003";
const INACTIVE: &str = "00000000000000000000000000000004";

fn second_bearer() -> Bearer {
    Bearer::from_input("22".repeat(32).as_bytes()).unwrap()
}
fn admin_bearer() -> Bearer {
    Bearer::from_input("33".repeat(32).as_bytes()).unwrap()
}
fn owner_authority() -> authority::codec::Records {
    use authority::codec::{Action, Grant, Principal};
    let mut r: authority::codec::Records =
        records::decode(&authority_bytes(), authority::MAX_BYTES).unwrap();
    for (id, active) in [(B, true), (ADMIN, true), (INACTIVE, false)] {
        r.principals.push(Principal {
            id: id.into(),
            active,
        });
    }
    r.tokens[0].grants.insert(
        1,
        Grant {
            action: Action::Owners,
            name: "answer".into(),
        },
    );
    for (id, principal, bearer, actions) in [
        (
            "02".repeat(16),
            B,
            second_bearer(),
            vec![Action::Owners, Action::Publish],
        ),
        ("03".repeat(16), ADMIN, admin_bearer(), vec![Action::Admin]),
    ] {
        let mut token = r.tokens[0].clone();
        token.id = id;
        token.principal = principal.into();
        token.authenticator_sha256 = bearer.authenticator_sha256();
        token.grants = actions
            .into_iter()
            .map(|action| Grant {
                action,
                name: "answer".into(),
            })
            .collect();
        r.tokens.push(token);
    }
    r
}
fn initialize_owners(parent: &Dir) -> Registry {
    let bytes = records::encode(&owner_authority(), authority::MAX_BYTES).unwrap();
    Registry::initialize(parent, "registry", &identity(), &bytes, PRINCIPAL, &At(100)).unwrap()
}
fn action(kind: OwnerActionKind, target: &str, id: char, revision: u64) -> OwnerAction {
    OwnerAction::new(
        "answer",
        target,
        &id.to_string().repeat(32),
        revision,
        kind,
        "ownership handoff",
    )
    .unwrap()
}
fn members(registry: &Registry, bearer: &Bearer) -> serde_json::Value {
    serde_json::from_slice(&registry.owners(bearer, "answer", &At(110)).unwrap()).unwrap()
}

#[test]
fn canonical_owner_action_matches_independent_digest_vectors() {
    let a = OwnerAction::new(
        "math",
        &"2".repeat(32),
        &"a".repeat(32),
        1,
        OwnerActionKind::AddOwner,
        "ownership handoff",
    )
    .unwrap();
    assert_eq!(a.as_bytes().len(), 205);
    assert_eq!(
        digest(a.as_bytes()),
        "682954cddbf3597314b8c2f3dfee606ca6a9aec1d7aa9ea9732872fb68f6a5bf"
    );
    assert_eq!(
        a.request_digest().unwrap(),
        "5ab32ddd58e4c0b92843e59999d01f63f1736b5b75a3805542f3a8014034168b"
    );
    assert_eq!(
        OwnerAction::admit("math", a.as_bytes()).unwrap().as_bytes(),
        a.as_bytes()
    );
    assert!(!format!("{a:?}").contains("ownership handoff"));
    let r = Revocation::new(&"2".repeat(32), &"e".repeat(32), 6, "credential rotation").unwrap();
    assert_eq!(r.as_bytes().len(), 225);
    assert_eq!(
        digest(r.as_bytes()),
        "e026380b51d06ee10382aefed48690b5ad6059963248b7cd5d463daec12bce07"
    );
    assert_eq!(
        r.request_digest().unwrap(),
        "0b043b277a723f8e2edea2b4cdf598b11a66baee9243b46791632f3a532e23c1"
    );
    assert_eq!(
        Revocation::admit(r.as_bytes()).unwrap().as_bytes(),
        r.as_bytes()
    );
    assert!(!format!("{r:?}").contains("credential rotation"));
    for (owner, bytes) in [(true, a.as_bytes()), (false, r.as_bytes())] {
        let admit = |input: &[u8]| -> Result<Vec<u8>, Error> {
            if owner {
                Ok(OwnerAction::admit("math", input)?.as_bytes().to_vec())
            } else {
                Ok(Revocation::admit(input)?.as_bytes().to_vec())
            }
        };
        for seed in 0..512 {
            let mut hostile = bytes.to_vec();
            let i = seed % hostile.len();
            hostile[i] ^= 1 << (seed % 8);
            match admit(&hostile) {
                Ok(accepted) => assert_eq!(accepted, hostile),
                Err(error) => assert!(error.to_string().len() < 128),
            }
        }
        let text = std::str::from_utf8(bytes).unwrap();
        let reason = if owner {
            "ownership handoff"
        } else {
            "credential rotation"
        };
        for hostile in [
            format!("{text}\n"),
            text.replace("\"schema\": 1", "\"schema\": 2"),
            text.replace("\"schema\": 1,", "\"schema\": 1,\n  \"schema\": 1,"),
            text.replace(
                "\"schema\": 1,",
                "\"schema\": 1,\n  \"version\": \"1.0.0\",",
            ),
            text.replace(reason, ""),
            text.replace(reason, "line\\nbreak"),
            text.replace(&"2".repeat(32), &"A".repeat(32)),
            text.replace("revision\": 1", "revision\": 0")
                .replace("revision\": 6", "revision\": 0"),
            text.replace("\"action\": \"add_owner\"", "\"action\": null")
                .replace("\"action\": \"revoke_credential\"", "\"action\": null"),
        ] {
            assert_ne!(hostile, text);
            assert_eq!(
                admit(hostile.as_bytes()).unwrap_err().kind,
                ErrorKind::Malformed
            );
        }
        assert_eq!(admit(&vec![b' '; 2049]).unwrap_err().kind, ErrorKind::Limit);
    }
    for reason in ["x".repeat(256), "é".repeat(128)] {
        assert!(
            OwnerAction::new("math", B, REQUEST, 1, OwnerActionKind::AddOwner, &reason).is_ok()
        );
        assert!(Revocation::new(B, REQUEST, 1, &reason).is_ok());
    }
    for reason in ["x".repeat(257), "é".repeat(129), "control\u{85}".into()] {
        assert_eq!(
            OwnerAction::new("math", B, REQUEST, 1, OwnerActionKind::AddOwner, &reason)
                .unwrap_err()
                .kind,
            ErrorKind::Malformed
        );
        assert_eq!(
            Revocation::new(B, REQUEST, 1, &reason).unwrap_err().kind,
            ErrorKind::Malformed
        );
    }
}

#[test]
fn owner_scope_targets_and_last_active_member_are_checked_before_writes() {
    for scoped in [true, false] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut authority = owner_authority();
        if !scoped {
            authority.tokens[0]
                .grants
                .retain(|g| g.action != authority::codec::Action::Owners);
        }
        let bytes = records::encode(&authority, authority::MAX_BYTES).unwrap();
        let mut registry = Registry::initialize(
            &parent,
            "registry",
            &identity(),
            &bytes,
            PRINCIPAL,
            &At(100),
        )
        .unwrap();
        registry
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(101),
            )
            .unwrap();
        // Membership reads need no mutation grant.
        assert_eq!(
            members(&registry, &bearer())["owners"],
            serde_json::json!([PRINCIPAL])
        );
        let before = snapshot(tmp.path());
        for (kind, target, revision, expected) in [
            (OwnerActionKind::AddOwner, INACTIVE, 1, ErrorKind::NotFound),
            (
                OwnerActionKind::AddOwner,
                "ffffffffffffffffffffffffffffffff",
                1,
                ErrorKind::NotFound,
            ),
            (OwnerActionKind::AddOwner, PRINCIPAL, 1, ErrorKind::Conflict),
            (OwnerActionKind::RemoveOwner, B, 1, ErrorKind::Conflict),
            (
                OwnerActionKind::RemoveOwner,
                PRINCIPAL,
                1,
                ErrorKind::Conflict,
            ),
            (OwnerActionKind::AddOwner, B, 2, ErrorKind::Conflict),
        ] {
            let result = registry
                .change_owner(&bearer(), &action(kind, target, 'a', revision), &At(110))
                .unwrap_err();
            assert_eq!(
                result.kind,
                if scoped {
                    expected
                } else {
                    ErrorKind::Authorization
                }
            );
            assert_eq!(result.outcome, PublicationOutcome::NotPublished);
            assert_eq!(snapshot(tmp.path()), before);
        }
        // Test this valid historical-policy shape directly. Actual audited
        // deactivation journeys are covered by the local_admin fixtures.
        let mut prior = registry.recover().unwrap().state;
        prior.authority.packages[0].owners.push(INACTIVE.into());
        prior.validate(&identity()).unwrap();
        assert!(matches!(
            history::advance_owner(&prior, "answer", PRINCIPAL, false, "", 110),
            Err(Error {
                kind: ErrorKind::Conflict,
                ..
            })
        ));
        let (next, _) = history::advance_owner(&prior, "answer", INACTIVE, false, "", 110).unwrap();
        assert_eq!(next.authority.packages[0].owners, [PRINCIPAL]);
    }
}

#[test]
fn actual_owner_capacity_is_sorted_bounded_and_reusable_after_removal() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut authority = owner_authority();
    authority.principals = (1..=129)
        .map(|id| authority::codec::Principal {
            id: format!("{id:032x}"),
            active: true,
        })
        .collect();
    let bytes = records::encode(&authority, authority::MAX_BYTES).unwrap();
    let mut registry = Registry::initialize(
        &parent,
        "registry",
        &identity(),
        &bytes,
        PRINCIPAL,
        &At(100),
    )
    .unwrap();
    registry
        .publish(
            &bearer(),
            &archive("0.1.0"),
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    for id in 2..=128 {
        let action = OwnerAction::new(
            "answer",
            &format!("{id:032x}"),
            &format!("{:032x}", 1000 + id),
            id - 1,
            OwnerActionKind::AddOwner,
            "membership capacity",
        )
        .unwrap();
        registry.change_owner(&bearer(), &action, &At(110)).unwrap();
    }
    let encoded = registry.owners(&bearer(), "answer", &At(110)).unwrap();
    assert!(encoded.len() <= MAX_OWNER_BYTES);
    let list: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    let owners = list["owners"].as_array().unwrap();
    assert_eq!(owners.len(), 128);
    for (i, id) in owners.iter().enumerate() {
        assert_eq!(id, &format!("{:032x}", i + 1));
    }
    let before = snapshot(tmp.path());
    let extra = action(
        OwnerActionKind::AddOwner,
        &format!("{:032x}", 129),
        'a',
        128,
    );
    assert_eq!(
        registry
            .change_owner(&bearer(), &extra, &At(110))
            .unwrap_err()
            .kind,
        ErrorKind::Limit
    );
    assert_eq!(snapshot(tmp.path()), before);
    registry
        .change_owner(
            &bearer(),
            &action(OwnerActionKind::RemoveOwner, B, 'b', 128),
            &At(110),
        )
        .unwrap();
    registry
        .change_owner(
            &bearer(),
            &action(
                OwnerActionKind::AddOwner,
                &format!("{:032x}", 129),
                'c',
                129,
            ),
            &At(110),
        )
        .unwrap();
    drop(registry);
    let registry = reopen(&parent);
    assert_eq!(
        members(&registry, &bearer())["owners"]
            .as_array()
            .unwrap()
            .len(),
        128
    );
    super::yank::execute(&registry.archive(archive("0.1.0").sha256()).unwrap());
}

fn prepared(root: &std::path::Path, mode: &str) {
    let parent = parent(root);
    let mut registry = initialize_owners(&parent);
    registry
        .publish(
            &bearer(),
            &archive("0.1.0"),
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    if matches!(mode, "remove" | "self") {
        registry
            .change_owner(
                &bearer(),
                &action(OwnerActionKind::AddOwner, B, 'a', 1),
                &At(110),
            )
            .unwrap();
    }
}
fn process_action(mode: &str) -> OwnerAction {
    match mode {
        "add" => action(OwnerActionKind::AddOwner, B, 'b', 1),
        "remove" => action(OwnerActionKind::RemoveOwner, B, 'b', 2),
        "self" => action(OwnerActionKind::RemoveOwner, PRINCIPAL, 'b', 2),
        _ => panic!("unknown owner mode"),
    }
}
fn process_revocation() -> Revocation {
    Revocation::new(&"01".repeat(16), &"b".repeat(32), 2, "credential rotation").unwrap()
}
#[test]
fn control_child() {
    let Some(root) = std::env::var_os("LANG_OWNER_TEST_ROOT") else {
        return;
    };
    let mode = std::env::var("LANG_OWNER_TEST_MODE").unwrap();
    let cut = std::env::var("LANG_OWNER_TEST_CUT")
        .unwrap()
        .parse()
        .unwrap();
    let parent = parent(std::path::Path::new(&root));
    if mode == "revoke" {
        let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
        CUT.with(|c| c.set(Some((cut, 0))));
        admin.revoke(&process_revocation(), &At(111)).unwrap();
    } else {
        let mut registry = reopen(&parent);
        CUT.with(|c| c.set(Some((cut, 0))));
        registry
            .change_owner(&bearer(), &process_action(&mode), &At(111))
            .unwrap();
    }
    println!("CONTROL_CHECKPOINTS {}", CUT.with(|c| c.get().unwrap().1));
}
fn interrupted(root: &std::path::Path, mode: &str, cut: usize) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "registry::store::tests::owners::control_child",
            "--nocapture",
        ])
        .env("LANG_OWNER_TEST_ROOT", root)
        .env("LANG_OWNER_TEST_MODE", mode)
        .env("LANG_OWNER_TEST_CUT", cut.to_string())
        .output()
        .unwrap()
}
#[test]
fn every_owner_and_local_revocation_process_cut_retains_complete_authority() {
    for mode in ["add", "remove", "self", "revoke"] {
        let tmp = Temp::new();
        prepared(tmp.path(), mode);
        let complete = interrupted(tmp.path(), mode, usize::MAX);
        assert!(
            complete.status.success(),
            "{}",
            String::from_utf8_lossy(&complete.stdout)
        );
        assert!(String::from_utf8_lossy(&complete.stdout).contains("CONTROL_CHECKPOINTS 18"));
        for cut in 1..=18 {
            let tmp = Temp::new();
            prepared(tmp.path(), mode);
            let output = interrupted(tmp.path(), mode, cut);
            assert_eq!(
                output.status.code(),
                Some(86),
                "{mode} cut {cut}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            let parent = parent(tmp.path());
            let mut registry = reopen(&parent);
            let old = if matches!(mode, "remove" | "self") {
                3
            } else {
                2
            };
            let committed = cut == 18;
            assert_eq!(registry.revision().unwrap(), old + u64::from(committed));
            let bytes = registry
                .owners(&admin_bearer(), "answer", &At(112))
                .unwrap();
            let list: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let expected = match (mode, committed) {
                ("add", true) | ("remove" | "self", false) => vec![PRINCIPAL, B],
                ("self", true) => vec![B],
                _ => vec![PRINCIPAL],
            };
            assert_eq!(list["owners"], serde_json::json!(expected));
            let retained = registry.archive(archive("0.1.0").sha256()).unwrap();
            assert_eq!(retained.as_bytes(), archive("0.1.0").as_bytes());
            super::yank::execute(&retained);
            if mode == "revoke" {
                assert_eq!(
                    registry.owners(&bearer(), "answer", &At(112)).is_err(),
                    committed
                );
                drop(registry);
                let mut admin =
                    Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
                assert_eq!(
                    admin
                        .revoke(&process_revocation(), &At(112))
                        .unwrap()
                        .state_revision(),
                    old + 1
                );
                assert_eq!(admin.revision().unwrap(), old + 1);
            } else {
                let retry = registry.change_owner(&bearer(), &process_action(mode), &At(112));
                if mode == "self" && committed {
                    assert_eq!(retry.unwrap_err().kind, ErrorKind::Authorization);
                } else {
                    assert_eq!(retry.unwrap().state_revision(), old + 1);
                }
                assert_eq!(registry.revision().unwrap(), old + 1);
            }
            assert_eq!(
                std::fs::read_dir(tmp.path().join("registry/staging"))
                    .unwrap()
                    .count(),
                0
            );
        }
    }
    println!("OWNER_REVOCATION_PROCESS_CUTS 72; old 68; new 4");
}

#[test]
fn recomputed_owner_and_revocation_histories_refuse_hidden_changes_and_noops() {
    for mode in ["owner", "revoke"] {
        for case in 0..9 {
            let tmp = Temp::new();
            prepared(tmp.path(), "add");
            let parent = parent(tmp.path());
            let mut registry = reopen(&parent);
            if mode == "owner" {
                registry
                    .change_owner(
                        &bearer(),
                        &action(OwnerActionKind::AddOwner, B, 'a', 1),
                        &At(110),
                    )
                    .unwrap();
            } else {
                drop(registry);
                let mut admin =
                    Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
                admin
                    .revoke(
                        &Revocation::new(
                            &"02".repeat(16),
                            &"a".repeat(32),
                            2,
                            "credential rotation",
                        )
                        .unwrap(),
                        &At(110),
                    )
                    .unwrap();
                drop(admin);
                registry = reopen(&parent);
            }
            let view = registry.recover().unwrap();
            let mut state = view.state;
            let mut audit: records::Audit = records::decode(
                &registry
                    .storage
                    .immutable(storage::Space::Audit, &state.last_audit_sha256)
                    .unwrap(),
                records::MAX_AUDIT,
            )
            .unwrap();
            match case {
                0 => state.packages[0].releases[0].yanked = true,
                1 => state.authority.tokens[0].grants.last_mut().unwrap().name = "other".into(),
                2 => state.authority.packages[0].owners = vec![ADMIN.into()],
                3 => audit.reason = "altered reason".into(),
                4 => audit.request_sha256 = Some("ab".repeat(32)),
                5 => audit.principal = INACTIVE.into(),
                6 => {
                    if mode == "owner" {
                        // Self-consistent add of an already-present A, with
                        // result and request digest recomputed to match it.
                        audit.operation = records::AuditOperation::AddOwner {
                            name: "answer".into(),
                            principal: PRINCIPAL.into(),
                        };
                        let records::AuditResult::Owner(r) = &mut audit.result else {
                            panic!("owner result")
                        };
                        r.principal = PRINCIPAL.into();
                        state.authority.packages[0].owners = vec![PRINCIPAL.into()];
                        audit.request_sha256 = Some(
                            action(OwnerActionKind::AddOwner, PRINCIPAL, 'a', 1)
                                .request_digest()
                                .unwrap(),
                        );
                    } else {
                        // A fully linked successor repeats an already-revoked
                        // credential with a new ID and exact next-state result.
                        audit.previous_audit_sha256 = Some(state.last_audit_sha256.clone());
                        audit.prior_state_sha256 = Some(view.head);
                        audit.sequence += 1;
                        state.revision += 1;
                        audit.request_id = Some("b".repeat(32));
                        audit.request_sha256 = Some(
                            Revocation::new(
                                &"02".repeat(16),
                                &"b".repeat(32),
                                3,
                                "credential rotation",
                            )
                            .unwrap()
                            .request_digest()
                            .unwrap(),
                        );
                        let records::AuditResult::Revocation(r) = &mut audit.result else {
                            panic!("revocation result")
                        };
                        r.state_revision += 1;
                    }
                }
                7 => {
                    audit.credential_id = if mode == "owner" {
                        None
                    } else {
                        Some("01".repeat(16))
                    };
                }
                8 => match &mut audit.result {
                    records::AuditResult::Owner(r) => r.member = false,
                    records::AuditResult::Revocation(r) => r.credential_id = "01".repeat(16),
                    _ => unreachable!(),
                },
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
fn competing_control_child() {
    let Some(root) = std::env::var_os("LANG_CONTROL_WRITER_ROOT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let parent = parent(root);
    let mode = std::env::var("LANG_CONTROL_WRITER_MODE").unwrap();
    if mode == "busy" {
        assert!(matches!(
            Registry::open(&parent, "registry", &identity()),
            Err(Error {
                kind: ErrorKind::Busy,
                ..
            })
        ));
        assert!(matches!(
            Administrator::open(&parent, "registry", &identity(), PRINCIPAL),
            Err(Error {
                kind: ErrorKind::Busy,
                ..
            })
        ));
    } else if mode == "revoke" {
        let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
        std::fs::write(root.join("ready"), b"").unwrap();
        wait_for(&root.join("release"));
        admin
            .revoke(
                &Revocation::new(&"02".repeat(16), &"b".repeat(32), 3, "credential rotation")
                    .unwrap(),
                &At(111),
            )
            .unwrap();
    } else {
        assert_eq!(mode, "remove");
        let mut registry = reopen(&parent);
        std::fs::write(root.join("ready"), b"").unwrap();
        wait_for(&root.join("release"));
        registry
            .change_owner(
                &bearer(),
                &action(OwnerActionKind::RemoveOwner, B, 'b', 2),
                &At(111),
            )
            .unwrap();
    }
}
fn control_writer(root: &std::path::Path, mode: &str) -> Child {
    Child(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "registry::store::tests::owners::competing_control_child",
                "--nocapture",
            ])
            .env("LANG_CONTROL_WRITER_ROOT", root)
            .env("LANG_CONTROL_WRITER_MODE", mode)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    )
}
#[test]
fn competing_owner_and_local_administrator_serialize_both_orders() {
    for mode in ["remove", "revoke"] {
        let tmp = Temp::new();
        prepared(tmp.path(), "remove");
        let mut first = control_writer(tmp.path(), mode);
        wait_for(&tmp.path().join("ready"));
        let mut busy = control_writer(tmp.path(), "busy");
        assert!(busy.0.wait().unwrap().success());
        std::fs::write(tmp.path().join("release"), b"").unwrap();
        assert!(first.0.wait().unwrap().success());
        let parent = parent(tmp.path());
        if mode == "remove" {
            let mut admin =
                Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
            let before = snapshot(tmp.path());
            let stale =
                Revocation::new(&"02".repeat(16), &"c".repeat(32), 3, "credential rotation")
                    .unwrap();
            assert_eq!(
                admin.revoke(&stale, &At(112)).unwrap_err().kind,
                ErrorKind::Conflict
            );
            assert_eq!(snapshot(tmp.path()), before);
            admin
                .revoke(
                    &Revocation::new(&"02".repeat(16), &"c".repeat(32), 4, "credential rotation")
                        .unwrap(),
                    &At(112),
                )
                .unwrap();
        } else {
            let mut registry = reopen(&parent);
            assert_eq!(
                registry
                    .owners(&second_bearer(), "answer", &At(112))
                    .unwrap_err()
                    .kind,
                ErrorKind::Authentication
            );
            // Credential-only revocation advances state, not package revision.
            registry
                .change_owner(
                    &bearer(),
                    &action(OwnerActionKind::RemoveOwner, B, 'c', 2),
                    &At(112),
                )
                .unwrap();
        }
        let registry = reopen(&parent);
        assert_eq!(registry.revision().unwrap(), 5);
        let list: serde_json::Value = serde_json::from_slice(
            &registry
                .owners(&admin_bearer(), "answer", &At(112))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(list["owners"], serde_json::json!([PRINCIPAL]));
        assert_eq!(list["revision"], 3);
        assert_eq!(
            registry
                .owners(&second_bearer(), "answer", &At(112))
                .unwrap_err()
                .kind,
            ErrorKind::Authentication
        );
        super::yank::execute(&registry.archive(archive("0.1.0").sha256()).unwrap());
    }
}

struct Times {
    values: Vec<u64>,
    index: Cell<usize>,
}
impl Clock for Times {
    fn now(&self) -> Result<u64, Error> {
        let i = self.index.get();
        self.index.set(i + 1);
        Ok(self.values[i])
    }
}
fn mutate(parent: &Dir, local: bool, clock: &impl Clock) -> Result<Receipt, Error> {
    if local {
        Administrator::open(parent, "registry", &identity(), PRINCIPAL)?
            .revoke(&process_revocation(), clock)
    } else {
        reopen(parent).change_owner(&bearer(), &process_action("add"), clock)
    }
}
#[test]
fn final_control_time_and_unknown_outcome_never_restore_old_authority() {
    for local in [false, true] {
        for times in [
            [110, 150, 149],
            [110, 150, if local { authority::MAX_TIME + 1 } else { 200 }],
        ] {
            let tmp = Temp::new();
            prepared(tmp.path(), "add");
            let parent = parent(tmp.path());
            let head = std::fs::read(tmp.path().join("registry/HEAD")).unwrap();
            let failure = mutate(
                &parent,
                local,
                &Times {
                    values: times.to_vec(),
                    index: Cell::new(0),
                },
            )
            .unwrap_err();
            assert_eq!(
                failure.kind,
                if !local && times[2] == 200 {
                    ErrorKind::Authentication
                } else {
                    ErrorKind::State
                }
            );
            assert_eq!(failure.outcome, PublicationOutcome::NotPublished);
            assert_eq!(
                std::fs::read(tmp.path().join("registry/HEAD")).unwrap(),
                head
            );
            assert_eq!(reopen(&parent).revision().unwrap(), 2);
            assert_eq!(
                mutate(&parent, local, &At(151)).unwrap().state_revision(),
                3
            );
        }
        for fault in [Fault::Before, Fault::After, Fault::Unreadable] {
            let tmp = Temp::new();
            prepared(tmp.path(), "add");
            let parent = parent(tmp.path());
            FAULT.with(|f| f.set(Some(fault)));
            let result = mutate(&parent, local, &At(110));
            match fault {
                Fault::Before => {
                    assert_eq!(
                        result.unwrap_err().outcome,
                        PublicationOutcome::NotPublished
                    );
                    assert_eq!(reopen(&parent).revision().unwrap(), 2);
                }
                Fault::After => assert_eq!(result.unwrap().state_revision(), 3),
                Fault::Unreadable => {
                    assert_eq!(result.unwrap_err().outcome, PublicationOutcome::Unknown);
                    assert!(Registry::open(&parent, "registry", &identity()).is_err());
                    // Restore only the exact bytes retained by this fault;
                    // production recovery never guesses a HEAD candidate.
                    let head = FAULT_HEAD.with(|h| h.borrow_mut().take().unwrap());
                    std::fs::write(tmp.path().join("registry/HEAD"), head).unwrap();
                }
            }
            assert_eq!(
                mutate(&parent, local, &At(111)).unwrap().state_revision(),
                3
            );
            assert_eq!(reopen(&parent).revision().unwrap(), 3);
        }
    }
    let tmp = Temp::new();
    prepared(tmp.path(), "add");
    let registry = reopen(&parent(tmp.path()));
    for final_time in [100, 200] {
        let before = snapshot(tmp.path());
        assert_eq!(
            registry
                .owners(
                    &bearer(),
                    "answer",
                    &Times {
                        values: vec![110, final_time],
                        index: Cell::new(0)
                    }
                )
                .unwrap_err()
                .kind,
            if final_time == 100 {
                ErrorKind::State
            } else {
                ErrorKind::Authentication
            }
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
}

#[test]
fn local_revocation_targets_expired_and_inactive_credentials_without_enrollment() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut authority = owner_authority();
    authority.tokens[1].principal = INACTIVE.into();
    let bytes = records::encode(&authority, authority::MAX_BYTES).unwrap();
    let mut registry = Registry::initialize(
        &parent,
        "registry",
        &identity(),
        &bytes,
        PRINCIPAL,
        &At(100),
    )
    .unwrap();
    registry
        .publish(
            &bearer(),
            &archive("0.1.0"),
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    drop(registry);
    let wrong_identity = ServiceIdentity::new("another", "https://registry.example").unwrap();
    assert!(Administrator::open(&parent, "registry", &wrong_identity, PRINCIPAL).is_err());
    assert!(matches!(
        Administrator::open(&parent, "registry", &identity(), &"f".repeat(32)),
        Err(Error {
            kind: ErrorKind::Authorization,
            ..
        })
    ));
    let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
    let before = snapshot(tmp.path());
    let missing =
        Revocation::new(&"f".repeat(32), &"a".repeat(32), 2, "credential rotation").unwrap();
    assert_eq!(
        admin.revoke(&missing, &At(201)).unwrap_err().kind,
        ErrorKind::NotFound
    );
    assert_eq!(snapshot(tmp.path()), before);
    let target =
        Revocation::new(&"02".repeat(16), &"b".repeat(32), 2, "credential rotation").unwrap();
    admin.revoke(&target, &At(201)).unwrap();
    let changed =
        Revocation::new(&"02".repeat(16), &"b".repeat(32), 2, "different reason").unwrap();
    let before = snapshot(tmp.path());
    assert_eq!(
        admin.revoke(&changed, &At(201)).unwrap_err().kind,
        ErrorKind::Conflict
    );
    assert_eq!(snapshot(tmp.path()), before);
    admin
        .revoke(
            &Revocation::new(&"01".repeat(16), &"c".repeat(32), 3, "credential rotation").unwrap(),
            &At(201),
        )
        .unwrap();
    drop(admin);
    let registry = reopen(&parent);
    let state = registry.recover().unwrap().state;
    assert!(state.authority.principals == authority.principals);
    assert_eq!(state.packages[0].revision, 1);
    assert!(state.authority.tokens[0].revoked && state.authority.tokens[1].revoked);
    assert_eq!(
        registry
            .owners(&bearer(), "answer", &At(201))
            .unwrap_err()
            .kind,
        ErrorKind::Authentication
    );
}

#[test]
fn local_revocation_preserves_package_and_refuses_old_bearer_even_for_retry() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut registry = initialize_owners(&parent);
    let archive = archive("0.1.0");
    let a = bearer();
    let b = second_bearer();
    registry
        .publish(&a, &archive, REQUEST, ExpectedRevision::Absent, &At(101))
        .unwrap();
    let add = action(OwnerActionKind::AddOwner, B, 'a', 1);
    registry.change_owner(&a, &add, &At(110)).unwrap();
    let index = registry.index("answer").unwrap();
    let owner_bytes = registry.owners(&b, "answer", &At(110)).unwrap();
    assert!(matches!(
        Administrator::open(&parent, "registry", &identity(), PRINCIPAL),
        Err(Error {
            kind: ErrorKind::Busy,
            ..
        })
    ));
    drop(registry);
    assert!(matches!(
        Administrator::open(&parent, "registry", &identity(), INACTIVE),
        Err(Error {
            kind: ErrorKind::Authorization,
            ..
        })
    ));
    assert!(Administrator::open(&parent, "registry", &identity(), "bad-operator").is_err());
    let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
    assert_eq!(admin.revision().unwrap(), 3);
    // The same actor/request namespace cannot reinterpret an ordinary success.
    let collision =
        Revocation::new(&"01".repeat(16), &"a".repeat(32), 3, "credential rotation").unwrap();
    assert_eq!(
        admin.revoke(&collision, &At(110)).unwrap_err().kind,
        ErrorKind::Conflict
    );
    let revoke =
        Revocation::new(&"01".repeat(16), &"b".repeat(32), 3, "credential rotation").unwrap();
    let receipt = admin.revoke(&revoke, &At(110)).unwrap();
    assert_eq!(receipt.state_revision(), 4);
    let before = snapshot(tmp.path());
    // Revoking the operator's bearer does not revoke its independent Dir capability.
    assert_eq!(admin.revoke(&revoke, &At(111)).unwrap(), receipt);
    assert_eq!(admin.revision().unwrap(), 4);
    assert_eq!(snapshot(tmp.path()), before);
    drop(admin);
    let mut registry = reopen(&parent);
    assert_eq!(registry.index("answer").unwrap(), index);
    assert_eq!(
        registry.owners(&b, "answer", &At(111)).unwrap(),
        owner_bytes
    );
    assert_eq!(
        registry.change_owner(&a, &add, &At(111)).unwrap_err().kind,
        ErrorKind::Authentication
    );
    assert_eq!(
        registry
            .publish(&a, &archive, REQUEST, ExpectedRevision::Absent, &At(111))
            .unwrap_err()
            .kind,
        ErrorKind::Authentication
    );
    assert_eq!(
        registry.owners(&a, "answer", &At(111)).unwrap_err().kind,
        ErrorKind::Authentication
    );
    let retained = registry.archive(archive.sha256()).unwrap();
    assert_eq!(retained.as_bytes(), archive.as_bytes());
    super::yank::execute(&retained);
    drop(registry);
    let mut admin = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
    assert_eq!(admin.revoke(&revoke, &At(111)).unwrap(), receipt);
    let stale =
        Revocation::new(&"02".repeat(16), &"c".repeat(32), 3, "credential rotation").unwrap();
    assert_eq!(
        admin.revoke(&stale, &At(111)).unwrap_err().kind,
        ErrorKind::Conflict
    );
    let repeated =
        Revocation::new(&"01".repeat(16), &"d".repeat(32), 4, "credential rotation").unwrap();
    assert_eq!(
        admin.revoke(&repeated, &At(111)).unwrap_err().kind,
        ErrorKind::Conflict
    );
    let last =
        Revocation::new(&"02".repeat(16), &"e".repeat(32), 4, "credential rotation").unwrap();
    admin.revoke(&last, &At(111)).unwrap();
    drop(admin);
    let registry = reopen(&parent);
    assert_eq!(
        registry.owners(&b, "answer", &At(111)).unwrap_err().kind,
        ErrorKind::Authentication
    );
    assert_eq!(registry.index("answer").unwrap(), index);
    let current: serde_json::Value = serde_json::from_slice(
        &registry
            .owners(&admin_bearer(), "answer", &At(111))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(current["owners"], serde_json::json!([PRINCIPAL, B]));
}

#[test]
fn actual_transfer_denies_removed_owner_retry_and_preserves_vm_native_source() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut registry = initialize_owners(&parent);
    let a = bearer();
    let b = second_bearer();
    let archive = archive("0.1.0");
    registry
        .publish(&a, &archive, REQUEST, ExpectedRevision::Absent, &At(101))
        .unwrap();
    assert_eq!(
        members(&registry, &a)["owners"],
        serde_json::json!([PRINCIPAL])
    );
    assert_eq!(
        registry.owners(&b, "answer", &At(110)).unwrap_err().kind,
        ErrorKind::Authorization
    );
    let add = action(OwnerActionKind::AddOwner, B, 'a', 1);
    let added = registry.change_owner(&a, &add, &At(110)).unwrap();
    assert_eq!(added.state_revision(), 3);
    assert_eq!(
        members(&registry, &b)["owners"],
        serde_json::json!([PRINCIPAL, B])
    );
    assert_eq!(registry.change_owner(&a, &add, &At(110)).unwrap(), added);
    let remove = action(OwnerActionKind::RemoveOwner, PRINCIPAL, 'b', 2);
    let before = snapshot(tmp.path());
    for collision in [
        OwnerAction::new(
            "answer",
            B,
            REQUEST,
            1,
            OwnerActionKind::AddOwner,
            "ownership handoff",
        )
        .unwrap(),
        OwnerAction::new(
            "answer",
            B,
            &"a".repeat(32),
            1,
            OwnerActionKind::AddOwner,
            "altered reason",
        )
        .unwrap(),
    ] {
        assert_eq!(
            registry
                .change_owner(&a, &collision, &At(110))
                .unwrap_err()
                .kind,
            ErrorKind::Conflict
        );
        assert_eq!(snapshot(tmp.path()), before);
    }
    let removed = registry.change_owner(&a, &remove, &At(110)).unwrap();
    assert_eq!(removed.state_revision(), 4);
    for old in [&add, &remove] {
        assert_eq!(
            registry.change_owner(&a, old, &At(110)).unwrap_err().kind,
            ErrorKind::Authorization
        );
    }
    assert_eq!(
        registry
            .publish(&a, &archive, REQUEST, ExpectedRevision::Absent, &At(110))
            .unwrap_err()
            .kind,
        ErrorKind::Authorization
    );
    assert_eq!(
        registry.owners(&a, "answer", &At(110)).unwrap_err().kind,
        ErrorKind::Authorization
    );
    assert_eq!(members(&registry, &b)["owners"], serde_json::json!([B]));
    assert_eq!(
        members(&registry, &admin_bearer())["owners"],
        serde_json::json!([B])
    );
    let last = action(OwnerActionKind::RemoveOwner, B, 'c', 3);
    let before = snapshot(tmp.path());
    assert_eq!(
        registry.change_owner(&b, &last, &At(110)).unwrap_err().kind,
        ErrorKind::Conflict
    );
    assert_eq!(
        registry
            .change_owner(&admin_bearer(), &last, &At(110))
            .unwrap_err()
            .kind,
        ErrorKind::Authorization
    );
    assert_eq!(snapshot(tmp.path()), before);
    drop(registry);
    let mut registry = reopen(&parent);
    let next = super::archive("0.2.0");
    registry
        .publish(
            &b,
            &next,
            &"d".repeat(32),
            ExpectedRevision::Revision(3),
            &At(110),
        )
        .unwrap();
    assert_eq!(registry.revision().unwrap(), 5);
    assert_eq!(members(&registry, &b)["revision"], 4);
    for original in [&archive, &next] {
        let retained = registry.archive(original.sha256()).unwrap();
        assert_eq!(retained.as_bytes(), original.as_bytes());
        super::yank::execute(&retained);
    }
    assert!(
        !String::from_utf8(registry.index("answer").unwrap())
            .unwrap()
            .contains("owners")
    );
    let add_back = action(OwnerActionKind::AddOwner, PRINCIPAL, 'e', 4);
    let old = registry.change_owner(&b, &add_back, &At(110)).unwrap();
    registry
        .change_owner(
            &b,
            &action(OwnerActionKind::RemoveOwner, PRINCIPAL, 'f', 5),
            &At(110),
        )
        .unwrap();
    let before = snapshot(tmp.path());
    assert_eq!(registry.change_owner(&b, &add_back, &At(110)).unwrap(), old);
    assert_eq!(members(&registry, &b)["owners"], serde_json::json!([B]));
    assert_eq!(snapshot(tmp.path()), before);
}
