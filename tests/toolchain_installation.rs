use ed25519_dalek::{Signer, SigningKey};
use lang::toolchain::{self as t, installation::Installation};
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};
use std::{
    path::Path,
    process::{Command, Output},
};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
struct Fixture {
    dir: PathBuf,
    key: SigningKey,
    root: Vec<u8>,
    now: u64,
}
impl Fixture {
    fn distribution(&self, sdk: &Path, version: &str, sequence: u64) -> PathBuf {
        let source = self.dir.join("distribution");
        fs::create_dir_all(source.join("objects")).unwrap();
        let sdk_bytes = fs::read(sdk.join("sdk.json")).unwrap();
        let manifest: Value = serde_json::from_slice(&sdk_bytes).unwrap();
        let compiler = if cfg!(windows) {
            "bin/lang.exe"
        } else {
            "bin/lang"
        };
        let mut files = Vec::new();
        for input in manifest["files"].as_array().unwrap() {
            let name = input["path"].as_str().unwrap();
            let executable = name == compiler;
            let object = source
                .join("objects")
                .join(input["sha256"].as_str().unwrap());
            if !object.exists() {
                fs::copy(sdk.join(name), &object).unwrap();
            }
            files.push(json!({"path":format!("sdk/{name}"),"bytes":input["bytes"],"sha256":input["sha256"],"executable":executable}));
            if executable {
                files.insert(0, json!({"path":compiler,"bytes":input["bytes"],"sha256":input["sha256"],"executable":true}));
            }
        }
        let hash = t::digest(&sdk_bytes);
        fs::write(source.join("objects").join(&hash), &sdk_bytes).unwrap();
        files.push(
            json!({"path":"sdk/sdk.json","bytes":sdk_bytes.len(),"sha256":hash,"executable":false}),
        );
        let payload = serde_json::to_string(&json!({"schema":1,"repository":"test.lang","channel":"stable","target":manifest["target"],
            "sequence":sequence,"issued":self.now-1,"expires":self.now+3600,"current_version":version,
            "releases":[{"version":version,"compiler_build_id":manifest["build_id"],"files":files}]})).unwrap();
        let mut message = t::SIGNING_DOMAIN.to_vec();
        message.extend_from_slice(payload.as_bytes());
        let envelope = serde_json::to_vec(&json!({"schema":1,"payload":payload,"signatures":[{
            "key_id":t::digest(&self.key.verifying_key().to_bytes()),"signature":hex(&self.key.sign(&message).to_bytes())}]})).unwrap();
        fs::create_dir_all(source.join("channels/stable")).unwrap();
        fs::write(
            source.join(format!(
                "channels/stable/{}.json",
                manifest["target"].as_str().unwrap()
            )),
            envelope,
        )
        .unwrap();
        source
    }
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "lang toolchain installation {}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let mut seed = [0; 32];
        getrandom::fill(&mut seed).unwrap();
        let key = SigningKey::from_bytes(&seed);
        let public = key.verifying_key().to_bytes();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let root = serde_json::to_vec(
            &json!({"schema":1,"repository":"test.lang","expires":now+86400,"threshold":1,
            "keys":[{"id":t::digest(&public),"public_key":hex(&public)}]}),
        )
        .unwrap();
        Self {
            dir,
            key,
            root,
            now,
        }
    }
    fn installed_root(&self) -> PathBuf {
        self.dir.join("installation")
    }
    fn read_state(&self) -> Value {
        serde_json::from_slice(&fs::read(self.installed_root().join("state.json")).unwrap())
            .unwrap()
    }
    fn write_state(&self, value: &Value) {
        fs::write(
            self.installed_root().join("state.json"),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }
    fn signed_receipt(&self, channel: &str, sequence: u64) -> (Value, String) {
        let target = lang_codegen_clif::Triple::host().to_string();
        let compiler = if cfg!(windows) {
            "bin/lang.exe"
        } else {
            "bin/lang"
        };
        let release = json!({"version":"0.1.0","compiler_build_id":"a".repeat(64),"files":[
            {"path":compiler,"bytes":1,"sha256":t::digest(b"x"),"executable":true},
            {"path":format!("sdk/{compiler}"),"bytes":1,"sha256":t::digest(b"x"),"executable":true},
            {"path":"sdk/sdk.json","bytes":2,"sha256":t::digest(b"{}"),"executable":false}]});
        let identity: t::Release = serde_json::from_value(release.clone()).unwrap();
        let payload = serde_json::to_string(&json!({"schema":1,"repository":"test.lang","channel":channel,"target":target,
            "sequence":sequence,"issued":self.now-1,"expires":self.now+3600,"current_version":"0.1.0","releases":[release]})).unwrap();
        let mut message = t::SIGNING_DOMAIN.to_vec();
        message.extend_from_slice(payload.as_bytes());
        let envelope = serde_json::to_vec(&json!({"schema":1,"payload":payload,"signatures":[{
            "key_id":t::digest(&self.key.verifying_key().to_bytes()),"signature":hex(&self.key.sign(&message).to_bytes())}]})).unwrap();
        let digest = t::digest(&envelope);
        fs::write(
            self.installed_root()
                .join(format!("metadata/{digest}.json")),
            envelope,
        )
        .unwrap();
        (
            json!({"schema":1,"metadata_sha256":digest,"admitted_at":self.now}),
            identity.identity().unwrap(),
        )
    }
}

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}
fn install_cli(root: &Path, source: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lang"))
        .args(["toolchain", "install", "--root"])
        .arg(root)
        .arg("--source")
        .arg(source)
        .args(["--channel", "stable", "--message-format", "json"])
        .output()
        .unwrap()
}
fn assert_old_active(fixture: &Fixture, active: &Value, executable: &Path, manifest: &Path) {
    assert_eq!(&fixture.read_state()["active"], active);
    let output = success(
        Command::new(executable)
            .args(["run", "--manifest-path"])
            .arg(manifest)
            .output()
            .unwrap(),
    );
    assert_eq!(output.stdout, b"42\n");
}

