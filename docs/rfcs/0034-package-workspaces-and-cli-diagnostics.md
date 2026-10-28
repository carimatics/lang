<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0034-package-workspaces-and-cli-diagnostics.md -->

# RFC 0034: Package workspaces and CLI diagnostics

[日本語版](../ja/rfcs/0034-package-workspaces-and-cli-diagnostics.md)

- Status: accepted
- Authors: project owner, Codex
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4 G1–G4 in [the delivery board](../milestone-4.md)
- Supersedes: none
- Superseded by: none

## Summary

Introduce explicitly versioned `Lang.json` package/workspace manifests, local dependency resolution, `Lang.lock` and newline-delimited JSON CLI diagnostics. This is a host toolchain contract; it changes no source expression, typed MIR, runtime capability or ABI.

Accepted under RFC 0001's experimental delegation on 2026-10-29 JST. Exact local versions, explicit filesystem authority and fail-closed formats are a feasible first reproducible package profile. Registry transport, dependency namespace integration, both-host reproducibility and tooling ergonomics remain unverified until their deliveries have executable evidence. Acceptance is not implementation or stabilization.

## Motivation

The source CLI can compile a root file but cannot identify a package, resolve an authorized dependency graph or replay a checked source snapshot. M4 needs a create/depend/build/run/test fixture before expanding editor or installer infrastructure. M5's registry is not an implicit source of package authority.

## Goals

- Deterministic local package graphs and exact offline lock replay.
- Versioned, bounded, strict manifest/lock parsing and stable diagnostic envelopes.
- Reuse the existing source compiler, VM/native pipelines and source diagnostics.

## Non-goals

- Registry/Git/HTTP dependencies, version ranges, build scripts, macros, feature unification and automatic downloads.
- A source language edition change or changes to native/component ABI and existing artifact formats.
- Installer signatures, editor protocols and profiling contracts; those require separate M4 designs.

## Guide-level design

`Lang.json` is UTF-8 JSON, without comments or duplicate keys. A package example is:

```json
{
  "schema": 1,
  "package": {"name": "hello", "version": "0.1.0", "edition": "2026", "entry": "src/main.lang"},
  "dependencies": {"helper": {"path": "deps/helper", "version": "0.1.0"}},
  "tests": ["tests/smoke.lang"]
}
```

A workspace root has `"workspace": {"members": ["app", "helper"]}` and may also contain a package. A manifest must declare a package or a nonempty workspace; dependencies/tests require a package. Paths use `/`, regardless of host. Entries/tests/members are nonempty relative paths with no empty, `.` or `..` component. Dependency paths additionally permit `..` for workspace siblings, but the resolver must enforce the canonical authority boundary before reading them. Absolute paths, backslashes, colons and NUL are refused.

Package names and dependency aliases match `[a-z][a-z0-9_]{0,63}`. Versions are exactly three decimal u32 components, without leading zeroes, prereleases or build metadata. Edition is exactly `2026`. The library root uses the same `entry` field as a binary; package orchestration selects the appropriate compilation entry. The existing `mod`/`use`, privacy and ownership rules apply. Dependency aliases expose only declared direct dependencies, including when two dependencies use the same transitive alias. Namespace integration must preserve source bytes/spans, diagnose name collisions and prevent undeclared imports.

`targets` on a dependency is an optional nonempty list of exact target triples: `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`. Absence includes both; duplicates and unknown triples are errors. Resolution selects one explicit supported target, defaulting to the host; target changes invalidate lock replay. Validate all declarations before filtering inactive dependency edges.

Planned commands are `lang new DIR`, `lang build`, package-aware `lang run`, `lang test`, `lang package`, `lang fmt`, `lang lint` and `lang doc`. Package commands accept `--manifest-path`, `--target`, `--offline`, `--locked` and `--message-format human|json` where applicable. `lang package check-manifest FILE` is the first inspection operation and does not grant dependency I/O. Existing explicit-file CLI forms retain their behavior.

## Reference semantics

