# RFC 0000: Title

- Status: proposed
- Authors:
- Created: YYYY-MM-DD
- Last updated: YYYY-MM-DD
- Tracking issue:
- Supersedes:
- Superseded by:

## Summary

Describe the user-visible change and the smallest useful example.

## Motivation

What concrete problem exists? Include representative code, measurements, or compatibility evidence. Explain why a library or tooling change is insufficient.

## Goals

- Goal

## Non-goals

- Non-goal

## Guide-level design

Explain the feature as users would encounter it, including diagnostics and common mistakes.

## Reference semantics

Specify:

- Syntax and name resolution
- Type checking and inference
- Evaluation order and side effects
- Ownership, borrowing, destruction, and allocation
- Errors, panic, unwinding, and cancellation
- Concurrency and memory-model consequences
- `unsafe`, FFI, and capability boundaries
- Behavior in constant/compile-time execution

## Execution modes

Explain equivalent behavior and any justified restrictions for:

- Bytecode VM
- Native AOT
- Optional JIT
- Embedded host calls and callbacks
- WebAssembly/component execution, if applicable

## ABI and compatibility

Address source compatibility, native ABI, component ABI, bytecode, serialized IR, package metadata, editions, and migration tooling.

## Diagnostics and tooling

Describe parser recovery, formatter behavior, error codes, fix-its, LSP behavior, debugger display, documentation, and lint rules.

## Performance and cost model

Identify allocations, reference counting, copies, dynamic dispatch, code size, compile time, runtime checks, and optimization opportunities. State what users can rely on versus what is an implementation detail.

## Security and trust boundaries

Consider untrusted source, macros, bytecode, plugins, manifests, package inputs, capabilities, resource exhaustion, and denial of service.

## Alternatives

Compare at least:

- No change
- Library/tooling solution
- The strongest competing language design
- A simpler design with fewer guarantees

## Implementation plan

Break implementation into testable vertical slices. Identify affected compiler stages, runtime, standard library, LSP, DAP, formatter, linter, package manager, and documentation.

## Test plan

Include compile-pass/fail cases, VM/native differential tests, ABI fixtures, fuzzing, benchmarks, platform coverage, and migration tests as relevant.

## Open questions

- Question

## Sources

List primary sources with publication/update dates. For living documentation without a published date, record the verification date.

