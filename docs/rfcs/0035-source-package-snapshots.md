<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0035-source-package-snapshots.md -->

# RFC 0035: Deterministic source package snapshots

[日本語版](../ja/rfcs/0035-source-package-snapshots.md)

- Status: implemented
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-B01
- Supersedes: None; extends RFC 0034
- Superseded by: None

## Summary

Implementation audited 2026-10-30 JST at `0b6591d`: deterministic archive headers and original bytes, trusted extracted execution, frozen-source replay and failure preservation are reconciled with the implementation and acceptance tests `package_archive`. The complete local baseline (174 suites / 2,514 tests, zero failures), fmt/full Clippy/doc and the macOS 15/Windows package and continuous installed-tool journeys pass. Acceptance-stage open-validation notes below are historical; this contract is implemented. This does not mean stabilized; the compatibility boundaries and non-goals below remain.

Add `lang package [--list]` with the existing manifest/member/target/locked/offline/message-format options. Capture the complete selected source snapshot once. List the planned files without publication, or publish a deterministic uncompressed TAR containing those original bytes, lock and versioned descriptor.

Accepted under RFC 0001's experimental delegation, 2026-10-29 JST, before implementation. Existing bounded inventory, lock verification and transactional output APIs make this a focused tooling outcome. TAR interoperability, extraction/replay, resource limits and both-host behavior require executable evidence; acceptance is not implementation or stabilization.

## Motivation

Build/run/test can execute a checked package but cannot show or archive their admitted inputs. Archiving the ambient directory would include output/private files, reread changing sources and lose the lock's bounded authority. Reuse the frozen inventory rather than a second filesystem walk. This is an experimental target/compiler-specific source snapshot, not M5 registry publication.

## Goals

- Inspect and archive exactly the frozen selected graph, declared tests and workspace metadata.
- Preserve original bytes, deterministic UTF-8 path order and checked offline replay after trusted extraction.
- Reuse failure-safe generations and retain old outputs after refusal.

## Non-goals

- Registry publication, signatures, compression, automatic dependency downloads or untrusted archive extraction.
- Executing archive inputs, changing language semantics or granting guest capabilities.
- Representing directory aliases in the first archive profile.

## Guide-level design

```text
lang package --manifest-path Lang.json --package app --list
lang package --manifest-path Lang.json --package app --locked --offline
```

`package check-manifest` retains its existing inspection-only route. Bare `package` selects the default `./Lang.json` and produces an archive. `--list` is package-only; native/SDK and execution-limit options are invalid here. Common selection/target/usage/refusal rules remain RFC 0034's.

Human listing prints one escaped authority-relative path per line. JSON listing is one ordered object `{schema:1,kind:"package-files",files:[...]}`; each file has ordered `path`, `kind` (`manifest`, `source`, `lock`, `descriptor`), `bytes`, `sha256`. Include generated `Lang.lock` and `.lang-snapshot.json` because this lists the planned archive. Listing creates no lock, target directory or archive. Locked listing still verifies the existing lock before stdout.

Packaging publishes `target/lang/<lock-sha256>/package/source.tar` and `inputs.lock`, then commits the root lock last unless locked. Successful output uses the existing human path or schema-1 artifact JSON record. Do not add arbitrary output-path authority.

## Reference semantics

Capture the canonical lock and source inventory before listing/encoding. Include original bytes for every selected graph package manifest, all catalogued workspace manifests and all captured source/test files, including unused files in admitted scopes. Deduplicate identical metadata paths; reject role/path collisions. Include the canonical expected lock, not a later reread of the ambient lock.

Archive profile 1 requires canonical directory bindings: refuse workspace member aliases, active dependency directory aliases, declared entry/test directory aliases and inventoried source-directory aliases with `PKG0003`. Existing build/run/test alias behavior is unchanged. This restriction keeps a plain-file archive replayable without inventing links or rewriting manifests. Existing containment, regular-file, cycle and physical-source-identity admission still applies before the archive profile. Final-file links already admitted as frozen regular bytes need no archive link.

Reserve `.lang-snapshot.json`, `Lang.lock` and their descendants in this archive profile, comparing the first path component without ASCII case distinctions on both hosts; refuse an admitted input collision. This prevents file/directory and case-folding collisions during trusted extraction. The descriptor's ordered canonical JSON fields are `format` (`lang-source-snapshot`), `schema` (1), `target`, `compiler_build_id`, `lock_sha256`; use two spaces and a final LF. Source manifest/lock versions remain unchanged.

