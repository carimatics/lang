<!-- translation-of: ../../rfcs/0020-native-sdk-and-startup.md; canonical-language: en -->

# RFC 0020: 実験段階のnative SDKとscope付き起動

[English](../../rfcs/0020-native-sdk-and-startup.md)

- Status: accepted
- Authors: Codex, under the owner's experimental delegation
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-P08, M2-P09, M2-P10, M2-P07, M2-P02
- Supersedes: none; extends RFCs 0008/0011/0015/0017/0018/0019
- Superseded by: none

## Summary

Native compilerコマンド、対応するRust SDK、scope付きproduction起動consumerを配布します。Sourceから元native objectと検証済み意味論／compiler記録を生成し、SDKのRust libraryと生成bindingでapplicationをlinkします。起動時は既存記録をdecode／準備して元machine codeを呼びます。

Review outcome：**accepted under experimental delegation, 2026-10-20 JST**。製品実装前に受理します。検証済みentry／binding契約を再利用し、nativeの明示的信頼と正確なSDK／toolchain識別を要求し、C権限を実hostに保持する設計は妥当です。下記Mac ownership実験がlibraryコピーによる起動を裏付けます。SDK組立、compiler／consumerコマンド、最小依存物の選択、Windows install、Copy／ownership／C全profileの導入証拠は未検証の実装要件です。受理で完了するのはP08のみで、G15は完了しません。

## Motivation

現在の`lang compile`はportable artifactを出力し、`prototype-object`は固定MIR prototype 3種のみを受け付けます。P05／P06で保存fileからproductionの通常／C実行を復元できますが、製品用source-nativeコマンドと配布startup consumerがありません。Test driverの配布ではtest Runtime、source再分析／backend生成が残り、そのコピーでP07を完了にはできません。

実装`2712700`での準備実験では、実Bytes programをcompileし、元objectをリポジトリ外のRust consumerへlink、コピーした依存libraryだけでconsumerをbuildしました。`guest.lang`を移動後も`42; live allocations 0`を出力しました。Consumerはfrontendをimportせず、Runtimeを独自実装せず、guest codeを生成しません。`otool -L`の動的依存はsystem libiconv／libSystemのみでした。Macのownership 1件の実験であり、隔離installや全profileの証拠ではありません。

## Goals

- 両tier-1 hostにCopy、ownership、明示C libraryの製品用source-native compileとsource不要起動を提供。
- 元entry識別、span、control、allocator、context、grant、cleanup、code寿命の契約を保持。
- Native呼出し／provider作用前にSDK、target、形式、toolchain不一致を拒否。
- RepositoryとCargo target／dependency directory外でSDKを利用可能にする。

## Non-goals

- SDK buildをまたぐRust artifact／native source言語ABIの安定化。
- C bootstrap ABI、process-global binding registry、自動C grant、第2 frontend、native sandbox、package registry／依存解決の追加。
- Source構文、C profile 1、portable schema 1–4、native record 1、component、任意JITの変更。

## Guide-level design

予定するコマンドは次のとおりです。

```text
lang native [--source-root ROOT] [--target TRIPLE] --output DIR SOURCE
lang link-native --sdk SDK --package DIR --output EXE
lang link-native --sdk SDK --package DIR --host HOST.rs [--object HOST.o ...] --output EXE
```

この決定だけではコマンドは実装されません。`native`は新規program directoryを出力します。既定targetはcompiler hostで、既存の両tier-1 tripleへのobject出力を維持します。Link／run hostはprogram／SDK targetと一致する必要があります。

Copy／通常owned packageでは、実際に検査した引数なしi64のsource mainを選び、0以外のfunction IDも保持します。既定の生成Rust mainは既存script adapterと同じく値を表示して0で終了し、失敗は構造化診断を表示して1で終了します。Scriptの結果はOS exit codeではありません。独自Rust hostからscope付きSDK APIで埋め込むこともできます。

C unitはprivateなmainを含んでも自動実行entryを持たないlibrary packageです。生成header／Rust bindingで既存profile-1 exportを公開します。明示Rust hostと、必要なら別途compileしたC host objectを指定します。実C callerがcontextを生成／呼出し／照会／破棄する間、Rust hostはbindingを保持します。C importには実hostのbinding／grantが必要で、private mainを動かすために仮のC export／MIR wrapperは作りません。

初期SDKは正確に対応する固定Rust release／commit、native linker、C host用C11 toolchainを明示要件とします。`link-native`は同梱libraryでrustcを実行し、Cargo、依存download、repository pathを使いません。Rust host要件は実験段階の配布判断です。将来Cだけでbootstrapする場合は別のreview済み変更が必要です。

