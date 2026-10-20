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

Extend the existing ownership MIR contract with Task/TaskError types, an async function effect independent from ABI, and independently verified task instructions, scopes, claim flow and suspension cleanup. Type/declaration evidence alone is not task flow evidence. Existing synchronous execution and artifact paths reject this profile so they cannot erase its meaning.

T11/T04–T06 checkpoint (2026-10-21 JST): types, effects and declaration validation are implemented. Old place/body, runtime layout and bytecode writer paths explicitly refuse task metadata; old decoders restore synchronous effects and assign no new tags. In particular, the generic runtime token-layout fallback must not invent storage for Task/TaskError. T04 now adds a separate task verification entry over the same body table, combining scope/claim/return-escrow analysis with move, loan, arena-region and failure-plan evidence. Async failure plans distinguish cancellation from primary execution failure and order loan ending, scope cancellation/join, owner retirement and escrow discard. T05 now lowers source tasks to that contract and executes the initial finite Copy/Task-local profile through an explicit task VM driver. Normal source checking/LSP retain combined TaskProgram evidence and CLI run executes the source fixture. T06 compiles the initial profile to native continuations with JIT and separately linked host-object parity. Owned/loan/foreign task execution and complete shared controls remain unimplemented; this RFC stays accepted while that contract is incomplete.

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

### T04 instruction and continuation decision

Accepted under experimental delegation, 2026-10-21 JST, before implementation: extend ownership `Op` with TaskScopeEnter/Exit, TaskSpawn, TaskAwait, TaskCancel, TaskYield, TaskCheck and TaskSaveReturn. Add ReturnSaved to the ownership terminator. Each operation retains its source span; its continuation is the next instruction site (or the block terminator), not a source AST node. Await/exit can suspend at that site; admission/move and result initialization happen once, and resumption cannot repeat preceding effects. Ordinary guest calls remain synchronous.

Each async body declares a bounded scope tree with root 0 (implicitly active at entry), earlier-parent IDs and source spans, plus exactly one lifetime-scope binding for every Task local/SSA value. Ordinary slots have no claim binding. A scope binding is semantic lifetime metadata, not inferred authority from a source range. Scope enter must match the active parent; exit closes the top scope. At exit, all claim slots bound to that scope retire if initialized, while the scope joins every child and disposes unclaimed results independently of remaining handles. Moving a root claim into an inner binding abandons retrieval when that binding exits without detaching the root child.

An independently computed origin follows each claim through moves, local initialization/replacement and CFG arguments. A destination lifetime scope must be the origin or its descendant, and active when initialized. Copying, borrowing, fabricating, escaping or reusing a consumed/retired claim is invalid. CFG merges require equal active scope stacks and compatible return-escrow state; origin sets are unioned under the shared work/storage limits. Generic ownership move, loan and arena-region proofs remain required. Await/yield/check/scope exit reject every live ordinary loan independently from HIR.

TaskSpawn calls only an async guest and produces Task(T); TaskAwait consumes it and initializes Result(T,TaskError) only after a successful or cancelled join. Terminal child failure follows the failure path. TaskCancel observes a claim and does not consume or suspend. Initial async transfers admit the accepted finite Copy profile; general owned Send/Share remains A01. Owned locals still use the ownership MIR rules, not a second value language.

TaskSaveReturn moves the result into one protected escrow before joins. Async normal exits require one saved result, an empty scope stack and ReturnSaved; duplicate saves or unsaved returns are rejected. Failure/fuel/interruption cleanup ends ordinary frame loans, requests descendant cancellation, joins active scopes from inner to outer and retires their results, then retires frame owners and discards escrow. This mandatory cleanup consumes no guest fuel and continues through secondary failures under RFCs 0008/0021. Synchronous bodies keep existing return/failure behavior. The task failure plan records these phases explicitly; a type/body wrapper alone cannot publish a task execution plan.

The task verifier is a separate entry over the same ownership body table. Existing synchronous verification/execution/writers retain their refusal. A verified task wrapper proves the task and ownership obligations, not host grants, physical layout or backend support. T05/T06 must execute this contract through explicit host services; no tags or encoded artifact revision are assigned here.

T04 cancellation-plan clarification (2026-10-21 JST): async failure plans expose a separate cancellation entry that skips primary-failure recording and enters mandatory cleanup. EndTaskLoans, CancelTaskScopes and JoinTaskScopes precede owner retirement and result discard. FinishTask terminates cleanup using the task driver's pending outcome and recorded failures, so ordinary cancellation is not fabricated as an execution fault; cleanup failures and RFC 0021 precedence remain observable. TaskCancel observes a whole place, including a local, without copying or moving its claim.

## Execution modes

