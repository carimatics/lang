//! Persisted C startup: producer code is gone before data is decoded or bound.
use super::super::runtime_cases::{MANAGED_SOURCES, Metered};
use super::*;
use lang_codegen_clif::native_record::Record;
use lang_mir::unit as m;

pub(super) const ZERO_C: &str = r#"
typedef uint32_t (*fixture_zero)(lang_context *, int64_t *);
EXPORT uint32_t c_zero_once(fixture_zero entry, lang_context *context, int64_t *output) {
    return entry(context, output);
}
"#;

pub(super) struct Stored {
    pub decoded: lang_bytecode::ownership::Program,
    pub record: n::RecordedObject,
    pub library: Library,
}
pub(super) fn stored(
    text: &str,
    extra: impl FnOnce(&Artifact) -> String,
    response: bool,
) -> Stored {
    let library = {
        let producer = source(text);
        let flow = producer.failure_flow();
        let declarations = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations();
        let layouts = l::Layouts::compile(declarations.types(), Default::default()).unwrap();
        let artifact = Artifact::compile(declarations, target(), Default::default()).unwrap();
        let object = n::emit_c_object_for_target(
            flow,
            &layouts,
            &artifact,
            &lang_codegen_clif::Triple::host(),
            Default::default(),
            4096,
        )
        .unwrap();
        let library = Library::with_object(&artifact, &extra(&artifact), Some((&object, response)));
        let record = object.into_recorded(Default::default()).unwrap();
        drop(layouts);
        let table = producer
            .into_failure_flow()
            .into_source()
            .into_activity()
            .into_moved()
            .into_typed()
            .into_table();
        let semantic = lang_bytecode::ownership::encode(table, None, Default::default()).unwrap();
        let wire = record
            .into_record()
            .encode(&semantic, Default::default())
            .unwrap();
        std::fs::write(library.directory.join("program.lbc"), semantic).unwrap();
        std::fs::write(library.directory.join("program.native"), wire).unwrap();
        library
    };
    // Source frontend, code emission and C fixture linking all precede teardown.
    let semantic = std::fs::read(library.directory.join("program.lbc")).unwrap();
    let wire = std::fs::read(library.directory.join("program.native")).unwrap();
    let decoded = lang_bytecode::ownership::decode(&semantic, Default::default()).unwrap();
    let record = n::RecordedObject::from_record(
        Record::decode(&wire, &semantic, Default::default()).unwrap(),
    )
    .unwrap();
    Stored {
        decoded,
        record,
        library,
    }
}

#[test]
fn persisted_c_objects_execute_all_managed_families_with_original_allocator_and_vm_usage() {
    for (index, original) in MANAGED_SOURCES.into_iter().enumerate() {
        let vm_source = source(original);
        let mut vm_allocator = a::Allocator::new(a::System, Default::default()).unwrap();
        let vm = lang_vm::unit::owned::execute_entry(
            vm_source.failure_flow(),
            &mut clean::Instance::default(),
            Some(&mut vm_allocator),
            Default::default(),
        )
        .unwrap();
        assert_eq!(vm_allocator.stats().live_allocations, 0);
        let text = original.replacen("fn main()", "pub extern \"C\" fn entry()", 1);
        let Stored {
            decoded,
            record,
            library,
        } = stored(&text, |_| ZERO_C.to_owned(), index % 2 == 0);
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
        let stats = Stats::default();
        let allocator = library.allocator(&stats);
        let once_address = library.function("c_zero_once");
        // SAFETY: unchanged saved object, exact symbols and retained native image.
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
        let factory = Metered {
            inner: NativeFactory::new(&compiled, &artifact, Default::default()).unwrap(),
            usage: Cell::new(e::Usage::default()),
        };
        // SAFETY: owner-thread calls; all contexts are destroyed before unbind.
        let exports = unsafe { ObjectExports::new(&program, &compiled, &factory) }.unwrap();
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
        type Zero = unsafe extern "C" fn(*mut c_void, *mut i64) -> u32;
        type Once = unsafe extern "C" fn(Zero, *mut c_void, *mut i64) -> u32;
        // SAFETY: exact compiler C signatures; image/allocator/config remain live.
        unsafe {
            let entry = std::mem::transmute::<*const u8, Zero>(exports.symbol(entry).unwrap().1);
            let once = std::mem::transmute::<config::ErasedFunction, Once>(once_address);
            let mut context = std::ptr::null_mut();
            assert_eq!(exports.create()(&config, &mut context), 0);
            let mut output = 77;
            assert_eq!(once(entry, context, &mut output), 0, "{text}");
            assert_eq!(output, vm.value, "{text}");
            let actual = factory.usage.get();
            assert_eq!(actual.fuel, vm.usage.fuel, "{text}");
            assert_eq!(actual.service_work, vm.usage.service_work, "{text}");
            assert_eq!(actual.peak_frames, vm.usage.peak_frames, "{text}");
            assert_eq!(actual.peak_live_cells, vm.usage.peak_live_cells, "{text}");
            assert_eq!(exports.destroy()(context), 0);
        }
        assert_eq!(stats.allocations.get(), stats.frees.get(), "{text}");
        assert_eq!(stats.wrong.get(), 0);
    }
}

