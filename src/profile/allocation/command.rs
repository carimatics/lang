use super::*;
use crate::{
    artifact,
    package::{command, graph::Graph, lock::Lock},
};
use lang_codegen_clif::{
    owned::{CompiledProgram, runtime as native},
    sdk,
};
use lang_runtime::{cleanup, execution, layout};
use std::{collections::BTreeSet, ffi::OsString, path::Path};

struct Options {
    package: command::Request,
    engine: Engine,
    events: usize,
}
fn parse(args: &[OsString]) -> Result<Options, Diagnostic> {
    let mut filtered = vec![OsString::from("run")];
    let mut values = args.iter().skip(1);
    let mut seen = BTreeSet::new();
    let mut engine = Engine::Vm;
    let mut events = 4096;
    while let Some(flag) = values.next() {
        let flag = flag
            .to_str()
            .ok_or_else(|| super::super::usage("invalid allocation trace option"))?;
        if !seen.insert(flag) {
            return Err(super::super::usage("repeated allocation trace option"));
        }
        let value = values
            .next()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| super::super::usage("allocation trace option requires a value"))?;
        match flag {
            "--engine" => {
                engine = match value.to_str() {
                    Some("vm") => Engine::Vm,
                    Some("native") => Engine::NativeJit,
                    _ => return Err(super::super::usage("unsupported allocation trace engine")),
                }
            }
            "--events" => {
                events = value
                    .to_str()
                    .and_then(|s| s.parse::<usize>().ok())
                    .filter(|n| (1..=a::MAX_TRACE_EVENTS).contains(n))
                    .ok_or_else(|| super::super::usage("invalid allocation trace event limit"))?
            }
            "--manifest-path" | "--package" | "--target" | "--message-format" | "--fuel"
            | "--frames" | "--cells" | "--service-work" => {
                filtered.push(OsString::from(flag));
                filtered.push(value.clone());
            }
            _ => return Err(super::super::usage("unsupported allocation trace option")),
        }
    }
    let command::Command::Package(package) = command::parse(&filtered)? else {
        return Err(super::super::usage("invalid allocation trace request"));
    };
    Ok(Options {
        package,
        engine,
        events,
    })
}

