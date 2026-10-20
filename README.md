<!-- canonical-language: en; japanese-translation: README.ja.md -->

# lang

[日本語版](README.ja.md)

`lang` is a pre-alpha research project for a new systems programming language and its complete developer toolchain. The implementation starts in Rust and is intended to become self-hosting after the language, compiler, and bootstrap process are stable enough to reproduce themselves.

The project is currently defining semantics and validating architecture. There is no stable syntax, ABI, package format, or release schedule yet.

Explicit source panic (2026-10-19): RFC 0008's `core::panic::<T>(code: i64)` never returns and preserves the signed code, original call span, guest stack and owner cleanup across VM/native execution. See the [example](examples/analysis/explicit_panic_owned.lang) and [versioned contract](docs/rfcs/0014-explicit-source-panic.md). C-free artifact units containing explicit panic use ownership schema 3; older ownership units retain schema 2. C-aware units select schema 4, including explicit panic. This remains experimental.

C-aware artifact transport (2026-10-20): [RFC 0017](docs/rfcs/0017-c-aware-ownership-artifacts.md) adds ownership schema 4. Complete Host/C and Guest/CExport declarations survive independent decoding, including finite aggregate signatures and explicit panic. Source-derived decoded fixtures call actual C code through explicit VM/native contexts; the CLI refuses C execution with zero usage. Native distribution remains P02 work.

Native compiler records (2026-10-20): [RFC 0018](docs/rfcs/0018-native-compiler-records.md) transports original object bytes/target/symbols/source ranges, checked against the exact portable semantic artifact. The Rust `native_record::Record` API bounds decoding and retains no live address or capability. `Record::prepare_copy` and `program::PreparedObject::bind` provide retained-image Copy calls with the same canonical value checks as JIT. Ownership records reconstruct checked plans through `owned::RecordedObject::from_record`. macOS linked fixtures reload semantic/record files after producer teardown, preserving record/enum calls, managed Text service work and explicit panic; they do no source analysis or code emission at startup. Copy native execution retains its trusted synchronous, unmetered contract. The ordinary production runtime is implemented below. Persisted C startup is implemented below; SDK assembly is described below; public linking and both-host clean install remain P12/P02 work.

Ordinary owned native runtime (2026-10-20): [RFC 0019](docs/rfcs/0019-owned-native-entry-runtime.md) adds `lang_codegen_clif::owned::runtime::execute_entry` for a compiled/linked ownership program, explicit zero-argument i64 entry, cleanup Instance and original `Allocator<P>`. It shares the production managed primitives with C exports and needs no C context or test runtime. Config provides finite frame/cell bounds (default 64/65,536), preparation limits and controls. Errors retain usage, function/span/panic code, guest stack and cleanup report, with supplemental host-storage completion failure. Clean failures allow retry; host panic permanently poisons the original Instance. Ten sources cover every managed family and panic, all fuel/work/provider refusal positions, frame/cell/interrupt/deadline boundaries, persisted linked startup and release. Run `cargo test --test native_owned_boundary production_owned_entry`. Public SDK linking and isolated installation on both hosts remain P12/P02.

Raw C context support (2026-10-19): the Rust embedding API `lang_codegen_clif::c_abi::context::ContextSupport` generates the profile-1 create/destroy/last_error symbols for a registered Program. An actual C caller can create an opaque context from copied descriptors, query its integer error summary and destroy it. Keep the support, registered code, allocator and callback user data alive through all calls and context destruction. The explicit native source-export embedding entry is F15 below; C-aware object publication remains F05.

Native C source exports (verified 2026-10-19 JST): retain `lang_codegen_clif::c_abi::exports::NativeExports` and a trusted `ExportRuntime` factory alongside `ContextSupport`/`CCompiledProgram` to call source increment/panic, bool/Unit and C callbacks from actual C. Generated machine code executes the source body; callbacks receive the original opaque context. Each context retains its actual C provider and cumulative quota; failed calls leave caller results unchanged. Run `cargo test --test c_abi imports::native::exports` for execution, reentry, quota/OOM and host-panic fixtures. The factory supplies trusted typed storage/cleanup primitives. The subsequent F16 connects a reusable native runtime and aggregate/owner bodies below. Linked C object/standalone publication remains F05.

