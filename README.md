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
- [Architecture proposal](docs/architecture.md)
- [Implementation roadmap](docs/roadmap.md)
- [RFC process and template](docs/rfcs/0000-template.md)
- [Contribution guide](CONTRIBUTING.md)

## Development

Requirements:

- Rust `1.99.0`, pinned by `rust-toolchain.toml`
- `rustfmt` and Clippy, installed by the pinned toolchain
- `mise` when using the repository's auxiliary tool versions

Common checks:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo doc --no-deps
```

The repository is currently a single crate. The proposed workspace split in `docs/architecture.md` should be introduced incrementally as real compiler components appear, not as empty scaffolding.

## Design process

Changes to syntax, type semantics, memory safety, concurrency, ABI, package compatibility, or the plugin boundary require an RFC. Use the repository skill `$language-design-rfc` or start from `docs/rfcs/0000-template.md`.

Implementation work must preserve equivalent observable behavior between the bytecode and native backends unless an RFC explicitly documents a mode-specific restriction.

## Project status

- Maturity: research / pre-alpha
- Supported host platforms: macOS and Windows are the initial goal
- Initial implementation language: Rust
- License: not selected yet

