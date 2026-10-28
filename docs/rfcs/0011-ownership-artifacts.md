<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0011-ownership-artifacts.md -->

# RFC 0011: Portable ownership artifacts

[日本語版](../ja/rfcs/0011-ownership-artifacts.md)

- Status: implemented
- Authors: Project contributors
- Created: 2026-10-18
- Last updated: 2026-10-28
- Tracking issue: M2-A01/A02/A03 and N01 in [the delivery board](../milestone-2.md)
- Supersedes: None; adds ownership schema 2 alongside RFC 0006 schema 1
- Superseded by:

Extension (2026-10-19): [RFC 0014](0014-explicit-source-panic.md) selects schema 3 for explicit Panic units. The schema-2 profile specified here remains closed and byte-compatible; its 33 instruction tags and 37 runtime core tags do not change.

## Current implementation status

Reconciled on 2026-10-20 JST against [M2 capabilities and evidence](../milestone-2-capabilities.md). This RFC's scope is implemented, not stabilized. Read the original plan, decision-time statements and dated checkpoints below as rollout history. The guide records current native/SDK/platform work that those checkpoints treated as separate deliveries. Its review correction explicitly resolves the discovered RFC 0012 logical-debugger evidence gap; this reconciliation changes no normative requirement or non-goal of this RFC.

## Summary

Add portable schema 2 for the RFC 0007/0008/0009 ownership program. Keep schema 1 and its Copy encoding unchanged. Store explicit backend-neutral ownership declarations, places and typed CFG operations, then independently reconstruct every proof and failure-flow plan when loading. Never serialize live values or runtime authority. This is an experimental artifact contract, not stable serialized MIR or a native/component ABI.

## Motivation

At `9d9d2ac`, source VM execution supports owning Bytes/Text, nested collections, shared/weak and lexical arena values, while `lang-bytecode::unit` accepts only schema 1 and the Copy `VerifiedUnit`. Ownership `BodyTable`, its loan/region proofs and failure elaboration exist independently of that format. Treating the Rust enums as bytes, or extending schema 1 tags in place, would change an accepted compatibility boundary. These are repository observations verified 2026-10-18 JST; no performance result is inferred.

## Goals

- Round-trip the accepted finite ownership type/operation profile and all original source spans.
- Reject untrusted malformed or semantically invalid input before host binding or guest effects.
- Share the independently verified semantic contract with direct VM execution and native AOT.
- Specify compatibility, numeric tags, field order, deterministic encoding and resource limits before implementation.

## Non-goals

Live heap/session snapshots, source embedding as a substitute for IR, native machine code, proof certificates, serialized allocator callbacks, stable MIR/native ABI, new source syntax, general generics, component/package formats, compression and new filesystem/network authority are excluded. Schema 2 does not enable unsupported native operations or indirect REPL previews.

## Guide-level design

An owning script such as [the Bytes fixture](../../examples/scripts/owned_bytes.lang) can eventually be compiled once and loaded without reparsing its source. Embedded source snapshots supply diagnostics only; their paths grant no file access. Loading verifies the entire unit. Executing it still requires explicit allocator and host-import authority, fresh runtime budgets and a supported entry/result contract.

Copy compilation continues to emit schema 1. Ownership compilation emits schema 2 only after A02/A03 implement this contract. An old loader rejects version 2 explicitly; a new general loader dispatches to the version's verifier. Existing schema-1-only APIs may retain that narrower contract. No loader retries an invalid version-2 payload as version 1 or as source. Rebuild from source to migrate; changing only the header is invalid.

A03 adds `lang compile [--source-root DIR] --output ARTIFACT SOURCE` and `lang run --artifact [--fuel N] [--host-calls N] [--frames N] [--cells N] ARTIFACT`. Compilation requires the normal script entry and publishes output only after validation. The existing `lang run SOURCE` remains source execution; artifact loading is explicit and never follows embedded paths. Library artifacts without an entry remain an embedding API feature. These commands are specified here but are not implemented by A01.

