<!-- canonical-language: en; japanese-translation: ja/benchmarks.md -->

# Prototype benchmark evidence

[日本語版](ja/benchmarks.md)

These measurements are design evidence for the pre-alpha prototypes. They are not language performance guarantees, release targets, or a final backend selection.

## Current macOS AArch64 baseline

The current [baseline](../benchmarks/results/2026-10-16-macos-aarch64-fa9a69b.json) was recorded from clean commit `fa9a69b8650b6a2f4b15ea645ab0e18f522a8790` on an Apple M1 Max running macOS 27.0.1 at 2026-10-16 12:24:45 JST. It used harness schema 12, the pinned Rust 1.99.0 toolchain, and Apple Clang 21.0.0 in a release build.

Schema 12 preserves all 44 timing rows and adds deterministic host-callback requested-payload heap profiles. The single callback records one allocation with 8 total/peak live bytes and zero live bytes afterward; the burst records 64 allocations with 512 total/peak live bytes and zero afterward. These profiles measure requested payload lifetime, not allocator or process memory.

```bash
./target/release/prototype-bench \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000 \
  --startup-iterations 10
```

Each row reports nanoseconds per operation. RSD is the relative population standard deviation across 31 samples.

| Measurement | Operations per sample | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering: wrapping `i64` add | 1,000 | 387.000 ns | 444.250 ns | 4.921% |
| Bytecode lowering: signed minimum | 1,000 | 1.047 us | 1.145 us | 3.947% |
| Bytecode lowering: countdown back-edge | 1,000 | 1.630 us | 1.730 us | 2.125% |
| Bytecode lowering: two-value accumulator | 1,000 | 1.729 us | 1.844 us | 1.904% |
| Bytecode lowering: conditional sum | 1,000 | 2.982 us | 3.151 us | 2.786% |
| Cranelift JIT compilation: wrapping `i64` add | 1 | 19.166 us | 39.792 us | 78.766% |
| Cranelift JIT compilation: signed minimum | 1 | 23.875 us | 29.792 us | 20.127% |
| Cranelift JIT compilation: countdown back-edge | 1 | 27.042 us | 40.958 us | 15.224% |
| Cranelift JIT compilation: two-value accumulator | 1 | 31.334 us | 41.958 us | 10.191% |
| Cranelift JIT compilation: conditional sum | 1 | 40.833 us | 48.959 us | 8.694% |
| Cranelift AOT object emission: wrapping `i64` add | 1 | 13.667 us | 16.375 us | 18.600% |
| Cranelift AOT object emission: signed minimum | 1 | 18.417 us | 20.375 us | 4.883% |
| Cranelift AOT object emission: countdown back-edge | 1 | 22.125 us | 28.083 us | 7.200% |
| Cranelift AOT object emission: two-value accumulator | 1 | 26.166 us | 28.375 us | 3.537% |
| Cranelift AOT object emission: conditional sum | 1 | 35.791 us | 38.792 us | 3.082% |
| macOS system link: precompiled C harness plus add, minimum, countdown, accumulator, and conditional-sum objects | 1 | 37.325 ms | 39.520 ms | 3.276% |
| macOS AOT process startup: single launch | 1 | 1.527 ms | 2.446 ms | 270.642% |
| macOS AOT process startup: ten-launch batch, per launch | 10 | 1.678 ms | 1.890 ms | 6.180% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.930 ns | 1.026 ns | 2.830% |
| VM wrapping `i64` add call | 100,000 | 182.222 ns | 184.347 ns | 0.609% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 2.083 ns | 3.646% |
| Direct Rust signed minimum call | 100,000 | 0.930 ns | 0.960 ns | 1.098% |
| VM signed minimum call | 100,000 | 499.119 ns | 503.471 ns | 0.567% |
| JIT signed minimum call | 100,000 | 2.662 ns | 2.863 ns | 2.932% |
| Direct Rust countdown call | 100,000 | 0.930 ns | 1.010 ns | 4.932% |
| VM countdown call | 100,000 | 1.606 us | 1.629 us | 10.380% |
| VM countdown call with unset interruption flag | 100,000 | 1.603 us | 1.608 us | 0.263% |
| VM countdown call with unexpired deadline | 100,000 | 3.314 us | 3.325 us | 0.181% |
| JIT countdown call | 100,000 | 12.799 ns | 13.977 ns | 2.816% |
| Direct Rust two-value accumulator call | 100,000 | 1.705 ns | 1.926 ns | 4.815% |
| VM two-value accumulator call | 100,000 | 1.651 us | 1.657 us | 0.220% |
| JIT two-value accumulator call | 100,000 | 12.750 ns | 13.379 ns | 1.616% |
| Direct Rust conditional-sum call | 100,000 | 5.605 ns | 6.068 ns | 3.529% |
| VM conditional-sum call | 100,000 | 3.039 us | 3.053 us | 0.237% |
| JIT conditional-sum call | 100,000 | 20.172 ns | 21.308 ns | 1.989% |
| Direct Rust host callback | 100,000 | 0.930 ns | 0.979 ns | 1.345% |
| VM host callback | 100,000 | 175.515 ns | 178.818 ns | 0.843% |
| JIT host callback | 100,000 | 2.795 ns | 3.116 ns | 4.188% |
| Direct Rust host-allocation callback | 100,000 | 11.857 ns | 12.503 ns | 2.064% |
| VM host-allocation callback | 100,000 | 187.240 ns | 190.214 ns | 0.699% |
| JIT host-allocation callback | 100,000 | 13.471 ns | 14.550 ns | 29.399% |
| Direct Rust 64-allocation-burst callback | 100,000 | 768.283 ns | 773.525 ns | 0.290% |
| VM 64-allocation-burst callback | 100,000 | 946.601 ns | 957.526 ns | 0.497% |
| JIT 64-allocation-burst callback | 100,000 | 771.625 ns | 777.079 ns | 0.369% |

