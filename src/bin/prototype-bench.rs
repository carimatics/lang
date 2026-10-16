use std::cmp::Ordering;
use std::env;
#[cfg(all(target_os = "windows", target_env = "msvc"))]
use std::ffi::OsString;
use std::fmt::Write as _;
#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
use std::fs;
use std::hint::black_box;
#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
use lang::linker::{LinkRequest, LinkTarget, SystemLinker};
use lang::{
    PROTOTYPE_ALLOCATE_HOST, PROTOTYPE_DOUBLE_HOST, prototype_add_function,
    prototype_countdown_function, prototype_host_allocation_function,
    prototype_host_callback_function, prototype_min_function, prototype_sum_below_function,
    prototype_sum_to_function,
};
use lang_bytecode::lower;
use lang_codegen_clif::{CompiledFunction, emit_host_object};
use lang_runtime::HostFunctionRegistry;
use lang_vm::{execute, execute_with_hosts};

const DEFAULT_SAMPLES: usize = 31;
const DEFAULT_LOWERING_ITERATIONS: u64 = 1_000;
const DEFAULT_RUNTIME_ITERATIONS: u64 = 100_000;
const DEFAULT_STARTUP_ITERATIONS: u64 = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Config {
    samples: usize,
    lowering_iterations: u64,
    runtime_iterations: u64,
    startup_iterations: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            samples: DEFAULT_SAMPLES,
            lowering_iterations: DEFAULT_LOWERING_ITERATIONS,
            runtime_iterations: DEFAULT_RUNTIME_ITERATIONS,
            startup_iterations: DEFAULT_STARTUP_ITERATIONS,
        }
    }
}

impl Config {
    fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut config = Self::default();
        let mut arguments = arguments.into_iter();

        while let Some(flag) = arguments.next() {
            let value = arguments
                .next()
                .ok_or_else(|| format!("missing value for {flag}"))?;
            match flag.as_str() {
                "--samples" => config.samples = parse_positive(&value, &flag)?,
                "--lowering-iterations" => {
                    config.lowering_iterations = parse_positive(&value, &flag)?;
                }
                "--runtime-iterations" => {
                    config.runtime_iterations = parse_positive(&value, &flag)?;
                }
                "--startup-iterations" => {
                    config.startup_iterations = parse_positive(&value, &flag)?;
                }
                _ => return Err(format!("unknown option {flag}")),
            }
        }

        if config.samples < 3 {
            return Err("--samples must be at least 3".to_owned());
        }
        Ok(config)
    }
}

fn parse_positive<T>(value: &str, flag: &str) -> Result<T, String>
where
    T: std::str::FromStr + PartialEq + From<u8>,
    T::Err: std::fmt::Display,
{
    let parsed = value
        .parse::<T>()
        .map_err(|error| format!("invalid value for {flag}: {error}"))?;
    if parsed == T::from(0) {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(parsed)
}

#[derive(Clone, Debug)]
struct Measurement {
    name: &'static str,
    operations_per_sample: u64,
    minimum_ns: f64,
    median_ns: f64,
    p95_ns: f64,
    maximum_ns: f64,
    mean_ns: f64,
    relative_stddev_percent: f64,
}

struct PreparedWorkloads<'a> {
    add_program: &'a lang_bytecode::Program,
    min_program: &'a lang_bytecode::Program,
    countdown_program: &'a lang_bytecode::Program,
    sum_to_program: &'a lang_bytecode::Program,
    sum_below_program: &'a lang_bytecode::Program,
    host_program: &'a lang_bytecode::Program,
    host_allocation_program: &'a lang_bytecode::Program,
    host_functions: &'a HostFunctionRegistry,
    compiled_add: &'a CompiledFunction,
    compiled_min: &'a CompiledFunction,
    compiled_countdown: &'a CompiledFunction,
    compiled_sum_to: &'a CompiledFunction,
    compiled_sum_below: &'a CompiledFunction,
    compiled_host: &'a CompiledFunction,
    compiled_host_allocation: &'a CompiledFunction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArtifactSizes {
    add_code: usize,
    min_code: usize,
    countdown_code: usize,
    sum_to_code: usize,
    sum_below_code: usize,
    host_code: usize,
    add_object: usize,
    min_object: usize,
    countdown_object: usize,
    sum_to_object: usize,
    sum_below_object: usize,
    aot_executable: Option<u64>,
}

impl Measurement {
    fn from_samples(
        name: &'static str,
        operations_per_sample: u64,
        mut nanoseconds_per_operation: Vec<f64>,
    ) -> Self {
        nanoseconds_per_operation
            .sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
        let count = nanoseconds_per_operation.len();
        let mean_ns = nanoseconds_per_operation.iter().sum::<f64>() / count as f64;
        let variance = nanoseconds_per_operation
            .iter()
            .map(|value| {
                let distance = value - mean_ns;
                distance * distance
            })
            .sum::<f64>()
            / count as f64;
        let p95_index = (count * 95).div_ceil(100).saturating_sub(1);

        Self {
            name,
            operations_per_sample,
            minimum_ns: nanoseconds_per_operation[0],
            median_ns: nanoseconds_per_operation[count / 2],
            p95_ns: nanoseconds_per_operation[p95_index],
            maximum_ns: nanoseconds_per_operation[count - 1],
            mean_ns,
            relative_stddev_percent: if mean_ns == 0.0 {
                0.0
            } else {
                variance.sqrt() / mean_ns * 100.0
            },
        }
    }
}

