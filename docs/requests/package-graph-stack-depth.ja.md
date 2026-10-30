<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: 有界な深い package graph の stack 使用量の調査

2026-10-30 JST照合: 最新の指摘・修正・検証・明示的に残す追加検証は[全体レビュー](../ja/review-2026-10-30.md)の対応行を参照してください。本書は元の調査範囲を保存する履歴であり、以下の継承観測を現在も未修正だと解釈しないでください。

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

許容される256 package の依存鎖を既定の test thread stack で処理できるか。失敗するならどの再帰経路が原因か。

**この単位で完結する理由:** 既存の有限 graph test 1件と DFS 再帰を対象とする再現性・algorithm 調査であり、package 機能全体と独立している。

## 引継ぎ観測と確認対象

レビューでは macOS arm64・pinned Rust 1.99.0 で `default_package_boundary_handles_a_deep_chain_without_unbounded_recursion` が単独実行でも abort した。RUST_MIN_STACK=8388608 では成功したが診断実験にすぎない。M4 の観測であり M3 完了判定とは別。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`src/package/graph.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/package/graph.rs) — 基準版における Git 最終変更日: 2026-10-29.
- [`tests/package_graph.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/tests/package_graph.rs) — 基準版における Git 最終変更日: 2026-10-29.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. 基準版269行付近の当該 test と graph 573/672行付近の visit を読み、package/深さ上限と再帰・cleanup 経路を記録する。
2. 専用子プロセスで `cargo test --locked --test package_graph default_package_boundary_handles_a_deep_chain_without_unbounded_recursion -- --exact` を実行する。安定した専用 TMPDIR を使い、継承された stack 設定を記録する。
3. 外側 watchdog と abort 時の回収を用意する。既定 stack の失敗が再現した場合だけ RUST_MIN_STACK=8388608 で1回比較し、stack 依存の診断とする。既定設定の合格に読み替えない。
4. cycle 検出・順序・診断を維持する明示 stack の走査と、明文化した深さ制限を比較し、最小回帰条件を1つ示す。test stack の増量や走査の修正は実装しない。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-package-graph-stack-depth-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** package graph 全体のレビュー、無制限 graph 生成、stack 増量だけの修正、修正実装、M3 board 編集。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/package-graph-stack-depth.md` を英語の正本、`docs/investigations/package-graph-stack-depth.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

当該 test の platform/設定別の結果、再帰経路の説明、graph 意味論を保つ限定した改善案がある。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
