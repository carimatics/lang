<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0007-source-ownership-and-core-values.md -->

# RFC 0007: Source ownership and core values

[日本語版](../ja/rfcs/0007-source-ownership-and-core-values.md)

- Status: implemented
- Authors: Project contributors
- Created: 2026-10-17
- Last updated: 2026-10-21
- Implemented: 2026-10-20
- Tracking issue:
- Supersedes:
- Superseded by:

## Current implementation status

Verified 2026-10-20 JST against the [M2 completion audit](../milestone-2-capabilities.md). The finite source/core profile is implemented through checking, independent MIR verification, VM/native execution and both-host conformance (M2 G03–G10, G14/G16). RFCs 0008/0009/0011 supply the required follow-up cleanup, extraction and artifact contracts. This is implemented, not stabilized.

The motivation, original staged plan and dated partial checkpoints below record the decision/rollout history. Their former pending gates are closed by the evidence above; normative contracts and explicit non-goals remain in force. This status reconciliation changes no source, ABI or format semantics.

## Summary

Complete Milestone 2's first decision gate without reopening RFC 0004's accepted ownership policy. Preserve RFC 0006 copy programs, add non-copy core values and lexical loans, and fix the smallest collection/type surface needed for command-line tools and an eventual bootstrap compiler. These accepted project choices are implemented at the finite profile documented above.

Use Rust-like `&T` / `&mut T` parameter types, explicit `shared T` / `weak T` ownership categories, and `arena T` for a reference branded by an inferred lexical arena. Ordinary records/enums are Copy exactly when all their fields/payloads are Copy. Core `Text`, `Bytes`, `Vec<T>`, and `Map<K, V>` own their storage and move. Only the listed built-in generic types/functions are introduced; user-defined generic declarations, methods, traits, lifetime parameters, and implicit cloning remain deferred.

The separate cleanup/allocator/unsafe/FFI decision gate remains mandatory. This RFC specifies the source type and operation surface, not allocator callbacks, foreign memory validity, destructor-failure ordering, or an ownership bytecode encoding. Acceptance alone does not complete any implementation task.

## Motivation

RFC 0006 intentionally has only copyable acyclic records/enums. Its `Own(ResourceTypeId)` backend prototype cannot represent owned text, lexical loans, containers, or arena-branded source values. RFC 0004 accepts ownership outcomes but leaves constructor and parameter spelling provisional. Implementing those independently in the parser, HIR, VM, and native backend would create incompatible contracts.

Representative tools require UTF-8 text, raw source/input bytes, growable sequences, symbol tables, explicit errors, and predictable lifetime boundaries. A library cannot enforce moves or arena escape checking. This proposal supplies those foundations; it does not replace the milestone with a scalar-only demonstration or claim a complete standard library.

## Goals

- Preserve every accepted RFC 0006 source/diagnostic fixture.
- Specify Copy, move, borrowing, field mutation, and lexical arena rules precisely enough for compile-pass/fail fixtures.
- Make allocation, shared cloning, weak upgrading, and checked shared mutation explicit named operations.
- Fix a finite core surface for text/bytes, collections, and fallible operations; permit bootstrap code to use concrete functions without requiring user generics.
- Keep typed MIR independent of the VM, native layout, and allocator implementation.

## Non-goals

- User generics, traits, methods, closures, inferred local annotations, implicit conversions, partial moves, reference fields, escaping ordinary loans, self-references/pinning, or borrowing across suspension.
- A user-defined destructor syntax, effect/exception syntax, `?` propagation, allocator callback ABI, C export syntax, raw pointers, or `unsafe` blocks. The second decision gate must settle the relevant contracts before their public interfaces exist.
- A new component ABI, package protocol, stable native representation, complete Unicode library, or uninstrumented native sandbox.
- Arbitrary key hashing/equality callbacks, concurrent container mutation, or performance promises.

## Guide-level design

All paths below are explicit `core::` paths; there is no new implicit prelude. Ordinary bindings retain RFC 0006's required `let mut name: Type = expression;` form, function-wide name uniqueness, explicit returns, and immutable parameter bindings.

