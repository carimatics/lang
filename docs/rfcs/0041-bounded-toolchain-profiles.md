<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0041-bounded-toolchain-profiles.md -->

# RFC 0041: Bounded toolchain profiles

[日本語版](../ja/rfcs/0041-bounded-toolchain-profiles.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-30
- Last updated: 2026-10-30
- Tracking issue: M4-E01–E04
- Supersedes: none
- Superseded by: none

## Summary

Add `lang profile compile --manifest-path Lang.json` to observe the ordinary package compiler through a bounded, versioned JSON report. Runtime profiles, allocator traces and rebuild explanations follow as separately verified report kinds; accepting this contract does not implement or close G8.

## Motivation

The existing pipeline provides frozen package inputs, structured diagnostics and verified portable artifacts. A standalone exact-SDK experiment on 2026-10-30 JST compiles hello through the existing frontend and lowering/verification, observing 2.684 ms and 0.212 ms respectively. This proves an observation boundary, not a benchmark, overhead estimate or speedup. Users need reproducible input identities and explicit measurement scope before comparing costs.

## Goals

- Observe real compiler operations without a second compilation path or guest execution.
- Preserve package authority, diagnostics, semantic results and existing limits.
- Version and bound reports, and distinguish elapsed observations from deterministic counters.

## Non-goals

- Optimizer changes, remote telemetry, automatic source upload or statistical performance claims.
- CPU-time accounting, per-parser/type-checker subphase attribution or compilation-cache claims.
- New source syntax, execution grants, stored program schemas or public C ABI changes.

## Guide-level design

`lang profile compile [--manifest-path FILE] [--package NAME] [--target TRIPLE] [--message-format human|json]` defaults to exactly `./Lang.json` and the host target. Success writes one complete report to stdout; diagnostics use stderr and retain the existing codes and spans. It does not publish artifacts, create or replace Lang.lock, execute main or grant host imports. Explicit workspace selection follows the ordinary package contract.

## Reference semantics

Compile reports have `schema: 1`, `kind: "compile"`, `clock: "host_monotonic_elapsed_ns"`, `compiler_id`, `target`, `input_digest`, `source_files`, `source_bytes`, `artifact_bytes`, `elapsed_ns` and `phases`. Each phase has `name` and `elapsed_ns`. The ordered phase names are exactly `package_capture`, `frontend`, `lower_verify_encode`.

Capture includes graph admission, freezing inputs and the ordinary canonical lock calculation. Frontend includes package module parsing, binding, typing and ownership analysis. Lower/verify/encode calls the existing complete portable compiler. These are coarse, sequential boundaries; elapsed_ns is their checked sum and excludes report encoding and unrelated bookkeeping. No duration is attributed to a finer operation than actually observed. The input digest is SHA-256 of the canonical captured lock bytes; compiler_id is the existing host SDK build identity, a compatibility observation rather than authentication.

Use checked monotonic clock differences and checked conversion/sums to u64 nanoseconds; refuse an invalid observation. A zero duration is valid and does not establish clock precision. Reports are at most 4 MiB, with exactly three compile phases, at most 4,096 source files, 32 MiB source bytes and 64 MiB artifact bytes. These are report policy bounds in addition to existing compiler limits. Validate complete JSON before writing; failed compilation emits no successful partial report. Readers reject unknown/duplicate fields, unsupported schema/kind/clock, invalid identities/targets, phase order/count, overflow and resource excess. Reading a report grants no source or execution authority.

Runtime/trace/rebuild kinds require an RFC elaboration and executable evidence before implementation. In particular, native attribution cannot be supplied by VM replay, provider counters cannot be replaced by invented allocation events, and incomplete observations cannot claim a complete trace. Rebuild explanations must identify actual input changes rather than infer cache behavior from timings.

### Runtime observation elaboration (2026-10-30 JST)

The exact-SDK `target/m4-e02-charge-point-probe.log` runs the same owned vector program through the original VM and actual compiled native JIT entry. Fuel limits 0/1/2/8 produce equal usage and original fault spans; completion returns 9 in both with fuel 20, host calls 0, service work 16, peak frames 1 and cells 30. Both original allocators retire all storage. This establishes existing charge/span boundaries, not production profiling or Windows acceptance.

Add a trusted optional borrowed point recorder to execution Control. Record only after an original fuel charge succeeds and before its effect, using that operation's existing optional source span. Preserve polling, fuel/service/host accounting, cleanup and primary failure order. Recorder identity participates in both configuration comparisons; preserve Control's Send/Sync properties. The recorder contains no user callback: reserve a bounded buffer before execution, use synchronized owned metadata, and never allocate while recording. Default capacity is 4,096 samples, maximum 65,536; the sampling interval is 1–65,536 and selects every Nth successful charge. Count every successful charge even when its optional span is absent. Buffer/counter/lock failure latches an observation error without interrupting or replaying the guest; snapshot/publication refuses rather than claiming a complete prefix.

`lang profile run` selects `--engine vm|native`, defaults to vm, and uses explicit package/host-target admission with no host imports. Initial VM modes are ordinary Copy and ownership; native mode is the original ownership JIT driver with its compiled code owner, without VM fallback. Tasks, C/component bindings and unmetered Copy native source-point profiling refuse before effects. Native AOT instruction profiling remains unsupported; the real JIT/native fixture satisfies this initial native runtime observation profile. Use the original finite fuel/frame/cell limits, with no guest capabilities added.

Run `cargo run -- profile run --manifest-path examples/packages/profile_runtime/Lang.json --engine native`; use `--engine vm` for the same captured source and `--sample-every 2` for deterministic subsampling. The owned vector/call fixture returns 42 after retiring its storage. `--observations N` selects the finite buffer capacity. Runtime limits use the ordinary `--fuel`, `--frames`, `--cells` and `--service-work` options. These observations never estimate unsampled counts or statistically unbiased hot spots.

Runtime reports use schema 1/kind runtime and explicitly identify engine, semantic profile, sampling interval, total successful charges, sampled/unattributed counts, per-source byte-span sample counts, original usage, result and host elapsed duration. Timed entry includes original execution/preflight/cleanup and instrumentation; code generation/preparation precedes that interval. Sample counts are frequencies of charged points, not CPU time, completed operations or estimated instruction costs. Interval 1 observes all successful charges when the buffer fits. Unknown spans remain unattributed. Sources are the same captured admitted program; report paths are relative observations and never become IO authority. Bound complete reports to 4 MiB and validate identities, admitted source ranges, totals and all finite counts before publication/reading. Compile-report fields/encoding remain unchanged.

Runtime schema-1 fields are `schema`, `kind`, `clock`, `compiler_id`, `target`, `input_digest`, `engine` (`vm` or `native_jit`), `profile` (`copy` or `ownership`), `measurement` (`successful_fuel_charges`), `sample_every`, `total_charged_points`, `sampled_points`, `unattributed_points`, `sources`, `points`, `usage`, `result` and `elapsed_ns`. A source has `id`, relative `path`, `bytes` and `digest`; a point aggregate has `source`, `start`, `end` and `samples`. Usage retains `fuel`, `host_calls`, `service_work`, `peak_frames` and `peak_live_cells`. Sources are unique and ordered by ID; aggregates are unique and ordered by source/start/end. Sampled totals equal floor(total/interval), aggregate samples plus unattributed samples, and fuel equals total charges. Native JIT requires ownership. Readers reject all unknown/duplicate fields, out-of-range spans/counts and inconsistent totals. Guest execution failure uses the ordinary diagnostic path and emits no successful runtime report.

Accept under RFC 0001 experimental delegation: the finite probe demonstrates shared spans, exact counters and cleanup with actual native code. Observation adds host locking, memory and timing overhead; deadlines still include real elapsed host work. Private Rust metadata/layout preparation costs may change; persisted formats and guest fuel/service/provider accounting do not. No timing or overhead equivalence is claimed. Original outcome/usage parity, sampled attribution, overflow behavior and both-host evidence remain required E02 tests. E03 allocator events and E04 rebuild explanations remain separate elaborations.

### Logical allocator trace elaboration (2026-10-30 JST)

The executable `target/m4-e03-allocator-boundary-probe.log` observes four authorized requests, three logical allocations and three retirements, while the provider sees only three attempts and two allocations/retirements. A zero-size owner still counts without a provider call; an authorized 64-byte OOM raises cumulative requests to 112 bytes without raising the 48-byte successful peak. Final live storage is zero. RFC 0008 already requires these semantics (last updated 2026-10-20). Provider callbacks alone cannot supply the requested complete counter trace.

Expose E03R (2 pt) before E03's remaining report/CLI/reader work (3 pt). E03R adds an optional bounded owned ledger on the original root Allocator and first actual VM/native fixtures. Bind it before any authorized request; reject reconfiguration, non-fresh state and borrowed views. Borrowed Memory views continue delegating to that original root, which records exactly once. Store original generation identities and requested size/alignment with the authoritative post-operation Stats. Record admitted allocation success, admitted OOM/invalid-storage failure after original live refunds, and successful retirement including zero-size owners and internal resize operations. Pure preflight refusals/checks with no authorized request are outside this event stream. Never reconstruct missing events from provider callbacks.

Reserve the trace buffer before guest entry (default 4,096 events, maximum 16,384). It retains metadata only, no region or byte contents, and invokes no callback or allocation while recording. Trace errors latch separately and never alter requests, counters, handles, provider calls, guest control/failure order or cleanup; incomplete snapshot/publication refuses. Preserve allocator identity and original borrowed-view authority. Trace attachment and retained metadata add host allocation/layout/locking costs; these are outside guest requested bytes and may not be called process RSS. Existing public C and stored program/SDK formats remain unchanged; private Rust SDK identity requires the normal rebuild.

E03 will publish a separately validated report kind after its precise fields/replay contract is elaborated. Its boundary is original entry return: reconcile the complete logical event ledger with that allocator's actual Stats at that boundary, including any still-live owners after failure. Do not force cleanup, invent physical allocations for zero-size owners, infer source/declaration locations from the last charge, or claim all host allocator teardown is captured. VM/native traces must execute the original engines with the same allocator, controls and original failure/retirement path.

Accept this prerequisite under RFC 0001 experimental delegation: the probe exposes a real missing observation boundary and existing RFC 0008 supplies its accounting/cleanup contract. Original operation/identity/counter parity, zero/OOM/invalid storage/resize/borrowed forwarding, bounded-buffer failures and both-host entry fixtures remain unverified implementation obligations. E03R adds two planning points; it does not retire E03 or close G8.

## Execution modes

Compile profiling uses all semantic profiles already admitted by the ordinary portable compiler, with their original entry/preflight rules. It runs no VM, native AOT, JIT or component code. Runtime modes remain E02/E03 obligations with explicit instrumentation/measurement limits.

## ABI and compatibility

The report is a new schema-1 host artifact. Existing source, editions, MIR, bytecode, package/lock, SDK and C/component formats are unchanged. A reader that does not know a future report kind must refuse it. Incompatible compile-report changes require a new schema; no existing program migration is required.

## Diagnostics and tooling

Reuse ordinary package/source diagnostics. CLI usage remains exit 2; compilation, report validation and observation failures exit 1. Reports expose no source text or ambient absolute paths. The initial CLI does not write a persistent profile file; callers may redirect stdout. Formatter/LSP/DAP semantics are unchanged.

## Performance and cost model

Clock reads, retaining canonical lock identity, report allocation and JSON encoding add host work. Timings include instrumentation and machine scheduling/I/O effects and must not be described as guest CPU time. Comparisons need repeated controlled experiments beyond this delivery. Ordinary unprofiled commands acquire no new timers or reports.

## Security and trust boundaries

Reuse explicit manifest/package/target admission and bounded immutable sources; do not use editor overlays or ancestor discovery. Compilation adds no guest capability. Reports are untrusted observations, never credentials or executable manifests. Bound parser input and validate all finite counts/identities before use.

## Alternatives

- No change leaves ad hoc benchmarks without a user-visible observation contract.
- External process timing is useful but cannot separate the admitted compiler boundaries or pin package inputs by itself.
- A general tracing framework offers finer observations but adds integration and buffering before a first executable report.
- One total stopwatch is simpler but hides capture versus frontend versus lowering costs. Three existing boundaries are sufficient for E01.

## Implementation plan

E01 delivers the compile report/reader and actual package CLI fixture. E02 observes real VM/native runtime work; E03 traces real allocator/retirement operations with bounded buffers; E04 explains actual rebuild inputs. Re-audit each remaining outcome before implementation. G8 remains open until all four are validated on supported hosts.

## Test plan

Compare ordinary and profiled artifact size/input identity, compile a real declared dependency and workspace selection, preserve source diagnostics and prior output, and reject malformed/version/phase/count/overflow reports. Do not assert exact elapsed values. Run consistency and the complete baseline for implementation; retain both-host CI evidence.

## Open questions

Accepted under RFC 0001 experimental delegation, 2026-10-30 JST. E01 compile reports are verified at source `e2f0c1d`/final head `4ee6043` with the complete local baseline and both-host CI. E02 final head `7baee25` passes the complete local baseline (170 suites / 2,485 tests), full CI `40951375087` and native CI `40951374893`; actual VM/native results, usage and original span observations agree. E03R original allocator trace implementation and E03/E04 report/rebuild contracts and evidence remain required. Single timings establish no performance comparison.

## Sources

- [Rust 1.99.0 Instant](https://doc.rust-lang.org/1.99.0/std/time/struct.Instant.html): page update date unavailable; verified 2026-10-30 JST. Monotonic measurement does not guarantee a steady clock; checked differences expose invalid observations.
- RFCs 0034/0035 (both last updated 2026-10-29), RFC 0008 (2026-10-20) and current package/frontend/artifact/allocator APIs, inspected 2026-10-30 JST; code has no embedded update date. Report bounds, fields and CLI behavior are project decisions.
- Ignored `target/m4-e01-compiler-cost-probe.log`, observed 2026-10-30 JST, with the matching SDK and one actual hello compilation; no performance comparison is claimed.
