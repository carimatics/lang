<!-- language: ja; status: investigation-request; updated: 2026-10-30 JST -->
# 調査依頼: SDK build identity と Cargo 実効設定の調査

作成日: 2026-10-30 JST。実施内容は、この1件の調査と報告書の作成です。

## 問いと分割根拠

SDK fingerprint は Cargo config・package profile override を含め、実効的な debug assertions と overflow checks の違いを識別するか。

**この単位で完結する理由:** compiler 設定2つと fingerprint producer 1つの入力識別を調べる有限の単位であり、SDK 配布全体の実験は必要ない。

## 引継ぎ観測と確認対象

2026-10-30 のレビューでは、他の入力を揃えて設定だけ変えると実際の compiled build script が同じ seed を返した。SDK 2組の混在や ABI 破壊までは実証していない。DEBUG は debuginfo の情報であり debug assertions の代用ではない。

これは 2026-10-30 JST の品質レビューから引き継いだ観測であり、本依頼書の作成時に再実行した結果ではありません。観測の基準 commit は `df55f6ebcf33edb824b7a96e878ab9da5540228d` です。依頼を実行する commit を最初に記録し、対象箇所に差分があれば、その差分だけを確認して再現済み・解消済み・未確定を区別してください。元の一時報告書や probe は必要ありません。

- [`crates/codegen-clif/build.rs`](https://github.com/carimatics/lang/blob/df55f6ebcf33edb824b7a96e878ab9da5540228d/crates/codegen-clif/build.rs) — 基準版における Git 最終変更日: 2026-10-20.

上記の日付は Git のファイル変更日で、文書自体の公開・更新日とは別です。実施時に必要となった追加資料は公開・更新日を記録し、不明なら不明と記してください。

## 調査手順

1. build.rs 58行付近の hash 入力と Cargo 側の供給元を列挙し、環境変数の値と実際の rustc 設定を区別する。
2. 専用 target を使用し、必要なら小さな compile/config 観測で実効値を確かめる。他の入力を固定した2 boolean の最大4組合せについて、実効設定・fingerprint 入力・出力を記録する。
3. profile 名を変えずに Cargo config や package override が実効値を変える経路を静的に調べる。build-script の再実行だけならその範囲を明示し、完全な producer/consumer 実験としない。
4. 許容 build profile の明示的固定と、必要な実効入力を含む version 付き fingerprint の2案を比較する。互換性・再現性の論点を記録し、新 ABI 契約は決定しない。

## 実行範囲と隔離

- リポジトリの AGENTS.md を守り、開始時の branch/upstream/変更を記録してください。既存の checkout は操作せず、記録した基点から専用の `codex/investigate-sdk-build-identity-<unique>` branch と別 worktree を作成してください。
- probe、ログ、TMPDIR、CARGO_TARGET_DIR はこの調査専用の一時領域に置いてください。他セッションの build 出力、port、FIFO、corpus を共有しないでください。正当な既存 fixture を再利用し、必要な小さい experiment だけを実行してください。
- 最初の10分で対象経路と最小確認を特定してください。対象外の問題は報告の制約に一文で残し、追跡しません。実験上限、tooling 不足、再現不能に達したら、事実と未確認点を報告して終了し、範囲を拡大しません。
- **対象外:** SDK 全体の混在試験、秘密情報を含む個人設定の調査、ABI layout 変更、release pipeline 全体の再設計、修正。
- Git 管理対象への変更は、下記の調査報告書2ファイルに限定します。実装・受け入れ済み設計・共有進捗は変更しません。文書の検証・commit・PR はリポジトリの手順に従い、既存 checkout の branch 切り替えや同期を行わないでください。

## 成果物と完了条件

`docs/investigations/sdk-build-identity.md` を英語の正本、`docs/investigations/sdk-build-identity.ja.md` を同内容の日本語版として作成してください。それぞれに次を簡潔に記載します。

- 調査日時・timezone・対象 commit・環境、今回確認した結論。
- 問いに対応する根拠表、path/symbol/行番号、最小コマンド、期待結果と観測結果。観測・推論・提案・未確認を区別する。
- この対象だけの改善案または問題なしの根拠、回帰確認条件、互換性上の論点、実行制約。

最大4設定の比較表、実効値の取得根拠、限定した識別方針案があり、観測した衝突と未検証の実行上の影響を区別している。 再現できない場合も、試した範囲・阻害条件・未確定の理由が具体的なら調査報告として完了できます。元のレビューの全面再調査や修正実装は完了条件ではありません。新しい文書を保存した後は `cargo test --locked --test repository_consistency` と `git diff --check` を実行し、結果を記録してください。
