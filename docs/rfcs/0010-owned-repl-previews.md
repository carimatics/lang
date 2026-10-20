<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0010-owned-repl-previews.md -->

# RFC 0010: Owned REPL result previews

[日本語版](../ja/rfcs/0010-owned-repl-previews.md)

- Status: implemented
- Authors: Project contributors
- Created: 2026-10-18
- Last updated: 2026-10-20
- Implemented: 2026-10-20
- Tracking issue: M2-V14/V16/V15/V10/V17 in [the delivery board](../milestone-2.md)
- Supersedes: None; extends the RFC 0006 REPL to the RFC 0007 ownership profile
- Superseded by:

## Current implementation status

Verified 2026-10-20 JST against the [M2 completion audit](../milestone-2-capabilities.md). The intended VM REPL interface, wrapper inference, bounded previews and retirement are implemented. RFC 0016 also completes V17's indirect-service preview follow-up; native REPL execution remains an explicit non-goal. See tests/repl.rs, crates/runtime/src/layout/storage/preview.rs and the M2 guide. This is implemented, not stabilized.

The motivation, original staged plan and dated partial checkpoints below record the decision/rollout history. Their former pending gates are closed by the evidence above; normative contracts and explicit non-goals remain in force. This status reconciliation changes no source, ABI or format semantics.

## Summary

Admit analyzed ownership definitions and evaluate ownership expressions in transactional REPL sessions. Infer only the private generated expression wrapper's concrete result. Show a bounded, canonical, read-only preview of supported returned values, retire every guest result before exposing output, and retain no mutable session values. This specifies tooling output, not new source literals or serialization.

## Motivation

At `5c32e25`, a valid `fn owned() -> i64` containing Bytes is rejected before REPL item commit; calling it then diagnoses an unknown name, while a later `1 + 2` prints `3`. `frontend::finish` routes expression inference through the Copy checker; `repl::Session` requires Copy execution before committing; the owned VM entry requires i64 main. RFC 0006 specifies Copy display, but RFCs 0007/0008 leave owning result previews and post-display retirement unspecified. A host library cannot repair the checker boundary, protect a staged return, or decide source-independent canonical output.

These are implementation observations verified 2026-10-18 JST, not performance measurements.

## Goals

- Preserve transactional definitions, original-source diagnostics, fresh VM counters and execution isolation.
- Display every supported current-profile result, including Bytes/Text nested in records, enums, Option and Result.
- Preserve normal move/loan/region rules, result escrow, allocation authority and cleanup precedence.
- Bound rendering without guest callbacks, guest clones, raw addresses or persistent guest owners.

## Non-goals

Top-level variables, user-defined display methods, general function return inference, text/byte literal syntax, object reflection, new file/host authority, native REPL execution, owning wire encoding and stable ABI are outside this RFC. It does not define future Vec/Map/shared/weak/arena preview formats; those remain gated until an explicit follow-up defines them. Their source operations and milestone outcomes remain required.

## Guide-level design

Each line below is a separate submission; a complete function may also span lines.

```text
> fn fresh() -> core::Bytes { return core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); }
> fresh()
core::Bytes []
> core::bytes_new(core::allocator())
core::Result::Ok(core::Bytes [])
> 1 + 2
3
```

The CLI explicitly supplies allocator authority for ownership evaluation, as it does for owned scripts. Embedding hosts explicitly supply an allocator through an ownership-capable submission API; the existing submission API grants none. Definitions require analysis but no runtime allocator. Missing authority fails before guest effects. Unsupported reachable operations/results fail before effects; defining an unused unsupported function does not execute it.

Committed items are source definitions only. A result is rendered and destroyed within its submission, including on display failure. Later submissions cannot refer to a previous heap value. `:reset` clears definitions, and errors leave prior definitions intact. A normal panic with successful cleanup permits the next fresh evaluation; a poisoned execution instance is never reused.

## Reference semantics

### Syntax, inference and admission

Keep the existing one-item-or-one-expression submission grammar. Public/user function annotations remain explicit. For the private synthesized wrapper only, infer the single returned expression without an expected result, then apply the normal concrete result, Copy/storage, move, loan, region and MIR checks. Validate the existing wrapper restrictions and prevent calls to it from user definitions. No result type is guessed for an ambiguous generic constructor such as context-free `core::Option::None`; ordinary inference diagnostics require a concrete context. Ordinary/provisional loans and escaping arena values gain no exemption from normal function-result restrictions.

