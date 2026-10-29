<!-- canonical-language: en -->

# 22 — Component version negotiation and locks

Created and source inventory verified: 2026-10-30 JST. Request type: owner-authorized defensive security review of an OSS repository. This English document is canonical.

## Purpose and authorized scope

Review only the boundary below in `carimatics/lang`. Determine defects, impact conditions, repair direction and the smallest useful regression check. Implementation/RFC changes, publication and third-party communication are not requested. Use fabricated data and local fixtures; do not touch external systems, production data or credentials. This request is self-contained and requires no prior review results.

Fixed audit commit: `067e1ee200d903eb97663d6157024e63f538782c`. It includes committed M4 work after M3 completion. Do not claim coverage of later commits or another session's uncommitted changes. If the object is unavailable, obtain it from the authorized remote under the normal permission rules or report the missing prerequisite; never silently substitute HEAD.

## Why this is one unit

Version and digest selection must fail closed without silently changing the implementation.

## Inputs at the audited revision

- `crates/runtime/src/component/negotiation.rs`
- `crates/runtime/src/component/lockfile.rs`
- `crates/runtime/src/component/instantiation.rs`
- `crates/runtime/src/component/wire/selection.rs`
- `tests/component_selection.rs`

Relevant contracts (canonical English):

- `docs/rfcs/0003-versioned-component-contracts.md`
- `docs/rfcs/0027-production-component-interface.md`
- `docs/rfcs/0033-component-v1-compatibility.md`

## Isolated setup

Check the date/time and read applicable AGENTS.md and continuation instructions first. Record the original checkout's branch, remote/upstream and changes. Run the POSIX-shell example below from a repository checkout; on Windows use an equivalent unique directory in the OS temporary area. Keep fixtures and reports outside the source worktree.

```sh
audit_root=$(mktemp -d "${TMPDIR:-/tmp}/lang-review-22.XXXXXX")
git worktree add --detach "$audit_root/source" 067e1ee200d903eb97663d6157024e63f538782c
export CARGO_TARGET_DIR="$audit_root/target"
cd "$audit_root/source"
```

Use only your worktree and target directory; do not edit other checkouts, branches, indexes or personal settings. Do not change managed source files; place additional validation harnesses in `$audit_root/fixtures/`. Any cleanup must affect only your own temporary files and retain an accessible final report. Limit test parallelism to available CPU, memory and disk; isolated paths do not provide unlimited host capacity.

## Investigation

Read the relevant canonical English RFCs first, record expected invariants, then trace code from input to effects. Follow called helpers only as needed; record out-of-scope observations as evidence-backed candidates. Apply the repository language-design-rfc procedure when reviewing design, without changing design or implementation in this request.

1. Check canonical package/interface names, SemVer prerelease/build handling and exact/compatible range comparisons.
2. Verify digest algorithm/length, same-precedence ambiguity, projection compatibility and lock mismatch all refuse before provider use.
3. Bound candidate sorting and repeated projection comparisons; use small candidate sets with absent, duplicate and conflicting versions.

## Focused validation

These are the initial checks in the audit worktree, not proof of security by themselves. Map every investigation item to existing tests and add only small local harnesses for missing evidence. Cap generated input, iterations, memory and subprocess duration; use an external timeout and confirm process termination. Record zero matching tests, platform skips and missing tools/dependencies as unverified.

```sh
cargo test --locked --test component_selection
cargo test --locked -p lang-runtime --lib component::negotiation
```

Respect tool, network and approval rules. If an automated safety check rejects an action, stop that action and record its stated reason and the unverified scope. Do not disguise the purpose to retry; continue only independently permitted reading or analysis.

## Deliverable and completion criteria

1. Write a Japanese report at `$audit_root/report.md`, recording the commit, OS/architecture, toolchain and verification timestamp.
2. For each investigation item, tabulate inspected code/contracts, commands/results and confirmed defect / no issue found in this review / unverified. Include test names/counts, timeouts and skip conditions.
3. For every finding give file/line, expected contract, required attacker authority, reachable path, bounded reproduction result, impact, reasoned severity, repair direction and regression-test conditions. Separate hypotheses, design concerns and explicitly trusted limitations from defects.
4. Cite source and update dates. For code use the audited commit's last-change date, for example `git log -1 --format=%cI -- PATH`. Use primary external sources; if no update date is published, say so and record verification time.
5. Use the established lifecycle for any applicable design-review conclusion and list unresolved questions. Finish only after classifying every item and confirming `git status --short` has no audit source edits. Finishing this review does not mean defects are fixed or the whole repository is secure.
6. Return report/fixture paths, the important conclusion and remaining unverified conditions concisely.
