<!-- translation-of: ../../rfcs/0029-component-adapter-work.md; canonical-language: en -->

# RFC 0029: Component adapter work と invocation 計量

[English](../../rfcs/0029-component-adapter-work.md)

- Status: implemented
- Authors: Project contributors
- Created: 2026-10-24
- Last updated: 2026-10-25
- Implemented: 2026-10-24
- Tracking issue: M3-P02
- Supersedes: なし。component profile について RFC 0013/0027 を具体化
- Superseded by: なし

E02同期childの受理判断（2026-10-25 JST、RFC 0001実験段階の委任）: reentry opt-inのnative methodも、完全unitの既存metering対応検査を通り、元control/accountingとServiceWorkを同一のまま借用する場合に有限workを受理します。child実行がServiceWorkLimit/CounterOverflowで失敗した場合は、元accountingに最初の型付き原因を保持し、復元した親は既存のABI status妥当性検査を維持したうえで、ABI上有効なhost statusや返却bufferを解釈する前に確認します。これによりraw host-error statusでquota原因を隠したり、結果を公開したりできません。既存のcanonical準備失敗の保持と同じinvocation単位の終端性を用い、counterを増設しません。cleanupは元allocator/ownerでunmeteredのまま行い、単なる枯渇ではpoisonしません。元counterのVM/JIT実callbackでゼロ/全prefix/上限一致・反復・最初の原因/数値・cleanup・fresh parentを検証します。一般のchild診断/stack・task identity・active-instance/typed transport・公開Cと両hostの条件は維持し、formatは変更しません。

## 概要

component root invocation は引数準備から結果公開まで既存の `ServiceWork` counter を1つ所有します。canonical value の観測、payload copy、UTF-8 検証を計量し、guest operation・service・native adapter が正確に同じ counter を共有します。全受入 operation を網羅するまで有限 component 実行は拒否します。有効化の前に、RFC 0027 が要求する event catalog を記録します。

2026-10-24、RFC 0001 の実験的委任で受理しました。元 counter の共有と全 event の受入検査は妥当と判断し、受理時点では実装と差分拒否の証拠を P02 の義務として残しました。

実装 checkpoint（2026-10-24 JST）: root counter と scoped Session を共通 Machine/service へ接続しました。保存 native-import の全 work prefix、owner/loan と rich result、未使用 reentry method の拒否、準備 failure の維持、新 root の初期化、destructor 失敗時の primary/suppressed、厳密な UTF-8/copy/0-byte node 単位を VM/生成 native と wire 回帰で確認しています。既存 wire bytes は一致します。P02 は全 baseline・fuzz compile/lint・commit 証拠まで doing を維持し、安定化/両 host 証拠は後続です。

## 動機

`3f35d0c` では decode 済み resource/native-import fixture が無制限 control で成功します。service-work 上限0の同じ fixture は VM/生成 native とも引数 commit 前に `Unsupported`、provider 呼出0回です。共通 runner は有限 import を拒否し、通常 service counter は component 引数準備後に作成します。canonical adapter は既に有限の wire Session を共有しますが、その準備 budget は invocation の service counter ではありません。これらは2026-10-24に確認した code/probe の観測であり、性能の証拠ではありません。

## 目標

- 既存の累積 byte/work/storage/element/depth 上限で非信頼 metadata と adapter 準備を制限します。
- rich value・import・元 cleanup を含む実 VM/native component 経路で、runtime work と拒否時の所有権結果を共有します。

## 非目標

新構文、別 allocator/registry/interpreter、CPU時間保証、任意の信頼 native 命令の計量、新永続形式、残る public embedding/reentry・合算 heap/control・installed package・両 host 安定化 delivery の完了。

## 利用者向け設計

明示的な checked component invocation に既存の `Control::and_service_work(Some(limit))` を使います。runtime event の拒否は limit・used・requested と元 operation span を保つ既存の構造化 service-work control failure です。受入/codec 準備 error の identity は維持します。枯渇だけでは poison せず、必須 cleanup と invariant 再検証で再利用可否を決定します。通常 CLI は引き続き明示 component binding を要求します。

## 参照意味論

### 準備と root identity

