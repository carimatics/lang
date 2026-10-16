<!-- canonical-language: en; japanese-translation: ja/roadmap.md -->

# Implementation roadmap

[日本語版](ja/roadmap.md)

The roadmap is ordered by risk reduction. Milestones are exit criteria, not calendar commitments.

## Task tracking

Tasks are listed in dependency-aware priority order within each milestone. A milestone can exit only when every `P0` task is checked and its exit evidence is recorded. Any unfinished `P1` task must be explicitly moved and re-estimated before exit; it cannot silently disappear. A checked task is complete; prototype groundwork completed in an earlier milestone does not complete a broader task in a later milestone.

Cost uses relative effort points rather than elapsed-time promises: `1` is a very small change, `2` is small, `3` is medium, `5` is large, and `8` is very large. Each unchecked task has one independently verifiable outcome; larger programs are split at dependency or acceptance boundaries. Progress is `completed points / total points × 100`, rounded to one decimal place. Long-horizon estimates are planning values and must be re-estimated when their milestone becomes current or when scope or evidence changes.

The 2026-10-16 whole-roadmap audit corrected three structural problems. Production work that requires the source frontend was incorrectly included in the design baseline; several tasks combined independently shippable outcomes; and later milestones omitted required registry, bootstrap, and interoperability work. The revised list preserved the 80 points of evidence completed at that audit, moved production hardening to its dependent milestone, and exposed previously hidden work rather than treating a large compound item as one task. A follow-up component-workstream audit separated in-memory compatibility negotiation, scalar execution-prototype integration, backend-neutral resource-adapter behavior, and boundary-cost measurement. The previous single cross-mode scalar/resource task mixed those acceptance boundaries and overlapped Milestone 3's production IDL and adapter implementation. A post-schema-18 audit found that native debug-metadata implementation and cross-platform debug-quality validation were named as backend-selection prerequisites only in prose; they are now explicit 5-point and 3-point tasks before backend selection. Decision tasks that require repository-owner acceptance remain unchecked and are ordered after their evidence prerequisites. The checklist records dependency order, not a claim that an external gate is locally executable: when an owner decision or remote runner is unavailable, work proceeds to the next unblocked P0 without changing priority.

An RFC-readiness audit on the same date corrected another planning gap: RFC 0003 was queued for an owner decision even though its review outcome then named stable serialization, host-value and cross-mode adapter conformance, lock integration, native Windows component execution, and broader cost evidence as blockers. Those outcomes became explicit prototype tasks before the RFC 0003 decision; Milestones 3 and 4 still own the production IDL, adapters, and package manager.

Progress snapshot verified against this repository state on 2026-10-16 21:53 JST:

| Milestone | Completed | Total | Progress |
| --- | ---: | ---: | ---: |
| 0 — design baseline (current) | 153 pt | 196 pt | **78.1%** |
| 1 — frontend and bytecode vertical slice | 0 pt | 65 pt | 0.0% |
| 2 — ownership and native AOT | 0 pt | 74 pt | 0.0% |
| 3 — concurrency and embedding | 0 pt | 86 pt | 0.0% |
| 4 — developer toolchain | 0 pt | 63 pt | 0.0% |
| 5 — ecosystem and compatibility | 0 pt | 65 pt | 0.0% |
| 6 — self-hosting | 0 pt | 74 pt | 0.0% |
| 7 — additional frontends | 0 pt | 55 pt | 0.0% |
| **Overall** | **153 pt** | **678 pt** | **22.6%** |

## Milestone 0: design baseline

**Progress: 153 / 196 pt (78.1%).**

### Workstream breakdown

This table aggregates every Milestone 0 checklist item exactly once; it is a reporting view and does not add scope.

| Workstream | Completed | Total | Progress |
| --- | ---: | ---: | ---: |
| Project framing and source semantics | 8 pt | 10 pt | 80.0% |
| Cross-mode compiler/runtime semantics | 21 pt | 21 pt | 100.0% |
| Resource control, ownership, and concurrency | 26 pt | 30 pt | 86.7% |
| Component and resource contracts | 50 pt | 71 pt | 70.4% |
| Platform and backend evidence | 48 pt | 62 pt | 77.4% |
| Architecture-baseline disposition | 0 pt | 2 pt | 0.0% |
| **Milestone 0** | **153 pt** | **196 pt** | **78.1%** |

Current execution state at this snapshot:

- **Unblocked local P0:** cross-mode host-value adapter conformance, a lockfile/package-selection prototype, and expanded boundary-cost evidence for RFC 0003.
- **External gates:** the RFC 0002, RFC 0004, and RFC 0005 repository-owner decisions plus remote Windows object-link-run, benchmark, and component-execution evidence.
- **Dependent gates:** decide RFC 0003 after its added evidence prerequisites, select the initial native backend after the Windows evidence is recorded, then decide RFC 0001 after all other prerequisites are complete.

### Tasks in priority order

