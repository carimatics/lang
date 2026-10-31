<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0054-registry-quarantine-transactions.md -->

# RFC 0054: Audited registry quarantine transactions

[日本語版](../ja/rfcs/0054-registry-quarantine-transactions.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E07, M5-E04
- Supersedes: none; elaborates RFCs 0046–0048 and 0053
- Superseded by: none

## Summary

Implementation checkpoint, verified 2026-10-31 22:39 JST: local host and actual HTTPS journeys quarantine/refuse/restore the original source and execute 42 in VM/native, including stale-index download refusal and old-ID reconciliation without generation rollback. All 62 store tests pass, including 36 quarantine process cuts, both competing writer orders, 12 fully rehashed forgeries, final-time and local-revocation refusal, exact index overflow and workspace −1/equality/+1 boundaries. The independent canonical/request digests and 512 admission mutations pass; the decoder is included in the existing fuzz target. Frozen full baseline and latest-head both-host CI/publication remain; this RFC stays accepted until those requirements are met. The earlier policy-only checkpoint below is historical.

An explicitly package-scoped administrator quarantines or restores an exact release through the existing authenticated action route. Quarantine stops fresh archive download while preserving the immutable archive, yank flag, owners and complete history. One HEAD replacement commits the flag, package revision and audit event. Freshly authorized retries return the original receipt without restoring an older read generation.

Accepted under RFC 0001's experimental delegation on 2026-10-31 JST: the existing `Operation::Admin` policy, transaction/replay guard and bounded index model support this closed extension. A separate 30-check policy/encoding experiment covers authority before replay, expiry, revision/no-op conflicts, opposite actions and retained archive/yank identity. It is not storage, HTTP or both-host evidence. Implementation and the required adversarial/process/transport tests remain unverified.

## Motivation

RFC 0046 requires administrative quarantine, but current audit variants cannot record it. Treating it as yank would incorrectly leave exact downloads available; directly editing a release flag would break full-history validation. E04's remaining-work audit also exposes independent local provisioning/owner-recovery requirements. Track quarantine as E07, local administrative lifecycle as E08, and retain E04 for the combined recovery/unauthorized-operation exit audit.

## Goals

- Commit a current, scoped administrator's quarantine/unquarantine with complete audited state reconstruction.
- Refuse remote exact archive reads while quarantined, retaining every historical byte and independent yank flag.
- Exercise authenticated HTTPS publication → quarantine refusal → restoration → unchanged VM/native execution.

## Non-goals

- Owner authority over quarantine, a public enrollment/credential-management route, or local provisioning/owner recovery (E08).
- Deleting retained releases, changing archive identity, signing/freshness evidence, cache erasure, or recalling bytes already delivered to a caller.
- An out-of-band repair that bypasses corrupt-history refusal or resets a registry.

## Guide-level design

After `math@1.0.0` is published at revision 1, a bearer with an `admin` grant for `math` sends a canonical `quarantine` action with expected revision 1 and a reason. The resulting revision 2 index marks that exact release quarantined; its object GET refuses. An owner-only publish/yank/owners token cannot perform this operation. `unquarantine` at revision 2 creates revision 3 and restores the exact archive, retaining the prior yank flag. It does not imply that the archive is trusted for production execution.

## Reference semantics

### Closed action and current authority

Add `QuarantineAction` with the ordered schema-1 fields `schema`, `request_id`, `expected_revision`, `action`, `reason`, `version`. The action is exactly `quarantine` or `unquarantine`; NAME is the separately admitted route component. Reuse the 2,048-byte canonical two-space JSON plus final LF bound, positive package revision, 32-lowercase-hex request ID, exact version grammar and 1–256-byte control-free reason. Reject unknown, duplicate and inapplicable fields or alternate encoding.

The request digest uses RFC 0048's existing HTTP request tuple for `POST /v1/packages/NAME/actions`, expected package revision and exact canonical body length/hash. It shares request-ID identity with other operations; changing the operation or body conflicts. No caller-supplied actor is accepted.

While holding the existing nonblocking exclusive writer, validate complete history, obtain fresh host time, authenticate the actual bearer and require `Operation::Admin(NAME)`. The package must exist. Current ownership is neither necessary nor sufficient. The exact admin grant cannot authorize ordinary publication, yank or owner changes. Reauthenticate before replay and after staging immediately before HEAD replacement; preserve the original request deadline and cancellation scope. Revoked, expired, inactive or wrong-scope actors cannot replay prior successes.

### Atomic transition and receipt

For a new request, require the current package revision and an existing exact version. Requesting its current quarantine value conflicts without a new event. Change only that release's `quarantined` flag, package revision (+1), state revision (+1), state/authority time floors, and audit sequence/link. Preserve owners, credentials, unrelated releases, immutable descriptor/archive bindings and the independent yank flag. Apply all existing checked increments, state/index/physical/workspace ceilings before writes; restoring `false` or increasing a decimal revision can enlarge the derived index and must still fit.

Add closed audit operations `quarantine` and `unquarantine`, each with `name`, `version`, `archive_sha256`. Their closed result has ordered fields `state_revision`, `package_revision`, `name`, `version`, `archive_sha256`, `quarantined`. Record fresh actor/credential, exact request digest/ID and reason using the existing bounded audit envelope. Historical validation reconstructs the canonical request, verifies the recorded actor at event time, reconstructs the entire next state and compares the complete result. A quarantine record cannot conceal another state mutation.

The successful HTTP response uses RFC 0046's six-field receipt with outcome `quarantined` or `unquarantined`, status 200. An exact authorized retry returns the original receipt without reapplying its flag or rolling back the generation. Standalone metered receipts retain their lease; HTTP bodies and receipts retain RFC 0053's existing reservations through retirement. Unknown commit/refresh/reply outcomes require same-request reconciliation after the required explicit reopen.

### Read, interruption and recovery behavior

The complete public index still reports the exact release and both flags. A current generation serves an object only when a committed non-quarantined release references it. Index/archive reads after quarantine cannot use a stale service generation; concurrent readers already handed bytes are not revocable. History and physical recovery continue validating and retaining the quarantined archive. Normalize both mutable flags only when computing the ephemeral immutable archive-binding fingerprint; full transition comparison still validates both flags separately.

Use the existing staged immutable install, final guard and single HEAD commit point. Interruption yields the complete old or new state; recognized bounded orphan cleanup follows RFC 0048 and never deletes committed quarantined objects. Corrupt/unknown entries refuse recovery. No lock queue, new filesystem/network capability, unsafe code or guest execution authority is introduced.

## Execution modes

No source semantics, MIR, bytecode, JIT, native AOT or component contract changes. A fixture first executes the published source to 42 in VM/native, observes fresh download refusal while quarantined, restores it, and executes the identical original bytes to 42 again. An isolated unsigned fixture is not a production trust constructor.

## ABI and compatibility

The already specified HTTP action/outcome variants become implemented. New Rust APIs are additive. The experimental private schema remains 1 with additional closed audit variants; older readers must reject unfamiliar history rather than infer or discard it. Upgrade the program explicitly before opening such a store. No in-place history rewrite, automatic downgrade, edition or native/component ABI change occurs.

## Diagnostics and tooling

Reuse `REG0001`–`REG0008` classes and the existing HTTP mapping for malformed, authentication, authorization, absent/hidden object, conflict, limit, busy, state and I/O failures. Do not echo bearer/body text. Clients associate the canonical receipt with registry, request, expected revision and quarantine outcome. This does not require parser, formatter, LSP or debugger changes.

## Performance and cost model

Retain the existing 256 MiB logical budget, bounded transaction workspace and overlapping read-generation reservations; no RSS or throughput claim is made. Quarantine does not copy/replace archive content, but complete history validation and retained immutable audit/state generations still have bounded cost and storage growth. Refuse exhausted quotas without pruning history.

## Security and trust boundaries

Quarantine is an availability decision granted only by the exact administrative scope. A still-valid stolen administrator bearer remains a risk until local revocation. It cannot alter source, erase evidence, claim signing trust or revoke already delivered bytes. Local host compromise and rollback remain explicit residual risks. Verify final-time expiry, inactive/revoked authority and differently scoped administrators, not just owner denial.

## Alternatives

- **No change:** leaves the accepted administrative quarantine requirement unimplemented.
- **A proxy/filter:** can deny a URL but cannot atomically bind the flag, index, audit and durable recovery history.
- **Physical deletion or archive replacement:** can destroy historical evidence and breaks immutable identity and recovery.
- **Reuse yank:** is smaller but incorrectly permits exact downloads and grants owners an administrative capability.
- **Chosen closed transition:** reuses the proven transaction boundary while preserving separate authority and flags.

## Implementation plan

1. While E05 is frozen, record this contract and its policy experiment; E05 remains the sole doing delivery.
2. Select E07 after E05 publication. Add bounded action, complete audit/history transition and metered/client/service dispatch together with a real source-execution journey.
3. Verify hostile histories, exact limits, authority/replay, process cuts, competing writers and actual TLS on both hosts, then publish through latest-head CI.
4. E08 supplies independently audited local administrative lifecycle; E04 audits the complete G5 recovery and unauthorized-operation journey. Neither is credited by E07.

## Test plan

- Independent canonical action/request vectors; duplicate/schema/UTF-8/size/reason/version/ID and mutation inputs.
- Current scope before replay, owner-only/admin-other-name refusal, final expiry/revocation/inactivity and old-ID retry after opposite actions.
- Quarantine/unquarantine preserving yank and exact archive identity; full rehashed forgeries of every unrelated authority/release/result field.
- Actual process-exit cuts, both writer orders, old/new reopen, recognized/unknown orphans, unavailable download, unknown-result reconciliation and explicit service stop/reopen.
- Exact derived-index and logical-memory boundaries without writes on refusal; complete local baseline, strict fuzz lint and latest-head Windows/macOS CI.

## Open questions

No unresolved contract decision blocks implementation. Actual storage/HTTP enforcement, full-history forged-transition refusal, resource boundaries and both-host behavior remain acceptance conditions, not conclusions from the policy model.

## Sources

- [RFC 0046](0046-registry-protocol-and-recovery.md), updated 2026-10-31: wire variants, scoped administration, immutable storage and recovery.
- [RFC 0047](0047-registry-authority-records.md), updated 2026-10-31: exact admin grants and fresh authority.
- [RFC 0048](0048-registry-state-and-publication.md), updated 2026-10-31: full-history validation, request identity and orphan recovery.
- [RFC 0051](0051-registry-yank-transactions.md), updated 2026-10-31: independent mutable yank transition.
- [RFC 0053](0053-registry-request-workspace.md), updated 2026-10-31: shared workspace, response lifetimes and cancellation.
