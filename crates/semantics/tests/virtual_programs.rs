use lang_diagnostics::{SourceMap, codes};
use lang_hir::{
    program::{
        CheckError, CheckInputErrorKind, CheckLimits, check_expression_function, check_program,
    },
    resolved as r,
};
use lang_mir::unit as m;
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, ModuleLoadError, load_virtual},
    program_lowering::{self, LowerError},
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn resolved(text: &str) -> (SourceMap, r::Program) {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("session.lang"),
        text.as_bytes(),
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let bound = bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
    assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
    (sources, bound.program.unwrap())
}
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "lang-virtual-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn virtual_sources_require_no_disk_file_and_keep_original_utf8_bytes() {
    let text = "// 😀\nfn value() -> i64 { return 42; }\r\n";
    let (sources, program) = resolved(text);
    let file = sources.file(program.modules[0].span.source()).unwrap();
    assert_eq!(file.name(), "session.lang");
    assert_eq!(file.text(), text);
    assert!(
        check_program(program, &sources, CheckLimits::default())
            .unwrap()
            .program
            .is_some()
    );
}

#[test]
fn virtual_task_source_cannot_become_a_synchronous_semantic_program() {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("task-session.lang"),
        b"// snapshot\nasync fn main()->i64{yield;return 42;}",
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.graph.is_none());
    assert_eq!(loaded.diagnostics.len(), 1);
    assert_eq!(loaded.diagnostics[0].code, codes::ASYNC_UNAVAILABLE);
    loaded.diagnostics[0].validate(&sources).unwrap();
}

#[test]
fn file_modules_are_unavailable_without_explicit_host_authority() {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("session.lang"),
        b"mod sibling;",
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.graph.is_none());
    assert_eq!(loaded.diagnostics.len(), 1);
    assert_eq!(loaded.diagnostics[0].code, codes::MODULE_ERROR);
    assert_eq!(
        sources.slice(loaded.diagnostics[0].primary.span).unwrap(),
        "sibling"
    );
}

#[test]
fn explicit_source_roots_allow_hierarchical_children_of_a_virtual_root() {
    let directory = Directory::new();
    fs::write(
        directory.0.join("math.lang"),
        "pub mod inner; pub fn value() -> i64 { return inner::value(); }",
    )
    .unwrap();
    fs::create_dir(directory.0.join("math")).unwrap();
    fs::write(
        directory.0.join("math/inner.lang"),
        "pub fn value() -> i64 { return 42; }",
    )
    .unwrap();
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("session.lang"),
        b"mod math; fn f() -> i64 { return math::value(); }",
        Some(&directory.0),
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let graph = loaded.graph.unwrap();
    assert_eq!(graph.modules().len(), 3);
    let bound = bind_program(&graph, &sources, BindingLimits::default()).unwrap();
    assert!(
        check_program(bound.program.unwrap(), &sources, CheckLimits::default())
            .unwrap()
            .program
            .is_some()
    );
    assert!(!directory.0.join("session.lang").exists());
}

#[test]
fn virtual_input_paths_encoding_and_cumulative_bytes_are_bounded() {
    for display in ["", "../escape.lang", "/outside.lang"] {
        assert!(matches!(
            load_virtual(
                &mut SourceMap::new(),
                Path::new(display),
                b"",
                None,
                ModuleLimits::default()
            ),
            Err(ModuleLoadError::InvalidConfiguration)
        ));
    }
    assert!(matches!(
        load_virtual(
            &mut SourceMap::new(),
            Path::new("session.lang"),
            &[0xff],
            None,
            ModuleLimits::default()
        ),
        Err(ModuleLoadError::Input { .. })
    ));
    let directory = Directory::new();
    fs::write(
        directory.0.join("child.lang"),
        "fn value() -> i64 { return 42; }",
    )
    .unwrap();
    assert!(matches!(
        load_virtual(
            &mut SourceMap::new(),
            Path::new("session.lang"),
            b"mod child;",
            Some(&directory.0),
            ModuleLimits {
                total_source_bytes: 12,
                ..ModuleLimits::default()
            }
        ),
        Err(ModuleLoadError::Input { .. })
    ));
}

