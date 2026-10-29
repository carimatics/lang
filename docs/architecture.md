<!-- canonical-language: en; japanese-translation: ja/architecture.md -->

# Architecture baseline

[日本語版](ja/architecture.md)

Current implementation verified at `9e78f2d`, 2026-10-27 JST. Milestones 0–3 have completed their exit gates. Public component C v1 and previous generated clients are stable under RFC 0033; source syntax, native Rust ABI and independently versioned bytecode/SDK formats remain experimental. The [M3 capability guide and 15-gate audit](milestone-3-capabilities.md) map commands, both-host evidence and limits. The [M2 guide](milestone-2-capabilities.md) and [dated architecture checkpoints](architecture-history.md) remain available.

## Goals

One language should support native applications and embeddable scripts with memory safety, explicit allocation and foreign authority, useful tooling, and a reproducible path to self-hosting. Native performance, productive higher-level features, data-race safety and multiple frontends are product goals, not claims that the current implementation already supplies them.

The first implementation targets macOS AArch64 and Windows x86-64. It does not promise a stable native language ABI, a tracing GC, arbitrary native hot reload, distributed execution, a public package registry or a second frontend.

## Compiler pipeline

```text
UTF-8 source + explicit source-root authority
  → lossless syntax / recovery / formatter
  → resolved source HIR
  → checked Copy or ownership HIR
  → independently validated typed MIR
      → versioned portable program → verifier → VM
      → Cranelift → JIT or native object → system linker
```

Typed MIR is the semantic contract. Backends preserve evaluation order, integer behavior, ownership, cleanup, failure identity and spans. The implementation has distinct Copy and ownership representations; “shared MIR” does not mean one physical layout or one artifact version. No frontend API depends on Cranelift, LLVM, VM or platform types.

| Layer | Implemented responsibility | Main code |
| --- | --- | --- |
| Diagnostics and syntax | Immutable source snapshots, checked byte spans, structured diagnostics/fixes, lossless arena trees, bounded recovery and canonical formatting | `crates/diagnostics`, `crates/syntax` |
| Source semantics and HIR | Root-bounded modules, visibility, names, finite concrete types, initialization/moves, lexical loans and arena escape checks | `crates/semantics`, `crates/hir` |
| MIR | Typed CFGs, independent type/move/loan/region verification, explicit failure and cleanup elaboration | `crates/mir` |
| Portable programs | Bounded encoding/decoding, version dispatch, source snapshots and independent verification | `crates/bytecode` |
| Execution | Shared allocation/storage/core services, failure reports and controls; VM execution and native adapters | `crates/runtime`, `crates/vm`, `crates/codegen-clif` |
| Tooling and drivers | Query invalidation, UTF-16 diagnostic mapping, CLI/REPL/LSP, linker and SDK packaging | `crates/query`, `crates/lsp`, `src` |

Source names are diagnostic data, not filesystem authority. Source loading checks explicit roots and overlays before cache lookup/read. Snapshots remain valid after edits or REPL reset. Parser, semantic, MIR and decoder limits are separate admission boundaries; an error must not become a partially validated executable.

## Source and memory model

[RFC 0002](rfcs/0002-prototype-control-flow.md) and [RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md) specify the Copy foundation: wrapping `i64` addition, signed comparison, left-to-right evaluation, functions/modules, records/enums, patterns, control flow and explicit returns. [RFC 0007](rfcs/0007-source-ownership-and-core-values.md) adds finite core types and ownership. [RFC 0009](rfcs/0009-whole-sum-ownership-extraction.md) adds whole Option/Result extraction.

- Scalars and aggregates whose fields are all Copy retain Copy behavior. Bytes, Text, Vec, Map, shared and weak values have explicit ownership; cloning is an explicit operation.
- Loans are lexical and tracked at whole-owner granularity. No partial moves, escaping ordinary loans or reference fields are introduced. Mutable replacement is prepared before publication and retains the accepted failure/cleanup ordering.
- Bytes/Text, finite Vec/Map specializations and shared/weak/arena operations use the supplied allocator. Allocation/growth can fail without publishing an invalid owner. UTF-8 validation, checked indices and exact key equality are runtime operations.
- Shared strong graphs are cycle-checked before publication; weak edges break ownership cycles. Lexical arenas permit same-brand cyclic storage, reject escape/cross-brand replacement and validate stale generations.
- Finite built-in specialization is implemented. User generics, traits, closures, lifetime parameters, ordinary function-result inference and borrowing across suspension remain future design work.

