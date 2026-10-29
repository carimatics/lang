<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate component preparation and metadata limits

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Which preparation operations consume `preparation_work` and `metadata_bytes`, and do instance/control values govern those operations?

**Why this is one independent unit:** The unit is the preparation budget of one binding. Separating charged preparation from execution prevents a successful zero-cost operation being mislabeled as a defect.

## Inherited observation and starting evidence

The 2026-10-30 review reported that a scalar call succeeded when each control field was individually zero. It identified engine-derived preparation settings, but did not establish that every scalar operation must consume metadata bytes.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — Git last-change date at baseline: 2026-10-27.
- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — Git last-change date at baseline: 2026-10-28.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Trace creation and reuse of the preparation budget for one scalar binding; locate the work and metadata charge sites and the lifetime of their counters.
2. Use existing gateway fixtures to identify a finite operation that actually charges each selected field. If no public call-phase consumer exists, document its phase and stop that experiment.
3. For each chargeable field use one backend, a generous-limit control, and zero at instance/control scope: at most six cases. Compare observed failure phase with the declared contract.
4. Report whether a rejected preparation retains prior usage and releases reservations. Give one minimal corrective direction and compatibility question; do not implement it.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-component-preparation-limits-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Heap accounting, codec limits, JIT caching, redesigning all 17 limit fields, and fixes.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/component-preparation-limits.md` as the English canonical report and `docs/investigations/component-preparation-limits.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

A phase/charge/scope table distinguishes real bypasses, legitimate zero consumption, and unspecified contracts for both fields. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
