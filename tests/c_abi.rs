use lang::{frontend, frontend::CompileLimits};
use lang_diagnostics::SourceMap;
use lang_runtime::c_abi::{
    Artifact, CollisionKind, Direction, Error, HeaderOptions, Kind, LimitKind, Limits, Target,
};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::path::Path;

fn source(text: &str) -> Box<OwnershipProgram> {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("c_abi.lang"),
        text.as_bytes(),
        None,
        CompileLimits::default(),
        None,
    );
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let Program::Ownership(program) = analysis.result.unwrap() else {
        panic!("C-aware ownership MIR")
    };
    program
}
fn artifact(text: &str, target: Target, limits: Limits) -> Result<Artifact, Error> {
    let program = source(text);
    Artifact::compile(
        program
            .mir()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations(),
        target,
        limits,
    )
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn scalar_descriptors_match_all_frozen_rfc_targets_and_directions() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../benchmarks/fixtures/c-abi-profile-1-fingerprints.json"
    ))
    .unwrap();
    for vector in vectors["vectors"].as_array().unwrap() {
        let target = if vector["target"] == 1 {
            Target::MacosAarch64
        } else {
            Target::WindowsX86_64
        };
        let import = vector["direction"] == 0;
        let text = if import {
            "extern \"C\" fn increment(value: i64) -> i64; fn main() -> i64 { return 0; }"
        } else {
            "pub extern \"C\" fn increment(value: i64) -> i64 { return value; }"
        };
        let artifact = artifact(text, target, Limits::default()).unwrap();
        let function = &artifact.functions()[0];
        assert_eq!(
            function.direction,
            if import {
                Direction::Import
            } else {
                Direction::Export
            }
        );
        assert_eq!(
            hex(function.descriptor()),
            vector["descriptor_hex"].as_str().unwrap()
        );
        assert_eq!(
            function.digest.to_string(),
            vector["sha256"].as_str().unwrap()
        );
    }
}

const AGGREGATES: &str = r#"
pub struct Empty {}
pub struct Pair { flag: bool, value: i64 }
pub enum Choice { Empty, Value(Pair, bool) }
extern "C" fn inspect(value: Choice, unit: core::Unit, empty: Empty) -> core::Result<Pair, core::BufferError>;
extern "C" fn scalar(flag: bool, value: i64) -> bool;
extern "C" fn errors(value: core::Utf8Error, owner: core::OwnershipError, region: core::ArenaError) -> core::BoundsError;
pub extern "C" fn identity(value: Pair) -> Pair { return value; }
pub extern "C" fn unit() -> core::Unit { return core::Unit {}; }
fn main() -> i64 { return 0; }
"#;

#[test]
fn aggregate_type_and_artifact_digests_match_independent_reference_vectors() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../benchmarks/fixtures/c-abi-profile-1-aggregate-fingerprints.json"
    ))
    .unwrap();
    for vector in vectors["vectors"].as_array().unwrap() {
        let target = if vector["target"] == 1 {
            Target::MacosAarch64
        } else {
            Target::WindowsX86_64
        };
        let declaration = if vector["direction"] == 0 {
            "extern \"C\" fn aggregate(value: Choice) -> Pair; fn main() -> i64 { return 0; }"
        } else {
            "pub extern \"C\" fn aggregate(value: Choice) -> Pair { return Pair { flag: true, value: 7 }; }"
        };
        let text = format!(
            "pub struct Pair {{ flag: bool, value: i64 }} pub enum Choice {{ Empty, Value(Pair, bool) }} {declaration}"
        );
        let artifact = artifact(&text, target, Limits::default()).unwrap();
        let pair = artifact
            .types()
            .iter()
            .find(|t| matches!(&t.kind, Kind::Record { name, .. } if name == "Pair"))
            .unwrap();
        let choice = artifact
            .types()
            .iter()
            .find(|t| matches!(&t.kind, Kind::Enum { name, .. } if name == "Choice"))
            .unwrap();
        assert_eq!(
            hex(pair.descriptor()),
            vectors["pair_encoding"].as_str().unwrap()
        );
        assert_eq!(
            hex(choice.descriptor()),
            vectors["choice_encoding"].as_str().unwrap()
        );
        assert_eq!(
            pair.digest.to_string(),
            vector["type_pair_sha256"].as_str().unwrap()
        );
        assert_eq!(
            choice.digest.to_string(),
            vector["type_choice_sha256"].as_str().unwrap()
        );
        assert_eq!(
            hex(artifact.functions()[0].descriptor()),
            vector["descriptor_hex"].as_str().unwrap()
        );
        assert_eq!(
            artifact.functions()[0].digest.to_string(),
            vector["sha256"].as_str().unwrap()
        );
        assert_eq!(
            hex(artifact.descriptor()),
            vector["artifact_hex"].as_str().unwrap()
        );
        assert_eq!(
            artifact.digest().to_string(),
            vector["artifact_sha256"].as_str().unwrap()
        );
    }
}

