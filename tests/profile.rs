use lang::{
    artifact, frontend,
    package::{graph::Graph, lock::Lock, source},
    profile::Report,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lang"))
}
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/packages/local-dependency/Lang.json")
}
fn compile(manifest: &Path) -> Report {
    let result = bin()
        .args(["profile", "compile", "--manifest-path"])
        .arg(manifest)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stderr.is_empty());
    Report::decode(&result.stdout).unwrap()
}

#[test]
fn real_package_profile_observes_the_ordinary_dependency_artifact_without_publication() {
    let manifest = fixture();
    let before = std::fs::read(&manifest).unwrap();
    let report = compile(&manifest);
    let graph = Graph::resolve(&manifest, &report.target, Default::default()).unwrap();
    let lock = Lock::capture(&graph).unwrap();
    assert_eq!(
        report.input_digest,
        lang_codegen_clif::sdk::digest(lock.as_bytes())
    );
    let limits = frontend::CompileLimits::default();
    let analysis = source::analyze(&graph, limits).unwrap();
    let expected_files = analysis.sources.iter().count();
    let expected_bytes: usize = analysis
        .sources
        .iter()
        .map(|(_, file)| file.text().len())
        .sum();
    let bytes = artifact::compile(
        analysis.result.unwrap(),
        &analysis.sources,
        limits,
        Default::default(),
    )
    .unwrap();
    assert_eq!(report.source_files as usize, expected_files);
    assert_eq!(report.source_bytes, expected_bytes as u64);
    assert_eq!(report.artifact_bytes, bytes.len() as u64);
    assert_eq!(
        report.elapsed_ns,
        report
            .phases
            .iter()
            .map(|phase| phase.elapsed_ns)
            .sum::<u64>()
    );
    assert_eq!(std::fs::read(manifest).unwrap(), before);
    let again = compile(&fixture());
    assert_eq!(again.input_digest, report.input_digest);
    assert_eq!(again.compiler_id, report.compiler_id);
    assert_eq!(Report::decode(&report.encode().unwrap()).unwrap(), report);
}

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "lang profile {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn profile_uses_explicit_workspace_selection_and_preserves_existing_build_state() {
    let temp = Temp::new();
    let app = temp.0.join("app");
    assert!(bin().arg("new").arg(&app).status().unwrap().success());
    assert!(
        bin()
            .arg("build")
            .arg("--manifest-path")
            .arg(app.join("Lang.json"))
            .status()
            .unwrap()
            .success()
    );
    let lock = std::fs::read(app.join("Lang.lock")).unwrap();
    std::fs::write(
        temp.0.join("Lang.json"),
        r#"{"schema":1,"workspace":{"members":["app"]}}"#,
    )
    .unwrap();
    let result = bin()
        .args(["profile", "compile", "--manifest-path"])
        .arg(temp.0.join("Lang.json"))
        .args(["--package", "app"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report = Report::decode(&result.stdout).unwrap();
    assert_eq!(report.source_files, 1);
    assert_eq!(std::fs::read(app.join("Lang.lock")).unwrap(), lock);
    assert!(!temp.0.join("Lang.lock").exists());
    std::fs::write(
        app.join("src/main.lang"),
        "fn main()->i64{return missing;}\n",
    )
    .unwrap();
    let failed = bin()
        .args(["profile", "compile", "--manifest-path"])
        .arg(app.join("Lang.json"))
        .args(["--message-format", "json"])
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stdout.is_empty());
    let lines = String::from_utf8(failed.stderr).unwrap();
    let diagnostics: Vec<serde_json::Value> = lines
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"].as_str().unwrap().starts_with("SRC") && d.get("span").is_some())
    );
    assert_eq!(std::fs::read(app.join("Lang.lock")).unwrap(), lock);
    assert!(app.join("target/lang").is_dir());
}