Cranelift's compiled code buffer contained 8 bytes for add, 20 bytes for minimum, 20 bytes for countdown, 36 bytes for the accumulator, 44 bytes for conditional sum, and 40 bytes for the host callback. The emitted add, minimum, countdown, accumulator, and conditional-sum objects were 352, 368, 376, 392, and 400 bytes. The linked AOT harness was 17,104 bytes. JIT code sizes exclude object-file structure, relocation data, unwind/debug information, runtime support, and linker output. Object sizes include Mach-O structure but not the C harness or final executable; the executable size includes both plus the platform linker's defaults, but not a complete runtime distribution.

## Schema 13 format and migration

Schema 13 preserves every schema 12 timing, artifact, and heap field and adds `process_memory_measurement_scope`, `process_memory_metric`, and `process_memory_measurements`. The scope is `linked_aot_executable_process`. The single row, `aot_process_peak_resident_memory_add_min_countdown_sum_to_sum_below`, records `bytes/process`, `sample_count`, minimum, median, p95, maximum, mean, and relative standard deviation after launching the linked AOT executable once per sample.

The metric is `macos_rusage_ru_maxrss_bytes` on macOS and `windows_process_peak_working_set_bytes` on Windows MSVC. The former is parsed from `/usr/bin/time -l`; the latter is read from `System.Diagnostics.Process.PeakWorkingSet64` through Windows PowerShell. Both include private and shared resident pages plus loader/runtime state and therefore are platform-specific process envelopes, not allocator-overhead or representative-application measurements. Consumers must branch on `schema_version`; schema 1–12 JSON files remain immutable evidence. A native three-sample smoke run validates the macOS path, and Windows CI requires one positive, ordered 31-sample distribution. No clean schema 13 baseline or remote Windows result has been committed yet.

## Schema 12 format and migration

Schema 12 adds `heap_measurement_scope` and `heap_measurements` while preserving all schema 11 timing rows and artifact fields. The scope is `host_callback_requested_payload_bytes`; each profile records `allocation_count`, `total_allocated_bytes`, `peak_live_bytes`, and `live_bytes_after`. The harness runs the same generic single and burst callback workloads through a safe measuring heap outside the timed loops.

The exact single profile is `(1, 8, 8, 0)` and the burst profile is `(64, 512, 512, 0)`. These values cover requested `i64` payloads and logical cleanup only. They exclude allocator metadata and fragmentation, the stack, generated code, VM/JIT internal allocations, and process RSS. Consumers must branch on `schema_version`; schema 1–11 JSON files remain immutable evidence. The clean macOS AArch64 schema 12 result above records both profiles and all 44 unique timing rows. Schema 13 and Windows CI retain the same heap contract, but no remote result has been inspected or committed.

