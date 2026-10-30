<!-- canonical-language: en; japanese-translation: ../ja/rfcs/0043-editions-and-compatibility.md -->

# RFC 0043: Editionと互換性方針

[English](../../rfcs/0043-editions-and-compatibility.md)

- ステータス: implemented
- Authors: ownerの実験段階委任に基づくCodex
- 作成日: 2026-10-30
- 最終更新日: 2026-10-30
- Tracking issue: M5-A01, M5-A02
- Supersedes: なし。RFC 0001とRFC 0034を具体化
- Superseded by: なし

## 概要

既存のsource edition `2026`を互換性reportで明示し、領域ごとの進化規則を定める。`lang compatibility --message-format json`はこのcompilerの対応source editionと独立したformat・ABI方針を表示する。filesystem・network・guest codeのI/Oは行わない。edition選択は言語全体の安定化を意味しない。

## 動機

RFC 0034は未知editionを拒否するが、利用者は対応方針を一箇所で確認できない。compiler version、source edition、lock schema、公開C ABIは異なる識別子であり、混同は不適合入力の受入やnative binary再利用の過大な保証につながる。

## 目標

- 既存の全互換性境界に具体的な受入・移行規則を記載する。
- edition-2026 manifestと過去の公開C clientを維持する。
- 破壊的遷移の前に非推奨・新edition導入・削除手順を定める。

## 非目標

新構文、架空の後継edition、pre-alpha全体の安定化、自動書換え、registry protocolやWasm formatの選択。移行・fix transactionはM5-B01で実装前に採択する。この判断だけではG2は完了しない。

## 利用者向け設計

`lang compatibility`はhuman reportを表示し、`--message-format human|json`で形式を指定する。JSON schema 1は`schema`、`compiler_version`、`source_editions`、`default_source_edition`、`domains`を持つ。各domain要素の文字列fieldは`domain`・`status`・`policy`。domain識別子は`source`・`package`・`bytecode`・`native-abi`・`component-abi`・`tooling`・`bootstrap`。statusは`experimental`・`stabilized`・`unavailable`で、安定化対象は公開component C v1 interfaceだけである。policyは制限を説明し、領域全体の相互交換を保証しない。不明・重複optionはusage error（exit 2、`CLI0001`）、JSON指定時のerrorはstderrへ構造化して出す。成功時はstdoutのreportのみ。

package editionは明示必須。defaultは既存standalone source・REPLの動作を示し、manifest field省略を許可しない。依存editionは各manifestから選び、rootから継承しない。未知editionは`PKG0002`で拒否し、既存受入規則に従ってinactive workspace catalogueも対象とする。

## 参照意味論

受け付けるeditionは`2026`のみ。parse・型・評価・所有権・破棄・割当・並行性・capability・FFI・定数実行の動作は変わらない。受入とreportに有限edition型を使い、現在のmanifest表現とwire bytesを維持する。

将来editionの採択RFCは変更する構文・意味論、package単位の選択、mixed-edition境界、対応source-to-target移行とVM/nativeの移行前後testを明示する。対応する既存editionの有効programは定義済み動作を保ち、古いsourceを暗黙に再解釈しない。target選択は依存の書換えやpackage version変更を意味しない。

非推奨方針:
1. domain・理由・影響する有効program・代替・最初のdiagnostic/toolchain release・最早削除release・明示移行または拒否経路を日英RFC/change noteに記録する。
2. 対応中の非推奨構文は引き続き有効とし、source span付きの安定diagnosticを出す。機械適用可能fixには意味保存を証明したレシピと元入力検査を要し、曖昧な変更は人が確認する。formatterは意味を暗黙に移行しない。
3. 安定化interfaceの計画的サポート終了には告知後少なくとも2回の公開toolchain releaseを設け、非互換変更には新version/namespaceを使う。RFC 0033のC v1維持保証は引き続き有効であり、この告知期間はv1削除を許可しない。
4. 実験的変更にも日付付き採択RFCと移行・拒否testを要するが、架空の2-release安定保証は与えない。security修正で以前受理した不正・権限外入力を即時拒否できるが、正確な契約と観測可能な影響を記録する。
5. 現在どのeditionにもサポート終了日はなく、本RFCで新たに非推奨となる構文もない。実際の非推奨追加時はdiagnosticと移行レシピを同時に備える。方針宣言は移行実装の証拠ではない。

## 実行方式

共通frontend/MIRをVM/JIT/AOTの意味契約とする。source editionは実行capabilityを緩めず、private native imageをportableにしない。既存standalone commandとsource package依存fixtureは同じ動作を維持する。Wasmとbootstrapは各milestoneの証拠が揃うまでunavailable。

## ABIと互換性

