<!-- translation-of: ../../rfcs/0030-native-task-component-records.md -->

# RFC 0030: Native task and component records

[English](../../rfcs/0030-native-task-component-records.md)

- Status: implemented
- Authors: Codex under the owner's experimental delegation
- Created: 2026-10-27
- Last updated: 2026-10-28
- Tracking issue: M3-I13 and installed follow-up M3-I08 (complete)
- Supersedes: None; adds a separate RFC 0018 record revision
- Superseded by: None

## 現在の範囲

2026-10-28 JST照合。Revision 2の再構築とRFC 0031／0032のinstalled follow-upは実装済みです。[M3終了監査](../milestone-3-capabilities.md)でI13／I08と両hostのtask／component startupを完了しています。以下の当初受理時の仮定は日付付き履歴です。公開C v1の安定化によってnative recordやprivate SDKがstabilizedになるわけではありません。

## Summary

検査済みtask/component object record用に非公開 **LANGNAT revision 2**を受理します。独立検証済みschema-5 MIRと厳密な記録済みnative metadataから、ISA検索・コード出力・JIT構築なしでruntime planを復元します。bindingには引き続き、信頼された対応native imageと生存中ownerが必要です。

2026-10-27 JST、RFC 0001の実験期委任に基づき受理しました。`47093ef`の既存component objectは引数・panic/再利用・VM/JIT/AOTへの元resource callback・取消しを実行します。既存実行契約の再利用を裏付けますが、永続化した復元やinstalled startupの証拠ではありません。それらはI13/I08の受入条件に残します。本RFCではpackage/SDK manifest revisionを選びません。

## Motivation

I12のリンクconsumerはまだcompiler ObjectArtifact metadataを再構築し、新規出力object bytesと比較します。保存したsemantic/native recordだけでは起動できません。RFC 0018 revision 1にはtask/component kindと明示entryがなく、旧SDK startupはcopy/owned/Cだけを受け付けます。revision 1へ暗黙にkindを足すと互換性境界が弱まります。

## Goals

- producer IRを借用せず元object bytes・entry・順序付きfunction metadata・source rangeを保存する。
- schema-5の意味論を独立再検証し、code解決前に同じ有界task/component runtime nodeを復元する。
- 元resource・明示binding/grant・allocator・control・preparation・借用task権限を実行中も維持する。

## Non-goals

- source構文・scheduling・component C ABI・LANGBC schema・LCVL/LCMP・native packetの変更。
- native認証/sandbox化・自動load・recordへの実行addressやlive権限の格納。
- installed package/SDK公開・Windows実行・安定化。それらはI08/Bの作業に残す。

## Guide-level design

producerはtask/component ObjectArtifactを受動的な記録データへ切り離し、厳密なschema-5 semantic bytesと並べて保存し、元objectをhostへリンクします。consumerはbytesを独立decodeし、元の検査済みtask proofまたは明示binding済みcomponent Programを渡してplanを復元し、対応する静的リンクsymbolをbindします。既存LinkedProgram entry APIで実行します。component recordからprovider・grant・instance・resource table・scheduler・allocatorを作りません。

## Reference semantics

envelopeは8 bytes `LANGNAT\0`・little-endian u16 version・u16 flags・u32 payload lengthを維持します。versionは **2**、flagsは **0**、lengthは16-byte header後の厳密なbyte数です。RFC 0018のprimitive・順序付きvector・hash・symbol/range規則・上限を維持します。

revision-2 payloadの順序は次のとおりです。

1. Target u8: **0** `aarch64-apple-darwin`、**1** `x86_64-pc-windows-msvc`。
2. Profile u8: **0** task、**1** component。続けて元entry FunctionIdをu32で保存。
3. 完全で厳密なsemantic bytesのSHA-256、元object bytesのSHA-256（各32 bytes）。
4. Object byte count u32とbytes、function count u32と順序付きfunction record。
5. 各functionはRFC 0018のFunctionId u32・長さ付きUTF-8 symbol・code-size u32・range count u32・range `(start u32, end u32, source u32, source-start u32, source-end u32)`を維持。

未知version/flags/target/profile・長さ・末尾bytes・digest・symbol・非整列/重複function ID・不正code/source rangeを既存の有限record上限で拒否します。framing decoderは受動データを返し、entry番号やmetadataを意味論のproofと扱いません。

runtime復元には厳密なhost target・一致するtask/component kind・有効な明示entry・独立検査済みの元semantic programが必要です。task entryは既存の引数なしi64契約を保ちます。component entryは渡された検査済みProgramのexportに限ります。共通の到達可能function・signature/type/import対応・layout・frame・native node準備を再利用します。resolver呼出しやguest/native効果より先に、期待される完全function集合・symbol namespace/entry identity・有界な記録済みsource rangeを検証します。Rust memory layoutをrecordへ保存しません。

通常の`lang_tasks_r1`とcomponentの`lang_component_tasks_r1` symbol namespace、その厳密なbuild/source/entry identity導出を維持します。記録pointerは空から始め、成功したunsafe bindingが厳密なtrusted imageから全必須symbolを解決し、実行/結果の間ownerを保持します。hashはデータの組合せを検証し、machine codeの意味論を認証しません。record検証のためにcodeを再生成しません。

