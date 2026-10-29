<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0040-bounded-debug-adapter-sessions.md -->

# RFC 0040: Bounded debug adapter sessions

[日本語版](../ja/rfcs/0040-bounded-debug-adapter-sessions.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-D01–D05
- Supersedes: none
- Superseded by: none

## Summary

Add `lang dap`, a bounded stdio Debug Adapter Protocol adapter over captured package source. Its first executable outcome prepares a Copy VM program and stops before a real source instruction without replay. Subsequent deliveries provide stepping/inspection, native launch on both tier-1 hosts and one actual mixed VM/native stack.

## Motivation

Source spans and failure stacks already exist, but the Copy VM currently consumes its Machine in a whole execution call. The ignored `target/m4-debug-vm-probe` observes main's original call span at zero fuel and main→answer spans at fuel one; fresh ordinary execution returns 42 with fuel four. Fuel failure discards that execution: the experiment proves source/stack information, not pause/resume. Expose retained execution rather than reproducing effects by replay.

## Goals

- Reach a source breakpoint in a real package before broader debugger scaffolding.
- Preserve call order, cells, program identity, source spans, cumulative controls and original effects across stops.
- Supply bounded read-only frames/variables/evaluation and actual native/VM frame correspondence.
- Complete D03/D04 both-host native and mixed-stack obligations before G7 closure.

## Non-goals

- New source syntax, bytecode formats, component ABI, time travel or frame/value mutation.
- Arbitrary process attach, arbitrary debugger commands or effectful evaluation.
- A JIT debugger registration system, optimized-variable recovery or a native plugin sandbox.

## Guide-level design

Launch the stdio server with `cargo run -- dap`. A client first initializes, then launches an explicitly authorized package:

```json
{"root":"/workspace/app","manifestPath":"Lang.json","mode":"vm","stopOnEntry":false}
```

`root` is an explicit existing local workspace directory. The relative manifest, optional package/target selection and all admitted dependencies remain inside it, using the package graph's existing guards. No ancestor search, arbitrary executable or ambient source read is implied. VM launch initially admits the existing Copy source profile; unsupported ownership/task/FFI profiles refuse before effects. This restriction must remain explicit in capabilities/documentation, while native launch and actual mixed stacks remain mandatory M4 outcomes.

`initialize` returns implemented capabilities. Prepared `launch` responds and emits `initialized`; configuration requests may then bind source breakpoints. `configurationDone` permits execution. Breakpoint responses report verified actual positions or an explicit unavailable reason; a requested empty/comment line may advance to the next admitted executable point. The adapter respects negotiated path/URI and line/column bases; IDE columns use UTF-16. Source requests return captured bytes, including after disk mutation.

## Reference semantics

### Retained execution

Factor the existing Copy VM preparation/run path into an additive debugger Session. Ordinary execution and the debugger use the same verified instruction implementation, argument/import preflight, call/return copies, parallel block-argument moves, control order and failure construction. A pause occurs before the next instruction/terminator's charge or effects and retains the exact Machine. Resume continues that Machine; it cannot reset fuel/host-call usage, re-run initializers or repeat a callback. Interruption/deadline refusal takes precedence over publishing a pause. Existing deadlines include time spent paused.

The ordinary API remains semantically unchanged. Debug sessions are not cloneable snapshots. Completion/failure is terminal; later resume refuses. Dropping a Copy session releases its buffers. No new guest allocation, arithmetic, ownership, borrowing, destruction, panic, concurrency or compile-time semantics are introduced. Existing instrumented profiles retain their cleanup and accounting contracts.

### Stops and inspection

The current point contains function/block/instruction identity, depth and original optional span. Bind breakpoints to concrete executable points, not a textual occurrence of a function name. After a stop, continue skips that point exactly once so a later loop/reentry can hit it again. Source stepping compares admitted source positions and call depth; no source position is invented for missing metadata.

Frames are innermost first; callers retain the child's original call site, not an already advanced caller PC. Thread/source references are bounded target identities; source references name immutable captured bytes until target termination. Frame/variable references additionally belong to one pause epoch. Resume, restart, termination and relevant native transitions invalidate stale frame/variable inspection references. VM inspection is read-only. D02 must establish initialized/live source-local locations through an in-memory, target-neutral lowering sidecar; unknown locations are unavailable rather than guessed. Such metadata does not become an unversioned serialized field.

Evaluation initially reads proven visible identifiers/fields and bounded scalar/Copy previews. It cannot call guest/host functions, mutate values, mint capabilities or expose uninitialized cells. Paused state stays on its owning thread; adapters exchange owned bounded snapshots/commands, never move synchronous component/owner sessions across threads. A separate pause request flag may stop at an execution safe point; cancellation uses the original control/cleanup path.

### Protocol and lifecycle

Use DAP Content-Length/ASCII-header/UTF-8-body framing and its seq/type/command/request_seq envelopes, not LSP method envelopes. Correlate every response and monotonically number adapter messages. Advertise only implemented configuration, stepping, inspection, restart/cancel and native capabilities. Unsupported requests produce bounded unsuccessful responses. D01 supplies initialize/launch/configuration/source breakpoints/threads/stack/source/continue/disconnect; later capabilities land with their fixtures.

Launch captures immutable source/semantic/portable state before execution. Do not answer from another target or an old pause. Disconnect/restart/cancel must release owned target/debugger resources and cannot leave a new execution active. Native process termination follows its explicit trusted-process contract; existing language cleanup guarantees do not imply recovery after external process death.

## Execution modes

- VM: the initial Copy Session shares the actual interpreter and cumulative controls. No host callback registration, FFI or capability is granted by launching source alone.
- Native AOT: explicit `mode: native` grants trusted execution of the selected compiled package through the matching SDK/receipt and an OS debugger. D03 must prove real source breakpoints on macOS and Windows. Existing ordinary Copy native execution remains its trusted unmetered profile; VM instruction budgets are not attributed to it.
- Mixed stacks: D04 must execute a real trusted native↔VM boundary and preserve actual frame/call-site order in one stopped invocation. Compose proven live descriptors/boundary identities with physical frames; never concatenate unrelated runs or guess from symbol text. Native backend/tool selection and bridge mechanics require executable evidence before advertisement.
- Optional JIT registration and broader ownership/task/source-variable profiles are separate capabilities; no silent VM fallback supplies a requested native session. Existing component/C execution behavior is unchanged.

## ABI and compatibility

Additive experimental Rust/session and DAP APIs only. Source, MIR semantics, portable schemas, package/lock metadata, editions, public C/component ABI and native SDK validation remain unchanged. Keep backend/process types out of frontend APIs. Source-local sidecars use semantic/MIR identities and explicit host limits. Any later persisted format change requires a reviewed version/migration amendment first. Existing whole-run APIs retain exact results, usage and failure precedence; users need no artifact migration.

## Diagnostics and tooling

Reuse compiler/package diagnostics and original sources. Stdout contains framed DAP only. Report malformed configuration, outside authority, stale IDs, missing metadata, unsupported profiles, quota refusal and debugger/tool failures without exposing ambient paths. LSP/formatter/linter behavior remains unchanged. Source breakpoints, stacks and values are observations of this captured target, not text matching or last-successful editor facts.

## Performance and cost model

Retain existing finite input/compiler/VM controls. Initial adapter bounds: 4 MiB message bodies, 8 KiB headers, 256 breakpoint requests, 256 frames per response, 65,536 inspection references, 32 MiB logical debug-index bytes and 10,000,000 query/index visits. Bound copied text/value previews and encoded responses before publication; resource refusal returns no partial successful prefix. Quotas are host policy/logical accounting, not exact heap or CPU-time measurements. Inspection does not charge guest fuel; resumption does not replenish it. Trusted host callbacks remain cooperatively bounded by their existing contract.

## Security and trust boundaries

Explicit launch root grants admitted source reads; it grants no guest filesystem/network/environment/process/FFI capability. Native compilation/debugger execution uses explicit trusted native mode, validated package/SDK inputs and host-selected tools. Never pass protocol/source strings as shell or debugger command fragments. No eval-based command escape, arbitrary memory writes or unvalidated process attach is provided. Validate every framing, path, span, ID, count, version and control transition before effects or output. Preserve source snapshots after admission and never use an editor overlay to expand authority.

## Alternatives

- No change retains only whole-run failures and manual native debugging, leaving G7 unmet.
- A library observer alone cannot retain state for a stdio debugger; replaying to a stop would repeat callbacks and reset controls.
- Delegating entirely to a native debugger/DAP implementation supplies physical frames but cannot establish this VM's source locals or mixed logical boundary identities. Reuse native tooling behind a bounded host interface where supported.
- A second debug interpreter or text-derived stacks are smaller initially but duplicate semantics and cannot prove effects, spans or control parity. A retained existing Machine is the simpler sufficient VM design.

## Implementation plan

D01 accepts this contract and delivers the first actual VM source breakpoint. D02 adds stepping, initialized locals, bounded read-only evaluation and pause authority. D03 proves native source debugging on both hosts. D04 supplies the real mixed-stack fixture. D05 audits lifecycle/cancel/malformed/resource behavior and both-host sessions. Keep one delivery doing and expose a new prerequisite/scope estimate before implementing one. Compiler/runtime/source semantics remain independent from debugger vendor choice.

## Test plan

Compare ordinary and stopped/resumed execution for exact result, usage, failures and callback counts, including stop-before-effect, recursion, loops, block moves and repeated same-location stops. Test stale frame/source/variable IDs, snapshots after file mutation, private/outside/unadmitted paths, malformed UTF-8/JSON/framing, finite quotas, interruption/deadline order and disconnect/restart/cancel. Use actual stdio clients plus real macOS/Windows native debugger runs and one mixed boundary invocation. Retain existing VM/native/C conformance and the full baseline; no benchmark or fuzz campaign is inferred.

## Open questions

Accepted under RFC 0001's experimental delegation, 2026-10-29 JST. Reusing the verified Machine and explicit snapshot/authority boundaries is reasonably sound; the finite fuel probe proves original spans and nested frames before effects. True retained resumption, source-local sidecar costs, native debugger/tool choice and mixed-boundary correspondence remain unverified delivery obligations. Acceptance does not close G7 or authorize pretending unsupported profiles are implemented.

## Sources

- [Official DAP overview](https://microsoft.github.io/debug-adapter-protocol/overview) and [specification](https://microsoft.github.io/debug-adapter-protocol/specification): publication/update dates unavailable; freshly verified 2026-10-29 JST. Framing, message envelopes, initialization/configuration and source/stack requests are protocol facts; initial profiles/limits are project decisions.
- RFC 0006 (updated 2026-10-28), RFC 0012 (2026-10-28), RFC 0013 (2026-10-24), RFC 0014 (2026-10-28), RFC 0020 (2026-10-29), architecture/roadmap and current VM execution loop; inspected 2026-10-29 JST. Code publishes no embedded update date.
- Ignored `target/m4-debug-vm-probe.log`, verified 2026-10-29 JST: fuel 0 observes main bytes 50..58; fuel 1 observes answer bytes 24..26 plus its parent call; fresh execution returns 42 with fuel 4. This is finite pre-design evidence, not a retained-session or native debugger result.
