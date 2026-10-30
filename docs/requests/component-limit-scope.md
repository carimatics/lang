<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate the declared scope of component C v1 limits

Reconciled 2026-10-30 JST: see the matching row in the [repository review](../review-2026-10-30.md) for current findings, fixes, validation and explicitly bounded follow-ups. This file preserves the original investigation scope; inherited observations below are not a claim that the issue remains unfixed.

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

For each of the 17 fields of `ffi::Limits`, which engine/instance/control scopes does the accepted contract support, and which scope supplies the value actually consumed?

**Why this is one independent unit:** One static contract-to-consumer table is a bounded deliverable. It needs no runtime reproduction or changes to the ABI.

## Inherited observation and starting evidence

The 2026-10-30 review reported that `bounds` takes fieldwise minima and that `call` calculates effective limits, while some consumers still receive engine settings. Zero fuel failed in its control experiment. This does not establish that every field is defective.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`src/component_embedding/ffi.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/ffi.rs) — Git last-change date at baseline: 2026-10-27.
- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — Git last-change date at baseline: 2026-10-27.
- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — Git last-change date at baseline: 2026-10-28.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Locate `Limits`, `bounds`, instance/control creation, and `call` with `rg -n 'struct Limits|fn bounds|preparation_work|trace_frames' src/component_embedding/{ffi,gateway}.rs`. Read only the relevant limit clauses in the RFC.
2. Create exactly 17 rows: fuel, service_work, host_calls, heap_bytes, live_allocations, frames, live_cells, live_tasks, canonical_indices, preparation_work, metadata_bytes, value_elements, serialization_bytes, depth, decoder_allocation_bytes, suppressed_records, trace_frames.
3. For each row record declared scope, setter, merge point, consuming symbol, zero semantics, shared-counter owner, and evidence location. Mark unspecified scope as an open specification question; do not invent it.
4. Classify each row as consistent, apparent mismatch, or unresolved. Propose at most one targeted confirmation per unresolved row, without executing a backend matrix.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-component-limit-scope-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Dynamic reproduction, new resource policy, ABI layout changes, fixes, and milestone edits.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/component-limit-scope.md` as the English canonical report and `docs/investigations/component-limit-scope.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

All 17 fields have evidence or an explicit missing-evidence explanation; specification ambiguities are separate from implementation mismatches. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
