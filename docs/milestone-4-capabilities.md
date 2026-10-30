<!-- canonical-language: en; japanese-translation: ja/milestone-4-capabilities.md -->

# Milestone 4: the developer toolchain

[日本語版](ja/milestone-4-capabilities.md)

Updated and verified: 2026-10-30 JST. This guide describes the implementation verified at `0b6591d`. The [delivery board](milestone-4.md) tracks final publication and merge separately. Language, concurrency and embedding capabilities from earlier milestones remain documented in the [M2](milestone-2-capabilities.md) and [M3](milestone-3-capabilities.md) guides.

The supported native hosts are Apple Silicon macOS (`aarch64-apple-darwin`) and Windows x64 MSVC (`x86_64-pc-windows-msvc`). Validation covers macOS 15/26 and Windows Server 2025. These tooling contracts are implemented and experimental; neither the language/native ABI nor every tooling schema is declared stabilized.

## Try the package workflow

Use a verified installed `lang` executable, or build this checkout with its pinned Rust toolchain:

```sh
cargo build --bin lang
```

Put `target/debug` on your command path. In a POSIX shell, run `export PATH="$PWD/target/debug:$PATH"`; in PowerShell, run `$env:Path = "$PWD/target/debug;$env:Path"`. The following commands work from the repository root on either host. Choose a fresh destination; package names use identifiers such as `m4_demo`, not hyphenated directory names. Use `new --name NAME DIRECTORY` when the destination basename is not a package name.

```sh
lang new target/m4_demo
lang fmt --manifest-path target/m4_demo/Lang.json
lang fmt --manifest-path target/m4_demo/Lang.json --check
lang build --manifest-path target/m4_demo/Lang.json
lang run --manifest-path target/m4_demo/Lang.json --locked --offline
lang test --manifest-path target/m4_demo/Lang.json --locked --offline
lang lint --manifest-path target/m4_demo/Lang.json --deny-warnings
lang doc --manifest-path target/m4_demo/Lang.json --locked --offline
lang package --manifest-path target/m4_demo/Lang.json --list
lang package --manifest-path target/m4_demo/Lang.json --locked --offline
```

The generated program prints `0`; its declared smoke test passes by returning zero. The doc and package commands print paths relative to the package authority root: HTML is under `target/lang/<lock-digest>/doc/index.html`, and the source archive is under `target/lang/<lock-digest>/package/source.tar` inside `target/m4_demo`.

An ordinary build captures and validates inputs before publishing `Lang.lock` and artifacts. After editing or formatting source, the old lock becomes stale: `--locked --offline` refuses with `PKG0007` and preserves prior output. Run an ordinary `build` to regenerate it, then repeat locked execution. Offline mode uses explicitly authorized local files; it does not download dependencies.

Commands default to exactly `./Lang.json`; they do not search parent directories. An explicit workspace uses `--package NAME` to select a member. A workspace-only root requires selection; standalone and workspace locks retain distinct schema-1/schema-2 contracts. `--message-format json` emits structured diagnostics with original UTF-8 byte spans, normally on stderr; validation failure exits 1 and usage failure exits 2.

## Add an exact local dependency

The [local-dependency example](../examples/packages/local-dependency/Lang.json) declares:

```json
{
  "schema": 1,
  "package": {"name": "app", "version": "0.1.0", "edition": "2026", "entry": "src/main.lang"},
  "dependencies": {"helper": {"path": "deps/helper", "version": "0.1.0"}}
}
```

The dependency's manifest name must match `helper`, and its exact version must match `0.1.0`. Source imports the admitted module with `mod helper;` and can call its public functions. Try the complete existing fixture:

```sh
lang build --manifest-path examples/packages/local-dependency/Lang.json
lang run --manifest-path examples/packages/local-dependency/Lang.json --locked --offline
```

It prints `42`. Resolution checks containment, versions, target conditions, cycles and resource limits, then freezes the source inventory. Relocating the same admitted inputs preserves lock identity. Changed source, metadata, selection or directory bindings refuse locked replay. Registry dependencies, version-range solving and automatic downloads remain M5 work.

## Develop with an editor

Configure an LSP client to launch `lang lsp` over stdio with one explicit workspace root. Package mode uses the root's `Lang.json`; initialization options can select a relative `manifestPath`, `package` and `target`. Navigation, references, completion, semantic tokens, rename and checked quickfixes use admitted package identities and open-document overlays. Positions use UTF-16; source diagnostics retain original byte spans.

The server returns edits. A client checks document versions and original text, applies the edit transaction and saves files. Stale edits refuse instead of partially applying. Dirty manifest overlays invalidate package semantic answers until valid metadata is saved/refreshed. Opening an arbitrary file does not expand package authority. Only the selected package and its dependencies supply semantic scope.

