use super::*;

const SECOND: &str = "00000000000000000000000000000002";
const SOURCE: &[u8] = b"fn main() -> i64 { return 42; }\r\n";

fn owner_fixture() -> Fixture {
    owner_fixture_with(|_| {})
}

fn owner_fixture_with(configure: impl FnOnce(&mut authority::codec::Records)) -> Fixture {
    let archive = crate::registry::transport::tests::binding::archive(SOURCE);
    Fixture::seed_with(archive, false, |records| {
        records
            .principals
            .push(serde_json::from_value(serde_json::json!({"id":SECOND,"active":true})).unwrap());
        for action in [
            authority::codec::Action::Yank,
            authority::codec::Action::Owners,
        ] {
            records.tokens[0].grants.push(authority::codec::Grant {
                action,
                name: "answer".into(),
            });
        }
        records.tokens[0]
            .grants
            .sort_by(|a, b| (a.action, &a.name).cmp(&(b.action, &b.name)));
        let mut second = records.tokens[0].clone();
        second.id = "02".repeat(16);
        second.principal = SECOND.into();
        second.authenticator_sha256 = Bearer::from_input("22".repeat(32).as_bytes())
            .unwrap()
            .authenticator_sha256();
        records.tokens.push(second);
        configure(records);
    })
}

fn start_mutable(fixture: &mut Fixture, time: u64) -> (Server, Client, rustls::RootCertStore) {
    let (certificates, key, roots) = fixture.credentials();
    let server = Server::spawn_mutable(
        fixture.registry.take().unwrap(),
        fixture.listener.take().unwrap(),
        certificates,
        key,
        Arc::new(At(time)),
    )
    .unwrap();
    (
        server,
        Client::new(fixture.identity.clone(), roots.clone()),
        roots,
    )
}

fn controlled_service(
    fixture: &mut Fixture,
    clock: Arc<dyn Clock + Send + Sync>,
    timeout: Duration,
    budget: Arc<Budget>,
) -> (Server, Client, rustls::RootCertStore) {
    let (certificates, key, roots) = fixture.credentials();
    let server = Server::start_mutable(
        fixture.registry.take().unwrap(),
        fixture.listener.take().unwrap(),
        configuration(certificates, key).unwrap(),
        budget,
        CONNECT_TIMEOUT,
        timeout,
        clock,
    )
    .unwrap();
    (
        server,
        Client::new(fixture.identity.clone(), roots.clone()),
        roots,
    )
}

fn publication_header(fixture: &Fixture, id: &str) -> Vec<u8> {
    format!("PUT /v1/packages/answer/1.0.0 HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/x-tar\r\nContent-Length: {}\r\nIdempotency-Key: {id}\r\nIf-None-Match: *\r\nAuthorization: Bearer {}\r\n\r\n", authority(&fixture.identity), fixture.archive.as_bytes().len(), "11".repeat(32)).into_bytes()
}

