<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0057-package-signing-and-freshness.md -->

# RFC 0057: Package signing roots, provenance and freshness

[日本語版](../ja/rfcs/0057-package-signing-and-freshness.md)

- Status: proposed
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-F01–F04
- Supersedes: none; elaborates RFCs 0045, 0046, 0049, 0050 and 0056
- Superseded by: none

## Summary

Propose independently configured registry signing roots, authenticated root succession, signed bindings to the existing package index, accurately scoped publication provenance, and durable freshness/revocation checks before constructing production `ConsumedArchive` evidence. No unsigned fallback or capability grant follows from a signature.

This is a preparation draft, not an accepted contract or implemented trust path. The existing pinned Ed25519 implementation passes a separate 13-case dual-threshold/domain-separation experiment. A 22-check pure freshness/yank policy and maximal-field encoding model passes: 128 proposed publication observations produce a 54,463-byte payload and 60,807-byte envelope. These experiments do not test production storage, transport, source integration or platform behavior. Resolve the open decisions below before acceptance or implementation.

## Motivation

RFC 0049 deliberately leaves the production consumed-archive constructor sealed. RFC 0050 caches immutable bytes but cannot grant authenticated consumption. RFC 0042's toolchain installer has an immutable bootstrap root and explicitly defers automatic succession; reusing that policy unchanged would fail the registry's rotation/revocation obligations. Registry ownership, transport TLS, byte hashes, provenance assertions and signing trust must remain distinct.

## Goals

- Bind an independently supplied bootstrap policy to one registry/origin and preserve its identity through authenticated key updates.
- Bind signed metadata, exact index bytes, immutable archive/descriptor identities and observed publication records before source consumption.
- Preserve monotonic root/metadata/time floors and known revocation/quarantine information across failures and offline replay.

## Non-goals

- Claiming TUF wire compatibility, a SLSA level, independently verified build provenance, source harmlessness or immediate global revocation.
- Implicit trust discovery, signed metadata URLs, origin migration that resets floors, package-selected keys or an unsigned CLI/cache bypass.
- Granting filesystem, network, process, FFI or guest capabilities through provenance or signatures.

## Guide-level design

A trusted host supplies initial root bytes independently of the registry, explicit service identity, a private durable trust-state capability, a separate package-cache capability, a clock and explicit transport access. The binding's root digest denotes the original policy; successor roots do not create a fresh identity or clear history. An expired root may need authenticated succession, but cannot authorize release consumption merely because it was once trusted.

A separate host producer reads a completely validated registry generation, captures the exact index and observed publication facts, and signs with explicitly supplied in-memory keys. Ordinary publish/yank/owner HTTP handlers do not receive signing secrets. A consumer verifies current trust and signed metadata before opening package payloads, then rechecks exact bytes and fresh authorization conditions before obtaining sealed source evidence. Offline operation makes no network or credential lookup and requires retained authenticated evidence and exact cached bytes.

## Reference semantics

The following is the candidate profile; unresolved decisions are explicit below.

### Root and signature boundary

Use closed, versioned, bounded records and Ed25519 strict verification, including weak-key refusal. A root payload binds format/schema, registry, origin, positive version, issued/expires, public keys and separate root/release roles with explicit thresholds. Each role has at most eight distinct keys; their key IDs are SHA-256 of the exact public bytes. Keep root and release roles disjoint. A candidate root succeeds only at the next contiguous version and under distinct authorized thresholds from both old and new root roles. A signer shared by successive root roles may contribute once to each role; duplicate signer records never increase a threshold. Signatures not authorized by either applicable root role refuse.

Proposed envelope fields are `{schema, payload, signatures}`, where payload is the exact UTF-8 string signed, and signatures contain key ID and signature. Domain prefixes `LANGREGROOT1\0` and `LANGREGRELEASE1\0` separate root and release signatures from each other and the toolchain profile. Verify exact signed bytes; do not normalize before verification. Require the existing canonical pretty-JSON-plus-LF representation after closed admission. Initial root policy identity should hash the canonical initial payload, independently of envelope signature ordering; finalize this choice before acceptance.

