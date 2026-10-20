<!-- canonical-language: en; japanese-translation: ja/architecture.md -->

# Architecture baseline

[日本語版](ja/architecture.md)

Current implementation reviewed against `e60e3d9`, 2026-10-20 JST. Milestones 0–2 are complete at their recorded scope. Syntax, native ABI, bytecode and SDK formats remain experimental. Accepted RFCs define semantics; this document maps those contracts to the implementation and identifies future work. The [capability guide](milestone-2-capabilities.md) provides commands and the 18-gate M2 audit. Dated development checkpoints are preserved in [architecture history](architecture-history.md).

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

[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md) remains the broader ownership policy. Its source subset and current runtime services are implemented; this does not add source concurrency or make every host API Send/Sync.

## Failure, allocation and execution controls

[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md) governs exactly-once cleanup in reverse actual initialization order, return escrow, primary/secondary failures, poison/reentry and allocator authority. Recoverable failures use typed Result values. [RFC 0014](rfcs/0014-explicit-source-panic.md) defines terminal i64-coded source panic. Language cleanup uses explicit status/control flow, not native unwinding.

Guest memory uses a fallible accounting allocator with retained provider identity and live/cumulative limits. Host metadata and frame/retirement storage have separate preparation bounds. Mandatory retirement continues after control exhaustion; report truncation never truncates cleanup. Trusted host panics are contained only at the documented Rust/C adapter boundaries. Foreign unwinding, abort-mode panics and invalid native pointers remain outside that guarantee.

Instrumented execution shares instruction fuel, interruption/deadline, host-call, frame/live-cell and [service-work](rfcs/0013-service-byte-work-budget.md) accounting. One logical invocation shares counters across calls and service views. Admission precedes the corresponding effects; unsupported finite controls are rejected before execution. Service work counts specified logical events, not all CPU time, allocator work or arbitrary trusted callbacks.

The ordinary Copy native facade is trusted, synchronous and unmetered. Ownership native execution uses prepared managed frames and compiled continuations under [RFC 0012](rfcs/0012-native-ownership-continuations.md); the host driver dispatches native addresses without evaluating MIR or falling back to the VM. Guest recursion uses bounded logical frames. These profiles must not be conflated when describing limits or sandboxing.

## Portable and native formats

Version namespaces are independent. A component prototype schema number does not identify a `LANGBC` source-program schema.

| Format | Current selection and authority |
| --- | --- |
| Source program schema 1 | RFC 0006 Copy program |
| Source program schema 2 | RFC 0011 ownership program without explicit panic or C metadata |
| Source program schema 3 | RFC 0014 ownership program with explicit panic and no C metadata |
| Source program schema 4 | RFC 0017 C-aware ownership program, with or without explicit panic; metadata grants no C execution authority |
| Private ownership native revisions 1/2/3 | Ordinary / C-aware / explicit-panic conventions as specified by RFCs 0012 and 0014 |
| C profile 1 and object bridge 1 | External finite C values/descriptors and the separate private per-image binding slot |
| Native record schema 1 | RFC 0018 original object and compiler records tied to the exact portable bytes/build/target |
| SDK/program manifest version 1 | RFC 0020 bounded file tables, identities, roles, sizes and SHA-256 values |

Decoders reject unsupported versions, malformed shapes, invalid spans and resource exhaustion. Ownership artifacts store the validated pre-cleanup representation and independently reconstruct required proofs/elaboration. Native records retain original code metadata; startup does not regenerate machine code.

Hash/build identity detects mismatches; it does not authenticate a malicious SDK or native object. Portable artifacts contain diagnostic source snapshots, not authority to reparse files, resolve imports, load native code or acquire an allocator.

## C ABI and native SDK

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

[RFC 0005](rfcs/0005-structured-tasks-and-isolated-services.md) defines structured cancellation/isolation policy. [RFC 0003](rfcs/0003-versioned-component-contracts.md) defines versioned component contracts, capabilities, typed failures and resource ownership. Their state models and scalar/resource interoperability prototypes are evidence for the decisions; production source async, scheduler/channels, IDL/adapters and general mixed-mode embedding remain M3 work.

M3 now has an [active delivery board](milestone-3.md). [RFC 0021](rfcs/0021-bounded-structured-task-driver.md) defines the explicit bounded task driver in `crates/runtime/src/task.rs`: FIFO resume/cleanup dispatches, invocation-scoped handles, live/cumulative task admission, direct-child joins and cooperative cancellation. `cargo run --example structured-tasks` executes real host-owned frame trees, returns 42 and retires frames in order 2,1,0 for both normal completion and nested cancellation. This kernel executes no guest code; VM/native task adapters and actual guest-frame cleanup remain tracked deliveries. Independent task MIR evidence is described below. Reviewed 2026-10-21 JST; it changes no M2 execution path or artifact format.