fn main() -> ExitCode {
    if env::args().any(|argument| argument == "-h" || argument == "--help") {
        print_usage();
        return ExitCode::SUCCESS;
    }

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_usage() {
    println!(
        "usage: cargo run --release --bin prototype-bench -- \\\n         [--samples <count>] [--lowering-iterations <count>] \\\n         [--runtime-iterations <count>]"
    );
}

fn run() -> Result<(), String> {
    let command_line = env::args().collect::<Vec<_>>().join(" ");
    let config = Config::parse(env::args().skip(1))?;

    let add_function = prototype_add_function();
    let min_function = prototype_min_function();
    let countdown_function = prototype_countdown_function();
    let sum_to_function = prototype_sum_to_function();
    let sum_below_function = prototype_sum_below_function();
    let host_function = prototype_host_callback_function();
    let host_allocation_function = prototype_host_allocation_function();
    let add_program = lower(&add_function).map_err(|error| error.to_string())?;
    let min_program = lower(&min_function).map_err(|error| error.to_string())?;
    let countdown_program = lower(&countdown_function).map_err(|error| error.to_string())?;
    let sum_to_program = lower(&sum_to_function).map_err(|error| error.to_string())?;
    let sum_below_program = lower(&sum_below_function).map_err(|error| error.to_string())?;
    let host_program = lower(&host_function).map_err(|error| error.to_string())?;
    let host_allocation_program =
        lower(&host_allocation_function).map_err(|error| error.to_string())?;
    let host_functions = prototype_host_registry()?;
    let compiled_add =
        CompiledFunction::compile(&add_function).map_err(|error| error.to_string())?;
    let compiled_min =
        CompiledFunction::compile(&min_function).map_err(|error| error.to_string())?;
    let compiled_countdown =
        CompiledFunction::compile(&countdown_function).map_err(|error| error.to_string())?;
    let compiled_sum_to =
        CompiledFunction::compile(&sum_to_function).map_err(|error| error.to_string())?;
    let compiled_sum_below =
        CompiledFunction::compile(&sum_below_function).map_err(|error| error.to_string())?;
    let compiled_host = CompiledFunction::compile_with_hosts(&host_function, &host_functions)
        .map_err(|error| error.to_string())?;
    let compiled_host_allocation =
        CompiledFunction::compile_with_hosts(&host_allocation_function, &host_functions)
            .map_err(|error| error.to_string())?;
    let aot_add = emit_host_object(&add_function).map_err(|error| error.to_string())?;
    let aot_min = emit_host_object(&min_function).map_err(|error| error.to_string())?;
    let aot_countdown = emit_host_object(&countdown_function).map_err(|error| error.to_string())?;
    let aot_sum_to = emit_host_object(&sum_to_function).map_err(|error| error.to_string())?;
    let aot_sum_below = emit_host_object(&sum_below_function).map_err(|error| error.to_string())?;
    #[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
    let link_benchmark = LinkBenchmark::prepare(
        aot_add.bytes(),
        aot_min.bytes(),
        aot_countdown.bytes(),
        aot_sum_to.bytes(),
        aot_sum_below.bytes(),
    )?;
    #[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
    let aot_executable_size = Some(link_benchmark.executable_size()?);
    #[cfg(not(any(target_os = "macos", all(target_os = "windows", target_env = "msvc"))))]
    let aot_executable_size = None;

    let workloads = PreparedWorkloads {
        add_program: &add_program,
        min_program: &min_program,
        countdown_program: &countdown_program,
        sum_to_program: &sum_to_program,
        sum_below_program: &sum_below_program,
        host_program: &host_program,
        host_allocation_program: &host_allocation_program,
        host_functions: &host_functions,
        compiled_add: &compiled_add,
        compiled_min: &compiled_min,
        compiled_countdown: &compiled_countdown,
        compiled_sum_to: &compiled_sum_to,
        compiled_sum_below: &compiled_sum_below,
        compiled_host: &compiled_host,
        compiled_host_allocation: &compiled_host_allocation,
    };
    check_results(&workloads)?;
    warm_up(config.runtime_iterations, &workloads);

    let mut measurements = Vec::new();
    measurements.push(measure_construction(
        "bytecode_lower_add",
        config.samples,
        config.lowering_iterations,
        || lower(black_box(&add_function)).expect("validated prototype MIR lowers"),
    ));
    measurements.push(measure_construction(
        "bytecode_lower_min",
        config.samples,
        config.lowering_iterations,
        || lower(black_box(&min_function)).expect("validated conditional MIR lowers"),
    ));
    measurements.push(measure_construction(
        "bytecode_lower_countdown",
        config.samples,
        config.lowering_iterations,
        || lower(black_box(&countdown_function)).expect("validated countdown MIR lowers"),
    ));
    measurements.push(measure_construction(
        "bytecode_lower_sum_to",
        config.samples,
        config.lowering_iterations,
        || lower(black_box(&sum_to_function)).expect("validated accumulator MIR lowers"),
    ));
    measurements.push(measure_construction(
        "bytecode_lower_sum_below",
        config.samples,
        config.lowering_iterations,
        || lower(black_box(&sum_below_function)).expect("validated conditional loop MIR lowers"),
    ));
    measurements.push(measure_construction(
        "jit_compile_add",
        config.samples,
        1,
        || {
            CompiledFunction::compile(black_box(&add_function))
                .expect("validated prototype MIR compiles")
        },
    ));
    measurements.push(measure_construction(
        "jit_compile_min",
        config.samples,
        1,
        || {
            CompiledFunction::compile(black_box(&min_function))
                .expect("validated conditional MIR compiles")
        },
    ));
    measurements.push(measure_construction(
        "jit_compile_countdown",
        config.samples,
        1,
        || {
            CompiledFunction::compile(black_box(&countdown_function))
                .expect("validated countdown MIR compiles")
        },
    ));
    measurements.push(measure_construction(
        "jit_compile_sum_to",
        config.samples,
        1,
        || {
            CompiledFunction::compile(black_box(&sum_to_function))
                .expect("validated accumulator MIR compiles")
        },
    ));
    measurements.push(measure_construction(
        "jit_compile_sum_below",
        config.samples,
        1,
        || {
            CompiledFunction::compile(black_box(&sum_below_function))
                .expect("validated conditional loop MIR compiles")
        },
    ));
    measurements.push(measure_construction(
        "aot_object_emit_add",
        config.samples,
        1,
        || {
            emit_host_object(black_box(&add_function))
                .expect("validated prototype MIR emits an object")
        },
    ));
    measurements.push(measure_construction(
        "aot_object_emit_min",
        config.samples,
        1,
        || {
            emit_host_object(black_box(&min_function))
                .expect("validated conditional MIR emits an object")
        },
    ));
    measurements.push(measure_construction(
        "aot_object_emit_countdown",
        config.samples,
        1,
        || {
            emit_host_object(black_box(&countdown_function))
                .expect("validated countdown MIR emits an object")
        },
    ));
    measurements.push(measure_construction(
        "aot_object_emit_sum_to",
        config.samples,
        1,
        || {
            emit_host_object(black_box(&sum_to_function))
                .expect("validated accumulator MIR emits an object")
        },
    ));
    measurements.push(measure_construction(
        "aot_object_emit_sum_below",
        config.samples,
        1,
        || {
            emit_host_object(black_box(&sum_below_function))
                .expect("validated conditional loop MIR emits an object")
        },
    ));
    #[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
    {
        measurements.push(link_benchmark.measure_link(config.samples)?);
        measurements.push(link_benchmark.measure_startup(
            "aot_process_startup_add_min_countdown_sum_to_sum_below",
            config.samples,
            1,
        )?);
        measurements.push(link_benchmark.measure_startup(
            "aot_process_startup_batch_add_min_countdown_sum_to_sum_below",
            config.samples,
            config.startup_iterations,
        )?);
    }
    measurements.push(measure_calls(
        "rust_add_call",
        config.samples,
        config.runtime_iterations,
        |iteration| rust_wrapping_add(iteration as i64, 17),
    ));
    measurements.push(measure_calls(
        "vm_add_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            execute(&add_program, &[iteration as i64, 17]).expect("verified bytecode executes")
        },
    ));
    measurements.push(measure_calls(
        "jit_add_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            compiled_add
                .invoke_i64_2(iteration as i64, 17)
                .expect("compiled prototype has the expected signature")
        },
    ));
    measurements.push(measure_calls(
        "rust_min_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            let (left, right) = alternating_min_inputs(iteration);
            rust_min(left, right)
        },
    ));
    measurements.push(measure_calls(
        "vm_min_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            let (left, right) = alternating_min_inputs(iteration);
            execute(&min_program, &[left, right]).expect("verified conditional bytecode executes")
        },
    ));
    measurements.push(measure_calls(
        "jit_min_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            let (left, right) = alternating_min_inputs(iteration);
            compiled_min
                .invoke_i64_2(left, right)
                .expect("compiled conditional prototype has the expected signature")
        },
    ));
    measurements.push(measure_calls(
        "rust_countdown_call",
        config.samples,
        config.runtime_iterations,
        |iteration| rust_countdown(alternating_countdown_input(iteration)),
    ));
    measurements.push(measure_calls(
        "vm_countdown_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            execute(
                &countdown_program,
                &[alternating_countdown_input(iteration)],
            )
            .expect("verified countdown bytecode executes")
        },
    ));
    measurements.push(measure_calls(
        "jit_countdown_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            compiled_countdown
                .invoke_i64_1(alternating_countdown_input(iteration))
                .expect("compiled countdown prototype has the expected signature")
        },
    ));
    measurements.push(measure_calls(
        "rust_sum_to_call",
        config.samples,
        config.runtime_iterations,
        |iteration| rust_sum_to(alternating_countdown_input(iteration)),
    ));
    measurements.push(measure_calls(
        "vm_sum_to_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            execute(&sum_to_program, &[alternating_countdown_input(iteration)])
                .expect("verified accumulator bytecode executes")
        },
    ));
    measurements.push(measure_calls(
        "jit_sum_to_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            compiled_sum_to
                .invoke_i64_1(alternating_countdown_input(iteration))
                .expect("compiled accumulator prototype has the expected signature")
        },
    ));
    measurements.push(measure_calls(
        "rust_sum_below_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            let (value, cutoff) = alternating_sum_below_inputs(iteration);
            rust_sum_below(value, cutoff)
        },
    ));
    measurements.push(measure_calls(
        "vm_sum_below_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            let (value, cutoff) = alternating_sum_below_inputs(iteration);
            execute(&sum_below_program, &[value, cutoff])
                .expect("verified conditional loop bytecode executes")
        },
    ));
    measurements.push(measure_calls(
        "jit_sum_below_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            let (value, cutoff) = alternating_sum_below_inputs(iteration);
            compiled_sum_below
                .invoke_i64_2(value, cutoff)
                .expect("compiled conditional loop prototype has the expected signature")
        },
    ));
    measurements.push(measure_calls(
        "rust_host_call",
        config.samples,
        config.runtime_iterations,
        |iteration| prototype_wrapping_double(iteration as i64),
    ));
    measurements.push(measure_calls(
        "vm_host_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            execute_with_hosts(&host_program, &[iteration as i64], &host_functions)
                .expect("verified host-call bytecode executes")
        },
    ));
    measurements.push(measure_calls(
        "jit_host_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            compiled_host
                .invoke_i64_1(iteration as i64)
                .expect("compiled host callback has the expected signature")
        },
    ));
    measurements.push(measure_calls(
        "rust_host_allocation_call",
        config.samples,
        config.runtime_iterations,
        |iteration| prototype_allocate_roundtrip(iteration as i64),
    ));
    measurements.push(measure_calls(
        "vm_host_allocation_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            execute_with_hosts(
                &host_allocation_program,
                &[iteration as i64],
                &host_functions,
            )
            .expect("verified host-allocation bytecode executes")
        },
    ));
    measurements.push(measure_calls(
        "jit_host_allocation_call",
        config.samples,
        config.runtime_iterations,
        |iteration| {
            compiled_host_allocation
                .invoke_i64_1(iteration as i64)
                .expect("compiled host-allocation callback has the expected signature")
        },
    ));

    println!(
        "{}",
        render_json(
            &command_line,
            config,
            ArtifactSizes {
                add_code: compiled_add.code_size(),
                min_code: compiled_min.code_size(),
                countdown_code: compiled_countdown.code_size(),
                sum_to_code: compiled_sum_to.code_size(),
                sum_below_code: compiled_sum_below.code_size(),
                host_code: compiled_host.code_size(),
                add_object: aot_add.bytes().len(),
                min_object: aot_min.bytes().len(),
                countdown_object: aot_countdown.bytes().len(),
                sum_to_object: aot_sum_to.bytes().len(),
                sum_below_object: aot_sum_below.bytes().len(),
                aot_executable: aot_executable_size,
            },
            &measurements,
        )
    );
    Ok(())
}

