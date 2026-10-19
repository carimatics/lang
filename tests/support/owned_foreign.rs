use super::*;
use lang_diagnostics::{DiagnosticCode, codes};
use lang_hir::resolved as h;
use lang_mir::unit::ownership::declarations as d;

fn fails(source: &str, code: DiagnosticCode, written: &str) {
    fails_with_limits(source, code, written, Default::default());
}
fn fails_with_limits(
    source: &str,
    code: DiagnosticCode,
    written: &str,
    limits: frontend::CompileLimits,
) {
    let analysis = frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("foreign.lang"),
        source.as_bytes(),
        None,
        limits,
        None,
    );
    assert!(analysis.result.is_err(), "{source}");
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == code
                && analysis.sources.slice(diagnostic.primary.span).unwrap() == written),
        "{source}: {:?}",
        analysis.diagnostics
    );
    for diagnostic in &analysis.diagnostics {
        analysis.sources.slice(diagnostic.primary.span).unwrap();
        for related in &diagnostic.secondary {
            analysis.sources.slice(related.span).unwrap();
        }
    }
}

#[test]
fn scalar_c_declarations_force_ownership_mir_and_keep_both_abi_identities() {
    for source in [
        "extern \"C\" fn foreign(value: i64) -> i64; fn main() -> i64 { unsafe { return foreign(1); } }",
        "extern \"C\" fn unused(value: i64) -> i64; fn main() -> i64 { return 1; }",
        "pub extern \"C\" fn main() -> i64 { return 1; }",
    ] {
        let program = checked(source);
        let hir = program.hir().loaned().moved().bodies().source();
        let mir = program
            .mir()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations();
        assert!(mir.foreign_interface_span().is_some());
        if hir.imports.is_empty() {
            assert_eq!(hir.functions[0].abi, h::FunctionAbi::CExport);
            assert_eq!(mir.functions()[0].abi, d::FunctionAbi::CExport);
        } else {
            assert_eq!(hir.imports[0].abi, h::ImportAbi::C);
            assert_eq!(mir.imports()[0].abi, d::ImportAbi::C);
        }
        let (mut allocator, trace) = allocator();
        let mut instance = c::Instance::default();
        let error = vm::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            control(),
        )
        .err()
        .expect("C adapter is not implemented");
        assert!(matches!(error.reason, vm::Reason::UnsupportedCInterface));
        assert!(trace.borrow().requests.is_empty());
        assert!(!instance.is_poisoned());
    }
}

#[test]
fn c_source_supports_copy_aggregates_zero_and_multiple_parameters_and_legacy_host() {
    let program = checked(
        r#"
        pub struct Pair { number: i64, flag: bool }
        extern fn host(value: i64) -> i64;
        extern "C" fn zero() -> core::Unit;
        extern "C" fn many(value: Pair, flag: bool, optional: core::Option<i64>) -> core::Result<Pair, core::AllocError>;
        pub extern "C" fn exported(value: Pair) -> Pair { return value; }
        fn main() -> i64 {
            unsafe {
                let mut ignored: core::Unit = zero();
                let mut result: core::Result<Pair, core::AllocError> = many(Pair { number: host(1), flag: true }, false, core::Option::Some(2));
            }
            return 0;
        }
    "#,
    );
    let declarations = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    let imports = declarations.imports();
    assert_eq!(
        imports
            .iter()
            .map(|i| i.parameters.len())
            .collect::<Vec<_>>(),
        [1, 0, 3]
    );
    assert_eq!(
        imports.iter().map(|i| i.abi).collect::<Vec<_>>(),
        [d::ImportAbi::Host, d::ImportAbi::C, d::ImportAbi::C]
    );
    let raw = program
        .into_failure_flow()
        .into_source()
        .into_activity()
        .into_moved()
        .into_typed()
        .into_table();
    let error = lang_bytecode::ownership::encode(raw, None, Default::default()).unwrap_err();
    assert!(matches!(
        error.kind,
        lang_bytecode::ownership::CodecErrorKind::UnsupportedCInterface { .. }
    ));
}

#[test]
fn c_call_obligation_is_lexical_and_does_not_escape_a_nested_block_or_callee() {
    fails(
        include_str!("../fixtures/foreign/outside_unsafe.lang"),
        codes::FOREIGN_BOUNDARY,
        "foreign(1)",
    );
    fails(
        "extern \"C\" fn foreign(value: i64) -> i64; fn main() -> i64 { unsafe { let mut before: i64 = foreign(1); } return foreign(2); }",
        codes::FOREIGN_BOUNDARY,
        "foreign(2)",
    );
    fails(
        "extern \"C\" fn foreign(value: i64) -> i64; fn helper() -> i64 { return foreign(1); } fn main() -> i64 { unsafe { return helper(); } }",
        codes::FOREIGN_BOUNDARY,
        "foreign(1)",
    );
    fails(
        "extern \"C\" fn foreign(value: i64) -> i64; pub extern \"C\" fn main() -> i64 { return foreign(1); }",
        codes::FOREIGN_BOUNDARY,
        "foreign(1)",
    );
    checked(
        "extern \"C\" fn foreign(value: i64) -> i64; fn main() -> i64 { unsafe { if true { unsafe { return foreign(1); } } else { return foreign(2); } } }",
    );
}

