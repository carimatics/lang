<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0017-c-aware-ownership-artifacts.md -->

# RFC 0017: C-aware ownership artifacts

[日本語版](../ja/rfcs/0017-c-aware-ownership-artifacts.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-P02
- Supersedes: none; extends RFCs 0011/0014
- Superseded by: none

## Current implementation status

Reconciled on 2026-10-20 JST against [M2 capabilities and evidence](../milestone-2-capabilities.md). This RFC's scope is implemented, not stabilized. Read the original plan, decision-time statements and dated checkpoints below as rollout history. The guide records current native/SDK/platform work that those checkpoints treated as separate deliveries. Its review correction explicitly resolves the discovered RFC 0012 logical-debugger evidence gap; this reconciliation changes no normative requirement or non-goal of this RFC.

## Summary

Add ownership schema **4**, preserving the existing Host/C import and Guest/CExport function identities through checked MIR encoding/decoding. Schema 1 Copy, schema 2 ownership and schema 3 explicit-panic bytes remain unchanged. Decoding supplies data and no FFI authority.

Review outcome: **accepted under experimental delegation**, before implementation. Existing declarations already verify these identities and finite C signatures; the missing transport is a format extension, not new source or C-call semantics. Structural/native re-preparation prototype 5324c64 provides the packaging motivation. Codec implementation and loaded-C execution evidence remain unverified at acceptance. This decision does not complete P02 or define its native distribution container.

Implementation evidence (2026-10-20 JST): the codec selects schema 4 for C declarations, preserves explicit ABI bytes and Panic, and retains legacy golden bytes. Loaded source-derived C calls execute through the existing explicit VM/native context APIs; CLI loading after source deletion grants no C authority. Wire corruption, truncation, finite aggregate signatures and cumulative limits are tested. See the paired architecture document for validation and the delivery board for P02 distribution work.

## Motivation

The current writer refuses any C import or CExport before publishing an artifact. The reader reconstructs Host/Guest defaults for schemas 2/3. Thus production native startup cannot recover a complete C-aware semantic contract from those formats without source analysis or silently erasing ABI identities. P02 needs to ship verified IR with its runtime/context dependencies. Reusing the shared MIR codec avoids inventing a second representation of language semantics.

## Goals

- Preserve complete C-aware ownership declarations and their existing spans/types/operations.
- Rerun the independent declaration, body, move, loan, region and failure verification pipeline.
- Preserve all existing format bytes and explicit binding/capability requirements.

## Non-goals

- Native code, addresses, contexts, allocator callbacks, grants or C bindings in bytecode.
- A native package/container, SDK, code authentication, dynamic library loader or Rust-layout serialization.
- New source syntax, C ABI profile, execution policy, panic behavior or guest operations.

## Guide-level design

An import whose verified identity is `ImportAbi::C` now round-trips as C, and a function marked `FunctionAbi::CExport` round-trips as CExport. Encoding selects:

| Ownership unit | Schema |
| --- | --- |
| Any C import or CExport, including unused declarations | 4 |
| Otherwise, explicit Panic operation | 3 |
| Otherwise | 2 |

Copy artifacts continue using schema 1. A decoded C-aware artifact can be supplied to the existing explicit C bindings/context execution interfaces. A regular CLI invocation gains no C bindings or native-loading authority. Compiler/native-package consumers can reconstruct semantic plans from decoded data instead of parsing source snapshots.

## Reference semantics

Source resolution, types, evaluation order, ownership, borrowing, destruction, allocation, cancellation, concurrency and constant evaluation are unchanged. The new format carries identities already present in verified MIR. C signature/name/visibility restrictions remain the existing finite profile-1 rules.

Schema 4 reuses the ownership envelope, little-endian scalars, tables, type/place/operation tags, optional entry ID and source snapshots. All existing ownership operation encodings, including schema 3's Panic tag/payload, keep their meanings.

Two mandatory u8 fields are added only for schema 4:

| Table field position | Values |
| --- | --- |
| Import ABI, immediately after import name and before its span | 0 = Host; 1 = C |
| Function ABI, immediately after name/public and before its span | 0 = Guest; 1 = CExport |

Every import/function row contains the field, including default identities. Values 2–255 are reserved and rejected with a structured unknown-tag error at the offending byte offset; never default them to Host/Guest. Schema 2/3 readers continue supplying Host/Guest without consuming an ABI byte.

Encoding selects 4 before sizing when any declaration uses a C identity; otherwise RFC 0011/0014 selection is unchanged. Schema 4 admits the existing Panic encoding whether or not the particular unit contains one. Parsing, sizing and writing agree on the same selected version. No partial artifact is published on verification, sizing, allocation or encoding failure.

Decoding validates the entire raw unit before exposing its checked form. Setting a C bit cannot make owning/borrowed/allocator types valid C arguments, bypass public-export/name requirements, relax arena checks or grant an address. Existing execution admission still applies after decoding. Unsupported/unbound foreign execution is not resolved by the codec.

## Execution modes

VM/native hosts may consume schema 4 only through their existing explicit execution/binding interfaces. Source and decoded-unit semantics must agree, including C context identity, original allocator, quotas, faults and cleanup. Native startup remains a separate P02 implementation. Optional JIT, components, embedded callbacks and constant execution gain no new implicit authority.

## ABI and compatibility

No source, native C/component ABI, native private packet or package metadata layout changes. Schema 4 is a new serialized MIR version. Older consumers must reject it as unsupported rather than interpret it as 2/3. Existing schema-1/2/3 golden bytes and decoding remain unchanged.

The encoder upgrades C-aware ownership output to 4; non-C output stays 2/3. There is no lossy downgrade to Host/Guest. Consumers needing C-aware data upgrade their codec. No source migration or edition change is needed. Further ABI kinds require a separate format decision.

## Diagnostics and tooling

Retain structured codec offsets/table/tag diagnostics and verifier source spans. Unknown ABI tags are distinct from invalid C signatures discovered by verification. No parser recovery, formatter, LSP or debugger protocol changes. Document schema selection, compatibility and the absence of serialized grants in the artifact documentation.

## Performance and cost model

Add one byte per import/function only in schema 4. Version selection scans bounded declaration tables. Read/sizing/write work and retained tables share existing cumulative codec/verification budgets and byte caps. No new guest allocation, retain, dispatch, source reparse or backend compilation is needed by decoding. No runtime performance promise is introduced.

## Security and trust boundaries

Artifacts remain untrusted. Validate version, lengths, tags, UTF-8, spans, indices, signatures and complete ownership/region/failure semantics before execution. The C identity describes intent, not authorization. No native pointer/code/allocator/capability may be reconstructed from a byte field. Native code/SDK matching and trust are separate package/linker responsibilities. Preserve bounded failure and no partial publication.

## Alternatives

- Keep rejecting C units: prevents the required source-free semantic transport.
- Erase C identities into Host/Guest: loses signature checks and can change dispatch; rejected.
- Add optional bytes to schema 2/3: breaks existing parsing/golden compatibility; rejected.
- Reuse schema 3: already assigned to explicit panic; rejected.
- Invent a separate native semantic IR: duplicates the shared contract and verifier; reuse the ownership format instead.
- Serialize native runtime objects/pointers: violates the format/trust boundaries; rejected.

## Implementation plan

1. Add schema-4 selection, explicit ABI tags and version-aware reader/writer handling; preserve legacy defaults and Panic.
2. Dispatch schema 4 in the shared artifact facade while preserving existing execution admission.
3. Add round-trip, malformed-input, budget, legacy and loaded-C execution evidence; update paired documentation.
4. Continue P02 separately with recorded native metadata, runtime/C-context distribution and isolated link/run. No package completion is implied here.

## Test plan

Preserve schema-1/2/3 golden fixtures. Round-trip mixed Host/C imports, Guest/CExport functions, finite aggregate signatures, all ordinary ownership operations and explicit Panic with exact ABI identities/spans/IDs. Reject unknown ABI tags, each truncated prefix, invalid signatures/visibility/names/indices, trailing data and work/allocation/element/byte-limit exhaustion without publication.

Execute source-derived decoded C fixtures through existing explicit VM/native bindings as applicable; check context/allocator identity, outputs, original failure/cleanup and release. Test absence of implicit bindings/provider effects, source-independent decoding, deterministic re-encoding and old-version rejection. Run the repository baseline. Native container/SDK/debug/performance tests belong to their remaining roadmap deliveries.

## Open questions

No blocking format question. Native distribution records, startup entry and SDK matching require their own P02 design and are not defined by these two ABI bytes.

## Sources

Project format decisions above; primary local sources verified **2026-10-20 JST**:

- [RFC 0011](0011-ownership-artifacts.md), updated 2026-10-19: ownership schema 2 and independent verification.
- [RFC 0014](0014-explicit-source-panic.md), updated 2026-10-19: schema 3 and explicit Panic.
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md), updated 2026-10-19: C profile, explicit grants and cleanup.
- [RFC 0012](0012-native-ownership-continuations.md) and [RFC 0015](0015-c-native-object-bridge.md), updated 2026-10-20: native/private C boundary and object binding.
- [Current writer](../../crates/bytecode/src/ownership/writer.rs), [reader](../../crates/bytecode/src/ownership/reader.rs), [declaration verification](../../crates/mir/src/unit/ownership/declarations.rs), [detached record prototype](../../crates/codegen-clif/src/owned/recorded.rs); living sources without publication dates, verified 2026-10-20 JST.
