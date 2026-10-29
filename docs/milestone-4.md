<!-- canonical-language: en; japanese-translation: ja/milestone-4.md -->

# Milestone 4 delivery board

[日本語版](ja/milestone-4.md)

Updated: 2026-10-30 JST. M4 is active. Goal start: clean main/origin/main `af9988f`, remote `git@github.com:carimatics/lang.git`. PRs #9–#18 are merged. Editor PR #18 final head `5cdc422` passes CI `40864753941` (18 success, intentional documentation-only skip) and debugger `40864753946` (3/3), merged as `0a15fb5`; local main/origin/main refs match with 0/0 ahead/behind. Current `codex/m4-debug-adapter` was created from clean, upstream-synchronized `codex/m4-editor-checked-edits` head `5cdc422` for the coherent D01–D05 outcome. No preexisting user changes; upstream `origin/codex/m4-debug-adapter`, [draft PR #19](https://github.com/carimatics/lang/pull/19) against main. D01 head `8421298` passes CI `40874260252` (18 success, intentional documentation-only skip) and debugger `40874260229` (3/3); D02 head `ced8e56` is pushed; CI `40879596677` (18 success, intentional documentation-only skip) and debugger `40879596614` (3/3) succeed. No merge is claimed. Its initially recorded base `carimatics/lang:codex/m4-editor-checked-edits` is now `carimatics/lang:main` because #18 merged. Current DAP work was preserved during base synchronization.

**26 done · 1 doing · 15 todo · 45 remaining pt**

D04 last executable checkpoint, 2026-10-30 03:45 JST: explicit Rust NativeHosts/mode:mixed/cooperative pause retain the real native callback and original VM on one worker. Exact PC suffix correspondence publishes the physical native prefix then original child→main spans. The independent exact-SDK stdio checker passes continue, disconnect and capture-limit cases locally: five physical frames plus VM lines 4/8, result 42/fuel 7, one callback entry and one owner drop in every case; cancellation/overflow never return a value or publish a partial stack. Twelve debugger tests and final fmt/warnings-denied Clippy/168 suites and 2,471 tests/docs pass. D04B source `67f62fa` is committed/pushed; its three native-debug CI jobs and both-host Rust tests pass. Full CI exposed an outdated separate fuzz lock; Cargo regenerated it for the new backtrace dependency. The new checker is wired into both-host debugger CI. Remaining D04 conditions: latest-head both-host independent frame/failure acceptance. D05 lifecycle remains open.

**Next: M4-D04 — both-host live mixed protocol/frame/failure acceptance (doing, 3 pt).** D04B is done at `67f62fa`; retire its last 2 pt with no additions. Additions +15, retired 73, remaining 45 pt across 42 deliveries. D05 lifecycle audit and latest-head full CI remain required.