Output protection correction accepted under RFC 0001's experimental delegation (2026-10-28 JST): compilation must reject an output path resolving to **any loaded source file**, including transitive modules and path aliases, before publication. Protecting only the entry file allowed a valid compile to destroy an imported module. The file-analysis loader's canonical identities determine this check; artifact-supplied display paths convey no authority. Other existing artifact outputs retain atomic replacement. Callers targeting a source input must select a different output; source syntax, artifact bytes and ABI are unchanged. Concurrent hostile filesystem replacement is outside this CLI check's guarantee. `tests/artifact_cli.rs` covers root/transitive inputs, aliases, preserved source bytes and successful artifact replacement.

## Reference semantics

### Envelope and primitive encoding

Use the existing 16-byte envelope: eight bytes `LANGBC\0\0`, little-endian `u16` version **2**, `u16` flags **0**, and `u32` payload byte length. Reject unknown versions/flags, truncation, length mismatch and trailing bytes. No extension sections are silently skipped.

All counts, lengths, indices, offsets and IDs are little-endian `u32`; check conversions to/from host `usize`. Signed literals use little-endian two's-complement `i64`. Tags and booleans are `u8`; booleans must be 0 or 1. A vector is its count followed by elements. A string is its byte length followed by exact UTF-8 bytes, without a terminator. Inherit schema 1's name/source/path validity rules. Source paths are normalized relative display paths with `/`, never filesystem authority.

`Span` is three `u32` fields: source index, start byte offset, end byte offset. Unlike schema 1's optional instruction spans, ownership spans are mandatory, matching the existing raw ownership contract; validate ranges and UTF-8 boundaries. `TypeUse` is a type followed by its span. An optional ID is tag 0 for absent, or tag 1 followed by its `u32` ID; reject other tags.

There are no Rust discriminants, pointers, platform layouts, native padding, registry addresses or implicit operands in the format. Every numeric assignment below is normative independently of Rust declaration order.

### Payload and table identity

Payload fields appear in this order:

1. Sources: vector of `(path string, source-text string)`.
2. Nominal definitions: vector of `(name, public bool, span, kind)`. Kind 0 is a record with a field vector; each field is `(name, span, TypeUse)`. Kind 1 is an enum with a case vector; each case is `(name, span, payload TypeUse vector)`.
3. Constructed types: vector of `(constructor, span)`, using the tags below.
4. Imports: vector of `(name, span, parameter TypeUse vector, result TypeUse)`. Retain the currently accepted `(i64) -> i64` trusted host-import signature; this adds no C-FFI import representation.
5. Function declarations: vector of `(name, public bool, span, signature span, parameter local-ID vector, result TypeUse, local vector)`. Each local is `(name, span, local-kind tag, TypeUse)`; local kinds 0/1/2/3 mean parameter/mutable/pattern/arena-owner.
6. Function places: one entry per declaration, each `(value TypeUse vector, place vector)`. A place is `(origin register, root, projection vector, span)`. Root tag 0/1 selects local/value ID. Projection tag 0 means dereference, or 1 followed by field ordinal; each projection is followed by its span.
7. Function bodies: one entry per declaration, each `(loan-count u32, prepared-place-count u32, block vector)`. A block is `(parameter-register vector, instruction vector, terminator, block span)`; block zero is the entry block. Counts declare logical identities, not active runtime registrations.
8. Optional entry function ID. If present, it must have zero parameters and result i64. Absence permits a verified library unit but cannot execute as a script. Execution uses the checked ID, not a name supplied by an artifact.

Preserve every vector's order and ID meaning. Require unique names/paths where the shared verifier requires them; constructed arguments precede their constructed parents. Preserve field/case/block/instruction/initialization order. Unlike schema 1, schema 2 does **not** sort global tables or renumber equivalent constructed types: an ordered raw table has exactly one encoding, but equivalent differently ordered programs need not have identical bytes. Encode/decode/re-encode must be byte-identical. Never serialize unordered host-map iteration.

### Types

| Type tag | Payload |
| --- | --- |
| 0 | i64; none |
| 1 | bool; none |
| 2 | nominal definition ID |
| 3 | constructed type ID |