fn observed(
    options: &Options,
    program: &artifact::Program,
    root: &Path,
    input_digest: String,
    mut stderr: &mut dyn Write,
) -> Result<Report, Diagnostic> {
    let artifact::Program::Ownership(program) = program else {
        return Err(error(
            "allocation tracing requires the ownership execution profile",
        ));
    };
    let flow = program.flow();
    let entry = program
        .entry()
        .ok_or_else(|| error("allocation trace requires an entry"))?;
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
    let mut instance = cleanup::Instance::default();
    let mut allocator = a::Allocator::new(a::System, Default::default()).map_err(error)?;
    let trace = allocator.start_trace(options.events).map_err(error)?;
    let control = options
        .package
        .execution
        .control()
        .constrained(execution::Limits {
            fuel: None,
            host_calls: Some(0),
            frames: None,
            live_cells: None,
        });
    let result = match options.engine {
        Engine::Vm => lang_vm::unit::owned::execute_function(
            flow,
            entry,
            &mut instance,
            Some(&mut allocator),
            control,
        )
        .map(|o| (o.value, o.usage))
        .map_err(|f| {
            (
                f.to_string(),
                f.usage,
                f.fault.map(|s| Fault {
                    source: s.source.0,
                    start: s.start,
                    end: s.end,
                }),
            )
        }),
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
            let compiled = CompiledProgram::compile(flow, &layouts, Default::default(), 1_000_000)
                .map_err(error)?;
            native::execute_entry(
                &compiled,
                entry,
                &mut instance,
                &mut allocator,
                native::Config {
                    control,
                    ..Default::default()
                },
            )
            .map(|o| (o.value, o.usage))
            .map_err(|f| {
                (
                    f.to_string(),
                    f.usage,
                    f.fault.map(|s| Fault {
                        source: s.source.0,
                        start: s.start,
                        end: s.end,
                    }),
                )
            })
        }
    };
    // Capture at original entry return, before dropping the allocator or taking
    // any action that could retire live guest owners. Observation is not cleanup.
    let actual = allocator.stats();
    let snapshot = trace.snapshot();
    let (outcome, usage) = match result {
        Ok((result, usage)) => (Outcome::Completed { result }, usage),
        Err((message, usage, fault)) => {
            let diagnostic = super::super::runtime::fault(
                &message,
                usage,
                fault.map(|f| execution::Point {
                    source: f.source,
                    start: f.start,
                    end: f.end,
                }),
                raw,
            );
            command::emit(&mut stderr, options.package.format, &diagnostic)
                .map_err(|_| error("cannot write original execution diagnostic"))?;
            if let Some(fault) = fault {
                let source = raw
                    .get(fault.source as usize)
                    .ok_or_else(|| error("invalid original failure source"))?;
                if !source.text.is_char_boundary(fault.start as usize)
                    || !source.text.is_char_boundary(fault.end as usize)
                {
                    return Err(error("invalid original failure boundary"));
                }
            }
            (Outcome::Failed { message, fault }, usage)
        }
    };
    let snapshot = snapshot.map_err(error)?;
    if snapshot.allocator != allocator.identity() || snapshot.stats != actual {
        return Err(error(
            "allocation trace does not match original entry-return counters",
        ));
    }
    let report = Report {
        schema: 1,
        kind: Kind::AllocationTrace,
        compiler_id: sdk::build_id(&Triple::host()),
        target: options.package.target.clone(),
        input_digest,
        engine: options.engine,
        profile: Profile::Ownership,
        measurement: Measurement::LogicalRequestedStorage,
        boundary: Boundary::EntryReturn,
        allocator: snapshot.allocator.get(),
        sources: super::super::runtime::source_list(raw, root),
        usage: usage.into(),
        outcome,
        events: snapshot.events.into_iter().map(Event::from).collect(),
        stats: actual.into(),
    };
    report.validate()?;
    Ok(report)
}

pub fn run(args: &[OsString], stdout: &mut impl Write, stderr: &mut impl Write) -> io::Result<u8> {
    if args.iter().any(|s| s == "--help" || s == "-h") {
        stdout.write_all(super::super::USAGE.as_bytes())?;
        return Ok(0);
    }
    let options = match parse(args) {
        Ok(r) => r,
        Err(d) => {
            command::emit(stderr, command::requested_format(args), &d)?;
            return Ok(2);
        }
    };
    let action = |stderr: &mut dyn Write| -> Result<Option<(Vec<u8>, u8)>, Diagnostic> {
        if options.package.target != Triple::host().to_string() {
            return Err(error("allocation traces require the host target"));
        }
        let graph = Graph::resolve_selected(
            &options.package.manifest,
            &options.package.target,
            Default::default(),
            options.package.package.as_deref(),
        )?;
        let lock = Lock::capture(&graph)?;
        let limits = crate::frontend::CompileLimits::default();
        let analysis = crate::package::source::analyze(&graph, limits)?;
        let mut stderr = stderr;
        let Some(bytes) =
            command::compile_analysis(analysis, options.package.format, limits, &mut stderr)
                .map_err(|_| error("cannot write source diagnostics"))?
        else {
            return Ok(None);
        };
        let program = artifact::decode(&bytes, Default::default()).map_err(error)?;
        let report = observed(
            &options,
            &program,
            graph.authority_root(),
            sdk::digest(lock.as_bytes()),
            stderr,
        )?;
        let exit = u8::from(matches!(report.outcome, Outcome::Failed { .. }));
        Ok(Some((report.encode()?, exit)))
    };
    match action(stderr) {
        Ok(Some((bytes, exit))) => {
            stdout.write_all(&bytes)?;
            writeln!(stdout)?;
            Ok(exit)
        }
        Ok(None) => Ok(1),
        Err(d) => {
            command::emit(stderr, options.package.format, &d)?;
            Ok(1)
        }
    }
}
