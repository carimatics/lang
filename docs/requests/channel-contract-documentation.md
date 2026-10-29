<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate contract/history separation in RFC 0025

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Are the current bounded-channel and backpressure rules identifiable without reading every dated checkpoint in RFC 0025?

**Why this is one independent unit:** A single channel contract supplies its own scope, state transitions, and editorial acceptance criteria.

## Inherited observation and starting evidence

The 2026-10-30 review cited RFC 0025's length and mixed history as a maintainability issue; it did not request new channel semantics or OS-thread concurrency.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`docs/rfcs/0025-bounded-channels-and-backpressure.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0025-bounded-channels-and-backpressure.md) — Git last-change date at baseline: 2026-10-27.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Classify English/Japanese sections as current rules, design rationale, dated checkpoint, or mixed; record size and anchors.
2. Extract only the document's current capacity, send/receive, close/cancel, backpressure, ordering, and ownership statements into a source-anchored outline. Do not invent absent guarantees.
3. Propose where dated material belongs in existing history and how current links remain valid. Retain decisions explaining compatibility.
4. Provide at most three paired wording examples and a document-local acceptance checklist; record uncertain semantics as unresolved.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-channel-contract-documentation-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Editing RFCs, implementing channels, threading/Loom, scheduler changes, and whole-project documentation cleanup.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/channel-contract-documentation.md` as the English canonical report and `docs/investigations/channel-contract-documentation.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

A concise current-contract outline and history move map preserve ordering, bounds, ownership, and bilingual equivalence. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
