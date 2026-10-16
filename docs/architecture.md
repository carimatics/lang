<!-- canonical-language: en; japanese-translation: ja/architecture.md -->

# Architecture baseline

[日本語版](ja/architecture.md)

Status: **accepted design baseline** under RFCs 0001–0005 (2026-10-17), with incomplete prototype implementations. Acceptance is not stabilization. The RFCs govern semantics; broader product directions below require follow-up RFCs before implementation.

## Goals

- Native performance and low-level control on Windows and macOS.
- Memory and data-race safety by default without a mandatory tracing GC.
- A productive, expression-oriented language suitable for application code.
- AOT executables/libraries and embeddable scripts using one semantic frontend.
- Efficient native-to-script and script-to-native calls in one process.
- A future path for several source languages without binding compatibility to one frontend ABI.
- Excellent diagnostics, build speed, debugging, IDE support, and reproducibility.

## Non-goals for the first implementation

- Supporting every CPU architecture or operating system.
- Stabilizing a native language ABI.
- Hot-reloading arbitrary native code safely.
- Transparent distributed execution.
- A tracing GC in the core language.
- Multiple source languages before the first frontend and component boundary are stable.
- A package registry before local and Git-based dependency workflows are reproducible.

## Product model

Start with one language that has two explicit programming layers:

- **Safe application layer:** value-oriented APIs, null safety, algebraic data types, pattern matching, type inference, traits, closures, structured concurrency, and managed shared references where requested.
- **Systems layer:** allocators, raw pointers, layout control, C ABI, atomics, SIMD, inline assembly/intrinsics, and platform APIs behind `unsafe` or capability-gated interfaces.

This keeps interoperability inside one type system. A second language should be added only when it can lower to the same component model and conformance suite without weakening safety.

## Compiler pipeline

```text
source text
    │
    ▼
lossless syntax tree ───────────── formatter / IDE recovery
    │
    ▼
resolved HIR ───────────────────── names / modules / desugaring
    │
    ▼
typed HIR ──────────────────────── inference / traits / effects
    │
    ▼
MIR ────────────────────────────── ownership / destruction / async lowering
    │
    ├──────────────► bytecode ───► verifier ─► embedded VM
    │
    ├──────────────► native SSA ─► Cranelift/LLVM ─► object/linker
    │
    └──────────────► interface IR / metadata ───────► bindings and plugins
```

The typed MIR is the semantic contract. Backend-specific optimizations may differ, but observable behavior, destruction order, panic/error behavior, integer semantics, and concurrency rules must not.

Accepted [RFC 0002](rfcs/0002-prototype-control-flow.md) defines the initial source subset: `i64` parameters/results, wrapping addition, signed comparison, mutable locals, `if`/`else`, `while`, explicit `return`, and left-to-right evaluation. Values live across joins/back-edges lower through typed block arguments. The diagnostic data model is implemented; parser/type-checker diagnostic producers, the parser, HIR, formatter, and source-to-MIR lowering are not. Lexical details and extensions beyond this subset must pass the Milestone 1 design gates.

### Incremental architecture

Use a dependency-tracked query system from the first non-trivial frontend. Queries should be deterministic and separately measurable:

- Parse file
- Expand bounded compile-time constructs
- Resolve module/import graph
- Resolve names
- Infer/check types and effects
- Check ownership and exclusivity
- Lower and optimize MIR
- Generate bytecode or native code per unit

Persist hashes and timing explanations so the build tool can answer why a unit rebuilt. Keep source files and package manifests as explicit query inputs.

## Memory model baseline

The design must acknowledge the fundamental tradeoff: memory safety, general shared object graphs, simple syntax, deterministic destruction, zero overhead, and no GC cannot all be maximized simultaneously.

The accepted ownership core is specified by RFC 0004. Parameter-passing inference, closure capture syntax, unsafe operations, allocator APIs, and freestanding profiles below are follow-up directions, not additional accepted rules:

