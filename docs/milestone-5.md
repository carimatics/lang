<!-- canonical-language: en; japanese-translation: ja/milestone-5.md -->

# Milestone 5 delivery board

[日本語版](ja/milestone-5.md)

Activated 2026-10-30 JST from `3b676c5`. This board tracks the entire M5 scope in [the roadmap](roadmap.md#milestone-5-ecosystem-and-compatibility), including both P1 gates. M4's installer and local packages are prerequisites, not registry completion.

**3 done · 1 doing · 36 todo · 108 remaining pt**

**Next: M5-B02 — preview checked source fixes across entry and declared tests.**

B02 executable checkpoint, verified 2026-10-30 JST: `Snapshot::capture_test` retains declared-test identity through rename/format recapture. A real test-only rename preserves its initializer and assignment, executes to 42 before/after, and leaves the package entry returning 99 and disk bytes unchanged. Collisions, undeclared test indices and invalid test overlays are refused without falling back to entry analysis. Two new execution/refusal tests plus 27 editor/source/repository checks pass; workspace all-target/all-feature Clippy passes with warnings denied. Remaining acceptance: selected-package fix collection, aggregate proof/work charging, original-span composition, rechecks across all roots, complete preview fixtures, full baseline and latest-head CI. B02 remains doing at 3 pt after auditing this remaining list; this checkpoint retires no delivery.

B01 checkpoint, 2026-10-30 JST: [RFC 0044](rfcs/0044-checked-source-migration.md) accepts preview-first fixes, checked publication and the explicit historical scalar profile. The retained old `shared`/`weak` source returns 42; manual alpha-renaming also returns 42 in the current VM and generates native output. The migration command and native execution journey remain unimplemented. B01 validation completed: bilingual consistency 7/7, committed as `1ab1201`. Work branch `codex/m5-checked-source-fixes` was created from `main` (`3b676c5`), PR base `main` in `git@github.com:carimatics/lang.git`, then fast-forwarded to prerequisite `8a43f76` from PR #29. Its checkout is `/private/tmp/lang-m5-fixes`; the original worktree remains clean while that PR's CI runs.

A02 checkpoint, verified 2026-10-30 JST: A01/A02 implement RFC 0043. Code `a473cc0` passes the real compatibility CLI, 45 focused tests, fmt, Clippy, 2,522 workspace tests and isolated-target rustdoc. Both-host CI `41064851495` and native-debug CI `41064851345` succeeded for that code. [PR #29](https://github.com/carimatics/lang/pull/29) targets `main`; publication of these documentation records still requires latest-head checks and remote merge. Starting branch is clean `main` tracking `origin/main`, remote `git@github.com:carimatics/lang.git`; work branch `codex/m5-editions-and-migration`. No user changes. M5 remains active with nine gates open.

## Exit gates

| Gate | Required outcome | State |
| --- | --- | --- |
| G1 | Editions, deprecation policy, compatibility matrix | done |
| G2 | Edition migration and machine-applicable fixes | open |
| G3 | Registry threat model accepted before uploads | open |
| G4 | Storage, indexing, download, immutable versions | open |
| G5 | Authenticated publish/yank/owners and audit | open |
| G6 | Signed release manifests and package provenance | open |
| G7 | Portable WebAssembly component plugin target | open |
| G8 | Wasm sandbox and capability conformance | open |
| G9 | Documentation hosting and metadata search (P1) | open |
| G10 | Published LLVM release-backend evaluation (P1) | open |

Gate completion: **1/10; 5/65 historical gate weight**. Delivery points below are remaining work estimates, not elapsed time or the roadmap's gate weights.

## Deliveries

| ID | State | Remaining | Needs | Gates | Observable acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M5-A01 | done | 0 | — | G1 | Accept edition/deprecation policy and domain compatibility matrix | `b654ae7` |
| M5-A02 | done | 0 | M5-A01 | G1 | Expose supported compatibility in CLI; test unknown editions and old consumer boundaries | `a473cc0` |
| M5-B01 | done | 0 | M5-A01 | G2 | Accept migration recipes, applicability and write transaction contract | `1ab1201` |
| M5-B02 | doing | 3 | M5-B01 | G2 | Preview real source fixes with original spans and semantic recheck | — |
| M5-B03 | todo | 3 | M5-B02 | G2 | Apply checked fixes; stale/conflicting/partial-write refusal fixtures | — |
| M5-B05 | todo | 3 | M5-B01 | G2 | Admit historical scalar syntax, prove fresh renames and preview the real old-source fixture | — |
| M5-B04 | todo | 3 | M5-B03,M5-B05 | G2 | Edition migration CLI and both-host before/after execution journey | — |
| M5-C01 | todo | 3 | M5-A01 | G3 | Threat model: identities, namespace, authority, abuse and adversaries | — |
| M5-C02 | todo | 3 | M5-C01 | G3 | Accept protocol, limits, recovery and security acceptance cases before upload | — |
| M5-D01 | todo | 3 | M5-C02 | G4 | Admit bounded untrusted archives and reject traversal/duplicate/bomb inputs | — |
| M5-D02 | todo | 3 | M5-D01 | G4 | Content-addressed storage and immutable version publication transaction | — |
| M5-D03 | todo | 3 | M5-D02 | G4 | Versioned index and verified download with exact identity/checksums | — |
| M5-D04 | todo | 3 | M5-D03 | G4 | Resolve a registry dependency and execute it on both hosts | — |
| M5-D05 | todo | 3 | M5-D04 | G4 | Lock/offline replay, interrupted publication and concurrent immutability tests | — |
| M5-E01 | todo | 3 | M5-C02 | G5 | Authenticated identities, credential handling and scoped authorization | — |
| M5-E02 | todo | 3 | M5-E01,M5-D02 | G5 | Publish and yank/unyank with owner and conflict enforcement | — |
| M5-E03 | todo | 3 | M5-E02 | G5 | Owner add/remove/transfer and last-owner/credential-revocation fixtures | — |
| M5-E04 | todo | 3 | M5-E03 | G5 | Durable audit trail, recovery and unauthorized-operation end-to-end tests | — |
| M5-F01 | todo | 3 | M5-C02 | G6 | Accept package trust roots, provenance, signature and freshness contract | — |
| M5-F02 | todo | 3 | M5-F01,M5-D02 | G6 | Produce signed package release manifests and verifiable provenance | — |
| M5-F03 | todo | 3 | M5-F02,M5-D03 | G6 | Verify trust, artifact binding, expiry and replay before consumption | — |
| M5-F04 | todo | 3 | M5-F03 | G6 | Rotation/revocation/tampering and signed both-host package journey | — |
| M5-G01 | todo | 3 | M5-A01 | G7 | Accept Wasm component lowering, interface mapping and runtime profile | — |
| M5-G02 | todo | 3 | M5-G01 | G7 | Compile a scalar source plugin to a validated component and execute it | — |
| M5-G03 | todo | 3 | M5-G02 | G7 | Typed host imports, result/error and resource adapter round trips | — |
| M5-G04 | todo | 3 | M5-G03 | G7 | Package/CLI portable component build and both-host plugin loading | — |
| M5-G05 | todo | 3 | M5-G04 | G7 | VM/native/Wasm conformance for the accepted plugin profile | — |
| M5-H01 | todo | 3 | M5-G01,M5-C02 | G8 | Accept sandbox capability policy and resource-limit failure contract | — |
| M5-H02 | todo | 3 | M5-G03,M5-H01 | G8 | Deny ambient filesystem/network/environment/process/FFI access | — |
| M5-H03 | todo | 3 | M5-H02 | G8 | Bound memory/fuel/stack/host calls and cancellation/cleanup | — |
| M5-H04 | todo | 3 | M5-H03,M5-G05 | G8 | Malformed component/escape/denial-of-service corpus and both-host conformance | — |
| M5-I01 | todo | 2 | M5-C02 | G9 | Accept documentation hosting/search metadata and untrusted-content policy | — |
| M5-I02 | todo | 3 | M5-I01,M5-D03 | G9 | Publish sanitized version-bound package docs with executable fixture | — |
| M5-I03 | todo | 3 | M5-I02 | G9 | Bounded metadata search, version/yank visibility and hostile query tests | — |
| M5-J01 | todo | 2 | M5-A01 | G10 | Define LLVM evaluation corpus, debug criteria and reproducible environment | — |
| M5-J02 | todo | 3 | M5-J01 | G10 | Build a comparable LLVM release-backend experiment for the corpus | — |
| M5-J03 | todo | 3 | M5-J02 | G10 | Measure compile/runtime/startup/size and inspect actual debugger behavior | — |
| M5-J04 | todo | 2 | M5-J03 | G10 | Publish benchmark artifacts and retain/adopt decision with limitations | — |
| M5-Z01 | todo | 3 | M5-A02,M5-B04,M5-D05,M5-E04,M5-F04,M5-H04,M5-I03,M5-J04 | G1,G2,G3,G4,G5,G6,G7,G8,G9,G10 | Run continuous signed ecosystem/migration/plugin journeys on both hosts | — |
| M5-Z02 | todo | 3 | M5-Z01 | G1,G2,G3,G4,G5,G6,G7,G8,G9,G10 | Audit every exit criterion and publish bilingual capability guide | — |

## Estimate reconciliation

**112 + 3 − 7 = 108 pt**

| Baseline remaining | Net additions/re-estimates | Retired at completion | Remaining |
| ---: | ---: | ---: | ---: |
| 112 pt | +3 pt | −7 pt | 108 pt |

## Delivery notes and roadmap audits

- B01 completed, 2026-10-30 JST: RFC 0044 accepted in `1ab1201` under the experimental delegation, with explicit unverified implementation assumptions. Bilingual repository consistency passes 7/7; the board's established Next marker was restored after its validator caught a renamed marker. Retire B01's last 2 pt; keep the separately audited +3 pt prerequisite. G2 remains open. B02 is the only doing row.

- B01 scope audit, 2026-10-30 JST: the real historical transition needs a bounded scalar reader and a resolved-name equivalence proof beyond current-edition LNT0001 fixes. Expose B05 at 3 pt before implementation (net +3; 40 total deliveries). Its acceptance is an actual historical `shared`/`weak` preview. B04 retains 3 pt for CLI integration, checked application and both-host VM/native execution after B03/B05. B02's 3 pt includes declared-test capture and aggregate proof accounting; B03's 3 pt reuses existing publication guards. No gate is removed or declared complete by this scope audit.

- Activation audit, 2026-10-30 JST: all ten original gates remain required; neither P1 item is deferred. Bottom-up 39 deliveries total 112 pt. Security design precedes upload admission; signed registry trust remains distinct from RFC 0042 toolchain distribution. Wasm must execute source plugins, not only encode a container. The LLVM gate is an evidence-based evaluation, not mandatory adoption.
- Sequence: compatibility policy → executable compatibility fixture → migration/fixes → authenticated signed registry consumption → portable plugin execution/sandbox → hosting/search and backend evaluation → both-host continuous journeys and exit audit. Independent gates remain represented; select at most one implementation row doing and re-audit prerequisites before adding work.
- No new edition or breaking syntax is invented merely to exercise migration. Migration must use accepted recipes and meaningful before/after execution evidence; unsupported transitions must be refused. Future format/protocol decisions need their own accepted RFC before implementation.
- Completion requires the bilingual `milestone-5-capabilities.md` guide, exact CI head evidence, merged PRs and restored/synchronized starting branch. This activation is not completion of an exit gate.

Sources: repository roadmap, architecture and accepted RFCs; verified 2026-10-30 JST. Source files without embedded update dates have no asserted publication date.


A01 completed (2026-10-30 JST): bilingual RFC 0043 accepted under delegation, defining existing-only edition admission, deprecation procedure and per-domain compatibility. Repository consistency 7/7 passes. Retire the last 2 pt with no additions/re-estimates. Roadmap audit retains all ten required gates.

B01 preparation, 2026-10-30 JST (no delivery retired): a two-module standalone source and the same bytes under an explicit edition-2026 manifest both execute to 42. This is package adoption, not cross-edition migration evidence, and cannot by itself close G2. The real stdio LSP returns a versioned `LNT0001` quickfix that changes only `unused` to `_unused`; applying it preserves the initializer, returns 42 and clears the lint. Reuse the existing semantic rename proof for the next fix CLI. The current editor snapshot captures the root entry; declared-test coverage must be handled explicitly in B02. The draft contract remains outside the accepted RFC set until B01 review. No estimates or exit gates change.

A02 completed (2026-10-30 JST): implementation `a473cc0` exposes bounded human/JSON compatibility reports and shares finite edition admission with manifests. Retained direct exits confirm fmt/Clippy and all 2,522 workspace tests (177 suites, no failures). Initial rustdoc used stale dependency metadata (`NativeContext`/`Method` API mismatches); the unchanged source passes `cargo doc --workspace --no-deps` with an isolated target. A detached retry also exposed an expired observation temporary directory; using persistent `/private/tmp` resolved it. Logs and actual exit codes are retained in ignored `target/m5-validation/`; observation timeouts are not counted as passes. CI `41064851495` and native-debug `41064851345` succeeded for the implementation head, including existing old-consumer boundaries. Retire 3 pt, no additions. G1 closes (5/65 gate weight); the other nine gates and all P1 outcomes remain required. Documentation publication must still pass checks for its own latest PR head before merge.

B01 historical migration evidence: [the retained probe](evidence/2026-10-30-m5-legacy-source-probe.json) uses the real RFC 0006 source at `69b1c4d` (2026-10-17), not a synthetic successor edition. Its `shared`/`weak` identifiers execute to 42 with the locked historical compiler but receive four `SRC0024` diagnostics now. A 315-byte artifact produced through that historical library still executes to 42 in the current compiler. This supplies a real historical-profile-to-2026 migration fixture. B01 must define explicit profile admission and expose/re-estimate its capture/rename prerequisite before implementation; the ordinary compiler must retain current keyword refusal. This experiment does not complete G2.

