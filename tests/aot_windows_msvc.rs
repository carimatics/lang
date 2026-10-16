#![cfg(all(target_os = "windows", target_arch = "x86_64", target_env = "msvc"))]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use lang::linker::{LinkRequest, LinkTarget, SystemLinker};
use lang::{
    prototype_add_function, prototype_host_allocation_burst_function,
    prototype_host_allocation_churn_function, prototype_host_allocation_function,
    prototype_host_allocation_variable_function, prototype_host_callback_function,
    prototype_min_function, prototype_sum_below_function, prototype_sum_to_function,
};
use lang_codegen_clif::emit_host_object;

const C_HARNESS: &str = r#"
#include <stdint.h>
#include <stdlib.h>

extern int64_t prototype_add(int64_t left, int64_t right);
extern int64_t prototype_host_allocation_burst(int64_t value);
extern int64_t prototype_host_allocation_churn(int64_t value);
extern int64_t prototype_host_allocation(int64_t value);
extern int64_t prototype_host_allocation_variable(int64_t value);
extern int64_t prototype_host_callback(int64_t value);
extern int64_t prototype_min(int64_t left, int64_t right);
extern int64_t prototype_sum_below(int64_t value, int64_t cutoff);
extern int64_t prototype_sum_to(int64_t value);

int64_t lang_host_i64_0(int64_t value) {
    return value * 2;
}

int64_t lang_host_i64_1(int64_t value) {
    int64_t *allocation = malloc(sizeof(*allocation));
    if (allocation == NULL) {
        abort();
    }
    *allocation = value;
    int64_t result = *allocation;
    free(allocation);
    return result;
}

int64_t lang_host_i64_2(int64_t value) {
    int64_t *allocations[64];
    for (int64_t index = 0; index < 64; index++) {
        allocations[index] = malloc(sizeof(*allocations[index]));
        if (allocations[index] == NULL) {
            abort();
        }
        *allocations[index] = value + index;
    }
    int64_t result = 0;
    for (int64_t index = 0; index < 64; index++) {
        result += *allocations[index];
        free(allocations[index]);
    }
    return result;
}

int64_t lang_host_i64_3(int64_t value) {
    int64_t *allocations[64];
    for (int64_t index = 0; index < 64; index++) {
        allocations[index] = malloc(sizeof(*allocations[index]));
        if (allocations[index] == NULL) {
            abort();
        }
        *allocations[index] = value + index;
    }
    int64_t result = 0;
    for (int64_t index = 0; index < 64; index += 2) {
        result += *allocations[index];
        free(allocations[index]);
    }
    for (int64_t index = 0; index < 64; index += 2) {
        allocations[index] = malloc(sizeof(*allocations[index]));
        if (allocations[index] == NULL) {
            abort();
        }
        *allocations[index] = value + 64 + index;
    }
    for (int64_t index = 0; index < 64; index++) {
        result += *allocations[index];
        free(allocations[index]);
    }
    return result;
}

int64_t lang_host_i64_4(int64_t value) {
    int64_t *allocations[16];
    for (int64_t index = 0; index < 16; index++) {
        allocations[index] = malloc((size_t)(index + 1) * sizeof(*allocations[index]));
        if (allocations[index] == NULL) {
            abort();
        }
        allocations[index][0] = value + index;
    }
    int64_t result = 0;
    for (int64_t index = 0; index < 16; index++) {
        result += allocations[index][0];
        free(allocations[index]);
    }
    return result;
}