#[test]
fn compile_profile_reader_rejects_foreign_formats_overflow_and_incomplete_observations() {
    let report = compile(&fixture());
    let good = serde_json::to_value(&report).unwrap();
    let mut failures = Vec::new();
    for (key, value) in [
        ("schema", serde_json::json!(2)),
        ("kind", serde_json::json!("runtime")),
        ("clock", serde_json::json!("cpu")),
        ("source_files", serde_json::json!(4097)),
        ("source_bytes", serde_json::json!(33 * 1024 * 1024)),
        ("artifact_bytes", serde_json::json!(65 * 1024 * 1024)),
        ("input_digest", serde_json::json!("forged")),
        ("elapsed_ns", serde_json::json!(u64::MAX)),
        ("target", serde_json::json!("../../outside")),
        ("unexpected", serde_json::json!(true)),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        failures.push(bad);
    }
    let mut bad = good.clone();
    bad["phases"][0]["elapsed_ns"] = serde_json::json!(u64::MAX);
    bad["phases"][1]["elapsed_ns"] = serde_json::json!(1);
    failures.push(bad);
    let mut bad = good.clone();
    bad["phases"].as_array_mut().unwrap().pop();
    failures.push(bad);
    let mut bad = good.clone();
    bad["phases"][0]["name"] = serde_json::json!("frontend");
    failures.push(bad);
    for bad in failures {
        assert!(
            Report::decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{bad}"
        );
    }
    assert!(Report::decode(&vec![b' '; lang::profile::MAX_REPORT_BYTES + 1]).is_err());
    let duplicate = String::from_utf8(report.encode().unwrap())
        .unwrap()
        .replacen("{", "{\"schema\":1,", 1);
    assert!(Report::decode(duplicate.as_bytes()).is_err());
    for args in [
        vec!["profile", "compile", "--unknown"],
        vec!["profile", "compile", "--target", "invalid"],
        vec!["profile", "compile", "--message-format", "unsupported"],
    ] {
        let result = bin().args(args).output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
    }
}

#[test]
fn runtime_points_observe_original_owned_vm_and_native_calls_without_changing_failures() {
    use lang_codegen_clif::owned::{CompiledProgram, runtime};
    use lang_runtime::{allocation as a, cleanup as c, execution as e, layout};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/scripts");
    let limits = frontend::CompileLimits::default();
    let analysis = frontend::analyze_file(&root.join("owned_calls.lang"), &root, limits);
    let bytes = artifact::compile(
        analysis.result.unwrap(),
        &analysis.sources,
        limits,
        Default::default(),
    )
    .unwrap();
    let artifact::Program::Ownership(program) =
        artifact::decode(&bytes, Default::default()).unwrap()
    else {
        panic!("owned")
    };
    let flow = program.flow();
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
    .unwrap();
    let native = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    let entry = program.entry().unwrap();
    for fuel in [0, 1, 8, 1000] {
        let observed_vm = e::PointRecorder::new(4096, 1).unwrap();
        let observed_native = e::PointRecorder::new(4096, 1).unwrap();
        let short_vm = e::PointRecorder::new(1, 1).unwrap();
        let short_native = e::PointRecorder::new(1, 1).unwrap();
        let vm = |control| {
            let mut instance = c::Instance::default();
            let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
            let outcome = lang_vm::unit::owned::execute_function(
                flow,
                entry,
                &mut instance,
                Some(&mut allocator),
                control,
            );
            assert_eq!(allocator.stats().live_allocations, 0);
            match outcome {
                Ok(o) => (Some(o.value), o.usage, None),
                Err(e) => (
                    None,
                    e.usage,
                    e.fault.map(|s| e::Point {
                        source: s.source.0,
                        start: s.start,
                        end: s.end,
                    }),
                ),
            }
        };
        let machine = |control| {
            let mut instance = c::Instance::default();
            let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
            let outcome = runtime::execute_entry(
                &native,
                entry,
                &mut instance,
                &mut allocator,
                runtime::Config {
                    control,
                    ..Default::default()
                },
            );
            assert_eq!(allocator.stats().live_allocations, 0);
            match outcome {
                Ok(o) => (Some(o.value), o.usage, None),
                Err(e) => (
                    None,
                    e.usage,
                    e.fault.map(|s| e::Point {
                        source: s.source.0,
                        start: s.start,
                        end: s.end,
                    }),
                ),
            }
        };
        let control = e::Control::with_limits(e::Limits {
            fuel: Some(fuel),
            host_calls: Some(0),
            frames: Some(64),
            live_cells: Some(65536),
        });
        let original = vm(control);
        assert_eq!(machine(control), original);
        assert_eq!(vm(control.and_observations(&observed_vm)), original);
        assert_eq!(
            machine(control.and_observations(&observed_native)),
            original
        );
        let left = observed_vm.snapshot().unwrap();
        let right = observed_native.snapshot().unwrap();
        assert_eq!(left.total, original.1.fuel);
        assert_eq!(right.total, left.total);
        assert_eq!(left.samples, right.samples);
        if fuel == 1000 {
            assert!(original.0.is_some());
            assert_eq!(vm(control.and_observations(&short_vm)), original);
            assert_eq!(machine(control.and_observations(&short_native)), original);
            assert!(matches!(
                short_vm.snapshot(),
                Err(e::ObservationError::Capacity)
            ));
            assert!(matches!(
                short_native.snapshot(),
                Err(e::ObservationError::Capacity)
            ));
        }
    }
}

