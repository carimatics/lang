<!-- canonical-language: en; japanese-translation: ja/milestone-4.md -->

# Milestone 4 delivery board

[日本語版](ja/milestone-4.md)

Updated: 2026-10-29 JST. M4 is active. Goal start: clean `main`/`origin/main` at `af9988f`, remote `git@github.com:carimatics/lang.git`. PRs #9–#12 are merged. Command branch `codex/m4-package-commands` was created from clean `codex/m4-package-lock-cli` at `6e11460` and merged through PR #12. Dependent branch `codex/m4-package-tests` was created from command head `9cf5f7c` in the original checkout, whose starting state was clean synchronized `main`/`origin/main` at `3f06a70`. Its recorded PR base changed from `carimatics/lang:codex/m4-package-commands` to `carimatics/lang:main` after PR #12 merged as `2cd06e2`. Local main was fast-forwarded to that remote tip (0/0 ahead/behind), and the test branch also fast-forwarded while preserving its own changes. It has no upstream yet. No preexisting user changes were present.

**6 done · 1 doing · 32 todo · 94 remaining pt**

**Next: M4-A04 — execute declared test entries with shared VM profiles and original diagnostics (doing, 3 pt).** Local executable checkpoint, verified 2026-10-29 JST: command 18/18, source 15/15, legacy CLI 30/30, service-work 6/6 and consistency 6/6 (**75 total**) pass, plus fmt and warning-free full Clippy. Own public nominal types, private UTF-8 spans, frozen analysis after deletion/project move, Copy/ownership/task/isolated-service execution, per-test fuel, manifest-order fail-fast, zero-test libraries and the 64 MiB batch-output refusal execute successfully. Remaining acceptance: full test/doc baseline and latest-head both-host CI. Build outputs are isolated from the completed command checkout. No additional exit gate is complete.

