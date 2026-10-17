<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0001-project-charter.md -->

# RFC 0001: Project charter and compatibility policy

[日本語版](../ja/rfcs/0001-project-charter.md)

- Status: accepted
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
- Treating prototype behavior outside accepted RFCs as language semantics.

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
- Dedicated accepted RFCs govern evaluation/overflow (RFC 0002), component failure boundaries (RFC 0003), ownership/destruction (RFC 0004), and structured cancellation/isolation (RFC 0005). Details outside their scope remain unspecified and require follow-up RFCs.
- A backend optimization must not change accepted observable behavior.
- Capability checks and trust boundaries are semantic inputs, not optional deployment documentation.
- Compile-time execution, when introduced, receives an explicit capability and resource model.

RFC 0002 accepts wrapping `i64` addition for the initial source subset. This does not specify overflow for future numeric types or operations.

## Execution modes

The initial execution model has one semantic input and multiple lowerings:

- The bytecode VM consumes verified register bytecode lowered from validated MIR.
- Native execution uses Cranelift to validate lowering, host calling conventions, and in-process execution.
- A production AOT path should emit object files and use system linkers before a bundled linker is justified.
- JIT is a development and embedding experiment until startup time, memory, security, and debugging behavior are measured.
- Embedded host calls and callbacks must eventually use typed registration and generated adapters rather than untyped raw pointers.
- WebAssembly/component execution remains a future sandbox tier and is not required by the first prototype.

The original scalar embedding experiment assigns each host function a backend-neutral MIR identifier and resolves a typed `extern "C" fn(i64) -> i64` from a per-runtime registry. The VM dispatches through that registry, while Cranelift imports the same registered function into the JIT module. The VM also prototypes composable opt-in instruction-fuel and host-call budgets, a value-slot limit, a shared one-way interruption flag, and an absolute monotonic deadline: each instruction/terminator and entered callback costs one unit in its respective counter, and each dynamic limit is checked before the corresponding side effect. After validation and arity checking, the verified register count is checked before register-file allocation. An already-set interruption or expired deadline stops before that allocation; later conditions are observed at instruction, terminator, and pre-callback safe points, with both consumed counters in the structured error and interruption taking precedence. Callback-triggered interruption and deadline expiration are cooperative and are observed only after that callback completes. Successful limited execution reports both counts and the static slot footprint. The legacy scalar convenience APIs remain unbounded, non-interruptible, and without a deadline, and native/JIT execution has no equivalent enforcement. A slot is a VM register entry rather than a byte-based heap charge, and host-callback allocations remain outside the limit. This scalar experiment is test scaffolding, not a stable ABI. RFC 0003 now defines component ownership/failure boundaries and candidate serialization, with executable prototypes; RFC 0006 implements source aggregates, portable bytecode, and a VM with default fuel, host-call, frame, and live-cell limits. Production heap/task accounting across backends, richer component calls, and preemptive callback interruption remain outside these implementations. See the current [capability guide](../milestone-1.md) for the supported boundary.

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

Milestone 0 is complete at its prototype/design boundary: RFCs 0001–0005 were accepted on 2026-10-17; validated MIR, VM/JIT differential execution, tier-1 AOT link/run, component/resource/failure fixtures, and native debug-format evidence support the initial backend decision. None of this implements a source frontend or stabilizes an ABI.

The [roadmap](../roadmap.md) owns the current dependency-ordered implementation tasks. The [benchmark record](../benchmarks.md) owns schema-by-schema measurement history and raw-artifact links. Earlier missing-Windows or proposed-RFC statements describe historical revisions, not current decision gates. Historical JIT/application measurements predate the 2026-10-17 JIT reclamation fix and must be recaptured before making current performance claims.

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
- How should the accepted RFC 0003 profile interoperate with a future WebAssembly component target without claiming WIT compatibility prematurely?
- How should production heap and live-task accounting compose across native code, source-VM frame/live-cell limits, and host callbacks? RFC 0006 already defines bounded guest recursion; cross-mode accounting remains Milestone 3 work.

Decision: **accepted on 2026-10-17** by the repository owner after accepting RFC 0003. This acceptance ratifies the project charter, compatibility policy, evidence requirements, and resulting architecture baseline, including Cranelift as the initial JIT/AOT backend. It does not stabilize unimplemented source, ABI, package, or serialized-format details. Naming and Windows AArch64 timing remain optional follow-ups.

## Sources

- Repository-owner acceptance, recorded 2026-10-17 02:25 JST.
- `docs/research.md`, project research, last updated 2026-10-15 22:29 JST.
- `docs/architecture.md`, initial-backend decision and accepted design baseline, last updated 2026-10-17 02:25 JST.
- `docs/roadmap.md`, implementation milestones and Milestone 0 completion, last updated 2026-10-17 02:25 JST.
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
