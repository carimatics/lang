use super::*;
use crate::registry::budget::Budget;
use crate::registry::transport::MAX_REQUEST_MEMORY;

#[test]
fn metered_publish_refresh_retry_and_source_execute_with_exact_bytes() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut metered = MeteredRegistry::new(initialize(&parent), &At(100)).unwrap();
    let source = archive("0.1.0");
    let receipt = metered
        .publish(
            &bearer(),
            source.as_bytes(),
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
    assert_eq!(receipt.state_revision(), 2);
    let index = metered.index("answer").unwrap().to_vec();
    let retained = metered.archive(source.sha256()).unwrap();
    assert_eq!(retained.as_bytes(), source.as_bytes());
    let admitted = AdmittedArchive::admit(retained.as_bytes()).unwrap();
    yank::execute(&admitted);
    assert_eq!(
        metered
            .publish(
                &bearer(),
                source.as_bytes(),
                REQUEST,
                ExpectedRevision::Absent,
                &At(111)
            )
            .unwrap(),
        receipt
    );
    assert_eq!(metered.index("answer").unwrap(), index);
    let second = archive("0.2.0");
    metered
        .publish(
            &bearer(),
            second.as_bytes(),
            &"02".repeat(16),
            ExpectedRevision::Revision(1),
            &At(112),
        )
        .unwrap();
    assert_ne!(metered.index("answer").unwrap(), index);
    // An old exact retry must not roll the public read generation back.
    let current = metered.index("answer").unwrap().to_vec();
    assert_eq!(
        metered
            .publish(
                &bearer(),
                source.as_bytes(),
                REQUEST,
                ExpectedRevision::Absent,
                &At(113)
            )
            .unwrap(),
        receipt
    );
    assert_eq!(metered.index("answer").unwrap(), current);
    assert_eq!(retained.as_bytes(), source.as_bytes());
    yank::execute(
        &AdmittedArchive::admit(metered.archive(second.sha256()).unwrap().as_bytes()).unwrap(),
    );
    drop(metered);
    let reopened = MeteredRegistry::new(reopen(&parent), &At(114)).unwrap();
    assert_eq!(reopened.index("answer").unwrap(), current);
}