#[test]
fn only_the_expression_wrapper_result_is_inferred_from_checked_facts() {
    for (prefix, expression, expected) in [
        ("", "1 + 2", r::Type::I64),
        ("", "true", r::Type::Bool),
        (
            "struct R { value: bool }",
            "R { value: true }",
            r::Type::Nominal(r::DefinitionId(0)),
        ),
        ("enum E { A }", "E::A", r::Type::Nominal(r::DefinitionId(0))),
    ] {
        let text = format!("{prefix} fn __repl_eval_0() -> i64 {{ return {expression}; }}");
        let (sources, resolved) = resolved(&text);
        let ordinary = check_program(resolved.clone(), &sources, CheckLimits::default()).unwrap();
        assert_eq!(ordinary.program.is_some(), expected == r::Type::I64);
        let checked =
            check_expression_function(resolved, &sources, CheckLimits::default(), r::FunctionId(0))
                .unwrap();
        assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
        let typed = checked.program.unwrap();
        assert_eq!(typed.source().functions[0].result.value_type, expected);
        let unit = program_lowering::lower_library(&typed, &sources, m::Limits::default()).unwrap();
        assert!(unit.unit().entry.is_none());
        assert_eq!(
            unit.unit().functions[0].signature.result,
            match expected {
                r::Type::Constructed(_) => panic!("fixture only covers RFC 0006 types"),
                r::Type::I64 => m::Type::I64,
                r::Type::Bool => m::Type::Bool,
                r::Type::Nominal(_) =>
                    if prefix.starts_with("struct") {
                        m::Type::Record(m::DefinitionId(0))
                    } else {
                        m::Type::Enum(m::DefinitionId(0))
                    },
            }
        );
    }
}

#[test]
fn wrapper_cannot_be_called_recurse_or_contain_ordinary_statements() {
    for text in [
        "fn __repl_eval_0() -> i64 { return __repl_eval_0(); }",
        "fn __repl_eval_0() -> i64 { return 1; } fn other() -> i64 { return __repl_eval_0(); }",
        "pub fn __repl_eval_0() -> i64 { return 1; }",
        "fn __repl_eval_0(x: i64) -> i64 { return x; }",
        "fn __repl_eval_0() -> i64 { let mut x: i64 = 1; return x; }",
        "fn ordinary() -> i64 { return true; }",
    ] {
        let (sources, source) = resolved(text);
        assert!(matches!(
            check_expression_function(source, &sources, CheckLimits::default(), r::FunctionId(0)),
            Err(CheckError::Input {
                kind: CheckInputErrorKind::InvalidExpressionFunction,
                ..
            })
        ));
    }
}

#[test]
fn inference_does_not_relax_other_function_typing_or_resource_limits() {
    let (sources, source) =
        resolved("fn bad() -> i64 { return false; } fn __repl_eval_0() -> i64 { return true; }");
    let checked = check_expression_function(
        source.clone(),
        &sources,
        CheckLimits::default(),
        r::FunctionId(1),
    )
    .unwrap();
    assert!(checked.program.is_none());
    assert!(
        checked
            .diagnostics
            .iter()
            .any(|d| d.code == codes::EXPECTED_I64)
    );
    assert!(matches!(
        check_expression_function(
            source,
            &sources,
            CheckLimits {
                work: 0,
                ..CheckLimits::default()
            },
            r::FunctionId(1)
        ),
        Err(CheckError::Limit(_))
    ));
}

#[test]
fn library_lowering_does_not_treat_a_main_named_function_as_a_script_entry() {
    let (sources, source) = resolved("fn main(x: i64) -> bool { return x < 1; }");
    let typed = check_program(source, &sources, CheckLimits::default())
        .unwrap()
        .program
        .unwrap();
    assert!(matches!(
        program_lowering::lower(&typed, &sources, m::Limits::default()),
        Err(LowerError::Mir(m::Error {
            kind: m::ErrorKind::InvalidEntry,
            ..
        }))
    ));
    let unit = program_lowering::lower_library(&typed, &sources, m::Limits::default()).unwrap();
    assert!(unit.unit().entry.is_none());
    assert_eq!(unit.unit().functions[0].signature.result, m::Type::Bool);
}
