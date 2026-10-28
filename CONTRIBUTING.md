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

### Personal Codex configuration

Updated: 2026-10-28 JST.

Codex reads `.codex/config.toml` as project configuration in a trusted checkout. This repository keeps that file **untracked and ignored**, so each contributor can store repository-specific preferences, local MCP endpoints, and individually approved tool settings without adding them to commits. Shell approvals remain in the separately ignored `.codex/rules/user.rules`.

The tracked [`.codex/config.example.toml`](.codex/config.example.toml) contains shareable starting defaults. On a new checkout, copy it to `.codex/config.toml` only if no personal config exists, then customize the copy. Keep existing files intact. The example is not loaded or merged automatically; review later changes to it and apply relevant defaults to your personal config manually. Each clone or worktree needs its own local setup. The optional RustRover endpoint in the example is commented out so a new checkout does not require that local server.

For a checkout from before this migration, back up `.codex/config.toml` outside the working tree **before pulling the change or switching to a branch that changes whether the file is tracked**. The migration removes the tracked copy, so an update can remove an unchanged file or stop on local modifications; switching to an older branch can overwrite an ignored copy. Restore your personal config after updating. Do not resolve a blocked update by discarding your settings. This migration preserves the initiating checkout's personal file while removing only its Git index entry; other checkouts need the backup/restore step.

Verify with `git ls-files -- .codex/config.toml` (no output), `git check-ignore -v .codex/config.toml` (the `.codex/.gitignore` rule), and `git status --short` (no config modification). A new `.gitignore` entry alone does not untrack an already tracked file. Keep personal approvals out of the example and never force-add the real config. Saving a file does not prove an already running session has reloaded it; verify effective settings in a new session when needed.

