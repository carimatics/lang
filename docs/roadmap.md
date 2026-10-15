<!-- canonical-language: en; japanese-translation: ja/roadmap.md -->

# Implementation roadmap

[日本語版](ja/roadmap.md)

The roadmap is ordered by risk reduction. Milestones are exit criteria, not calendar commitments.

## Milestone 0: design baseline

Current progress: the first typed function now runs through both verified bytecode and a Cranelift JIT prototype. A minimal typed `i64` host-function registration and script-to-host callback also run through both modes. RFC 0001 remains proposed, and embedding resource controls, memory-model stress cases, cross-platform evidence, and backend measurements remain open.

- Accept goals, non-goals, and terminology.
- Establish the RFC process and compatibility labels.
- Prototype three memory-model stress cases: cyclic graphs, async borrows/cancellation, and shared mutable services.
- Prototype a typed function through both bytecode and native lowering.
- Select initial tier-1 architecture pairs and linker strategy.

Exit: written semantics for the prototype subset and benchmark evidence for the initial backend choice.

## Milestone 1: frontend and bytecode vertical slice

- Lossless parser with error recovery and source spans.
- Name resolution, modules, primitive types, functions, records, variants, and pattern matching.
- Structured diagnostics with stable codes and fix-its.
- Typed MIR with validation.
- Register bytecode, verifier, VM, REPL/script runner, and host function registration.
- Formatter and parser/formatter round-trip tests.
- Initial incremental query graph shared with an LSP prototype.

Exit: small programs run as scripts with deterministic behavior and useful IDE diagnostics.

## Milestone 2: ownership and native AOT

- Owned values, moves, lexical borrows, deterministic destruction, and narrow unsafe operations.
- Allocator-aware standard containers.
- C ABI import/export and generated headers.
- Cranelift object generation for macOS AArch64 and Windows x86-64.
- Differential VM/native execution suite.
- Native debug metadata sufficient for breakpoints and stack traces.

Exit: representative command-line programs match VM semantics and performance is measured against equivalent Rust/C implementations.

## Milestone 3: concurrency and embedding

- Structured tasks, cancellation, channels, actor isolation, and data-race capability rules.
- Stable C embedding API and generated bindings.
- Versioned component IDL, resource handles, and capability imports.
- Mixed native/VM calls, callbacks, errors, stack traces, and cancellation.
- Script resource limits and interruption.
- Trusted native and verified script plugin examples.

Exit: a native host can load a user script, expose a typed API, call the script, receive callbacks, and enforce capabilities without IPC.

## Milestone 4: developer toolchain

- Complete LSP features for navigation, completion, rename, semantic tokens, and code actions.
- DAP adapter with mixed native/VM stacks.
- `lang` build/package/test/fmt/lint/doc commands.
- Manifest and lockfile with deterministic resolution, checksums, offline mode, and target conditions.
- Compile-time and runtime profiling, allocation traces, and rebuild explanations.
- Install/update/channel tool comparable in role to `rustup`.

Exit: a new developer can install one tool, create a package, develop in an editor, test, debug, and produce a reproducible binary.

## Milestone 5: ecosystem and compatibility

- Editions, deprecation policy, migration/fix tool, and compatibility test matrix.
- Signed release manifests and package provenance.
- Documentation hosting and package metadata search.
- Public registry threat model before accepting uploads.
- WebAssembly component target for sandboxed portable plugins.
- Evaluate LLVM release backend using published benchmarks and debug-quality criteria.

Exit: the language can evolve without silently breaking source, package, bytecode, or component consumers.

## Milestone 6: self-hosting

- Define and freeze a self-hosting subset.
- Implement frontend and core compiler stages in the language.
- Stage 0 Rust compiler builds stage 1; stage 1 builds stage 2.
- Compare reproducible outputs on both tier-1 platforms.
- Document bootstrap trust and disaster-recovery procedures.

Exit: stage 2 passes the same conformance suite as stage 0 and can build the supported toolchain from a documented bootstrap chain.

## Milestone 7: additional frontends

- Review whether a distinct high-level or domain-specific language has requirements that cannot be served by libraries, profiles, or syntax sugar.
- Keep a frontend-specific HIR and lower through the shared MIR only for semantics the core can represent faithfully.
- Use the component ABI when memory, effect, or runtime models differ.
- Add cross-language debugging, package metadata, and conformance tests before declaring interoperability stable.

Exit: a second frontend adds clear user value without weakening the first language’s safety or making the shared IR an unmaintainable union of unrelated semantics.

