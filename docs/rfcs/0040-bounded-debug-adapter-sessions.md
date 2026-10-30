<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0040-bounded-debug-adapter-sessions.md -->

# RFC 0040: Bounded debug adapter sessions

[日本語版](../ja/rfcs/0040-bounded-debug-adapter-sessions.md)

- Status: implemented
- Authors: project maintainers with Codex
- Created: 2026-10-29
- Last updated: 2026-10-30
- Tracking issue: M4-D01–D05
- Supersedes: none
- Superseded by: none

## Summary

`lang dap` implements a bounded stdio Debug Adapter Protocol adapter over captured package source. D01–D05 provide retained Copy VM pause/step/inspection, native launch on both tier-1 hosts, an actual granted mixed VM/native stack, and lifecycle cleanup. G7 is complete; evidence and remaining profile exclusions appear under Open questions. The motivation records the initial experiment, before retained execution was implemented.

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

The VM implementation captures checked lowering environments as an optional in-memory sidecar; ordinary and debug lowering produce identical verified MIR. It tracks initialized registers only for retained debug frames and queries suspended caller call points before their pending return destinations. Scope/variable/evaluate queries run on the original worker. Per-query bounds are 1,024 values, 1,024 copied cells per value, 16,384 copied cells in total, 1 MiB logical preview text and ten million visits. Evaluation paths have at most 32 components and 4,096 bytes. Source/frame/variable references share the target reference allowance; live reference paths additionally consume the 32 MiB logical debug index allowance. Refusal emits no partial successful variable list.

### Protocol and lifecycle

Use DAP Content-Length/ASCII-header/UTF-8-body framing and its seq/type/command/request_seq envelopes, not LSP method envelopes. Correlate every response and monotonically number adapter messages. Advertise only implemented configuration, stepping, inspection, restart/cancel and native capabilities. Unsupported requests produce bounded unsuccessful responses. D01 supplies initialize/launch/configuration/source breakpoints/threads/stack/source/continue/disconnect; later capabilities land with their fixtures.

Launch captures immutable source/semantic/portable state before execution. Do not answer from another target or an old pause. Disconnect/restart/cancel must release owned target/debugger resources and cannot leave a new execution active. Native process termination follows its explicit trusted-process contract; existing language cleanup guarantees do not imply recovery after external process death.

## Execution modes

- VM: the initial Copy Session shares the actual interpreter and cumulative controls. No host callback registration, FFI or capability is granted by launching source alone.
- Native AOT: explicit `mode: native` grants trusted execution of the selected compiled package through the matching SDK/receipt and an OS debugger. D03 must prove real source breakpoints on macOS and Windows. Existing ordinary Copy native execution remains its trusted unmetered profile; VM instruction budgets are not attributed to it.
- Mixed stacks: D04 must execute a real trusted native↔VM boundary and preserve actual frame/call-site order in one stopped invocation. Compose proven live descriptors/boundary identities with physical frames; never concatenate unrelated runs or guess from symbol text. Native backend/tool selection and bridge mechanics require executable evidence before advertisement.
- Optional JIT registration and broader ownership/task/source-variable profiles are separate capabilities; no silent VM fallback supplies a requested native session. Existing component/C execution behavior is unchanged.

### Native launch elaboration (2026-10-30 JST)

Select tools only through trusted host configuration: LLDB_DAP_EXECUTABLE may identify the Windows adapter, and LLDB_PYTHON_RUNTIME/LLDB_PYTHON_DLL may identify a matching Python runtime, applying PATH/PYTHONHOME only to that child. The official LLVM 23.1.4 x64 MSI proves Windows breakpoint/caller/step/guest exit/adapter exit 0 with the python3.dll dependency; runner LLVM 20.1.8 requires Python 3.10 and still fails shutdown. Source/protocol inputs cannot select tools, runtime executables or debugger commands.

Explicit native launch takes the same root/manifest selection plus an absolute `sdk` directory. Compile the captured admitted Copy program into a private native package, validate its existing package manifest/SDK snapshots and exact Rust release/commit/host, then debug-link a private executable and dSYM/PDB. Retain code/object/sidecar owners until the debugger exits. This is temporary host state using existing formats, not a new persistent artifact schema. Ordinary link-native publication and reproducible outputs remain unchanged.

