<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0001-project-charter.md -->

# RFC 0001: Project charter and compatibility policy

[日本語版](../ja/rfcs/0001-project-charter.md)

- Status: proposed
- Authors: project contributors
- Created: 2026-10-15
- Last updated: 2026-10-17
- Tracking issue:
- Supersedes:
- Superseded by:

## Summary

This RFC establishes the decision framework for the first implementation of `lang`. It defines the initial product boundary, platform tiers, compatibility domains, design invariants, and the evidence required before experimental behavior becomes a language commitment.

The first executable milestone is a deliberately small vertical slice: one validated typed MIR function must produce equivalent results through a verified register-bytecode VM and a Cranelift native backend in the same process.

## Motivation

The project vision spans language semantics, memory safety, native code, scripting, embedding, concurrency, debugging, package management, and eventual self-hosting. Implementing those areas concurrently would make failures difficult to attribute and would turn prototype accidents into compatibility constraints.

The project needs a stable way to distinguish:

- Long-term goals from current guarantees.
- Language semantics from backend implementation details.
- Source, bytecode, native ABI, component ABI, and package compatibility.
- Measured results from design hypotheses.
- Trusted native extension boundaries from capability-restricted plugin boundaries.

A library or tooling convention cannot provide this boundary because it governs language evolution, compiler behavior, runtime interoperability, and release policy together.

## Goals

- Define one coherent initial language rather than multiple source languages.
- Preserve a path to low-level systems programming and productive application programming in the same semantic model.
- Require observable equivalence between VM and native execution for shared language features.
- Make in-process embedding and bidirectional host calls a first-class architectural constraint.
- Keep memory safety as the default while making unsafe operations explicit and auditable.
- Establish compatibility domains and require intentional versioning for each domain.
- Require prototypes, differential tests, and measurements before accepting performance or ergonomics claims.
- Target macOS AArch64 and Windows x86-64 as the initial Tier 1 architecture pairs.
- Preserve a reproducible path to self-hosting without constraining the first compiler to be self-hosted.

## Non-goals

- Selecting final surface syntax.
- Accepting a complete ownership, trait, effect, concurrency, or exception model.
- Stabilizing a source, native, component, or bytecode ABI.
- Supporting multiple language frontends before the first frontend and component boundary are proven.
- Requiring JIT compilation in production script deployments.
- Guaranteeing a garbage-collector-free implementation for every future library abstraction.
- Promising release dates, long-term support periods, or semantic-versioning guarantees during pre-alpha development.
- Treating the current arithmetic prototype as accepted language semantics.

## Guide-level design

The project uses RFC status and compatibility labels to communicate what users may rely on.

RFCs use these states:

- `proposed`: under discussion; no compatibility promise.
- `accepted`: approved by the repository owner; implementation may still be incomplete.
- `implemented`: the accepted design is available behind its intended interface and validation passes.
- `stabilized`: users may rely on the documented compatibility policy.
- `withdrawn`: closed without adoption.
- `superseded`: replaced by a named RFC.

Every compatibility-sensitive RFC identifies the affected domains:

- `source`: parsing, name resolution, typing, and observable program behavior.
- `bytecode`: bytecode encoding, verifier rules, and VM behavior.
- `native-abi`: calling conventions and data layout within one toolchain release.
- `component-abi`: independently versioned interfaces, handles, and generated adapters.
- `package`: manifests, dependency resolution, lockfiles, and registry protocol.
- `tooling`: diagnostic codes, formatter output, LSP/DAP behavior, and machine-readable output.
- `bootstrap`: compiler-stage inputs, outputs, and reproducibility requirements.

During pre-alpha development, all domains are unstable unless an accepted RFC explicitly narrows that statement. Changes still require tests, documentation, and migration notes when they invalidate repository fixtures.

## Reference semantics

This RFC does not define user-facing syntax or complete language semantics. It establishes these invariants for later RFCs:

