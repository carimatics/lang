<!-- canonical-language: en; japanese-translation: ja/benchmarks.md -->

# Prototype benchmark evidence

[日本語版](ja/benchmarks.md)

These measurements are design evidence for the pre-alpha prototypes. They are not language performance guarantees, release targets, or a final backend selection.

## Current macOS AArch64 baseline

The current baseline was recorded from clean commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3` on an Apple M1 Max running macOS 27.0.1 at 2026-10-16 02:27:40 JST. It used harness schema 3, the pinned Rust 1.99.0 toolchain, and Apple Clang 21.0.0 in a release build.

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000
```

Each row reports nanoseconds per operation. RSD is the relative population standard deviation across 31 samples.

| Measurement | Operations per sample | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering: wrapping `i64` add | 1,000 | 452.917 ns | 537.375 ns | 10.463% |
| Bytecode lowering: signed minimum | 1,000 | 1,105.917 ns | 1,218.792 ns | 5.965% |
| Bytecode lowering: countdown back-edge | 1,000 | 1,659.458 ns | 1,738.083 ns | 2.702% |
| Cranelift JIT compilation: wrapping `i64` add | 1 | 19.375 us | 32.917 us | 72.237% |
| Cranelift JIT compilation: signed minimum | 1 | 23.792 us | 28.250 us | 16.980% |
| Cranelift JIT compilation: countdown back-edge | 1 | 27.541 us | 33.750 us | 11.217% |
| Cranelift AOT object emission: wrapping `i64` add | 1 | 13.375 us | 16.916 us | 19.445% |
| Cranelift AOT object emission: signed minimum | 1 | 17.834 us | 19.542 us | 3.779% |
| Cranelift AOT object emission: countdown back-edge | 1 | 21.625 us | 25.667 us | 5.546% |
| macOS system link: precompiled C harness plus add, minimum, and countdown objects | 1 | 37.622 ms | 39.545 ms | 3.051% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.961 ns | 1.206 ns | 7.807% |
| VM wrapping `i64` add call | 100,000 | 178.762 ns | 182.893 ns | 0.867% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 1.864 ns | 0.206% |
| Direct Rust signed minimum call | 100,000 | 0.930 ns | 1.214 ns | 8.554% |
| VM signed minimum call | 100,000 | 489.329 ns | 493.225 ns | 0.380% |
| JIT signed minimum call | 100,000 | 2.634 ns | 2.750 ns | 1.180% |
| Direct Rust countdown call | 100,000 | 0.930 ns | 0.931 ns | 0.048% |
| VM countdown call | 100,000 | 1,558.118 ns | 1,567.146 ns | 0.396% |
| JIT countdown call | 100,000 | 12.729 ns | 12.992 ns | 0.655% |
| Direct Rust host callback | 100,000 | 0.930 ns | 0.930 ns | 0.544% |
| VM host callback | 100,000 | 171.447 ns | 173.440 ns | 0.532% |
| JIT host callback | 100,000 | 2.789 ns | 2.818 ns | 1.424% |

Cranelift's compiled code buffer contained 8 bytes for add, 20 bytes for minimum, 20 bytes for countdown, and 40 bytes for the host callback. The emitted add, minimum, and countdown objects were 352, 368, and 376 bytes. JIT code sizes exclude object-file structure, relocation data, unwind/debug information, runtime support, and linker output. Object sizes include Mach-O structure but not the C harness or final executable.

## Schema 4 format and migration

Schema 4 adds `executable_file_bytes.aot_add_min_countdown` and the `aot_process_startup_add_min_countdown` measurement. The executable-size field is a positive byte count on the initial tier-1 hosts and `null` elsewhere. Consumers must branch on `schema_version`; existing schema 1–3 JSON files remain immutable evidence.

The startup row measures repeated process creation, dynamic loading, execution of the add, minimum, and countdown checks, output collection, and process termination. The harness validates one link and execution before timing and does not flush operating-system caches, so this is a warm-cache process-startup measurement rather than cold-boot latency. The executable includes the precompiled C harness and platform linker defaults; it is not a complete runtime distribution or installation-size measurement.

The schema 4 implementation and Windows evidence path are present, but no clean schema 4 result is recorded yet. The schema 3 table above remains the current measured baseline.

