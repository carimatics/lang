//! Native-host fixtures route operations through the public component-v1 table.
use lang::component_embedding::{self as embedding, Module, ffi, gateway};
use lang_codegen_clif::{c_abi::allocator::ForeignProvider, component, tasks};
use lang_mir::unit as m;
use lang_runtime::{
    allocation as a,
    component::{execution, grants, program, storage, wire},
    execution as e,
};
use std::{ffi::c_void, ptr};
const RESOURCE_PROVIDER_DIGEST: [u8; 32] = [0; 32];
const REGISTRY_PROVIDER_DIGEST: [u8; 32] = [0; 32];
const REGISTRY_STACK_DIGESTS: [[u8; 32]; 6] = [[0; 32]; 6];
const REGISTRY_ROOT_DIGESTS: [[u8; 32]; 2] = [[0; 32]; 2];
const REGISTRY_MIXED_SPANS: [[u32; 2]; 5] = [[0; 2]; 5];
const RESOURCE_RUN_MEMBER: u32 = 0;
thread_local! { static CALLBACK_SNAPSHOT_WORK: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }
#[unsafe(no_mangle)]
unsafe extern "C" fn probe_exhaust_preparation(context: *const ffi::Context, kind: u32) -> u32 {
    struct Spend(u32);
    impl execution::CallbackOperation for Spend {
        fn run<'code>(
            &mut self,
            instance: &mut execution::Instance<'_, '_>,
            _: &mut storage::Resources<'code>,
            _: &mut dyn storage::Memory,
            budget: &mut m::Budget,
            _: &mut wire::Session,
            _: &execution::CallbackArguments<'_, 'code>,
            _: storage::Holder,
            _: &e::InvocationAccounting,
            _: &mut lang_runtime::task::CurrentTask<'_>,
        ) -> Result<(), execution::CallbackFailure> {
            let snapshot = instance
                .program()
                .bindings()
                .unit()
                .document()
                .usage()
                .budget;
            CALLBACK_SNAPSHOT_WORK.with(|cell| {
                cell.set(
                    snapshot
                        .work
                        .saturating_add(snapshot.allocation_bytes)
                        .saturating_add(1) as u64,
                )
            });
            let result = (|| -> Result<(), u32> {
                match self.0 {
                    0 => budget.charge(budget.limits().work.saturating_sub(budget.work_used())),
                    1 => {
                        let remaining = budget
                            .limits()
                            .allocations
                            .saturating_sub(budget.allocation_bytes());
                        // Charge metadata without allocating physical test payloads.
                        budget
                            .allocate::<[u8; 1024]>(remaining / 1024)
                            .map_err(|_| 2u32)?;
                        budget.allocate::<u8>(remaining % 1024)
                    }
                    2 => budget.allocate::<()>(
                        budget
                            .limits()
                            .elements
                            .saturating_sub(budget.elements_used()),
                    ),
                    _ => return Err(2),
                }
                .map_err(|_| 2)
            })();
            result.map_err(execution::CallbackFailure::Admission)
        }
    }
    // SAFETY: the fixture retains the actual live native callback. Only its
    // original preparation ledger is spent; no guest body or resource runs.
    match unsafe {
        component::with_child(context, |_, call, _| call.with_callback(&mut Spend(kind)))
    } {
        Ok(()) => 0,
        Err(status) => status,
    }
}
const RESOURCE_MODULE_DIGESTS: [[u8; 32]; 2] = [[0; 32]; 2];

#[unsafe(no_mangle)]
unsafe extern "C" fn probe_exhaust_resource_preparation(context: *const ffi::Context, kind: u32) -> u32 {
    struct Spend(u32);
    impl execution::StorageOperation for Spend {
        fn run(
            &mut self,
            _: &mut storage::Resources<'_>,
            _: &mut dyn storage::Memory,
            budget: &mut m::Budget,
            _: storage::Holder,
            _: &e::InvocationAccounting,
        ) -> Result<(), u32> {
            match self.0 {
                0 => budget.charge(budget.limits().work.saturating_sub(budget.work_used())),
                1 => {
                    let remaining = budget.limits().allocations.saturating_sub(budget.allocation_bytes());
                    budget.allocate::<[u8; 1024]>(remaining / 1024).map_err(|_| 2u32)?;
                    budget.allocate::<u8>(remaining % 1024)
                }
                2 => budget.allocate::<()>(budget.limits().elements.saturating_sub(budget.elements_used())),
                _ => return Err(2),
            }.map_err(|_| 2)
        }
    }
    // SAFETY: the fixture keeps the checked native scope live through return;
    // this spends its original ledger without a guest child or replacement.
    match unsafe { component::with_resource_context(context, |call| call.with_storage(&mut Spend(kind))) } {
        Ok(()) => 0,
        Err(status) => status,
    }
}

fn assert_resource_guest_frame(frame: &wire::Frame, mode: u32, member: u32) {
    assert_eq!(
        frame.mode,
        if mode == 1 {
            wire::Mode::Jit
        } else {
            wire::Mode::Vm
        }
    );
    assert!(RESOURCE_MODULE_DIGESTS.contains(&frame.digest));
    assert_eq!(frame.member, member);
    assert!(frame.span.is_some());
}

