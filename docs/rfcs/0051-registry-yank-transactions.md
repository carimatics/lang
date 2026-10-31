<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0051-registry-yank-transactions.md -->

# RFC 0051: Authenticated yank and unyank transactions

[日本語版](../ja/rfcs/0051-registry-yank-transactions.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E02, M5-E05
- Supersedes: none; elaborates RFCs 0046–0048
- Superseded by: none

## Summary

Add closed yank/unyank actions and audit transitions to the existing immutable registry. An authorized current owner changes a release's selection flag without changing its archive, ownership or identity. One HEAD replacement publishes the flag, package revision and audit event together; a freshly authorized retry returns its original result.

## Motivation

RFC 0046 specifies the public action route, but RFC 0048 deliberately admits only initialization and publication audit variants. Implementing a flag change without specifying its complete transition would either fail recovery or allow unaudited state changes. This proposal defines the missing compatibility contract before E02 implementation.

## Goals

- Preserve exact archives while changing index visibility metadata.
- Authenticate the actual bearer and current owner at admission and commit.
- Specify canonical action identity, conflicts, history validation and retry results.
- Exercise publication and yank/unyank through the bounded authenticated service.

## Non-goals

- Owner changes, principal enrollment, token provisioning/revocation or quarantine workflows (E03–E04).
- Package signatures, trusted production consumption, new source syntax or compiler semantics.
- Deleting releases, pruning history, power-loss availability or malicious-operator rollback protection.

## Guide-level design

An owner publishes `math` version `1.0.0` at package revision 1, then submits a yank action with expected revision 1. Success creates package revision 2. The index marks this exact release yanked; its exact non-quarantined archive remains downloadable. Unyank at expected revision 2 creates revision 3 and clears the flag. Immutable version publication still refuses replacement, even while yanked.

Retry the same canonical action and request ID after a lost response. A known successful action returns its original receipt even if a subsequent unyank changed the current flag. It never re-applies the old action. Current authentication/ownership is required on every retry. A new request ID asking for an already-current flag conflicts and creates no event.

An intervening publication can consume the byte released by encoding `true` instead of `false`. Unyank then refuses REG0002 if its next exact canonical index exceeds 2 MiB; it preserves the current flag, archive and history. The index admission rule applies to actions as well as publication, without truncation or an implicit limit increase.

## Reference semantics

### Canonical action

`POST /v1/packages/NAME/actions` retains RFC 0046's explicitly configured HTTPS origin and bearer handling. E02's host action and E05's HTTP adapter admit exactly these ordered JSON fields: `schema:1`, `request_id`, `expected_revision`, `action`, `reason`, `version`. `action` is `yank` or `unyank`. The body uses canonical two-space JSON plus final LF, is at most 2,048 bytes, and rejects unknown/duplicate/inapplicable fields, noncanonical encodings and unsupported schemas. The request ID is 32 lowercase hex characters, expected revision is a positive `u64`, and version uses the existing exact numeric version grammar. NAME comes from the independently validated route, never an arbitrary body path.

Reason is 1–256 UTF-8 bytes without Unicode control characters. Callers must not place secrets, source text or host paths in this persisted human explanation. Error/debug output never echoes arbitrary body or bearer material. The host action constructor/admitter applies the same bounds and retains exact canonical action identity.

Reconstruct the existing RFC 0046 request digest with method `POST`, path `/v1/packages/NAME/actions`, conditional revision equal to the action's expected revision, and SHA-256/length of the complete canonical body, including request ID and reason. A changed reason, flag, version or revision therefore cannot reuse a successful principal/request-ID pair. No second replay namespace is introduced.

### Admission and transition

Under the existing private nonblocking exclusive registry lock:

1. Recover and validate the complete committed view, then authenticate the actual bearer at fresh host time with RFC 0047 `Operation::Yank(name)` authority. The credential needs the exact name's yank grant and current active owner membership; publish or admin scope alone is insufficient.
2. Reconcile the principal/request-ID pair only after current authorization. Equal request digest returns the original receipt after another fresh authorization check; unequal digest conflicts. Old expected revisions do not prevent an authorized exact retry.
3. For a new request, require the current package revision, an existing exact version, and a requested flag different from its current value. A stale revision or already-current flag conflicts. An authorized absent version is not found. Refusal leaves HEAD, archive bytes and committed history unchanged.
4. Construct a candidate that increments state and affected package revisions exactly once, changes only that release's `yanked` boolean, advances state/authority time floors and audit link, and retains all other release fields, owners, principals, credentials and scopes. Enforce the existing canonical derived-index, state, audit, event and physical/staging quotas before installing candidates.
5. Resample trusted time and reauthorize for the persisted event, stage/sync/install its audit and state, then use the existing final fresh-time/bearer guard immediately before HEAD replacement. No archive candidate is needed. Preserve RFC 0048's structured not-published/unknown-outcome reconciliation.

A quarantined release remains unavailable regardless of its yank flag; these actions cannot clear quarantine. This proposal grants no new guest filesystem or network capability. Concurrent writers use the existing busy/conflict behavior; no lock wait queue or saved actor authority is added.

### Closed audit and receipt variants

Schema 1 admits two additional operation variants, with ordered fields `{kind:"yank", name, version, archive_sha256}` and `{kind:"unyank", name, version, archive_sha256}`. Their result has ordered fields `{state_revision, package_revision, name, version, archive_sha256, yanked}`. `yanked` is respectively true or false, and both operation and result bind the unchanged archive digest from the prior exact release. The common audit reason is the admitted action reason. Principal, nonsecret credential ID, request ID/digest, prior hashes and event time retain RFC 0048's meanings. All required noninitial fields remain non-null. The host receipt remains `{schema:1, audit_sha256, result}`.

History validation reconstructs the canonical action from recorded request ID, prior package revision, operation, reason and version. It verifies the request digest and the recorded actor's authority at event time, reconstructs the entire expected next state, and compares it and the complete result. A flag change cannot conceal an ownership, credential, archive, quarantine or unrelated-package change. Retain every committed generation/event/object and reject duplicate request keys across all mutation kinds.

The HTTP response remains RFC 0046's canonical `{schema:1, registry, request_id, revision, audit_sequence, outcome}`. Revision is the affected package revision; audit sequence is the committed state revision. Yank/unyank outcomes are `yanked`/`unyanked`, including original results on replay. An internal typed commit/replay distinction supplies first-publish 201 versus replay 200; it must not be inferred from a stale caller index. Actions return 200.

### Bounded service integration

Enable mutations only through an explicit host service configuration owning the private registry capability and independently configured service identity/time source. Preserve the existing read-only entry point. Bearers come only from explicit client secret input and the single authenticated request header; no environment, redirect, fallback-origin or saved-actor authorization is added. Public GETs still reject credentials. PUT archive identity must exactly match the validated route before publication; action POSTs require the exact canonical body and JSON content type.

Retain RFCs 0046/0048's framing, deadlines, eight-connection ceiling, 256 MiB aggregate admitted-request memory budget, nonblocking mutation serialization, fresh locked authority and captured read-generation rules. Account for admitted bodies, parsed payloads, response copies and retained/new read data before allocation; old data retains its charge until dropped. A committed mutation must publish a matching validated read generation before another response uses that service state. If rebuilding/validating it fails after commit, refuse subsequent service reads until explicit recovery, while preserving the committed result and same-request reconciliation. Never keep serving a stale captured HEAD or claim rollback after commit. Actual peak accounting and concurrent reads/mutations remain implementation acceptance conditions.

## Execution modes

Yank changes registry selection metadata, not source execution. Exact non-quarantined locked archives remain usable under the independently required consumption evidence. VM and native AOT execute the same retained source; no JIT, component or embedding ABI changes are introduced. E02 must show host publish → yank → exact archive read → unyank without replacing the source; E05 repeats the journey through authenticated HTTPS.

## ABI and compatibility

Affected domains are package and tooling. Existing source editions, MIR, bytecode, component/native ABI, archive and lock formats remain unchanged. State/audit/receipt schema numbers remain 1 with explicitly added closed variants; an older reader rejects a history containing them. Upgrade the private service before the first yank; downgrade requires retaining the newer compatible reader or explicit restoration, never dropping events, clearing flags or resetting HEAD. No deployed public registry migration is assumed.

## Diagnostics and tooling

Retain REG0001 malformed, REG0002 limit, REG0003 authentication, REG0004 authorization, REG0005 conflict and the existing not-found, busy, integrity/state and transport mapping. Static bounded diagnostics never include bearer bytes. New source parser, formatter, lint, LSP and debugger behavior is unnecessary. HTTP request IDs are returned only after safe admission; a lost response requires exact same-request reconciliation.

## Performance and cost model

Each accepted action adds one complete immutable state and audit record, plus staged HEAD bytes, but no source archive. Full-history validation, canonical encoding and retained generations keep RFC 0048's bounded initial-profile costs; no throughput claim is made. A read-only captured generation must not become a mutable authority cache. Measure actual host fixtures and resource limits before completing their corresponding delivery; pure transition evidence does not establish HTTP memory, latency or crash behavior.

## Security and trust boundaries

An action or receipt is not a capability. Real bearer possession, current enrollment/ownership/scope and trusted time control every commit/retry. Retained archive hashes prevent flag changes from replacing content; they do not prove package authorship. Hash-chained history detects corruption relative to HEAD, not hostile operator rollback. Adversarial reasons, body framing, request collisions, revoked/removed actors and malformed historical transitions must fail closed without cleanup of unknown data.

## Alternatives

- **No change:** leaves yank mutation and its audit/history contract unimplementable.
- **External registry/tool:** may host similar actions but does not define this profile's receipt, immutable local state or fresh replay authority.
- **Separate mutable flag/index file:** offers a small write but loses atomic agreement with package revision and audit. The existing single-HEAD generation remains the stronger design.
- **Idempotent no-op with a fresh ID:** simplifies repeated CLI intent but needs a specified new audit/revision outcome. Refusing a duplicate flag keeps one accepted event equal to one actual state transition; exact IDs already provide safe retries.
- **Delete a yanked release or rewrite its archive:** breaks exact lock replay and immutable identity and conflicts with RFC 0046.

## Implementation plan

Scope audit, 2026-10-31: E06 implements guarded store workspace and bounded read-generation capture before E05 authenticated HTTPS integration. [RFC 0053](0053-registry-request-workspace.md) accepts the audited conservative reservation model; production enforcement remains an E06/E05 completion condition. It does not relax this RFC's accepted budget or acceptance requirements.

E03 elaboration: [RFC 0052](0052-registry-owner-and-credential-control.md), accepted 2026-10-31 JST, defines owner add/remove/read, two-step transfer and separate local credential revocation. It retains this transaction/replay boundary; E05 supplies authenticated HTTPS integration.

1. Prepare this accepted elaboration while D05 remains the sole doing delivery; preserve its frozen verification head. The 2026-10-31 17:57 JST scope audit retains E02 at 3 points for the host transaction and exposes E05 at 3 points for authenticated HTTP integration before implementation. No acceptance condition is removed.
2. Add bounded actions, audit/result variants, complete historical transition validation and fresh authenticated yank/unyank transactions. Preserve immutable publication and its existing API behavior.
3. After E03's owner transactions, E05 integrates explicit authenticated publish/actions/owners with the bounded service/client and fresh read generations; verify resource accounting, conflict and retry outcomes with actual HTTPS. E05 is a prerequisite of E04's complete unauthorized/recovery journey.
4. For each implementation delivery, run targeted fixtures, full baseline and latest-head both-host CI, then publish its evidence and re-audit G5's remaining work. E02 alone does not complete E05 or G5.

Branch preparation: `codex/m5-registry-mutations` starts from `codex/m5-registry-offline-cache` at `43dea74` in `carimatics/lang`; PR #38 is its dependency and recorded initial PR base. Original work-unit branch remains `main`; the new checkout had no pre-existing changes. Retarget to `main` only after that dependency merges.

## Test plan

- Publish, yank and unyank with the actual bearer and unchanged archive; stale/absent versions, duplicate flag, wrong scope/name/owner and immutable replacement refusal.
- Exact replay after intervening opposite action; changed reason/body/ID collisions; fresh expiry/time/ownership checks including final HEAD guard.
- Independent canonical action/request-digest vectors; reason byte/control boundaries; malformed/duplicate/unknown fields and bounded input.
- Complete transition forgery tests changing flags, authority, archive, quarantine, unrelated releases, audit result and replay digest.
- Actual initialization/publication/action process exits, reopened complete old/new state, unknown-result reconciliation and concurrent request orders on both hosts.
- E05: HTTPS publish/action/read journey, disabled mutation service, malformed framing/auth/conditional headers, slow or excess bodies, admission-budget boundaries and cancelled workers.
- VM/native execution of the exact downloaded archive throughout the flag journey; no production unsigned-consumption constructor.

## Open questions

Host E02 publication checkpoint, 2026-10-31 JST: `f40d643` passed all 2,655 tests / 182 suites, fmt, strict Clippy, rustdoc, CI `41188137283`'s 18 applicable jobs and native-debug `41188137315`'s three hosts. PR #39 merged as `f74428c`; original main is clean, synchronized (0/0) and identical to the tested tree. This RFC stays accepted because its E05 authenticated HTTPS integration remains outstanding; host completion is not service completion.

Local E02 implementation checkpoint, 2026-10-31 JST: bounded `YankAction` and `Registry::yank` preserve exact archives while publishing a complete audited flag transition. The real host publish → yank → reopen → another-version publish → unyank journey executes 42 in VM/native and returns original receipts on cross-state retries. Thirty store cases pass, including 36 actual yank/unyank process cuts (34 old states/two complete new states), both publish/yank writer orders, fresh final-time guards, unknown-result reconciliation using explicitly retained HEAD bytes, actual nonowner/revoked/wrong-scope credentials and nine recomputed-hash history forgeries. Exact-capacity index publication followed by a +1-byte unyank refuses before writes. Independent canonical vectors, 512 input mutations and strict workspace Clippy pass; the action reader is included in the existing authority fuzz entry point. This is host implementation evidence only: frozen full validation, latest-head both-host CI and publication remain E02 acceptance conditions. E05's authenticated HTTP/workspace-budget/read-generation integration is still unimplemented.

Accepted under RFC 0001 experimental delegation on 2026-10-31 JST. The pure `/private/tmp/lang-m5-yank-policy-probe.js` passed 22 cases covering flag/revision changes, original-result retry after an opposite action, ownership/time refusal, immutable digest preservation and reason bounds. Its canonical fixture is 173 bytes: action SHA-256 `d944ece5a45d67c089f6a0b81bfd9415c4fe5a23e11355e1b3ed90562d1c5dc1`, request SHA-256 `b61f815bee0a02abd24cd9e29e3ac22510c4b506e42b241692a0c3ec57ce6e47`; independent Python encoding agrees. These are policy/encoding evidence only, with no real authentication, filesystem, transport or crash proof.

The single flag transition, original-result retry and unchanged archive fit the existing accepted contracts. At acceptance, service memory accounting, mutable read-generation publication, final-time behavior, process interruption and Windows integration were unverified. The checkpoint above records subsequent local host evidence; frozen full/both-host publication and E05 service integration remain. Re-estimate or expose prerequisites before implementation if work exceeds one observable delivery. No performance or gate-completion claim follows from this acceptance.

## Sources

- [RFC 0046](0046-registry-protocol-and-recovery.md), updated 2026-10-31: HTTP action, request identity, authority and recovery profile.
- [RFC 0047](0047-registry-authority-records.md), updated 2026-10-31: current yank grant and owner policy.
- [RFC 0048](0048-registry-state-and-publication.md), updated 2026-10-31: generation, audit and fresh commit/replay contract.
- [RFC 0049](0049-registry-package-integration.md), updated 2026-10-31: exact registry dependency execution boundary.
- [RFC 0050](0050-registry-cache-and-offline-replay.md), updated 2026-10-31: immutable cached generations and offline replay.
- Local transition/encoding experiment above, run 2026-10-31 JST. Project decision evidence; not an external performance or security claim.
