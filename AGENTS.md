<!-- canonical-language: en; japanese-translation: AGENTS.ja.md -->

# Repository instructions

These instructions apply to the entire repository.

## Documentation languages

- English documents are canonical for implementation decisions and AI instructions.
- Japanese documents are human-readable mirrors and must not introduce additional requirements.
- `AGENTS.ja.md` is an informational translation; Codex must not treat it as another instruction source.
- Update both files in a translation pair in the same change. Preserve equivalent headings, links, examples, tables, status labels, and source dates.
- Translation pairs are:
  - `README.md` and `README.ja.md`
  - `AGENTS.md` and `AGENTS.ja.md`
  - `CONTRIBUTING.md` and `CONTRIBUTING.ja.md`
  - `docs/research.md` and `docs/ja/research.md`
  - `docs/architecture.md` and `docs/ja/architecture.md`
  - `docs/roadmap.md` and `docs/ja/roadmap.md`
  - `docs/benchmarks.md` and `docs/ja/benchmarks.md`
  - `docs/rfcs/0000-template.md` and `docs/ja/rfcs/0000-template.md`
  - Every numbered RFC in `docs/rfcs/` and its same-named mirror in `docs/ja/rfcs/`
  - `.agents/skills/language-design-rfc/SKILL.md` and `docs/ja/codex/language-design-rfc.md`

## Start here

- Read `README.md`, `docs/architecture.md`, and `docs/roadmap.md` before proposing structural compiler changes.
- Read relevant accepted RFCs before changing language semantics, memory rules, concurrency, ABI, or serialized formats.
- Use the `language-design-rfc` skill for language-feature proposals and reviews.
- Preserve user changes and keep each patch focused on the requested work.

## Research

- Confirm the current date and timezone before web research.
- Prefer specifications, official documentation, project maintainers, standards bodies, and original survey datasets.
- Record each source's publication/update date. If a living document does not publish one, state that and record the verification date instead.
- Separate sourced facts, project decisions, and design hypotheses.

## Rust

- Use stable Rust as pinned by `rust-toolchain.toml` and Rust 2024 edition conventions.
- Format with `cargo fmt`; lint with Clippy and treat warnings as errors.
- Avoid `unsafe`. When it is required, keep the block narrow, document the safety invariant, and add tests that exercise the boundary.
- Prefer explicit domain types and structured diagnostics over strings and booleans with overloaded meanings.
- Keep frontend semantics independent from a specific code-generation backend.
- Do not edit `Cargo.lock` manually.

## Compiler architecture

- Preserve source spans through syntax, HIR, MIR, bytecode, and native debug metadata.
- Keep lossless syntax data separate from semantic IR.
- Treat the typed MIR as the semantic contract shared by AOT and script execution.
- Do not expose LLVM, Cranelift, VM, or platform-specific types through frontend APIs.
- Treat serialized IR, bytecode, package metadata, and plugin interfaces as versioned formats with validation at trust boundaries.
- Avoid adding a second source language until the first frontend, shared ABI, and conformance suite are stable.

## Validation

Run the smallest relevant checks first, then the full baseline before finalizing code changes:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

Add focused tests for behavior changes. Compiler work should use the appropriate mix of parser snapshots, compile-pass tests, compile-fail diagnostics, MIR/bytecode snapshots, differential VM/native tests, fuzzing, and platform integration tests.

## Documentation and RFCs

- Update documentation in the same change as behavior or architecture.
- Use `docs/rfcs/0000-template.md` for changes that affect users or long-lived compatibility.
- Use the RFC 0001 lifecycle labels: proposed, accepted, implemented, stabilized, withdrawn, or superseded. Acceptance does not imply implementation or stabilization; record a rejected proposal as withdrawn with its decision rationale.
- Include migration and compatibility consequences for changes to syntax, ABI, package metadata, or serialized artifacts.

## Security

- Never commit credentials, tokens, private keys, or real user data.
- Script and plugin code is untrusted by default. Require explicit capabilities for filesystem, network, environment, process, native FFI, and host API access.
- Do not use native in-process plugins as a sandbox. Reserve them for trusted code.
- Validate bytecode, component metadata, package manifests, and foreign inputs before execution.

