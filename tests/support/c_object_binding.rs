//! Independent object-format and matched-image boundary checks for RFC 0015.
use super::*;
use object::{Object, ObjectSection, ObjectSymbol};

const BOUNDARY: &str = r#"
extern "C" fn increment(value: i64) -> i64;
pub extern "C" fn entry(value: i64) -> i64 { unsafe { return increment(value); } }
pub extern "C" fn fail(code: i64) -> i64 { return core::panic::<i64>(code); }
"#;

fn target_triple(target: Target) -> lang_codegen_clif::Triple {
    match target {
        Target::MacosAarch64 => "aarch64-apple-darwin",
        Target::WindowsX86_64 => "x86_64-pc-windows-msvc",
    }
    .parse()
    .unwrap()
}
fn native_symbol(target: Target, name: &str) -> String {
    if target == Target::MacosAarch64 {
        format!("_{name}")
    } else {
        name.to_owned()
    }
}

#[test]
fn c_objects_define_slot_entries_and_import_thunks_in_both_native_formats() {
    let owner = source(BOUNDARY);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    for target in [Target::MacosAarch64, Target::WindowsX86_64] {
        let profile = artifact(BOUNDARY, target, Limits::default()).unwrap();
        let emitted = n::emit_c_object_for_target(
            owner.failure_flow(),
            &layouts,
            &profile,
            &target_triple(target),
            Default::default(),
            4096,
        )
        .unwrap();
        let object = object::File::parse(emitted.bytes()).unwrap();
        assert_eq!(
            object.architecture(),
            if target == Target::MacosAarch64 {
                object::Architecture::Aarch64
            } else {
                object::Architecture::X86_64
            }
        );
        assert_eq!(
            object.format(),
            if target == Target::MacosAarch64 {
                object::BinaryFormat::MachO
            } else {
                object::BinaryFormat::Coff
            }
        );
        let slot_name = native_symbol(target, emitted.slot_symbol());
        let slot = object
            .symbols()
            .find(|s| s.name().ok() == Some(slot_name.as_str()))
            .unwrap();
        assert!(slot.is_definition());
        assert_eq!(slot.kind(), object::SymbolKind::Data);
        assert_eq!(slot.address() % 8, 0);
        let slot_section = object
            .section_by_index(slot.section_index().unwrap())
            .unwrap();
        assert!(slot_section.align() >= 8);
        let offset = slot.address() - slot_section.address();
        assert!(offset + 8 <= slot_section.size());
        if slot_section.kind() != object::SectionKind::UninitializedData {
            let bytes = slot_section.data().unwrap();
            assert_eq!(&bytes[offset as usize..offset as usize + 8], &[0; 8]);
        }
        let mut names = emitted.support_symbols().to_vec();
        names.extend(
            emitted
                .export_symbols()
                .iter()
                .map(|(_, name)| name.clone()),
        );
        names.extend(emitted.thunk_symbols().iter().map(|(_, name)| name.clone()));
        assert_eq!(emitted.thunk_symbols().len(), 1);
        for index in 0..owner.failure_flow().functions().len() {
            let function = emitted
                .function(lang_mir::unit::FunctionId(index as u32))
                .unwrap();
            let expected = format!("lang_c_obj1_r3_{}_f{index}", profile.digest());
            assert_eq!(function.metadata().symbol, expected);
            names.push(expected);
        }
        for name in names {
            let name = native_symbol(target, &name);
            let symbol = object
                .symbols()
                .find(|s| s.name().ok() == Some(name.as_str()))
                .unwrap();
            assert!(symbol.is_definition(), "{name}");
            assert_eq!(symbol.kind(), object::SymbolKind::Text, "{name}");
        }
        assert!(
            object
                .sections()
                .any(|section| section.relocations().any(|(_, relocation)| {
                    match relocation.target() {
                        object::RelocationTarget::Symbol(index) => {
                            object.symbol_by_index(index).unwrap().name().ok()
                                == Some(slot_name.as_str())
                        }
                        object::RelocationTarget::Section(index) => {
                            Some(index) == slot.section_index()
                        }
                        _ => false,
                    }
                })),
            "public wrappers must relocate their slot, rather than embed a process address"
        );
        // Both public wrappers and typed imports are object-resident. No
        // runtime JIT support entry may be a missing link dependency.
        for symbol in object.symbols().filter(|s| s.is_undefined()) {
            let name = symbol.name().unwrap().trim_start_matches('_');
            assert!(
                !name.starts_with("lang_c_"),
                "unexpected runtime code dependency: {name}"
            );
        }
    }
}

