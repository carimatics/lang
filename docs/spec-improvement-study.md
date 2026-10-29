<!-- canonical-language: en; japanese-translation: ja/spec-improvement-study.md -->

# Specification improvement study: Goose, memory domains, and GraalVM

[日本語版](ja/spec-improvement-study.md)

- Study and last update: 2026-10-30 JST.
- Review outcome: **needs prototype/measurement**.
- Project baseline: [`df55f6e`](https://github.com/carimatics/lang/commit/df55f6ebcf33edb824b7a96e878ab9da5540228d), the `main` snapshot containing the [GraalVM study](graalvm-study.md).
- Scope: preserve the evidence and decision criteria from the Goose investigation, mixed-memory-domain analysis, and their synthesis against current lang contracts. This document includes their relevant findings, examples, and sources; no separate local report is required.
- Status: research recommendations, not an accepted RFC, implementation claim, milestone activation, or new delivery estimate.

## Recommendation

Keep ownership as the default foundation. First make optimization obligations and retention reasons explicit; then measure selected native-code elimination, immutable compilation reuse, and destination construction. Investigate packed immutable data and an optional traced heap only for workloads that demonstrate a need.

The three investigations complement each other: Goose offers data-placement and construction ideas; GraalVM offers reachability, preparation, and observability ideas; memory-domain work explains the boundaries required to combine reclamation strategies. They do not justify replacing the shared typed MIR, adopting a mandatory GC, rewriting the runtime around Truffle, or weakening stale-reference safety.

| Candidate | Recommendation | Admission condition |
| --- | --- | --- |
| Optimization conformance contract | First priority | Consolidate existing failure, cost, ownership, authority, and provenance rules before changing behavior. |
| Reachability/ownership/allocation explanations | Start with a read-only report | Separate static facts, logical charges, and measured costs; bound analysis and output. |
| Entry/export-rooted native elimination | Narrow experiment | Validate the whole input first; retain non-call dependencies and valid artifact/debug mappings. |
| Reuse of immutable code/plans | Measure first | Preserve binding/code-owner lifetime and independent instance authority/state. |
| Destination construction | Narrow MIR experiment | Preserve evaluation, partial-failure cleanup, return escrow, and logical charging. |
| Packed data and detachable frozen segments | Workload-specific experiment | Compare against existing Bytes plus ownership transfer; prove boundary validation and ownership. |
| Optional traced references | Needs requirements and prototype | Separate type/category, limited bridges, collection/cost/failure contract; no silent change to shared/arena. |
| Universal Goose stacks, tracing shared by default, or a Graal/Truffle rewrite | Do not adopt on this evidence | No demonstrated advantage sufficient to justify the compatibility and runtime changes. |

## Current project constraints

[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md) permits future explicitly traced types but rejects tracing as the base `shared` representation. Owned values move; strong sharing is explicit; weak back-edges and lexical arenas address different graph lifetimes. Arena references cannot escape or cross into another arena/shared heap without a separately designed adapter. Reused numeric slots must not make stale handles alias later allocations. An arena guarantees bulk lifetime and reverse allocation-order destruction, not a particular bump allocator or packed layout. [L1]

[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md) defines reverse actual initialization-order cleanup, exactly-once obligations, unpublished return escrow, failure precedence, and explicit allocator authority. [RFC 0013](rfcs/0013-service-byte-work-budget.md) defines logical service charges and admission before effects. [RFC 0024](rfcs/0024-send-share-and-service-authority.md) defines logical-task Send/Share; it does not make every managed or host type OS-thread-safe. [RFC 0033](rfcs/0033-component-v1-compatibility.md) freezes component C v1 independently of private source/SDK representations. [L2–L5]

User generics, general closures, explicit lifetime parameters, and borrowing across suspension remain future design work at this snapshot. A second source language is not a prerequisite for testing any recommendation here. The active milestone is M4 tooling; this study neither adds memory-model work to its exit criteria nor creates another doing delivery. [L0]

The earlier Goose comparison used lang `1a9aac9`; the memory-domain comparison used `fb970d6`. RFCs 0004, 0008, 0013, 0024, and 0033 are unchanged between those snapshots and `df55f6e`. The synthesis rechecked architecture, roadmap, and package/editor contracts against the latter. Findings do not cover another session's unmerged changes.

## What Goose contributes

The inspected Goose snapshot is [`cce94b9`](https://github.com/aardappel/goose/tree/cce94b919d27458856ae2685974049eac78debcf); its specification is a **v4 draft**. These are facts about that design, not accepted lang behavior. [G1–G3]

| Goose mechanism | Useful idea | Tradeoff or incompatibility for lang |
| --- | --- | --- |
| Multiple data stacks; reserved address space and committed growth | Stable-address scratch storage and aggregate allocation | Reservations, platform behavior, reclamation, recursion, recoverable OOM, and foreign allocator contracts still matter. |
| Root-based inferred lifetimes | Explain why a reference is live and what prevents escape | No written lifetime arguments does not mean unrestricted borrowing or no lifetime checking. |
| Growth/containment restrictions, relative references, packed layouts | Dense self-contained data with fewer per-node allocations | Offset validation, alignment, dynamic field access, mutation constraints, and SIMD costs must be measured. |
| Specialization by type, root, mutability, static function value, and construction destination | Explain specialization and bound code growth | Rechecking reachable concrete bodies is not a replacement for lang's whole-input checking, package boundaries, or partial editor analysis. |
| Construction directly into the result destination | Avoid some physical temporary copies | By-value copies can still be proportional to payload size; initialization/failure semantics remain observable. |
| Reusable pool slots | Separate physical storage reuse from object identity | Goose can allow an old reference to observe a newly reused value of the same type. This conflicts with lang's stale-handle contract. |
| Independent thread programs and flat copied messages | Coarse isolation and batched transfer | Not a substitute for lang's structured tasks, capability/resource boundaries, and Send/Share rules. |

Goose's AOT route emits C; its embedded JIT uses TinyCC. Those implementation choices do not establish that replacing the existing VM/Cranelift pipeline would improve lang. Similarly, upstream benchmark tables compare different languages, compilers, and workloads; this study made no lang-versus-Goose speed claim. Best-of-three or aggregate scores are not tail-latency, memory, or reproducibility guarantees. [G2–G3]

### Historical Goose probes

On macOS 27.2 arm64, Apple clang 21.0.0 and CMake 4.4.5, a Release build of the pinned Goose snapshot was tested without extra built-in packages. The build succeeded with warnings. These small probes were run during the 2026-10-30 investigation; they were not a full test/benchmark campaign.

| Probe | Observed outcome | Meaning |
| --- | --- | --- |
| Keep the first reference while growing an array by 100,000 elements | JIT and C AOT print `100001 1 99.5` | The tested growth preserves the reference under the selected stack reservation. |
| Free a pool slot, reuse it, then read the old reference | JIT and C AOT print `99 99 1` | Type/bounds safety and logical-object identity are different guarantees. |
| Clear an array and subsequently use its element reference | Check rejects, exit 1 | Root/lifetime restrictions still exist. |
| Recurse while a nonfixed scratch value remains live across the recursive call | Check rejects, exit 1 | Data-stack lifetime restrictions constrain recursion. Scratch that dies before reentry is a different case. |

The complete inputs, in table order, are:

```text
// stable.goose
struct Item { id: i32, weight: f32 }
fn main() {
    var items: Item[>..] = [];
    let first .= items.push(Item { 1, 0.5 });
    for i: i32 in 2..100002 { items.push(Item { i, 0.0 }); }
    first.weight = 99.5;
    print(items.len, " ", first.id, " ", first.weight);
}

// reuse.goose
struct Slot { value: i64 }
fn main() {
    reusable var pool: Slot[>..] = [];
    let old .= pool.alloc_ref(Slot { 7 });
    pool.free(pool.index_of(old));
    let fresh .= pool.alloc_ref(Slot { 99 });
    print(old.value, " ", fresh.value, " ", pool.len);
}

// shrink.goose
fn main() {
    var xs: i64[>..] = [];
    let first .= xs.push(7);
    xs.clear();
    print(first);
}

// recursion.goose
recursive fn f(n: i64) -> i64 {
    var scratch: u8[>..] = [];
    scratch.push(1);
    if n == 0 { return scratch.len; }
    return f(n - 1) + scratch.len;
}
fn main() { print(f(2)); }
```

To reproduce in a separate Goose checkout at the pinned revision, save the four inputs as named, then run:

```sh
cmake -S . -B build-study -DCMAKE_BUILD_TYPE=Release -DGOOSE_BUILTIN_LIBS=
cmake --build build-study --parallel 2
./build-study/goose run stable.goose --stack-reserve 8M
./build-study/goose run reuse.goose --stack-reserve 8M
./build-study/goose check shrink.goose
./build-study/goose check recursion.goose
# The two check commands above are expected to exit 1.
./build-study/goose --standalone -o stable.c stable.goose --stack-reserve 8M
clang -O2 stable.c -o stable-native -lm -pthread
./stable-native
./build-study/goose --standalone -o reuse.c reuse.goose --stack-reserve 8M
clang -O2 reuse.c -o reuse-native -lm -pthread
./reuse-native
```

No Windows, sanitizer, long-running stress, or full upstream benchmark result is implied.

## Memory domains: share contracts, not every representation

Ownership, tracing, region lifetime, and physical layout are separate axes. One runtime can support more than one strategy without imposing Rust-like lifetime syntax on every future frontend. It cannot promise arbitrary raw cross-references, immediate independent reclamation, moving objects, unrestricted cycles, and zero boundary cost simultaneously.

Preserve each domain's efficient local representation. At a boundary, distinguish:

| Operation | Required contract | Initial restriction |
| --- | --- | --- |
| Scoped borrow | Domain/type identity, liveness, access permission, and a lease covering use; pin/re-resolve if movable | No ordinary loan across suspension; reentry must not close/reuse the owner. Nonmoving storage still needs liveness protection. |
| Long-lived retention | Explicit external root or a traced strong edge in a participating holder | An integer handle alone does not retain. Initially restrict cross-domain cycles with weak edges or an enforced ownership direction. |
| Ownership transfer | Detachable allocation unit, original allocator, destination metadata reservation, atomic publication | No arbitrary interior arena element promotion in O(1); failure before commit keeps the original owner valid. |
| Copy/snapshot | Independent destination value, checked conversion, explicit accounting | Often simpler for small values, incompatible layouts, or complex reference graphs; benchmark against many fine-grained bridges. |

An object root prevents collection; it does not necessarily pin an address. A generation check rejects stale identities; it does not prevent a close/reuse between checking and dereferencing. Access permission and exclusivity are separate again. Preserve domain/table, generation, provider/holder, thread, and loan checks at dynamic boundaries. Proven static local invariants may permit cheaper access without replacing all references with generic dynamic handles. [L1–L5, S4]

### Cycles, collection, and failure

Two collectors treating every bridge edge as a permanent external root can retain an otherwise unreachable mixed cycle. The choices are constrained cycles/weak back-edges, explicit domain teardown, or cooperative tracing of the participating graph. Independently owned resources must not silently become GC-finalized objects. Kotlin/Native documents an actual limitation: retain cycles containing both Kotlin and Objective-C objects cannot be reclaimed by the combined GC/ARC arrangement. [S3]

For a future traced category, the smallest candidate is thread-confined, nonmoving, and without user finalizers. It still needs root scopes, pointer maps, collector work and allocation budgets, collection triggers, weak-reference/OOM behavior, and cleanup rules. GC does not establish Send/Share. Moving or concurrent collection additionally requires compiler cooperation, mutation barriers, safepoints/stack maps, and a specified synchronization protocol. Wasmtime's scoped roots and LLVM statepoints are useful primary references, not ready-made lang semantics. [S4–S5]

VM/native need not collect at exactly the same physical instant. Any observable difference in weak references, finalization, OOM, budgets, or host effects must nevertheless be specified and tested. Deterministic resource close remains explicit. Mandatory retirement must survive guest budget exhaustion.

### What the abstract model established

The prior investigation compared tracing on four nodes, all 12 possible non-self directed edges, and all root subsets: `2^12 * 2^4 = 65,536` graph/root cases matched an independent transitive-closure oracle. It also enumerated order-preserving interleavings of acquire/read/release with close/reuse, and three normal/exception/simulated-cancellation cleanup paths. These are abstract protocol checks, not a production collector or memory-safety proof.

The decisive counterexamples can be understood without that model's original file:

| Scenario | Counterexample or required outcome |
| --- | --- |
| Root every bridge | With edges A→B and B→A and no external root, union tracing collects both; treating either bridge as an unconditional root keeps both. |
| Check generation only | Validate old token → close → reuse slot with value 99 → dereference cached address can observe 99. Acquire a lease atomically with validation, and exclude close/reuse while leased. |
| Assume root pins address | A rooted object survives compaction but changes address. A cached pointer needs a pin or safe re-resolution. |
| Ignore domain identity | Two domains can have the same generation value. A token/lease from one must not authorize read or release in the other. |
| Ignore tracing mutation | After scanning A, inserting A→unmarked B can hide a live object unless the collector's mutation protocol preserves it. |
| Wrap generations | A finite generation namespace must retire an exhausted slot instead of reviving an old identity. |
| Publish a frozen region | Zero payload copying was modeled only for a detachable pointer-free allocation whose old owner loses release authority. It says nothing about arbitrary interior-object promotion. |

Real native memory, atomic memory ordering, stack maps, actual cancellation, compiler effect checking, and performance were outside that model. A production proof needs those additional layers.

## The synthesis: distinguish three relations

| Relation | Roots/edges or authority | Question answered | Not a substitute for |
| --- | --- | --- | --- |
| Code reachability | Artifact entries/exports/retained REPL definitions; calls, spawn, callbacks, cleanup, helpers | Which code must this artifact retain? | Permission to invoke a host operation |
| Object liveness | External roots/owners, strong/weak edges, region ownership, loans | What can be reclaimed, and when? | Selecting executable functions |
| Access authority | Host grants, provider/holder, instance, thread, scoped loans | May this operation execute now? | Existence, address validity, or code identity |

Share immutable identities, provenance, and independently validated facts where useful. Do not turn these relations into one untyped graph whose node existence grants authority. A source lifetime root, GC root, and code entry root are different concepts. A reachability report may describe imports; it cannot grant them. [L1–L5, S1]

Closed-world assumptions must be artifact-specific. A standalone executable starts from its selected entry and all other contractual external entries. A C library/component retains callable exports, callbacks, and teardown. A REPL retains future-callable definitions. A selected test artifact has its test entries. Source `pub`, interface export, and backend `Linkage::Export` are not equivalent. Unknown indirect targets require a conservative sound set or declining that optimization. [S1, G1]

## Optimization conformance before optimization breadth

Returning the same integer is insufficient. An optimization must preserve the applicable existing contract, or explicitly revise it through an RFC before implementation.

| Observable | Existing obligation | Consequence |
| --- | --- | --- |
| Admission | Whole-input type/ownership/artifact validation and applicable whole-unit control support | Do not prune an invalid unused body before validation. RFC 0013's 2026-10-19 amendment explicitly inspects all functions. |
| Failure/effects | Evaluation order, failure identity/span, primary/secondary order, admitted host/provider prefix | Do not hoist capability checks across observable callbacks or execute effects after refusal. |
| Ownership | Reverse actual initialization order, exactly-once cleanup, unpublished return escrow | Destination construction must preserve history, partial-failure cleanup, and result discard after cleanup failure. |
| Fuel/service work | Logical charge points, used/requested fields, polling and failure precedence | Instruction fusion or physical-copy removal does not erase logical events. This does not demand identical wall-clock duration. |
| Allocation | Original provider, size/alignment, live/cumulative accounting, callbacks, OOM | Keeping only a virtual counter while removing a real allocator request may change effects and failure outcomes. |
| Identity/authority | Domain, generation, holder/provider, loan, grants | Same offsets or code hashes do not prove object identity or authorization. |
| Provenance/formats | Semantic IDs, source spans, native/debug ranges, verified record relationships | Emission compaction needs valid mapping; persistent format changes need independent versioning. |

RFC 0013 counts logical service operations, not arbitrary implementation memory traffic: a temporary swap buffer does not add another logical transfer. RFC 0008's allocation requests, however, can call a host and fail. Thus removing physical copies and removing observable allocation are different proposals. [L2–L3]

A minimal counterexample uses `charge(8); allocate(8); publish; free(8)`. With limit 7 it must refuse before effects; deleting the charge changes failure into success. With limit 8, keeping the charge but erasing allocate/free changes the allocator effect trace. These assertions passed in the synthesis's small illustrative model; it was not a pass implemented in lang.

Start with transformations that preserve the current contract. If allocation elision later has a measured benefit worth changing allocator observability, specify the accounting/failure/host contract explicitly rather than treating a name such as “fast profile” as permission to weaken it.

Keep the original validated semantic unit and IDs while experimenting with a separate emission selection. Existing native-record validation may require all-function correspondence; do not remove machine code and reuse incompatible old records. Reducing portable artifacts is a separate compatibility decision.

## Concrete opportunities

### Explain retention and costs

A first report should answer why a function, owner, allocation, copy, specialization, or helper remains, with original source provenance. Distinguish static facts, specified logical charges, physical measurements, and unknown estimates. Report code/debug/metadata/object/final-linked sizes separately, and requested live bytes separately from RSS. GraalVM's build report is a useful precedent. [S2]

Reuse admitted package/input/overlay epochs for editor-facing results; never attach old semantic facts to a newer file version. An initial internal read-only API need not invent source syntax. A public CLI/JSON report needs versioning, deterministic ordering, program/compiler/target/profile identity, analysis/output bounds, and explicit truncation. Its output is not a trusted proof or capability. Source text and paths in reports are not automatically approved for external publication. [L6]

### Reuse immutable preparation

Measure repeated component compilation before adding a cache. Potential key inputs include program digest, compiler build, target/CPU features, backend settings, roots/artifact kind, ABI/layout/schema, cost profile, and resolved binding assumptions. Embedded host addresses require retaining the matching provider/code owner; they are not portable across arbitrary instances.

On every hit, independently establish grants, allocator, budgets, resource tables, callback authority, thread conditions, and mutable state. A cache of authorized instances is not an immutable code cache. Keep executable ownership alive through result/resource retirement; bound cached entries/bytes and validate untrusted persisted inputs. Start in process before designing disk caches or native-image layers. [L5, GraalVM study]

### Construct directly and pack selectively

Try one MIR transformation that constructs an aggregate directly into its destination. Preserve written field evaluation order, initialized-field cleanup, atomic replacement, and return escrow; this is not a proposal to permit general source-level partial initialization. Preserve logical transfer costs even when physical copies disappear. [L2–L3, G1]

For packed data, first compare an actual immutable snapshot workload against existing Bytes plus explicit move. Only introduce detachable frozen segments if that baseline is inadequate. Validate offsets, overflow, alignment, tags, types, owner lifetime, and absence of prohibited external references/live mutable loans before publication. Transfer one independent allocation's release responsibility, not an arbitrary interior pointer. Measure density and access costs separately; packed layouts can make alignment and vectorization harder. Do not expose private layouts as stable component ABI or portable snapshots without a separate versioned design.

## Executable lang evidence

At `df55f6e`, the compiler was built with Rust 1.99.0 (`b940084d7`, 2026-09-28), default Cargo dev profile, on macOS 27.2 (`26B123`) arm64. The workspace and build output were isolated from other sessions. The final synthesis probe exited 0.

| Fixture | VM/check outcome | Native defined body symbols | Text bytes | Object bytes | Portable bytes | Native-record bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `base`: only main returning 42 | VM returns 42 | 1 | 44 | 944 | 173 | 1,102 |
| `dead`: add unused spare returning 43 | VM returns 42 | 2 | 88 | 1,248 | 294 | 1,474 |
| `invalid`: unused spare calls missing | check/run/native all exit 1, `SRC0002`, span `2:28..2:35`; no object | — | — | — | — | — |
| `export`: no main; C api calls helper | Native generation succeeds; public api symbol retained | 2, plus export/context wrappers | 2,272 | 4,480 | 547 | 5,658 |

The unused function adds **44 text bytes and 304 total object bytes**, not 304 machine-code bytes. This confirms the earlier GraalVM experiment's emission opportunity on the newer snapshot. It does not measure linked executable savings, startup, RSS, compile-speed improvement, or an implemented elimination pass. The C export was generated and inspected, not invoked from a linked C host.

### Self-contained reproduction

Run from a clean checkout of the recorded baseline. The script creates isolated temporary fixtures under `target`, checks outcomes, prints measurements, and removes only its own temporary directory. Native size/symbol inspection below is macOS-specific; use corresponding tooling and new evidence for Windows. Changed compiler/toolchain versions may change sizes.

```sh
cargo build --locked --bin lang --target-dir target/spec-study-build
python3 - <<'PY'
import json
import pathlib
import re
import subprocess
import tempfile

binary = pathlib.Path("target/spec-study-build/debug/lang").resolve()
fixtures = {
    "base": "fn main() -> i64 { return 42; }\n",
    "dead": "fn main() -> i64 { return 42; }\nfn spare() -> i64 { return 43; }\n",
    "invalid": "fn main() -> i64 { return 42; }\nfn spare() -> i64 { return missing(); }\n",
    "export": 'fn helper() -> i64 { return 41; }\npub extern "C" fn api() -> i64 { return helper() + 1; }\n',
}

def run(*args):
    return subprocess.run(list(map(str, args)), capture_output=True, text=True, timeout=30)

with tempfile.TemporaryDirectory(prefix="spec-study-", dir="target") as temporary:
    root = pathlib.Path(temporary).resolve()
    observations = []
    for name, source in fixtures.items():
        (root / (name + ".lang")).write_text(source, encoding="utf-8")
    for name in ("base", "dead", "export"):
        source, output = root / (name + ".lang"), root / name
        if name != "export":
            result = run(binary, "run", source)
            assert result.returncode == 0 and result.stdout.strip() == "42", result
        result = run(binary, "native", "--output", output, source)
        assert result.returncode == 0, result.stderr
        symbols = run("nm", "-g", output / "program.o")
        sizes = run("size", "-m", output / "program.o")
        assert symbols.returncode == sizes.returncode == 0
        match = re.search(r"Section \(__TEXT, __text\):\s+(\d+)", sizes.stdout)
        assert match, sizes.stdout
        defined = [line for line in symbols.stdout.splitlines() if re.search(r"\b[Tt]\b", line)]
        if name == "export":
            assert any("_lang_api_" in line for line in defined), defined
        else:
            assert len(defined) == (1 if name == "base" else 2), defined
        observations.append({"fixture": name, "text_bytes": int(match[1]),
            "file_bytes": {f: (output / f).stat().st_size
                           for f in ("program.o", "program.lbc", "program.native")},
            "defined_symbols": defined})
    for command in ("check", "run", "native"):
        args = [binary, command]
        if command == "native":
            args += ["--output", root / "invalid"]
        result = run(*args, root / "invalid.lang")
        assert result.returncode == 1, result
        assert "error[SRC0002]" in result.stderr and "2:28..2:35" in result.stderr, result.stderr
        observations.append({"command": command, "invalid_unused_body": result.stderr.strip()})
    assert not (root / "invalid" / "program.o").exists()
    print(json.dumps(observations, indent=2))
PY
```

The synthesis also checked illustrative code graphs and separate authorization contexts: call-only traversal missed cleanup/callback/spawn edges; a library rooted at `api` required `helper` without `main`; shared immutable code did not authorize a second instance without the required grant. These are counterexamples to overly broad designs, not tests of a production optimizer/cache.

## Adoption sequence and compatibility

| Order | Small observable delivery | Gate before expansion |
| --- | --- | --- |
| 1 | Conformance matrix and read-only Copy retention report | Distinguish unused code, invalid unused source, and a no-main export; explain unknown edges and bound report work. |
| 2 | Selected native emission for one closed Copy executable | VM/JIT/AOT values, failures/spans, fuel, records, debug mappings; protect exports; measure linked binary separately. |
| 3 | Component preparation measurements and immutable reuse | Cold/hit/miss cost, peak memory, grants/providers/allocators, exhaustion, reentry, last code-owner retirement. |
| 4 | One direct-construction transformation | Partial construction, replacement refusal, cleanup failure, return escrow, service-work thresholds; demonstrate fewer physical copies. |
| 5 | One packed/frozen snapshot workload | Compare existing Bytes/move; measure copies, allocations, live bytes, access time, invalid offsets, failure atomicity. |
| 6 | Optional GC for a demonstrated long-lived cyclic workload | Limited bridges, weak/close/cycle policy, collector budget, VM/native conformance, native memory-safety and both-host checks. |

Orders 3 and 4 may change after bottleneck measurement. These are recommendations, not milestone points. Before implementation, select one current-board delivery and re-estimate exposed prerequisites; do not add 5/6 as M4 requirements.

Internal analysis alone need not change syntax or ABI. Public reports, root manifests, persisted profiles/caches, new reference categories, or changed native/portable records require explicit compatibility decisions. Keep component C v1's existing 32-entry surface and handle meanings intact; incompatible extensions need a new version/namespace. Unsupported new artifacts must be rejected before execution and rebuilt/re-encoded from source with matching tools. Do not serialize host pointers or trace-descriptor addresses.

Alternatives remain meaningful: no change preserves the strongest baseline; a library/tool solution can provide reports or existing Bytes-based snapshots without new memory semantics; a simpler restricted bridge may suffice instead of cooperative collection. The strongest competing designs—universal tracing, Goose-style whole-program specialization, or a Graal/Truffle runtime—have materially larger semantic and implementation costs without a measured project-specific advantage.

Blocking design questions are the first artifact's root/export set and native-record mapping, complete cache binding assumptions, allocator observability if elision is attempted, and a concrete traced workload/cross-cycle policy. Rich LSP presentation, PGO, immutable precomputation, moving GC, and image layers are optional follow-ups, not prerequisites to the first experiment.

## Source registry

All sources were checked on **2026-10-30 JST**. Repository dates below are the last file-changing Git commit through the named snapshot, not a claim that every paragraph was freshly verified on that date. A snapshot date is not substituted for an unavailable document publication/update date. Earlier local investigations have been summarized above rather than used as required references.

| ID | Primary/project source | Publication/update information |
| --- | --- | --- |
| G1 | [Goose specification][G1] | No separately published document-update date; repository snapshot `cce94b9`, committed 2026-10-27 23:23 UTC (2026-10-28 JST); v4 draft. |
| G2 | [Goose implementation][G2], [README][G2a] | No separately published document-update date; same pinned snapshot date as G1. |
| G3 | [Goose benchmark results][G3], [summary][G3a] | Main table measured 2026-10-16; both files last changed 2026-10-25 08:10:53 UTC. No lang comparison was run. |
| S1 | [Native Image Basics][S1] | Publication/update dates not displayed; fetched 2026-10-30 JST. |
| S2 | [Native Image Build Report][S2] | Publication/update dates not displayed; fetched 2026-10-30 JST. |
| S3 | [Kotlin/Native ARC integration][S3] | Page last-modified label 2025-04-17; fetched 2026-10-30 JST. |
| S4 | [Wasmtime Rooted][S4] | Publication/update dates not displayed; fetched 2026-10-30 JST. |
| S5 | [LLVM statepoints][S5] | Living documentation, no published update date; checked in the memory-domain investigation on 2026-10-30 JST. |
| L0 | [Architecture](architecture.md), [roadmap](roadmap.md) | Both last changed 2026-10-29 JST through `df55f6e`. |
| L1 | [RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md) | Last changed 2026-10-28 JST through `df55f6e`. |
| L2 | [RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md) | Last changed 2026-10-20 JST through `df55f6e`. |
| L3 | [RFC 0013](rfcs/0013-service-byte-work-budget.md) | Last changed 2026-10-24 JST; whole-unit admission amendment dated 2026-10-19. |
| L4 | [RFC 0024](rfcs/0024-send-share-and-service-authority.md) | Last changed 2026-10-27 JST through `df55f6e`. |
| L5 | [RFC 0033](rfcs/0033-component-v1-compatibility.md) | Last changed 2026-10-27 JST through `df55f6e`. |
| L6 | [RFC 0039](rfcs/0039-package-editor-snapshots.md) | Last changed 2026-10-29 JST through `df55f6e`. |
| GraalVM study | [Repository GraalVM study](graalvm-study.md) | Last changed 2026-10-30 01:14 JST, `d2cd8ba`; includes additional pinned Graal sources and experiment provenance. |

[G1]: https://github.com/aardappel/goose/blob/cce94b919d27458856ae2685974049eac78debcf/docs/goose_spec.md
[G2]: https://github.com/aardappel/goose/blob/cce94b919d27458856ae2685974049eac78debcf/docs/implementation.md
[G2a]: https://github.com/aardappel/goose/blob/cce94b919d27458856ae2685974049eac78debcf/README.md
[G3]: https://github.com/aardappel/goose/blob/cce94b919d27458856ae2685974049eac78debcf/bench/results.md
[G3a]: https://github.com/aardappel/goose/blob/cce94b919d27458856ae2685974049eac78debcf/bench/summary.md
[S1]: https://www.graalvm.org/latest/reference-manual/native-image/basics/
[S2]: https://www.graalvm.org/latest/reference-manual/native-image/overview/build-report/
[S3]: https://kotlinlang.org/docs/native-arc-integration.html
[S4]: https://docs.wasmtime.dev/api/wasmtime/struct.Rooted.html
[S5]: https://llvm.org/docs/Statepoints.html
