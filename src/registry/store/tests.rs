use super::*;
use std::cell::Cell;
mod adversarial;
mod index_capacity;
mod local_admin;
mod owners;
mod quarantine;
mod reads;
mod workspace;
mod yank;

struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let mut random = [0u8; 16];
        getrandom::fill(&mut random).unwrap();
        let path =
            std::env::temp_dir().join(format!("lang-registry-{}", authority::codec::hex(&random)));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const PRINCIPAL: &str = "00000000000000000000000000000001";
const REQUEST: &str = "00000000000000000000000000000001";
const SOURCE: &[u8] = b"fn main() -> i64 { return 42; }\r\n";
thread_local! { static CUT: Cell<Option<(usize, usize)>> = const { Cell::new(None) }; }
#[derive(Clone, Copy)]
enum Fault {
    Before,
    After,
    Unreadable,
}
thread_local! { static FAULT: Cell<Option<Fault>> = const { Cell::new(None) }; }
thread_local! { static FAULT_HEAD: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) }; }
pub(super) fn replacement_fault(after: bool, dir: &Dir) -> Result<(), Error> {
    FAULT.with(|fault| match (fault.get(), after) {
        (Some(Fault::Before), false) | (Some(Fault::After), true) => {
            fault.set(None);
            Err(io())
        }
        (Some(Fault::Unreadable), true) => {
            fault.set(None);
            FAULT_HEAD.with(|head| *head.borrow_mut() = Some(dir.read("HEAD").unwrap()));
            dir.remove_file("HEAD").unwrap();
            Err(io())
        }
        _ => Ok(()),
    })
}
pub(super) fn checkpoint(_label: &str) {
    CUT.with(|cut| {
        if let Some((target, count)) = cut.get() {
            let count = count + 1;
            if count == target {
                std::process::exit(86);
            }
            cut.set(Some((target, count)));
        }
    });
}
struct At(u64);
impl Clock for At {
    fn now(&self) -> Result<u64, Error> {
        Ok(self.0)
    }
}
fn identity() -> ServiceIdentity {
    ServiceIdentity::new("example", "https://registry.example").unwrap()
}
fn bearer() -> Bearer {
    Bearer::from_input("11".repeat(32).as_bytes()).unwrap()
}
fn authority_bytes() -> Vec<u8> {
    let value = serde_json::json!({
        "format":"lang-registry-authority", "schema":1, "registry":"example", "origin":"https://registry.example", "time_floor":100,
        "principals":[{"id":PRINCIPAL, "active":true}],
        "tokens":[{"id":"01".repeat(16), "principal":PRINCIPAL, "registry":"example", "origin":"https://registry.example",
            "authenticator_sha256":"3dec5dc6d20c715eada8190369186e6884d3b159da462c13cc112a2a76eff05e", "issued":100, "expires":200, "revoked":false,
            "grants":[{"action":"claim", "name":"answer"}, {"action":"publish", "name":"answer"}]}],
        "packages":[]
    });
    let records: authority::codec::Records = serde_json::from_value(value).unwrap();
    records::encode(&records, authority::MAX_BYTES).unwrap()
}
fn archive(version: &str) -> AdmittedArchive {
    let descriptor: archive::Descriptor = serde_json::from_value(serde_json::json!({
        "format":"lang-registry-source", "schema":1,
        "package":{"name":"answer", "version":version, "edition":"2026", "entry":"src/main.lang"},
        "tests":[], "dependencies":[], "files":[{"path":"src/main.lang", "bytes":SOURCE.len(), "sha256":digest(SOURCE)}]
    })).unwrap();
    let metadata = records::encode(&descriptor, archive::MAX_DESCRIPTOR_BYTES).unwrap();
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in [
        (".lang-package.json", metadata.as_slice()),
        ("src/main.lang", SOURCE),
    ] {
        let mut h = tar::Header::new_ustar();
        h.set_path(path).unwrap();
        h.set_mode(0o644);
        h.set_uid(0);
        h.set_gid(0);
        h.set_mtime(0);
        h.set_size(bytes.len() as u64);
        h.set_entry_type(tar::EntryType::Regular);
        h.set_device_major(0).unwrap();
        h.set_device_minor(0).unwrap();
        h.set_cksum();
        let sum = h.cksum().unwrap();
        h.as_mut_bytes()[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        builder.append(&h, bytes).unwrap();
    }
    AdmittedArchive::admit(&builder.into_inner().unwrap()).unwrap()
}
fn parent(path: &std::path::Path) -> Dir {
    Dir::open_ambient_dir(path, cap_std::ambient_authority()).unwrap()
}
fn initialize(parent: &Dir) -> Registry {
    Registry::initialize(
        parent,
        "registry",
        &identity(),
        &authority_bytes(),
        PRINCIPAL,
        &At(100),
    )
    .unwrap()
}
fn reopen(parent: &Dir) -> Registry {
    Registry::open(parent, "registry", &identity()).unwrap()
}

#[test]
fn initialize_publish_reopen_replay_and_immutable_conflicts() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut store = initialize(&parent);
    assert_eq!(store.revision().unwrap(), 1);
    assert!(
        Registry::initialize(
            &parent,
            "registry",
            &identity(),
            &authority_bytes(),
            PRINCIPAL,
            &At(100)
        )
        .is_err()
    );
    assert_eq!(
        Registry::open(&parent, "registry", &identity())
            .err()
            .unwrap()
            .kind,
        ErrorKind::Busy
    );
    let archive = archive("0.1.0");
    let receipt = store
        .publish(
            &bearer(),
            &archive,
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
    assert_eq!(receipt.state_revision(), 2);
    drop(store);
    let mut store = reopen(&parent);
    assert_eq!(store.revision().unwrap(), 2);
    assert_eq!(
        store
            .publish(
                &bearer(),
                &archive,
                REQUEST,
                ExpectedRevision::Absent,
                &At(111)
            )
            .unwrap(),
        receipt
    );
    assert_eq!(
        store
            .publish(
                &bearer(),
                &archive,
                REQUEST,
                ExpectedRevision::Revision(1),
                &At(111)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    assert_eq!(
        store
            .publish(
                &bearer(),
                &archive,
                &"02".repeat(16),
                ExpectedRevision::Revision(1),
                &At(111)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Conflict
    );
    let raw = store
        .storage
        .immutable(storage::Space::Object, archive.sha256())
        .unwrap();
    let admitted = AdmittedArchive::admit(&raw).unwrap();
    assert_eq!(admitted.source("src/main.lang"), Some(SOURCE));
    assert_eq!(raw, archive.as_bytes());
    assert_eq!(
        archive.sha256(),
        "ca4fe329e4da43f690e2236c65a5888d5791275a86b22bf11ac9bf5cced433c4"
    );
    assert_eq!(
        records::request_digest(
            "answer",
            "0.1.0",
            ExpectedRevision::Absent,
            archive.sha256(),
            raw.len() as u64
        )
        .unwrap(),
        "0914a2741a3cfc461427595677d8e64c373fc6c104cf41102252a986ab69a08f"
    );
    let analysis = crate::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        std::path::Path::new("src/main.lang"),
        admitted.source("src/main.lang").unwrap(),
        None,
        Default::default(),
        None,
    );
    let lang_semantics::analysis::Program::Copy(typed) = analysis.result.unwrap() else {
        panic!("scalar fixture");
    };
    let program =
        crate::frontend::compile_script(&typed, &analysis.sources, Default::default()).unwrap();
    let result = lang_vm::unit::execute_entry(
        &program,
        &lang_runtime::HostFunctionRegistry::new(),
        &lang_vm::unit::HostBindings::new(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(result.value.as_i64(), Some(42));
    let view = history::load(&store.storage, &identity()).unwrap();
    assert_eq!(view.state.authority.packages[0].owners, [PRINCIPAL]);
    assert_eq!(view.requests.len(), 1);
    assert_eq!(view.reachable.len(), 5);
}

#[test]
fn numeric_versions_and_fresh_authentication_after_staging() {
    let tmp = Temp::new();
    let parent = parent(tmp.path());
    let mut store = initialize(&parent);
    struct Advances(Cell<usize>);
    impl Clock for Advances {
        fn now(&self) -> Result<u64, Error> {
            let n = self.0.get();
            self.0.set(n + 1);
            Ok(if n == 0 { 199 } else { 200 })
        }
    }
    let first = archive("1.10.0");
    let e = store
        .publish(
            &bearer(),
            &first,
            REQUEST,
            ExpectedRevision::Absent,
            &Advances(Cell::new(0)),
        )
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::Authentication);
    assert_eq!(e.outcome, PublicationOutcome::NotPublished);
    assert_eq!(store.revision().unwrap(), 1);
    store
        .publish(
            &bearer(),
            &first,
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
    store
        .publish(
            &bearer(),
            &archive("1.2.0"),
            &"02".repeat(16),
            ExpectedRevision::Revision(1),
            &At(111),
        )
        .unwrap();
    let state = history::load(&store.storage, &identity()).unwrap().state;
    assert_eq!(
        state.packages[0]
            .releases
            .iter()
            .map(|r| r.version.as_str())
            .collect::<Vec<_>>(),
        ["1.2.0", "1.10.0"]
    );
    assert_eq!(state.packages[0].revision, 2);
    assert_eq!(
        store
            .publish(
                &bearer(),
                &first,
                REQUEST,
                ExpectedRevision::Absent,
                &At(200)
            )
            .unwrap_err()
            .kind,
        ErrorKind::Authentication
    );
    assert_eq!(
        store
            .publish(
                &bearer(),
                &first,
                REQUEST,
                ExpectedRevision::Absent,
                &At(109)
            )
            .unwrap_err()
            .kind,
        ErrorKind::State
    );
}

#[test]
fn publication_child() {
    let Some(path) = std::env::var_os("LANG_REGISTRY_TEST_ROOT") else {
        return;
    };
    let parent = parent(std::path::Path::new(&path));
    let mut store = reopen(&parent);
    let cut: usize = std::env::var("LANG_REGISTRY_TEST_CUT")
        .unwrap()
        .parse()
        .unwrap();
    CUT.with(|c| c.set(Some((cut, 0))));
    store
        .publish(
            &bearer(),
            &archive("0.1.0"),
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
}

#[test]
fn every_publication_process_exit_reopens_complete_old_or_new_state() {
    for cut in 1..=24 {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        drop(initialize(&parent));
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "registry::store::tests::publication_child",
                "--nocapture",
            ])
            .env("LANG_REGISTRY_TEST_ROOT", tmp.path())
            .env("LANG_REGISTRY_TEST_CUT", cut.to_string())
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(86),
            "cut {cut}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let mut store = reopen(&parent);
        let expected = if cut == 24 { 2 } else { 1 };
        assert_eq!(store.revision().unwrap(), expected, "cut {cut}");
        assert_eq!(
            std::fs::read_dir(tmp.path().join("registry/staging"))
                .unwrap()
                .count(),
            0
        );
        let receipt = store
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(111),
            )
            .unwrap();
        assert_eq!(receipt.state_revision(), 2);
        assert_eq!(store.revision().unwrap(), 2);
    }
}

#[test]
fn final_head_guard_and_unknown_outcome_require_exact_reconciliation() {
    for times in [[110, 150, 200], [110, 150, 149]] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut store = initialize(&parent);
        struct Times {
            values: [u64; 3],
            index: Cell<usize>,
        }
        impl Clock for Times {
            fn now(&self) -> Result<u64, Error> {
                let i = self.index.get();
                self.index.set(i + 1);
                Ok(self.values[i])
            }
        }
        let failure = store
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &Times {
                    values: times,
                    index: Cell::new(0),
                },
            )
            .unwrap_err();
        assert_eq!(
            failure.kind,
            if times[2] == 200 {
                ErrorKind::Authentication
            } else {
                ErrorKind::State
            }
        );
        assert_eq!(failure.outcome, PublicationOutcome::NotPublished);
        drop(store);
        assert_eq!(reopen(&parent).revision().unwrap(), 1);
    }
    for fault in [Fault::Before, Fault::After, Fault::Unreadable] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut store = initialize(&parent);
        FAULT.with(|f| f.set(Some(fault)));
        let result = store.publish(
            &bearer(),
            &archive("0.1.0"),
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        );
        match fault {
            Fault::Before => {
                assert_eq!(
                    result.unwrap_err().outcome,
                    PublicationOutcome::NotPublished
                );
                assert_eq!(store.revision().unwrap(), 1);
            }
            Fault::After => {
                let receipt = result.unwrap();
                assert_eq!(receipt.state_revision(), 2);
                assert_eq!(
                    store
                        .publish(
                            &bearer(),
                            &archive("0.1.0"),
                            REQUEST,
                            ExpectedRevision::Absent,
                            &At(111)
                        )
                        .unwrap(),
                    receipt
                );
            }
            Fault::Unreadable => {
                assert_eq!(result.unwrap_err().outcome, PublicationOutcome::Unknown);
                assert!(store.revision().is_err());
            }
        }
    }
}