#[test]
fn implicit_builtin_payload_depth_matches_source_mir_and_c_descriptor_limits() {
    for builtin in ["BufferError", "Utf8Error", "OwnershipError", "ArenaError"] {
        let ty = format!(
            "{}core::{builtin}{}",
            "core::Option<".repeat(62),
            ">".repeat(62)
        );
        let text =
            format!("extern \"C\" fn deep(value: {ty}) -> i64; fn main() -> i64 {{ return 0; }}");
        let program = source(&text);
        let c = Artifact::compile(
            program
                .mir()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations(),
            Target::MacosAarch64,
            Limits::default(),
        )
        .unwrap();
        assert_eq!(c.types()[c.functions()[0].parameters[0].0].depth, 64);
        let mut raw = program
            .into_failure_flow()
            .into_source()
            .into_activity()
            .into_moved()
            .into_typed()
            .into_table()
            .places
            .declarations;
        let previous = raw.imports[0].parameters[0];
        let id = lang_mir::unit::ownership::ConstructedId(raw.types.constructed.len() as u32);
        raw.types
            .constructed
            .push(lang_mir::unit::ownership::Constructed {
                constructor: lang_mir::unit::ownership::Constructor::Option(previous.ty),
                span: previous.span,
            });
        raw.imports[0].parameters[0].ty = lang_mir::unit::ownership::Type::Constructed(id);
        let error = raw.verify(Default::default()).unwrap_err();
        assert!(matches!(
            error.kind,
            lang_mir::unit::ownership::ErrorKind::CProfileLimit {
                kind: lang_mir::unit::ownership::CProfileLimit::Depth,
                requested: 65,
                limit: 64
            }
        ));
        assert_eq!(error.span, Some(previous.span));
        let ty = format!("core::Option<{ty}>");
        let text =
            format!("extern \"C\" fn deep(value: {ty}) -> i64; fn main() -> i64 {{ return 0; }}");
        let analysis = frontend::analyze_virtual(
            SourceMap::new(),
            Path::new("deep.lang"),
            text.as_bytes(),
            None,
            CompileLimits::default(),
            None,
        );
        assert!(analysis.result.is_err());
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|d| d.code == lang_diagnostics::codes::FOREIGN_BOUNDARY
                    && analysis.sources.slice(d.primary.span).unwrap() == ty)
        );
    }
}

#[test]
fn c_layouts_preserve_actual_record_enum_and_empty_object_alignment() {
    for target in [Target::MacosAarch64, Target::WindowsX86_64] {
        let artifact = artifact(AGGREGATES, target, Limits::default()).unwrap();
        let pair = artifact
            .types()
            .iter()
            .find(|t| matches!(&t.kind, Kind::Record { name, .. } if name == "Pair"))
            .unwrap();
        assert_eq!((pair.size, pair.align), (16, 8));
        let Kind::Record { fields, .. } = &pair.kind else {
            unreachable!()
        };
        assert_eq!(fields.iter().map(|f| f.offset).collect::<Vec<_>>(), [0, 8]);
        let choice = artifact
            .types()
            .iter()
            .find(|t| matches!(&t.kind, Kind::Enum { name, .. } if name == "Choice"))
            .unwrap();
        assert_eq!((choice.size, choice.align), (32, 8));
        let Kind::Enum {
            cases,
            payload_offset,
            ..
        } = &choice.kind
        else {
            unreachable!()
        };
        assert_eq!(*payload_offset, 8);
        assert_eq!((cases[0].size, cases[0].align), (1, 1));
        assert_eq!((cases[1].size, cases[1].align), (24, 8));
        assert_eq!(cases[1].offsets, [0, 16]);
        let empty = artifact
            .types()
            .iter()
            .find(|t| matches!(&t.kind, Kind::Record { name, .. } if name == "Empty"))
            .unwrap();
        assert_eq!((empty.size, empty.align), (1, 1));
        for ty in artifact.types() {
            let Kind::Enum { name, cases, .. } = &ty.kind else {
                continue;
            };
            let Some(family) = name.strip_prefix("core::") else {
                continue;
            };
            let expected: Vec<_> = lang_hir::core::Case::ALL
                .into_iter()
                .filter(|case| case.family().name() == family)
                .map(|case| (case.name(), case.payload().len()))
                .collect();
            assert_eq!(
                cases
                    .iter()
                    .map(|case| (case.name.as_str(), case.payload.len()))
                    .collect::<Vec<_>>(),
                expected
            );
        }
        artifact
            .header(HeaderOptions::default(), Limits::default())
            .unwrap();
    }
}

