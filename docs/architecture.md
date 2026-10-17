<!-- canonical-language: en; japanese-translation: ja/architecture.md -->

# Architecture baseline

[日本語版](ja/architecture.md)

Status: **accepted design baseline** under RFCs 0001–0005 (2026-10-17), with incomplete prototype implementations. Acceptance is not stabilization. The RFCs govern semantics; broader product directions below require follow-up RFCs before implementation.

## Goals

- Native performance and low-level control on Windows and macOS.
- Memory and data-race safety by default without a mandatory tracing GC.
- A productive, expression-oriented language suitable for application code.
- AOT executables/libraries and embeddable scripts using one semantic frontend.
- Efficient native-to-script and script-to-native calls in one process.
- A future path for several source languages without binding compatibility to one frontend ABI.
- Excellent diagnostics, build speed, debugging, IDE support, and reproducibility.

## Non-goals for the first implementation

- Supporting every CPU architecture or operating system.
- Stabilizing a native language ABI.
- Hot-reloading arbitrary native code safely.
- Transparent distributed execution.
- A tracing GC in the core language.
- Multiple source languages before the first frontend and component boundary are stable.
- A package registry before local and Git-based dependency workflows are reproducible.

## Product model

Start with one language that has two explicit programming layers:

- **Safe application layer:** value-oriented APIs, null safety, algebraic data types, pattern matching, type inference, traits, closures, structured concurrency, and managed shared references where requested.
- **Systems layer:** allocators, raw pointers, layout control, C ABI, atomics, SIMD, inline assembly/intrinsics, and platform APIs behind `unsafe` or capability-gated interfaces.

This keeps interoperability inside one type system. A second language should be added only when it can lower to the same component model and conformance suite without weakening safety.

## Compiler pipeline

```text
source text
    │
    ▼
lossless syntax tree ───────────── formatter / IDE recovery
    │
    ▼
resolved HIR ───────────────────── names / modules / desugaring
    │
    ▼
typed HIR ──────────────────────── inference / traits / effects
    │
    ▼
MIR ────────────────────────────── ownership / destruction / async lowering
    │
    ├──────────────► bytecode ───► verifier ─► embedded VM
    │
    ├──────────────► native SSA ─► Cranelift/LLVM ─► object/linker
    │
    └──────────────► interface IR / metadata ───────► bindings and plugins
```

The typed MIR is the semantic contract. Backend-specific optimizations may differ, but observable behavior, destruction order, panic/error behavior, integer semantics, and concurrency rules must not.

Accepted [RFC 0002](rfcs/0002-prototype-control-flow.md) defines `i64` parameters/results, wrapping addition, signed comparison, mutable locals, `if`/`else`, `while`, explicit `return`, and left-to-right evaluation. Values cross joins/back-edges through typed block arguments. Accepted [RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md) adds lexical rules, modules/calls, aggregates/patterns, portable bytecode, bounded frames, and script/REPL behavior. Diagnostics, lossless parsing, source typing/lowering, portable artifacts, bounded VM execution, initial native scalar conformance, scripts, and transactional REPL are implemented. The formatter and parser/module-query LSP prototype are also implemented and verified.

Implemented 2026-10-17: `lang-syntax` validates UTF-8 input without creating a snapshot for invalid encoding or a leading BOM. Input errors retain raw byte ranges. Its lexer retains a contiguous, complete token partition of each accepted snapshot, including whitespace, comments, malformed tokens, and invalid Unicode scalars. It reports signed-decimal overflow, malformed tokens, NUL even inside comments, unterminated block comments, and direct-snapshot BOM violations. Source-byte, token, nested-comment-depth, and diagnostic limits stop analysis with `SRC0011`; comments are scanned iteratively. Thirteen lexer tests cover exact ranges, reconstruction, deterministic mixed Unicode inputs, and resource boundaries.

The parser constructs an immutable arena of syntax nodes, real-token references, and zero-width missing-syntax leaves. It implements the full accepted grammar with separate file/expression entry points; the tree is distinct from semantic HIR. Postfix field access, left-associative addition, non-chaining comparison, named calls, record-head disambiguation, and pattern shape remain explicit in the tree. Delimiter-aware recovery preserves enclosing braces and later declarations. `SRC0012` diagnostics attach machine-applicable fixes only to unambiguous delimiter insertions. Limits cover syntax nodes (2,000,000), tree and parser depth (256, also an internal recursion ceiling), comma-list elements (256), diagnostics (4,096), and work (10,000,000). Work charges token indexing/consumption, node starts, and missing-syntax insertions, including recovery. Resource failure returns an error, never a partial successful tree. Reconstruction, cloning, and dropping use arena references rather than recursive owned nodes. Eighteen parser tests cover syntax snapshots, the full grammar fixture and all its prefixes, 1,000 deterministic malformed inputs, exact depth/list boundaries, recovery, fix previews, and source identity. The `source_parser` fuzz target also validates byte ingestion, both parser entry points, reconstruction, and diagnostic spans. A stable-Rust, uninstrumented 10,000-run smoke completed; it does not establish coverage-guided or sanitizer evidence. Run an instrumented campaign separately with `cargo +nightly fuzz run source_parser` from the repository root. The following stages connect semantic resolution/typing to source execution.

Implemented 2026-10-17: `lang-hir` accepts syntax-independent RFC 0002 function arenas, resolves local names, and produces read-only typed HIR only when analysis has no errors. Parameters are immutable; declarations are visible only after their initializer and within their lexical block; any function-wide reuse is rejected, including disjoint branches. Type checking enforces integer operands/values, boolean conditions, and explicit return on every reachable exit. It emits `SRC0002`–`SRC0007` with primary/related spans; the `i64` operand check also covers signed comparison. Unreachable statements produce a structured warning and are still analyzed; the primitive API retains its structured warning type, while full source-program checking uses accepted `SRC0019`. All nodes retain source-snapshot byte spans, and locals retain display names/declaration spans. Caller-supplied work/local/depth/diagnostic limits, an internal recursion ceiling, checked arena references, cycle detection, and source/name-span validation protect this analysis boundary. Literal decoding/range diagnostics, lexical rules, module loading, source calls, aggregates, and a serialized HIR contract are not introduced by this step.

Also implemented 2026-10-17: `lang-semantics::lowering` maps this HIR to the existing typed scalar MIR, with explicit work/value/block limits and final MIR validation. It preserves left-to-right expression lowering, joins changed branch values with typed block arguments, and carries visible mutable values through loop-header parameters. Early returns create no unreachable joins; unreachable statements generate no instructions. Symbolic construction IDs are remapped to the MIR's required sequential definition order after CFG construction. Snapshot byte spans convert to the existing one-based MIR line/Unicode-scalar-column maps on every emitted instruction and terminator, including synthesized jumps. Structured-input integration tests cover VM/JIT arithmetic, both branch paths, nested branch/loop state, early returns, fuel exhaustion, bytecode span retention, and native JIT/AOT-object source mapping. The complete source lowering and budgeted unit verifier below handle parser-connected calls/aggregates. The legacy scalar validator retains its trusted-input boundary.

### Source name resolution

Implemented 2026-10-17: `lang-semantics::modules` provides host-side source loading and module namespaces. The host supplies a root file and explicit source-root directory; canonical containment is checked before reads. Child paths follow the declared hierarchy (`math.lang`, then `math/inner.lang`), independent of the process working directory. Reads stop at the remaining per-file/total byte budget plus one detection byte. Total bytes/tokens/nodes, prospective modules, function/definition/import counts, diagnostics, and resolution work are bounded. Host declarations and `use` aliases share the import-count bound. Invalid UTF-8 retains a raw input range; valid-source failures retain structured spans. Source display paths must be UTF-8 rather than silently creating lossy aliases.

