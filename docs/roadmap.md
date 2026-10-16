<!-- canonical-language: en; japanese-translation: ja/roadmap.md -->

# Implementation roadmap

[日本語版](ja/roadmap.md)

The roadmap is ordered by risk reduction. Milestones are exit criteria, not calendar commitments.

## Milestone 0: design baseline

Current progress: the first typed function now runs through both verified bytecode and a Cranelift JIT prototype, and the same MIR lowering can emit a host-native object artifact. Minimal typed `i64` host-function registration, a plain script-to-host callback, and an allocation-backed callback run through both execution modes; object output represents callbacks as imports. The VM now composes opt-in deterministic instruction-fuel and host-call budgets with a pre-allocation value-slot limit, cooperative one-way interruption flag, and absolute monotonic deadline. Typed basic blocks with explicit branch, jump, and return terminators are shared across MIR, bytecode, VM, JIT, and AOT; validators enforce target existence, reachability, and SSA dominance, while a fuel-bounded back-edge test covers non-termination. Typed block parameters and edge arguments carry an `i64` through a tested countdown back-edge, and a bounded sum workload carries both a counter and wrapping accumulator through the loop. MIR and bytecode validators enforce entry-block restrictions and edge argument arity/types; VM and JIT results agree, and AOT object emission succeeds. An initial shared `SystemLinker` driver validates ordered link requests, selects the host toolchain command flavor, and reports structured failures; macOS AArch64 integration links and executes add, minimum, plain host-callback, host-allocation, accumulator, and conditional-sum objects through it. The current clean macOS AArch64 [schema 12 benchmark baseline](benchmarks.md) covers add, minimum, loop-carried countdown, two-value accumulator, and conditional-sum lowering; JIT/AOT compilation; code, object, and linked-executable size; direct/VM/JIT calls; plain, single-allocation, and 64-allocation-burst host callbacks; requested-payload heap profiles; unset-flag interruption-check and unexpired-deadline comparisons; system linking; and single-launch plus batched warm-cache AOT process startup. The schema 13 harness additionally records an OS-specific peak resident-memory distribution for the linked AOT process, with a native macOS smoke test and a Windows CI contract; a clean schema 13 baseline is not yet recorded. All three memory-model stress cases now have executable strict models: no borrows across suspension with deterministic cancellation cleanup, explicit shared/weak and arena-scoped cyclic graphs, and actor-isolated shared mutable services with moved messages. Generated-code inspection confirms a two-instruction AArch64 add, compare-and-branch minimum, and position-independent GOT import for the host callback. RFC 0001 remains proposed; less restrictive alternatives, broader embedding limits and native enforcement, production linker hardening and startup/runtime packaging, Windows x86-64 linking, execution, benchmark, and CI evidence, source allocation semantics and representative allocation/memory workloads, acceptance and implementation of proposed source-level control-flow semantics, additional CFG workloads, allocator-overhead measurement and clean schema 13 process-memory baselines, native debug-metadata implementation, and cross-platform debug-quality validation, and the backend decision remain open.

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

The host-allocation boundary experiment now has a single-allocation callback and a burst callback that keeps 64 boxes with 512 payload bytes live before summing and dropping them. VM and JIT differential tests use Rust `Box` callbacks, while macOS AArch64 direct/response-file AOT integration and the configured Windows MSVC path use corresponding C `malloc`/`free` callbacks imported by distinct objects. Schema 8 adds direct Rust, VM, and JIT runtime rows for the single-allocation callback, and schema 11 adds the same three rows for the burst without changing the AOT link/startup set; Windows CI validates all six rows. The clean schema 11 macOS AArch64 baseline records single-allocation medians of 12.272 ns direct, 190.573 ns through the VM, and 14.123 ns through JIT, plus burst medians of 782.624 ns, 962.329 ns, and 786.270 ns. Source allocation semantics, ownership, allocator/collector policy, memory limits, allocator-overhead and peak-memory accounting, broader representative workloads, remote Windows evidence, and cross-platform cost remain open.

