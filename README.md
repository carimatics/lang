<!-- canonical-language: en; japanese-translation: README.ja.md -->

# lang

[日本語版](README.ja.md)

`lang` is a pre-alpha research project for a new systems programming language and its complete developer toolchain. The implementation starts in Rust and is intended to become self-hosting after the language, compiler, and bootstrap process are stable enough to reproduce themselves.

Source syntax, native language ABI and private SDK/package formats remain experimental. The admitted public component C v1 and existing generated clients are stable under [RFC 0033](docs/rfcs/0033-component-v1-compatibility.md), after both-host conformance and previous-client migration.

**Milestones 0–4 have completed their exit gates.** The [M4 capability guide](docs/milestone-4-capabilities.md) covers the verified developer workflow; its [board](docs/milestone-4.md) records all 44 completed deliveries. The [M3 capability guide and complete 15-gate audit](docs/milestone-3-capabilities.md) provide commands, evidence and limits for structured tasks, bounded channels, isolated services, typed native/script plugins, the public C API and installed SDK. The [M3 board](docs/milestone-3.md) is the completed record of all 79 deliveries. The [M2 capability guide](docs/milestone-2-capabilities.md) and historical benchmark evidence remain available.

Verified 2026-10-27 JST: source `9e78f2d` passes the macOS seven-stage baseline (153 suites / 2,321 passed) and [full Windows CI](https://github.com/carimatics/lang/actions/runs/40471721760) (153 suites / 2,306 passed), with zero failures. Both-host checkout-free/Cargo-free SDK startup passes all seven profiles. M4 completed on 2026-10-30 JST: 2,514 local tests, both-host journeys and final-head CI `41023185791` pass; PR #26 is merged and starting main is synchronized. See the [M4 guide](docs/milestone-4-capabilities.md).

## Vision

The language should combine:

- Native performance suitable for systems and application development.
- Memory safety without a mandatory tracing garbage collector.
- A Kotlin-like productive surface: null safety, algebraic data types, pattern matching, generics, traits, extension-oriented APIs, closures, and structured concurrency.
- Explicit access to pointers, allocators, SIMD, operating-system APIs, and foreign functions behind auditable unsafe boundaries.
- AOT compilation and an embeddable script runtime that can call each other in the same process.
- First-class tooling: diagnostics, formatter, linter, tests, package/build management, LSP, DAP, documentation, and toolchain installation.
- A path from one language frontend to multiple domain-focused frontends that share an interface model and compiler pipeline.

## Initial direction

The accepted architecture baseline is deliberately narrower than the full vision; unimplemented extensions still require focused RFCs:

1. Build one language first. Offer safe high-level code and explicit low-level facilities in the same semantic model instead of starting with two languages.
2. Share parsing, type checking, ownership analysis, and a typed mid-level IR between all execution modes.
3. Lower the shared MIR to a portable bytecode VM for scripts and to native object code for AOT builds.
4. Use the existing Cranelift JIT for development, embedding and VM/native conformance under each profile's explicit execution controls. JIT debugger registration and broader optimization remain separate work.
5. Define a versioned interface description and canonical component ABI. Use direct internal calls when layouts are known, and generated thunks/handles at independently versioned boundaries.
6. Keep trusted native plugins and capability-restricted script/WebAssembly plugins as separate deployment tiers.
7. Treat self-hosting as a reproducible bootstrap milestone, not as an early implementation constraint.

The rationale and unresolved tradeoffs are documented in [the architecture baseline](docs/architecture.md).

## Documents

- [M4 developer-toolchain capability guide](docs/milestone-4-capabilities.md): package workflows, editor/debugger use, profiles, signed installation, both-host evidence and limits.

- [Language and ecosystem research](docs/research.md)
- [Benchmark evidence: M2 source/Rust/C and historical prototypes](docs/benchmarks.md)
- [Architecture baseline](docs/architecture.md)
- [Implementation roadmap](docs/roadmap.md)
- [Milestone 1 capability guide](docs/milestone-1.md)
- [Milestone 2 capability guide and complete exit audit](docs/milestone-2-capabilities.md)
- [Milestone 3 completed delivery board](docs/milestone-3.md)
- [Milestone 4 completed delivery board](docs/milestone-4.md)
- [Milestone 5 active delivery board](docs/milestone-5.md)
- [Whole-repository review, 2026-10-17](docs/review-2026-10-17.md)
- [Whole-repository review, 2026-10-20](docs/review-2026-10-20.md)
- [Whole-repository review, 2026-10-27](docs/review-2026-10-27.md)
- [Whole-repository review, 2026-10-28](docs/review-2026-10-28.md)
- [Whole-repository review, 2026-10-30](docs/review-2026-10-30.md)
- [Specification improvement study: Goose, memory domains, and GraalVM](docs/spec-improvement-study.md)
- [M3 architecture history](docs/architecture-m3-history.md)
- [M3 implementation and estimate history](docs/milestone-3-history.md)
- [RFC process and template](docs/rfcs/0000-template.md)
- [RFC 0001: Project charter and compatibility policy (accepted)](docs/rfcs/0001-project-charter.md)
- [RFC 0002: Prototype integer and structured control-flow semantics (implemented)](docs/rfcs/0002-prototype-control-flow.md)
- [RFC 0003: Versioned component contracts and capability imports (accepted)](docs/rfcs/0003-versioned-component-contracts.md)
- [RFC 0004: Owned values, explicit sharing, and cyclic graph storage (accepted)](docs/rfcs/0004-owned-values-and-cyclic-graphs.md)
- [RFC 0005: Structured tasks, cancellation, and isolated services (accepted)](docs/rfcs/0005-structured-tasks-and-isolated-services.md)
- [RFC 0006: Source frontend and portable VM programs (implemented)](docs/rfcs/0006-source-frontend-and-portable-programs.md)
- [RFC 0007: Source ownership and core values (implemented)](docs/rfcs/0007-source-ownership-and-core-values.md)
- [RFC 0008: Cleanup, allocator authority, and C boundaries (implemented)](docs/rfcs/0008-cleanup-allocators-and-c-boundaries.md)
- [RFC 0009: Whole Option/Result ownership extraction (implemented)](docs/rfcs/0009-whole-sum-ownership-extraction.md)
- [RFC 0010: Owned REPL result previews (implemented)](docs/rfcs/0010-owned-repl-previews.md)
- [RFC 0011: Portable ownership artifacts (implemented)](docs/rfcs/0011-ownership-artifacts.md)
- [RFC 0012: Native ownership continuations and managed frames (implemented)](docs/rfcs/0012-native-ownership-continuations.md)
- [RFC 0013: Service byte-work budget (implemented)](docs/rfcs/0013-service-byte-work-budget.md)
- [RFC 0014: Explicit source panic (implemented)](docs/rfcs/0014-explicit-source-panic.md)
- [RFC 0015: Private C native object bridge (implemented)](docs/rfcs/0015-c-native-object-bridge.md)
- [RFC 0016: Indirect REPL result previews (implemented)](docs/rfcs/0016-indirect-repl-previews.md)
- [RFC 0017: C-aware ownership artifacts (implemented)](docs/rfcs/0017-c-aware-ownership-artifacts.md)
- [RFC 0018: Native compiler records (implemented)](docs/rfcs/0018-native-compiler-records.md)
- [RFC 0019: Ordinary owned native entry runtime (implemented)](docs/rfcs/0019-owned-native-entry-runtime.md)
- [RFC 0020: Experimental native SDK and scoped startup (implemented)](docs/rfcs/0020-native-sdk-and-startup.md)
- [RFC 0021: Bounded structured task driver (implemented)](docs/rfcs/0021-bounded-structured-task-driver.md)
- [RFC 0022: Source structured tasks (implemented)](docs/rfcs/0022-source-structured-tasks.md)
- [RFC 0023: Task MIR boundaries (implemented)](docs/rfcs/0023-task-mir-boundaries.md)
- [RFC 0024: Recursive Send/Share and service authority (implemented)](docs/rfcs/0024-send-share-and-service-authority.md)
- [RFC 0025: Bounded channels and backpressure (implemented)](docs/rfcs/0025-bounded-channels-and-backpressure.md)

- [RFC 0026: Source isolated services (implemented)](docs/rfcs/0026-source-isolated-services.md)
- [RFC 0027: Production component IDL, wire and embedding profile (implemented)](docs/rfcs/0027-production-component-interface.md)
- [RFC 0028: Serialized task and component MIR (implemented)](docs/rfcs/0028-task-component-artifacts.md)
- [RFC 0029: Component adapter work and invocation metering (implemented)](docs/rfcs/0029-component-adapter-work.md)
- [RFC 0030: Native task and component records (implemented)](docs/rfcs/0030-native-task-component-records.md)
- [RFC 0031: Installed native task packages (implemented)](docs/rfcs/0031-installed-native-task-packages.md)
- [RFC 0032: Installed component packages with explicit host bindings (implemented)](docs/rfcs/0032-installed-component-packages.md)
- [RFC 0033: Component C v1 compatibility and migration (stabilized)](docs/rfcs/0033-component-v1-compatibility.md)
- [RFC 0034: Package workspaces and CLI diagnostics (implemented)](docs/rfcs/0034-package-workspaces-and-cli-diagnostics.md)
- [RFC 0035: Deterministic source package snapshots (implemented)](docs/rfcs/0035-source-package-snapshots.md)
- [RFC 0036: Checked package formatting (implemented)](docs/rfcs/0036-checked-package-formatting.md)
- [RFC 0037: Package lint diagnostics (implemented)](docs/rfcs/0037-package-lint-diagnostics.md)
- [RFC 0038: Package API documentation (implemented)](docs/rfcs/0038-package-api-documentation.md)
- [RFC 0039: Package editor snapshots and checked edits (implemented)](docs/rfcs/0039-package-editor-snapshots.md)
- [RFC 0040: Bounded debug adapter sessions (implemented)](docs/rfcs/0040-bounded-debug-adapter-sessions.md)
- [RFC 0041: Bounded toolchain profiles (implemented)](docs/rfcs/0041-bounded-toolchain-profiles.md)
- [RFC 0042: Signed toolchain installation (implemented)](docs/rfcs/0042-signed-toolchain-installation.md)
- [RFC 0043: Editions and compatibility policy (implemented)](docs/rfcs/0043-editions-and-compatibility.md)
- [RFC 0044: Checked source fixes and historical migration (implemented)](docs/rfcs/0044-checked-source-migration.md)
- [RFC 0045: Public registry threat model (accepted)](docs/rfcs/0045-registry-threat-model.md)
- [RFC 0046: Registry protocol, admission and recovery (accepted)](docs/rfcs/0046-registry-protocol-and-recovery.md)
- [RFC 0047: Registry authority records and fresh authorization (implemented)](docs/rfcs/0047-registry-authority-records.md)
- [RFC 0048: Registry generations and immutable publication (implemented)](docs/rfcs/0048-registry-state-and-publication.md)
- [RFC 0049: Registry dependencies in frozen package graphs (accepted)](docs/rfcs/0049-registry-package-integration.md)
- [RFC 0050: Registry cache generations and offline replay (implemented)](docs/rfcs/0050-registry-cache-and-offline-replay.md)
- [RFC 0051: Authenticated yank and unyank transactions (accepted)](docs/rfcs/0051-registry-yank-transactions.md)
- [RFC 0052: Registry ownership and local credential revocation (implemented)](docs/rfcs/0052-registry-owner-and-credential-control.md)
- [RFC 0053: Registry request workspace and mutable read generations (accepted)](docs/rfcs/0053-registry-request-workspace.md)
- [RFC 0054: Audited registry quarantine transactions (accepted)](docs/rfcs/0054-registry-quarantine-transactions.md)
- [Contribution guide](CONTRIBUTING.md)

`cargo run -- toolchain verify --trust-root root.json --metadata channel.json --channel stable --message-format json` verifies bounded signed metadata for the current host and emits a JSON summary. The root must be trusted out of band. This read-only check uses a fresh trust context: it neither consults an installed rollback floor nor reads or installs payloads. See [RFC 0042](docs/rfcs/0042-signed-toolchain-installation.md).

`cargo run -- toolchain init --root target/tools --trust-root root.json` initializes a new isolated installation with an explicitly trusted root. `cargo run -- toolchain install --root target/tools --source distribution --channel stable` installs from a local signed distribution and reports the active identity as JSON. It checks exact compiler/SDK bytes and identities before activation; a failed transfer can advance the authenticated metadata floor while preserving the active version. `cargo run -- toolchain identity` reports this compiler's own version, target and build ID. The source may also be an explicitly trusted HTTPS base; TLS verification, no redirects/decoding and finite transfer limits are mandatory. Use `toolchain update --root target/tools --source distribution --channel stable` for the signed current bundle, `list`/`which --root target/tools` to inspect it, `select --root target/tools --channel stable --version 0.1.0` for an installed bundle, and `run --root target/tools -- run --manifest-path app/Lang.json` to forward a command to the verified compiler. `uninstall` takes an inactive channel/version; `recover --root target/tools` resolves recognized interrupted work without resetting floors.

Release authors can run `cargo run --example toolchain-release -- --sdk target/sdk --output distribution --trust-root root.json --channel stable --version 0.1.0 --sequence 1 --issued UNIX_SECONDS --expires UNIX_SECONDS` with externally provisioned signing material on stdin: schema-1 JSON containing `signing_keys`, an array of lowercase hexadecimal Ed25519 seeds (64 characters each). Replace both time placeholders with Unix seconds satisfying RFC 0042. Keep secrets out of command arguments, environment variables and logs. The producer verifies the existing SDK and signatures, exercises the ordinary installer in private staging, then atomically publishes a new destination containing only channel metadata and content-addressed objects. It does not generate keys or establish trust. Both-host install/update/failure journeys are verified at `bf14faf`; the complete M4 new-developer journey is verified at `0b6591d` and documented in the [capability guide](docs/milestone-4-capabilities.md).

`cargo run -- lint --manifest-path target/my_app/Lang.json --deny-warnings --message-format json` checks the selected package and declared tests without executing guest code or writing lock/artifacts. `LNT0001` reports unread bindings; `_`-prefixed names suppress that rule. Existing warnings retain their codes. Omit `--deny-warnings` to keep warnings non-failing. See [RFC 0037](docs/rfcs/0037-package-lint-diagnostics.md).

`cargo run -- doc --manifest-path target/my_app/Lang.json --locked --offline` publishes a detached static API reference at `target/lang/<lock-digest>/doc/index.html`. Public signatures and type links use checked identities; adjacent `///` text and original source panels are escaped. The source panels include private implementation text. Generation failures preserve the previous lock/output. See [RFC 0038](docs/rfcs/0038-package-api-documentation.md).

`cargo run -- lsp` uses package definition/references when its explicit workspace contains `Lang.json`. Initialize options `manifestPath`, `package` and `target` select the package; open source overlays and watched changes update one admitted epoch. Navigation retains original UTF-16 locations for dependency items, cases, parameters, locals and fields. Save/refresh dirty manifests before semantic navigation. See [RFC 0039](docs/rfcs/0039-package-editor-snapshots.md) for source authority and edit prerequisites.

Package completion returns public module/import/enum names, scoped bindings and fields recovered from declared receiver types. Incomplete identifiers use a bounded in-memory normal-parser/binder probe; returned text edits retain original UTF-16 ranges and carry epoch/version data. Unsupported receiver expressions produce no guessed fields.

Full semantic tokens use a fixed legend and the current binding index, with lexical fallback for malformed source. Tokens split at CR/LF, use non-overlapping UTF-16 delta encoding and carry an epoch result ID. Clients advertising semantic-token refresh support receive refresh requests after source/overlay/watch invalidation; range and delta requests are unsupported.

Prepare/rename proposes checked source identifier edits, including across dependency files. Every edited file must be open and versioned; clients need `workspaceEdit.documentChanges`, and multi-file edits additionally require `transactional` or `textOnlyTransactional` failure handling. The server rechecks all admitted input bytes, reanalyzes replacements and verifies complete resolved-HIR/type correspondence. Collisions, import aliases, modules, external ABI declarations and stale inputs are refused; the server never writes the proposal to disk.

Literal code actions reuse those input/version guards and the package linter rule. Clients must announce the supported kind families: `source.format` offers whole-document formatting after a source-position-independent HIR/type proof; `quickfix` prefixes one unambiguously selected unread binding with `_` through the rename proof. Initializers and assignment effects are retained, guest code is never executed, and ambiguous/colliding fixes produce no edit.

## Development

`cargo run -- compatibility --message-format json` reports supported source editions and independent compatibility policies without reading a project. The only edition is `2026`; packages must specify it explicitly. Public component C v1 is stabilized, while source, bytecode, package/tooling formats and private native linkage retain their separate experimental contracts. [RFC 0043](docs/rfcs/0043-editions-and-compatibility.md) defines deprecation and future migration requirements; [M5 progress](docs/milestone-5.md) tracks the implementation.

`cargo run -- fmt --manifest-path target/my_app/Lang.json --check` reports files needing the existing CST formatter without edits. Omit `--check` to apply validated per-file source replacements, including selected dependencies/tests. Original comments/spans and permissions are retained; stale or invalid input refuses before publication. Source changes leave the old lock stale; regenerate it through an ordinary build/run. See [RFC 0036](docs/rfcs/0036-checked-package-formatting.md) for the per-file publication boundary.

`cargo run -- fix --manifest-path target/my_app/Lang.json --message-format json` previews checked `LNT0001` renames across the selected entry and every declared test. Add `--apply` to recompute the proof and publish replacements; initializers, assignments, comments and unrelated bytes remain intact. Dependencies are checked but not edited. Reports include covered roots/files, original byte spans and changed/applied counts; a saved report cannot be applied as input. `--locked` validates the existing lock before the operation, and source edits leave it unchanged and stale until an ordinary build/run regenerates it. Publication requires an exclusive cooperative-writer window: every input is revalidated before per-file atomic replacement, but a later I/O failure reports the committed prefix without rolling it back. See [RFC 0044](docs/rfcs/0044-checked-source-migration.md).

`cargo run -- migrate --from-profile rfc0006-scalar --to-edition 2026 --source-root target/legacy --message-format json target/legacy/main.lang` previews migration of one historical scalar source. Add `--apply` to validate a fresh snapshot and replace that file. `FILE` is cwd-relative or absolute, must remain inside the explicit root, and is reported relative to that root. The finite profile admits private `i64`/`bool` functions and checked identifier renames; unsupported constructs, stale inputs and filesystem aliases are refused. Comments and unrelated bytes remain unchanged; no manifest or lock is created or rewritten. Application uses the publication boundary described above. The complete old-producer/current-VM/linked-native journey passes on macOS and Windows; see the [retained evidence](docs/evidence/2026-10-31-m5-source-migration.json) and [B04 completion](docs/milestone-5.md).

`cargo run -- package --manifest-path target/my_app/Lang.json --list` lists the planned frozen source snapshot without writing output. Omit `--list` to publish an uncompressed deterministic TAR with original manifests/source bytes, `Lang.lock` and a versioned descriptor. `--locked --offline` verifies exact replay. Profile 1 refuses directory aliases; it does not extract untrusted archives or publish to a registry. See [RFC 0035](docs/rfcs/0035-source-package-snapshots.md).

Use Rust **1.99.0** with rustfmt and Clippy, pinned by `rust-toolchain.toml`. The [M2](docs/milestone-2-capabilities.md) and [M3 capability guides](docs/milestone-3-capabilities.md) record implemented behavior, exit gates and limits. `cargo run -- run examples/scripts/tasks.lang` exercises implemented source async and returns 42. The [M3 board](docs/milestone-3.md) is the completed record of all 79 deliveries.

M4 starts with `cargo run -- package check-manifest --message-format json examples/packages/hello/Lang.json`. It validates the bounded RFC 0034 manifest without reading dependencies; success is silent, errors are JSON lines on stderr (exit 1; usage exit 2). The host `package::graph::Graph::resolve` API snapshots exact local dependency graphs inside the explicit manifest directory, with sorted edges and package/edge/source limits. `package::source::analyze` connects isolated package roots to the shared frontend; `cargo test --test package_source` executes the real local-dependency fixture through VM and native JIT, both returning 42. `package::lock::Lock::capture` hashes the frozen inventory and resolved entry/test/directory bindings. `Lock::check` replays a strict canonical lock offline; `Lock::publish` atomically replaces it through a separately supplied directory capability. `cargo test --test package_lock` covers relocation, stale input/binding refusal and failure-safe publication.

`cargo run -- new target/my_app` creates a complete new package exclusively. `cargo run -- build --manifest-path target/my_app/Lang.json` produces a verified portable artifact; `cargo run -- run --manifest-path target/my_app/Lang.json --locked --offline` executes it through the existing VM profiles. With no manifest option, package commands use exactly `./Lang.json`. Build is silent; run prints the i64 result and exits zero on success. Immutable generations under `target/lang/<lock-sha256>/build/` precede final `Lang.lock` publication, preserving prior output on failure. JSON diagnostics retain original source byte spans. Explicit-file commands remain available; workspace commands select an explicit member with `--package NAME`. `build --native --sdk SDK` emits a reproducible native executable and a pinned build receipt through the matching SDK.

`cargo run -- test --manifest-path target/my_app/Lang.json` executes declared tests in manifest order. In a test, `mod _package;` exposes the package's public API; returning zero passes. Tests use frozen source scopes, original spans and the existing Copy/ownership/task/service engines, with fresh execution controls per entry. Success is silent; the first source/runtime/nonzero failure exits one. A package with zero tests is still type-checked without requiring main. Test artifacts live under the same lock key's `test/` generation, published only after every test succeeds. `--locked --offline` applies to tests as well.

```bash
cargo run -- check examples/conformance/owned_pipeline.lang
cargo run -- run examples/conformance/owned_pipeline.lang
cargo run -- compile --output target/owned.lbc examples/conformance/owned_pipeline.lang
cargo run -- run --artifact target/owned.lbc
cargo run -- repl
cargo run --quiet -- repl < examples/repl/owned.repl
cargo run --quiet -- lsp
```

The combined program prints 42. Choose fresh output paths. The CLI grants allocator authority for owned execution, registers no host imports and refuses C execution before effects. The REPL keeps definitions and uses fresh guest counters/storage for each evaluation, with bounded core/indirect previews.

Before finalizing code changes:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

### Native program output

`cargo run -- native --output PROGRAM SOURCE` publishes the original object, portable semantic/native records, checked plans, `bindings.rs` and applicable `main.rs`/`program.h`. Ordinary Copy/owned startup uses a zero-argument i64 entry; C libraries need an explicit host. See [producer tests](tests/native_compiler.rs).

Task packages use program manifest version 2, schema-5 semantics and LANGNAT-2 objects. Component packages use program version 3 with explicit host bindings; current SDK manifest version 3 admits them. Legacy copy/owned/C/task profiles retain their admission rules. Actual seven-profile isolated startup on both hosts is recorded in the [M3 guide](docs/milestone-3-capabilities.md) and [RFCs 0031](docs/rfcs/0031-installed-native-task-packages.md)/[0032](docs/rfcs/0032-installed-component-packages.md), verified 2026-10-27 JST.

### Native package verification API

`lang::native` verifies bounded manifests, paths, hashes, target/build/toolchain pairing and semantic/native records in owned snapshots. Keep snapshots unchanged while tools consume them. Hashes check integrity/pairing; native inputs remain trusted. See [package tests](tests/native_packages.rs).

### Scoped native SDK runtime API

Unsafe `lang_codegen_clif::sdk::runtime::{execute_copy, execute_owned, execute_tasks, with_c}` binds the original linked image with no source compiler, JIT or guest interpreter at startup. Owned retains the allocator, Instance, controls and failures; task startup retains the caller's allocator/Control/runner limits and original task cleanup/failures. Copy is synchronous/unmetered and refuses controls. C contexts/callbacks/dependencies stay within the original binding lifetime. See [runtime](crates/codegen-clif/src/sdk/runtime.rs), [RFC 0020](docs/rfcs/0020-native-sdk-and-startup.md) and [RFC 0031](docs/rfcs/0031-installed-native-task-packages.md).

### Native SDK assembly

`python3 tools/build-native-sdk.py --output SDK` builds a fresh matching host SDK from exact Cargo outputs, with compiler/runtime/dependencies, C header, templates and bilingual instructions. Add `--release` for release; `panic=unwind` is required. See [SDK instructions](docs/native-sdk.md) and [assembly tests](tests/native_sdk.rs).

### Native executable linking

`lang link-native --sdk SDK --package PROGRAM --output EXE` checks the exact rustc release/commit/host and links bundled original code without Cargo, registry or repository lookup. C requires `--host HOST.rs`, optional repeated `--object HOST.o` and explicit context/provider/import grants. Outputs must be fresh; failures preserve existing output. See [full commands](docs/milestone-2-capabilities.md#build-a-native-executable) and [link tests](tests/native_link.rs).

### Native SDK isolated installation

[Run 39370391958](https://github.com/carimatics/lang/actions/runs/39370391958), `c54af31`, proves separate producer/consumer installation on both native hosts without consumer checkout/Cargo/repository inputs. Copy/owned/real-C execute after private source/SDK/program removal. See the [workflow](.github/workflows/native-sdk-install.yml) and [probe](tools/check-native-sdk-install.py). System libraries/toolchains remain external dependencies.

### Combined source conformance

The [combined program](examples/conformance/owned_pipeline.lang) exercises nested collections, owner transfers, shared/weak, arenas, recursive calls and cleanup, returning 42; its failure variant panics with −42. [Production-entry comparisons](tests/support/native_entry_runtime.rs), [actual AOT](tests/native_compiler.rs) and [real-C runtime](tests/support/c_exports_runtime.rs) reconcile values, usage, source failures and cleanup/refusal boundaries.

Both-host conformance, immutable Rust/C measurements and actual LLDB/CDB execution have separate named-revision evidence in the [final audit](docs/milestone-2-capabilities.md). Default linking does not publish debugger sidecars. Owned debug evidence covers physical native/runtime frames; logical continuation stepping, source variables and mixed VM/native stacks remain later work.

## Design process

Changes to syntax, type semantics, memory safety, concurrency, ABI, package compatibility, or the plugin boundary require an RFC. Use the repository skill `$language-design-rfc` or start from `docs/rfcs/0000-template.md`.

Implementation work must preserve equivalent observable behavior between the bytecode and native backends unless an RFC explicitly documents a mode-specific restriction.

## Review scope and current limits

The 2026-10-17 review hardened JIT memory reclamation, bytecode span preservation, CodeView machine types, bounded decoding, semantic-version validation, whole-request capability preflight, and Rust adapter cleanup. Linker response files now require a new path; existing files and symlinks are rejected without truncation. Repeated callers must choose a fresh response-file path or explicitly remove their own old file.

Historical prototype captures remain immutable and predate the JIT reclamation fix; M2 source/Rust/C captures are a separate named-revision experiment. See [benchmark evidence](docs/benchmarks.md) before interpreting current performance. Direct native host callbacks remain trusted and must not unwind; this is not a sandbox.

## Project status

- Maturity: research / pre-alpha
- Validated native hosts: macOS AArch64 and Windows x86-64 (experimental)
- Initial implementation language: Rust
- License: not selected yet

