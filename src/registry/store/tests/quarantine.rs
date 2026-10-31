use super::*;

const ADMIN: &str = "00000000000000000000000000000002";

pub(super) fn admin() -> Bearer {
    Bearer::from_input("22".repeat(32).as_bytes()).unwrap()
}
pub(super) fn initialize(parent: &Dir) -> Registry {
    initialize_with(parent, |_| {})
}
fn initialize_with(
    parent: &Dir,
    configure: impl FnOnce(&mut authority::codec::Records),
) -> Registry {
    let mut r: authority::codec::Records =
        records::decode(&authority_bytes(), authority::MAX_BYTES).unwrap();
    r.tokens[0].grants.push(authority::codec::Grant {
        action: authority::codec::Action::Yank,
        name: "answer".into(),
    });
    r.principals.push(authority::codec::Principal {
        id: ADMIN.into(),
        active: true,
    });
    let mut token = r.tokens[0].clone();
    token.id = "02".repeat(16);
    token.principal = ADMIN.into();
    token.authenticator_sha256 = admin().authenticator_sha256();
    token.grants = vec![authority::codec::Grant {
        action: authority::codec::Action::Admin,
        name: "answer".into(),
    }];
    r.tokens.push(token);
    configure(&mut r);
    Registry::initialize(
        parent,
        "registry",
        &identity(),
        &records::encode(&r, authority::MAX_BYTES).unwrap(),
        PRINCIPAL,
        &At(100),
    )
    .unwrap()
}
fn action(kind: QuarantineActionKind, id: char, revision: u64) -> QuarantineAction {
    QuarantineAction::new(
        "answer",
        "0.1.0",
        &id.to_string().repeat(32),
        revision,
        kind,
        "security review",
    )
    .unwrap()
}

