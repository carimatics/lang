<!-- canonical-language: en; japanese-translation: README.ja.md -->

# lang

[日本語版](README.ja.md)

`lang` is a pre-alpha research project for a new systems programming language and its complete developer toolchain. The implementation starts in Rust and is intended to become self-hosting after the language, compiler, and bootstrap process are stable enough to reproduce themselves.

The project is currently defining semantics and validating architecture. There is no stable syntax, ABI, package format, or release schedule yet.

## Vision

The language should combine:

- Native performance suitable for systems and application development.
- Memory safety without a mandatory tracing garbage collector.
- A Kotlin-like productive surface: null safety, algebraic data types, pattern matching, generics, traits, extension-oriented APIs, closures, and structured concurrency.
- Explicit access to pointers, allocators, SIMD, operating-system APIs, and foreign functions behind auditable unsafe boundaries.
- AOT compilation and an embeddable script runtime that can call each other in the same process.
- First-class tooling: diagnostics, formatter, linter, tests, package/build management, LSP, DAP, documentation, and toolchain installation.
- A path from one language frontend to multiple domain-focused frontends that share an interface model and compiler pipeline.

## Initial direction

The current design hypothesis is deliberately narrower than the full vision:

1. Build one language first. Offer safe high-level code and explicit low-level facilities in the same semantic model instead of starting with two languages.
2. Share parsing, type checking, ownership analysis, and a typed mid-level IR between all execution modes.
3. Lower the shared MIR to a portable bytecode VM for scripts and to native object code for AOT builds.
4. Add JIT compilation only after VM semantics, debug information, and the embedding API are stable.
5. Define a versioned interface description and canonical component ABI. Use direct internal calls when layouts are known, and generated thunks/handles at independently versioned boundaries.
6. Keep trusted native plugins and capability-restricted script/WebAssembly plugins as separate deployment tiers.
7. Treat self-hosting as a reproducible bootstrap milestone, not as an early implementation constraint.

The rationale and unresolved tradeoffs are documented in [the architecture proposal](docs/architecture.md).

## Documents

- [Language and ecosystem research](docs/research.md)
- [Prototype benchmark evidence](docs/benchmarks.md)
- [Architecture proposal](docs/architecture.md)
- [Implementation roadmap](docs/roadmap.md)
- [RFC process and template](docs/rfcs/0000-template.md)
- [RFC 0001: Project charter and compatibility policy](docs/rfcs/0001-project-charter.md)
- [RFC 0002: Prototype integer and structured control-flow semantics](docs/rfcs/0002-prototype-control-flow.md)
- [RFC 0003: Versioned component contracts and capability imports](docs/rfcs/0003-versioned-component-contracts.md)
- [RFC 0004: Owned values, explicit sharing, and cyclic graph storage](docs/rfcs/0004-owned-values-and-cyclic-graphs.md)
- [RFC 0005: Structured tasks, cancellation, and isolated services](docs/rfcs/0005-structured-tasks-and-isolated-services.md)
- [Contribution guide](CONTRIBUTING.md)

## Development

Requirements:

- Rust `1.99.0`, pinned by `rust-toolchain.toml`
- `rustfmt` and Clippy, installed by the pinned toolchain
- `mise` when using the repository's auxiliary tool versions

Run the current VM/native equivalence prototype:

```bash
cargo run -- 20 22
```

The command constructs a validated typed MIR function, lowers it to verified register bytecode, executes it in the VM, compiles the same MIR with Cranelift JIT, and compares the results. Its wrapping `i64` addition is a prototype behavior, not accepted language semantics.

Workspace tests also exercise a minimal embedding path: a backend-neutral host-function ID resolves a typed `extern "C" fn(i64) -> i64` registration, and the same MIR host call runs through both the VM and Cranelift JIT. The VM exposes composable opt-in instruction-fuel and host-call budgets plus a value-slot limit, optional shared one-way interruption flag, and absolute monotonic deadline. Successful limited execution reports both consumed counts and the static register-slot footprint. Every bytecode instruction and terminator costs one fuel unit; each entered callback costs one host-call unit, and both dynamic limits are checked before the corresponding side effect. After validation and arity checking, the VM rejects a program whose register count exceeds the slot limit before allocating its register file. Interruption and deadline checks run before register-file allocation and at instruction, terminator, and callback boundaries; structured errors report both consumed counts, and interruption takes precedence when both conditions hold at one safe point. A callback may request interruption or overrun the deadline, which is observed after that callback at the next safe point. Existing convenience functions remain unbounded, non-interruptible, and without a deadline. The slot count is not a byte-based heap limit and does not include callback allocations. This prototype does not define a stable ABI and does not yet provide aggregate values, error transport, panic containment, capabilities, heap/recursion/task limits, preemptive callback interruption, or native/JIT resource enforcement.

