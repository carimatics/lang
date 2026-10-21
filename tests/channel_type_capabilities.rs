use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_hir::resolved as r;
use lang_mir::unit::{self as m, ownership as o};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::path::Path;

fn analyze(text: &str) -> frontend::Analysis {
    frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("channel-types.lang"),
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

fn carry(definitions: &str, ty: &str) -> String {
    format!("{definitions}fn carry(value:{ty})->{ty}{{return value;}}fn main()->i64{{return 42;}}")
}

#[test]
fn source_and_mir_derive_distinct_capabilities_through_recursive_role_graphs() {
    for (definitions, ty, send, share) in [
        ("", "core::ChannelSetup<i64>", false, false),
        ("", "core::Sender<i64>", true, true),
        ("", "core::Receiver<i64>", true, false),
        ("", "core::Sender<core::Receiver<i64>>", true, true),
        ("", "core::Receiver<core::Sender<i64>>", true, false),
        ("", "shared core::Receiver<i64>", false, false),
        ("", "weak core::Receiver<i64>", false, false),
        ("", "shared core::Sender<core::Receiver<i64>>", true, true),
        ("", "core::Vec<core::Receiver<i64>>", true, false),
        ("", "core::Map<i64,core::Sender<i64>>", true, true),
        ("", "core::Option<core::ChannelSetup<i64>>", false, false),
        (
            "struct Node{inbox:core::Receiver<Node>,children:core::Vec<Node>}",
            "Node",
            true,
            false,
        ),
        (
            "enum Node{Empty,More(core::Sender<Node>,core::Vec<Node>)}",
            "Node",
            true,
            true,
        ),
        (
            "struct Left{right:core::Vec<Right>}struct Right{left:core::Vec<Left>,inbox:core::Receiver<i64>}",
            "Left",
            true,
            false,
        ),
        (
            "struct Right{left:core::Vec<Left>,inbox:core::Receiver<i64>}struct Left{right:core::Vec<Right>}",
            "Left",
            true,
            false,
        ),
    ] {
        let (_, program) = prepare(&carry(definitions, ty));
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
        let z = declarations.types().properties(function.result.ty).unwrap();
        assert_eq!((a.is_send(), a.is_share()), (send, share), "source {ty}");
        assert_eq!((z.is_send(), z.is_share()), (send, share), "MIR {ty}");
        assert!(
            !a.is_copy && a.requires_drop && !z.is_copy && z.requires_drop,
            "{ty}"
        );
        assert!(!a.contains_loan && !a.contains_arena && !a.contains_allocator && !a.contains_task);
        assert!(
            !z.contains_loan
                && !z.contains_arena
                && !z.contains_allocator
                && !z.contains_owner
                && !z.contains_task
        );
    }
}

#[test]
fn unused_channel_message_constructors_require_send_and_exact_arity() {
    for role in ["ChannelSetup", "Sender", "Receiver"] {
        for payload in [
            "&i64",
            "arena i64",
            "core::Allocator",
            "core::Task<i64>",
            "core::ChannelSetup<i64>",
            "shared core::Receiver<i64>",
            "weak core::Receiver<i64>",
        ] {
            let text = carry("", &format!("core::{role}<{payload}>"));
            let out = analyze(&text);
            assert!(out.result.is_err() && !out.diagnostics.is_empty(), "{text}");
            for diagnostic in &out.diagnostics {
                diagnostic.validate(&out.sources).unwrap();
            }
        }
        for ty in [format!("core::{role}"), format!("core::{role}<i64,bool>")] {
            let out = analyze(&carry("", &ty));
            assert!(out.result.is_err() && !out.diagnostics.is_empty(), "{ty}");
        }
    }
    // A forbidden dependency reached through nominal recursion cannot hide in
    // an unused constructor, in either nominal declaration order.
    for definitions in [
        "struct Left{next:core::Vec<Right>}struct Right{next:core::Vec<Left>,setup:core::ChannelSetup<i64>}",
        "struct Right{next:core::Vec<Left>,setup:core::ChannelSetup<i64>}struct Left{next:core::Vec<Right>}",
    ] {
        let out = analyze(&carry(definitions, "core::Sender<Left>"));
        assert!(out.result.is_err() && !out.diagnostics.is_empty());
    }
}

#[test]
fn role_ownership_and_async_signature_exclusions_cannot_be_erased() {
    for text in [
        "fn bad(value:core::Receiver<i64>)->core::Receiver<i64>{let mut first:core::Receiver<i64>=value;return value;}fn main()->i64{return 0;}",
        "async fn bad(value:core::ChannelSetup<i64>)->i64{return 0;}async fn main()->i64{return 0;}",
        "async fn bad(value:shared core::Receiver<i64>)->i64{return 0;}async fn main()->i64{return 0;}",
        "fn bad()->core::Sender<i64>{return core::Sender{};}fn main()->i64{return 0;}",
    ] {
        let out = analyze(text);
        assert!(out.result.is_err() && !out.diagnostics.is_empty(), "{text}");
    }
    // A same-named user type does not forge reserved endpoint authority.
    let (_, program) = prepare(&carry("struct Sender{value:i64}", "core::Sender<i64>"));
    let types = program
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    assert!(
        types
            .properties(o::Type::Nominal(m::DefinitionId(0)))
            .unwrap()
            .is_copy
    );
    assert!(types.channel_feature_span().is_some());
}

fn types(constructors: &[o::Constructor]) -> o::TypeTable {
    o::TypeTable {
        sources: vec![m::Source {
            path: "untrusted.lang".into(),
            text: "x".into(),
        }],
        definitions: vec![],
        constructed: constructors
            .iter()
            .map(|&constructor| o::Constructed {
                constructor,
                span: m::Span {
                    source: m::SourceId(0),
                    start: 0,
                    end: 1,
                },
            })
            .collect(),
    }
}
fn c(id: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(id))
}