#[test]
fn quarantine_reopen_restoration_preserve_independent_yank_and_exact_vm_native_source() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut registry = initialize(&parent);
    let archive = archive("0.1.0");
    let owner = bearer();
    let administrator = admin();
    let published = registry
        .publish(
            &owner,
            &archive,
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    yank::execute(&registry.archive(archive.sha256()).unwrap());
    let quarantine = action(QuarantineActionKind::Quarantine, 'a', 1);
    assert_eq!(
        registry
            .quarantine(&owner, &quarantine, &At(102))
            .unwrap_err()
            .kind,
        ErrorKind::Authorization
    );
    let receipt = registry
        .quarantine(&administrator, &quarantine, &At(102))
        .unwrap();
    assert_eq!(receipt.state_revision(), 3);
    assert_eq!(receipt.package_result(), Some((2, "quarantined")));
    let index = Index::admit(&registry.index("answer").unwrap(), &identity(), "answer").unwrap();
    assert_eq!(index.revision(), 2);
    assert!(index.release("0.1.0").unwrap().quarantined());
    assert!(!index.release("0.1.0").unwrap().yanked());
    assert_eq!(
        registry.archive(archive.sha256()).err().unwrap().kind,
        ErrorKind::NotFound
    );
    let yank = YankAction::new(
        "answer",
        "0.1.0",
        &"b".repeat(32),
        2,
        YankActionKind::Yank,
        "owner selection",
    )
    .unwrap();
    assert_eq!(
        registry
            .yank(&administrator, &yank, &At(103))
            .unwrap_err()
            .kind,
        ErrorKind::Authorization
    );
    registry.yank(&owner, &yank, &At(103)).unwrap();
    assert_eq!(
        registry.archive(archive.sha256()).err().unwrap().kind,
        ErrorKind::NotFound
    );
    drop(registry);
    let mut registry = Registry::open(&parent, "registry", &identity()).unwrap();
    let restore = action(QuarantineActionKind::Unquarantine, 'c', 3);
    let restored = registry
        .quarantine(&administrator, &restore, &At(104))
        .unwrap();
    assert_eq!(restored.state_revision(), 5);
    assert_eq!(restored.package_result(), Some((4, "unquarantined")));
    let index = Index::admit(&registry.index("answer").unwrap(), &identity(), "answer").unwrap();
    assert_eq!(index.revision(), 4);
    assert!(!index.release("0.1.0").unwrap().quarantined());
    assert!(index.release("0.1.0").unwrap().yanked());
    let exact = registry.archive(archive.sha256()).unwrap();
    assert_eq!(exact.as_bytes(), archive.as_bytes());
    yank::execute(&exact);
    assert_eq!(
        registry
            .quarantine(&administrator, &quarantine, &At(105))
            .unwrap(),
        receipt
    );
    assert_eq!(
        registry
            .publish(
                &owner,
                &archive,
                REQUEST,
                ExpectedRevision::Absent,
                &At(105)
            )
            .unwrap(),
        published
    );
    let index = Index::admit(&registry.index("answer").unwrap(), &identity(), "answer").unwrap();
    assert_eq!(index.revision(), 4);
    assert!(!index.release("0.1.0").unwrap().quarantined());
    assert_eq!(
        registry
            .quarantine(
                &administrator,
                &action(QuarantineActionKind::Unquarantine, 'd', 4),
                &At(105)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    assert_eq!(
        registry
            .quarantine(
                &administrator,
                &action(QuarantineActionKind::Quarantine, 'e', 3),
                &At(105)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    assert_eq!(
        registry
            .quarantine(&administrator, &quarantine, &At(200))
            .unwrap_err()
            .kind,
        ErrorKind::Authentication
    );
    drop(registry);
    let registry = Registry::open(&parent, "registry", &identity()).unwrap();
    yank::execute(&registry.archive(archive.sha256()).unwrap());
}

#[test]
fn canonical_quarantine_matches_independent_encoding_and_rejects_inapplicable_inputs() {
    let action = QuarantineAction::new(
        "math",
        "1.0.0",
        &"4".repeat(32),
        1,
        QuarantineActionKind::Quarantine,
        "security review",
    )
    .unwrap();
    assert_eq!(action.as_bytes().len(), 175);
    assert_eq!(
        digest(action.as_bytes()),
        "2a3279626278d75238c1bb21375ddde4bd4db70e26f3388c30a520575bc8af8e"
    );
    assert_eq!(
        action.request_digest().unwrap(),
        "4a33a22651dd4f718ce1f29b067fca6b97db01a7cfcec66101d6388dd36fcaf1"
    );
    assert_eq!(
        QuarantineAction::admit("math", action.as_bytes())
            .unwrap()
            .as_bytes(),
        action.as_bytes()
    );
    let text = std::str::from_utf8(action.as_bytes()).unwrap();
    for invalid in [
        format!("{text}\n"),
        text.replace("\"schema\": 1", "\"schema\": 2"),
        text.replace("\"schema\": 1,", "\"schema\": 1,\n  \"schema\": 1,"),
        text.replace(
            "\"schema\": 1,",
            "\"schema\": 1,\n  \"principal\": \"owner\",",
        ),
        text.replace("\"quarantine\"", "\"yank\""),
        text.replace("\"expected_revision\": 1", "\"expected_revision\": 0"),
        text.replace("security review", "line\\nbreak"),
        text.replace("security review", ""),
    ] {
        assert_eq!(
            QuarantineAction::admit("math", invalid.as_bytes())
                .unwrap_err()
                .kind,
            ErrorKind::Malformed
        );
    }
    assert_eq!(
        QuarantineAction::admit("math", &vec![b' '; 2049])
            .unwrap_err()
            .kind,
        ErrorKind::Limit
    );
    for reason in ["x".repeat(256), "é".repeat(128)] {
        assert!(
            QuarantineAction::new(
                "math",
                "1.0.0",
                &"4".repeat(32),
                1,
                QuarantineActionKind::Quarantine,
                &reason
            )
            .is_ok()
        );
    }
    for reason in ["x".repeat(257), "é".repeat(129), "control\u{85}".into()] {
        assert_eq!(
            QuarantineAction::new(
                "math",
                "1.0.0",
                &"4".repeat(32),
                1,
                QuarantineActionKind::Quarantine,
                &reason
            )
            .unwrap_err()
            .kind,
            ErrorKind::Malformed
        );
    }
    for seed in 0..512 {
        let mut hostile = action.as_bytes().to_vec();
        let index = seed % hostile.len();
        hostile[index] ^= 1 << (seed % 8);
        match QuarantineAction::admit("math", &hostile) {
            Ok(admitted) => assert_eq!(admitted.as_bytes(), hostile),
            Err(error) => assert!(error.to_string().len() < 128),
        }
    }
}

#[test]
fn current_admin_scope_and_local_revocation_precede_every_quarantine_retry() {
    for (case, expected) in [
        (0, ErrorKind::Authorization),
        (1, ErrorKind::Authentication),
        (2, ErrorKind::Authentication),
        (3, ErrorKind::Authentication),
        (4, ErrorKind::Authentication),
        (5, ErrorKind::Authorization),
    ] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut registry = initialize_with(&parent, |r| match case {
            0 => r.tokens[1].grants[0].name = "other".into(),
            1 => r.tokens[1].revoked = true,
            2 => r.principals[1].active = false,
            3 => r.tokens[1].expires = 101,
            4 => r.tokens[1].issued = 103,
            5 => r.tokens[1].grants[0].action = authority::codec::Action::Yank,
            _ => unreachable!(),
        });
        registry
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(101),
            )
            .unwrap();
        let before = snapshot(tmp.path());
        let failure = registry
            .quarantine(
                &admin(),
                &action(QuarantineActionKind::Quarantine, 'a', 1),
                &At(102),
            )
            .unwrap_err();
        assert_eq!(failure.kind, expected, "case {case}");
        assert_eq!(failure.outcome, PublicationOutcome::NotPublished);
        assert_eq!(snapshot(tmp.path()), before);
    }
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut registry = initialize(&parent);
    registry
        .publish(
            &bearer(),
            &archive("0.1.0"),
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    let request = action(QuarantineActionKind::Quarantine, 'a', 1);
    registry.quarantine(&admin(), &request, &At(102)).unwrap();
    let changed = QuarantineAction::new(
        "answer",
        "0.1.0",
        &"a".repeat(32),
        1,
        QuarantineActionKind::Quarantine,
        "changed reason",
    )
    .unwrap();
    assert_eq!(
        registry
            .quarantine(&admin(), &changed, &At(103))
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    drop(registry);
    let mut operator = Administrator::open(&parent, "registry", &identity(), PRINCIPAL).unwrap();
    operator
        .revoke(
            &Revocation::new(
                &"02".repeat(16),
                &"e".repeat(32),
                3,
                "revoke administrative bearer",
            )
            .unwrap(),
            &At(104),
        )
        .unwrap();
    drop(operator);
    let mut registry = reopen(&parent);
    let before = snapshot(tmp.path());
    for attempt in [request, action(QuarantineActionKind::Unquarantine, 'b', 2)] {
        assert_eq!(
            registry
                .quarantine(&admin(), &attempt, &At(105))
                .unwrap_err()
                .kind,
            ErrorKind::Authentication
        );
    }
    assert_eq!(snapshot(tmp.path()), before);
    assert_eq!(registry.revision().unwrap(), 4);
    assert_eq!(
        registry
            .archive(archive("0.1.0").sha256())
            .err()
            .unwrap()
            .kind,
        ErrorKind::NotFound
    );
}

#[test]
fn final_quarantine_guard_and_unknown_result_preserve_exact_reconciliation() {
    struct Times {
        values: [u64; 3],
        index: Cell<usize>,
    }
    impl Clock for Times {
        fn now(&self) -> Result<u64, Error> {
            let i = self.index.get();
            self.index.set(i + 1);
            Ok(self.values[i])
        }
    }
    for times in [[110, 150, 200], [110, 150, 149]] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut registry = initialize(&parent);
        registry
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(101),
            )
            .unwrap();
        let old = registry.storage.head().unwrap();
        let failure = registry
            .quarantine(
                &admin(),
                &action(QuarantineActionKind::Quarantine, 'a', 1),
                &Times {
                    values: times,
                    index: Cell::new(0),
                },
            )
            .unwrap_err();
        assert_eq!(
            failure.kind,
            if times[2] == 200 {
                ErrorKind::Authentication
            } else {
                ErrorKind::State
            }
        );
        assert_eq!(failure.outcome, PublicationOutcome::NotPublished);
        assert_eq!(registry.storage.head().unwrap(), old);
        drop(registry);
        let mut registry = reopen(&parent);
        assert_eq!(registry.revision().unwrap(), 2);
        assert_eq!(
            registry
                .quarantine(
                    &admin(),
                    &action(QuarantineActionKind::Quarantine, 'a', 1),
                    &At(151)
                )
                .unwrap()
                .state_revision(),
            3
        );
    }
    for fault in [Fault::Before, Fault::After, Fault::Unreadable] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut registry = initialize(&parent);
        registry
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(101),
            )
            .unwrap();
        let request = action(QuarantineActionKind::Quarantine, 'a', 1);
        FAULT.with(|f| f.set(Some(fault)));
        let result = registry.quarantine(&admin(), &request, &At(110));
        match fault {
            Fault::Before => {
                assert_eq!(
                    result.unwrap_err().outcome,
                    PublicationOutcome::NotPublished
                );
                assert_eq!(registry.revision().unwrap(), 2);
            }
            Fault::After => {
                let receipt = result.unwrap();
                assert_eq!(receipt.state_revision(), 3);
                assert_eq!(
                    registry.quarantine(&admin(), &request, &At(111)).unwrap(),
                    receipt
                );
            }
            Fault::Unreadable => {
                assert_eq!(result.unwrap_err().outcome, PublicationOutcome::Unknown);
                assert!(registry.revision().is_err());
                // Explicit test-operator restoration from the exact HEAD bytes
                // retained before the fixture fault removed them. Never select
                // an immutable candidate by its revision or recreate HEAD in open.
                let bytes = FAULT_HEAD.with(|head| head.borrow_mut().take().unwrap());
                std::fs::write(tmp.path().join("registry/HEAD"), bytes).unwrap();
                assert_eq!(
                    registry
                        .quarantine(&admin(), &request, &At(111))
                        .unwrap()
                        .state_revision(),
                    3
                );
                assert_eq!(registry.revision().unwrap(), 3);
            }
        }
    }
}