## Reference semantics

Frontend、typed MIR、評価順、推論、loan、move、arena／shared、破棄、失敗優先度は受理済みprofileを維持します。Native出力は意味論unit全体を使い、未使用C宣言を含むABI宣言でC分類します。名前やscalar署名でABI識別を代用しません。

Compilerは意味論dataを独立検証し、元object／compiler記録とcanonical symbol bindingを生成して、完全に検証した出力だけを公開します。診断用source snapshot／spanを保持し、起動時に元sourceを読みません。失敗時は既存出力／入力を保持し、非公開の同階層directoryにstaging後、排他的に公開して既存destinationを拒否します。事前確認後の上書きrenameに頼らず、同時公開／rollbackを検証します。

Startupはpackage／SDK／target識別、上限付き意味論／native dataとlayout／plan domainを検査してからbindします。生成symbol解決は元objectのsymbolだけを参照し、Cの場合はslot／support／export／thunkも含みます。ISA、backend module、実行可能memory、guest interpreterを作りません。Host glueのRust compileはapplication起動前のlink時に行います。

通常facadeはRFC 0019の元entryをhost allocator／cleanup Instanceで実行します。Ownershipのcontrolと未対応設定はVMと同じ優先度を維持し、有限frame／cell準備を必要とします。Copy facadeは既存のtrustedな同期・非計量native契約を維持し、未対応controlの指定は無視せず拒否します。Host importやC権限を作り出しません。

Scope付き実行／埋込み関数でdecode済みIR、layout、code owner、allocator、cleanupが意図した寿命から逃げないようにします。Cは従来の明示unsafeなmatching-image／同期owner-thread契約を維持し、unbind前に全呼出しが戻り全contextを破棄する必要があります。Raw C clientを受け付けるAPIはこの安全要件を公開し、安全に見えるfacadeで隠しません。Scheduling、共有、定数実行、memory modelは追加しません。Cancel／interrupt、host panic後のpoisonも既存runtimeのままです。

## Execution modes

- VM：既存compile／run artifactと制限は不変。`run --artifact`からnative packageへ暗黙fallbackしません。
- Native AOT：元object、独立decodeした記録、production primitivesを使用し、起動時source分析／再生成／JIT／MIR評価は不要。
- 任意JIT：既存明示APIを維持し、native不一致／資源不足からJITへ切り替えません。
- C：既存profile-1 wrapper、実C provider、元opaque context、明示binding／grantを使い、tagで実行権限を付与しません。
- WebAssembly／component：既存の独立契約を維持。

## ABI and compatibility

UTF-8 JSON manifest 2種を追加し、固定`format`と整数`version: 1`を持たせます。未知／重複field、未対応version／profile／target、重複path／singleton role、不正size／digest／非canonical名を使用前に拒否します。

| Manifest | 必須の論理field |
| --- | --- |
| `sdk.json` | `format = "lang-native-sdk"`、`version`、`build_id`、`target`、正確な`rustc` release／commit、build profile／feature policy、file table |
| `package.json` | `format = "lang-native-program"`、`version`、`build_id`、`target`、`profile = "copy" / "owned" / "c"`、`entry` function IDまたはnull、file table |

SDKの正確なkeyは`format`、`version`、`build_id`、`target`、`rustc`、`build_profile`、`feature_policy`、`files`です。`rustc`は`release`／`commit`のみを持つobject、`build_profile`は`"debug" / "release"`、`feature_policy`は1–128 byteのASCII識別です。Programのkeyは`format`、`version`、`build_id`、`target`、`profile`、`entry`、`files`です。File entryは`{"role": string, "path": string, "bytes": u64, "sha256": string}`のみを持ちます。SDK roleはsingletonの`compiler`／`runtime`、複数可の`dependency`／`header`／`startup`／`documentation`です。Program roleは全てsingletonで、`semantic`／`native_record`／`object`／`bindings`と、適用時の`main`／`c_header`です。未知roleも拒否します。

File entryはrole、相対path、u64 byte size、完全な小文字SHA-256を持ちます。Pathは正規化した相対名で、絶対path、空要素、dot／dot-dot、NUL、別separator、Windowsのdrive／ADS／device名や末尾dot・space、symlink／reparse point経由の脱出を拒否します。Object名はtargetに応じ`program.o`／`program.obj`です。Programのroleは意味論`program.lbc`、`program.native`、object、生成`bindings.rs`、必要に応じ既定`main.rs`または`program.h`です。Role／entry／profileは独立decode済み意味論／record kindと一致させ、通常ownershipはunit記録、Cは正確なC Artifact digestを持つCObjectを使います。C entryはnullです。Bindingはprivate名／revisionを推測せず、記録されたcanonical symbolを使います。

