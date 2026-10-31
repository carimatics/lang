use super::*;

fn yank_authority() -> authority::codec::Records {
    let mut authority: authority::codec::Records =
        records::decode(&authority_bytes(), authority::MAX_BYTES).unwrap();
    authority.tokens[0].grants.push(authority::codec::Grant {
        action: authority::codec::Action::Yank,
        name: "answer".into(),
    });
    authority
}
pub(super) fn initialize_yank(parent: &Dir) -> Registry {
    let bytes = records::encode(&yank_authority(), authority::MAX_BYTES).unwrap();
    Registry::initialize(parent, "registry", &identity(), &bytes, PRINCIPAL, &At(100)).unwrap()
}
fn action(kind: YankActionKind, id: char, revision: u64) -> YankAction {
    YankAction::new(
        "answer",
        "0.1.0",
        &id.to_string().repeat(32),
        revision,
        kind,
        "release maintenance",
    )
    .unwrap()
}
fn execute(archive: &AdmittedArchive) {
    assert_eq!(archive.source("src/main.lang"), Some(SOURCE));
    let analysis = crate::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        std::path::Path::new("src/main.lang"),
        archive.source("src/main.lang").unwrap(),
        None,
        Default::default(),
        None,
    );
    let lang_semantics::analysis::Program::Copy(typed) = analysis.result.unwrap() else {
        panic!("scalar fixture")
    };
    let program =
        crate::frontend::compile_script(&typed, &analysis.sources, Default::default()).unwrap();
    let hosts = lang_runtime::HostFunctionRegistry::new();
    let vm = lang_vm::unit::execute_entry(
        &program,
        &hosts,
        &lang_vm::unit::HostBindings::new(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(vm.value.as_i64(), Some(42));
    let unit = program.verified();
    let native =
        lang_codegen_clif::program::CompiledProgram::compile(unit, &[], &hosts, Default::default())
            .unwrap();
    assert_eq!(
        native
            .invoke(unit.unit().entry.unwrap(), &[])
            .unwrap()
            .cells,
        vec![42]
    );
}

#[test]
fn canonical_action_matches_independent_vectors_and_refuses_noncanonical_inputs() {
    let id = "a".repeat(32);
    let valid = YankAction::new(
        "math",
        "1.0.0",
        &id,
        1,
        YankActionKind::Yank,
        "release maintenance",
    )
    .unwrap();
    assert_eq!(valid.as_bytes().len(), 173);
    assert_eq!(
        digest(valid.as_bytes()),
        "d944ece5a45d67c089f6a0b81bfd9415c4fe5a23e11355e1b3ed90562d1c5dc1"
    );
    assert_eq!(
        valid.request_digest().unwrap(),
        "b61f815bee0a02abd24cd9e29e3ac22510c4b506e42b241692a0c3ec57ce6e47"
    );
    assert_eq!(
        YankAction::admit("math", valid.as_bytes())
            .unwrap()
            .as_bytes(),
        valid.as_bytes()
    );
    assert!(!format!("{valid:?}").contains("release maintenance"));
    for seed in 0..512 {
        let mut hostile = valid.as_bytes().to_vec();
        let index = seed % hostile.len();
        hostile[index] ^= 1 << (seed % 8);
        match YankAction::admit("math", &hostile) {
            Ok(admitted) => assert_eq!(admitted.as_bytes(), hostile),
            Err(error) => assert!(error.to_string().len() < 128),
        }
    }
    let text = std::str::from_utf8(valid.as_bytes()).unwrap();
    for invalid in [
        format!("{text}\n"),
        text.replace("\"schema\": 1", "\"schema\": 2"),
        text.replace("\"action\": \"yank\"", "\"action\": \"delete\""),
        text.replace("\"expected_revision\": 1", "\"expected_revision\": 0"),
        text.replace("\"schema\": 1,", "\"schema\": 1,\n  \"schema\": 1,"),
        text.replace("\"schema\": 1,", "\"schema\": 1,\n  \"unknown\": false,"),
        text.replace("release maintenance", ""),
        text.replace("release maintenance", "line\\nbreak"),
        text.replace(&id, &"A".repeat(32)),
    ] {
        assert_eq!(
            YankAction::admit("math", invalid.as_bytes())
                .unwrap_err()
                .kind,
            ErrorKind::Malformed
        );
    }
    assert_eq!(
        YankAction::admit("math", &vec![b' '; 2049])
            .unwrap_err()
            .kind,
        ErrorKind::Limit
    );
    for reason in ["x".repeat(256), "é".repeat(128)] {
        assert!(YankAction::new("math", "1.0.0", &id, 1, YankActionKind::Yank, &reason).is_ok());
    }
    for reason in ["x".repeat(257), "é".repeat(129), "control\u{85}".into()] {
        assert_eq!(
            YankAction::new("math", "1.0.0", &id, 1, YankActionKind::Yank, &reason)
                .unwrap_err()
                .kind,
            ErrorKind::Malformed
        );
    }
}

#[test]
fn yank_reopen_publish_unyank_preserve_exact_vm_native_archive_and_original_retry() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut registry = initialize_yank(&parent);
    let archive = archive("0.1.0");
    let bearer = bearer();
    let published = registry
        .publish(
            &bearer,
            &archive,
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    execute(&registry.archive(archive.sha256()).unwrap());
    let yank = action(YankActionKind::Yank, 'a', 1);
    let receipt = registry.yank(&bearer, &yank, &At(102)).unwrap();
    assert_eq!(receipt.state_revision(), 3);
    let index = Index::admit(&registry.index("answer").unwrap(), &identity(), "answer").unwrap();
    assert_eq!(index.revision(), 2);
    assert!(index.release("0.1.0").unwrap().yanked());
    let downloaded = registry.archive(archive.sha256()).unwrap();
    assert_eq!(downloaded.as_bytes(), archive.as_bytes());
    execute(&downloaded);
    assert_eq!(
        registry
            .publish(
                &bearer,
                &archive,
                REQUEST,
                ExpectedRevision::Absent,
                &At(103)
            )
            .unwrap(),
        published
    );
    let cross_kind = YankAction::new(
        "answer",
        "0.1.0",
        REQUEST,
        2,
        YankActionKind::Unyank,
        "different operation",
    )
    .unwrap();
    assert_eq!(
        registry
            .yank(&bearer, &cross_kind, &At(103))
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    assert_eq!(
        registry
            .publish(
                &bearer,
                &archive,
                &"c".repeat(32),
                ExpectedRevision::Revision(2),
                &At(103)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    drop(registry);
    let mut registry = reopen(&parent);
    assert_eq!(registry.yank(&bearer, &yank, &At(104)).unwrap(), receipt);
    let second = super::archive("0.2.0");
    registry
        .publish(
            &bearer,
            &second,
            &"2".repeat(32),
            ExpectedRevision::Revision(2),
            &At(105),
        )
        .unwrap();
    let index = Index::admit(&registry.index("answer").unwrap(), &identity(), "answer").unwrap();
    assert!(index.release("0.1.0").unwrap().yanked());
    assert!(!index.release("0.2.0").unwrap().yanked());
    let unyank = action(YankActionKind::Unyank, 'b', 3);
    assert_eq!(
        registry
            .yank(&bearer, &unyank, &At(106))
            .unwrap()
            .state_revision(),
        5
    );
    let stable_head = registry.storage.head().unwrap();
    assert_eq!(registry.yank(&bearer, &yank, &At(107)).unwrap(), receipt);
    assert_eq!(registry.storage.head().unwrap(), stable_head);
    let index = Index::admit(&registry.index("answer").unwrap(), &identity(), "answer").unwrap();
    assert_eq!(index.revision(), 4);
    assert!(!index.release("0.1.0").unwrap().yanked());
    assert_eq!(
        registry
            .yank(&bearer, &action(YankActionKind::Unyank, 'd', 4), &At(107))
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    assert_eq!(registry.storage.head().unwrap(), stable_head);
    drop(registry);
    let registry = reopen(&parent);
    let retained = registry.archive(archive.sha256()).unwrap();
    assert_eq!(retained.as_bytes(), archive.as_bytes());
    execute(&retained);
    assert_eq!(registry.revision().unwrap(), 5);
    assert_eq!(
        history::load(&registry.storage, &identity())
            .unwrap()
            .requests
            .len(),
        4
    );
}

#[test]
fn yank_requires_current_scope_revision_version_time_and_exact_request_identity() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut registry = initialize(&parent);
    let archive = archive("0.1.0");
    let bearer = bearer();
    registry
        .publish(
            &bearer,
            &archive,
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    let head = registry.storage.head().unwrap();
    assert_eq!(
        registry
            .yank(&bearer, &action(YankActionKind::Yank, 'a', 1), &At(102))
            .unwrap_err()
            .kind,
        ErrorKind::Authorization
    );
    assert_eq!(registry.storage.head().unwrap(), head);
    drop(registry);
    let other = Temp::new();
    let parent = super::parent(other.path());
    let mut registry = initialize_yank(&parent);
    registry
        .publish(
            &bearer,
            &archive,
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    let head = registry.storage.head().unwrap();
    for (request, time, kind) in [
        (
            action(YankActionKind::Yank, 'a', 2),
            102,
            ErrorKind::Conflict,
        ),
        (
            YankAction::new(
                "answer",
                "9.0.0",
                &"a".repeat(32),
                1,
                YankActionKind::Yank,
                "absent",
            )
            .unwrap(),
            102,
            ErrorKind::NotFound,
        ),
        (
            action(YankActionKind::Yank, 'a', 1),
            200,
            ErrorKind::Authentication,
        ),
        (action(YankActionKind::Yank, 'a', 1), 99, ErrorKind::State),
    ] {
        assert_eq!(
            registry
                .yank(&bearer, &request, &At(time))
                .unwrap_err()
                .kind,
            kind
        );
        assert_eq!(registry.storage.head().unwrap(), head);
    }
    let request = action(YankActionKind::Yank, 'a', 1);
    registry.yank(&bearer, &request, &At(102)).unwrap();
    let head = registry.storage.head().unwrap();
    let altered = YankAction::new(
        "answer",
        "0.1.0",
        &"a".repeat(32),
        1,
        YankActionKind::Yank,
        "different reason",
    )
    .unwrap();
    assert_eq!(
        registry.yank(&bearer, &altered, &At(103)).unwrap_err().kind,
        ErrorKind::Conflict
    );
    assert_eq!(
        registry.yank(&bearer, &request, &At(200)).unwrap_err().kind,
        ErrorKind::Authentication
    );
    assert_eq!(registry.storage.head().unwrap(), head);
}

#[test]
fn final_yank_guard_and_unknown_result_preserve_exact_reconciliation() {
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
        let mut registry = initialize_yank(&parent);
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
            .yank(
                &bearer(),
                &action(YankActionKind::Yank, 'a', 1),
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
                .yank(&bearer(), &action(YankActionKind::Yank, 'a', 1), &At(151))
                .unwrap()
                .state_revision(),
            3
        );
    }
    for fault in [Fault::Before, Fault::After, Fault::Unreadable] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut registry = initialize_yank(&parent);
        registry
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(101),
            )
            .unwrap();
        let request = action(YankActionKind::Yank, 'a', 1);
        FAULT.with(|f| f.set(Some(fault)));
        let result = registry.yank(&bearer(), &request, &At(110));
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
                    registry.yank(&bearer(), &request, &At(111)).unwrap(),
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
                        .yank(&bearer(), &request, &At(111))
                        .unwrap()
                        .state_revision(),
                    3
                );
                assert_eq!(registry.revision().unwrap(), 3);
            }
        }
    }
}

