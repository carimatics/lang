<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0049-registry-package-integration.md -->

# RFC 0049: Registry dependencies in frozen package graphs

[日本語版](../ja/rfcs/0049-registry-package-integration.md)

- Status: accepted
- Authors: Codex, under the owner's experimental design delegation
- Created: 2026-10-31
- Last updated: 2026-10-31
- Tracking issue: M5-D04 and M5-D05
- Supersedes: None; extends RFCs 0034, 0035 and 0046
- Superseded by: None

## Summary

Add manifest schema 2 registry edges and lock schema 3 for a package graph containing registry sources. Reuse the existing frozen graph, source provider and compiler pipeline. Registry identity is an independently configured service and trust-root policy, never a cache pathname. Downloaded source stays immutable and cannot acquire filesystem-edit authority.

Existing manifest schema 1, local lock schemas 1/2, source edition 2026, bytecode and native formats keep their meanings. D04 first proves the complete dependency execution path with isolated unsigned test fixtures. Production consumption still requires F01–F03; there is no unsigned CLI switch or fallback.

## Motivation

RFC 0046 requires an explicit manifest/lock elaboration before D04. The current graph requires filesystem metadata and directory handles for every package, whereas an admitted archive is already an owned, immutable inventory. Extracting it and synthesizing a local manifest would confuse provenance, source identity and edit authority.

The 2026-10-31 local experiment publishes a real archive, reads its index/archive over verified HTTPS, removes the service, then executes the frozen dependency through the existing VM and native JIT; both return 42. Two explicit source namespaces return 84 while preserving original CRLF/UTF-8. An undeclared dependency produces `SRC0013` without a graph. URL-shaped diagnostic paths fail existing MIR `InvalidPath` validation. These are feasibility results, not manifest/resolver/offline or Windows implementation evidence.

## Goals

- Resolve exact declared local and registry dependencies into one bounded immutable graph.
- Preserve original bytes, package namespaces, spans, target conditions and VM/native equivalence.
- Pin independent service/root policy, source and compiler identities in a canonical lock.
- Preserve existing local behavior and keep registry inputs read-only.

## Non-goals

Version ranges, a dependency solver, build scripts, automatic origin discovery, registry-to-local edges, archive extraction, a new language/backend, signing policy, a production trust root and a public unsigned consumer are excluded. D05 implements cache publication/offline replay; F01–F03 separately establish authenticated production consumption.

## Guide-level design

Schema 2 retains schema 1's package/workspace/tests fields and bounds. Every dependency explicitly selects one closed source form:

```json
{
  "schema": 2,
  "package": {"name": "app", "version": "1.0.0", "edition": "2026", "entry": "src/main.lang"},
  "dependencies": {
    "helper": {"source": "local", "path": "helper", "version": "1.0.0"},
    "math": {"source": "registry", "registry": "example", "name": "math", "version": "1.0.0"}
  }
}
```

Both forms may have the existing optional `targets` array. Registry aliases may differ from package names; local aliases retain the name-equality rule. Versions remain exact. Root-level `mod math;` selects only that declared active edge. No manifest URL, credential, trust root or cache directory grants network/filesystem authority. A trusted caller supplies explicit service/root-policy bindings and a source adapter; an absent binding refuses before network lookup.

Existing local callers continue to work. A caller lacking authenticated registry admission refuses a registry graph with a package diagnostic. A checksum, HTTPS success, a lock or an archive descriptor cannot satisfy that admission by itself.

## Reference semantics

### Manifest admission and resolution

Schema 1 retains its exact grammar. Schema 2 requires `source` on every dependency: `local` allows only `source,path,version,targets`; `registry` allows only `source,registry,name,version,targets`. Required fields are never inferred. Optional fields retain omission-versus-null rules. Reject duplicate/unknown fields, unsupported schemas/editions/targets and malformed names/versions/paths under the existing 64 KiB/128-entry bounds. Registry identifiers use RFC 0046's grammar. Schema 2 may reference schema-1 local packages; registry archives remain RFC 0046 schema 1, not manifest schema 2.

The host binding maps a registry identifier to exactly one `ServiceIdentity` and a 32-byte root-policy SHA-256. Its key must equal `ServiceIdentity::registry()`. The digest identifies the independent trust configuration whose exact authenticated computation is defined by F01; accepting arbitrary bytes is not proof of trust. Conflicting bindings refuse. Neither a descriptor nor a lock may add or replace a binding.

