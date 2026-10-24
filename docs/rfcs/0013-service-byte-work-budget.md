<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0013-service-byte-work-budget.md -->

# RFC 0013: Service byte-work budget

[日本語版](../ja/rfcs/0013-service-byte-work-budget.md)

- Status: implemented
- Authors: Codex, under the owner's experimental delegation
- Created: 2026-10-19
- Last updated: 2026-10-24
- Implemented: 2026-10-20
- Accepted: 2026-10-19
- Tracking issue: M2-N15, M2-N05, M2-N18
- Supersedes: None; elaborates RFC 0012's configured service admission
- Superseded by: None

## Current implementation status

Verified 2026-10-20 JST against the [M2 completion audit](../milestone-2-capabilities.md). The shared meter, CLI/host configuration, structured failures and full accepted managed-service event set are implemented in VM/native. Linked target and both-host evidence now completes the previously pending N06/N07 boundary (M2 G14/G16/G17). This does not establish a CPU-time bound or isolate meter overhead. This is implemented, not stabilized.

The motivation, original staged plan and dated partial checkpoints below record the decision/rollout history. Their former pending gates are closed by the evidence above; normative contracts and explicit non-goals remain in force. This status reconciliation changes no source, ABI or format semantics.

## Summary

Service follow-up decision (2026-10-22 JST): [RFC 0026](0026-source-isolated-services.md) reuses channel capacity/mint/clone/send/receive attempt/commit charges and existing actual managed observation, typed replacement and publication events. Fixed service role/generation checks, explicit handler finish, cancellation notification, wake/unlink and mandatory queue/state retirement are unmetered bookkeeping. Guest fuel/polls remain; state operations do not invent payload copies or duplicate frame-marshalling events. This additive unavailable source profile changes no previously admitted program's charges, artifact tags or cost-profile encoding; both adapters must prove the same numeric usage before admission.

C01 follow-up decision (2026-10-21 JST): [RFC 0025](0025-bounded-channels-and-backpressure.md) assigns new channel service-work events: capacity units before queue preparation, one before explicit endpoint mint/clone/seal, one per send/receive attempt and one per commit. Resumption/wakeup does not repeat attempt/commit charges; existing publication inspection remains charged. Mandatory closure, cancellation unlink, wake notification and retirement remain unmetered bookkeeping. Source/VM/native channel execution must expose identical numeric caps/usage before promotion. Existing service event costs are unchanged.

Add an optional cumulative service-work limit to the instrumented execution context. VM/native share charging events and usage. Exhaustion is a terminal control failure followed by mandatory cleanup. N15 recorded this contract; N05 implemented the managed-service integration.

### Component adapter follow-up — 2026-10-24

Accept [RFC 0029](0029-component-adapter-work.md) under RFC 0001's delegation, recording the catalog required by RFC 0027 before P02 admission. Existing M2 profiles remain implemented; the component extension implements original-counter sharing and scoped canonical processing. The board tracks P02's full validation/commit acceptance.

| Event | Units |
| --- | --- |
| Observe a dynamic value node during component translation/codec traversal | 1 |
| Runtime payload/frame/report copy | Bytes copied |
| UTF-8 validation | Bytes validated |
| Typed managed collection data copy/grow/replace | Existing Layout/active-extent rules |
| Fixed header/slot/claim/loan bookkeeping and opaque handle transfer | 0 |

Forward the original root invocation's same ServiceWork through argument preparation, Machine, native adapters and result publication. Metadata/signature/plans retain existing cumulative finite preparation Budget/Session limits and add no runtime event. Actual distinct value traversals/copies/validations are distinct events; do not duplicate a managed service event. Refuse finite component execution until all events use that original counter. Mandatory cleanup remains unmetered; old non-component program costs, fuel/poll order and wire/ABI fields/tags are unchanged. RFC 0029 specifies refusal, root identity, Rust API migration and validation requirements.

## Motivation

At proposal time (2026-10-19), Control had fuel, host-call, frame and live-cell limits, without service-work configuration. Map already bounds candidate visits, UTF-8 validation and comparison before those actions; buffers perform variable-length copies/scans. Instruction fuel does not distinguish payload sizes. These are implementation facts, not performance measurements.

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

For typed transfers, the data extent is the shared runtime Layout::size, including canonical inline padding and inactive variant storage. Separate initialization/order/retirement metadata is excluded. Moving one standalone core token is a fixed handle transfer and costs zero; copying an active collection's bulk data charges active length times its data stride, including stored handles, while spare collection capacity is excluded. A replacement charges each logical typed transfer, without counting an implementation's temporary swap buffer as another transfer. This permits checked whole-batch admission before allocation/publication without first walking payloads to estimate a smaller size.

Managed payload validation/inspection and reads through managed service views charge one unit per observed payload node; fixed cursor/tag/header checks and guest-frame marshalling do not create additional service events. A view's bulk typed copy charges its data extent. Both adapters must select matching logical runtime operations and forward one invocation meter through loans/views. Shared input checks and internal metadata revalidation must not duplicate one logical validation event; distinct publication/graph traversals still charge again. Mandatory retirement and all of its internal views remain unmetered.

