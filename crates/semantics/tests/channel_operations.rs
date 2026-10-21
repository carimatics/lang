use lang_diagnostics::{SourceMap, codes};
use lang_mir::unit::ownership::{self as o, bodies as b};
use lang_semantics::{
    analysis::{self, Program},
    binding::{BindingLimits, bind_program},
    modules::{ModuleLimits, load_virtual},
};
use std::path::Path;

fn analyze(text: &str) -> (SourceMap, analysis::Analysis) {
    let mut sources = SourceMap::new();
    let loaded = load_virtual(
        &mut sources,
        Path::new("channel-operations.lang"),
        text.as_bytes(),
        None,
        ModuleLimits::default(),
    )
    .unwrap();
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let bound = bind_program(&loaded.graph.unwrap(), &sources, BindingLimits::default()).unwrap();
    assert!(bound.diagnostics.is_empty(), "{:?}", bound.diagnostics);
    let checked = analysis::analyze(
        bound.program.unwrap(),
        &sources,
        Default::default(),
        Default::default(),
    )
    .unwrap();
    (sources, checked)
}
fn checked(text: &str) -> Box<analysis::TaskProgram> {
    let (_, checked) = analyze(text);
    assert!(checked.diagnostics.is_empty(), "{:?}", checked.diagnostics);
    let Some(Program::Tasks(program)) = checked.program else {
        panic!("channel task proof")
    };
    program
}

#[test]
fn direct_await_uses_explicit_channel_mir_and_has_no_child_task_result_wrapper() {
    for (ty, payload) in [("i64", "value"), ("core::Bytes", "value")] {
        let text = format!(
            "async fn send(sender:core::Sender<{ty}>,value:{ty})->core::SendResult<{ty}>{{return await core::channel_send::<{ty}>(&sender,{payload});}}async fn receive(argument:core::Receiver<{ty}>)->core::Option<{ty}>{{let mut receiver:core::Receiver<{ty}>=argument;return await core::channel_receive::<{ty}>(&mut receiver);}}"
        );
        let program = checked(&text);
        let typed = program.verified().typed();
        let mut sends = 0;
        let mut receives = 0;
        let mut captures = 0;
        for f in typed.functions() {
            for i in f.blocks.iter().flat_map(|b| &b.instructions) {
                match i.op {
                    b::Op::ChannelSend { .. } => sends += 1,
                    b::Op::ChannelReceive { .. } => receives += 1,
                    b::Op::PrepareChannel { .. } => captures += 1,
                    b::Op::TaskSpawn { .. } | b::Op::TaskAwait(_) => {
                        panic!("intrinsic became a child task")
                    }
                    _ => {}
                }
            }
        }
        assert_eq!((sends, receives, captures), (1, 1, 2));
        let types = typed.places().declarations().types();
        let result = typed.places().declarations().functions()[0].result.ty;
        assert!(
            matches!(result,o::Type::Constructed(id) if matches!(types.table().constructed[id.0 as usize].constructor,o::Constructor::SendResult(_)))
        );
    }
}

#[test]
fn explicit_reference_bindings_retain_their_lexical_suspension_rejection() {
    for source in [
        "async fn f(sender:core::Sender<i64>)->core::SendResult<i64>{let mut reference:&core::Sender<i64>=&sender;return await core::channel_send::<i64>(reference,20);}",
        "async fn f(input:core::Receiver<i64>)->core::Option<i64>{let mut receiver:core::Receiver<i64>=input;let mut reference:&mut core::Receiver<i64>=&mut receiver;return await core::channel_receive::<i64>(reference);}",
    ] {
        let (sources, out) = analyze(source);
        assert!(out.program.is_none());
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == codes::BORROW_ACROSS_SUSPENSION),
            "{:?}",
            out.diagnostics
        );
        for diagnostic in &out.diagnostics {
            diagnostic.validate(&sources).unwrap();
        }
    }
}

