<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0013-service-byte-work-budget.md -->

# RFC 0013: Service byte-work budget

[日本語版](../ja/rfcs/0013-service-byte-work-budget.md)

- Status: accepted
- Authors: Codex, under the owner's experimental delegation
- Created: 2026-10-19
- Last updated: 2026-10-19
- Accepted: 2026-10-19
- Tracking issue: M2-N15, M2-N05
- Supersedes: None; elaborates RFC 0012's configured service admission
- Superseded by: None

## Summary

Add an optional cumulative service-work limit to the instrumented execution context. VM/native share charging events and usage. Exhaustion is a terminal control failure followed by mandatory cleanup. N15 decides this contract; N05 implements it.

## Motivation

Current Control has fuel, host-call, frame and live-cell limits, without service-work configuration. Map already bounds candidate visits, UTF-8 validation and comparison before those actions; buffers perform variable-length copies/scans. Instruction fuel does not distinguish payload sizes. These are implementation facts, not performance measurements.

The existing Map suite passes 20 tests, including work refusal without retaining queries, allocating, changing owners or publishing output. This demonstrates useful runtime admission boundaries, not an implemented global meter.

## Goals

Share one meter across logical guest calls/backends; preserve inputs, spans and cleanup on refusal; retain local quotas and allocator accounting; expose configuration and usage to hosts/drivers.

## Non-goals

New syntax/effects, compile-time metering, CPU-time guarantees, metering arbitrary trusted host code, changing instruction fuel, or making native execution a sandbox. Allocator zeroing and spare capacity remain covered by allocation limits.

## Guide-level design

Add Control::with_limits(existing_limits).and_service_work(Some(64)), with None/unbounded as the default, Usage.service_work: u64, and execution CLI option --service-work N. Unsupported profiles reject finite budgets before guest effects.

For a three-byte input already owned by an entry, a clone payload-copy event costs 3 and a following UTF-8 validation event costs 3. Limit 5 refuses the second event as ServiceWorkLimit { limit: 5, used: 3, requested: 3 }. Its bytes are not scanned; admitted temporary/input owners retire during failure cleanup. These two events exclude unrelated service events.

## Reference semantics

### Context and events

Start at zero for each outer invocation. Logical calls and core-service callbacks forward the same meter; independent instances have independent counters. The synchronous context owns it without ambient globals or new atomics/capabilities.

Cost profile 1 counts logical events, not wall time or physical memory traffic. Bulk events charge their full lengths even if an implementation could stop early.

| Event | Units |
| --- | --- |
| Observe one managed payload/graph node or Map lookup candidate | 1 |
| Copy n payload bytes, including typed data copied by a core service | n |
| Validate n UTF-8 bytes | n |
| Compare equal-length byte sequences of length n | n |
| Compare different-length sequences using headers | 0 |
| Successful Bytes get/set, or a push's appended byte | 1 |
| Fixed header access, handle transfer, allocator bookkeeping | 0 |

Empty byte events cost zero. Growing Bytes push also charges the old logical bytes copied into new storage. Clone/concat charge copied logical lengths, excluding allocator initialization and unused capacity. Text len uses its checked header and costs zero; conversion validates the input length. Vec/Map/shared/arena services charge their actual payload copies, observations and Text inspections. Buffer handle transfers alone do not charge underlying payload length.

Charge once at the shared runtime boundary; wrappers must not duplicate an event. Repeated distinct service traversals charge their events again. Reuse Map's observation/byte boundaries. Keep existing local quotas: check both local and invocation budgets before the event and commit both counters only after both checks succeed. An existing local refusal retains its existing error.

Both backends use identical events within a cost-profile revision. Event changes require an explicit RFC revision and compatibility note.

### Admission and failure

Keep existing guest-step polling/fuel positions; the service meter adds no polls. Perform preceding fixed input/header/type/brand/bounds checks, calculate the event's units and admitted + requested with checked arithmetic, then compare the finite limit. Overflow yields CounterOverflow; exceeding the limit yields ServiceWorkLimit { limit, used, requested }. Refusal executes no part of that event and does not increment service_work. Earlier admitted work is not refunded after later refusal, provider failure or a recoverable data error. Unbounded execution still counts work and checks overflow.

The guest operation's fuel charge comes first. Its interruption/deadline/fuel precedence is unchanged. An admitted scan can discover its normal data error; a refused scan cannot discover a later data error.

ServiceWorkLimit is a structured shared execution/control failure, not a core Result error, guest panic or invariant poison. Preserve its numeric fields and first failing FunctionId/source operation span through gateway and report. Preserve ordered secondary cleanup failures. Mandatory retirement, result discard and host resource release never consult/charge this meter and cannot replace an existing primary failure.

Admit an event before its dependent allocation/copy/scan. A multi-stage intrinsic may have an admitted provider prefix before a later refusal; allocator request/cumulative statistics are not rolled back. Retire every provisional resource. No unadmitted event may publish a result, replace a source place or consume an input token. Move inputs only after required fallible inspection/admission, using prepared publication and retirement. VM/native preserve the same provider prefix and ownership outcome.

Batch known dependent costs before mutation or allocation when a later refusal could leave a stale input handle or an unregistered temporary. In particular, admit a growing push's old-byte copy plus append, and a clone/concat's complete copy length, before allocating or replacing storage. Refuse the whole batch without changing either counter; report its combined requested units. Variable inspections may charge a prefix only while inputs remain owned and provisional resources remain covered by cleanup.

### Other semantics

Typing, inference, evaluation order, loans, arena brands, concurrency and capabilities are unchanged. Constant/compile-time execution is outside this budget. Foreign inputs still require checked storage/UTF-8 validation; budget refusal grants no authority to inspect unchecked data.