#[test]
fn real_signed_install_preserves_active_execution_across_failed_and_interrupted_activation() {
    use lang::toolchain::installation::{Checkpoint, Request};
    // Re-execute this exact test as a real process-interruption worker. It
    // receives only public metadata; the parent's ephemeral key stays in memory.
    if let (Some(root), Some(source)) = (
        std::env::var_os("LANG_INSTALL_TEST_ROOT"),
        std::env::var_os("LANG_INSTALL_TEST_SOURCE"),
    ) {
        let mut installed = Installation::open(Path::new(&root)).unwrap();
        let _ = installed.install_with(
            Request {
                source: Path::new(&source),
                channel: t::Channel::Stable,
                version: None,
            },
            |point| {
                if point == Checkpoint::AfterGenerationPublication {
                    std::process::exit(77);
                }
                false
            },
        );
        panic!("interruption checkpoint was not reached");
    }
    let fixture = Fixture::new();
    let built = success(
        Command::new(env!("CARGO"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args([
                "build",
                "--locked",
                "--offline",
                "--all-features",
                "--bin",
                "lang",
                "--example",
                "native-sdk",
                "--message-format=json",
            ])
            .output()
            .unwrap(),
    );
    let sdk = fixture.dir.join("sdk");
    lang::native::sdk_package::assemble(&built.stdout, &sdk, Default::default()).unwrap();
    let trust = fixture.dir.join("trust.json");
    fs::write(&trust, &fixture.root).unwrap();
    let root = fixture.installed_root();
    let initialized = success(
        Command::new(env!("CARGO_BIN_EXE_lang"))
            .args(["toolchain", "init", "--root"])
            .arg(&root)
            .arg("--trust-root")
            .arg(&trust)
            .args(["--message-format", "json"])
            .output()
            .unwrap(),
    );
    let report: Value = serde_json::from_slice(&initialized.stdout).unwrap();
    assert_eq!(report["kind"], "toolchain_operation");
    assert!(report["active"].is_null());
    let source = fixture.distribution(&sdk, "0.1.0", 7);
    let installed = success(install_cli(&root, &source));
    let report: Value = serde_json::from_slice(&installed.stdout).unwrap();
    assert_eq!(report["active"]["version"], "0.1.0");
    assert_eq!(report["floor_advanced"], true);
    let original = fixture.read_state()["active"].clone();
    let compiler = if cfg!(windows) {
        "bin/lang.exe"
    } else {
        "bin/lang"
    };
    let executable = root
        .join("generations")
        .join(original["generation"].as_str().unwrap())
        .join(compiler);
    let app = fixture.dir.join("app");
    fs::create_dir_all(app.join("src")).unwrap();
    let manifest = app.join("Lang.json");
    fs::write(&manifest, br#"{"schema":1,"package":{"name":"installed_app","version":"0.1.0","edition":"2026","entry":"src/main.lang"}}"#).unwrap();
    fs::write(app.join("src/main.lang"), "fn main()->i64{return 42;}").unwrap();
    assert_old_active(&fixture, &original, &executable, &manifest);
    let sdk_manifest: Value =
        serde_json::from_slice(&fs::read(sdk.join("sdk.json")).unwrap()).unwrap();
    let compiler_hash = sdk_manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == compiler)
        .unwrap()["sha256"]
        .as_str()
        .unwrap();
    fixture.distribution(&sdk, "0.1.1", 8);
    let object = source.join("objects").join(compiler_hash);
    fs::write(&object, b"tampered compiler").unwrap();
    let refused = install_cli(&root, &source);
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stdout.is_empty());
    let error: Value = serde_json::from_slice(&refused.stderr).unwrap();
    assert_eq!(error["kind"], "toolchain_failure");
    assert_eq!(error["diagnostic"]["code"], "TLC0001");
    assert_eq!(error["floor_advanced"], true);
    assert!(!root.join("staging/install").exists());
    assert_old_active(&fixture, &original, &executable, &manifest);
    fs::copy(sdk.join(compiler), &object).unwrap();
    fixture.distribution(&sdk, "0.1.0", 7);
    let rollback = install_cli(&root, &source);
    assert_eq!(rollback.status.code(), Some(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&rollback.stderr).unwrap()["floor_advanced"],
        false
    );
    assert_old_active(&fixture, &original, &executable, &manifest);
    for (i, point) in [
        Checkpoint::BeforeFloorCommit,
        Checkpoint::AfterFloorCommit,
        Checkpoint::AfterPayloadVerification,
        Checkpoint::AfterGenerationPublication,
        Checkpoint::BeforeActivationCommit,
    ]
    .into_iter()
    .enumerate()
    {
        fixture.distribution(&sdk, "0.1.1", 9 + i as u64);
        let mut manager = Installation::open(&root).unwrap();
        let before = fs::read(root.join("state.json")).unwrap();
        let error = manager
            .install_with(
                Request {
                    source: &source,
                    channel: t::Channel::Stable,
                    version: None,
                },
                |observed| observed == point,
            )
            .unwrap_err();
        assert_eq!(error.floor_advanced, point != Checkpoint::BeforeFloorCommit);
        drop(manager);
        if point == Checkpoint::BeforeFloorCommit {
            assert_eq!(fs::read(root.join("state.json")).unwrap(), before);
        }
        assert_old_active(&fixture, &original, &executable, &manifest);
    }
    fixture.distribution(&sdk, "0.1.2", 14);
    let interrupted = Command::new(std::env::current_exe().unwrap()).args(["--exact", "real_signed_install_preserves_active_execution_across_failed_and_interrupted_activation", "--nocapture"])
        .env("LANG_INSTALL_TEST_ROOT", &root).env("LANG_INSTALL_TEST_SOURCE", &source).output().unwrap();
    assert_eq!(
        interrupted.status.code(),
        Some(77),
        "{}",
        String::from_utf8_lossy(&interrupted.stderr)
    );
    assert_old_active(&fixture, &original, &executable, &manifest);
    let retried = success(install_cli(&root, &source));
    let report: Value = serde_json::from_slice(&retried.stdout).unwrap();
    assert_eq!(report["active"]["version"], "0.1.2");
    let latest = root
        .join("generations")
        .join(report["active"]["generation"].as_str().unwrap())
        .join(compiler);
    let output = success(
        Command::new(latest)
            .args(["run", "--manifest-path"])
            .arg(&manifest)
            .output()
            .unwrap(),
    );
    assert_eq!(output.stdout, b"42\n");
    assert!(
        executable.is_file(),
        "activation retains the old generation"
    );
    let active_generation = root
        .join("generations")
        .join(report["active"]["generation"].as_str().unwrap());
    let installed_sdk = active_generation.join("sdk/sdk.json");
    let original_sdk = fs::read(&installed_sdk).unwrap();
    fs::write(&installed_sdk, b"{}").unwrap();
    let changed = install_cli(&root, &source);
    assert_eq!(
        changed.status.code(),
        Some(1),
        "an existing generation name cannot bypass byte verification"
    );
    assert!(changed.stdout.is_empty());
    assert_eq!(
        fixture.read_state()["active"]["generation"],
        report["active"]["generation"]
    );
    fs::write(&installed_sdk, original_sdk).unwrap();
    success(install_cli(&root, &source));
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn isolated_state_requires_exclusive_initialization_and_releases_its_nonblocking_lock() {
    let fixture = Fixture::new();
    let root = fixture.installed_root();
    let first = Installation::init(&root, &fixture.root).unwrap();
    assert_eq!(first.root_sha256(), t::digest(&fixture.root));
    assert_eq!(first.repository(), "test.lang");
    assert_eq!(first.time_floor(), 0);
    assert!(
        Installation::open(&root).is_err(),
        "contending open must not wait"
    );
    let original = fs::read(root.join("state.json")).unwrap();
    assert!(Installation::init(&root, &fixture.root).is_err());
    assert_eq!(fs::read(root.join("state.json")).unwrap(), original);
    drop(first);
    assert!(Installation::open(&root).is_ok());
    assert_eq!(fs::read(root.join("root.json")).unwrap(), fixture.root);
    assert!(!root.join("state.next").exists());
    let invalid = fixture.dir.join("invalid");
    assert!(Installation::init(&invalid, b"{}").is_err());
    assert!(!invalid.exists());
}

#[test]
fn persisted_state_reauthenticates_receipts_and_rejects_corruption_without_resetting_floors() {
    let fixture = Fixture::new();
    let root = fixture.installed_root();
    drop(Installation::init(&root, &fixture.root).unwrap());
    let (receipt, generation) = fixture.signed_receipt("stable", 7);
    let mut value = fixture.read_state();
    value["time_floor"] = json!(fixture.now);
    value["channels"] = json!([{"channel":"stable","receipt":receipt}]);
    value["installed"] =
        json!([{"channel":"stable","version":"0.1.0","generation":generation,"receipt":receipt}]);
    value["active"] = json!({"channel":"stable","version":"0.1.0","generation":generation});
    fixture.write_state(&value);
    assert_eq!(Installation::open(&root).unwrap().time_floor(), fixture.now);
    for case in 0..11 {
        let mut bad = value.clone();
        match case {
            0 => {
                bad.as_object_mut().unwrap().remove("active");
            }
            1 => bad["extra"] = json!(true),
            2 => bad["schema"] = json!(2),
            3 => bad["root_sha256"] = json!("0".repeat(64)),
            4 => bad["time_floor"] = json!(fixture.now - 1),
            5 => bad["channels"] = json!([]),
            6 => bad["channels"][0]["receipt"]["schema"] = json!(2),
            7 => bad["installed"][0]["generation"] = json!("b".repeat(64)),
            8 => bad["active"]["version"] = json!("0.2.0"),
            9 => bad["channels"]
                .as_array_mut()
                .unwrap()
                .push(value["channels"][0].clone()),
            _ => bad["installed"]
                .as_array_mut()
                .unwrap()
                .push(value["installed"][0].clone()),
        }
        fixture.write_state(&bad);
        let before = fs::read(root.join("state.json")).unwrap();
        assert!(Installation::open(&root).is_err(), "corruption case {case}");
        assert_eq!(fs::read(root.join("state.json")).unwrap(), before);
    }
    fixture.write_state(&value);
    let metadata = root.join(format!(
        "metadata/{}.json",
        receipt["metadata_sha256"].as_str().unwrap()
    ));
    let original = fs::read(&metadata).unwrap();
    fs::write(&metadata, b"{}").unwrap();
    assert!(Installation::open(&root).is_err());
    fs::write(&metadata, original).unwrap();
    let (newer, _) = fixture.signed_receipt("stable", 8);
    value["installed"][0]["receipt"] = newer;
    fixture.write_state(&value);
    assert!(
        Installation::open(&root).is_err(),
        "installed receipt cannot exceed persisted channel floor"
    );
    value["channels"][0]["receipt"] = value["installed"][0]["receipt"].clone();
    fixture.write_state(&value);
    assert!(Installation::open(&root).is_ok());
}

#[test]
fn owned_state_refuses_oversized_duplicate_nonregular_and_unknown_entries_without_rewriting_them() {
    let fixture = Fixture::new();
    let root = fixture.installed_root();
    drop(Installation::init(&root, &fixture.root).unwrap());
    let state = root.join("state.json");
    let original = fs::read(&state).unwrap();
    for invalid in [
        vec![b' '; 1024 * 1024 + 1],
        format!(
            "{{\"schema\":1,{}",
            std::str::from_utf8(&original)
                .unwrap()
                .trim_start_matches('{')
        )
        .into_bytes(),
    ] {
        fs::write(&state, &invalid).unwrap();
        assert!(Installation::open(&root).is_err());
        assert_eq!(fs::read(&state).unwrap(), invalid);
    }
    fs::remove_file(&state).unwrap();
    fs::create_dir(&state).unwrap();
    assert!(Installation::open(&root).is_err());
    assert!(state.is_dir());
    fs::remove_dir(&state).unwrap();
    #[cfg(unix)]
    {
        let backup = root.join("state-backup.json");
        fs::write(&backup, &original).unwrap();
        std::os::unix::fs::symlink("state-backup.json", &state).unwrap();
        assert!(Installation::open(&root).is_err());
        assert_eq!(fs::read(&backup).unwrap(), original);
        fs::remove_file(&state).unwrap();
        fs::remove_file(backup).unwrap();
        assert!(
            Command::new("mkfifo")
                .arg(&state)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Installation::open(&root).is_err(),
            "FIFO state must fail without blocking"
        );
        fs::remove_file(&state).unwrap();
    }
    fs::write(&state, &original).unwrap();
    let unknown = root.join("metadata/unknown.json");
    fs::write(&unknown, b"keep").unwrap();
    assert!(Installation::open(&root).is_err());
    assert_eq!(fs::read(&unknown).unwrap(), b"keep");
    fs::remove_file(unknown).unwrap();
    for i in 0..129_u64 {
        fs::write(
            root.join(format!("metadata/{}.json", t::digest(&i.to_le_bytes()))),
            b"{}",
        )
        .unwrap();
    }
    assert!(Installation::open(&root).is_err());
    assert_eq!(fs::read(&state).unwrap(), original);
    fs::remove_file(root.join(format!(
        "metadata/{}.json",
        t::digest(&128_u64.to_le_bytes())
    )))
    .unwrap();
    assert!(
        Installation::open(&root).is_ok(),
        "bounded unsigned leftovers never become installed receipts"
    );
}
