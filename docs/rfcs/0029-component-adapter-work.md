<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0029-component-adapter-work.md -->

# RFC 0029: Component adapter work and invocation metering

[日本語版](../ja/rfcs/0029-component-adapter-work.md)

- Status: implemented
- Authors: Project contributors
- Created: 2026-10-24
- Last updated: 2026-10-24
- Implemented: 2026-10-24
- Tracking issue: M3-P02
- Supersedes: None; elaborates RFC 0013/0027 for the component profile
- Superseded by: None

## Summary

One component root invocation owns the existing `ServiceWork` counter from argument preparation through result publication. Canonical value observations, payload copies and UTF-8 validation consume it; guest operations, services and native adapters share that exact counter. Finite component execution remains unavailable until every admitted operation is covered. This decision records RFC 0027's required event catalog before enabling it.

Accepted under RFC 0001's experimental delegation on 2026-10-24. The original-counter design and complete-event admission are reasonably sound; implementation and differential refusal evidence were P02 obligations at acceptance.

Implementation checkpoint (2026-10-24 JST): the root counter and scoped Session reach the shared Machine/services. Stored native-import work prefixes, owner/loan and rich-result boundaries, unused reentry-method refusal, first preparation failure, fresh-root reset, primary/suppressed destructor failure and exact UTF-8/copy/zero-byte-node units are covered by VM/generated-native and wire regressions. Existing wire bytes match. P02 remains doing until full baseline, fuzz compile/lint and commit evidence; stabilization and both-host evidence remain later work.

## Motivation

At `3f35d0c`, a decoded resource/native-import fixture succeeds with unbounded controls. The same fixture configured with service-work limit zero returns `Unsupported` on VM and generated native, before argument commit, with zero provider calls. The shared runner rejects finite imports; its ordinary service counter is created after component argument preparation. Canonical adapters already use one finite wire Session, but that preparation budget is not the invocation's service counter. These are code/probe observations verified 2026-10-24, not performance evidence.

## Goals

- Bound untrusted metadata and adapter preparation with the existing cumulative byte/work/storage/element/depth limits.
- Share runtime work and refusal ownership outcomes across the actual VM/native component path, including rich values, imports and original cleanup.

## Non-goals

New syntax, another allocator/registry/interpreter, CPU-time bounds, metering arbitrary trusted native instructions, new persistent formats, or completing the remaining public embedding/reentry, combined heap/control, installed-package and both-host stabilization deliveries.

## Guide-level design

Use the existing `Control::and_service_work(Some(limit))` on the explicit checked component invocation. A refused runtime event returns the existing structured service-work control failure, retaining limit, used, requested and original operation span. Admission/codec preparation errors retain their existing identities. Exhaustion alone does not poison; mandatory cleanup and invariant revalidation decide reuse. The ordinary CLI still requires explicit component bindings.

## Reference semantics

### Preparation and root identity

Source/IDL/artifact verification, snapshot pairing, grants, provider metadata, signature construction and adapter planning retain their finite preparation Budget/Session. Each preparation operation carries its prior usage without refunds; the whole grant/provider batch and a call's canonical processing cannot reset their supplied Session. These checks are preparation work, not additional service events. Unused imports/declarations must pass before discovery/publication; no native constructor is a compatibility query.

Create one `ServiceWork` at component root-invocation preparation, using the actual trusted invoker's control configuration. The shared Machine receives that same handle instead of creating a second counter. Validate control/counter correspondence before effects. Root canonical argument operations and result publication use it even when they precede guest entry or follow the final guest instruction. Earlier admitted work is not refunded on a rejected attempt, callback failure or later call. A fresh outer invocation starts a fresh counter; future nested calls must inherit the parent counter.

### Component event catalog for cost profile 1

| Event | Units |
| --- | --- |
| Observe one dynamic managed/canonical value node while translating or encoding/decoding a component value | 1 |
| Copy n payload bytes, including an actual runtime copy of a native frame/report buffer | n |
| Validate n UTF-8 bytes | n |
| Copy/grow/replace typed managed collection data | Existing RFC 0013 Layout/active-extent rules |
| Fixed envelope/slot/header checks, claim/index/loan bookkeeping and opaque handle transfers | 0 |

Count each actual value observation once in its traversal; aliases and static type expansion do not invent dynamic values. Separate source-to-arena, arena-to-wire, wire-to-arena and arena-to-guest traversals are distinct events. A zero-byte Unit array still observes its elements. Aggregate reconstruction, scratch IDs and allocator initialization do not invent payload copies. Known copy/validation batches precede dependent allocation/publication; an admitted validation may discover a data error, while a refused validation may not scan bytes. Bulk events charge their complete extent. Reuse managed-service metered entry points and their shared counter; do not add another charge for the same copy or observation.

Returning a checked raw backing view does not itself copy/scan its bytes. Arbitrary native reads/writes through that view remain trusted opaque work. Actual runtime backing publication copies are charged; moving a context buffer into backing is a handle transfer. Native buffer allocation/zeroing is allocation accounting, not service work. Guest fuel, host-call charging and cooperative interruption positions remain unchanged.

### Refusal, cleanup and provenance