所有権・破棄・取消し・失敗優先順位・source spanは既存共通runnerの動作を維持します。fresh rootは既存task admissionを使い、同期callbackは実CurrentTaskを借用してscheduler/taskを増やしません。runtime planは元の検査済みProgram/proofの借用を保ち、切り離すのはcompiler objectデータだけです。framingと復元は明示された有限上限を保ち、runtime plan準備は保持metadataを含む共通累積ledgerを使って返却しません。

## Execution modes

VM/JIT実行は不変で差分比較の基準になります。AOTは生成演算を行い、同じruntime node・control・component Hostを使います。native callbackは元instance/resource/accounting/taskを保持します。本RFCはWebAssembly実行modeを追加しません。

## ABI and compatibility

revision 1は既存tag 0–4とbytesを厳密に維持します。revision 2は独立した2-tag namespaceを持ち、revision-1 tag 0/1を読み替えません。旧readerはpayload解釈より先にversion 2を拒否します。新readerは全既存revision-1 vectorを受理し、旧profile変換APIはrevision-2 kindを拒否します。trusted binding前に厳密なSDK/toolchain/native packet revisionでobject/recordを再buildします。source edition・component ABI・schema 5・旧installed manifestの解釈は維持します。I08はtask/component package受理前にmanifest/SDK revisionとmigration fixtureを別途決定する必要があります。

## Diagnostics and tooling

byte offset付きの構造化framing errorを維持します。復元はcode解決前にprofile/entry/function/symbol/source metadataまたは元の有界preparation失敗を返します。runtime診断のsource ID/spanを維持します。parser・formatter・LSP・DAPの意味論は変更しません。

## Performance and cost model

保存native bytes/metadataと復元nodeは有界host storageを使います。startupでobjectをcompileしたり実行memoryを確保せず、共通準備を再利用します。有限graph/metadata/hash workと要求storageを計量し、usageを公開します。計測なしにstartup高速化・memory削減・binary縮小を主張しません。

## Security and trust boundaries

semantic/native bytesとmetadataは非信頼データです。symbol解決・constructor・provider呼出し・実行より先に検証します。明示unsafe bindingには引き続き厳密な実行pointer・ABI/toolchainの組合せ・native code owner保持が必要です。一致するhashや検査済みMIRは任意native codeを安全にしません。capability・provider address・resource claim・native pointer・live taskを保存しません。

## Alternatives

- 変更なしでは実行prototypeを保てますが、永続化/installedの受入条件を満たせません。
- startup時に再compileするlibrary/toolではcompiler依存が残り、コード生成なしの成果を満たしません。
- 別の完全portable実行runtimeは既存の検査済みMIR/native node契約を重複し、別backend設計に属します。
- revision 1へ2 tagを足す方が小さいものの、旧profile解釈が曖昧になります。別revisionは明示的で低コストです。

## Implementation plan

実装checkpoint（2026-10-27 01:59 JST検証、macOS AArch64）: revision-2 framing・切り離したrecord・共通のコード生成なしnode復元/binding・保存task/owned buffer/component/resource callbackの実行・不正metadata/厳密予算拒否・取消し/cleanup/owner寿命を実装しました。入力固定全baselineは149 suites / 2,298成功 / 失敗0 / 既存timing ignore 1件で、fmt/Clippy/docと全16 fuzz target compile/lintも成功しました。fuzz campaignとinstalled/Windows startupは検証していません。完全な証拠とhashを[実装Todo](../milestone-3.md)へ記録します。

1. 独立vector・不正値/上限拒否・未変更revision-1互換性とともにrevision-2 framingを追加する。
2. task/component metadataを切り離し、検査済みnative node準備をemitterと共有してコード生成なしで復元/bindする。
3. producer入力の解放後、保存taskと元resource/component callback fixtureを別リンクconsumerで実行する。実行証拠を得てからI13を完了し、installed SDK/両host startupをI08へ残す。

## Test plan

両profile/target記述の独立revision-2 vectorを固定し、revision-1 vectorを維持します。truncation・未知field/tag・digest/profile/entry/function/symbol/range不一致・有限上限の厳密値/1不足をresolver/効果前に検証します。VM/native差分と別リンクtask/component fixtureはowning storage・native resource/callback・取消し・失敗cleanup・code owner寿命・再利用を扱います。既存manifest targetに加えて有界native record decode/round-tripのfuzz coverageを追加し、compile/lintをcampaignと扱いません。実host coverageと全repository検証を、両host受入条件と別に記録します。

## Open questions

Revision 2に未解決のblockerはありません。Installed package／SDK revisionと明示的host bindingのstartupはRFC 0031／0032で決定済みで、[M3終了監査](../milestone-3-capabilities.md)に両hostのinstalled実行を記録しています。Native recordとprivate SDKは実験段階のままです。より広い性能主張には新たな計測が必要です。

## Sources

- [RFC 0018](0018-native-compiler-records.md)、2026-10-20更新。既存record framing/上限とtrusted binding。
- [RFC 0020](0020-native-sdk-and-startup.md)、2026-10-20更新。厳密なSDK/toolchainとinstalled startup境界。
- [RFC 0027](0027-production-component-interface.md)、2026-10-27更新。元component/task権限とI12 object決定。
- [RFC 0028](0028-task-component-artifacts.md)、2026-10-24更新。独立検査済みschema-5意味論。
- `47093ef`、2026-10-27 00:09 JST実行/検証: 148 suites / 2,294成功。codeに公開更新日記載はありません。component objectの証拠で、revision-2/installedの証拠ではありません。
