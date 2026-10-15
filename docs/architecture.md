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

