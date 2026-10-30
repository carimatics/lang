<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: RFC 0027 の現在の契約と履歴記述の整理調査

2026-10-30 JST照合: 最新の指摘・修正・検証・明示的に残す追加検証は[全体レビュー](../ja/review-2026-10-30.md)の対応行を参照してください。本書は元の調査範囲を保存する履歴であり、以下の継承観測を現在も未修正だと解釈しないでください。

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

RFC 0027 のどの節が現在の component 契約で、どの節が過去の checkpoint または reference artifact の説明か。

**この単位で完結する理由:** 契約文書1件の解釈・構成を単位とし、実装コメント1箇所を整合確認に使えば独立して判断できる。

## 引継ぎ観測と確認対象

レビューでは規範と日付付き checkpoint の混在が指摘された。“The reference remains a decision artifact, not an implemented library” は reference artifact を指す可能性があり、実 API の実装状況との矛盾と即断しない。Clif の説明にも public C table を将来作業とする記述があった。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — 基準版における Git 最終変更日: 2026-10-28.
- [`crates/codegen-clif/src/component.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/codegen-clif/src/component.rs) — 基準版における Git 最終変更日: 2026-10-27.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. RFC 0027 の見出しを現在の規範・理由・日付付き証拠・曖昧に分類し、日本語 mirror の意味の一致も確認する。
2. reference library の文と component.rs 160行付近のコメントについて、指す artifact/API と実装根拠を具体的に対応付ける。
3. 短い現行契約の構成案と、節から既存 history への移動先表を作る。accepted decision、anchor、互換性の理由、日付を維持し、行数だけでなく byte/語数も測る。
4. 英日対の文言例を最大3件示す。保証を変える案は設計判断が必要として記録し、暗黙に採用しない。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-component-contract-documentation-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** RFC の実際の移動・編集、component 保証/status 変更、全 RFC のレビュー、board 更新。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/component-contract-documentation.md` を英語の正本、`docs/investigations/component-contract-documentation.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

主要節の分類と、曖昧な reference/API 記述の根拠付き解釈・移動案がある。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
