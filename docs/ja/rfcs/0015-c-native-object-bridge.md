<!-- translation-of: ../../rfcs/0015-c-native-object-bridge.md; canonical-language: en -->

# RFC 0015: Private C native object bridge

[English](../../rfcs/0015-c-native-object-bridge.md)

- ステータス: implemented
- 著者: Codex、ownerの実験段階の委任に基づく
- 作成日: 2026-10-20
- 最終更新日: 2026-10-20
- Implemented: 2026-10-20
- Tracking issue: M2-F05–F07、M2-P02
- 置き換えるRFC: なし。明示C対応objectについてRFC 0012を拡張
- 置き換えたRFC: なし

## 現在の実装状態

2026-10-20 JSTに[M2完了監査](../milestone-2-capabilities.md)と照合しました。元objectのbridge、imageに結び付いたslot、linked import thunk、typed export runtimeは実装済みで、F05/F06/F07/P02と両hostのC/SDK根拠も完了しています（M2 G12/G14/G15）。Matching-imageのlifetimeとowner-thread責務は変更しません。 implementedであり、stabilizedではありません。

以下の動機、当初の段階的計画、日付付き部分実装checkpointは、判断とrolloutの履歴です。当時の未完了ゲートは上記根拠で完了しており、規範的契約と明示的非目標は引き続き有効です。この状態照合でsource、ABI、形式の意味論を変更しません。

## 概要

Profile 1 support／export入口、ownership継続、正確な型付きimport thunkを持つ明示C対応native objectを生成します。保持したlinked imageを、JITと同じ検証済みProgram／Context／runtimeへ結び付けます。公開入口はversion付きprivateなimageごとのslotでruntimeを得て、生成byteへhost addressを埋め込みません。

判断：**実験段階の委任に基づきaccepted、2026-10-20 JST**。上限付きinterfaceで受理済みsemanticsを保持し、実行時JITを使わないAOT実行が可能です。対応imageのbindingは信頼されたunsafe境界です。受理は実装完了・platform conformance・性能の証明ではありません。

## 動機

61af46dでは実CからNativeFactory越しにrecord／owner／callback／panic exportを実行できます。通常object APIはC unit全体を拒否します。ContextSupport／NativeExportsはprocess addressを捕捉し、NativeInvokerはJIT署名thunkを構築します。そのままobjectへ移すとaddress依存か実行時JITが残ります。

既存のin-memory ObjectArtifactは生成code・対応compiler metadata・保持した信頼imageを組にします。通常APIでC unitを黙って受け入れる代わりに、その明示境界を拡張します。

## 目標

- 実object machine codeと既存ownership gatewayでlinked C export／importを実行する。
- Span、limit、cleanup／first failure優先順位、allocator identity、grant、失敗時output保持を維持する。
- Private native object binding／namespaceに明示versionを与える。

## 非目標

- 新構文、source semantics、C profile 1のbyte／digest／status、安定native／plugin ABI、第2 frontend。
- 新portable MIR／bytecode／package encoding、信頼しないnative loading、async入口、threadを跨ぐcontext利用。
- この判断だけでstandalone package startup、debugger証拠、性能主張を完了すること。

## ガイドレベルの設計

明示C object emitterはbyteと対応する検証済みmetadataを返します。信頼hostがlinkし、宣言symbolをresolveし、検証済みlinked code ownerでimageを保持します。Binding guardがowner・Program・ExportRuntime factoryをobjectのprivate slotへ関連付け、Cは既存生成headerのcreate／export／query／destroyを呼びます。

すべての呼出しとcontext破棄までbinding・Program・metadata・link依存・allocator callback・callback user dataを保持します。Bind／unbindと呼出しはowner thread上の同期処理です。Image slotはactive bindingを1個だけ受け入れ、2個目は既存bindingを置換せず拒否します。Unbindにはactive callとlive opaque contextがないことが必要で、header／dispatch解放前にslotを消します。これは明示embedding APIで、汎用C loader APIではありません。

## 参照semantics

### Object bridge revision 1

