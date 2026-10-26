<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0032-installed-component-packages.md -->

# RFC 0032: Installed component packages with explicit host bindings

[日本語版](../ja/rfcs/0032-installed-component-packages.md)

- Status: implemented
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-27
- Last updated: 2026-10-27
- Tracking issue: M3-I15 (implemented); M3-I08 retains both-host startup
- Supersedes: None; extends RFCs 0020, 0030 and 0031
- Superseded by: None

## Summary

Admit native program manifest **version 3 / profile `component`** and SDK manifest **version 3** for a canonical schema-5 component and an explicitly bound host Program. A matching installed SDK links the original LANGNAT-2 Component object and a required trusted Rust host. The host independently decodes the embedded component, supplies its actual contract, grants and providers, then binds the original object without code generation.

Accepted under RFC 0001's experimental delegation on 2026-10-27 JST. A prototype using only the exact SDK from `042fc38` links a stored component, moves the executable away from all producer inputs and executes 42, panic −42 and clean reuse 1 without Cargo, with zero resource/allocation inventory. This proves library availability for the selected path, not package admission, resource callback coverage, isolated final-source installation or Windows startup. Those remain explicit acceptance conditions.

Implementation completes at `8b89aae` (verified 2026-10-27 04:50 JST). The full baseline passes 150 suites/2,305 tests and verifies actual installed resource callbacks/cleanup. A clean exact-commit producer supplied the 100-file SDK and real provenance; after relocating it, a consumer with 12 absent repository input paths and no Cargo passes all six profiles after all link inputs disappear (isolated=true). The component executes 42/panic −42/reuse 1/fuel refusal with zero inventory. Windows remains unverified and I08 remains open.

## Motivation

RFC 0031 admits tasks only. Component objects already retain checked export and host authority through the original Program, while package readers and linkers have no component profile. Implicitly treating such code as a task would discard required bindings. The current SDK already includes the exact root `lang` library and its dependency closure; installing a second compiler or inventing providers is unnecessary.

## Goals

- Publish, validate and link a component package with explicit host authority.
- Execute saved scalar and resource/callback fixtures with the original allocator, Control, task identity, preparation and mandatory retirement.
- Preserve all legacy package/SDK combinations and bounded transactional input validation.

## Non-goals

- Implicit providers/grants, automatic dynamic loading, a default component main, source-language or C ABI changes, or a new execution engine.
- General component source CLI inference, WebAssembly, cross-host object emission or Windows evidence obtained by simulation.

## Guide-level design

The trusted Rust producer supplies canonical component artifact bytes, the actual Program prepared from their independently decoded Unit, a selected exported FunctionId and the host target to the native component package API. The producer retains all actual provider owners until emission completes. A package contains semantic, native_record, object and bindings roles; it has neither main nor c_header.

`lang link-native --sdk SDK --package PACKAGE --host HOST.rs --output APP` requires that explicit host. Generated bindings retain exact target/build identity and symbol addresses. The linker exposes the matching `lang` library along with the existing runtime libraries. The host embeds package bytes, reconstructs its own explicit bindings and calls the native component binding facade, then uses the existing LinkedProgram component execution methods. The facade creates no resource table, invocation, allocator, Control, provider or task on the caller's behalf.

## Reference semantics

Source syntax, inference, static ownership/effects, evaluation, cancellation, cleanup and failure selection remain those of RFC 0027. The canonical component artifact is a library with no script entry. The manifest's non-null entry is an explicitly selected existing export and must equal the LANGNAT Component entry. It may accept component arguments and return rich results; it is not an ordinary no-argument i64 script entry.

Producer and consumer compare the decoded artifact's complete checked document, code and binding map against the supplied Program's original Unit before object emission or symbol resolution. Admit comparison work/storage on bounded preparation limits before traversing those immutable graphs. No partial correspondence or merely equal interface name is sufficient. A changed provider inventory does not rewrite the recorded object; its actual explicitly prepared Program remains the authority used by reconstruction and execution.

Publication emits the original Component object only for the actual host target. Before external tools, snapshot validation checks strict manifest/version/role/path/size/hash rules, target/build identity, complete independent component decoding, selected export, record kind/entry and byte equality of the separately supplied object. Full native node/metadata preparation requires the actual bound consumer Program and therefore occurs in the binding facade, before any symbol resolution or guest entry. This phase distinction is explicit: package validation grants no authority and fabricates no temporary provider. Existing task/copy/owned/C preparation phases are unchanged.

The unsafe binding contract requires the exact unmodified checked object, matching compiler/SDK, correctly resolved addresses and an owner retaining executable code for the LinkedProgram lifetime. Hashes do not make arbitrary native code safe. The returned LinkedProgram borrows the supplied Program and retains the supplied code owner. Existing typed runtime failures and owner/poison lifetimes remain unchanged.