The module graph retains parsed trees and compilation-local module/item IDs, display names, declaration spans, and canonical import targets. All declaration names are collected before imports resolve. Item kinds share one namespace; aliases use the last segment and cannot re-export private imports. Resolution checks each crossed module/item boundary, allows a module's own private children, and exposes enum cases through the enum. Duplicate canonical files and ancestor cycles are rejected before rereading; alias dependencies use an explicit stack with charged repeated visits. Work charges declarations, path segments, enum-case candidates, alias dependency visits, and ancestor checks. Seventeen local tests cover forward and hierarchical paths, aliases, privacy, cycles, duplicate names, malformed input, cumulative budgets, and Unix symlink boundaries; a Linux-only test additionally covers non-UTF-8 filenames, which macOS filesystems reject during creation. A successful namespace graph does not imply body typing.

Implemented 2026-10-17: `lang-semantics::binding` converts this graph and its original snapshots into backend-independent `lang-hir::resolved` arenas. Declaration IDs are allocated before bodies, allowing forward/mutual guest calls and nominal references. The pass resolves primitive/nominal type annotations, host calls, enum cases, record/field syntax, and all arm bodies. Local values shadow module values; declarations become visible after their initializer and only within their block. Parameters and pattern bindings are immutable; every local/parameter/binding name is unique across the whole function, including disjoint branches and arms. Public signatures, including exposed aggregate fields/payloads, reject private type declarations. Type-use spans, written field/argument order, call sites, block closers, and bare versus parenthesized enum-case syntax survive in the resolved IR. This is name-resolved input for typing, not an executable or serialized artifact.

Binding limits default to 1,000,000 locals/expressions/patterns/blocks each per unit, 4,096 diagnostics, depth 256 (also an internal ceiling), and 10,000,000 work steps. Each visited declaration, type use, block, statement, expression, pattern, arm, local declaration, and namespace request is charged; namespace resolution additionally charges actual path-segment and enum-candidate visits from the same budget. A charged token index excludes leading trivia from semantic spans without repeatedly scanning comment prefixes; the CST retains every original byte. Small-budget tests verify deterministic exact acceptance boundaries and added work for calls/trivia. Seventeen tests cover fixture conversion, annotation/source spans, names/scopes, mutability, public signatures, forward calls, preserved enum syntax, resource bounds, layout integration, and foreign snapshots. Any name failure suppresses the resolved program. Expression/pattern types, aggregate layouts/cycles, exhaustiveness, and return completeness remain subsequent checks; a resolved program must pass them before lowering or execution.

Implemented 2026-10-17: `lang-hir::layout` validates resolved copy-only nominal definitions before guest-value allocation. It checks IDs, original-source/name spans, member uniqueness, nonempty enums, and references; an iterative dependency stack rejects cycles through records or enum payloads without Rust recursion. A scalar costs one cell, a record one plus all fields, and an enum one plus its largest payload. Scalars and empty records have depth one. Checked arithmetic enforces configurable defaults of 4,096 definitions, 256 fields/cases/payload members, type depth 64, 4,096 cells per value, 4,096 diagnostics, and 10,000,000 work steps. Work charges definitions, names, references, dependency visits, case visits, and repeated field/payload accumulation. Ten HIR tests and two parsed-source integration tests cover exact 64/65-depth and 4096/4097-cell boundaries, custom deep stacks, overflow, cycles, malformed metadata, and small budgets. Shape failures return diagnostics without layouts; input/limit failures return structured errors. Layout success does not establish expression/pattern typing, exhaustiveness, or an executable program.

Implemented 2026-10-17: `lang-hir::program::check_program` consumes resolved source HIR and validates its metadata, scopes, signatures, and arena references before constructing a read-only `TypedProgram`. Source arenas must be trees; cycles, shared/orphan nodes, foreign spans, invisible locals, and forged signatures cannot reach lowering. It invokes nominal layout validation, checks primitive/nominal calls and values, exact record fields, enum payload syntax/arity, field projections, pattern shapes/bindings, all arm bodies, and return completeness. Facts retain expression/local/pattern types, resolved field indices, written-to-declared record order, normalized pattern field order, and block return flow. `SRC0005`/`SRC0006` retain their original contexts; new mismatches use `SRC0014`, shapes `SRC0015`, missing witnesses `SRC0016`, covered-arm warnings `SRC0017`, and unreachable-statement warnings `SRC0019`. Warnings allow successful typing; any error suppresses the typed program.

Exhaustiveness uses constructor specialization over boolean/record/enum products and mentioned integer literals plus a complement representative. The search and witness renderer use explicit stacks. An all-wildcard matrix row closes the remaining product without enumerating its values. Missing witnesses use paths valid from the function module. Type checking charges visited metadata/nodes/operands/signature parameters/field candidates from a 10,000,000-step budget; exhaustiveness separately charges every state, row/cell visit, copied member, constructor, related arm label, and witness fragment against 10,000,000 steps. Defaults also bound cumulative arenas at 1,000,000 each, declarations/lists as above, recursion at 256, diagnostics at 4,096, and witness output at 65,536 bytes. Limit failure returns `SRC0011`, never a false exhaustiveness result. Twenty-four source integration tests include 1,458 comparisons against independent finite-domain coverage masks, a 256-field boolean product, witness repair across modules, and forged-IR/budget cases. `typed_source_hir` fuzzes the independent resolved-IR boundary; the target compiles and completed an uninstrumented 10,000-run smoke, without coverage-guided/sanitizer evidence. Source MIR/VM/native integration, portable artifacts, and CLI/tooling remain later stages.

### Compilation units and validation

Implemented 2026-10-17: `lang_mir::unit` defines the RFC 0006 compilation-unit contract: source text/relative paths and byte spans, nominal definitions, typed host imports, named functions/registers/blocks, and an optional `main() -> i64` entry. Operations cover primitive constants/arithmetic, named calls, record construction/projection, enum construction/tests/checked payload projection, and internal integer equality. `.verify` returns an immutable `VerifiedUnit` only after names/types/layouts, signatures, source boundaries, register definitions, dominance, reachability, block arguments, and ownership restrictions pass. A checked enum projection is valid IR without a proven case guard and traps on a wrong runtime case. Source lowering, the compilation-unit VM, and the scalar native adapter now use this representation. Existing single-function APIs and their one-block owned-resource fixtures remain available.

Unit validation uses an iterative nominal dependency walk and compressed predecessor lists with iterative reverse-postorder immediate dominators. Repeated CFG/operand/dominator visits are charged, rather than assuming a structural bound also bounds dataflow work. A shared `Budget` accounts for decoder/verifier buffers: defaults are 10,000,000 work steps, 128 MiB cumulative requested buffers, and 8,000,000 cumulative requested vector elements, including temporary buffers; releasing a buffer does not replenish them. Name bytes/comparisons, source-line indexing, initialized table entries, metadata/type/signature/member visits, instructions, operands, edges, and ancestor walks are additional charged operations. Buffer sizes use checked arithmetic and fallible reservation before allocation. Caller-owned raw unit storage is excluded from verifier allocation accounting; decoding must charge its own buffers to the same budget. Fifteen tests cover these limits and exact layouts, spans, owned restrictions, and 1,536 SSA dominance comparisons against an independent path-removal oracle. Rust object layouts are not a wire format; the portable codec below implements schema-1 byte encoding.

### Source program lowering

The source-generated unit path now connects to the existing native profile through `lang_codegen_clif::unit`. The adapter revalidates requested unit limits, budgets conversion buffers/work, renumbers symbolic registers into legacy sequential order, maps explicit host IDs, and converts immutable byte spans to line/scalar-column debug metadata. Partial or cross-file function maps fail rather than silently losing locations. Unsupported signatures/operations fail before backend code allocation/publication; new guest calls and copy aggregates remain VM-only obligations until native follow-up work. All declared host imports are registry-preflighted before JIT compilation. The original native compilation/legacy validation APIs retain their trusted-input boundary; adapter budgets do not claim native compiler time/memory enforcement. Nine source conformance tests cover arithmetic/CFG VM-JIT equivalence, zero-arity checked invocation, spans through bytecode/JIT/AOT, unsupported cases, typed imports/owned identity, and metadata/budget failures. Tier-1-gated C harnesses link/run parsed-source objects in direct and response-file modes; local 2026-10-17 evidence is macOS AArch64, while the same Windows path requires its CI run.

