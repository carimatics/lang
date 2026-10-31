<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0046-registry-protocol-and-recovery.md -->

# RFC 0046: Registry protocol, admission and recovery

[日本語版](../ja/rfcs/0046-registry-protocol-and-recovery.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-C02, D01–D05, E01–E04
- Supersedes: none
- Superseded by: none

## Summary

Specify a bounded registry profile for portable source archives, immutable publication, exact-origin download, scoped mutation authority and process-crash recovery. RFC 0045 supplies the threat model; this RFC maps it to wire/state rules and observable security cases before uploads. Package signing/root succession remains the separately gated F01 contract: unsigned development fixtures are never an authenticated production consumer profile.

## Motivation

The current source snapshot includes an entire target/compiler-specific local graph. A registry release needs one portable package, declared exact dependency identities, a hostile-input reader and a transaction whose publication, ownership and audit agree after interruption. Reusing arbitrary TAR extraction, a mutable index file or a checksum-only download does not supply that boundary.

## Goals

- Admit a finite canonical source archive without filesystem access or guest execution, then materialize through a separate directory capability.
- Make origin, package, version and digest explicit from request through replay.
- Commit immutable release identity, current authorization and audit as one visible state transition.
- Bound requests, parsing, storage and recovery; retain failures without silently resetting state.

## Non-goals

Compression, arbitrary TAR compatibility, package scripts, version ranges, implicit credential discovery, arbitrary download URLs, HTTP redirects, distributed writers, public account signup, production key provisioning and power-loss availability guarantees. This RFC does not claim TUF/OAuth/SLSA conformance. F01 must accept signing/provenance/freshness formats before authenticated package consumption; I01 must accept documentation/search output rules before hosting. Manifest/lock integration has the explicit preimplementation elaboration boundary below.

## Guide-level design

The initial observable journey publishes `answer` at `0.1.0`, explicitly downloads its exact digest from the configured registry, and executes a dependent source to the same value in VM and native modes. The same package/version with other bytes is a conflict. Publishing from a local package previews the derived portable descriptor and exact source inventory; it never silently converts a private path dependency into a public dependency. A saved preview is not publication authority.

Client operations name an explicit registry configuration and cache directory capability. Publish and owner/yank operations additionally consume a token through a protected explicit input such as bounded stdin, never a command argument or package-selected credential provider. The server accepts only its configured origin; test/local directory transports must be explicitly injected and cannot activate an insecure public-client fallback.

## Reference semantics

### Common encoding and identity

All metadata is UTF-8 JSON with closed objects, no duplicate fields, bounded nesting and integers within the stated unsigned bounds. Hashes are lowercase 64-character SHA-256 hex; request/principal identifiers are lowercase 32-character hex. Reject unsupported schema/format, unknown fields, floats in integer fields and trailing non-whitespace. Canonical generated JSON uses the field order listed here, two-space indentation and one final LF; ordered arrays have no duplicates. A canonical-byte requirement is explicit for each hashed stored descriptor/state; ordinary request JSON need not arrive in canonical whitespace.

A registry configuration binds `schema: 1`, `registry`, `origin`, `trust_root_sha256`. Registry names use `[a-z][a-z0-9_]{0,63}`. Origin is canonical `https://HOST[:PORT]`, without path, userinfo, query or fragment. HOST is a lowercase ASCII DNS name or canonical IPv4 address; reject Unicode/IDNA spellings, trailing dot, ambiguous numeric forms and IPv6 in profile 1. PORT is decimal 1–65535 without leading zeroes; omit 443. No redirects or downloaded alternative origins. A configuration cannot bind one registry name to two origins or silently replace its root. Trust roots arrive independently under F01; the configuration digest itself establishes no trust.

Package/alias names, exact versions, edition and two allowed target triples retain RFC 0034. A dependency edge is the ordered object `{alias, registry, name, version, archive_sha256, targets}`. Targets are a sorted nonempty subset of `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`; all supported targets means both entries. Edges sort by alias. Aliases only bind direct dependencies. The same `(registry, name, version)` with two different archive digests is always a conflict, including through a diamond. A consumer must explicitly configure every referenced registry; no package may introduce network authority.

### Portable source archive profile 1

Use uncompressed POSIX USTAR with exactly one `.lang-package.json` first and the descriptor's source files in increasing ASCII path order. The descriptor fields in order are `format: "lang-registry-source"`, `schema: 1`, `package`, `tests`, `dependencies`, `files`. Package fields are `name`, `version`, `edition`, `entry`. Tests retain declared order; dependencies use the edge shape above. Each file record is `{path, bytes, sha256}`. Entry/tests name listed `.lang` files. There are no nested manifests, workspaces, locks, binaries, docs assets or implicit dependency archives. Source bytes must be UTF-8 and remain original, including comments, Unicode and CRLF; validation occurs during byte admission without compiling source. Source compilation is not an upload-side effect.

The publisher derives the descriptor from an explicitly frozen input snapshot and explicitly supplied registry dependency bindings. It refuses unresolved local-path dependencies. Preview lists every generated metadata field and source file/hash. It does not copy the original local `Lang.json` or `Lang.lock` into the release, rewrite them, or claim the resulting descriptor is their original byte identity. RFC 0035 snapshot export remains unchanged and cannot be accepted as this format.

File paths are ASCII slash paths of 1–100 bytes, with nonempty components containing only letters, digits, dot, underscore and hyphen. Reject absolute/backslash/colon paths, `.`/`..`, trailing dots, Windows reserved device stems (case-insensitive, before the first dot), case-fold duplicates and file/directory prefix collisions. Reserve `.lang-package.json` and descendants; refuse `.git`, `.codex` and `target` as first components, case-insensitively. All ordinary files end in `.lang`. The 100-byte bound deliberately avoids long-name extensions; source paths supported by local packages may therefore be refused for publication with a precise path diagnostic.

D01 clarification, 2026-10-31 JST: case-collision checks include implicit directory prefixes, so `src/a.lang` and `Src/b.lang` cannot introduce different source trees on case-sensitive and case-insensitive hosts. Repeated identical directory spellings are allowed; a file cannot also be a directory prefix. This makes the existing portable no-alias requirement explicit without changing a released format; profile 1 is not yet implemented.

Headers have name in the 100-byte name field, zero prefix/linkname/owner/group fields, type `0`, mode 0644, uid/gid/mtime zero, magic `ustar\0`, version `00`, device major/minor zero and zero unused bytes. Numeric fields use zero-padded ASCII octal and a terminal NUL; checksum uses six octal digits, NUL and space, calculated with eight spaces in the checksum field. Payload padding is zero. Exactly two zero blocks terminate the stream; refuse extra bytes, concatenated archives, base-256 numbers, directory/link/device entries, PAX/GNU extensions, sparse files and compression. Re-encoding an admitted archive must equal its original bytes. Descriptor JSON must be canonical, file sizes/hashes exact and every listed file present exactly once. A hash identifies the entire encoded archive, not only extracted files.

Read at most 18 MiB plus one detection byte. Limit descriptor to 128 KiB, 256 source files, 1 MiB per source, 16 MiB source total, 128 dependencies and 128 tests; JSON depth at most 16. Charge headers, payload/padding and decoded records before allocating. The byte-only admission API returns a private verified immutable inventory; a separate materializer takes explicit directory authority, exclusively creates a fresh destination, writes only verified regular bytes and cleans its recognized private staging on failure. Never call a generic extractor on foreign input. Refuse source/destination aliases and nonregular files at actual open/publication boundaries on both hosts.

### HTTP profile and records

Use certificate-verified HTTPS and HTTP/1.1 with one request per connection and `Connection: close`. No redirects, content encoding, automatic decompression, chunked transfer or `Transfer-Encoding`. Require one valid Content-Length for every body and read exactly that many bytes within its bound. The server must not wait for client EOF before responding; it never processes a second/pipelined request. The consumer refuses truncated responses and excess body bytes. Reject duplicate framing/authorization/conditional/request-ID headers, inconsistent Host, unsupported methods and unknown routes. Bound request line to 2 KiB, all headers to 8 KiB/32 fields, handshake/connect to 10 seconds and complete request/response to 120 seconds. Use a maintained HTTP/TLS implementation with explicit parsing limits; do not implement an ad hoc permissive HTTP parser. Server concurrency is at most eight connections including handshakes, with no unbounded accept queue in application state and a 256 MiB aggregate admitted-request memory budget. Excess work is refused/closed before body admission.

All paths are origin-rooted. Names/versions/digests are already restricted and are not percent-decoded into alternate identities. No endpoint returns or consumes arbitrary filesystem paths or remote URLs.

| Method and path | Contract |
| --- | --- |
| `GET /v1/packages/NAME` | Public bounded package index, including yank/quarantine flags; an absent package returns a stable refusal. |
| `GET /v1/objects/SHA256` | Public immutable archive referenced by a committed non-quarantined release, including exact yanked releases; exact bytes/length/digest checked. Never expose staging or uncommitted objects. |
| `GET /v1/packages/NAME/owners` | Authenticated current member or scoped administrator; returns `{schema:1, registry, name, revision, owners}` with sorted principal IDs. |
| `PUT /v1/packages/NAME/VERSION` | Authenticated archive body, `Content-Type: application/x-tar`, 32-hex `Idempotency-Key`. Existing name requires `If-Match` equal to its quoted decimal revision; initial claim requires `If-None-Match: *` plus both claim and publish scope. No replace operation. |
| `POST /v1/packages/NAME/actions` | Authenticated JSON action with required schema, request ID, expected package revision, action-specific fields and reason. |

Index fields are `format: "lang-registry-index"`, `schema: 1`, `registry`, `name`, `revision`, `releases`. Releases sort by numeric version tuple and contain `version`, `archive_sha256`, `archive_bytes`, `descriptor_sha256`, `edition`, `dependencies`, `yanked`, `quarantined`. They are derived from one committed state and bounded to 2 MiB. Index identity/digests must match the separately authenticated F01 release/selection metadata before a production consumer trusts them. HTTPS alone is not that authentication. Unknown signing profile or missing evidence refuses consumption; D-stage isolated unsigned fixtures cannot be silently selected by production CLI.

Action objects share `schema: 1`, `request_id`, `expected_revision`, `action`, `reason` (1–256 UTF-8 bytes, no control characters). `yank`/`unyank` additionally require `version`; `add_owner`/`remove_owner` require `principal`; `quarantine`/`unquarantine` require `version` and administrator scope. Variant-inapplicable fields are refused. Owner operations cannot remove the final active owner and target an already enrolled stable principal. Administrative owner recovery requires a separately scoped, audited local administrative transaction with a reason; it cannot overwrite release bytes or reset history. No public endpoint enrolls principals or mints credentials.

Successful mutations return canonical `{schema:1, registry, request_id, revision, audit_sequence, outcome}`; outcome is `published`, `yanked`, `unyanked`, `owner_added`, `owner_removed`, `quarantined` or `unquarantined`. A reconciled retry returns the original committed result without another revision/event. Publish first returns 201 and replay 200; actions return 200. Public GET success is 200. Refusals use 400 malformed, 401 generic authentication, 403 authorization, 404 absent/unserved resource, 409 conflict/recovery-required, 413 limit, 429 busy and 500 I/O failure. Error bodies are at most 16 KiB: `{schema:1, code, message, request_id}` with request_id null unless safely admitted. Diagnostic mapping below defines code classes; never echo credentials or arbitrary input bodies.

### Authentication and mutable authority

Use an opaque externally provisioned uniformly random 32-byte token encoded as 64 lowercase hex characters in a single `Authorization: Bearer TOKEN` header. Only mutation and owner endpoints consume it; public GETs omit/reject Authorization rather than forwarding it. Server persistence stores only `SHA256("LANGREGTOKEN1\0" || token_bytes)`, not the bearer. Compare authenticators without secret-dependent equality shortcuts. The explicit client input permits exactly 64 hex characters plus an optional final LF and is bounded before decoding. Test tokens are fixture-only, never production credentials.

A token record contains stable principal, registry audience, issued/expires timestamps, revoked flag and explicit `(action, package-name)` grants; actions are claim/publish/yank/owners/admin. Token lifetime is at most seven days, with at most 128 grants. Tokens never convey ownership by themselves. Existing-package mutation requires current owner membership and the corresponding grant; administrative quarantine/recovery uses separate admin scope. New-name publication requires explicit name-bound claim and publish grants and installs the publishing principal as initial owner atomically. Server configuration enrolls principals/provisions or revokes token records through explicit local administrative capability; these changes are serialized and audited too. Trusted administration is not an unauthenticated HTTP escape hatch.

Times are integral Unix seconds in 0..=253402300799. Read host time at admission and again before commit; require issued <= now < expires and now >= persisted time floor. Refuse clock regression, revoked tokens and changed membership. Body parsing/staging does not preserve an earlier authorization decision. Revocation, owner mutation and publish all use the same nonblocking exclusive registry lock at final admission; contention is busy, not an unbounded wait. A stolen still-valid correctly scoped token remains a residual risk until revocation; TLS, short lifetime and narrow grants reduce exposure without claiming to eliminate it.

### Immutable transaction, audit and recovery

The storage root is an explicitly granted private directory, not an archive path. Initialization exclusively creates a fresh root and audited bootstrap state; it never resets an existing root. Only one writer holds its regular nonblocking exclusive lock. Limits are 1,024 package names, 128 versions/name, 8,192 releases total, 4,096 principals/tokens each, 128 owners/name, 65,536 committed audit events, 16 MiB current state and 4 GiB total managed storage including staging/history. Bound managed entries and each recovery enumeration to 262,144; unknown entries cause refusal rather than deletion or unbounded traversal. Consumer cache storage has the same 4 GiB/262,144-entry ceilings, independently from graph limits. All arithmetic is checked. Reaching any limit refuses before mutation; reads continue. Quotas are profile-1 bounds, not a claim of internet-scale capacity. No silent audit pruning, automatic identity reuse or quota-triggered trust reset exists.

Persist content-addressed objects, immutable canonical state generations and audit records under private fixed namespaces. A small canonical HEAD contains only `{schema:1, state_sha256}`. A state contains registry identity, monotonic revision/time floor, package/version/owner/quarantine records, enrolled principals/token hashes and scopes, and the last audit sequence/hash. No stored value becomes a host path. [RFC 0047](0047-registry-authority-records.md) elaborates the closed authority records before E01; complete generation/state and committed audit link encoding must be accepted before D02. This sequencing clarification, accepted 2026-10-31 JST, permits pure policy implementation without inventing an independently writable authority file. Required atomicity, bounds and authority remain fixed here. Private versioned formats do not exempt strict admission.

An audit event records sequence, previous committed event hash, prior state hash, admitted time, principal, nonsecret credential identifier, operation, request ID and request digest, affected identity, reason and committed result/revision. It is at most 2 KiB; credential secrets, source bytes and filesystem paths are absent. Every state change, including token provisioning/revocation and administrative recovery, has an event. Its hash chain detects corruption relative to a retained head, not malicious-operator erasure; exportable verified receipts/checkpoints remain F01/E04 work.

While holding the lock: reload and validate committed HEAD/state/audit, reauthorize, reconcile request ID, check expected revision and quotas, verify complete archive, then write and sync all new immutable objects and event/state files. Atomically replace HEAD on the same filesystem only after all referenced bytes are complete. That replacement is the single visibility point for index, ownership and audit. Readers observe one captured committed state, never an independent mutable index. Return success after HEAD publication; a late response failure does not roll it back. No version identity is deleted or replaced; yank/quarantine/owners change state only.

Request IDs are unique per principal over retained audit history. The request digest is SHA-256 of canonical ordered JSON `{schema:1, method, path, conditional_revision, body_sha256, body_bytes}`: path is the canonical origin-rooted request path, conditional_revision is the integer expected revision or null for initial claim, and body identity covers canonical action JSON or exact archive bytes. Check current authority first. A known ID with the same digest returns its original result even if its old conditional revision is now stale; a different digest conflicts. An unknown ID with a stale revision conflicts. A revoked/removed caller cannot bypass current authorization by replaying an old success. Do not infer uncommitted state from a connection failure; clients retry/reconcile the same request identity.

Process interruption before HEAD replacement leaves the old committed state; interruption after it leaves the new complete state. Recovery locks, validates HEAD plus every reachable bounded record/object and deletes only recognized unreferenced staging/generations. It never adopts the highest uncommitted generation, regenerates a missing HEAD, rolls back to an older state or clears a floor. Corrupt/missing committed data requires explicit verified operator restoration; ordinary operations fail closed. File sync plus atomic replacement is required, but cross-platform power-loss durability/availability is not promised: torn or missing state after power loss must be detected and refused. D02 must validate platform replacement behavior, including Windows, before relying on it.

### Consumer, offline and integration boundary

Download only configured origins with the same framing/timeout bounds; bind response length/hash to the requested immutable identity, and verify F01 trust/freshness before making a generation consumable. Cache identity includes registry/root policy and archive digest; verify bytes on reuse. A fresh destination is published only after complete bounded admission. A caller must grant network and cache-write authority explicitly; `--offline` performs no DNS/network/credential lookup.

New resolution excludes yanked/quarantined versions. Exact locked replay may use a previously authenticated non-quarantined release even after a later yank, under F01's receipt/expiry/revocation policy; quarantine refuses new service and newly observed quarantined replay, without claiming to erase offline copies. Unknown or expired trust needed for new admission cannot be bypassed by offline mode. Missing local bytes fail rather than selecting a different package/version/origin. Retain RFC 0034 graph bounds (256 packages/modules, 16 MiB source, 4,096 edges) and account repeated visits; forbid dependency cycles.

D04 must elaborate the manifest schema and lock representation in this RFC or a follow-up before implementation: preserve local schema-1/2 lock behavior, add an explicitly versioned registry-edge form, pin registry/root/target/toolchain/source identity and keep exact canonical replay. The above archive/edge protocol is fixed; a lock must not reconstruct registry identity from an arbitrary cache path. This is part of D04's planned integration work, not an already implemented package feature or implicit schema extension.

## Execution modes

Neither upload, download, recovery nor index construction executes source, native tools or package hooks. The frozen admitted source and explicit graph feed the existing frontend and typed MIR; VM/native conformance remains required. Registry credentials/configuration never become guest capabilities. Wasm components and native linking keep their separate accepted ABI and trust rules.

## ABI and compatibility

Add independently versioned registry wire/archive formats. Existing RFC 0034 manifests/locks and RFC 0035 snapshots retain their original meaning and cannot be guessed as registry formats. The initial public archive deliberately narrows path length and input kinds; refusal is explicit and never silently renames source files. Source edition, bytecode, native ABI and component C v1 are unchanged. Future encoding/limits changes require explicit profile review and migration rules; F01 signing and D04 integration elaborations must precede their code.

## Diagnostics and tooling

Reserve `REG0001` malformed/unsupported profile, `REG0002` resource limit, `REG0003` authentication, `REG0004` authorization, `REG0005` identity/version/revision conflict, `REG0006` integrity/trust/freshness failure, `REG0007` persistent-state/recovery failure, `REG0008` busy/I/O/transport failure. Use existing CLI0001 for malformed command options and existing source diagnostics for guest source. Structured failures include bounded stage/identity context without tokens; a failed operation must distinguish definitely uncommitted from unknown-after-publication. Human output directs an unknown result to reconciliation, not blind creation of a new request ID. Saved JSON is never a capability or executable plan.

## Performance and cost model

The strict archive bounds allow bounded memory-first validation; materialization copies verified bytes once into staging. Full state validation/history lookup is bounded but may be linear and expensive; no throughput claim is made. Charge aggregate active bodies, staging, historical generations and audit before accepting work. Recovery cannot scan arbitrary ambient directories. Measurements may justify a later indexed state format without weakening the transaction or audit contract.

## Security and trust boundaries

| Case | RFC 0045 threats | Executable acceptance |
| --- | --- | --- |
| P01 | R01,R07,R14 | Two configured origins publish the same name/version; an exact dependency never crosses origin or digest, including a diamond and offline replay. Private path dependencies refuse public encoding. |
| P02 | R04,R05 | Reject every unsupported header/extension, traversal/link/device/case/prefix alias, size/hash mismatch, malformed descriptor, excess padding/trailing bytes and limit boundary before destination writes. |
| P03 | R02,R03 | Wrong audience/action/package, expired/revoked token, owner removal during upload, stale owner revision and last-owner deletion cannot commit. Authentication errors are indistinguishable for unknown token IDs. |
| P04 | R06,R11,R12 | Race distinct bodies at one version; exactly one commit/event. Inject interruption at each staging/HEAD/response boundary; retry keeps one immutable identity and one event. |
| P05 | R07,R08,R09 | Tampered object/index/state, signed rollback/expiry, root substitution and clock regression refuse. F01 adds concrete key/root fixtures before signed-consumption implementation. |
| P06 | R05,R13 | Reject redirect, off-origin/metadata URL, malformed framing, overlong/slow bodies and excess concurrent work; public reads never transmit publish credentials. |
| P07 | R10,R14,R15 | Upload/recovery causes no guest or provider execution or ambient file reads; I01 adds safe rendering/query cases before hosting. |
| P08 | R11,R12,R16 | Disk-full, corrupt HEAD/audit, full quota and unauthorized admin action preserve committed identity/floors or refuse recovery. No automatic rollback/reset/erasure. |

A trusted local administrator can still read server state or destroy service, and a malicious authorized publisher can publish malicious new source. TLS credentials do not establish package-signing trust. These are explicit RFC 0045 residual risks, not claims solved by a checksum or audit chain.

## Alternatives

- No change/local snapshots cannot satisfy registry download and authenticated mutation.
- A generic object store plus external authentication can host bytes, but the package/owner/audit commit still needs a single reviewed authority; a deployment adapter cannot bypass it.
- Cargo-compatible compressed crates and index conventions offer ecosystem reuse but import different manifest/resolution/archive assumptions. The initial profile chooses exact declared edges and a small strict source archive; measured usability can motivate a reviewed extension.
- A distributed database transaction would scale beyond one writer, but adds deployment/recovery dependencies before the first dependency execution fixture. The bounded single-writer profile gives one reviewable commit point without claiming scale.
- Mutable files with best-effort audit are simpler but permit version replacement and split success/history after crash; rejected.

## Implementation plan

Sequencing clarification, 2026-10-31 JST: implement the existing E01 authentication and scoped-authorization delivery before D02 publication. D02 must invoke that policy using its freshly locked state before committing; byte admission or an earlier authentication result cannot authorize a later mutation by itself. This exposes an existing dependency, adds no delivery or estimate and leaves E02–E04's operation/audit integration intact. D01 remains the active implementation while its full baseline and CI finish.

Review C02 before uploads. D01 implements byte-only archive admission and refusal corpus; D02 implements the elaborated immutable state transaction; D03 verifies index/download identity; D04 elaborates package integration then executes a real dependency on both hosts; D05 adds offline/race/crash journeys. E01–E04 implement token/owner/audit operations under the same transaction. F01 precedes signed production consumption; I01 precedes hosting/search. After two prerequisite deliveries without a new execution fixture, re-audit the sequence under the board's existing rule. No new implementation delivery is added by this document.

## Test plan

Retain P01–P08 inputs and state/side-effect observations with exact tested commits. Archive tests use actual encoded bytes and re-encoding equality; malformed corpus and fuzzing cover header/JSON integer, nesting and aggregate-accounting boundaries. Fault injection must address real publication boundaries and separately check process-crash recovery. TLS/framing tests use actual loopback connections with fixture certificates on macOS/Windows. Test exact token/audience/expiry/revocation and concurrent owner/version mutations, not only mocked authorization booleans. A complete real dependency must return the same result in VM/native and locked offline replay. No full compiler baseline is substituted for these domain tests; ordinary baseline/CI still apply to code changes.

## Open questions

Accepted under RFC 0001's experimental delegation on 2026-10-31 JST. Rationale: byte-only strict admission, explicit identity and current authority, one visible state/audit commit, bounded recovery and mapped refusals make the initial registry implementable without widening compiler execution authority. Review corrected an HTTP deadlock risk (waiting for client EOF), made quarantine object refusal explicit and bounded recovery enumeration/cache storage. All R01–R16 threats map to P01–P08 in both languages.

The [canonical archive experiment](../evidence/2026-10-31-m5-registry-archive-profile.json) constructed 3,072 bytes from the specified headers and a CRLF scalar source. The installed independent TAR reader listed both records and recovered the exact original source (both exits 0); this is local format feasibility only, not an implemented hostile-input decoder or both-host conformance. A conservative 17,172,223-byte upper bound for all admitted records fits the 18,874,368-byte encoded ceiling. No performance claim follows.

No blocking C02 design question remains. RFC 0047 accepts E01 authority records; complete state/audit encoding remains a D02 elaboration, manifest/lock encoding D04, signing/root/freshness F01, hosting I01; each must be accepted before its implementation. Limits are conservative project choices without production capacity/ergonomics measurements. TLS parser integration, atomic replacement on both hosts and actual hostile-input/cancellation behavior remain unverified until their deliveries. No public service or production root is established; accepted does not mean implemented or stabilized.

## Sources

- [RFC 0045](0045-registry-threat-model.md), updated 2026-10-31; [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md) and [RFC 0035](0035-source-package-snapshots.md), updated 2026-10-29; [RFC 0042](0042-signed-toolchain-installation.md), updated 2026-10-30. Verified 2026-10-31 JST. Threat obligations and existing capabilities come from these accepted project contracts; this protocol and its numeric limits are project decisions, not external compliance claims. RFC 0045 records primary TUF/SLSA/OAuth/Cargo sources and their publication/update dates.
