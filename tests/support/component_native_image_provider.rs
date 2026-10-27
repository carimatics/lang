#![allow(
    dead_code,
    reason = "Shared producer/standalone consumer callback fixture."
)]
use lang::component_embedding::{ffi, gateway};
use lang_mir::unit as m;
use lang_runtime::component::{idl, wire};
use std::{
    cell::{Cell, RefCell},
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

pub const HOST_IDL: &[u8]=br#"{"idl":1,"package":"demo:host","version":"1.0.0","interfaces":[{"key":"demo:host/api","resources":[],"types":[],"functions":[{"name":"add","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"host.add"}]}],"imports":[],"exports":["demo:host/api"]}"#;
pub const SOURCE_IDL: &[u8]=br#"{"idl":1,"package":"demo:object","version":"1.0.0","interfaces":[{"key":"demo:host/api","resources":[],"types":[],"functions":[{"name":"add","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"host.add"}]},{"key":"demo:object/api","resources":[],"types":[],"functions":[{"name":"answer","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"demo.object.answer"}]}],"imports":[{"interface":"demo:host/api","version":{"exact":"1.0.0"},"members":["add"]}],"exports":["demo:object/api"]}"#;
pub static RELEASED: AtomicUsize = AtomicUsize::new(0);
pub const HOST_RESOURCE_IDL: &[u8]=br#"{"idl":1,"package":"demo:host","version":"1.0.0","interfaces":[{"key":"demo:host/api","resources":["item"],"types":[],"functions":[{"name":"add","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"host.add"},{"name":"make","effect":"sync","params":[],"result":{"own":"item"},"capability":"host.make"}]}],"imports":[],"exports":["demo:host/api"]}"#;
pub const RESOURCE_SOURCE_IDL: &[u8]=br#"{"idl":1,"package":"demo:object","version":"1.0.0","interfaces":[{"key":"demo:host/api","resources":["item"],"types":[],"functions":[{"name":"add","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"host.add"},{"name":"make","effect":"sync","params":[],"result":{"own":"item"},"capability":"host.make"}]},{"key":"demo:object/api","resources":[],"types":[],"functions":[{"name":"answer","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"demo.object.answer"}]}],"imports":[{"interface":"demo:host/api","version":{"exact":"1.0.0"},"members":["add","make"]}],"exports":["demo:object/api"]}"#;
pub struct State {
    pub resource: bool,
    pub host: wire::Contract,
    pub source: wire::Contract,
    pub child: Cell<*mut ffi::Binding>,
    pub depth: Cell<u32>,
    pub calls: Cell<u32>,
    pub max_depth: Cell<u32>,
    pub reserve: Cell<Option<usize>>,
    pub before: Cell<m::Usage>,
    pub after: Cell<m::Usage>,
    pub created: Cell<usize>,
    pub retired: Cell<usize>,
    pub live: RefCell<Vec<usize>>,
    pub fail_destroy: Cell<bool>,
    pub explicit_drop: Cell<bool>,
    pub drop_report: Cell<*mut ffi::Result>,
    pub interrupt_drop: Cell<bool>,
    pub control: Cell<*mut ffi::Control>,
    pub drop_reserve: Cell<Option<usize>>,
    pub drop_before: Cell<m::Usage>,
    pub drop_after: Cell<m::Usage>,
}
pub fn header<T>() -> ffi::Header {
    ffi::Header {
        size: std::mem::size_of::<T>() as u32,
        version: 1,
    }
}
pub fn bytes(data: &[u8]) -> ffi::Bytes {
    ffi::Bytes {
        data: data.as_ptr(),
        length: data.len() as u64,
    }
}
fn api() -> &'static ffi::Api {
    let mut error: ffi::Error = unsafe { std::mem::zeroed() };
    error.header = header::<ffi::Error>();
    let mut output = ptr::null();
    assert_eq!(
        unsafe {
            gateway::lang_component_v1_get_api(
                1,
                size_of::<ffi::Api>() as u32,
                &mut output,
                &mut error,
            )
        },
        0
    );
    unsafe { &*output }
}
pub fn contract(input: &[u8]) -> wire::Contract {
    wire::Contract::new(
        idl::check(m::SourceId(1), input, Default::default()).unwrap(),
        vec![],
        &mut wire::Session::new(Default::default()),
    )
    .unwrap()
}
pub fn grant(resource: bool, member: &str) -> wire::Contract {
    let mut document: serde_json::Value = serde_json::from_slice(if resource {
        HOST_RESOURCE_IDL
    } else {
        HOST_IDL
    })
    .unwrap();
    document["package"] = serde_json::json!("demo:caller");
    document["exports"] = serde_json::json!([]);
    document["imports"] = serde_json::json!([{"interface":"demo:host/api","version":{"exact":"1.0.0"},"members":[member]}]);
    contract(&serde_json::to_vec(&document).unwrap())
}
impl State {
    pub fn new() -> Self {
        Self::with_resources(false)
    }
    pub fn with_resources(resource: bool) -> Self {
        Self {
            resource,
            host: contract(if resource {
                HOST_RESOURCE_IDL
            } else {
                HOST_IDL
            }),
            source: contract(if resource {
                RESOURCE_SOURCE_IDL
            } else {
                SOURCE_IDL
            }),
            child: Cell::new(ptr::null_mut()),
            depth: Cell::new(0),
            calls: Cell::new(0),
            max_depth: Cell::new(0),
            reserve: Cell::new(None),
            before: Cell::new(m::Usage::default()),
            after: Cell::new(m::Usage::default()),
            created: Cell::new(0),
            retired: Cell::new(0),
            live: RefCell::new(Vec::new()),
            fail_destroy: Cell::new(false),
            explicit_drop: Cell::new(false),
            drop_report: Cell::new(ptr::null_mut()),
            interrupt_drop: Cell::new(false),
            control: Cell::new(ptr::null_mut()),
            drop_reserve: Cell::new(None),
            drop_before: Cell::new(m::Usage::default()),
            drop_after: Cell::new(m::Usage::default()),
        }
    }
    pub fn signature<'a>(&'a self, host: bool, session: &mut wire::Session) -> wire::Signature<'a> {
        let contract = if host { &self.host } else { &self.source };
        let key = contract
            .document()
            .interfaces
            .iter()
            .find(|i| {
                i.key.value.interface() == "api"
                    && i.key.value.package().package() == if host { "host" } else { "object" }
            })
            .unwrap()
            .key
            .value
            .clone();
        wire::Signature::new(contract, &key, if host { "add" } else { "answer" }, session).unwrap()
    }
}