- [x] **P0 — Draft the project charter, goals/non-goals, terminology, RFC process, and compatibility labels.** — 3 pt
- [x] **P0 — Build and verify the typed `i64` MIR → bytecode/VM vertical slice.** — 5 pt
- [x] **P0 — Lower the same typed MIR through Cranelift JIT and host-native object emission.** — 5 pt
- [x] **P0 — Validate typed host registration and callbacks across VM, JIT, and AOT object contracts.** — 3 pt
- [x] **P0 — Validate typed CFG reachability, dominance, branches, and back-edges across execution modes.** — 5 pt
- [x] **P0 — Carry typed block arguments and loop values consistently through MIR, bytecode, VM, JIT, and AOT.** — 3 pt
- [x] **P0 — Establish shared linker requests, tier-1 target policy, Windows MSVC workflow, and host-independent COFF contracts.** — 5 pt
- [x] **P0 — Record macOS AArch64 link/run evidence and exercise direct and response-file paths.** — 3 pt
- [x] **P0 — Prototype deterministic VM instruction, host-call, and value-slot limits.** — 5 pt
- [x] **P0 — Prototype cooperative interruption and absolute execution deadlines.** — 3 pt
- [x] **P0 — Validate versioned scalar component metadata and explicit capability-gated imports against the typed host registry.** — 5 pt
- [x] **P0 — Prototype generation-tagged owned/borrowed resource handles and deterministic lifecycle validation.** — 3 pt
- [x] **P0 — Prototype strict and less-restrictive async-borrow and cancellation policies.** — 3 pt
- [x] **P0 — Prototype strict and tracing alternatives for cyclic graph ownership.** — 3 pt
- [x] **P0 — Prototype owner-isolated and exclusive-capability shared-service policies.** — 2 pt
- [x] **P0 — Draft decision-ready source-level integer and structured-control-flow semantics in RFC 0002.** — 5 pt
- [x] **P0 — Build the versioned benchmark harness and record compile, runtime, startup, and artifact-size baselines.** — 8 pt
- [x] **P0 — Record deterministic logical-heap profiles and linked-process memory baselines.** — 5 pt
- [x] **P0 — Exercise variable-size host allocation across direct Rust, VM, JIT, macOS AOT, and Windows object contracts.** — 3 pt
- [x] **P0 — Record AArch64 generated-code inspection and the current negative native debug-information baseline.** — 3 pt
- [ ] **P0 — Decide RFC 0002, recording acceptance, requested revisions, or rejection.** — 2 pt
- [ ] **P0 — Record successful Windows x86-64 CI object-link-run evidence.** — 3 pt
- [ ] **P0 — Record and interpret a clean Windows benchmark baseline against the macOS baseline.** — 5 pt
- [x] **P0 — Prototype a bounded schema-1 component encoder/decoder with byte/count/depth/string limits, location-bearing diagnostics, malformed fixtures, and a fuzz target.** — 5 pt
- [x] **P0 — Specify canonical scalar/resource representation and ownership transfer in RFC 0003.** — 5 pt
- [x] **P0 — Specify panic and destructor failure behavior at component boundaries in RFC 0003.** — 3 pt
- [x] **P0 — Specify package/interface version compatibility negotiation in RFC 0003.** — 3 pt
- [x] **P0 — Prototype deterministic package/interface negotiation over an in-memory candidate set and exact lock input.** — 5 pt
- [x] **P0 — Connect one validated schema-1 scalar import/export through the VM, JIT, and AOT prototypes and record equivalent outcomes.** — 3 pt
- [x] **P0 — Prototype canonical resource preflight/commit, borrow scoping, cleanup ledgers, and instance poisoning in a backend-neutral adapter trace.** — 5 pt
- [x] **P0 — Measure metadata validation/negotiation, scalar dispatch, and resource lifecycle costs as separate benchmark series and document their interpretation in RFC 0003.** — 5 pt
- [x] **P0 — Prototype and specify a stable versioned component serialization for normalized requirements, digests, resource identities, and failure records.** — 8 pt
- [ ] **P0 — Prototype general component instantiation and connect canonical host-value resource/failure adapters through VM, JIT, and AOT, recording equivalent outcomes.** — 8 pt
- [ ] **P0 — Prototype deterministic lockfile/package-selection integration over the in-memory resolver without implementing the production package manager.** — 5 pt
- [ ] **P0 — Record native Windows component execution for the same conformance fixture.** — 3 pt
- [ ] **P0 — Extend component-boundary cost evidence to instantiation, failure/destructor paths, memory, and code size, and document the interpretation in RFC 0003.** — 5 pt
- [ ] **P0 — Decide RFC 0003, recording acceptance, requested revisions, or rejection.** — 3 pt
- [x] **P0 — Draft a decision-ready ownership and cyclic-graph RFC from the completed experiments.** — 5 pt
- [x] **P0 — Draft a decision-ready async-cancellation and shared-service concurrency RFC from the completed experiments.** — 5 pt
- [ ] **P0 — Decide the ownership and cyclic-graph RFC.** — 2 pt
- [ ] **P0 — Decide the async-cancellation and shared-service concurrency RFC.** — 2 pt
- [x] **P0 — Time and profile the variable-size allocation callback, including exact requested payload.** — 5 pt
- [x] **P0 — Measure a representative allocation workload, including allocator overhead and fragmentation.** — 5 pt
- [x] **P0 — Add one additional representative CFG workload to the cross-mode benchmark set.** — 3 pt
- [x] **P0 — Implement source-span-driven native debug metadata for representative AOT objects and linked artifacts on macOS AArch64.** — 5 pt
- [x] **P0 — Record equivalent Windows x86-64 CodeView/PDB and source-mapping evidence for the same representative artifacts.** — 3 pt
- [ ] **P0 — Select the initial native backend from the completed cross-platform performance, debug-quality, and integration evidence, and record the decision.** — 3 pt
- [ ] **P0 — Decide RFC 0001 after its component, resource-control, and backend-evidence prerequisites, recording acceptance, requested revisions, or rejection.** — 2 pt

### Evidence and current state

The first component-contract slice now separates portable declarations from embedding-local bindings. Schema-1 in-memory metadata records package identity and version plus named scalar imports/exports; each import declares a capability, while a separate binding table maps its logical name to a process-local `HostFunctionId`. Validation rejects unknown schemas, invalid or duplicate names, import/export collisions, unsupported import types, undeclared or missing bindings, absent grants, and unregistered host functions. Capability checks precede registry lookup so an unauthorized component cannot probe host inventory, and successful validation returns a distinct wrapper with resolved bindings. Seven runtime tests cover the accepted path and key rejection/order contracts.

