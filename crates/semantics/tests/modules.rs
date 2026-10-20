use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lang_diagnostics::{SourceMap, codes};
use lang_semantics::modules::{
    ItemKind, ModuleAnalysis, ModuleGraph, ModuleId, ModuleLimits, ModuleLoadError,
    ResolutionError, ResolvedName, load_modules,
};
use lang_syntax::{InputErrorKind, LexError, ParseError, SyntaxKind};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Files {
    root: PathBuf,
}
impl Files {
    fn new(main: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "lang-modules-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let files = Self { root };
        files.write("main.lang", main.as_bytes());
        files
    }
    fn write(&self, path: &str, contents: &[u8]) {
        let file = self.root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }
    fn load(&self, limits: ModuleLimits) -> Result<(SourceMap, ModuleAnalysis), ModuleLoadError> {
        let mut sources = SourceMap::new();
        let result = load_modules(
            &mut sources,
            &self.root.join("main.lang"),
            &self.root,
            limits,
        );
        let analysis = match result {
            Ok(analysis) => analysis,
            Err(error) => {
                match &error {
                    ModuleLoadError::Limit(d)
                    | ModuleLoadError::Parse(ParseError::Limit(d))
                    | ModuleLoadError::Parse(ParseError::Lex(LexError::Limit(d))) => {
                        d.validate(&sources).unwrap();
                    }
                    _ => {}
                }
                return Err(error);
            }
        };
        for diagnostic in &analysis.diagnostics {
            diagnostic.validate(&sources).unwrap();
        }
        Ok((sources, analysis))
    }
    fn graph(&self) -> (SourceMap, ModuleGraph) {
        let (sources, analysis) = self.load(ModuleLimits::default()).unwrap();
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        (sources, analysis.graph.unwrap())
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn core_namespace_declarations_and_imports_fail_before_loading_reserved_modules() {
    for text in [
        "fn core() -> i64 { return 0; }",
        "struct core { value: i64 }",
        "enum core { A }",
        "extern fn core(value: i64) -> i64;",
        "mod core;",
    ] {
        let files = Files::new(text);
        // No core.lang exists: reporting the reserved name must precede IO.
        let (sources, analysis) = files.load(ModuleLimits::default()).unwrap();
        assert!(analysis.graph.is_none());
        let diagnostic = &analysis.diagnostics[0];
        assert_eq!(diagnostic.code, codes::UNSUPPORTED_SPECIALIZATION, "{text}");
        assert_eq!(sources.slice(diagnostic.primary.span).unwrap(), "core");
    }
    let files = Files::new("mod child; use child::core;");
    files.write("child.lang", b"pub fn core() -> i64 { return 0; }");
    let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
    assert!(analysis.graph.is_none());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == codes::UNSUPPORTED_SPECIALIZATION)
    );
}

#[test]
fn c_source_survives_module_collection_for_typed_binding() {
    for text in [
        "extern \"C\" fn f(x: i64) -> i64; fn main() -> i64 { return 0; }",
        "extern \"C\" fn f() -> core::Unit; fn main() -> i64 { return 0; }",
        "pub extern \"C\" fn f(x: i64) -> i64 { return x; }",
        "fn main() -> i64 { unsafe { return 1; } }",
    ] {
        let files = Files::new(text);
        let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
        assert!(
            analysis.graph.is_some(),
            "{text}: {:?}",
            analysis.diagnostics
        );
        assert!(analysis.diagnostics.is_empty(), "{text}");
    }
    let files = Files::new("extern fn f(x: i64) -> i64; fn main() -> i64 { return f(1); }");
    let (_, graph) = files.graph();
    assert!(
        graph
            .items()
            .iter()
            .any(|item| matches!(item.kind(), ItemKind::Host))
    );
}

fn module(graph: &ModuleGraph, hierarchy: &[&str]) -> ModuleId {
    graph
        .modules()
        .iter()
        .find(|m| {
            m.hierarchy()
                .iter()
                .map(String::as_str)
                .eq(hierarchy.iter().copied())
        })
        .unwrap()
        .id()
}