```text
struct Message { body: core::Text }

fn length(message: &Message) -> i64 {
    return core::text_len(&message.body);
}

fn forward(message: Message) -> Message {
    return message;
}
```

Calling `length(value)` on a compatible place infers a read loan; `length(&value)` spells it explicitly. Calling `forward(value)` moves `value`. A later use reports `ownership.use-after-move` with the move and use spans. An inferred loan for a call ends when that call returns; a loan stored in a local lasts until that local's enclosing lexical block exits.

```text
fn update(value: &mut Message, replacement: core::Text) -> core::Unit {
    (*value).body = replacement;
    return core::Unit {};
}
```

An exclusive loan prevents other access to the same owner until it ends. Borrowing is conservative at whole-owner granularity; disjoint fields do not bypass the rule. Replacing an initialized field destroys the old field under the separately accepted cleanup rules. It does not partially move the enclosing record.

```text
struct Node { next: core::Option<arena Node>, value: i64 }

fn example(allocator: &core::Allocator) -> core::Unit {
    arena graph(allocator) {
        // Allocation and replacement are fallible. Match their Result values
        // explicitly; the compiler does not insert allocation or unwraps.
    }
    return core::Unit {};
}
```

`arena graph(allocator) { ... }` fixes RFC 0004's provisional construction spelling. T16 clarification (2026-10-21 JST, accepted under experimental delegation before implementation): entry borrows the allocator argument only while capturing the invocation's allocator authority into the compiler-created arena owner; argument temporaries end immediately after entry. The supplied host allocator outlives all task frames and arena retirement. This retains allocator identity without retaining a guest reference across suspension. A separately bound ordinary allocator loan keeps its lexical lifetime and is still rejected across suspension, as are all ordinary payload/read loans. Existing region-escape and task-transfer restrictions remain mandatory. This admits suspended arena scopes whose entry temporaries have ended; earlier source rejected them because those temporaries lasted for the whole region. The arena owner is compiler-created and cannot be moved, returned, copied, or written as a user parameter type. Its references may form same-region cycles. `arena T` is a reference token, not a dereferenceable raw pointer; access goes through the core arena operations below.

## Reference semantics

### Grammar and names

Extend `type` with `& type`, `& mut type`, `shared type`, `weak type`, `arena type`, and the listed built-in generic paths. Add prefix place borrowing `&place` / `&mut place`, parenthesized dereference `(*expression)`, field assignment, and `arena IDENT "(" expression ")" block`. Place forms are locals, borrowed parameters/dereferences, and record-field chains. Arbitrary expression temporaries cannot be explicitly borrowed into a local. Automatic field dereference applies to one ordinary loan; explicit `(*loan)` is equivalent. Shared/weak/arena tokens do not automatically dereference.

`shared`, `weak`, and `arena` are reserved words in this new pre-alpha profile; old uses as identifiers receive a migration diagnostic. `core` is a reserved root namespace and cannot be shadowed by an item/import/local. All other RFC 0006 lexical rules, expression precedence, module privacy, record-head restrictions, and trailing match commas remain unchanged. A lexer must distinguish `&`, `*`, generic `<` / `>`, and `::` without changing signed integer tokenization or comparison behavior.

Generic type arguments are mandatory on `core::Option<T>`, `core::Result<T, E>`, `core::Vec<T>`, and `core::Map<K, V>`. Named generic core calls use explicit `::<...>` arguments, for example `core::vec_len::<i64>(&values)`. A `::<` sequence is a call specialization, whereas ordinary `<` remains comparison. Nested generic closing brackets are individual `>` tokens. User items cannot declare type parameters or use an unlisted built-in generic constructor.

Core Option/Result cases use qualified paths `core::Option::None`, `core::Option::Some(value)`, `core::Result::Ok(value)`, and `core::Result::Err(error)`. The type arguments are obtained from an expected annotated local, function parameter/result, aggregate field, or match scrutinee; missing/ambiguous context is an error. Patterns have the existing full-shape and exhaustiveness rules. Unit is the inhabited empty record `core::Unit {}`.

### Copy and owning types

