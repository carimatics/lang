<!-- translation-of: ../../rfcs/0010-owned-repl-previews.md; canonical-language: en -->

# RFC 0010: 所有値を返すREPL結果のpreview

[English](../../rfcs/0010-owned-repl-previews.md)

- Status: implemented
- Authors: Project contributors
- Created: 2026-10-18
- Last updated: 2026-10-20
- Implemented: 2026-10-20
- Tracking issue: [実装Todo](../milestone-2.md) のM2-V14／V16／V15／V10／V17
- Supersedes: なし。RFC 0006のREPLをRFC 0007のownership profileへ拡張
- Superseded by:

## 現在の実装状態

2026-10-20 JSTに[M2完了監査](../milestone-2-capabilities.md)と照合しました。所定のVM REPL interface、wrapper推論、上限付きpreview、retirementは実装済みです。RFC 0016でV17の間接service preview follow-upも完了しました。Native REPL実行は明示的非目標のままです。tests/repl.rs、crates/runtime/src/layout/storage/preview.rs、M2ガイドを参照してください。 implementedであり、stabilizedではありません。

以下の動機、当初の段階的計画、日付付き部分実装checkpointは、判断とrolloutの履歴です。当時の未完了ゲートは上記根拠で完了しており、規範的契約と明示的非目標は引き続き有効です。この状態照合でsource、ABI、形式の意味論を変更しません。

## 概要

TransactionalなREPL sessionで解析済みownership定義を受け入れ、ownership式を評価します。非公開の生成式wrapperだけで具体的resultを推論します。対応済み返却値を上限付き・canonical・読み取り専用のpreviewとして示し、出力を公開する前に全guest resultをretireし、mutableなsession valueは保持しません。Tooling出力の指定であり、新しいsource literalやserializationではありません。

## 動機

`5c32e25` ではBytesを含む妥当な `fn owned() -> i64` がREPL item commit前に拒否され、呼び出しがunknown nameになり、後続の `1 + 2` は `3` を表示します。`frontend::finish` の式推論はCopy checkerへ進み、`repl::Session` はcommit前にCopy実行profileを要求し、owned VM entryはi64 main専用です。RFC 0006にはCopy表示がありますが、RFC 0007／0008にはowning result previewと表示後retirementの規定がありません。Host libraryではchecker境界、staged returnの保護、source非依存のcanonical出力を修復できません。

2026-10-18 JST確認の実装上の観察であり、性能計測ではありません。

## 目標

- Transactionalな定義、元sourceの診断、新しいVM counter、実行の分離を維持する。
- Record／enum／Option／Result内のBytes／Textを含め、現在のprofileで対応済みの全resultを表示する。
- 通常のmove／loan／region規則、result escrow、allocation authority、cleanup優先順位を維持する。
- Guest callback、guest clone、raw address、持続するguest ownerなしにrenderingを制限する。

## 対象外

Top-level変数、user定義display method、一般の関数result推論、text／byte literal構文、object reflection、新しいfile／host authority、native REPL、owning wire encoding、stable ABIは対象外です。将来のVec／Map／shared／weak／arena preview形式は定めず、明示follow-upで定義するまでgateを維持します。それらのsource命令とmilestone成果は引き続き必須です。

## 利用者向け設計

次の各行は別submissionです。完全な関数は複数行でも入力できます。

```text
> fn fresh() -> core::Bytes { return core::result_unwrap::<core::Bytes, core::AllocError>(core::bytes_new(core::allocator())); }
> fresh()
core::Bytes []
> core::bytes_new(core::allocator())
core::Result::Ok(core::Bytes [])
> 1 + 2
3
```

CLIはowned scriptと同様にownership評価へallocator authorityを明示供給します。Embedding hostはownership対応submission APIへallocatorを明示供給し、従来APIにはauthorityを付けません。定義には解析が必要ですがruntime allocatorは不要です。Authority不足はguest effect前に失敗します。到達可能な未対応命令／resultはeffect前に拒否し、未使用の未対応関数を定義しても実行しません。

