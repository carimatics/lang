//! Private native-host experiment. This exports no component-v1 query/table.
use lang::component_embedding::{self as embedding, Module, ffi, gateway};
use lang_codegen_clif::{c_abi::allocator::ForeignProvider, component, tasks};
use lang_mir::unit as m;
use lang_runtime::{
    allocation as a,
    component::{execution, grants, program, storage, wire},
    execution as e,
};
use std::{ffi::c_void, ptr};

macro_rules! operation {
    ($name:ident,$target:ident,($($arg:ident:$ty:ty),*$(,)?)->$out:ty) => {
        #[unsafe(no_mangle)]
        unsafe extern "C" fn $name($($arg:$ty),*)->$out {
            // SAFETY: the native fixture supplies the exact C signature's
            // live objects/storage through this complete forwarded operation.
            unsafe {gateway::$target($($arg),*)}
        }
    };
}
operation!(probe_engine_create,engine_create,(c:*const ffi::Config,o:*mut *mut ffi::Engine,e:*mut ffi::Error)->u32);
operation!(probe_engine_destroy,engine_destroy,(v:*mut ffi::Engine,e:*mut ffi::Error)->u32);
operation!(probe_module_load,module_load,(v:*mut ffi::Engine,f:u32,p:ffi::Bytes,c:ffi::Bytes,o:*mut *mut ffi::Module,e:*mut ffi::Error)->u32);
operation!(probe_module_drop,module_drop,(v:*mut ffi::Module,e:*mut ffi::Error)->u32);
operation!(probe_resource_publish,resource_publish,(i:*mut ffi::Instance,p:*mut ffi::Provider,k:ffi::Bytes,b:ffi::Bytes,a:u64,o:*mut u32,e:*mut ffi::Error)->u32);
operation!(probe_resource_drop,resource_drop,(i:*mut ffi::Instance,r:u32,o:*mut *mut ffi::Result,e:*mut ffi::Error)->u32);
operation!(probe_context_resource_write,context_resource_write,(c:*const ffi::Context,r:u32,o:*mut ffi::MutBytes,e:*mut ffi::Error)->u32);
operation!(probe_context_resource_read,context_resource_read,(c:*const ffi::Context,r:u32,o:*mut ffi::Bytes,e:*mut ffi::Error)->u32);
operation!(probe_context_resource_publish,context_resource_publish,(c:*const ffi::Context,k:ffi::Bytes,b:*mut ffi::Buffer,o:*mut u32,e:*mut ffi::Error)->u32);
operation!(probe_provider_register,provider_register,(v:*mut ffi::Engine,c:*const ffi::ProviderConfig,o:*mut *mut ffi::Provider,e:*mut ffi::Error)->u32);
operation!(probe_provider_drop,provider_drop,(v:*mut ffi::Provider,o:*mut *mut ffi::Result,e:*mut ffi::Error)->u32);
operation!(probe_context_check,context_check,(c:*const ffi::Context,e:*mut ffi::Error)->u32);
operation!(probe_context_buffer_create,context_buffer_create,(c:*const ffi::Context,n:u64,a:u64,o:*mut *mut ffi::Buffer,e:*mut ffi::Error)->u32);
operation!(probe_context_buffer_drop,context_buffer_drop,(c:*const ffi::Context,b:*mut ffi::Buffer,e:*mut ffi::Error)->u32);
operation!(probe_buffer_mut_bytes,buffer_mut_bytes,(c:*const ffi::Context,b:*mut ffi::Buffer,o:*mut ffi::MutBytes,e:*mut ffi::Error)->u32);
operation!(probe_instance_create,instance_create,(v:*mut ffi::Module,c:*const ffi::InstanceConfig,o:*mut *mut ffi::Instance,e:*mut ffi::Error)->u32);
operation!(probe_instance_close,instance_close,(v:*mut ffi::Instance,o:*mut *mut ffi::Result,e:*mut ffi::Error)->u32);
operation!(probe_bind_export,bind_export,(v:*mut ffi::Instance,p:ffi::Bytes,o:*mut *mut ffi::Binding,e:*mut ffi::Error)->u32);
operation!(probe_binding_drop,binding_drop,(v:*mut ffi::Binding,e:*mut ffi::Error)->u32);
operation!(probe_control_create,control_create,(v:*mut ffi::Engine,c:*const ffi::Limits,o:*mut *mut ffi::Control,e:*mut ffi::Error)->u32);
operation!(probe_control_drop,control_drop,(v:*mut ffi::Control,e:*mut ffi::Error)->u32);
operation!(probe_interrupt,interrupt,(v:*mut ffi::Control)->());
operation!(probe_api_call,call,(b:*mut ffi::Binding,c:*mut ffi::Control,p:ffi::Bytes,o:*mut *mut ffi::Result,e:*mut ffi::Error)->u32);
operation!(probe_context_call,context_call,(c:*const ffi::Context,b:*mut ffi::Binding,p:ffi::Bytes,o:*mut *mut ffi::Result,e:*mut ffi::Error)->u32);
operation!(probe_result_info,result_info,(v:*const ffi::Result,o:*mut ffi::ResultInfo,e:*mut ffi::Error)->u32);
operation!(probe_result_usage,result_usage,(v:*const ffi::Result,o:*mut ffi::Usage,e:*mut ffi::Error)->u32);
operation!(probe_result_bytes,result_bytes,(v:*const ffi::Result,o:*mut ffi::Bytes,e:*mut ffi::Error)->u32);
operation!(probe_result_drop,result_drop,(v:*mut ffi::Result,e:*mut ffi::Error)->u32);