#[test]
fn c_object_binding_refuses_every_missing_symbol_and_wrong_target_before_publication() {
    let owner = source(BOUNDARY);
    let types = owner
        .mir()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let profile = artifact(BOUNDARY, target(), Limits::default()).unwrap();
    let emit = || {
        n::emit_c_object_for_target(
            owner.failure_flow(),
            &layouts,
            &profile,
            &lang_codegen_clif::Triple::host(),
            Default::default(),
            4096,
        )
        .unwrap()
    };
    let template = emit();
    let mut required = vec![template.slot_symbol().to_owned()];
    required.extend(template.support_symbols().iter().cloned());
    required.extend(
        template
            .export_symbols()
            .iter()
            .map(|(_, name)| name.clone()),
    );
    required.extend(
        template
            .thunk_symbols()
            .iter()
            .map(|(_, name)| name.clone()),
    );
    for index in 0..owner.failure_flow().functions().len() {
        required.push(
            template
                .function(lang_mir::unit::FunctionId(index as u32))
                .unwrap()
                .metadata()
                .symbol
                .clone(),
        );
    }
    let library = Library::with_object(&profile, "", Some((&template, false)));
    let slot = library
        .address(template.slot_symbol())
        .unwrap()
        .cast::<*const c_void>();
    // SAFETY: a real aligned pointer slot in the retained exact linked image.
    assert!(unsafe { slot.as_ptr().read() }.is_null());
    for missing in required {
        let emitted = emit();
        assert_eq!(
            emitted.bytes(),
            template.bytes(),
            "regenerated metadata must match the exact linked bytes"
        );
        let calls = Cell::new(0);
        // SAFETY: all supplied addresses belong to this exact retained image;
        // intentionally absent lookups return None and cannot publish a binding.
        let result = unsafe {
            emitted.bind(
                &library,
                |image, name| {
                    calls.set(calls.get() + 1);
                    if name == missing {
                        None
                    } else {
                        image.address(name)
                    }
                },
                Default::default(),
            )
        };
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("missing {missing} was accepted"),
        };
        match error {
            n::CObjectError::MissingSymbol(name) => assert_eq!(name, missing),
            n::CObjectError::Codegen(n::Error::MissingLinkedSymbol { symbol, .. }) => {
                assert_eq!(symbol, missing)
            }
            other => panic!("wrong missing-symbol error: {other:?}"),
        }
        assert!(calls.get() > 0);
        assert!(unsafe { slot.as_ptr().read() }.is_null());
    }
    let foreign = if target() == Target::MacosAarch64 {
        Target::WindowsX86_64
    } else {
        Target::MacosAarch64
    };
    let foreign_profile = artifact(BOUNDARY, foreign, Limits::default()).unwrap();
    assert!(matches!(
        n::emit_c_object_for_target(
            owner.failure_flow(),
            &layouts,
            &profile,
            &target_triple(foreign),
            Default::default(),
            4096
        ),
        Err(n::CObjectError::Target)
    ));
    let foreign_object = n::emit_c_object_for_target(
        owner.failure_flow(),
        &layouts,
        &foreign_profile,
        &target_triple(foreign),
        Default::default(),
        4096,
    )
    .unwrap();
    let calls = Cell::new(0);
    // SAFETY: target rejection precedes every resolver access. No foreign
    // address is returned, installed, or invoked in the host process.
    let result = unsafe {
        foreign_object.bind(
            (),
            |_, _| {
                calls.set(calls.get() + 1);
                None
            },
            Default::default(),
        )
    };
    assert!(matches!(result, Err(n::CObjectError::Target)));
    assert_eq!(calls.get(), 0);
    assert!(unsafe { slot.as_ptr().read() }.is_null());
    // SAFETY: complete real resolution from the matching still-live image.
    let linked = unsafe {
        template.bind(
            &library,
            |image, name| image.address(name),
            Default::default(),
        )
    }
    .unwrap();
    assert!(
        unsafe { slot.as_ptr().read() }.is_null(),
        "binding code metadata alone must not activate C entries"
    );
    drop(linked);
}

#[test]
fn linked_c_object_round_trips_every_finite_copy_case_and_scalar_extreme() {
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
    let index = artifact
        .functions()
        .iter()
        .position(|f| f.name == "roundtrip")
        .unwrap();
    let id = artifact.functions()[index].result;
    let variants = super::super::copy_cases::cases(&artifact, id, "input");
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
    let object = n::emit_c_object_for_target(
        owner.failure_flow(),
        &layouts,
        &artifact,
        &lang_codegen_clif::Triple::host(),
        Default::default(),
        4096,
    )
    .unwrap();
    let library = Library::with_object(&artifact, &extra, Some((&object, true)));
    // SAFETY: exact trusted linked bytes, symbols and retained dependencies.
    let compiled = unsafe {
        object.bind(
            &library,
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
    // SAFETY: all calls/contexts end before this owner-thread binding drops.
    let exports = unsafe { ObjectExports::new(&program, &compiled, &factory) }.unwrap();
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
            matrix(exports.create(), exports.destroy(), entry, &config),
            0
        );
    }
    assert_eq!(stats.allocations.get(), stats.frees.get());
    assert_eq!(stats.wrong.get(), 0);
}
