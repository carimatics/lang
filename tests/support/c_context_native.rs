use super::*;
use lang_codegen_clif::c_abi::context::{
    self as raw, ContextSupport, ImportBinding, ProgramConfig,
};

const SOURCE: &str = concat!(
    "extern \"C\" fn flag(value: bool) -> bool;\n",
    include_str!("../../examples/analysis/c_exports.lang")
);
fn metadata() -> Artifact {
    artifact(SOURCE, target(), Limits::default()).unwrap()
}
fn raw_bindings(artifact: &Artifact, library: &Library, stats: &Stats) -> Vec<ImportBinding> {
    bindings(artifact, library, stats)
        .iter()
        .map(|binding| ImportBinding {
            version: binding.version,
            size: binding.size,
            name: binding.name.unwrap().as_ptr(),
            name_bytes: binding.name_bytes,
            signature: binding.signature.0,
            function: binding.function,
            user: binding.user,
            flags: binding.flags,
        })
        .collect()
}
fn configuration(
    artifact: &Artifact,
    allocator: &config::Allocator,
    limits: &config::LimitsDescriptor,
    bindings: &[ImportBinding],
) -> ProgramConfig {
    ProgramConfig {
        version: 1,
        size: 80,
        allocator,
        limits,
        imports: bindings.as_ptr(),
        import_count: bindings.len() as u64,
        artifact: artifact.digest().0,
        flags: 0,
    }
}
fn capacities() -> config::Capacities {
    config::Capacities::from_strides(64, 16, 64).unwrap()
}

#[test]
fn generated_c_context_support_runs_real_c_calls_and_owns_descriptor_snapshots() {
    assert_eq!(
        (
            std::mem::size_of::<ProgramConfig>(),
            std::mem::align_of::<ProgramConfig>()
        ),
        (80, 8)
    );
    assert_eq!(
        (
            std::mem::size_of::<ImportBinding>(),
            std::mem::align_of::<ImportBinding>()
        ),
        (80, 8)
    );
    let owner = source(SOURCE);
    let artifact = metadata();
    let program = c::Program::new(&artifact, &owner, capacities()).unwrap();
    let support = ContextSupport::new(&program, Default::default()).unwrap();
    let header = artifact
        .header(HeaderOptions::default(), Limits::default())
        .unwrap();
    for (name, symbol) in ["create", "destroy", "last_error"]
        .into_iter()
        .zip(support.symbols())
    {
        assert_eq!(
            *symbol,
            format!("lang_context_{name}_{}", artifact.digest())
        );
        assert!(header.contains(symbol));
    }
    let library = Library::with_extra(&artifact, include_str!("c_context_native.c"));
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor::default();
    let bindings = raw_bindings(&artifact, &library, &stats);
    let config = configuration(&artifact, &allocator, &limits, &bindings);
    type Exercise = unsafe extern "C" fn(
        raw::Create,
        raw::Destroy,
        raw::LastError,
        *const ProgramConfig,
    ) -> u32;
    type Snapshot =
        unsafe extern "C" fn(raw::Create, *const ProgramConfig, *mut *mut c_void) -> u32;
    // SAFETY: the retained just-compiled C fixture has these exact typedef-
    // checked signatures. Program/code/descriptors/user cells outlive every call.
    let exercise = unsafe {
        std::mem::transmute::<config::ErasedFunction, Exercise>(
            library.function("c_context_exercise"),
        )
    };
    let snapshot = unsafe {
        std::mem::transmute::<config::ErasedFunction, Snapshot>(
            library.function("c_context_snapshot"),
        )
    };
    assert_eq!(
        unsafe {
            exercise(
                support.create(),
                support.destroy(),
                support.last_error(),
                &config,
            )
        },
        0
    );
    let mut handle = std::ptr::null_mut();
    assert_eq!(
        unsafe { snapshot(support.create(), &config, &mut handle) },
        0
    );
    // SAFETY: snapshot returned a live handle; its temporary C descriptors have
    // already been overwritten and retired, while code/user owners remain live.
    unsafe {
        raw::with_context(&program, handle, |context| {
            assert!(context.allocator().allocate.is_some());
            assert_eq!(context.limits().execution.frames, Some(256));
            let expected = artifact
                .functions()
                .iter()
                .position(|f| f.direction == lang_runtime::c_abi::Direction::Import)
                .unwrap();
            let binding = context.import(expected).unwrap();
            assert_eq!(binding.name(), artifact.functions()[expected].name);
            assert!(std::ptr::fn_addr_eq(
                binding.function(),
                bindings[0].function.unwrap()
            ));
            assert_eq!(context.last_error().unwrap().status, 0);
            Ok(())
        })
    }
    .unwrap();
    // SAFETY: the handle is live and uniquely consumed, after its Rust borrow ended.
    assert_eq!(unsafe { support.destroy()(handle) }, 0);
    assert_eq!(
        stats.allocations.get(),
        0,
        "context admission invokes no guest provider"
    );
    assert_eq!(stats.effects.get(), 0);
}