fn prototype_host_registry() -> Result<HostFunctionRegistry, String> {
    let mut host_functions = HostFunctionRegistry::new();
    host_functions
        .register_i64(PROTOTYPE_DOUBLE_HOST, prototype_wrapping_double)
        .map_err(|error| error.to_string())?;
    host_functions
        .register_i64(PROTOTYPE_ALLOCATE_HOST, prototype_allocate_roundtrip)
        .map_err(|error| error.to_string())?;
    Ok(host_functions)
}

#[inline(never)]
extern "C" fn rust_wrapping_add(left: i64, right: i64) -> i64 {
    left.wrapping_add(right)
}

#[inline(never)]
extern "C" fn rust_min(left: i64, right: i64) -> i64 {
    left.min(right)
}

fn alternating_min_inputs(iteration: u64) -> (i64, i64) {
    let left = iteration as i64;
    (left, left ^ 1)
}

fn alternating_countdown_input(iteration: u64) -> i64 {
    if iteration.is_multiple_of(2) { 64 } else { -7 }
}

#[inline(never)]
extern "C" fn rust_countdown(mut value: i64) -> i64 {
    while value >= 1 {
        value -= 1;
    }
    value
}

#[inline(never)]
extern "C" fn rust_sum_to(mut value: i64) -> i64 {
    let mut sum = 0_i64;
    while value >= 1 {
        sum = sum.wrapping_add(value);
        value = value.wrapping_sub(1);
    }
    sum
}

