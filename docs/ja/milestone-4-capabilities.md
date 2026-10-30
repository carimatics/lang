<!-- translation-of: ../milestone-4-capabilities.md -->

# Milestone 4: 開発者向けtoolchain

[English](../milestone-4-capabilities.md)

更新・確認: 2026-10-30 JST。このguideは`0b6591d`で検証した実装を説明します。最終公開とmergeは[作業ボード](milestone-4.md)で別に追跡します。以前のmilestoneの言語、並行処理、埋込み機能は[M2](milestone-2-capabilities.md)と[M3](milestone-3-capabilities.md)のguideを参照してください。

対応native hostはApple Silicon macOS（`aarch64-apple-darwin`）とWindows x64 MSVC（`x86_64-pc-windows-msvc`）です。macOS 15／26とWindows Server 2025で検証しています。これらのtooling契約は実装済みの実験的機能であり、言語／native ABIや全tooling schemaの安定化を宣言するものではありません。

## Package workflowを試す

検証済みの導入済み`lang` executableを使うか、このcheckoutを固定Rust toolchainでbuildします。

```sh
cargo build --bin lang
```

`target/debug`をcommand pathへ追加します。POSIX shellでは`export PATH="$PWD/target/debug:$PATH"`、PowerShellでは`$env:Path = "$PWD/target/debug;$env:Path"`を実行します。以下のcommandは両hostのrepository rootから使えます。新しいdestinationを選んでください。package名にはhyphen入りdirectory名ではなく、`m4_demo`のような識別子を使います。destinationのbasenameがpackage名として使えない場合は`new --name NAME DIRECTORY`を指定します。

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

生成programは`0`を出力し、宣言されたsmoke testはzeroを返して成功します。docとpackage commandはpackage権限rootからの相対pathを出力します。HTMLは`target/m4_demo`内の`target/lang/<lock-digest>/doc/index.html`、source archiveは同じく`target/lang/<lock-digest>/package/source.tar`です。

通常buildは入力を固定・検証してから`Lang.lock`とartifactを公開します。sourceの編集・format後は旧lockがstaleになり、`--locked --offline`は`PKG0007`で拒否して旧出力を保持します。通常の`build`でlockを再生成してからlocked実行を繰り返します。offline modeは明示的に許可したlocal fileだけを使い、dependencyをdownloadしません。

commandの既定manifestは正確に`./Lang.json`であり、親directoryを探索しません。明示workspaceでは`--package NAME`でmemberを選択します。workspace-only rootは選択が必須で、standaloneとworkspaceのlockは別々のschema-1／schema-2契約を保ちます。`--message-format json`は元のUTF-8 byte spanを持つ構造化診断を通常stderrへ出力し、検証失敗はexit 1、usage失敗はexit 2です。

## 完全一致のlocal dependencyを追加する

[local-dependency example](../../examples/packages/local-dependency/Lang.json)は次のように宣言します。

```json
{
  "schema": 1,
  "package": {"name": "app", "version": "0.1.0", "edition": "2026", "entry": "src/main.lang"},
  "dependencies": {"helper": {"path": "deps/helper", "version": "0.1.0"}}
}
```

dependency manifestの名前は`helper`、正確なversionは`0.1.0`と一致する必要があります。sourceは`mod helper;`でadmit済みmoduleをimportし、そのpublic functionを呼べます。完全な既存fixtureを試せます。

```sh
lang build --manifest-path examples/packages/local-dependency/Lang.json
lang run --manifest-path examples/packages/local-dependency/Lang.json --locked --offline
```

出力は`42`です。解決処理は包含、version、target条件、循環、resource上限を検査し、source inventoryを固定します。同じadmit済み入力の配置変更はlock identityを保持します。source、metadata、選択、directory bindingの変更はlocked replayを拒否します。registry dependency、version range解決、自動downloadはM5の作業です。

## Editorで開発する

LSP clientから`lang lsp`をstdioで起動し、workspace rootを一つ明示します。package modeはrootの`Lang.json`を使い、初期化optionで相対`manifestPath`、`package`、`target`を選択できます。navigation、参照、completion、semantic token、rename、検証付きquickfixは、admit済みpackage identityと開いているdocument overlayを使います。位置はUTF-16で、source診断は元のbyte spanを保持します。

serverは編集内容を返します。clientがdocument versionと元textを検査し、編集transactionを適用して保存します。stale編集は部分適用せず拒否します。未保存のmanifest overlayは、有効なmetadataの保存・refreshまでpackage意味情報を無効にします。任意fileを開いてもpackage権限は増えません。選択packageとそのdependencyだけが意味scopeを供給します。

[導入tool journey](../../tests/support/toolchain_developer_journey.rs)は実際の導入済みcompilerをLSP serverとして起動し、dependencyへのnavigation、複数fileに跨るpublic functionのrename、未使用bindingのquickfix、stale編集の拒否を実行して、保存後のsourceを再buildします。

