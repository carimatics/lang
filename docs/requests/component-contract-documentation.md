<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate current-contract wording in RFC 0027

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Which RFC 0027 sections state the current component contract, and which are historical checkpoints or reference-artifact descriptions?

**Why this is one independent unit:** The unit is the interpretation and organization of one contract document, with one implementation comment as a consistency check.

## Inherited observation and starting evidence

The review reported a long mixture of normative clauses and dated checkpoints. The phrase “The reference remains a decision artifact, not an implemented library” may describe the reference artifact rather than contradict the implemented API. A Clif comment also described the public C table as future work.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — Git last-change date at baseline: 2026-10-28.
- [`crates/codegen-clif/src/component.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/codegen-clif/src/component.rs) — Git last-change date at baseline: 2026-10-27.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Classify RFC 0027 headings as current normative contract, rationale, dated evidence, or ambiguous. Inspect its Japanese mirror for equivalent meaning.
2. For the reference-library phrase and the comment near component.rs line 160, identify exactly which artifact/API each sentence refers to and its implementation evidence.
3. Draft a compact current-contract outline and a section-to-existing-history destination table. Preserve accepted decisions, stable anchors, compatibility rationale, and dates; count bytes/words as well as lines.
4. Provide at most three paired English/Japanese wording examples. If a proposed edit changes a guarantee, mark it as requiring a design decision rather than silently accepting it.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-component-contract-documentation-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Moving/editing the RFC now, changing component guarantees or statuses, whole-RFC-set review, and board updates.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/component-contract-documentation.md` as the English canonical report and `docs/investigations/component-contract-documentation.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

Every top-level content section has a classification, and the ambiguous reference/API wording has evidence-backed interpretations and a migration outline. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