#[test]
fn raw_context_creation_rejects_complete_invalid_requests_without_output_or_callbacks() {
    let artifact = metadata();
    let owner = source(SOURCE);
    let program = c::Program::new(&artifact, &owner, capacities()).unwrap();
    let support = ContextSupport::new(&program, Default::default()).unwrap();
    let library = Library::new(&artifact);
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor::default();
    let bindings = raw_bindings(&artifact, &library, &stats);
    for mode in 0..19 {
        let mut allocator = allocator;
        let mut limits = limits;
        let mut bindings = bindings.clone();
        let mut config = configuration(&artifact, &allocator, &limits, &bindings);
        let bad_name = [0xff];
        let expected = if mode == 15 { 5 } else { 1 };
        match mode {
            0 => config.version = 2,
            1 => config.size = 79,
            2 => {
                config.flags = 1;
                config.allocator = std::ptr::dangling();
            }
            3 => {
                config.artifact[0] ^= 1;
                config.limits = std::ptr::dangling();
            }
            4 => config.allocator = std::ptr::null(),
            5 => {
                allocator.version = 2;
                config.allocator = &allocator;
            }
            6 => {
                allocator.allocate = None;
                config.allocator = &allocator;
            }
            7 => config.limits = std::ptr::null(),
            8 => {
                limits.reserved = 1;
                config.limits = &limits;
            }
            9 => {
                limits.cleanup_records = 4097;
                config.limits = &limits;
            }
            10 => {
                config.import_count = 4097;
                config.imports = std::ptr::dangling();
            }
            11 => config.imports = std::ptr::null(),
            12 => {
                bindings[0].name = std::ptr::null();
                config.imports = bindings.as_ptr();
            }
            13 => {
                bindings[0].name_bytes = u64::MAX;
                config.imports = bindings.as_ptr();
            }
            14 => {
                bindings[0].signature[0] ^= 1;
                config.imports = bindings.as_ptr();
            }
            15 => {
                bindings[0].flags = 0;
                config.imports = bindings.as_ptr();
            }
            16 => {
                bindings[0].name = bad_name.as_ptr();
                bindings[0].name_bytes = 1;
                config.imports = bindings.as_ptr();
            }
            17 => {
                bindings.push(bindings[0]);
                config.imports = bindings.as_ptr();
                config.import_count = 2;
            }
            _ => {
                config.imports = std::ptr::null();
                config.import_count = 0;
            }
        }
        let sentinel = std::ptr::without_provenance_mut::<c_void>(0x1238);
        let mut output = sentinel;
        // SAFETY: all accessed fields/ranges are actual live initialized C
        // layout objects. Dangling fields are behind rejected scalar prefixes;
        // numeric-null/overflow ranges must reject before dereferencing them.
        let status = unsafe { support.create()(&config, &mut output) };
        assert_eq!(status, expected, "mode {mode}");
        assert_eq!(output, sentinel, "mode {mode}");
    }
    assert_eq!(stats.allocations.get(), 0);
    assert_eq!(stats.effects.get(), 0);
}

#[test]
fn raw_context_preserves_program_identity_reentry_panic_and_poison_summaries() {
    let artifact = metadata();
    let owner = source(SOURCE);
    let program = c::Program::new(&artifact, &owner, capacities()).unwrap();
    let other = c::Program::new(&artifact, &owner, capacities()).unwrap();
    let support = ContextSupport::new(&program, Default::default()).unwrap();
    let foreign = ContextSupport::new(&other, Default::default()).unwrap();
    let library = Library::new(&artifact);
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor::default();
    let bindings = raw_bindings(&artifact, &library, &stats);
    let config = configuration(&artifact, &allocator, &limits, &bindings);
    let mut handle = std::ptr::null_mut();
    // SAFETY: native code/actual descriptors/user data remain live; every
    // admitted context is consumed once after synchronous borrows finish.
    unsafe {
        assert_eq!(support.create()(&config, &mut handle), 0);
        let mut summary = c::ErrorSummary::success();
        summary.panic_code = 99;
        summary.status = 77;
        let untouched = summary;
        assert_eq!(foreign.last_error()(handle, &mut summary), 1);
        assert_eq!(summary, untouched);
        assert_eq!(foreign.destroy()(handle), 1);
        raw::with_context(&program, handle, |context| {
            let control = e::Control::default();
            let work = e::ServiceWork::new(None);
            let mut usage = e::Usage::default();
            let result = context.run(
                &program,
                control,
                &work,
                &mut usage,
                c::EntryPoint::Guest,
                &mut |_| {
                    assert_eq!(
                        support.destroy()(handle),
                        1,
                        "reentry never frees the active context"
                    );
                    let before = summary;
                    assert_eq!(support.last_error()(handle, &mut summary), 1);
                    assert_eq!(summary, before);
                    Err(c::Fault::guest_panic(-42, None))
                },
            );
            assert_eq!(result.unwrap_err().status(), c::Status::GuestPanic);
            assert!(!context.is_poisoned().unwrap());
            Ok(())
        })
        .unwrap();
        assert_eq!(support.last_error()(handle, &mut summary), 0);
        assert_eq!((summary.status, summary.panic_code), (2, -42));
        raw::with_context(&program, handle, |context| {
            let mut usage = e::Usage::default();
            let result = context.run(
                &program,
                e::Control::default(),
                &e::ServiceWork::new(None),
                &mut usage,
                c::EntryPoint::Guest,
                &mut |_| panic!("contained native body"),
            );
            assert!(result.is_err());
            assert!(context.is_poisoned().unwrap());
            Ok(())
        })
        .unwrap();
        assert_eq!(support.last_error()(handle, &mut summary), 0);
        assert_eq!(summary.status, 3);
        let saved = summary;
        raw::with_context(&program, handle, |context| {
            let mut usage = e::Usage::default();
            let result = context.run(
                &program,
                e::Control::default(),
                &e::ServiceWork::new(None),
                &mut usage,
                c::EntryPoint::Guest,
                &mut |_| unreachable!("poison rejects before entry"),
            );
            assert_eq!(result.unwrap_err().status(), c::Status::Poisoned);
            Ok(())
        })
        .unwrap();
        assert_eq!(support.last_error()(handle, &mut summary), 0);
        assert_eq!(summary, saved);
        assert_eq!(support.destroy()(handle), 0);
    }
    assert_eq!(stats.allocations.get(), 0);
    assert_eq!(stats.effects.get(), 0);
}