Items commit after complete shared frontend analysis and MIR verification where applicable; they need not be executable in the current backend. Evaluate an expression by a checked function identity, not by renaming a user `main` or trusting its signature. A user `main` remains an ordinary library function in a session. The script entry contract remains zero-argument i64 main.

### Canonical preview

Keep RFC 0006 scalar and nominal formatting: signed decimal i64, `true`/`false`, qualified record/case names, declaration-order record fields and comma-space separators. Empty records use `Name {}`; nonempty records use `Name { field: value }`; cases with payloads use `Name::Case(value, ...)` and zero-payload cases omit parentheses.

Extend the same iterative traversal with:

| Value | Form |
| --- | --- |
| Unit | `core::Unit {}` |
| Option/Result | `core::Option::None`, `core::Option::Some(value)`, `core::Result::Ok(value)`, `core::Result::Err(value)` |
| Finite core error | Its accepted qualified constructor name and recursively displayed payload, if any |
| Bytes | `core::Bytes []` or `core::Bytes [0, 255, ...]`, each byte decimal in logical order |
| Text | `core::Text "..."`, preserving the exact valid UTF-8 scalar sequence without normalization |

Text preview content is ASCII. Emit printable ASCII U+0020–U+007E directly except quote/backslash, which use `\"` and `\\`. LF/CR/TAB use `\n`, `\r`, `\t`. Every remaining scalar uses `\u{h}` with lowercase hexadecimal and no leading zeroes, including NUL as `\u{0}`. The enclosing quotes are literal quote characters. These are preview escapes, not source string syntax. Qualifying core names does not add inferred type arguments or make the preview round-trip source. Never display allocator identity, address, capacity, hash order or private metadata.

Only the supported profile may be previewed. Read through checked managed storage and service tokens; malformed tags, invalid UTF-8 or invalid tokens are structured runtime faults, not text to print. No formatter may call guest code, perform a semantic clone/retain, or dereference a raw foreign address.

### Ownership, allocation and failure ordering

1. Evaluate with a fresh VM counter set and cleanup instance. Use the explicitly supplied instance allocator for all guest storage. Its host-selected cumulative quota may span submissions; do not silently reset it. The CLI supplies a fresh allocator per ownership expression. Hosts may intentionally reuse an authorized allocator and retain its cumulative charges.
2. Stage the return and complete ordinary callee/entry cleanup under RFC 0008. On execution/cleanup failure, do not render or expose a value; retain the original primary, ordered secondaries, panic code, poison and source stack.
3. Hold a successful owning result in protected managed storage while building the preview. Rendering has no guest safe point or host/user callback. It consumes no additional guest instruction fuel. It cannot publish partial text.
4. On render success, retire the result once as normal consumption/ScopeExit, then release all admitted guest frame/result storage. Only complete cleanup success exposes the finished host string. Result-cleanup failure discards the string, becomes primary if no earlier failure exists, and poisons the instance.
5. On render limit/allocation/validation failure, retain that failure as primary, destroy the unpublished result, and continue independent cleanup. Cleanup failures are ordered secondaries and poison according to RFC 0008. Teardown must not retry consumed obligations. A host diagnostic transport must preserve this precedence.

Fresh instances isolate guest poison; host effects are not rolled back and host-owned allocator/provider lifetimes remain the host's responsibility. Abort, corrupt foreign state and invalid provider behavior retain RFC 0008's trust boundary. There is no concurrency, atomic publication, unsafe, constant-evaluation or scheduling extension.

## Execution modes

The initial REPL continues to use the VM and independently checked typed MIR. Runtime preview traversal and result-consumption rules must be usable by a later native adapter without frontend/backend-specific source types. AOT/JIT and component calls retain their existing contracts; this RFC authorizes no native REPL, C export, host binding, component resource or ownership decoder tag. Unsupported execution/result kinds remain pre-effect gates until implemented. Pure Copy sessions keep their existing host-call behavior; mixed ownership sessions use the same reachable-operation gates as owned script execution.

## ABI and compatibility

Existing valid Copy submissions/output, item transaction rules, reset, file authority and noninteractive prompt behavior are unchanged. Ownership items previously rejected at the Copy boundary may now commit; an embedding host must opt into allocator authority to execute ownership expressions. Document the ownership-capable submission API without silently authorizing the existing API. New canonical output is a tooling contract requiring an explicit RFC revision/follow-up before a later change. It is neither source syntax nor a versioned artifact. No edition migration, package bytes, schema-1 golden bytes, C layout or stable native ABI changes.

