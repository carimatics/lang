//! Bounded immutable package inputs; capture handles never escape this module.
use super::{Identity, identity, package_io};
use crate::package::{
    Diagnostic,
    graph::{self, Graph, PackageId},
};
use cap_std::fs::Dir;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path, PathBuf},
};

#[derive(Debug)]
pub(crate) struct File {
    pub bytes: Vec<u8>,
    pub display: PathBuf,
    pub identity: Identity,
}
#[derive(Debug, Default)]
pub(crate) struct Scope {
    pub files: BTreeSet<PathBuf>,
    pub aliases: BTreeMap<PathBuf, PathBuf>,
}
#[derive(Debug, Default)]
pub(crate) struct Inventory {
    pub files: BTreeMap<PathBuf, File>,
    pub scopes: BTreeMap<PathBuf, Scope>,
    pub declared: BTreeMap<PackageId, Vec<Declared>>,
}
#[derive(Debug)]
pub(crate) struct Declared {
    pub kind: BindingKind,
    pub scope: PathBuf,
    pub path: PathBuf,
    pub target: PathBuf,
}
#[derive(
    Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BindingKind {
    Directory,
    Entry,
    Test,
}
impl BindingKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::Entry => "entry",
            Self::Test => "test",
        }
    }
}
impl Inventory {
    pub fn path(&self, requested: &Path, root: &Path) -> Option<PathBuf> {
        let scope = self.scopes.get(root)?;
        let mut path = requested.to_path_buf();
        for _ in 0..=scope.aliases.len() {
            let parent = path.parent()?;
            let Some((prefix, target)) = parent
                .ancestors()
                .find_map(|p| scope.aliases.get(p).map(|t| (p.to_path_buf(), t)))
            else {
                return scope.files.contains(&path).then_some(path);
            };
            path = target.join(path.strip_prefix(prefix).ok()?);
        }
        None
    }
}
fn error(path: &Path, message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("PKG0005", message).at_path(path)
}
fn normal(root: &Path) -> PathBuf {
    if root == Path::new(".") {
        PathBuf::new()
    } else {
        root.to_path_buf()
    }
}
fn portable(path: &Path) -> bool {
    path.to_str().is_some_and(|s| {
        !s.replace(std::path::MAIN_SEPARATOR, "/")
            .contains(['\\', ':', '\0'])
    }) && path.components().all(|p| matches!(p, Component::Normal(_)))
}

