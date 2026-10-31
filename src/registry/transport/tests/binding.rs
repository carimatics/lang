use super::*;
use crate::registry::archive::{AdmittedArchive, Descriptor};
use serde::Serialize;
use sha2::{Digest, Sha256};

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn canonical(value: &impl Serialize) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(value).unwrap();
    bytes.push(b'\n');
    bytes
}
pub(in crate::registry::transport) fn archive(source: &[u8]) -> AdmittedArchive {
    archive_files(&[("main.lang", source)])
}

pub(in crate::registry::transport) fn archive_files(files: &[(&str, &[u8])]) -> AdmittedArchive {
    let metadata: Vec<_> = files
        .iter()
        .map(|(path, source)| {
            serde_json::json!({
                "path":path, "bytes":source.len(), "sha256":hash(source)
            })
        })
        .collect();
    let descriptor: Descriptor = serde_json::from_value(serde_json::json!({
        "format":"lang-registry-source", "schema":1,
        "package":{"name":"answer", "version":"1.0.0", "edition":"2026", "entry":files[0].0},
        "tests":[], "dependencies":[], "files":metadata
    }))
    .unwrap();
    let descriptor = canonical(&descriptor);
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in
        std::iter::once((".lang-package.json", descriptor.as_slice())).chain(files.iter().copied())
    {
        let mut header = tar::Header::new_ustar();
        header.set_path(path).unwrap();
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_size(bytes.len() as u64);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_device_major(0).unwrap();
        header.set_device_minor(0).unwrap();
        header.set_cksum();
        let sum = header.cksum().unwrap();
        header.as_mut_bytes()[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        builder.append(&header, bytes).unwrap();
    }
    AdmittedArchive::admit(&builder.into_inner().unwrap()).unwrap()
}

#[derive(Serialize)]
struct Release<'a> {
    version: &'a str,
    archive_sha256: &'a str,
    archive_bytes: usize,
    descriptor_sha256: &'a str,
    edition: &'a str,
    dependencies: &'a [crate::registry::archive::Dependency],
    yanked: bool,
    quarantined: bool,
}
#[derive(Serialize)]
struct Index<'a> {
    format: &'a str,
    schema: u32,
    registry: &'a str,
    name: &'a str,
    revision: u64,
    releases: [Release<'a>; 1],
}
fn index(archive: &AdmittedArchive) -> Vec<u8> {
    canonical(&Index {
        format: "lang-registry-index",
        schema: 1,
        registry: "example",
        name: "answer",
        revision: 1,
        releases: [Release {
            version: "1.0.0",
            archive_sha256: archive.sha256(),
            archive_bytes: archive.as_bytes().len(),
            descriptor_sha256: archive.descriptor_sha256(),
            edition: "2026",
            dependencies: &[],
            yanked: false,
            quarantined: false,
        }],
    })
}

#[test]
fn actual_index_and_archive_reads_bind_origin_length_hash_and_all_metadata() {
    let expected = archive(b"fn main() -> i64 { return 42; }\r\n");
    let other = archive(b"fn main() -> i64 { return 43; }\r\n");
    for case in [
        "exact",
        "substitution",
        "length",
        "descriptor",
        "registry",
        "name",
        "version",
        "yanked",
        "quarantine",
    ] {
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
        let mut metadata = String::from_utf8(index(&expected)).unwrap();
        match case {
            "descriptor" => {
                metadata = metadata.replace(expected.descriptor_sha256(), &"01".repeat(32))
            }
            "registry" => metadata = metadata.replace("\"example\"", "\"other\""),
            "name" => metadata = metadata.replace("\"answer\"", "\"other\""),
            "version" => metadata = metadata.replace("1.0.0", "2.0.0"),
            "yanked" => metadata = metadata.replace("\"yanked\": false", "\"yanked\": true"),
            "quarantine" => {
                metadata = metadata.replace("\"quarantined\": false", "\"quarantined\": true")
            }
            _ => {}
        }
        let payload = if case == "substitution" {
            other.as_bytes()
        } else {
            expected.as_bytes()
        };
        let payload = if case == "length" {
            payload[..payload.len() - 1].to_vec()
        } else {
            payload.to_vec()
        };
        let index_only = matches!(case, "registry" | "name" | "quarantine");
        let task = thread::spawn(move || {
            let mut requests = Vec::new();
            let mut responses = vec![("application/json", metadata.into_bytes())];
            if !index_only {
                responses.push(("application/x-tar", payload));
            }
            for (content_type, body) in responses {
                let (socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                socket
                    .set_write_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut stream = rustls::StreamOwned::new(
                    rustls::ServerConnection::new(tls.clone()).unwrap(),
                    socket,
                );
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                requests.push(request);
                let header = format!(
                    "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n\r\n",
                    body.len()
                );
                let _ = stream
                    .write_all(header.as_bytes())
                    .and_then(|_| stream.write_all(&body));
                stream.conn.send_close_notify();
                let _ = stream.flush();
            }
            requests
        });
        let client = Client::new(identity, roots.clone());
        let admitted = client.index("answer");
        if matches!(case, "registry" | "name") {
            assert_eq!(admitted.err().unwrap().kind, ErrorKind::Integrity);
        } else {
            let admitted = admitted.unwrap();
            let wrong = Client::new(
                ServiceIdentity::new("example", "https://127.0.0.1:1").unwrap(),
                roots,
            );
            assert_eq!(
                wrong.archive(&admitted, "1.0.0").err().unwrap().kind,
                ErrorKind::Integrity
            );
            let received =
                client.archive(&admitted, if case == "version" { "2.0.0" } else { "1.0.0" });
            if matches!(case, "exact" | "yanked") {
                assert_eq!(received.unwrap().as_bytes(), expected.as_bytes());
            } else {
                assert_eq!(
                    received.err().unwrap().kind,
                    if case == "quarantine" {
                        ErrorKind::NotFound
                    } else {
                        ErrorKind::Integrity
                    },
                    "{case}"
                );
            }
        }
        let requests = task.join().unwrap();
        assert_eq!(requests.len(), if index_only { 1 } else { 2 });
        if requests.len() == 2 {
            assert!(String::from_utf8_lossy(&requests[1]).starts_with(&format!(
                "GET /v1/objects/{} HTTP/1.1\r\n",
                expected.sha256()
            )));
        }
        assert!(requests.iter().all(|bytes| {
            !String::from_utf8_lossy(bytes)
                .to_ascii_lowercase()
                .contains("authorization")
        }));
    }
}
