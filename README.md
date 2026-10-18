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
- [RFC 0008: Cleanup, allocator authority, and C boundaries (accepted)](docs/rfcs/0008-cleanup-allocators-and-c-boundaries.md)
- [RFC 0009: Whole Option/Result ownership extraction (accepted)](docs/rfcs/0009-whole-sum-ownership-extraction.md)
- [RFC 0010: Owned REPL result previews (accepted; current-profile REPL implemented)](docs/rfcs/0010-owned-repl-previews.md)
- [Contribution guide](CONTRIBUTING.md)

## Development

The Milestone 1 source pipeline supports multi-file modules, typed functions and recursion, copy-only records/enums, exhaustive patterns, verified portable bytecode, and bounded VM execution. Source spans survive analysis, lowering, artifacts, and runtime stacks. The formatter and stdio LSP prototype use the same lossless parser and revisioned module/diagnostic database.

Requirements: Rust `1.99.0` with rustfmt and Clippy, pinned by `rust-toolchain.toml`; `mise` supplies auxiliary tool versions when needed.

```bash
cargo run -- check examples/analysis/ownership.lang
cargo run -- check examples/analysis/extraction.lang
cargo run -- run examples/scripts/answer.lang
cargo run -- run examples/scripts/bytes.lang
cargo run -- run examples/scripts/owned_bytes.lang
cargo run -- run examples/analysis/extraction.lang
cargo run -- run examples/scripts/utf8.lang
cargo run -- run examples/scripts/text.lang
cargo run -- run examples/scripts/owned_calls.lang
cargo run -- run examples/scripts/cleanup_calls.lang
cargo run -- run examples/scripts/owned_replace.lang
cargo run -- run examples/scripts/owned_aggregate.lang
cargo run -- run examples/scripts/arena.lang
cargo run -- run examples/scripts/arena_owned.lang
cargo run -- run examples/scripts/shared.lang
cargo run -- run examples/scripts/shared_owned.lang
cargo run -- run examples/scripts/vec.lang
cargo run -- run examples/scripts/vec_owned.lang
cargo run -- repl
cargo run --quiet -- repl < examples/repl/owned.repl
cargo run --quiet -- lsp
```

