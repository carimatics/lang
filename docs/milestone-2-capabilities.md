<!-- canonical-language: en; japanese-translation: ja/milestone-2-capabilities.md -->

# Milestone 2 capability guide

[日本語版](ja/milestone-2-capabilities.md) · [Delivery board](milestone-2.md) · [Original exit gates](roadmap.md#milestone-2-ownership-and-native-aot)

Updated and verified: **2026-10-20 JST**. The 18 original M2 exit gates have execution/design evidence. Z01 publishes this final guide and audit. The implementation remains experimental; accepted or implemented RFCs are not stabilized contracts. Historical **113/113 gate weight** describes acceptance coverage, not hours, productivity or an accurate effort estimate.

**2026-10-20 review correction (resolved):** The old G18 evidence checked only physical owned callers and did not satisfy RFC 0012's logical guest-chain requirement. Read-only descriptors in prepared frames now expose that chain. [Both-host run 39427222132](https://github.com/carimatics/lang/actions/runs/39427222132) at exact head 73dca1d verifies three logical frames and parent call sites. RFC 0012 is implemented; the original 18-gate completion evidence is now supplemented.

## Run a complete source program

Use the pinned Rust **1.99.0** toolchain. The [combined program](../examples/conformance/owned_pipeline.lang) composes owner transfer, collections, shared/weak values, arenas, recursion and cleanup:

```bash
cargo run -- check examples/conformance/owned_pipeline.lang
cargo run -- run examples/conformance/owned_pipeline.lang
cargo run -- compile --output target/m2-owned.lbc examples/conformance/owned_pipeline.lang
cargo run -- run --artifact target/m2-owned.lbc
cargo run -- repl
```

Both executions print **42** and exit 0. Program results are printed values, not process exit codes; compile/runtime failures exit 1 and usage errors exit 2. Choose a fresh artifact path. Definitions persist in the REPL; guest storage is retired after each evaluation. The [M1 guide](milestone-1.md) covers modules, syntax, diagnostics, formatting and the stdio LSP.

## Available behavior

| Area | Implemented M2 behavior |
| --- | --- |
| Ownership | Owned parameters/results and moves; projected reads/replacement; independent HIR/MIR move, loan, initialization and region validation; recursive guest calls |
| Failure and cleanup | Deterministic normal/return/error/panic cleanup; protected return values; explicit `core::drop` and signed `core::panic`; source spans, guest stacks and cleanup reports |
| Core values | Unit, finite Option/Result specializations, Bytes/Text with fallible allocation, checked byte access, UTF-8 conversion, cloning and exact equality |
| Collections | Allocator-aware Vec and Map, owning/nested payloads, checked reads/mutation, move/failure cleanup; Map keys are i64 or exact UTF-8-byte Text |
| Shared/weak | Explicit retain/release, reads, replacement, weak upgrades/expiry, strong-cycle refusal and rollback through nested containers |
| Lexical arenas | Branded allocation/read/replacement, same-region cycles, escape/cross-region rejection, generation checks and deterministic teardown |
| VM and REPL | Portable owned execution, bounded service work and result previews for current core/indirect values; CLI grants allocator authority and no host imports |
| Native code | Full accepted Copy profile and owned source bodies in relocatable AOT objects on macOS AArch64 and Windows x86-64; production managed runtime, persisted records and source-free startup |
| C boundary | Explicit unsafe imports/exports, finite Copy aggregates, generated C11 headers, status/out-parameter transfers, opaque contexts, actual allocator/import grants and real C callers |
| Distribution | Matching experimental SDK assembly, bounded package verification, public `link-native`, isolated producer/consumer installation and source-free Copy/owned/C execution |
| Debugging | DWARF/CodeView source locations; actual LLDB/CDB breakpoint, stepping, physical backtraces and owned logical guest chains on both native hosts |

The accepted language uses finite built-in specializations; this does not introduce general user generics, traits, closures, concurrency or arbitrary C ABI layouts. Ordinary loans, allocator authority and arena owners remain unstorable. Source checking and decoded artifact validation enforce their respective trust boundaries.

## Build a native executable

On macOS use the native linker/C compiler; on Windows use the x86-64 MSVC developer environment with a compatible C11 compiler. Python 3 and the exact recorded Rust toolchain are required.

```bash
python3 tools/build-native-sdk.py --output target/m2-sdk
cargo run -- native --output target/m2-program examples/conformance/owned_pipeline.lang
cargo run -- link-native --sdk target/m2-sdk --package target/m2-program --output target/m2-application
./target/m2-application
```

On Windows, use `python` when appropriate, select `--output target/m2-application.exe` and run `.\target\m2-application.exe`. All output destinations must be fresh. Default startup prints 42. SDK assembly uses Cargo; installed linking uses bundled artifacts and the exact rustc release/commit/host without Cargo or repository lookup. After linking, the executable needs neither original source nor SDK/program directories; system libraries remain external dependencies.

C libraries require an explicit trusted Rust host and separately compiled C objects rather than a generated main. Use [SDK instructions and startup templates](native-sdk.md), [C source examples](../examples/analysis/c_owned_exports.lang) and [real-C integration tests](../tests/c_abi.rs). Default CLI VM execution refuses C units before effects. Decoding or hashing metadata grants no C capability and authenticates no native code.

## Resource and embedding contracts

Owned execution uses the caller's original allocator, cleanup Instance and explicit controls. The ordinary native entry defaults to **64 frames/65,536 cells**; preparation/storage and fuel/service-work policies are separate configurable bounds. Failure reports retain usage and original source context. Clean failures permit retry; a host panic poisons the original Instance. Guest storage is retired before publishing a result.

Native Copy uses a trusted synchronous, **unmetered** ABI and refuses configured execution controls. Native callbacks are non-preemptible. Rust/C hosts retain the original code image, libraries, callbacks and provider through all calls/context teardown; scoped C clients must not escape. This is trusted native embedding. See [ordinary entry](../crates/codegen-clif/src/owned/runtime.rs), [scoped SDK runtime](../crates/codegen-clif/src/sdk/runtime.rs) and [RFC 0020](rfcs/0020-native-sdk-and-startup.md).

Portable Copy uses schema 1; ordinary ownership uses schema 2, explicit-panic ownership schema 3 and C-aware ownership schema 4. Native records preserve original object bytes/target and are paired with independently checked semantics. Mismatched build/target/toolchain or unsupported formats require rebuilding with a matching SDK; Rust ABI stability across builds is not promised.

## Debug and performance evidence

[Both-host run 39427222132](https://github.com/carimatics/lang/actions/runs/39427222132), exact product `73dca1d`, passes actual execution against the production SDK. macOS LLDB evidence was recorded at **13:04:10 UTC**, Windows CDB at **13:05:08 UTC** on 2026-10-20:

- Copy: source breakpoint at line 2, source step to line 3 and caller lines 2/6/9.
- Owned: breakpoint at line 8, advancing native PC, an actual Rust runtime caller, plus main→debug_outer→debug_take logical frames (FunctionId 3/2/1) and parent call sites (lines 15/12) read from target memory.
- Both profiles: ordinary and debugger execution return 42 and exit normally.

The [probe](../tools/check-native-debugger.py) preserves source and manually links with debug information using the matching SDK/rustc. Default `link-native` does not publish these debug sidecars. Read-only logical records are reachable through the driver local debug_view. Parents retain call sites; the innermost record retains the latest charged instruction span, with the physical PC supplying the current source location. Source-variable locations, full logical continuation stepping, mixed VM/native stacks and DAP remain later tooling work. This does not weaken runtime guest error stacks.

[Performance captures and reproduction](benchmarks.md#milestone-2-sourcerustc-harness) compare three representative source programs with equivalent optimized Rust/C on each host. The macOS capture is from `ecc0fd1`; Windows from `dced632`. Guarded source execution is hundreds to thousands of times slower in these small workloads; parity is not achieved. Measurements include whole-process memory and file sizes with the documented scope. They are immutable earlier-revision observations, not measurements of `73dca1d` or this guide. No scaling, power-state control or general performance claim follows from them.

## Audit of every original exit gate

The [board](milestone-2.md) retains per-delivery commit evidence and estimate history. This audit preserves all original acceptance boundaries.

| Gate | Required outcome | Implementation and verification evidence |
| --- | --- | --- |
| G01 | Accepted source ownership/core profile | [RFC 0007](rfcs/0007-source-ownership-and-core-values.md), [RFC 0009](rfcs/0009-whole-sum-ownership-extraction.md) |
| G02 | Accepted cleanup/allocator/unsafe/FFI contract | [RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md), [explicit panic](rfcs/0014-explicit-source-panic.md) |
| G03 | Typed ownership, moves and lexical loans | [MIR moves](../crates/mir/tests/ownership_moves.rs), [loan analysis](../crates/mir/tests/ownership_loans.rs), [source conformance](../tests/source_conformance.rs) |
| G04 | Shared/weak retention, cycle preflight/publication and rollback/races | [Shared engine](../crates/runtime/tests/shared.rs), [typed storage](../crates/runtime/tests/shared_storage.rs), [managed graph](../crates/runtime/tests/managed_shared.rs), [native owner boundary](../tests/native_owned_boundary.rs) |
| G05 | Arenas, escape/stale checks and teardown | [Independent region analysis](../tests/mir_regions.rs), [call summaries](../tests/mir_region_calls.rs), [arena execution](../tests/arena_calls.rs), [managed arenas](../crates/runtime/tests/managed_arena.rs) |
| G06 | Deterministic normal/error/panic drops | [Source cleanup](../tests/ownership_cleanup.rs), [failure CFG](../tests/failure_flow.rs), [production native entry](../tests/support/native_entry_runtime.rs) |
| G07 | Text/Bytes, UTF-8, checked access and allocator preservation | [Buffers](../crates/runtime/tests/buffer.rs), [cleanup](../tests/buffer_cleanup.rs), [owned execution](../tests/owned_execution.rs), [native boundary](../tests/native_owned_boundary.rs) |
| G08 | Unit/Option/Result and required finite specializations | [Core MIR](../crates/mir/tests/ownership_core.rs), [source/VM conformance](../tests/source_conformance.rs), [C aggregate cases](../tests/c_abi.rs) |
| G09 | Allocator-aware Vec with move/loan/growth/failure conformance | [Vector engine](../crates/runtime/tests/vector.rs), [owned source](../examples/scripts/vec_owned.lang), [combined production entry](../tests/support/native_entry_runtime.rs) |
| G10 | Allocator-aware Map with exact keys/replacement/failure conformance | [Map engine](../crates/runtime/tests/map.rs), [owned source](../examples/scripts/map_owned.lang), [combined program](../examples/conformance/owned_pipeline.lang) |
| G11 | Narrow unsafe boundaries and FFI validity | [C boundary checks](../tests/c_abi.rs), [context contract](rfcs/0008-cleanup-allocators-and-c-boundaries.md), [scoped runtime](../crates/codegen-clif/src/sdk/runtime.rs) |
| G12 | C import/export and generated headers | [Real-C tests](../tests/c_abi.rs), [production exports](../tests/support/c_exports_runtime.rs), [persisted objects](../tests/support/c_object_records.rs) |
| G13 | Full accepted Copy native profile | [Copy conformance](../tests/source_conformance.rs), [native program](../tests/native_program.rs), [linked compiler tests](../tests/native_compiler.rs) |
| G14 | Owned source objects and both-host native link/run | [Native owner boundary](../tests/native_owned_boundary.rs), [source-free AOT](../tests/native_compiler.rs), both-host Q02 evidence below |
| G15 | Linker/startup/runtime packaging and production errors | [Package validation](../tests/native_packages.rs), [link refusal/atomicity](../tests/native_link.rs), [SDK assembly](../tests/native_sdk.rs), isolated installation below |
| G16 | Representative VM/native differential suite | [Combined fixture](../examples/conformance/owned_pipeline.lang), [production-entry comparisons](../tests/support/native_entry_runtime.rs), [real-C comparisons](../tests/support/c_exports_runtime.rs), both-host Q02 |
| G17 | Measure native workloads against equivalent Rust/C | [B02/B03 captures and B04 audit](benchmarks.md#milestone-2-sourcerustc-harness), with the limitations above |
| G18 | Emit and validate both-host source-span debug metadata | [Whole-source PDB regression](../tests/native_debug_windows.rs), [linked DWARF regression](../tests/native_compiler.rs), [actual LLDB/CDB run](https://github.com/carimatics/lang/actions/runs/39427222132) |

Cross-host records, all verified 2026-10-20 JST:

| Evidence | Exact scope |
| --- | --- |
| [Isolated install 39370391958](https://github.com/carimatics/lang/actions/runs/39370391958), `c54af31` | Separate producer/consumer jobs on both hosts; no checkout/Cargo/repository inputs; Copy/owned/real-C execution after private inputs are deleted |
| [Q02 macOS 39376049840](https://github.com/carimatics/lang/actions/runs/39376049840), `1d0c0dc`; [Windows 39373854894](https://github.com/carimatics/lang/actions/runs/39373854894), `3db077f` | Required native-host conformance checks; implementation/test/fixture/Cargo/toolchain Git blobs match between these two heads |
| [Windows full baseline 39398395224](https://github.com/carimatics/lang/actions/runs/39398395224), `2297a07` | fmt, C/linker, Clippy, full tests and rustdoc before the later unwind repair; not a claim of a full Windows baseline at `73dca1d` |
| [Current focused native run 39427222132](https://github.com/carimatics/lang/actions/runs/39427222132), `73dca1d` | Fresh SDKs, Windows metadata/unwind reader checks, macOS linked address regression and actual both-host debugger execution |
| Local `73dca1d` baseline | 1,593 passing tests/95 suites, 0 failures, 1 manual timing ignore; fmt, warnings-denied Clippy, rustdoc and all 11 fuzz-target compile/lint checks |

These are tests and recorded runs, not a coverage-guided fuzz campaign, formal safety proof, stable distribution or performance guarantee. Compiler/runtime behavior is unchanged by this final documentation delivery; its repository-consistency validation is recorded on the board.

## Sources and dates

Project decisions: RFC 0007 last updated **2026-10-18**, RFC 0008 **2026-10-19**, RFC 0020 **2026-10-20**; other linked implementation/tests and the board were checked **2026-10-20 JST** at the named revisions. Benchmark captures retain their own timestamps and revisions. No new external research is used for this guide.

The Windows repair follows [Microsoft x64 exception/unwind handling](https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64?view=msvc-170), last updated **2026-05-21**, verified **2026-10-20 JST**. Source interpretation, project decisions and actual execution evidence are distinguished above. Design changes after acceptance require a separate amendment/change task.

