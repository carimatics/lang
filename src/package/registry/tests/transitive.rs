use super::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::cell::RefCell;

fn dependency(alias: &str, archive: &AdmittedArchive) -> Value {
    json!({"alias":alias,"registry":"example","name":archive.descriptor().package().name(),"version":"1.0.0","archive_sha256":archive.sha256(),"targets":["aarch64-apple-darwin","x86_64-pc-windows-msvc"]})
}
fn package(name: &str, source: &str, dependencies: Vec<Value>) -> AdmittedArchive {
    let descriptor: crate::registry::archive::Descriptor = serde_json::from_value(json!({
        "format":"lang-registry-source","schema":1,
        "package":{"name":name,"version":"1.0.0","edition":"2026","entry":"src/lib.lang"},
        "tests":[],"dependencies":dependencies,
        "files":[{"path":"src/lib.lang","bytes":source.len(),"sha256":hex(&Sha256::digest(source.as_bytes()))}]
    })).unwrap();
    let mut bytes = serde_json::to_vec_pretty(&descriptor).unwrap();
    bytes.push(b'\n');
    pack(
        &bytes,
        &BTreeMap::from([("src/lib.lang".into(), source.into())]),
    )
}
fn binding(registry: &str, origin: &str, policy: u8) -> Binding {
    Binding::new(
        ServiceIdentity::new(registry, origin).unwrap(),
        [policy; 32],
    )
}
struct Multi {
    records: BTreeMap<(String, String), IsolatedMemory>,
    reads: RefCell<Vec<String>>,
}
impl Multi {
    fn new(records: Vec<IsolatedMemory>) -> Self {
        Self {
            records: records
                .into_iter()
                .map(|r| {
                    (
                        (
                            r.binding.identity().registry().to_owned(),
                            r.archive.descriptor().package().name().to_owned(),
                        ),
                        r,
                    )
                })
                .collect(),
            reads: RefCell::new(Vec::new()),
        }
    }
    fn bindings(&self) -> BTreeMap<String, Binding> {
        self.records
            .values()
            .map(|r| {
                (
                    r.binding.identity().registry().to_owned(),
                    r.binding.clone(),
                )
            })
            .collect()
    }
}
impl Source for Multi {
    fn resolve(&self, request: Request<'_>) -> Result<ConsumedArchive, Diagnostic> {
        self.reads.borrow_mut().push(format!(
            "{}:{}",
            request.binding.identity().registry(),
            request.name
        ));
        let record = self
            .records
            .get(&(
                request.binding.identity().registry().into(),
                request.name.into(),
            ))
            .ok_or_else(|| refused("isolated release unavailable"))?;
        record.resolve(request)
    }
}
fn app(temp: &Temp, dependencies: Value, source: &str) {
    let mut manifest: Value = serde_json::from_slice(APP).unwrap();
    manifest["dependencies"] = dependencies;
    fs::write(
        temp.0.join("Lang.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    fs::create_dir_all(temp.0.join("src")).unwrap();
    fs::write(temp.0.join("src/main.lang"), source).unwrap();
}
fn edge(registry: &str, name: &str) -> Value {
    json!({"source":"registry","registry":registry,"name":name,"version":"1.0.0"})
}
fn resolve(
    temp: &Temp,
    provider: &Multi,
    target: &str,
    limits: Limits,
) -> Result<Graph, Diagnostic> {
    let directory =
        cap_std::fs::Dir::open_ambient_dir(&temp.0, cap_std::ambient_authority()).unwrap();
    let context = Context::new(provider.bindings(), provider).unwrap();
    Graph::resolve_with_registries_in(
        directory,
        temp.0.clone(),
        Path::new("Lang.json"),
        target,
        limits,
        None,
        &context,
    )
}

#[test]
fn transitive_diamond_executes_with_exact_shared_identity_and_resource_boundaries() {
    const SOURCE: &str = "mod remote; fn main()->i64{return remote::answer();}\n";
    let temp = Temp::new();
    let math = archive();
    let left = package(
        "left",
        "mod math; pub fn answer()->i64{return math::answer();}\n",
        vec![dependency("math", &math)],
    );
    let right = package(
        "right",
        "mod math; pub fn answer()->i64{return math::answer();}\n",
        vec![dependency("math", &math)],
    );
    let top = package(
        "top",
        "mod left; mod right; pub fn answer()->i64{return left::answer()+right::answer();}\n",
        vec![dependency("left", &left), dependency("right", &right)],
    );
    let binding = binding("example", "https://registry.example", 0x22);
    let archives = [math, left, right, top];
    let source_bytes = SOURCE.len()
        + archives
            .iter()
            .flat_map(|a| a.descriptor().files())
            .map(|f| f.bytes() as usize)
            .sum::<usize>();
    let provider = Multi::new(
        archives
            .into_iter()
            .map(|a| IsolatedMemory::new(binding.clone(), a))
            .collect(),
    );
    app(&temp, json!({"remote":edge("example", "top")}), SOURCE);
    let limits = Limits {
        packages: 5,
        edges: 5,
        source_files: 6,
        source_bytes,
        ..Limits::default()
    };
    let graph = resolve(&temp, &provider, "aarch64-apple-darwin", limits).unwrap();
    assert_eq!(
        *provider.reads.borrow(),
        [
            "example:top",
            "example:left",
            "example:math",
            "example:right"
        ]
    );
    assert_eq!(graph.packages().len(), 5);
    assert_eq!(graph.inputs().unwrap().files.len(), 6);
    let left = graph
        .packages()
        .iter()
        .find(|p| p.name() == "left")
        .unwrap();
    let right = graph
        .packages()
        .iter()
        .find(|p| p.name() == "right")
        .unwrap();
    assert_eq!(left.dependencies()["math"], right.dependencies()["math"]);
    crate::package::lock::Lock::capture(&graph).unwrap();
    for limited in [
        Limits {
            packages: 4,
            ..limits
        },
        Limits { edges: 4, ..limits },
        Limits {
            source_files: 5,
            ..limits
        },
        Limits {
            source_bytes: source_bytes - 1,
            ..limits
        },
    ] {
        assert_eq!(
            resolve(&temp, &provider, "aarch64-apple-darwin", limited)
                .unwrap_err()
                .code,
            "PKG0004"
        );
    }
    fs::remove_dir_all(temp.0.join("src")).unwrap();
    execute(&graph, 84);
}

#[test]
fn pinned_transitive_content_and_inactive_host_bindings_fail_closed() {
    let temp = Temp::new();
    let math = archive();
    let mut bad = dependency("math", &math);
    bad["archive_sha256"] = json!("00".repeat(32));
    let wrong = package(
        "wrong",
        "mod math; pub fn answer()->i64{return math::answer();}\n",
        vec![bad],
    );
    let mut inactive = dependency("windows", &math);
    inactive["registry"] = json!("unbound");
    inactive["targets"] = json!(["x86_64-pc-windows-msvc"]);
    let conditional = package(
        "conditional",
        "mod math; pub fn answer()->i64{return math::answer();}\n",
        vec![dependency("math", &math), inactive],
    );
    let binding = binding("example", "https://registry.example", 0x22);
    let provider = Multi::new(
        [math, wrong, conditional]
            .into_iter()
            .map(|a| IsolatedMemory::new(binding.clone(), a))
            .collect(),
    );
    app(
        &temp,
        json!({"remote":edge("example", "wrong")}),
        "mod remote; fn main()->i64{return remote::answer();}\n",
    );
    let failure = resolve(&temp, &provider, "aarch64-apple-darwin", Limits::default()).unwrap_err();
    assert_eq!(failure.code, "PKG0006");
    assert!(failure.message.contains("does not match"));
    app(
        &temp,
        json!({"a_good":edge("example", "math"),"z_wrong":edge("example", "wrong")}),
        "fn main()->i64{return 0;}\n",
    );
    let conflict =
        resolve(&temp, &provider, "aarch64-apple-darwin", Limits::default()).unwrap_err();
    assert_eq!(conflict.code, "PKG0006");
    assert!(conflict.message.contains("conflicting archive identities"));
    app(
        &temp,
        json!({"remote":edge("example", "conditional")}),
        "mod remote; fn main()->i64{return remote::answer();}\n",
    );
    let graph = resolve(&temp, &provider, "aarch64-apple-darwin", Limits::default()).unwrap();
    execute(&graph, 42);
    let failure = resolve(
        &temp,
        &provider,
        "x86_64-pc-windows-msvc",
        Limits::default(),
    )
    .unwrap_err();
    assert_eq!(failure.code, "PKG0006");
    assert!(failure.message.contains("no explicit host binding"));
}

#[test]
fn identical_bytes_under_independent_origin_policies_remain_distinct() {
    let temp = Temp::new();
    let provider = Multi::new(vec![
        IsolatedMemory::new(
            binding("example", "https://registry.example", 0x22),
            archive(),
        ),
        IsolatedMemory::new(binding("other", "https://other.example", 0x33), archive()),
    ]);
    app(
        &temp,
        json!({"left":edge("example", "math"),"right":edge("other", "math")}),
        "mod left; mod right; fn main()->i64{return left::answer()+right::answer();}\n",
    );
    let graph = resolve(&temp, &provider, "aarch64-apple-darwin", Limits::default()).unwrap();
    let dependencies = graph.package(graph.root()).unwrap().dependencies();
    assert_ne!(dependencies["left"], dependencies["right"]);
    let left = graph.package(dependencies["left"]).unwrap();
    let right = graph.package(dependencies["right"]).unwrap();
    assert_ne!(left.root(), right.root());
    assert_eq!(
        left.registry().unwrap().archive().sha256(),
        right.registry().unwrap().archive().sha256()
    );
    crate::package::lock::Lock::capture(&graph).unwrap();
    execute(&graph, 84);
}