#[test]
fn independent_mir_refuses_non_send_messages_without_frontend_facts() {
    for payload in [
        o::Constructor::ChannelSetup(o::Type::I64),
        o::Constructor::Arena(o::Type::I64),
        o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64),
        o::Constructor::Task(o::Type::I64),
        o::Constructor::Builtin(o::Builtin::Allocator),
    ] {
        for role in [
            o::Constructor::ChannelSetup(c(0)),
            o::Constructor::Sender(c(0)),
            o::Constructor::Receiver(c(0)),
        ] {
            let error = types(&[payload, role])
                .verify(Default::default())
                .unwrap_err();
            assert_eq!(error.kind, o::ErrorKind::InvalidChannelMessage(c(0)));
            assert_eq!(
                error.span,
                Some(m::Span {
                    source: m::SourceId(0),
                    start: 0,
                    end: 1
                })
            );
        }
    }
    for shared in [o::Constructor::Shared(c(0)), o::Constructor::Weak(c(0))] {
        let error = types(&[
            o::Constructor::Receiver(o::Type::I64),
            shared,
            o::Constructor::Sender(c(1)),
        ])
        .verify(Default::default())
        .unwrap_err();
        assert_eq!(error.kind, o::ErrorKind::InvalidChannelMessage(c(1)));
    }
}

#[test]
fn role_aliases_keep_capabilities_and_nominally_distinct_roles() {
    let proof = types(&[
        o::Constructor::Receiver(o::Type::I64),
        o::Constructor::Receiver(o::Type::I64),
        o::Constructor::Sender(c(0)),
        o::Constructor::Sender(c(1)),
        o::Constructor::ChannelSetup(o::Type::I64),
    ])
    .verify(Default::default())
    .unwrap();
    for (a, b) in [(0, 1), (2, 3)] {
        assert_eq!(proof.equivalent(c(a), c(b)), Some(true));
        assert_eq!(proof.properties(c(a)), proof.properties(c(b)));
    }
    for (a, b) in [(0, 2), (0, 4), (2, 4)] {
        assert_eq!(proof.equivalent(c(a), c(b)), Some(false));
    }
}

