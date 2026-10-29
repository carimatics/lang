<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: component 呼び出し時の heap・allocation 上限の調査

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

instance/control に設定した `heap_bytes` と `live_allocations` は、engine の元の消費計数を保ったまま allocator に届くか。

**この単位で完結する理由:** 同じ allocator に属する2カウンタと、その伝播経路だけを対象にし、小さな有限のゲスト操作1種類で判断できる。

## 引継ぎ観測と確認対象

2026-10-30 のレビューでは、下位 scope の上限を0にしても Vec<i64> の作成・push・pop を含む呼び出しが VM/JIT とも42を返した。allocator callback の27回には準備用確保も含まれ、ゲスト確保回数とはいえない。AOT は未検証。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`src/component_embedding/ffi.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/ffi.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — 基準版における Git 最終変更日: 2026-10-28.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. `bounds`、`call`、allocator 構築を特定する。基準版 gateway の1774・1795・1967行付近から、選んだ2フィールドだけを具体的な allocator まで追う。
2. gateway の公開 C table テストの setup を利用し、一時領域に有限の Vec 操作を用意する。engine の上限を十分にし、通常成功と、instance/control の片方のフィールドだけ0にした場合を比較する。
3. 利用できる VM/JIT ごとに最大5ケースを実行する。status、戻り値、可能なら段階別 callback 数、cleanup を記録する。子プロセスの5秒 timeout は停止または未確定として扱う。
4. nested call が元のカウンタを保持する経路は静的に確認する。最小回帰ケースを示し、課金量を特定できた場合だけ境界一致・1不足の追加確認案を記す。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-component-allocation-limits-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** codec・準備予算、無制限の確保、nested call の網羅試験、AOT 環境構築、allocator の修正。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/component-allocation-limits.md` を英語の正本、`docs/investigations/component-allocation-limits.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

各フィールド・検証 scope に観測または制約があり、allocator までの経路と、共有消費を保持する回帰確認条件が示されている。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
