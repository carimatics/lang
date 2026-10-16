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

Progress snapshot verified against this repository state on 2026-10-17 (JST):

| Milestone | Completed | Total | Progress |
| --- | ---: | ---: | ---: |
| 0 — design baseline | 209 pt | 209 pt | **100.0%** |
| 1 — frontend and bytecode vertical slice (current) | 5 pt | 86 pt | 5.8% |
| 2 — ownership and native AOT | 0 pt | 92 pt | 0.0% |
| 3 — concurrency and embedding | 0 pt | 91 pt | 0.0% |
| 4 — developer toolchain | 0 pt | 63 pt | 0.0% |
| 5 — ecosystem and compatibility | 0 pt | 65 pt | 0.0% |
| 6 — self-hosting | 0 pt | 74 pt | 0.0% |
| 7 — additional frontends | 0 pt | 55 pt | 0.0% |
| **Overall** | **214 pt** | **735 pt** | **29.1%** |

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
- **Next active P0:** decide the Milestone 1 lexical specification before implementing source tokenization and parsing; the structured source-diagnostic foundation is complete.

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

## Milestone 1: frontend and bytecode vertical slice (current)

**Progress: 5 / 86 pt (5.8%).** Re-estimated in the 2026-10-17 review to include missing design gates and call-frame implementation. RFC 0002 alone does not specify modules, calls, aggregates, REPL state, or serialized bytecode. Implementation of each extension waits for its accepted contract.

- [x] **P0 — Establish structured source diagnostics with stable codes, labels, and fix-its.** — 5 pt
- [ ] **P0 — Accept the RFC 0002 lexical specification: identifier/keyword rules, whitespace/comments, signed-literal tokenization, source encoding, and malformed-token diagnostics, with fixtures.** — 3 pt
- [ ] **P0 — Implement a lossless parser with error recovery and source spans.** — 8 pt
- [ ] **P0 — Accept source module and function-call semantics, including visibility, cycles, entry points, host calls, call frames, and recursion limits beyond RFC 0002.** — 5 pt
- [ ] **P0 — Implement scopes, name resolution, and module loading.** — 5 pt
- [ ] **P0 — Type-check primitive expressions, functions, and structured control flow.** — 8 pt
- [ ] **P0 — Accept records, variants, patterns, and exhaustiveness semantics plus their typed MIR/value representation and compatibility fixtures.** — 5 pt
- [ ] **P0 — Type-check records, variants, patterns, and exhaustiveness.** — 8 pt
- [ ] **P0 — Lower resolved and typed source through HIR into validated typed MIR while preserving spans.** — 5 pt
- [ ] **P0 — Promote the prototype into versioned register bytecode with encoding, decoding, and verification.** — 5 pt
- [ ] **P0 — Implement accepted source calls, call frames, returns, and bounded recursion in MIR, bytecode, and VM with failure/stack tests.** — 8 pt
- [ ] **P0 — Execute the source-language subset in the VM with typed host registration.** — 5 pt
- [ ] **P0 — Provide deterministic script and REPL entry points.** — 3 pt
- [ ] **P1 — Implement the formatter and parser/formatter round-trip tests.** — 5 pt
- [ ] **P1 — Build the initial incremental query graph shared with an LSP prototype.** — 8 pt

Evidence verified 2026-10-17 (JST): commit `a4f135c` adds `lang-diagnostics` with immutable source snapshots, UTF-8-validated spans, Unicode/UTF-16 positions, accepted `SRC0001`–`SRC0007` identities, primary/secondary labels, notes, and atomic fix previews. Five focused tests cover positions, stale/foreign snapshots, code identities, cross-file rendering, edit conflicts, and control-character escaping. The full fmt/Clippy/test/doc baseline passes. Source diagnostic producers remain part of their parser/resolution/type-checking tasks. Roadmap review: the next dependency is the lexical acceptance gate; no task was removed or deferred, and the 86-point scope remains appropriate.

Exit: small programs run as scripts with deterministic behavior and useful IDE diagnostics.

## Milestone 2: ownership and native AOT

**Progress: 0 / 92 pt (0.0%).**

