<!-- translation-of: ../../rfcs/0031-installed-native-task-packages.md -->

# RFC 0031: Installed native task packages

[English](../../rfcs/0031-installed-native-task-packages.md)

- Status: implemented
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-27
- Last updated: 2026-10-28
- Tracking issue: M3-I14 and component/both-host follow-up M3-I08 (complete)
- Supersedes: None; extends RFC 0020 with independent manifest revisions
- Superseded by: None

## 現在の範囲

2026-10-28 JST照合。Program／SDK version 2はtask profileを維持し、RFC 0032が別途version 3のcomponentを追加します。[M3終了監査](../milestone-3-capabilities.md)に`8f41f44`でのmacOS／Windows両方の7 profile分離startup成功と、その後の`9e78f2d`での全適合性検証を記録しています。I08は完了しています。以下の最初のmacOSのみのcheckpointは履歴であり、現在のWindows対応を示しません。

## Summary

独立検査済みschema-5 task codeとRFC 0030 native recordに対して、native program manifest **version 2 / profile `task`**とSDK manifest **version 2**を受理します。元objectを厳密なinstalled SDKとリンクし、consumer startupでrepositoryアクセス・source解析・コード生成・Cargo・JITを使わず既存task runnerで実行します。

2026-10-27 JST、RFC 0001の実験期委任で受理し、`042fc38`で実装しました（同日03:49 JST検証）。cleanな厳密commitからprovenance付きSDKを作り、producer退避・checkoutなし・Cargoなしのconsumerでcopy/owned/C/task/owned-taskの5 profileが成功しました。起動前にsource/package/private SDKリンク入力を除去し、両taskは42を返します。149 suite・2,300 testの全baselineも成功しました。Windowsは未検証で、I08は未完です。component package受理には実装前の後続決定が必要です。

## Motivation

compilerは現在native package公開でProgram::Tasksを拒否します。両manifestはversion 1のみ、program profileはcopy/owned/C、SDK startupにはtask facadeがありません。RFC 0030データを暗黙にversion-1 packageと扱えません。既存のコピー済みSDK linkerと有限なtrust-boundary検査を再利用できます。

## Goals

- 元objectから実installed task packageをcompile・公開・検証・link・実行する。
- 独立semantic/record検証・厳密なSDK/toolchain/image identity・元allocator/control・必須task cleanupを保つ。
- version-1 manifest/profileと拒否境界の互換性を保つ。

## Non-goals

- component package受理・暗黙provider/grant・C embedding変更・動的native load。
- source構文・LANGBC・LANGNAT・非公開task packet・既存C ABIの変更。
- performance/安定化の主張、target記述検査をWindows実行と扱うこと。

## Guide-level design

`lang native`は既存の検査済みsource Programからtask profileを選びます。schema 5へencodeし、canonical bytesを独立decodeし、選択したhost task entryを出力してLANGNAT-2 Task recordへ切り離し、完全packageを原子的に公開します。生成する既定Rust mainは明示host allocatorとtask実行上限をscoped SDK facadeへ渡し、i64結果を表示します。custom Rust hostは既存の明示host source経路を保ちます。

コピー済みSDKとpackageを`link-native`へ渡します。元入力の検証/snapshot、installed rustc release/commitの確認後、同梱libraryでrustc/linkerを起動します。installed consumerのbuild/runtimeはCargoやrepository pathへ依存しません。VM/JITは別の明示modeのままです。

## Reference semantics

RFC 0020の厳密なJSON key・role/path規則・重複/未知field拒否・digest/size検証・有限上限を保ちます。新key/roleは追加しません。

| Manifest | Revision/profile | Required data |
| --- | --- | --- |
| Program | version 1, copy/owned/C | 既存の解釈を維持 |
| Program | version 2, task | 同じkey・非null entry FunctionId・schema-5 task意味論・LANGNAT-2 Task record |
| SDK | version 1 | 既存の解釈を維持し、version-2 task packageを受理しない |
| SDK | version 2 | 同じkey・task startupを備えた厳密なcompiler/runtime/toolchain/dependency closure |

Program version 2は`task`だけを受理し、version 1では拒否します。componentと未知profile/versionは明示拒否を維持します。SDK version 2はbuild identityが一致する既存version-1 profileをリンクできます。task packageはcompiler/linker起動前にSDK version 2を要求します。SDK assemblyはversion 2を既定にし、旧package用version-1 readerを保ちます。新task対応をversion 1へ読み替えるmodeは設けません。

task package roleはsemantic `program.lbc`・native record `program.native`・target object `program.o`/`program.obj`・生成`bindings.rs`・既定`main.rs`が各1件で、C headerを許可しません。bindingは記録されたcanonical symbol、厳密なBUILD_ID/TARGET、includeしたsemantic/native bytesを使います。pointer・task・allocator・capability・grantを保存しません。

Task schema 5・有効な元の引数なしi64 entry・manifest/semantic/nativeのentry ID一致・LANGNAT-2 Task kind・厳密なtarget/build・元objectの一致・有効で有界なmetadataを独立に要求します。native guest効果前に不一致を拒否し、manifest/file/profile/semantic受理をtoolより先に行います。runtime復元と全必須symbol解決はtask入場より先です。hashは組合せ/完全性を検証し、nativeを認証しません。

