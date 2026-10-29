<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0037-package-lint-diagnostics.md -->

# RFC 0037: Package lint diagnostics

[日本語版](../ja/rfcs/0037-package-lint-diagnostics.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-B03
- Supersedes: none
- Superseded by: none

## Summary

`lang lint` analyzes the selected package and its declared tests without running guest code or publishing artifacts. `--deny-warnings` makes warnings fail the command. Existing schema-1 human/JSON diagnostics retain original file paths and UTF-8 byte spans.

## Motivation

Build already diagnoses unreachable statements, but lacks a package-wide, non-executing lint command suitable for CI. Text searches cannot distinguish resolved local references from comments, names in another scope or fields.

## Goals

- Reuse frozen package inputs, frontend analysis and stable existing diagnostic identities.
- Add a precise unread-binding warning with deterministic package/test reporting.
- Provide pass/fail, warning-denial and JSON fixtures before marking B03 complete.

## Non-goals

- Automatic fixes, attributes, lint plugins or configuration files.
- Whole-program dead-code, topology, lifetime or concurrency inference without an accepted proof.
- LSP/code-action integration, which remains tracked separately.

## Guide-level design

`lang lint [--deny-warnings]` accepts the common `--manifest-path`, `--package`, `--target`, `--locked`, `--offline` and `--message-format human|json` options. A valid warning-free package exits 0. Warnings normally exit 0; `--deny-warnings` exits 1 for any warning. Analysis/admission failures exit 1; malformed options exit 2. Successful lint emits no stdout.

## Reference semantics

Analyze the ordinary selected package, then every declared root test from the same immutable input epoch. Tests expose `_package` using the existing test namespace. Do not require a runnable `main` or zero-valued test result: no execution occurs. Unselected workspace members retain the existing manifest-only validation policy. Unimported source files remain captured/locked but are not assigned invented semantic scopes.

`LNT0001` is a warning on a resolved parameter, mutable local or pattern binding whose name does not start with `_` and whose local ID never occurs as `ExpressionKind::Local` in that function's expression arena. An assignment alone does not read its destination. References in unreachable expressions count conservatively as uses. Exclude compiler-created arena-owner bindings. Use the original binding-name span. Do not propose deleting its initializer, which may have effects or ownership consequences.

Retain frontend warnings, including existing `SRC0019` for unreachable statements; do not create a second control-flow implementation or renumber this code. Only produce new semantic warnings after successful analysis. On frontend failure preserve its diagnostics and produce `CLI0002` only for a structural failure not already represented by diagnostics.

Deduplicate repeated root/dependency/test findings by original path, byte range, code, severity and message. Sort by path, start/end offsets, code, severity and message, independently of per-analysis source IDs. Bound aggregate diagnostics by 65,536 before output; report `PKG0004` and publish no partial diagnostic list on overflow. Warning denial changes exit status, not diagnostic severity or code.

## Execution modes

Lint uses frontend semantics shared by VM/native/task profiles. It does not lower an executable, invoke host callbacks, compile an SDK, execute tests or run Cargo. No runtime values or execution-mode behavior change.

## ABI and compatibility

Additive command and diagnostic code only. Existing source, ABI, bytecode, lock schemas and ordinary build/run/test warning behavior are unchanged. Lint does not write `Lang.lock` or artifacts; `--locked` verifies the current snapshot through existing preparation. No edition or artifact migration is required.

## Diagnostics and tooling

Keep schema 1, original paths, severities and primary byte spans. LSP/DAP/formatter behavior is unchanged. A future safe fix must separately specify edit validation and effect/ownership preservation. RFC 0004/0005's broader lints remain requirements for their eventual stabilization; this finite initial rule set does not claim those proofs.

## Performance and cost model

Reuse current frontend quotas per analysis and at most the manifest's bounded number of tests. The new rule visits each function's expression/local arenas once with a local-ID set. Collect at most 65,536 deduplicated findings; bounded package/source inputs still apply. No performance claim beyond these algorithmic bounds is made.

## Security and trust boundaries

Use captured package bytes and the existing capability-confined resolver. Manifest/source/lock errors precede semantic lint. No guest effects, filesystem writes, external tools or network capabilities are granted. Diagnostic messages/paths use existing escaped human or structured JSON output.

## Alternatives

- No change leaves CI without a non-executing package lint operation.
- An external text scanner lacks resolved local IDs and original semantic diagnostics.
- An independent full data-flow lint engine could prove more but duplicates compiler semantics and is unnecessary for the selected delivery.
- Merely forwarding compiler warnings is smaller but misses unread bindings; the resolved-ID rule adds a bounded, useful check without speculative effect analysis.

## Implementation plan

1. Add bounded analysis/collection and the resolved unread-binding rule.
2. Route lint options and warning-denial status without artifact publication.
3. Execute real dependency/test/JSON and effect-free fixtures, then the required baseline and both-host CI. Retain B03's reviewed 3 pt unless actual scope changes.

## Test plan

Cover pass, warning, denial, original spans, shadowed names, fields/comments, underscore suppression, write-only variables, existing unreachable diagnostics, dependency/test deduplication, invalid test/source/lock, no runnable entry requirement and host calls that would fail if executed. Verify existing outputs and lock bytes remain unchanged. Check exact diagnostic bound and overflow through the collector boundary. Run the repository baseline and both-host CI; no benchmark/fuzz campaign is implied.

## Open questions

Accepted under RFC 0001's experimental delegation, 2026-10-29 JST: reuse of proven frontend identities and a conservative resolved-ID rule is reasonably sound. Naming-based underscore suppression and the finite initial rule set are experimental product decisions, not stabilized policy. Broader lints need their own proofs and review before implementation.

## Sources

- Repository RFCs 0001 (updated 2026-10-28), 0002 (2026-10-27), 0004/0005 (2026-10-28) and 0034 (2026-10-29); inspected 2026-10-29 JST. Source implementation files have no embedded publication/update date. Existing `SRC0019` and resolved local arenas were verified in the current checkout on that date. No external comparative or measured-performance claim is used.
