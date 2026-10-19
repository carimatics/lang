use super::*;
use lang_codegen_clif::c_abi::{
    context::{self as raw, ContextSupport, ProgramConfig},
    exports::{ExportRuntime, NativeExports},
};
use lang_runtime::c_abi::validation as v;

type ScalarExport = unsafe extern "C" fn(*mut c_void, i64, *mut i64) -> u32;
type Once = unsafe extern "C" fn(ScalarExport, *mut c_void, i64, *mut i64) -> u32;

#[repr(C)]
struct Probe {
    context: *mut c_void,
    query: raw::LastError,
    again: ScalarExport,
    calls: u64,
}

#[path = "c_exports_aggregate.rs"]
mod aggregates;
#[path = "c_exports_copy_cases.rs"]
mod copy_cases;
#[path = "c_objects.rs"]
mod objects;
#[path = "c_exports_runtime.rs"]
mod runtime_cases;

struct Factory;
static FACTORY: Factory = Factory;

struct FaultFactory(u8);
impl<'p> ExportRuntime<'p> for FaultFactory {
    type State<'call>
        = RuntimeState<'call, 'call, 'p, 'p, 'call>
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
        if self.0 == 0 {
            panic!("trusted export preparation panic");
        }
        FACTORY.prepare(layout, instance, allocator, environment, arguments)
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
        if self.0 == 1 {
            panic!("trusted export completion panic");
        }
        // Deliberately buggy trusted adapter: cleanup is complete, but the
        // native failure must still prevent the wrapper from publishing success.
        let _ = FACTORY.complete(state, failure, output);
        if let Some(handle) = output {
            state.allocator.bytes_mut(handle).unwrap().fill(0);
        }
        Ok(())
    }
}
impl<'p> ExportRuntime<'p> for Factory {
    type State<'call>
        = RuntimeState<'call, 'call, 'p, 'p, 'call>
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
        let mut state = RuntimeState::new(layout, instance, allocator, environment)?;
        let typed = layout.source().source().activity().moved().typed();
        let function = &typed.places().declarations().functions()[layout.function().0 as usize];
        for (parameter, argument) in function.parameters.iter().zip(arguments) {
            let slot = layout.frame().local(*parameter).unwrap();
            let value = match argument {
                v::Argument::I64(value) => l::Immediate::I64(*value),
                v::Argument::Bool(value) => l::Immediate::Bool(*value != 0),
                v::Argument::Object(_)
                    if layout
                        .layouts()
                        .get(function.locals[parameter.0 as usize].ty.ty)
                        .is_some_and(|l| l.size == 0 && matches!(l.kind, l::Kind::Scalar)) =>
                {
                    l::Immediate::Unit
                }
                _ => return Err(c::Fault::new(c::Kind::ResultShape)),
            };
            let cell = state.storage.cell(slot).unwrap();
            cell.write_immediate(cell.root(), value, state.allocator)
                .unwrap();
            state
                .initialized(slot, function.locals[parameter.0 as usize].span)
                .unwrap();
        }
        Ok(state)
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
        if state.report.is_none() {
            state
                .finish(clean::Exit::Failure {
                    failure: clean::ExecutionFailure::Fault,
                    span: failure.map(|f| f.span),
                })
                .map_err(|_| c::Fault::new(c::Kind::Cleanup))?;
        }
        let report = state
            .report
            .take()
            .expect("native exit or preflight cleanup produced a report");
        if let Some(fault) =
            c::Fault::from_report(&report, failure.and_then(|f| f.panic_code).unwrap_or(0))
        {
            return Err(fault);
        }
        if let Some(failure) = failure {
            return Err(match failure.reason {
                n::Reason::Foreign(fault) => fault,
                _ => c::Fault::new(c::Kind::Execution(clean::ExecutionFailure::Fault)),
            });
        }
        let result = state.result.expect("successful native source return");
        if let Some(handle) = output {
            let output = state
                .allocator
                .bytes_mut(handle)
                .map_err(|_| c::Fault::new(c::Kind::ResultShape))?;
            match output.len() {
                1 => output[0] = result as u8,
                8 => output.copy_from_slice(&result.to_ne_bytes()),
                _ => return Err(c::Fault::new(c::Kind::ResultShape)),
            }
        }
        Ok(())
    }
}