初期task object producerはRFC 0030の既存emitterに合わせてhost targetだけを扱います。別tier-1 target要求はhostへ置換せずpackage公開前に明示拒否します。両host受入条件には各実tier-1 hostでproducerを動かす必要があります。Copy/ownership/Cのcross-target動作は不変です。

SDK task facadeはsemantic bytesを独立decodeし、記録entryを確認してRecordedObject準備/bindingを再利用します。caller allocator・元Control・runner上限を受け取り、task usage・source span・cleanup・poisonを含む元の構造化outcome/failureを返します。別scheduler・quota補充・source再parse・object出力・fallbackを許可しません。scoped callの間は厳密なcode ownerを保持し、明示unsafe image/resolver義務を見える形で保ちます。

## Execution modes

task VM/JITは不変で差分比較の基準になります。installed native実行は生成演算と既存task primitiveを使います。本決定はcomponent/C/WebAssembly実行modeを追加せず、明示component bindingとinstalled packageはI08の作業に残します。

## ABI and compatibility

Program/SDK JSON revisionはLANGBC 5・LANGNAT 2・native task packet r1から独立します。task profileをversion 1へ足しません。旧readerはversion 2を拒否し、新readerはversion-1 profile動作を維持します。厳密なbuild identityのprogram/SDKを一緒に再buildします。panic=unwind・固定rustc release/commit・target・feature/profile方針・元path security・file table・原子的出力を保ちます。後のcomponent profileには明示manifest/SDK互換性決定が必要で、このtask区間だけでI08/B01を完了しません。

## Diagnostics and tooling

構造化したversion/profile/target/entry/codec/plan/binding/execution拒否を使い、適切な場合は元source spanと期待/実値を保ちます。CLI usage errorはexit 2、source/tool/runtime errorはexit 1のままです。既存source parser・formatter・LSP・DAP意味論を変更しません。検証/tool失敗時に以前の出力を保ちます。

## Performance and cost model

既存SDK dependency closureと有界file snapshot/link経路を再利用します。semantic decode・record検証・native plan準備は有限上限、invocationは元caller allocator/control/task上限を保ちます。改善を主張する前にSDK/package/startupのsizeやtimingを計測します。compile/assemblyには引き続き固定toolchainが必要で、consumer startupはcompileしません。

## Security and trust boundaries

source/semantic/native/packageデータは非信頼入力として、対応境界でtool/guest効果より先に有界検証します。native object・rustc/linker・host source・symbol resolverは明示trustedのままです。厳密なhashでnative codeをsandbox化しません。metadata検証としてprovider探索・権限付与・constructor実行を行いません。SDKをcopyしてもguestのfilesystem/process/network権限は増えません。

## Alternatives

- 変更なしでは明示拒否を保てますが、installed task受入条件を満たせません。
- startupでsource/MIRを再compileしたりCargoを起動すると、installedのコード生成なし成果を満たせません。
- 新しいC-only task runtimeや別portable実行engineはadmit済みrunnerを重複し、独立ABI/runtime設計が必要です。
- version-1 JSON再利用は小さいものの旧profile境界を弱めます。独立version 2は明示的で検証できます。

## Implementation plan

1. tool起動前の厳密version/profile/entry受理と旧形式migration/拒否testを追加する。
2. canonical task意味論・元object/record/bindings/mainを公開し、scoped SDK task facadeを追加する。
3. repository/source/Cargoが使えないコピー済みSDKからinstalled scalar/owned taskを実行する。不正/不一致入力・元上限/cleanup・rollbackを検証し、実host coverageを記録する。component package設計と両host startupはI08内で続ける。

## Test plan

version-1 vector/profileを維持します。v1へのtask・SDK v1でのtask・未対応version/profile/target・schema/kind/entry/digest/object/file不一致・厳密上限/1不足・tool前拒否を検証します。厳密なrustcとCargo/repositoryアクセスなしのコピー済みinstallationで元task/owned buffer fixtureを実行します。caller control・panic/取消し/cleanup・image owner保持・再利用を検証します。必要に応じ有界manifest fuzz coverageを拡張し、compile/lintをcampaignと扱いません。repository全検査を実行し、target記述/拒否testとは別に実Windows startupを要求します。

## Open questions

このprofileに未解決のplatform／installation blockerはありません。Component packageと明示的host startupはRFC 0032で決定済みで、完了済み[M3監査](../milestone-3-capabilities.md)にWindows／macOSの7 profileの分離検証を記録しています。以前のmacOSのみのcheckpointは履歴です。Startup観測は一般的な性能保証ではありません。

## Sources

- [RFC 0020](0020-native-sdk-and-startup.md)、2026-10-27更新。manifest・厳密なSDK/toolchain・installed linking契約。
- [RFC 0030](0030-native-task-component-records.md)、2026-10-27更新。切り離したnative task/component recordとコード生成なし復元。
- [RFC 0028](0028-task-component-artifacts.md)、2026-10-24更新。独立検証済みschema-5 task意味論。
- `8ba2c43`、2026-10-27 01:59 JST検証: 149 suites / 2,298成功。codeに公開更新日記載はありません。保存task/owned bufferリンクはruntimeの証拠で、installed SDK/Windows startupの証拠ではありません。
