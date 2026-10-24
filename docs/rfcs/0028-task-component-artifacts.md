<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0028-task-component-artifacts.md -->

# RFC 0028: Serialized task and component MIR

[日本語版](../ja/rfcs/0028-task-component-artifacts.md)

- Status: implemented
- Authors: Project contributors
- Created: 2026-10-24
- Last updated: 2026-10-24
- Tracking issue: M3-I10, M3-I11, M3-I07
- Supersedes: None; extends RFC 0011/0014/0017 with a separate schema
- Superseded by: None

## Summary

Accept **LANGBC schema 5** for backend-neutral task/channel/service MIR and complete component declaration/binding data. Loading reconstructs the existing independent task, ownership, effect, loan, failure and component-binding proofs. Schema 1–4 bytes retain their meanings. No source reparse, native pointer, live handle, provider registration or capability grant is serialized.

Acceptance under RFC 0001's 2026-10-17 JST delegation records a format decision, not an implemented or stabilized loader. I10 decides the format; I11 must next execute the first stored task fixture; I07 completes channel/service/component integration. I08 still owns private native records, native-package/SDK admission and both-host installed execution.

Implementation checkpoint (2026-10-24 JST): I11's Task profile 0 and I07's Component profile 1 are implemented. They use the shared MIR codec, raw IDL document/maps, original snapshot/span mapping, complete independent task/type/effect/loan/failure/binding proof and one cumulative preparation budget. Raw MIR transport itself returns no execution authority. `artifact::component::{encode, decode}` and the general artifact loader admit passive checked units; explicit host bindings execute sync/task exports, original resources/native imports, rich results and failure cleanup on VM/generated native. An independent 1903-byte vector freezes every IDL kind, definitions, versions/requirements and resource/import/export maps. The ordinary CLI still requires explicit host bindings after validation. Installed native profiles/SDKs remain I08 and public embedding/both-host conformance remain later work. Status implemented does not imply stabilization.

## Motivation

At `4ba991d`, `examples/scripts/tasks.lang` executes from source, but artifact compilation refuses with “task artifacts require an accepted serialized task format” and publishes no output. The old codec intentionally refuses asynchronous declarations, task scopes, channel/service types and component identities, including unused metadata. In-memory execution and source loading cannot satisfy the M3 artifact requirement. These are repository observations verified 2026-10-24 JST, not performance measurements.

## Goals

- Store actual typed semantic data and independently reconstruct all execution prerequisites.
- Decide exact fields/tags, finite loading costs, original diagnostic origins and old-profile compatibility before admission.
- Reuse the existing MIR contract, task engines and component binding checker.

## Non-goals

- New syntax, source semantics, scheduling, Send/Share rules or authority.
- Stable serialization, new native/component C ABI, package/SDK revision, authentication or automatic native loading.
- Persisting a suspended invocation, cleanup receipt, provider inventory or grant.

## Guide-level design

After implementation, `lang compile --output tasks.lbc examples/scripts/tasks.lang` stores MIR. `lang run --artifact tasks.lbc` executes the verified artifact after the original source has been removed. Source snapshots only supply diagnostics. Native consumers compile the decoded MIR through the existing task backend; they must not evaluate the saved source as a fallback.

A component artifact retains the complete IDL declarations, source identities and binding map, including unused entries. Loading yields a passive checked unit. Explicit existing host bindings, original allocators, controls and grants remain necessary to call it. The ordinary CLI cannot obtain provider authority from an artifact. Unimplemented profiles fail before publication or provider/guest effects.

## Reference semantics

### Envelope, primitives and profile

Inherit RFC 0011's 16-byte envelope and primitive encodings: `LANGBC\0\0`, little-endian `u16` version **5**, `u16` flags **0**, `u32` payload length. Counts/IDs/offsets are `u32`, tags/booleans are `u8`, literal integers are `i64`; all multi-byte integers are little-endian. Strings are length-prefixed exact UTF-8. Vectors are count then elements. `Span` is `(source, start, end)`; `TypeUse` is `(type, Span)`. Optional IDs use 0 absent or 1 followed by ID. Reject unknown tags, non-0/1 booleans, overflow, invalid UTF-8, length mismatch, truncation and trailing bytes.

The first payload byte is the profile: **0 Task**, **1 Component**; 2–255 are invalid. Both contain the raw ownership table, followed by its optional entry. Only Component then contains the document and binding map specified below. Task rejects every component import/resource, including unused ones. Component admits synchronous and task functions, and requires complete independent component binding validation before publication. An entry, when present, must satisfy the existing parameterless-i64 script-entry contract; library artifacts may omit it.