Initial native breakpoints select source lines; non-default column requests refuse. Native stack columns remain unavailable at the negotiated base rather than claiming unverified column metadata. Source/line identity must match captured input before publication. Keep the original debugger frame order, with source locations omitted for physical frames outside admission.

Actual source breakpoints/callers/stepping/normal exit succeed with Xcode lldb-dap on macOS and the pinned LLVM 23.1.4 MSI on Windows (D03R source 99a20de, workflow 40907199433, 3/3). Production native launch also passes both-host workflow 40918693919 at source 39473b1 (3/3). Forward only admitted source breakpoints/threads/stacks/stepping/continue/disconnect; no shell/debugger commands, arbitrary attach or memory writes. Source responses always use captured bytes; physical native frames do not publish source locations outside admission. Initial native scopes/variables/evaluate are unavailable, with a capability update disabling hover evaluation on native launch.

Accept this elaboration under RFC 0001 experimental delegation because it reuses existing validated snapshots and SDK/ABI boundaries while making private debug-file ownership explicit. D03 production native launch is verified; live mixed-stack correspondence remains a D04 obligation. The official LLVM release was published 2026-10-20; its MSI size/hash and actual CI execution were verified 2026-10-30 JST.

### Trusted mixed-host elaboration (2026-10-30 JST)

Expose an additive trusted Rust host configuration for the embedded DAP server, mapping explicit import names to the existing `extern "C" fn(i64) -> i64` bindings. A configured server may admit `mode: mixed`; the ordinary CLI/default server still grants no imports. Source/protocol input cannot register callbacks, load a native library or select Rust code. Initial mixed support observes a cooperative native Rust callback within the original Copy VM invocation; arbitrary native instruction stepping, optimized-variable recovery and broader owned/task profiles remain separate capabilities.

Internally stop the retained VM immediately before a verified HostCall, capture its original source frames, and resume that same Machine into the explicitly granted callback. The existing interpreter retains this HostCall PC until the callback returns. A scoped host wrapper captures a bounded physical baseline before entering the real callback. A host-only pause hook captures the live physical PCs and finds their exact shared suffix with that baseline. The active owner-thread scope/target epoch establishes invocation identity; PC correspondence establishes the physical boundary. Never infer a boundary from symbol names or combine another invocation's trace. Publish the native physical prefix followed by the original logical VM frames, omitting native Rust source locations outside package admission. Require complete physical captures of at most 256 frames and at most 256 combined displayed frames; ambiguous/truncated correspondence refuses without a partial successful stack.

The callback and original Machine remain on their owning worker. Only bounded owned frame/command data crosses channels. Pause references expire on resume; the scoped call receipt expires on callback return, termination or failure. RAII restores prior thread-local association. Initial callback code must remain alive for the embedded server lifetime; unloading is unsupported. Continue resumes the same native call without replay or counter reset. Disconnect/cancel uses the original interruption flag and releases the blocked cooperative host pause; it cannot preempt arbitrary trusted callback code. Native/mixed-pause value inspection is unavailable while the Machine is borrowed by that callback; ordinary paused VM inspection remains unchanged. Capture/symbol labels consume existing index/work/reference/output limits. Symbol labels are observations only, with an address fallback; no symbol text grants source or execution authority.