- Plain `T` is an owned value with deterministic destruction.
- Moves are inferred; use-after-move is rejected.
- Function parameters default to non-owning reads for non-trivial values where this is unambiguous.
- Mutable access is exclusive and lexical. Ordinary code should not write lifetime parameters.
- Non-escaping borrows are inferred and cannot be stored without an explicit owning abstraction.
- `shared T` is an explicit atomic reference-counted reference; `weak T` breaks cycles.
- Regions/arenas provide bulk lifetime management for graphs, compilers, games, and request-scoped data.
- Closures state whether they borrow, move, or share captures.
- Raw pointers, unions, unchecked indexing, layout casts, and foreign ownership live behind narrow `unsafe` boundaries.
- The standard library exposes allocator-aware containers and permits no-runtime/freestanding subsets later.

Accepted [RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md) requires singly owned plain values, explicit `shared`/`weak`, rejection of cycle-closing strong shared edges before mutation, and lexical arenas for intentional cycles. This is a source/runtime implementation obligation, not a claim that the experiment is a production ownership checker.

Remaining implementation evidence includes nested cleanup, partial initialization, self-reference rejection, and the runtime/memory cost of atomic sharing, cycle preflight, and arenas. RFC 0005 already rejects every borrow across suspension; relaxing that baseline requires a new RFC.

### Cyclic graph experiment

The graph model in `crates/semantics` compares explicit shared/weak ownership and arena-scoped storage under two shared-cycle policies:

- Shared nodes begin with an external root. Strong edges retain their targets, while weak edges do not.
- The default `Reject` policy rejects a strong edge that would close a shared-reference cycle with structured diagnostic `SEM0018`; a weak edge may close the same cycle, and upgrading an expired weak edge reports `SEM0021`.
- The experimental `CollectUnreachable` policy permits strong cycles. After an external root is dropped, it traces strong edges from every remaining live root and releases unreachable shared nodes in reverse creation order; weak edges do not keep a target live.
- Nodes in one arena may form arbitrary cycles because the arena owns their lifetime. Edges across arenas or between arena and shared storage report `SEM0017`.
- Dropping an arena releases its nodes exactly once in reverse creation order. Later use of a stale node handle reports `SEM0012`.
- Every operation carries a source span, and diagnostics retain a related allocation or edge span where one exists.

RFC 0004 accepts `Reject`, weak back-edges, and lexical arenas. `CollectUnreachable` remains an unaccepted alternative. The synchronous graph experiment is not a runtime reference-count implementation: weak upgrade checks liveness but does not return a new retained owner. It does not model atomic publication, thread safety, destructor failure, or production weak-reference ordering. Those obligations require focused implementation and conformance tests.

Review outcome: **needs prototype/measurement for production implementation**, not another decision on the accepted strict policy. Milestone 2 owns source checking, shared/weak lifecycle, cycle-preflight atomicity, and arena lifetime validation.

### Async borrow and cancellation experiment

The executable model in `crates/semantics` compares the RFC 0005 strict baseline with one unaccepted, less restrictive policy:

- Task-owned values remain live in the task frame across suspension.
- The default `RejectAll` policy rejects an active shared or exclusive borrow at a suspension point with structured diagnostic `SEM0007`; a borrow that ends before suspension is accepted.
- The experimental `AllowSharedFrameBorrows` policy permits active shared borrows of task-owned values across suspension but still rejects exclusive borrows with `SEM0007`.
- Cancellation is observed only while the task is suspended.
- Under the experimental policy, resumption preserves the shared borrow, while cancellation ends active borrows before dropping task-owned values exactly once in reverse creation order.
- Every operation carries a source span so failures identify both the boundary and the borrow that crossed it.

RFC 0005 accepts `RejectAll`; `AllowSharedFrameBorrows` remains experimental. The model observes cancellation only at suspension and is not the specified task-tree runtime or an idempotent cancellation-request API. Nested scopes, partial initialization, destructor failure, panic containment, and equivalent VM/native cleanup traces remain implementation work.

Review outcome: **needs prototype/measurement for runtime implementation**. Pinned/shared/exclusive borrows across suspension are excluded from the accepted baseline, not blocking decisions that permit an implementation to choose its own behavior.

