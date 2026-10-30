use super::*;
use crate::toolchain::installation::{self as manager, Identity, Installation, Operation, Source};

enum Action {
    Identity,
    Init {
        root: PathBuf,
        trust: PathBuf,
    },
    Transfer {
        root: PathBuf,
        source: OsString,
        channel: Channel,
        version: Option<String>,
        update: bool,
    },
    State {
        root: PathBuf,
        operation: Operation,
        selection: Option<(Channel, String)>,
    },
    Which {
        root: PathBuf,
    },
    Run {
        root: PathBuf,
        args: Vec<OsString>,
    },
}
struct Options {
    action: Action,
    format: cli::Format,
}
fn parse(args: &[OsString]) -> Result<Options, Diagnostic> {
    let command = args
        .first()
        .and_then(|a| a.to_str())
        .ok_or_else(|| usage("missing toolchain operation"))?;
    let delimiter = args.iter().position(|a| a == "--");
    if command != "run" && delimiter.is_some() {
        return Err(usage("unexpected child argument delimiter"));
    }
    if command == "run" && delimiter.is_none() {
        return Err(usage("toolchain run requires -- before child arguments"));
    }
    let options_end = delimiter.unwrap_or(args.len());
    let mut values = BTreeMap::new();
    let mut options = args[1..options_end].iter();
    while let Some(flag) = options.next() {
        let flag = flag
            .to_str()
            .ok_or_else(|| usage("invalid toolchain option"))?;
        let allowed = flag == "--message-format"
            || match command {
                "init" => matches!(flag, "--root" | "--trust-root"),
                "install" => matches!(flag, "--root" | "--source" | "--channel" | "--version"),
                "update" => matches!(flag, "--root" | "--source" | "--channel"),
                "select" | "uninstall" => matches!(flag, "--root" | "--channel" | "--version"),
                "list" | "which" | "run" | "recover" => flag == "--root",
                _ => false,
            };
        if !allowed || values.contains_key(flag) {
            return Err(usage("unsupported or repeated toolchain option"));
        }
        let value = options
            .next()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| usage("toolchain option requires a value"))?;
        values.insert(flag, value);
    }
    let required = |name| {
        values
            .get(name)
            .copied()
            .ok_or_else(|| usage(&format!("missing {name}")))
    };
    let channel = || match required("--channel")?.to_str() {
        Some("stable") => Ok(Channel::Stable),
        Some("beta") => Ok(Channel::Beta),
        Some("nightly") => Ok(Channel::Nightly),
        _ => Err(usage("invalid toolchain channel")),
    };
    let version = || {
        values
            .get("--version")
            .map(|v| {
                v.to_str()
                    .filter(|s| crate::package::manifest::valid_version(s))
                    .map(str::to_owned)
                    .ok_or_else(|| usage("invalid toolchain version"))
            })
            .transpose()
    };
    let format = match values.get("--message-format").map(|v| v.to_str()) {
        None | Some(Some("human")) => cli::Format::Human,
        Some(Some("json")) => cli::Format::Json,
        _ => return Err(usage("invalid message format")),
    };
    let action = match command {
        "identity" => Action::Identity,
        "init" => Action::Init {
            root: required("--root")?.into(),
            trust: required("--trust-root")?.into(),
        },
        "install" | "update" => Action::Transfer {
            root: required("--root")?.into(),
            source: required("--source")?.clone(),
            channel: channel()?,
            version: version()?,
            update: command == "update",
        },
        "list" | "recover" => Action::State {
            root: required("--root")?.into(),
            operation: if command == "list" {
                Operation::List
            } else {
                Operation::Recover
            },
            selection: None,
        },
        "select" | "uninstall" => Action::State {
            root: required("--root")?.into(),
            operation: if command == "select" {
                Operation::Select
            } else {
                Operation::Uninstall
            },
            selection: Some((
                channel()?,
                version()?.ok_or_else(|| usage("missing --version"))?,
            )),
        },
        "which" => Action::Which {
            root: required("--root")?.into(),
        },
        "run" => Action::Run {
            root: required("--root")?.into(),
            args: args[options_end + 1..].to_vec(),
        },
        _ => return Err(usage("unsupported toolchain operation")),
    };
    Ok(Options { action, format })
}
type FailureResult = (Diagnostic, bool);
fn manager_error(error: manager::Error) -> FailureResult {
    (failure(&error), error.floor_advanced)
}
enum Output {
    Record(Vec<u8>),
    Child(std::process::ExitStatus),
}
fn encode(value: impl Serialize) -> Result<Output, FailureResult> {
    let bytes = serde_json::to_vec(&value).map_err(|e| (failure(e), false))?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err((failure("toolchain result byte limit exceeded"), false));
    }
    Ok(Output::Record(bytes))
}
fn execute(action: Action) -> Result<Output, FailureResult> {
    let open = |root: &Path| Installation::open(root).map_err(manager_error);
    match action {
        Action::Identity => encode(Identity::current().map_err(manager_error)?),
        Action::Init { root, trust } => {
            let bytes = input(&trust, MAX_ROOT_BYTES).map_err(|e| (e, false))?;
            encode(
                Installation::init(&root, &bytes)
                    .map_err(manager_error)?
                    .report(Operation::Init, false)
                    .map_err(manager_error)?,
            )
        }
        Action::Transfer {
            root,
            source,
            channel,
            version,
            update,
        } => {
            let mut installed = open(&root)?;
            let source = match source.to_str().filter(|s| s.contains("://")) {
                Some(url) => Source::https(url),
                None => Source::local(Path::new(&source)),
            }
            .map_err(manager_error)?;
            let result = if update {
                installed.update(&source, channel)
            } else {
                installed.install_source(&source, channel, version.as_deref())
            };
            encode(result.map_err(manager_error)?)
        }
        Action::State {
            root,
            operation,
            selection,
        } => {
            let mut installed = open(&root)?;
            let result = match (operation, selection) {
                (Operation::List, _) => installed.list(),
                (Operation::Recover, _) => installed.recover(),
                (Operation::Select, Some((channel, version))) => {
                    installed.select(channel, &version)
                }
                (Operation::Uninstall, Some((channel, version))) => {
                    installed.uninstall(channel, &version)
                }
                _ => unreachable!("parsed state operation"),
            };
            encode(result.map_err(manager_error)?)
        }
        Action::Which { root } => encode(open(&root)?.which().map_err(manager_error)?),
        Action::Run { root, args } => {
            let mut child = open(&root)?.spawn(&args).map_err(manager_error)?;
            child
                .wait()
                .map(Output::Child)
                .map_err(|e| (failure(e), false))
        }
    }
}
#[derive(Serialize)]
struct Failure<'a> {
    schema: u32,
    kind: &'static str,
    diagnostic: &'a Diagnostic,
    floor_advanced: bool,
}
pub(super) fn run(
    args: &[OsString],
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<Outcome> {
    let options = match parse(args) {
        Ok(value) => value,
        Err(error) => {
            cli::emit(stderr, cli::requested_format(args), &error)?;
            return Ok(Outcome::Code(2));
        }
    };
    match execute(options.action) {
        Ok(Output::Record(bytes)) => {
            stdout.write_all(&bytes)?;
            writeln!(stdout)?;
            Ok(Outcome::Code(0))
        }
        Ok(Output::Child(status)) => Ok(Outcome::Child(status)),
        Err((error, advanced)) => {
            match options.format {
                cli::Format::Json => {
                    serde_json::to_writer(
                        &mut *stderr,
                        &Failure {
                            schema: 1,
                            kind: "toolchain_failure",
                            diagnostic: &error,
                            floor_advanced: advanced,
                        },
                    )?;
                    writeln!(stderr)?;
                }
                cli::Format::Human => {
                    cli::emit(stderr, options.format, &error)?;
                    writeln!(stderr, "authenticated floor advanced: {advanced}")?;
                }
            }
            Ok(Outcome::Code(1))
        }
    }
}