The [installed-tool journey](../tests/support/toolchain_developer_journey.rs) opens a real installed compiler as an LSP server, navigates a dependency, renames its public function across files, applies an unused-binding quickfix, rejects a stale edit and then rebuilds the saved sources.

## Test, format and publish local artifacts

Package tests are explicit source entries with `fn main() -> i64`: zero passes, nonzero or a runtime failure fails. Root tests can import the public package surface through `mod _package;`. Tests stop at the first failure, and failed publication preserves committed lock/artifact bytes.

Formatting retains comments and permissions and validates the complete expected input set before source publication. Replacements are atomic per file; this is not a filesystem-wide crash transaction or exclusion of external writers. `fmt --check` writes nothing. Linting executes no guest code: `LNT0001` identifies unread resolved bindings, `_`-prefixed names suppress that rule, and `--deny-warnings` changes the exit status without changing diagnostic identity.

`doc` produces detached, escaped static HTML with public API indexing, resolved type links and original source panels. **Source panels include private implementation text.** Comments are plain `///` text, not Markdown or executable doctests. `package` produces a deterministic uncompressed source TAR containing original manifests/source bytes, the lock and a versioned descriptor. It does not publish to a registry or extract untrusted archives; its first profile refuses directory aliases.

## Debug a captured program

Configure a DAP client to launch `lang dap` over stdio. After `initialize`, a VM launch uses this argument shape, replacing the root with the actual canonical directory:

```json
{"root":"/absolute/path/to/package","manifestPath":"Lang.json","mode":"vm","stopOnEntry":false}
```

On Windows, use its actual canonical Windows path. Send source breakpoints after the adapter's `initialized` event, then `configurationDone`. The adapter provides real stop-before-instruction breakpoints, continue/step, captured sources, stack/scopes, initialized locals and bounded read-only evaluation. Use canonical workspace/source paths consistently. Restart/resume invalidates old pause/frame/value identities; reading captured source does not reread mutable disk contents.

Native launch uses `mode: "native"` plus an absolute `sdk` directory and host-configured OS debugger tools. It compiles captured inputs into privately owned executable/debug files. Real source debugging is verified on both hosts. Mixed VM/native stacks are available through the trusted embedded Rust host configuration, which explicitly grants callbacks; the ordinary CLI grants no imports. A cooperative callback can pause once and continue that same invocation without replay.

The implemented debugger profiles are Copy VM/native and the explicitly granted mixed-host profile. Owned/task debugger sessions, arbitrary process attach, arbitrary debugger commands, native value recovery and preempting arbitrary trusted callbacks are outside this profile. See [RFC 0040](rfcs/0040-bounded-debug-adapter-sessions.md) and the [native SDK guide](native-sdk.md) for host setup and exact limits.

## Build a reproducible native executable

Use the SDK reported by your verified installation, or assemble a fresh matching SDK from this checkout:

```sh
python3 tools/build-native-sdk.py --output target/m4_sdk
lang build --manifest-path target/m4_demo/Lang.json --native --sdk target/m4_sdk --locked --offline
```

Use your Python 3 command (`python` is common on Windows). The SDK destination must be new. The build reports an authority-relative executable path under `target/lang/<lock-digest>/native-<sdk-manifest-digest>/`, named `application` on macOS or `application.exe` on Windows. Run that executable under the package directory. The SDK must match the compiler and pinned Rust toolchain; native builds target the current host.

The build records the original SDK manifest bytes, compiler/toolchain identities, input lock and executable digest. The installed journey builds without Cargo available, executes the result, relocates the exact source/manifests/lock and compares complete executable bytes after rebuilding. Reproducibility is for fixed inputs and the same host target/SDK/external toolchain, not equality across architectures or arbitrary toolchain updates.

## Inspect compilation, execution and rebuilds

These commands report observations without publishing build/lock files:

```sh
lang profile compile --manifest-path examples/packages/local-dependency/Lang.json
lang profile run --manifest-path examples/packages/profile_runtime/Lang.json --engine vm
lang profile run --manifest-path examples/packages/profile_runtime/Lang.json --engine native
lang profile allocations --manifest-path examples/packages/profile_runtime/Lang.json --engine vm
lang profile rebuild --manifest-path target/m4_demo/Lang.json
```

Compile reports separate capture, frontend and lowering/verification elapsed time. Runtime reports count successful fuel charges with original source spans and execution usage; samples are not CPU time or completed operations. Native runtime observation uses the original ownership JIT engine, with no VM fallback; it is not native AOT instruction profiling.

