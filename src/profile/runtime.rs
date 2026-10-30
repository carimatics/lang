//! Actual VM/native charge-point profiles, never replayed attribution.
use super::{Clock, MAX_REPORT_BYTES};
use crate::{
    artifact,
    package::{Diagnostic, command, graph::Graph, lock::Lock},
};
use lang_codegen_clif::{
    Triple,
    owned::{CompiledProgram, runtime as native_entry},
    sdk,
};
use lang_runtime::{allocation as a, cleanup as c, execution as e, layout};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::{self, Write},
    path::Path,
    time::Instant,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    Vm,
    NativeJit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    Copy,
    Ownership,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Runtime,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Measurement {
    SuccessfulFuelCharges,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub id: u32,
    pub path: String,
    pub bytes: u64,
    pub digest: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub source: u32,
    pub start: u32,
    pub end: u32,
    pub samples: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
    pub fuel: u64,
    pub host_calls: u64,
    pub service_work: u64,
    pub peak_frames: u64,
    pub peak_live_cells: u64,
}
impl From<e::Usage> for Usage {
    fn from(u: e::Usage) -> Self {
        Self {
            fuel: u.fuel,
            host_calls: u.host_calls,
            service_work: u.service_work,
            peak_frames: u.peak_frames as u64,
            peak_live_cells: u.peak_live_cells as u64,
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub schema: u32,
    pub kind: Kind,
    pub clock: Clock,
    pub compiler_id: String,
    pub target: String,
    pub input_digest: String,
    pub engine: Engine,
    pub profile: Profile,
    pub measurement: Measurement,
    pub sample_every: u32,
    pub total_charged_points: u64,
    pub sampled_points: u64,
    pub unattributed_points: u64,
    pub sources: Vec<Source>,
    pub points: Vec<Point>,
    pub usage: Usage,
    pub result: i64,
    pub elapsed_ns: u64,
}
fn error(message: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::error("PRF0002", message.to_string())
}
pub(super) fn portable_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 8192
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}
impl Report {
    pub fn validate(&self) -> Result<(), Diagnostic> {
        if self.schema != 1
            || !super::digest(&self.compiler_id)
            || !super::digest(&self.input_digest)
            || self.target.len() > 256
            || self
                .target
                .parse::<Triple>()
                .ok()
                .is_none_or(|t| t.to_string() != self.target)
            || (self.engine == Engine::NativeJit && self.profile != Profile::Ownership)
        {
            return Err(error("unsupported runtime profile identity or mode"));
        }
        if self.sample_every == 0
            || self.sample_every > e::MAX_SAMPLES as u32
            || self.sources.len() > 4096
            || self.points.len() > e::MAX_SAMPLES
            || self.sampled_points > e::MAX_SAMPLES as u64
            || self.usage.host_calls != 0
            || self.total_charged_points != self.usage.fuel
            || self.sampled_points != self.total_charged_points / u64::from(self.sample_every)
        {
            return Err(error("invalid runtime profile counts"));
        }
        let mut sources = BTreeMap::new();
        let mut bytes = 0_u64;
        let mut previous = None;
        for source in &self.sources {
            if source.id >= 4096
                || previous.is_some_and(|id| id >= source.id)
                || !portable_path(&source.path)
                || !super::digest(&source.digest)
            {
                return Err(error("invalid runtime profile source"));
            }
            bytes = bytes
                .checked_add(source.bytes)
                .filter(|n| *n <= 32 * 1024 * 1024)
                .ok_or_else(|| error("runtime source limit exceeded"))?;
            previous = Some(source.id);
            sources.insert(source.id, source);
        }
        let mut total = self.unattributed_points;
        let mut previous = None;
        for point in &self.points {
            let key = (point.source, point.start, point.end);
            let source = sources
                .get(&point.source)
                .ok_or_else(|| error("unknown runtime profile source"))?;
            if point.samples == 0
                || point.start > point.end
                || u64::from(point.end) > source.bytes
                || previous.is_some_and(|p| p >= key)
            {
                return Err(error("invalid runtime profile point"));
            }
            previous = Some(key);
            total = total
                .checked_add(point.samples)
                .ok_or_else(|| error("runtime sample count overflow"))?;
        }
        if total != self.sampled_points {
            return Err(error("inconsistent runtime sample totals"));
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, Diagnostic> {
        self.validate()?;
        struct Buffer(Vec<u8>);
        impl Write for Buffer {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                if self.0.len().saturating_add(bytes.len()) > MAX_REPORT_BYTES {
                    return Err(io::Error::other("runtime report output limit"));
                }
                self.0
                    .try_reserve_exact(bytes.len())
                    .map_err(io::Error::other)?;
                self.0.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut buffer = Buffer(Vec::new());
        serde_json::to_writer(&mut buffer, self)
            .map_err(|_| error("cannot encode bounded runtime report"))?;
        Ok(buffer.0)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Diagnostic> {
        if bytes.len() > MAX_REPORT_BYTES {
            return Err(error("runtime report input limit exceeded"));
        }
        let report: Self =
            serde_json::from_slice(bytes).map_err(|_| error("invalid runtime profile JSON"))?;
        report.validate()?;
        Ok(report)
    }
}
struct Request {
    package: command::Request,
    engine: Engine,
    sample_every: u32,
    observations: usize,
}
fn parse(args: &[OsString]) -> Result<Request, Diagnostic> {
    let mut filtered = vec![OsString::from("run")];
    let mut values = args.iter().skip(1);
    let mut seen = BTreeSet::new();
    let mut engine = Engine::Vm;
    let mut sample_every = 1;
    let mut observations = 4096;
    while let Some(flag) = values.next() {
        let flag = flag
            .to_str()
            .ok_or_else(|| super::usage("invalid runtime profile option"))?;
        if !seen.insert(flag) {
            return Err(super::usage("repeated runtime profile option"));
        }
        let value = values
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| super::usage("runtime profile option requires a value"))?;
        match flag {
            "--engine" => {
                engine = match value.to_str() {
                    Some("vm") => Engine::Vm,
                    Some("native") => Engine::NativeJit,
                    _ => return Err(super::usage("unsupported runtime engine")),
                }
            }
            "--sample-every" => {
                sample_every = value
                    .to_str()
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|n| *n > 0 && *n <= e::MAX_SAMPLES as u32)
                    .ok_or_else(|| super::usage("invalid sampling interval"))?
            }
            "--observations" => {
                observations = value
                    .to_str()
                    .and_then(|s| s.parse::<usize>().ok())
                    .filter(|n| *n > 0 && *n <= e::MAX_SAMPLES)
                    .ok_or_else(|| super::usage("invalid observation limit"))?
            }
            "--manifest-path" | "--package" | "--target" | "--message-format" | "--fuel"
            | "--frames" | "--cells" | "--service-work" => {
                filtered.push(OsString::from(flag));
                filtered.push(value.clone());
            }
            _ => return Err(super::usage("unsupported runtime profile option")),
        }
    }
    let command::Command::Package(package) = command::parse(&filtered)? else {
        return Err(super::usage("invalid runtime profile request"));
    };
    Ok(Request {
        package,
        engine,
        sample_every,
        observations,
    })
}
pub(super) fn source_list(raw: &[lang_mir::unit::Source], root: &Path) -> Vec<Source> {
    raw.iter()
        .enumerate()
        .filter_map(|(id, source)| {
            let path = Path::new(&source.path);
            let relative = if path.is_absolute() {
                path.strip_prefix(root).ok()?
            } else {
                path
            };
            let path = relative.to_str()?.replace('\\', "/");
            portable_path(&path).then(|| Source {
                id: id as u32,
                path,
                bytes: source.text.len() as u64,
                digest: sdk::digest(source.text.as_bytes()),
            })
        })
        .collect()
}
pub(super) fn fault(
    message: impl std::fmt::Display,
    usage: e::Usage,
    point: Option<e::Point>,
    sources: &[lang_mir::unit::Source],
) -> Diagnostic {
    let mut diagnostic = Diagnostic::error(
        "CLI0002",
        format!(
            "{message} (fuel {}, service work {})",
            usage.fuel, usage.service_work
        ),
    );
    if let Some(point) = point
        && let Some(source) = sources.get(point.source as usize)
    {
        diagnostic.path = Some(source.path.clone());
        diagnostic.span = Some(crate::package::ByteSpan {
            start: point.start as usize,
            end: point.end as usize,
        });
    }
    diagnostic
}
fn observed(
    request: &Request,
    program: &artifact::Program,
    root: &Path,
    input_digest: String,
) -> Result<Report, Diagnostic> {
    let recorder =
        e::PointRecorder::new(request.observations, request.sample_every).map_err(error)?;
    let control = request
        .package
        .execution
        .control()
        .constrained(e::Limits {
            fuel: None,
            host_calls: Some(0),
            frames: None,
            live_cells: None,
        })
        .and_observations(&recorder);
    let (profile, raw, value, usage, elapsed_ns) = match program {
        artifact::Program::Copy(program) => {
            if request.engine != Engine::Vm {
                return Err(error(
                    "Copy native execution is unmetered; source-point native profiling requires ownership",
                ));
            }
            let raw = &program.verified().unit().sources;
            let start = Instant::now();
            let result = lang_vm::unit::execute_entry(
                program,
                &lang_runtime::HostFunctionRegistry::new(),
                &lang_vm::unit::HostBindings::new(),
                control,
            );
            let duration = super::elapsed(start)?;
            let outcome = result.map_err(|f| {
                fault(
                    format!("{:?}", f.reason),
                    f.usage,
                    f.fault.map(|s| e::Point {
                        source: s.source.0,
                        start: s.start,
                        end: s.end,
                    }),
                    raw,
                )
            })?;
            (
                Profile::Copy,
                raw.as_slice(),
                outcome
                    .value
                    .as_i64()
                    .ok_or_else(|| error("profile entry must return i64"))?,
                outcome.usage,
                duration,
            )
        }
        artifact::Program::Ownership(program) => {
            let flow = program.flow();
            let entry = program
                .entry()
                .ok_or_else(|| error("profile requires an entry"))?;
            let raw = &flow
                .source()
                .activity()
                .moved()
                .typed()
                .places()
                .declarations()
                .types()
                .table()
                .sources;
            let mut instance = c::Instance::default();
            let mut allocator = a::Allocator::new(a::System, Default::default()).map_err(error)?;
            let (value, usage, duration) = match request.engine {
                Engine::Vm => {
                    let start = Instant::now();
                    let result = lang_vm::unit::owned::execute_function(
                        flow,
                        entry,
                        &mut instance,
                        Some(&mut allocator),
                        control,
                    );
                    let duration = super::elapsed(start)?;
                    let outcome = result.map_err(|f| {
                        fault(
                            &f,
                            f.usage,
                            f.fault.map(|s| e::Point {
                                source: s.source.0,
                                start: s.start,
                                end: s.end,
                            }),
                            raw,
                        )
                    })?;
                    (outcome.value, outcome.usage, duration)
                }
                Engine::NativeJit => {
                    let layouts = layout::Layouts::compile(
                        flow.source()
                            .activity()
                            .moved()
                            .typed()
                            .places()
                            .declarations()
                            .types(),
                        Default::default(),
                    )
                    .map_err(error)?;
                    let native =
                        CompiledProgram::compile(flow, &layouts, Default::default(), 1_000_000)
                            .map_err(error)?;
                    let start = Instant::now();
                    let result = native_entry::execute_entry(
                        &native,
                        entry,
                        &mut instance,
                        &mut allocator,
                        native_entry::Config {
                            control,
                            ..Default::default()
                        },
                    );
                    let duration = super::elapsed(start)?;
                    let outcome = result.map_err(|f| {
                        fault(
                            &f,
                            f.usage,
                            f.fault.map(|s| e::Point {
                                source: s.source.0,
                                start: s.start,
                                end: s.end,
                            }),
                            raw,
                        )
                    })?;
                    (outcome.value, outcome.usage, duration)
                }
            };
            (Profile::Ownership, raw.as_slice(), value, usage, duration)
        }
        _ => {
            return Err(error(
                "runtime profiling requires the ordinary Copy or ownership profile",
            ));
        }
    };
    let snapshot = recorder.snapshot().map_err(error)?;
    let sources = source_list(raw, root);
    let admitted: BTreeSet<_> = sources.iter().map(|s| s.id).collect();
    let mut aggregates = BTreeMap::new();
    let mut unattributed_points = 0_u64;
    let sampled_points = snapshot.samples.len() as u64;
    for sample in snapshot.samples {
        let Some(point) = sample.filter(|p| admitted.contains(&p.source)) else {
            unattributed_points += 1;
            continue;
        };
        let source = &raw[point.source as usize];
        if point.start > point.end
            || !source.text.is_char_boundary(point.start as usize)
            || !source.text.is_char_boundary(point.end as usize)
        {
            return Err(error("invalid recorded source range"));
        }
        *aggregates
            .entry((point.source, point.start, point.end))
            .or_insert(0_u64) += 1;
    }
    let report = Report {
        schema: 1,
        kind: Kind::Runtime,
        clock: Clock::HostMonotonicElapsedNs,
        compiler_id: sdk::build_id(&Triple::host()),
        target: request.package.target.clone(),
        input_digest,
        engine: request.engine,
        profile,
        measurement: Measurement::SuccessfulFuelCharges,
        sample_every: snapshot.sample_every,
        total_charged_points: snapshot.total,
        sampled_points,
        unattributed_points,
        sources,
        points: aggregates
            .into_iter()
            .map(|((source, start, end), samples)| Point {
                source,
                start,
                end,
                samples,
            })
            .collect(),
        usage: usage.into(),
        result: value,
        elapsed_ns,
    };
    report.validate()?;
    Ok(report)
}
pub fn run(args: &[OsString], stdout: &mut impl Write, stderr: &mut impl Write) -> io::Result<u8> {
    if args.iter().any(|s| s == "--help" || s == "-h") {
        stdout.write_all(super::USAGE.as_bytes())?;
        return Ok(0);
    }
    let request = match parse(args) {
        Ok(r) => r,
        Err(d) => {
            command::emit(stderr, command::requested_format(args), &d)?;
            return Ok(2);
        }
    };
    let action = |stderr: &mut dyn Write| -> Result<Option<Vec<u8>>, Diagnostic> {
        if request.package.target != Triple::host().to_string() {
            return Err(error("runtime profiles require the host target"));
        }
        let graph = Graph::resolve_selected(
            &request.package.manifest,
            &request.package.target,
            Default::default(),
            request.package.package.as_deref(),
        )?;
        let lock = Lock::capture(&graph)?;
        let limits = crate::frontend::CompileLimits::default();
        let analysis = crate::package::source::analyze(&graph, limits)?;
        let mut stderr = stderr;
        let Some(bytes) =
            command::compile_analysis(analysis, request.package.format, limits, &mut stderr)
                .map_err(|_| error("cannot write source diagnostics"))?
        else {
            return Ok(None);
        };
        let program = artifact::decode(&bytes, Default::default()).map_err(error)?;
        observed(
            &request,
            &program,
            graph.authority_root(),
            sdk::digest(lock.as_bytes()),
        )?
        .encode()
        .map(Some)
    };
    match action(stderr) {
        Ok(Some(bytes)) => {
            stdout.write_all(&bytes)?;
            writeln!(stdout)?;
            Ok(0)
        }
        Ok(None) => Ok(1),
        Err(d) => {
            command::emit(stderr, request.package.format, &d)?;
            Ok(1)
        }
    }
}
