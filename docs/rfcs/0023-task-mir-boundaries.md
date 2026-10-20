<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0023-task-mir-boundaries.md -->

# RFC 0023: Task MIR boundaries

[日本語版](../ja/rfcs/0023-task-mir-boundaries.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-21
- Last updated: 2026-10-21
- Tracking issue: M3-T11, M3-T04–T08
- Supersedes:
- Superseded by:

## Summary

Extend the existing ownership MIR contract with Task/TaskError types and an async function effect, separately from ABI. Type/declaration evidence is not task flow evidence. Until task instructions and independent flow proofs exist, reject this profile at the place/body boundary, so existing execution and artifact paths cannot erase its meaning.

T11 checkpoint (2026-10-21 JST): types, effects and declaration validation are implemented. Old place/body, runtime layout and bytecode writer paths explicitly refuse task metadata; old decoders restore synchronous effects and assign no new tags. In particular, the generic runtime token-layout fallback must not invent storage for Task/TaskError. Task instructions/flow/cleanup and VM/native task execution remain unimplemented; this RFC stays accepted while that contract is incomplete.

## Motivation

Source typing and suspension-loan proofs exist at `9f7012d`, but ownership MIR has neither these types nor an async effect. Its type, declaration, place, body, move/loan/region and cleanup wrappers prove different obligations. Pretending the source proof establishes MIR validity would make foreign inputs and backend behavior unsafe. A library cannot enforce this boundary.

## Goals

- Extend the existing ownership type/declaration graph while preserving spans and independent validation.
- Preserve synchronous programs and all existing encoded bytes and ABI identities.
- Make the next task-flow/VM/native deliveries explicit without a second task language or execution fallback.

## Non-goals

This decision assigns no wire tags, artifact revision, native frame layout or component async ABI. Channels, Send/Share derivation, source execution and stabilization remain separate M3 outcomes. Type-only evidence does not implement a coroutine.

## Guide-level design

Source spelling stays RFC 0022's `async fn`, `core::Task<T>` and `core::TaskError::Cancelled`. MIR has a backend-neutral async effect independent of Guest/CExport ABI. Old execution/publication paths report an unsupported task profile, even for unused metadata or a pure async function with only scalar values.

## Reference semantics

TaskError is an ordinary finite Copy error with one unit case, Cancelled. Task(T) is a non-Copy claim requiring retirement; it is not an ordinary child/frame owner. Dropping a claim relinquishes retrieval without detaching or cancelling the child. The task scope owns joins and retained results under RFC 0022.

Type validation propagates `contains_task` independently from loan/arena/allocator facts. Reject Task in fields, containers, loans, parameters/results and nested Task payloads. A Task payload cannot contain loans, arena values, allocator authority or another Task. A direct Task local requires an async function. Reject async C exports and Task/TaskError in existing C profiles. General transfer sendability remains the source/flow profile's obligation; type-only validation must not claim that all owned values are non-sendable.

Async effects have validated source spans within the function signature. Old decoded declarations default explicitly to synchronous. Type/declaration checks do not prove claim origins, suspension, cleanup, capabilities or execution. Initially the place verifier rejects any Task, TaskError or async declaration before it can construct a body proof. T04 replaces that refusal only with task-aware instructions and independent type/move/loan/region/scope/cleanup evidence. No ordinary synchronous call may invoke an async function while erasing its effect.

The instruction/continuation and scope-exit representation must be recorded in this RFC before its implementation. It must cover scoped spawn, consuming await, non-consuming cancel, yield/check points, return escrow, implicit joins and failure/cancellation cleanup; repeat source invariants independently. This boundary decision does not select a weaker substitute for those obligations.

## Execution modes

VM, JIT and AOT consume the same verified task contract when their adapters exist. Native task execution must compile continuations, not interpret MIR or fall back to VM. Until then, existing synchronous adapters reject task profiles before bindings, allocation or effects. Constant execution and component tasks remain unavailable under their current profiles.

## ABI and compatibility

The Rust MIR API is experimental. New enum variants and the effect field require internal builders/matches to migrate. Preserve existing constructor/case order where ordinals are used internally. Rust enum layout is not a wire ABI. Existing schemas retain their bytes and meanings; writers reject unsupported task types/effects rather than omit them. Readers never infer async from names or scalar signatures. A future encoded task profile requires an explicit accepted revision and malformed-input/migration tests before publication.

Native packages remain bound to their exact compiler/SDK build ID under RFC 0018. A compiler-source change requires rebuilding a native package with its matching SDK even when the semantic wire bytes are unchanged. Do not weaken startup mismatch rejection to accommodate a mixed build.

## Diagnostics and tooling

Use structured MIR errors with validated source spans for invalid task types/declarations and unsupported task profiles. Source SRC0027/SRC0028 identities stay unchanged. Type-only success grants no source-check success or debugger task support. Formatter and parser behavior remain RFC 0022's.

## Performance and cost model

Reuse the bounded ownership type graph, canonicalization, property propagation and declaration budgets. Charge new traversal/storage work to existing limits. No runtime cost, frame-size or code-size claim follows from this type-only change. Task/frame/result costs remain measured execution requirements of M3.

## Security and trust boundaries

Validate raw MIR independently of HIR, including IDs, spans, cycles, storage, signatures and limits. No source metadata supplies scheduler capability. A type/declaration wrapper cannot authorize execution, serialization or native publication. Refusal must cover unused metadata so backends cannot discard a hidden async effect.

## Alternatives

- No change: leaves all source task work unavailable and does not advance the required execution path.
- Library/tool support: cannot validate foreign MIR or preserve async meaning through backend/artifact boundaries.
- A separate task MIR/type system: could isolate changes, but would duplicate ownership semantics and adapters before the first source fixture.
- Add task operations directly to the old executable profile: simpler wiring but would admit unproved suspension/cleanup or silently change serialized meaning.

The selected extension reuses ownership MIR and keeps a strict evidence boundary until task flow is proved.

## Implementation plan

1. T11: validated types/effects/declarations and explicit old-profile refusal; migrate internal builders and exhaustive matches.
2. T04: record instruction/continuation design here, then independently validate task scopes, claims, suspension and ownership/cleanup edges.
3. T05/T06: source VM fixture and compiled native continuation parity using RFC 0021's driver.
4. T07/T08: owned locals, cancellation/failure/cleanup and shared controls; retain every M3 exit gate.

## Test plan

Hand-built valid/invalid type and declaration graphs, canonicalization/property propagation, scope-independent storage/signature restrictions, spoofed effects, C refusal, resource caps and unused metadata. Prove that old place/body/execution/artifact paths reject rather than erase the new profile. Preserve old artifact round trips and the full compiler baseline. Later flow/malformed-input and VM/native differential fixtures must prove actual task execution; compile-only fuzz checks are not a fuzz campaign.

## Open questions

No blocking question for the type/declaration boundary. Instruction/continuation encoding, task-aware cleanup and serialized revisions remain unselected and must be recorded before implementation. Hypothesis: extending existing ownership MIR reduces semantic duplication; no compile-time or runtime performance improvement is claimed.

Decision: **accepted under RFC 0001 experimental delegation, 2026-10-21 JST**. The staged refusal preserves existing semantics and creates no execution authority. This is a necessary representation prerequisite, not a new milestone gate or a reduction of T04–T08.

## Sources

- [RFC 0022](0022-source-structured-tasks.md), last updated 2026-10-21; source task semantics and separate source proofs.
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md), last updated 2026-10-20; ownership/cleanup and staged C-aware rollout.
- [RFC 0018](0018-native-compiler-records.md), last updated 2026-10-20, and `crates/codegen-clif/build.rs` (no embedded update date; inspected 2026-10-21 JST): exact native compiler/SDK build identity.
- [Architecture](../architecture.md) and [roadmap](../roadmap.md), last updated 2026-10-21; inspected at `e4b17d0` on 2026-10-21 JST.
- `crates/mir/src/unit/ownership.rs`, `ownership/declarations.rs`, `ownership/places.rs` and `crates/bytecode/src/ownership/`: no embedded publication/update date; inspected at `e4b17d0` on 2026-10-21 JST. These are implementation evidence, not a serialized task-format decision.
