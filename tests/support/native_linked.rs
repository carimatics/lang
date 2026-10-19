//! Trusted tier-1 test image guards; standalone runners own their process image.
use super::*;
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
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang-owned-image-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let directory = Directory(path);
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

fn record_source(
    text: &str,
) -> (
    n::RecordedObject,
    Vec<lang_codegen_clif::program::FunctionCode>,
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
    (
        artifact.into_recorded(Default::default()).unwrap(),
        metadata,
    )
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
    let text = include_str!("../../examples/analysis/native_managed_text_work.lang");
    // The helper's producer IR/layouts are gone before this second analysis.
    let (record, metadata) = record_source(text);
    let original = record.bytes().to_vec();
    let source = source(text);
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
    let total = normal.unwrap().usage.service_work;
    assert_eq!(total, 228);
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
