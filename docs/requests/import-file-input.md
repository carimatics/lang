<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate regular-file handling for an imported module

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Can an import inside the allowed workspace block module loading when the child path names a FIFO?

**Why this is one independent unit:** The unit is one imported child after path resolution; it can use an ordinary root and has its own diagnostic and containment checks.

## Inherited observation and starting evidence

The 2026-10-30 review reported a regular root importing a FIFO child that blocked until a three-second timeout. Canonical workspace containment alone did not establish that the child was a regular file.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`crates/semantics/src/modules.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/semantics/src/modules.rs) — Git last-change date at baseline: 2026-10-29.
- [`src/main.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/main.rs) — Git last-change date at baseline: 2026-10-29.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Locate `load_file` and its import caller, starting near baseline modules line 1018. Record canonicalization, containment, symlink handling, and open order.
2. Reuse import syntax from an existing fixture to create a regular root and regular child. In a separate private case make only the child a FIFO.
3. Run `lang check` for the two cases under a three-second child timeout on Unix. Always terminate/reap and clean up; record the imported path/span in any diagnostic.
4. Describe the smallest handle-based regular-file check consistent with workspace rules and one ordinary-import regression. Discuss TOCTOU statically, without a filesystem-racing harness.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-import-file-input-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Root-source kind checks, package graphs, artifact loading, LSP, and code changes.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/import-file-input.md` as the English canonical report and `docs/investigations/import-file-input.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

Results distinguish import resolution from file opening and include prompt-rejection and valid-import acceptance criteria. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
