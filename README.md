<!-- canonical-language: en; japanese-translation: README.ja.md -->

# lang

[日本語版](README.ja.md)

`lang` is a pre-alpha research project for a new systems programming language and its complete developer toolchain. The implementation starts in Rust and is intended to become self-hosting after the language, compiler, and bootstrap process are stable enough to reproduce themselves.

The project is currently defining semantics and validating architecture. There is no stable syntax, ABI, package format, or release schedule yet.

Current capability snapshot (verified 2026-10-20 JST): ownership, collections, shared/arena storage, C boundaries, VM/native execution, an experimental SDK and both-host debugger validation are implemented. See the [M2 capability guide and complete exit audit](docs/milestone-2-capabilities.md) for workflows, evidence and limits.

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
- [Benchmark evidence: M2 source/Rust/C and historical prototypes](docs/benchmarks.md)
- [Architecture baseline](docs/architecture.md)
- [Implementation roadmap](docs/roadmap.md)
- [Milestone 1 capability guide](docs/milestone-1.md)
- [Milestone 2 capability guide and complete exit audit](docs/milestone-2-capabilities.md)
- [Whole-repository review, 2026-10-17](docs/review-2026-10-17.md)
- [Whole-repository review, 2026-10-20](docs/review-2026-10-20.md)
- [RFC process and template](docs/rfcs/0000-template.md)
- [RFC 0001: Project charter and compatibility policy (accepted)](docs/rfcs/0001-project-charter.md)
- [RFC 0002: Prototype integer and structured control-flow semantics (accepted)](docs/rfcs/0002-prototype-control-flow.md)
- [RFC 0003: Versioned component contracts and capability imports (accepted)](docs/rfcs/0003-versioned-component-contracts.md)
- [RFC 0004: Owned values, explicit sharing, and cyclic graph storage (accepted)](docs/rfcs/0004-owned-values-and-cyclic-graphs.md)
- [RFC 0005: Structured tasks, cancellation, and isolated services (accepted)](docs/rfcs/0005-structured-tasks-and-isolated-services.md)
- [RFC 0006: Source frontend and portable VM programs (implemented)](docs/rfcs/0006-source-frontend-and-portable-programs.md)
- [RFC 0007: Source ownership and core values (implemented)](docs/rfcs/0007-source-ownership-and-core-values.md)
- [RFC 0008: Cleanup, allocator authority, and C boundaries (implemented)](docs/rfcs/0008-cleanup-allocators-and-c-boundaries.md)
- [RFC 0009: Whole Option/Result ownership extraction (implemented)](docs/rfcs/0009-whole-sum-ownership-extraction.md)
- [RFC 0010: Owned REPL result previews (implemented)](docs/rfcs/0010-owned-repl-previews.md)
- [RFC 0011: Portable ownership artifacts (implemented)](docs/rfcs/0011-ownership-artifacts.md)
- [RFC 0012: Native ownership continuations and managed frames (implemented)](docs/rfcs/0012-native-ownership-continuations.md)
- [RFC 0013: Service byte-work budget (implemented)](docs/rfcs/0013-service-byte-work-budget.md)
- [RFC 0014: Explicit source panic (implemented)](docs/rfcs/0014-explicit-source-panic.md)
- [RFC 0015: Private C native object bridge (implemented)](docs/rfcs/0015-c-native-object-bridge.md)
- [RFC 0016: Indirect REPL result previews (implemented)](docs/rfcs/0016-indirect-repl-previews.md)
- [RFC 0017: C-aware ownership artifacts (implemented)](docs/rfcs/0017-c-aware-ownership-artifacts.md)
- [RFC 0018: Native compiler records (implemented)](docs/rfcs/0018-native-compiler-records.md)
- [RFC 0019: Ordinary owned native entry runtime (implemented)](docs/rfcs/0019-owned-native-entry-runtime.md)
- [RFC 0020: Experimental native SDK and scoped startup (implemented)](docs/rfcs/0020-native-sdk-and-startup.md)
- [Contribution guide](CONTRIBUTING.md)

## Development

Use Rust **1.99.0** with rustfmt and Clippy, pinned by `rust-toolchain.toml`. The [M2 capability guide](docs/milestone-2-capabilities.md) records implemented behavior, all 18 exit gates and limits; the [delivery board](docs/milestone-2.md) records current work and estimate changes.

```bash
cargo run -- check examples/conformance/owned_pipeline.lang
cargo run -- run examples/conformance/owned_pipeline.lang
cargo run -- compile --output target/owned.lbc examples/conformance/owned_pipeline.lang
cargo run -- run --artifact target/owned.lbc
cargo run -- repl
cargo run --quiet -- repl < examples/repl/owned.repl
cargo run --quiet -- lsp
```

