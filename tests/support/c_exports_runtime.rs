use super::*;
use lang_codegen_clif::c_abi::runtime::{NativeFactory, State};

type Zero = unsafe extern "C" fn(*mut c_void, *mut i64) -> u32;
type ZeroOnce = unsafe extern "C" fn(Zero, *mut c_void, *mut i64) -> u32;
const ZERO_C: &str = r#"
typedef uint32_t (*fixture_zero)(lang_context *, int64_t *);
EXPORT uint32_t c_zero_once(fixture_zero entry, lang_context *context, int64_t *output) {
    return entry(context, output);
}
"#;

pub(super) struct Metered<'p> {
    pub(super) inner: NativeFactory<'p>,
    pub(super) usage: Cell<e::Usage>,
}
impl<'p> ExportRuntime<'p> for Metered<'p> {
    type State<'call>
        = State<'call, 'p, 'call, 'p>
    where
        'p: 'call;
    fn prepare<'call>(
        &'p self,
        layout: &'p n::FunctionLayout<'p>,
        instance: &'call mut clean::Instance,
        allocator: &'call mut a::Allocator<lang_codegen_clif::c_abi::allocator::Provider<'p>>,
        environment: n::CEnvironment<'call>,
        arguments: &[v::Argument<'_>],
    ) -> Result<Self::State<'call>, c::Fault>
    where
        'p: 'call,
    {
        self.inner
            .prepare(layout, instance, allocator, environment, arguments)
    }
    fn complete<'call>(
        &self,
        state: &mut Self::State<'call>,
        failure: Option<n::Failure>,
        output: Option<a::Handle>,
    ) -> Result<(), c::Fault>
    where
        'p: 'call,
    {
        self.usage.set(state.c_usage().unwrap());
        self.inner.complete(state, failure, output)
    }
}

pub(super) const MANAGED_SOURCES: [&str; 10] = [
    include_str!("../../examples/analysis/native_core_bytes.lang"),
    include_str!("../../examples/scripts/text.lang"),
    include_str!("../../examples/scripts/utf8.lang"),
    include_str!("../../examples/analysis/native_vec_nested.lang"),
    include_str!("../../examples/analysis/native_map_nested.lang"),
    include_str!("../../examples/scripts/shared_owned.lang"),
    include_str!("../../examples/scripts/nested_cycle.lang"),
    include_str!("../../examples/analysis/native_shared_custody.lang"),
    include_str!("../../examples/scripts/arena_owned.lang"),
    include_str!("../../examples/analysis/native_arena_regions.lang"),
];

#[test]
fn production_c_export_runtime_matches_vm_across_all_managed_service_families() {
    for original in MANAGED_SOURCES {
        let vm_source = source(original);
        let mut vm_allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let mut vm_instance = clean::Instance::default();
        let vm = lang_vm::unit::owned::execute_entry(
            vm_source.failure_flow(),
            &mut vm_instance,
            Some(&mut vm_allocator),
            Default::default(),
        )
        .unwrap();
        assert_eq!(vm_allocator.stats().live_allocations, 0);
        assert!(original.contains("fn main()"));
        let text = original.replacen("fn main()", "pub extern \"C\" fn entry()", 1);
        let owner = source(&text);
        let types = owner
            .mir()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let artifact = artifact(&text, target(), Limits::default()).unwrap();
        let compiled = n::CCompiledProgram::compile_imports(
            owner.failure_flow(),
            &layouts,
            &artifact,
            Default::default(),
            4096,
        )
        .unwrap();
        let program = c::Program::new(
            &artifact,
            &compiled,
            config::Capacities::from_strides(64, 16, 64).unwrap(),
        )
        .unwrap();
        let support = ContextSupport::new(&program, Default::default()).unwrap();
        let factory = Metered {
            inner: NativeFactory::new(&compiled, &artifact, Default::default()).unwrap(),
            usage: Cell::new(e::Usage::default()),
        };
        let exports =
            NativeExports::new(&program, &compiled, &factory, Default::default()).unwrap();
        let library = Library::with_extra(&artifact, ZERO_C);
        let stats = Stats::default();
        let allocator = library.allocator(&stats);
        let limits = config::LimitsDescriptor {
            frames: 8,
            ..Default::default()
        };
        let config = ProgramConfig {
            version: 1,
            size: 80,
            allocator: &allocator,
            limits: &limits,
            imports: std::ptr::null(),
            import_count: 0,
            artifact: artifact.digest().0,
            flags: 0,
        };
        let entry = artifact
            .functions()
            .iter()
            .position(|f| f.name == "entry")
            .unwrap();
        // SAFETY: the generated entry and compiled C fixture have exactly this
        // zero-argument/i64-out profile. All actual objects/code/users stay live.
        unsafe {
            let entry = std::mem::transmute::<*const u8, Zero>(exports.symbol(entry).unwrap().1);
            let once = std::mem::transmute::<config::ErasedFunction, ZeroOnce>(
                library.function("c_zero_once"),
            );
            let mut context = std::ptr::null_mut();
            assert_eq!(support.create()(&config, &mut context), 0);
            let mut output = 77;
            assert_eq!(once(entry, context, &mut output), 0, "{text}");
            assert_eq!(output, vm.value, "{text}");
            let native = factory.usage.get();
            assert_eq!(native.fuel, vm.usage.fuel, "{text}");
            assert_eq!(native.service_work, vm.usage.service_work, "{text}");
            assert_eq!(native.peak_frames, vm.usage.peak_frames, "{text}");
            assert_eq!(native.peak_live_cells, vm.usage.peak_live_cells, "{text}");
            assert_eq!(support.destroy()(context), 0);
        }
        assert_eq!(stats.allocations.get(), stats.frees.get(), "{text}");
        assert_eq!(stats.wrong.get(), 0);
    }
}

