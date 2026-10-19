//! Host-side source loading and module namespaces for accepted RFC 0006.
//! Loading grants no runtime capability. All declaration names are collected
//! before import resolution; local body resolution and typing are subsequent stages.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lang_diagnostics::{Diagnostic, Label, Severity, SourceId, SourceMap, Span, codes};
use lang_syntax::{
    InputError, NodeId, ParseError, ParseLimits, SyntaxElement, SyntaxKind as S, SyntaxTree,
    TokenKind as K, add_source, parse,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModuleLimits {
    pub parse: ParseLimits,
    pub total_source_bytes: usize,
    pub modules: usize,
    pub functions: usize,
    pub definitions: usize,
    pub imports: usize,
    pub diagnostics: usize,
    /// Charges declarations, path segments, alias dependency visits, and
    /// ancestor checks, including repeated visits during alias resolution.
    pub work: usize,
}

impl Default for ModuleLimits {
    fn default() -> Self {
        Self {
            parse: ParseLimits::default(),
            total_source_bytes: 16 * 1024 * 1024,
            modules: 256,
            functions: 4096,
            definitions: 4096,
            imports: 4096,
            diagnostics: 4096,
            work: 10_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ModuleId(usize);
impl ModuleId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ItemId(usize);
impl ItemId {
    pub const fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvedName {
    Item(ItemId),
    Case { definition: ItemId, index: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclaredName {
    pub text: String,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ItemKind {
    Function,
    Host,
    Record,
    Enum {
        cases: Vec<DeclaredName>,
    },
    Module {
        module: Option<ModuleId>,
    },
    Import {
        path: NodeId,
        target: Option<ResolvedName>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Item {
    id: ItemId,
    module: ModuleId,
    name: DeclaredName,
    qualified_name: String,
    public: bool,
    syntax: NodeId,
    kind: ItemKind,
}

impl Item {
    pub const fn id(&self) -> ItemId {
        self.id
    }
    pub const fn module(&self) -> ModuleId {
        self.module
    }
    pub fn name(&self) -> &DeclaredName {
        &self.name
    }
    pub fn qualified_name(&self) -> &str {
        &self.qualified_name
    }
    pub const fn is_public(&self) -> bool {
        self.public
    }
    pub const fn syntax(&self) -> NodeId {
        self.syntax
    }
    pub fn kind(&self) -> &ItemKind {
        &self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SourcePath {
    root: bool,
    segments: Vec<DeclaredName>,
    span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    id: ModuleId,
    parent: Option<ModuleId>,
    hierarchy: Vec<String>,
    source: SourceId,
    tree: Arc<SyntaxTree>,
    items: Vec<ItemId>,
    names: BTreeMap<String, ItemId>,
    paths: BTreeMap<usize, SourcePath>,
}

impl Module {
    pub const fn id(&self) -> ModuleId {
        self.id
    }
    pub const fn parent(&self) -> Option<ModuleId> {
        self.parent
    }
    pub fn hierarchy(&self) -> &[String] {
        &self.hierarchy
    }
    pub const fn source(&self) -> SourceId {
        self.source
    }
    pub fn tree(&self) -> &SyntaxTree {
        &self.tree
    }
    pub fn items(&self) -> &[ItemId] {
        &self.items
    }
    pub fn find(&self, name: &str) -> Option<ItemId> {
        self.names.get(name).copied()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleGraph {
    modules: Vec<Module>,
    items: Vec<Item>,
}

impl ModuleGraph {
    pub const fn root(&self) -> ModuleId {
        ModuleId(0)
    }
    pub fn modules(&self) -> &[Module] {
        &self.modules
    }
    pub fn items(&self) -> &[Item] {
        &self.items
    }
    pub fn module(&self, id: ModuleId) -> Option<&Module> {
        self.modules.get(id.0)
    }
    pub fn item(&self, id: ItemId) -> Option<&Item> {
        self.items.get(id.0)
    }

    /// Resolves a path node in its own parsed module. Local values must be
    /// checked first by body analysis. Work is explicit and bounded per request.
    pub fn resolve(
        &self,
        module: ModuleId,
        path: NodeId,
        work: usize,
    ) -> Result<ResolvedName, ResolutionError> {
        let mut remaining = work;
        self.resolve_charged(module, path, &mut remaining)
    }

    pub(crate) fn resolve_charged(
        &self,
        module: ModuleId,
        path: NodeId,
        remaining: &mut usize,
    ) -> Result<ResolvedName, ResolutionError> {
        let source_path = self
            .module(module)
            .and_then(|m| m.paths.get(&path.index()))
            .ok_or(ResolutionError::InvalidInput)?;
        let mut work = Work { left: *remaining };
        let result = self.resolve_path(module, source_path, &mut work);
        *remaining = work.left;
        match result? {
            Step::Found(name) => Ok(name),
            Step::Alias(_) => Err(ResolutionError::InvalidInput),
        }
    }

    fn resolve_path(
        &self,
        origin: ModuleId,
        path: &SourcePath,
        work: &mut Work,
    ) -> Result<Step, ResolutionError> {
        let mut module = if path.root { self.root() } else { origin };
        for (index, segment) in path.segments.iter().enumerate() {
            work.charge(segment.span)?;
            let Some(id) = self.modules[module.0].find(&segment.text) else {
                return Err(resolution_error(
                    segment.span,
                    format!("unknown module item `{}`", segment.text),
                    None,
                ));
            };
            let item = &self.items[id.0];
            if item.module != origin && !item.public {
                return Err(resolution_error(
                    segment.span,
                    format!("`{}` is private", item.qualified_name),
                    Some(item.name.span),
                ));
            }
            let mut resolved = ResolvedName::Item(id);
            if let ItemKind::Import { target, .. } = item.kind {
                let Some(target) = target else {
                    return Ok(Step::Alias(id));
                };
                resolved = target;
            }
            if index + 1 == path.segments.len() {
                return Ok(Step::Found(resolved));
            }
            let ResolvedName::Item(target) = resolved else {
                return Err(resolution_error(
                    segment.span,
                    "enum cases cannot contain path segments",
                    None,
                ));
            };
            match &self.items[target.0].kind {
                ItemKind::Module {
                    module: Some(child),
                } => module = *child,
                ItemKind::Enum { cases } if index + 2 == path.segments.len() => {
                    let case = &path.segments[index + 1];
                    work.charge(case.span)?;
                    for (case_index, declared) in cases.iter().enumerate() {
                        work.charge(case.span)?;
                        if declared.text == case.text {
                            return Ok(Step::Found(ResolvedName::Case {
                                definition: target,
                                index: case_index,
                            }));
                        }
                    }
                    return Err(resolution_error(
                        case.span,
                        format!("unknown case `{}`", case.text),
                        Some(self.items[target.0].name.span),
                    ));
                }
                _ => {
                    return Err(resolution_error(
                        segment.span,
                        "only modules and enum cases may extend a path",
                        Some(self.items[target.0].name.span),
                    ));
                }
            }
        }
        Err(ResolutionError::InvalidInput)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResolutionError {
    InvalidInput,
    Diagnostic(Box<Diagnostic>),
    Limit(Box<Diagnostic>),
}

impl Display for ResolutionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput => f.write_str("invalid module/path ID"),
            Self::Diagnostic(d) | Self::Limit(d) => write!(f, "{}: {}", d.code.as_str(), d.message),
        }
    }
}
impl Error for ResolutionError {}

fn resolution_error(
    span: Span,
    message: impl Into<String>,
    original: Option<Span>,
) -> ResolutionError {
    let message = message.into();
    let mut diagnostic = Diagnostic::new(
        codes::MODULE_ERROR,
        Severity::Error,
        &message,
        Label::new(span, message.clone()),
    );
    if let Some(original) = original {
        diagnostic
            .secondary
            .push(Label::new(original, "declared here"));
    }
    ResolutionError::Diagnostic(Box::new(diagnostic))
}

struct Work {
    left: usize,
}
impl Work {
    fn charge(&mut self, span: Span) -> Result<(), ResolutionError> {
        if self.left == 0 {
            return Err(ResolutionError::Limit(Box::new(Diagnostic::new(
                codes::RESOURCE_LIMIT,
                Severity::Error,
                "module resolution work limit exceeded",
                Label::new(span, "work limit reached"),
            ))));
        }
        self.left -= 1;
        Ok(())
    }
}

enum Step {
    Found(ResolvedName),
    Alias(ItemId),
}

#[derive(Debug)]
pub enum ModuleLoadError {
    Io { path: PathBuf, error: io::Error },
    OutsideRoot { path: PathBuf },
    NonUtf8Path { path: PathBuf },
    Input { path: PathBuf, error: InputError },
    Parse(ParseError),
    Limit(Box<Diagnostic>),
    InvalidConfiguration,
}

impl Display for ModuleLoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, error } => write!(f, "{}: {error}", path.display()),
            Self::OutsideRoot { path } => write!(
                f,
                "source path is outside the explicit root: {}",
                path.display()
            ),
            Self::NonUtf8Path { path } => {
                write!(f, "source display path is not UTF-8: {}", path.display())
            }
            Self::Input { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Parse(error) => Display::fmt(error, f),
            Self::Limit(d) => write!(f, "{}: {}", d.code.as_str(), d.message),
            Self::InvalidConfiguration => f.write_str("module limit must permit the root module"),
        }
    }
}
impl Error for ModuleLoadError {}

#[derive(Debug)]
pub struct ModuleAnalysis {
    pub graph: Option<ModuleGraph>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Canonicalizes the explicit source root and every file before reading.
/// Reads at most the remaining per-file/total limit plus one detection byte.
/// Duplicate canonical files (including ancestor cycles) are never reread.
pub fn load_modules(
    sources: &mut SourceMap,
    root_file: &Path,
    source_root: &Path,
    limits: ModuleLimits,
) -> Result<ModuleAnalysis, ModuleLoadError> {
    if limits.modules == 0 {
        return Err(ModuleLoadError::InvalidConfiguration);
    }
    let root = canonicalize(source_root)?;
    if !root.is_dir() {
        return Err(ModuleLoadError::Io {
            path: root,
            error: io::Error::new(
                io::ErrorKind::InvalidInput,
                "source root is not a directory",
            ),
        });
    }
    let file = canonicalize(root_file)?;
    if !file.starts_with(&root) {
        return Err(ModuleLoadError::OutsideRoot { path: file });
    }
    load_at(sources, root, file, None, true, limits, None)
}

/// Analyze a host-supplied root snapshot without creating a temporary file.
/// Its display path is relative and conveys no read authority. File modules
/// are rejected unless a canonical source-root directory is explicitly given.
pub fn load_virtual(
    sources: &mut SourceMap,
    display: &Path,
    bytes: &[u8],
    source_root: Option<&Path>,
    limits: ModuleLimits,
) -> Result<ModuleAnalysis, ModuleLoadError> {
    if limits.modules == 0
        || display.as_os_str().is_empty()
        || !display
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
    {
        return Err(ModuleLoadError::InvalidConfiguration);
    }
    let root = match source_root {
        Some(root) => {
            let root = canonicalize(root)?;
            if !root.is_dir() {
                return Err(ModuleLoadError::InvalidConfiguration);
            }
            root
        }
        None => PathBuf::new(),
    };
    let file = root.join(display);
    load_at(
        sources,
        root,
        file,
        Some(bytes),
        source_root.is_some(),
        limits,
        None,
    )
}

#[derive(Clone, Debug)]
pub enum ParseFailure {
    Input(InputError),
    Parse(ParseError),
}
#[derive(Clone, Debug)]
pub struct ProvidedParse {
    pub sources: SourceMap,
    pub result: Result<Arc<SyntaxTree>, ParseFailure>,
    pub limits: ParseLimits,
}
/// Compiler hosts resolve canonical identities before reads and supply parsed
/// immutable snapshots. A provider must enforce the explicit source root before
/// filesystem IO; supplied virtual text itself grants no filesystem authority.
pub trait ModuleProvider {
    fn resolve(
        &mut self,
        requested: &Path,
        source_root: Option<&Path>,
    ) -> Result<PathBuf, ModuleLoadError>;
    fn parsed(
        &mut self,
        path: &Path,
        limits: ParseLimits,
    ) -> Result<ProvidedParse, ModuleLoadError>;
}
pub fn load_preparsed(
    sources: &mut SourceMap,
    root_file: &Path,
    source_root: Option<&Path>,
    limits: ModuleLimits,
    provider: &mut dyn ModuleProvider,
) -> Result<ModuleAnalysis, ModuleLoadError> {
    if limits.modules == 0 {
        return Err(ModuleLoadError::InvalidConfiguration);
    }
    let root = source_root
        .map(canonicalize)
        .transpose()?
        .unwrap_or_default();
    if source_root.is_some() && !root.is_dir() {
        return Err(ModuleLoadError::InvalidConfiguration);
    }
    let file = provider.resolve(root_file, source_root.map(|_| root.as_path()))?;
    if source_root.is_some() && !file.starts_with(&root) {
        return Err(ModuleLoadError::OutsideRoot { path: file });
    }
    load_at(
        sources,
        root,
        file,
        None,
        source_root.is_some(),
        limits,
        Some(provider),
    )
}

fn load_at(
    sources: &mut SourceMap,
    root: PathBuf,
    file: PathBuf,
    root_bytes: Option<&[u8]>,
    allow_files: bool,
    limits: ModuleLimits,
    provider: Option<&mut dyn ModuleProvider>,
) -> Result<ModuleAnalysis, ModuleLoadError> {
    let mut loader = Loader {
        sources,
        root,
        limits,
        graph: ModuleGraph {
            modules: Vec::new(),
            items: Vec::new(),
        },
        canonical: BTreeMap::new(),
        diagnostics: Vec::new(),
        work: Work { left: limits.work },
        bytes: 0,
        tokens: 0,
        nodes: 0,
        functions: 0,
        planned_modules: 1,
        definitions: 0,
        imports: 0,
        provider,
    };
    if loader.provider.is_some() {
        loader.load_cached(file, None, Vec::new())?;
    } else if let Some(bytes) = root_bytes {
        loader.load_bytes(file, bytes, None, Vec::new())?;
    } else {
        loader.load_file(file, None, Vec::new())?;
    }
    let mut jobs = loader.collect_items(ModuleId(0))?;
    while let Some(job) = jobs.pop() {
        if !allow_files {
            loader.report_module(
                job.span,
                "file modules require an explicit source root",
                None,
            )?;
            continue;
        }
        let requested = loader
            .root
            .join(job.hierarchy.iter().collect::<PathBuf>())
            .with_extension("lang");
        let resolved = if let Some(provider) = loader.provider.as_mut() {
            provider.resolve(&requested, Some(&loader.root))
        } else {
            canonicalize(&requested)
        };
        let path = match resolved {
            Ok(path) => path,
            Err(error) => {
                loader.report_module(job.span, format!("cannot load module: {error}"), None)?;
                continue;
            }
        };
        if !path.starts_with(&loader.root) {
            loader.report_module(
                job.span,
                "module canonical path escapes the source root",
                None,
            )?;
            continue;
        }
        if let Some(previous) = loader.canonical.get(&path).copied() {
            let mut ancestor = Some(job.parent);
            let mut cycle = false;
            while let Some(id) = ancestor {
                loader.charge(job.span)?;
                if id == previous {
                    cycle = true;
                    break;
                }
                ancestor = loader.graph.modules[id.0].parent;
            }
            let original = loader.graph.modules[previous.0]
                .tree
                .node(loader.graph.modules[previous.0].tree.root())
                .unwrap()
                .span();
            loader.report_module(
                job.span,
                if cycle {
                    "module cycle through a canonical file"
                } else {
                    "duplicate canonical module file"
                },
                Some(original),
            )?;
            continue;
        }
        let loaded = if loader.provider.is_some() {
            loader.load_cached(path, Some(job.parent), job.hierarchy)
        } else {
            loader.load_file(path, Some(job.parent), job.hierarchy)
        };
        let id = match loaded {
            Ok(id) => id,
            Err(ModuleLoadError::Io { path, error }) => {
                loader.report_module(
                    job.span,
                    format!("cannot read {}: {error}", path.display()),
                    None,
                )?;
                continue;
            }
            Err(error) => return Err(error),
        };
        loader.graph.items[job.item.0].kind = ItemKind::Module { module: Some(id) };
        jobs.extend(loader.collect_items(id)?);
    }
    if !loader
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error)
    {
        loader.resolve_aliases()?;
    }
    let successful = !loader
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
    Ok(ModuleAnalysis {
        graph: successful.then_some(loader.graph),
        diagnostics: loader.diagnostics,
    })
}

fn canonicalize(path: &Path) -> Result<PathBuf, ModuleLoadError> {
    std::fs::canonicalize(path).map_err(|error| ModuleLoadError::Io {
        path: path.to_path_buf(),
        error,
    })
}

struct Job {
    parent: ModuleId,
    item: ItemId,
    hierarchy: Vec<String>,
    span: Span,
}

struct Loader<'a, 'p> {
    sources: &'a mut SourceMap,
    root: PathBuf,
    limits: ModuleLimits,
    graph: ModuleGraph,
    canonical: BTreeMap<PathBuf, ModuleId>,
    diagnostics: Vec<Diagnostic>,
    work: Work,
    bytes: usize,
    tokens: usize,
    nodes: usize,
    functions: usize,
    planned_modules: usize,
    definitions: usize,
    imports: usize,
    provider: Option<&'p mut dyn ModuleProvider>,
}

impl Loader<'_, '_> {
    fn charge(&mut self, span: Span) -> Result<(), ModuleLoadError> {
        self.work.charge(span).map_err(|error| match error {
            ResolutionError::Limit(d) => ModuleLoadError::Limit(d),
            _ => unreachable!(),
        })
    }

    fn limit(&self, span: Span, message: &str) -> ModuleLoadError {
        ModuleLoadError::Limit(Box::new(Diagnostic::new(
            codes::RESOURCE_LIMIT,
            Severity::Error,
            message,
            Label::new(span, message),
        )))
    }

    fn report(&mut self, diagnostic: Diagnostic) -> Result<(), ModuleLoadError> {
        if self.diagnostics.len() >= self.limits.diagnostics {
            return Err(self.limit(diagnostic.primary.span, "module diagnostic limit exceeded"));
        }
        self.diagnostics.push(diagnostic);
        Ok(())
    }

    fn report_module(
        &mut self,
        span: Span,
        message: impl Into<String>,
        original: Option<Span>,
    ) -> Result<(), ModuleLoadError> {
        let ResolutionError::Diagnostic(diagnostic) = resolution_error(span, message, original)
        else {
            unreachable!()
        };
        self.report(*diagnostic)
    }

    fn load_file(
        &mut self,
        file: PathBuf,
        parent: Option<ModuleId>,
        hierarchy: Vec<String>,
    ) -> Result<ModuleId, ModuleLoadError> {
        let capacity = self
            .limits
            .parse
            .lex
            .source_bytes
            .min(self.limits.total_source_bytes.saturating_sub(self.bytes));
        let mut bytes = Vec::new();
        File::open(&file)
            .and_then(|input| {
                input
                    .take(capacity.saturating_add(1) as u64)
                    .read_to_end(&mut bytes)
            })
            .map_err(|error| ModuleLoadError::Io {
                path: file.clone(),
                error,
            })?;
        self.load_bytes(file, &bytes, parent, hierarchy)
    }

    fn load_cached(
        &mut self,
        file: PathBuf,
        parent: Option<ModuleId>,
        hierarchy: Vec<String>,
    ) -> Result<ModuleId, ModuleLoadError> {
        let provided = self
            .provider
            .as_mut()
            .expect("provider configured")
            .parsed(&file, self.limits.parse)?;
        if provided.limits != self.limits.parse {
            return Err(ModuleLoadError::InvalidConfiguration);
        }
        self.sources.include(&provided.sources);
        let tree = provided.result.map_err(|error| match error {
            ParseFailure::Input(error) => ModuleLoadError::Input {
                path: file.clone(),
                error,
            },
            ParseFailure::Parse(error) => ModuleLoadError::Parse(error),
        })?;
        self.install_tree(file, tree, parent, hierarchy)
    }

    fn load_bytes(
        &mut self,
        file: PathBuf,
        bytes: &[u8],
        parent: Option<ModuleId>,
        hierarchy: Vec<String>,
    ) -> Result<ModuleId, ModuleLoadError> {
        let relative = file
            .strip_prefix(&self.root)
            .expect("source root checked before loading");
        let display = relative
            .to_str()
            .ok_or_else(|| ModuleLoadError::NonUtf8Path { path: file.clone() })?
            .replace(std::path::MAIN_SEPARATOR, "/");
        let capacity = self
            .limits
            .parse
            .lex
            .source_bytes
            .min(self.limits.total_source_bytes.saturating_sub(self.bytes));
        let mut parse_limits = self.limits.parse;
        parse_limits.lex.source_bytes = capacity;
        parse_limits.lex.tokens = parse_limits.lex.tokens.saturating_sub(self.tokens);
        parse_limits.nodes = parse_limits.nodes.saturating_sub(self.nodes);
        let source =
            add_source(self.sources, display, bytes, parse_limits.lex).map_err(|error| {
                ModuleLoadError::Input {
                    path: file.clone(),
                    error,
                }
            })?;
        let tree = parse(self.sources, source, parse_limits).map_err(ModuleLoadError::Parse)?;
        self.install_tree(file, Arc::new(tree), parent, hierarchy)
    }

    fn install_tree(
        &mut self,
        file: PathBuf,
        tree: Arc<SyntaxTree>,
        parent: Option<ModuleId>,
        hierarchy: Vec<String>,
    ) -> Result<ModuleId, ModuleLoadError> {
        let source = tree.source();
        let bytes = self
            .sources
            .file(source)
            .map_err(|e| ModuleLoadError::Parse(ParseError::Lex(lang_syntax::LexError::Source(e))))?
            .text()
            .len();
        let span = tree.node(tree.root()).unwrap().span();
        if bytes > self.limits.parse.lex.source_bytes
            || bytes > self.limits.total_source_bytes.saturating_sub(self.bytes)
            || tree.tokens().len() > self.limits.parse.lex.tokens.saturating_sub(self.tokens)
            || tree.nodes().len() > self.limits.parse.nodes.saturating_sub(self.nodes)
        {
            return Err(self.limit(span, "cumulative parsed module limit exceeded"));
        }
        self.bytes += bytes;
        self.tokens += tree.tokens().len();
        self.nodes += tree.nodes().len();
        for diagnostic in tree.diagnostics() {
            self.report(diagnostic.clone())?;
        }
        let syntax_valid = !tree.has_errors();
        let mut paths = BTreeMap::new();
        for (index, node) in tree.nodes().iter().enumerate() {
            self.charge(node.span())?;
            if syntax_valid
                && matches!(
                    node.kind(),
                    S::ForeignImport | S::ForeignExport | S::UnsafeBlock
                )
            {
                // Syntax support must not silently omit C declarations or
                // route them through RFC 0006's unrelated host-import ABI.
                self.report(Diagnostic::new(
                    codes::FOREIGN_BOUNDARY,
                    Severity::Error,
                    "this source profile does not yet support C declarations or unsafe blocks",
                    Label::new(node.span(), "unsupported foreign source boundary"),
                ))?;
            }
            if node.kind() == S::Path {
                let mut segments = Vec::new();
                let mut root = false;
                for element in node.children() {
                    self.charge(node.span())?;
                    if let SyntaxElement::Token(index) = element {
                        let token = tree.tokens()[*index];
                        match token.kind {
                            K::Root => root = true,
                            K::Identifier => segments.push(DeclaredName {
                                text: self
                                    .sources
                                    .slice(token.span)
                                    .expect("parser source")
                                    .into(),
                                span: token.span,
                            }),
                            _ => {}
                        }
                    }
                }
                paths.insert(
                    index,
                    SourcePath {
                        root,
                        segments,
                        span: node.span(),
                    },
                );
            }
        }
        let id = ModuleId(self.graph.modules.len());
        self.canonical.insert(file, id);
        self.graph.modules.push(Module {
            id,
            parent,
            hierarchy,
            source,
            tree,
            items: Vec::new(),
            names: BTreeMap::new(),
            paths,
        });
        Ok(id)
    }

    fn collect_items(&mut self, module: ModuleId) -> Result<Vec<Job>, ModuleLoadError> {
        let mut jobs = Vec::new();
        let tree = &self.graph.modules[module.0].tree;
        if tree.has_errors() {
            return Ok(jobs);
        }
        let root = tree.root();
        let declarations: Vec<_> = child_nodes(tree, root).collect();
        for syntax in declarations {
            let tree = &self.graph.modules[module.0].tree;
            let node = tree.node(syntax).unwrap();
            let span = node.span();
            let is_public = node
                .children()
                .iter()
                .any(|e| matches!(e, SyntaxElement::Token(i) if tree.tokens()[*i].kind == K::Pub));
            let path =
                child_nodes(tree, syntax).find(|id| tree.node(*id).unwrap().kind() == S::Path);
            let name = if node.kind() == S::Import {
                self.graph.modules[module.0].paths[&path.unwrap().index()]
                    .segments
                    .last()
                    .unwrap()
                    .clone()
            } else {
                declaration_name(self.sources, tree, syntax)
            };
            let kind = match node.kind() {
                S::Function => ItemKind::Function,
                S::Host => ItemKind::Host,
                S::Record => ItemKind::Record,
                S::Enum => ItemKind::Enum {
                    cases: child_nodes(tree, syntax)
                        .filter(|id| tree.node(*id).unwrap().kind() == S::EnumCase)
                        .map(|id| declaration_name(self.sources, tree, id))
                        .collect(),
                },
                S::Module => ItemKind::Module { module: None },
                S::Import => ItemKind::Import {
                    path: path.unwrap(),
                    target: None,
                },
                _ => continue,
            };
            self.charge(span)?;
            if name.text == "core" {
                self.report(Diagnostic::new(
                    codes::UNSUPPORTED_SPECIALIZATION,
                    Severity::Error,
                    "core is a reserved root namespace; rename this declaration or import",
                    Label::new(name.span, "reserved core namespace"),
                ))?;
                continue;
            }
            let (used, maximum, description) = match kind {
                ItemKind::Function => (
                    &mut self.functions,
                    self.limits.functions,
                    "function declaration limit exceeded",
                ),
                ItemKind::Record | ItemKind::Enum { .. } => (
                    &mut self.definitions,
                    self.limits.definitions,
                    "nominal definition limit exceeded",
                ),
                ItemKind::Import { .. } | ItemKind::Host => (
                    &mut self.imports,
                    self.limits.imports,
                    "import declaration limit exceeded",
                ),
                ItemKind::Module { .. } => (
                    &mut self.planned_modules,
                    self.limits.modules,
                    "module count limit exceeded",
                ),
            };
            if *used >= maximum {
                return Err(self.limit(span, description));
            }
            *used += 1;
            if let Some(previous) = self.graph.modules[module.0].find(&name.text) {
                self.report_module(
                    name.span,
                    format!("duplicate module name `{}`", name.text),
                    Some(self.graph.items[previous.0].name.span),
                )?;
                continue;
            }
            let qualified_name = self.graph.modules[module.0]
                .hierarchy
                .iter()
                .map(String::as_str)
                .chain(std::iter::once(name.text.as_str()))
                .collect::<Vec<_>>()
                .join("::");
            let id = ItemId(self.graph.items.len());
            if matches!(kind, ItemKind::Module { .. }) {
                let mut hierarchy = self.graph.modules[module.0].hierarchy.clone();
                hierarchy.push(name.text.clone());
                jobs.push(Job {
                    parent: module,
                    item: id,
                    hierarchy,
                    span: name.span,
                });
            }
            if let ItemKind::Enum { cases } = &kind {
                let mut seen = BTreeMap::new();
                for case in cases {
                    self.charge(case.span)?;
                    if let Some(previous) = seen.insert(&case.text, case.span) {
                        let mut diagnostic = Diagnostic::new(
                            codes::INVALID_SHAPE,
                            Severity::Error,
                            "duplicate enum case",
                            Label::new(case.span, "duplicate case"),
                        );
                        diagnostic
                            .secondary
                            .push(Label::new(previous, "first case"));
                        self.report(diagnostic)?;
                    }
                }
            }
            self.graph.modules[module.0]
                .names
                .insert(name.text.clone(), id);
            self.graph.modules[module.0].items.push(id);
            self.graph.items.push(Item {
                id,
                module,
                name,
                qualified_name,
                public: is_public,
                syntax,
                kind,
            });
        }
        Ok(jobs)
    }

    fn resolve_aliases(&mut self) -> Result<(), ModuleLoadError> {
        let aliases: Vec<_> = self
            .graph
            .items
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Import { .. }))
            .map(|item| item.id)
            .collect();
        let mut state = vec![0_u8; self.graph.items.len()];
        for alias in aliases {
            if state[alias.0] != 0 {
                continue;
            }
            let mut pending = vec![alias];
            state[alias.0] = 1;
            while let Some(id) = pending.last().copied() {
                let item = &self.graph.items[id.0];
                let module = item.module;
                let ItemKind::Import { path, .. } = item.kind else {
                    unreachable!()
                };
                let path = &self.graph.modules[module.0].paths[&path.index()];
                let span = path.span;
                self.work.charge(span).map_err(|error| match error {
                    ResolutionError::Limit(d) => ModuleLoadError::Limit(d),
                    _ => unreachable!(),
                })?;
                match self.graph.resolve_path(module, path, &mut self.work) {
                    Ok(Step::Found(target)) => {
                        let ItemKind::Import { path, .. } = self.graph.items[id.0].kind else {
                            unreachable!()
                        };
                        self.graph.items[id.0].kind = ItemKind::Import {
                            path,
                            target: Some(target),
                        };
                        state[id.0] = 2;
                        pending.pop();
                    }
                    Ok(Step::Alias(dependency)) if state[dependency.0] == 0 => {
                        state[dependency.0] = 1;
                        pending.push(dependency);
                    }
                    Ok(Step::Alias(dependency)) => {
                        self.report_module(
                            span,
                            if state[dependency.0] == 1 {
                                "import alias cycle"
                            } else {
                                "import depends on an unresolved alias"
                            },
                            Some(self.graph.items[dependency.0].name.span),
                        )?;
                        state[id.0] = 3;
                        pending.pop();
                    }
                    Err(ResolutionError::Diagnostic(diagnostic)) => {
                        self.report(*diagnostic)?;
                        state[id.0] = 3;
                        pending.pop();
                    }
                    Err(ResolutionError::Limit(diagnostic)) => {
                        return Err(ModuleLoadError::Limit(diagnostic));
                    }
                    Err(ResolutionError::InvalidInput) => {
                        unreachable!("validated nonempty import path")
                    }
                }
            }
        }
        Ok(())
    }
}

fn declaration_name(sources: &SourceMap, tree: &SyntaxTree, node: NodeId) -> DeclaredName {
    let token = tree
        .node(node)
        .unwrap()
        .children()
        .iter()
        .find_map(|element| match element {
            SyntaxElement::Token(index) if tree.tokens()[*index].kind == K::Identifier => {
                Some(tree.tokens()[*index])
            }
            _ => None,
        })
        .expect("declarations have a name after successful parsing");
    DeclaredName {
        text: sources.slice(token.span).expect("parser snapshot").into(),
        span: token.span,
    }
}

pub(crate) fn child_nodes(tree: &SyntaxTree, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    tree.node(node)
        .into_iter()
        .flat_map(|node| node.children())
        .filter_map(|element| match element {
            SyntaxElement::Node(id) => Some(*id),
            _ => None,
        })
}
