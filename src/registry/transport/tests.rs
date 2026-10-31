use super::*;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

mod binding;

fn tls() -> (Arc<rustls::ServerConfig>, rustls::RootCertStore) {
    tls_names(vec!["localhost".into(), "127.0.0.1".into()])
}

fn tls_names(names: Vec<String>) -> (Arc<rustls::ServerConfig>, rustls::RootCertStore) {
    let certificate = rcgen::generate_simple_self_signed(names).unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate.cert.der().clone()).unwrap();
    let mut server = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![certificate.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der())
                .into(),
        )
        .unwrap();
    server.alpn_protocols = vec![b"http/1.1".to_vec()];
    (Arc::new(server), roots)
}

fn reply(body: &[u8]) -> Vec<u8> {
    let mut bytes = format!("HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

// This peer intentionally emits hostile messages. It is separate from the
// production parser, so a matching parser/writer bug cannot validate itself.
fn peer(bytes: Vec<u8>, trusted: bool, host: &str) -> (Client, thread::JoinHandle<Vec<u8>>) {
    let (tls, roots) = tls();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let identity = ServiceIdentity::new(
        "example",
        &format!("https://{host}:{}", listener.local_addr().unwrap().port()),
    )
    .unwrap();
    let task = thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let conn = rustls::ServerConnection::new(tls).unwrap();
        let mut stream = rustls::StreamOwned::new(conn, socket);
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") && request.len() <= MAX_LINE_BYTES + MAX_HEADER_BYTES
        {
            let mut byte = [0];
            if stream.read_exact(&mut byte).is_err() {
                return request;
            }
            request.push(byte[0]);
        }
        // A reply while the client's write half is still open proves this
        // protocol does not require request EOF to make progress.
        let _ = stream.write_all(&bytes);
        stream.conn.send_close_notify();
        let _ = stream.flush();
        request
    });
    (
        Client::new(
            identity,
            if trusted {
                roots
            } else {
                rustls::RootCertStore::empty()
            },
        ),
        task,
    )
}

fn get(client: &Client) -> Result<Vec<u8>, Error> {
    client.get(
        "/v1/packages/answer",
        "application/json",
        1024,
        None,
        Deadlines::with_limits(Duration::from_secs(2), Duration::from_secs(3)),
    )
}

#[test]
fn real_tls_client_requires_exact_framing_and_verified_origin() {
    let good = reply(b"X");
    let mut excess = good.clone();
    excess.extend_from_slice(b"JUNK");
    let truncated = reply(b"12345")[..good.len()].to_vec();
    let mutations = [
        (
            "Content-Length: 1",
            "Content-Length: 1\r\ncontent-length: 1",
        ),
        ("Content-Length: 1", "Content-Length: 1, 1"),
        ("Content-Length: 1", "Content-Length: +1"),
        (
            "Content-Length: 1",
            "Transfer-Encoding: chunked\r\nContent-Length: 1",
        ),
        (
            "Content-Length: 1",
            "Content-Encoding: identity\r\nContent-Length: 1",
        ),
        ("Content-Length: 1", "Content-Length: 1025"),
        ("Content-Length: 1", "Content-Length: 184467440737095516160"),
        ("Content-Length: 1\r\n", ""),
        ("Connection: close", "Connection: keep-alive"),
        ("Connection: close\r\n", ""),
        ("HTTP/1.1", "HTTP/1.0"),
        ("200 OK", "302 Found"),
        ("application/json", "application/x-tar"),
        ("\r\n", "\n"),
        ("Content-Length: 1", "Content-Length: 1\r\n X: folded"),
    ];
    let mut bad = vec![excess, truncated];
    bad.extend(mutations.into_iter().map(|(from, to)| {
        String::from_utf8(good.clone())
            .unwrap()
            .replace(from, to)
            .into_bytes()
    }));
    for (n, bytes) in bad.into_iter().enumerate() {
        let (client, task) = peer(bytes, true, "127.0.0.1");
        assert!(get(&client).is_err(), "case {n}");
        task.join().unwrap();
    }
    for host in ["127.0.0.1", "localhost"] {
        let (client, task) = peer(good.clone(), true, host);
        assert_eq!(get(&client).unwrap(), b"X");
        let request = String::from_utf8(task.join().unwrap()).unwrap();
        assert!(request.starts_with("GET /v1/packages/answer HTTP/1.1\r\n"));
        assert!(request.contains(&format!("Host: {}\r\n", authority(client.identity()))));
        assert!(!request.to_ascii_lowercase().contains("authorization"));
    }
    let (client, task) = peer(good, false, "127.0.0.1");
    assert_eq!(get(&client).unwrap_err().kind, ErrorKind::Transport);
    task.join().unwrap();
}

