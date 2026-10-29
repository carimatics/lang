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

Accepted under RFC 0001 experimental delegation, 2026-10-30 JST. Reusing existing compilation and a closed bounded report avoids changing language semantics. E01 production integration, platform clock observations and E02–E04 attribution/trace/rebuild contracts remain unverified. The coarse experiment establishes feasibility only.

## Sources

- [Rust 1.99.0 Instant](https://doc.rust-lang.org/1.99.0/std/time/struct.Instant.html): page update date unavailable; verified 2026-10-30 JST. Monotonic measurement does not guarantee a steady clock; checked differences expose invalid observations.
- RFCs 0034/0035 and current package/frontend/artifact APIs, inspected 2026-10-30 JST; code has no embedded update date. Report bounds, fields and CLI behavior are project decisions.
- Ignored `target/m4-e01-compiler-cost-probe.log`, observed 2026-10-30 JST, with the matching SDK and one actual hello compilation; no performance comparison is claimed.
