<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0005-structured-tasks-and-isolated-services.md -->

# RFC 0005: Structured tasks, cancellation, and isolated services

[日本語版](../ja/rfcs/0005-structured-tasks-and-isolated-services.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-16
- Last updated: 2026-10-21
- Tracking issue:
- Supersedes:
- Superseded by:

## Summary

Define the initial concurrency and cancellation baseline:

- Child tasks are scoped: a parent scope cannot exit until every child completes or is cancelled and joined.
- Cancellation is cooperative, propagates through the task tree, and is observed at explicit suspension/cancellation points rather than by preempting arbitrary code.
- Task frames own their initialized values across suspension and destroy them exactly once in reverse initialization order after borrows end.
- No shared or exclusive borrow may cross a suspension point in the initial language. Such a suspension is rejected as `concurrency.borrow-across-suspension` (`SEM0007` in the prototype).
- Mutable service state is owner-task isolated. Shareable service handles may move sendable payloads into one mailbox but cannot mutate state directly.
- Accepted messages are processed in mailbox-enqueue order. Cancellation waits for an active handler boundary, then discards queued payloads in order, destroys service state once, and rejects later sends.
- The base model exposes neither general shared mutable state nor an implicit global executor.

[RFC 0021](0021-bounded-structured-task-driver.md) accepts the first bounded driver policy and [RFC 0022](0022-source-structured-tasks.md) accepts source task spelling. Service/channel APIs and multithreaded scheduler choices remain provisional. The lifetime, isolation, ordering, cancellation, cleanup, and cross-mode rules are accepted semantic commitments.

## Motivation

Async lowering decides where values live, when borrows end, and what cancellation destroys. Service concurrency decides whether a shared handle conveys mutation authority and whether message payloads can alias across tasks. Deferring these decisions would let source lowering, MIR, VM, native execution, and embedding acquire incompatible safety rules.

Swift structured concurrency is primary-source evidence that child-task lifetimes, cancellation, priorities, and deadlines benefit from a task tree and scoped joins. Pony reference capabilities are evidence that aliasing guarantees can make actor communication data-race-free. These facts inform but do not determine this proposal.

The repository prototypes compare strict and less-restrictive policies. `RejectAll` rejects every borrow across suspension; `AllowSharedFrameBorrows` keeps a shared frame borrow across suspend/resume and ends it before cancellation cleanup. `OwnerIsolated` limits service mutation to one owner; `ExclusiveCapability` permits a handle holder to mutate under one dynamic capability. Both alternatives execute, but they do not establish pinning, address stability, lock fairness, poisoning, deadlock, backpressure, or acceptable cost. The strict pair forms a coherent initial baseline without those hidden contracts.

Library-only tasks and actors cannot enforce borrow termination, compiler-derived sendability, owner isolation, at-most-once cleanup, or backend-equivalent diagnostics.

## Goals

- Define scoped task lifetime and cooperative cancellation.
- Define ownership and destruction of task-frame values.
- Keep borrows out of suspended frames in the initial language.
- Make mutable services actor-isolated and message payload transfer explicit.
- Derive send/share capabilities from types rather than ambient runtime checks alone.
- Specify ordering, cancellation, and cleanup observably and equivalently across VM and native modes.
- Preserve source spans and stable logical diagnostics.
- Allow embedded hosts to provide or drive a scheduler.

## Non-goals

- Final `async`, task-group, service, mailbox, deadline, or select syntax.
- Borrows across suspension, self-referential/pinned task frames, generators, or stackful coroutines.
- Locks, read/write guards, atomics, memory ordering, transactional memory, or general shared mutable objects.
- Scheduler fairness, work stealing, thread counts, priority inheritance, real-time guarantees, or distributed actors.
- Mailbox backpressure policy, supervision trees, restart policy, persistence, or remote transport.
- Complete panic/destructor-failure transport; component-boundary failure remains governed by RFC 0003.

## Guide-level design

The spelling below is a historical policy illustration; [RFC 0022](0022-source-structured-tasks.md) defines the accepted source spelling and its current implementation limits. A structured scope owns every child:

```text
async fn load_page(urls: List<Url>) -> Result<Page, Error> {
    task_scope tasks {
        let header = tasks.spawn(fetch_header(urls[0]));
        let body = tasks.spawn(fetch_body(urls[1]));
        return Page(await header, await body);
    } // unfinished children are cancelled and joined before exit
}
```

A borrow must end before `await`:

```text
let view = data.read();
consume(view);
end view;
await next_event(); // accepted
```

```text
error[concurrency.borrow-across-suspension]: borrow is live at suspension
  --> worker.lang:9:1
   |
 7 | let view = data.read()
   |            ----------- borrow begins here
 9 | await next_event()
   | ^^^^^^^^^^^^^^^^^^ suspension would retain the borrow
   |
   = help: end the borrow before `await` or move owned data into the task
```

A service owns its mutable state. Handles send moved payloads:

```text
service Counter {
    state value: i64 = 0;

    on Add(amount: i64) {
        value = value + amount;
    }
}

let counter = spawn service Counter();
counter.send(Add(move amount));
// `amount` is unavailable here; the owner processes `Add` in enqueue order.
```

Direct mutation through `counter` is rejected as `concurrency.service-isolation` (`SEM0031`). Cancellation requested through a valid handle is cooperative: an active handler reaches its boundary, queued messages are discarded and dropped in enqueue order, state is dropped once, and future sends fail as unavailable.

## Reference semantics

### Structured tasks and suspension

1. A task has one parent except for an explicitly detached host/root task. Ordinary child creation occurs inside a lexical task scope.
2. Scope exit waits for all children. Normal exit joins them; exceptional or cancellation exit first requests cancellation, then joins them. No child accesses a destroyed parent frame.
3. A task frame owns every successfully initialized local not moved elsewhere. Suspension preserves those owned values.
4. A suspension is invalid while any shared or exclusive borrow is live. Rejection identifies the suspension and borrow origin. A borrow ended before suspension is valid.
5. This rule avoids address-stability and pinning promises. A suspended frame may move in memory without changing source behavior.
6. Child result transfer moves an owned result or returns a typed error. Unobserved child failure cancels sibling work according to the task-scope API; exact error aggregation belongs to the error RFC.

### Cancellation and cleanup

1. Cancellation is an idempotent request, not asynchronous destruction. It propagates from a cancelling scope to unfinished descendants.
2. A task observes cancellation before entering or while suspended at `await`, explicit yield, mailbox receive, timer/deadline wait, or explicit cancellation check. User code between points is not preempted.
3. A callback or handler already executing reaches the next semantic boundary before cancellation cleanup. Native signal/exception injection is not the language mechanism.
4. Before destroying a frame, the runtime closes compiler-managed borrow scopes, joins/cancels child scopes, and destroys remaining owned values exactly once in reverse initialization order.
5. Cleanup runs for success, typed error, panic containment, deadline cancellation, and explicit cancellation. Failure ordering and suppression must be deterministic; exact error encoding remains separate work.
6. A deadline is scheduler input that requests cancellation when observed. Wall-clock accuracy and callback preemption are not guaranteed.

### Isolated services and messages

1. A service has one owner task with exclusive mutable access to its state. A shareable handle conveys enqueue and cancellation-request authority only.
2. Direct state mutation from a non-owner fails as `concurrency.service-isolation`. The proposed source type system rejects it before execution when identity is known; validated dynamic boundaries retain the check.
3. Sending consumes a payload. Reuse after successful send is use-after-move. A payload is sendable when it contains no live borrow, arena reference, non-sendable host handle, or mutable alias and every field is recursively sendable.
4. The compiler derives `Send`; safe source cannot assert it manually. `shared T` is sendable only when `T`'s exposed operations satisfy the later `Share`/synchronization rules. This RFC does not make arbitrary shared mutation sendable.
5. A successful enqueue linearizes at one service mailbox position. The owner processes accepted messages FIFO by that position. Scheduling between concurrent senders is not deterministic, but a recorded enqueue order has identical meaning in every backend.
6. Message handling owns the payload for the handler duration. On success or failure it is destroyed or moved exactly once.
7. Cancellation waits for the current owner handler boundary. It then marks the service unavailable, discards queued messages in FIFO order, destroys each queued payload and service state once, and rejects subsequent send/receive/mutation operations.
8. Mailbox capacity and backpressure are host/runtime configuration. A later RFC must specify whether a full send suspends, fails, or applies an explicit policy; unbounded implicit growth is not a semantic guarantee.

### Unsafe, embedding, and constant execution

`unsafe` and host code that construct sendable values or service handles must establish the same ownership, alias, and liveness invariants. Native callbacks cannot mutate isolated state without entering the owner service operation. Untrusted components receive explicit task, timer, and service capabilities with resource limits; they do not gain ambient scheduler or thread access.

Constant evaluation does not create runtime tasks or services. A future deterministic compile-time scheduler requires a separate capability/resource design.

## Execution modes

Typed MIR is the shared contract. It records task-scope enter/exit, spawn, suspension, cancellation check/request, join, message move/enqueue/dequeue, service-owner mutation, and cleanup edges with source spans. Validation rejects live borrows across suspension, escaping child handles, use-after-send, non-sendable payloads, and non-owner mutation.

The bytecode VM may schedule cooperatively in one host thread. Native AOT/JIT may use threads or an event loop. Given the same ordered external events and enqueue order, all modes must agree on values, diagnostics, cancellation observation points, ownership moves, cleanup order, and service availability. Thread choice and timing are not observable guarantees.

Embedders may poll, block, or integrate the scheduler. Component interfaces pass owned/sendable resources through RFC 0003 adapters; raw task frames, wakers, mailbox pointers, and service state are not stable ABI values.

## ABI and compatibility

Task-frame layout, coroutine state numbers, wakers, executor queues, mailbox representation, and thread mapping are unstable implementation details. Typed MIR, bytecode, package capability declarations, and component adapters remain independently versioned.

Adding/removing `async`, changing payload sendability, changing a service operation's isolation, or changing cancellation points is compatibility-sensitive. Public metadata must eventually record async and sendability/isolation effects. Migration tooling may add explicit moves or scopes only when behavior is unambiguous; it cannot silently detach a child or make state shared-mutable.

## Diagnostics and tooling

- Diagnostics retain the suspension/send/mutation span plus related borrow, move, owner, or active-handler span.
- Stable identities include `concurrency.borrow-across-suspension`, `concurrency.payload-not-sendable`, `concurrency.payload-moved`, `concurrency.service-isolation`, and `concurrency.service-unavailable`.
- The formatter preserves task/service boundaries; LSP hover shows `async`, `Send`, owner isolation, and cancellation points.
- The linter reports detached tasks without documented lifetime, ignored child results, cancellation loops without a check, and potentially unbounded mailboxes without claiming scheduler behavior it cannot prove.
- DAP views show task ancestry/state, cancellation request/observation, mailbox position, and service owner without exposing raw runtime addresses.

## Performance and cost model

Structured scopes require child bookkeeping and joins. Suspension stores live owned values in a frame but, under the strict policy, stores no live borrow tokens. Message send performs a move plus mailbox synchronization; service handles and cancellation flags require atomic or scheduler-synchronized state in multithreaded runtimes.

The language guarantees no scheduler, latency, allocation, or throughput budget. Implementations may elide frames for non-suspending async calls, batch mailbox operations, specialize single-threaded embeddings, and remove redundant cancellation checks while preserving observable points.

Before stabilization, measure spawn/join, suspend/resume, cancellation fan-out/cleanup, message throughput and latency under one/many senders, queued-message cancellation, frame/mailbox peak memory, and owner-isolated versus experimental exclusive-capability paths across VM, macOS AArch64, and Windows x86-64.

## Security and trust boundaries

Untrusted bytecode, task metadata, handles, messages, and host wakeups are validated. Limits cover live tasks, descendants, frame bytes, mailbox messages/bytes, total sends, deadlines, and host callbacks. Counter exhaustion and malformed state transitions fail closed before side effects. Cancellation does not authorize new capabilities or bypass cleanup.

Payload sendability is checked before ownership commit. Enqueue failure leaves the sender owning the payload. Successful enqueue makes the sender's old value unavailable. Service cancellation and queue disposal do not invoke arbitrary code while mailbox invariants are partially updated.

## Alternatives

### No change

Leaving async ownership and service isolation open would make coroutine lowering and runtime scheduling accidental semantics.

### Library or tooling solution

Libraries can expose futures and actors but cannot enforce no-borrow-at-suspension, compiler-derived sendability, lexical child lifetime, or uniform cleanup across backends. Lints are insufficient for dynamic ownership transfer.

### Shared frame borrows and exclusive mutation capabilities

The strongest competing prototype permits shared task-frame borrows across suspension and lets any valid service-handle holder acquire one mutation capability. It can reduce copies and support lock-like APIs, but requires stable frame residency/pinning plus lock scheduling, fairness, poisoning, deadlock, reentrancy, and cancellation contracts. Those costs and rules are not established, so this RFC chooses the strict baseline while preserving the prototype for a later revision.

### Single-threaded event loop only

A single event loop is simpler and useful for freestanding/embedded subsets, but it cannot express parallel task execution or isolate a service handle used from multiple host threads. It may implement the semantics, not define their only execution strategy.

### Detached futures by default

Unscoped futures simplify spawning but permit children to outlive borrowed/owned parent context and weaken cancellation/deadline propagation. Explicit detached tasks may exist behind owned `'static`-like and capability requirements; they are not the default.

## Implementation plan

The strict isolation/cancellation policy is accepted. RFC 0021 implements the bounded host task driver; RFC 0022 implements lossless syntax, resolved async effects, non-executable typing/move/scope snapshots and separate suspension-loan proofs, while explicitly refusing the unavailable execution profile. Independent MIR and VM/native adapters remain separate M3 deliveries. Channel/backpressure, Send/Share derivation and the production component cancellation/embedding profile still require their explicit decisions before dependent implementation. Existing task/service state models do not complete those production outcomes.

1. Add source/HIR task scopes, suspension points, moves into messages, service-owner operations, and compile-pass/fail fixtures.
2. Add typed MIR operations and validation for no live borrow at suspension, scoped child handles, `Send` derivation, and isolation.
3. Generalize the existing task/service models into one deterministic reference interpreter with limits and stable diagnostics.
4. Implement VM scheduling and cleanup traces, then differential JIT/AOT execution for the same ordered event fixtures.
5. Integrate explicit scheduler/service capabilities and RFC 0003 resource adapters without exposing runtime layouts.
6. Fuzz malformed task state, cancellation races, enqueue commit, queue disposal, and handle generations.
7. Record the runtime/memory benchmarks above on both tier-1 targets.
8. Add formatter, linter, LSP, DAP, guide, and migration support before stabilization.

## Test plan

- Compile-pass: borrow ends before await, owned locals survive suspension, scoped children join, send moves payload, owner mutates service.
- Compile-fail: shared/exclusive borrow across await, child handle escape, payload reuse, non-sendable payload, non-owner mutation.
- Runtime: cancellation at each point, child propagation/join, reverse cleanup, FIFO accepted messages, active-handler boundary, queued payload disposal, later-send rejection.
- Differential: VM/JIT/AOT values, events, diagnostics, cancellation points, queue order, and cleanup traces.
- Property/fuzz: at-most-once destruction, no child escape, no payload alias after send, no non-owner mutation, idempotent cancellation, bounded resource accounting.
- Performance/platform: the cost series listed above on macOS AArch64 and Windows x86-64.

## Open questions

There are no blocking semantic questions for the accepted strict initial policy. The repository owner accepted it on 2026-10-17.

Optional follow-ups include exact surface APIs, detached-task requirements, mailbox backpressure, supervision/restart, static executor specialization, and whether pinned shared-frame borrows or explicit mutation capabilities deserve separate RFCs.

Decision: **accepted on 2026-10-17** by the repository owner.

## Sources

- [Swift Evolution SE-0304: Structured Concurrency](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0304-structured-concurrency.md), source last changed 2026-07-29; verified 2026-10-16. It specifies scoped child-task lifetime and cancellation propagation.
- [Pony Tutorial: Reference Capability Guarantees](https://tutorial.ponylang.org/capabilities/guarantees.html), living documentation with no published update date; verified 2026-10-16. It documents aliasing guarantees used for data-race-free actor communication.
- `crates/semantics/src/lib.rs`, async-borrow/cancellation model, repository commit `809fee1e09c279d4865811fc86a0972d3d6d37ab`, committed 2026-10-16.
- `crates/semantics/src/service.rs`, isolated/exclusive-capability service model, repository commit `8c473960e2c27a567b61606598c802c9eec9d30e`, committed 2026-10-16.
- [RFC 0004](0004-owned-values-and-cyclic-graphs.md), owned/shared/arena foundations, last updated 2026-10-16.
