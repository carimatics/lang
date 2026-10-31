<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0045-registry-threat-model.md -->

# RFC 0045: Public registry threat model

[日本語版](../ja/rfcs/0045-registry-threat-model.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-C01–C02 and G3 in the M5 delivery board
- Supersedes: none
- Superseded by: none

## Summary

Define identities, authorities, adversaries and security acceptance obligations before the registry accepts uploads. A package release is identified by its explicit registry, package name, exact version and immutable content digest. Downloading, authenticating or owning that release never grants permission to execute it or use host capabilities.

This is the C01 threat-model decision. C02 must separately accept concrete protocol formats, resource limits, recovery rules and mapped refusal cases before upload implementation. G3 remains open until both decisions exist; G4–G6 and G9 still require working storage, authenticated operations, signed consumption and hosting/search.

## Motivation

RFC 0034 currently resolves explicitly authorized local paths. RFC 0035 produces a trusted, target/compiler-specific graph snapshot; it is not an untrusted archive importer or a portable registry release. RFC 0042 authenticates toolchain distribution, with a different trust domain. Reusing any of these as an implicit registry would omit public namespace authority, malicious publishers, immutable-version races and recovery after credential compromise. A library checksum alone does not resolve those problems.

## Goals

- Preserve exact package/source identity across publication, dependency resolution, cache, lock replay and execution.
- Bound hostile unauthenticated requests and authenticated uploads before allocation, extraction or persistent mutation.
- Separate namespace ownership, operation authorization, byte integrity, provenance claims and execution authority.
- Make publish/yank/owner actions and recovery observable, attributable and testable on both tier-1 hosts.

## Non-goals

This RFC does not select wire encodings or cryptographic envelopes, implement a server, establish an official production registry/key, promise global availability or certify publisher code as safe. It does not add version ranges, ambient registry discovery, build scripts, package installation hooks or native-plugin sandboxing. Private package confidentiality and transparent cross-registry mirroring are outside the initial public profile. Malware moderation, name disputes and account vetting remain operator responsibilities, with the constrained administrative actions below.

## Guide-level design

A consumer explicitly selects a registry authority. `example_math` at `1.2.3` from registry A and the same spelling/version from registry B are different dependencies, even if their bytes match. A local alias is only a source-language binding; it cannot replace that identity. A lock pins the resolved origin, name, exact version and artifact digest. There is no fallback to another registry on missing package, denial, timeout or signature failure.

A publisher previews exactly which files and dependency metadata will be sent, then explicitly publishes one release. A successful retry of the same transaction has the same identity; another body cannot replace that version. Yank removes a version from new selection without erasing its immutable identity, provenance or existing exact-lock meaning. Ownership transfer changes future authority, not authorship of old releases. Security quarantine is a separately audited refusal of future service, never a silent rewrite or redirection to other bytes.

Errors distinguish malformed input, exhausted limit, authentication, authorization, version conflict, stale state, signature/freshness failure and recovery-required state. They disclose no credentials, private filesystem contents or bearer tokens. Authentication failure must not reveal whether a supplied token identifier exists.

## Reference semantics

### Identities and namespace

- Keep RFC 0034 package-name and alias grammar `[a-z][a-z0-9_]{0,63}`, exact three-component decimal u32 versions and edition `2026`. Different versions are distinct immutable releases. No case-folding or Unicode normalization creates alternate package identities.
- Bind a registry's logical identity to explicitly configured transport origin and independently supplied trust policy. A URL, key, issuer or logical name supplied by a downloaded package cannot replace that binding. C02 specifies canonical encoding; F01 specifies authenticated root succession. An origin move cannot reset the consumer's rollback history.
- A package name belongs to one namespace record within one registry. Initial claim is an authenticated, authorized, atomic operation; ordinary release upload cannot steal an existing name. Names are not recycled after owner removal, yank or quarantine. Flat names deliberately leave squatting and convincing lookalikes as residual risks; the UI must show the registry and exact name.
- Principals have stable server identities. Owner membership is explicit per package; display names are not authority. Every mutation rechecks current membership, token scope, audience, expiry and revocation inside its serialized transaction. A member cannot remove the last active owner. Transfer must establish the successor's authority before retiring the predecessor. An owner credential has no implicit registry-administrator privilege.

### Authorities and data flow

| Boundary | Required authority and validation |
| --- | --- |
| Local workspace → publish client | Explicit directory capability and frozen selected inventory; a preview grants no network/write permission. Exclude ambient credentials, output and undeclared files. |
| Client → registry | Explicit origin and operation; certificate-verified HTTPS, no redirects or content-driven URLs. Public reads send no publishing credential. Local test transports remain explicit isolated fixtures. |
| Credential → operation | Bind to one registry audience, principal, finite lifetime and named actions/package scope. No wildcard upgrade from read or publish to owner/admin. Secrets use an explicit protected input channel, never argv, logs, archives or locks. |
| Upload → storage/index | Strict bounded metadata/archive admission without executing source; private staging; atomic immutable identity plus committed audit record. |
| Index/object → consumer | Bound bytes before parsing; verify origin, requested identity, digest, signatures and freshness policy before publication or use. A cached object is rechecked, not an authority. |
| Package → compiler/runtime | Source stays untrusted; preserve spans and existing frontend/IR verification and resource limits. Imports grant no filesystem/network/process/FFI rights. |
| Documentation/search → browser/user | Treat rendered source, names, descriptions, URLs and query text as hostile. Encode output, disallow active content and separate document service authority from publication credentials. |

Authentication, membership and scoped authorization are distinct checks. A cryptographic digest identifies bytes but authenticates no publisher. A valid release signature establishes only the claims authorized by its trusted issuer; it does not prove harmlessness or an audited build. Source-package provenance must name what was actually observed and distinguish publisher assertions from independently verified builder evidence. No SLSA level is claimed by this decision.

Registry and consumer persistent state use explicit directory capabilities; do not reopen metadata-supplied host paths. Path alias, symlink, hardlink, device, case collision and file/directory-prefix checks must apply at the relevant open/publication boundary. C02 must specify a portable archive profile, complete accounting and crash recovery instead of delegating safety to generic TAR extraction.

Registry operations are serialized at their mutation boundary. A stale authorization check before a parallel owner removal must not authorize a later commit. A successful response requires the version/owner/yank change and its audit event to be committed together. Recovery must reject malformed or inconsistent state, preserve rollback floors and immutable identities, and never invent success from leftover staging. C02 specifies process-crash versus power-loss guarantees and retry reconciliation explicitly.

No change is made to source evaluation order, types, ownership, destruction, atomics, cancellation or constant execution. Cancelling a transfer must release bounded request resources; it cannot undo a committed version or imply that a lost response means no commit.

## Execution modes

Admission does not run guest code or a build hook. Accepted source follows the existing common frontend/typed-MIR path for VM and native execution. Actual dependency fixtures must agree across those modes. Native execution remains trusted host execution; a signature cannot make it a sandbox. Wasm plugin execution separately needs G7/G8's verified components, denied ambient authority and runtime limits. A registry token must never enter a guest host-call context merely because the guest came from that registry.

## ABI and compatibility

This decision affects the `package` and `tooling` domains. Source syntax, native/component ABI, MIR and bytecode are unchanged. Keep existing manifest/lock/archive schemas readable under their accepted contracts; do not reinterpret a local path or RFC 0035 snapshot as a registry identity. C02 must version new metadata and define migration and exact-lock/offline behavior. Unknown schemas, algorithms and identity forms fail closed. RFC 0042 roots, keys, signatures and rollback state do not implicitly authorize package releases.

## Diagnostics and tooling

Client reports show the selected registry, package, version, digest, operation outcome and bounded diagnostic location where applicable. Preserve original source paths/spans after materialization, using a separate origin label rather than synthetic source rewriting. Saved previews/reports grant no write authority. Publication must not run editor callbacks, credential-provider commands from package metadata, formatters or arbitrary subprocesses. C02 allocates stable diagnostic identities and response shapes; I01 defines safe hosting/search output policy. Parser, formatter, LSP and debugger source semantics remain unchanged.

## Performance and cost model

Archive bytes, decoded metadata, entries, paths, graph depth/edges, cache/staging size, request concurrency, token verification, signature work, owner/audit records and search work all need finite limits. Check declared and observed lengths with checked arithmetic; charging only the final extracted file count is insufficient. Bound rejects as well as successful work. Audited storage retention and unavailable-full-storage behavior are C02 obligations; unbounded logs are not a DoS defense. No latency, throughput, footprint or ergonomics improvement is claimed without measurement.

## Security and trust boundaries

The trusted base is the consumer binary/OS, explicitly supplied roots and configuration, local capability directories, and the registry's authorization/transaction implementation for owner actions. Registry credential administration and package-signing trust are distinct roles. Network peers, mirrors/caches, arbitrary unauthenticated clients, authenticated malicious publishers, dependency content, documentation and search input are untrusted. Assume stolen/revoked credentials, compromised online signing keys, malicious storage responses, process crashes and concurrent valid requests can occur.

| ID | Attack and asset | Required control and observable refusal | Delivery |
| --- | --- | --- | --- |
| R01 | Dependency confusion or package-origin substitution | Explicit origin on every edge/lock/cache identity; same spelling at another origin never supplies the request | C02,D03–D05 |
| R02 | Namespace takeover, owner race or lost last owner | Atomic name claim, current scoped membership checks, conditional owner mutation and nonempty ownership | E01–E04 |
| R03 | Stolen, expired, revoked or wrong-audience token | Fail closed on every mutation; bounded secret handling and redacted reports; revocation cannot be bypassed by a previously prepared request | E01–E04 |
| R04 | Malformed archive, traversal, links, device or platform alias | Validate bounded logical inventory before materialization; never escape the capability root or overwrite existing files | D01,D05 |
| R05 | Oversized, endless or deeply nested input; signature/query flood | Enforce byte/work/depth/concurrency/storage limits before side effects; cancellation/deadline cleanup | C02,D01,E01,I03 |
| R06 | Racing duplicate version or inconsistent publish/audit | Single immutable commit point; one winner; retry reconciles exact identity without duplicate mutation | D02,D05,E04 |
| R07 | Tampered cache/storage, mismatched index and payload | Bind requested identity, authenticated manifest and bytes; no partial/unverified generation becomes consumable | D03,F02–F04 |
| R08 | Rollback, freeze, mix-and-match, expired metadata or clock regression | Retained version/time floors, freshness checks and exact cross-record binding; offline policy cannot silently clear trust state | C02,F01–F04 |
| R09 | Compromised online key, unauthorized root replacement | Independent initial trust, domain separation, bounded authenticated succession and revocation; reject self-asserted new roots | F01–F04 |
| R10 | Honest signature over malicious source/build/doc content | No execution during admission, no implied host grants, bounded compiler/runtime, safe document service | D01,G7/G8,I01–I03 |
| R11 | Crash, disk-full or reply loss at publication | Recover committed state/audit together, clean only recognized staging, preserve old versions and reconcile unknown outcomes | C02,D05,E04 |
| R12 | Audit forgery, secret leakage or erased history | Attributable bounded durable records with no secrets; fail closed on detected corruption; externally retained checkpoints for operator-level evidence | E04,F01 |
| R13 | SSRF, credential forwarding, hostile origin redirect | Configured HTTPS origin only, no redirect, no metadata URLs, public reads omit credentials; explicit network capability | C02,D03,E01 |
| R14 | Workspace exfiltration or path-dependency publication | Preview frozen admitted files only; reject private local dependency paths in a public release unless explicitly converted to registry identities | C02,D01,E02 |
| R15 | Stored/reflected script or query injection | Inert version-bound documentation, encoded text, no arbitrary code evaluation and bounded structured search | I01–I03 |
| R16 | Administrative compromise or malicious operator | Separate explicit admin recovery/quarantine authority with audit; no silent byte replacement; consumers retain independent identity/trust state | C02,E04,F01–F04 |

Residual risk is explicit: a properly authorized publisher can publish malicious new code. Revocation cannot undo code already executed or invalidate copies outside the registry's control. Yank is not erasure or a universal malware kill switch. A compromised full signing threshold or trusted consumer host defeats guarantees within that authority. A malicious registry operator can withhold service, misrepresent mutable ownership state or delete its own unexported audit history; a local hash chain alone cannot prevent that. Independent retained receipts/checkpoints can expose some contradictions, not prove completeness or global non-equivocation. Traffic exhaustion and operator capacity still affect availability; finite per-request work is not a public-service SLA.

## Alternatives

- **No change:** local path dependencies preserve the M4 boundary but do not satisfy the public-registry M5 gates.
- **External library/tool:** reuse vetted HTTP, hashing, signature and archive primitives where suitable. They do not decide package identity, owner authority, atomic release/audit or execution capability policy; importing those choices accidentally is unacceptable.
- **Cargo-style registry ecosystem:** Cargo explicitly supports named alternate registries and credential providers, a stronger practical comparison than a bare file server. Adopting its protocol wholesale would also import a different package/manifest/resolution ecosystem. Retain explicit origin identity and protected credentials while defining this language's versioned profile; no claim that Cargo supplies this RFC's threat guarantees.
- **Full TUF repository model:** its separate roles, metadata binding, thresholds and update procedure provide a mature stronger distribution design. F01 must compare implementing that model with a narrower reviewed package-signing profile. This threat model adopts the attack obligations, not an unimplemented TUF-conformance claim.
- **Simpler HTTPS plus checksums:** detects accidental corruption and authenticates transport, but does not independently establish publisher authority, survive compromised storage, prevent signed-metadata replay or define version/audit transactions. Insufficient for G5/G6.

## Implementation plan

1. C01 accepts this threat model. C02 maps every R01–R16 obligation to concrete protocol/limits/recovery and executable acceptance cases; upload implementation waits for C02 acceptance.
2. D01–D05 build bounded archive admission, immutable storage/index/download and real dependency execution plus exact offline replay. E01–E04 implement scoped authentication, publication/ownership and durable audit under the same state transition rules.
3. F01 accepts package root/provenance/freshness formats before F02–F04 implement signed consumption and key recovery. Isolated unsigned development fixtures are not a production security profile or G6 completion.
4. I01–I03 implement safe document hosting/search; G7/G8 retain their independent plugin sandbox gates. Z01/Z02 require both-host continuous journeys and the final audit/guide.

## Test plan

For each R ID, retain an adversarial input, the exact denied boundary and a state/side-effect observation. Test limit − 1 / limit / limit + 1 where meaningful, not only nominal refusals. Exercise two origins with colliding names, token scope/expiry/revocation during owner races, two different payloads at one version, interrupted publish/retry at each commit boundary, tampered storage and signed rollback/expiry/root changes. Verify no guest execution, credential leakage or writes outside authorized staging on refusal. Include path/case/link behavior on macOS and Windows, exact offline identity replay and a real transitive dependency returning the same value through VM and native execution. Archive/metadata parsers require malformed-input corpus/fuzzing; finite resource guarantees need observable counters. Later signing, hosting and sandbox decisions elaborate their own fixtures without removing these obligations.

## Open questions

Accepted under RFC 0001's experimental delegation on 2026-10-31 JST. Rationale: explicit identity and capability boundaries, immutable releases, current-state authorization and test-mapped adversaries preserve existing compiler contracts while making the new public input boundary reviewable. No blocking question remains for C01. C02's encodings, finite limits, durable transaction profile and recovery cases and F01's key/freshness/provenance scheme remain unimplemented decisions, not implied behavior. Deployment quotas, moderator staffing, real key custody and incident response remain unverified operational assumptions. Acceptance proves neither security, implementation nor stabilization.

## Sources

- [RFC 0001](0001-project-charter.md), updated 2026-10-28; [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md) and [RFC 0035](0035-source-package-snapshots.md), updated 2026-10-29; [RFC 0042](0042-signed-toolchain-installation.md) and [RFC 0043](0043-editions-and-compatibility.md), updated 2026-10-30. Verified 2026-10-31 JST. These establish project contracts, not external security certification.
- [The Update Framework specification 1.0.37](https://theupdateframework.github.io/specification/latest/), last modified 2026-10-08, verified 2026-10-31 JST. Source for rollback/freeze/mix-and-match, bounded updates, independently trusted roots and authenticated rotation threats.
- [SLSA 1.2 threats overview](https://slsa.dev/spec/v1.2/threats-overview) and [provenance](https://slsa.dev/spec/v1.2/provenance), publication/update dates not provided; verified 2026-10-31 JST. Provenance describes origin evidence; the threat model explicitly does not directly prevent a malicious producer.
- [RFC 9700, OAuth 2.0 Security Best Current Practice](https://www.rfc-editor.org/rfc/rfc9700.html), published 2025-01; verified 2026-10-31 JST. Sections 2.3 and 4.10.2 support least-privilege/audience checks. This does not select OAuth for the project.
- [Cargo registries](https://doc.rust-lang.org/cargo/reference/registries.html) and [registry authentication](https://doc.rust-lang.org/cargo/reference/registry-authentication.html), publication/update dates not provided; verified 2026-10-31 JST. Named alternate registries and configurable credential providers are sourced facts; this RFC's authority/transaction policy is a project decision.
