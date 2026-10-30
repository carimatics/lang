//! RFC 0044 checked previews. This module has no source-write authority.
use super::{
    ByteSpan, Diagnostic,
    editor::{self, Overlay, Snapshot, edits, workspace::View},
    graph::Graph,
    lint,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy)]
pub struct Limits {
    pub editor: editor::Limits,
    pub roots: usize,
    pub fixes: usize,
    pub edits: usize,
    /// Cumulative immutable source bytes charged before each analysis.
    pub analysis_bytes: usize,
    /// Conservative reservations for index/rename/proof passes, never reset per fix.
    pub work: usize,
    /// Cumulative complete replacement bytes, including intermediate overlays.
    pub rewritten_bytes: usize,
    pub report_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            editor: Default::default(),
            roots: 64,
            fixes: 256,
            edits: 65_536,
            analysis_bytes: 256 * 1024 * 1024,
            work: 1_000_000_000,
            rewritten_bytes: 64 * 1024 * 1024,
            report_bytes: 16 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Edit {
    pub path: PathBuf,
    pub span: ByteSpan,
    pub replacement: String,
    pub diagnostic: &'static str,
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
#[derive(Debug, Serialize)]
pub struct Preview {
    schema: u32,
    operation: &'static str,
    source_edition: &'static str,
    target_edition: &'static str,
    roots: Vec<PathBuf>,
    files: Vec<PathBuf>,
    edits: Vec<Edit>,
    changed: usize,
    applied: usize,
    applicability: &'static str,
    outcome: &'static str,
    #[serde(skip)]
    changes: Vec<Change>,
}
fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error("FIX0001", message)
}
fn capacity() -> Diagnostic {
    Diagnostic::error("FIX0002", "aggregate source-fix resource limit exceeded")
}
fn charge(remaining: &mut usize, amount: usize) -> Result<(), Diagnostic> {
    *remaining = remaining.checked_sub(amount).ok_or_else(capacity)?;
    Ok(())
}
struct Budget {
    analysis: usize,
    work: usize,
    rewritten: usize,
    edit_bytes: usize,
}
impl Budget {
    fn analyze(&mut self, graph: &Graph, overlays: &[Overlay]) -> Result<(), Diagnostic> {
        let replacements: BTreeMap<_, _> =
            overlays.iter().map(|o| (&o.path, o.bytes.len())).collect();
        for (path, file) in &graph.inputs()?.files {
            charge(
                &mut self.analysis,
                replacements.get(path).copied().unwrap_or(file.bytes.len()),
            )?;
        }
        Ok(())
    }
    fn snapshot(
        &mut self,
        graph: &Graph,
        root: Option<usize>,
        overlays: &[Overlay],
        limits: editor::Limits,
    ) -> Result<Snapshot, Diagnostic> {
        self.analyze(graph, overlays)?;
        charge(&mut self.work, limits.work)?;
        let snapshot = Snapshot::capture_entry(graph, root, 0, overlays, limits)?;
        if snapshot.analysis().result.is_err() {
            return Err(invalid(
                "source fixes require valid entry and declared-test analyses",
            ));
        }
        Ok(snapshot)
    }
}
impl Preview {
    pub fn capture(graph: Arc<Graph>, limits: Limits) -> Result<Self, Diagnostic> {
        super::lock::Lock::capture(&graph)?;
        super::archive::canonical_directories(&graph)?;
        let inputs = graph.inputs()?;
        let selected = inputs
            .declared
            .get(&graph.root())
            .ok_or_else(|| invalid("missing selected package inputs"))?;
        let roots: Vec<_> = selected
            .iter()
            .filter(|d| {
                matches!(
                    d.kind,
                    super::source::inventory::BindingKind::Entry
                        | super::source::inventory::BindingKind::Test
                )
            })
            .map(|d| d.target.clone())
            .collect();
        if roots.len() > limits.roots {
            return Err(capacity());
        }
        let eligible: BTreeSet<_> = selected
            .iter()
            .flat_map(|d| inputs.scopes[&d.scope].files.iter().cloned())
            .collect();
        let foreign: BTreeSet<_> = inputs
            .declared
            .iter()
            .filter(|(id, _)| **id != graph.root())
            .flat_map(|(_, ds)| ds)
            .flat_map(|d| inputs.scopes[&d.scope].files.iter())
            .map(|p| inputs.files[p].identity)
            .collect();
        if eligible
            .iter()
            .any(|p| foreign.contains(&inputs.files[p].identity))
        {
            return Err(invalid(
                "selected source shares an identity with a dependency",
            ));
        }
        let tests = graph
            .package(graph.root())
            .expect("resolved root")
            .manifest()
            .tests
            .len();
        let mut budget = Budget {
            analysis: limits.analysis_bytes,
            work: limits.work,
            rewritten: limits.rewritten_bytes,
            edit_bytes: limits.report_bytes,
        };
        let mut views = Vec::new();
        for root in std::iter::once(None).chain((0..tests).map(Some)) {
            let snapshot = budget.snapshot(&graph, root, &[], limits.editor)?;
            views.push((
                root,
                View {
                    authority: graph.authority_root().to_path_buf(),
                    snapshot,
                    graph: graph.clone(),
                    overlays: Vec::new(),
                    limits: limits.editor,
                },
            ));
        }
        let mut originals = Vec::new();
        let mut changes = Vec::new();
        let mut fixes = 0usize;
        while let Some(Candidate {
            index,
            path,
            offset,
            name,
        }) = candidate(&views, &eligible, &mut budget.work)?
        {
            if fixes == limits.fixes {
                return Err(capacity());
            }
            fixes += 1;
            let view = &views[index].1;
            // rename has one independent index pass and one checked edit/proof pass.
            // Reserve both upper bounds before invoking it, even for tiny sources.
            charge(
                &mut budget.work,
                limits.editor.work.checked_mul(2).ok_or_else(capacity)?,
            )?;
            budget.analyze(&graph, &view.overlays)?;
            let target = edits::target(view, &view.authority.join(&path), offset, 0)?;
            // Each matching token grows by one ASCII underscore in the analysis
            // performed inside rename. Reserve that growth before invoking it.
            charge(
                &mut budget.analysis,
                view.snapshot
                    .occurrences()
                    .iter()
                    .filter(|occurrence| occurrence.symbol == target.id)
                    .count(),
            )?;
            let plan = edits::rename(
                view,
                &view.authority.join(&path),
                offset,
                &format!("_{name}"),
                0,
            )?;
            let mut additions = Vec::new();
            for (display, spans) in &plan.files {
                let canonical = view.snapshot.canonical_path(display);
                if !eligible.contains(canonical) {
                    return Err(invalid("fix would edit a dependency"));
                }
                for span in spans {
                    if originals.len() + additions.len() == limits.edits {
                        return Err(capacity());
                    }
                    charge(
                        &mut budget.edit_bytes,
                        canonical
                            .as_os_str()
                            .as_encoded_bytes()
                            .len()
                            .checked_add(plan.replacement.len())
                            .ok_or_else(capacity)?,
                    )?;
                    additions.push(Edit {
                        path: canonical.to_path_buf(),
                        span: ByteSpan {
                            start: original_offset(&originals, canonical, span.start)?,
                            end: original_offset(&originals, canonical, span.end)?,
                        },
                        replacement: plan.replacement.clone(),
                        diagnostic: "LNT0001",
                    });
                }
            }
            originals.extend(additions);
            originals.sort_by(|a, b| {
                (&a.path, a.span.start, a.span.end).cmp(&(&b.path, b.span.start, b.span.end))
            });
            changes = render(&graph, &originals, &mut budget.rewritten)?;
            let overlays: Vec<_> = changes
                .iter()
                .map(|c| Overlay {
                    path: c.path.clone(),
                    version: 0,
                    bytes: Arc::from(c.text.as_bytes()),
                })
                .collect();
            for (root, view) in &mut views {
                let next = budget.snapshot(&graph, *root, &overlays, limits.editor)?;
                charge(&mut budget.work, limits.editor.work)?;
                let mut proof_work = limits.editor.work;
                edits::verify(view, &next, &plan, &mut proof_work)?;
                view.snapshot = next;
                view.overlays.clone_from(&overlays);
            }
        }
        let files = views
            .iter()
            .flat_map(|(_, v)| {
                v.snapshot
                    .analysis()
                    .sources
                    .iter()
                    .map(|(_, f)| PathBuf::from(f.name()))
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let preview = Self {
            schema: 1,
            operation: "fix",
            source_edition: "2026",
            target_edition: "2026",
            roots,
            files,
            edits: originals,
            changed: changes.len(),
            applied: 0,
            applicability: "checked",
            outcome: "preview",
            changes,
        };
        serde_json::to_writer(ReportLimit(limits.report_bytes), &preview).map_err(|error| {
            if error.is_io() {
                capacity()
            } else {
                invalid("fix report cannot encode its source paths")
            }
        })?;
        Ok(preview)
    }
    pub fn edits(&self) -> &[Edit] {
        &self.edits
    }
    pub fn changes(&self) -> &[Change] {
        &self.changes
    }
}
struct ReportLimit(usize);
impl std::io::Write for ReportLimit {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| std::io::Error::other("fix report byte limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
#[derive(Eq, Ord, PartialEq, PartialOrd)]
struct Candidate {
    path: PathBuf,
    offset: usize,
    index: usize,
    name: String,
}
fn candidate(
    views: &[(Option<usize>, View)],
    eligible: &BTreeSet<PathBuf>,
    work: &mut usize,
) -> Result<Option<Candidate>, Diagnostic> {
    let mut selected: Option<Candidate> = None;
    for (index, (_, view)) in views.iter().enumerate() {
        let program = view
            .snapshot
            .analysis()
            .result
            .as_ref()
            .map_err(|_| invalid("invalid source-fix snapshot"))?;
        for function in &program.source().functions {
            charge(
                work,
                function
                    .expressions
                    .len()
                    .checked_add(function.locals.len())
                    .ok_or_else(capacity)?,
            )?;
        }
        for name in lint::unread_bindings(program.source()) {
            let file = view
                .snapshot
                .analysis()
                .sources
                .file(name.span.source())
                .map_err(|_| invalid("missing fix source"))?;
            let path = view.snapshot.canonical_path(Path::new(file.name()));
            if !eligible.contains(path) {
                continue;
            }
            let next = Candidate {
                path: path.to_path_buf(),
                offset: name.span.range().start,
                index,
                name: name.text.clone(),
            };
            if selected.as_ref().is_none_or(|old| next < *old) {
                selected = Some(next);
            }
        }
    }
    Ok(selected)
}
fn original_offset(edits: &[Edit], path: &Path, offset: usize) -> Result<usize, Diagnostic> {
    let (mut old, mut new) = (0usize, 0usize);
    for edit in edits.iter().filter(|e| e.path == path) {
        let boundary = new
            .checked_add(
                edit.span
                    .start
                    .checked_sub(old)
                    .ok_or_else(|| invalid("overlapping fixes"))?,
            )
            .ok_or_else(capacity)?;
        if offset <= boundary {
            return old
                .checked_add(
                    offset
                        .checked_sub(new)
                        .ok_or_else(|| invalid("overlapping fixes"))?,
                )
                .ok_or_else(capacity);
        }
        new = boundary
            .checked_add(edit.replacement.len())
            .ok_or_else(capacity)?;
        if offset < new {
            return Err(invalid("fix overlaps a previous replacement"));
        }
        old = edit.span.end;
    }
    old.checked_add(
        offset
            .checked_sub(new)
            .ok_or_else(|| invalid("overlapping fixes"))?,
    )
    .ok_or_else(capacity)
}
fn render(graph: &Graph, edits: &[Edit], remaining: &mut usize) -> Result<Vec<Change>, Diagnostic> {
    let mut grouped: BTreeMap<&Path, Vec<&Edit>> = BTreeMap::new();
    for edit in edits {
        grouped.entry(&edit.path).or_default().push(edit);
    }
    let mut changes = Vec::new();
    for (path, edits) in grouped {
        let original = std::str::from_utf8(&graph.inputs()?.files[path].bytes)
            .map_err(|_| invalid("fix source is not UTF-8"))?;
        let mut end = 0;
        let mut text = String::new();
        for edit in edits {
            let unchanged = original
                .get(end..edit.span.start)
                .ok_or_else(|| invalid("overlapping or invalid original fix span"))?;
            if original.get(edit.span.start..edit.span.end).is_none() {
                return Err(invalid("invalid original fix span"));
            }
            charge(
                remaining,
                unchanged
                    .len()
                    .checked_add(edit.replacement.len())
                    .ok_or_else(capacity)?,
            )?;
            text.push_str(unchanged);
            text.push_str(&edit.replacement);
            end = edit.span.end;
        }
        let tail = original
            .get(end..)
            .ok_or_else(|| invalid("invalid original fix span"))?;
        charge(remaining, tail.len())?;
        text.push_str(tail);
        changes.push(Change {
            path: path.to_path_buf(),
            text,
        });
    }
    Ok(changes)
}
