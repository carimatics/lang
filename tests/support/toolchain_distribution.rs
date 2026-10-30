use super::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use t::{
    Channel, distribution as producer,
    installation::{Failure, Source},
};

#[test]
fn signing_input_is_closed_bounded_and_never_echoes_secret_material() {
    let fixture = Fixture::new();
    let seed = hex(&fixture.key.to_bytes());
    let valid = json!({"schema":1,"signing_keys":[seed]});
    assert_eq!(
        producer::signing_keys(&serde_json::to_vec(&valid).unwrap()).unwrap()[0].verifying_key(),
        fixture.key.verifying_key()
    );
    for value in [
        json!({"schema":2,"signing_keys":[seed]}),
        json!({"schema":1,"signing_keys":[]}),
        json!({"schema":1,"signing_keys":vec![seed.clone();9]}),
        json!({"schema":1,"signing_keys":[seed,seed]}),
        json!({"schema":1,"signing_keys":[seed],"private_extra":seed}),
        json!({"schema":1,"signing_keys":["00"]}),
    ] {
        let error = producer::signing_keys(&serde_json::to_vec(&value).unwrap())
            .err()
            .unwrap();
        assert!(!error.to_string().contains(&seed));
        assert!(!error.floor_advanced);
    }
    assert!(producer::signing_keys(&vec![b' '; producer::MAX_SIGNING_INPUT + 1]).is_err());
    let duplicate = format!("{{\"schema\":1,\"schema\":1,\"signing_keys\":[\"{seed}\"]}}");
    assert!(producer::signing_keys(duplicate.as_bytes()).is_err());
}
fn produce(
    fixture: &Fixture,
    sdk: &Path,
    output: &Path,
    channel: Channel,
    version: &str,
    sequence: u64,
    keys: &[SigningKey],
) -> Result<producer::Report, t::installation::Error> {
    producer::produce(producer::Request {
        sdk,
        output,
        trust_root: &fixture.root,
        channel,
        version,
        sequence,
        issued: fixture.now - 1,
        expires: fixture.now + 3600,
        signing_keys: keys,
    })
}
fn install(root: &Path, source: &Path, channel: Channel) -> t::installation::Report {
    Installation::open(root)
        .unwrap()
        .install_source(&Source::local(source).unwrap(), channel, None)
        .unwrap()
}
fn copy_sdk(source: &Path, destination: &Path) {
    fs::create_dir(destination).unwrap();
    let manifest: Value =
        serde_json::from_slice(&fs::read(source.join("sdk.json")).unwrap()).unwrap();
    for file in manifest["files"].as_array().unwrap() {
        let name = file["path"].as_str().unwrap();
        let target = destination.join(name);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::copy(source.join(name), target).unwrap();
    }
    fs::copy(source.join("sdk.json"), destination.join("sdk.json")).unwrap();
}
fn published_shape(output: &Path) {
    let mut names: Vec<_> = fs::read_dir(output)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    names.sort();
    assert_eq!(names, ["channels", "objects"].map(std::ffi::OsString::from));
    for entry in fs::read_dir(output.join("objects")).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        let name = entry.file_name().into_string().unwrap();
        assert_eq!(name.len(), 64);
        assert!(
            name.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }
}
pub(super) fn journey(sdk: &Path, artifacts: &[u8]) {
    let mut fixture = Fixture::new();
    let mut seed = [0; 32];
    getrandom::fill(&mut seed).unwrap();
    let second = SigningKey::from_bytes(&seed);
    let keys = [fixture.key.clone(), second];
    let mut root: Value = serde_json::from_slice(&fixture.root).unwrap();
    root["threshold"] = json!(2);
    let public = keys[1].verifying_key().to_bytes();
    root["keys"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":t::digest(&public),"public_key":hex(&public)}));
    fixture.root = serde_json::to_vec(&root).unwrap();
    let trusted = fixture.dir.join("trusted-root.json");
    fs::write(&trusted, &fixture.root).unwrap();
    let stable = fixture.dir.join("stable-distribution");
    assert!(
        produce(
            &fixture,
            sdk,
            &stable,
            Channel::Stable,
            "0.2.0",
            7,
            &keys[..1]
        )
        .is_err()
    );
    assert!(!stable.exists(), "unmet threshold never publishes");
    let example: PathBuf = std::str::from_utf8(artifacts)
        .unwrap()
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v["reason"] == "compiler-artifact" && v["target"]["name"] == "toolchain-release")
        .unwrap()["executable"]
        .as_str()
        .unwrap()
        .into();
    let signing = serde_json::to_vec(&json!({"schema":1,"signing_keys":keys.iter().map(|k| hex(&k.to_bytes())).collect::<Vec<_>>()})).unwrap();
    let mut child = Command::new(example)
        .arg("--sdk")
        .arg(sdk)
        .arg("--output")
        .arg(&stable)
        .arg("--trust-root")
        .arg(&trusted)
        .args([
            "--channel",
            "stable",
            "--version",
            "0.2.0",
            "--sequence",
            "7",
            "--issued",
        ])
        .arg((fixture.now - 1).to_string())
        .arg("--expires")
        .arg((fixture.now + 3600).to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&signing).unwrap();
    let output = success(child.wait_with_output().unwrap());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["kind"], "toolchain_distribution");
    assert_eq!(report["root_sha256"], t::digest(&fixture.root));
    for key in &keys {
        assert!(!String::from_utf8_lossy(&output.stdout).contains(&hex(&key.to_bytes())));
    }
    published_shape(&stable);
    let target = t::installation::Identity::current().unwrap().target.text();
    let metadata_name = format!("channels/stable/{target}.json");
    let metadata = fs::read(stable.join(&metadata_name)).unwrap();
    assert!(produce(&fixture, sdk, &stable, Channel::Stable, "0.2.0", 7, &keys).is_err());
    assert_eq!(fs::read(stable.join(&metadata_name)).unwrap(), metadata);
    let root = fixture.installed_root();
    drop(Installation::init(&root, &fixture.root).unwrap());
    let first = install(&root, &stable, Channel::Stable).active.unwrap();
    assert_eq!(first.generation, report["generation"]);
    for (channel, name) in [(Channel::Beta, "beta"), (Channel::Nightly, "nightly")] {
        let source = fixture.dir.join(name);
        let made = produce(&fixture, sdk, &source, channel, "0.2.0", 7, &keys).unwrap();
        assert_eq!(made.generation, first.generation);
        assert_eq!(
            install(&root, &source, channel).active.unwrap().generation,
            first.generation
        );
    }
    let mut installed = Installation::open(&root).unwrap();
    installed.select(Channel::Stable, "0.2.0").unwrap();
    let old = installed.which().unwrap();
    installed.uninstall(Channel::Beta, "0.2.0").unwrap();
    installed.uninstall(Channel::Nightly, "0.2.0").unwrap();
    assert!(old.compiler.is_file());
    drop(installed);
    native_sdk_execution(&fixture, &old.compiler, &old.sdk);
    let mut running = RunningCompiler::start(&root);
    let second_sdk = fixture.dir.join("second-sdk");
    copy_sdk(sdk, &second_sdk);
    let original = fs::read(second_sdk.join("sdk.json")).unwrap();
    let mut parsed: Value = serde_json::from_slice(&original).unwrap();
    let mut packaging = serde_json::to_vec(&parsed).unwrap();
    if packaging == original {
        packaging = serde_json::to_vec_pretty(&parsed).unwrap();
    }
    assert_ne!(packaging, original);
    let updated = fixture.dir.join("updated-distribution");
    parsed["build_id"] = json!("a".repeat(64));
    fs::write(
        second_sdk.join("sdk.json"),
        serde_json::to_vec(&parsed).unwrap(),
    )
    .unwrap();
    let error = produce(
        &fixture,
        &second_sdk,
        &updated,
        Channel::Stable,
        "0.2.1",
        8,
        &keys,
    )
    .unwrap_err();
    assert!(!error.floor_advanced);
    assert!(
        !updated.exists(),
        "actual installer identity check rejects a falsely declared compiler build"
    );
    fs::write(second_sdk.join("sdk.json"), &packaging).unwrap();
    produce(
        &fixture,
        &second_sdk,
        &updated,
        Channel::Stable,
        "0.2.1",
        8,
        &keys,
    )
    .unwrap();
    published_shape(&updated);
    let mut installed = Installation::open(&root).unwrap();
    let changed = installed
        .update(&Source::local(&updated).unwrap(), Channel::Stable)
        .unwrap();
    assert_ne!(
        changed.active.as_ref().unwrap().generation,
        first.generation
    );
    assert_eq!(
        changed.active.as_ref().unwrap().compiler_build_id,
        first.compiler_build_id,
        "packaging update does not imply different compiler code"
    );
    let current = installed.which().unwrap();
    assert_eq!(fs::read(current.sdk.join("sdk.json")).unwrap(), packaging);
    let before = fixture.read_state();
    let result = installed.uninstall(Channel::Stable, "0.2.0");
    #[cfg(windows)]
    {
        let error = result.unwrap_err();
        match error.failure {
            Failure::Storage {
                operation: "move uninstall generation",
                ..
            } => assert_eq!(fixture.read_state(), before),
            Failure::Storage {
                operation: "remove recovery directory",
                ..
            } => {
                assert!(root.join("transaction.json").is_file());
                assert!(
                    !installed
                        .list()
                        .unwrap()
                        .installed
                        .iter()
                        .any(|s| s.version == "0.2.0")
                );
            }
            other => panic!("unexpected live executable removal failure: {other:?}"),
        }
    }
    #[cfg(not(windows))]
    {
        result.unwrap();
    }
    assert_eq!(fixture.read_state()["active"], before["active"]);
    drop(installed);
    running.finish();
    let mut installed = Installation::open(&root).unwrap();
    installed.recover().unwrap();
    if installed
        .list()
        .unwrap()
        .installed
        .iter()
        .any(|s| s.version == "0.2.0")
    {
        installed.uninstall(Channel::Stable, "0.2.0").unwrap();
    }
    assert!(!old.compiler.exists());
    assert!(current.compiler.is_file());
    assert_eq!(installed.list().unwrap().installed.len(), 1);
    drop(installed);
    historical_expiry(&fixture, &stable, &keys);
    assert!(!fs::read_dir(&fixture.dir).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".lang-distribution-")
    }));
}
fn native_sdk_execution(fixture: &Fixture, compiler: &Path, sdk: &Path) {
    let source = fixture.dir.join("native.lang");
    fs::write(&source, "fn main()->i64{return 42;}\n").unwrap();
    let package = fixture.dir.join("native-package");
    success(
        Command::new(compiler)
            .arg("native")
            .arg("--output")
            .arg(&package)
            .arg(&source)
            .output()
            .unwrap(),
    );
    fs::remove_file(&source).unwrap();
    let executable = fixture
        .dir
        .join(format!("installed-native{}", std::env::consts::EXE_SUFFIX));
    success(
        Command::new(compiler)
            .arg("link-native")
            .arg("--sdk")
            .arg(sdk)
            .arg("--package")
            .arg(&package)
            .arg("--output")
            .arg(&executable)
            .env("CARGO", fixture.dir.join("no-cargo"))
            .env("CARGO_HOME", fixture.dir.join("empty-cargo-home"))
            .output()
            .unwrap(),
    );
    assert_eq!(
        success(Command::new(executable).output().unwrap()).stdout,
        b"42\n"
    );
}
fn signed(payload: &Value, keys: &[SigningKey]) -> Vec<u8> {
    let payload = serde_json::to_string(payload).unwrap();
    let mut message = t::SIGNING_DOMAIN.to_vec();
    message.extend_from_slice(payload.as_bytes());
    let signatures: Vec<_> = keys.iter().map(|k| json!({"key_id":t::digest(&k.verifying_key().to_bytes()),"signature":hex(&k.sign(&message).to_bytes())})).collect();
    serde_json::to_vec(&json!({"schema":1,"payload":payload,"signatures":signatures})).unwrap()
}
fn historical_expiry(fixture: &Fixture, source: &Path, keys: &[SigningKey]) {
    let root = fixture.dir.join("historical-install");
    drop(Installation::init(&root, &fixture.root).unwrap());
    install(&root, source, Channel::Stable);
    let target = t::installation::Identity::current().unwrap().target.text();
    let name = format!("channels/stable/{target}.json");
    let envelope: Value = serde_json::from_slice(&fs::read(source.join(&name)).unwrap()).unwrap();
    let mut payload: Value = serde_json::from_str(envelope["payload"].as_str().unwrap()).unwrap();
    payload["issued"] = json!(fixture.now - 7200);
    payload["expires"] = json!(fixture.now - 3600);
    let bytes = signed(&payload, keys);
    let hash = t::digest(&bytes);
    fs::write(root.join(format!("metadata/{hash}.json")), &bytes).unwrap();
    // Construct authentic recorded history directly; production has no clock override.
    let mut state: Value =
        serde_json::from_slice(&fs::read(root.join("state.json")).unwrap()).unwrap();
    let receipt = json!({"schema":1,"metadata_sha256":hash,"admitted_at":fixture.now-7199});
    state["time_floor"] = json!(fixture.now - 7199);
    state["channels"][0]["receipt"] = receipt.clone();
    state["installed"][0]["receipt"] = receipt;
    fs::write(root.join("state.json"), serde_json::to_vec(&state).unwrap()).unwrap();
    let mut installed = Installation::open(&root).unwrap();
    installed.select(Channel::Stable, "0.2.0").unwrap();
    installed.which().unwrap();
    drop(installed);
    let output = success(state_cli(&root, "run", &["--", "toolchain", "identity"]));
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["schema"],
        1
    );
    let expired = fixture.dir.join("expired-source");
    fs::create_dir_all(expired.join("channels/stable")).unwrap();
    fs::write(expired.join(name), bytes).unwrap();
    let before = fs::read(root.join("state.json")).unwrap();
    let error = Installation::open(&root)
        .unwrap()
        .update(&Source::local(&expired).unwrap(), Channel::Stable)
        .unwrap_err();
    assert!(!error.floor_advanced);
    match error.failure {
        Failure::Admission(error) => assert_eq!(error.kind, t::ErrorKind::Expired),
        other => panic!("unexpected expiry failure: {other:?}"),
    }
    assert_eq!(fs::read(root.join("state.json")).unwrap(), before);
}
struct RunningCompiler {
    child: Child,
    input: Option<std::process::ChildStdin>,
    messages: mpsc::Receiver<Value>,
    reader: Option<thread::JoinHandle<()>>,
}
impl RunningCompiler {
    fn start(root: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_lang"))
            .args(["toolchain", "run", "--root"])
            .arg(root)
            .args(["--", "lsp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let mut output = BufReader::new(child.stdout.take().unwrap());
        let (sender, messages) = mpsc::channel();
        let reader = thread::spawn(move || {
            loop {
                let mut size = None;
                let mut header_bytes = 0;
                loop {
                    let mut line = String::new();
                    if output.read_line(&mut line).unwrap() == 0 {
                        return;
                    }
                    header_bytes += line.len();
                    assert!(header_bytes <= 8192);
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.strip_prefix("Content-Length: ") {
                        assert!(size.is_none());
                        size = Some(value.trim().parse::<usize>().unwrap());
                    }
                }
                let size = size.unwrap();
                assert!(size <= 4 * 1024 * 1024);
                let mut body = vec![0; size];
                output.read_exact(&mut body).unwrap();
                if sender.send(serde_json::from_slice(&body).unwrap()).is_err() {
                    return;
                }
            }
        });
        let mut client = Self {
            child,
            input,
            messages,
            reader: Some(reader),
        };
        client.send(
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}),
        );
        let response = client
            .messages
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        assert_eq!(response["id"], 1);
        assert!(response.get("error").is_none());
        assert!(response["result"]["capabilities"].is_object());
        client.send(json!({"jsonrpc":"2.0","method":"initialized","params":{}}));
        client
    }
    fn send(&mut self, value: Value) {
        let bytes = serde_json::to_vec(&value).unwrap();
        let input = self.input.as_mut().unwrap();
        write!(input, "Content-Length: {}\r\n\r\n", bytes.len()).unwrap();
        input.write_all(&bytes).unwrap();
        input.flush().unwrap();
    }
    fn finish(&mut self) {
        self.send(json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}));
        let response = self.messages.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(response["id"], 2);
        assert!(response.get("error").is_none());
        self.send(json!({"jsonrpc":"2.0","method":"exit","params":null}));
        self.input.take();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert_eq!(status.code(), Some(0));
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let mut errors = Vec::new();
        self.child
            .stderr
            .take()
            .unwrap()
            .take(65537)
            .read_to_end(&mut errors)
            .unwrap();
        assert!(errors.is_empty(), "{}", String::from_utf8_lossy(&errors));
        self.reader.take().unwrap().join().unwrap();
    }
}
impl Drop for RunningCompiler {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
