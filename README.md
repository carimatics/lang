<!-- canonical-language: en; japanese-translation: README.ja.md -->

# lang

[日本語版](README.ja.md)

`lang` is a pre-alpha research project for a new systems programming language and its complete developer toolchain. The implementation starts in Rust and is intended to become self-hosting after the language, compiler, and bootstrap process are stable enough to reproduce themselves.

The project is currently defining semantics and validating architecture. There is no stable syntax, ABI, package format, or release schedule yet.

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
- [RFC process and template](docs/rfcs/0000-template.md)
- [RFC 0001: Project charter and compatibility policy](docs/rfcs/0001-project-charter.md)
- [RFC 0002: Prototype integer and structured control-flow semantics](docs/rfcs/0002-prototype-control-flow.md)
- [RFC 0003: Versioned component contracts and capability imports](docs/rfcs/0003-versioned-component-contracts.md)
- [RFC 0004: Owned values, explicit sharing, and cyclic graph storage](docs/rfcs/0004-owned-values-and-cyclic-graphs.md)
- [RFC 0005: Structured tasks, cancellation, and isolated services](docs/rfcs/0005-structured-tasks-and-isolated-services.md)
- [RFC 0006: Source frontend and portable VM programs (accepted)](docs/rfcs/0006-source-frontend-and-portable-programs.md)
- [Contribution guide](CONTRIBUTING.md)

## Development

Milestone 1 now has backend-independent `lang-diagnostics`, `lang-syntax`, and `lang-hir` crates. Diagnostics provide immutable UTF-8 source snapshots, validated byte spans, Unicode/UTF-16 positions, labels, notes, and atomic fix previews. The RFC 0006 lexer/parser validates UTF-8 input, decodes signed decimal literals, and parses declarations, control flow, calls, aggregates, and patterns into an arena-based lossless tree. It retains trivia, invalid tokens, and missing-syntax nodes, recovers later declarations, and bounds bytes, tokens, nodes, depth, list elements, diagnostics, and work. HIR analysis resolves and type-checks structured RFC 0002 function input: lexical local scopes, function-wide name uniqueness, immutable parameters, `i64`/internal `bool`, mutation, condition types, and explicit-return paths. It preserves spans and rejects cyclic or over-budget analysis input. Run `cargo test -p lang-diagnostics -p lang-syntax -p lang-hir` for focused coverage. Source execution remains unimplemented.

`lang-semantics::modules` loads parsed modules under an explicit canonical source root, collects a shared item namespace, and resolves imports and module/enum-case paths with visibility checks. It rejects root escapes, duplicate canonical files, module/alias cycles, duplicate names, and resource-limit violations. `lang-semantics::binding` resolves parsed local/pattern scopes and type paths into `lang-hir::resolved`, rejects private types in public signatures, and retains source spans on declarations, type annotations, expressions, patterns, and control flow. Run `cargo test -p lang-semantics --test modules --test binding`. Source execution remains implementation work.

`lang-hir::layout` checks nominal cycles and member shapes and computes bounded copy-value cells/depth before guest allocation. Defaults are type depth 64 and 4,096 cells per value; enum storage reserves its largest payload. Run `cargo test -p lang-hir --test nominal_layouts` for layout boundaries and malformed-input coverage.

`lang-hir::program` checks complete resolved source functions, calls, aggregate operations, patterns, and bounded exhaustiveness into a read-only typed program. It reports missing witnesses and unreachable arms/statements while still checking their bodies. Run `cargo test -p lang-semantics --test program_typing` for source compile-pass/fail, finite-domain oracle, and malformed-IR coverage. MIR/VM execution and source CLI entry points remain unimplemented.

`lang_mir::unit` now defines and validates complete compilation units with source bytes/spans, calls, nominal values, SSA dominance, and bounded allocation/work accounting. Run `cargo test -p lang-mir --test units`. The unit data structures are not a serialized format.

`lang_semantics::program_lowering` lowers checked source programs, including calls, records/enums, pattern tests/bindings, joins, and loops, to verified units with byte spans on all generated functions/instructions/terminators. Construction and verification share resource budgets. Run `cargo test -p lang-semantics --test program_lowering`. Source VM/native execution remains integration work.