#[test]
fn runtime_cli_attributes_real_vm_native_and_sampled_points_without_publishing_builds() {
    use lang::profile::runtime::{Engine, Report as RuntimeReport};
    let temp = Temp::new();
    let app = temp.0.join("app");
    assert!(bin().arg("new").arg(&app).status().unwrap().success());
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/scripts/owned_calls.lang"),
        app.join("src/main.lang"),
    )
    .unwrap();
    let mut reports = Vec::new();
    for engine in ["vm", "native"] {
        let result = bin()
            .args(["profile", "run", "--manifest-path"])
            .arg(app.join("Lang.json"))
            .args(["--engine", engine])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let report = RuntimeReport::decode(&result.stdout).unwrap();
        assert!(!report.points.is_empty(), "{report:?}");
        assert_eq!(report.unattributed_points, 0);
        assert_eq!(report.sources[0].path, "src/main.lang");
        assert_eq!(report.total_charged_points, report.usage.fuel);
        assert_eq!(report.sampled_points, report.total_charged_points);
        assert_eq!(
            RuntimeReport::decode(&report.encode().unwrap()).unwrap(),
            report
        );
        reports.push(report);
    }
    assert_eq!(reports[0].engine, Engine::Vm);
    assert_eq!(reports[1].engine, Engine::NativeJit);
    assert_eq!(reports[0].result, reports[1].result);
    assert_eq!(reports[0].usage, reports[1].usage);
    assert_eq!(reports[0].points, reports[1].points);
    let sampled = bin()
        .args(["profile", "run", "--manifest-path"])
        .arg(app.join("Lang.json"))
        .args(["--sample-every", "2"])
        .output()
        .unwrap();
    assert!(sampled.status.success());
    let sample = RuntimeReport::decode(&sampled.stdout).unwrap();
    assert_eq!(sample.usage, reports[0].usage);
    assert_eq!(sample.sampled_points, sample.total_charged_points / 2);
    for options in [
        vec!["--observations", "1"],
        vec!["--engine", "native", "--fuel", "0"],
    ] {
        let failed = bin()
            .args(["profile", "run", "--manifest-path"])
            .arg(app.join("Lang.json"))
            .args(options)
            .args(["--message-format", "json"])
            .output()
            .unwrap();
        assert_eq!(failed.status.code(), Some(1));
        assert!(failed.stdout.is_empty());
        assert!(!failed.stderr.is_empty());
    }
    assert!(!app.join("Lang.lock").exists());
    assert!(!app.join("target").exists());
    std::fs::write(app.join("src/main.lang"), "fn main()->i64{return 42;}").unwrap();
    let copy = bin()
        .args(["profile", "run", "--manifest-path"])
        .arg(app.join("Lang.json"))
        .output()
        .unwrap();
    assert!(copy.status.success());
    assert_eq!(RuntimeReport::decode(&copy.stdout).unwrap().result, 42);
    let refused = bin()
        .args(["profile", "run", "--manifest-path"])
        .arg(app.join("Lang.json"))
        .args(["--engine", "native"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(1));
    assert!(refused.stdout.is_empty());
}

