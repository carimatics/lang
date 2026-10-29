<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: component の preparation_work・metadata_bytes の調査

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

`preparation_work` と `metadata_bytes` はどの準備操作で消費され、instance/control の値がその操作に適用されるか。

**この単位で完結する理由:** binding 1件の準備予算を単位とする。準備時の課金と実行を区別することで、対象予算を消費しない操作の成功を誤って不具合と判断しない。

## 引継ぎ観測と確認対象

2026-10-30 のレビューでは、control の各フィールドを個別に0にしてもスカラー呼び出しが成功し、準備設定が engine 由来だと報告された。ただしスカラー操作が必ず metadata bytes を消費することまでは示していない。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — 基準版における Git 最終変更日: 2026-10-28.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. スカラー binding 1件の準備予算の作成・再利用を追い、work・metadata の課金箇所とカウンタの寿命を特定する。
2. 既存 gateway fixture から各フィールドを実際に消費する有限の操作を選ぶ。公開 call 段階に消費箇所がなければ、その段階を記録し、当該実験は打ち切る。
3. 課金される各フィールドについて backend 1つで、十分な上限の対照と instance/control の0設定を比較する。最大6ケースとし、拒否段階を契約と照合する。
4. 準備失敗後に既存の消費量が保持され、予約が解放されるかを報告する。最小の修正方向と互換性上の疑問を示し、実装はしない。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-component-preparation-limits-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** heap 計数、codec 上限、JIT cache、17フィールド全体の再設計、修正実装。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/component-preparation-limits.md` を英語の正本、`docs/investigations/component-preparation-limits.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

2フィールドの段階・課金・scope の表があり、適用漏れ、正当な未消費、契約未規定を区別できる。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