fn resolve(
    sources: &SourceMap,
    graph: &ModuleGraph,
    from: ModuleId,
    text: &str,
) -> Result<ResolvedName, ResolutionError> {
    let tree = graph.module(from).unwrap().tree();
    let path = tree
        .nodes()
        .iter()
        .enumerate()
        .find(|(_, n)| {
            n.kind() == SyntaxKind::Path && sources.slice(n.span()).unwrap().trim() == text
        })
        .map(|(i, _)| i)
        .unwrap();
    let id = tree
        .nodes()
        .iter()
        .flat_map(|n| n.children())
        .find_map(|element| match element {
            lang_syntax::SyntaxElement::Node(id) if id.index() == path => Some(*id),
            _ => None,
        })
        .unwrap();
    let result = graph.resolve(from, id, 100_000);
    if let Err(ResolutionError::Diagnostic(d) | ResolutionError::Limit(d)) = &result {
        d.validate(sources).unwrap();
    }
    result
}

fn qualified(graph: &ModuleGraph, name: ResolvedName) -> &str {
    let ResolvedName::Item(id) = name else {
        panic!("expected item")
    };
    graph.item(id).unwrap().qualified_name()
}

#[test]
fn explicit_root_hierarchy_collects_forward_functions_before_resolution() {
    let files = Files::new(
        "mod math; fn first() -> i64 { return math::nested::later(); } fn second() -> i64 { return first(); }",
    );
    files.write(
        "math.lang",
        b"pub mod nested; pub fn call() -> i64 { return nested::later(); }",
    );
    files.write("math/nested.lang", b"pub fn later() -> i64 { return 42; }");
    files.write("nested.lang", b"this file must not be read");
    let (sources, graph) = files.graph();
    assert_eq!(graph.modules().len(), 3);
    let name = resolve(&sources, &graph, graph.root(), "math::nested::later").unwrap();
    assert_eq!(qualified(&graph, name), "math::nested::later");
    let math = module(&graph, &["math"]);
    assert_eq!(
        qualified(
            &graph,
            resolve(&sources, &graph, math, "nested::later").unwrap()
        ),
        "math::nested::later"
    );
    assert_eq!(
        qualified(
            &graph,
            resolve(&sources, &graph, graph.root(), "first").unwrap()
        ),
        "first"
    );
    let nested = graph.module(module(&graph, &["math", "nested"])).unwrap();
    assert_eq!(
        sources.file(nested.source()).unwrap().name(),
        "math/nested.lang"
    );
}

#[test]
fn forward_aliases_resolve_module_and_case_targets() {
    let files = Files::new(
        "mod math; use inner::value; use root::math::inner; use math::E::Some; fn f() -> i64 { return value(Some(1)); }",
    );
    files.write(
        "math.lang",
        b"pub mod inner; pub enum E { None, Some(i64) }",
    );
    files.write(
        "math/inner.lang",
        b"pub fn value(x: i64) -> i64 { return x; }",
    );
    let (sources, graph) = files.graph();
    assert_eq!(
        qualified(
            &graph,
            resolve(&sources, &graph, graph.root(), "value").unwrap()
        ),
        "math::inner::value"
    );
    assert!(matches!(
        resolve(&sources, &graph, graph.root(), "Some").unwrap(),
        ResolvedName::Case { index: 1, .. }
    ));
    assert_eq!(
        graph
            .items()
            .iter()
            .filter(|item| matches!(
                item.kind(),
                ItemKind::Import {
                    target: Some(_),
                    ..
                }
            ))
            .count(),
        3
    );
}

#[test]
fn private_items_are_visible_only_in_their_declaring_module() {
    let files = Files::new("mod math; fn f() -> i64 { return math::secret(); }");
    files.write(
        "math.lang",
        b"fn secret() -> i64 { return 1; } pub fn public() -> i64 { return secret(); }",
    );
    let (sources, graph) = files.graph();
    let error = resolve(&sources, &graph, graph.root(), "math::secret").unwrap_err();
    let ResolutionError::Diagnostic(d) = error else {
        panic!("expected visibility diagnostic")
    };
    assert_eq!(d.code, codes::MODULE_ERROR);
    assert_eq!(d.secondary.len(), 1);
    assert_eq!(
        qualified(
            &graph,
            resolve(&sources, &graph, module(&graph, &["math"]), "secret").unwrap()
        ),
        "math::secret"
    );
}

