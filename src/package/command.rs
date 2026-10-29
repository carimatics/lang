//! Host package command requests; source authority comes from the manifest.
use super::{Diagnostic, manifest};
use std::io::{self, Write};
use std::{collections::BTreeSet, ffi::OsString, path::PathBuf};

pub const USAGE: &str = "usage: lang new [--name NAME] [--message-format human|json] DIR\n       lang build [--manifest-path FILE] [--package NAME] [--target TRIPLE] [--locked] [--offline] [--message-format human|json]\n       lang run [--manifest-path FILE] [--package NAME] [--target TRIPLE] [--locked] [--offline] [--message-format human|json] [--fuel N] [--service-work N] [--host-calls N] [--frames N] [--cells N]\n       lang test [--manifest-path FILE] [--package NAME] [--target TRIPLE] [--locked] [--offline] [--message-format human|json] [--fuel N] [--service-work N] [--host-calls N] [--frames N] [--cells N]\n";

#[derive(Clone, Copy, Default)]
pub struct ExecutionOptions {
    pub limits: lang_vm::unit::Limits,
    pub service_work: Option<u64>,
}
impl ExecutionOptions {
    pub fn control(self) -> lang_vm::unit::Control<'static> {
        lang_vm::unit::Control::with_limits(self.limits).and_service_work(self.service_work)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Human,
    Json,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    Build,
    Run,
    Test,
}
pub enum Command {
    Help,
    New {
        directory: PathBuf,
        name: Option<String>,
        format: Format,
    },
    Package(Request),
}
pub struct Request {
    pub action: Action,
    pub manifest: PathBuf,
    pub package: Option<String>,
    pub target: String,
    pub locked: bool,
    pub offline: bool,
    pub format: Format,
    pub execution: ExecutionOptions,
}

pub fn requested_format(args: &[OsString]) -> Format {
    if args
        .windows(2)
        .any(|a| a[0] == "--message-format" && a[1] == "json")
    {
        Format::Json
    } else {
        Format::Human
    }
}