| Domain | 現在の受入・保証 | 更新・拒否 |
| --- | --- | --- |
| source | packageはedition 2026必須、standalone/REPLは2026。実験段階 | 未知edition拒否、将来遷移は明示採択レシピ |
| package | manifest schema 1、lock schema 1またはworkspace schema 2。version・target・toolchain・checksum完全一致 | frozen入力の完全replayまたは明示lock再生成。未知versionを推測しない |
| bytecode | profile別verifierを持つLANGBC schema 1–5。実験段階 | 宣言schemaをdecode/verifyし、未対応・不適合時はsourceから再生成 |
| native-abi | LANGNAT 1/2、program 1/2/3、SDK 1/2/3はprofile別受入。Rust/private linkageはtarget/build一致を要する | compiler/SDKを揃えて再build。version一致は信頼・権限ではない |
| component-abi | 公開C v1はRFC 0033で安定化。IDL 1、LCMP 1/2/3、LCVL 1、LCSG 1は独立version profile | 旧C caller・生成client維持。互換contractを交渉し未対応formatをeffect前に拒否 |
| tooling | package diagnostic schema 1・明示CLI exit code・versioned editor/debug/profile/distribution契約。実験段階 | 採択契約内のdiagnostic identity維持。非互換shapeはschema改訂 |
| bootstrap | 対応self-hosting chainはまだない | M6で具体subsetと再現可能stageを固定・検証 |

これは受入familyの表であり、profile間変換や全decoderが全旧schemaを受理する保証ではない。既存の厳密profile validatorを正とする。package SemVerとedition labelはC ABI・bytecode・SDK・host authorityを交渉しない。reportは未検証version直積を列挙せず方針を示す。

## 診断とtooling

manifest検証とreportで同じ有限edition受入を使う。未対応manifest schema/editionは`PKG0002`を維持し、既存span/error動作を保つ。report生成は固定サイズの有限dataを使い、依存解決を呼ばない。parser recovery・format・lint・LSP・DAPは変わらない。consumerは未知report schemaをschema 1と解釈せず拒否する。

## 性能とコスト

report生成はprojectサイズに依存しない定数処理で、小さいhost文字列とserializationのみ。edition受入は有限match。guest割当・runtime dispatch・code size・性能の主張は変わらない。

## Securityと信頼境界

互換性metadataは権限を付与せずartifactを認証しない。未知edition・formatはfail closed。既存の入力上限・profile検証・SDK一致・capability・署名検査を維持する。reportはambient pathからmanifestやcredentialを読まない。

## 代替案

- 変更なしではedition・release・formatの区別がRFC間に散在する。
- 文書のみでは方針は定まるがtoolが対応edition一覧を検査できない。
- 最新editionの自動選択は無変更packageの意味を暗黙に変える。
- 単一のglobal versionは簡単だが、安定C callerと実験private image・source意味論を混同する。
- 移行実演だけを目的とした破壊的editionは要件なしに利用者負担を増やす。現editionを有限に保ち、有用な検証fixを別途実装する。

## 実装計画

A01で日英方針を採択しM5全体ボードを開始する。A02で互換性CLIと共通edition受入を実装し、既存manifest/lock/artifact/旧C fixtureと必須baseline・両host CIを検証する。B01は移行レシピと検証書込みを独立に定義する。他のM5契約は未完了。

## テスト計画

JSONの正確なkey・domain順序、human出力、不明・重複引数、構造化errorを確認する。reportの対応editionを実manifest受入と依存実行に通し、省略・数値・未知・将来editionを拒否する。lock replay/refusal、bytecode profile往復、独立した旧C client実行を両host CIで維持する。これは現在の互換性検証であり、将来editionやWasm実行済みの主張ではない。

## 未決事項

2026-10-30 JST、RFC 0001の実験段階委任に基づき採択。有限で明示的な受入は既存source/format動作を保ち、領域分離は偶発的なABI保証を防ぐと判断。A02を妨げる設計質問なし。将来edition意味論・具体的非推奨・registry/Wasm protocol・安定sourceのサポート寿命は個別RFCで検証する未決事項。

## 出典

- [RFC 0001](../../rfcs/0001-project-charter.md)、更新2026-10-28: 互換性領域と実験段階委任。
- [RFC 0033](../../rfcs/0033-component-v1-compatibility.md)、更新2026-10-27: 公開C v1の厳密維持と旧client証拠。
- [RFC 0034](../../rfcs/0034-package-workspaces-and-cli-diagnostics.md)、更新2026-10-29: 必須edition、manifest/lock受入、diagnostic。
- repositoryのmanifest/lock validator・source/artifact test: 更新日記載なし、2026-10-30 JST確認。本RFCはproject方針であり外部標準や性能の主張ではない。