#[test]
fn allocator_trace_observes_original_vm_native_owners_and_entry_return_counters() {
    use lang_codegen_clif::owned::{CompiledProgram, runtime};
    use lang_runtime::{allocation as a, cleanup as c, execution as e, layout};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/packages/profile_runtime");
    let limits = frontend::CompileLimits::default();
    let analysis = frontend::analyze_file(&root.join("main.lang"), &root, limits);
    let bytes = artifact::compile(
        analysis.result.unwrap(),
        &analysis.sources,
        limits,
        Default::default(),
    )
    .unwrap();
    let artifact::Program::Ownership(program) =
        artifact::decode(&bytes, Default::default()).unwrap()
    else {
        panic!("owned")
    };
    let flow = program.flow();
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
    .unwrap();
    let native = CompiledProgram::compile(flow, &layouts, Default::default(), 1000).unwrap();
    let entry = program.entry().unwrap();
    // Include failures before and after owned storage is admitted, and the
    // successful call/Vec/cleanup path. Each observation runs its own entry once.
    for fuel in [0, 1, 8, 16, 1000] {
        let mut vm_baseline = None;
        let mut vm_events = None;
        for machine in [false, true] {
            let mut baseline = None;
            for capacity in [None, Some(4096), Some(1)] {
                let mut instance = c::Instance::default();
                let mut allocator = a::Allocator::new(a::System, Default::default()).unwrap();
                let trace = capacity.map(|limit| allocator.start_trace(limit).unwrap());
                let control = e::Control::with_limits(e::Limits {
                    fuel: Some(fuel),
                    host_calls: Some(0),
                    frames: Some(64),
                    live_cells: Some(65536),
                });
                let observed = if machine {
                    match runtime::execute_entry(
                        &native,
                        entry,
                        &mut instance,
                        &mut allocator,
                        runtime::Config {
                            control,
                            ..Default::default()
                        },
                    ) {
                        Ok(o) => (Some(o.value), o.usage, None),
                        Err(e) => (None, e.usage, e.fault),
                    }
                } else {
                    match lang_vm::unit::owned::execute_function(
                        flow,
                        entry,
                        &mut instance,
                        Some(&mut allocator),
                        control,
                    ) {
                        Ok(o) => (Some(o.value), o.usage, None),
                        Err(e) => (None, e.usage, e.fault),
                    }
                };
                let stats = allocator.stats();
                let result = (observed, stats);
                if let Some(baseline) = &baseline {
                    assert_eq!(
                        &result, baseline,
                        "fuel={fuel}, native={machine}, capacity={capacity:?}"
                    );
                } else {
                    baseline = Some(result);
                }
                if let Some(trace) = trace {
                    if capacity == Some(1) && stats.allocations + stats.deallocations > 1 {
                        assert_eq!(trace.snapshot(), Err(a::TraceError::Capacity));
                    } else {
                        let snapshot = trace.snapshot().unwrap();
                        assert_eq!(snapshot.stats, stats);
                        assert_eq!(snapshot.allocator, allocator.identity());
                        assert_eq!(
                            snapshot.events.len() as u64,
                            stats.requests + stats.deallocations
                        );
                        if capacity == Some(4096) {
                            if machine {
                                assert_eq!(snapshot.events, *vm_events.as_ref().unwrap());
                            } else {
                                vm_events = Some(snapshot.events);
                            }
                        }
                    }
                }
            }
            if machine {
                assert_eq!(baseline, vm_baseline);
            } else {
                vm_baseline = baseline;
            }
        }
        if fuel == 1000 {
            let ((value, usage, fault), stats) = vm_baseline.unwrap();
            assert!(value.is_some());
            assert_eq!(usage.fuel, 23);
            assert_eq!(fault, None);
            assert_eq!(stats.live_allocations, 0);
            assert!(stats.allocations > 0);
        }
    }
}