/// Preserve every existing explicit-file run form; only new package forms
/// enter this parser. Package-only flags force package validation so mixed
/// source/package usage can still report a JSON usage diagnostic.
pub fn selected(args: &[OsString]) -> bool {
    let Some(first) = args.first() else {
        return false;
    };
    if first == "new" || first == "build" || first == "test" {
        return true;
    }
    if first != "run" {
        return false;
    }
    let (mut package, mut positional, mut artifact, mut help) = (false, false, false, false);
    let mut values = args.iter().skip(1);
    while let Some(arg) = values.next() {
        if arg == "--artifact" {
            artifact = true;
        } else if arg == "--help" || arg == "-h" {
            help = true;
        } else if matches!(arg.to_str(), Some("--locked" | "--offline")) {
            package = true;
        } else if matches!(
            arg.to_str(),
            Some("--manifest-path" | "--package" | "--target" | "--message-format")
        ) {
            package = true;
            values.next();
        } else if matches!(
            arg.to_str(),
            Some(
                "--source-root"
                    | "--fuel"
                    | "--service-work"
                    | "--host-calls"
                    | "--frames"
                    | "--cells"
            )
        ) {
            values.next();
        } else if !arg.to_str().is_some_and(|a| a.starts_with("--")) {
            positional = true;
        }
    }
    package || (!positional && !artifact && !help)
}
fn usage(message: impl Into<String>) -> Diagnostic {
    Diagnostic::error("CLI0001", message)
}
pub fn parse(args: &[OsString]) -> Result<Command, Diagnostic> {
    let Some(first) = args.first() else {
        return Err(usage("missing package command"));
    };
    let new = first == "new";
    let action = if first == "build" {
        Action::Build
    } else if first == "run" {
        Action::Run
    } else if first == "test" {
        Action::Test
    } else if new {
        Action::Build
    } else {
        return Err(usage("expected new, build, run or test"));
    };
    let (mut directory, mut name) = (None, None);
    let mut request = Request {
        action,
        manifest: PathBuf::from("Lang.json"),
        package: None,
        target: lang_codegen_clif::Triple::host().to_string(),
        locked: false,
        offline: false,
        format: Format::Human,
        execution: ExecutionOptions::default(),
    };
    let mut seen = BTreeSet::new();
    let mut values = args.iter().skip(1);
    while let Some(arg) = values.next() {
        if arg == "--help" || arg == "-h" {
            return Ok(Command::Help);
        }
        let Some(flag) = arg.to_str().filter(|s| s.starts_with('-')) else {
            if arg.is_empty() || !new || directory.replace(PathBuf::from(arg)).is_some() {
                return Err(usage(
                    "package commands accept no source positional argument; new accepts one nonempty directory",
                ));
            }
            continue;
        };
        if !seen.insert(flag.to_string()) {
            return Err(usage(format!("duplicate option {flag}")));
        }
        if !new && matches!(flag, "--locked" | "--offline") {
            if flag == "--locked" {
                request.locked = true;
            } else {
                request.offline = true;
            }
            continue;
        }
        let permitted = flag == "--message-format"
            || (new && flag == "--name")
            || (!new && matches!(flag, "--manifest-path" | "--package" | "--target"))
            || (!new
                && matches!(action, Action::Run | Action::Test)
                && matches!(
                    flag,
                    "--fuel" | "--service-work" | "--host-calls" | "--frames" | "--cells"
                ));
        if !permitted {
            return Err(usage(format!("unknown package option {flag}")));
        }
        let value = values
            .next()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| usage(format!("missing value for {flag}")))?;
        let number = || {
            value
                .to_str()
                .ok_or_else(|| usage(format!("invalid {flag}")))
        };
        match flag {
            "--name" => {
                let v = number()?;
                if !manifest::valid_name(v) {
                    return Err(usage("invalid package name"));
                }
                name = Some(v.to_string());
            }
            "--message-format" => {
                request.format = match value.to_str() {
                    Some("human") => Format::Human,
                    Some("json") => Format::Json,
                    _ => return Err(usage("--message-format requires human or json")),
                }
            }
            "--manifest-path" => request.manifest = PathBuf::from(value),
            "--package" => {
                let name = number()?;
                if !manifest::valid_name(name) {
                    return Err(usage("invalid package name"));
                }
                request.package = Some(name.to_string());
            }
            "--target" => request.target = number()?.to_string(),
            "--fuel" => {
                request.execution.limits.fuel =
                    Some(number()?.parse().map_err(|_| usage("invalid --fuel"))?)
            }
            "--service-work" => {
                request.execution.service_work = Some(
                    number()?
                        .parse()
                        .map_err(|_| usage("invalid --service-work"))?,
                )
            }
            "--host-calls" => {
                request.execution.limits.host_calls = Some(
                    number()?
                        .parse()
                        .map_err(|_| usage("invalid --host-calls"))?,
                )
            }
            "--frames" => {
                request.execution.limits.frames =
                    Some(number()?.parse().map_err(|_| usage("invalid --frames"))?)
            }
            "--cells" => {
                request.execution.limits.live_cells =
                    Some(number()?.parse().map_err(|_| usage("invalid --cells"))?)
            }
            _ => unreachable!("validated package option"),
        }
    }
    if new {
        return Ok(Command::New {
            directory: directory.ok_or_else(|| usage("missing new package directory"))?,
            name,
            format: request.format,
        });
    }
    if !manifest::TARGETS.contains(&request.target.as_str()) {
        return Err(Diagnostic::error("PKG0003", "unsupported package target"));
    }
    Ok(Command::Package(request))
}