Implemented 2026-10-17: `lang_semantics::program_lowering` converts immutable typed source HIR to verified compilation units. It preserves left-to-right arguments/payloads, written-order record evaluation with declaration-order storage, single scrutinee evaluation, ordered effect-free pattern tests, checked enum projections after case guards, and arm-local bindings. Exhaustiveness permits an unconditional final arm, with no generic runtime match-failure path. Constant-true structural branches retain verifier reachability for warning-only covered arms. Joins use typed block arguments; loop headers carry visible mutable values, including aggregates/bools. Dead statements emit no instructions. Functions, every instruction, and every terminator (including synthesized jumps) retain semantic byte spans; snapshot source text is copied without file reads. Construction and final verification share cumulative work/allocation/element budgets and unit-wide register/block/instruction counts. Eight parsed-source tests cover these contracts and rejection boundaries. The execution tests below complete source VM and initial native scalar conformance.

### Compilation-unit execution

Virtual-source foundations implemented 2026-10-17: `modules::load_virtual` parses host-supplied root bytes with the same cumulative module/source/token/node budgets and raw encoding validation as file loading. It creates no temporary file and never rereads the root; child files require an explicit canonical source root. `check_expression_function` infers only a private root `__repl_eval_*` wrapper containing one return, no locals/parameters/patterns, and no calls to the wrapper anywhere in the unit. Other declarations retain explicit result checks. Its final signature comes from checked expression facts. `lower_library` suppresses automatic script-entry selection. Eight tests cover virtual UTF-8 snapshots, authority, nested modules, bounds, inference and forged wrapper restrictions, ordinary typing, and library entry policy. The transactional session below uses these APIs.

Implemented 2026-10-17: `lang_vm::unit` executes verified portable programs with explicit `Vec` frames, without Rust-recursive guest calls. Each frame allocates exactly the verified sum of maximum register cells as one flat buffer; offsets and parallel-copy scheduling are register metadata. Records carry a zero header; enums carry a case tag and zero padding up to the largest payload. Aggregate construction/projection, arguments, returns, and CFG copies stay within charged storage. Parallel copies schedule fan-out before overwrites and rotate cycles one cell at a time. Result storage reuses the final frame buffer. Typed embedding values validate exact sizes, bools, nonzero owned indices, record headers, enum tags/padding, and nested members before execution. Existing owned identity remains a logical resource-index transfer, not a new resource manager.

Safe default limits are fuel 1,000,000, host calls 1,000, frames 256, and live cells 1,000,000; embeddings explicitly configure alternatives or unbounded execution. Interruption precedes deadlines, which precede instruction-fuel checks. An instruction/terminator costs one fuel; a guest call then rechecks control, prospective frame depth, and live cells before allocation/entry. A refused call remains charged and excludes the unentered callee from its trace. Initial frame/cell rejection consumes zero fuel. Host calls recheck control/budget and charge immediately before the trusted, non-preemptible callback. All declared imports, including unused ones, resolve by qualified name to exact typed registry registrations before any guest effect. Failure releases guest frames and reports reason, counters, fault span, and innermost-first call sites; earlier host effects remain. Trace allocation itself is fallible and explicitly reports incompleteness if allocation fails. Limits account logical guest cells, not callback allocations or native execution.

Seventeen source-to-canonical-wire-to-VM tests cover calls, 2,000-level recursion, mutual recursion, aggregates/patterns, evaluation order, frame release, precise failed-call/host metering, control precedence, deadlines, imports, and source stacks. Seven hand-built unit tests include 324 parallel-copy oracle runs, canonical value failures, largest-enum-cell charges, owned identity, and `InvalidVariantProjection`. Parsed scalar conformance also compares this VM to the original scalar VM and JIT. The decoder fuzz target now executes accepted entries under 128 fuel, zero host calls, eight frames, and 4,096 cells; a 10,000-run uninstrumented smoke passed without claiming coverage/sanitizer evidence. The CLI/REPL and editor integrations described below complete those source-facing outcomes.

### Portable bytecode

Implemented 2026-10-17: `lang_bytecode::unit` promotes a verified MIR unit to a register program and implements portable schema 1 from accepted RFC 0006. Its explicit little-endian codec writes `LANGBC\0\0`, schema/flags/payload length, source/definition/import/function tables, typed registers, all twelve operations, terminators, byte spans, and optional entry. It sorts global tables canonically and remaps every corresponding type/call/import/span/entry ID; field/case/block/instruction order is retained. Decoding rejects noncanonical order, unknown versions/flags/tags, malformed UTF-8/booleans, length/trailing-data errors, and invalid typed units before returning a program. Existing prototype/component formats are unchanged.

The codec limits artifacts to 64 MiB by default. Counts and remaining-input minimums are checked before fallible reservation; source/register/block/instruction totals are cumulative. Raw strings and output bytes count against the shared 128 MiB requested-byte budget; structured vectors also count against the 8,000,000-element budget. Decoding and verification use one `Budget`. Encoding revalidates the caller's program under the requested limits, sorts with fallible budgeted comparisons, computes an exact output size before allocation, and writes explicit fields in a second pass. Reader words/tags, each UTF-8 validation chunk of up to 1 KiB, writer fields in both passes, and sorting/index members are charged in addition to verifier work. Eleven tests include fixed 88-byte golden bytes, every opcode, global-ID remapping, nonsequential register/CFG round trips, all truncations, malformed inputs, canonical reencoding, and cumulative budgets. `portable_bytecode_decoder` fuzzes raw bytes and a valid payload mutated after its header; it compiles and completed an uninstrumented 10,000-run smoke. The compilation-unit VM consumes these artifacts; this smoke does not establish instrumented fuzz coverage.

### Source formatter

Implemented 2026-10-17: `lang_syntax::format` formats actual parsed token owners and delimiter context. It distinguishes declarations, statement blocks, match bodies, and record values/patterns, uses four-space delimiter indentation, operator/annotation spacing, and one statement/member/declaration per line. Optional commas, literal spelling, and every real comment/token byte remain unchanged. Invalid trees return the complete original snapshot with `PreservedInvalid`; valid trees produce idempotent text with a final LF. The formatter uses iterative passes and a delimiter stack, fallible buffers, a 16 MiB output cap, and 10,000,000 charged node/child/token/indentation/text-chunk steps. Lookahead is indexed once rather than repeatedly scanning trivia. Nine syntax tests verify complete semantic CST shape/comment round trips, every comment-insertion boundary in a scalar fixture, malformed preservation, and resource/snapshot failures. Two source tests verify multi-file aggregate VM results/counters and scalar VM/JIT behavior after formatting. The source parser fuzz target also reparses/reformats valid output and checks node/token preservation; its 10,000-run uninstrumented smoke passed. A standalone CLI formatter command remains Milestone 4 integration.

### Script CLI

Script CLI implemented 2026-10-17: `lang run FILE` uses shared `lang::frontend` analysis/default compile limits, validates `main() -> i64`, and executes the verified unit with an empty typed host registry. The host chooses the file parent as source root unless `--source-root` is supplied. Resource diagnostics retain snapshots; raw encoding failures report stable codes/byte ranges without fabricated spans. Success prints one signed decimal and newline with exit 0; compile/runtime failures use stderr and exit 1; usage errors exit 2. Runtime errors retain counters and entered source frames. VM-limit flags and the old two-integer prototype remain available. Ten CLI/frontend tests cover these contracts, multi-file aggregates, entries, malformed input, typed-import refusal, and limits.

### Transactional REPL

