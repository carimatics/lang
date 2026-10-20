<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0020-native-sdk-and-startup.md -->

# RFC 0020: Experimental native SDK and scoped startup

[日本語版](../ja/rfcs/0020-native-sdk-and-startup.md)

- Status: implemented
- Authors: Codex, under the owner's experimental delegation
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-P08, M2-P09, M2-P11, M2-P10, M2-P07, M2-P12, M2-P02
- Supersedes: none; extends RFCs 0008/0011/0015/0017/0018/0019
- Superseded by: none

## Current implementation status

Reconciled on 2026-10-20 JST against [M2 capabilities and evidence](../milestone-2-capabilities.md). This RFC's scope is implemented, not stabilized. Read the original plan, decision-time statements and dated checkpoints below as rollout history. The guide records current native/SDK/platform work that those checkpoints treated as separate deliveries. Its review correction explicitly resolves the discovered RFC 0012 logical-debugger evidence gap; this reconciliation changes no normative requirement or non-goal of this RFC.

## Summary

Ship a native compiler command, a matching Rust SDK, and a scoped production startup consumer. Compile source to original native objects plus verified semantic/compiler records. Link an application using the SDK's Rust libraries and generated bindings; startup decodes/prepares existing records and calls the original machine code.

Review outcome: **accepted under experimental delegation, 2026-10-20 JST**, before product implementation. Reusing the tested entry/binding contracts, requiring explicit native trust and exact SDK/toolchain identity, and keeping C authority with the actual host is reasonably sound. The Mac ownership probe below supports copied-library startup. SDK assembly, compiler/consumer commands, minimal dependency selection, Windows installation and complete Copy/ownership/C install evidence remain unverified implementation obligations. Acceptance closes P08 only; it does not close G15.

## Motivation

The current `lang compile` emits portable artifacts. `prototype-object` accepts three fixed MIR prototypes. P05/P06 now restore production ordinary/C execution from files, but users still lack a source-native command and a distributable startup consumer. Packaging test drivers would retain test Runtime implementations or source reanalysis/backend emission. P07 cannot be completed by copying those fixtures.

A readiness probe at implementation `2712700` compiled a real Bytes program, linked its original object into a Rust consumer outside the repository, and used only copied dependency libraries to build that consumer. With `guest.lang` moved away, it printed `42; live allocations 0`. The consumer imports no frontend, supplies no Runtime implementation and performs no guest code generation. Its only dynamic dependencies reported by `otool -L` were system libiconv and libSystem. This is one Mac ownership experiment, not isolated install or all-profile evidence.

## Goals

- Provide production source-native compilation and source-free startup for Copy, ownership and explicit C libraries on both tier-1 hosts.
- Preserve the original entry identity, spans, controls, allocator, context, grants, cleanup and code lifetime contracts.
- Reject SDK, target, format and toolchain mismatches before native invocation/provider effects.
- Make an SDK install usable outside the repository and Cargo target/dependency directories.

## Non-goals

- Stabilize Rust artifacts or the native source-language ABI across SDK builds.
- Add a C bootstrap ABI, process-global binding registry, automatic C grants, second language frontend, native sandbox, package registry or dependency resolver.
- Change existing source syntax, C profile 1, portable schemas 1–4, native-record version 1, components or optional JIT.

## Guide-level design

The intended commands are:

```text
lang native [--source-root ROOT] [--target TRIPLE] --output DIR SOURCE
lang link-native --sdk SDK --package DIR --output EXE
lang link-native --sdk SDK --package DIR --host HOST.rs [--object HOST.o ...] --output EXE
```

These commands are not implemented by this decision. `native` emits a new program directory. The default target is the compiler host; both existing tier-1 target triples remain supported for object emission. A link/run host must match the program and SDK target.

A Copy/ordinary-owned package selects the actual checked zero-parameter i64 source main, including a nonzero function ID. Its generated default Rust main prints the value and exits 0, matching the existing script adapter; failure prints a structured diagnostic and exits 1. A script result is not an OS exit code. A custom Rust host can instead embed the scoped SDK API.

