<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: component の canonical value codec 上限の調査

2026-10-30 JST照合: 最新の指摘・修正・検証・明示的に残す追加検証は[全体レビュー](../ja/review-2026-10-30.md)の対応行を参照してください。本書は元の調査範囲を保存する履歴であり、以下の継承観測を現在も未修正だと解釈しないでください。

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

呼び出しが使用する canonical Session は、実効値の `value_elements`、`serialization_bytes`、`depth`、`decoder_allocation_bytes` を適用するか。

**この単位で完結する理由:** 4上限が集まる Session/codec 境界1箇所と小さな正当な値を対象にし、decoder 全体の監査に広げずに完結する。

## 引継ぎ観測と確認対象

2026-10-30 のレビューでは、各 control フィールドを0にしたスカラー呼び出しの成功と、gateway 1795行付近の engine 由来 Session が報告された。スカラーの成功だけでは各 codec 予算の消費を証明できない。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`src/component_embedding/ffi.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/ffi.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — 基準版における Git 最終変更日: 2026-10-28.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. Session 構築と4上限の課金箇所を追い、入力 decode、値の検証、結果 serialization を区別する。
2. 各予算を消費する既存の正当な文字列・list・入れ子値 fixture の最小例を選ぶ。データは1 KiB未満、入れ子は最大4段にする。
3. backend 1つで十分な上限の対照と、call-control の各値だけ0にした最大5ケースを比較する。instance の伝播は静的に確認し、消費を確認できない予算は未検証と記す。
4. 拒否段階、cleanup、encode/decode 間の共有消費を説明する。正の課金を観測できる回帰条件を示し、wire format は変更しない。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-component-codec-limits-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** 不正入力 campaign、資源枯渇、backend 全組合せ、heap・準備予算、修正実装。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/component-codec-limits.md` を英語の正本、`docs/investigations/component-codec-limits.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

4フィールドについて Session から課金までの経路と根拠付きの結論があり、未計測経路と0の意味の未規定箇所が明記されている。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