fn prepared(root: &std::path::Path, kind: QuarantineActionKind) {
    let parent = parent(root);
    let mut registry = initialize(&parent);
    registry
        .publish(
            &bearer(),
            &archive("0.1.0"),
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    if kind == QuarantineActionKind::Unquarantine {
        registry
            .quarantine(
                &admin(),
                &action(QuarantineActionKind::Quarantine, 'a', 1),
                &At(102),
            )
            .unwrap();
    }
}
#[test]
fn action_child() {
    let Some(root) = std::env::var_os("LANG_QUARANTINE_TEST_ROOT") else {
        return;
    };
    let kind = if std::env::var("LANG_QUARANTINE_TEST_KIND").unwrap() == "quarantine" {
        QuarantineActionKind::Quarantine
    } else {
        QuarantineActionKind::Unquarantine
    };
    let mut registry = reopen(&parent(std::path::Path::new(&root)));
    let cut = std::env::var("LANG_QUARANTINE_TEST_CUT")
        .unwrap()
        .parse()
        .unwrap();
    let request = action(
        kind,
        'b',
        if kind == QuarantineActionKind::Quarantine {
            1
        } else {
            2
        },
    );
    CUT.with(|c| c.set(Some((cut, 0))));
    registry.quarantine(&admin(), &request, &At(110)).unwrap();
    println!(
        "QUARANTINE_CHECKPOINTS {}",
        CUT.with(|c| c.get().unwrap().1)
    );
}
fn interrupted(
    root: &std::path::Path,
    kind: QuarantineActionKind,
    cut: usize,
) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "registry::store::tests::quarantine::action_child",
            "--nocapture",
        ])
        .env("LANG_QUARANTINE_TEST_ROOT", root)
        .env(
            "LANG_QUARANTINE_TEST_KIND",
            if kind == QuarantineActionKind::Quarantine {
                "quarantine"
            } else {
                "unquarantine"
            },
        )
        .env("LANG_QUARANTINE_TEST_CUT", cut.to_string())
        .output()
        .unwrap()
}
#[test]
fn every_quarantine_and_unquarantine_process_cut_reopens_complete_history_and_exact_archive() {
    for kind in [
        QuarantineActionKind::Quarantine,
        QuarantineActionKind::Unquarantine,
    ] {
        let tmp = Temp::new();
        prepared(tmp.path(), kind);
        let complete = interrupted(tmp.path(), kind, usize::MAX);
        assert!(
            complete.status.success(),
            "{}",
            String::from_utf8_lossy(&complete.stdout)
        );
        assert!(String::from_utf8_lossy(&complete.stdout).contains("QUARANTINE_CHECKPOINTS 18"));
        for cut in 1..=18 {
            let tmp = Temp::new();
            prepared(tmp.path(), kind);
            let output = interrupted(tmp.path(), kind, cut);
            assert_eq!(
                output.status.code(),
                Some(86),
                "{kind:?} cut {cut}: {}",
                String::from_utf8_lossy(&output.stdout)
            );
            let parent = parent(tmp.path());
            let mut registry = reopen(&parent);
            let old = if kind == QuarantineActionKind::Quarantine {
                2
            } else {
                3
            };
            assert_eq!(registry.revision().unwrap(), old + u64::from(cut == 18));
            let index =
                Index::admit(&registry.index("answer").unwrap(), &identity(), "answer").unwrap();
            let target = kind == QuarantineActionKind::Quarantine;
            assert_eq!(
                index.release("0.1.0").unwrap().quarantined(),
                if cut == 18 { target } else { !target }
            );
            let original = archive("0.1.0");
            let bytes = registry
                .storage
                .immutable(storage::Space::Object, original.sha256())
                .unwrap();
            assert_eq!(bytes, original.as_bytes());
            let archived = AdmittedArchive::admit(&bytes).unwrap();
            yank::execute(&archived);
            if index.release("0.1.0").unwrap().quarantined() {
                assert_eq!(
                    registry.archive(original.sha256()).err().unwrap().kind,
                    ErrorKind::NotFound
                );
            } else {
                assert_eq!(
                    registry.archive(original.sha256()).unwrap().as_bytes(),
                    original.as_bytes()
                );
            }
            let receipt = registry
                .quarantine(&admin(), &action(kind, 'b', old - 1), &At(111))
                .unwrap();
            assert_eq!(receipt.state_revision(), old + 1);
            assert_eq!(registry.revision().unwrap(), old + 1);
            assert_eq!(
                std::fs::read_dir(tmp.path().join("registry/staging"))
                    .unwrap()
                    .count(),
                0
            );
        }
    }
    println!("QUARANTINE_PROCESS_CUTS 36; old 34; new 2");
}