#[test]
fn precommit_refusals_do_not_poison_generation_or_write_candidates() {
    struct Cancel;
    impl Clock for Cancel {
        fn now(&self) -> Result<u64, Error> {
            Err(error(ErrorKind::Io, "cancelled before admission"))
        }
    }
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut metered = MeteredRegistry::new(initialize(&parent), &At(100)).unwrap();
    let source = archive("0.1.0");
    let before = snapshot(tmp.path());
    assert_eq!(
        metered
            .publish(
                &bearer(),
                source.as_bytes(),
                REQUEST,
                ExpectedRevision::Absent,
                &Cancel
            )
            .err()
            .unwrap()
            .kind,
        ErrorKind::Io
    );
    assert_eq!(snapshot(tmp.path()), before);
    assert!(
        metered
            .publish(
                &bearer(),
                b"not a source archive",
                REQUEST,
                ExpectedRevision::Absent,
                &At(110)
            )
            .is_err()
    );
    assert_eq!(snapshot(tmp.path()), before);
    let mut changed_source = source.as_bytes().to_vec();
    let offset = changed_source
        .windows(SOURCE.len())
        .position(|part| part == SOURCE)
        .unwrap();
    changed_source[offset] ^= 1;
    assert_eq!(
        metered
            .publish(
                &bearer(),
                &changed_source,
                REQUEST,
                ExpectedRevision::Absent,
                &At(110)
            )
            .err()
            .unwrap()
            .kind,
        ErrorKind::Integrity
    );
    assert_eq!(snapshot(tmp.path()), before);
    metered
        .publish(
            &bearer(),
            source.as_bytes(),
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
    yank::execute(
        &AdmittedArchive::admit(metered.archive(source.sha256()).unwrap().as_bytes()).unwrap(),
    );
}

#[test]
fn actual_shared_pressure_refuses_before_write_and_response_lease_outlives_owner() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let budget = Budget::new(MAX_REQUEST_MEMORY);
    let registry = initialize(&parent);
    let stage = tmp
        .path()
        .join("registry/staging")
        .join(format!("state-{}", "00".repeat(32)));
    std::fs::write(&stage, b"{").unwrap();
    let mut metered = MeteredRegistry::with_budget(registry, budget.clone(), &At(100)).unwrap();
    let baseline = budget.used();
    let pressure = budget
        .reserve(MAX_REQUEST_MEMORY - baseline - 2 * 1024 * 1024)
        .unwrap();
    let source = archive("0.1.0");
    let before = snapshot(tmp.path());
    assert_eq!(
        metered
            .publish(
                &bearer(),
                source.as_bytes(),
                REQUEST,
                ExpectedRevision::Absent,
                &At(110)
            )
            .err()
            .unwrap()
            .kind,
        ErrorKind::Busy
    );
    assert_eq!(snapshot(tmp.path()), before);
    assert_eq!(budget.used(), MAX_REQUEST_MEMORY - 2 * 1024 * 1024);
    drop(pressure);
    assert_eq!(budget.used(), baseline);
    metered
        .publish(
            &bearer(),
            source.as_bytes(),
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
    let generation = budget.used();
    assert!(
        !stage.exists(),
        "validated recovery can clean the recognized orphan only after admission"
    );
    let response = metered.archive(source.sha256()).unwrap();
    assert_eq!(budget.used(), generation + source.as_bytes().len());
    drop(metered);
    assert_eq!(budget.used(), source.as_bytes().len());
    yank::execute(&AdmittedArchive::admit(response.as_bytes()).unwrap());
    drop(response);
    assert_eq!(budget.used(), 0);
}

#[test]
fn committed_refresh_failure_refuses_reads_and_reconciles_only_after_reopen() {
    struct AfterHead {
        path: std::path::PathBuf,
        old: Vec<u8>,
    }
    impl Clock for AfterHead {
        fn now(&self) -> Result<u64, Error> {
            if std::fs::read(&self.path).unwrap() != self.old {
                Err(error(ErrorKind::Io, "injected postcommit refresh deadline"))
            } else {
                Ok(110)
            }
        }
    }
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let budget = Budget::new(MAX_REQUEST_MEMORY);
    let mut metered =
        MeteredRegistry::with_budget(initialize(&parent), budget.clone(), &At(100)).unwrap();
    let baseline = budget.used();
    let path = tmp.path().join("registry/HEAD");
    let clock = AfterHead {
        old: std::fs::read(&path).unwrap(),
        path,
    };
    let source = archive("0.1.0");
    assert_eq!(
        metered
            .publish(
                &bearer(),
                source.as_bytes(),
                REQUEST,
                ExpectedRevision::Absent,
                &clock
            )
            .err()
            .unwrap()
            .kind,
        ErrorKind::State
    );
    assert_ne!(std::fs::read(&clock.path).unwrap(), clock.old);
    assert_eq!(budget.used(), baseline);
    assert_eq!(
        metered.index("answer").err().unwrap().kind,
        ErrorKind::State
    );
    assert_eq!(
        metered.archive(source.sha256()).err().unwrap().kind,
        ErrorKind::State
    );
    assert_eq!(
        metered
            .publish(
                &bearer(),
                source.as_bytes(),
                REQUEST,
                ExpectedRevision::Absent,
                &At(111)
            )
            .err()
            .unwrap()
            .kind,
        ErrorKind::State
    );
    drop(metered);
    assert_eq!(budget.used(), 0);
    let mut metered = MeteredRegistry::new(reopen(&parent), &At(111)).unwrap();
    let receipt = metered
        .publish(
            &bearer(),
            source.as_bytes(),
            REQUEST,
            ExpectedRevision::Absent,
            &At(112),
        )
        .unwrap();
    assert_eq!(receipt.state_revision(), 2);
    yank::execute(
        &AdmittedArchive::admit(metered.archive(source.sha256()).unwrap().as_bytes()).unwrap(),
    );
}

#[test]
fn metadata_changes_after_probe_are_bounded_before_decode_or_cleanup() {
    struct Count(Cell<usize>);
    impl Clock for Count {
        fn now(&self) -> Result<u64, Error> {
            self.0.set(self.0.get() + 1);
            Ok(100)
        }
    }
    struct Change {
        calls: Cell<usize>,
        trigger: usize,
        path: std::path::PathBuf,
        state_length: Option<u64>,
    }
    impl Clock for Change {
        fn now(&self) -> Result<u64, Error> {
            let calls = self.calls.get() + 1;
            self.calls.set(calls);
            if calls == self.trigger {
                if let Some(length) = self.state_length {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .open(&self.path)
                        .unwrap()
                        .set_len(length)
                        .unwrap();
                } else {
                    for i in 0..9 {
                        std::fs::write(self.path.join(format!("state-{i:064x}")), b"{").unwrap();
                    }
                }
            }
            Ok(100)
        }
    }
    for grow_state in [true, false] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let registry = initialize(&parent);
        let count = Count(Cell::new(0));
        let probe = registry.storage.probe(&count).unwrap();
        let head = std::fs::read(tmp.path().join("registry/HEAD")).unwrap();
        let path = if grow_state {
            tmp.path().join("registry/states").join(&probe.head)
        } else {
            tmp.path().join("registry/staging")
        };
        let change = Change {
            calls: Cell::new(0),
            trigger: count.0.get() + 1,
            path,
            state_length: grow_state.then_some((probe.state + 4096 + 1) as u64),
        };
        let budget = Budget::new(MAX_REQUEST_MEMORY);
        assert_eq!(
            MeteredRegistry::with_budget(registry, budget.clone(), &change)
                .err()
                .unwrap()
                .kind,
            ErrorKind::Limit
        );
        assert_eq!(budget.used(), 0);
        assert_eq!(
            std::fs::read(tmp.path().join("registry/HEAD")).unwrap(),
            head
        );
        if grow_state {
            assert_eq!(
                std::fs::metadata(&change.path).unwrap().len(),
                change.state_length.unwrap()
            );
        } else {
            assert_eq!(std::fs::read_dir(&change.path).unwrap().count(), 9);
        }
    }
}

