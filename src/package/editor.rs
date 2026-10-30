//! RFC 0039 immutable package editor epochs and resolved navigation.
pub(crate) mod completion;
pub(crate) mod edits;
pub(crate) mod workspace;
use super::{ByteSpan, Diagnostic, graph::Graph, source};
use crate::frontend::{Analysis, CompileLimits};
use lang_diagnostics::{SourceMap, Span};
use lang_hir::resolved::{self as r, BindingKind, DefinitionKind, ExpressionKind, StatementKind};
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
    Parameter,
    Variable,
    Field,
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
    test: Option<usize>,
    versions: BTreeMap<PathBuf, i32>,
    analysis: Analysis,
    modules: Option<ModuleGraph>,
    symbols: Vec<Symbol>,
    occurrences: Vec<Occurrence>,
    indexed_bytes: usize,
    inputs: BTreeMap<PathBuf, Arc<[u8]>>,
    aliases: BTreeMap<PathBuf, PathBuf>,
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
        Self::capture_entry(graph, None, epoch, overlays, limits)
    }

    /// Capture a declared test from the same immutable package input epoch.
    pub fn capture_test(
        graph: &Graph,
        test: usize,
        epoch: u64,
        overlays: &[Overlay],
        limits: Limits,
    ) -> Result<Self, Diagnostic> {
        Self::capture_entry(graph, Some(test), epoch, overlays, limits)
    }

    fn recapture(
        &self,
        graph: &Graph,
        epoch: u64,
        overlays: &[Overlay],
        limits: Limits,
    ) -> Result<Self, Diagnostic> {
        Self::capture_entry(graph, self.test, epoch, overlays, limits)
    }

    pub(super) fn capture_entry(
        graph: &Graph,
        test: Option<usize>,
        epoch: u64,
        overlays: &[Overlay],
        limits: Limits,
    ) -> Result<Self, Diagnostic> {
        if overlays.len() > limits.overlays {
            return Err(capacity());
        }
        let inputs = graph.inputs()?;
        let metadata = graph
            .manifest_inputs()
            .map(|(p, m)| {
                source::identity(m, p)
                    .map_err(|e| Diagnostic::error("PKG0005", e.to_string()).at_path(p))
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        for (path, file) in &inputs.files {
            if metadata.contains(&file.identity) {
                return Err(invalid("editor source shares a manifest identity").at_path(path));
            }
        }
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
        let mut captured = BTreeMap::new();
        for (path, bytes) in graph.manifest_files() {
            captured
                .entry(path.to_path_buf())
                .or_insert_with(|| Arc::from(bytes));
        }
        let by_identity: BTreeMap<_, _> = replacements
            .iter()
            .map(|(p, b)| (inputs.files[p].identity, b))
            .collect();
        for (path, file) in &inputs.files {
            captured.insert(
                path.clone(),
                by_identity
                    .get(&file.identity)
                    .map(|bytes| (*bytes).clone())
                    .unwrap_or_else(|| Arc::from(file.bytes.as_slice())),
            );
        }
        let source::Indexed {
            analysis,
            modules,
            aliases,
        } = source::analyze_indexed(graph, test, &replacements, limits.compiler)?;
        let mut snapshot = Self {
            epoch,
            test,
            versions,
            analysis,
            modules,
            symbols: Vec::new(),
            occurrences: Vec::new(),
            indexed_bytes: 0,
            inputs: captured,
            aliases,
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
    pub fn input(&self, path: &Path) -> Option<&[u8]> {
        self.inputs.get(path).map(AsRef::as_ref)
    }
    /// Original display identity for a captured canonical source alias.
    pub fn display_path<'a>(&'a self, path: &'a Path) -> &'a Path {
        self.aliases.get(path).map(PathBuf::as_path).unwrap_or(path)
    }
    pub(super) fn canonical_path<'a>(&'a self, display: &'a Path) -> &'a Path {
        self.aliases
            .iter()
            .find(|(_, value)| value.as_path() == display)
            .map(|(path, _)| path.as_path())
            .unwrap_or(display)
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
        let path = self.display_path(path);
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
        let checked = self
            .analysis
            .result
            .as_ref()
            .expect("checked editor program");
        let program = checked.source();
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
        let mut fields = BTreeMap::new();
        for (definition, d) in program.definitions.iter().enumerate() {
            if let DefinitionKind::Record(values) = &d.kind {
                for (index, field) in values.iter().enumerate() {
                    spend(&mut work)?;
                    let qualified = format!("{}::{}", d.qualified_name, field.name.text);
                    let id = insert_symbol(
                        &mut self.symbols,
                        &mut self.occurrences,
                        sources,
                        limits,
                        &mut bytes,
                        Declaration {
                            name: &field.name.text,
                            qualified: &qualified,
                            kind: SymbolKind::Field,
                            span: field.name.span,
                            public: d.public,
                        },
                    )?;
                    fields.insert((definition, index), id);
                }
            }
        }
        let mut local_uses = BTreeSet::new();
        for (function_index, function) in program.functions.iter().enumerate() {
            let mut locals = BTreeMap::new();
            for (index, local) in function.locals.iter().enumerate() {
                spend(&mut work)?;
                if local.kind == BindingKind::ArenaOwner {
                    continue;
                }
                let qualified = format!("{}::{}", function.qualified_name, local.name.text);
                let kind = if local.kind == BindingKind::Parameter {
                    SymbolKind::Parameter
                } else {
                    SymbolKind::Variable
                };
                let id = insert_symbol(
                    &mut self.symbols,
                    &mut self.occurrences,
                    sources,
                    limits,
                    &mut bytes,
                    Declaration {
                        name: &local.name.text,
                        qualified: &qualified,
                        kind,
                        span: local.name.span,
                        public: false,
                    },
                )?;
                locals.insert(index, id);
            }
            let facts = &checked.functions()[function_index];
            for (expression_index, expression) in function.expressions.iter().enumerate() {
                spend(&mut work)?;
                if let ExpressionKind::Local(local) = expression.kind {
                    local_uses.insert(span_key(expression.span));
                    if let Some(id) = locals.get(&local.0) {
                        insert_occurrence(
                            &mut self.occurrences,
                            sources,
                            limits,
                            &mut bytes,
                            *id,
                            expression.span,
                        )?;
                    }
                }
                if let ExpressionKind::Field { receiver, name } = &expression.kind
                    && let Some(definition) =
                        nominal(program, facts.expressions()[receiver.0], &mut work)?
                    && let Some(index) = facts.field_index(r::ExpressionId(expression_index))
                    && let Some(id) = fields.get(&(definition.0, index))
                {
                    insert_occurrence(
                        &mut self.occurrences,
                        sources,
                        limits,
                        &mut bytes,
                        *id,
                        name.span,
                    )?;
                }
                if let ExpressionKind::Record {
                    definition,
                    fields: values,
                } = &expression.kind
                    && let Some(order) = facts.record_order(r::ExpressionId(expression_index))
                {
                    for (field, index) in values.iter().zip(order) {
                        spend(&mut work)?;
                        if let Some(id) = fields.get(&(definition.0, *index)) {
                            insert_occurrence(
                                &mut self.occurrences,
                                sources,
                                limits,
                                &mut bytes,
                                *id,
                                field.name.span,
                            )?;
                        }
                    }
                }
            }
            for (pattern_index, pattern) in function.patterns.iter().enumerate() {
                spend(&mut work)?;
                if let r::PatternKind::Record {
                    definition,
                    fields: written,
                } = &pattern.kind
                    && let Some(order) = facts.pattern_fields(r::PatternId(pattern_index))
                {
                    let mut spans = BTreeMap::new();
                    for field in written {
                        spend(&mut work)?;
                        spans.insert(field.pattern, field.name.span);
                    }
                    for (index, pattern) in order.iter().enumerate() {
                        spend(&mut work)?;
                        if let Some(span) = spans.get(pattern)
                            && let Some(id) = fields.get(&(definition.0, index))
                        {
                            insert_occurrence(
                                &mut self.occurrences,
                                sources,
                                limits,
                                &mut bytes,
                                *id,
                                *span,
                            )?;
                        }
                    }
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
                        if let Some(id) = locals.get(&local.0) {
                            insert_occurrence(
                                &mut self.occurrences,
                                sources,
                                limits,
                                &mut bytes,
                                *id,
                                span,
                            )?;
                        }
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
fn nominal(
    program: &r::Program,
    mut ty: r::Type,
    work: &mut usize,
) -> Result<Option<r::DefinitionId>, Diagnostic> {
    loop {
        spend(work)?;
        match ty {
            r::Type::Nominal(id) => return Ok(Some(id)),
            r::Type::Constructed(id) => match program.types[id.0].constructor {
                r::TypeConstructor::Loan(_, inner) => ty = inner,
                _ => return Ok(None),
            },
            _ => return Ok(None),
        }
    }
}
fn insert_occurrence(
    occurrences: &mut Vec<Occurrence>,
    sources: &SourceMap,
    limits: Limits,
    bytes: &mut usize,
    symbol: SymbolId,
    span: Span,
) -> Result<(), Diagnostic> {
    if occurrences.len() == limits.occurrences {
        return Err(capacity());
    }
    let location = location(sources, span)?;
    spend_bytes(bytes, location.path.as_os_str().as_encoded_bytes().len())?;
    occurrences.push(Occurrence {
        symbol,
        location,
        declaration: false,
    });
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