#[test]
fn synchronous_channel_construction_selects_the_task_profile_without_hidden_suspension() {
    let program = checked(
        "fn setup()->core::ChannelSetup<i64>{return core::result_unwrap::<core::ChannelSetup<i64>,core::ChannelError>(core::channel_new::<i64>(core::allocator(),1));}fn main()->i64{let mut value:core::ChannelSetup<i64>=setup();return 0;}",
    );
    assert!(program.verified().typed().functions().iter().flat_map(|f|&f.blocks).flat_map(|b|&b.instructions).any(|i|matches!(&i.op,b::Op::CoreCall(call) if call.operation==o::core::Operation::ChannelNew)));
}
#[test]
fn concrete_owned_channel_execution_fixture_passes_both_source_and_mir_proofs() {
    checked(include_str!("../../../tests/fixtures/task_channels.lang"));
}

#[test]
fn bare_send_receive_and_spawn_intrinsics_are_invalid_task_effects() {
    for source in [
        "async fn f(sender:core::Sender<i64>)->core::SendResult<i64>{return core::channel_send::<i64>(&sender,20);}",
        "fn f(sender:core::Sender<i64>)->core::SendResult<i64>{return await core::channel_send::<i64>(&sender,20);}",
        "async fn f(sender:core::Sender<i64>)->i64{let mut bad:core::Task<i64>=spawn core::channel_send::<i64>(&sender,20);return 0;}",
        "async fn f(input:core::Receiver<i64>)->core::Option<i64>{let mut receiver:core::Receiver<i64>=input;return core::channel_receive::<i64>(&mut receiver);}",
    ] {
        let (_, out) = analyze(source);
        assert!(out.program.is_none());
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == codes::INVALID_TASK),
            "{:?}",
            out.diagnostics
        );
    }
}

#[test]
fn send_result_cases_are_ordinary_owned_data_and_both_failures_are_extractable() {
    checked(
        "fn closed(value:core::Bytes)->core::SendResult<core::Bytes>{return core::SendResult::Closed(value);}fn invalid(value:core::Bytes)->core::SendResult<core::Bytes>{return core::SendResult::InvalidPublication(value);}fn sent()->core::SendResult<core::Bytes>{return core::SendResult::Sent;}fn extract(value:core::SendResult<core::Bytes>)->core::Bytes{return core::channel_unsent::<core::Bytes>(value);}",
    );
}

#[test]
fn loop_preparation_reuses_only_a_vacated_static_authority_slot() {
    checked(
        "async fn send(sender:core::Sender<i64>)->i64{let mut count:i64=0;while count<2{let mut result:core::SendResult<i64>=await core::channel_send::<i64>(&sender,count);count=count+1;}return count;}async fn receive(input:core::Receiver<i64>)->i64{let mut receiver:core::Receiver<i64>=input;let mut count:i64=0;while count<2{let mut result:core::Option<i64>=await core::channel_receive::<i64>(&mut receiver);count=count+1;}return count;}",
    );
}

#[test]
fn sending_consumes_an_owned_payload_but_preserves_copy_arguments() {
    checked(
        "async fn f(sender:core::Sender<i64>,value:i64)->i64{let mut result:core::SendResult<i64>=await core::channel_send::<i64>(&sender,value);return value;}",
    );
    let (sources, out) = analyze(
        "async fn f(sender:core::Sender<core::Bytes>,value:core::Bytes)->core::Bytes{let mut result:core::SendResult<core::Bytes>=await core::channel_send::<core::Bytes>(&sender,value);return value;}",
    );
    assert!(out.program.is_none());
    let diagnostic = out
        .diagnostics
        .iter()
        .find(|d| d.code == codes::INVALID_OWNER_USE)
        .expect("moved payload diagnostic");
    diagnostic.validate(&sources).unwrap();
}

#[test]
fn extracting_either_failure_preserves_arena_provenance() {
    for case in ["Closed", "InvalidPublication"] {
        let source = format!(
            "fn f(a:&core::Allocator)->i64{{let mut outside:core::Option<arena i64>=core::Option::None;arena graph(a){{let mut made:core::Result<arena i64,core::ArenaError>=core::arena_new::<i64>(graph,7);match made{{core::Result::Ok(token)=>{{outside=core::Option::Some(core::channel_unsent::<arena i64>(core::SendResult::{case}(token)));}},core::Result::Err(_)=>{{}},}}}}return 0;}}"
        );
        let (sources, out) = analyze(&source);
        assert!(out.program.is_none(), "{case}");
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.code == codes::REGION_ESCAPE),
            "{case}: {:?}",
            out.diagnostics
        );
        for diagnostic in &out.diagnostics {
            diagnostic.validate(&sources).unwrap();
        }
    }
}