#[test]
fn every_crossed_child_module_boundary_requires_pub() {
    let files = Files::new("mod hidden; pub mod client;");
    files.write("hidden.lang", b"pub fn value() -> i64 { return 1; }");
    files.write(
        "client.lang",
        b"fn f() -> i64 { return root::hidden::value(); }",
    );
    let (sources, graph) = files.graph();
    assert!(matches!(
        resolve(
            &sources,
            &graph,
            module(&graph, &["client"]),
            "root::hidden::value"
        ),
        Err(ResolutionError::Diagnostic(_))
    ));
    files.write("main.lang", b"pub mod hidden; pub mod client;");
    let (sources, graph) = files.graph();
    assert_eq!(
        qualified(
            &graph,
            resolve(
                &sources,
                &graph,
                module(&graph, &["client"]),
                "root::hidden::value"
            )
            .unwrap()
        ),
        "hidden::value"
    );
}

#[test]
fn imports_cannot_reexport_aliases_or_bypass_visibility() {
    let files = Files::new("pub mod math; pub mod client; use math::value;");
    files.write("math.lang", b"pub fn value() -> i64 { return 1; }");
    files.write("client.lang", b"use root::value;");
    let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
    assert!(analysis.graph.is_none());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.message.contains("private"))
    );
    files.write("client.lang", b"use root::math::secret;");
    files.write(
        "math.lang",
        b"fn secret() -> i64 { return 1; } pub fn value() -> i64 { return 1; }",
    );
    let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
    assert!(analysis.graph.is_none());
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.message.contains("private"))
    );
}

#[test]
fn alias_cycles_and_failed_dependencies_do_not_produce_a_graph() {
    for main in ["use a;", "use b::a; use a::b;", "use missing::a; use a::b;"] {
        let files = Files::new(main);
        let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
        assert!(analysis.graph.is_none());
        assert!(!analysis.diagnostics.is_empty());
        assert!(
            analysis
                .diagnostics
                .iter()
                .all(|d| d.code == codes::MODULE_ERROR)
        );
    }
}

#[test]
fn all_item_kinds_and_import_aliases_share_one_namespace() {
    for main in [
        "fn x() -> i64 { return 0; } struct x {}",
        "struct x {} enum x { A }",
        "mod x; fn x() -> i64 { return 0; }",
        "extern fn x(a: i64) -> i64; struct x {}",
        "mod math; use math::x; use math::x;",
    ] {
        let files = Files::new(main);
        files.write("x.lang", b"");
        files.write("math.lang", b"pub struct x {}");
        let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
        assert!(analysis.graph.is_none(), "{main}");
        let duplicate = analysis
            .diagnostics
            .iter()
            .find(|d| d.message.contains("duplicate module name"))
            .unwrap();
        assert_eq!(duplicate.secondary.len(), 1);
    }
}

#[test]
fn missing_child_is_a_source_diagnostic_and_missing_root_is_an_input_error() {
    let files = Files::new("mod absent;");
    let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
    assert!(analysis.graph.is_none());
    assert_eq!(analysis.diagnostics[0].code, codes::MODULE_ERROR);
    let mut sources = SourceMap::new();
    assert!(matches!(
        load_modules(
            &mut sources,
            &files.root.join("absent.lang"),
            &files.root,
            ModuleLimits::default()
        ),
        Err(ModuleLoadError::Io { .. })
    ));
}

#[test]
fn malformed_source_retains_parser_diagnostics_and_cannot_be_a_resolved_graph() {
    let files = Files::new("fn broken(");
    let (sources, analysis) = files.load(ModuleLimits::default()).unwrap();
    assert!(analysis.graph.is_none());
    assert!(
        analysis
            .diagnostics
            .iter()
            .all(|d| d.code == codes::EXPECTED_SYNTAX)
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .all(|d| sources.file(d.primary.span.source()).unwrap().text() == "fn broken(")
    );
}

#[test]
fn invalid_utf8_child_has_a_raw_input_range() {
    let files = Files::new("mod bad;");
    files.write("bad.lang", b"a\xffb");
    let error = files.load(ModuleLimits::default()).unwrap_err();
    let ModuleLoadError::Input { path, error } = error else {
        panic!("expected encoding error")
    };
    assert!(path.ends_with("bad.lang"));
    assert_eq!(error.kind, InputErrorKind::InvalidUtf8);
    assert_eq!(error.byte_range, 1..2);
}

