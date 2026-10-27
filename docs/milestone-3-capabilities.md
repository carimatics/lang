<!-- canonical-language: en; japanese-translation: ja/milestone-3-capabilities.md -->

# Milestone 3: concurrency and embedding

[日本語版](ja/milestone-3-capabilities.md) · [Completed delivery board](milestone-3.md) · [Roadmap](roadmap.md#milestone-3-concurrency-and-embedding)

Verified: **2026-10-27 JST**, executable source **`9e78f2d`**. All **15 exit gates / 96 gate points** pass at the admitted scope. The [exit record](evidence/2026-10-27-m3-exit-9e78f2d.json) maps every gate to its evidence. M2's completed record and historical measurements remain unchanged.

A native host can load a verified user script, supply a typed native API, call the script, receive nested callbacks and enforce explicit capabilities without IPC. Structured tasks, channels and isolated services run through the VM, generated native execution and admitted separately linked objects. The public component C v1 boundary is stable; source syntax and private Rust/SDK linkage remain experimental.

## Try the implemented workflows

Use the repository's pinned Rust toolchain (`rust-toolchain.toml`) and supported native tools. The tier-1 targets are macOS AArch64 and Windows x86-64 MSVC. Run these commands from the repository root:

```sh
cargo run -- run examples/scripts/tasks.lang
cargo run -- run tests/fixtures/task_owned_buffers.lang
cargo run -- run tests/fixtures/task_channels.lang
cargo run -- run tests/fixtures/task_service_state_replace.lang
cargo run -- run tests/fixtures/task_owned_transfer_all.lang
```

The respective results are **42, 71, 42, 42, 42**. These exercise spawn/join, owned buffers across suspension, bounded channels, owned service-state replacement and owning async arguments/results. Source and independent MIR checks reject moved claims, forbidden suspension loans, non-Send payloads and invalid service authority.

The same channel fixture executes generated native code:

```sh
cargo run --example native-structured-tasks -- tests/fixtures/task_channels.lang
```

It returns **42**. Native differential/linked-object tests compare values, events, failure origins, ownership and cleanup; matching one result alone is not the conformance claim.

Stored tasks execute after independent artifact validation:

```sh
cargo run -- compile --output target/m3-tasks.lbc examples/scripts/tasks.lang
cargo run -- run --artifact target/m3-tasks.lbc
```

The result is **42**. LANGBC schema 5 retains typed task/channel/service/component facts, source spans and independently checked ownership/effect/loan proofs. Unknown or incompatible versions refuse before execution. Component artifacts require their actual checked bindings and grants; they are not ordinary script-entry artifacts.

## Structured ownership and isolation

Tasks are scoped claims, not detached work. Descendants join before their parent's local owners retire. Cancellation is cooperative, with deterministic postorder cleanup; failed cleanup preserves the primary failure and ordered suppressed evidence. Unclaimed owning results stay in escrow until transfer or mandatory retirement.

Channels bound queue/backpressure and preserve ownership on a refused or cancelled uncommitted send. FIFO wakeups use actual scoped wait authority. Services keep one isolated state owner, distinguish producer authority from receiver/state authority and retire queued messages before state. Recursive Send/Share checks are independent in source and MIR. This profile does not execute guest tasks on multiple OS threads.

## Load a script with a typed native plugin

```sh
cargo run --example component-plugin-host -- vm
cargo run --example component-plugin-host -- jit
```

Both modes execute the ordinary-source script and the retained native increment provider through the public C API. They assert script result **42**, denial of a missing typed grant before a native call, panic, clean reuse with result **1**, fuel refusal, exactly-once native-owner release and **zero final managed allocations**. See [the plugin guide and source/IDL](../examples/plugins/README.md).

A provider's presence does not grant authority. The host supplies checked contract projections and explicit typed grants; missing/mismatched grants, incompatible signatures and malformed metadata refuse before provider discovery or effects. Native implementations and native images are trusted host code. Verified script execution and finite controls do not sandbox arbitrary native code.

## Stable C embedding boundary

[RFC 0033](rfcs/0033-component-v1-compatibility.md) stabilizes the admitted component C v1 contract after actual both-host cross-mode conformance and previous-client migration. The normative [header](component-abi-v1.h) and independent [layout vectors](component-abi-v1-layouts.json) define all **32 non-null operations**, **139 field offsets** and **38 signatures**. The API table is **280 bytes** on both supported targets.

Query `lang_component_v1_get_api` with version **1** and the exact table size. Request/response headers need their exact size/version and zero reserved fields. Unknown versions and malformed requests fail before publication and preserve output slots. A future incompatible extension needs a new version/namespace; it cannot append fields silently to v1.

Keep an existing compatible header/generated client when updating the public runtime on the same target. [Previous-client fixtures](../tests/fixtures/component-v1-40158d6/provenance.json) retain the first complete public source `40158d6`; tests compile its actual declarations/codecs and execute them against the current implementation. Old scalar clients exercise real VM/JIT registry/resource/task/failure paths; old counter bindings preserve exact frames and transfer-commit outcomes.

Bindings encode values, not authority. A contextual resource index must still match its actual table, generation, provider, holder and loan. Encoding or pre-commit refusal leaves input ownership unchanged; committed or unknown outcomes must not be treated as no transfer. Result bytes/reports and resource owners have separate lifetimes. Borrowed views cannot escape callback scope; original destructors retire owners exactly once, and failed cleanup poisons the original execution.

Actual generated-C tests cover source/artifact VM/JIT and exact-SDK AOT Images, nested callbacks, retained failures and original code/resource owners:

```sh
cargo test --test component_generation
cargo test --test component_embedding_inputs
cargo test --test component_native_image
```

## Installed native execution

The [SDK guide](native-sdk.md) describes assembly, explicit host bindings and `link-native`. Both-host [macOS](evidence/2026-10-27-m3-sdk-macos-8f41f44.json) and [Windows](evidence/2026-10-27-m3-sdk-windows-8f41f44.json) consumers actually start **seven profiles**: copy, owned, C, task, owned-task, component and component-gateway.

Each consumer has no checkout, verifies 12 absent repository paths, has no Cargo in PATH, uses an empty Cargo home and removes original link inputs before startup. Task entries return 42; component entries preserve 42/panic −42/reuse 1/fuel refusal and zero final inventory. Producers and consumers pass [Actions run 40456240378](https://github.com/carimatics/lang/actions/runs/40456240378). Later changes through `9e78f2d` affect tests/docs only, so this production/SDK evidence remains valid.

Stable C v1 does not stabilize private Rust/native-source ABI. AOT Images require the exact supported host/build SDK before private-pointer access, original selected code and actual grants/providers; no discovery or VM/JIT fallback supplies missing authority. Rebuild packages with matching tools when those identities or independently versioned formats differ.

| Version domain | Admitted revision |
| --- | --- |
| IDL / public C ABI | 1 / 1 |
| LCMP contract/report | 3 |
| LCVL frame / LCSG signature preimage | 1 / 1 |
| LANGBC task/component artifact | 5 |
| LANGNAT task/component record | 2 |
| Native component program / SDK | 3 / 3 |

These are separate version domains, not one shared compatibility number. Legacy copy/owned/C and task package profiles keep their recorded admission rules.

## Limits and failure behavior

One original allocator/control/preparation history spans guest work, native adapters and nested callbacks. Heap bytes/live allocations, frame/cell/task admission, fuel/host calls and service/adapter work are checked before their effects. Numeric failures retain the actual kind, bound and requested amount, source/task/caller context and ordered cleanup evidence.

For example, this command refuses with `ServiceWorkLimit { limit: 0, used: 0, requested: 1 }` and an original source span:

```sh
cargo run -- run --service-work 0 tests/fixtures/task_owned_buffers.lang
```

Cleanup still retires accepted owners after interruption or refusal. Limits cover admitted managed/accounted work; they are not process-RSS or arbitrary native execution limits. Cooperative polling cannot interrupt a blocking foreign callback. Async native callback stacks, detached tasks, a second frontend and WebAssembly component execution are outside this milestone. Async REPL submission remains unavailable; the CLI/artifact and explicit task APIs are the runnable workflows above.

## Cost observations

[The benchmark report](benchmarks.md#milestone-3-concurrency-and-component-entry-costs) and [clean release samples](../benchmarks/results/2026-10-27-m3-macos-c9ffd19.json) record six finite VM/JIT workloads and the C/script/native-callback boundary: three warmups and 21 measured samples, raw distributions, source/compiler provenance, equal logical counters and zero final managed inventory. Preparation/execution timing and per-call component JIT compilation are explicit.

These observations are macOS-only and do not claim Windows/AOT throughput or process memory. [M2 replay](../benchmarks/results/2026-10-27-m2-parity-macos-c9ffd19.json) passes 21 source/Rust/C/oracle cases plus unwind retirement/reuse; it is parity evidence and replaces no historical M2 timing. Test/CI durations are never benchmark samples.

## Complete exit audit

All rows below are complete at source `9e78f2d`; [the machine record](evidence/2026-10-27-m3-exit-9e78f2d.json) retains the mapping. No P0/P1 item was deferred or removed to obtain completion.

| Gate | Weight | Accepted outcome and executable evidence |
| --- | ---: | --- |
| G01 | 5 | Scheduler, async, Send/Share, channel/backpressure and isolation contracts: RFCs 0021–0026. |
| G02 | 5 | Production versioned IDL/wire/C, errors, reentry and cancellation: RFC 0027. |
| G03 | 8 | Owning structured tasks, join/cancellation and deterministic retirement: task integration/transfer and native task suites. |
| G04 | 5 | Bounded channels, wakeups, ownership recovery and cancellation: source/native channel suites. |
| G05 | 8 | Isolated service state and capability roles: source/native service and type-capability suites. |
| G06 | 8 | Bounded IDL/codecs/generation and independently verified stored artifacts: IDL/wire/generation/artifact suites. |
| G07 | 8 | Owned/borrowed resource provenance, transfer, loans and poison: source-resource/scoped-root/C suites. |
| G08 | 5 | Typed grants and malformed/incompatible refusal before effects: grants/selection/registration and installed packages. |
| G09 | 8 | Actual mixed VM/JIT/AOT calls/callbacks with typed failures and retained owners: C hosts/native Images/installed SDK. |
| G10 | 5 | Original mixed stacks, task/caller spans, cancellation and cleanup: scoped roots/C hosts, including LF/CRLF faults. |
| G11 | 5 | Original heap/allocation and callback accounting: foreign allocator/scoped roots/native Images. |
| G12 | 5 | Combined recursion/frame/cell/live-task admission and finite controls: task controls/scoped roots. |
| G13 | 8 | Cooperative VM/JIT/AOT/callback interruption with mandatory cleanup: native task/channel/service/Image/record suites. |
| G14 | 8 | Both-host conformance and previous C/generated-client migration: stabilized RFC 0033 and migration record. |
| G15 | 5 | Runnable trusted-native and verified-script plugin with explicit authority: plugin example and guide. |
| **Total** | **96** | **15/15 complete** |

## Validation and sources

The [migration record](evidence/2026-10-27-m3-migration-9e78f2d.json), recorded **2026-10-27**, proves **153 suites / 2,321 passed / 0 failed / 1 existing ignore** on macOS, with workspace fmt/Clippy/tests/rustdoc and all 16 fuzz-target format/build/lint checks successful; no fuzz campaign ran. Fixed before/after inputs match. [Windows CI 40471721760](https://github.com/carimatics/lang/actions/runs/40471721760), completed **2026-10-27 09:01:42 UTC**, passes every step and **153 suites / 2,306 passed / 0 failed / 1 existing ignore**, including both previous-client cases. The 15-case difference is documented platform-specific filesystem/linker/DWARF/dynamic-image coverage, with equivalent platform-specific swaps.

Primary project decisions: RFC 0021 updated **2026-10-25**; RFCs 0022–0026 **2026-10-27**; RFC 0027 **2026-10-27**; RFC 0028 **2026-10-24**; RFC 0029 **2026-10-25**; RFCs 0030–0033 **2026-10-27**. SDK, exit and cost records are dated **2026-10-27**. Test/example source files have no embedded publication/update date; their exact source and executable checks were verified **2026-10-27 JST**, rather than assigning an inferred update date.
