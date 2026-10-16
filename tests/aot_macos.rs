#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use lang::component_conformance::prototype_scalar_component_function;
use lang::component_resource_conformance::{
    run_prototype_resource_failure_adapter, run_prototype_resource_failure_component,
};
use lang::linker::{LinkRequest, LinkTarget, SystemLinker};
use lang::{
    prototype_add_function, prototype_fibonacci_function, prototype_host_allocation_burst_function,
    prototype_host_allocation_churn_function, prototype_host_allocation_function,
    prototype_host_allocation_variable_function, prototype_host_callback_function,
    prototype_min_function, prototype_sum_below_function, prototype_sum_to_function,
};
use lang_codegen_clif::emit_host_object;

const C_HARNESS: &str = r#"
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

extern int64_t prototype_add(int64_t left, int64_t right);
extern int64_t prototype_fibonacci(int64_t value);
extern int64_t prototype_host_allocation_burst(int64_t value);
extern int64_t prototype_host_allocation_churn(int64_t value);
extern int64_t prototype_host_allocation(int64_t value);
extern int64_t prototype_host_allocation_variable(int64_t value);
extern int64_t prototype_host_callback(int64_t value);
extern int64_t prototype_min(int64_t left, int64_t right);
extern int64_t prototype_sum_below(int64_t value, int64_t cutoff);
extern int64_t prototype_sum_to(int64_t value);
extern int64_t run(int64_t value);

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

int64_t lang_host_i64_5(int64_t value) {
    return value * 2;
}

int main(void) {
    int64_t add = prototype_add(20, 22);
    int64_t fibonacci = prototype_fibonacci(20);
    int64_t allocation = prototype_host_allocation(-9);
    int64_t allocation_burst = prototype_host_allocation_burst(-9);
    int64_t allocation_churn = prototype_host_allocation_churn(-9);
    int64_t allocation_variable = prototype_host_allocation_variable(-9);
    int64_t callback = prototype_host_callback(21);
    int64_t minimum = prototype_min(-7, 4);
    int64_t sum_below = prototype_sum_below(64, 33);
    int64_t sum = prototype_sum_to(64);
    int64_t component = run(21);
    printf("%lld %lld %lld %lld %lld %lld %lld %lld %lld %lld %lld\n", (long long)add, (long long)fibonacci, (long long)allocation, (long long)allocation_burst, (long long)allocation_churn, (long long)allocation_variable, (long long)callback, (long long)minimum, (long long)sum_below, (long long)sum, (long long)component);
    return add == 42 && fibonacci == 6765 && allocation == -9 && allocation_burst == 1440 && allocation_churn == 4192 && allocation_variable == -24 && callback == 42 && minimum == -7 && sum_below == 528 && sum == 2080 && component == 42 ? 0 : 1;
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
        let path =
            std::env::temp_dir().join(format!("lang aot test {} {nonce}", std::process::id()));
        fs::create_dir(&path).expect("creates an isolated AOT test directory");
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

fn assert_aot_executable(executable: &Path) -> i64 {
    let execution = Command::new(executable)
        .output()
        .expect("runs the linked AOT executable");
    assert!(
        execution.status.success(),
        "AOT executable failed with status {}: {}",
        execution.status,
        String::from_utf8_lossy(&execution.stderr)
    );
    assert_eq!(
        execution.stdout,
        b"42 6765 -9 1440 4192 -24 42 -7 528 2080 42\n"
    );
    String::from_utf8(execution.stdout)
        .expect("AOT fixture output is UTF-8")
        .split_whitespace()
        .next_back()
        .expect("AOT fixture prints the component result")
        .parse()
        .expect("component result is an i64")
}

fn assert_dwarf_object(object: &Path, function: &str, expected_positions: &[(u32, u32)]) {
    let verification = Command::new("xcrun")
        .arg("dwarfdump")
        .arg("--verify")
        .arg(object)
        .output()
        .expect("runs dwarfdump verification");
    assert!(
        verification.status.success(),
        "DWARF verification failed for {}:\nstdout:\n{}\nstderr:\n{}",
        object.display(),
        String::from_utf8_lossy(&verification.stdout),
        String::from_utf8_lossy(&verification.stderr)
    );

    let info = Command::new("xcrun")
        .arg("dwarfdump")
        .arg("--debug-info")
        .arg(object)
        .output()
        .expect("reads DWARF debug information");
    assert!(info.status.success());
    let info = String::from_utf8(info.stdout).expect("dwarfdump emits UTF-8");
    assert!(info.contains("benchmarks/fixtures/native-debug.mir"));
    assert!(info.contains(function));

    let lines = Command::new("xcrun")
        .arg("dwarfdump")
        .arg("--debug-line")
        .arg(object)
        .output()
        .expect("reads the DWARF line table");
    assert!(lines.status.success());
    let lines = String::from_utf8(lines.stdout).expect("dwarfdump emits UTF-8");
    let positions = lines
        .lines()
        .filter(|line| line.trim_start().starts_with("0x"))
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            fields.next()?;
            Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
        })
        .collect::<Vec<_>>();
    for expected in expected_positions {
        assert!(
            positions.contains(expected),
            "missing line/column {expected:?} in {}: {positions:?}",
            object.display()
        );
    }
}

