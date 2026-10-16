<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0004-owned-values-and-cyclic-graphs.md -->

# RFC 0004: Owned values, explicit sharing, and cyclic graph storage

[日本語版](../ja/rfcs/0004-owned-values-and-cyclic-graphs.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-16
- Last updated: 2026-10-17
- Tracking issue:
- Supersedes:
- Superseded by:

## Summary

Define the initial source-language ownership and cyclic-graph policy:

- Plain `T` is singly owned, moves instead of implicitly copying, and is destroyed deterministically.
- Non-escaping shared and exclusive borrows are inferred without user-written lifetime parameters.
- `shared T` is an explicit atomically reference-counted owner. Cloning it is explicit.
- `weak T` never retains its target, and upgrading it is explicitly fallible.
- Publishing a strong `shared` edge that would close a cycle fails before mutation with stable diagnostic identity `ownership.strong-cycle` (`SEM0018` in the prototype).
- A lexical `arena` owns graph nodes that may form arbitrary cycles. Arena references cannot escape or cross arena regions, and arena exit destroys live nodes exactly once in reverse allocation order.
- The base runtime does not require a tracing garbage collector. A future opt-in traced heap would require a separate RFC and type.

The exact constructor and parameter-modifier spellings remain provisional, but the ownership categories, cycle policy, failure atomicity, and execution-mode equivalence in this RFC are accepted semantic commitments.

## Motivation

The language cannot simultaneously maximize unrestricted shared object graphs, deterministic destruction, invisible runtime cost, simple syntax, and the absence of tracing collection. Leaving the tradeoff unspecified would allow frontend, MIR, VM, native runtime, and component adapters to acquire incompatible ownership behavior.

Reference counting alone is not sufficient. Rust documents that `Rc<T>` cycles can leak and uses non-owning `Weak<T>` to break cycles. Swift ARC likewise requires weak or unowned relationships to resolve strong-reference cycles. Kotlin/Native demonstrates the strongest competing design: a shared heap with tracing collection from roots. These are source facts, not project decisions.

The repository's executable graph model provides local evidence for two policies. `Reject` refuses a cycle-closing strong edge and permits weak back-edges. `CollectUnreachable` permits strong cycles and synchronously traces from remaining roots after every root drop. The latter proves reclamation behavior but does not establish acceptable production latency, memory, finalization, or concurrency behavior. Arena-scoped cycles also execute with deterministic reverse-order teardown and stale-handle rejection.

A library-only convention cannot enforce move/use rules, prevent arena references from escaping, ensure every shared-edge mutation performs the same preflight, or preserve stable diagnostics through typed MIR and every backend.

## Goals

- Define owned, borrowed, shared, weak, and arena-scoped lifetime categories.
- Preserve deterministic destruction without a mandatory tracing collector.
- Make reference-counting and cycle behavior visible in source types.
- Reject cycle-forming strong shared edges before observable mutation.
- Support intentional cyclic data inside explicit, bounded arena lifetimes.
- Specify equivalent outcomes for bytecode VM, JIT, AOT, and embedding paths.
- Preserve source spans and stable logical diagnostic identities.
- Define compatibility and trust-boundary rules for handles and foreign code.

## Non-goals

- Final parameter-modifier, constructor, or graph-container API spelling.
- Borrowing across suspension, cancellation cleanup, actor/service isolation, atomics, scheduling, or lock semantics; those belong to the concurrency RFC.
- Self-referential movable values or pinning.
- A general tracing collector, cycle collector, finalizer, or object-resurrection model.
- Choosing a production allocator or promising allocation, retain/release, or cycle-check performance.
- Changing RFC 0003's component resource representation or exposing source heap handles in a stable ABI.
- Defining panic payloads, destructor-failure transport, or a complete error/effect syntax.

## Guide-level design

The following spelling is illustrative; this RFC makes the type categories and behavior normative, while a syntax-focused revision may refine constructors and mutation APIs.

```text
record Node {
    name: String,
    parent: weak Node?,
    children: List<shared Node>,
}

let root: shared Node = shared(Node(name: "root"));
let child: shared Node = shared(Node(name: "child"));

root.children.push(child.clone()); // explicit strong ownership
child.parent = weak(root);         // non-retaining back-edge

if let some(parent) = child.parent.upgrade() {
    print(parent.name);
}
```

A type may deliberately store a strong peer edge, but the edge that closes a cycle fails before either field, reference count, or graph metadata changes:

```text
record Peer { peer: shared Peer? }

let left: shared Peer = shared(Peer());
let right: shared Peer = shared(Peer());
left.peer = right.clone();
right.peer = left.clone(); // rejected: `left` already reaches `right`
```

```text
error[ownership.strong-cycle]: strong shared edge would create a cycle
  --> peers.lang:6:1
   |
 6 | right.peer = left.clone()
   | ^^^^^^^^^^^^^^^^^^^^^^^^^ cycle-closing write
   |
   = related: existing strong path reaches `right`
   = help: use `weak(left)` or place the graph in an `arena`
```

Graphs with intentionally cyclic topology use a lexical arena:

```text
arena graph {
    let left = graph.new(Node(name: "left"));
    let right = graph.new(Node(name: "right"));
    left.next = right;
    right.next = left;
} // `right`, then `left`, are destroyed exactly once
```

An arena reference cannot be returned, stored in a global or `shared` value, captured by a longer-lived closure, sent to another arena, or dereferenced after its arena exits. The compiler reports a source error when it can prove such an escape. Validated MIR, bytecode, and dynamic embedding boundaries retain region/generation checks as defense in depth.

## Reference semantics

### Owned values and borrows

1. A plain non-copy `T` has exactly one owner. Assignment, argument passing to an owning parameter, return, and capture move the value unless an explicit clone operation is selected.
2. Reading or mutating a moved value is rejected. A move of a partially initialized aggregate is rejected until later RFCs define partial-move semantics.
3. Destruction occurs when the owner leaves its successfully initialized scope. Locals and aggregate fields are destroyed in reverse initialization order.
4. A shared borrow permits reads and aliases other shared borrows. An exclusive borrow permits mutation and excludes every other live borrow. Safe code cannot observe a borrow after its owner is moved or destroyed.
5. Ordinary non-escaping borrows use lexical inference. Storing, returning, or otherwise escaping a borrow requires a later explicit owning abstraction; this RFC introduces no user-written lifetime parameters.
6. The async/concurrency RFC decides which borrows, if any, may cross suspension or task boundaries.

### Shared and weak ownership

1. `shared T` owns one strong reference to a heap allocation. Creating another strong owner is explicit and increments an atomic strong count. Releasing the last strong owner marks the allocation unavailable before running user destruction and prevents resurrection.
2. `weak T` is created only from a live `shared T`. It does not contribute to liveness. `upgrade` returns an optional/result-like success containing a new strong owner only if the allocation is still live.
3. A strong edge is a stored `shared` owner reachable from another shared allocation. Before publishing a new or replacement strong edge from `from` to `to`, the runtime validates both endpoints and checks whether a strong path already reaches `from` from `to`.
4. If publication would close a cycle, it fails with `ownership.strong-cycle` before changing the destination field, strong counts, edge metadata, or any other ownership state. The diagnostic carries the write span and, when available, a related span from the existing path.
5. A weak edge is never included in reachability for cycle rejection and never retains its target. Upgrading an expired weak edge reports `ownership.expired-weak` (`SEM0021` in the prototype) without reviving the allocation.
6. Shared-edge replacement commits as one observable operation: validation and any required reservation happen first; then the new edge/count and removal of the old edge/count become visible without a user callback in between.
7. The concurrency RFC may define how shared mutation authority is acquired, but every implementation must linearize successful publication after cycle preflight and produce the same success/failure outcome for an equivalent ordered execution.

### Arena-scoped graphs

1. `arena name { ... }` introduces a unique lexical ownership region and its sole arena owner. Allocation returns a non-owning reference branded with that hidden region identity.
2. Arena nodes may contain arbitrary strong cycles among nodes in the same live arena. Arena edges do not maintain per-node reference counts.
3. An edge between distinct arenas, or between arena storage and the shared heap, is rejected as `ownership.cross-region-edge` (`SEM0017` in the prototype) unless a later RFC defines an explicit owned copy or component-resource adapter.
4. Arena references cannot outlive or escape their arena region. Region identity is inferred and is not written as a lifetime parameter in ordinary source.
5. Arena exit first prevents new access, then destroys every successfully initialized live node exactly once in reverse allocation order, regardless of graph topology. Cycles do not affect the order.
6. A stale or forged arena reference reaching a validated dynamic boundary is rejected as `ownership.unavailable-arena-reference` (`SEM0012` in the prototype); it never aliases a later allocation. Generation-tagged handles are an implementation requirement wherever numeric slots can be reused.
7. User destructors cannot resurrect arena nodes or publish arena references outside the exiting region. Panic/destructor failure ordering remains a separate error-semantics decision, but every backend must continue compiler-mandated ownership bookkeeping consistently.

### Constant execution, unsafe, and foreign code

Constant evaluation may support owned values and arenas only when all allocation and destruction are contained in the evaluator's resource limits. `shared` and `weak` operations are rejected in constant execution until deterministic allocation and atomic behavior are specified.

Raw pointers and foreign references are not `shared`, `weak`, or arena references. `unsafe` code that constructs safe ownership values must establish the same uniqueness, liveness, cycle-registration, and region invariants. Foreign code cannot mutate a tracked strong edge behind the runtime's back and then expose the object as safe `shared T`.

## Execution modes

Typed MIR is the common semantic contract. The implementation plan adds backend-neutral operations for moves, borrow begin/end, shared clone/release, weak downgrade/upgrade, checked strong-edge publication, and arena enter/allocate/exit. MIR and bytecode validation reject malformed region flow, use-after-move, unmatched borrow scopes, and ownership operations with inconsistent types before execution or native lowering.

The VM implements these operations directly or through the shared runtime. JIT and AOT may inline proven-safe retain/release or cycle checks, but optimization cannot change observable destruction order, weak-upgrade result, cycle rejection, diagnostic identity, or failure atomicity. Native unwinding is not the semantic mechanism.

Embedding APIs expose opaque generation-checked values rather than host addresses. RFC 0003 `own<T>`/`borrow<T>` component resources are the accepted, not publicly stabilized, cross-component ownership baseline; source `shared`, `weak`, and arena references are neither serialized nor passed directly across component boundaries. WebAssembly implementations use adapters with the same logical outcomes.

## ABI and compatibility

This RFC introduces source-level semantic categories, not a stable native layout. Reference-count headers, edge metadata, arena allocation strategy, and generation widths are implementation details. Native ABI, bytecode, typed MIR, debug metadata, and serialized component contracts remain versioned independently.

Changing a public field or parameter between owned, borrowed, `shared`, `weak`, or arena-scoped form is source- and behavior-affecting. Package metadata must record the normalized ownership category once such types enter public interfaces. Edition migration may insert explicit clone/downgrade operations only when behavior is unambiguous; it must not silently replace a strong edge with a weak edge or move data into an arena.

## Diagnostics and tooling

- The parser and formatter preserve `shared`, `weak`, and `arena` forms deterministically.
- Move and borrow diagnostics identify the move/borrow origin plus the invalid later use.
- Cycle, cross-region, expired-weak, and stale-arena diagnostics keep stable logical identities and source spans through HIR, MIR, bytecode, VM, and native debug metadata.
- LSP references and hover show the ownership category and whether a clone, weak upgrade, or arena boundary is involved.
- The linter warns about unnecessary shared ownership, immediately expired weak values, and arena values whose topology is acyclic and can use owned storage. It must not claim a runtime cycle when it cannot prove one.
- DAP views distinguish an owned value, strong count, weak handle, arena identity, and unavailable handle without exposing raw addresses by default.
- Documentation generation renders ownership categories in public signatures and links cycle-forming diagnostics to weak/arena guidance.

## Performance and cost model

Owned moves and inferred borrows require no reference count. `shared` clone/release performs atomic reference-count traffic unless whole-program analysis safely removes it. `weak` storage requires liveness metadata and an upgrade check.

The strict shared-cycle policy has a worst-case graph traversal proportional to the reachable strong vertices and edges for a publication that cannot be proven acyclic. Implementations may cache topology, use ownership-tree summaries, specialize types with no strong fields, or statically prove an insertion safe, but may not omit the check when a cycle remains possible. These optimizations and their invalidation costs are not language guarantees.

Arena allocation may be bump-like, but only bulk lifetime and reverse allocation-order destruction are guaranteed. Allocation complexity, reuse, fragmentation, and deallocation batching are implementation details.

The current prototype proves outcomes, not production cost. Before stabilization, benchmarks must separately measure owned moves, shared clone/release, weak upgrade, acyclic checked-edge insertion at multiple graph sizes, rejected cycle insertion, arena allocation/teardown, peak memory, and allocator/metadata overhead across VM and native execution. No latency or memory budget is established by this RFC.

## Security and trust boundaries

Source, MIR, bytecode, component input, numeric handles, and foreign references are untrusted at their respective boundaries. Validators check type, region, generation, and operation ordering before dereference. Cycle preflight and edge commit do not invoke user callbacks. Integer overflow in counts, generation exhaustion, allocation failure, malformed region flow, and resource limits fail closed without aliasing a stale value.

Untrusted plugins do not receive native addresses or ambient allocator access. Capability policy controls allocation and graph-size limits for script/component execution. Native plugins are trusted and require explicit opt-in; they are not made safe by reference counting or arenas.

## Alternatives

### No change

Keeping ownership as an architecture hypothesis would let parser, MIR, runtime, and component work invent incompatible move, destruction, and cycle behavior. This does not meet the milestone's design-baseline requirement.

### Library or tooling solution

Smart-pointer and arena libraries can expose useful APIs, but they cannot make use-after-move, borrow exclusivity, arena escape, backend-equivalent cleanup, or complete edge-registration a safe-language invariant. Lints cannot reliably detect dynamic cycles.

### Tracing shared heap

The strongest competing design uses a traced shared heap, as Kotlin/Native does. It naturally reclaims unreachable strong cycles and avoids a path check on every edge publication, but introduces collection triggers, tracing work, pause/concurrency policy, weak/finalizer ordering, a larger mandatory runtime, and nondeterministic reclamation timing. The `CollectUnreachable` prototype demonstrates semantics but not an acceptable production collector. This RFC therefore rejects tracing collection as the base `shared` representation; an explicit future traced type remains possible.

### Arena-only graphs

Supporting only owned values and arenas is simpler and makes cyclic lifetime explicit, but it cannot express independently shared DAGs whose nodes should be released before a surrounding arena. It also forces bulk lifetime onto services and caches that need explicit shared ownership. This remains a useful restricted/freestanding subset, not the complete design.

### Permit reference-counted cycles and warn

Rust `Rc` and Swift ARC show this familiar design. It avoids dynamic cycle-preflight cost but makes safe, reachable-looking operations capable of retaining an unreachable graph indefinitely. A warning cannot be complete for dynamic mutation. This RFC chooses a stronger no-silent-cycle guarantee and accepts its visible runtime check.

## Implementation plan

1. Add source/HIR ownership categories and compile-fail tests for moves, borrow conflicts, escaping arena references, and cross-region edges. Keep exact convenience syntax provisional behind the edition gate.
2. Add typed MIR ownership and region operations plus validation. Preserve source spans and stable diagnostic identities.
3. Generalize the existing `crates/semantics` graph model into a runtime-facing checked-edge and lexical-arena prototype with generation-tagged handles and resource limits.
4. Lower the same fixtures to bytecode VM and native execution; compare event traces, destruction order, weak upgrades, and every failure outcome.
5. Integrate source ownership with RFC 0003 adapters without exposing source heap layout or raw handles.
6. Add fuzzing for malformed MIR/bytecode operations and randomized graph sequences, including generation/count exhaustion and failed-publication atomicity.
7. Record the separate runtime and memory benchmarks listed above on macOS AArch64 and Windows x86-64 before stabilization.
8. Update the guide, formatter, linter, LSP, DAP, and migration tooling to implement the accepted policy; future semantic changes require a new RFC disposition.

## Test plan

- Compile-pass: owned move into return, inferred read borrow, exclusive mutation, explicit shared clone, weak parent, same-arena cycle, deterministic arena exit.
- Compile-fail: use after move, overlapping exclusive/shared borrows, escaping arena reference, cross-arena edge, arena-to-shared storage, closure outliving an arena.
- Runtime: rejected strong cycle leaves fields/counts unchanged; weak edge breaks a back-edge; expired upgrade never revives; last strong release destroys once; arena cycles destroy once in reverse allocation order; stale generation never aliases reuse.
- Differential: identical VM/JIT/AOT values, event traces, destruction order, diagnostics, and source spans for success and failure fixtures.
- Boundary: component adapters accept explicit `own`/`borrow` conversions and reject raw shared/weak/arena handles; host callbacks cannot bypass edge preflight.
- Property/fuzz: randomized operations preserve nonnegative counts, no live strong cycle, region separation, no stale alias, at-most-once destruction, and failure atomicity.
- Performance: graph-size series and allocation/memory profiles from the performance section on both tier-1 targets.

## Open questions

There are no blocking semantic questions for the accepted policy. The repository owner accepted it on 2026-10-17.

Optional follow-ups are the final constructor/parameter/API spellings, static acyclicity annotations that can remove checks, allocator selection, weak-upgrade ordering under the future concurrency model, and whether a separately named traced heap should be proposed later.

Decision: **accepted on 2026-10-17** by the repository owner.

## Sources

- [The Rust Programming Language, “Reference Cycles Can Leak Memory”](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html), source last changed 2025-10-03; verified 2026-10-16. It documents that `Rc<T>` cycles leak and that `Weak<T>` does not retain its target.
- [The Swift Programming Language, “Automatic Reference Counting”](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/LanguageGuide/AutomaticReferenceCounting.md), source last changed 2026-01-09; verified 2026-10-16. It documents strong-reference cycles and weak/unowned relationships.
- [Kotlin/Native memory management](https://kotlinlang.org/docs/native-memory-manager.html), living documentation with no published update date; verified 2026-10-16. It documents a shared heap, root-based tracing, concurrent mark-and-sweep collection, and weak-reference processing.
- `crates/semantics/src/graph.rs`, strict/tracing/arena graph prototype, repository commit `d4925b8c1d97a49916d6c3f7cd2274f9672e1c50`, committed 2026-10-16.
- [RFC 0003](0003-versioned-component-contracts.md), source/component ownership boundary, last updated 2026-10-16.