Commitするitemはsource定義だけです。Resultは表示失敗時もsubmission内でrender・破棄し、後続submissionから以前のheap valueは参照できません。`:reset` は定義を消し、エラー時は以前の定義を維持します。Cleanupが成功した通常panicの後も新たな評価へ進め、poisonした実行instanceは再利用しません。

## 規範的意味論

### 構文・推論・受け入れ

従来の1 itemまたは1式のsubmission grammarを維持します。公開／user関数のannotationは引き続き明示します。非公開の生成wrapperだけで、期待resultなしに単一return式を推論し、通常の具体的result、Copy／storage、move、loan、region、MIR検証を適用します。既存wrapper制約を確認し、user定義から呼び出せないようにします。文脈なしの `core::Option::None` のような曖昧なgeneric constructorのresult型を推測せず、通常の推論診断で具体的contextを要求します。Ordinary／provisional loanやescapeするarena valueにも、通常の関数result制約の例外を設けません。

Itemは共有frontend解析全体と、必要なMIR検証後にcommitし、現在のbackendで実行可能である必要はありません。式はuserの `main` のrenameやsignatureの無検証利用ではなく、検査済みfunction identityで評価します。Session内のuser `main` は通常のlibrary関数です。Script entryは引数なしi64 mainのままです。

### Canonical preview

RFC 0006のscalar／nominal形式を維持します。I64は符号付き10進、Boolは `true`／`false`、record／caseはqualified name、record fieldは宣言順、区切りはcomma-spaceです。空recordは `Name {}`、非空recordは `Name { field: value }`、payloadありcaseは `Name::Case(value, ...)`、payloadなしcaseでは括弧を付けません。

同じ反復的traversalへ次を追加します。

| Value | 形式 |
| --- | --- |
| Unit | `core::Unit {}` |
| Option／Result | `core::Option::None`、`core::Option::Some(value)`、`core::Result::Ok(value)`、`core::Result::Err(value)` |
| 有限core error | 受理済みqualified constructor nameと、存在する場合は再帰的に表示したpayload |
| Bytes | `core::Bytes []` または `core::Bytes [0, 255, ...]`。Logical順に各byteを10進表示 |
| Text | `core::Text "..."`。Valid UTF-8のscalar列全体をnormalizationせず保持 |

Text previewの内容はASCIIです。Printable ASCIIのU+0020〜U+007Eは、quote／backslashを除いてそのまま出力し、その2つは `\"`／`\\` とします。LF／CR／TABは `\n`／`\r`／`\t`、残りのscalarは小文字hex・leading zeroなしの `\u{h}` とし、NULも `\u{0}` とします。外側のquoteは実際のquote文字です。Preview escapeであり、source string構文ではありません。Core名をqualifiedにしても推論したtype argumentは表示せず、previewをsourceとしてround-trip可能にしません。Allocator identity、address、capacity、hash順序、private metadataは表示しません。

Previewできるのは対応済みprofileだけです。検査済みmanaged storageとservice tokenから読み、不正tag、invalid UTF-8、不正tokenは文字列ではなくstructured runtime faultにします。Formatterからguest codeの呼び出し、意味論上のclone／retain、raw foreign addressのdereferenceは行いません。

### Ownership・allocation・失敗の優先順位

