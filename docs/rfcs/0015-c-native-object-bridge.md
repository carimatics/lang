<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0015-c-native-object-bridge.md -->

# RFC 0015: Private C native object bridge

[日本語版](../ja/rfcs/0015-c-native-object-bridge.md)

- Status: accepted
- Authors: Codex, under the owner's experimental delegation
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-F05–F07, M2-P02
- Supersedes: None; extends RFC 0012 for explicitly C-aware objects
- Superseded by: None

## Summary

Emit explicitly C-aware native objects with profile-1 support/export entries, ownership continuations and exact typed import thunks. Bind a retained linked image to the same checked Program/Context/runtime used by JIT execution. Public entries obtain their runtime through a versioned private per-image slot; no host address is embedded in the emitted bytes.

Decision: **accepted under experimental delegation, 2026-10-20 JST**. The interface is bounded, preserves accepted semantics and permits actual AOT execution without runtime JIT. Matching-image binding remains a trusted unsafe boundary. Acceptance does not establish implementation completion, platform conformance or performance.

## Motivation

At 61af46d, real C executes native record/owner/callback/panic exports through NativeFactory. The ordinary object API still refuses complete C units. ContextSupport/NativeExports capture process addresses; NativeInvoker constructs JIT signature thunks. Copying those paths into object emission would leave an address-dependent object or a runtime JIT requirement.

The current in-memory ObjectArtifact already pairs emitted code with matching compiler metadata and a retained trusted image. Extend that explicit boundary rather than silently accepting C units through the ordinary API.

## Goals

- Execute linked C exports and C imports through actual object machine code and the existing ownership gateway.
- Preserve spans, limits, cleanup/first-failure precedence, allocator identity, grants and untouched output on failure.
- Give private native object binding and namespaces an explicit version.

## Non-goals

- New syntax, source semantics, C profile-1 bytes/digests/statuses, stable native/plugin ABI, or a second frontend.
- A new portable MIR/bytecode/package encoding, untrusted native loading, async entry or cross-thread context use.
- Completing standalone package startup, debugger evidence or performance claims through this decision alone.

## Guide-level design

The explicit C object emitter returns bytes plus matching checked metadata. A trusted host links those bytes, resolves their declared symbols and retains the image through a checked linked-code owner. A binding guard associates that owner, its Program and an ExportRuntime factory with the object's private slot. C then calls the existing generated header's create/export/query/destroy functions.

Keep the binding, Program, metadata, linked dependencies, allocator callbacks and callback user data alive through all calls and context destruction. Binding/unbinding and calls run synchronously on the owner thread. One image slot admits at most one active binding. A second bind refuses without replacing it. Unbind requires no active call and no live opaque context; clear the slot before releasing the header/dispatch. This explicit embedding API is not a general C loader API.

## Reference semantics

### Object bridge revision 1

Tier-1 targets have 64-bit pointers. Export one writable, zero-initialized, eight-byte-aligned private slot named `lang_c_obj1_slot_<artifact-digest>`. It holds a pointer to a live immutable native header. Header size/alignment is **48/8**:

| Offset | Field | Native representation |
| ---: | --- | --- |
| 0 | version | u32, exactly 1 |
| 4 | size | u32, exactly 48 |
| 8 | data | nonnull opaque dispatch pointer |
| 16 | create | target-C u32 fn(data, config, context_out) |
| 24 | destroy | target-C u32 fn(data, context) |
| 32 | query | target-C u32 fn(data, context, summary_out) |
| 40 | invoke | target-C u32 fn(data, function_index_u64, context, argument_words, output) |

All pointer fields are eight bytes. This is a toolchain-private native contract, not serialized Rust layout. The unsafe binder establishes complete actual header/slot extent, alignment, access, code types and lifetime. Entries first load the slot and check version/size, nonnull data and the selected callback. An unbound/mismatched/null-callback entry returns profile-1 invalid status without reading argument objects, invoking providers or changing caller outputs.

Source exports retain exact profile-1 signatures. Their wrappers marshal at most 256 initialized u64 argument words on a bounded aligned stack reservation; bool stays its original u8 bits, other scalars/pointers keep their native representation. Unit results have no output pointer. The callback authenticates the original opaque context/Program and executes the existing complete Export gate before managed transfer or guest execution.

Create/destroy/query delegate the existing checked raw context support. Invoke uses the linked code owner, original accounting allocator, control/work and cleanup Instance. Managed owners never cross the C value boundary. Ordinary guest panic permits reuse; cleanup/host panic poisons; first failure and source spans survive later cleanup. No syntax, name resolution, inference, evaluation order, loans, arena brands, constant evaluation or memory-model rule changes.

### Native bodies and import thunks

C object body symbols are `lang_c_obj1_r<revision>_<artifact-digest>_f<function-index>`. Select revision 3 for whole units containing Panic, otherwise revision 2. Preserve RFC 0012/0014 packets, statuses and helper indices/signatures. Ordinary objects and current JIT namespaces keep their existing names and complete ordinary C-unit refusal.