Manifest schema 1 rejects missing required and unknown fields, duplicate keys at every object level, unsupported schema/edition, malformed names/versions/paths/conditions and trailing input. No default package identity or inferred dependency path is allowed. Optional dependencies/tests default to empty; optional workspace defaults to absent. An optional field, when present, must have its declared type; explicit null is not omission. Manifest bytes are limited to 64 KiB, each dependency/member/test list to 128 entries. JSON nesting retains serde_json's bounded recursion limit.

Resolution starts from an explicit manifest. Its directory is the authority root; an explicitly selected containing workspace can authorize sibling members. Canonical dependency roots, entries and source files must remain inside that root, with symlink escapes refused. Local dependency identities are canonical root plus declared name/version. Alias must equal the dependency name in schema 1. Resolve edges in sorted alias order, reject cycles, duplicate identities with conflicting contents, mismatched exact versions and missing sources. Limits: 256 packages, 256 modules, 16 MiB total source bytes and 4096 edges; account repeated graph visits. No build script or dependency code executes during resolution.

`Lang.lock` schema 1 records selected target, toolchain/package edition, sorted package identities using authority-relative `/` paths, exact versions, sorted active edges and SHA-256 source/manifest checksums. Hashes cover original bytes, with length-prefixed paths and bytes in lexical order; no absolute host paths, mtimes or filesystem enumeration order enter identity. Every `.lang` source beneath a package source root is included; generated output, nested dependency roots and symlink escapes are excluded/refused explicitly. Compiler source-root access must be restricted to those inputs. The lock implementation must document its canonical byte encoding and fixture before being used for replay.

Ordinary resolution creates/updates the lock transactionally after complete validation. `--locked` refuses a missing/stale lock and never rewrites it. `--offline` permits only authorized local files and performs no network operation; combined with `--locked` it checks all identities, edges and checksums before compilation. No stale source or cached binary substitutes for a mismatch. Write outputs through same-directory temporary files and atomic publication; failures retain previous lock/build output. Concurrent source changes must invalidate publication or use the verified immutable snapshot. A content digest supplies integrity, not publisher authentication.

Source evaluation order, typing, ownership/destruction, cancellation, concurrency, const evaluation and foreign capabilities retain their accepted semantics. Package loading grants host compiler read authority only; guest runtime authority remains explicit.

## Execution modes

VM, native AOT and JIT consume the same checked source graph. Package tests are explicit source entries with `fn main()->i64`; zero passes and nonzero fails. Runtime failure fails the test. Native builds use existing target/SDK/link admission and record their toolchain inputs; a portable artifact alone does not establish the reproducible native-binary gate. Embedded/component artifacts keep explicit bindings and independently versioned formats.

## ABI and compatibility

Compatibility domains are `package` and `tooling`. Manifest, lock and diagnostic schema versions are independent of bytecode, native SDK and component C v1 versions. Unknown versions are errors, never guessed migrations. No preexisting package manifest is being migrated. Future changes that invalidate schema 1 require an RFC revision and explicit migration/refusal tests. Existing source commands and human diagnostics remain compatible.

## Diagnostics and tooling

JSON mode emits one complete JSON object per stderr line, with no human prose mixed in. Schema 1 diagnostic objects contain `schema: 1`, `kind: "diagnostic"`, `code`, `severity: "error"|"warning"|"note"`, `message`, `path` (string or null), and `span` (null or `{start,end}` UTF-8 byte offsets in that path). Optional contextual detail belongs in a future version, not ad hoc fields. Source diagnostics retain their stable existing codes and original spans; package errors use `PKG0001` malformed input, `PKG0002` unsupported version, `PKG0003` invalid contract field, `PKG0004` resource limit, `PKG0005` filesystem/authority, `PKG0006` dependency graph, `PKG0007` lock mismatch. Usage errors use `CLI0001`; tool/runtime failures without a more specific diagnostic use `CLI0002`. Exit status remains 2 for usage and 1 for source/tool/runtime failure, 0 for success. Program results remain stdout; command-specific success records must be explicitly documented before implementation. Error locations may be absent for manifest semantic validation; never fabricate an offset.