Source: [OpenAI Config basics](https://developers.openai.com/codex/config-basic), which documents project configuration paths, precedence, and the trusted-project requirement. Publication/update date is not provided; verified 2026-10-28 JST. Keeping the live config ignored and distributing an example is this repository's policy.

## Branch-to-merge procedure

Policy updated: 2026-10-28 JST. The [repository workflow rules](AGENTS.md#branch-and-pull-request-workflow) apply to a single prompt and to the related tasks in a Codex Goal.

1. Record the starting branch, remote/upstream, and working-tree state (`git branch --show-current`, `git remote -v`, and `git status --short --branch`). Preserve existing changes. Fetch the intended remote and inspect any divergence before starting; do not use a destructive reset. If the checkout is detached or the remote/base cannot be identified, resolve that before editing.
2. Before changing Git-managed files, create a branch for a coherent outcome, for example `git switch -c codex/describe-the-outcome`. Record its source branch as the PR base, even when that branch is not `main`. Continue related tasks on this branch. Split branches only when separate reviewable outcomes or dependencies justify it, not for every Goal subtask or delivery row.
3. Implement the scoped change, update translation pairs and relevant delivery evidence, and run the [required local checks](#required-checks). Inspect the diff, commit meaningful changes, and push the work branch to the intended remote. Do not edit, stage, or commit on `main`, or push task changes directly to a base branch.
4. Open a remote pull request with the recorded base and work branch as head. Explain the outcome, relevant RFCs or why none is needed, and validation results. Select a merge method that preserves any commit hashes used as delivery evidence; prefer a merge commit when those hashes must remain in the base history.
5. Check the PR's latest head SHA and wait for all applicable CI checks to succeed. Confirm that the expected workflows actually ran; pending, failed, cancelled, and unexpectedly absent checks block merging. Existing documented path/job conditions may make a check inapplicable; record the reason when relevant. Local validation and CI for an earlier SHA are insufficient. If the head changes, check the new head again.
6. Investigate failures and fix their underlying cause on the work branch, then push and repeat validation. Do not skip/delete failing tests, weaken assertions or CI coverage, or add a hack merely to turn CI green. Retry transient infrastructure failures only when the evidence supports that diagnosis. Satisfy required reviews and other repository protections without bypassing them.
7. Merge through the PR after the gate succeeds, and verify the remote PR reports `merged`. Fetch the remote, switch to the local base branch, and fast-forward it to the corresponding remote branch (for example `git merge --ff-only origin/main` when the recorded base is `main` on `origin`). If it has diverged, preserve local work and resolve the cause rather than force-resetting it. Repeat this lifecycle for each completed work branch, in dependency order when necessary.
8. At the end of the entire work unit, return to the branch recorded in step 1. Fetch and compare its local and remote tips: both counts in `git rev-list --left-right --count HEAD...@{upstream}` must be zero when that upstream is the intended remote branch. Confirm with `git status --short --branch` that no task changes remain uncommitted and that pre-existing user changes are preserved. Report the PR, CI outcome, merge confirmation, and synchronization result. If any step remains blocked, preserve the work and report the exact incomplete condition instead of declaring completion.

### Relationship to existing rules

- Delivery rows and meaningful commits remain the planning and evidence units; they do not each require a branch. A branch may contain several related deliveries, and a Goal may span several justified branches.
- The documentation-only planning exception below reduces repeated **local** validation. It does not waive PR CI or permit merging before applicable checks succeed.
- RFC 0001's delegated design acceptance remains in force. Accepting an RFC does not replace the branch, PR, CI, or merge requirements, and these requirements do not add routine owner reconfirmation.
- Updating `main` to commits already merged remotely is synchronization, not authoring changes on `main`. Preserve user changes throughout; permissions, conflicts, or divergence must be resolved or explicitly reported, never concealed by discarding work.

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

Updated: 2026-10-28 JST.

The `CI` workflow is the common entry point for pull requests and pushes to `main`; manual dispatch always selects full validation. Its change classifier permits a lightweight path only when every changed path is a known contributor document or personal-configuration example: the root README/AGENTS/CONTRIBUTING translation pairs, Markdown under `docs/`, `.codex/config.example.toml`, and the two `.codex` ignore files. The embedded `docs/native-sdk.md` and Japanese mirror always require full validation. Code, executable examples, other configuration, JSON/schema data, CI changes and unknown paths require full validation. Git comparisons use NUL-delimited names and both sides of renames; absent history, malformed events and empty comparisons also select full validation. PR comparisons use the merge base, while pushes compare `before` with `after`.

Documentation-only changes run the existing `cargo test --locked --test repository_consistency` on Ubuntu, without occupying tier-1 native runners. All other changes call the macOS, Windows and isolated-SDK workflows with their existing OS matrices and test scope. These workflows remain directly dispatchable for full manual checks; branch pushes no longer duplicate PR-triggered tests. The always-running **CI result** job accepts only successful selected jobs and the expected unselected skips; a failure, cancellation, missing job or invalid scope cannot pass. Use this stable check for the common gate, and continue to verify the separately triggered native-debug workflow when applicable. The classifier and gate are covered by `tools/ci/test_routing.py`, including actual Git histories, renames and divergent PR bases.

Windows runs three independent Cargo target groups using `python tools/ci/run_tests.py <core|native|owned>`. `owned` contains `native_owned_boundary` and `native_task_execution`, `native` contains the other `native_*` integration targets, and `core` contains all remaining integration targets, root library/binary/example/benchmark targets, and every other workspace member. macOS uses `python3 tools/ci/run_tests.py <core|native-all>`, where `native-all` combines `native` and `owned`. Four jobs across both macOS versions fit the five concurrent macOS slots observed in this repository; the initial six-job experiment made a complete test group wait for a runner. Targets come from Cargo metadata, so new tests are included automatically. `--dry-run` prints the commands. The partition validator checks both layouts with `python3 -m unittest discover -s tools/ci -p 'test_*.py'`.

Tier-1 CI passes `--optimized` to this runner. It sets **both** Cargo dev and test profiles to opt-level 1 and debuginfo 2, with debug assertions and overflow checks explicitly enabled. Environment settings reach nested Cargo builds, preserving the exact graph selected by native fixtures; release SDK/benchmark settings are unchanged. Run, for example, `python3 tools/ci/run_tests.py owned --optimized` to reproduce this locally, or omit the flag for the default Cargo profiles. Each command reports elapsed time even on failure. Standalone Rust conformance hosts also use opt-level 1 with assertions and overflow checks enabled, retaining their existing MSVC entry-stack settings. No guest limits, failure prefixes or test cases are removed.

Together these groups retain `cargo test --locked --workspace --all-targets --all-features` coverage on **each** OS. Libtest still runs tests within each executable as before, retaining process-local fixture reuse. Formatting, Clippy, documentation and validator checks run once per OS in `core`; fuzz lint runs on both macOS versions. Windows C ABI and linker tests run once in `core`, instead of also being repeated before the full suite (101 duplicate test executions removed). A separate `--no-run` pass is unnecessary because `cargo test` builds its selected executables. The complete schema 20 Windows benchmark runs concurrently on its own runner, with unchanged samples, assertions and artifact upload. SDK isolation and debugger workflows retain their distinct end-to-end checks.

Rust dependency caches are keyed by toolchain, Cargo manifests/lockfiles and build environment, with explicit OS-version and group keys. Workspace outputs and test results are not reused; every group executes its tests on every run. Cache misses remain valid. The fixed action revision is Swatinem/rust-cache v2.9.3 (released 2026-10-20); restoring dependencies cannot eliminate test execution time. Additional runners trade some repeated compilation and runner minutes for shorter elapsed time. Measure cache restore/save overhead and group balance before adding more groups.

Optimized test caches use a separate `checked-opt1` key. SDK producers cache release build dependencies after selecting the pinned toolchain; isolated consumers still have no checkout, build cache or repository test executable. The documentation job has its own Linux dependency cache. Cache contents never substitute for executing the selected checks.

The follow-up investigation on 2026-10-28 JST found medians of 18m08s for seven recent Windows runs and 16m54s for seven recent macOS runs under the previous shard configuration. A representative [Windows run](https://github.com/carimatics/lang/actions/runs/40628127920) spent 855.82s executing tests within its 913s owned test step, despite a dependency-cache hit. On a local M1 Max, three executions of `calls::control::managed_work_profiles_match_vm_at_finite_boundaries` had median 31.19s with the default test profile and 2.51s with opt-level 1 and both checks enabled. The optimized first build cost about 3m03s; this one-test local comparison is not an estimate of total CI acceleration. Compare cold and warm full-CI runs, job minutes and failures before further sharding or process-per-test changes. Sources: the linked GitHub run (completed 2026-10-28 JST) and [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) (update date unpublished, verified 2026-10-28 JST).

The implemented optimized `owned` partition was also checked locally on 2026-10-28 JST: all 77 ownership-boundary tests and 12 task tests passed, with the existing single ignored test unchanged. Their execution times were 71.01s and 17.93s, including the standalone conformance hosts; the whole command took 128.18s. This confirms the broader partition and nested build paths, while remote tier-1 validation remains the authority for platform compatibility. Documentation-routing changes are additionally exercised by a real documentation-only PR against the implementation branch before merging the common CI change.

The 2026-10-28 JST investigation used successful baseline commit `b62c5fa`: [Windows run](https://github.com/carimatics/lang/actions/runs/40538531740) took 51m36s from run creation to completion (tests 36m22s, benchmark 7m28s, preliminary ABI/linker build and tests 2m12s); [macOS run](https://github.com/carimatics/lang/actions/runs/40538531591) took 29m49s. The longest Windows test executable, `native_owned_boundary`, took 10m35s. These are individual hosted-runner observations, not performance guarantees. No failure-prefix cases, OS coverage, debug assertions or benchmark samples were removed.

The first [Windows experiment](https://github.com/carimatics/lang/actions/runs/40551325087) on `e72e49e` passed in 18m39s without a dependency cache hit. Its 2,307 full-suite test names and results exactly matched the baseline. `native_task_execution` then moved from `native` to `owned` to balance its measured 3m35s against the shorter group. The [initial macOS experiment](https://github.com/carimatics/lang/actions/runs/40551325072) exposed the runner queue described above. Both experiments were verified 2026-10-28 JST.

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

