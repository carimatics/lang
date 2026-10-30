<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: root source の通常ファイル制約の調査

2026-10-30 JST照合: 最新の指摘・修正・検証・明示的に残す追加検証は[全体レビュー](../ja/review-2026-10-30.md)の対応行を参照してください。本書は元の調査範囲を保存する履歴であり、以下の継承観測を現在も未修正だと解釈しないでください。

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

`lang check <root>` は、open が待機する前に通常ファイル以外の root source を短時間で拒否できるか。

**この単位で完結する理由:** CLI 入力1つのファイル種別判定を調べる単位であり、import や artifact 読み込みを必要としない。

## 引継ぎ観測と確認対象

2026-10-30 のレビューでは、通常の root source は成功し、FIFO の root は子プロセスの3秒 timeout まで停止した。File::open 後の byte 上限は open 自体の待機を制限しない。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`src/main.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/main.rs) — 基準版における Git 最終変更日: 2026-10-29.
- [`crates/semantics/src/modules.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/semantics/src/modules.rs) — 基準版における Git 最終変更日: 2026-10-29.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. lang check から root 読み込みと最初の open までを追い、root・symlink・byte 上限・診断の既存条件を確認する。
2. 対応する Unix host の専用一時ディレクトリに小さな正当な root と FIFO を1つずつ作る。CLI 2ケースだけを各3秒上限の子プロセスで実行し、finally で子プロセス群の終了・回収と FIFO 削除を行う。
3. 終了 status、経過時間、診断、通常ファイルの成功を記録する。timeout は停止の再現とし、正常拒否には数えない。別 OS は未検証とする。
4. 非 blocking open と open 後 handle の種別検証を対象 OS の API に照らして検討する。事前 path metadata の競合を説明し、回帰確認条件を1つ示す。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-root-source-file-input-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** import、artifact、editor/LSP 入力、競合の stress test、共通 loader の実装。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/root-source-file-input.md` を英語の正本、`docs/investigations/root-source-file-input.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

root 経路の2ケースの結果と、現在の path 制約を維持する具体的な拒否方針がある。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