#[test]
fn guarded_owner_and_yank_refresh_reauthorize_without_cached_membership() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let other = "00000000000000000000000000000002";
    let mut auth: authority::codec::Records = serde_json::from_slice(&authority_bytes()).unwrap();
    auth.principals.push(authority::codec::Principal {
        id: other.into(),
        active: true,
    });
    for action in [
        authority::codec::Action::Owners,
        authority::codec::Action::Yank,
    ] {
        auth.tokens[0].grants.push(authority::codec::Grant {
            action,
            name: "answer".into(),
        });
    }
    auth.tokens[0]
        .grants
        .sort_by(|a, b| a.action.cmp(&b.action).then(a.name.cmp(&b.name)));
    let registry = Registry::initialize(
        &parent,
        "registry",
        &identity(),
        &records::encode(&auth, authority::MAX_BYTES).unwrap(),
        PRINCIPAL,
        &At(100),
    )
    .unwrap();
    let budget = Budget::new(MAX_REQUEST_MEMORY);
    let mut metered = MeteredRegistry::with_budget(registry, budget.clone(), &At(100)).unwrap();
    let source = archive("0.1.0");
    metered
        .publish(
            &bearer(),
            source.as_bytes(),
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
    let add = OwnerAction::new(
        "answer",
        other,
        &"02".repeat(16),
        1,
        OwnerActionKind::AddOwner,
        "transfer",
    )
    .unwrap();
    metered
        .change_owner(&bearer(), "answer", add.as_bytes(), &At(111))
        .unwrap();
    let generation = budget.used();
    let response = metered.owners(&bearer(), "answer", &At(112)).unwrap();
    assert_eq!(budget.used(), generation + response.as_bytes().len());
    let owners: serde_json::Value = serde_json::from_slice(response.as_bytes()).unwrap();
    assert_eq!(owners["owners"], serde_json::json!([PRINCIPAL, other]));
    drop(response);
    assert_eq!(budget.used(), generation);
    let yank = YankAction::new(
        "answer",
        "0.1.0",
        &"03".repeat(16),
        2,
        YankActionKind::Yank,
        "review",
    )
    .unwrap();
    metered
        .yank(&bearer(), "answer", yank.as_bytes(), &At(113))
        .unwrap();
    let index: serde_json::Value =
        serde_json::from_slice(metered.index("answer").unwrap()).unwrap();
    assert_eq!(index["revision"], 3);
    assert_eq!(index["releases"][0]["yanked"], true);
    let remove = OwnerAction::new(
        "answer",
        PRINCIPAL,
        &"04".repeat(16),
        3,
        OwnerActionKind::RemoveOwner,
        "transfer",
    )
    .unwrap();
    metered
        .change_owner(&bearer(), "answer", remove.as_bytes(), &At(114))
        .unwrap();
    assert_eq!(
        metered
            .owners(&bearer(), "answer", &At(115))
            .err()
            .unwrap()
            .kind,
        ErrorKind::Authorization
    );
    assert_eq!(
        metered
            .change_owner(&bearer(), "answer", remove.as_bytes(), &At(115))
            .err()
            .unwrap()
            .kind,
        ErrorKind::Authorization
    );
    let current: serde_json::Value =
        serde_json::from_slice(metered.index("answer").unwrap()).unwrap();
    assert_eq!(current["revision"], 4);
    yank::execute(
        &AdmittedArchive::admit(metered.archive(source.sha256()).unwrap().as_bytes()).unwrap(),
    );
    drop(metered);
    assert_eq!(budget.used(), 0);
}