Candidate bounds are 32 KiB root payload / 64 KiB root envelope, at most 16 successor-root signatures, and 128 KiB release payload / 1 MiB release envelope with at most eight release signatures. Integer/time rules reuse bounded unsigned versions and RFC 0047's integral Unix seconds. Root-chain count, expired-intermediate handling and the durable progress boundary remain open.

### Signed index binding and provenance

Sign a per-package snapshot containing format/schema, registry/origin/name, root version, monotonic snapshot sequence, issued/expires, exact index SHA-256/length, a complete RFC 0056 checkpoint, and at most 128 publication observations keyed by version. The existing index remains its original bounded canonical artifact; do not embed its possible 2 MiB JSON inside another signed JSON string. The independent maximum-width experiment confirms that proposed observation fields fit the candidate payload limit without depending on short package names or timestamps.

An observation contains version, principal, nonsecret credential ID, request ID, publication audit hash/sequence and admitted time. The producer derives it from actual, completely verified publish history and binds it to the index's immutable release identity. Describe this claim as registry-observed authenticated publication. It does not authenticate a human author, an external source repository, a build platform or a reproducible build. The issuer's signature authenticates only claims authorized by its independent root.

### Freshness, replay and consumption

New admission requires a usable current root, correct registry/origin/name, authorized signature threshold, exact index/descriptor/archive binding, issued <= now < expires, and no regression below retained host-time or metadata floors. Candidate release lifetime is at most seven days and no later than root expiry. A lower sequence refuses; equal sequence requires identical signed payload bytes. Root replacement must preserve package floors and all previously observed authority changes. A fresh check before durable admission/source release must not use caller-controlled timestamps.

Persist authenticated metadata/root progress before payload I/O; a failed payload download may leave a higher floor but never grant source consumption. Cached bytes alone are insufficient. A requested exact digest alone is also insufficient to replay a yanked release: require a matching durable record of prior authenticated admission. Newly observed quarantine refuses replay; new selection excludes yank and quarantine. Revocation can only act on information the consumer has learned, and expiry bounds how long withheld updates may remain usable. Define current-root treatment of older signatures and offline evidence before acceptance.

Trust state is distinct from a writable byte cache. Tampered or missing committed trust records fail closed; reopening cannot recreate a bootstrap state over corruption or reset a floor. Successful verification is the sole production constructor for `ConsumedArchive`. The existing isolated unsigned constructor stays test-only. No package manifest, lock, receipt or downloaded metadata grants transport or filesystem authority.

## Execution modes

Source semantics and VM/native/bytecode/component ABIs do not change. F03/F04 must consume a real signed dependency through the existing graph and execute it in VM/native on macOS and Windows, including exact offline replay and adversarial trust refusal. Verification must not execute guest code or call guest providers.

## ABI and compatibility

Add independently versioned trust formats without silently changing index, archive, schema-3 lock or toolchain-root meanings. The existing `Binding` policy ID already includes registry, origin and root digest; successor handling must keep the initial policy digest stable. New formats fail closed on unknown schemas. Any cache/trust-store format elaboration must be accepted before implementation, with explicit migration and recovery consequences. No automatic origin-change or trust-reset migration is proposed.

## Diagnostics and tooling

Use structured bounded trust diagnostics without key material or arbitrary signed-record dumps. Distinguish unavailable evidence, authentication failure, expired/revoked trust, rollback/conflict, capacity and uncertain durable publication. Production command/API configuration must make independent trust, state/cache and network inputs explicit; the exact CLI integration remains an open design item. Existing local-only commands retain their behavior.

## Performance and cost model

Bound root-chain steps, keys, signatures, metadata bytes, versions, records and retained physical storage before work. Verify small authenticated metadata before reading index/archive payloads. Reuse exact retained bytes and the existing graph's aggregate limits. The service must charge signed metadata maps and per-request buffers against its existing budget, rather than retain 1,024 maximal envelopes without reservation. The verifier must account for malformed decoding and root-chain overlap. Encoding experiments establish record-size feasibility only; allocation, latency and durable-storage costs require implementation evidence.

## Security and trust boundaries