## Error and effect model

- Recoverable errors use typed result values with `throws E`/propagation syntax as surface sugar.
- Panics represent violated invariants and do not cross component or FFI boundaries.
- Native unwinding is not part of the stable ABI.
- Effects should initially track only behavior that changes safety or composition, such as `async`, `unsafe`, host capabilities, and possibly allocation. Avoid a general effect system until a prototype proves its ergonomic value.
- Cancellation is structured and propagates through task scopes; destructors and cleanup blocks must run predictably.

## Concurrency model

Combine several layers rather than forcing one abstraction:

- Structured tasks, cancellation, deadlines, and async I/O for application code.
- Actor isolation and message passing for mutable service state.
- Parallel iterators and scoped fork/join for data parallelism.
- Explicit atomics, locks, channels, and unsafe synchronization for systems code.
- Send/share capabilities derived from ownership and mutability so data races are rejected statically.

The scheduler is a replaceable runtime service. Libraries must not assume a global executor, and embedded hosts must be able to drive or integrate the scheduler.

### Shared mutable service experiment

The service model in `crates/semantics` compares the RFC 0005 owner-isolated baseline with one unaccepted shared-mutation policy:

- Under the default `OwnerIsolated` policy, one owner task has direct mutable access to service state. Shared handles may enqueue messages but direct mutation from another task reports `SEM0031`.
- Under the experimental `ExclusiveCapability` policy, any task with a valid handle may begin a mutation when none is active. A task without a handle is rejected, and a concurrent begin reports the existing structured mutation-conflict diagnostic with the active mutation span.
- Sending moves a payload into the mailbox. A non-sendable payload reports `SEM0028`, and reuse after a successful move reports `SEM0027`.
- The owner still processes messages in FIFO order. A cancellation request from any valid handle is deferred until an active mutation ends, including a non-owner mutation under the experimental policy.
- Cancellation discards queued messages in FIFO order, drops every queued payload and the service state exactly once, then rejects later sends with `SEM0024`.
- Operations retain primary and related source spans under stable diagnostic codes `SEM0022` through `SEM0035`.

RFC 0005 accepts `OwnerIsolated`; `ExclusiveCapability` remains an unaccepted lock-like alternative. The model lacks send/share derivation, task trees, repeated idempotent cancellation requests, backpressure, reentrancy, panic cleanup, and runtime scheduling. Model diagnostics are experiment evidence, not a complete source or scheduler contract.

Review outcome: **needs prototype/measurement for runtime implementation**. Milestone 3 must implement accepted isolation/cancellation and decide unspecified channel, scheduler, and error details before exposing them as source behavior.

## AOT and script execution

### Bytecode first

Use a typed register-based bytecode VM rather than an AST interpreter:

- The VM validates bytecode before execution.
- Instructions preserve language-level types and source locations where useful.
- The same runtime implements strings, collections, shared references, task primitives, and panic/error boundaries for native and script code.
- Inline caches and specialization can be added after profiling.
- JIT compilation is optional and follows stable bytecode semantics.

### Embedding API

Provide a stable C ABI embedding layer regardless of the implementation language:

- Create/destroy runtime and isolated script contexts.
- Load verified bytecode/components from memory.
- Register native functions, types, constants, and capabilities.
- Call exports by a validated function handle.
- Convert values through generated bindings or opaque handles.
- Set memory, instruction/fuel, recursion, task, and wall-clock limits.
- Interrupt execution and receive structured diagnostics and stack traces.

Cache call-site validation and generated thunks. Calls remain in-process and avoid IPC, but boundary checks and representation conversions are still real costs and must be benchmarked.