### MIR field order and unchanged tags

Inherit all schema-4 fields and tags from RFC 0011/0014/0017 except the explicit additions below. Table order is: source snapshots; nominal definitions; constructed types; imports; function declarations; per-function value/place tables; function bodies; optional entry. Every list retains its original order and indices. Sources preserve normalized diagnostic paths and exact UTF-8 snapshots; neither their names nor contents authorize file access or guest parsing.

An import row remains `(name, ABI, span, parameter TypeUses, result TypeUse)`; **ABI 2 = Component** joins Host 0 and C 1. A function row is `(name, public, ABI, effect, span, signature_span, parameter local IDs, result TypeUse, locals)`. Function ABI remains Guest 0 / CExport 1. **Effect 0 = Synchronous; 1 = Asynchronous followed by its original async-token Span**. Every function carries the effect byte, including synchronous ones. An async CExport remains invalid.

A function body is `(loan_count, prepared_count, task_scopes, task_bindings, blocks)`. A scope is `(optional parent scope ID, Span)`. A task binding is `(cleanup-target tag, target ID, scope ID)`; target tags remain Local 0 / Value 1 / Loan 2 / Arena 3, without a cleanup-step span. Each body retains independent scope/binding indices; nonempty task metadata is never silently discarded. Blocks, raw lexical cleanup steps, results, places, projections and original instruction/terminator spans retain the schema-4 layouts.

Existing constructor tags 0–9 remain Builtin, Option, Result, Vec, Map, Shared, Weak, Arena, Loan and ArenaOwner. Every constructed row ends in its original Span. Add:

| Tag | Constructor | Payload before Span |
| --- | --- | --- |
| 10 | Task | type |
| 11 | ChannelSetup | type |
| 12 | Sender | type |
| 13 | Receiver | type |
| 14 | SendResult | type |
| 15 | ServiceSetup | state type, message type |
| 16 | ServiceOwner | state type, message type |
| 17 | ServiceHandle | message type |
| 18 | ComponentResource | nominal definition ID |

Builtin tags 0–9 keep their meanings; **10 TaskError, 11 ChannelError, 12 ServiceError** are added. All other builtin/constructor tags are invalid. Nominal component tags are identities, never serialized resource payloads.

### Operations

Instruction layout remains `(result register vector, opcode, operands, Span)`. Opcodes 0–32 retain RFC 0011 meanings, and 33 remains RFC 0014 Panic with one register. Add these exact assignments, independently of Rust enum order:

| Opcode | Operation | Operands in order |
| --- | --- | --- |
| 34 | TaskScopeEnter | scope ID |
| 35 | TaskScopeExit | scope ID |
| 36 | TaskSpawn | function ID, argument register vector |
| 37 | TaskAwait | task register |
| 38 | TaskCancel | place ID |
| 39 | TaskYield | none |
| 40 | TaskCheck | none |
| 41 | TaskSaveReturn | value register |
| 42 | PrepareChannel | prepared ID, endpoint register, role byte, message TypeUse |
| 43 | ChannelSend | prepared authority ID, payload register |
| 44 | ChannelReceive | prepared authority ID |
| 45 | PrepareService | prepared ID, endpoint register, role byte, optional state TypeUse, message TypeUse |
| 46 | ServiceSend | prepared authority ID, payload register |
| 47 | ServiceReceive | prepared authority ID |

Channel roles are **0 Sender / 1 Receiver**; service roles are **0 Handle / 1 Owner**. Optional state TypeUse uses 0 absent or 1 followed by TypeUse. Verifiers independently require the role-appropriate state/message/endpoint relation and reject forged or stale prepared authority. Opcodes 48–255 are invalid.

CoreCall keeps `(operation byte, type-argument TypeUse vector, argument register vector, optional result loan ID)`. Core tags 0–36 retain their meanings. Add **37 ServiceNew, 38 ServiceHandle, 39 ServiceClone, 40 ServiceOwner, 41 ServiceRead, 42 ServiceReplace, 43 ServiceFinish, 44 ServiceCancel, 45 ChannelNew, 46 ChannelSender, 47 ChannelClone, 48 ChannelReceiver, 49 ChannelSeal, 50 ChannelUnsent**. Tags 51–255 are invalid. Type arguments and result-loan provenance remain checked, not implied by a valid tag.