- A shared typed MIR is the semantic boundary used by bytecode and native lowering.
- MIR must be validated before any backend consumes it.
- Safe source programs must not require unchecked memory access in user code.
- Unsafe compiler/runtime code must be isolated behind a narrow interface with explicit invariants and tests.
- Evaluation order, integer overflow, destruction, panic, unwinding, and cancellation remain unspecified until dedicated RFCs accept them.
- A backend optimization must not change accepted observable behavior.
- Capability checks and trust boundaries are semantic inputs, not optional deployment documentation.
- Compile-time execution, when introduced, receives an explicit capability and resource model.

The prototype's `i64` wrapping addition is chosen only to make VM/native differential tests deterministic. It does not decide the eventual default overflow semantics.

## Execution modes

The initial execution model has one semantic input and multiple lowerings:

- The bytecode VM consumes verified register bytecode lowered from validated MIR.
- Native execution uses Cranelift to validate lowering, host calling conventions, and in-process execution.
- A production AOT path should emit object files and use system linkers before a bundled linker is justified.
- JIT is a development and embedding experiment until startup time, memory, security, and debugging behavior are measured.
- Embedded host calls and callbacks must eventually use typed registration and generated adapters rather than untyped raw pointers.
- WebAssembly/component execution remains a future sandbox tier and is not required by the first prototype.

The current embedding experiment assigns each host function a backend-neutral MIR identifier and resolves a typed `extern "C" fn(i64) -> i64` from a per-runtime registry. The VM dispatches through that registry, while Cranelift imports the same registered function into the JIT module. The VM also prototypes composable opt-in instruction-fuel and host-call budgets, a value-slot limit, a shared one-way interruption flag, and an absolute monotonic deadline: each instruction/terminator and entered callback costs one unit in its respective counter, and each dynamic limit is checked before the corresponding side effect. After validation and arity checking, the verified register count is checked before register-file allocation. An already-set interruption or expired deadline stops before that allocation; later conditions are observed at instruction, terminator, and pre-callback safe points, with both consumed counters in the structured error and interruption taking precedence. Callback-triggered interruption and deadline expiration are cooperative and are observed only after that callback completes. Successful limited execution reports both counts and the static slot footprint. Existing convenience APIs remain unbounded, non-interruptible, and without a deadline, and native/JIT execution has no equivalent enforcement. A slot is a VM register entry rather than a byte-based heap charge, and host-callback allocations remain outside the limit. This is test scaffolding rather than a stable ABI: aggregate values, error transport, panic containment, capabilities, heap/recursion/task limits, preemptive callback interruption, and serialized interfaces remain unspecified and unimplemented.

Any intentional difference between execution modes requires an RFC, a diagnostic where relevant, and tests that demonstrate the boundary.

## ABI and compatibility

The project does not promise a stable native ABI during pre-alpha development. Internal calls may use toolchain-private layouts when producer and consumer are built together.

Independently versioned code must use a component boundary that defines:

- Canonical scalar and aggregate representations.
- Ownership transfer and destruction responsibilities.
- Error, panic, and cancellation behavior.
- String, collection, resource-handle, and callback representation.
- Capability declarations and trust level.
- Version negotiation and adapter generation.

Bytecode and serialized MIR include explicit format versions before they are persisted or distributed. Unknown versions and malformed input must fail closed. Editions may preserve source compatibility, but editions do not silently change component ABI contracts.

## Diagnostics and tooling

Compiler diagnostics use stable identifiers once a diagnostic participates in tests or editor integration. Human-readable wording may evolve independently from machine-readable codes.

The first frontend must preserve source spans and recovery information needed by the formatter and LSP. Formatter idempotence, parser/formatter round trips, VM/native differential execution, and debugger source mapping are release criteria for the milestones that introduce them.

CLI commands intended for automation provide a versioned structured-output mode before stabilization. The English documentation is canonical; Japanese documents mirror the same requirements without becoming a second instruction source for AI tooling.

## Performance and cost model

No zero-cost claim is accepted without a benchmark and generated-code inspection for representative workloads. Measurements must separate:

- Parse, analysis, optimization, code generation, and link time.
- VM startup, JIT compilation, AOT compilation, and steady-state execution.
- Direct calls, VM calls, host calls, callbacks, and component adapters.
- Owned values, inferred borrows, atomic shared references, arenas, and allocator traffic.
- Debug and release builds.
- macOS AArch64 and Windows x86-64 results.

