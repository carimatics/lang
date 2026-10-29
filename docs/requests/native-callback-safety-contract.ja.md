<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: native callback を渡す safe Rust API 境界の調査

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

safe Rust から raw `Method` を構築・dispatch するとき、callback の呼び出し側義務が型/API に表れているか。表れていなければ、どの入口で表すべきか。

**この単位で完結する理由:** Rust の構築から dispatch までの境界1つを対象にするため、C layout や component 資源計数と独立して判断できる。

## 引継ぎ観測と確認対象

2026-10-30 のレビューでは、`#[forbid(unsafe_code)]` の利用側から null user pointer を safe な `Invoker::invoke` に渡せた。callback は null 検出だけを行い、不正 pointer の逆参照は実行していない。任意型や `()` にも実装できる `CodeOwner` 自体は pointer 有効性の証明ではなかった。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`crates/runtime/src/component/native.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/runtime/src/component/native.rs) — 基準版における Git 最終変更日: 2026-10-26.
- [`crates/runtime/src/component/execution.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/runtime/src/component/execution.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`crates/codegen-clif/src/component.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/codegen-clif/src/component.rs) — 基準版における Git 最終変更日: 2026-10-27.
- [`crates/runtime/src/c_abi/context.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/runtime/src/c_abi/context.rs) — 基準版における Git 最終変更日: 2026-10-20.
- [`tests/component_native_registration.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/tests/component_native_registration.rs) — 基準版における Git 最終変更日: 2026-10-26.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. Method の公開フィールド、Invoker::invoke、Clif 実装、登録入口の可視性、owner 保持を追い、user/code pointer の寿命・thread・再入・unwind の呼び出し側義務を列挙する。
2. まず `#![forbid(unsafe_code)]` の一時 downstream module を compile のみで確認する。callback 実装は別 fixture に置き、user が null かを記録するだけにする。
3. 動的確認が必要なら、この逆参照しない callback だけを5秒上限の子プロセスで使う。有効な pointer が必要な callback に不正 pointer を渡して実行しない。
4. 局所的な API 案を最大2つ比較する。Safety 文書を持つ unsafe 入口、または検証済み登録の背後で raw dispatch を private/sealed にする案とし、Rust 互換性と C layout を分ける。
5. compile-fail 回帰条件と、維持すべき owner/retirement/reentry の既存挙動を示す。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-native-callback-safety-contract-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** メモリ破壊の実証、外部の任意 native library、修正実装、C v1 全体の再設計。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/native-callback-safety-contract.md` を英語の正本、`docs/investigations/native-callback-safety-contract.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

safe な構築経路の可否、欠けている義務、限定した API・回帰確認案が示され、未定義動作を実行せず結論を出している。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