The VM resource-control prototype now composes optional instruction-fuel and host-call budgets with a value-slot limit, shared one-way interruption flag, and absolute monotonic deadline. Instructions/terminators and entered callbacks consume their respective counters, checks happen before the corresponding side effect, and the verified register count is checked before register-file allocation. An already-set interruption or expired deadline stops before that allocation; later conditions are observed at instruction, terminator, and pre-callback safe points, with both consumed counters in the structured error and interruption taking precedence. A callback-triggered request or deadline overrun is observed after that callback completes. Successful limited execution reports both dynamic counts and the static slot footprint. Zero host-call budget suppresses the callback, and an undersized slot limit rejects execution, both with structured errors; existing convenience APIs remain unbounded, non-interruptible, and without a deadline. The heap, recursion, tasks, callback-internal preemption, byte-level accounting, and native/JIT enforcement remain open.

[RFC 0002](rfcs/0002-prototype-control-flow.md) now gives the implemented integer/CFG prototype a proposed source-level interpretation. It defines wrapping `i64` addition, signed comparison, left-to-right evaluation, mutable locals, structured `if`/`else` and `while`, explicit returns, and lowering of live values through typed block arguments. The focused RFC is ready for an owner decision but remains proposed; parser, HIR, formatter, source diagnostics, and source-to-MIR implementation remain open.

The schema 9 harness adds an unset-flag `vm_countdown_interruptible_call` row beside the existing VM countdown row, allowing direct measurement of the incremental safe-point acquire-load cost without changing the input sequence or bytecode. The current clean schema 12 run retains that row and records ordinary and interruption-enabled VM countdown medians of 1.606 us and 1.603 us. The 3.424 ns (0.213%) decrease is smaller than the rows' 10.380% and 0.263% RSD values, so it shows no measurable regression and does not establish an overhead budget. Schema 13 is the current CI contract; no remote Windows result is recorded yet.

The schema 10 harness adds `vm_countdown_deadline_call`, which executes the same countdown bytecode and inputs with an absolute monotonic deadline set one hour after harness preparation. It isolates the steady-state clock-check cost while the deadline remains unexpired. The clean schema 12 macOS AArch64 run retains the row; ordinary and unexpired-deadline VM countdown medians are 1.606 us and 3.314 us, a 1.708 us or 106.379% increase. Windows CI requires the row, but no remote Windows result is recorded yet.

The schema 11 harness adds direct Rust, VM, and JIT rows for the 64-allocation burst. A clean historical 31-sample macOS AArch64 run records all 44 unique rows and burst medians of 782.624 ns, 962.329 ns, and 786.270 ns; macOS AOT plus host-independent Windows COFF tests cover the new callback import. Windows CI retains all three rows under schema 13, but no remote Windows result is recorded yet.

The schema 12 harness keeps those 44 timing rows and adds deterministic requested-payload heap profiles for the host allocation callbacks. The single workload records one allocation with 8 total/peak live bytes and zero bytes after completion; the burst records 64 allocations with 512 total/peak live bytes and zero afterward. A clean 31-sample macOS AArch64 run records both exact profiles, all 44 unique timing rows, and burst medians of 768.283 ns, 946.601 ns, and 771.625 ns. Tests and Windows CI enforce the exact values and scope. This closes the first logical heap-lifetime measurement and local-baseline gaps without overstating the evidence: allocator metadata and fragmentation, VM/JIT internal allocations, process RSS/peak memory, representative application allocation, and remote Windows evidence remain open.

The schema 13 harness retains all schema 12 fields and adds a process-level peak resident-memory distribution for the linked, allocation-free AOT harness. Each sample launches a separate process; macOS reports `ru_maxrss` through `/usr/bin/time -l`, while Windows reports `Process.PeakWorkingSet64` through PowerShell. Both are byte-valued, OS-specific process envelopes that include shared pages and loader/runtime state. Unit tests and a native smoke run validate the macOS parser and distribution, and Windows CI requires a positive ordered 31-sample row. This implements the first process-level peak-memory mechanism without claiming allocator-overhead or representative-workload evidence. Clean schema 13 baselines on macOS and Windows remain open.

