<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0021-bounded-structured-task-driver.md -->

# RFC 0021: Bounded structured task driver

[日本語版](../ja/rfcs/0021-bounded-structured-task-driver.md)

- Status: implemented
- Authors: Project contributors
- Created: 2026-10-20
- Last updated: 2026-10-28
- Tracking issue: M3-T01, M3-T02
- Supersedes:
- Superseded by:

## Current scope

Reconciled 2026-10-28 JST. The kernel, source adapters, parked channels, descendant cleanup and original-task synchronous callback scopes are implemented and included in the [M3 exit audit](../milestone-3-capabilities.md). RFCs 0022–0027 supply the formerly separate source, Send/Share, channel, service and embedding decisions. Execution remains logical tasks within one invocation. Dated implementation prerequisites below describe their checkpoint, not remaining M3 work.

Accepted E13 current-task borrowing decision (2026-10-25 JST, RFC 0001 experimental delegation): synchronous native callbacks borrow the original Scheduler's actively running task through a checked exclusive view. The view reads the original branded TaskId, parent/origin, cancellation request, usage/limits, and delegates cancellation requests to that same original task. It grants no root start, task admission, copied counter, dispatch/finish or cleanup authority. Entry validates invocation/slot/active/running state; ready/waiting/cleaning/completed/foreign tasks refuse. Nested reborrows reference the same task/state; drop, error and unwind neither finish nor redispatch the original task. Existing kernel transitions remain unchanged. Reject copied IDs/cancellation flags or fresh roots because they do not share lifecycle. Next connect the existing VM/native frame execution: a synchronous child belongs to the original logical task, admits no replacement task and cleans only its own scope without closing the parent. E13 remains incomplete until actual source callbacks from a non-root task prove cancellation, repeats, unchanged admissions and complete owner/loan cleanup. Preserve task-export refusal and E02/E03/E04/L04/public-C conditions; syntax/persistent formats stay unchanged. Unverified implementation assumption: existing frame driving can connect to the original task lifecycle without another logical task.

Accepted E13 synchronous-call scope clarification (2026-10-25 JST, delegated before implementation): a borrowed-task adapter neither constructs/starts nor dispatches the original Scheduler. It holds the original CurrentTask plus call-local ready/running/cleanup/complete phases, reusing the existing Backend/frame executor. Logical TaskId, task usage and cancellation/closing state come from the original view. Scope finish/cleanup closes only that scope; it neither completes the original task nor dispatches its ready queue. Suspending/task exports remain refused. Native imports borrow the original task view and pass it to nested synchronous calls. Distinguish temporary frame-record indices from logical task identity; observable events use the original identity. Add no logical task, identity registry or source evaluator. Require actual non-root source callbacks proving this separation, cancellation scope cleanup and continued parent execution.

## Summary

C02 integration checkpoint (2026-10-21 JST): the channel Hub owns readiness/custody over C05's private tickets and prepared global FIFO. `Scheduler::is_quiescent` reports no active/ready dispatch with an unfinished root; the closed host driver records the oldest Hub operation as typed ChannelDeadlock before tree cancellation. Actual send/receive/full-send/close/cancel examples return 42 or the recorded failure and acknowledge descendant-first frame cleanup with zero live tasks. Source/MIR/VM/native channel admission remains separate in RFC 0025; no external event authority is inferred.

C05 implementation checkpoint (2026-10-21 JST): the driver now issues opaque invocation/task-branded WaitId tickets with checked global ordinals. check_park preflights admission; park relinquishes dispatch or closes on pending cancellation; wake validates the exact parked activation, appends it once after existing ready tasks and immediately invalidates the ticket. Parked tasks stay live and retain adapter-owned frames; cancellation/child failure revoke readiness before descendant-first cleanup acknowledgement. Twenty-six kernel tests cover these boundaries, active-dispatch preservation, forged/foreign/duplicate/stale wakes, 256 reuse rounds without storage growth and the final u64 ordinal/exhaustion boundary. `cargo run --example parked-tasks` returns 42 with actual frame cleanup [1,0] and cancelled all-parked cleanup [2,1,0], zero live tasks. No guest frames/messages or per-park storage are allocated by the kernel. Channel readiness, registration/payload custody, publication, deadlock diagnostics and source/MIR/VM/native channel operations remain C02/C03/C04; the adapter must unlink registrations before value cleanup. No channel profile or M3 gate is complete.