fn alternating_sum_below_inputs(iteration: u64) -> (i64, i64) {
    if iteration.is_multiple_of(2) {
        (64, 33)
    } else {
        (-7, 33)
    }
}

#[inline(never)]
extern "C" fn rust_sum_below(mut value: i64, cutoff: i64) -> i64 {
    let mut sum = 0_i64;
    while value >= 1 {
        if value < cutoff {
            sum = sum.wrapping_add(value);
        }
        value = value.wrapping_sub(1);
    }
    sum
}

#[inline(never)]
extern "C" fn prototype_wrapping_double(value: i64) -> i64 {
    value.wrapping_mul(2)
}

#[inline(never)]
extern "C" fn prototype_allocate_roundtrip(value: i64) -> i64 {
    let allocation = black_box(Box::new(value));
    black_box(*allocation)
}

fn check_results(workloads: &PreparedWorkloads<'_>) -> Result<(), String> {
    let expected_add = rust_wrapping_add(20, 22);
    let vm_add = execute(workloads.add_program, &[20, 22]).map_err(|error| error.to_string())?;
    let jit_add = workloads
        .compiled_add
        .invoke_i64_2(20, 22)
        .map_err(|error| error.to_string())?;
    for (left, right) in [(-7, 4), (9, 3)] {
        let expected_min = rust_min(left, right);
        let vm_min =
            execute(workloads.min_program, &[left, right]).map_err(|error| error.to_string())?;
        let jit_min = workloads
            .compiled_min
            .invoke_i64_2(left, right)
            .map_err(|error| error.to_string())?;
        if (vm_min, jit_min) != (expected_min, expected_min) {
            return Err("conditional execution modes disagreed before measurement".to_owned());
        }
    }
    for value in [-7, 0, 1, 64] {
        let expected_countdown = rust_countdown(value);
        let vm_countdown =
            execute(workloads.countdown_program, &[value]).map_err(|error| error.to_string())?;
        let jit_countdown = workloads
            .compiled_countdown
            .invoke_i64_1(value)
            .map_err(|error| error.to_string())?;
        if (vm_countdown, jit_countdown) != (expected_countdown, expected_countdown) {
            return Err("countdown execution modes disagreed before measurement".to_owned());
        }
        let expected_sum_to = rust_sum_to(value);
        let vm_sum_to =
            execute(workloads.sum_to_program, &[value]).map_err(|error| error.to_string())?;
        let jit_sum_to = workloads
            .compiled_sum_to
            .invoke_i64_1(value)
            .map_err(|error| error.to_string())?;
        if (vm_sum_to, jit_sum_to) != (expected_sum_to, expected_sum_to) {
            return Err("accumulator execution modes disagreed before measurement".to_owned());
        }
    }
    for (value, cutoff) in [(-7, 33), (0, 33), (1, 1), (1, 2), (64, 33)] {
        let expected_sum_below = rust_sum_below(value, cutoff);
        let vm_sum_below = execute(workloads.sum_below_program, &[value, cutoff])
            .map_err(|error| error.to_string())?;
        let jit_sum_below = workloads
            .compiled_sum_below
            .invoke_i64_2(value, cutoff)
            .map_err(|error| error.to_string())?;
        if (vm_sum_below, jit_sum_below) != (expected_sum_below, expected_sum_below) {
            return Err("conditional loop execution modes disagreed before measurement".to_owned());
        }
    }
    let expected_host = prototype_wrapping_double(21);
    let vm_host = execute_with_hosts(workloads.host_program, &[21], workloads.host_functions)
        .map_err(|error| error.to_string())?;
    let jit_host = workloads
        .compiled_host
        .invoke_i64_1(21)
        .map_err(|error| error.to_string())?;
    let expected_host_allocation = prototype_allocate_roundtrip(-9);
    let vm_host_allocation = execute_with_hosts(
        workloads.host_allocation_program,
        &[-9],
        workloads.host_functions,
    )
    .map_err(|error| error.to_string())?;
    let jit_host_allocation = workloads
        .compiled_host_allocation
        .invoke_i64_1(-9)
        .map_err(|error| error.to_string())?;

    if (
        vm_add,
        jit_add,
        vm_host,
        jit_host,
        vm_host_allocation,
        jit_host_allocation,
    ) != (
        expected_add,
        expected_add,
        expected_host,
        expected_host,
        expected_host_allocation,
        expected_host_allocation,
    ) {
        return Err("execution modes disagreed before measurement".to_owned());
    }
    Ok(())
}