The experimental schema-1 `LCMP` codec now deterministically encodes the scalar declaration subset and decodes untrusted bytes under configurable total-byte, declaration-count, structural-depth, and string-length limits. It rejects malformed magic, unsupported schemas before their payload, truncation, invalid UTF-8, unknown scalar tags, trailing bytes, and exceeded bounds with structured byte-offset errors. Round-trip, encoder/decoder limit, malformed-fixture, deterministic arbitrary-byte corpus, locked fuzz-target build, and a 1,000-run libFuzzer smoke execution pass. The format remains an unstable fixture; stable contract serialization and compatibility negotiation are still open in RFC 0003.

The completed schema-2 serialization prototype defines separate contract and failure-report documents under one versioned `LCMP` envelope. Contract documents encode package/full-SemVer identity, canonically sorted normalized interface requirements and required-member projections, SHA-256-tagged exact locks, and canonical resource-type identities. Required members preserve parameter/result order, exact capabilities, and scalar or nominal `own<T>`/`borrow<T>` types; validation rejects duplicate member names, invalid capabilities, undeclared resources, and borrowed results. Failure-report documents encode a primary record, ordered cleanup-only suppressed destructor records, poisoned state, fixed category/phase/diagnostic tags, and an explicit redacted-or-sanitized message policy. Construction and bounded decoding reject invalid semantics, malformed headers/tags/booleans/UTF-8, truncation, trailing bytes, and exceeded byte/item/depth/string limits with structured offsets. Checked-in golden encodings, canonical round trips, semantic and malformed-input tests, an arbitrary-byte corpus, and dedicated fuzz targets cover both document kinds. RFC 0003 remains proposed, so this completes the 8-point prototype task without claiming owner acceptance or a production IDL.

The component runtime now also prototypes generation-tagged owned handles and explicit shared/exclusive borrow tokens. Released slots advance their generation and the lowest free slot is reused deterministically; stale owners cannot alias replacements, and generation exhaustion retires a slot instead of wrapping. Shared borrows coexist, exclusive borrows and mutable access enforce exclusivity, active borrows block release, and repeated borrow ends/releases fail structurally. Six focused tests cover slot reuse, stale owners and borrows, double release/end, conflicts, mutation, deterministic reuse, generation retirement, and borrow-ID exhaustion.

[RFC 0003](rfcs/0003-versioned-component-contracts.md) now specifies the first normal-path canonical call subset: 64-bit `i64`, 32-bit normalized `bool`, and `own<T>`/`borrow<T>` resources represented by per-instance unsigned 32-bit adapter indices. Index zero is invalid and boundary indices never repeat within an instance, preventing stale numeric copies from aliasing later resources even when internal generation-tagged slots are reused. Whole-list preflight rejects stale, wrong-type, duplicate-owned, owned/borrowed-alias, active-lend, capacity, reservation, and index-exhaustion failures before ownership mutation. A successful owner-transfer commit performs no allocation, callback, or observable intermediate step; it installs a fresh destination owner while making the source index permanently stale. Borrows remain call-scoped and borrowed results are excluded. The proposal separates these project decisions from WIT/Canonical ABI source facts and keeps the RFC proposed.

The same RFC now defines post-commit failure behavior without exposing Rust or native unwinding. Panic and ordinary traps never roll ownership back; an explicit cleanup ledger closes nested borrows before destroying remaining frame-owned resources in reverse acquisition order. Destructors consume their owner before user code, run at most once, continue deterministic best-effort cleanup after failure, and preserve an earlier panic/trap as primary while appending destructor failures in suppressed order. Clean component failures may leave a revalidated instance reusable, while destructor, contained host-callback, invariant, or incomplete-cleanup failures poison it. The design records stable logical diagnostic identities, forbids cleanup reentrancy, treats process abort and foreign unwind as trusted-native limitations, and requires identical VM/JIT/AOT outcomes. The WebAssembly Canonical ABI source last changed 2026-10-13; the official Rust panic, `catch_unwind`, and FFI-unwinding documentation publish no update date and were verified 2026-10-16. Schema-2 failure-report serialization now records these categories and ordering; host-value and cross-mode adapter conformance remain open.

Package/interface negotiation is specified separately from metadata-schema selection and package-manager resolution. The runtime now prototypes that model entirely in memory: normalized full-SemVer values preserve prerelease and build identifiers, exact requirements include build metadata, bounded intervals compare SemVer precedence, and prereleases require explicit opt-in. Capability grants are checked before provider inventory validation; exact locks require the named identity, version, and opaque digest without fallback; unlocked candidates pass exact package/interface identity, version, and structural projection checks before deterministic highest-precedence selection. Projection checks cover ordered scalar and nominal `own`/`borrow` resource types plus exact capabilities, permit additive provider members, and report deterministically ordered rejections. Same-precedence build/digest ties are ambiguity errors, and the result is an immutable validated binding. Fourteen focused tests cover ordering, exact locks, non-fallback, structural fallback and rejection, capability ordering, resource ownership mode, ambiguity, and malformed declarations. This remains an in-memory prototype rather than a new metadata schema, stable digest encoding, package-manager integration, or VM/JIT/AOT instantiation path.

One executable scalar conformance fixture now starts from schema-1 metadata, capability grants, and a logical import binding, validates them against the typed host registry, and derives its MIR host call and exported symbol from the resulting immutable contract. The same function executes wrapping-double cases, including integer extremes, with equivalent VM and Cranelift JIT outcomes. The validated fixture also emits a host-native object whose `run` export and resolved host import are linked and executed on macOS AArch64 through both direct and response-file linker paths, producing the same result. This proves one scalar import/export connection; it does not implement general component instantiation, non-scalar host values, resource/failure adapter execution, accepted production serialization, or remote Windows execution.