#[test]
fn runtime_report_reader_refuses_unproven_points_and_false_sampling_totals() {
    use lang::profile::runtime::Report as RuntimeReport;
    let result = bin()
        .args(["profile", "run", "--manifest-path"])
        .arg(fixture())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report = RuntimeReport::decode(&result.stdout).unwrap();
    let good = serde_json::to_value(&report).unwrap();
    let mut failures = Vec::new();
    for (key, value) in [
        ("schema", serde_json::json!(2)),
        ("kind", serde_json::json!("compile")),
        ("sample_every", serde_json::json!(0)),
        ("sampled_points", serde_json::json!(u64::MAX)),
        ("unattributed_points", serde_json::json!(u64::MAX)),
        ("unknown", serde_json::json!(true)),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        failures.push(bad);
    }
    let mut bad = good.clone();
    bad["usage"]["fuel"] = serde_json::json!(0);
    failures.push(bad);
    let mut bad = good.clone();
    bad["points"][0]["end"] = serde_json::json!(u32::MAX);
    failures.push(bad);
    let mut bad = good.clone();
    bad["points"][0]["source"] = serde_json::json!(4096);
    failures.push(bad);
    let mut bad = good.clone();
    bad["sources"][0]["path"] = serde_json::json!("../outside.lang");
    failures.push(bad);
    let mut bad = good.clone();
    bad["engine"] = serde_json::json!("native_jit");
    failures.push(bad);
    for bad in failures {
        assert!(
            RuntimeReport::decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{bad}"
        );
    }
    assert!(RuntimeReport::decode(&vec![b' '; lang::profile::MAX_REPORT_BYTES + 1]).is_err());
    let duplicate = String::from_utf8(report.encode().unwrap())
        .unwrap()
        .replacen("{", "{\"schema\":1,", 1);
    assert!(RuntimeReport::decode(duplicate.as_bytes()).is_err());
}