source/IDL/artifact 検証、snapshot pairing、grant、provider metadata、signature 構築、adapter plan は有限の準備 Budget/Session を維持します。各準備 operation は prior usage を払い戻さず引き継ぎ、grant/provider 全 batch と呼出中の canonical 処理で供給 Session を reset しません。これは準備 work であり、追加 service event ではありません。未使用 import/宣言も discovery/publication 前に検証し、native constructor を互換性 query に使いません。

実際の信頼 invoker の control 設定を使い、component root invocation の準備時に `ServiceWork` を1つ作ります。共通 Machine は別 counter を作らず同じ handle を受け取ります。副作用前に control/counter の対応を検証します。root canonical 引数操作と結果公開も guest entry 前/最終命令後を含め同じ counter を使います。拒否した試行・callback 失敗・後続呼出で先の受入 work を払い戻しません。新しい外側 invocation は新 counter を開始し、将来の nested call は親 counter を引き継ぎます。

### Cost profile 1 の component event catalog

| Event | Units |
| --- | --- |
| component value の変換/encode/decode 中に dynamic managed/canonical value node を1つ観測 | 1 |
| native frame/report buffer の実 runtime copy を含む n payload byte の copy | n |
| n UTF-8 byte の検証 | n |
| typed managed collection data の copy/grow/replace | 既存 RFC 0013 の Layout/active extent 規則 |
| 固定 envelope/slot/header 検査、claim/index/loan bookkeeping、opaque handle の移動 | 0 |

各 traversal の実 value 観測を一度だけ計量し、alias や静的 type 展開から dynamic value を増やしません。source→arena、arena→wire、wire→arena、arena→guest は別の traversal event です。0-byte Unit 配列も要素を観測します。aggregate 再構築、scratch ID、allocator 初期化で payload copy を増やしません。既知の copy/validation batch は依存 allocation/publication 前に受け入れます。受入済み検証は data error を発見できますが、拒否された検証は byte を走査しません。bulk event は全 extent を計量します。managed service の既存 metered entry と共有 counter を再利用し、同じ copy/観測へ追加課金しません。

検証済み raw backing view の返却だけでは byte を copy/scan しません。その view による任意 native read/write は信頼 opaque work です。実 runtime backing publication copy は課金し、context buffer の backing への移動は handle transfer です。native buffer の allocation/zeroing は allocation 計量であり service work ではありません。guest fuel・host-call 課金・協調割込位置は維持します。

### 拒否、cleanup、provenance

対応する global event より先に local 上限を確認し、拒否された event は対応するどちらの work counter も変更しません。別の先行準備/event の受入 usage は維持します。commit 前の拒否は caller owner を保ち、commit 後は rollback せず受入 value を退役します。resource commit 前に表現を予約します。caller 表現、必須 callee cleanup、invariant 検査が公開を許すまで結果を escrow に保持します。

wire/marshalling の service-work failure と overflow は、不正 host result の poison ではなく元 control failure へ投影します。root 準備中の失敗でも数値 usage を維持します。最初の primary と順序付き suppressed cleanup failure を保ちます。必須の loan 終了、temporary/result 破棄、resource destructor、service 退役は元 allocator/table/code owner を使い、計量しません。native 結果 decode と guest 結果 staging 中の拒否にも適用します。

## 実行モード

両 component adapter は共通 Machine と canonical service を使います。有限実行前に未使用 function/import を含む完全 verified unit を調べ、checked component Program・全 typed grant・全 event の網羅を要求します。通常 task/C/ownership profile の受入 predicate と cost は維持します。生成 native body は生成 code のままです。optional JIT は既存可用性規則に従い、installed package・public callback・両 host runtime conformance は後続 gate に残します。

## ABI と互換性

従来未受入の有限 component profile に限り cost profile 1 を拡張します。既存非 component program の課金と fuel/poll 順序は不変です。定義した usage/overflow 計量を除き、既定の無制限 component 動作を保ちます。source 構文/type、C ABI v1、LCMP3/LCVL1/LCSG1、LANGBC5、native-package/SDK 形式へ field/tag を追加せず、既存 report/control identity が数値 failure を運びます。

