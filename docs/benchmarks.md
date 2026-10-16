<!-- canonical-language: en; japanese-translation: ja/benchmarks.md -->

# Prototype benchmark evidence

[日本語版](ja/benchmarks.md)

These measurements are design evidence for the pre-alpha prototypes. They are not language performance guarantees, release targets, or a final backend selection.

## Current macOS AArch64 baseline

The current baseline was recorded from clean commit `bad17cf2a42a5fc92b83f1aaebdc68e3ca2cb7a6` on an Apple M1 Max running macOS 27.0.1 at 2026-10-16 09:37:56 JST. It used harness schema 6, the pinned Rust 1.99.0 toolchain, and Apple Clang 21.0.0 in a release build.

```bash
target/release/prototype-bench \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000 \
  --startup-iterations 10
```

Each row reports nanoseconds per operation. RSD is the relative population standard deviation across 31 samples.

| Measurement | Operations per sample | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering: wrapping `i64` add | 1,000 | 387.375 ns | 457.750 ns | 5.394% |
| Bytecode lowering: signed minimum | 1,000 | 1.086 us | 1.128 us | 2.473% |
| Bytecode lowering: countdown back-edge | 1,000 | 1.711 us | 1.885 us | 4.356% |
| Bytecode lowering: two-value accumulator | 1,000 | 1.781 us | 1.833 us | 1.164% |
| Cranelift JIT compilation: wrapping `i64` add | 1 | 21.583 us | 113.000 us | 85.387% |
| Cranelift JIT compilation: signed minimum | 1 | 24.083 us | 30.458 us | 24.956% |
| Cranelift JIT compilation: countdown back-edge | 1 | 28.167 us | 32.334 us | 16.625% |
| Cranelift JIT compilation: two-value accumulator | 1 | 31.292 us | 37.458 us | 8.464% |
| Cranelift AOT object emission: wrapping `i64` add | 1 | 13.625 us | 21.833 us | 31.397% |
| Cranelift AOT object emission: signed minimum | 1 | 18.167 us | 20.459 us | 7.174% |
| Cranelift AOT object emission: countdown back-edge | 1 | 22.208 us | 26.750 us | 6.206% |
| Cranelift AOT object emission: two-value accumulator | 1 | 26.334 us | 28.959 us | 4.174% |
| macOS system link: precompiled C harness plus add, minimum, countdown, and accumulator objects | 1 | 36.834 ms | 40.020 ms | 3.516% |
| macOS AOT process startup: single launch | 1 | 1.486 ms | 2.005 ms | 263.849% |
| macOS AOT process startup: ten-launch batch, per launch | 10 | 1.415 ms | 1.638 ms | 5.510% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.930 ns | 0.982 ns | 1.913% |
| VM wrapping `i64` add call | 100,000 | 177.218 ns | 178.999 ns | 0.573% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 1.994 ns | 2.557% |
| Direct Rust signed minimum call | 100,000 | 0.931 ns | 0.992 ns | 2.091% |
| VM signed minimum call | 100,000 | 490.746 ns | 495.128 ns | 0.425% |
| JIT signed minimum call | 100,000 | 2.634 ns | 2.782 ns | 5.347% |
| Direct Rust countdown call | 100,000 | 0.930 ns | 0.958 ns | 1.154% |
| VM countdown call | 100,000 | 1.562 us | 1.651 us | 1.855% |
| JIT countdown call | 100,000 | 12.730 ns | 13.217 ns | 1.276% |
| Direct Rust two-value accumulator call | 100,000 | 1.704 ns | 1.732 ns | 0.989% |
| VM two-value accumulator call | 100,000 | 1.631 us | 1.640 us | 0.302% |
| JIT two-value accumulator call | 100,000 | 12.782 ns | 13.800 ns | 2.843% |
| Direct Rust host callback | 100,000 | 0.930 ns | 1.048 ns | 10.285% |
| VM host callback | 100,000 | 170.679 ns | 176.610 ns | 1.245% |
| JIT host callback | 100,000 | 2.789 ns | 2.920 ns | 1.465% |

