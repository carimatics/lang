//! Package namespaces backed by original immutable source snapshots.
use super::{Diagnostic, graph::Graph};
pub(super) mod inventory;
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
/// First use freezes all bounded source inputs and releases capture handles.
/// Later analyses use retained bytes, independent of edits and directory moves.
pub fn analyze(graph: &Graph, limits: CompileLimits) -> Result<Analysis, Diagnostic> {
    let inputs = graph.inputs()?;
    let mut provider = Provider {
        inventory: inputs,
        raw: BTreeMap::new(),
        identities: BTreeMap::new(),
        aliases: BTreeMap::new(),
        parsed: BTreeMap::new(),
    };
    let mut packages = Vec::new();
    for snapshot in graph.packages() {
        let path = snapshot
            .source_root
            .join(snapshot.entry_path().file_name().expect("entry filename"));
        provider
            .insert(
                path.clone(),
                snapshot.entry_path().to_path_buf(),
                snapshot.entry_bytes().to_vec(),
                inputs.files[&path].identity,
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
    for (path, file) in &inputs.files {
        provider
            .insert(
                path.clone(),
                file.display.clone(),
                file.bytes.clone(),
                file.identity,
            )
            .map_err(package_io)?;
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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Identity(u64, u64);
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
    inventory: &'a inventory::Inventory,
    raw: BTreeMap<PathBuf, Raw>,
    identities: BTreeMap<Identity, PathBuf>,
    aliases: BTreeMap<PathBuf, PathBuf>,
    parsed: BTreeMap<PathBuf, ProvidedParse>,
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
        self.inventory
            .path(requested, root)
            .ok_or_else(|| input_error(requested, "source is not in the frozen package inventory"))
    }
    fn insert(
        &mut self,
        path: PathBuf,
        display: PathBuf,
        bytes: Vec<u8>,
        id: Identity,
    ) -> Result<PathBuf, ModuleLoadError> {
        if let Some(previous) = self.identities.get(&id) {
            self.aliases.insert(path, previous.clone());
            return Ok(previous.clone());
        }
        self.aliases.insert(path.clone(), path.clone());
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
        self.aliases
            .get(&path)
            .cloned()
            .ok_or_else(|| input_error(&path, "source snapshot not admitted"))
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