#[unsafe(no_mangle)]
unsafe extern "C" fn probe_context_thread(
    context: *const ffi::Context,
    binding: *mut ffi::Binding,
) -> u32 {
    // SAFETY: the caller joins before its live native callback ends.
    let ffi::Context {
        header,
        engine,
        invocation,
        scope,
        reserved,
    } = unsafe { context.read() };
    let engine = engine as usize;
    let binding = binding as usize;
    std::thread::spawn(move || {
        let context = ffi::Context {
            header,
            engine: engine as *mut ffi::Engine,
            invocation,
            scope,
            reserved,
        };
        // SAFETY: Error v1 contains initialized C integer fields only.
        let mut error: ffi::Error = unsafe { std::mem::zeroed() };
        error.header.size = size_of::<ffi::Error>() as u32;
        error.header.version = 1;
        let sentinel = 42usize as *mut ffi::Result;
        let mut output = sentinel;
        // SAFETY: liveness is retained through join; foreign-thread refusal
        // must precede following any mutable original capability or binding.
        let status = unsafe {
            gateway::context_call(
                &context,
                binding as *mut ffi::Binding,
                ffi::Bytes {
                    data: ptr::null(),
                    length: 0,
                },
                &mut output,
                &mut error,
            )
        };
        if output == sentinel { status } else { u32::MAX }
    })
    .join()
    .unwrap_or(u32::MAX)
}

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