## Test・formatとlocal artifactの公開

package testは`fn main() -> i64`を持つ明示source entryです。zeroなら成功、nonzeroまたはruntime failureなら失敗します。root testは`mod _package;`でpublic package surfaceをimportできます。最初の失敗でtestを止め、公開失敗はcommit済みlock／artifact bytesを保持します。

formatはcommentとpermissionを保持し、source公開前に期待する入力全体を検証します。置換はfile単位でatomicですが、filesystem全体のcrash transactionや外部writerの排除ではありません。`fmt --check`は書き込みません。lintはguest codeを実行せず、`LNT0001`で解決済み未読bindingを識別します。`_`で始まる名前はそのruleを抑制し、`--deny-warnings`は診断identityを変えずexit statusを変えます。

`doc`はpublic API索引、解決済みtype link、元source panelを含む、単独で閲覧できるescape済みstatic HTMLを生成します。**source panelにはprivate実装textも含まれます。** commentはplainな`///` textで、Markdownや実行するdoctestではありません。`package`は元manifest／source bytes、lock、version付きdescriptorを含む決定的な非圧縮source TARを生成します。registry公開や信頼しないarchiveの展開は行わず、最初のprofileはdirectory aliasを拒否します。

## 固定したprogramをdebugする

DAP clientから`lang dap`をstdioで起動します。`initialize`後のVM launchは、rootを実際のcanonical directoryに置き換えた次の引数形式です。

```json
{"root":"/absolute/path/to/package","manifestPath":"Lang.json","mode":"vm","stopOnEntry":false}
```

Windowsでは実際のcanonical Windows pathを使います。adapterの`initialized` event後にsource breakpointを設定し、`configurationDone`を送ります。adapterは実命令の直前に止まるbreakpoint、continue／step、固定source、stack／scope、初期化済みlocal、上限付き読み取り専用evaluationを提供します。workspace／sourceは一貫してcanonical pathを使います。restart／resumeで古いpause／frame／value identityは無効になり、固定sourceの取得は変更可能なdisk内容を再読込みしません。

native launchは`mode: "native"`、絶対pathの`sdk` directory、host側で設定したOS debugger toolを使います。固定入力からprivateに所有するexecutable／debug fileをcompileします。実source debugは両hostで検証済みです。mixed VM／native stackは、callbackを明示許可するtrusted embedded Rust host設定で利用でき、通常CLIはimportを許可しません。協調callbackは一度pauseし、同じ呼出しをreplayせず続行できます。

実装したdebugger profileはCopy VM／nativeと明示許可したmixed-host profileです。owned／task debugger session、任意process attach、任意debugger command、native value recovery、任意trusted callbackの強制中断は範囲外です。host設定と正確な上限は[RFC 0040](rfcs/0040-bounded-debug-adapter-sessions.md)と[native SDK guide](native-sdk.md)を参照してください。

## 再現可能なnative executableをbuildする

検証済みinstallationが報告したSDKを使うか、このcheckoutから新しい一致SDKを組み立てます。

```sh
python3 tools/build-native-sdk.py --output target/m4_sdk
lang build --manifest-path target/m4_demo/Lang.json --native --sdk target/m4_sdk --locked --offline
```

利用環境のPython 3 commandを使ってください（Windowsでは`python`が一般的です）。SDK destinationは新規である必要があります。buildは`target/lang/<lock-digest>/native-<sdk-manifest-digest>/`内の権限root相対executable pathを報告し、macOSでは`application`、Windowsでは`application.exe`です。package directoryの下にあるそのexecutableを実行します。SDKはcompilerと固定Rust toolchainに一致し、native buildはcurrent hostを対象にします。

buildは元SDK manifest bytes、compiler／toolchain identity、入力lock、executable digestを記録します。導入tool journeyはCargoを使えない状態でbuild・実行し、正確なsource／manifest／lockを移動して再build後の全executable bytesを比較します。再現性は固定入力と同じhost target／SDK／外部toolchainに対するもので、異なるarchitectureや任意のtoolchain更新を跨ぐ一致ではありません。

## Compile・実行・rebuildを調べる

次のcommandはbuild／lock fileを公開せず観測結果を報告します。

```sh
lang profile compile --manifest-path examples/packages/local-dependency/Lang.json
lang profile run --manifest-path examples/packages/profile_runtime/Lang.json --engine vm
lang profile run --manifest-path examples/packages/profile_runtime/Lang.json --engine native
lang profile allocations --manifest-path examples/packages/profile_runtime/Lang.json --engine vm
lang profile rebuild --manifest-path target/m4_demo/Lang.json
```

compile reportはcapture、frontend、lowering／verificationの経過時間を分けます。runtime reportは元source spanと実行usageに対応する成功fuel chargeを数え、sampleはCPU時間や完了operation数ではありません。native runtime観測は元のownership JIT engineを使い、VM fallbackを行いません。native AOT命令profilingではありません。