fn assert_linked_dsym(executable: &Path, dsym: &Path) {
    let generation = Command::new("xcrun")
        .arg("dsymutil")
        .arg(executable)
        .arg("-o")
        .arg(dsym)
        .output()
        .expect("runs dsymutil");
    assert!(
        generation.status.success(),
        "dSYM generation failed for {}:\nstdout:\n{}\nstderr:\n{}",
        executable.display(),
        String::from_utf8_lossy(&generation.stdout),
        String::from_utf8_lossy(&generation.stderr)
    );

    let verification = Command::new("xcrun")
        .arg("dwarfdump")
        .arg("--verify")
        .arg(dsym)
        .output()
        .expect("verifies the linked dSYM");
    assert!(
        verification.status.success(),
        "linked dSYM verification failed for {}:\nstdout:\n{}\nstderr:\n{}",
        dsym.display(),
        String::from_utf8_lossy(&verification.stdout),
        String::from_utf8_lossy(&verification.stderr)
    );

    let info = Command::new("xcrun")
        .arg("dwarfdump")
        .arg("--debug-info")
        .arg(dsym)
        .output()
        .expect("reads the linked dSYM");
    assert!(info.status.success());
    let info = String::from_utf8(info.stdout).expect("dwarfdump emits UTF-8");
    assert!(info.contains("benchmarks/fixtures/native-debug.mir"));
    for function in ["prototype_add", "prototype_min", "prototype_host_callback"] {
        assert!(info.contains(function), "missing {function} in linked dSYM");
    }
}