#[test]
fn unsafe_preserves_name_type_move_and_loan_rules() {
    fails(
        include_str!("../fixtures/foreign/unsafe_scope.lang"),
        codes::UNKNOWN_NAME,
        "hidden",
    );
    fails(
        include_str!("../fixtures/foreign/unsafe_move.lang"),
        codes::INVALID_OWNER_USE,
        "value",
    );
    fails(
        "fn main() -> i64 { unsafe { let mut wrong: bool = 1; } return 0; }",
        codes::TYPE_MISMATCH,
        "1",
    );
    fails(
        "fn consume(value: core::Bytes) -> i64 { unsafe { let mut loan: &core::Bytes = &value; let mut moved: core::Bytes = value; } return 0; } fn main() -> i64 { return 0; }",
        codes::INVALID_LOAN,
        "value",
    );
}

#[test]
fn c_signature_rejects_owners_loans_and_transitive_non_copy_types() {
    fails(
        include_str!("../fixtures/foreign/owned_signature.lang"),
        codes::FOREIGN_BOUNDARY,
        "core::Bytes",
    );
    for ty in [
        "core::Text",
        "core::Vec<i64>",
        "core::Map<i64, i64>",
        "shared i64",
        "weak i64",
        "arena i64",
        "&i64",
        "core::Option<core::Bytes>",
    ] {
        fails(
            &format!(
                "extern \"C\" fn foreign(value: {ty}) -> i64; fn main() -> i64 {{ return 0; }}"
            ),
            codes::FOREIGN_BOUNDARY,
            ty,
        );
    }
    fails(
        "struct Owned { bytes: core::Bytes } extern \"C\" fn foreign(value: Owned) -> i64; fn main() -> i64 { return 0; }",
        codes::FOREIGN_BOUNDARY,
        "Owned",
    );
    fails(
        "pub extern \"C\" fn exported(value: core::Bytes) -> i64 { return 0; } fn main() -> i64 { return 0; }",
        codes::FOREIGN_BOUNDARY,
        "core::Bytes",
    );
}

#[test]
fn c_signature_parameter_bounds_are_fixed_and_duplicates_are_source_errors() {
    for count in [256, 257] {
        let parameters = (0..count)
            .map(|i| format!("p{i}: i64"))
            .collect::<Vec<_>>()
            .join(", ");
        let source = format!(
            "extern \"C\" fn foreign({parameters}) -> i64; fn main() -> i64 {{ return 0; }}"
        );
        if count == 256 {
            checked(&source);
        } else {
            fails_with_limits(
                &source,
                codes::FOREIGN_BOUNDARY,
                &format!("extern \"C\" fn foreign({parameters}) -> i64;"),
                {
                    let mut limits = frontend::CompileLimits::default();
                    limits.modules.parse.list_elements = 512;
                    limits.typing.layout.fields = 512;
                    limits
                },
            );
        }
    }
    fails(
        "extern \"C\" fn foreign(same: i64, same: bool) -> i64; fn main() -> i64 { return 0; }",
        codes::DUPLICATE_NAME,
        "same",
    );
    let name = "x".repeat(129);
    fails(
        &format!("pub extern \"C\" fn {name}() -> i64 {{ return 0; }}"),
        codes::FOREIGN_BOUNDARY,
        &name,
    );
}

#[test]
fn c_signature_depth_and_record_width_remain_fixed_when_general_limits_are_relaxed() {
    let mut definitions = String::new();
    for index in 0..64 {
        let child = if index == 0 {
            "i64".into()
        } else {
            format!("T{}", index - 1)
        };
        definitions.push_str(&format!("struct T{index} {{ value: {child} }} "));
    }
    let source = format!(
        "{definitions} extern \"C\" fn foreign(value: T63) -> i64; fn main() -> i64 {{ return 0; }}"
    );
    let mut limits = frontend::CompileLimits::default();
    limits.typing.types.depth = 128;
    limits.typing.layout.depth = 128;
    fails_with_limits(&source, codes::FOREIGN_BOUNDARY, "T63", limits);

    let fields = (0..257)
        .map(|i| format!("f{i}: i64"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        "struct Wide {{ {fields} }} extern \"C\" fn foreign(value: Wide) -> i64; fn main() -> i64 {{ return 0; }}"
    );
    limits.modules.parse.list_elements = 512;
    limits.typing.types.members = 512;
    limits.typing.layout.fields = 512;
    fails_with_limits(&source, codes::FOREIGN_BOUNDARY, "Wide", limits);
}

#[test]
fn unsafe_scope_cleanup_and_return_use_the_normal_vm_path() {
    let source = r#"
        fn main() -> i64 {
            let mut result: i64 = 0;
            unsafe {
                let mut made: core::Result<core::Bytes, core::AllocError> = core::bytes_new(core::allocator());
                match made { core::Result::Ok(_) => {}, core::Result::Err(_) => { return -1; }, }
                let mut bytes: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(made);
                result = core::bytes_len(bytes);
            }
            return result + 1;
        }
    "#;
    let program = checked(source);
    let (mut allocator, trace) = allocator();
    let mut instance = c::Instance::default();
    let outcome = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(outcome.value, 1);
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(trace.borrow().requests.len(), trace.borrow().released.len());
    let program = checked("fn main() -> i64 { unsafe { return 7; } }");
    let (mut allocator, _) = super::allocator();
    let mut instance = c::Instance::default();
    let result = vm::execute_entry(
        program.failure_flow(),
        &mut instance,
        Some(&mut allocator),
        control(),
    )
    .unwrap();
    assert_eq!(result.value, 7);
}
