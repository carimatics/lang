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
  - `docs/architecture-history.md` and `docs/ja/architecture-history.md`
  - `docs/architecture-m3-history.md` and `docs/ja/architecture-m3-history.md`
  - `docs/roadmap.md` and `docs/ja/roadmap.md`
  - `docs/roadmap-history.md` and `docs/ja/roadmap-history.md`
  - `docs/benchmarks.md` and `docs/ja/benchmarks.md`
  - `docs/milestone-1.md` and `docs/ja/milestone-1.md`
  - `docs/milestone-2.md` and `docs/ja/milestone-2.md`
  - `docs/milestone-3.md` and `docs/ja/milestone-3.md`
  - `docs/milestone-4.md` and `docs/ja/milestone-4.md`
  - `docs/milestone-3-history.md` and `docs/ja/milestone-3-history.md`
  - `docs/milestone-2-capabilities.md` and `docs/ja/milestone-2-capabilities.md`
  - `docs/milestone-3-capabilities.md` and `docs/ja/milestone-3-capabilities.md`
  - `docs/milestone-4-capabilities.md` and `docs/ja/milestone-4-capabilities.md`
  - `docs/native-sdk.md` and `docs/ja/native-sdk.md`
  - `docs/review-2026-10-17.md` and `docs/ja/review-2026-10-17.md`
  - `docs/review-2026-10-20.md` and `docs/ja/review-2026-10-20.md`
  - `docs/review-2026-10-27.md` and `docs/ja/review-2026-10-27.md`
  - `docs/review-2026-10-28.md` and `docs/ja/review-2026-10-28.md`
  - `docs/review-2026-10-30.md` and `docs/ja/review-2026-10-30.md`
  - `docs/spec-improvement-study.md` and `docs/ja/spec-improvement-study.md`
  - `docs/rfcs/0000-template.md` and `docs/ja/rfcs/0000-template.md`
  - Every numbered RFC in `docs/rfcs/` and its same-named mirror in `docs/ja/rfcs/`
  - `.agents/skills/language-design-rfc/SKILL.md` and `docs/ja/codex/language-design-rfc.md`

## Start here

- Read `README.md`, `docs/architecture.md`, and `docs/roadmap.md` before proposing structural compiler changes.
- Read relevant accepted RFCs before changing language semantics, memory rules, concurrency, ABI, or serialized formats.
- Use the `language-design-rfc` skill for language-feature proposals and reviews.
- Preserve user changes and keep each patch focused on the requested work.

## Branch and pull request workflow

Policy updated: 2026-10-28 JST.

