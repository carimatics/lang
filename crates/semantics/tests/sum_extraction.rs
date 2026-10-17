use lang_diagnostics::{DiagnosticCode, Severity, SourceMap, codes};
use lang_mir::unit::ownership::{bodies::Op, core::Operation};
use lang_semantics::{
    analysis::{self, Analysis, Program},
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_virtual},
};
use std::path::Path;

fn check(text: &str) -> (SourceMap, Analysis) {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("extraction.lang"),
        text.as_bytes(),
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let bound = bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
    if bound.program.is_none() {
        for diagnostic in &bound.diagnostics {
            diagnostic.validate(&sources).unwrap();
        }
        return (
            sources,
            Analysis {
                program: None,
                diagnostics: bound.diagnostics,
            },
        );
    }
    let result = analysis::analyze(
        bound.program.unwrap(),
        &sources,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    for diagnostic in &result.diagnostics {
        diagnostic.validate(&sources).unwrap();
    }
    (sources, result)
}

fn pass(text: &str) -> Program {
    let (_, result) = check(text);
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| d.severity == Severity::Warning),
        "{text}: {:?}",
        result.diagnostics
    );
    result.program.expect(text)
}

fn reject(text: &str, code: DiagnosticCode) {
    let (_, result) = check(text);
    assert!(result.program.is_none(), "accepted {text}");
    assert!(
        result.diagnostics.iter().any(|d| d.code == code),
        "{text}: {:?}",
        result.diagnostics
    );
}