## Execution modes

VM/JIT remain differential oracles. Native execution uses recorded generated computation and original component/task services, with no source parsing, ISA lookup, object emission, JIT or VM fallback. Explicit actual bindings admit resource imports and VM/JIT/AOT callbacks under the existing same-task rules. This does not broaden the public C embedding surface.

## ABI and compatibility

Program versions 1 and 2 retain their exact profile sets and bytes. Version 3 admits only component. SDK versions 1/2 retain their behavior; SDK 3 accepts version-1 copy/owned/C, version-2 task and version-3 component packages from its exact build. Task still requires SDK >=2; component requires SDK 3, refused before tools otherwise. Old readers reject revision 3. LANGBC 5, LANGNAT 2, task packet and component C ABI versions do not change. Rebuild program and SDK together; existing bounded schemas, panic=unwind and pinned rustc checks continue.

## Diagnostics and tooling

Use structured component decode/correspondence/export/plan/profile errors before code resolution, and the existing missing-host, toolchain, snapshot and rollback diagnostics. The package API is explicit Rust tooling; ordinary source native compilation continues its existing profiles. Update the SDK guide and both documentation languages. Source spans and actual module origins remain stored in the canonical artifact and native record.

## Performance and cost model

Reuse the exact existing SDK closure. Independently decode metadata and reconstruct checked native nodes once per binding scope; bounded correspondence comparison is additional preparation. No runtime code generation or new scheduler is introduced. Record executable compile/link/startup observations and binary size without claiming a performance gate. The prototype establishes functionality only.

## Security and trust boundaries

Packages, artifacts, records and manifests remain untrusted data with finite byte/work/storage/depth limits. Trusted host code and exact machine code are outside a sandbox. Neither publication, validation nor linkage can grant a capability or replace actual providers. Preserve validation-before-effects, original authority, owner lifetime, mandatory cleanup and transactional publication/output. Refuse malformed metadata before guest execution even when the pre-tool phase cannot reconstruct provider-dependent nodes.

## Alternatives

- No change: preserves refusal but leaves installed components unavailable.
- A standalone script linking loose objects: the prototype works but lacks strict packages, transactional publication and shared diagnostics.
- A separate component SDK/runtime: duplicates the already installed closure and risks divergent authority and cleanup.
- Reuse task version 2 or synthesize providers: simpler wiring, but conceals profile incompatibility or grants false authority. Reject both.

## Implementation plan

1. Record the executable SDK prototype and expose I15 before implementation.
2. Add strict program/SDK revision 3, explicit component publication/binding, required host linkage and focused refusal tests.
3. Execute installed scalar and resource/callback cases, cleanup/control failures and legacy profiles; run the full fixed-input baseline and final clean-producer isolated probe.
4. Retain actual both-host task/component startup in I08; Windows is unavailable and no push is authorized.

## Test plan

Cover every version/profile pairing, missing host, unexported/wrong entry, mismatched complete Unit, wrong record kind/target/hash/object, malformed metadata and bounded preparation refusal before tools or resolution as appropriate. Verify rollback. Copied SDK consumers must execute scalar success/panic/reuse and actual resource/callback/cancellation/destructor failures with original authority and no remaining inventory. Keep existing legacy profile tests and independent record/fuzz coverage; run workspace/fuzz format, warnings-denied Clippy, all-target/all-feature tests and rustdoc. Clean committed producer provenance and checkout-free consumer execution remain final acceptance. Record actual hosts separately.

## Open questions

Windows task/component startup remains unverified. Further CLI ergonomics and SDK size reduction are not required by this first installed Rust-host profile. The admitted API and resource/callback behavior are validated above; broader performance costs remain unmeasured. Expose new prerequisites before later implementation.

## Sources

- [RFC 0020](0020-native-sdk-and-startup.md), updated 2026-10-27; exact SDK/toolchain and bounded linking.
- [RFC 0027](0027-production-component-interface.md), updated 2026-10-27; actual component/grant/resource authority.
- [RFC 0030](0030-native-task-component-records.md) and [RFC 0031](0031-installed-native-task-packages.md), updated 2026-10-27; recorded code and installed task compatibility.
- `042fc38`, SDK build `fc1cbc129a1a4581f7bc674e165abd60a121891c4fa61d45db1efdf4754a8511`; prototype verified 2026-10-27 03:54 JST on macOS AArch64. Code has no published update date. No external design claims are used.
- `8b89aae`, verified 2026-10-27 04:50 JST on macOS AArch64: 150-suite/2,305-test baseline, installed resource/callback execution and exact clean-producer isolation for six profiles. Code has no published update date.
