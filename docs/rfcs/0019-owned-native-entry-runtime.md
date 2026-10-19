<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0019-owned-native-entry-runtime.md -->

# RFC 0019: Ordinary owned native entry runtime

[日本語版](../ja/rfcs/0019-owned-native-entry-runtime.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-P05
- Supersedes: none; extends RFCs 0012/0013/0014/0018
- Superseded by: none

## Summary

Provide a concrete Rust runtime entry for ordinary ownership programs using their actual compiled/linked continuations and existing managed primitives. Accept an explicit checked function ID with zero parameters and i64 result, an existing cleanup Instance, an accounting Allocator<P>, and bounded preparation/control configuration. Return the value, usage and cleanup report, or structured original native/configuration/allocation failure with usage/report/stack. Do not parse source, emit code, construct JIT code, or evaluate guest MIR during execution.

Review outcome: **accepted under experimental delegation, 2026-10-20 JST**, before runtime implementation. The existing C NativeFactory executes all managed service families; P03/P18 records already execute ordinary linked code with a test runtime. The current concrete State is tied to a C-only NativeProgram trait, context Provider and C export result transfer. Generalize storage over the allocator provider and share the same primitives/pool preparation, retaining C dispatch only in its actual C specialization. The ordinary System-provider runtime and complete differential matrix remain unverified at acceptance.

## Motivation

P05 can restore native plans from files but still needs repository test runtime code to execute ordinary owners. Shipping that fixture would preserve test/frontend dependencies. Creating a fake C context/export or synthetic MIR wrapper would grant inappropriate identity or alter entry fuel/work. A source-only duplicate of the managed runtime would diverge from the already working C implementation.

## Goals

- Run the original ordinary entry and all current ownership/core families with production primitives.
- Preserve configured control/allocator accounting, cleanup, panic codes/spans, guest stack and persistent instance poison.
- Share C/native storage and guest-call machinery without granting C authority to ordinary execution.

## Non-goals

- New syntax, semantic operations, portable/native record formats, machine packets/helpers, public C/component ABI or stable plugin ABI.
- Enabling C units/host bindings by default, a VM fallback, SDK installation, or claiming P06/P07/P02 completion.
- Changing Copy native execution's trusted synchronous unmetered contract.

## Guide-level design

Decode semantics/records and bind a trusted matching image as specified in RFC 0018. Call the ordinary runtime with that program and its explicit entry ID. Existing unsafe exact-code/image/toolchain obligations remain at binding; the runtime borrows the code owner and verified domain through completion. A sealed backend program interface supplies stable prepared layouts/usage and invokes the original continuations for JIT or linked ownership code. Ordinary and C execution share storage, loans, collections/graphs, arena inspection and mandatory retirement.

A host supplies its real allocator provider; System and failure-recording providers do not need a C context. C export callers keep NativeFactory and the original C allocator/context. No native pointer, context or grant comes from decoded data.

## Reference semantics

Preflight rejects invalid/mismatched entry/domain, nonzero parameters, non-i64 return, unsupported complete C interfaces, poison, unsupported control configuration and preparation exhaustion before guest/provider effects. Require finite configured logical-frame and live-cell limits for prepared pools; the ordinary configuration may provide documented finite defaults. Apply RFCs 0012/0013 controls and shared service-work accounting. Admission safe points/failure precedence match the VM under the same controls. Reject unsupported configurations instead of running a prefix.

Prepare bounded frame/loan/capture/logical-stack/heap/arena/inspection/retirement storage before the first guest instruction. Guest frame regions use the original accounting allocator at admission. Source root arguments are empty. Preserve root return escrow through generated cleanup and ledger completion, read the validated i64 before releasing its frame, then close every pool/heap once. Mandatory cleanup remains unmetered and continues after independent failures; first failure/panic wins and secondary failures remain recorded. A boundary/host panic must persistently poison the cleanup instance, not merely a temporary Rust wrapper. Clean failures permit same-instance retry.

Generalize internal Store/Drain/State and primitives over P: allocation::Provider. Keep C argument/result transfer and import invocation in the C specialization/adapter, whose original ContextProvider identity remains checked. An explicit internal entry-mode/domain type separates ordinary root result handling from C export metadata. The ordinary program interface is sealed; neither arbitrary external callbacks nor decoded metadata implement executable code ownership. Shared plan lookup must not change or re-emit machine functions.

## Execution modes

VM semantics remain the oracle. Ordinary native execution dispatches actual compiled continuations and uses typed storage primitives, never a MIR/bytecode interpreter. JIT and linked ordinary code share the new runtime. Existing C exports retain their public API and context/allocator gate, invoking the same generalized primitives. Components/WebAssembly and async execution are unchanged.

## ABI and compatibility

No wire tags, source syntax or native private calling conventions change. Native records 1 and portable schemas 1/2/3/4 remain unchanged; ordinary execution still refuses complete C units. The new Rust backend embedding API is experimental, not a cross-version SDK ABI. Recompile with matching compiler/runtime. Copy and existing C embedding paths retain behavior.

## Diagnostics and tooling

Return structured preparation/configuration, allocation, native failure and cleanup information. Preserve explicit entry/function IDs, original spans, panic code, ordered failure reports, source stack and consumed usage even on failure. No parser/formatter/LSP change. This API does not establish debugger stepping evidence.

## Performance and cost model

Provider generalization can add monomorphizations; shared primitives avoid a second implementation. Pool preparation and semantic verification still cost work/memory. Native code performs arithmetic/control, runtime services own storage/retirement. Measure startup/runtime/size under B01–B04; no improvement is assumed.

## Security and trust boundaries

Keep native code trusted and binding unsafe under RFC 0018. Do not fabricate a C allocator/context identity, suppress capabilities or authorize callbacks through an ordinary runtime. Check limits before effects; unwind cannot bypass mandatory retirement or allow reuse of poisoned state. Regions/owners and code borrows cannot escape the invocation.

## Alternatives

- No change/test runtime distribution: retains repository-only dependencies and incomplete production startup.
- A library wrapper around VM execution: does not execute native bodies; rejected.
- Fake C export/context or extra guest wrapper: confuses authority or changes fuel/stack; rejected.
- Duplicate ordinary runtime: increases semantic drift and maintenance; share provider-generic primitives instead.

## Implementation plan

1. Generalize managed storage/provider and separate C-specific dispatch; preserve existing C fixtures.
2. Add concrete ordinary entry/complete handling and execute a Bytes source-derived entry through actual native code.
3. Verify persisted linked startup, all managed service families, guest recursion/loans, panic/cleanup, every relevant limit/provider prefix, retry and original code/allocator ownership. Update paired docs and audit P05 only after these pass.

## Test plan

Compare value, fuel/host calls/work, peak frames/cells, panic/spans/stack, reports and allocation/release behavior with VM for current representative sources. Cover normal/failed root returns, finite fuel/work/frame/cell limits, interruption/deadline, each provider refusal, cleanup failure/host panic/poison and same-instance retry. Reject wrong entries/domains/C units/configuration before effects. Retain all existing C and Copy regression tests and both-host evidence; run focused tests then the repository baseline. No runtime/codegen performance claim follows from tests.

## Open questions

No blocking semantic question. Exact Rust API names/internal factoring may follow implementation; revise this RFC before changing the entry shape, effects, controls, wire formats or compatibility promises. SDK/public startup ABI remains separate work.

## Sources

- [Native continuation contract](0012-native-ownership-continuations.md), updated 2026-10-20.
- [Service work](0013-service-byte-work-budget.md), inspected 2026-10-20 JST.
- [Native record transport](0018-native-compiler-records.md), implemented 5e0d6a1, 2026-10-20 JST.
- [Existing concrete runtime](../../crates/codegen-clif/src/c_abi/runtime.rs) and [factory](../../crates/codegen-clif/src/c_abi/runtime/factory.rs), inspected 2026-10-20 JST. State::charge already delegates to Control::charge; the C-only program trait returns a required Digest, not an optional digest.
- [Delivery board](../milestone-2.md), updated 2026-10-20 JST. These are project evidence/decisions; the new ordinary runtime remains unverified at acceptance.