## Schema 3 format and migration

Schema 3 adds a bounded countdown workload to bytecode lowering, JIT compilation, AOT object emission, direct/VM/JIT calls, artifact sizes, and the system-link set. Runtime samples alternate between input `64`, which carries an `i64` block parameter around a back-edge until it reaches zero, and `-7`, which takes the immediate-return path. The harness checks `-7`, `0`, `1`, and `64` for direct/VM/JIT equivalence before timing.

Schema 3 adds `jit_countdown` and `aot_countdown` size fields plus six countdown measurement rows. It renames `system_link_add_min` to `system_link_add_min_countdown`; consumers must branch on `schema_version` rather than merging these names. Existing schema 1 and 2 JSON files remain immutable historical evidence.

## Windows x86-64 evidence path

The schema 4 harness records `system_link_add_min_countdown`, `aot_process_startup_add_min_countdown`, and `executable_file_bytes.aot_add_min_countdown` on Windows x86-64 MSVC. It compiles the C harness with `cl.exe`, discovers the tier-1 `link.exe` through `LinkTarget`, and validates one link and execution before timing. The Windows Server 2025 workflow runs the same 31/1,000/100,000 parameters, validates schema 4, clean-checkout metadata, the `windows`/`x86_64` identity, both platform measurements, and a positive executable size, then uploads the JSON for 30 days.

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

- The harness checks VM, JIT, and direct Rust results for equivalence before timing, including both outcomes of minimum and the immediate-return/loop paths of countdown.
- The VM rows include bytecode verification and register allocation on every invocation. They measure the current end-to-end embedding API, not only interpreter dispatch.
- The JIT rows include the checked Rust invocation wrapper. The direct Rust rows are narrow baselines, not predictions for representative programs.
- The object-emission rows include host ISA construction, MIR lowering, compilation, and object serialization. Schema 2's system-link row covers the harness, add, and minimum objects; schema 3 adds countdown. Both precompile the C harness, then time the platform driver and system linker (`cc` on macOS or `link.exe` on Windows MSVC).
- Schema 4's process-startup row includes process creation, loader work, the tiny harness workload, output collection, and waiting for termination. It neither flushes operating-system caches nor isolates loader time, and the executable-size field includes the platform C harness and default linker output.
- JIT add compilation has 72.237% RSD because one sample reached 116.333 us, while its median was 19.375 us. The countdown JIT and AOT medians are 27.541 us and 21.625 us with 11.217% and 5.546% RSD. These medians are useful as directional evidence, but the add compile-time variation is too high for a stable performance budget.
- VM/JIT runtime rows stayed below 1.5% RSD. Alternating between a 64-iteration back-edge and immediate return raises the JIT median to 12.729 ns and the VM median to 1,558.118 ns; this bounded microbenchmark does not predict representative control-flow performance. Sub-nanosecond direct-call rows are timer-resolution baselines, not credible throughput claims.
- This baseline supports continuing to use Cranelift for the prototype while broader evidence is collected. It does not complete Milestone 0's backend decision.

Before selecting the initial native backend, record schema 4 on both tier-1 hosts, add allocation and broader CFG workloads, and measure memory and debug quality. Schema 4 instruments process startup and linked executable size, but those dimensions remain unproven until clean results are recorded and interpreted.

## Earlier baseline

The [schema 2 result](../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json), measured 2026-10-16 00:39:25 JST from commit `fc86889fed093a910d575ac47eca66239a7b47e8`, predates countdown and is superseded by schema 3. The [schema 1 AOT/link result](../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json) and [earlier JIT-only result](../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json) predate conditional control flow. All remain available for harness history.

## Sources

- [Current raw macOS AArch64 schema 3 result](../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json), measured 2026-10-16 02:27:40 JST.
- [macOS AArch64 generated-object inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md), captured 2026-10-16 00:25 JST.
- [macOS AArch64 signed-minimum inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md), captured 2026-10-16 00:43:31 JST.
- Repository commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3`, committed 2026-10-16.
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`, committed 2026-10-16.
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`, committed 2026-10-16.
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`, committed 2026-10-16.
- Rust compiler metadata recorded in the raw result; compiler commit date 2026-09-28.

No external web sources were used for these locally measured baselines.
