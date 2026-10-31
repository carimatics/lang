<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0053-registry-request-workspace.md -->

# RFC 0053: Registry request workspace and mutable read generations

[日本語版](../ja/rfcs/0053-registry-request-workspace.md)

- Status: proposed
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E06, M5-E05
- Supersedes: none; elaborates RFCs 0046, 0048 and 0051
- Superseded by: none

## Summary

Prepare bounded host workspace admission and replaceable validated read generations before enabling authenticated HTTPS mutations. A metadata-only preflight derives a conservative allocation envelope; the shared budget reserves it before body decoding, recovery or state cloning. The actual store path enforces the observed limits again. A mutation reserves its replacement read generation before commit and refuses stale service reads if refresh fails afterward.

The proposed numerical coefficients below are not yet an established allocation bound. This RFC stays proposed until their type/allocation/lifetime audit is complete. The 30-case model establishes policy/arithmetic only. No implementation or gate completion follows from this proposal.

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

Let `S` be the largest observed state record, `A` the maximum of observed object length and admitted upload length, and `E` the physical entry count. Let `C = min(16 MiB, S + growth)`, with proposed growth `2 × 128 KiB + 4 KiB` for publication and `4 KiB` for owner/yank actions. The physical state ceiling still applies independently. Proposed reservations are:

```text
workspace = 48 × C + 6 × A + 8192 × (E + 8) + 2 MiB
generation = 2 × C + (1024 + 8192) × 1024 + 4096
```

All arithmetic is checked before allocation. Input bounds remain state 16 MiB, archive 18 MiB and 262,144 physical entries; the `E + 8` term is an allocation allowance for candidate metadata, not permission to exceed the physical-entry quota. Bodies, existing read data and connection/TLS/worker charges are additional live leases, not subtracted from this model. A reservation over the remaining 256 MiB budget refuses without writes.

Audit obligations for these coefficients: bound simultaneously live decoded adjacent/history/expected states, authority copies/authenticator tables, both prepared publication buffers, canonical re-encoding capacity, archive input/frozen copies and bounded descriptor/binding scratch; bound inventory/binding/reachable/staging/replay structures by physical entries; and bound new index/map storage from state bytes and existing package/release count ceilings. Include collection capacity growth and the pinned implementations' requested allocation sizes. The two fixed collection ceilings in `generation` deliberately over-reserve small registries. Existing valid large stores can be refused by this conservative service budget without becoming invalid stores.

Before accepting this RFC, establish the coefficients from the actual pinned types, codecs, collection behavior and lifetimes, with executable boundary checks. If this audit fails, revise the formula before implementation; do not present a successful arithmetic model or observed RSS as proof. Later record/layout/clone-lifetime changes must re-audit the envelope rather than silently retain it.

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

1. Audit the proposed coefficients and lifetime proof, then accept or revise this RFC under RFC 0001 delegation. E03 remains the only doing delivery while its frozen baseline/CI runs.
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

**Needs prototype/measurement:** establish the proposed coefficients and an enforceable store envelope before acceptance. The current `/private/tmp/lang-m5-service-policy-probe.js` passes 30 policy/arithmetic cases for finite connection admission, exclusive service access, current-authority replay, old/new lease overlap, precommit refusal and recovery-required postcommit refresh. Independent JavaScript/Python calculations agree for `S=1024`, `A=3072`, `E=6`, publication: candidate 267,264 bytes, workspace 15,058,944 bytes, generation 9,975,808 bytes. This proves neither real allocation bounds nor TLS, bearer, storage or Windows behavior.

The 2026-10-31 source audit identifies the missing request guard and ownership split; it does not measure performance. E06 is newly exposed by re-estimating E05's existing scope 3 → 6 pt, split 3+3. No user decision is required for that planning correction; acceptance is pending technical evidence, not permission.

## Sources

- [RFC 0046](0046-registry-protocol-and-recovery.md), updated 2026-10-31: shared budget, HTTP receipts, deadlines and capabilities.
- [RFC 0048](0048-registry-state-and-publication.md), updated 2026-10-31: complete history, quotas and single-HEAD outcomes.
- [RFC 0051](0051-registry-yank-transactions.md), updated 2026-10-31: explicit mutation service and retained/new read-memory requirements.
- [RFC 0052](0052-registry-owner-and-credential-control.md), updated 2026-10-31: current owner/revocation semantics and local stop/reopen administration.
- Local source audit at `18b2128` and 30-case model, verified 2026-10-31 JST; project evidence only, no external allocation or performance claim.
