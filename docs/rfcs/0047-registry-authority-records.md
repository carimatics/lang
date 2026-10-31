<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0047-registry-authority-records.md -->

# RFC 0047: Registry authority records and fresh authorization

[日本語版](../ja/rfcs/0047-registry-authority-records.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-E01, M5-D02
- Supersedes: none; elaborates RFC 0046
- Superseded by: none

## Summary

Specify the bounded authority records and pure authentication/authorization checks used by a registry transaction. An opaque token identifies an active enrolled principal only within one explicitly configured registry and HTTPS origin. Its grants and current ownership determine the allowed operation. An earlier successful check never authorizes a later commit.

## Motivation

RFC 0046 fixes token scope, revocation and commit-time checks, but leaves the exact private record encoding to an elaboration. Implementing publication first would invite an API accepting a caller-supplied actor or a cached authorization result. Implement E01's pure policy before D02's transaction, then invoke the policy on the freshly loaded committed state while holding D02's lock.

## Goals

- Validate finite, closed authority records before policy evaluation.
- Bind credentials to both logical registry and canonical origin.
- Distinguish authentication, operation grants and current ownership.
- Keep secrets out of persistence, diagnostics and ordinary debugging.
- Make expiration, revocation, owner removal and clock regression observable in tests.

## Non-goals

- HTTP serving, TLS termination, credential generation, login or identity federation.
- Authority storage, provisioning/revocation transactions, owner mutation or audit persistence.
- Package signing, a production consumer, or a new source-language capability.
- Timing guarantees for the entire request, memory-forensics protection or protection from the trusted host/operator.

## Guide-level design

A publishing credential has explicit grants for a package name. Creating that name requires both `claim` and `publish`; publishing another version requires `publish` and current owner membership. A token granting `publish` for `parser` cannot publish `lexer`, and an administrator token is not an ordinary publishing token. Removing an owner or revoking a credential prevents subsequent operations even if a preceding preview succeeded.

The client accepts a bearer only from its explicit bounded secret input. A future HTTP adapter sends it to the configured origin without redirects. The server never accepts actor IDs, timestamps, ownership lists or earlier authorization reports as caller-provided authority.

## Reference semantics

### Authority record profile 1

The host-only interchange object has ordered fields `format: "lang-registry-authority"`, `schema: 1`, `registry`, `origin`, `time_floor`, `principals`, `tokens`, `packages`. It uses RFC 0046's canonical JSON (two-space pretty encoding and final LF), integral numbers, no duplicate/unknown fields, and no alternate spellings. Maximum bytes are 16 MiB and maximum JSON nesting depth is 16. Enforce byte/depth/count bounds before unbounded typed allocation. This is a validated view of trusted registry state, not a signed document or an authority supplied by a network caller.

`registry` and package names follow RFC 0046's name grammar. `origin` follows its canonical HTTPS profile, including rejection of implicit normalization, an explicit default port 443, leading-zero ports and URL suffixes. Other decimal ports 1–65535 remain permitted. Registry ID alone is insufficient audience binding: two installations using the same label at different origins are distinct services. Signing roots are a separate domain and are not bearer audiences. Admission takes the expected registry/origin from independent trusted host configuration and refuses a mismatched enclosing identity; the record cannot choose its own trusted service.

| Collection | Ordered record fields | Ordering and bound |
| --- | --- | --- |
| `principals` | `id`, `active` | Strictly ascending 32-character lowercase-hex ID; at most 4,096. |
| `tokens` | `id`, `principal`, `registry`, `origin`, `authenticator_sha256`, `issued`, `expires`, `revoked`, `grants` | Strictly ascending 32-character lowercase-hex ID; at most 4,096. |
| Token `grants` | `action`, `name` | Strictly ascending `(action spelling, name)`; 1–128. Action is `admin`, `claim`, `owners`, `publish` or `yank`; names are exact, with no wildcard. |
| `packages` | `name`, `owners` | Strictly ascending package name; at most 1,024. Presence means the name is already claimed. |
| Package `owners` | Principal ID string | Strictly ascending unique IDs; 1–128. |

Every token principal and owner refers to an enrolled principal. Inactive principals may remain as historical members, but every package retains at least one active owner. Token IDs and authenticator hashes are unique; no ambiguous matching record is admitted. A token's registry/origin must equal the enclosing service. `authenticator_sha256` is exactly 64 lowercase hex characters, computed as RFC 0046 specifies from the uniformly random 32-byte bearer; it is not the bearer itself. Empty principal/token/package collections are permitted for a fresh unprovisioned view.

Times are integers in `0..=253402300799`. Require `issued < expires` and `expires - issued <= 604800`; the time floor has the same range. Revoked/expired tokens remain valid records and are denied at authentication. Record validation does not make them active again.

The complete immutable registry state embeds these fields or this closed authority object without changing their semantics. D02 must specify its complete generation/HEAD/audit encoding and validate consistency between claimed packages, owners and releases before it persists them. E01 introduces no independently writable authority file, second commit point or new recovery path. This narrows RFC 0046's sequencing requirement: these authority fields precede E01; the remaining transaction and audit fields precede D02.

### Secret input and authentication

Secret input is exactly 64 lowercase hex bytes, optionally followed by one LF. Reject CRLF, surrounding whitespace, uppercase, partial input and extra bytes. A bounded reader consumes at most 66 bytes to distinguish an allowed input from excess; it never drains an unbounded stream. HTTP header parsing is a separate adapter: a single exact `Bearer ` prefix followed by 64 lowercase hex bytes, with no trailing LF or duplicate Authorization headers.

Store a decoded bearer only in a private, non-serializable secret wrapper. Its `Debug` output is redacted and it has no implicit string/display conversion or automatic clone. Clear owned secret buffers on drop using the existing vetted zeroization dependency family. Caller-owned input and transport buffers remain the caller's responsibility; this does not promise elimination of every compiler, allocator or operating-system copy. Explicit future transport access must remain visibly secret-bearing.

Compute the domain-separated authenticator, compare all 32 bytes using a maintained constant-time comparison primitive, and scan the complete admitted token collection without returning early at a matching index. Do not invent an ordinary XOR loop as a compiler-proof timing primitive. The selected primitive's guarantee is limited to equality; record parsing, hash lookup validation and authorization are not advertised as constant-time.

Authenticate against the current view and trusted host time. Unknown token, revoked token, inactive principal, wrong audience, `now < issued`, or `now >= expires` is a generic authentication refusal. Require `now >= time_floor`; invalid/regressed host time is a state refusal. The host reads time again before commit rather than accepting a time supplied by the request. The pure E01 API takes time explicitly for deterministic testing; only its trusted host adapter chooses that argument.

### Fresh operation policy

Authentication precedes operation-specific checks and precedes idempotency lookup. Package names are admitted before use. All checks use the same current view.

| Operation | Required current authority |
| --- | --- |
| Read owners | Name exists; principal is an owner, or has `admin` for this name. No mutation grant is needed for a member's read. |
| Claim and publish | Name is absent; both `claim` and `publish` grants for this name. D02 atomically installs this principal as initial owner. |
| Publish existing version identity | Name exists; owner membership and `publish` grant. D02 separately enforces immutable version and expected revision. |
| Yank or unyank | Name exists; owner membership and `yank` grant. |
| Change owners | Name exists; owner membership and `owners` grant. E03 separately checks the proposed membership transition and last active owner. |
| Administrative quarantine or recovery | Name exists; `admin` grant for this name. It does not imply publish, yank or owner-mutation scope. |

Local principal/token provisioning and verified registry restoration use an explicit trusted administrative directory capability, not a public wildcard admin token or an unauthenticated HTTP endpoint. They remain D02/E04 transactions.

The pure result contains only the admitted principal ID and nonsecret token ID for an audit event. It is informational and is neither serializable authority nor an input accepted by a later mutation API. D02 reauthenticates the actual credential and reruns policy under its lock against the latest state and time before replay, expected-revision checks or mutation. The D02 adapter must derive the operation from the admitted request; a network caller cannot choose a weaker policy operation than the mutation performed.

### Failure and ownership behavior

Failures return bounded structured errors with static messages. Never interpolate a bearer, token hash, raw JSON, input path or hostile field. No policy call performs I/O, executes source code, persists a change or changes a time floor. Immutable owned validated records may be shared by trusted host code; their mere existence does not certify freshness. No `unsafe`, guest object, backend type or source span is involved in this host protocol layer.

## Execution modes

This host registry policy is independent of VM/native compilation and grants no guest execution capability. The same records and checks apply to both platforms. D02's real storage and network integration require separate platform validation.

## ABI and compatibility

Affected domains are `package` and `tooling`. No language edition, source, MIR, bytecode, native ABI or component ABI changes. Profile 1 is a new private versioned encoding; unknown formats/schemas refuse admission. No shipped registry authority state exists to migrate. A future schema change needs an explicit migration/recovery decision and must not reinterpret a previously rejected token or audience.

## Diagnostics and tooling

Use RFC 0046 classes: malformed authority records `REG0001`, bounded-resource refusal `REG0002`, generic credential failure `REG0003`, operation denial `REG0004`, service identity mismatch `REG0006`, and invalid/regressed trusted time `REG0007`. Network status mapping and user-facing publish commands remain adapter work. Redaction tests cover error and debug formatting. No source formatter, LSP or debugger semantic change is required.

## Performance and cost model

Admission is bounded by 16 MiB, collection ceilings and depth 16; validate references and uniqueness with bounded sets/maps. Every authentication scans at most 4,096 fixed-length authenticators; scope and owner checks are bounded by 128 entries. No latency or throughput target is claimed before measurement. Zeroization and canonical re-encoding have explicit bounded memory/copy costs; avoid cloning the whole view for each operation.

## Security and trust boundaries

An admitted authority object proves structural validity only. Trust comes from D02's private directory capability, validated committed state and lock; accepting an uploaded object as that state would be a security defect. A correct digest is neither token authenticity nor package provenance. Revocation, ownership and time are mutable and must be checked at the visibility point. A stolen live scoped bearer, malicious authorized publisher and compromised host/operator remain RFC 0045 residual risks.

## Alternatives

- **No change:** leaves the record and freshness contract implicit at the publication boundary.
- **External identity service/library:** useful for later enrollment, but does not specify current package ownership, exact service audience or atomic commit-time revocation. The bounded E01 policy still needs a local contract.
- **Self-contained signed access tokens:** can verify an issuer independently, but current revocation and owner membership still need mutable trusted state. Opaque high-entropy tokens match RFC 0046 and avoid a second token-signing/root lifecycle.
- **Simpler cached actor or boolean:** reduces plumbing but admits stale scope/ownership and replay after revocation. Returning audit identity while requiring the actual credential at every commit keeps the public boundary explicit.

## Implementation plan

1. Review this elaboration and update RFC 0046's sequencing reference before E01 implementation. Keep D01 as the sole doing row until its validation completes.
2. Select E01, implement the host-only bounded record reader, redacted bearer input and fresh policy; add focused behavior and malformed-input tests.
3. Run the baseline and both-host CI, update bilingual architecture/board evidence, and publish the work branch. This completes E01 only, not G5.
4. D02 elaborates and implements atomic storage around these checks; E02–E04 add their separately tracked owner, recovery and audit journeys.

Branch preparation: `codex/m5-registry-authority` starts from `codex/m5-registry-archive` at `b3c8d1b` in `carimatics/lang`; PR #33 is its dependency. Retarget to `main` only after that dependency merges. Original work-unit branch remains `main`; no pre-existing user changes were present in the new checkout.

## Test plan

- Valid authentication and every operation's success/denial, including separate admin and ordinary grants.
- Wrong name, same registry label at another origin, absent/unowned package, inactive principal and unknown/revoked token.
- Exact issued/expiry/seven-day/time-floor boundaries, including invalid trusted host time.
- Fresh reevaluation after revocation/owner removal; D02 later tests revocation before replay and changes while a body is staged.
- Canonical JSON, duplicate/unknown fields, broken references, duplicate hash/IDs, ordering, integer encoding and each allocation/count/depth boundary.
- Secret input at 63/64/65/66 bytes, optional LF, forbidden CRLF/whitespace/uppercase, bounded readers, redacted error/debug output.
- Deterministic hostile-input mutation without panic and both-host CI. Compile the fuzz entry point if added; do not claim a sustained fuzz campaign unless it actually runs.

## Open questions

Implemented host-policy scope, verified 2026-10-31 JST: `56f9f84` implements the bounded canonical authority reader, independent service identity, redacted/zeroizing bearer, fixed-length `subtle` comparison and current operation/owner checks. Ten focused integration tests, 512 deterministic mutations, compiled fuzz entry point, the full 2,572-test/182-suite baseline and both-host CI pass. This is not a timing benchmark or sustained fuzz campaign. RFC 0048 now accepts the complete storage/audit encoding; production transport and actual locked commit-time freshness remain subsequent implementation obligations. No public service or guest capability is established by E01.

Accepted under RFC 0001 experimental delegation on 2026-10-31 JST. The pure policy model `/private/tmp/lang-m5-authority-policy-model.js` passed 23 explicit operation/audience/time cases and demonstrated successful retry followed by refusal of the same retained request after credential revocation and owner removal. This is decision evidence only: it implements no real authentication or filesystem transaction. Review corrected ambiguous port wording and requires independent configured identity at admission. The separate audit identity and fresh credential checks make the E01/D02 boundary explicit. At acceptance, credential transport, constant-time/zeroization integration, state/audit encoding and locked freshness were unverified. The implementation checkpoint above records subsequently verified scope; G5 remains open until its complete workflows pass.

## Sources

- [RFC 0045](0045-registry-threat-model.md), last updated 2026-10-31: accepted project threat model and residual risks.
- [RFC 0046](0046-registry-protocol-and-recovery.md), last updated 2026-10-31: accepted project archive, token, HTTP and transaction requirements.
- [RFC 0001](0001-project-charter.md#experimental-rfc-decisions), delegation dated 2026-10-17 JST: acceptance authority and compatibility domains.

The encoding, policy matrix and implementation sequence here are project design decisions, not claims that an external specification mandates these exact choices.
