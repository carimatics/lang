use lang::frontend::component as f;
use lang_diagnostics::SourceMap;
use lang_mir::unit as m;
use lang_runtime::{
    allocation as a,
    component::{
        bindings, execution, grants, idl, native,
        negotiation::{InterfaceKey, PackageKey, ResourceTypeKey},
        program,
        serialization::{DigestAlgorithm, VersionedDigest},
        storage, wire,
    },
    execution as e,
    task::runner::EntryArgument,
};
use serde_json::json;
use std::{cell::Cell, ffi::c_void, path::Path, ptr, rc::Rc};

struct Destroyed {
    calls: Cell<usize>,
    value: Cell<i64>,
    allocator: Cell<u64>,
    context: Cell<Option<native::Context>>,
    fail: bool,
    cancelled: bool,
}
#[derive(Clone, Copy)]
enum Case {
    Consume,
    Return,
    Fuel(u64),
    FuelCleanup(u64),
    Oom(usize),
    Cancelled,
    ForeignControl,
    DestructorFailure,
    ReturnCleanupFailure,
}
impl Case {
    fn returns_resource(self) -> bool {
        matches!(self, Self::Return | Self::ReturnCleanupFailure)
    }
    fn failing_cleanup(self) -> bool {
        matches!(
            self,
            Self::DestructorFailure | Self::ReturnCleanupFailure | Self::FuelCleanup(_)
        )
    }
}
struct Failing {
    attempts: Rc<Cell<usize>>,
    fail_at: Rc<Cell<Option<usize>>>,
}
impl a::Provider for Failing {
    type Region = a::Storage;
    fn allocate(&mut self, request: a::Request) -> Option<Self::Region> {
        let attempt = self.attempts.get();
        self.attempts.set(attempt + 1);
        if self.fail_at.get() == Some(attempt) {
            None
        } else {
            a::Storage::try_new(request).ok()
        }
    }
    fn deallocate(&mut self, region: Self::Region, _: a::Request) {
        drop(region);
    }
}
extern "C" fn unused(
    _: *mut c_void,
    _: *const native::Context,
    _: native::Bytes,
    _: *mut *mut native::Buffer,
) -> u32 {
    2
}
unsafe extern "C" fn destroy(
    user: *mut c_void,
    context: *const native::Context,
    backing: native::MutBytes,
    _: *mut *mut native::Buffer,
) -> u32 {
    // SAFETY: registration retains this actual user object and engine scope;
    // the runtime supplies the unique live detached eight-byte backing region.
    let state = unsafe { &*user.cast::<Destroyed>() };
    if unsafe { lang_codegen_clif::component::context_check(context) } != u32::from(state.cancelled)
    {
        return 1;
    }
    if unsafe { lang_codegen_clif::component::context_allocator_identity(context) }
        != state.allocator.get()
    {
        return 1;
    }
    if backing.length != 8 || backing.data.is_null() {
        return 1;
    }
    let bytes = unsafe { std::slice::from_raw_parts(backing.data, 8) };
    state.calls.set(state.calls.get() + 1);
    state
        .value
        .set(i64::from_le_bytes(bytes.try_into().unwrap()));
    state.context.set(Some(unsafe { *context }));
    u32::from(state.fail)
}

#[test]
fn source_resource_argument_executes_and_retires_real_native_backing_on_vm_and_native() {
    run_source(Case::Consume);
}

#[test]
fn source_resource_result_moves_original_backing_to_caller_on_vm_and_native() {
    run_source(Case::Return);
}

#[test]
fn source_resource_fuel_prefixes_retire_once_after_commit_on_both_backends() {
    let results: Vec<_> = (0..16).map(|fuel| run_source(Case::Fuel(fuel))).collect();
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}
#[test]
fn source_resource_provider_refusals_preserve_or_retire_original_owner_on_both_backends() {
    let results: Vec<_> = (0..12)
        .map(|offset| run_source(Case::Oom(offset)))
        .collect();
    assert!(results.contains(&(false, false)));
    assert!(results.contains(&(false, true)));
    assert!(results.contains(&(true, true)));
}
#[test]
fn source_resource_cancellation_before_commit_preserves_the_caller_owner() {
    run_source(Case::Cancelled);
}
#[test]
fn source_resource_foreign_control_is_refused_before_argument_commit() {
    assert_eq!(run_source(Case::ForeignControl), (false, false));
}
#[test]
fn source_resource_destructor_failure_poisoning_is_reported_on_both_backends() {
    run_source(Case::DestructorFailure);
}
#[test]
fn source_resource_result_is_not_published_after_callee_cleanup_failure() {
    run_source(Case::ReturnCleanupFailure);
}
#[test]
fn source_resource_cleanup_failure_preserves_the_original_fuel_failure() {
    run_source(Case::FuelCleanup(0));
}

