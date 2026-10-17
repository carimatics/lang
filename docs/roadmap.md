<!-- canonical-language: en; japanese-translation: ja/roadmap.md -->

# Implementation roadmap

[日本語版](ja/roadmap.md)

The roadmap is ordered by risk reduction. Milestones are exit criteria, not calendar commitments.

## Task tracking

All implementation tasks that introduce unspecified source, ABI, package, or serialized behavior require an accepted RFC before that behavior is implemented. A decision task is complete only when the owner records a disposition; a rejected design requires replanning, not checking its dependent implementation. Run compile-pass/fail, malformed-input/resource-limit, and VM/native differential tests appropriate to each outcome. Decoder/verifier CPU and allocation budgets are required before untrusted deployment.

Tasks are listed in dependency-aware priority order within each milestone. A milestone can exit only when every `P0` task is checked and its exit evidence is recorded. Any unfinished `P1` task must be explicitly moved and re-estimated before exit; it cannot silently disappear. A checked task is complete; prototype groundwork completed in an earlier milestone does not complete a broader task in a later milestone.

Cost uses relative effort points rather than elapsed-time promises: `1` is a very small change, `2` is small, `3` is medium, `5` is large, and `8` is very large. Each unchecked task has one independently verifiable outcome; larger programs are split at dependency or acceptance boundaries. Progress is `completed points / total points × 100`, rounded to one decimal place. Long-horizon estimates are planning values and must be re-estimated when their milestone becomes current or when scope or evidence changes.

The 2026-10-16 whole-roadmap audit corrected three structural problems. Production work that requires the source frontend was incorrectly included in the design baseline; several tasks combined independently shippable outcomes; and later milestones omitted required registry, bootstrap, and interoperability work. The revised list preserved the 80 points of evidence completed at that audit, moved production hardening to its dependent milestone, and exposed previously hidden work rather than treating a large compound item as one task. A follow-up component-workstream audit separated in-memory compatibility negotiation, scalar execution-prototype integration, backend-neutral resource-adapter behavior, and boundary-cost measurement. The previous single cross-mode scalar/resource task mixed those acceptance boundaries and overlapped Milestone 3's production IDL and adapter implementation. A post-schema-18 audit found that native debug-metadata implementation and cross-platform debug-quality validation were named as backend-selection prerequisites only in prose; they became explicit 5-point and 3-point tasks before backend selection. A host-value follow-up audit split another 8-point item that still combined three acceptance boundaries: the host-value identity bridge, general instantiation, and VM/JIT/AOT resource/failure conformance. The split preserved the original 8-point total while making each outcome independently checkable. Decision tasks that required repository-owner acceptance were kept unchecked and ordered after their evidence prerequisites until the owner accepted them. The checklist records dependency order, not a claim that an external gate is locally executable: when an owner decision or remote runner is unavailable, work proceeds to the next unblocked P0 without changing priority.

An RFC-readiness audit on the same date corrected another planning gap: RFC 0003 was queued for an owner decision even though its review outcome then named stable serialization, host-value and cross-mode adapter conformance, lock integration, native Windows component execution, and broader cost evidence as blockers. Those outcomes became explicit prototype tasks before the RFC 0003 decision; Milestones 3 and 4 still own the production IDL, adapters, and package manager.

A native-evidence audit on the same date found that the 3-point Windows component item combined two acceptance boundaries: locally verifiable MSVC direct/response-file test integration and externally observable native-runner evidence. The item was split into a completed 1-point implementation task and a then-pending 2-point evidence task, preserving its total cost while preventing unavailable remote evidence from hiding local work.

An RFC-consistency audit then found two RFC 0003 decision blockers that were still described only in the RFC: typed resource lowering through the shared MIR/backend contracts and representative multi-component application evidence. They are now explicit 8-point and 5-point Milestone 0 prototype tasks. They do not replace Milestone 3's production component IDL and adapter implementation. Production package-manager I/O remains Milestone 4 work and is not a prerequisite for deciding the RFC 0003 design.

