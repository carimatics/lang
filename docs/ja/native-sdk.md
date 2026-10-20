<!-- canonical-language: en; japanese-translation: ja/native-sdk.md -->

# 実験段階native SDK

[English](../native-sdk.md)

更新・確認：2026-10-20 JST。RFC 0020はacceptedです。SDK組立は1成果であり、公開linker／隔離installは別のroadmap成果として残ります。

macOS AArch64またはMSVC developer環境のWindows x86-64で、Python 3、固定Rust toolchain、native linker、C11 compilerを用意します。

```text
python3 tools/build-native-sdk.py --output /absolute/path/to/fresh-sdk
```

WindowsでPythonのcommandが `python` なら置き換えます。Release SDKには `--release` を加えます。出力先は未存在である必要があります。Host panicの包含／poisonを保つためSDKとRust hostは `panic=unwind` を必要とし、abort SDK buildは公開前に拒否してpanic policyをbuild identityに含めます。ToolはcompilerとSDK builderを同時にbuildし、正確な成功Cargo JSONから選択Rust artifact／native support archiveをコピーします。組立中はbuild出力を変更しません。組立にはCargoを使いますが、install後のapplication linkは配布artifactと記録済みrustc release／commitを使い、Cargo／registry／repository pathを参照しません。

SDKには `bin/lang`（Windowsは `lang.exe`）、Rust artifactの `lib/`、必要なarchiveの `native/`、`include/lang/runtime.h`、`startup/` template、日英の本文書、`sdk.json` が含まれます。Runtime roleは `lang_codegen_clif` で、MIR／runtime／bytecodeと他の選択libraryはdependencyです。共通C headerは既存profile-1のsupport型とassertionだけを含み、各C programには正確なexport／interface identityを持つ生成 `program.h` が必要です。SDK directoryのコピーではbuild identityは変わりません。

Installしたcompilerでsource-native program fileを生成します。

```text
SDK/bin/lang native --output program application.lang
```

`program` は未存在である必要があります。元object、semantic／native記録、生成 `bindings.rs`、manifest、必要な `main.rs`／`program.h` を出力します。Default Copy／owned startupは引数なしi64 source mainの結果を表示し、OS exit statusには使いません。`startup/copy.rs` と `owned.rs` は `bindings.rs` の隣に置く明示host templateです。Ownershipはcallerのallocator／cleanup Instance／controlを保持し、例の有限準備policyは64 frame、65,536 cell、512 MiBです。Copyの同期ABIは設定済み実行controlを拒否します。

C libraryにはdefault executable entryがありません。`startup/c-host.rs` は `int native_client(void)` を定義する明示compile済みC objectを要求します。Clientは生成 `program.h` で実configuration／allocator／import grantを作り、exportを呼び、全failure pathを含め同じowner thread上で全contextを破棄してから戻ります。Raw context／clientを保持したり、Rust binding scope終了後にcallしてはいけません。Unsafe matching-image義務はunwindも含みます。信頼済みnative embeddingでありsandboxではなく、metadataはcapabilityを与えません。同梱 `startup/c-guest.lang`／`c-client.c.in` の例は実i64 exportを41で呼び、42とC allocation／context破棄の釣合いを検査します。`@ID@`、`@DIGEST_BYTES@`、`@ENTRY_SYMBOL@` は対応する検証済みC Artifact（decode済みprogram意味論とSDK C metadata API）から埋めます。Interface識別値であり、実行権限ではありません。Integration testにこのdataだけの準備例があります。

公開 `link-native` commandはP12の残作業です。実装までtemplateには完全一致rustc／native linkの明示呼出しが必要で、`--edition=2024`、`-L dependency=SDK/lib`、必要な `-L native=SDK/native`、`--extern lang_codegen_clif=RUNTIME_RLIB`、hostが使うMIR／runtime／bytecode rlibと `-C link-arg=PROGRAM_OBJECT`、信頼済みC host objectを指定します。検証済みSDK／program manifestのpathを使い、link中はprivate snapshot ownerを保持します。C objectはC11とplatform互換runtime（MSVCは `/MD`）を使います。Toolchain／system libraryは外部要件です。この手動手順だけではP02の隔離install検証には数えません。

Version-1 file表は全hash、上限付きsize、正確なtarget／build／rustc／profile／feature policyを記録します。Componentをstreamコピーして新directoryをatomic公開し、失敗時は所有staging fileだけを削除します。Compilerの実行権限を保持します。Toolがcomponentを使う前にproduction `SdkSnapshot` APIでSDKを検証します。Native code、library、生成Rust／C host、build出力は信頼済み入力であり、hashは対応／整合性を検査するだけでnative codeの真正性は証明しません。未対応／不一致artifactは対応SDKでrebuildします。Build間のRust ABI安定性、性能／size改善は約束しません。

対応する [project repository](https://github.com/carimatics/lang) の情報源：RFC 0020、`src/native/sdk_package.rs`、`tools/build-native-sdk.py`、`crates/codegen-clif/src/sdk/runtime.rs`。2026-10-20 JST更新・確認。Cargo JSON fieldは公式 [Cargo Book: External tools](https://doc.rust-lang.org/cargo/reference/external-tools.html)、panic policyは [catch_unwind](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html) と [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#panic) に従います。随時更新文書で更新日の記載はなく、2026-10-20 JST確認。