#[test]
fn byte_token_and_node_budgets_are_cumulative_across_files() {
    let main = "mod child; fn f() -> i64 { return 1; }";
    let child = "pub fn g() -> i64 { return 2; }";
    let files = Files::new(main);
    files.write("child.lang", child.as_bytes());
    let (_, graph) = files.graph();
    let tokens = graph
        .modules()
        .iter()
        .map(|m| m.tree().tokens().len())
        .sum();
    let nodes = graph.modules().iter().map(|m| m.tree().nodes().len()).sum();
    let exact_bytes = main.len() + child.len();
    let mut limits = ModuleLimits {
        total_source_bytes: exact_bytes,
        ..ModuleLimits::default()
    };
    limits.parse.lex.tokens = tokens;
    limits.parse.nodes = nodes;
    assert!(files.load(limits).unwrap().1.graph.is_some());
    let error = files
        .load(ModuleLimits {
            total_source_bytes: exact_bytes - 1,
            ..limits
        })
        .unwrap_err();
    assert!(matches!(
        error,
        ModuleLoadError::Input {
            error: lang_syntax::InputError {
                kind: InputErrorKind::SourceBytes,
                ..
            },
            ..
        }
    ));
    let mut fewer_tokens = limits;
    fewer_tokens.parse.lex.tokens -= 1;
    assert!(matches!(
        files.load(fewer_tokens),
        Err(ModuleLoadError::Parse(ParseError::Lex(LexError::Limit(_))))
    ));
    let mut fewer_nodes = limits;
    fewer_nodes.parse.nodes -= 1;
    assert!(matches!(
        files.load(fewer_nodes),
        Err(ModuleLoadError::Parse(ParseError::Limit(_)))
    ));
}

fn expect_limit(files: &Files, limits: ModuleLimits) {
    let error = files.load(limits).unwrap_err();
    let ModuleLoadError::Limit(diagnostic) = error else {
        panic!("expected module resource limit: {error}")
    };
    assert_eq!(diagnostic.code, codes::RESOURCE_LIMIT);
}

#[test]
fn declaration_module_diagnostic_and_work_limits_are_enforced() {
    let files = Files::new("mod missing;");
    expect_limit(
        &files,
        ModuleLimits {
            modules: 1,
            ..ModuleLimits::default()
        },
    );
    assert!(matches!(
        files.load(ModuleLimits {
            modules: 0,
            ..ModuleLimits::default()
        }),
        Err(ModuleLoadError::InvalidConfiguration)
    ));
    let files = Files::new("fn a() -> i64 { return 1; } fn b() -> i64 { return 2; }");
    expect_limit(
        &files,
        ModuleLimits {
            functions: 1,
            ..ModuleLimits::default()
        },
    );
    expect_limit(
        &files,
        ModuleLimits {
            work: 0,
            ..ModuleLimits::default()
        },
    );
    let files = Files::new("struct A {} enum B { One }");
    expect_limit(
        &files,
        ModuleLimits {
            definitions: 1,
            ..ModuleLimits::default()
        },
    );
    let files = Files::new("extern fn a(x: i64) -> i64; extern fn b(x: i64) -> i64;");
    expect_limit(
        &files,
        ModuleLimits {
            imports: 1,
            ..ModuleLimits::default()
        },
    );
    let files = Files::new("use missing;");
    expect_limit(
        &files,
        ModuleLimits {
            diagnostics: 0,
            ..ModuleLimits::default()
        },
    );
}

#[test]
fn enum_case_names_resolve_and_duplicates_are_shape_errors() {
    let files = Files::new("enum E { None, Some(i64) } fn f() -> E { return E::None; }");
    let (sources, graph) = files.graph();
    assert!(matches!(
        resolve(&sources, &graph, graph.root(), "E::None").unwrap(),
        ResolvedName::Case { index: 0, .. }
    ));
    files.write("main.lang", b"enum E { Same, Same(i64) }");
    let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
    assert!(analysis.graph.is_none());
    assert_eq!(analysis.diagnostics[0].code, codes::INVALID_SHAPE);
    assert_eq!(analysis.diagnostics[0].secondary.len(), 1);
}