Source arenas execute allocation, checked reads, replacement and lexical teardown for Copy values, Bytes/Text, inline aggregates and same-region references. `arena.lang` prints `9` after replacing and reading a value; `arena_owned.lang` prints `5` after following a cycle of nodes with owning Bytes payloads. Region re-entry creates fresh generations. Reads end before mutation/exit; teardown releases nodes in reverse allocation order and their fields in reverse initialization order. See [source arena execution](docs/architecture.md#source-arena-execution), updated/verified 2026-10-18 JST.

Source shared/weak values execute explicit cloning, checked reads, replacement, weak creation/cloning/upgrades and cleanup for Copy values, Bytes/Text and inline aggregates, including recursive shared shapes. `shared.lang` prints `9`; `shared_owned.lang` prints `13` after rejecting a strong cycle and reading nested owning nodes through a borrowed guest call. Weak owners preserve metadata after payload expiration. Live reads exclude alias replacement; failed publications release incoming owners. Vec/Map payloads, indirect result previews, native execution and ownership artifacts retain their separate gates. See [source shared execution](docs/architecture.md#source-shared-and-weak-execution), updated/verified 2026-10-18 JST.

Source Vec values execute new/len/push/pop and checked shared/mutable element reads for Copy values, Bytes/Text and inline records/enums/Option/Result. `vec.lang` prints `9`; `vec_owned.lang` prints `12` after growing owning pairs, mutating a buffer, replacing a field, popping a value and forwarding a read through guest calls. Live element loans exclude push/pop; failed push cleans its evaluated owning argument. Nested collections/shared/arena payload integration, indirect previews, native execution and ownership artifacts retain their gates. See [source Vec execution](docs/architecture.md#source-vec-execution), updated/verified 2026-10-18 JST.

`check` validates Copy or RFC 0007/0009 ownership source through its applicable HIR/MIR analyses and exits 0 on success; it executes no guest code and does not require `main` or host bindings. `run` supports the Copy profile and an initial ownership profile with allocator authority, Bytes new/len/get/set/push/clone, consuming UTF-8 conversion, Text byte length/clone/concat/to-bytes/equality, nested Bytes/Text/Copy records/enums and Option/Result construction/extraction, lexical loans and cleanup. The answer script prints `42`; `bytes.lang` and `owned_bytes.lang` print `0`, the extraction/mutation example prints `1`, `utf8.lang` prints the byte length `3`, and `text.lang` prints `2` after checking independent clones and conversions. `owned_aggregate.lang` prints `9` after moving nested owners and reading projected references. Storage is released before printing the result. Extraction checks its case and reports a source-located guest panic on mismatch. Unsupported operations/payloads fail preflight.

The ownership profile also executes guest functions with Copy/owned parameters and results and borrowed parameters, including self/mutual recursion. `owned_calls.lang` prints `1` after recursive Bytes transfer and a borrowed call; `cleanup_calls.lang` prints `13` after nested aggregate/call cleanup. `owned_replace.lang` prints `3` after whole-owner/field replacement and a scalar loop. Assignment captures its destination before the RHS and publishes the new value before old-value cleanup. Return values stay protected until callee cleanup succeeds; failures include the entered source call stack. Verified 2026-10-18 JST; see [calls](docs/architecture.md#owned-guest-calls-and-returns), [failure exits](docs/architecture.md#owned-failure-exit-conformance) and [replacement evidence](docs/architecture.md#prepared-ownership-replacement).

Compile/runtime errors exit 1; usage errors exit 2. A script result is not an exit code. `--source-root DIR` selects file authority; otherwise check/scripts use the root file's parent and REPL sessions have no file-module authority. `run` and `repl` accept `--fuel`, `--host-calls`, `--frames`, and `--cells`. The REPL supports current-profile ownership definitions and expressions, including bounded Bytes/Text/nested result previews, with a fresh allocator and cleanup instance per evaluation. The CLI registers no host imports. Verified 2026-10-18 JST.

The embedding API `lang_vm::unit::owned::execute_preview` executes a named zero-argument ownership function by checked ID, renders bounded Bytes/Text/nested results, and releases all guest storage before returning its string. [The source library](examples/analysis/repl_preview.lang) is executed by `cargo test --test owned_execution preview::named_owned`. The frontend also infers a validated generated ownership wrapper's result through normal HIR/MIR checks; `cargo test --test owned_execution inference::` executes these inferred results. Embedding sessions use `Session::submit_with_allocator` to explicitly authorize ownership expressions; the original `submit` grants no allocator authority. Definitions commit without execution, and callers may intentionally reuse an allocator's cumulative quota. [Preview](docs/architecture.md#owned-result-previews), [inference](docs/architecture.md#ownership-expression-inference) and [session evidence](docs/architecture.md#ownership-repl-sessions), updated/verified 2026-10-18 JST.

The REPL commits successfully checked item definitions and evaluates one expression per submission with fresh VM counters. It prints scalars, qualified aggregates and current-profile owned previews, accepts multiline input, and supports `:reset` and `:quit`. Only source definitions persist; every guest result is released before output. Failed submissions preserve prior definitions and original-source diagnostics/stacks; host effects are not rolled back. Unsupported reachable ownership operations/results fail before provider callbacks. Later indirect-service previews remain gated.

Configure an LSP client to execute `target/debug/lang lsp` after `cargo build`. The stdio profile supports initialize/shutdown/exit, UTF-16 positions, full document synchronization, versioned diagnostics, watched-file refresh, and canonical four-space document formatting. Initialization grants at most one explicit local workspace root; without one, only open buffers are available. Code actions, completion, navigation, and debugger integration remain later work. See [architecture and protocol sources](docs/architecture.md#diagnostics-lsp-and-debugging).

The shared APIs are `lang::frontend`, `lang::repl::Session`, `lang_semantics::analysis`, `lang_semantics::database`, `lang_syntax::format`, and the `unit` modules in MIR, bytecode, VM, and Cranelift crates. Source analysis returns an explicit Copy or ownership program; the ownership variant retains checked HIR and MIR. Compiler/codec/cache budgets are configurable. VM defaults are 1,000,000 fuel, 1,000 host calls, 256 guest frames, and 1,000,000 live value cells. Interruption/deadlines act at guest safe points; trusted native callbacks are non-preemptible and must not unwind.

The whole-unit native API in `lang_codegen_clif::program` supports the RFC 0006 Copy profile, including guest calls, boolean interfaces, records/enums and patterns. Ownership-aware native lowering, public C adapters and production native packaging remain Milestone 2 work. `cargo run -- 20 22` retains the original scalar VM/JIT comparison. Cranelift 0.136.3 is the initial native backend. Native linker/debug evidence and historical platform measurements are documented in [architecture](docs/architecture.md), [benchmarks](docs/benchmarks.md), and the [roadmap](docs/roadmap.md); local source-conformance execution on 2026-10-17 is macOS AArch64, with Windows execution requiring CI evidence.

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

