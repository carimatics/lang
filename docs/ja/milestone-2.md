<!-- translation-of: ../milestone-2.md; canonical-language: en -->

# Milestone 2 実装Todo

[English](../milestone-2.md) · [完了条件](roadmap.md#milestone-2ownershipとnative-aot)

更新・確認日：**2026-10-18 JST**。初回監査の基準commit：`0c2c012`。

**42 done · 0 doing · 49 todo · 125 remaining pt**

**次: M2-V05 → Text clone／concat／to-bytes／equalityをallocator・失敗の裏付け付きで実行する。**

直近の完了：**M2-V04**（`8b24c94`）。Bytes get／set／push、exclusive byte loan、所有権を消費するUTF-8変換、Text byte長を実行します。[変更fixture](../../examples/analysis/extraction.lang) は `1`、[UTF-8 script](../../examples/scripts/utf8.lang) は `3` を表示します。VM 20／CLI 24 testで厳密growth request／変換時確保なし、bounds／invalid byte、OOM／quota error、UTF-8 offset／NUL／空値、全fuel prefix、操作途中interruption、loop再利用を検証します。判明したCopy cellのloop制約を、affine上書きを許さず修正しました。全体の **71 suite・975 test**、fmt／Clippy／doc、fuzz workspaceのfmt／全bin locked Clippyが成功しています。

直近code baselineの実測（2026-10-18 JST）：workspace Clippy **9.3秒**、全test **169.5秒**、rustdoc **8.9秒**、fuzz Clippy **0.4秒**。Command所要時間であり、全稼働時間ではありません。Logだけでwall timeの全内訳は特定できません。対象testでloop制約を発見／修正してから全test baselineを実行しました。まとまったcode成果ごとにbaselineを1回実行し、文書のみの更新はconsistency checkを使います。

現在のsource対応範囲：引数なしi64 mainでBytes確保／変更／読み取り、UTF-8 Textへの変換、Text byte長、Bytes／Text／Copy sumの構築／取り出し、borrowed Result match、正常／失敗exitのcleanupを実行できます。Borrowed matchは維持します。次はText clone／concat／to-bytes／equalityです。V05行には既存要件の `text_equal` を明記し、既存見積もり2 ptを維持します。他owning service、一般source aggregate、REPL／native実行、ownership artifactは各gateに残します。未対応命令／payloadはprovider callback前に拒否します。

従来の完了条件は **18件中5件完了** です。完了成果にはcompiler／runtimeの前提作業と初期source実行fixtureを含み、core-libraryやbackend機能全体の完了を意味しません。

この文書を実装Todoの正本とします。Roadmapには従来の受入条件とその履歴上の重みを残します。件数は独立して検証した成果の数であり、各件の作業量が等しいという意味ではありません。残りpointは積み上げによる暫定見積もりで、native／C／platform統合の確度は低く、時間や完成時期の予測ではありません。完了済み作業に後付けのeffort pointは与えません。完了行の *残り* pointは0です。

## 従来の数値が止まった理由

`367218e`（2026-10-18 00:14:21 JST）から `0c2c012`（03:24:22）まで、完了条件の重みは34 / 113のままでしたが、実装9commitとroadmap 9commitが追加されました。記録されたpassing-test baselineは840件から930件になりました。差分は48file、10,407行追加、78行削除です。Commit日時が示すのは3時間10分の区間であり、実測した稼働時間ではありません。Test数や行数は変更の裏付けであり、生産性や完成機能の指標ではありません。

原因は次のとおりです。

1. 大きなend-to-endの受入条件を日々のTodoにも使っていました。たとえばdrop項目には、cleanup engine、生成range、構築順序、入れ子report、failure CFG、allocator、layout、本番実行が隠れていました。前提作業が完了しても数値が変わりませんでした。
2. RFC 0007／0008で要件が確定した後も、残作業を積み上げずに113 ptを妥当と繰り返し判断しました。Ownership VM実行、fallible allocationの統合、ownership artifactのencodingなどは主に本文中にしか現れていませんでした。**113 ptを作業量予測として使うことを取りやめます。** 残り79の受入条件の重みは、実装残作業の妥当な見積もりではありません。
3. Compiler／runtimeの基盤を横方向に進め、ソース実行が未対応のままでした。必要な前提作業もありますが、利用可能な経路を試す前に基盤を追加し続けることへの明示的な制限がありませんでした。設計検討の長期化も最初の具体的な変更を遅らせました。
4. 各実装で長い履歴reportと別のroadmap commitを追加し、活動記録は増えても現在のqueueが見えませんでした。Code sliceごとにfull baselineを再実行していましたが、所要時間は計測していないため、経過時間に対するtestの比率は断定できません。

初回の残り134 ptの作業一覧は欠けていた詳細見積もりを置き換えたもので、完了済み34 ptの付け直しや新しい完成率の作成ではありません。現在の残りpointは成果の完了で減り、再見積もりでは変更を明示します。新旧の総計は粒度が異なり、過去のcost超過率の計算には使えません。新たな作業が判明したら、task数と残りpointの増減を含めて明示的に再見積もりします。

## 作業ルール

最初の実行fixture後の監査（2026-10-18 JST）：RFC 0007の受理済みmove／match規則では、非Copyのcase bindingは読み取りloanです。生成したBytesをownedなmutable localへbindingすると、`SRC0014`（expected Bytes, found &Bytes）で拒否されることを確認しました。Allocating Option／Resultから変更／消費core命令へ進む経路を妨げるため、V04前にS01〜S03（8 pt）を追加し、明示的で互換性を持つ取り出し設計を判断してからcompiler／runtime contractを実装します。受理済みborrowed matchingを暗黙に変更しません。また新VMはCFG block argumentを明示的に拒否するため、一般guest-call conformance前にV11（2 pt）を追加します。V03で3 ptを減らし、新規4行で10 ptを加えるため、残作業は129から **136 pt**、**53 todo** へ変わります。S02／S03はS01で設計を選ぶまで暫定とし、大きい形式を選んだ場合はcode前に分割／再見積もりします。実行できたsource fixtureは完了成果として維持します。

S01判断時の監査（2026-10-18 JST）：RFC 0009は新しい消費match構文を作らず、既存generic-call構文とwhole-value argument規則を使う有限core call 3個を選びました。2 ptの設計行を完了し、残作業は **136 → 134 pt**、todoは **53 → 52件** です。S02はsignature／source／MIR検査の3 pt、S03は対応Bytes／Copy移譲とpanic cleanupの3 ptを維持します。新parser構文、任意enum取り出し、collection、native実行はこの2行の対象外です。Transfer実験まで見積もりは暫定とし、新前提が判明したら実装前に明示／分割します。S02に続くS03で次のsource owner取り出しfixtureを実行してから基盤を拡張します。

- 実装前に1行を選んで `doing` にし、同時進行は最大1件とします。1行は観測可能な1成果、残り1〜3 ptです。それより大きい、または前提が不明なら着手前に分割します。
- 計画開始から10分経ってもpatchまたは実行可能な実験がなければ、設計を広げる前にtaskを狭めて最小限の有用な実験を実行します。抽象化やhardeningの追加は、選択した成果または受理済み要件に必要な場合に限ります。取得できる検証所要時間を記録し、commit間隔を稼働時間とはみなしません。
- 直近は2件の前提作業（有限valueと上限付きframe）の後、Bytesソース実行fixtureを必須とします。別の前提が判明したら記録し、基盤作業を追加する前に順序を見直します。Fixtureを暗黙に先送りしません。
- Doneには行のoutcomeを満たすcommit（実装、または明示的に計画した設計判断）と対象を絞った検証evidenceが必要です。自分自身のcommitを参照できない場合は次の計画更新でcommitを記録し、evidenceが揃うまで仕掛かり状態を正直に保ちます。完了条件を満たすまで待たず、各成果の後にこの表を更新します。
- 対象を絞ったcheckから始め、まとまったcode成果ごとに必須full baselineを1回実行します。文書だけのfollow-upはrepository consistencyを確認し、追加code、失敗、未解決の懸念がある場合だけ広いcheckを再実行します。実装／roadmapのcommit順序は維持し、繰り返しの長文追記を行更新に置き換えます。監査記録の追記はscope、見積もり、依存、完了判断が変わった場合に限ります。
- ソース実行fixtureが増えない前提作業が2件続いたら、基盤の拡張を止め、次のvertical sliceを再検討します。漠然と「見積もりは妥当」とせず、具体的な阻害要因と残taskを報告します。
- 18件の完了条件を維持します。必要な実機tier-1実行を含め、全evidenceが揃った場合だけ完了にします。Localのobjectや設定済みCI jobはWindows native実行evidenceではありません。未完了のP1 debug作業はexit前に移動先と再見積もりを明示します。

## 完了済み実装

最初の35行はcommitからM2の実装成果を復元したものです。元のRFC 0007／0008判断は完了条件側に残します。以後の成果は、明示的に計画したfollow-up設計行を含め、計画時のIDを維持してdoneにし、各行に記したoutcomeだけを満たします。検証の詳細と限界は [architecture](architecture.md) と [roadmapの実装履歴](roadmap.md#milestone-2ownershipとnative-aot) を参照してください。

`Gate` はroadmapのchecklist順にG01〜G18です。1行が複数条件に貢献しても、作業を二重計上しません。

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-D01 | done | 0 | — | G13 | Copy profile全体のnative loweringと差分fixture。 | `926fb3b` |
| M2-D02 | done | 0 | — | G04 | Shared／weakのlifecycle、cycle publication、rollback、race。 | `ec16f8a` |
| M2-D03 | done | 0 | — | G03 | Ownership構文とformatter往復。 | `eb3b16f` |
| M2-D04 | done | 0 | — | G03 | HIRのownership型特性と上限。 | `81fab15` |
| M2-D05 | done | 0 | — | G03 | HIRのplaceとlexical arena scopeのbinding。 | `f6e0edb` |
| M2-D06 | done | 0 | — | G03 | 有限core-call signatureとprovenance template。 | `9342175` |
| M2-D07 | done | 0 | — | G03 | Core sum case、Unit value、pattern。 | `0115a46` |
| M2-D08 | done | 0 | — | G03 | 文脈を用いたbody／core型検査と網羅性。 | `8eda046` |
| M2-D09 | done | 0 | — | G05 | Lexical arena-owner authorityの型検査。 | `7213ae3` |
| M2-D10 | done | 0 | — | G03 | HIRの初期化とaffine move flow。 | `9263966` |
| M2-D11 | done | 0 | — | G03 | HIRのlexical loanとmutable access。 | `7e3d43d` |
| M2-D12 | done | 0 | — | G03 | 入れ子reference cellと推移的provenance。 | `83b5e36` |
| M2-D13 | done | 0 | — | G05 | ソースのarena brandとstorage flow。 | `3e6563a` |
| M2-D14 | done | 0 | — | G05 | 再帰guest callのregion summary。 | `b39b96a` |
| M2-D15 | done | 0 | — | G05 | Arenaのretain付きread、generation、逆順teardown。 | `02f349d` |
| M2-D16 | done | 0 | — | G03 | MIR ownership型graphの独立検証。 | `c6847a6` |
| M2-D17 | done | 0 | — | G03 | MIRのcanonical typeとsource declaration。 | `97672e4` |
| M2-D18 | done | 0 | — | G03 | MIR place／projectionの型検証。 | `51759b3` |
| M2-D19 | done | 0 | — | G03 | Ownership命令の型検証とCFG検証。 | `a3c5dce` |
| M2-D20 | done | 0 | — | G03 | MIRの初期化とmoveの独立解析。 | `9cf81bd` |
| M2-D21 | done | 0 | — | G03 | RHS effectをまたぐ代入先の事前評価。 | `9a57680` |
| M2-D22 | done | 0 | — | G03 | MIR loanの明示的な有効化／終了解析。 | `fd5f050` |
| M2-D23 | done | 0 | — | G03 | MIR reference provenanceの独立検査。 | `4c6c13e` |
| M2-D24 | done | 0 | — | G03 | MIR core-call contractとallocator要件。 | `4619937` |
| M2-D25 | done | 0 | — | G03 | Ownershipソースbodyの自動lowering。 | `39bb20e` |
| M2-D26 | done | 0 | — | G03 | Matchとprovisional-loan armのlowering。 | `ad40b7a` |
| M2-D27 | done | 0 | — | G03 | Driver／queryのownership解析とlang check。 | `d25706c` |
| M2-D28 | done | 0 | — | G06 | 順序付きcleanup engineとreturn escrow。 | `c437dc1` |
| M2-D29 | done | 0 | — | G06 | ソース生成cleanup rangeとruntime順序の接続。 | `6216b43` |
| M2-D30 | done | 0 | — | G06 | 実際のrecord初期化順序をloweringで保持。 | `a984e5e` |
| M2-D31 | done | 0 | — | G06 | 上限付きの入れ子field failure report。 | `9678072` |
| M2-D32 | done | 0 | — | G06 | Ownership failure／return CFGの生成。 | `3c3131f` |
| M2-D33 | done | 0 | — | G06 | Fallible instance allocatorと会計。 | `d7f85de` |
| M2-D34 | done | 0 | — | G06 | Managed layoutと事前確保したretirement storage。 | `f68a4da` |
| M2-D35 | done | 0 | — | G07 | Allocator付きBytes／Textと検証済みtoken cleanup。 | `82ed11b` |

## ソース実行を先行

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-V01 | done | 0 | M2-D34 | G08 | Managed Unit／Option／Result／error codec：入れ子往復、tag検証、厳密な上限。 | `71ece57` |
| M2-V02 | done | 0 | M2-V01 | G06 | 上限付きowned frameの受け入れ時、effect前にvalue／retirement storageを確保し、失敗rollbackを検証。 | `86c23ce` |
| M2-V03 | done | 0 | M2-V02,M2-D35 | G06,G07 | lang runでBytes new／lenのソースと通常dropを実行し、到達可能な未対応命令はeffect前に拒否。 | `ece8690` |
| M2-S01 | done | 0 | M2-V03 | G07,G08,G09,G10 | RFC 0009でSome／Ok／Err全体取り出しを受理。Borrowed match維持、明示panic／cleanupと互換性規則。設計のみ。 | `c4cb8b9` |
| M2-S02 | done | 0 | M2-S01 | G08 | RFC 0009の3操作を型検査し独立検証済みMIRへlower。Copy／move、loan、region、入れ子sumのcompile pass／fail。Runtimeはgateを維持。 | `b562c0f` |
| M2-S03 | done | 0 | M2-S02 | G06,G08 | 有限Bytes／Copy sumと3種のwhole-payload取り出しをtransfer確保なしで実行。Source fixture、panic／fuel／OOM cleanup、managedな入れ子順序／span移譲を検証。 | `e584902` |
| M2-V04 | done | 0 | M2-V03,M2-S03 | G07,G08 | Source Bytes get／set／push、UTF-8変換、Text byte長を実行。Bounds／OOM、変換確保数、fuel／interruption cleanup、Copy cell loop再利用を検証。 | `8b24c94` |
| M2-V05 | todo | 2 | M2-V04 | G07 | Text concat／clone／to-bytes／equalityのソース実行でallocator identityと失敗結果を保持。 | — |
| M2-V06 | todo | 3 | M2-V03,M2-S03 | G06,G08 | 入れ子record／enumのowner moveとreference projectionを実行し、dropが1回であることを検証。 | — |
| M2-V07 | todo | 3 | M2-V06,M2-V11 | G06 | Owned guest call／resultを再帰上限とreturn escrow付きで実行。 | — |
| M2-V08 | todo | 3 | M2-V07,M2-D32 | G06 | 生成failure exitでfuel／OOM／body failure、cleanup順序、入れ子reportを実行検証。 | — |
| M2-V09 | todo | 2 | M2-V04,M2-V06 | G07,G08 | Provisional-read match armとRHS effectをまたぐ事前評価済み代入を実行。 | — |
| M2-V10 | todo | 2 | M2-V08,M2-V09 | G08 | REPLのownership受け入れ／結果表示をrunに揃え、失敗を分離。 | — |
| M2-V11 | todo | 2 | M2-V06 | G08,G14,G16 | 上限付き事前予約scratch storageで、cycle／loopを含むCopy／affine CFG block argumentの同時転送を実行。 | — |

## Shared、arena、collection

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-R01 | todo | 3 | M2-D24 | G05 | MIR arena-brand／call-effectを独立検証し、偽造escapeをbudget付き診断で拒否。 | — |
| M2-R02 | todo | 3 | M2-D02,M2-D33 | G06 | Shared storageをguest fallible allocationに接続し、payload／retirement全領域のpreflight／rollbackを会計。 | — |
| M2-R03 | todo | 2 | M2-R02,M2-D34 | G06 | 本番shared／weak token adapterで所有権を検証し、test registryなしでpayload子孫をretire。 | — |
| M2-R04 | todo | 3 | M2-R03,M2-V08 | G06,G14 | Shared／weakソース命令、upgrade、cycle拒否、rollbackを実行。 | — |
| M2-R05 | todo | 3 | M2-D15,M2-D33 | G05,G06 | Arena payload allocationと子孫retirementにguest budgetと失敗rollbackを適用。 | — |
| M2-R06 | todo | 3 | M2-R01,M2-R05,M2-V08 | G05 | Arena allocation／read／reset／teardownをソースから実行し、stale referenceと逆順dropを検証。 | — |
| M2-R07 | todo | 2 | M2-R06,M2-V07 | G05 | Guest callを通じ、arena escape／call-brandのcompile-failとruntime結果を照合。 | — |
| M2-C01 | todo | 3 | M2-D33,M2-D34 | G09 | Vec storage／new／len／push growthをallocator-awareかつfailure-atomicに実装。 | — |
| M2-C02 | todo | 3 | M2-C01 | G09 | Vec read／mutable-read／take／replacementで移動／置換elementを正確に1回retire。 | — |
| M2-C03 | todo | 3 | M2-C02,M2-V09 | G09 | Vecソース命令をmove／loan／growth／failure fixtureで実行。 | — |
| M2-C04 | todo | 3 | M2-D35,M2-D34 | G10 | Map storage／lookupで受理済みの厳密key equalityとallocation budgetを実装。 | — |
| M2-C05 | todo | 3 | M2-C04 | G10 | Map insert／replacementをfailure-atomicにし、置換valueを1回だけretire。 | — |
| M2-C06 | todo | 3 | M2-C05,M2-V09 | G10 | Mapソース命令を独立したequality／replacement／failure oracleで検証。 | — |

## Ownership artifactとC境界

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-A01 | todo | 2 | M2-V08 | G14,G16 | Ownership artifactのversion／encodingを互換性と検証上限付きでRFC受理記録にする。 | — |
| M2-A02 | todo | 3 | M2-A01 | G14,G16 | Ownership型／命令のencode／decodeに不正入力budgetとfuzz targetを追加。 | — |
| M2-A03 | todo | 3 | M2-A02 | G16 | Owned source artifactをload、独立検証、実行し、往復時のbehavior一致を確認。 | — |
| M2-F01 | todo | 2 | M2-D27 | G11 | 受理済みunsafe／source境界規則をsource診断とcompile-fail fixtureで強制。 | — |
| M2-F02 | todo | 3 | M2-F01,M2-V08 | G11 | C境界のvalidity／capability／poison preflightとunwind containmentを失敗fixtureで検証。 | — |
| M2-F03 | todo | 3 | M2-F01 | G12 | 厳密なC descriptor／digest／headerを生成し、受理済みvectorでlayoutを検証。 | — |
| M2-F04 | todo | 3 | M2-F02,M2-F03 | G12 | C importでscalar／aggregate引数、allocation context、failure statusを扱う。 | — |
| M2-F05 | todo | 3 | M2-F04 | G12 | C exportで規定status／resultを返し、入れ子cleanup failureをcontain。 | — |
| M2-F06 | todo | 2 | M2-F05 | G11,G12 | Headerを独立compileし、native macOSで境界validity fixtureを実行。 | — |
| M2-F07 | todo | 2 | M2-F06 | G11,G12 | Native Windowsで対応するC境界fixtureを実行し、成功CI evidenceを記録。 | — |

## Native、packaging、最終evidence

| ID | State | Remaining pt | Needs | Gate | 成果／受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M2-N01 | todo | 3 | M2-V07,M2-A02 | G14 | Owned function／returnのmachine conventionとmanaged-value object layoutをemit。 | — |
| M2-N02 | todo | 3 | M2-N01 | G14 | Nativeのowner move、aggregate／sum命令、loan projectionをVM fixtureに一致させる。 | — |
| M2-N03 | todo | 3 | M2-N02,M2-C03,M2-C06 | G07,G08,G09,G10,G14 | Native core dispatchで同じBytes／Text／Vec／Map fixtureを実行。 | — |
| M2-N04 | todo | 3 | M2-N02,M2-R04,M2-R07 | G05,G14 | Native shared／arena dispatchでlifecycleとbrand検査を保持。 | — |
| M2-N05 | todo | 3 | M2-N02,M2-V08 | G06,G14 | Nativeの生成failure／return exitをVMのcleanup順序とfuel behaviorに一致させる。 | — |
| M2-N06 | todo | 2 | M2-N03,M2-N04,M2-N05 | G14 | Native macOSでowned programのdirect／response-file object link／runを検証。 | — |
| M2-N07 | todo | 2 | M2-N06 | G14 | Native Windowsで対応owned programを実行し、成功CI evidenceを記録。 | — |
| M2-P01 | todo | 2 | M2-N06 | G15 | 両tier-1でlinker discovery／出力path失敗に対処可能な診断を返す。 | — |
| M2-P02 | todo | 3 | M2-P01,M2-N07,M2-F05 | G15 | Startup／runtime／context依存をpackage化し、clean installでlink／run成功。 | — |
| M2-Q01 | todo | 3 | M2-A03,M2-N07,M2-F07,M2-V10 | G16 | 複合機能の代表ソースfixtureでVM／nativeの成功／失敗conformanceを検証。 | — |
| M2-Q02 | todo | 2 | M2-Q01,M2-P02 | G16 | 両tier-1 CIで代表conformanceとresource-limit scenarioの成功を記録。 | — |
| M2-B01 | todo | 2 | M2-Q01 | G17 | 同等Rust／C workloadとharnessで比較前に意味的一致を確認。 | — |
| M2-B02 | todo | 2 | M2-B01,M2-P02 | G17 | CleanなmacOSでruntime／startup／memory／sizeとばらつきを記録。 | — |
| M2-B03 | todo | 2 | M2-B01,M2-P02 | G17 | 同等のclean Windows計測とばらつきを記録。 | — |
| M2-B04 | todo | 1 | M2-B02,M2-B03 | G17 | 結果の解釈、再現command、残るperformance上の限界を公開。 | — |
| M2-G01 | todo | 3 | M2-N06 | G18 | Native owned-valueのsource span／locationをemitし、macOS debug fixtureで検証。 | — |
| M2-G02 | todo | 3 | M2-N07 | G18 | 対応するWindows native debug metadataをemitし、source locationを検証。 | — |
| M2-G03 | todo | 2 | M2-G01,M2-G02 | G18 | 両tier-1実機でdebuggerのstep／backtrace evidenceを記録。 | — |
| M2-Z01 | todo | 1 | M2-Q02,M2-B04,M2-G03 | G05,G06,G07,G08,G09,G10,G11,G12,G14,G15,G16,G17,G18 | 全exit evidenceを監査し、英日M2 capability guideを公開。 | — |

## 情報源と確認日

- Local Git履歴 `926fb3b`〜`0c2c012` と差分 `367218e..0c2c012`：commit日は2026-10-17／18 JST、確認日は2026-10-18 JST。
- [Architecture](architecture.md)、[roadmap](roadmap.md)、そこから参照する実装／test：更新・確認日2026-10-18 JST。930 testは今回の監査前の実装checkpointであり、この計画変更で増えたtest数ではありません。
- 受理済み [RFC 0007](rfcs/0007-source-ownership-and-core-values.md) と [RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)：最終更新日は両方とも2026-10-17、今回のtask一覧のため2026-10-18 JSTに確認しました。Web調査や外部の生産性benchmarkは使用していません。