#[test]
fn exact_mir_work_memory_and_element_limits_refuse_one_unit_below() {
    let table = types(&[
        o::Constructor::Receiver(o::Type::I64),
        o::Constructor::Sender(c(0)),
        o::Constructor::Shared(c(1)),
        o::Constructor::Vec(c(2)),
    ]);
    let proof = table.clone().verify(Default::default()).unwrap();
    for kind in 0..3 {
        let mut limits = o::Limits::default();
        let exact = match kind {
            0 => proof.work_used(),
            1 => proof.requested_bytes(),
            _ => proof.elements_used(),
        };
        let slot = match kind {
            0 => &mut limits.unit.work,
            1 => &mut limits.unit.allocations,
            _ => &mut limits.unit.elements,
        };
        *slot = exact;
        table.clone().verify(limits).unwrap();
        let slot = match kind {
            0 => &mut limits.unit.work,
            1 => &mut limits.unit.allocations,
            _ => &mut limits.unit.elements,
        };
        *slot = exact - 1;
        assert!(table.clone().verify(limits).is_err());
    }
}

#[test]
fn unused_channel_metadata_is_refused_by_layout_wire_and_native_package_profiles() {
    for role in [
        "ChannelSetup<i64>",
        "Sender<i64>",
        "Receiver<i64>",
        "SendResult<i64>",
        "ChannelError",
    ] {
        let text = carry("", &format!("core::{role}"));
        let (sources, program) = prepare(&text);
        let typed = program.mir().activity().moved().typed();
        let span = typed
            .places()
            .declarations()
            .types()
            .channel_feature_span()
            .unwrap();
        assert!(
            matches!(lang_runtime::layout::Layouts::compile(typed.places().declarations().types(),Default::default()),Err(lang_runtime::layout::Error::UnsupportedChannelProfile{span:s}) if s==span)
        );
        let error =
            lang_bytecode::ownership::encode(typed.clone().into_table(), None, Default::default())
                .unwrap_err();
        assert!(
            matches!(error.kind,lang_bytecode::ownership::CodecErrorKind::UnsupportedChannelProfile{span:s} if s==span)
        );
        for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let result = lang::native::compile(
                Program::Ownership(program.clone()),
                &sources,
                Default::default(),
                lang::native::target(target).unwrap(),
            );
            assert!(
                matches!(result, Err(lang::native::Error::OwnedCodec(ref error)) if matches!(error.kind,lang_bytecode::ownership::CodecErrorKind::UnsupportedChannelProfile{span:s} if s==span)),
                "{role} {target}"
            );
        }
    }
}

#[test]
fn source_graph_retains_role_identity_in_all_unused_type_entries() {
    let (_, program) = prepare(&carry("", "core::Sender<core::Receiver<i64>>"));
    let source = program.hir().loaned().moved().bodies();
    let mut roles = 0;
    for (id, node) in source.types().iter().enumerate() {
        let p = source
            .properties(r::Type::Constructed(r::ConstructedTypeId(id)))
            .unwrap();
        match node.constructor {
            r::TypeConstructor::Sender(_) => {
                assert!(p.is_send() && p.is_share());
                roles += 1;
            }
            r::TypeConstructor::Receiver(_) => {
                assert!(p.is_send() && !p.is_share());
                roles += 1;
            }
            _ => {}
        }
    }
    assert_eq!(roles, 2);
}

