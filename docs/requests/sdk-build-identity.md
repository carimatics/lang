<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate effective Cargo settings in the SDK build identity

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Does the SDK fingerprint distinguish effective debug assertions and overflow checks, including Cargo config and package-profile overrides?

**Why this is one independent unit:** Two compiler settings and one fingerprint producer form a finite input-identity investigation; no SDK distribution exercise is needed.

## Inherited observation and starting evidence

The 2026-10-30 review reported equal seeds from the actual compiled build script when these settings changed with other inputs fixed. It did not build and mix two complete SDKs or demonstrate ABI corruption. `DEBUG` describes debuginfo, not debug assertions.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`crates/codegen-clif/build.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/codegen-clif/build.rs) — Git last-change date at baseline: 2026-10-20.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. List the inputs hashed near build.rs line 58 and their Cargo sources; distinguish reported environment values from actual rustc settings.
2. Use isolated target directories and a tiny compile/config observation if needed to establish effective values. Compare at most the four combinations of the two booleans with all other inputs fixed; record both effective settings and fingerprint inputs/output.
3. Inspect how Cargo config and package overrides could alter those effective values without changing the profile name. A build-script-only replay must be labeled as such, not a complete producer/consumer result.
4. Compare two bounded policy choices: an explicitly supported build profile, or a versioned fingerprint including all relevant effective inputs. Record the compatibility and reproducibility questions without selecting a new ABI contract.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-sdk-build-identity-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Complete SDK mixing, private credential/config inspection, ABI layout changes, release pipeline redesign, and fixes.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/sdk-build-identity.md` as the English canonical report and `docs/investigations/sdk-build-identity.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

A four-setting matrix, provenance of each effective value, and a narrowly scoped identity-policy proposal distinguish observed collision from untested runtime consequences. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