[RFC 0003](rfcs/0003-versioned-component-contracts.md) specifies the first executable component-contract slice. The runtime now validates schema-versioned package metadata with named scalar imports/exports, conservative identifiers, explicit capability requirements, and embedding-supplied logical-name-to-host-function bindings. Portable metadata does not contain process-local `HostFunctionId` values. Validation rejects structural conflicts and unsupported import types, rejects bindings for undeclared imports, checks capability grants before host lookup, resolves every import against the typed registry, and returns a validated wrapper carrying the resolved local bindings. A deterministic experimental `LCMP` schema-1 codec encodes the scalar declaration subset and decodes untrusted bytes under explicit total-byte, declaration-count, structural-depth, and string-length limits; malformed-input errors carry byte offsets, and a libFuzzer target exercises the decoder. This schema-1 codec remains an unstable fixture rather than the accepted candidate schema-2 serialization, a WIT implementation, a component-IDL resource declaration, canonical ABI, or an execution path for component exports. The subsequent prototypes below cover the accepted candidate serialization and selected resource/adapter path, while production IDL and generated adapters remain downstream work.

A separate candidate `LCMP` schema-2 envelope now defines a contract document (`kind 0`) and a versioned failure-report document (`kind 1`). Contracts deterministically encode a package key and full semantic version, sorted normalized interface requirements and required-member projections, SHA-256-tagged exact locks, and sorted canonical resource-type identities. Required members preserve parameter/result order, exact capability, and `i64`, `bool`, `own<T>`, or `borrow<T>` value types with complete nominal resource keys. Failure reports preserve one primary record and ordered suppressed cleanup failures, encode poisoned state, use fixed category/phase/diagnostic tags, and distinguish message redaction from embedding-sanitized transport. A policy-explicit conversion projects runtime `BoundaryFailure` records while preserving cleanup invariant traps and destructor-failure order. Construction rejects invalid identifiers or capabilities, duplicate entries or member names, invalid intervals, undeclared resource references, borrowed results, inconsistent failure classifications, non-cleanup suppressed records, unsafe redaction states, unpoisoned destructor failures, orphan locks, and lock-version mismatches. Bounded decoders reject wrong schema/document kind, malformed tags or booleans, truncation, invalid UTF-8, trailing bytes, and exceeded byte/item/depth/string limits with byte offsets. Checked-in golden encodings, round-trip/canonical-order tests, semantic validation tests, deterministic arbitrary-byte coverage, and dedicated fuzz targets protect both layouts. Accepted RFC 0003 adopts schema 2 as the implementation baseline, while it remains versioned design evidence rather than a stable public production format.

The component runtime also has an in-memory generation-tagged resource table. Copyable owned handles model integer handles that untrusted callers can duplicate; releasing a resource advances its generation before the lowest free slot can be reused, or retires the slot if the generation is exhausted, so stale copies cannot alias a replacement. Shared borrows may coexist, exclusive borrows require no active borrow, mutable access requires a live exclusive token, and release rejects every live borrow. Borrow tokens are monotonic and explicitly ended; stale tokens, repeated ends, repeated releases, conflicting borrows, and identifier exhaustion are structured errors. This proves table-local lifecycle checks, not serialized handle layout, cross-component ownership transfer, destructor behavior, thread safety, or canonical resource lowering.

The runtime now also prototypes package/interface compatibility negotiation independently from schema-1 serialization and production package-manager resolution. Normalized semantic versions preserve prerelease and build identifiers without machine-word-bounding numeric prerelease identifiers. Exact requirements compare the complete value; intervals use SemVer precedence and an explicit prerelease policy. Capability grants are checked before provider inventory validation. Exact lock inputs require one identity, version, and opaque digest without fallback; otherwise candidates are filtered by exact keys and structural required-member projection before deterministic highest-precedence selection. The projection includes ordered scalar and nominal owned/borrowed resource types plus exact capabilities, while allowing unrequested provider members. Same-precedence build/digest ties are ambiguous, and successful resolution returns an immutable binding. A canonical in-memory lockfile records complete exact SHA-256 selections, replays without fallback, reconciles embedded contract locks, and moves the selected contract and bindings directly into fresh instance state. No on-disk lockfile syntax, registry access, artifact download, or publication workflow is defined yet.

