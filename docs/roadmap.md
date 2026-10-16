<!-- canonical-language: en; japanese-translation: ja/roadmap.md -->

# Implementation roadmap

[日本語版](ja/roadmap.md)

The roadmap is ordered by risk reduction. Milestones are exit criteria, not calendar commitments.

## Task tracking

Tasks are listed in priority order within each milestone. `P0` tasks block that milestone's exit; `P1` tasks are required follow-ups that should not displace a P0 blocker. A checked task is complete; prototype groundwork completed in an earlier milestone does not complete a broader task in a later milestone.

Cost uses relative effort points rather than elapsed-time promises: `1` is a very small change, `2` is small, `3` is medium, `5` is large, `8` is very large, `13` spans several coordinated changes, and `21` is a program-sized body of work. Progress is `completed points / total points × 100`, rounded to one decimal place. Estimates and completion state must be updated when scope or evidence changes.

Progress snapshot verified against this repository state on 2026-10-16 15:02 JST:

| Milestone | Completed | Total | Progress |
| --- | ---: | ---: | ---: |
| 0 — design baseline (current) | 77 pt | 138 pt | **55.8%** |
| 1 — frontend and bytecode vertical slice | 0 pt | 52 pt | 0.0% |
| 2 — ownership and native AOT | 0 pt | 53 pt | 0.0% |
| 3 — concurrency and embedding | 0 pt | 55 pt | 0.0% |
| 4 — developer toolchain | 0 pt | 58 pt | 0.0% |
| 5 — ecosystem and compatibility | 0 pt | 55 pt | 0.0% |
| 6 — self-hosting | 0 pt | 50 pt | 0.0% |
| 7 — additional frontends | 0 pt | 39 pt | 0.0% |
| **Overall** | **77 pt** | **500 pt** | **15.4%** |

## Milestone 0: design baseline

**Progress: 77 / 138 pt (55.8%).**

### Tasks in priority order

- [x] **P0 — Draft the project charter, goals/non-goals, terminology, RFC process, and compatibility labels.** — 3 pt
- [ ] **P0 — Decide RFC 0001 and RFC 0002, recording acceptance, requested revisions, or rejection.** — 3 pt
- [x] **P0 — Build the typed `i64` MIR → bytecode/VM → Cranelift JIT/AOT vertical slice with host registration and callbacks.** — 13 pt
- [x] **P0 — Add typed CFG validation and representative prototype branches, back-edges, and loop-carried values with VM/JIT/AOT differential coverage.** — 8 pt
- [x] **P0 — Establish the shared system-linker path, macOS AArch64 link-and-run integration, Windows MSVC workflow, and host-independent COFF contracts.** — 8 pt
- [ ] **P0 — Record successful Windows x86-64 CI link/run evidence and a clean benchmark baseline comparable with macOS.** — 8 pt
- [x] **P0 — Prototype deterministic VM fuel, host-call and value-slot limits, cooperative interruption, and absolute deadlines.** — 8 pt
- [x] **P0 — Validate versioned scalar component metadata and explicit capability-gated imports against the typed host registry.** — 5 pt
- [ ] **P0 — Prototype generation-tagged owned/borrowed resource handles and deterministic lifecycle validation.** — 3 pt
- [ ] **P0 — Define the bounded serialized contract, compatibility negotiation, and cross-mode instantiation/conformance tests for the stable embedding/component boundary, making RFC 0003 decision-ready.** — 5 pt
- [ ] **P0 — Extend resource enforcement to heap, recursion, tasks, and native/JIT execution, including callback behavior.** — 13 pt
- [x] **P0 — Prototype strict and less-restrictive alternatives for async cancellation, cyclic graphs, and shared mutable services.** — 8 pt
- [x] **P0 — Draft decision-ready source-level integer and structured-control-flow semantics in RFC 0002.** — 5 pt
- [x] **P0 — Build the versioned benchmark harness through schema 14 and record a clean macOS AArch64 baseline for compile, runtime, startup, code size, logical heap lifetime, and process memory.** — 13 pt
- [ ] **P0 — Select the initial native backend from the completed cross-platform performance, debug-quality, and integration evidence, and record the decision.** — 3 pt
- [x] **P1 — Add variable-size host-allocation execution across direct Rust, VM, JIT, macOS AOT, and Windows object contracts.** — 3 pt
- [ ] **P1 — Add timing and exact requested-payload profiling for the variable-size allocation callback and record the next clean baseline.** — 5 pt
- [ ] **P1 — Add representative allocation/memory and additional CFG workloads, including allocator-overhead and fragmentation evidence.** — 8 pt
- [x] **P1 — Record AArch64 generated-code inspection and the current negative native debug-information baseline.** — 3 pt
- [ ] **P1 — Implement and validate source-span native debug metadata on macOS AArch64 and Windows x86-64.** — 8 pt
- [ ] **P1 — Harden linker discovery, startup/runtime packaging, and production error handling.** — 5 pt

