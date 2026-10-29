<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate a bounded public-component lifecycle test model

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

What is the smallest bounded model for testing register/call/cancel/reenter/close through the public component API?

**Why this is one independent unit:** A model and oracle for one instance/binding can be delivered by static inspection and at most five finite traces; no fuzz infrastructure is required.

## Inherited observation and starting evidence

The review proposed operation-sequence exploration because single-buffer codec targets do not establish lifecycle coverage. This is a test-design hypothesis, not a newly reproduced lifecycle defect.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — Git last-change date at baseline: 2026-10-27.
- [`tests/component_native_registration.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/tests/component_native_registration.rs) — Git last-change date at baseline: 2026-10-26.
- [`fuzz/fuzz_targets/component_bindings.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/fuzz/fuzz_targets/component_bindings.rs) — Git last-change date at baseline: 2026-10-22.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Locate the public C-table lifecycle tests and existing binding target. Extract states and observable statuses; distinguish a logical single-thread scheduler from OS concurrency.
2. Define only one engine, one instance, one binding, and at most two nested calls. Keep typed live-handle tracking so generated traces never dereference stale raw pointers.
3. Specify at most five traces of up to eight operations covering normal close, rejected ordering, cancellation, reentry, and cleanup. For each give expected status, retained ownership, and post-failure reuse oracle.
4. Reuse an existing trace test if practical; otherwise deliver the model as a static proposal with explicit untested assumptions. Identify a single future harness entry and deterministic replay format.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-component-lifecycle-fuzz-model-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Implementing a fuzzer, real native plugins, OS-thread concurrency, arbitrary invalid pointers, or changing lifecycle semantics.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/component-lifecycle-fuzz-model.md` as the English canonical report and `docs/investigations/component-lifecycle-fuzz-model.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

A small state/operation/oracle table and five-or-fewer traces are sufficient to implement one future harness without interpreting a broad audit. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