The executable experiment `target/m4-d04-combined-probe.log`, verified 2026-10-30 JST, observes six native physical PCs above the boundary, original `child -> main` spans, an exact ten-frame shared physical suffix, result 42 and one host effect. Resumed and ordinary usage both equal fuel 7, host calls 1, peak frames 2 and live cells 6. This is macOS prototype evidence, not production DAP/Windows acceptance. The safe trace/PC API in [backtrace 0.3.77](https://github.com/rust-lang/backtrace-rs/releases/tag/backtrace-v0.3.77) avoids custom unsafe platform unwinding; published 2026-10-27. Its [Frame/trace implementation](https://github.com/rust-lang/backtrace-rs/blob/backtrace-v0.3.77/src/backtrace/mod.rs) provides no embedded publication/update date; verified 2026-10-30 JST. No portable/source/C ABI schema changes are introduced.

Accepted under RFC 0001 experimental delegation: reuse original host grants, Machine control order and source spans, with explicit live-scope provenance and finite observations. Physical capture/symbolization adds host work and binary dependencies only for this tooling path; no performance improvement is claimed. Windows PC correspondence, bounded embedded-server integration, stale receipts and failure cleanup remain unverified acceptance conditions. Expose D04B (2 pt) for the granted-host bridge and first mixed DAP stop; retain D04 (3 pt) for both-host real protocol/frame/failure acceptance, then D05 lifecycle audit. D04B is implemented at 67f62fa and D04 is verified at 759bae0 in all three native-debug CI environments (40933947573); the remaining obligation is D05 target lifecycle/epoch validation.

### Lifecycle elaboration (2026-10-30 JST)

Restart releases and joins the original target before capturing a fresh launch, then emits initialized for fresh breakpoint configuration. It reuses the last explicit launch configuration unless replacement launch arguments are supplied; it never expands root/tool authority implicitly. Failure leaves no new target running. Terminate releases the current target through its original interruption/shutdown path while keeping the adapter available for restart. Trusted arbitrary native callbacks remain cooperative, as above. Publish terminated once per target, including disconnect after normal completion.

Reserve a disjoint 65,536-reference block for each launch attempt, including failed attempts, within positive DAP int32 identities. Translate admitted thread/source/frame/variable IDs at the endpoint; old target IDs must refuse before accessing a replacement target. Internal per-target quotas remain unchanged. Refuse exhausted identity or message-sequence space before wraparound; no identity reuse or counter reset is allowed within an adapter lifetime. Restart performs a new captured execution, never a continuation of the old Machine.

The official cancel request concerns an outstanding request or progress sequence, not stopping the debuggee. This synchronous endpoint has no outstanding request when it dispatches cancel and emits no progress sequences: validate finite identifiers and acknowledge a best-effort no-op without interrupting execution. Terminate/disconnect supplies execution cancellation. Preparation is synchronous and cannot be cancelled through a queued request. Advertise these limits explicitly; do not mislabel a completed launch cancellation as an execution stop.

Accepted under RFC 0001 experimental delegation: preserve existing target ownership and finite quotas while preventing stale-reference aliasing across restarts. The independent D04 client at `759bae0` passes Windows/macOS workflow `40933947573`; D05 lifecycle and malformed-input evidence remain unverified. The official DAP schema was freshly verified 2026-10-30 JST; it publishes no embedded update date. No stored schema, source semantics or C ABI changes are introduced.

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

Accepted under RFC 0001's experimental delegation, 2026-10-29 JST; the bounded D01–D05 profile is implemented and verified 2026-10-30 JST at `cf5e27f`. Local fmt/full Clippy/169 suites and 2,477 tests/docs/fuzz Clippy, full CI `40937041512` (18 success, intentional documentation-only skip) and native-debug `40937041276` (3/3) pass. Actual native restart, mixed frame correspondence, one-call continuation, owner cleanup, stale identities and malformed transport are verified on both hosts. G7 is closed. General native instruction pauses, native value recovery, owned/task debugger sessions, JIT registration and interrupting synchronous preparation or arbitrary trusted callbacks remain outside this implemented profile; no stabilization or performance improvement is claimed.

## Sources

- [Official DAP overview](https://microsoft.github.io/debug-adapter-protocol/overview) and [specification](https://microsoft.github.io/debug-adapter-protocol/specification): publication/update dates unavailable; freshly verified 2026-10-29 JST. Framing, message envelopes, initialization/configuration and source/stack requests are protocol facts; initial profiles/limits are project decisions.
- RFC 0006 (updated 2026-10-28), RFC 0012 (2026-10-28), RFC 0013 (2026-10-24), RFC 0014 (2026-10-28), RFC 0020 (2026-10-29), architecture/roadmap and current VM execution loop; inspected 2026-10-29 JST. Code publishes no embedded update date.
- Ignored `target/m4-debug-vm-probe.log`, verified 2026-10-29 JST: fuel 0 observes main bytes 50..58; fuel 1 observes answer bytes 24..26 plus its parent call; fresh execution returns 42 with fuel 4. This is finite pre-design evidence, not a retained-session or native debugger result.