| Type | Copy rule |
| --- | --- |
| `i64`, `bool`, `core::Unit` | Copy |
| User record / enum | All stored fields / every case payload must be Copy |
| `core::Option<T>` / `core::Result<T, E>` | All type arguments must be Copy |
| `arena T` | Copy token, still branded and region-restricted |
| `core::Text`, `core::Bytes`, `core::Vec<T>`, `core::Map<K, V>` | Non-copy owning storage |
| `shared T`, `weak T` | Non-copy tokens; duplication requires explicit clone |
| Ordinary `&T`, `&mut T` | Loans, not storable owning values; forwarding creates a reborrow |
| `core::Allocator` | Opaque host-created authority, usable only through a loan |

Copy values preserve RFC 0006 assignment/call/return behavior. Moving a non-copy place transfers its entire initialized value. By-value field extraction from a non-copy aggregate is rejected, including moving only one match binding. Matching such an aggregate borrows its scrutinee once for the whole match; non-copy bindings become read loans to their fields, while Copy bindings copy. Mutating or moving the scrutinee while those loans are live is an error. A by-value wildcard does not silently consume an owner. Moving the whole scrutinee after the match is allowed once its loans have ended.

Follow-up accepted 2026-10-18 JST: [RFC 0009](0009-whole-sum-ownership-extraction.md) adds explicit whole Option/Result extraction through `option_unwrap`, `result_unwrap`, and `result_unwrap_err`. They consume the whole sum by value, return its expected payload, or panic with RFC 0008 cleanup. This extends the finite core catalog below without changing borrowed matching or permitting partial moves; acceptance does not imply implementation.

Records/enums may recursively mention themselves only through owning indirection (`Vec`, `Map`, `shared`, `weak`) or `arena` references. The compiler checks finite inline layouts and a finite set of concrete built-in instantiations; recursively expanding type arguments are rejected by a charged work/depth/instantiation budget. `Option<Node>` alone does not break an inline cycle. Types containing ordinary loans are forbidden in fields, containers, function results, and globals. `Allocator` cannot be stored in guest values.

### Loan and region checking

An owning argument never implicitly borrows. A borrowed parameter may accept an explicit matching loan or infer a loan from a compatible place. Shared reborrows permit reads; exclusive reborrows freeze their parent for their own lexical duration. A mutable loan requires a mutable local/place or an existing exclusive loan. A function parameter's owning binding remains immutable. Shared loans allow other shared loans and exclude mutation/move/destruction; exclusive loans exclude all other access except through that loan.

Named local loans last through their enclosing block, including unused ones. Call-only inferred/explicit loans and temporary reborrows end in reverse creation order after the call, including its error edge. Branch joins conservatively merge initialization/move/loan states: an owning use requires initialization on every reaching path. Reinitializing a wholly moved mutable local is allowed; borrowing an uninitialized local is not. Loop back-edges must reestablish the header's ownership state. No partial aggregate initialization or moves are permitted. Assignment evaluates its destination place, then RHS, validates replacement, publishes it, then cleans the replaced value without exposing an intermediate uninitialized owner.

User function results cannot be ordinary loans, and user bodies cannot return/store loans. Core read/projection operations are compiler-known exceptions producing loans tied to the passed owner at the call site, never arbitrary library-returned references. Every value's transitive arena brands are tracked through locals, fields, containers, calls, and patterns. A region-bearing value cannot outlive its region, enter shared/weak storage, cross a distinct arena, or escape as a function result. An ordinary owning local inside the same region may hold arena tokens. Arena references passed into a function are read-only tokens branded by the caller; callee allocation/mutation still needs that caller's arena owner, which is not a public parameter type. No task, suspension, global storage, or closure escape is introduced.

### Core type and operation surface

All generic parameters below are concrete at a call site. `A` abbreviates `&core::Allocator`, `U` is `core::Unit`, and `R<T, E>` is `core::Result<T, E>`; these abbreviations are documentation only. Counts/indices are nonnegative `i64`; negative/out-of-range indices return a structured error. No conversion truncates an index or size.

