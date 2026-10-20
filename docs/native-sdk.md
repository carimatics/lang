<!-- canonical-language: en; japanese-translation: ja/native-sdk.md -->

# Experimental native SDK

[日本語版](ja/native-sdk.md)

Updated/verified: 2026-10-20 JST. RFC 0020 is implemented: SDK assembly, public linking and both-host isolated installation are complete. See the [M2 capability guide and exit audit](milestone-2-capabilities.md) for exact workflows, native-host evidence and limits.

Build on macOS AArch64 or Windows x86-64 in an MSVC developer environment, with Python 3, the pinned Rust toolchain, native linker and C11 compiler:

```text
python3 tools/build-native-sdk.py --output /absolute/path/to/fresh-sdk
```

On Windows, use `python` if that is the installed Python command. Add `--release` to build a release SDK. The destination must not exist. SDKs and Rust hosts require `panic=unwind` for host-panic containment/poison; abort SDK builds are refused before publication, and panic policy participates in build identity. The tool builds the compiler and SDK builder together, consumes their exact successful Cargo JSON output and copies the selected Rust artifacts/native support archives. Keep build outputs unchanged during assembly. Assembly uses Cargo; installed application linking must use bundled artifacts and the exact recorded rustc release/commit without Cargo, registry access or repository paths.

The SDK contains `bin/lang` (`lang.exe` on Windows), `lib/` Rust artifacts, applicable `native/` archives, `include/lang/runtime.h`, `startup/` templates, these bilingual instructions and `sdk.json`. The runtime role identifies `lang_codegen_clif`; MIR/runtime/bytecode and other selected libraries are dependencies. The common C header contains only existing profile-1 support types and assertions; each C program still requires its generated `program.h` with exact exports/interface identity. Copying the SDK directory does not change its build identity.

Use the installed compiler to emit source-native program files:

```text
SDK/bin/lang native --output program application.lang
```

`program` must not exist. The compiler emits the original object, semantic/native records, generated `bindings.rs`, manifest and applicable `main.rs`/`program.h`. Default Copy/owned startup prints the zero-argument i64 source main result; the result is not an OS exit status. `startup/copy.rs` and `owned.rs` are explicit host templates to place beside `bindings.rs`. Ownership retains the caller allocator/cleanup Instance/control; its sample finite preparation policy is 64 frames, 65,536 cells and 512 MiB. Copy's synchronous ABI rejects configured execution controls.

C libraries have no default executable entry. `startup/c-host.rs` expects an explicitly compiled C object defining `int native_client(void)`. That client uses the generated `program.h`, creates the real configuration/allocator/import grants, calls exports, and destroys every context on the same owner thread before returning, including every failure path. It must not retain a raw context/client or call after the Rust binding scope ends. The unsafe matching-image obligation includes unwind. This is trusted native embedding, not a sandbox; metadata supplies no capabilities. The bundled `startup/c-guest.lang` / `c-client.c.in` example calls the real i64 export with 41, checks 42, and balances C allocation/context teardown. Fill `@ID@`, `@DIGEST_BYTES@` and `@ENTRY_SYMBOL@` from the matching checked C Artifact (decoded program semantics with the SDK C metadata API); these are interface identity values, not execution authority. The integration test demonstrates this data-only preparation.

The public `link-native` command verifies private SDK/program snapshots and the actual rustc release/commit/host, then links the original object with bundled libraries and exclusively publishes a fresh executable. It uses no Cargo, registry or repository paths. Ordinary packages use their generated main; C requires an explicit Rust host and optional separately compiled C objects.

```text
SDK/bin/lang link-native --sdk SDK --package program --output application
SDK/bin/lang link-native --sdk SDK --package c-program --host c-host.rs --object client.o --output c-application
```

Destinations must not exist. Rust hosts are standalone root files copied beside generated `bindings.rs`; additional Rust modules are not automatically collected. C objects use C11 and compatible platform runtime (`/MD` for MSVC). The executable retains original records and needs no original source/SDK directory at startup. Toolchain/system libraries remain external requirements; `RUSTC` may select an explicit installed compiler. Version checks and compilation use the same compiler path/current directory, including Rustup shims. This alone is not P02 both-host isolated-install evidence.

Default trusted-host bounds are 8 MiB Rust source, 64 objects at 64 MiB each, 256 MiB combined, 1 MiB per captured diagnostic stream and 2 GiB executable. Inputs are private copied before the tool starts; SDK/program owners stay live through linking. rustc response arguments are UTF-8, one per line, without shell quoting; paths containing line breaks or unrepresentable arguments are refused. SDK/target/toolchain/file/limit errors and failed/concurrent linking preserve prior outputs and clean owned staging files.

The version-1 file table records complete hashes and bounded sizes, plus exact target/build/rustc/profile/feature policy. Assembly streams each component and atomically publishes a fresh directory; failures clean only owned staging files. The compiler retains executable permissions. Validate SDKs with the production `SdkSnapshot` API before tools use their components. Native code, libraries, generated Rust/C hosts and build output are trusted inputs; hashes detect pairing/integrity errors, not native-code authenticity. Unsupported/mismatched artifacts require rebuilding with the matching SDK. No Rust ABI stability across builds or performance/size improvement is promised.

Sources in the matching [project repository](https://github.com/carimatics/lang): RFC 0020, `src/native/sdk_package.rs`, `src/native/link.rs`, `tools/build-native-sdk.py`, `crates/codegen-clif/src/sdk/runtime.rs`, updated/verified 2026-10-20 JST. Cargo JSON fields follow the official [Cargo Book: External tools](https://doc.rust-lang.org/cargo/reference/external-tools.html); panic policy follows [catch_unwind](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html) and [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#panic). Response argument rules follow the official [rustc command-line reference](https://doc.rust-lang.org/rustc/command-line-arguments.html#path-load-command-line-flags-from-a-path). These living pages publish no update dates; verified 2026-10-20 JST.