fn escaped(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_control() {
            out.extend(c.escape_default());
        } else {
            out.push(c);
        }
    }
    out
}
fn emit(stderr: &mut impl Write, format: Format, diagnostic: &Diagnostic) -> io::Result<()> {
    if format == Format::Json {
        serde_json::to_writer(&mut *stderr, diagnostic)?;
        writeln!(stderr)
    } else {
        writeln!(
            stderr,
            "{}[{}]: {}{}",
            match diagnostic.severity {
                super::Severity::Error => "error",
                super::Severity::Warning => "warning",
                super::Severity::Note => "note",
            },
            diagnostic.code,
            escaped(&diagnostic.message),
            diagnostic
                .path
                .as_ref()
                .map(|p| format!(" ({})", escaped(p)))
                .unwrap_or_default()
        )
    }
}
fn source_diagnostic(
    stderr: &mut impl Write,
    format: Format,
    diagnostic: &lang_diagnostics::Diagnostic,
    sources: &lang_diagnostics::SourceMap,
) -> io::Result<()> {
    if format == Format::Human {
        return stderr.write_all(
            diagnostic
                .render(sources)
                .map_err(io::Error::other)?
                .as_bytes(),
        );
    }
    let range = diagnostic.primary.span.range();
    let mut converted = Diagnostic::error(diagnostic.code.as_str(), diagnostic.message.clone());
    converted.severity = match diagnostic.severity {
        lang_diagnostics::Severity::Error => super::Severity::Error,
        lang_diagnostics::Severity::Warning => super::Severity::Warning,
        lang_diagnostics::Severity::Note => super::Severity::Note,
    };
    converted.path = Some(
        sources
            .file(diagnostic.primary.span.source())
            .map_err(io::Error::other)?
            .name()
            .to_string(),
    );
    converted.span = Some(super::ByteSpan {
        start: range.start,
        end: range.end,
    });
    emit(stderr, format, &converted)
}
fn fail(stderr: &mut impl Write, format: Format, diagnostic: Diagnostic) -> io::Result<u8> {
    emit(stderr, format, &diagnostic)?;
    Ok(1)
}
fn tool(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("CLI0002", error.to_string())
}

fn checked(
    analysis: crate::frontend::Analysis,
    format: Format,
    stderr: &mut impl Write,
) -> io::Result<
    Option<(
        lang_semantics::analysis::Program,
        lang_diagnostics::SourceMap,
    )>,
> {
    for diagnostic in &analysis.diagnostics {
        source_diagnostic(stderr, format, diagnostic, &analysis.sources)?;
    }
    match analysis.result {
        Ok(program) => Ok(Some((program, analysis.sources))),
        Err(crate::frontend::AnalysisError::Diagnostics) => Ok(None),
        Err(error) => {
            emit(stderr, format, &tool(error))?;
            Ok(None)
        }
    }
}

fn compile_analysis(
    analysis: crate::frontend::Analysis,
    format: Format,
    limits: crate::frontend::CompileLimits,
    stderr: &mut impl Write,
) -> io::Result<Option<Vec<u8>>> {
    let Some((program, sources)) = checked(analysis, format, stderr)? else {
        return Ok(None);
    };
    match crate::artifact::compile(
        program,
        &sources,
        limits,
        crate::artifact::Limits::default(),
    ) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(crate::artifact::CompileError::Script(crate::frontend::ScriptError::Entry(d))) => {
            source_diagnostic(stderr, format, &d, &sources)?;
            Ok(None)
        }
        Err(error) => {
            emit(stderr, format, &tool(error))?;
            Ok(None)
        }
    }
}

fn evaluate_artifact(
    bytes: &[u8],
    execution: ExecutionOptions,
    evaluate: &mut impl FnMut(&crate::artifact::Program, ExecutionOptions) -> Result<i64, Diagnostic>,
) -> Result<i64, Diagnostic> {
    let program =
        crate::artifact::decode(bytes, crate::artifact::Limits::default()).map_err(tool)?;
    evaluate(&program, execution)
}