| Family | Required named operations and signatures |
| --- | --- |
| Allocator authority | `allocator() -> &Allocator`, a compiler-known execution-instance loan; the host must supply and authorize this allocator before entry, otherwise entry preflight fails without guest effects |
| Text | `text_from_utf8(Bytes) -> R<Text, Utf8Error>`, `text_clone(A, &Text) -> R<Text, AllocError>`, `text_len(&Text) -> i64`, `text_bytes(A, &Text) -> R<Bytes, AllocError>`, `text_equal(&Text, &Text) -> bool`, `text_concat(A, &Text, &Text) -> R<Text, AllocError>` |
| Bytes | `bytes_new(A) -> R<Bytes, AllocError>`, `bytes_clone(A, &Bytes) -> R<Bytes, AllocError>`, `bytes_len(&Bytes) -> i64`, `bytes_get(&Bytes, i64) -> R<i64, BoundsError>`, `bytes_push(&mut Bytes, i64) -> R<U, BufferError>`, `bytes_set(&mut Bytes, i64, i64) -> R<U, BufferError>` |
| Vec | `vec_new::<T>(A) -> R<Vec<T>, AllocError>`, `vec_len::<T>(&Vec<T>) -> i64`, `vec_push::<T>(&mut Vec<T>, T) -> R<U, AllocError>`, `vec_pop::<T>(&mut Vec<T>) -> Option<T>`, `vec_get::<T>(&Vec<T>, i64) -> R<&T, BoundsError>`, `vec_get_mut::<T>(&mut Vec<T>, i64) -> R<&mut T, BoundsError>` |
| Map | `map_new::<K,V>(A) -> R<Map<K,V>, AllocError>`, `map_len::<K,V>(&Map<K,V>) -> i64`, `map_insert::<K,V>(&mut Map<K,V>, K, V) -> R<Option<V>, AllocError>`, `map_get::<K,V>(&Map<K,V>, &K) -> Option<&V>`, `map_remove::<K,V>(&mut Map<K,V>, &K) -> Option<V>` |
| Shared/weak | `shared_new::<T>(A, T) -> R<shared T, OwnershipError>`, `shared_clone::<T>(&shared T) -> R<shared T, OwnershipError>`, `shared_read::<T>(&shared T) -> R<&T, OwnershipError>`, `shared_replace::<T>(&mut shared T, T) -> R<U, OwnershipError>`, `weak_new::<T>(&shared T) -> R<weak T, OwnershipError>`, `weak_clone::<T>(&weak T) -> R<weak T, OwnershipError>`, `weak_upgrade::<T>(&weak T) -> R<shared T, OwnershipError>` |
| Arena | `arena_new::<T>(&mut arena-owner, T) -> R<arena T, ArenaError>`, `arena_read::<T>(&arena-owner, arena T) -> R<&T, ArenaError>`, `arena_replace::<T>(&mut arena-owner, arena T, T) -> R<U, ArenaError>`; the owner notation is internal, source passes the lexical arena name |

Here `Text`, `Bytes`, and error names denote their `core::` types. Borrow-bearing Option/Result results in this table are special compiler-known loan results: they may be matched immediately, but not stored, returned, or nested into another owning value. Successful bindings become lexical loans. Unused/failed arms end the provisional loan. There are no general `Option<&T>` / `Result<&T,E>` user types.

Text is valid UTF-8; length counts bytes, equality compares bytes, and no normalization or scalar/grapheme indexing is implied. `text_from_utf8` consumes its Bytes on either outcome, reports the first invalid byte offset, and never silently replaces malformed input. Bytes elements are checked integers in `0..=255`; invalid elements report `BufferError::InvalidByte`. There are no text/byte literal escapes in this profile: explicit typed host input and byte construction supply data. String literal ergonomics may be proposed separately without preventing compiler source ingestion.

Map keys are exactly `i64` or `core::Text`, with numeric / exact UTF-8-byte equality. No iteration API or observable hash order is introduced; bootstrap algorithms requiring an explicit order maintain a separate Vec. Replacing a key preserves the existing key, consumes/drops the supplied equal key, and returns the old value. Push/insert failure leaves the collection unchanged, but owning arguments have already moved and are cleaned inside the operation. Allocating constructors consume owning arguments on failure too. No failure implicitly restores a moved source variable. Failure atomicity describes preflight/publication; later cleanup of evaluated incoming owners is explicit and may release their references. Each allocating collection retains its allocator identity for later growth/destruction; the allocator and its host context must outlive every allocation. `core::allocator()` makes owning programs reachable from the existing zero-parameter `main`; it exposes only a loan to authority explicitly supplied for that execution instance, never an ambient native allocator or callback address.

