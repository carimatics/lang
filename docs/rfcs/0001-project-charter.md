<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0001-project-charter.md -->

# RFC 0001: Project charter and compatibility policy

[日本語版](../ja/rfcs/0001-project-charter.md)

- Status: proposed
- Authors: project contributors
- Created: 2026-10-15
- Last updated: 2026-10-15
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

The current embedding experiment assigns each host function a backend-neutral MIR identifier and resolves a typed `extern "C" fn(i64) -> i64` from a per-runtime registry. The VM dispatches through that registry, while Cranelift imports the same registered function into the JIT module. This is test scaffolding rather than a stable ABI: aggregate values, error transport, panic containment, capabilities, resource limits, and serialized interfaces remain unspecified and unimplemented.

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
- The VM enforces bounds and resource limits as those facilities are introduced.
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

Prototype status as of 2026-10-15: steps 1–3 are complete for the local `i64` vertical slice. Step 3 covers typed registration, VM/native differential execution, duplicate registration rejection, and unknown-function rejection; it does not complete the stable embedding or component boundary. Step 4 now has executable strict models for all three stress cases: async borrowing/cancellation, cyclic graphs, and shared mutable services. Less restrictive alternatives remain open.

1. Add the minimal typed MIR, bytecode lowering, verifier, VM, and Cranelift JIT prototype.
2. Differentially test a small typed function through VM and native execution on the development host.
3. Add typed host-function registration and a script-to-host callback; then call a compiled function from the embedded runtime.
4. Prototype cyclic graphs, async borrowing/cancellation, and shared mutable services under the proposed ownership model.
5. Record backend, call-overhead, code-size, and compile-time measurements on macOS AArch64 and Windows x86-64.
6. Draft focused RFCs for arithmetic, ownership, effects/errors, concurrency, embedding, and component ABI based on prototype evidence.
7. Accept or revise this charter only after its blocking questions are resolved.

## Test plan

- Unit-test MIR validation, bytecode verification, VM execution, and backend error handling.
- Run identical input tables through VM and native code and compare results.
- Add malformed MIR and bytecode tests before deserialization is introduced.
- Add property and fuzz tests as instruction and type counts grow.
- Test the implemented `i64` host call and callback for VM/native equivalence, duplicate registration rejection, and unknown-function rejection. Add panic containment and capability rejection when those mechanisms are introduced.
- Test semantics experiments with stable diagnostic codes, primary/secondary source spans, deterministic event traces, and exact destruction counts.
- Run formatting, Clippy with warnings denied, all workspace tests, and documentation builds in CI.
- Add macOS AArch64 and Windows x86-64 CI before declaring the vertical slice portable.
- Store benchmark inputs and raw metadata with summarized results.

## Open questions

- What permanent project and language names should replace `lang`?
- Which exact compatibility guarantees begin at the first public pre-1.0 release?
- Should Windows AArch64 join Tier 1 before or after the first native backend milestone?
- Which component model provides the starting point for canonical ABI experiments?
- What resource-budget interface should embedded hosts use for untrusted scripts?
- Which measurements are blocking for selecting Cranelift as the initial backend rather than only a prototype backend?

Review outcome: **needs prototype/measurement**. The blocking questions are the component boundary, embedded resource controls, and backend measurements. Naming and Windows AArch64 timing are optional follow-ups for this RFC.

## Sources

- `docs/research.md`, project research, last updated 2026-10-15 22:29 JST.
- `docs/architecture.md`, proposed design baseline, last updated 2026-10-15 23:46 JST.
- `docs/roadmap.md`, implementation milestones, last updated 2026-10-15 23:46 JST.
- Cranelift documentation, living documentation without a published update date, verified 2026-10-15: https://docs.rs/cranelift-codegen/0.136.3/
- Cranelift JIT documentation, living documentation without a published update date, verified 2026-10-15: https://docs.rs/cranelift-jit/0.136.3/
