<!-- canonical-language: en; japanese-translation: ja/architecture.md -->

# Architecture proposal

[日本語版](ja/architecture.md)

Status: **proposed design baseline**. Individual semantics require RFC acceptance and prototypes.

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

## Memory model hypothesis

The design must acknowledge the fundamental tradeoff: memory safety, general shared object graphs, simple syntax, deterministic destruction, zero overhead, and no GC cannot all be maximized simultaneously.

Prototype this model:

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

Open questions requiring prototypes include cyclic graph ergonomics, async suspension with borrows, self-referential values, destruction during cancellation, and the cost model of atomic `shared` references.

### Cyclic graph experiment

The graph model in `crates/semantics` compares two deliberately explicit ownership strategies:

- Shared nodes begin with an external root. Strong edges retain their targets, while weak edges do not.
- A strong edge that would close a shared-reference cycle is rejected with structured diagnostic `SEM0018`; a weak edge may close the same cycle, and upgrading an expired weak edge reports `SEM0021`.
- Nodes in one arena may form arbitrary cycles because the arena owns their lifetime. Edges across arenas or between arena and shared storage report `SEM0017`.
- Dropping an arena releases its nodes exactly once in reverse creation order. Later use of a stale node handle reports `SEM0012`.
- Every operation carries a source span, and diagnostics retain a related allocation or edge span where one exists.

This strict model avoids silent reference-counted cycles and makes region boundaries visible, but it does not define surface syntax, atomics, thread safety, destructor failure, or a production reclamation algorithm. The strongest competing design would permit strong cycles and add tracing or cycle collection; a simpler alternative would support arena-only graph storage. Library-only conventions cannot guarantee stale-handle rejection or preserve the same diagnostic contract through lowering.

Review outcome: **needs prototype/measurement**. Blocking questions are whether shared cycles should be rejected statically or dynamically, whether generation-tagged arena handles are required, and how both strategies affect allocation cost and destruction observability. Surface syntax and graph-container APIs are optional follow-ups.

### Async borrow and cancellation experiment

The executable model in `crates/semantics` tests a deliberately strict baseline hypothesis:

- Task-owned values remain live in the task frame across suspension.
- An active shared or exclusive borrow at a suspension point is rejected with structured diagnostic `SEM0007`; a borrow that ends before suspension is accepted.
- Cancellation is observed only while the task is suspended.
- Completion and cancellation drop task-owned values exactly once in reverse creation order.
- Every operation carries a source span so failures identify both the boundary and the borrow that crossed it.

This is the simpler alternative, not accepted semantics. The strongest competing design would permit selected frame-resident or pinned borrows across suspension. A library-only convention cannot enforce compiler borrow rules, while deferring the decision would move unresolved cancellation and self-reference constraints into MIR lowering. Later prototypes must compare those alternatives and cover nested scopes, partial initialization, destructor failure, panic containment, self-referential task frames, and VM/native traces.

Review outcome: **needs prototype/measurement**. Blocking questions are whether any borrow may cross suspension, how task-frame ownership and pinning interact, and how cleanup behaves when destructors fail. Surface syntax and scheduler APIs are optional follow-ups.

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

The service model in `crates/semantics` tests a strict actor-isolation hypothesis:

- One owner task has direct mutable access to service state. Shared handles may enqueue messages but direct mutation from another task reports `SEM0031`.
- Sending moves a payload into the mailbox. A non-sendable payload reports `SEM0028`, and reuse after a successful move reports `SEM0027`.
- The owner processes messages in FIFO order. A cancellation request from any valid handle is deferred until an active mutation ends.
- Cancellation discards queued messages in FIFO order, drops every queued payload and the service state exactly once, then rejects later sends with `SEM0024`.
- Operations retain primary and related source spans under stable diagnostic codes `SEM0022` through `SEM0035`.

This model does not define surface syntax, scheduler threads, atomic ordering, mailbox backpressure, fairness, panic behavior, or distributed services. The strongest competing design would expose shared mutable state through lock or atomic capabilities; a simpler alternative would provide a single-threaded event loop without shareable handles. Library-only actors cannot enforce sendability or isolation as language-level properties.

Review outcome: **needs prototype/measurement**. Blocking questions are how send/share capabilities are derived, how cancellation interacts with backpressure and failed handlers, and whether VM/native execution preserves equivalent ordering and cost. Surface syntax and standard actor APIs are optional follow-ups.

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

