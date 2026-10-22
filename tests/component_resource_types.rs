use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_hir::resolved as r;
use lang_mir::unit::{self as m, ownership as o};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::path::Path;

fn analyze(text: &str) -> frontend::Analysis {
    // Expand a test placeholder, not a source-language alias.
    let text = text.replace("RESOURCE", "core::ComponentResource<First>");
    frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("resource-types.lang"),
        text.as_bytes(),
        None,
        Default::default(),
        None,
    )
}
fn prepare(text: &str) -> (SourceMap, Box<OwnershipProgram>) {
    let out = analyze(text);
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let Program::Ownership(program) = out.result.unwrap() else {
        panic!("ownership proof")
    };
    (out.sources, program)
}
const TAGS: &str = "struct First {} struct Second {}";
fn carry(ty: &str) -> String {
    format!("{TAGS} fn carry(value:{ty})->{ty}{{return value;}} fn main()->i64{{return 42;}}")
}

#[test]
fn source_and_independent_mir_derive_move_drop_and_neither_send_nor_share() {
    for ty in [
        "RESOURCE",
        "core::ComponentResource<Second>",
        "core::Option<RESOURCE>",
        "core::Result<RESOURCE,i64>",
        "core::Vec<RESOURCE>",
        "shared RESOURCE",
        "weak RESOURCE",
    ] {
        let (_, program) = prepare(&carry(ty));
        let source = program.hir().loaned().moved().bodies();
        let function = source
            .source()
            .functions
            .iter()
            .find(|f| f.name.text == "carry")
            .unwrap();
        let a = source.properties(function.result.value_type).unwrap();
        let declarations = program
            .mir()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations();
        let function = declarations
            .functions()
            .iter()
            .find(|f| f.name == "carry")
            .unwrap();
        let b = declarations.types().properties(function.result.ty).unwrap();
        assert_eq!(
            (a.is_copy, a.requires_drop, a.is_send(), a.is_share()),
            (false, true, false, false),
            "source {ty}"
        );
        assert_eq!(
            (b.is_copy, b.requires_drop, b.is_send(), b.is_share()),
            (false, true, false, false),
            "MIR {ty}"
        );
        assert!(!a.contains_loan && !b.contains_loan);
    }
}

#[test]
fn owners_keep_nominal_tag_identity_and_repeated_annotation_equivalence() {
    let (_, program) = prepare(&format!(
        "{TAGS} fn first(a:RESOURCE)->RESOURCE{{return a;}} fn repeated(a:core::ComponentResource<First>)->RESOURCE{{return a;}} fn second(a:core::ComponentResource<Second>)->core::ComponentResource<Second>{{return a;}}"
    ));
    let declarations = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    let result = |name: &str| {
        declarations
            .functions()
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .result
            .ty
    };
    assert_eq!(
        declarations
            .types()
            .equivalent(result("first"), result("repeated")),
        Some(true)
    );
    assert_eq!(
        declarations
            .types()
            .equivalent(result("first"), result("second")),
        Some(false)
    );
    let out = analyze(&format!(
        "{TAGS} fn bad(a:RESOURCE)->core::ComponentResource<Second>{{return a;}}"
    ));
    assert!(out.result.is_err());
    let diagnostic = out
        .diagnostics
        .iter()
        .find(|d| d.message.contains("ComponentResource"))
        .unwrap();
    assert!(
        diagnostic.message.contains("First") && diagnostic.message.contains("Second"),
        "{diagnostic:?}"
    );
}

#[test]
fn tags_are_ordinary_data_and_never_construct_or_project_resource_owners() {
    let out = analyze("struct Tag {} fn main()->i64 { let mut tag:Tag=Tag {}; return 42; }");
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(out.result.is_ok());
    for body in [
        "fn bad()->RESOURCE{return First {};}",
        "fn bad()->RESOURCE{return RESOURCE {};}",
        "fn bad(a:RESOURCE)->i64{return a.field;}",
        "fn bad()->RESOURCE{return core::ComponentResource::<First>();}",
    ] {
        let out = analyze(&format!("{TAGS}{body}"));
        assert!(out.result.is_err(), "{body}");
        assert!(!out.diagnostics.is_empty(), "{body}");
        for d in &out.diagnostics {
            d.validate(&out.sources).unwrap();
        }
    }
}

