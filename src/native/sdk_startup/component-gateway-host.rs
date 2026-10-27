//! Installed C gateway consumer: exact SDK-owned Image and generated C codec.
mod bindings {
    include!("bindings.rs");
}
use lang::component_embedding::{ffi, gateway};
use lang_runtime::component::{idl, wire};
use std::{
    ffi::c_void,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};
static LIVE: AtomicUsize = AtomicUsize::new(0);
static RELEASED: AtomicUsize = AtomicUsize::new(0);
fn header<T>() -> ffi::Header {
    ffi::Header {
        size: size_of::<T>() as u32,
        version: 1,
    }
}
fn bytes(data: &[u8]) -> ffi::Bytes {
    ffi::Bytes {
        data: data.as_ptr(),
        length: data.len() as u64,
    }
}
unsafe extern "C" fn allocate(_: *mut c_void, n: u64, alignment: u64) -> *mut u8 {
    let layout =
        std::alloc::Layout::from_size_align(n.max(1) as usize, alignment as usize).unwrap();
    let result = unsafe { std::alloc::alloc(layout) };
    if !result.is_null() {
        LIVE.fetch_add(1, Ordering::Relaxed);
    }
    result
}
unsafe extern "C" fn deallocate(_: *mut c_void, data: *mut u8, n: u64, alignment: u64) {
    unsafe {
        std::alloc::dealloc(
            data,
            std::alloc::Layout::from_size_align(n.max(1) as usize, alignment as usize).unwrap(),
        )
    };
    assert!(LIVE.fetch_sub(1, Ordering::Relaxed) > 0);
}
unsafe extern "C" fn release(_: *mut c_void) {
    assert_eq!(RELEASED.fetch_add(1, Ordering::Relaxed), 0);
}
type Call = unsafe extern "C" fn(
    *mut ffi::Binding,
    *mut ffi::Control,
    ffi::Bytes,
    *mut *mut ffi::Result,
    *mut ffi::Error,
) -> u32;
type Info = unsafe extern "C" fn(*const ffi::Result, *mut ffi::ResultInfo, *mut ffi::Error) -> u32;
unsafe extern "C" {
    fn component_typed_call(
        call: Call,
        info: Info,
        binding: *mut ffi::Binding,
        control: *mut ffi::Control,
        value: i64,
        result: *mut *mut ffi::Result,
        error: *mut ffi::Error,
    ) -> u32;
    fn component_typed_result(bytes: ffi::Bytes) -> i64;
}
fn main() {
    // SAFETY: the exact original object and generated addresses are linked by
    // this matching SDK; they remain executable through the final owner return.
    let image = Box::new(
        unsafe {
            lang::native::component::Image::new(
                include_bytes!("package.json"),
                lang_codegen_clif::sdk::runtime::Input {
                    target: bindings::TARGET,
                    build_id: bindings::BUILD_ID,
                    semantic: bindings::SEMANTIC,
                    native: bindings::NATIVE,
                },
                include_bytes!("bindings.rs"),
                (),
                bindings::resolve,
                Default::default(),
            )
        }
        .unwrap(),
    );
    let request = br#"{"idl":1,"package":"demo:caller","version":"1.0.0","interfaces":[{"key":"demo:object/api","resources":[],"types":[],"functions":[{"name":"answer","effect":"sync","params":[{"name":"value","type":"i64"}],"result":"i64","capability":"demo.object.answer"}]}],"imports":[{"interface":"demo:object/api","version":{"exact":"1.0.0"},"members":["answer"]}],"exports":[]}"#;
    let mut session = wire::Session::new(Default::default());
    let request = wire::Contract::new(
        idl::check(lang_mir::unit::SourceId(1), request, Default::default()).unwrap(),
        vec![],
        &mut session,
    )
    .unwrap();
    let request = wire::encode_contract(&request, &mut session).unwrap();
    // These C descriptors contain integers, pointers and optional callbacks.
    let mut config: ffi::Config = unsafe { std::mem::zeroed() };
    config.header = header::<ffi::Config>();
    config.mode = 2;
    config.allocator.header = header::<ffi::Allocator>();
    config.allocator.allocate = Some(allocate);
    config.allocator.deallocate = Some(deallocate);
    config.ceiling.header = header::<ffi::Limits>();
    config.ceiling.fuel = 1_000_000;
    config.ceiling.service_work = 1_000_000;
    config.ceiling.host_calls = 1_000;
    config.ceiling.heap_bytes = 1_048_576;
    config.ceiling.live_allocations = 512;
    config.ceiling.frames = 64;
    config.ceiling.live_cells = 10_000;
    config.ceiling.live_tasks = 64;
    config.ceiling.canonical_indices = 4;
    config.ceiling.preparation_work = 1_000_000;
    config.ceiling.metadata_bytes = 16_777_216;
    config.ceiling.value_elements = 65_536;
    config.ceiling.serialization_bytes = 1_048_576;
    config.ceiling.depth = 128;
    config.ceiling.decoder_allocation_bytes = 16_777_216;
    config.ceiling.suppressed_records = 64;
    config.ceiling.trace_frames = 64;
    let mut error: ffi::Error = unsafe { std::mem::zeroed() };
    error.header = header::<ffi::Error>();
    let mut api = ptr::null();
    assert_eq!(
        unsafe {
            gateway::lang_component_v1_get_api(
                1,
                size_of::<ffi::Api>() as u32,
                &mut api,
                &mut error,
            )
        },
        0
    );
    let api = unsafe { &*api };
    let mut engine = ptr::null_mut();
    let mut module = ptr::null_mut();
    let mut instance = ptr::null_mut();
    let mut binding = ptr::null_mut();
    let native = ffi::NativeImage {
        header: header::<ffi::NativeImage>(),
        metadata: bytes(image.metadata()),
        program: image.as_opaque(),
        owner: ptr::null_mut(),
        release_owner: Some(release),
        reserved: [0; 2],
    };
    // SAFETY: every exact live handle is closed once in dependency order; all
    // initialized descriptors and their source views remain valid through use.
    unsafe {
        assert_eq!((api.engine_create)(&config, &mut engine, &mut error), 0);
        assert_eq!(
            (api.module_bind_native)(engine, &native, &mut module, &mut error),
            0
        );
        drop(image);
        let options = ffi::InstanceConfig {
            header: header::<ffi::InstanceConfig>(),
            grants: ptr::null(),
            grant_count: 0,
            ceiling: config.ceiling,
            reserved: [0; 2],
        };
        assert_eq!(
            (api.instance_create)(module, &options, &mut instance, &mut error),
            0
        );
        assert_eq!((api.module_drop)(module, &mut error), 0);
        assert_eq!(
            (api.bind_export)(instance, bytes(&request), &mut binding, &mut error),
            0
        );
        let mut retained = ptr::null_mut();
        for (value, fuel) in [(41, 1_000_000), (-1, 1_000_000), (0, 1_000_000), (41, 0)] {
            let mut limits = config.ceiling;
            limits.fuel = fuel;
            let mut control = ptr::null_mut();
            let mut result = ptr::null_mut();
            assert_eq!(
                (api.control_create)(engine, &limits, &mut control, &mut error),
                0
            );
            assert_eq!(
                component_typed_call(
                    api.call,
                    api.result_info,
                    binding,
                    control,
                    value,
                    &mut result,
                    &mut error
                ),
                u32::from(value < 0 || fuel == 0)
            );
            let mut view = ffi::Bytes {
                data: ptr::null(),
                length: 0,
            };
            assert_eq!((api.result_bytes)(result, &mut view, &mut error), 0);
            if value >= 0 && fuel > 0 {
                assert_eq!(component_typed_result(view), value + 1);
            } else {
                let report = wire::decode_report(
                    std::slice::from_raw_parts(view.data, view.length as usize),
                    &mut wire::Session::new(Default::default()),
                )
                .unwrap();
                assert!(!report.poisoned);
                assert_eq!(
                    report.primary.diagnostic,
                    if value < 0 {
                        wire::Diagnostic::Panic
                    } else {
                        wire::Diagnostic::Limit
                    }
                );
                if let Some(origin) = report.primary.origin {
                    assert_eq!(origin.mode, wire::Mode::Aot);
                }
            }
            if value < 0 {
                retained = result;
            } else {
                assert_eq!((api.result_drop)(result, &mut error), 0);
            }
            assert_eq!((api.control_drop)(control, &mut error), 0);
        }
        let mut closed = ptr::null_mut();
        assert_eq!((api.instance_close)(instance, &mut closed, &mut error), 0);
        assert!(closed.is_null());
        assert_eq!((api.binding_drop)(binding, &mut error), 0);
        assert_eq!(RELEASED.load(Ordering::Relaxed), 0);
        assert_eq!((api.result_drop)(retained, &mut error), 0);
        assert_eq!(RELEASED.load(Ordering::Relaxed), 1);
        assert_eq!((api.engine_destroy)(engine, &mut error), 0);
        assert_eq!(LIVE.load(Ordering::Relaxed), 0);
    }
    println!("component C gateway 42/panic/reuse/fuel; owners retired; zero inventory");
}