The component runtime now also exposes a backend-neutral canonical-resource adapter trace with non-zero monotonically allocated boundary indices that never alias stale copies. Whole-call preflight validates resource identity, ownership mode, duplicate/owned-borrow aliases, active lends, poison state, and destination live/index capacity while reserving commit storage without publishing a handle or changing ownership. The infallible commit path performs no allocation or callback. Dynamic borrows close from child to parent scope, frame-owned resources are removed before destructor callbacks and cleaned in reverse acquisition order, independent destructor failures continue in deterministic order, and callback/invariant/destructor failures poison later entry. Seven focused tests cover failed-preflight atomicity, alias rejection, non-reusing transfer and return, nested borrow order, post-commit non-rollback, reverse cleanup with ordered failures, capacity rejection, and poison enforcement. A separate versioned codec now represents the trace's failure categories and ordering, but this remains an opaque state-machine trace that is not connected to host values or VM/JIT/AOT resource calls; cancellation also remains unresolved.

The clean schema-18 macOS AArch64 [CFG/allocation baseline](../benchmarks/results/2026-10-16-macos-aarch64-b5ddb9b.json) records 56 unique timing rows, four exact callback profiles, eight component-boundary rows, and three process-memory distributions. Iterative Fibonacci carries a countdown and two wrapping sequence values through typed block arguments; direct Rust, VM, and JIT medians are 6.510 ns, 1.231 us, and 7.463 ns, with 48 bytes of JIT code and a 400-byte AOT object. The retained allocation trace records 18,102,042 peak live requested bytes, a 21,020,672-byte median resident delta, and a 2,918,630-byte combined allocator-overhead/fragmentation estimate. These are platform-specific directional results rather than performance or memory budgets. Remote Windows execution remains open.

Current progress: the first typed function now runs through both verified bytecode and a Cranelift JIT prototype, and the same MIR lowering can emit a host-native object artifact. Minimal typed `i64` host-function registration, a plain script-to-host callback, and allocation-backed callbacks run through both execution modes; object output represents callbacks as imports. The VM now composes opt-in deterministic instruction-fuel and host-call budgets with a pre-allocation value-slot limit, cooperative one-way interruption flag, and absolute monotonic deadline. Typed basic blocks with explicit branch, jump, and return terminators are shared across MIR, bytecode, VM, JIT, and AOT; validators enforce target existence, reachability, and SSA dominance, while a fuel-bounded back-edge test covers non-termination. Typed block parameters and edge arguments carry one value through countdown, two through bounded sum, and three through iterative Fibonacci; VM and JIT results agree, and AOT object emission succeeds. An initial shared `SystemLinker` driver validates ordered link requests, selects the host toolchain command flavor, and reports structured failures; macOS AArch64 integration links and executes add, minimum, Fibonacci, plain host-callback, host-allocation, accumulator, and conditional-sum objects through it. The current clean macOS AArch64 [schema 18 benchmark baseline](benchmarks.md) records 56 general timing rows, four exact callback heap profiles, three separate-process memory distributions, code/object/executable sizes, startup/link rows, and four separately labeled component-boundary cost series. Remote Windows execution and benchmark evidence is not yet recorded. All three memory-model stress cases now have executable strict models: no borrows across suspension with deterministic cancellation cleanup, explicit shared/weak and arena-scoped cyclic graphs, and actor-isolated shared mutable services with moved messages. Generated-code inspection confirms a two-instruction AArch64 add, compare-and-branch minimum, and position-independent GOT import for the host callback. Source-mapped DWARF/dSYM and CodeView/PDB evidence now covers the representative tier-1 artifacts. RFC 0001 remains proposed; less restrictive alternatives, broader embedding limits and native enforcement, production linker hardening and startup/runtime packaging, Windows x86-64 linking, execution, benchmark, and CI evidence, source allocation semantics, acceptance and implementation of proposed source-level control-flow semantics, source-language locals, unwind metadata, optimized stepping, JIT debugger registration, and the backend decision remain open.

A post-CFG reinspection confirms unchanged minimum code and object sizes and instruction sequence. `SystemLinker` now models `Cc` and `MsvcLink` command flavors, selects the matching host default, supports an explicit driver with ordered prefix arguments, exposes the complete command, and retains it in structured execution failures. Ordered library search paths and logical library names lower to flavor-specific arguments, and validation rejects empty object paths, search paths, and names. Explicit response-file linking uses GNU-escaped UTF-8 for `Cc` and Windows-escaped, BOM-prefixed UTF-16LE for `MsvcLink`; focused tests cover serialization and validation, and the macOS AArch64 integration executes both direct and response-file links. Toolchain discovery and broader target policy remain open. Targeted object emission now constructs an ISA from an explicit triple, and a host-independent test validates the format, architecture, export, and import of `x86_64-pc-windows-msvc` COFF output. A cfg-gated MSVC direct/response-file object-link-execute test and SHA-pinned Windows Server 2025 workflow are configured, but no Windows runner result is recorded for this commit, so Windows linking and execution evidence remain open.

Initial host-toolchain discovery now scans `PATH` in deterministic candidate order, rejects non-executable Unix files, reports the searched path and candidates structurally, and drives the macOS integration. The cfg-gated MSVC integration uses the same discovery after the developer environment initializes `PATH`; Visual Studio, SDK, and compiler installation discovery outside an initialized environment remains open.

The initial tier-1 linker policy is now explicit: backend-neutral `LinkTarget` values own the exact `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` triples and map them to command flavor, default program, and ordered discovery candidates. Explicit-target construction/discovery, the COFF object test, and both platform integrations share the mapping, while unsupported triples fail structurally. Policies for additional targets, installation discovery, and startup/runtime inputs remain open.

