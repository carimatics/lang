use super::*;
use crate::toolchain::installation::{self as manager, Identity, Installation, Operation};

enum Action {
    Identity,
    Init {
        root: PathBuf,
        trust: PathBuf,
    },
    Install {
        root: PathBuf,
        source: PathBuf,
        channel: Channel,
        version: Option<String>,
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
    let mut values = BTreeMap::new();
    let mut args = args.iter().skip(1);
    while let Some(flag) = args.next() {
        let flag = flag
            .to_str()
            .ok_or_else(|| usage("invalid toolchain option"))?;
        let allowed = flag == "--message-format"
            || match command {
                "init" => matches!(flag, "--root" | "--trust-root"),
                "install" => matches!(flag, "--root" | "--source" | "--channel" | "--version"),
                _ => false,
            };
        if !allowed || values.contains_key(flag) {
            return Err(usage("unsupported or repeated toolchain option"));
        }
        let value = args
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
        "install" => {
            let channel = match required("--channel")?.to_str() {
                Some("stable") => Channel::Stable,
                Some("beta") => Channel::Beta,
                Some("nightly") => Channel::Nightly,
                _ => return Err(usage("invalid toolchain channel")),
            };
            let version = values
                .get("--version")
                .map(|v| {
                    v.to_str()
                        .filter(|s| crate::package::manifest::valid_version(s))
                        .map(str::to_owned)
                        .ok_or_else(|| usage("invalid toolchain version"))
                })
                .transpose()?;
            let source = required("--source")?;
            if source.to_string_lossy().contains("://") {
                return Err(usage(
                    "this command requires a local distribution directory",
                ));
            }
            Action::Install {
                root: required("--root")?.into(),
                source: source.into(),
                channel,
                version,
            }
        }
        _ => return Err(usage("unsupported toolchain operation")),
    };
    Ok(Options { action, format })
}
fn manager_error(error: manager::Error) -> (Diagnostic, bool) {
    (failure(&error), error.floor_advanced)
}
fn encode(value: impl Serialize) -> Result<Vec<u8>, (Diagnostic, bool)> {
    let bytes = serde_json::to_vec(&value).map_err(|e| (failure(e), false))?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err((failure("toolchain result byte limit exceeded"), false));
    }
    Ok(bytes)
}
fn execute(action: Action) -> Result<Vec<u8>, (Diagnostic, bool)> {
    match action {
        Action::Identity => encode(Identity::current().map_err(manager_error)?),
        Action::Init { root, trust } => {
            let bytes = input(&trust, MAX_ROOT_BYTES).map_err(|e| (e, false))?;
            let installed = Installation::init(&root, &bytes).map_err(manager_error)?;
            encode(
                installed
                    .report(Operation::Init, false)
                    .map_err(manager_error)?,
            )
        }
        Action::Install {
            root,
            source,
            channel,
            version,
        } => {
            let mut installed = Installation::open(&root).map_err(manager_error)?;
            encode(
                installed
                    .install(manager::Request {
                        source: &source,
                        channel,
                        version: version.as_deref(),
                    })
                    .map_err(manager_error)?,
            )
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
) -> io::Result<u8> {
    let options = match parse(args) {
        Ok(value) => value,
        Err(error) => {
            cli::emit(stderr, cli::requested_format(args), &error)?;
            return Ok(2);
        }
    };
    match execute(options.action) {
        Ok(bytes) => {
            stdout.write_all(&bytes)?;
            writeln!(stdout)?;
            Ok(0)
        }
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
            Ok(1)
        }
    }
}