#[test]
fn raw_context_support_rejects_misaligned_null_and_overlapping_outputs_before_access() {
    let artifact = metadata();
    let owner = source(SOURCE);
    let program = c::Program::new(&artifact, &owner, capacities()).unwrap();
    let support = ContextSupport::new(&program, Default::default()).unwrap();
    let library = Library::new(&artifact);
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor::default();
    let bindings = raw_bindings(&artifact, &library, &stats);
    let mut config = configuration(&artifact, &allocator, &limits, &bindings);
    let mut output = std::ptr::without_provenance_mut::<c_void>(0x1238);
    let before = output;
    // SAFETY: deliberately numeric-invalid inputs must reject before access.
    // Admitted normal objects and output storage stay live throughout the calls.
    unsafe {
        assert_eq!(support.create()(std::ptr::null(), &mut output), 1);
        assert_eq!(output, before);
        assert_eq!(
            support.create()(std::ptr::dangling(), std::ptr::null_mut()),
            1
        );
        let misaligned = std::ptr::addr_of!(config)
            .cast::<u8>()
            .wrapping_add(1)
            .cast();
        assert_eq!(support.create()(misaligned, &mut output), 1);
        assert_eq!(output, before);
        let alias = std::ptr::addr_of_mut!(config.allocator).cast::<*mut c_void>();
        assert_eq!(support.create()(std::ptr::addr_of!(config), alias), 1);
        assert!(std::ptr::eq(config.allocator, &allocator));
        assert_eq!(support.create()(&config, &mut output), 0);
        let mut summary = c::ErrorSummary::success();
        summary.panic_code = 77;
        let saved = summary;
        let bad_context = output.cast::<u8>().wrapping_add(1).cast();
        assert_eq!(support.last_error()(bad_context, &mut summary), 1);
        assert_eq!(summary, saved);
        assert_eq!(support.last_error()(output, output.cast()), 1);
        assert_eq!(support.last_error()(output, &mut summary), 0);
        assert_eq!(summary, c::ErrorSummary::success());
        assert_eq!(support.destroy()(output), 0);
    }
    assert_eq!(stats.allocations.get(), 0);
    assert_eq!(stats.effects.get(), 0);
}

#[test]
fn raw_context_support_accepts_zero_guest_budgets_and_no_import_array() {
    let owner = source(include_str!("../../examples/analysis/c_exports.lang"));
    let artifact = artifact(
        include_str!("../../examples/analysis/c_exports.lang"),
        target(),
        Limits::default(),
    )
    .unwrap();
    let program = c::Program::new(&artifact, &owner, capacities()).unwrap();
    let support = ContextSupport::new(&program, Default::default()).unwrap();
    let library = Library::new(&artifact);
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor {
        live_bytes: 0,
        cumulative_bytes: 0,
        live_allocations: 0,
        frames: 0,
        cells: 0,
        host_calls: 0,
        fuel: 0,
        cleanup_records: 0,
        ..Default::default()
    };
    let mut config = configuration(&artifact, &allocator, &limits, &[]);
    config.imports = std::ptr::null();
    let mut context = std::ptr::null_mut();
    // SAFETY: zero budgets are valid descriptor values. Null imports accompany
    // exactly count zero; every other accessed object/code/user remains live.
    unsafe {
        assert_eq!(support.create()(&config, &mut context), 0);
        let mut summary = c::ErrorSummary::success();
        assert_eq!(support.last_error()(context, &mut summary), 0);
        assert_eq!(support.destroy()(context), 0);
    }
    assert_eq!(stats.allocations.get(), 0);
}
