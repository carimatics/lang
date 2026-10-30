use ed25519_dalek::{Signer, SigningKey};
use lang::toolchain::{self as t, Channel, ErrorKind, Request, Target, TrustedRoot};
use serde_json::{Value, json};

const NOW: u64 = 1_800_000_000;
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
struct Fixture {
    keys: [SigningKey; 2],
    root: Vec<u8>,
    payload: Value,
}
impl Fixture {
    fn new() -> Self {
        // Ephemeral test keys never leave memory or enter source control.
        let keys = std::array::from_fn(|_| {
            let mut seed = [0; 32];
            getrandom::fill(&mut seed).unwrap();
            SigningKey::from_bytes(&seed)
        });
        let public: Vec<_> = keys
            .iter()
            .map(|key| {
                let bytes = key.verifying_key().to_bytes();
                json!({"id": t::digest(&bytes), "public_key": hex(&bytes)})
            })
            .collect();
        let compiler = b"signature fixture compiler bytes";
        let file = |path: &str, bytes: &[u8], executable: bool| {
            json!({
                "path": path, "bytes": bytes.len(), "sha256": t::digest(bytes), "executable": executable
            })
        };
        Self {
            keys,
            root: serde_json::to_vec(&json!({"schema":1,"repository":"test.lang",
                "expires":NOW+1000,"threshold":2,"keys":public}))
            .unwrap(),
            payload: json!({"schema":1,"repository":"test.lang","channel":"stable",
                "target":"aarch64-apple-darwin","sequence":7,"issued":NOW-1,"expires":NOW+100,
                "current_version":"0.1.0","releases":[{"version":"0.1.0","compiler_build_id":"a".repeat(64),
                    "files":[file("bin/lang",compiler,true),file("sdk/bin/lang",compiler,true),file("sdk/sdk.json",b"{}",false)]}]}),
        }
    }
    fn root(&self) -> TrustedRoot {
        TrustedRoot::decode(&self.root, NOW).unwrap()
    }
    fn envelope_text(&self, payload: String) -> Vec<u8> {
        let mut message = t::SIGNING_DOMAIN.to_vec();
        message.extend_from_slice(payload.as_bytes());
        let signatures: Vec<_> = self
            .keys
            .iter()
            .map(|key| {
                json!({
                    "key_id":t::digest(&key.verifying_key().to_bytes()),
                    "signature":hex(&key.sign(&message).to_bytes())
                })
            })
            .collect();
        serde_json::to_vec(&json!({"schema":1,"payload":payload,"signatures":signatures})).unwrap()
    }
    fn envelope(&self, payload: &Value) -> Vec<u8> {
        self.envelope_text(payload.to_string())
    }
    fn request(&self) -> Request<'static> {
        Request {
            channel: Channel::Stable,
            target: Target::Macos,
            now: NOW,
            previous: None,
        }
    }
    fn admit(&self, payload: &Value) -> Result<t::Admitted, t::Error> {
        t::verify(&self.root(), &self.envelope(payload), self.request())
    }
}