A C unit produces a library package with no automatic executable entry, even if it contains a private function named main. Its generated header and Rust bindings expose existing profile-1 exports. Use an explicit Rust host and, when needed, separately compiled C host objects. The Rust host keeps the binding active while a real C caller creates/calls/queries/destroys contexts. C imports still require actual host bindings/grants. No synthetic C export or MIR wrapper is added to run a private main.

The initial SDK explicitly requires its exact pinned Rust release/commit, a native linker and a C11 toolchain for C host objects. Rust SDK/host builds use panic unwinding to retain the accepted host-panic containment/poison contract; SDK assembly refuses abort builds. The effective panic policy participates in build identity. `link-native` drives rustc with bundled libraries; it does not run Cargo, fetch dependencies or consult repository paths. This Rust host requirement is an experimental distribution choice. A future C-only bootstrap requires a separate reviewed change.

## Reference semantics

The frontend, typed MIR, evaluation order, inference, loans, moves, arena/shared rules, destruction and failure precedence remain those of the accepted profile. Native emission uses the whole semantic unit; ABI declarations determine C classification, including unused C declarations. Names or scalar signatures do not stand in for ABI identities.

The compiler independently validates semantic data, creates original object/compiler records, emits canonical symbol bindings, and publishes only complete validated outputs. It preserves source snapshots/spans for diagnostics without reading original sources at startup. Existing outputs/inputs are preserved on failure: stage in a private sibling directory and use exclusive publication; reject an existing destination. Test concurrent publication and rollback rather than relying on a check-then-overwrite rename.

Startup validates the package/SDK/target identity, bounded semantic/native data and layout/plan domain before binding. Generated symbol resolution references only original object symbols, including the C slot/support/export/thunk symbols when present. It creates no ISA, backend module, executable memory or guest interpreter. Rust compilation of the host glue occurs during linking, before application startup.

The ordinary facade executes RFC 0019's original entry with the host allocator and cleanup Instance. Ownership controls and unsupported configurations retain VM precedence; finite frame/cell preparation is required. The Copy facade keeps the existing trusted synchronous unmetered native contract; requested unsupported execution controls must be rejected, never ignored. Neither facade invents host imports or C authority.

Use scoped execution/embedding functions so decoded IR, layouts, code owner, allocator and cleanup cannot escape their intended lifetime. C embedding retains the original explicit unsafe matching-image and synchronous owner-thread contract: all calls return and every context is destroyed before unbinding. APIs that accept raw C clients must expose this safety obligation; a safe-looking facade must not hide it. No new scheduling, sharing, constant execution or memory-model behavior is introduced. Cancellation/interruption and host-panic poison remain the existing runtime behavior.

## Execution modes

- VM: existing compile/run artifacts and limits are unchanged; a native package is never an implicit fallback for `run --artifact`.
- Native AOT: original object plus independently decoded records and production primitives; no source analysis, re-emission, JIT or MIR evaluation at startup.
- Optional JIT: existing explicit APIs remain available; a native mismatch/resource failure never selects JIT.
- C: existing profile-1 wrappers, actual C provider, original opaque context and explicit bindings/grants; no data tag grants execution authority.
- WebAssembly/components: existing independent contracts remain unchanged.

## ABI and compatibility

Add two UTF-8 JSON manifests, each with a fixed `format` string and integer `version: 1`. Reject unknown/duplicate fields, unsupported versions/profiles/targets, duplicate file paths/roles where singleton, malformed sizes/digests and noncanonical names before use.

| Manifest | Required logical fields |
| --- | --- |
| `sdk.json` | `format = "lang-native-sdk"`, `version`, `build_id`, `target`, exact `rustc` release/commit, build profile/feature policy, file table |
| `package.json` | `format = "lang-native-program"`, `version`, `build_id`, `target`, `profile = "copy" / "owned" / "c"`, `entry` function ID or null, file table |