## Schema 11 format and migration

Schema 11 adds `rust_host_allocation_burst_call`, `vm_host_allocation_burst_call`, and `jit_host_allocation_burst_call`. Each operation creates 64 distinct `Box<i64>` values, keeps all 512 payload bytes live in a fixed array, computes their wrapping sum, and then drops them. `black_box` keeps each box and loaded value opaque to optimization. The VM and JIT rows include the same typed host-call dispatch around the registered callback, while the direct Rust row provides the host-side baseline. The harness verifies all three results before warm-up and timing.

The existing AOT link/startup series remains unchanged, while macOS and configured Windows integration tests execute a corresponding C callback that holds 64 `malloc` allocations live before reading and freeing them. The rows do not measure allocator metadata, the stack-resident pointer array, process peak RSS, failure behavior, a language heap, or a representative application. Schema 11 leaves all schema 10 rows and artifact fields unchanged. Consumers must branch on `schema_version`; schema 1–10 JSON files remain immutable evidence. The clean historical schema 11 result records all 44 unique rows and direct Rust, VM, and JIT burst medians of 782.624 ns, 962.329 ns, and 786.270 ns. Their RSD values are 0.331%, 11.822%, and 0.378%; the VM maximum of 1.620 us raises its RSD despite a 967.234 ns p95, so those values remain directional evidence rather than a performance budget. The Windows Server 2025 workflow now requires schema 13 while retaining all three burst rows.

## Schema 10 format and migration

Schema 10 adds `vm_countdown_deadline_call`. It executes the same alternating positive/immediate-return inputs and verified countdown bytecode as `vm_countdown_call`, but supplies an absolute monotonic deadline set one hour after harness preparation through `ExecutionControl`. The harness verifies equivalent results before warm-up and timing, and the deadline remains shared across all samples rather than being recomputed per call.

This row measures the steady-state cost of enabling deadline checks while the deadline remains unexpired. It does not measure expiration latency, scheduler accuracy, callback overrun, callback preemption, or native/JIT enforcement. Schema 10 leaves all existing rows and artifact fields unchanged. Consumers must branch on `schema_version`; schema 1–9 JSON files remain immutable evidence. The current clean schema 12 result records a 1.606 us ordinary VM countdown median and a 3.314 us unexpired-deadline median: a 1.708 us, 106.379% increase. The 10.380% and 0.181% RSD values show that the paired medians are directional evidence, not an overhead budget. Schema 11 retains this row, and schema 13 is the current Windows workflow contract.

## Schema 9 format and migration

Schema 9 adds `vm_countdown_interruptible_call`. It executes the same alternating positive/immediate-return inputs and the same verified countdown bytecode as `vm_countdown_call`, but supplies a shared, unset interruption flag through `ExecutionControl`. In the current schema 12 artifact, both rows include the VM's optional-control branch; the new row additionally performs the flag's acquire load at each executed instruction and terminator. The harness verifies equivalent results before warm-up and timing.

This row measures the steady-state cost of enabling cooperative interruption when no request occurs. It does not measure interruption latency, cross-thread wake-up, a set flag, callback preemption, or native/JIT enforcement. Schema 9 leaves all lowering, compilation, artifact-size, host-callback, link, and startup fields unchanged. Consumers must branch on `schema_version`; schema 1–8 JSON files remain immutable evidence. In the current clean schema 12 result, the ordinary VM countdown median is 1.606 us and the unset-interruption median is 1.603 us, a 3.424 ns or 0.213% decrease that is smaller than the rows' 10.380% and 0.263% RSD values. This single paired microbenchmark therefore shows no measurable interruption-check regression and does not establish an overhead budget. Schema 10 retains this row, and schema 13 is the current Windows workflow contract.

## Schema 8 format and migration

Schema 8 adds `rust_host_allocation_call`, `vm_host_allocation_call`, and `jit_host_allocation_call`. Each operation passes an `i64` through one Rust `Box` allocate/read/drop round trip; `black_box` makes the allocated box and returned value opaque to optimization. The VM and JIT rows include their existing typed host-call dispatch around the same registered callback, while the direct Rust row provides the host-side baseline. The harness checks all three results before warming and timing them.

