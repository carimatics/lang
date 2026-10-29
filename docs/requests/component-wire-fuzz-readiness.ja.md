<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: component_wire の小規模 fuzz 実行条件の調査

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

既存 component_wire target で、有用な seed と結果保存を備えた、再現可能で資源上限付きの coverage-guided campaign を実施できるか。

**この単位で完結する理由:** 既存 codec target 1つの実行準備を単位とし、リポジトリ全体の fuzz service や lifecycle 状態機械を必要としない。

## 引継ぎ観測と確認対象

2026-10-30 のレビューでは fuzz target の compile/lint は確認されたが、継続的 coverage-guided campaign は確認できず、新規 campaign も実施していない。既存の決定的な拒否テストには引き続き意味がある。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`fuzz/fuzz_targets/component_wire.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/fuzz/fuzz_targets/component_wire.rs) — 基準版における Git 最終変更日: 2026-10-23.
- [`fuzz/Cargo.toml`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/fuzz/Cargo.toml) — 基準版における Git 最終変更日: 2026-10-29.
- [`.github/workflows/macos-aarch64.yml`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/.github/workflows/macos-aarch64.yml) — 基準版における Git 最終変更日: 2026-10-28.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. この target の入力解釈、既存 codec fixture、現在の CI コマンドを確認する。外部 corpus は導入せず、正当な seed 1件と決定的に拒否される seed 1件を選ぶ。
2. インストール済み Rust/cargo-fuzz の version と instrumentation 対応を記録する。project の pinned stable は変更せず、対応 tooling がなければ前提不足として報告する。
3. tooling が利用可能な場合だけ、この target を最大60秒、libFuzzer の `-max_total_time=60 -timeout=2 -rss_limit_mb=512 -max_len=4096`、専用 corpus/artifact prefix、外側90秒 watchdog で実行する。
4. 正確なコマンド、version、得られた coverage、seed hash、終了 status、保存 artifact を記録する。発見時は有界な再現・縮小と通常回帰テストへの取り込み案を示し、未発見なら限定実行で未発見と記す。
5. schedule・資源上限・artifact 保存・回帰化の担当を含む1 job の案を作る。CI は変更しない。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-component-wire-fuzz-readiness-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** 長時間 campaign、別 target、依存/toolchain install、CI 導入、Miri/sanitizer 全体導入、exploit 開発。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/component-wire-fuzz-readiness.md` を英語の正本、`docs/investigations/component-wire-fuzz-readiness.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

target 1件について有界な実行記録または具体的な tooling 不足と、単一 job の提案がある。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
