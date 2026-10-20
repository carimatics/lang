<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0014-explicit-source-panic.md -->

# RFC 0014: Explicit source panic

[日本語版](../ja/rfcs/0014-explicit-source-panic.md)

- Status: implemented
- Authors: Codex, under the owner's experimental delegation
- Created: 2026-10-19
- Last updated: 2026-10-20
- Accepted: 2026-10-19
- Implemented: 2026-10-19
- Tracking issue: M2-F13
- Supersedes: None; extends RFC 0011 schema profiles and RFC 0012 private revisions
- Superseded by: None

## Summary

Implement RFC 0008's `core::panic::<T>(code: i64) -> T`. The call never returns. Preserve the dynamic code, call span, logical stack and mandatory cleanup on VM/native. Introduce ownership schema 3 and private native revision 3 only for units containing the explicit panic instruction.

## Motivation

The smallest source call currently reports SRC0024. Schema 2 has a closed instruction table; the existing native panic helper supports only fixed unwrap codes. Reusing an unrelated unwrap would obscure the accepted operation and lose its dynamic code.

## Goals

Execute explicit panic with ordinary argument evaluation and failure cleanup; preserve old artifacts and private native profiles; make divergence visible to source control-flow and ownership analysis.

## Non-goals

Catching panic, native unwinding, new source syntax, a source never type, new runtime core-service tags, C export publication or const execution.

## Guide-level design

`let mut stopped: core::Unit = core::panic::<core::Unit>(code);` terminates the current invocation even in a function returning i64. Later source remains type checked and receives existing unreachable diagnostics. A surrounding call evaluates arguments in source order until one diverges; later effects do not occur.

## Reference semantics

Use normal explicit generic and value argument checking. T is a valid source result type, including ordinary loans or allocator authority; no value of T is created. Recursively propagate divergence through evaluated expression children and statement control flow. An owning value evaluated before panic remains available to mandatory cleanup. Do not move or borrow operands that would only be evaluated after divergence.

Typed MIR adds result-free `Panic(code_register)`, requiring i64. It must be the final instruction of a block ending in Unreachable. It is one ordinary guest execution step, with existing fuel polling, no host call and no extra service work. On execution it records the dynamic signed i64 code and fails as Panic. The shared failure graph runs cleanup; an earlier panic remains primary if cleanup fails. No concurrency, capability, borrowing or FFI authority is introduced.

## Execution modes

VM, native JIT and native AOT implement the same instruction, span, stack, usage and cleanup. Generated native code calls a checked runtime helper and cannot continue successfully. Host callbacks retain their existing containment and precedence. Component execution and compile-time execution are outside this implementation.

## ABI and compatibility

Schema 2's 33 instruction tags and 37 runtime core tags remain closed. Schema 3 uses the same framing/table order, adding instruction tag 33 followed by its code register; result and span fields retain their existing encoding. Writers emit schema 3 exactly when a unit contains Panic, otherwise schema 2. Readers accept both and reject tag 33 in schema 2, or schema 3 without Panic, preserving canonical re-encoding. Version dispatch never falls back to another profile. Existing C identity serialization refusals remain.

For a whole unit containing Panic, use private symbols `lang_owned_r3_f<index>`. Keep revision 1/2 input/output packets, status meanings and helper indices 0–33. Append helper 34 for dynamic guest panic, extending the private helper table to 35 entries. The helper checks the code cell and records its fault before cleanup; invalid cells preserve their actual failure. All functions in that unit use revision 3, including C-aware units, without granting any additional C capability. Units without Panic retain revision 1 or 2. These private profiles are not external C ABI or stable plugin interfaces. No source, package or component migration is required; old readers reject new schema 3 by version.

## Diagnostics and tooling

Reuse generic arity, i64 mismatch and unreachable diagnostics. A returned loan is never fabricated. Invalid MIR continuations and wrong artifact profiles produce structured validation errors. Preserve source spans in artifacts/native metadata. No parser, formatter, LSP or debugger syntax extension is required.

## Performance and cost model

No owner allocation or copy is needed for Panic. One checked scalar read and failure dispatch precede existing mandatory cleanup. Divergent suffixes are omitted from MIR. No performance claim is made before measurement.

## Security and trust boundaries

Validate opcode/version, operand type, empty results, final position and Unreachable termination before execution. Decoded input cannot supply helper pointers or capabilities. The trusted gateway still owns context identity and checked continuation dispatch. Panic cannot bypass required cleanup or resume a poisoned instance.

## Alternatives

No change leaves an accepted source feature unavailable. A library cannot express terminal compiler control flow. A new panic terminator would duplicate existing instruction fuel/failure machinery. A fabricated generic result or unrelated unwrap invents a success path or loses dynamic codes. Unversioned opcode/helper changes violate the closed contracts. The result-free instruction reuses the validated failure graph with an explicit terminal invariant.

## Implementation plan

Add source signature/divergence and MIR validation; implement VM/native helpers and versioned artifacts; exercise source, malformed artifacts, cleanup/control prefixes and standalone AOT; then update bilingual delivery evidence. F13 is complete only when all execution paths and baseline checks pass.

## Test plan

Compile-pass/fail generic, code-type, loan and unreachable cases; nested expression/argument order; dynamic positive/negative/extreme codes and logical stack/span; cleanup failures and finite fuel/work; VM/JIT/AOT equivalence; schema 2 byte stability, schema 3 round trips and malformed version/tag/continuation refusals. Run the full baseline and request Windows evidence for the resulting snapshot.

Implementation correction (2026-10-20 JST): the [combined source](../../examples/conformance/owned_pipeline.lang) exposed false loan-activity rejection of a terminal panic with an active arena allocator loan. Rely on typed MIR's required final-Panic/Unreachable validation and defer only that exit's loan endings to mandatory failure retirement, after recording the primary panic. Normal returns, bare Unreachable and invalid success continuations still refuse. Compare VM/actual production native entry at ordinary 42/panic −42, every fuel/work/provider-refusal prefix and frame/cell boundaries, retaining original allocator/Instance retirement; actual AOT and real-C/persisted-C paths also exercise the combined source. This corrects accepted semantics without changing syntax, ABI, schemas 1–4 or native-record framing. Compiler-source changes require rebuilding SDKs under the existing exact-build policy. Sources: [loan analysis](../../crates/mir/src/unit/ownership/loan_activity.rs), [regression test](../../crates/mir/tests/ownership_loan_activity.rs), [VM/native comparison](../../tests/support/native_entry_runtime.rs), updated/verified 2026-10-20 JST.

## Open questions

Local source divergence, loan-valued specializations, VM/native differential failure/cleanup, schema compatibility and standalone AOT are validated. Windows execution for this snapshot and performance evidence remain tracked by M2-F07 and the benchmark tasks. The initial provisional 3-point estimate is not a time promise; implementation is not stabilization.

## Sources

- RFC 0008, accepted source panic contract; last updated 2026-10-19; verified 2026-10-19 JST.
- RFC 0011, closed schema-2 contract; last updated 2026-10-19; verified 2026-10-19 JST.
- RFC 0012, private native continuation revisions; last updated 2026-10-19; verified 2026-10-19 JST.
- Local source experiment at 0d43a41, 2026-10-19 JST: explicit_panic.lang reports SRC0024.

Decision: accepted under the owner's experimental delegation after checking the existing source contract and closed serialization/native boundaries. This additive, selected profile preserves older units and gives explicit panic a verifiable terminal shape. Acceptance does not claim implementation or stabilization.