#[test]
fn production_c_export_retires_every_fuel_and_provider_failure_prefix() {
    let original = include_str!("../../examples/analysis/native_core_bytes.lang");
    let vm_source = source(original);
    let mut vm_allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut vm_instance = clean::Instance::default();
    let vm = lang_vm::unit::owned::execute_entry(
        vm_source.failure_flow(),
        &mut vm_instance,
        Some(&mut vm_allocator),
        Default::default(),
    )
    .unwrap();
    let text = original.replacen("fn main()", "pub extern \"C\" fn entry()", 1);
    let owner = source(&text);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let artifact = artifact(&text, target(), Limits::default()).unwrap();
    let compiled = n::CCompiledProgram::compile_imports(
        owner.failure_flow(),
        &layouts,
        &artifact,
        Default::default(),
        4096,
    )
    .unwrap();
    let program = c::Program::new(
        &artifact,
        &compiled,
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let support = ContextSupport::new(&program, Default::default()).unwrap();
    let factory = NativeFactory::new(&compiled, &artifact, Default::default()).unwrap();
    let exports = NativeExports::new(&program, &compiled, &factory, Default::default()).unwrap();
    let library = Library::with_extra(&artifact, ZERO_C);
    let index = artifact
        .functions()
        .iter()
        .position(|f| f.name == "entry")
        .unwrap();
    // SAFETY: exact generated zero-argument profile and compiled C fixture.
    let entry = unsafe { std::mem::transmute::<*const u8, Zero>(exports.symbol(index).unwrap().1) };
    let once = unsafe {
        std::mem::transmute::<config::ErasedFunction, ZeroOnce>(library.function("c_zero_once"))
    };
    let run = |limits: config::LimitsDescriptor, refuse: u64, success: bool| {
        let stats = Stats::default();
        stats.refuse_at.set(refuse);
        let allocator = library.allocator(&stats);
        let config = ProgramConfig {
            version: 1,
            size: 80,
            allocator: &allocator,
            limits: &limits,
            imports: std::ptr::null(),
            import_count: 0,
            artifact: artifact.digest().0,
            flags: 0,
        };
        // SAFETY: descriptors, callback code/users and actual objects stay live
        // for both the refused call and the owning context's destruction.
        unsafe {
            let mut context = std::ptr::null_mut();
            assert_eq!(support.create()(&config, &mut context), 0);
            let mut output = 77;
            let status = once(entry, context, &mut output);
            if success {
                assert_eq!((status, output), (0, vm.value));
            } else {
                assert!(
                    matches!(status, 2 | 3),
                    "fuel {}, refusal {refuse}, status {status}",
                    limits.fuel
                );
                assert_eq!(output, 77);
                let mut summary = c::ErrorSummary::success();
                assert_eq!(support.last_error()(context, &mut summary), 0);
                assert_eq!(summary.status, status);
            }
            assert_eq!(support.destroy()(context), 0);
        }
        assert_eq!(stats.allocations.get(), stats.frees.get());
        assert_eq!(stats.wrong.get(), 0);
        stats.allocations.get()
    };
    let limits = config::LimitsDescriptor {
        frames: 8,
        ..Default::default()
    };
    let requests = run(limits, 0, true);
    assert!(requests > 2);
    for fuel in 0..vm.usage.fuel {
        run(config::LimitsDescriptor { fuel, ..limits }, 0, false);
    }
    for refuse in 1..=requests {
        run(limits, refuse, false);
    }
    for restricted in [
        config::LimitsDescriptor {
            frames: 0,
            ..limits
        },
        config::LimitsDescriptor { cells: 0, ..limits },
        config::LimitsDescriptor {
            live_bytes: 0,
            ..limits
        },
        config::LimitsDescriptor {
            cumulative_bytes: 0,
            ..limits
        },
        config::LimitsDescriptor {
            live_allocations: 0,
            ..limits
        },
        config::LimitsDescriptor {
            cleanup_records: 0,
            ..limits
        },
    ] {
        assert_eq!(
            run(restricted, 0, false),
            0,
            "refuse before provider effects"
        );
    }
}
