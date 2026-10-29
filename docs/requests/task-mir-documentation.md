<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate contract/history separation in RFC 0023

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Can the current task MIR contract in RFC 0023 be read separately from implementation checkpoints without losing compatibility rationale?

**Why this is one independent unit:** One RFC and its translation are a complete editorial assessment; no runtime implementation or other RFC rewrite is needed.

## Inherited observation and starting evidence

The 2026-10-30 review identified RFC 0023 as a long document mixing current rules with historical implementation evidence; it did not establish a semantic contradiction.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`docs/rfcs/0023-task-mir-boundaries.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0023-task-mir-boundaries.md) — Git last-change date at baseline: 2026-10-27.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Classify the English sections and Japanese counterparts as normative, rationale, checkpoint, or mixed. Record byte/word sizes and current anchor names.
2. Extract the present task MIR boundary, ownership/Send/Share, failure, and format-version rules only where this RFC actually defines them; mark external definitions as references rather than expanding the review.
3. Build a paragraph/section move map to existing history documents, preserving decisions and links. Identify a current-contract outline of at most eight sections.
4. Give at most three English/Japanese sample edits and a link/anchor preservation checklist specific to this document.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-task-mir-documentation-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Editing the RFC/history, changing MIR or scheduler behavior, implementing Send/Share, or redesigning documentation globally.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/task-mir-documentation.md` as the English canonical report and `docs/investigations/task-mir-documentation.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

A document-local classification, move map, and equivalent bilingual examples preserve the existing semantics. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