| Constructor tag | Payload |
| --- | --- |
| 0 | builtin tag |
| 1 | Option: type |
| 2 | Result: success type, error type |
| 3 | Vec: element type |
| 4 | Map: key type, value type |
| 5 | shared: payload type |
| 6 | weak: payload type |
| 7 | arena reference: payload type |
| 8 | ordinary loan: loan-kind tag, target type |
| 9 | logical arena owner: function ID, local ID |

Builtin tags 0–9 are respectively Unit, Text, Bytes, Allocator, AllocError, BoundsError, BufferError, Utf8Error, OwnershipError and ArenaError. Loan-kind tags 0/1 mean shared/exclusive. These describe types and lexical identities only; the Allocator or arena-owner type never carries an allocator instance or region authority. Retain RFC 0007 storage, Map-key, recursion, signature, loan and arena escape restrictions. Concrete core type arguments retain spans and are independently checked.

### Instructions and cleanup ranges

Each instruction is `(result-register vector, opcode u8, operands, span)`. All IDs below are `u32`; `vector` means a count-prefixed vector. The verifier checks result count/types rather than trusting the serialized result list.

| Opcode | Meaning | Operands in order |
| --- | --- | --- |
| 0 | I64 | i64 literal |
| 1 | Bool | bool |
| 2 | Unit | none |
| 3 | Add | left register, right register |
| 4 | LessThan | left register, right register |
| 5 | EqualI64 | left register, right register |
| 6 | Copy | value register |
| 7 | Move | value register |
| 8 | CopyPlace | place ID |
| 9 | MovePlace | place ID |
| 10 | Initialize | local ID, value register |
| 11 | Replace | place ID, value register |
| 12 | PreparePlace | prepared-place ID, place ID |
| 13 | ReplacePrepared | prepared-place ID, value register |
| 14 | DiscardPrepared | prepared-place ID |
| 15 | DropValue | value register |
| 16 | DropLocal | local ID |
| 17 | DropLocalIfInitialized | local ID |
| 18 | DropValueIfAvailable | value register |
| 19 | Cleanup | cleanup-step vector |
| 20 | Borrow | loan ID, loan-kind tag, place ID |
| 21 | EndBorrow | loan ID, end-mode tag |
| 22 | ArenaEnter | owner local ID, allocator register |
| 23 | ArenaExit | owner local ID |
| 24 | Call | function ID, argument-register vector |
| 25 | Import | import ID, argument-register vector |
| 26 | CoreCall | core-operation tag, type-argument TypeUse vector, argument-register vector, optional result-loan ID |
| 27 | Record | definition ID, field-register vector, initialization-order ordinal vector |
| 28 | Variant | case ordinal, payload-register vector |
| 29 | IsCase | value register, case ordinal |
| 30 | CopyCaseField | value register, case ordinal, field ordinal, guard register |
| 31 | BorrowCaseField | loan ID, value register, case ordinal, field ordinal, guard register |
| 32 | UnpackVariant | value register, case ordinal, guard register |

End-mode tags 0/1 mean required/if-active. A cleanup step is `(target tag, target ID, span)`; target tags 0/1/2/3 select local/value/loan/arena-owner local. A cleanup range expresses obligations; actual destruction still follows RFC 0008's dynamic reverse initialization order, with registrations ended before their owners. Prepared destinations, record initialization permutations, whole-sum extraction and provisional core loans retain their existing independently verified semantics. No serialized list can override cleanup priority or claim an owner is initialized.

Core-operation tags are fixed by this table, including RFC 0009 extraction:

