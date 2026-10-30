use lang::{
    frontend::{self, AnalysisError, CompileLimits, ScriptError},
    repl::{Event, Readiness, ReplError, ReplLimits, Report, Session, SubmissionBuffer},
    run_prototype_add,
};
use lang_runtime::HostFunctionRegistry;
use lang_vm::unit::{self as vm, HostBindings};
use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use lang::package::command::ExecutionOptions;

const USAGE: &str = "usage: lang check [--source-root DIR] FILE\n       lang compile [--source-root DIR] --output ARTIFACT SOURCE\n       lang native [--source-root DIR] [--target TRIPLE] --output DIR SOURCE\n       lang link-native --sdk SDK --package DIR [--host HOST.rs [--object HOST.o ...]] --output EXE\n       lang run [--source-root DIR] [--fuel N] [--service-work N] [--host-calls N] [--frames N] [--cells N] FILE\n       lang run --artifact [--fuel N] [--service-work N] [--host-calls N] [--frames N] [--cells N] ARTIFACT\n       lang repl [--source-root DIR] [--fuel N] [--service-work N] [--host-calls N] [--frames N] [--cells N]\n       lang lsp\n       lang dap\n       lang <left> <right>\n\nArtifact schemas: Copy/ownership/C 1-4; tasks/channels/services 5. Component artifacts require explicit host bindings.\n";
enum Command {
    Check {
        file: PathBuf,
        root: Option<PathBuf>,
    },
    Run {
        file: PathBuf,
        root: Option<PathBuf>,
        limits: ExecutionOptions,
    },
    Compile {
        file: PathBuf,
        root: Option<PathBuf>,
        output: PathBuf,
    },
    Native {
        file: PathBuf,
        root: Option<PathBuf>,
        output: PathBuf,
        target: Option<String>,
    },
    LinkNative(lang::native::link::Request),
    RunArtifact {
        file: PathBuf,
        limits: ExecutionOptions,
    },
    Repl {
        root: Option<PathBuf>,
        limits: ExecutionOptions,
    },
    Lsp,
    Dap,
    Prototype(i64, i64),
    Help,
}
enum CliError {
    Usage(String),
    Failure(String),
}
fn main() -> ExitCode {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|arg| arg == "toolchain") {
        use lang::toolchain::command::Outcome;
        let result = lang::toolchain::command::run(
            &arguments[1..],
            &mut io::stdout().lock(),
            &mut io::stderr().lock(),
        );
        return match result {
            Ok(Outcome::Code(code)) => ExitCode::from(code),
            Ok(Outcome::Child(status)) => {
                #[cfg(unix)]
                let code = status.code().unwrap_or_else(|| {
                    use std::os::unix::process::ExitStatusExt;
                    128 + status.signal().unwrap_or(1)
                });
                #[cfg(not(unix))]
                let code = status.code().unwrap_or(1);
                std::process::exit(code)
            }
            Err(error) => {
                eprintln!("error: {error}");
                ExitCode::from(1)
            }
        };
    }
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
    if first == "dap" {
        if arguments.len() != 1 {
            return Err(usage(
                "dap accepts no arguments; launch supplies the package root",
            ));
        }
        return Ok(Command::Dap);
    }
    if first == "lsp" {
        if arguments.len() != 1 {
            return Err(usage(
                "lsp accepts no arguments; initialize supplies the source root",
            ));
        }
        return Ok(Command::Lsp);
    }
    if first == "link-native" {
        return parse_link_native(arguments);
    }
    if first == "native" {
        return parse_native(arguments);
    }
    let repl = first == "repl";
    let check = first == "check";
    let compile = first == "compile";
    if first != "run" && !repl && !check && !compile {
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
    let mut output = None;
    let mut artifact = false;
    let mut limits = ExecutionOptions::default();
    let mut seen = std::collections::BTreeSet::new();
    let mut arguments = arguments.into_iter().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--help" || argument == "-h" {
            return Ok(Command::Help);
        }
        if let Some(flag) = argument.to_str().filter(|s| s.starts_with("--")) {
            if flag == "--artifact" && !check && !repl && !compile {
                if !seen.insert(flag.to_owned()) {
                    return Err(usage(format!("duplicate option {flag}")));
                }
                artifact = true;
                continue;
            }
            if check && flag != "--source-root" {
                return Err(usage(format!("unknown check option {flag}")));
            }
            if compile && !matches!(flag, "--source-root" | "--output") {
                return Err(usage(format!("unknown compile option {flag}")));
            }
            if !matches!(
                flag,
                "--source-root"
                    | "--fuel"
                    | "--service-work"
                    | "--host-calls"
                    | "--frames"
                    | "--cells"
                    | "--output"
            ) || (flag == "--output" && !compile)
            {
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
                "--output" => output = Some(PathBuf::from(value)),
                "--fuel" => limits.limits.fuel = Some(number(&value, flag)?),
                "--service-work" => limits.service_work = Some(number(&value, flag)?),
                "--host-calls" => limits.limits.host_calls = Some(number(&value, flag)?),
                "--frames" => limits.limits.frames = Some(number(&value, flag)?),
                "--cells" => limits.limits.live_cells = Some(number(&value, flag)?),
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
    if compile {
        return Ok(Command::Compile {
            file: file.ok_or_else(|| usage("missing root file"))?,
            root,
            output: output.ok_or_else(|| usage("missing --output"))?,
        });
    }
    if artifact {
        if root.is_some() {
            return Err(usage("--source-root is only valid for source input"));
        }
        return Ok(Command::RunArtifact {
            file: file.ok_or_else(|| usage("missing artifact file"))?,
            limits,
        });
    }
    Ok(Command::Run {
        file: file.ok_or_else(|| usage("missing root file"))?,
        root,
        limits,
    })
}
fn parse_link_native(arguments: Vec<OsString>) -> Result<Command, CliError> {
    let (mut sdk, mut package, mut host, mut output) = (None, None, None, None);
    let mut objects = Vec::new();
    let mut args = arguments.into_iter().skip(1);
    while let Some(option) = args.next() {
        if option == "--help" || option == "-h" {
            return Ok(Command::Help);
        }
        let flag = option
            .to_str()
            .ok_or_else(|| usage("link-native options must be UTF-8"))?;
        if !matches!(
            flag,
            "--sdk" | "--package" | "--host" | "--output" | "--object"
        ) {
            return Err(usage(format!("unknown link-native option {flag}")));
        }
        let value = args
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| usage(format!("missing value for {flag}")))?;
        let value = PathBuf::from(value);
        if flag == "--object" {
            objects.push(value);
            continue;
        }
        let slot = match flag {
            "--sdk" => &mut sdk,
            "--package" => &mut package,
            "--host" => &mut host,
            "--output" => &mut output,
            _ => unreachable!(),
        };
        if slot.replace(value).is_some() {
            return Err(usage(format!("duplicate option {flag}")));
        }
    }
    if !objects.is_empty() && host.is_none() {
        return Err(usage("--object requires an explicit --host"));
    }
    Ok(Command::LinkNative(lang::native::link::Request {
        rustc: std::env::var_os("RUSTC"),
        sdk: sdk.ok_or_else(|| usage("missing --sdk"))?,
        package: package.ok_or_else(|| usage("missing --package"))?,
        host,
        objects,
        output: output.ok_or_else(|| usage("missing --output"))?,
    }))
}
fn parse_native(arguments: Vec<OsString>) -> Result<Command, CliError> {
    let mut file = None;
    let mut root = None;
    let mut output = None;
    let mut target = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut args = arguments.into_iter().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--help" || arg == "-h" {
            return Ok(Command::Help);
        }
        if let Some(flag) = arg.to_str().filter(|s| s.starts_with("--")) {
            if !matches!(flag, "--source-root" | "--output" | "--target") {
                return Err(usage(format!("unknown native option {flag}")));
            }
            if !seen.insert(flag.to_owned()) {
                return Err(usage(format!("duplicate option {flag}")));
            }
            let value = args
                .next()
                .ok_or_else(|| usage(format!("missing value for {flag}")))?;
            match flag {
                "--source-root" => root = Some(PathBuf::from(value)),
                "--output" => output = Some(PathBuf::from(value)),
                "--target" => {
                    target = Some(
                        value
                            .into_string()
                            .map_err(|_| usage("target must be UTF-8"))?,
                    )
                }
                _ => unreachable!(),
            }
        } else if file.replace(PathBuf::from(arg)).is_some() {
            return Err(usage("only one root file is allowed"));
        }
    }
    Ok(Command::Native {
        file: file.ok_or_else(|| usage("missing root file"))?,
        root,
        output: output.ok_or_else(|| usage("missing --output"))?,
        target,
    })
}
fn compile_native(
    file: PathBuf,
    root: Option<PathBuf>,
    output: PathBuf,
    target: Option<String>,
) -> Result<u8, CliError> {
    let target = lang::native::target(
        &target.unwrap_or_else(|| lang_codegen_clif::Triple::host().to_string()),
    )
    .map_err(failure)?;
    let root = source_root(&file, root);
    let limits = CompileLimits::default();
    let analysis = frontend::analyze_file(&file, &root, limits);
    for d in &analysis.diagnostics {
        io::stderr()
            .lock()
            .write_all(d.render(&analysis.sources).map_err(failure)?.as_bytes())
            .map_err(failure)?;
    }
    let program = match analysis.result {
        Ok(p) => p,
        Err(AnalysisError::Diagnostics) => return Ok(1),
        Err(e) => return Err(failure(e)),
    };
    let package = match lang::native::compile(program, &analysis.sources, limits, target) {
        Ok(p) => p,
        Err(lang::native::Error::Entry(ScriptError::Entry(d))) => {
            io::stderr()
                .lock()
                .write_all(d.render(&analysis.sources).map_err(failure)?.as_bytes())
                .map_err(failure)?;
            return Ok(1);
        }
        Err(e) => return Err(failure(e)),
    };
    package.publish(&output).map_err(failure)?;
    Ok(0)
}
fn run() -> Result<u8, CliError> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|arg| arg == "compatibility") {
        return lang::package::compatibility::run(
            &arguments[1..],
            &mut io::stdout().lock(),
            &mut io::stderr().lock(),
        )
        .map_err(failure);
    }
    if arguments.first().is_some_and(|arg| arg == "profile") {
        return lang::profile::run(
            &arguments[1..],
            &mut io::stdout().lock(),
            &mut io::stderr().lock(),
        )
        .map_err(failure);
    }
    if lang::package::command::selected(&arguments) {
        return lang::package::command::run(
            &arguments,
            &mut io::stdout().lock(),
            &mut io::stderr().lock(),
            package_evaluate,
        )
        .map_err(failure);
    }
    if arguments
        .first()
        .is_some_and(|argument| argument == "package")
    {
        return lang::package::cli::run(&arguments[1..], &mut io::stderr().lock()).map_err(failure);
    }
    match parse(arguments)? {
        Command::Help => {
            print!("{USAGE}");
            print!("{}", lang::package::compatibility::USAGE);
            println!("       lang package check-manifest [--message-format human|json] FILE");
            print!("{}", lang::package::command::USAGE);
            print!("{}", lang::profile::USAGE);
            print!("{}", lang::toolchain::command::USAGE);
            Ok(0)
        }
        Command::Dap => lang::dap::serve_stdio().map_err(failure),
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
        Command::Compile { file, root, output } => compile_source(file, root, output),
        Command::Native {
            file,
            root,
            output,
            target,
        } => compile_native(file, root, output, target),
        Command::LinkNative(request) => {
            lang::native::link::link(&request, Default::default()).map_err(failure)?;
            Ok(0)
        }
        Command::RunArtifact { file, limits } => run_artifact(file, limits),
        Command::Repl { root, limits } => run_repl(root, limits),
    }
}