fn allocation_cli(engine: &str, extra: &[&str]) -> std::process::Output {
    bin()
        .args(["profile", "allocations", "--manifest-path"])
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("examples/packages/profile_runtime/Lang.json"),
        )
        .args(["--engine", engine])
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn allocation_cli_reconciles_original_vm_native_success_failure_and_capacity() {
    use lang::profile::allocation::{Outcome, Report};
    let mut original = None;
    for engine in ["vm", "native"] {
        let output = allocation_cli(engine, &[]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        let report = Report::decode(&output.stdout).unwrap();
        assert_eq!(report.outcome, Outcome::Completed { result: 42 });
        assert_eq!(report.usage.fuel, 23);
        assert_eq!(report.stats.live_allocations, 0);
        assert_eq!(report.stats.live_bytes, 0);
        assert!(report.stats.allocations > 0);
        assert_eq!(Report::decode(&report.encode().unwrap()).unwrap(), report);
        if let Some((events, stats, usage)) = &original {
            assert_eq!(&report.events, events);
            assert_eq!(&report.stats, stats);
            assert_eq!(&report.usage, usage);
        } else {
            original = Some((report.events, report.stats, report.usage));
        }
        for fuel in ["0", "8", "16"] {
            let output = allocation_cli(engine, &["--fuel", fuel, "--message-format", "json"]);
            assert_eq!(output.status.code(), Some(1));
            let report = Report::decode(&output.stdout).unwrap();
            assert!(matches!(
                report.outcome,
                Outcome::Failed { fault: Some(_), .. }
            ));
            assert!(
                String::from_utf8(output.stderr)
                    .unwrap()
                    .contains("CLI0002")
            );
            assert_eq!(report.usage.fuel, fuel.parse::<u64>().unwrap());
            assert_eq!(report.stats.live_allocations, 0);
        }
        let output = allocation_cli(engine, &["--events", "1", "--message-format", "json"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("PRF0003")
        );
        let output = allocation_cli(
            engine,
            &["--events", "1", "--fuel", "16", "--message-format", "json"],
        );
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let diagnostics = String::from_utf8(output.stderr).unwrap();
        assert!(diagnostics.contains("CLI0002"), "{diagnostics}");
        assert!(diagnostics.contains("PRF0003"), "{diagnostics}");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/packages/profile_runtime");
    assert!(!root.join("Lang.lock").exists());
    assert!(!root.join("target").exists());
    for extra in [
        vec!["--events", "0"],
        vec!["--events", "16385"],
        vec!["--sample-every", "1"],
        vec!["--events", "4", "--events", "4"],
    ] {
        let output = allocation_cli("vm", &extra);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn allocation_reader_replays_real_zero_resize_oom_and_preserves_live_prefixes() {
    use lang::profile::allocation::{Event, Report};
    use lang_runtime::allocation as a;
    struct Oom;
    impl a::Provider for Oom {
        type Region = a::Storage;
        fn allocate(&mut self, r: a::Request) -> Option<a::Storage> {
            if r.bytes() == 64 {
                None
            } else {
                a::System.allocate(r)
            }
        }
        fn deallocate(&mut self, r: a::Storage, q: a::Request) {
            a::System.deallocate(r, q);
        }
    }
    let mut report = Report::decode(&allocation_cli("vm", &[]).stdout).unwrap();
    let mut allocator = a::Allocator::new(Oom, Default::default()).unwrap();
    let trace = allocator.start_trace(8).unwrap();
    let zero = allocator.allocate(a::Request::new(0, 8).unwrap()).unwrap();
    let mut value = allocator.allocate(a::Request::new(16, 8).unwrap()).unwrap();
    allocator.resize(&mut value, 32).unwrap();
    assert_eq!(
        allocator.allocate(a::Request::new(64, 8).unwrap()),
        Err(a::Error::OutOfMemory)
    );
    let snapshot = trace.snapshot().unwrap();
    report.allocator = snapshot.allocator.get();
    report.events = snapshot.events.into_iter().map(Event::from).collect();
    report.stats = allocator.stats().into();
    report.validate().unwrap();
    assert_eq!(report.stats.cumulative_bytes, 112);
    assert_eq!(report.stats.peak_live_bytes, 48);
    assert_eq!(report.stats.live_bytes, 32);
    assert_eq!(report.stats.live_allocations, 2);
    assert_eq!(Report::decode(&report.encode().unwrap()).unwrap(), report);
    allocator.release(value).unwrap();
    allocator.release(zero).unwrap();
    report.events = trace
        .snapshot()
        .unwrap()
        .events
        .into_iter()
        .map(Event::from)
        .collect();
    report.stats = allocator.stats().into();
    report.validate().unwrap();
    assert_eq!(report.stats.live_allocations, 0);
    let good = serde_json::to_value(&report).unwrap();
    let mut failures = Vec::new();
    for (key, value) in [
        ("schema", serde_json::json!(2)),
        ("allocator", serde_json::json!(0)),
        ("profile", serde_json::json!("copy")),
        ("unknown", serde_json::json!(true)),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        failures.push(bad);
    }
    for (index, key, value) in [
        (0, "generation", serde_json::json!(0)),
        (1, "generation", serde_json::json!(1)),
        (3, "generation", serde_json::json!(u64::MAX)),
        (0, "unknown", serde_json::json!(true)),
    ] {
        let mut bad = good.clone();
        bad["events"][index]["operation"][key] = value;
        failures.push(bad);
    }
    for key in [
        "live_bytes",
        "cumulative_bytes",
        "peak_live_bytes",
        "live_allocations",
        "peak_live_allocations",
        "requests",
        "allocations",
        "deallocations",
    ] {
        let mut bad = good.clone();
        bad["stats"][key] = serde_json::json!(u64::MAX);
        failures.push(bad);
        let mut bad = good.clone();
        bad["events"][2]["stats"][key] = serde_json::json!(u64::MAX);
        failures.push(bad);
    }
    let mut bad = good.clone();
    bad["events"][3]["request"]["bytes"] = serde_json::json!(17);
    failures.push(bad);
    let mut bad = good.clone();
    bad["events"][4]["request"]["bytes"] = serde_json::json!(0);
    failures.push(bad);
    let mut bad = good.clone();
    bad["events"][0]["request"]["alignment"] = serde_json::json!(3);
    failures.push(bad);
    let mut bad = good.clone();
    bad["events"]
        .as_array_mut()
        .unwrap()
        .push(good["events"][6].clone());
    failures.push(bad);
    let mut bad = good.clone();
    bad["outcome"] = serde_json::json!({"status":"failed", "message":"x", "fault":{"source":4096,"start":0,"end":0}});
    failures.push(bad);
    for bad in failures {
        assert!(
            Report::decode(&serde_json::to_vec(&bad).unwrap()).is_err(),
            "{bad}"
        );
    }
    let duplicate = String::from_utf8(report.encode().unwrap())
        .unwrap()
        .replacen("{", "{\"schema\":1,", 1);
    assert!(Report::decode(duplicate.as_bytes()).is_err());
    assert!(Report::decode(&vec![b' '; lang::profile::MAX_REPORT_BYTES + 1]).is_err());
}

#[test]
fn allocation_report_bounds_and_cli_admission_preserve_source_diagnostics_and_files() {
    use lang::profile::allocation::{
        Event, Failure, Operation, Outcome, Report, Request, Statistics,
    };
    let report = Report::decode(&allocation_cli("vm", &[]).stdout).unwrap();
    let mut spaced = report.clone();
    for event in &mut spaced.events {
        match &mut event.operation {
            Operation::Allocated { generation } | Operation::Retired { generation } => {
                *generation *= 2
            }
            Operation::AllocationFailed { .. } => {}
        }
    }
    spaced.validate().unwrap();
    let mut overflow = report.clone();
    overflow.events = vec![
        Event {
            request: Request {
                bytes: i64::MAX as u64,
                alignment: 1,
            },
            operation: Operation::AllocationFailed {
                reason: Failure::OutOfMemory,
            },
            stats: Statistics {
                requests: 1,
                cumulative_bytes: i64::MAX as u64,
                ..Default::default()
            },
        },
        Event {
            request: Request {
                bytes: i64::MAX as u64,
                alignment: 1,
            },
            operation: Operation::AllocationFailed {
                reason: Failure::OutOfMemory,
            },
            stats: Statistics {
                requests: 2,
                cumulative_bytes: u64::MAX - 1,
                ..Default::default()
            },
        },
        Event {
            request: Request {
                bytes: 2,
                alignment: 1,
            },
            operation: Operation::AllocationFailed {
                reason: Failure::OutOfMemory,
            },
            stats: Statistics {
                requests: 3,
                cumulative_bytes: 0,
                ..Default::default()
            },
        },
    ];
    assert!(overflow.validate().is_err());
    let mut oversized = report.clone();
    oversized.events = vec![report.events[0]; lang_runtime::allocation::MAX_TRACE_EVENTS + 1];
    assert!(oversized.validate().is_err());
    let mut oversized = report.clone();
    oversized.outcome = Outcome::Failed {
        message: "x".repeat(8193),
        fault: None,
    };
    assert!(oversized.encode().is_err());
    let mut oversized = report;
    let source = oversized.sources[0].clone();
    oversized.sources = (0..512)
        .map(|id| lang::profile::runtime::Source {
            id,
            path: "x".repeat(8192),
            bytes: 0,
            digest: source.digest.clone(),
        })
        .collect();
    oversized.validate().unwrap();
    assert!(
        oversized.encode().is_err(),
        "preflight output must remain within 4 MiB"
    );
    let temp = Temp::new();
    let app = temp.0.join("app");
    assert!(bin().arg("new").arg(&app).status().unwrap().success());
    std::fs::write(app.join("src/main.lang"), "fn main()->i64{return 42;}").unwrap();
    std::fs::write(app.join("Lang.lock"), "existing lock").unwrap();
    for source in [
        "fn main()->i64{return 42;}",
        "fn main()->i64{return missing;}",
    ] {
        std::fs::write(app.join("src/main.lang"), source).unwrap();
        let output = bin()
            .args(["profile", "allocations", "--manifest-path"])
            .arg(app.join("Lang.json"))
            .args(["--message-format", "json"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let diagnostic = String::from_utf8(output.stderr).unwrap();
        if source.contains("missing") {
            assert!(
                diagnostic.contains("span") && diagnostic.contains("main.lang"),
                "{diagnostic}"
            );
            assert!(!diagnostic.contains("PRF0003"));
        } else {
            assert!(diagnostic.contains("PRF0003"));
        }
        assert_eq!(
            std::fs::read_to_string(app.join("src/main.lang")).unwrap(),
            source
        );
        assert_eq!(
            std::fs::read_to_string(app.join("Lang.lock")).unwrap(),
            "existing lock"
        );
        assert!(!app.join("target").exists());
    }
}