The schema 3 harness now measures lowering, JIT/AOT compilation, direct/VM/JIT calls, artifact sizes, and system linking for a bounded countdown back-edge in addition to add and minimum. It versions the new fields and renamed link series explicitly. A clean macOS AArch64 schema 3 baseline is recorded. Windows CI validates and uploads the corresponding JSON for 30 days, but no remote runner result is recorded yet; allocation, broader CFG coverage, and Windows benchmark interpretation remain open.

The schema 4 harness now records linked AOT executable size and repeated warm-cache process startup on both tier-1 evidence paths. It separates this row from system-link time and documents that process creation, loader work, the tiny harness, output collection, and termination are included. A clean macOS AArch64 result records a 16,992-byte executable and 1.511 ms median startup, but a 49.824 ms outlier raises startup RSD to 276.388%, so the method needs refinement before setting a budget. Windows CI validates both rows and a positive executable size; memory, debug quality, allocation, broader CFG coverage, remote Windows evidence, and cross-platform interpretation remain open.

The schema 5 harness preserves the single-launch startup row and adds a configurable batched row that reports per-launch elapsed time across sequential launches. This separates tail-sensitive latency evidence from a less noise-sensitive amortized estimate. A clean macOS AArch64 result keeps similar 1.487 ms single and 1.499 ms batched medians while reducing RSD from 256.272% to 4.688% for the ten-launch batch. Windows CI validates the configured batch size and both rows; remote Windows comparison and the remaining evidence gaps stay open.

The schema 6 harness adds the two-value accumulator to lowering, JIT/AOT compilation, direct/VM/JIT calls, generated code and object sizes, system linking, linked-executable size, and both startup rows. Platform series are renamed to reflect the four-object link set, and Windows CI validates the new accumulator and platform fields. A clean macOS AArch64 result records all 30 rows and a 17,056-byte linked harness; remote Windows evidence remains open alongside allocation, additional source-level control flow, memory, debug-quality, and backend-decision work.

A post-schema-6 conditional-sum CFG prototype adds an internal branch to the bounded accumulator loop and merges the add/skip paths through block parameters before the shared back-edge. VM/JIT differential tests cover both paths, macOS AArch64 direct/response-file integrations execute the fifth AOT object, and host-independent Windows x86-64 COFF validation checks its export; the cfg-gated MSVC integration mirrors that link-and-run path. Source-level control-flow semantics and further CFG/allocation workloads remain open.

The schema 7 harness now measures the conditional-sum workload across lowering, JIT/AOT compilation, direct/VM/JIT calls, generated-code/object size, and the expanded five-object system-link/startup set. It adds six conditional-sum rows and renames the platform series and linked-executable field for that set; Windows CI validates the schema 7 contract. A clean macOS AArch64 result records all 36 rows, the 44-byte JIT body, 400-byte object, and 17,104-byte linked harness. Source-level semantics, allocation and memory workloads, debug-quality evidence, further CFG coverage, and remote Windows execution and benchmark evidence remain open.

The async semantics experiment now compares the strict default `RejectAll` suspension policy with an explicit `AllowSharedFrameBorrows` alternative. The alternative preserves a shared task-frame borrow across suspend/resume, still rejects an exclusive borrow with `SEM0007`, and ends active shared borrows before cancellation drops owned values. This narrows the less-restrictive-design question without accepting semantics; pinned or exclusive borrows, source-level frame-residency proofs, destructor failure, VM/native traces, and runtime cost remain open.

The cyclic-graph experiment now compares the default `Reject` shared-cycle policy with an explicit `CollectUnreachable` alternative. The collector permits strong cycles, traces strong edges from remaining live roots after each root drop, preserves a cycle while any root reaches it, and reclaims it in reverse creation order after the last root disappears; weak edges never retain a target. [RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md) turns that comparison into a decision-ready proposal: singly owned plain values, explicit atomic `shared`/non-retaining `weak` references, pre-mutation rejection of cycle-closing strong edges, lexical arenas for intentional cycles, and no mandatory tracing collector. It specifies failure atomicity, deterministic arena teardown, cross-mode equivalence, compatibility, diagnostics, trust boundaries, alternatives, staged implementation, and observable tests while keeping exact convenience syntax provisional. The RFC remains proposed and records **ready for decision**; production cost benchmarks and any future separately named traced heap remain follow-up work rather than accepted behavior.

The shared-service experiment now compares the default `OwnerIsolated` mutation policy with an explicit `ExclusiveCapability` alternative. Under the alternative, any task with a valid handle may mutate while holding the single active capability; tasks without handles and concurrent mutators are rejected with structured diagnostics, and cancellation waits for a non-owner mutation boundary before cleanup. This makes a lock-like competing design executable without accepting it. Read/write and atomic capabilities, capability derivation, scheduling fairness, priority inversion, poisoning, deadlock, reentrancy, panic handling, backpressure, VM/native traces, and runtime cost remain open.

The host-allocation boundary experiment began with a single-allocation callback and a burst callback that keeps 64 boxes with 512 payload bytes live before summing and dropping them. VM and JIT differential tests use Rust `Box` callbacks, while macOS AArch64 direct/response-file AOT integration and the configured Windows MSVC path use corresponding C `malloc`/`free` callbacks imported by distinct objects. Schema 8 adds direct Rust, VM, and JIT runtime rows for the single-allocation callback, and schema 11 adds the same three rows for the burst without changing the AOT link/startup set; Windows CI validates all six rows. The clean schema 11 macOS AArch64 baseline records single-allocation medians of 12.272 ns direct, 190.573 ns through the VM, and 14.123 ns through JIT, plus burst medians of 782.624 ns, 962.329 ns, and 786.270 ns. Source allocation semantics, ownership, allocator/collector policy, memory limits, allocator-overhead and peak-memory accounting, broader representative workloads, remote Windows evidence, and cross-platform cost remain open.