## Diagnostics and tooling

Retain immutable original submission/module snapshots and map diagnostics, fixes, runtime faults and each call frame back to them. Never expose generated wrapper names/lines: its visible name is `<expression>`. Failed reports remain valid after later submissions/reset. Display-limit diagnostics do not commit items, persist a result, hide cleanup failure or print a partial prefix. Source parser/formatter/LSP spelling is unchanged; source result annotations and move/loan/region diagnostics remain canonical. This RFC does not implement general DAP or linter work.

## Performance and cost model

Keep the 65,536-byte default display cap. Charge/reserve bounded host output and traversal storage before guest entry, separately from guest payload quotas; reservation failure prevents effects. Rendering is iterative with a bounded depth stack, including zero-sized/deep Copy aggregates. Check output budget before copying names/content; Bytes/Text length supplies a lower bound that can reject an over-limit preview without scanning an arbitrarily larger buffer. No guest storage is allocated just to render; a host string copies the displayed data but retains no guest token. Work is bounded by validated type/layout preparation, capped output and mandatory cleanup. These are required algorithmic bounds, not measured speed or memory claims.

## Security and trust boundaries

Do not grant filesystem/network/process/environment/FFI or host-call authority through a preview. Use checked type identity, locations, tags, spans and buffer provenance; do not treat a pointer-like display marker as authority. Use fixed formatting, independent of guest overloads. Enforce limits at entry/output and preserve cleanup even when output fails. Explicit host effects and quarantine/teardown remain observable through the structured failure report, without publishing unsafe raw values.

## Alternatives

- **No change:** retains the unusable ownership REPL boundary and leaves V10 incomplete.
- **Library/helper functions:** a user can return i64 lengths, but cannot implement owning result inference, protected retirement or canonical aggregate output from a library.
- **Persistent values plus user display traits:** more interactive power, but adds source lifetimes, dispatch, callbacks, cycles, authority and reset semantics beyond M2's text-only session contract.
- **Opaque summaries or i64-only evaluation:** simpler, but hides supported returned contents and does not complete V10's result-display outcome.
- **Readable raw Unicode and source-round-trip strings:** useful future ergonomics, but introduces a different output/literal contract. Choose exact deterministic ASCII preview escapes here; source literals remain a separate decision.

## Implementation plan

1. **V14:** review and record this explicit follow-up under RFC 0001's experimental delegation; update both document languages and the board.
2. **V16:** add bounded managed traversal and named zero-argument result execution, protected preview/consumption and a real owned-result source fixture. Complete this execution outcome before adding further compiler foundation.
3. **V15:** extend only generated-wrapper result inference through normal HIR/MIR checks, preserving ordinary function annotations and precise sources.
4. **V10:** connect item admission, explicit allocator authority, runtime dispatch, retained failure mapping, CLI output and submission isolation. Extend existing tests and bilingual docs.
5. **V17:** after real indirect-service adapters, decide the explicit follow-up and implement their bounded result previews with VM/REPL evidence. Preserve Map non-iteration and shared/weak/arena authority rules; this row retains the remaining result family rather than silently shrinking M2 to Bytes/Text.

Later service adapters retain their original milestone tasks; enabling their preview formats requires an explicit follow-up. General native/debug/conformance/measurement exit evidence remains required. Acceptance alone completes no implementation gate.

## Test plan

Use actual source-derived execution and real managed buffers: empty/NUL/all-byte Bytes, ASCII/quote/backslash/control/multibyte/extreme Text, canonically equivalent but byte-distinct strings, core errors and nested nominal/sum values. Compare scalar/aggregate Copy output with existing fixtures. Test exact/one-below/zero output caps, deep/zero-sized traversal, malformed storage/tokens, pre-entry host allocation limits, guest OOM/fuel/interruption, failure during result cleanup, ordered/truncated reports, exactly-once release and no partial output. Session/CLI tests cover valid/invalid ownership definitions, inferred expressions, ordinary annotation/loan/region rejection, missing allocator, source authority, old snapshots/stacks after reset, fresh VM counters and later recovery. Pure Copy host-effect tests remain valid. No real process OOM, fuzz-campaign, benchmark or native-platform result is implied; run appropriate existing compiler/fuzz compile checks for implementation changes.