Emit an object-resident exact-signature thunk for every declared C import, with private names `lang_c_obj1_t<artifact-function-index>_<artifact-digest>`. It restores the generated callback type from the granted code pointer, including bool/Unit/aggregate and 256-parameter profiles. The linked invoker uses these retained addresses and the existing ordinary-padding restoration/validation; it constructs no JIT module or executable code at runtime, including zero-import units.

The linked runtime reads the same verified layouts/call sites as the JIT runtime. Metadata access alone cannot select a substitute guest evaluator. Compiled continuations execute all guest arithmetic/control and call/return paths.

## Execution modes

VM semantics remain the comparison reference. Explicit JIT embedding retains its existing interface and symbols. C-aware AOT uses emitted object bodies, support/export wrappers and import thunks with a matching trusted runtime; it never substitutes VM execution or runtime JIT. Wasm/component mappings are unchanged and deferred.

## ABI and compatibility

Affected domains: `native-abi` and `bootstrap`. Rebuild C-aware objects and their matching runtime for object bridge revision 1. Symbol names and header checks refuse another bridge revision instead of guessing a layout. The ordinary object API still refuses C units; use the explicit C object API. C headers, profile-1 descriptors/digests/statuses and all accepted source behavior remain unchanged.

Portable schemas 1/2/3 keep their C-metadata refusal. The initial linked API retains matching in-memory ObjectArtifact metadata, with no independent native metadata wire decoder. Any later persisted native metadata/bootstrap representation needs its own recorded version and validation contract before implementation. Package startup remains P02; this does not relabel in-memory compiler metadata as a standalone package.

## Diagnostics and tooling

Use structured target/profile/bridge/symbol/already-bound/resource errors before publication. Preserve original function/site spans in linked metadata and runtime failures. No parser, formatter, linter, LSP or diagnostic-code change is required. Native debug location/stepping evidence remains G01–G03.

## Performance and cost model

Each public entry adds a slot load, version/size/null checks and an indirect runtime call; export arguments use bounded word packing. Typed thunks and wrappers increase object size. Host metadata preparation and guest allocation/cleanup follow existing bounds and accounting. No compile-time, runtime, size or memory advantage is claimed; target measurements remain unverified.

## Security and trust boundaries

Native images, matching-image resolvers and runtime adapters are trusted. Verify target, artifact identity, required symbols, body/import/export mappings, header revision/size and binding state before use/publication. Do not decode arbitrary Rust memory layouts or accept attacker-supplied function pointers as a sandbox. Script/plugin capabilities, explicit native-FFI grants, reentry/poison rules and actual C object validity obligations remain unchanged. All foreign callbacks retain synchronous no-unwind/code/user lifetime obligations.

## Alternatives

- **No change:** keeps AOT C execution missing.
- **Generated C shim/tool library:** can marshal signatures but still needs a versioned binding/runtime contract and matching code metadata; direct object wrappers reuse the backend emitter.
- **Process-global registry (strongest competing design):** resolves an artifact identifier per entry, but introduces global lookup, registration and lifetime coordination. A bounded per-image slot makes the retained owner explicit.
- **Captured addresses/runtime JIT (simpler):** retains current embedding behavior but does not produce a relocatable AOT C path.

## Implementation plan

1. Share checked C metadata/layout access and retain actual linked code ownership; keep ordinary APIs unchanged.
2. Emit revision-1 slot, support/export wrappers and typed import thunks beside ownership machine bodies.
3. Bind a linked image/runtime and run real C source increment/panic, then aggregate/owner/import scenarios through the same path.
4. Audit complete F05 scope, direct/response-file linking, malformed/unbound/lifetime/state/resource cases and both targets. Keep F06/F07/P02 and the final guide explicit.

## Test plan

Keep existing source compile-pass/fail, C validity, schema refusal, JIT and ordinary-object regression fixtures. Add object inspection, missing/duplicate/wrong-target binding, unbound/revision/size/null callback untouched-output tests, actual C header compilation/link/run, original-context callbacks, aggregate padding/canonical cases, owner recursion/panic cleanup and fuel/provider prefixes. Compare equivalent VM/native behavior. Prove import addresses come from the linked image, not a JIT fallback. macOS/Windows and system-link direct/response-file evidence are distinct requirements; benchmark/debug gates stay open.

## Open questions

No blocking semantic question. Unverified implementation assumptions: Cranelift slot relocations/symbol visibility on both formats, sharing metadata/runtime lifetimes without a self-reference, complete linked thunk reuse, package bootstrap and actual target cost. Prototype these and revise this RFC explicitly before an incompatible change.

## Sources

- RFC 0001 experimental delegation, last updated 2026-10-17; verified 2026-10-20 JST.
- RFC 0008 C boundary contract, last updated 2026-10-19; verified 2026-10-20 JST.
- RFC 0012 private native continuations (amended alongside this RFC), last updated 2026-10-20; verified 2026-10-20 JST.
- RFC 0014 explicit panic/r3 contract, last updated 2026-10-19; verified 2026-10-20 JST.
- Local implementation version `61af46d`, committed 2026-10-20 JST: `owned.rs` ordinary object refusal, `c_abi/scalar.rs` JIT thunks, `c_abi/exports.rs` address capture, `c_abi/runtime/factory.rs` runtime; verified 2026-10-20 JST. The existing source-derived object-boundary experiment still refuses both tier-1 targets before effects.