Build IDは32 byte digestを小文字64桁hexで表すopaque識別で、対応compiler／runtime SDK buildへ一貫して割り当てます。Production source、manifest／Cargo.lock、固定rustc release／commit、有効feature／profile policy、target、native ABI revisionを入力に含めます。Crate version、Gitのclean状態、interface digest、timestampだけでは不十分です。SDK buildは対応componentに1つの識別を与えfile tableを検証します。Compilerは共通build入力と要求targetからtarget別識別を導出し、自身のhostで要求targetを代用しません。Cross-target出力にはlink／run時の対応target SDKが必要で、hostをまたぐRust library互換性は約束しません。

Compilerはbuild識別を記録し、consumerは呼出し前にprogram／SDK／compile済みruntime識別を比較します。Host glue compile前に実rustc release／commitを確認し、元objectとnative記録の対応を含めlink入力をsnapshot／検証、そのbyteをlinkします。Build ID／hashは取り違え検出で、native codeの認証ではありません。

既定manifest上限はSDK 1 MiB／4,096 file、program 64 KiB／8 file、JSON depth 32、path UTF-8 4,096 byte、識別／toolchain文字列も制限します。Program componentは既存意味論／native／header上限を維持し、生成bindingは8 MiB、program staging合計は256 MiBに制限します。SDK file検証は上限付きbufferでstreamし、既定で1 file 2 GiB／合計8 GiBまでとします。設定上限超過は無制限予約前の構造化拒否です。意味のある境界と不足のfixtureを作ります。

SDKにはcompiler、選択した対応Rust依存artifact、production startup／binding support、header資源、使用文書を含めます。ProgramのC headerは既存profile／target／interface／toolchain宣言を維持し、C署名を変えずSDK buildを示します。Rust layout、pointer、context、allocator callback、capabilityはserializeしません。Native recordとportable形式は不変です。

既存の裸native記録は明示trusted Rust APIで引き続き使えますが、SDK packageとして暗黙には扱いません。未知manifest／buildは対応SDKでの再buildが必要です。Source edition、現portable artifact、C profile 1は維持します。以後のmanifest／schema／公開契約変更はRFCの明示改訂または後継RFCとmigration fixtureを必要とします。

## Diagnostics and tooling

Source診断識別／span、既存parser／formatter／linter／LSPを維持します。Compiler、package、SDK、toolchain、linker、runtimeのerrorはstage、path／offset、必要ならexpected／actualを持つ構造化型にします。Xcode／MSVC／rustc不足は対処可能な診断にし、guestを呼ばず既存出力を壊しません。CLI usageは2、source／tool／runtime errorは1で終了します。通常／C host例、toolchain、制限、未対応経路を文書化します。

Native source range／debug入力を維持しますが、この判断でG01–G03のmetadata／step／backtrace証拠を完了にはしません。新DAPやsource fix-it意味論は追加しません。

## Performance and cost model

Buildごとにcompiler pass／native object生成に加え、Rust host compile／native linkを行います。起動には既存の上限付き意味論検証、metadata hash、layout／plan準備、runtime storage予約が必要です。Native record 1はlink済みcode以外にもobject全byteを保持し、Cは従来のwrapper／dispatchとwire storageを使います。

準備実験でコピーした既存debug rlib 116件の論理sizeは868.8 MiBで、複数feature variantを含みます。最小／release SDKの測定ではありません。依存closure、archive／exe size、startup、allocation、Rust／C workload比較は後続で測定します。起動・実行・size・memory改善は主張しません。SDK fileは組立／link時に検証し、自己完結したapplicationが毎起動時にSDK全体をhashしてはいけません。

## Security and trust boundaries

Source／portable dataは既存予算で非信頼入力として扱います。Manifest／記録はfilesystem／network／process／FFI権限を付与しません。Native object、SDK library、生成Rust host code、追加host objectはtrustedなnative build入力で、明示nativeコマンドは非信頼plugin sandboxではありません。非信頼codeはportable／Wasmのcapability制限経路を既定とします。

Native linker／compiler起動前とprovider／guest作用前に上限付きdataを検証します。互換性確認のために未知imageをloadしません。Exact image／toolchainの寿命とC owner-thread要件を維持し、hashを署名や安全なmachine codeの証明としません。Metadata検証でconstructor／callback／guest関数を実行しません。