An executable schema-1 scalar fixture now connects validated component metadata to all three prototype execution paths. Capability and logical-name bindings are validated against the typed host registry before a selected `i64 -> i64` import/export pair is lowered to MIR. VM and JIT execute the same function with equivalent wrapping results, including integer extremes. The same validated fixture emits a host-native object whose export and resolved host import link and execute on macOS AArch64 through direct and response-file paths. This is narrow differential evidence for one scalar contract, not a general component loader, canonical resource adapter, failure boundary, stable serialization, or cross-platform component runtime.

A backend-neutral canonical-resource adapter trace now makes the RFC 0003 ownership and failure ordering executable without exposing runtime handle layout. Per-instance boundary indices are non-zero, monotonically allocated, and never reused. Whole-call preflight validates every resource argument and destination limit and reserves storage before an allocation-free, callback-free commit moves owners or publishes call-scoped borrows. Cleanup closes nested borrows inside-out, removes remaining frame owners before invoking destructors in reverse acquisition order, continues after independent destructor failures, preserves an earlier panic/trap as primary, and poisons the callee after callback, invariant, or destructor failure. A shared host-value bridge allocates opaque identities, maps them to generation-checked `ResourceTable<T>` owners, rolls back values when canonical publication fails without reusing the identity, resolves runtime borrow operations, and lets cleanup consume the actual value exactly once. General instantiation negotiates every canonically ordered schema-2 requirement before publishing an instance containing immutable bindings and fresh bounded adapter state. A conformance fixture feeds the VM, JIT, and macOS AOT scalar result through the same owned transfer, guest-trap cleanup, and redacted failure projection; value, state, and report outcomes agree. The later straight-line nominal `own<T>` lane now runs through MIR, bytecode, VM, JIT, and tier-1 AOT; general resource control flow and return ownership adapters remain unimplemented. Asynchronous cancellation also remains open.

The VM prototype implements three composable opt-in limits—instruction fuel, a host-call budget, and a value-slot limit—plus an optional shared one-way interruption flag and absolute monotonic deadline. Each bytecode instruction and terminator consumes one fuel unit, and each entered host callback consumes one host-call unit; validation and arity checks occur before metering, and each dynamic limit is checked before the corresponding side effect. The VM compares the verified program's register count with the value-slot limit before allocating the register file. An already-set interruption or expired deadline stops execution before that allocation; subsequent safe points are the start of each instruction and terminator and the boundary immediately before an entered callback. The structured termination errors report instruction and host-call consumption at the observed safe point, and interruption takes precedence if both conditions hold. A callback can set the flag or overrun the deadline, but its internal work completes before the next VM safe point observes the condition. The limited API returns the value, both consumed counts, and the static slot footprint, while existing convenience APIs remain unbounded, non-interruptible, and without a deadline. A non-terminating back-edge test, a finite loop-carried countdown test, a zero-budget callback test, an undersized slot-limit test, pre-execution/callback-triggered interruption tests, and pre-execution/running-loop deadline tests demonstrate loop termination, callback suppression, pre-allocation rejection, and safe-point accounting. A slot is a backend-defined VM register entry, not a byte count, and does not account for host-callback allocations. These controls do not yet cover the heap, recursion, tasks, preemption inside callbacks, or native/JIT execution, so they are evidence for the embedding interface rather than a complete sandbox.

The initial two typed `i64` callbacks exercise host-owned allocation. The first performs one `Box` allocate/read/drop round trip. The second creates 64 boxes containing distinct wrapping values, keeps all 512 payload bytes live, sums them, and then drops them. VM and JIT dispatch to the Rust implementations, while linked AOT objects import corresponding C `malloc`/read/`free` implementations; macOS direct and response-file integration execute both wrappers. Benchmark schema 8 measures direct Rust, VM, and JIT latency for the single-allocation callback, and schema 11 adds the same three rows for the burst. Process-startup rows remain unchanged because callback allocation work would be dominated by process creation and loading. This proves that all three execution paths can cross the typed boundary for bounded allocation-backed operations and provides a less trivial VM/JIT boundary-cost workload. It is not a language heap, allocator ABI, ownership rule, garbage collector, memory limiter, allocator-overhead account, or representative application workload, so those design and cost questions remain unresolved.