### Evidence and current state

The first component-contract slice now separates portable declarations from embedding-local bindings. Schema-1 in-memory metadata records package identity and version plus named scalar imports/exports; each import declares a capability, while a separate binding table maps its logical name to a process-local `HostFunctionId`. Validation rejects unknown schemas, invalid or duplicate names, import/export collisions, unsupported import types, undeclared or missing bindings, absent grants, and unregistered host functions. Capability checks precede registry lookup so an unauthorized component cannot probe host inventory, and successful validation returns a distinct wrapper with resolved bindings. Seven runtime tests cover the accepted path and key rejection/order contracts. [RFC 0003](rfcs/0003-versioned-component-contracts.md) remains proposed and explicitly leaves serialization, resource handles, compatibility negotiation, canonical lowering, component execution, and boundary-cost evidence open.

Current progress: the first typed function now runs through both verified bytecode and a Cranelift JIT prototype, and the same MIR lowering can emit a host-native object artifact. Minimal typed `i64` host-function registration, a plain script-to-host callback, and an allocation-backed callback run through both execution modes; object output represents callbacks as imports. The VM now composes opt-in deterministic instruction-fuel and host-call budgets with a pre-allocation value-slot limit, cooperative one-way interruption flag, and absolute monotonic deadline. Typed basic blocks with explicit branch, jump, and return terminators are shared across MIR, bytecode, VM, JIT, and AOT; validators enforce target existence, reachability, and SSA dominance, while a fuel-bounded back-edge test covers non-termination. Typed block parameters and edge arguments carry an `i64` through a tested countdown back-edge, and a bounded sum workload carries both a counter and wrapping accumulator through the loop. MIR and bytecode validators enforce entry-block restrictions and edge argument arity/types; VM and JIT results agree, and AOT object emission succeeds. An initial shared `SystemLinker` driver validates ordered link requests, selects the host toolchain command flavor, and reports structured failures; macOS AArch64 integration links and executes add, minimum, plain host-callback, host-allocation, accumulator, and conditional-sum objects through it. The current clean macOS AArch64 [schema 14 benchmark baseline](benchmarks.md) covers add, minimum, loop-carried countdown, two-value accumulator, and conditional-sum lowering; JIT/AOT compilation; code, object, and linked-executable size; direct/VM/JIT calls; plain, single-allocation, 64-allocation-burst, and allocation-churn host callbacks; three requested-payload heap profiles; unset-flag interruption-check and unexpired-deadline comparisons; system linking; single-launch plus batched warm-cache AOT process startup; and 31 linked-process peak-memory samples from 1,654,784 to 1,671,168 bytes. Remote Windows evidence is not yet recorded. All three memory-model stress cases now have executable strict models: no borrows across suspension with deterministic cancellation cleanup, explicit shared/weak and arena-scoped cyclic graphs, and actor-isolated shared mutable services with moved messages. Generated-code inspection confirms a two-instruction AArch64 add, compare-and-branch minimum, and position-independent GOT import for the host callback. RFC 0001 remains proposed; less restrictive alternatives, broader embedding limits and native enforcement, production linker hardening and startup/runtime packaging, Windows x86-64 linking, execution, benchmark, and CI evidence, source allocation semantics and representative allocation/memory workloads, acceptance and implementation of proposed source-level control-flow semantics, additional CFG workloads, allocator-overhead measurement, native debug-metadata implementation, and cross-platform debug-quality validation, and the backend decision remain open.