fn warm_up(runtime_iterations: u64, workloads: &PreparedWorkloads<'_>) {
    let iterations = runtime_iterations.clamp(1_000, 10_000);
    for iteration in 0..iterations {
        let value = iteration as i64;
        black_box(rust_wrapping_add(value, 17));
        black_box(
            execute(workloads.add_program, &[value, 17]).expect("verified bytecode executes"),
        );
        black_box(
            workloads
                .compiled_add
                .invoke_i64_2(value, 17)
                .expect("compiled prototype has the expected signature"),
        );
        let (left, right) = alternating_min_inputs(iteration);
        black_box(rust_min(left, right));
        black_box(
            execute(workloads.min_program, &[left, right])
                .expect("verified conditional bytecode executes"),
        );
        black_box(
            workloads
                .compiled_min
                .invoke_i64_2(left, right)
                .expect("compiled conditional prototype has the expected signature"),
        );
        let countdown_input = alternating_countdown_input(iteration);
        black_box(rust_countdown(countdown_input));
        black_box(
            execute(workloads.countdown_program, &[countdown_input])
                .expect("verified countdown bytecode executes"),
        );
        black_box(
            workloads
                .compiled_countdown
                .invoke_i64_1(countdown_input)
                .expect("compiled countdown prototype has the expected signature"),
        );
        black_box(rust_sum_to(countdown_input));
        black_box(
            execute(workloads.sum_to_program, &[countdown_input])
                .expect("verified accumulator bytecode executes"),
        );
        black_box(
            workloads
                .compiled_sum_to
                .invoke_i64_1(countdown_input)
                .expect("compiled accumulator prototype has the expected signature"),
        );
        let (sum_below_value, sum_below_cutoff) = alternating_sum_below_inputs(iteration);
        black_box(rust_sum_below(sum_below_value, sum_below_cutoff));
        black_box(
            execute(
                workloads.sum_below_program,
                &[sum_below_value, sum_below_cutoff],
            )
            .expect("verified conditional loop bytecode executes"),
        );
        black_box(
            workloads
                .compiled_sum_below
                .invoke_i64_2(sum_below_value, sum_below_cutoff)
                .expect("compiled conditional loop prototype has the expected signature"),
        );
        black_box(prototype_wrapping_double(value));
        black_box(
            execute_with_hosts(workloads.host_program, &[value], workloads.host_functions)
                .expect("verified host-call bytecode executes"),
        );
        black_box(
            workloads
                .compiled_host
                .invoke_i64_1(value)
                .expect("compiled host callback has the expected signature"),
        );
        black_box(prototype_allocate_roundtrip(value));
        black_box(
            execute_with_hosts(
                workloads.host_allocation_program,
                &[value],
                workloads.host_functions,
            )
            .expect("verified host-allocation bytecode executes"),
        );
        black_box(
            workloads
                .compiled_host_allocation
                .invoke_i64_1(value)
                .expect("compiled host-allocation callback has the expected signature"),
        );
    }
}

#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
const LINK_HARNESS: &str = r#"
#include <stdint.h>

extern int64_t prototype_add(int64_t left, int64_t right);
extern int64_t prototype_countdown(int64_t value);
extern int64_t prototype_min(int64_t left, int64_t right);
extern int64_t prototype_sum_below(int64_t value, int64_t cutoff);
extern int64_t prototype_sum_to(int64_t value);

int main(void) {
    return prototype_add(20, 22) == 42 &&
        prototype_countdown(64) == 0 &&
        prototype_countdown(-7) == -7 &&
        prototype_min(-7, 4) == -7 &&
        prototype_sum_below(64, 33) == 528 &&
        prototype_sum_below(-7, 33) == 0 &&
        prototype_sum_to(64) == 2080 &&
        prototype_sum_to(-7) == 0 ? 0 : 1;
}
"#;

#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
struct BenchmarkDirectory {
    path: PathBuf,
}