int main(void) {
    int64_t add = prototype_add(20, 22);
    int64_t allocation = prototype_host_allocation(-9);
    int64_t allocation_burst = prototype_host_allocation_burst(-9);
    int64_t allocation_churn = prototype_host_allocation_churn(-9);
    int64_t allocation_variable = prototype_host_allocation_variable(-9);
    int64_t callback = prototype_host_callback(21);
    int64_t minimum = prototype_min(-7, 3);
    int64_t sum_below = prototype_sum_below(64, 33);
    int64_t sum = prototype_sum_to(64);
    return add == 42 && allocation == -9 && allocation_burst == 1440 && allocation_churn == 4192 && allocation_variable == -24 && callback == 42 && minimum == -7 && sum_below == 528 && sum == 2080 ? 0 : 1;
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
            "lang aot windows test {} {nonce}",
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

fn assert_aot_executable(executable: &Path) {
    let execution = Command::new(executable)
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

#[test]
fn msvc_linker_runs_exported_functions_and_host_callback() {
    let directory = TestDirectory::create();
    let add_object = directory.path().join("prototype_add.obj");
    let allocation_object = directory.path().join("prototype_host_allocation.obj");
    let allocation_burst_object = directory.path().join("prototype_host_allocation_burst.obj");
    let allocation_churn_object = directory.path().join("prototype_host_allocation_churn.obj");
    let allocation_variable_object = directory
        .path()
        .join("prototype_host_allocation_variable.obj");
    let callback_object = directory.path().join("prototype_host_callback.obj");
    let min_object = directory.path().join("prototype_min.obj");
    let sum_below_object = directory.path().join("prototype_sum_below.obj");
    let sum_object = directory.path().join("prototype_sum_to.obj");
    let harness_source = directory.path().join("harness.c");
    let harness_object = directory.path().join("harness.obj");
    let executable = directory.path().join("prototype-aot.exe");
    let response_executable = directory.path().join("prototype-aot-response.exe");
    let response_file = directory.path().join("link arguments.rsp");

    let add = emit_host_object(&prototype_add_function()).expect("emits the add object");
    let allocation = emit_host_object(&prototype_host_allocation_function())
        .expect("emits the host-allocation object");
    let allocation_burst = emit_host_object(&prototype_host_allocation_burst_function())
        .expect("emits the host-allocation burst object");
    let allocation_churn = emit_host_object(&prototype_host_allocation_churn_function())
        .expect("emits the host-allocation churn object");
    let allocation_variable = emit_host_object(&prototype_host_allocation_variable_function())
        .expect("emits the variable host-allocation object");
    let callback = emit_host_object(&prototype_host_callback_function())
        .expect("emits the host-callback object");
    let minimum = emit_host_object(&prototype_min_function()).expect("emits the minimum object");
    let sum_below = emit_host_object(&prototype_sum_below_function())
        .expect("emits the conditional sum object");
    let sum = emit_host_object(&prototype_sum_to_function()).expect("emits the sum object");
    fs::write(&add_object, add.bytes()).expect("writes the add object");
    fs::write(&allocation_object, allocation.bytes()).expect("writes the allocation object");
    fs::write(&allocation_burst_object, allocation_burst.bytes())
        .expect("writes the allocation burst object");
    fs::write(&allocation_churn_object, allocation_churn.bytes())
        .expect("writes the allocation churn object");
    fs::write(&allocation_variable_object, allocation_variable.bytes())
        .expect("writes the variable allocation object");
    fs::write(&callback_object, callback.bytes()).expect("writes the callback object");
    fs::write(&min_object, minimum.bytes()).expect("writes the minimum object");
    fs::write(&sum_below_object, sum_below.bytes()).expect("writes the conditional sum object");
    fs::write(&sum_object, sum.bytes()).expect("writes the sum object");
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
        .with_object(&allocation_object)
        .with_object(&allocation_burst_object)
        .with_object(&allocation_churn_object)
        .with_object(&allocation_variable_object)
        .with_object(&callback_object)
        .with_object(&min_object)
        .with_object(&sum_below_object)
        .with_object(&sum_object);
    let linker = SystemLinker::discover_target(LinkTarget::WindowsX86_64Msvc)
        .expect("discovers link.exe in the MSVC developer environment")
        .with_argument("/NOLOGO");
    linker
        .link(&request)
        .expect("links the AOT objects through link.exe");
    assert_aot_executable(&executable);

    let response_request = LinkRequest::new(&response_executable)
        .with_object(&harness_object)
        .with_object(&add_object)
        .with_object(&allocation_object)
        .with_object(&allocation_burst_object)
        .with_object(&allocation_churn_object)
        .with_object(&allocation_variable_object)
        .with_object(&callback_object)
        .with_object(&min_object)
        .with_object(&sum_below_object)
        .with_object(&sum_object);
    linker
        .link_with_response_file(&response_request, &response_file)
        .expect("links the AOT objects through an MSVC response file");
    assert!(response_file.is_file());
    assert_aot_executable(&response_executable);
}
