//! Trusted tier-1 test image guards; standalone runners own their process image.
use super::*;
#[cfg(test)]
#[path = "native_entry_runtime.rs"]
mod source_runtime;
use std::{
    cell::Cell,
    ffi::{CString, c_void},
    ptr::NonNull,
};
#[cfg(target_os = "macos")]
use std::{
    ffi::{c_char, c_int},
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(
        module: *mut c_void,
        name: *const u8,
    ) -> Option<unsafe extern "system" fn() -> isize>;
}
#[cfg(target_os = "macos")]
struct Directory(PathBuf);
#[cfg(target_os = "macos")]
impl Directory {
    fn new(prefix: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
#[cfg(target_os = "macos")]
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(super) struct Image {
    handle: NonNull<c_void>,
    #[cfg(target_os = "macos")]
    _directory: Option<Directory>,
    dropped: Rc<Cell<bool>>,
}
impl Image {
    #[cfg(target_os = "macos")]
    fn link(bytes: &[u8], dropped: Rc<Cell<bool>>) -> Self {
        let directory = Directory::new("lang-owned-image");
        let object = directory.0.join("guest.o");
        let library = directory.0.join("guest.dylib");
        fs::write(&object, bytes).unwrap();
        let output = Command::new("cc")
            .arg("-dynamiclib")
            .arg(&object)
            .arg("-o")
            .arg(&library)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        let path = CString::new(library.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: this private path contains only our just-linked host object;
        // the terminated path is live for the call. RTLD_NOW resolves it now.
        let handle = NonNull::new(unsafe { dlopen(path.as_ptr(), 2) })
            .expect("loads the linked owned image");
        Self {
            handle,
            _directory: Some(directory),
            dropped,
        }
    }
    pub(super) fn current_executable(dropped: Rc<Cell<bool>>) -> Self {
        #[cfg(target_os = "macos")]
        // SAFETY: Darwin defines a null path as this process's loaded images.
        let handle = NonNull::new(unsafe { dlopen(std::ptr::null(), 2) })
            .expect("opens the current executable image");
        #[cfg(target_os = "windows")]
        // SAFETY: NULL selects the current executable. This borrowed module
        // remains loaded for process lifetime; Drop must not call FreeLibrary.
        let handle = NonNull::new(unsafe { GetModuleHandleW(std::ptr::null()) })
            .expect("gets the current executable module");
        Self {
            handle,
            #[cfg(target_os = "macos")]
            _directory: None,
            dropped,
        }
    }
    pub(super) fn symbol(&self, name: &str) -> Option<NonNull<c_void>> {
        let name = CString::new(name).unwrap();
        // SAFETY: handle is live, and the terminated name survives the lookup.
        // The binder accepts only this artifact's exact private function names.
        #[cfg(target_os = "macos")]
        {
            NonNull::new(unsafe { dlsym(self.handle.as_ptr(), name.as_ptr()) })
        }
        #[cfg(target_os = "windows")]
        {
            // SAFETY: the live process module and terminated exact export name
            // come from this toolchain's object metadata. Do not use ordinals.
            unsafe { GetProcAddress(self.handle.as_ptr(), name.as_ptr().cast()) }
                .and_then(|function| NonNull::new(function as *const () as *mut c_void))
        }
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        // SAFETY: this Image owns one successful dlopen reference. LinkedProgram
        // retains it until all gateway, helper and cleanup calls have returned.
        #[cfg(target_os = "macos")]
        assert_eq!(unsafe { dlclose(self.handle.as_ptr()) }, 0);
        // Windows GetModuleHandleW did not increment the reference count.
        // Its process-owned module is deliberately not released here.
        self.dropped.set(true);
    }
}

#[cfg(test)]
fn record_source(
    text: &str,
) -> (
    n::RecordedObject,
    Vec<lang_codegen_clif::program::FunctionCode>,
) {
    let (record, metadata, semantic) = record_artifact(text);
    let record =
        lang_codegen_clif::native_record::Record::decode(&record, &semantic, Default::default())
            .unwrap();
    (n::RecordedObject::from_record(record).unwrap(), metadata)
}

#[cfg(test)]
fn record_artifact(
    text: &str,
) -> (
    Vec<u8>,
    Vec<lang_codegen_clif::program::FunctionCode>,
    Vec<u8>,
) {
    let producer = source(text);
    let flow = producer.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let entry = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .functions()
        .iter()
        .position(|function| function.name == "main")
        .map(|index| m::FunctionId(index as u32));
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let artifact = n::emit_object_for_target(
        flow,
        &layouts,
        &lang_codegen_clif::Triple::host(),
        Default::default(),
        1000,
    )
    .unwrap();
    let metadata = (0..flow.functions().len())
        .map(|index| {
            artifact
                .function(m::FunctionId(index as u32))
                .unwrap()
                .metadata()
                .clone()
        })
        .collect();
    let record = artifact.into_recorded(Default::default()).unwrap();
    drop(layouts);
    let table = producer
        .into_failure_flow()
        .into_source()
        .into_activity()
        .into_moved()
        .into_typed()
        .into_table();
    let semantic = lang_bytecode::ownership::encode(table, entry, Default::default()).unwrap();
    let record = record
        .into_record()
        .encode(&semantic, Default::default())
        .unwrap();
    (record, metadata, semantic)
}

#[test]
fn detached_owned_records_reject_foreign_layouts_counts_and_preparation_exhaustion() {
    let text = "fn main() -> i64 { let mut value: core::Bytes = core::result_unwrap::<core::Bytes,core::AllocError>(core::bytes_new(core::allocator())); return 7; }";
    let consumer = source(text);
    let flow = consumer.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let (record, _) = record_source(&format!("{text} fn other() -> i64 {{ return 8; }}"));
    assert!(matches!(
        record.prepare(flow, &layouts, Default::default(), 1000),
        Err(n::RecordedError::FunctionCount {
            expected: 1,
            recorded: 2
        })
    ));
    let foreign = source(text);
    let foreign_types = foreign
        .failure_flow()
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let foreign_layouts = l::Layouts::compile(foreign_types, Default::default()).unwrap();
    let (record, _) = record_source(text);
    assert!(matches!(
        record.prepare(flow, &foreign_layouts, Default::default(), 1000),
        Err(n::RecordedError::Preparation(n::Error::Layout(
            l::Error::ForeignLocation
        )))
    ));
    let (record, _) = record_source(text);
    assert!(matches!(
        record.prepare(
            flow,
            &layouts,
            m::Limits {
                allocations: 0,
                ..Default::default()
            },
            1000
        ),
        Err(n::RecordedError::Preparation(_))
    ));
}

#[cfg(target_os = "macos")]
#[test]
fn linked_owned_image_executes_managed_calls_and_keeps_its_code_owner_alive() {
    for (text, version) in [
        (
            include_str!("../../examples/analysis/native_managed_text_work.lang"),
            2u16,
        ),
        (
            include_str!("../../examples/analysis/explicit_panic_owned.lang"),
            3u16,
        ),
    ] {
        // Producer IR/layouts are gone. Startup only decodes semantic data; it
        // performs neither source analysis nor machine-code emission.
        let (files, metadata) = {
            let (record, metadata, semantic) = record_artifact(text);
            let files = Directory::new("lang owned startup");
            fs::write(files.0.join("program.lbc"), &semantic).unwrap();
            fs::write(files.0.join("program.native"), &record).unwrap();
            (files, metadata)
        };
        let semantic = fs::read(files.0.join("program.lbc")).unwrap();
        let record = fs::read(files.0.join("program.native")).unwrap();
        assert_eq!(&semantic[8..10], &version.to_le_bytes());
        let record = lang_codegen_clif::native_record::Record::decode(
            &record,
            &semantic,
            Default::default(),
        )
        .unwrap();
        let original = record.object().to_vec();
        let record = n::RecordedObject::from_record(record).unwrap();
        let loaded = lang_bytecode::ownership::decode(&semantic, Default::default()).unwrap();
        let flow = loaded.flow();
        let types = flow
            .source()
            .activity()
            .moved()
            .typed()
            .places()
            .declarations()
            .types();
        let layouts = l::Layouts::compile(types, Default::default()).unwrap();
        let artifact = record
            .prepare(flow, &layouts, Default::default(), 1000)
            .unwrap();
        assert_eq!(artifact.bytes, original);
        for expected in &metadata {
            assert_eq!(
                artifact.function(expected.function).unwrap().metadata(),
                expected
            );
        }
        let dropped = Rc::new(Cell::new(false));
        let image = Image::link(&artifact.bytes, dropped.clone());
        // SAFETY: Image links these exact bytes for the host, resolves their private
        // symbols, owns the loader reference and cannot be accessed after binding.
        let program = unsafe { artifact.bind(image, Image::symbol, Default::default()) }.unwrap();
        let (normal, _) = control::vm(flow, e::Control::with_limits(control::limits()), None);
        let total = match normal {
            Ok(outcome) => {
                assert_eq!(version, 2);
                assert_eq!(outcome.usage.service_work, 228);
                outcome.usage.service_work
            }
            Err(failure) => {
                assert_eq!(version, 3);
                failure.usage.service_work
            }
        };
        for limit in 0..=total + 1 {
            let control = e::Control::with_limits(control::limits()).and_service_work(Some(limit));
            let actual = execute_controlled(
                flow,
                &layouts,
                &program,
                Run {
                    inputs: &[],
                    fuel: None,
                    max_frames: 8,
                    cells: 1000,
                    refuse: None,
                    refuse_cleanup: false,
                },
                control,
                None,
            );
            let (expected, requests) = control::vm(flow, control, None);
            control::same(&actual, &expected);
            assert_eq!(actual.total_requests, requests);
            assert!(!dropped.get());
        }
        drop(program);
        assert!(dropped.get());
    }
}

#[cfg(target_os = "macos")]
#[test]
fn persisted_copy_records_bind_checked_calls_without_source_or_backend_startup() {
    use lang_codegen_clif::{native_record::Record, program as copy};
    let (semantic, record, expected_metadata) = {
        let analysis = frontend::analyze_virtual(
            SourceMap::new(),
            Path::new("startup-copy.lang"),
            r#"// 日本語
struct Pair { flag: bool, x: i64 }
enum Choice { None, One(Pair) }
fn pack(x: i64) -> Choice { return Choice::One(Pair { x: x, flag: true }); }
fn identity(value: Choice) -> Choice { return value; }
fn main() -> i64 { return 42; }
"#
            .as_bytes(),
            None,
            Default::default(),
            None,
        );
        assert!(
            analysis.diagnostics.is_empty(),
            "{:?}",
            analysis.diagnostics
        );
        let semantic = lang::artifact::compile(
            analysis.result.unwrap(),
            &analysis.sources,
            Default::default(),
            Default::default(),
        )
        .unwrap();
        let lang::artifact::Program::Copy(producer) =
            lang::artifact::decode(&semantic, Default::default()).unwrap()
        else {
            panic!("Copy");
        };
        let object = copy::emit_host_object(producer.verified(), &[], Default::default()).unwrap();
        let metadata = object.functions.clone();
        let record = object
            .into_record(lang_codegen_clif::Triple::host())
            .encode(&semantic, Default::default())
            .unwrap();
        (semantic, record, metadata)
    };
    let files = Directory::new("lang Copy startup");
    fs::write(files.0.join("program.lbc"), &semantic).unwrap();
    fs::write(files.0.join("program.native"), &record).unwrap();
    drop((semantic, record));
    let semantic = fs::read(files.0.join("program.lbc")).unwrap();
    let record = fs::read(files.0.join("program.native")).unwrap();
    // Only serialized files/metadata cross startup. Source IR and producer are gone.
    let lang::artifact::Program::Copy(loaded) =
        lang::artifact::decode(&semantic, Default::default()).unwrap()
    else {
        panic!("Copy");
    };
    let read = || Record::decode(&record, &semantic, Default::default()).unwrap();
    let mut bad = read().into_copy().unwrap();
    bad.functions.clear();
    assert!(matches!(
        bad.into_record(lang_codegen_clif::Triple::host())
            .prepare_copy(loaded.verified(), &[], Default::default()),
        Err(copy::BindError::FunctionCount {
            expected: 3,
            recorded: 0
        })
    ));
    let mut bad = read().into_copy().unwrap();
    bad.functions[0].symbol = "lang_unit_f00".into();
    assert!(matches!(
        bad.into_record(lang_codegen_clif::Triple::host())
            .prepare_copy(loaded.verified(), &[], Default::default()),
        Err(copy::BindError::Metadata { .. })
    ));
    let mut bad = read().into_copy().unwrap();
    let inside = loaded.verified().unit().sources[0]
        .text
        .as_bytes()
        .iter()
        .position(|b| !b.is_ascii())
        .unwrap() as u32
        + 1;
    bad.functions[0].source_ranges[0].span.start = inside;
    bad.functions[0].source_ranges[0].span.end = inside + 1;
    assert!(matches!(
        bad.into_record(lang_codegen_clif::Triple::host())
            .prepare_copy(loaded.verified(), &[], Default::default()),
        Err(copy::BindError::SourceSpan { .. })
    ));
    assert!(matches!(
        read().prepare_copy(
            loaded.verified(),
            &[],
            m::Limits {
                allocations: 0,
                ..Default::default()
            }
        ),
        Err(copy::BindError::Preparation(_))
    ));
    let foreign = read()
        .into_copy()
        .unwrap()
        .into_record("x86_64-pc-windows-msvc".parse().unwrap())
        .prepare_copy(loaded.verified(), &[], Default::default())
        .unwrap();
    let lookups = Cell::new(0);
    // SAFETY: the mismatched-target preflight returns before resolving/calling
    // any address; this resolver provides no executable pointer.
    let result = unsafe {
        foreign.bind(
            (),
            |_, _| {
                lookups.set(lookups.get() + 1);
                None
            },
            Default::default(),
        )
    };
    assert!(matches!(result, Err(copy::BindError::Target)));
    assert_eq!(lookups.get(), 0);

    let missing_dropped = Rc::new(Cell::new(false));
    let prepared = read()
        .prepare_copy(loaded.verified(), &[], Default::default())
        .unwrap();
    let image = Image::link(prepared.bytes(), missing_dropped.clone());
    // SAFETY: a matching retained image is supplied; the resolver intentionally
    // withholds every symbol and cannot publish an invalid address.
    let missing = unsafe { prepared.bind(image, |_, _| None, Default::default()) };
    assert!(matches!(
        missing,
        Err(copy::BindError::MissingSymbol { .. })
    ));
    assert!(missing_dropped.get());

    let prepared = read()
        .prepare_copy(loaded.verified(), &[], Default::default())
        .unwrap();
    assert_eq!(prepared.metadata(), expected_metadata);
    let dropped = Rc::new(Cell::new(false));
    let image = Image::link(prepared.bytes(), dropped.clone());
    // SAFETY: this image is linked from these exact compiler bytes and owns
    // the resolved private cell-buffer entries through every borrowed call.
    let linked = unsafe { prepared.bind(image, Image::symbol, Default::default()) }.unwrap();
    let function = |name: &str| {
        m::FunctionId(
            loaded
                .verified()
                .unit()
                .functions
                .iter()
                .position(|f| f.name == name)
                .unwrap() as u32,
        )
    };
    for n in [i64::MIN, 42, i64::MAX] {
        let packed = linked
            .invoke(function("pack"), &[copy::Value::i64(n)])
            .unwrap();
        let arg = lang_vm::unit::Value::from_cells(&loaded, m::Type::I64, vec![n as u64]).unwrap();
        let expected = lang_vm::unit::execute(
            &loaded,
            function("pack"),
            &[arg],
            &lang_runtime::HostFunctionRegistry::new(),
            &lang_vm::unit::HostBindings::new(),
            Default::default(),
        )
        .unwrap()
        .value;
        assert_eq!(packed.value_type, expected.value_type());
        assert_eq!(
            packed.cells,
            expected
                .cells()
                .iter()
                .map(|v| *v as i64)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            linked
                .invoke(function("identity"), std::slice::from_ref(&packed))
                .unwrap(),
            packed
        );
        assert!(!dropped.get());
    }
    assert_eq!(
        linked.invoke(function("main"), &[]).unwrap(),
        copy::Value::i64(42)
    );
    assert!(matches!(
        linked.invoke(function("pack"), &[copy::Value::boolean(true)]),
        Err(copy::InvokeError::InvalidArguments)
    ));
    drop(linked);
    assert!(dropped.get());
}

#[test]
fn source_runtime_executes_bytes_without_test_runtime_or_c_context() {
    let source = source(include_str!("../../examples/scripts/owned_bytes.lang"));
    let flow = source.failure_flow();
    let types = flow
        .source()
        .activity()
        .moved()
        .typed()
        .places()
        .declarations()
        .types();
    let layouts = l::Layouts::compile(types, Default::default()).unwrap();
    let program = n::CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
    let mut instance = c::Instance::default();
    let control = e::Control::with_limits(control::limits());
    let expected = control::vm(flow, control, None).0.unwrap();
    let actual = n::runtime::execute_entry(
        &program,
        m::FunctionId(0),
        &mut instance,
        &mut allocator,
        n::runtime::Config {
            control,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(actual.value, expected.value);
    assert_eq!(actual.usage, expected.usage);
    assert!(actual.report.primary().is_none());
    assert!(!instance.is_poisoned());
    assert_eq!(allocator.stats().live_allocations, 0);
    assert_eq!(
        allocator.stats().allocations,
        allocator.stats().deallocations
    );
}