The VM resource-control prototype now composes optional instruction-fuel and host-call budgets with a value-slot limit, shared one-way interruption flag, and absolute monotonic deadline. Instructions/terminators and entered callbacks consume their respective counters, checks happen before the corresponding side effect, and the verified register count is checked before register-file allocation. An already-set interruption or expired deadline stops before that allocation; later conditions are observed at instruction, terminator, and pre-callback safe points, with both consumed counters in the structured error and interruption taking precedence. A callback-triggered request or deadline overrun is observed after that callback completes. Successful limited execution reports both dynamic counts and the static slot footprint. Zero host-call budget suppresses the callback, and an undersized slot limit rejects execution, both with structured errors; existing convenience APIs remain unbounded, non-interruptible, and without a deadline. The heap, recursion, tasks, callback-internal preemption, byte-level accounting, and native/JIT enforcement remain open.

[RFC 0002](rfcs/0002-prototype-control-flow.md) now gives the implemented integer/CFG prototype a proposed source-level interpretation. It defines wrapping `i64` addition, signed comparison, left-to-right evaluation, mutable locals, structured `if`/`else` and `while`, explicit returns, and lowering of live values through typed block arguments. The focused RFC is ready for an owner decision but remains proposed; parser, HIR, formatter, source diagnostics, and source-to-MIR implementation remain open.

The schema 9 harness adds an unset-flag `vm_countdown_interruptible_call` row beside the existing VM countdown row, allowing direct measurement of the incremental safe-point acquire-load cost without changing the input sequence or bytecode. The historical clean schema 14 run records ordinary and interruption-enabled VM countdown medians of 1.611 us and 1.641 us. The 29.868 ns (1.854%) increase is smaller than the interruption row's 15.866% RSD, so it shows no measurable regression and does not establish an overhead budget. Schema 18 retains the row and is the current CI contract; no remote Windows result is recorded yet.

The schema 10 harness adds `vm_countdown_deadline_call`, which executes the same countdown bytecode and inputs with an absolute monotonic deadline set one hour after harness preparation. It isolates the steady-state clock-check cost while the deadline remains unexpired. The historical clean schema 14 macOS AArch64 run records ordinary and unexpired-deadline VM countdown medians of 1.611 us and 3.320 us, a 1.709 us or 106.071% increase. Schema 18 and Windows CI retain the row, but no remote Windows result is recorded yet.

The schema 11 harness adds direct Rust, VM, and JIT rows for the 64-allocation burst. A clean historical 31-sample macOS AArch64 run records all 44 unique rows and burst medians of 782.624 ns, 962.329 ns, and 786.270 ns; macOS AOT plus host-independent Windows COFF tests cover the new callback import. Windows CI retains all three rows under schema 18, but no remote Windows result is recorded yet.

The schema 12 harness keeps those 44 timing rows and adds deterministic requested-payload heap profiles for the host allocation callbacks. The single workload records one allocation with 8 total/peak live bytes and zero bytes after completion; the burst records 64 allocations with 512 total/peak live bytes and zero afterward. A clean 31-sample macOS AArch64 run records both exact profiles, all 44 unique timing rows, and burst medians of 768.283 ns, 946.601 ns, and 771.625 ns. Tests and Windows CI enforce the exact values and scope. This closes the first logical heap-lifetime measurement without overstating the evidence; schema 13 now supplies the first local process envelope, while allocator metadata and fragmentation, VM/JIT internal allocations, representative application allocation, and remote Windows evidence remain open.

The schema 13 harness retains all schema 12 fields and adds a process-level peak resident-memory distribution for the linked, allocation-free AOT harness. Each sample launches a separate process; macOS reports `ru_maxrss` through `/usr/bin/time -l`, while Windows reports `Process.PeakWorkingSet64` through PowerShell. Both are byte-valued, OS-specific process envelopes that include shared pages and loader/runtime state. Unit tests and a native smoke run validate the macOS parser and distribution, and Windows CI requires a positive ordered 31-sample row. This implements the first process-level peak-memory mechanism without claiming allocator-overhead or representative-workload evidence. The historical clean schema 13 macOS AArch64 result records all 31 samples as 1,671,168 bytes with 0.000% RSD; the Windows baseline remains open.

The schema 14 harness retains every schema 13 field and adds direct Rust, VM, and JIT timing rows for the allocation-churn callback plus an exact requested-payload heap profile. The deterministic sequence allocates 64 boxed `i64` values, frees every other allocation, fills the 32 holes with replacements, and then consumes the final 64, producing `(96 allocations, 768 total bytes, 512 peak live bytes, 0 live bytes afterward)`. VM/JIT differential tests cover wrapping results, macOS AArch64 direct/response-file integration executes the matching C `malloc`/`free` import, host-independent Windows COFF validation checks `lang_host_i64_3`, and the cfg-gated MSVC integration and Windows workflow retain the contract. A clean 31-sample macOS AArch64 run records all 47 unique timing rows, all three exact heap profiles, direct Rust/VM/JIT churn medians of 1.192 us, 1.348 us, and 1.173 us, and linked-process peak memory from 1,654,784 to 1,671,168 bytes. Variable-size and representative workloads, allocator overhead/fragmentation, and remote Windows execution remain open.

The schema 16 harness adds direct Rust, VM, and JIT timing for the variable-size allocation callback, which holds 16 boxed `i64` slices ranging from 8 to 128 payload bytes, keeps 1,088 requested payload bytes live at once, and returns a deterministic wrapping fold. The clean 31-sample macOS AArch64 run records medians of 215.438 ns, 397.544 ns, and 218.453 ns and the exact profile `(16 allocations, 1,088 total bytes, 1,088 peak live bytes, 0 live bytes afterward)`. VM/JIT differential tests cover integer extremes, macOS AArch64 direct/response-file AOT integration executes the matching C import, host-independent Windows COFF validation checks `lang_host_i64_4`, and the cfg-gated MSVC path and Windows workflow retain the contract.

