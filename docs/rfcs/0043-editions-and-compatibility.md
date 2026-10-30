<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0043-editions-and-compatibility.md -->

# RFC 0043: Editions and compatibility policy

[日本語版](../ja/rfcs/0043-editions-and-compatibility.md)

- Status: accepted
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-30
- Last updated: 2026-10-30
- Tracking issue: M5-A01, M5-A02
- Supersedes: None; refines RFC 0001 and RFC 0034
- Superseded by: None

## Summary

Make the existing `2026` source edition explicit in a compatibility report and define how each compatibility domain may evolve. `lang compatibility --message-format json` reports this compiler's supported source edition and independent format/ABI policies. It performs no filesystem, network or guest-code I/O. Edition selection does not stabilize all current language behavior.

## Motivation

RFC 0034 already rejects unknown editions, but users cannot inspect the support policy in one place. A compiler version, source edition, lock schema and public C ABI are different identifiers; silently treating them as interchangeable can accept incompatible persisted inputs or overpromise native binary reuse.

## Goals

- Describe every existing compatibility boundary, with concrete admission and migration rules.
- Preserve existing edition-2026 manifests and historical public C clients.
- Define deprecation, future edition introduction and removal before a breaking transition.

## Non-goals

New source syntax, a fictitious successor edition, blanket pre-alpha stabilization, automatic rewriting, registry protocol or Wasm format selection. M5-B01 must accept the migration/fix transaction before implementation; this decision alone does not complete G2.

## Guide-level design

`lang compatibility` prints a human report; `--message-format human|json` selects the encoding. JSON schema 1 contains `schema`, `compiler_version`, `source_editions`, `default_source_edition`, and `domains`. Each domain entry has `domain`, `status` and `policy` string fields. Domain identifiers are `source`, `package`, `bytecode`, `native-abi`, `component-abi`, `tooling`, and `bootstrap`. Statuses are `experimental`, `stabilized` or `unavailable`; only the named public component C v1 interface is stabilized. Policies explain restrictions rather than implying that an entire domain is universally interchangeable. Unknown/duplicate options are usage errors (exit 2, `CLI0001`); JSON requests retain structured errors on stderr. Success writes only the report to stdout.

An explicit package edition is required; the default names the existing standalone source/REPL behavior, not permission to omit the manifest field. Dependency editions are selected by their own manifest, never inherited from the root. Unknown editions fail with `PKG0002`, including in inactive workspace catalogues under existing admission rules.

## Reference semantics

The only admitted edition is `2026`. Its selection changes no parsing, typing, evaluation, ownership, destruction, allocation, concurrency, capability, FFI or constant-execution behavior. Internally use a finite edition type for admission/reporting while preserving the current manifest representation and wire bytes.

For a future edition, an accepted RFC must identify exact changed syntax/semantics, specify per-package selection and mixed-edition boundary behavior, list supported source-to-target migrations and supply before/after VM/native tests. Existing editions keep their specified behavior for supported valid programs; a new edition must not reinterpret old source implicitly. Selecting a target never implies rewriting dependencies or changing package versions.

Deprecation policy:
1. Record domain, reason, affected valid programs, replacement, first diagnostic/toolchain release, earliest removal release and an explicit migration or refusal path in a bilingual RFC/change note.
2. A deprecated supported construct remains valid and emits a stable diagnostic with source span. A machine-applicable fix requires a proven semantics-preserving recipe and checked original input; ambiguous changes require human review. Formatting must not silently migrate semantics.
3. For a stabilized interface, provide at least two subsequent published toolchain releases with notice before any planned end of support; incompatible changes use a new version/namespace. The existing RFC 0033 C v1 preservation promise still applies: this notice period does not authorize removing v1.
4. Experimental changes also require a dated accepted RFC and migration/refusal tests; they do not acquire a fictitious two-release stability guarantee. Security corrections may immediately reject previously accepted malformed/unauthorized input, with the precise contract and observable consequence recorded.
5. No edition currently has an end-of-support date and no construct is newly deprecated by this RFC. Adding an actual deprecation requires the diagnostic and migration recipe together; a policy declaration is not evidence of a migration implementation.

## Execution modes

The shared frontend/MIR remains the semantic contract for VM/JIT/AOT. Source editions cannot relax execution capabilities or make a private native image portable. Existing standalone commands and source-package dependency fixtures retain identical behavior. Wasm and bootstrap remain unavailable until their own milestone evidence is supplied.

## ABI and compatibility