#[test]
fn real_c_calls_source_increment_and_panic_through_generated_native_exports() {
    let text = include_str!("../../examples/analysis/c_exports.lang");
    let owner = source(text);
    let flow = owner.failure_flow();
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let artifact = artifact(text, target(), Limits::default()).unwrap();
    let compiled =
        n::CCompiledProgram::compile_imports(flow, &layouts, &artifact, Default::default(), 4096)
            .unwrap();
    let program = c::Program::new(
        &artifact,
        &compiled,
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let support = ContextSupport::new(&program, Default::default()).unwrap();
    let factory = Factory;
    let exports = NativeExports::new(&program, &compiled, &factory, Default::default()).unwrap();
    let library = Library::with_extra(&artifact, include_str!("c_exports_native.c"));
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor::default();
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
    type ScalarExport = unsafe extern "C" fn(*mut c_void, i64, *mut i64) -> u32;
    type Exercise = unsafe extern "C" fn(
        raw::Create,
        raw::Destroy,
        raw::LastError,
        ScalarExport,
        ScalarExport,
        *const ProgramConfig,
    ) -> u32;
    let increment = artifact
        .functions()
        .iter()
        .position(|f| f.name == "increment")
        .unwrap();
    let fail = artifact
        .functions()
        .iter()
        .position(|f| f.name == "fail")
        .unwrap();
    assert_eq!(
        exports.symbol(increment).unwrap().0,
        artifact.functions()[increment].symbol
    );
    // SAFETY: exact generated C scalar signatures and the just-compiled C
    // fixture prototype match. All code/context/allocator/user owners remain live.
    let increment = unsafe {
        std::mem::transmute::<*const u8, ScalarExport>(exports.symbol(increment).unwrap().1)
    };
    let fail =
        unsafe { std::mem::transmute::<*const u8, ScalarExport>(exports.symbol(fail).unwrap().1) };
    let exercise = unsafe {
        std::mem::transmute::<config::ErasedFunction, Exercise>(
            library.function("c_exports_exercise"),
        )
    };
    assert_eq!(
        unsafe {
            exercise(
                support.create(),
                support.destroy(),
                support.last_error(),
                increment,
                fail,
                &config,
            )
        },
        0
    );
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert!(
        stats.allocations.get() > 0,
        "actual context allocator stores native frames"
    );
    assert_eq!(stats.effects.get(), 0);
    assert_eq!(stats.wrong.get(), 0);
}

#[test]
fn source_export_callbacks_keep_opaque_context_and_refuse_reentry() {
    let text = "extern \"C\" fn probe(value: i64) -> i64; pub extern \"C\" fn through(value: i64) -> i64 { unsafe { return probe(value); } }";
    let owner = source(text);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let artifact = artifact(text, target(), Limits::default()).unwrap();
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
    let factory = Factory;
    let exports = NativeExports::new(&program, &compiled, &factory, Default::default()).unwrap();
    let library = Library::with_extra(&artifact, include_str!("c_exports_native.c"));
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor::default();
    let import = artifact
        .functions()
        .iter()
        .find(|f| f.name == "probe")
        .unwrap();
    let index = artifact
        .functions()
        .iter()
        .position(|f| f.name == "through")
        .unwrap();
    // SAFETY: these signatures match the generated entry and compiled C fixture.
    let entry =
        unsafe { std::mem::transmute::<*const u8, ScalarExport>(exports.symbol(index).unwrap().1) };
    let once = unsafe {
        std::mem::transmute::<config::ErasedFunction, Once>(library.function("c_export_once"))
    };
    let mut probe = Probe {
        context: std::ptr::null_mut(),
        query: support.last_error(),
        again: entry,
        calls: 0,
    };
    let binding = raw::ImportBinding {
        version: 1,
        size: 80,
        name: import.name.as_ptr(),
        name_bytes: import.name.len() as u64,
        signature: import.digest.0,
        function: Some(library.function("c_probe")),
        user: (&mut probe as *mut Probe).cast(),
        flags: config::NATIVE_FFI,
    };
    let config = ProgramConfig {
        version: 1,
        size: 80,
        allocator: &allocator,
        limits: &limits,
        imports: &binding,
        import_count: 1,
        artifact: artifact.digest().0,
        flags: 0,
    };
    let mut context = std::ptr::null_mut();
    // SAFETY: descriptors, exact callback code/user, code owners and actual C
    // objects remain live. Reentry is synchronous and uses the same opaque context.
    unsafe {
        assert_eq!(support.create()(&config, &mut context), 0);
        probe.context = context;
        let mut result = 77;
        assert_eq!(once(entry, context, 41, &mut result), 0);
        assert_eq!((result, probe.calls), (42, 1));
        result = 77;
        assert_eq!(
            once(entry, context, -1, &mut result),
            c::Status::Runtime.code()
        );
        assert_eq!((result, probe.calls), (77, 2));
        let mut summary = c::ErrorSummary::success();
        assert_eq!(support.last_error()(context, &mut summary), 0);
        assert_eq!(summary.status, c::Status::Runtime.code());
        assert_eq!(once(entry, context, 41, &mut result), 0);
        assert_eq!((result, probe.calls), (42, 3));
        assert_eq!(support.destroy()(context), 0);
    }
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert_eq!(stats.wrong.get(), 0);
}

#[test]
fn source_exports_enforce_limits_and_retain_cumulative_allocation_accounting() {
    let text = include_str!("../../examples/analysis/c_exports.lang");
    let owner = source(text);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let artifact = artifact(text, target(), Limits::default()).unwrap();
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
    let factory = Factory;
    let exports = NativeExports::new(&program, &compiled, &factory, Default::default()).unwrap();
    let library = Library::with_extra(&artifact, include_str!("c_exports_native.c"));
    let index = artifact
        .functions()
        .iter()
        .position(|f| f.name == "increment")
        .unwrap();
    let frame_bytes: u64 = compiled
        .layout(compiled.exported_function(index).unwrap())
        .unwrap()
        .frame()
        .requests()
        .iter()
        .map(|r| r.bytes())
        .sum();
    assert!(frame_bytes > 0);
    let frame_allocations = compiled
        .layout(compiled.exported_function(index).unwrap())
        .unwrap()
        .frame()
        .requests()
        .iter()
        .filter(|r| r.bytes() != 0)
        .count() as u64;
    // SAFETY: exact generated profile and compiled C fixture prototype.
    let entry =
        unsafe { std::mem::transmute::<*const u8, ScalarExport>(exports.symbol(index).unwrap().1) };
    let once = unsafe {
        std::mem::transmute::<config::ErasedFunction, Once>(library.function("c_export_once"))
    };
    for mode in 0..8 {
        let stats = Stats::default();
        let allocator = library.allocator(&stats);
        let mut limits = config::LimitsDescriptor::default();
        match mode {
            0 => limits.frames = 0,
            1 => limits.cells = 0,
            2 => limits.live_bytes = 0,
            3 => limits.live_allocations = 0,
            4 => limits.fuel = 0,
            5 => stats.fail.set(1),
            6 => limits.cumulative_bytes = frame_bytes + 8,
            7 => {}
            _ => unreachable!(),
        }
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
        let mut context = std::ptr::null_mut();
        // SAFETY: all accessed actual objects and retained code/user are live.
        // Refused null/misaligned addresses are passed only to numeric preflight.
        unsafe {
            assert_eq!(support.create()(&config, &mut context), 0);
            let mut result = 77;
            let first = once(entry, context, 41, &mut result);
            if mode < 6 {
                assert_eq!(first, c::Status::Runtime.code(), "mode {mode}");
                assert_eq!(result, 77, "mode {mode}");
                if mode < 4 {
                    assert_eq!(stats.allocations.get(), 0, "preflight mode {mode}");
                }
            } else {
                assert_eq!((first, result), (0, 42));
                let allocations = stats.allocations.get();
                assert_eq!(
                    allocations,
                    frame_allocations + 1,
                    "result staging uses the actual C provider too"
                );
                result = 77;
                assert_eq!(
                    once(entry, context, 41, &mut result),
                    if mode == 6 {
                        c::Status::Runtime.code()
                    } else {
                        0
                    }
                );
                assert_eq!(result, if mode == 6 { 77 } else { 42 });
                if mode == 6 {
                    assert_eq!(
                        stats.allocations.get(),
                        allocations,
                        "retired first frame still consumes cumulative quota"
                    );
                }
            }
            let allocations = stats.allocations.get();
            let address_status = if mode == 0 {
                c::Status::Runtime.code()
            } else {
                c::Status::Invalid.code()
            };
            assert_eq!(
                once(entry, context, 41, std::ptr::null_mut()),
                address_status,
                "mode {mode}"
            );
            let mut bytes = [0x5au64; 2];
            let misaligned = bytes.as_mut_ptr().cast::<u8>().add(1).cast::<i64>();
            assert_eq!(
                once(entry, context, 41, misaligned),
                address_status,
                "mode {mode}"
            );
            assert_eq!(bytes, [0x5a; 2]);
            assert_eq!(stats.allocations.get(), allocations);
            assert_eq!(support.destroy()(context), 0);
        }
        assert_eq!(stats.allocations.get(), stats.frees.get());
        assert_eq!(stats.wrong.get(), 0);
    }
}

#[test]
fn source_bool_and_unit_exports_use_exact_c_profiles_and_reject_noncanonical_inputs() {
    let text = "pub extern \"C\" fn flag(value: bool) -> bool { return value; } pub extern \"C\" fn notify(value: core::Unit) -> core::Unit { return value; }";
    let owner = source(text);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let artifact = artifact(text, target(), Limits::default()).unwrap();
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
    let factory = Factory;
    let exports = NativeExports::new(&program, &compiled, &factory, Default::default()).unwrap();
    let unit = artifact
        .types()
        .iter()
        .find(|t| matches!(t.kind, Kind::Unit))
        .unwrap();
    let extra = format!(
        r#"
typedef uint32_t (*fixture_bool_export)(lang_context *, uint8_t, uint8_t *);
typedef uint32_t (*fixture_unit_export)(lang_context *, const lang_type_{digest} *);
EXPORT uint32_t c_bool_once(fixture_bool_export entry, lang_context *context, uint8_t value, uint8_t *output) {{ return entry(context, value, output); }}
EXPORT uint32_t c_unit_once(fixture_unit_export entry, lang_context *context, uint8_t reserved) {{ lang_type_{digest} value; value.reserved = reserved; return entry(context, &value); }}
"#,
        digest = unit.digest
    );
    let library = Library::with_extra(&artifact, &extra);
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor::default();
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
    type BoolExport = unsafe extern "C" fn(*mut c_void, u8, *mut u8) -> u32;
    type BoolOnce = unsafe extern "C" fn(BoolExport, *mut c_void, u8, *mut u8) -> u32;
    type UnitExport = unsafe extern "C" fn(*mut c_void, *const u8) -> u32;
    type UnitOnce = unsafe extern "C" fn(UnitExport, *mut c_void, u8) -> u32;
    let flag = artifact
        .functions()
        .iter()
        .position(|f| f.name == "flag")
        .unwrap();
    let notify = artifact
        .functions()
        .iter()
        .position(|f| f.name == "notify")
        .unwrap();
    // SAFETY: Unit's actual C reserved-byte type has size/alignment one. Each
    // function pointer exactly matches its generated native or C fixture profile.
    unsafe {
        let flag = std::mem::transmute::<*const u8, BoolExport>(exports.symbol(flag).unwrap().1);
        let notify =
            std::mem::transmute::<*const u8, UnitExport>(exports.symbol(notify).unwrap().1);
        let boolean = std::mem::transmute::<config::ErasedFunction, BoolOnce>(
            library.function("c_bool_once"),
        );
        let unit = std::mem::transmute::<config::ErasedFunction, UnitOnce>(
            library.function("c_unit_once"),
        );
        let mut context = std::ptr::null_mut();
        assert_eq!(support.create()(&config, &mut context), 0);
        let mut out = 77;
        assert_eq!(
            boolean(flag, context, 2, &mut out),
            c::Status::Invalid.code()
        );
        assert_eq!(out, 77);
        assert_eq!(unit(notify, context, 1), c::Status::Invalid.code());
        assert_eq!(stats.allocations.get(), 0);
        for value in [0, 1] {
            assert_eq!(boolean(flag, context, value, &mut out), 0);
            assert_eq!(out, value);
        }
        assert_eq!(unit(notify, context, 0), 0);
        assert_eq!(support.destroy()(context), 0);
    }
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert_eq!(stats.wrong.get(), 0);
}

#[test]
fn source_export_host_panics_poison_and_adapters_cannot_hide_native_failure() {
    let text = include_str!("../../examples/analysis/c_exports.lang");
    let owner = source(text);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let artifact = artifact(text, target(), Limits::default()).unwrap();
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
    let other = c::Program::new(
        &artifact,
        &compiled,
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let support = ContextSupport::new(&program, Default::default()).unwrap();
    let other_support = ContextSupport::new(&other, Default::default()).unwrap();
    let library = Library::with_extra(&artifact, include_str!("c_exports_native.c"));
    // SAFETY: exact compiled C fixture prototype.
    let once = unsafe {
        std::mem::transmute::<config::ErasedFunction, Once>(library.function("c_export_once"))
    };
    for mode in 0..3 {
        let factory = FaultFactory(mode);
        let exports =
            NativeExports::new(&program, &compiled, &factory, Default::default()).unwrap();
        let name = if mode == 2 { "fail" } else { "increment" };
        let index = artifact
            .functions()
            .iter()
            .position(|f| f.name == name)
            .unwrap();
        // SAFETY: exact generated scalar export signature.
        let entry = unsafe {
            std::mem::transmute::<*const u8, ScalarExport>(exports.symbol(index).unwrap().1)
        };
        let stats = Stats::default();
        let allocator = library.allocator(&stats);
        let limits = config::LimitsDescriptor::default();
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
        let mut context = std::ptr::null_mut();
        // SAFETY: accessed actual objects, callback users and code remain live;
        // another live registration is valid storage but must be refused.
        unsafe {
            assert_eq!(other_support.create()(&config, &mut context), 0);
            let mut output = 77;
            assert_eq!(
                once(entry, context, -42, &mut output),
                c::Status::Invalid.code()
            );
            assert_eq!(output, 77);
            assert_eq!(stats.allocations.get(), 0);
            assert_eq!(other_support.destroy()(context), 0);
            assert_eq!(support.create()(&config, &mut context), 0);
            let status = if mode == 2 {
                c::Status::GuestPanic.code()
            } else {
                c::Status::Runtime.code()
            };
            assert_eq!(once(entry, context, -42, &mut output), status);
            assert_eq!(output, 77);
            let mut summary = c::ErrorSummary::success();
            assert_eq!(support.last_error()(context, &mut summary), 0);
            assert_eq!(summary.status, status);
            if mode == 2 {
                assert_eq!(summary.panic_code, -42);
            }
            assert_eq!(
                once(entry, context, -42, &mut output),
                if mode == 2 {
                    status
                } else {
                    c::Status::Poisoned.code()
                }
            );
            assert_eq!(output, 77);
            assert_eq!(support.destroy()(context), 0);
        }
        assert_eq!(stats.allocations.get(), stats.frees.get());
        assert_eq!(stats.wrong.get(), 0);
    }
}