#[test]
fn source_rejects_bad_arity_non_nominal_tags_and_private_public_signatures() {
    for text in [
        "fn f(a:core::ComponentResource<i64>)->i64{return 0;}",
        "fn f(a:core::ComponentResource<core::Text>)->i64{return 0;}",
        "struct Tag {} fn f(a:core::ComponentResource)->i64{return 0;}",
        "struct Tag {} fn f(a:core::ComponentResource<Tag,Tag>)->i64{return 0;}",
        "struct Tag {} pub fn f(a:core::ComponentResource<Tag>)->i64{return 0;}",
    ] {
        let out = analyze(text);
        assert!(out.result.is_err(), "{text}");
        for d in &out.diagnostics {
            d.validate(&out.sources).unwrap();
        }
    }
}

#[test]
fn source_and_mir_reject_even_unused_forged_tag_ids_with_original_spans() {
    let (sources, program) = prepare(&carry("RESOURCE"));
    let mut source = program.hir().loaned().moved().bodies().source().clone();
    let node = source
        .types
        .iter_mut()
        .find(|t| matches!(t.constructor, r::TypeConstructor::ComponentResource(_)))
        .unwrap();
    let span = node.span;
    assert!(sources.slice(span).unwrap().contains("ComponentResource"));
    node.constructor = r::TypeConstructor::ComponentResource(r::DefinitionId(usize::MAX));
    assert!(
        matches!(lang_hir::types::analyze(&source,&sources,Default::default()),Err(lang_hir::types::Error::Input{span:Some(s),..}) if s==span)
    );
    let types = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let mut table = types.table().clone();
    let node = table
        .constructed
        .iter_mut()
        .find(|t| matches!(t.constructor, o::Constructor::ComponentResource(_)))
        .unwrap();
    let span = node.span;
    node.constructor = o::Constructor::ComponentResource(m::DefinitionId(u32::MAX));
    let error = table.verify(Default::default()).unwrap_err();
    assert_eq!(
        error.kind,
        o::ErrorKind::InvalidType(o::Type::Nominal(m::DefinitionId(u32::MAX)))
    );
    assert_eq!(error.span, Some(span));
}

#[test]
fn independent_mir_rejects_a_forged_public_owner_signature_with_a_private_tag() {
    let (_, program) = prepare(&carry("RESOURCE"));
    let mut table = program
        .mir()
        .activity()
        .moved()
        .typed()
        .clone()
        .into_table();
    let function = table
        .places
        .declarations
        .functions
        .iter_mut()
        .find(|f| f.name == "carry")
        .unwrap();
    function.public = true;
    let ty = function.result;
    let error = table.verify(Default::default()).unwrap_err();
    assert_eq!(error.kind, o::ErrorKind::PrivateSignatureType(ty.ty));
    assert_eq!(error.span, Some(ty.span));
}

#[test]
fn synchronous_direct_loans_end_before_the_owner_moves() {
    let (_, program) = prepare(&format!(
        "{TAGS} fn read(a:&RESOURCE)->i64{{return 0;}} fn write(a:&mut RESOURCE)->i64{{return 0;}} fn carry(a:RESOURCE)->RESOURCE{{let mut owner:RESOURCE=a;let mut first:i64=read(&owner);let mut second:i64=write(&mut owner);return owner;}}"
    ));
    let types = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    assert!(types.component_feature_span().is_some());
}

