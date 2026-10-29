<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: component C v1 の上限値が適用される scope の調査

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

`ffi::Limits` の17フィールドそれぞれについて、契約上の engine/instance/control の適用範囲と、実際に値を消費する処理の scope は一致しているか。

**この単位で完結する理由:** 契約と値の消費箇所の対応表だけを成果とする静的調査であり、実行再現や ABI 変更を前提とせずに完結する。

## 引継ぎ観測と確認対象

2026-10-30 のレビューでは、`bounds` が各フィールドの最小値を計算し、`call` でも実効上限を作る一方、一部の利用側には engine の設定が渡ると報告された。対照実験の fuel=0 は拒否された。全フィールドに不具合があるという意味ではない。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`src/component_embedding/ffi.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/ffi.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — 基準版における Git 最終変更日: 2026-10-28.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. `rg -n 'struct Limits|fn bounds|preparation_work|trace_frames' src/component_embedding/{ffi,gateway}.rs` で Limits、bounds、instance/control 作成、call を特定し、RFC の上限に関係する節だけを読む。
2. fuel、service_work、host_calls、heap_bytes、live_allocations、frames、live_cells、live_tasks、canonical_indices、preparation_work、metadata_bytes、value_elements、serialization_bytes、depth、decoder_allocation_bytes、suppressed_records、trace_frames の17行を作る。
3. 各行に契約上の scope、設定入口、合成点、消費する symbol、0 の意味、共有カウンタの所有者、根拠箇所を記録する。scope が未規定なら仕様上の未解決点とし、推測で埋めない。
4. 各行を整合・不一致の疑い・未確定に分類する。未確定行には局所的な確認方法を最大1件示し、backend 全組合せの実験には広げない。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-component-limit-scope-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** 動的再現、新しい資源 policy の決定、ABI layout 変更、修正実装、milestone 更新。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/component-limit-scope.md` を英語の正本、`docs/investigations/component-limit-scope.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

17フィールドすべてに根拠または根拠不足の理由があり、仕様の曖昧さと実装の不一致が区別されている。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