Native C runtime (implemented/verified 2026-10-20 JST): pass `lang_codegen_clif::c_abi::runtime::NativeFactory::new(&compiled, &artifact, limits)` to `NativeExports` to run typed native exports without implementing a fixture runtime. Examples cover [records/nested sums](examples/analysis/c_aggregate_exports.lang), [every core error/empty records](examples/analysis/c_copy_export_matrix.lang) and [owner recursion/C callbacks/panic](examples/analysis/c_owned_exports.lang). The full 256 mixed i64/bool/Unit/record parameter profile is supported. Generated machine code executes bodies; shared storage/loan/cleanup primitives handle Bytes/Text/Vec/Map/shared/arena and guest calls. All guest regions use the original accounting C allocator; only validated results publish after cleanup. Run `cargo test --test c_abi imports::native::exports` for real C callers, ten-source VM comparisons and every fuel/provider failure prefix. This is an explicit Rust embedding API for JIT and matching linked images; default CLI C execution is refused, while RFC 0017 schema 4 now transports C identities without granting authority. Linked C objects and both-host boundary evidence are recorded in F05–F07; persisted C startup is implemented below, while public SDK linking/isolated installation remain P12/P02 work.

Persisted C object startup (2026-10-20 JST): save `CObjectArtifact::into_recorded` as RFC 0018 data, rebuild `c_abi::Artifact` from decoded verified declarations, then use `RecordedObject::prepare_c` to restore the C interface/private bridge symbols. Bind the original object image under the existing unsafe contract and run `NativeFactory`/`ObjectExports` with the original opaque context, allocator, explicit import grants and cleanup. Startup needs no source analysis, code emission or JIT. Tests cover all managed families, aggregate/256-parameter and finite Copy cases, owner recursion/callback/panic, fuel/allocation failures, both object formats and budgets. Run `cargo test --test c_abi persisted_c_object`. SDK assembly is described below; exact-toolchain linking and both-host isolated install remain P12/P02.

## Vision

The language should combine:

- Native performance suitable for systems and application development.
- Memory safety without a mandatory tracing garbage collector.
- A Kotlin-like productive surface: null safety, algebraic data types, pattern matching, generics, traits, extension-oriented APIs, closures, and structured concurrency.
- Explicit access to pointers, allocators, SIMD, operating-system APIs, and foreign functions behind auditable unsafe boundaries.
- AOT compilation and an embeddable script runtime that can call each other in the same process.
- First-class tooling: diagnostics, formatter, linter, tests, package/build management, LSP, DAP, documentation, and toolchain installation.
- A path from one language frontend to multiple domain-focused frontends that share an interface model and compiler pipeline.

## Initial direction

The accepted architecture baseline is deliberately narrower than the full vision; unimplemented extensions still require focused RFCs:

1. Build one language first. Offer safe high-level code and explicit low-level facilities in the same semantic model instead of starting with two languages.
2. Share parsing, type checking, ownership analysis, and a typed mid-level IR between all execution modes.
3. Lower the shared MIR to a portable bytecode VM for scripts and to native object code for AOT builds.
4. Add JIT compilation only after VM semantics, debug information, and the embedding API are stable.
5. Define a versioned interface description and canonical component ABI. Use direct internal calls when layouts are known, and generated thunks/handles at independently versioned boundaries.
6. Keep trusted native plugins and capability-restricted script/WebAssembly plugins as separate deployment tiers.
7. Treat self-hosting as a reproducible bootstrap milestone, not as an early implementation constraint.

The rationale and unresolved tradeoffs are documented in [the architecture baseline](docs/architecture.md).

## Documents