C05 adapter decision (2026-10-21 JST, before implementation): `check_park` preflights a running task and a checked invocation-wide wait ordinal before adapter publication. `park` returns `Park::Waiting(WaitId)` after an allocation-free transition to Parked, or `Park::Closing` when pending cancellation/child failure closes the task without issuing a ticket. The opaque, non-Copy ticket includes its invocation-branded TaskId and unique ordinal; `wake(&WaitId)` appends that exact parked task once at the ready tail and immediately invalidates the ticket. Foreign, duplicate, cancelled and earlier-generation wakes fail structurally without altering queue/task state. Cancellation settles Parked exactly like child Waiting and joins descendants before cleanup; cancellation after wake follows the existing Ready/Running boundary rules. Checked ordinal exhaustion refuses before changing active state. One parked state per admitted task bounds metadata; no new allocation, guest frame, payload inventory or external-event authority is added. The channel adapter retains operation spans and registration custody. C02 supplies readiness and interprets a closed invocation's all-blocked state.

C01 follow-up decision (2026-10-21 JST): [RFC 0025](0025-bounded-channels-and-backpressure.md) accepts channel park/wake and FIFO admission on this invocation driver. C05 must add bounded invocation/task-branded wait identities, allocation-free park/wake, stale/duplicate-wake rejection without double enqueue, cancellation revocation before cleanup and safe wait-generation exhaustion. C02 supplies channel readiness and commit; the kernel must not own/evaluate guest messages or interpret MIR. Global ready FIFO and descendant-join-before-cleanup remain unchanged. At the C01 decision checkpoint the committed driver had only child-join waiting; C05 above implements the bounded readiness extension.

Add one backend-neutral, host-driven scheduler kernel under RFC 0005. It owns task ancestry and scheduling state, while execution adapters own guest frames, values and diagnostics. There is one explicit root and at most one active dispatch. Ready tasks run FIFO until the adapter reports a semantic boundary. Scope exit joins children; cancellation/failure requests cancellation of unfinished descendants. Guest cleanup is a separate dispatch, acknowledged only after actual frame cleanup.

This decides the first production task-driver contract. [RFC 0022](0022-source-structured-tasks.md) accepts the source async surface; Send/Share, channels, component cancellation and the public C embedding API remain separate M3 decisions. Accepting this RFC completes neither roadmap G01 nor structured source execution.

Implementation checkpoint (2026-10-21 JST): `crates/runtime/src/task.rs` implements this kernel. Eighteen focused tests cover the state/limit/cleanup contract, including cancellation before cleanup entry and during active cleanup. `cargo run --example structured-tasks` executes real host-owned frames through success and nested cancellation (42; cleanup 2,1,0; zero live tasks). Full fmt/Clippy/test/rustdoc baseline passes: 96 suites, 1,613 tests, zero failures, one existing timing ignore. Source/VM/native task execution and public embedding remain the separate work described below.

## Motivation

The accepted task prototype in `crates/semantics` models one task's borrows and destruction. It does not schedule children, join scopes, bound task admission or drive actual VM/native frames. M2 already has explicit managed continuations and failure cleanup; the scheduler must dispatch those adapters without interpreting their private layouts or relying on Rust destructor timing for guest destruction.

## Goals

- Explicit scheduler authority, bounded admission and deterministic ordered-event behavior.
- Cooperative cancellation, no escaping unfinished child and descendant-before-parent cleanup.
- One scheduling contract for VM, JIT, AOT and host-driven continuations.
- Validate foreign, stale and illegal handles/transitions before mutation.

## Non-goals

- Source syntax, coroutines, channels, services, threads, timers and external wakeups in this first slice.
- Fairness/latency guarantees or preemption of arbitrary trusted host/native code.
- Stable Rust/C ABI, serialized task handles, guest allocator accounting or a sandbox claim for the scheduler alone.

## Guide-level design

The host explicitly constructs a scheduler with live-task and total-task limits, starts its root and drives dispatches. A resume dispatch permits the adapter to access its task frame, spawn a direct child, yield, join a direct child or finish. A cleanup dispatch permits only guest cleanup followed by acknowledgement. `dispatch` refuses reentry while either dispatch is active.

```text
root resumes → spawn left → spawn right → join left
left resumes → yield
right resumes → finish(success)
left resumes → finish(success)
right cleanup → acknowledge
left cleanup → acknowledge
root resumes → read left/right results → finish(success)
root cleanup → acknowledge → root complete
```

