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
    task::runner::{self as r, EntryArgument},
};
use serde_json::json;
use std::{
    cell::Cell,
    ffi::c_void,
    path::Path,
    ptr,
    rc::{Rc, Weak},
};

struct Owner {
    destroyed: Rc<Cell<usize>>,
    produced: Rc<Cell<usize>>,
    released: Rc<Cell<usize>>,
    allocator: u64,
    backing: Cell<usize>,
    fail: bool,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.released.set(self.released.get() + 1);
    }
}
struct Watch {
    destroyed: Rc<Cell<usize>>,
    produced: Rc<Cell<usize>>,
    released: Rc<Cell<usize>>,
    owner: Weak<Owner>,
}
extern "C" fn unused(
    _: *mut c_void,
    _: *const native::Context,
    _: native::Bytes,
    _: *mut *mut native::Buffer,
) -> u32 {
    panic!("unused import executed")
}

unsafe extern "C" fn destroy(
    user: *mut c_void,
    context: *const native::Context,
    backing: native::MutBytes,
    _: *mut *mut native::Buffer,
) -> u32 {
    // SAFETY: registration retains user through return; the invocation supplies
    // its live original context and the unique detached original backing.
    let owner = unsafe { &*user.cast::<Owner>() };
    assert_eq!(backing.data as usize, owner.backing.get());
    assert_eq!(
        unsafe { lang_codegen_clif::component::context_check(context) },
        0
    );
    assert_eq!(
        unsafe { lang_codegen_clif::component::context_allocator_identity(context) },
        owner.allocator
    );
    let bytes = unsafe { std::slice::from_raw_parts(backing.data, backing.length as usize) };
    assert_eq!(bytes, &41_i64.to_le_bytes());
    owner.destroyed.set(owner.destroyed.get() + 1);
    u32::from(owner.fail)
}
unsafe fn buffer(
    context: *const native::Context,
    length: u64,
    alignment: u64,
) -> Result<(*mut native::Buffer, native::MutBytes), u32> {
    let mut handle = ptr::null_mut();
    // SAFETY: called only within the checked native callback; both output
    // structures are writable and the returned view stays inside this scope.
    let status = unsafe {
        lang_codegen_clif::component::context_buffer_create(context, length, alignment, &mut handle)
    };
    if status != 0 {
        return Err(status);
    }
    let mut view = native::MutBytes {
        data: ptr::null_mut(),
        length: 0,
    };
    let status =
        unsafe { lang_codegen_clif::component::buffer_mut_bytes(context, handle, &mut view) };
    if status != 0 {
        return Err(status);
    }
    if view.length != length || view.data.is_null() {
        return Err(2);
    }
    Ok((handle, view))
}
unsafe extern "C" fn make(
    user: *mut c_void,
    context: *const native::Context,
    arguments: native::Bytes,
    output: *mut *mut native::Buffer,
) -> u32 {
    let result = (|| -> Result<(), u32> {
        if output.is_null() || arguments.length != 45 || arguments.data.is_null() {
            return Err(2);
        }
        // SAFETY: the owning registration and live adapter retain user, context
        // and all canonical parameter bytes through this callback.
        let owner = unsafe { &*user.cast::<Owner>() };
        assert_eq!(
            unsafe { lang_codegen_clif::component::context_check(context) },
            0
        );
        let parameters = unsafe { std::slice::from_raw_parts(arguments.data, 45) };
        let (backing, view) = unsafe { buffer(context, 8, 8) }?;
        unsafe { std::slice::from_raw_parts_mut(view.data, 8) }
            .copy_from_slice(&41_i64.to_le_bytes());
        owner.backing.set(view.data as usize);
        let mut index = 0;
        let key = b"demo:counter/api.counter";
        let status = unsafe {
            lang_codegen_clif::component::context_resource_publish(
                context,
                native::Bytes {
                    data: key.as_ptr(),
                    length: key.len() as u64,
                },
                backing,
                &mut index,
            )
        };
        if status != 0 {
            return Err(status);
        }
        owner.produced.set(owner.produced.get() + 1);
        let (result, view) = unsafe { buffer(context, 49, 1) }?;
        // LCVL v1: same full signature, result tag, one contextual resource
        // index. The runtime independently checks this complete returned frame.
        let frame = unsafe { std::slice::from_raw_parts_mut(view.data, 49) };
        frame[..45].copy_from_slice(parameters);
        frame[8] = 1;
        frame[41..45].copy_from_slice(&4_u32.to_le_bytes());
        frame[45..].copy_from_slice(&index.to_le_bytes());
        unsafe {
            *output = result;
        }
        Ok(())
    })();
    result.map_or_else(|status| status, |()| 0)
}
fn host_document() -> serde_json::Value {
    let mut doc: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/component_counter.idl.json")).unwrap();
    doc["interfaces"][0]["functions"].as_array_mut().unwrap().push(json!({
        "name":"make","effect":"sync","params":[],"result":{"own":"counter"},"capability":"demo.counter.make"
    }));
    doc
}
fn provider(owner: Rc<Owner>) -> native::Provider<'static> {
    let bytes = serde_json::to_vec(&host_document()).unwrap();
    let doc = idl::check(m::SourceId(71), &bytes, Default::default()).unwrap();
    let mut session = wire::Session::new(Default::default());
    let contract = wire::Contract::new(doc, vec![], &mut session).unwrap();
    native::Provider::new_owned(
        contract,
        VersionedDigest::new(DigestAlgorithm::Sha256, vec![1; 32]).unwrap(),
        vec![
            native::Method {
                interface: 0,
                member: 0,
                invoke: unused,
                user: ptr::null_mut(),
                flags: 0,
            },
            native::Method {
                interface: 0,
                member: 1,
                invoke: make,
                user: Rc::as_ptr(&owner).cast_mut().cast(),
                flags: 0,
            },
        ],
        vec![native::ResourceType {
            interface: 0,
            resource: 0,
            destroy,
            user: Rc::as_ptr(&owner).cast_mut().cast(),
        }],
        &mut || owner.clone(),
        &mut session,
    )
    .unwrap()
}
fn stored_unit() -> bindings::Unit {
    let host = host_document();
    let bytes = serde_json::to_vec(&json!({
        "idl":1,"package":"demo:scope","version":"1.0.0",
        "interfaces":[host["interfaces"][0].clone(),
            {"key":"demo:scope/api","resources":[],"types":[],"functions":[
                {"name":"relay","effect":"sync","params":[{"name":"value","type":{"own":"demo:counter/api.counter"}}],
                    "result":{"own":"demo:counter/api.counter"},"capability":"demo.scope.relay"},
                {"name":"consume","effect":"sync","params":[{"name":"value","type":{"own":"demo:counter/api.counter"}}],
                    "result":"i64","capability":"demo.scope.consume"},
                {"name":"create","effect":"sync","params":[],"result":{"own":"demo:counter/api.counter"},"capability":"demo.scope.create"}
            ]}],
        "imports":[{"interface":"demo:counter/api","version":{"exact":"1.0.0"},"members":["apply","make"]}],
        "exports":["demo:scope/api"]
    })).unwrap();
    let host = InterfaceKey::new(PackageKey::new("demo", "counter"), "api");
    let script = InterfaceKey::new(PackageKey::new("demo", "scope"), "api");
    let out = f::analyze(f::Request {
        sources: SourceMap::new(),
        display: Path::new("component-scoped-roots.lang"),
        bytes: include_bytes!("fixtures/component_scoped_roots.lang"),
        root: None,
        compile: Default::default(),
        document: idl::check(m::SourceId(82), &bytes, Default::default()).unwrap(),
        bindings: f::Input {
            version: 1,
            resources: vec![f::Resource {
                tag: "CounterKind".into(),
                resource: ResourceTypeKey::new(host.clone(), "counter"),
            }],
            imports: [("host_apply", "apply"), ("host_make", "make")]
                .into_iter()
                .map(|(source, member)| f::Import {
                    source: source.into(),
                    member: bindings::MemberKey {
                        interface: host.clone(),
                        member: member.into(),
                    },
                })
                .collect(),
            exports: ["relay", "consume", "create"]
                .into_iter()
                .map(|name| f::Export {
                    source: name.into(),
                    member: bindings::MemberKey {
                        interface: script.clone(),
                        member: name.into(),
                    },
                })
                .collect(),
        },
        limits: Default::default(),
    });
    assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
    let artifact = lang::artifact::component::encode(
        out.result.unwrap(),
        m::Source {
            path: "component-scoped-roots.idl.json".into(),
            text: String::from_utf8(bytes).unwrap(),
        },
        None,
        Default::default(),
    )
    .unwrap();
    lang::artifact::component::decode(&artifact, Default::default())
        .unwrap()
        .into_unit()
}