#[test]
fn source_graph_work_memory_and_depth_are_bounded_independently() {
    let (sources, program) = prepare(&carry(
        "struct Node{inbox:core::Receiver<Node>,children:core::Vec<Node>}",
        "core::Sender<Node>",
    ));
    let source = program.hir().loaned().moved().bodies().source();
    let baseline = lang_hir::types::analyze(source, &sources, Default::default())
        .unwrap()
        .facts
        .unwrap();
    for memory in [false, true] {
        let mut limits = lang_hir::types::Limits::default();
        let exact = if memory {
            baseline.allocation_bytes()
        } else {
            baseline.work_used()
        };
        let slot = if memory {
            &mut limits.allocations
        } else {
            &mut limits.work
        };
        *slot = exact;
        lang_hir::types::analyze(source, &sources, limits)
            .unwrap()
            .facts
            .unwrap();
        let slot = if memory {
            &mut limits.allocations
        } else {
            &mut limits.work
        };
        *slot = exact - 1;
        assert!(lang_hir::types::analyze(source, &sources, limits).is_err());
    }
    let limits = lang_hir::types::Limits {
        depth: 1,
        ..Default::default()
    };
    assert!(lang_hir::types::analyze(source, &sources, limits).is_err());
}

#[test]
fn task_channel_vm_and_native_profiles_are_explicit_while_old_layouts_refuse() {
    for text in [
        "async fn carry(value:core::Sender<i64>)->core::Sender<i64>{yield;return value;}async fn main()->i64{return 42;}",
        "async fn carry(value:core::Receiver<i64>)->core::Receiver<i64>{yield;return value;}async fn main()->i64{return 42;}",
        "async fn send(sender:core::Sender<i64>)->core::SendResult<i64>{return await core::channel_send::<i64>(&sender,20);}async fn main()->i64{return 42;}",
        "fn main()->i64{let mut setup:core::ChannelSetup<i64>=core::result_unwrap::<core::ChannelSetup<i64>,core::ChannelError>(core::channel_new::<i64>(core::allocator(),1));return 42;}",
    ] {
        let out = analyze(text);
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        let Program::Tasks(program) = out.result.unwrap() else {
            panic!("task proof")
        };
        let types = program.verified().typed().places().declarations().types();
        let span = types.channel_feature_span().unwrap();
        assert!(
            matches!(lang_runtime::layout::Layouts::compile(types,Default::default()),Err(lang_runtime::layout::Error::UnsupportedChannelProfile{span:s}) if s==span)
        );
        assert!(
            matches!(lang_runtime::layout::Layouts::compile_tasks(program.verified(),Default::default()),Err(lang_runtime::layout::Error::UnsupportedChannelProfile{span:s}) if s==span)
        );
        let mut allocator = lang_runtime::allocation::Allocator::new(
            lang_runtime::allocation::System,
            Default::default(),
        )
        .unwrap();
        let result = lang_vm::unit::tasks::execute_entry(
            program.verified(),
            &mut allocator,
            lang_runtime::execution::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
        assert_eq!(result.value, 42);
        assert_eq!(allocator.stats().live_allocations, 0);
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )
        .unwrap();
        let result = native
            .execute(
                &mut allocator,
                lang_runtime::execution::Control::with_limits(Default::default()),
                Default::default(),
            )
            .unwrap();
        assert_eq!(result.value, 42);
        assert_eq!(allocator.stats().live_allocations, 0);
    }
}

#[test]
fn malformed_role_references_and_constructor_depth_never_publish_mir_facts() {
    let error = types(&[o::Constructor::Sender(c(0))])
        .verify(Default::default())
        .unwrap_err();
    assert_eq!(
        error.kind,
        o::ErrorKind::ForwardConstructed(o::ConstructedId(0))
    );
    let table = types(&[
        o::Constructor::Receiver(o::Type::I64),
        o::Constructor::Sender(c(0)),
    ]);
    let mut limits = o::Limits {
        constructed: 1,
        ..Default::default()
    };
    assert!(table.clone().verify(limits).is_err());
    limits = Default::default();
    limits.unit.depth = 2;
    assert!(table.verify(limits).is_err());
}