A04B source `9cf5f7c` passes [full CI](https://github.com/carimatics/lang/actions/runs/40800668325) (all 19 jobs: 18 success; intentional documentation-only skip) and [debugger execution](https://github.com/carimatics/lang/actions/runs/40800668297) (3/3). Command fixtures pass 12/12 on macOS and 11/11 on Windows. Its independent checked opt-level 1 local full baseline completed **158 suites / 2,376 passed / 0 failed**, with fmt/Clippy/doc also passing. [PR #12](https://github.com/carimatics/lang/pull/12) merged as `2cd06e2`; local main tips match with 0/0 ahead/behind. This closes the earlier command baseline/PR lifecycle notes.

A03 verification is complete: corrected source `117a2ab`, final head `6e11460`, local fmt, warning-free full Clippy, checked opt-level 1 full workspace tests (**157 suites / 2,364 passed / 0 failed**) and doc pass with assertions/overflow checks enabled. Final-head [full CI](https://github.com/carimatics/lang/actions/runs/40788344128) has all 19 expected jobs (18 success; intentional documentation-only skip), and [debugger execution](https://github.com/carimatics/lang/actions/runs/40788344085) passes 3/3. Root/fuzz fmt, all fuzz-bin Clippy and 10,000 uninstrumented metadata fuzz smoke runs pass; no coverage/sanitizer campaign is claimed. [PR #11](https://github.com/carimatics/lang/pull/11) merged as `3f06a70`; local `main` is clean and synchronized, 0/0 ahead/behind. This supersedes the earlier open API verification notes below.

A01's corrected source is `9e5b0ce`, verified locally 2026-10-29 02:24 JST: fmt, warning-free Clippy, checked opt-level 1 full tests (**154 suites / 2,330 passed / 0 failed**) and doc passed with assertions/overflow checks enabled. Manifest/CLI tests passed 7/7; consistency passed 6/6. All prior test identities/results remain unchanged. Latest head `f3d0a52` passed [full CI](https://github.com/carimatics/lang/actions/runs/40736268875) (18 success, expected documentation-only skip) and [native debugger execution](https://github.com/carimatics/lang/actions/runs/40736267899) (all 3 environments). [PR #9](https://github.com/carimatics/lang/pull/9) merged at 02:43 JST as `42e6045`; local `main` was fast-forwarded and verified clean with 0/0 ahead/behind.

A02R/A02S/A02 are implemented and verified at corrected source `b81633e`, 2026-10-29 JST. The real declared dependency returns 42 through portable wire replay/VM and whole-program native JIT. Isolated roots, shared nominal types, original privacy spans, duplicate identities, nonblocking FIFO/escape refusals and frozen inputs after edit/delete/rename pass. Capture closes all directory handles on success or failure; the positive project-rename test now passes on Windows as well as macOS.

Local verification: source 13/13, graph 9/9, manifest/CLI 7/7 and consistency 6/6; fmt, warning-free full Clippy, checked opt-level 1 full workspace tests (**156 suites / 2,352 passed / 0 failed**) and doc passed, with assertions/overflow checks enabled. The separate fuzz workspace passes `cargo check --manifest-path fuzz/Cargo.toml --locked --offline --bins` after Cargo regenerated its dependency lock in `4611c86`.

Final head `c0784fc` passed [full CI](https://github.com/carimatics/lang/actions/runs/40779348154): all 19 expected jobs are present, 18 succeed and the documentation-only job is intentionally skipped for this source change. [Native debugger execution](https://github.com/carimatics/lang/actions/runs/40779348160) succeeds in all 3 environments. [PR #10](https://github.com/carimatics/lang/pull/10) merged at 07:34 JST as `260e67a`; local `main` was fast-forwarded and verified clean, 0/0 ahead/behind. Fuzz fmt and warning-free locked/offline Clippy also pass. G2–G9 remain unfinished.

The A01 input correction refuses FIFO/device/directory inputs with `PKG0005`/exit 1 using nonblocking Unix open and opened-handle metadata. It adds no planning points or completed delivery; its full validation and merge are recorded above.

## Exit gates

The [roadmap](roadmap.md#milestone-4-developer-toolchain) retains the original 63 pt acceptance weights. **1/9 gates verified; 5/63 gate weight (7.9%).** Accepting a design does not complete its implementation gates. Every P0 and P1 remains in scope; no work is deferred.

| Gate | Priority | Outcome | Weight | State |
| --- | --- | --- | ---: | --- |
| G1 | P0 | Accepted manifest/workspace, targets, lock compatibility and CLI diagnostics | 5 | done |
| G2 | P0 | Deterministic dependency resolution, checksums, locks and offline replay | 8 | pending |
| G3 | P0 | Package-aware build/run/test and structured diagnostics | 5 | pending |
| G4 | P0 | Package/fmt/lint/doc CLI integration | 5 | pending |
| G5 | P0 | LSP navigation/completion/semantic tokens | 8 | pending |
| G6 | P0 | Safe rename and code actions | 8 | pending |
| G7 | P0 | DAP adapter with mixed native/VM stacks | 8 | pending |
| G8 | P1 | Compile/runtime profiles, allocation traces and rebuild explanations | 8 | pending |
| G9 | P0 | Signed install/update/channel tool with recovery | 8 | pending |

## Deliveries

`pt` is remaining planning effort, not elapsed time. Each 1–3-point row has an observable outcome. Only one implementation row may be doing. Evidence cells contain only the code commit hash after completion. Dependencies order work but do not substitute for the gate's exit audit.

| ID | State | pt | Needs | Gates | Observable acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M4-A01 | done | 0 | — | G1 | Accept RFC 0034; inspect valid/invalid bounded manifests with real JSON CLI diagnostics | `9e5b0ce` |
| M4-A02R | done | 0 | M4-A01 | G2,G3 | Capability-bounded local dependency graph; deterministic order and cycle/escape/version/target/resource refusals | `aae5242` |
| M4-A02S | done | 0 | M4-A02R | G2,G3 | Freeze bounded source inputs once, release directory handles and preserve execution after edits/renames on both hosts | `b81633e` |
| M4-A02 | done | 0 | M4-A02S | G2,G3 | Integrate isolated package module roots and immutable source access; execute a real declared dependency import | `b81633e` |
| M4-A03 | done | 0 | M4-A02 | G2,G3 | Canonical checksummed lock fixture replays offline and refuses changed sources without rewriting prior output | `117a2ab` |
| M4-A04B | done | 0 | M4-A03 | G2,G3 | Create a package/local dependency; real package build/run, locked/offline replay and failure-safe generations | `9cf5f7c` |
| M4-A04 | doing | 3 | M4-A04B | G2,G3 | Declared test-entry scopes, shared VM profiles and failed-test/source/JSON diagnostic fixtures | — |
| M4-A05 | todo | 3 | M4-A04 | G2,G3 | Workspace member selection and exact target-condition locks on both supported targets | — |
| M4-A06 | todo | 3 | M4-A05 | G2,G3 | Build a native executable twice from a relocated locked package; compare pinned-input digests on both hosts | — |
| M4-B01 | todo | 3 | M4-A04 | G4 | Deterministic package source listing/archive and escaping/changed-input refusal | — |
| M4-B02 | todo | 2 | M4-A04 | G4 | Package fmt/check preserves CST comments and refuses invalid/stale edits | — |
| M4-B03 | todo | 3 | M4-B02 | G4 | Accepted lint rules, stable codes and package lint pass/fail/JSON fixture | — |
| M4-B04 | todo | 3 | M4-B03 | G4 | Generate package API documentation with resolved links, source locations and failure-safe output | — |
| M4-C01 | todo | 3 | M4-A05 | G5,G6 | Accept editor symbol/snapshot/edit contract and navigate a real package dependency | — |
| M4-C02 | todo | 3 | M4-C01 | G5 | Definition/references across package overlays, edits and watched-file refresh | — |
| M4-C03 | todo | 3 | M4-C02 | G5 | Scope/privacy/type-aware completion with UTF-16 and stale-result tests | — |
| M4-C04 | todo | 3 | M4-C03 | G5 | Semantic tokens and refresh survive Unicode, malformed source and document lifecycle | — |
| M4-C05 | todo | 3 | M4-C02 | G6 | Prepare/rename across files with collision/privacy checks and versioned atomic edits | — |
| M4-C06 | todo | 3 | M4-C05 | G6 | Accepted code actions produce checked fixes and reject stale/ambiguous edits | — |
| M4-C07 | todo | 2 | M4-C04,M4-C06 | G5,G6 | End-to-end editor package fixture plus protocol/resource/refusal coverage | — |
| M4-D01 | todo | 3 | M4-A06 | G7 | Accept DAP launch/control/stack/source contract and stop at a VM source breakpoint | — |
| M4-D02 | todo | 3 | M4-D01 | G7 | VM stepping, frames, locals and evaluation with pause/authority limits | — |
| M4-D03 | todo | 3 | M4-D02 | G7 | Native debugger launch and source breakpoints on both tier-1 hosts | — |
| M4-D04 | todo | 3 | M4-D03 | G7 | Mixed native/VM stack fixture preserves call order, spans and failure cleanup | — |
| M4-D05 | todo | 3 | M4-D04 | G7 | DAP disconnect/restart/cancel/malformed-input and both-host session evidence | — |
| M4-E01 | todo | 3 | M4-A06 | G8 | Accept profile/trace schema and report bounded compiler phase costs | — |
| M4-E02 | todo | 3 | M4-E01 | G8 | Source-attributed VM/native runtime profile with explicit sampling/measurement limits | — |
| M4-E03 | todo | 3 | M4-E02 | G8 | Allocation/retirement traces reconcile real allocator counters and bounded buffers | — |
| M4-E04 | todo | 3 | M4-E03 | G8 | Rebuild explanations identify changed source/dependency/target/toolchain inputs | — |
| M4-F01 | todo | 3 | M4-A01 | G9 | Accept installer trust roots, signed metadata, channel/version selection and atomic recovery design | — |
| M4-F02 | todo | 3 | M4-F01 | G9 | Verify signatures, expiry/rollback/target/digests before any install; tamper refusals | — |
| M4-F03 | todo | 3 | M4-F02 | G9 | Install verified tools in an isolated root; failed/interrupted installs preserve active version | — |
| M4-F04 | todo | 3 | M4-F03 | G9 | Channel update/select/list/uninstall with atomic activation and recovery | — |
| M4-F05 | todo | 3 | M4-F04 | G9 | Signed release/distribution fixture installs and updates on both hosts | — |
| M4-Z01 | todo | 3 | M4-B01,M4-B04,M4-C07,M4-D05,M4-E04,M4-F05 | G1,G2,G3,G4,G5,G6,G7,G8,G9 | New-developer install/create/editor/test/debug/reproducible-binary journey on macOS | — |
| M4-Z02 | todo | 3 | M4-Z01 | G1,G2,G3,G4,G5,G6,G7,G8,G9 | Same complete journey and full baseline on Windows; diagnose any divergence | — |
| M4-Z03 | todo | 2 | M4-Z02 | G1,G2,G3,G4,G5,G6,G7,G8,G9 | Audit every roadmap exit requirement against latest code, both-host evidence and limitations | — |
| M4-Z04 | todo | 2 | M4-Z03 | G1,G2,G3,G4,G5,G6,G7,G8,G9 | Publish bilingual M4 capability guide with runnable commands and final reconciliation | — |
| M4-Z05 | todo | 2 | M4-Z04 | G1,G2,G3,G4,G5,G6,G7,G8,G9 | Latest-head CI, PR merge, start-branch synchronization and clean final completion audit | — |

## Estimate reconciliation

Activation audit, 2026-10-29 JST: 36 observable deliveries, baseline 103 pt. These are bottom-up provisional estimates; the inherited 63 gate-weight points remain unchanged. Re-audit unimplemented rows when their actual acceptance task list becomes clear. Expose prerequisites and scope growth before implementation; do not preserve an estimate mechanically. Completed M2/M3 history remains closed.

| Baseline remaining | Net added/re-estimated | Retired on completion | Remaining |
| ---: | ---: | ---: | ---: |
| 103 pt | +8 pt | −17 pt | 94 pt |

**103 + 8 − 17 = 94 pt**

Namespace prerequisite audit, 2026-10-29 JST: the existing `ModuleGraph::root()` is always `ModuleId(0)`. A path provider alone cannot give each dependency its own package-root namespace or safely reuse a shared dependency identity. Split the original A02 outcome into an authorized graph (new A02R, 3 pt) and namespace/source execution integration (A02, remaining 3 pt after reviewing its task list). Net addition: +3 pt; 37 deliveries total. A01 and A02R are two prerequisites, so A02 must supply the next source execution fixture before further horizontal scaffolding. No gate is completed or deferred by this audit.

Snapshot lifecycle audit, 2026-10-29 JST: Windows CI executes the dependency fixture successfully but refuses project rename with live directory handles. The input snapshot must outlive its capture handles. Add A02S (3 pt) for a bounded inventory and handle release, retaining the positive rename test. A02 still needs frozen-provider integration and both-host validation (3 pt); move its doing state to A02S. A03 now consumes that inventory, reducing its remaining estimate from 3 to 2 pt after reviewing canonical serialization, checksum, replay and atomic publication tasks. Net addition +2 pt, cumulative +5 pt; 38 deliveries. The dependency source fixture already executes in both modes, so this correction follows executable evidence. No gate is completed or deferred.

CLI outcome audit, 2026-10-29 JST, before implementation: the accepted transaction profile exposes two executable outcomes. A04B (new, 3 pt) covers scaffold/existing-file compatibility, whole-profile build/run and immutable generation publication with a real dependency returning 42. A04 retains 3 pt after reviewing its remaining test-entry selection, multi-profile result checks and structured failure/span fixtures. Split the original combined CLI outcome into these vertical deliveries rather than hiding publication or shared-engine work inside the original estimate. Net addition +3 pt, cumulative +8 pt; 39 deliveries. Separately, retire A03's last 2 pt after corrected-source both-host CI proves its API outcome. Both CLI rows belong to one coherent command implementation branch; the lock wire API is its reviewed dependency. The independent local API baseline/PR lifecycle continues in its frozen checkout. No gate is completed or deferred.

## Delivery notes and roadmap review

Checkout audit, 2026-10-29 JST: the earlier plan kept A04B/A04 in one branch. Preserve the native baseline's source fingerprint by separating the dependent test-entry namespace/execution outcome into the existing original checkout while the command checkout remains frozen. This is a dependency between two observable outcomes, not another delivery or prerequisite; estimates stay unchanged. CPU builds/tests remain sequential. A04B's final-head both-host CI now proves the command outcome; A04 is the only doing implementation. The independent command baseline/PR lifecycle remains open until its actual completion.

A04B complete: `9cf5f7c` verifies real package creation/dependency build/run and locked/offline replay, all three VM profiles, exact generation reuse, stale/tampered/resource/runtime refusals, original UTF-8 diagnostics and legacy explicit-file compatibility. Both-host CI and three debugger environments pass. Windows verifies final-lock failure retains old output and removes stages; macOS verifies publication through the retained directory after the diagnostic path is replaced. Retire its last 3 pt with no additions. After reviewing namespace selection, result/control handling and output/failure fixtures, A04 remains 3 pt. Its implementation uses the accepted `_package` edge and existing engines. Exit gates G2/G3 remain pending workspace/native-package evidence.

Pre-merge A01 input audit checklist:

- [x] Refuse nonregular manifest inputs without blocking and retain structured `PKG0005`/exit 1.
- [x] Add a supervised real FIFO CLI regression and pass the 7 focused tests plus fmt/Clippy.
- [x] Complete full test/doc and commit the corrected source with updated evidence.
- [x] Verify the updated head's full CI/debugger checks, merge and synchronize the base branch.

- A01 complete: [RFC 0034](rfcs/0034-package-workspaces-and-cli-diagnostics.md) accepts the initial package/tooling profile. `lang package check-manifest` is the executable experiment, including duplicate/unknown/null/type/version/path/target/list/byte-limit and nonregular-file refusals with structured exit 1/2. The four required local baseline commands passed; corrected source evidence is `9e5b0ce` (initial delivery `d1f2667`). Retire A01's last 3 pt separately from the new A02R prerequisite (+3 pt); the pre-merge input correction adds no estimates. G1 alone is complete; source dependency, offline lock, editor, debugger and installer gates remain open.
- A02R complete: immutable original manifest/entry bytes, sorted exact local edges, shared diamond identity, cycle/name/version/target/resource refusals and the 256-package boundary pass. Four supervised FIFO path cases finish without blocking; directory-only canonicalization and capability-relative file admission refuse parent/symlink escapes. Retire its last 3 pt; no new work is added. The next implementation is A02's dependency source execution, followed by A03 checksum/offline replay and A04 create/build/run/test; no additional horizontal prerequisite is selected.
- A02S/A02 complete: `b81633e` freezes the bounded source inventory, caches capture errors, releases handles and integrates the immutable provider with isolated dependency namespaces. Both-host execution and project rename succeed. Retire each row's last 3 pt (6 pt total), separately from prior additions; add no estimates. Windows exposed the unsupported identity import and retained-handle defect; both were corrected without weakening the tests. Cargo also synchronized the separate fuzz lock. An early full run overlapped source edits and failed the SDK build-ID check; the final stable-source baseline above passes all native-image tests and supersedes that invalid checkpoint.
- A03 contract review: source hashes alone miss a directory alias retargeted between two already inventoried trees. RFC 0034 now includes sorted directory/entry/test bindings in canonical lock records and package checksums, with a changed-binding refusal fixture. No lock implementation or existing lock artifact precedes this accepted correction; A03 remains 2 pt after reviewing serialization, replay and publication tasks.
- A03 API complete: `117a2ab` passes both-host full CI and all three debugger environments. The canonical fixture and independent SHA-256/UTF-8 ordering, frozen relocated dependency replay, changed/unused/binding/compiler/target/malformed/resource refusals, physical-identity admission and failure-safe lock publication are verified. Windows exposed checkout CRLF in the fixture; explicit LF attributes fix the fixture while CRLF lock rejection stays strict. Retire the last 2 pt, separately from A04B's +3 pt scope audit. The independent local full test/doc and final documentation head's CI remain branch merge conditions; no local completion or PR merge is claimed yet.
- A04B compatibility checkpoint, 2026-10-29 JST: Windows CI and the first local baseline exposed two obsolete assertions: source-less `run` expected usage exit 2, and help expected three service-work forms. RFC 0034 intentionally selects the default package manifest and adds a fourth help form. Updated checks retain all malformed explicit-file exit-2 cases, assert missing-manifest exit 1, execute default-manifest run returning 42, and refuse ancestor lookup. CLI/service-work/package-command/consistency pass 53/53. The failed baseline is retained as diagnostic evidence; corrected full baseline and latest-head CI remain required. This correction adds no delivery or estimates.
- Roadmap audit: all nine original gates, including P1 profiling, remain necessary for the stated new-developer journey. M4 is now activated; the roadmap must link this live board. Signed installer verification precedes install/download. Registry distribution stays M5; it is not silently supplied or removed here.
- Sources: repository roadmap/architecture and RFCs inspected 2026-10-29 JST; source files have no embedded update dates. RFC 0034 updated 2026-10-29. Test checkpoint is finite local evidence, not a both-host or full milestone claim.
