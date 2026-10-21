use lang_mir::unit::ownership::{self as o, bodies as b, tasks as t};
use std::path::Path;

const OWNED: &str = "async fn worker(value:core::Bytes)->core::Bytes{yield;return value;}async fn main()->i64{let mut input:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));let mut output:core::Bytes=core::result_unwrap::<core::Bytes,core::TaskError>(await worker(input));return core::bytes_len(output);}";

fn prepare(text: &str) -> Box<lang_semantics::analysis::TaskProgram> {
    let result = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        Path::new("task-send.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    let lang_semantics::analysis::Program::Tasks(program) = result.result.unwrap() else {
        panic!("task proof")
    };
    program
}

#[test]
fn owned_async_signatures_retain_independent_mir_proof_at_runtime_admission() {
    let program = prepare(OWNED);
    let mut allocator = lang_runtime::allocation::Allocator::new(
        lang_runtime::allocation::System,
        Default::default(),
    )
    .unwrap();
    let output = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        lang_runtime::execution::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(output.value, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    let output = native
        .execute(
            &mut allocator,
            lang_runtime::execution::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
    assert_eq!(output.value, 0);
    assert_eq!(allocator.stats().live_allocations, 0);
}

#[test]
fn recursive_owned_and_shared_signatures_have_matching_source_and_mir_capabilities() {
    for (definitions, ty) in [
        ("", "i64"),
        ("", "core::Bytes"),
        ("", "core::Text"),
        ("", "core::Vec<core::Bytes>"),
        ("", "core::Map<core::Text,core::Vec<core::Bytes>>"),
        (
            "",
            "core::Result<core::Option<core::Bytes>,core::BufferError>",
        ),
        ("", "shared core::Vec<core::Bytes>"),
        ("", "weak core::Vec<core::Bytes>"),
        (
            "struct Node{children:core::Vec<Node>,text:core::Text}",
            "Node",
        ),
        (
            "enum Message{None,Packet(core::Bytes,core::Vec<Message>)}",
            "Message",
        ),
        (
            "struct Left{right:core::Vec<Right>}struct Right{left:core::Vec<Left>,bytes:core::Bytes}",
            "Left",
        ),
        (
            "struct Right{left:core::Vec<Left>,bytes:core::Bytes}struct Left{right:core::Vec<Right>}",
            "Left",
        ),
    ] {
        // Unused async signatures are checked too, including forward recursion.
        let program = prepare(&format!(
            "{definitions}async fn carry(value:{ty})->{ty}{{yield;return value;}}async fn main()->i64{{return 0;}}"
        ));
        let source = program.hir().loaned().moved().bodies();
        let function = source
            .source()
            .functions
            .iter()
            .find(|f| f.name.text == "carry")
            .unwrap();
        let source_fact = source.properties(function.result.value_type).unwrap();
        let declarations = program.verified().typed().places().declarations();
        let function = declarations
            .functions()
            .iter()
            .find(|f| f.name == "carry")
            .unwrap();
        let mir_fact = declarations.types().properties(function.result.ty).unwrap();
        assert!(source_fact.is_send() && source_fact.is_share(), "{ty}");
        assert!(mir_fact.is_send() && mir_fact.is_share(), "{ty}");
        assert_eq!(source_fact.is_copy, mir_fact.is_copy);
        assert_eq!(source_fact.requires_drop, mir_fact.requires_drop);
        if ty != "i64" {
            assert!(!mir_fact.is_copy, "{ty}");
        }
    }
}

#[test]
fn forbidden_authority_and_reuse_never_publish_a_source_task_proof() {
    for (index,source) in [
        "async fn bad(value:&i64)->i64{return 0;}async fn main()->i64{return 0;}",
        "async fn bad(value:&mut i64)->i64{return 0;}async fn main()->i64{return 0;}",
        "async fn bad(value:&core::Allocator)->i64{return 0;}async fn main()->i64{return 0;}",
        "async fn bad(value:core::Task<i64>)->i64{return 0;}async fn main()->i64{return 0;}",
        "struct Node{children:core::Vec<Node>,next:core::Option<arena Node>}async fn bad(value:Node)->i64{return 0;}async fn main()->i64{return 0;}",
        "async fn worker(value:core::Bytes)->i64{return 0;}async fn main()->i64{let mut input:core::Bytes=core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator()));let mut child:core::Task<i64>=spawn worker(input);return core::bytes_len(input);}",
    ].into_iter().enumerate() {
        let result=lang::frontend::analyze_virtual(lang_diagnostics::SourceMap::new(),Path::new("forbidden-send.lang"),source.as_bytes(),None,Default::default(),None);
        assert!(result.result.is_err(),"{source}");assert!(!result.diagnostics.is_empty());
        if index<5 {assert!(result.diagnostics.iter().any(|d|d.code==lang_diagnostics::codes::INVALID_TASK),"{:?}",result.diagnostics);}else{assert!(result.diagnostics.iter().any(|d|d.message.to_ascii_lowercase().contains("moved")),"{:?}",result.diagnostics);}
        for diagnostic in &result.diagnostics{diagnostic.validate(&result.sources).unwrap();}
    }
}

#[test]
fn independent_mir_rejects_a_forged_async_loan_signature_at_the_type_span() {
    // Supply a literal marker span while the frontend keeps this helper sync.
    // Its diverging body avoids an unrelated async return-escrow violation.
    let program = prepare(
        "fn /*async*/ local(value:&i64)->i64{return core::panic::<i64>(7);}async fn main()->i64{return 0;}",
    );
    let mut raw = program.verified().typed().clone().into_table();
    let index = raw
        .places
        .declarations
        .functions
        .iter()
        .position(|f| f.name == "local")
        .unwrap();
    let function = &mut raw.places.declarations.functions[index];
    let parameter = function.locals[function.parameters[0].0 as usize].ty;
    let mut effect_span = function.signature_span;
    effect_span.start += 5;
    effect_span.end = effect_span.start + 5;
    function.effect = o::declarations::FunctionEffect::Asynchronous { span: effect_span };
    raw.functions[index].task_scopes.push(b::TaskScope {
        parent: None,
        span: function.span,
    });
    let error = t::verify(raw, Default::default()).unwrap_err();
    assert_eq!(error.kind, t::ErrorKind::UnsupportedTransfer(parameter.ty));
    assert_eq!(error.primary, Some(parameter.span));
}

#[test]
fn independent_mir_consumes_the_owned_spawn_argument_once() {
    let program = prepare(OWNED);
    let mut raw = program.verified().typed().clone().into_table();
    let block = raw
        .functions
        .iter_mut()
        .flat_map(|f| &mut f.blocks)
        .find(|b| {
            b.instructions
                .iter()
                .any(|i| matches!(i.op, b::Op::TaskSpawn { .. }))
        })
        .unwrap();
    let at = block
        .instructions
        .iter()
        .position(|i| matches!(i.op, b::Op::TaskSpawn { .. }))
        .unwrap();
    let b::Op::TaskSpawn { arguments, .. } = &block.instructions[at].op else {
        unreachable!()
    };
    let argument = arguments[0];
    let span = block.instructions[at].span;
    block.instructions.insert(
        at + 1,
        b::Instruction {
            results: vec![],
            op: b::Op::DropValue(argument),
            span,
        },
    );
    let error = t::verify(raw, Default::default()).unwrap_err();
    assert!(matches!(error.kind, t::ErrorKind::Moves(_)), "{error:?}");
    assert_eq!(error.primary, Some(span));
}

#[test]
fn canonical_aliases_keep_capabilities_and_exact_proof_work_limits() {
    let program = prepare(
        "async fn carry(value:core::Vec<core::Bytes>)->core::Vec<core::Bytes>{return value;}async fn main()->i64{return 0;}",
    );
    let mut raw = program.verified().typed().clone().into_table();
    let types = &mut raw.places.declarations.types;
    let index = types
        .constructed
        .iter()
        .position(|t| matches!(t.constructor, o::Constructor::Vec(_)))
        .unwrap();
    let alias = o::Type::Constructed(o::ConstructedId(types.constructed.len() as u32));
    types.constructed.push(types.constructed[index]);
    let baseline = t::verify(raw.clone(), Default::default()).unwrap();
    let types = baseline.typed().places().declarations().types();
    let original = o::Type::Constructed(o::ConstructedId(index as u32));
    assert_eq!(types.canonical(alias), types.canonical(original));
    assert_eq!(types.properties(alias), types.properties(original));
    assert!(types.properties(alias).unwrap().is_send());
    let work = baseline.plan().work_used();
    assert!(work > 0);
    let mut limits = t::Limits::default();
    limits.bodies.places.ownership.unit.work = work;
    t::verify(raw.clone(), limits).unwrap();
    limits.bodies.places.ownership.unit.work = work - 1;
    let error = t::verify(raw, limits).unwrap_err();
    assert!(
        matches!(
            error.kind,
            t::ErrorKind::Resource(_) | t::ErrorKind::Failure(o::failure::ErrorKind::Resource(_))
        ),
        "{error:?}"
    );
}

#[test]
fn arena_loan_allocator_and_claim_types_remain_local_in_both_graphs() {
    let program = prepare(include_str!("fixtures/task_owned_arena.lang"));
    let source = program.hir().loaned().moved().bodies();
    let mut source_local = 0;
    for (id, _) in source.types().iter().enumerate() {
        let p = source
            .properties(lang_hir::resolved::Type::Constructed(
                lang_hir::resolved::ConstructedTypeId(id),
            ))
            .unwrap();
        if p.contains_arena || p.contains_loan || p.contains_allocator || p.contains_task {
            assert!(!p.is_send() && !p.is_share());
            source_local += 1;
        }
    }
    let types = program.verified().typed().places().declarations().types();
    let mut local = 0;
    for (id, ty) in types.table().constructed.iter().enumerate() {
        if matches!(
            ty.constructor,
            o::Constructor::Loan(..)
                | o::Constructor::Arena(_)
                | o::Constructor::ArenaOwner(_)
                | o::Constructor::Task(_)
                | o::Constructor::Builtin(o::Builtin::Allocator)
        ) {
            let p = types
                .properties(o::Type::Constructed(o::ConstructedId(id as u32)))
                .unwrap();
            assert!(!p.is_send() && !p.is_share());
            local += 1;
        }
    }
    assert!(source_local >= 5 && local >= 5);
}