#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
impl BenchmarkDirectory {
    fn create() -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "lang-link-benchmark-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
impl Drop for BenchmarkDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
struct LinkBenchmark {
    _directory: BenchmarkDirectory,
    linker: SystemLinker,
    harness_object: PathBuf,
    add_object: PathBuf,
    min_object: PathBuf,
    countdown_object: PathBuf,
    sum_to_object: PathBuf,
    sum_below_object: PathBuf,
    executable: PathBuf,
}

#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
impl LinkBenchmark {
    fn prepare(
        add_object_bytes: &[u8],
        min_object_bytes: &[u8],
        countdown_object_bytes: &[u8],
        sum_to_object_bytes: &[u8],
        sum_below_object_bytes: &[u8],
    ) -> Result<Self, String> {
        let directory = BenchmarkDirectory::create()?;
        let harness_source = directory.path().join("harness.c");
        #[cfg(target_os = "macos")]
        let (
            harness_object,
            add_object,
            min_object,
            countdown_object,
            sum_to_object,
            sum_below_object,
            executable,
        ) = (
            directory.path().join("harness.o"),
            directory.path().join("prototype_add.o"),
            directory.path().join("prototype_min.o"),
            directory.path().join("prototype_countdown.o"),
            directory.path().join("prototype_sum_to.o"),
            directory.path().join("prototype_sum_below.o"),
            directory.path().join("prototype-aot"),
        );
        #[cfg(all(target_os = "windows", target_env = "msvc"))]
        let (
            harness_object,
            add_object,
            min_object,
            countdown_object,
            sum_to_object,
            sum_below_object,
            executable,
        ) = (
            directory.path().join("harness.obj"),
            directory.path().join("prototype_add.obj"),
            directory.path().join("prototype_min.obj"),
            directory.path().join("prototype_countdown.obj"),
            directory.path().join("prototype_sum_to.obj"),
            directory.path().join("prototype_sum_below.obj"),
            directory.path().join("prototype-aot.exe"),
        );
        fs::write(&harness_source, LINK_HARNESS).map_err(|error| error.to_string())?;
        fs::write(&add_object, add_object_bytes).map_err(|error| error.to_string())?;
        fs::write(&min_object, min_object_bytes).map_err(|error| error.to_string())?;
        fs::write(&countdown_object, countdown_object_bytes).map_err(|error| error.to_string())?;
        fs::write(&sum_to_object, sum_to_object_bytes).map_err(|error| error.to_string())?;
        fs::write(&sum_below_object, sum_below_object_bytes).map_err(|error| error.to_string())?;

        let compile = compile_link_harness(&harness_source, &harness_object)?;
        ensure_command_succeeded("C harness compilation", &compile)?;

        let target = LinkTarget::host()
            .ok_or_else(|| "system-link benchmark requires an initial tier-1 host".to_owned())?;
        let linker = SystemLinker::discover_target(target).map_err(|error| error.to_string())?;
        #[cfg(all(target_os = "windows", target_env = "msvc"))]
        let linker = linker.with_argument("/NOLOGO");

        let benchmark = Self {
            _directory: directory,
            linker,
            harness_object,
            add_object,
            min_object,
            countdown_object,
            sum_to_object,
            sum_below_object,
            executable,
        };
        benchmark.link_once()?;
        let execution = Command::new(&benchmark.executable)
            .output()
            .map_err(|error| format!("failed to run the linked executable: {error}"))?;
        ensure_command_succeeded("linked executable", &execution)?;
        Ok(benchmark)
    }

    fn executable_size(&self) -> Result<u64, String> {
        fs::metadata(&self.executable)
            .map(|metadata| metadata.len())
            .map_err(|error| format!("failed to inspect the linked executable: {error}"))
    }

    fn measure_link(&self, sample_count: usize) -> Result<Measurement, String> {
        let mut samples = Vec::with_capacity(sample_count);
        for _ in 0..sample_count {
            let started = Instant::now();
            self.link_once()?;
            samples.push(started.elapsed().as_secs_f64() * 1_000_000_000.0);
        }
        Ok(Measurement::from_samples(
            "system_link_add_min_countdown_sum_to_sum_below",
            1,
            samples,
        ))
    }

    fn measure_startup(
        &self,
        name: &'static str,
        sample_count: usize,
        operations_per_sample: u64,
    ) -> Result<Measurement, String> {
        let mut samples = Vec::with_capacity(sample_count);
        for _ in 0..sample_count {
            let started = Instant::now();
            for _ in 0..operations_per_sample {
                let execution = Command::new(&self.executable)
                    .output()
                    .map_err(|error| format!("failed to run the linked executable: {error}"))?;
                ensure_command_succeeded("linked executable", &execution)?;
            }
            samples.push(
                started.elapsed().as_secs_f64() * 1_000_000_000.0 / operations_per_sample as f64,
            );
        }
        Ok(Measurement::from_samples(
            name,
            operations_per_sample,
            samples,
        ))
    }

    fn link_once(&self) -> Result<(), String> {
        let request = LinkRequest::new(&self.executable)
            .with_object(&self.harness_object)
            .with_object(&self.add_object)
            .with_object(&self.min_object)
            .with_object(&self.countdown_object)
            .with_object(&self.sum_to_object)
            .with_object(&self.sum_below_object);
        self.linker
            .link(&request)
            .map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "macos")]
fn compile_link_harness(
    harness_source: &Path,
    harness_object: &Path,
) -> Result<std::process::Output, String> {
    Command::new("cc")
        .args(["-O2", "-c"])
        .arg(harness_source)
        .arg("-o")
        .arg(harness_object)
        .output()
        .map_err(|error| format!("failed to run the system C compiler: {error}"))
}

#[cfg(all(target_os = "windows", target_env = "msvc"))]
fn compile_link_harness(
    harness_source: &Path,
    harness_object: &Path,
) -> Result<std::process::Output, String> {
    let mut output_argument = OsString::from("/Fo:");
    output_argument.push(harness_object);
    Command::new("cl.exe")
        .args(["/nologo", "/O2", "/c"])
        .arg(harness_source)
        .arg(output_argument)
        .output()
        .map_err(|error| format!("failed to run the MSVC C compiler: {error}"))
}

#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
fn ensure_command_succeeded(label: &str, output: &std::process::Output) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{label} failed with {}:\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn measure_construction<T>(
    name: &'static str,
    sample_count: usize,
    operations_per_sample: u64,
    mut operation: impl FnMut() -> T,
) -> Measurement {
    let mut samples = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        let mut outputs = Vec::with_capacity(operations_per_sample as usize);
        let started = Instant::now();
        for _ in 0..operations_per_sample {
            outputs.push(black_box(operation()));
        }
        let elapsed = started.elapsed();
        black_box(&outputs);
        samples.push(elapsed.as_secs_f64() * 1_000_000_000.0 / operations_per_sample as f64);
    }
    Measurement::from_samples(name, operations_per_sample, samples)
}

fn measure_calls(
    name: &'static str,
    sample_count: usize,
    operations_per_sample: u64,
    mut operation: impl FnMut(u64) -> i64,
) -> Measurement {
    let mut samples = Vec::with_capacity(sample_count);
    for _ in 0..sample_count {
        let mut checksum = 0_i64;
        let started = Instant::now();
        for iteration in 0..operations_per_sample {
            checksum = checksum.wrapping_add(black_box(operation(iteration)));
        }
        let elapsed = started.elapsed();
        black_box(checksum);
        samples.push(elapsed.as_secs_f64() * 1_000_000_000.0 / operations_per_sample as f64);
    }
    Measurement::from_samples(name, operations_per_sample, samples)
}

