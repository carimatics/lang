<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0030-native-task-component-records.md -->

# RFC 0030: Native task and component records

[日本語版](../ja/rfcs/0030-native-task-component-records.md)

- Status: implemented
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-27
- Last updated: 2026-10-28
- Tracking issue: M3-I13 and installed follow-up M3-I08 (complete)
- Supersedes: None; adds a separate RFC 0018 record revision
- Superseded by: None

## Current scope

Reconciled 2026-10-28 JST. Revision 2 reconstruction and the installed follow-ups in RFCs 0031/0032 are implemented. The [M3 exit audit](../milestone-3-capabilities.md) completes I13/I08 and both-host task/component startup. The initial acceptance assumptions below remain dated history; the native record and private SDK are not stabilized by public C v1 stabilization.

## Summary

Accept private **LANGNAT revision 2** for checked task/component object records. Rebuild runtime plans from independently validated schema-5 MIR and exact recorded native metadata without ISA lookup, code emission or JIT construction. Binding still requires a trusted matching native image and its live owner.

Accepted under RFC 0001's experimental delegation on 2026-10-27 JST. The existing generated component object at `47093ef` executes arguments, panic/reuse, original-resource callbacks into VM/JIT/AOT and cancellation. This supports reusing the existing execution contract, but does not prove persisted reconstruction or installed startup. Those remain I13/I08 acceptance conditions. No package/SDK manifest revision is selected here.

## Motivation

I12's linked consumer still reconstructs compiler ObjectArtifact metadata and compares freshly emitted object bytes. It cannot start from stored semantic/native records alone. RFC 0018 revision 1 has no task/component kind or explicit entry, and old SDK startup admits only copy/owned/C. Silently adding kinds to revision 1 would weaken its compatibility boundary.

## Goals

- Persist original object bytes, entry, ordered function metadata and source ranges without producer-IR borrows.
- Independently revalidate schema-5 semantics and reconstruct the same bounded task/component runtime nodes before resolving code.
- Preserve original resources, explicit bindings/grants, allocator, control, preparation and borrowed-task authority during execution.

## Non-goals

- Source syntax, scheduling, component C ABI, LANGBC schema, LCVL/LCMP or native packet changes.
- Native authentication/sandboxing, automatic loading, executable addresses or live authority in records.
- Installed package/SDK publication, Windows execution or stabilization; these remain I08/B work.

## Guide-level design

The producer detaches a task/component ObjectArtifact into passive recorded data, writes it beside the exact schema-5 semantic bytes, and links its original object into a host. The consumer independently decodes those bytes, supplies the original checked task proof or explicitly bound component Program, reconstructs plans and binds matching statically linked symbols. It executes through the existing LinkedProgram entry APIs. A component record never creates a provider, grant, instance, resource table, scheduler or allocator.

## Reference semantics

The envelope remains eight bytes `LANGNAT\0`, little-endian u16 version, u16 flags and u32 payload length. Version is **2**, flags **0**, and length is exactly the bytes after the 16-byte header. Preserve RFC 0018 primitives, ordered vectors, hashes, symbol/range rules and bounds.

Revision-2 payload order is:

1. Target u8: **0** `aarch64-apple-darwin`, **1** `x86_64-pc-windows-msvc`.
2. Profile u8: **0** task, **1** component; then original entry FunctionId as u32.
3. SHA-256 of the complete exact semantic bytes, then SHA-256 of original object bytes (32 bytes each).
4. Object byte count u32 and bytes; function count u32 and ordered function records.
5. Each function retains RFC 0018's FunctionId u32, length-prefixed UTF-8 symbol, code-size u32, range count u32 and ranges `(start u32, end u32, source u32, source-start u32, source-end u32)`.

Reject unknown versions/flags/targets/profiles, lengths, trailing bytes, digests, symbols, unordered/duplicate function IDs and invalid code/source ranges under the existing finite record limits. The framing decoder returns passive data; it does not treat an entry number or metadata as a semantic proof.

Runtime reconstruction requires the exact host target, matching task/component kind, valid explicit entry and the independently checked original semantic program. A task entry retains the existing no-argument i64 contract. A component entry must be an export of the supplied checked Program. Reuse common reachable-function, signature/type/import support, layout, frame and native-node preparation. Validate the complete expected function set, symbol namespace/entry identity and bounded recorded source ranges before any resolver call or guest/native effects. Store no Rust memory layouts in the record.

Keep ordinary `lang_tasks_r1` and component `lang_component_tasks_r1` symbol namespaces and their exact-build/source/entry identity derivation. Recorded pointers start empty; successful unsafe binding resolves every required symbol from the exact trusted image and retains its owner through execution/results. Hashes pair data and do not authenticate machine-code semantics. Do not regenerate code to validate a record.

Ownership, destruction, cancellation, failure precedence and source spans remain the existing shared runner's behavior. Fresh roots use its original task admission; synchronous callbacks borrow the actual CurrentTask without creating a scheduler/task. Runtime plans retain their original checked Program/proof borrows; only compiler object data is detached. Framing and reconstruction retain explicit finite limits; runtime plan preparation uses the common cumulative ledger, including retained metadata, without refunds.