The existing AOT system-link, executable-size, and process-startup series do not change: a single allocation inside a launched harness would be dominated by process creation and loader work. Separate platform integration tests execute the corresponding C `malloc`/read/`free` callback through linked AOT objects. The new rows therefore measure a narrow host-owned allocation plus VM/JIT boundary cost; they do not measure a language heap, allocator policy, peak memory, allocation failure, collection, or representative application behavior. Consumers must branch on `schema_version`; schema 1–7 JSON files remain immutable evidence. The clean macOS AArch64 schema 8 result is recorded above.

The Windows Server 2025 workflow first required schema 8 and all three single-allocation rows in addition to the unchanged platform measurements. No remote schema 8 result has been inspected or committed; the workflow now requires schema 13 as described above.

## Schema 7 format and migration

Schema 7 adds the `sum_below` conditional-sum workload to bytecode lowering, JIT compilation, AOT object emission, direct/VM/JIT calls, generated-code size, and object size. Runtime samples alternate between `(64, 33)`, which exercises both the skip and add paths and returns `528`, and `(-7, 33)`, which takes the immediate-return path and returns `0`. Before timing, the harness checks direct/VM/JIT equivalence for `(-7, 33)`, `(0, 33)`, `(1, 1)`, `(1, 2)`, and `(64, 33)`.

The tier-1 link and process-startup sets now include the fifth, conditional-sum object. Schema 7 therefore renames the three platform rows to `system_link_add_min_countdown_sum_to_sum_below`, `aot_process_startup_add_min_countdown_sum_to_sum_below`, and `aot_process_startup_batch_add_min_countdown_sum_to_sum_below`, and renames the executable-size field to `aot_add_min_countdown_sum_to_sum_below`. It also adds `jit_sum_below` and `aot_sum_below` size fields plus six conditional-sum measurement rows. Consumers must branch on `schema_version`; existing schema 1–6 JSON files remain immutable evidence.

A clean macOS AArch64 schema 7 result is recorded above. The Windows evidence path is present, but no remote schema 7 result has been inspected or committed.

## Schema 6 format and migration

Schema 6 adds the two-value `sum_to` accumulator workload to bytecode lowering, JIT compilation, AOT object emission, direct/VM/JIT calls, generated-code size, and object size. Runtime samples reuse inputs `64` and `-7`, producing `2080` after a 64-iteration loop or `0` on the immediate-return path. The harness checks `-7`, `0`, `1`, and `64` for direct/VM/JIT equivalence before timing.

The tier-1 link and process-startup sets now include the accumulator object. Schema 6 therefore renames the three platform rows to `system_link_add_min_countdown_sum_to`, `aot_process_startup_add_min_countdown_sum_to`, and `aot_process_startup_batch_add_min_countdown_sum_to`, and renames the executable-size field to `aot_add_min_countdown_sum_to`. It also adds `jit_sum_to` and `aot_sum_to` size fields plus six accumulator measurement rows. Consumers must branch on `schema_version`; existing schema 1–5 JSON files remain immutable evidence.

A clean macOS AArch64 schema 6 result is recorded above. The Windows evidence path is present, but no remote schema 6 result has been inspected or committed.

## Schema 5 format and migration

Schema 5 adds the top-level `startup_iterations` field and the `aot_process_startup_batch_add_min_countdown` measurement. The existing single-launch row remains at one operation per sample. The batched row runs the configured number of sequential process launches inside each sample and divides the elapsed time by that count; the default is 10. This provides an amortized launch-cost estimate that is less sensitive to one scheduling delay, while the single-launch row remains the tail-sensitive latency observation.

The two rows answer different questions and must not be merged. Batching does not isolate loader time, remove host noise, or replace individual-latency percentiles. Consumers must branch on `schema_version`; existing schema 1–4 JSON files remain immutable evidence.

A clean macOS AArch64 schema 5 result is recorded above. The Windows evidence path is present, but no remote schema 5 result has been inspected or committed.

## Schema 4 format and migration

Schema 4 adds `executable_file_bytes.aot_add_min_countdown` and the `aot_process_startup_add_min_countdown` measurement. The executable-size field is a positive byte count on the initial tier-1 hosts and `null` elsewhere. Consumers must branch on `schema_version`; existing schema 1–3 JSON files remain immutable evidence.

