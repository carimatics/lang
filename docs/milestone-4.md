<!-- canonical-language: en; japanese-translation: ja/milestone-4.md -->

# Milestone 4 delivery board

[日本語版](ja/milestone-4.md)

Updated: 2026-10-29 JST. M4 is active. Goal start: clean `main`/`origin/main` at `af9988f`, remote `git@github.com:carimatics/lang.git`. Foundation branch `codex/m4-package-toolchain` merged through PR #9. Current coherent package-execution branch: `codex/m4-local-package-execution`, created from clean `main`/`origin/main` at `42e6045`; PR base: `carimatics/lang:main`.

**1 done · 1 doing · 35 todo · 103 remaining pt**

**Next: M4-A02R — resolve a capability-bounded local dependency graph.**

A01's corrected source is `9e5b0ce`, verified locally 2026-10-29 02:24 JST: fmt, warning-free Clippy, checked opt-level 1 full tests (**154 suites / 2,330 passed / 0 failed**) and doc passed with assertions/overflow checks enabled. Manifest/CLI tests passed 7/7; consistency passed 6/6. All prior test identities/results remain unchanged. Latest head `f3d0a52` passed [full CI](https://github.com/carimatics/lang/actions/runs/40736268875) (18 success, expected documentation-only skip) and [native debugger execution](https://github.com/carimatics/lang/actions/runs/40736267899) (all 3 environments). [PR #9](https://github.com/carimatics/lang/pull/9) merged at 02:43 JST as `42e6045`; local `main` was fast-forwarded and verified clean with 0/0 ahead/behind.

A02R executable checkpoint, verified 2026-10-29 03:17 JST: immutable manifest/entry snapshots, sorted exact local edges, shared diamond identity, cycle/name/version/target/resource refusals and 256-package boundary all pass. The 9 graph, 7 manifest/CLI and 6 consistency tests pass; four supervised FIFO path cases finish without blocking. Directory-only canonicalization and capability-relative file admission refuse parent/symlink escapes. Remaining acceptance: full lint/test/doc baseline, review and source commit. A02 must then supply the next real dependency source-execution fixture. G2–G9 remain unfinished.

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
| M4-A02R | doing | 3 | M4-A01 | G2,G3 | Capability-bounded local dependency graph; deterministic order and cycle/escape/version/target/resource refusals | — |
| M4-A02 | todo | 3 | M4-A02R | G2,G3 | Integrate isolated package module roots and immutable source access; execute a real declared dependency import | — |
| M4-A03 | todo | 3 | M4-A02 | G2,G3 | Canonical checksummed lock fixture replays offline and refuses changed sources without rewriting prior output | — |
| M4-A04 | todo | 3 | M4-A03 | G2,G3 | Create a package and local dependency; package build/run/test and failed-test fixture | — |
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
| 103 pt | +3 pt | −3 pt | 103 pt |

**103 + 3 − 3 = 103 pt**

Namespace prerequisite audit, 2026-10-29 JST: the existing `ModuleGraph::root()` is always `ModuleId(0)`. A path provider alone cannot give each dependency its own package-root namespace or safely reuse a shared dependency identity. Split the original A02 outcome into an authorized graph (new A02R, 3 pt) and namespace/source execution integration (A02, remaining 3 pt after reviewing its task list). Net addition: +3 pt; 37 deliveries total. A01 and A02R are two prerequisites, so A02 must supply the next source execution fixture before further horizontal scaffolding. No gate is completed or deferred by this audit.

## Delivery notes and roadmap review

Pre-merge A01 input audit checklist:

- [x] Refuse nonregular manifest inputs without blocking and retain structured `PKG0005`/exit 1.
- [x] Add a supervised real FIFO CLI regression and pass the 7 focused tests plus fmt/Clippy.
- [x] Complete full test/doc and commit the corrected source with updated evidence.
- [x] Verify the updated head's full CI/debugger checks, merge and synchronize the base branch.

- A01 complete: [RFC 0034](rfcs/0034-package-workspaces-and-cli-diagnostics.md) accepts the initial package/tooling profile. `lang package check-manifest` is the executable experiment, including duplicate/unknown/null/type/version/path/target/list/byte-limit and nonregular-file refusals with structured exit 1/2. The four required local baseline commands passed; corrected source evidence is `9e5b0ce` (initial delivery `d1f2667`). Retire A01's last 3 pt separately from the new A02R prerequisite (+3 pt); the pre-merge input correction adds no estimates. G1 alone is complete; source dependency, offline lock, editor, debugger and installer gates remain open.
- Next vertical slice: A02R establishes the authorized graph; A02 executes a real dependency import; A03 adds the checksum/offline failure path; A04 exposes create/build/run/test. Review the sequence after two prerequisites without a new source execution fixture.
- Roadmap audit: all nine original gates, including P1 profiling, remain necessary for the stated new-developer journey. M4 is now activated; the roadmap must link this live board. Signed installer verification precedes install/download. Registry distribution stays M5; it is not silently supplied or removed here.
- Sources: repository roadmap/architecture and RFCs inspected 2026-10-29 JST; source files have no embedded update dates. RFC 0034 updated 2026-10-29. Test checkpoint is finite local evidence, not a both-host or full milestone claim.
