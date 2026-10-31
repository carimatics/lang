<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0053-registry-request-workspace.md -->

# RFC 0053: Registry request workspace and mutable read generations

[日本語版](../ja/rfcs/0053-registry-request-workspace.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E06, M5-E05
- Supersedes: none; elaborates RFCs 0046, 0048 and 0051
- Superseded by: none

## Summary

Prepare bounded host workspace admission and replaceable validated read generations before enabling authenticated HTTPS mutations. A metadata-only preflight derives a conservative allocation envelope; the shared budget reserves it before body decoding, recovery or state cloning. The actual store path enforces the observed limits again. A mutation reserves its replacement read generation before commit and refuses stale service reads if refresh fails afterward.

The 2026-10-31 allocation/lifetime audit replaces the proposed state coefficient 48 with 96 and accepts the conservative model below under RFC 0001 delegation. The layout experiment and policy model support this design; they do not establish production enforcement, peak RSS, implementation or gate completion. E06 must enforce the envelope before allocating, including malformed input and postcommit refresh.

## Motivation

The current read-only transport charges connection/TLS/worker storage, retained captured indexes/maps and outgoing archive buffers. Host mutations independently scan an inventory of up to 262,144 physical entries, decode records up to 16 MiB, retain replay/history tables and clone state/authority before producing staged generations. Those paths have no request-budget envelope. Charging an HTTP input body alone cannot cover them. `CapturedReads` also owns the registry and one immutable generation; it cannot refresh after a mutation while keeping allocation charges tied to their owners.

The previous E05 3-point estimate combined this store prerequisite with authentication/framing/client integration. Split it into E06 (3 pt) for guarded host workspace and generation capture, then E05 (3 pt) for the actual HTTPS journey. This is a +3 pt re-estimate of required existing scope, not a new feature or relaxed acceptance condition.

## Goals

- Reserve conservative logical peak workspace before allocating request-owned store payloads or maps.
- Recheck preflight limits at actual read/decode/collection/encoding boundaries.
- Preserve a single committed HEAD, current authority, complete history and exact retries.
- Tie old/new read-generation and response charges to their actual lifetimes.
- Reach a guarded host publish/read/source-execution fixture in E06, then authenticated HTTPS in E05.

## Non-goals

- Raise the 256 MiB aggregate request budget, eight-connection ceiling or existing storage/archive/record limits.
- Prove total-process RSS, allocator/runtime bookkeeping, throughput or availability.
- Make saved plans, cached authority or remote administration into capabilities.
- Add pruning, pagination, a second authority file, signatures or production consumption.

## Guide-level design

Preserve the existing read-only service entry point. A separate explicit mutable service entry point receives the private registry capability and configured host clock. A request may be refused with the existing limit/busy diagnostics even though its archive fits the archive-format ceiling: simultaneous bodies, retained reads and conservative store workspace share the same finite budget.

The mutable initial profile uses a nonblocking exclusive service guard through request processing and response completion. Concurrent work receives busy rather than waiting in an application queue. An in-flight read completes its captured generation before a writer can commit. This trades concurrency for a simple bounded lifetime; it is not a throughput claim. The current read-only entry point retains its existing behavior.

A committed mutation installs a matching validated generation before another successful response uses service state. A refresh failure never rolls back the commit or serves the old generation. Mark the mutable service recovery-required, stop/reopen explicitly, then reconcile the same request identity against the committed history. Local credential administration still requires releasing the writer and uses no HTTP admin route.

## Reference semantics

### Metadata preflight and guarded workspace

Under the existing private directory/writer capability, perform a bounded metadata-only scan of the fixed namespaces. Count physical entries and observe maximum state/object lengths, including recognized staging candidates. Validate bounded names, regular-file/no-follow/link constraints and the existing per-kind limits; do not decode payloads, allocate entry-proportional maps, clean files or change HEAD. Check cancellation/deadline during the scan. Its small fixed scratch must already be charged before scanning.

The resulting envelope is private, ephemeral and tied to this locked request. It is not deserializable authority. Recheck the captured HEAD and enforce its entry/size ceilings again before every corresponding payload allocation, map insertion or decode. A file growing or an entry appearing beyond the envelope refuses before that additional allocation. Existing hash, canonical-format, full-transition and cleanup validation remains mandatory; metadata alone admits no record or source.

Reserve the conservative workspace and candidate-generation allowances from the same shared budget before archive/action decoding and full recovery. A guarded host session may call the existing transaction semantics only while those leases and enforced limits are active. Standalone host APIs retain their explicit local capability semantics; the network adapter must have no unmetered fallback. Returned receipts/read data must acquire their own retained charge before transient workspace is released.

### Proposed allocation envelope

Let `S` be the largest observed state record, `A` the maximum of observed object length and admitted upload length, and `E` the physical entry count. Let `C = min(16 MiB, S + growth)`, with growth `2 × 128 KiB + 4 KiB` for publication and `4 KiB` for owner/yank actions. The physical state ceiling still applies independently. Reservations are:

```text
workspace = 96 × C + 6 × A + 8192 × (E + 8) + 2 MiB
generation = 2 × C + (1024 + 8192) × 1024 + 4096
```

All arithmetic is checked before allocation. Input bounds remain state 16 MiB, archive 18 MiB and 262,144 physical entries; the `E + 8` term is an allocation allowance for candidate metadata, not permission to exceed the physical-entry quota. Bodies, existing read data and connection/TLS/worker charges are additional live leases, not subtracted from this model. A reservation over the remaining 256 MiB budget refuses without writes.

Audit obligations for these coefficients: bound simultaneously live decoded adjacent/history/expected states, authority copies/authenticator tables, both prepared publication buffers, canonical re-encoding capacity, archive input/frozen copies and bounded descriptor/binding scratch; bound inventory/binding/reachable/staging/replay structures by physical entries; and bound new index/map storage from state bytes and existing package/release count ceilings. Include collection capacity growth and the pinned implementations' requested allocation sizes. The two fixed collection ceilings in `generation` deliberately over-reserve small registries. Existing valid large stores can be refused by this conservative service budget without becoming invalid stores.

The audit below is the design basis. Production boundary checks must confirm its assumptions; any failed assumption requires revising the envelope before enabling the service. An arithmetic model or observed RSS is not proof. Later record/layout/clone-lifetime changes must re-audit the envelope rather than silently retain it.

### Allocation and lifetime audit

The audit uses the `18b2128` store, Rust 1.99.0, serde 1.0.229 and serde_json 1.0.154 on supported 64-bit targets. It accounts for requested collection capacity, not allocator arenas, OS page cache or process RSS. Canonical validation happens after deserialization/re-encoding, so compact malformed JSON is included.

| Allocation class | Conservative accounting |
| --- | --- |
| Noncanonical decode | Empty string elements can retain two 24-byte vector slots per roughly three input bytes. Allow 16 times input bytes for decoded records, 32 for pretty-encoding capacity at depth at most 16, and 4 for input/serde scratch: 52 times in total. Fixed roots/minimum capacities use the fixed allowance and remaining state margin. |
| Other state storage | Canonical indentation and required field names bound the audited record layouts within 4 times canonical bytes. Reserve six state equivalents, or 24 times candidate bytes, despite adjacent-history walking and prepared publications retaining serialized bytes rather than decoded states. The overlapping 52 + 24 = 76 factors fit under 96; the remaining 20 covers canonical buffers, authority validation tables, index encoding and conservative overlap. The original 48 factor is superseded before implementation. |
| Archive and scratch | Six times the largest object/upload covers read-buffer capacity and input/frozen archive copies. Descriptor/binding, audit/head/action and fixed metadata scratch also have 2 MiB. Large index encoding uses the state allowance. Source payloads remain ranges into one frozen archive. |
| Physical-entry structures | Inventory, binding, reachability, staging and replay share 8 KiB per entry, including B-tree spare slots and bounded identifier strings. Replay retains neither reason/body nor decoded state; each audit is at most 2 KiB. Eight candidate entries do not enlarge the physical quota. |
| Read generation | Index encoding borrows releases and retains boxed bytes; aggregate content fits twice candidate-state bytes. Maps separately reserve 1 KiB per maximum package/release plus a root. Old generation memory remains charged until dropped. |

The external mirrored-layout experiment passes 72 compact/canonical cases and 8,192 vector-growth checks. Observed maxima, rounded upward, are decoded/canonical 3.196, decoded/compact 14.299, and retained input + decoded state + canonical-buffer capacity/compact 20.749. Measured byte sizes are state 304, authority records 160, principal 32, token 168, grant 32, owner group 48, package 56, release 136 and dependency 144. Cases include invalid empty strings and oversized collection shapes. Ordinary serde visitors make this a layout/capacity check, not production admission or peak-allocation measurement.

Bind the reservation to the owned store and exclusive service operation. Recheck file length and entry ceilings before each read/decode, cap actual reads even if metadata changes, and enforce candidate/generation bounds before allocation and HEAD publication. Reserve before archive admission, full inventory/recovery, cleanup, authority construction or transaction preparation. Keep the guard through refresh/response completion; only a bounded metadata probe precedes reservation. Both supported host families must exercise these paths before E06/E05 completion.

### Read-generation ownership and commit

Separate registry ownership from a validated immutable read generation internally, retaining the old read-only API as a wrapper where useful. A generation contains its exact HEAD identity, bounded indexes and exact committed non-quarantined object identities/lengths; it contains no cached bearer/authority decision. Capture validates complete history under the guarded workspace. Apply the generation allowance incrementally before each retained index/map allocation, so a mistaken estimate fails closed instead of allocating past its lease.

Before mutation, reserve transient workspace plus the replacement-generation allowance while the old generation remains charged. Preserve the existing prewrite derived-index limit, physical quotas, fresh event time, final fresh authorization and single HEAD replacement. After the host result, capture/validate the actual committed generation, replace it under the exclusive service guard, and release old data only when dropped. Exact retries retain the current generation and return the original result; they never restore an old snapshot.

After a committed refresh failure, retain the result/HEAD and enter recovery-required service state. Refuse ordinary service reads and new mutations until explicit reopen validates the current committed state; after recovery the same request ID can reconcile. Never select a highest candidate, recreate a missing HEAD or clear an authority/time floor. A precommit refusal remains definitely uncommitted; ambiguous storage/transport outcomes remain unknown.

### HTTPS adapter boundary retained for E05

Keep RFC 0046's HTTP receipt `{schema:1, registry, request_id, revision, audit_sequence, outcome}` distinct from the host audit receipt. Map original package/state revisions and the admitted request identity; publish returns 201 for a new commit and 200 for replay, actions 200. Do not expose the host receipt as a replacement wire schema. POST action conditions/IDs come from the canonical body; PUT uses its required conditional and idempotency headers. Reject duplicate/inapplicable headers and any route/archive identity mismatch.

Only explicit bearer input may populate the authenticated header; public GETs still reject it. Clear owned secret/header buffers on retirement and emit only bounded static errors. Keep TLS/origin/framing/body/deadline ceilings and no redirects. The final store clock guard must also observe cancellation/deadline before HEAD replacement; a response failure after commit does not undo it. Client failures distinguish definite pre-send refusal from possible post-send publication and direct unknown outcomes to same-identity reconciliation. Actual framing, receipts, current owner/revocation and stop/revoke/reopen remain E05 obligations.

## Execution modes

The envelope changes host admission, not guest execution or authority. E06 must publish and capture a package through the guarded path and execute its exact archive in VM/native. E05 repeats through real certificate-verified loopback HTTPS on both hosts. No compiler, VM, JIT, native ABI or component semantics change.

## ABI and compatibility

Archive, state, audit, lock and wire schema numbers stay unchanged. Existing standalone host and read-only service callers remain supported. The new mutable entry point is explicit and can refuse work under conservative shared-budget pressure. It adds no ambient capability or network administrative endpoint. The explicit busy serialization policy is a new-entry-point availability tradeoff, not an existing read-only API regression.

## Diagnostics and tooling

Use REG0002/413 for resource refusal, REG0008/429 for busy and existing malformed/authentication/authorization/conflict/integrity/state/I/O categories. Unknown outcomes must not be reported as rollback. No source formatter, parser, LSP or debugger changes are needed. Test-only counters may expose requested reservations and lease lifetimes without logging credentials or arbitrary payloads.

## Performance and cost model

Metadata preflight adds a bounded linear directory pass; existing complete inventory/history validation remains linear in retained data. Conservative over-reservation and exclusive responses can reduce throughput and reject work that another allocation strategy could fit. Prefer that explicit initial-profile tradeoff to an unproved peak bound. Measure actual execution only after implementing the audited envelope; do not infer RSS, latency or scalability from the pure model.

## Security and trust boundaries

All payloads still cross full admission and current authorization. The metadata envelope only limits subsequent allocations and must not authorize cleanup or mutation. Recheck actual sizes/counts to prevent preflight races from bypassing reservations. Hold charges until their last owned data retires, including old/new generation overlap and response copies. A hash-chain/HTTP receipt remains unsigned host evidence; F01–F04 trust is separate. Host compromise, external rollback and power-loss availability retain existing residual risks.

## Alternatives

- **No change/body-only charging:** leaves request-owned recovery/state allocations outside the accepted shared budget.
- **Reserve all profile maxima:** simple but exceeds 256 MiB and can make every mutation unusable.
- **Metadata-derived whole-operation envelope:** selected proposal; conservative and auditable without threading allocation hooks through every value, provided actual boundaries enforce it.
- **Per-allocation tracked wrappers:** potentially tighter, but introduce more lease-bearing record/container types and codec/lifetime changes. Reconsider if the whole-operation audit cannot establish a useful bound.
- **Concurrent immutable generation leases:** improve read concurrency but retain arbitrary slow readers' old snapshots and add more overlap accounting. Defer until this explicit serial profile is demonstrated.
- **Drop old reads before commit/rebuild:** simplifies memory but loses the valid current generation on a definitely uncommitted refusal. Reserve the overlap explicitly instead.

## Implementation plan

1. The 2026-10-31 audit accepts the revised coefficients under RFC 0001 delegation. E03 remains the only doing delivery until frozen baseline/CI/publication completes; then select E06 and implement the audited guard before E05.
2. E06: implement bounded metadata preflight, enforced guarded workspace, replacement-generation reservation/capture and the real guarded host execution fixture. Test limits before allocation, races, cancellation and unchanged HEAD on refusal. Keep it at 3 pt only after its selection audit.
3. E05: integrate the explicit mutable HTTPS entry point and client, strict framing/credentials/receipts, serialized generation publication, recovery-required behavior and real both-host execution/revocation journey. Preserve its 3 pt for this distinct outcome.
4. Run each required local baseline and latest-head CI, publish evidence and audit remaining G5 work. E04 remains after E05; no gate closes on workspace preparation alone.

Preparation branch `codex/m5-registry-authenticated-service` starts from `codex/m5-registry-owner-control` at frozen `18b2128` in `carimatics/lang`, with origin/codex/m5-registry-owner-control as its recorded PR base until #40 merges. The new checkout was clean; the original work-unit branch remains main. No implementation has begun on this branch.

## Test plan

- Audited structure/capacity/clone-lifetime bounds on both hosts, including maximal valid collections and bounded malformed records.
- Exact reservation and one-byte-short refusal before corresponding payload/map allocation or staging; checked arithmetic and physical ceilings.
- Actual metadata changes after preflight, oversized/unknown/nonregular entries and no cleanup before full validation.
- Guarded host publish/action/private read/capture with retained-source VM/native execution, complete history and original-result retries.
- Old/new generation overlap, dropped/error responses, failed refresh after commit, explicit reopen and exact reconciliation.
- E05 real TLS framing/authentication/conditional/receipt cases, eight connections, slow peers, cancellation/deadlines and no unbounded queue.

## Open questions

**Accepted under experimental delegation, 2026-10-31 JST:** conservative reservation, exclusive mutable service access and separately charged read generations preserve the accepted protocol budget. The revised state factor explicitly includes noncanonical decoding. The current `/private/tmp/lang-m5-service-policy-probe.js` covers 30 policy/arithmetic cases for finite connections, exclusive access, current-authority replay, old/new lease overlap, precommit refusal and recovery-required refresh. For `S=1024`, `A=3072`, `E=6`, publication, revised values are candidate 267,264 bytes, workspace 27,887,616 bytes, generation 9,975,808 bytes. The Rust probe is `/private/tmp/lang-m5-workspace-probe`. These experiments do not prove production enforcement, TLS, bearer, storage or Windows behavior; actual guarded execution and both-host boundary tests remain E06/E05 acceptance conditions.

The 2026-10-31 source audit identifies the missing request guard and ownership split; it does not measure performance. E06 is newly exposed by re-estimating E05's existing scope 3 → 6 pt, split 3+3. The planning correction and delegated design acceptance do not complete implementation or a gate.

## Sources

- [RFC 0046](0046-registry-protocol-and-recovery.md), updated 2026-10-31: shared budget, HTTP receipts, deadlines and capabilities.
- [RFC 0048](0048-registry-state-and-publication.md), updated 2026-10-31: complete history, quotas and single-HEAD outcomes.
- [RFC 0051](0051-registry-yank-transactions.md), updated 2026-10-31: explicit mutation service and retained/new read-memory requirements.
- [RFC 0052](0052-registry-owner-and-credential-control.md), updated 2026-10-31: current owner/revocation semantics and local stop/reopen administration.
- Local source audit at `18b2128`, 30-case policy model and 72-case/8,192-growth-check mirrored Rust probe, verified 2026-10-31 JST; project evidence only, no external allocation or performance claim.
