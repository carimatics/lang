<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0025-bounded-channels-and-backpressure.md -->

# RFC 0025: Bounded channels and backpressure

[日本語版](../ja/rfcs/0025-bounded-channels-and-backpressure.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-21
- Last updated: 2026-10-21
- Tracking issue: M3-C01, M3-C02, M3-C03, M3-C04, M3-C05, M3-A02
- Supersedes: none
- Superseded by: none

## Summary

Specify an invocation-local bounded multiple-producer, single-consumer channel for RFCs 0005/0021/0022/0024. A full send waits with ownership of its uncommitted payload. Successful enqueue commits ownership at one FIFO position; receive moves the oldest committed payload once. Close, cancellation, resource refusal and a stalled closed invocation have explicit outcomes and mandatory cleanup. Sender authority never projects receiver or service state.

This decides a protocol, not an implemented channel profile or stable host ABI. C02 executes the runtime protocol, C03 connects independently checked source/MIR and VM operations, and C04 proves generated native/linked-object parity. The concrete reserved-core call catalog requires a recorded follow-up here before C03 implementation; no second task language or unverified synchronous call may hide suspension.

## Motivation

At `224c1f2`, owned async arguments/results execute, but the production scheduler waits only for child joins and has no channel park/wake operation. `semantics::ServiceModel` is an earlier semantic experiment with caller-supplied Sendable classification and an unbounded mailbox. Neither implements bounded production channels. RFC 0005 leaves the full-mailbox policy undecided and requires FIFO accepted messages, owner isolation and cancellation disposal.

## Goals

- Finite capacity, FIFO committed values and FIFO admission of blocked senders.
- One owner of each payload at preparation, waiting, publication, receive and retirement.
- Explicit close/cancellation/failure precedence and shared accounting on VM/native.
- Source/MIR capability, effect and loan proofs independent from backend code.

## Non-goals

Rendezvous capacity zero, multiple consumers, select, timers, priorities, broadcast, detached tasks, OS threads, cross-invocation handles, remote transport and a public C/component channel ABI. This RFC does not supply actor state projections or a general future/closure interface.

## Guide-level design

The following is a protocol trace, not runnable source spelling:

```text
capacity = 1
send(A)           -> committed at position 0
send(B)           -> waiting; B remains owned by the sending operation
receive()         -> A; oldest sender becomes eligible
resume send(B)    -> committed at position 1
close receiver    -> seal sends; keep committed values available
receive()         -> B
receive()         -> closed
```

Ordinary source move analysis consumes the send operand when the operation is evaluated. A closed send returns that owned payload in its failure value; it does not restore a moved source variable. Cancellation of the sending task instead retires its pending payload. Once enqueue has committed, cancelling the sender does not retract the message.

Source construction must work with RFC 0009's whole-value extraction and borrowed matches, without assuming owning tuple/field destructuring. The construction protocol uses a unique local setup authority: create bounded storage; explicitly mint sender tickets; consume setup into the unique receiver. Setup includes one temporary producer keepalive, released on conversion; a receiver with no minted senders is already send-closed. Dropping setup aborts the channel. This chooses construction semantics; the later call catalog fixes the names/signatures and failure sums before implementation.

## Reference semantics

### Types, authority and lifetime

| Authority | Copy | Send | Share | Operations |
| --- | --- | --- | --- | --- |
| Setup<T> | no | no | no | mint sender; consume into receiver; abort on retirement |
| Sender<T> | no | T is Send | T is Send | explicit clone, send, retire |
| Receiver<T> | no | T is Send | no | receive, seal, retire |

These are abstract protocol roles, not assigned source or wire tags. Source and MIR must derive the eventual concrete constructors independently, including unused declarations and transitive containers/shared values. Existing `is_share == is_send` for the old closed family cannot be reused for Receiver or Setup. Message type T requires RFC 0024 Send and existing storage/region validation. Sender's Share does not require observing T: its interface only moves messages. No handle is implicitly cloned. Receiver cannot be cloned or shared to manufacture a second consumer.

Receiver/setup exclusively owns the queued payload storage. Sender retains only checked identity/closed-state metadata; it cannot keep queued values alive after receiver retirement. Each operation temporarily pins validated endpoint metadata until completion/cancellation. Counts, generations and invocation identities are checked; exhaustion fails closed before effects. A numeric index alone authorizes nothing. Ending the last sender, including in-flight producer tickets, seals future sends after setup's keepalive has ended. Existing committed values remain receivable.

Handle-bearing payloads are not rejected merely for containing handles. Publication must reject a strong ownership cycle through receiver/setup, queue, container or shared storage before enqueue, including a receiver sent into its own mailbox. Sender identity edges do not own queued payloads. Extend the existing bounded publication inspection and mixed retirement rules; do not introduce a trusted caller-supplied Sendable/cycle-free bit. Acyclic handle transfer, aliases and recursive nominal types retain their checked behavior. Failure leaves the incoming payload under operation ownership.

### Capacity, preparation and commit

Capacity is a checked positive count fitting the configured channel, slot and allocation bounds. Zero/negative/overflowing capacity is refused before allocation/publication. The host explicitly supplies invocation allocator/scheduler authority and finite channel, slot, handle and waiter limits; source metadata cannot grant them. Preparation reserves queue storage and bounded wait bookkeeping. At most one channel operation is pending per logical task; no hidden child task is spawned for it.

Evaluate arguments once left-to-right. Validate type, endpoint identity/generation, availability, publication graph and destination storage under shared limits before the atomic ownership transition. A non-Copy payload moves through its existing reservation and initialization metadata, never a bit-copied token. Allocation/control failure before commit publishes nothing and retains all initialized values for failure cleanup. A closed or invalid-publication send returns its original owned payload with the typed protocol reason; control/resource exhaustion is an execution failure, not a successful send or cancellation value.

Commit has no intervening guest-fuel, allocation, interruption or callback boundary. Send commit removes the pending owner, appends one initialized queue entry and records its enqueue position. Receive commit removes the oldest queue owner and initializes one destination, preserving spans, allocator identity and transitive ownership. No second owner or ownerless interval is observable. Cleanup callbacks run only outside commit. A received value retained by a suspended/cancelled task is charged and retired as that task's value, not again as a queue item.

### Backpressure and scheduling

A full send registers a bounded waiter and parks the task. An empty open receive parks the unique receiver operation. Registration stores a checked task identity, continuation/origin and globally ordered registration ordinal. No guest loan or native stack frame is retained. Invocation storage keeps uncommitted payloads initialized and charged until actual transfer/retirement.

Blocked senders become eligible FIFO by registration for that channel. Only the waitlist head may commit; a new sender joins behind existing waiters even if physical storage has space. A freed slot belongs to the oldest still-live eligible sender until it commits or is unregistered. After that transition, make the next head eligible if space remains. Waking appends the eligible task at the existing global ready queue tail exactly once; it does not recursively execute guest code. A sender whose reserved turn is cancelled is unlinked before the next waiter becomes eligible. A receive wakes when a committed value or terminal close is observable. Revalidate publication/endpoint state when a parked operation becomes eligible; another task may have changed reachable ownership while it waited. No waitlist scan may be unbounded or allocate after admission; use bounded links/indices and existing task identity checks. This promises ordering, not wall-clock fairness or scheduling between different channels.

Send/receive are explicit async effects and potential suspension/cancellation points, even when they can complete immediately. They may complete in the current dispatch when not blocked; there is no mandatory extra task or yield. Source/MIR reject every ordinary loan live at the actual operation boundary. Compiler-created argument temporaries may end after endpoint authority has been captured into a private operation ticket and before that boundary, as with existing allocator capture; separately bound loans keep their ordinary lexical lifetime. This never passes a guest reference across suspension. The eventual core catalog must expose this boundary to both proofs, not special-case it in a backend.

### Close, cancellation and stalled execution

Receiver seal is idempotent: reject new sends, resolve waiting sends as Closed with their uncommitted payloads, and allow FIFO draining of already committed values. After the queue empties, receive returns Closed without a T. Last-producer closure has the same drain behavior. Receiver/setup retirement aborts: mark receive authority unavailable first, detach the queue, then retire committed payloads FIFO exactly once. Continue after cleanup failure/report truncation. Pending sends are not committed queue items; each remains owned by its operation until a Closed return or cancellation cleanup.

Cancellation is checked before registration and commit. Cancelling a parked operation unlinks/invalidate its registration and revokes its reservation before mandatory frame cleanup; a stale wake cannot resume it. A cancelled sender retires its uncommitted payload once. A cancelled receiver's frame retires its unique receiver, aborting queued messages before that handle finishes retirement. Commit observed before cancellation remains committed; no rollback removes a message that the receiver may already own. Source task cancellation retains RFC 0022's terminal/TaskError boundary and is not converted into an ordinary Closed return. A trusted host protocol adapter may expose Cancelled with the still-owned uncommitted payload, but must choose either return or retirement, never both.

In the current closed invocation, no ready/active task plus unfinished tasks and channel waiters is a deterministic ChannelDeadlock execution failure, not success, an invariant panic or an indefinite busy loop. Select the oldest still-pending registration as the origin, record it before cleanup, request tree cancellation and drain all registrations/owners through mandatory cleanup. Join-only ancestors do not mask the blocked descendant. Future external polling/timer/host wake capabilities require the production embedding decision before changing this rule; no ambient external event is assumed.

Keep RFCs 0008/0021's primary-failure and poison rules: an earlier execution panic/control failure stays primary; cleanup failures are secondary; cancellation/cleanup precedence is unchanged. Per-channel committed payload disposal is FIFO; ordinary frame owners retain reverse actual initialization order. A03 must establish service queue disposal before private owner-state disposal. No new global order between independent channels is promised.

### Accounting and protocol errors

Queue entries, endpoint metadata, registrations, pending/received values and retained tombstones count against the invocation's actual allocator and logical limits until released. Every reserved queue slot counts as one live value cell from preparation to storage release, including when empty; pending/result reservations use existing cell accounting without counting the same storage twice. Limits also bound cumulative metadata/ordinal exhaustion; capacity is not permission to exceed heap/cell/handle/task limits. Cleanup and cancellation bookkeeping require no new guest allocation and remain unmetered/non-preemptible under RFC 0013.

Extend service-work explicitly: charge capacity units before preparing capacity-sized queue storage, one unit before each explicit mint/clone/seal operation, one per send/receive attempt, and one before each send/receive commit. New attempt/commit units do not charge again merely because an operation is resumed or woken; a failed precommit inspection retains already consumed work. Existing graph/element inspection charges remain in force. Waitlist unlink, wake notification and mandatory closure/retirement are bookkeeping, not new guest service events. Work/fuel/interruption refusal before a chargeable effect leaves ownership recoverable by cleanup. Every backend must expose the same numeric usage and refusal origin. These are project cost choices, not performance measurements.

Typed protocol refusals distinguish Closed and InvalidPublication and retain a failed send's payload. Invalid capacity/allocation is a construction refusal. Foreign/stale/wrong-role identity, type mismatch and duplicate pending receiver are structured trust-boundary failures. Channel/slot/handle/waiter/control exhaustion and ChannelDeadlock are execution failures. Concrete diagnostic variants and core error-sum spellings are recorded before their implementation; no backend string classification is permitted.

## Execution modes

Independent source and MIR proofs retain concrete message/handle types, moves, ended loans, async effects, pending continuations, commit/failure/cancellation cleanup and source spans. The shared invocation driver owns the protocol and services; VM dispatch and native generated continuations use the same checked operations. Native callbacks perform storage/scheduling services, never MIR evaluation or VM fallback. Compare values, enqueue/wakeup order, origins, counters, reports, poison and physical retirement, including separately linked objects. Unsupported profiles refuse before guest/provider effects or artifact publication.

Source checking must not promote channels solely because types parse or an old semantic model succeeds. Constant execution remains unavailable. REPL, C imports/exports, foreign callbacks and cross-invocation handles retain existing refusal until their accepted production profile is implemented. No OS-thread Send/Sync guarantee is inferred.

## ABI and compatibility

This follow-up selects RFC 0005's bounded full-send policy and RFC 0024's concrete endpoint capabilities; it preserves ordinary move/loan and owner-isolation rules. Existing synchronous and task programs retain their behavior/event costs. The later core catalog uses reserved names and existing call syntax or records any required syntax change explicitly. It may not assume new partial-move patterns.

No C layout, native public ABI, serialized MIR/bytecode tag or native-package profile is assigned. Old schema-2/3/4 writers/readers and runtime layouts must reject channel metadata/operations, even unused, rather than infer an opaque token fallback. A serialized task/channel revision and migration tests are required before persisted execution. New handles cannot be laundered through legacy host/foreign types.

## Diagnostics and tooling

Keep SRC0020 move, SRC0023 storage/authority, SRC0027 async-effect and SRC0028 live-loan identities with operation/loan origins. Rollout retains SRC0026 or a structured unavailable-profile error until the complete relevant proof/adapter exists. Record new finite channel error identities before implementation. Existing CST formatting/LSP uses the same resolved call/effect facts and preserves comments/spans; no alternate permissive analysis mode. Future debug views show role, logical channel identity, queue state and waiting origin, never an address. This does not claim DAP/linter completion.

## Performance and cost model

Bound queue storage and wait metadata before admission; do not grow an implicit mailbox. Endpoint clone retains checked metadata and never clones messages. Enqueue/dequeue move reservations and do not deep-copy payload graphs. Publication validation may traverse stored ownership edges under existing work/storage limits; cancellation/retirement must remain iterative. No constant-time, latency, throughput or allocation-free guarantee is made. Z01 measures queue/retained-payload peaks, send/receive/wakeup/cancellation work and representative M2 costs on both tier-1 hosts.

## Security and trust boundaries

Untrusted source cannot assert Send/Share, forge endpoint/task identity, clone receiver authority, bypass publication inspection or acquire ambient scheduler/allocator/host permissions. Revalidate untrusted MIR and future wire metadata independently. A count/generation overflow or exhausted traversal cannot authorize an unchecked fallback. Trusted adapters must retain pending owners and acknowledge cleanup only after actual retirement. Native providers/callbacks retain their existing trust/no-unwind contracts; this is not a native-process sandbox.

## Alternatives

- No change leaves bounded source channels and actor services unimplemented; the old model is insufficient.
- A library may build services after these primitives exist, but cannot add compiler-visible suspension, independent move/loan proof or trusted commit semantics alone.
- Unbounded queues avoid blocked sends but violate finite memory/backpressure requirements. Fail-fast Full is simpler but cannot provide the selected waiting producer contract; a future explicit try-send may coexist without changing this default.
- Rendezvous, multi-consumer/select and parallel lock-free queues are stronger competing designs with additional fairness, cancellation and memory-order contracts. Defer them rather than infer them from FIFO in one serialized invocation.
- Implicit Copy endpoints or two owning pattern bindings would conceal retain/authority costs or change RFC 0009; explicit setup/clone/whole moves preserve current rules.

## Implementation plan

1. C01: accept this protocol, revise affected RFCs/architecture and preserve decision versus implementation/gate status.
2. C05 executes bounded checked task park/wake, wait-identity reuse and cancellation before/after wake, closing the missing kernel prerequisite for C02. This supports the channel fixture, not a general executor framework. Re-audit C02 against bounded storage, typed ownership commit and cancellation; expose any further independently observable missing prerequisite before implementation. Execute a trusted host fixture returning 42 through send/receive, close, blocked-sender cancellation and mandatory disposal. This must be a runtime service used by later adapters, not another semantic-model framework.
3. Before C03, record the exact core catalog, constructor graph rules, diagnostic/error variants and typed MIR operations here or in an accepted follow-up. Then connect source/independent MIR/VM; run the next source channel fixture and payload-reuse failure. Preserve all accepted handle-bearing/publication outcomes even if staged admission initially refuses them.
4. C04: generated native and linked-object values, suspension, failure origins/usage and retirement parity. A02/A03/A04 retain concrete actor authority, state isolation and queue-before-state cancellation disposal.
5. Existing embedding, resource/adversarial, both-host conformance, serialized/package, cost and final-guide deliveries remain required. Do not mark a gate complete from this decision or a host fixture.

## Test plan

Check capacity one/exact/max/refusal, sender FIFO with late arrivals, multiple freed slots, closed drain, last sender, setup abandonment, receiver abort, already-woken cancellation, stale/foreign/wrong-role identities, duplicate wake, count/generation/ordinal exhaustion and all-blocked deadlock. Track every payload from staging through commit/receive/return/retirement, including owned records/sums/collections/shared/weak and acyclic endpoint-bearing values. Reject direct/transitive strong publication cycles atomically. Exercise nested scopes, cancellation on either side of commit, panic/cleanup panic/truncated reports and service queue-before-state disposal.

Source compile-pass/fail tests must prove endpoint capabilities, explicit clone, unique receiver, payload reuse, invalid effects and live-loan suspension rejection; forge raw MIR to repeat those checks independently. Differential VM/native/linked-object tests cover every applicable fuel/work/provider/interruption prefix and exact/one-below queue/handle/waiter/frame/cell/heap/task limits. Add raw graph/wire fuzz mutations when the profile exists; compilation is not a fuzz campaign. Run both-host conformance and Z01 measurements before exit/stabilization.

## Open questions

Concrete source names/error sums/MIR forms are a mandatory recorded follow-up before C03, not permission to guess during implementation. Unverified assumptions: existing managed publication/retirement can extend to endpoint edges without another owner registry; admitted wait links can keep wake/cancel transitions allocation-free; setup/clone spelling remains usable under existing whole-value rules; representative costs and both-host equivalence are acceptable. No implementation, stability or performance result follows from this RFC.

Decision: **accepted under RFC 0001 experimental delegation, 2026-10-21 JST**. Bounded MPSC, unique receive authority, explicit Send/Share, custody of uncommitted payloads, atomic publication, FIFO admission and existing cleanup preserve strict ownership/loan rules. A capacity-one trace experiment exposes A,C,B overtaking under naive wake and preserves A,B,C with head reservation. Five traces also check B cancellation after wake, C sender cancellation after commit, seal/abort and one owner/return/retirement per payload. This is a protocol experiment, not production channel execution evidence. Keep the assumptions and concrete core catalog explicitly unverified/unimplemented.

## Sources

- [RFC 0001](0001-project-charter.md), last updated 2026-10-17, including experimental owner delegation dated 2026-10-17 JST; verified 2026-10-21 JST. [RFC 0005](0005-structured-tasks-and-isolated-services.md), last updated 2026-10-21: strict ownership, accepted-message FIFO, cancellation and the previously open backpressure choice.
- [RFC 0021](0021-bounded-structured-task-driver.md), [RFC 0022](0022-source-structured-tasks.md) and [RFC 0024](0024-send-share-and-service-authority.md), last updated 2026-10-21: scheduler, task effects/loans and logical Send/Share. [RFC 0009](0009-whole-sum-ownership-extraction.md), last updated 2026-10-20: whole-value extraction without owning partial moves.
- [RFC 0007](0007-source-ownership-and-core-values.md) and [RFC 0013](0013-service-byte-work-budget.md), last updated 2026-10-21, verified 2026-10-21 JST: checked publication, captured allocator authority and shared service-work/unmetered retirement.
- `crates/runtime/src/task.rs`, `task/runner/`, `crates/semantics/src/service.rs` and source/core/MIR boundaries inspected at `224c1f2` on 2026-10-21 JST; no embedded code update dates. The current driver has no channel wait state; the old model supplies no bounded runtime evidence. These are repository facts. MPSC, roles, FIFO admission, seal/abort, costs and deadlock handling above are project decisions; ergonomics/performance/reuse are hypotheses. No external-language claim is needed.