fn package_runtime(
    message: impl std::fmt::Display,
    location: Option<(String, usize, usize)>,
) -> lang::package::Diagnostic {
    let mut diagnostic = lang::package::Diagnostic::error("CLI0002", message.to_string());
    if let Some((path, start, end)) = location {
        diagnostic.path = Some(path);
        diagnostic.span = Some(lang::package::ByteSpan { start, end });
    }
    diagnostic
}
// This adapter invokes the same engines and allocator/control contracts as the
// explicit-file helpers below. It returns structured data before publication;
// it does not interpret source or grant host capabilities to guest code.
fn package_evaluate(
    program: &lang::artifact::Program,
    limits: ExecutionOptions,
) -> Result<i64, lang::package::Diagnostic> {
    match program {
        lang::artifact::Program::Copy(program) => {
            let unit = program.verified().unit();
            let entry = unit
                .entry
                .ok_or_else(|| package_runtime("artifact has no script entry", None))?;
            let signature = &unit.functions[entry.0 as usize].signature;
            if !signature.parameters.is_empty() || signature.result != lang_mir::unit::Type::I64 {
                return Err(package_runtime(
                    "script entry must have zero parameters and return i64",
                    None,
                ));
            }
            vm::execute_entry(
                program,
                &HostFunctionRegistry::new(),
                &HostBindings::new(),
                limits.control(),
            )
            .map(|o| o.value.as_i64().expect("verified script result"))
            .map_err(|e| {
                let location = e.stack.iter().find_map(|f| f.span).and_then(|s| {
                    unit.sources
                        .get(s.source.0 as usize)
                        .map(|source| (source.path.clone(), s.start as usize, s.end as usize))
                });
                package_runtime(
                    format!(
                        "{:?} (fuel {}, host calls {}) (service work {})",
                        e.reason, e.usage.fuel, e.usage.host_calls, e.usage.service_work
                    ),
                    location,
                )
            })
        }
        lang::artifact::Program::Ownership(program) => {
            let entry = program
                .entry()
                .ok_or_else(|| package_runtime("artifact has no script entry", None))?;
            let mut instance = lang_runtime::cleanup::Instance::default();
            let mut allocator = lang_runtime::allocation::Allocator::new(
                lang_runtime::allocation::System,
                Default::default(),
            )
            .map_err(|e| package_runtime(e, None))?;
            vm::owned::execute_function(
                program.flow(),
                entry,
                &mut instance,
                Some(&mut allocator),
                limits.control(),
            )
            .map(|o| o.value)
            .map_err(|e| {
                let sources = &program
                    .flow()
                    .source()
                    .activity()
                    .moved()
                    .typed()
                    .places()
                    .declarations()
                    .types()
                    .table()
                    .sources;
                let span = e.fault.or_else(|| e.stack.iter().find_map(|f| f.span));
                let location = span.and_then(|s| {
                    sources
                        .get(s.source.0 as usize)
                        .map(|source| (source.path.clone(), s.start as usize, s.end as usize))
                });
                package_runtime(
                    format!(
                        "{e} (fuel {}, host calls {}) (service work {})",
                        e.usage.fuel, e.usage.host_calls, e.usage.service_work
                    ),
                    location,
                )
            })
        }
        lang::artifact::Program::Tasks(program) => {
            let entry = program
                .entry()
                .ok_or_else(|| package_runtime("artifact has no script entry", None))?;
            let mut allocator = lang_runtime::allocation::Allocator::new(
                lang_runtime::allocation::System,
                Default::default(),
            )
            .map_err(|e| package_runtime(e, None))?;
            match vm::tasks::execute_function(
                program.proof(),
                entry,
                &mut allocator,
                limits.control(),
                Default::default(),
            ) {
                Ok(o) if o.poisoned || o.report.is_some() => Err(package_runtime(
                    "task cleanup failed; execution instance is poisoned",
                    None,
                )),
                Ok(o) => Ok(o.value),
                Err(e) => {
                    let sources = &program
                        .proof()
                        .typed()
                        .places()
                        .declarations()
                        .types()
                        .table()
                        .sources;
                    let location = e.span.and_then(|s| {
                        sources
                            .get(s.source.0 as usize)
                            .map(|source| (source.path.clone(), s.start as usize, s.end as usize))
                    });
                    Err(package_runtime(
                        format!("{e} (fuel {}, tasks {})", e.usage.fuel, e.tasks.admitted),
                        location,
                    ))
                }
            }
        }
        lang::artifact::Program::Component(_) => Err(package_runtime(
            "component artifacts require explicit host bindings",
            None,
        )),
    }
}

