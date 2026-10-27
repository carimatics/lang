//! Observational M3 entry costs; run in release mode. These are not test times.
use lang_runtime::{allocation as a, execution as e, task::runner as r};
use sha2::{Digest, Sha256};
use std::{hint::black_box, path::Path, time::Instant};

const WARMUPS: usize = 3;
const WORKLOADS: [(&str, &str, &[u8], i64); 6] = [
    (
        "spawn-join",
        "examples/scripts/tasks.lang",
        include_bytes!("scripts/tasks.lang"),
        42,
    ),
    (
        "owned-channel",
        "tests/fixtures/task_channels.lang",
        include_bytes!("../tests/fixtures/task_channels.lang"),
        42,
    ),
    (
        "mixed-cancellation",
        "tests/fixtures/task_channel_mixed_cancel.lang",
        include_bytes!("../tests/fixtures/task_channel_mixed_cancel.lang"),
        42,
    ),
    (
        "owned-buffers",
        "tests/fixtures/task_owned_buffers.lang",
        include_bytes!("../tests/fixtures/task_owned_buffers.lang"),
        71,
    ),
    (
        "service-state",
        "tests/fixtures/task_service_state_replace.lang",
        include_bytes!("../tests/fixtures/task_service_state_replace.lang"),
        42,
    ),
    (
        "single-task",
        "inline:single-task",
        b"async fn main() -> i64 { return 42; }",
        42,
    ),
];

fn distribution(samples: &[u64]) -> serde_json::Value {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    serde_json::json!({"samples_ns":samples,"min_ns":sorted[0],
        "median_ns":sorted[sorted.len()/2],"p95_ns":sorted[(95*sorted.len()).div_ceil(100)-1],
        "max_ns":sorted[sorted.len()-1]})
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let samples: usize = args.next().map_or(Ok(21), |s| s.parse())?;
    if !(1..=1001).contains(&samples) || args.next().is_some() {
        return Err("usage: m3-costs [samples:1..1001]".into());
    }
    let mut workloads = Vec::new();
    for (name, path, source, expected) in WORKLOADS {
        let started = Instant::now();
        let analyzed = lang::frontend::analyze_virtual(
            lang_diagnostics::SourceMap::new(),
            Path::new(path),
            source,
            None,
            Default::default(),
            None,
        );
        assert!(
            analyzed.diagnostics.is_empty(),
            "{:?}",
            analyzed.diagnostics
        );
        let lang_semantics::analysis::Program::Tasks(program) = analyzed.result.unwrap() else {
            return Err("task workload required".into());
        };
        let frontend_ns = u64::try_from(started.elapsed().as_nanos())?;
        let started = Instant::now();
        let native = lang_codegen_clif::tasks::CompiledProgram::compile(
            program.verified(),
            Default::default(),
        )?;
        let jit_compile_ns = u64::try_from(started.elapsed().as_nanos())?;
        let control = e::Control::with_limits(Default::default()).and_service_work(Some(1_000_000));
        let limits = r::Limits {
            trace_events: 0,
            ..Default::default()
        };
        let mut times: [Vec<u64>; 2] = Default::default();
        let mut counters = None;
        for sample in 0..WARMUPS + samples {
            // Alternate first backend to reduce systematic ordering bias.
            for backend in [sample % 2, 1 - sample % 2] {
                let mut allocator = a::Allocator::new(a::System, Default::default())?;
                let started = Instant::now();
                let outcome = if backend == 0 {
                    lang_vm::unit::tasks::execute_entry(
                        black_box(program.verified()),
                        &mut allocator,
                        control,
                        limits,
                    )
                } else {
                    native.execute(&mut allocator, control, limits)
                }?;
                let ns = u64::try_from(started.elapsed().as_nanos())?;
                // Validation and report construction are outside the interval.
                assert_eq!(black_box(outcome.value), expected, "{name}");
                assert!(!outcome.poisoned);
                assert_eq!(outcome.tasks.live, 0);
                let stats = allocator.stats();
                assert_eq!(stats.live_allocations, 0);
                assert_eq!(stats.live_bytes, 0);
                let values = serde_json::json!({"fuel":outcome.usage.fuel,"service_work":outcome.usage.service_work,"host_calls":outcome.usage.host_calls,
                    "peak_frames":outcome.usage.peak_frames,"peak_cells":outcome.usage.peak_live_cells,"admitted_tasks":outcome.tasks.admitted,"peak_tasks":outcome.tasks.peak_live,
                    "peak_managed_bytes":stats.peak_live_bytes,"cumulative_managed_bytes":stats.cumulative_bytes,"peak_managed_allocations":stats.peak_live_allocations,"allocation_requests":stats.requests,
                    "final_live_tasks":outcome.tasks.live,"final_live_allocations":stats.live_allocations,"final_live_bytes":stats.live_bytes});
                if let Some(previous) = &counters {
                    assert_eq!(previous, &values, "{name}");
                } else {
                    counters = Some(values);
                }
                if sample >= WARMUPS {
                    times[backend].push(ns);
                }
            }
        }
        let sha256: String = Sha256::digest(source)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        workloads.push(serde_json::json!({"workload":name,"source":path,"sha256":sha256,
            "expected":expected,"frontend_ns":frontend_ns,"jit_compile_ns":jit_compile_ns,
            "jit_preparation_usage":{"work":native.usage().work,"allocation_bytes":native.usage().allocation_bytes,"elements":native.usage().elements},
            "vm":distribution(&times[0]),"jit":distribution(&times[1]),"counters":counters}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "format":"lang-m3-entry-costs","version":1,"profile":if cfg!(debug_assertions){"dev"}else{"release"},
            "target":{"arch":std::env::consts::ARCH,"os":std::env::consts::OS},
            "samples":samples,"warmups":WARMUPS,"order":"alternating VM/JIT first within each paired sample",
            "entry_includes":"per-call preparation, execution, mandatory cleanup",
            "entry_excludes":"frontend, JIT compile, allocator creation, outcome validation and report construction",
            "limits":"default finite execution/allocator/runner limits; service work 1000000; trace events 0",
            "workloads":workloads
        }))?
    );
    Ok(())
}