fn prepared<T>(
    provider: native::Provider<'_>,
    run: impl FnOnce(&program::Program<'_>, &mut wire::Session) -> T,
) -> T {
    let unit = stored_unit();
    let mut session = wire::Session::new(Default::default());
    let contract = wire::artifact::source_contract(&unit, &mut session, 65_536).unwrap();
    let signatures: Vec<_> = unit
        .mappings()
        .imports
        .iter()
        .map(|map| {
            wire::Signature::new(
                &contract,
                &map.member.interface,
                &map.member.member,
                &mut session,
            )
            .unwrap()
            .preimage(&mut session)
            .unwrap()
        })
        .collect();
    let grants: Vec<_> = unit
        .mappings()
        .imports
        .iter()
        .zip(&signatures)
        .map(|(map, sig)| grants::Grant {
            member: &map.member,
            capability: if map.member.member == "apply" {
                "demo.counter.apply"
            } else {
                "demo.counter.make"
            },
            signature: sig,
        })
        .collect();
    let providers = [provider];
    let bound = grants::prepare(&unit, &contract, &grants, || &providers, &mut session).unwrap();
    let program = program::Program::prepare(&bound, Default::default()).unwrap();
    run(&program, &mut session)
}
fn entry(program: &program::Program<'_>, name: &str) -> m::FunctionId {
    let unit = program.bindings().unit();
    unit.exports()
        .iter()
        .find_map(|(id, f)| {
            (unit.document().document().interfaces[f.interface].functions[f.member]
                .name
                .value
                == name)
                .then_some(*id)
        })
        .unwrap()
}
struct Harness {
    allocator: a::Allocator<a::System>,
    values: storage::Resources<'static>,
    state: execution::InstanceState,
    engine: lang_codegen_clif::component::Engine,
}
impl Harness {
    fn new() -> Self {
        let allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut budget = m::Budget::new(Default::default());
        let mut values = storage::Resources::prepare(&allocator, 4, 4, &mut budget).unwrap();
        let state = execution::InstanceState::prepare(&mut values).unwrap();
        let engine = lang_codegen_clif::component::Engine::prepare(&mut budget).unwrap();
        Self {
            allocator,
            values,
            state,
            engine,
        }
    }
    fn registration(&self, fail: bool) -> (native::Provider<'static>, Watch) {
        let destroyed = Rc::new(Cell::new(0));
        let produced = Rc::new(Cell::new(0));
        let released = Rc::new(Cell::new(0));
        let owner = Rc::new(Owner {
            destroyed: destroyed.clone(),
            produced: produced.clone(),
            released: released.clone(),
            allocator: self.allocator.identity().get(),
            backing: Cell::new(0),
            fail,
        });
        let watch = Watch {
            destroyed,
            produced,
            released,
            owner: Rc::downgrade(&owner),
        };
        (provider(owner), watch)
    }
    fn publish(&mut self, provider: &native::Provider<'static>, watch: &Watch) -> storage::Claim {
        let key = ResourceTypeKey::new(
            InterfaceKey::new(PackageKey::new("demo", "counter"), "api"),
            "counter",
        );
        let kind = provider
            .resource_kind(&key, &mut wire::Session::new(Default::default()))
            .unwrap()
            .into_owned()
            .ok()
            .unwrap();
        let claim = self
            .values
            .publish(
                kind.clone(),
                self.state.holder(),
                &41_i64.to_le_bytes(),
                &mut self.allocator,
            )
            .unwrap();
        let address = self
            .values
            .bytes(claim, self.state.holder(), kind, &self.allocator)
            .unwrap()
            .as_ptr() as usize;
        watch.owner.upgrade().unwrap().backing.set(address);
        claim
    }
    fn call(
        &mut self,
        provider: native::Provider<'static>,
        name: &str,
        input: Option<storage::Claim>,
        compiled: bool,
    ) -> Result<r::Outcome<execution::Value>, Box<r::Failure>> {
        self.call_control(
            provider,
            name,
            input,
            compiled,
            e::Control::with_limits(Default::default()),
        )
    }
    fn call_control(
        &mut self,
        provider: native::Provider<'static>,
        name: &str,
        input: Option<storage::Claim>,
        compiled: bool,
        control: e::Control<'_>,
    ) -> Result<r::Outcome<execution::Value>, Box<r::Failure>> {
        let Self {
            allocator,
            values,
            state,
            engine,
        } = self;
        prepared(provider, |program, session| {
            let function = entry(program, name);
            let generated = compiled.then(|| {
                lang_codegen_clif::tasks::CompiledProgram::compile_component(
                    program,
                    function,
                    Default::default(),
                )
                .unwrap()
            });
            let mut invoker = engine.begin(control).unwrap();
            let mut budget = m::Budget::new(Default::default());
            state
                .with_program(program, |instance| {
                    let mut invocation = execution::Invocation::prepare_owned(
                        instance,
                        function,
                        values,
                        &mut invoker,
                        &mut budget,
                    )
                    .unwrap();
                    invocation.prepare_output(session).unwrap();
                    let arguments: Vec<_> =
                        input.into_iter().map(EntryArgument::Resource).collect();
                    let mut outcome = if let Some(generated) = generated {
                        generated.execute_component(
                            allocator,
                            control,
                            Default::default(),
                            &arguments,
                            &mut invocation,
                        )
                    } else {
                        lang_vm::unit::tasks::execute_component(
                            program,
                            function,
                            allocator,
                            control,
                            Default::default(),
                            &arguments,
                            &mut invocation,
                        )
                    }
                    .map_err(Box::new)?;
                    if let execution::Value::Frame(frame) = &outcome.value {
                        outcome.value = match frame.values().nodes.as_slice() {
                            [wire::Value::Resource(_)] if frame.resources().len() == 1 => {
                                execution::Value::Resource(frame.resources()[0])
                            }
                            [wire::Value::I64(value)] => {
                                execution::Value::Copy(lang_runtime::layout::Immediate::I64(*value))
                            }
                            _ => panic!("unexpected result frame"),
                        };
                    }
                    Ok(outcome)
                })
                .unwrap()
        })
    }
}
fn retained_roundtrip(compiled: bool, fresh: bool) -> (e::Usage, e::Usage) {
    let mut h = Harness::new();
    let (first, watch) = h.registration(false);
    let input = (!fresh).then(|| h.publish(&first, &watch));
    let before = h.allocator.stats();
    let outcome = h
        .call(
            first,
            if fresh { "create" } else { "relay" },
            input,
            compiled,
        )
        .unwrap();
    let first_usage = outcome.usage;
    let execution::Value::Resource(returned) = outcome.value else {
        panic!("owner result");
    };
    let key = ResourceTypeKey::new(
        InterfaceKey::new(PackageKey::new("demo", "counter"), "api"),
        "counter",
    );
    if let Some(input) = input {
        assert!(
            h.values
                .validate(input, h.state.holder(), &key, &h.allocator)
                .is_err()
        );
    }
    h.values
        .validate(returned, h.state.holder(), &key, &h.allocator)
        .unwrap();
    if !fresh {
        assert_eq!(
            h.allocator.stats().live_allocations,
            before.live_allocations
        );
        assert_eq!(h.allocator.stats().live_bytes, before.live_bytes);
    }
    assert!(watch.owner.upgrade().is_some());
    assert_eq!(watch.destroyed.get(), 0);
    assert_eq!(watch.produced.get(), usize::from(fresh));
    assert_eq!(watch.released.get(), 0);
    let (second, other) = h.registration(false);
    let outcome = h.call(second, "consume", Some(returned), compiled).unwrap();
    assert!(matches!(
        outcome.value,
        execution::Value::Copy(lang_runtime::layout::Immediate::I64(42))
    ));
    assert_eq!(watch.destroyed.get(), 1);
    assert_eq!(watch.released.get(), 1);
    assert!(watch.owner.upgrade().is_none());
    assert_eq!(other.destroyed.get(), 0);
    assert_eq!(other.released.get(), 1);
    assert!(h.values.is_empty());
    assert_eq!(h.allocator.stats().live_allocations, 0);
    assert!(!h.state.poisoned());
    (first_usage, outcome.usage)
}
#[test]
fn stored_roots_keep_original_backing_and_code_after_each_program_scope() {
    assert_eq!(
        retained_roundtrip(false, false),
        retained_roundtrip(true, false)
    );
}
#[test]
fn native_publication_uses_owning_kinds_beyond_the_preparing_program_scope() {
    assert_eq!(
        retained_roundtrip(false, true),
        retained_roundtrip(true, true)
    );
}

#[test]
fn cleanup_poison_survives_program_scopes_and_refuses_a_fresh_preparation() {
    for compiled in [false, true] {
        let mut h = Harness::new();
        let (first, watch) = h.registration(true);
        let result = h.call(first, "create", None, compiled).unwrap();
        let execution::Value::Resource(claim) = result.value else {
            panic!("owner result");
        };
        let (second, other) = h.registration(false);
        let failure = h
            .call(second, "consume", Some(claim), compiled)
            .err()
            .expect("cleanup failure");
        assert!(failure.poisoned);
        assert!(failure.span.is_some());
        assert!(failure.report.as_ref().unwrap().primary().is_some());
        assert!(h.state.poisoned());
        assert!(h.values.is_empty());
        assert_eq!(h.allocator.stats().live_allocations, 0);
        assert_eq!(watch.destroyed.get(), 1);
        assert_eq!(watch.released.get(), 1);
        assert_eq!(other.destroyed.get(), 0);
        let (next, next_watch) = h.registration(false);
        let before = h.allocator.stats();
        prepared(next, |program, _| {
            let called = Cell::new(false);
            assert!(matches!(
                h.state.with_program(program, |_| called.set(true)),
                Err(r::Reason::Instance(execution::InstanceError::Poisoned)),
            ));
            assert!(!called.get());
        });
        assert_eq!(h.allocator.stats(), before);
        assert_eq!(next_watch.produced.get(), 0);
        assert_eq!(next_watch.destroyed.get(), 0);
    }
}

#[test]
fn foreign_retained_table_is_refused_before_entry_and_keeps_state_ready() {
    let mut h = Harness::new();
    let mut foreign = Harness::new();
    let (registration, watch) = h.registration(false);
    let before = foreign.allocator.stats();
    prepared(registration, |program, _| {
        let control = e::Control::with_limits(Default::default());
        let mut invoker = foreign.engine.begin(control).unwrap();
        let result = h
            .state
            .with_program(program, |instance| {
                execution::Invocation::prepare_owned(
                    instance,
                    entry(program, "relay"),
                    &mut foreign.values,
                    &mut invoker,
                    &mut m::Budget::new(Default::default()),
                )
                .err()
            })
            .unwrap();
        assert!(matches!(
            result,
            Some(r::Reason::Component(storage::Error::ForeignHolder))
        ));
    });
    assert!(!h.state.poisoned());
    assert_eq!(foreign.allocator.stats(), before);
    assert_eq!(watch.produced.get(), 0);
    assert_eq!(watch.destroyed.get(), 0);
    assert_eq!(watch.released.get(), 1);
}

#[test]
fn retained_entry_checks_unused_native_owners_and_preparation_work_before_effects() {
    let mut h = Harness::new();
    let (owned, watch) = h.registration(false);
    let methods = owned.methods().to_vec();
    let kinds = [native::ResourceType {
        interface: 0,
        resource: 0,
        destroy,
        user: watch.owner.as_ptr().cast_mut().cast(),
    }];
    let borrowed = native::Provider::with_resources(
        owned.metadata(),
        &methods,
        &kinds,
        &(),
        &mut wire::Session::new(Default::default()),
    )
    .unwrap();
    let before = h.allocator.stats();
    prepared(borrowed, |program, _| {
        let control = e::Control::with_limits(Default::default());
        let mut invoker = h.engine.begin(control).unwrap();
        let refused = h
            .state
            .with_program(program, |instance| {
                execution::Invocation::prepare_owned(
                    instance,
                    entry(program, "relay"),
                    &mut h.values,
                    &mut invoker,
                    &mut m::Budget::new(Default::default()),
                )
                .err()
            })
            .unwrap();
        assert!(matches!(refused, Some(r::Reason::Entry)));
    });
    assert!(!h.state.poisoned());
    prepared(owned, |program, _| {
        let control = e::Control::with_limits(Default::default());
        let mut invoker = h.engine.begin(control).unwrap();
        let mut budget = m::Budget::new(m::Limits {
            work: 0,
            ..Default::default()
        });
        assert!(
            h.state
                .with_program(program, |instance| {
                    execution::Invocation::prepare_owned(
                        instance,
                        entry(program, "relay"),
                        &mut h.values,
                        &mut invoker,
                        &mut budget,
                    )
                    .is_err()
                })
                .unwrap()
        );
        assert_eq!(budget.work_used(), 0);
    });
    assert!(!h.state.poisoned());
    assert_eq!(h.allocator.stats(), before);
    assert_eq!(watch.produced.get(), 0);
    assert_eq!(watch.destroyed.get(), 0);
    assert_eq!(watch.released.get(), 1);
}

#[test]
fn unwind_restores_the_actual_phase_without_reissuing_holder_authority() {
    for committed in [false, true] {
        let mut h = Harness::new();
        let original = h.state.holder();
        let before = h.allocator.stats();
        let (registration, watch) = h.registration(false);
        prepared(registration, |program, session| {
            let control = e::Control::with_limits(Default::default());
            let mut invoker = h.engine.begin(control).unwrap();
            let mut budget = m::Budget::new(Default::default());
            let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                h.state
                    .with_program(program, |instance| {
                        let mut invocation = execution::Invocation::prepare_owned(
                            instance,
                            entry(program, "create"),
                            &mut h.values,
                            &mut invoker,
                            &mut budget,
                        )
                        .unwrap();
                        invocation.prepare_output(session).unwrap();
                        if committed {
                            execution::Host::prepare_arguments(&mut invocation, &[], &h.allocator)
                                .unwrap();
                            execution::Host::commit_arguments(&mut invocation, &mut h.allocator)
                                .unwrap();
                            assert!(execution::Host::arguments_committed(&invocation));
                        }
                        panic!("scoped root unwind");
                    })
                    .unwrap();
            }));
            assert!(panicked.is_err());
        });
        assert_eq!(h.state.poisoned(), committed);
        assert_eq!(h.state.holder(), original);
        let fresh = execution::InstanceState::prepare(&mut h.values).unwrap();
        assert_ne!(fresh.holder(), original);
        assert_eq!(h.state.poisoned(), committed);
        assert!(h.values.is_empty());
        assert_eq!(h.allocator.stats(), before);
        assert_eq!(watch.produced.get(), 0);
        assert_eq!(watch.destroyed.get(), 0);
        assert_eq!(watch.released.get(), 1);
    }
}

