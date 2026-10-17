<!-- canonical-language: en; japanese-translation: ja/milestone-2.md -->

# Milestone 2 delivery board

[日本語版](ja/milestone-2.md) · [Exit gates](roadmap.md#milestone-2-ownership-and-native-aot)

Updated and verified: **2026-10-18 JST**. Initial audit checkpoint: `0c2c012`.

**43 done · 0 doing · 48 todo · 123 remaining pt**

**Next: M2-V06 → execute nested record/enum owner moves and reference projections with exactly-once drops.**

Latest delivery: **M2-V05** (`752a369`) executes Text clone/concat/to-bytes/equality and Bytes clone. [The Text script](../examples/scripts/text.lang) prints `2` after checking independent copies. VM 25/CLI 25 tests cover exact allocation requests, byte equality including NUL and canonical-equivalent byte differences, shared aliases, OOM/quota input preservation, each script buffer allocation failure, all fuel prefixes and loop cleanup. The full baseline passes **981 tests in 71 suites**, fmt/Clippy/doc, and fuzz-workspace fmt/all-bin locked Clippy.

Measured last code baseline (2026-10-18 JST): workspace Clippy **9.7 s**, full tests **105.7 s**, rustdoc **9.0 s**, fuzz Clippy **0.7 s**. These are command durations, not total working time; the logs do not attribute every part of wall time. Focused VM/CLI tests passed before the full baseline. Run the baseline once per coherent code delivery; documentation-only updates use consistency checks.

Current source capability: a zero-parameter i64 main can allocate/mutate/read/clone Bytes, convert them to valid UTF-8 Text, read/clone/concatenate/compare Text or copy it to Bytes, construct/extract Bytes/Text/Copy sums, borrow-match results and clean normal/failure exits. Borrowed matching is unchanged. V05 includes the already required Bytes clone/equality within its 2-point estimate. General source aggregates are next. G07 remains open for V09 assignment and N03 native evidence; other owning services, REPL execution and ownership artifacts also retain their gates. Unsupported operations/payloads fail before provider callbacks.

The original exit gates remain **5 / 18 complete**. Completed deliveries include compiler/runtime prerequisites and the initial source execution fixture; they do not assert that entire core-library or backend features are complete.

This is the authoritative implementation Todo list. The roadmap retains the original acceptance gates and their historical weights. Counts measure independently verified deliveries, not equal amounts of work. Remaining points are a provisional bottom-up estimate, with low confidence for native/C/platform integration; they are not hours or a completion forecast. Completed work is not assigned retrospective effort points. A done row has zero *remaining* points.

## Why the previous number stopped

Between `367218e` (2026-10-18 00:14:21 JST) and `0c2c012` (03:24:22), the exit-gate score stayed at 34 / 113 while nine implementation commits and nine roadmap commits landed. The recorded passing-test baseline grew from 840 to 930. The net diff is 48 files, 10,407 insertions and 78 deletions. Commit timestamps describe a 3 h 10 min interval, not measured active working time; test counts and lines are evidence of changes, not productivity or delivered features.

The causes were:

1. Broad end-to-end acceptance gates doubled as the daily Todo list. For example, the drop gate hid the cleanup engine, generated ranges, construction order, nested reporting, failure CFG, allocator, layouts, and production execution. Completed prerequisites could not change its score.
2. The 113-point estimate was repeatedly declared appropriate without a bottom-up remaining-work review after RFCs 0007/0008 fixed the requirements. Ownership VM execution, fallible allocation integration and ownership artifact encoding were mostly prose rather than visible tasks. **113 pt is retired as an effort forecast.** Its remaining 79 gate-weight points are not a defensible estimate of remaining implementation.
3. Work proceeded horizontally through compiler/runtime foundations while source execution stayed unavailable. Some prerequisites are necessary, but there was no visible limit on adding more before testing a usable path. Extended planning also delayed the first concrete change.
4. Each slice appended a long historical report and a separate roadmap commit. That recorded activity without exposing a current queue. Full baseline checks were repeated for code slices; their time was not measured, so this audit cannot attribute a specific share of elapsed time to testing.

The initial 134-point remaining-work inventory replaced the missing detailed estimate; it did not rescore the completed 34 points or manufacture a new completion percentage. Current remaining points decrease as deliveries finish and change explicitly when work is re-estimated. The old and new totals have different granularity and must not be used to calculate a historical cost overrun. Re-estimate newly discovered work explicitly, including the change in task count and remaining points.

## Working rules

Audit after the first execution fixture (2026-10-18 JST): RFC 0007's accepted move/match rule makes non-Copy case bindings read loans. A checked attempt to bind the generated Bytes as an owned mutable local reports `SRC0014`: expected Bytes, found &Bytes. This blocks reaching mutating/consuming core operations from allocating Option/Result values. Add S01–S03 (8 pt) before V04: decide an explicit compatible extraction design, then implement its compiler and runtime contracts. Do not silently change accepted borrowed matching. The new VM also explicitly rejects CFG block arguments; add V11 (2 pt) before general guest-call conformance. V03 removes 3 pt and the four new rows add 10 pt: remaining work changes from 129 to **136 pt**, with **53 todo**. S02/S03 remain provisional until S01 selects the design; split/re-estimate them before coding if its form is larger. The demonstrated source execution is preserved as the completed vertical slice.

S01 decision audit (2026-10-18 JST): RFC 0009 selects three finite core calls using existing generic-call syntax and whole-value argument rules, rather than a new consuming match grammar. Completing the 2-point design row changes remaining work **136 → 134 pt** and todo **53 → 52**. Keep S02 at 3 pt for signatures/source/MIR checks and S03 at 3 pt for supported Bytes/Copy transfer and panic cleanup; parser syntax, arbitrary enum extraction, collections and native execution are outside those slices. Their estimates remain provisional until the transfer experiment; expose/split a new prerequisite before coding it. S02 followed by S03 must produce the next source owner-extraction fixture before expanding the foundation.

- Select one row before implementation and mark it `doing`; keep at most one doing row. A row has one observable outcome and 1–3 remaining points. Split it before starting if that outcome is larger or its prerequisites are unknown.
- If planning produces no patch or executable experiment within 10 minutes, narrow the task and run the smallest useful experiment before extending the design. Add abstractions and hardening only when the selected outcome or accepted requirements need them. Record validation duration when available; commit intervals are not work-time measurements.
- The next sequence allows two prerequisite deliveries (finite values and bounded frames), then requires the source Bytes execution fixture. If another prerequisite appears, record it and revise the sequence before more infrastructure work. Do not silently postpone the fixture.
- Done requires a commit for the row's outcome (implementation or an explicitly planned design decision) and focused validation evidence. Add the commit in the next planning update when it cannot be referenced in its own commit. Keep work-in-progress honest until that evidence exists. Update this board after every delivery; do not wait for an exit gate.
- Run focused checks first and the required full baseline once for a coherent code delivery. Documentation-only follow-ups run repository consistency checks; repeat broader checks only for new code, failures, or unresolved concerns. Keep the implementation/roadmap commit sequence, but replace repetitive narrative appends with row updates. Append an audit only when scope, estimates, dependencies or an exit decision changes.
- After two consecutive prerequisite deliveries without a new source execution fixture, stop expanding the foundation and review the next vertical slice. Report the concrete blocker and remaining task, not a generic claim that estimates remain appropriate.
- Keep all 18 exit gates. Close a gate only with its full evidence, including actual tier-1 execution where required. A local object or configured CI job is not native Windows execution evidence. Unfinished P1 debug work must be explicitly moved and re-estimated before exit.

## Completed deliveries

The initial 35 rows reconstruct completed M2 implementation boundaries from commits; the original RFC 0007/0008 decisions remain in the exit gates. Subsequent deliveries, including explicitly planned follow-up design rows, retain their planned IDs when marked done and satisfy only their stated outcome. Evidence details and validation limits are in [architecture](architecture.md) and the [roadmap evidence history](roadmap.md#implementation-evidence-history-1).

`Gate` is the gate's ordinal G01–G18 in the roadmap checklist. A row can contribute to multiple gates without counting its work twice.

| ID | State | Remaining pt | Needs | Gate | Outcome / acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-D01 | done | 0 | — | G13 | Full Copy-profile native lowering and differential fixtures. | `926fb3b` |
| M2-D02 | done | 0 | — | G04 | Shared/weak lifecycle, cycle publication, rollback and races. | `ec16f8a` |
| M2-D03 | done | 0 | — | G03 | Ownership grammar and formatter round trips. | `eb3b16f` |
| M2-D04 | done | 0 | — | G03 | HIR ownership type properties and bounds. | `81fab15` |
| M2-D05 | done | 0 | — | G03 | HIR places and lexical arena scope binding. | `f6e0edb` |
| M2-D06 | done | 0 | — | G03 | Finite core-call signatures and provenance templates. | `9342175` |
| M2-D07 | done | 0 | — | G03 | Core sum cases, Unit values and patterns. | `0115a46` |
| M2-D08 | done | 0 | — | G03 | Contextual body/core typing and exhaustiveness. | `8eda046` |
| M2-D09 | done | 0 | — | G05 | Lexical arena-owner authority typing. | `7213ae3` |
| M2-D10 | done | 0 | — | G03 | HIR initialization and affine move flow. | `9263966` |
| M2-D11 | done | 0 | — | G03 | HIR lexical loans and mutable access. | `7e3d43d` |
| M2-D12 | done | 0 | — | G03 | Nested reference cells and transitive provenance. | `83b5e36` |
| M2-D13 | done | 0 | — | G05 | Source arena brands and storage flow. | `3e6563a` |
| M2-D14 | done | 0 | — | G05 | Recursive guest-call region summaries. | `b39b96a` |
| M2-D15 | done | 0 | — | G05 | Retained arena reads, generations and reverse teardown. | `02f349d` |
| M2-D16 | done | 0 | — | G03 | Independent MIR ownership type-graph validation. | `c6847a6` |
| M2-D17 | done | 0 | — | G03 | Canonical MIR types and source declarations. | `97672e4` |
| M2-D18 | done | 0 | — | G03 | Typed MIR place/projection validation. | `51759b3` |
| M2-D19 | done | 0 | — | G03 | Typed ownership instructions and CFG validation. | `a3c5dce` |
| M2-D20 | done | 0 | — | G03 | Independent MIR initialization and move analysis. | `9cf81bd` |
| M2-D21 | done | 0 | — | G03 | Prepared assignment destinations across RHS effects. | `9a57680` |
| M2-D22 | done | 0 | — | G03 | Explicit MIR loan activation/end analysis. | `fd5f050` |
| M2-D23 | done | 0 | — | G03 | Independent MIR reference-provenance checking. | `4c6c13e` |
| M2-D24 | done | 0 | — | G03 | MIR core-call contracts and allocator requirements. | `4619937` |
| M2-D25 | done | 0 | — | G03 | Automatic ownership source-body lowering. | `39bb20e` |
| M2-D26 | done | 0 | — | G03 | Match and provisional-loan arm lowering. | `ad40b7a` |
| M2-D27 | done | 0 | — | G03 | Driver/query ownership analysis and lang check. | `d25706c` |
| M2-D28 | done | 0 | — | G06 | Ordered cleanup engine and return escrow. | `c437dc1` |
| M2-D29 | done | 0 | — | G06 | Generated source cleanup ranges reach runtime ordering. | `6216b43` |
| M2-D30 | done | 0 | — | G06 | Actual record initialization order survives lowering. | `a984e5e` |
| M2-D31 | done | 0 | — | G06 | Bounded nested field-failure reporting. | `9678072` |
| M2-D32 | done | 0 | — | G06 | Generated ownership failure/return CFG. | `3c3131f` |
| M2-D33 | done | 0 | — | G06 | Fallible instance allocator and accounting. | `d7f85de` |
| M2-D34 | done | 0 | — | G06 | Managed layouts and pre-reserved retirement storage. | `f68a4da` |
| M2-D35 | done | 0 | — | G07 | Allocator-backed Bytes/Text and validated token cleanup. | `82ed11b` |

## Source execution first

| ID | State | Remaining pt | Needs | Gate | Outcome / acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-V01 | done | 0 | M2-D34 | G08 | Managed Unit/Option/Result/error codecs: nested round trips, checked tags and exact bounds. | `71ece57` |
| M2-V02 | done | 0 | M2-V01 | G06 | Bounded owned frame admission reserves value/retirement storage before effects; allocation-failure rollback fixture. | `86c23ce` |
| M2-V03 | done | 0 | M2-V02,M2-D35 | G06,G07 | lang run executes a source Bytes new/len program and normal drop; reject unsupported reachable operations before effects. | `ece8690` |
| M2-S01 | done | 0 | M2-V03 | G07,G08,G09,G10 | RFC 0009 accepts whole Some/Ok/Err extraction with unchanged borrowed matching, explicit panic/cleanup and compatibility rules. Design only. | `c4cb8b9` |
| M2-S02 | done | 0 | M2-S01 | G08 | Three RFC 0009 calls type-check and lower to independently verified MIR; compile pass/fail covers Copy/move, loans, regions and nested sums. Runtime remains gated. | `b562c0f` |
| M2-S03 | done | 0 | M2-S02 | G06,G08 | Finite Bytes/Copy sums and all three whole-payload extractions execute without transfer allocation; source fixture, panic/fuel/OOM cleanup and nested managed order/span transfer verified. | `e584902` |
| M2-V04 | done | 0 | M2-V03,M2-S03 | G07,G08 | Source Bytes get/set/push, UTF-8 conversion and Text byte length execute; bounds/OOM, conversion allocation counts, fuel/interruption cleanup and Copy-cell loop reuse verified. | `8b24c94` |
| M2-V05 | done | 0 | M2-V04 | G07 | Source Text concat/clone/to-bytes/equality and Bytes clone execute with independent owners, allocator identity, typed OOM/quota errors, fuel and loop cleanup. | `752a369` |
| M2-V06 | todo | 3 | M2-V03,M2-S03 | G06,G08 | Execute nested record/enum owner moves and checked reference projections with exactly-once drops. | — |
| M2-V07 | todo | 3 | M2-V06,M2-V11 | G06 | Execute owned guest calls/results with recursion limits and return escrow. | — |
| M2-V08 | todo | 3 | M2-V07,M2-D32 | G06 | Execute generated failure exits: fuel/OOM/body failures, ordered cleanup and nested failure reports. | — |
| M2-V09 | todo | 2 | M2-V04,M2-V06 | G07,G08 | Execute provisional-read match arms and prepared replacement across RHS effects. | — |
| M2-V10 | todo | 2 | M2-V08,M2-V09 | G08 | REPL ownership admission/result display matches run and preserves failure isolation. | — |
| M2-V11 | todo | 2 | M2-V06 | G08,G14,G16 | Execute simultaneous Copy/affine CFG block-argument transfers, including cycles/loops, using bounded pre-reserved scratch storage. | — |

## Shared, arenas and collections

| ID | State | Remaining pt | Needs | Gate | Outcome / acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-R01 | todo | 3 | M2-D24 | G05 | Independent MIR arena-brand/call-effect validation rejects forged escapes with budgeted diagnostics. | — |
| M2-R02 | todo | 3 | M2-D02,M2-D33 | G06 | Shared storage uses guest fallible allocation; preflight/rollback accounts for all payload/retirement storage. | — |
| M2-R03 | todo | 2 | M2-R02,M2-D34 | G06 | Production shared/weak token adapters validate ownership and retire payload descendants without a test registry. | — |
| M2-R04 | todo | 3 | M2-R03,M2-V08 | G06,G14 | Execute source shared/weak operations, upgrades, cycle rejection and rollback. | — |
| M2-R05 | todo | 3 | M2-D15,M2-D33 | G05,G06 | Arena payload allocation and descendant retirement obey guest budgets and failure rollback. | — |
| M2-R06 | todo | 3 | M2-R01,M2-R05,M2-V08 | G05 | Execute source arena allocation/read/reset/teardown with stale-reference and reverse-drop fixtures. | — |
| M2-R07 | todo | 2 | M2-R06,M2-V07 | G05 | Source arena escape/call-brand compile failures and runtime cases agree through guest calls. | — |
| M2-C01 | todo | 3 | M2-D33,M2-D34 | G09 | Vec storage/new/len/push growth is allocator-aware and failure-atomic. | — |
| M2-C02 | todo | 3 | M2-C01 | G09 | Vec read/mutable-read/take/replacement retire moved or replaced elements exactly once. | — |
| M2-C03 | todo | 3 | M2-C02,M2-V09 | G09 | Execute source Vec operations with move/loan/growth/failure fixtures. | — |
| M2-C04 | todo | 3 | M2-D35,M2-D34 | G10 | Map storage and lookup implement exact accepted key equality under allocation budgets. | — |
| M2-C05 | todo | 3 | M2-C04 | G10 | Map insert/replacement is failure-atomic and retires displaced values exactly once. | — |
| M2-C06 | todo | 3 | M2-C05,M2-V09 | G10 | Execute source Map operations with independent equality/replacement/failure oracles. | — |

## Ownership artifacts and C boundaries

| ID | State | Remaining pt | Needs | Gate | Outcome / acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-A01 | todo | 2 | M2-V08 | G14,G16 | Record an accepted ownership artifact version/encoding decision with compatibility and validation limits. | — |
| M2-A02 | todo | 3 | M2-A01 | G14,G16 | Encode/decode ownership types/operations with malformed-input budgets and a fuzz target. | — |
| M2-A03 | todo | 3 | M2-A02 | G16 | Load, independently verify and execute owned source artifacts; round-trip behavior agrees. | — |
| M2-F01 | todo | 2 | M2-D27 | G11 | Enforce accepted unsafe/source boundary rules with source diagnostics and compile-fail fixtures. | — |
| M2-F02 | todo | 3 | M2-F01,M2-V08 | G11 | C-boundary validity/capability/poison preflight and unwind containment have failure fixtures. | — |
| M2-F03 | todo | 3 | M2-F01 | G12 | Generate exact C descriptors/digests/headers and validate layouts against accepted vectors. | — |
| M2-F04 | todo | 3 | M2-F02,M2-F03 | G12 | C imports handle scalar/aggregate arguments, allocation context and failure statuses. | — |
| M2-F05 | todo | 3 | M2-F04 | G12 | C exports return accepted statuses/results and contain nested cleanup failures. | — |
| M2-F06 | todo | 2 | M2-F05 | G11,G12 | Independently compile headers and run boundary-validity fixtures on native macOS. | — |
| M2-F07 | todo | 2 | M2-F06 | G11,G12 | Run matching C-boundary fixtures on native Windows; record successful CI evidence. | — |

## Native, packaging and final evidence

| ID | State | Remaining pt | Needs | Gate | Outcome / acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-N01 | todo | 3 | M2-V07,M2-A02 | G14 | Emit owned function/return machine conventions and managed-value object layouts. | — |
| M2-N02 | todo | 3 | M2-N01 | G14 | Native owner moves, aggregate/sum operations and loan projections match VM fixtures. | — |
| M2-N03 | todo | 3 | M2-N02,M2-C03,M2-C06 | G07,G08,G09,G10,G14 | Native core dispatch executes the same Bytes/Text/Vec/Map fixtures. | — |
| M2-N04 | todo | 3 | M2-N02,M2-R04,M2-R07 | G05,G14 | Native shared/arena dispatch preserves lifecycle and brand checks. | — |
| M2-N05 | todo | 3 | M2-N02,M2-V08 | G06,G14 | Native generated failure/return exits match ordered VM cleanup and fuel behavior. | — |
| M2-N06 | todo | 2 | M2-N03,M2-N04,M2-N05 | G14 | Native macOS owned programs pass direct/response-file object link/run conformance. | — |
| M2-N07 | todo | 2 | M2-N06 | G14 | Native Windows passes matching owned programs; record successful CI evidence. | — |
| M2-P01 | todo | 2 | M2-N06 | G15 | Linker discovery/output-path failures produce actionable diagnostics on both tier-1 platforms. | — |
| M2-P02 | todo | 3 | M2-P01,M2-N07,M2-F05 | G15 | Package startup/runtime/context dependencies; clean-install link/run succeeds. | — |
| M2-Q01 | todo | 3 | M2-A03,M2-N07,M2-F07,M2-V10 | G16 | Representative combined-feature VM/native success/failure conformance uses source fixtures. | — |
| M2-Q02 | todo | 2 | M2-Q01,M2-P02 | G16 | Both tier-1 CI runs pass representative conformance and resource-limit scenarios. | — |
| M2-B01 | todo | 2 | M2-Q01 | G17 | Equivalent Rust/C workloads and harness establish semantic parity before comparison. | — |
| M2-B02 | todo | 2 | M2-B01,M2-P02 | G17 | Record clean macOS runtime/startup/memory/size measurements with variance. | — |
| M2-B03 | todo | 2 | M2-B01,M2-P02 | G17 | Record equivalent clean Windows measurements with variance. | — |
| M2-B04 | todo | 1 | M2-B02,M2-B03 | G17 | Publish interpreted results, reproducibility commands and remaining performance limits. | — |
| M2-G01 | todo | 3 | M2-N06 | G18 | Emit native owned-value source spans/locations and validate macOS debug fixtures. | — |
| M2-G02 | todo | 3 | M2-N07 | G18 | Emit corresponding Windows native debug metadata and validate source locations. | — |
| M2-G03 | todo | 2 | M2-G01,M2-G02 | G18 | Record actual debugger stepping/backtrace evidence on both tier-1 platforms. | — |
| M2-Z01 | todo | 1 | M2-Q02,M2-B04,M2-G03 | G05,G06,G07,G08,G09,G10,G11,G12,G14,G15,G16,G17,G18 | Audit all exit evidence and publish the bilingual M2 capability guide. | — |

## Sources and verification dates

- Local Git history `926fb3b`–`0c2c012` and diff `367218e..0c2c012`: commits dated 2026-10-17/18 JST; verified 2026-10-18 JST.
- [Architecture](architecture.md), [roadmap](roadmap.md), and their implementation/test references: updated/verified 2026-10-18 JST. The 930-test baseline is the pre-audit implementation checkpoint, not a claim about tests added by this planning change.
- Accepted [RFC 0007](rfcs/0007-source-ownership-and-core-values.md) and [RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md): both last updated 2026-10-17; reviewed for this task inventory on 2026-10-18 JST. No web research or external productivity benchmark was used.