fn execute(archive: &AdmittedArchive) {
    let analysis = crate::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        std::path::Path::new("main.lang"),
        archive.source("main.lang").unwrap(),
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
    let result = lang_vm::unit::execute_entry(
        &program,
        &hosts,
        &lang_vm::unit::HostBindings::new(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value.as_i64(), Some(42));
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
fn actual_https_quarantine_refresh_refuses_stale_download_and_restores_identical_source() {
    let mut fixture = owner_fixture_with(|records| {
        records.tokens[1].grants = vec![authority::codec::Grant {
            action: authority::codec::Action::Admin,
            name: "answer".into(),
        }];
    });
    let budget = Budget::new(MAX_REQUEST_MEMORY);
    let (mut server, client, _) = controlled_service(
        &mut fixture,
        Arc::new(At(110)),
        Duration::from_secs(120),
        budget.clone(),
    );
    let owner = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    let admin = Bearer::from_input("22".repeat(32).as_bytes()).unwrap();
    client
        .publish(
            &owner,
            &fixture.archive,
            &"a".repeat(32),
            ExpectedRevision::Absent,
        )
        .unwrap();
    let prior = client.index("answer").unwrap();
    execute(&client.archive(&prior, "1.0.0").unwrap());
    let quarantine = store::QuarantineAction::new(
        "answer",
        "1.0.0",
        &"b".repeat(32),
        1,
        store::QuarantineActionKind::Quarantine,
        "security review",
    )
    .unwrap();
    let refusal = client.quarantine(&owner, &quarantine).unwrap_err();
    assert_eq!(refusal.error.kind, ErrorKind::Authorization);
    assert_eq!(refusal.delivery, Delivery::Refused);
    let receipt = client.quarantine(&admin, &quarantine).unwrap();
    assert_eq!(receipt.outcome(), MutationOutcome::Quarantined);
    assert_eq!(receipt.revision(), 2);
    assert_eq!(receipt.audit_sequence(), 3);
    let quarantined = client.index("answer").unwrap();
    assert!(quarantined.release("1.0.0").unwrap().quarantined());
    // A caller retaining the previously admitted index still reaches the new
    // server generation; quarantine must not be merely a client-side check.
    assert_eq!(
        client.archive(&prior, "1.0.0").err().unwrap().kind,
        ErrorKind::NotFound
    );
    let yank = store::YankAction::new(
        "answer",
        "1.0.0",
        &"c".repeat(32),
        2,
        store::YankActionKind::Yank,
        "owner selection",
    )
    .unwrap();
    client.yank(&owner, &yank).unwrap();
    assert_eq!(
        client.archive(&prior, "1.0.0").err().unwrap().kind,
        ErrorKind::NotFound
    );
    let restore = store::QuarantineAction::new(
        "answer",
        "1.0.0",
        &"d".repeat(32),
        3,
        store::QuarantineActionKind::Unquarantine,
        "review completed",
    )
    .unwrap();
    let restored = client.quarantine(&admin, &restore).unwrap();
    assert_eq!(restored.outcome(), MutationOutcome::Unquarantined);
    assert_eq!(restored.revision(), 4);
    let current = client.index("answer").unwrap();
    assert!(!current.release("1.0.0").unwrap().quarantined());
    assert!(current.release("1.0.0").unwrap().yanked());
    let exact = client.archive(&current, "1.0.0").unwrap();
    assert_eq!(exact.as_bytes(), fixture.archive.as_bytes());
    execute(&exact);
    assert_eq!(client.quarantine(&admin, &quarantine).unwrap(), receipt);
    let current = client.index("answer").unwrap();
    assert_eq!(current.revision(), 4);
    assert!(!current.release("1.0.0").unwrap().quarantined());
    execute(&client.archive(&current, "1.0.0").unwrap());
    server.stop().unwrap();
    assert_eq!(budget.used(), 0);
    let registry = Registry::open(&fixture.parent, "registry", &fixture.identity).unwrap();
    execute(&registry.archive(fixture.archive.sha256()).unwrap());
}

#[test]
fn actual_https_publication_replay_and_refreshed_download_execute_in_vm_and_native() {
    let archive =
        crate::registry::transport::tests::binding::archive(b"fn main() -> i64 { return 42; }\r\n");
    let mut fixture = Fixture::seed(archive, false);
    let (certificates, key, roots) = fixture.credentials();
    let mut server = Server::spawn_mutable(
        fixture.registry.take().unwrap(),
        fixture.listener.take().unwrap(),
        certificates,
        key,
        Arc::new(At(110)),
    )
    .unwrap();
    let client = Client::new(fixture.identity.clone(), roots);
    assert_eq!(
        client.index("answer").err().unwrap().kind,
        ErrorKind::NotFound
    );
    let bearer = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    let id = "a".repeat(32);
    let first = client
        .publish(&bearer, &fixture.archive, &id, ExpectedRevision::Absent)
        .unwrap();
    assert!(first.created);
    assert_eq!(first.receipt.revision(), 1);
    assert_eq!(first.receipt.audit_sequence(), 2);
    assert_eq!(first.receipt.outcome(), MutationOutcome::Published);
    assert_eq!(first.receipt.request_id(), id);
    let index = client.index("answer").unwrap();
    let downloaded = client.archive(&index, "1.0.0").unwrap();
    assert_eq!(downloaded.as_bytes(), fixture.archive.as_bytes());
    execute(&downloaded);
    let replay = client
        .publish(&bearer, &fixture.archive, &id, ExpectedRevision::Absent)
        .unwrap();
    assert!(!replay.created);
    assert_eq!(replay.receipt, first.receipt);
    let conflict = client
        .publish(
            &bearer,
            &fixture.archive,
            &"b".repeat(32),
            ExpectedRevision::Absent,
        )
        .unwrap_err();
    assert_eq!(conflict.error.kind, ErrorKind::Conflict);
    assert_eq!(conflict.delivery, Delivery::Refused);
    let before_send = client
        .publish(
            &bearer,
            &fixture.archive,
            "bad-id",
            ExpectedRevision::Absent,
        )
        .unwrap_err();
    assert_eq!(before_send.delivery, Delivery::NotSent);
    execute(
        &client
            .archive(&client.index("answer").unwrap(), "1.0.0")
            .unwrap(),
    );
    server.stop().unwrap();
    let registry = Registry::open(&fixture.parent, "registry", &fixture.identity).unwrap();
    assert_eq!(
        registry
            .archive(fixture.archive.sha256())
            .unwrap()
            .as_bytes(),
        fixture.archive.as_bytes()
    );
}

#[test]
fn actual_https_owner_transfer_yank_and_old_receipts_keep_current_authority_and_generation() {
    let mut fixture = owner_fixture();
    let (mut server, client, _) = start_mutable(&mut fixture, 110);
    let a = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    let b = Bearer::from_input("22".repeat(32).as_bytes()).unwrap();
    let published = client
        .publish(
            &a,
            &fixture.archive,
            &"a".repeat(32),
            ExpectedRevision::Absent,
        )
        .unwrap();
    assert_eq!(client.owners(&a, "answer").unwrap().owners(), &[PRINCIPAL]);
    assert_eq!(
        client.owners(&b, "answer").unwrap_err().kind,
        ErrorKind::Authorization
    );
    let add = store::OwnerAction::new(
        "answer",
        SECOND,
        &"b".repeat(32),
        1,
        store::OwnerActionKind::AddOwner,
        "transfer",
    )
    .unwrap();
    let added = client.change_owner(&a, &add).unwrap();
    assert_eq!(added.revision(), 2);
    assert_eq!(added.outcome(), MutationOutcome::OwnerAdded);
    assert_eq!(
        client.owners(&b, "answer").unwrap().owners(),
        &[PRINCIPAL, SECOND]
    );
    let yank = store::YankAction::new(
        "answer",
        "1.0.0",
        &"c".repeat(32),
        2,
        store::YankActionKind::Yank,
        "maintenance",
    )
    .unwrap();
    let yanked = client.yank(&a, &yank).unwrap();
    assert_eq!(yanked.revision(), 3);
    assert!(
        client
            .index("answer")
            .unwrap()
            .release("1.0.0")
            .unwrap()
            .yanked()
    );
    assert_eq!(
        client
            .publish(
                &a,
                &fixture.archive,
                &"a".repeat(32),
                ExpectedRevision::Absent
            )
            .unwrap()
            .receipt,
        published.receipt
    );
    assert_eq!(client.index("answer").unwrap().revision(), 3);
    execute(
        &client
            .archive(&client.index("answer").unwrap(), "1.0.0")
            .unwrap(),
    );
    let remove = store::OwnerAction::new(
        "answer",
        PRINCIPAL,
        &"d".repeat(32),
        3,
        store::OwnerActionKind::RemoveOwner,
        "transfer",
    )
    .unwrap();
    let removed = client.change_owner(&a, &remove).unwrap();
    assert_eq!(removed.revision(), 4);
    assert_eq!(removed.outcome(), MutationOutcome::OwnerRemoved);
    assert_eq!(
        client.owners(&a, "answer").unwrap_err().kind,
        ErrorKind::Authorization
    );
    assert_eq!(
        client.change_owner(&a, &remove).unwrap_err().error.kind,
        ErrorKind::Authorization
    );
    assert_eq!(
        client.yank(&a, &yank).unwrap_err().error.kind,
        ErrorKind::Authorization
    );
    assert_eq!(client.owners(&b, "answer").unwrap().owners(), &[SECOND]);
    let unyank = store::YankAction::new(
        "answer",
        "1.0.0",
        &"e".repeat(32),
        4,
        store::YankActionKind::Unyank,
        "restored",
    )
    .unwrap();
    let unyanked = client.yank(&b, &unyank).unwrap();
    assert_eq!(unyanked.revision(), 5);
    assert!(
        !client
            .index("answer")
            .unwrap()
            .release("1.0.0")
            .unwrap()
            .yanked()
    );
    let next = crate::registry::transport::tests::binding::archive_files_version(
        &[("main.lang", SOURCE)],
        "1.0.1",
    );
    let next_receipt = client
        .publish(&b, &next, &"f".repeat(32), ExpectedRevision::Revision(5))
        .unwrap();
    assert_eq!(next_receipt.receipt.revision(), 6);
    assert!(next_receipt.created);
    assert_eq!(client.yank(&b, &unyank).unwrap(), unyanked);
    let current = client.index("answer").unwrap();
    assert_eq!(current.revision(), 6);
    execute(&client.archive(&current, "1.0.1").unwrap());
    server.stop().unwrap();
}

#[test]
fn actual_service_stop_local_revocation_and_reopen_refuse_old_credentials_and_retries() {
    let mut fixture = owner_fixture();
    // The host explicitly retains its listener capability across downtime;
    // no second service or writer remains running.
    let reopen_listener = fixture.listener.as_ref().unwrap().try_clone().unwrap();
    let (mut server, client, _) = start_mutable(&mut fixture, 110);
    let a = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    let b = Bearer::from_input("22".repeat(32).as_bytes()).unwrap();
    let id = "a".repeat(32);
    client
        .publish(&a, &fixture.archive, &id, ExpectedRevision::Absent)
        .unwrap();
    let add = store::OwnerAction::new(
        "answer",
        SECOND,
        &"b".repeat(32),
        1,
        store::OwnerActionKind::AddOwner,
        "retain second owner",
    )
    .unwrap();
    client.change_owner(&a, &add).unwrap();
    assert_eq!(
        store::Administrator::open(&fixture.parent, "registry", &fixture.identity, PRINCIPAL)
            .err()
            .unwrap()
            .kind,
        store::ErrorKind::Busy
    );
    server.stop().unwrap();
    let mut admin =
        store::Administrator::open(&fixture.parent, "registry", &fixture.identity, PRINCIPAL)
            .unwrap();
    assert_eq!(admin.revision().unwrap(), 3);
    let revoke =
        store::Revocation::new(&"01".repeat(16), &"c".repeat(32), 3, "retire credential").unwrap();
    assert_eq!(admin.revoke(&revoke, &At(120)).unwrap().state_revision(), 4);
    drop(admin);
    fixture.registry =
        Some(Registry::open(&fixture.parent, "registry", &fixture.identity).unwrap());
    fixture.listener = Some(reopen_listener);
    let (mut reopened, client, _) = start_mutable(&mut fixture, 120);
    let retry = client
        .publish(&a, &fixture.archive, &id, ExpectedRevision::Absent)
        .unwrap_err();
    assert_eq!(retry.error.kind, ErrorKind::Authentication);
    assert_eq!(retry.delivery, Delivery::Refused);
    assert_eq!(
        client.owners(&a, "answer").unwrap_err().kind,
        ErrorKind::Authentication
    );
    assert_eq!(
        client.change_owner(&a, &add).unwrap_err().error.kind,
        ErrorKind::Authentication
    );
    let next = crate::registry::transport::tests::binding::archive_files_version(
        &[("main.lang", SOURCE)],
        "1.0.1",
    );
    let receipt = client
        .publish(&b, &next, &"d".repeat(32), ExpectedRevision::Revision(2))
        .unwrap();
    assert_eq!(receipt.receipt.revision(), 3);
    assert_eq!(receipt.receipt.audit_sequence(), 5);
    execute(
        &client
            .archive(&client.index("answer").unwrap(), "1.0.1")
            .unwrap(),
    );
    reopened.stop().unwrap();
}

struct FailCommitted {
    head: std::path::PathBuf,
    initial: Vec<u8>,
}
impl Clock for FailCommitted {
    fn now(&self) -> Result<u64, store::Error> {
        if std::fs::read(&self.head).unwrap() != self.initial {
            return Err(store::Error {
                kind: store::ErrorKind::Io,
                outcome: store::PublicationOutcome::NotPublished,
                message: "fixture refresh interruption",
            });
        }
        Ok(110)
    }
}

#[test]
fn committed_https_refresh_failure_is_unknown_and_requires_reopen_for_original_receipt() {
    let mut fixture = owner_fixture();
    let reopen_listener = fixture.listener.as_ref().unwrap().try_clone().unwrap();
    let head = fixture.temp.0.join("registry/HEAD");
    let initial = std::fs::read(&head).unwrap();
    let budget = Budget::new(MAX_REQUEST_MEMORY);
    let (mut server, client, _) = controlled_service(
        &mut fixture,
        Arc::new(FailCommitted { head, initial }),
        REQUEST_TIMEOUT,
        budget.clone(),
    );
    let bearer = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    let id = "a".repeat(32);
    let result = client
        .publish(&bearer, &fixture.archive, &id, ExpectedRevision::Absent)
        .unwrap_err();
    assert_eq!(result.error.kind, ErrorKind::State);
    assert_eq!(result.delivery, Delivery::Unknown);
    assert_eq!(client.index("answer").err().unwrap().kind, ErrorKind::State);
    assert_eq!(
        client
            .publish(&bearer, &fixture.archive, &id, ExpectedRevision::Absent)
            .unwrap_err()
            .delivery,
        Delivery::Unknown
    );
    server.stop().unwrap();
    assert_eq!(budget.used(), 0);
    fixture.registry =
        Some(Registry::open(&fixture.parent, "registry", &fixture.identity).unwrap());
    fixture.listener = Some(reopen_listener);
    let (mut reopened, client, _) = start_mutable(&mut fixture, 110);
    let receipt = client
        .publish(&bearer, &fixture.archive, &id, ExpectedRevision::Absent)
        .unwrap();
    assert!(!receipt.created);
    assert_eq!(receipt.receipt.revision(), 1);
    assert_eq!(receipt.receipt.audit_sequence(), 2);
    execute(
        &client
            .archive(&client.index("answer").unwrap(), "1.0.0")
            .unwrap(),
    );
    reopened.stop().unwrap();
}

struct DelayOnce(AtomicBool);
impl Clock for DelayOnce {
    fn now(&self) -> Result<u64, store::Error> {
        if self.0.swap(false, Ordering::AcqRel) {
            thread::sleep(Duration::from_millis(1100));
        }
        Ok(110)
    }
}

#[test]
fn original_request_deadline_reaches_host_admission_and_does_not_poison_uncommitted_state() {
    let mut fixture = owner_fixture();
    let clock = Arc::new(DelayOnce(AtomicBool::new(false)));
    let budget = Budget::new(MAX_REQUEST_MEMORY);
    let (mut server, client, _) = controlled_service(
        &mut fixture,
        clock.clone(),
        Duration::from_secs(1),
        budget.clone(),
    );
    let baseline = budget.used();
    let head = fixture.temp.0.join("registry/HEAD");
    let initial = std::fs::read(&head).unwrap();
    let staged = fixture
        .temp
        .0
        .join(format!("registry/staging/object-{}", "c".repeat(64)));
    std::fs::write(&staged, b"partial").unwrap();
    clock.0.store(true, Ordering::Release);
    let bearer = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    let id = "a".repeat(32);
    let started = Instant::now();
    let refused = client
        .publish(&bearer, &fixture.archive, &id, ExpectedRevision::Absent)
        .unwrap_err();
    assert_eq!(refused.delivery, Delivery::Unknown);
    assert!(started.elapsed() < Duration::from_secs(5));
    wait_active(&server, 0);
    assert_eq!(std::fs::read(&head).unwrap(), initial);
    assert_eq!(std::fs::read(&staged).unwrap(), b"partial");
    assert_eq!(budget.used(), baseline);
    assert_eq!(
        client.index("answer").err().unwrap().kind,
        ErrorKind::NotFound
    );
    assert!(
        client
            .publish(&bearer, &fixture.archive, &id, ExpectedRevision::Absent)
            .unwrap()
            .created
    );
    assert!(!staged.exists());
    server.stop().unwrap();
    assert_eq!(budget.used(), 0);
}

#[test]
fn incomplete_authenticated_body_holds_exact_lease_refuses_lock_wait_and_stop_joins() {
    let mut fixture = owner_fixture();
    let budget = Budget::new(MAX_REQUEST_MEMORY);
    let (mut server, client, roots) = controlled_service(
        &mut fixture,
        Arc::new(At(110)),
        REQUEST_TIMEOUT,
        budget.clone(),
    );
    let baseline = budget.used();
    let head = fixture.temp.0.join("registry/HEAD");
    let initial = std::fs::read(&head).unwrap();
    let mut peer = stream(server.local_addr(), roots);
    peer.write_all(&publication_header(&fixture, &"a".repeat(32)))
        .unwrap();
    peer.write_all(&fixture.archive.as_bytes()[..1]).unwrap();
    peer.flush().unwrap();
    let end = Instant::now() + Duration::from_secs(3);
    let expected = baseline + CONNECTION_MEMORY + fixture.archive.as_bytes().len();
    while budget.used() != expected && Instant::now() < end {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(budget.used(), expected);
    let started = Instant::now();
    assert_eq!(client.index("answer").err().unwrap().kind, ErrorKind::Busy);
    assert!(started.elapsed() < Duration::from_secs(3));
    let stopped = Instant::now();
    server.stop().unwrap();
    assert!(stopped.elapsed() < Duration::from_secs(3));
    assert_eq!(budget.used(), 0);
    assert_eq!(std::fs::read(&head).unwrap(), initial);
    drop(peer);
    assert!(Registry::open(&fixture.parent, "registry", &fixture.identity).is_ok());
}

#[test]
fn actual_authenticated_framing_and_route_refusals_preserve_head_and_recognized_staging() {
    let mut fixture = owner_fixture();
    let (mut server, client, roots) = start_mutable(&mut fixture, 110);
    let head = fixture.temp.0.join("registry/HEAD");
    let before = std::fs::read(&head).unwrap();
    let staged = fixture
        .temp
        .0
        .join(format!("registry/staging/object-{}", "c".repeat(64)));
    std::fs::write(&staged, b"partial").unwrap();
    let valid = String::from_utf8(publication_header(&fixture, &"a".repeat(32))).unwrap();
    let auth = format!("Authorization: Bearer {}\r\n", "11".repeat(32));
    let length = format!("Content-Length: {}\r\n", fixture.archive.as_bytes().len());
    let cases = [
        (valid.replace(&auth, ""), 401),
        (
            valid.replace(&auth, &(auth.clone() + &auth.to_lowercase())),
            400,
        ),
        (valid.replace(&"11".repeat(32), &"gg".repeat(32)), 401),
        (
            valid.replace(&length, &(length.clone() + &length.to_lowercase())),
            400,
        ),
        (valid.replace(&length, "Content-Length: -1\r\n"), 400),
        (
            valid.replace(
                &length,
                "Content-Length: 999999999999999999999999999999\r\n",
            ),
            413,
        ),
        (
            valid.replace(
                &length,
                &format!(
                    "Content-Length: {}\r\n",
                    crate::registry::archive::MAX_ARCHIVE_BYTES + 1
                ),
            ),
            413,
        ),
        (
            valid.replace(
                "Connection: close\r\n",
                "Connection: close\r\nTransfer-Encoding: chunked\r\n",
            ),
            400,
        ),
        (
            valid.replace(
                "Connection: close\r\n",
                "Connection: close\r\nContent-Encoding: gzip\r\n",
            ),
            400,
        ),
        (
            valid.replace("application/x-tar", "application/octet-stream"),
            400,
        ),
        (valid.replacen("PUT ", "PATCH ", 1), 400),
        (
            valid.replace(
                "If-None-Match: *\r\n",
                "If-None-Match: *\r\nIf-Match: \"1\"\r\n",
            ),
            400,
        ),
        (
            valid.replace("If-None-Match: *\r\n", "If-Match: \"0\"\r\n"),
            400,
        ),
        (
            valid.replace("If-None-Match: *\r\n", "If-Match: \"01\"\r\n"),
            400,
        ),
        (
            valid.replace(
                "If-None-Match: *\r\n",
                "If-Match: \"18446744073709551616\"\r\n",
            ),
            400,
        ),
        (valid.replace("If-None-Match: *\r\n", ""), 400),
        (valid.replace(&"a".repeat(32), "not-an-id"), 400),
        (valid.replace("/answer/1.0.0", "/another/1.0.0"), 409),
        (valid.replace("/answer/1.0.0", "/answer/1.0.1"), 409),
        (
            valid.replace(
                &format!("Host: {}", authority(&fixture.identity)),
                "Host: other.invalid",
            ),
            400,
        ),
    ];
    for (case, (header, expected)) in cases.into_iter().enumerate() {
        let mut bytes = header.into_bytes();
        bytes.extend_from_slice(fixture.archive.as_bytes());
        let (status, body) = exchange(&server, roots.clone(), &bytes);
        assert_eq!(status, expected, "framing case {case}");
        assert!(wire::error_body(&body, status).is_ok());
        assert_eq!(std::fs::read(&head).unwrap(), before);
        assert_eq!(std::fs::read(&staged).unwrap(), b"partial");
    }
    let public = String::from_utf8(request(&fixture.identity, "/v1/packages/answer"))
        .unwrap()
        .replace("\r\n\r\n", &format!("\r\n{auth}\r\n"));
    assert_eq!(exchange(&server, roots.clone(), public.as_bytes()).0, 400);
    let mut pipeline = publication_header(&fixture, &"a".repeat(32));
    pipeline.extend_from_slice(fixture.archive.as_bytes());
    let action = store::YankAction::new(
        "answer",
        "1.0.0",
        &"b".repeat(32),
        1,
        store::YankActionKind::Yank,
        "must not run",
    )
    .unwrap();
    pipeline.extend_from_slice(format!("POST /v1/packages/answer/actions HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{auth}\r\n", authority(&fixture.identity), action.as_bytes().len()).as_bytes());
    pipeline.extend_from_slice(action.as_bytes());
    assert_eq!(exchange(&server, roots, &pipeline).0, 201);
    let index = client.index("answer").unwrap();
    assert_eq!(index.revision(), 1);
    assert!(!index.release("1.0.0").unwrap().yanked());
    execute(&client.archive(&index, "1.0.0").unwrap());
    server.stop().unwrap();
}

#[test]
fn unauthenticated_host_refusal_keeps_head_and_only_verified_recovery_can_remove_orphans() {
    let mut fixture = owner_fixture();
    let (mut server, client, _) = start_mutable(&mut fixture, 110);
    let head = fixture.temp.0.join("registry/HEAD");
    let before = std::fs::read(&head).unwrap();
    let staged = fixture
        .temp
        .0
        .join(format!("registry/staging/object-{}", "c".repeat(64)));
    std::fs::write(&staged, b"partial").unwrap();
    let invalid = Bearer::from_input("ff".repeat(32).as_bytes()).unwrap();
    let refused = client
        .publish(
            &invalid,
            &fixture.archive,
            &"a".repeat(32),
            ExpectedRevision::Absent,
        )
        .unwrap_err();
    assert_eq!(refused.error.kind, ErrorKind::Authentication);
    assert_eq!(refused.delivery, Delivery::Refused);
    assert_eq!(std::fs::read(&head).unwrap(), before);
    // RFC 0048 permits only proof-backed orphan recovery before authorization.
    assert!(!staged.exists());
    assert_eq!(
        std::fs::read_dir(fixture.temp.0.join("registry/objects"))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(
        client.index("answer").err().unwrap().kind,
        ErrorKind::NotFound
    );
    std::fs::write(&staged, b"partial").unwrap();
    let unknown = fixture.temp.0.join("registry/staging/unknown");
    std::fs::write(&unknown, b"operator-owned").unwrap();
    let corrupt = client
        .publish(
            &invalid,
            &fixture.archive,
            &"a".repeat(32),
            ExpectedRevision::Absent,
        )
        .unwrap_err();
    assert_eq!(corrupt.error.kind, ErrorKind::State);
    assert_eq!(corrupt.delivery, Delivery::Unknown);
    assert_eq!(std::fs::read(&head).unwrap(), before);
    assert_eq!(std::fs::read(&staged).unwrap(), b"partial");
    assert_eq!(std::fs::read(&unknown).unwrap(), b"operator-owned");
    server.stop().unwrap();
}

struct ExpireStaged {
    states: std::path::PathBuf,
    expire: AtomicBool,
    observed: AtomicBool,
}
impl Clock for ExpireStaged {
    fn now(&self) -> Result<u64, store::Error> {
        if self.expire.load(Ordering::Acquire)
            && std::fs::read_dir(&self.states).unwrap().count() > 1
        {
            self.observed.store(true, Ordering::Release);
            return Ok(200);
        }
        Ok(110)
    }
}

#[test]
fn actual_https_final_guard_resamples_expiry_after_candidate_staging_before_head_commit() {
    let mut fixture = owner_fixture();
    let clock = Arc::new(ExpireStaged {
        states: fixture.temp.0.join("registry/states"),
        expire: AtomicBool::new(true),
        observed: AtomicBool::new(false),
    });
    let budget = Budget::new(MAX_REQUEST_MEMORY);
    let head = fixture.temp.0.join("registry/HEAD");
    let before = std::fs::read(&head).unwrap();
    let (mut server, client, _) =
        controlled_service(&mut fixture, clock.clone(), REQUEST_TIMEOUT, budget.clone());
    let bearer = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    let id = "a".repeat(32);
    let expired = client
        .publish(&bearer, &fixture.archive, &id, ExpectedRevision::Absent)
        .unwrap_err();
    assert_eq!(expired.error.kind, ErrorKind::Authentication);
    assert_eq!(expired.delivery, Delivery::Refused);
    assert!(clock.observed.load(Ordering::Acquire));
    assert_eq!(std::fs::read(&head).unwrap(), before);
    assert_eq!(
        client.index("answer").err().unwrap().kind,
        ErrorKind::NotFound
    );
    clock.expire.store(false, Ordering::Release);
    let accepted = client
        .publish(&bearer, &fixture.archive, &id, ExpectedRevision::Absent)
        .unwrap();
    assert!(accepted.created);
    assert_eq!(accepted.receipt.audit_sequence(), 2);
    execute(
        &client
            .archive(&client.index("answer").unwrap(), "1.0.0")
            .unwrap(),
    );
    server.stop().unwrap();
    assert_eq!(budget.used(), 0);
}

#[test]
fn scoped_administrator_can_read_owners_but_cannot_gain_owner_or_local_revocation_authority() {
    let mut fixture = owner_fixture_with(|records| {
        records.tokens[1].grants = vec![authority::codec::Grant {
            action: authority::codec::Action::Admin,
            name: "answer".into(),
        }];
    });
    let (mut server, client, roots) = start_mutable(&mut fixture, 110);
    let a = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    let admin = Bearer::from_input("22".repeat(32).as_bytes()).unwrap();
    client
        .publish(
            &a,
            &fixture.archive,
            &"a".repeat(32),
            ExpectedRevision::Absent,
        )
        .unwrap();
    assert_eq!(
        client.owners(&admin, "answer").unwrap().owners(),
        &[PRINCIPAL]
    );
    let action = store::OwnerAction::new(
        "answer",
        SECOND,
        &"b".repeat(32),
        1,
        store::OwnerActionKind::AddOwner,
        "must be refused",
    )
    .unwrap();
    assert_eq!(
        client.change_owner(&admin, &action).unwrap_err().error.kind,
        ErrorKind::Authorization
    );
    let before = std::fs::read(fixture.temp.0.join("registry/HEAD")).unwrap();
    let local_route = format!(
        "POST /v1/admin/credentials/{}/revoke HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: 0\r\nAuthorization: Bearer {}\r\n\r\n",
        "01".repeat(16),
        authority(&fixture.identity),
        "22".repeat(32)
    );
    assert_eq!(exchange(&server, roots, local_route.as_bytes()).0, 400);
    assert_eq!(
        std::fs::read(fixture.temp.0.join("registry/HEAD")).unwrap(),
        before
    );
    assert_eq!(client.owners(&a, "answer").unwrap().owners(), &[PRINCIPAL]);
    server.stop().unwrap();
}

#[test]
fn actual_action_bodies_and_private_owner_headers_reject_inapplicable_or_noncanonical_input() {
    let mut fixture = owner_fixture();
    let (mut server, client, roots) = start_mutable(&mut fixture, 110);
    let bearer = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    client
        .publish(
            &bearer,
            &fixture.archive,
            &"a".repeat(32),
            ExpectedRevision::Absent,
        )
        .unwrap();
    let head = fixture.temp.0.join("registry/HEAD");
    let before = std::fs::read(&head).unwrap();
    let staged = fixture
        .temp
        .0
        .join(format!("registry/staging/object-{}", "c".repeat(64)));
    std::fs::write(&staged, b"partial").unwrap();
    let action = store::YankAction::new(
        "answer",
        "1.0.0",
        &"b".repeat(32),
        1,
        store::YankActionKind::Yank,
        "maintenance",
    )
    .unwrap();
    let valid = std::str::from_utf8(action.as_bytes()).unwrap();
    let auth = format!("Authorization: Bearer {}\r\n", "11".repeat(32));
    let make_header = |length| {
        format!(
            "POST /v1/packages/answer/actions HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {length}\r\n{auth}\r\n",
            authority(&fixture.identity)
        )
    };
    for body in [
        valid.replace("\"schema\": 1", "\"schema\": 2"),
        valid.replace("\"schema\": 1", "\"schema\": true"),
        valid.replace("\"expected_revision\": 1", "\"expected_revision\": 0"),
        valid.replace(
            "\"expected_revision\": 1,",
            "\"expected_revision\": 1,\n  \"expected_revision\": 1,",
        ),
        valid.replace(&"b".repeat(32), &"B".repeat(32)),
        valid.replace("\"yank\"", "\"unknown\""),
        valid.replace("maintenance", "bad\\nreason"),
        valid.replace(
            "\"version\": \"1.0.0\"",
            &format!("\"principal\": \"{SECOND}\""),
        ),
        valid.replace("\"version\"", "\"unexpected\": 0,\n  \"version\""),
        valid.trim_end().into(),
        " ".repeat(2048),
        String::new(),
    ] {
        let mut bytes = make_header(body.len()).into_bytes();
        bytes.extend_from_slice(body.as_bytes());
        assert_eq!(exchange(&server, roots.clone(), &bytes).0, 400);
        assert_eq!(std::fs::read(&head).unwrap(), before);
        assert_eq!(std::fs::read(&staged).unwrap(), b"partial");
    }
    let header = make_header(valid.len());
    for (header, status) in [
        (header.replace(&auth, ""), 401),
        (header.replace(&auth, &(auth.clone() + &auth)), 400),
        (header.replace("application/json", "application/x-tar"), 400),
        (
            header.replace(&auth, &format!("If-Match: \"1\"\r\n{auth}")),
            400,
        ),
        (
            header.replace(
                &auth,
                &format!("Idempotency-Key: {}\r\n{auth}", "b".repeat(32)),
            ),
            400,
        ),
        (header.replace("/actions", "/owners"), 400),
        (make_header(2049), 413),
    ] {
        let mut bytes = header.into_bytes();
        bytes.extend_from_slice(valid.as_bytes());
        assert_eq!(exchange(&server, roots.clone(), &bytes).0, status);
    }
    let owners =
        String::from_utf8(request(&fixture.identity, "/v1/packages/answer/owners")).unwrap();
    assert_eq!(exchange(&server, roots.clone(), owners.as_bytes()).0, 401);
    let conditional = owners.replace("\r\n\r\n", &format!("\r\nIf-Match: \"1\"\r\n{auth}\r\n"));
    assert_eq!(
        exchange(&server, roots.clone(), conditional.as_bytes()).0,
        400
    );
    assert_eq!(std::fs::read(&head).unwrap(), before);
    assert_eq!(std::fs::read(&staged).unwrap(), b"partial");
    assert_eq!(client.yank(&bearer, &action).unwrap().revision(), 2);
    server.stop().unwrap();
}