| Tag | Operation | Tag | Operation | Tag | Operation |
| --- | --- | --- | --- | --- | --- |
| 0 | Allocator | 13 | VecNew | 26 | SharedRead |
| 1 | TextFromUtf8 | 14 | VecLen | 27 | SharedReplace |
| 2 | TextClone | 15 | VecPush | 28 | WeakNew |
| 3 | TextLen | 16 | VecPop | 29 | WeakClone |
| 4 | TextBytes | 17 | VecGet | 30 | WeakUpgrade |
| 5 | TextEqual | 18 | VecGetMut | 31 | ArenaNew |
| 6 | TextConcat | 19 | MapNew | 32 | ArenaRead |
| 7 | BytesNew | 20 | MapLen | 33 | ArenaReplace |
| 8 | BytesClone | 21 | MapInsert | 34 | OptionUnwrap |
| 9 | BytesLen | 22 | MapGet | 35 | ResultUnwrap |
| 10 | BytesGet | 23 | MapRemove | 36 | ResultUnwrapErr |
| 11 | BytesPush | 24 | SharedNew | — | — |
| 12 | BytesSet | 25 | SharedClone | — | — |

A terminator is `(tag u8, operands, span)`: 0 return(value register), 1 jump(target), 2 branch(condition register, then target, else target), 3 unreachable(no operands). A target is `(block ID, argument-register vector)`. Unreachable is a contained invariant failure if reached, not permission to omit remaining cleanup. Unknown tags at any level are errors.

### Verification, effects and authority

Decode bounded raw tables first. Before returning an executable artifact, independently validate ownership types/declarations/places and typed CFG/SSA, move obligations, loan activity, transitive reference provenance/access, region identities and recursive call effects, and complete failure/return cleanup. Regenerate failure CFG, layout/slot plans, graph inspection and retirement preparation from those checked contracts. Never persist or trust proof booleans, dominators, hashes, liveness/region summaries, executable host pools or failure-flow certificates. A decoder that returns only an unverified raw table must make that nonexecutable state explicit in its type/API.

Embedded source text is not reparsed as proof. Type IDs, spans, declarations, core signatures, initialization permutations, cleanup ranges, loan endings and arena-owner symbols all require their own validation. Forging an ownership operation must fail even if source snapshots contain a different valid program. Import bindings, allocator authority and dynamic execution/host limits remain explicit instantiation inputs; successful validation alone grants none. No guest allocation, provider call, host callback or filesystem read occurs during decoding/verification. Host allocations for bounded compiler data are permitted.

Arithmetic, result matching, borrowing, panic codes, cleanup order, interruption and poison remain RFC 0007/0008/0009 behavior. No new constant-evaluation, scheduling, atomics, callback or data-race rule is introduced.

### Resource limits

Retain schema 1's default 64 MiB artifact cap, 4 MiB/source, 16 MiB total source, 256 sources, 4096 definitions/functions/imports each, 256 fields/cases/parameters each, type depth 64, 4096 value cells, and 1,000,000 registers/places/blocks/instructions/loan IDs/prepared IDs each. Add at most 65,536 constructed types. Decoder/verification requested allocations default to **128 MiB**, cumulative elements to **8,000,000**, and charged work to **10,000,000** across the complete load. Apply the shared verifier's additional explicit region/dataflow caps as well.

Check remaining-input minimums, count limits, cumulative elements, checked arithmetic, requested bytes and work **before** reservation/traversal. Keep a load-wide ledger across decoding and all proof stages; no phase reset may grant another full allowance. Charge retained input copies, strings/vectors, type/CFG/provenance matrices, iterative worklists, repeated fixed-point visits and derived failure data. Caller-owned input bytes are excluded; requested allocation bytes count even if later freed, rather than representing resident memory. Encoding has its own output/temporary-allocation/work bounds and validates its raw input before publishing bytes. Check failure and cancellation paths for release of partial host buffers. Reject exhaustion with a structured error, never a partially executable unit.

Charge each decoded/encoded scalar field or tag, each vector element/operand visited, and each UTF-8 validation chunk of at most 1 KiB, in addition to all shared-verifier visits and repeated dataflow-set members. Encoding size and write passes both consume work. Publish any additional charged operations and exercise exact small-budget boundaries; a byte/count cap does not replace repeated-work accounting.

These are configurable admission budgets, not throughput or memory-usage measurements. If cumulative admission cannot be integrated through existing APIs, expose that prerequisite and re-estimate A02 before implementing the loader; do not silently use per-stage fresh budgets.