Historical schema-by-schema timing, allocation, and process-memory results are maintained in [benchmark evidence](benchmarks.md), not as current architecture requirements. Native Windows evidence is recorded by schema 20 and run 38932964398. The records predate the 2026-10-17 JIT reclamation fix; fresh clean measurements are required before current compile/application performance claims.

### Plugin trust tiers

1. **Trusted native:** AOT object/static library, specialized calls, maximum performance, no sandbox claim.
2. **Verified script:** bytecode VM with capabilities and resource limits.
3. **Portable sandboxed component:** WebAssembly Component Model/WIT-compatible boundary where portability and isolation matter more than absolute call overhead.

User-authored plugins default to tiers 2 or 3. Native plugins are an explicit trust decision.

## Component ABI and multiple languages

Do not use the native source-language ABI as a compatibility promise. Define a separate interface language inspired by WIT:

- Records, variants, enums, flags, resources/handles, lists, strings, options, results, futures, streams, and functions.
- Explicit ownership transfer and borrowed/owned parameters.
- Package/interface semantic versions and compatibility checks.
- Capability imports and exported worlds.
- Generated adapters for native code, VM bytecode, C, and WebAssembly.

Inside a whole-program build, the compiler may specialize adapters into direct calls after proving compatible layouts and ownership. Independently versioned components retain the canonical boundary.

Future frontends should lower first to their own HIR, then to the shared typed MIR only when their semantics match. Frontend-specific features that cannot be represented safely should lower through the component interface instead of expanding the core MIR indefinitely.

## Native backends

### Initial targets

- Tier 1 prototype: macOS AArch64 and Windows x86-64.
- Next targets: macOS x86-64 and Windows AArch64.
- Object formats: Mach-O and COFF/PE.
- C ABI interoperability is required before a custom ABI.

### Backend strategy

- Use Cranelift as the initial native JIT and AOT backend for the first implementation.
- Keep backend APIs narrow and target-neutral.
- Measure release runtime, compile latency, code size, debug quality, and platform integration on representative programs.
- Add an LLVM backend if Cranelift cannot meet release performance, optimization, or debug requirements.
- Use system linkers initially; evaluate bundled LLD only when reproducibility and installation data justify it.

**Decision (2026-10-17):** Cranelift 0.136.3, as resolved by the workspace lockfile, is the initial native backend for both JIT and AOT. This is an implementation baseline, not a stable native ABI, an exclusive permanent backend, or a language-level performance guarantee. The frontend, typed MIR, source maps, linker requests, and artifact interfaces remain backend-neutral; add an LLVM backend later only if representative release workloads expose unmet optimization, target, or debug requirements.

| Criterion | Recorded evidence | Disposition |
| --- | --- | --- |
| Semantic and ABI integration | Differential VM/JIT coverage plus macOS and Windows direct/response-file AOT execution for integer, CFG, callback, scalar-component, and one typed-owner lane | Sufficient for the initial backend |
| Tier-1 portability | Native macOS AArch64 and Windows x86-64 object-link-run CI, with target-neutral linker policy and explicit triples | Sufficient for both prototype Tier-1 targets |
| Debug quality | Source-mapped Mach-O DWARF/dSYM and COFF CodeView/PDB inspection for representative artifacts | Sufficient for the first implementation; locals, unwind metadata, optimized stepping, and JIT debugger registration remain later criteria |
| Performance and memory evidence | Historical clean schema-20 macOS and Windows baselines | Directional evidence only; JIT reclamation was fixed after these captures, so fresh clean measurements are required for current compile/application performance claims |
| Architecture cost | The existing `codegen-clif` crate exposes no Cranelift types through frontend APIs and already supplies JIT and object emission | Lower implementation and maintenance risk than introducing a second backend now |

