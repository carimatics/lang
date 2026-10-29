//! Host workspace admission for the package LSP profile.
use super::{Limits, Overlay, Snapshot};
use crate::package::{Diagnostic, graph::Graph};
use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};

pub(crate) struct Config {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub selected: Option<String>,
    pub target: String,
}
pub(crate) struct OpenDocument {
    pub path: PathBuf,
    pub version: i32,
    pub bytes: Arc<[u8]>,
}
pub(crate) struct View {
    pub authority: PathBuf,
    pub snapshot: Snapshot,
    pub graph: Graph,
    pub overlays: Vec<Overlay>,
    pub limits: Limits,
}
impl Config {
    pub fn capture(
        &self,
        epoch: u64,
        documents: &[OpenDocument],
        limits: Limits,
    ) -> Result<View, Diagnostic> {
        let manifest = std::fs::canonicalize(&self.manifest)
            .map_err(|e| Diagnostic::error("PKG0005", e.to_string()))?;
        if !manifest.starts_with(&self.root) {
            return Err(Diagnostic::error(
                "PKG0003",
                "manifest is outside the editor workspace",
            ));
        }
        let authority = manifest
            .parent()
            .ok_or_else(|| Diagnostic::error("PKG0003", "manifest has no parent"))?
            .to_path_buf();
        let graph = Graph::resolve_selected(
            &manifest,
            &self.target,
            Default::default(),
            self.selected.as_deref(),
        )?;
        let inputs = graph.inputs()?;
        let mut overlays = Vec::new();
        for document in documents {
            let Ok(path) = document.path.strip_prefix(&authority) else {
                continue;
            };
            if !path.components().all(|c| matches!(c, Component::Normal(_))) {
                return Err(Diagnostic::error(
                    "PKG0003",
                    "invalid editor document identity",
                ));
            }
            if let Some((_, bytes)) = graph.manifest_files().find(|(p, _)| *p == path) {
                if bytes != document.bytes.as_ref() {
                    return Err(Diagnostic::error(
                        "PKG0007",
                        "save and refresh the manifest before package navigation",
                    )
                    .at_path(path));
                }
                continue;
            }
            if inputs.files.contains_key(path) {
                overlays.push(Overlay {
                    path: path.to_path_buf(),
                    version: document.version,
                    bytes: document.bytes.clone(),
                });
            }
        }
        let snapshot = Snapshot::capture(&graph, epoch, &overlays, limits)?;
        Ok(View {
            authority,
            snapshot,
            graph,
            overlays,
            limits,
        })
    }
}
impl View {
    pub fn same_inputs(&self, other: &Self) -> bool {
        self.snapshot.inputs == other.snapshot.inputs
            && self.snapshot.aliases == other.snapshot.aliases
            && self.snapshot.versions() == other.snapshot.versions()
    }
    pub fn relative<'a>(&self, path: &'a Path) -> Result<&'a Path, Diagnostic> {
        path.strip_prefix(&self.authority)
            .map_err(|_| Diagnostic::error("PKG0003", "document is outside the selected package"))
    }
    pub fn source(
        &self,
        path: &Path,
    ) -> Result<(lang_diagnostics::SourceId, &lang_diagnostics::SourceFile), Diagnostic> {
        let relative = self.relative(path)?;
        let relative = self.snapshot.display_path(relative);
        self.snapshot
            .analysis()
            .sources
            .iter()
            .find(|(_, f)| Path::new(f.name()) == relative)
            .ok_or_else(|| {
                Diagnostic::error("PKG0003", "document is not loaded in the selected package")
            })
    }
}
