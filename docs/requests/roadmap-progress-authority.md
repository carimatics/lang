<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate duplicated active-progress information in the roadmap

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Which active-progress statements in the roadmap should be authoritative there, and which should be sourced from the current delivery board?

**Why this is one independent unit:** This unit is a finite inventory of duplicated active-progress facts in two documents, separate from implementation or planning estimates.

## Inherited observation and starting evidence

The review proposed keeping the roadmap as an entry to the live board and reducing duplicated branch/PR/progress counts. It did not propose reopening completed M3 work or inventing maintenance effort points.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`docs/roadmap.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/roadmap.md) — Git last-change date at baseline: 2026-10-29.
- [`docs/milestone-4.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/milestone-4.md) — Git last-change date at baseline: 2026-10-29.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Confirm the active milestone in the inspected commit. If it is no longer M4, record that fact and examine only the currently linked board, without historical-board expansion.
2. List duplicated active branch/PR identifiers, completion counts, remaining estimates, and gate states. Distinguish roadmap outcome definitions from live execution progress.
3. For each duplicate, name the authoritative location, validation consumer, and whether a link or retained summary is appropriate. Do not recalculate or change another session's progress.
4. Draft at most three equivalent English/Japanese examples and identify repository-consistency expectations affected by a future edit.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-roadmap-progress-authority-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Board/roadmap edits, new delivery rows, estimate changes, completed-milestone reopening, and RFC rewrites.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/roadmap-progress-authority.md` as the English canonical report and `docs/investigations/roadmap-progress-authority.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

A source-of-truth table covers the active-progress duplication without altering status or estimate accounting. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