Cranelift's compiled code buffer contained 8 bytes for add, 20 bytes for minimum, 20 bytes for countdown, 36 bytes for the accumulator, and 40 bytes for the host callback. The emitted add, minimum, countdown, and accumulator objects were 352, 368, 376, and 392 bytes. The linked AOT harness was 17,056 bytes. JIT code sizes exclude object-file structure, relocation data, unwind/debug information, runtime support, and linker output. Object sizes include Mach-O structure but not the C harness or final executable; the executable size includes both plus the platform linker's defaults, but not a complete runtime distribution.

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

The schema 6 harness records the accumulator lowering/compilation/runtime rows, the renamed system-link and process-startup rows, and `executable_file_bytes.aot_add_min_countdown_sum_to` on Windows x86-64 MSVC. It compiles the C harness with `cl.exe`, discovers the tier-1 `link.exe` through `LinkTarget`, and validates one link and execution before timing. The Windows Server 2025 workflow runs the 31/1,000/100,000/10 parameters, validates schema 6, clean-checkout metadata, the `windows`/`x86_64` identity, accumulator and platform measurements, the startup batch size, and a positive executable size, then uploads the JSON for 30 days.

This is an evidence-producing path, not recorded Windows evidence: no remote runner result from this change has been inspected or committed. The macOS table above therefore remains the only current measured baseline.

## Generated-code inspection

The add and host-callback objects were regenerated from clean commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad` and inspected on the macOS AArch64 host on 2026-10-16 at 00:25 JST. The signed-minimum object was first regenerated from clean commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483` at 00:43 JST, then regenerated after the explicit-CFG migration from clean commit `f6eb016798c237412b59057fa32257ca2bc5d016` at 01:08 JST.

| Prototype | Export | Import | Generated body | Relocations |
| --- | --- | --- | --- | --- |
| Wrapping `i64` add | `_prototype_add` | None | `add x0, x0, x1`; `ret` | None |
| Signed `i64` minimum | `_prototype_min` | None | `cmp`; conditional `b.lt`; else-side `mov`; two `ret` paths | None |
| Typed host callback | `_prototype_host_callback` | `_lang_host_i64_0` | Pointer-authenticated frame, GOT load, indirect `blr`, authenticated return | `GOTLDP` and `GOTLDPOFF` against `_lang_host_i64_0` |

All three files are 64-bit ARM64 Mach-O objects. The add prototype lowers to an eight-byte, two-instruction body without relocations. Minimum lowers to a 20-byte compare and real conditional branch with no relocations: the less-than path preserves the first argument in `x0`, while the other path moves `x1` into the return register. Reinspection after the explicit-CFG migration produced the same code and object sizes, symbol, instruction sequence, and empty relocation table. The callback remains a linker-resolved external import and uses position-independent GOT addressing; its body is 32 bytes. This confirms the intended arithmetic, control-flow, and export/import lowering boundaries. It does not evaluate loop lowering, block parameters, loop-carried values, allocation, debug metadata, unwind behavior, or other targets.

The complete commands and captured `file`, `nm`, and `otool` output are preserved in the [add/callback transcript](../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md), [pre-CFG minimum transcript](../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md), and [explicit-CFG minimum transcript](../benchmarks/inspection/2026-10-16-macos-aarch64-f6eb016-cfg-min.md).

## Interpretation and limits