#[test]
fn header_limits_are_separate_and_exact() {
    let end = Instant::now() + Duration::from_secs(3);
    let line = format!("HTTP/1.1 200 {}\r\n", "a".repeat(MAX_LINE_BYTES - 15));
    assert_eq!(line.len(), MAX_LINE_BYTES);
    let fields = format!("X: {}\r\n\r\n", "b".repeat(MAX_HEADER_BYTES - 7));
    assert_eq!(fields.len(), MAX_HEADER_BYTES);
    let exact = format!("{line}{fields}");
    assert_eq!(
        wire::head(&mut exact.as_bytes(), end).unwrap().len(),
        MAX_LINE_BYTES + MAX_HEADER_BYTES
    );
    for bad in [
        exact.replacen("200 ", "200 a", 1),
        exact.replacen("X: ", "X: b", 1),
    ] {
        assert_eq!(
            wire::head(&mut bad.as_bytes(), end).unwrap_err().kind,
            ErrorKind::Limit
        );
    }
    let base = String::from_utf8(reply(b"")).unwrap();
    for (fields, pass) in [(29, true), (30, false)] {
        let header = base.replacen(
            "\r\n\r\n",
            &format!("\r\n{}\r\n", "X: a\r\n".repeat(fields)),
            1,
        );
        assert_eq!(
            wire::response(header.as_bytes(), "application/json").is_ok(),
            pass
        );
    }
}

#[test]
fn error_records_are_closed_and_never_echoed() {
    let valid = br#"{"schema":1,"code":"REG0008","message":"unavailable","request_id":null}"#;
    wire::error_body(valid, 404).unwrap();
    for bad in [
        String::from_utf8(valid.to_vec())
            .unwrap()
            .replace("REG0008", "REG0003"),
        String::from_utf8(valid.to_vec())
            .unwrap()
            .replace("\"schema\":1", "\"schema\":1,\"schema\":1"),
        String::from_utf8(valid.to_vec())
            .unwrap()
            .replace("\"schema\":1", "\"schema\":1,\"url\":\"https://elsewhere\""),
        String::from_utf8(valid.to_vec())
            .unwrap()
            .replace(",\"request_id\":null", ""),
    ] {
        assert!(wire::error_body(bad.as_bytes(), 404).is_err());
    }
}

#[test]
fn absolute_deadlines_refuse_drips_and_missing_response_eof() {
    for phase in ["header", "body", "eof", "handshake"] {
        let (tls, roots) = tls();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let identity = ServiceIdentity::new(
            "example",
            &format!(
                "https://127.0.0.1:{}",
                listener.local_addr().unwrap().port()
            ),
        )
        .unwrap();
        let task = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            if phase == "handshake" {
                thread::sleep(Duration::from_millis(600));
                return;
            }
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut stream =
                rustls::StreamOwned::new(rustls::ServerConnection::new(tls).unwrap(), socket);
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            match phase {
                "header" => {
                    for byte in b"HTTP/1.1 200 OK\r\n" {
                        if stream
                            .write_all(&[*byte])
                            .and_then(|_| stream.flush())
                            .is_err()
                        {
                            break;
                        }
                        thread::sleep(Duration::from_millis(70));
                    }
                }
                "body" => {
                    let message = reply(&[b'X'; 30]);
                    stream.write_all(&message[..message.len() - 30]).unwrap();
                    stream.flush().unwrap();
                    for _ in 0..30 {
                        if stream.write_all(b"X").and_then(|_| stream.flush()).is_err() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(70));
                    }
                }
                "eof" => {
                    stream.write_all(&reply(b"X")).unwrap();
                    stream.flush().unwrap();
                    thread::sleep(Duration::from_millis(600));
                }
                _ => unreachable!(),
            }
        });
        let client = Client::new(identity, roots);
        let start = Instant::now();
        let result = client.get(
            "/v1/packages/answer",
            "application/json",
            1024,
            None,
            Deadlines::with_limits(Duration::from_millis(200), Duration::from_millis(300)),
        );
        assert_eq!(result.unwrap_err().kind, ErrorKind::Timeout, "{phase}");
        assert!(start.elapsed() < Duration::from_secs(2), "{phase}");
        task.join().unwrap();
    }
}

#[test]
fn trusted_certificate_for_another_host_is_refused() {
    let (tls, roots) = tls_names(vec!["elsewhere.invalid".into()]);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let identity = ServiceIdentity::new(
        "example",
        &format!(
            "https://127.0.0.1:{}",
            listener.local_addr().unwrap().port()
        ),
    )
    .unwrap();
    let task = thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut stream =
            rustls::StreamOwned::new(rustls::ServerConnection::new(tls).unwrap(), socket);
        let mut byte = [0];
        assert!(stream.read(&mut byte).is_err());
    });
    assert_eq!(
        get(&Client::new(identity, roots)).unwrap_err().kind,
        ErrorKind::Transport
    );
    task.join().unwrap();
}

#[test]
fn low_level_deadline_does_not_restart_after_progress() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let socket = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut other, _) = listener.accept().unwrap();
    other.write_all(b"X").unwrap();
    let mut socket = socket::Socket {
        stream: socket,
        end: Instant::now(),
    };
    assert_eq!(
        socket.read(&mut [0]).unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(
        socket.write(b"Y").unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
}
