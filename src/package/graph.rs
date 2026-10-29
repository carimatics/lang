//! Authorized, deterministic local package snapshots for RFC 0034.
//! Resolution reads data only: it never runs dependency code or fetches a source.
use super::{
    Diagnostic,
    manifest::{MAX_BYTES, Manifest, Package, TARGETS},
};
use cap_std::fs::{Dir, OpenOptions};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub packages: usize,
    pub edges: usize,
    pub source_bytes: usize,
    pub source_files: usize,
    pub directory_entries: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            packages: 256,
            edges: 4096,
            source_bytes: 16 * 1024 * 1024,
            source_files: 256,
            directory_entries: 4096,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PackageId(usize);

impl PackageId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Debug)]
pub struct Snapshot {
    id: PackageId,
    root: PathBuf,
    manifest_path: PathBuf,
    manifest: Manifest,
    manifest_bytes: Vec<u8>,
    pub(super) manifest_metadata: cap_std::fs::Metadata,
    entry_path: PathBuf,
    entry_bytes: Vec<u8>,
    pub(super) entry_metadata: cap_std::fs::Metadata,
    pub(super) source_root: PathBuf,
    pub(super) source_directory: Mutex<Option<Dir>>,
    dependencies: BTreeMap<String, PackageId>,
}

impl Snapshot {
    pub const fn id(&self) -> PackageId {
        self.id
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.manifest_bytes
    }
    pub fn entry_path(&self) -> &Path {
        &self.entry_path
    }
    pub fn entry_bytes(&self) -> &[u8] {
        &self.entry_bytes
    }
    pub fn dependencies(&self) -> &BTreeMap<String, PackageId> {
        &self.dependencies
    }
}

#[derive(Debug)]
pub struct Graph {
    authority_root: PathBuf,
    target: String,
    packages: Vec<Snapshot>,
    pub(super) directory: Mutex<Option<Dir>>,
    pub(super) limits: Limits,
    pub(super) inventory: OnceLock<Result<super::source::inventory::Inventory, Diagnostic>>,
}

impl Graph {
    /// The explicit manifest's directory grants the only ambient read authority.
    /// Workspace member selection is a separate orchestration operation.
    pub fn resolve(manifest: &Path, target: &str, limits: Limits) -> Result<Self, Diagnostic> {
        if !TARGETS.contains(&target) {
            return Err(Diagnostic::error("PKG0003", "unsupported package target"));
        }
        if limits.packages == 0 {
            return Err(limit("package count limit exceeded"));
        }
        let absolute = if manifest.is_absolute() {
            manifest.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| io(manifest, e))?
                .join(manifest)
        };
        let name = absolute
            .file_name()
            .ok_or_else(|| io_message(manifest, "missing manifest filename"))?;
        let parent = absolute
            .parent()
            .ok_or_else(|| io_message(manifest, "missing manifest directory"))?;
        let root = std::fs::canonicalize(parent.join(".")).map_err(|e| io(parent, e))?;
        let directory = Dir::open_ambient_dir(root.join("."), cap_std::ambient_authority())
            .map_err(|e| io(&root, e))?;
        Self::resolve_in(directory, root, Path::new(name), target, limits)
    }

    /// Resolve through the caller's exact capability. `display` is diagnostic
    /// data only; it is never reopened. The caller may separately retain a
    /// clone as output authority while capture consumes this read capability.
    pub fn resolve_in(
        directory: Dir,
        display: PathBuf,
        name: &Path,
        target: &str,
        limits: Limits,
    ) -> Result<Self, Diagnostic> {
        if !TARGETS.contains(&target) {
            return Err(Diagnostic::error("PKG0003", "unsupported package target"));
        }
        if limits.packages == 0 {
            return Err(limit("package count limit exceeded"));
        }
        let mut components = name.components();
        if !matches!(components.next(), Some(std::path::Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(Diagnostic::error(
                "PKG0003",
                "manifest filename must be one normal component",
            ));
        }
        let mut resolver = Resolver {
            directory,
            display: display.clone(),
            target,
            limits,
            states: BTreeMap::new(),
            packages: Vec::new(),
            edges: 0,
            source_bytes: 0,
        };
        let package_root = resolver.canonical_directory(Path::new("."))?;
        resolver.visit(package_root, name, None)?;
        Ok(Self {
            authority_root: display,
            target: target.into(),
            packages: resolver.packages,
            directory: Mutex::new(Some(resolver.directory)),
            limits,
            inventory: OnceLock::new(),
        })
    }

    pub fn authority_root(&self) -> &Path {
        &self.authority_root
    }
    pub fn target(&self) -> &str {
        &self.target
    }
    pub const fn root(&self) -> PackageId {
        PackageId(0)
    }
    pub fn packages(&self) -> &[Snapshot] {
        &self.packages
    }
    pub fn package(&self, id: PackageId) -> Option<&Snapshot> {
        self.packages.get(id.0)
    }

    pub(super) fn inputs(&self) -> Result<&super::source::inventory::Inventory, Diagnostic> {
        self.inventory
            .get_or_init(|| super::source::inventory::capture(self))
            .as_ref()
            .map_err(Clone::clone)
    }
}