The combined program prints 42. Choose fresh output paths. The CLI grants allocator authority for owned execution, registers no host imports and refuses C execution before effects. The REPL keeps definitions and uses fresh guest counters/storage for each evaluation, with bounded core/indirect previews.

Before finalizing code changes:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

### Native program output

`cargo run -- native --output PROGRAM SOURCE` publishes the original object, portable semantic/native records, checked plans, `bindings.rs` and applicable `main.rs`/`program.h`. Ordinary Copy/owned startup uses a zero-argument i64 entry; C libraries need an explicit host. See [producer tests](tests/native_compiler.rs).

### Native package verification API

`lang::native` verifies bounded manifests, paths, hashes, target/build/toolchain pairing and semantic/native records in owned snapshots. Keep snapshots unchanged while tools consume them. Hashes check integrity/pairing; native inputs remain trusted. See [package tests](tests/native_packages.rs).

### Scoped native SDK runtime API

Unsafe `lang_codegen_clif::sdk::runtime::{execute_copy, execute_owned, with_c}` binds the original linked image with no source compiler, JIT or guest interpreter at startup. Owned retains the allocator, Instance, controls and failures; Copy is synchronous/unmetered and refuses controls. C contexts/callbacks/dependencies stay within the original binding lifetime. See [runtime](crates/codegen-clif/src/sdk/runtime.rs) and [RFC 0020](docs/rfcs/0020-native-sdk-and-startup.md).

### Native SDK assembly

`python3 tools/build-native-sdk.py --output SDK` builds a fresh matching host SDK from exact Cargo outputs, with compiler/runtime/dependencies, C header, templates and bilingual instructions. Add `--release` for release; `panic=unwind` is required. See [SDK instructions](docs/native-sdk.md) and [assembly tests](tests/native_sdk.rs).

### Native executable linking

`lang link-native --sdk SDK --package PROGRAM --output EXE` checks the exact rustc release/commit/host and links bundled original code without Cargo, registry or repository lookup. C requires `--host HOST.rs`, optional repeated `--object HOST.o` and explicit context/provider/import grants. Outputs must be fresh; failures preserve existing output. See [full commands](docs/milestone-2-capabilities.md#build-a-native-executable) and [link tests](tests/native_link.rs).

### Native SDK isolated installation

[Run 39370391958](https://github.com/carimatics/lang/actions/runs/39370391958), `c54af31`, proves separate producer/consumer installation on both native hosts without consumer checkout/Cargo/repository inputs. Copy/owned/real-C execute after private source/SDK/program removal. See the [workflow](.github/workflows/native-sdk-install.yml) and [probe](tools/check-native-sdk-install.py). System libraries/toolchains remain external dependencies.

### Combined source conformance

The [combined program](examples/conformance/owned_pipeline.lang) exercises nested collections, owner transfers, shared/weak, arenas, recursive calls and cleanup, returning 42; its failure variant panics with −42. [Production-entry comparisons](tests/support/native_entry_runtime.rs), [actual AOT](tests/native_compiler.rs) and [real-C runtime](tests/support/c_exports_runtime.rs) reconcile values, usage, source failures and cleanup/refusal boundaries.

Both-host conformance, immutable Rust/C measurements and actual LLDB/CDB execution have separate named-revision evidence in the [final audit](docs/milestone-2-capabilities.md). Default linking does not publish debugger sidecars. Owned debug evidence covers physical native/runtime frames; logical continuation stepping, source variables and mixed VM/native stacks remain later work.

## Design process

Changes to syntax, type semantics, memory safety, concurrency, ABI, package compatibility, or the plugin boundary require an RFC. Use the repository skill `$language-design-rfc` or start from `docs/rfcs/0000-template.md`.

Implementation work must preserve equivalent observable behavior between the bytecode and native backends unless an RFC explicitly documents a mode-specific restriction.

## Review scope and current limits

The 2026-10-17 review hardened JIT memory reclamation, bytecode span preservation, CodeView machine types, bounded decoding, semantic-version validation, whole-request capability preflight, and Rust adapter cleanup. Linker response files now require a new path; existing files and symlinks are rejected without truncation. Repeated callers must choose a fresh response-file path or explicitly remove their own old file.

Historical prototype captures remain immutable and predate the JIT reclamation fix; M2 source/Rust/C captures are a separate named-revision experiment. See [benchmark evidence](docs/benchmarks.md) before interpreting current performance. Direct native host callbacks remain trusted and must not unwind; this is not a sandbox.

## Project status

- Maturity: research / pre-alpha
- Validated native hosts: macOS AArch64 and Windows x86-64 (experimental)
- Initial implementation language: Rust
- License: not selected yet

