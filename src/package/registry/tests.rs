use super::*;
use crate::{
    frontend,
    package::{
        graph::{Graph, Limits},
        source,
    },
    registry::{
        store::{Clock, ExpectedRevision, Registry},
        transport::{Client, Server},
    },
};
use std::{
    cell::Cell,
    fs,
    net::TcpListener,
    path::{Path, PathBuf},
};

mod cache;
mod transitive;

const PRINCIPAL: &str = "00000000000000000000000000000001";
const APP: &[u8] =
    include_bytes!("../../../tests/fixtures/package-lock/schema3-registry-inputs/app/Lang.json");
const MAIN: &[u8] = include_bytes!(
    "../../../tests/fixtures/package-lock/schema3-registry-inputs/app/src/main.lang"
);
const METADATA: &[u8] = include_bytes!(
    "../../../tests/fixtures/package-lock/schema3-registry-inputs/math/.lang-package.json"
);
const SOURCE_BYTES: &str = include_str!(
    "../../../tests/fixtures/package-lock/schema3-registry-inputs/math/source-bytes.json"
);

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        let mut id = [0; 16];
        getrandom::fill(&mut id).unwrap();
        let path = std::env::temp_dir().join(format!("lang-registry-graph-{}", hex(&id)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
struct At(u64);
impl Clock for At {
    fn now(&self) -> Result<u64, crate::registry::store::Error> {
        Ok(self.0)
    }
}

fn archive() -> AdmittedArchive {
    let sources: BTreeMap<String, String> = serde_json::from_str(SOURCE_BYTES).unwrap();
    pack(METADATA, &sources)
}

fn pack(metadata: &[u8], sources: &BTreeMap<String, String>) -> AdmittedArchive {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in std::iter::once((".lang-package.json", metadata))
        .chain(sources.iter().map(|(p, b)| (p.as_str(), b.as_bytes())))
    {
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

struct IsolatedHttps {
    client: Client,
    reads: Cell<usize>,
}
impl Source for IsolatedHttps {
    fn resolve(&self, request: Request<'_>) -> Result<ConsumedArchive, Diagnostic> {
        assert_eq!(request.binding.identity(), self.client.identity());
        self.reads.set(self.reads.get() + 1);
        let index = self.client.index(request.name).unwrap();
        let archive = self.client.archive(&index, request.version).unwrap();
        ConsumedArchive::isolated_unsigned(
            request.binding.clone(),
            &index,
            request.version,
            archive.as_bytes(),
        )
    }
}

fn serve(parent: &cap_std::fs::Dir, archive: &AdmittedArchive) -> (Server, IsolatedHttps, Binding) {
    use crate::registry::authority::Bearer;
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
    let authority = r#"{
  "format": "lang-registry-authority",
  "schema": 1,
  "registry": "example",
  "origin": "$ORIGIN",
  "time_floor": 100,
  "principals": [
    {
      "id": "00000000000000000000000000000001",
      "active": true
    }
  ],
  "tokens": [
    {
      "id": "01010101010101010101010101010101",
      "principal": "00000000000000000000000000000001",
      "registry": "example",
      "origin": "$ORIGIN",
      "authenticator_sha256": "$HASH",
      "issued": 100,
      "expires": 200,
      "revoked": false,
      "grants": [
        {
          "action": "claim",
          "name": "math"
        },
        {
          "action": "publish",
          "name": "math"
        }
      ]
    }
  ],
  "packages": []
}
"#
    .replace("$ORIGIN", identity.origin())
    .replace("$HASH", &bearer.authenticator_sha256());
    let mut registry = Registry::initialize(
        parent,
        "service",
        &identity,
        authority.as_bytes(),
        PRINCIPAL,
        &At(100),
    )
    .unwrap();
    registry
        .publish(
            &bearer,
            archive,
            PRINCIPAL,
            ExpectedRevision::Absent,
            &At(110),
        )
        .unwrap();
    let cert = rcgen::generate_simple_self_signed(vec!["127.0.0.1".into()]).unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert.cert.der().clone()).unwrap();
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()).into();
    let server = Server::spawn(registry, listener, vec![cert.cert.der().clone()], key).unwrap();
    let binding = Binding::new(identity.clone(), [0x22; 32]);
    (
        server,
        IsolatedHttps {
            client: Client::new(identity, roots),
            reads: Cell::new(0),
        },
        binding,
    )
}

fn execute(graph: &Graph, expected: i64) {
    let analysis = source::analyze(graph, Default::default()).unwrap();
    assert!(analysis.result.is_ok(), "{:?}", analysis.diagnostics);
    let lang_semantics::analysis::Program::Copy(typed) = analysis.result.unwrap() else {
        panic!("Copy fixture")
    };
    let program = frontend::compile_script(&typed, &analysis.sources, Default::default()).unwrap();
    let hosts = lang_runtime::HostFunctionRegistry::new();
    let vm = lang_vm::unit::execute_entry(
        &program,
        &hosts,
        &lang_vm::unit::HostBindings::new(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(vm.value.as_i64(), Some(expected));
    let unit = program.verified();
    let original: BTreeMap<String, String> = serde_json::from_str(SOURCE_BYTES).unwrap();
    for text in original.values() {
        assert!(unit.unit().sources.iter().any(|s| s.text == *text));
    }
    let native =
        lang_codegen_clif::program::CompiledProgram::compile(unit, &[], &hosts, Default::default())
            .unwrap();
    assert_eq!(
        native
            .invoke(unit.unit().entry.unwrap(), &[])
            .unwrap()
            .cells,
        vec![expected]
    );
}

#[test]
fn real_https_graph_freezes_original_sources_and_executes_vm_and_native() {
    let temp = Temp::new();
    let directory =
        cap_std::fs::Dir::open_ambient_dir(&temp.0, cap_std::ambient_authority()).unwrap();
    let archive = archive();
    assert_eq!(
        archive.sha256(),
        "5b94f073ed533cc26edfff1f357c83d9cf94d68a6289822678bdd36336a58fea"
    );
    let (mut server, provider, binding) = serve(&directory, &archive);
    fs::write(temp.0.join("Lang.json"), APP).unwrap();
    fs::create_dir(temp.0.join("src")).unwrap();
    fs::write(temp.0.join("src/main.lang"), MAIN).unwrap();
    let context = Context::new(BTreeMap::from([("example".into(), binding)]), &provider).unwrap();
    let graph = Graph::resolve_with_registries_in(
        directory.try_clone().unwrap(),
        temp.0.clone(),
        Path::new("Lang.json"),
        "aarch64-apple-darwin",
        Limits::default(),
        None,
        &context,
    )
    .unwrap();
    assert_eq!(provider.reads.get(), 1);
    assert_eq!(graph.packages().len(), 2);
    assert_eq!(
        graph.package(graph.root()).unwrap().root(),
        Path::new("local")
    );
    let math = &graph.packages()[1];
    assert!(math.manifest().is_none());
    assert_eq!(math.manifest_bytes(), METADATA);
    assert_eq!(
        math.registry().unwrap().archive().sha256(),
        archive.sha256()
    );
    assert!(crate::package::archive::Archive::capture(&graph).is_err());
    server.stop().unwrap();
    drop(server);
    fs::remove_dir_all(temp.0.join("src")).unwrap();
    fs::remove_file(temp.0.join("Lang.json")).unwrap();
    execute(&graph, 42);
    execute(&graph, 42);
    assert_eq!(provider.reads.get(), 1);
}

#[test]
fn hybrid_graph_keeps_colliding_local_keys_distinct_and_reuses_registry_aliases() {
    let temp = Temp::new();
    let directory =
        cap_std::fs::Dir::open_ambient_dir(&temp.0, cap_std::ambient_authority()).unwrap();
    let archive = archive();
    let (mut server, provider, binding) = serve(&directory, &archive);
    let collision = format!(
        "registry/{}/math/1.0.0/{}",
        binding.policy_id(),
        archive.sha256()
    );
    let local_root = temp.0.join(&collision);
    fs::create_dir_all(local_root.join("src")).unwrap();
    fs::write(local_root.join("Lang.json"), serde_json::to_vec(&serde_json::json!({"schema":1,"package":{"name":"local_math","version":"1.0.0","edition":"2026","entry":"src/lib.lang"}})).unwrap()).unwrap();
    for file in archive.descriptor().files() {
        fs::write(
            local_root.join(file.path()),
            archive.source(file.path()).unwrap(),
        )
        .unwrap();
    }
    let mut manifest: serde_json::Value = serde_json::from_slice(APP).unwrap();
    let remote = manifest["dependencies"]["math"].clone();
    manifest["dependencies"] = serde_json::json!({
        "remote_math": remote, "second": remote,
        "local_math": {"source":"local", "path":collision, "version":"1.0.0"}
    });
    fs::write(
        temp.0.join("Lang.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    fs::create_dir(temp.0.join("src")).unwrap();
    fs::write(temp.0.join("src/main.lang"), "mod remote_math; mod local_math; fn main() -> i64 { return remote_math::answer() + local_math::answer(); }\n").unwrap();
    let context = Context::new(BTreeMap::from([("example".into(), binding)]), &provider).unwrap();
    let graph = Graph::resolve_with_registries_in(
        directory.try_clone().unwrap(),
        temp.0.clone(),
        Path::new("Lang.json"),
        "aarch64-apple-darwin",
        Limits::default(),
        None,
        &context,
    )
    .unwrap();
    assert_eq!(graph.packages().len(), 3);
    assert_eq!(provider.reads.get(), 1);
    let edges = graph.package(graph.root()).unwrap().dependencies();
    assert_eq!(edges["remote_math"], edges["second"]);
    assert_ne!(edges["remote_math"], edges["local_math"]);
    let inputs = graph.inputs().unwrap();
    let registry_key = Path::new(&collision).join("src/lib.lang");
    let local_key = Path::new("local").join(&registry_key);
    assert_eq!(
        inputs.files[&local_key].physical.as_deref(),
        Some(registry_key.as_path())
    );
    assert!(inputs.files[&registry_key].physical.is_none());
    assert_ne!(
        inputs.files[&local_key].identity,
        inputs.files[&registry_key].identity
    );
    let plan = crate::package::format::Plan::capture(&graph).unwrap();
    assert!(plan.changes().iter().any(|c| c.path() == registry_key));
    assert!(plan.changes().iter().all(|c| {
        inputs
            .files
            .values()
            .any(|f| f.physical.as_deref() == Some(c.path()))
    }));
    assert!(
        crate::package::format::Plan::from_replacements(
            &graph,
            [(
                registry_key.clone(),
                "pub fn answer()->i64{return 0;}".into()
            )]
        )
        .is_err()
    );
    plan.apply(&directory).unwrap();
    assert_eq!(
        graph.packages()[edges["remote_math"].index()]
            .registry()
            .unwrap()
            .archive()
            .as_bytes(),
        archive.as_bytes()
    );
    let error = source::analyze_indexed(
        &graph,
        None,
        &BTreeMap::from([(
            registry_key,
            std::sync::Arc::from(b"pub fn answer()->i64{return 0;}".as_slice()),
        )]),
        Default::default(),
    )
    .err()
    .expect("registry overlay refused");
    assert_eq!(error.code, "PKG0003");
    assert!(error.message.contains("read-only"));
    server.stop().unwrap();
    drop(server);
    fs::remove_dir_all(local_root).unwrap();
    fs::remove_dir_all(temp.0.join("src")).unwrap();
    execute(&graph, 84);
}

#[test]
fn host_binding_keys_are_explicit_before_any_source_request() {
    struct Never(Cell<usize>);
    impl Source for Never {
        fn resolve(&self, _: Request<'_>) -> Result<ConsumedArchive, Diagnostic> {
            self.0.set(self.0.get() + 1);
            Err(refused("unexpected source request"))
        }
    }
    let provider = Never(Cell::new(0));
    let binding = Binding::new(
        ServiceIdentity::new("example", "https://registry.example").unwrap(),
        [0x22; 32],
    );
    assert!(
        Context::new(
            BTreeMap::from([("other".into(), binding.clone())]),
            &provider
        )
        .is_err()
    );
    let context = Context::new(BTreeMap::from([("example".into(), binding)]), &provider).unwrap();
    assert!(context.resolve("other", "math", "1.0.0", None).is_err());
    assert_eq!(provider.0.get(), 0);
}

#[test]
fn policy_key_matches_independent_fixture() {
    let binding = Binding::new(
        ServiceIdentity::new("example", "https://registry.example").unwrap(),
        [0x22; 32],
    );
    assert_eq!(
        binding.policy_id(),
        "21ccc6bc6516ce62c75c6f34d3990e619db61fc8000ce53d452940845371d0c9"
    );
}

struct IsolatedMemory {
    binding: Binding,
    index: crate::registry::store::Index,
    archive: AdmittedArchive,
}
impl IsolatedMemory {
    fn new(binding: Binding, archive: AdmittedArchive) -> Self {
        #[derive(serde::Serialize)]
        struct Release<'a> {
            version: &'a str,
            archive_sha256: &'a str,
            archive_bytes: u64,
            descriptor_sha256: &'a str,
            edition: &'a str,
            dependencies: &'a [crate::registry::archive::Dependency],
            yanked: bool,
            quarantined: bool,
        }
        #[derive(serde::Serialize)]
        struct Index<'a> {
            format: &'static str,
            schema: u32,
            registry: &'a str,
            name: &'a str,
            revision: u64,
            releases: [Release<'a>; 1],
        }
        let package = archive.descriptor().package();
        let record = Index {
            format: "lang-registry-index",
            schema: 1,
            registry: binding.identity().registry(),
            name: package.name(),
            revision: 1,
            releases: [Release {
                version: package.version(),
                archive_sha256: archive.sha256(),
                archive_bytes: archive.as_bytes().len() as u64,
                descriptor_sha256: archive.descriptor_sha256(),
                edition: package.edition(),
                dependencies: archive.descriptor().dependencies(),
                yanked: false,
                quarantined: false,
            }],
        };
        let mut bytes = serde_json::to_vec_pretty(&record).unwrap();
        bytes.push(b'\n');
        let index =
            crate::registry::store::Index::admit(&bytes, binding.identity(), package.name())
                .unwrap();
        Self {
            binding,
            index,
            archive,
        }
    }
}
impl Source for IsolatedMemory {
    fn resolve(&self, request: Request<'_>) -> Result<ConsumedArchive, Diagnostic> {
        ConsumedArchive::isolated_unsigned(
            self.binding.clone(),
            &self.index,
            request.version,
            self.archive.as_bytes(),
        )
    }
}

#[test]
fn independent_policy_and_exact_release_requirements_cannot_be_substituted() {
    let binding = Binding::new(
        ServiceIdentity::new("example", "https://registry.example").unwrap(),
        [0x22; 32],
    );
    let provider = IsolatedMemory::new(binding.clone(), archive());
    for foreign in [
        Binding::new(binding.identity().clone(), [0x33; 32]),
        Binding::new(
            ServiceIdentity::new("example", "https://other.example").unwrap(),
            [0x22; 32],
        ),
    ] {
        let context =
            Context::new(BTreeMap::from([("example".into(), foreign)]), &provider).unwrap();
        assert!(
            context
                .resolve("example", "math", "1.0.0", None)
                .unwrap_err()
                .message
                .contains("does not match")
        );
    }
    let context = Context::new(
        BTreeMap::from([("example".into(), binding.clone())]),
        &provider,
    )
    .unwrap();
    assert!(context.resolve("example", "other", "1.0.0", None).is_err());
    assert!(context.resolve("example", "math", "2.0.0", None).is_err());
    assert!(
        context
            .resolve("example", "math", "1.0.0", Some(&"00".repeat(32)))
            .is_err()
    );
    for flag in ["yanked", "quarantined"] {
        let mut unavailable = IsolatedMemory::new(binding.clone(), archive());
        let bytes = String::from_utf8(unavailable.index.as_bytes().unwrap())
            .unwrap()
            .replace(&format!("\"{flag}\": false"), &format!("\"{flag}\": true"));
        unavailable.index =
            crate::registry::store::Index::admit(bytes.as_bytes(), binding.identity(), "math")
                .unwrap();
        let context = Context::new(
            BTreeMap::from([("example".into(), binding.clone())]),
            &unavailable,
        )
        .unwrap();
        assert!(
            context
                .resolve("example", "math", "1.0.0", None)
                .unwrap_err()
                .message
                .contains("unavailable")
        );
    }
}

#[test]
fn schema_three_preserves_workspace_catalogue_and_selected_local_root() {
    use crate::package::lock::Lock;
    let temp = Temp::new();
    fs::write(
        temp.0.join("Lang.json"),
        br#"{"schema":2,"workspace":{"members":["idle","app"]}}"#,
    )
    .unwrap();
    fs::create_dir_all(temp.0.join("app/src")).unwrap();
    fs::write(temp.0.join("app/Lang.json"), APP).unwrap();
    fs::write(temp.0.join("app/src/main.lang"), MAIN).unwrap();
    fs::create_dir(temp.0.join("idle")).unwrap();
    let idle = r#"{"schema":1,"package":{"name":"idle","version":"1.0.0","edition":"2026","entry":"missing.lang"}}"#;
    fs::write(temp.0.join("idle/Lang.json"), idle).unwrap();
    let directory =
        cap_std::fs::Dir::open_ambient_dir(&temp.0, cap_std::ambient_authority()).unwrap();
    let binding = Binding::new(
        ServiceIdentity::new("example", "https://registry.example").unwrap(),
        [0x22; 32],
    );
    let provider = IsolatedMemory::new(binding.clone(), archive());
    let context = Context::new(BTreeMap::from([("example".into(), binding)]), &provider).unwrap();
    let resolve = || {
        Graph::resolve_with_registries_in(
            directory.try_clone().unwrap(),
            temp.0.clone(),
            Path::new("Lang.json"),
            "aarch64-apple-darwin",
            Limits::default(),
            Some("app"),
            &context,
        )
        .unwrap()
    };
    let graph = resolve();
    let lock = Lock::capture(&graph).unwrap();
    let record: serde_json::Value = serde_json::from_slice(lock.as_bytes()).unwrap();
    assert_eq!(record["root"], "local/app");
    assert_eq!(record["workspace"]["manifest_path"], "Lang.json");
    assert_eq!(record["workspace"]["selected_root"], "app");
    assert_eq!(
        record["workspace"]["members"][0]["manifest_path"],
        "app/Lang.json"
    );
    assert_eq!(
        record["workspace"]["members"][1]["manifest_path"],
        "idle/Lang.json"
    );
    fs::write(
        temp.0.join("idle/Lang.json"),
        idle.replace("1.0.0", "1.0.1"),
    )
    .unwrap();
    let changed = Lock::capture(&resolve()).unwrap();
    assert!(lock.verify(changed.as_bytes()).is_err());
    assert_eq!(Lock::capture(&graph).unwrap().as_bytes(), lock.as_bytes());
    execute(&graph, 42);
}

#[test]
fn schema_three_capture_matches_independent_original_byte_fixture() {
    use crate::package::lock::{Lock, validate_encoding};
    let temp = Temp::new();
    let directory =
        cap_std::fs::Dir::open_ambient_dir(&temp.0, cap_std::ambient_authority()).unwrap();
    fs::write(temp.0.join("Lang.json"), APP).unwrap();
    fs::create_dir(temp.0.join("src")).unwrap();
    fs::write(temp.0.join("src/main.lang"), MAIN).unwrap();
    let binding = Binding::new(
        ServiceIdentity::new("example", "https://registry.example").unwrap(),
        [0x22; 32],
    );
    let provider = IsolatedMemory::new(binding.clone(), archive());
    let context = Context::new(BTreeMap::from([("example".into(), binding)]), &provider).unwrap();
    let graph = Graph::resolve_with_registries_in(
        directory.try_clone().unwrap(),
        temp.0.clone(),
        Path::new("Lang.json"),
        "aarch64-apple-darwin",
        Limits::default(),
        None,
        &context,
    )
    .unwrap();
    let lock = Lock::capture(&graph).unwrap();
    let target = "aarch64-apple-darwin".parse().unwrap();
    let fixture = include_str!("../../../tests/fixtures/package-lock/schema3-registry.json")
        .replace(
            "<compiler_build_id>",
            &lang_codegen_clif::sdk::build_id(&target),
        );
    assert!(
        lock.as_bytes() == fixture.as_bytes(),
        "canonical fixture differs at byte {:?}; lengths {} / {}",
        lock.as_bytes()
            .iter()
            .zip(fixture.as_bytes())
            .position(|(a, b)| a != b),
        lock.as_bytes().len(),
        fixture.len()
    );
    validate_encoding(lock.as_bytes()).unwrap();
    lock.verify(fixture.as_bytes()).unwrap();
    for bad in [
        fixture.replace("  \"workspace\": null,\n", ""),
        fixture.replacen("\"schema\": 3", "\"schema\": 3, \"schema\": 3", 1),
        fixture.replacen(
            "\"kind\": \"registry\"",
            "\"kind\": \"registry\", \"url\": \"https://other.example\"",
            1,
        ),
        fixture.replace("  ", " "),
        fixture.replacen("\"name\": \"math\"", "\"name\": \"other\"", 1),
    ] {
        assert!(validate_encoding(bad.as_bytes()).is_err());
    }
    let changed = fixture.replace("aarch64-apple-darwin", "x86_64-pc-windows-msvc");
    validate_encoding(changed.as_bytes()).unwrap();
    assert!(lock.verify(changed.as_bytes()).is_err());
    let plan = crate::package::format::Plan::capture(&graph).unwrap();
    assert_eq!(plan.changes().len(), 1);
    assert_eq!(plan.changes()[0].path(), Path::new("src/main.lang"));
    let replacement = crate::package::format::Plan::from_replacements(
        &graph,
        [(
            PathBuf::from("local/src/main.lang"),
            plan.changes()[0].text().into(),
        )],
    )
    .unwrap();
    assert_eq!(replacement.changes()[0].path(), Path::new("src/main.lang"));
    plan.apply(&directory).unwrap();
    fs::remove_dir_all(temp.0.join("src")).unwrap();
    assert_eq!(Lock::capture(&graph).unwrap().as_bytes(), lock.as_bytes());
    execute(&graph, 42);
}