Implemented 2026-10-17: `lang::repl::Session` accepts exactly one item or expression. It analyzes the complete candidate definitions and verifies library MIR before committing an item; errors preserve earlier text definitions. Expressions select a collision-free generated wrapper, execute with fresh VM counters, and print scalar or qualified/declaration-order aggregate values. No mutable values persist. `:reset` clears definitions; `:quit` or EOF exits. The CLI scans appended bytes once for nested comments and balanced parentheses/braces, bounds input before allocation, and diagnoses incomplete EOF. TTY input has primary/continuation prompts; piped input has none. Normal quit/EOF exits 0 after handled submission errors; incomplete EOF and fatal IO/input-limit failures exit 1. File modules require `--source-root`; host imports remain empty in the CLI.

Original submissions are separate immutable snapshots. A segment map redirects primary/related labels, fix edits, fault locations, and every guest call site from assembled source to those snapshots; generated wrapper names/lines are hidden in runtime display. Each report owns its sources and failed runtime's unit, so subsequent submissions/reset cannot invalidate locations. Only text definitions persist in the session. Output is iterative and capped at 65,536 bytes by default. A linear byte scan generates a wrapper name longer than every existing identifier; scanned/generated bytes are charged against the configured parse-work allowance. Compiler phases keep their own configured limits. Host effects are not rolled back by guest or display failure. Sixteen tests cover transactions, exact item counts, canonical display, source/fix/stack mapping, fresh budgets, host effects, file authority, malformed UTF-8/comments, limits, buffering, and piped CLI recovery/reset/EOF. A local TTY smoke also verified continuation and quit.

### Incremental architecture

Implemented 2026-10-17: immutable `SourceMap` clones/includes share `Arc<SourceFile>` snapshots and existing identities; new edits still get globally unique IDs. Modules retain `Arc<SyntaxTree>`. `load_preparsed` accepts a host `ModuleProvider` that resolves canonical identities under explicit read authority and supplies those parser results. It validates identical parse policy and cumulative source/token/node budgets before module installation, preserves parser-limit snapshots, and checks canonical duplicates before requesting their parse again. Module indexing now charges node/path-child visits as well as existing declaration/alias work. Six tests verify pointer-identical tree/source reuse, duplicate preflight, cumulative budgets, malformed/foreign inputs, policy mismatch, and retained diagnostic snapshots. Automatic query recipes and transport remain the final integration scope.

Implemented 2026-10-17: `lang_semantics::database` supplies automatic input → parser → module → semantic-diagnostic recipes over `lang-query`. Module construction consumes the cached CSTs through `load_preparsed`; semantic analysis actually runs name resolution and full source typing, retaining immutable diagnostics/snapshots rather than duplicate typed HIR. Ordinary edits invalidate reverse consumers and preserve unrelated entries. Open overlays win over disk, including new virtual files. Disk reads are bounded and require a canonical explicit root; no-root sessions read no files. Watched-file/discovery epochs conservatively invalidate module discovery, including missing files/symlink identity changes, while unrelated parser snapshots stay reusable. Close drops overlay input and invalidates parser caches to release shared pool capacity; closed dependencies reload from disk on demand.

Separate prepare/compute/publish APIs allow off-database work. Graph tickets reject edits, close/reopen, superseding work, and foreign graphs before replacing cached results. Default host cache limits are 256 known files, 16 MiB input bytes, 2,000,000 parser nodes, 2,000,000 module indexing nodes, 16 MiB diagnostic text, and 4,096 graph nodes/65,536 edges/10,000,000 graph-work steps. Compiler phases retain their own limits. Cache counts exclude caller-held old snapshots/jobs; hosts own their lifetimes. Twelve tests verify actual tree reuse, selective invalidation, stale-publication rejection, original incomplete-source diagnostics, close/disk fallback, missing/watched/virtual module recovery, read authority/symlink preflight, resource boundaries, and recovery after edits/close. The LSP document/protocol adapter remains the final integration step.

Use a dependency-tracked query system from the first non-trivial frontend. Queries should be deterministic and separately measurable:

- Parse file
- Expand bounded compile-time constructs
- Resolve module/import graph
- Resolve names
- Infer/check types and effects
- Check ownership and exclusivity
- Lower and optimize MIR
- Generate bytecode or native code per unit

Persist hashes and timing explanations so the build tool can answer why a unit rebuilt. Keep source files and package manifests as explicit query inputs.

Implemented 2026-10-17: `lang-query` is a backend-independent in-memory graph/cache core. Hosts supply opaque keys, immutable `Arc` values, and complete explicit dependency sets. Input edits invalidate transitive consumers while preserving unrelated cache entries. Query publication tickets capture the graph identity, query generation, and dependency revisions; edits, newer work, publication, removal, and key reuse prevent old results from replacing current values. Dependencies can be replaced without retaining obsolete reverse edges. Closing a key invalidates its consumers; incoming references remain bounded tombstones so reopening cannot revive old work. Cycle, node/edge/work-limit, and revision-overflow failures preflight bookkeeping before any graph state change. Traversals are iterative and deterministic. These bounds cover graph bookkeeping, not the host's analysis-value memory or code. A structured-input HIR integration test demonstrates diagnostic-cache reuse and stale-result rejection. The source database connects automatic query recipes and parser/module dependencies to the LSP server. Persisted hashes/timings remain future work; the graph core alone does not supply compiler integration.

## Memory model baseline

The design must acknowledge the fundamental tradeoff: memory safety, general shared object graphs, simple syntax, deterministic destruction, zero overhead, and no GC cannot all be maximized simultaneously.

The accepted ownership core is specified by RFC 0004. Parameter-passing inference, closure capture syntax, unsafe operations, allocator APIs, and freestanding profiles below are follow-up directions, not additional accepted rules:

- Plain `T` is an owned value with deterministic destruction.
- Moves are inferred; use-after-move is rejected.
- Function parameters default to non-owning reads for non-trivial values where this is unambiguous.
- Mutable access is exclusive and lexical. Ordinary code should not write lifetime parameters.
- Non-escaping borrows are inferred and cannot be stored without an explicit owning abstraction.
- `shared T` is an explicit atomic reference-counted reference; `weak T` breaks cycles.
- Regions/arenas provide bulk lifetime management for graphs, compilers, games, and request-scoped data.
- Closures state whether they borrow, move, or share captures.
- Raw pointers, unions, unchecked indexing, layout casts, and foreign ownership live behind narrow `unsafe` boundaries.
- The standard library exposes allocator-aware containers and permits no-runtime/freestanding subsets later.

Accepted [RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md) requires singly owned plain values, explicit `shared`/`weak`, rejection of cycle-closing strong shared edges before mutation, and lexical arenas for intentional cycles. This is a source/runtime implementation obligation, not a claim that the experiment is a production ownership checker.

Remaining implementation evidence includes nested cleanup, partial initialization, self-reference rejection, and the runtime/memory cost of atomic sharing, cycle preflight, and arenas. RFC 0005 already rejects every borrow across suspension; relaxing that baseline requires a new RFC.

### Cyclic graph experiment

The graph model in `crates/semantics` compares explicit shared/weak ownership and arena-scoped storage under two shared-cycle policies:

- Shared nodes begin with an external root. Strong edges retain their targets, while weak edges do not.
- The default `Reject` policy rejects a strong edge that would close a shared-reference cycle with structured diagnostic `SEM0018`; a weak edge may close the same cycle, and upgrading an expired weak edge reports `SEM0021`.
- The experimental `CollectUnreachable` policy permits strong cycles. After an external root is dropped, it traces strong edges from every remaining live root and releases unreachable shared nodes in reverse creation order; weak edges do not keep a target live.
- Nodes in one arena may form arbitrary cycles because the arena owns their lifetime. Edges across arenas or between arena and shared storage report `SEM0017`.
- Dropping an arena releases its nodes exactly once in reverse creation order. Later use of a stale node handle reports `SEM0012`.
- Every operation carries a source span, and diagnostics retain a related allocation or edge span where one exists.