### Admission and failure

Keep existing guest-step polling/fuel positions; the service meter adds no polls. Perform preceding fixed input/header/type/brand/bounds checks, calculate the event's units and admitted + requested with checked arithmetic, then compare the finite limit. Overflow yields CounterOverflow; exceeding the limit yields ServiceWorkLimit { limit, used, requested }. Refusal executes no part of that event and does not increment service_work. Earlier admitted work is not refunded after later refusal, provider failure or a recoverable data error. Unbounded execution still counts work and checks overflow.

The guest operation's fuel charge comes first. Its interruption/deadline/fuel precedence is unchanged. An admitted scan can discover its normal data error; a refused scan cannot discover a later data error.

ServiceWorkLimit is a structured shared execution/control failure, not a core Result error, guest panic or invariant poison. Preserve its numeric fields and first failing FunctionId/source operation span through gateway and report. Preserve ordered secondary cleanup failures. Mandatory retirement, result discard and host resource release never consult/charge this meter and cannot replace an existing primary failure.

Admit an event before its dependent allocation/copy/scan. A multi-stage intrinsic may have an admitted provider prefix before a later refusal; allocator request/cumulative statistics are not rolled back. Retire every provisional resource. No unadmitted event may publish a result, replace a source place or consume an input token. Move inputs only after required fallible inspection/admission, using prepared publication and retirement. VM/native preserve the same provider prefix and ownership outcome.

Batch known dependent costs before mutation or allocation when a later refusal could leave a stale input handle or an unregistered temporary. In particular, admit a growing push's old-byte copy plus append, and a clone/concat's complete copy length, before allocating or replacing storage. Refuse the whole batch without changing either counter; report its combined requested units. Variable inspections may charge a prefix only while inputs remain owned and provisional resources remain covered by cleanup.

### Other semantics

Typing, inference, evaluation order, loans, arena brands, concurrency and capabilities are unchanged. Constant/compile-time execution is outside this budget. Foreign inputs still require checked storage/UTF-8 validation; budget refusal grants no authority to inspect unchecked data.

## Execution modes

Instrumented VM/native AOT/owned JIT share events and service_work. Before effects, finite configuration requires complete metering for the entire verified unit: inspect every function, including unused functions, with a shared VM/native capability predicate. Reject a unit that requires any unmetered service event; do not run a supported prefix or substitute VM execution. Dynamic service targets must be resolved and checked or rejected. The existing trusted opaque host-callback exemption remains. Uninstrumented Copy remains a distinct API/profile.

During rollout, a backend may admit a unit using only fully metered service families. This does not establish full backend support: N05 still requires all managed families, all payloads, driver/report migration, and the complete conformance matrix. Text token decoding performs UTF-8 validation and must charge that scan even when the following operation only reads a header.

Opaque trusted host callbacks retain host-call charging; arbitrary host work is not inferred from arguments. Callbacks using owned services in the same context forward the meter. FFI/components propagate budgets only through explicit versioned contexts; this Rust API does not automatically meter another component's internals.

## ABI and compatibility

Default unbounded execution retains source behavior/fuel positions apart from defined counter overflow. No syntax, type, package, edition or typed-source artifact change is needed. Store the limit privately in Control so existing Limits literals remain valid. Exhaustive Rust Usage literals must initialize service_work or use Default.

Metered buffer entry points are additive; existing buffer methods retain their unmetered host/cleanup behavior. Buffer Fault::ServiceWork carries the shared structured failure, so exhaustive Rust matches on Fault need that branch. It has no new serialized representation in this stage.

Managed services use an optional cloned ServiceWork handle backed by Rc/Cell for one synchronous invocation. Prepared Vec/Map/shared/arena services, registered loans and views retaining this handle are no longer Send/Sync, even when their optional meter is absent. Rust embedders must construct these execution contexts on the execution thread; interruption flags retain their independent atomic cross-thread signaling contract. Existing unmetered methods remain available for trusted host work and mandatory cleanup. Runtime error enums gain structured service-work variants and source chains; exhaustive Rust matches must handle them. These are Rust API changes, without a source-language, serialized-format or C ABI change.

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
2. N16 adds meter/builder/usage/failure data, N17 buffer metering, and N18 CLI configuration/report-format audit. N05 wires every managed service event while preserving local quotas. Reject finite configuration for incomplete units.
3. N05 verifies full success/refusal/panic/cleanup and migration coverage. N06/N07 still require actual target object link/run evidence. Do not introduce frontend/backend coupling or new serialized IR.

Implementation status (2026-10-19 JST): N16/N17/N18 and N05 cover all current owned VM and canonical native service families. One invocation counter crosses guest calls/loans/views; structured refusal and unmetered retirement preserve numeric details and cleanup. Twenty-four sources compare every finite work prefix; tests also cover mixed controls/provider failures, guest panic, secondary cleanup, local quotas and source/artifact CLI behavior. CLI settings remain execution-only. Copy schema 1 / ownership schema 2 artifacts, unversioned REPL display and private revision-1 native packets gain no wire fields/tags. Native driver/target execution and platform performance remain N06/N07 and later gates; this RFC stays accepted pending those boundaries.