Benchmarks record toolchain versions, commit identifiers, hardware, operating system, command lines, and variance. Prototype measurements guide decisions but do not become language guarantees unless a later RFC defines a user-visible cost model.

## Security and trust boundaries

Source text, package metadata, bytecode, component descriptions, debug data, and plugin inputs are untrusted unless explicitly established otherwise.

- MIR and bytecode are validated before execution or native lowering.
- The VM validates bytecode and can enforce prototype instruction-fuel and host-call budgets before side effects, a register-slot cap before allocation, and cooperative interruption or an absolute deadline at explicit safe points; broader bounds and resource limits remain incremental work.
- Native plugins are trusted code and require an explicit opt-in.
- Untrusted plugins use capability-restricted script or WebAssembly tiers.
- Panic and unwinding do not cross FFI or component boundaries without an accepted representation.
- JIT memory must not be writable and executable simultaneously except where the platform API requires a tightly scoped transition.
- Parser, verifier, package, and component inputs receive fuzzing and denial-of-service tests before exposure to untrusted ecosystems.

## Alternatives

### No charter

The project could implement features directly from the architecture document. This is rejected because an architecture proposal does not define acceptance authority, compatibility domains, or evidence requirements.

### Library or tooling policy only

Repository conventions could label unstable APIs without a language RFC. This is insufficient because VM/native equivalence, unsafe boundaries, ABI evolution, and editions affect semantics across the complete toolchain.

### Stabilize an LLVM-compatible native ABI first

A native-first design could make C interoperability the primary boundary and add scripting later. This simplifies early code generation but risks making platform ABI details the language model and makes safe in-process plugins harder to version.

### Build only a bytecode language first

A VM-only implementation would deliver scripting earlier. It is simpler, but it postpones the defining requirement that one semantic representation support native systems performance and embedded scripts without process boundaries.

## Implementation plan

Prototype status as of 2026-10-16: steps 1–3 are complete for the local `i64` vertical slice. Step 3 covers typed registration, VM/native differential execution, duplicate registration rejection, unknown-function rejection, deterministic VM instruction fuel, a host-call budget, a pre-allocation value-slot cap, cooperative interruption, and an absolute monotonic deadline; it does not complete the stable embedding or component boundary. Typed MIR and bytecode now use explicit basic blocks with branch, jump, and return terminators; validation covers targets, reachability, and SSA dominance, and VM fuel stops a tested back-edge loop. Typed block parameters and edge arguments model phi-like merges and carry an `i64` through a tested countdown back-edge; a bounded sum workload carries both a descending counter and wrapping accumulator through the loop, and a conditional-sum workload branches and merges add/skip paths before the shared back-edge. Validation rejects entry-block parameters, edge argument arity/type mismatches, and non-dominating values; VM and JIT results agree, and the AOT path emits all three loop functions. Source allocation remains open. [RFC 0002](0002-prototype-control-flow.md) provides decision-ready proposed source-level integer and structured-control-flow semantics, while parser, HIR, formatter, source diagnostics, and source-to-MIR lowering remain unimplemented. The Cranelift backend can emit an object artifact with an exported MIR function and imported host callbacks for either the native host or an explicit target triple. A host-independent test validates x86-64 COFF format and the expected add, accumulator, conditional-sum, and callback export/import symbols, including `lang_host_i64_4`, for `x86_64-pc-windows-msvc`; Windows linking and execution remain open. An initial shared `SystemLinker` driver validates ordered requests, selects `cc -o` on non-MSVC hosts or `link.exe /OUT:` on MSVC hosts, permits explicit driver/flavor configuration, and reports structured failures; a macOS AArch64 integration test links the add, minimum-CFG, plain callback, all four host-allocation callbacks, accumulator, and conditional-sum objects through it and executes the resulting C harness. Production driver hardening and a stable native ABI remain open. Step 4 has executable strict models for all three stress cases: async borrowing/cancellation, cyclic graphs, and shared mutable services. Step 5 has a clean macOS AArch64 [schema 14 benchmark baseline](../benchmarks.md) for add, minimum, loop-carried countdown, two-value accumulator, and conditional-sum bytecode lowering; Cranelift JIT and AOT compilation; generated code, object, and linked-executable size; VM/native calls; plain, single-allocation, 64-allocation-burst, and allocation-churn host callbacks; three requested-payload heap profiles; unset-flag interruption-check and unexpired-deadline comparisons; system linking; single-launch plus batched warm-cache AOT process startup; and linked-process peak-memory sampling. Recorded AArch64 generated-code inspection confirms the expected two-instruction add, compare-and-branch minimum, and position-independent host import. Windows x86-64 linking, execution, benchmark, and CI evidence, variable-size timing/profile evidence, additional control-flow and representative allocation/memory workloads, broader embedding limits, native enforcement, and less restrictive memory-model alternatives remain open.

