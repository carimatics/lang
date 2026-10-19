<!-- translation-of: ../../rfcs/0016-indirect-repl-previews.md; canonical-language: en -->

# RFC 0016: 間接値の REPL 結果プレビュー

[English](../../rfcs/0016-indirect-repl-previews.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-20
- Last updated: 2026-10-20
- Tracking issue: M2-V17
- Supersedes: none; RFC 0010 を拡張
- Superseded by: none

## 概要

RFC 0010 の VM/REPL 結果表示を Vec、Map、shared 所有値、weak トークンに拡張する。例えば 9 を含む Vec を返す関数は、実行前の `Unsupported` ではなく `core::Vec [9]` を表示する。ソースの arena 脱出検査は必須のままとする。

受理判断: 実装前に **実験段階の委任に基づき accepted**。保護済み結果を読み取るツール機能であり、有限のホスト作業領域を明示し、ゲスト操作を追加しないため、RFC 0004/0007/0008/0010 との矛盾はないと判断した。受理時点では実装と境界テストは未検証であり、受理だけでは V17 は完了しない。

## 動機

既存 REPL は collection 操作を実行できるが、RFC 0010 が表示契約を後続設計に委ねたため、それらの結果型をゲスト実行前に拒否する。有限のプレビューにより、利用者がスカラー抽出関数を追加せずに結果を確認できる。Map の内部格納順や暗黙の weak 昇格を導入してはならない。

## 目標

- 所有権の移転・retain・release なしに collection の論理要素と shared ペイロードを表示する。
- Map の表示を内部格納順・挿入順から独立した決定的な順序にする。
- 結果 escrow、callback のない観察、明示的 allocator 権限、cleanup 後の公開を維持する。

## 非目標

- ゲストの反復 API、ユーザー定義表示 callback、グラフ同一性や alias の表示、ゲスト値の永続化。
- weak 経由のペイロード参照、arena 寿命の延長、native REPL 実行。
- ABI、bytecode、シリアライズ形式の変更。

## 利用者向け設計

| 結果 | プレビュー |
| --- | --- |
| 2 要素の Vec | `core::Vec [9, -1]` |
| 空の Vec | `core::Vec []` |
| 2、1 の順に挿入した Map | `core::Map { 1: false, 2: true }` |
| 空の Map | `core::Map {}` |
| 9 の shared 所有値 | `shared(9)` |
| strong 所有値が生存する weak | `weak::Live` |
| 最後の strong 所有値が破棄された weak | `weak::Expired` |

入れ子のペイロードには既存の scalar、Text、record、enum、Option、Result の形式を使う。Map の i64 キーは数値順、Text キーは元の UTF-8 byte 列の辞書順で並べる。Text キー自体は RFC 0010 の escape 形式で表示する。これはツールの表示規則であり、ソース Map に反復や hash 順序の契約は加えない。

shared ペイロードは値として表示し、同じ alias の内容を繰り返し表示する場合がある。weak は通常 cleanup 後の保護された結果観察時点で生存状態だけを表示し、対象ペイロードは読まない。参照数やアドレスは表示しない。

ソース式は arena のブランド付き参照を字句領域外に返せない。通常 cleanup 後には生きた領域権限を持たないため、preview adapter も直接たどる arena 参照を含む結果型をゲストの効果が生じる前に拒否する。たどらない weak の対象型にはペイロード表示対応を要求しない。失効した arena 参照を使えると誤解させる placeholder は表示しない。

## 参照セマンティクス

構文、名前解決、型推論、move、loan、評価順は変えない。表示準備では nominal/core のペイロード、Vec 要素、Map のキー・値、shared ペイロードを再帰的に型検査する。weak の対象ペイロードはたどらない。arena 参照など未対応の権限型は実行前に拒否する。

表示中は allocator と保護された storage を不変借用する。Vec はトークンの生存、初期化、添字を検査する。Map はキー・値を検証し、有限のホスト添字配列だけを並べ替え、ゲスト storage は変更しない。shared は registry 所有者、ticket 世代、種別、ペイロード型、customer custody を検証する。strong ペイロードに進む際、その検証済み node の非公開 custody を入れ子のトークンに引き継ぐ。ゲスト read ticket は確保しない。weak は同じ token/custody 境界を検証し、昇格せずに strong 所有値の残存だけを確認する。

表示はゲストコード、provider callback、意味的 clone、retain/release、loan 取得、safepoint、instruction fuel、service-work 消費を追加しない。Rust の不変借用が観察中のゲスト変更を排除する。strong cycle は既存 service が禁止し、weak 辺は探索を終える。

RFC 0010 の失敗契約を維持する。表示失敗が主原因となり、その後も cleanup を試み、cleanup 失敗では context を poison し、部分的な表示は公開しない。破損・失効・他所有者の token は観察エラーであり `weak::Expired` に偽装しない。作業領域・出力の不足は有限の preview-limit エラーとする。

## 実行モード

bytecode VM と REPL にホスト表示器を実装する。native AOT の意味と C export は変えず、native 表示器は追加しない。任意 JIT、component、定数評価、組み込み callback に新たな入口や権限を追加しない。

## ABI と互換性

従来未対応だった結果型の REPL テキスト出力を追加する。既存 RFC 0010 の出力 byte 列は変えない。ソース構文、native/component ABI、bytecode、シリアライズ IR、package metadata、edition、移行ツールは変更しない。preview はシリアライズ形式ではなく診断表示として扱う。

## 診断とツール

既存の構造化 Unsupported、準備 limit、表示 limit/error 経路を使い、span と cleanup 診断を維持する。parser recovery、formatter、LSP、DAP は変えない。arena 脱出は frontend ownership 診断のままとする。REPL 文書には新形式と arena 制約を記載する。

## 性能とコストモデル

出力、反復的探索セル、Map 並べ替え領域をゲスト実行前に、RFC 0010 と同じ累積 compiler/host budget で確保する。既存の出力上限の既定値は 65,536 bytes。間接探索と Map 添字領域にはホスト検証 limit から導く有限のセル上限も設け、表示中に拡張しない。既存の有限型だけの表示は小さい型深さベースの領域を維持する。

Vec は要素を逐次訪問する。Map は sort 前に要素数、領域容量、Text キーの元 byte 数合計を残り出力上限に対して確認する。添字を追加 allocation なしの O(n log n) sort で並べ、比較には検証した不変キーを使う。全ループは有限の入力 cursor を進めるか、事前に許可した出力・探索領域を消費する。shared alias の再訪問にも同じ上限を適用する。Rust stack の再帰やゲスト allocation は不要。これらは上限であり、遅延・メモリ効率の保証ではない。

## セキュリティと信頼境界

保護された結果内の guest token も未信頼として扱う。検査済み allocator/collection/registry 観察を再利用し、観察用 token byte のコピーから所有値を作らない。非公開 custody 座標はソースやホスト呼出元が指定できない。アドレス、内部 capacity、registry 同一性、Map の内部順序を公開しない。ペイロード観察前に検証し、全失敗で cleanup を維持する。

## 代替案

- 現状維持では有用な collection 式が未対応のままになる。
- ゲスト・library 表示器は実行・効果・明示的 ownership 操作を要し、保護された preview 契約と矛盾する。
- Map の内部順・挿入順の表示は表現詳細を公開する。キーの正規順はその契約を回避する。
- 長さだけの表示は単純だが、通常の REPL 利用で必要な値を隠す。
- weak の対象探索は昇格や新権限を要し cycle を作り得る。検証済み生存表示には両方不要。
- arena 寿命の延長は ownership model を変えるため採用しない。

## 実装計画

1. 既存 runtime の反復的表示器に、collection/shared の読み取り専用観察を追加する。
2. VM の既存 shared registry を、不変の結果 state 借用下で渡す。escrow と cleanup の順序を維持する。
3. 正確な出力、順序、生存状態、入れ子、limit、失敗・cleanup の証拠を追加し、英日 architecture と REPL 文書を更新する。frontend/IR、native codegen、package manager の構造変更は不要。

## テスト計画

空・非空・入れ子の Vec、両キー型の Map、逆順挿入・置換・削除、shared ペイロード custody、生存・期限切れ weak、weak 辺で探索が止まる再帰グラフを検証する。ちょうどの出力上限と不足、作業領域不足、service counter 不変、allocator の release 均衡、表示失敗 cleanup を確認する。arena 脱出 compile-fail と未対応権限の効果前拒否を維持する。VM、CLI REPL と repository baseline を実行し、既存 native conformance で guest semantics 不変を確認する。新しい ABI・fuzz・benchmark 形式は追加しない。

## 未決事項

設計を妨げる未決事項はない。明示的に生きた arena 権限を持つ表示器や native adapter は別の設計変更とする。測定に基づくホストメモリ調整は、上限を弱めずに行える。

## 情報源

上記はプロジェクトの設計判断であり外部情報の主張ではない。ローカル資料の確認日: **2026-10-20 JST**。

- [RFC 0004](0004-owned-values-and-cyclic-graphs.md)、更新 2026-10-17: 非循環の strong ownership と arena 領域。
- [RFC 0007](0007-source-ownership-and-core-values.md)、更新 2026-10-18: Map キー・反復禁止、weak 操作、arena 脱出規則。
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md)、更新 2026-10-19: allocator 権限と失敗 cleanup。
- [RFC 0010](0010-owned-repl-previews.md)、更新 2026-10-18: 結果保護、上限、公開、明示的な後続 RFC の要件。
- [読み取り専用観察](../../../crates/runtime/src/inspection.rs) と [shared registry 検査](../../../crates/runtime/src/shared/managed/graph.rs)、公開更新日時のない living source。確認 2026-10-20 JST。