fn run_source(case: Case) -> (bool, bool) {
    let return_resource = case.returns_resource();
    let extra = matches!(case, Case::ReturnCleanupFailure);
    let host_json: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/component_counter.idl.json")).unwrap();
    let consumer = json!({"idl":1,"package":"demo:consumer","version":"1.0.0","interfaces":[host_json["interfaces"][0].clone(),
        {"key":"demo:consumer/api","resources":[],"types":[],"functions":[{"name":"consume","effect":"sync","params":[{"name":"value","type":{"own":"demo:counter/api.counter"}},{"name":"number","type":"i64"}],"result":if return_resource{json!({"own":"demo:counter/api.counter"})}else{json!("i64")},"capability":"demo.consumer.consume"}]}
    ],"imports":[{"interface":"demo:counter/api","version":{"exact":"1.0.0"},"members":["apply"]}],"exports":["demo:consumer/api"]});
    let mut consumer = consumer;
    if extra {
        consumer["interfaces"][1]["functions"][0]["params"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"extra","type":{"own":"demo:counter/api.counter"}}));
    }
    let bytes = serde_json::to_vec(&consumer).unwrap();
    let checked = || idl::check(m::SourceId(82), &bytes, Default::default()).unwrap();
    let source = if extra {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(value:core::ComponentResource<CounterKind>,number:i64,extra:core::ComponentResource<CounterKind>)->core::ComponentResource<CounterKind>{return value;}"
    } else if return_resource {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(value:core::ComponentResource<CounterKind>,number:i64)->core::ComponentResource<CounterKind>{return value;}"
    } else if matches!(case, Case::Oom(_)) {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(value:core::ComponentResource<CounterKind>,number:i64)->i64{return add(number);}fn add(number:i64)->i64{return number+1;}"
    } else {
        "pub struct CounterKind{}pub struct Request{delta:i64,label:core::Text}extern \"component\" fn host_apply(state:&mut core::ComponentResource<CounterKind>,request:Request)->core::Result<i64,core::Text>;pub fn consume(value:core::ComponentResource<CounterKind>,number:i64)->i64{return number+1;}"
    };
    let interface = InterfaceKey::new(PackageKey::new("demo", "counter"), "api");
    let out = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-consume.lang"),
        bytes: source.as_bytes(),
        root: None,
        compile: Default::default(),
        document: checked(),
        bindings: f::Input {
            version: 1,
            resources: vec![f::Resource {
                tag: "CounterKind".into(),
                resource: ResourceTypeKey::new(interface.clone(), "counter"),
            }],
            imports: vec![f::Import {
                source: "host_apply".into(),
                member: bindings::MemberKey {
                    interface,
                    member: "apply".into(),
                },
            }],
            exports: vec![f::Export {
                source: "consume".into(),
                member: bindings::MemberKey {
                    interface: InterfaceKey::new(PackageKey::new("demo", "consumer"), "api"),
                    member: "consume".into(),
                },
            }],
        },
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let unit = out.result.unwrap();
    let mut session = wire::Session::new(Default::default());
    let consumer = wire::Contract::new(checked(), vec![], &mut session).unwrap();
    let host = wire::Contract::new(
        idl::check(
            m::SourceId(71),
            include_bytes!("fixtures/component_counter.idl.json"),
            Default::default(),
        )
        .unwrap(),
        vec![],
        &mut session,
    )
    .unwrap();
    let digest = VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap();
    let destroyed = Destroyed {
        calls: Cell::new(0),
        value: Cell::new(0),
        allocator: Cell::new(0),
        context: Cell::new(None),
        fail: case.failing_cleanup(),
        cancelled: matches!(case, Case::Cancelled),
    };
    let methods = [native::Method {
        interface: 0,
        member: 0,
        invoke: unused,
        user: ptr::null_mut(),
        flags: 0,
    }];
    let resources = [native::ResourceType {
        interface: 0,
        resource: 0,
        destroy,
        user: ptr::from_ref(&destroyed).cast_mut().cast(),
    }];
    let providers = [native::Provider::with_resources(
        wire::Provider {
            contract: &host,
            digest: &digest,
        },
        &methods,
        &resources,
        &destroyed,
        &mut session,
    )
    .unwrap()];
    let member = &unit.mappings().imports[0].member;
    let signature =
        wire::Signature::new(&consumer, &member.interface, &member.member, &mut session)
            .unwrap()
            .preimage(&mut session)
            .unwrap();
    let grant = grants::Grant {
        member,
        capability: "demo.counter.apply",
        signature: &signature,
    };
    let bindings =
        grants::prepare(&unit, &consumer, &[grant], || &providers, &mut session).unwrap();
    let program = program::Program::prepare(&bindings, Default::default()).unwrap();
    let native = lang_codegen_clif::tasks::CompiledProgram::compile_component(
        &program,
        m::FunctionId(0),
        Default::default(),
    )
    .unwrap();
    let kind = bindings
        .resource_kind(m::ImportId(0), m::DefinitionId(0), &mut session)
        .unwrap();
    let mut previous = None;
    for compiled in [false, true] {
        destroyed.calls.set(0);
        let attempts = Rc::new(Cell::new(0));
        let fail_at = Rc::new(Cell::new(None));
        let mut allocator = a::Allocator::new(
            Failing {
                attempts: attempts.clone(),
                fail_at: fail_at.clone(),
            },
            Default::default(),
        )
        .unwrap();
        destroyed.allocator.set(allocator.identity().get());
        let mut budget = m::Budget::new(Default::default());
        let mut values = storage::Resources::prepare(&allocator, 4, 4, &mut budget).unwrap();
        let holder = values.host();
        let input = values
            .publish(kind, holder, &41_i64.to_le_bytes(), &mut allocator)
            .unwrap();
        let extra_input = extra.then(|| {
            values
                .publish(kind, holder, &41_i64.to_le_bytes(), &mut allocator)
                .unwrap()
        });
        if let Case::Oom(offset) = case {
            fail_at.set(Some(attempts.get() + offset));
        }
        let interruption = e::InterruptionFlag::new();
        if matches!(case, Case::Cancelled) {
            interruption.interrupt();
        }
        let control = e::Control::with_limits(e::Limits {
            fuel: if let Case::Fuel(fuel) | Case::FuelCleanup(fuel) = case {
                Some(fuel)
            } else {
                e::Limits::default().fuel
            },
            ..Default::default()
        })
        .and_interruption(&interruption);
        let mut engine = lang_codegen_clif::component::Engine::prepare(&mut budget).unwrap();
        let bridge_control = if matches!(case, Case::ForeignControl) {
            e::Control::with_limits(e::Limits {
                fuel: Some(1),
                ..Default::default()
            })
            .and_interruption(&interruption)
        } else {
            control
        };
        let mut bridge = engine.begin(bridge_control).unwrap();
        let mut invocation = execution::Invocation::prepare(
            &program,
            m::FunctionId(0),
            &mut values,
            &mut bridge,
            &mut budget,
        )
        .unwrap();
        let mut arguments = vec![EntryArgument::Resource(input), EntryArgument::I64(41)];
        if let Some(extra) = extra_input {
            arguments.push(EntryArgument::Resource(extra));
        }
        let outcome = if compiled {
            native.execute_component(
                &mut allocator,
                control,
                Default::default(),
                &arguments,
                &mut invocation,
            )
        } else {
            lang_vm::unit::tasks::execute_component(
                &program,
                m::FunctionId(0),
                &mut allocator,
                control,
                Default::default(),
                &arguments,
                &mut invocation,
            )
        };
        let committed = execution::Host::arguments_committed(&invocation);
        drop(invocation);
        if let Err(failure) = outcome {
            if case.failing_cleanup() {
                assert!(failure.poisoned);
                assert!(failure.report.as_ref().unwrap().primary().is_some());
            } else {
                assert!(!failure.poisoned);
            }
            if matches!(case, Case::DestructorFailure | Case::ReturnCleanupFailure) {
                assert_eq!(
                    failure.report.as_ref().unwrap().primary().unwrap().kind,
                    lang_runtime::cleanup::FailureKind::Cleanup(19)
                );
            }
            if matches!(case, Case::Fuel(_) | Case::FuelCleanup(_)) {
                assert!(matches!(
                    failure.report.as_ref().unwrap().primary().unwrap().kind,
                    lang_runtime::cleanup::FailureKind::Execution(
                        lang_runtime::cleanup::ExecutionFailure::FuelLimit
                    )
                ));
            }
            if matches!(case, Case::Cancelled) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Control(e::Failure::Interrupted)
                ));
            }
            if matches!(case, Case::ForeignControl) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Entry
                ));
            }
            if matches!(case, Case::Oom(_)) {
                assert!(matches!(
                    failure.reason,
                    lang_runtime::task::runner::Reason::Layout(
                        lang_runtime::layout::Error::Allocation(a::Error::OutOfMemory)
                    )
                ));
            }
            if committed {
                assert!(values.is_empty());
                assert_eq!(destroyed.calls.get(), if extra { 2 } else { 1 });
            } else {
                assert_eq!(destroyed.calls.get(), 0);
                assert_eq!(
                    values.bytes(input, holder, kind, &allocator).unwrap(),
                    41_i64.to_le_bytes()
                );
                let mut backing = values.detach(input, holder, &allocator).unwrap();
                let mut cleanup = lang_runtime::cleanup::Session::prepare(
                    std::num::NonZeroUsize::new(2).unwrap(),
                    &mut budget,
                )
                .unwrap();
                let mut cleaner = lang_runtime::cleanup::Contextual::new(
                    |_: &mut (), _, context: &mut lang_runtime::cleanup::Context<'_>| {
                        execution::Invoker::destroy(
                            &mut bridge,
                            kind,
                            &mut backing,
                            &mut allocator,
                            context,
                        )
                    },
                );
                cleanup
                    .retire(
                        lang_runtime::cleanup::Slot(0),
                        (),
                        None,
                        lang_runtime::cleanup::Reason::ScopeExit,
                        &mut cleaner,
                    )
                    .unwrap();
                backing.release(&mut allocator).unwrap();
            }
            assert_eq!(allocator.stats().live_allocations, 0);
            let evidence = (false, committed, failure.usage);
            if let Some(previous) = previous {
                assert_eq!(evidence, previous);
            }
            previous = Some(evidence);
            continue;
        }
        let outcome = outcome.unwrap_or_else(|_| unreachable!());
        assert!(!case.failing_cleanup());
        assert!(committed);
        if return_resource {
            let execution::Value::Resource(result) = outcome.value else {
                panic!("resource result required")
            };
            assert_ne!(input, result);
            assert_eq!(destroyed.calls.get(), 0);
            assert!(
                values
                    .validate(input, holder, kind.key(), &allocator)
                    .is_err()
            );
            assert_eq!(
                values.bytes(result, holder, kind, &allocator).unwrap(),
                41_i64.to_le_bytes()
            );
            assert_eq!(allocator.stats().live_allocations, 2);
            let mut backing = values.detach(result, holder, &allocator).unwrap();
            let mut cleanup = lang_runtime::cleanup::Session::prepare(
                std::num::NonZeroUsize::new(2).unwrap(),
                &mut budget,
            )
            .unwrap();
            let mut cleaner = lang_runtime::cleanup::Contextual::new(
                |_: &mut (), _, context: &mut lang_runtime::cleanup::Context<'_>| {
                    execution::Invoker::destroy(
                        &mut bridge,
                        kind,
                        &mut backing,
                        &mut allocator,
                        context,
                    )
                },
            );
            cleanup
                .retire(
                    lang_runtime::cleanup::Slot(0),
                    (),
                    None,
                    lang_runtime::cleanup::Reason::ScopeExit,
                    &mut cleaner,
                )
                .unwrap();
            backing.release(&mut allocator).unwrap();
            assert!(!cleanup.is_poisoned());
        } else {
            assert_eq!(
                outcome.value,
                execution::Value::Copy(lang_runtime::layout::Immediate::I64(42))
            );
        }
        assert!(values.is_empty());
        assert_eq!(destroyed.calls.get(), 1);
        assert_eq!(destroyed.value.get(), 41);
        assert_eq!(engine.check(&destroyed.context.get().unwrap()), 5);
        assert_eq!(allocator.stats().live_allocations, 0);
        assert!(!outcome.poisoned);
        let evidence = (true, committed, outcome.usage);
        if let Some(previous) = previous {
            assert_eq!(evidence, previous);
        }
        previous = Some(evidence);
    }
    let (success, committed, _) = previous.unwrap();
    (success, committed)
}