fn finite_create(compiled: bool, limit: u64) -> (bool, e::Usage) {
    let mut h = Harness::new();
    let (registration, watch) = h.registration(false);
    let result = h.call_control(
        registration,
        "create",
        None,
        compiled,
        e::Control::with_limits(Default::default()).and_service_work(Some(limit)),
    );
    let (success, usage) = match result {
        Ok(outcome) => {
            let execution::Value::Resource(claim) = outcome.value else {
                panic!("owner result");
            };
            let (cleanup, _) = h.registration(false);
            h.call(cleanup, "consume", Some(claim), compiled).unwrap();
            (true, outcome.usage)
        }
        Err(failure) => {
            assert!(matches!(
                failure.reason,
                r::Reason::Control(e::Failure::ServiceWorkLimit { .. })
            ));
            assert!(!failure.poisoned);
            assert!(failure.span.is_some());
            (false, failure.usage)
        }
    };
    assert!(!h.state.poisoned());
    assert!(h.values.is_empty());
    assert_eq!(h.allocator.stats().live_allocations, 0);
    assert_eq!(watch.destroyed.get(), watch.produced.get());
    assert_eq!(watch.released.get(), 1);
    (success, usage)
}

#[test]
fn retained_native_results_keep_original_work_limits_and_cleanup_across_scopes() {
    let baseline = finite_create(false, u64::MAX);
    assert!(baseline.0);
    let used = baseline.1.service_work;
    assert!(used > 1);
    assert_eq!(baseline, finite_create(true, u64::MAX));
    for limit in [0, used - 1, used] {
        let vm = finite_create(false, limit);
        let native = finite_create(true, limit);
        assert_eq!(vm, native);
        assert_eq!(vm.0, limit == used);
    }
}

