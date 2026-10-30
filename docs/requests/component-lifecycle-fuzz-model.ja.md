<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: 公開 component lifecycle の小さな状態遷移テストモデル調査

2026-10-30 JST照合: 最新の指摘・修正・検証・明示的に残す追加検証は[全体レビュー](../ja/review-2026-10-30.md)の対応行を参照してください。本書は元の調査範囲を保存する履歴であり、以下の継承観測を現在も未修正だと解釈しないでください。

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

公開 component API の register/call/cancel/reenter/close を確認する、最小の有界な状態遷移モデルは何か。

**この単位で完結する理由:** instance/binding 各1つの model と判定条件を、静的調査と最大5件の有限 trace でまとめる単位であり、fuzz 基盤は不要。

## 引継ぎ観測と確認対象

レビューは、単一 buffer の codec target では lifecycle の網羅性を示せないため、操作列の探索を提案した。これは検証設計の仮説であり、新たな lifecycle 不具合の再現ではない。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`tests/component_native_registration.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/tests/component_native_registration.rs) — 基準版における Git 最終変更日: 2026-10-26.
- [`fuzz/fuzz_targets/component_bindings.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/fuzz/fuzz_targets/component_bindings.rs) — 基準版における Git 最終変更日: 2026-10-22.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. 公開 C table の lifecycle test と既存 binding target を特定し、状態・観測できる status を抽出する。論理的な単一 thread scheduler と OS concurrency を区別する。
2. engine・instance・binding 各1つ、nested call 最大2段に限定する。型付き live-handle 管理を前提とし、古い raw pointer の逆参照を生成しない。
3. 正常 close、順序違反の拒否、cancel、再入、cleanup を扱う最大5 trace を、各最大8操作で定義する。各 trace に期待 status・保持 ownership・失敗後再利用の判定条件を付ける。
4. 既存 trace test を流用できれば限定実行する。難しければ未検証の仮定を明記した静的 model 提案までとし、将来の harness 入口1つと決定的 replay 形式を示す。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-component-lifecycle-fuzz-model-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** fuzzer 実装、実 native plugin、OS thread 並行性、任意の不正 pointer、lifecycle 意味論の変更。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/component-lifecycle-fuzz-model.md` を英語の正本、`docs/investigations/component-lifecycle-fuzz-model.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

小さな状態・操作・判定表と5件以下の trace があり、広範な監査を読み解かず将来の harness 1件を実装できる。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
