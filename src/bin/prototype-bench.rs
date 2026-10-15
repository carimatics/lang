use std::cmp::Ordering;
use std::env;
use std::fmt::Write as _;
use std::hint::black_box;
use std::process::{Command, ExitCode};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use lang::{PROTOTYPE_DOUBLE_HOST, prototype_add_function, prototype_host_callback_function};
use lang_bytecode::lower;
use lang_codegen_clif::CompiledFunction;
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
        "usage: cargo run --release --bin prototype-bench -- \\\n+         [--samples <count>] [--lowering-iterations <count>] \\\n+         [--runtime-iterations <count>]"
    );
}

fn run() -> Result<(), String> {
    let command_line = env::args().collect::<Vec<_>>().join(" ");
    let config = Config::parse(env::args().skip(1))?;

    let add_function = prototype_add_function();
    let host_function = prototype_host_callback_function();
    let add_program = lower(&add_function).map_err(|error| error.to_string())?;
    let host_program = lower(&host_function).map_err(|error| error.to_string())?;
    let host_functions = prototype_host_registry()?;
    let compiled_add =
        CompiledFunction::compile(&add_function).map_err(|error| error.to_string())?;
    let compiled_host = CompiledFunction::compile_with_hosts(&host_function, &host_functions)
        .map_err(|error| error.to_string())?;

    check_results(
        &add_program,
        &host_program,
        &host_functions,
        &compiled_add,
        &compiled_host,
    )?;
    warm_up(
        config.runtime_iterations,
        &add_program,
        &host_program,
        &host_functions,
        &compiled_add,
        &compiled_host,
    );

    let mut measurements = Vec::new();
    measurements.push(measure_construction(
        "bytecode_lower_add",
        config.samples,
        config.lowering_iterations,
        || lower(black_box(&add_function)).expect("validated prototype MIR lowers"),
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
            compiled_add.code_size(),
            compiled_host.code_size(),
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
extern "C" fn prototype_wrapping_double(value: i64) -> i64 {
    value.wrapping_mul(2)
}

fn check_results(
    add_program: &lang_bytecode::Program,
    host_program: &lang_bytecode::Program,
    host_functions: &HostFunctionRegistry,
    compiled_add: &CompiledFunction,
    compiled_host: &CompiledFunction,
) -> Result<(), String> {
    let expected_add = rust_wrapping_add(20, 22);
    let vm_add = execute(add_program, &[20, 22]).map_err(|error| error.to_string())?;
    let jit_add = compiled_add
        .invoke_i64_2(20, 22)
        .map_err(|error| error.to_string())?;
    let expected_host = prototype_wrapping_double(21);
    let vm_host = execute_with_hosts(host_program, &[21], host_functions)
        .map_err(|error| error.to_string())?;
    let jit_host = compiled_host
        .invoke_i64_1(21)
        .map_err(|error| error.to_string())?;

    if (vm_add, jit_add, vm_host, jit_host)
        != (expected_add, expected_add, expected_host, expected_host)
    {
        return Err("execution modes disagreed before measurement".to_owned());
    }
    Ok(())
}

fn warm_up(
    runtime_iterations: u64,
    add_program: &lang_bytecode::Program,
    host_program: &lang_bytecode::Program,
    host_functions: &HostFunctionRegistry,
    compiled_add: &CompiledFunction,
    compiled_host: &CompiledFunction,
) {
    let iterations = runtime_iterations.clamp(1_000, 10_000);
    for iteration in 0..iterations {
        let value = iteration as i64;
        black_box(rust_wrapping_add(value, 17));
        black_box(execute(add_program, &[value, 17]).expect("verified bytecode executes"));
        black_box(
            compiled_add
                .invoke_i64_2(value, 17)
                .expect("compiled prototype has the expected signature"),
        );
        black_box(prototype_wrapping_double(value));
        black_box(
            execute_with_hosts(host_program, &[value], host_functions)
                .expect("verified host-call bytecode executes"),
        );
        black_box(
            compiled_host
                .invoke_i64_1(value)
                .expect("compiled host callback has the expected signature"),
        );
    }
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
    add_code_size: usize,
    host_code_size: usize,
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
    writeln!(output, "  \"schema_version\": 1,").expect("writing to String succeeds");
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
    writeln!(output, "  \"samples\": {},", config.samples).expect("writing to String succeeds");
    writeln!(output, "  \"generated_code_bytes\": {{").expect("writing to String succeeds");
    writeln!(output, "    \"jit_add\": {add_code_size},").expect("writing to String succeeds");
    writeln!(output, "    \"jit_host_callback\": {host_code_size}")
        .expect("writing to String succeeds");
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