#[test]
fn artifact_identity_is_sorted_and_covers_target_direction_names_and_signatures() {
    let first = "extern \"C\" fn a(value: bool) -> i64; extern \"C\" fn b() -> bool; fn main() -> i64 { return 0; }";
    let second = "extern \"C\" fn b() -> bool; extern \"C\" fn a(value: bool) -> i64; fn main() -> i64 { return 0; }";
    let a = artifact(first, Target::MacosAarch64, Limits::default()).unwrap();
    let b = artifact(second, Target::MacosAarch64, Limits::default()).unwrap();
    assert_eq!(a.descriptor(), b.descriptor());
    assert_eq!(a.digest(), b.digest());
    let other = artifact(first, Target::WindowsX86_64, Limits::default()).unwrap();
    assert_ne!(a.digest(), other.digest());
    for text in [
        first.replace("a(value", "renamed(value"),
        first.replace("value: bool", "value: i64"),
    ] {
        assert_ne!(
            a.digest(),
            artifact(&text, Target::MacosAarch64, Limits::default())
                .unwrap()
                .digest()
        );
    }
}

#[test]
fn descriptor_and_header_limits_are_fallible_and_apply_before_publication() {
    let text = "extern \"C\" fn increment(value: i64) -> i64; fn main() -> i64 { return 0; }";
    let complete = artifact(text, Target::MacosAarch64, Limits::default()).unwrap();
    let mut limits = Limits {
        descriptor_bytes: complete.descriptor().len(),
        ..Default::default()
    };
    artifact(text, Target::MacosAarch64, limits).unwrap();
    limits.descriptor_bytes -= 1;
    assert!(matches!(
        artifact(text, Target::MacosAarch64, limits),
        Err(Error::Limit {
            kind: LimitKind::DescriptorBytes,
            ..
        })
    ));
    limits = Limits::default();
    limits.budget.work = 0;
    assert!(matches!(
        artifact(text, Target::MacosAarch64, limits),
        Err(Error::Resource(_))
    ));
    limits = Limits::default();
    limits.budget.allocations = 0;
    assert!(matches!(
        artifact(text, Target::MacosAarch64, limits),
        Err(Error::Resource(_))
    ));
    limits = Limits::default();
    limits.functions = 0;
    assert!(matches!(
        artifact(text, Target::MacosAarch64, limits),
        Err(Error::Limit {
            kind: LimitKind::Functions,
            ..
        })
    ));
    let two = "extern \"C\" fn first(value: i64) -> i64; extern \"C\" fn second(value: i64) -> i64; fn main() -> i64 { return 0; }";
    let two_complete = artifact(two, Target::MacosAarch64, Limits::default()).unwrap();
    let mut combined = Limits {
        descriptor_bytes: two_complete.descriptor().len(),
        ..Default::default()
    };
    artifact(two, Target::MacosAarch64, combined).unwrap();
    combined.descriptor_bytes -= 1;
    assert!(matches!(
        artifact(two, Target::MacosAarch64, combined),
        Err(Error::Limit {
            kind: LimitKind::DescriptorBytes,
            ..
        })
    ));
    let header = complete
        .header(HeaderOptions::default(), Limits::default())
        .unwrap();
    limits = Limits {
        header_bytes: header.len(),
        ..Default::default()
    };
    assert_eq!(
        complete.header(HeaderOptions::default(), limits).unwrap(),
        header
    );
    limits.header_bytes -= 1;
    assert!(matches!(
        complete.header(HeaderOptions::default(), limits),
        Err(Error::Limit {
            kind: LimitKind::HeaderBytes,
            ..
        })
    ));
}

