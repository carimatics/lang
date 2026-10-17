use lang_diagnostics::{SourceMap, codes};
use lang_semantics::modules::{
    ModuleLimits, ModuleLoadError, ModuleProvider, ParseFailure, ProvidedParse, load_preparsed,
};
use lang_syntax::{ParseLimits, add_source, parse};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Provider {
    root: PathBuf,
    files: BTreeMap<PathBuf, ProvidedParse>,
    aliases: BTreeMap<PathBuf, PathBuf>,
    calls: BTreeMap<PathBuf, usize>,
}
impl Provider {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lang-preparsed-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self {
            root: fs::canonicalize(path).unwrap(),
            files: BTreeMap::new(),
            aliases: BTreeMap::new(),
            calls: BTreeMap::new(),
        }
    }
    fn add(&mut self, name: &str, text: &str, limits: ParseLimits) {
        let mut sources = SourceMap::new();
        let source = add_source(&mut sources, name, text.as_bytes(), limits.lex).unwrap();
        let result = parse(&sources, source, limits)
            .map(Arc::new)
            .map_err(ParseFailure::Parse);
        self.files.insert(
            self.root.join(name),
            ProvidedParse {
                sources,
                result,
                limits,
            },
        );
    }
    fn load(
        &mut self,
        sources: &mut SourceMap,
        limits: ModuleLimits,
    ) -> Result<lang_semantics::modules::ModuleAnalysis, ModuleLoadError> {
        let file = self.root.join("main.lang");
        let root = self.root.clone();
        load_preparsed(sources, &file, Some(&root), limits, self)
    }
}
impl ModuleProvider for Provider {
    fn resolve(
        &mut self,
        requested: &Path,
        root: Option<&Path>,
    ) -> Result<PathBuf, ModuleLoadError> {
        let path = self
            .aliases
            .get(requested)
            .map_or(requested, PathBuf::as_path);
        if root.is_some_and(|root| !path.starts_with(root)) {
            return Err(ModuleLoadError::OutsideRoot { path: path.into() });
        }
        if self.files.contains_key(path) {
            Ok(path.into())
        } else {
            Err(ModuleLoadError::Io {
                path: path.into(),
                error: io::Error::from(io::ErrorKind::NotFound),
            })
        }
    }
    fn parsed(&mut self, path: &Path, _: ParseLimits) -> Result<ProvidedParse, ModuleLoadError> {
        *self.calls.entry(path.into()).or_default() += 1;
        Ok(self.files[path].clone())
    }
}
impl Drop for Provider {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn cached_parser_trees_and_immutable_snapshots_are_actually_shared_by_module_analysis() {
    let mut p = Provider::new();
    p.add("main.lang", "mod math;", ParseLimits::default());
    p.add(
        "math.lang",
        "pub fn value() -> i64 { return 42; }",
        ParseLimits::default(),
    );
    let original = p.files[&p.root.join("math.lang")].clone();
    let tree = original.result.unwrap();
    let source = tree.source();
    let mut sources = SourceMap::new();
    let loaded = p.load(&mut sources, ModuleLimits::default()).unwrap();
    assert!(loaded.diagnostics.is_empty());
    let graph = loaded.graph.unwrap();
    assert!(
        graph
            .modules()
            .iter()
            .any(|m| std::ptr::eq(m.tree(), tree.as_ref()))
    );
    assert!(std::ptr::eq(
        sources.file(source).unwrap(),
        original.sources.file(source).unwrap()
    ));
    let mut cloned = sources.clone();
    let new = cloned.add("new.lang", "new").unwrap();
    assert_ne!(new, source);
    assert!(sources.file(new).is_err());
    assert_eq!(
        cloned
            .slice(tree.node(tree.root()).unwrap().span())
            .unwrap(),
        "pub fn value() -> i64 { return 42; }"
    );
}

#[test]
fn duplicate_canonical_files_are_rejected_before_requesting_the_cached_parse_again() {
    let mut p = Provider::new();
    p.add("main.lang", "mod a; mod b;", ParseLimits::default());
    p.add("shared.lang", "", ParseLimits::default());
    let shared = p.root.join("shared.lang");
    p.aliases.insert(p.root.join("a.lang"), shared.clone());
    p.aliases.insert(p.root.join("b.lang"), shared.clone());
    let loaded = p
        .load(&mut SourceMap::new(), ModuleLimits::default())
        .unwrap();
    assert!(loaded.graph.is_none());
    assert!(
        loaded
            .diagnostics
            .iter()
            .any(|d| d.code == codes::MODULE_ERROR)
    );
    assert_eq!(p.calls[&shared], 1);
}

#[test]
fn cached_trees_still_obey_cumulative_module_source_token_and_node_limits() {
    for which in 0..3 {
        let mut p = Provider::new();
        p.add("main.lang", "mod child;", ParseLimits::default());
        p.add(
            "child.lang",
            "fn value() -> i64 { return 42; }",
            ParseLimits::default(),
        );
        let mut limits = ModuleLimits::default();
        if which == 0 {
            limits.total_source_bytes = 12;
        } else {
            let root = p.files[&p.root.join("main.lang")].result.as_ref().unwrap();
            let child_tree = p.files[&p.root.join("child.lang")].result.as_ref().unwrap();
            if which == 1 {
                limits.parse.lex.tokens = root.tokens().len().max(child_tree.tokens().len());
            } else {
                limits.parse.nodes = root.nodes().len().max(child_tree.nodes().len());
            }
            // Parse configuration is a trust boundary: cached results created
            // with another requested policy cannot bypass a lower limit.
            let root_text = "mod child;";
            let child = "fn value() -> i64 { return 42; }";
            p.add("main.lang", root_text, limits.parse);
            p.add("child.lang", child, limits.parse);
        }
        let mut sources = SourceMap::new();
        assert!(matches!(
            p.load(&mut sources, limits),
            Err(ModuleLoadError::Limit(_))
        ));
    }
}

#[test]
fn parser_resource_failure_preserves_its_original_snapshot_for_diagnostics() {
    let mut p = Provider::new();
    let mut limits = ModuleLimits::default();
    limits.parse.work = 0;
    p.add("main.lang", "fn f() -> i64 { return 1; }", limits.parse);
    let mut sources = SourceMap::new();
    let Err(ModuleLoadError::Parse(lang_syntax::ParseError::Limit(d))) =
        p.load(&mut sources, limits)
    else {
        panic!("expected parser limit")
    };
    d.validate(&sources).unwrap();
    assert!(d.render(&sources).unwrap().contains("main.lang:"));
}

#[test]
fn incomplete_cached_roots_keep_diagnostics_and_cannot_be_a_resolved_graph() {
    let mut p = Provider::new();
    p.add("main.lang", "fn f( {", ParseLimits::default());
    let mut sources = SourceMap::new();
    let loaded = p.load(&mut sources, ModuleLimits::default()).unwrap();
    assert!(loaded.graph.is_none());
    assert!(!loaded.diagnostics.is_empty());
    for d in loaded.diagnostics {
        d.validate(&sources).unwrap();
    }
}

#[test]
fn foreign_cached_sources_and_mismatched_policies_are_rejected() {
    let mut p = Provider::new();
    p.add("main.lang", "", ParseLimits::default());
    p.files.get_mut(&p.root.join("main.lang")).unwrap().sources = SourceMap::new();
    assert!(
        p.load(&mut SourceMap::new(), ModuleLimits::default())
            .is_err()
    );
    p.add(
        "main.lang",
        "",
        ParseLimits {
            depth: 1,
            ..ParseLimits::default()
        },
    );
    assert!(matches!(
        p.load(&mut SourceMap::new(), ModuleLimits::default()),
        Err(ModuleLoadError::InvalidConfiguration)
    ));
}