fn prepared(root: &std::path::Path, kind: YankActionKind) {
    let parent = parent(root);
    let mut registry = initialize_yank(&parent);
    registry
        .publish(
            &bearer(),
            &archive("0.1.0"),
            REQUEST,
            ExpectedRevision::Absent,
            &At(101),
        )
        .unwrap();
    if kind == YankActionKind::Unyank {
        registry
            .yank(&bearer(), &action(YankActionKind::Yank, 'a', 1), &At(102))
            .unwrap();
    }
}
#[test]
fn action_child() {
    let Some(root) = std::env::var_os("LANG_YANK_TEST_ROOT") else {
        return;
    };
    let kind = if std::env::var("LANG_YANK_TEST_KIND").unwrap() == "yank" {
        YankActionKind::Yank
    } else {
        YankActionKind::Unyank
    };
    let mut registry = reopen(&parent(std::path::Path::new(&root)));
    let cut = std::env::var("LANG_YANK_TEST_CUT")
        .unwrap()
        .parse()
        .unwrap();
    let request = action(kind, 'b', if kind == YankActionKind::Yank { 1 } else { 2 });
    CUT.with(|c| c.set(Some((cut, 0))));
    registry.yank(&bearer(), &request, &At(110)).unwrap();
    println!("YANK_CHECKPOINTS {}", CUT.with(|c| c.get().unwrap().1));
}
fn interrupted(root: &std::path::Path, kind: YankActionKind, cut: usize) -> std::process::Output {
    std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "registry::store::tests::yank::action_child",
            "--nocapture",
        ])
        .env("LANG_YANK_TEST_ROOT", root)
        .env(
            "LANG_YANK_TEST_KIND",
            if kind == YankActionKind::Yank {
                "yank"
            } else {
                "unyank"
            },
        )
        .env("LANG_YANK_TEST_CUT", cut.to_string())
        .output()
        .unwrap()
}
#[test]
fn every_yank_and_unyank_process_cut_reopens_complete_history_and_exact_archive() {
    for kind in [YankActionKind::Yank, YankActionKind::Unyank] {
        let tmp = Temp::new();
        prepared(tmp.path(), kind);
        let complete = interrupted(tmp.path(), kind, usize::MAX);
        assert!(
            complete.status.success(),
            "{}",
            String::from_utf8_lossy(&complete.stdout)
        );
        assert!(String::from_utf8_lossy(&complete.stdout).contains("YANK_CHECKPOINTS 18"));
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
            let old = if kind == YankActionKind::Yank { 2 } else { 3 };
            assert_eq!(registry.revision().unwrap(), old + u64::from(cut == 18));
            let index =
                Index::admit(&registry.index("answer").unwrap(), &identity(), "answer").unwrap();
            let target = kind == YankActionKind::Yank;
            assert_eq!(
                index.release("0.1.0").unwrap().yanked(),
                if cut == 18 { target } else { !target }
            );
            let archived = registry.archive(archive("0.1.0").sha256()).unwrap();
            assert_eq!(archived.as_bytes(), archive("0.1.0").as_bytes());
            execute(&archived);
            let receipt = registry
                .yank(&bearer(), &action(kind, 'b', old - 1), &At(111))
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
    println!("YANK_PROCESS_CUTS 36; old 34; new 2");
}

#[test]
fn actual_credentials_need_both_current_ownership_and_exact_yank_scope() {
    for (case, expected) in [
        (0, ErrorKind::Authorization),
        (1, ErrorKind::Authentication),
        (2, ErrorKind::Authentication),
        (3, ErrorKind::Authorization),
    ] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut authority = yank_authority();
        let other = Bearer::from_input("22".repeat(32).as_bytes()).unwrap();
        let mut token = authority.tokens[0].clone();
        token.id = "02".repeat(16);
        token.authenticator_sha256 = other.authenticator_sha256();
        token.grants = vec![authority::codec::Grant {
            action: authority::codec::Action::Yank,
            name: if case == 3 { "other" } else { "answer" }.into(),
        }];
        if case < 2 {
            token.principal = "2".repeat(32);
            authority.principals.push(authority::codec::Principal {
                id: token.principal.clone(),
                active: case == 0,
            });
        }
        token.revoked = case == 2;
        authority.tokens.push(token);
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
        let before = snapshot(tmp.path());
        assert_eq!(
            registry
                .yank(&other, &action(YankActionKind::Yank, 'a', 1), &At(102))
                .unwrap_err()
                .kind,
            expected
        );
        assert_eq!(snapshot(tmp.path()), before);
        assert_eq!(registry.revision().unwrap(), 2);
    }
}

