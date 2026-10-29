//! RFC 0034 locks derived solely from frozen package and workspace inputs.
//! Checking and publication require an explicit, separately retained directory
//! capability; a diagnostic display path never grants output authority.
use super::{
    Diagnostic,
    graph::{self, Graph},
    source::inventory::BindingKind,
};
use cap_std::fs::{Dir, OpenOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_BYTES: usize = 1024 * 1024;
pub const NAME: &str = "Lang.lock";

#[derive(Debug)]
pub struct Lock {
    bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    schema: u32,
    target: String,
    compiler_version: String,
    compiler_build_id: String,
    packages: Vec<Package>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceRecord {
    schema: u32,
    target: String,
    compiler_version: String,
    compiler_build_id: String,
    workspace: Workspace,
    packages: Vec<Package>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Workspace {
    manifest_path: String,
    manifest_sha256: String,
    members: Vec<Member>,
    selected_root: String,
    checksum: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    path: String,
    root: String,
    manifest_path: String,
    manifest_sha256: String,
    name: String,
    version: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    root: String,
    name: String,
    version: String,
    edition: String,
    manifest_path: String,
    manifest_sha256: String,
    sources: Vec<Source>,
    dependencies: Vec<Edge>,
    bindings: Vec<Binding>,
    checksum: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    path: String,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Edge {
    alias: String,
    root: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    kind: BindingKind,
    scope: String,
    path: String,
    target: String,
}
fn error(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("PKG0007", message).at_path(Path::new(NAME))
}
fn filesystem(path: &Path, e: io::Error) -> Diagnostic {
    Diagnostic::error("PKG0005", e.to_string()).at_path(path)
}
fn path(path: &Path) -> Result<String, Diagnostic> {
    if path.as_os_str().is_empty() || path == Path::new(".") {
        return Ok(".".into());
    }
    let value = path
        .to_str()
        .ok_or_else(|| error("lock paths must be UTF-8"))?
        .replace(std::path::MAIN_SEPARATOR, "/");
    if value.contains(['\\', ':', '\0'])
        || !path.components().all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(error(
            "lock paths must be normalized authority-relative paths",
        ));
    }
    Ok(value)
}
fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
fn checksum(files: &BTreeMap<String, &[u8]>, bindings: &[Binding]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"LANGPACKAGE1\0");
    hash.update((files.len() as u64).to_le_bytes());
    for (path, bytes) in files {
        field(&mut hash, path.as_bytes());
        field(&mut hash, bytes);
    }
    hash.update((bindings.len() as u64).to_le_bytes());
    for binding in bindings {
        for text in [
            binding.scope.as_str(),
            binding.kind.as_str(),
            binding.path.as_str(),
            binding.target.as_str(),
        ] {
            field(&mut hash, text.as_bytes());
        }
    }
    hex(&hash.finalize())
}
fn workspace_record(workspace: &graph::WorkspaceSnapshot) -> Result<Workspace, Diagnostic> {
    let manifest_path = path(workspace.manifest_path())?;
    let mut files = BTreeMap::from([(manifest_path.clone(), workspace.manifest_bytes())]);
    let mut members = Vec::new();
    for member in workspace.members() {
        let manifest_path = path(member.manifest_path())?;
        files.insert(manifest_path.clone(), member.manifest_bytes());
        members.push(Member {
            path: member.path().into(),
            root: path(member.root())?,
            manifest_path,
            manifest_sha256: digest(member.manifest_bytes()),
            name: member.package().name.clone(),
            version: member.package().version.clone(),
        });
    }
    let selected_root = path(workspace.selected_root())?;
    let mut hash = Sha256::new();
    hash.update(b"LANGWORKSPACE1\0");
    hash.update((files.len() as u64).to_le_bytes());
    for (path, bytes) in files {
        field(&mut hash, path.as_bytes());
        field(&mut hash, bytes);
    }
    hash.update((members.len() as u64).to_le_bytes());
    for member in &members {
        for text in [
            &member.path,
            &member.root,
            &member.manifest_path,
            &member.name,
            &member.version,
        ] {
            field(&mut hash, text.as_bytes());
        }
    }
    field(&mut hash, selected_root.as_bytes());
    Ok(Workspace {
        manifest_path,
        manifest_sha256: digest(workspace.manifest_bytes()),
        members,
        selected_root,
        checksum: hex(&hash.finalize()),
    })
}
impl Lock {
    /// Capture and validate every bounded input before lock verification.
    /// This never reads the previous lock, publishes output or executes code.
    pub fn capture(graph: &Graph) -> Result<Self, Diagnostic> {
        let inputs = graph.inputs()?;
        let mut identities = BTreeMap::new();
        for (key, file) in &inputs.files {
            if let Some(previous) = identities.insert(file.identity, key) {
                return Err(error(format!(
                    "duplicate physical source inputs: {} and {}",
                    previous.display(),
                    key.display()
                )));
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
                keys.extend(scope.files.iter());
                for (alias, target) in &scope.aliases {
                    bindings.push(Binding {
                        kind: BindingKind::Directory,
                        scope: path(root)?,
                        path: path(alias)?,
                        target: path(target)?,
                    });
                }
            }
            bindings.sort_by(|a, b| {
                (&a.scope, a.kind.as_str(), &a.path, &a.target).cmp(&(
                    &b.scope,
                    b.kind.as_str(),
                    &b.path,
                    &b.target,
                ))
            });
            let mut files: BTreeMap<_, &[u8]> = BTreeMap::new();
            files.insert(path(snapshot.manifest_path())?, snapshot.manifest_bytes());
            let mut sources = Vec::new();
            for key in keys {
                let file = &inputs.files[key];
                let key = path(key)?;
                if files.insert(key.clone(), &file.bytes).is_some() {
                    return Err(error("duplicate lock input path"));
                }
                sources.push(Source {
                    path: key,
                    sha256: digest(&file.bytes),
                });
            }
            sources.sort_by(|a, b| a.path.cmp(&b.path));
            let mut dependencies = Vec::new();
            for (alias, id) in snapshot.dependencies() {
                dependencies.push(Edge {
                    alias: alias.clone(),
                    root: path(graph.package(*id).expect("resolved dependency").root())?,
                });
            }
            let identity = snapshot
                .manifest()
                .package
                .as_ref()
                .expect("resolved package");
            packages.push(Package {
                root: path(snapshot.root())?,
                name: identity.name.clone(),
                version: identity.version.clone(),
                edition: identity.edition.clone(),
                manifest_path: path(snapshot.manifest_path())?,
                manifest_sha256: digest(snapshot.manifest_bytes()),
                sources,
                dependencies,
                checksum: checksum(&files, &bindings),
                bindings,
            });
        }
        packages.sort_by(|a, b| a.root.cmp(&b.root));
        let target = graph
            .target()
            .parse()
            .map_err(|_| error("invalid compiler target"))?;
        let mut output = Bytes(Vec::new());
        let compiler_build_id = lang_codegen_clif::sdk::build_id(&target);
        if let Some(workspace) = graph.workspace() {
            let record = WorkspaceRecord {
                schema: 2,
                target: graph.target().into(),
                compiler_version: env!("CARGO_PKG_VERSION").into(),
                compiler_build_id,
                workspace: workspace_record(workspace)?,
                packages,
            };
            serde_json::to_writer_pretty(&mut output, &record).map_err(|_| capacity())?;
        } else {
            let record = Record {
                schema: 1,
                target: graph.target().into(),
                compiler_version: env!("CARGO_PKG_VERSION").into(),
                compiler_build_id,
                packages,
            };
            serde_json::to_writer_pretty(&mut output, &record).map_err(|_| capacity())?;
        }
        output.write_all(b"\n").map_err(|_| capacity())?;
        Ok(Self { bytes: output.0 })
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Strict schema admission followed by exact canonical replay comparison.
    pub fn verify(&self, bytes: &[u8]) -> Result<(), Diagnostic> {
        validate_encoding(bytes)?;
        if bytes != self.bytes {
            return Err(error(
                "lock does not match canonical frozen inputs, target and compiler",
            ));
        }
        Ok(())
    }
    /// Locked/offline replay: never creates or rewrites an existing lock.
    pub fn check(&self, directory: &Dir) -> Result<(), Diagnostic> {
        let input =
            graph::read(directory, Path::new(""), Path::new(NAME), MAX_BYTES).map_err(|e| {
                if e.code == "PKG0004" {
                    e
                } else {
                    error(format!("lock unavailable: {}", e.message))
                }
            })?;
        self.verify(&input.bytes)
    }
    /// Publish only after the caller's complete operation has succeeded.
    /// The separately supplied output capability survives source capture.
    pub fn publish(&self, directory: &Dir) -> Result<(), Diagnostic> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let name = PathBuf::from(format!(
                ".Lang.lock.stage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut file =
                match directory.open_with(&name, OpenOptions::new().write(true).create_new(true)) {
                    Ok(file) => file,
                    Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(filesystem(&name, e)),
                };
            let stage = Stage { directory, name };
            let written = file.write_all(&self.bytes).and_then(|_| file.sync_all());
            drop(file);
            written.map_err(|e| filesystem(&stage.name, e))?;
            directory
                .rename(&stage.name, directory, NAME)
                .map_err(|e| filesystem(Path::new(NAME), e))?;
            return Ok(());
        }
        Err(capacity())
    }
}
/// Check bounded wire shape and schema only, without I/O or execution.
/// This does not verify checksums, bindings or compiler/source identity;
/// replay must use `Lock::verify` or `Lock::check` against captured inputs.
pub fn validate_encoding(bytes: &[u8]) -> Result<(), Diagnostic> {
    if bytes.len() > MAX_BYTES {
        return Err(capacity());
    }
    #[derive(Deserialize)]
    struct Header {
        schema: u32,
    }
    let invalid = |e| error(format!("invalid lock: {e}"));
    let header: Header = serde_json::from_slice(bytes).map_err(invalid)?;
    match header.schema {
        1 => {
            let _: Record = serde_json::from_slice(bytes).map_err(invalid)?;
        }
        2 => {
            let _: WorkspaceRecord = serde_json::from_slice(bytes).map_err(invalid)?;
        }
        _ => {
            return Err(
                Diagnostic::error("PKG0002", "unsupported lock schema").at_path(Path::new(NAME))
            );
        }
    }
    Ok(())
}
fn capacity() -> Diagnostic {
    Diagnostic::error("PKG0004", "lock byte or publication capacity exceeded")
        .at_path(Path::new(NAME))
}
struct Bytes(Vec<u8>);
impl Write for Bytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("lock byte limit"));
        }
        self.0.try_reserve(bytes.len()).map_err(io::Error::other)?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
struct Stage<'a> {
    directory: &'a Dir,
    name: PathBuf,
}
impl Drop for Stage<'_> {
    fn drop(&mut self) {
        let _ = self.directory.remove_file(&self.name);
    }
}