## Execution modes

VM/JIT execution is unchanged and remains the differential oracle. AOT performs generated computation and uses the same runtime nodes, controls and component Host. Native callbacks retain their original instance/resources/accounting/task. This RFC adds no WebAssembly execution mode.

## ABI and compatibility

Revision 1 keeps exactly its existing tags 0–4 and bytes. Revision 2 has its own two-tag namespace; do not reinterpret revision-1 tag 0/1. Old readers reject version 2 before payload interpretation. New readers continue accepting all existing revision-1 vectors; old-profile conversion APIs reject revision-2 kinds. Rebuild objects/records with the exact SDK/toolchain/native packet revision before trusted binding. Source editions, component ABI, schema 5 and old installed manifests retain their interpretation. I08 must separately decide any manifest/SDK revision and migration fixtures before accepting task/component packages.

## Diagnostics and tooling

Keep structured framing errors with byte offsets. Reconstruction reports profile/entry/function/symbol/source metadata or original bounded preparation failures before resolving code. Preserve source IDs/spans for runtime diagnostics. Parser, formatter, LSP and DAP semantics do not change.

## Performance and cost model

Stored native bytes/metadata and rebuilt nodes consume bounded host storage. Reuse common preparation instead of compiling an object or allocating executable memory at startup. Charge finite graph/metadata/hash work and requested storage; expose usage rather than claiming faster startup, lower memory or smaller binaries without measurements.

## Security and trust boundaries

Semantic/native bytes and metadata are untrusted data. Validate them before symbol resolution, constructors, provider calls or execution. Explicit unsafe binding still requires exact executable pointers, ABI/toolchain pairing and retained native code ownership. Neither matching hashes nor checked MIR make arbitrary native code safe. No capability, provider address, resource claim, native pointer or live task is serialized.

## Alternatives

- No change keeps the executable prototype but cannot meet persisted/installed acceptance.
- A library/tool that recompiles on startup retains the compiler dependency and fails the no-codegen outcome.
- A separate fully portable executable runtime would duplicate the existing checked MIR/native-node contract and belongs to a different backend design.
- Adding two tags to revision 1 is smaller but makes old profile interpretation ambiguous; a separate revision is explicit and inexpensive.

## Implementation plan

Implementation checkpoint (verified 2026-10-27 01:59 JST, macOS AArch64): revision-2 framing, detached records, common no-codegen node reconstruction/binding, stored task/owned-buffer/component/resource callback execution, malformed metadata/exact-budget refusal and cancellation/cleanup/owner lifetimes are implemented. The fixed-input full baseline passes 149 suites / 2,298 tests / 0 failures / 1 existing timing ignore, plus fmt/Clippy/doc and all 16 fuzz-target compile/lint stages. No fuzz campaign or installed/Windows startup is proved. Complete evidence/hashes are recorded on the [delivery board](../milestone-3.md).

1. Add revision-2 framing with independent vectors, malformed/budget refusal and unchanged revision-1 compatibility.
2. Detach task/component metadata and share checked native-node preparation with the emitter; reconstruct/bind without code generation.
3. Execute stored task and original-resource/component callback fixtures in separately linked consumers after releasing producer inputs. Complete I13 only with executable evidence; keep installed SDK/both-host startup in I08.

## Test plan

Freeze independent revision-2 vectors for both profiles/target descriptions and retain revision-1 vectors. Test truncation, unknown fields/tags, digest/profile/entry/function/symbol/range mismatch and exact/one-below finite limits before resolver/effects. Differential VM/native and separately linked task/component fixtures cover owning storage, native resources/callbacks, cancellation, failure cleanup, code-owner lifetime and reuse. Add bounded native-record decode/round-trip fuzz coverage alongside the existing manifest target; compile/lint is not a campaign. Record actual host coverage and full repository validation separately from both-host acceptance.

## Open questions

No unresolved blocker for revision 2. RFCs 0031/0032 resolve installed package/SDK revisions and explicit host-binding startup; the [M3 exit audit](../milestone-3-capabilities.md) records both-host installed execution. Native records and the private SDK remain experimental. Broader performance claims require new measurements.

## Sources

- [RFC 0018](0018-native-compiler-records.md), updated 2026-10-20; existing record framing/limits and trusted binding.
- [RFC 0020](0020-native-sdk-and-startup.md), updated 2026-10-20; exact SDK/toolchain and installed startup boundary.
- [RFC 0027](0027-production-component-interface.md), updated 2026-10-27; original component/task authority and I12 object decision.
- [RFC 0028](0028-task-component-artifacts.md), updated 2026-10-24; independently checked schema-5 semantics.
- `47093ef`, executed/verified 2026-10-27 00:09 JST: 148 suites / 2,294 passes; code has no published update date. This is component-object evidence, not revision-2/installed proof.