## Execution modes

Direct source VM and loaded schema 2 use the same verified ownership/failure contract and supported operation profile. Native AOT consumes that contract independently of this wire encoding; object layouts and machine conventions remain separate N01–N05 work. Validate every reachable function/import and reject unsupported execution before effects. JIT, native REPL and WebAssembly/component execution gain no implementation claim. Runtime service preparation remains bounded and occurs before guest entry as already required.

## ABI and compatibility

Affected domains: `bytecode` and `tooling` (explicit compile/load commands and structured artifact errors). Source syntax/typing and native/component/package ABI are unchanged. Schema 1 byte fixtures, sorting, validation and encoding remain byte-identical. Version 2 has its own ordered-table rules and is never schema-1-compatible even for a scalar-only body. Future incompatible tags/field order/semantics require an explicit schema decision and rejection/migration fixtures; do not append an unnegotiated opcode under version 2. Pre-alpha acceptance is not stabilization. Rebuild artifacts from the matching source/toolchain; no binary translator is required.

## Diagnostics and tooling

Report version/flags/tag/truncation/overflow/UTF-8/budget/validation failures as structured artifact errors with byte offsets and, after bounded span validation, original source locations where available. Do not invent new source `SRC` codes for malformed binary input or trust an invalid span to render a diagnostic. Preserve complete source snapshots for runtime stacks and later native debug metadata. Parser, formatter, LSP and REPL source grammar are unchanged. CLI success is published only after validation; filesystem output failure remains a host error.

## Performance and cost model

The format stores compiler metadata and embedded sources, not heap snapshots. Loading allocates bounded tables and reruns expensive dataflow/region proofs; it is not a trusted proof cache. Preserving table order avoids canonical graph renumbering. Bytewise reproducibility applies to the same ordered tables, not all equivalent source programs. No startup, size, memory or runtime advantage is claimed; measure A03 and the existing benchmark rows before making one.

## Security and trust boundaries

Treat every artifact as untrusted regardless of origin or extension. Lengths, names, debug paths, logical owners and valid CRC/hash-like data cannot grant capabilities or replace proofs. There is no executable/native pointer payload, native plugin loading or implicit process/environment/network access. Decoder/verification limits do not sandbox trusted host callbacks. Preserve the existing allocation and C-boundary contracts; this RFC introduces no `unsafe` requirement.

## Alternatives

- **No change:** keep ownership source-only; this leaves the accepted artifact/conformance outcome unfinished.
- **Library/tooling or embedded-source replay:** recompilation can preserve source behavior but is not an ownership IR round-trip and cannot independently validate forged IR. Debug text stays informational.
- **Extend schema 1 in place:** reuse more codec code but make old version checks misleading and overload incompatible type/operation tags. Choose an explicit version instead.
- **Serialize elaborated bytecode and proof certificates:** could reduce some recomputation, but expands the trusted format to derived cleanup/layout/proof internals and still needs independent checking. Preserve only raw semantic tables.
- **Simpler snapshot of Rust enums/serde layout:** less initial specification, but couples disk data to implementation layout and implicit enum order. Explicit finite fields/tags provide a smaller compatibility boundary.

## Implementation plan

1. A01: review and accept this decision with the bilingual document; acceptance enables design-dependent work only.
2. A02: first audit/prototype cumulative budget admission; expose independent prerequisites before coding. Implement bounded explicit type/place/body codec, malformed-input coverage and a fuzz target. Keep any raw decode result nonexecutable.
3. A03: independently reverify and instantiate loaded units; integrate compile/load/script execution and demonstrate the first owned Bytes artifact before additional artifact infrastructure. Compare VM results, errors, cleanup/loan behavior and source spans against direct source execution.
4. N01 and later native/platform rows: consume the shared verified contract and supply their existing differential, ABI, platform, performance and debug evidence. Update capability docs only for demonstrated behavior.

## Test plan

Freeze independent schema-1 bytes and old/new-loader version rejection. Add hand-authored schema-2 scalar/Bytes vectors with exact header/tags/field order; round-trip all 33 opcodes, 37 core operations and type constructors, including inactive recursive shapes and CFG arguments. Verify decode/re-encode byte identity while preserving table and record initialization order.

