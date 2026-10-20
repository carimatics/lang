<!-- translation-of: ../../rfcs/0017-c-aware-ownership-artifacts.md; canonical-language: en -->

# RFC 0017: Cに対応したownership artifact

[English](../../rfcs/0017-c-aware-ownership-artifacts.md)

- Status: implemented
- Authors: project contributors
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-P02
- Supersedes: none; RFC 0011/0014 を拡張
- Superseded by: none

## 現在の実装状態

2026-10-20 JSTに[M2 capabilityと検証根拠](../milestone-2-capabilities.md)と照合済みです。本RFCの範囲はimplementedであり、stabilizedではありません。以下の当初計画、decision時点の記述、日付付きcheckpointはrollout履歴として読みます。そこで別作業としていたnative／SDK／platformの現在状態は上記ガイドを参照してください。今回発見したRFC 0012の論理debugger検証不足は同ガイドの訂正で解消し、本RFCの非目標や規範的要求は変更しません。

## 概要

既存のHost／C importとGuest／CExport function識別を、検証済みMIRの保存・復元で保持するownership **schema 4**を追加する。Copyのschema 1、ownershipのschema 2、explicit panicのschema 3のbytesは変えない。Decodeはdataを供給するだけでFFI権限を与えない。

Review outcome: 実装前に **実験段階の委任に基づきaccepted**。既存declaration verifierが識別と有限C signatureを検証しており、不足するtransportをformatとして拡張する判断で、sourceやC呼出しの意味は増やさない。Native再準備のprototype 5324c64が配布上の必要性を示す。受理時点ではcodec実装と復元後C実行は未検証。この判断だけではP02を完了せず、native配布containerも定義しない。

実装の証拠（2026-10-20 JST）：codecはC宣言にschema 4を選び、明示ABI byteとPanic、従来golden byteを保持します。Sourceから復元したC callを既存の明示VM／native context APIで実行し、source削除後のCLI loadもC権限を与えません。Wire破損、切断、有限aggregate署名、累積上限を検証します。検証は対訳architecture文書、P02配布作業は実装Todoを参照してください。

## 動機

現writerはC import／CExportがあると公開前に拒否し、schema 2／3 readerはHost／Guestを補う。そのため、production native startupはsource解析やABI識別の消去なしにC対応の意味契約を復元できない。P02は検証済みIRとruntime／context依存を配布する必要がある。共通MIR codecを使い、言語の意味を別のIRで複製しない。

## 目標

- C対応ownership宣言全体と既存span／型／操作を保持する。
- 独立declaration／body／move／loan／region／failure検証を再実行する。
- 既存format bytes、明示binding／capability要件を保つ。

## 非目標

- Bytecode内のnative code、address、context、allocator callback、grant、C binding。
- Native package／container、SDK、code認証、動的library loader、Rust layoutの保存。
- 新しいsource構文、C ABI profile、実行policy、panic意味、guest操作。

## 利用者向け設計

検証済みimportの `ImportAbi::C` はCとして、functionの `FunctionAbi::CExport` はCExportとしてround-tripする。Ownershipのencodeは次を選ぶ。

| Ownership unit | Schema |
| --- | --- |
| 未使用宣言も含めC import／CExportがある | 4 |
| それ以外でexplicit Panic操作がある | 3 |
| それ以外 | 2 |

Copyはschema 1を維持する。復元したC対応artifactは既存の明示C binding／context実行interfaceへ渡せる。通常CLIにC bindingやnative loading権限は増えない。Compiler／native package側はsource snapshotを再解析せず、復元dataから意味計画を構築できる。

## 参照セマンティクス

名前解決、型、評価順、ownership、borrow、破棄、allocation、cancellation、concurrency、定数評価は変えない。検証済みMIRに存在する識別を保存するだけで、C signature／名前／visibilityの制約は既存の有限profile 1を維持する。

Schema 4は既存ownership envelope、little-endian scalar、table、type／place／operation tag、任意entry ID、source snapshotを再利用する。Schema 3のPanic tag／payloadを含む既存ownership操作の意味は変えない。

Schema 4だけに、必須u8を2種類追加する。

| Table内の位置 | 値 |
| --- | --- |
| Import名の直後、spanの直前にImport ABI | 0 = Host、1 = C |
| Function名／publicの直後、spanの直前にFunction ABI | 0 = Guest、1 = CExport |

既定の識別も含め全import／function行にfieldを置く。2–255は予約値として、該当byte offset付きの構造化unknown-tag errorで拒否し、Host／Guestへ補わない。Schema 2／3はABI byteを消費せず、従来どおりHost／Guestを補う。

EncodeはC識別があればsizing前に4を選び、それ以外はRFC 0011／0014の選択を維持する。Schema 4は、そのunitにPanicがあるかにかかわらず既存Panic encodingを許容する。Parse／sizing／writeは同じversionを使う。検証／sizing／allocation／encode失敗で部分artifactを公開しない。