RFC 0004 accepts `Reject`, weak back-edges, and lexical arenas. `CollectUnreachable` remains an unaccepted alternative. The synchronous graph experiment is not a runtime reference-count implementation: weak upgrade checks liveness but does not return a new retained owner. It does not model atomic publication, thread safety, destructor failure, or production weak-reference ordering. Those obligations require focused implementation and conformance tests.

Review outcome: **needs prototype/measurement for production implementation**, not another decision on the accepted strict policy. Milestone 2 owns source checking, shared/weak lifecycle, cycle-preflight atomicity, and arena lifetime validation.

### Async borrow and cancellation experiment

The executable model in `crates/semantics` compares the RFC 0005 strict baseline with one unaccepted, less restrictive policy:

- Task-owned values remain live in the task frame across suspension.
- The default `RejectAll` policy rejects an active shared or exclusive borrow at a suspension point with structured diagnostic `SEM0007`; a borrow that ends before suspension is accepted.
- The experimental `AllowSharedFrameBorrows` policy permits active shared borrows of task-owned values across suspension but still rejects exclusive borrows with `SEM0007`.
- Cancellation is observed only while the task is suspended.
- Under the experimental policy, resumption preserves the shared borrow, while cancellation ends active borrows before dropping task-owned values exactly once in reverse creation order.
- Every operation carries a source span so failures identify both the boundary and the borrow that crossed it.

RFC 0005 accepts `RejectAll`; `AllowSharedFrameBorrows` remains experimental. The model observes cancellation only at suspension and is not the specified task-tree runtime or an idempotent cancellation-request API. Nested scopes, partial initialization, destructor failure, panic containment, and equivalent VM/native cleanup traces remain implementation work.

Review outcome: **needs prototype/measurement for runtime implementation**. Pinned/shared/exclusive borrows across suspension are excluded from the accepted baseline, not blocking decisions that permit an implementation to choose its own behavior.

## Error and effect model

- Recoverable errors use typed result values with `throws E`/propagation syntax as surface sugar.
- Panics represent violated invariants and do not cross component or FFI boundaries.
- Native unwinding is not part of the stable ABI.
- Effects should initially track only behavior that changes safety or composition, such as `async`, `unsafe`, host capabilities, and possibly allocation. Avoid a general effect system until a prototype proves its ergonomic value.
- Cancellation is structured and propagates through task scopes; destructors and cleanup blocks must run predictably.

## Concurrency model

Combine several layers rather than forcing one abstraction:

- Structured tasks, cancellation, deadlines, and async I/O for application code.
- Actor isolation and message passing for mutable service state.
- Parallel iterators and scoped fork/join for data parallelism.
- Explicit atomics, locks, channels, and unsafe synchronization for systems code.
- Send/share capabilities derived from ownership and mutability so data races are rejected statically.

The scheduler is a replaceable runtime service. Libraries must not assume a global executor, and embedded hosts must be able to drive or integrate the scheduler.

### Shared mutable service experiment

The service model in `crates/semantics` compares the RFC 0005 owner-isolated baseline with one unaccepted shared-mutation policy:

- Under the default `OwnerIsolated` policy, one owner task has direct mutable access to service state. Shared handles may enqueue messages but direct mutation from another task reports `SEM0031`.
- Under the experimental `ExclusiveCapability` policy, any task with a valid handle may begin a mutation when none is active. A task without a handle is rejected, and a concurrent begin reports the existing structured mutation-conflict diagnostic with the active mutation span.
- Sending moves a payload into the mailbox. A non-sendable payload reports `SEM0028`, and reuse after a successful move reports `SEM0027`.
- The owner still processes messages in FIFO order. A cancellation request from any valid handle is deferred until an active mutation ends, including a non-owner mutation under the experimental policy.
- Cancellation discards queued messages in FIFO order, drops every queued payload and the service state exactly once, then rejects later sends with `SEM0024`.
- Operations retain primary and related source spans under stable diagnostic codes `SEM0022` through `SEM0035`.

RFC 0005 accepts `OwnerIsolated`; `ExclusiveCapability` remains an unaccepted lock-like alternative. The model lacks send/share derivation, task trees, repeated idempotent cancellation requests, backpressure, reentrancy, panic cleanup, and runtime scheduling. Model diagnostics are experiment evidence, not a complete source or scheduler contract.

Review outcome: **needs prototype/measurement for runtime implementation**. Milestone 3 must implement accepted isolation/cancellation and decide unspecified channel, scheduler, and error details before exposing them as source behavior.

## AOT and script execution

### Bytecode first

Use a typed register-based bytecode VM rather than an AST interpreter:

- The VM validates bytecode before execution.
- Instructions preserve language-level types and source locations where useful.
- The same runtime implements strings, collections, shared references, task primitives, and panic/error boundaries for native and script code.
- Inline caches and specialization can be added after profiling.
- JIT compilation is optional and follows stable bytecode semantics.

### Embedding API

Provide a stable C ABI embedding layer regardless of the implementation language:

- Create/destroy runtime and isolated script contexts.
- Load verified bytecode/components from memory.
- Register native functions, types, constants, and capabilities.
- Call exports by a validated function handle.
- Convert values through generated bindings or opaque handles.
- Set memory, instruction/fuel, recursion, task, and wall-clock limits.
- Interrupt execution and receive structured diagnostics and stack traces.

Cache call-site validation and generated thunks. Calls remain in-process and avoid IPC, but boundary checks and representation conversions are still real costs and must be benchmarked.

[RFC 0003](rfcs/0003-versioned-component-contracts.md) specifies the first executable component-contract slice. The runtime now validates schema-versioned package metadata with named scalar imports/exports, conservative identifiers, explicit capability requirements, and embedding-supplied logical-name-to-host-function bindings. Portable metadata does not contain process-local `HostFunctionId` values. Validation rejects structural conflicts and unsupported import types, rejects bindings for undeclared imports, checks capability grants before host lookup, resolves every import against the typed registry, and returns a validated wrapper carrying the resolved local bindings. A deterministic experimental `LCMP` schema-1 codec encodes the scalar declaration subset and decodes untrusted bytes under explicit total-byte, declaration-count, structural-depth, and string-length limits; malformed-input errors carry byte offsets, and a libFuzzer target exercises the decoder. This schema-1 codec remains an unstable fixture rather than the accepted candidate schema-2 serialization, a WIT implementation, a component-IDL resource declaration, canonical ABI, or an execution path for component exports. The subsequent prototypes below cover the accepted candidate serialization and selected resource/adapter path, while production IDL and generated adapters remain downstream work.

A separate candidate `LCMP` schema-2 envelope now defines a contract document (`kind 0`) and a versioned failure-report document (`kind 1`). Contracts deterministically encode a package key and full semantic version, sorted normalized interface requirements and required-member projections, SHA-256-tagged exact locks, and sorted canonical resource-type identities. Required members preserve parameter/result order, exact capability, and `i64`, `bool`, `own<T>`, or `borrow<T>` value types with complete nominal resource keys. Failure reports preserve one primary record and ordered suppressed cleanup failures, encode poisoned state, use fixed category/phase/diagnostic tags, and distinguish message redaction from embedding-sanitized transport. A policy-explicit conversion projects runtime `BoundaryFailure` records while preserving cleanup invariant traps and destructor-failure order. Construction rejects invalid identifiers or capabilities, duplicate entries or member names, invalid intervals, undeclared resource references, borrowed results, inconsistent failure classifications, non-cleanup suppressed records, unsafe redaction states, unpoisoned destructor failures, orphan locks, and lock-version mismatches. Bounded decoders reject wrong schema/document kind, malformed tags or booleans, truncation, invalid UTF-8, trailing bytes, and exceeded byte/item/depth/string limits with byte offsets. Checked-in golden encodings, round-trip/canonical-order tests, semantic validation tests, deterministic arbitrary-byte coverage, and dedicated fuzz targets protect both layouts. Accepted RFC 0003 adopts schema 2 as the implementation baseline, while it remains versioned design evidence rather than a stable public production format.

