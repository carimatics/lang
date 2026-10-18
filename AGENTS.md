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
  - `docs/milestone-1.md` and `docs/ja/milestone-1.md`
  - `docs/milestone-2.md` and `docs/ja/milestone-2.md`
  - `docs/review-2026-10-17.md` and `docs/ja/review-2026-10-17.md`
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
- During experimental early development, apply the owner's 2026-10-17 JST delegation in [RFC 0001](docs/rfcs/0001-project-charter.md): accept reasonably sound RFCs without individual user approval, record the rationale and unverified assumptions, and continue work.
- Revise the affected RFC or write a follow-up with the change rationale and compatibility consequences before implementing a changed accepted design; update its translation pair together.
- Include migration and compatibility consequences for changes to syntax, ABI, package metadata, or serialized artifacts.

## Active milestone tracking

- Use the [M2 delivery board](docs/milestone-2.md) for implementation work; the roadmap's broad exit-gate weights are not a remaining-effort forecast.
- Before coding, select one observable 1–3-point delivery and mark it doing. Keep at most one doing row. Expose and re-estimate a new prerequisite before implementing it; never repeatedly retain an estimate without checking the remaining task list.
- If planning produces no patch or executable experiment within 10 minutes, narrow the task and run the smallest useful experiment before further design. Add abstractions/hardening only for the selected outcome or accepted requirements.
- Update the bilingual board with implementation/validation evidence after each delivery. Evidence cells contain only the code commit hash; put validation details in the delivery note. Keep exit-gate completion separate from completed prerequisites. Prefer the next source execution fixture over additional horizontal scaffolding; review the sequence after two prerequisite deliveries without a new execution fixture.
- Keep planning updates short: update rows and current counts. Add narrative audits for changed scope, dependencies, estimates or exit decisions. For documentation-only planning updates, run `cargo test --test repository_consistency`; use this existing validator rather than a separate partial check or another unchanged full compiler baseline.

## Security

- Never commit credentials, tokens, private keys, or real user data.
- Script and plugin code is untrusted by default. Require explicit capabilities for filesystem, network, environment, process, native FFI, and host API access.
- Do not use native in-process plugins as a sandbox. Reserve them for trusted code.
- Validate bytecode, component metadata, package manifests, and foreign inputs before execution.