1. 新しいVM counter群とcleanup instanceで評価します。全guest storageに明示供給されたinstance allocatorを使います。Hostが選んだcumulative quotaは複数submissionにまたがってよく、暗黙にresetしません。CLIはownership式ごとに新allocatorを供給し、hostは承認済みallocatorと累積課金を意図的に再利用できます。
2. Returnをstageし、RFC 0008に従って通常のcallee／entry cleanupを完了します。Execution／cleanupが失敗したらrenderもvalue公開もせず、元のprimary、順序付きsecondary、panic code、poison、source stackを維持します。
3. Preview作成中は成功したowning resultを保護したmanaged storageに保持します。Rendering内にguest safe pointやhost／user callbackはなく、追加のguest instruction fuelは消費せず、部分的なtextも公開しません。
4. Render成功時はresultを通常の消費／ScopeExitとして1回retireし、全admission済みguest frame／result storageを解放します。Cleanupがすべて成功した場合だけ完成host stringを公開します。Result cleanup失敗時はstringを破棄し、先行failureがなければprimaryにしてinstanceをpoisonします。
5. Renderの上限／allocation／validation失敗時は、そのfailureをprimaryに維持し、未公開resultを破棄して独立cleanupを続けます。Cleanup失敗は順序付きsecondaryとし、RFC 0008どおりpoisonします。Teardownで消費済みobligationを再試行せず、host診断の伝送でも優先順位を維持します。

新instanceでguest poisonを分離しますが、host effectはrollbackせず、host所有allocator／providerのlifetimeもhostの責任です。Abort、壊れたforeign state、不正provider動作はRFC 0008のtrust boundaryを維持します。Concurrency、atomic publication、unsafe、constant評価、schedulingの拡張はありません。

## 実行モード

初期REPLは引き続きVMと独立検証済みtyped MIRを使います。Runtime preview traversalとresult消費規則は、frontend／backend固有source型を導入せず将来のnative adapterでも使えるものにします。AOT／JIT／component callは従来contractを維持し、native REPL、C export、host binding、component resource、ownership decoder tagを認可しません。未対応の実行／result kindは実装までeffect前のgateを維持します。純粋なCopy sessionのhost-call動作は従来どおりで、ownership混在sessionはowned script実行と同じ到達命令gateを使います。

## ABIと互換性

既存の妥当なCopy submission／出力、item transaction、reset、file authority、非interactive時のprompt動作は維持します。以前Copy境界で拒否したownership itemがcommit可能になり、embedding hostはownership式実行のallocator authorityへ明示opt-inする必要があります。従来APIを暗黙認可せず、ownership対応submission APIを文書化します。新canonical出力はtooling contractで、後に変更する前には明示RFC改訂／follow-upを必要とします。Source構文やversioned artifactではなく、edition migration、package byte、schema-1 golden byte、C layout、stable native ABIは変更しません。

## 診断とtooling

不変の元submission／module snapshotを保持し、diagnostic、fix、runtime fault、各call frameをそこへmappingします。生成wrapper名／行は表示せず、見えるnameは `<expression>` とします。Failure reportは後続submission／reset後も有効です。Display-limit診断でitemをcommitしたりresultを永続化したりcleanup失敗を隠したり部分prefixを表示したりしません。Source parser／formatter／LSP spellingは変更せず、source result annotationとmove／loan／region診断をcanonicalとします。一般DAP／linter作業は実装しません。

## 性能とコストモデル

表示上限の既定値65,536 byteを維持します。Guest entry前に上限付きhost出力／traversal storageをguest payload quotaとは別に課金・予約し、予約失敗はeffect前に拒否します。Renderingは反復処理と上限付きdepth stackを使い、size 0や深いCopy aggregateも扱います。Name／contentのcopy前に出力budgetを確認し、Bytes／Textの長さによる下限を使って、はるかに大きなbuffer全体をscanせず上限超過を拒否できるようにします。Render専用guest storageは確保せず、host stringは表示dataをcopyしますがguest tokenをretainしません。Workは検証済みtype／layout準備、上限付き出力、必須cleanupで制限します。要求するalgorithmic boundであり、計測済み速度やmemoryの主張ではありません。

## Securityとtrust boundary

Previewでfilesystem／network／process／environment／FFIやhost-call authorityを付与しません。検証済みtype identity、location、tag、span、buffer provenanceを使い、pointer風の表示markerをauthorityとして扱いません。Guest overloadに依存しない固定formatを使い、entry／出力に上限を適用し、表示失敗時もcleanupを維持します。明示host effectやquarantine／teardownはstructured failure reportへ保持し、unsafeなraw valueは公開しません。