実験的 Rust component invoker/host interface は実 root の work 設定/counter を公開する必要があります。同期 `ServiceWork` handle を一時保持する wire Session は idle 時も Send/Sync ではなくなるため、準備/実行 Session を実行 thread で作ります。immutable contract と直列化 byte は runtime counter を必要としません。scope guard は return/unwind 時に Session の以前の対応を戻し、foreign counter で active root を置換できません。これは明示的な Rust API 影響であり、安定 C layout の変更ではありません。

## 診断とツール

構造化準備/codec 診断と元 source snapshot を保ちます。service-work 拒否は既存 control/report identity と数値 field を使い、一般 component-frame/native-contract error に変換しません。parser・formatter・source 推論は不変です。完全な debug/callback stack は E03/E04 の範囲です。

## 性能とコストモデル

同期 Rc/Cell counter を1つと既存の反復 traversal を再利用します。Session の scope 対応付け、counter 検査、保持準備 buffer には cost があり、overhead や速度を主張しません。論理単位は CPU時間・allocator 初期化・信頼 native work を制限しません。測定は Z01 に残します。

## セキュリティと信頼境界

source/artifact/IDL/value frame は非信頼かつ有限であり、grant を provider discovery より先に確認します。counter 共有で native/resource 権限を付与しません。不正 native result は host-contract failure のままで、work 拒否だけではそうなりません。native code は信頼され、foreign exception を封じ込める必要があります。枯渇後も cleanup は公開/再入できません。

## 代替案

- 変更なしでは有限 component 拒否が残り、P02 は未完です。
- caller/library counter では共有 service、早期準備、native 結果 cleanup を網羅できません。
- 全呼出の完全 transaction preplan は強力ですが、任意 native 結果の work を先に知ることはできず、plan/traversal を増やします。有限受入 prefix と escrow/cleanup で所有権を維持します。
- adapter 呼出ごとの新 counter は先行 work を払い戻します。準備 Budget の実装 step 全体を runtime event にする方が簡単ですが、alias 展開/metadata algorithm/固定 frame が service cost に入ります。選んだ論理 event は既存 service model を保ちます。

## 実装計画

選択済み P02 の3-point outcome 内で、まず不完全な有限受入を開かず早期 root counter の identity/引継ぎを確立し、value/copy/UTF-8 と managed staging event・control 投影を接続し、最後に全 unit の有限受入を有効化して decode 済み resource/native-import の拒否・厳密上限・両 engine の cleanup を証明します。既存 table/allocator/Session/service/Machine を再利用します。範囲外の具体的前提が不足すれば実装前に公開・再見積りします。

## テスト計画

decode 済み source/native-import fixture から始めます。VM/native の厳密 usage、0/ちょうど/1不足と失敗 prefix、provider 呼出 prefix、元 span、poison/再利用、allocation/destructor balance を比較します。Unit/0-byte collection、scalar/rich value、Text/Bytes/list/record/variant/Option/Result、resource owner/loan、不正/未使用 import・遅い拒否、report byte、overflow、複数呼出、準備失敗を網羅します。local quota の優先順、非計量 cleanup、旧 profile 拒否/cost、正確な wire byte を維持します。repository baseline と bounded fuzz-target compile/lint を実行し、campaign・両 host 実行・性能は別証拠を要求します。

## 未解決事項

意味論上の blocking question はありません。早期 counter 引継ぎの cost、staging の完全網羅、failure context 再利用、Session scope の使いやすさは未検証です。nested public callback、合算 heap/frame/task/interruption control、installed package、両 host 実行は後続の必須作業です。

## 出典

- [RFC 0013](0013-service-byte-work-budget.md)、[RFC 0027](0027-production-component-interface.md)、[RFC 0028](0028-task-component-artifacts.md) は2026-10-24更新。RFC 0013 の以前の service-only catalog は2026-10-22更新。
- `3f35d0c` の共通 runner/service 準備、component execution/canonical codec、`tests/component_source_resource.rs` を2026-10-24 JSTに確認/probe。code に更新日の埋込みはありません。上限0の local probe は VM/生成 native の `Unsupported`・引数未commit・provider 呼出0回を示し、提案した計量実装の証拠ではありません。
