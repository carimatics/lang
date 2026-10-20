<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0018-native-compiler-records.md -->

# RFC 0018: Native compiler records

[日本語版](../ja/rfcs/0018-native-compiler-records.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-P05, M2-P06, M2-P07, M2-P02
- Supersedes: none; extends RFCs 0011/0014/0015/0017
- Superseded by: none

## Current implementation status

Reconciled on 2026-10-20 JST against [M2 capabilities and evidence](../milestone-2-capabilities.md). This RFC's scope is implemented, not stabilized. Read the original plan, decision-time statements and dated checkpoints below as rollout history. The guide records current native/SDK/platform work that those checkpoints treated as separate deliveries. Its review correction explicitly resolves the discovered RFC 0012 logical-debugger evidence gap; this reconciliation changes no normative requirement or non-goal of this RFC.

## Summary

Add a bounded, versioned transport for native object bytes and compiler metadata, paired by SHA-256 with the existing portable semantic artifact. Recover runtime plans from independently decoded semantics without source analysis, ISA lookup, code emission or JIT construction.

Review outcome: **accepted under experimental delegation, 2026-10-20 JST**, before wire implementation. P03's detached records already rebuild native plans. A source-derived managed Text/Vec/shared/arena fixture now executes its linked object from decoded semantics after producer teardown (focused prototype: 2.41s); the consumer does no source analysis or backend emission. This supports the separation, not distribution completion or performance claims. Copy startup, persisted records, general runtime startup and C/SDK integration remain unverified.

Implementation checkpoint (2026-10-20 JST): bounded records, all target/scope tags, canonical golden bytes, checksums/limits and checked retained-image Copy calls are implemented. Serialized-file Copy/managed ownership/explicit-panic linked fixtures execute after producer teardown without source analysis or code emission. This completes record transport and preparation evidence; the concrete owned bootstrap, C/SDK integration and installation remain the separately tracked P05/P06/P07/P02 outcomes.

Runtime/C integration checkpoint (2026-10-20 JST): RFC 0019 supplies the ordinary production entry; `CObjectArtifact::into_recorded` / `RecordedObject::prepare_c` now detach/restore C interface/bridge metadata. Real C callers reload saved files after producer teardown without source analysis, backend emission or JIT, retaining original context/allocator/grants/cleanup. Both-target scope/digest/budget checks, all managed families, aggregate/256-parameter and finite Copy cases, owner recursion/callback/panic and fuel/allocation-failure prefixes are verified. SDK assembly P07 and isolated both-host install P02 remain open.

## Motivation

RecordedObject currently owns native bytes/metadata but only within one Rust process. The standalone conformance driver still recompiles matching source to reconstruct metadata. An installed runtime needs compiler output as data; source reanalysis and re-emission would retain compiler/startup dependencies. Portable schemas 1/2/3/4 already carry semantic data and original spans. Reuse that contract, with a separate native record rather than a second semantic IR.

## Goals

- Preserve original object bytes, target, private compilation scope, function identities/symbols/code sizes and source ranges.
- Detect accidental semantic/object substitution before preparing plans.
- Bound decoding/encoding work, allocations, elements and bytes; preserve the unsafe exact-code binding contract.
- Cover Copy and ownership records, including C scopes needed by subsequent P06 integration.

## Non-goals

- New source semantics, opcodes, public C profile, native packet/helper ABI or component format.
- Native authentication, sandboxing native code, safe automatic loading/execution, dynamic plugin loading or stable SDK ABI.
- Completing C-context/SDK distribution, clean-install target proof, debugger or performance gates by codec implementation alone.

## Guide-level design

The compiler retains an ordinary semantic artifact plus a native record. A trusted host decodes the semantic artifact independently, checks the native record against those exact semantic bytes, rebuilds target layouts/plans, links the stored object and explicitly binds its retained matching image. Copy invocation reuses the existing canonical cell-buffer ABI; ownership uses existing checked continuations/runtime services.

The native record is data. Decode must not resolve symbols, load libraries, allocate guest regions, create a C context or grant capabilities. A host must still trust the native code/toolchain and uphold the full unsafe matching-image contract. Checksums detect corruption/mix-ups, not malicious replacement.

## Reference semantics

Version 1 is little-endian. All lengths/counts are u32. There is no padding.

| Field | Encoding |
| --- | --- |
| Envelope | magic bytes LANGNAT followed by NUL (8 bytes), version u16 = 1, flags u16 = 0, payload length u32 |
| Target | u8: 0 macOS AArch64, 1 Windows x86-64 MSVC; other tags rejected |
| Kind | u8: 0 Copy, 1 ownership Function, 2 ownership Unit, 3 ownership CUnit, 4 ownership CObject |
| C object identity | exactly 32 C Artifact digest bytes only for kind 4 |
| Semantic identity | SHA-256 of the complete portable semantic artifact, 32 bytes |
| Object identity | SHA-256 of the exact object bytes, 32 bytes |
| Object | u32 byte length followed by exact bytes |
| Functions | u32 count followed by ordered function records |

Each function record is function ID u32, symbol (u32 UTF-8 byte length plus bytes), code size u32, source-range count u32 and that many ranges. Each range is five u32 fields: native start, native end, source ID, source start, source end. Code size is nonzero. Ranges satisfy start < end <= code size; source start <= source end. Function IDs are strictly increasing. Symbols are nonempty UTF-8 without NUL, at most 4,096 bytes. Invalid count/tag/length/UTF-8/range, unsupported version/flags, digest mismatch, truncation or trailing bytes are structured errors with byte offsets.

Independent semantic verification remains mandatory. Preparation validates expected function IDs/counts, canonical symbols, source identities/UTF-8 boundaries, target and private scope against that verified semantic domain. Structural checks do not prove native bytes implement those semantics. The trusted compiler/linker and unsafe binder retain that obligation.

A single cumulative MIR-style budget covers each complete codec operation: retained object/metadata buffers, decode/string/vector buffers, writer sizing/output passes and output reservation. Charge scalar fields/elements and each len/1024 + 1 chunk for hashed/copied/string byte sequences, including the empty/final chunk. Reject oversized lists before reserving them, using remaining bytes and minimum row sizes as well as configured limits. Default record input/output cap is 64 MiB; callers may tighten it. Limits also apply to externally supplied semantic bytes. Failure publishes no partial record.

## Execution modes

VM execution still decodes the existing portable artifact and ignores native records. Native startup uses decoded semantic data plus matching compiler records; it does not evaluate guest MIR as a fallback. Optional JIT and existing direct in-memory object APIs keep their behavior. C records preserve their private scope/digest, but P06 must separately reconstruct/validate C interface and bridge metadata and obtain explicit context/grants/bindings. No C authority comes from a tag.

## ABI and compatibility

Native record version 1 is experimental and separate from portable schema versions, C profile 1, bridge revision 1 and native helper revisions. Existing portable bytes and private calling conventions stay unchanged. Unknown versions fail closed; migration requires recompile/repackage with the matching compiler/SDK, with no source fallback or silent downgrade. Relocated image addresses and live runtime objects are never encoded. Tier-1 target tags do not make a cross-target object executable on the current host. Exact compiler/toolchain/image compatibility remains a caller requirement until the SDK distribution contract is defined.

## Diagnostics and tooling

Use structured error kinds and byte offsets, retaining resource-limit causes. Source locations survive in the semantic artifact and original native ranges. No parser, formatter, inference, LSP or source diagnostic change is needed. A record codec does not supply debugger evidence. Update paired documentation with the first persisted linked execution.

## Performance and cost model

The container adds two 32-byte digests, framing and original metadata; it stores one full object and does not compress it. Hashing and two writer traversals are linear in their input sizes. Runtime still pays independent semantic verification, layout/plan preparation and bounded runtime allocation. This removes required source analysis/backend emission from the startup path; no latency, size or memory improvement is claimed without measurement.

## Security and trust boundaries

All record data is untrusted until validated, but validated native code is still trusted code. Hashes are not signatures. Decoding cannot establish that an object/image is safe or matches source semantics. Native link/bind remains explicit and unsafe with exact compiler/toolchain/image and owner-lifetime invariants. Preserve capability restrictions for script/C/host calls and validate foreign pointers through their existing interfaces. No native pointer, resolver, allocator, instance, grant or live owner is serialized.

## Alternatives

- No change: retain source reanalysis/backend dependencies in installed startup.
- Library/tooling only: detached Rust records already help in-process use, but cannot cross the distribution boundary.
- A complete native package/SDK ABI now: couples record transport to installation, context policy and linker layout before their execution experiments; stage them in P06/P07 instead.
- A second native semantic IR or Rust-layout dump: duplicates the shared verifier or exposes unstable layouts/pointers; rejected.
- Simpler unchecked object/metadata tuple: cannot reliably detect accidental pairing or bound malformed metadata; keep explicit versioning/checks.

## Implementation plan

1. Encode/decode bounded Copy/ownership native records and bind them to exact portable artifact bytes.
2. Rebuild checked metadata/plans and execute persisted linked fixtures after producer teardown, without source analysis/backend emission; cover Copy and ownership.
3. Integrate the general native runtime bootstrap under P05, C context under P06, SDK assembly under P07 and both-host isolated installation under P02. None is completed by a codec alone.

## Test plan

Freeze explicit envelope/tag/layout assignments; round-trip objects and complete metadata. Preserve old portable golden bytes. Reject wrong versions/flags/tags, every truncated prefix, repaired lengths, noncanonical strings/counts/ranges, digest substitutions, trailing data and exact byte/work/allocation/element exhaustion. Validate mismatched semantic domains before binding. Exercise actual linked source-derived Copy and managed ownership execution from decoded semantics/records, retaining original results, spans, cleanup/control/allocator behavior and image ownership. Keep both target formats/hosts and later C/startup/distribution gates visible. Run focused checks then the repository baseline.

## Open questions

No blocking record-format question. General runtime entry and SDK distribution may need additional decisions; revise their RFC explicitly before adding wire fields or a public startup ABI.

## Sources

- [C record restoration](../../crates/codegen-clif/src/owned/c_objects.rs) and [persisted execution/refusal tests](../../tests/support/c_object_records.rs), implemented/verified 2026-10-20 JST.

- [Detached records](../../crates/codegen-clif/src/owned/recorded.rs), implementation 5324c64, 2026-10-20 JST.
- [Linked execution prototype](../../tests/support/native_linked.rs), inspected/executed 2026-10-20 JST; 2.41s is one focused test duration.
- [Portable artifact contract](0017-c-aware-ownership-artifacts.md), updated 2026-10-20.
- [Native C object trust/ABI](0015-c-native-object-bridge.md), updated 2026-10-20.
- [Delivery board](../milestone-2.md), updated 2026-10-20 JST. These are project evidence/decisions; no external performance claim is used.
