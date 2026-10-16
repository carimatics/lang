<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0003-versioned-component-contracts.md -->

# RFC 0003: Versioned component contracts and capability imports

[日本語版](../ja/rfcs/0003-versioned-component-contracts.md)

- Status: proposed
- Authors: Project contributors
- Created: 2026-10-16
- Last updated: 2026-10-16
- Tracking issue:
- Supersedes:
- Superseded by:

## Summary

Define a backend-neutral, versioned component contract that describes package identity, scalar function imports and exports, and the capability required by each import. Keep embedding-local host-function identifiers outside the portable metadata. Before execution, validate the contract against explicit capability grants, logical import bindings, and the typed host-function registry, producing a validated wrapper or a structured error.

The executable prototype supports `i64 -> i64` host imports, records `i64` and `bool` scalar function types, and provides a generation-tagged table for owned resources with explicit shared and exclusive borrow tokens. It is an in-memory design probe, not a stable serialized format or a WebAssembly Interface Types (WIT) implementation.

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
- Return structured, deterministic failures at the trust boundary.

## Non-goals

- Stabilizing a serialized IDL or wire encoding.
- Claiming WIT source or binary compatibility.
- Defining strings, lists, records, variants, options, results, or asynchronous functions.
- Stabilizing serialized resource-handle layouts, cross-component ownership transfer, destructors, or lifetimes.
- Stabilizing the C embedding ABI, native calling convention, or symbol naming.
- Executing component exports through the VM, JIT, or AOT paths in this stage.
- Defining semantic-version compatibility ranges or dependency resolution.

## Guide-level design

A component declaration conceptually contains:

```text
schema = 1
package = example:counter@1.2.3

import read-clock: (i64) -> i64 requires clock.monotonic
export run: (i64) -> i64
```

The declaration does not contain a `HostFunctionId`. The embedder separately configures:

```text
grant clock.monotonic
bind read-clock = host-function 7
```

Validation succeeds only when schema 1 is understood, declarations are structurally valid, every binding names a declared import, every required capability is granted, every import has a binding, and the bound host function exists with a supported typed registration. The resulting validated value retains the portable declaration and resolved local IDs.

The textual examples are explanatory only. This RFC does not select a file syntax or serialization.

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

Schema 1 can validate only `i64 -> i64` imports because that is the only current host-registry shape. The metadata type records `bool` so the contract type system is not defined in terms of the registry method name, but a `bool -> bool` import is rejected until an equally typed registry entry and execution path exist. Exports are declarations only in this stage.

`ResourceTable<T>` owns host values. `OwnedResourceHandle` is `(slot: u32, generation: u64)`. `BorrowedResourceHandle` additionally carries a monotonically allocated borrow identifier and `Shared` or `Exclusive` kind. A released slot increments its generation before reuse; a slot whose generation cannot advance is retired. Free slots are selected in ascending slot order, making reuse deterministic. Borrow identifiers are never reused, and exhaustion is an error.

Resource operations first validate slot occupancy and generation. Shared-borrow creation rejects an active exclusive borrow. Exclusive-borrow creation rejects any active borrow. Mutable access rejects shared or inactive tokens. Ending a borrow removes exactly that token; a repeated end is stale. Release rejects any active borrow, removes and returns the value exactly once, and makes all copied owners stale. The copyable representation therefore does not imply copyable ownership.

## Execution modes

Contract and resource-table validation are backend-neutral and run before selecting VM, JIT, or AOT execution. The prototype resolves imports against `HostFunctionRegistry` and exercises resource lifecycles directly in the runtime crate; it does not yet instantiate a VM module, patch JIT call sites, lower AOT imports/resources, or call exports. Later execution paths must consume the same validated contract and resource rules rather than independently reinterpreting unchecked metadata.

## ABI and compatibility

The metadata schema version and package version have separate meanings. The schema version selects the shape and validation rules of the contract. The package version identifies the declared component revision. Schema 1 requires an exact supported schema value and records, but does not interpret, the package version.

`HostFunctionId` is an embedding-local binding and is deliberately excluded from portable metadata. Resource slot, generation, and borrow identifiers are runtime-local validation data, not a selected wire representation. No Rust layout, enum discriminant, in-memory map order, symbol name, handle bit layout, or explanatory text syntax in this prototype is a stable format. A later RFC must choose a validated serialization, compatibility negotiation, canonical lowering rules, and serialized resource-handle representation before the boundary can be called stable.

## Diagnostics and tooling

Contract validation returns structured error variants for unsupported schemas, invalid identifiers, duplicate names, namespace collisions, unsupported function types, extra or missing bindings, missing capabilities, and unknown host functions. Resource operations distinguish stale owned and borrowed handles, conflicting borrows, release while in use, shared-borrow mutation, table capacity, and borrow-identifier exhaustion. Human-readable messages include logical names or handle identity, while callers can match the variant without parsing text.

