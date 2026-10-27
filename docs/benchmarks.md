<!-- canonical-language: en; japanese-translation: ja/benchmarks.md -->

# Benchmark evidence

[日本語版](ja/benchmarks.md)

These measurements are design evidence for the pre-alpha prototypes. They are not language performance guarantees, release targets, or a final backend selection.

## Milestone 3 concurrency and component entry costs

Protocol updated and locally checked: **2026-10-27 JST**. [M3 runner](../tools/m3-bench/run.py) builds the pinned toolchain's release examples from a clean committed tree and writes a new private `lang-m3-benchmark` version-1 JSON report. Existing M2 sources and historical reports remain unchanged.

```bash
python3 tools/m3-bench/run.py --output /tmp/m3-costs.json --samples 21
```

Use a fresh output file. The runner refuses dirty sources, an existing output, samples outside 1–1,001, incorrect results, inconsistent logical counters or incomplete retirement. It records the exact revision, source/fixture hashes, rustc identity, host, build/run commands and raw chronological samples. Each backend has three untimed warmups; summaries use min, upper median, nearest-rank p95 and max. There is no performance acceptance threshold.

Evidence validation checks the six distinct workload identities and source/oracle/hash, VM/JIT identity, schema/profile, integer samples/counters, summaries recomputed from raw samples, warmups and per-call retirement. `python3 -m unittest discover -s tools/m3-bench -p 'test_*.py'` accepts the saved release capture and refuses duplicated, substituted or corrupted evidence. Routine macOS/Windows CI runs both M2/M3 validators. These are evidence checks, not new CI performance measurements.

| Workload | Observable behavior | Expected result |
| --- | --- | --- |
| `single-task` | One async entry | 42 |
| `spawn-join` | Two spawned workers, yield and joins | 42 |
| `owned-channel` | Capacity-one owned Bytes producer/consumer | 42 |
| `mixed-cancellation` | Queued mixed ownership and blocked-send cancellation | 42 |
| `owned-buffers` | Owned Bytes/text cloning, sums and retirement | 71 |
| `service-state` | Actor messages and nested owned-state replacement | 42 |
| component boundary | Public C API → verified script → actual native increment → canonical result | 42 |

The task measurements alternate which backend runs first in each paired sample. Frontend analysis and one JIT compilation are observed separately, once per workload. The entry timer includes the actual VM/JIT API's **per-call preparation, execution and mandatory cleanup**; allocator creation, frontend/JIT compilation, validation and JSON formatting are outside it. The workload inputs are fixed, with finite default execution/allocator/runner bounds, service work capped at 1,000,000 and tracing disabled. Each entry checks its result, matching VM/JIT fuel/service/task/frame/cell/allocation counters, zero live tasks and zero managed bytes/allocations.

The component measurements reuse the explicit-grant plugin example in separate VM and JIT processes. They separately observe source-to-artifact and engine/module/provider/instance/binding admission, then time only `Api.call`. That interval includes per-call preparation, canonical adapters, script/native callback execution, mandatory cleanup and retained-result creation. **The current component JIT compiles inside each call**; it is not the task example's reused compiled entry. Control creation, input encoding, result inspection and result/control drop are outside the call timer. Every iteration returns 42 with stable counters and returns the allocator to its previous inventory after dropping results/controls. The original denial/panic/reuse/fuel checks still run; final native-owner release is exactly once and managed allocations are zero. Component VM runs before JIT; these processes are not interleaved.

These are finite workload entry observations on the recorded host, with warm caches. They do not isolate pure guest instructions, scheduler primitives or channel operations, and do not establish throughput, OS-thread parallelism, AOT or Windows performance. Managed heap counters exclude compiler metadata, native codec scratch and other unmetered host allocations; they are not process RSS. Single preparation observations do not estimate a distribution. Test-suite wall times are never benchmark samples. M2 source/Rust/C parity must also be replayed with its existing release-SDK harness; it is separate evidence, not an M3 speed comparison.

Sources: [task measurement example](../examples/m3-costs.rs), [component example](../examples/component-plugin-host.rs), [runner](../tools/m3-bench/run.py), and [RFC 0027](rfcs/0027-production-component-interface.md). Code has no published update date; inspected/executed **2026-10-27 JST**. RFC 0027 was updated **2026-10-27 JST**.

### 2026-10-27 macOS observation

Raw [M3 report](../benchmarks/results/2026-10-27-m3-macos-c9ffd19.json), verified **2026-10-27 13:39 JST**, clean source `c9ffd19`, Rust **1.99.0** (`b940084d7`), macOS **27.0.1**, arm64, 10 logical CPUs. Each row has **21 samples after 3 warmups**.

| Workload | VM median µs | JIT median µs | Peak managed bytes | Peak live tasks |
| --- | ---: | ---: | ---: | ---: |
| `spawn-join` | 50.250 | 50.250 | 192 | 3 |
| `owned-channel` | 173.792 | 172.250 | 6,354 | 3 |
| `mixed-cancellation` | 230.334 | 226.667 | 484,384 | 3 |
| `owned-buffers` | 57.750 | 56.625 | 6,764 | 2 |
| `service-state` | 210.917 | 208.500 | 24,322 | 2 |
| `single-task` | 8.416 | 8.292 | 16 | 1 |

All six workloads have identical logical counters in VM/JIT and retire to zero live tasks, allocations and bytes on every entry. The mixed-cancellation fixture has the largest requested managed peak here (484,384 bytes); that includes its admitted mixed-owned queue/retirement state. It is not a universal cancellation footprint. Frontend observations range **64–2,280 µs** and one-time task JIT compilation **529–13,758 µs**; these are single observations, not latency distributions.

| Public component call | Median µs | p95 µs | Peak managed bytes |
| --- | ---: | ---: | ---: |
| VM | 94.292 | 108.250 | 118 |
| JIT, including compilation inside each call | 1,267.583 | 1,730.458 | 118 |

Both component modes report **8 fuel / 60 service work / 1 host call / 19 peak managed allocations** per measured call. Each process performs 24 warmup/measured native calls plus the two original successful demo calls; all **26** calls complete, the owner releases once and final managed allocations are zero. The current per-call JIT preparation explains why these component entry measurements cover substantially more work than the reused task JIT entry. The close task medians do not establish backend instruction parity: both timers include common runtime preparation and retirement. Fixed small workloads, process order, cache state and lack of independent process repetitions limit generalization.

The [M2 parity replay](../benchmarks/results/2026-10-27-m2-parity-macos-c9ffd19.json), verified **2026-10-27 13:40 JST**, uses the same clean source and release SDK `da0d5af4177cbe6d46a037f9f213b5f0d850239975064edf6c9d549574bc0531`. All **21 source/Rust/C/oracle cases** and the unwind-retirement/fresh-context probe pass. It contains **no new M2 timing samples**; historical M2 results and workload/harness sources remain byte-for-byte unchanged.

```bash
python3 tools/build-native-sdk.py --release --output /tmp/m3-sdk
python3 tools/m2-bench/run.py --sdk /tmp/m3-sdk --output /tmp/m3-m2-parity --parity-only
```

These macOS observations complete Z01's bounded workload-cost record, not I08/B01's both-host execution, B02's C stabilization or Z02's final audit/guide. Sources and update-date limits are listed in the protocol above; the two raw reports contain exact verification timestamps.

## Milestone 2 source/Rust/C harness

Updated and verified: **2026-10-20 JST**. [`tools/m2-bench`](../tools/m2-bench/) uses the actual source compiler, original C-export object and installed release SDK production runtime. It is independent of the historical schema20 prototype below. Its private result format is `lang-m2-benchmark`, version 1.

Three workloads have the same bounded, nonnegative input and integer result in all implementations:

| Workload | Operations | Independent result check |
| --- | --- | --- |
| `scalar` | Addition, a data-dependent branch and a loop; all intermediates fit i64 | Python recurrence and fixed small cases |
| `buffers` | Grow Bytes/byte vector and Vec/i64 vector, then read and sum both | Closed sum, including the 250-byte value wrap |
| `regions` | Allocate shared payload, weak observer, retained owner and arena payload; read, retire, require weak expiry | `n * (n + 1)` |

Before timing, the harness requires **21 source/Rust/C/oracle matches** at `n = 0, 1, 7, 64, 250, 251, 1000`. It rejects missing, duplicate or incorrect results. It also forces Rust host unwind inside the live source context, checks allocator retirement, then successfully registers and calls a second context. These are correctness checks, not performance evidence.

```bash
python3 tools/build-native-sdk.py --release --output /tmp/m2-sdk
python3 -m unittest discover -s tools/m2-bench -p 'test_*.py'
python3 tools/m2-bench/run.py --sdk /tmp/m2-sdk --output /tmp/m2-parity --parity-only
python3 tools/m2-bench/run.py --sdk /tmp/m2-sdk --output /tmp/m2-measure --samples 15 --repeats 20 --input 1000
```

Use fresh output paths. Windows requires the x64 MSVC developer environment and `python` instead of `python3` when appropriate. The SDK must match the native tier-1 host and pinned rustc identity. `M2 source Rust C benchmark` CI runs parity on `codex/bench-parity-*`; `codex/bench-macos-*` and `codex/bench-windows-*` select one native measurement host. These branches do not trigger the older prototype or isolated-install workflows.