Resolve active edges in sorted alias order. Local edges retain capability-relative capture and exact name/version checks. Registry edges select the exact requested release from the configured service, excluding yanked/quarantined versions for new resolution. Descriptor edges additionally require their exact `archive_sha256`; registry packages cannot introduce local path edges. Check all index/archive duplicated fields before graph admission. One service/root-policy/name/version locator must resolve to one archive; conflicting digests refuse instead of creating two identities. Shared diamonds reuse the same admitted package. Distinct origins or root policies remain distinct even when every source byte matches. Retain cycle rejection, bounded iterative traversal, 256 packages/modules, 4,096 edges and 16 MiB source accounting; count repeated visits against edge/traversal limits and all retained archive source files against the graph's source inventory limits.

No dependency code runs during resolution, hashing, parsing or type checking. The caller's original execution controls and host grants remain required after compilation.

### Frozen inputs and logical source paths

Represent local capture and admitted registry archives as distinct input kinds. A registry package retains its original descriptor and source bytes, not a synthesized `Lang.json`. Original filesystem metadata/handles belong only to local inputs. Registry source identity includes the complete admitted release identity and its original path; it never uses fabricated device/inode values. Capture success or failure remains frozen in the graph's existing epoch. Later analysis neither reopens a cache path nor refreshes a registry.

Local-only graphs keep existing source keys/displays. A graph with registry inputs uses disjoint logical namespaces for all compiler source paths:

- Local package key: `local` for authority-relative root `.`, otherwise `local/<root>`; source key `local/<original-authority-relative-path>`.
- Registry package key: `registry/<policy-id>/<name>/<version>/<archive-sha256>`; source/descriptor keys append the original admitted archive path.

`policy-id` is lowercase SHA-256 of `LANGREGISTRYPOLICY1` plus NUL, followed by registry identifier, canonical origin and lowercase root-policy digest, each prefixed by its UTF-8 byte length as little-endian u64. This spelling is a logical source namespace, not a cache layout or authority. The typed origin record and original local input binding control every actual file operation. Never strip a string prefix to recover filesystem authority.

Source scopes still restrict module discovery to the admitted entry/test directories, and every package's `root::` remains local. All retained files are bounded and pinned even when not loaded by the selected entry. Original UTF-8 bytes, CRLF and spans survive lowering. The logical paths satisfy the existing portable MIR path contract without introducing URLs or changing the bytecode schema.

### Lock schema 3

A graph containing registry inputs uses schema 3. Local-only graphs continue to use schemas 1/2, including when a local manifest uses schema 2. Lock versions and manifest versions remain independent. Schema 3 is closed UTF-8 JSON, two-space indentation and one final LF, within the existing 1 MiB limit and a nesting bound of 16. Fields in order:

`schema`, `target`, `compiler_version`, `compiler_build_id`, `root`, `workspace`, `packages`.

`root` is the selected local package key. `workspace` is mandatory: null for standalone, otherwise the existing schema-2 workspace record/checksum using its original authority-relative paths. Packages sort by logical package key in UTF-8 byte order. A package has fields:

`id`, `origin`, `name`, `version`, `edition`, `metadata`, `sources`, `dependencies`, `bindings`, `checksum`.

The two closed origin records are:

- Local: `kind` (`local`), `root` (original authority-relative directory).
- Registry: `kind` (`registry`), `registry`, `origin` (configured canonical HTTPS origin), `root_sha256`, `archive_sha256`, `archive_bytes`, `descriptor_sha256`.

`metadata` has `kind` (`manifest` or `descriptor`), `path` (logical key), `sha256` of original bytes. Sources have `path,sha256`, sorted by path. Active dependencies have `alias,id`, sorted by alias. Bindings retain schema 1's `kind,scope,path,target`, sorted by `(scope,kind,path,target)`, using logical keys. Every referenced package/source/scope must exist and agree with its typed origin. Registry metadata/sources must exactly match the admitted archive; local bindings retain their original captured alias relationships.

`checksum` is lowercase SHA-256 of `LANGPACKAGE3` plus NUL followed by the canonical JSON encoding, including final LF, of that package record with the `checksum` field omitted. Its metadata/source digests pin original bytes; origin, edges and bindings are also covered. This does not authenticate the record. Document an independently calculated canonical fixture before implementing the schema-3 writer/replay path. Old schema-1/2 fixtures and checksum algorithms do not change.