#[test]
fn recomputed_quarantine_hashes_cannot_hide_other_changes_or_forge_noop_history() {
    for case in 0..12 {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut registry = initialize(&parent);
        registry
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(101),
            )
            .unwrap();
        registry
            .publish(
                &bearer(),
                &archive("0.2.0"),
                &"2".repeat(32),
                ExpectedRevision::Revision(1),
                &At(102),
            )
            .unwrap();
        registry
            .quarantine(
                &admin(),
                &action(QuarantineActionKind::Quarantine, 'a', 2),
                &At(103),
            )
            .unwrap();
        let mut state = history::load(&registry.storage, &identity()).unwrap().state;
        let mut audit: records::Audit = records::decode(
            &registry
                .storage
                .immutable(storage::Space::Audit, &state.last_audit_sha256)
                .unwrap(),
            records::MAX_AUDIT,
        )
        .unwrap();
        match case {
            0 => state.packages[0].releases[0].quarantined = false,
            1 => state.packages[0].releases[1].quarantined = true,
            2 => state.authority.tokens[0].revoked = true,
            3 => audit.reason = "altered explanation".into(),
            4 => audit.request_sha256 = Some("ab".repeat(32)),
            5 => {
                let records::AuditResult::Quarantine(result) = &mut audit.result else {
                    panic!("quarantine result")
                };
                result.quarantined = false;
            }
            6 => {
                // All digests/result fields agree on an unquarantine from an already
                // unquarantined predecessor. Complete transition checking must refuse.
                let records::AuditResult::Quarantine(result) = &mut audit.result else {
                    panic!("quarantine result")
                };
                result.quarantined = false;
                audit.operation = records::AuditOperation::Unquarantine {
                    name: result.name.clone(),
                    version: result.version.clone(),
                    archive_sha256: result.archive_sha256.clone(),
                };
                state.packages[0].releases[0].quarantined = false;
                audit.request_sha256 = Some(
                    action(QuarantineActionKind::Unquarantine, 'a', 2)
                        .request_digest()
                        .unwrap(),
                );
            }
            7 => audit.principal = "f".repeat(32),
            8 => state.packages[0].releases[0].yanked = true,
            9 => {
                audit.principal = PRINCIPAL.into();
                audit.credential_id = Some("01".repeat(16));
            }
            10 => state.authority.tokens[1].grants[0].name = "other".into(),
            11 => state.packages[0].releases[0].descriptor_sha256 = "ff".repeat(32),
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

#[test]
fn competing_action_child() {
    let Some(root) = std::env::var_os("LANG_QUARANTINE_WRITER_ROOT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let parent = parent(root);
    let mode = std::env::var("LANG_QUARANTINE_WRITER_MODE").unwrap();
    if mode == "busy" {
        assert_eq!(
            Registry::open(&parent, "registry", &identity())
                .err()
                .unwrap()
                .kind,
            ErrorKind::Busy
        );
        return;
    }
    let mut registry = reopen(&parent);
    std::fs::write(root.join("ready"), b"").unwrap();
    wait_for(&root.join("release"));
    if mode == "publish" {
        registry
            .publish(
                &bearer(),
                &archive("0.2.0"),
                &"a".repeat(32),
                ExpectedRevision::Revision(1),
                &At(110),
            )
            .unwrap();
    } else {
        registry
            .quarantine(
                &admin(),
                &action(QuarantineActionKind::Quarantine, 'a', 1),
                &At(110),
            )
            .unwrap();
    }
}
fn action_writer(root: &std::path::Path, mode: &str) -> Child {
    Child(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "registry::store::tests::quarantine::competing_action_child",
                "--nocapture",
            ])
            .env("LANG_QUARANTINE_WRITER_ROOT", root)
            .env("LANG_QUARANTINE_WRITER_MODE", mode)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    )
}
#[test]
fn competing_publish_and_quarantine_serialize_both_orders_without_replacing_archives() {
    for mode in ["publish", "quarantine"] {
        let tmp = Temp::new();
        prepared(tmp.path(), QuarantineActionKind::Quarantine);
        let mut first = action_writer(tmp.path(), mode);
        wait_for(&tmp.path().join("ready"));
        let mut busy = action_writer(tmp.path(), "busy");
        assert!(busy.0.wait().unwrap().success());
        std::fs::write(tmp.path().join("release"), b"").unwrap();
        assert!(first.0.wait().unwrap().success());
        let parent = parent(tmp.path());
        let mut registry = reopen(&parent);
        assert_eq!(registry.revision().unwrap(), 3);
        let old = registry.storage.head().unwrap();
        if mode == "publish" {
            assert_eq!(
                registry
                    .quarantine(
                        &admin(),
                        &action(QuarantineActionKind::Quarantine, 'b', 1),
                        &At(111)
                    )
                    .unwrap_err()
                    .kind,
                ErrorKind::Conflict
            );
            assert_eq!(registry.storage.head().unwrap(), old);
            registry
                .quarantine(
                    &admin(),
                    &action(QuarantineActionKind::Quarantine, 'b', 2),
                    &At(111),
                )
                .unwrap();
        } else {
            let next = archive("0.2.0");
            assert_eq!(
                registry
                    .publish(
                        &bearer(),
                        &next,
                        &"b".repeat(32),
                        ExpectedRevision::Revision(1),
                        &At(111)
                    )
                    .unwrap_err()
                    .kind,
                ErrorKind::Conflict
            );
            assert_eq!(registry.storage.head().unwrap(), old);
            registry
                .publish(
                    &bearer(),
                    &next,
                    &"b".repeat(32),
                    ExpectedRevision::Revision(2),
                    &At(111),
                )
                .unwrap();
        }
        let index =
            Index::admit(&registry.index("answer").unwrap(), &identity(), "answer").unwrap();
        assert_eq!(index.revision(), 3);
        assert!(index.release("0.1.0").unwrap().quarantined());
        assert!(!index.release("0.2.0").unwrap().quarantined());
        assert_eq!(registry.revision().unwrap(), 4);
        let original = archive("0.1.0");
        assert_eq!(
            registry.archive(original.sha256()).err().unwrap().kind,
            ErrorKind::NotFound
        );
        let bytes = registry
            .storage
            .immutable(storage::Space::Object, original.sha256())
            .unwrap();
        assert_eq!(bytes, original.as_bytes());
        let retained = AdmittedArchive::admit(&bytes).unwrap();
        yank::execute(&retained);
        yank::execute(&registry.archive(archive("0.2.0").sha256()).unwrap());
        registry
            .quarantine(
                &admin(),
                &action(QuarantineActionKind::Unquarantine, 'c', 3),
                &At(112),
            )
            .unwrap();
        yank::execute(&registry.archive(original.sha256()).unwrap());
    }
}
