<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: roadmap と現行 board の進捗情報重複の調査

2026-10-30 JST照合: 最新の指摘・修正・検証・明示的に残す追加検証は[全体レビュー](../ja/review-2026-10-30.md)の対応行を参照してください。本書は元の調査範囲を保存する履歴であり、以下の継承観測を現在も未修正だと解釈しないでください。

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

roadmap の現行進捗記述のうち、roadmap に正本として置くべきものと、現行 delivery board を参照すべきものはどれか。

**この単位で完結する理由:** 2文書に重複する現行進捗の事実を列挙する有限の単位であり、実装や見積もりの変更とは独立している。

## 引継ぎ観測と確認対象

レビューは roadmap を live board への入口とし、branch/PR/進捗数の重複を減らすことを提案した。完了済み M3 の再開や保守作業への effort point の創作を求めていない。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`docs/roadmap.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/roadmap.md) — 基準版における Git 最終変更日: 2026-10-29.
- [`docs/milestone-4.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/milestone-4.md) — 基準版における Git 最終変更日: 2026-10-29.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. 対象 commit の active milestone を確認する。M4 でなくなっていればその事実を記録し、現在リンクされる board 1件だけを対象にする。過去 board 全体へ広げない。
2. 現行 branch/PR、完了件数、残見積もり、gate 状態の重複を列挙し、roadmap の outcome 定義と日々の実行進捗を区別する。
3. 各重複に正本の位置、検証側の参照、リンク化または要約維持の判断を付ける。他 session の進捗を再計算・更新しない。
4. 英日例を最大3件と、将来の編集で影響する repository consistency の期待条件を示す。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-roadmap-progress-authority-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** board/roadmap 編集、新 delivery 行、見積もり変更、完了 milestone の再開、RFC 書き換え。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/roadmap-progress-authority.md` を英語の正本、`docs/investigations/roadmap-progress-authority.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

現行進捗の重複と正本の対応表があり、status・見積もり計算を変更していない。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