[RFC 0022](rfcs/0022-source-structured-tasks.md) accepts explicit async effects, scoped Task claims, typed join cancellation and strict suspension loans. Lossless syntax binds to resolved async effects, Task/TaskError types and bounded move/scope claim snapshots. These reject invalid calls, consumed handles, scope escape and unavailable C surfaces; Task cannot be borrowed, matched, nested or stored in a container. Initial async transfers admit scalar/finite Copy values pending general Send/Share derivation. Available source analysis now retains TaskProgram evidence and CLI run/LSP use the connected task profile; raw HIR compatibility APIs and async REPL submissions keep explicit unavailable-profile refusal. The lexical loan engine now rejects live shared/exclusive loans at await, yield, cancel_check and implicit joins with SRC0028 plus the loan origin. It checks return/failure edges before branch states merge, ends exiting local loans before joins and preserves outer loans at nested joins. Combined TaskLoans snapshots retain source claim and loan evidence; independent MIR and initial VM evidence are described below; native task execution remains a later delivery. No task execution authority or artifact schema changes are introduced (2026-10-21 JST).

[RFC 0023](rfcs/0023-task-mir-boundaries.md) extends the existing ownership MIR type/declaration graph with Task/TaskError and an async effect separate from ABI. Independent validation enforces storage/signature restrictions and effect spans; ordinary call queries cannot erase async. Type/declaration success grants no task flow proof. The old place/body boundary, runtime layout compiler and bytecode writers reject this profile, including unused metadata; old readers explicitly restore synchronous effects and no task tags are assigned. A separate `ownership::tasks::verify` entry now proves task instructions over the same body table: bounded scope stacks and origin sets, move-only claims through locals/prepared destinations/CFG edges, suspension-loan refusal and protected return escrow. It also requires existing move/loan/arena-region proofs and elaborates explicit cancellation/failure cleanup phases before owner retirement. `VerifiedTasks` retains that evidence without granting layout, services or execution. Old VM/native APIs reject even scalar async proofs before effects; old codec tags remain unchanged. TaskProgram now combines source claim/loan evidence and independently lowered task MIR. An explicit VM task API uses managed finite Copy layouts, TaskClaim sidecars, per-activation scope identities and detached return reservations; ordinary call and task suspension continuations consume the failure plan. `cargo run -- run examples/scripts/tasks.lang` returns 42, and cancellation, failure-origin, loop-scope, fuel/trace/provider-refusal tests drain tasks and storage. Initial reachable locals are finite Copy/Task; owned/loan/foreign task execution, native continuations and complete shared controls remain T06–T08 work. Source analysis grants no host allocator, scheduler or artifact authority (2026-10-21 JST).

Keep deployment tiers explicit: trusted native code has no sandbox claim; verified scripts require capability/resource enforcement; portable WebAssembly components are planned for a later milestone. Native in-process plugins cannot serve as an untrusted sandbox. Future frontends may share MIR only where their semantics match, and otherwise require a component boundary.

Package-aware build/test/lint/doc, a package manager/installer, richer LSP/DAP, registry/provenance, LLVM evaluation, self-hosting and additional frontends remain in the [roadmap](roadmap.md). Current SDK packaging does not complete the package manager or installer. Bootstrap requires a frozen supported subset, stage 0→1→2 conformance and reproducibility on both hosts; a second frontend waits for a stable first frontend and ABI.

## Validation and maintenance

Run focused behavior regressions, then the repository fmt/Clippy/test/doc baseline for code changes. Preserve source/format fixtures and differential values, failure spans, cleanup and resource accounting. Native target execution, debugger execution, sanitizer/coverage-guided fuzz campaigns and performance measurements are distinct evidence; compilation alone does not establish them.

Keep present contracts here and in RFCs. Put chronological implementation notes in the relevant board or dated review, and update implementation status when its stated interface and validation are complete. Do not duplicate old “remaining work” paragraphs into this document.

Sources: repository implementation, lockfile, accepted RFCs and [M2 completion audit](milestone-2-capabilities.md), inspected 2026-10-20 JST at `e60e3d9`; individual RFC update dates and historical external source dates remain in the linked documents. This consolidation changes no language, ABI or serialized contract.