Core errors are finite Copy enums: `AllocError::{Limit, OutOfMemory, Overflow}`, `BoundsError::OutOfBounds`, `BufferError::{Bounds, InvalidByte, Allocation(AllocError)}`, `Utf8Error::Invalid(i64)`, `OwnershipError::{Allocation(AllocError), StrongCycle, ExpiredWeak, BorrowConflict, Unavailable, CountOverflow}`, and `ArenaError::{Allocation(AllocError), CrossRegion, Unavailable, BorrowConflict}`. `text_from_utf8` requires no additional allocation: it transfers owned bytes and their allocator identity after validation. Clone/new/concat/growth errors use the table's declared result error. Empty storage may avoid an allocation but must preserve allocator identity and enforce configured accounting consistently. Lengths must fit nonnegative i64 before allocation/publication; unrepresentable requests return the operation's allocation-overflow error.

Shared payloads are only read through checked read loans or atomically replaced through `shared_replace`; unrestricted mutable dereference is absent. Outstanding reads on any alias prevent replacement at the dynamic boundary. Replacement preflights every nested stored strong edge before publishing the whole new payload, preserves the old payload on rejection, and runs cleanup only after leaving the atomic publication boundary. It obeys RFC 0004's cycle/count rollback policy, including explicit ownership of incoming values. Expired weak upgrade returns `OwnershipError::ExpiredWeak` with the accepted logical identity. Arena reads borrow the arena, preventing replacement/allocation through that owner until the loan ends; same-region references can be copied without retaining nodes. Arena exit revokes access before reverse-allocation-order teardown. Neither category creates raw address authority.

## Execution modes

HIR records ownership and inferred regions/loans; MIR makes moves, borrow begin/end, replacement, shared/weak operations, arena enter/exit, and cleanup edges explicit. VM, JIT, and AOT must use the same verified semantic transitions and runtime operation results. Every checked operation retains its original span, and rejection must precede guarded guest/host effects. Resource exhaustion must not publish partial state. Existing copy-only bytecode schema 1 remains unchanged; a separately accepted versioned ownership encoding is required before serializing the new operations. Accepting in-memory types does not authorize inventing decoder tags.

The native backend's private cell convention in `926fb3b` is implementation evidence for copy values, not an owning heap ABI. Until a path implements and validates an ownership operation it reports a structured unsupported-feature error before execution/code publication. Such intermediate restrictions cannot satisfy the Milestone 2 exit. Constant evaluation of these operations remains unavailable. Embedding supplies typed values and allocator authority explicitly; component resources still use RFC 0003 adapters. No new ambient filesystem/network/process/environment permission follows from these types.

## ABI and compatibility

Compatibility domains: source, bytecode, native-abi, tooling, and bootstrap. All RFC 0006 Copy programs retain their meaning except newly reserved words/root namespace; migration reports each affected name and offers a conflict-checked rename, never implicit clones. Generic delimiters and loan syntax must preserve every old parse fixture. The bytecode schema-1 golden artifact remains byte-identical and supported. A future ownership artifact has a distinct schema and rejects unsupported versions.

Native ownership/allocator representations remain toolchain-private and must not leak through frontend APIs. Shared/weak/arena tokens and source containers are not C/component layouts. Public module signatures record normalized ownership/type arguments and reject leaked ordinary loans/arena brands. Package metadata recording those signatures remains subject to its own accepted versioned format; this proposal introduces no unversioned package bytes or stable ABI. Bootstrap can initially use concrete helper functions and built-in concrete collection specializations without user generics.

## Diagnostics and tooling