A post-CFG reinspection confirms unchanged minimum code and object sizes and instruction sequence. `SystemLinker` now models `Cc` and `MsvcLink` command flavors, selects the matching host default, supports an explicit driver with ordered prefix arguments, exposes the complete command, and retains it in structured execution failures. Ordered library search paths and logical library names lower to flavor-specific arguments, and validation rejects empty object paths, search paths, and names. Explicit response-file linking uses GNU-escaped UTF-8 for `Cc` and Windows-escaped, BOM-prefixed UTF-16LE for `MsvcLink`; focused serialization tests and macOS direct/response-file execution pass. Toolchain discovery and broader target policy remain open. A cfg-gated Windows x86-64 MSVC direct/response-file object-link-execute test and SHA-pinned Windows Server 2025 CI workflow are configured, but no runner result is recorded for this commit.

Initial host-toolchain discovery scans `PATH` in deterministic candidate order, requires executable Unix files, and reports the searched path and candidates through a structured error. The macOS integration uses the discovered driver, and the cfg-gated MSVC integration uses the same discovery after its developer environment initializes `PATH`. Installation discovery outside an initialized environment remains open.

The initial tier-1 linker strategy is now encoded by a backend-neutral `LinkTarget`: exact `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` triples map to their command flavor, default program, and ordered discovery candidates. Explicit-target linker construction/discovery, both platform integrations, and the cross-target COFF test use that shared mapping; unsupported triples fail structurally. Additional target policies, installation discovery, and startup/runtime inputs remain open.

The schema 3 benchmark adds the typed countdown back-edge to lowering, JIT/AOT compilation, direct/VM/JIT calls, artifact sizes, and both tier-1 system-link paths. The schema bump records the added fields and link-series rename explicitly. A clean macOS AArch64 schema 3 result is recorded. Windows CI validates schema, clean-checkout metadata, target identity, and the expanded link row before retaining the JSON for 30 days; without an inspected Windows result, this is not yet cross-platform evidence.

Schema 4 adds linked AOT executable size and repeated warm-cache process-startup measurements to both tier-1 evidence paths. Process startup remains distinct from system-link time and includes process creation, loader work, the tiny harness, output collection, and termination; it neither flushes operating-system caches nor represents a complete runtime distribution. A clean macOS AArch64 result records a 16,992-byte executable and 1.511 ms median startup, but a 49.824 ms outlier raises startup RSD to 276.388%, so methodology refinement is required before setting a budget. Windows CI validates the new row and size field, but remote Windows evidence remains open.

Schema 5 retains the single-launch startup row and adds a configurable batched row whose samples report elapsed time per sequential launch. Keeping both rows separates tail-sensitive individual latency from a less noise-sensitive amortized estimate. A clean macOS AArch64 result keeps similar 1.487 ms and 1.499 ms medians while reducing RSD from 256.272% to 4.688% for ten-launch batches. The batch size is part of the machine-readable envelope and Windows CI validates both rows, but remote Windows evidence remains open.