pub(crate) fn capture(graph: &Graph) -> Result<Inventory, Diagnostic> {
    // Move every capture handle out first, so success and failure release all
    // I/O authority. OnceLock caches this epoch's result in the Graph.
    let authority = graph
        .directory
        .lock()
        .map_err(|_| error(graph.authority_root(), "source authority lock poisoned"))?
        .take()
        .ok_or_else(|| error(graph.authority_root(), "source authority already consumed"))?;
    let mut roots = BTreeMap::new();
    for package in graph.packages() {
        let dir = package
            .source_directory
            .lock()
            .map_err(|_| error(package.root(), "source directory lock poisoned"))?
            .take()
            .ok_or_else(|| error(package.root(), "source directory already consumed"))?;
        roots.entry(package.source_root.clone()).or_insert(dir);
    }
    let mut builder = Builder {
        graph,
        data: Inventory::default(),
        remaining: graph.limits.source_bytes,
        entries: 0,
    };
    let mut seeds = Vec::new();
    for package in graph.packages() {
        let path = package
            .source_root
            .join(package.entry_path().file_name().expect("entry filename"));
        builder
            .data
            .declared
            .entry(package.id())
            .or_default()
            .push(Declared {
                kind: BindingKind::Entry,
                scope: package.source_root.clone(),
                path: package.entry_path().to_path_buf(),
                target: path.clone(),
            });
        builder.charge_bytes(package.entry_bytes().len())?;
        builder.file(
            path.clone(),
            File {
                bytes: package.entry_bytes().to_vec(),
                display: package.entry_path().to_path_buf(),
                identity: identity(&package.entry_metadata, &path).map_err(package_io)?,
            },
        )?;
        seeds.push((package.source_root.clone(), path));
        for test in &package.manifest().tests {
            let requested = normal(package.root()).join(test);
            let root = normal(
                &authority
                    .canonicalize(requested.parent().expect("test directory").join("."))
                    .map_err(|e| error(&requested, e.to_string()))?,
            );
            if !roots.contains_key(&root) {
                let dir = authority
                    .open_dir(root.join("."))
                    .map_err(|e| error(&root, e.to_string()))?;
                roots.insert(root.clone(), dir);
            }
            let name = Path::new(requested.file_name().expect("test filename"));
            let input = graph::read(
                &roots[&root],
                &graph.authority_root().join(&root),
                name,
                builder.remaining,
            )?;
            builder.charge_bytes(input.bytes.len())?;
            let path = root.join(name);
            builder
                .data
                .declared
                .entry(package.id())
                .or_default()
                .push(Declared {
                    kind: BindingKind::Test,
                    scope: root.clone(),
                    path: requested.clone(),
                    target: path.clone(),
                });
            builder.file(
                path.clone(),
                File {
                    identity: identity(&input.metadata, &path).map_err(package_io)?,
                    bytes: input.bytes,
                    display: requested,
                },
            )?;
            seeds.push((root, path));
        }
    }
    for (root, dir) in roots {
        let mut scope = Scope::default();
        for (_, path) in &seeds {
            if builder.allowed(&root, path) {
                scope.files.insert(path.clone());
            }
        }
        builder.walk(&root, &dir, &mut scope)?;
        builder.data.scopes.insert(root, scope);
    }
    Ok(builder.data)
}
struct Builder<'a> {
    graph: &'a Graph,
    data: Inventory,
    remaining: usize,
    entries: usize,
}
impl Builder<'_> {
    fn charge_bytes(&mut self, bytes: usize) -> Result<(), Diagnostic> {
        self.remaining = self
            .remaining
            .checked_sub(bytes)
            .ok_or_else(|| Diagnostic::error("PKG0004", "source snapshot byte limit exceeded"))?;
        Ok(())
    }
    fn file(&mut self, path: PathBuf, file: File) -> Result<(), Diagnostic> {
        if !portable(&path) {
            return Err(error(&path, "source path must be portable UTF-8"));
        }
        if let Some(previous) = self.data.files.get(&path) {
            if previous.bytes != file.bytes || previous.identity != file.identity {
                return Err(
                    Diagnostic::error("PKG0006", "source changed during snapshot capture")
                        .at_path(&path),
                );
            }
        } else {
            if self.data.files.len() >= self.graph.limits.source_files {
                return Err(
                    Diagnostic::error("PKG0004", "source snapshot file limit exceeded")
                        .at_path(&path),
                );
            }
            self.data.files.insert(path, file);
        }
        Ok(())
    }
    fn allowed(&self, root: &Path, path: &Path) -> bool {
        path.starts_with(root)
            && !self.graph.packages().iter().any(|p| {
                let package = normal(p.root());
                (package != root && package.starts_with(root) && path.starts_with(&package))
                    || path.starts_with(package.join("target"))
            })
    }
    fn walk(&mut self, root: &Path, directory: &Dir, scope: &mut Scope) -> Result<(), Diagnostic> {
        enum Visit {
            Enter(PathBuf),
            Exit(PathBuf),
        }
        let mut pending = vec![Visit::Enter(PathBuf::new())];
        let mut state = BTreeMap::new();
        while let Some(visit) = pending.pop() {
            let path = match visit {
                Visit::Exit(path) => {
                    state.insert(path, false);
                    continue;
                }
                Visit::Enter(path) => path,
            };
            match state.get(&path) {
                Some(true) => return Err(error(&root.join(&path), "source directory cycle")),
                Some(false) => continue,
                None => {}
            }
            state.insert(path.clone(), true);
            pending.push(Visit::Exit(path.clone()));
            let dir = directory
                .open_dir(path.join("."))
                .map_err(|e| error(&root.join(&path), e.to_string()))?;
            let mut entries = Vec::new();
            for entry in dir
                .entries()
                .map_err(|e| error(&root.join(&path), e.to_string()))?
            {
                if self.entries >= self.graph.limits.directory_entries {
                    return Err(Diagnostic::error(
                        "PKG0004",
                        "source directory entry limit exceeded",
                    ));
                }
                self.entries += 1;
                let entry = entry.map_err(|e| error(&root.join(&path), e.to_string()))?;
                entries.push((
                    entry.file_name(),
                    entry
                        .file_type()
                        .map_err(|e| error(&root.join(&path), e.to_string()))?,
                ));
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut children = Vec::new();
            for (name, ty) in entries {
                let child = path.join(name);
                let global = root.join(&child);
                if !self.allowed(root, &global)
                    || child
                        .file_name()
                        .is_some_and(|n| n == ".git" || n == ".codex")
                {
                    continue;
                }
                let is_dir = if ty.is_dir() {
                    true
                } else if ty.is_symlink() {
                    match directory.open_dir(child.join(".")) {
                        Ok(_) => true,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::NotADirectory
                                || child.extension().is_some_and(|ext| ext == "lang") =>
                        {
                            false
                        }
                        Err(e) => return Err(error(&global, e.to_string())),
                    }
                } else {
                    false
                };
                if is_dir {
                    let canonical = normal(
                        &directory
                            .canonicalize(child.join("."))
                            .map_err(|e| error(&global, e.to_string()))?,
                    );
                    if canonical != child {
                        scope.aliases.insert(global, root.join(&canonical));
                    }
                    if self.allowed(root, &root.join(&canonical)) {
                        children.push(canonical);
                    }
                } else if child.extension().is_some_and(|ext| ext == "lang") {
                    if !portable(&global) {
                        return Err(error(&global, "source path must be portable UTF-8"));
                    }
                    // Explicit entry bytes were already admitted by their
                    // immediate directory capability and remain immutable.
                    if !scope.files.contains(&global) {
                        let input = graph::read(
                            directory,
                            &self.graph.authority_root().join(root),
                            &child,
                            self.remaining,
                        )?;
                        self.charge_bytes(input.bytes.len())?;
                        self.file(
                            global.clone(),
                            File {
                                identity: identity(&input.metadata, &global).map_err(package_io)?,
                                bytes: input.bytes,
                                display: global.clone(),
                            },
                        )?;
                    }
                    scope.files.insert(global);
                }
            }
            for child in children.into_iter().rev() {
                pending.push(Visit::Enter(child));
            }
        }
        Ok(())
    }
}
