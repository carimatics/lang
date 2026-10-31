use super::*;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const RECEIPT: &[u8] = b"{\n  \"schema\": 1,\n  \"registry\": \"example\",\n  \"request_id\": \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\n  \"revision\": 1,\n  \"audit_sequence\": 2,\n  \"outcome\": \"published\"\n}\n";

fn response(status: u16, body: &[u8]) -> Vec<u8> {
    let mut bytes = format!("HTTP/1.1 {status} fixture\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend_from_slice(body);
    bytes
}

fn peer(reply: Vec<u8>, certificate_name: &str) -> (Client, JoinHandle<bool>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let identity = ServiceIdentity::new(
        "example",
        &format!(
            "https://127.0.0.1:{}",
            listener.local_addr().unwrap().port()
        ),
    )
    .unwrap();
    let certificate = rcgen::generate_simple_self_signed(vec![certificate_name.into()]).unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(certificate.cert.der().clone()).unwrap();
    let mut config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![certificate.cert.der().clone()],
            rustls::pki_types::PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der())
                .into(),
        )
        .unwrap();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    let client = Client::new(identity.clone(), roots);
    let task = thread::spawn(move || {
        let (socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut conn = rustls::ServerConnection::new(Arc::new(config)).unwrap();
        conn.set_buffer_limit(Some(64 * 1024));
        let mut stream: rustls::StreamOwned<_, TcpStream> = rustls::StreamOwned::new(conn, socket);
        let end = Instant::now() + Duration::from_secs(5);
        let Ok(head) = wire::head(&mut stream, end) else {
            return false;
        };
        let length = match wire::request(&head, authority(&identity)).unwrap() {
            wire::Request::Publish { length, .. } | wire::Request::Action { length, .. } => length,
            wire::Request::Owners { .. } => 0,
            _ => panic!("authenticated request expected"),
        };
        wire::body(
            &mut stream,
            length,
            crate::registry::archive::MAX_ARCHIVE_BYTES,
            end,
        )
        .unwrap();
        let _ = stream.write_all(&reply);
        stream.conn.send_close_notify();
        let _ = stream.flush();
        true
    });
    (client, task)
}

fn publish(client: &Client) -> Result<Publication, MutationError> {
    let bearer = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    let archive =
        crate::registry::transport::tests::binding::archive(b"fn main() -> i64 { return 42; }\n");
    client.publish(&bearer, &archive, ID, store::ExpectedRevision::Absent)
}

#[test]
fn actual_tls_mutation_receipts_bind_request_origin_revision_outcome_and_canonical_bytes() {
    for status in [200, 201] {
        let (client, task) = peer(response(status, RECEIPT), "127.0.0.1");
        let receipt = publish(&client).unwrap();
        assert_eq!(receipt.created, status == 201);
        assert_eq!(receipt.receipt.revision(), 1);
        assert_eq!(receipt.receipt.audit_sequence(), 2);
        assert!(task.join().unwrap());
    }
    let valid = std::str::from_utf8(RECEIPT).unwrap();
    let bodies = [
        valid.replace("\"schema\": 1", "\"schema\": 2"),
        valid.replace("\"schema\": 1", "\"schema\": true"),
        valid.replace("\"schema\": 1,", "\"schema\": 1,\n  \"schema\": 1,"),
        valid.replace("example", "another"),
        valid.replace(ID, &"b".repeat(32)),
        valid.replace("\"revision\": 1", "\"revision\": 2"),
        valid.replace("\"audit_sequence\": 2", "\"audit_sequence\": 1"),
        valid.replace("published", "yanked"),
        valid.replace("published", "quarantined"),
        valid.replace("\"outcome\"", "\"unexpected\": 0,\n  \"outcome\""),
        valid.trim_end().to_owned(),
        serde_json::to_string(&serde_json::from_str::<serde_json::Value>(valid).unwrap()).unwrap(),
        "{\"schema\":1,\"audit_sha256\":\"not-a-wire-receipt\",\"result\":{}}".into(),
    ];
    for (case, body) in bodies.iter().enumerate() {
        let (client, task) = peer(response(201, body.as_bytes()), "127.0.0.1");
        assert_eq!(
            publish(&client).unwrap_err().delivery,
            Delivery::Unknown,
            "receipt case {case}"
        );
        assert!(task.join().unwrap());
    }
}

