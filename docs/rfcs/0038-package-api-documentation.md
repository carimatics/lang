<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0038-package-api-documentation.md -->

# RFC 0038: Package API documentation

[日本語版](../ja/rfcs/0038-package-api-documentation.md)

- Status: implemented
- Authors: project maintainers with Codex
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-B04
- Supersedes: none
- Superseded by: none

## Summary

Implementation audited 2026-10-30 JST at `0b6591d`: public API indexing, resolved nested type links, escaped source panels, relocation and exact rendering/publication limits are reconciled with the implementation and acceptance tests `package_doc; package::doc::tests`. The complete local baseline (174 suites / 2,514 tests, zero failures), fmt/full Clippy/doc and the macOS 15/Windows package and continuous installed-tool journeys pass. Acceptance-stage open-validation notes below are historical; this contract is implemented. This does not mean stabilized; the compatibility boundaries and non-goals below remain.

`lang doc` generates a detached, offline HTML API reference from the selected package's checked frontend program. Public types/functions retain original signatures, adjacent `///` text, resolved type links and original source locations. Publish `target/lang/<lock-digest>/doc/index.html` through the existing failure-safe generation/lock protocol.

## Motivation

Package consumers need an inspectable API without running guest code or retaining source files beside a generated page. Text-only extraction loses resolved type identity, including dependency types and same-spelled declarations.

## Goals

- Document checked public functions, records and enum cases across the selected package's loaded dependency/module graph.
- Link nominal signature/field/payload types by resolved definition IDs, including nested constructed types.
- Preserve original paths, byte ranges, Unicode source text and deterministic offline output.
- Refuse invalid input or output failure before replacing the committed lock/generation.

## Non-goals

- Markdown, intra-comment link syntax, doctest execution, search service or JavaScript.
- Semantic lint enforcement, private-item API listings or automatic browser launch.
- A stable serialized API metadata schema or stable fragment IDs across compiler builds.

## Guide-level design

`lang doc` accepts the existing common manifest/member/target/locked/offline/message-format options. It analyzes the ordinary selected package, requires no runnable `main`, and does not execute declared tests or host calls. Declared tests remain frozen lock inputs. Success prints the HTML artifact path, or the existing schema-1 artifact JSON record. Analysis/output errors exit 1; malformed options exit 2.

The HTML has a title/navigation, public type/function sections, original signature/declaration text, adjacent line documentation, resolved referenced-type links and source links. Source panels embed the original loaded source files, including their private implementation text; distribute generated documentation only when that source disclosure is intended. The document remains usable after original files are removed.

## Reference semantics

Reuse the ordinary immutable package frontend result. Preserve existing source diagnostics and warnings; any analysis error prevents publication. Include only definitions/functions whose checked public bit is true. Module names provide navigation context; public field/case data uses the existing type visibility contract.

Copy function signatures from their original signature span and record/enum declarations from their original span. Collect contiguous preceding `///` lines as plain text, stripping the marker and one optional space; other comments and blank lines end the group. Do not reinterpret comment contents as markup or link syntax.

Resolved type references visit `Type::Nominal`, nested `TypeConstructor::arguments()` and component-resource nominal tags. Link only public documented definitions; do not invent links for builtins or private definitions. Resolve by typed ID, never by spelling. Function parameters/results, record fields and enum payloads supply the reference roots. Deduplicate targets per item. Self/recursive type references do not recursively expand declarations.

Sort visible items by qualified name/kind and loaded source panels by original path. Assign document-local numeric fragment IDs using that deterministic inventory, independently of global source IDs. Every internal link has an emitted destination. Source links include original path and UTF-8 start/end byte offsets; validate all slices through SourceMap. Paths are labels, not ambient file URLs.

## Execution modes

Frontend semantics are shared by VM, native and task programs. Documentation needs no executable lowering, SDK/linker, Cargo, network, guest execution or host callback. Runtime, ownership, concurrency and cleanup behavior are unchanged.

## ABI and compatibility

Additive command and `doc` output generation. Source/ABI/bytecode/lock schemas remain unchanged. HTML is a human-readable exact-build artifact, not a stable metadata wire format. Internal anchor numbering may change with the compiler/input graph; links within one generated file remain complete. No edition migration is needed.

## Diagnostics and tooling

Reuse CLI/source/package diagnostic codes and schema 1; `PKG0004` reports rendering limits, `PKG0005` filesystem failure and `PKG0007` stale lock/cache mismatches. Preserve originals before publication. Formatter/linter/LSP/DAP behavior is unchanged.

## Performance and cost model

Bound generated HTML by 64 MiB, including escaped source text, before publication. The existing aggregate output limit also includes generation lock bytes. Bound renderer work by 4,000,000 steps; charge item, type-reference and source/escape visits. Walk constructed type graphs with a visited set per item. No timing or memory benchmark claim is made.

## Security and trust boundaries

Treat source/comments/paths as untrusted text: escape all HTML special characters, use numeric internal fragments, and embed no source-derived URL, script, event attribute or CSS. Emit a static document with a restrictive CSP allowing only its fixed inline style. Reuse capability-relative output staging, input identity guards, private files, cached-byte verification and lock-last commit. Rendering, source failure or cached-output mismatch must preserve previous committed output/lock. Network and browser access are not granted.

## Alternatives

- No change leaves users without package API documentation.
- A separate source scanner cannot preserve frontend-resolved dependency/type identity.
- An external documentation engine adds a markup/plugin execution boundary unnecessary for this delivery.
- Signature-only text output is smaller but does not satisfy resolved navigation and detached source locations.

## Implementation plan

1. Build a bounded static HTML renderer from the checked frontend program/source map.
2. Add doc command routing and the existing staged generation publication.
3. Execute dependency/type/source/escaping/relocation/invalid-output fixtures; run baseline and both-host CI. Retain B04's reviewed 3 pt unless the actual scope changes.

## Test plan

Verify real CLI documentation of root/dependency types/functions, nested type links, same-spelled declarations, original Unicode comments/byte locations and omission of private API listings. Parse every fragment link independently to prove a matching unique ID. Check HTML injection text, deterministic relocation, source removal, no main/guest execution, invalid source/stale lock and damaged cached output preserving lock/previous files. Exercise exact/one-below render byte/work limits. Run existing command and repository checks plus full baseline and both-host CI.

## Open questions

Accepted under RFC 0001's experimental delegation, 2026-10-29 JST: a static bounded artifact over proven frontend identities meets B04 without a new execution or serialized-metadata boundary. Plain line documentation and exact-build fragments are experimental product decisions. Rich markup, stable API metadata and private-item controls need later designs.

## Sources

- RFC 0001 (updated 2026-10-28), RFC 0034 (2026-10-29) and the current checked HIR/SourceMap/output APIs; verified 2026-10-29 JST. Implementation files have no embedded publication/update dates. The limits/output policy above are project decisions, not external comparative or measured facts.