The startup row measures repeated process creation, dynamic loading, execution of the add, minimum, and countdown checks, output collection, and process termination. The harness validates one link and execution before timing and does not flush operating-system caches, so this is a warm-cache process-startup measurement rather than cold-boot latency. The executable includes the precompiled C harness and platform linker defaults; it is not a complete runtime distribution or installation-size measurement.

A clean macOS AArch64 schema 4 result is recorded above. The Windows evidence path is present, but no remote schema 4 result has been inspected or committed.

## Schema 3 format and migration

Schema 3 adds a bounded countdown workload to bytecode lowering, JIT compilation, AOT object emission, direct/VM/JIT calls, artifact sizes, and the system-link set. Runtime samples alternate between input `64`, which carries an `i64` block parameter around a back-edge until it reaches zero, and `-7`, which takes the immediate-return path. The harness checks `-7`, `0`, `1`, and `64` for direct/VM/JIT equivalence before timing.

Schema 3 adds `jit_countdown` and `aot_countdown` size fields plus six countdown measurement rows. It renames `system_link_add_min` to `system_link_add_min_countdown`; consumers must branch on `schema_version` rather than merging these names. Existing schema 1 and 2 JSON files remain immutable historical evidence.

## Windows x86-64 evidence path

The schema 7 harness records the conditional-sum lowering/compilation/runtime rows, the renamed system-link and process-startup rows, and `executable_file_bytes.aot_add_min_countdown_sum_to_sum_below` on Windows x86-64 MSVC. It compiles the C harness with `cl.exe`, discovers the tier-1 `link.exe` through `LinkTarget`, and validates one link and execution before timing. Schema 8 retains those platform fields and the 31/1,000/100,000/10 parameters and adds the three single-allocation runtime rows. Schema 9 adds `vm_countdown_interruptible_call`, schema 10 adds `vm_countdown_deadline_call`, schema 11 adds three allocation-burst rows, schema 12 adds the two exact heap profiles, and schema 13 adds the linked-process peak-memory distribution; schema 13 is now required by the Windows Server 2025 workflow. The workflow also validates clean-checkout metadata, the `windows`/`x86_64` identity, interruption, deadline, conditional-sum, single-allocation, allocation-burst, heap, process-memory, and platform measurements, the startup batch size, and a positive executable size before uploading the JSON for 30 days.

This is an evidence-producing path, not recorded Windows evidence: no remote runner result from this change has been inspected or committed. The macOS table above therefore remains the only current measured baseline.

## Generated-code inspection

The add and host-callback objects were regenerated from clean commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad` and inspected on the macOS AArch64 host on 2026-10-16 at 00:25 JST. The signed-minimum object was first regenerated from clean commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483` at 00:43 JST, then regenerated after the explicit-CFG migration from clean commit `f6eb016798c237412b59057fa32257ca2bc5d016` at 01:08 JST.

| Prototype | Export | Import | Generated body | Relocations |
| --- | --- | --- | --- | --- |
| Wrapping `i64` add | `_prototype_add` | None | `add x0, x0, x1`; `ret` | None |
| Signed `i64` minimum | `_prototype_min` | None | `cmp`; conditional `b.lt`; else-side `mov`; two `ret` paths | None |
| Typed host callback | `_prototype_host_callback` | `_lang_host_i64_0` | Pointer-authenticated frame, GOT load, indirect `blr`, authenticated return | `GOTLDP` and `GOTLDPOFF` against `_lang_host_i64_0` |

All three files are 64-bit ARM64 Mach-O objects. The add prototype lowers to an eight-byte, two-instruction body without relocations. Minimum lowers to a 20-byte compare and real conditional branch with no relocations: the less-than path preserves the first argument in `x0`, while the other path moves `x1` into the return register. Reinspection after the explicit-CFG migration produced the same code and object sizes, symbol, instruction sequence, and empty relocation table. The callback remains a linker-resolved external import and uses position-independent GOT addressing; its body is 32 bytes. This confirms the intended arithmetic, control-flow, and export/import lowering boundaries. It does not evaluate loop lowering, block parameters, loop-carried values, allocation, unwind behavior, or other targets.

The complete commands and captured `file`, `nm`, and `otool` output are preserved in the [add/callback transcript](../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md), [pre-CFG minimum transcript](../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md), and [explicit-CFG minimum transcript](../benchmarks/inspection/2026-10-16-macos-aarch64-f6eb016-cfg-min.md).