Retain RFC 0004 logical identities and existing `SRC0001`–`SRC0019`. Reserve `SRC0020` for use-after-move/uninitialized owner, `SRC0021` for loan conflict/illegal borrowing, `SRC0022` for escaping loan/arena brand, `SRC0023` for illegal ownership/type storage, and `SRC0024` for unsupported built-in specialization/reserved-name migration. The precise category and related move/borrow/region spans are structured fields; diagnostics must not be selected merely from a later backend failure. Compile-limit failures keep `SRC0011`. Runtime cycle/cross-region/stale/expired-weak identities retain RFC 0004 meanings and point at the operation.

Parser recovery preserves malformed loan/type-argument forms and later items. Formatter round trips cover nested `>>`, `::<...>`, `&mut`, prefix categories, dereference-field assignment, and arenas. LSP hover/references show Copy vs move/loan, source owner, and inferred arena region; diagnostic publication remains revision-safe. Native debug views distinguish source values/tokens and unavailable states without exposing addresses. Full linter/DAP implementation remains in their scheduled milestones; this RFC does not mark those tools done.

## Performance and cost model

These are hypotheses and implementation requirements, not measured performance results. Copy aggregate classification is structural; moves/loans need not allocate or retain. Text/Bytes/Vec/Map construction and growth have explicit fallible allocation paths and allocator identity. Text clone/concat and bytes conversion can copy input length. Generic built-ins may specialize per concrete type, increasing code size; compiler instantiation/work/allocation budgets must bound expansion. Shared/weak metadata and cycle traversal keep RFC 0004's visible costs, while arenas provide bulk lifetime without promising bump allocation.

Before claiming representative performance, measure source parsing/checking, container growth, UTF-8 validation, symbol-table workloads, retain/release and cycle publication, peak live/requested bytes, executable size, and VM/native differences on both tier-1 platforms. Compare equivalent Rust/C workloads under the same inputs, optimization mode, allocator/accounting scope, and sample policy. Current native copy tests prove behavior, not ownership cost or bootstrap speed.

## Security and trust boundaries

Validate owning type/region identity, loans, handles, generation, initialization, and operation sequence at MIR/bytecode/host boundaries. No source path/diagnostic name authorizes I/O. Checked size/index arithmetic precedes storage access/reservation; count/generation exhaustion fails closed. Explicit compile/decode/runtime work and allocation limits cover generic instantiation, container requests, UTF-8 scans, graph traversal, and cleanup bookkeeping. Foreign mutation cannot bypass tracked shared publication. Native callbacks and uninstrumented native execution remain trusted; ownership is not a process sandbox.

## Alternatives

- **No change:** continue RFC 0006 Copy-only source. It leaves both ownership/core-library roadmap outcomes incomplete and cannot implement the required production profile.
- **Library/tooling only:** host-provided containers can supply functionality, but cannot establish move/loan validity or arena escape checks across all modes.
- **Strongest competing surface:** unrestricted user generics, traits, methods, nonlexical lifetime analysis, and reference-valued APIs. They offer broader abstraction but require a much larger inference, coherence, monomorphization, and lifetime contract before the first owning source program. They remain possible future RFCs; no ergonomics/performance superiority is inferred here.
- **Simpler ownership-only surface:** add Text plus moves and defer all sharing, arenas, and collections. It is a useful first implementation slice, but cannot replace the existing Milestone 2 requirements. This proposal retains all of them while allowing staged implementation.

## Implementation plan

1. Record disposition by the owner or under RFC 0001's experimental delegation. Keep `proposed` until accepted; keep both roadmap decision gates unchecked until their respective accepted contracts exist.
2. Add lossless grammar/format/recovery fixtures and normalized source type identities. Implement Copy/move/lexical-loan/region analysis with compile-pass/fail diagnostics and charged dataflow/instantiation bounds.
3. Specify/accept remaining cleanup, allocator, unsafe/FFI, and versioned ownership bytecode contracts before implementing their public/wire interfaces. Re-estimate core-library and native integration work against the concrete surface above; preserve every original outcome.
4. Add explicit MIR transitions/verifier and runtime allocation/shared/arena services, then implement the same source operations in VM/JIT/AOT. Reject unsupported paths before effects/publication during development.
5. Add text/bytes, finite collection specializations, allocator-aware growth, and representative source CLI/bootstrap-shaped workloads. Typed host I/O requires separately accepted capability and host-value interfaces.
6. Complete tier-1 ownership-aware link/run/debug evidence, differential cleanup/failure traces, representative Rust/C comparisons, bilingual guides, formatter/LSP changes, and fuzzing. Acceptance is not milestone completion or stabilization.

