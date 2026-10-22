//! Executable backing checkpoint while the production contextual adapter is connected.
use lang_runtime::{
    allocation::{Allocator, Request, System},
    component::{
        adapter::{
            AdapterLimits, CallOutcome, CanonicalOwnership, CanonicalResourceTable,
            TransferArgument, preflight_call,
        },
        host_value::HostResourceValues,
        negotiation::{InterfaceKey, PackageKey, ResourceTypeKey},
    },
};

fn kind(name: &str) -> ResourceTypeKey {
    ResourceTypeKey::new(
        InterfaceKey::new(PackageKey::new("demo", "counter"), "api"),
        name,
    )
}

#[test]
fn actual_allocator_backing_survives_preflight_refusal_then_moves_and_retires_once() {
    let mut allocator = Allocator::new(System, Default::default()).unwrap();
    let backing = allocator.allocate(Request::new(8, 8).unwrap()).unwrap();
    allocator
        .bytes_mut(backing)
        .unwrap()
        .copy_from_slice(&41_i64.to_le_bytes());
    let mut values = HostResourceValues::new();
    let mut caller = CanonicalResourceTable::new(AdapterLimits::default());
    let source = values
        .publish_owned(&mut caller, kind("counter"), backing)
        .unwrap();
    let identity = caller.resource_identity(source).unwrap();
    let mut callee = CanonicalResourceTable::new(AdapterLimits::default());
    let before = allocator.stats();
    assert!(
        preflight_call(
            &mut caller,
            &mut callee,
            &[TransferArgument::new(
                source,
                kind("wrong-kind"),
                CanonicalOwnership::Own
            )]
        )
        .is_err()
    );
    assert!(caller.is_live(source));
    assert!(callee.is_empty());
    assert_eq!(allocator.stats(), before);
    let prepared = preflight_call(
        &mut caller,
        &mut callee,
        &[TransferArgument::new(
            source,
            kind("counter"),
            CanonicalOwnership::Own,
        )],
    )
    .unwrap();
    assert_eq!(allocator.stats(), before);
    let call = prepared.commit();
    assert_eq!(allocator.stats(), before);
    let loan = values.borrow_exclusive(identity).unwrap();
    let handle = *values.get_borrowed(loan).unwrap();
    let bytes = allocator.bytes_mut(handle).unwrap();
    let value = i64::from_le_bytes(bytes.try_into().unwrap()) + 1;
    bytes.copy_from_slice(&value.to_le_bytes());
    assert_eq!(value, 42);
    values.end_borrow(loan).unwrap();
    let completion = call.finish(CallOutcome::Returned, |resource| {
        allocator.release(values.release(resource).unwrap())
    });
    assert!(completion.failure().is_none());
    assert!(!caller.is_live(source));
    assert!(callee.is_empty());
    assert!(values.is_empty());
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(allocator.stats().deallocations, 1);
    assert!(allocator.bytes(backing).is_err());
}