Terminator tags 0 Return, 1 Jump, 2 Branch and 3 Unreachable retain their layouts; **4 ReturnSaved** has no operand. Every terminator ends in Span. ReturnSaved requires the independently proven saved-return/scope-exit contract; it does not reconstruct a runtime return cell from bytes. Tags 5–255 are invalid.

### Complete component document and binding map

The component suffix stores declaration data directly, preserving original IDL spans and type-node IDs. It does not store JSON source for parsing, a proof bit or a Rust memory layout. LCMP schema 3 and its normalized wire representation remain unchanged and separate. `Spanned<T>` below means `(T, Span)`; an IDL type ID is `u32`, checked when converting to/from the in-memory index.

Keys are structured strings: PackageKey `(namespace, package)`; InterfaceKey `(PackageKey, interface)`; ResourceTypeKey `(InterfaceKey, resource)`; ValueKey `(InterfaceKey, name)`. Validate the existing key grammars and full nominal identity. SemanticVersion is `(u64 major, u64 minor, u64 patch, prerelease vector, build-string vector)`; a prerelease element is tag **0 Numeric / 1 AlphaNumeric**, then string. VersionRequirement is tag **0 Exact + SemanticVersion**, or **1 Interval + minimum-inclusive SemanticVersion + maximum-exclusive SemanticVersion + allow-prerelease boolean**. These retain LCMP3's existing version/key semantics and primitive layouts.

Document fields, in order:

1. `source ID, source_bytes, Spanned<PackageKey>, Spanned<SemanticVersion>`.
2. Interface vector. An interface is `(Spanned<InterfaceKey>, Spanned<ResourceTypeKey> vector, definition vector, function vector, Span)`.
3. Import vector. An import is `(Spanned<InterfaceKey>, Spanned<VersionRequirement>, Spanned<string> member vector, Span)`.
4. `Spanned<InterfaceKey>` export vector.
5. Type-node vector. A node is `(kind byte, payload, Span)`.

An IDL field is `(Spanned<string> name, type ID)`. A definition is `(Spanned<ValueKey>, kind byte, payload, Span)`: **0 Record + field vector, 1 Variant + field vector, 2 Alias + type ID**. A function is `(Spanned<string> name, Spanned<effect byte>, parameter field vector, result type ID, Spanned<string> capability, Span)`; effect is **0 Sync / 1 Task**. These bytes do not represent a live task.

Type-node kind tags/payloads are **0 Unit / 1 I64 / 2 Bool / 3 Text / 4 Bytes** (no payload), **5 List / 6 Option** (one type ID), **7 Result** (ok then error type IDs), **8 Named** (`Spanned<ValueKey>`), **9 Own** (`Spanned<ResourceTypeKey>`), **10 Borrow** (access byte **0 Shared / 1 Exclusive**, then `Spanned<ResourceTypeKey>`). Other tags are invalid. Type-node sharing/order is preserved; independent validation rejects invalid references, recursion, aliases, resource nesting and depth exactly as for checked IDL input.

The document source must identify an actual retained source snapshot; `source_bytes` must equal its UTF-8 byte length. Every document span must belong to that source, be within it and lie on UTF-8 boundaries; owner-containment rules also apply. Names, effects, keys, versions and declarations are independently checked as data; loading never reparses the snapshot to manufacture declarations.

Origin mapping clarification (2026-10-24, before implementation): I06's checked document retains compiler/global IDL source IDs/spans while MIR snapshots use code-local indices. A component writer takes the explicit path/text snapshot owning the document origins, retains the existing code snapshot order/indices, reuses an identical path/text snapshot or appends it, and consistently maps document.source and every IDL span.source to that artifact-local index. Preserve path, text, start/end and mutual source identity; never invent padding snapshots to preserve an integer from another namespace. A decoder retains the artifact-local indices. This grants no source parsing or file access. Metadata declaration order is the order normalized by the existing IDL proof; a decoder must reject noncanonical order rather than silently sort it into acceptance. Existing vectors/profiles do not change; unpublished Component profile 1 needs no migration. Accept this implementation-grounded clarification under RFC 0001's delegation and verify source mapping, independent proof and cumulative budgets with executable fixtures.

The binding map follows: `(u32 version = 1, resource vector, import vector, export vector)`. Resource rows are `(nominal definition ID, ResourceTypeKey)`; import rows `(source import ID, InterfaceKey, member string)`; export rows `(source function ID, InterfaceKey, member string)`. Preserve order and complete entries, including unused ones. Reapply I06's complete coverage, unique resource/member/source correspondence, ABI/effect/signature/nominal identity, public-export and direct-loan checks. An empty or omitted map cannot authorize component code. Provider selection/locks and live grant plans remain explicit host inputs, not map entries.