#[test]
fn c_records_restore_both_target_formats_and_reject_wrong_identity_scope_and_budgets() {
    let text = include_str!("../../examples/analysis/c_owned_exports.lang");
    for target in [Target::MacosAarch64, Target::WindowsX86_64] {
        let (semantic, wire, names) = {
            let producer = source(text);
            let flow = producer.failure_flow();
            let declarations = flow
                .source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations();
            let layouts = l::Layouts::compile(declarations.types(), Default::default()).unwrap();
            let artifact = Artifact::compile(declarations, target, Default::default()).unwrap();
            let triple = match target {
                Target::MacosAarch64 => "aarch64-apple-darwin",
                Target::WindowsX86_64 => "x86_64-pc-windows-msvc",
            }
            .parse()
            .unwrap();
            let object = n::emit_c_object_for_target(
                flow,
                &layouts,
                &artifact,
                &triple,
                Default::default(),
                4096,
            )
            .unwrap();
            let names = (
                object.slot_symbol().to_owned(),
                object.support_symbols().clone(),
                object.export_symbols().to_vec(),
                object.thunk_symbols().to_vec(),
            );
            let record = object.into_recorded(Default::default()).unwrap();
            drop(layouts);
            let table = producer
                .into_failure_flow()
                .into_source()
                .into_activity()
                .into_moved()
                .into_typed()
                .into_table();
            let semantic =
                lang_bytecode::ownership::encode(table, None, Default::default()).unwrap();
            let wire = record
                .into_record()
                .encode(&semantic, Default::default())
                .unwrap();
            (semantic, wire, names)
        };
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
        let artifact = Artifact::compile(declarations, target, Default::default()).unwrap();
        let restore = |wire: &[u8], limits| {
            n::RecordedObject::from_record(
                Record::decode(wire, &semantic, Default::default()).unwrap(),
            )
            .unwrap()
            .prepare_c(flow, &layouts, &artifact, limits, 4096)
        };
        let object = restore(&wire, Default::default()).unwrap();
        assert_eq!(object.slot_symbol(), names.0);
        assert_eq!(object.support_symbols(), &names.1);
        assert_eq!(object.export_symbols(), names.2);
        assert_eq!(object.thunk_symbols(), names.3);
        let usage = object.usage();
        let exact = m::Limits {
            work: usage.work,
            allocations: usage.allocation_bytes,
            elements: usage.elements,
            ..Default::default()
        };
        restore(&wire, exact).unwrap();
        for limits in [
            m::Limits {
                work: usage.work - 1,
                ..exact
            },
            m::Limits {
                allocations: usage.allocation_bytes - 1,
                ..exact
            },
            m::Limits {
                elements: usage.elements - 1,
                ..exact
            },
        ] {
            assert!(restore(&wire, limits).is_err());
        }
        // RFC 0018 freezes target/kind at offsets 16/17 and the C digest at 18.
        // Repair framing when changing kind so decoding itself still succeeds.
        let mut wrong = wire.clone();
        wrong[18] ^= 1;
        assert!(matches!(
            restore(&wrong, Default::default()),
            Err(n::CObjectError::ArtifactDigest { .. })
        ));
        let mut wrong = wire.clone();
        wrong[16] ^= 1;
        assert!(matches!(
            restore(&wrong, Default::default()),
            Err(n::CObjectError::Target)
        ));
        let mut wrong = wire.clone();
        wrong[17] = 3;
        wrong.drain(18..50);
        let payload = (wrong.len() - 16) as u32;
        wrong[12..16].copy_from_slice(&payload.to_le_bytes());
        assert!(matches!(
            restore(&wrong, Default::default()),
            Err(n::CObjectError::RecordScope)
        ));
        let foreign = source(&format!("{text}\nfn extra() -> i64 {{ return 0; }}"));
        let foreign_flow = foreign.failure_flow();
        let foreign_layouts = l::Layouts::compile(
            foreign_flow
                .source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations()
                .types(),
            Default::default(),
        )
        .unwrap();
        let record = n::RecordedObject::from_record(
            Record::decode(&wire, &semantic, Default::default()).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            record.prepare_c(
                foreign_flow,
                &foreign_layouts,
                &artifact,
                Default::default(),
                4096
            ),
            Err(n::CObjectError::Recorded(
                n::RecordedError::FunctionCount { .. }
            ))
        ));
    }
}