A post-CFG reinspection confirms unchanged minimum code and object sizes and instruction sequence. `SystemLinker` now models `Cc` and `MsvcLink` command flavors, selects the matching host default, supports an explicit driver with ordered prefix arguments, exposes the complete command, and retains it in structured execution failures. Ordered library search paths and logical library names lower to flavor-specific arguments, and validation rejects empty object paths, search paths, and names. Explicit response-file linking uses GNU-escaped UTF-8 for `Cc` and Windows-escaped, BOM-prefixed UTF-16LE for `MsvcLink`; focused tests cover serialization and validation, and the macOS AArch64 integration executes both direct and response-file links. Toolchain discovery and broader target policy remain open. Targeted object emission now constructs an ISA from an explicit triple, and a host-independent test validates the format, architecture, export, and import of `x86_64-pc-windows-msvc` COFF output. A cfg-gated MSVC direct/response-file object-link-execute test and SHA-pinned Windows Server 2025 workflow are configured, but no Windows runner result is recorded for this commit, so Windows linking and execution evidence remain open.

Initial host-toolchain discovery now scans `PATH` in deterministic candidate order, rejects non-executable Unix files, reports the searched path and candidates structurally, and drives the macOS integration. The cfg-gated MSVC integration uses the same discovery after the developer environment initializes `PATH`; Visual Studio, SDK, and compiler installation discovery outside an initialized environment remains open.

The initial tier-1 linker policy is now explicit: backend-neutral `LinkTarget` values own the exact `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` triples and map them to command flavor, default program, and ordered discovery candidates. Explicit-target construction/discovery, the COFF object test, and both platform integrations share the mapping, while unsupported triples fail structurally. Policies for additional targets, installation discovery, and startup/runtime inputs remain open.

The schema 3 harness now measures lowering, JIT/AOT compilation, direct/VM/JIT calls, artifact sizes, and system linking for a bounded countdown back-edge in addition to add and minimum. It versions the new fields and renamed link series explicitly. A clean macOS AArch64 schema 3 baseline is recorded. Windows CI validates and uploads the corresponding JSON for 30 days, but no remote runner result is recorded yet; allocation, broader CFG coverage, and Windows benchmark interpretation remain open.

The schema 4 harness now records linked AOT executable size and repeated warm-cache process startup on both tier-1 evidence paths. It separates this row from system-link time and documents that process creation, loader work, the tiny harness, output collection, and termination are included. A clean macOS AArch64 result records a 16,992-byte executable and 1.511 ms median startup, but a 49.824 ms outlier raises startup RSD to 276.388%, so the method needs refinement before setting a budget. Windows CI validates both rows and a positive executable size; memory, debug quality, allocation, broader CFG coverage, remote Windows evidence, and cross-platform interpretation remain open.

The schema 5 harness preserves the single-launch startup row and adds a configurable batched row that reports per-launch elapsed time across sequential launches. This separates tail-sensitive latency evidence from a less noise-sensitive amortized estimate. A clean macOS AArch64 result keeps similar 1.487 ms single and 1.499 ms batched medians while reducing RSD from 256.272% to 4.688% for the ten-launch batch. Windows CI validates the configured batch size and both rows; remote Windows comparison and the remaining evidence gaps stay open.

The schema 6 harness adds the two-value accumulator to lowering, JIT/AOT compilation, direct/VM/JIT calls, generated code and object sizes, system linking, linked-executable size, and both startup rows. Platform series are renamed to reflect the four-object link set, and Windows CI validates the new accumulator and platform fields. A clean macOS AArch64 result records all 30 rows and a 17,056-byte linked harness; remote Windows evidence remains open alongside allocation, additional source-level control flow, memory, debug-quality, and backend-decision work.