### Independent proofs and execution

Decode into raw MIR and declaration data, not pre-elaborated execution plans. With one cumulative budget, reconstruct types, declarations/ABI/effects, places/CFG, initialization/moves, Send/Share, task scopes/claims, suspension loans, channel/service authority, lexical/failure cleanup and regions through the existing task verifier. Component then independently validates the complete document and binding map, including unused metadata, before returning a passive unit. Do not expose partial decoded tables as execution authority.

Raw cleanup steps and binding/scope declarations express obligations and must be checked; they are not proofs. Do not serialize derived cleanup order, resource-table generations, loan registrations, scheduler state, suspension frames, native addresses, allocator callbacks, controls, call holders, retained code owners or trusted flags. Each invocation still creates fresh runtime state using the existing engines and host authority. Evaluation order, reverse-publication cleanup, first-failure/suppressed errors, poison and cancellation retain their accepted semantics. No constant-execution or unsafe privilege is added.

### Limits and deterministic encoding

The complete envelope, code, IDL snapshots, metadata and maps share the artifact byte cap; default **64 MiB**. Use checked arithmetic before counts, multiplication, slicing or allocation; reject impossible lengths before reserving. Existing MIR/task/IDL/binding depth, element, table, work and requested-allocation limits all remain mandatory. Use the tightest configured common ceiling where phases overlap and carry prior usage forward; decoding, verification, matching, sizing and writing may not reset/refund counters. Repeated references, strings, unused entries and origin validation consume work. Host runtime quotas remain separate fresh invocation limits; their values are not accepted from the artifact.

Writers validate first, size under the same budget and publish only a complete valid artifact. Preserve table/vector order and exact snapshots; no implicit sorting, deduplication or renumbering. Re-encoding an admitted decoded schema-5 value with sufficient limits produces the same bytes. General source compilation selects schema 5 for task/channel/service. The component API requires explicit IDL and a checked binding unit; the ordinary CLI does not infer them. Existing non-task/component Copy/ownership/C inputs retain schemas 1/2/3/4 and their exact bytes. A dedicated schema-5 library API may encode an empty/synchronous task-profile table; validity does not depend on finding a new opcode.

## Execution modes

VM and generated native execution consume the same revalidated task MIR. Optional JIT uses the existing backend and availability rules; it gains no fallback evaluator. Native source compilation from decoded MIR is distinct from loading an installed AOT package. Components require the existing explicit checked Program/binding/instance boundary; nested synchronous callbacks and root task calls retain RFC 0027's controls and restrictions. Both-host artifact equivalence must be measured; this decision alone proves neither Windows execution nor installed linking.

## ABI and compatibility

Schema 1–4 readers/writers and golden vectors remain unchanged, including explicit refusal of task/channel/service/component features. Old consumers reject version 5 rather than retry it as an older version or source. A version-5 decoder rejects unknown flags/profiles/tags and malformed unused metadata before effects. Native private packets, native-package metadata, LCMP3, LCVL1, LCSG1 and component C ABI v1 do not change. Migrate by rebuilding from source with a supporting compiler; editing a version field is invalid. No downgrade or mixed-version interpretation is accepted.

## Diagnostics and tooling

Retain structured byte offset, profile/table/tag and verifier source spans; unknown wire tags differ from well-formed but invalid semantic data. Preserve code/IDL origins and binding indices for mismatch reports. Unsupported runtime/entry/grant requests fail before effects. Existing parser/formatter/LSP/DAP protocols do not change; decoded source paths remain display-only. CLI/help and the final M3 guide must describe schema selection and limits when implemented.

## Performance and cost model

Schema 5 adds one profile byte, one effect byte per function, an async Span where present and explicit task/component metadata. Original IDL spans cost space compared with normalized LCMP; preserving them avoids a metadata reparse and ambiguous origin reconstruction. Decode allocates bounded raw tables and reconstructs existing proofs; native compilation remains a separate bounded operation. Task runtime costs/quotas and provider-retention/destruction behavior do not change. No size, throughput or latency improvement is claimed; Z01 must measure them.

## Security and trust boundaries