Results live in adapter-owned storage. Scheduling success is distinct from a guest `Result::Err`, which is an ordinary typed value. Terminal panic/control/adapter failures cannot be recovered merely by joining a child.

## Reference semantics

### Admission and identity

One scheduler invocation starts exactly one root. Only the actively running task may spawn a direct child; no detached child is available. Handles carry private invocation identity and a monotonic slot. Handles are never reused within an invocation and cannot be serialized or constructed by guest code. A handle from another scheduler is invalid even if its slot matches.

Before publishing a root/child or queue entry, check both live and cumulative task limits. Root, running, suspended, joining and cleaning tasks count as live. Admission does not consume a slot on refusal. The total-task limit bounds retained ancestry/completion metadata and traversal work even when completed tasks no longer count as live. Allocate bounded metadata/queue capacity fallibly before starting. Guest frames/results remain subject to the execution adapter's separate limits; this kernel does not exempt them from accounting.

### Scheduling and join

States are ready, running, waiting for a direct child, joining all children, ready for cleanup, cleaning, and complete. Ready and cleanup-ready dispatches share one FIFO queue; each task has at most one queue position. Spawn and yield append to the tail. Joining an already complete child still yields to the queue. A parent waiting on a child resumes only after that child's cleanup acknowledgement.

`finish(success)` begins normal scope closure and waits for every direct child to become complete. It does not discard unfinished work. A parent frame cannot be cleaned while any child remains live. Joining self, a sibling, an ancestor or a foreign task is rejected before state changes. Adapters retain ownership of completed child results until a validated move/retirement; the scheduler never clones them.

### Cancellation and failures

Cancellation is idempotent and propagates to unfinished descendants. It is observed at dispatch entry, yield, join or an adapter's explicit cancellation check. A running resume/cleanup callback is never preempted. A cancellation request arriving during it takes effect at the next boundary. A normal success reported after a pending cancellation becomes cancelled; an explicit terminal instruction failure retains its own primary identity. A parent already joining successful children may be cancelled while waiting.

When a child finishes with terminal failure, request cancellation of unfinished siblings and mark the parent for failure closure at its next boundary. This propagates upward after each parent's own cleanup. An explicit parent terminal failure is primary. Otherwise, after children finish, choose the first failed direct child in spawn order, ahead of cancellation or success. Adapter diagnostic storage retains that failure's origin and secondary/suppressed failures. A typed guest error is not such a terminal failure.

Cancellation or failure closure joins cancelled descendants before cleaning the parent. Children that already completed remain complete. Cleanup is dispatched exactly once per admitted task and is not charged against guest fuel. The adapter performs RFC 0008 reverse initialization cleanup and returns an optional cleanup failure identity. A cleanup failure replaces success, but an existing failure/cancellation remains primary and the adapter records the cleanup failure as secondary. Completion/live-slot release occurs only after acknowledgement. Queue/report truncation or exhausted guest controls cannot skip cleanup.

The kernel validates scheduling transitions, not guest types, loans, capabilities, result moves or instruction fuel. A VM/native adapter must validate those contracts independently. In particular it must reject all live shared/exclusive loans at suspension under RFC 0005.

## Execution modes

The kernel never calls MIR, VM or native machine code. Drivers map a resume to their prepared continuation and a cleanup dispatch to their failure/retirement path. The same ordered operations must produce the same task ancestry, dispatch order, cancellation observation, completion outcomes and cleanup order in all modes. The first host fixture exercises the kernel; source/VM/native fixtures remain explicit deliveries T03–T07.

Host callbacks cannot obtain ambient scheduling authority or reenter an active dispatch. A later component profile defines how an authorized callback requests cancellation or schedules work. Constant evaluation has no scheduler. WebAssembly components and multithreaded executors require their own adapter work while preserving this contract.

## ABI and compatibility

All state, queue layouts, handles and failure IDs are private in-process Rust interfaces. No source, MIR, bytecode, native ABI, package or C ABI format changes in this delivery. Future source task operations require explicit versioned MIR/artifact support; old artifacts cannot acquire tasks implicitly. Do not expose frame pointers or private handles as component values. Replacing FIFO/observation/failure rules requires an RFC revision and differential migration fixtures.

## Diagnostics and tooling

