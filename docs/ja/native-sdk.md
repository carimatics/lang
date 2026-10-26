<!-- translation-of: ../native-sdk.md; canonical-language: en; reviewed: 2026-10-27 -->

# 実験段階native SDK

[English](../native-sdk.md)

更新日：2026-10-27 JST。RFC 0020のversion-1 SDK（copy/owned/C）はimplementedで、両host隔離installの証拠は2026-10-20 JSTの[M2 capability guideと出口監査](https://github.com/carimatics/lang/blob/e12c120/docs/ja/milestone-2-capabilities.md)に記録済みです。以下のversion-2 task拡張は実験段階で、実Windows task/component起動はまだ未検証です。

Task拡張（[RFC 0031](rfcs/0031-installed-native-task-packages.md)、2026-10-27 JST）: SDK assemblyはmanifest version 2を公開します。`lang native`はtask sourceをprogram version 2/task・LANGBC schema 5・元LANGNAT-2 Task objectへ出力し、生成mainが`execute_tasks`でcaller allocator/Control/runner上限を保ちます。task packageはSDK version 2を必要とし、SDK v1ではtool前に拒否します。旧version-1 copy/owned/C packageは対応buildのSDK v2でも動作します。初期task出力はhost targetに限り、component packageには後続受入決定が必要です。`042fc38`のclean producerからSDKを作り、producer退避後のcheckoutなし・Cargoなしconsumerで5 profileの隔離検証に成功しました（2026-10-27 03:49 JST、macOS AArch64）。起動前にsource/package/private SDKリンク入力を除去し、task/owned-taskとも42を返しました。全baselineは149 suite・2,300 test成功です。component/両host受入条件はM3進捗表で追跡し、M2の旧profileの両host証拠とは区別します。

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

公開 `link-native` はprivate SDK／program snapshotと実rustc release／commit／hostを検証し、元objectと配布libraryをlinkして新executableをexclusive公開します。Cargo／registry／repository pathは使いません。通常packageは生成mainを使い、Cは明示Rust hostと必要な個別compile済みC objectを要求します。

```text
SDK/bin/lang link-native --sdk SDK --package program --output application
SDK/bin/lang link-native --sdk SDK --package c-program --host c-host.rs --object client.o --output c-application
```

出力先は未存在である必要があります。Rust hostは生成 `bindings.rs` の隣へコピーする単一root fileであり、追加Rust moduleは自動収集しません。C objectはC11と互換platform runtime（MSVCは `/MD`）を使います。Executableは元記録を保持し、起動時に元source／SDK directoryを必要としません。Toolchain／system libraryは外部要件で、`RUSTC` で明示的なinstall済みcompilerを選べます。Rustup shimを含め、version検査とcompileは同じcompiler path／current directoryを使います。これだけではP02の両host隔離install証拠には数えません。

信頼済みhostの既定上限はRust source 8 MiB、各64 MiBのobject最大64件、合計256 MiB、診断streamごと1 MiB、executable 2 GiBです。Tool起動前に入力をprivate copyし、link中はSDK／program ownerを保持します。Rustc response引数はUTF-8の1行1引数でshell quoteを使わず、改行を含むpathや表現不能な引数は拒否します。SDK／target／toolchain／file／上限の不一致と、失敗／並行linkは既存出力を保ち、所有staging fileを削除します。

Version-1/2 file表は全hash、上限付きsize、正確なtarget／build／rustc／profile／feature policyを記録します。Componentをstreamコピーして新directoryをatomic公開し、失敗時は所有staging fileだけを削除します。Compilerの実行権限を保持します。Toolがcomponentを使う前にproduction `SdkSnapshot` APIでSDKを検証します。Native code、library、生成Rust／C host、build出力は信頼済み入力であり、hashは対応／整合性を検査するだけでnative codeの真正性は証明しません。未対応／不一致artifactは対応SDKでrebuildします。Build間のRust ABI安定性、性能／size改善は約束しません。

対応する [project repository](https://github.com/carimatics/lang) の情報源：RFC 0020/0031、`src/native/sdk_package.rs`、`src/native/link.rs`、`tools/build-native-sdk.py`、`crates/codegen-clif/src/sdk/runtime.rs`。project変更は2026-10-27 JST更新・確認。Cargo JSON fieldは公式 [Cargo Book: External tools](https://doc.rust-lang.org/cargo/reference/external-tools.html)、panic policyは [catch_unwind](https://doc.rust-lang.org/std/panic/fn.catch_unwind.html) と [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html#panic) に従います。Response引数規則は公式 [rustc command-line reference](https://doc.rust-lang.org/rustc/command-line-arguments.html#path-load-command-line-flags-from-a-path) に従います。随時更新文書で更新日の記載はなく、2026-10-20 JST確認。