- [Language and ecosystem research](docs/research.md)
- [Prototype benchmark evidence](docs/benchmarks.md)
- [Architecture baseline](docs/architecture.md)
- [Implementation roadmap](docs/roadmap.md)
- [Milestone 1 capability guide](docs/milestone-1.md)
- [Whole-repository review, 2026-10-17](docs/review-2026-10-17.md)
- [RFC process and template](docs/rfcs/0000-template.md)
- [RFC 0001: Project charter and compatibility policy](docs/rfcs/0001-project-charter.md)
- [RFC 0002: Prototype integer and structured control-flow semantics](docs/rfcs/0002-prototype-control-flow.md)
- [RFC 0003: Versioned component contracts and capability imports](docs/rfcs/0003-versioned-component-contracts.md)
- [RFC 0004: Owned values, explicit sharing, and cyclic graph storage](docs/rfcs/0004-owned-values-and-cyclic-graphs.md)
- [RFC 0005: Structured tasks, cancellation, and isolated services](docs/rfcs/0005-structured-tasks-and-isolated-services.md)
- [RFC 0006: Source frontend and portable VM programs (implemented)](docs/rfcs/0006-source-frontend-and-portable-programs.md)
- [RFC 0007: Source ownership and core values (accepted)](docs/rfcs/0007-source-ownership-and-core-values.md)
- [RFC 0008: Cleanup, allocator authority, and C boundaries (accepted)](docs/rfcs/0008-cleanup-allocators-and-c-boundaries.md)
- [RFC 0009: Whole Option/Result ownership extraction (accepted)](docs/rfcs/0009-whole-sum-ownership-extraction.md)
- [RFC 0010: Owned REPL result previews (accepted; current-profile REPL implemented)](docs/rfcs/0010-owned-repl-previews.md)
- [RFC 0016: Indirect REPL result previews (implemented)](docs/rfcs/0016-indirect-repl-previews.md)
- [RFC 0017: C-aware ownership artifacts (implemented)](docs/rfcs/0017-c-aware-ownership-artifacts.md)
- [RFC 0018: Native compiler records (implemented)](docs/rfcs/0018-native-compiler-records.md)
- [RFC 0019: Ordinary owned native entry runtime (implemented)](docs/rfcs/0019-owned-native-entry-runtime.md)
- [RFC 0020: Experimental native SDK and scoped startup (accepted)](docs/rfcs/0020-native-sdk-and-startup.md)
- [RFC 0011: Portable ownership artifacts (implemented)](docs/rfcs/0011-ownership-artifacts.md)
- [RFC 0012: Native ownership continuations and managed frames (accepted)](docs/rfcs/0012-native-ownership-continuations.md)
- [RFC 0013: Service byte-work budget (accepted; implementation pending)](docs/rfcs/0013-service-byte-work-budget.md)
- [Contribution guide](CONTRIBUTING.md)

## Development

RFC 0008 `core::drop::<T>(value) -> core::Unit` now retires owners immediately, preserves Copy originals, and rejects loan/allocator specializations (`SRC0024`). Moving a borrowed owner remains `SRC0021`; using a dropped owner remains `SRC0020`. [Bytes](examples/analysis/explicit_drop.lang) and [shared/weak](examples/analysis/explicit_drop_shared.lang) examples return 42 on VM/native. Drop lowers to existing cleanup instructions and round-trips through ownership schema 2; source `core::panic` remains F13 work. Verified 2026-10-19 JST.

The Milestone 1 source pipeline supports multi-file modules, typed functions and recursion, copy-only records/enums, exhaustive patterns, verified portable bytecode, and bounded VM execution. Source spans survive analysis, lowering, artifacts, and runtime stacks. The formatter and stdio LSP prototype use the same lossless parser and revisioned module/diagnostic database.

RFC 0008 C import/export declarations retain typed C identities through source checking and independently verified ownership MIR. Calls require lexical `unsafe`, preserving ordinary type/move/loan/cleanup rules; invalid C signatures/calls report `SRC0025`, and old names spelled `unsafe` report migration `SRC0024`. Explicit Rust context/binding APIs execute [scalar/Unit](examples/analysis/c_scalar_imports.lang) and [finite Copy aggregate imports](examples/analysis/c_aggregate_imports.lang) on VM/native. Records, enums, nested Option/Result and core errors use field-wise C transfers and the actual context allocator; generated C11 headers check callback signatures. Run `cargo test --test c_abi imports::aggregate_c_imports_execute_records_and_sums_from_source`. Raw C context factories and an explicit native source-export embedding entry are implemented (F15 above). Explicit native aggregate/owner-body exports are verified in F16 below. C object publication and complete native packaging remain unfinished. C-aware artifacts now use schema 4, preserving C identities without bindings or capabilities; decoding alone grants no C authority. Default CLI/legacy execution still reject complete C units before effects. Legacy one-i64 `extern fn` host imports keep their behavior. Verified 2026-10-20 JST.

