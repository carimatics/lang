<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate stack use for a bounded deep package graph

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Can the documented 256-package chain be processed on the default test-thread stack, and which recursive path accounts for failure?

**Why this is one independent unit:** One existing finite graph test and its DFS recursion are a small reproducibility/algorithm investigation, independent of package features.

## Inherited observation and starting evidence

The review reported `default_package_boundary_handles_a_deep_chain_without_unbounded_recursion` aborting on macOS arm64 with pinned Rust 1.99.0, including a standalone run. `RUST_MIN_STACK=8388608` passed as a diagnostic only. This observation concerns M4, not M3 completion.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`src/package/graph.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/package/graph.rs) — Git last-change date at baseline: 2026-10-29.
- [`tests/package_graph.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/tests/package_graph.rs) — Git last-change date at baseline: 2026-10-29.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Inspect the named test near baseline line 269 and `visit` near graph lines 573/672. Record package/depth limits and the recursion/cleanup path.
2. In an isolated child run `cargo test --locked --test package_graph default_package_boundary_handles_a_deep_chain_without_unbounded_recursion -- --exact` with a stable private TMPDIR and the inherited stack setting recorded.
3. Use an outer watchdog and reap any aborting child. If default-stack failure reproduces, run once with RUST_MIN_STACK=8388608 solely to test stack sensitivity; do not mark the default configuration passing.
4. Compare an explicit traversal stack with a documented depth limit while preserving cycle detection, ordering, and diagnostics. Give one minimal regression proposal; do not raise the test's stack or implement a traversal rewrite.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-package-graph-stack-depth-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Full package-graph review, unbounded graph generation, stack-size-only fixes, implementation changes, and M3 board edits.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/package-graph-stack-depth.md` as the English canonical report and `docs/investigations/package-graph-stack-depth.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

The exact test has a platform/configuration-specific result, a recursion explanation, and a bounded corrective proposal preserving graph semantics. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