unsafe fn observe_budget(
    context: *const ffi::Context,
    reserve: Option<usize>,
) -> Result<m::Usage, u32> {
    use lang_runtime::{
        component::{execution, storage},
        execution as e,
    };
    struct Meter {
        reserve: Option<usize>,
        seen: m::Usage,
    }
    impl execution::CallbackOperation for Meter {
        fn run<'code>(
            &mut self,
            _: &mut execution::Instance<'_, '_>,
            _: &mut storage::Resources<'code>,
            _: &mut dyn storage::Memory,
            budget: &mut m::Budget,
            _: &mut wire::Session,
            _: &execution::CallbackArguments<'_, 'code>,
            _: storage::Holder,
            _: &e::InvocationAccounting,
            _: &mut lang_runtime::task::CurrentTask<'_>,
        ) -> Result<(), execution::CallbackFailure> {
            if let Some(reserve) = self.reserve {
                let available = budget.limits().work.saturating_sub(budget.work_used());
                budget.charge(available.saturating_sub(reserve))?;
            }
            self.seen = m::Usage {
                work: budget.work_used(),
                allocation_bytes: budget.allocation_bytes(),
                elements: budget.elements_used(),
            };
            Ok(())
        }
    }
    let mut meter = Meter {
        reserve,
        seen: m::Usage::default(),
    };
    // SAFETY: only the original live callback ledger is observed/spent. No
    // guest body, resource or replacement ledger is constructed by this loan.
    unsafe {
        lang_codegen_clif::component::with_child(context, |_, call, _| {
            call.with_callback(&mut meter)
        })
    }?;
    Ok(meter.seen)
}

