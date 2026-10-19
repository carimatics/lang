use super::*;
use lang_codegen_clif::c_abi::{object::ObjectExports, runtime::NativeFactory};

#[path = "c_object_binding.rs"]
mod binding;

#[path = "c_object_records.rs"]
mod records;

#[test]
fn persisted_c_object_imports_preserve_scalar_unit_aggregate_and_failure_profiles() {
    let aggregate = include_str!("../../examples/analysis/c_aggregate_imports.lang");
    let bad_aggregate = aggregate.replace("echo_result", "echo_bad_tag");
    let foreign = "extern \"C\" fn increment(value: i64) -> i64; fn main() -> i64 { unsafe { return increment(-1); } }";
    let (many, expected_many) = mixed_import_source();
    for (original, expected) in [
        (
            include_str!("../../examples/analysis/c_scalar_imports.lang"),
            Some(59),
        ),
        (aggregate, Some(84)),
        (bad_aggregate.as_str(), None),
        (foreign, None),
        (many.as_str(), Some(expected_many)),
    ] {
        let text = original.replacen("fn main()", "pub extern \"C\" fn entry()", 1);
        let records::Stored {
            decoded,
            record,
            library,
        } = records::stored(
            &text,
            |artifact| {
                let entry = artifact
                    .functions()
                    .iter()
                    .find(|f| f.name == "entry")
                    .unwrap();
                format!(
                    "EXPORT uint32_t c_linked_entry(lang_context *context, int64_t *output) {{ return {}(context, output); }}\n",
                    entry.symbol
                )
            },
            true,
        );
        let flow = decoded.flow();
        let declarations = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations();
        let layouts = l::Layouts::compile(declarations.types(), Default::default()).unwrap();
        let artifact = Artifact::compile(declarations, target(), Default::default()).unwrap();
        let object = record
            .prepare_c(flow, &layouts, &artifact, Default::default(), 4096)
            .unwrap();
        type Entry = unsafe extern "C" fn(*mut c_void, *mut i64) -> u32;
        // SAFETY: actual C definition includes this exact generated prototype.
        let entry: Entry = unsafe { std::mem::transmute(library.function("c_linked_entry")) };
        let stats = Stats::default();
        let allocator = library.allocator(&stats);
        let imports: Vec<_> = artifact
            .functions()
            .iter()
            .filter(|f| f.direction == lang_runtime::c_abi::Direction::Import)
            .map(|f| raw::ImportBinding {
                version: 1,
                size: 80,
                name: f.name.as_ptr(),
                name_bytes: f.name.len() as u64,
                signature: f.digest.0,
                function: Some(
                    library.function(&format!("c_{}", f.name.rsplit("::").next().unwrap())),
                ),
                user: (&stats as *const Stats).cast_mut().cast(),
                flags: config::NATIVE_FFI,
            })
            .collect();
        // SAFETY: matching image, retained through the linked owner's borrow.
        let linked = unsafe {
            object.bind(
                &library,
                |image, name| image.address(name),
                Default::default(),
            )
        }
        .unwrap();
        let program = c::Program::new(
            &artifact,
            &linked,
            config::Capacities::from_strides(64, 16, 64).unwrap(),
        )
        .unwrap();
        let factory = NativeFactory::new(&linked, &artifact, Default::default()).unwrap();
        // SAFETY: all calls and created contexts end before binding drop.
        let exports = unsafe { ObjectExports::new(&program, &linked, &factory) }.unwrap();
        let limits = config::LimitsDescriptor {
            frames: 8,
            ..Default::default()
        };
        let config = ProgramConfig {
            version: 1,
            size: 80,
            allocator: &allocator,
            limits: &limits,
            imports: imports.as_ptr(),
            import_count: imports.len() as u64,
            artifact: artifact.digest().0,
            flags: 0,
        };
        unsafe {
            let mut context = std::ptr::null_mut();
            assert_eq!(exports.create()(&config, &mut context), 0);
            let mut output = 77;
            let status = entry(context, &mut output);
            if let Some(value) = expected {
                assert_eq!((status, output), (0, value));
            } else {
                assert_ne!(status, 0);
                assert_eq!(output, 77);
            }
            assert_eq!(exports.destroy()(context), 0);
        }
        assert!(stats.effects.get() > 0);
        assert_eq!(stats.allocations.get(), stats.frees.get());
        assert_eq!(stats.wrong.get(), 0);
    }
}