A post-schema-6 conditional-sum CFG prototype adds an internal branch to the bounded accumulator loop and merges the add/skip paths through block parameters before the shared back-edge. VM/JIT differential tests cover both paths, macOS AArch64 direct/response-file integrations execute the fifth AOT object, and host-independent Windows x86-64 COFF validation checks its export; the cfg-gated MSVC integration mirrors that link-and-run path. Source-level control-flow semantics and further CFG/allocation workloads remain open.

The schema 7 harness now measures the conditional-sum workload across lowering, JIT/AOT compilation, direct/VM/JIT calls, generated-code/object size, and the expanded five-object system-link/startup set. It adds six conditional-sum rows and renames the platform series and linked-executable field for that set; Windows CI validates the schema 7 contract. A clean macOS AArch64 result records all 36 rows, the 44-byte JIT body, 400-byte object, and 17,104-byte linked harness. Source-level semantics, allocation and memory workloads, debug-quality evidence, further CFG coverage, and remote Windows execution and benchmark evidence remain open.

The async semantics experiment now compares the strict default `RejectAll` suspension policy with an explicit `AllowSharedFrameBorrows` alternative. The alternative preserves a shared task-frame borrow across suspend/resume, still rejects an exclusive borrow with `SEM0007`, and ends active shared borrows before cancellation drops owned values. This narrows the less-restrictive-design question without accepting semantics; pinned or exclusive borrows, source-level frame-residency proofs, destructor failure, VM/native traces, and runtime cost remain open.

The cyclic-graph experiment now compares the default `Reject` shared-cycle policy with an explicit `CollectUnreachable` alternative. The collector permits strong cycles, traces strong edges from remaining live roots after each root drop, preserves a cycle while any root reaches it, and reclaims it in reverse creation order after the last root disappears; weak edges never retain a target. This makes the strongest competing design executable without accepting it. Production collection triggers and latency, incremental/concurrent behavior, allocation and peak-memory cost, finalization, weak-reference ordering, and destruction observability remain open.

The shared-service experiment now compares the default `OwnerIsolated` mutation policy with an explicit `ExclusiveCapability` alternative. Under the alternative, any task with a valid handle may mutate while holding the single active capability; tasks without handles and concurrent mutators are rejected with structured diagnostics, and cancellation waits for a non-owner mutation boundary before cleanup. This makes a lock-like competing design executable without accepting it. Read/write and atomic capabilities, capability derivation, scheduling fairness, priority inversion, poisoning, deadlock, reentrancy, panic handling, backpressure, VM/native traces, and runtime cost remain open.

The host-allocation boundary experiment began with a single-allocation callback and a burst callback that keeps 64 boxes with 512 payload bytes live before summing and dropping them. VM and JIT differential tests use Rust `Box` callbacks, while macOS AArch64 direct/response-file AOT integration and the configured Windows MSVC path use corresponding C `malloc`/`free` callbacks imported by distinct objects. Schema 8 adds direct Rust, VM, and JIT runtime rows for the single-allocation callback, and schema 11 adds the same three rows for the burst without changing the AOT link/startup set; Windows CI validates all six rows. The clean schema 11 macOS AArch64 baseline records single-allocation medians of 12.272 ns direct, 190.573 ns through the VM, and 14.123 ns through JIT, plus burst medians of 782.624 ns, 962.329 ns, and 786.270 ns. Source allocation semantics, ownership, allocator/collector policy, memory limits, allocator-overhead and peak-memory accounting, broader representative workloads, remote Windows evidence, and cross-platform cost remain open.