The parity executable contains all three implementations. **The measured source, Rust and C executables are separate**; reference executables link neither the language object nor its SDK/runtime. All three use the Rust measurement/JSON driver; `c` identifies the C workload implementation, and its startup/size includes that common driver. Rust host/reference code uses `-Copt-level=3`, edition 2024, unwind and no LTO; C uses `-O3` on macOS or `/O2 /MD` on MSVC. The source object uses the compiler's current Cranelift defaults (PIC, no explicit `opt_level` override). This measures the current implementation against optimized references; it does not claim equivalent optimizer maturity.

`preparation_ns` covers source record validation/decoding, artifact preparation/binding and C context creation, once per process. `elapsed_ns / repeats` covers calls through the reused artifact/context **including per-call runtime admission, preparation, allocation and retirement**, with three untimed warmups. The timer also includes the same C dispatch/status and checksum loop for each implementation. Inputs and outputs are observable; C allocation callbacks use volatile indirection, and Rust buffers/region owners pass through `black_box`.

Separate `startup` invocations perform no workload/warmup and report fresh-process wall time through initialization and clean exit; they run with warm OS/file caches. Parent wall times include process launch and output capture. Workload process wall times additionally include `/usr/bin/time` on macOS and are **not** startup latency. Memory is each workload child's macOS maximum RSS (`time -l`, bytes) or Windows `PeakWorkingSetSize` (K32), without subtracting a noisy baseline. Sizes are the complete linked executables and original source object; system dynamic libraries are not added to those file sizes.

Source calls have explicit finite limits: 256 MiB requested live memory, 1 GiB cumulative memory, 65,536 allocations, 8 frames, 65,536 cells, 100,000,000 fuel and 4,096 cleanup records; no host calls/import grants. The original provider counts requested bytes, allocations and releases, and must return to zero after context destruction. C uses the same tracked heap callbacks. Rust uses standard `Vec`, `Rc`/`Weak` and a lexical `Vec<Box<_>>` region, with RAII and required weak expiry; its heap counters are **not instrumented** (zero fields are not zero allocation claims). C uses geometric buffers, explicit single-thread reference counts and lexical region retirement. Policies, layouts, validation, reference-count synchronization and allocator-accounting overhead differ; only workload behavior is equivalent. The region test does not compare concurrent use or a general arena implementation.

Provider heap counters cover allocations routed through that guest/C provider; they do not account for all Rust runtime/preparation metadata or the whole process. Use the OS peak-memory samples for process comparisons.

Reports retain individual samples, min/median/max/mean/stdev, rotated implementation order, compiler commands/versions, SDK ID, source/harness hashes, clean/dirty revision, host, input and executable sizes. Timing runs verify every checksum and allocator balance. A local smoke run validates the harness; G17 still requires native measurements on **both** tier-1 hosts and a reproducible interpretation report (B02–B04).