## 代替案

- **変更しない：** ownership REPL境界が使えないままで、V10は未完了です。
- **Library／helper関数：** 利用者は長さをi64で返せますが、owning result推論、保護したretirement、canonical aggregate出力はlibraryだけでは実装できません。
- **持続valueとuser display trait：** 対話機能は増えますが、source lifetime、dispatch、callback、cycle、authority、reset意味論がM2のtextだけのsession contractを超えます。
- **Opaque summaryかi64専用評価：** 単純ですが、対応済み返却値の内容を隠し、V10のresult表示を満たしません。
- **読みやすいraw Unicodeとsourceへround-trip可能なstring：** 将来の利便性には有用ですが、別の出力／literal contractです。ここでは厳密で決定的なASCII preview escapeを選び、source literalは別判断に残します。

## 実装計画

1. **V14：** RFC 0001の実験的委任に従って本明示follow-upをreview・判断し、両言語文書とboardを更新する。
2. **V16：** 上限付きmanaged traversal、namedな引数なしresult実行、保護preview／消費、実際のowned-result source fixtureを追加する。この実行成果をcompiler基盤の追加より先に完了する。
3. **V15：** 通常のHIR／MIR検証を通し、生成wrapperだけのresult推論を広げ、通常関数annotationと正確なsourceを維持する。
4. **V10：** Item受け入れ、明示allocator authority、runtime dispatch、failure mapping保持、CLI出力、submission分離を接続し、既存testと両言語docsを更新する。
5. **V17：** 実indirect-service adapterの後、明示follow-upを判断して上限付きresult previewを実装し、VM／REPL根拠を示す。Mapの非iterationとshared／weak／arena authority規則を維持し、M2をBytes／Textへ暗黙に縮めず残りresult familyをこの行に保持する。

後続service adapterの従来milestone taskは維持し、それらのpreview形式を有効にする前には明示follow-upを必要とします。一般native／debug／conformance／measurementのexit evidenceも必須です。Acceptanceだけではimplementation gateは完了しません。

## テスト計画

実際のsource由来実行とmanaged bufferを使い、empty／NUL／全byteのBytes、ASCII／quote／backslash／control／multibyte／scalar極値のText、canonical等価でもbyteが異なるstring、core error、入れ子nominal／sumを確認します。Scalar／aggregateのCopy出力は既存fixtureと比較します。厳密値／1不足／0の出力上限、深い／size 0のtraversal、不正storage／token、entry前host allocation上限、guest OOM／fuel／interruption、result cleanup失敗、順序付き／truncated report、1回限りの解放、部分出力なしを検証します。Session／CLIでは妥当／不正ownership定義、式推論、通常annotation／loan／region拒否、allocator不足、source authority、reset後も有効なsnapshot／stack、fresh VM counter、後続復帰を検証し、純Copyのhost-effect testも維持します。実process OOM、fuzz campaign、benchmark、native platformの結果は主張せず、実装変更時には適切な既存compiler／fuzz compile checkを実行します。

## 未解決事項

現在のpayload profileに意味論上のblockerはありません。性能主張前にはformatter準備costと混在sessionのcompileをprototype／計測します。将来のindirect service形式とnative preview統合には後続の明示設計／evidenceを必要とし、従来milestone成果は緩和しません。

## 判断と実装状態

Review結果：ownerのRFC 0001の委任に従い、**2026-10-18 JSTにexperimental delegationでaccepted** とします。User関数の推論、source literal、loan escape、execution authorityを変更せず、具体的なREPL境界の不足を埋めます。保護resultの消費とrender failureの優先順位はRFC 0008と整合し、対応profileの境界とV17によってM2の残りresult familyも維持します。このprofileに意味論上のblockerはありません。

