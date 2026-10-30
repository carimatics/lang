<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate a bounded component-wire fuzz campaign

Reconciled 2026-10-30 JST: see the matching row in the [repository review](../review-2026-10-30.md) for current findings, fixes, validation and explicitly bounded follow-ups. This file preserves the original investigation scope; inherited observations below are not a claim that the issue remains unfixed.

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Can the existing `component_wire` target run a reproducible, resource-bounded coverage-guided campaign with useful seeds and retained results?

**Why this is one independent unit:** Readiness of one existing codec target is bounded independently of a repository-wide fuzz service or lifecycle state machine.

## Inherited observation and starting evidence

The 2026-10-30 review found fuzz targets compiled/linted by CI but did not find a sustained coverage-guided campaign. It did not perform a new campaign. Existing deterministic negative tests remain valuable.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`fuzz/fuzz_targets/component_wire.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/fuzz/fuzz_targets/component_wire.rs) — Git last-change date at baseline: 2026-10-23.
- [`fuzz/Cargo.toml`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/fuzz/Cargo.toml) — Git last-change date at baseline: 2026-10-29.
- [`.github/workflows/macos-aarch64.yml`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/.github/workflows/macos-aarch64.yml) — Git last-change date at baseline: 2026-10-28.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Inspect this target's input interpretation, existing codec fixtures, and current CI commands. Identify one valid seed and one deterministic rejected seed without importing external corpora.
2. Record installed Rust/cargo-fuzz versions and instrumentation support. Do not alter the pinned stable project toolchain; absent compatible tooling is a reported prerequisite.
3. If already available, run only this target for at most 60 seconds with libFuzzer `-max_total_time=60 -timeout=2 -rss_limit_mb=512 -max_len=4096`, a private corpus/artifact prefix, and a 90-second outer watchdog.
4. Record the exact command, versions, coverage if provided, seed hashes, exit status, and retained artifacts. If a finding occurs, describe bounded reproduction/minimization and a deterministic regression; no finding means no finding in this limited run.
5. Draft a single-job proposal covering schedule, resource limits, artifact retention, and regression ownership; do not modify CI.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-component-wire-fuzz-readiness-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Long campaigns, additional targets, dependency/toolchain installation, CI deployment, Miri/sanitizer rollout, and exploit development.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/component-wire-fuzz-readiness.md` as the English canonical report and `docs/investigations/component-wire-fuzz-readiness.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

One target has either a reproducible bounded-run record or a specific tooling blocker, plus a concrete single-job proposal. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
