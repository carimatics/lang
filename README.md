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

The accepted architecture baseline is deliberately narrower than the full vision; unimplemented extensions still require focused RFCs:

1. Build one language first. Offer safe high-level code and explicit low-level facilities in the same semantic model instead of starting with two languages.
2. Share parsing, type checking, ownership analysis, and a typed mid-level IR between all execution modes.
3. Lower the shared MIR to a portable bytecode VM for scripts and to native object code for AOT builds.
4. Add JIT compilation only after VM semantics, debug information, and the embedding API are stable.
5. Define a versioned interface description and canonical component ABI. Use direct internal calls when layouts are known, and generated thunks/handles at independently versioned boundaries.
6. Keep trusted native plugins and capability-restricted script/WebAssembly plugins as separate deployment tiers.
7. Treat self-hosting as a reproducible bootstrap milestone, not as an early implementation constraint.

The rationale and unresolved tradeoffs are documented in [the architecture baseline](docs/architecture.md).

## Documents

- [Language and ecosystem research](docs/research.md)
- [Prototype benchmark evidence](docs/benchmarks.md)
- [Architecture baseline](docs/architecture.md)
- [Implementation roadmap](docs/roadmap.md)
- [Milestone 1 capability guide](docs/milestone-1.md)
- [Whole-repository review, 2026-10-17](docs/review-2026-10-17.md)
- [RFC process and template](docs/rfcs/0000-template.md)
- [RFC 0001: Project charter and compatibility policy](docs/rfcs/0001-project-charter.md)
- [RFC 0002: Prototype integer and structured control-flow semantics](docs/rfcs/0002-prototype-control-flow.md)
- [RFC 0003: Versioned component contracts and capability imports](docs/rfcs/0003-versioned-component-contracts.md)
- [RFC 0004: Owned values, explicit sharing, and cyclic graph storage](docs/rfcs/0004-owned-values-and-cyclic-graphs.md)
- [RFC 0005: Structured tasks, cancellation, and isolated services](docs/rfcs/0005-structured-tasks-and-isolated-services.md)
- [RFC 0006: Source frontend and portable VM programs (implemented)](docs/rfcs/0006-source-frontend-and-portable-programs.md)
- [RFC 0007: Source ownership and core values (accepted)](docs/rfcs/0007-source-ownership-and-core-values.md)
- [Contribution guide](CONTRIBUTING.md)

## Development

The Milestone 1 source pipeline supports multi-file modules, typed functions and recursion, copy-only records/enums, exhaustive patterns, verified portable bytecode, and bounded VM execution. Source spans survive analysis, lowering, artifacts, and runtime stacks. The formatter and stdio LSP prototype use the same lossless parser and revisioned module/diagnostic database.

Requirements: Rust `1.99.0` with rustfmt and Clippy, pinned by `rust-toolchain.toml`; `mise` supplies auxiliary tool versions when needed.

```bash
cargo run -- run examples/scripts/answer.lang
cargo run -- repl
cargo run --quiet -- lsp
```

The script prints `42` and exits 0. Compile/runtime errors exit 1; usage errors exit 2. Its result is not an exit code. `--source-root DIR` selects file authority; otherwise scripts use the root file's parent and REPL sessions have no file-module authority. Both commands accept `--fuel`, `--host-calls`, `--frames`, and `--cells`. The CLI registers no host imports.

The REPL commits successfully checked item definitions and evaluates one expression per submission with fresh VM counters. It prints scalars or qualified aggregate values, accepts multiline input, and supports `:reset` and `:quit`. Failed submissions preserve prior definitions; host effects are not rolled back.

Configure an LSP client to execute `target/debug/lang lsp` after `cargo build`. The stdio profile supports initialize/shutdown/exit, UTF-16 positions, full document synchronization, versioned diagnostics, watched-file refresh, and canonical four-space document formatting. Initialization grants at most one explicit local workspace root; without one, only open buffers are available. Code actions, completion, navigation, and debugger integration remain later work. See [architecture and protocol sources](docs/architecture.md#diagnostics-lsp-and-debugging).

The shared APIs are `lang::frontend`, `lang::repl::Session`, `lang_semantics::database`, `lang_syntax::format`, and the `unit` modules in MIR, bytecode, VM, and Cranelift crates. Compiler/codec/cache budgets are configurable. VM defaults are 1,000,000 fuel, 1,000 host calls, 256 guest frames, and 1,000,000 live value cells. Interruption/deadlines act at guest safe points; trusted native callbacks are non-preemptible and must not unwind.

The initial native adapter handles the scalar/CFG profile and existing straight-line owned fixture; source guest calls and aggregates return unsupported-feature errors until Milestone 2. `cargo run -- 20 22` retains the original scalar VM/JIT comparison. Cranelift 0.136.3 is the initial native backend. Native linker/debug evidence and historical platform measurements are documented in [architecture](docs/architecture.md), [benchmarks](docs/benchmarks.md), and the [roadmap](docs/roadmap.md); local source-conformance execution on 2026-10-17 is macOS AArch64, with Windows execution requiring CI evidence.

Before finalizing code changes, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

## Design process

Changes to syntax, type semantics, memory safety, concurrency, ABI, package compatibility, or the plugin boundary require an RFC. Use the repository skill `$language-design-rfc` or start from `docs/rfcs/0000-template.md`.

Implementation work must preserve equivalent observable behavior between the bytecode and native backends unless an RFC explicitly documents a mode-specific restriction.

## Review scope and current limits

The 2026-10-17 review hardened JIT memory reclamation, bytecode span preservation, CodeView machine types, bounded decoding, semantic-version validation, whole-request capability preflight, and Rust adapter cleanup. Linker response files now require a new path; existing files and symlinks are rejected without truncation. Repeated callers must choose a fresh response-file path or explicitly remove their own old file.

Historical benchmark captures remain immutable and predate the JIT reclamation fix. See [benchmark evidence](docs/benchmarks.md) before interpreting current performance. Direct native host callbacks remain trusted and must not unwind; this is not a sandbox.

## Project status

- Maturity: research / pre-alpha
- Supported host platforms: macOS and Windows are the initial goal
- Initial implementation language: Rust
- License: not selected yet