Schema 6 adds the two-value accumulator to bytecode lowering, JIT/AOT compilation, direct/VM/JIT calls, generated code and object sizes, system linking, linked-executable size, and both startup rows. Platform series are renamed for the four-object link set, and Windows CI validates the new accumulator and platform fields. A clean macOS AArch64 result records all 30 rows and a 17,056-byte linked harness; remote Windows evidence remains open. This implementation evidence does not change the RFC's proposed status.

A post-schema-6 conditional-sum prototype branches inside the bounded loop and merges add/skip paths through block parameters before the shared back-edge. VM/JIT differential tests, macOS AArch64 direct/response-file link-and-run integration, host-independent Windows x86-64 COFF symbol validation, and the cfg-gated MSVC path cover the new function. Its source-level semantics and allocation remain open; the RFC stays proposed.

The schema 7 harness now covers the conditional-sum workload in lowering, JIT/AOT compilation, direct/VM/JIT calls, artifact sizes, and the five-object tier-1 system-link/startup set. Windows CI validates the corresponding schema contract. A clean macOS AArch64 result records all 36 rows and the 17,104-byte linked harness; remote Windows evidence remains open, and this prototype evidence does not change the RFC's proposed status.

The async semantics prototype now compares the strict default `RejectAll` suspension policy with an explicit `AllowSharedFrameBorrows` alternative. The alternative carries a shared task-frame borrow across suspend/resume, preserves the `SEM0007` rejection for exclusive borrows, and ends active shared borrows before cancellation destroys owned values. This is evidence for one less restrictive design, not accepted ownership or cancellation semantics. Pinning, exclusive borrows across suspension, source-level frame-residency proofs, destructor failure, VM/native trace equivalence, and runtime cost remain open; the review outcome remains **needs prototype/measurement**.

The cyclic-graph prototype now compares the default `Reject` shared-cycle policy with an explicit `CollectUnreachable` alternative. The collector permits strong cycles, traces strong edges from live external roots after each root drop, does not retain through weak edges, and reclaims an unreachable cycle in reverse creation order. This makes the strongest competing graph design executable, but it remains a synchronous model rather than accepted ownership or reclamation semantics. Production collection triggers and latency, incremental/concurrent collection, allocation and peak-memory cost, finalization, weak-reference ordering, and destruction observability remain open; the review outcome remains **needs prototype/measurement**.

The shared-service prototype now compares the default `OwnerIsolated` mutation policy with an explicit `ExclusiveCapability` alternative. A valid handle holder can acquire the one active mutation capability, a task without a handle or a concurrent mutator is rejected with a structured diagnostic, and cancellation waits for a non-owner mutation to end before dropping service state. This makes one lock-like competing design executable, but it is not accepted concurrency semantics. Read/write and atomic capabilities, capability derivation, scheduling fairness, priority inversion, poisoning, deadlock, reentrancy, panic handling, backpressure, VM/native trace equivalence, and runtime cost remain open; the review outcome remains **needs prototype/measurement**.

The host-allocation boundary prototype has a single-allocation host function, a second allocation function that keeps 64 boxes with 512 payload bytes live before summing and dropping them, a post-schema-13 churn function that consumes every other box before allocating 32 replacements into the freed slots, and a post-schema-14 variable-size function that keeps 16 boxed slices with 1,088 requested payload bytes live simultaneously. VM and JIT differential tests register Rust `Box` callbacks; the macOS AArch64 direct/response-file AOT integration executes distinct C `malloc`/`free` imports, host-independent Windows COFF validation checks every allocation import through `lang_host_i64_4`, and the configured Windows MSVC path mirrors the link-and-run coverage. Schema 8 adds direct Rust, VM, and JIT latency rows for the single-allocation callback, and schema 11 adds the same three rows for the burst without changing the AOT process-startup set. The clean schema 14 macOS AArch64 result records single-allocation medians of 11.931 ns, 188.068 ns, and 13.631 ns; burst medians of 775.515 ns, 959.024 ns, and 789.842 ns; churn medians of 1.192 us, 1.348 us, and 1.173 us; the exact churn requested-payload profile `(96 allocations, 768 total bytes, 512 peak live bytes, 0 live bytes afterward)`; and 31 allocation-free linked-process peak-memory samples from 1,654,784 to 1,671,168 bytes. The variable-size callback is not part of schema 14 and does not yet have timing or requested-payload-profile evidence. This demonstrates equivalent callback return values and permits less trivial boundary-cost workloads plus a whole-process envelope, not source-language allocation or ownership semantics. Allocator and collector policy, failure and resource-limit behavior, isolated allocator-overhead accounting, broader representative workloads and remote Windows evidence remain open; the RFC stays proposed and the review outcome remains **needs prototype/measurement**.

