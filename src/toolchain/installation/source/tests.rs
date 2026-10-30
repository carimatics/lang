use super::*;
use std::{io::Write, net::TcpListener, sync::Arc, thread, time::Instant};
use ureq::tls::{Certificate, RootCerts, TlsConfig};

struct Server {
    base: String,
    anchor: Certificate<'static>,
    worker: thread::JoinHandle<Vec<String>>,
}
fn serve(responses: Vec<Vec<u8>>, delay: bool) -> Server {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let anchor = Certificate::from_der(cert.der()).to_owned();
    let config = Arc::new(
        rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.der().clone()],
            rustls::pki_types::PrivateKeyDer::Pkcs8(signing_key.serialize_der().into()),
        )
        .unwrap(),
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!(
        "https://localhost:{}",
        listener.local_addr().unwrap().port()
    );
    let worker = thread::spawn(move || {
        let mut paths = Vec::new();
        for response in responses {
            let deadline = Instant::now() + Duration::from_secs(3);
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("TLS fixture accept: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut tls = rustls::StreamOwned::new(
                rustls::ServerConnection::new(config.clone()).unwrap(),
                stream,
            );
            let mut request = Vec::new();
            let mut byte = [0];
            while request.len() < 8192 && !request.ends_with(b"\r\n\r\n") {
                if tls.read_exact(&mut byte).is_err() {
                    return paths;
                }
                request.push(byte[0]);
            }
            paths.push(
                String::from_utf8(request)
                    .unwrap()
                    .lines()
                    .next()
                    .unwrap()
                    .to_owned(),
            );
            if delay {
                let _ = tls.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\na",
                );
                let _ = tls.flush();
                thread::sleep(Duration::from_millis(300));
                let _ = tls.write_all(b"bc");
            } else {
                let _ = tls.write_all(&response);
            }
            let _ = tls.flush();
        }
        paths
    });
    Server {
        base,
        anchor,
        worker,
    }
}
fn response(body: &[u8]) -> Vec<u8> {
    let mut bytes = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    bytes.extend_from_slice(body);
    bytes
}
fn trusted(server: &Server, timeout: Duration) -> Source {
    let agent = configured_agent(
        TlsConfig::builder()
            .root_certs(RootCerts::new_with_certs(std::slice::from_ref(
                &server.anchor,
            )))
            .build(),
        timeout,
    );
    Source::with_agent(&server.base, agent).unwrap()
}
#[test]
fn https_enforces_tls_status_encoding_lengths_headers_and_deadlines() {
    for base in [
        "http://localhost",
        "https://user@localhost",
        "https://localhost/?q",
        "https://localhost/#f",
        "https:///",
        "https://localhost\\evil",
    ] {
        assert!(Source::https(base).is_err(), "{base}");
    }
    assert!(Source::https(&format!("https://localhost/{}", "x".repeat(2048))).is_err());
    let server = serve(vec![response(b"abc")], false);
    let mut bytes = Vec::new();
    trusted(&server, Duration::from_secs(2))
        .reader("objects/test", Some(3))
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes, b"abc");
    assert_eq!(
        server.worker.join().unwrap(),
        ["GET /objects/test HTTP/1.1"]
    );
    let server = serve(vec![response(b"abc")], false);
    assert!(
        Source::https(&server.base)
            .unwrap()
            .reader("objects/test", Some(3))
            .is_err()
    );
    server.worker.join().unwrap();
    let server = serve(vec![response(b"abc")], false);
    let mut source = trusted(&server, Duration::from_secs(2));
    if let Transport::Https { base, .. } = &mut source.inner {
        *base = base.replace("localhost", "127.0.0.1");
    }
    assert!(source.reader("objects/test", Some(3)).is_err());
    server.worker.join().unwrap();
    for raw in [
        b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/no\r\nContent-Length: 0\r\n\r\n"
            .to_vec(),
        b"HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 3\r\n\r\nabc".to_vec(),
        response(b"abcd"),
        format!(
            "HTTP/1.1 200 OK\r\nX-Long: {}\r\nContent-Length: 3\r\n\r\nabc",
            "x".repeat(65536)
        )
        .into_bytes(),
    ] {
        let server = serve(vec![raw], false);
        assert!(
            trusted(&server, Duration::from_secs(2))
                .reader("objects/test", Some(3))
                .is_err()
        );
        server.worker.join().unwrap();
    }
    let server = serve(
        vec![b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nabc".to_vec()],
        false,
    );
    assert!(
        trusted(&server, Duration::from_secs(2))
            .reader("objects/test", Some(5))
            .unwrap()
            .read_to_end(&mut Vec::new())
            .is_err()
    );
    server.worker.join().unwrap();
    let server = serve(vec![response(b"abc")], true);
    let start = Instant::now();
    let result = trusted(&server, Duration::from_millis(100)).reader("objects/test", Some(3));
    if let Ok(mut reader) = result {
        assert!(reader.read_to_end(&mut Vec::new()).is_err());
    }
    assert!(start.elapsed() < Duration::from_secs(2));
    server.worker.join().unwrap();
    let server = serve(vec![response(&vec![b'x'; MAX_METADATA_BYTES + 1])], false);
    assert!(
        trusted(&server, Duration::from_secs(2))
            .metadata(Channel::Stable, Target::Macos)
            .is_err()
    );
    server.worker.join().unwrap();
}