[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md) remains the broader ownership policy. Its source subset and runtime services are implemented. Logical-task concurrency is implemented separately by RFCs 0021–0026; this does not make every host API Send/Sync.

## Failure, allocation and execution controls

[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md) governs exactly-once cleanup in reverse actual initialization order, return escrow, primary/secondary failures, poison/reentry and allocator authority. Recoverable failures use typed Result values. [RFC 0014](rfcs/0014-explicit-source-panic.md) defines terminal i64-coded source panic. Language cleanup uses explicit status/control flow, not native unwinding.

Guest memory uses a fallible accounting allocator with retained provider identity and live/cumulative limits. Host metadata and frame/retirement storage have separate preparation bounds. Mandatory retirement continues after control exhaustion; report truncation never truncates cleanup. Trusted host panics are contained only at the documented Rust/C adapter boundaries. Foreign unwinding, abort-mode panics and invalid native pointers remain outside that guarantee.

Instrumented execution shares instruction fuel, interruption/deadline, host-call, frame/live-cell and [service-work](rfcs/0013-service-byte-work-budget.md) accounting. One logical invocation shares counters across calls and service views. Admission precedes the corresponding effects; unsupported finite controls are rejected before execution. Service work counts specified logical events, not all CPU time, allocator work or arbitrary trusted callbacks.

Component work (2026-10-24 JST, [RFC 0029](rfcs/0029-component-adapter-work.md)): `Invocation` creates the original ServiceWork during root preparation and forwards it to Machine and every service. Scoped wire Sessions charge canonical value observations, copies and UTF-8 to that counter, restoring prior association on exit/unwind. Whole-function/unused-import finite admission also consumes the current preparation Budget; unsupported method flags refuse before execution. Root-preparation work failure latches across retries/later controls; fresh roots reset the counter. Codec/admission retains cumulative preparation limits, mandatory cleanup is unmetered and destructor failures remain after the original primary. Sessions retain a synchronous counter and are no longer Send/Sync.

The ordinary Copy native facade is trusted, synchronous and unmetered. Ownership native execution uses prepared managed frames and compiled continuations under [RFC 0012](rfcs/0012-native-ownership-continuations.md); the host driver dispatches native addresses without evaluating MIR or falling back to the VM. Guest recursion uses bounded logical frames. These profiles must not be conflated when describing limits or sandboxing.

## Portable and native formats

Version namespaces are independent. A component prototype schema number does not identify a `LANGBC` source-program schema.

| Format | Current selection and authority |
| --- | --- |
| Source program schema 1 | RFC 0006 Copy program |
| Source program schema 2 | RFC 0011 ownership program without explicit panic or C metadata |
| Source program schema 3 | RFC 0014 ownership program with explicit panic and no C metadata |
| Source program schema 4 | RFC 0017 C-aware ownership program, with or without explicit panic; metadata grants no C execution authority |
| Source program schema 5 | RFC 0028 Task/Component MIR, complete IDL/maps, original snapshots/spans and independent task/binding proof; metadata grants no authority |
| Private ownership native revisions 1/2/3 | Ordinary / C-aware / explicit-panic conventions as specified by RFCs 0012 and 0014 |
| C profile 1 and object bridge 1 | External finite C values/descriptors and the separate private per-image binding slot |
| Native record schema 1 | RFC 0018 original object and compiler records tied to the exact portable bytes/build/target |
| SDK/program manifest version 1 | RFC 0020 bounded file tables, identities, roles, sizes and SHA-256 values |
| Native record schema 2 | RFC 0030 task/component records tied to the original schema-5 program and exact build/target |
| SDK/program manifest version 2 | RFC 0031 installed task packages |
| SDK/program manifest version 3 | RFC 0032 installed components with explicit host bindings |
| Component IDL / C ABI / LCMP / LCVL / LCSG | RFCs 0027/0033: 1 / 1 / 3 / 1 / 1, independently versioned domains |

Decoders reject unsupported versions, malformed shapes, invalid spans and resource exhaustion. Ownership artifacts store the validated pre-cleanup representation and independently reconstruct required proofs/elaboration. Native records retain original code metadata; startup does not regenerate machine code.

Hash/build identity detects mismatches; it does not authenticate a malicious SDK or native object. Portable artifacts contain diagnostic source snapshots, not authority to reparse files, resolve imports, load native code or acquire an allocator.

## C ABI and native SDK

