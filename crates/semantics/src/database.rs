//! Automatic parser/module/diagnostic queries over immutable source revisions.
//! Filesystem authority is an explicit canonical root. Open overlays win over
//! disk inputs; new discovery/watched-file events conservatively invalidate
//! module queries, while ordinary edits invalidate only reverse dependencies.
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::binding::{BindingError, BindingLimits, bind_program};
use crate::modules::{
    ModuleGraph, ModuleLimits, ModuleLoadError, ModuleProvider, ParseFailure, ProvidedParse,
    load_preparsed,
};
use lang_diagnostics::{Diagnostic, Label, Severity, SourceId, SourceMap, codes};
use lang_hir::{
    layout::LayoutError,
    program::{CheckError, CheckLimits, check_program},
};
use lang_query::{Graph, GraphError, Key, Ticket};
use lang_syntax::{ParseError, ParseLimits, add_source, parse};

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub modules: ModuleLimits,
    pub binding: BindingLimits,
    pub typing: CheckLimits,
    pub graph: lang_query::Limits,
    pub files: usize,
    pub input_bytes: usize,
    pub parse_nodes: usize,
    pub module_nodes: usize,
    pub diagnostic_bytes: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            modules: ModuleLimits::default(),
            binding: BindingLimits::default(),
            typing: CheckLimits::default(),
            graph: lang_query::Limits {
                nodes: 4096,
                edges: 65_536,
                work: 10_000_000,
            },
            files: 256,
            input_bytes: 16 * 1024 * 1024,
            parse_nodes: 2_000_000,
            module_nodes: 2_000_000,
            diagnostic_bytes: 16 * 1024 * 1024,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    Files,
    InputBytes,
    ParseNodes,
    ModuleNodes,
    DiagnosticBytes,
}
#[derive(Debug)]
pub enum DbError {
    Graph(GraphError),
    Io { path: PathBuf, error: io::Error },
    OutsideRoot(PathBuf),
    InvalidPath,
    AlreadyOpen,
    NotOpen,
    Limit(LimitKind),
}
impl From<GraphError> for DbError {
    fn from(e: GraphError) -> Self {
        Self::Graph(e)
    }
}
impl Display for DbError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "source database error: {self:?}")
    }
}
impl Error for DbError {}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Stats {
    pub parses: u64,
    pub modules: u64,
    pub analyses: u64,
    pub disk_reads: u64,
}
#[derive(Debug)]
pub struct ModuleSnapshot {
    pub sources: SourceMap,
    pub paths: BTreeMap<SourceId, PathBuf>,
    pub graph: Option<ModuleGraph>,
    pub diagnostics: Vec<Diagnostic>,
    pub error: Option<Arc<ModuleLoadError>>,
    cost: usize,
}
#[derive(Debug)]
pub enum AnalysisFailure {
    Module(Arc<ModuleLoadError>),
    Binding(BindingError),
    Typing(CheckError),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisStatus {
    Valid,
    Diagnostics,
}
#[derive(Debug)]
pub struct AnalysisSnapshot {
    pub sources: SourceMap,
    pub paths: BTreeMap<SourceId, PathBuf>,
    pub diagnostics: Vec<Diagnostic>,
    pub status: AnalysisStatus,
    pub error: Option<AnalysisFailure>,
}
enum Cached {
    Input(Option<Arc<[u8]>>),
    Parsed(Arc<ProvidedParse>),
    Module(Arc<ModuleSnapshot>),
    Analysis(Arc<AnalysisSnapshot>),
    Epoch,
}
#[derive(Clone, Copy)]
struct Keys {
    input: Key,
    parse: Key,
    module: Key,
    analysis: Key,
    open: bool,
}
pub struct Database {
    root: Option<PathBuf>,
    limits: Limits,
    graph: Graph<Cached>,
    files: BTreeMap<PathBuf, Keys>,
    next: u64,
    stats: Stats,
}
const EPOCH: Key = Key::new(0);
const CACHE_EPOCH: Key = Key::new(1);
impl Database {
    pub fn new(root: Option<&Path>, limits: Limits) -> Result<Self, DbError> {
        let root = root
            .map(|path| {
                std::fs::canonicalize(path).map_err(|error| DbError::Io {
                    path: path.into(),
                    error,
                })
            })
            .transpose()?;
        if root.as_ref().is_some_and(|root| !root.is_dir()) {
            return Err(DbError::InvalidPath);
        }
        let mut graph = Graph::new(limits.graph)?;
        graph.set_input(EPOCH, Arc::new(Cached::Epoch))?;
        graph.set_input(CACHE_EPOCH, Arc::new(Cached::Epoch))?;
        Ok(Self {
            root,
            limits,
            graph,
            files: BTreeMap::new(),
            next: 2,
            stats: Stats::default(),
        })
    }
    pub fn source_root(&self) -> Option<&Path> {
        self.root.as_deref()
    }
    pub const fn stats(&self) -> Stats {
        self.stats
    }
    pub const fn limits(&self) -> Limits {
        self.limits
    }
    pub fn raw_input(&self, path: &Path) -> Option<&[u8]> {
        let keys = self.files.get(path)?;
        match self.graph.get(keys.input)?.as_ref() {
            Cached::Input(Some(bytes)) => Some(bytes),
            _ => None,
        }
    }
    pub fn open_paths(&self) -> impl Iterator<Item = &Path> {
        self.files
            .iter()
            .filter(|(_, keys)| keys.open)
            .map(|(path, _)| path.as_path())
    }
    /// Open overlays retain their authorized canonical identity. Otherwise,
    /// existing files canonicalize and new files need a canonical parent.
    /// With no root this is a pure lexical operation.
    pub fn document_path(&self, path: &Path) -> Result<PathBuf, DbError> {
        if !path.is_absolute()
            || path.to_str().is_none()
            || path.components().any(|p| {
                matches!(
                    p,
                    std::path::Component::ParentDir | std::path::Component::CurDir
                )
            })
        {
            return Err(DbError::InvalidPath);
        }
        // An authorized open overlay remains usable after its on-disk path is
        // deleted or replaced. Reading it cannot acquire new disk authority.
        if self.files.get(path).is_some_and(|keys| keys.open) {
            return Ok(path.into());
        }
        if let Some(root) = &self.root {
            let canonical = match std::fs::canonicalize(path) {
                Ok(path) => path,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let parent = path.parent().ok_or(DbError::InvalidPath)?;
                    std::fs::canonicalize(parent)
                        .map_err(|error| DbError::Io {
                            path: parent.into(),
                            error,
                        })?
                        .join(path.file_name().ok_or(DbError::InvalidPath)?)
                }
                Err(error) => {
                    return Err(DbError::Io {
                        path: path.into(),
                        error,
                    });
                }
            };
            if !canonical.starts_with(root) {
                return Err(DbError::OutsideRoot(canonical));
            }
            if canonical.is_dir() {
                return Err(DbError::InvalidPath);
            }
            Ok(canonical)
        } else {
            Ok(path.into())
        }
    }
    fn keys(&mut self, path: &Path) -> Result<Keys, DbError> {
        if let Some(keys) = self.files.get(path) {
            return Ok(*keys);
        }
        if self.files.len() >= self.limits.files {
            return Err(DbError::Limit(LimitKind::Files));
        }
        if self
            .graph
            .len()
            .checked_add(4)
            .is_none_or(|n| n > self.limits.graph.nodes)
        {
            return Err(GraphError::LimitExceeded(lang_query::LimitKind::Nodes).into());
        }
        let end = self
            .next
            .checked_add(4)
            .ok_or(GraphError::RevisionExhausted)?;
        let keys = Keys {
            input: Key::new(self.next),
            parse: Key::new(self.next + 1),
            module: Key::new(self.next + 2),
            analysis: Key::new(self.next + 3),
            open: false,
        };
        self.graph
            .set_input(keys.input, Arc::new(Cached::Input(None)))?;
        for key in [keys.parse, keys.module, keys.analysis] {
            self.graph.define_query(key)?;
        }
        self.next = end;
        self.files.insert(path.into(), keys);
        Ok(keys)
    }
    fn input_size(&self) -> usize {
        self.files
            .values()
            .filter_map(|keys| match self.graph.get(keys.input).map(Arc::as_ref) {
                Some(Cached::Input(Some(bytes))) => Some(bytes.len()),
                _ => None,
            })
            .sum()
    }
    fn store(&mut self, path: &Path, bytes: Option<Arc<[u8]>>) -> Result<(), DbError> {
        let old = match self
            .files
            .get(path)
            .and_then(|keys| self.graph.get(keys.input))
            .map(Arc::as_ref)
        {
            Some(Cached::Input(Some(bytes))) => bytes.len(),
            _ => 0,
        };
        if self
            .input_size()
            .checked_sub(old)
            .and_then(|n| n.checked_add(bytes.as_ref().map_or(0, |b| b.len())))
            .is_none_or(|n| n > self.limits.input_bytes)
        {
            return Err(DbError::Limit(LimitKind::InputBytes));
        }
        let keys = self.keys(path)?;
        self.graph
            .set_input(keys.input, Arc::new(Cached::Input(bytes)))?;
        Ok(())
    }
    pub fn open(&mut self, path: &Path, bytes: Arc<[u8]>) -> Result<PathBuf, DbError> {
        let path = self.document_path(path)?;
        if self.files.get(&path).is_some_and(|k| k.open) {
            return Err(DbError::AlreadyOpen);
        }
        if bytes.len() > self.limits.modules.parse.lex.source_bytes {
            return Err(DbError::Limit(LimitKind::InputBytes));
        }
        if self.root.is_some() {
            self.graph.set_input(EPOCH, Arc::new(Cached::Epoch))?;
        }
        self.store(&path, Some(bytes))?;
        self.files.get_mut(&path).unwrap().open = true;
        Ok(path)
    }
    pub fn change(&mut self, path: &Path, bytes: Arc<[u8]>) -> Result<(), DbError> {
        if !self.files.get(path).is_some_and(|k| k.open) {
            return Err(DbError::NotOpen);
        }
        if bytes.len() > self.limits.modules.parse.lex.source_bytes {
            return Err(DbError::Limit(LimitKind::InputBytes));
        }
        self.store(path, Some(bytes))
    }
    pub fn close(&mut self, path: &Path) -> Result<(), DbError> {
        if !self.files.get(path).is_some_and(|k| k.open) {
            return Err(DbError::NotOpen);
        }
        self.graph.set_input(CACHE_EPOCH, Arc::new(Cached::Epoch))?;
        if self.root.is_some() {
            self.graph.set_input(EPOCH, Arc::new(Cached::Epoch))?;
        }
        self.store(path, None)?;
        self.files.get_mut(path).unwrap().open = false;
        Ok(())
    }
    /// Watch notifications invalidate discovery (including missing/symlinked
    /// modules). Open overlays remain authoritative and are never reread.
    pub fn refresh(&mut self, path: &Path) -> Result<(), DbError> {
        if self.root.is_none() {
            return Err(DbError::InvalidPath);
        }
        self.graph.set_input(EPOCH, Arc::new(Cached::Epoch))?;
        let path = self.document_path(path)?;
        if self.files.get(&path).is_some_and(|k| !k.open) {
            self.store(&path, None)?;
        }
        Ok(())
    }
    fn resolve(&self, requested: &Path) -> Result<PathBuf, DbError> {
        if self.files.get(requested).is_some_and(|k| k.open) {
            return Ok(requested.into());
        }
        let Some(root) = &self.root else {
            return Err(DbError::NotOpen);
        };
        let path = std::fs::canonicalize(requested).map_err(|error| DbError::Io {
            path: requested.into(),
            error,
        })?;
        if !path.starts_with(root) {
            return Err(DbError::OutsideRoot(path));
        }
        Ok(path)
    }
    fn input(&mut self, path: &Path) -> Result<Arc<[u8]>, DbError> {
        let keys = self.keys(path)?;
        if let Some(Cached::Input(Some(bytes))) = self.graph.get(keys.input).map(Arc::as_ref) {
            return Ok(bytes.clone());
        }
        if self
            .root
            .as_ref()
            .is_none_or(|root| !path.starts_with(root))
        {
            return Err(DbError::NotOpen);
        }
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|file| {
                file.take(self.limits.modules.parse.lex.source_bytes.saturating_add(1) as u64)
                    .read_to_end(&mut bytes)
            })
            .map_err(|error| DbError::Io {
                path: path.into(),
                error,
            })?;
        self.stats.disk_reads = self.stats.disk_reads.saturating_add(1);
        let bytes: Arc<[u8]> = bytes.into();
        self.store(path, Some(bytes.clone()))?;
        Ok(bytes)
    }
    pub fn parsed(&mut self, path: &Path) -> Result<Arc<ProvidedParse>, DbError> {
        let path = self.document_path(path)?;
        let path = path.as_path();
        let keys = self.keys(path)?;
        if let Some(Cached::Parsed(parsed)) = self.graph.get(keys.parse).map(Arc::as_ref) {
            return Ok(parsed.clone());
        }
        let bytes = self.input(path)?;
        let mut sources = SourceMap::new();
        let limits = self.limits.modules.parse;
        let display = if let Some(root) = &self.root {
            path.strip_prefix(root)
                .map_err(|_| DbError::OutsideRoot(path.into()))?
        } else {
            Path::new(path.file_name().ok_or(DbError::InvalidPath)?)
        };
        let display = display
            .to_str()
            .ok_or(DbError::InvalidPath)?
            .replace(std::path::MAIN_SEPARATOR, "/");
        let mut result = match add_source(&mut sources, display, &bytes, limits.lex) {
            Ok(source) => parse(&sources, source, limits)
                .map(Arc::new)
                .map_err(ParseFailure::Parse),
            Err(error) => Err(ParseFailure::Input(error)),
        };
        if let Ok(tree) = &result {
            let existing: usize = self
                .files
                .values()
                .filter_map(|k| match self.graph.get(k.parse).map(Arc::as_ref) {
                    Some(Cached::Parsed(p)) => p.result.as_ref().ok().map(|t| t.nodes().len()),
                    _ => None,
                })
                .sum();
            if existing
                .checked_add(tree.nodes().len())
                .is_none_or(|n| n > self.limits.parse_nodes)
            {
                let span = tree.node(tree.root()).unwrap().span();
                result = Err(ParseFailure::Parse(ParseError::Limit(Box::new(
                    Diagnostic::new(
                        codes::RESOURCE_LIMIT,
                        Severity::Error,
                        "parser cache node limit exceeded",
                        Label::new(span, "parser cache limit"),
                    ),
                ))));
            }
        }
        let parsed = Arc::new(ProvidedParse {
            sources,
            result,
            limits,
        });
        let ticket = self.graph.begin(keys.parse, [keys.input, CACHE_EPOCH])?;
        self.graph
            .publish(ticket, Arc::new(Cached::Parsed(parsed.clone())))?;
        self.stats.parses = self.stats.parses.saturating_add(1);
        Ok(parsed)
    }
    pub fn modules(&mut self, path: &Path) -> Result<Arc<ModuleSnapshot>, DbError> {
        let path = self.document_path(path)?;
        let path = path.as_path();
        let keys = self.keys(path)?;
        if let Some(Cached::Module(module)) = self.graph.get(keys.module).map(Arc::as_ref) {
            return Ok(module.clone());
        }
        let root = self.root.clone();
        let limits = self.limits.modules;
        let mut sources = SourceMap::new();
        let mut provider = Provider {
            db: self,
            dependencies: vec![keys.input],
            paths: BTreeMap::new(),
            cost: 0,
        };
        let loaded = load_preparsed(&mut sources, path, root.as_deref(), limits, &mut provider);
        let mut dependencies = provider.dependencies;
        let paths = provider.paths;
        let cost = provider.cost;
        if root.is_some() {
            dependencies.push(EPOCH);
        }
        let existing: usize = self
            .files
            .values()
            .filter_map(|k| match self.graph.get(k.module).map(Arc::as_ref) {
                Some(Cached::Module(m)) => Some(m.cost),
                _ => None,
            })
            .sum();
        if existing
            .checked_add(cost)
            .is_none_or(|n| n > self.limits.module_nodes)
        {
            return Err(DbError::Limit(LimitKind::ModuleNodes));
        }
        let (graph, diagnostics, error) = match loaded {
            Ok(module) => (module.graph, module.diagnostics, None),
            Err(error) => (None, Vec::new(), Some(Arc::new(error))),
        };
        let module = Arc::new(ModuleSnapshot {
            sources,
            paths,
            graph,
            diagnostics,
            error,
            cost,
        });
        let ticket = self.graph.begin(keys.module, dependencies)?;
        self.graph
            .publish(ticket, Arc::new(Cached::Module(module.clone())))?;
        self.stats.modules = self.stats.modules.saturating_add(1);
        Ok(module)
    }
    pub fn prepare(&mut self, path: &Path) -> Result<AnalysisJob, DbError> {
        let path = self.document_path(path)?;
        let path = path.as_path();
        let module = self.modules(path)?;
        let keys = self.keys(path)?;
        let ticket = self.graph.begin(keys.analysis, [keys.module])?;
        self.stats.analyses = self.stats.analyses.saturating_add(1);
        Ok(AnalysisJob {
            ticket,
            module,
            binding: self.limits.binding,
            typing: self.limits.typing,
        })
    }
    pub fn publish(
        &mut self,
        completed: CompletedAnalysis,
    ) -> Result<Arc<AnalysisSnapshot>, DbError> {
        let existing: usize = self
            .files
            .values()
            .filter_map(|k| match self.graph.get(k.analysis).map(Arc::as_ref) {
                Some(Cached::Analysis(a)) => Some(diagnostic_bytes(a)),
                _ => None,
            })
            .sum();
        if existing
            .checked_add(diagnostic_bytes(&completed.snapshot))
            .is_none_or(|n| n > self.limits.diagnostic_bytes)
        {
            return Err(DbError::Limit(LimitKind::DiagnosticBytes));
        }
        self.graph.publish(
            completed.ticket,
            Arc::new(Cached::Analysis(completed.snapshot.clone())),
        )?;
        Ok(completed.snapshot)
    }
    pub fn analyze(&mut self, path: &Path) -> Result<Arc<AnalysisSnapshot>, DbError> {
        let path = self.document_path(path)?;
        let path = path.as_path();
        let keys = self.keys(path)?;
        if let Some(Cached::Analysis(analysis)) = self.graph.get(keys.analysis).map(Arc::as_ref) {
            return Ok(analysis.clone());
        }
        let completed = self.prepare(path)?.compute();
        self.publish(completed)
    }
}
fn diagnostic_bytes(analysis: &AnalysisSnapshot) -> usize {
    analysis
        .diagnostics
        .iter()
        .map(|d| {
            d.message.len()
                + d.notes.iter().map(String::len).sum::<usize>()
                + d.primary.message.len()
                + d.secondary.iter().map(|s| s.message.len()).sum::<usize>()
                + d.fixes
                    .iter()
                    .map(|f| {
                        f.message.len() + f.edits.iter().map(|e| e.replacement.len()).sum::<usize>()
                    })
                    .sum::<usize>()
        })
        .sum()
}
struct Provider<'a> {
    db: &'a mut Database,
    dependencies: Vec<Key>,
    paths: BTreeMap<SourceId, PathBuf>,
    cost: usize,
}
impl ModuleProvider for Provider<'_> {
    fn resolve(&mut self, requested: &Path, _: Option<&Path>) -> Result<PathBuf, ModuleLoadError> {
        self.db.resolve(requested).map_err(|error| match error {
            DbError::OutsideRoot(path) => ModuleLoadError::OutsideRoot { path },
            DbError::Io { path, error } => ModuleLoadError::Io { path, error },
            e => ModuleLoadError::Io {
                path: requested.into(),
                error: io::Error::other(e.to_string()),
            },
        })
    }
    fn parsed(
        &mut self,
        path: &Path,
        limits: ParseLimits,
    ) -> Result<ProvidedParse, ModuleLoadError> {
        if limits != self.db.limits.modules.parse {
            return Err(ModuleLoadError::InvalidConfiguration);
        }
        let keys = self.db.keys(path).map_err(|e| ModuleLoadError::Io {
            path: path.into(),
            error: io::Error::other(e.to_string()),
        })?;
        self.dependencies.push(keys.input);
        let parsed = self.db.parsed(path).map_err(|e| ModuleLoadError::Io {
            path: path.into(),
            error: io::Error::other(e.to_string()),
        })?;
        self.dependencies.push(self.db.files[path].parse);
        for (source, _) in parsed.sources.iter() {
            self.paths.insert(source, path.into());
        }
        if let Ok(tree) = &parsed.result {
            self.cost = self.cost.saturating_add(tree.nodes().len());
        }
        Ok(parsed.as_ref().clone())
    }
}
pub struct AnalysisJob {
    ticket: Ticket,
    module: Arc<ModuleSnapshot>,
    binding: BindingLimits,
    typing: CheckLimits,
}
pub struct CompletedAnalysis {
    ticket: Ticket,
    snapshot: Arc<AnalysisSnapshot>,
}
impl AnalysisJob {
    /// Can run outside the database; publication must still return through its
    /// ticket guard after edits, close/reopen, or newer work.
    pub fn compute(self) -> CompletedAnalysis {
        let mut diagnostics = self.module.diagnostics.clone();
        let mut failure = self.module.error.clone().map(AnalysisFailure::Module);
        let mut valid = false;
        if failure.is_none()
            && let Some(graph) = &self.module.graph
        {
            match bind_program(graph, &self.module.sources, self.binding) {
                Err(error) => failure = Some(AnalysisFailure::Binding(error)),
                Ok(bound) => {
                    diagnostics.extend(bound.diagnostics);
                    if let Some(program) = bound.program {
                        match check_program(program, &self.module.sources, self.typing) {
                            Err(error) => failure = Some(AnalysisFailure::Typing(error)),
                            Ok(checked) => {
                                diagnostics.extend(checked.diagnostics);
                                valid = checked.program.is_some();
                            }
                        }
                    }
                }
            }
        }
        failure = match failure {
            Some(AnalysisFailure::Binding(BindingError::Limit(d)))
            | Some(AnalysisFailure::Typing(
                CheckError::Limit(d)
                | CheckError::Layout(LayoutError::Limit(d))
                | CheckError::Types(lang_hir::types::Error::Limit(d)),
            )) => {
                diagnostics.push(*d);
                None
            }
            Some(AnalysisFailure::Module(error)) => match error.as_ref() {
                ModuleLoadError::Limit(d)
                | ModuleLoadError::Parse(
                    ParseError::Limit(d) | ParseError::Lex(lang_syntax::LexError::Limit(d)),
                ) => {
                    diagnostics.push(d.as_ref().clone());
                    None
                }
                _ => Some(AnalysisFailure::Module(error)),
            },
            failure => failure,
        };
        let snapshot = Arc::new(AnalysisSnapshot {
            sources: self.module.sources.clone(),
            paths: self.module.paths.clone(),
            diagnostics,
            status: if valid {
                AnalysisStatus::Valid
            } else {
                AnalysisStatus::Diagnostics
            },
            error: failure,
        });
        CompletedAnalysis {
            ticket: self.ticket,
            snapshot,
        }
    }
}