- Apply this workflow to each work unit: a single prompt or the related tasks in a Codex Goal. Record the starting branch, its remote/upstream, and existing working-tree changes before starting; preserve user changes.
- Do not edit, stage, or commit Git-managed files on `main`, including additions intended for version control, deletions, and generated tracked files. Create a work branch before making such changes. Branches created by Codex must use a descriptive `codex/*` name.
- Scope branches to coherent, reviewable outcomes. Keep related tasks on the same branch; do not create a branch mechanically for every prompt continuation, Goal subtask, delivery row, or commit. Multiple branches are allowed when their outcomes or dependencies justify them. For each branch, record the branch it was created from and the remote repository as its pull request base.
- After completing a work branch, run the relevant local checks, commit and push the changes, and open a pull request against its recorded base branch. Complete this lifecycle within the requested scope without asking for routine reconfirmation; respect actual permission barriers and repository protections.
- Before merging, confirm that all applicable pull request CI checks succeeded for the latest head commit. Pending, failed, cancelled, or unexpectedly missing checks are not success. Existing intentional path/job conditions still apply; verify why a check is omitted rather than treating an unexplained absence as a pass. A local check or successful run for an older commit does not replace this gate.
- Diagnose and fix CI failures, push the fixes, and verify CI again. Do not skip or delete failing tests, weaken assertions or CI coverage, or add superficial hacks solely to make checks pass. Resolve the underlying defect; retry a transient infrastructure failure only with a supported diagnosis.
- Merge through the remote pull request after CI succeeds and repository requirements are satisfied. Confirm that the pull request is merged, then fetch and update its local base branch to the remote's merged state. Synchronizing `main` with already merged remote commits is allowed; direct edits, local merge commits, and direct pushes to `main` are not part of this workflow.
- Before declaring the work unit complete, switch back to the branch recorded at its start, fetch its remote, and verify that local and remote tips match with no ahead/behind commits and no uncommitted task changes. Use fast-forward updates where possible; never discard user changes or force-reset divergent history to manufacture synchronization.
- If permissions, CI, conflicts, missing remote configuration, or unrelated local changes prevent completion, preserve the work and report the branch, pull request, and unresolved condition. Do not claim merge or synchronization completed. See [CONTRIBUTING.md](CONTRIBUTING.md#branch-to-merge-procedure) for the procedure and its relationship to existing rules.

## Repository-scoped approvals

Policy updated: 2026-10-28 JST.

- Manage persistent approvals granted for work in this repository at repository scope by default, with separate storage for each approval type. Preserve the scope the user approved; a one-time approval does not authorize a persistent rule or a broader command, tool, app, or server allowance.
- Store shell-command approvals in `.codex/rules/user.rules` using the supported `prefix_rule` syntax. Keep this personal file ignored by Git. Do not put app or MCP approval settings in a `.rules` file.
- Store app and MCP tool approvals in the repository's `.codex/config.toml`, using the documented tool-specific `approval_mode` setting, such as `apps.<id>.tools.<tool>.approval_mode` or `mcp_servers.<id>.tools.<tool>.approval_mode`. Prefer the individual approved tool over an app-wide or server-wide allowance.
- Approval dialogs may save settings to the user layer instead. After persisting an approval, inspect the actual destination. Move a newly saved repository-only approval to the appropriate repository file and remove its user-level duplicate only after verifying the local entry. Preserve unrelated settings and pre-existing shared approvals; do not infer that an older global approval belongs exclusively to this repository. Respect permission barriers when editing user-level settings and report any migration that could not be completed.
- Keep `.codex/config.toml` untracked and ignored as each checkout's personal repository configuration; preserve its contents and do not force-add it. Keep shareable starting defaults in `.codex/config.example.toml`, which Codex does not load automatically. Create a personal config from that example only when no config exists; never overwrite existing settings or approvals. See [personal repository configuration](CONTRIBUTING.md#personal-codex-configuration) for setup and migration.
- Keep personal approval entries out of commits and out of the shared example. Commit approval defaults only when the user explicitly intends to establish a shared repository policy. Never include credentials or tokens.
- Project-local config and rules require a trusted project. Their scope controls when settings load; it does not constrain a GitHub tool's target repository or a command's remote destination. Continue to target the repository authorized by the task explicitly.
- Before reporting completion of approval management, verify the destination, syntax, and intended scope, and check that newly created repository-only approvals were not left in the user layer. Report persistent local settings separately from uncommitted task changes; do not claim that storing a rule proves it has been reloaded into an already running session.

Sources: [OpenAI Rules](https://developers.openai.com/codex/rules), [Config basics](https://developers.openai.com/codex/config-basic), and [Configuration Reference](https://developers.openai.com/codex/config-reference). Publication/update dates are not provided; verified 2026-10-28 JST.

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

- Use the current milestone's delivery board when that milestone is activated. The [M2 delivery board](docs/milestone-2.md) is a completed record, not the queue for new work. Review and maintenance work can use a scoped checklist in its review report; do not reopen a completed milestone or invent effort points for routine fixes.
- Before implementing an active milestone, break its next outcome into observable 1–3-point deliveries and select one doing row. Keep at most one implementation delivery doing. Expose and re-estimate a new prerequisite before implementing it; never repeatedly retain an estimate without checking the remaining task list. The roadmap's broad exit-gate weights are not a remaining-effort forecast.
- If planning produces no patch or executable experiment within 10 minutes, narrow the task and run the smallest useful experiment before further design. Add abstractions/hardening only for the selected outcome or accepted requirements.
- Update the bilingual board with implementation/validation evidence after each delivery. Evidence cells contain only the code commit hash; put validation details in the delivery note. Keep exit-gate completion separate from completed prerequisites. Prefer the next source execution fixture over additional horizontal scaffolding; review the sequence after two prerequisite deliveries without a new execution fixture.
- If a doing row remains open across a work session, show its last executable checkpoint, verification time, and remaining acceptance conditions at the top of the board. A checkpoint does not count as a completed delivery. Report completed work and newly added work separately, and re-audit an item that grows beyond its stated outcome before adding more implementation.
- Keep planning updates short: update rows and current counts. Add narrative audits for changed scope, dependencies, estimates or exit decisions. Record current contracts in RFCs/architecture and retain dated implementation checkpoints in the board or review history. Update code, evidence and the plan together; a separate planning commit is not required. For documentation-only planning updates, run `cargo test --test repository_consistency`; use this existing validator rather than another unchanged full compiler baseline.
- Keep the board's estimate reconciliation current: baseline remaining + net added/re-estimated work − estimates retired at completion = remaining. Report completed deliveries separately from newly exposed tasks. Use each row's last remaining estimate when it becomes done; change net additions only with an explicit scope/estimate audit. If a new row is first committed as done, preserve its estimate in the audit and include it in both additions and completed estimates. Points are planning units, not measured working time.

## Security

- Never commit credentials, tokens, private keys, or real user data.
- Script and plugin code is untrusted by default. Require explicit capabilities for filesystem, network, environment, process, native FFI, and host API access.
- Do not use native in-process plugins as a sandbox. Reserve them for trusted code.
- Validate bytecode, component metadata, package manifests, and foreign inputs before execution.