Component C v1 is stable under [stabilized RFC 0033](rfcs/0033-component-v1-compatibility.md), verified 2026-10-27 JST. The public 32-entry exact-size/version surface, statuses and ownership/failure contracts are frozen; previous headers/generated clients pass actual macOS/Windows execution at `9e78f2d`. The [migration record](evidence/2026-10-27-m3-migration-9e78f2d.json) retains provenance and full validation. Both-host isolated seven-profile SDK startup also passes. An incompatible C extension needs a new version/namespace; private Rust/SDK ABI requires an exact build and rebuilding. Unsupported stored formats require re-encoding/recompilation from source with matching tools.

The stable component-v1 query `lang_component_v1_get_api` publishes one immutable, exact-size/version-checked table with all 32 entries non-null. VM/JIT source/artifact calls and exact-SDK AOT Images use that public surface with their actual grants and retained owners. Callback resource retirement checks the original live holder/table/owning receipt, precharges result/report snapshots in the original preparation Budget, and uses the original pre-reserved cleanup Session. Its restricted retirement loan saves/restores the parent bridge state and allows cleanup after interruption without guest reentry. Borrowed, transferred, stale and in-use claims refuse before effects; foreign owners stay opaque but retire through their original destructor. Receipt retirement prevents retries, mandatory backing release continues after failure, and Session poison prevents successful native result publication. Generated C executes these paths on VM/JIT/AOT. E01's complete query, I08's both-host isolation, B01 conformance and B02 old-client migration are complete; RFC 0033 stabilizes the public C v1 contract. C/serialized layouts are unchanged (RFC 0027, updated 2026-10-27 JST).

The SDK-owned component Image implements the C gateway's trusted AOT mode 2. Bounded program-v3/component manifests and exact host/SDK correspondence are checked before private Image access. The factory retains immutable semantic/native/bindings snapshots and actual code resolvers/owners; instance identity, grants/providers, resource loans and original control/task/allocator/preparation remain independent checks. Only the recorded selected export is callable, without VM/JIT fallback. Result bodies and instance state retire before their executable owner. The complete C table, generated-C scalar/task/reentry/resource fixtures, both-host conformance and isolated startup are complete in the [M3 exit audit](milestone-3-capabilities.md).

The implemented RFC 0008 boundary admits scalars, Unit and finite Copy aggregates through generated C11 headers and typed adapters. It does not expose managed tokens or private source layouts as C values. C imports require lexical `unsafe` and explicit host bindings/grants; `unsafe` does not disable ownership, loan or region checking.

Shared preflight validates complete descriptors, signatures, grants, addresses, alignment and canonical active fields. Ordinary indeterminate C padding is not read as a Rust byte slice. Actual native object validity, code/provider lifetime and synchronous owner-thread use remain caller obligations. Outputs publish only after successful execution, cleanup and validation. Callbacks retain the original context; rejected reentry/poison preserves the prior failure.

[RFC 0015](rfcs/0015-c-native-object-bridge.md) connects linked C exports/imports through an explicit private binding slot. [RFC 0018](rfcs/0018-native-compiler-records.md), [RFC 0019](rfcs/0019-owned-native-entry-runtime.md) and [RFC 0020](rfcs/0020-native-sdk-and-startup.md) provide source-native compilation, original-entry startup and isolated SDK installation.

`lang native` publishes a bounded program directory; `lang link-native` verifies SDK/program snapshots and the exact Rust toolchain, drives rustc/system linking with bundled libraries, and exclusively publishes a fresh executable. It does not need Cargo, the original repository or a registry at consumer link time. Explicit C hosts still supply their context, grants and allocator. The SDK requires the documented Rust release/commit, native linker and C11 compiler for C inputs; it is not a stable C-only runtime distribution. See [SDK usage](native-sdk.md).

## Native backends and debugging

Cranelift 0.136.3 in `Cargo.lock` is the initial JIT/AOT backend. Mach-O/AArch64 and COFF/PE/x86-64 use target-neutral linker requests, deterministic discovery and direct/response-file invocation. JIT mappings remain owned until calls/code borrows end. Adding LLVM, another target or bundled LLD requires evidence and a scoped decision.

Source function/instruction ranges survive to macOS DWARF/dSYM and Windows CodeView/PDB. Windows x64 source objects include unwind tables. The [M2 debug audit](milestone-2-capabilities.md) records actual LLDB/CDB breakpoints, Copy line stepping/callers and owned instruction stepping/physical runtime callers on both hosts.