A later clean [debug-information inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) regenerates the add, minimum, and host-callback objects from commit `013da31f9f0fb752aad5ec887f8cf5a5ecf1650d`. All retain external symbols, but none contains a `__debug_*` section, DWARF compile unit, line entry, source variable, parameter, or local. This records the current debug-quality floor rather than satisfying the backend criterion: source-span mapping, linked/dSYM output, unwind metadata, Windows CodeView/PDB, JIT debugger registration, and optimized stepping remain unimplemented or unmeasured.

## Interpretation and limits

- The harness checks VM, JIT, and direct Rust results for equivalence before timing, including both outcomes of minimum, the immediate-return/loop paths of countdown and the accumulator, and the immediate-return/add/skip paths of conditional sum.
- The two-value accumulator carries a descending counter and wrapping sum around the same back-edge. Its direct, VM, and JIT medians are 1.705 ns, 1.651 us, and 12.750 ns. This bounded MIR workload is useful differential evidence, not a representative program benchmark.
- The conditional-sum prototype includes both branch outcomes and the immediate-return path. Its direct, VM, and JIT medians are 5.605 ns, 3.039 us, and 20.172 ns. This is broader CFG evidence, not a representative program benchmark.
- The single-allocation host-callback medians are 11.857 ns direct, 187.240 ns through the VM, and 13.471 ns through JIT. The three rows have 2.064%, 0.699%, and 29.399% RSD; the JIT maximum is 37.373 ns beyond its 14.550 ns p95. This supports a narrow boundary-cost comparison, but one `Box<i64>` round trip and one outlier-sensitive row are not a representative allocator or memory workload.
- The 64-allocation-burst medians are 768.283 ns direct, 946.601 ns through the VM, and 771.625 ns through JIT. VM minus direct is 178.318 ns (23.210%), while JIT minus direct is 3.342 ns (0.435%). The three rows' 0.290%, 0.497%, and 0.369% RSD provide stable evidence for this run but do not establish a general boundary-overhead budget.
- Schema 12 profiles the same generic callback workloads outside the timed loops. The single callback records one allocation, 8 total and peak live payload bytes, and zero live bytes afterward; the burst records 64 allocations, 512 total and peak live payload bytes, and zero afterward. The scope is deliberately `host_callback_requested_payload_bytes`: it confirms logical payload lifetime and cleanup, but excludes allocator metadata and fragmentation, stacks, generated code, VM/JIT internal allocations, and process RSS.
- The VM rows include bytecode verification and register allocation on every invocation. They measure the current end-to-end embedding API, not only interpreter dispatch.
- The JIT rows include the checked Rust invocation wrapper. The direct Rust rows are narrow baselines, not predictions for representative programs.
- The object-emission rows include host ISA construction, MIR lowering, compilation, and object serialization. Schema 2's system-link row covers the harness, add, and minimum objects; schema 3 adds countdown, schema 6 adds the accumulator, and schema 7 adds conditional sum. All precompile the C harness, then time the platform driver and system linker (`cc` on macOS or `link.exe` on Windows MSVC).
- Schema 4's process-startup row includes process creation, loader work, the tiny harness workload, output collection, and waiting for termination. It neither flushes operating-system caches nor isolates loader time, and the executable-size field includes the platform C harness and default linker output.
- Schema 5 retains the single-launch row and adds a configurable batched row. The batched per-operation value reduces the leverage of one delayed launch on a sample, but it is an amortized throughput estimate rather than a replacement for individual startup latency.
- JIT add and minimum compilation have 78.766% and 20.127% RSD. Conditional-sum JIT/AOT medians are 40.833 us and 35.791 us with 8.694% and 3.082% RSD. These values are useful as directional evidence, but compile-time variation remains too high for a stable performance budget.
- The single-launch AOT startup median is 1.527 ms and p95 is 2.446 ms, but RSD is 270.642%. The ten-launch batched row reports a 1.678 ms median with 1.890 ms p95 and 6.180% RSD. Batching materially reduces host-noise leverage for an amortized estimate, while the single row still exposes tail-sensitive latency.
- Alternating between a 64-iteration back-edge and immediate return gives countdown JIT/VM medians of 12.799 ns and 1.606 us and accumulator JIT/VM medians of 12.750 ns and 1.651 us. Alternating conditional-sum inputs `(64, 33)` and `(-7, 33)` gives JIT/VM medians of 20.172 ns and 3.039 us. These bounded microbenchmarks do not predict representative control-flow performance. Near-one-nanosecond direct add, countdown, and host-call rows remain timer-resolution baselines, not credible throughput claims.
- This baseline supports continuing to use Cranelift for the prototype while broader evidence is collected. It does not complete Milestone 0's backend decision.

