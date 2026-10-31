<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0055-registry-local-administration.md -->

# RFC 0055: Registry local lifecycle and scoped owner recovery

[日本語版](../ja/rfcs/0055-registry-local-administration.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E08, M5-E04
- Supersedes: none; elaborates RFCs 0046–0048 and 0052
- Superseded by: none

## Summary

Extend the explicit local registry administrator with audited principal enrollment, credential-hash provisioning and principal deactivation. A separately constructed, exact-package recovery administrator can atomically replace that package's owners with one active enrolled principal. No action is available through HTTP or a bearer credential. Existing immutable archives and source execution remain unchanged.

Accepted under RFC 0001's experimental delegation on 2026-10-31 JST. The design completes the local lifecycle required by RFC 0046, preserves the existing local directory capability and reconstructible transaction history, and makes emergency recovery usable even when all 128 owner slots are occupied. A 23-check external policy/encoding experiment covers proposed authority/replay rules and record sizes; it does not establish production enforcement, crash safety or platform behavior. Those remain implementation acceptance conditions. E08 stays todo while E07 completes publication.

## Motivation

RFC 0052 permits owner membership changes and local credential revocation but cannot enroll a later principal or provision its credential. Ordinary transfer also requires a currently authorized owner. An operator who loses every owner credential needs a scoped, audited recovery path instead of editing authority files or rewriting history. Provisioned grants can exceed the 2 KiB audit-record bound: the experiment's 128-grant credential is 15,694 canonical bytes. A digest-bound audit operation is 179 bytes, allowing the complete record to remain in bounded authority state.

## Goals

- Enroll, provision, deactivate and recover through explicit local capabilities, current operator authority and the single durable transaction protocol.
- Preserve append-only identities, global request identity, exact replay and complete history validation.
- Demonstrate local recovery followed by ordinary authorized publication and identical VM/native source execution.

## Non-goals

- Public enrollment, token minting, secret storage, HTTP administrative endpoints, principal reactivation or ID reuse.
- Changing normal two-step ownership transfer, quarantine, release bytes, package trust or signatures.
- A general identity provider, online local/service writer multiplexing, new language semantics or a public native ABI.

## Guide-level design

Stop the registry service and release its writer before opening a local administrator with the trusted directory capability, service identity and configured active operator. Enroll B, then provision a separately generated credential's SHA-256 authenticator and explicit grants. Enrollment alone grants no package ownership or credential. Provisioning never accepts or generates raw bearer material.

For lost or compromised package ownership, construct `RecoveryAdministrator` with the same explicit local inputs plus the exact package name. Submit `OwnerRecovery` with current state and package revisions, active B, a new request ID and a reason. Its single transaction changes the owner list to `[B]`, removing every prior owner. It works at the 128-owner limit. Existing B-only ownership is a conflict; B among several owners can be recovered to B alone. B still needs independently provisioned name-bound grants to publish.

Deactivate a principal only after every affected package retains another active owner. The configured acting operator cannot deactivate itself. Existing ownership entries and credential records remain in history; inactive principals cannot authenticate. Re-enrollment and reactivation are refused. Recovery may first replace an otherwise stranded owner list.

After an unknown commit outcome, reopen with the explicit capability and reconcile the identical request. A saved successful provision or recovery receipt does not reinstate a later revoked credential or an older owner list. Restart the HTTPS service after releasing the administrator's writer.

## Reference semantics

### Capabilities and closed actions

`Administrator` gains enrollment, provisioning and deactivation methods, without a conversion from `Registry`, bearer token, saved JSON or boolean. `RecoveryAdministrator::open` separately requires the local parent `Dir`, child, `ServiceIdentity`, operator and one canonical package name; its private owned registry is not exposed or convertible to either ordinary registry access or a broader administrator. Its name scope is checked before replay as well as before a new transaction. No HTTP route, bearer `admin` grant or action body constructs these capabilities.

Actions are opaque admitted Rust host values. Canonical JSON field order is exact, unknown/duplicate fields are refused, and schema is 1. All actions use the existing 32-lowercase-hex request/principal/credential IDs, checked revisions and nonempty control-free UTF-8 reason of at most 256 bytes. Ordered fields are:

| Type | Ordered fields | Maximum canonical bytes |
| --- | --- | ---: |
| `Enrollment` | `schema,request_id,expected_state_revision,action,reason,principal` | 2,048 |
| `CredentialProvision` | `schema,request_id,expected_state_revision,action,reason,credential` | 65,536 |
| `Deactivation` | `schema,request_id,expected_state_revision,action,reason,principal` | 2,048 |
| `OwnerRecovery` | `schema,request_id,expected_state_revision,expected_package_revision,action,reason,name,principal` | 2,048 |

The corresponding action strings are `enroll_principal`, `provision_credential`, `deactivate_principal` and `recover_owner`. The `credential` is the complete existing RFC 0047 token record, in its unchanged canonical order: `id,principal,registry,origin,authenticator_sha256,issued,expires,revoked,grants`. Admission bounds structure, depth, grants and scalar values; state-dependent checks occur only for a new request. This local 64 KiB ceiling does not enlarge the HTTP action or audit-record limit.

The existing request-digest tuple uses method `LOCAL`, conditional revision `expected_state_revision`, canonical body digest/length and these paths:

| Action | Path |
| --- | --- |
| Enrollment | `/v1/admin/principals/ID/enroll` |
| Provision | `/v1/admin/credentials/ID/provision` |
| Deactivation | `/v1/admin/principals/ID/deactivate` |
| Recovery | `/v1/admin/packages/NAME/recover-owner` |

These are digest-domain identifiers, not routable HTTP endpoints. Recovery's expected package revision is additionally bound inside the body.

### Current authority, replay and transitions

Every action reopens and validates the complete current history under the existing exclusive writer, checks the configured operator is currently active, samples a fresh valid clock at or above the persisted floor, and checks any exact recovery scope. It then looks up the existing global `(operator, request_id)` map. A matching digest returns the original receipt; a different digest conflicts. This lookup precedes new-target state and credential-lifetime checks. Thus an authorized replay after deactivation, expiry, revocation or another owner change returns old evidence without reapplying a transition. An inactive operator cannot use old evidence to regain authority.

For a new ID, expected state revision must equal the current revision. Revisions and time floors advance by the existing checked rules; targets are:

- Enrollment: the principal ID has never existed. Insert one active principal in canonical order. No credential or package changes.
- Provision: the enrolled target is currently active; the new ID and authenticator digest have never appeared, including revoked credentials. Require exact configured registry/origin, `revoked == false`, canonical 1–128 grants, valid ranges and at most seven days between issuance and expiry, and `issued <= now < expires`. Retain the existing 4,096-principal and 4,096-token bounds. Insert the unchanged credential in canonical order; do not modify any package.
- Deactivation: the target exists, is active and differs from the acting operator. Changing only its active bit must leave at least one active owner for every package. Preserve all credential fields, owners and package revisions; deactivation does not implicitly rewrite token revocation bits.
- Recovery: the body name equals the capability's exact scope, the package exists, both revisions match and the target is enrolled and active. Replace only the selected owner list with `[target]`, advance that package revision and refuse the B-only no-op. Preserve all other owners, release descriptors/flags, archives and credentials. Apply exact derived-index size preflight, including revision-digit growth.

Every candidate must satisfy the complete authority/state limits, retained-object accounting, audit capacity and storage quota before staging. Enrollment, provisioning and deactivation leave package revisions and derived index/archive bytes unchanged. All use the existing stage/sync/install/final-guard/HEAD sequence. The final guard resamples current operator authority and clock; a new provisioning transaction also requires the credential to remain currently valid at that clock. Cancellation, conflict, recovery-required and unknown-result behavior are unchanged. No implicit retry, orphan adoption, highest-generation selection or rollback-floor reset is introduced.

### Audit and history reconstruction

The common audit credential ID is null for these local operations, as for initialization/local revocation; bearer operations still require their actual credential ID. Request ID/hash and reason are mandatory. Closed operations and uniquely shaped results are:

| Operation fields | Result fields |
| --- | --- |
| `kind:enroll_principal,principal` | `state_revision,principal,active:true` |
| `kind:provision_credential,credential_id,record_sha256` | `state_revision,credential_id,principal` |
| `kind:deactivate_principal,principal` | `state_revision,principal,active:false` |
| `kind:recover_owner,name,principal` | `state_revision,package_revision,name,recovered_principal` |

`record_sha256` hashes the exact canonical token record, not the full action. No complete credential is embedded in the 2 KiB audit record. History validation retrieves the unique introduced token from the candidate state, validates its canonical record hash, reconstructs the complete action and digest, then checks the full next state and result against the permitted transition. Other operations reconstruct their bodies from prior state, operation fields and reason in the same way. Validate the prior active local operator and recorded time, all target rules and the global request map; merely checking a result or hash chain is insufficient. Full-state comparison rejects unrelated principal, grant, release, ownership or floor changes.

The new local result shapes must remain distinct from existing untagged result variants. In particular recovery does not reuse `OwnerResult`, and no local operation exposes an HTTP `package_result` projection. This audit provides consistency evidence within the trusted local-host model, not a signature proving a human possessed a directory capability.

## Execution modes

These host operations do not change parsing, typing, effects, MIR, source spans, bytecode, native execution, callbacks or constant evaluation. Registry archives restored to authorized use execute identical source in VM and native AOT. No JIT, Wasm or component-host capability is added. Untrusted scripts cannot acquire the directory or local administrative handle.

## ABI and compatibility

Keep existing credential, authority and audit schema numbers and canonical old encodings. Add closed private operation/result variants; older readers reject histories containing them. Upgrade the service and local administrator together before the first new operation. No downgrade/history rewrite or automatic migration is provided. New Rust host APIs are experimental, with no stable C/component ABI or language-edition change. Existing normal owner-transfer requests and receipts retain their semantics.

## Diagnostics and tooling

Use existing bounded structured registry admission, unauthorized, conflict, capacity, busy, recovery-required and unknown-result failures. Debug output and errors omit credential records, authenticators and unbounded bodies. A writer already held by a service or another administrator returns busy without queuing. No new parser, formatter, LSP, DAP or secret-bearing CLI option is required; document the host lifecycle and explicit recovery scope.

## Performance and cost model

Reuse bounded full-history validation and the one-writer transaction machinery. Provision admission may allocate up to the local 64 KiB action ceiling; candidate authority retains existing aggregate limits. Hashing one token and scanning bounded principals, credentials and packages add no unbounded collection. Revoked tokens and inactive principals count against limits permanently in this profile. The standalone local API retains RFC 0053's explicit local capability semantics; it does not become a service request or bypass the network service's shared memory accounting. No latency or throughput claim is made from the policy experiment.

## Security and trust boundaries

The trusted host explicitly grants local directory authority and configures the operator/scope. A malicious local operator who can rewrite storage remains outside the cryptographic trust model. Network bearers, cached receipts, body-supplied identity and ordinary package owners cannot create local administrative authority. Never log or persist bearer secrets; only externally produced hashes enter authority state. Refuse reused hashes even after revocation. Preserve immutable bindings, quarantine/yank independence, full index/storage limits, current authority before replay and final commit, and exact-generation recovery after interruptions.

## Alternatives

- No change leaves required enrollment and lost-owner recovery dependent on unaudited file editing.
- A library/tool wrapper cannot safely add these transitions while complete history rejects them; a UI can later call the explicit host API.
- A general identity provider or HTTP administration API offers online management but expands remote authority and secret lifecycle beyond this milestone.
- Reuse normal owner membership or add-only recovery is smaller, but requires an existing authorized owner or fails at 128 occupied slots and retains compromised owners. An exact-scoped atomic replacement provides the required emergency outcome.
- Embedding the full token in every audit operation is simpler to replay but violates the existing audit bound for valid grants. Binding the token's canonical hash to its unique next-state insertion preserves full reconstruction without enlarging all audit records.

## Implementation plan

1. Add closed action codecs, local capabilities, transition/result variants and complete history validation. Execute enroll → provision → exact-scoped recovery → ordinary publication → VM/native 42.
2. Add current-authority/replay, hostile-history, process interruption, writer contention and exact limit fixtures, including stop/local-admin/restart HTTPS execution.
3. Fuzz admission, run focused and full baseline checks, validate both-host latest-head CI, and publish E08. E04 separately audits the combined G5 outcome; no G5 completion or final M5 capability guide follows from design acceptance.

## Test plan

- Independent canonical body/request/record digest vectors, round trips and malformed/duplicate/unknown/deep/oversized fields; 2 KiB and 64 KiB boundaries and 128-grant constraints.
- Missing, inactive, duplicate and self targets; future/expired provision, seven-day boundary, wrong identity/origin, revoked input, retained duplicate IDs/hashes, 4,096-entry capacity and full 128-owner recovery.
- Refuse last-active-owner deactivation, wrong recovery scope/revisions and no-ops; preserve unrelated package, credentials, both release flags and exact source bytes.
- Reconcile old enrollment/provision/recovery after subsequent deactivation/revocation/owner changes without rollback; reject changed digest or inactive operator before replay, final-time expiry/regression and cross-operation identity reuse.
- Fully rehash forged histories for every new transition; prove exact full-state reconstruction, receipt discrimination and refusal of concealed unrelated mutations.
- Interrupt each durable boundary in child processes, exercise local/service writer contention in both orders, preserve the one acknowledged generation and explicitly reconcile unknown outcomes. Test exact index/storage limits and no-write preflight failures.
- Execute real local enrollment/recovery then ordinary HTTPS publication/download and 42 in VM/native after reopen on supported macOS and Windows; run fmt, strict workspace/fuzz Clippy, full tests, rustdoc and repository consistency.

## Open questions

None blocking acceptance. Production boundary/process tests, actual platform behavior and resource accounting remain unverified implementation assumptions. Reactivation, identity compaction and online administrative service management require separate future decisions.

## Sources

- [RFC 0001](0001-project-charter.md), experimental delegation dated 2026-10-17 JST; consulted 2026-10-31 JST.
- [RFC 0046](0046-registry-protocol-and-recovery.md), [RFC 0047](0047-registry-authority-records.md) and [RFC 0048](0048-registry-state-and-publication.md), repository contracts; last updated 2026-10-31, consulted 2026-10-31 JST.
- [RFC 0052](0052-registry-owner-and-credential-control.md), [RFC 0053](0053-registry-request-workspace.md) and [RFC 0054](0054-registry-quarantine-transactions.md), last updated 2026-10-31; consulted 2026-10-31 JST.
- External 23-check local-administration policy/encoding experiment, executed 2026-10-31 JST; design evidence only. This RFC records project decisions, not claims about external standards or measured production performance.