The [independent fixture](../../tests/fixtures/package-lock/schema3-registry.json), with its literal `<compiler_build_id>` placeholder, is 3,619 bytes and has SHA-256 `82d2766f6a3593ca68448c9a20da438f601d89af3ebd4ff326f41eeab8d5c91b`. Only that placeholder is replaced by the actual compiler identity in replay tests. [Original inputs](../../tests/fixtures/package-lock/schema3-registry-inputs/) retain the local manifest/source and registry descriptor; `math/source-bytes.json` stores the exact source strings with escaped CRLF solely as test input storage, not a new archive format. Rust archive admission reproduces the 4,608-byte archive's exact hash. Independent JavaScript/Python calculations agree on policy ID `21ccc6bc6516ce62c75c6f34d3990e619db61fc8000ce53d452940845371d0c9` and package checksums `3cec99cdc1c8eadb7fd8badaf0a627d371821de53aba25943f77c134d2de2538` / `ac38271807a61aad98e890479af04eaf9e180f66aa5fe2c52e6e2896d189e6bd`. The root digest is synthetic fixture input, not a production trust root. Local compilation of these logical keys returns 42 on VM/native JIT; a local package with the complete registry-shaped path suffix remains distinct and the combined fixture returns 84. Schema-3 production encoding/replay is still unimplemented.

Structural lock validation establishes no replay/trust authority. Capture derives the complete record from the same frozen graph used by analysis; locked replay compares exact canonical bytes and independently verifies current required trust evidence. A changed source, binding, origin/root, target/compiler or selected workspace refuses without rewriting old output. Do not reconstruct any of these from a cache path.

### Publication and offline continuation

D04 captures/admit-checks schema 3 without adding an implicit cache. D05 must grant cache reads/writes separately, verify exact bytes on every reuse, and publish a complete immutable generation before its root lock commit point. Offline mode performs no DNS, network or credential lookup, refuses missing bytes/evidence and never changes version/origin. Exact previously authenticated replay may retain a yanked release only under RFC 0046 and F01's freshness/revocation rules; newly observed quarantine refuses. F01–F03 define the signed evidence and its durable retention before production replay is enabled.

## Execution modes

Use the existing package module provider, binding, type analysis and shared typed MIR. VM, JIT and linked native consume the same frozen program and original execution grants. Registry metadata cannot supply a native import, allocator, task authority, callback or component binding. D04 must execute an actual downloaded dependency in VM and native mode on macOS AArch64 and Windows x86-64. Native SDK/compiler matching is unchanged.

## ABI and compatibility

Manifest schema 2 and lock schema 3 are explicit experimental package-format additions. Old readers reject them; existing schema-1 manifests and local lock schemas 1/2 keep exact replay. There is no guessed or automatic migration: add explicit source tags when changing a manifest to schema 2, then perform an authorized unlocked resolution. Preserve previous files on refusal. Source edition, language/native ABI, component C v1, MIR, bytecode and SDK formats are unchanged.

RFC 0035's source-snapshot archive profile 1 remains a local-graph format; reject hybrid graphs rather than silently broadening that profile. RFC 0046 portable registry publication is a different archive contract. A later snapshot-profile extension needs its own compatibility decision.

## Diagnostics and tooling

Retain original compiler diagnostic codes/spans and the existing package errors for schema, field, limit, authority, graph and lock failures. Explain absent bindings, untrusted consumption and exact-identity conflicts without echoing remote response text or credentials. Logical registry paths identify frozen source; they are not navigable physical paths by authority alone.

Read-only semantic tooling may use the frozen graph. Formatting/checking selects local editable sources while retaining registry dependencies in semantic proofs; an explicitly requested registry edit refuses. Local edits use original local paths/metadata and the existing revalidation/publication boundary. Registry overlays or operations that require a physical file binding must refuse unless they have an explicit immutable-source adapter; they cannot turn a logical key into a filesystem lookup. Existing local editor/LSP/DAP behavior remains unchanged. Production registry tooling must carry the same authenticated source context as build/run, not discover one from a document URI.

## Performance and cost model

One admitted package is retained per complete identity. Shared diamonds may share owned archive storage; different policies/origins remain separate semantic identities and count against graph budgets. Account source bytes, metadata, paths, edges and traversal before retaining unbounded work. Logical namespace/hash metadata is additional bounded work, not a speed claim. Cache reuse cannot skip archive/hash/trust checks. No extraction or second source parse from mutable files is required.

