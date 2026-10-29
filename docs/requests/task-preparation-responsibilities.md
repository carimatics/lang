<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate the responsibilities of the task preparation closure

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Can the preparation closure be described as a few named phases while preserving its current failure priority, budget consumption, and cleanup?

**Why this is one independent unit:** One closure has a bounded control flow; an ordered responsibility map can be delivered without splitting files or introducing abstractions.

## Inherited observation and starting evidence

The review identified a closure near runner.rs line 597 combining entry selection, authority, control identity, work, reachability, service support, layout, and limit composition. It reported readability debt, not a reproduced malfunction.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`crates/runtime/src/task/runner.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/runtime/src/task/runner.rs) — Git last-change date at baseline: 2026-10-26.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Locate the closure by symbols if line numbers changed. Enumerate checks, mutations, charges, reservations, and early returns in execution order.
2. Map existing failure-priority, exact-budget, and cleanup tests to the relevant stages using targeted `rg`; do not run the whole task suite.
3. Propose at most three named phases: entry/authority checks, reachable-profile checks, and layout/budget preparation. List carried state and ordering invariants at each boundary.
4. Suggest scope-explicit names only for ambiguous budget variables. Select at most three existing focused tests for a future behavior-preserving refactor; optionally run only those tests.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-task-preparation-responsibilities-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Refactoring the closure, new generic frameworks, file splitting, semantic changes, and broad test cleanup.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/task-preparation-responsibilities.md` as the English canonical report and `docs/investigations/task-preparation-responsibilities.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

An ordered phase map, carried-state list, and focused regression selection explain how to preserve observable behavior. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