The VM resource-control prototype now composes optional instruction-fuel and host-call budgets with a value-slot limit, shared one-way interruption flag, and absolute monotonic deadline. Instructions/terminators and entered callbacks consume their respective counters, checks happen before the corresponding side effect, and the verified register count is checked before register-file allocation. An already-set interruption or expired deadline stops before that allocation; later conditions are observed at instruction, terminator, and pre-callback safe points, with both consumed counters in the structured error and interruption taking precedence. A callback-triggered request or deadline overrun is observed after that callback completes. Successful limited execution reports both dynamic counts and the static slot footprint. Zero host-call budget suppresses the callback, and an undersized slot limit rejects execution, both with structured errors; existing convenience APIs remain unbounded, non-interruptible, and without a deadline. The heap, recursion, tasks, callback-internal preemption, byte-level accounting, and native/JIT enforcement remain open.

[RFC 0002](rfcs/0002-prototype-control-flow.md) now gives the implemented integer/CFG prototype a proposed source-level interpretation. It defines wrapping `i64` addition, signed comparison, left-to-right evaluation, mutable locals, structured `if`/`else` and `while`, explicit returns, and lowering of live values through typed block arguments. The focused RFC is ready for an owner decision but remains proposed; parser, HIR, formatter, source diagnostics, and source-to-MIR implementation remain open.

The schema 9 harness adds an unset-flag `vm_countdown_interruptible_call` row beside the existing VM countdown row, allowing direct measurement of the incremental safe-point acquire-load cost without changing the input sequence or bytecode. The current clean schema 14 run retains that row and records ordinary and interruption-enabled VM countdown medians of 1.611 us and 1.641 us. The 29.868 ns (1.854%) increase is smaller than the interruption row's 15.866% RSD, so it shows no measurable regression and does not establish an overhead budget. Schema 14 is the current CI contract; no remote Windows result is recorded yet.

The schema 10 harness adds `vm_countdown_deadline_call`, which executes the same countdown bytecode and inputs with an absolute monotonic deadline set one hour after harness preparation. It isolates the steady-state clock-check cost while the deadline remains unexpired. The clean schema 14 macOS AArch64 run retains the row; ordinary and unexpired-deadline VM countdown medians are 1.611 us and 3.320 us, a 1.709 us or 106.071% increase. Windows CI requires the row, but no remote Windows result is recorded yet.

The schema 11 harness adds direct Rust, VM, and JIT rows for the 64-allocation burst. A clean historical 31-sample macOS AArch64 run records all 44 unique rows and burst medians of 782.624 ns, 962.329 ns, and 786.270 ns; macOS AOT plus host-independent Windows COFF tests cover the new callback import. Windows CI retains all three rows under schema 14, but no remote Windows result is recorded yet.

The schema 12 harness keeps those 44 timing rows and adds deterministic requested-payload heap profiles for the host allocation callbacks. The single workload records one allocation with 8 total/peak live bytes and zero bytes after completion; the burst records 64 allocations with 512 total/peak live bytes and zero afterward. A clean 31-sample macOS AArch64 run records both exact profiles, all 44 unique timing rows, and burst medians of 768.283 ns, 946.601 ns, and 771.625 ns. Tests and Windows CI enforce the exact values and scope. This closes the first logical heap-lifetime measurement without overstating the evidence; schema 13 now supplies the first local process envelope, while allocator metadata and fragmentation, VM/JIT internal allocations, representative application allocation, and remote Windows evidence remain open.

The schema 13 harness retains all schema 12 fields and adds a process-level peak resident-memory distribution for the linked, allocation-free AOT harness. Each sample launches a separate process; macOS reports `ru_maxrss` through `/usr/bin/time -l`, while Windows reports `Process.PeakWorkingSet64` through PowerShell. Both are byte-valued, OS-specific process envelopes that include shared pages and loader/runtime state. Unit tests and a native smoke run validate the macOS parser and distribution, and Windows CI requires a positive ordered 31-sample row. This implements the first process-level peak-memory mechanism without claiming allocator-overhead or representative-workload evidence. The historical clean schema 13 macOS AArch64 result records all 31 samples as 1,671,168 bytes with 0.000% RSD; the Windows baseline remains open.