The schema 17 harness adds a compiler-like mixed-lifetime allocation trace with exact requested-payload accounting and interleaved control/workload process-memory samples. The clean macOS AArch64 result records 23,894 allocations, 19,183,210 total requested bytes, 18,102,042 peak live requested bytes, a 21,037,056-byte median resident delta, and a 2,935,014-byte combined allocator-overhead/fragmentation estimate. The trace exercises 12 size classes, partial release, differently sized replacement, and overlapping transient buffers. It closes the planned representative allocation evidence without claiming a source-language heap, production allocator choice, or separable metadata/fragmentation accounting; remote Windows execution remains open.

The schema 18 harness retains every schema 17 field and adds iterative Fibonacci lowering, JIT compilation, AOT emission, direct/VM/JIT runtime, and artifact-size evidence. The workload rotates three loop-carried values over a back-edge using existing MIR operations. The clean macOS AArch64 result records all 56 unique timing rows, Fibonacci medians of 6.510 ns direct, 1.231 us through the VM, and 7.463 ns through JIT, a 48-byte JIT body, a 400-byte object, and a 17,168-byte linked harness. VM/JIT differential tests, macOS direct/response-file execution, host-independent Windows COFF validation, the cfg-gated MSVC path, and schema 18 Windows CI retain the contract. This completes the planned additional CFG evidence without accepting source semantics; remote Windows execution remains open.

A clean pre-implementation macOS AArch64 [AOT debug-information inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md) established the original negative baseline. The committed [source-mapped DWARF inspection](../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md) regenerates the representative add, minimum, and host-callback objects from commit `31e182ac27e324048597a5ebd0fda4d2d4934395`. Structurally validated source-file, function-span, per-instruction-span, and terminator-span data reaches backend-neutral JIT/AOT machine-code ranges and relocated DWARF v4 compile-unit, subprogram, and line-table data. All three objects pass `dwarfdump --verify`; direct and response-file executables link and run; and both generated dSYMs pass verification while retaining the fixture path and all three functions. The committed [Windows x86-64 CodeView/PDB inspection](../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md) emits the same representative COFF objects from commit `465b0d66ba6ea61a0d4b26201e6ab043f2f15f9d`, validates their C13 symbols, types, lines, and relocations, links them cross-target with LLD, validates the PE/PDB identity, and has LLDB resolve all three functions to the fixture. These captures close the two planned source-mapping tasks. Native Windows runtime/CI and benchmark evidence, source-language locals, unwind metadata, JIT debugger registration, and optimized stepping remain separate work.

Exit: every P0 task above is checked; source semantics, component contracts, ownership/cyclic-graph policy, and async-cancellation/shared-service policy have evidence-backed RFC dispositions; macOS and Windows evidence supports a recorded initial backend choice; and RFC 0001 records the resulting architecture baseline.

## Milestone 1: frontend and bytecode vertical slice

**Progress: 0 / 65 pt (0.0%).** Milestone 0 prototypes are groundwork; these tasks require the complete source-driven vertical slice.

- [ ] **P0 — Establish structured source diagnostics with stable codes, labels, and fix-its.** — 5 pt
- [ ] **P0 — Implement a lossless parser with error recovery and source spans.** — 8 pt
- [ ] **P0 — Implement scopes, name resolution, and module loading.** — 5 pt
- [ ] **P0 — Type-check primitive expressions, functions, and structured control flow.** — 8 pt
- [ ] **P0 — Type-check records, variants, patterns, and exhaustiveness.** — 8 pt
- [ ] **P0 — Lower resolved and typed source through HIR into validated typed MIR while preserving spans.** — 5 pt
- [ ] **P0 — Promote the prototype into versioned register bytecode with encoding, decoding, and verification.** — 5 pt
- [ ] **P0 — Execute the source-language subset in the VM with typed host registration.** — 5 pt
- [ ] **P0 — Provide deterministic script and REPL entry points.** — 3 pt
- [ ] **P1 — Implement the formatter and parser/formatter round-trip tests.** — 5 pt
- [ ] **P1 — Build the initial incremental query graph shared with an LSP prototype.** — 8 pt

Exit: small programs run as scripts with deterministic behavior and useful IDE diagnostics.

## Milestone 2: ownership and native AOT

**Progress: 0 / 74 pt (0.0%).**

- [ ] **P0 — Implement owned values, moves, and lexical borrow checking in typed HIR and MIR.** — 8 pt
- [ ] **P0 — Elaborate deterministic drops and define destruction behavior on return, error, and panic paths.** — 8 pt
- [ ] **P0 — Define narrow unsafe boundaries and enforce FFI validity rules.** — 5 pt
- [ ] **P0 — Implement C ABI import/export and generated headers.** — 8 pt
- [ ] **P0 — Complete source-program object generation for macOS AArch64 and Windows x86-64.** — 8 pt
- [ ] **P0 — Harden linker discovery, startup/runtime packaging, and production error handling.** — 5 pt
- [ ] **P0 — Build the representative differential VM/native conformance suite.** — 8 pt
- [ ] **P0 — Measure representative native workloads against equivalent Rust/C implementations.** — 8 pt
- [ ] **P1 — Implement allocator-aware standard containers.** — 8 pt
- [ ] **P1 — Emit and validate source-span native debug metadata on both tier-1 platforms.** — 8 pt

Exit: representative command-line programs match VM semantics and performance is measured against equivalent Rust/C implementations.

## Milestone 3: concurrency and embedding

**Progress: 0 / 86 pt (0.0%).**