#[test]
fn exact_workspace_budget_admits_at_boundary_and_static_shortage_is_a_limit() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    drop(initialize(&parent));
    let root = tmp.path().join("registry");
    let state_bytes = std::fs::read_dir(root.join("states"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .metadata()
        .unwrap()
        .len() as usize;
    // Independently count physical entries and apply the accepted RFC formula.
    let entries = std::fs::read_dir(&root).unwrap().count()
        + ["states", "audits", "objects", "staging"]
            .iter()
            .map(|n| std::fs::read_dir(root.join(n)).unwrap().count())
            .sum::<usize>();
    let candidate = state_bytes + 4096;
    let workspace = 96 * candidate + 8192 * (entries + 8) + 2 * 1024 * 1024;
    let generation = 2 * candidate + (1024 + 8192) * 1024 + 4096;
    let required = workspace + generation;
    let before = snapshot(tmp.path());
    for maximum in [required - 1, required, required + 1] {
        let budget = Budget::new(maximum);
        let result = MeteredRegistry::with_budget(reopen(&parent), budget.clone(), &At(100));
        if maximum < required {
            assert_eq!(result.err().unwrap().kind, ErrorKind::Limit);
        } else {
            let mut metered = result.unwrap();
            assert_eq!(budget.used(), 4096);
            let source = archive("0.1.0");
            assert_eq!(
                metered
                    .publish(
                        &bearer(),
                        source.as_bytes(),
                        REQUEST,
                        ExpectedRevision::Absent,
                        &At(110)
                    )
                    .err()
                    .unwrap()
                    .kind,
                ErrorKind::Limit
            );
            assert_eq!(
                metered.index("answer").err().unwrap().kind,
                ErrorKind::NotFound
            );
            assert_eq!(budget.used(), 4096);
            drop(metered);
        }
        assert_eq!(budget.used(), 0);
        assert_eq!(snapshot(tmp.path()), before);
    }
}

#[test]
fn retained_generation_capacity_refuses_one_byte_short_without_cleanup() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut registry = initialize(&parent);
    let source = archive("0.1.0");
    registry
        .publish(
            &bearer(),
            &source,
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
    let index = registry.index("answer").unwrap();
    let expected = 4096 + 1024 + index.len() + 1024;
    let stage = tmp
        .path()
        .join("registry/staging")
        .join(format!("state-{}", "00".repeat(32)));
    std::fs::write(&stage, b"{").unwrap();
    let before = snapshot(tmp.path());
    for maximum in [expected - 1, expected, expected + 1] {
        let result = super::super::reads::ReadGeneration::capture(&registry, maximum);
        if maximum < expected {
            assert_eq!(result.err().unwrap().kind, ErrorKind::Limit);
        } else {
            let generation = result.unwrap();
            assert_eq!(generation.memory_bytes(), expected);
            assert_eq!(generation.index(&registry, "answer").unwrap(), index);
            assert_eq!(
                generation
                    .archive_length(&registry, source.sha256())
                    .unwrap(),
                source.as_bytes().len()
            );
        }
        assert_eq!(snapshot(tmp.path()), before);
    }
}
