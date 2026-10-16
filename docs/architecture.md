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

[RFC 0002](rfcs/0002-prototype-control-flow.md) proposes the first source interpretation of the implemented MIR subset: `i64` parameters/results, wrapping addition, signed comparison, mutable locals, `if`/`else`, `while`, and explicit `return`. It specifies left-to-right evaluation and lowers values live across joins/back-edges through typed block arguments. The RFC is ready for a repository-owner decision but remains proposed; no parser, HIR, source diagnostics, formatter, or source-to-MIR lowering is implemented yet.

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

The graph model in `crates/semantics` compares explicit shared/weak ownership and arena-scoped storage under two shared-cycle policies:

- Shared nodes begin with an external root. Strong edges retain their targets, while weak edges do not.
- The default `Reject` policy rejects a strong edge that would close a shared-reference cycle with structured diagnostic `SEM0018`; a weak edge may close the same cycle, and upgrading an expired weak edge reports `SEM0021`.
- The experimental `CollectUnreachable` policy permits strong cycles. After an external root is dropped, it traces strong edges from every remaining live root and releases unreachable shared nodes in reverse creation order; weak edges do not keep a target live.
- Nodes in one arena may form arbitrary cycles because the arena owns their lifetime. Edges across arenas or between arena and shared storage report `SEM0017`.
- Dropping an arena releases its nodes exactly once in reverse creation order. Later use of a stale node handle reports `SEM0012`.
- Every operation carries a source span, and diagnostics retain a related allocation or edge span where one exists.

Neither shared-cycle policy is accepted semantics. The strict policy avoids silent reference-counted cycles, while the collector policy provides executable evidence that a cycle stays live while any root reaches it and is reclaimed after the last root disappears. The collector is intentionally synchronous and performs a full strong-edge trace on each root drop; it does not model incremental or concurrent collection, finalization, atomics, thread safety, destructor failure, or weak-reference ordering. A simpler alternative would support arena-only graph storage. Library-only conventions cannot guarantee stale-handle rejection or preserve the same diagnostic contract through lowering.

Review outcome: **needs prototype/measurement**. Both reject-and-weak and trace-and-collect behaviors are now executable; blocking questions are the production collection trigger and latency model, whether generation-tagged arena handles are required, and how the policies affect allocation cost, peak memory, and destruction observability. Surface syntax and graph-container APIs are optional follow-ups.

### Async borrow and cancellation experiment

The executable model in `crates/semantics` compares a deliberately strict baseline hypothesis with one explicit, less restrictive policy:

- Task-owned values remain live in the task frame across suspension.
- The default `RejectAll` policy rejects an active shared or exclusive borrow at a suspension point with structured diagnostic `SEM0007`; a borrow that ends before suspension is accepted.
- The experimental `AllowSharedFrameBorrows` policy permits active shared borrows of task-owned values across suspension but still rejects exclusive borrows with `SEM0007`.
- Cancellation is observed only while the task is suspended.
- Under the experimental policy, resumption preserves the shared borrow, while cancellation ends active borrows before dropping task-owned values exactly once in reverse creation order.
- Every operation carries a source span so failures identify both the boundary and the borrow that crossed it.

Neither policy is accepted semantics. The new comparison is evidence that a frame-resident shared borrow can survive suspend/resume and can be revoked before cancellation cleanup without weakening exclusive access. It does not model address stability, pinning, an exclusive borrow across suspension, or a borrow escaping its task frame. A library-only convention cannot enforce compiler borrow rules, while deferring the decision would move unresolved cancellation and self-reference constraints into MIR lowering. Later prototypes must cover nested scopes, partial initialization, destructor failure, panic containment, self-referential task frames, VM/native traces, and the runtime cost of retaining frame borrows.

Review outcome: **needs prototype/measurement**. The shared frame-resident case is now executable; blocking questions are whether pinned or exclusive borrows may cross suspension, how source semantics prove frame residency and address stability, and how cleanup behaves when destructors fail. Surface syntax and scheduler APIs are optional follow-ups.

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

The service model in `crates/semantics` compares a strict actor-isolation hypothesis with one explicit shared-mutation policy:

