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

The macOS workflow runs the full baseline on both `macos-26` and `macos-15` ARM64 runners and asserts the actual architecture so cfg-gated native tests cannot silently be skipped on an Intel runner. Native debugger validation, isolated SDK installation and source/Rust/C benchmarks also cover both macOS versions; Windows continues to use `windows-2025` x86-64 MSVC. Matrix jobs disable fail-fast so a failure on one OS does not cancel validation on another. Source: [GitHub runner-image catalog](https://github.com/actions/runner-images/blob/main/README.md), updated 2026-09-17, verified 2026-10-27. New workflow execution is not evidence until a remote run succeeds.

SDK artifacts, debugger caches/evidence and benchmark artifacts include the runner label to keep the two macOS builds separate. Each isolated SDK consumer downloads the artifact built for its own OS and target; this checks installation on each OS, not cross-version binary compatibility. The benchmark workflow's `macos` selector runs both macOS versions, `windows` runs only Windows, and `both` runs all three environments.

### CI execution and timing

Each tier-1 OS runs three independent Cargo target groups using `python3 tools/ci/run_tests.py <core|native|owned>` (`python` on Windows). `owned` contains `native_owned_boundary`, `native` contains the other `native_*` integration targets, and `core` contains all remaining integration targets, root library/binary/example/benchmark targets, and every other workspace member. Targets come from Cargo metadata, so new tests are included automatically. `--dry-run` prints the commands. The partition validator runs with `python3 -m unittest discover -s tools/ci -p 'test_*.py'`.

Together these groups retain `cargo test --locked --workspace --all-targets --all-features` coverage on **each** OS. Libtest still runs tests within each executable as before, retaining process-local fixture reuse. Formatting, Clippy, documentation and validator checks run once per OS in `core`; fuzz lint runs on both macOS versions. Windows C ABI and linker tests run once in `core`, instead of also being repeated before the full suite. A separate `--no-run` pass is unnecessary because `cargo test` builds its selected executables. The complete schema 20 Windows benchmark runs concurrently on its own runner, with unchanged samples, assertions and artifact upload. SDK isolation and debugger workflows retain their distinct end-to-end checks.

Rust dependency caches are keyed by toolchain, Cargo manifests/lockfiles and build environment, with explicit OS-version and group keys. Workspace outputs and test results are not reused; every group executes its tests on every run. Cache misses remain valid. The fixed action revision is Swatinem/rust-cache v2.9.3 (released 2026-10-20); restoring dependencies cannot eliminate test execution time. Additional runners trade some repeated compilation and runner minutes for shorter elapsed time. Measure cache restore/save overhead and group balance before adding more groups.

The 2026-10-28 JST investigation used successful baseline commit `b62c5fa`: [Windows run](https://github.com/carimatics/lang/actions/runs/40538531740) took 51m36s from run creation to completion (tests 36m22s, benchmark 7m28s, preliminary ABI/linker build and tests 2m12s); [macOS run](https://github.com/carimatics/lang/actions/runs/40538531591) took 29m49s. The longest Windows test executable, `native_owned_boundary`, took 10m35s. These are individual hosted-runner observations, not performance guarantees. No failure-prefix cases, OS coverage, debug assertions or benchmark samples were removed.

Sources: [Cargo test reference](https://doc.rust-lang.org/cargo/commands/cargo-test.html) (explains serial execution of multiple test targets), [GitHub matrix documentation](https://docs.github.com/en/actions/using-jobs/using-a-matrix-for-your-jobs), and [rust-cache README](https://github.com/Swatinem/rust-cache/tree/v2.9.3). Page update dates are not published; verified 2026-10-28 JST. The linked baseline runs completed 2026-10-28 JST.

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

