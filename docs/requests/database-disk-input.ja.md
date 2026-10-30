<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: query database の disk input 経路の調査

2026-10-30 JST照合: 最新の指摘・修正・検証・明示的に残す追加検証は[全体レビュー](../ja/review-2026-10-30.md)の対応行を参照してください。本書は元の調査範囲を保存する履歴であり、以下の継承観測を現在も未修正だと解釈しないでください。

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

disk-backed な database::input は待機する open に到達するか。editor overlay がある場合、その操作を回避するか到達するか。

**この単位で完結する理由:** database が入力元を選ぶ1つの判断を library fixture で調べるため、editor 起動や LSP protocol 全体の監査を必要としない。

## 引継ぎ観測と確認対象

2026-10-30 のレビューは database 378行付近の類似読み込みを静的に指摘しただけであり、editor/LSP の停止を実行確認していない。再現済みとして扱わない。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`crates/semantics/src/database.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/semantics/src/database.rs) — 基準版における Git 最終変更日: 2026-10-18.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. `rg -n 'fn input|File::open|overlay' crates/semantics/src/database.rs crates/lsp` で input と呼び出し側を特定し、disk と memory の選択だけを追う。
2. この method の path 正規化、cache/overlay 優先順位、open、入力エラー変換を記録する。
3. 既存の直接 database fixture から disk input に到達できる場合、通常ファイル・memory overlay・FIFO を各1ケース、3秒上限の子プロセスで比較する。fixture がなければ静的根拠で止め、不足を明記する。
4. 最小の拒否境界と、未保存 overlay を保つ回帰条件を示す。CLI の結果から LSP 応答性まで確認したと推定しない。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-database-disk-input-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** editor 自動操作、LSP 全 session、一般的 cache invalidation、別の入力 loader、修正実装。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/database-disk-input.md` を英語の正本、`docs/investigations/database-disk-input.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

disk/overlay の分岐が明確で、静的な疑いと新たな動的確認を区別した結論がある。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
