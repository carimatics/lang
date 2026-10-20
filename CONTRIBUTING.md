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

Before submitting a code change, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

For a documentation-only planning follow-up, run `cargo test --test repository_consistency`. Reuse the successful full baseline from the unchanged implementation; rerun broader checks when code, failures or unresolved concerns require them.

## Planning a delivery

Use a delivery board for the active milestone; the [M2 delivery board](docs/milestone-2.md) is complete and remains a historical record. Before starting the next milestone, review its dependencies and split the next outcome into observable 1–3-point tasks. Keep at most one implementation task doing, and record implementation evidence and validation before marking it done. Reviews and maintenance can use a scoped checklist in their report without assigning points or reopening M2.

Update the plan and its translation with the implementation; a separate planning commit is not required. A newly discovered prerequisite changes the plan before it changes the code. Keep the original exit gates separate from completed implementation slices. After two prerequisite deliveries without new source execution evidence, review the next vertical slice. Keep current contracts in RFCs/architecture and dated implementation checkpoints in the board or review history. Avoid repeating historical narrative and unchanged estimate claims after each commit.

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

During experimental early development, [RFC 0001's delegated decision policy](docs/rfcs/0001-project-charter.md#experimental-rfc-decisions) allows the coding agent to accept a reasonably sound RFC without individual user approval. Record the decision and remaining uncertainty; revise the RFC explicitly before implementing a later design change. Acceptance remains separate from implementation and stabilization.

## Testing strategy

`cargo test --test repository_consistency` checks roadmap totals, mirrored task states, RFC status/update dates, and complete README RFC indexes with current statuses. Run it after planning changes. Both tier-1 CI workflows run the full baseline and check formatting in the separate fuzz workspace; the macOS workflow also lints all fuzz binaries with warnings denied. Run `cargo fmt --manifest-path fuzz/Cargo.toml --check` and `cargo clippy --locked --manifest-path fuzz/Cargo.toml --bins -- -D warnings` after fuzz-target or shared dependency changes. Compiling fuzz targets is not a fuzz campaign; longer fuzzing remains explicit validation work.

Relevant native changes also trigger actual LLDB/CDB source-step and logical-stack validation on both hosts. A metadata-only test does not replace this evidence. The debugger validator tests run with `python3 -m unittest discover -s tools -p 'test_*.py'`; benchmark validator tests run with `python3 -m unittest discover -s tools/m2-bench -p 'test_*.py'` and are included in regular macOS CI.

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

