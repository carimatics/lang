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
    cell::{Cell, RefCell},
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
    expected_engine: Cell<usize>,
    expected_gateway: Cell<usize>,
    fail: bool,
    child: Option<Rc<Child>>,
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
        if owner.expected_engine.get() != 0 {
            assert_eq!(
                unsafe { (*context).engine } as usize,
                owner.expected_engine.get()
            );
            // SAFETY: this actual callback's engine is retained by Harness.
            let gateway =
                unsafe { lang_codegen_clif::component::gateway_owner((*context).engine) }.unwrap();
            assert_eq!(gateway.as_ptr() as usize, owner.expected_gateway.get());
        }
        assert_eq!(
            unsafe { lang_codegen_clif::component::context_check(context) },
            0
        );
        let parameters = unsafe { std::slice::from_raw_parts(arguments.data, 45) };
        let (backing, view) = unsafe { buffer(context, 8, 8) }?;
        let value = if let Some(child) = &owner.child {
            child.parent.set(Some(unsafe { *context }));
            let mut value = 0;
            for _ in 0..child.repetitions {
                value = unsafe { child.run(context) }?;
            }
            value - 1
        } else {
            41
        };
        unsafe { std::slice::from_raw_parts_mut(view.data, 8) }
            .copy_from_slice(&value.to_le_bytes());
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
    // Native host-error status is 1; context-operation status codes are separate.
    result.map_or_else(|_| 1, |()| 0)
}
struct Child {
    unit: bindings::Unit,
    owner: RefCell<Weak<Owner>>,
    state: RefCell<execution::InstanceState>,
    compiled: bool,
    repetitions: usize,
    parent: Cell<Option<native::Context>>,
    nested: Cell<Option<native::Context>>,
    before: Cell<Option<e::Usage>>,
    after: Cell<Option<e::Usage>>,
    value: Cell<i64>,
    budget_address: Cell<Option<usize>>,
    preparation_before: Cell<Option<m::Usage>>,
    wire_preparation: Cell<Option<m::Usage>>,
    wire_attempt: Cell<Option<m::Usage>>,
    wire_work: usize,
    wire_total: Cell<Option<wire::Usage>>,
    reply_work: Cell<Option<usize>>,
    reply_before: Cell<Option<m::Usage>>,
    preparation_after: Cell<Option<m::Usage>>,
    failure: RefCell<Option<String>>,
}
impl Child {
    fn new(
        resources: &mut storage::Resources<'_>,
        compiled: bool,
        repetitions: usize,
        wire_work: usize,
    ) -> Self {
        Self {
            unit: stored_unit(),
            owner: RefCell::new(Weak::new()),
            state: RefCell::new(execution::InstanceState::prepare(resources).unwrap()),
            compiled,
            repetitions,
            parent: Cell::new(None),
            nested: Cell::new(None),
            before: Cell::new(None),
            after: Cell::new(None),
            value: Cell::new(0),
            budget_address: Cell::new(None),
            preparation_before: Cell::new(None),
            wire_preparation: Cell::new(None),
            wire_attempt: Cell::new(None),
            wire_work,
            wire_total: Cell::new(None),
            reply_work: Cell::new(None),
            reply_before: Cell::new(None),
            preparation_after: Cell::new(None),
            failure: RefCell::new(None),
        }
    }
    unsafe fn run(&self, context: *const native::Context) -> Result<i64, u32> {
        let owner = self.owner.borrow().upgrade().ok_or(5u32)?;
        let current = unsafe { *context };
        assert!(matches!(
            unsafe {
                lang_codegen_clif::component::with_child(context, |_, _, _| Err::<(), u32>(2))
            },
            Err(2)
        ));
        assert_eq!(
            unsafe { lang_codegen_clif::component::context_check(context) },
            0
        );
        let mut invalid = current;
        invalid.reserved[0] = 1;
        assert!(matches!(
            unsafe { lang_codegen_clif::component::with_child(&invalid, |_, _, _| Ok(())) },
            Err(2)
        ));
        let engine = current.engine as usize;
        let invocation = current.invocation;
        let scope = current.scope;
        assert_eq!(
            std::thread::spawn(move || {
                let copied = native::Context {
                    header: native::Header {
                        size: size_of::<native::Context>() as u32,
                        version: 1,
                    },
                    engine: engine as *mut native::Engine,
                    invocation,
                    scope,
                    reserved: [0; 2],
                };
                // SAFETY: owner callback waits for this thread, retaining exact
                // context engine bytes; refusal precedes active borrow restoration.
                unsafe { lang_codegen_clif::component::with_child(&copied, |_, _, _| Ok(())) }
            })
            .join()
            .unwrap(),
            Err(4)
        );
        // SAFETY: invoked only by make with its exact live original context.
        let value = unsafe {
            lang_codegen_clif::component::with_child(context, |invoker, call, control| {
                self.before
                    .set(Some(call.accounting().ok_or(4u32)?.snapshot()));
                assert_eq!(lang_codegen_clif::component::context_check(context), 5);
                let mut operation = ChildOperation {
                    target: self,
                    owner,
                    invoker,
                    control,
                    value: None,
                };
                call.with_storage(&mut operation)?;
                operation.value.ok_or(17u32)
            })
        }?;
        assert_eq!(
            unsafe { lang_codegen_clif::component::context_check(context) },
            0
        );
        assert_eq!(
            unsafe { lang_codegen_clif::component::context_check(&self.nested.get().unwrap()) },
            5
        );
        self.value.set(value);
        Ok(value)
    }
}
struct OriginalBudget<'a>(&'a Child);
impl execution::StorageOperation for OriginalBudget<'_> {
    fn run(
        &mut self,
        _: &mut storage::Resources<'_>,
        _: &mut dyn storage::Memory,
        budget: &mut m::Budget,
        _: storage::Holder,
        _: &e::InvocationAccounting,
    ) -> Result<(), u32> {
        assert_eq!(
            self.0.budget_address.get(),
            Some(std::ptr::from_mut(&mut *budget) as usize)
        );
        let before = self.0.preparation_before.get().unwrap();
        let now = preparation_usage(budget);
        assert!(
            now.work > before.work
                && now.allocation_bytes > before.allocation_bytes
                && now.elements > before.elements
        );
        Ok(())
    }
}
/// Reserve the original preparation work allowance around the native i64
/// reply's 53-byte copy, forcing copy/decode refusal at an exact boundary.
struct ReplyWindow<'a> {
    target: &'a Child,
    remaining: usize,
}
impl execution::StorageOperation for ReplyWindow<'_> {
    fn run(
        &mut self,
        _: &mut storage::Resources<'_>,
        _: &mut dyn storage::Memory,
        budget: &mut m::Budget,
        _: storage::Holder,
        _: &e::InvocationAccounting,
    ) -> Result<(), u32> {
        assert_eq!(
            self.target.budget_address.get(),
            Some(std::ptr::from_mut(&mut *budget) as usize)
        );
        let available = budget
            .limits()
            .work
            .checked_sub(budget.work_used())
            .ok_or(6u32)?;
        budget
            .charge(available.checked_sub(self.remaining).ok_or(6u32)?)
            .map_err(|_| 6u32)?;
        self.target
            .reply_before
            .set(Some(preparation_usage(budget)));
        Ok(())
    }
}
struct BudgetInvoker<'a> {
    original: &'a mut dyn execution::Invoker,
    target: &'a Child,
}
impl execution::Invoker for BudgetInvoker<'_> {
    fn reentry_supported(&self) -> bool {
        self.original.reentry_supported()
    }
    fn control(&self) -> e::Control<'_> {
        self.original.control()
    }
    fn accounting(&self) -> Option<&e::InvocationAccounting> {
        self.original.accounting()
    }
    fn control_matches(&self, control: e::Control<'_>) -> bool {
        self.original.control_matches(control)
    }
    fn service_work_limit(&self) -> Option<u64> {
        self.original.service_work_limit()
    }
    fn check_control(&self) -> Result<(), e::Failure> {
        self.original.check_control()
    }
    fn invoke(
        &mut self,
        method: &native::Method,
        parameters: &[u8],
        context: &mut dyn execution::NativeContext,
    ) -> Result<execution::NativeReturn, u32> {
        context.with_storage(&mut OriginalBudget(self.target))?;
        let returned = self.original.invoke(method, parameters, context)?;
        if returned.status == 0
            && let Some(remaining) = self.target.reply_work.get()
        {
            context.with_storage(&mut ReplyWindow {
                target: self.target,
                remaining,
            })?;
        }
        Ok(returned)
    }
    fn destroy(
        &mut self,
        kind: native::ResourceKind<'_>,
        backing: &mut storage::Detached<'_>,
        memory: &mut dyn storage::Memory,
        context: &mut lang_runtime::cleanup::Context<'_>,
    ) -> Result<(), u32> {
        self.original.destroy(kind, backing, memory, context)
    }
}
struct ChildOperation<'a, 'ctrl> {
    target: &'a Child,
    owner: Rc<Owner>,
    invoker: &'a mut dyn execution::Invoker,
    control: e::Control<'ctrl>,
    value: Option<i64>,
}
impl execution::StorageOperation for ChildOperation<'_, '_> {
    fn run(
        &mut self,
        resources: &mut storage::Resources<'_>,
        memory: &mut dyn storage::Memory,
        budget: &mut m::Budget,
        _: storage::Holder,
        accounting: &e::InvocationAccounting,
    ) -> Result<(), u32> {
        assert!(accounting.same_invocation(self.invoker.accounting().unwrap()));
        assert_eq!(memory.identity().get(), self.owner.allocator);
        let mut view = a::Allocator::borrow(memory).map_err(|_| 6u32)?;
        let preparation = budget.limits();
        let address = std::ptr::from_mut(&mut *budget) as usize;
        if let Some(previous) = self.target.budget_address.get() {
            assert_eq!(previous, address);
        }
        self.target.budget_address.set(Some(address));
        if let Some(previous) = self.target.preparation_after.get() {
            let current = preparation_usage(budget);
            assert!(current.work >= previous.work);
            assert!(current.allocation_bytes >= previous.allocation_bytes);
            assert!(current.elements >= previous.elements);
        }
        self.target
            .preparation_before
            .set(Some(preparation_usage(budget)));
        let result = prepared_unit_budget(
            &self.target.unit,
            provider(self.owner.clone()),
            Some(&mut *budget),
            self.target.wire_work,
            |program, session, budget| {
                let budget = budget.ok_or(4u32)?;
                self.target
                    .wire_preparation
                    .set(Some(preparation_usage(budget)));
                let function = entry(program, "answer");
                let generated = self.target.compiled.then(|| {
                    lang_codegen_clif::tasks::CompiledProgram::compile_component(
                        program,
                        function,
                        Default::default(),
                    )
                    .unwrap()
                });
                let mut invoker = BudgetInvoker {
                    original: self.invoker,
                    target: self.target,
                };
                let mut state = self.target.state.borrow_mut();
                state
                    .with_program(program, |instance| {
                        let mut invocation = execution::Invocation::prepare_owned(
                            instance,
                            function,
                            resources,
                            &mut invoker,
                            budget,
                        )
                        .map_err(|_| 1u32)?;
                        invocation.prepare_output(session).map_err(|_| 1u32)?;
                        let outcome = execute_memory_with_budget(
                            program,
                            function,
                            generated.as_ref(),
                            &mut view,
                            CallControl {
                                execution: self.control,
                                preparation,
                            },
                            &[],
                            &mut invocation,
                            Some(budget),
                        )
                        .map_err(|error| {
                            self.target.failure.replace(Some(format!("{error:?}")));
                            1u32
                        })?;
                        self.target
                            .preparation_after
                            .set(Some(preparation_usage(budget)));
                        assert!(!outcome.poisoned);
                        let execution::Value::Frame(frame) = outcome.value else {
                            return Err(17);
                        };
                        let [wire::Value::I64(value)] = frame.values().nodes.as_slice() else {
                            return Err(17);
                        };
                        self.value = Some(*value);
                        self.target.after.set(Some(outcome.usage));
                        Ok(())
                    })
                    .unwrap()
            },
        );
        self.target
            .wire_attempt
            .set(Some(preparation_usage(budget)));
        let (result, wire_usage) = result?;
        self.target.wire_total.set(Some(wire_usage));
        result
    }
}
fn preparation_usage(budget: &m::Budget) -> m::Usage {
    m::Usage {
        work: budget.work_used(),
        allocation_bytes: budget.allocation_bytes(),
        elements: budget.elements_used(),
    }
}
unsafe extern "C" fn tick(
    user: *mut c_void,
    context: *const native::Context,
    input: native::Bytes,
    output: *mut *mut native::Buffer,
) -> u32 {
    let result = (|| -> Result<(), u32> {
        // SAFETY: registration and adapter retain the exact owner/context/views.
        let owner = unsafe { &*user.cast::<Owner>() };
        let child = owner.child.as_ref().ok_or(4u32)?;
        let parent = child.parent.get().ok_or(5u32)?;
        let current = unsafe { *context };
        assert_eq!(current.engine, parent.engine);
        assert_eq!(current.invocation, parent.invocation);
        assert_ne!(current.scope, parent.scope);
        assert_eq!(
            unsafe { lang_codegen_clif::component::context_check(&parent) },
            5
        );
        assert_eq!(
            unsafe { lang_codegen_clif::component::context_check(context) },
            0
        );
        assert_eq!(
            unsafe { lang_codegen_clif::component::context_allocator_identity(context) },
            owner.allocator
        );
        assert!(matches!(
            unsafe { lang_codegen_clif::component::with_child(context, |_, _, _| Ok(())) },
            Err(4)
        ));
        if let Some(previous) = child.nested.get() {
            assert!(current.scope > previous.scope);
        }
        child.nested.set(Some(current));

        if output.is_null() || input.data.is_null() || input.length != 45 {
            return Err(2);
        }
        let parameters = unsafe { std::slice::from_raw_parts(input.data, 45) };
        let (buffer, bytes) = unsafe { buffer(context, 53, 1) }?;
        let frame = unsafe { std::slice::from_raw_parts_mut(bytes.data, 53) };
        frame[..45].copy_from_slice(parameters);
        frame[8] = 1;
        frame[41..45].copy_from_slice(&8_u32.to_le_bytes());
        frame[45..].copy_from_slice(&41_i64.to_le_bytes());
        unsafe { output.write(buffer) };
        Ok(())
    })();
    // Native host-error status is 1; context-operation status codes are separate.
    result.map_or_else(|_| 1, |()| 0)
}

