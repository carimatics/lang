//! Private native-host experiment. This exports no component-v1 query/table.
use lang::component_embedding::{self as embedding, Module, ffi};
use lang_codegen_clif::{c_abi::allocator::ForeignProvider, component, tasks};
use lang_mir::unit as m;
use lang_runtime::{
    allocation as a,
    component::{execution, grants, program, storage, wire},
    execution as e,
};
use std::{ffi::c_void, ptr};

struct Probe {
    module: Module,
    allocator: a::Allocator<ForeignProvider<'static>>,
    values: storage::Resources<'static>,
    state: execution::InstanceState,
    bridge: component::Engine,
}
unsafe fn input<'a>(data: *const u8, length: u64) -> Option<&'a [u8]> {
    let length = usize::try_from(length).ok()?;
    if length > isize::MAX as usize || (length != 0 && data.is_null()) {
        return None;
    }
    if length == 0 {
        return Some(&[]);
    }
    // SAFETY: the native fixture supplies this entire readable range for the
    // call. The loader snapshots it and retains no foreign input reference.
    Some(unsafe { std::slice::from_raw_parts(data, length) })
}
#[unsafe(no_mangle)]
unsafe extern "C" fn probe_load(
    format: u32,
    code: *const u8,
    code_length: u64,
    idl: *const u8,
    idl_length: u64,
    allocator: ffi::Allocator,
) -> *mut c_void {
    if allocator.header.size != std::mem::size_of::<ffi::Allocator>() as u32
        || allocator.header.version != 1
        || allocator.reserved != [0; 2]
    {
        return ptr::null_mut();
    }
    let (Some(allocate), Some(deallocate)) = (allocator.allocate, allocator.deallocate) else {
        return ptr::null_mut();
    };
    // SAFETY: fixture input arrays remain live through this call.
    let Some(code) = (unsafe { input(code, code_length) }) else {
        return ptr::null_mut();
    };
    // SAFETY: the same input-range contract holds for IDL.
    let Some(idl) = (unsafe { input(idl, idl_length) }) else {
        return ptr::null_mut();
    };
    let Ok(module) = embedding::load(format, code, idl, Default::default()) else {
        return ptr::null_mut();
    };
    // SAFETY: the C owner and callbacks remain live through probe_drop and all
    // its original allocations. No scalar context or replacement allocator.
    let provider = unsafe { ForeignProvider::component(allocate, deallocate, allocator.user) };
    let mut allocator = a::Allocator::new(provider, Default::default()).unwrap();
    // Exercise the actual native allocator contract before driving the roots.
    let allocation = allocator
        .allocate(a::Request::new(13, 16).unwrap())
        .unwrap();
    assert_eq!(allocator.bytes(allocation).unwrap(), &[0; 13]);
    allocator.release(allocation).unwrap();
    let mut budget = m::Budget::new(Default::default());
    let mut values = storage::Resources::prepare(&allocator, 4, 4, &mut budget).unwrap();
    let state = execution::InstanceState::prepare(&mut values).unwrap();
    let bridge = component::Engine::prepare(&mut budget).unwrap();
    Box::into_raw(Box::new(Probe {
        module,
        allocator,
        values,
        state,
        bridge,
    }))
    .cast()
}
#[unsafe(no_mangle)]
unsafe extern "C" fn probe_call(
    probe: *mut c_void,
    mode: u32,
    arguments: *const u8,
    arguments_length: u64,
    output: *mut u8,
    capacity: u64,
    length: *mut u64,
) -> u32 {
    if probe.is_null() || output.is_null() || length.is_null() || mode > 1 {
        return 2;
    }
    // SAFETY: the fixture passes the unique live handle and complete input.
    let probe = unsafe { &mut *probe.cast::<Probe>() };
    // SAFETY: argument storage remains readable through this call.
    let Some(arguments) = (unsafe { input(arguments, arguments_length) }) else {
        return 2;
    };
    let Probe {
        module,
        allocator,
        values,
        state,
        bridge,
    } = probe;
    let mut session = wire::Session::new(Default::default());
    let providers = [];
    let bound = grants::prepare(
        module.unit(),
        module.contract(),
        &[],
        || &providers,
        &mut session,
    )
    .unwrap();
    let program = program::Program::prepare(&bound, Default::default()).unwrap();
    let entry = module.unit().exports()[0].0;
    let control = e::Control::default().and_service_work(Some(1_000_000));
    let mut invoker = bridge.begin(control).unwrap();
    let mut budget = m::Budget::new(Default::default());
    state
        .with_program(&program, |instance| {
            let mut call = execution::Invocation::prepare_owned(
                instance,
                entry,
                values,
                &mut invoker,
                &mut budget,
            )
            .unwrap();
            let arguments = call
                .decode_arguments(arguments, &[], allocator, &mut session)
                .unwrap();
            call.prepare_output(&mut session).unwrap();
            let outcome = if mode == 1 {
                tasks::CompiledProgram::compile_component(&program, entry, Default::default())
                    .unwrap()
                    .execute_component(
                        allocator,
                        control,
                        Default::default(),
                        &arguments,
                        &mut call,
                    )
            } else {
                lang_vm::unit::tasks::execute_component(
                    &program,
                    entry,
                    allocator,
                    control,
                    Default::default(),
                    &arguments,
                    &mut call,
                )
            }
            .unwrap();
            let execution::Value::Frame(frame) = outcome.value else {
                return 2;
            };
            if frame.bytes().len() as u64 > capacity {
                return 2;
            }
            // SAFETY: the fixture supplies this writable output capacity and a
            // disjoint initialized length slot; the frame is still borrowed live.
            unsafe {
                ptr::copy_nonoverlapping(frame.bytes().as_ptr(), output, frame.bytes().len());
                length.write(frame.bytes().len() as u64);
            }
            0
        })
        .unwrap()
}
#[unsafe(no_mangle)]
unsafe extern "C" fn probe_drop(probe: *mut c_void) {
    // SAFETY: the fixture relinquishes this exact unique live handle once,
    // after every call; the native allocator owner remains live until return.
    let probe = unsafe { Box::from_raw(probe.cast::<Probe>()) };
    assert!(!probe.state.poisoned());
    assert!(probe.values.is_empty());
    drop(probe);
}
unsafe extern "C" {
    fn fixture() -> u32;
}
fn main() {
    // SAFETY: the linked C fixture performs its complete native host lifecycle.
    assert_eq!(unsafe { fixture() }, 0);
}
