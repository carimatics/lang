---
name: language-design-rfc
description: Draft or review RFCs for this repository's language semantics, compiler IR, memory model, concurrency, ABI, runtime, and developer tooling. Use for user-visible or compatibility-sensitive design changes; do not use for routine implementation that follows an accepted design.
---

<!-- canonical-language: en; japanese-translation: ../../../docs/ja/codex/language-design-rfc.md -->

# Language Design RFC

Produce a decision-ready RFC or review that exposes semantic and implementation tradeoffs without presenting hypotheses as accepted behavior.

## Context

Read these files before drafting or reviewing:

- `README.md`
- `docs/architecture.md`
- `docs/roadmap.md`
- `docs/research.md` when comparing external designs
- Existing RFCs that affect the proposal

Start new RFCs from `docs/rfcs/0000-template.md`. Apply [RFC 0001's experimental delegation](../../../docs/rfcs/0001-project-charter.md#experimental-rfc-decisions): accept a reasonably sound design without individual user approval, record the date, rationale, and unverified assumptions, then continue implementation. Keep unresolved designs `proposed`. Revise the RFC explicitly before implementing a later design change.

## Required analysis

Cover the dimensions that the proposal actually affects:

- User-facing syntax and guide-level examples
- Static and dynamic semantics
- Type inference, traits, effects, and error behavior
- Ownership, allocation, destruction, aliasing, and unsafe invariants
- Data-race freedom, cancellation, atomics, and scheduling
- Bytecode VM and native AOT equivalence
- Embedding, FFI, component ABI, callbacks, and versioning
- Diagnostics, formatter, linter, LSP, DAP, and documentation
- Compile time, runtime, memory, binary size, and hidden costs
- Security boundaries, capabilities, malformed inputs, and resource limits
- Compatibility, editions, migration, and rollout
- Implementation stages and observable tests

State non-goals. Compare no change, a library/tool solution, the strongest competing design, and a simpler alternative.

## Evidence

Use primary sources for claims about current languages, standards, or tools. Record publication/update dates; when a living document has no published date, say so and record the verification date. Separate sourced facts from project choices and unresolved hypotheses.

Use small prototypes and benchmarks for claims about ergonomics, compile time, runtime performance, call overhead, or code size. Do not infer those properties from language popularity.

## Project invariants

- Keep the frontend independent from LLVM, Cranelift, VM, and platform-specific APIs.
- Preserve source spans and stable diagnostic identities through lowering.
- Treat typed MIR as the shared semantic contract between execution modes.
- Treat native source-language ABI as unstable; independently versioned components use the component interface.
- Default user-authored plugins to capability-restricted bytecode or WebAssembly, not trusted native loading.
- Do not add a second source language until the first frontend and component conformance suite are stable.

## Review outcome

End reviews with one of: accepted under experimental delegation, ready for decision, needs revision, needs prototype/measurement, or conflicts with an accepted invariant. Use delegated acceptance when the review supports it; do not queue routine owner approval. List blocking questions separately from optional follow-ups.
