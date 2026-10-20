<!-- canonical-language: en; japanese-translation: ja/milestone-3.md -->

# Milestone 3 delivery board

[日本語版](ja/milestone-3.md) · [Exit gates](roadmap.md#milestone-3-concurrency-and-embedding)

Updated: **2026-10-21 JST**. Entry checkpoint: `984557d`; M2 and the repository review are complete.

**1 done · 1 doing · 37 todo · 108 remaining pt**

**Next: M3-T02 → execute the bounded host-driven task-tree fixture.**

Current delivery: **M3-T02**. Last executable checkpoint: **2026-10-21 JST**, 18 task tests, the host example and the full baseline pass (96 suites / 1,613 tests; zero failures, one existing timing ignore). Normal execution returns 42; normal and nested cancellation retire frames in order 2,1,0 with zero live tasks. fmt, warnings-denied Clippy and rustdoc pass. Remaining acceptance: record the implementation commit and publish completion evidence. No M3 gate is complete.

## Deliveries

Estimates describe remaining work, not elapsed time. Each row is an observable 1–3-point delivery; later rows must be re-audited against the actual interfaces before implementation. `G01`–`G15` name the roadmap gates in order. Evidence cells contain only a completed implementation commit.

| ID | State | Remaining | Needs | Gates | Observable acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M3-T01 | done | 0 | — | G01 | Accept explicit FIFO driver, task ancestry, join, cancellation, failure and bounded admission rules | `8ab751b` |
| M3-T02 | doing | 3 | M3-T01 | G03,G12 | Execute a bounded host-driven task tree with joins, cooperative cancellation and postorder retirement | — |
| M3-T03 | todo | 3 | M3-T01 | G01,G03 | Accept async syntax; parse/check tasks and reject escaping handles and live loans at suspension | — |
| M3-T04 | todo | 3 | M3-T03 | G03 | Verify typed MIR suspension, task scopes and ownership edges independently | — |
| M3-T05 | todo | 3 | M3-T02,M3-T04 | G03 | Execute a source spawn/yield/join fixture in the VM through the shared task driver | — |
| M3-T06 | todo | 3 | M3-T05 | G03,G13 | Execute the same task fixture through native continuations with equivalent events | — |
| M3-T07 | todo | 3 | M3-T06 | G03 | Verify owned locals, nested cancellation, primary/secondary failures and exactly-once cleanup | — |
| M3-T08 | todo | 3 | M3-T07 | G12,G13 | Enforce shared task/frame/fuel controls before effects on all execution paths | — |
| M3-C01 | todo | 2 | M3-T05 | G01,G04 | Accept bounded channel, FIFO wakeup, backpressure, ownership commit and cancellation errors | — |
| M3-C02 | todo | 3 | M3-C01 | G04 | Execute bounded channel send/receive, close and cancelled waiter fixtures | — |
| M3-C03 | todo | 3 | M3-C02 | G04 | Run source channel fixtures and reject payload reuse in VM execution | — |
| M3-C04 | todo | 3 | M3-C03,M3-T06 | G04 | Match channel values, failures and cleanup through native execution | — |
| M3-A01 | todo | 2 | M3-T03 | G01,G05 | Accept recursive Send/Share derivation and source service authority rules | — |
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

Activation audit (2026-10-20 JST): 39 deliveries, initial **110 pt**. This is a bottom-up provisional forecast; the roadmap's **96 pt** are historical gate weights. No work is retired and no additional work has been exposed yet. Re-estimate changed scope explicitly before implementation. Review sequencing after two prerequisites without a new source execution fixture; T02 and T03 must not turn into an open-ended scheduler framework.

T01 completed at `8ab751b`: accepted RFC 0021 and added independent board validation. Semantic-model tests 29/29 and repository consistency 5/5 pass. Retire 2 pt; no additions/re-estimates. Roadmap audit: all 15 gates remain necessary and none is proven by the decision alone.

**110 + 0 − 2 = 108 pt**

| Initial remaining | Net additions/re-estimates | Retired at completion | Remaining |
| ---: | ---: | ---: | ---: |
| 110 pt | +0 pt | −2 pt | 108 pt |

## Exit audit

**0/15 gates complete; 0/96 gate weight.** A delivery is not a gate. G01 waits for task syntax, channels and Send/Share decisions; G14 waits for real cross-mode conformance on macOS AArch64 and Windows x86-64. No P1 work is moved out of M3.

## Sources

- [Roadmap](roadmap.md), [architecture](architecture.md), [completed M2 board](milestone-2.md), and [review](review-2026-10-20.md): last updated 2026-10-20; inspected at `984557d`, 2026-10-20 JST.
- [RFC 0005](rfcs/0005-structured-tasks-and-isolated-services.md) and [RFC 0003](rfcs/0003-versioned-component-contracts.md): last updated 2026-10-17. These contracts remain binding; prototypes do not prove production completion.