After the first typed-resource slice, the 8-point lowering task was split at its next acceptance boundary without changing total cost. The completed 3-point task covers nominal MIR/bytecode typing and the Cranelift JIT/AOT machine lane; the second 5-point task covers typed VM values, adapter-integrated ownership transfer/cleanup, and native link/run conformance.

The second typed-resource slice is now complete at its stated prototype boundary: typed VM values and adapter-integrated VM/JIT execution share the nominal owner lane with macOS and Windows AOT direct/response-file native link/run tests. GitHub Actions run 38932964398 records the matching Windows execution and cleanup outcomes, completing the separate 2-point evidence task below.

The representative-application task is also complete at its prototype boundary. A host-orchestrated fixture composes two-provider negotiation, deterministic lock generation, exact-lock replay, instance publication, scalar and typed-resource VM/JIT calls, and guest-trap cleanup. Its clean schema-20 macOS baseline records a 67.592 us median, 68.459 us p95, and 1.109% RSD across 31 samples of 1,000 operations. The clean Windows baseline records a 236.804 us median, 266.366 us p95, and 6.201% RSD for the same row; production generated dispatch stubs remain separate work.

The 2026-10-17 repository review exposed missing entry gates for source ownership/core-library syntax and the production component profile, and separated native compilation of the existing copy-value profile from ownership-aware AOT. It adds 21 pending points to Milestone 2 and 5 to Milestone 3; completed work remains 298 points. The lower overall percentage reflects newly explicit work. Source/core-library foundations must precede representative workloads and self-hosting; C embedding stabilization follows conformance and resource enforcement.

Progress snapshot verified against this repository state on 2026-10-17 (JST):

| Milestone | Completed | Total | Progress |
| --- | ---: | ---: | ---: |
| 0 — design baseline | 209 pt | 209 pt | **100.0%** |
| 1 — frontend and bytecode vertical slice (complete) | 89 pt | 89 pt | **100.0%** |
| 2 — ownership and native AOT | 21 pt | 113 pt | 18.6% |
| 3 — concurrency and embedding | 0 pt | 96 pt | 0.0% |
| 4 — developer toolchain | 0 pt | 63 pt | 0.0% |
| 5 — ecosystem and compatibility | 0 pt | 65 pt | 0.0% |
| 6 — self-hosting | 0 pt | 74 pt | 0.0% |
| 7 — additional frontends | 0 pt | 55 pt | 0.0% |
| **Overall** | **319 pt** | **764 pt** | **41.8%** |

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

Completion state at this snapshot:

- **Completed dispositions:** the repository owner accepted RFC 0003 and then RFC 0001 on 2026-10-17; RFC 0002, RFC 0004, and RFC 0005 were already accepted on the same date.
- **Completed architecture decision:** Cranelift 0.136.3 is the initial native JIT/AOT backend; frontend and artifact contracts remain backend-neutral, and LLVM is deferred unless later release evidence justifies it.
- **Milestone 1 exit:** all P0/P1 outcomes are complete; the [capability guide](milestone-1.md) records usage, evidence, and remaining limits. Milestone 2 is the next implementation scope.

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

### Evidence and current state

Verified 2026-10-17: RFCs 0001–0005 are accepted, while source compilation and production runtime/toolchain implementation remain incomplete. Completed tasks above record prototype outcomes, not production feature completion.

- MIR → bytecode/VM and Cranelift JIT/AOT share the tested scalar/CFG and straight-line owned-resource subset. Source spans survive bytecode lowering as well as native mapping.
- Bounded schema-1/2 codecs, whole-request capability preflight, negotiation/lock replay, resource adapters, host-value cleanup, and failure-report fixtures exist. They do not provide a complete sandbox, production package manager, or general component execution.
- macOS AArch64 tests and [Windows run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398) record direct/response-file native execution. Debug inspections are linked from the [architecture](architecture.md).
- [Benchmark evidence](benchmarks.md) owns measurement history. Historical clean schema-20 values predate the 2026-10-17 JIT reclamation fix; a new clean run on each tier-1 platform is required before current compile/application budgets.
- RFCs 0004/0005 accept the strict ownership/concurrency policies. Semantic models exercise selected traces, not complete reference-counting, task-tree, scheduler, or cancellation-request implementations.