fn render_json(
    command_line: &str,
    config: Config,
    sizes: ArtifactSizes,
    measurements: &[Measurement],
) -> String {
    let mut output = String::new();
    let unix_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let commit = command_output("git", &["rev-parse", "HEAD"]);
    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .is_ok_and(|result| !result.stdout.is_empty());

    writeln!(output, "{{").expect("writing to String succeeds");
    writeln!(output, "  \"schema_version\": 8,").expect("writing to String succeeds");
    writeln!(output, "  \"timestamp_unix_seconds\": {unix_time},")
        .expect("writing to String succeeds");
    write_json_string(&mut output, "command", command_line, true);
    write_json_string(&mut output, "git_commit", &commit, true);
    writeln!(output, "  \"git_dirty\": {dirty},").expect("writing to String succeeds");
    write_json_string(
        &mut output,
        "rustc",
        &command_output("rustc", &["-Vv"]),
        true,
    );
    write_json_string(&mut output, "os", env::consts::OS, true);
    write_json_string(&mut output, "os_version", &os_version(), true);
    write_json_string(&mut output, "architecture", env::consts::ARCH, true);
    write_json_string(&mut output, "cpu", &cpu_model(), true);
    write_json_string(&mut output, "system_linker", &system_linker(), true);
    writeln!(output, "  \"samples\": {},", config.samples).expect("writing to String succeeds");
    writeln!(
        output,
        "  \"startup_iterations\": {},",
        config.startup_iterations
    )
    .expect("writing to String succeeds");
    writeln!(output, "  \"generated_code_bytes\": {{").expect("writing to String succeeds");
    writeln!(output, "    \"jit_add\": {},", sizes.add_code).expect("writing to String succeeds");
    writeln!(output, "    \"jit_min\": {},", sizes.min_code).expect("writing to String succeeds");
    writeln!(output, "    \"jit_countdown\": {},", sizes.countdown_code)
        .expect("writing to String succeeds");
    writeln!(output, "    \"jit_sum_to\": {},", sizes.sum_to_code)
        .expect("writing to String succeeds");
    writeln!(output, "    \"jit_sum_below\": {},", sizes.sum_below_code)
        .expect("writing to String succeeds");
    writeln!(output, "    \"jit_host_callback\": {}", sizes.host_code)
        .expect("writing to String succeeds");
    writeln!(output, "  }},").expect("writing to String succeeds");
    writeln!(output, "  \"object_file_bytes\": {{").expect("writing to String succeeds");
    writeln!(output, "    \"aot_add\": {},", sizes.add_object).expect("writing to String succeeds");
    writeln!(output, "    \"aot_min\": {},", sizes.min_object).expect("writing to String succeeds");
    writeln!(output, "    \"aot_countdown\": {},", sizes.countdown_object)
        .expect("writing to String succeeds");
    writeln!(output, "    \"aot_sum_to\": {},", sizes.sum_to_object)
        .expect("writing to String succeeds");
    writeln!(output, "    \"aot_sum_below\": {}", sizes.sum_below_object)
        .expect("writing to String succeeds");
    writeln!(output, "  }},").expect("writing to String succeeds");
    writeln!(output, "  \"executable_file_bytes\": {{").expect("writing to String succeeds");
    match sizes.aot_executable {
        Some(size) => writeln!(
            output,
            "    \"aot_add_min_countdown_sum_to_sum_below\": {size}"
        )
        .expect("writing to String succeeds"),
        None => writeln!(
            output,
            "    \"aot_add_min_countdown_sum_to_sum_below\": null"
        )
        .expect("writing to String succeeds"),
    }
    writeln!(output, "  }},").expect("writing to String succeeds");
    writeln!(output, "  \"measurements\": [").expect("writing to String succeeds");
    for (index, measurement) in measurements.iter().enumerate() {
        writeln!(output, "    {{").expect("writing to String succeeds");
        write_json_string(&mut output, "name", measurement.name, true);
        write_json_string(&mut output, "unit", "ns/operation", true);
        writeln!(
            output,
            "      \"operations_per_sample\": {},",
            measurement.operations_per_sample
        )
        .expect("writing to String succeeds");
        writeln!(output, "      \"minimum\": {:.3},", measurement.minimum_ns)
            .expect("writing to String succeeds");
        writeln!(output, "      \"median\": {:.3},", measurement.median_ns)
            .expect("writing to String succeeds");
        writeln!(output, "      \"p95\": {:.3},", measurement.p95_ns)
            .expect("writing to String succeeds");
        writeln!(output, "      \"maximum\": {:.3},", measurement.maximum_ns)
            .expect("writing to String succeeds");
        writeln!(output, "      \"mean\": {:.3},", measurement.mean_ns)
            .expect("writing to String succeeds");
        writeln!(
            output,
            "      \"relative_stddev_percent\": {:.3}",
            measurement.relative_stddev_percent
        )
        .expect("writing to String succeeds");
        let suffix = if index + 1 == measurements.len() {
            ""
        } else {
            ","
        };
        writeln!(output, "    }}{suffix}").expect("writing to String succeeds");
    }
    writeln!(output, "  ]").expect("writing to String succeeds");
    writeln!(output, "}}").expect("writing to String succeeds");
    output
}

fn write_json_string(output: &mut String, name: &str, value: &str, comma: bool) {
    let suffix = if comma { "," } else { "" };
    writeln!(
        output,
        "  \"{}\": \"{}\"{suffix}",
        escape_json(name),
        escape_json(value)
    )
    .expect("writing to String succeeds");
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                write!(escaped, "\\u{:04x}", character as u32).expect("writing to String succeeds");
            }
            character => escaped.push(character),
        }
    }
    escaped
}

fn command_output(program: &str, arguments: &[&str]) -> String {
    Command::new(program)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map_or_else(|| "unknown".to_owned(), |output| output.trim().to_owned())
}

#[cfg(target_os = "macos")]
fn system_linker() -> String {
    command_output("cc", &["--version"])
}