A04 source `cf6cf5c` passes fmt, full Clippy/doc and checked opt-level 1 full tests (**158 suites / 2,384 passed / 0 failed**), plus 75 focused tests. Final head `75a2041` passes [full CI](https://github.com/carimatics/lang/actions/runs/40810065213) (all 19 jobs: 18 success; intentional documentation-only skip) and [debugger execution](https://github.com/carimatics/lang/actions/runs/40810065278) (3/3). [PR #13](https://github.com/carimatics/lang/pull/13) merged as `58527c2`; main was fast-forwarded, clean and synchronized with 0/0 ahead/behind. Workspace branch creation and its current PR/upstream are recorded above. No preexisting user changes were present.

A04B source `9cf5f7c` passes [full CI](https://github.com/carimatics/lang/actions/runs/40800668325) (all 19 jobs: 18 success; intentional documentation-only skip) and [debugger execution](https://github.com/carimatics/lang/actions/runs/40800668297) (3/3). Command fixtures pass 12/12 on macOS and 11/11 on Windows. Its independent checked opt-level 1 local full baseline completed **158 suites / 2,376 passed / 0 failed**, with fmt/Clippy/doc also passing. [PR #12](https://github.com/carimatics/lang/pull/12) merged as `2cd06e2`; local main tips match with 0/0 ahead/behind. This closes the earlier command baseline/PR lifecycle notes.

A03 verification is complete: corrected source `117a2ab`, final head `6e11460`, local fmt, warning-free full Clippy, checked opt-level 1 full workspace tests (**157 suites / 2,364 passed / 0 failed**) and doc pass with assertions/overflow checks enabled. Final-head [full CI](https://github.com/carimatics/lang/actions/runs/40788344128) has all 19 expected jobs (18 success; intentional documentation-only skip), and [debugger execution](https://github.com/carimatics/lang/actions/runs/40788344085) passes 3/3. Root/fuzz fmt, all fuzz-bin Clippy and 10,000 uninstrumented metadata fuzz smoke runs pass; no coverage/sanitizer campaign is claimed. [PR #11](https://github.com/carimatics/lang/pull/11) merged as `3f06a70`; local `main` is clean and synchronized, 0/0 ahead/behind. This supersedes the earlier open API verification notes below.

A01's corrected source is `9e5b0ce`, verified locally 2026-10-29 02:24 JST: fmt, warning-free Clippy, checked opt-level 1 full tests (**154 suites / 2,330 passed / 0 failed**) and doc passed with assertions/overflow checks enabled. Manifest/CLI tests passed 7/7; consistency passed 6/6. All prior test identities/results remain unchanged. Latest head `f3d0a52` passed [full CI](https://github.com/carimatics/lang/actions/runs/40736268875) (18 success, expected documentation-only skip) and [native debugger execution](https://github.com/carimatics/lang/actions/runs/40736267899) (all 3 environments). [PR #9](https://github.com/carimatics/lang/pull/9) merged at 02:43 JST as `42e6045`; local `main` was fast-forwarded and verified clean with 0/0 ahead/behind.

A02R/A02S/A02 are implemented and verified at corrected source `b81633e`, 2026-10-29 JST. The real declared dependency returns 42 through portable wire replay/VM and whole-program native JIT. Isolated roots, shared nominal types, original privacy spans, duplicate identities, nonblocking FIFO/escape refusals and frozen inputs after edit/delete/rename pass. Capture closes all directory handles on success or failure; the positive project-rename test now passes on Windows as well as macOS.

Local verification: source 13/13, graph 9/9, manifest/CLI 7/7 and consistency 6/6; fmt, warning-free full Clippy, checked opt-level 1 full workspace tests (**156 suites / 2,352 passed / 0 failed**) and doc passed, with assertions/overflow checks enabled. The separate fuzz workspace passes `cargo check --manifest-path fuzz/Cargo.toml --locked --offline --bins` after Cargo regenerated its dependency lock in `4611c86`.

Final head `c0784fc` passed [full CI](https://github.com/carimatics/lang/actions/runs/40779348154): all 19 expected jobs are present, 18 succeed and the documentation-only job is intentionally skipped for this source change. [Native debugger execution](https://github.com/carimatics/lang/actions/runs/40779348160) succeeds in all 3 environments. [PR #10](https://github.com/carimatics/lang/pull/10) merged at 07:34 JST as `260e67a`; local `main` was fast-forwarded and verified clean, 0/0 ahead/behind. Fuzz fmt and warning-free locked/offline Clippy also pass. G2–G9 remain unfinished.

The A01 input correction refuses FIFO/device/directory inputs with `PKG0005`/exit 1 using nonblocking Unix open and opened-handle metadata. It adds no planning points or completed delivery; its full validation and merge are recorded above.

## Exit gates

The [roadmap](roadmap.md#milestone-4-developer-toolchain) retains the original 63 pt acceptance weights. **6/9 gates verified; 39/63 gate weight (61.9%).** Accepting a design does not complete its implementation gates. Every P0 and P1 remains in scope; no work is deferred.

| Gate | Priority | Outcome | Weight | State |
| --- | --- | --- | ---: | --- |
| G1 | P0 | Accepted manifest/workspace, targets, lock compatibility and CLI diagnostics | 5 | done |
| G2 | P0 | Deterministic dependency resolution, checksums, locks and offline replay | 8 | done |
| G3 | P0 | Package-aware build/run/test and structured diagnostics | 5 | done |
| G4 | P0 | Package/fmt/lint/doc CLI integration | 5 | done |
| G5 | P0 | LSP navigation/completion/semantic tokens | 8 | done |
| G6 | P0 | Safe rename and code actions | 8 | done |
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
| M4-A04 | done | 0 | M4-A04B | G2,G3 | Declared test-entry scopes, shared VM profiles and failed-test/source/JSON diagnostic fixtures | `cf6cf5c` |
| M4-A05W | done | 0 | M4-A04 | G2,G3 | Frozen workspace catalogue/schema-2 integrity and real sibling build/run/test execution | `183d393` |
| M4-A05 | done | 0 | M4-A05W | G2,G3 | Workspace member selection and exact target-condition locks on both supported targets | `fc032f5` |
| M4-A06 | done | 0 | M4-A05 | G2,G3 | Build a native executable twice from a relocated locked package; compare pinned-input digests on both hosts | `d66c764` |
| M4-B01 | done | 0 | M4-A04 | G4 | Deterministic package source listing/archive and escaping/changed-input refusal | `b150762` |
| M4-B02 | done | 0 | M4-A04 | G4 | Package fmt/check preserves CST comments and refuses invalid/stale edits | `5010d45` |
| M4-B03 | done | 0 | M4-B02 | G4 | Accepted lint rules, stable codes and package lint pass/fail/JSON fixture | `99fabc4` |
| M4-B04 | done | 0 | M4-B03 | G4 | Generate package API documentation with resolved links, source locations and failure-safe output | `4f62c78` |
| M4-C01 | done | 0 | M4-A05 | G5,G6 | Accept editor symbol/snapshot/edit contract and navigate a real package dependency | `cd37106` |
| M4-C02 | done | 0 | M4-C01 | G5 | Definition/references across package overlays, edits and watched-file refresh | `569260b` |
| M4-C03 | done | 0 | M4-C02 | G5 | Scope/privacy/type-aware completion with UTF-16 and stale-result tests | 0612c45 |
| M4-C04 | done | 0 | M4-C03 | G5 | Semantic tokens and refresh survive Unicode, malformed source and document lifecycle | be05690 |
| M4-C05 | done | 0 | M4-C02 | G6 | Prepare/rename across files with collision/privacy checks and versioned atomic edits | 3cd27e2 |
| M4-C06 | done | 0 | M4-C05 | G6 | Accepted code actions produce checked fixes and reject stale/ambiguous edits | 503912f |
| M4-C07 | done | 0 | M4-C04,M4-C06 | G5,G6 | End-to-end editor package fixture plus protocol/resource/refusal coverage | 6d16e93 |
| M4-D01 | done | 0 | M4-A06 | G7 | Accept DAP launch/control/stack/source contract and stop at a VM source breakpoint | `8421298` |
| M4-D02 | done | 0 | M4-D01 | G7 | VM stepping, frames, locals and evaluation with pause/authority limits | `ced8e56` |
| M4-D03R | done | 0 | M4-D02 | G7 | Configure the matching Windows LLVM Python runtime and prove native DAP breakpoints | `99a20de` |
| M4-D03 | done | 0 | M4-D03R | G7 | Native debugger launch and source breakpoints on both tier-1 hosts | `39473b1` |
| M4-D04B | done | 0 | M4-D03 | G7 | Explicitly granted host bridge and bounded native physical stop on the original VM | `67f62fa` |
| M4-D04 | doing | 3 | M4-D04B | G7 | Mixed native/VM stack fixture preserves call order, spans and failure cleanup | — |
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
| 103 pt | +15 pt | −73 pt | 45 pt |

**103 + 15 − 73 = 45 pt**

Namespace prerequisite audit, 2026-10-29 JST: the existing `ModuleGraph::root()` is always `ModuleId(0)`. A path provider alone cannot give each dependency its own package-root namespace or safely reuse a shared dependency identity. Split the original A02 outcome into an authorized graph (new A02R, 3 pt) and namespace/source execution integration (A02, remaining 3 pt after reviewing its task list). Net addition: +3 pt; 37 deliveries total. A01 and A02R are two prerequisites, so A02 must supply the next source execution fixture before further horizontal scaffolding. No gate is completed or deferred by this audit.

Snapshot lifecycle audit, 2026-10-29 JST: Windows CI executes the dependency fixture successfully but refuses project rename with live directory handles. The input snapshot must outlive its capture handles. Add A02S (3 pt) for a bounded inventory and handle release, retaining the positive rename test. A02 still needs frozen-provider integration and both-host validation (3 pt); move its doing state to A02S. A03 now consumes that inventory, reducing its remaining estimate from 3 to 2 pt after reviewing canonical serialization, checksum, replay and atomic publication tasks. Net addition +2 pt, cumulative +5 pt; 38 deliveries. The dependency source fixture already executes in both modes, so this correction follows executable evidence. No gate is completed or deferred.

CLI outcome audit, 2026-10-29 JST, before implementation: the accepted transaction profile exposes two executable outcomes. A04B (new, 3 pt) covers scaffold/existing-file compatibility, whole-profile build/run and immutable generation publication with a real dependency returning 42. A04 retains 3 pt after reviewing its remaining test-entry selection, multi-profile result checks and structured failure/span fixtures. Split the original combined CLI outcome into these vertical deliveries rather than hiding publication or shared-engine work inside the original estimate. Net addition +3 pt, cumulative +8 pt; 39 deliveries. Separately, retire A03's last 2 pt after corrected-source both-host CI proves its API outcome. Both CLI rows belong to one coherent command implementation branch; the lock wire API is its reviewed dependency. The independent local API baseline/PR lifecycle continues in its frozen checkout. No gate is completed or deferred.

## Delivery notes and roadmap review

B03 executable checkpoint, 2026-10-29 15:55 JST, current worktree: all 110 focused checks and the source-frozen full baseline (**163 suites / 2,419 passed / 0 failed**), fmt, warning-free full Clippy and doc pass. Six real CLI fixtures cover resolved unread bindings, original UTF-8 spans, existing warnings, dependency/test deduplication, denial, no-main/non-executing analysis and unchanged lock/artifact bytes; the library boundary accepts exactly 65,536 unique diagnostics and refuses one more. Formatter head `5010d45` exposed a Windows-only slash-string test assumption in core-1 job `123305175897`; compare native Path values with the same exact span requirement, including the new lint tests. The corrected 19 path/formatter/lint/consistency checks and Clippy pass locally. Remaining: code evidence and corrected latest-head both-host CI/PR lifecycle. B03 remains doing at 3 pt.

B02 executable checkpoint, 2026-10-29 15:44 JST, current worktree: all 7 formatter fixtures and 29 archive/command/consistency regressions pass. Comment preservation, dependency/test formatting, unchanged old lock, stale manifest/source refusal, read-only files, staging-failure cleanup, retained-root rename and Unix permission preservation execute successfully. Fmt, warning-free full Clippy, source-frozen full tests (**162 suites / 2,412 passed / 0 failed**) and doc pass. The first baseline caught a README snapshot compiled before RFC 0037 was indexed; the corrected complete run passes without weakening validation. Remaining: code evidence and latest-head both-host CI on the coherent tooling branch. B02 remains doing at its reviewed 3 pt.

B01 executable checkpoint, 2026-10-29 15:03 JST, current worktree: archive 5/5, existing command 18/18, manifest 7/7 and consistency 6/6 pass; full Clippy is warning-free. Trusted extraction replays the real sibling run/test with the original lock. Python's independent standard `tarfile` reader confirms ordinary files, fixed uid/gid/mtime/mode/owner metadata and descriptor schema 1. Root/fuzz Cargo locks were generated by Cargo for tar 0.4.46/filetime; fuzz-bin check passes. Full tests (161 suites / 2,405 passed / 0 failed) and doc pass with production source frozen. Remaining: code evidence and both-host latest-head CI/PR lifecycle. B01 remains doing; no extra estimates are added.

A06 Windows correction, 2026-10-29 JST: head `fd11166` fails the real native fixture in [Windows native job](https://github.com/carimatics/lang/actions/runs/40820496903/job/123271717757): relocated locked replay detects changed `build.json` and preserves output. RFC 0020 accepts explicit MSVC `/Brepro` before the correction; compare native package files/direct-link digests before cache replay and report PE timestamps. Timestamp/linker metadata remains a hypothesis until the corrected Windows test passes. Re-run the relevant local checks/full baseline and latest-head CI. A06 remains doing at its reviewed 3 pt; no completed delivery or new estimate is claimed.

A06 executable checkpoint, 2026-10-29 14:15 JST, source `d66c764`: fmt, warning-free full Clippy, full tests (**160 suites / 2,400 passed / 0 failed**), doc and 14 focused correction checks pass (initial focused coverage: 51 checks). Two relocated native builds have identical complete executable digests and return 42. Real native CLI receipts, locked/offline replay, source-free startup after SDK/program/source deletion, stale/SDK/rustc refusals and Unix executable-cache permissions pass. The relocated packages also have identical original inputs; startup paths and Mach-O OSO paths required the RFC 0020 correction. Remaining: actual Windows reproducibility, latest-head CI and branch merge. This checkpoint does not complete A06 or another exit gate.

Workspace scope audit, 2026-10-29 JST, before code: schema 1 has only real package inputs; it cannot pin a workspace catalogue's root/unselected manifests without overloading input roles. The probe above confirms that missing integrity input. Add A05W (3 pt) for explicit selection, frozen catalogue, schema-2 checksum/compatibility and a real sibling execution fixture. After reviewing the remaining existing target-filter integration and both-target replay/refusal fixtures, re-estimate A05 from 3 to 2 pt. Net addition +2 pt, cumulative +10 pt; 40 deliveries. Separately, A04 already retired its final 3 pt. Both executable outcomes remain required; no gate is completed or deferred.

A04 complete: `cf6cf5c` executes frozen declared test roots with `_package`, ordinary visibility/nominal identity and original UTF-8 diagnostics. Manifest order, fail-fast preservation, zero-test libraries, entry reuse, per-entry fuel, ownership/task/isolated-service profiles and a real 64 MiB accumulated-output refusal pass. Full local baseline and both-host CI/debugger checks pass as recorded above. Executable probes corrected the RFC/fixture example to existing addition plus signed literals. Retire the last 3 pt with no additions. Workspace selection and native-package reproducibility still keep G2/G3 open; A05 starts with a scope/integrity audit.

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

## C07 editor protocol audit

Both-host closure, 2026-10-29 JST: source `6d16e93`, final head `5cdc422`, full baseline 166 suites / 2,459 tests, fmt/Clippy/doc and 57 focused checks pass. CI `40864753941` has 18 successful jobs plus the intentional documentation-only skip; debugger `40864753946` passes 3/3. PR #18 merged as `0a15fb5`; local main/origin/main refs are synchronized, 0/0. G5/G6 are done.

- Authority/input lifetime: `tests/package_editor.rs` and package LSP admission fixtures cover frozen inputs, physical metadata/source aliases, original display aliases, outside roots and unadmitted overlays.
- Semantic/protocol boundaries: package LSP fixtures cover privacy/import/scope/field identities, malformed source, original URI/version/UTF-16/CRLF, token ordering/refresh, count/output/mapping-work refusal and no partial successful response.
- Edit safety: complete HIR/type correspondence and the separate valid-types/wrong-binding proof test supplement concrete versions, open-file/transactional client guards, collision/alias/ABI refusal, stale closed inputs and unchanged files. Formatting and unread-binding actions reuse the formatter/linter, retain initializer/assignment operations and refuse ambiguous or forged fixes.
- Interoperability/execution: `tests/support/lsp_client.rs` independently consumes Content-Length frames and UTF-16 edits with deadlines and atomic version checks. The actual stdio package journey applies both actions and multi-file rename, closes/reopens a dependency, recovers watched metadata and runs client-saved source returning 42. Existing standalone protocol/framing fixtures remain enabled.

This finite audit does not claim arbitrary-client atomicity, external-writer exclusion, performance benchmarks or fuzz coverage. The latest-head both-host CI and PR lifecycle above close G5/G6.

Windows runtime prerequisite audit, 2026-10-30 JST: LLVM 20.1.8 lldb-dap.exe exists but exits with 0xc0000135; liblldb.dll imports require python310.dll. Add D03R (1 pt) for the matching x64 Python 3.10 runtime and actual Windows breakpoint fixture. Review D03’s remaining SDK snapshot/source authority/adapter integration and retain 3 pt; move doing to D03R. Net addition +1, 41 deliveries, 103 +12 −66 =49 pt. No gate closure is claimed.

Windows debugger estimate audit, 2026-10-30 JST: runtime/PDB selection proves actual breakpoints/callers/steps/guest exit, but LLVM 20.1.8 adapter shutdown still fails. Prepare the official stable LLVM 23.1.4 (published 2026-10-20) separately with a fixed SHA-256, then verify its actual runtime ABI and clean exit. Review remaining tasks and re-estimate D03R from 1 to 2 pt, net addition +1. Cumulative additions +13, retired 66, 103 +13 −66 =50 pt. Keep the clean-exit assertion and existing LLVM checks.

Mixed-host prerequisite audit, 2026-10-30 JST: review D04’s remaining tasks: explicit host-import grants/public binding API, an original-worker scoped wrapper, bounded physical PC capture with live VM correspondence and the first mixed DAP stop are not implemented. Add D04B (2 pt) before implementation and move doing to it. Retain D04 at 3 pt for both-host real protocol/frame/failure cleanup acceptance. Net addition +2, cumulative additions +15, retired 71, 42 deliveries, 103+15−71=47 pt. Keep D03 native success separate from G7 closure; the gate stays pending.