fn snapshot(path: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, String> {
    let mut map = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let p = entry.path();
        if entry.file_type().unwrap().is_dir() {
            map.extend(snapshot(&p));
        } else if entry.file_type().unwrap().is_symlink() {
            map.insert(p.clone(), format!("{:?}", std::fs::read_link(p).unwrap()));
        } else if entry.metadata().unwrap().len() == 0 {
            // The exact contents are empty. Avoid a second-handle EOF read of
            // the exclusively locked LOCK file, which Windows rejects.
            map.insert(p, digest(b""));
        } else {
            map.insert(p.clone(), digest(&std::fs::read(p).unwrap()));
        }
    }
    map
}
fn refuse_without_cleanup(tmp: &Temp, parent: &Dir) {
    let root = tmp.path().join("registry");
    let before = snapshot(&root);
    assert!(Registry::open(parent, "registry", &identity()).is_err());
    assert_eq!(snapshot(&root), before);
}

#[test]
fn corrupt_unknown_aliased_and_missing_inputs_are_never_cleaned_or_reset() {
    for case in 0..8 {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut store = initialize(&parent);
        store
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(110),
            )
            .unwrap();
        let head = store.storage.head().unwrap();
        let state = history::load(&store.storage, &identity()).unwrap().state;
        drop(store);
        let root = tmp.path().join("registry");
        std::fs::write(
            root.join(format!("staging/object-{}", "aa".repeat(32))),
            b"partial",
        )
        .unwrap();
        match case {
            0 => std::fs::write(root.join("unknown"), b"unrecognized").unwrap(),
            1 => std::fs::write(root.join("HEAD"), b"{}").unwrap(),
            2 => std::fs::write(root.join(format!("states/{head}")), b"{}").unwrap(),
            3 => std::fs::remove_file(root.join("HEAD")).unwrap(),
            4 => std::fs::remove_file(root.join(format!("audits/{}", state.last_audit_sha256)))
                .unwrap(),
            5 => std::fs::hard_link(root.join("HEAD"), tmp.path().join("alias")).unwrap(),
            6 => std::fs::write(
                root.join(format!("objects/{}", "bb".repeat(32))),
                b"incomplete immutable",
            )
            .unwrap(),
            7 => std::fs::create_dir(root.join(format!("staging/state-{}", "cc".repeat(32))))
                .unwrap(),
            _ => unreachable!(),
        }
        refuse_without_cleanup(&tmp, &parent);
        assert!(
            Registry::initialize(
                &parent,
                "registry",
                &identity(),
                &authority_bytes(),
                PRINCIPAL,
                &At(100)
            )
            .is_err()
        );
    }
}