`lang-codegen-clif` can also emit an object artifact for the same MIR, exporting the compiled function and leaving host callbacks as imports. In addition to the host-native path, callers can select an explicit target triple; a host-independent test emits `x86_64-pc-windows-msvc` COFF objects and verifies their architecture, format, exported function, and callback import. The root crate provides an initial `LinkRequest` and `SystemLinker` layer that preserves object, library-search-path, and logical-library order and validates non-empty inputs. The linker selects `cc` with `-o`, `-L`, and `-l` on non-MSVC hosts and `link.exe` with `/OUT:`, `/LIBPATH:`, and `.lib` names on MSVC hosts; callers can instead select an explicit driver, command flavor, and ordered prefix arguments. It exposes the complete command before execution and includes it in structured spawn, exit-status, and missing-output failures. Callers may also select an explicit response-file path: `Cc` arguments use UTF-8 with GNU escaping, while `MsvcLink` arguments use Windows escaping and BOM-prefixed UTF-16LE. The macOS AArch64 integration test executes both direct and response-file links through the shared path. A Windows x86-64 MSVC integration test is configured to compile a C harness with `cl.exe`, link and execute add, minimum, host-callback, two-value loop-accumulator, and conditional-sum objects through both modes, and run in a SHA-pinned Windows Server 2025 CI workflow. No Windows runner result is recorded for this commit, so Windows link/execution and benchmark evidence remain open alongside toolchain discovery, broader target policy, startup code, a stable native ABI, and debug metadata.

A clean macOS AArch64 [debug-information inspection](benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) of add, minimum, and host-callback objects records external symbols but no `__debug_*` section, DWARF compile unit, line entry, source variable, parameter, or local. This is negative backend evidence: source-level native debugging, linked/dSYM output, Windows CodeView/PDB, JIT debugger registration, and stepping quality remain unimplemented or unmeasured.

The two-value accumulator prototype carries both a countdown value and a wrapping sum through block parameters on a back-edge. A conditional-sum prototype adds a branch inside that loop and merges the add/skip paths through another pair of block parameters. VM and JIT differential tests cover non-positive inputs, both conditional paths, and bounded positive loops; macOS AArch64 direct/response-file integration executes both AOT objects, and the host-independent Windows test verifies their exported COFF symbols. These prototypes broaden MIR CFG evidence without defining source-level loop syntax or allocation semantics.

`SystemLinker::discover_host` scans `PATH` for the host-flavor candidates (`cc`, `clang`, and `gcc`, or `link.exe` on MSVC), skips non-executable files on Unix, and reports the searched path and candidates through a structured error. The platform integration tests use the discovered executable. Visual Studio, SDK, and compiler installation discovery outside an initialized toolchain environment remains open.

`LinkTarget` centralizes the initial tier-1 linker policy for `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`. It maps each exact triple to its linker flavor, default program, and ordered discovery candidates, rejects unsupported triples structurally, and drives explicit-target construction and discovery through `SystemLinker`. The object-format test and both cfg-gated platform integrations consume this shared policy. Additional target policies remain open.

The schema 15 benchmark harness preserves schema 14's bounded CFG, host-callback, allocation, heap-profile, and process-memory evidence and adds eight separately classified component-boundary rows. The latest clean macOS AArch64 baseline records scalar metadata validation at a 286.833 ns median, three-candidate interface negotiation at 842.584 ns, direct and validated scalar dispatch at 0.958 ns and 11.451 ns, generation-tagged owner and shared-borrow lifecycles at 10.661 ns and 22.731 ns, and canonical owner-transfer and borrow-scope traces at 412.334 ns and 347.250 ns. These are exact in-memory prototype paths, not stable performance budgets; adapter rows include preflight reservation plus trace/ledger construction and are not VM/JIT/AOT resource calls. The 47 general timing rows, three exact requested-payload heap profiles, and linked-process distribution remain present. A remote Windows result remains open. Windows CI requires the schema 15 component-series, heap, process-memory, churn, clean-checkout, target, timing-row, batch-size, and executable-size contract and retains the JSON as a 30-day workflow artifact.

The typed MIR, bytecode verifier, VM, JIT, and object path also share signed `i64` comparison and an explicit control-flow graph. The two-argument minimum function branches from an entry block to separate return blocks, while a focused jump test covers an explicit edge. Typed block parameters and edge arguments represent phi-like merges without a separate phi instruction. MIR and bytecode validation reject invalid targets, unreachable blocks, entry-block predecessors or parameters, edge argument arity/type mismatches, and values that do not dominate their uses. A countdown workload carries an `i64` around a back-edge and produces the same result in the VM and JIT; the object path also emits it. A bytecode back-edge test proves that instruction fuel stops a non-terminating loop. Allocation and source-level control-flow semantics remain unimplemented.