fn assert_resource_cleanup_origin(record: &wire::Record) {
    let origin = record
        .origin
        .as_ref()
        .expect("actual registered destructor origin");
    assert_eq!(origin.mode, wire::Mode::Host);
    assert_eq!(origin.digest, RESOURCE_PROVIDER_DIGEST);
    assert_eq!(origin.member, 1);
    assert!(origin.span.is_none());
}

fn api() -> &'static ffi::Api {
    let mut error: ffi::Error = unsafe { std::mem::zeroed() };
    error.header = ffi::Header {size: size_of::<ffi::Error>() as u32, version: 1};
    let mut output = ptr::null();
    // SAFETY: exact initialized local outputs receive the immutable SDK table.
    assert_eq!(unsafe {gateway::lang_component_v1_get_api(1,size_of::<ffi::Api>() as u32,&mut output,&mut error)},0);
    unsafe { &*output }
}
macro_rules! operation {
    ($name:ident,$target:ident,($($arg:ident:$ty:ty),*$(,)?)->$out:ty) => {
        #[unsafe(no_mangle)]
        unsafe extern "C" fn $name($($arg:$ty),*)->$out {
            // SAFETY: the native fixture supplies the exact C signature's
            // live objects/storage through this complete forwarded operation.
            unsafe {(api().$target)($($arg),*)}
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
operation!(probe_context_resource_keep,context_resource_keep,(c:*const ffi::Context,r:u32,o:*mut u32,e:*mut ffi::Error)->u32);
operation!(probe_context_resource_take,context_resource_take,(c:*const ffi::Context,r:u32,o:*mut u32,e:*mut ffi::Error)->u32);
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
        let mut resource_index = 42;
        // SAFETY: the original native scope remains live on its owner thread;
        // both resource operations must refuse before original state access.
        let kept = unsafe { gateway::context_resource_keep(&context, 1, &mut resource_index, &mut error) };
        let taken = unsafe { gateway::context_resource_take(&context, 1, &mut resource_index, &mut error) };
        assert_ne!(kept, 0);
        assert_ne!(taken, 0);
        assert_eq!(resource_index, 42);
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
unsafe extern "C" fn probe_task_report(bytes: ffi::Bytes, mode: u32, kind: u32, ceiling: u64) -> u32 {
    // SAFETY: the C host retains this immutable result view until return.
    let bytes = unsafe { input(bytes.data, bytes.length) }.unwrap();
    let report = wire::decode_report(bytes, &mut wire::Session::new(Default::default())).unwrap();
    assert!(!report.poisoned && report.suppressed.is_empty());
    assert_eq!(report.policy, wire::MessagePolicy::Redacted);
    assert!(report.primary.message.is_none());
    assert_eq!(report.primary.phase, wire::Phase::Call);
    if kind == 1 {
        assert_eq!(report.primary.category, wire::Category::Interruption);
        assert_eq!(report.primary.diagnostic, wire::Diagnostic::Interrupted);
        assert!(report.primary.limit.is_none());
    } else {
        assert_eq!(report.primary.category, wire::Category::Limit);
        assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
        let limit = report.primary.limit.unwrap();
        assert_eq!(limit.kind, if kind == 0 {wire::ReportLimitKind::LiveTasks} else {wire::ReportLimitKind::Fuel});
        assert_eq!((limit.limit, limit.attempted), (ceiling, ceiling + 1));
    }
    if let Some(origin) = report.primary.origin {
        assert_eq!(origin.mode, if mode == 1 {wire::Mode::Jit} else {wire::Mode::Vm});
    }
    0
}

#[unsafe(no_mangle)]
unsafe extern "C" fn probe_scope_limit(bytes: ffi::Bytes, kind: u32) -> u32 {
    // SAFETY: the C test retains this exact immutable result view.
    let bytes = unsafe { input(bytes.data, bytes.length) }.unwrap();
    let report = wire::decode_report(bytes, &mut wire::Session::new(Default::default())).unwrap();
    assert!(!report.poisoned);
    assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
    let limit = report.primary.limit.unwrap();
    assert_eq!(limit.kind as u32, kind);
    assert_eq!(limit.limit, 0);
    assert!(limit.attempted > 0);
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn probe_report(bytes: ffi::Bytes, mode: u32, case: u32) -> u32 {
    // SAFETY: the native caller retains this exact immutable result view.
    let bytes = unsafe { input(bytes.data, bytes.length) }.unwrap();
    let report = wire::decode_report(bytes, &mut wire::Session::new(Default::default())).unwrap();
    assert_eq!(report.policy, wire::MessagePolicy::Redacted);
    assert!(report.primary.message.is_none());
    if (75..=80).contains(&case) {
        let outer=case%2==1;
        let native=case==77||case==78;
        let capped=case>=79;
        let guest=match mode {0=>wire::Mode::Vm,1=>wire::Mode::Jit,2=>wire::Mode::Aot,_=>panic!("mode")};
        let offset=if outer {2}else{0};
        assert!(!report.poisoned);
        assert_eq!(report.primary.category,if native {wire::Category::Trap}else{wire::Category::Panic});
        assert_eq!(report.primary.phase,wire::Phase::Call);
        assert!(report.primary.limit.is_none()&&report.suppressed.is_empty());
        assert_eq!(report.primary.diagnostic,if native {wire::Diagnostic::Trap}else{wire::Diagnostic::Panic});
        if !capped {assert_eq!(report.primary.origin.as_ref(),report.frames.last());}
        else {assert_ne!(report.primary.origin.as_ref(),report.frames.last());}
        assert_eq!(report.frames.len(),if capped {3}else{offset+if native {2}else{4}});
        assert_eq!(report.omitted,if capped {if outer {3}else{1}}else{u64::from(native)});
        assert_eq!(report.usage.host_calls,2);
        let child=report.frames[offset].digest;
        let format=usize::from(child!=REGISTRY_STACK_DIGESTS[4]);
        assert_eq!(child,REGISTRY_STACK_DIGESTS[4+format]);
        if !native {
            let origin=report.primary.origin.as_ref().unwrap();
            assert_eq!(origin.mode,guest);assert_eq!(origin.digest,child);assert_eq!(origin.member,2);
            let span=origin.span.unwrap();assert_eq!(span.source,m::SourceId(0));
            assert_eq!([span.start,span.end],REGISTRY_MIXED_SPANS[3]);
        }
        for (i,frame) in report.frames[offset..].iter().take(if native {1}else{4}).enumerate() {
            assert_eq!(frame.mode,guest);assert_eq!(frame.digest,child);
            assert_eq!(frame.member,if i==0 {0}else{2});assert!(frame.span.is_some());
            let span=frame.span.unwrap();assert_eq!(span.source,m::SourceId(0));
            assert_eq!([span.start,span.end],REGISTRY_MIXED_SPANS[if native {4}else if i==0 {1}else if i<3 {2}else{3}]);
        }
        if native {
            let host=&report.frames[offset+1];
            assert_eq!(host.mode,wire::Mode::Host);assert_eq!(host.digest,REGISTRY_PROVIDER_DIGEST);
            assert_eq!(host.member,0);assert!(host.span.is_none());
            assert!(report.frames.iter().all(|frame|frame.digest!=[17;32]));
            if outer {assert_eq!(report.frames[1],*host);}
        }
        if outer {
            assert_eq!(report.frames[0].mode,guest);
            assert_eq!(report.frames[0].digest,REGISTRY_ROOT_DIGESTS[format]);
            assert_ne!(REGISTRY_ROOT_DIGESTS[format],child);
            let span=report.frames[0].span.unwrap();assert_eq!(span.source,m::SourceId(0));
            assert_eq!([span.start,span.end],REGISTRY_MIXED_SPANS[0]);
            assert_eq!(report.frames[1].mode,wire::Mode::Host);
            assert_eq!(report.frames[1].digest,REGISTRY_PROVIDER_DIGEST);
            assert_eq!(report.frames[1].member,0);assert!(report.frames[1].span.is_none());
        }
        return 0;
    }
    if matches!(case, 28 | 29 | 30 | 35 | 36) {
        assert!(report.poisoned);
        assert_eq!(
            report.primary.category,
            if matches!(case, 28 | 30) {
                wire::Category::HostContract
            } else {
                wire::Category::Destructor
            }
        );
        assert_eq!(report.primary.phase, wire::Phase::Cleanup);
        assert_eq!(
            report.primary.diagnostic,
            if matches!(case, 28 | 30) {
                wire::Diagnostic::HostContract
            } else {
                wire::Diagnostic::Destructor
            }
        );
        assert_resource_cleanup_origin(&report.primary);
        if case == 35 {
            assert_eq!(report.suppressed.len(), 2);
            assert_eq!(
                report.suppressed[0].diagnostic,
                wire::Diagnostic::HostContract
            );
            assert_eq!(
                report.suppressed[1].diagnostic,
                wire::Diagnostic::Destructor
            );
            for record in &report.suppressed {
                assert_eq!(record.phase, wire::Phase::Cleanup);
                assert!(record.message.is_none() && record.limit.is_none());
                assert_resource_cleanup_origin(record);
            }
        } else {
            assert!(report.suppressed.is_empty());
        }
        assert_eq!(
            report.omitted,
            if case == 36 {
                u64::MAX
            } else {
                u64::from(case != 30)
            }
        );
        assert_eq!(report.usage.host_calls, 1);
        assert_eq!(report.frames.len(), 1);
        assert_resource_guest_frame(&report.frames[0], mode, 2);
        assert!(report.frames.iter().all(|f| f.digest != [17; 32]));
        return 0;
    }
    if matches!(case, 31 | 32 | 33 | 37) {
        assert!(report.poisoned);
        assert_eq!(
            report.primary.category,
            if case == 33 {
                wire::Category::Interruption
            } else {
                wire::Category::Panic
            }
        );
        assert_eq!(
            report.primary.diagnostic,
            if case == 33 {
                wire::Diagnostic::Interrupted
            } else {
                wire::Diagnostic::Panic
            }
        );
        assert_eq!(report.primary.phase, wire::Phase::Call);
        let origin = report.primary.origin.as_ref().unwrap();
        // Generated source bodies are ordered explode, relay, retire, run.
        assert_resource_guest_frame(origin, mode, if case == 33 { 2 } else { 0 });
        assert_eq!(
            origin.mode,
            if mode == 1 {
                wire::Mode::Jit
            } else {
                wire::Mode::Vm
            }
        );
        assert_ne!(origin.digest, [17; 32]);
        assert!(origin.span.is_some());
        assert_eq!(report.suppressed.len(), 1);
        let record = &report.suppressed[0];
        assert_eq!(
            record.category,
            if matches!(case, 32 | 37) {
                wire::Category::HostContract
            } else {
                wire::Category::Destructor
            }
        );
        assert_eq!(record.phase, wire::Phase::Cleanup);
        assert!(record.message.is_none() && record.limit.is_none());
        assert_resource_cleanup_origin(record);
        assert_eq!(report.omitted, u64::from(case != 37));
        assert_eq!(report.usage.host_calls, 1);
        assert_eq!(report.frames.len(), if case==33 {2}else{1}, "resource parent case {case}: {report:?}");
        assert_resource_guest_frame(&report.frames[0], mode, if case == 33 { 2 } else { 0 });
        if case==33 {
            assert_eq!(report.frames[1].mode,wire::Mode::Host);
            assert_eq!(report.frames[1].digest,RESOURCE_PROVIDER_DIGEST);
            assert_eq!(report.frames[1].member,0);assert!(report.frames[1].span.is_none());
        }
        assert!(report.frames.iter().all(|f| f.digest != [17; 32]));
        return 0;
    }
    if matches!(case, 14 | 15) {
        assert!(report.poisoned);
        assert_eq!(report.primary.category, wire::Category::Destructor);
        assert_eq!(report.primary.phase, wire::Phase::Cleanup);
        assert_eq!(report.primary.diagnostic, wire::Diagnostic::Destructor);
        assert_resource_cleanup_origin(&report.primary);
        assert!(report.primary.limit.is_none());
        assert_eq!(report.suppressed.len(), usize::from(case == 15));
        for record in &report.suppressed {
            assert_eq!(record.category, wire::Category::Destructor);
            assert_eq!(record.phase, wire::Phase::Cleanup);
            assert_resource_cleanup_origin(record);
            assert!(record.message.is_none() && record.limit.is_none());
        }
        assert!(report.frames.is_empty());
        assert_eq!(report.omitted, 0);
        return 0;
    }
    if (70..=72).contains(&case) {
        assert_eq!(report.poisoned,case!=71);
        assert_eq!(report.primary.category,if case==71 {wire::Category::Interruption}else{wire::Category::HostContract});
        assert_eq!(report.primary.diagnostic,if case==71 {wire::Diagnostic::Interrupted}else{wire::Diagnostic::HostContract});
        assert_eq!(report.primary.phase,wire::Phase::Call);
        assert!(report.primary.limit.is_none()&&report.suppressed.is_empty());
        let origin=report.primary.origin.as_ref().unwrap();
        assert_eq!(origin.mode,if mode==1 {wire::Mode::Jit}else{wire::Mode::Vm});
        assert_eq!(report.omitted,0);
        assert_eq!(report.frames.len(),2);
        assert_eq!(&report.frames[0],origin);
        let host=&report.frames[1];
        assert_eq!(host.mode,wire::Mode::Host);assert_eq!(host.digest,REGISTRY_PROVIDER_DIGEST);
        assert_eq!(host.member,0);assert!(host.span.is_none());
        assert_eq!(report.usage.host_calls,1);assert_eq!(report.usage.canonical_indices,0);
        return 0;
    }
    if case==99 {eprintln!("deep refusal: {:?}",report.primary);return 0;}
    if (60..=64).contains(&case) {
        assert!(!report.poisoned && report.suppressed.is_empty());
        assert_eq!(report.primary.phase,wire::Phase::Call);
        if case==63 {
            assert_eq!(report.primary.category,wire::Category::Panic);
            assert!(report.primary.limit.is_none());
        } else {
            assert_eq!(report.primary.category,wire::Category::Limit);
            assert_eq!(report.primary.limit,Some(wire::LimitDetail {
            kind:wire::ReportLimitKind::Frames,
            limit:if case==62 {72}else{4},attempted:if case==62 {73}else{5},
            }));
        }
        let origin=report.primary.origin.as_ref().unwrap();
        assert_eq!(origin.mode,if mode==1 {wire::Mode::Jit}else{wire::Mode::Vm});
        assert!(REGISTRY_STACK_DIGESTS.contains(&origin.digest));
        assert_eq!(origin.member, if case==63 {2}else{1});
        assert_eq!(report.frames.len(), if case==60||case==63 {4} else if case==61 {2} else if case==64 {0} else {64});
        assert_eq!(report.omitted, if case==60||case==63 {0} else if case==61 {2} else if case==64 {4} else {8});
        for (i,frame) in report.frames.iter().enumerate() {
            assert_eq!((frame.mode,frame.digest),(origin.mode,origin.digest));
            assert_eq!(frame.member,if i==0 {0}else{origin.member});
            assert!(frame.span.is_some());
        }
        if case==60||case==63 {
            assert_eq!(report.frames[1],report.frames[2]);
            assert_eq!(&report.frames[3],origin);
        }
        assert_eq!(report.usage.host_calls,1);
        return 0;
    }
    if matches!(case, 57 | 58 | 59) {
        assert!(!report.poisoned && report.suppressed.is_empty());
        assert_eq!(report.primary.category, wire::Category::Limit);
        assert_eq!(report.primary.phase, wire::Phase::Call);
        assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
        assert_eq!(report.primary.limit, Some(wire::LimitDetail { kind: wire::ReportLimitKind::PreparationWork, limit: 10_000_000, attempted: 10_000_001 }));
        let origin=report.primary.origin.as_ref().unwrap();
        assert!(REGISTRY_STACK_DIGESTS.contains(&origin.digest));
        assert_eq!(origin.member, 1);
        assert!(origin.span.is_some());
        assert_eq!(report.frames.len(), if case==57 { 5 } else if case==58 { 3 } else { 64 });
        assert_eq!(report.omitted, if case==57 { 0 } else if case==58 { 2 } else { 9 });
        for (i,frame) in report.frames.iter().enumerate() {
            if i<4 || case==59 {
                assert_eq!((frame.mode,frame.digest),(origin.mode,origin.digest));
                assert_eq!(frame.member,if i==0 {0}else{1});
                assert!(frame.span.is_some());
            } else {
                assert_eq!(frame.mode,wire::Mode::Host);
                assert_eq!(frame.digest,REGISTRY_PROVIDER_DIGEST);
                assert_eq!(frame.member,0);assert!(frame.span.is_none());
            }
        }
        assert_eq!(report.frames[1],report.frames[2]);
        if case==59 { assert_eq!(report.frames[1],report.frames[63]); }
        assert_eq!(report.usage.host_calls,1);
        return 0;
    }
    if (54..=56).contains(&case) || (65..=68).contains(&case) || ((102..=166).contains(&case)&&case%2==0) {
        assert!(!report.poisoned && report.suppressed.is_empty());
        assert_eq!(report.primary.category, wire::Category::Limit);
        assert_eq!(report.primary.phase, wire::Phase::Call);
        assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
        assert_eq!(report.primary.limit, Some(wire::LimitDetail {
            kind: wire::ReportLimitKind::PreparationWork,
            limit: 10_000_000, attempted: 10_000_001,
        }));
        let origin = report.primary.origin.as_ref().unwrap();
        let expected=if case>=102 {(case-100).min(64) as usize}else{match case {54|67=>2,55|68=>4,56=>3,65=>6,66=>5,_=>unreachable!()}};
        assert_eq!(report.frames.len(), expected);
        assert_eq!(report.omitted, if case>=102 {u64::from((case-100).saturating_sub(64))}else{u64::from(case == 56 || case == 66)});
        for (i, frame) in report.frames.iter().enumerate() {
            if i % 2 == 0 {
                assert_eq!((frame.mode, frame.digest, frame.member), (origin.mode, origin.digest, origin.member));
                assert!(frame.span.is_some());
            } else {
                assert_eq!(frame.mode, wire::Mode::Host);
                assert_eq!(frame.digest, REGISTRY_PROVIDER_DIGEST);
                assert_eq!(frame.member, 0);
                assert!(frame.span.is_none());
            }
        }
        if expected>2 { assert_eq!(report.frames[0], report.frames[2]); }
        if expected>3 { assert_eq!(report.frames[1], report.frames[3]); }
        if expected>4 { assert_eq!(report.frames[0], report.frames[4]); }
        if expected>5 { assert_eq!(report.frames[1], report.frames[5]); }
        assert_eq!(report.usage.host_calls, if case>=102 {33}else if case>=65 {3}else{2});
        assert_eq!(report.usage.canonical_indices, 0);
        return 0;
    }
    if matches!(case, 42 | 53) {
        assert!(report.poisoned);
        assert_eq!(report.primary.category, wire::Category::Destructor);
        assert_eq!(report.primary.phase, wire::Phase::Cleanup);
        assert_eq!(report.primary.diagnostic, wire::Diagnostic::Destructor);
        assert_resource_cleanup_origin(&report.primary);
        assert!(report.primary.message.is_none() && report.primary.limit.is_none());
        assert!(report.frames.is_empty() && report.suppressed.is_empty());
        assert_eq!(report.omitted, 0);
        assert_eq!(report.usage.canonical_indices, if case == 42 { 3 } else { 1 });
        assert_eq!(report.usage.host_calls, 0);
        return 0;
    }
    if (43..=52).contains(&case) {
        let scenario = 2 + (case - 43) / 2;
        let failed_cleanup = scenario >= 3 && case % 2 == 0;
        assert_eq!(report.poisoned, failed_cleanup);
        assert_eq!(report.primary.category, wire::Category::Limit);
        assert_eq!(report.primary.phase, wire::Phase::Call);
        assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
        let limit = report.primary.limit.unwrap();
        match scenario {
            2 | 3 => assert_eq!(limit, wire::LimitDetail {
                kind: wire::ReportLimitKind::CanonicalIndices, limit: 2, attempted: 3,
            }),
            4 => assert_eq!(limit, wire::LimitDetail {
                kind: wire::ReportLimitKind::PreparationWork, limit: 10_000_000, attempted: 10_000_001,
            }),
            5 => {
                assert_eq!((limit.kind, limit.limit), (wire::ReportLimitKind::MetadataBytes, 16_777_216));
                assert!(limit.attempted > limit.limit);
            }
            6 => assert_eq!(limit, wire::LimitDetail {
                kind: wire::ReportLimitKind::ValueElements, limit: 1_000_000, attempted: 1_000_001,
            }),
            _ => unreachable!(),
        }
        let origin = report.primary.origin.as_ref().unwrap();
        assert_resource_guest_frame(origin, mode, RESOURCE_RUN_MEMBER);
        assert_eq!(report.frames.len(), 2);
        assert_resource_guest_frame(&report.frames[0], mode, RESOURCE_RUN_MEMBER);
        assert_eq!(report.frames[1].mode, wire::Mode::Host);
        assert_eq!(report.frames[1].digest, RESOURCE_PROVIDER_DIGEST);
        assert_eq!(report.frames[1].member, 0);
        assert!(report.frames[1].span.is_none());
        assert_eq!(report.suppressed.len(), usize::from(failed_cleanup));
        for record in &report.suppressed {
            assert_eq!(record.category, wire::Category::Destructor);
            assert_eq!(record.phase, wire::Phase::Cleanup);
            assert_eq!(record.diagnostic, wire::Diagnostic::Destructor);
            assert_resource_cleanup_origin(record);
        }
        assert_eq!(report.omitted, 0);
        assert_eq!(report.usage.host_calls, 1);
        assert_eq!(report.usage.canonical_indices, 2);
        return 0;
    }
    assert_eq!(
        report.poisoned,
        matches!(case, 4 | 5 | 7 | 8 | 9 | 10 | 17 | 21 | 22 | 24 | 27 | 69)
    );
    let origin = report.primary.origin.as_ref().unwrap();
    assert_eq!(
        origin.mode,
        if matches!(case, 7 | 8 | 10 | 21 | 23 | 69) {
            wire::Mode::Host
        } else if mode == 1 {
            wire::Mode::Jit
        } else {
            wire::Mode::Vm
        },
        "report case {case}: {report:?}"
    );
    assert_ne!(origin.digest, [0; 32]);
    assert_eq!(
        origin.span.is_none(),
        matches!(case, 7 | 8 | 10 | 21 | 23 | 69),
        "{report:?}"
    );
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
        26 | 27 => {
            assert_eq!(report.primary.category, wire::Category::Interruption);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Interrupted);
            assert!(report.primary.limit.is_none());
            assert_eq!(report.suppressed.len(), usize::from(case == 27));
            for record in &report.suppressed {
                assert_eq!(record.category, wire::Category::Destructor);
                assert_eq!(record.phase, wire::Phase::Cleanup);
                assert!(record.message.is_none() && record.limit.is_none());
                assert_resource_cleanup_origin(record);
            }
            assert_eq!(report.omitted, 0);
            assert_eq!(report.usage.host_calls, 1);
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
                assert_resource_cleanup_origin(record);
            }
            assert_eq!(report.usage.host_calls, 3, "case {case}: {report:?}");
            assert_eq!(report.usage.canonical_indices, 3);
            assert_eq!(report.omitted, 1);
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
        7 | 8 | 69 => {
            assert_eq!(report.primary.category, wire::Category::HostContract);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::HostContract);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_ne!(origin.digest, [17; 32]);
            assert_eq!(origin.member, 0);
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
            assert!(report.suppressed[0].origin.is_none());
            assert_eq!(report.omitted, 8, "native report case {case}: {report:?}");
            if case!=8 {
                assert_eq!(report.frames.len(),2);
                assert_eq!(report.frames[0].mode,if mode==1 {wire::Mode::Jit}else{wire::Mode::Vm});
                assert!(report.frames[0].span.is_some());
                assert_eq!(&report.frames[1],origin);
            } else {
                assert_eq!(report.frames.len(),4);
            }
            assert_eq!(
                report.usage.host_calls,
                if case == 7 { 1 } else { 2 },
                "native report case {case}: {report:?}"
            );
            assert_eq!(report.usage.canonical_indices, 0);
            assert_ne!(report.usage.fuel, 9999);
            assert_ne!(report.usage.peak_heap_bytes, 9999);
        }
        9 | 22 => {
            assert_eq!(report.primary.category, wire::Category::HostContract);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::HostContract);
            assert!(report.primary.limit.is_none() && report.suppressed.is_empty());
            assert_eq!(report.omitted, 0);
            assert_eq!(report.frames.len(), 2);
            assert_eq!(report.frames[1].mode,wire::Mode::Host);
            assert_eq!(report.frames[1].digest,REGISTRY_PROVIDER_DIGEST);
            assert_eq!(report.frames[1].member,0);assert!(report.frames[1].span.is_none());
            assert_eq!(report.usage.host_calls, 1);
        }
        10 | 23 => {
            assert_eq!(
                report.primary.category,
                if case == 10 {
                    wire::Category::Panic
                } else {
                    wire::Category::Trap
                }
            );
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(
                report.primary.diagnostic,
                if case == 10 {
                    wire::Diagnostic::Panic
                } else {
                    wire::Diagnostic::Trap
                }
            );
            assert_ne!(origin.digest, [17; 32]);
            assert!(report.primary.limit.is_none() && report.suppressed.is_empty());
            assert_eq!(report.omitted, 1);
            assert_eq!(report.frames.len(), 2);
            assert_eq!(&report.frames[1],origin);
            assert_eq!(report.usage.host_calls, 1);
        }
        38 | 39 | 40 | 41 => {
            assert_eq!(report.primary.category, wire::Category::Limit);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
            let limit = report.primary.limit.unwrap();
            match case {
                38 | 39 => assert_eq!(
                    limit,
                    wire::LimitDetail {
                        kind: wire::ReportLimitKind::PreparationWork,
                        limit: 10_000_000,
                        attempted: 10_000_000
                            + if case == 38 {
                                1
                            } else {
                                CALLBACK_SNAPSHOT_WORK.with(std::cell::Cell::get)
                            },
                    }
                ),
                40 => {
                    assert_eq!(
                        (limit.kind, limit.limit),
                        (wire::ReportLimitKind::MetadataBytes, 16_777_216)
                    );
                    assert!(limit.attempted > limit.limit);
                }
                41 => {
                    assert_eq!(
                        (limit.kind, limit.limit),
                        (wire::ReportLimitKind::ValueElements, 1_000_000)
                    );
                    assert!(limit.attempted > limit.limit);
                }
                _ => unreachable!(),
            }
            assert!(!report.poisoned && report.suppressed.is_empty());
            assert_eq!(report.omitted, 0);
            // Preparation refused before guest entry. There is one source
            // activation and the actual host callback, without a new child.
            assert_eq!(report.frames.iter().filter(|frame| {
                (frame.mode, frame.digest, frame.member) == (origin.mode, origin.digest, origin.member)
            }).count(), 1);
            let hosts = report.frames.iter().filter(|frame| frame.mode == wire::Mode::Host).collect::<Vec<_>>();
            assert_eq!(hosts.len(), 1);
            assert!(hosts[0].span.is_none());
            assert_eq!(hosts[0].digest, REGISTRY_PROVIDER_DIGEST);
            assert_eq!(hosts[0].member, 0);
            assert_ne!(hosts[0].digest, origin.digest);
            assert_eq!(report.usage.host_calls, 1);
            assert_eq!(report.usage.canonical_indices, 0);
        }
        25 | 200 | 201 => {
            assert_eq!(report.primary.category, wire::Category::Limit);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
            let limit = report.primary.limit.unwrap();
            if case==201 {
                assert_eq!(limit.kind, wire::ReportLimitKind::LiveAllocations);
                assert_eq!((limit.limit,limit.attempted),(128,129));
            } else {
                assert_eq!(limit.kind, wire::ReportLimitKind::HeapBytes);
                assert_eq!(limit.limit,4096);
                if case==200 {
                    assert!(limit.attempted>4096&&limit.attempted<=6144,"{report:?}");
                } else {assert_eq!(limit.attempted,4097);}
            }
            assert!(report.suppressed.is_empty());
            assert_eq!(report.omitted, 0);
            assert!((1..=2).contains(&report.frames.len()));
            assert_eq!(report.usage.host_calls, 1);
            assert_eq!(report.usage.canonical_indices, 0);
        }
        11 | 12 | 24 | 73 | 74 => {
            assert_eq!(report.primary.category, wire::Category::Limit);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
            let limit = report.primary.limit.unwrap();
            assert_eq!(
                limit.kind,
                if case != 12 && case != 74 {
                    wire::ReportLimitKind::Frames
                } else {
                    wire::ReportLimitKind::LiveCells
                }
            );
            assert_eq!(
                (limit.limit, limit.attempted),
                if case != 12 && case != 74 { (1, 2) } else { (5, 8) },
                "{report:?}"
            );
            assert_eq!(report.suppressed.len(), usize::from(case == 24));
            if case == 24 {
                assert_eq!(report.suppressed[0].category, wire::Category::HostContract);
                assert_eq!(report.suppressed[0].phase, wire::Phase::Cleanup);
            }
            assert_eq!(report.omitted, 0);
            assert_eq!(report.frames.len(),if case==73||case==74 {0}else{2});
            assert_eq!(report.usage.host_calls, 1);
            assert_eq!(report.usage.canonical_indices, 0);
        }
        13 => {
            assert_eq!(report.primary.category, wire::Category::Limit);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
            let limit = report.primary.limit.unwrap();
            assert_eq!(limit.kind, wire::ReportLimitKind::MetadataBytes);
            assert_eq!(limit.limit, 1048576);
            assert!(limit.attempted > limit.limit);
            assert_eq!(report.usage.host_calls, 0);
            assert!(report.suppressed.is_empty());
        }
        16 | 17 => {
            assert_eq!(report.primary.category, wire::Category::Panic);
            assert_eq!(report.primary.phase, wire::Phase::Call);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Panic);
            assert!(report.primary.limit.is_none());
            assert_eq!(report.suppressed.len(), (case & 1) as usize);
            for record in &report.suppressed {
                assert_eq!(record.category, wire::Category::Destructor);
                assert_eq!(record.phase, wire::Phase::Cleanup);
                assert_resource_cleanup_origin(record);
                assert!(record.message.is_none());
            }
            assert_eq!(report.omitted, 0);
            assert_eq!(report.frames.len(), 1);
            assert_eq!(report.usage.host_calls, 1);
            assert_eq!(report.usage.canonical_indices, 1);
        }
        20 => {
            assert_eq!(report.primary.category, wire::Category::Limit);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Limit);
            assert_eq!(
                report.primary.limit,
                Some(wire::LimitDetail {
                    kind: wire::ReportLimitKind::HostCalls,
                    limit: 0,
                    attempted: 1,
                })
            );
            assert_eq!(report.usage.host_calls, 0);
            assert!(report.suppressed.is_empty());
        }
        21 => {
            assert_eq!(report.primary.category, wire::Category::Destructor);
            assert_eq!(report.primary.diagnostic, wire::Diagnostic::Destructor);
            assert_eq!(report.primary.phase, wire::Phase::Cleanup);
            assert_resource_cleanup_origin(&report.primary);
            assert!(report.primary.limit.is_none() && report.suppressed.is_empty());
            assert_eq!(report.omitted, 0);
            assert_eq!(report.usage.host_calls, 1);
            assert_eq!(report.usage.canonical_indices, 1);
        }
        _ => panic!("unknown report case"),
    }
    0
}

