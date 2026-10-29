<!-- canonical-language: en -->

# 28 — Native code generation and managed continuations

Created and source inventory verified: 2026-10-30 JST. Request type: owner-authorized defensive security review of an OSS repository. This English document is canonical.

## Purpose and authorized scope

Review only the boundary below in `carimatics/lang`. Determine defects, impact conditions, repair direction and the smallest useful regression check. Implementation/RFC changes, publication and third-party communication are not requested. Use fabricated data and local fixtures; do not touch external systems, production data or credentials. This request is self-contained and requires no prior review results.

Fixed audit commit: `067e1ee200d903eb97663d6157024e63f538782c`. It includes committed M4 work after M3 completion. Do not claim coverage of later commits or another session's uncommitted changes. If the object is unavailable, obtain it from the authorized remote under the normal permission rules or report the missing prerequisite; never silently substitute HEAD.

## Why this is one unit

Verified semantics become machine instructions and callable code addresses.

## Inputs at the audited revision

- `crates/codegen-clif/src/lib.rs`
- `crates/codegen-clif/src/unit.rs`
- `crates/codegen-clif/src/owned.rs`
- `crates/codegen-clif/src/owned/`
- `crates/codegen-clif/src/tasks.rs`
- `crates/codegen-clif/src/tasks/`
- `crates/codegen-clif/src/program.rs`
- `crates/codegen-clif/src/program/`
- `tests/native_compiler.rs`
- `tests/native_task_execution.rs`
- `tests/owned_execution.rs`
- `tests/aot_macos.rs`
- `tests/aot_windows_msvc.rs`
- `tests/aot_windows_x86_64.rs`
- `tests/component_object_execution.rs`
- `tests/native_program.rs`
- `tests/owned_frame.rs`

Relevant contracts (canonical English):

- `docs/rfcs/0002-prototype-control-flow.md`
- `docs/rfcs/0012-native-ownership-continuations.md`
- `docs/rfcs/0019-owned-native-entry-runtime.md`
- `docs/rfcs/0030-native-task-component-records.md`

## Isolated setup

Check the date/time and read applicable AGENTS.md and continuation instructions first. Record the original checkout's branch, remote/upstream and changes. Run the POSIX-shell example below from a repository checkout; on Windows use an equivalent unique directory in the OS temporary area. Keep fixtures and reports outside the source worktree.

```sh
audit_root=$(mktemp -d "${TMPDIR:-/tmp}/lang-review-28.XXXXXX")
git worktree add --detach "$audit_root/source" 067e1ee200d903eb97663d6157024e63f538782c
export CARGO_TARGET_DIR="$audit_root/target"
cd "$audit_root/source"
```

Use only your worktree and target directory; do not edit other checkouts, branches, indexes or personal settings. Do not change managed source files; place additional validation harnesses in `$audit_root/fixtures/`. Any cleanup must affect only your own temporary files and retain an accessible final report. Limit test parallelism to available CPU, memory and disk; isolated paths do not provide unlimited host capacity.

## Investigation

Read the relevant canonical English RFCs first, record expected invariants, then trace code from input to effects. Follow called helpers only as needed; record out-of-scope observations as evidence-backed candidates. Apply the repository language-design-rfc procedure when reviewing design, without changing design or implementation in this request.

1. Check verifier/profile admission precedes JIT/object emission and dispatch, including unsupported operations and unused imports.
2. Audit unsafe call/address and managed-frame invariants for code lifetime, calling convention, bounds and cleanup continuation validity.
3. Compare bounded VM/JIT/AOT fixtures for results, failure, fuel and retirement; explicitly separate trusted unmetered Copy execution from instrumented profiles.

## Focused validation

These are the initial checks in the audit worktree, not proof of security by themselves. Map every investigation item to existing tests and add only small local harnesses for missing evidence. Cap generated input, iterations, memory and subprocess duration; use an external timeout and confirm process termination. Record zero matching tests, platform skips and missing tools/dependencies as unverified.

```sh
cargo test --locked --test native_compiler --test native_task_execution --test owned_execution
```

Respect tool, network and approval rules. If an automated safety check rejects an action, stop that action and record its stated reason and the unverified scope. Do not disguise the purpose to retry; continue only independently permitted reading or analysis.

## Deliverable and completion criteria

1. Write a Japanese report at `$audit_root/report.md`, recording the commit, OS/architecture, toolchain and verification timestamp.
2. For each investigation item, tabulate inspected code/contracts, commands/results and confirmed defect / no issue found in this review / unverified. Include test names/counts, timeouts and skip conditions.
3. For every finding give file/line, expected contract, required attacker authority, reachable path, bounded reproduction result, impact, reasoned severity, repair direction and regression-test conditions. Separate hypotheses, design concerns and explicitly trusted limitations from defects.
4. Cite source and update dates. For code use the audited commit's last-change date, for example `git log -1 --format=%cI -- PATH`. Use primary external sources; if no update date is published, say so and record verification time.
5. Use the established lifecycle for any applicable design-review conclusion and list unresolved questions. Finish only after classifying every item and confirming `git status --short` has no audit source edits. Finishing this review does not mean defects are fixed or the whole repository is secure.
6. Return report/fixture paths, the important conclusion and remaining unverified conditions concisely.