Structured errors distinguish busy/no active dispatch, foreign/invalid handle, wrong active task, wrong phase, non-child join, duplicate root, cancellation pending, live/total limit and preparation capacity failure. Rejected operations leave scheduling state unchanged. Task origins retain optional validated MIR source spans supplied by the adapter. The kernel exposes read-only status/parent/cancellation information; formatter/LSP/source diagnostics and DAP ancestry remain the source/tooling deliveries.

## Performance and cost model

Prepare task metadata and queue capacity once per invocation. Retain at most the total-task limit entries. No per-transition allocation is required after preparation. The initial bounded implementation may scan ancestry/state to settle joins and propagate cancellation; it promises no constant-time transition or throughput target. Benchmark total-task sizes and cancellation fan-out before stabilization. Do not infer performance from the old single-task prototype. M2 execution paths remain unchanged until actual task adapters are added.

## Security and trust boundaries

The scheduler is a trusted host facility, not an untrusted bytecode verifier. Invalid public operations fail before effects; private handles protect invocation identity. Limits bound task count and metadata, not arbitrary callback CPU or memory. The adapter must poll shared execution controls, admit frame allocations before spawn and continue mandatory cleanup after control exhaustion. No native in-process sandbox is claimed. Fallible metadata preparation reports capacity failure before a root is published.

## Alternatives

- **No change:** the prototype cannot establish structured tree lifetime or drive executable frames.
- **Library-only futures:** suitable behind an adapter, but do not supply independent guest ownership/loan verification or equivalent native cleanup.
- **Work-stealing multithreaded executor:** supports parallelism, but adds synchronization, wakeup and memory-accounting contracts before the first source fixture. This design leaves that executor possible behind the accepted task semantics.
- **Run each child to completion immediately:** simpler, but cannot expose suspension, bounded concurrent admission or cancellation of queued siblings. It does not satisfy the selected execution outcome.

## Implementation plan

1. M3-T02: bounded kernel and actual host-driven spawn/yield/join/cancel/cleanup fixture.
2. M3-T03/T09/T10/T04: decide/parse source syntax, check task typing and suspension ownership, and add independently verified typed MIR.
3. M3-T05/T06: source fixture through VM then compiled native continuations, preserving events and spans.
4. M3-T07/T08: actual owned-frame cleanup and combined execution controls. Do not substitute kernel-only events for these gates.

## Test plan

Exercise FIFO dispatch, both normal and exceptional scope exit, nested descendants, joins of completed children, cancellation during active resume/cleanup, failure ordering, cleanup failures and one acknowledgement per task. Reject wrong invocation/parent/phase and reentry without changing state. Test live/total capacity at exact boundaries and slot retention after completion. Execute the host fixture as a runnable example. Later differential/source/ABI/fuzz/platform coverage follows the board; kernel tests alone cannot complete M3.

## Open questions

No blocking question for the implemented kernel and source adapters. RFCs 0022–0027 resolve source syntax, Send/Share, channels, services and production embedding; [M3 conformance](../milestone-3-capabilities.md) verifies reservation, cancellation and original-task callback scopes. Scanning/dispatch costs remain workload-dependent observations, not a parallel-executor or latency guarantee. The decision below records the earlier acceptance state.

Decision: **accepted under RFC 0001 experimental delegation, 2026-10-20 JST**. The explicit driver preserves RFC 0005 while separating scheduling from already validated guest cleanup. No owner approval wait is required. Source syntax is accepted in RFC 0022; channels/Send/Share and the production embedding profile remain undecided.

## Sources

- [RFC 0005](0005-structured-tasks-and-isolated-services.md), last updated 2026-10-21: accepted task lifetime/cancellation rules and follow-up mapping.
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md), last updated 2026-10-20: implemented cleanup/failure boundary.
- [RFC 0012](0012-native-ownership-continuations.md), last updated 2026-10-20: implemented native managed continuations.
- Repository `crates/semantics/src/lib.rs`, `crates/runtime/src/execution.rs` and [architecture](../architecture.md), inspected at `984557d` on 2026-10-20 JST. The single-task semantic model's 29 library tests pass on this date; this is baseline evidence, not kernel implementation evidence.
- All scheduler choices above are project decisions. No new external-language or performance claim is made.

- C05 implementation evidence: `crates/runtime/src/task.rs`, `task/tests/wait.rs` and `examples/parked-tasks.rs`; no embedded code update date, inspected/executed 2026-10-21 JST on macOS AArch64. Wait tickets hold private identity/ordinal; the kernel owns no guest messages and supplies no new channel source/ABI/wire evidence.