#[unsafe(no_mangle)]
unsafe extern "C" fn probe_native_failure_report(
    context: *const ffi::Context,
    output: *mut *mut ffi::Buffer,
    kind: u32,
) -> u32 {
    let origin = wire::Frame {
        mode: if kind != 0 {
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
    if matches!(kind, 4 | 5 | 13 | 14) {
        report.primary.category = if kind != 5 {
            wire::Category::Destructor
        } else {
            wire::Category::HostContract
        };
        report.primary.phase = wire::Phase::Cleanup;
        report.primary.diagnostic = if kind != 5 {
            wire::Diagnostic::Destructor
        } else {
            wire::Diagnostic::HostContract
        };
        report.suppressed.clear();
        if kind == 13 {
            report.suppressed.push(wire::Record {
                category: wire::Category::HostContract,
                diagnostic: wire::Diagnostic::HostContract,
                ..report.primary.clone()
            });
            report.suppressed.push(report.primary.clone());
        }
        report.omitted = if kind == 14 { u64::MAX } else { 0 };
    } else if kind != 0 {
        report.primary.category = match kind {
            1 => wire::Category::Panic,
            2 => wire::Category::Trap,
            _ => wire::Category::Limit,
        };
        report.primary.diagnostic = match kind {
            1 => wire::Diagnostic::Panic,
            2 => wire::Diagnostic::Trap,
            _ => wire::Diagnostic::Limit,
        };
        if kind == 3 {
            report.primary.limit = Some(wire::LimitDetail {
                kind: wire::ReportLimitKind::Fuel,
                limit: 0,
                attempted: 1,
            });
        }
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
    let mut native = unsafe { *config };
    native.mode = 2;
    let mut native_engine = std::ptr::null_mut();
    // SAFETY: native mode accepts configuration, while source/artifact loading
    // still refuses before publication; it requires module_bind_native.
    assert_eq!(
        unsafe { gateway::engine_create(&native, &mut native_engine, &mut error) },
        0
    );
    let module_sentinel = 1usize as *mut ffi::Module;
    let mut untouched = module_sentinel;
    assert_eq!(unsafe { gateway::module_load(native_engine, format, code, idl, &mut untouched, &mut error) }, 3);
    assert_eq!(untouched, module_sentinel);
    assert_eq!(unsafe { gateway::engine_destroy(native_engine, &mut error) }, 0);
    0
}
fn main() {
    // SAFETY: the linked C fixture performs its complete native host lifecycle.
    assert_eq!(unsafe { fixture() }, 0);
}