Exact SDK keys are `format`, `version`, `build_id`, `target`, `rustc`, `build_profile`, `feature_policy` and `files`. `rustc` is an object containing only `release`/`commit`; `build_profile` is `"debug" / "release"` and `feature_policy` is a 1–128-byte ASCII identifier. Program keys are `format`, `version`, `build_id`, `target`, `profile`, `entry` and `files`. Each file entry contains only `{"role": string, "path": string, "bytes": u64, "sha256": string}`. SDK roles are singleton `compiler`/`runtime` and repeatable `dependency`/`header`/`startup`/`documentation`. Program roles are singleton `semantic`/`native_record`/`object`/`bindings` and applicable `main`/`c_header`. Unknown roles are rejected too.

Each file entry records its role, relative path, u64 byte size and complete lowercase SHA-256. Paths are normalized relative names: no absolute path, empty component, dot/dot-dot, NUL, alternate separator, Windows drive/ADS/device names or trailing dots/spaces, or escape through symlinks/reparse points. Object filenames use the target's `program.o`/`program.obj`. Program roles are semantic `program.lbc`, `program.native`, object, generated `bindings.rs` and, as applicable, default `main.rs` or `program.h`. File roles/entry/profile must agree with independently decoded semantics and record kind; ordinary ownership uses a whole-unit record and C uses CObject with the exact C Artifact digest. C entry is null. Generated bindings use recorded canonical symbols rather than assuming a private name/revision.

Build ID is an opaque 32-byte digest rendered as 64 lowercase hexadecimal digits, assigned consistently to the compiler and runtime SDK build. Its inputs cover production sources, manifests/Cargo.lock, pinned rustc release/commit, effective feature/profile policy, target and native ABI revisions. It is not just the crate version, Git cleanliness, interface digest or timestamp. The SDK build must produce one identity for its matching components and verify their file table. The compiler derives target-specific identity from common build inputs and the requested target; its own host is not substituted for that target. Cross-target emission requires the corresponding matching target SDK at link/run time; it does not promise cross-host Rust-library compatibility.

The compiler records its build identity; the consumer compares program, SDK and compiled runtime identity before invocation. Verify the actual installed rustc release/commit before compiling host glue. Snapshot/verify files used for linking, including the original object versus the native record; link those verified bytes. A build ID or hash detects mismatches and is not native-code authentication.

Default manifest bounds are 1 MiB/4,096 files for an SDK and 64 KiB/8 files for a program, with JSON depth 32, paths at most 4,096 UTF-8 bytes and bounded identity/toolchain strings. Program components retain existing semantic/native/header limits; generated bindings are bounded at 8 MiB and aggregate program staging at 256 MiB. SDK file verification streams bounded buffers, with at most 2 GiB per file and 8 GiB total by default. Exceeding configured limits is a structured refusal before unbounded reservation. Implement meaningful exact-limit/refusal fixtures.

The SDK contains the compiler, selected matching Rust dependency artifacts, production startup/binding support, header resources and usage documentation. Program C headers retain the existing profile/target/interface/toolchain declarations and identify their SDK build without changing C signatures. No Rust layout, pointer, context, allocator callback or capability is serialized. Native record and portable formats do not change.

Existing bare native records remain usable through their explicit trusted Rust APIs; they are not silently treated as SDK packages. Unknown manifests/builds require rebuilding with the matching SDK. Source editions, current portable artifacts and C profile 1 remain unchanged. Any subsequent manifest/schema/public contract change requires an explicit RFC revision or successor and migration fixtures.

## Diagnostics and tooling

Keep source diagnostic identities/spans and existing parser/formatter/linter/LSP behavior. Compiler, package, SDK, toolchain, linker and runtime errors are structured and identify the failing stage, path/offset and expected/actual values when relevant. Report an actionable missing Xcode/MSVC/rustc requirement without invoking guest code or destroying previous output. CLI usage errors exit 2; source/tool/runtime errors exit 1. Document ordinary and C host examples, required toolchains, limits and unsupported paths.