- [ ] **P0 — Implement owned values, moves, and lexical borrow checking in typed HIR and MIR.** — 8 pt
- [ ] **P0 — Implement RFC 0004 shared/weak retain-release, retained weak upgrades, and atomic cycle-preflight/publication with rollback and race tests.** — 8 pt
- [ ] **P0 — Implement RFC 0004 lexical arenas, escape rejection, stale-reference checks, and deterministic teardown with compile-fail and runtime tests.** — 5 pt
- [ ] **P0 — Accept unspecified error/panic cleanup, unsafe/FFI validity, and allocator contracts before implementing their public interfaces.** — 5 pt
- [ ] **P0 — Elaborate deterministic drops and define destruction behavior on return, error, and panic paths.** — 8 pt
- [ ] **P0 — Define narrow unsafe boundaries and enforce FFI validity rules.** — 5 pt
- [ ] **P0 — Implement C ABI import/export and generated headers.** — 8 pt
- [ ] **P0 — Complete source-program object generation for macOS AArch64 and Windows x86-64.** — 8 pt
- [ ] **P0 — Harden linker discovery, startup/runtime packaging, and production error handling.** — 5 pt
- [ ] **P0 — Build the representative differential VM/native conformance suite.** — 8 pt
- [ ] **P0 — Measure representative native workloads against equivalent Rust/C implementations.** — 8 pt
- [ ] **P1 — Implement allocator-aware standard containers.** — 8 pt
- [ ] **P1 — Emit and validate source-span native debug metadata on both tier-1 platforms.** — 8 pt

Exit: representative command-line programs match VM semantics and performance is measured against equivalent Rust/C implementations.

## Milestone 3: concurrency and embedding

**Progress: 0 / 91 pt (0.0%).**

- [ ] **P0 — Accept remaining scheduler/channel/backpressure/error contracts, send/share derivation, and source async syntax without relaxing RFC 0005.** — 5 pt
- [ ] **P0 — Implement structured task lifetimes and deterministic cancellation cleanup.** — 8 pt
- [ ] **P0 — Implement channels with bounded backpressure and cancellation behavior.** — 5 pt
- [ ] **P0 — Enforce actor isolation and accepted data-race capability rules.** — 8 pt
- [ ] **P0 — Stabilize the C embedding API and generated bindings.** — 8 pt
- [ ] **P0 — Implement the versioned component IDL and bounded serialization.** — 8 pt
- [ ] **P0 — Implement canonical owned/borrowed resource adapters.** — 8 pt
- [ ] **P0 — Enforce capability-gated component imports.** — 5 pt
- [ ] **P0 — Support mixed native/VM calls and callbacks with typed errors.** — 8 pt
- [ ] **P0 — Preserve stack traces and cancellation across native/VM boundaries.** — 5 pt
- [ ] **P0 — Enforce heap/allocation limits, including host-callback accounting.** — 5 pt
- [ ] **P0 — Enforce recursion-depth and live-task limits.** — 5 pt
- [ ] **P0 — Enforce cooperative interruption across VM, JIT, AOT, and callback boundaries.** — 8 pt
- [ ] **P1 — Ship trusted-native and verified-script plugin examples.** — 5 pt

Exit: a native host can load a user script, expose a typed API, call the script, receive callbacks, and enforce capabilities without IPC.

## Milestone 4: developer toolchain

**Progress: 0 / 63 pt (0.0%).**

- [ ] **P0 — Implement `lang` build, run, and test orchestration.** — 5 pt
- [ ] **P0 — Define package/workspace manifests and target conditions.** — 5 pt
- [ ] **P0 — Implement deterministic dependency resolution, lockfiles, checksums, and offline mode.** — 8 pt
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

- [ ] **P0 — Demonstrate that a distinct frontend provides requirements that libraries, profiles, or syntax sugar cannot serve.** — 5 pt
- [ ] **P0 — Specify the frontend semantics and its shared-MIR/component boundary.** — 5 pt
- [ ] **P0 — Implement frontend-specific parsing, HIR, and semantic analysis.** — 8 pt
- [ ] **P0 — Lower only faithfully represented semantics through the shared typed MIR.** — 8 pt
- [ ] **P0 — Implement component adapters where memory, effect, or runtime models differ.** — 8 pt
- [ ] **P0 — Add cross-language package metadata and version resolution.** — 5 pt
- [ ] **P0 — Add differential cross-language conformance tests.** — 8 pt
- [ ] **P0 — Add cross-language stack traces, source mapping, and debugger integration.** — 8 pt

Exit: a second frontend adds clear user value without weakening the first language’s safety or making the shared IR an unmaintainable union of unrelated semantics.