根拠：当初のREPL境界実験では妥当なowning定義が拒否されました。V16は [named-result library](../../../examples/analysis/repl_preview.lang) を `lang_vm::unit::owned::execute_preview` で実行し、Bytes／Text／入れ子resultをstring公開前に解放します。V15は通常HIRと独立MIR検証で妥当なwrapper resultを推論します。V10でownership item／式をsessionとCLIへ接続し、明示allocator authority、fresh instance、対応付けたfailure、sourceだけの永続化を実装しました。REPL 24 testとreport-adapter unit testで実内容、transaction、上限、OOM／interruption、quota再利用、厳密解放、source／reset lifetime、Copy host effect、failure優先順位transportを検証します。[CLI transcript](../../../examples/repl/owned.repl) は成功します。[Preview](../architecture-history.md#owned-result-previews)、[推論](../architecture-history.md#ownership-expression-inference)、[sessionの根拠](../architecture-history.md#ownership-repl-sessions)を参照してください。

過去のcheckpoint、2026-10-18 JST（現在の状態は冒頭で更新済み）：当時の実装は **一部完了** でした。V16／V15／V10はcurrent-profile preview、推論、session／CLI経路を実装します。後続indirect-service形式／実装はV17の明示follow-upへ残し、元のM2完了要件は維持します。準備costと混在sessionのcompile costは未計測で、indirect形式／native previewも未検証です。受理済みcontractを後に変更する場合は、実装前に明示revision／follow-upを行い、両言語を同時更新します。

## 情報源

- [RFC 0001](0001-project-charter.md)：実験的decision delegation。2026-10-17最終更新、2026-10-18 JST確認。
- [RFC 0006](0006-source-frontend-and-portable-programs.md)：transactional Copy REPL、canonical value、counter。2026-10-17最終更新、2026-10-18 JST確認。
- [RFC 0007](0007-source-ownership-and-core-values.md)：owned result、厳密byte／UTF-8、通常のloan／storage／region規則、明示allocator authority。2026-10-18 JST最終更新。
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md)：保護return、優先順位、上限report、poison。2026-10-17最終更新、2026-10-18 JST確認。
- [REPL](../../../src/repl.rs)、[frontend wrapper dispatch](../../../src/frontend.rs)、[HIR推論境界](../../../crates/hir/src/program.rs)、[owned entry](../../../crates/vm/src/unit/owned.rs)、[REPL test](../../../tests/repl.rs)：当初の境界根拠を `5c32e25` とpipeした実験で2026-10-18 JST確認。その後のV16／V15実装根拠は以下に記録します。
- [Managed preview](../../../crates/runtime/src/layout/storage/preview.rs)、[VM adapter](../../../crates/vm/src/unit/owned/calls.rs)、[source test](../../../tests/support/owned_preview.rs)、[storage test](../../../crates/runtime/tests/support/preview.rs)、[cleanup fault test](../../../crates/vm/src/unit/owned/failure_tests.rs)：V16実装。2026-10-18 JST更新・確認。
- [HIR推論](../../../crates/hir/src/program.rs)、[body検証](../../../crates/hir/src/program/bodies.rs)、[semantic解析](../../../crates/semantics/src/analysis.rs)、[推論test](../../../tests/support/owned_inference.rs)：V15実装。2026-10-18 JST更新・確認。
- [Session](../../../src/repl.rs)、[owned adapter／report transport](../../../src/repl/owned.rs)、[CLI](../../../src/main.rs)、[REPL test](../../../tests/repl.rs)、[ownership test](../../../tests/support/repl_owned.rs)、[transcript](../../../examples/repl/owned.repl)：V10実装。2026-10-18 JST更新・確認。
- [実装Todo](../milestone-2.md)：scope／依存／暫定見積もり。2026-10-18 JST更新・確認。本RFCはprojectのtooling判断であり、外部言語や性能の主張は行いません。
