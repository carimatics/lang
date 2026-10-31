use super::*;
use crate::registry::{
    archive::AdmittedArchive,
    authority::{self, Bearer},
    store::{Clock, ExpectedRevision, Registry},
};
use std::io::Read;

const PRINCIPAL: &str = "00000000000000000000000000000001";
struct At(u64);
impl Clock for At {
    fn now(&self) -> Result<u64, store::Error> {
        Ok(self.0)
    }
}
struct Temp(std::path::PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Fixture {
    parent: cap_std::fs::Dir,
    registry: Option<Registry>,
    listener: Option<TcpListener>,
    identity: ServiceIdentity,
    archive: AdmittedArchive,
    // Close capabilities/registry handles before removing the fixture on Windows.
    temp: Temp,
}
impl Fixture {
    fn new() -> Self {
        Self::with_archive(crate::registry::transport::tests::binding::archive(
            b"fn main() -> i64 { return 42; }\r\n",
        ))
    }
    fn with_archive(archive: AdmittedArchive) -> Self {
        let mut id = [0u8; 16];
        getrandom::fill(&mut id).unwrap();
        let temp = Temp(std::env::temp_dir().join(format!(
            "lang-registry-https-{}",
            authority::codec::hex(&id)
        )));
        std::fs::create_dir(&temp.0).unwrap();
        let parent =
            cap_std::fs::Dir::open_ambient_dir(&temp.0, cap_std::ambient_authority()).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let identity = ServiceIdentity::new(
            "example",
            &format!(
                "https://127.0.0.1:{}",
                listener.local_addr().unwrap().port()
            ),
        )
        .unwrap();
        let bearer = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
        let authority: authority::codec::Records = serde_json::from_value(serde_json::json!({
            "format":"lang-registry-authority", "schema":1, "registry":"example", "origin":identity.origin(), "time_floor":100,
            "principals":[{"id":PRINCIPAL, "active":true}],
            "tokens":[{"id":"01".repeat(16), "principal":PRINCIPAL, "registry":"example", "origin":identity.origin(),
                "authenticator_sha256":bearer.authenticator_sha256(), "issued":100, "expires":200, "revoked":false,
                "grants":[{"action":"claim", "name":"answer"}, {"action":"publish", "name":"answer"}]}],
            "packages":[]
        })).unwrap();
        let mut bytes = serde_json::to_vec_pretty(&authority).unwrap();
        bytes.push(b'\n');
        let mut registry =
            Registry::initialize(&parent, "registry", &identity, &bytes, PRINCIPAL, &At(100))
                .unwrap();
        registry
            .publish(
                &bearer,
                &archive,
                PRINCIPAL,
                ExpectedRevision::Absent,
                &At(110),
            )
            .unwrap();
        Self {
            temp,
            parent,
            registry: Some(registry),
            listener: Some(listener),
            identity,
            archive,
        }
    }
    fn credentials(
        &self,
    ) -> (
        Vec<CertificateDer<'static>>,
        PrivateKeyDer<'static>,
        rustls::RootCertStore,
    ) {
        let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots.add(cert.cert.der().clone()).unwrap();
        (
            vec![cert.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()).into(),
            roots,
        )
    }
    fn start(&mut self) -> (Server, Client, rustls::RootCertStore) {
        let (certificates, key, roots) = self.credentials();
        let server = Server::spawn(
            self.registry.take().unwrap(),
            self.listener.take().unwrap(),
            certificates,
            key,
        )
        .unwrap();
        (
            server,
            Client::new(self.identity.clone(), roots.clone()),
            roots,
        )
    }
    fn limited(
        &mut self,
        adjustment: isize,
        connect: Duration,
        request: Duration,
    ) -> (Server, Client, rustls::RootCertStore) {
        let (certificates, key, roots) = self.credentials();
        let mut tls = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certificates, key)
            .unwrap();
        tls.alpn_protocols = vec![b"http/1.1".to_vec()];
        tls.session_storage = Arc::new(rustls::server::NoServerSessionStorage {});
        tls.send_tls13_tickets = 0;
        let reads = store::CapturedReads::capture(self.registry.take().unwrap()).unwrap();
        let maximum = (reads.memory_bytes()
            + SERVER_MEMORY
            + CONNECTION_MEMORY
            + self.archive.as_bytes().len())
        .checked_add_signed(adjustment)
        .unwrap();
        let server = Server::start(
            reads,
            self.listener.take().unwrap(),
            Arc::new(tls),
            maximum,
            connect,
            request,
        )
        .unwrap();
        (
            server,
            Client::new(self.identity.clone(), roots.clone()),
            roots,
        )
    }
}
fn request(identity: &ServiceIdentity, path: &str) -> Vec<u8> {
    format!(
        "GET {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n",
        authority(identity)
    )
    .into_bytes()
}
fn stream(
    address: SocketAddr,
    roots: rustls::RootCertStore,
) -> rustls::StreamOwned<rustls::ClientConnection, TcpStream> {
    let mut config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let conn = rustls::ClientConnection::new(
        Arc::new(config),
        rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap(),
    )
    .unwrap();
    let socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    rustls::StreamOwned::new(conn, socket)
}
fn exchange(server: &Server, roots: rustls::RootCertStore, bytes: &[u8]) -> (u16, Vec<u8>) {
    let mut stream = stream(server.local_addr(), roots);
    stream.write_all(bytes).unwrap();
    stream.flush().unwrap();
    // No client EOF or shutdown is sent before the response.
    let end = Instant::now() + Duration::from_secs(3);
    let head = wire::head(&mut stream, end).unwrap();
    let response = wire::response(&head, "application/json").unwrap();
    let bytes = wire::body(&mut stream, response.length, store::MAX_INDEX_BYTES, end).unwrap();
    wire::eof(&mut stream, end).unwrap();
    (response.status, bytes)
}
fn active(server: &Server) -> usize {
    server
        .control
        .sockets
        .lock()
        .unwrap()
        .iter()
        .flatten()
        .count()
}
fn wait_active(server: &Server, count: usize) {
    let end = Instant::now() + Duration::from_secs(3);
    while active(server) != count && Instant::now() < end {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(active(server), count);
}

#[test]
fn real_service_serves_committed_bytes_and_rejects_pipeline_and_hostile_headers() {
    let mut fixture = Fixture::new();
    let (mut server, client, roots) = fixture.start();
    let index = client.index("answer").unwrap();
    assert_eq!(
        client.archive(&index, "1.0.0").unwrap().as_bytes(),
        fixture.archive.as_bytes()
    );
    assert_eq!(
        client.index("absent").err().unwrap().kind,
        ErrorKind::NotFound
    );
    let base = String::from_utf8(request(&fixture.identity, "/v1/packages/answer")).unwrap();
    for (from, to) in [
        ("GET ", "POST "),
        ("HTTP/1.1", "HTTP/1.0"),
        ("Content-Length: 0", "Content-Length: 1"),
        (
            "Content-Length: 0",
            "Content-Length: 0\r\nContent-Length: 0",
        ),
        (
            "Content-Length: 0",
            "Content-Length: 0\r\nTransfer-Encoding: chunked",
        ),
        (
            "Content-Length: 0",
            "Content-Length: 0\r\nContent-Encoding: identity",
        ),
        (
            "Content-Length: 0",
            "Content-Length: 0\r\nAuthorization: secret",
        ),
        (
            "Content-Length: 0",
            "Content-Length: 0\r\nIdempotency-Key: secret",
        ),
        ("Content-Length: 0", "Content-Length: 0\r\nIf-Match: 1"),
        (
            "Content-Length: 0",
            "Content-Length: 0\r\nExpect: 100-continue",
        ),
        ("Connection: close", "Connection: keep-alive"),
        ("/v1/packages/answer", "/v1/packages/%61nswer"),
        ("/v1/packages/answer", "/v1/packages/answer?other"),
        ("Host: ", "Host: wrong"),
    ] {
        let (status, body) = exchange(&server, roots.clone(), base.replace(from, to).as_bytes());
        assert_eq!(status, 400, "{from} -> {to}");
        assert!(!String::from_utf8_lossy(&body).contains("secret"));
        assert_eq!(
            wire::error_body(&body, status).unwrap().kind,
            ErrorKind::Malformed
        );
    }
    let (status, _) = exchange(
        &server,
        roots.clone(),
        request(&fixture.identity, "/unknown").as_slice(),
    );
    assert_eq!(status, 404);
    let pipeline = format!("{base}{base}");
    assert_eq!(exchange(&server, roots, pipeline.as_bytes()).0, 200);
    server.stop().unwrap();
    Registry::open(&fixture.parent, "registry", &fixture.identity).unwrap();
}

#[test]
fn eight_handshakes_are_counted_and_stop_joins_all_workers() {
    let mut fixture = Fixture::new();
    let (mut server, _, _) = fixture.start();
    let peers: Vec<_> = (0..MAX_CONNECTIONS)
        .map(|_| TcpStream::connect(server.local_addr()).unwrap())
        .collect();
    wait_active(&server, MAX_CONNECTIONS);
    let mut excess = TcpStream::connect(server.local_addr()).unwrap();
    excess
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    match excess.read(&mut [0]) {
        Ok(0) => {}
        Err(e) => assert!(!matches!(
            e.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        )),
        other => panic!("unexpected excess result {other:?}"),
    }
    // A timeout would mean the ninth peer was queued instead of refused.
    let mut excess = TcpStream::connect(server.local_addr()).unwrap();
    excess
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    match excess.read(&mut [0]) {
        Ok(0) => {}
        Err(e) => assert!(!matches!(
            e.kind(),
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
        )),
        other => panic!("unexpected excess result {other:?}"),
    }
    let start = Instant::now();
    server.stop().unwrap();
    assert!(start.elapsed() < Duration::from_secs(2));
    assert_eq!(active(&server), 0);
    drop(peers);
    Registry::open(&fixture.parent, "registry", &fixture.identity).unwrap();
}

#[test]
fn aggregate_memory_is_charged_before_archive_read_at_the_exact_boundary() {
    for adjustment in [-1, 0, 1] {
        let mut fixture = Fixture::new();
        let (mut server, client, _) = fixture.limited(adjustment, CONNECT_TIMEOUT, REQUEST_TIMEOUT);
        let index = client.index("answer").unwrap();
        wait_active(&server, 0);
        let path = fixture
            .temp
            .0
            .join("registry/objects")
            .join(fixture.archive.sha256());
        if adjustment == -1 {
            // If the archive is read before charging, its corrupted hash would
            // produce State instead of the required Busy admission refusal.
            std::fs::write(&path, b"corrupt").unwrap();
            assert_eq!(
                client.archive(&index, "1.0.0").err().unwrap().kind,
                ErrorKind::Busy
            );
            assert_eq!(std::fs::read(&path).unwrap(), b"corrupt");
            std::fs::write(&path, fixture.archive.as_bytes()).unwrap();
        } else {
            assert_eq!(
                client.archive(&index, "1.0.0").unwrap().as_bytes(),
                fixture.archive.as_bytes()
            );
        }
        server.stop().unwrap();
    }
    let budget = Budget::new(MAX_REQUEST_MEMORY);
    let full = budget.reserve(MAX_REQUEST_MEMORY).unwrap();
    assert_eq!(budget.used(), MAX_REQUEST_MEMORY);
    assert!(budget.reserve(1).is_err());
    drop(full);
    assert_eq!(budget.used(), 0);
    assert!(budget.reserve(usize::MAX).is_err());
}

#[test]
fn captured_reads_refuse_head_and_object_tampering_without_refresh_or_cleanup() {
    let mut fixture = Fixture::new();
    let (mut server, client, _) = fixture.start();
    let index = client.index("answer").unwrap();
    let head = fixture.temp.0.join("registry/HEAD");
    let before = std::fs::read(&head).unwrap();
    std::fs::write(&head, b"corrupt").unwrap();
    assert_eq!(client.index("answer").err().unwrap().kind, ErrorKind::State);
    assert_eq!(std::fs::read(&head).unwrap(), b"corrupt");
    std::fs::write(&head, &before).unwrap();
    let object = fixture
        .temp
        .0
        .join("registry/objects")
        .join(fixture.archive.sha256());
    let mut changed = fixture.archive.as_bytes().to_vec();
    changed[600] ^= 1;
    std::fs::write(&object, &changed).unwrap();
    assert_eq!(
        client.archive(&index, "1.0.0").err().unwrap().kind,
        ErrorKind::State
    );
    assert_eq!(std::fs::read(&object).unwrap(), changed);
    std::fs::write(&object, fixture.archive.as_bytes()).unwrap();
    assert_eq!(
        client.archive(&index, "1.0.0").unwrap().as_bytes(),
        fixture.archive.as_bytes()
    );
    server.stop().unwrap();
}

#[test]
fn server_handshake_and_slow_header_deadlines_release_slots() {
    let mut fixture = Fixture::new();
    let (mut server, _, roots) =
        fixture.limited(0, Duration::from_millis(200), Duration::from_millis(300));
    let peer = TcpStream::connect(server.local_addr()).unwrap();
    wait_active(&server, 1);
    wait_active(&server, 0);
    drop(peer);
    let mut peer = stream(server.local_addr(), roots);
    let start = Instant::now();
    for byte in b"GET /v1/packages/answer HTTP/1.1\r\n" {
        if peer.write_all(&[*byte]).and_then(|_| peer.flush()).is_err() {
            break;
        }
        thread::sleep(Duration::from_millis(70));
    }
    assert!(start.elapsed() < Duration::from_secs(2));
    wait_active(&server, 0);
    server.stop().unwrap();
}

#[test]
fn maximum_source_archive_round_trips_over_the_actual_service() {
    let mut source = vec![b' '; crate::registry::archive::MAX_SOURCE_BYTES];
    let program = b"fn main() -> i64 { return 42; }\r\n";
    source[..program.len()].copy_from_slice(program);
    let names: Vec<_> = (0..16).map(|i| format!("source_{i:02}.lang")).collect();
    let files: Vec<_> = names
        .iter()
        .map(|name| (name.as_str(), source.as_slice()))
        .collect();
    let archive = crate::registry::transport::tests::binding::archive_files(&files);
    assert!(archive.as_bytes().len() > 16 * 1024 * 1024);
    let mut fixture = Fixture::with_archive(archive);
    let (mut server, client, _) = fixture.start();
    let index = client.index("answer").unwrap();
    let received = client.archive(&index, "1.0.0").unwrap();
    assert_eq!(received.as_bytes(), fixture.archive.as_bytes());
    server.stop().unwrap();
}

#[test]
fn actual_request_line_header_bytes_and_field_counts_enforce_exact_limits() {
    let mut fixture = Fixture::new();
    let (mut server, _, roots) = fixture.start();
    let base = String::from_utf8(request(&fixture.identity, "/v1/packages/answer")).unwrap();
    let line_end = base.find("\r\n").unwrap() + 2;
    let padding = MAX_HEADER_BYTES - (base.len() - line_end) - 5;
    for (extra, expected) in [(0, 200), (1, 413)] {
        let header = base.replacen(
            "\r\n\r\n",
            &format!("\r\nX: {}\r\n\r\n", "v".repeat(padding + extra)),
            1,
        );
        assert_eq!(header.len() - line_end, MAX_HEADER_BYTES + extra);
        assert_eq!(
            exchange(&server, roots.clone(), header.as_bytes()).0,
            expected
        );
    }
    for (count, expected) in [(29, 200), (30, 413)] {
        let header = base.replacen(
            "\r\n\r\n",
            &format!("\r\n{}\r\n", "X: v\r\n".repeat(count)),
            1,
        );
        assert_eq!(
            exchange(&server, roots.clone(), header.as_bytes()).0,
            expected
        );
    }
    for (extra, expected) in [(0, 404), (1, 413)] {
        let path = format!(
            "/{}",
            "x".repeat(MAX_LINE_BYTES - b"GET / HTTP/1.1\r\n".len() + extra)
        );
        let bytes = request(&fixture.identity, &path);
        assert_eq!(
            bytes.iter().position(|b| *b == b'\n').unwrap() + 1,
            MAX_LINE_BYTES + extra
        );
        assert_eq!(exchange(&server, roots.clone(), &bytes).0, expected);
    }
    server.stop().unwrap();
}

#[test]
fn captured_service_never_opens_unpublished_hashes_or_linked_objects() {
    let mut fixture = Fixture::new();
    let (mut server, client, roots) = fixture.start();
    let unpublished =
        crate::registry::transport::tests::binding::archive(b"fn main() -> i64 { return 99; }\r\n");
    let directory = fixture.temp.0.join("registry/objects");
    std::fs::write(directory.join(unpublished.sha256()), unpublished.as_bytes()).unwrap();
    let (status, _) = exchange(
        &server,
        roots,
        &request(
            &fixture.identity,
            &format!("/v1/objects/{}", unpublished.sha256()),
        ),
    );
    assert_eq!(status, 404);
    let index = client.index("answer").unwrap();
    let object = directory.join(fixture.archive.sha256());
    let linked = fixture.temp.0.join("linked");
    std::fs::hard_link(&object, &linked).unwrap();
    assert_eq!(
        client.archive(&index, "1.0.0").err().unwrap().kind,
        ErrorKind::State
    );
    assert_eq!(std::fs::read(&linked).unwrap(), fixture.archive.as_bytes());
    std::fs::remove_file(linked).unwrap();
    assert_eq!(
        client.archive(&index, "1.0.0").unwrap().as_bytes(),
        fixture.archive.as_bytes()
    );
    server.stop().unwrap();
}