## Alternatives

- **変更なし：** 製品AOT／起動と隔離installが未実装のまま残ります。
- **Library／toolingのみ：** 既存Rust APIとtest手順で動作は示せますが、各利用者がmetadata／toolchain方針を組み直し、test Runtimeを配る危険があります。
- **事前buildしたstable C bootstrap（最有力対案）：** C hostにRust compilerを不要にできますが、新しい初期化／終了／handle ABI、allocator／失敗／寿命契約、互換性証拠が必要です。別途reviewする将来案とします。
- **Raw objectとad-hoc C shim（単純案）：** 意味論／control／cleanupの復元とmatching-build検査が不足します。
- **Process-global／lazy binding：** 寿命／公開と初期化／並行性の作用を隠すため、明示scope付きbindingを維持します。

## Implementation plan

1. P09：source-native compiler、unit全体の分類、検証entry、元object／記録／header／binding、manifest／build識別、排他的出力公開。
2. P10：productionのscope付きCopy／owned／C consumerと`link-native`。不一致SDK／rustc／target／dataをnative呼出し前に拒否し、元runtime／context契約を保持。
3. P07：上限付きfile tableと外部toolchain要件を持つcompiler／runtime／依存／header／startup SDKの組立。
4. P02：両tier-1でSDK／applicationをrepository、target/deps、source、test helperから隔離し、実Copy／ownership／C callerをcompile／link／run。
5. Q01／Q02、B01–B04、G01–G03、最終英日guide Z01を維持。

Frontend言語、標準library、package managerの機能は追加しません。Probe、manifestだけ、tree内link成功をSDK install完了に数えません。

## Test plan

Portable／native golden byteを維持します。CLI／source profile、0以外のmain ID、C library分類、host／grant不足、入力／出力alias、既存出力、同時／失敗公開を検証します。Manifestの全切断／tag／key／path／hash／build／toolchain／target拒否、予算、構造化errorを呼出し前に確認し、JSONをtrustedとみなさず不正／fuzz入力でparseを制限します。

Mac AArch64／Windows x86-64 MSVCの導入済みSDK出力から実exe／C callerを動かします。Pure Copy aggregate／call、ownership全family、明示panic、設定制限／OOM／poison／retry、C scalar／Unit／aggregate／256引数、元context／provider／grant、完全cleanupを確認します。値／usage／report／source識別を適切なscopeで既存VM／native oracleと比較します。元sourceを削除し、repository／target/depsへアクセスさせません。既定生成codeにfrontend／backend／JIT／test Runtimeの呼出し経路がないことを検証し、SDK／build不一致をguest再生成で修復しません。

重点検証後にcode baselineを実行し、文書だけのRFC／計画変更はrepository consistencyを実行します。後続性能／debug条件は専用測定と実debugger証拠を必要とします。

## Open questions

契約上のblocking questionはありません。未検証の実装仮説は、最小Rust artifact closure／platform link flags、一貫したbuild識別、大規模programでのmanifest／component上限、C hostの使い勝手、Windowsの完全隔離installです。P09／P10／P07／P02で実験し、受理済み契約を変える前に本RFCを明示改訂します。将来Cだけでbootstrapすることは任意の後続作業です。

## Sources

- [SDK実験producer](../../experiments/sdk-owned-producer.rs)、[consumer](../../experiments/sdk-owned-consumer.rs)、[guest](../../experiments/sdk-owned.lang)：`2712700`に対する実験、2026-10-20 JST実行・確認。起動時に元sourceを移動し、42／live allocation 0を確認。
- 導入済みrustc：`1.99.0`、commit `b940084d7eb6a299eb4bfeb8e34901bc051e7ac4`、commit日2026-09-28。`rustc --version --verbose`を2026-10-20 JST確認。環境証拠であり、versionをまたぐABI互換性の主張ではありません。
- [C profile](0008-cleanup-allocators-and-c-boundaries.md)、[native continuation](0012-native-ownership-continuations.md)、[C object bridge](0015-c-native-object-bridge.md)、[schema 4](0017-c-aware-ownership-artifacts.md)、[native record](0018-native-compiler-records.md)、[通常entry](0019-owned-native-entry-runtime.md)：現repository契約、2026-10-20 JST更新・確認。
- [CLI](../../../src/main.rs)、[固定prototype tool](../../../src/bin/prototype-object.rs)、[実装Todo](../milestone-2.md)：`0295793`時点の現状を2026-10-20 JST確認。上記新command／manifestはproject判断で、外部性能主張は使用しません。
