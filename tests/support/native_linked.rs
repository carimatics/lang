//! A trusted macOS test image, linked from the exact current ObjectArtifact.
//! N06 retains standalone target executables and the full platform matrix.
use super::*;
use std::{
    cell::Cell,
    ffi::{CString, c_char, c_int, c_void},
    fs,
    path::PathBuf,
    process::Command,
    ptr::NonNull,
    sync::atomic::{AtomicU64, Ordering},
};

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
}

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
pub(super) struct Image {
    handle: NonNull<c_void>,
    _directory: Directory,
    dropped: Rc<Cell<bool>>,
}
impl Image {
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
            _directory: directory,
            dropped,
        }
    }
    fn symbol(&self, name: &str) -> Option<NonNull<c_void>> {
        let name = CString::new(name).unwrap();
        // SAFETY: handle is live, and the terminated name survives the lookup.
        // The binder accepts only this artifact's exact private function names.
        NonNull::new(unsafe { dlsym(self.handle.as_ptr(), name.as_ptr()) })
    }
}
impl Drop for Image {
    fn drop(&mut self) {
        // SAFETY: this Image owns one successful dlopen reference. LinkedProgram
        // retains it until all gateway, helper and cleanup calls have returned.
        assert_eq!(unsafe { dlclose(self.handle.as_ptr()) }, 0);
        self.dropped.set(true);
    }
}

#[test]
fn linked_owned_image_executes_managed_calls_and_keeps_its_code_owner_alive() {
    let source = source(include_str!(
        "../../examples/analysis/native_managed_text_work.lang"
    ));
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
    let artifact = n::emit_object_for_target(
        flow,
        &layouts,
        &lang_codegen_clif::Triple::host(),
        Default::default(),
        1000,
    )
    .unwrap();
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