#[test]
fn actual_tls_lost_truncated_excess_and_misassociated_refusals_never_claim_not_sent() {
    let complete = response(201, RECEIPT);
    let mut truncated = complete.clone();
    truncated.pop();
    let mut excess = complete;
    excess.push(b'X');
    for reply in [Vec::new(), truncated, excess] {
        let (client, task) = peer(reply, "127.0.0.1");
        assert_eq!(publish(&client).unwrap_err().delivery, Delivery::Unknown);
        assert!(task.join().unwrap());
    }
    for (status, code, id, delivery, kind) in [
        (
            401,
            "REG0003",
            "null".into(),
            Delivery::Refused,
            ErrorKind::Authentication,
        ),
        (
            401,
            "REG0003",
            format!("\"{}\"", "b".repeat(32)),
            Delivery::Unknown,
            ErrorKind::Malformed,
        ),
        (
            409,
            "REG0007",
            "null".into(),
            Delivery::Unknown,
            ErrorKind::State,
        ),
        (
            409,
            "REG0005",
            format!("\"{ID}\""),
            Delivery::Refused,
            ErrorKind::Conflict,
        ),
        (
            500,
            "REG0008",
            "null".into(),
            Delivery::Unknown,
            ErrorKind::Transport,
        ),
    ] {
        let body = format!(
            "{{\"schema\":1,\"code\":\"{code}\",\"message\":\"foreign-secret-never-echo\",\"request_id\":{id}}}"
        );
        let (client, task) = peer(response(status, body.as_bytes()), "127.0.0.1");
        let error = publish(&client).unwrap_err();
        assert_eq!(error.delivery, delivery);
        assert_eq!(error.error.kind, kind);
        assert!(!error.to_string().contains("foreign-secret-never-echo"));
        assert!(task.join().unwrap());
    }
    let (client, task) = peer(response(201, RECEIPT), "different.invalid");
    assert_eq!(publish(&client).unwrap_err().delivery, Delivery::NotSent);
    assert!(!task.join().unwrap());
}

#[test]
fn actual_private_owner_responses_are_closed_canonical_and_bound_to_the_requested_package() {
    let valid = b"{\n  \"schema\": 1,\n  \"registry\": \"example\",\n  \"name\": \"answer\",\n  \"revision\": 1,\n  \"owners\": [\n    \"00000000000000000000000000000001\"\n  ]\n}\n";
    let bearer = Bearer::from_input("11".repeat(32).as_bytes()).unwrap();
    let (client, task) = peer(response(200, valid), "127.0.0.1");
    assert_eq!(
        client.owners(&bearer, "answer").unwrap().owners(),
        &["00000000000000000000000000000001"]
    );
    assert!(task.join().unwrap());
    let valid = std::str::from_utf8(valid).unwrap();
    for count in [128, 129] {
        let ids = (1..=count)
            .map(|id| format!("\"{id:032x}\""))
            .collect::<Vec<_>>()
            .join(",\n    ");
        let body = valid.replace("\"00000000000000000000000000000001\"", &ids);
        let (client, task) = peer(response(200, body.as_bytes()), "127.0.0.1");
        let owners = client.owners(&bearer, "answer");
        if count == 128 {
            assert_eq!(owners.unwrap().owners().len(), 128);
        } else {
            assert!(owners.is_err());
        }
        assert!(task.join().unwrap());
    }
    for body in [
        valid.replace("answer", "another"),
        valid.replace("example", "another"),
        valid.replace("\"revision\": 1", "\"revision\": 0"),
        valid.replace("00000000000000000000000000000001", "not-a-principal"),
        valid.replace(
            "\"00000000000000000000000000000001\"",
            "\"00000000000000000000000000000001\",\n    \"00000000000000000000000000000001\"",
        ),
        valid.replace("    \"00000000000000000000000000000001\"\n", ""),
        valid.trim_end().into(),
    ] {
        let (client, task) = peer(response(200, body.as_bytes()), "127.0.0.1");
        assert!(client.owners(&bearer, "answer").is_err());
        assert!(task.join().unwrap());
    }
}