#[test]
fn recomputed_yank_hashes_cannot_hide_other_changes_or_forge_noop_history() {
    for case in 0..9 {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut registry = initialize_yank(&parent);
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
            .yank(&bearer(), &action(YankActionKind::Yank, 'a', 2), &At(103))
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
            0 => state.packages[0].releases[0].yanked = false,
            1 => state.packages[0].releases[1].yanked = true,
            2 => state.authority.tokens[0].revoked = true,
            3 => audit.reason = "altered explanation".into(),
            4 => audit.request_sha256 = Some("ab".repeat(32)),
            5 => {
                let records::AuditResult::Yank(result) = &mut audit.result else {
                    panic!("yank result")
                };
                result.yanked = false;
            }
            6 => {
                // All digests/result fields agree on an unyank from an already
                // unyanked predecessor. Complete transition checking must refuse.
                let records::AuditResult::Yank(result) = &mut audit.result else {
                    panic!("yank result")
                };
                result.yanked = false;
                audit.operation = records::AuditOperation::Unyank {
                    name: result.name.clone(),
                    version: result.version.clone(),
                    archive_sha256: result.archive_sha256.clone(),
                };
                state.packages[0].releases[0].yanked = false;
                audit.request_sha256 = Some(
                    action(YankActionKind::Unyank, 'a', 2)
                        .request_digest()
                        .unwrap(),
                );
            }
            7 => audit.principal = "f".repeat(32),
            8 => state.packages[0].releases[0].quarantined = true,
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
    let Some(root) = std::env::var_os("LANG_YANK_WRITER_ROOT") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let parent = parent(root);
    let mode = std::env::var("LANG_YANK_WRITER_MODE").unwrap();
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
            .yank(&bearer(), &action(YankActionKind::Yank, 'a', 1), &At(110))
            .unwrap();
    }
}
fn action_writer(root: &std::path::Path, mode: &str) -> Child {
    Child(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "registry::store::tests::yank::competing_action_child",
                "--nocapture",
            ])
            .env("LANG_YANK_WRITER_ROOT", root)
            .env("LANG_YANK_WRITER_MODE", mode)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    )
}
#[test]
fn competing_publish_and_yank_serialize_both_orders_without_replacing_archives() {
    for mode in ["publish", "yank"] {
        let tmp = Temp::new();
        prepared(tmp.path(), YankActionKind::Yank);
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
                    .yank(&bearer(), &action(YankActionKind::Yank, 'b', 1), &At(111))
                    .unwrap_err()
                    .kind,
                ErrorKind::Conflict
            );
            assert_eq!(registry.storage.head().unwrap(), old);
            registry
                .yank(&bearer(), &action(YankActionKind::Yank, 'b', 2), &At(111))
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
        assert!(index.release("0.1.0").unwrap().yanked());
        assert!(!index.release("0.2.0").unwrap().yanked());
        assert_eq!(registry.revision().unwrap(), 4);
        let retained = registry.archive(archive("0.1.0").sha256()).unwrap();
        assert_eq!(retained.as_bytes(), archive("0.1.0").as_bytes());
        execute(&retained);
    }
}
