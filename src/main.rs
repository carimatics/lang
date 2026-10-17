use lang::{
    frontend::{self, AnalysisError, CompileLimits, ScriptError},
    repl::{Event, Readiness, ReplError, ReplLimits, Report, Session, SubmissionBuffer},
    run_prototype_add,
};
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit::{self as vm, Control, HostBindings, Limits};
use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: lang check [--source-root DIR] FILE\n       lang run [--source-root DIR] [--fuel N] [--host-calls N] [--frames N] [--cells N] FILE\n       lang repl [--source-root DIR] [--fuel N] [--host-calls N] [--frames N] [--cells N]\n       lang lsp\n       lang <left> <right>\n";
enum Command {
    Check {
        file: PathBuf,
        root: Option<PathBuf>,
    },
    Run {
        file: PathBuf,
        root: Option<PathBuf>,
        limits: Limits,
    },
    Repl {
        root: Option<PathBuf>,
        limits: Limits,
    },
    Lsp,
    Prototype(i64, i64),
    Help,
}
enum CliError {
    Usage(String),
    Failure(String),
}
fn main() -> ExitCode {
    ExitCode::from(match run() {
        Ok(status) => status,
        Err(CliError::Usage(message)) => {
            eprintln!("error: {message}\n{USAGE}");
            2
        }
        Err(CliError::Failure(message)) => {
            eprintln!("error: {message}");
            1
        }
    })
}
fn usage(message: impl Into<String>) -> CliError {
    CliError::Usage(message.into())
}
fn failure(error: impl std::fmt::Display) -> CliError {
    CliError::Failure(error.to_string())
}
fn number<T: std::str::FromStr>(value: &OsStr, name: &str) -> Result<T, CliError> {
    value
        .to_str()
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| usage(format!("invalid {name}")))
}
fn parse(arguments: Vec<OsString>) -> Result<Command, CliError> {
    let Some(first) = arguments.first() else {
        return Err(usage("missing command"));
    };
    if first == "--help" || first == "-h" {
        return Ok(Command::Help);
    }
    if first == "lsp" {
        if arguments.len() != 1 {
            return Err(usage(
                "lsp accepts no arguments; initialize supplies the source root",
            ));
        }
        return Ok(Command::Lsp);
    }
    let repl = first == "repl";
    let check = first == "check";
    if first != "run" && !repl && !check {
        if arguments.len() != 2 {
            return Err(usage("expected a command or two integers"));
        }
        return Ok(Command::Prototype(
            number(&arguments[0], "left integer")?,
            number(&arguments[1], "right integer")?,
        ));
    }
    let mut file = None;
    let mut root = None;
    let mut limits = Limits::default();
    let mut seen = std::collections::BTreeSet::new();
    let mut arguments = arguments.into_iter().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            return Ok(Command::Help);
        }
        if let Some(flag) = argument.to_str().filter(|s| s.starts_with("--")) {
            if check && flag != "--source-root" {
                return Err(usage(format!("unknown check option {flag}")));
            }
            if !matches!(
                flag,
                "--source-root" | "--fuel" | "--host-calls" | "--frames" | "--cells"
            ) {
                return Err(usage(format!("unknown option {flag}")));
            }
            if !seen.insert(flag.to_owned()) {
                return Err(usage(format!("duplicate option {flag}")));
            }
            let value = arguments
                .next()
                .ok_or_else(|| usage(format!("missing value for {flag}")))?;
            match flag {
                "--source-root" => root = Some(PathBuf::from(value)),
                "--fuel" => limits.fuel = Some(number(&value, flag)?),
                "--host-calls" => limits.host_calls = Some(number(&value, flag)?),
                "--frames" => limits.frames = Some(number(&value, flag)?),
                "--cells" => limits.live_cells = Some(number(&value, flag)?),
                _ => unreachable!(),
            }
        } else if repl {
            return Err(usage("repl accepts no positional arguments"));
        } else if file.replace(PathBuf::from(argument)).is_some() {
            return Err(usage("only one root file is allowed"));
        }
    }
    if repl {
        return Ok(Command::Repl { root, limits });
    }
    if check {
        return Ok(Command::Check {
            file: file.ok_or_else(|| usage("missing root file"))?,
            root,
        });
    }
    Ok(Command::Run {
        file: file.ok_or_else(|| usage("missing root file"))?,
        root,
        limits,
    })
}
fn run() -> Result<u8, CliError> {
    match parse(std::env::args_os().skip(1).collect())? {
        Command::Help => {
            print!("{USAGE}");
            Ok(0)
        }
        Command::Lsp => lang::lsp::serve(
            &mut io::stdin().lock(),
            &mut io::stdout().lock(),
            lang::lsp::Limits::default(),
        )
        .map_err(failure),
        Command::Prototype(left, right) => {
            let results = run_prototype_add(left, right).map_err(failure)?;
            println!(
                "bytecode: {}\nnative:   {}",
                results.bytecode, results.native
            );
            if results.bytecode != results.native {
                return Err(failure("execution modes produced different results"));
            }
            Ok(0)
        }
        Command::Check { file, root } => check_source(file, root),
        Command::Run { file, root, limits } => run_script(file, root, limits),
        Command::Repl { root, limits } => run_repl(root, limits),
    }
}

