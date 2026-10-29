<!-- translation-of: ../../rfcs/0037-package-lint-diagnostics.md -->

# RFC 0037: package lint 診断

[English](../../rfcs/0037-package-lint-diagnostics.md)

- Status: accepted
- Authors: project maintainers with Codex
- Created: 2026-10-29
- Last updated: 2026-10-29
- Tracking issue: M4-B03
- Supersedes: none
- Superseded by: none

## 概要

`lang lint` は選択 package と宣言 test を解析し、guest code の実行や artifact 公開を行わない。`--deny-warnings` は warning を command 失敗にする。既存 schema-1 human／JSON 診断は元 file path と UTF-8 byte span を保持する。

## 動機

build はすでに到達不能文を診断するが、CI 向けの実行を伴わない package 全体の lint command がない。text 検索では解決済み local 参照と comment、別 scope の名前、field を区別できない。

## 目標

- 固定 package input、frontend 解析、既存の安定診断識別子を再利用する。
- 正確な未読 binding warning と決定的な package／test 報告を追加する。
- B03 完了前に pass／fail、warning 拒否、JSON fixture を提供する。

## 対象外

- 自動修正、attribute、lint plugin、設定 file。
- 採択済み証明を伴わない program 全体の dead-code、topology、lifetime、concurrency 推論。
- 別途追跡する LSP／code-action 統合。

## ガイドレベル設計

`lang lint [--deny-warnings]` は共通の `--manifest-path`、`--package`、`--target`、`--locked`、`--offline`、`--message-format human|json` を受け付ける。warning のない有効 package は exit 0。通常 warning は exit 0、`--deny-warnings` は warning があれば exit 1。解析／入力失敗は exit 1、不正 option は exit 2。成功した lint の stdout は空。

## 参照セマンティクス

通常の選択 package、続いて宣言した root test 全てを同じ不変 input epoch から解析する。test は既存 test namespace で `_package` を公開する。実行しないため、実行可能 `main` や test 結果 zero は要求しない。未選択 workspace member は既存の manifest のみ検証する方針を保つ。import しない source file も取得／lock するが、架空の semantic scope を付与しない。

`LNT0001` は、名前が `_` で始まらず、その function の expression arena に `ExpressionKind::Local` として local ID が一度も現れない解決済み parameter、mutable local、pattern binding への warning。代入だけでは代入先を読んだとしない。到達不能 expression の参照も保守的に use と数える。compiler が生成した arena-owner binding は除外する。元 binding 名の span を使う。initializer には effect／ownership の影響があり得るため、削除を提案しない。

到達不能文の既存 `SRC0019` など frontend warning を保持し、第 2 の制御フロー実装や code 再割当てを行わない。新 semantic warning は解析成功後だけ生成する。frontend 失敗時は既存診断を保持し、診断で表現されていない構造的失敗だけに `CLI0002` を生成する。

root／dependency／test で反復する所見は元 path、byte range、code、severity、message で重複除去する。各解析の source ID に依存せず、path、start／end offset、code、severity、message 順に並べる。集約診断上限は出力前に 65,536 とし、超過時は `PKG0004` を報告して部分診断 list を公開しない。warning 拒否は終了値だけを変え、severity／code は変えない。

## 実行モード

lint は VM／native／task profile 共通の frontend semantics を使う。実行形式への lowering、host callback、SDK compile、test 実行、Cargo 実行を行わない。runtime 値や実行 mode の挙動は変わらない。

## ABI と互換性

command と診断 code の追加のみ。既存 source、ABI、bytecode、lock schema、通常 build／run／test の warning 挙動は不変。lint は `Lang.lock` や artifact を書かず、`--locked` は既存 preparation で現在 snapshot を検証する。edition／artifact migration は不要。

## 診断とツール

schema 1、元 path、severity、primary byte span を保持する。LSP／DAP／formatter は不変。将来の安全な fix は edit 検証と effect／ownership 保持を別途定義する。RFC 0004／0005 の広い lint は将来の stabilization 要件として残し、この有限な初期 rule set はそれらの証明を主張しない。

## 性能とコストモデル

解析ごとに現在 frontend quota を使い、test 数は manifest の既存上限まで。新 rule は function の expression／local arena を local-ID set とともに各一度走査する。重複除去済み所見は最大 65,536 件、既存 package／source 上限も適用する。この計算量以外の性能を主張しない。

## セキュリティと信頼境界

取得した package bytes と既存 capability 制限 resolver を使う。manifest／source／lock 失敗は semantic lint より先。guest effect、filesystem 書込み、外部 tool、network capability を与えない。診断 message／path は既存 escaped human／structured JSON 出力を使う。

## 代替案

- 変更なしでは CI 用の実行しない package lint 操作がない。
- 外部 text scanner は解決済み local ID と元 semantic 診断を欠く。
- 独立した完全 data-flow lint engine はより多くを証明できるが compiler semantics を重複し、選択 delivery には不要。
- compiler warning の転送だけなら小さいが未読 binding を逃す。解決済み ID rule は推測の effect 解析をせず、上限付きで有用な検査を加える。

## 実装計画

1. 上限付き解析／集約と解決済み未読 binding rule を追加する。
2. artifact を公開せず lint option と warning 拒否終了値を接続する。
3. 実 dependency／test／JSON と effect なし fixture、必須 baseline、両 host CI を実行する。実 scope が変わらなければ B03 の review 済み 3 pt を保つ。

## テスト計画

pass、warning、拒否、元 span、shadow する名前、field／comment、underscore 抑制、write-only 変数、既存到達不能診断、dependency／test 重複除去、不正 test／source／lock、実行可能 entry 不要、実行すれば失敗する host call を検証する。既存 output と lock bytes の保持を確認する。collector 境界で診断上限の丁度と超過を検査する。repository baseline と両 host CI を実行し、benchmark／fuzz campaign は主張しない。

## 未解決事項

RFC 0001 の実験委譲で 2026-10-29 JST に採択。証明済み frontend 識別子の再利用と保守的な解決済み ID rule は合理的に妥当。名前の underscore 抑制と有限な初期 rule set は実験的 product 判断で、stabilized 方針ではない。広い lint は実装前に独自の証明と review を要する。

## 出典

- repository RFC 0001（更新 2026-10-28）、0002（2026-10-27）、0004／0005（2026-10-28）、0034（2026-10-29）を 2026-10-29 JST に確認。source 実装 file に埋込み公開／更新日はない。既存 `SRC0019` と解決済み local arena は同日に現在 checkout で確認した。外部比較や測定性能の主張は使わない。