- Under the default `OwnerIsolated` policy, one owner task has direct mutable access to service state. Shared handles may enqueue messages but direct mutation from another task reports `SEM0031`.
- Under the experimental `ExclusiveCapability` policy, any task with a valid handle may begin a mutation when none is active. A task without a handle is rejected, and a concurrent begin reports the existing structured mutation-conflict diagnostic with the active mutation span.
- Sending moves a payload into the mailbox. A non-sendable payload reports `SEM0028`, and reuse after a successful move reports `SEM0027`.
- The owner still processes messages in FIFO order. A cancellation request from any valid handle is deferred until an active mutation ends, including a non-owner mutation under the experimental policy.
- Cancellation discards queued messages in FIFO order, drops every queued payload and the service state exactly once, then rejects later sends with `SEM0024`.
- Operations retain primary and related source spans under stable diagnostic codes `SEM0022` through `SEM0035`.

Neither mutation policy is accepted semantics. The exclusive-capability path is executable evidence for a lock-like alternative, but it does not model read/write capabilities, atomics, lock scheduling, fairness, priority inversion, poisoning, deadlock, reentrancy, panic behavior, mailbox backpressure, or distributed services. A simpler alternative would provide a single-threaded event loop without shareable handles. Library-only actors cannot enforce sendability or isolation as language-level properties.

Review outcome: **needs prototype/measurement**. Actor isolation and one exclusive-capability alternative are now executable; blocking questions are how send/share/mutation capabilities are derived, how lock scheduling and failure compose with cancellation and backpressure, and whether VM/native execution preserves equivalent ordering and cost. Surface syntax and standard actor APIs are optional follow-ups.

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

The VM prototype implements three composable opt-in limits—instruction fuel, a host-call budget, and a value-slot limit—plus an optional shared one-way interruption flag and absolute monotonic deadline. Each bytecode instruction and terminator consumes one fuel unit, and each entered host callback consumes one host-call unit; validation and arity checks occur before metering, and each dynamic limit is checked before the corresponding side effect. The VM compares the verified program's register count with the value-slot limit before allocating the register file. An already-set interruption or expired deadline stops execution before that allocation; subsequent safe points are the start of each instruction and terminator and the boundary immediately before an entered callback. The structured termination errors report instruction and host-call consumption at the observed safe point, and interruption takes precedence if both conditions hold. A callback can set the flag or overrun the deadline, but its internal work completes before the next VM safe point observes the condition. The limited API returns the value, both consumed counts, and the static slot footprint, while existing convenience APIs remain unbounded, non-interruptible, and without a deadline. A non-terminating back-edge test, a finite loop-carried countdown test, a zero-budget callback test, an undersized slot-limit test, pre-execution/callback-triggered interruption tests, and pre-execution/running-loop deadline tests demonstrate loop termination, callback suppression, pre-allocation rejection, and safe-point accounting. A slot is a backend-defined VM register entry, not a byte count, and does not account for host-callback allocations. These controls do not yet cover the heap, recursion, tasks, preemption inside callbacks, or native/JIT execution, so they are evidence for the embedding interface rather than a complete sandbox.

Two typed `i64` callbacks exercise host-owned allocation. The first performs one `Box` allocate/read/drop round trip. The second creates 64 boxes containing distinct wrapping values, keeps all 512 payload bytes live, sums them, and then drops them. VM and JIT dispatch to the Rust implementations, while linked AOT objects import corresponding C `malloc`/read/`free` implementations; macOS direct and response-file integration execute both wrappers. Benchmark schema 8 measures direct Rust, VM, and JIT latency for the single-allocation callback, and schema 11 adds the same three rows for the burst. Process-startup rows remain unchanged because callback allocation work would be dominated by process creation and loading. This proves that all three execution paths can cross the typed boundary for bounded allocation-backed operations and provides a less trivial VM/JIT boundary-cost workload. It is not a language heap, allocator ABI, ownership rule, garbage collector, memory limiter, allocator-overhead account, or representative application workload, so those design and cost questions remain unresolved.