fn run_tests(
    request: &Request,
    prepared: &super::output::Prepared,
    stderr: &mut impl Write,
    evaluate: &mut impl FnMut(&crate::artifact::Program, ExecutionOptions) -> Result<i64, Diagnostic>,
) -> io::Result<u8> {
    let limits = crate::frontend::CompileLimits::default();
    let root = prepared
        .graph
        .package(prepared.graph.root())
        .expect("resolved root");
    let tests = &root.manifest().tests;
    let mut files = Vec::new();
    let mut bytes = prepared.lock.as_bytes().len();
    if tests.is_empty() {
        let analysis = match super::source::analyze(&prepared.graph, limits) {
            Ok(analysis) => analysis,
            Err(d) => return fail(stderr, request.format, d),
        };
        if checked(analysis, request.format, stderr)?.is_none() {
            return Ok(1);
        }
    }
    for (index, test) in tests.iter().enumerate() {
        let analysis = match super::source::analyze_test(&prepared.graph, index, limits) {
            Ok(analysis) => analysis,
            Err(d) => return fail(stderr, request.format, d),
        };
        let Some(artifact) = compile_analysis(analysis, request.format, limits, stderr)? else {
            return Ok(1);
        };
        bytes = match bytes
            .checked_add(artifact.len())
            .filter(|n| *n <= super::output::MAX_OUTPUT)
        {
            Some(bytes) => bytes,
            None => {
                return fail(
                    stderr,
                    request.format,
                    Diagnostic::error("PKG0004", "package output byte limit exceeded"),
                );
            }
        };
        match evaluate_artifact(&artifact, request.execution, evaluate) {
            Ok(0) => {}
            Ok(value) => {
                return fail(
                    stderr,
                    request.format,
                    Diagnostic::error(
                        "CLI0002",
                        format!("test {test} returned {value}; expected zero"),
                    )
                    .at_path(&super::graph::input_path(
                        root.root(),
                        std::path::Path::new(test),
                    )),
                );
            }
            Err(d) => return fail(stderr, request.format, d),
        }
        files.push((format!("tests/{index}.lbc"), artifact));
    }
    let files: Vec<_> = files
        .iter()
        .map(|(path, bytes)| super::output::File { path, bytes })
        .collect();
    match prepared.output.publish(
        &prepared.lock,
        super::output::Generation::Test,
        &files,
        request.locked,
    ) {
        Ok(()) => Ok(0),
        Err(d) => fail(stderr, request.format, d),
    }
}

/// Compile immutable package inputs and call the same VM engines used by the
/// explicit-file host. The callback receives a verified decoded artifact, never
/// filesystem authority. Publication follows successful evaluation.
pub fn run(
    args: &[OsString],
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    mut evaluate: impl FnMut(&crate::artifact::Program, ExecutionOptions) -> Result<i64, Diagnostic>,
) -> io::Result<u8> {
    let command = match parse(args) {
        Ok(command) => command,
        Err(diagnostic) => {
            let format = requested_format(args);
            emit(stderr, format, &diagnostic)?;
            if format == Format::Human && diagnostic.code == "CLI0001" {
                stderr.write_all(USAGE.as_bytes())?;
            }
            return Ok(if diagnostic.code == "CLI0001" { 2 } else { 1 });
        }
    };
    let request = match command {
        Command::Help => {
            stdout.write_all(USAGE.as_bytes())?;
            return Ok(0);
        }
        Command::New {
            directory,
            name,
            format,
        } => {
            return match super::scaffold::create(&directory, name.as_deref()) {
                Ok(()) => Ok(0),
                Err(d) => fail(stderr, format, d),
            };
        }
        Command::Package(request) => request,
    };
    let format = request.format;
    let prepared = match super::output::Prepared::open(&request) {
        Ok(p) => p,
        Err(d) => return fail(stderr, format, d),
    };
    if request.action == Action::Test {
        return run_tests(&request, &prepared, stderr, &mut evaluate);
    }
    let limits = crate::frontend::CompileLimits::default();
    let analysis = match super::source::analyze(&prepared.graph, limits) {
        Ok(a) => a,
        Err(d) => return fail(stderr, format, d),
    };
    let Some(bytes) = compile_analysis(analysis, format, limits, stderr)? else {
        return Ok(1);
    };
    let value = if request.action == Action::Run {
        match evaluate_artifact(&bytes, request.execution, &mut evaluate) {
            Ok(v) => Some(v),
            Err(d) => return fail(stderr, format, d),
        }
    } else {
        None
    };
    if let Err(d) = prepared.output.publish(
        &prepared.lock,
        super::output::Generation::Build,
        &[super::output::File {
            path: "main.lbc",
            bytes: &bytes,
        }],
        request.locked,
    ) {
        return fail(stderr, format, d);
    }
    if let Some(value) = value {
        writeln!(stdout, "{value}")?;
    }
    Ok(0)
}