#[test]
fn verification_cli_uses_fresh_trust_context_without_payload_reads_or_publication() {
    use std::{
        fs,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Temp(std::path::PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    let temp = Temp(std::env::temp_dir().join(format!(
        "lang toolchain verify {} {}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&temp.0).unwrap();
    let mut f = Fixture::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut root: Value = serde_json::from_slice(&f.root).unwrap();
    root["expires"] = json!(now + 1000);
    f.root = serde_json::to_vec(&root).unwrap();
    f.payload["issued"] = json!(now.saturating_sub(1));
    f.payload["expires"] = json!(now + 100);
    if cfg!(windows) {
        f.payload["target"] = json!("x86_64-pc-windows-msvc");
        f.payload["releases"][0]["files"][0]["path"] = json!("bin/lang.exe");
        f.payload["releases"][0]["files"][1]["path"] = json!("sdk/bin/lang.exe");
    }
    let metadata = f.envelope(&f.payload);
    fs::write(temp.0.join("root.json"), &f.root).unwrap();
    fs::write(temp.0.join("metadata.json"), &metadata).unwrap();
    let command = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_lang"));
        command.current_dir(&temp.0).args([
            "toolchain",
            "verify",
            "--trust-root",
            "root.json",
            "--metadata",
            "metadata.json",
            "--channel",
            "stable",
            "--message-format",
            "json",
        ]);
        command
    };
    let output = command().output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema"], 1);
    assert_eq!(report["kind"], "toolchain_metadata");
    assert_eq!(report["scope"], "fresh_trust_context");
    assert_eq!(report["root_sha256"], t::digest(&f.root));
    assert_eq!(
        report["payload_sha256"],
        t::digest(f.payload.to_string().as_bytes())
    );
    assert_eq!(report["releases"][0]["files"], 3);
    // No declared compiler/SDK payload exists, yet metadata verification succeeds.
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 2);
    assert_eq!(fs::read(temp.0.join("root.json")).unwrap(), f.root);
    assert_eq!(fs::read(temp.0.join("metadata.json")).unwrap(), metadata);
    let mut bad: Value = serde_json::from_slice(&metadata).unwrap();
    bad["payload"] = json!(format!("{} ", f.payload));
    fs::write(
        temp.0.join("metadata.json"),
        serde_json::to_vec(&bad).unwrap(),
    )
    .unwrap();
    let output = command().output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
        "TLC0001"
    );
    let output = command().args(["--channel", "beta"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stderr).unwrap()["code"],
        "TLC0002"
    );
    fs::remove_file(temp.0.join("metadata.json")).unwrap();
    fs::create_dir(temp.0.join("metadata.json")).unwrap();
    let output = command().output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    #[cfg(unix)]
    {
        fs::remove_dir(temp.0.join("metadata.json")).unwrap();
        assert!(
            Command::new("mkfifo")
                .arg(temp.0.join("metadata.json"))
                .status()
                .unwrap()
                .success()
        );
        let output = command().output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn signed_catalogue_binds_exact_payload_root_channel_target_and_prior_admission() {
    let f = Fixture::new();
    let first = f.admit(&f.payload).unwrap();
    assert_eq!(first.admission().sequence(), 7);
    assert_eq!(
        first.admission().payload_digest(),
        t::digest(f.payload.to_string().as_bytes())
    );
    assert_eq!(first.release(None).unwrap().version, "0.1.0");
    assert!(first.release(Some("9.9.9")).is_err());
    let request = || Request {
        previous: Some(first.admission()),
        ..f.request()
    };
    let same = t::verify(&f.root(), &f.envelope(&f.payload), request()).unwrap();
    assert_eq!(same.admission(), first.admission());
    let mut next = f.payload.clone();
    next["sequence"] = json!(8);
    let next = t::verify(&f.root(), &f.envelope(&next), request()).unwrap();
    assert_eq!(next.admission().sequence(), 8);
    let rollback = Request {
        previous: Some(next.admission()),
        ..f.request()
    };
    assert_eq!(
        t::verify(&f.root(), &f.envelope(&f.payload), rollback)
            .err()
            .unwrap()
            .kind,
        ErrorKind::Rollback
    );
    let same_sequence_new_bytes = f.envelope_text(format!("{}\n", f.payload));
    assert_eq!(
        t::verify(&f.root(), &same_sequence_new_bytes, request())
            .err()
            .unwrap()
            .kind,
        ErrorKind::Rollback
    );
    assert_eq!(
        t::verify(
            &f.root(),
            &f.envelope(&f.payload),
            Request {
                now: NOW - 1,
                ..request()
            }
        )
        .err()
        .unwrap()
        .kind,
        ErrorKind::Clock
    );
    for req in [
        Request {
            channel: Channel::Beta,
            ..f.request()
        },
        Request {
            target: Target::Windows,
            ..f.request()
        },
    ] {
        assert_eq!(
            t::verify(&f.root(), &f.envelope(&f.payload), req)
                .err()
                .unwrap()
                .kind,
            ErrorKind::Identity
        );
    }
    let mut root: Value = serde_json::from_slice(&f.root).unwrap();
    root["expires"] = json!(NOW + 999);
    let different = TrustedRoot::decode(&serde_json::to_vec(&root).unwrap(), NOW).unwrap();
    assert_eq!(
        t::verify(&different, &f.envelope(&f.payload), request())
            .err()
            .unwrap()
            .kind,
        ErrorKind::Identity
    );
    let release = first.release(None).unwrap();
    assert_eq!(
        release.identity().unwrap(),
        t::digest(&serde_json::to_vec(release).unwrap())
    );
    let file = &release.files[2];
    t::verify_file(file, b"{}").unwrap();
    for bytes in [b"{ }".as_slice(), b"[]", b""] {
        assert_eq!(
            t::verify_file(file, bytes).unwrap_err().kind,
            ErrorKind::Payload
        );
    }
}

#[test]
fn threshold_signatures_refuse_tampering_duplicates_unknown_signers_and_noncanonical_scalars() {
    let f = Fixture::new();
    let envelope: Value = serde_json::from_slice(&f.envelope(&f.payload)).unwrap();
    for case in 0..7 {
        let mut bad = envelope.clone();
        match case {
            0 => bad["payload"] = json!(format!("{} ", f.payload)),
            1 => {
                bad["signatures"].as_array_mut().unwrap().pop();
            }
            2 => bad["signatures"][1] = bad["signatures"][0].clone(),
            3 => bad["signatures"][0]["key_id"] = json!("0".repeat(64)),
            4 => bad["signatures"][0]["signature"] = json!("0".repeat(128)),
            5 => {
                let s = bad["signatures"][0]["signature"].as_str().unwrap();
                bad["signatures"][0]["signature"] = json!(format!("{}ff", &s[..126]));
            }
            6 => bad["signatures"][0]["extra"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            t::verify(&f.root(), &serde_json::to_vec(&bad).unwrap(), f.request()).is_err(),
            "case {case}"
        );
    }
    // A fully re-signed duplicate field remains malformed after authentication.
    let duplicate = f.payload.to_string().replacen('{', "{\"schema\":1,", 1);
    assert!(t::verify(&f.root(), &f.envelope_text(duplicate), f.request()).is_err());
    let duplicate =
        String::from_utf8(f.envelope(&f.payload))
            .unwrap()
            .replacen('{', "{\"schema\":1,", 1);
    assert!(t::verify(&f.root(), duplicate.as_bytes(), f.request()).is_err());
    assert!(
        t::verify(
            &f.root(),
            &vec![b' '; t::MAX_METADATA_BYTES + 1],
            f.request()
        )
        .is_err()
    );
}

#[test]
fn trust_root_is_bounded_closed_unexpired_and_rejects_weak_duplicate_or_misidentified_keys() {
    let f = Fixture::new();
    let root: Value = serde_json::from_slice(&f.root).unwrap();
    for case in 0..10 {
        let mut bad = root.clone();
        match case {
            0 => bad["schema"] = json!(2),
            1 => bad["repository"] = json!("../repo"),
            2 => bad["threshold"] = json!(0),
            3 => bad["threshold"] = json!(3),
            4 => bad["keys"][1] = bad["keys"][0].clone(),
            5 => bad["keys"][0]["id"] = json!("0".repeat(64)),
            6 => {
                bad["keys"][0]["id"] = json!(t::digest(&[0; 32]));
                bad["keys"][0]["public_key"] = json!("0".repeat(64));
            }
            7 => bad["expires"] = json!(NOW),
            8 => bad["expires"] = json!(t::MAX_TIME + 1),
            9 => bad["extra"] = json!(true),
            _ => unreachable!(),
        }
        assert!(
            TrustedRoot::decode(&serde_json::to_vec(&bad).unwrap(), NOW).is_err(),
            "case {case}"
        );
    }
    let duplicate = String::from_utf8(f.root.clone())
        .unwrap()
        .replacen('{', "{\"schema\":1,", 1);
    assert!(TrustedRoot::decode(duplicate.as_bytes(), NOW).is_err());
    assert!(TrustedRoot::decode(&vec![b' '; t::MAX_ROOT_BYTES + 1], NOW).is_err());
    assert!(TrustedRoot::decode(&f.root, t::MAX_TIME + 1).is_err());
    assert_eq!(
        t::verify(
            &f.root(),
            &f.envelope(&f.payload),
            Request {
                now: NOW + 1000,
                ..f.request()
            }
        )
        .err()
        .unwrap()
        .kind,
        ErrorKind::Expired
    );
}

#[test]
fn signed_metadata_still_refuses_invalid_times_counts_paths_executables_and_sdk_identities() {
    let f = Fixture::new();
    for (field, value) in [
        ("schema", json!(2)),
        ("sequence", json!(0)),
        ("issued", json!(NOW + 1)),
        ("expires", json!(NOW)),
        ("expires", json!(NOW + 90 * 24 * 60 * 60 + 1)),
        ("expires", json!(u64::MAX)),
        ("repository", json!("other")),
        ("current_version", json!("9.9.9")),
        ("extra", json!(0)),
        ("releases", json!([])),
    ] {
        let mut bad = f.payload.clone();
        bad[field] = value;
        assert!(f.admit(&bad).is_err(), "field {field}");
    }
    for path in [
        "/absolute",
        "a/../b",
        "a//b",
        "a\\b",
        "a:b",
        "CON",
        "aux.txt",
        "x/COM1.log",
        "NUL/a",
        "trailing.",
        "a/./b",
        "a/",
        "café",
    ] {
        let mut bad = f.payload.clone();
        bad["releases"][0]["files"][2]["path"] = json!(path);
        assert!(f.admit(&bad).is_err(), "path {path}");
    }
    for case in 0..10 {
        let mut bad = f.payload.clone();
        match case {
            0 => bad["releases"][0]["version"] = json!("01.0.0"),
            1 => bad["releases"][0]["compiler_build_id"] = json!("A".repeat(64)),
            2 => bad["releases"][0]["files"][2]["executable"] = json!(true),
            3 => bad["releases"][0]["files"][0]["executable"] = json!(false),
            4 => bad["releases"][0]["files"][1]["sha256"] = json!("0".repeat(64)),
            5 => bad["releases"][0]["files"][2]["bytes"] = json!(u64::MAX),
            6 => {
                let copy = bad["releases"][0].clone();
                bad["releases"].as_array_mut().unwrap().push(copy);
            }
            7 => {
                let mut copy = bad["releases"][0]["files"][2].clone();
                copy["path"] = json!("SDK/SDK.JSON");
                bad["releases"][0]["files"]
                    .as_array_mut()
                    .unwrap()
                    .push(copy);
            }
            8 => {
                let mut copy = bad["releases"][0]["files"][2].clone();
                copy["path"] = json!("sdk");
                bad["releases"][0]["files"]
                    .as_array_mut()
                    .unwrap()
                    .push(copy);
            }
            9 => {
                bad["releases"][0]["files"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
            }
            _ => unreachable!(),
        }
        assert!(f.admit(&bad).is_err(), "case {case}");
    }
    // Exact total size succeeds; one extra byte refuses without payload allocation.
    let mut exact = f.payload.clone();
    let compiler_bytes = exact["releases"][0]["files"][0]["bytes"].as_u64().unwrap();
    exact["releases"][0]["files"][2]["bytes"] = json!(t::MAX_RELEASE_BYTES - 2 * compiler_bytes);
    f.admit(&exact).unwrap();
    exact["releases"][0]["files"][2]["bytes"] =
        json!(t::MAX_RELEASE_BYTES - 2 * compiler_bytes + 1);
    assert!(f.admit(&exact).is_err());
}