## Execution modes

Instrumented VM/native AOT/owned JIT share events and service_work. An incompletely metered backend rejects finite configuration before effects, without running a supported prefix or substituting VM execution. Uninstrumented Copy remains a distinct API/profile.

Opaque trusted host callbacks retain host-call charging; arbitrary host work is not inferred from arguments. Callbacks using owned services in the same context forward the meter. FFI/components propagate budgets only through explicit versioned contexts; this Rust API does not automatically meter another component's internals.

## ABI and compatibility

Default unbounded execution retains source behavior/fuel positions apart from defined counter overflow. No syntax, type, package, edition or typed-source artifact change is needed. Store the limit privately in Control so existing Limits literals remain valid. Exhaustive Rust Usage literals must initialize service_work or use Default.

Add ServiceWorkLimit with numeric fields to the shared dynamic failure domain, without reusing another identity. Current artifacts store pre-failure-elaboration data, not dynamic execution causes. N05 must audit serializers again: versioned reports/interfaces encoding new usage/errors require a format bump or support negotiation and migration fixtures. Never append tags/fields under an unchanged closed schema. Private revision-1 native packets remain unchanged; Rust context layouts are not a public C ABI. FFI/component mappings remain independently versioned.

## Diagnostics and tooling

Report the failure identity, source span, limit, admitted and refused units; keep logical guest frames and secondary ordering. Document the builder, CLI option, usage and Rust migration. Parser/formatter/type inference/LSP grammar do not change. DAP displays the existing logical chain/span with the new failure data.

## Performance and cost model

One counter check per event requires no guest allocation or registry; existing service iteration supplies boundaries. Include enlarged usage/failure types in metadata/report memory budgets. No overhead or speed advantage is claimed. Measure representative mixed services under N05 and the existing performance gates.

## Security and trust boundaries

This complements fuel, memory and local inspection quotas; it does not bound all CPU work, allocator initialization, opaque host callbacks or mandatory cleanup. Use checked lengths and explicit allocator/host grants. Native callbacks stay trusted and panic-contained. Exhaustion must not prevent cleanup or resume a poisoned instance.

## Alternatives

- No change leaves N05's configured-work requirement unimplemented.
- Library-only counters cannot share invocation limits/usage/cleanup without context integration.
- Byte-weighted instruction fuel changes the accepted one-step fuel/poll contract.
- A conservative estimate per intrinsic is simpler but misses nested Map/graph work or charges unused capacity and duplicates runtime policy.
- Full preplanning offers stronger all-or-nothing provider admission but adds temporary plans/traversals; this design permits an admitted prefix with ownership-safe cleanup.

## Implementation plan

1. N15 decides the RFC using current service/failure boundaries and compatibility evidence.
2. N05 adds shared meter/builder/usage/failure data and driver configuration, then wires buffer and all managed service events while preserving local quotas. Reject finite configuration for incomplete profiles.
3. N05 verifies full success/refusal/panic/cleanup and migration coverage. N06/N07 still require actual target object link/run evidence. Do not introduce frontend/backend coupling or new serialized IR.

## Test plan

- Zero/exact/one-below/u64-overflow admission; refused events preserve counter/input.
- Multi-stage prefixes, growth OOM, meter sharing across guest calls and unmetered mandatory cleanup.
- All buffer/Vec/Map/shared/arena payload families, nested Text, Copy/ownership/replacement and provider prefixes. Compare VM/native usage, numeric failure data/spans, ordered cleanup and zero leaked allocations.
- Existing local refusal, malformed foreign inputs, interruption/deadline/fuel precedence, panic and poison.
- Default unbounded compatibility, Rust literal migration, closed schemas and unsupported-profile preflight.
- Existing parser/MIR/artifact/platform conformance and representative performance checks; no parser snapshot change solely for this host option.

## Open questions

Optional follow-ups: measure event-check overhead and evaluate later cost profiles. N05 must prove complete event coverage and rollback behavior before claiming implementation; the 20 Map tests establish feasibility only.

## Decision review

Accepted under experimental delegation on 2026-10-19 JST. No blocking design question remains: shared runtime events avoid divergent formulas; local checks precede counter commits; known dependent growth/copy costs are batched before handle mutation; terminal failure leaves mandatory cleanup available. The explicit Rust migration and closed-schema rule cover compatibility without changing source typing or fuel.

Unverified assumptions are implementation coverage, safe integration of every provisional-resource path, and measured overhead. These remain N05 validation obligations, not claims of implementation or stabilization. Existing Map boundary tests support feasibility only. A later contract change requires an explicit RFC revision/follow-up before implementation.

## Sources

The normative choices above are project decisions. These local facts were verified 2026-10-19 JST; no external language or performance claim is used.

- [RFC 0007](0007-source-ownership-and-core-values.md), updated 2026-10-18; [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md), updated 2026-10-17; [RFC 0012](0012-native-ownership-continuations.md), updated 2026-10-18.
- [Shared controls](../../crates/runtime/src/execution.rs), last committed 2026-10-19; [buffer](../../crates/runtime/src/buffer.rs) and [Map work](../../crates/runtime/src/map.rs), last committed 2026-10-18.
- [Map storage tests](../../crates/runtime/tests/support/map_storage.rs) and [mutation refusal tests](../../crates/runtime/tests/support/map_mutation_failures.rs), last committed 2026-10-18. Readiness command cargo test -p lang-runtime --test map: 20 pass, 0.06 s.
- [Failure domain](../../crates/mir/src/unit/ownership/failure.rs) and [artifact codec](../../src/artifact.rs), last committed 2026-10-18. Repeat the serializer audit during N05.
