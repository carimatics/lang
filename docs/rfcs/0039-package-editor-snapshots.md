<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0039-package-editor-snapshots.md -->

# RFC 0039: Package editor snapshots and checked edits

[日本語版](../ja/rfcs/0039-package-editor-snapshots.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-C01–C07
- Supersedes: none
- Superseded by: none

## Summary

Extend the existing UTF-16 LSP server with package-aware navigation, completion, semantic tokens and checked rename/code actions. All semantic answers refer to one immutable admitted package/input/overlay epoch and original source spans. Edits are proposals for the client, never direct filesystem writes.

## Motivation

The existing server supplies revision-safe diagnostics and CST formatting for one explicit source root, but lacks package namespace navigation and safe semantic edits. The package frontend already resolves dependency identities, privacy and typed local/field facts. Reusing those facts avoids a second language resolver.

## Goals

- Navigate a real declared package dependency before broader editor scaffolding.
- Preserve definition/reference identity through source changes, open overlays and watched-file refresh.
- Provide scope/privacy/type-aware completion and source-aligned semantic tokens.
- Validate rename/fix proposals against the complete observed snapshot and client document versions.

## Non-goals

- New source semantics, serialized IR fields, compiler/runtime ABI or guest execution.
- Multiple independent workspace folders, registry downloads or arbitrary filesystem reads.
- Direct source writes, file-resource operations, rich snippet edits or a globally atomic external-writer guarantee.
- Inventing valid semantic bindings for erroneous or unadmitted source.

## Guide-level design

Retain one explicit local workspace root and existing full-document synchronization. At a root containing `Lang.json`, package mode uses the common selected root/member/target contract; explicit initialization options may select a relative `manifestPath`, `package` and `target`. The manifest and every package-authorized path must remain within the client-authorized workspace. No ancestor search or authority expansion occurs. Without a selected/default manifest, existing standalone behavior remains available.

Open source overlays replace admitted file bytes in memory; existing module/dependency boundaries remain authoritative. New on-disk modules enter through refresh/admission. A not-yet-admitted unsaved file gets an unavailable result rather than filesystem authority. Dirty manifest overlays invalidate package semantic answers until valid metadata is saved/refreshed. Unselected workspace members do not silently become active packages.

Definition/references, completion and semantic-token capabilities are advertised only as their implementations land. Unsupported, malformed or stale requests retain existing JSON-RPC error handling. Plain Location results need no LocationLink capability; positions/ranges use the server's existing UTF-16 encoding.

At the protocol boundary, clamp a character offset beyond the line length to that line's end as LSP requires. Reject a nonexisting line or an interior UTF-16 surrogate position. The internal byte-offset API continues to require an in-bounds UTF-8 boundary.

## Reference semantics

### Input epochs

A snapshot owns frozen manifest/source bytes, SourceMap, checked program, module/name graph, open-document versions and a monotonically changing epoch. Package capture and existing compiler quotas apply; additionally bound open overlay bytes/count, indexed occurrences, query work and encoded response bytes. Input keys refer to canonical admitted identities; aliases cannot introduce conflicting overlay text for one identity.

C01 defaults permit 256 overlays / 16 MiB overlay bytes, 65,536 symbols, 262,144 occurrences, 64 MiB logical owned index name/path bytes and 10,000,000 index/resolution steps. The byte budget supplements count bounds; it is not an exact heap-allocation measurement. Protocol response bounds remain required at integration.

Retain all admitted manifest/source bytes, including unused inputs, in the epoch. Source inputs sharing a manifest's physical identity are refused before overlays: an editor buffer cannot reinterpret package metadata as source. A closed diagnostic URI is retained only while it has findings; send one empty publication when those findings disappear.

Document open/change/close and applicable watched-file changes invalidate dependent semantic answers. Open overlays remain authoritative over disk refresh. Compute from one captured epoch; publish only if the current epoch and relevant document versions still match. Failure invalidates semantic navigation/edit facts, while parser diagnostics and lexical token fallback may remain available. Do not answer from the last successful program as though it described changed input.

### Symbol identity

Symbols are snapshot-local declaration identities, with kind and original declaration-name span. Item/case resolution uses ModuleGraph; local/parameter/pattern identities and field targets use resolved/typed HIR. A use points to that exact declaration and original name-token span. IDs are not stable serialized metadata.

Local bindings take precedence over module-name lookup. Do not classify comments, field names or same-spelled names in other scopes by text search. Global paths use the existing resolver's privacy/alias rules and a shared finite resolution budget. Unknown/ambiguous/unproven bindings produce no guessed target. Deduplicate physical occurrences, sort responses by original path/byte range, and map validated UTF-8 spans to UTF-16 only at the protocol boundary.

### Queries

Definition navigates to the declaration-name span; references optionally include that declaration. Completion uses the lexical scope and checked receiver type, applies privacy and returns a bounded deterministic list with plain labels/text edits. Invalid/incomplete source may offer only facts whose scope/identity is established; no hidden private members are invented.

Semantic tokens use a fixed legend for namespace, type, enum member, function, parameter, variable, property, keyword, comment, string, number and operator. Split multiline tokens at line boundaries, excluding CR/LF; emit sorted non-overlapping UTF-16 lengths and checked delta encoding. Initial support is full-document tokens; no unsupported delta/range capability is advertised. Syntax errors may use lexical classification without manufacturing semantic identities.

C03 may replace only the current identifier slot with a bounded private marker in a temporary source overlay. Run the normal parser/module/binding pipeline against the same admitted epoch; never publish probe diagnostics, source locations or executable output. Module candidates reuse the existing resolver/privacy rules. Field candidates may use a normally resolved receiver's declared type annotation or function result even when later body typing rejects the marker; this proves a declaration/name relationship, not program validity. Ambiguous, unbound or unsupported receivers produce no guessed fields. Bound copied names, candidates, work and encoded edits; all returned edits use the original document's range/version epoch. No disk writes or last-successful stale facts are used.

### Checked edit proposals

Prepare/rename supports proven source item, case, field and local bindings; names must be legal non-keyword identifiers. Refuse unresolved/core/external declarations, overlapping edits, collisions and unsupported alias/module/file renames. Build replacements for the exact declaration/use set, apply them to an in-memory copy, reanalyze and compare binding correspondence under byte-offset remapping. The intended symbol changes name; other bindings, type/privacy/ownership/effect facts must retain their meaning. Successful type checking alone does not prove absence of accidental capture.

The initial binding proof structurally compares the complete resolved HIR after remapping source spans and only the names selected by the edit plan; declaration-qualified names are rebuilt from their unchanged module and edited declaration name. Compare checked function/type facts as well. Preserve callable/local/nominal/field identities, ABI, effects, arena authority, control flow and initializer operations; do not normalize away semantic IDs or compare formatted debug strings. An unsupported or ambiguous correspondence is a refusal, including import-alias/module/external renames. This is a bounded sufficient proof for identifier replacements, not a general refactoring equivalence algorithm.

Before emission, refresh/validate relevant closed inputs and recheck open versions/epoch. Every edited file must be open, with a concrete version, because an LSP null version provides no client stale-edit guard. Require `workspaceEdit.documentChanges` and `failureHandling` of `transactional` or `textOnlyTransactional` for multi-file text edits. Return versioned non-overlapping TextDocumentEdits; otherwise explain the missing prerequisite and return no partial edit. Client application supplies atomicity within that advertised contract. There is no direct server write, resource operation or guarantee against arbitrary changes after proposal emission.

Initial code actions are explicit document formatting and prefixing an unread binding with `_`, only when their validated edit proof succeeds. Reuse the formatter and unread-binding rule, preserve initializer effects/ownership, and rename all proven assignment uses when needed. Do not auto-insert unsafe, change allocators, swallow failures or delete effectful initializers.

C06 initially requires code-action literal support and versioned documentChanges. Use `source.format` for whole-document formatting and `quickfix` only when the requested range selects exactly one proven unread binding; multiple eligible bindings are ambiguous and offer no underscore fix. Reuse the package linter rule. Formatting reanalyzes the candidate and compares complete resolved HIR with only source-position metadata erased on both sides, plus checked type facts; names, IDs, ABI, effects and operations remain exact. Rename keeps its stricter span/name correspondence proof. These bounded initial actions never execute guest code, delete initializers or apply edits themselves.

## Execution modes

Editor analysis uses frontend contracts shared by VM/native/task profiles. No bytecode execution, native linking, SDK compilation, host callback or Cargo process is needed. Runtime/concurrency/memory behavior is unchanged.

## ABI and compatibility

Additive experimental tooling APIs and standard LSP methods only. Keep source syntax, ABI, lock/bytecode/serialized IR schemas unchanged. Symbol sidecars and epochs remain in-memory exact-analysis data. Preserve existing standalone diagnostics/formatting and default UTF-16 synchronization. No artifact migration is needed.

## Diagnostics and tooling

Retain original source diagnostic identities and stable package/CLI errors. Map malformed parameters, out-of-snapshot positions, UTF-16 surrogate interiors, stale requests and resource refusal through structured protocol errors. Error messages must not expose ambient paths outside authority. Cap encoded locations/completions/tokens/edits before returning a response, with no partial successful prefix.

## Performance and cost model

Reuse bounded immutable source/CST/HIR data; collect only the selected package's loaded graph. Charge node/token visits, resolver steps, type/scope walks and response bytes. Expensive rename validates the complete relevant semantic graph before proposing edits. Cache by epoch and discard stale computed results. No latency, incremental-performance or memory benchmark claim is inferred from the prototype.

## Security and trust boundaries

Initialize grants only the explicit local root. Package resolution/canonical identity guards and open-overlay admission constrain all I/O. URI/position/version validation precedes access. Requests cannot grant network/process/runtime capabilities. Rename/code actions are versioned proposals requiring explicit client application, with the limited atomicity contract above.

## Alternatives

- No change leaves package navigation and safe semantic edits absent.
- Text/name matching is smaller but confuses scopes, fields, comments and dependency privacy.
- A separate editor resolver duplicates language semantics and can drift from execution.
- Direct filesystem edits or unversioned closed-file edits cannot meet the selected stale/transactional contract.
- Incremental compiler restructuring could reduce work later; a bounded snapshot sidecar gives the first real navigation fixture without requiring it.

## Implementation plan

C01 accepts this contract and adds an admitted immutable package snapshot plus real dependency-definition navigation. C02 connects definition/references, overlays and watched refresh. C03 adds scoped typed completion; C04 adds semantic tokens. C05 validates rename and client edit guarantees; C06 connects the two checked code actions. C07 executes the complete protocol/package/resource/lifecycle fixture. Keep one implementation delivery doing; audit actual task/estimate growth before code.

## Test plan

Test real sibling package functions/types, imports/aliases/privacy, local/global shadowing, field/pattern identities, UTF-8/UTF-16/CRLF boundaries, malformed source, open/change/close, closed-file refresh and stale publication. Rename tests include valid multi-file changes, capture/collision, missing versions/transactional capability, stale input, invalid names and unchanged files on refusal. Check token ordering, response limits and end-to-end wire framing. Run both-host CI and the required baseline; no benchmark/fuzz campaign is implied.

## Open questions

Accepted under RFC 0001's experimental delegation, 2026-10-29 JST: existing resolver/HIR identities and explicit snapshot/edit boundaries are reasonably sound. A finite prototype resolves `helper::answer` bytes 41..47 to dependency declaration bytes 7..13, retains original bytes after provider mutation, and excludes a shadowing local through resolved HIR. It does not prove full overlay/LSP/edit integration or costs; C01–C07 must supply those fixtures. The open-file/transactional client requirement is an experimental product tradeoff.

## Sources

- [Official LSP 3.18 specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/), current as labeled by the publisher; publication/update date unavailable; verified 2026-10-29 JST. UTF-16 negotiation, Location, TextDocumentEdit versions and transactional/textOnlyTransactional failure handling are protocol facts.
- RFC 0001 (updated 2026-10-28), RFCs 0034/0036/0037 (2026-10-29), current module/binding/HIR/database/LSP implementation and ignored resolver prototype; verified 2026-10-29 JST. Code has no embedded publication/update dates. Snapshot limits, initial actions and stricter edit prerequisites are project decisions.