The VM prototype implements one narrow resource-control layer: an opt-in instruction-fuel budget. Each bytecode instruction and terminator consumes one unit; validation and arity checks occur before metering, and host callbacks run only after their instruction's fuel is consumed. The limited API returns both the value and consumed fuel, while existing convenience APIs remain unbounded. A non-terminating back-edge test and a finite loop-carried countdown test demonstrate that the budget covers bytecode loops. It does not yet cover memory, recursion, tasks, wall-clock time, external interruption, or native/JIT execution, so it is evidence for the embedding interface rather than a complete sandbox.

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

- Start with a Cranelift object/JIT prototype to validate MIR lowering, call conventions, and fast iteration.
- Keep backend APIs narrow and target-neutral.
- Measure release runtime, compile latency, code size, debug quality, and platform integration on representative programs.
- Add an LLVM backend if Cranelift cannot meet release performance, optimization, or debug requirements.
- Use system linkers initially; evaluate bundled LLD only when reproducibility and installation data justify it.

The current prototype emits either in-process JIT code or an object artifact from the same validated MIR lowering. The object artifact exposes bytes, the exported function symbol, and generated function-code size without exposing Cranelift or object-library types to callers. Object emission can use the native host ISA or construct an ISA from an explicit target triple. A host-independent test parses `x86_64-pc-windows-msvc` output as x86-64 COFF and checks both the exported MIR function and an undefined host-callback import; this proves cross-target object construction, not Windows linking or execution. Typed basic blocks with `Branch`, `Jump`, and `Return` terminators provide the first explicit CFG shared by MIR, verified bytecode, the VM, and Cranelift. Typed block parameters and edge arguments represent phi-like merges without a separate phi instruction. Validation requires every target to exist, every block to be reachable, the entry block to have no predecessors or block parameters, edge argument arity and types to match the target parameters, and each SSA definition to dominate its uses. Back-edges outside the entry block are valid; a countdown workload carries an `i64` through a back-edge and agrees across VM and JIT execution, while the object path emits the same function. The VM charges every dynamically executed instruction and terminator, so fuel deterministically stops a non-terminating loop. The root crate owns a target-neutral ordered `LinkRequest` and an initial host `SystemLinker` adapter. Requests preserve object, library-search-path, and logical-library order and reject empty values. The adapter models `Cc` and `MsvcLink` command flavors, lowers search paths and libraries to `-L`/`-l` or `/LIBPATH:`/`.lib`, selects the matching host driver and output syntax, and permits an explicit driver, flavor, and ordered prefix arguments. It exposes the complete command and preserves that command in structured spawn, process-failure, and missing-output diagnostics. Explicit response-file linking serializes lowered `Cc` arguments as GNU-escaped UTF-8 and `MsvcLink` arguments as Windows-escaped, BOM-prefixed UTF-16LE; validation rejects unsafe output/object path conflicts, non-Unicode arguments, NUL, and newlines before writing. macOS AArch64 integration coverage executes both direct and response-file links. A cfg-gated Windows x86-64 MSVC test mirrors both object-link-execute paths with `cl.exe` and `link.exe`; a Windows Server 2025 workflow configures the x64 developer environment and runs the full baseline with read-only permissions and full-SHA action pins. The test is not executed by the current macOS baseline, and no remote workflow result is recorded for this commit. Aggregate values, allocation, source-level control-flow semantics, production linker hardening such as toolchain discovery and broader per-target policy, startup code, stable ABI adapters, debug metadata, recorded Windows link/execution results, and Windows benchmarks remain separate work.

Host linker discovery now searches `PATH` in deterministic candidate order, requires executable files on Unix, and returns a structured diagnostic containing the searched path and candidate names when no driver is available. The macOS integration uses the discovered driver, and the cfg-gated MSVC integration will exercise the same discovery after the developer environment initializes `PATH`. Installation discovery for Visual Studio, SDKs, and compilers outside such an environment remains separate work.

The root crate now expresses the initial tier-1 linker policy as a backend-neutral `LinkTarget` domain type. Exact `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` triples determine command flavor, default program, and ordered `PATH` candidates; unsupported triples remain structured errors instead of silently inheriting host policy. Explicit-target linker construction, discovery, platform integration, and the cross-target COFF test share this mapping. Supporting another target requires an explicit policy addition; installation discovery and target-specific startup/runtime inputs remain separate layers.

## Diagnostics, LSP, and debugging

Diagnostics are structured values containing stable codes, primary/secondary spans, notes, fix-its, and machine-readable fields.

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