- The harness checks VM, JIT, and direct Rust results for equivalence before timing, including both outcomes of minimum and the immediate-return/loop paths of countdown and the accumulator.
- The two-value accumulator carries a descending counter and wrapping sum around the same back-edge. Its direct, VM, and JIT medians are 1.704 ns, 1.631 us, and 12.782 ns. This bounded MIR workload is useful differential evidence, not a representative program benchmark.
- A post-schema-6 conditional-sum prototype adds a branch and add/skip merge inside the same bounded loop. It is covered by differential and AOT integration tests but is not present in this schema 6 baseline; benchmark claims for it remain open.
- The VM rows include bytecode verification and register allocation on every invocation. They measure the current end-to-end embedding API, not only interpreter dispatch.
- The JIT rows include the checked Rust invocation wrapper. The direct Rust rows are narrow baselines, not predictions for representative programs.
- The object-emission rows include host ISA construction, MIR lowering, compilation, and object serialization. Schema 2's system-link row covers the harness, add, and minimum objects; schema 3 adds countdown and schema 6 adds the accumulator. All precompile the C harness, then time the platform driver and system linker (`cc` on macOS or `link.exe` on Windows MSVC).
- Schema 4's process-startup row includes process creation, loader work, the tiny harness workload, output collection, and waiting for termination. It neither flushes operating-system caches nor isolates loader time, and the executable-size field includes the platform C harness and default linker output.
- Schema 5 retains the single-launch row and adds a configurable batched row. The batched per-operation value reduces the leverage of one delayed launch on a sample, but it is an amortized throughput estimate rather than a replacement for individual startup latency.
- JIT add compilation has 85.387% RSD, and AOT add emission has 31.397% RSD. Countdown JIT/AOT medians are 28.167 us and 22.208 us; accumulator JIT/AOT medians are 31.292 us and 26.334 us. These values are useful as directional evidence, but compile-time variation remains too high for a stable performance budget.
- The single-launch AOT startup median is 1.486 ms and p95 is 2.005 ms, but one 45.908 ms sample raises RSD to 263.849%. The ten-launch batched row reports a 1.415 ms median with 1.638 ms p95 and 5.510% RSD. Batching materially reduces host-noise leverage for an amortized estimate, while the single row still exposes tail-sensitive latency.
- Alternating between a 64-iteration back-edge and immediate return gives countdown JIT/VM medians of 12.730 ns and 1.562 us and accumulator JIT/VM medians of 12.782 ns and 1.631 us. These bounded microbenchmarks do not predict representative control-flow performance. Sub-nanosecond direct-call rows remain timer-resolution baselines, not credible throughput claims.
- This baseline supports continuing to use Cranelift for the prototype while broader evidence is collected. It does not complete Milestone 0's backend decision.

Before selecting the initial native backend, record schema 6 on Windows x86-64, add allocation and additional CFG workloads, and measure memory and debug quality. The batched startup row is suitable for directional amortized evidence, not a tail-latency guarantee, and executable-size results are harness artifacts rather than release packaging budgets.

## Earlier baseline

The [schema 5 result](../benchmarks/results/2026-10-16-macos-aarch64-180af85.json), measured 2026-10-16 04:50:50 JST from commit `180af8502341c4ed00d90a658193cfa14b5ce1da`, predates accumulator evidence and is superseded by schema 6. The [schema 4 result](../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json), [schema 3 result](../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json), [schema 2 result](../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json), [schema 1 AOT/link result](../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json), and [earlier JIT-only result](../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json) remain available for harness history.

## Sources

- [Current raw macOS AArch64 schema 6 result](../benchmarks/results/2026-10-16-macos-aarch64-bad17cf.json), measured 2026-10-16 09:37:56 JST.
- [macOS AArch64 generated-object inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md), captured 2026-10-16 00:25 JST.
- [macOS AArch64 signed-minimum inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md), captured 2026-10-16 00:43:31 JST.
- Repository commit `bad17cf2a42a5fc92b83f1aaebdc68e3ca2cb7a6`, committed 2026-10-16.
- Repository commit `180af8502341c4ed00d90a658193cfa14b5ce1da`, committed 2026-10-16.
- Repository commit `78fd3edff8d5dbeb38ff501999f7094a604404bd`, committed 2026-10-16.
- Repository commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3`, committed 2026-10-16.
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`, committed 2026-10-16.
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`, committed 2026-10-16.
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`, committed 2026-10-16.
- Rust compiler metadata recorded in the raw result; compiler commit date 2026-09-28.

No external web sources were used for these locally measured baselines.