#[test]
fn all_three_owned_payloads_lower_through_independently_checked_mir_with_call_spans() {
    let program = pass(
        "fn some(value: core::Option<core::Bytes>) -> core::Bytes { return core::option_unwrap::<core::Bytes>(value); }
         fn ok(value: core::Result<core::Bytes, core::Text>) -> core::Bytes { return core::result_unwrap::<core::Bytes, core::Text>(value); }
         fn err(value: core::Result<core::Bytes, core::Text>) -> core::Text { return core::result_unwrap_err::<core::Bytes, core::Text>(value); }",
    );
    let Program::Ownership(program) = program else {
        panic!()
    };
    let typed = program.mir().activity().moved().typed();
    let table = typed.clone().into_table();
    for (body, expected) in table.functions.iter().zip([
        Operation::OptionUnwrap,
        Operation::ResultUnwrap,
        Operation::ResultUnwrapErr,
    ]) {
        let calls: Vec<_> = body
            .blocks
            .iter()
            .flat_map(|b| &b.instructions)
            .filter_map(|i| {
                if let Op::CoreCall(call) = &i.op {
                    Some((call, i.span))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(calls.len(), 1);
        let (call, span) = calls[0];
        assert_eq!(call.operation, expected);
        assert!(call.result_loan.is_none());
        let text = &table.places.declarations.types.sources[span.source.0 as usize].text;
        assert!(
            text[span.start as usize..span.end as usize]
                .starts_with(&format!("core::{}::<", expected.name()))
        );
    }
}

#[test]
fn guarded_constructor_extraction_reaches_mutation_without_changing_borrowed_match() {
    pass(include_str!("../../../examples/analysis/extraction.lang"));
    reject(
        "fn f(value: core::Option<core::Bytes>) -> core::Bytes { match value { core::Option::Some(bytes) => { return bytes; }, core::Option::None => { return core::option_unwrap::<core::Bytes>(value); }, } }",
        codes::TYPE_MISMATCH,
    );
}

#[test]
fn copy_sums_can_be_reused_but_a_noncopy_sum_is_consumed_even_for_a_copy_payload() {
    pass(
        "fn f(value: core::Result<i64, bool>, option: core::Option<i64>) -> i64 {
            let mut first: i64 = core::result_unwrap::<i64, bool>(value);
            let mut second: i64 = core::result_unwrap::<i64, bool>(value);
            let mut third: i64 = core::option_unwrap::<i64>(option);
            return core::option_unwrap::<i64>(option) + first + second + third;
        }",
    );
    reject(
        "fn f(value: core::Result<i64, core::Bytes>) -> i64 { let mut first: i64 = core::result_unwrap::<i64, core::Bytes>(value); return core::result_unwrap::<i64, core::Bytes>(value); }",
        codes::INVALID_OWNER_USE,
    );
    for (ty, call) in [
        (
            "core::Option<core::Bytes>",
            "core::option_unwrap::<core::Bytes>",
        ),
        (
            "core::Result<core::Bytes, i64>",
            "core::result_unwrap::<core::Bytes, i64>",
        ),
        (
            "core::Result<i64, core::Bytes>",
            "core::result_unwrap_err::<i64, core::Bytes>",
        ),
    ] {
        reject(
            &format!(
                "fn f(value: {ty}) -> core::Bytes {{ let mut first: core::Bytes = {call}(value); return {call}(value); }}"
            ),
            codes::INVALID_OWNER_USE,
        );
    }
}

#[test]
fn named_loans_partial_moves_and_branch_or_loop_consumption_remain_rejected() {
    reject(
        "fn f(value: core::Option<core::Bytes>) -> core::Bytes { let mut read: &core::Option<core::Bytes> = &value; return core::option_unwrap::<core::Bytes>(value); }",
        codes::INVALID_LOAN,
    );
    reject(
        "struct Holder { value: core::Option<core::Bytes> } fn f(holder: Holder) -> core::Bytes { return core::option_unwrap::<core::Bytes>(holder.value); }",
        codes::INVALID_OWNERSHIP_TYPE,
    );
    reject(
        "fn f(value: core::Option<core::Bytes>, flag: bool) -> core::Bytes { if flag { let mut first: core::Bytes = core::option_unwrap::<core::Bytes>(value); } return core::option_unwrap::<core::Bytes>(value); }",
        codes::INVALID_OWNER_USE,
    );
    reject(
        "fn f(value: core::Option<core::Bytes>, flag: bool) -> i64 { let mut again: bool = flag; while again { let mut bytes: core::Bytes = core::option_unwrap::<core::Bytes>(value); again = false; } return 0; }",
        codes::INVALID_OWNER_USE,
    );
}

#[test]
fn signatures_require_exact_explicit_storable_arguments_and_cannot_unwrap_provisional_loans() {
    for (text, code) in [
        (
            "fn f(value: core::Option<i64>) -> i64 { return core::option_unwrap(value); }",
            codes::UNSUPPORTED_SPECIALIZATION,
        ),
        (
            "fn f(value: core::Option<i64>) -> i64 { return core::option_unwrap::<i64, bool>(value); }",
            codes::UNSUPPORTED_SPECIALIZATION,
        ),
        (
            "fn f(value: core::Option<bool>) -> i64 { return core::option_unwrap::<i64>(value); }",
            codes::TYPE_MISMATCH,
        ),
        (
            "fn f(value: core::Bytes) -> i64 { return core::option_unwrap::<i64>(value); }",
            codes::TYPE_MISMATCH,
        ),
        (
            "fn f(value: &core::Option<core::Bytes>) -> core::Bytes { return core::option_unwrap::<core::Bytes>(value); }",
            codes::TYPE_MISMATCH,
        ),
        (
            "fn f() -> i64 { let mut x: &i64 = core::option_unwrap::<&i64>(core::Option::None); return 0; }",
            codes::INVALID_OWNERSHIP_TYPE,
        ),
        (
            "fn f() -> i64 { let mut x: core::Allocator = core::option_unwrap::<core::Allocator>(core::Option::None); return 0; }",
            codes::INVALID_OWNERSHIP_TYPE,
        ),
        (
            "fn f(value: &core::Vec<i64>) -> i64 { return core::result_unwrap::<i64, core::BoundsError>(core::vec_get::<i64>(value, 0)); }",
            codes::TYPE_MISMATCH,
        ),
    ] {
        reject(text, code);
    }
}

#[test]
fn each_selected_payload_preserves_arena_brands_including_nested_sums() {
    for (ty, wrap, call) in [
        (
            "core::Option<arena i64>",
            "core::Option::Some(node)",
            "core::option_unwrap::<arena i64>(wrapped)",
        ),
        (
            "core::Result<arena i64, i64>",
            "core::Result::Ok(node)",
            "core::result_unwrap::<arena i64, i64>(wrapped)",
        ),
        (
            "core::Result<i64, arena i64>",
            "core::Result::Err(node)",
            "core::result_unwrap_err::<i64, arena i64>(wrapped)",
        ),
    ] {
        let prefix = format!(
            "let mut made: core::Result<arena i64, core::ArenaError> = core::arena_new::<i64>(graph, 7); let mut node: arena i64 = core::result_unwrap::<arena i64, core::ArenaError>(made); let mut wrapped: {ty} = {wrap}; let mut selected: arena i64 = {call};"
        );
        pass(&format!(
            "fn f(a: &core::Allocator) -> i64 {{ arena graph(a) {{ {prefix} match core::arena_read::<i64>(graph, selected) {{ core::Result::Ok(read) => {{ return (*read); }}, core::Result::Err(_) => {{ return 0; }}, }} }} }}"
        ));
        reject(
            &format!(
                "fn f(a: &core::Allocator) -> i64 {{ let mut outside: core::Option<arena i64> = core::Option::None; arena graph(a) {{ {prefix} let mut nested: core::Option<core::Option<arena i64>> = core::Option::Some(core::Option::Some(selected)); outside = core::option_unwrap::<core::Option<arena i64>>(nested); }} return 0; }}"
            ),
            codes::REGION_ESCAPE,
        );
    }
}
