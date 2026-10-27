<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0003-versioned-component-contracts.md -->

# RFC 0003: Versioned component contracts and capability imports

[日本語版](../ja/rfcs/0003-versioned-component-contracts.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-16
- Last updated: 2026-10-27
- Tracking issue:
- Supersedes:
- Superseded by:

## Current scope

**Verified 2026-10-27 JST.** This remains the accepted component policy. RFC 0027 implements the selected production IDL, rich values, typed grants, registry/code-owner retention and failure policy; RFC 0033 stabilizes only the public C v1 boundary. The earlier schema-2 experiments remain historical compatibility evidence, not the production schema selection. Production LCMP is schema 3. WIT interoperability, package-manager range spelling and future authority delegation remain separate follow-ups.

The [M3 exit audit](../milestone-3-capabilities.md) and [current review](../review-2026-10-27.md) provide current evidence. Implementation source has no embedded publication/update date. Dated checkpoints and original acceptance decisions are retained as historical records.

## Summary

Define a backend-neutral, versioned component contract that describes package identity, scalar function imports and exports, and the capability required by each import. Keep embedding-local host-function identifiers outside the portable metadata. Before execution, validate the contract against explicit capability grants, logical import bindings, and the typed host-function registry, producing a validated wrapper or a structured error.

Production follow-up (2026-10-22 JST): accepted [RFC 0027](0027-production-component-interface.md) defines strict JSON IDL v1, rich LCMP schema 3, typed value frames and a separate component C ABI v1, including shared/exclusive access, original-context callbacks and cancellation. This resolves the production policy beyond this RFC's scalar/resource prototypes; schema 1/2 and C profile 1 keep their existing meanings and restrictions. Acceptance is not implementation or stabilization. Source/MIR, adapters, grants, artifacts/native packages and both-host conformance remain explicit M3 deliveries.

The executable prototype supports `i64 -> i64` host imports, records `i64` and `bool` scalar function types, provides a bounded experimental schema-1 scalar-metadata codec, provides a generation-tagged table for owned resources with explicit shared and exclusive borrow tokens, and traces backend-neutral canonical ownership transfer and failure cleanup. These are design evidence, not a stable serialized format or a WebAssembly Interface Types (WIT) implementation.

The RFC additionally defines normal-path scalar/resource transfer, panic/trap/destructor-failure behavior, and deterministic package/interface compatibility negotiation. In-memory negotiation, exact lock replay, general instantiation, host-value integration, and a straight-line nominal owned-resource path through VM, JIT, macOS AOT, and Windows MSVC AOT execution are executable. The broader production adapter ABI remains downstream work. The repository owner accepted this RFC on 2026-10-17, including its candidate schema-2 serialization and forward-compatibility policy as the implementation baseline; acceptance does not declare a stable public format or WIT compatibility.

## Motivation

The numeric `HostFunctionId` registry proves typed callbacks but does not express a portable component identity, named imports and exports, compatibility version, or authority requirements. Putting process-local numeric IDs into component metadata would also couple a supposedly portable contract to one embedding instance.

The component boundary needs a narrow executable model before resource handles, canonical lowering, and a stable C embedding API are fixed. This proposal establishes that first separation: portable declarations name requirements, while the embedder independently maps those names to local host registrations and grants authority.

## Goals

- Distinguish metadata schema evolution from component package versioning.
- Keep component declarations independent from MIR, VM, JIT, AOT, and process-local host IDs.
- Require every host import to name an explicit capability.
- Validate names, duplicate declarations, supported signatures, grants, bindings, and registrations before execution.
- Preserve successful validation as a distinct type carrying resolved local bindings.
- Reject stale, repeatedly released, or illegally borrowed resource handles without relying on Rust references crossing the boundary.
- Specify canonical scalar/resource call representation and normal-path ownership transfer without exposing runtime handle layout.
- Define backend-neutral panic/trap containment and deterministic, at-most-once destructor-failure behavior after ownership transfer.
- Define deterministic package/interface version negotiation without treating a SemVer claim as proof of structural compatibility.
- Return structured, deterministic failures at the trust boundary.

## Non-goals

- Stabilizing a serialized IDL or wire encoding.
- Claiming WIT source or binary compatibility.
- Defining strings, lists, records, variants, options, results, or asynchronous functions.
- Serializing live resource handles into package metadata or stabilizing a runtime handle bit layout.
- Recovering from a process abort, permitting a native unwind or foreign exception to cross the component boundary, or defining asynchronous cancellation.
- Stabilizing the C embedding ABI, native calling convention, or symbol naming.
- General production component loading and generated export adapters beyond the scalar and straight-line owned-resource conformance fixtures.
- Resolving dependencies over the network, selecting a registry, solving source-package dependency graphs, or independently versioning interfaces within one package in the first contract.

## Guide-level design

A component declaration conceptually contains:

```text
schema = 1
package = example:counter@1.2.3

import read-clock: (i64) -> i64 requires clock.monotonic
export run: (i64) -> i64
```

The accepted resource extension uses explicit ownership at each call position:

```text
resource timer
import inspect: (borrow<timer>) -> i64 requires clock.read
export adopt: (own<timer>) -> i64
```

`inspect` receives a call-scoped loan; `adopt` consumes the caller's owner on a successful transfer commit. If `adopt` then panics, the caller does not regain that owner: the callee cleanup ledger destroys it unless callee code already moved it elsewhere. A failing destructor is reported once, remaining cleanup continues, and the affected instance becomes poisoned. This notation is explanatory and does not select the eventual IDL syntax.

An interface import conceptually names both a package/interface and a normalized version requirement:

```text
import clocks from example:time/clocks
  version >=1.2.0,<2.0.0
  requires members { now: () -> i64 requires clock.read }
```

The serialized contract stores an exact requirement or explicit lower/upper bounds, not this explanatory range syntax. At instantiation, the embedder chooses the highest authorized provider that satisfies the version bounds and every required member. A lock binding to an exact content digest overrides that search. A SemVer match with a changed signature, ownership mode, resource identity, or capability still fails.

The declaration does not contain a `HostFunctionId`. The embedder separately configures:

```text
grant clock.monotonic
bind read-clock = host-function 7
```

Validation succeeds only when schema 1 is understood, declarations are structurally valid, every binding names a declared import, every required capability is granted, every import has a binding, and the bound host function exists with a supported typed registration. The resulting validated value retains the portable declaration and resolved local IDs.

The textual examples are explanatory only. The repository-local binary codec described below is an experimental fixture format and does not select the eventual stable component serialization.

Component resources use a separate in-memory table. An owned handle contains a slot and generation and is intentionally copyable to model an integer handle duplicated by untrusted code. A component may request shared or exclusive borrowed handles. Shared borrows may coexist; an exclusive borrow requires no other live borrow and is the only borrowed handle that permits mutation. Every borrow must be ended before the owner can release the resource. Release returns the host value, advances the slot generation, and makes the lowest reusable slot available. Reusing a copied old owner, ending a borrow twice, releasing twice, or using a borrow after its end returns a structured stale-handle error.

## Reference semantics

`ComponentMetadata` contains a schema version, package namespace, package name, three-part component version, imports, and exports. Each function declaration uses a component-specific scalar type instead of MIR or backend types. Each import additionally names one capability. `ComponentImportBindings` maps logical import names to process-local `HostFunctionId` values, and `CapabilityGrants` contains the authority supplied to this instance.

Prototype identifiers use a conservative ASCII subset. Package and function identifiers consist of lowercase alphanumeric segments separated by single hyphens, with each segment beginning with a lowercase letter. Capability identifiers consist of one or more such identifiers separated by dots. This is a project-local prototype grammar, not a claim about the complete WIT identifier grammar.

Validation is deterministic and occurs in this order:

1. Reject an unsupported metadata schema before interpreting later fields.
2. Validate package, import, export, and capability names; reject duplicate imports, duplicate exports, import/export collisions, and import signatures unsupported by the current registry.
3. Reject embedding bindings that do not name declared imports.
4. Reject the first import whose declared capability is absent. This happens before host lookup so an unauthorized component cannot use validation to discover registered functions.
5. Require one binding for each import and reject a binding whose local host function is not registered.
6. Return `ValidatedComponentMetadata` containing the metadata and resolved local bindings.

Schema 1 can validate only `i64 -> i64` imports because that is the only current host-registry shape. The metadata type records `bool` so the contract type system is not defined in terms of the registry method name, but a `bool -> bool` import is rejected until an equally typed registry entry and execution path exist. Metadata validation checks export declarations; a separate conformance fixture executes a validated `i64 -> i64` export, not arbitrary declared exports.

The experimental scalar-metadata encoding begins with `LCMP`, followed by little-endian `u32` schema version `1`, length-prefixed UTF-8 package namespace and name, three little-endian `u32` package-version parts, and length-prefixed import and export sections. Each function stores one-byte scalar tags (`0` for `i64`, `1` for `bool`); imports additionally store a length-prefixed capability. Encoding preserves declaration order. `ComponentCodecLimits` bounds total bytes, declarations per section, structural depth, and bytes per string before allocation. The decoder rejects unsupported schemas before the payload, invalid magic or UTF-8, unknown scalar tags, truncation, trailing bytes, and every exceeded bound with a structured error carrying the byte offset. Decoding constructs unvalidated `ComponentMetadata`; semantic and capability validation remains a separate required step.

The candidate versioned encoding reuses `LCMP` with little-endian `u32` schema version `2` and a one-byte document-kind discriminator. Contract kind `0` stores a package key, a full semantic version with typed prerelease and build identifiers, interface requirements sorted by complete interface key, exact locks sorted by key, and canonical resource-type identities sorted by complete package/interface/resource key. Requirement tag `0` carries a build-sensitive exact version; tag `1` carries inclusive-minimum and exclusive-maximum versions plus a canonical `0`/`1` prerelease-policy byte. Each requirement is followed by required members sorted by member name. A member stores its name, ordered parameter and result lists, and exact capability; value-type tags are `0` for `i64`, `1` for `bool`, `2` for `own<T>`, and `3` for `borrow<T>`, with resource tags followed by the complete resource-type key. Borrowed results are rejected, and every nominal resource value must reference an identity declared in the contract resource section. Digest tag `0` carries exactly 32 SHA-256 bytes. Failure-report kind `1` stores message policy `0` for fully redacted or `1` for embedding-sanitized transport, one primary record, an ordered count-prefixed suppressed-record list, and a canonical `0`/`1` poisoned flag. Each record stores category (`0` panic, `1` trap, `2` destructor), phase (`0` call, `1` cleanup), diagnostic identity (`0` boundary panic, `1` boundary trap, `2` destructor failure, `3` instance poisoned), and a canonical message-presence byte followed by a bounded string when present. Validation fixes each diagnostic's category and any diagnostic-specific phase, restricts suppressed records to the cleanup phase, forbids messages under redacted policy, and requires poisoning for destructor failures and the instance-poisoned diagnostic. Every count and string is little-endian `u32` length-prefixed and bounded by `ComponentCodecLimits`; all semantic-version components are little-endian `u64`, and nested member values require depth five. Construction validates semantics before encoding, and each decoder repeats validation after bounded parsing while reporting malformed headers, tags, booleans, UTF-8, truncation, trailing bytes, and limits at byte offsets. Collection sorting gives one canonical contract encoding while parameter/result and suppressed-record order remain significant. Checked-in golden fixtures, round-trip, semantic-validation, and malformed-input tests, a deterministic arbitrary-byte corpus, and dedicated libFuzzer targets cover both document kinds. Schema 2 is the accepted implementation baseline, not a stable public compatibility promise.

### Canonical scalar and resource call representation

**Source facts.** WIT distinguishes uniquely owned resource handles from borrowed handles. Passing an owned handle transfers unique ownership; a borrowed handle is a temporary loan for the duration of a call. The WebAssembly Component Model Canonical ABI lowers both handle kinds through a 32-bit boundary index and checks resource identity and active lends when lifting or dropping a handle. Its reference table reuses holes left by removed entries. Its scalar lowering uses a 64-bit lane for `s64`; boolean lowering emits `0` or `1`, while lifting treats `0` as false and every non-zero integer as true.

**Project decision.** The first canonical call subset contains `i64`, `bool`, `own<resource T>`, and `borrow<resource T>`. An `i64` preserves its two's-complement bits in one 64-bit lane. A `bool` uses one unsigned 32-bit lane, lowers canonically to `0` or `1`, and lifts `0` as false and any non-zero value as true. A resource value uses an unsigned 32-bit index into an adapter-owned table scoped to one component instance. Index `0` is invalid; subsequent boundary indices are allocated monotonically without reuse or wrap for the lifetime of that instance. Exhausting the configured allocation limit or the remaining index namespace is a structured failure. This deliberately differs from the Canonical ABI reference table's free-list reuse so that an untrusted stale numeric copy cannot alias a later resource. The adapter may reuse an internal storage slot, but its generation-tagged owner and a never-reused boundary index remain distinct. The adapter entry carries the validated resource-type identity and resolves to the runtime's generation-tagged owner or borrow token. The boundary index, internal slot, generation, borrow identifier, and host address are never interchangeable and are not serialized into package metadata. Canonical `borrow<T>` specifies lifetime and ownership only; the later memory/concurrency RFC must decide whether a given operation acquires the runtime table's shared or exclusive token.

Every call performs a preflight over the complete argument list before changing ownership. Preflight validates the function signature, capability grants, resource type identity, live generation, requested ownership mode, active lends, duplicate owned arguments, any alias that presents one resource as both owned and borrowed, configured live/total-index limits, and destination capacity. Repeated borrow-only aliases may be admitted only when the later memory/concurrency policy permits their requested access modes. Preflight reserves all destination entries and backing storage, but publishes no handle and changes no resource or ownership state; allocation or reservation failure therefore leaves the caller unchanged. Normal-path transfer then follows these rules:

1. Lowering an `own<T>` parameter removes exactly one live owner from the caller adapter and inserts a fresh owner under a single transfer commit. The commit uses the reserved destination entry, performs no allocation, invokes no user code, callback, or destructor, and cannot fail. No component-boundary operation can observe an intermediate state. Copies of the caller's old numeric index remain stale permanently because boundary indices are never reused. A borrowed, wrong-type, already moved, actively lent, duplicated, or borrow-aliased owner is rejected before the commit.
2. Lowering a `borrow<T>` parameter leaves ownership with the caller and creates a token tied to the dynamic call scope. The callee cannot release, transfer, return, or retain that token after the scope. Nested calls may reborrow it only within a child scope, and all such borrows end before the parent scope ends.
3. Lifting an `own<T>` result performs the symmetric preflight and commit from callee to caller. The callee's numeric index becomes stale only when the destination owner has been installed successfully.
4. Borrowed results are excluded from the first canonical subset because they would outlive the call scope that proves the loan. Dropping an owned boundary handle consumes that owner exactly once; destructor invocation and failure follow the boundary rules below.

A failure before a transfer commit leaves ownership unchanged. The adapter must prevent callback reentrancy while preflight reservations and a transfer commit are active. Panic, trap, callback reentrancy after commit, and destructor failure follow the boundary rules below; asynchronous cancellation remains open. VM, JIT, and AOT adapters must invoke the same preflight and table operations; a whole-program direct-call specialization is valid only when it proves observational equivalence to those checks.

**Open hypothesis.** The per-instance adapter table should make VM/native behavior deterministic and prevent internal handle-layout leakage, but its validation, reservation, monotonically consumed index namespace, table mutation, and callback costs remain measurement questions rather than sourced performance claims.

`ResourceTable<T>` owns host values. `OwnedResourceHandle` contains a table-local `(slot: u32, generation: u64)` pair and private host-side table provenance. Owned and borrowed tokens from another table are rejected before any access or mutation, even when slot, generation, and borrow identifiers coincide. Table identities are never reused; identity exhaustion prevents insertion. This provenance is not serialized and does not change the canonical instance-local `u32` boundary lane. `BorrowedResourceHandle` additionally carries a monotonically allocated borrow identifier and `Shared` or `Exclusive` kind. A released slot increments its generation before reuse; a slot whose generation cannot advance is retired. Free slots are selected in ascending slot order, making reuse deterministic. Borrow identifiers are never reused, and exhaustion is an error.

Resource operations first validate slot occupancy and generation. Shared-borrow creation rejects an active exclusive borrow. Exclusive-borrow creation rejects any active borrow. Mutable access rejects shared or inactive tokens. Ending a borrow removes exactly that token; a repeated end is stale. Release rejects any active borrow, removes and returns the value exactly once, and makes all copied owners stale. The copyable representation therefore does not imply copyable ownership.

The executable host-value bridge allocates non-zero, monotonically increasing opaque `ResourceIdentity` values and maps each identity to one generation-checked `OwnedResourceHandle` in a shared `ResourceTable<T>`. Canonical adapter entries receive only the opaque identity. Publishing the host value and canonical owner is atomic from the caller's perspective: if the adapter rejects the entry, the bridge releases the inserted value but never reuses the consumed identity. Borrow operations resolve the mapped owner through the runtime table, and canonical cleanup passes the identity back to the bridge so that the actual value is removed and returned exactly once. This prototype establishes the identity and lifecycle linkage independently from instantiation and backend execution.

General instantiation consumes a validated schema-2 contract and negotiates each canonically ordered requirement against the complete provider inventory, explicit capability grants, and its optional exact lock before publishing any instance state. The first failure identifies the requirement key and retains the structured negotiation cause. Success owns the original contract, immutable validated bindings, and a fresh `CanonicalResourceTable` configured with per-instance limits. Contract selection and adapter-state creation therefore remain a separately testable stage before the cross-mode fixture described next.

The first cross-mode conformance fixture uses the scalar result produced by the validated component call as an actual host value. VM, JIT, and macOS AOT results pass through identical schema-2 instantiation, owned publication and transfer, guest-trap cleanup, exactly-once host-value release, and redacted failure-report projection. Direct and response-file AOT links agree with the VM and JIT value, reusable-state, and report outcomes. This demonstrates host-side canonical-adapter equivalence around backend-produced data; it does not add resource types or operations to MIR, bytecode, or the native ABI.

### Boundary panic and destructor failure

**Source facts.** The WebAssembly Component Model Canonical ABI removes a resource handle from its table, rejects active lends, and only then invokes the destructor for an owned handle. Rust supports recoverable unwinding and non-recoverable abort panic strategies. `std::panic::catch_unwind` catches only unwinding Rust panics, not aborting panics, and Rust's FFI rules do not provide a portable way to catch arbitrary foreign exceptions. These are implementation constraints, not the component contract.

**Project decision.** A Rust panic payload, native exception, or backend unwind never crosses the stable component boundary. A declared error result is an ordinary value once result types exist; it is not a boundary panic. The backend-neutral failure model conceptually contains a primary record, zero or more ordered suppressed records, and a reusable-or-poisoned instance state. Each record has a stable category (`panic`, `trap`, or `destructor`), a phase (`call` or `cleanup`), a logical diagnostic identity (`component.boundary.panic`, `component.boundary.trap`, or `component.boundary.destructor-failure`), and an optional policy-sanitized message. Reentry of a poisoned instance uses `component.instance-poisoned`. The numeric encoding and message transport remain versioned ABI work.

Ownership and cleanup follow these rules:

1. Failure before a transfer commit preserves the caller's ownership. Failure after a successful commit never rolls ownership back: the caller index remains stale, because the callee may already have observed, mutated, stored, or moved the resource.
2. Before user code runs, the adapter creates a cleanup ledger for every transferred owner, temporary owner, and dynamic borrow scope. Publishing a later move updates both ledgers atomically. An owner successfully stored in persistent component state or returned to the caller leaves the current frame's ledger.
3. On every normal or failed exit, child borrow scopes close from innermost to outermost before owned cleanup begins. Remaining frame-owned resources are destroyed in reverse acquisition order. An owned result that was created but not committed to the caller remains callee-owned and is cleaned up.
4. Compiler-generated language panic and guest traps lower to an explicit backend-neutral failure path. VM bytecode uses explicit trap/cleanup control flow; JIT and AOT may use internal landing pads or status returns, but no Rust/C++/platform unwind may escape the adapter. `catch_unwind` may contain a known Rust host callback compiled for unwinding, but it is not the semantic mechanism and cannot recover an aborting panic. Disposal of an opaque caught panic payload is itself contained because its destructor may panic. Native callback registration must require foreign exceptions to be caught before adapter entry; callbacks that cannot guarantee containment belong only to the trusted-native tier.
5. A component panic or ordinary guest trap leaves the instance reusable only after the cleanup ledger is empty, no dynamic borrow remains, and adapter invariants revalidate. An adapter invariant violation, incomplete cleanup, contained host-callback panic, or destructor failure poisons the instance; later entry fails with `instance-poisoned`, while the host process remains alive. A process abort is outside this recoverable contract and remains a trusted-native limitation.

Dropping an owned resource marks its boundary owner consumed and removes it from lookup before invoking user destructor code. The destructor is attempted at most once: panic or trap cannot resurrect the handle or trigger an automatic retry, and external side effects are not rolled back. Cleanup forbids component-boundary reentrancy; an attempted reentry is recorded as a destructor failure. If a destructor fails, cleanup records the failure and continues with the remaining independent ledger entries in the same deterministic order. With no earlier failure, the first destructor failure becomes primary; otherwise the original panic or trap remains primary and destructor failures are appended as suppressed records. Additional destructor failures are also suppressed in cleanup order. Completion then poisons the instance.

**Open hypothesis.** Explicit cleanup ledgers and per-destructor containment should avoid leaks and double destruction while preserving the host process, but their code-size, steady-state, failure-path, and native landing-pad costs require implementation and cross-mode measurement.

### Package and interface compatibility negotiation

**Source facts.** WIT package names contain a namespace, package name, and optional full SemVer version; qualified `use` paths name an interface in such a package. SemVer 2.0.0 requires released version contents not to be modified, assigns incompatible public-API changes to a major-version increment, treats `0.y.z` as unstable initial development, gives prereleases lower precedence than the associated normal version, and ignores build metadata when comparing precedence. Neither a version number nor a range proves that two concrete interface declarations are structurally compatible.

**Project decision.** Metadata schema negotiation, package selection, and interface compatibility are separate checks:

- The metadata schema remains an exact supported integer. Schema 1 continues to record an exact package version but cannot express an interface requirement; adding the fields below requires a new schema and does not reinterpret existing `LCMP` bytes.
- A `PackageKey` is the exact, case-sensitive `(namespace, package)` pair. An `InterfaceKey` adds the exact interface name. A nominal `ResourceTypeKey` adds the exact resource name but deliberately excludes the package version; it is compared only after the provider passes the version requirement, preserving one resource lineage across compatible releases without equating different package/interface/resource names. Every interface in the initial model inherits its package's SemVer version; independently versioned interfaces are deferred.
- A provider declaration carries an exact SemVer version, an immutable package-content digest, and its exported interface declarations. A consumer carries an `Exact(version)` requirement or a normalized `Interval { min_inclusive, max_exclusive, allow_prerelease }` plus the required member projection. `Exact` compares the complete SemVer value including prerelease and build metadata; `Interval` uses SemVer precedence, where build metadata does not order candidates. The serialized form contains these fields directly, so runtimes do not disagree over textual range grammar.
- Tooling defaults a stable `1.0.0` or later dependency to the containing-major interval and defaults any `0.y.z` dependency to `Exact`; authors must opt in explicitly to broader zero-major or prerelease intervals. A prerelease candidate never satisfies a requirement unless `Exact` names that prerelease or the interval explicitly allows prereleases. Build metadata is ignored for precedence but retained in the selected identity.

Instantiation performs this deterministic algorithm before exposing provider inventory or patching VM/JIT/AOT call sites:

1. Validate the consumer contract and all capability grants. A component without the required grants receives the existing capability error before candidate lookup.
2. If the embedding or lock data names an exact content digest, require the exact package/interface identity, version, and digest. A mismatch is terminal; negotiation does not silently fall back.
3. Otherwise filter registered providers by exact `PackageKey` and `InterfaceKey`, then by the normalized SemVer requirement and prerelease policy.
4. Structurally validate the consumer's required projection against each candidate. Every required member name, parameter/result order, scalar type, resource type identity, `own`/`borrow` mode, and capability must match exactly. Unknown schema constructs fail closed. A provider may expose additional members because they are not reachable through the validated projection.
5. Select the remaining candidate with highest SemVer precedence. If distinct candidates at that precedence differ by build metadata or content digest, return ambiguity instead of choosing by registration or map order.
6. Produce an immutable `ValidatedInterfaceBinding` containing the selected exact version, digest, required-member mappings, capabilities, and adapter rules. All execution modes consume this binding. Registry mutation or rebinding requires fresh validation; a running instance never falls through to another provider after failure.

The compatibility promise is therefore two-layered. SemVer bounds express which releases the consumer is willing to consider, while structural projection proves that the selected release still supplies the expected observable contract. Adding an unrequested export is compatible with an existing consumer. Removing or renaming a required member, changing parameter/result order or type, changing resource identity or ownership mode, or changing a matched member's capability is incompatible regardless of the version label. Package-manager resolution determines which artifacts and lock digests are available; runtime negotiation performs no network or registry access.

The executable package-selection prototype places a canonical in-memory lockfile artifact between contract requirements and runtime instantiation. An unlocked pass records one exact SHA-256 version/digest entry for every requirement in interface-key order. Replay requires the same consumer package and version plus complete, non-orphaned requirement coverage; an external entry must equal any embedded contract lock. Every replayed entry uses the resolver's existing exact-lock path, so unavailable or digest-mismatched providers fail without compatible-version fallback. A successful selection owns the contract and immutable bindings and can create fresh bounded instance state without renegotiation. This intentionally defines no stable on-disk syntax, registry protocol, artifact fetch, or publication behavior.

**Open hypothesis.** Projection matching should permit additive provider evolution without weakening safety, but declaration size, digest/fingerprint strategy, candidate-set scaling, and the ergonomics of explicit zero-major and prerelease requirements require prototype and registry/tooling measurement.

## Execution modes

Contract/resource-table validation and package/interface negotiation are backend-neutral and run before selecting VM, JIT, or AOT execution. Every mode consumes the same immutable `ValidatedInterfaceBinding`, then applies the same preflight, adapter-table transfer, dynamic-scope borrow, cleanup-ledger order, failure precedence, and instance-poisoning rules. The prototype now represents normalized version requirements, selects and exactly locks providers, creates general schema-2 instance state, and executes one host-side resource/failure adapter fixture around VM, JIT, macOS AOT, and Windows MSVC AOT scalar results. One straight-line nominal owner also crosses MIR, bytecode, typed VM values, Cranelift JIT/AOT lanes, adapter transfer, and guest-trap cleanup on both native tier-1 paths. General resource-aware control flow and signatures, return ownership transfer, richer generated adapters, and backend landing-pad enforcement remain downstream work. Later execution paths must not independently reinterpret unchecked metadata or renegotiate after code patching.

## ABI and compatibility

The metadata schema version and package version have separate meanings. The schema version selects the shape and validation rules of the contract. The package version identifies the declared component revision. Schema 1 requires an exact supported schema value and records, but does not interpret, the package version.

`HostFunctionId` is an embedding-local binding and is deliberately excluded from portable metadata. Resource slot, generation, and borrow identifiers are runtime-local validation data, not a portable representation. The accepted unsigned 32-bit canonical resource index is an adapter-table key, not the runtime handle layout and not a serializable identity. Rust panic payloads, native exception objects, and backend unwind metadata are likewise non-portable. Schema 1 and its `LCMP` fixture continue to encode only an exact package version. Candidate schema 2 supplies explicit normalized-requirement, interface-projection, content-digest algorithm, resource-identity, and failure-report representations without serializing runtime handles or unwind payloads. Rust layout, enum discriminants, in-memory map order, symbol names, and explanatory text syntax remain unstable. An accepted RFC revision must ratify or revise the candidate tags, ordering, compatibility rules, and message policy before the boundary can be called stable.

## Diagnostics and tooling

The codec returns structured errors for byte, declaration-count, nesting-depth, and string-length limits; malformed headers, lengths, UTF-8, and scalar tags; unsupported schemas; and trailing bytes. Every codec error includes a byte offset. Contract validation separately returns structured error variants for invalid identifiers, duplicate names, namespace collisions, unsupported function types, extra or missing bindings, missing capabilities, and unknown host functions. Negotiation distinguishes unsupported schema, unresolved import, invalid requirement, no compatible version, prerelease rejection, structural member mismatch, lock-digest mismatch, and ambiguous provider. Authorized embedding diagnostics identify the package/interface, candidate version, member, and mismatch dimension; untrusted components receive no provider-inventory detail before capability validation. Resource operations distinguish stale owned and borrowed handles, conflicting borrows, release while in use, shared-borrow mutation, table capacity, boundary-index exhaustion, destination-reservation failure, and borrow-identifier exhaustion. Boundary failures expose the stable category, phase, diagnostic identity, ordered suppressed failures, and poisoned state without exposing a Rust panic payload or native exception object. Human-readable messages include locations, logical names, or handle identity, while callers can match the variant without parsing text.

Tooling should display portable import names rather than local numeric host IDs except when diagnosing embedding configuration. Resolution output should show the normalized requirement, selected exact version and digest, whether a lock forced the choice, and the first structural incompatibility for each rejected candidate. Mapping post-decode semantic failures back to byte ranges remains future work.

## Performance and cost model

Current validation is bounded by the supplied document/table limits but is not uniformly linear. Schema-1 extra-binding checks scan imports for each binding (O(B × I)); schema-2 canonicalization and negotiation use ordered collections and sorting (including O(C log C) candidate ordering), plus structural projection checks. All required capabilities are checked before provider inventory or lock lookup across the complete instantiation request. Resource slot lookup is constant time; borrow/free-slot sets add logarithmic costs. Bound candidate counts, total projections, byte sizes, and verifier work before exposing these prototypes to untrusted input. Schema-20 evidence includes one host-orchestrated application, not production thunk, unwind, or general typed-resource costs; see the benchmark caveat below.

## Security and trust boundaries

Component metadata, capability declarations, version requirements, content digests, provider registrations, and binding configuration are untrusted inputs at their respective boundaries. Validation and negotiation must complete before execution or linker/JIT patching. Authority comes from embedder-supplied grants, never from a component merely declaring a capability. Capability failure precedes provider or host-registry lookup to avoid exposing inventory to unauthorized components. Package/interface names compare canonically and case-sensitively with no fallback, prerelease admission is explicit, digest algorithm/length is validated before comparison, and lock mismatch or same-precedence ambiguity fails closed rather than downgrading.

The scalar codec checks its configured bounds before allocation, rejects malformed input deterministically, and is fuzz-targeted independently from semantic validation. The validated wrapper and resource table prove only their implemented checks. Copyable handles are treated as untrusted tokens: generation and active-borrow state are revalidated on every operation, boundary indices are never reused within an instance, and authority is not derived from possession of a stale numeric value. Preflight rejects owned aliases before mutation, and no user code runs inside a transfer commit. Panic payloads and native exception objects are untrusted and are never reflected verbatim across the boundary; message exposure is an embedder policy. Poisoning prevents reentry after destructor, host-callback, or invariant failures that may have left external state partial. The prototype implements the backend-neutral cleanup ledger but does not serialize live resources, sandbox native callbacks, preempt callback work, enforce heap limits, install backend landing pads, or make native plugins safe. Aborting panics and foreign exceptions remain outside the recoverable in-process contract.

## Alternatives

### No change

Continue passing numeric host IDs directly through prototype code. This is smaller, but it cannot represent a portable contract, compatibility identity, or explicit authority and would encourage process-local details to leak into a long-lived boundary.

### Library or tooling solution

Generate ad hoc binding tables in build tooling while leaving runtime validation unchanged. This can improve ergonomics, but malformed or mismatched metadata would still reach the runtime without one authoritative trust-boundary check.

### Adopt WIT and the WebAssembly Component Model immediately

Use WIT packages, worlds, resources, and the Canonical ABI as the first implementation. This is the strongest competing design because it offers an existing cross-language interface model and explicit import/export contracts. It is not selected yet because the project has not chosen WebAssembly as its only component representation, has no rich-value or resource runtime to lower, and still needs direct native and script execution to share one contract. The prototype intentionally preserves concepts that can later map to WIT without claiming compatibility.

### Stabilize a flat C ABI first

Expose only C-compatible scalar calls and symbol names. This is simpler for current AOT objects, but it does not provide package-level versioning, capability requirements, rich values, resource ownership, or a shared VM/native component model.

### Use raw slot indexes or host references

A raw index is smaller, but after slot reuse an old copy can silently name a different resource. A Rust reference encodes borrowing locally, but cannot be trusted when reconstructed from component data or retained across an ABI call. Generation-tagged integer handles with table-checked borrow tokens preserve explicit validation without exposing host addresses.

### Roll back ownership or abort on failure

Rolling transferred owners back to the caller after panic appears convenient, but is unsound once callee code may have mutated, stored, or moved the resource. Aborting the process on every panic is simpler and matches an unavoidable native `panic=abort`, but violates the embedded-script isolation goal. Stopping cleanup at the first destructor failure avoids secondary failures but leaks later frame-owned resources. The proposal instead keeps committed ownership, runs deterministic best-effort cleanup, records suppressed destructor failures, and poisons the affected instance.

### Trust SemVer alone or require exact versions only

Trusting an in-range SemVer label without checking declarations is fast but allows a mislabeled release to change signatures, ownership, or capabilities silently. Exact-version-only matching is safer but turns every compatible patch into lockstep churn. Structural matching without version intent accepts shapes the author did not choose to support. The proposal requires both an explicit normalized SemVer requirement and structural projection, with an optional exact lock digest.

## Implementation plan

1. Implemented and tested schema-1 in-memory scalar metadata, explicit capability grants, separate import bindings, host-registry resolution, and structured validation errors.
2. Implemented and tested generation-tagged owned and borrowed resource handles with deterministic stale-handle, repeated-release, repeated-borrow-end, and borrow-conflict rejection.
3. Implemented and tested a deterministic experimental schema-1 scalar-metadata encoder/decoder with byte/count/depth/string limits, byte-offset diagnostics, malformed fixtures, and a libFuzzer target. Unsupported schema versions are rejected before their payload; schema 2 and its forward-compatibility policy are now accepted, while public format stabilization remains future work.
4. Implemented and tested one validated schema-1 scalar contract connected to VM and JIT execution plus macOS AArch64 and Windows MSVC AOT object emission, direct/response-file linking, and process execution. General schema-2 instantiation now negotiates every requirement before publishing immutable bindings and fresh bounded adapter state. A straight-line nominal `own<T>` signature is represented in MIR and bytecode and lowered to a 32-bit Cranelift JIT/AOT lane.
5. Implemented and tested a backend-neutral trace of adapter-table `own<T>`/`borrow<T>` resource indexes, whole-call preflight and reservation, allocation/callback-free commit, non-reusing boundary indices, nested call-scoped borrows, and generation-checked host-value identity linkage. The initial MIR and bytecode validators preserve the nominal resource type and reject dropping or reusing an owner in their one-block prototype. A typed VM boundary validates nominal owned-resource values and rejects raw scalar entry, while adapter-integrated VM/JIT and macOS AOT plus Windows MSVC AOT direct/response-file execution preserve the committed destination index through guest-trap cleanup. General control-flow/resource signatures, return ownership transfer, richer production adapters, and exact source-level scalar/boolean canonical conversion remain open.
6. Implemented and tested backend-neutral post-commit non-rollback, deterministic borrow/owner cleanup ledgers, remove-before-callback at-most-once destructors, ordered primary/suppressed failures, reusable guest panic/trap exits, and conservative poisoning after host-callback, invariant, or destructor failure. A host-side VM/JIT/macOS-AOT/Windows-AOT fixture records equivalent cleanup and serialized-failure outcomes. Rust adapter cleanup also contains panicking panic-payload destruction and destructor-error formatting/destruction, continues independent cleanup, and poisons abandoned active calls. A second payload whose destruction could panic is intentionally leaked to stop recursive unwinding; aborts and foreign exceptions cannot be contained by this mechanism. Backend landing pads, foreign-exception policy enforcement, abandoned-call host teardown, and asynchronous cancellation remain open.
7. Implemented and tested exact package/interface and version-free resource keys, normalized full-SemVer exact/interval requirements, explicit prerelease policy, structural required-member projection, exact lock digests without fallback, deterministic highest-version selection, and same-precedence ambiguity rejection. A canonical in-memory lockfile now records complete selections, replays exact locks without fallback, reconciles embedded locks, and hands the selected bindings to instantiation. Stable on-disk lockfile syntax and production package-manager I/O remain open.
8. Implemented and tested candidate schema-2 `LCMP` contract and failure-report documents. The contract covers canonical package/resource identities, full semantic versions, normalized exact/interval requirements, structural required-member projections, and SHA-256-tagged exact locks. The failure report covers fixed category/phase/diagnostic tags, a primary record, ordered cleanup-phase suppressed records, poisoned state, and explicit redacted/sanitized message policy. A policy-explicit projection from runtime `BoundaryFailure` records preserves cleanup invariant traps and destructor failures without exposing unsanitized messages. Deterministic sorting where order is not semantic, checked-in golden encodings, explicit byte/item/depth/string limits, byte-offset diagnostics, semantic validation, arbitrary-byte tests, and dedicated fuzz targets cover both layouts. RFC acceptance includes this implementation baseline and its forward-compatibility policy; public stabilization remains separate.
9. Implemented an eight-series schema-20 boundary-cost harness and clean macOS AArch64 and Windows x86-64 baselines. The `representative_application` series composes two-provider negotiation, deterministic lock generation, exact-lock replay, instance publication, scalar and typed-resource VM/JIT calls, and reusable guest-trap cleanup. The 31-sample, 1,000-operation row records a 67.592 us median, 68.459 us p95, and 1.109% RSD on macOS and a 236.804 us median, 266.366 us p95, and 6.201% RSD on Windows. The host-orchestrated row includes lowering and JIT compilation and is directional application evidence, not isolated adapter overhead. The Windows run also records successful native component conformance and all four process-memory distributions.

## Boundary-cost evidence

`representative_application` is synthetic host orchestration: it executes separate fixed scalar/resource fixtures, not dispatch through the published instance's negotiated bindings. Selecting and invoking actual provider implementations and measuring generated adapters end to end remain follow-up conformance/measurement work.

Historical measurements below predate the 2026-10-17 JIT reclamation fix. The representative row includes JIT compilation, so recapture clean results before comparing current compile/application costs; historical raw files are not rewritten.

The clean schema-15 macOS AArch64 [baseline](../../benchmarks/results/2026-10-16-macos-aarch64-f163409.json) records 31 samples from commit `f163409c00d94e08cd636357e58143134e02f568`. Median and p95 costs are 286.833/316.292 ns for cloning and validating the one-import/one-export scalar metadata, 842.584/926.250 ns for validating and selecting from three providers, 0.958/1.031 ns for the direct scalar callback, 11.451/11.695 ns for logical-import lookup plus host-registry lookup and callback, 10.661/12.452 ns for generation-tagged owner insert/release, 22.731/24.259 ns for insert/shared-borrow/read/end/release, 412.334/442.417 ns for canonical owner preflight/commit/persist, and 347.250/396.250 ns for canonical borrow preflight/commit/scope cleanup.

The comparison boundaries matter more than ratios. Metadata validation includes cloning because the current API consumes metadata and produces an owned immutable binding. Negotiation validates the consumer and all providers and clones the selected projection. The validated dispatch row performs both map lookups on every operation; it is not a cached direct-call specialization. The canonical rows include preflight allocations/reservations, trace and cleanup-ledger construction, and monotonically growing boundary-index vectors, whereas the table rows reuse internal generation-tagged slots. Consequently these values are evidence for the exact prototype paths only, not stable ABI budgets or a claim about optimized VM/JIT/AOT adapters. That initial schema-15 result did not cover general instantiation, host-value integration, failure/destructor paths, component memory or code size, asynchronous cancellation, or remote Windows execution.

The clean schema-19 macOS AArch64 [baseline](../../benchmarks/results/2026-10-16-macos-aarch64-76c8f81.json) records 31 samples from commit `76c8f81516be5eb4cd77ab063b3ecd3935d16e58`. Median/p95 costs are 1.226/1.352 us for unlocked schema-2 negotiation and instantiation, 1.358/1.384 us for complete exact-lock replay through instance creation, 507.875/559.375 ns for one host-value publication/owned transfer/release, 3.529/3.619 us for two instantiations plus guest-trap transfer cleanup and redacted failure projection, and 524.959/545.417 ns for host-value transfer, destructor failure, poisoning, and redacted projection from prebuilt adapter tables.

These rows are not pairwise component overheads. Both instantiation rows validate structural projections and build owned state; the locked row additionally validates complete package/version/digest coverage. The guest-trap row deliberately includes two general instantiations, while the destructor row starts from fresh adapter tables but no component instantiation. The measured host-value and failure rows use the backend-neutral host-side adapter after a scalar backend result; the typed-resource VM/JIT/AOT path was implemented later and is not part of this schema-19 baseline.

A separate child process retaining 4,096 exactly locked instances has a 10,108,928-byte median peak resident set versus 2,064,384 bytes for the control, an 8,044,544-byte whole-process delta. This includes loader/allocator state, duplicated owned contracts and bindings, adapter containers, and benchmark overhead, so simple per-instance division would overstate precision. The scalar `i64 -> i64` component function has a 40-byte JIT body and 416-byte AOT object; those sizes exclude the host registry, adapters, runtime support, and final link. The 271-byte encoded contract and 19-byte redacted failure report are serialized-document sizes, not machine code. All results are directional evidence for this host and exact fixtures, not acceptance budgets.

## Test plan

- Unit tests for accepted schema-1 metadata and resolved local bindings.
- Negative tests for unknown schemas, invalid names, duplicate declarations, import/export collisions, and unsupported signatures.
- Tests proving missing capabilities are reported before unknown host registrations.
- Tests for missing, duplicate, and undeclared embedding bindings.
- Round-trip and malformed-fixture tests for deterministic scalar metadata encoding, explicit byte/declaration-count/nesting-depth/string-length limits, and byte-offset diagnostics.
- A libFuzzer target plus a deterministic arbitrary-byte smoke corpus for the bounded decoder.
- Schema-2 candidate-contract tests cover canonical ordering, an exact golden encoding, full-SemVer exact/interval requirements, SHA-256-tagged locks, resource identities, duplicate/orphan/mismatched-lock rejection, every parser bound, malformed headers, and arbitrary bytes; a separate fuzz target exercises the decoder.
- VM/JIT differential tests and macOS AArch64 plus Windows MSVC AOT link/run integration use the same validated schema-1 scalar contract; a host-side resource/failure fixture wraps all results after general instantiation. One nominal owned-resource path additionally crosses typed VM/JIT and both native tier-1 direct/response-file executions with adapter cleanup. Future tests add richer resource calls.
- Resource-handle tests for generation-safe reuse, stale owners/borrows, repeated release/end, shared/exclusive conflicts, mutation, deterministic reuse, and identifier exhaustion.
- Backend-neutral adapter tests cover wrong-type and duplicate/borrow-aliased owner rejection, non-reusing boundary indices, destination-limit failure without ownership change, allocation/callback-free commit, owner transfer/return, call-scoped and nested borrows, deterministic traces, host-value linkage, host-side VM/JIT/macOS-AOT equivalence, and one typed backend owner lane. Future tests cover exact scalar lanes, boolean normalization, stale/active-lend and total-index exhaustion injection, forbidden borrowed results, and richer typed resource calls.
- Backend-neutral failure tests cover pre-commit preservation, post-commit non-rollback, reverse-order cleanup after inner-to-outer borrow closure, moved-owner ledger updates, remove-before-callback destructors, continued cleanup with ordered suppressed failures, reusable guest panic/trap exits, poisoned host-callback/destructor exits, rejected later entry, contained panic-payload/error-formatting/error-disposal failures, and equivalent host-side failure projection across current backends. Future tests cover uncommitted results, callback reentrancy, incomplete-cleanup injection, rejected foreign unwind, asynchronous cancellation, and backend landing pads.
- In-memory negotiation and lockfile tests cover exact build-sensitive and bounded requirements, prerelease opt-in, arbitrary-precision numeric prerelease precedence, locked digest success/mismatch, highest compatible selection independent of registration order, same-precedence ambiguity, additive provider members, function/resource-ownership/capability mismatches, capability-before-inventory ordering, canonical complete lock generation, and exact no-fallback replay. Future tooling and integration tests cover zero-major defaults, compatible-version resource lineage and rename/identity mismatch, remaining structural dimensions, stable file I/O, and identical typed VM/JIT/AOT bindings.
- Future capability-denial conformance and richer `own<T>`/`borrow<T>` cross-mode tests, including return transfer and multiple resource types.
- Schema-20 benchmark tests enforce fourteen rows across metadata-validation, interface-negotiation, scalar-dispatch, resource-lifecycle, instantiation, host-value-lifecycle, failure-cleanup, and representative-application series; four process-memory rows; 4,096 retained instances; and positive scalar-component code/object and schema-2 document sizes. Clean macOS and Windows artifacts are recorded. Future benchmarks cover cached/specialized dispatch and broader typed resource conversion through VM/JIT/AOT ABIs.

## Open questions

- Should the serialized contract adopt WIT directly, embed a compatible subset, or use a project-specific encoding with a WIT adapter?
- Which source-level range syntax should tooling lower into the normalized exact/interval representation?
- When, if ever, should independently versioned interfaces replace package-inherited versions?
- Which rich values should follow the initial scalar/resource subset, and which require a later schema?
- How should capability namespaces, delegation, attenuation, and ambient host policy compose?
- Should validated contracts retain direct function pointers, immutable registry references, or generation-checked registry handles?
- Which sanitizer should each embedding apply, and what evidence would justify revising the accepted candidate failure-record tags or redacted/sanitized message policy before public stabilization?

Decision: **accepted on 2026-10-17** by the repository owner. Acceptance includes the candidate schema-2 contract/failure-report serialization and forward-compatibility policy specified here as the implementation baseline. It does not stabilize a public serialized format, claim WIT compatibility, or complete production package-manager I/O; those remain explicitly versioned downstream work. The open questions above are optional follow-ups and do not block the accepted baseline.

## Sources

- Repository-owner acceptance, recorded 2026-10-17 02:25 JST.
- [GitHub Actions run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398), completed 2026-10-17; native Windows object-link-run, component conformance, schema-20 benchmark, and artifact-upload evidence. The retained result is [`2026-10-17-windows-x86_64-7c37d5d.json`](../../benchmarks/results/2026-10-17-windows-x86_64-7c37d5d.json), recorded 2026-10-17 01:45:50 JST.
- WebAssembly Component Model, “Why the Component Model?”, official living documentation; no publication or update date stated; verified 2026-10-16. <https://component-model.bytecodealliance.org/design/why-component-model.html>
- WebAssembly Component Model, “WIT”, official living documentation; no publication or update date stated; verified 2026-10-16. <https://component-model.bytecodealliance.org/design/wit.html>
- WebAssembly Component Model, “Worlds”, official living documentation; no publication or update date stated; verified 2026-10-16. <https://component-model.bytecodealliance.org/design/worlds.html>
- WebAssembly Component Model, “Canonical ABI”, official living documentation; no publication or update date stated; verified 2026-10-16. <https://component-model.bytecodealliance.org/advanced/canonical-abi.html>
- WebAssembly Component Model, `design/mvp/WIT.md`, official specification source; last changed 2026-09-15 in commit `d6a17ea9f828f44c4659d70c35822ab53d2a8a1d`; verified 2026-10-16. <https://github.com/WebAssembly/component-model/blob/d6a17ea9f828f44c4659d70c35822ab53d2a8a1d/design/mvp/WIT.md>
- WebAssembly Component Model, `design/mvp/CanonicalABI.md`, official specification source; last changed 2026-10-13 in commit `2e7895c76905cd3fb7e6be4ac632258972b1f688`; verified 2026-10-16. <https://github.com/WebAssembly/component-model/blob/2e7895c76905cd3fb7e6be4ac632258972b1f688/design/mvp/CanonicalABI.md>
- Semantic Versioning 2.0.0, official versioned specification; no publication or page-update date stated; verified 2026-10-16. <https://semver.org/spec/v2.0.0.html>
- The Rust Reference, “Panic,” official living documentation; no publication or update date stated; verified 2026-10-16. <https://doc.rust-lang.org/reference/panic.html>
- Rust standard library, `std::panic::catch_unwind`, official living API documentation; no page update date stated; verified 2026-10-16. <https://doc.rust-lang.org/std/panic/fn.catch_unwind.html>
- The Rustonomicon, “FFI and unwinding,” official living documentation; no publication or update date stated; verified 2026-10-16. <https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-unwinding>
- `docs/architecture.md`, embedding and component architecture plus the initial-backend decision, last updated 2026-10-17 02:25 JST.
- `docs/roadmap.md`, Milestone 0 component-boundary tasks, acceptance dispositions, and current evidence, last updated 2026-10-17 02:25 JST.
- `crates/runtime/src/lib.rs`, typed host-function registry prototype, last updated 2026-10-16.
- `crates/runtime/src/component.rs`, schema-1 component validation prototype, last updated 2026-10-16.
- `crates/runtime/src/component/serialization.rs`, candidate schema-2 contract and failure-report serialization, last updated 2026-10-16.
- `fuzz/fuzz_targets/versioned_component_contract_decoder.rs` and `fuzz/fuzz_targets/versioned_component_failure_report_decoder.rs`, bounded schema-2 decoder fuzz targets, last updated 2026-10-16.
- `crates/runtime/src/component/resource.rs`, generation-tagged resource-table prototype, last updated 2026-10-16.
- `crates/runtime/src/component/host_value.rs`, canonical-identity to host-value bridge, last updated 2026-10-16.
- `crates/runtime/src/component/instantiation.rs` and `crates/runtime/src/component/lockfile.rs`, general instantiation and deterministic exact-lock replay, last updated 2026-10-16.
- `crates/mir/src/lib.rs`, nominal owned-resource type and straight-line linearity validation, last updated 2026-10-16 23:14 JST.
- `crates/bytecode/src/lib.rs`, typed owned-resource signature lowering and verification, last updated 2026-10-16 23:14 JST.
- `crates/codegen-clif/src/lib.rs`, 32-bit owned-resource JIT/AOT lane prototype, last updated 2026-10-16 23:14 JST.
- `crates/vm/src/lib.rs`, typed VM value boundary and owned-resource execution, last updated 2026-10-16 23:21 JST.
- `src/component_resource_conformance.rs`, adapter-integrated typed-resource VM/JIT conformance and cleanup, last updated 2026-10-16 23:21 JST.
- `tests/aot_macos.rs` and `tests/aot_windows_msvc.rs`, typed-resource native AOT conformance paths, last updated 2026-10-16 23:21 JST.
- `src/component_application_conformance.rs`, representative multi-component application fixture, last updated 2026-10-16 23:29 JST.
- `src/bin/prototype-bench.rs`, schema-20 representative-application benchmark contract, last updated 2026-10-16 23:29 JST.
- `benchmarks/results/2026-10-16-macos-aarch64-de248a1.json`, clean schema-20 macOS AArch64 representative-application baseline, measured 2026-10-16 23:34:00 JST.
- `benchmarks/results/2026-10-16-macos-aarch64-76c8f81.json`, clean schema-19 macOS AArch64 boundary-cost baseline, measured 2026-10-16 22:51:15 JST.