#[test]
fn linked_c_object_executes_actual_c_source_exports_direct_and_response() {
    for response in [false, true] {
        let text = include_str!("../../examples/analysis/c_exports.lang");
        let owner = source(text);
        let artifact = artifact(text, target(), Limits::default()).unwrap();
        let types = owner
            .mir()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let object = n::emit_c_object_for_target(
            owner.failure_flow(),
            &layouts,
            &artifact,
            &lang_codegen_clif::Triple::host(),
            Default::default(),
            4096,
        )
        .unwrap();
        let increment = artifact
            .functions()
            .iter()
            .find(|f| f.name == "increment")
            .unwrap();
        let fail = artifact
            .functions()
            .iter()
            .find(|f| f.name == "fail")
            .unwrap();
        let extra = format!(
            "\nEXPORT uint32_t c_linked_increment(lang_context *context, int64_t value, int64_t *out) {{ return {}(context, value, out); }}\nEXPORT uint32_t c_linked_fail(lang_context *context, int64_t value, int64_t *out) {{ return {}(context, value, out); }}\n",
            increment.symbol, fail.symbol
        );
        let library = Library::with_object(&artifact, &extra, Some((&object, response)));
        let slot_address = library
            .address(object.slot_symbol())
            .unwrap()
            .cast::<*const c_void>();
        #[repr(C)]
        struct InvalidHeader {
            version: u32,
            size: u32,
            data: usize,
            callbacks: [usize; 4],
        }
        let invalid_headers = [
            InvalidHeader {
                version: 0,
                size: 48,
                data: 1,
                callbacks: [1; 4],
            },
            InvalidHeader {
                version: 1,
                size: 40,
                data: 1,
                callbacks: [1; 4],
            },
            InvalidHeader {
                version: 1,
                size: 48,
                data: 0,
                callbacks: [1; 4],
            },
            InvalidHeader {
                version: 1,
                size: 48,
                data: 1,
                callbacks: [0; 4],
            },
        ];
        // SAFETY: these actual C fixture definitions use the generated header
        // and retain their image below through every invocation.
        let increment_c: ScalarExport =
            unsafe { std::mem::transmute(library.function("c_linked_increment")) };
        let fail_c: ScalarExport =
            unsafe { std::mem::transmute(library.function("c_linked_fail")) };
        let mut untouched = 91;
        // Unbound wrappers refuse before reading an opaque context or output.
        assert_eq!(
            unsafe { increment_c(std::ptr::null_mut(), 1, &mut untouched) },
            1
        );
        assert_eq!(untouched, 91);
        for header in &invalid_headers {
            // SAFETY: a real complete immutable 48-byte header is retained
            // throughout the synchronous rejection. Guard failure prevents
            // dereferencing its deliberately invalid data/code pointer words.
            unsafe {
                slot_address
                    .as_ptr()
                    .write((header as *const InvalidHeader).cast());
            }
            assert_eq!(
                unsafe { increment_c(std::ptr::null_mut(), 1, &mut untouched) },
                1
            );
            assert_eq!(untouched, 91);
        }
        unsafe {
            slot_address.as_ptr().write(std::ptr::null());
        }
        // SAFETY: this exact object/header was linked into the retained fixture;
        // the resolver returns its real code/data symbols without substitution.
        let linked = unsafe {
            object.bind(
                library,
                |image, name| image.address(name),
                Default::default(),
            )
        }
        .unwrap();
        let program = c::Program::new(
            &artifact,
            &linked,
            config::Capacities::from_strides(64, 16, 64).unwrap(),
        )
        .unwrap();
        let factory = NativeFactory::new(&linked, &artifact, Default::default()).unwrap();
        // SAFETY: matched image, single owner thread, all contexts destroyed
        // before binding drop. No call or slot mutation overlaps binding changes.
        let exports = unsafe { ObjectExports::new(&program, &linked, &factory) }.unwrap();
        let active_header = unsafe { slot_address.as_ptr().read() };
        assert!(unsafe { ObjectExports::new(&program, &linked, &factory) }.is_err());
        assert_eq!(unsafe { slot_address.as_ptr().read() }, active_header);
        let another = c::Program::new(
            &artifact,
            &owner,
            config::Capacities::from_strides(64, 16, 64).unwrap(),
        )
        .unwrap();
        assert!(unsafe { ObjectExports::new(&another, &linked, &factory) }.is_err());
        let stats = Stats::default();
        // Callback/provider fixture is separately retained; the generated
        // source body and wrappers themselves reside only in the linked object.
        let callbacks = Library::new(&artifact);
        let allocator = callbacks.allocator(&stats);
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
        unsafe {
            let mut context = std::ptr::null_mut();
            assert_eq!(exports.create()(&config, &mut context), 0);
            let i = artifact
                .functions()
                .iter()
                .position(|f| f.name == "increment")
                .unwrap();
            assert!(exports.symbol(i).is_some());
            let mut output = 77;
            assert_eq!(increment_c(context, 41, &mut output), 0);
            assert_eq!(output, 42);
            let i = artifact
                .functions()
                .iter()
                .position(|f| f.name == "fail")
                .unwrap();
            assert!(exports.symbol(i).is_some());
            assert_ne!(fail_c(context, -31, &mut output), 0);
            assert_eq!(output, 42);
            assert_eq!(exports.destroy()(context), 0);
        }
        assert_eq!(stats.allocations.get(), stats.frees.get());
        drop(exports);
        assert_eq!(
            unsafe { increment_c(std::ptr::null_mut(), 1, &mut untouched) },
            1
        );
        assert_eq!(untouched, 91);
        let rebound = unsafe { ObjectExports::new(&program, &linked, &factory) }.unwrap();
        let mut context = std::ptr::null_mut();
        assert_eq!(unsafe { rebound.create()(&config, &mut context) }, 0);
        assert_eq!(unsafe { increment_c(context, 8, &mut untouched) }, 0);
        assert_eq!(untouched, 9);
        assert_eq!(unsafe { rebound.destroy()(context) }, 0);
        assert_eq!(stats.allocations.get(), stats.frees.get());
    }
}