Sort all logical file entries by UTF-8 path bytes. Emit GNU-compatible TAR regular-file headers through `tar` 0.4 with default features disabled. Set mode 0644, uid/gid/mtime zero, empty owner/group names and no filesystem-derived metadata. Use deterministic long-name extensions supplied by the library. Do not emit directories, links, devices, xattrs, compression headers or executable source permissions. Finish the standard archive terminator. No source parse, typing, guest effect, process invocation or source reread occurs during encoding.

Keep the existing graph bounds and at most 643 logical archive files (256 package manifests + 129 workspace manifests + 256 source files + lock/descriptor, before deduplication). Bound encoded bytes to 64 MiB through a fallible writer. Listing records are bounded by the same file/path inventory. A source snapshot owns immutable encoded bytes and descriptions; verification against a freshly captured expected archive is exact bounded byte comparison, without parsing/extracting foreign files or granting authority.

## Execution modes

VM/native/JIT/embedding/component semantics and spans are unchanged. A trusted test may extract the produced plain-file archive into an owned directory and replay its selected graph with the original lock. This does not introduce a production archive-execution API.

## ABI and compatibility

The descriptor independently versions this experimental archive profile; TAR is the container, not an authentication mechanism. Existing source/IR/bytecode/native/SDK/C formats are unchanged. The lock retains exact target/compiler identity, so cross-target/toolchain source distribution remains a later registry design. Future profile changes require an explicit compatibility/migration decision; readers must not infer authority from a descriptor or checksum.

## Diagnostics and tooling

Use existing schema-1 diagnostics: `CLI0001`/exit 2 for invalid options, `PKG0002` for unsupported metadata versions where parsed, `PKG0003` for unsupported alias/reserved paths, `PKG0004` for resource refusal, `PKG0005` for filesystem admission and `PKG0007` for lock/archive/cache mismatch. Errors produce no listing/artifact stdout. Preserve `check-manifest`, explicit-file execution and native build compatibility. Formatter, lint, LSP and DAP behavior is unchanged by this delivery.

## Performance and cost model

Reuse captured buffers and sorted metadata, with bounded hashing and a TAR output buffer. No compression or second filesystem scan is needed. Archive encoding and comparison are linear in admitted bytes, apart from bounded path sorting. No performance or size improvement is claimed. Disable the crate's optional xattr behavior because snapshot headers are generated from explicit fields.

## Security and trust boundaries

Manifests/source bytes are untrusted data under existing bounds. Capture through the original directory capabilities, verify locked inputs before output, and reuse exact generation-cache/alias checks and final-lock transaction. Plain relative paths and alias refusal avoid manufacturing filesystem links. There is no untrusted extraction API, provider discovery, guest execution or implicit network/process capability. TAR libraries/standard tools do not authenticate source snapshots.

## Alternatives

- No change leaves package inspection/archive integration incomplete.
- A shell TAR recipe rereads ambient files and cannot enforce this frozen graph/transaction contract.
- A custom JSON/binary container offers explicit framing but adds a new data encoding and decoder instead of standard tooling.
- Preserving directory links is more flexible, but requires a separate virtual/extraction binding contract. Refuse them in profile 1.
- A permissive directory archive is simpler but loses bounded input identity and exact replay.

## Implementation plan

1. Add frozen file catalogue/TAR encoding and a real sibling extraction/replay fixture.
2. Connect package/list to existing request/diagnostic/output paths, with alias, stale-input, collision, resource and cache refusals.
3. Verify local baseline and both-host CI, record B01's last 3 points, then continue package fmt/lint/doc without changing the completed native outcome.

## Test plan

Compare repeated complete archive bytes and original extracted file bytes. Execute a declared sibling import and test after trusted extraction, including locked/offline replay. Verify source freeze after edit/delete/rename, UTF-8 ordering, deterministic header fields, no output on list/failure, unchanged old generations, alias/escape/reserved-path and resource refusals. Retain all existing package/native tests. Run focused tests then the full required baseline and actual Windows/macOS CI. Exact expected-byte verification is not a general TAR fuzzing campaign.

## Open questions

No blocking question for this plain-file snapshot profile. Directory-link support, compression, cross-toolchain registry sources and untrusted extraction require later design/evidence and are not implicit features of B01.

## Sources

- [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md), updated 2026-10-29; package authority, frozen inventory, locks, CLI and publication contracts.
- [tar-rs](https://github.com/composefs/tar-rs), version 0.4.46 selected by Cargo; its Cargo manifest's latest commit is 2026-05-18, verified 2026-10-29 JST. Crate release publication/update dates were not available from the queried registry endpoint. Default xattr support is disabled. Container/alias/budget choices above are project decisions, not claims made by the library.
