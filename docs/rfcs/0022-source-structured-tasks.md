<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0022-source-structured-tasks.md -->

# RFC 0022: Source structured tasks

[日本語版](../ja/rfcs/0022-source-structured-tasks.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-21
- Last updated: 2026-10-21
- Tracking issue: M3-T03, M3-T09, M3-T10, M3-T04–T08
- Supersedes:
- Superseded by:

## Summary

A01 follow-up decision (2026-10-21 JST): [RFC 0024](0024-send-share-and-service-authority.md) defines recursive Send/Share for logical tasks in one invocation and interface-only producer handles without mutable state projections. It extends the initial Copy-only transfer subset while preserving no-loan/arena/allocator/Task transfer, checked shared operations and independent source/MIR proof. It makes no host-thread promise. A06 derivation and A05 owned async publication/await/disposal must pass before executable admission; A02 retains actual actor authority proof after concrete channel handles exist. Earlier dated checkpoints remain historical. Channel/backpressure and foreign/C/serialized/package decisions remain separate; no M3 gate is complete.

Specify the source surface of RFC 0005's strict structured concurrency, driven by RFC 0021. Functions declare `async`; `spawn` creates a scoped child, `await` consumes a child handle, and task scopes join before exit. No borrow crosses suspension. This accepts source semantics, not source task execution or a serialized coroutine format.

T05/T06 checkpoint (2026-10-21 JST): resolved async effects and bounded move/scope claim snapshots combine with a separate source loan proof. Await/yield/cancel_check/implicit joins reject live shared/exclusive loans with SRC0028 and loan-origin spans; exiting lexical loans end before their joins and outer loans remain live at nested joins. TaskLoans alone is not executable. TaskProgram now retains those source proofs with independently verified ownership task MIR, and the task VM consumes that proof through explicit host allocator/task/control authority. `cargo run -- run examples/scripts/tasks.lang` returns 42 after both workers yield; CLI check and LSP retain combined source/MIR evidence. Raw HIR compatibility APIs, REPL task submissions and unavailable execution/artifact adapters still refuse their unsupported profiles. Async transfers admit scalar/finite Copy values pending A01; initial VM/native locals are finite Copy/Task. Experimental native continuations now match VM values/events/cleanup through JIT and separately linked host objects. Owned/loan/foreign task execution, full shared controls and serialized task formats remain separate deliveries. This RFC stays accepted until its complete contract is implemented.

```text
async fn worker(value: i64) -> i64 {
    yield;
    return value;
}
async fn main() -> i64 {
    task_scope {
        let mut left: core::Task<i64> = spawn worker(20);
        let mut right: core::Task<i64> = spawn worker(22);
        let mut a: core::Result<i64, core::TaskError> = await left;
        let mut b: core::Result<i64, core::TaskError> = await right;
        return core::result_unwrap::<i64, core::TaskError>(a)
            + core::result_unwrap::<i64, core::TaskError>(b);
    }
}
```

T07 partial execution checkpoint (2026-10-21 JST): [RFC 0023](0023-task-mir-boundaries.md) records real owned Text/Bytes, records, whole sums, synchronous owned calls and local loans on VM/generated native/linked objects. Cancellation and failure retire actual owners with bounded ordered reports, including suppressed cleanup evidence under cancelled joins. Cross-frame loans, container/shared/arena task services and wider integration remain T07 work; this checkpoint completes no M3 gate. Async transfers still require the existing finite Copy profile pending A01.

## Motivation

The bounded host task driver executes a tree and retires real host-owned frames, but source has no task syntax/effects or independently verified task MIR. Existing source declarations require `let mut name: Type = expression;`; this proposal preserves that grammar and explicit typing. The candidate fixture fails the existing parser at `async`, `yield` and task expressions; accepting them as ordinary identifiers or ignoring unknown statements would lose concurrency semantics.

## Goals

- Observable scoped spawn/await/yield/cancel with explicit async effects and typed join cancellation.
- Preserve strict loans, ownership, result escrow, deterministic cleanup and source spans.
- One semantic contract for VM/JIT/AOT and host-driven execution, with explicit scheduler authority.
- Stage parser support without silently executing async source synchronously.

## Non-goals

- Closures, generators, pinning, borrowed captures, detached tasks, arbitrary first-class futures and implicit thread/global executor creation.
- Final channels/services/Send/Share derivation or the production component/C embedding profile; their M3 decisions remain required.
- Async C exports/imports, source-variable debug locations, new general inference, or a stable coroutine layout.

## Guide-level design

`async fn` and `pub async fn` declare asynchronous guest functions. An async body has an implicit task scope; `task_scope { ... }` introduces an earlier lexical join boundary. A synchronous function cannot spawn, await, yield, check task cancellation or introduce a task scope. Async functions may call synchronous guest functions normally. `async fn main() -> i64` is a valid eventual script entry; the host supplies its root scheduler. A synchronous entry remains unchanged.

`spawn worker(args)` eagerly enqueues a direct child in the nearest task scope and returns `core::Task<T>`, where the async worker returns `T`. A direct async call is legal only under `spawn` or `await`; there is no hidden lazy future type. `await worker(args)` means spawn and join in that scope. `await handle` moves the handle and returns `core::Result<T, core::TaskError>`; `TaskError::Cancelled` is the initial unit case. Terminal execution failure propagates through the scope instead of becoming a recoverable TaskError. An ordinary guest `Result::Err` remains a successful task value: awaiting such a worker returns a nested Result.

`cancel handle;` makes an idempotent cancellation request without consuming the live handle. It does not suspend. `cancel_check;` observes a pending request without yielding when none exists. `yield;` is a cooperative suspension and puts the task at the ready queue's tail. Cancellation of the awaiting parent closes that parent; cancellation of the child alone produces `Err(Cancelled)` on join.

```text
async fn cancelled() -> core::Result<i64, core::TaskError> {
    let mut child: core::Task<i64> = spawn worker(1);
    cancel child;
    return await child;
}
```

## Reference semantics

### Grammar and resolution

Reserve `async`, `task_scope`, `spawn`, `await`, `yield`, `cancel` and `cancel_check`. Keywords remain distinct even next to comments. The additions are:

```text
function       := ["pub"] ["async"] "fn" signature block
task-scope     := "task_scope" block
yield-stmt     := "yield" ";"
cancel-stmt    := "cancel" expression ";"
check-stmt     := "cancel_check" ";"
unary          := "spawn" unary | "await" unary | existing-unary
```

Spawn/await bind more tightly than addition/comparison and less tightly than call/field access. Parentheses override precedence. The parser retains the operand tree; typing requires a spawn operand to be a direct async guest call and an await operand to be a Task value or direct async call. Existing expression-statement restrictions remain: use a typed local to retain/discard a join Result. Modifier order is `pub async fn`; C declarations do not accept `async`.

`core::Task<T>` and `core::TaskError` belong to the reserved core namespace. Task is a linear, non-Copy, non-Send, non-Share claim on one direct child's result, branded by the creating lexical task scope. Source cannot fabricate a task/scope brand. Initial Task types occur only in locals inside an async body; reject them in fields, containers, function parameters/results, host/C signatures and nested Task results. Reject a move/assignment into a local outside the creating scope. Ordinary move/initialization checks reject use after await or move; cancel inspects a live task place rather than cloning/moving it.

Task claims cannot be borrowed. Existing owned pattern matching creates ordinary loans, so a direct `match` on a Task is rejected; move it into a typed local and await it instead. This introduces no special move-pattern semantics. T09 compatibility audit (2026-10-21 JST): this makes the opaque-claim restriction explicit, and the guide's Result unwrap calls now spell the existing required core type arguments; no new inference is introduced.

### Effects, evaluation and admission

Async is a checked function effect, independent of native ABI. Function lookup resolves its effect before call checking. Async arguments/results must be recursively sendable under RFC 0005; no loan, arena reference, task handle, mutable alias or non-sendable host value may transfer. Safe source cannot assert Send. The first executable fixture may admit scalar/finite Copy values; later accepted derivation extends support without relaxing the rule.

Evaluate spawn arguments once, left to right, in the parent. Before publishing the child, preflight scheduler capability, live/cumulative task limits, frame/result capacity, types and moves. Only then commit argument ownership and the queue entry. Refusal publishes no child and failure cleanup retires staged argument owners. Task/frame quota exhaustion is an execution-control failure, consistent with existing call/frame admission, not a recoverable TaskError. Frame allocation uses the invocation's explicitly supplied allocator/services, never an ambient executor or allocator grant from source metadata.

Await, yield, cancellation check and implicit scope joins are suspension/cancellation points. Reject every live shared/exclusive loan across them. At scope exit, end loans whose lexical scopes exit before the implicit join; outer-scope loans still live at a nested join are rejected. These proofs must exist both in source analysis and independently in typed MIR. `unsafe` does not bypass them. Async/task operations are unavailable in constant evaluation.

### Lifetime, result escrow and cleanup

Every child belongs to its creating scope, independently of the result handle. Normal scope exit joins all children; terminal failure/cancellation first requests descendant cancellation, then joins. Return evaluates its result once into escrow before closing scopes, and publishes it only after required cleanup. No child survives its scope or accesses a retired parent frame.

A child result remains escrowed until child cleanup succeeds. Successful publication records one initialization in scope-owned result storage. Await consumes its unique claim and moves the result once into `Ok(T)`; a cancelled child publishes no `T`. Moving a Task transfers the claim. Dropping or overwriting a handle relinquishes retrieval but neither detaches nor implicitly cancels the child. Unclaimed results remain owned/accounted by the scope and retire in reverse result-publication order after its joins, before the scope's ordinary owned-local cleanup. Results and their retained cells/heap count against shared invocation limits even after the child stops counting as live.

Frame locals retire under RFC 0008 in reverse actual initialization order after loans end and descendants join. Cleanup dispatch/acknowledgement follows RFC 0021, including pending cancellation at cleanup entry, non-preemptible active callbacks, explicit-parent-failure precedence, first failed child in spawn order and secondary cleanup failures. Guest fuel exhaustion or report truncation never skips mandatory retirement.

## Execution modes

Typed MIR records async effects, scope identities, spawn argument moves, result claims, suspension continuations, cancellation and cleanup edges with spans. Verification checks scope/handle liveness and every suspension loan state independently from HIR. VM and compiled continuations use the same explicit driver; native execution must not interpret MIR or fall back to VM. Given the same external event order they agree on values, failures, observations, ownership and cleanup.

The CLI acts as an explicit host with bounded task authority. Component hosts must grant scheduler authority and limits explicitly; source/package metadata only declares a requirement. No C/native callback gains ambient scheduling/reentry authority. The production embedding decision defines polling/callback cancellation. Ordinary M2 synchronous programs retain their existing execution controls; task cancellation does not erase instruction fuel/interruption checks.

## ABI and compatibility

This decision adds source syntax and core type/effect contracts. It does not assign a native frame layout or change an existing wire schema. Task-capable MIR, bytecode, native records and startup adapters require explicit versioned extensions and malformed-input/compatibility validation before serialized execution is admitted. Unsupported task artifacts/profiles fail before effects; older programs cannot acquire tasks implicitly. Raw Task handles are not C/component values. Public component async signatures await their production profile.

Previously valid identifiers matching the seven new keywords must be renamed, e.g. `async` → `async_value`. No automatic rename is claimed. There is no stable source edition yet. C declarations stay synchronous and cannot expose Task. Async effect changes are compatibility-sensitive; imports/exports must record them in the relevant future profile.

T16 allocator/arena decision (2026-10-21 JST, accepted under experimental delegation before implementation): arena entry captures invocation allocator authority and immediately ends only its argument temporaries. This follows RFC 0007's clarified arena-entry contract; a separately bound ordinary allocator loan and all ordinary arena-read loans retain strict suspension rejection. Source lowering emits the corresponding cleanup immediately after ArenaEnter, and independent MIR verifies the ended loan state rather than exempting arena loans from suspension checks. Each activation prepares its own arena state before payload admission under the preparation budget. Registered arena read leases carry their owning activation/slot plus origin/generation; checked views, publication inspection, same-region validation and mixed iterative retirement reuse the existing runtime services. End leases before joined-scope owner retirement, capture failure/poison evidence in the same bounded report, and continue independent retirement after cleanup failures. No arena value crosses a task transfer and no serialized/C/package profile changes. The new source fixture must execute owning cycles, suspension, nested descendant joins, cancellation and failure cleanup on VM/generated native/linked objects. Temporary-scope shortening is a compatibility extension for previously rejected arena task scopes, not a loan-at-suspension exception.

## Diagnostics and tooling

Distinct CST nodes preserve async declarations, task scopes, prefix operations and statements. Recovery must retain all tokens, enclosing braces and following declarations, including malformed `async` signatures and missing unary operands. Formatting changes whitespace only and retains comments/precedence/idempotence. LSP formatting reuses the CST formatter.

Use `SRC0026` for an unavailable async execution/analysis profile during rollout, `SRC0027` for task effect/scope/claim violations and `SRC0028` for `concurrency.borrow-across-suspension`, with a secondary loan-origin span. Existing move/type diagnostics remain applicable. Source type/claim and loan proofs alone must not make executable source check succeed: until task MIR and adapters are connected, report an unavailable profile before producing an executable checked program. Debug/task ancestry and lints remain the later tooling work; no source-variable or DAP support is claimed here.

## Performance and cost model

Task creation requires admitted frame/result storage and driver bookkeeping. Suspension retains owned locals; unclaimed results retain memory until scope cleanup. Await moves a result rather than cloning it. No parallelism, latency or allocation-free async guarantee is made. Measure frame/result peaks and spawn/join/cancellation costs on both tier-1 hosts; preserve M2 workload baselines. The formatter/parser use their existing token/node/depth/work/output bounds for the new syntax.

## Security and trust boundaries

Untrusted source cannot construct handles, assert Send, escape a scope, retain loans across suspension or create ambient scheduler authority. Source validation is insufficient for foreign MIR/bytecode: repeat proofs and resource admission at those boundaries. Task/result/heap/control limits compose across callbacks under the remaining M3 contracts. Trusted host code is responsible for valid adapter storage and cleanup acknowledgement; no in-process native sandbox is asserted.

## Alternatives

- **No change:** leaves the accepted driver inaccessible to source programs.
- **Library-only task functions:** cannot enforce lexical task brands, async effects, suspension loans or independent MIR proofs.
- **First-class lazy futures with frame borrows/pinning:** more composable, but require new lifetime/address-stability, cancellation and ownership contracts beyond RFC 0005's strict baseline.
- **Synchronous spawn sugar:** easier to lower, but hides suspension and queued-child cancellation and does not implement the intended source behavior.
- **Await returning T directly:** concise, but cannot express explicit child cancellation as a typed outcome without conflating it with terminal failure. This design uses Result and preserves ordinary guest errors separately.

## Implementation plan

1. T03: accept this decision; lossless lexer/parser/recovery/formatter fixtures and explicit semantic admission refusal.
2. T09: resolved async effects, Task/TaskError typing, scoped claims, call/move/escape/C-surface checks.
3. T10: source loan checks at explicit and implicit suspension points with related spans.
4. T04/T05: independent task MIR and source VM fixture; T06 compiled native parity. Decide exact artifact revisions before their dependent serialization/startup work.
5. T07/T08: actual owned locals/results, cancellation cleanup and shared controls. Channels, Send/Share and embedding retain their board decisions.

## Test plan

Lossless CST spans/recovery and exact precedence; comments and idempotent formatting; reserved-name and resource-limit boundaries. Exercise profile discovery in unused functions, imports and REPL/LSP/virtual/preparsed inputs. Available task analysis retains combined source/MIR proof; unavailable adapters retain explicit refusal. Later compile-pass/fail covers async calls, task scope escape, consumed handles, both loan kinds, all exits, and C boundaries. Execute source and malformed MIR/bytecode fixtures with VM/JIT/AOT differential values, typed cancellation, terminal failure ordering, escrow/moves and exactly-once cleanup. Fuzz suspension/claim transitions and measure both hosts before stabilization.

## Open questions

No blocking question for this strict source contract. Unverified assumptions: local-only scoped handles suffice for the first source slice; retained result escrow costs are acceptable under shared limits; broader native profiles and their costs remain unverified. T06 demonstrates managed-frame driver reuse without MIR interpretation for finite Copy/Task locals. General Task parameters/containers and richer futures need separate decisions. Exact serialized revisions and production embedding remain explicit prerequisites, not implicitly accepted formats.

Decision: **accepted under RFC 0001 experimental delegation, 2026-10-21 JST**. The explicit effects, scoped claims and typed child cancellation preserve RFCs 0005/0008/0021. Implementation/stabilization require the evidence above.

## Sources

- T06 native evidence: `crates/runtime/src/task/runner/`, `crates/codegen-clif/src/tasks.rs`, `tasks/object.rs`, `tasks/tests.rs`, `tests/native_task_execution.rs`, `tests/support/task_static.rs` and `examples/native-structured-tasks.rs`; no embedded update date, inspected/executed 2026-10-21 JST on macOS AArch64. Initial VM/JIT/linked-object parity does not complete owned tasks, shared controls or platform gates.

- T05 source/VM evidence: `crates/semantics/src/analysis.rs`, `ownership_types/bodies.rs`, `crates/vm/src/unit/tasks/`, `tests/task_execution.rs` and `examples/scripts/tasks.lang`; no embedded update date, inspected and executed 2026-10-21 JST. This is initial VM evidence, not native/owned-task or full M3 conformance.
- [RFC 0005](0005-structured-tasks-and-isolated-services.md), last updated 2026-10-21; strict task/borrow/cancellation baseline and follow-up mapping.
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md) and [RFC 0021](0021-bounded-structured-task-driver.md), last updated 2026-10-20 and 2026-10-21 respectively; cleanup and implemented driver.
- `crates/syntax/src/{lexer,parser,format}.rs`, `crates/hir/src/resolved.rs`, `crates/semantics/src/{modules,binding}.rs`, inspected at `b45a62d` on 2026-10-21 JST. Candidate source check before changes reports SRC0012 at async/task syntax. This is a grammar experiment, not runtime/performance evidence.
- All new syntax/effect/result choices are project decisions; no new external-language or performance fact is asserted.
