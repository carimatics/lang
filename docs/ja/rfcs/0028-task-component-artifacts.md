<!-- translation-of: ../../rfcs/0028-task-component-artifacts.md; canonical-language: en -->

# RFC 0028: Task と component MIR の直列化

[English](../../rfcs/0028-task-component-artifacts.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-24
- Last updated: 2026-10-24
- Tracking issue: M3-I10, M3-I11, M3-I07
- Supersedes: なし。別 schema で RFC 0011/0014/0017 を拡張
- Superseded by: なし

## 概要

backend に依存しない task/channel/service MIR と完全な component 宣言/binding データ用に **LANGBC schema 5** を受理します。読み込み時に既存の独立 task・ownership・effect・loan・failure・component binding proof を再構築します。schema 1–4 の byte 解釈は維持します。source の再 parse、native pointer、live handle、provider 登録、capability grant は直列化しません。

RFC 0001 の2026-10-17 JSTの委任に基づく受理は format の決定を記録し、loader の実装/安定化を意味しません。I10 が形式を決定し、続く I11 は最初の保存 task fixture の実行を必須とし、I07 が channel/service/component 統合を完了します。private native record・native package/SDK の受入・両 host installed 実行は引き続き I08 の範囲です。

実装 checkpoint（2026-10-24 JST）: I11 は Task profile 0 を共通 codec に追加し、独立 task proof・明示 entry ID・source 削除後の CLI 実行・VM/生成 native の所有値/channel/service fixture・独立 byte vector・累積上限を検証します。Component profile 1 は未対応として明示拒否し、完全 document/map admission は I07、installed native profile は I08 の残作業です。全 schema 5 の実装/安定化を意味しないため Status は accepted を維持します。

## 動機

`4ba991d` では `examples/scripts/tasks.lang` を source から実行できますが、artifact compile は “task artifacts require an accepted serialized task format” で拒否し、出力を公開しません。旧 codec は未使用 metadata を含む async 宣言・task scope・channel/service 型・component identity を意図的に拒否します。メモリ内実行や source 読み込みでは M3 の artifact 要件を満たせません。これは2026-10-24 JSTに検証した repository の観測であり、性能測定ではありません。

## 目標

- 実際の型付き意味データを保存し、全実行前提を独立に再構築する。
- 受入前に正確な field/tag、有限な読み込みコスト、元の診断 origin、旧 profile 互換性を決める。
- 既存 MIR 契約・task engine・component binding checker を再利用する。

## 非目標

- 新しい構文・source 意味論・scheduling・Send/Share 規則・authority。
- 安定した直列化、新 native/component C ABI、package/SDK revision、認証、自動 native loading。
- 中断 invocation・cleanup receipt・provider inventory・grant の永続化。

## 利用者向け設計

実装後、`lang compile --output tasks.lbc examples/scripts/tasks.lang` は MIR を保存します。`lang run --artifact tasks.lbc` は元 source を削除した後も検証済み artifact を実行します。source snapshot は診断用です。native consumer は decode した MIR を既存 task backend で compile し、保存 source の評価を fallback にしてはいけません。

component artifact は未使用項目を含む完全 IDL 宣言・source identity・binding map を保持します。読み込みで得られるのは受動的な検証済み unit です。呼び出しには既存の明示的 host binding・元 allocator・control・grant が引き続き必要です。通常 CLI は artifact から provider authority を得ません。未実装 profile は公開または provider/guest 副作用前に拒否します。

## 参照意味論

### Envelope・primitive・profile

RFC 0011 の16-byte envelope と primitive encoding を継承します。`LANGBC\0\0`、little-endian `u16` version **5**、`u16` flags **0**、`u32` payload 長です。count/ID/offset は `u32`、tag/boolean は `u8`、整数 literal は `i64`、複数 byte の整数は全て little-endian です。string は byte 長と正確な UTF-8、vector は count と要素です。`Span` は `(source, start, end)`、`TypeUse` は `(type, Span)`。optional ID は0が不在、1とIDが存在です。不明 tag、0/1以外の boolean、overflow、不正 UTF-8、長さ不一致、切断、末尾 byte を拒否します。

payload 先頭 byte は **0 Task / 1 Component** で、2–255 は不正です。どちらも生 ownership table と optional entry を持ち、Component だけが後述の document と binding map を続けます。Task は未使用を含む全 component import/resource を拒否します。Component は同期/task 関数を受け入れ、公開前に完全な独立 component binding 検証を必須とします。entry が存在する場合は既存の引数なし i64 script-entry 契約を満たし、library artifact は省略できます。

### MIR field 順序と不変 tag

明示した追加以外は RFC 0011/0014/0017 の schema 4 の全 field/tag を継承します。table 順は source snapshot、nominal definition、constructed type、import、関数宣言、関数ごとの value/place table、関数 body、optional entry です。全 list の元順序と index を保持します。source は正規化した診断 path と正確な UTF-8 snapshot を保持し、その名前/内容は file access や guest parse を許可しません。

import 行は `(name, ABI, span, parameter TypeUses, result TypeUse)` のままで、Host 0 / C 1 に **ABI 2 = Component** を追加します。関数行は `(name, public, ABI, effect, span, signature_span, parameter local IDs, result TypeUse, locals)`。関数 ABI は Guest 0 / CExport 1 のままです。**Effect 0 = Synchronous、1 = Asynchronous と元 async token の Span**。同期を含む全関数に effect byte を持たせます。async CExport は引き続き不正です。

関数 body は `(loan_count, prepared_count, task_scopes, task_bindings, blocks)`。scope は `(optional parent scope ID, Span)`、task binding は `(cleanup-target tag, target ID, scope ID)` で、target tag は Local 0 / Value 1 / Loan 2 / Arena 3 のまま、cleanup-step span は付けません。scope/binding index は body ごとに保持し、非空 task metadata を黙って捨てません。block・生 lexical cleanup step・result・place・projection・元 instruction/terminator span は schema 4 の layout を維持します。

既存 constructor tag 0–9 は Builtin・Option・Result・Vec・Map・Shared・Weak・Arena・Loan・ArenaOwner のままです。各 constructed 行は元 Span で終わります。次を追加します。

| Tag | Constructor | Span の前の payload |
| --- | --- | --- |
| 10 | Task | type |
| 11 | ChannelSetup | type |
| 12 | Sender | type |
| 13 | Receiver | type |
| 14 | SendResult | type |
| 15 | ServiceSetup | state type, message type |
| 16 | ServiceOwner | state type, message type |
| 17 | ServiceHandle | message type |
| 18 | ComponentResource | nominal definition ID |

builtin tag 0–9 は不変で、**10 TaskError、11 ChannelError、12 ServiceError** を追加します。他の builtin/constructor tag は不正です。nominal component tag は identity であり、resource payload の直列化ではありません。

### 操作

instruction layout は `(result register vector, opcode, operands, Span)` のままです。opcode 0–32 は RFC 0011 の意味を維持し、33 は1 register の RFC 0014 Panic です。Rust enum 順とは独立に次を正確に割り当てます。

| Opcode | Operation | 順序付き operand |
| --- | --- | --- |
| 34 | TaskScopeEnter | scope ID |
| 35 | TaskScopeExit | scope ID |
| 36 | TaskSpawn | function ID, argument register vector |
| 37 | TaskAwait | task register |
| 38 | TaskCancel | place ID |
| 39 | TaskYield | none |
| 40 | TaskCheck | none |
| 41 | TaskSaveReturn | value register |
| 42 | PrepareChannel | prepared ID, endpoint register, role byte, message TypeUse |
| 43 | ChannelSend | prepared authority ID, payload register |
| 44 | ChannelReceive | prepared authority ID |
| 45 | PrepareService | prepared ID, endpoint register, role byte, optional state TypeUse, message TypeUse |
| 46 | ServiceSend | prepared authority ID, payload register |
| 47 | ServiceReceive | prepared authority ID |

channel role は **0 Sender / 1 Receiver**、service role は **0 Handle / 1 Owner**。optional state TypeUse は0が不在、1とTypeUseが存在です。verifier は role に適した state/message/endpoint 関係を独立に要求し、偽造/失効した prepared authority を拒否します。opcode 48–255 は不正です。

CoreCall は `(operation byte, type-argument TypeUse vector, argument register vector, optional result loan ID)` を維持します。core tag 0–36 は不変で、**37 ServiceNew、38 ServiceHandle、39 ServiceClone、40 ServiceOwner、41 ServiceRead、42 ServiceReplace、43 ServiceFinish、44 ServiceCancel、45 ChannelNew、46 ChannelSender、47 ChannelClone、48 ChannelReceiver、49 ChannelSeal、50 ChannelUnsent** を追加します。51–255 は不正です。有効な tag でも type argument と result-loan provenance の検証は必要です。

terminator tag 0 Return / 1 Jump / 2 Branch / 3 Unreachable は不変で、**4 ReturnSaved** は operand を持ちません。全 terminator は Span で終わります。ReturnSaved は独立証明した saved-return/scope-exit 契約を必要とし、byte から runtime return cell を復元しません。tag 5–255 は不正です。

### 完全な component document と binding map

component suffix は元 IDL span と type-node ID を保つ宣言データを直接保存します。parse 用 JSON source、proof bit、Rust memory layout は保存しません。LCMP schema 3 とその正規化 wire 表現は別のまま不変です。以下の `Spanned<T>` は `(T, Span)`、IDL type ID は `u32` であり、メモリ内 index との変換時に検査します。

key は構造化 string です。PackageKey は `(namespace, package)`、InterfaceKey は `(PackageKey, interface)`、ResourceTypeKey は `(InterfaceKey, resource)`、ValueKey は `(InterfaceKey, name)`。既存 key grammar と完全 nominal identity を検証します。SemanticVersion は `(u64 major, u64 minor, u64 patch, prerelease vector, build-string vector)`、prerelease 要素は **0 Numeric / 1 AlphaNumeric** tag と string です。VersionRequirement は **0 Exact + SemanticVersion**、または **1 Interval + minimum-inclusive SemanticVersion + maximum-exclusive SemanticVersion + allow-prerelease boolean**。LCMP3 の既存 version/key 意味論と primitive layout を維持します。

document の field 順序は次の通りです。

1. `source ID, source_bytes, Spanned<PackageKey>, Spanned<SemanticVersion>`。
2. interface vector。interface は `(Spanned<InterfaceKey>, Spanned<ResourceTypeKey> vector, definition vector, function vector, Span)`。
3. import vector。import は `(Spanned<InterfaceKey>, Spanned<VersionRequirement>, Spanned<string> member vector, Span)`。
4. `Spanned<InterfaceKey>` export vector。
5. type-node vector。node は `(kind byte, payload, Span)`。

IDL field は `(Spanned<string> name, type ID)`。definition は `(Spanned<ValueKey>, kind byte, payload, Span)` で、**0 Record + field vector、1 Variant + field vector、2 Alias + type ID**。function は `(Spanned<string> name, Spanned<effect byte>, parameter field vector, result type ID, Spanned<string> capability, Span)`、effect は **0 Sync / 1 Task**です。この byte は live task を表しません。

type-node の kind tag/payload は **0 Unit / 1 I64 / 2 Bool / 3 Text / 4 Bytes**（payloadなし）、**5 List / 6 Option**（type ID 1つ）、**7 Result**（ok、errorのtype ID順）、**8 Named**（`Spanned<ValueKey>`）、**9 Own**（`Spanned<ResourceTypeKey>`）、**10 Borrow**（access byte **0 Shared / 1 Exclusive** と `Spanned<ResourceTypeKey>`）です。他の tag は不正です。type-node の共有/順序を保持し、独立検証は検証済み IDL input と同様に不正参照・再帰・alias・resource nesting・depth を拒否します。

document source は実際に保持した source snapshot を指し、`source_bytes` はその UTF-8 byte 長と一致しなければなりません。全 document span は同じ source に属し、その範囲内かつ UTF-8 境界にあり、owner-containment 規則にも従います。名前・effect・key・version・宣言をデータとして独立検証し、snapshot の再 parse で宣言を作りません。

続く binding map は `(u32 version = 1, resource vector, import vector, export vector)`。resource 行は `(nominal definition ID, ResourceTypeKey)`、import 行は `(source import ID, InterfaceKey, member string)`、export 行は `(source function ID, InterfaceKey, member string)`。未使用を含め全項目と順序を保持します。I06 の完全 coverage・resource/member/source の一意対応・ABI/effect/signature/nominal identity・public export・direct loan 検証を再適用します。空/省略 map は component code を許可しません。provider 選択/lock と live grant plan は明示的 host input のままで、map 項目ではありません。

### 独立 proof と実行

decode 先は生 MIR と宣言データであり、elaboration 済み実行 plan ではありません。単一の累積 budget で既存 task verifier を通し、型・宣言/ABI/effect・place/CFG・初期化/move・Send/Share・task scope/claim・suspension loan・channel/service authority・lexical/failure cleanup・region を再構築します。Component は完全 document と binding map を未使用 metadata まで独立検証してから受動 unit を返します。途中の decode table を実行 authority として公開しません。

生 cleanup step と binding/scope 宣言は検査対象の obligation であり、proof ではありません。導出 cleanup 順・resource-table generation・loan 登録・scheduler state・suspension frame・native address・allocator callback・control・call holder・保持 code owner・trusted flag を直列化しません。各 invocation は既存 engine と host authority で新しい runtime state を作ります。評価順・逆公開順 cleanup・最初の failure/suppressed error・poison・取消は既存の受理済み意味論を維持します。定数実行や unsafe の権限は追加しません。

### 上限と決定的 encoding

envelope 全体・code・IDL snapshot・metadata・map は同じ artifact byte 上限を共有し、既定は **64 MiB**です。count・乗算・slice・allocation の前に算術検査を行い、不可能な長さは reserve 前に拒否します。既存 MIR/task/IDL/binding の depth・element・table・work・requested-allocation 上限を全て必須とします。phase が重なる箇所は設定済み共通上限の最小を使い、使用量を引き継ぎ、decode・検証・matching・sizing・write で counter を reset/refund しません。反復参照・string・未使用項目・origin 検証にも work を課します。host runtime quota は別の新規 invocation 上限で、artifact から値を受け入れません。

writer は先に検証し、同じ budget で size を求め、完全で有効な artifact だけを公開します。table/vector 順と正確な snapshot を保ち、暗黙の sort・重複排除・番号振り直しをしません。受入済み schema 5 を十分な上限で再 encode すると同じ byte になります。通常 source compile は task/channel/service または component metadata に schema 5 を選び、それ以外の Copy/ownership/C は schema 1/2/3/4 と正確な既存 byte を維持します。専用 schema 5 library API は空/同期 task-profile table も encode でき、新 opcode の有無で妥当性は決まりません。

## 実行モード

VM と生成 native は同じ再検証済み task MIR を使います。optional JIT は既存 backend と availability 規則を使い、fallback evaluator を追加しません。decode MIR からの native compile は installed AOT package 読み込みとは別です。component は既存の明示的な検証済み Program/binding/instance 境界を必要とし、nested 同期 callback と root task call は RFC 0027 の control/制約を維持します。両 host の artifact 同等性は測定対象であり、この決定だけで Windows 実行や installed linking を立証しません。

## ABI と互換性

schema 1–4 の reader/writer と golden vector は task/channel/service/component feature の明示拒否を含め不変です。旧 consumer は version 5 を拒否し、旧 version や source として再試行しません。version 5 decoder は不明 flags/profile/tag と不正な未使用 metadata を副作用前に拒否します。native private packet・native package metadata・LCMP3・LCVL1・LCSG1・component C ABI v1 は変更しません。移行は対応 compiler による source からの再 build とし、version field の編集は不正です。downgrade や version 混在の解釈は受理しません。

## 診断と tooling

構造化 byte offset・profile/table/tag・verifier source span を保持し、不明 wire tag と形式は正しいが意味的に不正なデータを区別します。code/IDL origin と binding index を mismatch report に保持します。未対応 runtime/entry/grant 要求は副作用前に失敗します。既存 parser/formatter/LSP/DAP protocol は不変で、decode source path は表示専用です。実装時に CLI/help と最終 M3 guide へ schema 選択と上限を記載します。

## 性能とコストモデル

schema 5 は profile 1 byte、関数ごとの effect 1 byte、async の Span、明示 task/component metadata を追加します。元 IDL span は正規化 LCMP より空間を使いますが、metadata 再 parse や曖昧な origin 復元を避けます。decode は有限の生 table を割り当て既存 proof を再構築し、native compile は別の有限処理です。task runtime コスト/quota と provider 保持/破棄の動作は変えません。size・throughput・latency 改善は主張せず、Z01 で測定します。

## Security と信頼境界

全 artifact byte を信頼しません。公開・binding・provider 発見・呼び出しの前に完全 metadata・alias・span・effect・authority claim・合計 quota を検証します。有効な resource key・component ABI byte・source path・capability 名は resource・filesystem・network・process・allocator・native code authority を与えません。host が選ぶ provider/code owner と元 control は artifact の外です。不正/未対応データで native image constructor や source loading を起動しません。

## 代案

- **変更なし:** source 専用 task を維持するが、受理済み artifact gate を満たさない。
- **source を包む library/tool:** guest source を再 parse し、MIR の独立直列化境界を失うため不採用。
- **新しい統一 IR/package container:** 最も強い統合案だが、不要に旧 byte を変え、frontend 意味論を installation/SDK policy に結び付ける。
- **component metadata を省く task 専用 schema:** 最初の実装は簡単だが、完全な受理 format ではない。I11 は段階実装でき、I07 が全 metadata 要件を維持する。
- **正規化 LCMP と origin 復元、または IDL JSON 埋込み:** wire/parser code を再利用できるが、元 span/type-node identity を失うか metadata 再 parse を要する。明示 document table はその曖昧さなく既存の検証モデルを使い、LCMP は標準 interoperation format のまま。

## 実装計画

1. **I10（2 pt）:** 完全対訳決定を受理し、旧 profile 拒否を調べ、後述の envelope vector を独立検証し repository consistency を実行する。新 format 実行は主張しない。
2. **I11（3 pt）:** 明示 schema 5 分岐と累積 task proof で共通 MIR codec を拡張し、source 削除後の `tasks.lang` を保存 MIR から VM/生成 native で実行する。全旧 vector/拒否を維持し、I07 までは未対応 component loading を明示拒否する。
3. **I07（3 pt）:** 完全 channel/service/component document/map・CLI/embedding loading・rich 所有差分 fixture・不正/未使用 metadata・旧 vector を統合する。実装前に具体的残件を再監査し、新前提があれば先に公開する。
4. I08/P/E/L/B/X/Z は別途継続する。足場作りで source 実行 fixture・全 baseline・両 host 証拠・最終 guide を置き換えない。

## 検証計画

独立した **空 Task library の framing vector** は46 byte です。header hex `4c414e4742430000050000001e000000` の後に **0 byte を30個**続けます（profile 0・空 table count 7つ・entry 不在）。規範 encoder/decoder fixture であり、現 loader の対応証拠ではありません。byte 16を2にすると不明 profile、byte 10を1にすると未対応 flags、payload 長を29にすると長さ error です。不明 version 6 は payload 解釈前に失敗します。実際の意味検証/round-trip は I11 の証拠とします。

独立 reference packer、旧 schema 1/2/3/4 golden、全新 operation/constructor tag、全切断位置、不明 tag、overflow/count/UTF-8/span 破損、累積上限ちょうど/1未満を検証します。偽造 async effect・task scope/claim・saved return・prepared channel/service authority・Send/Share・suspension を越す live loan・component identity は writer とは独立に拒否します。未使用 metadata と snapshot 差し替えも含めます。

task・owned transfer・統合 channel/service・component resource/loan/import/rich result・失敗/取消・元 cleanup の source→artifact→decode→VM/生成 native fixture を実行します。guest source の削除/変更で再 parse しないことを示し、provider counter で不正入力に副作用がないことを示します。適切な artifact fuzz target/corpus を追加し、repository 全 baseline を実行します。両 host runtime/package 証拠は追跡 gate で必須であり、target compile や fuzz compile/lint から実行/campaign を推論しません。

## 未解決事項

未解決の意味論 blocker はありません。I11 の focused test で空 library table と累積 task-codec proof を確認しました。残る未検証の実装仮説は、schema 2–4 との codec traversal 共有が引き続き保守可能であること、既存累積 verifier API が全 IDL/map 準備コストを reset せず引き継げることです。I07 で検証し、field/tag/proof 要件/互換性を変える前に本決定を改訂します。性能と両 host 同等性は測定事項であり、仮定を証拠扱いしません。

## 出典

- Project decision: [RFC 0001](0001-project-charter.md) 最終更新2026-10-17、[RFC 0011](0011-ownership-artifacts.md) 2026-10-20、[RFC 0014](0014-explicit-source-panic.md) 2026-10-20、[RFC 0017](0017-c-aware-ownership-artifacts.md) 2026-10-20、[RFC 0023](0023-task-mir-boundaries.md) 2026-10-22、[RFC 0027](0027-production-component-interface.md) 2026-10-24。repository の決定であり外部仕様ではありません。
- 実装観測: `4ba991d` の ownership codec/tag table、生 MIR task/channel/service 宣言と task verifier、component IDL model・LCMP3 codec・独立 binding checker。file に更新日の埋込みはなく、2026-10-24 JSTに調査し実行拒否を検証しました。この決定に新しい外部設計/性能の主張はなく、web 調査は不要でした。
