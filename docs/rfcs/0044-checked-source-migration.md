<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0044-checked-source-migration.md -->

# RFC 0044: Checked source fixes and historical migration

[日本語版](../ja/rfcs/0044-checked-source-migration.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-30
- Last updated: 2026-10-31
- Tracking issue: M5-B01–B05 in the M5 delivery board
- Supersedes: none
- Superseded by: none

## Summary

Add preview-first checked source fixes and an explicit migration from this language's historical scalar syntax to edition `2026`. Ordinary compilation retains its present grammar and finite edition set. Neither a current-edition no-op nor adding a package manifest establishes migration completion.

## Motivation

The real compiler at `69b1c4dc66d04a4a103d4eb843232df22e52eac0` executes a source using local names `shared` and `weak` to `42`; the current compiler rejects those identifiers with four `SRC0024` diagnostics. Its historical 315-byte artifact still executes to `42` now. The [retained probe](../evidence/2026-10-30-m5-legacy-source-probe.json) identifies source, producer, lock and artifact hashes. This separates source migration from bytecode compatibility. A second probe shows that the existing editor's `LNT0001` rename fix preserves an initializer and execution result. Batch tooling must carry that proof across declared tests and checked writes.

## Goals

- Offer deterministic reviewable previews, explicit writes and structured refusal.
- Preserve resolved behavior and effects for a finite set of recipes.
- Migrate an actual historical source through current VM/native execution on both hosts.

## Non-goals

Inventing another edition; accepting old syntax in ordinary compilation; a general historical compiler; arbitrary user-authored patches; changing public or foreign symbols; moving module files; formatting unrelated bytes; changing lock files or dependencies; multi-file rollback against concurrent writers.

## Guide-level design

```text
lang fix --manifest-path PATH [--package NAME] [--target TRIPLE] [--apply] [--message-format human|json]
lang migrate --from-profile rfc0006-scalar --to-edition 2026 --source-root DIR [--apply] [--message-format human|json] FILE
```

Both commands preview by default. `--apply` computes and validates a fresh private plan before writes; a prior JSON report is not executable input. Duplicate/unknown options, unsupported profiles/editions and incompatible argument combinations fail before writes. `fix` uses the selected edition-2026 package, including its declared test entries. `migrate` requires an explicit historical profile and source root; it does not infer them from parse failures or ambient manifests.

For example, migration renames declarations and all references of historical `shared`/`weak` locals to fresh `_migrated_shared_1`/`_migrated_weak_1`, leaving their initializers, addition and return unchanged. The reported names depend deterministically on identifiers already present. Comments are preserved even when they contain the old spelling.

## Reference semantics

### Current-source fixes

The first recipe is `LNT0001`: prefix an unread local/parameter with `_`, using RFC 0039's resolved-identity rename and semantic equivalence proof. Keep the declaration, initializer, assignments, evaluation order, control flow and cleanup. Do not delete apparently unused expressions. A collision, ambiguous identity, foreign/unsupported declaration or failed proof refuses the plan. This is not general automatic application of parser suggestions.

Capture the selected package entry and every declared test entry, respecting its selected target. Analyze reachable dependencies for proof but edit only selected-package sources. Report the covered roots and files. Recheck every affected root after all edits; a test-only source cannot disappear from coverage because the editor's original snapshot only captured the entry. Merge identical overlapping observations of a fix; reject conflicting edits. An unselected package is never silently modified.

### Historical scalar admission

`rfc0006-scalar` is a migration-only subset of the source at the pinned historical commit, not an additional supported compilation edition. Admit one UTF-8 file with private scalar functions, `i64`/`bool` parameters/results/locals, mutable local declarations and assignment, `if`/`else`, `while`, `return`, scalar literals, parentheses, names, calls, addition and less-than. Require `main() -> i64`. Preserve RFC 0006's declaration-before-use, function-wide local-name uniqueness, typing, RFC 0002's wrapping addition, left-to-right evaluation and control-flow rules. Use the historical operator/keyword set: accepting newer constructs merely because the current parser accepts them is forbidden.

Reject modules/imports/qualified paths, public/foreign declarations, aggregates, patterns, ownership/borrowing, tasks, host capabilities and every construct outside that subset, with no writes. In particular, `break`, `continue`, strings and subtraction are not introduced into this old grammar. Their spellings may be ordinary identifiers where the historical grammar permits them. Old identifier syntax is ASCII and excludes lone `_`; comments and CRLF retain their exact bytes. Apply historical UTF-8, NUL, token and nested-comment validation before rewriting.

Read a bounded historical token/AST view. For each identifier spelling newly reserved by current syntax, and the now-reserved root spelling `core`, choose a distinct valid name absent from every identifier in the input. Rewrite all identifier-token occurrences of that spelling consistently, including private function declarations and references. Never rewrite comments, literals or substrings. The mapping is injective and globally fresh; all unaffected names remain fixed, so no scope acquires a capture. Validate the historical scalar structure and resolution before transformation, then compare the current parsed/resolved scalar structure under that mapping, including types, calls, values, operators and statement order. A parse-only success is not a semantic proof. Keep source-to-output offset maps for diagnostic and proof comparison.

There is no allocation, destruction, cancellation, unwinding, unsafe/FFI or concurrency transition in this profile. Their absence is an admission restriction, not a claim to migrate them. The recipe preserves runtime evaluation and errors; it adds no compile-time execution. A valid input with no conflicts produces an unchanged report and does not by itself demonstrate a source transition.

The [pinned-producer admission probes](../evidence/2026-10-31-m5-scalar-admission.json), verified 2026-10-31 JST, make the historical rules concrete: locals need explicit types, parameters are immutable, block locals do not escape, and local names remain unique across the entire function even in disjoint branches. A local binding shadows a same-named callable and cannot be called. Unreachable names are still resolved, all required return paths are checked, and forward/recursive private calls are admitted. B05 must preserve these conditions before rewriting; successful parsing under the current grammar cannot replace them.

The same probes admit an omitted `else` and trailing parameter/argument commas, while requiring `mut` on local declarations. These are part of the existing historical scalar grammar, not extensions or optional simplifications for the migration reader.

### Plans and publication

Freeze manifests, source bytes and filesystem identities used by every analysis, including unmodified dependencies/tests. Plans have private constructors and ordered original byte spans; JSON serialization exposes information, not write authority. Validate semantic proofs and encoded output limits before staging writes.

Reuse RFC 0036's publication boundary: explicit write authority, regular canonical inputs/directories, no symlink or hardlink alias, complete staging before final frozen-input revalidation, atomic replacement per file, and exact committed-prefix reporting if a later replacement fails. A stale untouched input or manifest invalidates the entire unpublished plan. Cooperating writers must honor the documented exclusive-write window; the command cannot guarantee rollback or defeat a malicious writer racing path checks. Do not overwrite a concurrent edit during attempted recovery.

## Execution modes

Fixes preserve source meaning for VM and native AOT through the existing shared typed MIR contract. The migration acceptance fixture must execute before migration under the pinned historical producer and after migration through current VM and native binaries. Current execution of the historical artifact is additional format evidence. Optional JIT, embedded callbacks and Wasm acquire no new semantics or automatic migration support from this RFC.

## ABI and compatibility

Source edition stays `2026`; the historical profile is identified separately. Bytecode, component C v1, private native ABI and package schemas do not change. Renaming private scalar functions changes display/debug names as reported; public/foreign symbols are excluded. Current-source fixes retain the existing rename tool's ABI restrictions. Changed package source invalidates its exact source checksum; regenerate locks through the normal explicit package workflow. Never rewrite versions, manifest editions, dependency constraints or `Lang.lock` merely to make the fix succeed.

## Diagnostics and tooling

Report path strings retain the platform's native separators, as package diagnostics do; use native path APIs to interpret them. Both-host fixtures compare exact root identities and order with native paths.

Use schema-1 JSON reports with operation, profile/edition, covered roots/files, ordered edits (original byte spans, replacement and diagnostic/recipe identity), applicability, changed/applied counts and outcome. Order by authority-relative canonical path then original byte offset. Encode paths with the established package diagnostic rules. Human output conveys the same coverage and refusals. Usage errors follow `CLI0001`/exit 2; operational refusals use structured migration/fix diagnostics and exit 1; successful preview/apply, including no-op, exits 0. Reserve exact diagnostic identifiers during implementation without reusing unrelated codes.

Ordinary parser recovery and existing LSP behavior stay unchanged. Editor quickfixes and batch fixes share proof logic, while batch coverage/publication is explicit. Formatting, debugger data and documentation are regenerated from the edited source normally; the tool does not patch cached IR or debug artifacts.

## Performance and cost model

Bound aggregate source bytes, roots, tokens, AST/proof work, candidates, replacements and report bytes over the whole operation. Reanalysis must charge one shared work budget rather than resetting it for each rename. Reuse frozen snapshots where proven safe. Preview allocates original/replacement bytes and proof data; apply additionally stages complete replacements. No runtime overhead is added. Implementation must publish concrete defaults and exact-limit tests; there is no unmeasured latency or memory claim.

B02 preview defaults (2026-10-31): 64 roots, 256 fixes, 65,536 token edits, 256 MiB cumulative analyzed source bytes, 1,000,000,000 aggregate work units, 64 MiB cumulative replacement bytes and a 16 MiB encoded report. Existing editor/compiler limits remain independently enforced. Each index or proof pass reserves its complete editor work allowance before running; a checked rename reserves both its index and edit/proof passes. Source-byte charging includes each analysis and the underscore growth of the prospective rename. These conservative reservations can reject a request before all individual maxima are reached. Report encoding is measured through a bounded counting writer, without first allocating an oversized JSON buffer. These are admission limits, not measurements of CPU time or peak resident memory.

B05 preview defaults (2026-10-31): one source of at most 1 MiB, 4,096 encoded path bytes, 65,536 tokens/nodes, 256 functions, comment depth 64 and recursion depth 128 (an unraisable ceiling). Each existing parse/module/binding/type phase reserves 1,000,000 work units from a 16,000,000-unit shared budget; token reading, candidate names, rewrites and structural proofs debit the same budget. Cumulative analyzed source bytes are limited to 8 MiB, rewritten source to 4 MiB and encoded JSON to 8 MiB. Reservations may refuse before an individual limit is reached. Boundary tests lower every configurable limit to its exact successful threshold and reject one less, including escaped report paths. `MIG0001` denotes unsupported/invalid source or failed equivalence; `MIG0002` denotes exhausted migration limits. The byte-only preview library has no filesystem or execution authority. These defaults are admission limits, not measured throughput or resident memory.

## Security and trust boundaries

Compiler reservations include independent task-syntax classifiers even for scalar input: one before historical scalar checking and two in the current semantic facade/HIR pipeline. Together with the scalar checks and current parse/module/binding passes, they reserve eight phase allowances before completion. These internal scans do not enable task migration. A small-source regression refuses a six-million-unit aggregate budget even when each one-million-unit phase could succeed independently.

Source and manifests are untrusted data. Analysis must not execute source, native code or host imports and obtains no network/process capability. Source-root/package authority bounds reads and a separate explicit authority bounds writes. Refuse nonregular/aliased/stale inputs, malformed UTF-8, unsupported legacy syntax, ambiguous edits and exhausted limits without publishing an unvalidated plan. Do not accept an arbitrary JSON edit program or trust a user-supplied applicability label. Per-file atomicity and cooperative-writer limits must be visible in CLI documentation.

## Alternatives

- No change leaves users to discover reserved-name breakage and reimplement safe renames.
- An editor/library-only solution already provides a useful proof, but does not cover declared tests or a batch filesystem transaction. Reuse it inside these commands.
- Cargo's `fix --edition` is a strong source-first migration model, but its documentation limits analysis by selected features/targets. It does not establish this project's exact-input publication guarantees.
- Plain token substitution plus successful compilation is simpler, but does not independently establish historical admission/resolution, protect all inputs, or preserve external names.
- A full historical frontend or new edition is broader than this outcome. Reject unsupported historical constructs explicitly and extend only with another reviewed recipe.

## Implementation plan

1. B01 accepts this contract and audits the historical prerequisite before implementation.
2. B02 adds package fix previews with entry/test coverage and existing rename proofs.
3. B03 reuses checked publication and proves stale/conflict/late-failure behavior.
4. B05 adds bounded historical scalar admission, fresh-name proof and the real `shared`/`weak` preview fixture.
5. B04 exposes the explicit migration CLI and validates before/after VM/native journeys on macOS and Windows. G2 closes only after both checked fixes and this real transition pass.

This affects frontend/editor proof adapters, package/format publication and CLI/tests/docs. Runtime and ABI changes are unnecessary. The additional historical reader/proof slice is a newly exposed 3 pt prerequisite; retain B04's 3 pt for integration and both-host evidence rather than hiding the new work there.

## Test plan

Exercise unread locals/parameters and preserved initializer/assignment effects, test-only modules, dependencies/unselected packages, UTF-8/CRLF spans, collision refusal, exact limits and deterministic repeat-run no-op. Check frozen manifests and untouched inputs, symlink/hardlink/directory aliases, staging failure and exact committed prefixes. Check human/JSON errors and lock consequences.

For migration, retain historical source/artifact hashes; cover each newly reserved spelling, scope/call resolution, fresh-name collisions, comments, unsupported syntax/ABI/module boundaries and malformed input. Compare normalized pre/post semantic structures and use generated small scalar programs for differential validation. Execute the retained real fixture with both current VM/native backends on both hosts. Run the full repository baseline and latest-head CI before closing the implementation deliveries. Do not count manual edits, packaging or same-edition no-ops as implemented migration.

## Open questions

Accepted under RFC 0001's experimental delegation on 2026-10-30 JST. Rationale: explicit historical admission, a globally fresh name mapping, resolved-structure comparison and reuse of existing checked rename/publication boundaries make the narrow recipes reviewable without changing normal compilation. The retained historical producer establishes a real transition. As of 2026-10-31, B05's bounded reader/proof preview passes the full baseline and both-host CI at `7f41dd0`, including execution, refusal, semantic counterexamples and exact limits. The migration CLI, checked application and native execution journey remain B04 work. Acceptance is not G2 completion. Concrete aggregate budget defaults and diagnostic allocations are implementation decisions with exact-limit tests, not authorization to broaden the grammar. Additional historical profiles and future editions need separate reviewed recipes. No blocking design question remains for the selected slices.

## Sources

- [RFC 0002](0002-prototype-control-flow.md), updated 2026-10-27: two's-complement wrapping addition. Correction on 2026-10-31: the original draft incorrectly called addition checked; an executable overflow probe and the canonical arithmetic contract disproved that wording. This corrects the migration specification without changing compiler semantics or compatibility.

- [RFC 0006](0006-source-frontend-and-portable-programs.md), current update 2026-10-30; historical revision at the pinned commit updated 2026-10-17: source lexical, scalar and module rules.
- [RFC 0007](0007-source-ownership-and-core-values.md), updated 2026-10-21: reserved ownership names and migration diagnostics.
- [RFC 0036](0036-checked-package-formatting.md), updated 2026-10-31: checked publication and committed-prefix limits.
- [RFC 0039](0039-package-editor-snapshots.md), updated 2026-10-30: rename and quickfix proofs.
- [RFC 0043](0043-editions-and-compatibility.md), updated 2026-10-30: editions and independent compatibility domains.
- [Cargo Book: cargo fix](https://doc.rust-lang.org/cargo/commands/cargo-fix.html), publication/update date unavailable; verified 2026-10-30 JST. Source-first migration and analysis coverage are sourced facts; this RFC's grammar, proof and write guarantees are project decisions.
- [Historical probe](../evidence/2026-10-30-m5-legacy-source-probe.json), produced 2026-10-30 JST. Manual renamed-source VM execution and native generation were also observed that day; they are feasibility observations, not an implemented migration command or native execution result.
