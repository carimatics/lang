<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0026-source-isolated-services.md -->

# RFC 0026: Source isolated services

[日本語版](../ja/rfcs/0026-source-isolated-services.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-22
- Last updated: 2026-10-22
- Tracking issue: M3-A02, M3-A03, M3-A04
- Supersedes: none; completes the concrete service catalog required by RFCs 0005/0024
- Superseded by: none

## Summary

Accept opaque setup, owner and producer roles, explicit core operations and a handler boundary for invocation-local isolated services. Only the unique owner can read or replace state; producers can enqueue and request cancellation. Typed MIR must independently prove role, moves, loans and suspension. Acceptance is not execution or stabilization evidence.

The initial state interface uses checked shared reads and whole-state replacement, following the existing managed shared/arena interface. It adds no exclusive state projection, implicit clone, actor declaration syntax, closure or first-class function value. State and messages may contain any independently proved Send/storable family, including Receiver/ServiceOwner; cycle-safe publication remains mandatory.

## Motivation

Ordinary async state plus a Receiver does not supply this contract. In the [composition experiment](../experiments/2026-10-22-service-composition.lang), state Bytes(1) is replaced by Bytes(42), a Bytes(7) message remains queued, then the owner task is cancelled. The existing provider trace is **[1,42,7]**, with result 42 and live allocations 0: ordinary reverse publication retires the new state before the queue. A plain Sender also supplies no service cancellation authority.

This is correctness evidence for a distinct managed ownership boundary, not a throughput/ergonomics benchmark. Keep ordinary task cleanup unchanged. A library may compose the accepted service operations but cannot invent independent source/MIR role/loan proof or the queue-before-state retirement boundary.

## Goals

- Private owner state, message-only producer authority, recursive Send and ordinary moves.
- FIFO bounded messages, current-handler completion on service cancellation, queue-before-state retirement.
- Shared finite accounting, checked dynamic identity and matching VM/native/linked-object evidence.

## Non-goals

OS-thread execution, detached tasks, traits/asserted Send, mutable state projection, automatic state copying, implicit handlers, public native/C layouts and persisted task/service formats.

## Guide-level design

This is accepted spelling; it is not claimed executable before A02/A03.

```text
struct Counter { total:i64 }

fn add(owner:&mut core::ServiceOwner<Counter,i64>, amount:i64)->core::Unit {
    let mut total:i64=0;
    {
        let mut state:&Counter=core::result_unwrap::<&Counter,core::ServiceError>(
            core::service_read::<Counter,i64>(owner));
        total=state.total;
    }
    return core::result_unwrap::<core::Unit,core::ServiceError>(
        core::service_replace::<Counter,i64>(owner,Counter { total:total+amount }));
}

async fn serve(input:core::ServiceOwner<Counter,i64>)->i64 {
    let mut owner:core::ServiceOwner<Counter,i64>=input;
    let mut first:i64=core::option_unwrap::<i64>(
        await core::service_receive::<Counter,i64>(&mut owner));
    let mut added:core::Unit=add(&mut owner,first);
    let mut second:i64=core::option_unwrap::<i64>(
        await core::service_receive::<Counter,i64>(&mut owner));
    let mut added_again:core::Unit=add(&mut owner,second);
    let mut answer:i64=0;
    {
        let mut state:&Counter=core::result_unwrap::<&Counter,core::ServiceError>(
            core::service_read::<Counter,i64>(&owner));
        answer=state.total;
    }
    return answer;
}
```

A caller creates ServiceSetup with state Counter(0) and capacity 2, mints a handle, consumes setup into owner, spawns serve(owner), sends 20 and 22 through that handle, and awaits the scoped task for 42. The producer cannot call read/replace with its handle or project a state field. An explicitly bound read must end before replacement, finish, receive or any suspension. Passing an ordinary loan to an async worker remains forbidden.

## Reference semantics

### Types and core catalog

All names are reserved in core. S and M are explicit type arguments; S and M must independently be Send and storable, including unused constructors. Setup/Owner have two parameters; Handle has only M and grants no knowledge or projection of S.

| Constructor | Copy | Send | Share | Ownership |
| --- | --- | --- | --- | --- |
| ServiceSetup<S,M> | no | no | no | unique unclaimed state/mailbox owner |
| ServiceOwner<S,M> | no | S and M Send | no | unique state/receive authority |
| ServiceHandle<M> | no | M Send | M Send | producer/cancellation ticket; no strong queued/state ownership |
| ServiceError | yes | yes | yes | ordinary data |

Mint/clone on a valid closed identity may create another closed handle; it cannot revive state or message acceptance and still consumes cumulative handle admission. All three opaque roles need retirement. Recursive derivation uses the existing bounded graphs; a Handle's M is phantom for strong ownership traversal. Shared/Weak storage of a Handle requires actual validated identity, not merely capability facts. Shared/Weak cannot hide an Owner because it is not Share.

ServiceError has canonical data cases 0 InvalidCapacity, 1 Allocation(AllocError), 2 Unavailable, 3 NoActiveHandler and 4 InvalidPublication. These cases confer no authority.

| Core call | Result | Effect |
| --- | --- | --- |
| service_new<S,M>(&Allocator,i64,S) | Result<ServiceSetup<S,M>,ServiceError> | synchronous; consume initial state |
| service_handle<S,M>(&ServiceSetup<S,M>) | ServiceHandle<M> | synchronous explicit mint |
| service_clone<M>(&ServiceHandle<M>) | ServiceHandle<M> | synchronous explicit clone |
| service_owner<S,M>(ServiceSetup<S,M>) | ServiceOwner<S,M> | synchronous unique conversion |
| service_read<S,M>(&ServiceOwner<S,M>) | Result<&S,ServiceError> | synchronous checked read |
| service_replace<S,M>(&mut ServiceOwner<S,M>,S) | Result<Unit,ServiceError> | synchronous whole replacement |
| service_finish<S,M>(&mut ServiceOwner<S,M>) | Unit | synchronous handler boundary |
| service_cancel<M>(&ServiceHandle<M>) | Unit | synchronous idempotent request |
| service_send<M>(&ServiceHandle<M>,M) | SendResult<M> | direct await only |
| service_receive<S,M>(&mut ServiceOwner<S,M>) | Option<M> | direct await only |

Arguments evaluate once left to right. Explicit type arity, storage and capability failures use source type spans. Construction rejects nonpositive capacity as InvalidCapacity and recoverable guest allocation failure as Allocation. An unsuccessful construction still consumes and retires its initial S exactly once, as ordinary moved core arguments; it does not restore the source binding. Control/proof/metadata quota failures are structured execution/preparation failures, not ServiceError. Preflight all fallible resources before publishing an identity or committing ownership.

Send reuses RFC 0025's Sent/Closed(M)/InvalidPublication(M), including whole-result unsent extraction, original payload custody and no restoration of moved locals. Receive/send are operations of the current task, not child Task claims. Bare calls, spawn of either intrinsic, and synchronous callers are INVALID_TASK. Cancel is a request, never a completion acknowledgement or implicit Task cancel/join.

### Owner authority, reads and replacement

The unique Setup/Owner token owns the state and receive authority. Moving Owner transfers that authority once; it does not clone state or create a second owner task. A synchronous helper can borrow its caller's owner within the same segment. Ordinary task loans cannot transfer it to another activation.

A successful receive of Some(M) begins one handler. The next receive first finishes the preceding handler, then polls the next FIFO message. Explicit finish and retirement also end the handler; explicit finish while idle/closed is idempotent. Read/replace while idle returns NoActiveHandler; after closure it returns Unavailable. An active handler may yield or await only after all ordinary loans end. A pending service cancellation does not preempt it; it reaches finish, the next receive, or owner retirement. Structured task cancellation/failure instead follows RFCs 0005/0022 and retires the service through the same queue-before-state boundary.

A read is an ordinary shared loan tied to that exact owner and state generation, with provisional Result binding and checked origin. It never becomes an owning S or mutable loan. Bound reads remain lexical and prevent replacement/finish/receive/move/drop through ordinary borrow rules. Compiler-created call loans end at their accepted boundaries; they do not end an explicitly bound read early.

Replacement consumes its candidate S under ordinary move rules. It requires an active owner and no outstanding read. Before commit, inspect prospective actual strong receiver edges, including state-owned Receivers/ServiceOwners, against current committed message/state ownership. Rejected cyclic publication returns InvalidPublication, leaves the old state unchanged and retires the consumed candidate once. Unavailable/idle replacement likewise retires its candidate. The existing replacement service establishes the new sole owner before retiring old state; cleanup failures retain the existing primary/secondary rules. Replacement retirement order cannot change the service's eventual queue-before-state boundary.

State read/replace is the initial mutation interface. A future exclusive descendant projection requires its own decision and transitive mutation/publication validation; safe source cannot reach it through a producer or cast.

### Cancellation, closure and retirement

Cancellation request latches once, stops accepting new messages, wakes parked operations and returns without waiting. Existing blocked sends recover Closed(M) under RFC 0025. If no handler is active, unlink registrations and retire committed messages FIFO, then state once, before returning from the request. If a handler is active, keep its state/read generations valid until its boundary; then mark it unavailable and perform that same drain. Reads/replacement by that existing active handler remain valid before the boundary; a new handler never starts after a request. Repeated requests on a valid closed handle are no-ops.

Receive after shutdown returns None; state operations return Unavailable. Last-producer closure drains accepted messages normally, then finishes/retire state at the final handler boundary or the receive of None. Dropping Setup/Owner, owner task cancellation/failure and invocation abort unlink first and retire queued messages FIFO before state. Dropping a Handle never owns or directly retires queued/state values; its last-producer bookkeeping may make the owner ready.

A message already returned to a handler is an ordinary owner local, not a queued message. Its lifetime follows ordinary move/scope cleanup; do not duplicate it in the service inventory. A blocked send's uncommitted payload remains that sender's pending owner. Queue-before-state refers to committed queue custody, not unrelated caller locals or pending payloads. Preserve owner source spans through all moves and actual retirement.

Use the existing iterative mixed drain, mandatory unmetered cleanup, panic quarantine, report truncation and primary-failure precedence. Retain the parent header through queued children and state, even if a child drops the last producer. No callback runs while publication/queue invariants are partly committed.

### Independent MIR and trust boundaries

Add typed core operations for the eight synchronous calls. Add PrepareService { destination, endpoint, role, state_type, message_type }, ServiceSend { authority,payload } and ServiceReceive { authority }. state_type is present only for Owner receive. Reuse the bounded prepared namespace with a distinct service role/kind; channel/place authority cannot substitute for it. Independent MIR checks exact role, canonical types, effect, initialized input, single capture consumption, no capture escape/reuse, payload move, read origin/generation and no live ordinary loan at suspension. Capture only private invocation-local authority; end temporary endpoint loans before suspension. Malformed unused operations/metadata cannot skip proof.

Preserve lossless syntax and operation/loan/move/state origins. Old ordinary layout, REPL, C/foreign, schema-2/3/4 codec and native-package paths refuse service metadata before guest/provider effects, including unused constructors. A02 proves semantics with explicit adapter refusal; A03 admits verified private VM execution, and A04 admits generated native execution after parity. Acceptance cannot silently expand any of those profiles.

Extend the existing Hub/managed channel backing for service role/status and one actual state Reservation owned by its unique token. The Hub remains the only owner of committed/pending messages. Read graph edges from actual guest/state/queue/shared storage under checked read-only context; do not add a second owner registry, trust a cached edge inventory or accept caller-supplied capability/acyclic flags. Prospective whole replacement substitutes only that state during cycle inspection; resumed pending sends re-inspect the current graph. Validate Handle publication/replacement into Shared/Weak using explicit service context, including actual live identity even when no strong receive edge exists.

Finite channel/slot/handle/waiter, preparation, frame/cell, guest allocation/heap and retained-result limits include service objects, actual state storage, closed identities and pending operations. Reuse the scheduler and ServiceWork; add no ambient executor, detached state or recursive host-stack drain.

## Execution modes

VM, generated native continuations and exact-build linked objects must match values, events, origins, full usage, cancellation/closure, ordered failures/reports and actual retirement. Native callbacks perform typed storage/scheduling services and do not interpret MIR instructions. Both tier-1 hosts and production embedding remain M3 requirements. A trusted Rust service model or a JIT-only example is insufficient.

## ABI and compatibility

This extends formerly unknown reserved core names and rejected service programs. Previously valid ordinary task/channel programs retain their cleanup and costs. Whole-state replacement, request-only cancellation and handler boundaries are source contracts, not incidental backend behavior. No public ABI, source edition, artifact tag, package field or C representation is assigned. Persisted service metadata needs an explicit independently versioned format decision, validation and migration tests.

## Diagnostics and tooling

Reuse structured type/arity, use-after-move, invalid loan and INVALID_TASK diagnostics. Add SRC0029 (SERVICE_ISOLATION) for a known producer handle used as owner/state authority, with the operation and handle/type origin; the historical semantic model's SEM0031 stays distinct. Suspension reports SRC0028 plus the live read/loan origin. Independent MIR retains typed errors/spans. Runtime wrong brand/role/generation/type remains a structured execution refusal, never a forged ServiceError data value.

Existing generic call/parser recovery and formatter syntax are sufficient; LSP uses the same source/MIR analysis. Debug displays show role/state/cancellation status rather than raw addresses. No separate permissive analysis mode, general mutable state fix-it or new DAP implementation is implied.

## Performance and cost model

Prepare finite mailbox storage plus one state reservation and identity descriptors before publication. Moves/handles do not imply deep clones. Reads retain checked generation/loan metadata; whole replacement constructs a new S and retires the old one. This may cost more than exclusive field mutation; no ergonomic, throughput, code-size or scalability advantage is claimed.

Reuse RFC 0025 capacity, mint/clone and send/receive attempt/commit charges, and RFC 0013 actual managed observation/typed replacement/publication events. Fixed role/generation/handler flag bookkeeping creates no payload-copy event. Explicit finish, cancellation notification, wake/unlink and mandatory retirement are unmetered bookkeeping; guest fuel/poll boundaries remain in place. Both backends must use identical charges before effects. Measure representative state replacement, queued cancellation, receiver-edge inspection and retained memory before stabilization.

## Security and trust boundaries

Untrusted code cannot construct role identities, assert Send/Share, project producer state or transfer ordinary loans. Dynamic validation is required at actual service, shared-publication and foreign boundaries. Limits bound graph work, state/queue storage and metadata before commit. This private trusted native profile is not a sandbox or OS-thread safety claim; future component/plugin admission still needs explicit capability grants and a versioned interface.

## Alternatives

No change leaves the reproduced cancellation/disposal hole and no cancellation handle. A library of async functions and Sender cannot enforce the managed state boundary; a library on these checked primitives can compose services. A built-in actor declaration/static-handler function catalog is the strongest competing design: it can hide loops and select handler shapes but needs new declaration/effect/lowering rules and is unnecessary for the selected source authority outcome. A mutable state loan is more flexible but needs transitive publication hooks across every mutating managed service. Plain Copy-only state/messages is simpler but drops accepted owned/recursive Send behavior. The chosen whole-state interface keeps those families and places publication at one checked boundary.

## Implementation plan

A02 implements constructor facts/core catalog, producer/owner diagnostics, ordinary read/move/suspension proof and independent MIR with old/backend pre-effect refusal. Audit actual runtime prerequisites before A03: execute a new owned-state service fixture with FIFO and cancellation/disposal through existing managed services; expose/re-estimate any independent missing outcome rather than hide it in A02. A04 proves generated-native and separately linked parity. Update RFCs 0005/0024, architecture, the bilingual board and roadmap alongside evidence. Gates, both-host embedding, measurements and the final capability guide remain separate acceptance conditions.

## Test plan

Compile-pass Copy/owned/nested/recursive state/messages, helper reads, legal move to owner and producer clones. Compile-fail handle state projection/read/replace, duplicate owner consumption, non-Send nested loans/arenas/allocator/Task, wrong arity and bound reads across replacement/finish/receive/await. Forge MIR roles/types/capture reuse/loan origins and check exact/one-below proof budgets, unused metadata and all old artifact/profile refusals.

Execute 20+22=42, full/empty FIFO/backpressure, pending send recovery, cancel idle/active/parked and repeated cancel, handler yields before boundary, state replacement before cancel, closure/return/task failure, received versus queued custody and queue-before-state traces. Include Receiver/ServiceOwner state and messages, prospective state cycles, resumed reinspection and shared producer publication. Compare all control/provider/interruption positions, real quotas, panic/quarantine/truncated reports, deep iterative cleanup and inventory zero across VM/native/linked objects. Compile/lint all fuzz targets; claim a fuzz campaign only when one is actually run. Retain both-host/ABI migration tests and measurements before stabilization.

## Open questions

No blocking semantic question remains for A02. Unverified implementation assumptions: existing Hub inspection can include prospective state without duplicating custody; read generations and provisional loan binding can reuse existing managed views; the mixed drain can order queue then state without extra per-retirement allocation. Validate these with the first runtime fixture and expose missing prerequisites. Representative cost, both-host behavior and later persisted/foreign representation remain unverified; do not infer them from acceptance.

Decision: **accepted under RFC 0001 experimental delegation, 2026-10-22 JST**. Explicit roles, ordinary loans/moves, whole-state publication and an observable handler boundary preserve RFC 0005 isolation/cancellation without a new source language, unrestricted mutable projection or interpreter fallback. The composition experiment demonstrates why the ownership boundary is required. Assumptions above are recorded rather than treated as implementation evidence.

## Sources

- RFCs [0005](0005-structured-tasks-and-isolated-services.md), [0013](0013-service-byte-work-budget.md), [0022](0022-source-structured-tasks.md), [0024](0024-send-share-and-service-authority.md), [0025](0025-bounded-channels-and-backpressure.md): project semantic contracts; last updated 0005/0013/0024/0025 **2026-10-22**, 0022 **2026-10-21**; verified **2026-10-22 JST**. No external factual/performance claim is based on them.
- [Composition experiment](../experiments/2026-10-22-service-composition.lang), created/executed **2026-10-22 JST** against substrate f882074/14f1af7. A temporary integration probe used the existing tests/support/channel_fixtures.rs provider: result 42, buffer retirement [1,42,7], zero live inventory; one experiment passed. The probe is not a service conformance suite.
- crates/hir/src/core.rs, crates/hir/src/types.rs, crates/mir/src/unit/ownership/, crates/runtime/src/channel/managed.rs, existing shared/arena views and task retirement: no embedded code update date; inspected **2026-10-22 JST**. C04 validation evidence is in the [delivery board](../milestone-3.md).
