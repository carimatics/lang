<!-- language: en; canonical: true; status: investigation-request; updated: 2026-10-30 JST -->
# Investigation request: Investigate regular-file handling for a root source

Created: 2026-10-30 JST. Deliver only this investigation and its report.

## Question and unit rationale

Does `lang check <root>` reject a non-regular root source promptly, before an open operation can block?

**Why this is one independent unit:** One CLI input and one OS file-kind decision can be investigated without importing modules or loading artifacts.

## Inherited observation and starting evidence

The 2026-10-30 review reported that a regular root succeeded while a root FIFO remained blocked until a three-second child timeout. A byte limit after `File::open` does not bound the open itself.

This is an observation inherited from the quality review dated 2026-10-30 JST, not a result rerun while authoring this request. Its baseline commit is `df55f6ebcf33edb824b7a96e878ab9da5540228d`. Record the actual investigation commit first; if the target changed, inspect only its relevant diff and distinguish reproduced, already resolved, and unresolved behavior. The original temporary report and probes are not prerequisites.

- [`src/main.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/main.rs) — Git last-change date at baseline: 2026-10-29.
- [`crates/semantics/src/modules.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/semantics/src/modules.rs) — Git last-change date at baseline: 2026-10-29.

These are Git file-change dates, distinct from a document's own publication/update date. Record publication/update dates for any additional sources consulted; explicitly say when unavailable.

## Investigation procedure

1. Trace `lang check` to root loading and the first open; identify existing root, symlink, byte-limit, and diagnostic rules.
2. On a supported Unix host create one tiny valid root and one FIFO in a private temp directory. Run only the two CLI cases in child processes, each with a three-second timeout; terminate and reap the child/process group and remove the FIFO in finally cleanup.
3. Record exit status, elapsed time, diagnostic, and whether the ordinary file succeeds. Treat timeout as a reproduced hang, not successful rejection; mark other platforms untested.
4. Assess a nonblocking-open plus opened-handle file-kind check against the actual platform APIs. Explain the path-metadata race and propose one regression criterion.

## Scope and isolation

- Follow the repository AGENTS.md and record the starting branch/upstream/changes. Leave existing checkouts alone; create a dedicated `codex/investigate-root-source-file-input-<unique>` branch and separate worktree from a recorded base.
- Use private temporary locations for probes, logs, TMPDIR, and CARGO_TARGET_DIR. Do not share another session's build outputs, ports, FIFOs, or corpus. Reuse valid existing fixtures and run only the small necessary experiments.
- Within the first ten minutes, identify the target path and smallest useful check. Note out-of-scope observations in one sentence as a limitation without pursuing them. At an experiment limit, missing tooling, or failed reproduction, report facts and unknowns and stop instead of widening scope.
- **Excluded:** Imports, artifacts, editor/LSP inputs, race stress tests, and a shared loader implementation.
- Limit Git-managed changes to the two report files below. Do not change implementation, accepted designs, or shared progress records. Follow repository validation/commit/PR procedures without switching or synchronizing another session's checkout.

## Deliverables and completion

Create `docs/investigations/root-source-file-input.md` as the English canonical report and `docs/investigations/root-source-file-input.ja.md` as an equivalent Japanese version. Each should concisely contain:

- Investigation time/timezone, commit, environment, and the conclusion established now.
- A question-specific evidence table, paths/symbols/line numbers, minimal commands, and expected versus observed results. Separate observation, inference, proposal, and unverified claims.
- A proposal limited to this target or evidence that no change is needed, regression criteria, compatibility questions, and execution limitations.

The root path has a two-case result and a concrete rejection strategy preserving current path constraints. An unreproduced result can still complete the investigation when the attempted scope, blocker, and uncertainty are concrete. Repeating the whole original review or implementing a fix is not a completion requirement. After saving the documents, run `cargo test --locked --test repository_consistency` and `git diff --check` and record the results.