`lang_bytecode::unit` encodes/decodes canonical schema-1 artifacts with explicit versions/opcodes, ID remapping, UTF-8/span validation, and shared decoder/verifier resource budgets. Run `cargo test -p lang-bytecode --test unit_codec`. The codec includes a fixed golden fixture and malformed-input/fuzz coverage; source VM execution is still pending.

`lang-semantics::lowering` converts that successful primitive HIR to validated typed MIR. Branch assignments merge through typed block parameters; loops carry mutable values over back-edges; early returns and unreachable statements preserve their accepted behavior. Instruction/terminator spans map into the existing bytecode and native debug metadata. Run `cargo test --test hir_lowering` for structured-input VM/JIT differential execution, host-object debug mapping, and lowering-budget checks. These tests establish the HIR-to-MIR path, not end-to-end source compilation.

`lang-query` provides the shared incremental graph/cache core: immutable input revisions, explicit query dependencies, transitive invalidation, and publication tickets that reject obsolete analysis results after edits or close/reopen. It bounds graph nodes, edges, and bookkeeping work and preflights failures before state changes. `cargo test -p lang-query` covers cache isolation, stale/foreign results, dependency replacement, cycles, limits, and real structured-input HIR diagnostic caching. Adapters must declare every observed dependency; automatic compiler queries, parser/module dependency discovery, and LSP transport remain pending.