fn run_repl(root: Option<PathBuf>, limits: Limits) -> Result<u8, CliError> {
    let root = root
        .map(std::fs::canonicalize)
        .transpose()
        .map_err(failure)?;
    if root.as_ref().is_some_and(|root| !root.is_dir()) {
        return Err(failure("REPL source root must be a directory"));
    }
    let mut session = Session::new(root, ReplLimits::default());
    let byte_limit = session.limits().compile.modules.parse.lex.source_bytes;
    let depth = session
        .limits()
        .compile
        .modules
        .parse
        .depth
        .min(session.limits().compile.modules.parse.lex.comment_depth);
    let hosts = HostFunctionRegistry::new();
    let bindings = HostBindings::new();
    let interactive = io::stdin().is_terminal();
    let mut input = io::stdin().lock();
    let mut buffer = SubmissionBuffer::default();
    loop {
        if interactive {
            let mut output = io::stdout().lock();
            write!(
                output,
                "{}",
                if buffer.bytes().is_empty() {
                    "> "
                } else {
                    ".. "
                }
            )
            .map_err(failure)?;
            output.flush().map_err(failure)?;
        }
        let mut line = Vec::new();
        let read = (&mut input)
            .take(
                byte_limit
                    .saturating_sub(buffer.bytes().len())
                    .saturating_add(1) as u64,
            )
            .read_until(b'\n', &mut line)
            .map_err(failure)?;
        if read == 0 {
            if buffer.bytes().is_empty() {
                return Ok(0);
            }
            let report = session.submit(
                buffer.bytes(),
                &hosts,
                &bindings,
                Control::with_limits(limits),
            );
            let failed = report.result.is_err();
            print_repl_report(&report)?;
            return Ok(u8::from(failed));
        }
        let readiness = match buffer.push(&line, byte_limit, depth) {
            Ok(readiness) => readiness,
            Err(_) => {
                eprintln!("error[SRC0011]: REPL submission byte limit exceeded");
                return Ok(1);
            }
        };
        if readiness == Readiness::Continue {
            continue;
        }
        let report = session.submit(
            buffer.bytes(),
            &hosts,
            &bindings,
            Control::with_limits(limits),
        );
        print_repl_report(&report)?;
        buffer.clear();
        if matches!(report.result, Ok(Event::Quit)) {
            return Ok(0);
        }
    }
}
fn print_repl_report(report: &Report) -> Result<(), CliError> {
    let mut stderr = io::stderr().lock();
    for diagnostic in &report.diagnostics {
        stderr
            .write_all(
                diagnostic
                    .render(&report.sources)
                    .map_err(failure)?
                    .as_bytes(),
            )
            .map_err(failure)?;
    }
    match &report.result {
        Ok(Event::Value(value)) => writeln!(io::stdout().lock(), "{value}").map_err(failure)?,
        Err(ReplError::Diagnostics) => {}
        Err(ReplError::Runtime(error)) => {
            writeln!(
                stderr,
                "error[runtime]: {:?} (fuel {}, host calls {})",
                error.reason, error.usage.fuel, error.usage.host_calls
            )
            .map_err(failure)?;
            for frame in &error.stack {
                let name = error.function_name(*frame);
                if let Some(span) = frame.span {
                    let source = report.sources.file(span.source()).map_err(failure)?;
                    let position = source.position(span.range().start).map_err(failure)?;
                    writeln!(
                        stderr,
                        "  at {name} ({:?}:{}:{})",
                        source.name(),
                        position.line,
                        position.column
                    )
                    .map_err(failure)?;
                } else {
                    writeln!(stderr, "  at {name}").map_err(failure)?;
                }
            }
        }
        Err(error) => writeln!(stderr, "error: {error}").map_err(failure)?,
        Ok(_) => {}
    }
    Ok(())
}
fn source_root(file: &std::path::Path, root: Option<PathBuf>) -> PathBuf {
    root.unwrap_or_else(|| {
        file.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf()
    })
}
fn check_source(file: PathBuf, root: Option<PathBuf>) -> Result<u8, CliError> {
    let root = source_root(&file, root);
    let analysis = frontend::analyze_file(&file, &root, CompileLimits::default());
    let mut stderr = io::stderr().lock();
    for diagnostic in &analysis.diagnostics {
        stderr
            .write_all(
                diagnostic
                    .render(&analysis.sources)
                    .map_err(failure)?
                    .as_bytes(),
            )
            .map_err(failure)?;
    }
    match analysis.result {
        Ok(_) => Ok(0),
        Err(AnalysisError::Diagnostics) => Ok(1),
        Err(error) => Err(failure(error)),
    }
}
fn run_script(file: PathBuf, root: Option<PathBuf>, limits: Limits) -> Result<u8, CliError> {
    // CLI is the host choosing compiler inputs. Child modules use this
    // source root; source execution itself acquires no file authority.
    let root = source_root(&file, root);
    let compile = CompileLimits::default();
    let analysis = frontend::analyze_file(&file, &root, compile);
    let mut stderr = io::stderr().lock();
    for diagnostic in &analysis.diagnostics {
        stderr
            .write_all(
                diagnostic
                    .render(&analysis.sources)
                    .map_err(failure)?
                    .as_bytes(),
            )
            .map_err(failure)?;
    }
    let typed = match analysis.result {
        Ok(typed) => typed,
        Err(AnalysisError::Diagnostics) => return Ok(1),
        Err(error) => return Err(failure(error)),
    };
    if let lang_semantics::analysis::Program::Ownership(program) = &typed {
        let mut instance = lang_runtime::cleanup::Instance::default();
        let mut allocator = lang_runtime::allocation::Allocator::new(
            lang_runtime::allocation::System,
            Default::default(),
        )
        .map_err(failure)?;
        return match vm::owned::execute_entry(
            program.failure_flow(),
            &mut instance,
            Some(&mut allocator),
            Control::with_limits(limits),
        ) {
            Ok(outcome) => {
                writeln!(io::stdout().lock(), "{}", outcome.value).map_err(failure)?;
                Ok(0)
            }
            Err(error) => {
                writeln!(
                    stderr,
                    "error[runtime]: {error} (fuel {}, host calls {})",
                    error.usage.fuel, error.usage.host_calls
                )
                .map_err(failure)?;
                if let Some(span) = error.fault {
                    let types = program
                        .mir()
                        .activity()
                        .moved()
                        .typed()
                        .places()
                        .declarations()
                        .types();
                    if let Some(source) = types.table().sources.get(span.source.0 as usize)
                        && let Some(prefix) = source.text.get(..span.start as usize)
                    {
                        let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
                        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
                        writeln!(stderr, "  at main ({:?}:{line}:{column})", source.path)
                            .map_err(failure)?;
                    }
                }
                Ok(1)
            }
        };
    }
    let typed = frontend::copy_executable(&typed).map_err(failure)?;
    let program = match frontend::compile_script(typed, &analysis.sources, compile) {
        Ok(program) => program,
        Err(ScriptError::Entry(diagnostic)) => {
            stderr
                .write_all(
                    diagnostic
                        .render(&analysis.sources)
                        .map_err(failure)?
                        .as_bytes(),
                )
                .map_err(failure)?;
            return Ok(1);
        }
        Err(error) => return Err(failure(error)),
    };
    match vm::execute_entry(
        &program,
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        Control::with_limits(limits),
    ) {
        Ok(outcome) => {
            writeln!(
                io::stdout().lock(),
                "{}",
                outcome.value.as_i64().expect("verified script result")
            )
            .map_err(failure)?;
            Ok(0)
        }
        Err(error) => {
            writeln!(
                stderr,
                "error[runtime]: {:?} (fuel {}, host calls {})",
                error.reason, error.usage.fuel, error.usage.host_calls
            )
            .map_err(failure)?;
            for frame in &error.stack {
                let unit = program.verified();
                let name = &unit.unit().functions[frame.function.0 as usize].name;
                if let Some(span) = frame.span {
                    let source = &unit.unit().sources[span.source.0 as usize];
                    let (line, column) = unit.facts().sources()[span.source.0 as usize]
                        .position(source, span.start)
                        .expect("verified span");
                    writeln!(stderr, "  at {name} ({:?}:{line}:{column})", source.path)
                        .map_err(failure)?;
                } else {
                    writeln!(stderr, "  at {name}").map_err(failure)?;
                }
            }
            Ok(1)
        }
    }
}