Tier-1 pointerは64 bitです。`lang_c_obj1_slot_<artifact-digest>`というprivate symbolで、8 byte alignmentの書込み可能なゼロ初期化slotを1個公開します。内容はliveで不変なnative headerへのpointerです。Headerのsize／alignmentは **48/8** です。

| Offset | Field | Native表現 |
| ---: | --- | --- |
| 0 | version | u32、正確に1 |
| 4 | size | u32、正確に48 |
| 8 | data | 非nullのopaque dispatch pointer |
| 16 | create | target-C u32 fn(data, config, context_out) |
| 24 | destroy | target-C u32 fn(data, context) |
| 32 | query | target-C u32 fn(data, context, summary_out) |
| 40 | invoke | target-C u32 fn(data, function_index_u64, context, argument_words, output) |

Pointer fieldはすべて8 byteです。Toolchain-privateなnative契約で、serialized Rust layoutではありません。Unsafe binderがheader／slotの実object全体のextent・alignment・access・code型・寿命を保証します。入口はslotを読みversion／sizeとdata・選択callbackの非nullを検査します。未bind・不一致・null callbackなら、argument objectのread・provider呼出し・caller output変更なしにprofile 1 invalidを返します。

Source exportは正確なprofile 1署名を維持します。Wrapperはalignmentを満たす上限付きstack領域へ最大256個の初期化済みu64引数wordを詰めます。Boolは元のu8 bits、他のscalar／pointerはnative表現を保ちます。Unit resultにoutput pointerはありません。Callbackは元opaque context／Programを認証し、managed転送・guest実行前に既存の完全なExport gateを実行します。

Create／destroy／queryは既存の検証済みraw context supportへ委ねます。Invokeはlinked code owner・元のaccounting allocator・control／work・cleanup Instanceを使います。Managed ownerはC値境界を跨ぎません。通常guest panicは再利用可能でcleanup／host panicはpoisonし、後続cleanupでも最初のfailure／source spanを維持します。構文・名前解決・推論・評価順序・loan・arena brand・定数評価・memory modelは変更しません。

### Native bodyとimport thunk

C object body symbolは`lang_c_obj1_r<revision>_<artifact-digest>_f<function-index>`です。Panicを含むunit全体はrevision 3、それ以外はrevision 2を選びます。RFC 0012／0014のpacket・status・helper index／署名を維持します。通常objectと既存JITのnamespaceは従来名と通常APIのC unit全体拒否を維持します。

すべての宣言C importについて、`lang_c_obj1_t<artifact-function-index>_<artifact-digest>`というprivate名の正確な署名thunkをobject内へ生成します。Grantされたcode pointerから生成callback型を復元し、bool／Unit／aggregate／256引数を含みます。Linked invokerは保持addressと既存の通常padding復元／検証を使い、importゼロのunitでも実行時にJIT module／実行codeを構築しません。

Linked runtimeはJITと同じ検証済みlayout／call siteを読みます。Metadata参照だけでguest evaluatorを置き換えられません。Guest算術／制御とcall／returnはcompile済み継続で実行します。

## 実行mode

VM semanticsを比較基準に保ちます。明示JIT embeddingのinterface／symbolは維持します。C対応AOTは生成objectのbody・support／export wrapper・import thunkと対応する信頼runtimeを使い、VM実行や実行時JITへ置換しません。Wasm／component mappingは不変で延期したままです。

## ABIと互換性

影響domainは`native-abi`と`bootstrap`です。C対応objectと対応runtimeをobject bridge revision 1向けに再buildします。別revisionはsymbol名とheader検査で拒否し、layoutを推測しません。通常object APIはC unitを拒否したままで、明示C object APIを使います。C header、profile 1 descriptor／digest／status、受理済みsource挙動は不変です。

Portable schema 1／2／3のC metadata拒否を維持します。初期linked APIは対応in-memory ObjectArtifact metadataを保持し、独立native metadata wire decoderを追加しません。将来native metadata／bootstrap表現を永続化する場合は、実装前に独自versionと検証契約を記録します。Package startupはP02であり、in-memory compiler metadataをstandalone packageと呼び替えません。

