<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0050-registry-cache-and-offline-replay.md -->

# RFC 0050: Registry cache generations and offline replay

[日本語版](../ja/rfcs/0050-registry-cache-and-offline-replay.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: [M5-D05](../milestone-5.md)
- Supersedes: none; elaborates RFC 0046 and RFC 0049
- Superseded by: none

## Summary

Persist exact registry archives and complete lock-schema-3 generations in a separately granted private cache. Publish the root lock only after its entire cache generation is complete. Reopen a selected generation without network access, verify every reused byte, and feed the existing frozen graph through an explicitly supplied offline trust adapter. Cached bytes and lock checksums never authenticate a package.

## Motivation

D04 executes immutable registry inputs and captures their canonical lock, but all retained archive bytes are in memory. Restarting cannot replay those bytes after the service disappears. A directory of individually downloaded files also cannot establish that every archive required by one root lock was completely published. D05 needs one bounded cache transaction and an observable download → publish → stop service → reopen → VM/native journey.

## Goals

- Preserve exact origin/root-policy/name/version/archive identities across restart, interruption and competing writers.
- Reuse lock schema 3, archive admission and the existing graph/source/compiler pipeline.
- Keep cache mutation, root-lock output and independent trust authority explicit and separate.

## Non-goals

Dependency solving, extraction, garbage collection of retained generations, a second compiler pipeline, automatic cache discovery, production trust roots and a public unsigned/offline CLI are excluded. F01–F03 define signed receipts, their durable retention and freshness/revocation verification before production consumption is enabled.

## Guide-level design

A trusted host grants a private cache directory separately from the local source and root-lock output directories. An unlocked operation first resolves the complete graph using explicit registry bindings and sealed consumption evidence, then publishes its cache generation and root lock. A locked offline operation supplies an existing root lock and cache read authority, captures the selected immutable archive bytes, resolves the same local inputs and verifies the newly captured lock against the original bytes. Missing data, changed local input, target/compiler, origin/root policy or archive identity refuses without writing output or selecting another release.

The cache API performs no DNS, network, environment credential discovery or credential-provider call. An offline trust adapter independently consumes only explicitly granted local evidence and returns the existing sealed consumption proof. D05's isolated unsigned adapter remains `cfg(test)`; a stored record cannot select it. No production proof constructor or fallback is introduced here.

## Reference semantics

### Private cache profile 1

Initialization exclusively creates a fresh host-selected child of an explicit directory capability; opening never initializes or resets an existing root. The only root entries are regular `LOCK`, regular `FORMAT`, and real directories `objects`, `generations`, `staging`. Refuse symlinks/reparse aliases, nonregular files, multiple hard links, unexpected entries and unsupported profiles at actual open boundaries. Serialize mutations and recovery with a nonblocking exclusive lock. Read-only capture may take a shared lock; it never cleans or writes. Account lock-file length through metadata while locked on Windows, without reading through another handle.

`FORMAT` is exactly the following UTF-8 bytes, with two-space indentation and final LF:

```json
{
  "format": "lang-registry-cache",
  "schema": 1
}
```

The independent marker is 53 bytes with SHA-256 `60488014b120ab4f7b157fb97abe6af19c7efc12b80976d43d8740083f5c368f`. Unknown or absent markers refuse; there is no inferred migration. An interrupted initialization can leave an incomplete private root requiring explicit operator removal/restoration; ordinary open never repairs it by guessing.

`objects/HASH` contains one exact RFC 0046 archive, where HASH is its lowercase 64-character SHA-256. `generations/HASH` contains one complete canonical RFC 0049 lock-schema-3 record, including final LF, where HASH is SHA-256 of those exact bytes. There is no second generation manifest or mutable cache HEAD. Stored paths are fixed namespaces plus validated digests; source paths, package names and origins are never host cache paths. The root `Lang.lock` independently selects the complete generation by its own exact digest.

The existing independent schema-3 fixture, with only `<compiler_build_id>` replaced by 64 ASCII zeroes, is 3,664 bytes with generation digest `03bf5a6f4df5180f7915ea02bab6e44693bdae4fbdf787c44ad3c97d69781360`. Zeroes are synthetic format-fixture input, not the current compiler identity. Package checksums and original archive bytes remain those in RFC 0049; executable replay must use the actual compiler identity.

### Admission and identity

Admit the full bounded inventory before mutation or cleanup. Validate every retained generation's canonical schema, digest and registry archive references; verify referenced archive length/hash, descriptor identity and original source inventory. Validate all immutable objects, including unreferenced candidates, before considering cleanup. A policy/name/version locator must bind to one archive across all retained generations; conflicting digests refuse. Distinct origin/root policies remain distinct even when their object bytes are shared. Local-only lock schemas 1/2 do not become cache generations.

Keep RFC 0046's independent 4 GiB physical-storage and 262,144-entry ceilings, charging metadata, directories, existing files, staging and peak installation space with checked arithmetic before mutation. Preserve archive/descriptor bounds and the 1 MiB lock limit. A selected generation additionally permits at most 64 MiB of aggregate retained raw archive bytes, charged before allocation/read; existing graph bounds still limit source bytes to 16 MiB and packages/files/edges independently. The extra raw-byte ceiling accommodates descriptor/TAR overhead without raising any source limit. Enumerations are bounded, including rejected entries and recovery staging.

### Publication, interruption and recovery

The derived cross-generation identity index is limited to 262,144 distinct policy/name/version locators, charged before insertion. This separately bounds metadata retained while checking immutable-version conflicts; it does not permit retaining whole archive buffers for the full cache. Existing entries remain reusable at equality.

Under the cache lock: validate the current inventory, capture the complete graph/lock, check immutable locator conflicts and peak quotas, then install every needed archive through exclusively created, fully written and synced staging files. Existing immutable names must match exact verified bytes; never replace a conflicting object. Install the canonical generation only after every reference is complete. Only then stage, sync and atomically replace the root lock through its separately supplied output capability. The root replacement is the operation's visibility point; cache installation alone does not report a published root.

Staging names are fixed kind plus digest components for objects/generations; they cannot contain package-selected paths. Before root replacement, interruption leaves the old root lock. After replacement, the root references a complete immutable generation. A failure whose root replacement may have occurred reports an unknown outcome; reconcile the same desired lock bytes against the root and complete generation instead of changing version or blindly starting another identity. Exact retries reuse immutable bytes. Competing writers either obtain the lock or return busy, then revalidate after acquiring it.

Recovery never promotes a staged/highest generation, reconstructs a missing root lock, resets trust state or prunes a retained complete generation. After full admission, mutation recovery may delete only recognized bounded regular staging files and complete digest-verified objects unreferenced by any retained generation. Complete generations left before a root commit remain retained, since external root locks are not enumerated. Unknown/corrupt immutable files refuse before cleanup. Root-directory stage files remain under the existing root-lock output contract; cache recovery gains no authority to sweep source directories. File sync and same-filesystem atomic replacement are required; cross-platform power-loss durability is not promised. Missing/torn data after power loss must be detected and refused.

### Frozen offline continuation

An explicit root lock selects one exact admitted generation. Capture owned archive bytes under the cache lock, verifying length/hash and complete identity on every reuse; later graph execution never reopens mutable cache paths. The caller's independent registry bindings must match the lock's typed origin/root-policy records. A cache adapter exposes only the selected generation's exact releases, not arbitrary objects or newer generations. Resolve through the existing Graph, then require exact canonical `Lock::verify` before execution/output publication. Structural admission alone is insufficient.

Production trust evidence remains separately governed by F01–F03. Before a generation becomes consumable in that profile, the verifier must independently authenticate and durably retain the required evidence, and offline replay must enforce its current receipt/expiry/revocation policy. Cache profile 1 stores source bytes and locks, not an invented signature or trust-receipt format. Missing evidence refuses even when all cache hashes match. Exact authenticated yank continuation and newly observed quarantine follow RFC 0046; D05 does not invent a freshness exception.

## Execution modes

The same captured source provider, typed MIR and selected target feed VM/native execution. Cache identity does not grant guest filesystem, network, process or native-plugin authority. No Wasm lowering, ABI or execution semantics change is introduced.

## ABI and compatibility

This adds an experimental private cache profile, not a manifest/lock/archive schema change. Existing local lock schemas 1/2 and snapshot profile 1 retain their behavior. Old readers refuse the new marker; there is no automatic migration, ambient cache adoption or trust reset. Future trust-receipt formats need their own accepted contract before production enablement. Native/component ABI, editions, bytecode and serialized MIR are unchanged.

## Diagnostics and tooling

Use REG0001 for malformed/unsupported cache records, REG0002 for quotas, REG0005 for immutable identity conflicts, REG0006 for byte/trust mismatch, REG0007 for corrupt/incomplete persistent state and REG0008 for busy/I/O. Preserve typed definitely-uncommitted versus unknown-publication outcomes. Package source/lock diagnostics retain their existing codes when crossing the package boundary. Messages are bounded and never echo credentials. Semantic tooling continues to use frozen inputs; cache paths never become editor write authority.

## Performance and cost model

Full bounded inventory validation can be linear in retained storage and is not a throughput promise. Deduplicate object verification within one inventory capture, but reverify actual bytes on subsequent reuse. Selected archive buffers and frozen source inventories are owned and bounded; avoid extracting or duplicating an entire cache in memory. No benchmark claim is made before measurement.

## Security and trust boundaries

Treat cache bytes as hostile. Digest names and closed canonical records prove integrity/identity consistency, not authorship or current authorization. No cache/lock field grants filesystem or network authority. Keep original source bytes, typed local bindings and sealed registry evidence separate. Use the existing capability-relative, no-follow, staged-write and lock patterns without unsafe code or assumptions that native plugins form a sandbox.

## Alternatives

- No cache leaves restart/offline execution impossible.
- A generic download directory or external cache library can store bytes but cannot bind the entire frozen graph, current trust policy and root publication together.
- A mutable cache index/HEAD is the strongest competing layout; complete content-addressed lock generations avoid a second mutable visibility point and reuse schema 3.
- Extracted local packages are simpler for filesystem tooling but lose typed immutable origin and invite cache edits; owned admitted archives preserve D04's boundary.

## Implementation plan

Within existing D05: implement profile admission/capability storage and exact generation publication, then an isolated HTTPS-to-cache-to-offline VM/native journey, then real process-exit and competing-writer cases. Keep D04 as the sole doing implementation until its publication gates pass. This preparation adds no delivery or point. F01–F03 subsequently supply authenticated durable evidence and production adapters without an unsigned fallback.

## Test plan

Local implementation checkpoint, 2026-10-31 JST: the host `Cache`/`Captured` path passes real HTTPS-to-offline VM/native execution, twelve initialization cuts, twenty publication cuts (eighteen old roots/two complete new roots), same-identity unknown-outcome reconciliation and competing processes in both variant orders. Hostile inventory refusal precedes cleanup; read-only capture preserves staging and exact reused bytes/policies. These isolated unsigned fixtures do not establish authenticated production consumption. The frozen full baseline and latest-head both-host CI/publication remain required before delivery completion.

Check the independent marker/generation digests, malformed/unknown/missing entries, canonical bytes, source/hash/identity mismatches, same bytes under distinct policies, conflicting immutable versions and exact quota boundaries. Stop the real service before reopening the cache and executing 42/84 on VM/native; prove no network/credential path is called. Remove/corrupt cache bytes, alter local inputs or roots/target/compiler and require refusal without changing prior output. Exercise every create/partial-write/sync/install/root-replace process boundary, exact retry, unknown outcome and competing processes on both hosts. Verify corruption before cleanup, alias refusals and frozen execution after cache removal. Run the full compiler baseline and latest-head CI; unsigned fixtures do not establish G6 trust.

## Open questions

Accepted under RFC 0001's experimental delegation on 2026-10-31 JST. Rationale: content-addressed complete lock generations, explicit capabilities, exact revalidation and a single root publication boundary satisfy the existing D05 outcome without another source model or mutable index. The marker and synthetic generation digests are independent calculations; the local implementation checkpoint does not replace full/both-host verification or performance measurement. F01's signed evidence/freshness policy remains a separate gate, not a silent default here.

## Sources

- [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md), last updated 2026-10-29: root-lock output and local graph bounds.
- [RFC 0046](0046-registry-protocol-and-recovery.md), [RFC 0048](0048-registry-state-and-publication.md) and [RFC 0049](0049-registry-package-integration.md), last updated 2026-10-31: archive/cache quotas, immutable publication/recovery and frozen package identity. Verified 2026-10-31 JST. This RFC's layout and raw-generation bound are project decisions; no new external specification or durability claim is made.
