<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0031-installed-native-task-packages.md -->

# RFC 0031: Installed native task packages

[日本語版](../ja/rfcs/0031-installed-native-task-packages.md)

- Status: implemented
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-27
- Last updated: 2026-10-28
- Tracking issue: M3-I14 and component/both-host follow-up M3-I08 (complete)
- Supersedes: None; extends RFC 0020 with independent manifest revisions
- Superseded by: None

## Current scope

Reconciled 2026-10-28 JST. Program/SDK version 2 remains the task profile; RFC 0032 separately adds version-3 components. The [M3 exit audit](../milestone-3-capabilities.md) records successful seven-profile isolated startup on macOS and Windows at `8f41f44`, followed by full conformance at `9e78f2d`. I08 is complete. The initial macOS-only checkpoint below is historical and does not describe current Windows support.

## Summary

Accept native program manifest **version 2 / profile `task`** and SDK manifest **version 2** for independently checked schema-5 task code and RFC 0030 native records. Link original objects with the exact installed SDK and run through the existing task runner without repository access, source analysis, code generation, Cargo or JIT at consumer startup.

Accepted under RFC 0001's experimental delegation on 2026-10-27 JST and implemented at `042fc38` (verified 03:49 JST that day). A clean exact-commit producer supplied the provenance-bearing SDK; after relocating it, a checkout-free consumer without Cargo successfully linked and ran copy/owned/C/task/owned-task. All source/package/private SDK link inputs were removed before startup; both task fixtures return 42. The complete baseline passes 149 suites/2,300 tests. Windows remains unverified and I08 remains open. Component package admission requires a follow-up decision before implementation.

## Motivation

The compiler currently refuses Program::Tasks in native package publication. Both manifests accept only version 1, program profiles are copy/owned/C, and SDK startup exposes no task facade. RFC 0030 data cannot silently become a version-1 package. The existing copied-SDK linker and finite trust-boundary checks can be reused.

## Goals

- Compile, publish, verify, link and execute an actual installed task package from its original object.
- Preserve independent semantic/record validation, exact SDK/toolchain/image identity, original allocator/control and mandatory task cleanup.
- Keep version-1 manifests/profiles and their refusal boundaries compatible.

## Non-goals

- Component package admission, inferred providers/grants, C embedding changes or dynamic native loading.
- Source syntax, LANGBC, LANGNAT, private task packet or existing C ABI changes.
- Performance/stabilization claims or treating target-description checks as Windows execution.

## Guide-level design

`lang native` selects the task profile from the existing checked source Program. It encodes schema 5, independently decodes the canonical bytes, emits the selected host task entry, detaches a LANGNAT-2 Task record and publishes a complete package atomically. The default generated Rust main uses an explicit host allocator and task execution limits through a scoped SDK facade, printing the i64 result. Custom Rust hosts retain the existing explicit host-source path.

Use `link-native` with a copied SDK and the package. It verifies/snapshots the original inputs, checks the installed rustc release/commit and invokes rustc/linker with bundled libraries. Cargo and repository paths are not build/runtime dependencies of the installed consumer. VM and JIT remain separate explicit modes.

## Reference semantics

Keep RFC 0020's exact JSON keys, role/path rules, duplicate/unknown-field refusal, digest/size validation and finite bounds. No new key or role is added.

| Manifest | Revision/profile | Required data |
| --- | --- | --- |
| Program | version 1, copy/owned/C | Existing interpretation unchanged |
| Program | version 2, task | Same keys; non-null entry FunctionId; schema-5 task semantics; LANGNAT-2 Task record |
| SDK | version 1 | Existing interpretation; cannot admit version-2 task packages |
| SDK | version 2 | Same keys; exact compiler/runtime/toolchain/dependency closure with task startup support |

Program version 2 admits only `task`; version 1 rejects it. Component and every unknown profile/version remain explicit refusals. SDK version 2 can link existing version-1 profiles with matching build identity. Task packages require SDK version 2 before compiler/linker launch. SDK assembly defaults to version 2; legacy version-1 readers remain supported for legacy packages, without a mode that relabels new task support as version 1.

Task package roles are exactly one semantic `program.lbc`, native record `program.native`, target object `program.o`/`program.obj`, generated `bindings.rs` and default `main.rs`; no C header is permitted. Bindings use recorded canonical symbols, exact BUILD_ID/TARGET and included semantic/native bytes. No pointer, task, allocator, capability or grant is serialized.

Independently require Task schema 5, a valid original no-argument i64 entry, matching manifest/semantic/native entry IDs, LANGNAT-2 Task kind, exact target/build, original object equality and valid bounded metadata. Reject mismatches before native guest effects; manifest/file/profile/semantic admission precedes tools. Runtime reconstruction and all required-symbol resolution precede task entry. Hashes check pairing/integrity and are not native authentication.

