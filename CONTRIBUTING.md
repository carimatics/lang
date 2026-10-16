<!-- canonical-language: en; japanese-translation: CONTRIBUTING.ja.md -->

# Contributing

[日本語版](CONTRIBUTING.ja.md)

This project is in the research and bootstrapping phase. Small, testable vertical slices are preferred over broad scaffolding.

## Setup

Rust is pinned in `rust-toolchain.toml`. If you use `mise`, install the auxiliary tools with:

```bash
mise install
```

Verify the checkout with:

```bash
cargo test --workspace --all-targets --all-features
```

## Required checks

Before submitting a change, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

## When an RFC is required

Start from `docs/rfcs/0000-template.md` when changing any of the following:

- Syntax or source compatibility
- Type inference, generics, traits, effects, or error semantics
- Ownership, borrowing, regions, reference counting, destruction, or `unsafe`
- Concurrency, cancellation, atomics, or the memory model
- FFI, calling conventions, component interfaces, plugin capabilities, or ABI stability
- Bytecode, serialized IR, package manifests, lockfiles, or registry behavior
- Toolchain compatibility, editions, deprecation, or migration policy

An RFC must distinguish accepted requirements from hypotheses and must include alternatives, costs, migration, diagnostics, implementation stages, and test strategy.

## Testing strategy

`cargo test --test repository_consistency` checks roadmap totals, mirrored task states, and RFC status/update dates. Run it after planning changes. Both tier-1 CI workflows run the full baseline; the macOS workflow also compiles the separate decoder fuzz workspace. Compiling fuzz targets is not a fuzz campaign; longer fuzzing remains explicit validation work.

The tier-1 AOT integrations share `tests/fixtures/aot_harness.c`. Its optimized C callbacks use unsigned arithmetic and bit-preserving conversion to match Rust wrapping behavior, and exercise `i64` extremes, allocation sums, and overflowing Fibonacci as well as ordinary inputs.

The macOS workflow uses the `macos-15` ARM64 label and asserts the actual architecture so cfg-gated native tests cannot silently be skipped on an Intel runner. Source: [GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners), living documentation without a publication/update date, verified 2026-10-17. New workflow execution is not evidence until a remote run succeeds.

Prefer observable and cross-backend tests:

- Parser and formatter round-trip tests
- Compile-pass and compile-fail diagnostic tests
- Type and ownership checker unit tests
- MIR and bytecode verification tests
- Differential execution tests between the VM and native backend
- ABI and embedding tests across independently built components
- Property tests and fuzzing for parsers, decoders, and verifiers
- Windows and macOS integration tests
- Compile-time, runtime, memory, and binary-size benchmarks

## Pull requests

- Keep unrelated refactors separate.
- Explain the user-visible behavior and architectural impact.
- Link the relevant RFC or state why one is unnecessary.
- Include tests and documentation with the implementation.
- Do not weaken safety checks to improve a benchmark without documenting the new invariant.

