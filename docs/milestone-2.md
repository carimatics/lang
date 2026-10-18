<!-- canonical-language: en; japanese-translation: ja/milestone-2.md -->

# Milestone 2 delivery board

[日本語版](ja/milestone-2.md) · [Exit gates](roadmap.md#milestone-2-ownership-and-native-aot)

Updated and verified: **2026-10-18 JST**. Initial audit checkpoint: `0c2c012`.

**60 done · 0 doing · 40 todo · 102 remaining pt**

**Next: M2-R02 → connect shared storage to guest fallible allocation and verify preflight/rollback accounting.**

Latest delivery: **M2-R07** (`cecd6e8`) verifies source arena brands, caller stores and forwarded read loans through guest calls. Runtime cases return **9/70/7/8/12**; nonreturning recursion reaches the configured frame limit and releases all guest storage. Compile-fail cases reject transient/nonreturning escapes, wrong-arena field reuse, collection/result/shared/weak escapes and mutation during a live read, with current-snapshot diagnostic labels. Eleven new tests and the existing 14 MIR call tests pass without a production fix. The full baseline passes **1132 tests in 76 suites**, with fmt/Clippy/doc passing. Completing R07 leaves **104 → 102 remaining pt**, 60 done rows and 100 total. R06's source arena scripts remain executable; native and artifact gates remain open.

Measured last code baseline (2026-10-18 JST): workspace Clippy **2.2 s**, full tests **36.9 s** (log creation to final write), rustdoc **9.2 s**, all passing. The full baseline ran once after the 25 focused source/MIR call checks. These are validation durations, not total working time or runtime performance measurements; the logs do not attribute every part of wall time. Documentation-only updates use consistency checks.

Current source capability: a zero-parameter i64 main can use Bytes/Text operations and checked arena allocation/read/replace/teardown, move owners through nested records/enums/Option/Result and recursive guest calls, read or mutate checked borrowed fields, replace whole/projected values after RHS effects, run mutable scalar loops, return through cleanup-protected escrow, and clean normal/failure exits. V16 previews named current-profile results, V15 infers private generated expression wrappers through normal HIR/MIR checks, and V10 connects them to isolated REPL/CLI evaluation with explicit allocator authority and original-source failures. Borrowed matching and the partial-move prohibition are unchanged. V11 adds in-memory MIR CFG argument execution; current ownership source lowering does not generate those parameters. Later indirect previews remain V17 and provisional core reads V13. G06 still needs R02–R04/N05, G07 needs N03 native evidence, G08 retains service/REPL/read outcomes, and G14/G16 require native/cross-mode conformance. Other owning services and ownership artifacts retain their gates. Unsupported reachable operations/payloads fail before provider callbacks.

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

V07 readiness audit (2026-10-18 JST): [the owned-call fixture](../examples/scripts/owned_calls.lang) passes source analysis but execution still rejects `Call` before effects. `FramePlan::reserve_after` currently allocates a fresh host wrapper vector per invocation, while the existing implementation contract requires host pools before outermost guest entry. Add **M2-V12 (2 pt)** for reusable wrapper admission/cancellation before V07: **118 → 120 remaining pt**, one new row, **45 done / 1 doing / 46 todo**. V07 retains source owned/loan calls, recursive limits and return escrow at a provisional 3 pt; expose any further prerequisite before coding. V11 and V12 form two prerequisite deliveries, so the next delivery must execute V07's source fixture before further foundation work.

V09 readiness audit (2026-10-18 JST): provisional core reads require the still-pending Vec/Map/shared/arena source adapters, while C03/C06 already depend on V09; the old combined row hid a dependency cycle. Keep prepared replacement in V09, including the missing allocation-free root/field storage exchange and publication-before-old-cleanup adapter, and re-estimate it **2 → 3 pt**. Move provisional-read success/unused/failure-arm and VM/REPL loan conformance into **V13 (2 pt)** after C03/C06/R04/R06/V10. This preserves both outcomes: **112 → 115 remaining pt**, one new row (93 total), **48 done / 1 doing / 44 todo**. V09 must deliver an actual source replacement fixture before further foundation work; the original exit gates are unchanged.

V10 readiness audit (2026-10-18 JST): a piped REPL probe rejects a valid owning function, then reports its call as unknown; a later Copy expression still prints `3`. `src/frontend.rs` forces wrapper inference through the Copy checker, `src/repl.rs` rejects ownership before committing items, and the owned VM entry accepts only zero-argument i64 main. RFC 0006 defines Copy aggregate display but RFCs 0007/0008 do not specify Bytes/Text preview syntax or result retirement after display. Expose **V14 (2 pt)** for that decision, **V16 (3 pt)** for bounded managed-result preview/execution, and **V15 (3 pt)** for independently checked ownership expression inference. Re-estimate V10's session/CLI/authority/error integration **2 → 3 pt**. Keep the later indirect-service result preview design/implementation visible as **V17 (3 pt)** after its real adapters, rather than quietly treating Bytes/Text as the complete M2 result family. Remaining work changes **112 → 124 pt**, four new rows (97 total), **49 done / 1 doing / 47 todo**. The sequence is V14 → V16 source execution → V15 → V10; after the decision, prove an owned-result fixture before more compiler foundation. Preserve all previous outcomes and exit gates; no new implementation is claimed by this audit.

R01 readiness audit (2026-10-18 JST): changing only a checked arena-token `CopyPlace` to read a different live arena passes type, move, loan-activity, loan and failure-CFG checks in [the source-derived probe](../tests/mir_regions.rs). HIR rejects such source; no independent MIR region stage exists. Loan topology deliberately treats non-loan record fields as opaque and supplies no region value graph or call-effect summaries. The 3-point R01 row hid four outcomes. Expose **R08 (3 pt)** for bounded shapes/paths/owner and call boundaries, **R09 (3 pt)** for intra-function brand/loan/store/CFG transfer, and **R10 (3 pt)** for recursive caller-projection/mutation/transient-store summaries. Keep **R01 (2 pt)** for mandatory pipeline admission, structured diagnostics and forged-input/resource/fuzz conformance after all three; preserve the original rejection outcome. This is **113 → 121 remaining pt**, three new rows (100 total), **53 done / 1 doing / 46 todo**. R08 is preparation, not a region proof or permission to enable arena execution/artifacts. After R08/R09, review the source arena path before extending the foundation. The probe also found that reusing a shared allocator parameter for a nested arena is moved during lowering; retain its repair in R06/R07 source conformance. No exit gate closes.

R07 source-path review (2026-10-18 JST): the [source call/escape matrix](../tests/arena_calls.rs), independent MIR call proof and existing R06/runtime cycle, re-entry, stale-generation and cleanup cases pass. No additional production gap was found. G05 remains open because full native arena evidence, including N04, is pending. Next is R02 after its completed D02/D33 prerequisites, then R03 → R04 for the next source shared/weak fixture. R02's 3 pt is provisional until the first fallible-allocation/rollback experiment; expose and re-estimate any missing prerequisite before implementing it rather than silently extending the row. Ownership artifact gates remain open.

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
| M2-V06 | done | 0 | M2-V03,M2-S03 | G06,G08 | Nested Bytes/Text/Copy record/enum/sum owners and reference projections execute; no packing allocation, reverse written exactly-once drops, OOM/panic/fuel and loop cleanup verified. | `92d225a` |
| M2-V12 | done | 0 | M2-V11 | G06 | Reusable host wrappers admit/release main/callee regions without host growth; exact budgets, zero-sized charges, OOM rollback/retry, cumulative requests and pre-callback rejection verified. | `05ebf0b` |
| M2-V07 | done | 0 | M2-V06,M2-V11,M2-V12 | G06 | Execute owned guest calls/results with recursion limits and return escrow. | `15436f0` |
| M2-V08 | done | 0 | M2-V07,M2-D32 | G06 | Generated single/call failure exits preserve nested drop order/spans, protected returns and ordered bounded reports across body/fuel/OOM/interruption and injected cleanup error/panic; poison/reuse and full release verified. | `a7af470` |
| M2-V09 | done | 0 | M2-V04,M2-V06 | G07,G08 | Root/projected replacement retains the captured target across RHS effects, publishes without transfer allocation before old cleanup, and preserves caller ownership, field order/spans, failure cleanup and retry. | `0311bd0` |
| M2-V13 | todo | 2 | M2-C03,M2-C06,M2-R04,M2-R06,M2-V10 | G08 | Verify provisional core read success/unused/failure arms and loan endings across source VM/REPL execution. | — |
| M2-V14 | done | 0 | M2-V08,M2-V09 | G08 | RFC 0010 accepted under delegation: owned REPL preview formats, result lifetime, explicit allocator authority, bounded output and failure precedence; source contract checks, implementation pending. | `b09cc46` |
| M2-V16 | done | 0 | M2-V14,M2-V08,M2-V09 | G08 | Execute a named zero-argument owned result with bounded managed preview and complete result retirement; source fixture and failure evidence. | `040c082` |
| M2-V15 | done | 0 | M2-V14 | G08 | Infer the generated ownership expression wrapper's concrete result through normal HIR/MIR move/loan/region checks without adding user function inference. | `3ecef3b` |
| M2-V10 | done | 0 | M2-V08,M2-V09,M2-V15,M2-V16 | G08 | REPL ownership items/expressions, explicit allocator authority, source-mapped failures and cleanup-isolated canonical results match the script execution semantics. | `834abe8` |
| M2-V17 | todo | 3 | M2-V14,M2-V16,M2-V10,M2-C03,M2-C06,M2-R04,M2-R06 | G08 | Decide an explicit follow-up and implement bounded indirect-service result previews, preserving Map order and shared/weak/arena access constraints; VM/REPL evidence. | — |
| M2-V11 | done | 0 | M2-V06 | G08,G14,G16 | Simultaneous Copy/affine CFG arguments execute through bounded admitted snapshots; cycle/fan-out/enum/owner oracles, loan rebinding, cell/quota/OOM and fuel cleanup verified. | `1dcf260` |

## Shared, arenas and collections

| ID | State | Remaining pt | Needs | Gate | Outcome / acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-R08 | done | 0 | M2-D24 | G05 | Admit finite region type/slot/projection topology, arena-owner identities and region-relevant call boundaries under cumulative budgets; preserve recursive shape edges without expansion. No region proof. | `24fb835` |
| M2-R09 | done | 0 | M2-R08 | G05 | Independently transfer arena brands, reference/captured paths and storage lifetimes through local MIR operations, core values and CFG joins/loops; reject direct cross-region/escape cases. | `ee5b12e` |
| M2-R10 | done | 0 | M2-R09 | G05 | Derive recursive guest-call summaries for caller projections, normal-return mutations and every transient store; reject transitive/cyclic call escapes without HIR facts. | `28f2080` |
| M2-R01 | done | 0 | M2-D24,M2-R08,M2-R09,M2-R10 | G05 | Make independent MIR arena-brand/call-effect proof mandatory before failure/execution admission; preserve structured span diagnostics and forged-input/resource/fuzz conformance. | `dd0b006` |
| M2-R02 | todo | 3 | M2-D02,M2-D33 | G06 | Shared storage uses guest fallible allocation; preflight/rollback accounts for all payload/retirement storage. | — |
| M2-R03 | todo | 2 | M2-R02,M2-D34 | G06 | Production shared/weak token adapters validate ownership and retire payload descendants without a test registry. | — |
| M2-R04 | todo | 3 | M2-R03,M2-V08 | G06,G14 | Execute source shared/weak operations, upgrades, cycle rejection and rollback. | — |
| M2-R05 | done | 0 | M2-D15,M2-D33 | G05,G06 | Arena payload allocation and descendant retirement obey guest budgets and failure rollback. | `cb7a72b` |
| M2-R06 | done | 0 | M2-R01,M2-R05,M2-V08 | G05 | Execute source arena allocation/read/replace/region re-entry (reset)/teardown with stale-reference and reverse-drop fixtures. | `e0f6bb4` |
| M2-R07 | done | 0 | M2-R06,M2-V07 | G05 | Source arena escape/call-brand compile failures and runtime cases agree through guest calls. | `cecd6e8` |
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
