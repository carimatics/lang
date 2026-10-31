//! RFC 0036 formatting plans over immutable admitted source snapshots.
use super::{ByteSpan, Diagnostic, graph::Graph, lock::Lock, source};
use cap_std::fs::{Dir, OpenOptions};
use lang_diagnostics::SourceMap;
use lang_syntax::{ParseLimits, format as cst, parse};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use std::{
    ffi::OsString,
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug)]
pub struct Error {
    diagnostics: Vec<Diagnostic>,
}
impl Error {
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
impl From<Diagnostic> for Error {
    fn from(diagnostic: Diagnostic) -> Self {
        Self {
            diagnostics: vec![diagnostic],
        }
    }
}
#[derive(Debug)]
pub struct Change {
    path: PathBuf,
    text: String,
}
impl Change {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    Manifest,
    Source,
}
#[derive(Debug)]
struct Input {
    path: PathBuf,
    bytes: Vec<u8>,
    identity: source::Identity,
    role: Role,
}
#[derive(Debug)]
pub struct Plan {
    inputs: Vec<Input>,
    changes: Vec<Change>,
}
fn capacity(path: &Path, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("PKG0004", error.to_string()).at_path(path)
}
impl Plan {
    pub fn capture(graph: &Graph) -> Result<Self, Error> {
        if graph.has_registry() {
            return Err(Diagnostic::error(
                "PKG0003",
                "registry graph requires typed local edit bindings",
            )
            .into());
        }
        let inputs = capture_inputs(graph)?;
        let mut changes = Vec::new();
        let mut diagnostics = Vec::new();
        let mut total = 0usize;
        for (path, file) in &graph.inputs()?.files {
            let text = match std::str::from_utf8(&file.bytes) {
                Ok(text) => text,
                Err(e) => {
                    let mut d =
                        Diagnostic::error("FMT0002", "source is not valid UTF-8").at_path(path);
                    d.span = Some(ByteSpan {
                        start: e.valid_up_to(),
                        end: e.valid_up_to() + e.error_len().unwrap_or(0),
                    });
                    diagnostics.push(d);
                    continue;
                }
            };
            let mut sources = SourceMap::new();
            let id = sources
                .add(path.to_string_lossy(), text)
                .map_err(|e| Error::from(capacity(path, e)))?;
            let tree = parse(&sources, id, ParseLimits::default())
                .map_err(|e| Error::from(capacity(path, e)))?;
            if tree.has_errors() {
                for original in tree.diagnostics() {
                    let mut d = Diagnostic::error(original.code.as_str(), original.message.clone())
                        .at_path(path);
                    d.span = Some(ByteSpan {
                        start: original.primary.span.range().start,
                        end: original.primary.span.range().end,
                    });
                    diagnostics.push(d);
                }
                continue;
            }
            let formatted = cst::format(&tree, &sources, cst::Limits::default())
                .map_err(|e| Error::from(capacity(path, e)))?;
            total = total
                .checked_add(formatted.text.len())
                .filter(|n| *n <= super::output::MAX_OUTPUT)
                .ok_or_else(|| {
                    Error::from(capacity(path, "aggregate formatted byte limit exceeded"))
                })?;
            if formatted.text != text {
                changes.push(Change {
                    path: path.clone(),
                    text: formatted.text,
                });
            }
        }
        if !diagnostics.is_empty() {
            return Err(Error { diagnostics });
        }
        Ok(Self {
            inputs: inputs.into_values().collect(),
            changes,
        })
    }
    /// Only checked in-process recipes may bind replacements to a frozen graph.
    pub(super) fn from_replacements(
        graph: &Graph,
        replacements: impl IntoIterator<Item = (PathBuf, String)>,
    ) -> Result<Self, Diagnostic> {
        let inputs = capture_inputs(graph)?;
        let mut changes = BTreeMap::new();
        let mut total = 0usize;
        for (path, text) in replacements {
            let input = inputs
                .get(&path)
                .filter(|i| i.role == Role::Source)
                .ok_or_else(|| {
                    Diagnostic::error("PKG0003", "replacement is not a frozen source input")
                        .at_path(&path)
                })?;
            total = total
                .checked_add(text.len())
                .filter(|n| *n <= super::output::MAX_OUTPUT)
                .ok_or_else(|| capacity(&path, "aggregate replacement byte limit exceeded"))?;
            if input.bytes == text.as_bytes() {
                continue;
            }
            if changes
                .insert(
                    path.clone(),
                    Change {
                        path: path.clone(),
                        text,
                    },
                )
                .is_some()
            {
                return Err(
                    Diagnostic::error("PKG0003", "duplicate source replacement").at_path(&path)
                );
            }
        }
        Ok(Self {
            inputs: inputs.into_values().collect(),
            changes: changes.into_values().collect(),
        })
    }
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }
    /// Bind one already captured standalone source to a checked in-process
    /// replacement. The caller must prove these exact bytes before publication.
    pub(super) fn from_source(
        path: &Path,
        input: super::graph::Input,
        replacement: String,
    ) -> Result<Self, Diagnostic> {
        if replacement.len() > super::output::MAX_OUTPUT {
            return Err(capacity(path, "replacement byte limit exceeded"));
        }
        let identity =
            source::identity(&input.metadata, path).map_err(|e| operation(path, 0, e))?;
        let changes = if input.bytes == replacement.as_bytes() {
            Vec::new()
        } else {
            vec![Change {
                path: path.to_path_buf(),
                text: replacement,
            }]
        };
        Ok(Self {
            inputs: vec![Input {
                path: path.to_path_buf(),
                bytes: input.bytes,
                identity,
                role: Role::Source,
            }],
            changes,
        })
    }
    /// Revalidate a captured plan without publishing source changes.
    pub fn validate(&self, authority: &Dir) -> Result<(), Diagnostic> {
        self.live(authority, false).map(|_| ())
    }
    fn live(&self, authority: &Dir, writing: bool) -> Result<BTreeMap<PathBuf, Dir>, Diagnostic> {
        let mut parents = BTreeMap::new();
        for input in &self.inputs {
            let parent_path = input.path.parent().expect("relative input parent");
            let name = Path::new(input.path.file_name().expect("input filename"));
            let canonical = authority
                .canonicalize(parent_path.join("."))
                .map_err(|e| stale(&input.path, e))?;
            let normal = |p: &Path| {
                p.components()
                    .filter(|c| !matches!(c, std::path::Component::CurDir))
                    .collect::<PathBuf>()
            };
            if normal(&canonical) != normal(parent_path) {
                return Err(stale(&input.path, "parent binding changed"));
            }
            let parent = authority
                .open_dir(parent_path.join("."))
                .map_err(|e| stale(&input.path, e))?;
            if input.role == Role::Source {
                let metadata = parent
                    .symlink_metadata(name)
                    .map_err(|e| stale(&input.path, e))?;
                if !metadata.is_file() || metadata.file_type().is_symlink() {
                    return Err(Diagnostic::error(
                        "PKG0003",
                        "edited source must be a regular non-link file",
                    )
                    .at_path(&input.path));
                }
                if writing
                    && self.changes.iter().any(|c| c.path == input.path)
                    && metadata.permissions().readonly()
                {
                    return Err(operation(&input.path, 0, "source is read-only"));
                }
            }
            let current = super::graph::read(&parent, Path::new(""), name, input.bytes.len())
                .map_err(|e| stale(&input.path, e.message))?;
            if input.role == Role::Source {
                // Windows link counts, like identities, require metadata from
                // the opened File, not a path-only metadata query.
                #[cfg(unix)]
                let links = {
                    use cap_std::fs::MetadataExt;
                    current.metadata.nlink()
                };
                #[cfg(windows)]
                let links = {
                    use cap_fs_ext::MetadataExt;
                    current.metadata.nlink()
                };
                #[cfg(not(any(unix, windows)))]
                let links = 0;
                if links != 1 {
                    return Err(Diagnostic::error(
                        "PKG0003",
                        "edited source must have exactly one filesystem link",
                    )
                    .at_path(&input.path));
                }
            }
            let id = source::identity(&current.metadata, &input.path)
                .map_err(|e| operation(&input.path, 0, e))?;
            if id != input.identity || current.bytes != input.bytes {
                return Err(stale(&input.path, "input identity or bytes changed"));
            }
            if input.role == Role::Source && self.changes.iter().any(|c| c.path == input.path) {
                parents.entry(parent_path.to_path_buf()).or_insert(parent);
            }
        }
        Ok(parents)
    }
    /// Validate the complete plan, then publish atomic per-file replacements.
    /// A later I/O error reports the committed prefix; no external data is rolled back.
    pub fn apply(&self, authority: &Dir) -> Result<usize, Diagnostic> {
        self.apply_with(authority, |parent, from, to| {
            parent.rename(from, parent, to)
        })
    }
    fn apply_with(
        &self,
        authority: &Dir,
        mut replace: impl FnMut(&Dir, &std::ffi::OsStr, &std::ffi::OsStr) -> std::io::Result<()>,
    ) -> Result<usize, Diagnostic> {
        let parents = self.live(authority, true)?;
        let mut stages = Vec::new();
        for change in &self.changes {
            let parent = &parents[change.path.parent().expect("source parent")];
            let metadata = parent
                .metadata(change.path.file_name().expect("source filename"))
                .map_err(|e| operation(&change.path, 0, e))?;
            stages.push(Stage::new(parent, change, metadata.permissions())?);
        }
        self.live(authority, true)?;
        for (committed, (change, stage)) in self.changes.iter().zip(&stages).enumerate() {
            replace(
                stage.parent,
                &stage.name,
                change.path.file_name().expect("source filename"),
            )
            .map_err(|e| operation(&change.path, committed, e))?;
        }
        Ok(self.changes.len())
    }
}
fn capture_inputs(graph: &Graph) -> Result<BTreeMap<PathBuf, Input>, Diagnostic> {
    Lock::capture(graph)?;
    super::archive::canonical_directories(graph)?;
    let metadata: BTreeMap<_, _> = graph
        .manifest_inputs()
        .map(|(p, m)| {
            source::identity(m, p)
                .map(|id| (p.to_path_buf(), id))
                .map_err(|e| Diagnostic::error("PKG0005", e.to_string()).at_path(p))
        })
        .collect::<Result<_, _>>()?;
    let mut inputs = BTreeMap::new();
    for (path, bytes) in graph.manifest_files() {
        inputs.insert(
            path.to_path_buf(),
            Input {
                path: path.to_path_buf(),
                bytes: bytes.to_vec(),
                identity: metadata[path],
                role: Role::Manifest,
            },
        );
    }
    for (path, file) in &graph.inputs()?.files {
        inputs.insert(
            path.clone(),
            Input {
                path: path.clone(),
                bytes: file.bytes.clone(),
                identity: file.identity,
                role: Role::Source,
            },
        );
    }
    Ok(inputs)
}
fn stale(path: &Path, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("PKG0007", format!("source snapshot is stale: {error}")).at_path(path)
}
fn operation(path: &Path, committed: usize, error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error(
        "PKG0005",
        format!("source operation failed after {committed} replacements: {error}"),
    )
    .at_path(path)
}
struct Stage<'a> {
    parent: &'a Dir,
    name: OsString,
}
impl<'a> Stage<'a> {
    fn new(
        parent: &'a Dir,
        change: &Change,
        permissions: cap_std::fs::Permissions,
    ) -> Result<Self, Diagnostic> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let name = OsString::from(format!(
                ".lang-format-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use cap_std::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = match parent.open_with(&name, &options) {
                Ok(file) => file,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(operation(&change.path, 0, e)),
            };
            let stage = Self { parent, name };
            let result = file
                .write_all(change.text.as_bytes())
                .and_then(|_| file.set_permissions(permissions))
                .and_then(|_| file.sync_all());
            drop(file);
            result.map_err(|e| operation(&change.path, 0, e))?;
            return Ok(stage);
        }
        Err(capacity(&change.path, "format staging capacity exceeded"))
    }
}
impl Drop for Stage<'_> {
    fn drop(&mut self) {
        let _ = self.parent.remove_file(&self.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_replace_failure_reports_committed_prefix_and_cleans_unpublished_stages() {
        let root = std::env::temp_dir().join(format!(
            "lang late source publication {}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let authority = Dir::open_ambient_dir(&root, cap_std::ambient_authority()).unwrap();
        authority.write("Lang.json", br#"{"schema":1,"package":{"name":"app","version":"0.1.0","edition":"2026","entry":"main.lang"},"tests":["test.lang"]}"#).unwrap();
        let original = "fn main()->i64{let mut unused:i64=1;return 0;}";
        for file in ["main.lang", "test.lang"] {
            authority.write(file, original).unwrap();
        }
        let graph = std::sync::Arc::new(
            Graph::resolve(
                &root.join("Lang.json"),
                "aarch64-apple-darwin",
                Default::default(),
            )
            .unwrap(),
        );
        let preview =
            crate::package::fix::Preview::capture(graph.clone(), Default::default()).unwrap();
        let plan = Plan::from_replacements(
            &graph,
            preview
                .changes()
                .iter()
                .map(|c| (c.path().to_path_buf(), c.text().to_owned())),
        )
        .unwrap();
        let mut calls = 0;
        let error = plan
            .apply_with(&authority, |parent, from, to| {
                calls += 1;
                if calls == 2 {
                    Err(std::io::Error::other("injected late replacement failure"))
                } else {
                    parent.rename(from, parent, to)
                }
            })
            .unwrap_err();
        assert_eq!(calls, 2);
        assert_eq!(error.code, "PKG0005");
        assert_eq!(error.path.as_deref(), Some("test.lang"));
        assert!(error.message.contains("after 1 replacements"));
        assert_eq!(
            authority.read_to_string("main.lang").unwrap(),
            original.replace("unused", "_unused")
        );
        assert_eq!(authority.read_to_string("test.lang").unwrap(), original);
        assert!(
            authority
                .entries()
                .unwrap()
                .map(|e| e.unwrap())
                .all(|e| !e.file_name().to_string_lossy().starts_with(".lang-format-"))
        );
        drop(authority);
        std::fs::remove_dir_all(root).unwrap();
    }
}
