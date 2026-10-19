<!-- translation-of: ../../rfcs/0018-native-compiler-records.md; canonical-language: en -->

# RFC 0018: Native compiler record

[English](../../rfcs/0018-native-compiler-records.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-P05, M2-P06, M2-P07, M2-P02
- Supersedes: none; extends RFCs 0011/0014/0015/0017
- Superseded by: none

## Summary

Native object byteとcompiler metadataの上限付きversion管理transportを追加し、既存portable意味論artifactのSHA-256と対応させます。独立decode済み意味論から、source分析、ISA検索、機械語生成、JIT構築なしにruntime planを復元します。

Review結果：wire実装前の**実験的委任に基づくaccepted、2026-10-20 JST**。P03の独立記録でnative planを再構築できます。Source由来のmanaged Text／Vec／shared／arena fixtureは、生成元破棄後に意味論をdecodeしてlinked objectを実行できました（重点prototype test 2.41秒）。Consumerはsource分析もbackend生成も行いません。分離の証拠であり、配布完了や性能の主張ではありません。Copy起動、永続記録、汎用runtime起動、C／SDK統合は未検証です。

実装checkpoint（2026-10-20 JST）：上限付き記録、全target／scope tag、canonical golden byte、checksum／上限検査、image保持付きCopy呼出しを実装しました。保存fileのCopy／managed ownership／明示的panicを生成元破棄後にsource分析／機械語生成なしでlinked実行できます。記録transportと準備の証拠であり、具体的owned起動、C／SDK統合、installは別途P05／P06／P07／P02で追跡します。

Runtime／C統合checkpoint（2026-10-20 JST）：RFC 0019の通常production入口と、`CObjectArtifact::into_recorded`／`RecordedObject::prepare_c`によるC interface／bridge metadataの保存・復元を実装しました。生成元破棄後の保存fileから、source分析／backend生成／JITなしで元context／allocator／grant／cleanupを保つ実C callerを実行します。両targetのscope／digest／予算、managed全family、aggregate／256引数、有限Copy全case、owner付き再帰／callback／panic、fuel／割当失敗位置を検証します。SDK組立P07と両host隔離install P02は未完了です。

## Motivation

RecordedObjectはnative byte／metadataを所有しますが、単一Rust process内に限られます。Standalone conformance driverはmetadata再構築のため同じsourceを再compileしています。導入先runtimeにはcompiler出力をdataとして渡す必要があり、再解析／再生成を残すとcompiler／startup依存が残ります。Portable schema 1／2／3／4は意味論と元spanを既に保持します。第2の意味論IRを作らず、独立native記録で同じ契約を使います。

## Goals

- 元object byte、target、private compile scope、function識別／symbol／code size／source rangeを保持。
- Plan準備前に意味論／objectの偶発的な取り違えを検出。
- Decode／encodeのwork、allocation、element、byteを制限し、unsafeなexact-code binding契約を維持。
- 後続P06に必要なC scopeを含め、Copyとownership記録に対応。

## Non-goals

- 新source意味論、opcode、公開C profile、native packet／helper ABI、component形式。
- Native認証、native codeのsandbox、safeな自動load／実行、動的plugin load、安定SDK ABI。
- CodecだけでC context／SDK配布、clean install、debugger、性能gateを完了扱いにすること。

## Guide-level design

Compilerは通常の意味論artifactとnative記録を保持します。Trusted hostは意味論artifactを独立decodeし、その正確なbyte列に対してnative記録を検査し、target layout／planを再構築し、保存objectをlinkして対応するimageを明示bindします。Copy呼出しは既存canonical cell-buffer ABI、ownershipは既存の検証済みcontinuation／runtime serviceを使います。

Native記録はdataです。Decodeはsymbol解決、library load、guest領域割当、C context生成、capability付与を行いません。Hostにはnative code／toolchainの信頼とunsafeな対応image契約の全条件が残ります。Checksumは破損／取り違え検出であり、悪意ある差し替えの認証ではありません。

## Reference semantics

Version 1はlittle-endian、すべての長さ／個数はu32で、paddingはありません。

| Field | Encoding |
| --- | --- |
| Envelope | LANGNATの後にNULのmagic（8 byte）、version u16 = 1、flags u16 = 0、payload長u32 |
| Target | u8：0 macOS AArch64、1 Windows x86-64 MSVC。他tagは拒否 |
| Kind | u8：0 Copy、1 ownership Function、2 ownership Unit、3 ownership CUnit、4 ownership CObject |
| C object identity | kind 4だけにC Artifact digestの正確な32 byte |
| Semantic identity | portable意味論artifact全体のSHA-256、32 byte |
| Object identity | 正確なobject byte列のSHA-256、32 byte |
| Object | byte長u32とそのbyte列 |
| Functions | 個数u32と順序付きfunction記録 |

各function記録はfunction ID u32、symbol（UTF-8 byte長u32とbyte列）、code size u32、source range数u32、そのrange列です。各rangeはnative start／end、source ID／start／endの5個のu32。Code sizeは非ゼロ、rangeはstart < end <= code size、source start <= source end、function IDは厳密昇順です。Symbolは非空UTF-8でNULを含まず最大4,096 byte。不正個数／tag／長さ／UTF-8／range、未対応version／flags、digest不一致、切断、余剰byteはbyte offset付き構造化errorです。

意味論の独立検証は必須です。準備時に、その検証済みdomainに対してfunction数／ID、canonical symbol、source識別／UTF-8境界、target、private scopeを検査します。構造検査はnative byteが意味論を実装する証明ではなく、trusted compiler／linkerとunsafe binderにその義務が残ります。

Codec操作全体で単一のMIR型累積予算を使い、保持object／metadata buffer、decode／string／vector、writer sizing／出力pass、出力reservationを計上します。Scalar field／element、hash／copy／stringのbyte列のlen/1024 + 1 chunk（空／終端chunkを含む）を課金します。残byteと最小row size、設定上限の両方で過大listをreserve前に拒否。記録の既定入出力上限は64 MiBで、callerは縮小できます。外から与えた意味論byte列にも上限を適用します。失敗時は部分記録を公開しません。

## Execution modes

VMは既存portable artifactをdecodeし、native記録を使いません。Native起動はdecode済み意味論と対応compiler記録を使い、guest MIRをfallback評価しません。JITと既存in-memory object APIは維持します。C記録はprivate scope／digestを保持しますが、P06でC interface／bridge metadataを別途再構築・検証し、明示context／grant／bindingを得る必要があります。TagでC権限を付与しません。

## ABI and compatibility

Native記録version 1は実験的で、portable schema、C profile 1、bridge revision 1、native helper revisionとは別です。既存portable byteとprivate呼出し規約は維持します。未知versionは拒否し、移行は対応compiler／SDKで再compile／再packageし、source fallbackや暗黙downgradeは行いません。Relocation後のaddressとlive runtime objectは保存しません。Tier-1 tagがあっても別targetのobjectは現hostで実行できません。SDK配布契約の定義まで、正確なcompiler／toolchain／image互換性はcaller要件です。

## Diagnostics and tooling

構造化error kind／byte offsetを使い、resource-limit causeを保持します。意味論artifactと元native rangeにsource位置を保持。Parser、formatter、型推論、LSP、source診断の変更は不要です。Codecだけではdebugger証拠になりません。最初の永続記録linked実行と同時に対訳文書を更新します。

## Performance and cost model

Containerは32-byte digest 2個、framing、元metadataを加え、object全体を1個保持して圧縮しません。Hashとwriter 2回の走査は入力sizeに比例します。Runtimeには意味論の独立検証、layout／plan準備、上限付き割当のcostが残ります。起動経路からsource分析／backend生成の必須依存を除きますが、計測なしにlatency、size、memoryの改善を主張しません。

## Security and trust boundaries

記録dataは検証までuntrustedであり、検証後もnative code自体はtrusted codeです。Hashは署名ではありません。Decodeはobject／imageの安全性やsource意味論との一致を証明しません。Link／bindは正確なcompiler／toolchain／imageとowner寿命の不変条件を伴う明示unsafe操作です。Script／C／host callのcapability制限と既存foreign pointer検証を維持します。Native pointer、resolver、allocator、instance、grant、live ownerは保存しません。

## Alternatives

- 変更なし：導入先起動にsource分析／backend依存を残す。
- Library／toolingだけ：独立Rust記録はprocess内で有用だが配布境界を越えられない。
- 完全native package／SDK ABIを今決める：実行実験前にinstall、context policy、linker layoutとtransportが結合するため、P06／P07で段階的に実施。
- 第2のnative意味論IRやRust layout dump：共有verifierを重複するか不安定layout／pointerを露出するため拒否。
- 単純な未検査object／metadata tuple：取り違え検出と不正metadataの上限を保証できず、明示version／検査を維持。

## Implementation plan

1. Copy／ownershipの上限付きnative記録codecと、正確なportable artifact byteへの対応付け。
2. 意味論／記録をdecode後、生成元破棄済みでsource分析／backend生成なしにplanを再構築し、Copy／ownershipの実linked実行を確認。
3. P05の汎用native起動、P06のC context、P07のSDK組立、P02の両host隔離installへ統合。Codecだけでこれらを完了にしない。

## Test plan

Envelope／tag／layoutを固定し、objectと全metadataをround-tripします。既存portable golden byteを保持。未知version／flags／tag、全切断prefix、修正長、不正string／count／range、digest差し替え、余剰data、厳密なbyte／work／allocation／element上限を拒否します。Bind前に意味論domain不一致を検査。Decode済み意味論／記録で実linkedのsource由来Copy／managed ownershipを実行し、結果、span、cleanup／control／allocator動作、image所有を保持。両target形式／hostと後続C／起動／配布gateを維持します。重点検証後に全baselineを実行します。

## Open questions

記録形式のblocking questionはありません。汎用runtime入口とSDK配布には追加判断が必要な可能性があり、wire fieldや公開起動ABIの追加前にRFCを明示改訂します。

## Sources

- [C記録の復元](../../../crates/codegen-clif/src/owned/c_objects.rs)と[保存fileの実行／拒否検査](../../../tests/support/c_object_records.rs)、2026-10-20 JST実装・確認。

- [独立記録](../../../crates/codegen-clif/src/owned/recorded.rs)、実装5324c64、2026-10-20 JST。
- [Linked実行prototype](../../../tests/support/native_linked.rs)、2026-10-20 JST確認・実行。2.41秒は重点test 1件の時間。
- [Portable artifact契約](../../rfcs/0017-c-aware-ownership-artifacts.md)、2026-10-20更新。
- [Native C objectの信頼／ABI](../../rfcs/0015-c-native-object-bridge.md)、2026-10-20更新。
- [実装Todo](../milestone-2.md)、2026-10-20 JST更新。Projectの証拠／決定であり、外部性能の主張は用いません。