## Open questions

No unresolved semantic blocker for the current payload profile. Prototype/measure formatter preparation cost and mixed-session compilation before performance claims. Future indirect service formats and native preview integration require later explicit design/evidence; they do not relax existing milestone outcomes.

## Decision and implementation status

Review outcome: **accepted under experimental delegation on 2026-10-18 JST**, applying the owner's RFC 0001 delegation. The design fills the concrete REPL boundary without changing user function inference, source literals, loan escape rules or execution authority. Protected result consumption and render-failure precedence remain consistent with RFC 0008; the supported-profile boundary and V17 retain the remaining M2 result family. No blocking semantic question remains for this profile.

Evidence: the original REPL boundary probe rejected a valid owning definition. V16 executes [the named-result library](../../examples/analysis/repl_preview.lang) through `lang_vm::unit::owned::execute_preview`, with Bytes/Text/nested results retired before string publication. V15 infers validated wrapper results through ordinary HIR and independent MIR checks. V10 now connects ownership items/expressions to sessions and CLI with explicit allocator authority, fresh instances, mapped failures and source-only persistence. The 24-test REPL suite and report-adapter unit test cover actual contents, transactions, limits, OOM/interruption, quota reuse, exact release, source/reset lifetime, Copy host effects and failure precedence transport. [The CLI transcript](../../examples/repl/owned.repl) succeeds. See [preview](../architecture-history.md#owned-result-previews), [inference](../architecture-history.md#ownership-expression-inference) and [session evidence](../architecture-history.md#ownership-repl-sessions).

Historical checkpoint, 2026-10-18 JST (superseded by the implementation status above): implementation was **partial**. V16/V15/V10 implement the current-profile preview, inference and session/CLI path. Later indirect-service formats/implementation remain V17's explicit follow-up; the original M2 exit requirements are unchanged. Preparation cost and mixed-session compile cost are unmeasured, and indirect formats/native preview remain unverified. Any later change to this accepted contract requires an explicit revision/follow-up before implementation, updating both languages together.

## Sources

- [RFC 0001](0001-project-charter.md): experimental decision delegation; last updated 2026-10-17, verified 2026-10-18 JST.
- [RFC 0006](0006-source-frontend-and-portable-programs.md): transactional Copy REPL, canonical values and counters; last updated 2026-10-17, verified 2026-10-18 JST.
- [RFC 0007](0007-source-ownership-and-core-values.md): owned results, exact bytes/UTF-8, normal loan/storage/region rules and explicit allocator authority; last updated 2026-10-18 JST.
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md): protected returns, precedence, bounded reports and poison; last updated 2026-10-17, verified 2026-10-18 JST.
- [REPL](../../src/repl.rs), [frontend wrapper dispatch](../../src/frontend.rs), [HIR inference boundary](../../crates/hir/src/program.rs), [owned entry](../../crates/vm/src/unit/owned.rs) and [REPL tests](../../tests/repl.rs): original boundary evidence checked at `5c32e25` and by the piped probe, verified 2026-10-18 JST. The later V16/V15 implementation evidence is listed below.
- [Managed preview](../../crates/runtime/src/layout/storage/preview.rs), [VM adapter](../../crates/vm/src/unit/owned/calls.rs), [source tests](../../tests/support/owned_preview.rs), [storage tests](../../crates/runtime/tests/support/preview.rs) and [cleanup fault tests](../../crates/vm/src/unit/owned/failure_tests.rs): V16 implementation, updated/verified 2026-10-18 JST.
- [HIR inference](../../crates/hir/src/program.rs), [body checking](../../crates/hir/src/program/bodies.rs), [semantic analysis](../../crates/semantics/src/analysis.rs) and [inference tests](../../tests/support/owned_inference.rs): V15 implementation, updated/verified 2026-10-18 JST.
- [Session](../../src/repl.rs), [owned adapter/report transport](../../src/repl/owned.rs), [CLI](../../src/main.rs), [REPL tests](../../tests/repl.rs), [ownership tests](../../tests/support/repl_owned.rs) and [transcript](../../examples/repl/owned.repl): V10 implementation, updated/verified 2026-10-18 JST.
- [Delivery board](../milestone-2.md): scope/dependencies and provisional estimates, updated/verified 2026-10-18 JST. This RFC records a project tooling decision; it makes no external language or performance claim.