The VM resource-control prototype now composes an optional host-call budget, value-slot limit, shared one-way interruption flag, and absolute monotonic deadline with instruction fuel. Each callback that is about to run consumes one host-call unit after its bytecode instruction consumes fuel; exhausting either counter returns a structured error before that counter's guarded side effect. The verified register count is compared with the value-slot limit before the register file is allocated. An already-set interruption or expired deadline stops before allocation, while later conditions are observed at instruction, terminator, and pre-callback safe points; structured errors report both dynamic counts, and interruption takes precedence. A callback-triggered request or deadline overrun is deliberately observed after that callback completes. Successful limited execution reports both dynamic counts and the static slot footprint; tests cover zero host-call budget, an undersized slot limit, pre-execution interruption, callback-triggered interruption, pre-execution deadline expiration, and a deadline stopping a running back-edge. Callback-internal work is one opaque unit, and slots neither measure bytes nor cover the heap. Heap memory, recursion, tasks, callback-internal preemption, and native/JIT enforcement remain open. This narrows another blocking resource-control question without defining accepted sandbox semantics; the review outcome remains **needs prototype/measurement**.

Benchmark schema 9 adds `vm_countdown_interruptible_call` beside the existing VM countdown row. It executes identical bytecode and inputs with an unset shared interruption flag, allowing the acquire-load cost of enabled safe points to be measured directly. The historical clean schema 14 macOS AArch64 run records ordinary and interruption-enabled VM countdown medians of 1.611 us and 1.641 us. The 29.868 ns (1.854%) increase is smaller than the interruption row's 15.866% RSD, so it shows no measurable regression and does not establish an overhead budget. Schema 15 retains the row and is the current CI contract; remote evidence remains open.

Benchmark schema 10 adds `vm_countdown_deadline_call`, which executes identical bytecode and inputs with an absolute monotonic deadline set one hour after harness preparation. This permits direct measurement of steady-state clock-check cost while the deadline remains unexpired. The historical clean schema 14 macOS AArch64 run retains the row and records ordinary/unexpired-deadline VM countdown medians of 1.611 us and 3.320 us, a 1.709 us or 106.071% increase. Windows CI requires the row; remote evidence remains open.

Benchmark schema 11 adds direct Rust, VM, and JIT rows for the 64-allocation burst while retaining all schema 10 fields. A clean historical 31-sample macOS AArch64 run records all 44 unique rows and burst medians of 782.624 ns, 962.329 ns, and 786.270 ns; macOS AOT integration executes the imported callback, and a host-independent test validates its Windows COFF import. Windows CI retains the three rows under schema 15; remote Windows evidence remains open.

Benchmark schema 12 preserves those 44 timing rows and adds backend-independent requested-payload heap profiles for the same callback workloads. Safe accounting records `(allocation count, total bytes, peak live bytes, live bytes after)` as `(1, 8, 8, 0)` for the single allocation and `(64, 512, 512, 0)` for the burst; tests and Windows CI enforce the values. A clean 31-sample macOS AArch64 run records both exact profiles and all 44 unique rows. This measures logical payload lifetime and cleanup, not allocator metadata, fragmentation, VM/JIT internals, or process RSS. Remote Windows evidence, allocator-overhead isolation, and representative allocation workloads remain open, so the review outcome stays **needs prototype/measurement**.

