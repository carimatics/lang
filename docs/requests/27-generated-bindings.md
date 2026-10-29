<!-- canonical-language: en -->

# 27 — Generated source and C bindings

Created and source inventory verified: 2026-10-30 JST. Request type: owner-authorized defensive security review of an OSS repository. This English document is canonical.

## Purpose and authorized scope

Review only the boundary below in `carimatics/lang`. Determine defects, impact conditions, repair direction and the smallest useful regression check. Implementation/RFC changes, publication and third-party communication are not requested. Use fabricated data and local fixtures; do not touch external systems, production data or credentials. This request is self-contained and requires no prior review results.

Fixed audit commit: `067e1ee200d903eb97663d6157024e63f538782c`. It includes committed M4 work after M3 completion. Do not claim coverage of later commits or another session's uncommitted changes. If the object is unavailable, obtain it from the authorized remote under the normal permission rules or report the missing prerequisite; never silently substitute HEAD.

## Why this is one unit

Untrusted interface names become code, ABI identifiers and layouts.

## Inputs at the audited revision

- `src/component_bindings.rs`
- `src/component_bindings/`
- `crates/runtime/src/c_abi/header.rs`
- `crates/runtime/src/c_abi/support.h`
- `tests/component_generation.rs`
- `tests/component_source.rs`
- `tests/component_source_resource.rs`
- `fuzz/fuzz_targets/component_bindings.rs`

Relevant contracts (canonical English):

- `docs/rfcs/0008-cleanup-allocators-and-c-boundaries.md`
- `docs/rfcs/0027-production-component-interface.md`
- `docs/rfcs/0033-component-v1-compatibility.md`

## Isolated setup

Check the date/time and read applicable AGENTS.md and continuation instructions first. Record the original checkout's branch, remote/upstream and changes. Run the POSIX-shell example below from a repository checkout; on Windows use an equivalent unique directory in the OS temporary area. Keep fixtures and reports outside the source worktree.

```sh
audit_root=$(mktemp -d "${TMPDIR:-/tmp}/lang-review-27.XXXXXX")
git worktree add --detach "$audit_root/source" 067e1ee200d903eb97663d6157024e63f538782c
export CARGO_TARGET_DIR="$audit_root/target"
cd "$audit_root/source"
```

Use only your worktree and target directory; do not edit other checkouts, branches, indexes or personal settings. Do not change managed source files; place additional validation harnesses in `$audit_root/fixtures/`. Any cleanup must affect only your own temporary files and retain an accessible final report. Limit test parallelism to available CPU, memory and disk; isolated paths do not provide unlimited host capacity.

## Investigation

Read the relevant canonical English RFCs first, record expected invariants, then trace code from input to effects. Follow called helpers only as needed; record out-of-scope observations as evidence-backed candidates. Apply the repository language-design-rfc procedure when reviewing design, without changing design or implementation in this request.

1. Check identifier escaping, reserved names, deterministic mangling and collision handling across source and C namespaces.
2. Compare generated signatures, resources, ownership direction and C size/alignment with the canonical checked IDL.
3. Compile small generated clients where a C toolchain exists, and validate bounded output/error behavior; record unavailable platform checks explicitly.

## Focused validation

These are the initial checks in the audit worktree, not proof of security by themselves. Map every investigation item to existing tests and add only small local harnesses for missing evidence. Cap generated input, iterations, memory and subprocess duration; use an external timeout and confirm process termination. Record zero matching tests, platform skips and missing tools/dependencies as unverified.

```sh
cargo test --locked --test component_generation --test component_source --test component_source_resource
```

Respect tool, network and approval rules. If an automated safety check rejects an action, stop that action and record its stated reason and the unverified scope. Do not disguise the purpose to retry; continue only independently permitted reading or analysis.

## Deliverable and completion criteria

1. Write a Japanese report at `$audit_root/report.md`, recording the commit, OS/architecture, toolchain and verification timestamp.
2. For each investigation item, tabulate inspected code/contracts, commands/results and confirmed defect / no issue found in this review / unverified. Include test names/counts, timeouts and skip conditions.
3. For every finding give file/line, expected contract, required attacker authority, reachable path, bounded reproduction result, impact, reasoned severity, repair direction and regression-test conditions. Separate hypotheses, design concerns and explicitly trusted limitations from defects.
4. Cite source and update dates. For code use the audited commit's last-change date, for example `git log -1 --format=%cI -- PATH`. Use primary external sources; if no update date is published, say so and record verification time.
5. Use the established lifecycle for any applicable design-review conclusion and list unresolved questions. Finish only after classifying every item and confirming `git status --short` has no audit source edits. Finishing this review does not mean defects are fixed or the whole repository is secure.
6. Return report/fixture paths, the important conclusion and remaining unverified conditions concisely.