## Test plan

| Area | Required evidence |
| --- | --- |
| Compatibility | All RFC 0006 source fixtures and schema-1 golden bytes unchanged; reserved-name migrations diagnose precisely |
| Copy/move | Scalars and nested Copy aggregates remain usable; Text/Vec/shared/weak use after move fails; whole-local reinitialization works; partial moves fail |
| Loans | Inferred/explicit read, exclusive update, shared/exclusive reborrow, lexical unused-loan conflict, overlapping fields, moved owner, borrowed parameter return/storage, and joins/back-edges |
| Regions | Same-arena cycles; nested arena escape; cross-arena replacement; arena-to-shared/container/function-result escapes; read loan blocks arena mutation; stale generations never alias reuse |
| Core | UTF-8 invalid offsets, 0/255/256 byte values, negative/max indices, empty/pop/map replacement, allocator retention, every allocating-operation failure before publication |
| Generic bounds | Built-in arity, invalid key types, recursive inline types, finite recursive indirection, repeated concrete specialization, exact depth/work/allocation/instantiation limits |
| Shared runtime | Explicit clone/drop, retained weak upgrade, read/replace conflict through another alias, cycle preflight through nested containers, rollback/races and count exhaustion |
| Cross-mode | Same values, side-effect/drop order, failure reasons, spans and resource accounting across VM/JIT/AOT; direct/response-file execution on both tier-1 hosts |
| Tooling/trust | Recovery/format round trips, revision-safe loan diagnostics, malformed ownership MIR/artifacts/handles, fuzzed generic/type/region graphs, documented ABI rejection |

## Open questions

The repository owner explicitly accepted this source/core profile on 2026-10-17 JST, including lexical whole-owner borrowing, compiler-known built-in generic collections, and checked whole-payload shared mutation. There are no remaining blocking questions for this profile. Cleanup/allocator/FFI validity and exact ownership wire encoding deliberately require their own decisions before dependent implementation. Those follow-ups are not optional for Milestone 2 completion.

Decision: **accepted on 2026-10-17 JST** by the repository owner. The pre-decision review outcome was ready for decision. Implementation, stabilization, and measured ergonomics/performance are not claimed.

Follow-up decision: **RFC 0009 accepted under experimental delegation on 2026-10-18 JST** after the first source execution exposed the inability to obtain owned payloads from allocating results. Its explicit operations and failure/compatibility rules amend this catalog; the original match rule stays in force.

## Sources

- [RFC 0009](0009-whole-sum-ownership-extraction.md), accepted whole-sum extraction follow-up and reproduced source blocker; created/updated and verified 2026-10-18 JST.
- [RFC 0004](0004-owned-values-and-cyclic-graphs.md), accepted ownership/category/cycle/arena policy; last updated 2026-10-17 JST, verified 2026-10-17 JST.
- [RFC 0006](0006-source-frontend-and-portable-programs.md), implemented source/Copy/portable-program baseline; last updated 2026-10-17 JST, verified 2026-10-17 JST.
- [Architecture](../architecture.md) and [roadmap](../roadmap.md), project contracts/remaining outcomes; last updated 2026-10-17 JST, verified 2026-10-17 JST.
- `crates/semantics/src/graph.rs`, comparative ownership state model, last updated in `98bbb08` on 2026-10-17 JST; `crates/codegen-clif/src/program.rs` and `tests/native_program.rs`, copy-value native implementation/evidence, commit `926fb3b`, committed 2026-10-17 JST. These prove their stated prototype/copy boundaries, not this source extension.
- This proposal makes no new external-language factual or performance claim. Syntax, finite core APIs, migration rules, limits, and deferrals are project choices derived from the accepted repository contracts; comparative designs are alternatives, not independently measured findings.