#[cfg(all(target_os = "windows", target_env = "msvc"))]
fn system_linker() -> String {
    command_output("link.exe", &["/?"])
}

#[cfg(not(any(target_os = "macos", all(target_os = "windows", target_env = "msvc"))))]
fn system_linker() -> String {
    "not measured by this harness on this platform".to_owned()
}

#[cfg(target_os = "macos")]
fn cpu_model() -> String {
    command_output("sysctl", &["-n", "machdep.cpu.brand_string"])
}

#[cfg(target_os = "windows")]
fn cpu_model() -> String {
    env::var("PROCESSOR_IDENTIFIER").unwrap_or_else(|_| "unknown".to_owned())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn cpu_model() -> String {
    "unknown".to_owned()
}

#[cfg(target_os = "macos")]
fn os_version() -> String {
    command_output("sw_vers", &["-productVersion"])
}

#[cfg(target_os = "windows")]
fn os_version() -> String {
    command_output("cmd", &["/C", "ver"])
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn os_version() -> String {
    "unknown".to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactSizes, Config, Measurement, alternating_countdown_input,
        alternating_sum_below_inputs, escape_json, render_json, rust_countdown, rust_sum_below,
        rust_sum_to,
    };

    #[test]
    fn parses_explicit_configuration() {
        let config = Config::parse([
            "--samples".to_owned(),
            "5".to_owned(),
            "--lowering-iterations".to_owned(),
            "7".to_owned(),
            "--runtime-iterations".to_owned(),
            "11".to_owned(),
            "--startup-iterations".to_owned(),
            "13".to_owned(),
        ]);

        assert_eq!(
            config,
            Ok(Config {
                samples: 5,
                lowering_iterations: 7,
                runtime_iterations: 11,
                startup_iterations: 13,
            })
        );
    }

    #[test]
    fn rejects_too_few_samples_and_zero_iterations() {
        assert_eq!(
            Config::parse(["--samples".to_owned(), "2".to_owned()]),
            Err("--samples must be at least 3".to_owned())
        );
        assert_eq!(
            Config::parse(["--runtime-iterations".to_owned(), "0".to_owned()]),
            Err("--runtime-iterations must be greater than zero".to_owned())
        );
        assert_eq!(
            Config::parse(["--startup-iterations".to_owned(), "0".to_owned()]),
            Err("--startup-iterations must be greater than zero".to_owned())
        );
    }

    #[test]
    fn calculates_distribution_summary() {
        let measurement = Measurement::from_samples("test", 1, vec![5.0, 1.0, 3.0, 2.0, 4.0]);

        assert_eq!(measurement.minimum_ns, 1.0);
        assert_eq!(measurement.median_ns, 3.0);
        assert_eq!(measurement.p95_ns, 5.0);
        assert_eq!(measurement.maximum_ns, 5.0);
        assert_eq!(measurement.mean_ns, 3.0);
    }

    #[test]
    fn escapes_json_control_characters() {
        assert_eq!(escape_json("a\n\"b\\c"), "a\\n\\\"b\\\\c");
    }

    #[test]
    fn countdown_inputs_cover_the_loop_and_immediate_return() {
        assert_eq!(alternating_countdown_input(0), 64);
        assert_eq!(alternating_countdown_input(1), -7);
        assert_eq!(rust_countdown(64), 0);
        assert_eq!(rust_countdown(-7), -7);
        assert_eq!(rust_sum_to(64), 2_080);
        assert_eq!(rust_sum_to(-7), 0);
        assert_eq!(alternating_sum_below_inputs(0), (64, 33));
        assert_eq!(alternating_sum_below_inputs(1), (-7, 33));
        assert_eq!(rust_sum_below(64, 33), 528);
        assert_eq!(rust_sum_below(-7, 33), 0);
    }

    #[test]
    fn host_allocation_roundtrip_preserves_values() {
        for value in [0_i64, 21, -7, i64::MAX, i64::MIN] {
            assert_eq!(super::prototype_allocate_roundtrip(value), value);
        }
    }

    #[test]
    fn schema_eight_retains_conditional_loop_and_executable_artifact_sizes() {
        let output = render_json(
            "prototype-bench",
            Config {
                samples: 3,
                lowering_iterations: 1,
                runtime_iterations: 1,
                startup_iterations: 2,
            },
            ArtifactSizes {
                add_code: 1,
                min_code: 2,
                countdown_code: 3,
                sum_to_code: 4,
                sum_below_code: 5,
                host_code: 6,
                add_object: 7,
                min_object: 8,
                countdown_object: 9,
                sum_to_object: 10,
                sum_below_object: 11,
                aot_executable: Some(12),
            },
            &[],
        );

        assert!(output.contains("\"schema_version\": 8"));
        assert!(output.contains("\"startup_iterations\": 2"));
        assert!(output.contains("\"jit_countdown\": 3"));
        assert!(output.contains("\"jit_sum_to\": 4"));
        assert!(output.contains("\"jit_sum_below\": 5"));
        assert!(output.contains("\"aot_countdown\": 9"));
        assert!(output.contains("\"aot_sum_to\": 10"));
        assert!(output.contains("\"aot_sum_below\": 11"));
        assert!(output.contains("\"aot_add_min_countdown_sum_to_sum_below\": 12"));
    }

    #[test]
    fn schema_eight_uses_null_when_no_tier_one_executable_is_available() {
        let output = render_json(
            "prototype-bench",
            Config {
                samples: 3,
                lowering_iterations: 1,
                runtime_iterations: 1,
                startup_iterations: 2,
            },
            ArtifactSizes {
                add_code: 1,
                min_code: 2,
                countdown_code: 3,
                sum_to_code: 4,
                sum_below_code: 5,
                host_code: 6,
                add_object: 7,
                min_object: 8,
                countdown_object: 9,
                sum_to_object: 10,
                sum_below_object: 11,
                aot_executable: None,
            },
            &[],
        );

        assert!(output.contains("\"aot_add_min_countdown_sum_to_sum_below\": null"));
    }
}