Benchmark schema 9 adds `vm_countdown_interruptible_call`, which runs the same verified countdown program and input sequence as `vm_countdown_call` with an unset shared interruption flag. Comparing the two rows isolates the incremental acquire-load cost of enabling VM safe-point checks when no request occurs; it does not measure interruption latency or native/JIT enforcement. In the clean schema 14 macOS AArch64 result, the ordinary and unset-interruption VM countdown medians are 1.611 us and 1.641 us. The 29.868 ns (1.854%) increase is smaller than the interruption row's 15.866% RSD, so this sample shows no measurable regression and does not establish an overhead budget. The Windows CI contract is configured, but no remote Windows result is recorded yet.

Benchmark schema 10 adds `vm_countdown_deadline_call`, which runs the same bytecode and inputs with an absolute monotonic deadline set one hour after harness preparation. Comparing it with the uncontrolled row isolates the steady-state clock-check cost while the deadline remains unexpired; it does not measure expiration latency, scheduler accuracy, callback preemption, or native/JIT enforcement. The clean schema 14 macOS AArch64 result records ordinary and unexpired-deadline VM countdown medians of 1.611 us and 3.320 us, a 1.709 us or 106.071% increase. Windows CI requires the row, but no remote Windows result is recorded yet.

Benchmark schema 11 adds direct Rust, VM, and JIT rows for the 64-allocation burst while retaining all schema 10 fields. The clean historical macOS AArch64 result validates all 44 unique rows and records burst medians of 782.624 ns, 962.329 ns, and 786.270 ns. Windows CI retains all three burst rows, and macOS/Windows AOT integration executes the corresponding imported callback, but no remote Windows benchmark result is recorded yet.

Benchmark schema 12 keeps the 44 timing rows and adds a separate `heap_measurements` section with scope `host_callback_requested_payload_bytes`. A safe measuring heap runs the same generic single and burst allocation workload outside the timed loops, recording allocation count, total requested payload bytes, peak simultaneously live payload bytes, and live payload bytes after completion. Tests and Windows CI require `(1, 8, 8, 0)` for the single workload and `(64, 512, 512, 0)` for the burst. This is deterministic logical-payload evidence shared by direct, VM, and JIT callback paths, not allocator metadata, fragmentation, stacks, generated code, VM/JIT internal allocation, or process RSS. The historical clean macOS AArch64 schema 12 result records both profiles and all 44 unique timing rows. Schema 13 retains that contract; remote Windows evidence remains open.

Benchmark schema 13 retains every schema 12 timing, artifact, and heap field and adds `process_memory_measurement_scope`, `process_memory_metric`, and `process_memory_measurements`. After timing completes, the harness launches the allocation-free linked AOT executable once per sample and summarizes the operating system's peak process-resident metric in bytes. macOS reads `ru_maxrss` through `/usr/bin/time -l`; Windows reads `System.Diagnostics.Process.PeakWorkingSet64` through Windows PowerShell. Both metrics include private and shared resident pages and loader/runtime state, so values are platform-specific process envelopes rather than allocator-overhead or representative-application measurements. Unit tests validate distribution rendering and the macOS parser, a native smoke run validates the macOS path, and Windows CI requires a positive ordered 31-sample distribution. The historical clean schema 13 macOS AArch64 baseline records all 31 samples as 1,671,168 bytes with 0.000% RSD; remote Windows evidence remains open.

Benchmark schema 14 retains every schema 13 field and adds direct Rust, VM, and JIT timing rows for the allocation-churn callback plus a third exact requested-payload heap profile. The callback allocates 64 boxed `i64` values, consumes every other allocation, fills those 32 holes with replacements, and then consumes the final 64; the deterministic profile is `(96 allocations, 768 total bytes, 512 peak live bytes, 0 live bytes afterward)`. VM/JIT differential tests cover wrapping results including integer extremes, macOS AArch64 direct/response-file integration executes the corresponding C `malloc`/`free` import, host-independent Windows COFF validation checks `lang_host_i64_3`, and the cfg-gated MSVC integration and Windows workflow enforce the same contract. A clean 31-sample macOS AArch64 run records all 47 unique timing rows, all three exact heap profiles, direct Rust/VM/JIT churn medians of 1.192 us, 1.348 us, and 1.173 us, and a linked-process peak-memory distribution from 1,654,784 to 1,671,168 bytes. This adds lifetime-interleaving evidence without claiming a variable-size or representative application workload; allocator metadata/fragmentation and remote Windows execution remain open.

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

