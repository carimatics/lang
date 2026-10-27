mod bindings { include!(concat!(@PACKAGE@,"/bindings.rs")); }
mod provider { @PROVIDER@ }
use lang::component_embedding::{ffi, gateway};
use lang_runtime::component::wire;
use std::{ffi::c_void, ptr, sync::atomic::{AtomicUsize,Ordering}};
static LIVE: AtomicUsize=AtomicUsize::new(0);
static RELEASED: AtomicUsize=AtomicUsize::new(0);
fn header<T>() -> ffi::Header {ffi::Header{size:std::mem::size_of::<T>() as u32,version:1}}
fn bytes(data:&[u8])->ffi::Bytes {ffi::Bytes{data:data.as_ptr(),length:data.len() as u64}}
unsafe extern "C" fn allocate(_: *mut c_void,n:u64,a:u64)->*mut u8 {
    let layout=std::alloc::Layout::from_size_align(n.max(1) as usize,a as usize).unwrap();
    let p=unsafe{std::alloc::alloc(layout)};if !p.is_null(){LIVE.fetch_add(1,Ordering::Relaxed);}p
}
unsafe extern "C" fn deallocate(_: *mut c_void,p:*mut u8,n:u64,a:u64) {
    unsafe{std::alloc::dealloc(p,std::alloc::Layout::from_size_align(n.max(1) as usize,a as usize).unwrap())};
    assert!(LIVE.fetch_sub(1,Ordering::Relaxed)>0);
}
unsafe extern "C" fn release(_: *mut c_void) {assert_eq!(RELEASED.fetch_add(1,Ordering::Relaxed),0);}
type Call=unsafe extern "C" fn(*mut ffi::Binding,*mut ffi::Control,ffi::Bytes,*mut *mut ffi::Result,*mut ffi::Error)->u32;
type Info=unsafe extern "C" fn(*const ffi::Result,*mut ffi::ResultInfo,*mut ffi::Error)->u32;
unsafe extern "C" {
    fn typed_call(call:Call,info:Info,binding:*mut ffi::Binding,control:*mut ffi::Control,value:i64,result:*mut *mut ffi::Result,error:*mut ffi::Error)->u32;
    fn typed_result(bytes:ffi::Bytes)->i64;
}
fn main() {
    assert!(!std::path::Path::new(@PACKAGE@).exists());
    let image=Box::new(unsafe{lang::native::component::Image::new(
        include_bytes!(concat!(@PACKAGE@,"/package.json")),
        lang_codegen_clif::sdk::runtime::Input{target:bindings::TARGET,build_id:bindings::BUILD_ID,semantic:bindings::SEMANTIC,native:bindings::NATIVE},
        include_bytes!(concat!(@PACKAGE@,"/bindings.rs")),(),bindings::resolve,Default::default(),
    )}.unwrap());
    let mut config:ffi::Config=unsafe{std::mem::zeroed()};
    config.header=header::<ffi::Config>();config.mode=2;
    config.allocator.header=header::<ffi::Allocator>();config.allocator.allocate=Some(allocate);config.allocator.deallocate=Some(deallocate);
    config.ceiling.header=header::<ffi::Limits>();
    config.ceiling.fuel=1000000;config.ceiling.service_work=1000000;config.ceiling.host_calls=1000;
    config.ceiling.heap_bytes=1048576;config.ceiling.live_allocations=512;config.ceiling.frames=64;config.ceiling.live_cells=10000;
    config.ceiling.live_tasks=64;config.ceiling.canonical_indices=4;config.ceiling.preparation_work=1000000;
    config.ceiling.metadata_bytes=16777216;config.ceiling.value_elements=65536;config.ceiling.serialization_bytes=1048576;
    config.ceiling.depth=128;config.ceiling.decoder_allocation_bytes=16777216;config.ceiling.suppressed_records=64;config.ceiling.trace_frames=64;
    let mut error:ffi::Error=unsafe{std::mem::zeroed()};error.header=header::<ffi::Error>();
    let mut engine=ptr::null_mut();let mut module=ptr::null_mut();let mut instance=ptr::null_mut();let mut binding=ptr::null_mut();
    let descriptor=ffi::NativeImage{header:header::<ffi::NativeImage>(),metadata:bytes(image.metadata()),program:image.as_opaque(),owner:ptr::null_mut(),release_owner:Some(release),reserved:[0;2]};
    unsafe {
        assert_eq!(gateway::engine_create(&config,&mut engine,&mut error),0);
        // Incompatible passive metadata must refuse before touching this
        // deliberately invalid private pointer or consuming the supplied owner.
        let mut incompatible=image.metadata().to_vec();
        let start=std::str::from_utf8(&incompatible).unwrap().find(bindings::BUILD_ID).unwrap();
        incompatible[start]=if incompatible[start]==b'0'{b'1'}else{b'0'};
        let mut bad=descriptor;bad.metadata=bytes(&incompatible);bad.program=1usize as *const c_void;
        let sentinel=42usize as *mut ffi::Module;let mut untouched=sentinel;
        assert_eq!(gateway::module_bind_native(engine,&bad,&mut untouched,&mut error),3);
        assert_eq!(untouched,sentinel);assert_eq!(RELEASED.load(Ordering::Relaxed),0);
        bad.metadata=bytes(b"{");
        assert_eq!(gateway::module_bind_native(engine,&bad,&mut untouched,&mut error),2);
        assert_eq!(untouched,sentinel);assert_eq!(RELEASED.load(Ordering::Relaxed),0);
        assert_eq!(gateway::module_bind_native(engine,&descriptor,&mut module,&mut error),0,"native admission");
        drop(image);
        let mut registered=ptr::null_mut();let mut child_instance=ptr::null_mut();let mut child_binding=ptr::null_mut();
        let mut state: *mut provider::State=ptr::null_mut();let mut grant_bytes=Vec::new();
        if @CALLBACK@ {
            let owner=Box::new(provider::State::with_resources(@RESOURCE@));
            let mut session=wire::Session::new(Default::default());
            let host=wire::encode_contract(&owner.host,&mut session).unwrap();
            for member in if @RESOURCE@ {&["add","make"][..]}else{&["add"][..]} {
                grant_bytes.push(wire::encode_contract(&provider::grant(@RESOURCE@,member),&mut session).unwrap());
            }
            let signature=owner.signature(true,&mut session).digest();
            state=Box::into_raw(owner);
            let mut methods=vec![ffi::Import{header:header::<ffi::Import>(),function_key:bytes(b"demo:host/api.add"),signature,invoke:Some(provider::invoke),user:state.cast(),flags:1,reserved0:0,reserved:[0;2]}];
            if @RESOURCE@ {
                let key=(*state).host.document().interfaces[0].key.value.clone();
                let signature=wire::Signature::new(&(*state).host,&key,"make",&mut session).unwrap().digest();
                methods.push(ffi::Import{header:header::<ffi::Import>(),function_key:bytes(b"demo:host/api.make"),signature,invoke:Some(provider::make),user:state.cast(),flags:0,reserved0:0,reserved:[0;2]});
            }
            let resource=ffi::ResourceType{header:header::<ffi::ResourceType>(),resource_key:bytes(b"demo:host/api.item"),destroy:Some(provider::destroy),user:state.cast(),reserved:[0;2]};
            let options=ffi::ProviderConfig{header:header::<ffi::ProviderConfig>(),contract:bytes(&host),imports:methods.as_ptr(),import_count:methods.len() as u64,resource_types:if @RESOURCE@ {&resource}else{ptr::null()},resource_type_count:if @RESOURCE@ {1}else{0},owner:state.cast(),release_owner:Some(provider::release),reserved:[0;2]};
            assert_eq!(gateway::provider_register(engine,&options,&mut registered,&mut error),0);
        }
        let grants=grant_bytes.iter().map(|b|bytes(b)).collect::<Vec<_>>();
        let options=ffi::InstanceConfig{header:header::<ffi::InstanceConfig>(),grants:grants.as_ptr(),grant_count:grants.len() as u64,ceiling:config.ceiling,reserved:[0;2]};
        assert_eq!(gateway::instance_create(module,&options,&mut instance,&mut error),0);
        if @CALLBACK@ {
            assert_eq!(gateway::instance_create(module,&options,&mut child_instance,&mut error),0);
            assert_eq!(gateway::bind_export(child_instance,bytes(include_bytes!(concat!(@PACKAGE@,"/request.bin"))),&mut child_binding,&mut error),0);
            (*state).child.set(child_binding);grant_bytes.clear();
        }
        assert_eq!(gateway::module_drop(module,&mut error),0);assert_eq!(RELEASED.load(Ordering::Relaxed),0);
        assert_eq!(gateway::bind_export(instance,bytes(include_bytes!(concat!(@PACKAGE@,"/request.bin"))),&mut binding,&mut error),0);
        let mut retained=ptr::null_mut();let mut baseline:ffi::Usage=std::mem::zeroed();baseline.header=header::<ffi::Usage>();
        for (value,fuel,interrupt,task_limit) in [(41,1000000,false,64),(-1,1000000,false,64),(0,1000000,false,64),(41,0,false,64),(41,1000000,true,64),(41,1000000,false,1)] {
            let mut limits=config.ceiling;limits.fuel=fuel;limits.live_tasks=task_limit;let mut control=ptr::null_mut();let mut result=ptr::null_mut();
            assert_eq!(gateway::control_create(engine,&limits,&mut control,&mut error),0);
            if interrupt {gateway::interrupt(control);}
            let status=typed_call(gateway::call,gateway::result_info,binding,control,value,&mut result,&mut error);
            if status==1&&value>=0&&fuel>0&&!interrupt&&!(@TASK@&&task_limit==1) {
                let mut failed=ffi::Bytes{data:ptr::null(),length:0};assert_eq!(gateway::result_bytes(result,&mut failed,&mut error),0);
                let report=wire::decode_report(std::slice::from_raw_parts(failed.data,failed.length as usize),&mut wire::Session::new(Default::default())).unwrap();
                panic!("unexpected native failure {report:?}");
            }
            assert_eq!(status,u32::from(value<0||fuel==0||interrupt||(@TASK@&&task_limit==1)),"native call value={value} fuel={fuel}");assert!(!result.is_null());
            let mut view=ffi::Bytes{data:ptr::null(),length:0};assert_eq!(gateway::result_bytes(result,&mut view,&mut error),0);
            if value==41&&fuel>0&&!interrupt&&task_limit==64 {assert_eq!(gateway::result_usage(result,&mut baseline,&mut error),0);}
            if status==0 {assert_eq!(typed_result(view),value+1);} else {
                let report=wire::decode_report(std::slice::from_raw_parts(view.data,view.length as usize),&mut wire::Session::new(Default::default())).unwrap();
                assert!(!report.poisoned,"control value={value} fuel={fuel} interrupted={interrupt} report={report:?}");assert_eq!(report.primary.diagnostic,if value<0{wire::Diagnostic::Panic}else if interrupt{wire::Diagnostic::Interrupted}else{wire::Diagnostic::Limit});
                if let Some(origin)=report.primary.origin {assert_eq!(origin.mode,wire::Mode::Aot);}
                if @TASK@&&task_limit==1 {let detail=report.primary.limit.unwrap();assert_eq!(detail.kind,wire::ReportLimitKind::LiveTasks);assert_eq!((detail.limit,detail.attempted),(1,2));}
            }
            if value<0 {retained=result;} else {assert_eq!(gateway::result_drop(result,&mut error),0);}
            assert_eq!(gateway::control_drop(control,&mut error),0);
        }
        assert!(baseline.fuel>0&&baseline.fuel<1000);
        assert_eq!(baseline.peak_live_tasks,if @TASK@ {2}else{1});
        assert_eq!(baseline.host_calls,if @RESOURCE@ {4}else if @CALLBACK@ {2}else{0});
        if @CALLBACK@ {assert_eq!((*state).max_depth.get(),2);assert!((*state).calls.get()>=4);}
        for prefix in 0..=baseline.fuel {
            let mut limits=config.ceiling;limits.fuel=prefix;
            limits.frames=baseline.peak_frames;limits.live_cells=baseline.peak_live_cells;limits.live_tasks=baseline.peak_live_tasks;
            let mut control=ptr::null_mut();let mut result=ptr::null_mut();
            assert_eq!(gateway::control_create(engine,&limits,&mut control,&mut error),0);
            let status=typed_call(gateway::call,gateway::result_info,binding,control,41,&mut result,&mut error);
            assert_eq!(status,u32::from(prefix<baseline.fuel));assert!(!result.is_null());
            let mut view=ffi::Bytes{data:ptr::null(),length:0};assert_eq!(gateway::result_bytes(result,&mut view,&mut error),0);
            if status==0 {assert_eq!(typed_result(view),42);} else {
                let report=wire::decode_report(std::slice::from_raw_parts(view.data,view.length as usize),&mut wire::Session::new(Default::default())).unwrap();
                assert!(!report.poisoned,"fuel prefix={prefix} report={report:?}");assert_eq!(report.primary.diagnostic,wire::Diagnostic::Limit);
                let detail=report.primary.limit.unwrap();assert_eq!(detail.kind,wire::ReportLimitKind::Fuel);
                assert_eq!((detail.limit,detail.attempted),(prefix,prefix+1));
            }
            assert_eq!(gateway::result_drop(result,&mut error),0);assert_eq!(gateway::control_drop(control,&mut error),0);
        }
        if @CALLBACK@ {
            let cost=(*state).after.get().work.checked_sub((*state).before.get().work).unwrap();
            assert!(cost>4&&cost<config.ceiling.preparation_work as usize);
            for remaining in [0,1,cost/4,cost/2,3*cost/4,cost-1] {
                (*state).reserve.set(Some(remaining));
                let mut control=ptr::null_mut();let mut result=ptr::null_mut();
                assert_eq!(gateway::control_create(engine,&config.ceiling,&mut control,&mut error),0);
                let status=typed_call(gateway::call,gateway::result_info,binding,control,41,&mut result,&mut error);
                assert_eq!(status,1,"original preparation remaining={remaining} cost={cost}");assert!(!result.is_null());
                let mut view=ffi::Bytes{data:ptr::null(),length:0};assert_eq!(gateway::result_bytes(result,&mut view,&mut error),0);
                let report=wire::decode_report(std::slice::from_raw_parts(view.data,view.length as usize),&mut wire::Session::new(Default::default())).unwrap();
                assert!(!report.poisoned,"preparation remaining={remaining} cost={cost} report={report:?}");assert_eq!(report.primary.diagnostic,wire::Diagnostic::Limit);
                let detail=report.primary.limit.unwrap();assert_eq!(detail.kind,wire::ReportLimitKind::PreparationWork);
                assert_eq!(detail.limit,config.ceiling.preparation_work);assert!(detail.attempted>detail.limit);
                assert_eq!(gateway::result_drop(result,&mut error),0);assert_eq!(gateway::control_drop(control,&mut error),0);
            }
            (*state).reserve.set(None);
            let mut control=ptr::null_mut();let mut result=ptr::null_mut();
            assert_eq!(gateway::control_create(engine,&config.ceiling,&mut control,&mut error),0);
            assert_eq!(typed_call(gateway::call,gateway::result_info,binding,control,0,&mut result,&mut error),0);
            let mut view=ffi::Bytes{data:ptr::null(),length:0};assert_eq!(gateway::result_bytes(result,&mut view,&mut error),0);assert_eq!(typed_result(view),1);
            assert_eq!(gateway::result_drop(result,&mut error),0);assert_eq!(gateway::control_drop(control,&mut error),0);
        }
        let mut poisoned=ptr::null_mut();
        if @RESOURCE@ {
            assert_eq!((*state).created.get(),(*state).retired.get());assert!((*state).live.borrow().is_empty());
            (*state).fail_destroy.set(true);
            let mut control=ptr::null_mut();assert_eq!(gateway::control_create(engine,&config.ceiling,&mut control,&mut error),0);
            assert_eq!(typed_call(gateway::call,gateway::result_info,binding,control,41,&mut poisoned,&mut error),1);assert!(!poisoned.is_null());
            let mut view=ffi::Bytes{data:ptr::null(),length:0};assert_eq!(gateway::result_bytes(poisoned,&mut view,&mut error),0);
            let report=wire::decode_report(std::slice::from_raw_parts(view.data,view.length as usize),&mut wire::Session::new(Default::default())).unwrap();
            assert!(report.poisoned);assert_eq!(report.primary.diagnostic,wire::Diagnostic::Destructor);assert!(!report.suppressed.is_empty());
            assert_eq!((*state).created.get(),(*state).retired.get());assert!((*state).live.borrow().is_empty());
            assert_eq!(gateway::control_drop(control,&mut error),0);
            let before=((*state).created.get(),(*state).calls.get());
            assert_eq!(gateway::control_create(engine,&config.ceiling,&mut control,&mut error),0);
            let mut refused=ptr::null_mut();assert_eq!(typed_call(gateway::call,gateway::result_info,binding,control,41,&mut refused,&mut error),2);
            assert!(refused.is_null());assert_eq!(((*state).created.get(),(*state).calls.get()),before);
            assert_eq!(gateway::control_drop(control,&mut error),0);
        }
        let mut closed=1usize as *mut ffi::Result;
        assert_eq!(gateway::instance_close(instance,&mut closed,&mut error),0);assert!(closed.is_null());
        if @CALLBACK@ {
            (*state).child.set(ptr::null_mut());
            assert_eq!(gateway::instance_close(child_instance,&mut closed,&mut error),0);assert!(closed.is_null());
            assert_eq!(gateway::binding_drop(child_binding,&mut error),0);
            assert_eq!(gateway::provider_drop(registered,&mut closed,&mut error),0);assert!(closed.is_null());
            assert_eq!(provider::RELEASED.load(Ordering::Relaxed),0);
        }
        assert_eq!(gateway::binding_drop(binding,&mut error),0);assert_eq!(RELEASED.load(Ordering::Relaxed),0);
        assert_eq!(gateway::result_drop(retained,&mut error),0);
        if @RESOURCE@ {
            assert_eq!(RELEASED.load(Ordering::Relaxed),0);assert_eq!(provider::RELEASED.load(Ordering::Relaxed),0);
            assert_eq!(gateway::result_drop(poisoned,&mut error),0);
        }
        assert_eq!(RELEASED.load(Ordering::Relaxed),1);
        if @CALLBACK@ {assert_eq!(provider::RELEASED.load(Ordering::Relaxed),1);}
        assert_eq!(gateway::engine_destroy(engine,&mut error),0);assert_eq!(LIVE.load(Ordering::Relaxed),0);
    }
    println!("native image C: 42; panic; reuse; fuel; owner released");
}
