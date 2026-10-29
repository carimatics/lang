<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate the safe Rust boundary for native callbacks

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Can safe Rust construct and dispatch a raw `Method` without expressing the callback's caller obligations, and where should those obligations be represented?

**Why this is one independent unit:** One Rust construction-to-dispatch boundary can be assessed independently of C layout and component resource accounting.

## Inherited observation and starting evidence

The 2026-10-30 review reported a `#[forbid(unsafe_code)]` caller passing a null user pointer through safe `Invoker::invoke`. A null-check-only callback observed it; no invalid pointer was dereferenced. `CodeOwner`, which can be implemented for arbitrary types including `()`, was not itself proof of pointer validity.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`crates/runtime/src/component/native.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/runtime/src/component/native.rs) — Git last-change date at baseline: 2026-10-26.
- [`crates/runtime/src/component/execution.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/runtime/src/component/execution.rs) — Git last-change date at baseline: 2026-10-27.
- [`crates/codegen-clif/src/component.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/codegen-clif/src/component.rs) — Git last-change date at baseline: 2026-10-27.
- [`crates/runtime/src/c_abi/context.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/runtime/src/c_abi/context.rs) — Git last-change date at baseline: 2026-10-20.
- [`tests/component_native_registration.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/tests/component_native_registration.rs) — Git last-change date at baseline: 2026-10-26.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Trace `Method` fields, `Invoker::invoke`, its Clif implementation, registration visibility, and owner retention. List caller obligations for user/code pointer lifetime, thread, reentry, and unwind.
2. First establish constructibility with a compile-only temporary downstream module using `#![forbid(unsafe_code)]`. Keep any callback implementation in a separate fixture that only records whether user is null.
3. If runtime confirmation is necessary, use that bounded non-dereferencing callback in a child process with a five-second limit; never invoke a callback that requires a valid pointer with an invalid one.
4. Compare at most two local API directions: explicit unsafe entry with Safety documentation, or private/sealed raw dispatch behind validated registration. Describe the Rust compatibility effect separately from C layout.
5. Specify a compile-fail regression and the existing owner/retirement/reentry behavior that must remain valid.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-native-callback-safety-contract-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Memory-corruption demonstrations, arbitrary external native libraries, implementation changes, and redesigning C v1.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/native-callback-safety-contract.md` as the English canonical report and `docs/investigations/native-callback-safety-contract.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

The report demonstrates or rules out the safe construction path, names the missing obligations, and gives a narrowly scoped API/regression proposal without executing undefined behavior. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