Benchmark schema 14 preserves every schema 13 field and adds three allocation-churn timing rows plus the exact `(96, 768, 512, 0)` requested-payload profile. VM/JIT differential tests, macOS direct/response-file integration, host-independent Windows COFF validation, and the cfg-gated MSVC path cover the callback/import contract; a clean 31-sample macOS AArch64 run records 47 unique timing rows, all three exact heap profiles, churn medians of 1.192 us, 1.348 us, and 1.173 us, and linked-process peak memory from 1,654,784 to 1,671,168 bytes. Allocator-overhead isolation, representative allocation workloads, and remote Windows evidence remain open, so the review outcome stays **needs prototype/measurement**.

After schema 14, a fourth callback exercises variable-size allocation by keeping 16 boxed `i64` slices with payloads from 8 to 128 bytes, or 1,088 requested payload bytes total, live simultaneously before returning a deterministic wrapping fold. VM/JIT differential tests cover integer extremes, macOS direct/response-file integration executes the corresponding C import, host-independent Windows COFF validation checks `lang_host_i64_4`, and the cfg-gated MSVC path mirrors link-and-run coverage. Timing and exact requested-payload profiling for this callback remain open alongside representative workloads, allocator metadata/fragmentation, and remote Windows execution, so the review outcome stays **needs prototype/measurement**.

Benchmark schema 13 preserves all schema 12 fields and adds one distribution for the linked AOT process's OS-reported peak resident memory. macOS uses byte-valued `ru_maxrss`; on Windows, each measured process self-reports byte-valued `PROCESS_MEMORY_COUNTERS.PeakWorkingSetSize` through `K32GetProcessMemoryInfo` before exit. Both include shared pages and loader/runtime state, so this is a process envelope rather than allocator-overhead or allocation-workload evidence. Tests, a native macOS smoke run, and the schema 20 Windows CI contract validate the mechanism. The historical clean schema 13 macOS AArch64 result records all 31 samples as 1,671,168 bytes with 0.000% RSD; the Windows baseline remains open. The review outcome therefore stays **needs prototype/measurement**.

A clean pre-implementation macOS AArch64 [AOT debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) establishes the original negative baseline. The committed [source-mapped DWARF inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md) regenerates the representative add, minimum, and host-callback objects from commit `31e182ac27e324048597a5ebd0fda4d2d4934395`. All three pass `dwarfdump --verify` with relocated DWARF v4 compile-unit, subprogram, and source-line data; direct and response-file executables link and run; and both generated dSYMs pass verification while retaining the fixture path and all three functions. The committed [Windows x86-64 CodeView/PDB inspection](../../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md) validates equivalent C13 COFF records and relocations from commit `465b0d66ba6ea61a0d4b26201e6ab043f2f15f9d`, links them into PE/PDB artifacts with LLD, and verifies linked function-to-source resolution with LLDB. Native Windows run 38932964398 adds direct/response-file execution, component conformance, and a clean schema-20 performance/memory baseline. The planned backend-selection evidence is complete. Source-language locals, unwind metadata, JIT debugger registration, optimized stepping, and representative source-program validation remain later implementation criteria rather than blockers for this charter's initial backend choice.

1. Add the minimal typed MIR, bytecode lowering, verifier, VM, and Cranelift JIT prototype.
2. Differentially test a small typed function through VM and native execution on the development host.
3. Add typed host-function registration and a script-to-host callback; then call a compiled function from the embedded runtime.
4. Prototype cyclic graphs, async borrowing/cancellation, and shared mutable services under the proposed ownership model.
5. Record backend, call-overhead, code-size, and compile-time measurements on macOS AArch64 and Windows x86-64.
6. Draft focused RFCs for arithmetic, ownership, effects/errors, concurrency, embedding, and component ABI based on prototype evidence.
7. Accept or revise this charter only after its blocking questions are resolved.

## Initial native backend decision

**Project decision (2026-10-17): select Cranelift as the initial JIT and AOT backend.** The completed macOS AArch64 and Windows x86-64 evidence exercises the shared MIR across VM/JIT equivalence, native object emission, direct and response-file system linking, process execution, host callbacks, typed component/resource cleanup, source-mapped DWARF/dSYM and CodeView/PDB artifacts, and schema-20 performance/memory measurements. No result reveals a blocker that justifies adding a second backend before the first frontend and ABI are stable.