#[test]
fn path_resolution_rejects_non_namespace_prefixes_unknown_names_and_small_work() {
    let files = Files::new("fn f() -> i64 { return f::x() + unknown(); }");
    let (sources, graph) = files.graph();
    for path in ["f::x", "unknown"] {
        assert!(matches!(
            resolve(&sources, &graph, graph.root(), path),
            Err(ResolutionError::Diagnostic(_))
        ));
    }
    let root = graph.module(graph.root()).unwrap();
    assert_eq!(
        graph.resolve(graph.root(), root.tree().root(), 10),
        Err(ResolutionError::InvalidInput)
    );
    let path = root
        .tree()
        .nodes()
        .iter()
        .flat_map(|n| n.children())
        .find_map(|e| match e {
            lang_syntax::SyntaxElement::Node(id)
                if root.tree().node(*id).unwrap().kind() == SyntaxKind::Path =>
            {
                Some(*id)
            }
            _ => None,
        })
        .unwrap();
    let ResolutionError::Limit(d) = graph.resolve(graph.root(), path, 0).unwrap_err() else {
        panic!("expected work limit")
    };
    d.validate(&sources).unwrap();
}

#[cfg(unix)]
fn symlink(target: &std::path::Path, link: &std::path::Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(unix)]
#[test]
fn symlinks_cannot_escape_root_and_outside_root_files_are_never_loaded() {
    let outside = Files::new("fn secret() -> i64 { return 99; }");
    let files = Files::new("mod escape;");
    symlink(
        &outside.root.join("main.lang"),
        &files.root.join("escape.lang"),
    );
    let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
    assert!(analysis.graph.is_none());
    assert!(analysis.diagnostics[0].message.contains("escapes"));
    assert!(matches!(
        load_modules(
            &mut SourceMap::new(),
            &outside.root.join("main.lang"),
            &files.root,
            ModuleLimits::default()
        ),
        Err(ModuleLoadError::OutsideRoot { .. })
    ));
}

#[cfg(unix)]
#[test]
fn canonical_file_cycles_and_duplicates_are_rejected_without_recursion() {
    let files = Files::new("mod again;");
    symlink(
        &files.root.join("main.lang"),
        &files.root.join("again.lang"),
    );
    let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
    assert!(analysis.graph.is_none());
    assert!(analysis.diagnostics[0].message.contains("cycle"));
    let files = Files::new("mod a; mod b;");
    files.write("a.lang", b"pub struct A {}");
    symlink(&files.root.join("a.lang"), &files.root.join("b.lang"));
    let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
    assert!(analysis.graph.is_none());
    assert!(
        analysis.diagnostics[0]
            .message
            .contains("duplicate canonical")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn non_utf8_display_paths_are_rejected_without_lossy_aliases() {
    use std::os::unix::ffi::OsStringExt;
    let files = Files::new("");
    let path = files
        .root
        .join(std::ffi::OsString::from_vec(b"bad\xff.lang".to_vec()));
    fs::write(&path, b"").unwrap();
    assert!(matches!(
        load_modules(
            &mut SourceMap::new(),
            &path,
            &files.root,
            ModuleLimits::default()
        ),
        Err(ModuleLoadError::NonUtf8Path { .. })
    ));
}

#[test]
fn long_alias_cycles_use_an_explicit_stack_and_charge_repeated_visits() {
    let count = 512;
    let input = (0..count)
        .map(|i| format!("use a{}::a{i};", (i + 1) % count))
        .collect::<String>();
    let files = Files::new(&input);
    let (_, analysis) = files.load(ModuleLimits::default()).unwrap();
    assert!(analysis.graph.is_none());
    assert_eq!(analysis.diagnostics.len(), count);
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.message.contains("cycle"))
    );
    expect_limit(
        &files,
        ModuleLimits {
            work: count + 10,
            ..ModuleLimits::default()
        },
    );
}

#[test]
fn task_syntax_is_preserved_for_semantic_analysis() {
    for text in [
        "async fn unused()->i64{return 1;}fn main()->i64{return 42;}",
        "fn main()->i64{task_scope{}yield;cancel_check;cancel child;return await spawn child();}",
    ] {
        let files = Files::new(text);
        let (_, loaded) = files.load(ModuleLimits::default()).unwrap();
        assert!(loaded.diagnostics.is_empty());
        assert!(loaded.graph.is_some());
    }
    let files = Files::new("mod child;fn main()->i64{return 42;}");
    files.write("child.lang", b"pub async fn unused()->i64{return 1;}");
    let (sources, loaded) = files.load(ModuleLimits::default()).unwrap();
    let graph = loaded.graph.unwrap();
    let bound =
        lang_semantics::binding::bind_program(&graph, &sources, Default::default()).unwrap();
    let result =
        lang_hir::program::analyze_program(bound.program.unwrap(), &sources, Default::default())
            .unwrap();
    assert!(result.program.is_none());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == codes::ASYNC_UNAVAILABLE)
    );
}
