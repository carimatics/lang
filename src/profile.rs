//! Bounded observations of the ordinary package compiler (RFC 0041).
use crate::package::{
    Diagnostic,
    command::{self, Format},
    graph::Graph,
    lock::Lock,
};
use lang_codegen_clif::{Triple, sdk};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::{self, Write},
    path::PathBuf,
    time::Instant,
};

pub const USAGE: &str = "       lang profile compile [--manifest-path FILE] [--package NAME] [--target TRIPLE] [--message-format human|json]\n";
pub const MAX_REPORT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Compile,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Clock {
    HostMonotonicElapsedNs,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseName {
    PackageCapture,
    Frontend,
    LowerVerifyEncode,
}
const PHASES: [PhaseName; 3] = [
    PhaseName::PackageCapture,
    PhaseName::Frontend,
    PhaseName::LowerVerifyEncode,
];
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Phase {
    pub name: PhaseName,
    pub elapsed_ns: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema: u32,
    pub kind: Kind,
    pub clock: Clock,
    pub compiler_id: String,
    pub target: String,
    pub input_digest: String,
    pub source_files: u32,
    pub source_bytes: u64,
    pub artifact_bytes: u64,
    pub elapsed_ns: u64,
    pub phases: [Phase; 3],
}
fn error(message: &str) -> Diagnostic {
    Diagnostic::error("PRF0001", message)
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Report {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self.schema != 1 {
            return Err(error("unsupported compile profile schema"));
        }
        if !digest(&self.compiler_id) || !digest(&self.input_digest) {
            return Err(error("invalid compile profile identity"));
        }
        if self.target.len() > 256
            || self
                .target
                .parse::<Triple>()
                .ok()
                .is_none_or(|t| t.to_string() != self.target)
        {
            return Err(error("invalid compile profile target"));
        }
        if self.source_files == 0
            || self.source_files > 4096
            || self.source_bytes > 32 * 1024 * 1024
            || self.artifact_bytes > 64 * 1024 * 1024
        {
            return Err(error("compile profile resource limit exceeded"));
        }
        let mut total = 0_u64;
        for (phase, expected) in self.phases.iter().zip(PHASES) {
            if phase.name != expected {
                return Err(error("invalid compile profile phase order"));
            }
            total = total
                .checked_add(phase.elapsed_ns)
                .ok_or_else(|| error("compile profile duration overflow"))?;
        }
        if total != self.elapsed_ns {
            return Err(error("inconsistent compile profile duration"));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, Diagnostic> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| error("cannot encode compile profile"))?;
        if bytes.len() > MAX_REPORT_BYTES {
            return Err(error("compile profile output limit exceeded"));
        }
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Diagnostic> {
        if bytes.len() > MAX_REPORT_BYTES {
            return Err(error("compile profile input limit exceeded"));
        }
        let report: Self =
            serde_json::from_slice(bytes).map_err(|_| error("invalid compile profile JSON"))?;
        report.validate()?;
        Ok(report)
    }
}
fn elapsed(start: Instant) -> Result<u64, Diagnostic> {
    let duration = Instant::now()
        .checked_duration_since(start)
        .ok_or_else(|| error("invalid monotonic clock observation"))?;
    u64::try_from(duration.as_nanos()).map_err(|_| error("compile profile duration overflow"))
}
struct Request {
    manifest: PathBuf,
    package: Option<String>,
    target: String,
    format: Format,
}
fn usage(message: &str) -> Diagnostic {
    Diagnostic::error("CLI0001", message)
}
fn parse(args: &[OsString]) -> Result<Request, Diagnostic> {
    if args.first().is_none_or(|arg| arg != "compile") {
        return Err(usage("expected profile compile"));
    }
    let mut request = Request {
        manifest: PathBuf::from("Lang.json"),
        package: None,
        target: Triple::host().to_string(),
        format: Format::Human,
    };
    let mut values = args.iter().skip(1);
    let mut seen = BTreeSet::new();
    while let Some(flag) = values.next() {
        let flag = flag
            .to_str()
            .ok_or_else(|| usage("invalid profile option"))?;
        if !matches!(
            flag,
            "--manifest-path" | "--package" | "--target" | "--message-format"
        ) || !seen.insert(flag)
        {
            return Err(usage("unknown or repeated profile option"));
        }
        let value = values
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| usage("profile option requires a value"))?;
        match flag {
            "--manifest-path" => request.manifest = PathBuf::from(value),
            "--package" => {
                request.package = Some(
                    value
                        .to_str()
                        .ok_or_else(|| usage("invalid package name"))?
                        .to_owned(),
                )
            }
            "--target" => {
                let value = value.to_str().ok_or_else(|| usage("invalid target"))?;
                request.target = value
                    .parse::<Triple>()
                    .map_err(|_| usage("invalid target"))?
                    .to_string();
            }
            "--message-format" => {
                request.format = match value.to_str() {
                    Some("human") => Format::Human,
                    Some("json") => Format::Json,
                    _ => return Err(usage("unsupported message format")),
                }
            }
            _ => unreachable!(),
        }
    }
    Ok(request)
}

pub fn run(args: &[OsString], stdout: &mut impl Write, stderr: &mut impl Write) -> io::Result<u8> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        stdout.write_all(USAGE.as_bytes())?;
        return Ok(0);
    }
    let request = match parse(args) {
        Ok(request) => request,
        Err(d) => {
            command::emit(stderr, command::requested_format(args), &d)?;
            return Ok(2);
        }
    };
    let execute = |stderr: &mut dyn Write| -> Result<Option<Vec<u8>>, Diagnostic> {
        let start = Instant::now();
        let graph = Graph::resolve_selected(
            &request.manifest,
            &request.target,
            Default::default(),
            request.package.as_deref(),
        )?;
        let lock = Lock::capture(&graph)?;
        let capture_ns = elapsed(start)?;
        let limits = crate::frontend::CompileLimits::default();
        let start = Instant::now();
        let analysis = crate::package::source::analyze(&graph, limits)?;
        let frontend_ns = elapsed(start)?;
        let source_files = u32::try_from(analysis.sources.iter().count())
            .map_err(|_| error("source count overflow"))?;
        let source_bytes = analysis
            .sources
            .iter()
            .try_fold(0_u64, |n, (_, file)| {
                n.checked_add(file.text().len() as u64)
            })
            .ok_or_else(|| error("source byte count overflow"))?;
        let start = Instant::now();
        let mut stderr = stderr;
        let artifact = command::compile_analysis(analysis, request.format, limits, &mut stderr)
            .map_err(|_| error("cannot write compiler diagnostics"))?;
        let Some(artifact) = artifact else {
            return Ok(None);
        };
        let lower_ns = elapsed(start)?;
        let elapsed_ns = capture_ns
            .checked_add(frontend_ns)
            .and_then(|n| n.checked_add(lower_ns))
            .ok_or_else(|| error("compile profile duration overflow"))?;
        let report = Report {
            schema: 1,
            kind: Kind::Compile,
            clock: Clock::HostMonotonicElapsedNs,
            compiler_id: sdk::build_id(&Triple::host()),
            target: request.target.clone(),
            input_digest: sdk::digest(lock.as_bytes()),
            source_files,
            source_bytes,
            artifact_bytes: artifact.len() as u64,
            elapsed_ns,
            phases: [
                Phase {
                    name: PHASES[0],
                    elapsed_ns: capture_ns,
                },
                Phase {
                    name: PHASES[1],
                    elapsed_ns: frontend_ns,
                },
                Phase {
                    name: PHASES[2],
                    elapsed_ns: lower_ns,
                },
            ],
        };
        report.encode().map(Some)
    };
    match execute(stderr) {
        Ok(Some(bytes)) => {
            stdout.write_all(&bytes)?;
            writeln!(stdout)?;
            Ok(0)
        }
        Ok(None) => Ok(1),
        Err(d) => {
            command::emit(stderr, request.format, &d)?;
            Ok(1)
        }
    }
}