#[test]
fn ordinary_owner_moves_and_loans_cannot_duplicate_escape_or_suspend() {
    for (body, code) in [
        (
            "fn bad(a:RESOURCE)->RESOURCE{let mut b:RESOURCE=a;return a;}",
            "SRC0020",
        ),
        (
            "fn bad(a:RESOURCE)->RESOURCE{let mut b:RESOURCE=a;let mut c:RESOURCE=a;return b;}",
            "SRC0020",
        ),
        ("fn bad(a:&RESOURCE)->&RESOURCE{return a;}", "SRC0022"),
        ("async fn bad(a:RESOURCE)->i64{yield;return 0;}", "SRC0027"),
    ] {
        let out = analyze(&format!("{TAGS}{body}"));
        assert!(out.result.is_err(), "{body}");
        assert!(
            out.diagnostics.iter().any(|d| d.code.as_str() == code),
            "{body}: {:?}",
            out.diagnostics
        );
    }
}

#[test]
fn unused_resource_metadata_refuses_old_layout_wire_and_both_native_packages() {
    let (sources, program) = prepare(&carry("RESOURCE"));
    let typed = program.mir().activity().moved().typed();
    let types = typed.places().declarations().types();
    let span = types.component_feature_span().unwrap();
    assert!(
        matches!(lang_runtime::layout::Layouts::compile(types,Default::default()),Err(lang_runtime::layout::Error::UnsupportedComponentProfile{span:s}) if s==span)
    );
    let error =
        lang_bytecode::ownership::encode(typed.clone().into_table(), None, Default::default())
            .unwrap_err();
    assert!(
        matches!(error.kind,lang_bytecode::ownership::CodecErrorKind::UnsupportedComponentProfile{span:s} if s==span)
    );
    for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
        let result = lang::native::compile(
            Program::Ownership(program.clone()),
            &sources,
            Default::default(),
            lang::native::target(target).unwrap(),
        );
        assert!(
            matches!(result,Err(lang::native::Error::OwnedCodec(ref e)) if matches!(e.kind,lang_bytecode::ownership::CodecErrorKind::UnsupportedComponentProfile{span:s} if s==span)),
            "{target}"
        );
    }
}

#[test]
fn independent_move_proof_refuses_retiring_a_transferred_owner_twice() {
    let (_, program) = prepare(&carry("RESOURCE"));
    let moved = program.mir().activity().moved();
    let index = moved
        .typed()
        .places()
        .declarations()
        .functions()
        .iter()
        .position(|f| f.name == "carry")
        .unwrap();
    assert!(moved.functions()[index].exits()[0].owners().is_empty());
    let mut table = moved.typed().clone().into_table();
    let block = &mut table.functions[index].blocks[0];
    let o::bodies::Term::Return(value) = block.terminator.kind else {
        panic!("return")
    };
    block.instructions.push(o::bodies::Instruction {
        results: vec![],
        op: o::bodies::Op::DropValueIfAvailable(value),
        span: block.span,
    });
    let typed = table.verify(Default::default()).unwrap();
    assert!(matches!(
        o::moves::analyze(typed, Default::default())
            .unwrap_err()
            .kind,
        o::moves::ErrorKind::Unavailable { .. }
    ));
}

#[test]
fn tag_fields_do_not_become_resource_payload_or_region_dependencies() {
    let (_, program) = prepare(
        "struct Tag{state:arena i64} fn carry(a:core::ComponentResource<Tag>)->core::ComponentResource<Tag>{return a;}",
    );
    let source = program.hir().loaned().moved().bodies();
    let owner = source.source().functions[0].result.value_type;
    assert!(!source.properties(owner).unwrap().contains_arena);
    let types = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let owner = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .functions()[0]
        .result
        .ty;
    assert!(!types.properties(owner).unwrap().contains_arena);
}