#[test]
fn system_linker_runs_exported_functions_and_host_callback() {
    let directory = TestDirectory::create();
    let add_object = directory.path().join("prototype_add.o");
    let fibonacci_object = directory.path().join("prototype_fibonacci.o");
    let allocation_object = directory.path().join("prototype_host_allocation.o");
    let allocation_burst_object = directory.path().join("prototype_host_allocation_burst.o");
    let allocation_churn_object = directory.path().join("prototype_host_allocation_churn.o");
    let allocation_variable_object = directory
        .path()
        .join("prototype_host_allocation_variable.o");
    let callback_object = directory.path().join("prototype_host_callback.o");
    let component_object = directory.path().join("component_scalar.o");
    let min_object = directory.path().join("prototype_min.o");
    let sum_below_object = directory.path().join("prototype_sum_below.o");
    let sum_object = directory.path().join("prototype_sum_to.o");
    let harness_source = directory.path().join("harness.c");
    let harness_object = directory.path().join("harness.o");
    let executable = directory.path().join("prototype-aot");
    let response_executable = directory.path().join("prototype-aot-response");
    let dsym = directory.path().join("prototype-aot.dSYM");
    let response_dsym = directory.path().join("prototype-aot-response.dSYM");
    let response_file = directory.path().join("link arguments.rsp");

    let add = emit_host_object(&prototype_add_function()).expect("emits the add object");
    let fibonacci =
        emit_host_object(&prototype_fibonacci_function()).expect("emits the fibonacci object");
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
    let component = emit_host_object(
        &prototype_scalar_component_function().expect("validates the scalar component contract"),
    )
    .expect("emits the scalar component object");
    let minimum = emit_host_object(&prototype_min_function()).expect("emits the minimum object");
    let sum_below = emit_host_object(&prototype_sum_below_function())
        .expect("emits the conditional sum object");
    let sum = emit_host_object(&prototype_sum_to_function()).expect("emits the sum object");
    fs::write(&add_object, add.bytes()).expect("writes the add object");
    fs::write(&fibonacci_object, fibonacci.bytes()).expect("writes the fibonacci object");
    fs::write(&allocation_object, allocation.bytes()).expect("writes the allocation object");
    fs::write(&allocation_burst_object, allocation_burst.bytes())
        .expect("writes the allocation burst object");
    fs::write(&allocation_churn_object, allocation_churn.bytes())
        .expect("writes the allocation churn object");
    fs::write(&allocation_variable_object, allocation_variable.bytes())
        .expect("writes the variable allocation object");
    fs::write(&callback_object, callback.bytes()).expect("writes the callback object");
    fs::write(&component_object, component.bytes()).expect("writes the component object");
    fs::write(&min_object, minimum.bytes()).expect("writes the minimum object");
    fs::write(&sum_below_object, sum_below.bytes()).expect("writes the conditional sum object");
    fs::write(&sum_object, sum.bytes()).expect("writes the sum object");
    fs::write(&harness_source, C_HARNESS).expect("writes the C harness");
    assert_dwarf_object(&add_object, "prototype_add", &[(2, 3), (3, 3)]);
    assert_dwarf_object(&min_object, "prototype_min", &[(9, 3), (11, 3), (13, 3)]);
    assert_dwarf_object(
        &callback_object,
        "prototype_host_callback",
        &[(17, 3), (18, 3)],
    );

    let compilation = Command::new("cc")
        .arg("-c")
        .arg(&harness_source)
        .arg("-o")
        .arg(&harness_object)
        .output()
        .expect("runs the host C compiler");
    assert!(
        compilation.status.success(),
        "C harness compilation failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&compilation.stdout),
        String::from_utf8_lossy(&compilation.stderr)
    );

    let request = LinkRequest::new(&executable)
        .with_object(&harness_object)
        .with_object(&add_object)
        .with_object(&fibonacci_object)
        .with_object(&allocation_object)
        .with_object(&allocation_burst_object)
        .with_object(&allocation_churn_object)
        .with_object(&allocation_variable_object)
        .with_object(&callback_object)
        .with_object(&component_object)
        .with_object(&min_object)
        .with_object(&sum_below_object)
        .with_object(&sum_object);
    let linker = SystemLinker::discover_target(LinkTarget::MacosAarch64)
        .expect("discovers the tier-one macOS AArch64 C compiler driver");
    linker
        .link(&request)
        .expect("links the AOT objects through the host C compiler driver");
    let aot_value = assert_aot_executable(&executable);
    let cross_mode = run_prototype_resource_failure_component()
        .expect("VM and JIT execute the resource/failure fixture");
    let aot_outcome = run_prototype_resource_failure_adapter(aot_value)
        .expect("AOT result executes the resource/failure adapter");
    assert_eq!(aot_outcome, cross_mode.vm);
    assert_eq!(aot_outcome, cross_mode.jit);
    assert_linked_dsym(&executable, &dsym);

    let response_request = LinkRequest::new(&response_executable)
        .with_object(&harness_object)
        .with_object(&add_object)
        .with_object(&fibonacci_object)
        .with_object(&allocation_object)
        .with_object(&allocation_burst_object)
        .with_object(&allocation_churn_object)
        .with_object(&allocation_variable_object)
        .with_object(&callback_object)
        .with_object(&component_object)
        .with_object(&min_object)
        .with_object(&sum_below_object)
        .with_object(&sum_object);
    linker
        .link_with_response_file(&response_request, &response_file)
        .expect("links the AOT objects through a response file");
    assert!(response_file.is_file());
    let response_aot_value = assert_aot_executable(&response_executable);
    assert_eq!(
        run_prototype_resource_failure_adapter(response_aot_value)
            .expect("response-file AOT result executes the resource/failure adapter"),
        aot_outcome
    );
    assert_linked_dsym(&response_executable, &response_dsym);
}
