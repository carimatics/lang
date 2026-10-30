<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0042-signed-toolchain-installation.md -->

# RFC 0042: Signed toolchain installation

[日本語版](../ja/rfcs/0042-signed-toolchain-installation.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-30
- Last updated: 2026-10-30
- Tracking issue: M4-F01–F05
- Supersedes: none
- Superseded by: none

## Summary

Add an explicit-root toolchain manager that authenticates release metadata before reading payloads, installs actual compiler/SDK files, selects a channel/version and preserves the active installation on failure or process interruption. This distributes toolchains for M4; it does not implement M5's package registry.

## Motivation

The current native SDK can be assembled and tested without a source checkout, but there is no signed distribution, persistent channel selection or recoverable update command. A new developer needs the same verified compiler and SDK as the source build, with a clear initial trust decision and no implicit shell/global configuration changes.

## Goals

- Strict signatures, repository/channel/target binding, expiry, rollback floors and exact payload digests before publication.
- Isolated installation of the actual compiler and matching SDK on macOS AArch64 and Windows x86-64 MSVC.
- Install/update/select/list/run/which/uninstall/recover with finite storage and coherent activation.
- Local signed distribution and bounded HTTPS transport, with real both-host fixtures.

## Non-goals

Automatic discovery of a trust root, an official production signing identity, in-place trust-root rotation, package registry signing, package installation scripts, arbitrary archive extraction, OS code signing/notarization, self-replacing a running manager, implicit PATH/shell edits, hostile local administrators, and power-loss durability guarantees are outside this initial profile. No test key is a production trust root. Existing installed tools do not become unusable merely because distribution metadata expires.

## Guide-level design

The bootstrap manager and root file must arrive through an independently trusted channel. A signature verified using a key obtained from the same untrusted download does not establish that initial trust. An explicitly supplied root file is local trust configuration, not a self-authenticating downloaded certificate.

```text
lang toolchain identity
lang toolchain init --root DIR --trust-root FILE
lang toolchain verify --trust-root FILE --metadata FILE --channel stable
lang toolchain install --root DIR --source DIRECTORY_OR_HTTPS_BASE --channel stable [--version 0.1.0]
lang toolchain update --root DIR --source DIRECTORY_OR_HTTPS_BASE --channel stable
lang toolchain list --root DIR
lang toolchain select --root DIR --channel stable --version 0.1.0
lang toolchain which --root DIR
lang toolchain run --root DIR -- --version
lang toolchain uninstall --root DIR --channel stable --version 0.1.0
lang toolchain recover --root DIR
```

Each command supports the ordinary human/JSON diagnostic selection. Installation operations name their root; no ancestor, environment or global default discovery occurs. `identity` only returns the current compiler's closed schema-1 `schema`, `version`, `target` and `compiler_build_id` record and needs no installation root. `init` exclusively creates a new root and refuses an existing destination. Install selects the requested/current signed version and activates it only after complete verification. Update selects the signed current version. Select uses an already installed version and never lowers a channel's metadata floor. Uninstall refuses the active version. `which` returns the active compiler path; `run` invokes that verified executable, forwards explicit arguments and returns its status. It does not grant capabilities to guest programs.

The three channel names are `stable`, `beta` and `nightly`. Version labels use the existing canonical three-component unsigned package version syntax; channel and build identity distinguish experimental releases without introducing another version grammar. Selecting a version is exact, not a range resolution operation.

## Reference semantics

### Trust and signed metadata

The schema-1 root is a closed JSON object with `schema`, `repository`, `expires`, `threshold` and `keys`. Repository names are 1–128 ASCII letters/digits/dot/underscore/hyphen. A key has `id` (lowercase SHA-256 of its 32 public bytes) and `public_key` (64 lowercase hexadecimal characters). Accept 1–8 distinct nonweak Ed25519 keys and a threshold in 1..=key_count. Root input is at most 64 KiB. Persist its exact bytes and SHA-256 once; all subsequent state binds that identity. Renewing/replacing this root requires explicitly bootstrapping a fresh isolated root, not silently resetting an existing installation's rollback history. Automatic root succession is a future format change.

A schema-1 envelope has exactly `schema`, `payload` and `signatures`. Payload is a UTF-8 JSON string. A signature has `key_id` and `signature` (128 lowercase hexadecimal characters). Verify distinct authorized signer IDs with Ed25519 strict verification over `b"lang-toolchain-channel-v1\0"` followed by the exact payload UTF-8 bytes. No JSON reserialization occurs before verification. Refuse unknown/duplicate signer IDs, malformed encodings, weak keys, invalid signatures and an unmet threshold. At most eight signatures and 1 MiB envelope bytes are accepted. This is a project format, not a TUF wire implementation.

The closed payload has `schema`, `repository`, `channel`, `target`, `sequence`, `issued`, `expires`, `current_version` and `releases`. All integers are unsigned; sequence is positive. Times are integral Unix seconds, at most 253402300799; require issued <= current system time < expires and a metadata lifetime of at most 90 days. The root must also be unexpired. Production commands use the host clock, with a single captured time for admission and a fresh expiry check before activation. Test clocks are injectable in library tests, not a CLI bypass.

The target must be the current supported host triple. The repository must equal the root's repository. Channel must equal the explicit request. A catalogue contains 1–64 releases with distinct versions and includes current_version. A release has `version`, `compiler_build_id` and `files`. The build ID is 64 lowercase hexadecimal characters. A file has `path`, `bytes`, `sha256` and `executable`. Across the catalogue accept at most 4,096 file records; each release contains 1–4,096 files, at most 1 GiB total, and no single file exceeds 1 GiB. Sums use checked arithmetic. File SHA-256 strings are lowercase 64-character hexadecimal.

Paths are relative UTF-8 ASCII slash paths, at most 240 bytes, whose components use letters/digits/dot/underscore/hyphen. Refuse empty/dot/dot-dot components, absolute paths, backslashes, colons, trailing dots, Windows reserved device stems, case-fold duplicates and file/directory prefix collisions. The compiler is `bin/lang` or `bin/lang.exe` according to the host; the matching SDK is rooted at `sdk/`. Exactly that compiler and the SDK's matching `bin/lang[.exe]` copy have executable=true; their digests must match. SDK compatibility remains its original exact-build contract; the admitted SDK target/build ID and installed compiler identity must agree with the signed release. Check all candidate file bytes before invoking its bounded `toolchain identity` subprocess, and verify file permissions, platform formats and required SDK files before activation. Never execute an unverified payload.

### Admission, transport and rollback

Metadata lives at `channels/<channel>/<target>.json`; payloads are content-addressed `objects/<sha256>` files. Metadata never supplies arbitrary network URLs. The caller supplies either an explicit local distribution directory capability or an HTTPS base with no credentials, query or fragment. Use certificate-verified HTTPS, no redirects, no transparent content decoding and finite connect/overall timeouts (10/120 seconds per request). Read at most the admitted byte limit plus one; reject extra or missing bytes. Local reads use bounded regular-file checks and refuse symlink escapes, FIFOs/devices and directories. A valid signature is required before any payload is opened/downloaded.

State retains the highest committed sequence and exact payload digest per channel. Lower sequences refuse; equal sequence requires the same payload digest. A new authenticated metadata record is retained and its floor atomically committed before payload transfer. Therefore a failed payload install may advance the authenticated floor, while leaving the active version unchanged. An explicit version must exist in the newly admitted catalogue. The read-only `verify` command inspects metadata in a fresh trust context, without an installed-state rollback floor, state publication or payload reads. Its output must identify that scope; it does not authorize an existing installation update. Install/update always use the persisted floor. F02's verifier API takes an explicit prior admission record, so rollback/equal-sequence tests do not depend on the later filesystem transaction implementation.

The state retains a time floor from successful admissions and refuses a clock earlier than that floor for new metadata admission. This does not establish correct wall-clock time on a fresh machine or resist a malicious writer of the local installation root. Expired metadata/root prevents new install/update admission; previously admitted installed releases can still be selected and run after their signatures and bytes are rechecked. Same-sequence retry permits recovery from failed transfer. Metadata retention is bounded by installed receipts and the three current channel records, not an unbounded history.

### Publication, concurrency and recovery

The installation root is trusted local state owned by the invoking user. Keep all temporary and final files on its filesystem. Use a dedicated regular lock file and the stable standard library's nonblocking exclusive file lock; contention is an explicit refusal, not an unbounded wait. Hold it for state-changing operations and for selecting/verifying a child executable. Release it before waiting for a child process. An already running executable may make later Windows removal fail; report that failure and preserve a coherent installation record.

Prepare exclusive private staging directories; stream/hash payloads and check the full manifest and matching SDK before publication. Name immutable generations by a SHA-256 of the typed release's canonical compact JSON representation. Existing generations require complete byte/permission comparison; their name alone is never a cache authenticity check. Persist signed metadata/receipt evidence alongside the generation. Bound the local state to 1 MiB and 64 installed releases. Refuse reaching capacity instead of evicting active/installed versions implicitly.

The closed schema-1 state binds repository/root digest/host target, three channel floors, installed release identities and an optional active identity. Write complete temporary state, sync it and replace the prior state file atomically on the same filesystem. A failed replacement preserves the old state. Generation publication precedes activation. Process interruption before activation therefore leaves the old active state and possibly an unreferenced candidate; recovery deletes only recognized unreferenced manager staging/generations after acquiring the lock and never adopts unsigned leftovers. Refuse malformed state instead of resetting rollback floors. The exact state/receipt record fields are an F03 elaboration before implementation, within these bounds and invariants.

Uninstall first moves an inactive generation to private trash without changing its bytes, then commits removal from state, then deletes trash. If state commit fails, restore the original directory; recovery can restore referenced trash or discard unreferenced trash from a bounded transaction marker. A failed Windows rename leaves state unchanged. Transaction markers must bind the original generation and intended operation; never follow arbitrary paths from a marker. The precise marker/failure checkpoints belong in F03/F04 before implementation. The contract covers injected failures and process interruption; filesystem/hardware power-loss behavior is not promised beyond successful sync/atomic operations.

## Execution modes

This is a trusted host CLI. Installed VM/native execution retains existing limits, grants and source semantics. It does not execute package installation hooks, introduce a guest network capability or change JIT/component/embedding execution. The native SDK remains usable only under its existing exact compiler/rustc/profile checks.

## ABI and compatibility

Root, channel envelope/payload, state and receipts are independently versioned new host records. Unknown versions/fields and duplicates refuse. Source syntax, editions, MIR, bytecode, package locks, SDK and public C ABI are unchanged. Existing source checkouts need no migration. There is no automatic import from an arbitrary tool directory or a different trust root; initialize a separate root and reinstall from authenticated metadata.

## Diagnostics and tooling

Use structured installer diagnostics with ordinary exit 2 for misuse and exit 1 for admission/transaction failure. Do not print signing secrets. JSON output reports exact repository/channel/version/target/generation identity and active state, including whether an authenticated floor advanced on a failed transfer. LSP/DAP and compiler diagnostics are unaffected. `run` preserves child arguments/output/status and reports spawn failures separately. Provide a release-fixture producer that takes externally supplied signing material; no private keys enter version control or published fixture roots.

## Performance and cost model

Signature verification is bounded by eight keys/signatures and 1 MiB metadata. Transfer is streaming, bounded by signed sizes and timeouts; candidate disk use includes one release plus retained installed versions and bounded recovery trash. Rechecking installed hashes costs file I/O and is explicit, not a claim of zero-work cache reuse. File-set transport avoids an untrusted archive parser but can require one request per file. No performance comparison is established by the initial signature probe.

## Security and trust boundaries

The attacker controls distribution metadata/payloads and can replay, mix, truncate or withhold them. Trust comes from the explicitly bootstrapped root and monotonic committed state. Reject authenticity/expiry/target/path/digest errors before activation; do not weaken a check to recover availability. A compromised signing threshold can authorize malicious tools; a malicious local administrator or incorrect initial trust/clock remains outside this guarantee. Recovery is not a trust reset. The producer and private keys are trusted host operations; ordinary scripts receive no additional authority.

## Alternatives

- No manager retains manual trusted SDK assembly but does not provide the requested update/channel/recovery journey.
- An external package manager can supply bootstrap trust but cannot define this compiler's exact SDK/receipt identity by itself.
- Full TUF provides multi-role delegation and root rotation. It is the stronger long-term update design; adopting its complete wire/workflow now would add roles beyond this initial single-repository catalogue. This profile explicitly does not claim TUF compatibility or equivalent compromise recovery.
- An unsigned checksum list is simpler but does not authenticate a malicious distribution; blindly replacing the active directory also fails interruption recovery.

## Implementation plan

F01 accepts this design and reconciles the signature/file-lock experiment. F02 implements strict root/envelope/payload verification, expiry/floors/target/digests and malformed-input tests before transfer. F03 elaborates finite state/receipts and implements one real isolated install and interruption-preserving activation. F04 elaborates recovery markers and completes update/select/list/run/which/uninstall/recover. F05 creates ephemeral signed distributions of actual tools/SDKs and executes install/update/failure journeys on both supported hosts. Keep the existing five three-point estimates; expose and re-estimate a prerequisite if implementation exceeds its stated outcome. G9 stays open until the actual journey succeeds.

## Test plan

Use RFC 8032 public verification vectors, ephemeral signing keys, threshold/duplicate/weak-key/tamper/version/expiry/rollback/clock/target/count/overflow/path refusals and bounded reader fuzzing. Verify no payload I/O before signature admission. Run the real installed compiler and SDK, then update and select another signed version. Inject failures before/after metadata-floor commit, payload verification, generation publication, state replacement and uninstall rename/commit; prove the old active executable remains usable and recovery is deterministic. Exercise nonregular input, symlink/case/prefix collisions, concurrent manager contention and Windows executable locking. Test both local transport and HTTPS policy/limits; no live production signing credential is needed. Run repository baseline and both-host CI for implementation.

## Open questions

Accepted under RFC 0001 experimental delegation, 2026-10-30 JST. The isolated Rust 1.99/ed25519-dalek 3.0.0 experiment accepts RFC 8032 test 1 and refuses changed message/signature, noncanonical scalar and weak key; macOS file-lock contention/release succeeds. These establish library feasibility, not a completed installer. Windows atomic replacement/locking, HTTPS boundaries, actual release/SDK reconciliation and all interruption checkpoints remain explicit F02–F05 validation assumptions. Root replacement intentionally uses a fresh explicitly trusted installation; automatic rotation requires a later compatible design decision. No implementation gate is closed by this acceptance.

## Sources

- [TUF specification 1.0.37](https://theupdateframework.github.io/specification/latest/), updated 2026-10-08; verified 2026-10-30 JST. Signature thresholds, bounded metadata, expiry, rollback and root succession inform the threat model; this format is not TUF.
- [RFC 8032](https://www.rfc-editor.org/rfc/rfc8032), published 2017-01; verified 2026-10-30 JST. Ed25519 verification and public test vectors.
- [ed25519-dalek 3.0.0 VerifyingKey](https://docs.rs/ed25519-dalek/3.0.0/ed25519_dalek/struct.VerifyingKey.html), release 2026-09-21; verified 2026-10-30 JST. Strict verification and weak-key refusal.
- [ureq 3.5.0 configuration](https://docs.rs/ureq/3.5.0/ureq/config/struct.ConfigBuilder.html), release 2026-10-21; verified 2026-10-30 JST. Candidate bounded synchronous HTTPS transport; executable transport validation remains open.
- [Rust 1.99 File locks](https://doc.rust-lang.org/1.99.0/std/fs/struct.File.html#method.try_lock) and [rustup channels](https://rust-lang.github.io/rustup/concepts/channels.html): page update dates unavailable; verified 2026-10-30 JST.
- Existing native SDK snapshot/publication and package admission APIs inspected 2026-10-30 JST; source has no embedded update date. Wire fields, limits and CLI/recovery policy above are project decisions.
