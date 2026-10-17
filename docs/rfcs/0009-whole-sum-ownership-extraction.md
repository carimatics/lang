<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0009-whole-sum-ownership-extraction.md -->

# RFC 0009: Whole Option/Result ownership extraction

[日本語版](../ja/rfcs/0009-whole-sum-ownership-extraction.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-18
- Last updated: 2026-10-18
- Tracking issue: M2-S01–S03 in [the delivery board](../milestone-2.md)
- Supersedes:
- Superseded by:

## Summary

Extend RFC 0007's finite core catalog with three explicit operations that take an entire Option/Result by value. A matching case returns its entire payload; the other case raises a guest panic with RFC 0008 cleanup. Existing borrowed matching and the prohibition on partial moves remain unchanged. This is an accepted design, not implemented source behavior.

| Operation | Signature | Expected case | Other-case panic code |
| --- | --- | --- | ---: |
| `core::option_unwrap::<T>` | `(core::Option<T>) -> T` | Some | -1 |
| `core::result_unwrap::<T,E>` | `(core::Result<T,E>) -> T` | Ok | -2 |
| `core::result_unwrap_err::<T,E>` | `(core::Result<T,E>) -> E` | Err | -3 |

The i64 codes are fixed for these operations; explicit user `core::panic` may use the same numbers. They are not a reserved user-code namespace. The operation's source span and guest-panic category accompany the code.

## Motivation

The first source Bytes execution fixture (`ece8690`) constructs `Result<Bytes, AllocError>` and reads its length through a borrowed match. Trying to initialize a mutable owned Bytes local from the Ok binding fails with `SRC0014`: expected `core::Bytes`, found `&core::Bytes`. This is the specified RFC 0007 behavior, reproduced on 2026-10-18 JST, rather than a lowering bug.

Consequently, allocation results cannot currently supply an owner to Bytes mutation, consuming UTF-8 conversion, or collection/shared constructors. Borrowed matching also cannot extract an owned Vec pop or Map removal result. A user library cannot implement the missing transfer: its own match has the same read-loan rule. Fix the design explicitly before adding a divergent implementation.

## Goals

- Reach owning mutation/conversion from fallible constructors with a small explicit source surface.
- Support both Result payloads and Option Some, including nested owning payloads, without partial moves.
- Preserve existing loan/region rules, allocator identity, exactly-once destruction, source spans, and backend-independent typed MIR.
- Keep recoverable errors as explicit matches/returns; make unchecked extraction visibly capable of panic.

## Non-goals

- Consuming pattern syntax, extraction from arbitrary record fields or user-enum cases, mutable match bindings, `?`, catch/throw, user generics, or inferred type arguments.
- A new allocator, ordinary loan result, lexical-loan shortening, arena escape, concurrency permission, public representation, wire encoding, or completed VM/native ownership profile.

## Guide-level design

Use an unwrap only when the expected case is required, or after a borrowed match handles the other case. This accepted example first handles allocation failure and then moves the entire Result after the match loan ends:

```text
fn main() -> i64 {
    let mut made: core::Result<core::Bytes, core::AllocError> =
        core::bytes_new(core::allocator());
    match made {
        core::Result::Ok(_) => {},
        core::Result::Err(_) => { return -1; },
    }
    let mut bytes: core::Bytes =
        core::result_unwrap::<core::Bytes, core::AllocError>(made);
    let mut pushed: core::Result<core::Unit, core::BufferError> =
        core::bytes_push(bytes, 65);
    match pushed {
        core::Result::Ok(_) => {},
        core::Result::Err(_) => { return -2; },
    }
    return core::bytes_len(bytes);
}
```

The intended successful result is `1`. This example becomes executable only when extraction and Bytes push dispatch are implemented. An unguarded unwrap is legal and checks its case at runtime; a compiler need not prove the preceding match. To recover an owning Err, handle Ok in a borrowed match and then call `result_unwrap_err` on the whole Result. A caller may instead return the whole Result without extraction. No automatic propagation or conversion is inserted.

## Reference semantics

### Syntax, types, and evaluation

These are three additional compiler-known names in the reserved `core` namespace, using the existing `::<...>` call grammar. All listed type arguments and exactly one value argument are mandatory. The actual argument must have the exact specialized Option/Result type; an owning parameter never implicitly borrows, and an ordinary loan is not automatically dereferenced into an owner. The result is the selected type argument. No new parser keyword or context-dependent match form is introduced.

Each type argument satisfies the existing Storable rule: its transitive contents contain no ordinary loan, allocator authority, or lexical arena-owner authority. In particular, a provisional `Result<&T,E>` / `Option<&T>` from a core read operation cannot be unwrapped; it still requires immediate matching. Storable arena-bearing values remain permitted only under RFC 0007's existing region constraints.

The single argument is evaluated once, in the existing expression evaluation order. A non-Copy argument is moved in full before the case check and is unavailable on every returning path. A Copy Option/Result supplies a copy and leaves its source usable. The Copy rule is determined by the whole sum, including both Result type arguments, not just the active payload. Evaluation failure before completion cleans the temporaries already initialized under RFC 0008.

### Ownership, failure, and regions

On the expected case, transfer the entire sole payload into the result and retire the consumed sum's enclosing initialization obligation. Do not clone, retain, release, run payload cleanup, or leave a usable alias at the source. Preserve nested construction-order metadata, allocator identity, logical identities, and transitive arena provenance. A payload that is Copy may be copied into the result, but an enclosing non-Copy sum is still consumed. This is whole-value consumption, not a move of one field from a surviving aggregate.

The case check and successful obligation transfer form one ownership transition: no fuel, interruption, callback, or other failure boundary may observe an unowned payload or two owning obligations. Validation and bounded destination/retirement storage admission precede that transition. The unpublished result remains subject to the normal failure/return cleanup rules until its caller receives it.

On the other case, first record the table's guest panic as the primary execution failure at the call span, then clean the entire active input payload and all other initialized obligations on the generated failure path. An owning Err in `result_unwrap`, or an owning Ok in `result_unwrap_err`, is cleaned exactly once. Cleanup failures are secondary, cleanup continues, and poisoning follows RFC 0008. Successful cleanup of this guest panic alone does not poison the instance. There is no source catcher, hidden typed Err, payload formatting, or allocation for a panic message. Input-evaluation failures retain their original primary reason rather than becoming an unwrap panic.

Live loans prevent moving the non-Copy input, including a named loan that is no longer used but remains lexically live. A borrowed non-Copy field cannot be passed by value; a Copy whole sum can be copied from an otherwise valid read place. Extraction preserves the selected payload's arena restrictions through locals, aggregates, containers, calls, and returns; it cannot launder a region into shared storage or out of its scope. It creates no new loan, authority, sharing, atomic, scheduling, or unsafe exemption. Constant/compile-time execution remains unavailable, as for the other owning core operations.

## Execution modes

Typed MIR must describe each operation's concrete signature, by-value operand, result type/provenance, checked case, and panic/failure edge independently of a backend. Extend the finite HIR/MIR core-call contracts; independently validate arity, storage bounds, input/result types and loan/region behavior. Do not infer safety from the source checker alone. A backend may expand the operation to a case test and whole-payload transfer; a source match continues to lower as a borrow. An unchecked `UnpackVariant` on a mismatched case is not an implementation of the specified guest panic.

VM, native AOT, optional JIT and component adapters must agree on returned values, moved-source behavior, panic codes/spans, drop order and failure precedence. During staged implementation, an unsupported operation or payload kind must be rejected before guest/provider effects or code publication, using the existing structured feature gate. Initially executing Bytes does not claim nested aggregates, Vec/Map/shared/arena, REPL, or native completion. Frame/cell limits and fuel remain charged normally; transfer and failure reporting cannot acquire additional guest storage.

Embedded hosts receive the same structured failure through existing boundaries. RFC 0008's C profile still rejects owning/loan-bearing signatures; these helpers are not C exports or a foreign unwind mechanism. RFC 0003 component resources retain their adapter contracts. No separate JIT, component, LSP, or DAP implementation is introduced by accepting this RFC.

## ABI and compatibility

Affected domains: source, internal MIR/VM semantics, native implementation, tooling, and bootstrap. This explicitly amends RFC 0007's finite core catalog; it does not supersede the rest of that RFC or RFC 0008. Existing source matches retain their meaning, including `_` and non-Copy binding loans. The reserved namespace prevents a previously valid user declaration from conflicting with the new names. No edition migration is required; users opt into calls and may replace failed attempts at moving match bindings with match-then-unwrap.

Schema-1 artifacts and their golden bytes stay unchanged. These operations have no serialized tags until the separate ownership-format RFC is accepted and its decoder/verifier is implemented. Native representations remain toolchain-private, with no new C/component layout or package bytes. Future public metadata must use its accepted versioned format. A later change to panic, extraction, or match semantics needs another explicit RFC revision/follow-up before implementation.

## Diagnostics and tooling

Use existing structured identities: `SRC0024` for unsupported specialization/arity, `SRC0023` for illegal storage, `SRC0014` for argument type mismatch, `SRC0020` for moved/uninitialized input, `SRC0021` for a live-loan conflict, and `SRC0022` for a region escape, with the existing specific diagnostic precedence. Preserve call/type-argument/move/borrow/region spans and related locations. A wrong case is a runtime guest panic, not a type mismatch or invariant corruption. Document each fixed panic code beside the signature.

Existing formatter generic-call rules apply without syntax changes; add nested-specialization round trips. LSP hover can show the specialized by-value signature and result, and debugger views mark the moved source unavailable. Do not auto-fix a borrowed binding to an unwrap: that introduces a possible panic. Existing revision-safe diagnostics and deferred linter/DAP delivery obligations remain unchanged.

## Performance and cost model

The case check and ownership transfer allocate no new guest payload, transfer metadata, drop stack, or panic report. Required storage is admitted in the normal bounded frame/allocation plan before effects; admission itself can fail. This does not make the entire allocating expression infallible or eliminate normal frame accounting. Transfer need not be a physical zero-byte copy: relocating inline payload and construction metadata can cost work proportional to their layout. It does not traverse heap payloads to clone or increment reference counts. There is no user dispatch or callback on a successful extraction.

The match-then-unwrap idiom may test the tag twice. Eliminating a redundant test is optional and must preserve failure behavior and provenance. Compile-time specialization/type checking remains subject to existing charged work/depth/instantiation limits. No measured speed, ergonomics, code-size, or constant-time guarantee is claimed; measure the source fixture and nested transfer costs when those paths execute.

## Security and trust boundaries

Untrusted MIR/artifacts cannot assert an expected tag, forge a payload type, duplicate an owner, erase an arena brand, or bypass pre-admission/cleanup by naming an intrinsic. Validate the type/tag/layout and authority at the existing trust boundaries. A malformed internal value is a validated runtime fault under existing rules, whereas a valid other case is the specified ordinary guest panic. Cleanup must remain bounded and allocation-free after failure. No filesystem, network, process, environment, native address, or host capability is granted.

## Alternatives

| Alternative | Decision and tradeoff |
| --- | --- |
| No change | Keeps an accepted but unusable construction-to-mutation path; the observed `SRC0014` blocks the next source fixture. |
| User helper/library or tooling | Cannot cross the existing move/borrow rule. A compiler-known helper is required; the selected finite core operations make that exception explicit. |
| Consuming `match move` or consuming patterns | Strongest general alternative: permits recoverable ownership handling of every user-enum case without a second tag check. It needs a new syntax/typing/branch-join contract, all-case obligation transfer, and a decision for Copy/wildcard/nested patterns. Defer that broader design; it may later coexist with these helpers. |
| Mutable payload loans | Helps in-place Bytes changes but cannot supply the owning Bytes consumed by UTF-8 conversion or move a collection removal result. It also needs a new loan form. |
| Only Ok/Some unwrap | Slightly fewer intrinsics, but strands owning Err payloads. The third operation makes either Result branch recoverable after a guard match. |
| Infallible constructors or unchecked extraction | Simpler failure paths discard the accepted recoverable allocation or checked cleanup guarantees; rejected. |

## Implementation plan

1. **M2-S01:** accept this explicit follow-up under RFC 0001 delegation; amend RFC 0007 and both translations. Keep implementation status separate.
2. **M2-S02 (3 pt):** extend HIR/MIR finite signatures and source lowering with compile-pass/fail, move/loan/region/provenance, malformed MIR, and formatter evidence. Reuse existing call syntax and by-value checking. Add no wire encoding or runtime dispatch in this slice.
3. **M2-S03 (3 pt):** add admitted whole-payload transfer and case-failure execution for the supported Bytes/Copy profile; execute the source owner-extraction fixture, panic cleanup, fuel prefixes, OOM/admission boundaries, and no-transfer-allocation assertions. Gate unsupported payloads before effects.
4. **M2-V04/V06 and existing collection/shared/arena/native tasks:** extend actual operation/payload coverage, including mutation, nested construction-order transfer, both Result branches and Option, then cross-mode and tier-1 conformance. No original exit gate is removed. S02/S03 remain provisional: split/re-estimate before coding if an implementation prerequisite exceeds its bounded outcome.

## Test plan

- Compile pass: all three explicit specializations; Copy source reuse; Bytes owner extraction followed by mutable local use; guarded Err extraction; same-region arena-bearing payloads; existing borrowed-match behavior.
- Compile fail: wrong/missing type arguments, wrong sum/type/loan argument, prohibited storable types, provisional read unwrap, live named loans, moved-source reuse, branch/loop initialization, and transitive arena escape through the new calls.
- MIR/trust: independently reject wrong signatures/result types, invented tags or guards, forged provenance, duplicate owners and invalid transfer destinations within current verification boundaries. The existing broader arena-verifier task remains mandatory.
- Execution: expected cases, each fixed wrong-case panic, owning payloads on both success/failure, nested exactly-once reverse construction-order cleanup, shared/allocator/region identity preservation, all fuel prefixes, host interruption, cleanup failure precedence/poisoning, and preflight/OOM rejection before effects. Count provider requests to prove no new transfer allocation.
- Compatibility/tooling: existing Copy source and borrowed-match fixtures, schema-1 golden artifact, generic-call formatting/recovery and precise source diagnostics. Add unwrap inputs to the existing fuzz targets when their implementation lands; no new wire fuzz target precedes an accepted encoding.
- Cross-mode: identical values, panic code/span, drop traces and limits in VM/native differential and both tier-1 executions once those implementations exist. Measure inline/nested transfer and duplicate-tag costs before making performance claims. Design acceptance alone supplies none of this execution evidence.

## Open questions

Decision: **accepted under experimental delegation on 2026-10-18 JST**. The review finds a concrete reproduced blocker, an additive finite API using existing syntax/ownership rules, explicit panic/cleanup semantics, and bounded compiler/runtime slices consistent with RFCs 0007/0008. No blocking design question remains for this scope; no individual owner approval is queued.

Unverified assumptions: three specialized operations fit the provisional S02/S03 slices; guard-then-unwrap ergonomics are sufficient for early source workloads; all supported payload layouts can transfer with admitted metadata. Implementation must test these assumptions, expose newly discovered work before expanding a slice, and retain gates for unsupported payloads/modes. A later general consuming match is a separate design decision, not implicit permission to change borrowed matching.

## Sources

- [RFC 0001](0001-project-charter.md#experimental-rfc-decisions), delegated decision/revision policy; last updated 2026-10-17 JST (`b08f24b`), verified 2026-10-18 JST.
- [RFC 0007](0007-source-ownership-and-core-values.md), whole-owner moves, borrowed matches, core catalog and region rules; original accepted text last updated 2026-10-17 JST (`31ab7ac`); follow-up reference added 2026-10-18 JST, verified the same day.
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md), panic, cleanup precedence, allocator and C boundaries; last updated 2026-10-17 JST (`b094a58`), verified 2026-10-18 JST.
- [MIR core validation](../../crates/mir/src/unit/ownership/core/validation.rs), existing transitive Storable check, last changed 2026-10-17 JST (`4619937`); [ownership instructions](../../crates/mir/src/unit/ownership/bodies.rs), existing whole `UnpackVariant` contract, last changed 2026-10-18 JST (`a984e5e`); both verified 2026-10-18 JST. These show implementation seams, not completed extraction.
- [Source Bytes fixture](../../examples/scripts/bytes.lang) and [execution tests](../../tests/owned_execution.rs), last changed 2026-10-18 JST (`ece8690`); the read-only source run prints `0`, while attempting an owned Bytes binding from the match payload reproduces `SRC0014`, verified 2026-10-18 JST. Checking this RFC's guide example with the existing compiler reaches the new name and reports `SRC0024` (unknown core operation), confirming the grammar path but not new type/runtime semantics.
- [Delivery board](../milestone-2.md), scope/dependencies/remaining estimates, updated and verified 2026-10-18 JST. This RFC makes project choices, with no new external-language or measured performance claim.