The current backend emits either in-process JIT code or an object artifact from the same validated MIR lowering without exposing Cranelift or object-library types to callers. Typed blocks, explicit terminators, block parameters, source spans, imports, and exports share the same backend-neutral contracts across the VM, JIT, and AOT paths. The shared `SystemLinker` models `Cc` and `MsvcLink`, validates ordered requests, supports direct and response-file invocation, and records structured failures. macOS integration and [Windows run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398) execute both link modes natively; the clean [schema-20 benchmark evidence](benchmarks.md) records both Tier-1 platforms. Production installation discovery, startup/runtime packaging, stable ABI adapters, and additional targets remain separate work.

A clean pre-implementation macOS AArch64 [debug-information inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) established the original negative baseline. The committed [source-mapped DWARF inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md) demonstrates that typed MIR carries a structurally validated source file, function span, and one source span for every instruction and terminator; Cranelift preserves them as backend-neutral JIT/AOT machine-code ranges; and Mach-O serialization emits a relocated DWARF v4 compile unit, subprogram entry, and line table. The representative add, minimum, and host-callback objects pass `dwarfdump --verify`; direct and response-file executables link and run; and both generated dSYMs pass verification while retaining the fixture path and all three functions. The committed [Windows x86-64 CodeView/PDB inspection](../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md) validates equivalent C13 symbols, types, line tables, and relocations in COFF, links them into a PE/PDB pair with LLD, and verifies function-to-source resolution with LLDB. This is cross-target debug-format evidence rather than a native Windows run. Source-language locals, unwind metadata, JIT debugger registration, optimized stepping, and a future source-language frontend remain separate acceptance work; backend selection must include their cost and quality.

Host linker discovery now searches `PATH` in deterministic candidate order, requires executable files on Unix, and returns a structured diagnostic containing the searched path and candidate names when no driver is available. The macOS integration uses the discovered driver, and the cfg-gated MSVC integration will exercise the same discovery after the developer environment initializes `PATH`. Installation discovery for Visual Studio, SDKs, and compilers outside such an environment remains separate work.

The root crate now expresses the initial tier-1 linker policy as a backend-neutral `LinkTarget` domain type. Exact `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` triples determine command flavor, default program, and ordered `PATH` candidates; unsupported triples remain structured errors instead of silently inheriting host policy. Explicit-target linker construction, discovery, platform integration, and the cross-target COFF test share this mapping. Supporting another target requires an explicit policy addition; installation discovery and target-specific startup/runtime inputs remain separate layers.

The historical backend experiments and their original evidence gaps remain documented in [benchmark evidence](benchmarks.md). They do not reopen the accepted source rules or completed Windows baseline.

## Diagnostics, LSP, and debugging

Diagnostics are structured values containing stable codes, primary/secondary spans, notes, fix-its, and machine-readable fields.

Implemented 2026-10-17: `lang-diagnostics` owns immutable UTF-8 snapshots, globally distinct snapshot IDs, half-open byte spans validated at character boundaries, one-based Unicode scalar positions, and zero-based UTF-16 columns for IDE adapters. Diagnostics require one primary label and may include cross-file secondary labels, notes, and fixes marked machine-applicable or speculative. Fix previews validate every referenced snapshot and reject overlapping or ambiguous edits before returning changed text; they do not mutate files. Plain-text rendering is deterministic and escapes control and directional-formatting characters. RFC 0002 codes `SRC0001`–`SRC0007` are represented, but source producers and their diagnostic conformance are still pending. This crate introduces no source grammar or serialized diagnostic format.

The LSP reuses the compiler’s parser, resolver, type checker, source map, and incremental query database. It must remain responsive with incomplete and invalid programs.

The debug stack has three layers:

- Native debug metadata: DWARF on macOS, CodeView/PDB on Windows.
- VM debug metadata: bytecode/source maps, lexical scopes, variable locations, and async task frames.
- A DAP adapter that merges native and VM frames, supports mixed breakpoints, and labels optimized/unavailable values honestly.

Source spans and stable symbol/function identifiers must survive every lowering stage. Debuggability is part of backend acceptance criteria.

## Toolchain UX

Use one provisional umbrella command until naming is settled:

```text
lang new       create a package
lang build     build incrementally
lang run       build and run, or execute a script
lang test      run unit, integration, and documentation tests
lang fmt       canonical formatting
lang lint      semantic and style diagnostics
lang doc       generate API documentation
lang lsp       start the language server
lang debug     start or connect the DAP adapter
lang package   manage dependencies and publication
lang toolchain install|list|default
```

The package manager needs a manifest, exact lockfile, checksums, offline mode, source replacement/mirrors, feature resolution, target conditions, and reproducible build metadata before a public registry.

## Self-hosting

1. Stage 0 Rust compiler implements the full accepted core semantics.
2. A deliberately small self-hosting subset is written in the new language.
3. Stage 0 builds stage 1; stage 1 builds stage 2.
4. Stage 1 and stage 2 outputs are compared under reproducible settings.
5. The Rust compiler remains a bootstrap reference until multiple platforms pass conformance and reproducibility checks.

Do not fork semantics between the Rust and self-hosted compilers. Both consume the same conformance suite, serialized test cases, and component ABI fixtures.

## Proposed Rust workspace boundaries

Create these crates only as implementation appears:

```text
crates/
  syntax        lossless tree, parser, source map
  diagnostics   stable diagnostic model
  hir           resolution and typed high-level IR
  semantics     inference, traits, effects, ownership
  mir           typed mid-level IR and verification
  bytecode      format, encoder, decoder, verifier
  vm            embedded runtime and interpreter
  codegen       backend-neutral lowering interface
  codegen-clif  Cranelift backend
  runtime       native runtime services
  component     IDL, canonical ABI, bindings
  lsp           language server
  dap           debug adapter
  cli           toolchain command
```

Avoid cyclic crate dependencies. Data-oriented IR crates should not depend on CLI, LSP, VM, or backend crates.

## Implementation review (2026-10-17)

The current implementation preserves MIR source maps in bytecode and native ranges, records actual CodeView parameter/result types, rejects NUL function names and collisions with host-import symbols, and owns JIT executable mappings until the compiled function is dropped. Its narrow unsafe release is justified by private code pointers and invocation borrowing the owner; error paths release unpublished mappings too.

Component validation checks malformed directly constructed SemVer values at every boundary and rejects borrowed results. All interface capabilities are preflighted before any provider/lock lookup. Both decoder schemas check that declared collections can fit the remaining payload before reserving storage. These checks do not authenticate artifacts: selected digests are supplied metadata until a future loader verifies actual artifact bytes.

Rust adapter cleanup contains destructor-error formatting/destruction and panic-payload destruction failures, continues independent cleanup, and poisons both instances when an active call is abandoned. Abandonment cannot run host destructors without the host callback; the host must tear down quarantined values. Recursive panic-payload destruction may intentionally leak the secondary payload. Abort-mode panics, foreign unwinding, and direct native callbacks are outside this containment boundary.

Response files are exclusively created and retained; callers must use fresh paths on retries. Existing files and symlinks are not overwritten. Linker configuration and native callbacks are trusted host inputs, not script capabilities.

Sources: locked Cranelift JIT 0.136.3 [release-memory contract](https://docs.rs/cranelift-jit/0.136.3/cranelift_jit/struct.JITModule.html#method.free_memory), verified against the installed versioned source on 2026-10-17 (source publication/update date not stated); Microsoft [CodeView type definitions](https://github.com/microsoft/microsoft-pdb/blob/master/include/cvinfo.h), living source without a displayed publication date, verified 2026-10-17. Repository implementation and regression tests updated 2026-10-17.

## Required validation

- Parser/formatter round trips and malformed-input recovery.
- Compile-pass and compile-fail diagnostics with stable error codes.
- MIR and bytecode verifier tests.
- Interpreter/native differential tests.
- Cross-version ABI and package compatibility fixtures.
- Fuzzing for parsers, manifests, bytecode, debug data, and component metadata.
- Miri/sanitizer runs for the Rust implementation where practical.
- Windows/macOS CI and representative benchmarks.
- Bootstrap stage comparison before claiming self-hosting.