#[cfg(unix)]
#[test]
fn symlink_files_and_namespaces_are_refused_without_following() {
    use std::os::unix::fs::symlink;
    for is_dir in [false, true] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        drop(initialize(&parent));
        let root = tmp.path().join("registry");
        let original = root.join(if is_dir { "objects" } else { "HEAD" });
        let outside = tmp.path().join("outside");
        std::fs::rename(&original, &outside).unwrap();
        symlink(&outside, &original).unwrap();
        refuse_without_cleanup(&tmp, &parent);
    }
}

#[test]
fn recomputed_hashes_do_not_hide_unaudited_policy_or_release_changes() {
    for case in 0..6 {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let mut store = initialize(&parent);
        store
            .publish(
                &bearer(),
                &archive("0.1.0"),
                REQUEST,
                ExpectedRevision::Absent,
                &At(110),
            )
            .unwrap();
        let mut state = history::load(&store.storage, &identity()).unwrap().state;
        match case {
            0 => state.authority.tokens[0].revoked = true,
            1 => state.authority.tokens[0].expires += 1,
            2 => state.packages[0].releases[0].edition = "2027".into(),
            3 => state.packages[0].revision += 1,
            4 => {
                let mut audit: records::Audit = records::decode(
                    &store
                        .storage
                        .immutable(storage::Space::Audit, &state.last_audit_sha256)
                        .unwrap(),
                    records::MAX_AUDIT,
                )
                .unwrap();
                audit.request_sha256 = Some("ab".repeat(32));
                let bytes = records::encode(&audit, records::MAX_AUDIT).unwrap();
                let hash = digest(&bytes);
                store
                    .storage
                    .install(storage::Space::Audit, &hash, &bytes)
                    .unwrap();
                state.last_audit_sha256 = hash;
            }
            5 => state.packages[0].releases[0].yanked = true,
            _ => unreachable!(),
        }
        let bytes = records::encode(&state, records::MAX_STATE).unwrap();
        let hash = digest(&bytes);
        store
            .storage
            .install(storage::Space::State, &hash, &bytes)
            .unwrap();
        store
            .storage
            .replace_head(&hash, &head_bytes(&hash).unwrap(), || Ok(()))
            .unwrap();
        drop(store);
        refuse_without_cleanup(&tmp, &parent);
    }
}