A future decoder should attach byte offsets or source spans to the same semantic failures. Tooling should display portable import names rather than local numeric host IDs except when diagnosing embedding configuration.

## Performance and cost model

Contract validation is linear in the number of declarations and bindings on average, using hash sets and ordered maps for duplicate and lookup checks. It allocates owned strings and a resolved-binding map once per successful validation. Resource lookup is constant time by slot; shared-borrow tracking and deterministic free-slot selection use ordered sets and are logarithmic in their respective set sizes. Call-site thunk generation, canonical representation conversion, cross-boundary resource dispatch, and destructor overhead are outside this prototype and remain benchmark requirements before stabilization.

## Security and trust boundaries

Component metadata, capability declarations, and binding configuration are untrusted inputs at their respective boundaries. Validation must complete before execution or linker/JIT patching. Authority comes from embedder-supplied grants, never from a component merely declaring a capability. Capability failure precedes host-registry lookup to avoid exposing host inventory to unauthorized components.

The validated wrapper and resource table prove only their implemented checks. Copyable handles are treated as untrusted tokens: generation and active-borrow state are revalidated on every operation, and authority is not derived from possession of a stale numeric value. The prototype does not sandbox native callbacks, preempt callback work, enforce heap limits, validate a serialized byte stream, run destructors across failures, or make native plugins safe. Panic or unwind behavior must remain represented and contained at the eventual component boundary.

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

## Implementation plan

1. Implemented and tested schema-1 in-memory scalar metadata, explicit capability grants, separate import bindings, host-registry resolution, and structured validation errors.
2. Implemented and tested generation-tagged owned and borrowed resource handles with deterministic stale-handle, repeated-release, repeated-borrow-end, and borrow-conflict rejection.
3. Define a bounded serialized component format and decoder with size/depth limits, source locations, and forward-compatibility rules.
4. Connect one validated contract to VM, JIT, and AOT instantiation and add differential import/export tests.
5. Specify canonical rich-value lowering, C embedding handles, panic/error containment, and compatibility negotiation.
6. Benchmark validation, instantiation, adapter conversion, and steady-state call overhead before accepting the stable boundary.

## Test plan

- Unit tests for accepted schema-1 metadata and resolved local bindings.
- Negative tests for unknown schemas, invalid names, duplicate declarations, import/export collisions, and unsupported signatures.
- Tests proving missing capabilities are reported before unknown host registrations.
- Tests for missing, duplicate, and undeclared embedding bindings.
- Future decoder fuzzing and explicit byte, declaration-count, nesting-depth, and string-length limits.
- Future VM/JIT/AOT differential tests using the same validated contract.
- Resource-handle tests for generation-safe reuse, stale owners/borrows, repeated release/end, shared/exclusive conflicts, mutation, deterministic reuse, and identifier exhaustion.
- Future cross-mode resource lifecycle and capability-denial conformance tests.
- Future benchmarks for validation, instantiation, conversion, and repeated calls.

## Open questions

- Should the serialized contract adopt WIT directly, embed a compatible subset, or use a project-specific encoding with a WIT adapter?
- What package-version compatibility rules and negotiation algorithm should an embedder apply?
- Which rich value and resource ownership types belong in the first stable schema?
- How should capability namespaces, delegation, attenuation, and ambient host policy compose?
- Should validated contracts retain direct function pointers, immutable registry references, or generation-checked registry handles?

Review outcome: **needs prototype/measurement**. Scalar metadata, capability-gated binding, and table-local owned/borrowed resource lifecycles are executable, but serialized resource representation and transfer, bounded decoding, VM/JIT/AOT instantiation, canonical lowering, compatibility negotiation, destructor/failure behavior, and boundary-cost evidence remain blocking work before this RFC can be accepted as the stable component contract.

## Sources

- WebAssembly Component Model, “Why the Component Model?”, official living documentation; no publication or update date stated; verified 2026-10-16. <https://component-model.bytecodealliance.org/design/why-component-model.html>
- WebAssembly Component Model, “WIT”, official living documentation; no publication or update date stated; verified 2026-10-16. <https://component-model.bytecodealliance.org/design/wit.html>
- WebAssembly Component Model, “Worlds”, official living documentation; no publication or update date stated; verified 2026-10-16. <https://component-model.bytecodealliance.org/design/worlds.html>
- WebAssembly Component Model, “Canonical ABI”, official living documentation; no publication or update date stated; verified 2026-10-16. <https://component-model.bytecodealliance.org/advanced/canonical-abi.html>
- `docs/architecture.md`, proposed embedding and component architecture, last updated 2026-10-16.
- `docs/roadmap.md`, Milestone 0 component-boundary task and current evidence, last updated 2026-10-16.
- `crates/runtime/src/lib.rs`, typed host-function registry prototype, last updated 2026-10-16.
- `crates/runtime/src/component.rs`, schema-1 component validation prototype, last updated 2026-10-16.
- `crates/runtime/src/component/resource.rs`, generation-tagged resource-table prototype, last updated 2026-10-16.