Requirements: Rust `1.99.0` with rustfmt and Clippy, pinned by `rust-toolchain.toml`; `mise` supplies auxiliary tool versions when needed.

```bash
cargo run -- check examples/analysis/ownership.lang
cargo run -- check examples/analysis/extraction.lang
cargo run -- run examples/scripts/answer.lang
cargo run -- run examples/scripts/bytes.lang
cargo run -- run examples/scripts/owned_bytes.lang
cargo run -- compile --output target/owned-bytes.lbc examples/scripts/owned_bytes.lang
cargo run -- run --artifact target/owned-bytes.lbc
cargo run -- run examples/analysis/extraction.lang
cargo run -- run examples/scripts/utf8.lang
cargo run -- run examples/scripts/text.lang
cargo run -- run examples/scripts/owned_calls.lang
cargo run -- run examples/scripts/cleanup_calls.lang
cargo run -- run examples/scripts/owned_replace.lang
cargo run -- run examples/scripts/owned_aggregate.lang
cargo run -- run examples/scripts/arena.lang
cargo run -- run examples/scripts/arena_owned.lang
cargo run -- run examples/scripts/shared.lang
cargo run -- run examples/scripts/shared_owned.lang
cargo run -- run examples/scripts/vec.lang
cargo run -- run examples/scripts/vec_owned.lang
cargo run -- run examples/scripts/map.lang
cargo run -- run examples/scripts/map_owned.lang
cargo run -- run examples/scripts/nested_collections.lang
cargo run -- run examples/scripts/nested_cycle.lang
cargo run -- run examples/scripts/nested_arena.lang
cargo run -- run examples/scripts/nested_mutation.lang
cargo run -- repl
cargo run --quiet -- repl < examples/repl/owned.repl
cargo run --quiet -- lsp
```

