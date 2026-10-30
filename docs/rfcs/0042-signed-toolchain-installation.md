<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0042-signed-toolchain-installation.md -->

# RFC 0042: Signed toolchain installation

[日本語版](../ja/rfcs/0042-signed-toolchain-installation.md)

- Status: implemented
- Authors: project maintainers with Codex
- Created: 2026-10-30
- Last updated: 2026-10-30
- Tracking issue: M4-F01–F05
- Supersedes: none
- Superseded by: none

## Summary

Add an explicit-root toolchain manager that authenticates release metadata before reading payloads, installs actual compiler/SDK files, selects a channel/version and preserves the active installation on failure or process interruption. This distributes toolchains for M4; it does not implement M5's package registry.

Implementation verified 2026-10-30 14:57 JST: `bf14faf` passes [full CI](https://github.com/carimatics/lang/actions/runs/41009253145) (18 success, intentional documentation-only skip), [native-debug CI](https://github.com/carimatics/lang/actions/runs/41009253195) (3/3) and the complete local baseline (2,513 tests). Both hosts verify two-key distribution production, ordinary-installer pre-publication validation, real SDK native execution, all channels, historical receipt expiry and update while the old compiler runs; Windows also verifies live-executable uninstall refusal/deferred cleanup and recovery after exit. The update fixture changes actual SDK packaging bytes while retaining compiler identity. The acceptance-stage experiments and open-validation notes below are historical; this implementation evidence completes F01–F05. Implemented does not mean stabilized or establish an official distribution trust identity.

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

The installation root is trusted local state owned by the invoking user. Keep all temporary and final files on its filesystem. Use a dedicated regular lock file and the stable standard library's nonblocking exclusive file lock; contention is an explicit refusal, not an unbounded wait. Hold it for state-changing operations and for selecting/verifying a child executable. For `run`, release it after spawning the child and before waiting for its exit. Bounded candidate identity verification remains inside the locked installation transaction. An already running executable may make later Windows removal fail; report that failure and preserve a coherent installation record.

Prepare exclusive private staging directories; stream/hash payloads and check the full manifest and matching SDK before publication. Name immutable generations by a SHA-256 of the typed release's canonical compact JSON representation. Existing generations require complete byte/permission comparison; their name alone is never a cache authenticity check. Persist signed metadata/receipt evidence alongside the generation. Bound the local state to 1 MiB and 64 installed releases. Refuse reaching capacity instead of evicting active/installed versions implicitly.

The closed schema-1 state binds repository/root digest/host target, three channel floors, installed release identities and an optional active identity. Write complete temporary state, sync it and replace the prior state file atomically on the same filesystem. A failed replacement preserves the old state. Generation publication precedes activation. Process interruption before activation therefore leaves the old active state and possibly an unreferenced candidate; recovery deletes only recognized unreferenced manager staging/generations after acquiring the lock and never adopts unsigned leftovers. Refuse malformed state instead of resetting rollback floors. The exact state/receipt record fields are an F03 elaboration before implementation, within these bounds and invariants.

Uninstall first moves an inactive generation to private trash without changing its bytes, then commits removal from state, then deletes trash. If state commit fails, restore the original directory; recovery can restore referenced trash or discard unreferenced trash from a bounded transaction marker. A failed Windows rename leaves state unchanged. Transaction markers must bind the original generation and intended operation; never follow arbitrary paths from a marker. The precise marker/failure checkpoints belong in F03/F04 before implementation. The contract covers injected failures and process interruption; filesystem/hardware power-loss behavior is not promised beyond successful sync/atomic operations.

### Installed state and receipts

F03 elaboration, accepted under the same experimental delegation on 2026-10-30 JST: the closed schema-1 state fields are `schema`, `root_sha256`, `repository`, `target`, `time_floor`, `channels`, `installed` and `active`. Initially both arrays are empty, time_floor is zero and active is null. Channels contain at most three distinct `{channel, receipt}` entries. Installed contains at most 64 distinct channel/version entries `{channel, version, generation, receipt}`. Active is null or exactly one installed `{channel, version, generation}` identity. Each closed receipt is `{schema: 1, metadata_sha256, admitted_at}`. Times and hashes retain the metadata bounds. No state field supplies an arbitrary filesystem path.

Retain exact envelopes at `metadata/<metadata_sha256>.json` and the pinned root at `root.json`. On state load, check every referenced content hash and reauthenticate its envelope at the receipt's admitted_at time. Reconstruct the channel's prior Admission from that verified envelope rather than deserializing unchecked rollback values. Every receipt time is at or below time_floor; each installed release matches its authenticated generation and has a sequence no greater than its channel's current floor, with identical payload hash at an equal sequence. New admission also checks the global time floor. Historical verification at the recorded time permits already installed tools to survive expiry; production users cannot override admission time.

The manager-owned layout is `lock`, `state.json`, `root.json`, `metadata/`, `generations/`, `staging/` and `trash/`. State/root inputs must be regular files, and owned-state symlinks refuse. A single exclusive `staging/install` directory and `state.next` file reserve the transaction under the lock. Write/sync/close state.next before atomic replacement. An interrupted temporary file or staging directory refuses another mutation until recovery. Metadata is written and synced before state references it; existing metadata must match its exact digest. Bound metadata/generation directory entries to 128 each, with recovery before a capacity-limited retry. Refuse unknown entries instead of following or deleting arbitrary names. Installed receipts and current channel floors determine retained metadata; there is no unbounded history.

A generation contains exactly the signed files and required parent directories. Reconcile the original bounded SDK manifest's target/build ID and complete file table against the signed SDK files, then validate compiler platform format and permissions before the identity subprocess. The subprocess has a 10-second bound and 64 KiB per output stream. Its closed schema-1 identity must have a canonical compiler version and exactly the signed target/build ID. The manager's own build ID need not match the candidate: updates may cross compiler builds. Distribution version labels identify signed bundles and may change for SDK documentation or packaging while keeping compiler bytes unchanged; identity.version reports the compiler crate version, not that distribution label. Different labels alone do not establish different compiler code.

Installing a channel/version already bound to another generation refuses replacement; an identical retry is allowed after full verification. Generations may be shared across channels, so uninstall removes files only after the final installed reference is removed and never removes an active identity. Library failure checkpoints cover before/after floor commit, after payload verification, after generation publication and before activation commit. They only inject failure or process termination; ordinary CLI options cannot bypass verification. The macOS transaction experiment verifies failed replacement, interrupted temporary-file refusal, committed-floor preservation, process exit before activation and lock release. Windows and real installer validation remain required.

For install/init and later state operations, success is a closed schema-1 `toolchain_operation` record with `schema`, `kind`, `operation`, `root_sha256`, `repository`, `target`, `active`, `installed` and `floor_advanced`. Active is null or a `{channel, version, compiler_build_id, generation}` summary; installed is an array of those summaries. Operation failure uses a closed `{schema: 1, kind: "toolchain_failure", diagnostic, floor_advanced}` record in JSON mode, where diagnostic is the ordinary structured diagnostic. The boolean reports whether this operation committed a newer sequence or admission-time floor before failing. Human mode states the same fact. CLI misuse and the read-only verify command retain their ordinary diagnostic records. These outputs are bounded to 1 MiB and contain no signing material.

### Channel operations and recovery markers

F04 elaboration, accepted under RFC 0001 experimental delegation on 2026-10-30 JST. The real TLS probe verifies certificate/hostname checks, HTTP refusal, no redirects/decoding, truncation and total timeouts. Use ureq 3.5.0 with only its rustls feature, default WebPki roots, ambient proxies disabled, a 64 KiB response-header limit and the existing 10/120-second connect/total limits. An HTTPS base is at most 2,048 bytes, has a host and no credentials, query or fragment; append only the fixed metadata/object paths. Require HTTP 200 and absent or identity Content-Encoding. Other responses, oversized/truncated/encoded bodies and TLS failures refuse. Private test constructors may supply an ephemeral test anchor and shorter timeouts; production exposes no insecure/test-CA option.

List returns authenticated installed summaries without executing each compiler. Select verifies the chosen generation before atomic activation. Which and run verify the active generation, including its SDK and identity. Which returns the closed schema-1 `toolchain_location` record with `schema`, `kind`, `root_sha256`, `repository`, `target`, `active`, `compiler` and `sdk`; the last two are explicit absolute paths. Run accepts child arguments only after `--`, inherits standard streams and emits no manager success record. The library returns the child's ExitStatus; the executable preserves its full Windows exit-code bits and ordinary Unix code, mapping Unix signal termination to 128 + signal. Spawn/wait errors use the manager failure record. There is no library process exit. Release the lock after successful spawn, before waiting. Update admits the signed current version through the same transaction as install, reporting operation update. These operations never reset authenticated floors.

Uninstall's only transaction marker is a closed, at most 1 KiB `transaction.json` object `{schema: 1, kind: "uninstall", root_sha256, generation}`. Validate the pinned digest and canonical generation; no marker field is a path. Write/sync/close it before moving the inactive generation to `trash/<generation>`. Atomically remove its installed entry, then delete trash and marker. Shared generations keep their files while another installed entry references them. Failure before state commit attempts restoration; if restoration fails, retain the marker for recovery. A failed Windows move preserves installed state. Library checkpoints after marker creation, after the move and after state commit exercise failures/interruption without CLI bypasses.

Recovery first authenticates committed state and validates the complete bounded transaction inventory. Trash has at most one canonical directory, matching a valid marker; trash without a marker refuses. If the marker's generation remains referenced, restore trash when the original is absent, keep an existing original when trash is absent, and refuse both present or both missing. If no committed reference remains, discard its trash; an original-plus-trash conflict still refuses. Never adopt state.next. Then remove only recognized `state.next`, `staging/install`, unreferenced canonical generations and metadata not referenced by installed receipts or channel floors. Collect bounded names before mutation so Windows directory enumeration handles are closed. Unknown entries, symlinks or malformed records refuse; recursive removal never follows symlinks. Recovery leaves the committed active identity and all floors unchanged. Other mutations refuse outstanding transaction/staging state until recovery; read operations may inspect intact active generations.

The F04 remaining scope is one bounded transport adapter, operations over existing authenticated receipts and marker-based recovery with failure tests; retain its three-point estimate after this task audit. F05 retains distribution production and complete both-host journeys. TLS behavior is experimentally established; integrated transport ordering, Windows locked-executable removal and all recovery checkpoints remain required validation, not assumptions of completion. These are new host formats; no existing source/ABI/SDK migration is introduced.

### Distribution producer

F05 elaboration, accepted under RFC 0001 experimental delegation on 2026-10-30 JST. The producer is a trusted host tool taking an existing real SDK, exact independently trusted root bytes, channel, canonical bundle version, sequence, issued/expires times, 1–8 externally supplied signing keys and a new output directory. It generates no key, discovers no trust root and changes no guest authority. The library receives keys in memory. A `toolchain-release` repository example takes `--sdk`, `--output`, `--trust-root`, `--channel`, `--version`, `--sequence`, `--issued` and `--expires`. Its sole signing input is at most 4 KiB of closed schema-1 JSON from stdin: `{schema: 1, signing_keys: [64-character lowercase hexadecimal Ed25519 seeds]}`. Unknown/duplicate fields, invalid sizes/counts, repeated/unauthorized keys and an unmet threshold refuse. No private key arguments, environment discovery, temporary key files or raw signing input in diagnostics are permitted.

Read the original bounded SDK manifest through an explicit directory capability. Construct one release/current-version catalogue from its complete file table, plus `sdk/sdk.json` and the matching root compiler copy. All existing target/build/SDK and signed file/path/count/byte/lifetime limits apply. Production uses the host clock without a bypass. Sign exact payload bytes with the supplied distinct authorized keys and verify the resulting envelope using the existing admission verifier. Stream/hash regular SDK inputs into content-addressed objects. Duplicate digests share an object but still require matching bytes and length. Published contents are only `channels/` and `objects/`; keys and the trust root are not published or implicitly imported.

Require an existing output parent and reserve an exclusive private sibling stage on that filesystem; refuse an existing destination. Before publication, initialize a temporary checked-install root within the stage using the explicit trust root and run the ordinary installer against the staged distribution. This reuses authenticated admission, exact SDK/compiler checks and bounded candidate identity execution across compiler builds. Drop all handles and remove checked-install before the existing no-replace atomic rename publishes the distribution. Failure or failed cleanup prevents publication and preserves an existing destination. Process interruption may leave a private sibling stage, never an adopted/published distribution; no signing secret is stored there. The extra validation copy is explicit: staging contains at most two release copies plus bounded metadata, without a performance claim.

Success is a closed schema-1 `toolchain_distribution` record with `schema`, `kind`, `root_sha256`, `repository`, `channel`, `target`, `sequence`, `version`, `generation`, `compiler_build_id`, `files` and `bytes`. These public values describe the authenticated published bundle. Failure emits no success record; the repository example reports a human diagnostic without echoing signing input. This adds a host tool/report and requires no source, SDK, package or ABI migration.

The installer self-check adds bounded I/O but avoids a second SDK/identity validator. The original SDK parser experiment verifies distinct compact/pretty manifest bytes and hashes with identical identity and all 117 file records; it establishes packaging feasibility, not installed execution. The experiment uses an isolated offline dependency lock and is not the workspace baseline. F05 must validate actual produced distributions with install/update/select/run, SDK native linking, historical expiry, all three channels/shared generations and live Windows executable removal, retaining the prior tamper/rollback/interruption/TLS checks. After auditing this finite producer and integrated journey scope, retain F05's three points. These acceptance conditions and both-host CI remain open; this elaboration does not close G9 or establish an official signing identity.

## Execution modes

This is a trusted host CLI. Installed VM/native execution retains existing limits, grants and source semantics. It does not execute package installation hooks, introduce a guest network capability or change JIT/component/embedding execution. The native SDK remains usable only under its existing exact compiler/rustc/profile checks.

## ABI and compatibility

Root, channel envelope/payload, state and receipts are independently versioned new host records. Unknown versions/fields and duplicates refuse. Source syntax, editions, MIR, bytecode, package locks, SDK and public C ABI are unchanged. Existing source checkouts need no migration. There is no automatic import from an arbitrary tool directory or a different trust root; initialize a separate root and reinstall from authenticated metadata.

## Diagnostics and tooling

The F02 verification result is a schema-1 JSON record with `schema`, `kind: "toolchain_metadata"`, `scope: "fresh_trust_context"`, `root_sha256`, `repository`, `channel`, `target`, `sequence`, `payload_sha256`, `current_version` and `releases`. Each release summary has `version`, `compiler_build_id`, `generation`, `files` and `bytes`; generation identifies the signed typed release, not a verified installed directory. The result is bounded to 1 MiB and omits payload bytes and local absolute paths. `TLC0001` identifies admission/input failures and `TLC0002` CLI misuse. No partial successful result is emitted on failure.

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
- [ureq 3.5.0 configuration](https://docs.rs/ureq/3.5.0/ureq/config/struct.ConfigBuilder.html), release 2026-10-21. Separate page update date unavailable; verified 2026-10-30 JST. Bounded HTTPS, header limits and proxy/redirect/encoding policy. Real TLS probe verified; integrated ordering and both-host validation belong to F04–F05.
- [rcgen 0.14.10](https://docs.rs/rcgen/0.14.10/rcgen/fn.generate_simple_self_signed.html), release 2026-10-20, and [rustls 0.23.46](https://docs.rs/rustls/0.23.46/rustls/), release 2026-10-14. Separate page update dates unavailable; verified 2026-10-30 JST. Ephemeral TLS test keys are generated only in memory.
- [Rust 1.99 File locks](https://doc.rust-lang.org/1.99.0/std/fs/struct.File.html#method.try_lock) and [rustup channels](https://rust-lang.github.io/rustup/concepts/channels.html): page update dates unavailable; verified 2026-10-30 JST.
- Existing native SDK snapshot/publication and package admission APIs inspected 2026-10-30 JST; source has no embedded update date. Wire fields, limits and CLI/recovery policy above are project decisions.