#[unsafe(no_mangle)]
unsafe extern "C" fn probe_report(bytes: ffi::Bytes, mode: u32, case: u32) -> u32 {
    // SAFETY: the native caller retains this exact immutable result view.
    let bytes = unsafe { input(bytes.data, bytes.length) }.unwrap();
    let report = wire::decode_report(bytes, &mut wire::Session::new(Default::default())).unwrap();
    assert_eq!(report.policy, wire::MessagePolicy::Redacted);
    assert!(report.primary.message.is_none());
    assert_eq!(report.poisoned, matches!(case, 4 | 5 | 7 | 8 | 9));
    let origin = report.primary.origin.as_ref().unwrap();
    assert_eq!(
        origin.mode,
        if case == 10 {
            wire::Mode::Vm
        } else if matches!(case, 7 | 8) {
            wire::Mode::Host
        } else if mode == 1 {
            wire::Mode::Jit
        } else {
            wire::Mode::Vm
        }
    );
    assert_ne!(origin.digest, [0; 32]);
    assert!(origin.span.is_some());
    match case {
        0 => {
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Panic);
            assert!(report.primary.limit.is_none());
        }
        1 => {
            let limit = report.primary.limit.unwrap();
            assert_eq!(limit.kind, wire::ReportLimitKind::Fuel);
            assert_eq!((limit.limit, limit.attempted), (0, 1));
        }
        2 => {
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Interrupted);
            assert!(report.primary.limit.is_none());
        }
        3 => {
            let limit = report.primary.limit.unwrap();
            assert_eq!(limit.kind, wire::ReportLimitKind::ServiceWork);
            assert_eq!(limit.limit, 0);
            assert!(limit.attempted > 0);
        }
        4 | 6 => {
            assert_eq!(report.primary.category, wire::Category::Limit);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
            let limit = report.primary.limit.unwrap();
            assert_eq!(limit.kind, wire::ReportLimitKind::CanonicalIndices);
            assert_eq!((limit.limit, limit.attempted), (3, 4));
            assert_eq!(report.suppressed.len(), usize::from(case == 4));
            for record in &report.suppressed {
                assert_eq!(record.category, wire::Category::Destructor);
                assert_eq!(record.phase, wire::Phase::Cleanup);
                assert_eq!(record.diagnostic, wire::Diagnostic::Destructor);
                assert!(record.message.is_none() && record.limit.is_none());
                assert_eq!(record.origin, report.primary.origin);
            }
            assert_eq!(report.usage.host_calls, 3);
            assert_eq!(report.usage.canonical_indices, 3);
        }
        5 => {
            assert_eq!(report.primary.category, wire::Category::Admission);
            assert_eq!(report.primary.phase, wire::Phase::Admission);
            assert_eq!(
                report.primary.diagnostic,
                wire::Diagnostic::InstancePoisoned
            );
            assert!(report.primary.limit.is_none());
        }
        7 | 8 => {
            assert_eq!(report.primary.category, wire::Category::HostContract);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::HostContract);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(origin.digest, [17; 32]);
            assert_eq!(origin.member, 9);
            assert_eq!(
                report.suppressed.len(),
                1,
                "native report case {case}: {report:?}"
            );
            assert_eq!(
                report.suppressed[0].diagnostic,
                wire::Diagnostic::Destructor
            );
            assert!(report.suppressed[0].message.is_none());
            assert_eq!(report.omitted, 7, "native report case {case}: {report:?}");
            assert_eq!(
                report.frames.len(),
                1,
                "native report case {case}: {report:?}"
            );
            assert_eq!(
                report.usage.host_calls,
                if case == 7 { 1 } else { 2 },
                "native report case {case}: {report:?}"
            );
            assert_eq!(report.usage.canonical_indices, 0);
            assert_ne!(report.usage.fuel, 9999);
            assert_ne!(report.usage.peak_heap_bytes, 9999);
        }
        9 => {
            assert_eq!(report.primary.category, wire::Category::HostContract);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::HostContract);
            assert!(report.primary.limit.is_none() && report.suppressed.is_empty());
            assert_eq!(report.omitted, 0);
            assert_eq!(report.frames.len(), 1);
            assert_eq!(report.usage.host_calls, 1);
        }
        10 => {
            assert_eq!(report.primary.category, wire::Category::Panic);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Panic);
            assert_eq!(origin.digest, [17; 32]);
            assert!(report.primary.limit.is_none() && report.suppressed.is_empty());
            assert_eq!(report.omitted, 0);
            assert_eq!(report.frames.len(), 1);
            assert_eq!(report.usage.host_calls, 1);
        }
        _ => panic!("unknown report case"),
    }
    0
}

