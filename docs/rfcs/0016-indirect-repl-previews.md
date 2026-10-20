<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0016-indirect-repl-previews.md -->

# RFC 0016: Indirect REPL result previews

[日本語版](../ja/rfcs/0016-indirect-repl-previews.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-V17
- Supersedes: none; extends RFC 0010
- Superseded by: none

## Current implementation status

Reconciled on 2026-10-20 JST against [M2 capabilities and evidence](../milestone-2-capabilities.md). This RFC's scope is implemented, not stabilized. Read the original plan, decision-time statements and dated checkpoints below as rollout history. The guide records current native/SDK/platform work that those checkpoints treated as separate deliveries. Its review correction explicitly resolves the discovered RFC 0012 logical-debugger evidence gap; this reconciliation changes no normative requirement or non-goal of this RFC.

## Summary

Extend RFC 0010's VM/REPL result formatter to Vec, Map, shared owners and weak tokens. For example, a function returning a Vec containing 9 prints `core::Vec [9]` instead of failing before execution with `Unsupported`. Source arena escape checks remain mandatory.

Acceptance: **accepted under experimental delegation**, before implementation. Review finds no conflict with RFCs 0004/0007/0008/0010: this is read-only tooling over the already protected result, with explicit finite host scratch and no new guest operations. Implementation and boundary tests remain unverified at acceptance; acceptance does not complete V17.

Implementation status (2026-10-20 JST): available through the intended VM/REPL interfaces, with output/order/custody/liveness/output and host-budget boundaries/cleanup verified. Latest per-suite workspace results total **1,529 tests / 89 suites** passing (after replacing the legacy Vec-refusal fixture with arena refusal and rerunning the non-root workspace). fmt/Clippy/rustdoc pass. The accepted design is unchanged.

## Motivation

The existing REPL can execute collection services, but refuses their result types before guest entry because RFC 0010 deliberately deferred their display contract. A bounded collection preview makes those executions useful without asking users to add scalar extraction helpers. Formatting must not expose private Map storage order or silently upgrade a weak reference.

## Goals

- Render logical collection contents and shared payloads without transferring, retaining or releasing owners.
- Give Map previews a deterministic presentation independent of physical/insertion order.
- Preserve result escrow, callback-free inspection, explicit allocator authority and cleanup-before-publication.

## Non-goals

- A guest iteration API, user formatting callbacks, graph identity/alias displays, or persistence of guest values.
- Reading payloads through weak tokens, extending arena lifetimes, or native REPL execution.
- ABI, bytecode or serialized-format changes.

## Guide-level design

| Result | Preview |
| --- | --- |
| Vec with two elements | `core::Vec [9, -1]` |
| Empty Vec | `core::Vec []` |
| Map containing keys 2 then 1 | `core::Map { 1: false, 2: true }` |
| Empty Map | `core::Map {}` |
| Shared owner of 9 | `shared(9)` |
| Weak token with a live strong owner | `weak::Live` |
| Weak token after the last strong owner retires | `weak::Expired` |

Nested payloads use the existing scalar, Text, record, enum, Option and Result formats. Map keys are sorted numerically for i64 and lexicographically by original UTF-8 bytes for Text. Formatting a Text key uses RFC 0010's escaping. This is a tooling presentation rule; source Map operations acquire no iteration or hash-order contract.

Shared payloads are printed by value. Repeated aliases may therefore print the same payload repeatedly. Weak previews report only liveness at the protected-result observation point, after ordinary cleanup. They never inspect the target payload. No reference counts or addresses are printed.

A source expression still cannot return an arena-branded reference outside its lexical region. The preview adapter also refuses a result type containing a directly traversed arena reference before guest effects: after ordinary cleanup it has no live region authority. A weak token's unvisited target type does not itself require payload preview support. No placeholder suggests that a dead arena reference can be used.

## Reference semantics

Syntax, resolution, inference, moves, loans and evaluation order are unchanged. Admission recursively visits nominal/core payload types, Vec elements, Map keys/values and shared payloads. Weak target payloads are not traversed. Arena references and other unsupported authority types retain the pre-entry refusal.

Inspection borrows the allocator and protected storage immutably for the entire render. Vec observations check the live token, initialization and index. Map observations validate every key/value and use a bounded host index permutation; guest storage is never reordered. Shared observations validate registry owner, ticket generation, kind, payload type and customer custody. Descending into a strong payload carries that validated node's private custody to its nested tokens. It does not allocate a guest read ticket. Weak liveness validates the same token/custody boundary and checks whether a strong owner remains, without upgrading.

No guest code, provider callback, semantic clone, retain/release, loan acquisition, safepoint, instruction fuel or service-work charge is introduced by formatting. Rust immutable borrows exclude guest mutation while inspecting. Strong cycles remain forbidden by the existing service; weak edges terminate traversal.

RFC 0010's failure contract is unchanged: formatting is primary on render failure, cleanup is still attempted, cleanup failure poisons the context, and no partial preview is published. A corrupt/stale/foreign token is an inspection failure, never `weak::Expired`. Scratch/output exhaustion is a bounded preview-limit failure.

## Execution modes

The bytecode VM and REPL implement this host formatter. Native AOT semantics and C exports are unchanged; they do not gain a native formatter. Optional JIT, component execution, constant evaluation and embedded callbacks gain no new entry points or capabilities.

## ABI and compatibility

This extends the REPL text output of previously unsupported result types. Existing RFC 0010 output remains byte-for-byte compatible. No source syntax, native/component ABI, bytecode, serialized IR, package metadata, edition or migration tooling changes. Consumers should treat previews as diagnostics rather than a serialization format.

## Diagnostics and tooling

Use the current structured Unsupported, preparation-limit and render-limit/error paths, preserving source spans and cleanup diagnostics. Parser recovery, formatter, LSP and DAP behavior are unchanged. Arena escape diagnostics remain frontend ownership errors. REPL documentation must show the new formats and the arena restriction.

## Performance and cost model

Reserve output, iterative traversal cells and Map ordering scratch before guest entry, in the same cumulative compiler/host budget as RFC 0010. The existing output cap defaults to 65,536 bytes. Indirect traversal and Map index scratch additionally have a finite admitted cell bound derived from host validation limits; neither may grow during rendering. Existing finite-only previews retain their small depth-based scratch.

Visit Vec elements incrementally. Before sorting a Map, check entry count, scratch capacity and the sum of raw Text key bytes against the remaining output bound. Use allocation-free O(n log n) sorting of indices; comparisons read checked immutable keys. All loops either advance a bounded input cursor or consume admitted output/traversal space. Repeated shared aliases count against the same limits. No recursion on the Rust stack or guest allocation is needed. These are bounds, not a latency or memory-efficiency promise.

## Security and trust boundaries

Treat guest tokens as untrusted even inside a protected result. Reuse checked allocator/collection/registry observations; copying token bytes for inspection never creates an owner. Private custody coordinates cannot be supplied by source or a host caller. Do not expose raw addresses, private capacity, registry identity or physical Map order. Validate before observing payloads, and retain cleanup on every failure.

## Alternatives

- No change keeps useful collection expressions unsupported.
- A guest/library formatter would need guest execution, effects and explicit ownership operations, contradicting the protected-preview contract.
- Printing Map entries in physical/insertion order exposes a representation detail; canonical key order avoids that commitment.
- Length-only summaries are simpler but hide the values needed for ordinary REPL use.
- Following weak targets needs an upgrade or new authority and can introduce cycles; a checked liveness marker has neither cost.
- Extending arena lifetimes would change the ownership model and is rejected.

## Implementation plan

1. Add read-only collection/shared inspection to the existing iterative runtime formatter.
2. Supply the VM's existing shared registry under its immutable result-state borrow; preserve escrow and cleanup order.
3. Add exact-output, order, liveness, nested-value, limit and failure/cleanup evidence, then update bilingual architecture and REPL documentation. Frontend/IR, native codegen and package management require no structural change.

## Test plan

Exercise empty/nonempty/nested Vec and both Map key types, reverse insertion/replacement/removal, shared payload custody, live/expired weak tokens and recursive graphs whose weak edges stop. Check exact and insufficient output/scratch limits, unchanged service counters, balanced allocator releases, and failed-render cleanup. Preserve compile-fail arena escape and pre-effect unsupported authority tests. Run VM and CLI REPL fixtures plus the repository baseline; existing native conformance verifies unchanged guest semantics. No new ABI/fuzz/benchmark format is introduced.

## Open questions

No blocking design question. A future formatter with explicit live arena authority or native adapters requires a separate design change. Host-memory tuning can follow measured cost without weakening these bounds.

## Sources

Project decisions above are not external claims. Local sources verified **2026-10-20 JST**:

- [RFC 0004](0004-owned-values-and-cyclic-graphs.md), updated 2026-10-17: acyclic strong ownership and arena regions.
- [RFC 0007](0007-source-ownership-and-core-values.md), updated 2026-10-18: Map keys/no iteration, weak operations and arena escape rules.
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md), updated 2026-10-19: allocator authority and failure cleanup.
- [RFC 0010](0010-owned-repl-previews.md), updated 2026-10-18: protected result, limits, publication and explicit follow-up requirement.
- [Read-only inspection](../../crates/runtime/src/inspection.rs) and [shared registry checks](../../crates/runtime/src/shared/managed/graph.rs), living source without publication date; verified 2026-10-20 JST.
