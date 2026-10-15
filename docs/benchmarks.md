<!-- canonical-language: en; japanese-translation: ja/benchmarks.md -->

# Prototype benchmark evidence

[日本語版](ja/benchmarks.md)

These measurements are design evidence for the pre-alpha prototypes. They are not language performance guarantees, release targets, or a final backend selection.

## 2026-10-16 macOS AArch64 baseline

The first baseline was recorded from clean commit `b55848cfd5d24678f09eb00a9edda987dc25b257` on an Apple M1 Max running macOS 27.0.1. It used the pinned Rust 1.99.0 toolchain in a release build.

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000
```

Each row reports nanoseconds per operation. RSD is the relative population standard deviation across 31 samples.

| Measurement | Operations per sample | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering: wrapping `i64` add | 1,000 | 37.666 ns | 38.083 ns | 9.368% |
| Cranelift JIT compilation: wrapping `i64` add | 1 | 29.791 us | 38.625 us | 19.754% |
| Direct Rust wrapping `i64` add call | 100,000 | 1.713 ns | 1.778 ns | 8.927% |
| VM wrapping `i64` add call | 100,000 | 38.105 ns | 55.598 ns | 14.605% |
| JIT wrapping `i64` add call | 100,000 | 1.860 ns | 1.910 ns | 1.873% |
| Direct Rust host callback | 100,000 | 0.959 ns | 1.023 ns | 7.332% |
| VM host callback | 100,000 | 36.919 ns | 38.520 ns | 1.811% |
| JIT host callback | 100,000 | 4.058 ns | 4.369 ns | 2.974% |

Cranelift's compiled code buffer contained 8 bytes for the add function and 40 bytes for the host-callback function. Those sizes exclude object-file structure, relocation data, unwind/debug information, runtime support, and linker output.

## Interpretation and limits

- The harness checks VM, JIT, and direct Rust results for equivalence before timing.
- The VM rows include bytecode verification and register allocation on every invocation. They measure the current end-to-end embedding API, not only interpreter dispatch.
- The JIT rows include the checked Rust invocation wrapper. The direct Rust rows are narrow baselines, not predictions for representative programs.
- The JIT compile result has 19.754% RSD and the VM add result has 14.605% RSD. Their medians are useful for an initial baseline, but the variation is too high for a stable performance budget.
- This one-instruction microbenchmark supports continuing to use Cranelift for the prototype while broader evidence is collected. It does not complete Milestone 0's backend decision.

Before selecting the initial native backend, repeat the same schema on Windows x86-64, add representative control-flow and allocation workloads, inspect generated code, and measure startup, memory, and debug quality. The benchmark harness now supports AOT object-emission and macOS system-link measurements, but this baseline predates those series and must be superseded by a clean result before they can inform the backend decision.

## Sources

- [Raw macOS AArch64 result](../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json), measured 2026-10-16 00:05:30 JST.
- Repository commit `b55848cfd5d24678f09eb00a9edda987dc25b257`, committed 2026-10-16.
- Rust compiler metadata recorded in the raw result; compiler commit date 2026-09-28.

No external web sources were used for this locally measured baseline.
