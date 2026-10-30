<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate repeated JIT preparation for one component binding

Reconciled 2026-10-30 JST: see the matching row in the [repository review](../review-2026-10-30.md) for current findings, fixes, validation and explicitly bounded follow-ups. This file preserves the original investigation scope; inherited observations below are not a claim that the issue remains unfixed.

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

For one repeated module/binding call, how often does JIT compilation occur, and what can be reused without skipping per-call validation or accounting?

**Why this is one independent unit:** One binding's compile count and phase costs produce a useful decision without implementing a cache or benchmarking the whole language.

## Inherited observation and starting evidence

The review cited an existing 2026-10-27 workload at source c9ffd19: VM median 94.292 µs and JIT 1,267.583 µs, including preparation. These are historical workload observations, not new measurements or a general backend speed ratio.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — Git last-change date at baseline: 2026-10-27.
- [`tools/m3-bench/run.py`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/tools/m3-bench/run.py) — Git last-change date at baseline: 2026-10-28.
- [`docs/benchmarks.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/benchmarks.md) — Git last-change date at baseline: 2026-10-28.
- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — Git last-change date at baseline: 2026-10-28.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Trace the JIT branch near gateway line 1837 and the existing benchmark workload. Identify preparation, compilation, dynamic checks, and invocation boundaries.
2. Use one finite scalar binding and ten repeated calls. Count compile events using a temporary probe/debugger or explain static call counts; keep runtime observations below 60 seconds per process.
3. If measurement is available, use at most three independent processes; report cold preparation, repeated call latency, compile count, and retained-memory observation separately, with build mode and environment. Do not edit the published benchmark ledger.
4. Propose the smallest immutable verified-plan reuse experiment: cache key, owner/lifetime, invalidation, memory bound, and required per-call grants/provenance/loans/usage checks. Define a future structural criterion of one compilation for N calls, without claiming it now.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-component-jit-preparation-cost-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Cache implementation, LLVM evaluation, whole-suite benchmarking, CI timing thresholds, and removing dynamic validation.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/component-jit-preparation-cost.md` as the English canonical report and `docs/investigations/component-jit-preparation-cost.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

The compile frequency and evidence limits are clear, with a bounded reuse experiment and no unsupported performance promise. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