Formatting/lint/doc operate on the declared verified package inputs. Editor overlays and debugger source mappings use original paths/spans, not generated concatenated source. Package syntax does not change the source formatter. Further LSP/DAP/code-action designs must state their edit and snapshot invariants.

## Performance and cost model

Parse memory/work is bounded by manifest bytes and list limits. Graph traversal and hashing consume explicit package/edge/source limits; byte hashing and canonical sorting are visible build work. Cache hits require matching source, target and toolchain identities. No timing or binary-size improvement is claimed without measurements.

## Security and trust boundaries

Treat manifests, locks and dependency sources as untrusted input. Reject path traversal outside the explicit canonical authority, symlink escapes, invalid UTF-8, unknown fields/versions and oversized input before allocating unbounded structures. Do not invoke a shell, fetch a registry, load native plugins or grant guest capabilities during resolution. Local path checks must not imply race-resistant isolation; immutable verified snapshots and handle-relative access are required before publishing output under concurrent changes.

## Alternatives

- No change leaves M4's reproducibility and onboarding gates unsatisfied.
- A shell wrapper around source commands cannot independently verify the package graph or provide the diagnostic contract.
- Cargo-like TOML manifests and range resolution are a strong alternative, but add a format/parser and solver/registry policy before the first local fixture. Exact JSON fits the existing serde stack and keeps authority inspectable. Ergonomics remain an open measurement, not a claim of superiority.
- Concatenating dependencies or ignoring checksums is simpler but loses source namespaces/spans or reproducibility. It does not satisfy this design.

## Implementation plan

1. Strict manifest parser and structured diagnostic inspection, with malformed-input/resource-limit fixtures.
2. Authorized deterministic local graph and exact lock/replay tests.
3. Create/local-dependency/build/run/test fixture, including changed-source offline refusal; then native reproducibility.
4. Package/fmt/lint/doc integration and the broader M4 editor/debug/installer deliveries.

Use [the M4 board](../milestone-4.md) for observable 1–3-point deliveries, validation and estimate changes.

## Test plan

Round-trip admitted manifests; reject duplicates, missing/unknown fields, unsupported editions/versions/targets, invalid paths, cycles, version mismatches, escapes, stale locks and resource exhaustion. Verify JSON lines and exit status with real CLI processes. The first package fixture must create a project and local dependency, compile/run/test, replay offline exactly and refuse modified input without changing prior output. Check VM/native values and spans, relocation-independent lock identity, target changes, both tier-1 hosts and repeated binary digests with pinned toolchain inputs. Fuzz parser/lock boundaries when implemented; do not equate finite malformed tests with a fuzz campaign.

## Open questions

- Registry identity, signing and version ranges belong to M5, not implicit fallback behavior.
- Installer/editor/profiling protocols remain separate accepted-design work on the board.
- JSON ergonomics, immutable package snapshot costs and reproducible native linking require evidence; revise explicitly if the measured design must change.

## Sources

- Repository `src/main.rs`, `src/frontend.rs`, `crates/semantics/src/modules.rs`, `Cargo.lock` and M4 roadmap, inspected 2026-10-29 JST. Embedded publication/update dates unavailable.
- [RFC 0001](0001-project-charter.md), last updated 2026-10-28, experimental delegation recorded 2026-10-17 JST; [RFC 0006](0006-source-frontend-and-portable-programs.md), source/module contract, last updated 2026-10-28. Verified 2026-10-29 JST. This RFC makes project decisions and no new comparative performance claims.
- Cargo Book: [manifest format](https://doc.rust-lang.org/cargo/reference/manifest.html) and [workspaces](https://doc.rust-lang.org/cargo/reference/workspaces.html). Both publish no update date; verified 2026-10-29 JST. They document TOML manifests, explicit workspace members and a shared workspace lock. The JSON/exact-local profile here is a project choice, not an inferred Cargo guarantee.
