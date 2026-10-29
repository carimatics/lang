<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: component binding 1件の JIT 反復準備コストの調査

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

同じ module/binding を反復呼び出しすると JIT compile は何回起き、call ごとの検証・課金を省かずに何を再利用できるか。

**この単位で完結する理由:** binding 1件の compile 回数と段階別コストだけで改善判断を支えられ、cache 実装や言語全体の benchmark を必要としない。

## 引継ぎ観測と確認対象

レビューが引用した既存計測は2026-10-27、source c9ffd19 の固定 workload で、準備込みの中央値が VM 94.292 µs、JIT 1,267.583 µs だった。今回の新規計測でも backend 全般の速度比でもない。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`src/component_embedding/gateway.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/src/component_embedding/gateway.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`tools/m3-bench/run.py`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/tools/m3-bench/run.py) — 基準版における Git 最終変更日: 2026-10-28.
- [`docs/benchmarks.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/benchmarks.md) — 基準版における Git 最終変更日: 2026-10-28.
- [`docs/rfcs/0027-production-component-interface.md`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/docs/rfcs/0027-production-component-interface.md) — 基準版における Git 最終変更日: 2026-10-28.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. gateway 1837行付近の JIT branch と既存 workload を読み、準備・compile・動的検証・invoke の境界を特定する。
2. 有限のスカラー binding 1件を10回呼ぶ。compile 回数を一時 probe/debugger で数えるか、静的経路から分かる回数を説明する。実行観測は1 process 60秒以内にする。
3. 計測可能なら独立 process 最大3回で、cold 準備、反復 call latency、compile 回数、保持メモリの観測を分け、build mode・環境を記す。既存 benchmark 台帳は編集しない。
4. immutable verified plan の最小再利用実験案として key、owner/寿命、失効、保持量上限、毎回必要な grants/provenance/loans/消費検査を示す。将来の構造的受入条件を N call で compile 1回とし、実現済みとは扱わない。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-component-jit-preparation-cost-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** cache 実装、LLVM 評価、全 benchmark、CI の絶対時間閾値、動的検証の省略。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/component-jit-preparation-cost.md` を英語の正本、`docs/investigations/component-jit-preparation-cost.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

compile 頻度と根拠の限界が明確で、限定した再利用実験案があり、裏付けのない性能改善を約束していない。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
