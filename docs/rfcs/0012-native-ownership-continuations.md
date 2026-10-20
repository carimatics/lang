<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0012-native-ownership-continuations.md -->

# RFC 0012: Native ownership continuations and managed frames

[日本語版](../ja/rfcs/0012-native-ownership-continuations.md)

- Status: implemented
- Authors: Codex, under the owner's experimental delegation
- Created: 2026-10-18
- Last updated: 2026-10-20
- Implemented: 2026-10-20
- Tracking issue: M2-N08, M2-N01–N07, M2-G01–G03
- Supersedes: None; elaborates RFC 0008's private native execution contract
- Superseded by: None

Extension (2026-10-19): [RFC 0014](0014-explicit-source-panic.md) selects private revision 3 for whole units containing explicit Panic. It preserves packets/statuses and helper indices 0–33, appending dynamic panic helper 34. Other units retain revision 1/2.

Extension (2026-10-20): [RFC 0015](0015-c-native-object-bridge.md) selects private bridge revision 1, artifact-qualified symbols and object-resident import thunks for explicit C-aware objects. Existing packets/helpers and ordinary/JIT namespaces remain unchanged.

## Current implementation status

The logical guest-chain gap found by the 2026-10-20 review is fixed. Revision-1 read-only descriptors expose prepared frames; [actual both-host validation](https://github.com/carimatics/lang/actions/runs/39427222132) at exact head 73dca1d verifies main→debug_outer→debug_take, parent source call sites, native stepping and normal 42 completion. Together with production continuation, service-family and SDK/platform evidence, this completes this RFC's implemented scope. It is not stabilized.

## Debugger descriptor clarification (2026-10-20, accepted)

Expose the logical chain through read-only repr(C) debug records in the production runtime's pre-reserved active frames. The driver local debug_view contains revision 1, the first record address, frame count and stride. Each record contains the original FunctionId, SourceId and start/end byte offsets. Records run outermost to innermost; the preceding record is the logical parent. Callers retain their call site; the innermost record retains the latest charged instruction span, with the physical PC supplying its exact current source location.

Read only while guest execution is stopped; reacquire address/count after a runtime transition. Rust code never dereferences the exported addresses, and debugger readers validate count/stride. Existing frame admission accounts for capacity and bytes; dispatch adds no allocation or guest effects. Custom Runtime implementations default to unsupported revision 0. This private tooling descriptor changes neither Input/Output packets, helper indices, bytecode nor the public C ABI. Completion requires actual three-deep owned calls and parent source call sites on both hosts.

## Summary

Compile ownership MIR into native continuations using the existing two-pointer/i32-status machine convention. A bounded host driver dispatches compiled function addresses; managed values remain in the runtime's prepared frames. Guest arithmetic, branches and continuation selection are machine code. The driver does not interpret MIR or bytecode and does not fall back to the VM.

This decision supplies the missing ownership call/context/publication contract before N01. It does not complete the ownership backend, C ABI adapters, platform execution, performance or debugger gates.

## Motivation

The current native API accepts RFC 0006 Copy `VerifiedUnit` values flattened into i64 cells. Ownership execution instead uses independently verified failure MIR, `layout::Layouts`, `FramePlan`/`FrameStorage`, service tokens, initialization order and retirement work areas. Copying a 32-byte token is not an ownership transfer. Aggregate payload bytes do not include their cleanup metadata.

Direct recursive native calls would introduce a platform stack limit independently of the accepted guest frame/cell budgets. Use prepared logical frames and a constant-depth dispatch loop so recursive ownership calls have explicit admission and retirement boundaries. This is a project design choice; no speed advantage is asserted.

## Goals

- Preserve RFCs 0007/0008 source semantics, limits, failure precedence, return escrow, ordered cleanup and allocator authority across VM/native execution.
- Keep runtime layouts and proofs independent of Cranelift and preserve the existing Copy native ABI.
- Define an implementable native boundary with no implicit host allocation during guest call dispatch.

## Non-goals

- Stabilizing a source ABI, exposing raw managed tokens to C, changing C profile 1, bytecode schemas 1/2, component ABI or package formats.
- New syntax, user generics, async suspension, migrating a running native frame between threads, or native sandboxing.
- Claiming that the feasibility experiment executes ownership source or live managed values.

## Guide-level design

Source remains unchanged. [The recursive owner fixture](../../examples/scripts/owned_calls.lang) moves Bytes through `carry`, then borrows it for `length`; the current VM prints `1`. Native execution must eventually produce the same result, release each owner once, and match failure/limit cases. No native CLI command is introduced by this RFC.

Reject an unsupported native operation or admission configuration with a structured diagnostic before guest/provider effects. Source-operation errors retain their original span; host configuration errors identify the rejected limit and related entry span when available. Do not run only the supported prefix or substitute VM execution.

## Reference semantics

### Private revision 1 packets

Tier-1 targets use 64-bit pointers. The machine signature remains target-C `fn(input_pointer, output_pointer) -> i32`. Ownership symbols use the private whole-unit namespace `lang_owned_r1_f<function-index>`, distinct from Copy `lang_unit_f<index>`. The packets below use target-native C alignment; they are not serialized Rust values or an independently linkable interface.

| Input field | Offset / width | Meaning |
| --- | --- | --- |
| context | 0 / pointer (8 bytes) | Opaque, uniquely admitted invocation context |
| frame | 8 / u64 | Context-local logical frame identity |
| continuation | 16 / u32 | Checked continuation in that frame's function |
| reserved | 20 / u32 | Zero |

Input size/alignment is 24/8. Output size/alignment is 16/8:

| Output field | Offset / width | Meaning |
| --- | --- | --- |
| frame | 0 / u64 | Admitted next logical frame |
| function | 8 / u32 | Compiled whole-unit function index |
| continuation | 12 / u32 | Its checked continuation |

Status **0** means the outer invocation returned; **1** dispatches the next continuation; **2** reports terminal failure. Only status 1 publishes an output packet. Status 0/2 leaves the packet untouched; results and structured failures reside in context-owned storage. No other status is valid. A callee's successful return dispatches its caller; it is not an outer return.

Only the trusted gateway constructs packets. Before calling generated code it checks program/context identity, live frame identity, function/continuation bounds, zero reserved bits, instance poison state and non-reentrancy. Before dispatch it validates the complete proposed target against the context's prepared transition. Malformed transitions fail closed. Neither source nor decoded artifacts supply raw pointers or authority. Generated entry guards reject invalid continuations before effects.

### Private revision 2 for explicit C imports

Accepted amendment, **2026-10-19 JST**, under RFC 0001's experimental delegation. Explicit C-aware ownership units use **lang_owned_r2_f<function-index>**. Ordinary ownership units retain revision 1 and their existing symbols. Revision 2 preserves the input/output packet sizes, field offsets, two-pointer/i32 machine signature and terminal/dispatch statuses above; it is not an external C export signature.

The matching runtime retains revision 1's helper prefix (indices 0–32, unchanged signatures/meanings/offsets) and appends the declared-C-import helper at index **33**. A runtime may retain this extended table while executing revision-1 code, which accesses only the original prefix. Revision-2 emission and function metadata use the revision-2 namespace; an ordinary revision-1/object gateway cannot enable C execution by accepting that symbol spelling alone.

The new helper receives a statically prepared import identity, argument/result cell locations and source span. It performs the declared foreign service through the RFC 0008 context gate, not decoded MIR control flow. Before guest/provider effects, require the actual code owner, complete C metadata and explicit grants, the context's once-bound accounting allocator, and matching control/shared-work configuration. Charge one host call only after complete C value admission; preserve raw foreign status separately from guest panic and native continuation status. Validate/copy results before source publication and preserve ordinary failure cleanup and poison behavior. The first implementation supports scalar/Unit imports; unsupported complete import profiles reject before effects. Aggregate imports and external export/AOT support remain required M2 work.

Rationale and alternatives: leaving C execution refused does not deliver M2; an operation-decoding library path does not satisfy native lowering. An unversioned extra helper hides a contract change. Renaming every ordinary revision-1 unit imposes migration without changing its semantics. A separate revision-2 C-aware namespace with a preserved revision-1 prefix makes the extension reviewable while retaining current ordinary objects. Rebuild C-aware code with its matching runtime; do not relabel previously emitted code. Future incompatible packet/status/helper/layout changes still require another RFC and private revision. C profile 1 descriptors/digests/headers, source syntax, schemas 1/2 and their C-identity refusal are unchanged.

Feasibility evidence, verified 2026-10-19 JST: compiler-produced C callbacks checked against generated typedefs return 42/59 through source VM and native execution, and 32,896 across 256 parameters. Finite fuel/work prefixes, foreign failures and code/allocator provenance have focused fixtures. These pre-revision probes motivate the decision, not final revision-2 or Windows evidence. Fresh revision-2 checks, native Windows/MSVC execution, general aggregate/export/factory integration and performance measurements remain unverified. No performance claim or new source/FFI authority follows from this amendment.

### Prepared frames and runtime operations

The context owns the verified failure MIR identity, source map, compiled-address table, grants, allocator, control/usage state, services, logical frame stack, return escrows and failure reports. All host wrapper pools and report buffers are prepared before outermost guest entry. Guest payload and retirement/work storage are admitted at the same activation points as the VM, before argument transfer, using `FrameStorage::admit`; they are not hoisted into an eager guest allocation pool. Preserve provider request order/sizes, attempted charges and rollback. Compile/layout/frame budgets are cumulative; no proof stage resets earlier usage. Overflow or unsupported capacity fails before entry rather than silently reducing requested limits.

Payload, initialization/order metadata, logical loans, service identities and iterative cleanup areas use the existing runtime layouts. A packet contains no owning value. A raw token copy or native stack spill never creates a second owner. Scalar arithmetic/comparisons/branches and continuation choice are lowered into machine instructions. Typed, bounded runtime helpers load/store admitted locations, transfer managed roots, resolve projections and invoke actual services; they are not an operation-decoding interpreter.

A guest call reserves the callee activation and return escrow before consuming arguments, commits the verified argument transfers, records the caller continuation/source call site and dispatches the callee. The driver selects a compiled address from its sealed table. A return prepares its result, executes the generated failure/cleanup edges, and publishes into the caller's reserved destination only after required cleanup succeeds. Failed preparation, cleanup or publication follows the existing failure MIR; no half-return becomes observable. Releases follow reverse initialization order and preserve original fault/call/cleanup spans.

Native guest recursion uses logical frames, not recursive driver calls or growing C call stacks. A single machine continuation may run ordinary basic blocks until a call, return or terminal failure; helper invocations must use bounded/iterative service and cleanup algorithms. Host callbacks retain the accepted reentrancy restrictions.

### Limits, failure and authority

The owned native profile is instrumented. Fuel charging/poll positions, host-call charging, frame/cell admission, interruption, allocator outcomes and usage reports must match the shared VM contract for identical inputs, grants and limits. Runtime helpers do not count as user-declared host calls. The existing uninstrumented Copy profile remains unchanged and explicitly distinct.

Structured failures are recorded in the context. The generated failure CFG drives cleanup before terminal status 2. Provider/host callback panics cannot unwind across the private C boundary: adapters contain them, preserve the accepted failure precedence and poison the instance when invariants cannot be restored. No guest execution resumes from poison. Unsafe pointer erasure is restricted to the trusted gateway/helper shims with documented packet extent, lifetime, exclusivity and no-retention invariants.

Source typing, inference, evaluation order, lexical loans, arena brands, concurrency/atomicity and explicit capabilities retain their accepted meanings. This synchronous private execution contract adds no new constant evaluation or async behavior. Runtime helpers receive only explicitly granted allocator/host authorities; native execution remains trusted code, not a capability sandbox.

## Execution modes

The bytecode VM remains the independent semantic comparison path. Native AOT and JIT use the same continuation lowering, packets, layouts and runtime helpers. AOT emits real machine functions and links a matching trusted driver/runtime. A compiled continuation never enters the VM. C exports enter/leave the synchronous ownership gateway and apply C profile 1 validation/publication; internal packets are not C export signatures. Wasm/component mappings remain deferred.

## ABI and compatibility

Affected domain: `native-abi` within one toolchain build. Existing Copy symbols, two-pointer/status calling convention, typed host imports, source diagnostics, C profile 1, schema-1 golden bytes and schema-2 ownership artifacts are unchanged. Every native ownership unit and its runtime are built together; independently compiled ownership objects are not a compatibility promise. Check private revision/program identity before invocation. A later packet/status/helper/layout-contract change requires an explicit RFC revision and private symbol/descriptor revision before emission. No user-source migration is required.

## Diagnostics and tooling

Carry each continuation's original FunctionId, source span, call site and logical parent in the private context/debug descriptors. Use structured native verification/admission/unsupported-operation failures; preserve existing runtime failure identities. No parser, formatter or LSP grammar changes are needed. Debug metadata must support stepping native code at guest source locations. Because hardware frames do not represent the whole guest call chain, debugger integration must expose the logical chain from descriptors; a raw driver-only backtrace does not complete G03. Actual macOS/Windows stepping and backtrace evidence remains required.

## Performance and cost model

Calls/returns incur fixed packet stores, admission/runtime work and one dispatch-loop transition; owner transfer still incurs required metadata/loan/cleanup work. Host wrappers are prepared before entry; managed payload/metadata and service allocations retain the VM's fallible activation/operation points. Generated native code and compile metadata have explicit compilation budgets. Cranelift's internal allocation remains a trusted compiler boundary, not a fully accounted host-memory bound. No host container growth is hidden in dispatch. Scalar blocks may run without returning to the driver; fuel/poll effects cannot be optimized away. Dispatch overhead, code size, optimization quality and platform costs remain unmeasured until the existing M2 benchmark gates.

## Security and trust boundaries

Accept only fully verified ownership failure MIR, including independent region/loan checks. Validate foreign inputs at their own gateway before creating live managed values. Raw context addresses, frame IDs, generated addresses and private descriptors never enter portable artifacts or source values. The executable owner outlives every call and releases unpublished code on error. Helpers neither retain packet pointers nor expose temporary host references. Native plugins remain trusted; this profile grants no filesystem, network, environment, process or FFI authority by itself.

## Alternatives

- **No change / Copy API:** cannot represent managed metadata, loans or return escrow.
- **Library wrapper around the VM:** can execute current source but does not implement native ownership lowering.
- **Direct recursive native calls:** preserves familiar hardware backtraces and may cost less per call, but needs a separate native-stack admission/overflow contract and platform evidence. Choosing it later requires an explicit revision.
- **One helper per decoded MIR operation:** reuses interpreter dispatch but fails the machine-code lowering objective.
- **Selected continuations:** share bounded runtime primitives while emitting actual operations/control flow; accept the explicit call/return dispatch and logical-debug-chain costs.

## Implementation plan

1. **N08:** this decision and a narrow machine-code/target-object feasibility experiment.
2. **N01:** implement production packet/context/frame/helper admission and native function/return layout emission. After N08/N01, demonstrate ownership source execution before more foundation work; split any demonstrated independent prerequisite before implementing it.
3. **N02–N05:** lower managed moves/aggregates/loans, core services, shared/arena operations and generated failure/cleanup/control. Preserve every accepted payload family and source result, not a scalar-only substitute.
4. **N06/N07, F/P/Q/B/G/Z rows:** actual macOS/Windows object/link/run, C boundary, packaging, combined conformance, measurement, debugger evidence and the final bilingual capability guide remain required.

## Test plan

`owned_abi_probe` executes two generated functions on the host: 1,000 call/return sequences compute 42 per iteration without a MIR/bytecode evaluator, preserve inert 32-byte data, and reject invalid continuations without state/output mutation. It checks runtime Bytes layout versus aggregate retirement metadata and emits/inspects symbols in aarch64 macOS Mach-O and x86-64 Windows COFF objects. This proves packet/dispatch feasibility, not live ownership, source lowering, C validity, performance, target link/run or target debugger behavior.

Production tests must exercise recursive owned/loan source calls; every provider refusal/fuel prefix/interruption point; invalid contexts/frames/transitions; moved/initialized state; ordered cleanup/failure precedence; result escrow; poisoned-instance rejection; allocation-free wrapper reuse; all accepted service/payload combinations; and VM/JIT/AOT equivalence. Existing schema-1/2 compatibility fixtures stay unchanged. Native adapters need narrow unsafe-boundary tests and actual tier-1 execution/debug evidence. Fuzz only bounded validated metadata; do not execute arbitrary untrusted native code.

## Open questions

No blocking contract question remains for N01. Performance, production helper integration, platform linker/runtime integration and debugger reconstruction remain unverified implementation assumptions. Keep their existing rows/gates open and re-estimate from actual experiments; this acceptance does not establish their completion.

Accepted under RFC 0001's experimental delegation on **2026-10-18 JST**. Rationale: preserves the established machine signature and managed runtime invariants, avoids an unbounded native recursion stack, has a concrete host/target-object prototype and retains full source/native/platform obligations. Explicitly revise this RFC if the production experiments contradict its assumptions.

## Sources

- [RFC 0001](0001-project-charter.md), experimental delegation: last updated 2026-10-17; verified 2026-10-18 JST.
- [RFC 0007](0007-source-ownership-and-core-values.md): last updated 2026-10-18; [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md): last updated 2026-10-17; both verified 2026-10-18 JST. [RFC 0011](0011-ownership-artifacts.md): last updated 2026-10-18; verified 2026-10-18 JST.
- Local [Copy native lowering](../../crates/codegen-clif/src/program.rs), [managed layouts](../../crates/runtime/src/layout.rs), [frame admission](../../crates/runtime/src/layout/frame.rs), and [ABI experiment](../../crates/codegen-clif/src/owned_abi_probe.rs): inspected/verified 2026-10-18 JST. Cranelift 0.136.3 is resolved by the repository lockfile; its local API has no publication/update date recorded here, verified 2026-10-18 JST. No external performance claim or web research is used.