The schema 14 harness retains every schema 13 field and adds direct Rust, VM, and JIT timing rows for the allocation-churn callback plus an exact requested-payload heap profile. The deterministic sequence allocates 64 boxed `i64` values, frees every other allocation, fills the 32 holes with replacements, and then consumes the final 64, producing `(96 allocations, 768 total bytes, 512 peak live bytes, 0 live bytes afterward)`. VM/JIT differential tests cover wrapping results, macOS AArch64 direct/response-file integration executes the matching C `malloc`/`free` import, host-independent Windows COFF validation checks `lang_host_i64_3`, and the cfg-gated MSVC integration and Windows workflow retain the contract. A clean 31-sample macOS AArch64 run records all 47 unique timing rows, all three exact heap profiles, direct Rust/VM/JIT churn medians of 1.192 us, 1.348 us, and 1.173 us, and linked-process peak memory from 1,654,784 to 1,671,168 bytes. Variable-size and representative workloads, allocator overhead/fragmentation, and remote Windows execution remain open.

An unbenchmarked variable-size allocation callback now holds 16 boxed `i64` slices ranging from 8 to 128 payload bytes, for 1,088 requested payload bytes live at once, and returns a deterministic wrapping fold. VM/JIT differential tests cover integer extremes, macOS AArch64 direct/response-file AOT integration executes the matching C import, host-independent Windows COFF validation checks `lang_host_i64_4`, and the cfg-gated MSVC path mirrors link-and-run coverage. Timing and exact requested-payload profiling, representative allocation workloads, allocator overhead/fragmentation, and remote Windows execution remain open.

A clean macOS AArch64 [AOT debug-information inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) regenerates add, minimum, and host-callback objects from commit `013da31f9f0fb752aad5ec887f8cf5a5ecf1650d`. Linker-visible symbols remain, but all three objects have no `__debug_*` section, DWARF compile unit, line entry, source variable, parameter, or local. This supplies negative debug-quality evidence rather than completing the criterion: source-span mapping, linked/dSYM output, unwind metadata, Windows CodeView/PDB, JIT debugger registration, optimized stepping, and representative source-program validation remain open.

Exit: written semantics for the prototype subset and benchmark evidence for the initial backend choice.

## Milestone 1: frontend and bytecode vertical slice

**Progress: 0 / 52 pt (0.0%).** Milestone 0 prototypes are groundwork; these tasks require the complete source-driven vertical slice.

- [ ] **P0 — Implement a lossless parser with error recovery and source spans.** — 8 pt
- [ ] **P0 — Implement name resolution, modules, primitive types, functions, records, variants, and pattern matching.** — 13 pt
- [ ] **P0 — Add structured diagnostics with stable codes and fix-its.** — 5 pt
- [ ] **P0 — Integrate source semantics with validated typed MIR.** — 5 pt
- [ ] **P0 — Complete register bytecode, verifier, VM, REPL/script runner, and host-function registration for source programs.** — 8 pt
- [ ] **P1 — Implement the formatter and parser/formatter round-trip tests.** — 5 pt
- [ ] **P1 — Build the initial incremental query graph shared with an LSP prototype.** — 8 pt

Exit: small programs run as scripts with deterministic behavior and useful IDE diagnostics.

## Milestone 2: ownership and native AOT

**Progress: 0 / 53 pt (0.0%).**

- [ ] **P0 — Implement owned values, moves, lexical borrows, deterministic destruction, and narrow unsafe operations.** — 13 pt
- [ ] **P0 — Implement C ABI import/export and generated headers.** — 8 pt
- [ ] **P0 — Complete Cranelift object generation for macOS AArch64 and Windows x86-64.** — 8 pt
- [ ] **P0 — Build the representative differential VM/native execution suite.** — 8 pt
- [ ] **P1 — Implement allocator-aware standard containers.** — 8 pt
- [ ] **P1 — Provide native debug metadata sufficient for breakpoints and stack traces.** — 8 pt

Exit: representative command-line programs match VM semantics and performance is measured against equivalent Rust/C implementations.

