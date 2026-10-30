<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: CLI artifact 入力の通常ファイル制約の調査

2026-10-30 JST照合: 最新の指摘・修正・検証・明示的に残す追加検証は[全体レビュー](../ja/review-2026-10-30.md)の対応行を参照してください。本書は元の調査範囲を保存する履歴であり、以下の継承観測を現在も未修正だと解釈しないでください。

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

`lang run --artifact <path>` は、有界な artifact 読み込みより前の open で停止せずにファイル種別を検証できるか。

**この単位で完結する理由:** 既存の正当な artifact を使い、binary file の CLI 境界1つとして source/import と独立して確認できる。

## 引継ぎ観測と確認対象

2026-10-30 のレビューでは、FIFO artifact が3秒間停止した。基準版 main 1135行付近の read_artifact は byte 上限の適用前に path を open していた。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`src/main.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/main.rs) — 基準版における Git 最終変更日: 2026-10-29.
- [`tests/artifact_cli.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/tests/artifact_cli.rs) — 基準版における Git 最終変更日: 2026-10-28.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. read_artifact と CLI 側を読む。`cargo test --locked --test artifact_cli -- --list` で既存の artifact producer/consumer fixture を見つけ、その setup だけを確認する。
2. fixture のコマンドで専用ディレクトリに小さな正当な artifact を作り、通常ファイルの対照として実行する。
3. Unix では同じ artifact option に FIFO を渡す。両ケースとも3秒上限の子プロセスとし、finally で終了・回収・削除する。
4. open と decode のどちらで停止・拒否したか、終了コード、診断を記録する。既存 byte 上限と version 検証を保つ handle ベース拒否条件を示す。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-artifact-file-input-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** 不正 artifact campaign、format 変更、source 読み込み、package install、修正。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/artifact-file-input.md` を英語の正本、`docs/investigations/artifact-file-input.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

正当な artifact の対照、FIFO の結果、有界なファイル読み込みの受入条件がある。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
