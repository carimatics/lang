use super::*;

fn case(artifact: &Artifact, id: lang_runtime::c_abi::TypeId, name: &str) -> usize {
    let Kind::Enum { cases, .. } = &artifact.types()[id.0].kind else {
        panic!("enum metadata")
    };
    cases.iter().position(|c| c.name == name).unwrap()
}

#[test]
fn real_c_calls_record_and_nested_sum_exports_with_cleanup_and_atomic_results() {
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
    let factory = lang_codegen_clif::c_abi::runtime::NativeFactory::new(
        &compiled,
        &artifact,
        Default::default(),
    )
    .unwrap();
    let exports = NativeExports::new(&program, &compiled, &factory, Default::default()).unwrap();
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
        let exercise = std::mem::transmute::<config::ErasedFunction, Exercise>(
            library.function("c_aggregate_exports"),
        );
        assert_eq!(
            exercise(
                support.create(),
                support.destroy(),
                support.last_error(),
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
fn real_c_exports_execute_owner_recursion_callbacks_and_nested_panic_cleanup() {
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
    let factory = lang_codegen_clif::c_abi::runtime::NativeFactory::new(
        &compiled,
        &artifact,
        Default::default(),
    )
    .unwrap();
    let exports = NativeExports::new(&program, &compiled, &factory, Default::default()).unwrap();
    let library = Library::with_extra(&artifact, include_str!("c_exports_native.c"));
    let stats = Stats::default();
    let allocator = library.allocator(&stats);
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
        function: Some(library.function("c_increment")),
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
    let owned = artifact
        .functions()
        .iter()
        .position(|f| f.name == "owned")
        .unwrap();
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
        let owned =
            std::mem::transmute::<*const u8, ScalarExport>(exports.symbol(owned).unwrap().1);
        let fail = std::mem::transmute::<*const u8, ScalarExport>(exports.symbol(fail).unwrap().1);
        let exercise = std::mem::transmute::<config::ErasedFunction, Exercise>(
            library.function("c_exports_exercise"),
        );
        assert_eq!(
            exercise(
                support.create(),
                support.destroy(),
                support.last_error(),
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
