use super::*;
use crate::package::{Diagnostic, command as cli, lock::explain::read_record};
use cap_std::fs::Dir;
use std::{
    ffi::OsString,
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const USAGE: &str = "       lang toolchain verify --trust-root FILE --metadata FILE --channel stable|beta|nightly [--message-format human|json]\n";
struct Options {
    root: PathBuf,
    metadata: PathBuf,
    channel: Channel,
    format: cli::Format,
}
fn usage(message: &str) -> Diagnostic {
    Diagnostic::error("TLC0002", message)
}
fn failure(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("TLC0001", error.to_string())
}
fn parse(args: &[OsString]) -> Result<Options, Diagnostic> {
    if args.first().is_none_or(|arg| arg != "verify") {
        return Err(usage("expected toolchain verify"));
    }
    let (mut root, mut metadata, mut channel) = (None, None, None);
    let mut format = cli::Format::Human;
    let mut seen = BTreeSet::new();
    let mut args = args.iter().skip(1);
    while let Some(flag) = args.next() {
        let flag = flag
            .to_str()
            .ok_or_else(|| usage("invalid toolchain option"))?;
        if !seen.insert(flag) {
            return Err(usage("repeated toolchain option"));
        }
        let value = args
            .next()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| usage("toolchain option requires a value"))?;
        match flag {
            "--trust-root" => root = Some(PathBuf::from(value)),
            "--metadata" => metadata = Some(PathBuf::from(value)),
            "--channel" => {
                channel = Some(match value.to_str() {
                    Some("stable") => Channel::Stable,
                    Some("beta") => Channel::Beta,
                    Some("nightly") => Channel::Nightly,
                    _ => return Err(usage("invalid toolchain channel")),
                })
            }
            "--message-format" => {
                format = match value.to_str() {
                    Some("human") => cli::Format::Human,
                    Some("json") => cli::Format::Json,
                    _ => return Err(usage("invalid message format")),
                }
            }
            _ => return Err(usage("unsupported toolchain option")),
        }
    }
    Ok(Options {
        root: root.ok_or_else(|| usage("missing trust root"))?,
        metadata: metadata.ok_or_else(|| usage("missing metadata"))?,
        channel: channel.ok_or_else(|| usage("missing channel"))?,
        format,
    })
}
fn input(path: &Path, limit: usize) -> Result<Vec<u8>, Diagnostic> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| failure("input requires a file path"))?;
    let dir = Dir::open_ambient_dir(parent, cap_std::ambient_authority())
        .map_err(|e| failure(e).at_path(path))?;
    read_record(&dir, parent, Path::new(name), limit).map_err(|e| failure(e.message).at_path(path))
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    ToolchainMetadata,
}
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum Scope {
    FreshTrustContext,
}
#[derive(Serialize)]
struct Summary<'a> {
    version: &'a str,
    compiler_build_id: &'a str,
    generation: String,
    files: usize,
    bytes: u64,
}
#[derive(Serialize)]
struct Report<'a> {
    schema: u32,
    kind: Kind,
    scope: Scope,
    root_sha256: &'a str,
    repository: &'a str,
    channel: Channel,
    target: Target,
    sequence: u64,
    payload_sha256: &'a str,
    current_version: &'a str,
    releases: Vec<Summary<'a>>,
}
fn report(options: &Options) -> Result<Vec<u8>, Diagnostic> {
    let target = match lang_codegen_clif::Triple::host().to_string().as_str() {
        "aarch64-apple-darwin" => Target::Macos,
        "x86_64-pc-windows-msvc" => Target::Windows,
        _ => return Err(failure("unsupported toolchain host target")),
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(failure)?
        .as_secs();
    let root = TrustedRoot::decode(&input(&options.root, MAX_ROOT_BYTES)?, now).map_err(failure)?;
    let metadata = input(&options.metadata, MAX_METADATA_BYTES)?;
    let admitted = verify(
        &root,
        &metadata,
        Request {
            channel: options.channel,
            target,
            now,
            previous: None,
        },
    )
    .map_err(failure)?;
    let payload = admitted.payload();
    let releases = payload
        .releases
        .iter()
        .map(|r| {
            Ok(Summary {
                version: &r.version,
                compiler_build_id: &r.compiler_build_id,
                generation: r.identity().map_err(failure)?,
                files: r.files.len(),
                bytes: r.files.iter().map(|f| f.bytes).sum(),
            })
        })
        .collect::<Result<Vec<_>, Diagnostic>>()?;
    let bytes = serde_json::to_vec(&Report {
        schema: 1,
        kind: Kind::ToolchainMetadata,
        scope: Scope::FreshTrustContext,
        root_sha256: root.identity(),
        repository: root.repository(),
        channel: payload.channel,
        target: payload.target,
        sequence: payload.sequence,
        payload_sha256: admitted.admission().payload_digest(),
        current_version: &payload.current_version,
        releases,
    })
    .map_err(failure)?;
    if bytes.len() > MAX_METADATA_BYTES {
        return Err(failure("verification result byte limit exceeded"));
    }
    Ok(bytes)
}
pub fn run(args: &[OsString], stdout: &mut impl Write, stderr: &mut impl Write) -> io::Result<u8> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        stdout.write_all(USAGE.as_bytes())?;
        return Ok(0);
    }
    let options = match parse(args) {
        Ok(o) => o,
        Err(e) => {
            cli::emit(stderr, cli::requested_format(args), &e)?;
            return Ok(2);
        }
    };
    match report(&options) {
        Ok(bytes) => {
            stdout.write_all(&bytes)?;
            writeln!(stdout)?;
            Ok(0)
        }
        Err(e) => {
            cli::emit(stderr, options.format, &e)?;
            Ok(1)
        }
    }
}