Preserve existing native source ranges/debug inputs; this decision does not complete G01–G03 debugger metadata, stepping or backtrace evidence. No new DAP or source fix-it semantics are introduced.

## Performance and cost model

There is one compiler pass and native object generation per build, plus Rust host compilation/native linking. Startup pays existing bounded semantic verification, metadata hashing, layout/plan preparation and runtime storage allocation. Native record 1 retains a full object copy in addition to linked code. C adds the existing wrappers/dispatch and wire storage costs.

The readiness experiment copied 116 existing debug rlibs with 868.8 MiB logical size, including multiple feature variants; this is not a minimal or release SDK measurement. SDK dependency closure, archive/executable size, startup, allocations and Rust/C workload comparisons must be measured later. No startup, runtime, size or memory improvement is claimed. Verify SDK files at assembly/link time; a self-contained linked application must not hash an entire installed SDK at every invocation.

## Security and trust boundaries

Source and portable data remain untrusted inputs under their existing budgets. Manifests/records grant no filesystem/network/process/FFI authority. Native objects, SDK libraries, generated Rust host code and additional host objects are trusted native build inputs; an explicit native command is not an untrusted plugin sandbox. Keep portable/Wasm capability-restricted execution as the default untrusted-code path.

Validate bounded data before launching a native linker/compiler and before provider/guest effects. Do not load an unknown image to discover whether it is compatible. Retain exact image/toolchain lifetime and C owner-thread obligations. Hashes are pairing/integrity checks, not signatures or proof of safe machine code. Do not execute constructors, callbacks or guest functions as part of metadata validation.

## Alternatives

- **No change:** leaves product AOT/startup and isolated installation missing.
- **Library/tooling only:** existing Rust APIs and test recipes prove behavior but make every user reconstruct metadata/toolchain policy and risk shipping a test Runtime.
- **Prebuilt stable C bootstrap (strongest competing design):** avoids a Rust compiler for C hosts, but requires a new initialization/shutdown/handle ABI, allocator/failure/lifetime contract and compatibility proof. Keep that as a separately reviewed future option.
- **Raw object plus ad-hoc C shim (simpler):** omits semantic/control/cleanup reconstruction and matching-build checks.
- **Process-global/lazy binding:** hides lifetime/publication and introduces initialization/concurrency effects; retain explicit scoped binding.

## Implementation plan

1. P09: source-native compiler, complete-unit classification, checked entry, original objects/records/header/bindings, manifests/build identity and exclusive output publication.
2. P11: bounded SDK/program manifests and verified component snapshots before tool/provider effects.
3. P10: production scoped Copy/owned/C consumer; retain original runtime/control/context/grant and matching-image contracts.
4. P07: assemble matching compiler/runtime/dependency/header/startup SDK with a bounded file table and declared external toolchains.
5. P12: exact rustc/SDK/target checks and link-native using verified original inputs, explicit Rust/C hosts and exclusive executable publication.
6. P02: isolate SDK/application from repository, target/deps, source and test helpers on each tier-1 host; compile/link/run actual Copy, ownership and C callers.
7. Keep Q01/Q02, B01–B04, G01–G03 and final bilingual Z01 guide open.

No frontend language, standard-library or package-manager feature is added by these steps. Do not count the probe, manifests alone or a successful in-tree link as SDK installation.

## Test plan

Retain portable/native golden bytes. Test CLI/source profiles and nonzero main IDs, C library classification, host/grant absence, output/input aliasing, existing output and concurrent/failed publication. Exercise every manifest truncation/tag/key/path/hash/build/toolchain/target refusal, budgets and structured errors before invocation. Bound parsing with malformed/fuzz inputs rather than treating JSON as trusted.