Exit achieved: accepted design baselines, cross-mode prototypes, and initial backend evidence. Later milestones must supply source-level and production conformance.

## Milestone 1: frontend and bytecode vertical slice (complete)

**Progress: 89 / 89 pt (100.0%).**

The owner accepted [RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md) on 2026-10-17; commit `031575e` records the disposition in both languages. Its five accepted design gates account for 16 completed points across lexical rules, modules/calls, aggregates/patterns, bytecode schema 1, bounded frames, and script/REPL behavior. Lossless parsing, module loading/resolution, source-wide typing, nominal layouts, patterns, source MIR, portable artifacts, bounded VM execution, and initial scalar native conformance are implemented. CLI/REPL, formatter, and LSP integration are also implemented and verified.

The 2026-10-17 estimate remains 89 points: it exposed bytecode/REPL acceptance as 2-point/1-point tasks without dropping implementation scope. Primitive typing was split into 3 points of structured-input HIR and 5 points of parser-connected source/call conformance; lowering into 2 points of primitive HIR/MIR and 3 points of parser-connected/extended-source conformance. Foundations were counted separately from the broader parser-connected outcomes; the final audit verifies their integration.

The 8-point incremental-tooling outcome comprises the shared graph/cache core (3 pt), protocol diagnostic/position conversion (1 pt), and parser/module-query plus LSP-prototype integration (4 pt), all complete. Lossless tokenization (3 pt) and syntax-tree construction/recovery (5 pt) likewise preserve the parser's full scope and the 89-point total.

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

### Implementation evidence history

The records below describe each commit's state; the final completion audit supersedes their then-remaining work.

Evidence verified 2026-10-17 (JST): commit `a4f135c` adds `lang-diagnostics` with immutable source snapshots, UTF-8-validated spans, Unicode/UTF-16 positions, accepted `SRC0001`–`SRC0007` identities, primary/secondary labels, notes, and atomic fix previews. Five focused tests cover positions, stale/foreign snapshots, code identities, cross-file rendering, edit conflicts, and control-character escaping. The full fmt/Clippy/test/doc baseline passes. Parser-connected diagnostic conformance remains implementation work. Commit `40a56cb` supplied the English/Japanese RFC 0006 proposal and compatibility-fixture tables; commit `031575e` records subsequent explicit owner acceptance. The complete records/variants/calls/tooling scope remains in this 89-point plan.

Commit `f8d7370` implements `lang-syntax` UTF-8 ingestion and lossless tokenization with 13 conformance/limit tests, including integer boundaries, malformed tokens, comments, Unicode, NUL, exact source ranges, deterministic reconstruction, and resource bounds. The full fmt/Clippy/test/doc baseline passes.

Commit `c301665` completes arena-based lossless parsing of the accepted grammar with separate file/expression entry points, missing-syntax nodes, delimiter-aware recovery, conservative fix-its, and node/depth/list/diagnostic/work bounds. Eighteen parser tests cover syntax shape snapshots, all fixture prefixes, deterministic malformed inputs, resource boundaries, brace preservation, and later recovered items. The source-parser fuzz target compiles with the locked fuzz workspace and completed an uninstrumented 10,000-run smoke; instrumented coverage-guided evidence is not claimed. The full fmt/Clippy/test/doc baseline passes. Semantic resolution/typing, formatter, LSP integration, and source execution remain separate unchecked outcomes.

Commit `da778cc` adds accepted-subset `lang-hir` analysis with 13 structured-input conformance/limit tests. It resolves lexical locals, rejects function-wide reuse and parameter assignment, enforces primitive types/conditions/returns, and preserves diagnostic spans without choosing lexical rules. The full fmt/Clippy/test/doc baseline passes. Parser diagnostics and text-to-HIR compile-pass/fail coverage remain unchecked.

Commit `a198518` completes the accepted primitive HIR-to-MIR slice with branch joins, loop-carried mutable state, early returns, sequential SSA renumbering, source maps, construction limits, and final MIR validation. Eight structured-input integration tests verify scalar extrema, branch paths, nested loop state, VM/JIT equivalence, VM fuel exhaustion, bytecode spans, and JIT/AOT-object debug mapping. The full fmt/Clippy/test/doc baseline passes. This does not supply parser-connected execution, native link/run evidence for parsed source, guest calls, or aggregates; those tasks remain unchecked after RFC 0006 acceptance.

