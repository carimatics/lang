//! Package namespaces backed by original immutable source snapshots.
use super::{
    Diagnostic,
    graph::{self, Graph},
};
use crate::frontend::{self, Analysis, CompileLimits};
use lang_diagnostics::SourceMap;
use lang_semantics::modules::{
    ModuleLoadError, ModuleProvider, PackageSource, ParseFailure, ProvidedParse,
    load_package_sources,
};
use lang_syntax::{ParseLimits, add_source, parse};
use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

/// Resolve only package-authorized inputs. Guest runtime grants are unchanged.
/// The graph's entry snapshots survive later file edits; each newly reached
/// local module is admitted once and parsed from its retained bytes.
pub fn analyze(graph: &Graph, limits: CompileLimits) -> Result<Analysis, Diagnostic> {
    let mut provider = Provider {
        graph,
        roots: BTreeMap::new(),
        raw: BTreeMap::new(),
        identities: BTreeMap::new(),
        parsed: BTreeMap::new(),
        remaining: limits.modules.total_source_bytes,
    };
    let mut packages = Vec::new();
    for snapshot in graph.packages() {
        let path = snapshot
            .source_root
            .join(snapshot.entry_path().file_name().expect("entry filename"));
        provider
            .roots
            .insert(snapshot.source_root.clone(), &snapshot.source_directory);
        provider.remaining = provider
            .remaining
            .checked_sub(snapshot.entry_bytes().len())
            .ok_or_else(|| Diagnostic::error("PKG0004", "total source byte limit exceeded"))?;
        provider
            .insert(
                path.clone(),
                snapshot.entry_path().to_path_buf(),
                snapshot.entry_bytes().to_vec(),
                &snapshot.entry_metadata,
            )
            .map_err(package_io)?;
        packages.push(PackageSource {
            source_root: path.parent().unwrap_or(Path::new("")).to_path_buf(),
            entry: path,
            dependencies: snapshot
                .dependencies()
                .iter()
                .map(|(name, id)| (name.clone(), id.index()))
                .collect(),
        });
    }
    let mut sources = SourceMap::new();
    let modules = load_package_sources(&mut sources, &packages, limits.modules, &mut provider);
    Ok(frontend::finish(sources, modules, limits, None))
}

fn package_io(error: ModuleLoadError) -> Diagnostic {
    Diagnostic::error("PKG0005", error.to_string())
}
fn input_error(path: &Path, message: impl Into<String>) -> ModuleLoadError {
    ModuleLoadError::Io {
        path: path.to_path_buf(),
        error: io::Error::other(message.into()),
    }
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct Identity(u64, u64);
fn identity(metadata: &cap_std::fs::Metadata, path: &Path) -> Result<Identity, ModuleLoadError> {
    #[cfg(unix)]
    {
        use cap_std::fs::MetadataExt;
        let _ = path;
        Ok(Identity(metadata.dev(), metadata.ino()))
    }
    #[cfg(windows)]
    {
        use cap_fs_ext::MetadataExt;
        // graph::read obtains metadata from the opened regular File, as
        // required by cap-fs-ext's Windows dev()/ino() contract.
        let _ = path;
        Ok(Identity(metadata.dev(), metadata.ino()))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = metadata;
        Err(input_error(
            path,
            "package source identity is unsupported on this host",
        ))
    }
}

struct Raw {
    display: PathBuf,
    bytes: Vec<u8>,
}
struct Provider<'a> {
    graph: &'a Graph,
    roots: BTreeMap<PathBuf, &'a cap_std::fs::Dir>,
    raw: BTreeMap<PathBuf, Raw>,
    identities: BTreeMap<Identity, PathBuf>,
    parsed: BTreeMap<PathBuf, ProvidedParse>,
    remaining: usize,
}
impl Provider<'_> {
    fn path(&self, requested: &Path, root: Option<&Path>) -> Result<PathBuf, ModuleLoadError> {
        if requested.as_os_str().is_empty()
            || !requested
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err(ModuleLoadError::OutsideRoot {
                path: requested.to_path_buf(),
            });
        }
        let root = root.ok_or(ModuleLoadError::InvalidConfiguration)?;
        let relative = requested
            .strip_prefix(root)
            .map_err(|_| ModuleLoadError::OutsideRoot {
                path: requested.to_path_buf(),
            })?;
        let source_directory = self
            .roots
            .get(root)
            .ok_or(ModuleLoadError::InvalidConfiguration)?;
        let parent = relative.parent().unwrap_or(Path::new(""));
        let directory = source_directory
            .canonicalize(parent.join("."))
            .map_err(|e| input_error(requested, e.to_string()))?;
        let directory = if directory == Path::new(".") {
            PathBuf::new()
        } else {
            directory
        };
        let path = root
            .join(directory)
            .join(requested.file_name().expect("normal path"));
        Ok(path)
    }
    fn insert(
        &mut self,
        path: PathBuf,
        display: PathBuf,
        bytes: Vec<u8>,
        metadata: &cap_std::fs::Metadata,
    ) -> Result<PathBuf, ModuleLoadError> {
        let id = identity(metadata, &path)?;
        if let Some(previous) = self.identities.get(&id) {
            return Ok(previous.clone());
        }
        self.identities.insert(id, path.clone());
        self.raw.insert(path.clone(), Raw { display, bytes });
        Ok(path)
    }
}
impl ModuleProvider for Provider<'_> {
    fn resolve(
        &mut self,
        requested: &Path,
        source_root: Option<&Path>,
    ) -> Result<PathBuf, ModuleLoadError> {
        let path = self.path(requested, source_root)?;
        if self.raw.contains_key(&path) {
            return Ok(path);
        }
        let input = graph::read(
            self.roots[source_root.ok_or(ModuleLoadError::InvalidConfiguration)?],
            &self
                .graph
                .authority_root()
                .join(source_root.expect("root checked")),
            path.strip_prefix(source_root.expect("root checked"))
                .expect("path checked"),
            self.remaining,
        )
        .map_err(|e| input_error(&path, e.to_string()))?;
        self.remaining -= input.bytes.len();
        self.insert(
            path.clone(),
            requested.to_path_buf(),
            input.bytes,
            &input.metadata,
        )
    }
    fn parsed(
        &mut self,
        path: &Path,
        limits: ParseLimits,
    ) -> Result<ProvidedParse, ModuleLoadError> {
        if let Some(parsed) = self.parsed.get(path) {
            if parsed.limits != limits {
                return Err(ModuleLoadError::InvalidConfiguration);
            }
            return Ok(parsed.clone());
        }
        let raw = self
            .raw
            .get(path)
            .ok_or_else(|| input_error(path, "source snapshot not admitted"))?;
        let display = raw
            .display
            .to_str()
            .ok_or_else(|| ModuleLoadError::NonUtf8Path {
                path: path.to_path_buf(),
            })?
            .replace(std::path::MAIN_SEPARATOR, "/");
        let mut sources = SourceMap::new();
        let result = add_source(&mut sources, display, &raw.bytes, limits.lex)
            .map_err(ParseFailure::Input)
            .and_then(|id| {
                parse(&sources, id, limits)
                    .map(Arc::new)
                    .map_err(ParseFailure::Parse)
            });
        let parsed = ProvidedParse {
            sources,
            result,
            limits,
        };
        self.parsed.insert(path.to_path_buf(), parsed.clone());
        Ok(parsed)
    }
}