unsafe fn observe_retirement_budget(
    context: *const ffi::Context,
    reserve: Option<usize>,
) -> m::Usage {
    use lang_runtime::{
        component::{execution, storage},
        execution as e,
    };
    struct Meter {
        reserve: Option<usize>,
        seen: m::Usage,
    }
    impl execution::StorageOperation for Meter {
        fn run(
            &mut self,
            _: &mut storage::Resources<'_>,
            _: &mut dyn storage::Memory,
            budget: &mut m::Budget,
            _: storage::Holder,
            _: &e::InvocationAccounting,
        ) -> Result<(), u32> {
            if let Some(remaining) = self.reserve {
                budget
                    .charge(
                        budget
                            .limits()
                            .work
                            .saturating_sub(budget.work_used())
                            .saturating_sub(remaining),
                    )
                    .map_err(|_| 2u32)?;
            }
            self.seen = m::Usage {
                work: budget.work_used(),
                allocation_bytes: budget.allocation_bytes(),
                elements: budget.elements_used(),
            };
            Ok(())
        }
    }
    let mut meter = Meter {
        reserve,
        seen: m::Usage::default(),
    };
    // SAFETY: only the actual original retirement ledger is observed/spent.
    // No resource, invocation, guest entry or replacement budget is created.
    unsafe {
        lang_codegen_clif::component::with_retirement(context, |_, call| {
            call.with_storage(&mut meter)
        })
    }
    .unwrap();
    meter.seen
}

// This same real implementation supplies the producer's actual provider and
// the separately linked consumer's owning C registration.
pub unsafe extern "C" fn invoke(
    user: *mut c_void,
    context: *const ffi::Context,
    input: ffi::Bytes,
    output: *mut *mut ffi::Buffer,
) -> u32 {
    let state = unsafe { &*user.cast::<State>() };
    let mut error: ffi::Error = unsafe { std::mem::zeroed() };
    error.header = header::<ffi::Error>();
    if unsafe { (api().context_check)(context, &mut error) } != 0 {
        return 1;
    }
    state.calls.set(state.calls.get() + 1);
    state
        .max_depth
        .set(state.max_depth.get().max(state.depth.get() + 1));
    let mut session = wire::Session::new(Default::default());
    let signature = state.signature(true, &mut session);
    let values = wire::decode_values(
        &signature,
        wire::FrameKind::Parameters,
        unsafe { std::slice::from_raw_parts(input.data, input.length as usize) },
        &mut session,
    )
    .unwrap();
    let wire::Value::I64(value) = values.nodes[values.roots[0].0] else {
        panic!("i64 native argument")
    };
    let answer = if state.depth.get() == 0 && !state.child.get().is_null() {
        let child = state.signature(false, &mut session);
        let encoded = wire::encode_values(
            &child,
            wire::FrameKind::Parameters,
            &wire::Values {
                roots: vec![wire::ValueId(0)],
                nodes: vec![wire::Value::I64(value)],
            },
            &mut session,
        )
        .unwrap();
        let mut result = ptr::null_mut();
        match unsafe { observe_budget(context, state.reserve.get()) } {
            Ok(usage) => state.before.set(usage),
            Err(status) => return status,
        }
        state.depth.set(1);
        let status = unsafe {
            (api().context_call)(
                context,
                state.child.get(),
                bytes(&encoded),
                &mut result,
                &mut error,
            )
        };
        state.depth.set(0);
        if status != 0 {
            if !result.is_null() {
                assert_eq!(unsafe { (api().result_drop)(result, &mut error) }, 0);
            }
            if status != 1 && state.reserve.get().is_some() {
                // A refused preparation loan preserves the original failure.
                // Its reporting loan projects that failure without guest entry.
                let mut failure = ptr::null_mut();
                assert_eq!(
                    unsafe {
                        (api().context_call)(
                            context,
                            state.child.get(),
                            bytes(&encoded),
                            &mut failure,
                            &mut error,
                        )
                    },
                    1
                );
                assert!(!failure.is_null());
                assert_eq!(unsafe { (api().result_drop)(failure, &mut error) }, 0);
                return 0;
            }
            // The original child failure stays latched in the original scope.
            return if status == 1 { 0 } else { status };
        }
        match unsafe { observe_budget(context, None) } {
            Ok(usage) => state.after.set(usage),
            Err(status) => return status,
        }
        let mut view = ffi::Bytes {
            data: ptr::null(),
            length: 0,
        };
        assert_eq!(
            unsafe { (api().result_bytes)(result, &mut view, &mut error) },
            0
        );
        let values = wire::decode_values(
            &child,
            wire::FrameKind::Result,
            unsafe { std::slice::from_raw_parts(view.data, view.length as usize) },
            &mut session,
        )
        .unwrap();
        let wire::Value::I64(value) = values.nodes[values.roots[0].0] else {
            panic!("i64 child result")
        };
        assert_eq!(unsafe { (api().result_drop)(result, &mut error) }, 0);
        value
    } else {
        let Some(answer) = value.checked_add(1) else {
            return 2;
        };
        answer
    };
    let encoded = wire::encode_values(
        &signature,
        wire::FrameKind::Result,
        &wire::Values {
            roots: vec![wire::ValueId(0)],
            nodes: vec![wire::Value::I64(answer)],
        },
        &mut session,
    )
    .unwrap();
    let status = unsafe {
        (api().context_buffer_create)(context, encoded.len() as u64, 1, output, &mut error)
    };
    if status != 0 {
        return status;
    }
    let mut view = ffi::MutBytes {
        data: ptr::null_mut(),
        length: 0,
    };
    let status = unsafe { (api().buffer_mut_bytes)(context, *output, &mut view, &mut error) };
    if status != 0 {
        return status;
    }
    unsafe { ptr::copy_nonoverlapping(encoded.as_ptr(), view.data, encoded.len()) };
    0
}