Commit `8dafdeb` adds `lang-query` with 10 graph tests and one real structured-input HIR-cache integration test. Evidence covers dependency replacement, diamond invalidation, unaffected cache reuse, stale/foreign result rejection, close/reopen, cycles, node/edge/work limits, and revision overflow without partial bookkeeping updates. The full fmt/Clippy/test/doc baseline passes. The graph operates only on declared dependencies and host-supplied immutable values; automatic compiler recipes and the LSP-connected source path remain unchecked. Roadmap review preserves the full 89-point plan and records the core/integration boundary explicitly.

Commit `b4a6f24` adds `lang-lsp` diagnostic data conversion with four focused tests covering UTF-16/surrogate widths, LF/CRLF/lone-CR line endings, CRLF interior boundaries, EOF ranges, related locations, severity/code preservation, missing snapshots/URIs, and source/label/note/output limits. The [architecture](architecture.md#diagnostics-lsp-and-debugging) records official LSP 3.18 sources and their 2026-10-17 verification dates; publication/update dates are not provided in those living documents. The full fmt/Clippy/test/doc baseline passes. This is a data adapter, not a server or a wire codec. The remaining 4-point integration task retains parser/module queries, capabilities, transport, document lifecycle, and revision-safe publication after RFC 0006 acceptance.

Commit `eadf992` adds explicit-root canonical module loading, compilation-local IDs, namespace collection, visibility-aware module/enum-case paths, forward aliases, and iterative alias-cycle rejection. Seventeen local tests cover hierarchy, privacy, cycles, duplicates, parser/encoding failures, cumulative budgets, declaration/work limits, and Unix symlink boundaries; a Linux-only case checks non-UTF-8 filenames. The full fmt/Clippy/test/doc baseline passes. The original 5-point scopes/resolution/loading task is split into this verified 3-point module outcome and 2 points of parsed local/pattern scopes, type paths, and public-signature visibility. The 89-point total and source integration scope are preserved. A namespace graph does not establish typed bodies or source execution.

Commit `844aad0` resolves parsed scopes, nominal type paths, forward/mutual calls, host imports, and record/enum/pattern names into backend-independent `lang-hir::resolved` arenas. It preserves annotation/expression/pattern/control-flow spans and written evaluation order, rejects function-wide name reuse and immutable assignment, and checks public-signature type visibility. Fifteen tests verify these outcomes, complete syntax-fixture conversion, exact work boundaries, resource limits, and snapshot identity. The full fmt/Clippy/test/doc baseline passes (319 tests). This completes source name resolution only: expression/pattern typing, nominal layouts/cycles, exhaustiveness, and return completeness remain in the unchanged typing tasks. A resolved program is not an executable artifact.

Commit `1d5fa88` adds bounded nominal layout analysis to `lang-hir`: validated IDs/names/spans, duplicate member and empty-enum rejection, iterative cycle detection, checked record cells/enum maximum-payload cells, and depth/work/structural limits before guest allocation. Ten HIR tests and two parsed-source tests verify cycles, overflow, exact depth 64/65 and cell 4096/4097 boundaries, explicit deep stacks, malformed inputs, and small budgets. The full baseline passes (331 tests). The original 8-point aggregate-typing task is split into this verified 3-point layout outcome and 5 points of aggregate operations/pattern typing and exhaustiveness. All scope and the 89-point total are retained; layouts do not establish typed bodies or execution.

Commit `ef95dc0` completes source-wide HIR typing for primitives/control flow, guest/host calls, exact nominal aggregates/fields, pattern bindings/shapes, return paths, and bounded exhaustiveness. Typed programs expose immutable type/field/order/flow facts only after successful checking; warnings preserve artifacts, while semantic errors or forged IR cannot reach lowering. Twenty-four integration tests include compile-pass/fail source cases, 1,458 finite-domain oracle comparisons, a 256-field product, module-valid missing witnesses, and malformed-ID/tree/scope/budget boundaries. Both fuzz targets compile; the new `typed_source_hir` target completed an uninstrumented 10,000-run smoke without claiming sanitizer/coverage-guided evidence. The full baseline passes (355 tests). This closes the original source/call and aggregate/pattern typing outcomes, not MIR lowering or source execution.

Commit `e6b6987` adds the accepted compilation-unit MIR model and a budgeted verifier with source-byte metadata, all required operations, nominal layouts, SSA/dominance, CFG arguments/reachability, signatures, entry validation, and existing owned restrictions. Fifteen tests include 1,536 independent dominance-oracle comparisons, exact layout/resource boundaries, source positions, and cumulative allocation/work accounting. The full baseline passes (370 tests). This is a prerequisite shared by remaining lowering, bytecode, and VM tasks; no checklist points are awarded for their incomplete integration. The next scope is canonical schema-1 encoding/decoding using the same cumulative decoder/verifier budget.

Commit `b5b83b7` completes portable schema-1 bytecode encoding/decoding and shared-budget verification. Eleven tests cover an 88-byte fixed fixture, all twelve operations, canonical global table sorting and ID relocation, byte-identical reencoding, truncation/malformed inputs, and cumulative byte/count/allocation/work limits. The decoder fuzz target compiles and completed an uninstrumented 10,000-run smoke. The full fmt/Clippy/test/doc baseline passes (381 tests). This closes the 5-point portable-artifact outcome; execution remains separate.

### Completion audit (2026-10-17)

All 24 Milestone 1 outcomes are complete: **89 / 89 pt**, including both P1 tooling outcomes. No scope was removed or moved to obtain this result. The overall roadmap is **298 / 738 pt (40.4%)**; Milestones 2–7 retain their original unchecked scope.

Commit `402a5dc` completes the final 4-point LSP integration. Twelve protocol/stdio integration tests cover lifecycle/capabilities, UTF-16 ranges, incomplete source, increasing/stale versions, close/reopen and clearing, parent-context module overlays, closed-dependency routing, watched files, local-root authority, malformed framing/JSON, bounded output/recovery, and canonical formatting. Commit `d419df9` separately verifies automatic parser/module/diagnostic queries and stale-publication rejection with twelve database tests. The complete fmt/Clippy/test/doc baseline passes **497 tests** on macOS AArch64.

Source execution evidence includes `cb3312d` (typed source to verified MIR), `b1964e7` (nine scalar VM/JIT/native conformance tests and local direct/response-file AOT link/run), and `d041598` (portable VM, seventeen source-to-wire execution tests and seven unit tests with 324 independent simultaneous-copy comparisons). `77287e7` and `bbc1f3d` complete scripts and transactional REPL sessions, with ten CLI tests, sixteen session/CLI tests, and a local TTY smoke. `2d3a3ff` completes the formatter with nine CST/comment/idempotence tests and two source VM/native round trips. Earlier evidence above records parsing, resolution, typing, layouts, and portable schema-1 verification.

Accepted [RFC 0002](rfcs/0002-prototype-control-flow.md) and implemented [RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md), last updated 2026-10-17 JST, define this completed profile. Native guest calls/aggregates remain explicitly unsupported until Milestone 2; production ownership/concurrency and complete developer tooling remain later scope. Local source-native evidence does not claim a new Windows CI run, sanitizer campaign, coverage-guided fuzz result, stable ABI, or production sandbox.

The final bilingual [capability guide](milestone-1.md) records the implemented language, CLI/REPL/editor workflows, embedding APIs, limits, evidence, and later scope. It completes the documentation deliverable after this exit audit without replacing or reducing any implementation requirement.

Exit verified: small programs run as scripts with deterministic behavior and useful IDE diagnostics.

## Milestone 2: ownership and native AOT

**Progress: 21 / 113 pt (18.6%).**

Entry: RFC 0006's implemented copy-value profile and its compatibility fixtures are the baseline. RFC 0004's state models do not specify source grammar, production allocation, or an ABI. Complete the corresponding decision gate before each dependent implementation; re-estimate the core-library slice after its RFC fixes the minimal requirements.

- [x] **P0 — Accept the source ownership/borrow/arena profile, Copy classification, and minimal core-library type surface (including text/bytes, collection use, and any required generic syntax).** — 5 pt
- [ ] **P0 — Accept unspecified error/panic cleanup, unsafe/FFI validity, and allocator contracts before implementing their public interfaces.** — 5 pt
- [ ] **P0 — Implement owned values, moves, and lexical borrow checking in typed HIR and MIR.** — 8 pt
- [x] **P0 — Implement RFC 0004 shared/weak retain-release, retained weak upgrades, and atomic cycle-preflight/publication with rollback and race tests.** — 8 pt
- [ ] **P0 — Implement RFC 0004 lexical arenas, escape rejection, stale-reference checks, and deterministic teardown with compile-fail and runtime tests.** — 5 pt
- [ ] **P0 — Elaborate deterministic drops using accepted return, error, and panic cleanup rules.** — 8 pt
- [ ] **P0 — Implement RFC 0007 Text/Bytes source operations, UTF-8 validation, checked indices, and allocator-preserving conversions.** — 5 pt
- [ ] **P0 — Implement RFC 0007 Unit/Option/Result and finite built-in specializations needed by core source operations.** — 3 pt
- [ ] **P0 — Implement the RFC 0007 allocator-aware Vec source operations with move, loan, growth, and failure conformance.** — 5 pt
- [ ] **P0 — Implement the RFC 0007 allocator-aware Map source operations with exact key equality and replacement/failure conformance.** — 3 pt
- [ ] **P0 — Define narrow unsafe boundaries and enforce FFI validity rules.** — 5 pt
- [ ] **P0 — Implement C ABI import/export and generated headers.** — 8 pt
- [x] **P0 — Lower the full RFC 0006 copy-value profile (guest calls, boolean interfaces, records, enums, and patterns) to native code with differential fixtures.** — 8 pt
- [ ] **P0 — Complete ownership-aware source-program object generation and native link/run conformance for macOS AArch64 and Windows x86-64.** — 8 pt
- [ ] **P0 — Harden linker discovery, startup/runtime packaging, and production error handling.** — 5 pt
- [ ] **P0 — Build the representative differential VM/native conformance suite.** — 8 pt
- [ ] **P0 — Measure representative native workloads against equivalent Rust/C implementations.** — 8 pt
- [ ] **P1 — Emit and validate source-span native debug metadata on both tier-1 platforms.** — 8 pt

Exit: representative command-line programs match VM semantics and performance is measured against equivalent Rust/C implementations.

Implementation evidence verified 2026-10-17 JST: commit `926fb3b` adds `lang-codegen-clif::program` whole-unit JIT/object lowering for all twelve RFC 0006 copy-value operations. Ten focused tests cover canonical wire round trips, forward/mutual calls, bool and nested aggregate interfaces, structural/literal patterns, loop joins and self-edge swaps, host evaluation order/preflight, invalid inputs/projections, compile budgets/frame capacity, multi-file spans, both Mach-O/COFF generation, and macOS direct/response-file link/run. The full fmt/Clippy/test/doc baseline passes (515 tests). The original scalar/owned compatibility adapter remains available. New Windows native execution, ownership-aware objects, public C ABI/headers, full source debug formats, production packaging, and representative performance/conformance remain their own unchecked outcomes.

Roadmap review after this scope: the 113-point total and all decision gates remain appropriate. This accepted copy-value implementation was independent of the source-ownership decisions; its completion does not satisfy any ownership/core-library or production-AOT task. No implicit acceptance or stabilization is recorded.

Commit `a607ed2` supplied [RFC 0007](rfcs/0007-source-ownership-and-core-values.md) and its Japanese mirror: Copy classification, lexical loans/regions, explicit shared/weak/arena syntax, and a finite allocator-aware text/byte/collection type surface. The owner explicitly accepted it on 2026-10-17 JST, recorded in `31ab7ac`, closing the first 5-point decision gate. Cleanup/allocator/unsafe/FFI contracts and the ownership bytecode encoding remain separate decisions. Link/translation metadata checks and repository-consistency tests pass. The accepted finite surface was re-estimated as four independently verifiable outcomes: Text/Bytes (5), Unit/Option/Result/built-in specialization (3), Vec (5), and Map (3). This preserves the former 8+8-point total and all source, platform, conformance, and measurement scope. Next define the remaining contracts and progress accepted RFC 0004 runtime services without claiming source/allocator integration prematurely.

Commit `ec16f8a` adds the runtime shared-ownership engine: real retained owners/upgrades, tracked strong/weak fields, generation/provenance checks, alias-aware read leases, and mutex-linearized whole-payload cycle preflight/publication. Sixteen focused tests include 1,000 independent transitive-closure/count-oracle comparisons, 64 opposite-publication races, 128 weak-upgrade/last-release races, 15,001-node iterative reclamation, and rollback/limit/exhaustion/poison boundaries. The full fmt/Clippy/test/doc baseline passes (531 tests). This closes only the runtime lifecycle/publication outcome. Source/MIR/VM/native ownership integration, arenas, guest cleanup, and the public allocator/FFI contracts remain unchecked. The engine currently uses host Rust storage, including potentially aborting Arc allocation; it does not promise the pending source allocator's recoverable-OOM contract or a sandbox. Roadmap review retains 113 total points and all remaining outcomes; the next decision scope is cleanup/allocator/unsafe/FFI, with source ownership analysis independently unblocked by RFC 0007 acceptance.

Source grammar follow-up (2026-10-17 JST): the lossless parser/formatter now implements the accepted RFC 0007 loan/ownership types, nested built-in argument lists and `::<...>` calls, place borrowing/dereference/assignment, and allocator-supplied arena blocks. Ten new tests cover all ownership fixture prefixes, syntax/CST/comment round trips, limits/recovery, `SRC0024` reserved-name migration, and explicit span-bearing rejection before legacy source binding. The full baseline passes (541 tests). This is prerequisite progress within the still-unchecked 8-point HIR/MIR ownership outcome; it does not establish moves/loans, arena escape safety, built-in typing, execution, or ownership wire support. Roadmap review retains 21/113 completed points and all remaining decision/implementation/measurement gates. Next draft the cleanup/allocator/unsafe/FFI contract for owner disposition and implement ownership-aware source analysis under accepted RFC 0007 independently.

Decision-gate preparation (2026-10-17 JST): proposed [RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md) and its Japanese mirror specify cleanup/failure precedence and return escrow, recoverable budgeted guest allocation, unsafe/capability rules, and the finite generated C status/header/descriptor contract. Proposal layout assertions pass for both tier-1 target triples under Apple Clang 21; four scalar descriptor/SHA-256 vectors are independently verified. These are proposal/cross-target fixtures, not source implementation or native Windows execution. The second 5-point gate remains unchecked until explicit owner disposition. Ownership wire encoding remains a separate required decision; grammar/source analysis can proceed independently under RFC 0007. Review keeps 113 total points and every original ownership/core/platform/conformance/performance deliverable.

## Milestone 3: concurrency and embedding

**Progress: 0 / 96 pt (0.0%).**

Entry: Milestone 2 supplies ownership and cleanup semantics on both execution paths. RFCs 0003 and 0005 remain the accepted semantic baseline; their prototypes do not complete production IDL, scheduler, or sandbox work. The source VM already bounds frames and live cells; this milestone extends accounting across tasks, native code, and host callbacks.

- [ ] **P0 — Accept remaining scheduler/channel/backpressure/error contracts, send/share derivation, and source async syntax without relaxing RFC 0005.** — 5 pt
- [ ] **P0 — Accept the production component IDL/wire profile and embedding ABI, including compatibility, typed failures, callback reentrancy, and cancellation mapping to RFC 0003.** — 5 pt
- [ ] **P0 — Implement structured task lifetimes and deterministic cancellation cleanup.** — 8 pt
- [ ] **P0 — Implement channels with bounded backpressure and cancellation behavior.** — 5 pt
- [ ] **P0 — Enforce actor isolation and accepted data-race capability rules.** — 8 pt
- [ ] **P0 — Implement the versioned component IDL and bounded serialization.** — 8 pt
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
- [ ] **P1 — Build an install/update/channel tool comparable in role to `rustup`.** — 8 pt

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