fn host_document() -> serde_json::Value {
    let mut doc: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/component_counter.idl.json")).unwrap();
    doc["interfaces"][0]["functions"].as_array_mut().unwrap().push(json!({
        "name":"make","effect":"sync","params":[],"result":{"own":"counter"},"capability":"demo.counter.make"
    }));
    doc["interfaces"][0]["functions"].as_array_mut().unwrap().push(json!({
        "name":"tick","effect":"sync","params":[],"result":"i64","capability":"demo.counter.tick"
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
                flags: u32::from(owner.child.is_some()),
            },
            native::Method {
                interface: 0,
                member: 2,
                invoke: tick,
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
                {"name":"create","effect":"sync","params":[],"result":{"own":"demo:counter/api.counter"},"capability":"demo.scope.create"},
                {"name":"answer","effect":"sync","params":[],"result":"i64","capability":"demo.scope.answer"}
            ]}],
        "imports":[{"interface":"demo:counter/api","version":{"exact":"1.0.0"},"members":["apply","make","tick"]}],
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
            imports: [
                ("host_apply", "apply"),
                ("host_make", "make"),
                ("host_tick", "tick"),
            ]
            .into_iter()
            .map(|(source, member)| f::Import {
                source: source.into(),
                member: bindings::MemberKey {
                    interface: host.clone(),
                    member: member.into(),
                },
            })
            .collect(),
            exports: ["relay", "consume", "create", "answer"]
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
    prepared_unit(&unit, provider, run)
}
fn prepared_unit<T>(
    unit: &bindings::Unit,
    provider: native::Provider<'_>,
    run: impl FnOnce(&program::Program<'_>, &mut wire::Session) -> T,
) -> T {
    prepared_unit_budget(unit, provider, None, usize::MAX, |program, session, _| {
        run(program, session)
    })
    .unwrap()
    .0
}
fn preparing<T, E: From<wire::Error>>(
    session: &mut wire::Session,
    budget: &mut Option<&mut m::Budget>,
    run: impl FnOnce(&mut wire::Session) -> Result<T, E>,
) -> Result<T, E> {
    if let Some(budget) = budget.as_deref_mut() {
        let mut scope = session.scope_preparation(budget).map_err(E::from)?;
        run(&mut scope)
    } else {
        run(session)
    }
}
fn prepared_unit_budget<T>(
    unit: &bindings::Unit,
    provider: native::Provider<'_>,
    mut budget: Option<&mut m::Budget>,
    wire_work: usize,
    run: impl FnOnce(&program::Program<'_>, &mut wire::Session, Option<&mut m::Budget>) -> T,
) -> Result<(T, wire::Usage), u32> {
    let mut limits = wire::Limits::default();
    if let Some(budget) = budget.as_deref() {
        let original = budget.limits();
        limits.work = original.work.min(wire_work);
        limits.allocation_bytes = original.allocations;
        limits.elements = original.elements;
    }
    let mut session = wire::Session::new(limits);
    let contract = preparing(&mut session, &mut budget, |session| {
        wire::artifact::source_contract(unit, session, 65_536)
    })
    .map_err(|_| 1u32)?;
    let signatures: Vec<_> = unit
        .mappings()
        .imports
        .iter()
        .map(|map| {
            preparing(&mut session, &mut budget, |session| {
                wire::Signature::new(
                    &contract,
                    &map.member.interface,
                    &map.member.member,
                    session,
                )?
                .preimage(session)
            })
        })
        .collect::<Result<_, _>>()
        .map_err(|_| 1u32)?;
    let grants: Vec<_> = unit
        .mappings()
        .imports
        .iter()
        .zip(&signatures)
        .map(|(map, sig)| grants::Grant {
            member: &map.member,
            capability: if map.member.member == "apply" {
                "demo.counter.apply"
            } else if map.member.member == "tick" {
                "demo.counter.tick"
            } else {
                "demo.counter.make"
            },
            signature: sig,
        })
        .collect();
    let providers = [provider];
    let bound = preparing(&mut session, &mut budget, |session| {
        grants::prepare(unit, &contract, &grants, || &providers, session)
    })
    .map_err(|_| 1u32)?;
    let program = program::Program::prepare(&bound, Default::default()).map_err(|_| 1u32)?;
    let result = run(&program, &mut session, budget);
    Ok((result, session.usage()))
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
#[derive(Clone, Copy)]
enum MemoryAccess {
    Original,
    View,
    NestedView,
}

#[allow(
    clippy::too_many_arguments,
    reason = "Keep original optional preparation budget explicit at the fixture's VM/native boundary."
)]
fn execute_memory_with_budget<P: a::Provider>(
    program: &program::Program<'_>,
    function: m::FunctionId,
    generated: Option<&lang_codegen_clif::tasks::CompiledProgram<'_>>,
    allocator: &mut a::Allocator<P>,
    control: CallControl<'_>,
    arguments: &[EntryArgument],
    host: &mut dyn execution::Host,
    budget: Option<&mut m::Budget>,
) -> Result<r::Outcome<execution::Value>, Box<r::Failure>> {
    let limits = r::Limits {
        preparation: control.preparation,
        ..Default::default()
    };
    (if let Some(budget) = budget {
        if let Some(generated) = generated {
            generated.execute_component_with_budget(
                allocator,
                control.execution,
                limits,
                arguments,
                host,
                budget,
            )
        } else {
            lang_vm::unit::tasks::execute_component_with_budget(
                program,
                function,
                allocator,
                control.execution,
                limits,
                arguments,
                host,
                budget,
            )
        }
    } else if let Some(generated) = generated {
        generated.execute_component(allocator, control.execution, limits, arguments, host)
    } else {
        lang_vm::unit::tasks::execute_component(
            program,
            function,
            allocator,
            control.execution,
            limits,
            arguments,
            host,
        )
    })
    .map_err(Box::new)
}

#[derive(Clone, Copy)]
struct CallControl<'a> {
    execution: e::Control<'a>,
    preparation: m::Limits,
}