This decision minimizes current implementation and maintenance risk because the existing backend already serves both execution modes and remains isolated behind backend-neutral MIR, artifact, source-map, and linker interfaces. It does not promise a stable Cranelift-specific ABI, guarantee current benchmark values, or reject LLVM permanently. Re-evaluate LLVM or another backend when representative release workloads identify an unmet optimization, target-support, debug-quality, or integration requirement that cannot be addressed reasonably in the Cranelift path.

## Test plan

- Unit-test MIR validation, bytecode verification, VM execution, and backend error handling.
- Run identical input tables through VM and native code and compare results.
- Add malformed MIR and bytecode tests before deserialization is introduced.
- Add property and fuzz tests as instruction and type counts grow.
- Test the implemented `i64` host call and callback for VM/native equivalence, duplicate registration rejection, and unknown-function rejection. Add panic containment and capability rejection when those mechanisms are introduced.
- Test semantics experiments with stable diagnostic codes, primary/secondary source spans, deterministic event traces, and exact destruction counts. Compare strict and less restrictive policy traces without presenting either as accepted behavior.
- Run formatting, Clippy with warnings denied, all workspace tests, and documentation builds in CI.
- Add macOS AArch64 and Windows x86-64 CI before declaring the vertical slice portable.
- Store benchmark inputs and raw metadata with summarized results.

## Open questions

- What permanent project and language names should replace `lang`?
- Which exact compatibility guarantees begin at the first public pre-1.0 release?
- Should Windows AArch64 join Tier 1 before or after the first native backend milestone?
- Which component model provides the starting point for canonical ABI experiments?
- How should byte-based heap, recursion, and task budgets compose with the prototype instruction-fuel, host-call-budget, value-slot-limit, cooperative-interruption, and absolute-deadline interface?

Review outcome: **awaiting RFC 0003 owner disposition**. The component, resource-control, concurrency, debug-quality, cross-platform execution, benchmark, and backend-selection evidence is complete. RFC 0003 now awaits owner acceptance or revision of its component serialization and compatibility policy; after that disposition, no prototype or measurement blocker remains for deciding this charter. Naming and Windows AArch64 timing are optional follow-ups.

## Sources

- `docs/research.md`, project research, last updated 2026-10-15 22:29 JST.
- `docs/architecture.md`, initial-backend decision and design baseline, last updated 2026-10-17 01:57 JST.
- `docs/roadmap.md`, implementation milestones, last updated 2026-10-17 01:47 JST.
- `crates/vm/src/lib.rs`, deadline-enabled VM resource-control prototype, last updated 2026-10-16 11:20 JST.
- `src/lib.rs`, allocation burst/churn/variable-size boundary prototypes, last updated 2026-10-16 14:12 JST.
- `src/bin/prototype-bench.rs`, schema-20 cross-platform benchmark harness, last updated 2026-10-17.
- `benchmarks/results/2026-10-16-macos-aarch64-6a2caf8.json`, clean schema 14 macOS AArch64 evidence, measured 2026-10-16 14:02:20 JST.
- `benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md`, macOS AArch64 AOT debug-information inspection, captured 2026-10-16 12:06:01 JST.
- `benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md`, macOS AArch64 source-mapped DWARF inspection, captured 2026-10-16 19:41:57 JST.
- `benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md`, Windows x86-64 source-mapped CodeView/PDB inspection, captured 2026-10-16 20:19:36 JST.
- `benchmarks/results/2026-10-16-macos-aarch64-de248a1.json`, clean schema-20 macOS AArch64 evidence, measured 2026-10-16 23:34:00 JST.
- `benchmarks/results/2026-10-17-windows-x86_64-7c37d5d.json`, clean schema-20 Windows x86-64 evidence from [GitHub Actions run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398), measured 2026-10-17 01:45:50 JST; the run completed 2026-10-17.
- Cranelift 0.136.3 documentation, official package documentation and living documentation without a published update date, verified 2026-10-17: https://docs.rs/cranelift-codegen/0.136.3/
- Cranelift JIT documentation, living documentation without a published update date, verified 2026-10-15: https://docs.rs/cranelift-jit/0.136.3/