Run actual compiled executables and real C callers from installed SDK outputs on macOS AArch64 and Windows x86-64 MSVC. Cover pure Copy aggregates/calls, every ownership service family, explicit panic, configured limits/OOM/poison/retry, C scalar/Unit/aggregate/256-parameter profiles, original context/provider/grants and complete cleanup. Compare values/usage/reports/source identity to existing VM/native oracles at the appropriate scope. Remove original sources and prevent repository/target/deps access. Verify default generated code has no frontend/backend/JIT/test Runtime call path. SDK/build mismatch must refuse; never repair it by re-emitting guest code.

Use focused checks then the required code baseline. Documentation-only RFC/planning changes run repository consistency checks. Later performance/debug gates require their own measurements and actual debugger evidence.

Delivery breakdown audit (2026-10-20 JST): P11 owns bounded SDK/program validation and verified file snapshots before effects; P10 owns the scoped runtime consumer; P07 assembles the SDK; P12 implements the exact-toolchain `link-native` command and exclusive executable publication. P02 requires P12 for isolated installation. P11/P12 expose existing obligations, with no manifest/ABI/public behavior change.

## Implementation checkpoint

2026-10-20 JST: P09 implements `lang native`, whole-unit classification/original checked entry, semantic/native/object/header/binding output, target-specific SDK identity and exclusive publication. Tests run source-free generated drivers, managed/panic VM comparisons and an explicit real-C host; cross-target fixtures check Mach-O/COFF. The initial generated ordinary host policy retains 64 logical frames/65,536 live cells and a 512 MiB runtime-preparation bound, distinct from semantic decoding limits; explicit embedding can configure these finite controls. This remains in-tree producer/startup evidence. `link-native`, complete untrusted manifest validation, SDK assembly and both-host isolated installation remain P11/P10/P07/P12/P02. Status remains accepted until the intended SDK interface is implemented.

P11 checkpoint (2026-10-20 JST): typed bounded manifest validation and private streamed SDK/program snapshots are implemented. Verify compiled identity and every file before returning a snapshot; independently decode/prepare matching semantic/native/object data without code binding, tools or providers. Staging parents and native inputs remain trusted host policy; callers retain owners and leave private files unchanged during later linking. The parser fuzz target performs no I/O. This data boundary alone does not implement runtime consumption, SDK assembly/linking or isolated installation.

P10 checkpoint (2026-10-20 JST): production unsafe SDK APIs execute Copy/owned/C from the original image. Target/build and bounded records are checked before symbol/provider effects. Copy controls are refused rather than silently ignored; ownership retains the same allocator/Instance/control/usage/poison/original failure and source bytes. C is restricted to the owner-thread closure, with the unsafe caller retaining the obligation to destroy all contexts before unbinding, including unwind. Generated default drivers and the explicit real C host use these APIs. P07/P12/P02, Q/B/G and the final guide remain open.

