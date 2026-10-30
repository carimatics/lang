<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate the query database's disk-input path

Reconciled 2026-10-30 JST: see the matching row in the [repository review](../review-2026-10-30.md) for current findings, fixes, validation and explicitly bounded follow-ups. This file preserves the original investigation scope; inherited observations below are not a claim that the issue remains unfixed.

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Can a disk-backed `database::input` reach a blocking open, and do editor overlays bypass or still reach that operation?

**Why this is one independent unit:** One database input-source decision can be traced using a direct library fixture, without launching an editor or auditing the LSP protocol.

## Inherited observation and starting evidence

The 2026-10-30 review identified similar reading near database line 378 by static inspection only. It did not execute an editor/LSP hang; no such reproduction should be claimed.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`crates/semantics/src/database.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/semantics/src/database.rs) — Git last-change date at baseline: 2026-10-18.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Find the input method and callers using `rg -n 'fn input|File::open|overlay' crates/semantics/src/database.rs crates/lsp`. Follow only disk versus in-memory selection.
2. Document path normalization, cache/overlay precedence, file open, and input-error mapping for this method.
3. If a direct existing database fixture can reach disk input, compare one regular file, one in-memory overlay, and one FIFO in separate child cases with three-second timeouts. Otherwise stop at static evidence and state the missing harness.
4. Identify the narrowest rejection boundary and a regression protecting unsaved overlay contents. Do not infer LSP responsiveness from a CLI result.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-database-disk-input-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Editor automation, full LSP sessions, general cache invalidation, unrelated input loaders, and implementation changes.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/database-disk-input.md` as the English canonical report and `docs/investigations/database-disk-input.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

The disk/overlay decision is explicit and any dynamic result is separated from the original static suspicion. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