#[unsafe(no_mangle)]
unsafe extern "C" fn probe_native_failure_report(
    context: *const ffi::Context,
    output: *mut *mut ffi::Buffer,
    recoverable: u32,
) -> u32 {
    let origin = wire::Frame {
        mode: if recoverable == 1 {
            wire::Mode::Vm
        } else {
            wire::Mode::Host
        },
        digest: [17; 32],
        member: 9,
        span: Some(m::Span {
            source: m::SourceId(0),
            start: 3,
            end: 11,
        }),
    };
    let mut report = wire::Report {
        policy: wire::MessagePolicy::Sanitized,
        primary: wire::Record {
            category: wire::Category::HostContract,
            phase: wire::Phase::Call,
            diagnostic: wire::Diagnostic::HostContract,
            message: Some("native private primary".into()),
            limit: None,
            origin: Some(origin.clone()),
        },
        suppressed: vec![wire::Record {
            category: wire::Category::Destructor,
            phase: wire::Phase::Cleanup,
            diagnostic: wire::Diagnostic::Destructor,
            message: Some("native private cleanup".into()),
            limit: None,
            origin: Some(origin.clone()),
        }],
        poisoned: true,
        omitted: 7,
        usage: wire::ReportUsage {
            fuel: 9999,
            service_work: 9999,
            host_calls: 9999,
            peak_heap_bytes: 9999,
            peak_live_allocations: 9999,
            peak_frames: 9999,
            peak_live_cells: 9999,
            peak_live_tasks: 9999,
            canonical_indices: 9999,
        },
        frames: vec![origin],
    };
    if recoverable == 1 {
        report.primary.category = wire::Category::Panic;
        report.primary.diagnostic = wire::Diagnostic::Panic;
        report.suppressed.clear();
        report.poisoned = false;
        report.omitted = 0;
    }
    let bytes = wire::encode_report(&report, &mut wire::Session::new(Default::default())).unwrap();
    let mut error = ffi::Error {
        header: ffi::Header {
            size: size_of::<ffi::Error>() as u32,
            version: 1,
        },
        diagnostic: 0,
        limit_kind: 0,
        limit: 0,
        attempted: 0,
        input_digest: [0; 32],
        source_id: 0,
        start: 0,
        end: 0,
        detail_code: 0,
        flags: 0,
        reserved0: 0,
        reserved: [0; 2],
    };
    // SAFETY: the fixture retains the exact live callback and initialized output slot.
    let status = unsafe {
        gateway::context_buffer_create(context, bytes.len() as u64, 1, output, &mut error)
    };
    if status != 0 {
        return status;
    }
    let mut view = ffi::MutBytes {
        data: ptr::null_mut(),
        length: 0,
    };
    // SAFETY: the successful creation published this live context buffer.
    let status = unsafe { gateway::buffer_mut_bytes(context, *output, &mut view, &mut error) };
    if status != 0 || view.length != bytes.len() as u64 {
        return 1;
    }
    // SAFETY: the checked view is uniquely writable for exactly the requested length.
    unsafe { std::slice::from_raw_parts_mut(view.data, bytes.len()) }.copy_from_slice(&bytes);
    1
}

#[unsafe(no_mangle)]
unsafe extern "C" fn probe_engine_lifecycle(
    config: *const ffi::Config,
    format: u32,
    code: ffi::Bytes,
    idl: ffi::Bytes,
) -> u32 {
    let mut error = ffi::Error {
        header: ffi::Header {
            size: std::mem::size_of::<ffi::Error>() as u32,
            version: 1,
        },
        diagnostic: 0,
        limit_kind: 0,
        limit: 0,
        attempted: 0,
        input_digest: [0; 32],
        source_id: 0,
        start: 0,
        end: 0,
        detail_code: 0,
        flags: 0,
        reserved0: 0,
        reserved: [0; 2],
    };
    let sentinel = ptr::without_provenance_mut(42);
    let mut engine = sentinel;
    // SAFETY: C retains the complete initialized config, byte views and
    // original allocator owner through this complete native lifecycle.
    assert_eq!(
        unsafe { gateway::engine_create(config, &mut engine, &mut error) },
        0
    );
    assert_ne!(engine, sentinel);
    let mut module = sentinel.cast::<ffi::Module>();
    // SAFETY: this exact live engine and views remain valid until return.
    assert_eq!(
        unsafe { gateway::module_load(engine, 99, code, idl, &mut module, &mut error) },
        2
    );
    assert_eq!(module, sentinel.cast());
    // SAFETY: complete checked module preparation snapshots its views.
    assert_eq!(
        unsafe { gateway::module_load(engine, format, code, idl, &mut module, &mut error) },
        0
    );
    // SAFETY: the engine remains live while a dependent module exists.
    assert_eq!(unsafe { gateway::engine_destroy(engine, &mut error) }, 4);
    // SAFETY: close each exact live opaque handle once, in dependency order.
    assert_eq!(unsafe { gateway::module_drop(module, &mut error) }, 0);
    // SAFETY: all dependents closed; original allocator callbacks remain live.
    assert_eq!(unsafe { gateway::engine_destroy(engine, &mut error) }, 0);
    // SAFETY: a copied initialized descriptor is valid for this test.
    let mut unavailable = unsafe { *config };
    unavailable.mode = 2;
    let mut untouched = sentinel;
    // SAFETY: unsupported mode must refuse without output publication.
    assert_eq!(
        unsafe { gateway::engine_create(&unavailable, &mut untouched, &mut error) },
        3
    );
    assert_eq!(untouched, sentinel);
    0
}
fn main() {
    // SAFETY: the linked C fixture performs its complete native host lifecycle.
    assert_eq!(unsafe { fixture() }, 0);
}
