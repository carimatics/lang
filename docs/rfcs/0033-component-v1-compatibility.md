<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0033-component-v1-compatibility.md -->

# RFC 0033: Component C v1 compatibility and migration

[日本語版](../ja/rfcs/0033-component-v1-compatibility.md)

- Status: accepted
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-27
- Last updated: 2026-10-27
- Tracking issue: M3-B02
- Supersedes: None; refines RFC 0027's stabilization gate
- Superseded by: None

## Summary

Freeze the admitted component C v1 interface after B01's actual macOS AArch64 and Windows x86-64 conformance. A previously compiled C client continues to use `lang_component_v1_get_api(1, ...)` with the original exact-size table. Old generated bindings for the same admitted contract must keep their frame, ownership and failure behavior. Stabilization is conditional on executable previous-client migration tests on both hosts.

## Motivation

Layout vectors alone cannot detect a runtime change that breaks an older caller's ownership or error handling. Source `40158d6` first published the complete query; the reference header and generator remain unchanged through B01 source `19a29c0`. Retain that previous client's declarations independently of future generation and execute its calls against the current implementation.

## Goals

- Preserve all v1 layouts, calling conventions, constants, 32 signatures, query negotiation and documented lifetimes.
- Preserve old generated clients' canonical frames, signature validation, explicit authority, transfer commit and cleanup behavior for compatible contracts.
- Provide refusal and rebuild guidance across independently versioned formats.

## Non-goals

Stable source-language syntax, Rust/native-language ABI, private SDK linkage across builds, arbitrary native binary loading, new platforms, WebAssembly execution or a general native sandbox.

## Guide-level design

Keep an existing v1 header and generated client when updating the runtime on the same supported target. Compile/link them using the target's compatible C toolchain and query version 1. The complete table retains its exact size; zero reserved fields and correct version/size headers remain mandatory. Unknown versions and malformed requests fail before publication, leaving output slots untouched. Do not append fields to v1: incompatible extension needs a separately named/versioned interface. Rebuild native packages with the matching SDK; a stable public C table does not admit mismatched private Rust images.

## Reference semantics

RFC 0027's ownership, provenance, grants, resource loans, nested callbacks, retained failure reports, poisoning and interruption remain normative. Old callers receive the same success/failure and transfer semantics. No source syntax, inference, evaluation order, task scheduling, memory model or constant execution changes. Helpers embedded in generated headers are client code: future generators may improve implementation, but must not require old compatible callers to regenerate. Contract changes still undergo signature/version negotiation; compatibility never creates a capability or revives a stale receipt.

## Execution modes

The C contract applies to VM/JIT source and validated artifacts, and to admitted exact-SDK AOT Images. Actual mode availability and explicit bindings remain required. Existing both-host installed SDK probes retain AOT evidence. A native image continues to require trusted code and the exact host/build identity before private-pointer access.

## ABI and compatibility

Freeze `docs/component-abi-v1.h` and independent layouts against previous-client fixtures. IDL 1, LCMP 3, LCVL 1, LCSG 1, LANGBC 5, LANGNAT 2, program 3 and SDK 3 retain separate version domains; this decision changes none. Unknown or incompatible stored formats require re-encoding/recompilation from original source using matching tools. Additive provider members do not change a requested compatible projection. Major C changes require a new query/symbol namespace while preserving v1. Fixes may reject malformed inputs never admitted by the contract; document any observable compatibility consequence before implementation.

## Diagnostics and tooling

Keep numeric status/error identities and source provenance. No parser, formatter, LSP, debugger or lint changes. SDK documentation identifies the stable public boundary and experimental private linkage separately. Migration fixtures must be deliberate historical snapshots, never automatically regenerated during tests.

## Performance and cost model

No runtime implementation or cost change. Existing copies, bounded preparation, allocation/control accounting and retained owner costs remain. Tests add compilation/execution of previous clients; test durations are not performance evidence.

## Security and trust boundaries

Preserve validation before effects, explicit capabilities and finite controls. C pointer validity, compatible toolchains and trusted native implementations remain host responsibilities. Version/hash matching is neither authority nor native-code authentication. Existing original-scope cleanup and exactly-once retirement remain mandatory after failures.

## Alternatives

- No change: leaves users without a migration promise despite passing conformance.
- Documentation/tool-only check: cannot establish old-client runtime behavior.
- Extensible prefix-sized v1 tables: conflicts with accepted exact-size refusal; use a new version instead.
- Freeze only layout: simpler, but misses codec/transfer/cleanup regressions.

## Implementation plan

1. Retain the previous public reference and generated scalar/counter clients with source and digest provenance.
2. Execute previous-header/generated scalar C calls through the actual current VM/JIT host, including registry/resources/tasks/failure cleanup; execute old counter codecs and transfer outcomes alongside the new generator.
3. Run the full baseline and Windows CI, then record B02 completion and stabilization. No compiler/runtime stages need modification.

## Test plan

Use independent target layout/signature vectors, actual old/current C clients, exact canonical bytes, unknown-version/malformed-size refusal, ownership/failure/capability/resource/task tests and full both-host baselines. Existing SDK probes cover persisted AOT and isolated startup; rerun when production/SDK inputs change. No new fuzz campaign or performance claim is required for a test/documentation-only contract freeze.

## Open questions

No blocking design question. Future platforms and new major APIs need their own acceptance and execution evidence.

## Sources

- [RFC 0027](0027-production-component-interface.md), updated 2026-10-27: normative interface and conformance requirements.
- [B01 evidence](../evidence/2026-10-27-m3-conformance-19a29c0.json), recorded 2026-10-27: actual both-host conformance; Windows run completed 2026-10-27 07:45:35 UTC.
- [RFC 0032](0032-installed-component-packages.md), updated 2026-10-27: separate SDK/image trust and versioning.

Decision: **accepted under RFC 0001's experimental delegation**, 2026-10-27 JST. The existing contract has actual both-host evidence; retaining old clients adds a meaningful migration boundary without broadening private ABI guarantees. The remaining unverified assumption is executable old-client compatibility on both hosts, required before marking this RFC stabilized.