A clean macOS AArch64 [debug-information inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) of the add, minimum, and host-callback Mach-O objects finds external function/import symbols but no `__debug_*` section, DWARF compile unit, line table, source variable, parameter, or local. The current AOT prototype therefore preserves linker-visible boundaries but does not map MIR source spans into native debug metadata. Linked executable/dSYM behavior, unwind metadata, Windows CodeView/PDB, JIT debugger registration, optimized stepping, and representative source programs remain unmeasured; backend selection must include the cost and quality of implementing them.

Host linker discovery now searches `PATH` in deterministic candidate order, requires executable files on Unix, and returns a structured diagnostic containing the searched path and candidate names when no driver is available. The macOS integration uses the discovered driver, and the cfg-gated MSVC integration will exercise the same discovery after the developer environment initializes `PATH`. Installation discovery for Visual Studio, SDKs, and compilers outside such an environment remains separate work.

The root crate now expresses the initial tier-1 linker policy as a backend-neutral `LinkTarget` domain type. Exact `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` triples determine command flavor, default program, and ordered `PATH` candidates; unsupported triples remain structured errors instead of silently inheriting host policy. Explicit-target linker construction, discovery, platform integration, and the cross-target COFF test share this mapping. Supporting another target requires an explicit policy addition; installation discovery and target-specific startup/runtime inputs remain separate layers.

The schema 3 benchmark uses that policy for system-link measurements on both initial tier-1 hosts. It extends the measured MIR set with a bounded countdown loop that carries an `i64` block parameter around a back-edge, precompiles the platform C harness, validates one linked executable, and times repeated `cc` or `link.exe` invocations without including compiler setup or linker discovery. Windows CI validates the machine-readable envelope and uploads it as a time-bounded artifact; benchmark values become project evidence only after a clean runner result is inspected and recorded.

Schema 4 keeps the same backend-neutral link path and adds the linked executable's byte size plus a repeated process-startup row. The startup measurement includes process creation, platform loader work, the tiny AOT harness, output collection, and termination, after one validation run and without flushing operating-system caches. This distinguishes the measured warm-cache process cost from cold boot, loader-only cost, or a complete runtime distribution size. A clean macOS AArch64 result is recorded; its 49.824 ms outlier and 276.388% startup RSD require methodology refinement before setting a budget, and Windows evidence remains open.

Schema 5 retains that single-launch row and adds a configurable batched startup row. Each sample runs the requested number of sequential launches and reports elapsed time per launch, reducing the leverage of one delayed launch on the sample while preserving the single-launch row for tail-sensitive evidence. The two rows remain semantically distinct: batching estimates amortized process-launch cost and does not replace individual startup latency. A clean macOS AArch64 result reduces startup RSD from 256.272% for single launches to 4.688% for ten-launch batches while keeping similar 1.487 ms and 1.499 ms medians; Windows comparison remains open.

A broader CFG prototype now carries two `i64` values—a descending counter and wrapping accumulator—through block parameters on the same back-edge. A conditional-sum variant branches inside the loop, adds or skips the current value, merges both paths through block parameters, and then takes the shared back-edge. MIR validation, bytecode execution, and Cranelift JIT agree for non-positive inputs, both conditional paths, and bounded positive loops. Native object emission is exercised by macOS direct/response-file link-and-run integration and by host-independent Windows x86-64 COFF symbol validation; the cfg-gated MSVC integration mirrors the execution path. This remains MIR evidence: source-level loop semantics and clean conditional-workload benchmark evidence are still open.

Schema 6 adds the accumulator workload to bytecode lowering, JIT/AOT compilation, direct/VM/JIT calls, generated code and object sizes, and the tier-1 system-link/startup set. The platform series and linked-executable field are renamed because the link set now contains add, minimum, countdown, and accumulator objects. A clean macOS AArch64 result records the new rows and fields; Windows CI validates the same contract, but remote Windows comparison remains open.

Schema 7 adds the conditional-sum workload across the same dimensions and adds its fifth object to the tier-1 system-link/startup set. The platform series and linked-executable field are renamed again for the expanded set, and Windows CI validates the schema 7 conditional-loop and platform contract. A clean macOS AArch64 result records all 36 rows and the 17,104-byte linked harness; remote Windows comparison remains open.

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

