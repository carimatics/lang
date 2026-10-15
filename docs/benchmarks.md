<!-- canonical-language: en; japanese-translation: ja/benchmarks.md -->

# Prototype benchmark evidence

[日本語版](ja/benchmarks.md)

These measurements are design evidence for the pre-alpha prototypes. They are not language performance guarantees, release targets, or a final backend selection.

## Current macOS AArch64 baseline

The current baseline was recorded from clean commit `fc86889fed093a910d575ac47eca66239a7b47e8` on an Apple M1 Max running macOS 27.0.1 at 2026-10-16 00:39:25 JST. It used harness schema 2, the pinned Rust 1.99.0 toolchain, and Apple Clang 21.0.0 in a release build.

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000
```

Each row reports nanoseconds per operation. RSD is the relative population standard deviation across 31 samples.

| Measurement | Operations per sample | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering: wrapping `i64` add | 1,000 | 86.166 ns | 87.417 ns | 7.638% |
| Bytecode lowering: signed minimum | 1,000 | 76.000 ns | 77.000 ns | 0.535% |
| Cranelift JIT compilation: wrapping `i64` add | 1 | 22.583 us | 36.833 us | 57.043% |
| Cranelift JIT compilation: signed minimum | 1 | 26.167 us | 32.208 us | 15.909% |
| Cranelift AOT object emission: wrapping `i64` add | 1 | 16.041 us | 22.709 us | 15.231% |
| Cranelift AOT object emission: signed minimum | 1 | 23.042 us | 63.083 us | 50.846% |
| macOS system link: precompiled C harness plus add and minimum objects | 1 | 38.585 ms | 39.743 ms | 2.604% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.932 ns | 0.961 ns | 1.286% |
| VM wrapping `i64` add call | 100,000 | 42.638 ns | 44.422 ns | 1.807% |
| JIT wrapping `i64` add call | 100,000 | 1.867 ns | 1.932 ns | 1.500% |
| Direct Rust signed minimum call | 100,000 | 0.932 ns | 0.965 ns | 1.195% |
| VM signed minimum call | 100,000 | 42.483 ns | 44.651 ns | 1.777% |
| JIT signed minimum call | 100,000 | 2.664 ns | 2.852 ns | 2.946% |
| Direct Rust host callback | 100,000 | 0.932 ns | 0.979 ns | 1.311% |
| VM host callback | 100,000 | 40.401 ns | 41.765 ns | 2.028% |
| JIT host callback | 100,000 | 2.820 ns | 2.880 ns | 1.563% |

Cranelift's compiled code buffer contained 8 bytes for add, 20 bytes for minimum, and 40 bytes for the host callback. The emitted add and minimum objects were 352 and 368 bytes. JIT code sizes exclude object-file structure, relocation data, unwind/debug information, runtime support, and linker output. Object sizes include Mach-O structure but not the C harness or final executable.

## Windows x86-64 evidence path

The schema 2 harness now records the same `system_link_add_min` row on Windows x86-64 MSVC. It compiles the C harness with `cl.exe`, discovers the tier-1 `link.exe` through `LinkTarget`, validates one link and execution before timing, and then measures only repeated links. The Windows Server 2025 workflow runs the same 31/1,000/100,000 parameters, validates schema 2, a clean checkout, the `windows`/`x86_64` identity, and presence of the system-link row, then uploads the JSON for 30 days.

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

- The harness checks VM, JIT, and direct Rust results for equivalence before timing, including both outcomes of the minimum function.
- The VM rows include bytecode verification and register allocation on every invocation. They measure the current end-to-end embedding API, not only interpreter dispatch.
- The JIT rows include the checked Rust invocation wrapper. The direct Rust rows are narrow baselines, not predictions for representative programs.
- The object-emission rows include host ISA construction, MIR lowering, compilation, and object serialization. The system-link row precompiles the C harness, then times the platform driver and system linker producing an executable from the harness, add, and minimum objects (`cc` on macOS or `link.exe` on Windows MSVC).
- JIT add compilation has 57.043% RSD because one sample reached 105.542 us, while its median was 22.583 us. Minimum AOT emission has 50.846% RSD, with an 89.667 us maximum and 23.042 us median. These medians are useful as directional evidence, but the compile-time variation is too high for a stable performance budget.
- Runtime rows stayed below 3% RSD. The conditional workload increases the JIT call median from 1.867 ns for add to 2.664 ns for minimum, but both are narrow microbenchmarks and do not predict representative program performance.
- This baseline supports continuing to use Cranelift for the prototype while broader evidence is collected. It does not complete Milestone 0's backend decision.

Before selecting the initial native backend, repeat schema 2 on Windows x86-64, add allocation and general CFG workloads, and measure startup, memory, executable size, and debug quality.

## Earlier baseline

The [schema 1 AOT/link result](../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json), measured 2026-10-16 00:20:21 JST from commit `0808ce75f128dfb16bf9c6e6453200fea7749401`, predates the conditional workload and is superseded by schema 2. The [earlier JIT-only result](../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json), measured 2026-10-16 00:05:30 JST from commit `b55848cfd5d24678f09eb00a9edda987dc25b257`, predates PIC object emission and system-link measurements. Both remain available for harness history.

## Sources

- [Current raw macOS AArch64 result](../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json), measured 2026-10-16 00:39:25 JST.
- [macOS AArch64 generated-object inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md), captured 2026-10-16 00:25 JST.
- [macOS AArch64 signed-minimum inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md), captured 2026-10-16 00:43:31 JST.
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`, committed 2026-10-16.
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`, committed 2026-10-16.
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`, committed 2026-10-16.
- Rust compiler metadata recorded in the raw result; compiler commit date 2026-09-28.

No external web sources were used for these locally measured baselines.