#[test]
fn a_fresh_holder_cannot_consume_another_retained_instances_owner() {
    for compiled in [false, true] {
        let mut h = Harness::new();
        let (first, watch) = h.registration(false);
        let result = h.call(first, "create", None, compiled).unwrap();
        let execution::Value::Resource(claim) = result.value else {
            panic!("owner result");
        };
        let original = h.state.holder();
        let fresh = execution::InstanceState::prepare(&mut h.values).unwrap();
        assert_ne!(fresh.holder(), original);
        let state = std::mem::replace(&mut h.state, fresh);
        let (foreign, other) = h.registration(false);
        let before = h.allocator.stats();
        let failure = h
            .call(foreign, "consume", Some(claim), compiled)
            .err()
            .expect("holder refusal");
        assert!(matches!(
            failure.reason,
            r::Reason::Component(storage::Error::WrongHolder)
        ));
        assert_eq!(
            h.allocator.stats().live_allocations,
            before.live_allocations
        );
        assert_eq!(h.allocator.stats().live_bytes, before.live_bytes);
        assert_eq!(watch.destroyed.get(), 0);
        assert_eq!(other.destroyed.get(), 0);
        h.state = state;
        assert_eq!(h.state.holder(), original);
        assert!(!h.state.poisoned());
        let (cleanup, _) = h.registration(false);
        h.call(cleanup, "consume", Some(claim), compiled).unwrap();
        assert_eq!(watch.destroyed.get(), 1);
        assert_eq!(watch.released.get(), 1);
        assert!(h.values.is_empty());
        assert_eq!(h.allocator.stats().live_allocations, 0);
    }
}
