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

Workspace tests also exercise a minimal embedding path: a backend-neutral host-function ID resolves a typed `extern "C" fn(i64) -> i64` registration, and the same MIR host call runs through both the VM and Cranelift JIT. The VM exposes an opt-in instruction-fuel limit and reports consumed fuel; every bytecode instruction and terminator costs one unit, and fuel is checked before invoking a host callback. Existing execution functions remain unbounded. This prototype does not define a stable ABI and does not yet provide aggregate values, error transport, panic containment, capabilities, memory/recursion/task/wall-clock limits, interruption, or JIT resource enforcement.

`lang-codegen-clif` can also emit an object artifact for the same MIR, exporting the compiled function and leaving host callbacks as imports. In addition to the host-native path, callers can select an explicit target triple; a host-independent test emits `x86_64-pc-windows-msvc` COFF objects and verifies their architecture, format, exported function, and callback import. The root crate provides an initial `LinkRequest` and `SystemLinker` layer that preserves object, library-search-path, and logical-library order and validates non-empty inputs. The linker selects `cc` with `-o`, `-L`, and `-l` on non-MSVC hosts and `link.exe` with `/OUT:`, `/LIBPATH:`, and `.lib` names on MSVC hosts; callers can instead select an explicit driver, command flavor, and ordered prefix arguments. It exposes the complete command before execution and includes it in structured spawn, exit-status, and missing-output failures. Callers may also select an explicit response-file path: `Cc` arguments use UTF-8 with GNU escaping, while `MsvcLink` arguments use Windows escaping and BOM-prefixed UTF-16LE. The macOS AArch64 integration test executes both direct and response-file links through the shared path. A Windows x86-64 MSVC integration test is configured to compile a C harness with `cl.exe`, link and execute add, minimum, host-callback, and two-value loop-accumulator objects through both modes, and run in a SHA-pinned Windows Server 2025 CI workflow. No Windows runner result is recorded for this commit, so Windows link/execution and benchmark evidence remain open alongside toolchain discovery, broader target policy, startup code, a stable native ABI, and debug metadata.

The two-value accumulator prototype carries both a countdown value and a wrapping sum through block parameters on a back-edge. VM and JIT differential tests cover non-positive inputs and bounded positive loops; macOS AArch64 direct/response-file integration executes the AOT object, and the host-independent Windows test verifies its exported COFF symbol. It broadens MIR CFG evidence without defining source-level loop syntax or allocation semantics.

`SystemLinker::discover_host` scans `PATH` for the host-flavor candidates (`cc`, `clang`, and `gcc`, or `link.exe` on MSVC), skips non-executable files on Unix, and reports the searched path and candidates through a structured error. The platform integration tests use the discovered executable. Visual Studio, SDK, and compiler installation discovery outside an initialized toolchain environment remains open.

`LinkTarget` centralizes the initial tier-1 linker policy for `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`. It maps each exact triple to its linker flavor, default program, and ordered discovery candidates, rejects unsupported triples structurally, and drives explicit-target construction and discovery through `SystemLinker`. The object-format test and both cfg-gated platform integrations consume this shared policy. Additional target policies remain open.

The schema 5 benchmark harness measures a bounded countdown loop with block parameters and a back-edge, in addition to add, minimum, and host-callback workloads. It also measures system linking, linked executable size, single-launch process startup, and configurable batched startup on both tier-1 hosts: `cc` on macOS and `link.exe` on Windows MSVC. A clean macOS AArch64 schema 5 baseline is recorded; no remote Windows result is recorded yet. Windows CI runs schema 5 with the documented parameters, validates clean-checkout metadata, target identity, both startup rows, the batch size, and a positive executable size, and retains the JSON as a 30-day workflow artifact.

The typed MIR, bytecode verifier, VM, JIT, and object path also share signed `i64` comparison and an explicit control-flow graph. The two-argument minimum function branches from an entry block to separate return blocks, while a focused jump test covers an explicit edge. Typed block parameters and edge arguments represent phi-like merges without a separate phi instruction. MIR and bytecode validation reject invalid targets, unreachable blocks, entry-block predecessors or parameters, edge argument arity/type mismatches, and values that do not dominate their uses. A countdown workload carries an `i64` around a back-edge and produces the same result in the VM and JIT; the object path also emits it. A bytecode back-edge test proves that instruction fuel stops a non-terminating loop. Allocation and source-level control-flow semantics remain unimplemented.

Write either inspection target to a host-native object file for symbol and disassembly inspection:

```bash
cargo run --release --bin prototype-object -- add /tmp/prototype-add.o
cargo run --release --bin prototype-object -- min /tmp/prototype-min.o
cargo run --release --bin prototype-object -- host-callback /tmp/prototype-host-callback.o
```

The command reports the exported symbol, generated function-code size, and complete object size for the add, signed-minimum, or host-callback target. It does not invoke a linker.

Run the executable ownership experiments separately with `cargo test -p lang-semantics`. The async model covers source spans, structured diagnostics, borrow exclusivity, suspension, cancellation, and deterministic destruction. The graph model compares explicit shared/weak edges with arena-scoped cyclic storage. The service model tests actor-isolated mutation, sendable payload moves, FIFO mailboxes, and deferred cancellation cleanup. These models record design evidence only; they are not accepted language semantics.

Run the prototype backend measurement harness from a clean release build:

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000
```

The harness emits versioned JSON containing the commit and dirty state, toolchain, host, and system-linker metadata, generated JIT code and AOT object sizes, and distribution statistics for bytecode lowering, JIT and AOT compilation, direct calls, VM calls, host callbacks, control flow, and system linking. Schema version 3 adds countdown lowering/compilation/call measurements and artifact sizes, and renames `system_link_add_min` to `system_link_add_min_countdown` because the link set now includes all three objects. Countdown calls alternate between `64`, which takes the back-edge to zero, and `-7`, which returns immediately. Schema 2 files remain historical inputs and must be interpreted by their own version. These measurements are prototype evidence rather than accepted performance guarantees. Record results only from a quiet machine and retain the exact command and raw JSON.

Common checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

The workspace currently contains only the components exercised by the prototypes: `lang-mir`, `lang-bytecode`, `lang-vm`, `lang-codegen-clif`, the typed host-function registry in `lang-runtime`, and executable design models in `lang-semantics`. Additional boundaries from `docs/architecture.md` should appear only with their implementation.

## Design process

Changes to syntax, type semantics, memory safety, concurrency, ABI, package compatibility, or the plugin boundary require an RFC. Use the repository skill `$language-design-rfc` or start from `docs/rfcs/0000-template.md`.

Implementation work must preserve equivalent observable behavior between the bytecode and native backends unless an RFC explicitly documents a mode-specific restriction.

## Project status

- Maturity: research / pre-alpha
- Supported host platforms: macOS and Windows are the initial goal
- Initial implementation language: Rust
- License: not selected yet