The component runtime also has an in-memory generation-tagged resource table. Copyable owned handles model integer handles that untrusted callers can duplicate; releasing a resource advances its generation before the lowest free slot can be reused, or retires the slot if the generation is exhausted, so stale copies cannot alias a replacement. Shared borrows may coexist, exclusive borrows require no active borrow, mutable access requires a live exclusive token, and release rejects every live borrow. Borrow tokens are monotonic and explicitly ended; stale tokens, repeated ends, repeated releases, conflicting borrows, and identifier exhaustion are structured errors. This proves table-local lifecycle checks, not serialized handle layout, cross-component ownership transfer, destructor behavior, thread safety, or canonical resource lowering.

The runtime now also prototypes package/interface compatibility negotiation independently from schema-1 serialization and production package-manager resolution. Normalized semantic versions preserve prerelease and build identifiers without machine-word-bounding numeric prerelease identifiers. Exact requirements compare the complete value; intervals use SemVer precedence and an explicit prerelease policy. Capability grants are checked before provider inventory validation. Exact lock inputs require one identity, version, and opaque digest without fallback; otherwise candidates are filtered by exact keys and structural required-member projection before deterministic highest-precedence selection. The projection includes ordered scalar and nominal owned/borrowed resource types plus exact capabilities, while allowing unrequested provider members. Same-precedence build/digest ties are ambiguous, and successful resolution returns an immutable binding. A canonical in-memory lockfile records complete exact SHA-256 selections, replays without fallback, reconciles embedded contract locks, and moves the selected contract and bindings directly into fresh instance state. No on-disk lockfile syntax, registry access, artifact download, or publication workflow is defined yet.

An executable schema-1 scalar fixture now connects validated component metadata to all three prototype execution paths. Capability and logical-name bindings are validated against the typed host registry before a selected `i64 -> i64` import/export pair is lowered to MIR. VM and JIT execute the same function with equivalent wrapping results, including integer extremes. The same validated fixture emits a host-native object whose export and resolved host import link and execute on macOS AArch64 through direct and response-file paths. This is narrow differential evidence for one scalar contract, not a general component loader, canonical resource adapter, failure boundary, stable serialization, or cross-platform component runtime.

A backend-neutral canonical-resource adapter trace now makes the RFC 0003 ownership and failure ordering executable without exposing runtime handle layout. Per-instance boundary indices are non-zero, monotonically allocated, and never reused. Whole-call preflight validates every resource argument and destination limit and reserves storage before an allocation-free, callback-free commit moves owners or publishes call-scoped borrows. Cleanup closes nested borrows inside-out, removes remaining frame owners before invoking destructors in reverse acquisition order, continues after independent destructor failures, preserves an earlier panic/trap as primary, and poisons the callee after callback, invariant, or destructor failure. A shared host-value bridge allocates opaque identities, maps them to generation- and table-provenance-checked `ResourceTable<T>` owners, rolls back values when canonical publication fails without reusing the identity, resolves runtime borrow operations, and lets cleanup consume the actual value exactly once. General instantiation negotiates every canonically ordered schema-2 requirement before publishing an instance containing immutable bindings and fresh bounded adapter state. A conformance fixture feeds the VM, JIT, and macOS AOT scalar result through the same owned transfer, guest-trap cleanup, and redacted failure projection; value, state, and report outcomes agree. The later straight-line nominal `own<T>` lane now runs through MIR, bytecode, VM, JIT, and tier-1 AOT; general resource control flow and return ownership adapters remain unimplemented. Asynchronous cancellation also remains open.

The VM prototype implements three composable opt-in limits—instruction fuel, a host-call budget, and a value-slot limit—plus an optional shared one-way interruption flag and absolute monotonic deadline. Each bytecode instruction and terminator consumes one fuel unit, and each entered host callback consumes one host-call unit; validation and arity checks occur before metering, and each dynamic limit is checked before the corresponding side effect. The VM compares the verified program's register count with the value-slot limit before allocating the register file. An already-set interruption or expired deadline stops execution before that allocation; subsequent safe points are the start of each instruction and terminator and the boundary immediately before an entered callback. The structured termination errors report instruction and host-call consumption at the observed safe point, and interruption takes precedence if both conditions hold. A callback can set the flag or overrun the deadline, but its internal work completes before the next VM safe point observes the condition. The limited API returns the value, both consumed counts, and the static slot footprint, while existing convenience APIs remain unbounded, non-interruptible, and without a deadline. A non-terminating back-edge test, a finite loop-carried countdown test, a zero-budget callback test, an undersized slot-limit test, pre-execution/callback-triggered interruption tests, and pre-execution/running-loop deadline tests demonstrate loop termination, callback suppression, pre-allocation rejection, and safe-point accounting. A slot is a backend-defined VM register entry, not a byte count, and does not account for host-callback allocations. These controls do not yet cover the heap, recursion, tasks, preemption inside callbacks, or native/JIT execution, so they are evidence for the embedding interface rather than a complete sandbox.

The initial two typed `i64` callbacks exercise host-owned allocation. The first performs one `Box` allocate/read/drop round trip. The second creates 64 boxes containing distinct wrapping values, keeps all 512 payload bytes live, sums them, and then drops them. VM and JIT dispatch to the Rust implementations, while linked AOT objects import corresponding C `malloc`/read/`free` implementations; macOS direct and response-file integration execute both wrappers. Benchmark schema 8 measures direct Rust, VM, and JIT latency for the single-allocation callback, and schema 11 adds the same three rows for the burst. Process-startup rows remain unchanged because callback allocation work would be dominated by process creation and loading. This proves that all three execution paths can cross the typed boundary for bounded allocation-backed operations and provides a less trivial VM/JIT boundary-cost workload. It is not a language heap, allocator ABI, ownership rule, garbage collector, memory limiter, allocator-overhead account, or representative application workload, so those design and cost questions remain unresolved.

Historical schema-by-schema timing, allocation, and process-memory results are maintained in [benchmark evidence](benchmarks.md), not as current architecture requirements. Native Windows evidence is recorded by schema 20 and run 38932964398. The records predate the 2026-10-17 JIT reclamation fix; fresh clean measurements are required before current compile/application performance claims.

### Plugin trust tiers

1. **Trusted native:** AOT object/static library, specialized calls, maximum performance, no sandbox claim.
2. **Verified script:** bytecode VM with capabilities and resource limits.
3. **Portable sandboxed component:** WebAssembly Component Model/WIT-compatible boundary where portability and isolation matter more than absolute call overhead.

User-authored plugins default to tiers 2 or 3. Native plugins are an explicit trust decision.

## Component ABI and multiple languages

Do not use the native source-language ABI as a compatibility promise. Define a separate interface language inspired by WIT:

- Records, variants, enums, flags, resources/handles, lists, strings, options, results, futures, streams, and functions.
- Explicit ownership transfer and borrowed/owned parameters.
- Package/interface semantic versions and compatibility checks.
- Capability imports and exported worlds.
- Generated adapters for native code, VM bytecode, C, and WebAssembly.

Inside a whole-program build, the compiler may specialize adapters into direct calls after proving compatible layouts and ownership. Independently versioned components retain the canonical boundary.

Future frontends should lower first to their own HIR, then to the shared typed MIR only when their semantics match. Frontend-specific features that cannot be represented safely should lower through the component interface instead of expanding the core MIR indefinitely.

## Native backends

### Initial targets

- Tier 1 prototype: macOS AArch64 and Windows x86-64.
- Next targets: macOS x86-64 and Windows AArch64.
- Object formats: Mach-O and COFF/PE.
- C ABI interoperability is required before a custom ABI.

### Backend strategy

- Use Cranelift as the initial native JIT and AOT backend for the first implementation.
- Keep backend APIs narrow and target-neutral.
- Measure release runtime, compile latency, code size, debug quality, and platform integration on representative programs.
- Add an LLVM backend if Cranelift cannot meet release performance, optimization, or debug requirements.
- Use system linkers initially; evaluate bundled LLD only when reproducibility and installation data justify it.