#[test]
fn opaque_type_graph_limits_are_exact_and_cumulative() {
    let (sources, program) = prepare(&carry("RESOURCE"));
    let source = program.hir().loaned().moved().bodies().source();
    let facts = lang_hir::types::analyze(source, &sources, Default::default())
        .unwrap()
        .facts
        .unwrap();
    let exact = lang_hir::types::Limits {
        work: facts.work_used(),
        allocations: facts.allocation_bytes(),
        ..Default::default()
    };
    assert!(
        lang_hir::types::analyze(source, &sources, exact)
            .unwrap()
            .facts
            .is_some()
    );
    for limits in [
        lang_hir::types::Limits {
            work: exact.work - 1,
            ..exact
        },
        lang_hir::types::Limits {
            allocations: exact.allocations - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            lang_hir::types::analyze(source, &sources, limits),
            Err(lang_hir::types::Error::Limit(_))
        ));
    }
    let raw = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types()
        .table();
    let verified = raw.clone().verify(Default::default()).unwrap();
    let exact = m::Limits {
        work: verified.work_used(),
        allocations: verified.requested_bytes(),
        elements: verified.elements_used(),
        ..Default::default()
    };
    assert!(
        raw.clone()
            .verify(o::Limits {
                unit: exact,
                ..Default::default()
            })
            .is_ok()
    );
    for unit in [
        m::Limits {
            work: exact.work - 1,
            ..exact
        },
        m::Limits {
            allocations: exact.allocations - 1,
            ..exact
        },
        m::Limits {
            elements: exact.elements - 1,
            ..exact
        },
    ] {
        assert!(matches!(
            raw.clone()
                .verify(o::Limits {
                    unit,
                    ..Default::default()
                })
                .unwrap_err()
                .kind,
            o::ErrorKind::Mir(m::ErrorKind::Limit { .. })
        ));
    }
    let mut budget = m::Budget::new(exact);
    raw.clone()
        .verify_with_budget(&mut budget, o::Limits::default().constructed)
        .unwrap();
    let before = (
        budget.work_used(),
        budget.allocation_bytes(),
        budget.elements_used(),
    );
    assert!(
        raw.clone()
            .verify_with_budget(&mut budget, o::Limits::default().constructed)
            .is_err()
    );
    let after = (
        budget.work_used(),
        budget.allocation_bytes(),
        budget.elements_used(),
    );
    assert!(after.0 >= before.0 && after.1 >= before.1 && after.2 >= before.2);
}

#[test]
fn every_current_task_layout_refuses_unused_resource_metadata() {
    let out = analyze(&format!(
        "{TAGS} fn unused(a:RESOURCE)->RESOURCE{{return a;}} async fn main()->i64{{yield;return 42;}}"
    ));
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let Program::Tasks(program) = out.result.unwrap() else {
        panic!("task proof")
    };
    let proof = program.verified();
    let span = proof
        .typed()
        .places()
        .declarations()
        .types()
        .component_feature_span()
        .unwrap();
    for result in [
        lang_runtime::layout::Layouts::compile_tasks(proof, Default::default()),
        lang_runtime::layout::Layouts::compile_channel_tasks(proof, Default::default()),
        lang_runtime::layout::Layouts::compile_service_tasks(proof, Default::default()),
    ] {
        assert!(
            matches!(result,Err(lang_runtime::layout::Error::UnsupportedComponentProfile{span:s}) if s==span)
        );
    }
}

#[test]
fn vm_refuses_resource_metadata_before_fuel_storage_or_guest_effects() {
    use lang_runtime::{allocation as a, cleanup as c};
    use lang_vm::unit::{Control, owned as vm};
    let (_, program) = prepare(&carry("RESOURCE"));
    let declarations = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    let main = m::FunctionId(
        declarations
            .functions()
            .iter()
            .position(|f| f.name == "main")
            .unwrap() as u32,
    );
    let mut instance = c::Instance::default();
    let error = vm::execute_function::<a::System>(
        program.failure_flow(),
        main,
        &mut instance,
        None,
        Control::with_limits(Default::default()),
    )
    .err()
    .unwrap();
    assert!(
        matches!(
            error.reason,
            vm::Reason::Preparation(
                lang_runtime::layout::Error::UnsupportedComponentProfile { .. }
            )
        ),
        "{:?}",
        error.reason
    );
    assert_eq!(error.usage.fuel, 0);
    assert!(error.report.is_none());
    assert!(!instance.is_poisoned());
}