| Domain | Current admission / promise | Upgrade or refusal |
| --- | --- | --- |
| source | Required package edition 2026; standalone/REPL use 2026; experimental | Unknown edition refused; explicit accepted recipe for future transition |
| package | Manifest schema 1; lock schema 1 or workspace schema 2; exact version, target, toolchain and checksums | Replay exact frozen inputs or explicitly regenerate a lock; never guess unknown versions |
| bytecode | LANGBC schemas 1–5 with profile-specific verifier rules; experimental | Decode/verify the declared schema; rebuild from source for unsupported/incompatible inputs |
| native-abi | LANGNAT 1/2, program 1/2/3, SDK 1/2/3 admission remains profile-specific; Rust/private linkage requires matching target/build | Rebuild with matching compiler/SDK; version equality is not trust or permission |
| component-abi | Public C v1 stabilized by RFC 0033; IDL 1, LCMP 1/2/3, LCVL 1 and LCSG 1 remain separately versioned profiles | Preserve old C callers and generated clients; negotiate compatible contracts, refuse unsupported formats before effects |
| tooling | Package diagnostics schema 1, explicit CLI exit codes, versioned editor/debug/profile/distribution contracts; experimental | Preserve diagnostic identity within its accepted contract; revise schemas for incompatible shape changes |
| bootstrap | No supported self-hosting chain yet | M6 must freeze and verify a concrete subset and reproducible stages |

This table lists admitted families, not cross-profile conversions or a promise that every decoder accepts every historical schema. Existing exact profile validators remain authoritative. Package SemVer and edition labels do not negotiate C ABI, bytecode, SDK or host authority. The report deliberately gives policies rather than an unvalidated cross-product of version numbers.

## Diagnostics and tooling

Use the same finite edition admission in manifest validation and reporting. Maintain `PKG0002` for unsupported manifest schema/edition and existing span/error behavior. Report generation has fixed-size bounded data and does not invoke dependency resolution. Parser recovery, formatting, lint, LSP and DAP are unchanged. Consumers must reject unknown report schemas rather than interpret a later shape as schema 1.

## Performance and cost model

Report generation is constant in project size and uses small host-side strings/serialization. Edition admission is a finite match. No guest allocation, runtime dispatch, code-size or performance claim changes.

## Security and trust boundaries

Compatibility metadata grants no authority and authenticates no artifact. Unknown editions and formats fail closed. Keep all existing input-size, profile verification, exact-SDK, capability and signature checks. A report must not read manifests or credentials from ambient paths.

## Alternatives

- No change leaves the distinction between edition, release and format scattered across RFCs.
- Documentation alone defines policy but gives tools no inspectable supported-edition list.
- Automatic latest-edition selection would make an unchanged package silently change meaning.
- One global version is simpler, but conflates stable C callers with experimental private images and source semantics.
- Creating a breaking edition solely to demonstrate migration imposes user cost without a language requirement; keep a finite current edition and separately implement useful checked fixes.

## Implementation plan

A01 accepts this bilingual policy and activates the full M5 board. A02 adds the compatibility CLI and shared edition admission, verifies existing manifest/lock/artifact/old-C fixtures, and runs the required baseline and both-host CI. B01 separately specifies migration recipes and checked writes. Other M5 contracts remain pending.

## Test plan

Check exact JSON keys/domain order, human output, unknown/duplicate arguments and structured errors. Exercise the report's supported edition through real manifest admission and dependency execution; reject missing, numeric, unknown and future editions. Preserve lock replay/refusal, bytecode profile round trips and independent old C client execution in both-host CI. These checks validate current compatibility, not a claim that future editions or Wasm already run.

## Open questions

Accepted under RFC 0001's experimental delegation on 2026-10-30 JST. Rationale: explicit finite admission preserves current source and format behavior; separating compatibility domains prevents accidental ABI promises. No blocking design question for A02. Future edition semantics, concrete deprecations, registry/Wasm protocols and stabilized-source support lifetime remain unverified decisions for their own RFCs.

## Sources

- [RFC 0001](0001-project-charter.md), updated 2026-10-28: compatibility domains and experimental delegation.
- [RFC 0033](0033-component-v1-compatibility.md), updated 2026-10-27: exact public C v1 preservation and historical-client evidence.
- [RFC 0034](0034-package-workspaces-and-cli-diagnostics.md), updated 2026-10-29: required edition, manifest/lock admission and diagnostics.
- Repository manifest/lock validators and source/artifact tests: no embedded update date; inspected 2026-10-30 JST. This RFC makes project policy decisions, not external standards or performance claims.