Read-only revision-1 debug descriptors now expose the live owned logical frame IDs and source spans without allocating during dispatch. [Both-host actual debugger validation](https://github.com/carimatics/lang/actions/runs/39427222132) verifies three nested frames and parent call sites. The older G18 evidence alone did not satisfy that RFC 0012 requirement.

That evidence uses a manual matching-SDK debug link with retained source. Default `link-native` does not publish debugger sidecars. Source-variable locations, logical continuation stepping, optimized stepping, JIT debugger registration, mixed VM/native stacks and DAP remain separate work.

The [M2 source/Rust/C measurements](benchmarks.md) are tied to named immutable revisions and show substantial performance gaps. They establish measurement, not native parity or a speed guarantee. Use those workloads to decide subsequent optimizations; historical scalar microbenchmarks cannot substitute for source-program costs.

## Diagnostics, REPL and editor tooling

The CLI supports checking, source/artifact execution, portable compilation, native packages/linking, transactional REPL and a synchronous LSP prototype. Formatting is available through the shared CST API and LSP document formatting. [RFC 0010](rfcs/0010-owned-repl-previews.md) and [RFC 0016](rfcs/0016-indirect-repl-previews.md) define bounded returned-value previews. Results retire before output publication; sessions retain source definitions, not mutable guest owners.

The query graph invalidates transitive consumers and rejects stale publication after edits, close/reopen or newer work. LSP reuses source analysis, immutable overlays and UTF-16 mapping. It implements lifecycle, full synchronization, diagnostics, formatting and watched-module refresh with bounded framing/output. It does not yet provide navigation, completion, rename, code actions or DAP.

## Concurrency, components and future toolchain

[RFC 0021](rfcs/0021-bounded-structured-task-driver.md)'s bounded FIFO driver and the source/MIR contracts in [RFCs 0022](rfcs/0022-source-structured-tasks.md)/[0023](rfcs/0023-task-mir-boundaries.md) are implemented. Scoped task claims, joins, owned arguments/results, suspension-loan rules, cancellation and descendant-first cleanup are shared by VM, generated native continuations and admitted separately linked objects. Unclaimed owning results retain escrow/live-cell accounting; child join and result retirement precede parent-local cleanup. Tasks are logical tasks within one invocation, not guest execution on multiple OS threads.

[RFC 0024](rfcs/0024-send-share-and-service-authority.md)'s recursive Send/Share facts are independently derived by source and MIR. [RFC 0025](rfcs/0025-bounded-channels-and-backpressure.md)'s bounded MPSC channels retain FIFO backpressure, original wait identity, uncommitted-payload recovery, generation checks and cancellation. [RFC 0026](rfcs/0026-source-isolated-services.md)'s services separate producer authority from state ownership, inspect prospective replacements and retire queues before state. These reuse the existing allocator, ownership and mixed retirement machinery rather than adding an owner registry.

The IDL/codecs of [RFC 0027](rfcs/0027-production-component-interface.md), artifacts of [0028](rfcs/0028-task-component-artifacts.md) and adapter work of [0029](rfcs/0029-component-adapter-work.md) are implemented. Typed grants and selected providers are checked before effects. Canonical resource indices grant no authority: original table, generation, provider, holder and loan must match. Callbacks share the original allocator/control/preparation ledger, task and source/caller spans. Explicitly permitted synchronous reentry lends a scoped capability and restores the original scope on return, failure or unwind. Only immutable identity fields are read through raw pointers before thread validation; no mutable State reference or active capability is restored first. Expired/foreign-thread tokens, conflicting loans and guest reentry from cleanup are refused.

Root task binding checks an LCMP3 projection with no imports, one exported interface and one task function in it. Actual member, capability, exact version and full LCSG preimage match in the same preparation ledger. Root VM/JIT/AOT executes the original structured tree. Task imports and callback-context task export entry remain refused.

The stored records/packages of [RFCs 0030](rfcs/0030-native-task-component-records.md)–[0032](rfcs/0032-installed-component-packages.md) require original code, grants, providers and SDK identity, without discovery or VM/JIT fallback supplying missing authority. Actual macOS AArch64/Windows x86-64 seven-profile isolated startup and old-C-client migration are documented in the [M3 guide](milestone-3-capabilities.md) and dated evidence.

M4 is active as of 2026-10-29 JST. [RFC 0034](rfcs/0034-package-workspaces-and-cli-diagnostics.md) defines strict versioned manifests, explicit local dependency authority, lock/offline compatibility and JSON CLI diagnostics. `lang package check-manifest [--message-format human|json] FILE` validates at most 64 KiB without reading dependencies; success is silent, errors retain exit 1 (usage 2). The host `package::graph` API retains immutable manifest/entry bytes for sorted exact local graphs, sharing canonical dependency-directory identities and refusing cycles, escapes, name/version/target mismatches and resource excess. Capability-relative regular-file reads are bounded and nonblocking on Unix; unchecked files are never canonicalized. Default limits are 256 packages, 4096 active edges and 16 MiB entry bytes.

`package::source::analyze` supplies package namespaces to the shared frontend. Root-level `mod helper;` selects a declared active direct dependency; each package's `root::` stays local, and shared diamonds retain one nominal identity. First preparation captures all declared entry/test source directories once and releases filesystem handles. Later analyses use frozen bytes, directory aliases and per-source-root membership, including after deletion or project rename. Opened-file metadata detects duplicate source identities without canonicalizing final filenames. Capture failures are also cached; a new filesystem epoch requires a new Graph. Limits cover 256 source files, 16 MiB and 4096 directory entries, including unused inputs. Original source bytes/spans flow through existing HIR/MIR/portable VM and whole-program native JIT; the dependency fixture returns 42 in both modes.

`package::lock::Lock::capture` consumes the same frozen inventory without further I/O. Schema 1 pins target/compiler identity, exact package edges, original manifest/source SHA-256 and resolved entry/test/directory bindings, with length-prefixed package checksums and UTF-8 lexical order. Every declared selection survives overlapping inventories. `verify` bounds input to 1 MiB, refuses duplicate/unknown fields and unsupported versions, then requires exact canonical bytes. `check` reads only `Lang.lock` through an explicit output-directory capability and never writes; `publish` uses an exclusive same-directory temporary file, sync, closed handles and atomic rename, cleaning failed stages. The caller retains this separate output authority and publishes only after its complete operation succeeds. No ambient reopen of the Graph's display root or network fallback is performed. The fixed serialization/checksum fixture and relocated VM dependency replay live in `tests/package_lock.rs`; package CLI orchestration remains A04 work.

`lock::validate_encoding` performs bounded structural/schema admission only; it cannot establish checksum or replay validity. The `package_metadata` fuzz target exercises that reader and `Manifest::parse` without filesystem access or code execution. Actual lock replay still requires `Lock::verify`/`check` against the captured graph.

`package::command` now implements exclusive `new` and package-aware portable `build`/`run`. `Graph::resolve_in` consumes the same directory capability whose clone the caller retains for output; no source display pathname is reopened. `package::output` checks source/output aliases, bounds complete output to 64 MiB, stages immutable `target/lang/<lock-sha256>/build/{inputs.lock,main.lbc}` generations, compares complete expected bytes on reuse and publishes `Lang.lock` last. Locked mode checks before typing and never rewrites the lock. Compilation uses the frozen provider and execution calls the existing Copy/ownership/task VM engines and controls. Source diagnostics keep original codes, paths and UTF-8 offsets in schema-1 JSON. Legacy explicit-file forms keep their parser and execution paths. CLI fixtures exercise a real dependency returning 42, stale/tampered/runtime refusals, output aliases and profile selection.

Package test-entry integration, workspace selection/native executables, navigation/completion/rename, DAP and the installer remain on the [live board](milestone-4.md). Registry, WebAssembly components, additional backends, self-hosting and another frontend remain later milestones, not implemented capabilities. Async REPL submission, detached tasks and asynchronous foreign callback stacks also remain outside the current execution profile.

## Validation and maintenance

Run focused behavior regressions, then workspace fmt/Clippy/test/doc for code changes. Preserve source/format fixtures and differential values, failure spans, cleanup and resource accounting. Native target execution, actual debugger execution, sanitizer/coverage-guided fuzz campaigns and performance measurements are distinct evidence; compilation or CI durations do not establish them.

Keep present contracts here, detail in RFCs, and dated checkpoints in the [M3 architecture archive](architecture-m3-history.md) and [completed board history](milestone-3-history.md). Do not duplicate new checkpoints into this overview. The [2026-10-27 review](review-2026-10-27.md) records current maintenance fixes and validation.

Sources: repository implementation/lockfile, RFCs and the [M3 exit audit](milestone-3-capabilities.md), inspected 2026-10-27 JST. Implementation files have no embedded publication/update date; use individual RFC update dates and evidence verification dates. This consolidation changes no language, ABI or serialized contract.
