<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate regular-file handling for CLI artifacts

Reconciled 2026-10-30 JST: see the matching row in the [repository review](../review-2026-10-30.md) for current findings, fixes, validation and explicitly bounded follow-ups. This file preserves the original investigation scope; inherited observations below are not a claim that the issue remains unfixed.

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Does `lang run --artifact <path>` validate file kind without blocking before bounded artifact reading?

**Why this is one independent unit:** One binary-file CLI boundary can be checked with an existing valid artifact, independently of source/import loading.

## Inherited observation and starting evidence

The 2026-10-30 review reported a FIFO artifact blocking for three seconds. `read_artifact` near baseline main line 1135 opened the path before applying byte limits.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`src/main.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/main.rs) — Git last-change date at baseline: 2026-10-29.
- [`tests/artifact_cli.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/tests/artifact_cli.rs) — Git last-change date at baseline: 2026-10-28.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Read `read_artifact` and its CLI caller. Use `cargo test --locked --test artifact_cli -- --list` to identify the existing artifact producer/consumer fixture and inspect only its setup.
2. Produce one tiny valid artifact in a private directory using that fixture's command; run it as the ordinary-file control.
3. On Unix, pass a FIFO instead to the same artifact option. Use a three-second child timeout, finally cleanup, and explicit process reaping for both cases.
4. Record file-open versus decode failure, exit code, and diagnostic. Propose a handle-based rejection criterion preserving the existing artifact byte limit and version validation.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-artifact-file-input-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Malformed artifact campaigns, format changes, source loading, package installation, and fixes.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/artifact-file-input.md` as the English canonical report and `docs/investigations/artifact-file-input.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

The report gives a valid-artifact control, the FIFO outcome, and a bounded-file-read acceptance criterion. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
