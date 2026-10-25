<!-- canonical-language: en; japanese-translation: ja/roadmap.md -->

# Implementation roadmap

[日本語版](ja/roadmap.md)

The roadmap is ordered by risk reduction. Milestones are exit criteria, not calendar commitments.

## Current work and completed M2

**133 verified deliveries · 0 in progress · 0 remaining tasks (provisional 0 pt).**

M2 is complete: 18/18 gates, 113/113 historical gate weight. The [completed board](milestone-2.md) records its deliveries and estimate reconciliation; the [capability guide](milestone-2-capabilities.md) records actual source/VM/native/C/SDK/debug evidence and limits. These counts describe M2, not all future work.

**M3 is active:** the [delivery board](milestone-3.md) tracks **57 done · 1 doing · 14 todo · 43 remaining pt**. E02's owning-input checkpoint `3676a9d` (verified 2026-10-25 12:01 JST) connects actual native custody receipts to the original table's child argument commit. Bounded receipt checks use the original Session/Budget; preparation refusal preserves native custody and successful commit retires exactly the accepted receipts before guest effects. VM/JIT guest→native→guest→native mutates original backing, consumes once and rejects old native indices. Every fuel/provider prefix, destructor failure/poison, fresh roots and non-root identity at two-task limits pass. All 112 focused and **2,244 total tests**, fmt/Clippy/rustdoc and 15 fuzz-target compile/lint pass; 827-input before/after hashes match. E02 stays doing at 3 pt for fresh owning-result indices/escrow, same/cross-instance binding/grant/loan authority and public context_call/standalone C11. Resource-bearing callback result signatures refuse before effects until fresh-index transport is connected. **0 completed pt · 0 added/re-estimated pt**. All 72 rows reconcile: ledger **110 + 88 − 155 = 43**; gates remain **18/96 weight, 3/15**. Preserve every E03/E04/L04, E09, E01 full 32-operation, installed/P1, both-host and final-guide condition; the private gateway remains 25 real operations. Updated 2026-10-25 JST.

E08 caller-resource retirement, E10 accounting, E11 original-allocator views and E02 private child execution have source evidence. The public 32-operation API still requires complete checked callbacks, persistent provider lifetimes, combined controls and native AOT/SDK integration. Proceed **E02 → E03 → E09 → E04**, then controls/I08 before publishing E01; B01 both-host conformance also depends on E01. Reuse the original table/allocator, explicit accounting, source/MIR, grants and code owners.

## Active milestone execution

M3’s source/task/channel/service contracts are accepted and execute in the admitted VM/native profile. RFC 0027 IDL/codecs, I09 opaque-type/refusal proof, I06 independent source/MIR bindings, I04 generation, I05 rich negotiation and P01 typed grant/live binding admission are implemented at their recorded scope. Actual resource arguments/results, direct loans, native backing views and instance lifecycle execute through R01/R02/R03/R04/R05/E05/R06. I10/I11/I07 implement the schema-5 decision and stored task/channel/service/component execution. P02 implements original service-work accounting across root preparation and canonical adapters. E06 implements registration-owner retention. E07 implements scoped-program/retained-resource integration. The native-registry checkpoint connects explicit C grants and callbacks with original owners/contexts. Continue **E02 → E03 → E09 → E04**, shared controls and I08, then complete E01 publication and B01/B02. Reuse the canonical graph/signatures, version policy and existing code-owner lifetime pattern; prototype metadata alone is not a live binding. Use bottom-up remaining estimates separately from the 96-point exit-gate weights. Preserve M2 measurements; Z01 measures actual concurrency/boundary costs.

Do not expand the completed M2 board for maintenance or reviews. Record fixes in their scoped checklist, and keep code, documentation and validation in the same change. New language/ABI/format choices still require an explicit RFC decision; experimental delegation avoids a separate owner-approval wait.

## Task tracking

