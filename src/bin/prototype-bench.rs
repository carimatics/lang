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
    PROTOTYPE_DOUBLE_HOST, prototype_add_function, prototype_host_callback_function,
    prototype_min_function,
};
use lang_bytecode::lower;
use lang_codegen_clif::{CompiledFunction, emit_host_object};
use lang_runtime::HostFunctionRegistry;
use lang_vm::{execute, execute_with_hosts};

const DEFAULT_SAMPLES: usize = 31;
const DEFAULT_LOWERING_ITERATIONS: u64 = 1_000;
const DEFAULT_RUNTIME_ITERATIONS: u64 = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Config {
    samples: usize,
    lowering_iterations: u64,
    runtime_iterations: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            samples: DEFAULT_SAMPLES,
            lowering_iterations: DEFAULT_LOWERING_ITERATIONS,
            runtime_iterations: DEFAULT_RUNTIME_ITERATIONS,
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
    host_program: &'a lang_bytecode::Program,
    host_functions: &'a HostFunctionRegistry,
    compiled_add: &'a CompiledFunction,
    compiled_min: &'a CompiledFunction,
    compiled_host: &'a CompiledFunction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ArtifactSizes {
    add_code: usize,
    min_code: usize,
    host_code: usize,
    add_object: usize,
    min_object: usize,
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
    let host_function = prototype_host_callback_function();
    let add_program = lower(&add_function).map_err(|error| error.to_string())?;
    let min_program = lower(&min_function).map_err(|error| error.to_string())?;
    let host_program = lower(&host_function).map_err(|error| error.to_string())?;
    let host_functions = prototype_host_registry()?;
    let compiled_add =
        CompiledFunction::compile(&add_function).map_err(|error| error.to_string())?;
    let compiled_min =
        CompiledFunction::compile(&min_function).map_err(|error| error.to_string())?;
    let compiled_host = CompiledFunction::compile_with_hosts(&host_function, &host_functions)
        .map_err(|error| error.to_string())?;
    let aot_add = emit_host_object(&add_function).map_err(|error| error.to_string())?;
    let aot_min = emit_host_object(&min_function).map_err(|error| error.to_string())?;
    #[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
    let link_benchmark = LinkBenchmark::prepare(aot_add.bytes(), aot_min.bytes())?;

    let workloads = PreparedWorkloads {
        add_program: &add_program,
        min_program: &min_program,
        host_program: &host_program,
        host_functions: &host_functions,
        compiled_add: &compiled_add,
        compiled_min: &compiled_min,
        compiled_host: &compiled_host,
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
    #[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
    measurements.push(link_benchmark.measure(config.samples)?);
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

    println!(
        "{}",
        render_json(
            &command_line,
            config,
            ArtifactSizes {
                add_code: compiled_add.code_size(),
                min_code: compiled_min.code_size(),
                host_code: compiled_host.code_size(),
                add_object: aot_add.bytes().len(),
                min_object: aot_min.bytes().len(),
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

#[inline(never)]
extern "C" fn prototype_wrapping_double(value: i64) -> i64 {
    value.wrapping_mul(2)
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
    let expected_host = prototype_wrapping_double(21);
    let vm_host = execute_with_hosts(workloads.host_program, &[21], workloads.host_functions)
        .map_err(|error| error.to_string())?;
    let jit_host = workloads
        .compiled_host
        .invoke_i64_1(21)
        .map_err(|error| error.to_string())?;

    if (vm_add, jit_add, vm_host, jit_host)
        != (expected_add, expected_add, expected_host, expected_host)
    {
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
    }
}

#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
const LINK_HARNESS: &str = r#"
#include <stdint.h>

extern int64_t prototype_add(int64_t left, int64_t right);
extern int64_t prototype_min(int64_t left, int64_t right);

int main(void) {
    return prototype_add(20, 22) == 42 && prototype_min(-7, 4) == -7 ? 0 : 1;
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
    executable: PathBuf,
}

#[cfg(any(target_os = "macos", all(target_os = "windows", target_env = "msvc")))]
impl LinkBenchmark {
    fn prepare(add_object_bytes: &[u8], min_object_bytes: &[u8]) -> Result<Self, String> {
        let directory = BenchmarkDirectory::create()?;
        let harness_source = directory.path().join("harness.c");
        #[cfg(target_os = "macos")]
        let (harness_object, add_object, min_object, executable) = (
            directory.path().join("harness.o"),
            directory.path().join("prototype_add.o"),
            directory.path().join("prototype_min.o"),
            directory.path().join("prototype-aot"),
        );
        #[cfg(all(target_os = "windows", target_env = "msvc"))]
        let (harness_object, add_object, min_object, executable) = (
            directory.path().join("harness.obj"),
            directory.path().join("prototype_add.obj"),
            directory.path().join("prototype_min.obj"),
            directory.path().join("prototype-aot.exe"),
        );
        fs::write(&harness_source, LINK_HARNESS).map_err(|error| error.to_string())?;
        fs::write(&add_object, add_object_bytes).map_err(|error| error.to_string())?;
        fs::write(&min_object, min_object_bytes).map_err(|error| error.to_string())?;

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
            executable,
        };
        benchmark.link_once()?;
        let execution = Command::new(&benchmark.executable)
            .output()
            .map_err(|error| format!("failed to run the linked executable: {error}"))?;
        ensure_command_succeeded("linked executable", &execution)?;
        Ok(benchmark)
    }

    fn measure(&self, sample_count: usize) -> Result<Measurement, String> {
        let mut samples = Vec::with_capacity(sample_count);
        for _ in 0..sample_count {
            let started = Instant::now();
            self.link_once()?;
            samples.push(started.elapsed().as_secs_f64() * 1_000_000_000.0);
        }
        Ok(Measurement::from_samples("system_link_add_min", 1, samples))
    }

    fn link_once(&self) -> Result<(), String> {
        let request = LinkRequest::new(&self.executable)
            .with_object(&self.harness_object)
            .with_object(&self.add_object)
            .with_object(&self.min_object);
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
    writeln!(output, "  \"schema_version\": 2,").expect("writing to String succeeds");
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
    writeln!(output, "  \"generated_code_bytes\": {{").expect("writing to String succeeds");
    writeln!(output, "    \"jit_add\": {},", sizes.add_code).expect("writing to String succeeds");
    writeln!(output, "    \"jit_min\": {},", sizes.min_code).expect("writing to String succeeds");
    writeln!(output, "    \"jit_host_callback\": {}", sizes.host_code)
        .expect("writing to String succeeds");
    writeln!(output, "  }},").expect("writing to String succeeds");
    writeln!(output, "  \"object_file_bytes\": {{").expect("writing to String succeeds");
    writeln!(output, "    \"aot_add\": {},", sizes.add_object).expect("writing to String succeeds");
    writeln!(output, "    \"aot_min\": {}", sizes.min_object).expect("writing to String succeeds");
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
    use super::{Config, Measurement, escape_json};

    #[test]
    fn parses_explicit_configuration() {
        let config = Config::parse([
            "--samples".to_owned(),
            "5".to_owned(),
            "--lowering-iterations".to_owned(),
            "7".to_owned(),
            "--runtime-iterations".to_owned(),
            "11".to_owned(),
        ]);

        assert_eq!(
            config,
            Ok(Config {
                samples: 5,
                lowering_iterations: 7,
                runtime_iterations: 11,
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
}