Source arenas execute allocation, checked reads, replacement and lexical teardown for Copy values, Bytes/Text, inline aggregates and nested Vec/Map values with same-region references. `arena.lang` prints `9` after replacing and reading a value; `arena_owned.lang` prints `5` after following a cycle of nodes with owning Bytes payloads. Region re-entry creates fresh generations. Reads end before mutation/exit; teardown releases nodes in reverse allocation order and their fields in reverse initialization order. See [source arena execution](docs/architecture.md#source-arena-execution), updated/verified 2026-10-18 JST.

`compile --output ARTIFACT SOURCE` emits Copy schema 1 or ownership schema 2/3/4 after source/format validation (2 ordinary, 3 explicit panic, 4 C-aware). `run --artifact ARTIFACT` independently verifies and executes its explicit entry ID; embedded module snapshots preserve diagnostics even after source files are removed. The Bytes commands above print `0`. Binary input is capped at 64 MiB; output is published through a temporary file only after successful compilation. Library artifacts without an entry cannot run as scripts. The CLI grants allocator authority for owned execution, registers no host imports and rejects `--source-root` with artifact input. The current owned adapter refuses reachable host calls during preflight; unused declarations stay inert. This implements the current VM profile; native/platform and combined conformance remain separate work. See [artifact execution](docs/architecture.md#ownership-artifact-cli-and-execution), updated/verified 2026-10-20 JST.

Source shared/weak values execute explicit cloning, checked reads, replacement, weak creation/cloning/upgrades and cleanup for storable values, including nested Vec/Map and recursive shared shapes. `shared.lang` prints `9`; `shared_owned.lang` prints `13` after rejecting a strong cycle and reading nested owning nodes through a borrowed guest call. Weak owners preserve metadata after payload expiration. Live reads exclude alias replacement; failed publications release incoming owners. Indirect previews are implemented under [RFC 0016](docs/rfcs/0016-indirect-repl-previews.md). Native execution and combined VM/native conformance retain their separate gates. See [source shared execution](docs/architecture.md#source-shared-and-weak-execution), updated/verified 2026-10-20 JST.

Source Vec values execute new/len/push/pop and checked shared/mutable element reads for storable values, including nested collections, shared/weak values and in-region arena references. `vec.lang` prints `9`; `vec_owned.lang` prints `12` after growing owning pairs, mutating a buffer, replacing a field, popping a value and forwarding a read through guest calls. Live element loans exclude push/pop; failed push cleans its evaluated owning argument. Indirect previews are implemented under [RFC 0016](docs/rfcs/0016-indirect-repl-previews.md). Native execution and combined VM/native conformance retain their gates. See [source Vec execution](docs/architecture.md#source-vec-execution), updated/verified 2026-10-20 JST.

Source Map values execute new/len/insert/get/remove with i64 or exact UTF-8-byte Text keys and storable nested values. `map.lang` prints `49`; `map_owned.lang` prints `33` after growing owning pairs, replacing an equal Text key, reading through guest calls and removing a value. Replacement preserves the original key and returns the old value; evaluated incoming owners are cleaned on refusal. Live reads exclude mutation. Indirect previews are implemented under [RFC 0016](docs/rfcs/0016-indirect-repl-previews.md). Native execution and combined VM/native conformance retain their gates. See [source Map execution](docs/architecture.md#source-map-execution), updated/verified 2026-10-20 JST.

[Nested service examples](docs/architecture.md#nested-service-source-execution) print `8` (shared → Vec → Map clone/weak/read), `21` (container cycle rejection), `4` (arena → Map → Vec same-region cycle) and `6` (nested mutable replacement/removal). Strong edges and arena brands are checked through containers, with iterative cleanup preserving original spans. Ordinary loans, allocator authority and arena owners remain unstorable; distinct-arena/shared-heap edges and region escapes are rejected. Updated/verified 2026-10-18 JST.

`check` validates Copy or RFC 0007/0009 ownership source through its applicable HIR/MIR analyses and exits 0 on success; it executes no guest code and does not require `main` or host bindings. `run` supports the Copy profile and an initial ownership profile with allocator authority, Bytes new/len/get/set/push/clone, consuming UTF-8 conversion, Text byte length/clone/concat/to-bytes/equality, nested Bytes/Text/Copy records/enums and Option/Result construction/extraction, lexical loans and cleanup. The answer script prints `42`; `bytes.lang` and `owned_bytes.lang` print `0`, the extraction/mutation example prints `1`, `utf8.lang` prints the byte length `3`, and `text.lang` prints `2` after checking independent clones and conversions. `owned_aggregate.lang` prints `9` after moving nested owners and reading projected references. Storage is released before printing the result. Extraction checks its case and reports a source-located guest panic on mismatch. Unsupported operations/payloads fail preflight.

The ownership profile also executes guest functions with Copy/owned parameters and results and borrowed parameters, including self/mutual recursion. `owned_calls.lang` prints `1` after recursive Bytes transfer and a borrowed call; `cleanup_calls.lang` prints `13` after nested aggregate/call cleanup. `owned_replace.lang` prints `3` after whole-owner/field replacement and a scalar loop. Assignment captures its destination before the RHS and publishes the new value before old-value cleanup. Return values stay protected until callee cleanup succeeds; failures include the entered source call stack. Verified 2026-10-18 JST; see [calls](docs/architecture.md#owned-guest-calls-and-returns), [failure exits](docs/architecture.md#owned-failure-exit-conformance) and [replacement evidence](docs/architecture.md#prepared-ownership-replacement).

Compile/runtime errors exit 1; usage errors exit 2. A script result is not an exit code. `--source-root DIR` selects file authority; otherwise check/scripts use the root file's parent and REPL sessions have no file-module authority. `run` and `repl` accept `--fuel`, `--service-work`, `--host-calls`, `--frames`, and `--cells`. The REPL supports current-profile ownership definitions and expressions, including bounded Bytes/Text, Vec, key-ordered Map, shared payload, weak-liveness and nested result previews, with a fresh allocator and cleanup instance per evaluation. Arena references still cannot escape their region. The CLI registers no host imports. Verified 2026-10-20 JST.

The embedding API `lang_vm::unit::owned::execute_preview` executes a named zero-argument ownership function by checked ID, renders bounded Bytes/Text/nested results, and releases all guest storage before returning its string. [The source library](examples/analysis/repl_preview.lang) is executed by `cargo test --test owned_execution preview::named_owned`. The frontend also infers a validated generated ownership wrapper's result through normal HIR/MIR checks; `cargo test --test owned_execution inference::` executes these inferred results. Embedding sessions use `Session::submit_with_allocator` to explicitly authorize ownership expressions; the original `submit` grants no allocator authority. Definitions commit without execution, and callers may intentionally reuse an allocator's cumulative quota. [Preview](docs/architecture.md#owned-result-previews), [inference](docs/architecture.md#ownership-expression-inference) and [session evidence](docs/architecture.md#ownership-repl-sessions), updated/verified 2026-10-18 JST.

The REPL commits successfully checked item definitions and evaluates one expression per submission with fresh VM counters. It prints scalars, qualified aggregates and current-profile owned previews, accepts multiline input, and supports `:reset` and `:quit`. Only source definitions persist; every guest result is released before output. Failed submissions preserve prior definitions and original-source diagnostics/stacks; host effects are not rolled back. Unsupported reachable ownership operations/results fail before provider callbacks. Later indirect-service previews remain gated.

Configure an LSP client to execute `target/debug/lang lsp` after `cargo build`. The stdio profile supports initialize/shutdown/exit, UTF-16 positions, full document synchronization, versioned diagnostics, watched-file refresh, and canonical four-space document formatting. Initialization grants at most one explicit local workspace root; without one, only open buffers are available. Code actions, completion, navigation, and debugger integration remain later work. See [architecture and protocol sources](docs/architecture.md#diagnostics-lsp-and-debugging).

The shared host APIs include `lang::frontend`, `lang::artifact`, `lang::repl::Session`, `lang_semantics::analysis`, `lang_semantics::database`, `lang_syntax::format`, and the `unit` modules in MIR, bytecode, VM, and Cranelift crates. Source analysis returns an explicit Copy or ownership program; the ownership variant retains checked HIR and MIR. Compiler/codec/cache budgets are configurable. VM defaults are 1,000,000 fuel, 1,000 host calls, 256 guest frames, and 1,000,000 live value cells. Interruption/deadlines act at guest safe points; trusted native callbacks are non-preemptible and must not unwind.

The whole-unit native API in `lang_codegen_clif::program` supports the RFC 0006 Copy profile, including guest calls, boolean interfaces, records/enums and patterns. Ownership-aware native lowering and explicit C adapters are implemented below; production native SDK packaging remains Milestone 2 work. `cargo run -- 20 22` retains the original scalar VM/JIT comparison. Cranelift 0.136.3 is the initial native backend. Native linker/debug evidence and historical platform measurements are documented in [architecture](docs/architecture.md), [benchmarks](docs/benchmarks.md), and the [roadmap](docs/roadmap.md); local source-conformance execution on 2026-10-17 is macOS AArch64, with Windows execution requiring CI evidence.

Before finalizing code changes, run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

The Rust native embedding boundary executes [source Bytes parameters/returns](examples/analysis/native_owned_boundary.lang), [owned guest calls](examples/analysis/native_owned_calls.lang), [records](examples/analysis/native_owned_records.lang), [whole-sum extraction](examples/analysis/native_owned_sums.lang), [borrowed field mutation](examples/analysis/native_owned_loans.lang) and allocator, buffer, collection, shared/weak and lexical arena services (the existing [shared](examples/scripts/shared_owned.lang), [Map](examples/scripts/map_owned.lang), [Vec](examples/scripts/vec_owned.lang), [Text](examples/scripts/text.lang) and [UTF-8](examples/scripts/utf8.lang) examples), together with [nested arena owners and guest-call reads](examples/analysis/native_arena_regions.lang) and [owner reopening](examples/analysis/native_arena_reuse.lang), through generated machine code. It lowers scalar/Copy control, place projections, ordinary loans, prepared replacement and simultaneous CFG arguments. VM and native adapters share instrumented control and usage; native reports include loan/escrow cells and the original failure span. Run `cargo test --test native_owned_boundary` for VM/native comparisons, failure cleanup, admission, poison and compilation budgets. Matching Mach-O/COFF code and in-memory metadata are available; macOS standalone owned programs also pass direct/response-file link/run conformance; Source compilation and generated startup are described below; SDK distribution remains milestone work. See [the boundary's scope](docs/architecture.md#native-whole-owner-boundary), updated/verified 2026-10-19 JST.

Finite service-work budgets cover all current allocator, Bytes/Text, Vec/Map, shared/weak and arena services in the owned VM and canonical native profile. One invocation counter follows guest calls, loans and service views; typed copy/growth, UTF-8 scans, Map lookup and managed payload inspection share deterministic events. Refusal preserves numeric limit/used/requested details and drains owners without charging mandatory cleanup. The [buffer example](examples/analysis/native_buffer_work.lang) still returns 67 using 37 units; [managed Text payloads](examples/analysis/native_managed_text_work.lang) cross Vec/shared/arena views. Native object execution on both tier-1 hosts is demonstrated below; production driver packaging remains P02 work. Verified 2026-10-19 JST.

The trusted Rust ObjectArtifact::bind API retains a linked image owner and uses the same checked owned gateway as JIT execution. Host-target and cumulative compilation-budget preflight precede symbol resolution; missing symbols have structured errors. A real linked macOS image runs the mixed Vec/Text/shared/arena example with matching VM results at every work prefix. The unsafe binding requires code linked from the exact artifact/toolchain and live for the owner lifetime. The development standalone drivers below demonstrate both tier-1 targets; clean-install packaging remains P02 work. Verified 2026-10-19 JST.

On macOS AArch64 and Windows x86-64 MSVC, the [standalone conformance test](tests/support/native_static.rs) links the same 31 source units into 62 executables per host through direct and response-file requests, including paths with spaces. It compares every work/fuel prefix, allocation refusal, interruption, frame/cell limits, panic and cleanup against VM semantics. [Copy payload control](examples/analysis/native_owned_control.lang) combines nominal records/enums, a managed vector, boolean calls and an owning return. [Full Windows workflow run 39198008813](https://github.com/carimatics/lang/actions/runs/39198008813), at 516f663, passes both the native tests and the later prototype benchmark. The development driver regenerates/checks matching metadata and invokes the prelinked machine functions; SDK assembly is described below; public linking/isolated installation remains P12/P02 work. Run cargo test --test native_owned_boundary standalone_owned_object -- --nocapture. Verified 2026-10-19 JST.

Use `--service-work N` for source, loaded artifacts and REPL evaluation. Omission is unbounded; zero allows only zero-cost work; values are u64. Configure the limit at execution, without storing it in the artifact; REPL limits reset per submission. Runtime errors show service-work usage and refused limit/used/requested values. Invalid options exit 2, script budget failures exit 1, and REPL runtime errors allow later submissions. These CLI paths execute in the VM; native execution remains separate. Verified 2026-10-19 JST.

```bash
cargo run --bin lang -- run --service-work 37 examples/analysis/native_buffer_work.lang
```

### Native program output

Updated/verified 2026-10-20 JST: `lang native` compiles complete source units for `aarch64-apple-darwin` or `x86_64-pc-windows-msvc` (default: compiler host).

```bash
cargo run --bin lang -- native --output /tmp/lang-owned-native examples/scripts/owned_bytes.lang
cargo run --bin lang -- native --target x86_64-pc-windows-msvc --output /tmp/lang-c-native examples/analysis/c_owned_exports.lang
```

Choose a fresh output directory. It contains `package.json`, independently validated `program.lbc`/`program.native`, original `program.o`/`program.obj` and canonical `bindings.rs`. Ordinary Copy/ownership programs receive `main.rs` for the actual zero-parameter i64 main; C units receive `program.h` with no default entry. Only complete outputs publish atomically; existing destinations are preserved.

Generated startup uses production runtime APIs, prints the script value and checks SDK identity before binding the original object. [Compiler tests](tests/native_compiler.rs) build/run these outputs and a real C caller after source removal. Public `link-native` and isolated installation remain P12/P02 under accepted [RFC 0020](docs/rfcs/0020-native-sdk-and-startup.md). The SDK requires the exact Rust toolchain and native linker; C hosts also require C11.

### Native package verification API

Updated/verified 2026-10-20 JST: `native::manifest::SdkSnapshot::open` and `ProgramSnapshot::open` validate RFC 0020 data and retain private copies for later trusted linking. They reject duplicate/unknown fields and roles, bad paths, budgets, SDK/build/toolchain tags, hashes and semantic/native/object mismatches. SDK copies stream through a 64 KiB buffer; program data undergoes independent decoding and plan checks without binding or guest execution.

The caller selects the source roots and a trusted private staging parent, retains each snapshot owner and leaves its files unchanged while tools consume them. Original inputs can be removed after a successful snapshot; failure/drop removes only the owned staging directory. Native files and Rust host glue remain trusted inputs; hashes are pairing checks. These APIs do not invoke rustc or link an application. P12/P02 remain necessary for `link-native` and both-host isolated installation. See [verification tests](tests/native_packages.rs); `native_manifests` is a bounded parser fuzz target, with no filesystem or native authority.

### Scoped native SDK runtime API

Updated/verified 2026-10-20 JST: `lang_codegen_clif::sdk::runtime` provides unsafe `execute_copy`, `execute_owned` and `with_c` APIs for the original linked image. They check host target/build before symbol resolution, independently decode bounded semantic/native records and reuse production binding/runtime primitives. The caller retains the exact image and all dependencies. Startup does not compile source, emit/JIT code or interpret guest instructions.

Copy rejects all configured controls, including an unfired interruption/deadline, under its synchronous unmetered ABI. Ownership uses the caller's actual allocator, cleanup Instance and control, retaining usage, poison, the original failure report and semantic source bytes. C binding lasts only for the explicit closure: all calls return and every context is destroyed on the owner thread before it exits, including unwind. The unsafe caller prevents raw clients/contexts from escaping and supplies actual C configuration, providers and import grants. Metadata grants none. See [runtime implementation](crates/codegen-clif/src/sdk/runtime.rs) and [source-free execution/guard/state tests](tests/native_compiler.rs). Public SDK linking and both-host isolated installation remain P12/P02.

### Native SDK assembly

Updated/verified 2026-10-20 JST: `python3 tools/build-native-sdk.py --output SDK` builds and assembles a fresh matching host SDK; add `--release` for release artifacts. It consumes the exact successful Cargo artifact/build-script output, including Rust/proc-macro libraries and native support archives, rather than scanning cached dependency variants. The SDK includes the compiler, scoped runtime, common C header, Copy/owned/C startup examples, bilingual instructions and bounded `sdk.json`. It streams/hash-checks private copies through the production verifier before exclusive publication and retains compiler execute permissions. See [SDK build/use instructions](docs/native-sdk.md) and [assembly/source-free tests](tests/native_sdk.rs). Public `link-native` and both-host isolated installation remain P12/P02.

## Design process

Changes to syntax, type semantics, memory safety, concurrency, ABI, package compatibility, or the plugin boundary require an RFC. Use the repository skill `$language-design-rfc` or start from `docs/rfcs/0000-template.md`.

Implementation work must preserve equivalent observable behavior between the bytecode and native backends unless an RFC explicitly documents a mode-specific restriction.

## Review scope and current limits

The 2026-10-17 review hardened JIT memory reclamation, bytecode span preservation, CodeView machine types, bounded decoding, semantic-version validation, whole-request capability preflight, and Rust adapter cleanup. Linker response files now require a new path; existing files and symlinks are rejected without truncation. Repeated callers must choose a fresh response-file path or explicitly remove their own old file.

Historical benchmark captures remain immutable and predate the JIT reclamation fix. See [benchmark evidence](docs/benchmarks.md) before interpreting current performance. Direct native host callbacks remain trusted and must not unwind; this is not a sandbox.

## Project status

- Maturity: research / pre-alpha
- Supported host platforms: macOS and Windows are the initial goal
- Initial implementation language: Rust
- License: not selected yet