## 診断とtooling

公開前にstructuredなtarget／profile／bridge／symbol／already-bound／resource errorを返します。Linked metadataとruntime failureの元function／site spanを維持します。Parser／formatter／linter／LSP／診断codeの変更は不要です。Native debug location／stepping証拠はG01–G03です。

## 性能とcost model

公開入口ごとにslot load・version／size／null検査・runtimeへの間接callが増え、export引数を上限付きword列へ詰めます。型付きthunk／wrapperでobject sizeが増えます。Host metadata準備とguest allocation／cleanupは既存の上限／計上に従います。Compile時間・実行時間・size・memoryの優位性を主張せず、target測定は未検証です。

## Securityとtrust boundary

Native image・対応image resolver・runtime adapterは信頼します。使用／公開前にtarget・artifact identity・必須symbol・body／import／export対応・header revision／size・binding状態を検査します。任意Rust memory layoutをdecodeせず、攻撃者提供function pointerをsandboxとして受け入れません。Script／plugin capability、明示native FFI grant、再入／poison、実C objectの有効性の責任は不変です。Foreign callbackは同期・no-unwind・code／user寿命の義務を維持します。

## 代替案

- **変更しない：** AOT C実行が未実装のままです。
- **生成C shim／tool library：** 署名を変換できますがversion付きbinding／runtime契約と対応code metadataが必要で、object wrapperならbackend emitterを再利用できます。
- **Process global registry（最有力競合）：** 入口ごとにartifact IDを引けますが、global検索・登録・寿命調停が増えます。上限付きimage slotで保持ownerを明示します。
- **捕捉address／実行時JIT（単純案）：** 既存embeddingの挙動を保てますが、再配置可能なAOT C経路にはなりません。

## 実装計画

1. 検証済みC metadata／layout参照を共有し実linked codeを保持します。通常APIは不変です。
2. Ownership machine bodyと同じobjectへrevision 1 slot・support／export wrapper・型付きimport thunkを生成します。
3. Linked image／runtimeをbindし、実Cのsource increment／panicからaggregate／owner／importへ同じ経路で進めます。
4. F05全範囲、direct／response-file link、malformed／unbound／lifetime／state／resource、両targetを監査します。F06／F07／P02と最終guideを維持します。

## Test計画

既存source compile-pass／fail、C値検証、schema拒否、JIT、通常object回帰fixtureを維持します。Object検査、missing／duplicate／別target binding、未bind／revision／size／null callbackでのoutput保持、実C header compile／link／run、元contextのcallback、aggregate padding／canonical case、owner再帰／panic cleanup、fuel／provider prefixを追加します。同等VM／native挙動を比較し、import addressがJITでなくlinked image由来と証明します。macOS／Windowsとsystem linkerのdirect／response-file証拠を分け、benchmark／debug gateは維持します。

## 未解決事項

Blockingなsemantics質問はありません。未検証の実装仮定は、両formatのCranelift slot relocation／symbol visibility、自己参照なしのmetadata／runtime寿命共有、linked thunkの完全な再利用、package bootstrap、実target costです。Prototypeで確認し、非互換変更の前にRFCを明示改訂します。

## 情報源

- RFC 0001の実験段階の委任、最終更新2026-10-17、2026-10-20 JST確認。
- RFC 0008のC境界契約、最終更新2026-10-19、2026-10-20 JST確認。
- RFC 0012のprivate native継続（本RFCと同時改訂）、最終更新2026-10-20、2026-10-20 JST確認。
- RFC 0014の明示panic／r3契約、最終更新2026-10-19、2026-10-20 JST確認。
- Local実装version `61af46d`、2026-10-20 JST commit：`owned.rs`の通常object拒否、`c_abi/scalar.rs`のJIT thunk、`c_abi/exports.rs`のaddress捕捉、`c_abi/runtime/factory.rs`のruntime。2026-10-20 JST確認。既存source由来object境界実験は両tier-1 targetを作用前に拒否します。