T12 ordering clarification (2026-10-21 JST, before implementation): preserve RFC 0022’s existing contract by lowering exiting loan/reference cleanup before a source task-scope join and ordinary owner/value cleanup after that join and unclaimed-result retirement. Return stages its result once, closes nested scopes inside-out and retires each scope’s locals after its own join. Reorder existing MIR instructions without new source semantics or wire tags. Runtime scopes retain a publication-order result chain and retire unclaimed results in reverse publication order only after all children join. Already-awaited holes are not retired again; failure/cancellation use the same inventory. Verify source/MIR and VM/native region release ordering before T07 owner admission.

T06 native decision (2026-10-21 JST, before implementation): share the invocation driver, typed frame/claim/result state and scheduling/storage primitives in runtime, with explicit backend resume/cleanup entries. Keep MIR node dispatch in the VM backend. Cranelift emits native blocks, arithmetic, comparisons, branches, failure edges and bounded continuation selection; native callbacks perform only typed storage, scheduling, control and failure primitives, never evaluate a MIR instruction/body. A private invocation packet holds a scoped context and checked callback entry; it is neither a C embedding API nor a serialized interface. Native invocation selects generated function addresses for normal/suspended/cancellation continuations. Match VM values, events, failure origins and cleanup under the initial finite Copy/Task profile, including native object execution where the exact generated code/metadata and compiler build are trusted. Keep existing task artifact/native-package refusal until a separate serialized revision is accepted. Reuse managed frame/result layouts; add no scheduler/IDL framework or second IR.

T06 shares invocation, managed frame/claim/result state and scheduling/storage services in `runtime::task::runner`, with separate backend resume/cleanup entries. VM keeps MIR dispatch; `codegen_clif::tasks` generates arithmetic, comparisons, branches, failure edges and checked continuation selection. Native callbacks never evaluate MIR instructions/bodies. `cargo run --example native-structured-tasks` prints 42. JIT and separately linked host objects match VM values, events, failure origins, usage and storage retirement for finite Copy/Task locals. Unsafe object binding requires exact trusted code/metadata/compiler/SDK and a live code owner; it defines no public C embedding ABI or task package format. Existing artifact/native-package refusal and later owned/loan/foreign/shared-control gates remain (verified 2026-10-21 JST, macOS AArch64). Compilation accumulates proof/layout/native metadata under its limit; invocation retains separate preparation and guest controls. The private callback contains unwinding with source-span failure evidence; allocation providers retain their separate no-unwind contract. Out-of-range continuations reject before callbacks, and the standalone fixture compares re-emitted object bytes to the linked input. This evidence makes no broader performance/platform claim.

T05 adapter decision (2026-10-21 JST, before implementation): proof-consuming task layout/frame entries reuse managed finite Copy layouts and FrameStorage. TaskError has its ordinary one-case finite error layout. A TaskClaim layout is a zero-payload marker, with no generic token fallback; an invocation-local sidecar stores the scheduler's unforgeable TaskId. Claims still count as logical cells. Runtime scopes have distinct activation identities for loop re-entry, without inventing extra scheduled tasks. Cleanup releases frame storage while a detached typed return reservation remains in the creating scope until retrieval/disposal. These private layouts assign no native, C or wire ABI. The initial VM adapter preflights reachable finite Copy/Task locals and supported operations before effects; owned/loan/foreign execution remains explicit later work in T07/T08 rather than weaker static rules. The host supplies allocator authority, task limits and execution controls. Source promotion retains source claim/loan and independent MIR evidence together; unavailable backends/artifacts retain refusal.

VM, JIT and AOT consume the same verified task contract when their adapters exist. Native task execution must compile continuations, not interpret MIR or fall back to VM. Until then, existing synchronous adapters reject task profiles before bindings, allocation or effects. Constant execution and component tasks remain unavailable under their current profiles.

T12 implementation (2026-10-21 JST): source lowering splits cleanup around each normal scope join and nested return boundary, while existing independent task/move/loan proofs revalidate the resulting MIR. Runtime scopes keep an allocation-free linked publication stack; results remain until all joins and retire newest-first, including cancellation/failure before RetireFailure. Awaited holes are skipped once, and mandatory abort paths retain the same inventory. Provider-region execution checks both publication orders and a slow sibling on VM/native; source tests inspect owning locals and loan order on normal/nested return exits. Owned execution remains refused pending T07; no wire/packet revision is assigned.

## ABI and compatibility

The Rust MIR API is experimental. New enum variants and the effect field require internal builders/matches to migrate. Preserve existing constructor/case order where ordinals are used internally. Rust enum layout is not a wire ABI. Existing schemas retain their bytes and meanings; writers reject unsupported task types/effects rather than omit them. Readers never infer async from names or scalar signatures. A future encoded task profile requires an explicit accepted revision and malformed-input/migration tests before publication.

