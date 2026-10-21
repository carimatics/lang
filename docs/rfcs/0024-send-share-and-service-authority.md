<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0024-send-share-and-service-authority.md -->

# RFC 0024: Recursive Send/Share and service authority

[日本語版](../ja/rfcs/0024-send-share-and-service-authority.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-21
- Last updated: 2026-10-21
- Tracking issue: M3-A01, M3-A02, M3-A05, M3-A06
- Supersedes: none
- Superseded by: none

## Summary

Derive two internal capabilities from verified source types: `Send` permits moving an owned value between logical tasks in one invocation; `Share` permits checked shared observation without granting mutable alias authority. Async arguments/results and channel messages require Send. No user assertion, trait syntax, closure capture syntax, implicit clone, detached task or cross-thread host ABI is added.

Service state belongs to its owner task. A service handle conveys only the message/cancellation operations accepted by its interface; it never exposes a mutable state projection. The later channel/service decisions select concrete handle types and operations. Acceptance here does not implement those operations or complete an M3 gate.

## Motivation

RFC 0022 deliberately admits only finite Copy async transfers. Owned locals already execute across suspension, but `async fn worker(value: core::Bytes) -> core::Bytes` still reports SRC0026. Channels and isolated owners need an explicit recursive transfer rule before accepting owned messages. A manually supplied Sendable bit in the old service experiment is not source or independent MIR evidence.

## Goals

- Independent, budgeted source and MIR derivation, including recursive nominal/container graphs.
- Move-only owned async transfer with existing cleanup, result escrow and shared controls.
- Owner state access through ordinary ownership/loans; enqueue authority cannot mutate another task's state.

## Non-goals

Threads, arbitrary host handles, public Rust Send/Sync implementations, user-defined traits, borrowed async arguments, Task claim transfer, channel/backpressure spelling and serialized formats.

## Guide-level design

An async worker may receive/return Bytes, Text, Vec, Map or records/variants composed of sendable values. Passing a non-Copy argument consumes it under ordinary source move analysis. An awaited successful result moves once from child escrow. A cancelled child returns no payload; unclaimed successful results remain scope-owned until retirement. A source service is an async owner with private moved state; producers communicate through a separately checked message handle. Passing an ordinary shared/exclusive loan to another task remains forbidden, even if its target is Share.

Contract example (execution pending A06/A05); after implementation it returns 0:

```text
async fn worker(value: core::Bytes) -> core::Bytes { yield; return value; }
async fn main() -> i64 {
    let mut input: core::Bytes = core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator()));
    let mut output: core::Bytes = core::result_unwrap::<core::Bytes, core::TaskError>(await worker(input));
    return core::bytes_len(output);
}
```

## Reference semantics

### Derivation

| Type | Send | Share |
| --- | --- | --- |
| Unit, bool, i64, finite built-in error | yes | yes |
| Text, Bytes | yes | yes |
| Option/Result, record, variant | every field/payload is Send | every field/payload is Share |
| Vec<T>, Map<K,V> | all type arguments are Send | all type arguments are Share |
| shared T, weak T | T is Send and Share | T is Send and Share |
| ordinary loan, allocator authority, arena owner/reference, Task claim | no | no |
| future foreign/resource/service handle | only an accepted explicit capability contract | only an accepted explicit capability contract |

Share does not make an owner Copy or supply a clone. Bytes/container mutation still needs an exclusive ordinary loan; shared payload access uses the established read/whole-replacement boundary. An outstanding read on any alias prevents replacement. Every backend serializes these managed operations within the invocation under RFCs 0004/0007/0021. These are logical-task capabilities, not a promise that current allocator/providers or private Rust runtime values may cross OS threads. A parallel adapter must establish synchronization before admitting the same profile.

Derive the greatest fixed point on the already validated type graph: start candidate capabilities true, propagate each forbidden leaf or missing dependency to its reverse dependents, and stop when stable. Reuse existing transitive type facts when they are equivalent to this rule; no second graph framework is required. An otherwise well-formed recursive Vec/nominal graph is not rejected merely for recursion. A forbidden field on any reachable cycle makes the containing capability false. Existing layout/storage validation still rejects invalid by-value recursion and invalid stored loans. Canonical aliases have identical capabilities. The source and MIR implementations independently enforce the same rule, with bounded work/storage and structured refusal on exhaustion. No caller-supplied capability bit is trusted.

### Transfer, admission and ownership

Async parameters/results require Send at their declared type spans, including unused functions. All current no-loan-at-suspension, scoped Task claim and region restrictions remain. Arguments evaluate once left to right under ordinary move semantics. Runtime spawn preflights task/frame/cell/allocation resources before publishing a child or transferring ownership; refusal leaves prepared values under caller cleanup. Successful publication transfers each non-Copy root once; it never bit-copies an owning ticket. Synchronous loans remain limited to the current execution segment.

An owned result remains escrowed through child cleanup and mandatory joins; successful await moves it into Ok once. Cancellation or terminal/cleanup failure retires the appropriate staged/result owners without inventing a T. Scope-owned unclaimed results retain their cells/heap until actual retirement. Mandatory cleanup remains non-preemptible and unmetered under RFC 0013. Static capability derivation and guest-frame marshalling add no service-work event.

### Service authority

Only the owning async activation can borrow or replace its ordinary local state and exclusively owned descendants. Send transfers ownership; it cannot grant a producer a loan into that state. A later service interface must expose only typed message enqueue/cancellation authority to other tasks, keep mutable state out of producer projections, and validate dynamic identity/generation at trust boundaries. Explicit shared subgraphs retain their separate checked shared semantics; they are not private owner state and do not grant access to its local state root. The existing model's alternative non-owner exclusive mutation policy remains unaccepted. Channel/message commit, FIFO backpressure and concrete source operations require the channel decision before implementation.

## Execution modes

Source/HIR and independent MIR must agree. VM, generated native and linked objects must move/retire identical owners and report identical origins, usage and cancellation outcomes. The unavailable owned-transfer adapter continues an explicit pre-effect refusal until its implementation delivery. Foreign/C/component, REPL and persisted task/package profiles retain their current restrictions.

## ABI and compatibility

This extends previously rejected owned async signatures. Previously valid Copy programs retain their values, ownership and event costs. Add internal derived facts/proofs only, with no source trait annotation or public native/C representation. A serialized task format remains a separate versioned decision; do not assign tags or silently teach old readers these capabilities. Future handles default to unavailable until their accepted contract supplies capability requirements and validation.

## Diagnostics and tooling

Report a non-sendable async type at its source signature/argument with the first forbidden dependency where available; preserve ordinary use-after-move and borrow-at-suspension diagnostics. Independent MIR rejection retains its typed error/span. Bounded derivation exhaustion is a resource failure, never a permissive fallback. LSP/formatter keep existing async syntax and use the same checked source facts; add no alternate analysis mode that accepts unproved captures.

## Performance and cost model

Use budgeted graph worklists/SCC-equivalent propagation, not recursive host stack traversal. Reuse verified ownership layouts, reservations and transfer/retirement machinery. No implicit deep copy, retain or graph scan is implied by Send. Measure actual transfer, result and shared-operation overhead before stabilization; no speedup or thread scalability is claimed.

## Security and trust boundaries

Safe source cannot assert capabilities or fabricate task/service identity. Type proof does not replace runtime shared/publication checks, allocator identity, receiver authority or foreign resource validation. Untrusted serialized/foreign input receives no capability merely from a type name. This contract does not turn trusted native code into a sandbox.

## Alternatives

A library/tool alone cannot authorize cross-task ownership transfer: source/MIR proofs are required. A library may compose services from accepted async/channel operations, while consuming those proofs and authority restrictions.


Keep Copy-only transfers: simple but cannot satisfy owned messages. Permit every non-loan owner: misses arena/allocator/Task/foreign authority and shared operation safety. User-marked Send: trusts assertions without a validated unsafe boundary. Introduce unrestricted thread-safe shared mutation: contradicts the accepted owner and loan contracts and needs a separate memory-model design.

## Implementation plan

Accept the rule, then implement independent source/MIR capability derivation and compile-pass/fail ownership checks in A06, and owned async argument/result execution in A05. A02 completes source actor authority evidence once concrete channel handles exist. Audit and expose the independently observable owned async argument/result execution prerequisite before implementing it; changing signature acceptance alone does not implement transfer/escrow. Channel/source service deliveries then consume these proofs. Revise RFCs 0005/0022/0023 and their mirrors when accepting this follow-up, preserving their dated checkpoints. Keep implementation and gate completion distinct.

## Test plan

Compile-pass recursive records/variants/Vec/Map/Text/Bytes/shared/weak and Copy cases. Compile-fail direct/nested loans, allocator/arena/Task authority, non-sendable future resources and payload reuse. Exercise declaration order, aliases, recursive forbidden dependencies, unused metadata, malformed MIR and exact derivation budgets. Later execute owned parameters/results, failed admission, unclaimed result disposal, cancellation before/after transfer, shared read/replace conflicts, nested joins, panic and cleanup/report exhaustion on VM/native/linked objects. Check every actual fuel/work/provider refusal boundary and retained-result cell limits. Add real producer/owner isolation fixtures with channel/service execution; the old host model is not sufficient.

## Open questions

Unverified assumptions: the existing managed transfer/escrow machinery can support owned async publication without a new owner registry; bounded recursive derivation costs remain acceptable; future channel handles can carry only interface authority. The implementation audit must expose any missing observable prerequisite and preserve all M3 acceptance conditions. No cross-thread or foreign-handle safety is inferred.

Decision: **accepted under RFC 0001 experimental delegation, 2026-10-21 JST**. Recursive verified type facts, ordinary moves/loans and the existing checked shared-publication/serialized-invocation boundary define owned task transfer without introducing borrowed transfers or assuming thread safety. Keeping state projections out of service handles preserves producer/owner separation. The assumptions above remain unverified until implementation and both-host validation; acceptance establishes neither implementation, stabilization nor gate completion.

## Sources

- [RFC 0004](0004-owned-values-and-cyclic-graphs.md), last updated 2026-10-17; RFCs [0005](0005-structured-tasks-and-isolated-services.md)/[0007](0007-source-ownership-and-core-values.md)/[0021](0021-bounded-structured-task-driver.md)/[0022](0022-source-structured-tasks.md)/[0023](0023-task-mir-boundaries.md), last updated 2026-10-21; verified 2026-10-21 JST. These are project contracts, not external performance evidence.
- `crates/hir/src/tasks.rs`, `crates/hir/src/types.rs`, `crates/mir/src/unit/ownership/tasks.rs`, `crates/runtime/src/task/runner/`, and the experimental `crates/semantics/src/service.rs`, inspected 2026-10-21 JST; code has no embedded update date. A minimal owned Bytes worker argument/result reproduces SRC0026 before this decision's implementation.
