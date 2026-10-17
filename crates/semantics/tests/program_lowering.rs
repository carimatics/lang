use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use lang_diagnostics::{Severity, SourceMap};
use lang_hir::program::{CheckLimits, TypedProgram, check_program};
use lang_mir::unit as m;
use lang_semantics::{
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_modules},
    program_lowering::{LowerError, lower},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Files(PathBuf);
impl Files {
    fn new(text: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "lang-lowering-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let files = Self(root);
        files.write("main.lang", text);
        files
    }
    fn write(&self, path: &str, text: &str) {
        fs::write(self.0.join(path), text).unwrap();
    }
    fn typed(&self) -> (SourceMap, TypedProgram) {
        let mut sources = SourceMap::new();
        let loaded = load_modules(
            &mut sources,
            &self.0.join("main.lang"),
            &self.0,
            ModuleLimits::default(),
        )
        .unwrap();
        assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
        let bound =
            bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
        assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
        let checked =
            check_program(bound.program.unwrap(), &sources, CheckLimits::default()).unwrap();
        assert!(
            checked
                .diagnostics
                .iter()
                .all(|d| d.severity == Severity::Warning),
            "{:?}",
            checked.diagnostics
        );
        (sources, checked.program.unwrap())
    }
    fn unit(&self) -> m::VerifiedUnit {
        let (sources, program) = self.typed();
        lower(&program, &sources, m::Limits::default()).unwrap()
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn scalar_cfg_loops_joins_early_returns_and_live_scope_are_verified() {
    let files = Files::new(
        "fn main() -> i64 { let mut x: i64 = 0; let mut flag: bool = true; while flag { if x < 3 { x = x + 1; } else { flag = false; } } if x < 4 { return x; } else { return 9; } return 42; }",
    );
    let verified = files.unit();
    let u = verified.unit();
    assert_eq!(u.entry, Some(m::FunctionId(0)));
    assert!(u.functions[0].blocks.iter().any(|b| {
        b.parameters
            .iter()
            .any(|r| u.functions[0].registers[r.0 as usize] == m::Type::Bool)
    }));
    assert!(
        !u.functions[0]
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .any(|i| matches!(i.op, m::Op::I64Constant { value: 42, .. }))
    );
}

#[test]
fn all_source_operations_and_multi_file_metadata_are_preserved() {
    let files = Files::new(
        "pub mod data; use data::Pair; use data::Maybe; extern fn record(x: i64) -> i64; fn main() -> i64 { let mut p: Pair = Pair { right: record(2) < 3, left: record(1) }; let mut m: Maybe = data::wrap(p); match m { Maybe::None => { return 0; }, Maybe::Some(Pair { right: false, left: 7 }) => { return 1; }, Maybe::Some(Pair { right: flag, left: value }) => { if flag { return value; } else { return p.left; } }, } }",
    );
    files.write("data.lang","pub struct Pair { left: i64, right: bool } pub enum Maybe { None, Some(Pair) } pub fn wrap(p: Pair) -> Maybe { return Maybe::Some(p); }");
    let verified = files.unit();
    let u = verified.unit();
    assert_eq!(u.sources.len(), 2);
    assert!(u.sources.iter().any(|s| s.path == "data.lang"));
    let mut tags = std::collections::BTreeSet::new();
    for f in &u.functions {
        assert!(f.span.is_some());
        for b in &f.blocks {
            assert!(b.terminator.span.is_some());
            for i in &b.instructions {
                let s = i.span.unwrap();
                assert!(
                    !u.sources[s.source.0 as usize].text[s.start as usize..s.end as usize]
                        .is_empty()
                );
                tags.insert(match i.op {
                    m::Op::I64Constant { .. } => 0,
                    m::Op::Add { .. } => 1,
                    m::Op::LessThan { .. } => 2,
                    m::Op::HostCall { .. } => 3,
                    m::Op::BoolConstant { .. } => 4,
                    m::Op::Call { .. } => 5,
                    m::Op::Record { .. } => 6,
                    m::Op::Field { .. } => 7,
                    m::Op::Enum { .. } => 8,
                    m::Op::IsCase { .. } => 9,
                    m::Op::EnumField { .. } => 10,
                    m::Op::EqualI64 { .. } => 11,
                });
            }
        }
    }
    assert!(
        [0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
            .into_iter()
            .all(|tag| tags.contains(&tag))
    );
}

#[test]
fn record_fields_evaluate_in_written_order_and_store_in_declaration_order() {
    let files = Files::new(
        "struct P { a: i64, b: i64 } extern fn visit(x: i64) -> i64; fn main() -> i64 { return P { b: visit(2), a: visit(1) }.a; }",
    );
    let verified = files.unit();
    let instructions = &verified.unit().functions[0].blocks[0].instructions;
    let constants: Vec<_> = instructions
        .iter()
        .filter_map(|i| match i.op {
            m::Op::I64Constant { value, .. } => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(constants, vec![2, 1]);
    let calls: Vec<_> = instructions
        .iter()
        .filter_map(|i| match i.op {
            m::Op::HostCall { destination, .. } => Some(destination),
            _ => None,
        })
        .collect();
    let fields = instructions
        .iter()
        .find_map(|i| match &i.op {
            m::Op::Record { fields, .. } => Some(fields),
            _ => None,
        })
        .unwrap();
    assert_eq!(fields, &vec![calls[1], calls[0]]);
}

#[test]
fn pattern_products_boolean_integer_and_warning_only_arms_have_valid_cfg() {
    for text in [
        "fn f(x: i64) -> i64 { match x { -9223372036854775808 => { return 1; }, 9223372036854775807 => { return 2; }, n => { return n; }, } }",
        "struct P { a: bool, b: bool } fn f(p: P) -> i64 { let mut result: i64 = 0; match p { P { a: false, b: _ } => { result = 1; }, P { b: false, a: true } => { result = 2; }, P { a: true, b: true } => { result = 3; }, } return result; }",
        "enum E { A, B(bool) } fn f(e: E) -> i64 { match e { _ => { return 1; }, E::B(x) => { if x { return 2; } else { return 3; } }, } }",
        "struct Empty {} fn f(e: Empty) -> i64 { match e { Empty {} => { return 1; }, _ => { return 2; }, } }",
        "fn f(flag: bool) -> i64 { match flag { false => { return 1; }, true => {}, } return 2; }",
    ] {
        Files::new(text).unit();
    }
}

#[test]
fn forward_mutual_and_aggregate_calls_keep_compilation_unit_signatures() {
    let files = Files::new(
        "struct R { value: i64 } fn first(r: R, flag: bool) -> R { if flag { return later(r); } else { return first(r, true); } } fn later(r: R) -> R { return first(r, false); }",
    );
    let verified = files.unit();
    assert!(verified.unit().entry.is_none());
    assert_eq!(
        verified.unit().functions[0].signature.parameters,
        vec![m::Type::Record(m::DefinitionId(0)), m::Type::Bool]
    );
    assert_eq!(
        verified.unit().functions[1].signature.result,
        m::Type::Record(m::DefinitionId(0))
    );
}

#[test]
fn semantic_byte_spans_exclude_leading_comments_and_preserve_unicode_text() {
    let files = Files::new("// 😀\n  fn main() -> i64 {\n    /* α */ return /* β */ 40 + 2;\n}\n");
    let (sources, program) = files.typed();
    let f = &program.source().functions[0];
    assert!(sources.slice(f.span).unwrap().starts_with("fn main"));
    assert!(
        sources
            .slice(f.blocks[f.body.0].statements[0].span)
            .unwrap()
            .starts_with("return")
    );
    let verified = lower(&program, &sources, m::Limits::default()).unwrap();
    let u = verified.unit();
    let f = &u.functions[0];
    let map = |s: m::Span| &u.sources[s.source.0 as usize].text[s.start as usize..s.end as usize];
    assert_eq!(map(f.blocks[0].instructions[0].span.unwrap()), "40");
    assert_eq!(map(f.blocks[0].instructions[2].span.unwrap()), "40 + 2");
    assert_eq!(
        map(f.blocks[0].terminator.span.unwrap()),
        "return /* β */ 40 + 2;"
    );
}

#[test]
fn construction_and_verification_limits_fail_without_a_partial_unit() {
    let files =
        Files::new("fn main() -> i64 { let mut x: i64 = 0; while x < 3 { x = x + 1; } return x; }");
    let (sources, program) = files.typed();
    for limits in [
        m::Limits {
            work: 0,
            ..m::Limits::default()
        },
        m::Limits {
            allocations: 0,
            ..m::Limits::default()
        },
        m::Limits {
            elements: 0,
            ..m::Limits::default()
        },
        m::Limits {
            registers: 1,
            ..m::Limits::default()
        },
        m::Limits {
            blocks: 1,
            ..m::Limits::default()
        },
        m::Limits {
            instructions: 1,
            ..m::Limits::default()
        },
        m::Limits {
            sources: 0,
            ..m::Limits::default()
        },
        m::Limits {
            total_source_bytes: 1,
            ..m::Limits::default()
        },
    ] {
        assert!(matches!(
            lower(&program, &sources, limits),
            Err(LowerError::Mir(m::Error {
                kind: m::ErrorKind::Limit { .. },
                ..
            }))
        ));
    }
    assert!(matches!(
        lower(&program, &SourceMap::new(), m::Limits::default()),
        Err(LowerError::Source(_))
    ));
}

#[test]
fn invalid_script_main_is_rejected_before_executable_publication() {
    for text in [
        "fn main(x: i64) -> i64 { return x; }",
        "fn main() -> bool { return true; }",
    ] {
        let files = Files::new(text);
        let (sources, program) = files.typed();
        assert!(matches!(
            lower(&program, &sources, m::Limits::default()),
            Err(LowerError::Mir(m::Error {
                kind: m::ErrorKind::InvalidEntry,
                ..
            }))
        ));
    }
}