#[derive(Clone, Copy)]
enum State {
    Loading(PackageId),
    Ready(PackageId),
}

#[derive(Clone, Copy)]
struct Requirement<'a> {
    name: &'a str,
    version: &'a str,
}

struct Resolver<'a> {
    directory: Dir,
    display: PathBuf,
    target: &'a str,
    limits: Limits,
    states: BTreeMap<PathBuf, State>,
    packages: Vec<Snapshot>,
    edges: usize,
    source_bytes: usize,
}

fn limit(message: &str) -> Diagnostic {
    Diagnostic::error("PKG0004", message)
}
fn io(path: &Path, error: std::io::Error) -> Diagnostic {
    io_message(path, &error.to_string())
}
fn io_message(path: &Path, message: &str) -> Diagnostic {
    Diagnostic::error("PKG0005", message).at_path(path)
}
fn graph_error(path: &Path, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("PKG0006", message).at_path(path)
}

pub(super) fn input_path(root: &Path, name: &Path) -> PathBuf {
    if root == Path::new(".") {
        name.to_path_buf()
    } else {
        root.join(name)
    }
}

fn identity(
    package: &Package,
    expected: Option<Requirement<'_>>,
    path: &Path,
) -> Result<(), Diagnostic> {
    if let Some(expected) = expected {
        if package.name != expected.name {
            return Err(graph_error(
                path,
                format!(
                    "dependency alias {} does not match package {}",
                    expected.name, package.name
                ),
            ));
        }
        if package.version != expected.version {
            return Err(graph_error(
                path,
                format!(
                    "dependency {} requires exact version {}, found {}",
                    expected.name, expected.version, package.version
                ),
            ));
        }
    }
    Ok(())
}

impl Resolver<'_> {
    fn canonical_directory(&self, path: &Path) -> Result<PathBuf, Diagnostic> {
        self.directory
            // Requiring a directory avoids opening a FIFO as a final file.
            .canonicalize(path.join("."))
            .map_err(|e| io(&self.display.join(path), e))
    }

    fn read(&self, path: &Path, maximum: usize) -> Result<Input, Diagnostic> {
        read(&self.directory, &self.display, path, maximum)
    }
}

pub(super) struct Input {
    pub bytes: Vec<u8>,
    pub metadata: cap_std::fs::Metadata,
}

pub(super) fn read(
    directory: &Dir,
    display_root: &Path,
    path: &Path,
    maximum: usize,
) -> Result<Input, Diagnostic> {
    let display = display_root.join(path);
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use cap_std::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = directory
        .open_with(path, &options)
        .map_err(|e| io(&display, e))?;
    let metadata = file.metadata().map_err(|e| io(&display, e))?;
    if !metadata.is_file() {
        return Err(io_message(&display, "package input must be a regular file"));
    }
    if metadata.len() > maximum as u64 {
        return Err(limit("package input byte limit exceeded").at_path(&display));
    }
    let mut file = file.take((maximum as u64).saturating_add(1));
    let mut bytes = Vec::new();
    let mut chunk = [0; 16384];
    loop {
        let n = file.read(&mut chunk).map_err(|e| io(&display, e))?;
        if n == 0 {
            break;
        }
        let length = bytes
            .len()
            .checked_add(n)
            .ok_or_else(|| limit("package input capacity exceeded"))?;
        if length > maximum {
            return Err(limit("package input byte limit exceeded").at_path(&display));
        }
        bytes
            .try_reserve(n)
            .map_err(|_| limit("package input allocation refused"))?;
        bytes.extend_from_slice(&chunk[..n]);
    }
    Ok(Input { bytes, metadata })
}