- [ ] **P0 — Implement structured task lifetimes and deterministic cancellation cleanup.** — 8 pt
- [ ] **P0 — Implement channels with bounded backpressure and cancellation behavior.** — 5 pt
- [ ] **P0 — Enforce actor isolation and accepted data-race capability rules.** — 8 pt
- [ ] **P0 — Stabilize the C embedding API and generated bindings.** — 8 pt
- [ ] **P0 — Implement the versioned component IDL and bounded serialization.** — 8 pt
- [ ] **P0 — Implement canonical owned/borrowed resource adapters.** — 8 pt
- [ ] **P0 — Enforce capability-gated component imports.** — 5 pt
- [ ] **P0 — Support mixed native/VM calls and callbacks with typed errors.** — 8 pt
- [ ] **P0 — Preserve stack traces and cancellation across native/VM boundaries.** — 5 pt
- [ ] **P0 — Enforce heap/allocation limits, including host-callback accounting.** — 5 pt
- [ ] **P0 — Enforce recursion-depth and live-task limits.** — 5 pt
- [ ] **P0 — Enforce cooperative interruption across VM, JIT, AOT, and callback boundaries.** — 8 pt
- [ ] **P1 — Ship trusted-native and verified-script plugin examples.** — 5 pt

Exit: a native host can load a user script, expose a typed API, call the script, receive callbacks, and enforce capabilities without IPC.

## Milestone 4: developer toolchain

**Progress: 0 / 63 pt (0.0%).**

- [ ] **P0 — Implement `lang` build, run, and test orchestration.** — 5 pt
- [ ] **P0 — Define package/workspace manifests and target conditions.** — 5 pt
- [ ] **P0 — Implement deterministic dependency resolution, lockfiles, checksums, and offline mode.** — 8 pt
- [ ] **P0 — Integrate package, fmt, lint, and doc commands into the CLI.** — 5 pt
- [ ] **P0 — Implement LSP navigation, completion, and semantic tokens.** — 8 pt
- [ ] **P0 — Implement safe rename and code actions.** — 8 pt
- [ ] **P0 — Implement a DAP adapter with mixed native/VM stacks.** — 8 pt
- [ ] **P1 — Add compile-time/runtime profiling, allocation traces, and rebuild explanations.** — 8 pt
- [ ] **P1 — Build an install/update/channel tool comparable in role to `rustup`.** — 8 pt

Exit: a new developer can install one tool, create a package, develop in an editor, test, debug, and produce a reproducible binary.

## Milestone 5: ecosystem and compatibility

**Progress: 0 / 65 pt (0.0%).**

- [ ] **P0 — Define editions, deprecation policy, and the compatibility matrix.** — 5 pt
- [ ] **P0 — Build the edition migration and machine-applicable fix tool.** — 8 pt
- [ ] **P0 — Complete the public-registry threat model before accepting uploads.** — 8 pt
- [ ] **P0 — Implement registry package storage, indexing, download, and immutable-version rules.** — 5 pt
- [ ] **P0 — Implement authenticated publish/yank/owner operations and an audit trail.** — 5 pt
- [ ] **P0 — Implement signed release manifests and package provenance.** — 8 pt
- [ ] **P0 — Add a WebAssembly component target for portable plugins.** — 8 pt
- [ ] **P0 — Validate WebAssembly plugin sandbox and capability conformance.** — 5 pt
- [ ] **P1 — Provide documentation hosting and package-metadata search.** — 5 pt
- [ ] **P1 — Evaluate an LLVM release backend using published benchmarks and debug-quality criteria.** — 8 pt

Exit: the language can evolve without silently breaking source, package, bytecode, or component consumers.

## Milestone 6: self-hosting

**Progress: 0 / 74 pt (0.0%).**

- [ ] **P0 — Define and freeze the self-hosting subset.** — 5 pt
- [ ] **P0 — Implement lossless syntax, parsing, and diagnostics in the language.** — 8 pt
- [ ] **P0 — Implement resolution, type checking, and typed HIR in the language.** — 8 pt
- [ ] **P0 — Implement MIR lowering and verification in the language.** — 8 pt
- [ ] **P0 — Implement the required bytecode and native code-generation paths in the language.** — 8 pt
- [ ] **P0 — Implement the minimal package and compiler driver needed for bootstrap.** — 5 pt
- [ ] **P0 — Make the stage 0 Rust compiler build stage 1.** — 8 pt
- [ ] **P0 — Make stage 1 build stage 2 without using untracked stage 0 behavior.** — 8 pt
- [ ] **P0 — Run the same conformance suite against stages 0, 1, and 2.** — 5 pt
- [ ] **P0 — Compare reproducible outputs on both tier-1 platforms.** — 8 pt
- [ ] **P1 — Document bootstrap trust and disaster-recovery procedures.** — 3 pt

Exit: stage 2 passes the same conformance suite as stage 0 and can build the supported toolchain from a documented bootstrap chain.

## Milestone 7: additional frontends

**Progress: 0 / 55 pt (0.0%).**

- [ ] **P0 — Demonstrate that a distinct frontend provides requirements that libraries, profiles, or syntax sugar cannot serve.** — 5 pt
- [ ] **P0 — Specify the frontend semantics and its shared-MIR/component boundary.** — 5 pt
- [ ] **P0 — Implement frontend-specific parsing, HIR, and semantic analysis.** — 8 pt
- [ ] **P0 — Lower only faithfully represented semantics through the shared typed MIR.** — 8 pt
- [ ] **P0 — Implement component adapters where memory, effect, or runtime models differ.** — 8 pt
- [ ] **P0 — Add cross-language package metadata and version resolution.** — 5 pt
- [ ] **P0 — Add differential cross-language conformance tests.** — 8 pt
- [ ] **P0 — Add cross-language stack traces, source mapping, and debugger integration.** — 8 pt

Exit: a second frontend adds clear user value without weakening the first language’s safety or making the shared IR an unmaintainable union of unrelated semantics.

