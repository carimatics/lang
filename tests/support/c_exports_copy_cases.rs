use super::*;

fn fields(parts: Vec<Vec<String>>) -> Vec<String> {
    let mut values = vec![parts.iter().map(|p| p[0].as_str()).collect::<String>()];
    for (selected, alternatives) in parts.iter().enumerate() {
        for alternative in alternatives.iter().skip(1) {
            values.push(
                parts
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        if i == selected {
                            alternative.as_str()
                        } else {
                            p[0].as_str()
                        }
                    })
                    .collect(),
            );
        }
    }
    values
}
fn cases(artifact: &Artifact, id: lang_runtime::c_abi::TypeId, value: &str) -> Vec<String> {
    match &artifact.types()[id.0].kind {
        Kind::I64 => ["41", "INT64_MIN", "INT64_MAX"]
            .map(|n| format!("{value} = {n}; "))
            .to_vec(),
        Kind::Bool => [0, 1].map(|n| format!("{value} = {n}; ")).to_vec(),
        Kind::Unit => vec![format!("{value}.reserved = 0; ")],
        Kind::Record {
            fields: members, ..
        } if members.is_empty() => vec![format!("{value}.reserved = 0; ")],
        Kind::Record {
            fields: members, ..
        } => fields(
            members
                .iter()
                .map(|f| cases(artifact, f.ty, &format!("{value}.f_{}", f.name)))
                .collect(),
        ),
        Kind::Enum {
            cases: variants, ..
        } => variants
            .iter()
            .enumerate()
            .flat_map(|(tag, case)| {
                let prefix = format!("{value}.tag = {tag}; ");
                if case.payload.is_empty() {
                    return vec![format!("{prefix}{value}.payload.c{tag}.reserved = 0; ")];
                }
                fields(
                    case.payload
                        .iter()
                        .enumerate()
                        .map(|(i, id)| {
                            cases(artifact, *id, &format!("{value}.payload.c{tag}.p{i}"))
                        })
                        .collect(),
                )
                .into_iter()
                .map(|v| format!("{prefix}{v}"))
                .collect()
            })
            .collect(),
    }
}

#[test]
fn finite_copy_export_matrix_covers_every_nested_case_core_error_and_scalar_extreme() {
    let text = include_str!("../../examples/analysis/c_copy_export_matrix.lang");
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
    let index = artifact
        .functions()
        .iter()
        .position(|f| f.name == "roundtrip")
        .unwrap();
    let id = artifact.functions()[index].result;
    let variants = cases(&artifact, id, "input");
    assert!(
        variants.len() > 50,
        "all nested sum cases, not just the default path"
    );
    let mut body = String::new();
    for (index, value) in variants.iter().enumerate() {
        body.push_str(&format!("memset(&input, 0, sizeof(input)); {value} if (entry(context, &input, &output) != 0 || memcmp(&input, &output, sizeof(input))) return {};\n", 1000+index));
    }
    let extra = format!(
        r#"
typedef uint32_t (*fixture_create)(const lang_program_config *, lang_context **);
typedef uint32_t (*fixture_destroy)(lang_context *);
typedef uint32_t (*fixture_copy)(lang_context *, const lang_type_{digest} *, lang_type_{digest} *);
EXPORT uint32_t c_copy_matrix(fixture_create create, fixture_destroy destroy, fixture_copy entry, const lang_program_config *config) {{
    lang_context *context = NULL;
    lang_type_{digest} input;
    lang_type_{digest} output;
    if (create(config, &context) != 0) return 91;
    {body}
    if (destroy(context) != 0) return 92;
    return 0;
}}
"#,
        digest = artifact.types()[id.0].digest
    );
    let library = Library::with_extra(&artifact, &extra);
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
    type Object = unsafe extern "C" fn(*mut c_void, *const u8, *mut u8) -> u32;
    type Matrix =
        unsafe extern "C" fn(raw::Create, raw::Destroy, Object, *const ProgramConfig) -> u32;
    // SAFETY: C checks the metadata-derived actual Envelope pointer profile;
    // all callback/code, allocator/user and actual object owners remain live.
    unsafe {
        let entry = std::mem::transmute::<*const u8, Object>(exports.symbol(index).unwrap().1);
        let matrix = std::mem::transmute::<config::ErasedFunction, Matrix>(
            library.function("c_copy_matrix"),
        );
        assert_eq!(
            matrix(support.create(), support.destroy(), entry, &config),
            0
        );
    }
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert_eq!(stats.wrong.get(), 0);
}

#[test]
fn generated_c_export_accepts_the_full_256_mixed_parameter_profile() {
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
    let compiled = n::CCompiledProgram::compile_imports(
        owner.failure_flow(),
        &layouts,
        &artifact,
        native_limits,
        8192,
    )
    .unwrap();
    let program = c::Program::new(
        &artifact,
        &compiled,
        config::Capacities::from_strides(64, 16, 64).unwrap(),
    )
    .unwrap();
    let support = ContextSupport::new(&program, Default::default()).unwrap();
    let factory =
        lang_codegen_clif::c_abi::runtime::NativeFactory::new(&compiled, &artifact, native_limits)
            .unwrap();
    let exports = NativeExports::new(&program, &compiled, &factory, Default::default()).unwrap();
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
    let library = Library::with_extra(&artifact, &extra);
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
        assert_eq!(support.create()(&config, &mut context), 0);
        let mut output = 77;
        assert_eq!(many(entry, context, &mut output), 0);
        assert_eq!(output, expected);
        assert_eq!(support.destroy()(context), 0);
    }
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert_eq!(stats.wrong.get(), 0);
}
