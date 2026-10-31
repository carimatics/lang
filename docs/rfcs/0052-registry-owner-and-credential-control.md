<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0052-registry-owner-and-credential-control.md -->

# RFC 0052: Registry ownership and local credential revocation

[日本語版](../ja/rfcs/0052-registry-owner-and-credential-control.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E03
- Supersedes: none; elaborates RFCs 0046–0048 and 0051
- Superseded by: none

Publication checkpoint, verified 2026-10-31 19:15 JST: `18b2128` passes fmt, strict workspace Clippy, 2,667 tests/182 suites, rustdoc and locked fuzz Clippy. CI 41190281059 passes 18 applicable jobs and native-debug 41190281058 passes all three hosts. Windows and both macOS logs confirm owner/revocation and index-capacity fixtures. PR #40 merged as `ee0d985`; main is clean, synchronized and identical to the tested tree. Earlier pending-validation notes below are historical. This implements the local host contract; HTTPS integration remains RFC 0051/E05 with RFC 0053/E06 workspace support.

## Summary

Define authenticated owner add/remove, private owner reads, safe two-step ownership transfer and explicitly local credential revocation. These operations share the current registry lock, complete history validation, fresh-time checks and single HEAD commit. Removed owners and revoked credentials cannot replay earlier successes to recover lost authority.

## Motivation

RFC 0047 separates current owner/credential policy from actual mutation. E02 now changes audited release flags but cannot transfer a package or persist revocation. Those remaining E03 fixtures need exact state/audit transitions before implementation; directly editing authority records would violate RFC 0048's history contract.

## Goals

- Add/remove enrolled owners while retaining at least one active owner.
- Return current owner membership only to an authorized member or scoped administrator.
- Transfer ownership with explicit intermediate co-ownership and recoverable request identities.
- Persist credential revocation under a separate local administrative capability and prove fresh refusal of old ordinary requests.

## Non-goals

- An atomic transfer action, public principal enrollment, credential minting or a network credential-administration endpoint.
- Principal deactivation, later provisioning, administrative owner recovery or quarantine workflows: [RFC 0054](0054-registry-quarantine-transactions.md) and [RFC 0055](0055-registry-local-administration.md) elaborate these separately in E07/E08 before E04's combined audit. Normal two-step ownership transfer remains unchanged.
- Signed provenance, production consumption, source-language changes, power-loss availability or malicious-operator authentication.

## Guide-level design

Owner A adds active enrolled principal B at package revision R. This creates R+1 with both owners. A or B then removes A at R+1, creating R+2 with B. Each action has a different request ID. If the second step fails, both owners remain; transfer is not an atomic replacement. Existing name-bound grants still constrain B's operations; ownership does not mint credentials or grants.

The successful self-removal returns its committed receipt. A later retry by A is denied because A is no longer an owner, even if that exact ID previously succeeded. After a lost self-removal reply, an authorized remaining owner or scoped administrator reads the current membership to confirm the result. Never preserve removed authority merely to return a cached receipt. An actor who remains authorized may retry an old successful action and receive its original result without reapplying it.

A local operator explicitly releases the service/store writer, opens its private directory through the administrative host API, and revokes a credential with an expected state revision and reason. Reopen the service from the new committed state; the old bearer is immediately refused, including retries. A retained writer makes administrative open busy. This bounded initial-profile workflow uses no ambient credential, hidden HTTP route or automatic lock wait. E05 must exercise service stop/revoke/reopen when integrating the live service.

## Reference semantics

### Owner actions and reads

Owner action bodies use RFC 0051's canonical JSON and 2,048-byte ceiling, with ordered fields `schema:1`, `request_id`, `expected_revision`, `action`, `reason`, `principal`. Action is `add_owner` or `remove_owner`; principal is an existing 32-character lowercase-hex ID. Request ID, positive package revision, reason (1–256 UTF-8 bytes, no control characters), exact name and all closed-field/encoding rules retain their existing meanings. Inapplicable `version`, unknown/duplicate fields, nulls and unsupported schemas refuse. Debug/errors do not echo arbitrary reasons or credentials.

The bounded host action is not authority. `Operation::ChangeOwners(name)` authenticates the real bearer against the currently locked view before request reconciliation and at commit. Its principal must be an active current owner with the exact name's `owners` grant. An `admin` grant alone is not ordinary owner-mutation scope. Reconcile the global principal/request-ID map before checking the old conditional revision or target membership; current authorization remains mandatory.

For a new action, require the current package revision. Add requires an active enrolled target not already in the owner set; unavailable/inactive targets return not found, existing membership conflicts, and growth above 128 owners returns limit before insertion. Remove requires existing membership and must leave at least one active owner; absent membership or removal of the last active owner conflicts. An inactive historical member can be removed while an active owner remains. Keep IDs sorted and unique. These checks are transition preconditions, not substitutes for caller authentication.

The candidate increments state and affected package revisions once, changes only that package's authority owner set, and advances the time floors/audit link. Preserve all releases, source bytes, other packages, principals, token records and grants. Apply the derived-index bound even though only its revision changes. Use the same preflight quota reservation, staged/synced audit/state, fresh event time, final fresh authorization and atomic HEAD replacement as E02. Retain the structured not-published/unknown outcome and complete prior history.

An owner read uses `Operation::ReadOwners(name)`: an active current member needs no mutation grant; a nonmember needs the exact name's `admin` grant. Capture one complete view, authenticate at fresh time, encode, and check fresh time/authorization again before returning. The canonical response has ordered fields `{schema:1, registry, name, revision, owners}`, sorted IDs and an 8 KiB ceiling. It does not mutate revisions/floors or grant authority to its recipient. Public package indexes never expose owners. E05 supplies the already specified authenticated GET route.

### Owner audit and replay

Add schema-1 operations `{kind:"add_owner", name, principal}` and `{kind:"remove_owner", name, principal}`. `principal` inside the operation is the affected member; the common audit principal remains the authenticated actor. Their result has ordered fields `{state_revision, package_revision, name, principal, member}`, where member is respectively true or false. Common credential/request/prior-hash fields are non-null; reason is the admitted action reason. The host receipt remains `{schema:1, audit_sha256, result}`.

Reconstruct the canonical action using recorded request ID, prior package revision, operation, reason and affected principal. Its request digest uses `POST`, `/v1/packages/NAME/actions`, that prior revision, and complete canonical body hash/length. Validate the recorded actor against prior authority at event time, reconstruct the complete expected state and result, and compare both. Owner changes cannot hide credential, release, unrelated ownership or quarantine changes. A reconstructed no-op, last-owner removal or duplicate request identity refuses even when every hash was recomputed.

### Separate local administrative capability

Expose an `Administrator` host wrapper whose construction requires the explicit parent directory capability, bounded private child name, independently configured service identity and configured operator ID. It exclusively opens the same registry through the existing no-follow/regular-file/history boundary. It privately owns that registry and operator binding; provide no constructor from a bearer, owner list, receipt, network body or boolean, no public conversion from an ordinary `Registry`, and no dereference exposing its internals. Ordinary authenticated mutation APIs cannot obtain this capability.

The operator ID is audit attribution selected by trusted host configuration, not a credential or caller-provided authority. Require a well-formed, currently enrolled active operator at admission and final commit. The granted private directory controls local administrative access; a name-scoped bearer `admin` grant is not a global credential-revocation capability. The host/operator remains trusted, as in bootstrap. This does not cryptographically identify which human used the local capability.

The wrapper can read the current validated state revision and revoke one existing credential. Revoke does not enroll a principal, create a token, change grants or recover ownership. Expired credentials or credentials of inactive principals may still be revoked. Revoking an operator's own bearer does not revoke the separately held local directory capability. Ordinary requests using that bearer nevertheless fail authentication immediately against the new state.

### Local revocation request and transition

The canonical bounded local request has ordered fields `schema:1`, `request_id`, `expected_state_revision`, `action:"revoke_credential"`, `reason`, `credential_id`. Use the same 2,048-byte, closed JSON, ID and reason rules; expected state revision is positive and distinct from package revision. The canonical request-digest fields remain RFC 0046's `{schema, method, path, conditional_revision, body_sha256, body_bytes}`, but method is exactly `LOCAL`, path `/v1/admin/credentials/ID/revoke`, and the condition is expected state revision. This is an internal domain separator, not an HTTP method or supported network route. A request body cannot confer the local capability.

After current operator/time validation, reconcile the same global principal/request-ID map. Equal digest returns the original receipt after another current operator/time check, even though its condition is stale; different digest conflicts. For a new ID, require the exact current state revision and an existing unrevoked credential. Missing target is not found; an already-revoked target or stale condition conflicts. No record or identity is deleted or reused.

Change only the target token's revoked flag from false to true, state revision, state/authority time floors and audit link. Package revisions, owners, release/index bytes, enrolled principals and every other token field remain unchanged. Reserve all peak quotas before writes, resample event time, stage/sync/install audit and state, then recheck active configured operator and fresh time immediately before the existing HEAD replacement. Final time cannot regress below event time. The administrative read/retry never clears a floor. Unknown outcomes require the same explicit recovery/reconciliation contract as existing mutations.

Add operation `{kind:"revoke_credential", credential_id}` and result `{state_revision, credential_id}`. The common audit principal is the configured active operator; common `credential_id` is null because this is local capability administration, not bearer authentication. Request ID/digest and prior hashes are non-null; reason is the admitted reason. History validates prior active operator attribution, target existence/unrevoked state, reconstructed LOCAL request digest and the entire exact transition/result. Null credential fields remain forbidden for ordinary noninitial operations. The result state revision equals event sequence. A host receipt or hash chain is not a package signature.

## Execution modes

Owner and credential state change who may operate the registry, not source execution. The E03 fixture transfers a package to B, refuses A's retained requests, publishes through B, and executes the same retained source in VM/native. A separate actual local revocation then rejects B's old bearer while leaving package revisions and exact archives unchanged. No MIR, VM, native, JIT, component or embedding semantics change.

## ABI and compatibility

Affected domains are package and tooling. Authority/state fields and source/archive/lock/ABI formats stay unchanged. Private schema-1 audit/receipt gains closed variants; older readers must reject histories containing them. Upgrade the host reader before the first operation; do not downgrade by pruning events, resetting HEAD, restoring a revoked credential or inventing an owner. Unsupported variants remain refusals. HTTP owner actions retain RFC 0046; local revocation adds no network endpoint.

## Diagnostics and tooling

Use existing REG0001 malformed, REG0002 limit, REG0003 authentication, REG0004 authorization, REG0005 conflict, REG0006 integrity, REG0007 state and REG0008 I/O/busy/not-found categories and the existing HTTP mapping where applicable. Malformed operator configuration is refused; a missing/inactive configured operator has no administrative authority. Diagnostics are bounded and static. No source formatter, language server, parser or debugger changes are needed.

## Performance and cost model

Each mutation retains one complete state and audit, but adds no archive. Owner sets remain bounded at 128, principals/tokens at 4,096, all records/entries/history at their existing limits. Retrying adds no event. Full-history validation and encoding retain the initial profile's costs; no throughput/latency claim is made. The explicit stop/reopen administrative workflow avoids a new shared-lock, queue or RPC authority system; live-service availability and workspace accounting remain E05 concerns.

## Security and trust boundaries

Current bearer possession, active membership and exact grants govern ordinary actions and private reads. Active local operator attribution plus an explicitly granted directory capability governs revocation. Neither a stale actor nor a successful receipt bypasses current checks. Stop/reopen releases a held writer before local administration; contention stays bounded busy. Last-active-owner checks do not promise that an owner has a live credential: emergency revocation may intentionally remove the last network credential, and later provisioning must be an explicit audited administrative operation. Persisted reasons must not contain secrets, source text or host paths. Host compromise/rollback remains an accepted residual risk.

## Alternatives

- **No change:** leaves real transfer and revocation fixtures impossible without unaudited authority edits.
- **External identity/registry service:** may manage accounts but does not define this registry's atomic ownership, retained request identity and local history semantics.
- **Atomic transfer variant:** avoids intermediate co-ownership, but adds a new action/compatibility contract absent from RFC 0046. Explicit add/remove reuses its accepted operations and exposes partial completion.
- **Public/global admin bearer endpoint:** enables remote administration but broadens the credential audience and transport surface. An explicit local capability matches the initial profile and cannot be inferred from a package grant.
- **Cached success after self-removal/revocation:** simplifies retry responses but revives authority. Require current authority and let remaining authorized actors inspect the committed outcome.
- **Unrecorded token-file rewrite:** is small but breaks single-HEAD audit/owner/index agreement and recovery validation.

## Implementation plan

1. Prepare/accept this contract while E02's source/head remain frozen and E02 remains the sole doing delivery. E03 stays todo at its existing 3 pt pending a remaining-task audit at selection.
2. Implement owner action/read admission and complete state/audit/replay transitions. Reach the actual transfer and both-mode source fixture before unrelated administration.
3. Implement separately constructed local administration and one audited revocation, then prove actual current-bearer refusal and unchanged package/source state. Broader administrative workflows stay E04; HTTPS dispatch stays E05.
4. Run focused boundary/forgery/process/concurrency fixtures, the full baseline and latest-head both-host CI; publish evidence and audit the remaining G5 work. Do not mark G5 complete with E03 alone.

Branch preparation: `codex/m5-registry-owner-control` starts from `codex/m5-registry-mutations` at `f40d643` in `carimatics/lang`; PR #39 is its dependency and recorded initial origin PR base. Original work-unit branch remains `main`; the new checkout had no pre-existing changes. Retarget only after #39 merges.

## Test plan

- Actual add/remove/transfer, private owner read, no-op/stale conditions, unknown/inactive targets, sorted 128-owner boundary and last active owner versus inactive historical members.
- Exact original-result retry while authorized; denial after self-removal/credential revocation; cross-operation request-ID collisions and changed reason/target/body.
- Real B publication and VM/native source execution after transfer; unchanged archives/package revision across credential-only revocation.
- Local capability construction, held-writer busy, wrong service/operator, ordinary admin bearer versus local authority, fresh final time and explicit recovery of unknown outcomes.
- Canonical 2 KiB action/8 KiB owner response bounds, independent digest vectors, closed fields, hostile input, peak quotas and derived-index revision growth.
- Full transition forgeries with recomputed hashes, including hidden release/grant changes, unauthorized operator claims, no-op owners/revocation and duplicate request keys.
- Actual process cuts and competing writers on both hosts, preserving complete old/new state, last active owner and every committed audit.

## Open questions

Implementation checkpoint, verified 2026-10-31 JST: 42 store tests pass, including actual owner transfer and B publication, unchanged-archive VM/native execution, local credential revocation, denied former-owner/revoked retries, 128-owner capacity, 72 real process cuts (68 old/four new), both owner/admin writer orders, 18 recomputed-hash forgeries, final time and explicit unknown-result reconciliation. Owner/revocation admission agrees with both independent vectors and survives 1,024 bounded input mutations. A synthetic valid-state preflight independently proves package revision 99 → 100 can exceed an exact 2 MiB index by one byte; it is not claimed as 68 actual extra commits. The earlier pure-model acceptance below is historical. Full baseline, latest-head both-host CI and publication remain required.

Accepted under RFC 0001 experimental delegation on 2026-10-31 JST. The pure `/private/tmp/lang-m5-owner-policy-probe.js` passes 24 cases for two-step transfer, current-authority retry, last active owner, 128-owner capacity, unchanged archive/package revision and separately modeled local revocation. Independent JavaScript/Python encodings agree: owner body 205 bytes, SHA-256 `682954cddbf3597314b8c2f3dfee606ca6a9aec1d7aa9ea9732872fb68f6a5bf`, request digest `5ab32ddd58e4c0b92843e59999d01f63f1736b5b75a3805542f3a8014034168b`; revocation body 225 bytes, SHA-256 `e026380b51d06ee10382aefed48690b5ad6059963248b7cd5d463daec12bce07`, request digest `0b043b277a723f8e2edea2b4cdf598b11a66baee9243b46791632f3a532e23c1`.

Rationale: explicit current-authority checks, a separate local capability and exact full transitions fit the accepted registry model without a second authority file or atomic-transfer wire variant. The model is policy/encoding evidence only; it proves no real authentication, directory capability, process interruption, Windows behavior or service availability. Those remain implementation obligations. E03's existing estimate must be re-audited before implementation if its observable outcome grows; no gate is closed by this acceptance.

## Sources

- [RFC 0046](0046-registry-protocol-and-recovery.md), updated 2026-10-31: owner actions, local administration, request identity and quotas.
- [RFC 0047](0047-registry-authority-records.md), updated 2026-10-31: current owner/read/admin policy, active owners and revocation.
- [RFC 0048](0048-registry-state-and-publication.md), updated 2026-10-31: single-HEAD generations, full transitions and retained audit/replay.
- [RFC 0051](0051-registry-yank-transactions.md), updated 2026-10-31: closed action/receipt variants and latest host transaction pattern.
- Local 24-case policy/independent-encoding experiment above, run 2026-10-31 JST; project decision evidence, not a security or performance measurement.