## Security and trust boundaries

Keep TLS/index/archive integrity, independent package trust, network/cache grants and guest execution authority separate. The production consumed-archive type has private construction restricted to the F01–F03 verifier; a caller cannot promote an arbitrary admitted archive to authenticated consumption. A `cfg(test)` fixture constructor may supply isolated unsigned archives for D04/D05 unit journeys; it is absent from production builds and cannot be selected by CLI/config/lock metadata. Unknown/missing trust evidence refuses consumption.

Keep source origin typed through capture, locks and tooling. Path spellings and synthetic registry identifiers never become filesystem-edit capabilities. Reject cycles, conflicting immutable versions, undeclared imports, wrong origins, aliases outside source scopes, hostile metadata and aggregate excess before execution or publication. No new unsafe code or native sandbox assumption is needed.

## Alternatives

- No change/manual download cannot provide a declared, replayable registry dependency.
- A separate library/compiler pipeline duplicates package identity and tooling proofs; reuse the existing graph/provider instead.
- Extracting archives into the local graph is the strongest simpler integration, but would require fake local manifests and careful protection against mutable cache aliases. Owned archive inputs preserve provenance directly.
- Extending schema 1 implicitly or deriving identity from virtual paths is smaller, but changes old meanings and conflates presentation with authority. Explicit versions and typed origins retain the independent boundaries.

## Implementation plan

1. D04: implement strict schema-2 edges, typed local/registry graph capture, explicit authenticated-source adapter, logical namespaces and the independently pinned schema-3 fixture/capture. Execute a real HTTPS dependency and transitive/shared graph through existing VM/native paths on both hosts; preserve local regressions and readonly-source boundaries.
2. D05: implement separately granted immutable cache/lock publication and exact offline replay, including interruption, concurrent reuse and conflicting identity refusal.
3. F01–F03: provide independently verified production trust/root/receipt construction before enabling any production registry-consuming CLI path. Existing D-stage fixtures never become an unsigned fallback.

This elaborates existing deliveries; it adds no prerequisite row or effort point. D03 remains the only implementation delivery doing until its publication gates pass.

## Test plan

Cover closed schema dispatch and old fixtures; exact transitive/target-conditioned dependencies; shared diamonds and same-name different-origin/root identities; cycles, conflicting digests and undeclared imports; source/path namespace collisions; original CRLF/UTF-8 diagnostics; immutable source after registry/cache removal; aggregate limits; local-only edit authority; canonical schema-3 fixture and every pinned input mismatch. Run actual index/archive service-to-consumer VM/native fixtures on both supported hosts and the full baseline. D05 adds no-network offline proof and filesystem crash/race cases. F01–F04 add authenticated production journeys. Passing an unsigned fixture proves none of those later trust gates.

## Open questions

Accepted under RFC 0001's experimental delegation on 2026-10-31 JST. The original-byte archive/namespace execution and independently calculated fixture support the integration design. Typed input origins, explicit physical bindings, private consumed-archive construction and versioned metadata preserve the accepted authority/compatibility boundaries. No blocking D04 design question remains. The implementation, both-host graph/refusal tests and full validation are still required; acceptance proves none of them. F01's root-policy computation and signed evidence remain separately gated, and performance/broader editor integration remain unmeasured. No production capacity claim follows from the local experiment.

## Sources

- [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md), last updated 2026-10-29: local manifests, graph bounds, lock schemas and output publication.
- [RFC 0035](0035-source-package-snapshots.md), last updated 2026-10-29: distinct local source-snapshot archive contract.
- [RFC 0046](0046-registry-protocol-and-recovery.md), last updated 2026-10-31: immutable archive/edge identity, network limits, D04 elaboration and trust/offline gates.
- Repository implementation at `f58659c`, verified 2026-10-31 JST: `src/package/{manifest,graph,source,lock,format}.rs`, source inventory and `crates/semantics/src/modules.rs`. This is version-specific implementation evidence, not an external standard.
- Local executable experiment `/private/tmp/lang-m5-registry-source-probe.rs`, SHA-256 `ca92d3c29cde8c2ef511a046221f045a597c1182fe3065ae8c4356c9befaddb3`, verified 2026-10-31 JST. This is local feasibility evidence only; retained repository tests must replace it for delivery completion.
