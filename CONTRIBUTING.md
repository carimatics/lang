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