Allocation reports reconcile the original logical allocator's requests, successful allocations and retirements, including zero-size ownership and failed requests. The bounded event ledger is not process RSS. Rebuild reports explain changed package/lock inputs; optional `--native-sdk DIR --previous-native DIR` compares a prior native generation and current SDK/toolchain inputs. Reports expose their measured scope and refuse incomplete or inconsistent data. See [RFC 0041](rfcs/0041-bounded-toolchain-profiles.md).

## Install and update signed toolchains

Installation requires an independently trusted root and a signed distribution for the current host. There is no official hosted trust root or automatic trust discovery. Given actual `root.json` and local `distribution` inputs, the commands are:

```sh
lang toolchain init --root target/tools --trust-root root.json
lang toolchain install --root target/tools --source distribution --channel stable
lang toolchain list --root target/tools
lang toolchain which --root target/tools
lang toolchain run --root target/tools -- run --manifest-path target/m4_demo/Lang.json
lang toolchain update --root target/tools --source distribution --channel stable
lang toolchain recover --root target/tools
```

An explicit HTTPS base is also supported with verified TLS and bounded transfers. Stable/beta/nightly are signed channel identities; bundle versions and compiler versions are separate. `select` chooses an already installed channel/version, and `uninstall` refuses the active selection. Installation checks signatures, expiry, monotonic metadata floors, exact compiler/SDK bytes and compiler identity before activation. Transfer failure can advance an authenticated floor while preserving the active version. Historical installed receipts remain usable after fresh byte/signature checks.

Updates can coexist with an old compiler process. Windows can refuse removal of its live executable; the manager preserves consistent records and recovery completes recognized cleanup after exit. Exclusive ownership ends with explicit unlock, even while a duplicate descriptor survives. Recovery neither resets authenticated floors nor adopts arbitrary files.

Release authors can use the trusted `toolchain-release` example with externally supplied keys on bounded stdin; it validates a real SDK and exercises the ordinary installer before atomic publication. It does not generate keys or establish trust. The [README](../README.md#native-sdk-isolated-installation) and [RFC 0042](rfcs/0042-signed-toolchain-installation.md) document the producer and formats. Automatic key rotation and an official distribution service require later work.

## Exit evidence and sources

The [bilingual board audit](milestone-4.md#z03-exit-audit-2026-10-30-jst) maps every G1–G9 requirement to executable tests. The installed journey at `0b6591d` covers install → create/dependency → editor → test → debug → reproducible native binary on macOS and Windows. It retains stale-edit/lock refusals and unchanged prior outputs, rather than testing only successful commands.

- Implementation/local verification: 2026-10-30 JST, `0b6591d`; 51 focused tests, fmt, workspace Clippy with warnings denied, 174 suites / 2,514 tests, doc and six repository-consistency checks. Zero failures; one preexisting timing test remains ignored.
- [Full CI `41019002147`](https://github.com/carimatics/lang/actions/runs/41019002147), completed 2026-10-30: all 18 applicable jobs pass; the documentation-only job is intentionally skipped for source changes. Actual installed-journey results are recorded in [macOS 15 core](https://github.com/carimatics/lang/actions/runs/41019002147/job/123941821348) and [Windows core-2](https://github.com/carimatics/lang/actions/runs/41019002147/job/123941821378).
- Native/mixed debugging: [F06 native CI `41015803370`](https://github.com/carimatics/lang/actions/runs/41015803370), completed 2026-10-30, all three environments pass. Production/debugger code is unchanged in `0b6591d`; its test/documentation paths do not trigger the separate native-debug workflow.
- Contracts: [RFC 0034](rfcs/0034-package-workspaces-and-cli-diagnostics.md), [0035](rfcs/0035-source-package-snapshots.md), [0036](rfcs/0036-checked-package-formatting.md), [0037](rfcs/0037-package-lint-diagnostics.md), [0038](rfcs/0038-package-api-documentation.md), [0039](rfcs/0039-package-editor-snapshots.md), [0040](rfcs/0040-bounded-debug-adapter-sessions.md), [0041](rfcs/0041-bounded-toolchain-profiles.md) and [0042](rfcs/0042-signed-toolchain-installation.md); project contracts updated/audited 2026-10-30 JST.
- Package metadata fuzzing covers manifest and lock readers. The recorded A03 run is 10,000 uninstrumented smoke inputs, not a coverage-guided sanitizer campaign. Finite tests do not establish exhaustive security, external-writer race exclusion or performance improvements.

Final guide/merge evidence follows the delivery board; successful CI for an earlier commit does not satisfy a later head's merge gate.
