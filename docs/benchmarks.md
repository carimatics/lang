<!-- canonical-language: en; japanese-translation: ja/benchmarks.md -->

# Prototype benchmark evidence

[日本語版](ja/benchmarks.md)

These measurements are design evidence for the pre-alpha prototypes. They are not language performance guarantees, release targets, or a final backend selection.

## Current macOS AArch64 baseline

The current baseline was recorded from clean commit `0808ce75f128dfb16bf9c6e6453200fea7749401` on an Apple M1 Max running macOS 27.0.1. It used the pinned Rust 1.99.0 toolchain and Apple Clang 21.0.0 in a release build.

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000
```

Each row reports nanoseconds per operation. RSD is the relative population standard deviation across 31 samples.

| Measurement | Operations per sample | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering: wrapping `i64` add | 1,000 | 35.250 ns | 42.458 ns | 11.394% |
| Cranelift JIT compilation: wrapping `i64` add | 1 | 20.583 us | 34.000 us | 66.169% |
| Cranelift AOT object emission: wrapping `i64` add | 1 | 15.792 us | 20.083 us | 13.802% |
| macOS system link: precompiled C harness plus AOT object | 1 | 38.031 ms | 39.093 ms | 1.665% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.988 ns | 0.990 ns | 1.425% |
| VM wrapping `i64` add call | 100,000 | 38.567 ns | 39.618 ns | 2.093% |
| JIT wrapping `i64` add call | 100,000 | 1.916 ns | 1.962 ns | 0.714% |
| Direct Rust host callback | 100,000 | 0.930 ns | 0.995 ns | 3.368% |
| VM host callback | 100,000 | 37.450 ns | 38.214 ns | 1.686% |
| JIT host callback | 100,000 | 2.874 ns | 2.928 ns | 0.751% |

Cranelift's compiled code buffer contained 8 bytes for the add function and 40 bytes for the host-callback function. The emitted add object was 352 bytes. The JIT code sizes exclude object-file structure, relocation data, unwind/debug information, runtime support, and linker output. The object size includes its Mach-O structure but not the C harness or final executable.

## Generated-code inspection

The two native object prototypes were regenerated from clean commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad` and inspected on the same macOS AArch64 host on 2026-10-16 at 00:25 JST.

| Prototype | Export | Import | Generated body | Relocations |
| --- | --- | --- | --- | --- |
| Wrapping `i64` add | `_prototype_add` | None | `add x0, x0, x1`; `ret` | None |
| Typed host callback | `_prototype_host_callback` | `_lang_host_i64_0` | Pointer-authenticated frame, GOT load, indirect `blr`, authenticated return | `GOTLDP` and `GOTLDPOFF` against `_lang_host_i64_0` |

Both files are 64-bit ARM64 Mach-O objects. The add prototype lowers to an eight-byte, two-instruction body without relocations. The callback remains a linker-resolved external import and uses position-independent GOT addressing; its body is 32 bytes. This confirms the intended export/import boundary and explains the callback's larger code size. It does not evaluate control flow, allocation, debug metadata, unwind behavior, or other targets.

The complete commands and captured `file`, `nm`, and `otool` output are preserved in the [raw inspection transcript](../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md).

## Interpretation and limits

- The harness checks VM, JIT, and direct Rust results for equivalence before timing.
- The VM rows include bytecode verification and register allocation on every invocation. They measure the current end-to-end embedding API, not only interpreter dispatch.
- The JIT rows include the checked Rust invocation wrapper. The direct Rust rows are narrow baselines, not predictions for representative programs.
- The object-emission row includes host ISA construction, MIR lowering, compilation, and object serialization. The system-link row precompiles the C harness, then times the `cc` driver and system linker producing an executable from two object files.
- The JIT compile result has 66.169% RSD because one sample reached 112.333 us, while its median was 20.583 us. The object-emission result has 13.802% RSD. These medians are useful for an initial baseline, but their variation is too high for a stable performance budget.
- This one-instruction microbenchmark supports continuing to use Cranelift for the prototype while broader evidence is collected. It does not complete Milestone 0's backend decision.

Before selecting the initial native backend, repeat the same schema on Windows x86-64, benchmark the implemented conditional-return workload, add allocation and broader control-flow workloads, and measure startup, memory, executable size, and debug quality.

## Earlier baseline

The [earlier JIT-only result](../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json), measured 2026-10-16 00:05:30 JST from commit `b55848cfd5d24678f09eb00a9edda987dc25b257`, remains available for harness history. It predates PIC object emission and system-link measurements and is superseded by the current baseline for design decisions.

## Sources

- [Current raw macOS AArch64 result](../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json), measured 2026-10-16 00:20:21 JST.
- [macOS AArch64 generated-object inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md), captured 2026-10-16 00:25 JST.
- Repository commit `0808ce75f128dfb16bf9c6e6453200fea7749401`, committed 2026-10-16.
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`, committed 2026-10-16.
- Rust compiler metadata recorded in the raw result; compiler commit date 2026-09-28.

No external web sources were used for these locally measured baselines.
