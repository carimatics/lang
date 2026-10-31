//! RFC 0049 canonical hybrid graph capture. Encoding is not trust authority.
use super::*;
use crate::package::{manifest, registry::Binding as Policy};
use crate::registry::authority::ServiceIdentity;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    target: String,
    compiler_version: String,
    compiler_build_id: String,
    root: String,
    #[serde(deserialize_with = "workspace")]
    workspace: Option<Workspace>,
    packages: Vec<Package>,
}
fn workspace<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Workspace>, D::Error> {
    Option::<Workspace>::deserialize(d)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    id: String,
    origin: Origin,
    name: String,
    version: String,
    edition: String,
    metadata: Metadata,
    sources: Vec<Source>,
    dependencies: Vec<Edge>,
    bindings: Vec<Binding>,
    #[serde(skip_serializing_if = "String::is_empty")]
    checksum: String,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
enum Origin {
    Local {
        root: String,
    },
    Registry {
        registry: String,
        origin: String,
        root_sha256: String,
        archive_sha256: String,
        archive_bytes: u64,
        descriptor_sha256: String,
    },
}
#[derive(Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
enum MetadataKind {
    Manifest,
    Descriptor,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    kind: MetadataKind,
    path: String,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Edge {
    alias: String,
    id: String,
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>, Diagnostic> {
    let mut output = Bytes(Vec::new());
    serde_json::to_writer_pretty(&mut output, value).map_err(|_| capacity())?;
    output.write_all(b"\n").map_err(|_| capacity())?;
    Ok(output.0)
}
fn checksum(package: &mut Package) -> Result<String, Diagnostic> {
    let previous = std::mem::take(&mut package.checksum);
    let bytes = encode(package);
    package.checksum = previous;
    let mut hash = Sha256::new();
    hash.update(b"LANGPACKAGE3\0");
    hash.update(bytes?);
    Ok(hex(&hash.finalize()))
}

pub(super) fn capture(graph: &Graph) -> Result<Lock, Diagnostic> {
    let inputs = graph.inputs()?;
    let mut identities = BTreeSet::new();
    if inputs
        .files
        .values()
        .any(|f| !identities.insert(f.identity))
    {
        return Err(error("duplicate physical source inputs"));
    }
    for (path, metadata) in graph.manifest_inputs() {
        let identity = super::super::source::identity(metadata, path)
            .map_err(|_| error("invalid local metadata identity"))?;
        if identities.contains(&identity) {
            return Err(error("source shares a manifest identity"));
        }
    }
    let mut packages = Vec::new();
    for snapshot in graph.packages() {
        let declared = &inputs.declared[&snapshot.id()];
        let scopes: BTreeSet<_> = declared.iter().map(|d| &d.scope).collect();
        let mut keys = BTreeSet::new();
        let mut bindings = Vec::new();
        for d in declared {
            bindings.push(Binding {
                kind: d.kind,
                scope: path(&d.scope)?,
                path: path(&d.path)?,
                target: path(&d.target)?,
            });
        }
        for root in scopes {
            let scope = &inputs.scopes[root];
            keys.extend(scope.files.iter().cloned());
            for (alias, target) in &scope.aliases {
                bindings.push(Binding {
                    kind: BindingKind::Directory,
                    scope: path(root)?,
                    path: path(alias)?,
                    target: path(target)?,
                });
            }
        }
        let (origin, kind) = if let Some(registry) = snapshot.registry() {
            let archive = registry.archive();
            // Even a source outside the entry/test scopes remains archive input.
            keys.extend(
                archive
                    .descriptor()
                    .files()
                    .iter()
                    .map(|f| snapshot.root().join(f.path())),
            );
            (
                Origin::Registry {
                    registry: registry.binding().identity().registry().into(),
                    origin: registry.binding().identity().origin().into(),
                    root_sha256: hex(registry.binding().root_sha256()),
                    archive_sha256: archive.sha256().into(),
                    archive_bytes: archive.as_bytes().len() as u64,
                    descriptor_sha256: archive.descriptor_sha256().into(),
                },
                MetadataKind::Descriptor,
            )
        } else {
            (
                Origin::Local {
                    root: path(&snapshot.local().expect("typed origin").root)?,
                },
                MetadataKind::Manifest,
            )
        };
        bindings.sort_by(|a, b| binding_key(a).cmp(&binding_key(b)));
        let mut sources = keys
            .iter()
            .map(|key| {
                Ok(Source {
                    path: path(key)?,
                    sha256: digest(&inputs.files[key].bytes),
                })
            })
            .collect::<Result<Vec<_>, Diagnostic>>()?;
        sources.sort_by(|a, b| a.path.cmp(&b.path));
        let dependencies = snapshot
            .dependencies()
            .iter()
            .map(|(alias, id)| {
                Ok(Edge {
                    alias: alias.clone(),
                    id: path(graph.package(*id).expect("resolved dependency").root())?,
                })
            })
            .collect::<Result<Vec<_>, Diagnostic>>()?;
        let mut package = Package {
            id: path(snapshot.root())?,
            origin,
            name: snapshot.name().into(),
            version: snapshot.version().into(),
            edition: snapshot.edition().into(),
            metadata: Metadata {
                kind,
                path: path(snapshot.manifest_path())?,
                sha256: digest(snapshot.manifest_bytes()),
            },
            sources,
            dependencies,
            bindings,
            checksum: String::new(),
        };
        package.checksum = checksum(&mut package)?;
        packages.push(package);
    }
    packages.sort_by(|a, b| a.id.cmp(&b.id));
    let target = graph
        .target()
        .parse()
        .map_err(|_| error("invalid compiler target"))?;
    let record = Record {
        schema: 3,
        target: graph.target().into(),
        compiler_version: env!("CARGO_PKG_VERSION").into(),
        compiler_build_id: lang_codegen_clif::sdk::build_id(&target),
        root: path(graph.package(graph.root()).expect("selected root").root())?,
        workspace: graph.workspace().map(workspace_record).transpose()?,
        packages,
    };
    let bytes = encode(&record)?;
    validate(&bytes)?;
    Ok(Lock { bytes })
}

fn binding_key(binding: &Binding) -> (&str, &str, &str, &str) {
    (
        &binding.scope,
        binding.kind.as_str(),
        &binding.path,
        &binding.target,
    )
}
fn normalized(value: &str, allow_root: bool) -> bool {
    if allow_root && value == "." {
        return true;
    }
    !value.is_empty()
        && value.len() <= 8192
        && !value.contains(['\\', ':', '\0'])
        && value
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn within(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|tail| tail.starts_with('/'))
}
fn depth(bytes: &[u8]) -> Result<(), Diagnostic> {
    let (mut depth, mut string, mut escape) = (0usize, false, false);
    for &byte in bytes {
        if string {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > 16 {
                        return Err(capacity());
                    }
                }
                b'}' | b']' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
    Ok(())
}

pub(super) fn validate(bytes: &[u8]) -> Result<(), Diagnostic> {
    depth(bytes)?;
    let mut record: Record =
        serde_json::from_slice(bytes).map_err(|_| error("invalid closed schema-3 lock"))?;
    if encode(&record)? != bytes {
        return Err(error("schema-3 lock is not canonical"));
    }
    if record.schema != 3
        || !manifest::TARGETS.contains(&record.target.as_str())
        || !manifest::valid_version(&record.compiler_version)
        || !hash(&record.compiler_build_id)
        || record.packages.is_empty()
        || record.packages.len() > 256
    {
        return Err(error("invalid schema-3 graph identity"));
    }
    if !record.packages.windows(2).all(|p| p[0].id < p[1].id) {
        return Err(error("schema-3 package order or identity conflict"));
    }
    let mut has_registry = false;
    let mut registry_releases = BTreeMap::new();
    let mut source_inputs = BTreeMap::new();
    for package in &mut record.packages {
        if !normalized(&package.id, false)
            || !manifest::valid_name(&package.name)
            || !manifest::valid_version(&package.version)
            || super::super::compatibility::SourceEdition::parse(&package.edition).is_err()
            || !hash(&package.metadata.sha256)
            || !hash(&package.checksum)
            || checksum(package)? != package.checksum
        {
            return Err(error("invalid schema-3 package identity or checksum"));
        }
        let namespace = match &package.origin {
            Origin::Local { root } => {
                if !normalized(root, true)
                    || package.id != path(&graph::local_key(Path::new(root)))?
                    || package.metadata.kind != MetadataKind::Manifest
                    || Path::new(&package.metadata.path).parent() != Some(Path::new(&package.id))
                {
                    return Err(error("invalid local origin binding"));
                }
                "local"
            }
            Origin::Registry {
                registry,
                origin,
                root_sha256,
                archive_sha256,
                archive_bytes,
                descriptor_sha256,
            } => {
                has_registry = true;
                if !hash(root_sha256)
                    || !hash(archive_sha256)
                    || !hash(descriptor_sha256)
                    || *archive_bytes == 0
                    || *archive_bytes > crate::registry::archive::MAX_ARCHIVE_BYTES as u64
                {
                    return Err(error("invalid registry origin fields"));
                }
                let mut root = [0; 32];
                for (byte, pair) in root
                    .iter_mut()
                    .zip(root_sha256.as_bytes().as_chunks::<2>().0)
                {
                    *byte = u8::from_str_radix(std::str::from_utf8(pair).expect("ASCII hash"), 16)
                        .expect("validated hash");
                }
                let identity = ServiceIdentity::new(registry, origin)
                    .map_err(|_| error("invalid registry service identity"))?;
                let binding = Policy::new(identity, root);
                if registry_releases
                    .insert(
                        (
                            binding.policy_id().to_owned(),
                            package.name.clone(),
                            package.version.clone(),
                        ),
                        archive_sha256.clone(),
                    )
                    .is_some_and(|previous| previous != *archive_sha256)
                {
                    return Err(error("conflicting schema-3 registry release identities"));
                }
                let expected = format!(
                    "registry/{}/{}/{}/{}",
                    binding.policy_id(),
                    package.name,
                    package.version,
                    archive_sha256
                );
                if package.id != expected
                    || package.metadata.kind != MetadataKind::Descriptor
                    || package.metadata.path != format!("{expected}/.lang-package.json")
                    || package.metadata.sha256 != *descriptor_sha256
                {
                    return Err(error("invalid registry metadata binding"));
                }
                package.id.as_str()
            }
        };
        if !normalized(&package.metadata.path, false)
            || package.sources.is_empty()
            || !package.sources.windows(2).all(|s| s[0].path < s[1].path)
            || !package
                .dependencies
                .windows(2)
                .all(|d| d[0].alias < d[1].alias)
            || !package
                .bindings
                .windows(2)
                .all(|b| binding_key(&b[0]) < binding_key(&b[1]))
        {
            return Err(error("invalid schema-3 input order"));
        }
        for source in &package.sources {
            if !normalized(&source.path, false)
                || !within(&source.path, namespace)
                || !source.path.ends_with(".lang")
                || !hash(&source.sha256)
            {
                return Err(error("invalid schema-3 source binding"));
            }
            if source_inputs
                .insert(source.path.clone(), source.sha256.clone())
                .is_some_and(|previous| previous != source.sha256)
                || source_inputs.len() > 256
            {
                return Err(error("conflicting or excessive schema-3 source inputs"));
            }
        }
        let mut entries = 0;
        for binding in &package.bindings {
            if [&binding.scope, &binding.path, &binding.target]
                .iter()
                .any(|p| !normalized(p, false) || !within(p, namespace))
                || !within(&binding.target, &binding.scope)
                || !package
                    .sources
                    .iter()
                    .any(|s| within(&s.path, &binding.scope))
            {
                return Err(error("invalid schema-3 scope binding"));
            }
            if binding.kind == BindingKind::Entry {
                entries += 1;
            }
            if binding.kind != BindingKind::Directory
                && !package.sources.iter().any(|s| s.path == binding.target)
            {
                return Err(error("missing schema-3 source target"));
            }
            if matches!(package.origin, Origin::Registry { .. })
                && (binding.kind == BindingKind::Directory
                    || binding.path != binding.target
                    || Path::new(&binding.path).parent() != Some(Path::new(&binding.scope)))
            {
                return Err(error("registry source binding is not immutable"));
            }
        }
        if entries != 1 {
            return Err(error("schema-3 package requires one entry binding"));
        }
    }
    let packages: BTreeMap<_, _> = record.packages.iter().map(|p| (p.id.as_str(), p)).collect();
    let root = packages
        .get(record.root.as_str())
        .ok_or_else(|| error("missing schema-3 root"))?;
    if !has_registry || !matches!(root.origin, Origin::Local { .. }) {
        return Err(error("schema-3 requires local root and registry input"));
    }
    let mut edges = 0;
    for package in &record.packages {
        for edge in &package.dependencies {
            edges += 1;
            let target = packages
                .get(edge.id.as_str())
                .ok_or_else(|| error("missing schema-3 dependency"))?;
            if edges > 4096
                || !manifest::valid_name(&edge.alias)
                || matches!(target.origin, Origin::Local { .. })
                    && (matches!(package.origin, Origin::Registry { .. })
                        || edge.alias != target.name)
            {
                return Err(error("invalid schema-3 dependency binding"));
            }
        }
    }
    let mut visited = BTreeMap::new();
    let mut pending = vec![(record.root.as_str(), false)];
    while let Some((id, exiting)) = pending.pop() {
        if exiting {
            visited.insert(id, false);
            continue;
        }
        match visited.get(id) {
            Some(true) => return Err(error("schema-3 dependency cycle")),
            Some(false) => continue,
            None => {}
        }
        visited.insert(id, true);
        pending.push((id, true));
        pending.extend(
            packages[id]
                .dependencies
                .iter()
                .rev()
                .map(|d| (d.id.as_str(), false)),
        );
    }
    if visited.len() != packages.len() {
        return Err(error("unreachable schema-3 package"));
    }
    if let Some(workspace) = &record.workspace {
        if !normalized(&workspace.manifest_path, false)
            || !hash(&workspace.manifest_sha256)
            || !hash(&workspace.checksum)
            || workspace.members.is_empty()
            || workspace.members.len() > manifest::MAX_ENTRIES
            || !workspace.members.windows(2).all(|m| m[0].path < m[1].path)
            || path(&graph::local_key(Path::new(&workspace.selected_root)))? != record.root
        {
            return Err(error("invalid schema-3 workspace selection"));
        }
        let mut roots = BTreeSet::new();
        for member in &workspace.members {
            if !normalized(&member.path, true)
                || !normalized(&member.root, true)
                || !normalized(&member.manifest_path, false)
                || !hash(&member.manifest_sha256)
                || !manifest::valid_name(&member.name)
                || !manifest::valid_version(&member.version)
                || !roots.insert(&member.root)
            {
                return Err(error("invalid schema-3 workspace member"));
            }
        }
        if !roots.contains(&workspace.selected_root) {
            return Err(error("schema-3 selection is not a member"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Record {
        let bytes = include_str!("../../../tests/fixtures/package-lock/schema3-registry.json")
            .replace("<compiler_build_id>", &"0".repeat(64));
        serde_json::from_str(&bytes).unwrap()
    }
    fn reseal(record: &mut Record) {
        for package in &mut record.packages {
            package.checksum = checksum(package).unwrap();
        }
    }
    fn refuses(mut record: Record, message: &str) {
        reseal(&mut record);
        let failure = validate(&encode(&record).unwrap()).unwrap_err();
        assert!(failure.message.contains(message), "{}", failure.message);
    }

    #[test]
    fn recomputed_checksums_do_not_authorize_invalid_references_or_origins() {
        validate(&encode(&fixture()).unwrap()).unwrap();
        let mut missing = fixture();
        missing.packages[0].dependencies[0].id = "missing".into();
        refuses(missing, "missing schema-3 dependency");
        let mut cycle = fixture();
        cycle.packages[0].dependencies.insert(
            0,
            Edge {
                alias: "app".into(),
                id: "local".into(),
            },
        );
        refuses(cycle, "dependency cycle");
        let mut disconnected = fixture();
        disconnected.packages[0].dependencies.clear();
        refuses(disconnected, "unreachable");
        let mut local_from_registry = fixture();
        local_from_registry.packages[1].dependencies.push(Edge {
            alias: "app".into(),
            id: "local".into(),
        });
        refuses(local_from_registry, "dependency binding");
        let mut policy = fixture();
        let Origin::Registry { root_sha256, .. } = &mut policy.packages[1].origin else {
            panic!()
        };
        *root_sha256 = "33".repeat(32);
        refuses(policy, "metadata binding");
        let mut foreign_source = fixture();
        foreign_source.packages[1].sources[0].path = "local/stolen.lang".into();
        refuses(foreign_source, "source binding");
        let mut missing_source = fixture();
        missing_source.packages[1].bindings[0].target += ".missing";
        refuses(missing_source, "source target");
        let mut bad_alias = fixture();
        bad_alias.packages[1].bindings[0].path += ".alias";
        refuses(bad_alias, "not immutable");
        let mut root = fixture();
        root.packages[0].id += "/";
        refuses(root, "package identity");
    }

    #[test]
    fn schema_three_canonical_fields_depth_and_size_are_bounded() {
        let canonical = String::from_utf8(encode(&fixture()).unwrap()).unwrap();
        for bad in [
            canonical.replace("  \"workspace\": null,\n", ""),
            canonical.replacen(
                "\"kind\": \"local\"",
                "\"kind\": \"local\", \"kind\": \"local\"",
                1,
            ),
            canonical.replacen("\"root\": \".\"", "\"root\": \".\", \"path\": \".\"", 1),
            canonical.trim_end().into(),
        ] {
            assert!(validate(bad.as_bytes()).is_err());
        }
        let nested = format!("{}0{}", "[".repeat(17), "]".repeat(17));
        assert_eq!(validate(nested.as_bytes()).unwrap_err().code, "PKG0004");
        assert_eq!(
            validate_encoding(&vec![b' '; MAX_BYTES + 1])
                .unwrap_err()
                .code,
            "PKG0004"
        );
        let mut checksum = fixture();
        checksum.packages[0].checksum = "00".repeat(32);
        assert!(
            validate(&encode(&checksum).unwrap())
                .unwrap_err()
                .message
                .contains("checksum")
        );
    }
}