**Decision (2026-10-17):** Cranelift 0.136.3, as resolved by the workspace lockfile, is the initial native backend for both JIT and AOT. This is an implementation baseline, not a stable native ABI, an exclusive permanent backend, or a language-level performance guarantee. The frontend, typed MIR, source maps, linker requests, and artifact interfaces remain backend-neutral; add an LLVM backend later only if representative release workloads expose unmet optimization, target, or debug requirements.

| Criterion | Recorded evidence | Disposition |
| --- | --- | --- |
| Semantic and ABI integration | Differential VM/JIT coverage plus macOS and Windows direct/response-file AOT execution for integer, CFG, callback, scalar-component, and one typed-owner lane | Sufficient for the initial backend |
| Tier-1 portability | Native macOS AArch64 and Windows x86-64 object-link-run CI, with target-neutral linker policy and explicit triples | Sufficient for both prototype Tier-1 targets |
| Debug quality | Source-mapped Mach-O DWARF/dSYM and COFF CodeView/PDB inspection for representative artifacts | Sufficient for the first implementation; locals, unwind metadata, optimized stepping, and JIT debugger registration remain later criteria |
| Performance and memory evidence | Historical clean schema-20 macOS and Windows baselines | Directional evidence only; JIT reclamation was fixed after these captures, so fresh clean measurements are required for current compile/application performance claims |
| Architecture cost | The existing `codegen-clif` crate exposes no Cranelift types through frontend APIs and already supplies JIT and object emission | Lower implementation and maintenance risk than introducing a second backend now |

The current backend emits either in-process JIT code or an object artifact from the same validated MIR lowering without exposing Cranelift or object-library types to callers. Typed blocks, explicit terminators, block parameters, source spans, imports, and exports share the same backend-neutral contracts across the VM, JIT, and AOT paths. The shared `SystemLinker` models `Cc` and `MsvcLink`, validates ordered requests, supports direct and response-file invocation, and records structured failures. macOS integration and [Windows run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398) execute both link modes natively; the clean [schema-20 benchmark evidence](benchmarks.md) records both Tier-1 platforms. Production installation discovery, startup/runtime packaging, stable ABI adapters, and additional targets remain separate work.

A clean pre-implementation macOS AArch64 [debug-information inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) established the original negative baseline. The committed [source-mapped DWARF inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md) demonstrates that typed MIR carries a structurally validated source file, function span, and one source span for every instruction and terminator; Cranelift preserves them as backend-neutral JIT/AOT machine-code ranges; and Mach-O serialization emits a relocated DWARF v4 compile unit, subprogram entry, and line table. The representative add, minimum, and host-callback objects pass `dwarfdump --verify`; direct and response-file executables link and run; and both generated dSYMs pass verification while retaining the fixture path and all three functions. The committed [Windows x86-64 CodeView/PDB inspection](../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md) validates equivalent C13 symbols, types, line tables, and relocations in COFF, links them into a PE/PDB pair with LLD, and verifies function-to-source resolution with LLDB. This is cross-target debug-format evidence rather than a native Windows run. Source-language locals, unwind metadata, JIT debugger registration, and optimized stepping remain separate acceptance work; backend selection must include their cost and quality.

Host linker discovery now searches `PATH` in deterministic candidate order, requires executable files on Unix, and returns a structured diagnostic containing the searched path and candidate names when no driver is available. The macOS integration uses the discovered driver, and the cfg-gated MSVC integration will exercise the same discovery after the developer environment initializes `PATH`. Installation discovery for Visual Studio, SDKs, and compilers outside such an environment remains separate work.

The root crate now expresses the initial tier-1 linker policy as a backend-neutral `LinkTarget` domain type. Exact `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` triples determine command flavor, default program, and ordered `PATH` candidates; unsupported triples remain structured errors instead of silently inheriting host policy. Explicit-target linker construction, discovery, platform integration, and the cross-target COFF test share this mapping. Supporting another target requires an explicit policy addition; installation discovery and target-specific startup/runtime inputs remain separate layers.

The historical backend experiments and their original evidence gaps remain documented in [benchmark evidence](benchmarks.md). They do not reopen the accepted source rules or completed Windows baseline.

### Whole-unit copy-value lowering

Implemented 2026-10-17: `lang-codegen-clif::program` lowers the complete RFC 0006 copy-value operation set from verified compilation units into JIT code or one native object. All guest functions are declared before bodies, supporting forward and mutual recursion, bool parameters/results, nested records/enums, field projection, ordered patterns, and aggregate loop joins. A private pointer/cell convention carries arguments and results; it is neither a stable source ABI nor a C export interface. Values retain nominal identities, checked bool/tag/header/padding invariants, and zero-initialized inactive enum storage. Every block edge reads all operands before writing destinations. Invalid enum projection returns a status that propagates through guest calls before later effects.

Compilation revalidates MIR with cumulative budgets, preflights all host bindings, charges expanded cell copies and temporary buffers, and rejects owned registers, layouts deeper than 64, or frames plus outgoing arguments above 1 MiB. These are explicit native compiler capacity limits. Cranelift's internal allocation is a trusted compiler boundary. JIT invocation validates canonical input buffers and holds the executable owner throughout the call; dropping the owner releases executable mappings. Native recursion and host callbacks remain trusted synchronous execution without VM fuel/frame/interruption enforcement.

Native ranges preserve original source IDs and byte spans for every emitted instruction/terminator, including multiple modules. Emitting complete source-level DWARF/CodeView for this new internal convention remains the separate Milestone 2 debug task. `tests/native_program.rs` covers portable-bytecode round trips, VM/JIT result equality, recursion, composite inputs/results, literal/structural patterns, evaluation order, self-edge swaps, invalid projections/inputs, compile budgets/frame capacity, both Mach-O/COFF objects, and cfg-gated direct/response-file object link/run. macOS AArch64 execution is verified locally; new Windows x86-64 execution evidence remains pending.

Sources: accepted [RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md), MIR layout/verifier and native implementation/tests, all updated/verified 2026-10-17 JST. Locked Cranelift 0.136.3 installed source supplies stack-slot, memory-flag, module, and executable-release APIs; no publication/update date is stated in those source files, verified 2026-10-17 JST. Cell layout and capacity limits here are project implementation choices.

## Diagnostics, LSP, and debugging

Diagnostics are structured values containing stable codes, primary/secondary spans, notes, fix-its, and machine-readable fields.

Implemented 2026-10-17: `lang-diagnostics` owns immutable UTF-8 snapshots, globally distinct snapshot IDs, half-open byte spans validated at character boundaries, one-based Unicode scalar positions, and zero-based UTF-16 columns for IDE adapters. Diagnostics require one primary label and may include cross-file secondary labels, notes, and fixes marked machine-applicable or speculative. Fix previews validate every referenced snapshot and reject overlapping or ambiguous edits before returning changed text; they do not mutate files. Plain-text rendering is deterministic and escapes control and directional-formatting characters. RFC 0002 codes `SRC0001`–`SRC0007` are represented; structured HIR emits `SRC0002`–`SRC0007`, and the source frontend supplies literal/parser producers and text-connected diagnostic conformance through `SRC0019`. This crate introduces no source grammar or serialized diagnostic format.

The LSP reuses the compiler’s parser, resolver, type checker, source map, and incremental query database. It must remain responsive with incomplete and invalid programs.

Implemented 2026-10-17: `lang-lsp` converts the existing structured diagnostic model to LSP diagnostic data for a UTF-16 client profile. Source byte spans are converted independently of compiler line numbers: LSP recognizes LF, CRLF, and lone CR, counts zero-based UTF-16 positions, and uses exclusive range ends. A byte boundary inside CRLF canonicalizes to the preceding line's content end. The adapter validates source spans and coordinate bounds, preserves source codes/severities and related labels, and includes primary-label details/notes in plain diagnostic text. Hosts supply already validated document URIs; source display names never infer URI or filesystem access. Explicit source-byte, label-count, note-count, and output-byte limits bound conversion. Fixes stay in the compiler diagnostic for a later code-action adapter.