Write either inspection target to a host-native object file for symbol and disassembly inspection:

```bash
cargo run --release --bin prototype-object -- add /tmp/prototype-add.o
cargo run --release --bin prototype-object -- min /tmp/prototype-min.o
cargo run --release --bin prototype-object -- host-callback /tmp/prototype-host-callback.o
```

The command reports the exported symbol, generated function-code size, and complete object size for the add, signed-minimum, or host-callback target. It does not invoke a linker.

Run the executable ownership experiments separately with `cargo test -p lang-semantics`. The async model compares the strict `RejectAll` suspension policy with an explicit `AllowSharedFrameBorrows` alternative that permits shared task-frame borrows while retaining exclusive-borrow rejection and deterministic cancellation cleanup. The graph model compares the default `Reject` shared-cycle policy, an experimental `CollectUnreachable` tracing policy, and arena-scoped cyclic storage. The service model compares `OwnerIsolated` mutation with an `ExclusiveCapability` alternative while retaining sendable payload moves, FIFO mailboxes, and deferred cancellation cleanup. These models record design evidence only; they are not accepted language semantics.

Run the prototype backend measurement harness from a clean release build:

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000 \
  --startup-iterations 10
```

The harness emits versioned JSON containing the commit and dirty state, toolchain, host, and system-linker metadata, generated JIT code and AOT object sizes, linked executable size, requested-payload heap profiles, process peak-memory distributions, and distribution statistics for lowering, JIT/AOT compilation, direct/VM/JIT calls, host callbacks, control flow, linking, and process startup. Schema 15 adds separately labeled component metadata-validation, interface-negotiation, scalar-dispatch, and resource-lifecycle series; schema 14 adds allocation-churn timing and heap evidence; earlier schemas add the process-memory, heap, allocation, control-flow, and startup rows documented in [the benchmark evidence](docs/benchmarks.md). Platform series are renamed when their linked workload set grows. Consumers must branch on `schema_version`, and prior JSON files remain immutable evidence. These measurements are prototype evidence rather than accepted performance guarantees. Record results only from a quiet machine and retain the exact command and raw JSON.

The allocation-boundary prototype includes one callback that performs a single `Box<i64>` allocate/read/drop round trip, a second callback that creates 64 distinct boxes and keeps all 512 payload bytes live before dropping them, a third churn callback that consumes every other box before allocating 32 replacements into the freed slots, and a fourth variable-size callback that keeps 16 boxed slices with payloads from 8 to 128 bytes live simultaneously before folding one stored value from each allocation. The linked C harness performs corresponding `malloc`/read/`free` operations for AOT; macOS AArch64 executes all four callbacks through direct and response-file links, host-independent Windows COFF validation checks their imports, and the cfg-gated Windows MSVC path mirrors the link-and-run coverage. The clean schema 14 baseline records direct Rust, VM, and JIT medians of 11.931 ns, 188.068 ns, and 13.631 ns for the single-allocation callback; 775.515 ns, 959.024 ns, and 789.842 ns for the burst; and 1.192 us, 1.348 us, and 1.173 us for churn. It records exact requested-payload profiles of `(1, 8, 8, 0)`, `(64, 512, 512, 0)`, and `(96, 768, 512, 0)`, plus 31 linked-process peak-memory samples from 1,654,784 to 1,671,168 bytes. The variable-size callback is not part of schema 14, so timing and exact requested-payload profiling for its 16 allocations and 1,088 live payload bytes remain open. The schema 14 process result broadens process-level evidence, but does not define source-language allocation or ownership, choose a runtime allocator or collector, impose memory limits, isolate allocator overhead, or measure a representative allocation workload; those questions remain open.

Common checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

The workspace currently contains only the components exercised by the prototypes: `lang-mir`, `lang-bytecode`, `lang-vm`, `lang-codegen-clif`, the typed host-function registry, bounded experimental schema-1 component codec, scalar component-contract validator, and generation-tagged resource table in `lang-runtime`, and executable design models in `lang-semantics`. Additional boundaries from `docs/architecture.md` should appear only with their implementation.

## Design process

Changes to syntax, type semantics, memory safety, concurrency, ABI, package compatibility, or the plugin boundary require an RFC. Use the repository skill `$language-design-rfc` or start from `docs/rfcs/0000-template.md`.

Implementation work must preserve equivalent observable behavior between the bytecode and native backends unless an RFC explicitly documents a mode-specific restriction.

## Project status

- Maturity: research / pre-alpha
- Supported host platforms: macOS and Windows are the initial goal
- Initial implementation language: Rust
- License: not selected yet