A clean macOS AArch64 [AOT debug-information inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) regenerates add, minimum, and host-callback objects from commit `013da31f9f0fb752aad5ec887f8cf5a5ecf1650d`. Linker-visible symbols remain, but all three objects have no `__debug_*` section, DWARF compile unit, line entry, source variable, parameter, or local. This supplies negative debug-quality evidence rather than completing the criterion: source-span mapping, linked/dSYM output, unwind metadata, Windows CodeView/PDB, JIT debugger registration, optimized stepping, and representative source-program validation remain open.

- Accept goals, non-goals, and terminology.
- Establish the RFC process and compatibility labels.
- Prototype three memory-model stress cases: cyclic graphs, async borrows/cancellation, and shared mutable services.
- Prototype a typed function through both bytecode and native lowering.
- Select initial tier-1 architecture pairs and linker strategy.

Exit: written semantics for the prototype subset and benchmark evidence for the initial backend choice.

## Milestone 1: frontend and bytecode vertical slice

- Lossless parser with error recovery and source spans.
- Name resolution, modules, primitive types, functions, records, variants, and pattern matching.
- Structured diagnostics with stable codes and fix-its.
- Typed MIR with validation.
- Register bytecode, verifier, VM, REPL/script runner, and host function registration.
- Formatter and parser/formatter round-trip tests.
- Initial incremental query graph shared with an LSP prototype.

Exit: small programs run as scripts with deterministic behavior and useful IDE diagnostics.

## Milestone 2: ownership and native AOT

- Owned values, moves, lexical borrows, deterministic destruction, and narrow unsafe operations.
- Allocator-aware standard containers.
- C ABI import/export and generated headers.
- Cranelift object generation for macOS AArch64 and Windows x86-64.
- Differential VM/native execution suite.
- Native debug metadata sufficient for breakpoints and stack traces.

Exit: representative command-line programs match VM semantics and performance is measured against equivalent Rust/C implementations.

## Milestone 3: concurrency and embedding

- Structured tasks, cancellation, channels, actor isolation, and data-race capability rules.
- Stable C embedding API and generated bindings.
- Versioned component IDL, resource handles, and capability imports.
- Mixed native/VM calls, callbacks, errors, stack traces, and cancellation.
- Script resource limits and interruption.
- Trusted native and verified script plugin examples.

Exit: a native host can load a user script, expose a typed API, call the script, receive callbacks, and enforce capabilities without IPC.

## Milestone 4: developer toolchain

- Complete LSP features for navigation, completion, rename, semantic tokens, and code actions.
- DAP adapter with mixed native/VM stacks.
- `lang` build/package/test/fmt/lint/doc commands.
- Manifest and lockfile with deterministic resolution, checksums, offline mode, and target conditions.
- Compile-time and runtime profiling, allocation traces, and rebuild explanations.
- Install/update/channel tool comparable in role to `rustup`.

Exit: a new developer can install one tool, create a package, develop in an editor, test, debug, and produce a reproducible binary.

## Milestone 5: ecosystem and compatibility

- Editions, deprecation policy, migration/fix tool, and compatibility test matrix.
- Signed release manifests and package provenance.
- Documentation hosting and package metadata search.
- Public registry threat model before accepting uploads.
- WebAssembly component target for sandboxed portable plugins.
- Evaluate LLVM release backend using published benchmarks and debug-quality criteria.

Exit: the language can evolve without silently breaking source, package, bytecode, or component consumers.

## Milestone 6: self-hosting

- Define and freeze a self-hosting subset.
- Implement frontend and core compiler stages in the language.
- Stage 0 Rust compiler builds stage 1; stage 1 builds stage 2.
- Compare reproducible outputs on both tier-1 platforms.
- Document bootstrap trust and disaster-recovery procedures.

Exit: stage 2 passes the same conformance suite as stage 0 and can build the supported toolchain from a documented bootstrap chain.

## Milestone 7: additional frontends

- Review whether a distinct high-level or domain-specific language has requirements that cannot be served by libraries, profiles, or syntax sugar.
- Keep a frontend-specific HIR and lower through the shared MIR only for semantics the core can represent faithfully.
- Use the component ABI when memory, effect, or runtime models differ.
- Add cross-language debugging, package metadata, and conformance tests before declaring interoperability stable.

Exit: a second frontend adds clear user value without weakening the first language’s safety or making the shared IR an unmaintainable union of unrelated semantics.