All artifact bytes are untrusted. Validate complete metadata, aliases, spans, effects, authority claims and aggregate quotas before publication, binding, provider discovery or invocation. A valid resource key, component ABI byte, source path or capability name grants no resource, filesystem, network, process, allocator or native-code authority. Host-selected providers/code owners and original controls remain outside the artifact. Malformed/unsupported data cannot trigger native image constructors or source loading.

## Alternatives

- **No change:** retain source-only tasks; fails the accepted artifact gate.
- **Library/tool wrapper around source:** reparses guest source and loses the independently serialized MIR boundary; rejected.
- **One new universal IR/package container:** strongest consolidation alternative, but unnecessarily changes old bytes and couples frontend semantics to installation/SDK policy.
- **Task-only schema with component metadata omitted:** simpler first implementation, but not a complete accepted format; I11 may stage implementation while I07 retains all metadata requirements.
- **Normalized LCMP plus reconstructed origins or embedded IDL JSON:** reuses wire/parser code but either loses exact original spans/type-node identities or requires metadata reparsing. Explicit document tables reuse the existing checked model without those ambiguities; LCMP remains the canonical interoperation format.

## Implementation plan

1. **I10 (2 pt):** accept the complete paired decision, inspect old-profile refusal, independently verify the envelope vectors below and run repository consistency. No new format execution is claimed.
2. **I11 (3 pt):** extend the shared MIR codec with explicit schema-5 branches and cumulative task proof; execute `tasks.lang` from stored MIR on VM/generated native after source deletion. Retain all old vectors/refusals. Unsupported component loading remains explicit until I07.
3. **I07 (3 pt):** integrate full channel/service/component document/maps, CLI/embedding loading, rich owning differential fixtures, malformed/unused metadata and old vectors. Re-audit concrete remaining work before implementation and expose any new prerequisite first.
4. I08/P/E/L/B/X/Z continue separately. No scaffold replaces a source-execution fixture, full baseline, both-host evidence or final guide.

## Test plan

The independent **empty Task library framing vector** is 46 bytes: header hex `4c414e4742430000050000001e000000`, then **30 zero bytes** (profile 0, seven empty table counts and absent entry). It is a normative encoder/decoder fixture, not evidence that the current loader supports it. Changing byte 16 to 2 is an unknown-profile error; changing byte 10 to 1 is an unsupported-flags error; changing the payload length to 29 is a length error. Unknown version 6 must fail before interpreting payload. Actual semantic admission/round-trip becomes I11 evidence.

Use an independent reference packer, old schema-1/2/3/4 golden fixtures, all new operation/constructor tags, every truncation prefix, unknown tags, overflow/count/UTF-8/span corruption and exact/one-below cumulative ceilings. Forged async effects, task scopes/claims, saved returns, prepared channel/service authorities, Send/Share, live loans across suspension and forged component identities must fail independently of the writer. Include unused metadata and snapshot substitution.

Execute source→artifact→decode→VM/generated-native fixtures for tasks, owned transfers, integrated channels/services, component resources/loans/imports/rich results, failures/cancellation and original cleanup. Delete/alter guest sources to prove no reparse; use explicit provider counters to prove malformed inputs cause no effects. Add the appropriate artifact fuzz target/corpus and complete the repository baseline. Both-host runtime/package evidence is still required at its tracked gate; target compilation and fuzz compile/lint do not imply execution or a campaign.

## Open questions

No unresolved semantic blocker. I11's focused tests confirm the empty library table and cumulative task-codec proof; I07 regressions confirm source mapping, complete IDL/map validation and cumulative preparation history. Work admission precedes long name/capability/version inspection. Long-term maintainability of shared traversal with schemas 2–4 remains unverified. Revise this decision before changing a field, tag, proof requirement or compatibility consequence. Performance and both-host equivalence remain measurements.

## Sources

- Project decisions: [RFC 0001](0001-project-charter.md), last updated 2026-10-17; [RFC 0011](0011-ownership-artifacts.md), 2026-10-20; [RFC 0014](0014-explicit-source-panic.md), 2026-10-20; [RFC 0017](0017-c-aware-ownership-artifacts.md), 2026-10-20; [RFC 0023](0023-task-mir-boundaries.md), 2026-10-22; [RFC 0027](0027-production-component-interface.md), 2026-10-24. These are repository decisions, not external specifications.
- Implementation observations: ownership codec/tag tables; raw MIR task/channel/service declarations and task verifier; component IDL model, LCMP3 codec and independent binding checker at `4ba991d`. Files publish no embedded update dates; inspected and executable refusal verified 2026-10-24 JST. No new external-design or performance claim requires web research for this decision.