struct TestProvider {
    calls: Rc<Cell<usize>>,
    failure: Rc<Cell<Option<usize>>>,
}
impl a::Provider for TestProvider {
    type Region = <a::System as a::Provider>::Region;
    fn allocate(&mut self, request: a::Request) -> Option<Self::Region> {
        let index = self.calls.get();
        self.calls.set(index + 1);
        if self.failure.get() == Some(index) {
            None
        } else {
            a::Provider::allocate(&mut a::System, request)
        }
    }
    fn deallocate(&mut self, region: Self::Region, request: a::Request) {
        a::Provider::deallocate(&mut a::System, region, request);
    }
}
struct Harness {
    allocator: a::Allocator<TestProvider>,
    provider_calls: Rc<Cell<usize>>,
    provider_failure: Rc<Cell<Option<usize>>>,
    preparation_trace: Option<Rc<Cell<Option<m::Usage>>>>,
    values: storage::Resources<'static>,
    state: execution::InstanceState,
    engine: lang_codegen_clif::component::Engine,
    memory_access: MemoryAccess,
    preparation_limits: m::Limits,
}
impl Harness {
    fn new() -> Self {
        let provider_calls = Rc::new(Cell::new(0));
        let provider_failure = Rc::new(Cell::new(None));
        let allocator = a::Allocator::new(
            TestProvider {
                calls: provider_calls.clone(),
                failure: provider_failure.clone(),
            },
            Default::default(),
        )
        .unwrap();
        let mut budget = m::Budget::new(Default::default());
        let mut values = storage::Resources::prepare(&allocator, 4, 4, &mut budget).unwrap();
        let state = execution::InstanceState::prepare(&mut values).unwrap();
        let engine = lang_codegen_clif::component::Engine::prepare(&mut budget).unwrap();
        Self {
            allocator,
            provider_calls,
            provider_failure,
            preparation_trace: None,
            values,
            state,
            engine,
            memory_access: MemoryAccess::Original,
            preparation_limits: Default::default(),
        }
    }
    fn registration(&self, fail: bool) -> (native::Provider<'static>, Watch) {
        self.registration_child(fail, None)
    }
    fn registration_child(
        &self,
        fail: bool,
        child: Option<Rc<Child>>,
    ) -> (native::Provider<'static>, Watch) {
        let destroyed = Rc::new(Cell::new(0));
        let produced = Rc::new(Cell::new(0));
        let released = Rc::new(Cell::new(0));
        let owner = Rc::new(Owner {
            destroyed: destroyed.clone(),
            produced: produced.clone(),
            released: released.clone(),
            allocator: self.allocator.identity().get(),
            backing: Cell::new(0),
            expected_engine: Cell::new(0),
            expected_gateway: Cell::new(0),
            fail,
            child: child.clone(),
        });
        if let Some(child) = child {
            child.owner.replace(Rc::downgrade(&owner));
        }
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
        self.call_execution(provider, name, input, compiled, control, None)
    }
    fn call_execution(
        &mut self,
        provider: native::Provider<'static>,
        name: &str,
        input: Option<storage::Claim>,
        compiled: bool,
        control: e::Control<'_>,
        accounting: Option<&e::InvocationAccounting>,
    ) -> Result<r::Outcome<execution::Value>, Box<r::Failure>> {
        let Self {
            allocator,
            values,
            state,
            engine,
            memory_access,
            preparation_limits,
            preparation_trace,
            ..
        } = self;
        let memory_access = *memory_access;
        let preparation_limits = *preparation_limits;
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
            if let Some(accounting) = accounting {
                invoker = invoker.with_accounting(accounting).unwrap();
            }
            let mut budget = m::Budget::new(preparation_limits);
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
                    let mut execution_budget = m::Budget::new(preparation_limits);
                    let borrowed_preparation = preparation_trace.is_some();
                    let result = match memory_access {
                        MemoryAccess::Original => execute_memory_with_budget(
                            program,
                            function,
                            generated.as_ref(),
                            allocator,
                            CallControl {
                                execution: control,
                                preparation: preparation_limits,
                            },
                            &arguments,
                            &mut invocation,
                            if borrowed_preparation {
                                Some(&mut execution_budget)
                            } else {
                                None
                            },
                        ),
                        MemoryAccess::View => {
                            let mut view = a::Allocator::borrow(allocator).unwrap();
                            execute_memory_with_budget(
                                program,
                                function,
                                generated.as_ref(),
                                &mut view,
                                CallControl {
                                    execution: control,
                                    preparation: preparation_limits,
                                },
                                &arguments,
                                &mut invocation,
                                if borrowed_preparation {
                                    Some(&mut execution_budget)
                                } else {
                                    None
                                },
                            )
                        }
                        MemoryAccess::NestedView => {
                            let mut first = a::Allocator::borrow(allocator).unwrap();
                            let mut second = a::Allocator::borrow(&mut first).unwrap();
                            execute_memory_with_budget(
                                program,
                                function,
                                generated.as_ref(),
                                &mut second,
                                CallControl {
                                    execution: control,
                                    preparation: preparation_limits,
                                },
                                &arguments,
                                &mut invocation,
                                if borrowed_preparation {
                                    Some(&mut execution_budget)
                                } else {
                                    None
                                },
                            )
                        }
                    };
                    if let Some(trace) = preparation_trace {
                        trace.set(Some(preparation_usage(&execution_budget)));
                    }
                    let mut outcome = result?;
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

#[test]
fn gateway_identity_matches_actual_callbacks_across_wrapper_moves_without_extending_scopes() {
    for compiled in [false, true] {
        let tag = Box::new(42_u64);
        let mut harness = Harness::new();
        let owner = std::ptr::NonNull::from(&*tag).cast();
        let handle = harness.engine.attach_gateway(owner).unwrap();
        assert_eq!(handle, harness.engine.native_handle());
        assert_eq!(harness.engine.attach_gateway(owner), Err(2));
        let mut harness = Box::new(harness);
        assert_eq!(handle, harness.engine.native_handle());
        // SAFETY: the boxed harness and tag retain both live allocations.
        assert_eq!(
            unsafe { lang_codegen_clif::component::gateway_owner(handle) }.unwrap(),
            owner
        );
        let (provider, watch) = harness.registration(false);
        let native_owner = watch.owner.upgrade().unwrap();
        native_owner.expected_engine.set(handle as usize);
        native_owner.expected_gateway.set(owner.as_ptr() as usize);
        drop(native_owner);
        let result = harness
            .call(provider.clone(), "create", None, compiled)
            .unwrap();
        let execution::Value::Resource(claim) = result.value else {
            panic!("resource result")
        };
        let result = harness
            .call(provider, "consume", Some(claim), compiled)
            .unwrap();
        assert!(matches!(
            result.value,
            execution::Value::Copy(lang_runtime::layout::Immediate::I64(42))
        ));
        assert_eq!(watch.destroyed.get(), 1);
        let expired = harness.engine.last_context().unwrap();
        assert_eq!(harness.engine.check(&expired), 5);
        // SAFETY: engine identity remains live while callback scope expired.
        assert_eq!(
            unsafe { lang_codegen_clif::component::gateway_owner(handle) }.unwrap(),
            owner
        );
        let address = handle as usize;
        std::thread::scope(|scope| {
            scope
                .spawn(move || {
                    // SAFETY: owner thread is idle and retains the live engine
                    // until join. Rejection happens before gateway dereference.
                    assert_eq!(
                        unsafe {
                            lang_codegen_clif::component::gateway_owner(
                                address as *mut native::Engine,
                            )
                        },
                        Err(4)
                    );
                })
                .join()
                .unwrap();
        });
    }
}
fn retained_roundtrip(compiled: bool, fresh: bool) -> (e::Usage, e::Usage) {
    retained_roundtrip_memory(compiled, fresh, MemoryAccess::Original)
}
fn retained_roundtrip_memory(
    compiled: bool,
    fresh: bool,
    access: MemoryAccess,
) -> (e::Usage, e::Usage) {
    let mut h = Harness::new();
    h.memory_access = access;
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
fn scoped_allocator_views_execute_source_and_native_resource_callbacks_without_new_custody() {
    for fresh in [false, true] {
        let expected = retained_roundtrip_memory(false, fresh, MemoryAccess::Original);
        for access in [MemoryAccess::View, MemoryAccess::NestedView] {
            for compiled in [false, true] {
                assert_eq!(retained_roundtrip_memory(compiled, fresh, access), expected);
            }
        }
    }
}

#[test]
fn finite_callback_budget_boundaries_repeats_and_every_provider_refusal_retire_owners() {
    fn run(
        compiled: bool,
        repetitions: usize,
        limits: m::Limits,
        failure: Option<usize>,
    ) -> (bool, m::Usage, usize) {
        let mut h = Harness::new();
        h.preparation_limits = limits;
        h.provider_failure.set(failure);
        let trace = Rc::new(Cell::new(None));
        h.preparation_trace = Some(trace.clone());
        let child = Rc::new(Child::new(&mut h.values, compiled, repetitions, usize::MAX));
        let (registration, watch) = h.registration_child(false, Some(child.clone()));
        let result = h.call_control(
            registration,
            "create",
            None,
            compiled,
            e::Control::with_limits(e::Limits::unbounded()),
        );
        let usage = trace.get().expect("original root preparation ledger");
        let calls = h.provider_calls.get();
        let succeeded = result.is_ok();
        h.provider_failure.set(None);
        h.preparation_trace = None;
        h.preparation_limits = Default::default();
        match result {
            Ok(created) => {
                assert_eq!(child.value.get(), 42);
                let execution::Value::Resource(claim) = created.value else {
                    panic!("original owner")
                };
                let (cleanup, _) = h.registration(false);
                h.call(cleanup, "consume", Some(claim), compiled).unwrap();
            }
            Err(error) => {
                assert!(
                    !error.poisoned,
                    "{error:?}; child={:?}",
                    child.failure.borrow()
                );
            }
        }
        for context in [child.parent.get(), child.nested.get()]
            .into_iter()
            .flatten()
        {
            assert_eq!(h.engine.check(&context), 5);
        }
        assert_eq!(watch.destroyed.get(), watch.produced.get());
        assert_eq!(watch.released.get(), 1);
        assert!(h.values.is_empty());
        assert_eq!(h.allocator.stats().live_allocations, 0);
        assert!(!h.state.poisoned() && !child.state.borrow().poisoned());

        // A failed child must not poison or consume the next original parent call.
        let (next, next_watch) = h.registration(false);
        let next = h.call(next, "create", None, compiled).unwrap();
        let execution::Value::Resource(claim) = next.value else {
            panic!("next owner")
        };
        let (cleanup, _) = h.registration(false);
        h.call(cleanup, "consume", Some(claim), compiled).unwrap();
        assert_eq!(next_watch.produced.get(), 1);
        assert_eq!(next_watch.destroyed.get(), 1);
        assert_eq!(next_watch.released.get(), 1);
        assert!(h.values.is_empty());
        assert_eq!(h.allocator.stats().live_allocations, 0);
        (succeeded, usage, calls)
    }
    let wide = m::Limits::default();
    let mut reference = Vec::new();
    for compiled in [false, true] {
        let mut previous = None;
        for repetitions in [1, 2] {
            let (ok, complete, calls) = run(compiled, repetitions, wide, None);
            assert!(ok, "finite baseline for {repetitions} children");
            if compiled {
                assert_eq!(
                    complete,
                    reference[repetitions - 1],
                    "VM/native preparation costs"
                );
            } else {
                reference.push(complete);
            }
            if let Some(before) = previous {
                let before: m::Usage = before;
                assert!(complete.work > before.work);
                assert!(complete.allocation_bytes > before.allocation_bytes);
                assert!(complete.elements > before.elements);
            }
            for dimension in 0..3 {
                for exact in [false, true] {
                    let mut limits = wide;
                    let ceiling = match dimension {
                        0 => complete.work,
                        1 => complete.allocation_bytes,
                        _ => complete.elements,
                    } - usize::from(!exact);
                    match dimension {
                        0 => limits.work = ceiling,
                        1 => limits.allocations = ceiling,
                        _ => limits.elements = ceiling,
                    }
                    let (ok, accepted, _) = run(compiled, repetitions, limits, None);
                    assert_eq!(ok, exact, "dimension {dimension}, {repetitions} children");
                    assert!(
                        match dimension {
                            0 => accepted.work,
                            1 => accepted.allocation_bytes,
                            _ => accepted.elements,
                        } <= ceiling
                    );
                    if exact {
                        assert_eq!(accepted, complete);
                    }
                }
                if let Some(before) = previous {
                    let mut limits = wide;
                    match dimension {
                        0 => limits.work = before.work,
                        1 => limits.allocations = before.allocation_bytes,
                        _ => limits.elements = before.elements,
                    }
                    assert!(
                        !run(compiled, repetitions, limits, None).0,
                        "repeated children share dimension {dimension}"
                    );
                }
            }
            assert!(calls > 0);
            for failure in 0..calls {
                assert!(
                    !run(compiled, repetitions, wide, Some(failure)).0,
                    "provider refusal {failure}"
                );
            }
            previous = Some(complete);
        }
    }
}

fn original_native_callback_roundtrip(unbounded: bool) {
    for compiled in [false, true] {
        for repetitions in [1, 2] {
            let mut h = Harness::new();
            if unbounded {
                h.preparation_limits.work = usize::MAX;
                h.preparation_limits.allocations = usize::MAX;
                h.preparation_limits.elements = usize::MAX;
            }
            let child = Rc::new(Child::new(&mut h.values, compiled, repetitions, usize::MAX));
            let (registration, watch) = h.registration_child(false, Some(child.clone()));
            let control = e::Control::with_limits(e::Limits::unbounded());
            let created = h
                .call_control(registration, "create", None, compiled, control)
                .unwrap();
            assert_eq!(child.value.get(), 42);
            let before = child.before.get().unwrap();
            let after = child.after.get().unwrap();
            assert_eq!(before.host_calls, repetitions as u64);
            assert_eq!(after.host_calls, repetitions as u64 + 1);
            assert!(after.fuel > before.fuel);
            assert!(created.usage.fuel > after.fuel);
            assert_eq!(created.usage.host_calls, repetitions as u64 + 1);
            let prep_before = child.preparation_before.get().unwrap();
            let wire = child.wire_preparation.get().unwrap();
            assert!(wire.work > prep_before.work);
            assert!(wire.allocation_bytes > prep_before.allocation_bytes);
            assert!(wire.elements > prep_before.elements);
            let prep_after = child.preparation_after.get().unwrap();
            assert!(prep_after.work > prep_before.work);
            assert!(prep_after.allocation_bytes > prep_before.allocation_bytes);
            assert!(prep_after.elements > prep_before.elements);
            assert_eq!(after.peak_frames, 2);
            assert_eq!(created.usage.peak_frames, 2);
            assert!(after.peak_live_cells > before.peak_live_cells || repetitions > 1);
            assert_eq!(h.engine.check(&child.parent.get().unwrap()), 5);
            assert_eq!(h.engine.check(&child.nested.get().unwrap()), 5);
            assert!(matches!(
                unsafe {
                    lang_codegen_clif::component::with_child(
                        &child.parent.get().unwrap(),
                        |_, _, _| Ok(()),
                    )
                },
                Err(5)
            ));
            let execution::Value::Resource(claim) = created.value else {
                panic!("original native owner")
            };
            let (cleanup, _) = h.registration(false);
            h.call(cleanup, "consume", Some(claim), compiled).unwrap();
            assert_eq!(watch.destroyed.get(), 1);
            assert_eq!(watch.produced.get(), 1);
            assert_eq!(watch.released.get(), 1);
            assert!(h.values.is_empty());
            assert_eq!(h.allocator.stats().live_allocations, 0);
            assert!(!h.state.poisoned() && !child.state.borrow().poisoned());
        }
    }
}

#[test]
fn original_native_callback_executes_a_guest_child_and_restores_both_scopes() {
    original_native_callback_roundtrip(true);
}

#[test]
fn finite_original_preparation_executes_guest_children_and_releases_original_owners() {
    original_native_callback_roundtrip(false);
}
#[test]
fn private_child_profile_refuses_unconnected_finite_policies_before_native_effects() {
    for compiled in [false, true] {
        for policy in 0..3 {
            let mut h = Harness::new();
            h.preparation_limits.work = usize::MAX;
            h.preparation_limits.allocations = usize::MAX;
            h.preparation_limits.elements = usize::MAX;

            let child = Rc::new(Child::new(&mut h.values, compiled, 1, usize::MAX));
            let (registration, watch) = h.registration_child(false, Some(child.clone()));
            let mut limits = e::Limits::unbounded();
            if policy == 0 {
                limits.frames = Some(256);
            }
            if policy == 1 {
                limits.live_cells = Some(1000);
            }
            let mut control = e::Control::with_limits(limits);
            if policy == 2 {
                control = control.and_service_work(Some(1000));
            }
            let failure = h
                .call_control(registration, "create", None, compiled, control)
                .err()
                .expect("unsupported private policy");
            assert!(matches!(failure.reason, r::Reason::Unsupported));
            assert_eq!(watch.produced.get(), 0);
            assert_eq!(watch.destroyed.get(), 0);
            assert_eq!(watch.released.get(), 1);
            assert!(child.parent.get().is_none() && child.nested.get().is_none());
            assert!(h.values.is_empty());
            assert_eq!(h.allocator.stats().requests, 0);
            assert!(!h.state.poisoned() && !child.state.borrow().poisoned());
        }
    }
}

#[test]
fn shared_accounting_exhausts_cumulative_fuel_with_original_owner_cleanup() {
    for compiled in [false, true] {
        let baseline = retained_roundtrip(compiled, true);
        let limit = baseline.0.fuel.max(baseline.1.fuel);
        let control = e::Control::with_limits(e::Limits {
            fuel: Some(limit),
            ..Default::default()
        });
        let mut budget = m::Budget::new(Default::default());
        let accounting = e::InvocationAccounting::prepare(control, &mut budget).unwrap();
        let mut h = Harness::new();
        let (provider, watch) = h.registration(false);
        let created = h
            .call_execution(
                provider,
                "create",
                None,
                compiled,
                control,
                Some(&accounting),
            )
            .unwrap();
        let execution::Value::Resource(claim) = created.value else {
            panic!("original owner")
        };
        let (cleanup, _) = h.registration(false);
        let failure = h
            .call_execution(
                cleanup,
                "consume",
                Some(claim),
                compiled,
                control,
                Some(&accounting),
            )
            .err()
            .expect("expected accounted refusal");
        assert!(matches!(
            failure.reason,
            r::Reason::Control(e::Failure::FuelExhausted)
        ));
        assert_eq!(failure.usage.fuel, limit);
        assert_eq!(failure.usage, accounting.snapshot());
        assert_eq!(watch.destroyed.get(), 1);
        assert_eq!(watch.released.get(), 1);
        assert!(h.values.is_empty());
        assert_eq!(h.allocator.stats().live_allocations, 0);
        assert!(!h.state.poisoned());
    }
}

#[test]
fn shared_accounting_preserves_caller_after_combined_host_call_refusal() {
    for compiled in [false, true] {
        let control = e::Control::with_limits(e::Limits {
            host_calls: Some(1),
            ..Default::default()
        });
        let accounting =
            e::InvocationAccounting::prepare(control, &mut m::Budget::new(Default::default()))
                .unwrap();
        let mut h = Harness::new();
        let (provider, watch) = h.registration(false);
        let created = h
            .call_execution(
                provider.clone(),
                "create",
                None,
                compiled,
                control,
                Some(&accounting),
            )
            .unwrap();
        let execution::Value::Resource(claim) = created.value else {
            panic!("original owner")
        };
        let failure = h
            .call_execution(
                provider.clone(),
                "create",
                None,
                compiled,
                control,
                Some(&accounting),
            )
            .err()
            .expect("expected accounted refusal");
        assert!(matches!(
            failure.reason,
            r::Reason::Control(e::Failure::HostCallLimit)
        ));
        assert_eq!(failure.usage.host_calls, 1);
        assert_eq!(failure.usage, accounting.snapshot());
        assert_eq!(watch.produced.get(), 1);
        assert_eq!(watch.destroyed.get(), 0);
        let outcome = h
            .call_execution(
                provider,
                "consume",
                Some(claim),
                compiled,
                control,
                Some(&accounting),
            )
            .unwrap();
        assert_eq!(outcome.usage.host_calls, 1);
        assert_eq!(outcome.usage, accounting.snapshot());
        assert_eq!(watch.destroyed.get(), 1);
        assert_eq!(watch.released.get(), 1);
        assert!(h.values.is_empty());
        assert_eq!(h.allocator.stats().live_allocations, 0);
    }
}

#[test]
fn shared_accounting_work_prefixes_preserve_or_retire_every_original_owner() {
    for compiled in [false, true] {
        let baseline = retained_roundtrip(compiled, true);
        let first_work = baseline.0.service_work;
        let total_work = first_work + baseline.1.service_work;
        let mut previous: Option<(u64, bool)> = None;
        for limit in [
            0,
            first_work.saturating_sub(1),
            first_work,
            total_work - 1,
            total_work,
            total_work + 1,
        ] {
            let control = e::Control::with_limits(Default::default()).and_service_work(Some(limit));
            let accounting =
                e::InvocationAccounting::prepare(control, &mut m::Budget::new(Default::default()))
                    .unwrap();
            let mut h = Harness::new();
            let (provider, watch) = h.registration(false);
            let first = h.call_execution(
                provider.clone(),
                "create",
                None,
                compiled,
                control,
                Some(&accounting),
            );
            let success = match first {
                Err(failure) => {
                    assert!(matches!(
                        failure.reason,
                        r::Reason::Control(e::Failure::ServiceWorkLimit { .. })
                    ));
                    assert_eq!(failure.usage, accounting.snapshot());
                    false
                }
                Ok(created) => {
                    let immutable_usage = created.usage;
                    let execution::Value::Resource(claim) = created.value else {
                        panic!("original owner")
                    };
                    match h.call_execution(
                        provider.clone(),
                        "consume",
                        Some(claim),
                        compiled,
                        control,
                        Some(&accounting),
                    ) {
                        Ok(consumed) => {
                            assert_eq!(consumed.usage.fuel, baseline.0.fuel + baseline.1.fuel);
                            assert_eq!(
                                consumed.usage.host_calls,
                                baseline.0.host_calls + baseline.1.host_calls
                            );
                            assert_eq!(consumed.usage.service_work, total_work);
                            assert_eq!(consumed.usage, accounting.snapshot());
                            assert_eq!(created.usage, immutable_usage);
                            assert!(consumed.usage.fuel > created.usage.fuel);
                            true
                        }
                        Err(failure) => {
                            assert!(matches!(
                                failure.reason,
                                r::Reason::Control(e::Failure::ServiceWorkLimit { .. })
                            ));
                            assert_eq!(failure.usage, accounting.snapshot());
                            let held = h
                                .values
                                .held_claims(
                                    h.state.holder(),
                                    &h.allocator,
                                    &mut m::Budget::new(Default::default()),
                                )
                                .unwrap();
                            let refused_usage = accounting.snapshot();
                            for claim in held {
                                h.call(provider.clone(), "consume", Some(claim), compiled)
                                    .unwrap();
                            }
                            assert_eq!(accounting.snapshot(), refused_usage);
                            false
                        }
                    }
                }
            };
            assert_eq!(success, limit >= total_work, "mode={compiled} work={limit}");
            assert!(accounting.snapshot().service_work <= limit);
            assert_eq!(watch.produced.get(), watch.destroyed.get());
            assert!(h.values.is_empty());
            assert_eq!(h.allocator.stats().live_allocations, 0);
            assert!(!h.state.poisoned());
            drop(provider);
            assert_eq!(watch.released.get(), 1);
            if let Some((earlier, was_success)) = previous {
                assert!(limit >= earlier && (!was_success || success));
            }
            previous = Some((limit, success));
        }
    }
}

#[test]
fn shared_accounting_rejects_configuration_changes_and_refunds_no_preparation() {
    let mut budget = m::Budget::new(Default::default());
    let flag = e::InterruptionFlag::new();
    let control = e::Control::with_limits(Default::default())
        .and_interruption(&flag)
        .and_service_work(Some(0));
    let accounting = e::InvocationAccounting::prepare(control, &mut budget).unwrap();
    let mut h = Harness::new();
    let mismatched = e::Control::with_limits(Default::default()).and_service_work(Some(0));
    assert!(
        h.engine
            .begin(mismatched)
            .unwrap()
            .with_accounting(&accounting)
            .is_err()
    );
    assert!(accounting.same_invocation(&accounting.clone()));
    let other = e::InvocationAccounting::prepare(control, &mut budget).unwrap();
    assert!(!accounting.same_invocation(&other));
    let (provider, watch) = h.registration(false);
    let claim = h.publish(&provider, &watch);
    {
        let Harness {
            allocator,
            values,
            state,
            engine,
            ..
        } = &mut h;
        prepared(provider.clone(), |program, _| {
            state
                .with_program(program, |instance| {
                    let mut invoker = engine
                        .begin(control)
                        .unwrap()
                        .with_accounting(&accounting)
                        .unwrap();
                    let invocation = execution::Invocation::prepare_owned(
                        instance,
                        entry(program, "consume"),
                        values,
                        &mut invoker,
                        &mut m::Budget::new(Default::default()),
                    )
                    .unwrap();
                    let input = wire::Values {
                        roots: vec![wire::ValueId(0)],
                        nodes: vec![wire::Value::Resource(
                            lang_runtime::component::adapter::BoundaryResourceIndex::new(1)
                                .unwrap(),
                        )],
                    };
                    for _ in 0..2 {
                        let mut fresh_session = wire::Session::new(Default::default());
                        assert!(matches!(
                            invocation.encode_arguments(
                                &input,
                                &[claim],
                                allocator,
                                &mut fresh_session
                            ),
                            Err(r::Reason::Control(e::Failure::ServiceWorkLimit { .. }))
                        ));
                        assert!(accounting.preparation_failure().is_some());
                    }
                    assert!(!execution::Host::arguments_committed(&invocation));
                })
                .unwrap();
        });
    }
    h.call(provider, "consume", Some(claim), false).unwrap();
    assert_eq!(watch.destroyed.get(), 1);
    assert_eq!(watch.released.get(), 1);
    assert_eq!(watch.produced.get(), 0);
    assert!(h.values.is_empty());
    assert_eq!(h.allocator.stats().live_allocations, 0);
    let mut refused = m::Budget::new(m::Limits {
        allocations: 0,
        ..Default::default()
    });
    assert!(e::InvocationAccounting::prepare(control, &mut refused).is_err());
    assert_eq!(accounting.snapshot(), Default::default());
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
#[test]
fn borrowed_preparation_entry_keeps_precharges_and_refuses_mismatched_or_exhausted_limits() {
    for compiled in [false, true] {
        for case in 0..4 {
            let mut h = Harness::new();
            let (registration, watch) = h.registration(false);
            let result = prepared(registration, |program, session| {
                let function = entry(program, "create");
                let generated = compiled.then(|| {
                    lang_codegen_clif::tasks::CompiledProgram::compile_component(
                        program,
                        function,
                        Default::default(),
                    )
                    .unwrap()
                });
                let control = e::Control::default();
                let mut invoker = h.engine.begin(control).unwrap();
                let mut budget = m::Budget::new(Default::default());
                budget.charge(17).unwrap();
                budget.bytes(23).unwrap();
                budget.allocate::<()>(29).unwrap();
                h.state
                    .with_program(program, |instance| {
                        let mut invocation = execution::Invocation::prepare_owned(
                            instance,
                            function,
                            &mut h.values,
                            &mut invoker,
                            &mut budget,
                        )
                        .unwrap();
                        invocation.prepare_output(session).unwrap();
                        let before = preparation_usage(&budget);
                        let address = std::ptr::from_mut(&mut budget) as usize;
                        let checkpoint = before.work.checked_add(program.usage().work).unwrap();
                        let mut limits = budget.limits();
                        limits.work = match case {
                            1 => checkpoint - 1,
                            2 => checkpoint,
                            _ => checkpoint + 1_000_000,
                        };
                        budget = m::Budget::with_usage(limits, before).unwrap();
                        let mut requested = limits;
                        if case == 0 {
                            requested.depth += 1;
                        }
                        let result = execute_memory_with_budget(
                            program,
                            function,
                            generated.as_ref(),
                            &mut h.allocator,
                            CallControl {
                                execution: control,
                                preparation: requested,
                            },
                            &[],
                            &mut invocation,
                            Some(&mut budget),
                        );
                        assert_eq!(std::ptr::from_mut(&mut budget) as usize, address);
                        let after = preparation_usage(&budget);
                        if case == 0 || case == 1 {
                            assert_eq!(after, before);
                        } else if case == 2 {
                            assert_eq!(after.work, checkpoint);
                        } else {
                            assert!(after.work > checkpoint);
                            assert!(
                                after.allocation_bytes
                                    > before.allocation_bytes + program.usage().allocation_bytes
                            );
                            assert!(after.elements > before.elements + program.usage().elements);
                        }
                        result
                    })
                    .unwrap()
            });
            if case < 3 {
                let failure = result.err().expect("entry refusal");
                if case == 0 {
                    assert!(matches!(failure.reason, r::Reason::Entry));
                } else {
                    assert!(matches!(
                        failure.reason,
                        r::Reason::Resource(m::Error {
                            kind: m::ErrorKind::Limit {
                                kind: m::LimitKind::Work,
                                ..
                            },
                            ..
                        })
                    ));
                }
                assert_eq!(watch.produced.get(), 0);
                assert_eq!(watch.destroyed.get(), 0);
                assert_eq!(h.allocator.stats().requests, 0);
            } else {
                let outcome = result.unwrap();
                let execution::Value::Frame(frame) = outcome.value else {
                    panic!("resource result frame")
                };
                let claim = frame.resources()[0];
                let (cleanup, _) = h.registration(false);
                h.call(cleanup, "consume", Some(claim), compiled).unwrap();
                assert_eq!(watch.produced.get(), 1);
                assert_eq!(watch.destroyed.get(), 1);
            }
            assert_eq!(watch.released.get(), 1);
            assert!(h.values.is_empty());
            assert_eq!(h.allocator.stats().live_allocations, 0);
            assert!(!h.state.poisoned());
        }
    }
}

#[test]
fn child_grant_session_caps_keep_original_history_and_parent_cleanup() {
    for compiled in [false, true] {
        let run = |work| {
            let mut h = Harness::new();
            h.preparation_limits.work = usize::MAX;
            h.preparation_limits.allocations = usize::MAX;
            h.preparation_limits.elements = usize::MAX;
            let child = Rc::new(Child::new(&mut h.values, compiled, 1, work));
            let (registration, watch) = h.registration_child(false, Some(child.clone()));
            let result = h.call_control(
                registration,
                "create",
                None,
                compiled,
                e::Control::with_limits(e::Limits::unbounded()),
            );
            let before = child.preparation_before.get().unwrap();
            let observed = child.wire_attempt.get().unwrap();
            assert!(
                observed.work > before.work && observed.allocation_bytes > before.allocation_bytes
            );
            let metadata = child
                .wire_preparation
                .get()
                .map(|used| used.work - before.work);
            let total = child.wire_total.get().map(|used| used.work);
            if let Some(total) = total {
                assert!(total <= work);
            }
            let success = result.is_ok();
            if success {
                let execution::Value::Resource(claim) = result.unwrap().value else {
                    panic!("parent resource");
                };
                let (cleanup, _) = h.registration(false);
                h.call(cleanup, "consume", Some(claim), compiled).unwrap();
                assert_eq!(watch.produced.get(), 1);
                assert_eq!(watch.destroyed.get(), 1);
            } else {
                let failure = result.err().unwrap();
                assert!(matches!(
                    failure.reason,
                    r::Reason::Native(execution::NativeFailure::Status { code: 1, .. })
                ));
                assert_eq!(watch.produced.get(), 0);
                assert_eq!(watch.destroyed.get(), 0);
                let (retry, retry_watch) = h.registration(false);
                let execution::Value::Resource(claim) =
                    h.call(retry, "create", None, compiled).unwrap().value
                else {
                    panic!("ready parent after refusal");
                };
                let (cleanup, _) = h.registration(false);
                h.call(cleanup, "consume", Some(claim), compiled).unwrap();
                assert_eq!(retry_watch.produced.get(), 1);
                assert_eq!(retry_watch.destroyed.get(), 1);
                assert_eq!(retry_watch.released.get(), 1);
            }
            assert_eq!(watch.released.get(), 1);
            assert_eq!(h.allocator.stats().live_allocations, 0);
            assert!(h.values.is_empty());
            assert!(!h.state.poisoned(), "parent poison with wire cap {work}");
            assert!(!child.state.borrow().poisoned());
            assert_eq!(h.engine.check(&child.parent.get().unwrap()), 5);
            if let Some(nested) = child.nested.get() {
                assert_eq!(h.engine.check(&nested), 5);
            }
            if metadata.is_none() {
                assert!(child.nested.get().is_none());
            }
            (success, total, metadata, observed.work - before.work)
        };
        let (success, Some(total), Some(metadata), _) = run(usize::MAX) else {
            panic!("successful unbounded session");
        };
        assert!(success && total > metadata && metadata > 0);
        let (success, Some(exact), Some(exact_metadata), _) = run(total) else {
            panic!("exact session cap");
        };
        assert!(success);
        assert_eq!((exact, exact_metadata), (total, metadata));
        let (success, _, phase, retained) = run(metadata - 1);
        assert!(!success && phase.is_none() && retained < metadata);
        let (success, Some(retained), Some(_), _) = run(total - 1) else {
            panic!("canonical refusal after metadata");
        };
        assert!(!success && retained < total);
    }
}

#[test]
fn canonical_reply_copy_spends_the_original_child_ledger_before_decode_refusal() {
    for compiled in [false, true] {
        for remaining in [0, 52, 53] {
            let mut h = Harness::new();
            h.preparation_limits.work = usize::MAX;
            h.preparation_limits.allocations = usize::MAX;
            h.preparation_limits.elements = usize::MAX;
            let child = Rc::new(Child::new(&mut h.values, compiled, 1, usize::MAX));
            child.reply_work.set(Some(remaining));
            let (registration, watch) = h.registration_child(false, Some(child.clone()));
            let failure = h
                .call_control(
                    registration,
                    "create",
                    None,
                    compiled,
                    e::Control::with_limits(e::Limits::unbounded()),
                )
                .err()
                .expect("canonical reply/decode work refusal");
            assert!(matches!(
                failure.reason,
                r::Reason::Native(execution::NativeFailure::Status { code: 1, .. })
            ));
            let before = child.reply_before.get().unwrap();
            let after = child.wire_attempt.get().unwrap();
            let spent = if remaining == 53 { 53 } else { 0 };
            assert_eq!(before.work, usize::MAX - remaining);
            assert_eq!(after.work - before.work, spent);
            assert_eq!(after.work, before.work + spent);
            if spent == 53 {
                assert!(after.allocation_bytes >= before.allocation_bytes + 53);
                assert!(after.elements >= before.elements + 53);
            } else {
                assert_eq!(after.allocation_bytes, before.allocation_bytes);
                assert_eq!(after.elements, before.elements);
            }
            assert!(
                child.preparation_after.get().is_none(),
                "child fails before the parent publishes a resource"
            );
            assert_eq!(child.value.get(), 0);
            assert_eq!(watch.produced.get(), 0);
            assert_eq!(watch.destroyed.get(), 0);
            assert_eq!(watch.released.get(), 1);
            assert!(h.values.is_empty());
            assert_eq!(h.allocator.stats().live_allocations, 0);
            assert!(!h.state.poisoned() && !child.state.borrow().poisoned());
            assert_eq!(h.engine.check(&child.parent.get().unwrap()), 5);
            assert_eq!(h.engine.check(&child.nested.get().unwrap()), 5);
            let (retry, retry_watch) = h.registration(false);
            let execution::Value::Resource(claim) =
                h.call(retry, "create", None, compiled).unwrap().value
            else {
                panic!("ready parent");
            };
            let (cleanup, _) = h.registration(false);
            h.call(cleanup, "consume", Some(claim), compiled).unwrap();
            assert_eq!(retry_watch.produced.get(), 1);
            assert_eq!(retry_watch.destroyed.get(), 1);
            assert_eq!(retry_watch.released.get(), 1);
            assert_eq!(h.allocator.stats().live_allocations, 0);
            assert!(h.values.is_empty());
        }
    }
}
#[test]
fn decoded_metadata_proof_cost_excludes_unrelated_original_precharges() {
    let unit = stored_unit();
    let mut seed = wire::Session::new(Default::default());
    let contract = wire::artifact::source_contract(&unit, &mut seed, 65_536).unwrap();
    let bytes = wire::encode_contract(&contract, &mut seed).unwrap();
    for canonical in [false, true] {
        let mut reference = None;
        for initial in [
            m::Usage::default(),
            m::Usage {
                work: 17,
                allocation_bytes: 27_000_000,
                elements: 92_608,
            },
        ] {
            let mut budget = m::Budget::with_usage(Default::default(), initial).unwrap();
            let mut session = wire::Session::new(Default::default());
            let proof = {
                let mut scope = session.scope_preparation(&mut budget).unwrap();
                let contract = if canonical {
                    wire::decode_contract(m::SourceId(0), &bytes, &mut scope).unwrap()
                } else {
                    wire::artifact::source_contract(&unit, &mut scope, 65_536).unwrap()
                };
                contract.checked().usage().budget
            };
            let after = preparation_usage(&budget);
            let delta = m::Usage {
                work: after.work - initial.work,
                allocation_bytes: after.allocation_bytes - initial.allocation_bytes,
                elements: after.elements - initial.elements,
            };
            assert_eq!(session.usage().work, delta.work);
            assert_eq!(session.usage().allocation_bytes, delta.allocation_bytes);
            assert_eq!(session.preparation_usage().elements, delta.elements);
            if let Some((expected_proof, expected_delta)) = reference {
                assert_eq!(
                    proof, expected_proof,
                    "immutable proof excludes global caller history"
                );
                assert_eq!(
                    delta, expected_delta,
                    "original ledger keeps every actual local charge"
                );
            } else {
                reference = Some((proof, delta));
            }
        }
    }
}

#[test]
fn borrowed_runtime_pools_keep_costs_failed_prefixes_and_guest_refusal() {
    use lang_runtime::{
        arena::managed as arena, inspection, layout, retirement, shared::managed as shared,
    };
    let h = Harness::new();
    let (registration, watch) = h.registration(false);
    prepared(registration, |program, _| {
        let layouts =
            layout::Layouts::compile_component_program(program, Default::default()).unwrap();
        let function = entry(program, "create");
        let frame = layout::FramePlan::compile_tasks_after(
            &layouts,
            program.proof(),
            function,
            Default::default(),
            usize::MAX,
            program.usage(),
        )
        .unwrap();
        let base = frame.usage();
        let initial = m::Usage {
            work: base.work + 17,
            allocation_bytes: base.allocation_bytes + 23,
            elements: base.elements + 29,
        };
        let allocator = a::Allocator::new(
            a::System,
            a::Limits {
                live_allocations: 64,
                ..Default::default()
            },
        )
        .unwrap();
        let arena_limits = arena::Limits {
            nodes: 3,
            value_cells: 5,
            readers: 2,
            ..Default::default()
        };
        let heap_limits = shared::Limits {
            nodes: 3,
            slots: 4,
            tickets: 5,
            readers: 2,
            value_cells: 6,
            ..Default::default()
        };
        for stage in 0..5 {
            let run = |budget: &mut m::Budget| -> Result<m::Usage, String> {
                match stage {
                    0 => {
                        let plan = layout::FramePlan::compile_tasks_after(
                            &layouts,
                            program.proof(),
                            function,
                            budget.limits(),
                            usize::MAX,
                            program.usage(),
                        )
                        .map_err(|e| format!("{e:?}"))?;
                        plan.prepare_pool_with_budget(budget)
                            .map(|v| v.usage())
                            .map_err(|e| format!("{e:?}"))
                    }
                    1 => arena::Arena::prepare_with_budget(&layouts, arena_limits, budget)
                        .map(|v| v.usage())
                        .map_err(|e| format!("{e:?}")),
                    2 => inspection::Inspector::prepare_with_budget(&layouts, 5, budget)
                        .map(|v| v.usage())
                        .map_err(|e| format!("{e:?}")),
                    3 => shared::Heap::prepare_with_budget(&layouts, heap_limits, budget)
                        .map(|v| v.usage())
                        .map_err(|e| format!("{e:?}")),
                    _ => retirement::Drain::prepare_with_budget(&layouts, &allocator, budget)
                        .map(|v| v.usage())
                        .map_err(|e| format!("{e:?}")),
                }
            };
            let owned = match stage {
                0 => frame.prepare_pool_after(initial).unwrap().usage(),
                1 => arena::Arena::prepare(&layouts, arena_limits, Default::default(), initial)
                    .unwrap()
                    .usage(),
                2 => inspection::Inspector::prepare(&layouts, 5, Default::default(), initial)
                    .unwrap()
                    .usage(),
                3 => shared::Heap::prepare(&layouts, heap_limits, Default::default(), initial)
                    .unwrap()
                    .usage(),
                _ => retirement::Drain::prepare(&layouts, &allocator, Default::default(), initial)
                    .unwrap()
                    .usage(),
            };
            let mut budget = m::Budget::with_usage(Default::default(), initial).unwrap();
            assert_eq!(run(&mut budget).unwrap(), owned, "stage {stage}");
            assert_eq!(preparation_usage(&budget), owned);
            let fields = |v: m::Usage| [v.work, v.allocation_bytes, v.elements];
            for dimension in 0..3 {
                if fields(owned)[dimension] == fields(initial)[dimension] {
                    continue;
                }
                for exact in [false, true] {
                    let mut limits = m::Limits::default();
                    let ceiling = fields(owned)[dimension] - usize::from(!exact);
                    match dimension {
                        0 => limits.work = ceiling,
                        1 => limits.allocations = ceiling,
                        _ => limits.elements = ceiling,
                    }
                    let mut budget = m::Budget::with_usage(limits, initial).unwrap();
                    assert_eq!(
                        run(&mut budget).is_ok(),
                        exact,
                        "stage {stage}, dimension {dimension}"
                    );
                    assert_eq!(budget.limits(), limits);
                    let after = preparation_usage(&budget);
                    if exact {
                        assert_eq!(after, owned);
                    } else {
                        assert!(
                            fields(after)
                                .iter()
                                .zip(fields(initial))
                                .all(|(a, b)| *a >= b)
                        );
                        if stage == 1 || stage == 3 {
                            assert!(
                                after.allocation_bytes > initial.allocation_bytes,
                                "accepted allocation prefix in stage {stage}"
                            );
                        }
                        assert!(run(&mut budget).is_err());
                        let repeated = preparation_usage(&budget);
                        assert!(
                            fields(repeated)
                                .iter()
                                .zip(fields(after))
                                .all(|(a, b)| *a >= b)
                        );
                        assert!(fields(repeated)[dimension] <= ceiling);
                    }
                }
            }
            let mut invalid = m::Budget::new(Default::default());
            assert!(run(&mut invalid).is_err());
            assert_eq!(preparation_usage(&invalid), m::Usage::default());
        }
        // Failure after bounded host pools must keep their accepted charges.
        let mut overflow = m::Budget::with_usage(Default::default(), initial).unwrap();
        let arena_overflow = arena::Limits {
            readers: usize::MAX,
            ..arena_limits
        };
        assert!(
            arena::Arena::prepare_with_budget(&layouts, arena_overflow, &mut overflow).is_err()
        );
        assert!(overflow.allocation_bytes() > initial.allocation_bytes);
        let accepted = preparation_usage(&overflow);
        assert!(
            arena::Arena::prepare_with_budget(&layouts, arena_overflow, &mut overflow).is_err()
        );
        assert!(overflow.allocation_bytes() > accepted.allocation_bytes);
        let mut overflow = m::Budget::with_usage(Default::default(), initial).unwrap();
        let heap_overflow = shared::Limits {
            tickets: usize::MAX,
            ..heap_limits
        };
        assert!(shared::Heap::prepare_with_budget(&layouts, heap_overflow, &mut overflow).is_err());
        assert!(overflow.allocation_bytes() > initial.allocation_bytes);

        struct Refusing(Rc<Cell<usize>>);
        impl a::Provider for Refusing {
            type Region = a::NoRegion;
            fn allocate(&mut self, _: a::Request) -> Option<Self::Region> {
                self.0.set(self.0.get() + 1);
                None
            }
            fn deallocate(&mut self, region: Self::Region, _: a::Request) {
                match region {}
            }
        }
        let calls = Rc::new(Cell::new(0));
        let mut refusing = a::Allocator::new(Refusing(calls.clone()), Default::default()).unwrap();
        let mut budget = m::Budget::with_usage(Default::default(), initial).unwrap();
        let pool_cost = frame.prepare_pool_after(initial).unwrap().usage();
        let changed = m::Limits {
            depth: 1,
            ..Default::default()
        };
        let mut mismatch = m::Budget::with_usage(changed, initial).unwrap();
        assert!(matches!(
            frame.prepare_pool_with_budget(&mut mismatch),
            Err(layout::Error::State)
        ));
        assert!(matches!(
            frame.reserve_with_budget(&mut refusing, &mut mismatch),
            Err(layout::Error::State)
        ));
        assert_eq!(preparation_usage(&mismatch), initial);
        assert_eq!(calls.get(), 0);
        for attempt in 1..=2 {
            let before = preparation_usage(&budget);
            assert!(matches!(
                frame.reserve_with_budget(&mut refusing, &mut budget),
                Err(layout::Error::Allocation(a::Error::OutOfMemory))
            ));
            let after = preparation_usage(&budget);
            assert_eq!(after.work - before.work, pool_cost.work - initial.work);
            assert_eq!(
                after.allocation_bytes - before.allocation_bytes,
                pool_cost.allocation_bytes - initial.allocation_bytes
            );
            assert_eq!(
                after.elements - before.elements,
                pool_cost.elements - initial.elements
            );
            assert_eq!(calls.get(), attempt);
            assert_eq!(refusing.stats().live_allocations, 0);
        }
        assert_eq!(allocator.stats().requests, 0);
    });
    assert_eq!(watch.released.get(), 1);
    assert_eq!(h.allocator.stats().live_allocations, 0);
    assert!(h.values.is_empty());
}

#[test]
fn borrowed_layout_and_frame_failures_keep_every_precharged_dimension() {
    let h = Harness::new();
    let (registration, watch) = h.registration(false);
    prepared(registration, |program, _| {
        let types = program.proof().typed().places().declarations().types();
        let function = entry(program, "create");
        let owned =
            lang_runtime::layout::Layouts::compile_component_program(program, Default::default())
                .unwrap();
        let initial = program.usage();
        let initial = m::Usage {
            work: initial.work + 17,
            allocation_bytes: initial.allocation_bytes + 23,
            elements: initial.elements + 29,
        };
        let mut probe = m::Budget::with_usage(Default::default(), initial).unwrap();
        let borrowed = lang_runtime::layout::Layouts::compile_component_program_with_budget(
            program, &mut probe,
        )
        .unwrap();
        assert_eq!(
            borrowed.usage(),
            owned.usage(),
            "caller history does not inflate local layout estimates"
        );
        let after_layout = preparation_usage(&probe);
        assert_eq!(
            after_layout.work - initial.work,
            owned.usage().work - types.work_used()
        );
        assert_eq!(
            after_layout.allocation_bytes - initial.allocation_bytes,
            owned.usage().allocation_bytes - types.requested_bytes()
        );
        assert_eq!(
            after_layout.elements - initial.elements,
            owned.usage().elements - types.elements_used()
        );
        let owned_frame = lang_runtime::layout::FramePlan::compile_tasks_after(
            &owned,
            program.proof(),
            function,
            Default::default(),
            usize::MAX,
            after_layout,
        )
        .unwrap();
        let borrowed_frame = lang_runtime::layout::FramePlan::compile_tasks_with_budget(
            &borrowed,
            program.proof(),
            function,
            &mut probe,
            usize::MAX,
        )
        .unwrap();
        assert_eq!(borrowed_frame.usage(), owned_frame.usage());
        assert_eq!(preparation_usage(&probe), owned_frame.usage());
        let finals = [after_layout, preparation_usage(&probe)];
        for frames in [false, true] {
            let start = if frames { after_layout } else { initial };
            let complete = finals[usize::from(frames)];
            for dimension in 0..3 {
                for exact in [false, true] {
                    let mut limits = m::Limits::default();
                    let ceiling = match dimension {
                        0 => complete.work,
                        1 => complete.allocation_bytes,
                        _ => complete.elements,
                    } - usize::from(!exact);
                    match dimension {
                        0 => limits.work = ceiling,
                        1 => limits.allocations = ceiling,
                        _ => limits.elements = ceiling,
                    };
                    let mut budget = m::Budget::with_usage(limits, start).unwrap();
                    let run = |budget: &mut m::Budget| {
                        if frames {
                            lang_runtime::layout::FramePlan::compile_tasks_with_budget(
                                &owned,
                                program.proof(),
                                function,
                                budget,
                                usize::MAX,
                            )
                            .map(drop)
                        } else {
                            lang_runtime::layout::Layouts::compile_component_program_with_budget(
                                program, budget,
                            )
                            .map(drop)
                        }
                    };
                    let result = run(&mut budget);
                    let after = preparation_usage(&budget);
                    assert_eq!(result.is_ok(), exact);
                    assert_eq!(budget.limits(), limits);
                    if exact {
                        assert_eq!(after, complete);
                    } else {
                        assert!(
                            after.work >= start.work
                                && after.allocation_bytes >= start.allocation_bytes
                                && after.elements >= start.elements
                        );
                        assert!(match dimension {
                            0 => after.work > start.work,
                            1 => after.allocation_bytes > start.allocation_bytes,
                            _ => after.elements > start.elements,
                        });
                        assert!(
                            match dimension {
                                0 => after.work,
                                1 => after.allocation_bytes,
                                _ => after.elements,
                            } <= ceiling
                        );
                        assert!(run(&mut budget).is_err());
                        let repeated = preparation_usage(&budget);
                        assert!(
                            repeated.work >= after.work
                                && repeated.allocation_bytes >= after.allocation_bytes
                                && repeated.elements >= after.elements
                        );
                        assert!(
                            match dimension {
                                0 => repeated.work,
                                1 => repeated.allocation_bytes,
                                _ => repeated.elements,
                            } <= ceiling
                        );
                    }
                }
            }
        }
    });
    assert_eq!(watch.released.get(), 1);
    assert_eq!(h.allocator.stats().live_allocations, 0);
    assert!(h.values.is_empty());
}