impl Resolver<'_> {
    fn visit(
        &mut self,
        root: PathBuf,
        name: &Path,
        expected: Option<Requirement<'_>>,
    ) -> Result<PackageId, Diagnostic> {
        if let Some(state) = self.states.get(&root).copied() {
            let (id, loading) = match state {
                State::Loading(id) => (id, true),
                State::Ready(id) => (id, false),
            };
            let snapshot = &self.packages[id.0];
            identity(
                snapshot
                    .manifest
                    .package
                    .as_ref()
                    .expect("resolved package"),
                expected,
                &self.display.join(&snapshot.manifest_path),
            )?;
            if loading {
                return Err(graph_error(&self.display.join(&root), "dependency cycle"));
            }
            return Ok(id);
        }
        if self.packages.len() >= self.limits.packages {
            return Err(limit("package count limit exceeded").at_path(&self.display.join(&root)));
        }
        let manifest_path = input_path(&root, name);
        let input = self.read(&manifest_path, MAX_BYTES)?;
        let manifest_bytes = input.bytes;
        let manifest = Manifest::parse(&manifest_bytes)
            .map_err(|e| e.at_path(&self.display.join(&manifest_path)))?;
        let package = manifest.package.as_ref().ok_or_else(|| {
            graph_error(
                &self.display.join(&manifest_path),
                "select a package before resolving a workspace-only manifest",
            )
        })?;
        identity(package, expected, &self.display.join(&manifest_path))?;
        let entry_path = input_path(&root, Path::new(&package.entry));
        let source_root =
            self.canonical_directory(entry_path.parent().expect("entry directory"))?;
        let source_directory = self
            .directory
            .open_dir(source_root.join("."))
            .map_err(|e| io(&self.display.join(&source_root), e))?;
        let remaining = self.limits.source_bytes.saturating_sub(self.source_bytes);
        let entry = read(
            &source_directory,
            &self.display.join(&source_root),
            Path::new(entry_path.file_name().expect("entry filename")),
            remaining,
        )?;
        let entry_bytes = entry.bytes;
        self.source_bytes = self
            .source_bytes
            .checked_add(entry_bytes.len())
            .ok_or_else(|| limit("source byte capacity exceeded"))?;
        let dependencies: Vec<_> = manifest
            .dependencies
            .iter()
            .filter(|(_, d)| {
                d.targets
                    .as_ref()
                    .is_none_or(|ts| ts.iter().any(|t| t == self.target))
            })
            .map(|(a, d)| (a.clone(), d.clone()))
            .collect();
        let id = PackageId(self.packages.len());
        self.states.insert(root.clone(), State::Loading(id));
        self.packages.push(Snapshot {
            id,
            root: root.clone(),
            manifest_path,
            manifest,
            manifest_bytes,
            manifest_metadata: input.metadata,
            entry_path,
            entry_bytes,
            entry_metadata: entry.metadata,
            source_root: if source_root == Path::new(".") {
                PathBuf::new()
            } else {
                source_root
            },
            source_directory: Mutex::new(Some(source_directory)),
            dependencies: BTreeMap::new(),
        });
        for (alias, dependency) in dependencies {
            self.edges = self
                .edges
                .checked_add(1)
                .ok_or_else(|| limit("dependency edge capacity exceeded"))?;
            if self.edges > self.limits.edges {
                return Err(limit("dependency edge limit exceeded"));
            }
            let dependency_root = self.canonical_directory(&root.join(&dependency.path))?;
            let child = self.visit(
                dependency_root,
                Path::new("Lang.json"),
                Some(Requirement {
                    name: &alias,
                    version: &dependency.version,
                }),
            )?;
            self.packages[id.0].dependencies.insert(alias, child);
        }
        self.states.insert(root, State::Ready(id));
        Ok(id)
    }
}
