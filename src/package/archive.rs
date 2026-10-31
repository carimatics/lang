//! RFC 0035 plain-file snapshots from the already frozen package inventory.
use super::{
    Diagnostic,
    graph::{Graph, input_path},
    lock::{self, Lock},
    manifest::DependencySource,
    output::MAX_OUTPUT,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::{self, Cursor, Write},
    path::{Component, Path, PathBuf},
};

pub const MAX_FILES: usize = 643;
const DESCRIPTOR: &str = ".lang-snapshot.json";
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Manifest,
    Source,
    Lock,
    Descriptor,
}
#[derive(Debug, Serialize)]
pub struct Entry {
    pub path: String,
    pub kind: Kind,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Debug)]
pub struct Archive {
    entries: Vec<Entry>,
    bytes: Vec<u8>,
}
fn invalid(path: &Path, message: &str) -> Diagnostic {
    Diagnostic::error("PKG0003", message).at_path(path)
}
fn capacity() -> Diagnostic {
    Diagnostic::error("PKG0004", "source archive capacity exceeded")
}
fn normal(path: &Path) -> Option<PathBuf> {
    let mut result = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(p) => result.push(p),
            Component::CurDir => {}
            Component::ParentDir if result.pop() => {}
            _ => return None,
        }
    }
    Some(result)
}
pub(super) fn canonical_directories(graph: &Graph) -> Result<(), Diagnostic> {
    let inputs = graph.inputs()?;
    for scope in inputs.scopes.values() {
        if let Some(path) = scope.aliases.keys().next() {
            return Err(invalid(
                path,
                "source tooling requires canonical directories",
            ));
        }
    }
    for entries in inputs.declared.values() {
        for entry in entries {
            if normal(&entry.path) != normal(&entry.target) {
                return Err(invalid(
                    &entry.path,
                    "source entry/test directory must be canonical",
                ));
            }
        }
    }
    if let Some(workspace) = graph.workspace() {
        for member in workspace.members() {
            if normal(Path::new(member.path())) != normal(member.root()) {
                return Err(invalid(
                    Path::new(member.path()),
                    "source member directory must be canonical",
                ));
            }
        }
    }
    for package in graph.packages() {
        for (alias, id) in package.dependencies() {
            let DependencySource::Local { path } = &package.manifest().dependencies[alias].source
            else {
                return Err(invalid(
                    package.manifest_path(),
                    "source snapshots require local dependencies",
                ));
            };
            let declared = input_path(package.root(), Path::new(path));
            let target = graph.package(*id).expect("resolved dependency").root();
            if normal(&declared) != normal(target) {
                return Err(invalid(
                    &declared,
                    "source dependency directory must be canonical",
                ));
            }
        }
    }
    Ok(())
}
#[derive(Serialize)]
struct Descriptor<'a> {
    format: &'static str,
    schema: u32,
    target: &'a str,
    compiler_build_id: String,
    lock_sha256: String,
}
impl Archive {
    pub fn capture(graph: &Graph) -> Result<Self, Diagnostic> {
        let lock = Lock::capture(graph)?;
        Self::capture_locked(graph, &lock)
    }
    pub(super) fn capture_locked(graph: &Graph, expected: &Lock) -> Result<Self, Diagnostic> {
        canonical_directories(graph)?;
        let mut files: BTreeMap<String, (Kind, &[u8])> = BTreeMap::new();
        for (path, bytes) in graph.manifest_files() {
            let path = lock::path(path)?;
            if let Some((kind, previous)) = files.insert(path.clone(), (Kind::Manifest, bytes))
                && (kind != Kind::Manifest || previous != bytes)
            {
                return Err(invalid(Path::new(&path), "conflicting archive metadata"));
            }
        }
        for (path, file) in &graph.inputs()?.files {
            let path = lock::path(path)?;
            if files
                .insert(path.clone(), (Kind::Source, &file.bytes))
                .is_some()
            {
                return Err(invalid(Path::new(&path), "conflicting archive source"));
            }
        }
        if let Some(path) = files.keys().find(|p| {
            p.split('/').next().is_some_and(|first| {
                first.eq_ignore_ascii_case(DESCRIPTOR) || first.eq_ignore_ascii_case(lock::NAME)
            })
        }) {
            return Err(invalid(
                Path::new(path),
                "input occupies reserved archive metadata",
            ));
        }
        let target = graph
            .target()
            .parse()
            .map_err(|_| invalid(Path::new(DESCRIPTOR), "invalid archive target"))?;
        let mut descriptor = serde_json::to_vec_pretty(&Descriptor {
            format: "lang-source-snapshot",
            schema: 1,
            target: graph.target(),
            compiler_build_id: lang_codegen_clif::sdk::build_id(&target),
            lock_sha256: lang_codegen_clif::sdk::digest(expected.as_bytes()),
        })
        .map_err(|_| capacity())?;
        descriptor.push(b'\n');
        files.insert(DESCRIPTOR.into(), (Kind::Descriptor, &descriptor));
        files.insert(lock::NAME.into(), (Kind::Lock, expected.as_bytes()));
        if files.len() > MAX_FILES {
            return Err(capacity());
        }
        let mut entries = Vec::new();
        let mut builder = tar::Builder::new(Bytes(Vec::new()));
        builder.mode(tar::HeaderMode::Deterministic);
        for (path, (kind, bytes)) in files {
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_mode(0o644);
            header.set_uid(0);
            header.set_gid(0);
            header.set_mtime(0);
            header.set_size(bytes.len() as u64);
            builder
                .append_data(&mut header, &path, Cursor::new(bytes))
                .map_err(|_| capacity())?;
            entries.push(Entry {
                path,
                kind,
                bytes: bytes.len() as u64,
                sha256: lang_codegen_clif::sdk::digest(bytes),
            });
        }
        builder.finish().map_err(|_| capacity())?;
        Ok(Self {
            entries,
            bytes: builder.into_inner().map_err(|_| capacity())?.0,
        })
    }
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Exact expected-byte verification; this never extracts foreign inputs.
    pub fn verify(&self, bytes: &[u8]) -> Result<(), Diagnostic> {
        if bytes.len() > MAX_OUTPUT {
            return Err(capacity());
        }
        if bytes != self.bytes {
            return Err(Diagnostic::error(
                "PKG0007",
                "archive does not match canonical frozen inputs",
            ));
        }
        Ok(())
    }
}
struct Bytes(Vec<u8>);
impl Write for Bytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_OUTPUT.saturating_sub(self.0.len()) {
            return Err(io::Error::other("archive byte limit"));
        }
        self.0.try_reserve(bytes.len()).map_err(io::Error::other)?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
