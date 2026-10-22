use lang::frontend;
use lang_diagnostics::SourceMap;
use lang_mir::unit::{self as m, ownership as o};
use lang_semantics::analysis::{OwnershipProgram, Program};
use std::path::Path;

fn analyze(text: &str) -> frontend::Analysis {
    frontend::analyze_virtual(
        SourceMap::new(),
        Path::new("service-types.lang"),
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
fn source_and_independent_mir_keep_service_owner_and_producer_capabilities_distinct() {
    for (definitions, ty, send, share) in [
        (
            "",
            "core::ServiceSetup<core::Bytes,core::Receiver<i64>>",
            false,
            false,
        ),
        (
            "",
            "core::ServiceOwner<core::Bytes,core::Receiver<i64>>",
            true,
            false,
        ),
        ("", "core::ServiceHandle<core::Receiver<i64>>", true, true),
        (
            "",
            "core::Vec<core::ServiceOwner<core::Text,core::Bytes>>",
            true,
            false,
        ),
        ("", "shared core::ServiceOwner<i64,i64>", false, false),
        ("", "weak core::ServiceOwner<i64,i64>", false, false),
        (
            "",
            "shared core::ServiceHandle<core::ServiceOwner<i64,i64>>",
            true,
            true,
        ),
        (
            "struct Node{owner:core::ServiceOwner<Node,i64>,children:core::Vec<Node>}",
            "Node",
            true,
            false,
        ),
        (
            "struct Left{right:core::Vec<Right>}struct Right{left:core::Vec<Left>,owner:core::ServiceOwner<i64,i64>}",
            "Left",
            true,
            false,
        ),
        (
            "struct Right{left:core::Vec<Left>,owner:core::ServiceOwner<i64,i64>}struct Left{right:core::Vec<Right>}",
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
        let b = declarations.types().properties(function.result.ty).unwrap();
        assert_eq!((a.is_send(), a.is_share()), (send, share), "source {ty}");
        assert_eq!((b.is_send(), b.is_share()), (send, share), "MIR {ty}");
        assert!(
            !a.is_copy && a.requires_drop && !b.is_copy && b.requires_drop,
            "{ty}"
        );
    }
}

#[test]
fn unused_service_state_and_message_require_send_and_exact_arity() {
    for payload in [
        "&i64",
        "arena i64",
        "core::Allocator",
        "core::Task<i64>",
        "core::ChannelSetup<i64>",
        "core::ServiceSetup<i64,i64>",
        "shared core::ServiceOwner<i64,i64>",
    ] {
        for ty in [
            format!("core::ServiceSetup<{payload},i64>"),
            format!("core::ServiceSetup<i64,{payload}>"),
            format!("core::ServiceOwner<{payload},i64>"),
            format!("core::ServiceOwner<i64,{payload}>"),
            format!("core::ServiceHandle<{payload}>"),
        ] {
            let out = analyze(&carry("", &ty));
            assert!(out.result.is_err() && !out.diagnostics.is_empty(), "{ty}");
            for diagnostic in &out.diagnostics {
                diagnostic.validate(&out.sources).unwrap();
            }
        }
    }
    for ty in [
        "core::ServiceSetup",
        "core::ServiceSetup<i64>",
        "core::ServiceSetup<i64,i64,i64>",
        "core::ServiceOwner",
        "core::ServiceOwner<i64>",
        "core::ServiceOwner<i64,i64,i64>",
        "core::ServiceHandle",
        "core::ServiceHandle<i64,i64>",
    ] {
        let out = analyze(&carry("", ty));
        assert!(out.result.is_err() && !out.diagnostics.is_empty(), "{ty}");
    }
}

fn span() -> m::Span {
    m::Span {
        source: m::SourceId(0),
        start: 0,
        end: 1,
    }
}
fn c(id: u32) -> o::Type {
    o::Type::Constructed(o::ConstructedId(id))
}
fn types(constructors: &[o::Constructor]) -> o::TypeTable {
    o::TypeTable {
        sources: vec![m::Source {
            path: "service.lang".into(),
            text: "x".into(),
        }],
        definitions: vec![],
        constructed: constructors
            .iter()
            .map(|&constructor| o::Constructed {
                constructor,
                span: span(),
            })
            .collect(),
    }
}

#[test]
fn independent_mir_rejects_non_send_state_and_messages_without_source_facts() {
    for payload in [
        o::Constructor::Loan(o::LoanKind::Shared, o::Type::I64),
        o::Constructor::Arena(o::Type::I64),
        o::Constructor::Task(o::Type::I64),
        o::Constructor::Builtin(o::Builtin::Allocator),
        o::Constructor::ServiceSetup(o::Type::I64, o::Type::I64),
    ] {
        for role in [
            o::Constructor::ServiceSetup(c(0), o::Type::I64),
            o::Constructor::ServiceSetup(o::Type::I64, c(0)),
            o::Constructor::ServiceOwner(c(0), o::Type::I64),
            o::Constructor::ServiceOwner(o::Type::I64, c(0)),
            o::Constructor::ServiceHandle(c(0)),
        ] {
            let error = types(&[payload, role])
                .verify(Default::default())
                .unwrap_err();
            assert_eq!(error.kind, o::ErrorKind::InvalidServiceType(c(0)));
            assert_eq!(error.span, Some(span()));
        }
    }
}

#[test]
fn unused_service_metadata_stays_out_of_old_layout_wire_and_both_native_packages() {
    for role in [
        "ServiceSetup<i64,i64>",
        "ServiceOwner<i64,i64>",
        "ServiceHandle<i64>",
        "ServiceError",
    ] {
        let (sources, program) = prepare(&carry("", &format!("core::{role}")));
        let typed = program.mir().activity().moved().typed();
        let types = typed.places().declarations().types();
        let span = types.service_feature_span().unwrap();
        assert!(
            matches!(lang_runtime::layout::Layouts::compile(types,Default::default()),
            Err(lang_runtime::layout::Error::UnsupportedServiceProfile{span:s}) if s==span)
        );
        let error =
            lang_bytecode::ownership::encode(typed.clone().into_table(), None, Default::default())
                .unwrap_err();
        assert!(
            matches!(error.kind,lang_bytecode::ownership::CodecErrorKind::UnsupportedServiceProfile{span:s} if s==span)
        );
        for target in ["aarch64-apple-darwin", "x86_64-pc-windows-msvc"] {
            let result = lang::native::compile(
                Program::Ownership(program.clone()),
                &sources,
                Default::default(),
                lang::native::target(target).unwrap(),
            );
            assert!(
                matches!(result,Err(lang::native::Error::OwnedCodec(ref error)) if matches!(error.kind,
                lang_bytecode::ownership::CodecErrorKind::UnsupportedServiceProfile{span:s} if s==span)),
                "{role} {target}"
            );
        }
    }
}

#[test]
fn synchronous_service_calls_preserve_owner_moves_and_state_read_provenance() {
    let out = analyze(
        r#"
struct Counter { total:i64 }
fn add(owner:&mut core::ServiceOwner<Counter,i64>,amount:i64)->core::Unit {
    let mut total:i64=0;
    match core::service_read::<Counter,i64>(owner) {
        core::Result::Ok(state) => { total=state.total; },
        core::Result::Err(error) => { return core::panic::<core::Unit>(-6); },
    }
    return core::result_unwrap::<core::Unit,core::ServiceError>(
        core::service_replace::<Counter,i64>(owner,Counter { total:total+amount }));
}
fn make()->core::ServiceHandle<i64> {
    let mut setup:core::ServiceSetup<Counter,i64>=core::result_unwrap::<core::ServiceSetup<Counter,i64>,core::ServiceError>(
        core::service_new::<Counter,i64>(core::allocator(),2,Counter { total:0 }));
    let mut handle:core::ServiceHandle<i64>=core::service_handle::<Counter,i64>(&setup);
    let mut clone:core::ServiceHandle<i64>=core::service_clone::<i64>(&handle);
    let mut owner:core::ServiceOwner<Counter,i64>=core::service_owner::<Counter,i64>(setup);
    let mut added:core::Unit=add(&mut owner,42);
    let mut finished:core::Unit=core::service_finish::<Counter,i64>(&mut owner);
    let mut cancelled:core::Unit=core::service_cancel::<i64>(&handle);
    return clone;
}
fn main()->i64 { return 42; }
"#,
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    assert!(matches!(out.result.unwrap(), Program::Tasks(_)));
}

#[test]
fn producer_state_projection_and_owner_calls_have_the_isolation_identity_and_origin() {
    for body in [
        "return handle.state;",
        "match core::service_read::<i64,i64>(&handle) { core::Result::Ok(state)=>{return 0;}, core::Result::Err(error)=>{return 0;}, }",
        "let mut value:core::Result<core::Unit,core::ServiceError>=core::service_replace::<i64,i64>(&mut handle,0);return 0;",
        "let mut value:core::Unit=core::service_finish::<i64,i64>(&mut handle);return 0;",
    ] {
        let text = format!(
            "fn bad(input:core::ServiceHandle<i64>)->i64{{let mut handle:core::ServiceHandle<i64>=input;{body}}}fn main()->i64{{return 42;}}"
        );
        let out = analyze(&text);
        assert!(out.result.is_err(), "{text}");
        let diagnostic = out
            .diagnostics
            .iter()
            .find(|d| d.code.as_str() == "SRC0029")
            .unwrap_or_else(|| panic!("{text}: {:?}", out.diagnostics));
        diagnostic.validate(&out.sources).unwrap();
        assert!(!diagnostic.secondary.is_empty(), "{diagnostic:?}");
    }
}

#[test]
fn service_owner_moves_and_bound_state_reads_keep_ordinary_ownership_checks() {
    for (definitions, body, code) in [
        (
            "",
            "fn bad(input:core::ServiceSetup<i64,i64>)->i64{let mut first:core::ServiceOwner<i64,i64>=core::service_owner::<i64,i64>(input);let mut second:core::ServiceOwner<i64,i64>=core::service_owner::<i64,i64>(input);return 0;}",
            "SRC0020",
        ),
        (
            "struct Counter{total:i64}",
            "fn bad(owner:&mut core::ServiceOwner<Counter,i64>)->i64{match core::service_read::<Counter,i64>(owner){core::Result::Ok(state)=>{let mut ended:core::Unit=core::service_finish::<Counter,i64>(owner);return state.total;},core::Result::Err(error)=>{return 0;},}}",
            "SRC0021",
        ),
        (
            "struct Counter{total:i64}",
            "async fn bad(input:core::ServiceOwner<Counter,i64>)->i64{let mut owner:core::ServiceOwner<Counter,i64>=input;match core::service_read::<Counter,i64>(&owner){core::Result::Ok(state)=>{yield;return state.total;},core::Result::Err(error)=>{return 0;},}}",
            "SRC0028",
        ),
    ] {
        let text = format!("{definitions}{body}fn main()->i64{{return 42;}}");
        let out = analyze(&text);
        assert!(out.result.is_err(), "{text}");
        assert!(
            out.diagnostics.iter().any(|d| d.code.as_str() == code),
            "{text}: {:?}",
            out.diagnostics
        );
        for diagnostic in &out.diagnostics {
            diagnostic.validate(&out.sources).unwrap();
        }
    }
}

fn service_program() -> Box<lang_semantics::analysis::TaskProgram> {
    let out = analyze(include_str!("fixtures/task_service.lang"));
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let Program::Tasks(program) = out.result.unwrap() else {
        panic!("service task proof")
    };
    program
}

#[test]
fn initial_service_layout_admits_share_state_and_refuses_receiver_state() {
    let program = service_program();
    let layouts = lang_runtime::layout::Layouts::compile_service_tasks(
        program.verified(),
        Default::default(),
    )
    .unwrap();
    let types = program.verified().typed().places().declarations().types();
    for (index, entry) in types.table().constructed.iter().enumerate() {
        let expected = match entry.constructor {
            o::Constructor::ServiceSetup(..)
            | o::Constructor::ServiceOwner(..)
            | o::Constructor::ServiceHandle(..) => Some(32),
            o::Constructor::Builtin(o::Builtin::ServiceError) => Some(8),
            _ => None,
        };
        if let Some(size) = expected {
            assert_eq!(
                layouts
                    .get(o::Type::Constructed(o::ConstructedId(index as u32)))
                    .unwrap()
                    .size,
                size
            );
        }
    }
    let out = analyze(
        "fn carry(owner:core::ServiceOwner<core::Receiver<i64>,i64>)->core::ServiceOwner<core::Receiver<i64>,i64>{return owner;}async fn main()->i64{yield;return 42;}",
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let Program::Tasks(program) = out.result.unwrap() else {
        panic!("task proof")
    };
    assert!(
        lang_runtime::layout::Layouts::compile_service_tasks(
            program.verified(),
            Default::default()
        )
        .is_ok()
    );
}

#[test]
fn owned_service_source_preserves_distinct_state_message_and_capture_authority() {
    use o::bodies::{Op, ServiceRole};
    let program = service_program();
    let typed = program.verified().typed();
    let types = typed.places().declarations().types();
    let mut counts = [0; 3];
    for i in typed
        .functions()
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.instructions)
    {
        match i.op {
            Op::PrepareService {
                role,
                state_type,
                message_type,
                ..
            } => {
                if role == ServiceRole::Owner {
                    assert_eq!(
                        types.equivalent(state_type.unwrap().ty, message_type.ty),
                        Some(false)
                    );
                } else {
                    assert!(state_type.is_none());
                }
                counts[0] += 1;
            }
            Op::ServiceSend { .. } => counts[1] += 1,
            Op::ServiceReceive { .. } => counts[2] += 1,
            _ => {}
        }
    }
    assert_eq!(counts, [4, 2, 2]);
    let mut allocator = lang_runtime::allocation::Allocator::new(
        lang_runtime::allocation::System,
        Default::default(),
    )
    .unwrap();
    let before = allocator.stats();
    let outcome = lang_vm::unit::tasks::execute_entry(
        program.verified(),
        &mut allocator,
        lang_runtime::execution::Control::with_limits(Default::default()),
        Default::default(),
    )
    .unwrap();
    assert_eq!(outcome.value, 42);
    assert_eq!(allocator.stats().live_allocations, before.live_allocations);
    let after = allocator.stats();
    let native =
        lang_codegen_clif::tasks::CompiledProgram::compile(program.verified(), Default::default())
            .unwrap();
    assert_eq!(allocator.stats(), after);
    let outcome = native
        .execute(
            &mut allocator,
            lang_runtime::execution::Control::with_limits(Default::default()),
            Default::default(),
        )
        .unwrap();
    assert_eq!(outcome.value, 42);
    assert_eq!(allocator.stats().live_allocations, 0);
    let span = types.service_feature_span().unwrap();
    assert!(
        matches!(lang_runtime::layout::Layouts::compile_tasks(program.verified(),Default::default()),
        Err(lang_runtime::layout::Error::UnsupportedServiceProfile{span:s}) if s==span)
    );
}

#[test]
fn independent_mir_rejects_forged_service_roles_types_and_channel_capture_aliases() {
    use o::bodies::{Op, ServiceRole};
    let program = service_program();
    for kind in 0..4 {
        let mut table = program.verified().typed().clone().into_table();
        let mut changed = false;
        for i in table
            .functions
            .iter_mut()
            .flat_map(|f| &mut f.blocks)
            .flat_map(|b| &mut b.instructions)
        {
            if changed {
                break;
            }
            match &mut i.op {
                Op::PrepareService { role, .. } if kind == 0 => {
                    *role = if *role == ServiceRole::Handle {
                        ServiceRole::Owner
                    } else {
                        ServiceRole::Handle
                    };
                    changed = true;
                }
                Op::PrepareService {
                    state_type: Some(state),
                    message_type,
                    ..
                } if kind == 1 => {
                    state.ty = message_type.ty;
                    changed = true;
                }
                Op::PrepareService { message_type, .. } if kind == 2 => {
                    message_type.ty = o::Type::I64;
                    changed = true;
                }
                Op::ServiceSend { authority, payload } if kind == 3 => {
                    i.op = Op::ChannelSend {
                        authority: *authority,
                        payload: *payload,
                    };
                    changed = true;
                }
                _ => {}
            }
        }
        assert!(changed);
        let error = o::tasks::verify(table, Default::default()).unwrap_err();
        assert!(
            matches!(
                error.kind,
                o::tasks::ErrorKind::Types(
                    o::ErrorKind::InvalidServiceEndpoint(_) | o::ErrorKind::PreparedKind(_)
                )
            ),
            "kind {kind}: {error:?}"
        );
        assert!(error.primary.is_some());
    }
}

#[test]
fn service_capture_cannot_be_consumed_twice_and_preparation_is_bounded() {
    use o::bodies::Op;
    let program = service_program();
    let table = program.verified().typed().clone().into_table();
    let prepared_total = table.functions.iter().map(|f| f.prepared).sum::<usize>();
    assert!(prepared_total >= 2);
    let mut limits = o::tasks::Limits::default();
    limits.bodies.prepared = prepared_total;
    o::tasks::verify(table.clone(), limits).unwrap();
    limits.bodies.prepared = prepared_total - 1;
    assert!(o::tasks::verify(table.clone(), limits).is_err());
    let mut forged = table;
    let mut changed = false;
    for function in &mut forged.functions {
        let mut first = None;
        for instruction in function.blocks.iter_mut().flat_map(|b| &mut b.instructions) {
            if let Op::ServiceSend { authority, .. } = &mut instruction.op {
                if let Some(previous) = first {
                    *authority = previous;
                    changed = true;
                    break;
                }
                first = Some(*authority);
            }
        }
        if changed {
            break;
        }
    }
    assert!(changed);
    let error = o::tasks::verify(forged, Default::default()).unwrap_err();
    assert!(
        matches!(
            error.kind,
            o::tasks::ErrorKind::Moves(o::moves::ErrorKind::Unavailable {
                slot: o::moves::Slot::Prepared(_),
                ..
            })
        ),
        "{error:?}"
    );
    assert!(error.primary.is_some());
}

#[test]
fn service_async_spelling_and_non_send_captures_remain_checked() {
    for body in [
        "fn bad(handle:&core::ServiceHandle<i64>)->i64{let mut value:core::SendResult<i64>=core::service_send::<i64>(handle,1);return 0;}",
        "async fn bad(handle:core::ServiceHandle<i64>)->i64{let mut child:core::Task<core::SendResult<i64>>=spawn core::service_send::<i64>(&handle,1);return 0;}",
        "async fn bad(input:core::ServiceOwner<i64,i64>)->i64{let mut owner:core::ServiceOwner<i64,i64>=input;let mut value:core::Option<i64>=core::service_receive::<i64,i64>(&mut owner);return 0;}",
        "async fn bad(value:core::ServiceSetup<i64,i64>)->i64{return 0;}",
        "async fn bad(value:shared core::ServiceOwner<i64,i64>)->i64{return 0;}",
        "async fn bad(value:&core::ServiceOwner<i64,i64>)->i64{return 0;}",
    ] {
        let out = analyze(&format!("{body}fn main()->i64{{return 42;}}"));
        assert!(out.result.is_err(), "{body}");
        assert!(
            out.diagnostics.iter().any(|d| d.code.as_str() == "SRC0027"),
            "{body}: {:?}",
            out.diagnostics
        );
        for diagnostic in &out.diagnostics {
            diagnostic.validate(&out.sources).unwrap();
        }
    }
    let out = analyze(
        "struct State{value:core::Bytes}fn bad(owner:&core::ServiceOwner<State,i64>)->i64{match core::service_read::<State,i64>(owner){core::Result::Ok(state)=>{let mut pushed:core::Result<core::Unit,core::BufferError>=core::bytes_push(state.value,1);return 0;},core::Result::Err(error)=>{return 0;},}}fn main()->i64{return 42;}",
    );
    assert!(out.result.is_err());
    assert!(
        out.diagnostics.iter().any(|d| d.code.as_str() == "SRC0021"),
        "{:?}",
        out.diagnostics
    );
}

#[test]
fn service_canonical_aliases_and_exact_type_proof_limits_keep_roles_distinct() {
    let table = types(&[
        o::Constructor::Builtin(o::Builtin::Bytes),
        o::Constructor::Receiver(o::Type::I64),
        o::Constructor::ServiceOwner(c(0), c(1)),
        o::Constructor::ServiceOwner(c(0), c(1)),
        o::Constructor::ServiceHandle(c(2)),
        o::Constructor::ServiceHandle(c(3)),
        o::Constructor::ServiceSetup(c(0), c(1)),
    ]);
    let proof = table.clone().verify(Default::default()).unwrap();
    assert_eq!(proof.equivalent(c(2), c(3)), Some(true));
    assert_eq!(proof.equivalent(c(4), c(5)), Some(true));
    assert_eq!(proof.equivalent(c(2), c(6)), Some(false));
    assert_eq!(proof.equivalent(c(2), c(4)), Some(false));
    for kind in 0..3 {
        let mut limits = o::Limits::default();
        let exact = match kind {
            0 => proof.work_used(),
            1 => proof.requested_bytes(),
            _ => proof.elements_used(),
        };
        match kind {
            0 => limits.unit.work = exact,
            1 => limits.unit.allocations = exact,
            _ => limits.unit.elements = exact,
        }
        table.clone().verify(limits).unwrap();
        match kind {
            0 => limits.unit.work -= 1,
            1 => limits.unit.allocations -= 1,
            _ => limits.unit.elements -= 1,
        }
        assert!(table.clone().verify(limits).is_err());
    }
}

#[test]
fn independent_service_suspension_rejects_an_unended_endpoint_loan() {
    use o::bodies::Op;
    let program = service_program();
    let mut table = program.verified().typed().clone().into_table();
    let mut changed = false;
    for block in table.functions.iter_mut().flat_map(|f| &mut f.blocks) {
        let Some(capture) = block
            .instructions
            .iter()
            .position(|i| matches!(i.op, Op::PrepareService { .. }))
        else {
            continue;
        };
        let suspend = (capture + 1..block.instructions.len())
            .find(|&at| {
                matches!(
                    block.instructions[at].op,
                    Op::ServiceSend { .. } | Op::ServiceReceive { .. }
                )
            })
            .unwrap();
        assert!(suspend > capture + 1);
        // Preserve child-before-parent ending, but delay the complete argument
        // cleanup until after suspension so activity still closes every loan.
        let delayed: Vec<_> = block.instructions.drain(capture + 1..suspend).collect();
        block.instructions.splice(capture + 2..capture + 2, delayed);
        changed = true;
        break;
    }
    assert!(changed);
    let error = o::tasks::verify(table, Default::default()).unwrap_err();
    assert!(
        matches!(
            error.kind,
            o::tasks::ErrorKind::Loans(o::loans::ErrorKind::Suspension(_))
        ),
        "{error:?}"
    );
    assert!(error.primary.is_some() && error.secondary.is_some());
}