B01 correctness evidence (verified **2026-10-20 JST**): [CI run 39382631742](https://github.com/carimatics/lang/actions/runs/39382631742) passes on both native hosts at clean `fffe86c`. The [macOS parity report](../benchmarks/results/2026-10-20-m2-parity-macos-fffe86c.json) and [Windows parity report](../benchmarks/results/2026-10-20-m2-parity-windows-fffe86c.json) contain 21 oracle-checked matches and successful unwind retirement each, matching release SDK IDs, verified harness hashes and separate reference link commands. Windows checkout newline differences are allowed when comparing canonical source hashes. **There are no timing/startup observations in these parity reports**; their input settings are planned measurement settings. B02–B04 remain open.

### macOS native capture (B02)

Verified **2026-10-20 JST**. [CI run 39383945072 / job 118499215009](https://github.com/carimatics/lang/actions/runs/39383945072/job/118499215009) succeeds at clean `ecc0fd1`. The [complete native report](../benchmarks/results/2026-10-20-m2-macos-ecc0fd1.json), recorded at **2026-10-20 07:03:45 UTC**, contains **135 workload observations and 45 dedicated startup observations**: 15 samples per case, `n=1000`, 20 calls per sample and three untimed warmups. The required 21-case parity/unwind gate passes first. Every checksum and allocator retirement, case/sample identity, aggregate, SDK ID, harness hash and reference link separation was rechecked from the saved data.

Host: **macOS 15.7.9, arm64, 5 logical CPUs, Python 3.14.7**. The processor field is only `arm`; the chip model was not recorded. Source uses release SDK `06794c1ec70d06992a0e9f7c5714d05d32e8e1b5c029beac78062fbe8a32bad1`. The harness and production/reference code are unchanged from B01. Flags, rustc/C compiler versions and commands are retained in the report; measurement boundaries remain those above.

| Workload | Implementation | Median µs/call | Runtime RSD | Artifact/context prep ms | Median RSS MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| `scalar` | source | 1484.383 | 4.68% | 1.554 | 27.328 |
| `scalar` | rust | 1.004 | 4.70% | — | 1.344 |
| `scalar` | c | 1.012 | 8.86% | — | 1.328 |
| `buffers` | source | 6661.158 | 1.76% | 1.521 | 27.469 |
| `buffers` | rust | 1.981 | 5.71% | — | 1.375 |
| `buffers` | c | 1.883 | 5.03% | — | 1.359 |
| `regions` | source | 18913.673 | 0.86% | 1.532 | 33.766 |
| `regions` | rust | 44.850 | 4.47% | — | 1.344 |
| `regions` | c | 30.492 | 4.67% | — | 1.328 |

Runtime is the median repeated-call time divided by 20; RSD is sample standard deviation / mean. Preparation is once per source process and excludes per-call runtime work; references have no SDK/context preparation. RSS is each whole workload process, without baseline subtraction.

| Implementation | Median fresh-process startup ms | RSD |
| --- | ---: | ---: |
| source | 4.085 | 19.02% |
| rust | 2.277 | 30.52% |
| c | 2.493 | 27.03% |

Startup uses workload-free processes, warm OS/file caches, initialization and clean exit. Its 19–31% RSD makes small differences noisy. Complete linked executable sizes: source **2,283,096 B**, rust **493,976 B**, c **493,168 B**. The original source object is **27,912 B**.

Source pipeline times are much higher than optimized reference times in all three cases. They include the scoped runtime checks, per-call preparation and ownership services. CPU scheduling/power state and chip model were not captured; these samples do not establish performance across machines. **B03 Windows measurements and B04 both-host interpretation remain required for G17.**

### Windows native capture (B03)

Verified **2026-10-20 JST**. [CI run 39385041497 / job 118502756853](https://github.com/carimatics/lang/actions/runs/39385041497/job/118502756853) succeeds at clean `dced632` (job completed 2026-10-20 07:17:54 UTC). The [complete native report](../benchmarks/results/2026-10-20-m2-windows-dced632.json), recorded at **07:17:49 UTC**, retains all JSON data with normalized LF newlines. It contains **135 workload and 45 dedicated startup observations**, 15 samples per case, `n=1000`, 20 calls and three untimed warmups, following the successful 21-case parity/unwind gate. Every checksum, allocator retirement, sample/case identity, aggregate, SDK/harness identity and reference link separation was independently checked.

Host: **Windows Server 2025, build 10.0.26100, x86-64, 2 logical CPUs, Python 3.14.8**; processor identifier `AMD64 Family 26 Model 2 Stepping 1, AuthenticAMD` (chip brand/power state not recorded). Release SDK `1a19e49debefcffaec8ab17b288c2a575c528c1f9a22c64d1493960e086f6602` and the unchanged B01 harness use MSVC `/O2 /MD` and Rust `opt-level=3`. Compiler versions/commands and canonical LF/CRLF-equivalent harness hashes are retained. This run selects Windows alone.

| Workload | Implementation | Median µs/call | Runtime RSD | Artifact/context prep ms | Median peak working set MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| `scalar` | source | 1900.895 | 11.53% | 2.603 | 12.953 |
| `scalar` | rust | 1.165 | 3.10% | — | 3.426 |
| `scalar` | c | 0.415 | 20.03% | — | 3.438 |
| `buffers` | source | 8872.875 | 6.51% | 2.610 | 13.145 |
| `buffers` | rust | 4.375 | 33.39% | — | 3.543 |
| `buffers` | c | 2.620 | 10.80% | — | 3.445 |
| `regions` | source | 20966.035 | 4.75% | 2.602 | 13.605 |
| `regions` | rust | 74.030 | 21.90% | — | 3.426 |
| `regions` | c | 60.055 | 20.32% | — | 3.441 |

Each workload process reports actual nonzero **K32 PeakWorkingSetSize**; the parent's stored peak equals that exact reading. These are Windows working-set peaks, distinct from macOS RSS. Runtime/preparation boundaries and the uninstrumented Rust heap counters retain the scope above.

| Implementation | Median fresh-process startup ms | RSD |
| --- | ---: | ---: |
| source | 7.896 | 6.97% |
| rust | 4.566 | 18.88% |
| c | 4.929 | 23.17% |

Dedicated startup executes no workload; the common Rust measurement/JSON driver is included for all three implementations. Startup RSD is approximately 7–23%; Rust/C workload RSD reaches 33.39% in this run. Complete linked executable sizes: source **1,849,344 B**, rust **154,624 B**, c **153,088 B**. The original source object is **29,687 B**.

Source pipeline times exceed optimized references for all three cases. These observations cover the checked C-export embedding and its runtime services; optimizer/accounting policies differ. **B04 must interpret both-host costs, variability, memory definitions and reproducibility before G17 is complete.**

### Both-host comparison and reproduction (B04)

Verified **2026-10-20 JST**. G17's requirement is to measure representative native workloads against equivalent Rust/C implementations. The [macOS capture](../benchmarks/results/2026-10-20-m2-macos-ecc0fd1.json) (clean `ecc0fd1`, 2026-10-20 07:03:45 UTC) and [Windows capture](../benchmarks/results/2026-10-20-m2-windows-dced632.json) (clean `dced632`, 07:17:49 UTC) provide **270 workload and 90 dedicated startup observations** in total. Each native host first passes 21 source/Rust/C/oracle cases and the unwind/retirement/re-registration probe. Full case matrices, checksums, retirement, sample identities, aggregates, SDK identities, canonical harness hashes and reference link boundaries were independently reconciled.

The compiler/runtime, source/Rust/C workloads, harness, SDK builder, Cargo files and pinned Rust toolchain have identical Git blobs between these two revisions; the intervening changes are documentation/results. They use Rust **1.99.0 (`b940084d7`, 2026-09-28)**, Apple Clang **17.0.0 (clang-1700.0.13.5)** or MSVC **19.51.36260**, the flags and matching target-specific SDKs recorded above. Workload inputs, 20 calls, three warmups and 15 samples match. The native hosts differ in OS, ISA, CPU identity and logical-CPU count, so comparisons below stay **within each host**.

| Native host | Workload | Source / Rust elapsed | Source / C elapsed |
| --- | --- | ---: | ---: |
| macOS | `scalar` | ~1,478× | ~1,466× |
| macOS | `buffers` | ~3,362× | ~3,537× |
| macOS | `regions` | ~422× | ~620× |
| Windows | `scalar` | ~1,632× | ~4,580× |
| Windows | `buffers` | ~2,028× | ~3,387× |
| Windows | `regions` | ~283× | ~349× |

These are rounded ratios of per-case sample medians (source elapsed / reference elapsed), not paired-sample averages. A call performs the entire `n=1000` workload. The measured source program shape currently costs hundreds to thousands of times the optimized references in these cases. This establishes an experimental baseline and does not meet a Rust/C-level speed goal.

The source call includes C-export admission, per-call runtime preparation, finite fuel/resource checks and ownership services; its separate `preparation_ns` is one-time artifact/context initialization and cannot be subtracted to obtain “pure native code” cost. Rust/C have different optimizer settings, collection/reference/region implementations and accounting/control overhead. All three applications include the common Rust measurement/JSON driver; C-labelled startup/size includes it. The data measures this checked embedding configuration. It does not isolate which internal operation causes the cost gap; profiling or controlled variants are needed before attributing it.

Variability is retained, rather than hidden by a single number. Workload RSD is approximately **0.86–8.86% on macOS** and **3.10–33.39% on Windows**; fresh-process startup RSD spans **6.97–30.52%** across both. Short reference batches, scheduling and unrecorded power state limit precision. The tables are descriptive medians with raw samples, not confidence intervals or significance claims. Small reference/startup differences need more controlled evidence.

Memory is the range of per-case median **whole-process** peaks: source **27.328–33.766 MiB** versus references **1.328–1.375 MiB** on macOS RSS; source **12.953–13.605 MiB** versus references **3.426–3.543 MiB** on Windows peak working set. Those OS counters are defined/measured differently; compare each to its own host's references. Provider counters cover routed guest/C requests, not every runtime/system allocation; Rust heap counters are uninstrumented. Successful provider balance proves that scoped retirement check, not zero process memory use. Complete executable/object sizes and dedicated startup values are in the per-host tables. Dynamic system libraries and external SDK/toolchain installation sizes are outside those executable file sizes.

CPU chip brand, pinning, scheduler load and power state were not recorded. There is one runner environment per host, one input size and three successful single-thread workloads, with no hardware counters, scaling study or profiling. Historical schema20 hand-written MIR/Rust data is a separate experiment. Preserve these immutable captures and rerun relevant measurements after compiler/runtime/workload/options changes before making claims about a new revision.

#### Recompute the published values

From the repository root, use Python 3.14. The checked-in read-only [summary script](../tools/summarize-m2-bench.py) validates the published sample matrices, checksums and retirement before recomputing median µs/call and the rounded source/reference ratios. On macOS:

```bash
python3 tools/summarize-m2-bench.py
```

On Windows:

```powershell
python tools/summarize-m2-bench.py
```

#### Rerun the native experiment

Use a clean checkout at the capture revision for that host, pinned Rust 1.99.0 and the recorded native C toolchain. Output paths must be unused. On macOS:

```bash
python3 tools/build-native-sdk.py --release --output /tmp/m2-sdk-reproduce
python3 tools/m2-bench/run.py --sdk /tmp/m2-sdk-reproduce --output /tmp/m2-run-reproduce --samples 15 --repeats 20 --input 1000
```

In the Windows x64 MSVC developer environment:

```powershell
python tools/build-native-sdk.py --release --output "$env:TEMP\m2-sdk-reproduce"
python tools/m2-bench/run.py --sdk "$env:TEMP\m2-sdk-reproduce" --output "$env:TEMP\m2-run-reproduce" --samples 15 --repeats 20 --input 1000
```

The dedicated `codex/bench-macos-*` / `codex/bench-windows-*` workflows run this same experiment on one native host and retain the report. Confirm clean revision, expected target/SDK and toolchain, 21 parity cases/unwind, 135 workload + 45 startup observations, all checksums/retirement, canonical hashes and sample/aggregate identities. Recompute medians/RSD from the individual samples; numeric timing equality is not a reproducibility requirement on different runner instances. G17 is supported by both-host measurement and this interpretation/reproduction record; source debugger stepping/backtraces remain G18.

## Interpretation after the 2026-10-17 review

`representative_application` is synthetic host orchestration: it executes separate fixed scalar/resource fixtures, not dispatch through the published instance's negotiated bindings. Selecting and invoking actual provider implementations and measuring generated adapters end to end remain follow-up conformance/measurement work.

All checked-in captures predate the JIT executable-memory reclamation fix: dropping a compiled function previously leaked its finalized mappings. Construction samples and the representative application could therefore accumulate mappings across iterations. Their timings are historical evidence for the recorded commits, not measurements of the corrected implementation. The harness excludes output destruction from construction timing, but releasing mappings between samples changes process state. Recapture clean macOS and Windows results before claiming current compile/application performance or memory budgets; preserve these raw files unchanged. Separate-process memory rows measure their documented child workloads, not the parent JIT leak.

Source: `crates/codegen-clif/src/lib.rs` and its release-on-success/early-exit regression tests, updated 2026-10-17; locked Cranelift JIT 0.136.3 `JITModule::free_memory` and `SystemMemoryProvider::drop`, verified in installed upstream source 2026-10-17 (publication/update date not stated). The accepted initial backend choice is recorded in RFC 0001, not inferred from these timing values alone.

## Current Windows x86-64 schema 20 baseline

The current [Windows baseline](../benchmarks/results/2026-10-17-windows-x86_64-7c37d5d.json) was recorded by [GitHub Actions run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398) from clean commit `7c37d5d1a8d7ac7da5653e9d6931d4759a3a5fa7` at 2026-10-17 01:45:50 JST. The Windows Server 2025 runner reported Microsoft Windows 10.0.26100.33438 on an AMD64 Family 25 Model 1 CPU. It used harness schema 20, the pinned Rust 1.99.0 MSVC toolchain, 31 samples, 1,000 lowering/component operations, 100,000 runtime operations, and ten launches per startup batch. The artifact reports `git_dirty: false`; the same run passed formatting, lint, the full test suite, documentation, direct and response-file MSVC object-link-run tests, typed component-resource cleanup conformance, benchmark validation, and artifact upload.

Selected cross-platform rows are:

| Measurement | Windows median | Windows p95 | Windows RSD | Windows/macOS median ratio |
| --- | ---: | ---: | ---: | ---: |
| Six-object system link | 74.068 ms | 121.479 ms | 20.896% | 2.026× |
| Ten-launch batched AOT startup | 7.444 ms | 8.986 ms | 6.832% | 4.972× |
| VM Fibonacci call | 3.526 us | 3.621 us | 3.565% | 2.845× |
| JIT Fibonacci call | 13.240 ns | 15.684 ns | 5.459% | 1.469× |
| Representative component application | 236.804 us | 266.366 us | 6.201% | 3.503× |

The Windows peak-process-memory rows are all positive, ordered, and based on 31 separate processes:

| Process row | Median | p95 | RSD |
| --- | ---: | ---: | ---: |
| Allocation-free linked AOT harness | 3,207,168 bytes | 3,211,264 bytes | 0.050% |
| Benchmark control | 6,242,304 bytes | 6,246,400 bytes | 0.124% |
| Representative allocation workload | 26,189,824 bytes | 26,198,016 bytes | 0.032% |
| 4,096 retained component instances | 14,241,792 bytes | 14,245,888 bytes | 0.025% |

The representative workload preserves the exact logical profile from macOS: 23,894 allocations, 19,183,210 total requested bytes, 18,102,042 peak live requested bytes, and zero live bytes afterward. Its Windows control-adjusted median resident delta is 19,947,520 bytes versus 21,053,440 bytes on macOS (0.947×); the corresponding combined allocator-overhead/fragmentation estimates are 1,845,478 and 2,951,398 bytes. These ratios are directional system-level evidence, not backend-only comparisons: the runs use different CPUs, operating systems, object formats, linkers, process loaders, and OS-specific peak-working-set metrics. The 20.896% link RSD and 6.201% representative-application RSD also rule out treating these medians as stable budgets.

## Current macOS AArch64 schema 20 representative-application baseline

The current [baseline](../benchmarks/results/2026-10-16-macos-aarch64-de248a1.json) was recorded from clean commit `de248a126e2e7b20e7be2df083b104a6c2e438df` on an Apple M1 Max running macOS 27.0.1 at 2026-10-16 23:34:00 JST. It used harness schema 20, the pinned Rust 1.99.0 toolchain, Apple Clang 21.0.0, 31 samples, 1,000 operations per component construction/lifecycle/application sample, and the retained schema-19 parameters for all other rows. The artifact reports `git_dirty: false`, fourteen component-boundary rows, and all retained schema-19 fields.

Schema 20 adds one row to the existing component-boundary series:

| Series | Measurement | Operations per sample | Median | p95 | RSD |
| --- | --- | ---: | ---: | ---: | ---: |
| Representative application | Two-provider negotiation, lock generation/replay, instantiation, scalar/resource execution, and failure cleanup | 1,000 | 67.592 us | 68.459 us | 1.109% |

`component_application_negotiate_lock_replay_instantiate_execute_cleanup` composes two independently selected provider interfaces, deterministic unlocked lock generation, exact-lock replay, fresh instance publication, scalar and nominal owned-resource VM/JIT calls, and reusable guest-trap cleanup in one operation. The checked-in application test requires two bindings and two exact lock entries and requires all scalar, resource, and cleanup paths to preserve the fixture value 42.

The row includes package selection, fresh instance construction, bytecode lowering, JIT compilation, adapter setup, and cleanup; it is end-to-end directional evidence rather than an isolated call-overhead budget. The fixture is host-orchestrated: validated bindings gate the calls, but they do not yet materialize production generated dispatch stubs. The Windows baseline above validates the same schema-20 row and every retained schema-19 contract on the native MSVC runner.

## Previous macOS AArch64 schema 19 component-boundary baseline

The current [baseline](../benchmarks/results/2026-10-16-macos-aarch64-76c8f81.json) was recorded from clean commit `76c8f81516be5eb4cd77ab063b3ecd3935d16e58` on an Apple M1 Max running macOS 27.0.1 at 2026-10-16 22:51:15 JST. It used harness schema 19, the pinned Rust 1.99.0 toolchain, Apple Clang 21.0.0, 31 samples, 1,000 operations per component construction/lifecycle sample, and the retained schema 18 parameters for all other rows.

Schema 19 preserves the 56 general timing rows and adds five component-boundary rows:

| Series | Measurement | Operations per sample | Median | p95 | RSD |
| --- | --- | ---: | ---: | ---: | ---: |
| Instantiation | Negotiate and instantiate one unlocked schema-2 contract | 1,000 | 1.226 us | 1.352 us | 3.802% |
| Instantiation | Replay a complete exact lockfile and create an instance | 1,000 | 1.358 us | 1.384 us | 0.934% |
| Host-value lifecycle | Publish, transfer, and release one owned host value | 1,000 | 507.875 ns | 559.375 ns | 3.630% |
| Failure cleanup | Instantiate, transfer, clean a guest trap, and project a redacted report | 1,000 | 3.529 us | 3.619 us | 1.675% |
| Failure cleanup | Release the host value, record destructor failure/poisoning, and project a redacted report | 1,000 | 524.959 ns | 545.417 ns | 2.084% |

The unlocked and locked rows both validate the required projection; lock replay additionally validates complete exact package/version/digest coverage and constructs the canonical lock artifact before creating fresh adapter state. The guest-trap row also performs two general instantiations and therefore must not be read as cleanup cost alone. The destructor row starts from already constructed adapter tables and isolates publication, transfer, cleanup failure, poisoning, and projection more narrowly. Neither row lowers typed resources through MIR, bytecode, JIT, or native ABI.

A separate child process retains 4,096 exact-lock instances. Its peak-resident median is 10,108,928 bytes versus 2,064,384 bytes for the unchanged control, an 8,044,544-byte whole-process delta; p95 is 10,174,464 bytes and RSD is 0.268%. This envelope includes loader state, allocator behavior, duplicated owned contracts/bindings, adapter containers, and measurement-process overhead. It is not a per-instance language memory guarantee and is not normalized by simple division.

The scalar `i64 -> i64` component function has a 40-byte JIT body and a 416-byte AOT object. The encoded schema-2 benchmark contract is 271 bytes and the redacted failure report is 19 bytes. Machine-code sizes exclude the host registry, adapter/resource implementation, runtime support, and final linking; document sizes are serialization evidence, not executable code size. The later Windows schema-20 baseline records native component execution and retains these measurement contracts.

## Previous schema 18 macOS AArch64 CFG and representative-allocation baseline

The previous [baseline](../benchmarks/results/2026-10-16-macos-aarch64-b5ddb9b.json) was recorded from clean commit `b5ddb9b6ed0e8339935c55df2a138233209f13d0` on an Apple M1 Max running macOS 27.0.1 at 2026-10-16 19:17:44 JST. It used harness schema 18, the pinned Rust 1.99.0 toolchain, Apple Clang 21.0.0, 31 samples, 1,000 lowering/validation/negotiation/adapter operations per sample, and 100,000 runtime, callback, scalar-dispatch, and table-lifecycle operations per sample.

Schema 18 preserves every schema 17 field and adds iterative Fibonacci as a representative CFG workload. The loop carries a countdown and two wrapping sequence values through typed block arguments. Its retained cross-mode rows are:

| Execution path | Operations per sample | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Direct Rust Fibonacci | 100,000 | 6.510 ns | 6.747 ns | 1.442% |
| VM Fibonacci | 100,000 | 1.231 us | 1.238 us | 0.349% |
| JIT Fibonacci | 100,000 | 7.463 ns | 8.593 ns | 4.483% |

The JIT body is 48 bytes, the AOT object is 400 bytes, and the linked six-workload harness is 17,168 bytes. The schema also retains the compiler-like mixed-lifetime allocation workload: 16,384 retained payloads over 12 size classes, every-third release, differently sized replacement, and 2,048 overlapping transient buffers. Every payload byte is touched, and the exact logical profile remains `(23,894 allocations, 19,183,210 total requested bytes, 18,102,042 peak live requested bytes, 0 live bytes afterward)`.

The same benchmark executable is launched in interleaved control/workload pairs so loader and binary structure are shared between the two distributions:

| Process measurement | Minimum | Median | p95 | Maximum | RSD |
| --- | ---: | ---: | ---: | ---: | ---: |
| Benchmark control | 2,048,000 B | 2,064,384 B | 2,064,384 B | 2,064,384 B | 0.140% |
| Representative allocation workload | 23,068,672 B | 23,085,056 B | 23,085,056 B | 23,085,056 B | 0.035% |

The median workload-minus-control resident delta is 21,020,672 bytes. Subtracting peak live requested payload yields a 2,918,630-byte combined allocator-overhead/fragmentation estimate. The calculation saturates at zero and cannot separate allocator metadata, size-class slack, retained pages, page rounding, or true external fragmentation. It is evidence for this trace, allocator, OS, and binary only—not a language memory budget.

The retained variable-size callback rows are:

| Execution path | Operations per sample | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Direct Rust variable-size allocation callback | 100,000 | 216.019 ns | 219.115 ns | 0.714% |
| VM variable-size allocation callback | 100,000 | 397.814 ns | 402.769 ns | 0.635% |
| JIT variable-size allocation callback | 100,000 | 218.037 ns | 221.265 ns | 0.590% |

The exact callback profile remains `(16 allocations, 1,088 total bytes, 1,088 peak live bytes, 0 live bytes afterward)`. Together with the retained single, burst, and churn profiles, schema 18 contains 56 general timing rows and four callback requested-payload profiles.

The retained component-boundary rows in this run are:

| Series | Measurement | Operations per sample | Median | p95 | RSD |
| --- | --- | ---: | ---: | ---: | ---: |
| Metadata validation | Clone and validate the one-import/one-export schema-1 scalar contract | 1,000 | 277.916 ns | 304.083 ns | 4.407% |
| Interface negotiation | Validate and select the highest compatible member from three providers | 1,000 | 824.958 ns | 866.750 ns | 2.276% |
| Scalar dispatch | Direct wrapping callback | 100,000 | 0.930 ns | 0.932 ns | 0.521% |
| Scalar dispatch | Validated logical-import lookup, host-registry lookup, and callback | 100,000 | 11.322 ns | 11.536 ns | 1.443% |
| Resource lifecycle | Generation-tagged owner insert and release | 100,000 | 14.209 ns | 14.781 ns | 1.795% |
| Resource lifecycle | Owner insert, shared borrow/read/end, and release | 100,000 | 26.504 ns | 27.220 ns | 1.426% |
| Resource lifecycle | Canonical owner preflight/commit/persist while alternating between two adapters | 1,000 | 367.666 ns | 454.000 ns | 7.847% |
| Resource lifecycle | Canonical borrow preflight/commit and call-scope cleanup | 1,000 | 341.542 ns | 435.834 ns | 11.015% |

These rows deliberately answer different questions. Metadata validation includes cloning the consumed metadata and constructing the immutable resolved binding. Negotiation validates the consumer and all three providers, performs version and structural filtering, and constructs the selected binding. The validated scalar row performs a logical-name lookup and host-registry lookup on every operation; its 10.392 ns median delta from the direct row is evidence for this exact in-memory path, not a stable dispatch budget. The `ResourceTable` rows exercise reusable generation-tagged storage, while the canonical rows also include preflight allocations/reservations, trace and cleanup-ledger construction, and monotonically growing boundary-index vectors. They do not measure VM/JIT/AOT resource lowering, host-value integration, failure/destructor paths, component code size or memory, remote Windows behavior, or a representative application.

## Previous schema 14 macOS AArch64 baseline

This historical [baseline](../benchmarks/results/2026-10-16-macos-aarch64-6a2caf8.json) was recorded from clean commit `6a2caf827de5b3a28746708cc36178c86a514a45` on an Apple M1 Max running macOS 27.0.1 at 2026-10-16 14:02:20 JST. It used harness schema 14, the pinned Rust 1.99.0 toolchain, and Apple Clang 21.0.0 in a release build.

Schema 14 records all 47 timing rows and three deterministic host-callback requested-payload heap profiles. The single callback is `(1, 8, 8, 0)`, the burst is `(64, 512, 512, 0)`, and the churn sequence is `(96, 768, 512, 0)`. Direct Rust, VM, and JIT churn medians are 1.192 us, 1.348 us, and 1.173 us. The linked AOT process-memory distribution has 31 samples with a 1,654,784-byte minimum and 1,671,168-byte median, p95, and maximum. Heap profiles measure logical payload lifetime; the process metric is a whole-process resident envelope rather than allocator overhead.

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
| Bytecode lowering: wrapping `i64` add | 1,000 | 387.583 ns | 467.875 ns | 6.639% |
| Bytecode lowering: signed minimum | 1,000 | 1.067 us | 1.184 us | 4.369% |
| Bytecode lowering: countdown back-edge | 1,000 | 1.724 us | 1.980 us | 5.461% |
| Bytecode lowering: two-value accumulator | 1,000 | 1.786 us | 1.942 us | 3.314% |
| Bytecode lowering: conditional sum | 1,000 | 3.211 us | 3.473 us | 3.893% |
| Cranelift JIT compilation: wrapping `i64` add | 1 | 19.500 us | 32.584 us | 66.655% |
| Cranelift JIT compilation: signed minimum | 1 | 22.917 us | 30.000 us | 19.507% |
| Cranelift JIT compilation: countdown back-edge | 1 | 26.708 us | 35.334 us | 12.975% |
| Cranelift JIT compilation: two-value accumulator | 1 | 30.791 us | 36.000 us | 7.686% |
| Cranelift JIT compilation: conditional sum | 1 | 40.500 us | 47.916 us | 7.178% |
| Cranelift AOT object emission: wrapping `i64` add | 1 | 13.459 us | 16.833 us | 22.039% |
| Cranelift AOT object emission: signed minimum | 1 | 17.834 us | 21.042 us | 5.882% |
| Cranelift AOT object emission: countdown back-edge | 1 | 22.584 us | 51.541 us | 57.765% |
| Cranelift AOT object emission: two-value accumulator | 1 | 26.583 us | 35.459 us | 15.450% |
| Cranelift AOT object emission: conditional sum | 1 | 36.250 us | 43.375 us | 10.384% |
| macOS system link: precompiled C harness plus add, minimum, countdown, accumulator, and conditional-sum objects | 1 | 39.705 ms | 42.020 ms | 3.349% |
| macOS AOT process startup: single launch | 1 | 1.923 ms | 3.496 ms | 232.018% |
| macOS AOT process startup: ten-launch batch, per launch | 10 | 1.822 ms | 1.988 ms | 5.362% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.930 ns | 1.019 ns | 4.162% |
| VM wrapping `i64` add call | 100,000 | 183.391 ns | 186.630 ns | 1.243% |
| JIT wrapping `i64` add call | 100,000 | 1.901 ns | 2.065 ns | 3.281% |
| Direct Rust signed minimum call | 100,000 | 0.930 ns | 0.993 ns | 2.147% |
| VM signed minimum call | 100,000 | 505.068 ns | 509.653 ns | 0.493% |
| JIT signed minimum call | 100,000 | 2.640 ns | 2.878 ns | 2.821% |
| Direct Rust countdown call | 100,000 | 0.960 ns | 1.064 ns | 4.144% |
| VM countdown call | 100,000 | 1.611 us | 1.622 us | 0.383% |
| VM countdown call with unset interruption flag | 100,000 | 1.641 us | 1.947 us | 15.866% |
| VM countdown call with unexpired deadline | 100,000 | 3.320 us | 3.369 us | 0.614% |
| JIT countdown call | 100,000 | 12.846 ns | 13.716 ns | 2.466% |
| Direct Rust two-value accumulator call | 100,000 | 1.705 ns | 1.753 ns | 0.855% |
| VM two-value accumulator call | 100,000 | 1.665 us | 1.678 us | 0.363% |
| JIT two-value accumulator call | 100,000 | 12.790 ns | 13.716 ns | 2.246% |
| Direct Rust conditional-sum call | 100,000 | 5.605 ns | 5.738 ns | 0.997% |
| VM conditional-sum call | 100,000 | 3.046 us | 3.073 us | 1.002% |
| JIT conditional-sum call | 100,000 | 20.011 ns | 21.299 ns | 2.295% |
| Direct Rust host callback | 100,000 | 0.930 ns | 0.979 ns | 1.865% |
| VM host callback | 100,000 | 176.513 ns | 181.394 ns | 1.109% |
| JIT host callback | 100,000 | 2.797 ns | 3.333 ns | 5.898% |
| Direct Rust host-allocation callback | 100,000 | 11.931 ns | 13.015 ns | 3.752% |
| VM host-allocation callback | 100,000 | 188.068 ns | 191.508 ns | 1.018% |
| JIT host-allocation callback | 100,000 | 13.631 ns | 14.070 ns | 1.723% |
| Direct Rust 64-allocation-burst callback | 100,000 | 775.515 ns | 1.269 us | 20.514% |
| VM 64-allocation-burst callback | 100,000 | 959.024 ns | 968.330 ns | 0.544% |
| JIT 64-allocation-burst callback | 100,000 | 789.842 ns | 825.818 ns | 6.002% |
| Direct Rust allocation-churn callback | 100,000 | 1.192 us | 1.373 us | 4.901% |
| VM allocation-churn callback | 100,000 | 1.348 us | 1.373 us | 0.861% |
| JIT allocation-churn callback | 100,000 | 1.173 us | 1.184 us | 0.388% |

The separate linked-AOT process-memory distribution has a 1,654,784-byte (1.578 MiB) minimum; a 1,671,168-byte (1.594 MiB) median, p95, and maximum; a 1,670,639.484-byte mean; and 0.173% RSD. The 16,384-byte spread is evidence about this tiny allocation-free harness on this host, not a universal memory floor.

Cranelift's compiled code buffer contained 8 bytes for add, 20 bytes for minimum, 20 bytes for countdown, 36 bytes for the accumulator, 44 bytes for conditional sum, and 40 bytes for the host callback. The emitted add, minimum, countdown, accumulator, and conditional-sum objects were 352, 368, 376, 392, and 400 bytes. The linked AOT harness was 17,104 bytes. JIT code sizes exclude object-file structure, relocation data, unwind/debug information, runtime support, and linker output. Object sizes include Mach-O structure but not the C harness or final executable; the executable size includes both plus the platform linker's defaults, but not a complete runtime distribution.

## Schema 19 format and migration

Schema 19 preserves every schema 18 field. `component_boundary_measurements` grows from eight to thirteen rows and adds the `instantiation`, `host_value_lifecycle`, and `failure_cleanup` series. `process_memory_measurements` adds `benchmark_process_component_instances_peak_resident_memory`, described by `component_instance_memory_profile.retained_instance_count` and its exact row name. `generated_code_bytes.jit_component_scalar`, `object_file_bytes.aot_component_scalar`, and the new `component_document_bytes` object report component artifact sizes with distinct semantics. The Windows workflow requires all new rows, series, positive artifact sizes, exactly 4,096 retained instances, and a component-process median above the control median. Consumers must branch on `schema_version`; schema 1–18 JSON files remain immutable evidence.

## Schema 18 format and migration

Schema 18 preserves every schema 17 field and adds six Fibonacci rows: bytecode lowering, JIT compilation, AOT object emission, and direct Rust/VM/JIT runtime. It also adds `jit_fibonacci` and `aot_fibonacci` artifact sizes. Because the linked workload set grows, the system-link, single-process startup, batched-startup, linked-process memory, and executable-size names insert `fibonacci` after `countdown`. The clean macOS AArch64 result contains all 56 unique timing rows, four exact callback profiles, eight component rows, and three process-memory rows. Consumers must branch on `schema_version`; schema 1–17 JSON files remain immutable evidence.

The workload computes iterative wrapping Fibonacci with a signed countdown and two sequence values carried through typed block arguments. It exercises three-value loop state and back-edge argument rotation using the existing MIR operations; it does not add or accept source-language semantics. VM/JIT differential tests include wrapping overflow, macOS direct/response-file integration executes its AOT object, host-independent Windows COFF validation checks the export, and the cfg-gated MSVC path plus Windows workflow retain the contract.

## Schema 17 format and migration

Schema 17 preserves every schema 16 timing, artifact, callback-heap, component-boundary, and linked-AOT process-memory field. It adds `representative_allocation_workload` with exact requested-payload counts, references to paired control/workload process rows, their median resident delta, and `estimated_allocator_overhead_and_fragmentation_bytes_at_median`. The estimate is `max(0, workload median - control median - peak live requested payload)`. `process_memory_measurement_scope` becomes `separate_process_peak_resident_memory`, and the process array contains the retained linked-AOT row plus `benchmark_process_control_peak_resident_memory` and `benchmark_process_representative_allocation_peak_resident_memory`.

The workload deliberately mixes size classes and lifetimes and touches all requested bytes. It is more representative of compiler allocation pressure than the bounded callbacks but remains a synthetic trace. The estimate combines metadata, internal/external fragmentation, retained pages, and page rounding and cannot attribute them independently. The initial clean macOS AArch64 result is the representative-allocation baseline above. Consumers must branch on `schema_version`; schema 1–16 JSON files remain immutable evidence.

## Schema 16 format and migration

Schema 16 preserves every schema 15 field and adds `rust_host_allocation_variable_call`, `vm_host_allocation_variable_call`, and `jit_host_allocation_variable_call`. The generic heap path adds `host_allocation_variable_call` with `(allocation_count, total_allocated_bytes, peak_live_bytes, live_bytes_after)` equal to `(16, 1088, 1088, 0)`. The harness test and Windows workflow require all three timing rows and the exact profile.

The initial clean macOS AArch64 result remains the historical variable-allocation baseline. Consumers must branch on `schema_version`; schema 1–15 JSON files remain immutable evidence. Schema 16 does not isolate allocator overhead or fragmentation, provide representative-application evidence, or make the measured prototype paths a stable ABI or performance commitment.

## Schema 15 format and migration

Schema 15 adds `component_boundary_measurements`, an array whose rows use the same distribution fields as `measurements` plus a required `series` value. The initial series are `metadata_validation`, `interface_negotiation`, `scalar_dispatch`, and `resource_lifecycle`. The eight exact row names are checked by the harness test and Windows workflow. Existing artifact, heap, process-memory, and 47 general timing rows remain unchanged in shape.

The initial clean macOS AArch64 result remains the historical component-boundary baseline. Consumers must branch on `schema_version`; schema 1–14 JSON files remain immutable evidence. Schema 15 does not make the measured prototype paths a stable ABI or performance commitment.

## Schema 14 format and migration

Schema 14 preserves every schema 13 timing, artifact, heap, and process-memory field and adds `rust_host_allocation_churn_call`, `vm_host_allocation_churn_call`, and `jit_host_allocation_churn_call`. Each operation allocates 64 boxed `i64` values, consumes every other allocation, allocates 32 replacements into the released slots, then consumes the final 64. The same generic heap path adds `host_allocation_churn_call` with `(allocation_count, total_allocated_bytes, peak_live_bytes, live_bytes_after)` equal to `(96, 768, 512, 0)`.

The rows measure a fixed-size synthetic host-callback sequence with free/reallocate interleaving. They do not isolate allocator metadata, fragmentation, or reuse policy and are not representative-application evidence. VM/JIT differential tests, macOS direct/response-file AOT integration, host-independent Windows COFF validation, and the cfg-gated MSVC path cover the callback/import contract. The clean 31-sample macOS AArch64 run records 47 unique timing rows, all three exact heap profiles, direct Rust/VM/JIT churn medians of 1.192 us, 1.348 us, and 1.173 us, and linked-process peak memory from 1,654,784 to 1,671,168 bytes. Current Windows CI requires schema 20 while retaining the exact churn rows/profile. Consumers must branch on `schema_version`; schema 1–13 JSON files remain immutable evidence.

## Schema 13 format and migration

Schema 13 preserves every schema 12 timing, artifact, and heap field and adds `process_memory_measurement_scope`, `process_memory_metric`, and `process_memory_measurements`. The scope is `linked_aot_executable_process`. The single row, `aot_process_peak_resident_memory_add_min_countdown_sum_to_sum_below`, records `bytes/process`, `sample_count`, minimum, median, p95, maximum, mean, and relative standard deviation after launching the linked AOT executable once per sample.

The metric is `macos_rusage_ru_maxrss_bytes` on macOS and `windows_process_peak_working_set_bytes` on Windows MSVC. The former is parsed from `/usr/bin/time -l`; for the latter, each measured process self-reports `PROCESS_MEMORY_COUNTERS.PeakWorkingSetSize` through `K32GetProcessMemoryInfo` before exit. Both include private and shared resident pages plus loader/runtime state and therefore are platform-specific process envelopes. Consumers must branch on `schema_version`; schema 1–12 JSON files remain immutable evidence. The historical clean schema 13 macOS AArch64 result records all 31 samples as 1,671,168 bytes with 0.000% RSD. Current schema 20 Windows CI requires this retained row plus positive, ordered control/workload/component distributions, but no remote result has been inspected or committed.

## Schema 12 format and migration

Schema 12 adds `heap_measurement_scope` and `heap_measurements` while preserving all schema 11 timing rows and artifact fields. The scope is `host_callback_requested_payload_bytes`; each profile records `allocation_count`, `total_allocated_bytes`, `peak_live_bytes`, and `live_bytes_after`. The harness runs the same generic single and burst callback workloads through a safe measuring heap outside the timed loops.

The exact single profile is `(1, 8, 8, 0)` and the burst profile is `(64, 512, 512, 0)`. These values cover requested `i64` payloads and logical cleanup only. They exclude allocator metadata and fragmentation, the stack, generated code, VM/JIT internal allocations, and process RSS. Consumers must branch on `schema_version`; schema 1–11 JSON files remain immutable evidence. The historical schema 12 result records both profiles and all 44 unique timing rows. Schemas 13 and 14 plus Windows CI retain the same heap contract, but no remote result has been inspected or committed.

## Schema 11 format and migration

Schema 11 adds `rust_host_allocation_burst_call`, `vm_host_allocation_burst_call`, and `jit_host_allocation_burst_call`. Each operation creates 64 distinct `Box<i64>` values, keeps all 512 payload bytes live in a fixed array, computes their wrapping sum, and then drops them. `black_box` keeps each box and loaded value opaque to optimization. The VM and JIT rows include the same typed host-call dispatch around the registered callback, while the direct Rust row provides the host-side baseline. The harness verifies all three results before warm-up and timing.

The existing AOT link/startup series remains unchanged, while macOS and configured Windows integration tests execute a corresponding C callback that holds 64 `malloc` allocations live before reading and freeing them. The rows do not measure allocator metadata, the stack-resident pointer array, process peak RSS, failure behavior, a language heap, or a representative application. Schema 11 leaves all schema 10 rows and artifact fields unchanged. Consumers must branch on `schema_version`; schema 1–10 JSON files remain immutable evidence. The clean historical schema 11 result records all 44 unique rows and direct Rust, VM, and JIT burst medians of 782.624 ns, 962.329 ns, and 786.270 ns. Their RSD values are 0.331%, 11.822%, and 0.378%; the VM maximum of 1.620 us raises its RSD despite a 967.234 ns p95, so those values remain directional evidence rather than a performance budget. The Windows Server 2025 workflow now requires schema 20 while retaining all three burst rows.

## Schema 10 format and migration

Schema 10 adds `vm_countdown_deadline_call`. It executes the same alternating positive/immediate-return inputs and verified countdown bytecode as `vm_countdown_call`, but supplies an absolute monotonic deadline set one hour after harness preparation through `ExecutionControl`. The harness verifies equivalent results before warm-up and timing, and the deadline remains shared across all samples rather than being recomputed per call.

This row measures the steady-state cost of enabling deadline checks while the deadline remains unexpired. It does not measure expiration latency, scheduler accuracy, callback overrun, callback preemption, or native/JIT enforcement. Schema 10 leaves all existing rows and artifact fields unchanged. Consumers must branch on `schema_version`; schema 1–9 JSON files remain immutable evidence. The historical clean schema 14 result records a 1.611 us ordinary VM countdown median and a 3.320 us unexpired-deadline median: a 1.709 us, 106.071% increase. The 0.383% and 0.614% RSD values make the paired medians stable for this run but do not establish an overhead budget. The current schema-20 Windows workflow retains this row.

## Schema 9 format and migration

Schema 9 adds `vm_countdown_interruptible_call`. It executes the same alternating positive/immediate-return inputs and the same verified countdown bytecode as `vm_countdown_call`, but supplies a shared, unset interruption flag through `ExecutionControl`. In the current schema 13 artifact, both rows include the VM's optional-control branch; the new row additionally performs the flag's acquire load at each executed instruction and terminator. The harness verifies equivalent results before warm-up and timing.

This row measures the steady-state cost of enabling cooperative interruption when no request occurs. It does not measure interruption latency, cross-thread wake-up, a set flag, callback preemption, or native/JIT enforcement. Schema 9 leaves all lowering, compilation, artifact-size, host-callback, link, and startup fields unchanged. Consumers must branch on `schema_version`; schema 1–8 JSON files remain immutable evidence. In the historical clean schema 14 result, the ordinary VM countdown median is 1.611 us and the unset-interruption median is 1.641 us, a 29.868 ns or 1.854% increase. The interruption row's 15.866% RSD exceeds that delta, so this paired microbenchmark shows no measurable interruption-check regression and does not establish an overhead budget. The current schema-20 Windows workflow retains this row.

## Schema 8 format and migration

Schema 8 adds `rust_host_allocation_call`, `vm_host_allocation_call`, and `jit_host_allocation_call`. Each operation passes an `i64` through one Rust `Box` allocate/read/drop round trip; `black_box` makes the allocated box and returned value opaque to optimization. The VM and JIT rows include their existing typed host-call dispatch around the same registered callback, while the direct Rust row provides the host-side baseline. The harness checks all three results before warming and timing them.

The existing AOT system-link, executable-size, and process-startup series do not change: a single allocation inside a launched harness would be dominated by process creation and loader work. Separate platform integration tests execute the corresponding C `malloc`/read/`free` callback through linked AOT objects. The new rows therefore measure a narrow host-owned allocation plus VM/JIT boundary cost; they do not measure a language heap, allocator policy, peak memory, allocation failure, collection, or representative application behavior. Consumers must branch on `schema_version`; schema 1–7 JSON files remain immutable evidence. The clean macOS AArch64 schema 8 result is recorded above.

The Windows Server 2025 workflow first required schema 8 and all three single-allocation rows in addition to the unchanged platform measurements. No remote schema 8 result has been inspected or committed; the workflow now requires schema 20 as described above.

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

The schema 7 harness records the conditional-sum lowering/compilation/runtime rows, the renamed system-link and process-startup rows, and `executable_file_bytes.aot_add_min_countdown_sum_to_sum_below` on Windows x86-64 MSVC. It compiles the C harness with `cl.exe`, discovers the tier-1 `link.exe` through `LinkTarget`, and validates one link and execution before timing. Schema 8 retains those platform fields and the 31/1,000/100,000/10 parameters and adds the three single-allocation runtime rows. Schema 9 adds `vm_countdown_interruptible_call`, schema 10 adds `vm_countdown_deadline_call`, schema 11 adds three allocation-burst rows, schema 12 adds the two exact heap profiles, schema 13 adds the linked-process peak-memory distribution, schema 14 adds allocation-churn evidence, schema 15 adds component-boundary series, schema 16 adds variable-size allocation timing/profile evidence, schema 17 adds representative allocation/process-memory evidence, schema 18 adds Fibonacci CFG timing/artifact evidence, schema 19 adds component lifecycle/artifact/process-memory evidence, and schema 20 adds representative-application evidence; schema 20 is now required by the Windows Server 2025 workflow. The workflow also validates clean-checkout metadata, the `windows`/`x86_64` identity, interruption, deadline, conditional-sum, Fibonacci, single-allocation, allocation-burst, allocation-churn, variable-size allocation, exact representative-workload accounting, all four process-memory rows, all fourteen component-boundary rows, the startup batch size, and positive Fibonacci/component/executable sizes before uploading the JSON for 30 days.

The committed schema-20 Windows baseline above is the recorded output of this path. GitHub Actions run 38932964398 completed every workflow step in 11 minutes 55 seconds and uploaded the artifact retained in this repository; the result complements rather than replaces the macOS baseline because the platform metrics are not interchangeable.

## Generated-code inspection

The add and host-callback objects were regenerated from clean commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad` and inspected on the macOS AArch64 host on 2026-10-16 at 00:25 JST. The signed-minimum object was first regenerated from clean commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483` at 00:43 JST, then regenerated after the explicit-CFG migration from clean commit `f6eb016798c237412b59057fa32257ca2bc5d016` at 01:08 JST.

| Prototype | Export | Import | Generated body | Relocations |
| --- | --- | --- | --- | --- |
| Wrapping `i64` add | `_prototype_add` | None | `add x0, x0, x1`; `ret` | None |
| Signed `i64` minimum | `_prototype_min` | None | `cmp`; conditional `b.lt`; else-side `mov`; two `ret` paths | None |
| Typed host callback | `_prototype_host_callback` | `_lang_host_i64_0` | Pointer-authenticated frame, GOT load, indirect `blr`, authenticated return | `GOTLDP` and `GOTLDPOFF` against `_lang_host_i64_0` |

All three files are 64-bit ARM64 Mach-O objects. The add prototype lowers to an eight-byte, two-instruction body without relocations. Minimum lowers to a 20-byte compare and real conditional branch with no relocations: the less-than path preserves the first argument in `x0`, while the other path moves `x1` into the return register. Reinspection after the explicit-CFG migration produced the same code and object sizes, symbol, instruction sequence, and empty relocation table. The callback remains a linker-resolved external import and uses position-independent GOT addressing; its body is 32 bytes. This confirms the intended arithmetic, control-flow, and export/import lowering boundaries. It does not evaluate loop lowering, block parameters, loop-carried values, allocation, unwind behavior, or other targets.

The complete commands and captured `file`, `nm`, and `otool` output are preserved in the [add/callback transcript](../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md), [pre-CFG minimum transcript](../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md), and [explicit-CFG minimum transcript](../benchmarks/inspection/2026-10-16-macos-aarch64-f6eb016-cfg-min.md).

The clean pre-implementation [debug-information inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) records the original negative baseline. The committed [source-mapped DWARF inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md) regenerates the representative add, minimum, and host-callback objects from commit `31e182ac27e324048597a5ebd0fda4d2d4934395`. All three objects pass `dwarfdump --verify` with relocated DWARF v4 compile-unit, subprogram, and source-line data; direct and response-file executables link and run; and both generated dSYMs pass verification while retaining the fixture path and all three functions. The committed [Windows x86-64 CodeView/PDB inspection](../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md) emits the same representative COFF objects from commit `465b0d66ba6ea61a0d4b26201e6ab043f2f15f9d`, validates C13 records and relocations, links them into PE/PDB artifacts with LLD, and verifies the linked source mappings with LLDB. Together these satisfy the planned tier-1 source-mapping criteria without claiming native Windows execution, source-language locals, unwind metadata, JIT debugger registration, or optimized stepping.

## Interpretation and limits

- The harness checks VM, JIT, and direct Rust results for equivalence before timing, including both outcomes of minimum, the immediate-return/loop paths of countdown and the accumulator, and the immediate-return/add/skip paths of conditional sum.
- The two-value accumulator carries a descending counter and wrapping sum around the same back-edge. Its direct, VM, and JIT medians are 1.705 ns, 1.665 us, and 12.790 ns. This bounded MIR workload is useful differential evidence, not a representative program benchmark.
- The conditional-sum prototype includes both branch outcomes and the immediate-return path. Its direct, VM, and JIT medians are 5.605 ns, 3.046 us, and 20.011 ns. This is broader CFG evidence, not a representative program benchmark.
- The schema 18 Fibonacci workload alternates 30 loop iterations with an immediate non-positive return while rotating three loop-carried values. Direct, VM, and JIT medians are 6.510 ns, 1.231 us, and 7.463 ns. It adds a recognizable stateful loop to the cross-mode set, but remains a bounded microbenchmark rather than representative application performance.
- The single-allocation host-callback medians are 11.931 ns direct, 188.068 ns through the VM, and 13.631 ns through JIT. VM minus direct is 176.137 ns (1,476.297%), while JIT minus direct is 1.700 ns (14.249%). The three rows have 3.752%, 1.018%, and 1.723% RSD, providing stable narrow boundary-cost evidence for this run; one `Box<i64>` round trip is still not a representative allocator or memory workload.
- The 64-allocation-burst medians are 775.515 ns direct, 959.024 ns through the VM, and 789.842 ns through JIT. VM minus direct is 183.509 ns (23.663%), while JIT minus direct is 14.327 ns (1.847%). The direct, VM, and JIT rows have 20.514%, 0.544%, and 6.002% RSD, so the result is directional rather than a general boundary-overhead budget.
- The 96-allocation churn medians are 1.192 us direct, 1.348 us through the VM, and 1.173 us through JIT. VM minus direct is 156.544 ns (13.137%), while JIT minus direct is -18.203 ns (-1.528%). The rows have 4.901%, 0.861%, and 0.388% RSD; the negative JIT delta is noise-sensitive microbenchmark evidence, not a claim that the boundary removes allocation cost.
- Schema 12 profiles the same generic callback workloads outside the timed loops. The single callback records one allocation, 8 total and peak live payload bytes, and zero live bytes afterward; the burst records 64 allocations, 512 total and peak live payload bytes, and zero afterward. The scope is deliberately `host_callback_requested_payload_bytes`: it confirms logical payload lifetime and cleanup, but excludes allocator metadata and fragmentation, stacks, generated code, VM/JIT internal allocations, and process RSS.
- Schema 13 introduced separate-process peak-memory sampling for the allocation-free linked AOT harness. The historical schema 14 macOS run reports a 1,654,784-byte minimum; a 1,671,168-byte median, p95, and maximum; and 0.173% RSD through `ru_maxrss`. The current macOS schema-20 run reports a 1,671,168-byte median after retaining Fibonacci in the linked set, while the Windows schema-20 run reports a 3,207,168-byte median through `PeakWorkingSetSize`. The separate rows retain the representative allocation estimate and component-instance evidence; the AOT row remains an allocation-free process envelope, and the two OS metrics must be interpreted independently.
- The VM rows include bytecode verification and register allocation on every invocation. They measure the current end-to-end embedding API, not only interpreter dispatch.
- The JIT rows include the checked Rust invocation wrapper. The direct Rust rows are narrow baselines, not predictions for representative programs.
- The object-emission rows include host ISA construction, MIR lowering, compilation, and object serialization. Schema 2's system-link row covers the harness, add, and minimum objects; schema 3 adds countdown, schema 6 adds the accumulator, and schema 7 adds conditional sum. All precompile the C harness, then time the platform driver and system linker (`cc` on macOS or `link.exe` on Windows MSVC).
- Schema 4's process-startup row includes process creation, loader work, the tiny harness workload, output collection, and waiting for termination. It neither flushes operating-system caches nor isolates loader time, and the executable-size field includes the platform C harness and default linker output.
- Schema 5 retains the single-launch row and adds a configurable batched row. The batched per-operation value reduces the leverage of one delayed launch on a sample, but it is an amortized throughput estimate rather than a replacement for individual startup latency.
- JIT add and minimum compilation have 66.655% and 19.507% RSD. Conditional-sum JIT/AOT medians are 40.500 us and 36.250 us with 7.178% and 10.384% RSD. These values are useful as directional evidence, but compile-time variation remains too high for a stable performance budget.
- The single-launch AOT startup median is 1.923 ms and p95 is 3.496 ms, but RSD is 232.018%. The ten-launch batched row reports a 1.822 ms median with 1.988 ms p95 and 5.362% RSD. Batching materially reduces host-noise leverage for an amortized estimate, while the single row still exposes tail-sensitive latency.
- Alternating between a 64-iteration back-edge and immediate return gives countdown JIT/VM medians of 12.846 ns and 1.611 us and accumulator JIT/VM medians of 12.790 ns and 1.665 us. Alternating conditional-sum inputs `(64, 33)` and `(-7, 33)` gives JIT/VM medians of 20.011 ns and 3.046 us. These bounded microbenchmarks do not predict representative control-flow performance. Near-one-nanosecond direct add, countdown, and host-call rows remain timer-resolution baselines, not credible throughput claims.
- This baseline supports continuing to use Cranelift for the prototype while broader evidence is collected. It does not complete Milestone 0's backend decision.

The clean native Windows x86-64 schema-20 result and its comparison with macOS are now recorded. Together with the completed macOS AArch64 DWARF/dSYM and Windows-target CodeView/PDB source-mapping criteria, this closes the planned cross-platform evidence prerequisite for the initial native-backend decision. The schema 17 allocation trace retained by schema 20 is still a synthetic compiler-like proxy, and its combined estimate cannot separate allocator metadata from fragmentation. The batched startup row is suitable for directional amortized evidence, not a tail-latency guarantee, and executable-size results are harness artifacts rather than release packaging budgets.

## Earlier baseline

The [schema 14 result](../benchmarks/results/2026-10-16-macos-aarch64-6a2caf8.json), measured 2026-10-16 14:02:20 JST from commit `6a2caf827de5b3a28746708cc36178c86a514a45`, predates component-boundary series and is superseded by schema 15. The [schema 13 result](../benchmarks/results/2026-10-16-macos-aarch64-11a1af7.json), measured 2026-10-16 13:00:19 JST from commit `11a1af7cac32625ebfad5a6bde921ac5382f320d`, predates allocation-churn timing/profile evidence and is superseded by schema 14. The [schema 12 result](../benchmarks/results/2026-10-16-macos-aarch64-fa9a69b.json), measured 2026-10-16 12:24:45 JST from commit `fa9a69b8650b6a2f4b15ea645ab0e18f522a8790`, predates process-level peak-memory distributions and is superseded by schema 13. The [schema 11 result](../benchmarks/results/2026-10-16-macos-aarch64-664edb9.json), [schema 10 result](../benchmarks/results/2026-10-16-macos-aarch64-133172b.json), [schema 9 result](../benchmarks/results/2026-10-16-macos-aarch64-e390a5d.json), [schema 8 result](../benchmarks/results/2026-10-16-macos-aarch64-6034dc8.json), [schema 7 result](../benchmarks/results/2026-10-16-macos-aarch64-0865167.json), [schema 6 result](../benchmarks/results/2026-10-16-macos-aarch64-bad17cf.json), [schema 5 result](../benchmarks/results/2026-10-16-macos-aarch64-180af85.json), [schema 4 result](../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json), [schema 3 result](../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json), [schema 2 result](../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json), [schema 1 AOT/link result](../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json), and [earlier JIT-only result](../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json) remain available for harness history.

## Sources

- [Current raw macOS AArch64 schema 20 result](../benchmarks/results/2026-10-16-macos-aarch64-de248a1.json), measured 2026-10-16 23:34:00 JST.
- [Historical raw macOS AArch64 schema 19 result](../benchmarks/results/2026-10-16-macos-aarch64-76c8f81.json), measured 2026-10-16 22:51:15 JST.
- [Historical raw macOS AArch64 schema 18 result](../benchmarks/results/2026-10-16-macos-aarch64-b5ddb9b.json), measured 2026-10-16 19:17:44 JST.
- [Historical raw macOS AArch64 schema 17 result](../benchmarks/results/2026-10-16-macos-aarch64-aac146d.json), measured 2026-10-16 19:01:45 JST.
- [Historical raw macOS AArch64 schema 16 result](../benchmarks/results/2026-10-16-macos-aarch64-7f2482a.json), measured 2026-10-16 18:42:15 JST.
- [Historical raw macOS AArch64 schema 15 result](../benchmarks/results/2026-10-16-macos-aarch64-f163409.json), measured 2026-10-16 18:00:07 JST.
- [Historical raw macOS AArch64 schema 14 result](../benchmarks/results/2026-10-16-macos-aarch64-6a2caf8.json), measured 2026-10-16 14:02:20 JST.
- `crates/vm/src/lib.rs`, deadline-enabled VM resource controls, last updated 2026-10-16 11:20 JST.
- `src/lib.rs`, cross-mode CFG and allocation-callback prototypes, last updated 2026-10-16 19:15:05 JST.
- `src/component_application_conformance.rs`, representative multi-component application fixture, last updated 2026-10-16 23:29 JST.
- `src/bin/prototype-bench.rs`, schema 20 CFG/allocation/component/process-memory harness, last updated 2026-10-16 23:29 JST.
- [macOS AArch64 generated-object inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md), captured 2026-10-16 00:25 JST.
- [macOS AArch64 signed-minimum inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md), captured 2026-10-16 00:43:31 JST.
- [macOS AArch64 AOT debug-information inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md), captured 2026-10-16 12:06:01 JST.
- [macOS AArch64 source-mapped DWARF inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md), captured 2026-10-16 19:41:57 JST.
- [Windows x86-64 source-mapped CodeView/PDB inspection](../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md), captured 2026-10-16 20:19:36 JST.
- Repository commit `76c8f81516be5eb4cd77ab063b3ecd3935d16e58`, committed 2026-10-16.
- Repository commit `b5ddb9b6ed0e8339935c55df2a138233209f13d0`, committed 2026-10-16.
- Repository commit `aac146dad79cc02518c809d0bb203559799f72db`, committed 2026-10-16.
- Repository commit `7f2482a52c11354d29c61e68608f85a1f7c9611c`, committed 2026-10-16.
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
- Microsoft Learn [`GetProcessMemoryInfo`](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-getprocessmemoryinfo) and [`PROCESS_MEMORY_COUNTERS`](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters), published 2018-12-05 and last updated 2024-02-22 19:54 UTC; verified 2026-10-17. They document `K32GetProcessMemoryInfo` and define `PeakWorkingSetSize` as the process's peak working-set size in bytes.
