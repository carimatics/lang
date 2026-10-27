//! Experimental production-component example: trusted native math plus a
//! verified script, explicit authority and the public component-v1 API.
use lang::{
    component_embedding::{ffi, gateway},
    frontend::component as f,
};
use lang_mir::unit as m;
use lang_runtime::component::{
    bindings, idl,
    negotiation::{InterfaceKey, PackageKey},
    wire,
};
use std::{
    cell::Cell,
    ffi::c_void,
    ptr,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

const SCRIPT_IDL: &[u8] = include_bytes!("plugins/verified.idl.json");
const NATIVE_IDL: &[u8] = include_bytes!("plugins/native.idl.json");
static LIVE: AtomicUsize = AtomicUsize::new(0);
fn header<T>() -> ffi::Header {
    ffi::Header {
        size: size_of::<T>() as u32,
        version: 1,
    }
}
fn bytes(input: &[u8]) -> ffi::Bytes {
    ffi::Bytes {
        data: input.as_ptr(),
        length: input.len() as u64,
    }
}
fn contract(input: &[u8]) -> wire::Contract {
    wire::Contract::new(
        idl::check(m::SourceId(1), input, Default::default()).unwrap(),
        vec![],
        &mut wire::Session::new(Default::default()),
    )
    .unwrap()
}
fn interface(package: &str) -> InterfaceKey {
    InterfaceKey::new(PackageKey::new("example", package), "api")
}
fn projection(interface: &str, member: &str) -> Vec<u8> {
    // These two explicit requested names are host policy, never inventory-
    // derived grants. The complete checked declaration supplies its types.
    let mut value: serde_json::Value = serde_json::from_slice(SCRIPT_IDL).unwrap();
    value["package"] = serde_json::json!("example:caller");
    value["imports"] =
        serde_json::json!([{"interface":interface,"version":{"exact":"1.0.0"},"members":[member]}]);
    value["exports"] = serde_json::json!([]);
    wire::encode_contract(
        &contract(&serde_json::to_vec(&value).unwrap()),
        &mut wire::Session::new(Default::default()),
    )
    .unwrap()
}
fn artifact() -> Vec<u8> {
    let checked = idl::check(m::SourceId(1), SCRIPT_IDL, Default::default()).unwrap();
    let analysis = f::analyze(f::Request {
        sources: lang_diagnostics::SourceMap::new(),
        display: std::path::Path::new("examples/plugins/verified.lang"),
        bytes: include_bytes!("plugins/verified.lang"),
        root: None,
        compile: Default::default(),
        document: checked,
        bindings: f::Input {
            version: 1,
            resources: vec![],
            imports: vec![f::Import {
                source: "increment".into(),
                member: bindings::MemberKey {
                    interface: interface("math"),
                    member: "increment".into(),
                },
            }],
            exports: vec![f::Export {
                source: "answer".into(),
                member: bindings::MemberKey {
                    interface: interface("plugin"),
                    member: "answer".into(),
                },
            }],
        },
        limits: Default::default(),
    });
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    lang::artifact::component::encode(
        analysis.result.unwrap(),
        m::Source {
            path: "examples/plugins/verified.idl.json".into(),
            text: String::from_utf8(SCRIPT_IDL.to_vec()).unwrap(),
        },
        None,
        Default::default(),
    )
    .unwrap()
}
fn api() -> &'static ffi::Api {
    let mut error: ffi::Error = unsafe { std::mem::zeroed() };
    error.header = header::<ffi::Error>();
    let mut output = ptr::null();
    // SAFETY: initialized local outputs receive this process's immutable table.
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
unsafe extern "C" fn allocate(_: *mut c_void, n: u64, a: u64) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(n.max(1) as usize, a as usize).unwrap();
    let output = unsafe { std::alloc::alloc(layout) };
    if !output.is_null() {
        LIVE.fetch_add(1, Ordering::Relaxed);
    }
    output
}
unsafe extern "C" fn deallocate(_: *mut c_void, p: *mut u8, n: u64, a: u64) {
    unsafe {
        std::alloc::dealloc(
            p,
            std::alloc::Layout::from_size_align(n.max(1) as usize, a as usize).unwrap(),
        )
    };
    assert!(LIVE.fetch_sub(1, Ordering::Relaxed) > 0);
}
struct NativePlugin {
    contract: wire::Contract,
    calls: Rc<Cell<usize>>,
    released: Rc<Cell<usize>>,
}
unsafe extern "C" fn increment(
    user: *mut c_void,
    context: *const ffi::Context,
    input: ffi::Bytes,
    output: *mut *mut ffi::Buffer,
) -> u32 {
    // SAFETY: successful registration retained this exact native plugin owner.
    let plugin = unsafe { &*user.cast::<NativePlugin>() };
    let api = api();
    let mut error: ffi::Error = unsafe { std::mem::zeroed() };
    error.header = header::<ffi::Error>();
    if unsafe { (api.context_check)(context, &mut error) } != 0 {
        return 1;
    }
    let mut session = wire::Session::new(wire::Limits {
        bytes: 1024,
        serialization_bytes: 1024,
        work: 10000,
        elements: 128,
        ..Default::default()
    });
    let signature = wire::Signature::new(
        &plugin.contract,
        &interface("math"),
        "increment",
        &mut session,
    )
    .unwrap();
    let parameters = wire::decode_values(
        &signature,
        wire::FrameKind::Parameters,
        unsafe { std::slice::from_raw_parts(input.data, input.length as usize) },
        &mut session,
    )
    .unwrap();
    let wire::Value::I64(value) = parameters.nodes[parameters.roots[0].0] else {
        return 2;
    };
    plugin.calls.set(plugin.calls.get() + 1);
    let encoded = wire::encode_values(
        &signature,
        wire::FrameKind::Result,
        &wire::Values {
            roots: vec![wire::ValueId(0)],
            nodes: vec![wire::Value::I64(value.saturating_add(1))],
        },
        &mut session,
    )
    .unwrap();
    // Native code cooperates with the original context. Its returned buffer
    // uses the caller's metered allocator and expires with this invocation.
    let status = unsafe {
        (api.context_buffer_create)(context, encoded.len() as u64, 1, output, &mut error)
    };
    if status != 0 {
        return 1;
    }
    let mut view = ffi::MutBytes {
        data: ptr::null_mut(),
        length: 0,
    };
    if unsafe { (api.buffer_mut_bytes)(context, *output, &mut view, &mut error) } != 0 {
        return 1;
    }
    unsafe { ptr::copy_nonoverlapping(encoded.as_ptr(), view.data, encoded.len()) };
    0
}
unsafe extern "C" fn release(user: *mut c_void) {
    let plugin = unsafe { Box::from_raw(user.cast::<NativePlugin>()) };
    plugin.released.set(plugin.released.get() + 1);
}
// SAFETY: main retains these original handles/descriptors for this entire call.
unsafe fn measure(
    api: &ffi::Api,
    engine: *mut ffi::Engine,
    binding: *mut ffi::Binding,
    limits: &ffi::Limits,
    consumer: &wire::Contract,
    samples: usize,
) -> serde_json::Value {
    let mut session = wire::Session::new(Default::default());
    let signature =
        wire::Signature::new(consumer, &interface("plugin"), "answer", &mut session).unwrap();
    let arguments = wire::encode_values(
        &signature,
        wire::FrameKind::Parameters,
        &wire::Values {
            roots: vec![wire::ValueId(0)],
            nodes: vec![wire::Value::I64(41)],
        },
        &mut session,
    )
    .unwrap();
    let baseline = LIVE.load(Ordering::Relaxed);
    let mut times = Vec::new();
    let mut counters = None;
    // SAFETY: all handles are live; views are inspected before result retirement.
    unsafe {
        for sample in 0..samples + 3 {
            let mut error: ffi::Error = std::mem::zeroed();
            error.header = header::<ffi::Error>();
            let mut control = ptr::null_mut();
            let mut result = ptr::null_mut();
            assert_eq!(
                (api.control_create)(engine, limits, &mut control, &mut error),
                0
            );
            let started = Instant::now();
            let status = (api.call)(binding, control, bytes(&arguments), &mut result, &mut error);
            let ns = u64::try_from(started.elapsed().as_nanos()).unwrap();
            assert_eq!(status, 0);
            let mut usage: ffi::Usage = std::mem::zeroed();
            usage.header = header::<ffi::Usage>();
            assert_eq!((api.result_usage)(result, &mut usage, &mut error), 0);
            let values = serde_json::json!({"fuel":usage.fuel,"service_work":usage.service_work,"host_calls":usage.host_calls,
                "peak_managed_bytes":usage.peak_heap_bytes,"peak_managed_allocations":usage.peak_live_allocations,
                "peak_frames":usage.peak_frames,"peak_cells":usage.peak_live_cells,"peak_tasks":usage.peak_live_tasks,"canonical_indices":usage.canonical_indices});
            if let Some(previous) = &counters {
                assert_eq!(previous, &values);
            } else {
                counters = Some(values);
            }
            let mut view = ffi::Bytes {
                data: ptr::null(),
                length: 0,
            };
            assert_eq!((api.result_bytes)(result, &mut view, &mut error), 0);
            let values = wire::decode_values(
                &signature,
                wire::FrameKind::Result,
                std::slice::from_raw_parts(view.data, view.length as usize),
                &mut wire::Session::new(Default::default()),
            )
            .unwrap();
            assert_eq!(values.nodes[values.roots[0].0], wire::Value::I64(42));
            assert_eq!((api.result_drop)(result, &mut error), 0);
            assert_eq!((api.control_drop)(control, &mut error), 0);
            assert_eq!(LIVE.load(Ordering::Relaxed), baseline);
            if sample >= 3 {
                times.push(ns);
            }
        }
    }
    let mut sorted = times.clone();
    sorted.sort_unstable();
    serde_json::json!({"samples_ns":times,"min_ns":sorted[0],"median_ns":sorted[sorted.len()/2],
        "p95_ns":sorted[(95*sorted.len()).div_ceil(100)-1],"max_ns":sorted[sorted.len()-1],"counters":counters,
        "warmups":3,"live_allocations_after_each_call":baseline})
}
fn main() {
    let mut args = std::env::args().skip(1);
    let mode = match args.next().as_deref() {
        None | Some("vm") => 0,
        Some("jit") => 1,
        _ => panic!("usage: component-plugin-host [vm|jit] [--measure samples:1..1001]"),
    };
    let samples = args.next().map(|flag| {
        assert_eq!(flag, "--measure");
        let n: usize = args
            .next()
            .expect("sample count")
            .parse()
            .expect("integer sample count");
        assert!((1..=1001).contains(&n));
        n
    });
    assert!(args.next().is_none());
    let api = api();
    let started = Instant::now();
    let code = artifact();
    let source_artifact_ns = u64::try_from(started.elapsed().as_nanos()).unwrap();
    let requested = projection("example:plugin/api", "answer");
    let grant = projection("example:math/api", "increment");
    let calls = Rc::new(Cell::new(0));
    let released = Rc::new(Cell::new(0));
    let plugin = Box::new(NativePlugin {
        contract: contract(NATIVE_IDL),
        calls: calls.clone(),
        released: released.clone(),
    });
    let mut session = wire::Session::new(Default::default());
    let native_contract = wire::encode_contract(&plugin.contract, &mut session).unwrap();
    let signature = wire::Signature::new(
        &plugin.contract,
        &interface("math"),
        "increment",
        &mut session,
    )
    .unwrap()
    .digest();
    let user = Box::into_raw(plugin).cast();
    let mut config: ffi::Config = unsafe { std::mem::zeroed() };
    config.header = header::<ffi::Config>();
    config.mode = mode;
    config.allocator.header = header::<ffi::Allocator>();
    config.allocator.allocate = Some(allocate);
    config.allocator.deallocate = Some(deallocate);
    let limits = &mut config.ceiling;
    limits.header = header::<ffi::Limits>();
    limits.fuel = 10000;
    limits.service_work = 10000;
    limits.host_calls = 8;
    limits.heap_bytes = 1048576;
    limits.live_allocations = 256;
    limits.frames = 32;
    limits.live_cells = 4096;
    limits.live_tasks = 1;
    limits.canonical_indices = 4;
    limits.preparation_work = 1000000;
    limits.metadata_bytes = 16777216;
    limits.value_elements = 65536;
    limits.serialization_bytes = 1048576;
    limits.depth = 128;
    limits.decoder_allocation_bytes = 16777216;
    limits.suppressed_records = 16;
    limits.trace_frames = 32;
    let mut error: ffi::Error = unsafe { std::mem::zeroed() };
    error.header = header::<ffi::Error>();
    let mut engine = ptr::null_mut();
    let mut module = ptr::null_mut();
    let mut provider = ptr::null_mut();
    let mut instance = ptr::null_mut();
    let mut binding = ptr::null_mut();
    let started = Instant::now();
    let mut measurement = None;
    // SAFETY: each descriptor/view is retained through admission; successful
    // registrations transfer user ownership once, and handles close in order.
    unsafe {
        assert_eq!((api.engine_create)(&config, &mut engine, &mut error), 0);
        assert_eq!(
            (api.module_load)(engine, 1, bytes(&code), bytes(&[]), &mut module, &mut error),
            0
        );
        let method = ffi::Import {
            header: header::<ffi::Import>(),
            function_key: bytes(b"example:math/api.increment"),
            signature,
            invoke: Some(increment),
            user,
            flags: 0,
            reserved0: 0,
            reserved: [0; 2],
        };
        let options = ffi::ProviderConfig {
            header: header::<ffi::ProviderConfig>(),
            contract: bytes(&native_contract),
            imports: &method,
            import_count: 1,
            resource_types: ptr::null(),
            resource_type_count: 0,
            owner: user,
            release_owner: Some(release),
            reserved: [0; 2],
        };
        assert_eq!(
            (api.provider_register)(engine, &options, &mut provider, &mut error),
            0
        );
        let mut options = ffi::InstanceConfig {
            header: header::<ffi::InstanceConfig>(),
            grants: ptr::null(),
            grant_count: 0,
            ceiling: config.ceiling,
            reserved: [0; 2],
        };
        let sentinel = 42usize as *mut ffi::Instance;
        let mut denied = sentinel;
        assert_eq!(
            (api.instance_create)(module, &options, &mut denied, &mut error),
            2
        );
        assert_eq!(denied, sentinel);
        assert_eq!(calls.get(), 0);
        let grant = bytes(&grant);
        options.grants = &grant;
        options.grant_count = 1;
        assert_eq!(
            (api.instance_create)(module, &options, &mut instance, &mut error),
            0
        );
        assert_eq!(
            (api.bind_export)(instance, bytes(&requested), &mut binding, &mut error),
            0
        );
        assert_eq!((api.module_drop)(module, &mut error), 0);
        let admission_ns = u64::try_from(started.elapsed().as_nanos()).unwrap();
        let consumer = contract(SCRIPT_IDL);
        if let Some(samples) = samples {
            measurement = Some(
                serde_json::json!({"format":"lang-m3-boundary-costs","version":1,
                "backend":if mode==0 {"vm"} else {"jit"},"profile":if cfg!(debug_assertions){"dev"}else{"release"},
                "source_artifact_ns":source_artifact_ns,"admission_ns":admission_ns,"samples":samples,
                "entry_includes":"public API call: per-call preparation, canonical adapters, script, native callback, mandatory cleanup and retained result creation",
                "entry_excludes":"source artifact compilation, engine/module/provider/instance/binding admission, control creation, result inspection and result/control drop",
                "entry":measure(api, engine, binding, &config.ceiling, &consumer, samples)}),
            );
        }
        let mut retained = ptr::null_mut();
        for (value, fuel, expected) in [(41, 10000, 0), (-1, 10000, 1), (0, 10000, 0), (41, 0, 1)] {
            let mut session = wire::Session::new(Default::default());
            let signature =
                wire::Signature::new(&consumer, &interface("plugin"), "answer", &mut session)
                    .unwrap();
            let arguments = wire::encode_values(
                &signature,
                wire::FrameKind::Parameters,
                &wire::Values {
                    roots: vec![wire::ValueId(0)],
                    nodes: vec![wire::Value::I64(value)],
                },
                &mut session,
            )
            .unwrap();
            let mut limits = config.ceiling;
            limits.fuel = fuel;
            let mut control = ptr::null_mut();
            let mut result = ptr::null_mut();
            assert_eq!(
                (api.control_create)(engine, &limits, &mut control, &mut error),
                0
            );
            assert_eq!(
                (api.call)(binding, control, bytes(&arguments), &mut result, &mut error),
                expected
            );
            let mut view = ffi::Bytes {
                data: ptr::null(),
                length: 0,
            };
            assert_eq!((api.result_bytes)(result, &mut view, &mut error), 0);
            let encoded = std::slice::from_raw_parts(view.data, view.length as usize);
            if expected == 0 {
                let values =
                    wire::decode_values(&signature, wire::FrameKind::Result, encoded, &mut session)
                        .unwrap();
                assert_eq!(values.nodes[values.roots[0].0], wire::Value::I64(value + 1));
            } else {
                let report = wire::decode_report(encoded, &mut session).unwrap();
                assert!(!report.poisoned);
                assert_eq!(
                    report.primary.diagnostic,
                    if value < 0 {
                        wire::Diagnostic::Panic
                    } else {
                        wire::Diagnostic::Limit
                    }
                );
            }
            if value < 0 {
                retained = result;
            } else {
                assert_eq!((api.result_drop)(result, &mut error), 0);
            }
            assert_eq!((api.control_drop)(control, &mut error), 0);
        }
        assert_eq!(calls.get(), 2 + samples.map_or(0, |n| n + 3));
        let mut failure = ptr::null_mut();
        assert_eq!((api.instance_close)(instance, &mut failure, &mut error), 0);
        assert!(failure.is_null());
        assert_eq!((api.binding_drop)(binding, &mut error), 0);
        assert_eq!((api.provider_drop)(provider, &mut failure, &mut error), 0);
        assert!(failure.is_null());
        assert_eq!(released.get(), 0);
        assert_eq!((api.result_drop)(retained, &mut error), 0);
        assert_eq!(released.get(), 1);
        assert_eq!((api.engine_destroy)(engine, &mut error), 0);
    }
    assert_eq!(LIVE.load(Ordering::Relaxed), 0);
    if let Some(mut measurement) = measurement {
        measurement["final_live_allocations"] = serde_json::json!(0);
        measurement["owner_releases"] = serde_json::json!(released.get());
        measurement["native_calls"] = serde_json::json!(calls.get());
        println!("{}", serde_json::to_string_pretty(&measurement).unwrap());
        return;
    }
    println!(
        "{}: script=42; native calls=2; grant denied; panic; reuse=1; fuel refused; owner released; live allocations=0",
        if mode == 0 { "VM" } else { "JIT" }
    );
}