All implementation tasks that introduce unspecified source, ABI, package, or serialized behavior require an accepted RFC before that behavior is implemented. A decision task is complete when the owner or the agent acting under [RFC 0001's experimental delegation](rfcs/0001-project-charter.md#experimental-rfc-decisions) records a disposition; a rejected design requires replanning, not checking its dependent implementation. Run compile-pass/fail, malformed-input/resource-limit, and VM/native differential tests appropriate to each outcome. Decoder/verifier CPU and allocation budgets are required before untrusted deployment.

Tasks are listed in dependency-aware priority order within each milestone. A milestone can exit only when every `P0` task is checked and its exit evidence is recorded. Any unfinished `P1` task must be explicitly moved and re-estimated before exit; it cannot silently disappear. A checked task is complete; prototype groundwork completed in an earlier milestone does not complete a broader task in a later milestone.

The milestone checklists below retain relative exit-gate weights: `1` is very small, `2` small, `3` medium, `5` large, and `8` very large. Their weighted completion is `completed weights / total weights × 100`, rounded to one decimal place. These coarse weights do not predict elapsed time or remaining implementation effort. Long-horizon estimates require a bottom-up review when their milestone becomes current or scope/evidence changes. For each active milestone, a separate delivery board records independently verifiable 1–3-point remaining tasks and explicit scope/estimate changes.

Snapshot of exit-gate weights verified 2026-10-24 JST. Acceptance weights are separate from remaining-effort estimates; use the active M3 delivery board above for current bottom-up remaining work. The completed M2 record remains unchanged.

| Milestone | Completed gate weight | Total gate weight | Gate completion |
| --- | ---: | ---: | ---: |
| 0 — design baseline | 209 pt | 209 pt | **100.0%** |
| 1 — frontend and bytecode vertical slice (complete) | 89 pt | 89 pt | **100.0%** |
| 2 — ownership and native AOT | 113 pt | 113 pt | 100.0% |
| 3 — concurrency and embedding | 18 pt | 96 pt | 18.8% |
| 4 — developer toolchain | 0 pt | 63 pt | 0.0% |
| 5 — ecosystem and compatibility | 0 pt | 65 pt | 0.0% |
| 6 — self-hosting | 0 pt | 74 pt | 0.0% |
| 7 — additional frontends | 0 pt | 55 pt | 0.0% |
| **Overall** | **429 pt** | **764 pt** | **56.2%** |

## Milestone 0: design baseline

**Progress: 209 / 209 pt (100.0%).**

### Workstream breakdown

This table aggregates every Milestone 0 checklist item exactly once; it is a reporting view and does not add scope.

| Workstream | Completed | Total | Progress |
| --- | ---: | ---: | ---: |
| Project framing and source semantics | 10 pt | 10 pt | 100.0% |
| Cross-mode compiler/runtime semantics | 21 pt | 21 pt | 100.0% |
| Resource control, ownership, and concurrency | 30 pt | 30 pt | 100.0% |
| Component and resource contracts | 84 pt | 84 pt | 100.0% |
| Platform and backend evidence | 62 pt | 62 pt | 100.0% |
| Architecture-baseline disposition | 2 pt | 2 pt | 100.0% |
| **Milestone 0** | **209 pt** | **209 pt** | **100.0%** |


### Tasks in priority order

- [x] **P0 — Draft the project charter, goals/non-goals, terminology, RFC process, and compatibility labels.** — 3 pt
- [x] **P0 — Build and verify the typed `i64` MIR → bytecode/VM vertical slice.** — 5 pt
- [x] **P0 — Lower the same typed MIR through Cranelift JIT and host-native object emission.** — 5 pt
- [x] **P0 — Validate typed host registration and callbacks across VM, JIT, and AOT object contracts.** — 3 pt
- [x] **P0 — Validate typed CFG reachability, dominance, branches, and back-edges across execution modes.** — 5 pt
- [x] **P0 — Carry typed block arguments and loop values consistently through MIR, bytecode, VM, JIT, and AOT.** — 3 pt
- [x] **P0 — Establish shared linker requests, tier-1 target policy, Windows MSVC workflow, and host-independent COFF contracts.** — 5 pt
- [x] **P0 — Record macOS AArch64 link/run evidence and exercise direct and response-file paths.** — 3 pt
- [x] **P0 — Prototype deterministic VM instruction, host-call, and value-slot limits.** — 5 pt
- [x] **P0 — Prototype cooperative interruption and absolute execution deadlines.** — 3 pt
- [x] **P0 — Validate versioned scalar component metadata and explicit capability-gated imports against the typed host registry.** — 5 pt
- [x] **P0 — Prototype generation-tagged owned/borrowed resource handles and deterministic lifecycle validation.** — 3 pt
- [x] **P0 — Prototype strict and less-restrictive async-borrow and cancellation policies.** — 3 pt
- [x] **P0 — Prototype strict and tracing alternatives for cyclic graph ownership.** — 3 pt
- [x] **P0 — Prototype owner-isolated and exclusive-capability shared-service policies.** — 2 pt
- [x] **P0 — Draft decision-ready source-level integer and structured-control-flow semantics in RFC 0002.** — 5 pt
- [x] **P0 — Build the versioned benchmark harness and record compile, runtime, startup, and artifact-size baselines.** — 8 pt
- [x] **P0 — Record deterministic logical-heap profiles and linked-process memory baselines.** — 5 pt
- [x] **P0 — Exercise variable-size host allocation across direct Rust, VM, JIT, macOS AOT, and Windows object contracts.** — 3 pt
- [x] **P0 — Record AArch64 generated-code inspection and the current negative native debug-information baseline.** — 3 pt
- [x] **P0 — Decide RFC 0002, recording acceptance, requested revisions, or rejection.** — 2 pt
- [x] **P0 — Record successful Windows x86-64 CI object-link-run evidence.** — 3 pt
- [x] **P0 — Record and interpret a clean Windows benchmark baseline against the macOS baseline.** — 5 pt
- [x] **P0 — Prototype a bounded schema-1 component encoder/decoder with byte/count/depth/string limits, location-bearing diagnostics, malformed fixtures, and a fuzz target.** — 5 pt
- [x] **P0 — Specify canonical scalar/resource representation and ownership transfer in RFC 0003.** — 5 pt
- [x] **P0 — Specify panic and destructor failure behavior at component boundaries in RFC 0003.** — 3 pt
- [x] **P0 — Specify package/interface version compatibility negotiation in RFC 0003.** — 3 pt
- [x] **P0 — Prototype deterministic package/interface negotiation over an in-memory candidate set and exact lock input.** — 5 pt
- [x] **P0 — Connect one validated schema-1 scalar import/export through the VM, JIT, and AOT prototypes and record equivalent outcomes.** — 3 pt
- [x] **P0 — Prototype canonical resource preflight/commit, borrow scoping, cleanup ledgers, and instance poisoning in a backend-neutral adapter trace.** — 5 pt
- [x] **P0 — Measure metadata validation/negotiation, scalar dispatch, and resource lifecycle costs as separate benchmark series and document their interpretation in RFC 0003.** — 5 pt
- [x] **P0 — Prototype and specify a stable versioned component serialization for normalized requirements, digests, resource identities, and failure records.** — 8 pt
- [x] **P0 — Connect opaque canonical resource identities to generation-checked host values with atomic publication, borrow resolution, rollback, and exactly-once cleanup.** — 2 pt
- [x] **P0 — Prototype general component instantiation from validated contracts, negotiated bindings, and per-instance adapter state.** — 3 pt
- [x] **P0 — Execute the same host-value resource/failure fixture through VM, JIT, and AOT adapters and record equivalent outcomes.** — 3 pt
- [x] **P0 — Prototype deterministic lockfile/package-selection integration over the in-memory resolver without implementing the production package manager.** — 5 pt
- [x] **P0 — Connect the same scalar and resource/failure conformance fixture to the Windows MSVC direct and response-file AOT test paths.** — 1 pt
- [x] **P0 — Extend component-boundary cost evidence to instantiation, failure/destructor paths, memory, and code size, and document the interpretation in RFC 0003.** — 5 pt
- [x] **P0 — Define a nominal canonical `own<T>` type and carry one straight-line owner through MIR, bytecode, Cranelift JIT, and AOT object contracts with linear-use and zero-index checks.** — 3 pt
- [x] **P0 — Execute that typed owner through typed VM values and adapter-integrated VM/JIT plus macOS AOT direct/response-file native link/run ownership-transfer and guest-trap cleanup conformance, and configure the matching Windows path.** — 5 pt
- [x] **P0 — Compose and measure one representative multi-component application fixture across negotiation, exact-lock replay, instantiation, scalar/resource calls, and failure cleanup.** — 5 pt
- [x] **P0 — Record native Windows execution evidence for the component-conformance paths.** — 2 pt
- [x] **P0 — Decide RFC 0003, recording acceptance, requested revisions, or rejection.** — 3 pt
- [x] **P0 — Draft a decision-ready ownership and cyclic-graph RFC from the completed experiments.** — 5 pt
- [x] **P0 — Draft a decision-ready async-cancellation and shared-service concurrency RFC from the completed experiments.** — 5 pt
- [x] **P0 — Decide the ownership and cyclic-graph RFC.** — 2 pt
- [x] **P0 — Decide the async-cancellation and shared-service concurrency RFC.** — 2 pt
- [x] **P0 — Time and profile the variable-size allocation callback, including exact requested payload.** — 5 pt
- [x] **P0 — Measure a representative allocation workload, including allocator overhead and fragmentation.** — 5 pt
- [x] **P0 — Add one additional representative CFG workload to the cross-mode benchmark set.** — 3 pt
- [x] **P0 — Implement source-span-driven native debug metadata for representative AOT objects and linked artifacts on macOS AArch64.** — 5 pt
- [x] **P0 — Record equivalent Windows x86-64 CodeView/PDB and source-mapping evidence for the same representative artifacts.** — 3 pt
- [x] **P0 — Select the initial native backend from the completed cross-platform performance, debug-quality, and integration evidence, and record the decision.** — 3 pt
- [x] **P0 — Decide RFC 0001 after its component, resource-control, and backend-evidence prerequisites, recording acceptance, requested revisions, or rejection.** — 2 pt


### Completion evidence

Milestone 0 completed the accepted design baselines and prototypes, not the later production implementations. [The historical audit](roadmap-history.md#evidence-and-current-state) preserves the original evidence and benchmark qualifications. Current source/runtime capabilities are described by M1/M2 below.

Exit achieved: accepted design baselines, cross-mode prototypes, and initial backend evidence.

## Milestone 1: frontend and bytecode vertical slice (complete)

**Progress: 89 / 89 pt (100.0%).**

The owner accepted [RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md) on 2026-10-17; commit `031575e` records the disposition in both languages. Its five accepted design gates account for 16 completed points across lexical rules, modules/calls, aggregates/patterns, bytecode schema 1, bounded frames, and script/REPL behavior. Lossless parsing, module loading/resolution, source-wide typing, nominal layouts, patterns, source MIR, portable artifacts, bounded VM execution, and initial scalar native conformance are implemented. CLI/REPL, formatter, and LSP integration are also implemented and verified.

- [x] **P0 — Establish structured source diagnostics with stable codes, labels, and fix-its.** — 5 pt
- [x] **P0 — Accept the RFC 0002 lexical specification: identifier/keyword rules, whitespace/comments, signed-literal tokenization, source encoding, and malformed-token diagnostics, with fixtures.** — 3 pt
- [x] **P0 — Implement bounded UTF-8 ingestion and lossless tokenization with literal/comment diagnostics and source spans.** — 3 pt
- [x] **P0 — Construct the lossless syntax tree and parse the accepted grammar with error recovery and source spans.** — 5 pt
- [x] **P0 — Accept source module and function-call semantics, including visibility, cycles, entry points, host calls, call frames, and recursion limits beyond RFC 0002.** — 5 pt
- [x] **P0 — Load bounded modules under an explicit source root and resolve item/import namespaces with visibility and cycle checks.** — 3 pt
- [x] **P0 — Resolve parsed local/pattern scopes and type paths, including public-signature visibility, into source HIR.** — 2 pt
- [x] **P0 — Resolve and type-check structured RFC 0002 function input into span-preserving HIR with bounded analysis and diagnostic tests.** — 3 pt
- [x] **P0 — Connect parsed source to HIR and type-check source functions/control flow, including accepted call signatures, with compile-pass/fail fixtures.** — 5 pt
- [x] **P0 — Accept records, variants, patterns, and exhaustiveness semantics plus their typed MIR/value representation and compatibility fixtures.** — 5 pt
- [x] **P0 — Validate copy-only nominal definition shapes, reject recursive type cycles, and compute checked value-cell/depth layouts with malformed-input tests.** — 3 pt
- [x] **P0 — Type-check record/variant construction, fields, patterns, and exhaustiveness into source HIR.** — 5 pt
- [x] **P0 — Lower accepted primitive HIR into validated typed MIR with CFG block arguments, source maps, and VM/JIT differential tests.** — 2 pt
- [x] **P0 — Complete parser-connected HIR-to-MIR lowering, including accepted calls/aggregates and source-span conformance.** — 3 pt
- [x] **P0 — Accept the portable bytecode schema and bounded decoding/verification contract.** — 2 pt
- [x] **P0 — Promote the prototype into versioned register bytecode with encoding, decoding, and verification.** — 5 pt
- [x] **P0 — Implement accepted source calls, call frames, returns, and bounded recursion in MIR, bytecode, and VM with failure/stack tests.** — 8 pt
- [x] **P0 — Execute the source-language subset in the VM with typed host registration.** — 5 pt
- [x] **P0 — Accept script entry/output and transactional REPL state/error contracts.** — 1 pt
- [x] **P0 — Provide deterministic script and REPL entry points.** — 3 pt
- [x] **P1 — Implement the formatter and parser/formatter round-trip tests.** — 5 pt
- [x] **P1 — Implement a bounded revisioned query graph/cache with transitive invalidation, stale-result guards, and structured HIR diagnostic-cache tests.** — 3 pt
- [x] **P1 — Map structured diagnostics to bounded UTF-16 LSP data with protocol line endings, related labels, and position tests.** — 1 pt
- [x] **P1 — Integrate parser/module queries with an LSP prototype, including incomplete-source diagnostics, protocol transport/capabilities, and revision-safe publication.** — 4 pt


### Completion evidence

All 24 M1 outcomes are complete: 89/89 gate weight. [The M1 guide](milestone-1.md) records the 2026-10-17 source/VM/CLI/REPL/formatter/LSP acceptance evidence; [the M2 guide](milestone-2-capabilities.md) supplies subsequent ownership and full native support. M1-era native limitations are historical.

Exit verified: small programs run as scripts with deterministic behavior and useful IDE diagnostics.

## Milestone 2: ownership and native AOT

**Milestone 2 complete: 18/18 gates, historical weight 113 / 113 pt (100.0%), final capability guide/full exit audit Z01 complete.**

**Completed delivery record: [M2 board and estimate ledger](milestone-2.md).** The checklist below preserves all original acceptance boundaries; no M2 work remains.

Entry: RFC 0006's implemented copy-value profile and its compatibility fixtures are the baseline. RFCs 0007/0008 now supply the accepted source/core, cleanup, allocator and C-boundary requirements missing from RFC 0004's state models. Complete any new decision gate before dependent behavior; the delivery board records the 2026-10-18 bottom-up remaining-work review.

- [x] **P0 — Accept the source ownership/borrow/arena profile, Copy classification, and minimal core-library type surface (including text/bytes, collection use, and any required generic syntax).** — 5 pt
- [x] **P0 — Accept unspecified error/panic cleanup, unsafe/FFI validity, and allocator contracts before implementing their public interfaces.** — 5 pt
- [x] **P0 — Implement owned values, moves, and lexical borrow checking in typed HIR and MIR.** — 8 pt
- [x] **P0 — Implement RFC 0004 shared/weak retain-release, retained weak upgrades, and atomic cycle-preflight/publication with rollback and race tests.** — 8 pt
- [x] **P0 — Implement RFC 0004 lexical arenas, escape rejection, stale-reference checks, and deterministic teardown with compile-fail and runtime tests.** — 5 pt
- [x] **P0 — Elaborate deterministic drops using accepted return, error, and panic cleanup rules.** — 8 pt
- [x] **P0 — Implement RFC 0007 Text/Bytes source operations, UTF-8 validation, checked indices, and allocator-preserving conversions.** — 5 pt
- [x] **P0 — Implement RFC 0007 Unit/Option/Result and finite built-in specializations needed by core source operations.** — 3 pt
- [x] **P0 — Implement the RFC 0007 allocator-aware Vec source operations with move, loan, growth, and failure conformance.** — 5 pt
- [x] **P0 — Implement the RFC 0007 allocator-aware Map source operations with exact key equality and replacement/failure conformance.** — 3 pt
- [x] **P0 — Define narrow unsafe boundaries and enforce FFI validity rules.** — 5 pt
- [x] **P0 — Implement C ABI import/export and generated headers.** — 8 pt
- [x] **P0 — Lower the full RFC 0006 copy-value profile (guest calls, boolean interfaces, records, enums, and patterns) to native code with differential fixtures.** — 8 pt
- [x] **P0 — Complete ownership-aware source-program object generation and native link/run conformance for macOS AArch64 and Windows x86-64.** — 8 pt
- [x] **P0 — Harden linker discovery, startup/runtime packaging, and production error handling.** — 5 pt
- [x] **P0 — Build the representative differential VM/native conformance suite.** — 8 pt
- [x] **P0 — Measure representative native workloads against equivalent Rust/C implementations.** — 8 pt
- [x] **P1 — Emit and validate source-span native debug metadata on both tier-1 platforms.** — 8 pt

Review correction (2026-10-20, resolved): implement the RFC 0012 owned logical debugger chain missing from the old G18 evidence and verify it in [both-host run 39427222132](https://github.com/carimatics/lang/actions/runs/39427222132) at 73dca1d. The [updated guide](milestone-2-capabilities.md) records three logical frames and parent call sites.

G18 verification (2026-10-20 JST): exact product `1496b8b` passes [both-host actual debugger execution](https://github.com/carimatics/lang/actions/runs/39413674593). Copy source line 2→3 and caller lines 2/6/9, owned instruction stepping/production runtime caller and normal 42 results are asserted on LLDB/CDB. The manual matching-SDK debug link preserves source; source variables, logical continuation stepping and automatic CLI debug sidecars remain outside this evidence. [The final bilingual guide](milestone-2-capabilities.md) audits all 18 gates and completes Z01.

Exit: representative command-line programs match VM semantics and performance is measured against equivalent Rust/C implementations.

## Milestone 3: concurrency and embedding

**Progress: 18 / 96 pt (18.8%).**

Entry satisfied: Milestone 2 supplies ownership and cleanup semantics on both execution paths, and the repository review is complete. RFCs 0003 and 0005 remain the accepted semantic baseline; their prototypes do not complete production IDL, scheduler, or sandbox work. [The active delivery board](milestone-3.md) records implementation scope and estimates. The source VM already bounds frames and live cells; this milestone extends accounting across tasks, native code, and host callbacks.

- [x] **P0 — Accept remaining scheduler/channel/backpressure/error contracts, send/share derivation, and source async syntax without relaxing RFC 0005.** — 5 pt
- [x] **P0 — Accept the production component IDL/wire profile and embedding ABI, including compatibility, typed failures, callback reentrancy, and cancellation mapping to RFC 0003.** — 5 pt
- [ ] **P0 — Implement structured task lifetimes and deterministic cancellation cleanup.** — 8 pt
- [ ] **P0 — Implement channels with bounded backpressure and cancellation behavior.** — 5 pt
- [ ] **P0 — Enforce actor isolation and accepted data-race capability rules.** — 8 pt
- [x] **P0 — Implement the versioned component IDL and bounded serialization.** — 8 pt
- [ ] **P0 — Implement canonical owned/borrowed resource adapters.** — 8 pt
- [ ] **P0 — Enforce capability-gated component imports.** — 5 pt
- [ ] **P0 — Support mixed native/VM calls and callbacks with typed errors.** — 8 pt
- [ ] **P0 — Preserve stack traces and cancellation across native/VM boundaries.** — 5 pt
- [ ] **P0 — Enforce heap/allocation limits, including host-callback accounting.** — 5 pt
- [ ] **P0 — Enforce recursion-depth and live-task limits.** — 5 pt
- [ ] **P0 — Enforce cooperative interruption across VM, JIT, AOT, and callback boundaries.** — 8 pt
- [ ] **P0 — Stabilize the C embedding API and generated bindings after cross-mode ownership, failure, capability, and resource-limit conformance passes on both tier-1 platforms.** — 8 pt
- [ ] **P1 — Ship trusted-native and verified-script plugin examples.** — 5 pt

Exit: a native host can load a user script, expose a typed API, call the script, receive callbacks, and enforce capabilities without IPC.

## Milestone 4: developer toolchain

**Progress: 0 / 63 pt (0.0%).**

- [ ] **P0 — Accept versioned package/workspace manifests, target conditions, lockfile compatibility, and the machine-readable CLI diagnostic contract.** — 5 pt
- [ ] **P0 — Implement deterministic dependency resolution, lockfiles, checksums, and offline mode.** — 8 pt
- [ ] **P0 — Extend the existing `lang run` frontend into package-aware build/run/test orchestration with the accepted structured diagnostic output.** — 5 pt
- [ ] **P0 — Integrate package, fmt, lint, and doc commands into the CLI.** — 5 pt
- [ ] **P0 — Implement LSP navigation, completion, and semantic tokens.** — 8 pt
- [ ] **P0 — Implement safe rename and code actions.** — 8 pt
- [ ] **P0 — Implement a DAP adapter with mixed native/VM stacks.** — 8 pt
- [ ] **P1 — Add compile-time/runtime profiling, allocation traces, and rebuild explanations.** — 8 pt
- [ ] **P0 — Build an install/update/channel tool comparable in role to `rustup`.** — 8 pt

Exit: a new developer can install one tool, create a package, develop in an editor, test, debug, and produce a reproducible binary.

## Milestone 5: ecosystem and compatibility

**Progress: 0 / 65 pt (0.0%).**

- [ ] **P0 — Define editions, deprecation policy, and the compatibility matrix.** — 5 pt
- [ ] **P0 — Build the edition migration and machine-applicable fix tool.** — 8 pt
- [ ] **P0 — Complete the public-registry threat model before accepting uploads.** — 8 pt
- [ ] **P0 — Implement registry package storage, indexing, download, and immutable-version rules.** — 5 pt
- [ ] **P0 — Implement authenticated publish/yank/owner operations and an audit trail.** — 5 pt
- [ ] **P0 — Implement signed release manifests and package provenance.** — 8 pt
- [ ] **P0 — Add a WebAssembly component target for portable plugins.** — 8 pt
- [ ] **P0 — Validate WebAssembly plugin sandbox and capability conformance.** — 5 pt
- [ ] **P1 — Provide documentation hosting and package-metadata search.** — 5 pt
- [ ] **P1 — Evaluate an LLVM release backend using published benchmarks and debug-quality criteria.** — 8 pt

Exit: the language can evolve without silently breaking source, package, bytecode, or component consumers.

## Milestone 6: self-hosting

**Progress: 0 / 74 pt (0.0%).**

Entry: the Rust compiler must already compile the required text/byte, collection, allocation, module, and host-I/O primitives on both tier-1 platforms. Freezing the subset is an evidence-based compatibility decision, not permission to invent missing stage-0 semantics during the port.

- [ ] **P0 — Define and freeze the self-hosting subset.** — 5 pt
- [ ] **P0 — Implement lossless syntax, parsing, and diagnostics in the language.** — 8 pt
- [ ] **P0 — Implement resolution, type checking, and typed HIR in the language.** — 8 pt
- [ ] **P0 — Implement MIR lowering and verification in the language.** — 8 pt
- [ ] **P0 — Implement the required bytecode and native code-generation paths in the language.** — 8 pt
- [ ] **P0 — Implement the minimal package and compiler driver needed for bootstrap.** — 5 pt
- [ ] **P0 — Make the stage 0 Rust compiler build stage 1.** — 8 pt
- [ ] **P0 — Make stage 1 build stage 2 without using untracked stage 0 behavior.** — 8 pt
- [ ] **P0 — Run the same conformance suite against stages 0, 1, and 2.** — 5 pt
- [ ] **P0 — Compare reproducible outputs on both tier-1 platforms.** — 8 pt
- [ ] **P1 — Document bootstrap trust and disaster-recovery procedures.** — 3 pt

Exit: stage 2 passes the same conformance suite as stage 0 and can build the supported toolchain from a documented bootstrap chain.

## Milestone 7: additional frontends

**Progress: 0 / 55 pt (0.0%).**

Entry: the first frontend, shared ABI, and conformance suite must be stable. An additional frontend requires an accepted RFC and cannot be used to bypass unresolved semantics in the first language.

- [ ] **P0 — Demonstrate that a distinct frontend provides requirements that libraries, profiles, or syntax sugar cannot serve.** — 5 pt
- [ ] **P0 — Specify the frontend semantics and its shared-MIR/component boundary.** — 5 pt
- [ ] **P0 — Implement frontend-specific parsing, HIR, and semantic analysis.** — 8 pt
- [ ] **P0 — Lower only faithfully represented semantics through the shared typed MIR.** — 8 pt
- [ ] **P0 — Implement component adapters where memory, effect, or runtime models differ.** — 8 pt
- [ ] **P0 — Add cross-language package metadata and version resolution.** — 5 pt
- [ ] **P0 — Add differential cross-language conformance tests.** — 8 pt
- [ ] **P0 — Add cross-language stack traces, source mapping, and debugger integration.** — 8 pt

Exit: a second frontend adds clear user value without weakening the first language’s safety or making the shared IR an unmaintainable union of unrelated semantics.