pub unsafe extern "C" fn make(
    user: *mut c_void,
    context: *const ffi::Context,
    input: ffi::Bytes,
    output: *mut *mut ffi::Buffer,
) -> u32 {
    let state = unsafe { &*user.cast::<State>() };
    if state.explicit_drop.replace(false) {
        // The temporary result buffer and its independently owned resource
        // have separate lifetimes. Reuse the real implementation once, then
        // release the buffer before explicitly retiring its actual owner.
        let mut temporary = ptr::null_mut();
        let status = unsafe { make(user, context, input, &mut temporary) };
        state.explicit_drop.set(true);
        if status != 0 {
            return status;
        }
        let mut error: ffi::Error = unsafe { std::mem::zeroed() };
        error.header = header::<ffi::Error>();
        let mut view = ffi::MutBytes {
            data: ptr::null_mut(),
            length: 0,
        };
        assert_eq!(
            unsafe { (api().buffer_mut_bytes)(context, temporary, &mut view, &mut error) },
            0
        );
        assert_eq!(view.length, 49);
        let index = u32::from_le_bytes(
            unsafe { std::slice::from_raw_parts(view.data.add(45), 4) }
                .try_into()
                .unwrap(),
        );
        assert_eq!(
            unsafe { (api().context_buffer_drop)(context, temporary, &mut error) },
            0
        );
        let sentinel = 42usize as *mut ffi::Result;
        let mut retired = sentinel;
        state
            .drop_before
            .set(unsafe { observe_retirement_budget(context, state.drop_reserve.get()) });
        let interrupted = state.interrupt_drop.replace(false);
        if interrupted {
            assert!(!state.control.get().is_null());
            unsafe { (api().interrupt)(state.control.get()) };
        }
        let status =
            unsafe { (api().context_resource_drop)(context, index, &mut retired, &mut error) };
        state
            .drop_after
            .set(unsafe { observe_retirement_budget(context, None) });
        if status == 1 {
            assert!(!retired.is_null() && retired != sentinel);
            assert!(state.drop_report.replace(retired).is_null());
            return 1;
        }
        if status != 0 {
            assert_eq!(retired, sentinel);
            return 1;
        }
        assert!(retired.is_null());
        retired = sentinel;
        assert_eq!(
            unsafe { (api().context_resource_drop)(context, index, &mut retired, &mut error) },
            5
        );
        assert_eq!(retired, sentinel);
        assert_eq!(
            unsafe { (api().context_check)(context, &mut error) },
            u32::from(interrupted)
        );
        if interrupted {
            return 1;
        }
    }
    let result = (|| -> Result<(), u32> {
        if input.length != 45 || input.data.is_null() || output.is_null() {
            return Err(2);
        }
        let mut error: ffi::Error = unsafe { std::mem::zeroed() };
        error.header = header::<ffi::Error>();
        let mut backing = ptr::null_mut();
        let status =
            unsafe { (api().context_buffer_create)(context, 8, 8, &mut backing, &mut error) };
        if status != 0 {
            return Err(status);
        }
        let mut view = ffi::MutBytes {
            data: ptr::null_mut(),
            length: 0,
        };
        let status = unsafe { (api().buffer_mut_bytes)(context, backing, &mut view, &mut error) };
        if status != 0 {
            return Err(status);
        }
        unsafe { std::slice::from_raw_parts_mut(view.data, 8) }
            .copy_from_slice(&41i64.to_le_bytes());
        let address = view.data as usize;
        let mut index = 0;
        let status = unsafe {
            (api().context_resource_publish)(
                context,
                bytes(b"demo:host/api.item"),
                backing,
                &mut index,
                &mut error,
            )
        };
        if status != 0 {
            return Err(status);
        }
        state.created.set(state.created.get() + 1);
        state.live.borrow_mut().push(address);
        let mut result = ptr::null_mut();
        let status =
            unsafe { (api().context_buffer_create)(context, 49, 1, &mut result, &mut error) };
        if status != 0 {
            return Err(status);
        }
        let status = unsafe { (api().buffer_mut_bytes)(context, result, &mut view, &mut error) };
        if status != 0 {
            return Err(status);
        }
        // Existing LCVL-v1 contextual resource vector: preserve the complete
        // checked signature, change to result tag and encode the actual index.
        let frame = unsafe { std::slice::from_raw_parts_mut(view.data, 49) };
        frame[..45].copy_from_slice(unsafe { std::slice::from_raw_parts(input.data, 45) });
        frame[8] = 1;
        frame[41..45].copy_from_slice(&4u32.to_le_bytes());
        frame[45..].copy_from_slice(&index.to_le_bytes());
        unsafe { output.write(result) };
        Ok(())
    })();
    result.map_or_else(|_| 1, |()| 0)
}

pub unsafe extern "C" fn destroy(
    user: *mut c_void,
    context: *const ffi::Context,
    backing: ffi::MutBytes,
    _: *mut *mut ffi::Buffer,
) -> u32 {
    let state = unsafe { &*user.cast::<State>() };
    let mut error: ffi::Error = unsafe { std::mem::zeroed() };
    error.header = header::<ffi::Error>();
    assert_eq!(unsafe { (api().context_check)(context, &mut error) }, 0);
    assert_eq!(backing.length, 8);
    assert_eq!(
        unsafe { std::slice::from_raw_parts(backing.data, 8) },
        &41i64.to_le_bytes()
    );
    let mut live = state.live.borrow_mut();
    let index = live
        .iter()
        .position(|&p| p == backing.data as usize)
        .expect("exact original backing, retired once");
    live.remove(index);
    state.retired.set(state.retired.get() + 1);
    u32::from(state.fail_destroy.get())
}

pub unsafe extern "C" fn release(user: *mut c_void) {
    assert_eq!(RELEASED.fetch_add(1, Ordering::Relaxed), 0);
    drop(unsafe { Box::from_raw(user.cast::<State>()) });
}