The initial task object producer is host-target only, matching RFC 0030's existing emitter. A request for the other tier-1 target must fail explicitly before package publication rather than substituting the host target. Run the producer on each real tier-1 host for both-host acceptance; cross-target Copy/ownership/C behavior is unchanged.

The SDK task facade independently decodes the semantic bytes, verifies the recorded entry and reuses RecordedObject preparation/binding. It accepts the caller's allocator, original Control and runner limits and returns the original structured outcome/failure, including task usage, source spans, cleanup and poison. No second scheduler, restored quota, source reparse, object emission or fallback is allowed. Retain the exact code owner through the scoped call; explicit unsafe image/resolver obligations remain visible.

## Execution modes

The task VM/JIT remain differential oracles and are unchanged. Installed native execution uses generated computation and the existing task primitives. This decision adds no component/C/WebAssembly execution mode; explicit component bindings and installed packages remain I08 work.

## ABI and compatibility

Program/SDK JSON revisions are independent of LANGBC 5, LANGNAT 2 and native task packet r1. Do not add the task profile to version 1. Old readers reject version 2; new readers preserve version-1 profile behavior. Rebuild program and SDK together for their exact build identity. Retain panic=unwind, pinned rustc release/commit, target, feature/profile policy, original path security, file tables and transactional output behavior. A later component profile needs its own explicit manifest/SDK compatibility decision; this task slice cannot complete I08 or B01.

## Diagnostics and tooling

Use structured version/profile/target/entry/codec/plan/binding/execution refusals with original source spans and expected/actual values where applicable. CLI usage errors remain exit 2; source/tool/runtime errors remain exit 1. Existing source parser, formatter, LSP and DAP semantics do not change. Preserve previous output after verification/tool failure.

## Performance and cost model

Reuse the existing SDK dependency closure and bounded file snapshot/link pipeline. Semantic decoding, record validation and native plan preparation retain finite limits; invocation retains the caller's original allocator/control/task bounds. Measure SDK/package/startup size or timing before claiming improvements. Compilation/assembly still require the pinned toolchain; consumer startup performs no compilation.

## Security and trust boundaries

Source/semantic/native/package data is untrusted input; validate bounded data before tools/guest effects at the corresponding boundary. Native objects, rustc/linker, host source and symbol resolvers remain explicitly trusted. Exact hashes do not sandbox native code. Never discover providers, grant permissions or run constructors as metadata validation. SDK copying does not confer guest filesystem/process/network authority.

## Alternatives

- No change retains explicit refusal and cannot meet installed task acceptance.
- Recompiling source/MIR or invoking Cargo on startup fails the installed no-codegen outcome.
- A new C-only task runtime or second portable execution engine would duplicate the admitted runner and require a separate ABI/runtime design.
- Reusing version-1 JSON is smaller but weakens old profile boundaries; independent version 2 is explicit and testable.

## Implementation plan

1. Add strict version/profile/entry admission and legacy migration/refusal tests before tool invocation.
2. Publish canonical task semantics, original object/record/bindings/main and add the scoped SDK task facade.
3. Execute installed scalar/owned tasks using a copied SDK with repository/source/Cargo unavailable. Verify malformed/mismatched inputs, original limits/cleanup and rollback; record actual host coverage. Continue component package design and both-host startup within I08.

## Test plan

Preserve version-1 vectors/profiles. Test task-under-v1, task-with-SDK-v1, unsupported versions/profiles/targets, schema/kind/entry/digest/object/file mismatch, exact/one-below limits and rejection before tools. Run original task/owned-buffer fixtures from a copied installation with exact rustc and no Cargo/repository access. Exercise caller controls, panic/cancellation/cleanup, retained image ownership and reuse. Extend bounded manifest fuzz coverage as appropriate; compile/lint is not a campaign. Run full repository checks; require actual Windows startup separately from target description/refusal tests.

## Open questions

No unresolved platform/installation blocker for this profile. RFC 0032 resolves component package and explicit host startup; the completed [M3 audit](../milestone-3-capabilities.md) records Windows and macOS seven-profile isolation. The earlier macOS-only checkpoint is historical. Startup observations are not a general performance guarantee.

## Sources

- [RFC 0020](0020-native-sdk-and-startup.md), updated 2026-10-27; manifest, exact SDK/toolchain and installed linking contracts.
- [RFC 0030](0030-native-task-component-records.md), updated 2026-10-27; detached native task/component records and no-codegen reconstruction.
- [RFC 0028](0028-task-component-artifacts.md), updated 2026-10-24; independently validated schema-5 task semantics.
- `8ba2c43`, verified 2026-10-27 01:59 JST: 149 suites / 2,298 passes; code has no published update date. Stored task/owned-buffer linkage is evidence for the runtime, not installed SDK or Windows startup.
