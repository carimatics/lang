<!-- canonical-language: en; japanese-translation: ja/milestone-3.md -->

# Milestone 3 delivery board

[日本語版](ja/milestone-3.md) · [Exit gates](roadmap.md#milestone-3-concurrency-and-embedding)

Updated: **2026-10-21 JST**. Entry checkpoint: `984557d`; M2 and the repository review are complete.

**5 done · 1 doing · 35 todo · 102 remaining pt**

**Next: M3-T04 → independently verify task MIR scopes, suspension and ownership edges before the source VM fixture.**

Current delivery: **M3-T04**. Last executable checkpoint: **`9f7012d`, verified 2026-10-21 03:29 JST**: source claim and lexical suspension-loan proofs are combined without executable promotion. Eleven new regressions cover both loan kinds at explicit/implicit points, argument temporaries, return/failure edges, reborrows/unsafe, branch/loop/pattern state, resource limits and the old MIR refusal. Full baseline: **98 suites / 1,652 passed / 0 failed / 1 existing timing ignore**; fmt, warnings-denied Clippy, rustdoc and 11 fuzz-target compile/lint checks pass. Remaining T04 acceptance: independent task MIR effects/types/scopes, suspension continuations and ownership/cleanup edges, with malformed-input validation. Retain SRC0026 until the required MIR proofs and execution adapter exist. No source task execution or M3 gate is complete.

**This update: T10 completed (−2 pt); no new tasks or estimates.** Previous remaining **104 − 2 = 102 pt**. There are **41 rows**; cumulative retirement is **13 pt**, separate from **+5 pt** added estimates.

## Deliveries

Estimates describe remaining work, not elapsed time. Each row is an observable 1–3-point delivery; later rows must be re-audited against the actual interfaces before implementation. `G01`–`G15` name the roadmap gates in order. Evidence cells contain only a completed implementation commit.

| ID | State | Remaining | Needs | Gates | Observable acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M3-T01 | done | 0 | — | G01 | Accept explicit FIFO driver, task ancestry, join, cancellation, failure and bounded admission rules | `8ab751b` |
| M3-T02 | done | 0 | M3-T01 | G03,G12 | Execute a bounded host-driven task tree with joins, cooperative cancellation and postorder retirement | `a543d23` |
| M3-T03 | done | 0 | M3-T01 | G01,G03 | Accept source async contracts and preserve task syntax through lossless parsing, recovery and formatting | `0655939` |
| M3-T09 | done | 0 | M3-T03 | G03 | Bind/type async calls and scoped task handles; reject handle escape and invalid async C surfaces | `390e3ca` |
| M3-T10 | done | 0 | M3-T09 | G03 | Reject shared/exclusive loans at suspension with suspension and loan-origin diagnostics | `9f7012d` |
| M3-T04 | doing | 3 | M3-T10 | G03 | Verify typed MIR suspension, task scopes and ownership edges independently | — |
| M3-T05 | todo | 3 | M3-T02,M3-T04 | G03 | Execute a source spawn/yield/join fixture in the VM through the shared task driver | — |
| M3-T06 | todo | 3 | M3-T05 | G03,G13 | Execute the same task fixture through native continuations with equivalent events | — |
| M3-T07 | todo | 3 | M3-T06 | G03 | Verify owned locals, nested cancellation, primary/secondary failures and exactly-once cleanup | — |
| M3-T08 | todo | 3 | M3-T07 | G12,G13 | Enforce shared task/frame/fuel controls before effects on all execution paths | — |
| M3-C01 | todo | 2 | M3-T05 | G01,G04 | Accept bounded channel, FIFO wakeup, backpressure, ownership commit and cancellation errors | — |
| M3-C02 | todo | 3 | M3-C01 | G04 | Execute bounded channel send/receive, close and cancelled waiter fixtures | — |
| M3-C03 | todo | 3 | M3-C02 | G04 | Run source channel fixtures and reject payload reuse in VM execution | — |
| M3-C04 | todo | 3 | M3-C03,M3-T06 | G04 | Match channel values, failures and cleanup through native execution | — |
| M3-A01 | todo | 2 | M3-T09 | G01,G05 | Accept recursive Send/Share derivation and source service authority rules | — |
| M3-A02 | todo | 3 | M3-A01 | G05 | Check source actor ownership, moves and non-sendable captures with diagnostics | — |
| M3-A03 | todo | 3 | M3-A02,M3-C03 | G05 | Execute owner-isolated service state, FIFO messages and cancellation disposal | — |
| M3-A04 | todo | 3 | M3-A03,M3-C04 | G05 | Match actor isolation and disposal traces in native execution | — |
| M3-I01 | todo | 3 | — | G02 | Accept production IDL/wire/embedding profile, errors, reentry and cancellation | — |
| M3-I02 | todo | 3 | M3-I01 | G06 | Parse/check a versioned IDL with bounded diagnostics and invalid-schema fixtures | — |
| M3-I03 | todo | 3 | M3-I02 | G06 | Encode/decode bounded scalar, record, variant and sequence values independently | — |
| M3-I04 | todo | 3 | M3-I03 | G06,G14 | Generate C/guest bindings for a typed native-host/script fixture | — |
| M3-I05 | todo | 2 | M3-I03 | G06 | Validate interface/version negotiation and incompatible metadata before effects | — |
| M3-R01 | todo | 3 | M3-I04 | G07 | Execute canonical resource preflight/commit/rollback through actual adapters | — |
| M3-R02 | todo | 3 | M3-R01 | G07 | Enforce borrow scopes, generations, ownership transfer and poisoning | — |
| M3-R03 | todo | 3 | M3-R02 | G07 | Match resource cleanup across VM/native errors, cancellation and malformed inputs | — |
| M3-P01 | todo | 2 | M3-I04 | G08 | Require explicit typed import grants and reject missing/mismatched capabilities | — |
| M3-P02 | todo | 3 | M3-P01,M3-I05 | G08 | Bound metadata/adapter work and reject malformed/untrusted imports before effects | — |
| M3-E01 | todo | 3 | M3-I04,M3-P01 | G09 | Native host loads a verified script and invokes the generated typed API | — |
| M3-E02 | todo | 3 | M3-E01 | G09 | Script calls the native host and receives a nested guest callback | — |
| M3-E03 | todo | 3 | M3-E02,M3-R03 | G09 | Preserve typed failures, original context, reentry policy and poison across modes | — |
| M3-E04 | todo | 3 | M3-E03,M3-T07 | G10 | Preserve logical mixed stacks, cancellation and cleanup across callbacks | — |
| M3-L01 | todo | 3 | M3-E03 | G11 | Enforce shared heap/allocation limits including callback allocation | — |
| M3-L02 | todo | 3 | M3-T08,M3-E03 | G12 | Enforce combined recursion/frame/live-task limits across guest/host calls | — |
| M3-L03 | todo | 3 | M3-E04,M3-T08 | G13 | Observe interruption consistently in VM/JIT/AOT and callback boundaries | — |
| M3-L04 | todo | 3 | M3-L01,M3-L02,M3-L03,M3-P02 | G11,G12,G13 | Exercise adversarial resource exhaustion without losing mandatory cleanup | — |
| M3-B01 | todo | 3 | M3-L04,M3-A04 | G14 | Pass ownership/failure/capability/resource conformance on both tier-1 hosts | — |
| M3-B02 | todo | 3 | M3-B01 | G14 | Stabilize the documented C embedding API and generated bindings with migration tests | — |
| M3-X01 | todo | 2 | M3-B02 | G15 | Ship runnable trusted-native and verified-script examples with explicit authority | — |
| M3-Z01 | todo | 3 | M3-B01 | G03,G04,G05 | Record spawn/join/channel/cancellation/memory costs and preserve M2 workload evidence | — |
| M3-Z02 | todo | 2 | M3-X01,M3-Z01 | G01,G02,G03,G04,G05,G06,G07,G08,G09,G10,G11,G12,G13,G14,G15 | Audit every gate and publish a bilingual capability guide with commands, evidence and limits | — |

## Progress and estimate changes

Activation audit (2026-10-20 JST): 39 deliveries, initial **110 pt**. This is a bottom-up provisional forecast; the roadmap's **96 pt** are historical gate weights. At activation no work was retired and no additional work was exposed. Re-estimate changed scope explicitly before implementation. Review sequencing after two prerequisites without a new source execution fixture; T02 and T03 must not turn into an open-ended scheduler framework.

T01 completed at `8ab751b`: accepted RFC 0021 and added independent board validation. Semantic-model tests 29/29 and repository consistency 5/5 pass. Retire 2 pt; no additions/re-estimates. Roadmap audit: all 15 gates remain necessary and none is proven by the decision alone.

T02 completed at `a543d23` (2026-10-21 JST): bounded FIFO kernel, invocation-specific handles, direct-child joins, live/total admission, cooperative cancellation and separately acknowledged cleanup. Eighteen focused tests include 512-deep cancellation, boundary-position exploration, failure ordering, refusal atomicity and cleanup-entry cancellation. The executable host example returns 42 and verifies normal/nested-cancel frame retirement 2,1,0 with zero live tasks. Full baseline: **96 suites / 1,613 passed / 0 failed / 1 existing timing ignore**, suite-time sum **439.11s**; fmt, Clippy (**1.08s**) and rustdoc (**26.32s**) pass. Documentation consistency passes after RFC/index updates. Retire T02's last 3 pt.

Source-scope audit (2026-10-21 JST): the initial T03 bundled a language decision/grammar with two independent source proofs. The actual frontend has separate resolved typing and lexical loan analyses, followed by independent MIR verification; syntax acceptance cannot prove scoped-handle lifetime or no-borrow-at-suspension. Preserve all original acceptance conditions: keep T03's 3 pt for the decision/lossless syntax and expose **T09 (3 pt)** and **T10 (2 pt)** before their implementation. Net addition **+5 pt**, rows **39 → 41**; T04 now depends on T10 and A01 on typed task handles in T09. These are new remaining estimates, not completed work or retroactive increases to T02.

Roadmap/sequencing audit after two prerequisites: T01/T02 establish the first host outcome, not G03. Retain all 15 gates and all P1 work. Stop expanding kernel/IDL scaffolding; proceed **T03 → T09 → T10 → T04 → T05** to the first actual source VM fixture, then T06 native parity. Current source/ABI/format choices still require their accepted decisions. No milestone weight or exit requirement is reduced.

T03 completed at `0655939` (2026-10-21 JST): accepted RFC 0022; exact task keywords and distinct async/scope/prefix/statement CST nodes; bounded recovery, precedence and whitespace-only formatting. `async: T` is diagnosed locally as a reserved parameter name while `async fn` remains a following-declaration recovery boundary. Fourteen regressions prove task syntax, comment/span preservation, keyword migration, depth limits, unavailable-profile admission through file/virtual/cached modules, CLI, REPL and LSP. LSP formatting retains SRC0026; REPL refusal preserves earlier definitions. The candidate fixture now parses and receives the explicit unavailable-profile diagnostic rather than syntax errors. Full baseline: **96 suites / 1,627 passed / 0 failed / 1 existing timing ignore**, suite-time sum **430.74s**; fmt, Clippy (**39.56s**), rustdoc (**36.94s**) and 11 fuzz-target compile/lint checks (**3.22s**) pass. Fuzz-target compilation is not a fuzz campaign. Bilingual metadata/code examples/local references and repository consistency pass. Retire the last T03 estimate of 3 pt; additions remain +5 pt and row count remains 41.

T03 roadmap audit: the source async decision closes only part of G01. Channels/backpressure, Send/Share, production embedding, source proofs and cross-mode execution are still required; all 15 gates stay open. Preserve T09 → T10 → T04 → T05 → T06 as the next source/VM/native path. Do not expose Task through existing C/serialized profiles or retire artifact/startup work without accepted format extensions and conformance evidence. Current T09 acceptance remains the explicitly split 3-point typing/scope outcome; no new prerequisite or estimate is introduced by this syntax delivery.

T09 completed at `390e3ca` (2026-10-21 JST): resolved async effects, Task/TaskError typing, await Result types, call/move/consume diagnostics and bounded lexical scope provenance. Task claims cannot escape, be borrowed/matched, enter containers/nested tasks or appear in parameters/results/C surfaces. Existing finite C validation also refuses TaskError. The first transfer profile admits scalar/finite Copy values; owned Send/Share remains A01, not a claim that owned values are intrinsically non-sendable. Explicit generic Result unwraps use the existing syntax; opaque Task matching follows existing loan semantics. Fourteen new regressions cover branches/loops, raw IDs/spans, work/allocation/diagnostic caps and separation from executable/loan proof. Existing file/virtual/cached modules and CLI/REPL/LSP retain transactional refusal; valid source still receives SRC0026. Full baseline: **97 suites / 1,641 passed / 0 failed / 1 existing timing ignore**, suite-time sum **437.84s**; fmt, warnings-denied Clippy, rustdoc and all 11 fuzz-target compile/lint checks pass. The raw HIR fuzz generator now includes async effects, Task/TaskError and spawn/await; no fuzz campaign is claimed. Retire T09’s last 3 pt; no additions or re-estimates.

T09 roadmap audit: all 15 gates and P1 outcomes remain required. Proceed **T10 → T04 → T05 → T06** using the existing lexical loan engine and bounded task driver. Do not add scheduler/IDL scaffolding before the next source fixture; source task execution, independent MIR proofs and native parity remain unproven.

T10 completed at `9f7012d` (2026-10-21 JST): the lexical loan engine now handles task expressions/scopes and rejects live shared/exclusive loans at await, yield, cancel_check and implicit joins with SRC0028 and the loan origin. Return/failure edges are checked before control-flow merging can discard them; exiting lexical loans end before their join and outer loans remain live at a nested join. Spawn/cancel are not suspension points. TaskLoans combines immutable source claim/loan evidence; valid source still receives SRC0026 and both pure async and Task-valued source are refused by the old ownership MIR boundary. Eleven regressions plus the existing 47 related tests cover the rule, call argument order, temporary loan release, reborrows, unsafe, branching/loops/patterns, source identity and work/allocation/depth/loan/diagnostic caps. Full baseline: **98 suites / 1,652 passed / 0 failed / 1 existing timing ignore**, suite-time sum **440.07s**; fmt, warnings-denied Clippy, rustdoc and all 11 fuzz-target compile/lint checks pass. No fuzz campaign or source task execution is claimed. Retire T10’s last 2 pt; no additions or re-estimates.

T10 roadmap audit after the two source prerequisites T09/T10: source claims and loans are necessary but do not prove G03. Existing MIR ownership declaration/body verification is a separate boundary; T04 must add actual task scopes/suspension/ownership edges and independent validation rather than treating HIR evidence as MIR evidence. Continue **T04 → T05 → T06** to the first source VM fixture and native parity; add no scheduler/IDL framework in between. Keep all 15 gates, all P1 outcomes and the current transfer/ABI/format restrictions.

**110 + 5 − 13 = 102 pt**

| Initial remaining | Net additions/re-estimates | Retired at completion | Remaining |
| ---: | ---: | ---: | ---: |
| 110 pt | +5 pt | −13 pt | 102 pt |

## Exit audit

**0/15 gates complete; 0/96 gate weight.** A delivery is not a gate. Source async syntax is accepted, but G01 still waits for channel/backpressure and Send/Share decisions; G14 waits for real cross-mode conformance on macOS AArch64 and Windows x86-64. No P1 work is moved out of M3.

## Sources

- Activation references: [roadmap](roadmap.md), [architecture](architecture.md), [completed M2 board](milestone-2.md), and [review](review-2026-10-20.md), as of `984557d`; those versions last updated 2026-10-20, inspected 2026-10-20 JST. The T09 roadmap/architecture reconciliation is dated 2026-10-21 JST.
- [RFC 0005](rfcs/0005-structured-tasks-and-isolated-services.md), last updated 2026-10-21, and [RFC 0003](rfcs/0003-versioned-component-contracts.md), last updated 2026-10-17. These contracts remain binding; prototypes do not prove production completion.
- [RFC 0021](rfcs/0021-bounded-structured-task-driver.md), last updated 2026-10-21; current kernel, tests and example inspected at `a543d23` on 2026-10-21 JST. Source-scope audit uses `crates/hir/src/resolved.rs`, `crates/hir/src/program/`, `crates/hir/src/loans.rs` and `crates/semantics/src/analysis.rs` from the same checkpoint.
- [RFC 0022](rfcs/0022-source-structured-tasks.md), last updated 2026-10-21; syntax, module admission, REPL and CLI/LSP fixtures inspected at `0655939` on 2026-10-21 JST. Source grammar/formatter contracts and current execution limits are recorded separately there.

- T09 evidence: [RFC 0022](rfcs/0022-source-structured-tasks.md), `crates/hir/src/tasks.rs` and `crates/semantics/tests/tasks.rs`; last updated 2026-10-21, inspected at `390e3ca` on 2026-10-21 JST.
- T10 evidence: [RFC 0022](rfcs/0022-source-structured-tasks.md), `crates/hir/src/loans.rs`, `crates/hir/src/tasks.rs` and `crates/semantics/tests/task_loans.rs`; last updated 2026-10-21, inspected at `9f7012d` on 2026-10-21 JST. MIR boundary audit: `crates/mir/src/unit/ownership/declarations.rs` and `bodies.rs` have no embedded update date; verified at the same checkpoint/date.