#[test]
fn https_authentication_precedes_payload_requests_and_commits_the_floor_before_transfer() {
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;
    let mut seed = [0; 32];
    getrandom::fill(&mut seed).unwrap();
    let key = SigningKey::from_bytes(&seed);
    let public = key.verifying_key().to_bytes();
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let current = now().unwrap();
    let target = host().unwrap();
    let compiler = if target == Target::Windows {
        "bin/lang.exe"
    } else {
        "bin/lang"
    };
    let root = serde_json::to_vec(&json!({"schema":1,"repository":"tls.test","expires":current+3600,"threshold":1,"keys":[{"id":digest(&public),"public_key":hex(&public)}]})).unwrap();
    let path = std::env::temp_dir().join(format!(
        "lang-tls-{}-{}",
        std::process::id(),
        digest(&public)
    ));
    let mut installation = Installation::init(&path, &root).unwrap();
    let payload = serde_json::to_string(&json!({"schema":1,"repository":"tls.test","channel":"stable","target":target,"sequence":7,"issued":current-1,"expires":current+1800,"current_version":"0.1.0","releases":[{"version":"0.1.0","compiler_build_id":"a".repeat(64),"files":[
        {"path":compiler,"bytes":3,"sha256":digest(b"abc"),"executable":true},
        {"path":format!("sdk/{compiler}"),"bytes":3,"sha256":digest(b"abc"),"executable":true},
        {"path":"sdk/sdk.json","bytes":2,"sha256":digest(b"{}"),"executable":false}]}]})).unwrap();
    let mut message = SIGNING_DOMAIN.to_vec();
    message.extend_from_slice(payload.as_bytes());
    let envelope = json!({"schema":1,"payload":payload,"signatures":[{"key_id":digest(&public),"signature":hex(&key.sign(&message).to_bytes())}]});
    let mut tampered = envelope.clone();
    tampered["payload"] = json!(format!("{payload} "));
    let server = serve(
        vec![response(&serde_json::to_vec(&tampered).unwrap())],
        false,
    );
    let error = installation
        .install_source(
            &trusted(&server, Duration::from_secs(2)),
            Channel::Stable,
            None,
        )
        .unwrap_err();
    assert!(!error.floor_advanced);
    assert_eq!(installation.time_floor(), 0);
    assert_eq!(server.worker.join().unwrap().len(), 1);
    let server = serve(
        vec![
            response(&serde_json::to_vec(&envelope).unwrap()),
            response(b"bad"),
        ],
        false,
    );
    let error = installation
        .install_source(
            &trusted(&server, Duration::from_secs(2)),
            Channel::Stable,
            None,
        )
        .unwrap_err();
    assert!(error.floor_advanced);
    assert!(installation.time_floor() >= current);
    assert!(installation.list().unwrap().active.is_none());
    let paths = server.worker.join().unwrap();
    assert_eq!(paths.len(), 2);
    assert_eq!(
        paths[1],
        format!("GET /objects/{} HTTP/1.1", digest(b"abc"))
    );
    drop(installation);
    std::fs::remove_dir_all(path).unwrap();
}