Implemented 2026-10-17: `lang lsp` is a synchronous stdio LSP 3.18 prototype over the shared database. It implements initialize/initialized/shutdown/exit, explicitly returns mandatory UTF-16, advertises full document synchronization and document formatting, and filters related information and diagnostic versions against client capabilities. Increasing document versions update immutable overlays before query evaluation/publication; obsolete versions are ignored. The database rejects stale jobs after edits, close/reopen, or newer work. Close clears diagnostics, releases overlay authority, and refreshes remaining roots from disk when authorized. Open imported files use the parent module context; diagnostics in closed dependencies attach to the open root's module declaration, with the original location as related information when supported. Missing modules recover through open buffers or watched-file notifications. Dynamic watcher registration is requested only when supported.

Initialize grants one canonical local source root, or no filesystem reads when absent. Local file URIs are validated/percent-decoded; nonlocal authorities, NUL, parent traversal, and outside-root/symlink escapes are rejected before source reads. Stdio framing uses ASCII CRLF headers and UTF-8 byte lengths, rejects duplicate/invalid lengths and truncated frames, and bounds headers to 8 KiB and messages to 16 MiB before allocating payloads. JSON parsing retains its nesting limit. Invalid JSON and unsupported charsets report parse errors; ambiguous framing terminates the connection. Outbound serialization is bounded before writing a frame. The server supports no JSON-RPC batches, completion, navigation, code actions, or DAP. Formatting returns a whole-document canonical four-space edit and preserves malformed input. Twelve integration tests cover actual stdio processes, lifecycle/capabilities, UTF-16, version/close behavior, module overlays/watchers, input/authority boundaries, framing, malformed JSON, and formatting.

Transport/lifecycle sources: official Microsoft [LSP 3.18 specification](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/specification.md), [initialize](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/general/initialize.md), [didChange](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/textDocument/didChange.md), [shutdown](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_includes/messages/3.18/shutdown.md), and [exit](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_includes/messages/3.18/exit.md). These living documents state no publication/update date; verified 2026-10-17 JST. [JSON-RPC 2.0](https://www.jsonrpc.org/specification) was published 2010-03-26 and last updated 2013-01-04; verified 2026-10-17 JST. Protocol requirements are sourced facts; selected features, synchronous scheduling, routing, and resource caps are project implementation decisions.

Protocol sources: Microsoft LSP 3.18 [text documents](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/textDocuments.md), [positions](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/position.md), [ranges](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/range.md), [diagnostics](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/diagnostic.md), and [base integer bounds](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/specification.md#baseTypes). These living documents do not publish an update date in their content; verified 2026-10-17 JST against the official repository. The adapter follows these external position/data requirements; its resource limits and staged integration are project implementation choices.

The debug stack has three layers:

- Native debug metadata: DWARF on macOS, CodeView/PDB on Windows.
- VM debug metadata: bytecode/source maps, lexical scopes, variable locations, and async task frames.
- A DAP adapter that merges native and VM frames, supports mixed breakpoints, and labels optimized/unavailable values honestly.

Source spans and stable symbol/function identifiers must survive every lowering stage. Debuggability is part of backend acceptance criteria.

## Toolchain UX

Use one provisional umbrella command until naming is settled:

```text
lang new       create a package
lang build     build incrementally
lang run       build and run, or execute a script
lang test      run unit, integration, and documentation tests
lang fmt       canonical formatting
lang lint      semantic and style diagnostics
lang doc       generate API documentation
lang lsp       start the language server
lang debug     start or connect the DAP adapter
lang package   manage dependencies and publication
lang toolchain install|list|default
```

The package manager needs a manifest, exact lockfile, checksums, offline mode, source replacement/mirrors, feature resolution, target conditions, and reproducible build metadata before a public registry.

## Self-hosting

1. Stage 0 Rust compiler implements the full accepted core semantics.
2. A deliberately small self-hosting subset is written in the new language.
3. Stage 0 builds stage 1; stage 1 builds stage 2.
4. Stage 1 and stage 2 outputs are compared under reproducible settings.
5. The Rust compiler remains a bootstrap reference until multiple platforms pass conformance and reproducibility checks.

Do not fork semantics between the Rust and self-hosted compilers. Both consume the same conformance suite, serialized test cases, and component ABI fixtures.

## Proposed Rust workspace boundaries

Create these crates only as implementation appears:

```text
crates/
  syntax        lossless tree, parser, source map
  diagnostics   stable diagnostic model
  hir           resolution and typed high-level IR
  semantics     inference, traits, effects, ownership
  mir           typed mid-level IR and verification
  bytecode      format, encoder, decoder, verifier
  vm            embedded runtime and interpreter
  codegen       backend-neutral lowering interface
  codegen-clif  Cranelift backend
  runtime       native runtime services
  component     IDL, canonical ABI, bindings
  lsp           language server
  dap           debug adapter
  cli           toolchain command
```

Avoid cyclic crate dependencies. Data-oriented IR crates should not depend on CLI, LSP, VM, or backend crates.

## Implementation review (2026-10-17)

The current implementation preserves MIR source maps in bytecode and native ranges, records actual CodeView parameter/result types, rejects NUL function names and collisions with host-import symbols, and owns JIT executable mappings until the compiled function is dropped. Its narrow unsafe release is justified by private code pointers and invocation borrowing the owner; error paths release unpublished mappings too.

Component validation checks malformed directly constructed SemVer values at every boundary and rejects borrowed results. All interface capabilities are preflighted before any provider/lock lookup. Both decoder schemas check that declared collections can fit the remaining payload before reserving storage. These checks do not authenticate artifacts: selected digests are supplied metadata until a future loader verifies actual artifact bytes.

Rust adapter cleanup contains destructor-error formatting/destruction and panic-payload destruction failures, continues independent cleanup, and poisons both instances when an active call is abandoned. Abandonment cannot run host destructors without the host callback; the host must tear down quarantined values. Recursive panic-payload destruction may intentionally leak the secondary payload. Abort-mode panics, foreign unwinding, and direct native callbacks are outside this containment boundary.

Response files are exclusively created and retained; callers must use fresh paths on retries. Existing files and symlinks are not overwritten. Linker configuration and native callbacks are trusted host inputs, not script capabilities.

The subsequent whole-repository review also binds runtime resource handles to their originating table, validates source-root authority at every public database query before cache lookup or disk reads, and preserves option-like relative linker filenames in both direct and response-file commands. Rejected oversized document opens do not consume file slots; authorized open overlays remain usable after disk deletion or replacement, and closing them revokes that authority. See the [dated review](review-2026-10-17.md) for reproduction, coverage, and validation.

Sources: locked Cranelift JIT 0.136.3 [release-memory contract](https://docs.rs/cranelift-jit/0.136.3/cranelift_jit/struct.JITModule.html#method.free_memory), verified against the installed versioned source on 2026-10-17 (source publication/update date not stated); Microsoft [CodeView type definitions](https://github.com/microsoft/microsoft-pdb/blob/master/include/cvinfo.h), living source without a displayed publication date, verified 2026-10-17. Repository implementation and regression tests updated 2026-10-17.

## Required validation

- Parser/formatter round trips and malformed-input recovery.
- Compile-pass and compile-fail diagnostics with stable error codes.
- MIR and bytecode verifier tests.
- Interpreter/native differential tests.
- Cross-version ABI and package compatibility fixtures.
- Fuzzing for parsers, manifests, bytecode, debug data, and component metadata.
- Miri/sanitizer runs for the Rust implementation where practical.
- Windows/macOS CI and representative benchmarks.
- Bootstrap stage comparison before claiming self-hosting.