## Test plan

- Zero/exact/one-below/u64-overflow admission; refused events preserve counter/input.
- Multi-stage prefixes, growth OOM, meter sharing across guest calls and unmetered mandatory cleanup.
- All buffer/Vec/Map/shared/arena payload families, nested Text, Copy/ownership/replacement and provider prefixes. Compare VM/native usage, numeric failure data/spans, ordered cleanup and zero leaked allocations.
- Existing local refusal, malformed foreign inputs, interruption/deadline/fuel precedence, panic and poison.
- Default unbounded compatibility, Rust literal migration, closed schemas and unsupported-profile preflight.
- Existing parser/MIR/artifact/platform conformance and representative performance checks; no parser snapshot change solely for this host option.

## Open questions

Optional follow-ups: release-mode overhead measurements and later cost profiles. The manual managed_service_budget_timing_observation test reuses one compiled Vec/Text/shared/arena program using 228 work units. Five samples of 16 invocations after two warmups gave debug-profile medians: VM unbounded 4.663 ms / finite 4.652 ms; native unbounded 4.644 ms / finite 4.593 ms. Per-invocation storage/teardown is included; source/native compilation is excluded. This verifies stable usage without establishing meter overhead, a speed advantage, a Rust/C comparison or platform performance. Raw timing source: /tmp/lang-n05-timing.log, verified 2026-10-19 JST.

## Decision review

Accepted under experimental delegation on 2026-10-19 JST. No blocking design question remains: shared runtime events avoid divergent formulas; local checks precede counter commits; known dependent growth/copy costs are batched before handle mutation; terminal failure leaves mandatory cleanup available. The explicit Rust migration and closed-schema rule cover compatibility without changing source typing or fuel.

Current conformance establishes event/refusal/rollback behavior for the documented owned profiles. Release-mode overhead, linked target drivers and platform execution remain unverified boundaries. A later cost-contract change still requires an explicit RFC revision/follow-up before implementation.

### Admission amendment — 2026-10-19

Accepted under experimental delegation after reviewing the buffer integration prototype. Whole-unit admission permits independently verifiable progress while preserving rejection before effects for unsupported programs. Compared with rejecting every unit until the last family is metered, this narrows the admitted program set without weakening the cost contract. Inspecting all functions avoids dependence on executed paths. The shared predicate and end-to-end finite-budget, unused-function, failure-span, and cleanup tests are implementation obligations; prototype feasibility alone does not close N05. No source syntax, serialized format, or cost-profile event changes. This explicit amendment precedes the production integration commit.

### Typed-transfer amendment — 2026-10-19

Accepted under experimental delegation during N05's runtime audit. Shared typed extents provide a deterministic VM/native cost without an unadmitted active-field prewalk. Charging allocation sizes would include spare capacity/initialization excluded by this RFC; summing active fields would require a separate metered traversal and a more complex transactional plan. The chosen rule preserves the existing buffer events and 37-unit fixture, adds no serialized field/tag, and is recorded before managed finite-budget profiles are enabled. The shared counter/atomic-quota prototype supports feasibility only. Typed copy/growth/replacement boundaries, loan/view propagation, prefix refusal, numeric errors, and VM/native equivalence remain implementation obligations; this amendment does not close N05.

## Sources

The normative choices above are project decisions. These local facts were verified 2026-10-19 JST; no external language or performance claim is used.

- [RFC 0007](0007-source-ownership-and-core-values.md), updated 2026-10-18; [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md), updated 2026-10-17; [RFC 0012](0012-native-ownership-continuations.md), updated 2026-10-18.
- [Shared controls](../../crates/runtime/src/execution.rs) and [buffer](../../crates/runtime/src/buffer.rs), last committed 2026-10-19; [Map work](../../crates/runtime/src/map.rs), last committed 2026-10-18.
- [Map storage tests](../../crates/runtime/tests/support/map_storage.rs) and [mutation refusal tests](../../crates/runtime/tests/support/map_mutation_failures.rs), last committed 2026-10-18. Readiness command cargo test -p lang-runtime --test map: 20 pass, 0.06 s.
- [Failure domain](../../crates/mir/src/unit/ownership/failure.rs) and [artifact codec](../../src/artifact.rs), last committed 2026-10-18. Repeat the serializer audit during N05.
- [CLI](../../src/main.rs), [REPL transport](../../src/repl/owned.rs) and [CLI conformance](../../tests/service_work_cli.rs), updated/verified 2026-10-19. N18 audits the existing source/artifact/REPL configuration and report surfaces; it does not establish complete native/managed or future wire-format conformance.
- [Managed metering](../../crates/runtime/src/execution.rs), [typed transfers](../../crates/runtime/src/layout/storage/transfer.rs), [VM adapter](../../crates/vm/src/unit/owned.rs), [native conformance](../../tests/support/native_control.rs) and [managed Text example](../../examples/analysis/native_managed_text_work.lang), updated/verified 2026-10-19. The N05 audit confirms execution-only numeric usage/failures; current artifact and private packet schemas are unchanged.
