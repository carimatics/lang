<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0048-registry-state-and-publication.md -->

# RFC 0048: Registry generations and immutable publication

[日本語版](../ja/rfcs/0048-registry-state-and-publication.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-D02, M5-E02–E04
- Supersedes: none; elaborates RFCs 0046–0047
- Superseded by: none

## Summary

Define the private state and audit encoding required before immutable publication. One replaced `HEAD` makes the complete release, current ownership and audit visible together. A publishing caller presents the real bearer and exact request identity; the store reloads and validates committed state under its nonblocking exclusive lock before authorizing or replaying it.

## Motivation

D01 admits immutable source bytes and E01 checks bounded authority records, but neither commits a registry release. Independently replacing an index and an owner/audit file can expose an incomplete transaction after interruption. A cached actor can outlive revocation. This proposal joins the already accepted boundaries without adding another commit point.

## Goals

- Publish a new exact `(registry, name, version, archive digest)` once.
- Serialize authorization, expected revision, idempotency and publication under one lock.
- Validate a finite committed history and refuse corruption without selecting an older head.
- Prove old-or-new visibility at actual process-exit boundaries on both supported hosts.

## Non-goals

- HTTP endpoints, production signing, dependency resolution or source execution.
- Power-loss availability, malicious-operator rollback protection or internet-scale throughput.
- Owner/yank/revocation command workflows, which remain E02–E04 integrations.

## Guide-level design

An explicitly granted private registry directory is initialized once with an independently configured service identity and administratively provisioned authority records. Initialization fails on an existing destination. To claim `parser` version `1.0.0`, the caller supplies its admitted archive, bearer, request ID and an absent-name condition. A later version requires the current package revision. Reusing a successful request ID with the same digest returns its original receipt only after current authorization; reusing it for different bytes conflicts. No operation replaces a published version.

## Reference semantics

### Private layout and encoding

The private root contains only regular `LOCK`, regular `HEAD`, and fixed `objects`, `states`, `audits`, `staging` directories. Immutable record filenames are 64 lowercase hex digest characters, without suffixes. Staging names are exactly `object-HASH`, `audit-HASH`, `state-HASH` or `head-HASH`, where HASH is the candidate object's digest, or the referenced state digest for HEAD. Staging files may contain incomplete bytes within their respective bounds; they are never referenced by committed state. Stored strings never become arbitrary paths. All JSON is closed, ordered, integral and canonical under RFC 0046. Refuse symlinks, nonregular files, hardlink aliases, unknown names and unexpected subdirectories. Bounds remain 4 GiB total managed bytes, 262,144 entries and one bounded enumeration per recovery pass; arithmetic is checked before allocation or writes.

`HEAD` is at most 128 bytes and has ordered fields `{schema:1, state_sha256}`. Its referenced state is at most 16 MiB:

| Field | Meaning |
| --- | --- |
| `format`, `schema` | `"lang-registry-state"`, integer `1`. |
| `registry`, `origin` | Must equal independent configured service identity. |
| `revision`, `time_floor` | Positive state revision and last committed trusted time. |
| `authority` | Exact RFC 0047 object; identity and time floor equal the enclosing state. |
| `packages` | Sorted unique name records, at most 1,024, exactly the names in `authority.packages`. |
| `last_audit_sha256` | Digest of the event whose sequence equals this state revision. |

A package record is `{name, revision, releases}`. Package revisions start at 1 and change once per committed package mutation. Releases sort by the numeric exact-version tuple and use RFC 0046's index release fields in their specified order: `version`, `archive_sha256`, `archive_bytes`, `descriptor_sha256`, `edition`, `dependencies`, `yanked`, `quarantined`. Maximum releases are 128/name and 8,192 total. New publications set both flags false. Metadata must exactly match the corresponding admitted archive; a derived index is never an independent writable authority.

An audit record is at most 2 KiB, with ordered fields `format: "lang-registry-audit"`, `schema:1`, `sequence`, `previous_audit_sha256`, `prior_state_sha256`, `time`, `principal`, `credential_id`, `operation`, `request_id`, `request_sha256`, `reason`, `result`. IDs/hashes use RFCs 0046–0047 encodings. Maximum events are 65,536. Sequence is positive. Reason is at most 256 UTF-8 bytes, has no control characters, and contains no secret/source/path material. D02 uses these closed tagged operation/result pairs:

| Operation | Result |
| --- | --- |
| `{kind:"initialize"}` | `{state_revision:1}` |
| `{kind:"publish", name, version, archive_sha256}` | `{state_revision, package_revision, name, version, archive_sha256}` |

Initialization's sequence is 1, previous/prior hashes and credential/request fields are null, and principal is an active administratively selected enrolled ID. Initial authority has no claimed packages. Publication requires non-null previous/prior hashes, credential ID, request ID and request digest; its principal/credential come from fresh E01 authentication. The result state revision equals sequence. D02 emits the fixed reason `initialize` or `publish`, without accepting arbitrary caller text. Later E02–E04 operation variants require explicit RFC elaboration before implementation; older readers reject unknown variants rather than guessing. The receipt returned by D02 is `{schema:1, audit_sha256, result}`; it does not claim a package signature or include a circular reference to the resulting state hash.

E02 elaboration, accepted 2026-10-31 JST: [RFC 0051](0051-registry-yank-transactions.md) specifies closed yank/unyank actions, audit/result variants and complete flag-transition reconstruction. It retains exact archive bytes, fresh authority, original-result replay and the existing single HEAD; older readers refuse the added operation variants. E03/E04 variants still require their own explicit elaboration before implementation.

### History validation and reachability

State → last audit → prior state forms a finite backwards chain. Each state and audit is read by exact digest and canonical encoding, with consistent identity, revisions, package/authority records and nondecreasing time. State time floor equals its event time. A noninitial event must reference the preceding state's last audit; sequences decrease exactly by one while walking backwards. Revision 1 is the unique initialization terminus. Missing/repeated/inconsistent links refuse admission. Validate all reachable releases against exact source archives before trusting the committed store.

Validate the complete transition, not only matching revision fields. A publish event may append exactly one declared release, increment that package's revision (or start it at 1), install the publisher as initial owner only for a newly claimed name, and advance state/authority time floor and the audit link. All other packages, owners, principals, tokens and scopes remain byte-equivalent canonical data. Reconstruct the RFC 0046 request digest from the prior package revision or initial null condition and exact archive identity. The named historical principal/credential must have been active, valid and sufficiently scoped in the preceding state at event time; this validates the stored claim, not possession of the old secret or malicious-operator authenticity. Current requests still require the actual bearer. This prevents an unrecorded authority change from masquerading as a publish event.

**Accepted history choice:** retain all committed state generations, audit events and source objects reachable through this chain. A prior-state hash is a real reachability edge, not an unaudited observation. This costs historical state bytes and may exhaust the 4 GiB quota before the event-count ceiling. It avoids pruning or an implicit trust reset in the initial profile. No committed version or audit is removed automatically.

Complete bounded inventory and committed-chain validation precedes cleanup. Recovery may remove only recognized bounded regular staging files (including partial writes) and complete digest-verified unreferenced immutable candidates left before HEAD publication. Corrupt immutable candidates, aliases or unknown entries refuse cleanup. It never adopts a candidate head, regenerates missing HEAD or rolls back to an older generation. This explicitly elaborates RFC 0046's orphan cleanup to include unpublished objects/audits as well as states; the paired RFC revision records that acceptance before code relies on deletion.

### Transaction and replay

1. Open the granted root with no-follow/regular-file checks and acquire `LOCK` nonblockingly. Do not inherit a retained lock through a child process; explicitly unlock on release as the existing installer does.
2. Reload `HEAD`, validate the complete bounded committed view and select fresh trusted host time. Refuse regression below its floor.
3. Reauthenticate the actual bearer and authorize the requested name operation against that current authority. An absent name selects claim+publish; an existing name selects publish. A retained audit actor is never an input authority.
4. Reconcile the principal/request-ID pair across retained audit history using RFC 0046's canonical request digest. Equal digest returns the original receipt; different digest conflicts. Current authority is required even for a known successful ID.
5. For a new request, check expected package revision, immutable version identity, archive/request/service correspondence and every peak count/byte quota. Frozen `AdmittedArchive` supplies validated bytes; never accept caller replacement buffers beside its proof.
6. Create each new candidate in private staging, write all bytes and sync its file before moving it into the immutable namespace on the same filesystem. Never write directly to a final immutable name: a crash could otherwise leave an incomplete candidate indistinguishable from corruption. Existing immutable digest files must match exact bytes and are reused without replacement. The lock serializes checks and moves between cooperative writers. Write/sync a new HEAD candidate in staging after all referenced immutable files are installed.
7. Atomically replace HEAD as the sole visibility point. Only then return the receipt. A lost reply may mean success; retry the same request identity. For an I/O error around replacement, re-read HEAD: a proved complete new head returns the committed receipt, a proved old head is not published, and inability to establish either returns an explicit unknown publication outcome. Do not report an unknown outcome as guaranteed rollback. This outcome is a structured host error distinction; RFC 0046's wire error shape remains unchanged and its static message explains when reconciliation is required.

Public reads capture a single validated committed state. They expose only referenced non-quarantined objects; staging and unpublished content are never discoverable by hash alone. Yanked exact versions remain addressable as RFC 0046 specifies. D03 supplies index/download APIs; D02 need not expose a general filesystem interface.

D03 read-snapshot elaboration, accepted 2026-10-31 JST under RFC 0001 delegation: a read-only service may retain one completely validated committed generation while owning the exclusive registry lock. Derive shared immutable index bytes and the referenced non-quarantined archive digest/length table at startup; do not retain mutable authorization evidence or reread/clone the entire history for each public request. Recheck the exact captured HEAD before every response, and open each requested archive through the same regular-file/no-follow boundary, checking exact length, EOF and digest against the captured table before sending bytes. A changed/corrupt HEAD refuses service; it never selects another head or silently refreshes a snapshot. Mutation integration must publish a newly validated read generation with the committed state, and must still reauthenticate against freshly locked authority. This is a read-only D03 checkpoint, not permission to serve stale mutable authority. Retained indexes and maps count against the service memory budget; oversize legacy indexes keep their explicit per-package refusal and do not hide otherwise committed archives. Startup full-history validation precedes accepting connections. Rationale: shared immutable data removes per-request history copies while preserving exact captured-state identity. Wire/storage formats, limits and signing/freshness requirements remain unchanged; concurrency, budget and tamper behavior require executable validation before D03 completion.

Implementation clarification, 2026-10-31 JST: admission reads trusted host time; after archive staging, fresh authentication selects the event time persisted in the event/state floor. After the event, state and HEAD candidate are synced, a final guard resamples time, refuses regression below that event time and reauthenticates immediately before HEAD replacement. It does not rewrite the already admitted event timestamp. This closes expiry during metadata staging without claiming that a clock read and filesystem replacement happen at the same instant. All three checks use actual bearer possession and the same locked committed authority.

Initialization uses a fresh private destination and publishes its first HEAD only after bootstrap event/state completion. An interrupted incomplete destination is never silently reinitialized. Explicit operator cleanup/restoration remains distinct from ordinary recovery. No ambient credential, registry, root or cache fallback exists.

## Execution modes

This host storage layer performs no guest execution and does not change VM/native semantics. Supported macOS/Windows integration must exercise actual child-process interruption, file locking, replacement and reopened state. A local macOS prototype is not Windows evidence.

## ABI and compatibility

Affected domains are `package` and `tooling`. New private versioned state/audit/receipt formats do not change source editions, MIR, bytecode, native ABI or component ABI. No deployed registry state is migrated by this proposal. Unknown schema/operation variants refuse; future migrations require explicit recovery and downgrade consequences.

## Diagnostics and tooling

Use REG0001–REG0008 as defined by RFC 0046. Authentication and authorization failures remain distinct; stale revision/different request identity/version replacement are conflicts, corrupt committed history is state/trust failure, and lock contention is busy. Diagnostics contain static operation labels and safe IDs only, never arbitrary payloads or secrets. Reports are observations, not authority for subsequent mutation.

## Performance and cost model

Initial validation is O(total retained bytes/records), bounded by the physical storage quota; history, archive hashing and canonical decoding are real costs. New generations copy bounded state, not entire source objects. Peak quota accounts for old history plus new candidates before HEAD publication. No throughput or large-registry capacity claim is made. Retaining full history is deliberately simple and must be revisited through an accepted format/migration change if measurements make it unsuitable.

## Security and trust boundaries

The directory capability grants trusted host storage access, not guest filesystem access. Cooperative locking serializes registry operations; it does not sandbox a malicious operator with direct directory access. Digest chains detect corruption relative to the retained HEAD, not operator rollback of all state. Package provenance/freshness remains F01. Fresh authorization under the lock is required even when archive parsing happened earlier or a request was previously successful.

## Alternatives

- **No change:** byte admission and policy cannot publish a complete registry state.
- **Transactional database:** can provide transactions but adds an engine, migration/backup surface and deployment assumptions. Revisit if measured initial-profile costs warrant it.
- **Separate atomic files for index/owners/audit:** each file can be atomic while their joint visibility is inconsistent after interruption.
- **Only current state plus hash-only history:** uses less space, but needs explicit audit/recovery reachability and checkpoint semantics. The first profile instead proposes retaining complete committed generations within the fixed quota.

## Implementation plan

1. Review/accept exact records, history cost and orphan cleanup, updating RFC 0046's mirror pair before D02 code.
2. Keep E01 as the sole doing row until its complete validation. D02 retains its existing 3 pt; no additional prerequisite is introduced here.
3. Implement private bounded storage, initialization and immutable publish/replay around D01/E01. Add actual reopened-state/process-exit and competing-writer tests on both hosts.
4. D03 exposes index/download; D04/D05 execute and replay a real registry dependency. Re-audit that execution sequence after E01/D02 rather than adding more horizontal prerequisites.

## Test plan

- Initial claim, second version, exact retry, altered retry, stale revision and attempted replacement; verify original receipt and immutable bytes.
- Every write/sync/HEAD cut, with parent process reopening and validating complete old/new release/owner/audit state.
- Two independent writer handles/processes, busy refusal and both competing publication orders.
- Wrong service, forged/stale actors, invalid credential, scope/owner failure, clock regression and fresh policy after intervening state change where an implemented mutation exists.
- Every physical/record/byte/count bound; corrupt HEAD, linked state/audit/object, malformed canonical fields, unknown entries and symlink/hardlink/nonregular refusal without cleanup.
- Orphan cleanup only after valid committed-chain proof; no missing-HEAD reset, highest-candidate adoption or history pruning.

## Open questions

Implemented checkpoint, verified 2026-10-31 JST: `registry::store` at `e76dd7b` passes the full baseline and macOS/Windows CI, including 22 bootstrap and 24 publication process-exit cuts, competing writer processes, exact reopened-source execution, replay/final authorization, unknown-outcome reconciliation and hostile recovery/quotas. Windows lock-file validation uses metadata rather than a second-handle EOF read while holding the exclusive lock. The [M5 board](../milestone-5.md) records exact runs and remaining PR publication. Power-loss availability remains a non-goal; this is not HTTP/signing or production consumption. The original acceptance record below is historical.

Accepted under RFC 0001 experimental delegation on 2026-10-31 JST. Review found that writing directly to an immutable name could leave partial content after a process exit; the accepted sequence stages and syncs first. The expanded `/private/tmp/lang-m5-store-crash-probe.py` passed 13 real process-exit cuts on local macOS 27.2, including partial object/audit/state/HEAD writes: cuts 0–11 retained complete revision 0, cut 12 exposed complete revision 1, every installed immutable digest matched, and recognized orphan staging/candidates could be removed after the visible state was proved complete. The earlier six-cut complete-write probe is superseded by this stronger experiment.

Full-history retention was accepted as a conservative bounded initial-profile cost, not a capacity claim. Exact transition validation and explicit unknown outcomes resolved the remaining state/audit/error ambiguities. At acceptance, Windows replacement, concurrent readers/writers, complete hostile recovery, actual authentication integration and power-loss behavior were unverified. The prototype alone did not close D02, and E01 remained the sole doing delivery until its own complete validation. The implemented checkpoint above supplies the later both-host process/concurrency evidence; power-loss availability remains a non-goal.

## Sources

- [RFC 0046](0046-registry-protocol-and-recovery.md) and [RFC 0047](0047-registry-authority-records.md), last updated 2026-10-31: accepted project requirements.
- [POSIX.1-2024 rename](https://pubs.opengroup.org/onlinepubs/9799919799/functions/rename.html), 2024 edition; individual page update date unavailable; verified 2026-10-31 JST. Specifies atomic rename semantics, not cross-platform power-loss availability.
- [Microsoft MoveFileExW](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw), last updated 2023-06-01; verified 2026-10-31 JST. Platform replacement flags need actual adapter validation; the page alone is not evidence for our storage implementation.
- [Rust File](https://doc.rust-lang.org/std/fs/struct.File.html#method.try_lock), publication/update date unavailable; verified 2026-10-31 JST. Lock interactions and cloned-handle behavior are platform-dependent.

Record shapes, full-history retention and cleanup rules above are project choices. Prototype observations and unverified platform/durability assumptions are stated separately.