Compare each candidate control against RFC 0045 R07–R09, R12 and R16. A compromised release threshold can sign malicious content until a consumer learns revocation or trust expires. A compromised root threshold defeats that trust domain. A local trusted-state writer or incorrect initial trust/clock is outside the protection of signatures; an untrusted cache is not. A retained checkpoint can expose contradiction with a chosen anchor, but neither it nor signed metadata proves global completeness or non-equivocation. Honest provenance can describe malicious source, and no signature confers execution authority.

## Alternatives

- **No change / HTTPS plus hashes:** leaves independent issuer trust, authenticated succession and signed rollback/freshness unresolved.
- **Reuse the toolchain verifier unchanged:** reuses a reviewed primitive but its immutable-root policy cannot provide required registry succession. Share narrowly suitable primitives without treating installer policy as registry policy.
- **Full TUF:** separates root, targets, snapshot and timestamp roles, binds metadata versions/hashes, and specifies authenticated root continuity and rollback/freeze defenses. This is the strongest mature alternative. A narrower package snapshot profile reduces roles and implementation surface but sacrifices role isolation and does not inherit TUF's security or interoperability claims. Acceptance requires an explicit obligation comparison, not a claim of equivalence.
- **Public transparency / keyless build attestations:** useful for other threat assumptions, but needs witness/issuer/builder trust and accurately scoped provenance. It cannot substitute for explicit package identity, current revocation or durable consumer floors.

## Implementation plan

F01 resolves this draft's open decisions, records independent vectors and accepts the contract. F02 implements the explicit producer and verifiable publication claims. F03 implements bounded verification, durable trust/evidence retention and the sealed production source adapter. F04 covers succession/revocation/tampering and the signed both-host journey. Re-audit each selected outcome before implementation; expose any work exceeding 1–3 points instead of hiding it in the current estimate. E08 remains the sole doing implementation delivery during this preparation; no gate or delivery completes here.

## Test plan

Use public RFC 8032 vectors and fixture-only keys, distinct/duplicate/unknown/weak signers, old/new threshold combinations, domain substitution and noncanonical signatures. Exercise root continuity, expired intermediates, revoked old keys, wrong origin/name, same-sequence substitution, clock regression, expiry boundaries, signed-index mismatch, descriptor/archive tampering, forged publication observations, cache corruption, known quarantine and prior-admission requirements for yank replay. Inject real storage/process interruption around every floor/receipt commit and refuse payload/source execution before proof. Require full baseline and latest-head both-host CI for implementation; model success alone completes no implementation requirement.

## Open questions

- Finalize full TUF versus the narrower candidate with a threat-obligation comparison and explicit compromise-recovery limits.
- Fix the root-chain step cap, expired-root succession and any root-only progress API so long-offline recovery cannot silently reset floors or perform unbounded work.
- Fix signed-artifact layout, publication/serving atomicity and synchronization with the mutable index, including newly observed quarantine.
- Fix durable trust-state and cache evidence ownership, current-root verification of prior admissions, lock order, bounds and crash semantics.
- Specify the production source/CLI integration and exact initial root-policy digest; no unsigned compatibility escape hatch is permitted.

## Sources

- [TUF specification 1.0.37](https://theupdateframework.github.io/specification/latest/), last modified 2026-10-08; verified 2026-10-31 JST. Roles, authenticated old/new root thresholds, metadata hash/version binding and rollback/freeze obligations.
- [SLSA v1.2 build provenance](https://slsa.dev/spec/v1.2/build-provenance), approved specification; page publication/update date unavailable, verified 2026-10-31 JST. Builder trust and the scope of provenance assertions; no SLSA level is claimed here.
- [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032), published 2017-01; verified 2026-10-31 JST. Ed25519 and public vectors.
- [ed25519-dalek 3.0.0](https://docs.rs/ed25519-dalek/3.0.0/ed25519_dalek/struct.VerifyingKey.html), released 2026-09-21; separate page update date unavailable, verified 2026-10-31 JST. Already pinned; strict verification and weak-key refusal.
- [RFC 0042](0042-signed-toolchain-installation.md), last updated 2026-10-30; [RFC 0045](0045-registry-threat-model.md), [RFC 0049](0049-registry-package-integration.md), [RFC 0050](0050-registry-cache-and-offline-replay.md) and [RFC 0056](0056-registry-retained-checkpoints.md), last updated 2026-10-31. Existing project contracts, not external security certifications.