Local bounds precede the corresponding global event; a refused event changes neither applicable work counter. Distinct earlier preparation/events retain their admitted usage. Pre-commit refusal preserves caller owners; post-commit refusal retires accepted values rather than rolling back. Reserve representations before committing resources. A result stays in escrow until caller representation, mandatory callee cleanup and invariant checks permit publication.

Wire/marshalling service-work failures project to the original control failure, including overflow, rather than malformed-host-result poison. Retain numeric usage even for failure during root preparation. Preserve the first primary and ordered suppressed cleanup failures. All mandatory loan ending, temporary/result discard, resource destruction and service retirement use the original allocator/table/code owners and remain unmetered. This includes refusal during native result decoding or guest result staging.

## Execution modes

Both component adapters use the shared Machine and canonical services. Before finite execution, inspect the complete verified unit, including unused functions/imports; require the checked component Program, all typed grants and complete event coverage. Ordinary task/C/ownership profiles retain their existing admission predicates and costs. Generated native bodies remain generated code. Optional JIT follows existing availability rules; installed packages, public callbacks and both-host runtime conformance retain their later delivery gates.

## ABI and compatibility

This extends cost profile 1 only for the previously unavailable finite component profile. Existing non-component program charges and fuel/poll order remain unchanged. Default unbounded component behavior is retained apart from defined usage/overflow accounting. Source syntax/types, C ABI v1, LCMP3/LCVL1/LCSG1, LANGBC5 and native-package/SDK formats gain no fields or tags; existing report/control identities already carry the numeric failure.

The experimental Rust component invoker/host interfaces must expose the actual root work configuration/counter. A wire Session temporarily retaining the synchronous `ServiceWork` handle is no longer Send/Sync, even when idle; create preparation/execution sessions on their execution thread. Immutable contracts and serialized bytes need no runtime counter. Scope guards restore a Session's previous association on return/unwind; foreign counters cannot replace an active root. These are explicit Rust API consequences, not a stable C layout change.

## Diagnostics and tooling

Retain structured preparation/codec diagnostics and original source snapshots. Service-work refusal uses the existing control/report identity and numeric fields; it must not become a generic component-frame or native-contract error. Parser, formatter and source inference are unchanged. Debug/callback stack completeness remains E03/E04.

## Performance and cost model

Reuse one synchronous Rc/Cell counter and existing iterative traversals. Scoped Session association, counter checks and retained preparation buffers have costs; no overhead or speed claim follows. Logical units do not bound CPU time, allocator initialization or trusted native work. Z01 retains measurements.

## Security and trust boundaries

Source/artifacts/IDL/value frames remain untrusted and bounded, and grants precede provider discovery. Counter sharing grants no native/resource authority. A malformed native result remains a host-contract failure; work refusal alone does not. Native code is trusted and must contain foreign exceptions. Cleanup cannot publish or reenter, even after exhaustion.

## Alternatives

- No change preserves finite component refusal and leaves P02 incomplete.
- Caller/library counters do not cover shared services, early preparation or native-result cleanup.
- Full transactional preplanning of the complete call is stronger but cannot know arbitrary native result work in advance and adds plans/traversals; bounded admitted prefixes with escrow/cleanup preserve ownership.
- Fresh counters per adapter call refund prior work. Charging preparation-Budget implementation steps as runtime events is simpler but makes alias expansion/metadata algorithms and fixed framing part of the service cost. The selected logical events preserve the existing service model.

## Implementation plan

Within P02's selected 3-point outcome: first establish early root-counter identity and forwarding without opening incomplete finite admission; then connect value/copy/UTF-8 and managed staging events with control projection; finally enable whole-unit finite admission and prove actual decoded resource/native-import refusal, exact ceilings and cleanup on both engines. Reuse the existing table, allocator, Session, services and Machine. Expose/re-estimate a concrete missing prerequisite before implementing beyond this outcome.

## Test plan

Run decoded source/native-import fixtures first. Compare VM/native exact usage, zero/exact/one-below and failure prefixes, provider-call prefixes, original spans, poison/reuse and allocation/destructor balance. Cover Unit and zero-byte collections, scalar/rich values, Text/Bytes/list/record/variant/Option/Result, resource owners/loans, malformed/unused imports and late refusal, report bytes, overflow, multiple calls and preparation failures. Preserve existing local quota precedence, unmetered cleanup, old profile refusal/costs and exact wire bytes. Run the repository baseline and bounded fuzz-target compile/lint; campaigns, both-host execution and performance require separate evidence.

## Open questions

No blocking semantic question remains. Unverified implementation assumptions are early-counter forwarding cost, complete staging coverage, failure-context reuse and scoped Session ergonomics. Nested public callbacks, combined heap/frame/task/interruption controls, installed packages and both-host execution remain required later work.

## Sources

- [RFC 0013](0013-service-byte-work-budget.md), [RFC 0027](0027-production-component-interface.md) and [RFC 0028](0028-task-component-artifacts.md), last updated 2026-10-24. RFC 0013's previous service-only catalog was last updated 2026-10-22.
- Shared runner/service preparation, component execution/canonical codecs and `tests/component_source_resource.rs` at `3f35d0c`, inspected/probed 2026-10-24 JST. Code has no embedded update date. The local zero-limit probe shows `Unsupported`, no argument commit and zero provider calls on VM/generated native; it does not prove the proposed metering implementation.