fn case(artifact: &Artifact, id: lang_runtime::c_abi::TypeId, name: &str) -> usize {
    let Kind::Enum { cases, .. } = &artifact.types()[id.0].kind else {
        panic!("enum metadata")
    };
    cases.iter().position(|c| c.name == name).unwrap()
}

#[test]
fn linked_c_object_round_trips_aggregates_and_refuses_bad_c_values() {
    let text = include_str!("../../examples/analysis/c_aggregate_exports.lang");
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
    let pair = artifact
        .functions()
        .iter()
        .position(|f| f.name == "increment_pair")
        .unwrap();
    let echo = artifact
        .functions()
        .iter()
        .position(|f| f.name == "echo_result")
        .unwrap();
    let fail = artifact
        .functions()
        .iter()
        .position(|f| f.name == "fail_pair")
        .unwrap();
    let pair_type = &artifact.types()[artifact.functions()[pair].result.0];
    let result_id = artifact.functions()[echo].result;
    let result_type = &artifact.types()[result_id.0];
    let ok = case(&artifact, result_id, "Ok");
    let Kind::Enum { cases: results, .. } = &result_type.kind else {
        unreachable!()
    };
    let option_id = results[ok].payload[0];
    let some = case(&artifact, option_id, "Some");
    let none = case(&artifact, option_id, "None");
    let extra = format!(
        r#"
typedef uint32_t (*fixture_create)(const lang_program_config *, lang_context **);
typedef uint32_t (*fixture_destroy)(lang_context *);
typedef uint32_t (*fixture_query)(lang_context *, lang_error_summary *);
typedef uint32_t (*pair_export)(lang_context *, const lang_type_{pair} *, lang_type_{pair} *);
typedef uint32_t (*sum_export)(lang_context *, const lang_type_{result} *, lang_type_{result} *);
EXPORT uint32_t c_aggregate_exports(fixture_create create, fixture_destroy destroy, fixture_query query,
                                  pair_export increment, sum_export echo, pair_export fail,
                                  const lang_program_config *config) {{
    lang_context *context = NULL;
    lang_type_{pair} value;
    lang_type_{pair} output;
    lang_type_{pair} sentinel;
    lang_type_{result} input_sum;
    lang_type_{result} output_sum;
    lang_type_{result} sentinel_sum;
    lang_error_summary summary;
    uint32_t status;
    if (create(config, &context) != 0) return 91;
    /* Ordinary Pair padding remains indeterminate. Only fields may be read. */
    value.f_number = 41; value.f_flag = 1;
    status = increment(context, &value, &output);
    if (status != 0) return 200 + status;
    if (output.f_number != 42 || output.f_flag != 1) return 92;
    if (increment(context, &value, &value) != LANG_STATUS_INVALID || value.f_number != 41 || value.f_flag != 1) return 93;
    memset(&output, 0x5a, sizeof(output)); memcpy(&sentinel, &output, sizeof(output));
    value.f_flag = 2;
    if (increment(context, &value, &output) != LANG_STATUS_INVALID || memcmp(&output, &sentinel, sizeof(output))) return 94;
    value.f_flag = 1;
    memset(&input_sum, 0, sizeof(input_sum));
    input_sum.tag = {ok}; input_sum.payload.c{ok}.p0.tag = {some};
    input_sum.payload.c{ok}.p0.payload.c{some}.p0.f_number = 84;
    input_sum.payload.c{ok}.p0.payload.c{some}.p0.f_flag = 1;
    if (echo(context, &input_sum, &output_sum) != 0 || memcmp(&input_sum, &output_sum, sizeof(input_sum))) return 95;
    memset(&input_sum, 0, sizeof(input_sum));
    input_sum.tag = {ok}; input_sum.payload.c{ok}.p0.tag = {none};
    if (echo(context, &input_sum, &output_sum) != 0 || memcmp(&input_sum, &output_sum, sizeof(input_sum))) return 96;
    memset(&output_sum, 0x5a, sizeof(output_sum)); memcpy(&sentinel_sum, &output_sum, sizeof(output_sum));
    ((uint8_t *)&input_sum.payload.c{ok}.p0.payload)[1] = 1;
    if (echo(context, &input_sum, &output_sum) != LANG_STATUS_INVALID || memcmp(&output_sum, &sentinel_sum, sizeof(output_sum))) return 97;
    value.f_number = -42;
    if (fail(context, &value, &output) != LANG_STATUS_PANIC || memcmp(&output, &sentinel, sizeof(output))) return 98;
    if (query(context, &summary) != 0 || summary.status != LANG_STATUS_PANIC || summary.panic_code != -42) return 99;
    value.f_number = 41;
    if (increment(context, &value, &output) != 0 || output.f_number != 42) return 100;
    if (query(context, &summary) != 0 || summary.status != 0) return 101;
    if (destroy(context) != 0) return 102;
    return 0;
}}
"#,
        pair = pair_type.digest,
        result = result_type.digest
    );
    let object = n::emit_c_object_for_target(
        owner.failure_flow(),
        &layouts,
        &artifact,
        &lang_codegen_clif::Triple::host(),
        Default::default(),
        4096,
    )
    .unwrap();
    let library = Library::with_object(&artifact, &extra, Some((&object, false)));
    let exercise_address = library.function("c_aggregate_exports");
    // SAFETY: matching trusted image and all actual code/data dependencies.
    let compiled = unsafe {
        object.bind(
            library,
            |image, name| image.address(name),
            Default::default(),
        )
    }
    .unwrap();
    let program = c::Program::new(
        &artifact,
        &compiled,
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let factory = NativeFactory::new(&compiled, &artifact, Default::default()).unwrap();
    // SAFETY: single owner thread, every context destroyed before drop.
    let exports = unsafe { ObjectExports::new(&program, &compiled, &factory) }.unwrap();
    let callbacks = Library::new(&artifact);
    let stats = Stats::default();
    let allocator = callbacks.allocator(&stats);
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
    type ObjectExport = unsafe extern "C" fn(*mut c_void, *const u8, *mut u8) -> u32;
    type Exercise = unsafe extern "C" fn(
        raw::Create,
        raw::Destroy,
        raw::LastError,
        ObjectExport,
        ObjectExport,
        ObjectExport,
        *const ProgramConfig,
    ) -> u32;
    // SAFETY: C compiled the exact metadata-derived Pair/Result prototypes;
    // erased pointee types preserve their native pointer calling convention.
    // All actual C objects and code/allocator/context owners remain live.
    unsafe {
        let pair = std::mem::transmute::<*const u8, ObjectExport>(exports.symbol(pair).unwrap().1);
        let echo = std::mem::transmute::<*const u8, ObjectExport>(exports.symbol(echo).unwrap().1);
        let fail = std::mem::transmute::<*const u8, ObjectExport>(exports.symbol(fail).unwrap().1);
        let exercise = std::mem::transmute::<config::ErasedFunction, Exercise>(exercise_address);
        assert_eq!(
            exercise(
                exports.create(),
                exports.destroy(),
                exports.last_error(),
                pair,
                echo,
                fail,
                &config
            ),
            0
        );
    }
    assert!(stats.allocations.get() > 0);
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert_eq!(stats.wrong.get(), 0);
}

#[test]
fn linked_c_object_executes_owner_recursion_original_context_imports_and_panic() {
    let text = include_str!("../../examples/analysis/c_owned_exports.lang");
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
    let object = n::emit_c_object_for_target(
        owner.failure_flow(),
        &layouts,
        &artifact,
        &lang_codegen_clif::Triple::host(),
        Default::default(),
        4096,
    )
    .unwrap();
    let library = owned_context_library(&artifact, &object);
    exercise_owned_context(&artifact, object, library);
}

#[test]
fn persisted_c_object_restores_original_context_allocator_owner_calls_and_panic() {
    let (library, names) = {
        let text = include_str!("../../examples/analysis/c_owned_exports.lang");
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
        let object = n::emit_c_object_for_target(
            owner.failure_flow(),
            &layouts,
            &artifact,
            &lang_codegen_clif::Triple::host(),
            Default::default(),
            4096,
        )
        .unwrap();
        let library = owned_context_library(&artifact, &object);
        let names = (
            object.slot_symbol().to_owned(),
            object.support_symbols().clone(),
            object.export_symbols().to_vec(),
            object.thunk_symbols().to_vec(),
        );
        let record = object.into_recorded(Default::default()).unwrap();
        drop(layouts);
        let table = owner
            .into_failure_flow()
            .into_source()
            .into_activity()
            .into_moved()
            .into_typed()
            .into_table();
        let semantic = lang_bytecode::ownership::encode(table, None, Default::default()).unwrap();
        assert_eq!(&semantic[8..10], &4u16.to_le_bytes());
        let record = record
            .into_record()
            .encode(&semantic, Default::default())
            .unwrap();
        std::fs::write(library.directory.join("program.lbc"), semantic).unwrap();
        std::fs::write(library.directory.join("program.native"), record).unwrap();
        (library, names)
    };
    // All producer IR, layouts, C metadata and object wrappers have been dropped.
    // The original already-linked image is retained; startup emits no code.
    let semantic = std::fs::read(library.directory.join("program.lbc")).unwrap();
    let record = std::fs::read(library.directory.join("program.native")).unwrap();
    let decoded = lang_bytecode::ownership::decode(&semantic, Default::default()).unwrap();
    let flow = decoded.flow();
    let declarations = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    let layouts = l::Layouts::compile(declarations.types(), Default::default()).unwrap();
    let artifact = Artifact::compile(declarations, target(), Default::default()).unwrap();
    let record =
        lang_codegen_clif::native_record::Record::decode(&record, &semantic, Default::default())
            .unwrap();
    let object = n::RecordedObject::from_record(record)
        .unwrap()
        .prepare_c(flow, &layouts, &artifact, Default::default(), 4096)
        .unwrap();
    assert_eq!(object.slot_symbol(), names.0);
    assert_eq!(object.support_symbols(), &names.1);
    assert_eq!(object.export_symbols(), names.2);
    assert_eq!(object.thunk_symbols(), names.3);
    exercise_owned_context(&artifact, object, library);
}

fn owned_context_library(artifact: &Artifact, object: &n::CObjectArtifact<'_>) -> Library {
    assert_eq!(object.thunk_symbols().len(), 1);
    let owned_symbol = &artifact
        .functions()
        .iter()
        .find(|f| f.name == "owned")
        .unwrap()
        .symbol;
    let extra = format!(
        "{}\nstatic lang_context *expected_context;\nEXPORT uint32_t c_same_context(lang_context *context, void *user, int64_t value, int64_t *output) {{ if (context != expected_context) return 81; return c_increment(context, user, value, output); }}\nEXPORT uint32_t c_owned_forward(lang_context *context, int64_t value, int64_t *output) {{ expected_context = context; return {owned_symbol}(context, value, output); }}\n",
        include_str!("c_exports_native.c")
    );
    Library::with_object(artifact, &extra, Some((object, true)))
}

fn exercise_owned_context(artifact: &Artifact, object: n::CObjectArtifact<'_>, library: Library) {
    let exercise_address = library.function("c_exports_exercise");
    let callback_address = library.function("c_same_context");
    let owned_address = library.function("c_owned_forward");
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
    // SAFETY: all matching code/data symbols resolve from the retained image.
    let compiled = unsafe {
        object.bind(
            library,
            |image, name| image.address(name),
            Default::default(),
        )
    }
    .unwrap();
    let program = c::Program::new(
        artifact,
        &compiled,
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let factory = NativeFactory::new(&compiled, artifact, Default::default()).unwrap();
    // SAFETY: owner-thread, no active call/context at binding destruction.
    let exports = unsafe { ObjectExports::new(&program, &compiled, &factory) }.unwrap();
    let limits = config::LimitsDescriptor {
        frames: 8,
        ..Default::default()
    };
    let import = artifact
        .functions()
        .iter()
        .find(|f| f.name == "increment")
        .unwrap();
    let binding = raw::ImportBinding {
        version: 1,
        size: 80,
        name: import.name.as_ptr(),
        name_bytes: import.name.len() as u64,
        signature: import.digest.0,
        function: Some(callback_address),
        user: (&stats as *const Stats).cast_mut().cast(),
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
    // Restored metadata grants neither callback authority nor a signature.
    for denied in [
        raw::ImportBinding {
            flags: 0,
            ..binding
        },
        raw::ImportBinding {
            signature: [0; 32],
            ..binding
        },
    ] {
        let denied_config = ProgramConfig {
            imports: &denied,
            ..config
        };
        let mut context = std::ptr::null_mut();
        // SAFETY: complete live descriptors; only grant/signature is invalid.
        assert_ne!(unsafe { exports.create()(&denied_config, &mut context) }, 0);
        assert!(context.is_null());
        assert_eq!(stats.allocations.get(), 0);
        assert_eq!(stats.effects.get(), 0);
    }
    let fail = artifact
        .functions()
        .iter()
        .position(|f| f.name == "fail_owned")
        .unwrap();
    type Exercise = unsafe extern "C" fn(
        raw::Create,
        raw::Destroy,
        raw::LastError,
        ScalarExport,
        ScalarExport,
        *const ProgramConfig,
    ) -> u32;
    // SAFETY: exact generated scalar profiles and compiled C fixture prototype;
    // all code, actual objects, allocator/user and opaque contexts stay live.
    unsafe {
        let owned = std::mem::transmute::<config::ErasedFunction, ScalarExport>(owned_address);
        let fail = std::mem::transmute::<*const u8, ScalarExport>(exports.symbol(fail).unwrap().1);
        let exercise = std::mem::transmute::<config::ErasedFunction, Exercise>(exercise_address);
        assert_eq!(
            exercise(
                exports.create(),
                exports.destroy(),
                exports.last_error(),
                owned,
                fail,
                &config
            ),
            0
        );
    }
    assert_eq!(stats.effects.get(), 2);
    assert!(stats.allocations.get() > 0);
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert_eq!(stats.wrong.get(), 0);
}

#[test]
fn persisted_c_object_retires_every_fuel_and_provider_failure_prefix() {
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
    let records::Stored {
        decoded,
        record,
        library,
    } = records::stored(&text, |_| records::ZERO_C.to_owned(), false);
    let flow = decoded.flow();
    let declarations = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations();
    let layouts = l::Layouts::compile(declarations.types(), Default::default()).unwrap();
    let artifact = Artifact::compile(declarations, target(), Default::default()).unwrap();
    let object = record
        .prepare_c(flow, &layouts, &artifact, Default::default(), 4096)
        .unwrap();
    let once_address = library.function("c_zero_once");
    let callbacks = Library::new(&artifact);
    // SAFETY: exact matched object image and actual symbol addresses.
    let compiled = unsafe {
        object.bind(
            library,
            |image, name| image.address(name),
            Default::default(),
        )
    }
    .unwrap();
    let program = c::Program::new(
        &artifact,
        &compiled,
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let factory = NativeFactory::new(&compiled, &artifact, Default::default()).unwrap();
    // SAFETY: all created contexts are destroyed synchronously below.
    let exports = unsafe { ObjectExports::new(&program, &compiled, &factory) }.unwrap();
    type Zero = unsafe extern "C" fn(*mut c_void, *mut i64) -> u32;
    type ZeroOnce = unsafe extern "C" fn(Zero, *mut c_void, *mut i64) -> u32;
    let index = artifact
        .functions()
        .iter()
        .position(|f| f.name == "entry")
        .unwrap();
    // SAFETY: exact generated zero-argument profile and compiled C fixture.
    let entry = unsafe { std::mem::transmute::<*const u8, Zero>(exports.symbol(index).unwrap().1) };
    let once = unsafe { std::mem::transmute::<config::ErasedFunction, ZeroOnce>(once_address) };
    let run = |limits: config::LimitsDescriptor, refuse: u64, success: bool| {
        let stats = Stats::default();
        stats.refuse_at.set(refuse);
        let allocator = callbacks.allocator(&stats);
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
            assert_eq!(exports.create()(&config, &mut context), 0);
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
                assert_eq!(exports.last_error()(context, &mut summary), 0);
                assert_eq!(summary.status, status);
            }
            assert_eq!(exports.destroy()(context), 0);
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

#[test]
fn linked_c_object_accepts_the_full_256_mixed_parameter_profile() {
    let mut parameters = Vec::new();
    let mut body = String::from("let mut s0: i64 = 0;\n");
    let mut current = String::from("s0");
    let mut next = 1;
    let mut expected = 0i64;
    for i in 0usize..256 {
        let ty = ["i64", "bool", "core::Unit", "Pair"][i % 4];
        parameters.push(format!("p{i}: {ty}"));
        let inputs = match i % 4 {
            0 => {
                expected += (i + 1) as i64;
                vec![format!("p{i}")]
            }
            1 => {
                expected += i64::from(i % 8 == 1);
                vec![format!("truth(p{i})")]
            }
            2 => Vec::new(),
            _ => {
                expected += (i + 1) as i64 + i64::from(i % 8 == 3);
                vec![format!("p{i}.number"), format!("truth(p{i}.flag)")]
            }
        };
        for value in inputs {
            let target = format!("s{next}");
            body.push_str(&format!("let mut {target}: i64 = {current} + {value};\n"));
            current = target;
            next += 1;
        }
    }
    let text = format!(
        "pub struct Pair {{ number: i64, flag: bool }} fn truth(value: bool) -> i64 {{ if value {{ return 1; }} return 0; }} pub extern \"C\" fn many({}) -> i64 {{ {body} return {current}; }}",
        parameters.join(", ")
    );
    let owner = source(&text);
    let native_limits = m::Limits::default();
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let artifact = Artifact::compile(
        owner
            .mir()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations(),
        target(),
        Limits::default(),
    )
    .unwrap();
    let index = artifact
        .functions()
        .iter()
        .position(|f| f.name == "many")
        .unwrap();
    let function = &artifact.functions()[index];
    assert_eq!(function.parameters.len(), 256);
    let mut c_parameters = Vec::new();
    let mut arguments = Vec::new();
    let mut values = String::new();
    for (i, id) in function.parameters.iter().enumerate() {
        let ty = &artifact.types()[id.0];
        match ty.kind {
            Kind::I64 => {
                c_parameters.push("int64_t".into());
                arguments.push(format!("(int64_t){}", i + 1));
            }
            Kind::Bool => {
                c_parameters.push("uint8_t".into());
                arguments.push(format!("(uint8_t){}", u8::from(i % 8 == 1)));
            }
            Kind::Unit => {
                c_parameters.push(format!("const lang_type_{} *", ty.digest));
                values.push_str(&format!(
                    "lang_type_{} p{i}; p{i}.reserved = 0;\n",
                    ty.digest
                ));
                arguments.push(format!("&p{i}"));
            }
            Kind::Record { .. } => {
                c_parameters.push(format!("const lang_type_{} *", ty.digest));
                values.push_str(&format!(
                    "lang_type_{} p{i}; p{i}.f_number = {}; p{i}.f_flag = {};\n",
                    ty.digest,
                    i + 1,
                    u8::from(i % 8 == 3)
                ));
                arguments.push(format!("&p{i}"));
            }
            _ => unreachable!(),
        }
    }
    let extra = format!(
        r#"
typedef uint32_t (*fixture_many)(lang_context *, {parameters}, int64_t *);
EXPORT uint32_t c_many(fixture_many entry, lang_context *context, int64_t *output) {{
    {values}
    return entry(context, {arguments}, output);
}}
"#,
        parameters = c_parameters.join(", "),
        arguments = arguments.join(", ")
    );
    let object = n::emit_c_object_for_target(
        owner.failure_flow(),
        &layouts,
        &artifact,
        &lang_codegen_clif::Triple::host(),
        native_limits,
        8192,
    )
    .unwrap();
    let library = Library::with_object(&artifact, &extra, Some((&object, true)));
    // SAFETY: trusted matched object and live unchanged code/data image.
    let compiled =
        unsafe { object.bind(&library, |image, name| image.address(name), native_limits) }.unwrap();
    let program = c::Program::new(
        &artifact,
        &compiled,
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let factory = NativeFactory::new(&compiled, &artifact, native_limits).unwrap();
    // SAFETY: contexts/calls are complete before this binding is dropped.
    let exports = unsafe { ObjectExports::new(&program, &compiled, &factory) }.unwrap();
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
    let limits = config::LimitsDescriptor {
        frames: 2,
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
    type Many = unsafe extern "C" fn(config::ErasedFunction, *mut c_void, *mut i64) -> u32;
    // SAFETY: the actual C function pointer has the full metadata-derived
    // signature; only C invokes it. All objects/code/users remain live.
    unsafe {
        let entry = std::mem::transmute::<*const u8, config::ErasedFunction>(
            exports.symbol(index).unwrap().1,
        );
        let many = std::mem::transmute::<config::ErasedFunction, Many>(library.function("c_many"));
        let mut context = std::ptr::null_mut();
        assert_eq!(exports.create()(&config, &mut context), 0);
        let mut output = 77;
        assert_eq!(many(entry, context, &mut output), 0);
        assert_eq!(output, expected);
        assert_eq!(exports.destroy()(context), 0);
    }
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert_eq!(stats.wrong.get(), 0);
}