## Milestone 3: concurrency and embedding

**Progress: 0 / 55 pt (0.0%).**

- [ ] **P0 — Implement structured tasks, cancellation, channels, actor isolation, and data-race capability rules.** — 13 pt
- [ ] **P0 — Stabilize the C embedding API and generated bindings.** — 8 pt
- [ ] **P0 — Implement the versioned component IDL, resource handles, and capability imports.** — 13 pt
- [ ] **P0 — Support mixed native/VM calls, callbacks, errors, stack traces, and cancellation.** — 8 pt
- [ ] **P0 — Enforce script resource limits and interruption.** — 8 pt
- [ ] **P1 — Ship trusted-native and verified-script plugin examples.** — 5 pt

Exit: a native host can load a user script, expose a typed API, call the script, receive callbacks, and enforce capabilities without IPC.

## Milestone 4: developer toolchain

**Progress: 0 / 58 pt (0.0%).**

- [ ] **P0 — Implement `lang` build/package/test/fmt/lint/doc commands.** — 8 pt
- [ ] **P0 — Implement the manifest and lockfile with deterministic resolution, checksums, offline mode, and target conditions.** — 13 pt
- [ ] **P0 — Complete LSP navigation, completion, rename, semantic tokens, and code actions.** — 13 pt
- [ ] **P0 — Implement a DAP adapter with mixed native/VM stacks.** — 8 pt
- [ ] **P1 — Add compile-time/runtime profiling, allocation traces, and rebuild explanations.** — 8 pt
- [ ] **P1 — Build an install/update/channel tool comparable in role to `rustup`.** — 8 pt

Exit: a new developer can install one tool, create a package, develop in an editor, test, debug, and produce a reproducible binary.

## Milestone 5: ecosystem and compatibility

**Progress: 0 / 55 pt (0.0%).**

- [ ] **P0 — Define editions and deprecation policy, then build the migration/fix tool and compatibility matrix.** — 13 pt
- [ ] **P0 — Complete the public-registry threat model before accepting uploads.** — 8 pt
- [ ] **P0 — Implement signed release manifests and package provenance.** — 8 pt
- [ ] **P0 — Add a WebAssembly component target for sandboxed portable plugins.** — 13 pt
- [ ] **P1 — Provide documentation hosting and package-metadata search.** — 5 pt
- [ ] **P1 — Evaluate an LLVM release backend using published benchmarks and debug-quality criteria.** — 8 pt

Exit: the language can evolve without silently breaking source, package, bytecode, or component consumers.

## Milestone 6: self-hosting

**Progress: 0 / 50 pt (0.0%).**

- [ ] **P0 — Define and freeze the self-hosting subset.** — 5 pt
- [ ] **P0 — Implement the frontend and core compiler stages in the language.** — 21 pt
- [ ] **P0 — Make the stage 0 Rust compiler build stage 1 and stage 1 build stage 2.** — 13 pt
- [ ] **P0 — Compare reproducible outputs on both tier-1 platforms.** — 8 pt
- [ ] **P1 — Document bootstrap trust and disaster-recovery procedures.** — 3 pt

Exit: stage 2 passes the same conformance suite as stage 0 and can build the supported toolchain from a documented bootstrap chain.

## Milestone 7: additional frontends

**Progress: 0 / 39 pt (0.0%).**

- [ ] **P0 — Demonstrate that a distinct high-level or domain-specific frontend provides requirements that libraries, profiles, or syntax sugar cannot serve.** — 5 pt
- [ ] **P0 — Implement a frontend-specific HIR and lower through shared MIR only where the core represents semantics faithfully.** — 13 pt
- [ ] **P0 — Use the component ABI where memory, effect, or runtime models differ.** — 8 pt
- [ ] **P0 — Add cross-language debugging, package metadata, and conformance tests before declaring interoperability stable.** — 13 pt

Exit: a second frontend adds clear user value without weakening the first language’s safety or making the shared IR an unmaintainable union of unrelated semantics.

