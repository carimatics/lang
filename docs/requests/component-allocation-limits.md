<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate per-call component allocation limits

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Do `heap_bytes` and `live_allocations` from an instance or call control reach the allocator while preserving the engine's original accounting?

**Why this is one independent unit:** Two allocator-owned counters share one propagation path and can be assessed with one tiny finite guest operation.

## Inherited observation and starting evidence

The 2026-10-30 review reported a `Vec<i64>` create/push/pop call returning 42 on both VM and JIT with either lower-scope limit set to zero. Its 27 allocator callbacks included preparation allocations and are not a guest-allocation count; AOT was not tested.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — Git last-change date at baseline: 2026-10-27.
- [`src/component_embedding/ffi.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/ffi.rs) — Git last-change date at baseline: 2026-10-27.
- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — Git last-change date at baseline: 2026-10-28.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Locate `bounds`, `call`, and allocator construction; begin around baseline gateway lines 1774, 1795, and 1967. Follow only the two selected fields into the concrete allocator.
2. Reuse a public C-table setup from the gateway tests. Build a temporary finite Vec operation, with generous engine limits. Compare engine-only success with one zero field at a time in instance and control.
3. Run at most these five cases per available VM/JIT backend. Record status, result, callback totals by phase where observable, and cleanup. A five-second child timeout is an inconclusive/hanging result, never success.
4. Trace how a nested call would retain the original counter; report that statically. Describe a minimal regression and boundary-equal/one-less follow-up only when the charged amount can be determined.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-component-allocation-limits-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Codec/preparation budgets, unbounded allocation, exhaustive nested-call testing, AOT setup, or allocator implementation changes.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/component-allocation-limits.md` as the English canonical report and `docs/investigations/component-allocation-limits.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

Each field and tested scope has an observed result or explicit limitation, an allocator data-flow explanation, and a regression criterion that preserves shared usage. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
