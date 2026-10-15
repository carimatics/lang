#![cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use lang::linker::{LinkRequest, SystemLinker};
use lang::{prototype_add_function, prototype_host_callback_function, prototype_min_function};
use lang_codegen_clif::emit_host_object;

const C_HARNESS: &str = r#"
#include <stdint.h>

extern int64_t prototype_add(int64_t left, int64_t right);
extern int64_t prototype_host_callback(int64_t value);
extern int64_t prototype_min(int64_t left, int64_t right);

int64_t lang_host_i64_0(int64_t value) {
    return value * 2;
}

int main(void) {
    int64_t add = prototype_add(20, 22);
    int64_t callback = prototype_host_callback(21);
    int64_t minimum = prototype_min(-7, 3);
    return add == 42 && callback == 42 && minimum == -7 ? 0 : 1;
}
"#;

struct TestDirectory {
    path: PathBuf,
}

impl TestDirectory {
    fn create() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lang-aot-windows-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("creates an isolated Windows AOT test directory");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn msvc_linker_runs_exported_functions_and_host_callback() {
    let directory = TestDirectory::create();
    let add_object = directory.path().join("prototype_add.obj");
    let callback_object = directory.path().join("prototype_host_callback.obj");
    let min_object = directory.path().join("prototype_min.obj");
    let harness_source = directory.path().join("harness.c");
    let harness_object = directory.path().join("harness.obj");
    let executable = directory.path().join("prototype-aot.exe");

    let add = emit_host_object(&prototype_add_function()).expect("emits the add object");
    let callback = emit_host_object(&prototype_host_callback_function())
        .expect("emits the host-callback object");
    let minimum = emit_host_object(&prototype_min_function()).expect("emits the minimum object");
    fs::write(&add_object, add.bytes()).expect("writes the add object");
    fs::write(&callback_object, callback.bytes()).expect("writes the callback object");
    fs::write(&min_object, minimum.bytes()).expect("writes the minimum object");
    fs::write(&harness_source, C_HARNESS).expect("writes the C harness");

    let mut harness_output_argument = OsString::from("/Fo:");
    harness_output_argument.push(&harness_object);
    let compilation = Command::new("cl.exe")
        .arg("/nologo")
        .arg("/c")
        .arg(&harness_source)
        .arg(harness_output_argument)
        .output()
        .expect("runs the MSVC C compiler");
    assert!(
        compilation.status.success(),
        "C harness compilation failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&compilation.stdout),
        String::from_utf8_lossy(&compilation.stderr)
    );

    let request = LinkRequest::new(&executable)
        .with_object(&harness_object)
        .with_object(&add_object)
        .with_object(&callback_object)
        .with_object(&min_object);
    SystemLinker::host()
        .with_argument("/NOLOGO")
        .link(&request)
        .expect("links the AOT objects through link.exe");

    let execution = Command::new(&executable)
        .output()
        .expect("runs the linked Windows AOT executable");
    assert!(
        execution.status.success(),
        "AOT executable failed with status {}:\nstdout:\n{}\nstderr:\n{}",
        execution.status,
        String::from_utf8_lossy(&execution.stdout),
        String::from_utf8_lossy(&execution.stderr)
    );
}