#[test]
fn bootstrap_child() {
    let Some(path) = std::env::var_os("LANG_REGISTRY_BOOTSTRAP_ROOT") else {
        return;
    };
    let parent = parent(std::path::Path::new(&path));
    let cut: usize = std::env::var("LANG_REGISTRY_TEST_CUT")
        .unwrap()
        .parse()
        .unwrap();
    CUT.with(|c| c.set(Some((cut, 0))));
    drop(initialize(&parent));
}
#[test]
fn interrupted_bootstrap_never_silently_reinitializes_existing_root() {
    for cut in 1..=22 {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "registry::store::tests::bootstrap_child",
                "--nocapture",
            ])
            .env("LANG_REGISTRY_BOOTSTRAP_ROOT", tmp.path())
            .env("LANG_REGISTRY_TEST_CUT", cut.to_string())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(86), "bootstrap cut {cut}");
        if cut == 22 {
            assert_eq!(reopen(&parent).revision().unwrap(), 1);
        } else {
            assert!(Registry::open(&parent, "registry", &identity()).is_err());
        }
        assert!(
            Registry::initialize(
                &parent,
                "registry",
                &identity(),
                &authority_bytes(),
                PRINCIPAL,
                &At(100)
            )
            .is_err()
        );
    }
}

fn wait_for(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "child coordination timeout"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
#[test]
fn competing_writer_child() {
    let Some(path) = std::env::var_os("LANG_REGISTRY_WRITER_ROOT") else {
        return;
    };
    let root = std::path::Path::new(&path);
    let parent = parent(root);
    let mode = std::env::var("LANG_REGISTRY_WRITER_MODE").unwrap();
    if mode == "busy" {
        assert_eq!(
            Registry::open(&parent, "registry", &identity())
                .err()
                .unwrap()
                .kind,
            ErrorKind::Busy
        );
        return;
    }
    let mut store = reopen(&parent);
    std::fs::write(root.join("ready"), b"").unwrap();
    wait_for(&root.join("release"));
    store
        .publish(
            &bearer(),
            &archive(&mode),
            REQUEST,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
}
struct Child(std::process::Child);
impl Drop for Child {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn writer(root: &std::path::Path, mode: &str) -> Child {
    Child(
        std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "registry::store::tests::competing_writer_child",
                "--nocapture",
            ])
            .env("LANG_REGISTRY_WRITER_ROOT", root)
            .env("LANG_REGISTRY_WRITER_MODE", mode)
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    )
}
#[test]
fn independent_process_writers_serialize_both_version_orders() {
    for (first, second) in [("0.1.0", "0.2.0"), ("0.2.0", "0.1.0")] {
        let tmp = Temp::new();
        let parent = parent(tmp.path());
        drop(initialize(&parent));
        let mut child = writer(tmp.path(), first);
        wait_for(&tmp.path().join("ready"));
        let mut competitor = writer(tmp.path(), "busy");
        assert!(competitor.0.wait().unwrap().success());
        std::fs::write(tmp.path().join("release"), b"").unwrap();
        assert!(child.0.wait().unwrap().success());
        let mut store = reopen(&parent);
        assert_eq!(
            store
                .publish(
                    &bearer(),
                    &archive(second),
                    &"02".repeat(16),
                    ExpectedRevision::Absent,
                    &At(111)
                )
                .unwrap_err()
                .kind,
            ErrorKind::Conflict
        );
        store
            .publish(
                &bearer(),
                &archive(second),
                &"02".repeat(16),
                ExpectedRevision::Revision(1),
                &At(111),
            )
            .unwrap();
        assert_eq!(store.revision().unwrap(), 3);
        let view = history::load(&store.storage, &identity()).unwrap();
        assert_eq!(view.state.packages[0].releases.len(), 2);
        assert_eq!(view.requests.len(), 2);
    }
}
