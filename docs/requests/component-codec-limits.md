<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate component canonical-value codec limits

Reconciled 2026-10-30 JST: see the matching row in the [repository review](../review-2026-10-30.md) for current findings, fixes, validation and explicitly bounded follow-ups. This file preserves the original investigation scope; inherited observations below are not a claim that the issue remains unfixed.

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Does the canonical Session used by a call honor effective `value_elements`, `serialization_bytes`, `depth`, and `decoder_allocation_bytes` limits?

**Why this is one independent unit:** These four limits meet at one Session/codec boundary and can be investigated with a small valid value, without auditing the full decoder.

## Inherited observation and starting evidence

The 2026-10-30 review reported successful scalar calls with each control field at zero and an engine-derived Session near gateway line 1795. Scalar success alone does not prove that a particular codec budget was consumed.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — Git last-change date at baseline: 2026-10-27.
- [`src/component_embedding/ffi.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/ffi.rs) — Git last-change date at baseline: 2026-10-27.
- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — Git last-change date at baseline: 2026-10-28.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Trace Session construction and each selected limit to its charge site. Distinguish input decode, value validation, and result serialization.
2. Select the smallest existing valid string/list/nested-value fixture that consumes each budget; keep data under 1 KiB and nesting at most four.
3. On one backend compare the generous control with each individually zeroed call-control field: at most five dynamic cases. Inspect instance propagation statically. Record unexercised budgets rather than treating scalar success as proof.
4. Explain failure phase, cleanup, and shared usage across encode/decode. Specify a regression with an observable positive charge; propose no wire-format change.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-component-codec-limits-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Malformed-input campaigns, allocation exhaustion, complete backend matrices, heap/preparation budgets, and fixes.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/component-codec-limits.md` as the English canonical report and `docs/investigations/component-codec-limits.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

All four fields have a Session-to-charge path and a supported conclusion; unmeasured paths and unspecified zero semantics are explicit. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