Before selecting the initial native backend, record clean schema 13 results on macOS AArch64 and Windows x86-64, add broader allocation/memory and additional CFG workloads, isolate allocator overhead, and implement and validate native debug metadata across both tier-1 targets. The linked-harness peak-memory row is a process envelope rather than an allocation-workload measurement; the 64-allocation burst is more representative than one box but remains a synthetic host callback. The batched startup row is suitable for directional amortized evidence, not a tail-latency guarantee, and executable-size results are harness artifacts rather than release packaging budgets.

## Earlier baseline

The [schema 11 result](../benchmarks/results/2026-10-16-macos-aarch64-664edb9.json), measured 2026-10-16 11:50:37 JST from commit `664edb9d3a986614a8160d4d4adf9c1f0a8bb7bd`, predates requested-payload heap profiles and is superseded by schema 12. The [schema 10 result](../benchmarks/results/2026-10-16-macos-aarch64-133172b.json), [schema 9 result](../benchmarks/results/2026-10-16-macos-aarch64-e390a5d.json), [schema 8 result](../benchmarks/results/2026-10-16-macos-aarch64-6034dc8.json), [schema 7 result](../benchmarks/results/2026-10-16-macos-aarch64-0865167.json), [schema 6 result](../benchmarks/results/2026-10-16-macos-aarch64-bad17cf.json), [schema 5 result](../benchmarks/results/2026-10-16-macos-aarch64-180af85.json), [schema 4 result](../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json), [schema 3 result](../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json), [schema 2 result](../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json), [schema 1 AOT/link result](../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json), and [earlier JIT-only result](../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json) remain available for harness history.

## Sources

- [Current raw macOS AArch64 schema 12 result](../benchmarks/results/2026-10-16-macos-aarch64-fa9a69b.json), measured 2026-10-16 12:24:45 JST.
- `crates/vm/src/lib.rs`, deadline-enabled VM resource controls, last updated 2026-10-16 11:20 JST.
- `src/lib.rs`, 64-allocation burst callback prototype, last updated 2026-10-16 11:42 JST.
- `src/bin/prototype-bench.rs`, schema 13 linked-AOT process peak-memory harness, last updated 2026-10-16 12:50 JST.
- [macOS AArch64 generated-object inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md), captured 2026-10-16 00:25 JST.
- [macOS AArch64 signed-minimum inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md), captured 2026-10-16 00:43:31 JST.
- [macOS AArch64 AOT debug-information inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md), captured 2026-10-16 12:06:01 JST.
- Repository commit `0865167bfe75331a29e2d4ee9e509c9d17b4b4f1`, committed 2026-10-16.
- Repository commit `bad17cf2a42a5fc92b83f1aaebdc68e3ca2cb7a6`, committed 2026-10-16.
- Repository commit `180af8502341c4ed00d90a658193cfa14b5ce1da`, committed 2026-10-16.
- Repository commit `78fd3edff8d5dbeb38ff501999f7094a604404bd`, committed 2026-10-16.
- Repository commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3`, committed 2026-10-16.
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`, committed 2026-10-16.
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`, committed 2026-10-16.
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`, committed 2026-10-16.
- Rust compiler metadata recorded in the raw result; compiler commit date 2026-09-28.

- macOS `time(1)` and `getrusage(2)` manual pages installed with macOS 27.0.1; no publication/update date is exposed, verified 2026-10-16. The latter specifies `ru_maxrss` in bytes.
- [.NET `System.Diagnostics.Process` API documentation](https://github.com/dotnet/dotnet-api-docs/blob/cfbdb94f711c6122622fd1f9cacc91bb6ee53e3c/xml/System.Diagnostics/Process.xml), updated 2026-10-15 02:27:32 UTC. It defines `PeakWorkingSet64` as the maximum physical working-set memory used since process start, in bytes, including shared and private pages.