Reject unknown tags, invalid booleans, count/offset overflow, truncation at each boundary, invalid UTF-8/spans, duplicate identities, forward constructed references, bad result lists and entry signatures. Mutate valid fixtures to forge moves, loan endings/custody, prepared destinations, cleanup obligations, guards, cross-arena stores/returns and recursive call effects; source text must not rescue them. Test exact load-wide budgets across phase boundaries and allocation-failure release, then fuzz bounded decode/verification without executing arbitrary host imports.

A03 compares direct/loaded source results and original-span failure/cleanup traces for Bytes/Text/Vec/Map/shared/arena, provider OOM, fuel and interruption. Native/macOS/Windows and final benchmark/debug acceptance remain their existing rows; a codec test cannot close them.

## Open questions

No unresolved blocker for the implemented format. The load-wide ledger, loaded/direct execution, source spans and cleanup are verified by A02/A03 and the completed [M2 audit](../milestone-2-capabilities.md). Native/platform and benchmark/debug gates are complete at their recorded revisions; those observations do not guarantee future performance. The 2026-10-28 output-protection correction above requires a different destination when it aliases any loaded source. Revise the RFC before changing the format contract.

## Decision

**Accepted under experimental delegation**, 2026-10-18 JST. The review finds the design consistent with RFC 0001 and the accepted ownership/cleanup contracts: schema 1 remains unchanged, explicit finite tags cover the existing 33 ownership opcodes and 37 core operations, runtime authority is absent, and independent validation remains mandatory. Preserving ordered raw tables avoids a new graph canonicalization requirement. Bilingual headings, links, tag coverage and repository consistency were checked. This completes A01's decision only; it is not an implemented codec, loader, native backend or exit gate. The cumulative-budget and execution-equivalence assumptions listed above remain A02/A03 obligations. Revise this RFC explicitly before implementing a changed contract.

## Sources

Implementation recorded 2026-10-18 JST (A02/A03): schema 2, independent verification, explicit compile/load commands and checked-entry VM dispatch are implemented. The first loaded Bytes fixture returns `0`; ten service fixtures compare all provider refusals and fuel prefixes plus interruption/reuse, and CLI tests cover both versions, metadata/entry rejection, input/output bounds and deleted multi-file snapshots. Status `implemented` covers this format/current VM contract, not stabilization or the native/platform exit gates.

- Project decisions: [RFC 0001 delegation/compatibility](0001-project-charter.md) and [RFC 0006 schema 1](0006-source-frontend-and-portable-programs.md), updated 2026-10-17 JST; [RFC 0008 cleanup/authority](0008-cleanup-allocators-and-c-boundaries.md), updated 2026-10-17 JST; [RFC 0007](0007-source-ownership-and-core-values.md) and [RFC 0009](0009-whole-sum-ownership-extraction.md), updated 2026-10-18 JST.
- Design-stage evidence: [schema-1 codec](../../crates/bytecode/src/unit/codec.rs), [ownership types](../../crates/mir/src/unit/ownership.rs), [places](../../crates/mir/src/unit/ownership/places.rs), [bodies](../../crates/mir/src/unit/ownership/bodies.rs), [core signatures](../../crates/mir/src/unit/ownership/core/mod.rs), [mandatory region/failure proof](../../crates/mir/src/unit/ownership/failure.rs) and [source analysis pipeline](../../crates/semantics/src/analysis.rs), verified at `9d9d2ac` on 2026-10-18 JST. Numeric wire assignments originate in this project decision, not an external standard.
- Current implementation: [schema-2 codec](../../crates/bytecode/src/ownership.rs), [compile/load dispatch](../../src/artifact.rs), [CLI](../../src/main.rs), [checked-entry execution](../../crates/vm/src/unit/owned.rs), [CLI tests](../../tests/artifact_cli.rs) and [differential execution](../../tests/support/artifact_execution.rs), updated/verified 2026-10-18 JST.
