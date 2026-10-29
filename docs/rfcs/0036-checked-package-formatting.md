<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0036-checked-package-formatting.md -->

# RFC 0036: Checked package formatting

[日本語版](../ja/rfcs/0036-checked-package-formatting.md)

- Status: accepted
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-B02
- Supersedes: None; extends RFCs 0034/0035
- Superseded by: None

## Summary

Add `lang fmt [--check]` with the common manifest/member/target/locked/offline/message-format options. Format all frozen selected-graph source/test files through the existing lossless CST formatter. Check mode reports required changes without modifying files; normal mode applies checked source replacements.

Accepted under RFC 0001's experimental delegation, 2026-10-29 JST, before implementation. Reusing the existing parser/formatter keeps language behavior unchanged. The new host write boundary requires explicit snapshot guards and executable stale/alias/failure tests. No performance or cross-file transaction guarantee is inferred.

## Motivation

The LSP already formats immutable document snapshots, but package users lack a CLI covering dependencies and declared tests. Calling a formatter over an ambient directory would lose the admitted source set and stale-input checks. Add a host adapter for existing CST output, not another formatter.

## Goals

- Preserve tokens, comments and the existing formatter policy across the selected source graph.
- Parse/prepare the entire bounded plan before publishing edits, refusing invalid or detected stale inputs.
- Retain explicit filesystem authority and source permissions.

## Non-goals

- Formatting manifests, changing source syntax/typing, semantic lint fixes or editor rename.
- A filesystem-wide transaction, crash recovery, or exclusion of uncooperative external writers.
- Automatic lock regeneration after changing source.

## Guide-level design

```text
lang fmt --manifest-path Lang.json --package app --check
lang fmt --manifest-path Lang.json --package app
```

Common selection, bounds and diagnostic rules remain RFC 0034's. `--check` is fmt-only; native/SDK, archive-list and execution-limit options are invalid. Success is silent. Check mode emits schema-1 `FMT0001`/exit 1 for each file needing formatting, preserving stdout and every file. Invalid UTF-8 uses `FMT0002` with the original byte range; original CST syntax diagnostics retain their stable code/path/byte span. Parse/formatter resource refusals use `PKG0004`.

## Reference semantics

Capture the original package graph, manifests and complete source inventory once. Reuse the existing CST parser, comment/token owners and work/output limits; do not require type-correct source merely to format it. Prepare every file before edits, and refuse syntax errors rather than treating preserved-invalid formatter output as success. Bound aggregate formatted bytes to 64 MiB. Unselected workspace source files and manifests are not formatted.

The plan owns immutable original source/metadata bytes and physical input identities. Directory bindings must be canonical, using the same directory-alias restrictions as archive profile 1. Final source-file links are refused too; manifest links may be read but are never replaced. No manifest, lock, generated artifact or guest resource is edited.

Apply requires an explicit separately retained authority-root directory capability. Open and retain each canonical input parent directory; validate regular files, identity and original bytes for every source and admitted manifest before staging. Missing/replaced/changed input is `PKG0007`; aliases and unsupported file types are `PKG0003`. Check mode performs this live validation without staging or publication.

For normal mode, prepare private same-directory temporary files for changed sources, preserving the original permissions and bounding output. Revalidate the complete expected input set immediately before the first source replacement. Publish each file through its retained parent capability with an atomic replacement, and clean unpublished temporary files on failure. Successful source edits intentionally make an old lock stale; do not rewrite it.

The validation barrier refuses detected stale plans before any source replacement. Parent handles preserve authority under directory renames and do not follow later display-path replacement. Publication is atomic per file, not across a batch: a later filesystem failure reports the already committed prefix and leaves uncommitted originals intact. Concurrent external writes after validation follow ordinary filesystem races; do not claim a global compare-and-swap or overwrite newer data during rollback. No rollback is attempted across external writers.

## Execution modes

VM/native/JIT/embedding/component behavior and spans are unchanged. This command does not execute or compile guest code and grants no guest filesystem capability. LSP formatting retains its existing document/version policy.

## ABI and compatibility

No source, MIR, bytecode, native, SDK, C or archive format changes. Add stable formatting diagnostic codes under the existing schema-1 envelope. The opaque Rust plan API is experimental; a new filesystem epoch needs a new plan. Existing explicit-file commands and package archive/build/run/test remain compatible.

## Diagnostics and tooling

Retain original UTF-8 source locations. Emit all syntax failures from preparation before publication; stale validation uses `PKG0007` and filesystem operations use `PKG0005` with stage/path and committed-prefix information. A successful check/write prints no artifact record. Formatter options remain the existing deterministic four-space policy. Lint/doc/editor/DAP behavior is not changed here.

## Performance and cost model

Reuse frozen source buffers and one CST formatting pass per file. The plan retains original bytes plus bounded replacement text. Live validation reads bounded inputs; staging writes only changed files. No timing/size improvement is claimed. Directory/file counts retain existing package limits.

## Security and trust boundaries

User invocation authorizes host edits only to captured source paths inside the explicit directory capability. Private plan fields prevent callers from replacing arbitrary paths through a forged edit. Recheck identities/bytes before publication, refuse aliases/special files, use nonblocking admission where supported and preserve permissions. Do not infer process/network/FFI authority or interpret metadata as executable code.

## Alternatives

- No change leaves CLI formatting absent.
- An ambient formatter recipe loses frozen selection and stale checks.
- A new formatting engine duplicates the existing checked CST/comment policy.
- Returning only editor edits is simpler but does not supply CLI application.
- A cross-file journal/locking protocol is stronger but introduces crash/concurrency semantics beyond this outcome; retain explicit per-file publication and report partial I/O failure.

## Implementation plan

1. Build a bounded formatting plan from the immutable graph and prove comment/token preservation and invalid-source refusal.
2. Apply through explicit retained parent capabilities with full input validation, permission preservation and temporary cleanup.
3. Connect fmt/check diagnostics; test stale/alias/read-only/failure cases and existing package compatibility, then full baseline and both-host CI.

## Test plan

Format real root/dependency/test files, retain Unicode/comments, replay run/test after ordinary lock regeneration, and prove idempotence/check-mode no writes. Invalid syntax/UTF-8 and a changed/replaced manifest or source refuse before source publication. Exercise directory/file aliases and retained-parent authority, readonly/output failures, permissions and temporary cleanup. Preserve existing CST tests and package/archive/native suites. Require local baseline and actual Windows/macOS CI; finite tests do not prove exclusion of external writer races.

## Open questions

No blocking question for this scoped host adapter. Crash-atomic batches, cooperative locks and general safe refactoring remain separately designed features.

## Sources

- [CST formatter](../../crates/syntax/src/format.rs), [formatter tests](../../crates/syntax/tests/formatting.rs), [LSP adapter](../../src/lsp.rs), [checked edit previews](../../crates/diagnostics/src/lib.rs): repository source has no embedded update date; verified 2026-10-29 JST.
- [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md) and [RFC 0035](0035-source-package-snapshots.md), updated 2026-10-29; source authority, frozen inputs, diagnostics and canonical-directory profile.
