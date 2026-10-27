<!-- translation-of: ../../rfcs/0033-component-v1-compatibility.md -->

# RFC 0033: Component C v1の互換性と移行

[English](../../rfcs/0033-component-v1-compatibility.md)

- Status: stabilized
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-27
- Last updated: 2026-10-27
- Tracking issue: M3-B02
- Supersedes: None; refines RFC 0027's stabilization gate
- Superseded by: None

## Summary

B01で実際のmacOS AArch64とWindows x86-64の適合を確認したcomponent C v1 interfaceを固定します。以前コンパイルしたC clientは元の厳密size tableで`lang_component_v1_get_api(1, ...)`を引き続き利用します。同じ受理contractの旧生成bindingのframe・所有権・失敗動作を保持します。安定化は両hostの旧client移行試験成功を条件とします。

## Motivation

layout vectorだけでは旧callerの所有権やerror処理を壊すruntime変更を検出できません。`40158d6`で完全queryを初めて公開し、reference headerとgeneratorはB01の`19a29c0`まで未変更です。旧clientの宣言を今後の生成とは独立に保持し、現在の実装に対して実行します。

## Goals

- v1の全layout・呼出規約・定数・32 signature・query交渉・文書化した寿命を保持します。
- 互換contractの旧生成clientでcanonical frame・signature検証・明示authority・transfer commit・cleanupを保持します。
- 独立versionのformat間で拒否と再buildの手順を示します。

## Non-goals

source構文・Rust/native language ABI・buildをまたぐprivate SDK link・任意native binary load・新platform・WebAssembly実行・汎用native sandboxの安定化は対象外です。

## Guide-level design

同じ対応targetでruntime更新時も既存v1 headerと生成clientを使えます。互換C toolchainでcompile/linkしてversion 1をqueryします。完全tableのsizeは厳密に維持し、reserved 0と正しいversion/size headerを引き続き要求します。不明version・不正requestは公開前に拒否しoutputを変更しません。v1へのfield追加はせず、非互換拡張は別名/versionのinterfaceとします。native packageは一致するSDKで再buildします。公開C tableの安定性は不一致private Rust imageの受入を意味しません。

## Reference semantics

RFC 0027の所有権・来歴・grant・resource loan・nested callback・保持する失敗report・poison・割込みを規範とします。旧callerに同じ成功/失敗とtransfer意味論を提供します。構文・推論・評価順・task scheduling・memory model・定数実行は未変更です。生成header内helperはclient codeなので、将来のgeneratorが実装改善しても旧互換callerの再生成を必須にしません。contract変更はsignature/version交渉に従い、互換性はcapabilityを与えずstale receiptを復活させません。

## Execution modes

C契約はVM/JIT source・検証済みartifactと、受理する厳密SDK AOT Imageに適用します。mode対応と明示bindingは引き続き必須です。既存両host isolated SDK probeがAOT証拠を保持します。native imageはtrusted codeとprivate pointer参照前の厳密host/build一致を必要とします。

## ABI and compatibility

`docs/component-abi-v1.h`と独立layoutを旧client fixtureに対して固定します。IDL 1・LCMP 3・LCVL 1・LCSG 1・LANGBC 5・LANGNAT 2・program 3・SDK 3は別version領域のままで変更しません。不明/非互換の保存formatは元sourceと一致toolで再encode/compileします。providerの追加memberは要求済み互換projectionを変えません。Cのmajor変更には新query/symbol namespaceを用いv1を保持します。契約上未受理だった不正入力は修正で拒否できますが、観測可能な互換性影響は実装前に記録します。

## Diagnostics and tooling

numeric status/errorとsource来歴を保持します。parser・formatter・LSP・debugger・lintは未変更です。SDK文書で安定した公開境界と実験的private linkを区別します。移行fixtureは意図的な履歴snapshotとしtest中に自動再生成しません。

## Performance and cost model

runtime実装とcostは未変更です。既存copy・有限準備・割当/control計上・owner保持costを維持します。旧clientのcompile/実行をtestに追加しますが、test時間は性能証拠にしません。

## Security and trust boundaries

副作用前の検証・明示capability・有限controlを保持します。C pointer有効性・互換toolchain・trusted native実装はhostの責任です。version/hash一致はauthorityでもnative code認証でもありません。失敗後も元scopeでのcleanupと一度だけの退役を必須とします。

## Alternatives

- 変更なし: 適合成功後も利用者に移行保証を示せません。
- 文書/toolだけの検査: 旧clientのruntime動作を確立できません。
- prefix sizeによる拡張table: 受理済み厳密size拒否と矛盾するため新versionを使います。
- layoutだけ固定: 単純ですがcodec/transfer/cleanup回帰を検出できません。

## Implementation plan

1. 旧公開referenceと生成scalar/counter clientをsource/digest来歴と共に保持します。
2. 旧header/生成scalar Cを実際の現VM/JIT hostで実行しregistry/resource/task/失敗cleanupを確認します。旧counter codecとtransfer結果も新generatorと共に検証します。
3. 全baselineとWindows CI後にB02完了と安定化を記録します。compiler/runtimeの変更は不要です。

## Test plan

独立target layout/signature vector、実旧/現C client、厳密canonical byte、不明version/不正size拒否、所有権/失敗/capability/resource/taskと両host全baselineを使います。既存SDK probeで保存AOTと隔離起動を確認し、production/SDK入力変更時に再実行します。test/文書だけの契約固定で新fuzz campaignや性能主張はしません。

## Open questions

blocking設計課題はありません。将来のplatformとmajor APIには別の受入・実行証拠を必要とします。

## Sources

- [RFC 0027](0027-production-component-interface.md)、更新2026-10-27: 規範interfaceと適合要件。
- [B01証拠](../../evidence/2026-10-27-m3-conformance-19a29c0.json)、記録2026-10-27: 実両host適合。Windows runは2026-10-27 07:45:35 UTC完了。
- [RFC 0032](0032-installed-component-packages.md)、更新2026-10-27: SDK/imageの独立trust/version。

決定: **RFC 0001の実験段階委譲によりaccepted**、2026-10-27 JST。**同日stabilized**: `9e78f2d`の旧client実行がmacOS全7段階（153 suite / 2,321成功）と[Windows全CI](https://github.com/carimatics/lang/actions/runs/40471721760)（153 suite / 2,306成功）、失敗0で成功しました。[移行記録](../../evidence/2026-10-27-m3-migration-9e78f2d.json)に固定入力・来歴・log digest・旧client 2ケースを保持します。private ABI保証は広げず、受入を妨げる未検証の仮定はありません。