Native packages remain bound to their exact compiler/SDK build ID under RFC 0018. A compiler-source change requires rebuilding a native package with its matching SDK even when the semantic wire bytes are unchanged. Do not weaken startup mismatch rejection to accommodate a mixed build.

## Diagnostics and tooling

Use structured MIR errors with validated source spans for invalid task types/declarations and unsupported task profiles. Source SRC0027/SRC0028 identities stay unchanged. Type-only success grants no source-check success or debugger task support. Formatter and parser behavior remain RFC 0022's.

## Performance and cost model

Reuse the bounded ownership type graph, canonicalization, property propagation and declaration budgets. Charge new traversal/storage work to existing limits. No runtime cost, frame-size or code-size claim follows from static MIR proof. Task/frame/result costs remain measured execution requirements of M3.

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

Hand-built valid/invalid type and declaration graphs, canonicalization/property propagation, scope-independent storage/signature restrictions, spoofed effects, C refusal, resource caps and unused metadata. T04 adds raw MIR scope/claim flow, branch/loop joins, local/prepared moves, loan boundaries, return escrow and cancellation/failure ordering without HIR authority. Prove that old place/body/execution/artifact paths reject rather than erase the new profile. Preserve old artifact round trips and the full compiler baseline. T06 adds actual VM/JIT/linked-object comparisons for finite Copy/Task locals, aggregates, cancellation, loops/calls, failure origins, fuel/trace/provider refusals and private packet boundaries; broader owned/loan/foreign/shared-control conformance remains pending; compile-only fuzz checks are not a fuzz campaign.

## Open questions

No blocking question for the in-memory T04 proof. The instruction/continuation and cleanup-plan decisions above are implemented; initial task VM/native layout/resumption is implemented, while owned-task execution, full shared controls and serialized task revisions remain separate unverified deliveries. A serialized revision must be accepted before publication. Hypothesis: extending existing ownership MIR reduces semantic duplication; no compile-time or runtime performance improvement is claimed.

Decision: **accepted under RFC 0001 experimental delegation, 2026-10-21 JST**. The staged refusal preserves existing semantics and creates no execution authority. This is a necessary representation prerequisite, not a new milestone gate or a reduction of T04–T08.

## Sources

- T12 ordering evidence: `crates/semantics/src/ownership_types/bodies.rs`, `crates/runtime/src/task/runner/`, `crates/vm/src/unit/tasks/machine.rs`, `tests/task_retirement.rs`, `tests/fixtures/task_owned.lang` and `examples/scripts/task_result_retirement.lang`; no embedded code update date, inspected/executed 2026-10-21 JST. The owning source fixture proves lowering only; executable provider-region ordering uses admitted finite Copy tasks.

- T06 native evidence: `crates/runtime/src/task/runner/`, `crates/codegen-clif/src/tasks.rs`, `tasks/object.rs`, `tasks/tests.rs`, `tests/native_task_execution.rs`, `tests/support/task_static.rs` and `examples/native-structured-tasks.rs`; no embedded update date, inspected/executed 2026-10-21 JST on macOS AArch64. Initial VM/JIT/linked-object parity does not complete owned tasks, shared controls or platform gates.

- T05 implementation: `crates/runtime/src/layout/`, `crates/runtime/src/task.rs`, `crates/semantics/src/analysis.rs`, `ownership_types/bodies.rs`, `crates/vm/src/unit/tasks/` and `tests/task_execution.rs`; no embedded update date, inspected/executed 2026-10-21 JST. Initial source VM values, cancellation, scope instances, failure origins and resource refusals are separate from native/owned-task conformance.
- `crates/mir/src/unit/ownership/tasks.rs`, `ownership/failure.rs`, `crates/mir/tests/tasks.rs` and `tests/task_mir_boundary.rs`: no embedded update date; implementation and independent boundary evidence inspected 2026-10-21 JST. This is static MIR evidence, not executed task conformance.
- [RFC 0021](0021-bounded-structured-task-driver.md), last updated 2026-10-21; bounded driver outcomes and cancellation/cleanup precedence.
- [RFC 0022](0022-source-structured-tasks.md), last updated 2026-10-21; source task semantics and separate source proofs.
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md), last updated 2026-10-20; ownership/cleanup and staged C-aware rollout.
- [RFC 0018](0018-native-compiler-records.md), last updated 2026-10-20, and `crates/codegen-clif/build.rs` (no embedded update date; inspected 2026-10-21 JST): exact native compiler/SDK build identity.
- [Architecture](../architecture.md) and [roadmap](../roadmap.md), last updated 2026-10-21; inspected at `e4b17d0` on 2026-10-21 JST.
- `crates/mir/src/unit/ownership.rs`, `ownership/declarations.rs`, `ownership/places.rs` and `crates/bytecode/src/ownership/`: no embedded publication/update date; inspected at `e4b17d0` on 2026-10-21 JST. These are implementation evidence, not a serialized task-format decision.