#[test]
fn source_tag_typed_import_and_registered_native_kind_share_one_binding() {
    use lang::frontend::component as c;
    use lang_diagnostics::SourceMap;
    use lang_mir::unit as m;
    use lang_runtime::{
        component::{
            bindings as b, grants, idl, native,
            serialization::{DigestAlgorithm, VersionedDigest},
            wire,
        },
        layout,
    };
    use serde_json::json;
    use std::{ffi::c_void, path::Path, ptr};
    extern "C" fn invoke(
        _: *mut c_void,
        _: *const native::Context,
        _: native::Bytes,
        _: *mut *mut native::Buffer,
    ) -> u32 {
        panic!("binding invoked a callback")
    }
    extern "C" fn destroy(
        _: *mut c_void,
        _: *const native::Context,
        _: native::MutBytes,
        _: *mut *mut native::Buffer,
    ) -> u32 {
        panic!("binding destroyed backing")
    }
    let source = "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn relay(value:core::ComponentResource<CounterKind>)->core::ComponentResource<CounterKind>{return value;}";
    let host_json: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/component_counter.idl.json")).unwrap();
    let json = json!({"idl":1,"package":"demo:relay","version":"1.0.0","interfaces":[
        host_json["interfaces"][0].clone(),
        {"key":"demo:relay/api","resources":[],"types":[],"functions":[{"name":"relay","effect":"sync","params":[{"name":"value","type":{"own":"demo:counter/api.counter"}}],"result":{"own":"demo:counter/api.counter"},"capability":"demo.relay.call"}]}
    ],"imports":[{"interface":"demo:counter/api","version":{"exact":"1.0.0"},"members":["apply"]}],"exports":["demo:relay/api"]});
    let bytes = serde_json::to_vec(&json).unwrap();
    let checked = || idl::check(m::SourceId(82), &bytes, Default::default()).unwrap();
    let out = c::analyze(c::Request {
        sources: SourceMap::new(),
        display: Path::new("component-resource-relay.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: checked(),
        bindings: c::Input {
            version: 1,
            resources: vec![c::Resource {
                tag: "CounterKind".into(),
                resource: kind("counter"),
            }],
            imports: vec![c::Import {
                source: "host_apply".into(),
                member: b::MemberKey {
                    interface: InterfaceKey::new(PackageKey::new("demo", "counter"), "api"),
                    member: "apply".into(),
                },
            }],
            exports: vec![c::Export {
                source: "relay".into(),
                member: b::MemberKey {
                    interface: InterfaceKey::new(PackageKey::new("demo", "relay"), "api"),
                    member: "relay".into(),
                },
            }],
        },
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let unit = out.result.unwrap();
    let mut s = wire::Session::new(Default::default());
    let consumer = wire::Contract::new(checked(), vec![], &mut s).unwrap();
    let host = wire::Contract::new(
        idl::check(
            m::SourceId(71),
            include_bytes!("fixtures/component_counter.idl.json"),
            Default::default(),
        )
        .unwrap(),
        vec![],
        &mut s,
    )
    .unwrap();
    let digest = VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap();
    let methods = [native::Method {
        interface: 0,
        member: 0,
        invoke,
        user: ptr::null_mut(),
        flags: 0,
    }];
    let resources = [native::ResourceType {
        interface: 0,
        resource: 0,
        destroy,
        user: ptr::null_mut(),
    }];
    let owner = 42;
    let providers = [native::Provider::with_resources(
        wire::Provider {
            contract: &host,
            digest: &digest,
        },
        &methods,
        &resources,
        &owner,
        &mut s,
    )
    .unwrap()];
    let member = &unit.mappings().imports[0].member;
    let signature = wire::Signature::new(&consumer, &member.interface, &member.member, &mut s)
        .unwrap()
        .preimage(&mut s)
        .unwrap();
    let grant = grants::Grant {
        member,
        capability: "demo.counter.apply",
        signature: &signature,
    };
    let bound = grants::prepare(&unit, &consumer, &[grant], || &providers, &mut s).unwrap();
    assert_eq!(bound.imports().len(), 1);
    assert!(ptr::eq(bound.imports()[0].provider(), &providers[0]));
    let selected = bound
        .resource_kind(m::ImportId(0), m::DefinitionId(0), &mut s)
        .unwrap();
    assert_eq!(selected.key(), &kind("counter"));
    assert!(selected.provider().retains_code(&owner));
    assert!(matches!(
        bound.resource_kind(m::ImportId(0), m::DefinitionId(1), &mut s),
        Err(grants::Error::Resource {
            kind: grants::ResourceErrorKind::UnknownTag,
            ..
        })
    ));
    let layouts = layout::Layouts::compile_component(&unit, Default::default()).unwrap();
    let d = &unit.code().declarations().functions()[0];
    let ty = d.locals[d.parameters[0].0 as usize].ty.ty;
    assert_eq!(layouts.get(ty).unwrap().size, 32);
    assert!(matches!(
        layouts.get(ty).unwrap().kind,
        layout::Kind::Token(_)
    ));
    assert!(matches!(
        layout::Layouts::compile(unit.code().declarations().types(), Default::default()),
        Err(layout::Error::UnsupportedComponentProfile { .. })
    ));
}

fn root_argument_program() -> Box<lang_semantics::analysis::TaskProgram> {
    let source = "pub async fn entry(value:i64,flag:bool)->i64{if flag{return value + 1;}else{return value + 2;}}";
    let out = lang::frontend::analyze_virtual(
        lang_diagnostics::SourceMap::new(),
        std::path::Path::new("component-root-arguments.lang"),
        source.as_bytes(),
        None,
        Default::default(),
        None,
    );
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let lang_semantics::analysis::Program::Tasks(program) = out.result.unwrap() else {
        panic!("task proof")
    };
    program
}

#[test]
fn root_argument_transport_executes_source_branches_on_the_existing_vm_and_native_engines() {
    use lang_mir::unit as m;
    use lang_runtime::{execution as e, task::runner::EntryArgument as A};
    let program = root_argument_program();
    let native = lang_codegen_clif::tasks::CompiledProgram::compile_function_with_arguments(
        program.verified(),
        m::FunctionId(0),
        Default::default(),
    )
    .unwrap();
    assert!(
        lang_codegen_clif::tasks::CompiledProgram::compile_function(
            program.verified(),
            m::FunctionId(0),
            Default::default()
        )
        .is_err()
    );
    for (flag, expected) in [(true, 42), (false, 43)] {
        let arguments = [A::I64(41), A::Bool(flag)];
        let mut previous = None;
        for compiled in [false, true] {
            let mut allocator = Allocator::new(System, Default::default()).unwrap();
            let control = e::Control::with_limits(Default::default());
            let outcome = if compiled {
                native.execute_with_arguments(
                    &mut allocator,
                    control,
                    Default::default(),
                    &arguments,
                )
            } else {
                lang_vm::unit::tasks::execute_function_with_arguments(
                    program.verified(),
                    m::FunctionId(0),
                    &mut allocator,
                    control,
                    Default::default(),
                    &arguments,
                )
            }
            .unwrap();
            assert_eq!(outcome.value, expected);
            assert_eq!(outcome.tasks.live, 0);
            assert_eq!(allocator.stats().live_allocations, 0);
            if let Some(usage) = previous {
                assert_eq!(outcome.usage, usage);
            }
            previous = Some(outcome.usage);
        }
    }
}

#[test]
fn root_argument_arity_and_type_refusal_precede_allocator_effects() {
    use lang_mir::unit as m;
    use lang_runtime::{
        execution as e,
        task::runner::{EntryArgument as A, Reason},
    };
    let program = root_argument_program();
    let native = lang_codegen_clif::tasks::CompiledProgram::compile_function_with_arguments(
        program.verified(),
        m::FunctionId(0),
        Default::default(),
    )
    .unwrap();
    for arguments in [
        &[][..],
        &[A::I64(41)][..],
        &[A::I64(41), A::I64(1)][..],
        &[A::Bool(true), A::Bool(true)][..],
    ] {
        for compiled in [false, true] {
            let mut allocator = Allocator::new(System, Default::default()).unwrap();
            let control = e::Control::with_limits(Default::default());
            let error = if compiled {
                native.execute_with_arguments(
                    &mut allocator,
                    control,
                    Default::default(),
                    arguments,
                )
            } else {
                lang_vm::unit::tasks::execute_function_with_arguments(
                    program.verified(),
                    m::FunctionId(0),
                    &mut allocator,
                    control,
                    Default::default(),
                    arguments,
                )
            }
            .err()
            .unwrap();
            assert!(matches!(error.reason, Reason::Entry));
            assert!(error.span.is_some());
            assert_eq!(allocator.stats().requests, 0);
        }
    }
}