`lang-lsp` maps existing structured diagnostics into the LSP 3.18 diagnostic data model for UTF-16 positions. It handles protocol LF/CRLF/CR line endings, surrogate-pair widths, EOF ranges, stable codes/severities, and cross-file related locations using host-supplied URIs. Run `cargo test -p lang-lsp` for conversion and resource-limit tests. This data adapter does not yet provide a server, position-encoding negotiation, JSON-RPC transport, revisioned publication, or code actions; those remain integration work. Official specification sources and verification dates are recorded in the [architecture](docs/architecture.md#diagnostics-lsp-and-debugging).

Requirements:

- Rust `1.99.0`, pinned by `rust-toolchain.toml`
- `rustfmt` and Clippy, installed by the pinned toolchain
- `mise` when using the repository's auxiliary tool versions

Run the current VM/native equivalence prototype:

```bash
cargo run -- 20 22
```

The command constructs a validated typed MIR function, lowers it to verified register bytecode, executes it in the VM, compiles the same MIR with Cranelift JIT, and compares the results. Its wrapping `i64` addition matches accepted RFC 0002; the source frontend is not implemented. Cranelift 0.136.3 is the selected initial native JIT/AOT backend; the decision and reevaluation triggers are recorded in the [architecture](docs/architecture.md#backend-strategy).

Workspace tests also exercise a minimal embedding path: a backend-neutral host-function ID resolves a typed `extern "C" fn(i64) -> i64` registration, and the same MIR host call runs through both the VM and Cranelift JIT. The VM exposes composable opt-in instruction-fuel and host-call budgets plus a value-slot limit, optional shared one-way interruption flag, and absolute monotonic deadline. Successful limited execution reports both consumed counts and the static register-slot footprint. Every bytecode instruction and terminator costs one fuel unit; each entered callback costs one host-call unit, and both dynamic limits are checked before the corresponding side effect. After validation and arity checking, the VM rejects a program whose register count exceeds the slot limit before allocating its register file. Interruption and deadline checks run before register-file allocation and at instruction, terminator, and callback boundaries; structured errors report both consumed counts, and interruption takes precedence when both conditions hold at one safe point. A callback may request interruption or overrun the deadline, which is observed after that callback at the next safe point. Existing convenience functions remain unbounded, non-interruptible, and without a deadline. The slot count is not a byte-based heap limit and does not include callback allocations. This prototype does not define a stable ABI and does not yet provide aggregate values, error transport, panic containment, capabilities, heap/recursion/task limits, preemptive callback interruption, or native/JIT resource enforcement.

`lang-codegen-clif` can also emit an object artifact for the same MIR, exporting the compiled function and leaving host callbacks as imports. In addition to the host-native path, callers can select an explicit target triple; host-independent tests validate `x86_64-pc-windows-msvc` COFF structure and source-driven CodeView records. The root crate's target-neutral `LinkRequest` and `SystemLinker` preserve ordered inputs, support `cc` and `link.exe`, expose structured failures, and serialize explicit GNU UTF-8 or Windows UTF-16LE response files. macOS AArch64 integration and [Windows x86-64 run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398) both execute direct and response-file links through this shared path. Production installation discovery, startup/runtime packaging, additional target policy, and a stable native ABI remain separate work.

A clean pre-implementation macOS AArch64 [debug-information inspection](benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) records the original negative baseline. The later committed [source-mapped DWARF inspection](benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md) verifies that the representative add, minimum, and host-callback Mach-O objects contain relocated DWARF v4 compile-unit, subprogram, and line-table data derived from structurally validated MIR source maps. All three objects pass `dwarfdump --verify`; direct and response-file executables link and run; and both generated dSYMs pass verification while retaining the fixture path and all three functions. The committed [Windows x86-64 CodeView/PDB inspection](benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md) validates C13 records and relocations in the equivalent COFF objects, links them cross-target into PE/PDB artifacts with LLD, and has LLDB resolve the same functions to the fixture. The inspection is cross-target; the separate Windows CI run supplies native execution evidence. Source-language locals, JIT debugger registration, unwind metadata, and optimized stepping remain separate work.

The two-value accumulator prototype carries both a countdown value and a wrapping sum through block parameters on a back-edge. A conditional-sum prototype adds a branch inside that loop and merges the add/skip paths through another pair of block parameters. VM and JIT differential tests cover non-positive inputs, both conditional paths, and bounded positive loops; macOS AArch64 direct/response-file integration executes both AOT objects, and the host-independent Windows test verifies their exported COFF symbols. These prototypes broaden MIR CFG evidence without defining source-level loop syntax or allocation semantics.

`SystemLinker::discover_host` scans `PATH` for the host-flavor candidates (`cc`, `clang`, and `gcc`, or `link.exe` on MSVC), skips non-executable files on Unix, and reports the searched path and candidates through a structured error. The platform integration tests use the discovered executable. Visual Studio, SDK, and compiler installation discovery outside an initialized toolchain environment remains open.

`LinkTarget` centralizes the initial tier-1 linker policy for `aarch64-apple-darwin` and `x86_64-pc-windows-msvc`. It maps each exact triple to its linker flavor, default program, and ordered discovery candidates, rejects unsupported triples structurally, and drives explicit-target construction and discovery through `SystemLinker`. The object-format test and both cfg-gated platform integrations consume this shared policy. Additional target policies remain open.

The clean schema-20 [macOS AArch64](benchmarks/results/2026-10-16-macos-aarch64-de248a1.json) and [Windows x86-64](benchmarks/results/2026-10-17-windows-x86_64-7c37d5d.json) benchmarks preserve 56 general timing rows, four exact callback heap profiles, four process-memory distributions, and fourteen component-boundary rows. The representative application records a 67.592 us median with 1.109% RSD on macOS and a 236.804 us median with 6.201% RSD on Windows across 31 samples of 1,000 operations. The fixture is host-orchestrated and includes lowering and JIT compilation, so the results are directional application evidence rather than isolated call overhead or stable cross-platform budgets.

The typed MIR, bytecode verifier, VM, JIT, and object path also share signed `i64` comparison and an explicit control-flow graph. The two-argument minimum function branches from an entry block to separate return blocks, while a focused jump test covers an explicit edge. Typed block parameters and edge arguments represent phi-like merges without a separate phi instruction. MIR and bytecode validation reject invalid targets, unreachable blocks, entry-block predecessors or parameters, edge argument arity/type mismatches, and values that do not dominate their uses. A countdown carries one `i64` around a back-edge, a bounded sum carries a counter and accumulator, and iterative Fibonacci carries and rotates a counter plus two sequence values; VM/JIT differential tests and AOT object paths cover all three. A bytecode back-edge test proves that instruction fuel stops a non-terminating loop. Allocation and source-level control-flow semantics remain unimplemented.

Write either inspection target to a host-native object file for symbol and disassembly inspection:

```bash
cargo run --release --bin prototype-object -- add /tmp/prototype-add.o
cargo run --release --bin prototype-object -- min /tmp/prototype-min.o
cargo run --release --bin prototype-object -- host-callback /tmp/prototype-host-callback.o
```

The command reports the exported symbol, generated function-code size, and complete object size for the add, signed-minimum, or host-callback target. It does not invoke a linker.

Run the executable ownership experiments separately with `cargo test -p lang-semantics`. The async model compares the strict `RejectAll` suspension policy with an explicit `AllowSharedFrameBorrows` alternative that permits shared task-frame borrows while retaining exclusive-borrow rejection and deterministic cancellation cleanup. The graph model compares the default `Reject` shared-cycle policy, an experimental `CollectUnreachable` tracing policy, and arena-scoped cyclic storage. The service model compares `OwnerIsolated` mutation with an `ExclusiveCapability` alternative while retaining sendable payload moves, FIFO mailboxes, and deferred cancellation cleanup. RFCs 0004 and 0005 accept the strict policies; the models remain incomplete evidence, not production ownership/concurrency implementations, and the permissive alternatives are unaccepted.

Run the prototype backend measurement harness from a clean release build:

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000 \
  --startup-iterations 10
```

The harness emits versioned JSON containing the commit and dirty state, toolchain, host, and system-linker metadata, generated JIT code and AOT object sizes, linked executable size, requested-payload heap profiles, process peak-memory distributions, and distribution statistics for lowering, JIT/AOT compilation, direct/VM/JIT calls, host callbacks, control flow, linking, and process startup. Schema 20 adds one representative application row that composes two-provider negotiation, deterministic lock generation, exact-lock replay, instantiation, scalar and typed-resource VM/JIT calls, and guest-trap cleanup; schema 19 adds component instantiation, lockfile replay, host-value/failure cleanup, retained-instance process memory, and component artifact/document sizes; schema 18 adds iterative-Fibonacci CFG timing and artifact evidence; schema 17 adds the representative mixed-lifetime allocation trace and paired process-memory evidence; schema 16 adds variable-size allocation timing and heap evidence; schema 15 adds separately labeled component metadata-validation, interface-negotiation, scalar-dispatch, and resource-lifecycle series; earlier schemas add the allocation-churn, process-memory, heap, allocation, control-flow, and startup rows documented in [the benchmark evidence](docs/benchmarks.md). Platform series are renamed when their linked workload set grows. Consumers must branch on `schema_version`, and prior JSON files remain immutable evidence. These measurements are prototype evidence rather than accepted performance guarantees. Record results only from a quiet machine and retain the exact command and raw JSON.

The allocation-boundary prototype includes single, 64-box burst, free/reallocate churn, and 16-slice variable-size callbacks, with matching VM/JIT paths and macOS/Windows AOT contracts. Schema 17 added a larger compiler-like process workload while retaining their four exact requested-payload profiles: many small long-lived graph nodes, one-third release and differently sized replacement, then larger transient buffers. Every payload byte is touched; paired launches of the same benchmark executable provide a control and workload peak-resident distribution. The current schema 20 macOS run records 18,102,042 peak live requested bytes, a 21,053,440-byte median resident delta, and a 2,951,398-byte combined overhead/fragmentation estimate. The estimate remains OS- and allocator-specific, includes page rounding and allocator retention, cannot separate metadata from fragmentation, and does not define source-language allocation, ownership, collector policy, or memory limits.

Common checks:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

The workspace currently contains only the components exercised by the prototypes: `lang-mir`, `lang-bytecode`, `lang-vm`, `lang-codegen-clif`, the typed host-function registry, bounded experimental schema-1 scalar codec, candidate schema-2 contract/failure-report codec, scalar component-contract validator, and generation-tagged resource table in `lang-runtime`, and executable design models in `lang-semantics`. Additional boundaries from `docs/architecture.md` should appear only with their implementation.

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