#[test]
fn optional_readable_aliases_reject_ambiguous_qualified_imports() {
    let program = source(
        "extern \"C\" fn a() -> i64; extern \"C\" fn b() -> i64; fn main() -> i64 { return 0; }",
    );
    let mut raw = program
        .into_failure_flow()
        .into_source()
        .into_activity()
        .into_moved()
        .into_typed()
        .into_table()
        .places
        .declarations;
    raw.imports[0].name = "a::tick".into();
    raw.imports[1].name = "b::tick".into();
    let checked = raw.verify(Default::default()).unwrap();
    let artifact = Artifact::compile(&checked, Target::MacosAarch64, Limits::default()).unwrap();
    artifact
        .header(HeaderOptions::default(), Limits::default())
        .unwrap();
    assert!(matches!(
        artifact.header(HeaderOptions { aliases: true }, Limits::default()),
        Err(Error::Collision(CollisionKind::Alias))
    ));
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64", target_env = "msvc")
))]
#[test]
fn generated_headers_compile_as_actual_c11_objects() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "lang-c-abi-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    let target = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Target::WindowsX86_64
    } else {
        Target::MacosAarch64
    };
    let profile = artifact(AGGREGATES, target, Limits::default()).unwrap();
    let header = profile
        .header(HeaderOptions { aliases: true }, Limits::default())
        .unwrap();
    let scalar = profile
        .functions()
        .iter()
        .find(|f| f.name == "scalar")
        .unwrap();
    let unit_function = profile
        .functions()
        .iter()
        .find(|f| f.name == "unit")
        .unwrap();
    let inspect = profile
        .functions()
        .iter()
        .find(|f| f.name == "inspect")
        .unwrap();
    let arg_names: Vec<_> = inspect
        .parameters
        .iter()
        .map(|id| format!("lang_type_{}", profile.types()[id.0].digest))
        .collect();
    let result_name = format!("lang_type_{}", profile.types()[inspect.result.0].digest);
    let checks = format!(
        "
typedef uint32_t (*expected_scalar)(lang_context *, void *, uint8_t, int64_t, uint8_t *);
typedef uint32_t (*expected_unit)(lang_context *);
typedef uint32_t (*expected_inspect)(lang_context *, void *, const {arg0} *, const {arg1} *, const {arg2} *, {result} *);
void check_types(lang_callback_{scalar} callback, lang_callback_{inspect} aggregate) {{
  expected_scalar a = callback;
  expected_unit b = &{unit};
  expected_inspect c = aggregate;
  (void)a; (void)b; (void)c;
}}
",
        scalar = scalar.digest,
        unit = unit_function.symbol,
        inspect = inspect.digest,
        arg0 = arg_names[0], arg1 = arg_names[1], arg2 = arg_names[2],
        result = result_name
    );
    let second = artifact("pub struct Pair { flag: bool, value: i64 } pub extern \"C\" fn another(value: Pair) -> Pair { return value; }", target, Limits::default()).unwrap();
    let second_header = second
        .header(HeaderOptions::default(), Limits::default())
        .unwrap();
    let unit = path.join("probe.c");
    std::fs::write(
        &unit,
        format!("{header}\n{header}\n{second_header}\n{checks}\nint main(void) {{ return 0; }}\n"),
    )
    .unwrap();
    let output = if cfg!(all(target_os = "windows", target_env = "msvc")) {
        std::process::Command::new("cl")
            .args(["/nologo", "/std:c11", "/W4", "/WX", "/c"])
            .arg(&unit)
            .arg(format!("/Fo{}", path.join("probe.obj").display()))
            .output()
            .unwrap()
    } else {
        std::process::Command::new("cc")
            .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-fsyntax-only"])
            .arg(&unit)
            .output()
            .unwrap()
    };
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    #[cfg(target_os = "macos")]
    for (target, triple) in [
        (Target::MacosAarch64, "arm64-apple-macos"),
        (Target::WindowsX86_64, "x86_64-pc-windows-msvc"),
    ] {
        let header = artifact(AGGREGATES, target, Limits::default())
            .unwrap()
            .header(HeaderOptions::default(), Limits::default())
            .unwrap();
        std::fs::write(&unit, header).unwrap();
        let output = std::process::Command::new("clang")
            .arg(format!("--target={triple}"))
            .args([
                "-ffreestanding",
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsyntax-only",
            ])
            .arg(&unit)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{triple}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_dir_all(path).unwrap();
}