P07 build-policy clarification: **accepted under experimental delegation, 2026-10-20 JST, before implementation**. Refusing abort preserves RFC 0019 host-panic recovery/poison; silently overriding build flags would hide the caller configuration, while accepting them could terminate instead of returning the required report. The full SDK retains all three profiles. Include the effective panic cfg in the existing build digest, with no manifest field/version or source/C ABI change; rebuilding an incompatible SDK remains the migration. Verify normal/release assembly and abort refusal before publication. Sources: official [catch_unwind](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html) (catches unwinding, not aborting panics) and [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#panic) (panic build policy), living documents without published update dates, verified 2026-10-20 JST.

P07 checkpoint (2026-10-20 JST): a repository build tool assembles the compiler/runtime and exact successful Cargo-selected Rust/proc-macro/native dependency artifacts, plus the unchanged common C profile-1 header, startup examples and bilingual instructions. The same build produces the assembler and compiler. Bounded streamed files/manifest are checked with the production SDK verifier before exclusive publication; compiler executable permissions survive. This implements assembly only: P12 public linking and P02 complete both-host isolated Copy/owned/C installation remain. SDK metadata keys/roles, source semantics and C signatures do not change; an incompatible SDK still requires rebuilding. No minimal-size/performance claim is made.

P12 checkpoint (2026-10-20 JST): the public `link-native` command snapshots/verifies SDK/program and bounded trusted Rust/C inputs, checks actual rustc release/commit/host in the same working directory used for compilation, then drives rustc from UTF-8 response arguments with bundled libraries and the original object. It does not use Cargo/registry/repository paths. A fresh executable is exclusively published after success; failure/concurrency keeps previous output. Ordinary entry defaults and explicit C host/context/grants remain unchanged. Tests cover toolchain/pre-tool refusal, diagnostic/host/output limits, rollback and one-winner publication, plus all three actual SDK profiles after source/SDK/program removal. P02 complete both-host isolation, Q/B/G and final guide remain; no manifest/ABI/version change or performance improvement is claimed. Source: official [rustc command-line reference](https://doc.rust-lang.org/rustc/command-line-arguments.html#path-load-command-line-flags-from-a-path), living document without a published update date, verified 2026-10-20 JST.

P02 checkpoint (2026-10-20 JST): exact head `c54af31` [isolated-install run 39370391958](https://github.com/carimatics/lang/actions/runs/39370391958) succeeded at 2026-10-20 04:44:42 UTC. Separate-VM macOS AArch64/Windows x86-64 consumers do not check out the repository, check original repository/target inputs absent and compile/link/run actual Copy/owned/real-C without Cargo. JSON reports reconcile execution after private source/SDK/program deletion, balanced original C allocator/context and matching revision/build/target. This completes P02's existing 3 pt. Matching Windows [run 39369585929](https://github.com/carimatics/lang/actions/runs/39369585929) (head `991768e`) passes fmt, C/linker checks, Clippy, full tests and rustdoc by 2026-10-20 04:47:51 UTC. The only difference at `c54af31` is the transfer workflow. Close G15 and set status implemented: the intended SDK interface and validation are available. This claims neither active prototype-benchmark/whole-workflow success nor stabilization. Q/B/G and the final guide remain open; no manifest/ABI/contract change or performance improvement is claimed.

## Open questions

No blocking contract question. Unverified implementation assumptions: minimal selected Rust artifact closure and platform link flags, deterministic shared build identity, exact manifest/component bounds under large programs, C-host ergonomics and broader combined-feature/platform conformance beyond the completed isolated installation. Review remaining validation within Q/B/G and revise this RFC explicitly before changing the accepted contract. An eventual C-only bootstrap is optional future work.

## Sources

- [SDK readiness producer](../experiments/sdk-owned-producer.rs), [consumer](../experiments/sdk-owned-consumer.rs) and [guest](../experiments/sdk-owned.lang): experiment against `2712700`, executed/verified 2026-10-20 JST; source removed at startup, output 42/live allocations 0.
- Installed rustc: `1.99.0`, commit `b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`, commit date 2026-09-28; `rustc --version --verbose` verified 2026-10-20 JST. This is environment evidence, not a cross-version ABI claim.
- [C profile](0008-cleanup-allocators-and-c-boundaries.md), [native continuations](0012-native-ownership-continuations.md), [C object bridge](0015-c-native-object-bridge.md), [schema 4](0017-c-aware-ownership-artifacts.md), [native records](0018-native-compiler-records.md), [ordinary entry](0019-owned-native-entry-runtime.md): RFC 0008 last updated 2026-10-19; the other linked RFCs last updated 2026-10-20; all verified 2026-10-20 JST.
- [CLI](../../src/main.rs), [fixed prototype tool](../../src/bin/prototype-object.rs), [delivery board](../milestone-2.md): current-state inspection at `0295793`, verified 2026-10-20 JST. The new command/manifest design above is a project decision; no external performance claim is used.