Decodeはraw unit全体を検証してからchecked formを公開する。C bitによってowning／borrow／allocator型のC引数利用、public export／名前の要件回避、arena検査緩和、address権限付与はできない。Decode後も既存の実行admissionを適用し、未対応／未binding foreign実行をcodecで解決しない。

## 実行モード

VM／native hostは既存の明示実行／binding interfaceでだけschema 4を利用する。Sourceと復元unitは、C context同一性、元allocator、quota、fault、cleanupを含め一致させる。Native startupはP02の別実装。任意JIT、component、組み込みcallback、定数実行に暗黙権限は追加しない。

## ABI と互換性

Source、native C／component ABI、native private packet、package metadata layoutは変えない。Schema 4は新しいserialized MIR versionであり、旧consumerは2／3と誤読せず未対応として拒否する。既存schema 1／2／3のgolden bytesとdecodeを維持する。

C対応ownership出力は4へ移行し、非C出力は2／3のままとする。Host／Guestへの情報損失を伴うdowngradeはしない。C対応dataが必要なconsumerはcodecを更新する。Source移行やedition変更は不要。別のABI種別追加は別format判断を要する。

## 診断とツール

構造化codec offset／table／tag診断とverifierのsource spanを維持する。Unknown ABI tagと、検証で見つかる不正C signatureは区別する。Parser recovery、formatter、LSP、debugger protocolは変えない。Artifact文書へschema選択、互換性、grantを保存しないことを記載する。

## 性能とコストモデル

Schema 4だけimport／function各行に1 byteを追加する。Version選択は上限付き宣言tableを走査する。Read／sizing／writeと保持tableは既存の累積codec／verification budgetとbyte上限を共有する。Decodeはguest allocation、retain、dispatch、source再解析、backend compileを追加しない。Runtime性能は保証しない。

## セキュリティと信頼境界

Artifactは未信頼のまま。実行前にversion、長さ、tag、UTF-8、span、index、signature、ownership／region／failure意味全体を検証する。C識別は意図を示すだけで認可ではない。Byte fieldからnative pointer／code／allocator／capabilityを復元しない。Native code／SDKの一致と信頼は別のpackage／linker責務。上限付きの失敗と部分公開禁止を維持する。

## 代替案

- C unit拒否の継続：sourceを要しない意味transportを作れない。
- C識別をHost／Guestへ消す：signature検査やdispatchが変わるため不採用。
- Schema 2／3へ任意byteを追加：parse／golden互換性を壊すため不採用。
- Schema 3の再利用：既にexplicit panic用のため不採用。
- Native専用の別semantic IR：共通契約とverifierが重複するため、既存ownership形式を再利用する。
- Native runtime object／pointerの保存：format／信頼境界に反するため不採用。

## 実装計画

1. Schema 4選択、明示ABI tag、version別reader／writerを追加し、既存defaultとPanicを保持。
2. 共通artifact facadeで4をdispatchし、実行admissionを維持。
3. Round-trip、不正入力、budget、旧形式、復元後C実行を検証して英日文書を更新。
4. Native metadata、runtime／C context配布、隔離link/runはP02で別途継続。このRFCだけでpackage完了とはしない。

## テスト計画

Schema 1／2／3のgoldenを維持。Host／C import、Guest／CExport function、有限aggregate signature、既存ownership全操作、explicit Panicを、ABI識別／span／IDを変えずround-tripする。Unknown ABI tag、各truncated prefix、不正signature／visibility／名前／index、trailing data、work／allocation／element／byte予算不足を部分公開せず拒否する。

Source由来の復元C fixtureを、既存の明示VM／native bindingで必要に応じて実行し、context／allocator同一性、出力、元failure／cleanup、解放を確認する。暗黙binding／provider効果がないこと、sourceに依存しないdecode、決定的re-encode、旧version拒否を検証し、repository baselineを実行する。Native container／SDK／debug／性能は残るroadmap成果で扱う。

## 未決事項

Format判断を妨げる未決事項はない。Native配布記録、startup entry、SDK一致はP02の別設計であり、このABI 2 fieldでは定義しない。

## 情報源

上記はprojectのformat判断。一次ローカル資料の確認日：**2026-10-20 JST**。

- [RFC 0011](0011-ownership-artifacts.md)、更新2026-10-19：ownership schema 2と独立検証。
- [RFC 0014](0014-explicit-source-panic.md)、更新2026-10-19：schema 3とexplicit Panic。
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md)、更新2026-10-19：C profile、明示grant、cleanup。
- [RFC 0012](0012-native-ownership-continuations.md)、[RFC 0015](0015-c-native-object-bridge.md)、更新2026-10-20：native／private C境界とobject binding。
- [現writer](../../../crates/bytecode/src/ownership/writer.rs)、[reader](../../../crates/bytecode/src/ownership/reader.rs)、[declaration検証](../../../crates/mir/src/unit/ownership/declarations.rs)、[記録分離prototype](../../../crates/codegen-clif/src/owned/recorded.rs)。公開更新日のないliving source、確認2026-10-20 JST。