allocation reportは元の論理allocatorの要求、成功allocation、retirementを照合し、zero-size所有や失敗要求も含みます。上限付きevent ledgerはprocess RSSではありません。rebuild reportはpackage／lock入力の変更を説明し、任意の`--native-sdk DIR --previous-native DIR`で以前のnative generationと現SDK／toolchain入力を比較できます。reportは測定範囲を明示し、不完全・不整合なdataを拒否します。[RFC 0041](rfcs/0041-bounded-toolchain-profiles.md)を参照してください。

## 署名toolchainを導入・更新する

導入には独立に信頼したrootとcurrent host向け署名配布物が必要です。公式hosted trust rootや自動trust探索はありません。実際の`root.json`とlocal `distribution`を用意した場合のcommandは次のとおりです。

```sh
lang toolchain init --root target/tools --trust-root root.json
lang toolchain install --root target/tools --source distribution --channel stable
lang toolchain list --root target/tools
lang toolchain which --root target/tools
lang toolchain run --root target/tools -- run --manifest-path target/m4_demo/Lang.json
lang toolchain update --root target/tools --source distribution --channel stable
lang toolchain recover --root target/tools
```

明示HTTPS baseにも、TLS検証と有限transferで対応します。stable／beta／nightlyは署名channel identityで、bundle versionとcompiler versionは別です。`select`は導入済みchannel／versionを選び、`uninstall`はactiveな選択を拒否します。導入はactivate前に署名、期限、単調metadata floor、compiler／SDKの正確なbytes、compiler identityを検査します。transfer失敗時も認証済みfloorが進むことはありますが、active versionは保持します。過去の導入receiptは、改めてbytes／署名を検査すれば利用できます。

旧compiler processの実行中でもupdateできます。Windowsは実行中executableの削除を拒否する場合がありますが、managerは整合したrecordを保持し、終了後にrecoveryが認識済みcleanupを完了します。複製descriptorが存続しても、明示unlockで排他所有は終了します。recoveryは認証済みfloorをresetせず、任意fileを採用しません。

release作成者は、外部から供給する鍵を上限付きstdinで渡すtrusted `toolchain-release` exampleを利用できます。実SDKを検証し、通常installerで確認してからatomicに公開します。鍵生成やtrust確立は行いません。producerとformatは[README](../../README.ja.md)と[RFC 0042](rfcs/0042-signed-toolchain-installation.md)を参照してください。自動鍵rotationや公式配布serviceは後続作業です。

## Exitの証拠と出典

[両言語ボードの監査](milestone-4.md)がG1–G9の全要件を実行testへ対応づけます。`0b6591d`の導入tool journeyは、install → create／dependency → editor → test → debug → 再現native binaryをmacOS／Windowsで実行します。成功commandだけでなく、stale編集／lockの拒否と旧出力の保持も検証します。

- 実装／local確認: 2026-10-30 JST、`0b6591d`。重点51 test、fmt、warnings deniedのworkspace Clippy、174 suite／2,514 test、doc、repository consistency 6件。失敗0、既存timing testのignore 1件。
- [全体CI `41019002147`](https://github.com/carimatics/lang/actions/runs/41019002147)、2026-10-30完了。適用18 jobすべて成功し、source変更のため文書専用jobは意図したskip。実導入journeyの結果は[macOS 15 core](https://github.com/carimatics/lang/actions/runs/41019002147/job/123941821348)と[Windows core-2](https://github.com/carimatics/lang/actions/runs/41019002147/job/123941821378)に記録しています。
- native／mixed debug: [F06 native CI `41015803370`](https://github.com/carimatics/lang/actions/runs/41015803370)、2026-10-30完了、3環境すべて成功。`0b6591d`ではproduction／debugger codeを変更しておらず、そのtest／文書pathは別native-debug workflowを起動しません。
- 契約: [RFC 0034](rfcs/0034-package-workspaces-and-cli-diagnostics.md)、[0035](rfcs/0035-source-package-snapshots.md)、[0036](rfcs/0036-checked-package-formatting.md)、[0037](rfcs/0037-package-lint-diagnostics.md)、[0038](rfcs/0038-package-api-documentation.md)、[0039](rfcs/0039-package-editor-snapshots.md)、[0040](rfcs/0040-bounded-debug-adapter-sessions.md)、[0041](rfcs/0041-bounded-toolchain-profiles.md)、[0042](rfcs/0042-signed-toolchain-installation.md)。project契約は2026-10-30 JSTに更新・監査しています。
- package metadata fuzzingはmanifestとlockのreaderを対象にします。A03で記録した実行は10,000件の非instrumented smokeで、coverage-guided sanitizer campaignではありません。有限testは網羅的security、外部writer raceの排除、性能改善を証明しません。

guideの最終公開／merge証拠は作業ボードに記録します。古いcommitのCI成功は、後続headのmerge gateを満たしません。
