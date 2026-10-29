//! RFC 0039 immutable package editor epochs and resolved navigation.
use super::{ByteSpan, Diagnostic, graph::Graph, source};
use crate::frontend::{Analysis, CompileLimits};
use lang_diagnostics::{SourceMap, Span};
use lang_hir::resolved::{ExpressionKind, StatementKind};
use lang_semantics::modules::{ItemKind, ModuleGraph, ResolutionError, ResolvedName};
use lang_syntax::{SyntaxElement, SyntaxKind, TokenKind};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy)]
pub struct Limits {
    pub compiler: CompileLimits,
    pub overlays: usize,
    pub overlay_bytes: usize,
    pub index_bytes: usize,
    pub symbols: usize,
    pub occurrences: usize,
    pub work: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            compiler: Default::default(),
            overlays: 256,
            overlay_bytes: 16 * 1024 * 1024,
            index_bytes: 64 * 1024 * 1024,
            symbols: 65_536,
            occurrences: 262_144,
            work: 10_000_000,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Overlay {
    /// Canonical source identity relative to the package authority.
    pub path: PathBuf,
    pub version: i32,
    pub bytes: Arc<[u8]>,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SymbolId(usize);
impl SymbolId {
    pub const fn index(self) -> usize {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SymbolKind {
    Function,
    Host,
    Record,
    Enum,
    EnumCase,
    Module,
    Import,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Location {
    pub path: PathBuf,
    pub span: ByteSpan,
}
#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: SymbolId,
    pub name: String,
    pub qualified_name: String,
    pub kind: SymbolKind,
    pub location: Location,
    pub public: bool,
}
#[derive(Clone, Debug)]
pub struct Occurrence {
    pub symbol: SymbolId,
    pub location: Location,
    pub declaration: bool,
}
#[derive(Debug)]
pub struct Snapshot {
    epoch: u64,
    versions: BTreeMap<PathBuf, i32>,
    analysis: Analysis,
    modules: Option<ModuleGraph>,
    symbols: Vec<Symbol>,
    occurrences: Vec<Occurrence>,
    indexed_bytes: usize,
}
fn capacity() -> Diagnostic {
    Diagnostic::error("PKG0004", "editor snapshot resource limit exceeded")
}
fn invalid(message: &str) -> Diagnostic {
    Diagnostic::error("PKG0003", message)
}
fn location(sources: &SourceMap, span: Span) -> Result<Location, Diagnostic> {
    let file = sources
        .file(span.source())
        .map_err(|e| Diagnostic::error("CLI0002", e.to_string()))?;
    sources
        .slice(span)
        .map_err(|e| Diagnostic::error("CLI0002", e.to_string()))?;
    Ok(Location {
        path: PathBuf::from(file.name()),
        span: ByteSpan {
            start: span.range().start,
            end: span.range().end,
        },
    })
}
impl Snapshot {
    pub fn capture(
        graph: &Graph,
        epoch: u64,
        overlays: &[Overlay],
        limits: Limits,
    ) -> Result<Self, Diagnostic> {
        if overlays.len() > limits.overlays {
            return Err(capacity());
        }
        let inputs = graph.inputs()?;
        let mut bytes = 0usize;
        let mut versions = BTreeMap::new();
        let mut replacements = BTreeMap::new();
        for overlay in overlays {
            if !inputs.files.contains_key(&overlay.path)
                || versions
                    .insert(overlay.path.clone(), overlay.version)
                    .is_some()
            {
                return Err(
                    invalid("overlay must name one unique admitted canonical source")
                        .at_path(&overlay.path),
                );
            }
            bytes = bytes
                .checked_add(overlay.bytes.len())
                .filter(|n| *n <= limits.overlay_bytes)
                .ok_or_else(capacity)?;
            if overlay.bytes.len() > limits.compiler.modules.parse.lex.source_bytes {
                return Err(capacity());
            }
            replacements.insert(overlay.path.clone(), overlay.bytes.clone());
        }
        let (analysis, modules) = source::analyze_indexed(graph, &replacements, limits.compiler)?;
        let mut snapshot = Self {
            epoch,
            versions,
            analysis,
            modules,
            symbols: Vec::new(),
            occurrences: Vec::new(),
            indexed_bytes: 0,
        };
        if snapshot.analysis.result.is_ok() {
            snapshot.index(limits)?;
        }
        Ok(snapshot)
    }
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn versions(&self) -> &BTreeMap<PathBuf, i32> {
        &self.versions
    }
    pub fn analysis(&self) -> &Analysis {
        &self.analysis
    }
    pub fn modules(&self) -> Option<&ModuleGraph> {
        self.modules.as_ref()
    }
    pub fn symbols(&self) -> &[Symbol] {
        &self.symbols
    }
    pub fn occurrences(&self) -> &[Occurrence] {
        &self.occurrences
    }
    /// Logical owned name/path bytes in the bounded symbol/occurrence index.
    pub const fn indexed_bytes(&self) -> usize {
        self.indexed_bytes
    }
    pub fn matches(&self, epoch: u64, versions: &BTreeMap<PathBuf, i32>) -> bool {
        self.epoch == epoch && &self.versions == versions
    }
    pub fn definition(
        &self,
        path: &Path,
        offset: usize,
        current_epoch: u64,
    ) -> Result<Option<&Symbol>, Diagnostic> {
        if current_epoch != self.epoch {
            return Err(Diagnostic::error("PKG0007", "editor epoch is stale"));
        }
        let file = self
            .analysis
            .sources
            .iter()
            .find(|(_, f)| Path::new(f.name()) == path)
            .map(|(_, f)| f)
            .ok_or_else(|| invalid("document is outside the loaded editor snapshot"))?;
        if offset > file.text().len() || !file.text().is_char_boundary(offset) {
            return Err(invalid("invalid UTF-8 source offset"));
        }
        Ok(self
            .occurrences
            .iter()
            .filter(|o| {
                o.location.path == path
                    && o.location.span.start <= offset
                    && offset < o.location.span.end
            })
            .min_by_key(|o| o.location.span.end - o.location.span.start)
            .map(|o| &self.symbols[o.symbol.0]))
    }
    fn index(&mut self, limits: Limits) -> Result<(), Diagnostic> {
        let graph = self
            .modules
            .as_ref()
            .ok_or_else(|| invalid("checked editor module graph is missing"))?;
        let program = self
            .analysis
            .result
            .as_ref()
            .expect("checked editor program")
            .source();
        let sources = &self.analysis.sources;
        let mut work = limits.work;
        let mut bytes = limits.index_bytes;
        let mut targets = BTreeMap::new();
        for item in graph.items() {
            spend(&mut work)?;
            let kind = match item.kind() {
                ItemKind::Function => SymbolKind::Function,
                ItemKind::Host => SymbolKind::Host,
                ItemKind::Record => SymbolKind::Record,
                ItemKind::Enum { .. } => SymbolKind::Enum,
                ItemKind::Module { .. } => SymbolKind::Module,
                ItemKind::Import { .. } => SymbolKind::Import,
            };
            let id = insert_symbol(
                &mut self.symbols,
                &mut self.occurrences,
                sources,
                limits,
                &mut bytes,
                Declaration {
                    name: &item.name().text,
                    qualified: item.qualified_name(),
                    kind,
                    span: item.name().span,
                    public: item.is_public(),
                },
            )?;
            targets.insert((item.id().index(), None), id);
            if let ItemKind::Enum { cases } = item.kind() {
                for (index, case) in cases.iter().enumerate() {
                    spend(&mut work)?;
                    let qualified = format!("{}::{}", item.qualified_name(), case.text);
                    let id = insert_symbol(
                        &mut self.symbols,
                        &mut self.occurrences,
                        sources,
                        limits,
                        &mut bytes,
                        Declaration {
                            name: &case.text,
                            qualified: &qualified,
                            kind: SymbolKind::EnumCase,
                            span: case.span,
                            public: item.is_public(),
                        },
                    )?;
                    targets.insert((item.id().index(), Some(index)), id);
                }
            }
        }
        let mut local_uses = BTreeSet::new();
        for function in &program.functions {
            for expression in &function.expressions {
                spend(&mut work)?;
                if matches!(expression.kind, ExpressionKind::Local(_)) {
                    local_uses.insert(span_key(expression.span));
                }
            }
            for block in &function.blocks {
                for statement in &block.statements {
                    spend(&mut work)?;
                    if let StatementKind::Assign { local, .. } = statement.kind {
                        let name = &function.locals[local.0].name.text;
                        let start = statement.span.range().start;
                        let end = start.checked_add(name.len()).ok_or_else(capacity)?;
                        let span = sources
                            .span(statement.span.source(), start..end)
                            .map_err(|_| invalid("unproven local assignment span"))?;
                        if sources
                            .slice(span)
                            .map_err(|_| invalid("unproven local assignment"))?
                            != name
                        {
                            return Err(invalid("unproven local assignment name"));
                        }
                        local_uses.insert(span_key(span));
                    }
                }
            }
        }
        for module in graph.modules() {
            let tree = module.tree();
            let mut pending = vec![tree.root()];
            while let Some(id) = pending.pop() {
                spend(&mut work)?;
                let node = tree.node(id).expect("validated syntax node");
                for child in node.children() {
                    spend(&mut work)?;
                    if let SyntaxElement::Node(id) = child {
                        pending.push(*id);
                    }
                }
                if node.kind() != SyntaxKind::Path {
                    continue;
                }
                let mut name = None;
                for child in node.children().iter().rev() {
                    spend(&mut work)?;
                    if let SyntaxElement::Token(index) = child {
                        let token = &tree.tokens()[*index];
                        if token.kind == TokenKind::Identifier {
                            name = Some(token.span);
                            break;
                        }
                    }
                }
                let Some(name) = name else { continue };
                if local_uses.contains(&span_key(name)) {
                    continue;
                }
                let target = match graph.resolve_with_budget(module.id(), id, &mut work) {
                    Ok(ResolvedName::Item(id)) => (id.index(), None),
                    Ok(ResolvedName::Case { definition, index }) => {
                        (definition.index(), Some(index))
                    }
                    Err(ResolutionError::Limit(_)) => return Err(capacity()),
                    Err(_) => continue,
                };
                let Some(symbol) = targets.get(&target) else {
                    continue;
                };
                if self.occurrences.len() == limits.occurrences {
                    return Err(capacity());
                }
                let location = location(sources, name)?;
                spend_bytes(
                    &mut bytes,
                    location.path.as_os_str().as_encoded_bytes().len(),
                )?;
                self.occurrences.push(Occurrence {
                    symbol: *symbol,
                    location,
                    declaration: false,
                });
            }
        }
        self.occurrences.sort_by(|a, b| {
            (
                &a.location.path,
                a.location.span.start,
                a.location.span.end,
                a.symbol,
                a.declaration,
            )
                .cmp(&(
                    &b.location.path,
                    b.location.span.start,
                    b.location.span.end,
                    b.symbol,
                    b.declaration,
                ))
        });
        self.occurrences.dedup_by(|a, b| {
            a.symbol == b.symbol && a.location == b.location && a.declaration == b.declaration
        });
        self.indexed_bytes = limits.index_bytes - bytes;
        Ok(())
    }
}
fn spend(work: &mut usize) -> Result<(), Diagnostic> {
    *work = work.checked_sub(1).ok_or_else(capacity)?;
    Ok(())
}
fn spend_bytes(bytes: &mut usize, amount: usize) -> Result<(), Diagnostic> {
    *bytes = bytes.checked_sub(amount).ok_or_else(capacity)?;
    Ok(())
}
fn span_key(span: Span) -> (lang_diagnostics::SourceId, usize, usize) {
    (span.source(), span.range().start, span.range().end)
}
struct Declaration<'a> {
    name: &'a str,
    qualified: &'a str,
    kind: SymbolKind,
    span: Span,
    public: bool,
}
fn insert_symbol(
    symbols: &mut Vec<Symbol>,
    occurrences: &mut Vec<Occurrence>,
    sources: &SourceMap,
    limits: Limits,
    bytes: &mut usize,
    d: Declaration<'_>,
) -> Result<SymbolId, Diagnostic> {
    if symbols.len() == limits.symbols || occurrences.len() == limits.occurrences {
        return Err(capacity());
    }
    let id = SymbolId(symbols.len());
    let location = location(sources, d.span)?;
    spend_bytes(bytes, d.name.len())?;
    spend_bytes(bytes, d.qualified.len())?;
    spend_bytes(bytes, location.path.as_os_str().as_encoded_bytes().len())?;
    if d.kind != SymbolKind::Import {
        spend_bytes(bytes, location.path.as_os_str().as_encoded_bytes().len())?;
        occurrences.push(Occurrence {
            symbol: id,
            location: location.clone(),
            declaration: true,
        });
    }
    symbols.push(Symbol {
        id,
        name: d.name.to_string(),
        qualified_name: d.qualified.to_string(),
        kind: d.kind,
        location,
        public: d.public,
    });
    Ok(id)
}