fn run_repl(root: Option<PathBuf>, limits: ExecutionOptions) -> Result<u8, CliError> {
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
            let report = submit_repl(&mut session, buffer.bytes(), &hosts, &bindings, limits)?;
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
        let report = submit_repl(&mut session, buffer.bytes(), &hosts, &bindings, limits)?;
        print_repl_report(&report)?;
        buffer.clear();
        if matches!(report.result, Ok(Event::Quit)) {
            return Ok(0);
        }
    }
}
fn submit_repl(
    session: &mut Session,
    bytes: &[u8],
    hosts: &HostFunctionRegistry,
    bindings: &HostBindings,
    limits: ExecutionOptions,
) -> Result<Report, CliError> {
    let mut allocator = lang_runtime::allocation::Allocator::new(
        lang_runtime::allocation::System,
        Default::default(),
    )
    .map_err(failure)?;
    Ok(session.submit_with_allocator(bytes, hosts, bindings, &mut allocator, limits.control()))
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
                "error[runtime]: {:?} (fuel {}, host calls {}) (service work {})",
                error.reason, error.usage.fuel, error.usage.host_calls, error.usage.service_work
            )
            .map_err(failure)?;
            for frame in &error.stack {
                print_repl_frame(
                    &mut stderr,
                    &report.sources,
                    *frame,
                    error.function_name(*frame),
                )?;
            }
        }
        Err(ReplError::OwnedRuntime(error)) => {
            writeln!(stderr, "error[runtime]: {error}").map_err(failure)?;
            for frame in &error.stack {
                print_repl_frame(
                    &mut stderr,
                    &report.sources,
                    *frame,
                    error.function_name(*frame),
                )?;
            }
            if !error.trace_complete {
                writeln!(stderr, "  stack trace truncated").map_err(failure)?;
            }
            if let Some(cleanup) = &error.report {
                if let Some(primary) = &cleanup.primary {
                    print_repl_cleanup(&mut stderr, &report.sources, "primary", primary)?;
                }
                for secondary in &cleanup.secondary {
                    print_repl_cleanup(&mut stderr, &report.sources, "secondary", secondary)?;
                }
                if cleanup.truncated != 0 {
                    writeln!(stderr, "  {} additional failure records", cleanup.truncated)
                        .map_err(failure)?;
                }
                if cleanup.quarantined != 0 {
                    writeln!(stderr, "  {} quarantined host values", cleanup.quarantined)
                        .map_err(failure)?;
                }
            }
        }
        Err(error) => writeln!(stderr, "error: {error}").map_err(failure)?,
        Ok(_) => {}
    }
    Ok(())
}
fn print_repl_frame(
    stderr: &mut impl Write,
    sources: &lang_diagnostics::SourceMap,
    frame: lang::repl::Frame,
    name: &str,
) -> Result<(), CliError> {
    if let Some(span) = frame.span {
        let source = sources.file(span.source()).map_err(failure)?;
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
    Ok(())
}
fn print_repl_cleanup(
    stderr: &mut impl Write,
    sources: &lang_diagnostics::SourceMap,
    label: &str,
    record: &lang::repl::CleanupFailure,
) -> Result<(), CliError> {
    write!(stderr, "  {label}: {:?}", record.kind).map_err(failure)?;
    if let Some(span) = record.span {
        let source = sources.file(span.source()).map_err(failure)?;
        let position = source.position(span.range().start).map_err(failure)?;
        write!(
            stderr,
            " ({:?}:{}:{})",
            source.name(),
            position.line,
            position.column
        )
        .map_err(failure)?;
    }
    writeln!(stderr).map_err(failure)
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
fn run_script(
    file: PathBuf,
    root: Option<PathBuf>,
    limits: ExecutionOptions,
) -> Result<u8, CliError> {
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
    if let lang_semantics::analysis::Program::Tasks(program) = &typed {
        drop(stderr);
        return run_tasks(program.verified(), None, limits);
    }
    if let lang_semantics::analysis::Program::Ownership(program) = &typed {
        drop(stderr);
        return run_owned(program.failure_flow(), OwnedEntry::Main, limits);
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
    drop(stderr);
    run_copy(&program, limits)
}

fn run_tasks(
    proof: &lang_mir::unit::ownership::tasks::VerifiedTasks,
    entry: Option<lang_mir::unit::FunctionId>,
    limits: ExecutionOptions,
) -> Result<u8, CliError> {
    let mut allocator = lang_runtime::allocation::Allocator::new(
        lang_runtime::allocation::System,
        Default::default(),
    )
    .map_err(failure)?;
    let result = if let Some(entry) = entry {
        vm::tasks::execute_function(
            proof,
            entry,
            &mut allocator,
            limits.control(),
            Default::default(),
        )
    } else {
        vm::tasks::execute_entry(proof, &mut allocator, limits.control(), Default::default())
    };
    match result {
        Ok(output) => {
            if output.poisoned || output.report.is_some() {
                let mut stderr = io::stderr().lock();
                writeln!(
                    stderr,
                    "error[runtime]: task cleanup failed; execution instance is poisoned"
                )
                .map_err(failure)?;
                if let Some(report) = &output.report {
                    print_task_cleanup(&mut stderr, report)?;
                }
                return Ok(1);
            }
            writeln!(io::stdout().lock(), "{}", output.value).map_err(failure)?;
            Ok(0)
        }
        Err(error) => {
            let mut stderr = io::stderr().lock();
            writeln!(
                stderr,
                "error[runtime]: {error} (fuel {}, tasks {})",
                error.usage.fuel, error.tasks.admitted
            )
            .map_err(failure)?;
            if let Some(report) = &error.report {
                print_task_cleanup(&mut stderr, report)?;
            }
            let declarations = proof.typed().places().declarations();
            if let Some(function) = error.function {
                let name = &declarations.functions()[function.0 as usize].name;
                if let Some(span) = error.span
                    && let Some(source) = declarations
                        .types()
                        .table()
                        .sources
                        .get(span.source.0 as usize)
                    && let Some(prefix) = source.text.get(..span.start as usize)
                {
                    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
                    let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
                    writeln!(stderr, "  at {name} ({:?}:{line}:{column})", source.path)
                        .map_err(failure)?;
                }
            }
            Ok(1)
        }
    }
}

fn print_task_cleanup(
    stderr: &mut impl Write,
    report: &lang_runtime::cleanup::Report,
) -> Result<(), CliError> {
    for (index, record) in report
        .primary()
        .into_iter()
        .chain(report.secondary())
        .enumerate()
    {
        let label = if index == 0 { "primary" } else { "secondary" };
        writeln!(stderr, "  {label}: {:?} at {:?}", record.kind, record.span).map_err(failure)?;
    }
    if report.truncated() != 0 {
        writeln!(
            stderr,
            "  {} additional failure records",
            report.truncated()
        )
        .map_err(failure)?;
    }
    if report.quarantined() != 0 {
        writeln!(stderr, "  {} quarantined host values", report.quarantined()).map_err(failure)?;
    }
    Ok(())
}

fn run_copy(
    program: &lang_bytecode::unit::Program,
    limits: ExecutionOptions,
) -> Result<u8, CliError> {
    let mut stderr = io::stderr().lock();
    let unit = program.verified().unit();
    let entry = unit
        .entry
        .ok_or_else(|| failure("artifact has no script entry"))?;
    let signature = &unit.functions[entry.0 as usize].signature;
    if !signature.parameters.is_empty() || signature.result != lang_mir::unit::Type::I64 {
        return Err(failure(
            "script entry must have zero parameters and return i64",
        ));
    }
    match vm::execute_entry(
        program,
        &HostFunctionRegistry::new(),
        &HostBindings::new(),
        limits.control(),
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
                "error[runtime]: {:?} (fuel {}, host calls {}) (service work {})",
                error.reason, error.usage.fuel, error.usage.host_calls, error.usage.service_work
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

enum OwnedEntry {
    Main,
    Function(lang_mir::unit::FunctionId),
}
fn run_owned(
    flow: &lang_mir::unit::ownership::failure::Elaborated,
    entry: OwnedEntry,
    limits: ExecutionOptions,
) -> Result<u8, CliError> {
    let mut instance = lang_runtime::cleanup::Instance::default();
    let mut allocator = lang_runtime::allocation::Allocator::new(
        lang_runtime::allocation::System,
        Default::default(),
    )
    .map_err(failure)?;
    let control = limits.control();
    let result = match entry {
        OwnedEntry::Main => {
            vm::owned::execute_entry(flow, &mut instance, Some(&mut allocator), control)
        }
        OwnedEntry::Function(id) => {
            vm::owned::execute_function(flow, id, &mut instance, Some(&mut allocator), control)
        }
    };
    match result {
        Ok(outcome) => {
            writeln!(io::stdout().lock(), "{}", outcome.value).map_err(failure)?;
            Ok(0)
        }
        Err(error) => {
            let mut stderr = io::stderr().lock();
            writeln!(
                stderr,
                "error[runtime]: {error} (fuel {}, host calls {}) (service work {})",
                error.usage.fuel, error.usage.host_calls, error.usage.service_work
            )
            .map_err(failure)?;
            let declarations = flow
                .source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations();
            let frames = error.stack.iter().copied().chain(
                error
                    .function
                    .filter(|_| error.stack.is_empty())
                    .map(|function| vm::StackFrame {
                        function,
                        span: error.fault,
                    }),
            );
            for frame in frames {
                let name = declarations
                    .functions()
                    .get(frame.function.0 as usize)
                    .map_or("<invalid>", |f| f.name.as_str());
                let Some(span) = frame.span else {
                    writeln!(stderr, "  at {name}").map_err(failure)?;
                    continue;
                };
                if let Some(source) = declarations
                    .types()
                    .table()
                    .sources
                    .get(span.source.0 as usize)
                    && let Some(prefix) = source.text.get(..span.start as usize)
                {
                    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
                    let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
                    writeln!(stderr, "  at {name} ({:?}:{line}:{column})", source.path)
                        .map_err(failure)?;
                }
            }
            Ok(1)
        }
    }
}

fn compile_source(file: PathBuf, root: Option<PathBuf>, output: PathBuf) -> Result<u8, CliError> {
    let root = source_root(&file, root);
    let limits = CompileLimits::default();
    let analysis = frontend::analyze_file(&file, &root, limits);
    for diagnostic in &analysis.diagnostics {
        io::stderr()
            .lock()
            .write_all(
                diagnostic
                    .render(&analysis.sources)
                    .map_err(failure)?
                    .as_bytes(),
            )
            .map_err(failure)?;
    }
    let program = match analysis.result {
        Ok(p) => p,
        Err(AnalysisError::Diagnostics) => return Ok(1),
        Err(e) => return Err(failure(e)),
    };
    let bytes = match lang::artifact::compile(
        program,
        &analysis.sources,
        limits,
        lang::artifact::Limits::default(),
    ) {
        Ok(bytes) => bytes,
        Err(lang::artifact::CompileError::Script(ScriptError::Entry(d))) => {
            io::stderr()
                .lock()
                .write_all(d.render(&analysis.sources).map_err(failure)?.as_bytes())
                .map_err(failure)?;
            return Ok(1);
        }
        Err(error) => return Err(failure(error)),
    };
    if let Ok(target) = std::fs::canonicalize(&output) {
        let canonical_root = std::fs::canonicalize(&root).map_err(failure)?;
        // These snapshots come only from analyze_file's root-checked loader,
        // which names every source relative to its canonical root. Do not use
        // display names from decoded artifacts or virtual inputs for file IO.
        if analysis
            .sources
            .iter()
            .any(|(_, source)| canonical_root.join(source.name()) == target)
        {
            return Err(failure(
                "artifact output must differ from every source input",
            ));
        }
    }
    write_artifact(&output, &bytes).map_err(|e| failure(format!("{output:?}: {e}")))?;
    Ok(0)
}

fn run_artifact(file: PathBuf, limits: ExecutionOptions) -> Result<u8, CliError> {
    let format = lang::artifact::Limits::default();
    let bytes =
        read_artifact(&file, format.bytes).map_err(|e| failure(format!("{file:?}: {e}")))?;
    let program = lang::artifact::decode(&bytes, format).map_err(failure)?;
    drop(bytes);
    match program {
        lang::artifact::Program::Copy(program) => run_copy(&program, limits),
        lang::artifact::Program::Component(_) => Err(failure(
            "component artifacts require explicit host bindings",
        )),
        lang::artifact::Program::Ownership(program) => {
            let entry = program
                .entry()
                .ok_or_else(|| failure("artifact has no script entry"))?;
            run_owned(program.flow(), OwnedEntry::Function(entry), limits)
        }
        lang::artifact::Program::Tasks(program) => {
            let entry = program
                .entry()
                .ok_or_else(|| failure("artifact has no script entry"))?;
            run_tasks(program.proof(), Some(entry), limits)
        }
    }
}

// The CLI's initial input buffer is bounded separately by the artifact byte
// cap. Decoder-retained copies and all verification buffers use its own ledger.
fn read_artifact(path: &std::path::Path, maximum: usize) -> io::Result<Vec<u8>> {
    let mut input = lang_semantics::file_input::open_regular(path)?;
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let allowance = maximum
            .saturating_sub(bytes.len())
            .saturating_add(1)
            .min(chunk.len());
        let count = match input.read(&mut chunk[..allowance]) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if count == 0 {
            return Ok(bytes);
        }
        let required = bytes
            .len()
            .checked_add(count)
            .ok_or_else(|| io::Error::other("artifact input size overflow"))?;
        if required > maximum {
            return Err(io::Error::other(format!(
                "artifact exceeds {maximum} bytes"
            )));
        }
        if required > bytes.capacity() {
            let capacity = bytes
                .capacity()
                .saturating_mul(2)
                .max(chunk.len())
                .max(required)
                .min(maximum);
            bytes
                .try_reserve_exact(capacity - bytes.len())
                .map_err(|_| io::Error::other("artifact input allocation failed"))?;
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}

fn write_artifact(path: &std::path::Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    for attempt in 0..16 {
        let temporary = parent.join(format!(".lang-{}-{attempt}.tmp", std::process::id()));
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        let result = (|| {
            file.write_all(bytes)?;
            file.flush()?;
            drop(file);
            std::fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        return result;
    }
    Err(io::Error::other(
        "cannot reserve an artifact output temporary file",
    ))
}
