<!-- canonical-language: en; japanese-translation: ja/milestone-3.md -->

# Milestone 3 delivery board

[日本語版](ja/milestone-3.md) · [Exit gates](roadmap.md#milestone-3-concurrency-and-embedding)

Updated: **2026-10-27 JST**. Entry checkpoint: `984557d`; M2 and the repository review are complete.

**79 done · 0 doing · 0 todo · 0 remaining pt**

**Next: none — every M3 delivery and exit gate is complete; the next milestone has not been activated.**

Z02 completion (2026-10-27 JST, executable source `9e78f2d`): individually audited all 15 gates against their accepted contracts and implementation/validation. The [bilingual capability guide](milestone-3-capabilities.md) records runnable commands, ownership/failure/authority, public C v1 compatibility, SDK version boundaries, cost evidence and limits. Executed CLI task/channel/service/owned transfers, stored artifacts, native channels, VM/JIT plugins and finite-work refusal. Retained full both-host conformance/old-client migration, isolated SDK startup and unchanged M2 evidence. **Completed: 1 delivery / 2 retired pt; added/re-estimated: 0 pt.** Reconcile **2 + 0 − 2 = 0 pt**, **79 rows / 79 done / 0 doing / 0 todo**, final ledger **110 + 99 − 209 = 0**. Every P0/P1 is complete with no deferral; M4 and later remain unactivated. Earlier checkpoints are preserved in the linked historical record.

Final guide/audit artifact commit: `3dbf7d0`. Verified 2026-10-27 18:19 JST: all five repository-consistency tests, matching bilingual 15-gate/command content, reference targets and `git diff --check`; M2 benchmark sources/historical results remain unchanged.

[Earlier checkpoints, estimate audits and validation evidence](milestone-3-history.md) are retained separately. Do not add maintenance work to this completed record; use the current [review](review-2026-10-27.md) or the next active milestone.

## Deliveries

Estimates describe remaining work, not elapsed time. Each row is an observable 1–3-point delivery; later rows must be re-audited against the actual interfaces before implementation. `G01`–`G15` name the roadmap gates in order. Evidence cells contain only a completed implementation commit.

| ID | State | Remaining | Needs | Gates | Observable acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M3-T01 | done | 0 | — | G01 | Accept explicit FIFO driver, task ancestry, join, cancellation, failure and bounded admission rules | `8ab751b` |
| M3-T02 | done | 0 | M3-T01 | G03,G12 | Execute a bounded host-driven task tree with joins, cooperative cancellation and postorder retirement | `a543d23` |
| M3-T03 | done | 0 | M3-T01 | G01,G03 | Accept source async contracts and preserve task syntax through lossless parsing, recovery and formatting | `0655939` |
| M3-T09 | done | 0 | M3-T03 | G03 | Bind/type async calls and scoped task handles; reject handle escape and invalid async C surfaces | `390e3ca` |
| M3-T10 | done | 0 | M3-T09 | G03 | Reject shared/exclusive loans at suspension with suspension and loan-origin diagnostics | `9f7012d` |
| M3-T11 | done | 0 | M3-T10 | G03 | Validate Task/TaskError and async MIR declarations; reject the old executable/artifact profile | `ae9404d` |
| M3-T04 | done | 0 | M3-T11 | G03 | Verify typed MIR suspension, task scopes and ownership edges independently | `700391d` |
| M3-T05 | done | 0 | M3-T02,M3-T04 | G03 | Execute a source spawn/yield/join fixture in the VM through the shared task driver | `2f58084` |
| M3-T06 | done | 0 | M3-T05 | G03,G13 | Execute the same task fixture through native continuations with equivalent events | `8d6c283` |
| M3-T07 | done | 0 | M3-T12,M3-T13,M3-T14,M3-T15,M3-T16 | G03 | Verify owned locals, nested cancellation, primary/secondary failures and exactly-once cleanup | `8e72ea7` |
| M3-T12 | done | 0 | M3-T06 | G03,G13 | Preserve join-before-local cleanup and reverse publication retirement before owned task execution | `29badaa` |
| M3-T13 | done | 0 | M3-T12 | G03,G13 | Execute owned flat buffers, records and whole sums with moves, replacement and bounded cleanup reports on VM/native | `f75738a` |
| M3-T14 | done | 0 | M3-T13 | G03 | Execute ordinary shared/exclusive loans and borrowed payloads through synchronous calls within task segments | `642e4be` |
| M3-T15 | done | 0 | M3-T14 | G03 | Execute a suspended/cancelled owned Vec/Map/shared/weak pipeline through existing managed services on VM/native | `c1b7887` |
| M3-T16 | done | 0 | M3-T15 | G03 | Execute owned arena task scopes with region-safe loans and descendant-join-before-retirement on VM/native | `59a5070` |
| M3-T08 | done | 0 | M3-T07 | G12,G13 | Enforce shared task/frame/fuel controls before effects on all execution paths | `fdfc755` |
| M3-C01 | done | 0 | M3-T08,M3-A01,M3-A05 | G01,G04 | Accept bounded channel, FIFO wakeup, backpressure, ownership commit and cancellation errors | `2a13940` |
| M3-C05 | done | 0 | M3-C01,M3-T08 | G04,G13 | Execute bounded task park/wake with branded wait tickets, FIFO resumption, cancellation revocation and stale-wake checks | `72e759c` |
| M3-C02 | done | 0 | M3-C01,M3-C05 | G04 | Execute bounded channel send/receive, close and cancelled waiter fixtures | `4056cce` |
| M3-C06 | done | 0 | M3-C02,M3-A06 | G01,G04,G05 | Decide/check opaque channel role types and distinct Send/Share independently in source/MIR; refuse legacy layouts/artifacts | `e16e537` |
| M3-C03 | done | 0 | M3-C02,M3-C06,M3-A05 | G04 | Run source channel fixtures and reject payload reuse in VM execution | `11769c1` |
| M3-C04 | done | 0 | M3-C03,M3-T06 | G04 | Match channel values, failures and cleanup through native execution | `f882074` |
| M3-C07 | done | 0 | M3-C04,M3-A08 | G04,G05,G13 | Execute typed Copy aggregate/sum message staging/recovery and inventory with VM/native/linked channel and service-VM conformance | `7d1a71a` |
| M3-A01 | done | 0 | M3-T09 | G01,G05 | Accept recursive Send/Share derivation and source service authority rules | `9b2b68c` |
| M3-A06 | done | 0 | M3-A01,M3-T11 | G05 | Derive Send/Share independently in source/MIR and check owned async signatures with compile-pass/fail evidence | `d3b33e3` |
| M3-A05 | done | 0 | M3-A06,M3-T08 | G03,G05,G13 | Execute owned async arguments/results with move publication, consuming await, retained-result disposal and controls on VM/native | `ea822bd` |
| M3-A02 | done | 0 | M3-A01,M3-A06,M3-C03 | G05 | Check source actor ownership, moves and non-sendable captures with diagnostics | `8944645` |
| M3-A07 | done | 0 | M3-A02,M3-C03 | G05 | Execute an owned service FIFO source in VM with real state storage/read/whole replacement and finite controls | `d90de9b` |
| M3-A08 | done | 0 | M3-A07 | G05 | Execute handler-boundary service cancellation, pending-send recovery and FIFO queue-before-state retirement | `4223d7b` |
| M3-A03 | done | 0 | M3-A07,M3-A08,M3-C07 | G05 | Execute owner-isolated service state, FIFO messages and cancellation disposal | `8d945b0` |
| M3-A04 | done | 0 | M3-A03,M3-C04 | G05 | Match actor isolation and disposal traces in native execution | `cfd9641` |
| M3-I01 | done | 0 | — | G02 | Accept production IDL/wire/embedding profile, errors, reentry and cancellation | `1f477fa` |
| M3-I02 | done | 0 | M3-I01 | G06 | Parse/check a versioned IDL with bounded diagnostics and invalid-schema fixtures | `6816a74` |
| M3-I03 | done | 0 | M3-I02 | G06 | Implement LCMP3 contract/report and LCVL1/LCSG1 value/signature codecs with independent canonical/malformed/budget vectors | `228daf2` |
| M3-I09 | done | 0 | M3-I02,M3-I03 | G01,G06,G07 | Verify opaque nominal component resource types/capabilities/moves and refuse unsupported execution | `13db0a6` |
| M3-I06 | done | 0 | M3-I02,M3-I03,M3-I09 | G01,G06,G07 | Verify component binding maps and resource-key/call/signature/effect/loan facts independently in source/MIR; retain old-profile refusal | `0646497` |
| M3-I04 | done | 0 | M3-I03,M3-I06 | G06,G14 | Generate C/guest bindings for a typed native-host/script fixture | `adb2e03` |
| M3-I05 | done | 0 | M3-I03 | G06 | Validate interface/version negotiation and incompatible metadata before effects | `6544afe` |
| M3-P01 | done | 0 | M3-I04,M3-I05 | G08 | Require explicit typed import grants and reject missing/mismatched capabilities | `ff94b34` |
| M3-R01 | done | 0 | M3-I04,M3-P01,M3-R04,M3-R05 | G07 | Validate contextual resource indices and execute whole-call preflight/commit through actual adapters; preserve owners on pre-commit refusal | `f3b086c` |
| M3-R02 | done | 0 | M3-R01,M3-E05 | G07 | Enforce borrow scopes, generations, ownership transfer and poisoning | `2adfeb8` |
| M3-R06 | done | 0 | M3-R02 | G07 | Execute whole-call owning native-import arguments with original custody and mandatory retirement | `8914a50` |
| M3-R03 | done | 0 | M3-R02,M3-R06 | G07 | Match resource cleanup across VM/native errors, cancellation and malformed inputs | `0e7b8ae` |
| M3-R04 | done | 0 | M3-I04,M3-P01 | G07 | Execute independently bound source resource arguments/results on VM/native with actual allocator/provider/code ownership and mandatory cleanup; retain old-profile refusal | f06bd95 |
| M3-R05 | done | 0 | M3-I04,M3-P01,M3-R04 | G07 | Transport rich owning source arguments/results through existing VM/native engines with nested owners, original allocator/code ownership, staging cleanup and return escrow | `3a3bbc4` |
| M3-I10 | done | 0 | M3-I06,M3-R03 | G06,G09,G14 | Accept schema-5 task/channel/service/component fields, tags, proofs, limits and compatibility vectors | `51459cb` |
| M3-I11 | done | 0 | M3-I10 | G06,G09,G14 | Encode/decode independently verified task MIR and execute the first stored source task fixture on VM/generated native | `669cc4e` |
| M3-I07 | done | 0 | M3-I06,M3-R03,M3-I10,M3-I11 | G06,G09,G14 | Accept and execute the versioned task/channel/service/component artifact profile with independent validation and old vectors | `a0501f4` |
| M3-P02 | done | 0 | M3-P01,M3-I05,M3-I07 | G08 | Bound metadata/adapter work and reject malformed/untrusted imports before effects | `fc4bd48` |
| M3-E06 | done | 0 | M3-P01,M3-R05,M3-R06 | G07,G09 | Retain original native registrations/code owners through resource retirement after handle release and stored source VM/native execution | `2c26241` |
| M3-E07 | done | 0 | M3-E06,M3-I07,M3-R05 | G07,G09 | Execute successive stored-source VM/native roots from scoped programs over retained resources/allocator, holder authority and lifecycle | `39c9265` |
| M3-E08 | done | 0 | M3-R03,M3-E07,M3-P01 | G07,G09 | Execute C resource indices, typed move/borrow/result/drop and holder-specific close over the original table/allocator | `75ca09c` |
| M3-E09 | done | 0 | M3-E08,M3-E03 | G07,G09 | Execute persistent C callback keep/take and provider close with original owners/destructors and loan/poison/cleanup rules | `7ba6a45` |
| M3-E01 | done | 0 | M3-I04,M3-P01,M3-I07,M3-E06,M3-E07,M3-E08,M3-E09,M3-E04,M3-L04,M3-I15,M3-E15,M3-E16 | G09 | Publish the complete 32-operation C entry and execute generated typed host calls in supported installed modes | `40158d6` |
| M3-E15 | done | 0 | M3-I04,M3-I07,M3-E04,M3-L04 | G09 | Bind checked task-root export requests and execute generated C task calls, refusal and cleanup | `ae90a88` |
| M3-E16 | done | 0 | M3-I15,M3-E15,M3-L04 | G09 | Bind exact SDK-owned native images and execute installed AOT through the C gateway | `42b27ba` |
| M3-E10 | done | 0 | M3-E08,M3-P02 | G09,G12 | Execute cumulative original fuel/host-call/service-work accounting across scoped VM/JIT source calls before child-entry integration | `879a534` |
| M3-E11 | done | 0 | M3-E08,M3-E10 | G09,G12 | Execute VM/JIT source on a scoped original-allocator view with unchanged handles, limits, statistics and provider effects | `93f9b52` |
| M3-E12 | done | 0 | M3-E07,M3-E10,M3-E11 | G09,G12 | Preserve original preparation charges through frame-pool, arena, inspector, shared-heap and retirement failures, then execute finite-budget VM/JIT child callbacks | `44823d6` |
| M3-E13 | done | 0 | M3-E07,M3-E10,M3-E12 | G09,G12 | Borrow original task identity/cancellation/lifecycle and execute actual synchronous VM/JIT callbacks without task readmission | `be604da` |
| M3-E14 | done | 0 | M3-E07,M3-E12,M3-E13 | G09 | Lend resumed component verification to the original preparation ledger with failure prefixes, intrinsic proof costs and one-time entry accounting | `078e9b4` |
| M3-E02 | done | 0 | M3-E08,M3-E07,M3-E06,M3-E10,M3-E11,M3-E12,M3-E13,M3-E14,M3-E03 | G09 | Script calls the native host and receives a nested guest callback | `082d1de` |
| M3-E03 | done | 0 | M3-E06,M3-E07,M3-E14,M3-R03 | G09 | Preserve typed failures, original context, reentry policy and poison across modes | 52b20e0 |
| M3-E04 | done | 0 | M3-E03,M3-T07,M3-E09 | G10 | Preserve logical mixed stacks, cancellation and cleanup across callbacks | `8227a74` |
| M3-E05 | done | 0 | M3-I04,M3-P01,M3-R01 | G07,G09 | Execute a retained native member from VM/native source with canonical transport, actual direct Resource backing mutation, original allocator/context and complete scoped cleanup | `a13272b` |
| M3-L01 | done | 0 | M3-E03 | G11 | Enforce shared heap/allocation limits including callback allocation | `85e7dce` |
| M3-L02 | done | 0 | M3-T08,M3-E03 | G12 | Enforce combined recursion/frame/live-task limits across guest/host calls | `599d7ee` |
| M3-L03 | done | 0 | M3-E04,M3-T08,M3-I12 | G13 | Observe interruption consistently in VM/JIT/AOT and callback boundaries | `c7daed1` |
| M3-L04 | done | 0 | M3-L01,M3-L02,M3-L03,M3-P02 | G11,G12,G13 | Exercise adversarial resource exhaustion without losing mandatory cleanup | `d5ee99a` |
| M3-I08 | done | 0 | M3-I07,M3-E04,M3-P02,M3-I13,M3-I14,M3-I15 | G08,G09,G14 | Verify actual installed task/component startup on both tier-1 hosts | `8f41f44` |
| M3-I12 | done | 0 | M3-I07,M3-E04,M3-P02 | G08,G09 | Emit/bind checked component objects and execute scalar/resource VM/JIT/AOT callbacks with original task authority | `47093ef` |
| M3-I13 | done | 0 | M3-I07,M3-I12 | G08,G09,G14 | Persist/revalidate native task/component records and bind exact linked code without code generation | `8ba2c43` |
| M3-I14 | done | 0 | M3-I13,M3-T08 | G09,G14 | Publish/link installed task packages with exact SDK v2 and original control/cleanup | `042fc38` |
| M3-I15 | done | 0 | M3-I13,M3-I14,M3-P02 | G08,G09,G14 | Publish/link installed component packages with explicit bindings/grants and original callbacks/cleanup | `8b89aae` |
| M3-B01 | done | 0 | M3-L04,M3-A04,M3-I08,M3-E01 | G14 | Pass ownership/failure/capability/resource conformance on both tier-1 hosts | 19a29c0 |
| M3-B02 | done | 0 | M3-B01 | G14 | Stabilize the documented C embedding API and generated bindings with migration tests | 9e78f2d |
| M3-X01 | done | 0 | M3-E01 | G15 | Ship runnable trusted-native and verified-script examples with explicit authority | `8a3ae2e` |
| M3-Z01 | done | 0 | M3-E01,M3-L04 | G03,G04,G05 | Record spawn/join/channel/cancellation/memory costs and preserve M2 workload evidence | `c9ffd19` |
| M3-Z02 | done | 0 | M3-X01,M3-Z01,M3-B02 | G01,G02,G03,G04,G05,G06,G07,G08,G09,G10,G11,G12,G13,G14,G15 | Audit every gate and publish a bilingual capability guide with commands, evidence and limits | 3dbf7d0 |

## Progress and estimate changes

Final reconciliation: baseline 110 pt + explicit additions/re-estimates 99 pt − last estimates retired on completion 209 pt = 0 pt remaining. All 79 deliveries are done, with no doing/todo rows. This archival split changes no estimate, gate or evidence commit. Individual scope audits and accounting remain in the [history](milestone-3-history.md#progress-and-estimate-changes).

**110 + 99 − 209 = 0 pt**

| Baseline remaining | Net additions/re-estimates | Retired at completion | Remaining |
| ---: | ---: | ---: | ---: |
| 110 pt | +99 pt | −209 pt | 0 pt |

## Exit audit

**15/15 gates complete; 96/96 gate weight.** Audited implementation and both-host validation at `9e78f2d`, I08's seven-profile isolated SDK on both hosts, stabilized RFC 0033's old C clients, X01's native/script plugins and Z01's finite costs/M2 parity. The [complete 15-row audit and usage guide](milestone-3-capabilities.md#complete-exit-audit) and [machine record](evidence/2026-10-27-m3-exit-9e78f2d.json) map each requirement to concrete suites. G01/G02 establish accepted contracts; G03–G13 establish actual source/VM/JIT/AOT ownership, failures, capabilities, resources and combined original-scope controls; G14 establishes C compatibility after both-host full conformance; G15 supplies executable examples. Header cross-compilation substitutes for no runtime requirement, and no P0/P1 was deferred. Public C v1 stability does not broaden private SDK/native-language ABI guarantees.

## Sources

- [M3 capability guide](milestone-3-capabilities.md) and [exit record](evidence/2026-10-27-m3-exit-9e78f2d.json): 2026-10-27.
- [Both-host conformance/old-client migration](evidence/2026-10-27-m3-migration-9e78f2d.json): 2026-10-27.
- [Original verification dates, sources and estimate audits](milestone-3-history.md): records dated 2026-10-20–27, preserved verbatim.
