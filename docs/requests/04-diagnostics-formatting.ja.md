<!-- translation-language: ja; canonical-language: en -->

# 04 — 診断表示とソース整形

作成・対象確認: 2026-10-30 JST。依頼種別: 所有者が許可したOSSの防御目的のセキュリティ監査。英語版を正本とし、この日本語版は同内容を表す。

## 目的と許可範囲

対象は `carimatics/lang` の以下の境界に限定する。欠陥の有無、影響条件、修正方針、最小の回帰検証を調べる。実装・RFCの変更、公開、第三者への連絡は依頼に含めない。架空データとlocal fixtureだけを用い、外部システム・本番データ・認証情報には触れない。この書面だけで着手でき、別の調査結果は必要ない。

固定対象コミット: `067e1ee200d903eb97663d6157024e63f538782c`。M3完了後のコミット済みM4実装を含む。このコミット以後の変更や、別セッションの未コミット変更について安全性を主張しない。対象objectがなければ通常の権限規則に従ってauthorized remoteから取得するか、不足前提として報告し、黙ってHEADへ切り替えない。

## この単位に分ける理由

未検証テキストが端末・エディタ出力や整形後ソースへ反映される境界だから。

## 対象revisionの入力

- `crates/diagnostics/src/`
- `crates/syntax/src/format.rs`
- `src/bin/`
- `tests/formatter.rs`
- `crates/syntax/tests/formatting.rs`

関連契約（英語正本）:

- `docs/rfcs/0006-source-frontend-and-portable-programs.md`
- `docs/rfcs/0010-owned-repl-previews.md`
- `docs/rfcs/0016-indirect-repl-previews.md`
- `docs/rfcs/0036-checked-package-formatting.md`

## 隔離した作業環境

開始時に日時を確認し、適用されるAGENTS.mdとその継続指示を読む。元checkoutのbranch・remote/upstream・変更状態を記録する。下のPOSIX shell例はrepository checkoutから実行する。WindowsではOSの一時領域に一意なdirectoryを作る同等手順を使う。fixtureと報告はsourceの外に置く。

```sh
audit_root=$(mktemp -d "${TMPDIR:-/tmp}/lang-review-04.XXXXXX")
git worktree add --detach "$audit_root/source" 067e1ee200d903eb97663d6157024e63f538782c
export CARGO_TARGET_DIR="$audit_root/target"
cd "$audit_root/source"
```

自分のworktreeとtargetだけを使用し、他のcheckout・branch・index・個人設定は編集しない。managed fileを変更せず、追加の検証harnessは `$audit_root/fixtures/` に置く。cleanupする場合も自分の一時物だけに限定し、最終報告が読める状態を保つ。test並列度はCPU・memory・diskの余力に合わせる。pathの隔離はhost資源が無制限であることを意味しない。

## 調査項目

関連RFCの英語正本を先に読み、期待する不変条件を記録してから入口から副作用までcodeを追う。必要な呼出し先だけ範囲を広げ、対象外の問題は根拠付き候補として記録する。設計レビューにはrepositoryのlanguage-design-rfc手順を適用するが、この依頼で設計や実装を変更しない。

1. 診断を端末・JSON・エディタへ出す前のsource/span検証、出力サイズ計数、制御文字、秘匿境界を確認する。
2. 整形がコメント・リテラルの意味・不正入力を保持し、対応入力で冪等であり、実行内容を黙って削除しないことを確認する。
3. 整形・診断CLIは入出力の信頼境界に限って追跡する。一時出力先を使い、失敗時に元のbyte列が変わらないことを確認する。

## 対象を絞った検証

以下は対象worktreeで実行する最初の検証であり、成功だけで安全性を結論しない。各確認項目を既存testへ対応付け、欠けたものだけ小さなlocal harnessで補う。生成入力・反復回数・memory・subprocess時間を有限に制限し、外部timeoutと終了確認を付ける。testがゼロ件、platform条件で省略、依存やtool不足の場合は未検証として記録する。

```sh
cargo test --locked -p lang-syntax --test formatting
cargo test --locked --test formatter
```

tool・network・承認の規則を守る。自動安全性チェックで拒否された操作は停止し、表示理由と未確認範囲を記録する。目的を偽装して再試行せず、独立に許可された読取り・分析だけを続ける。

## 成果物と完了条件

1. `$audit_root/report.md` に日本語の報告書を作り、対象commit・OS/architecture・toolchain・確認日時を記録する。
2. 調査項目ごとに、確認したcode/契約、実行commandと結果、確認済み欠陥／今回問題なし／未検証を表にする。test名・件数・timeout・省略条件を明記する。
3. 欠陥ごとにfileと行、期待契約、必要な攻撃者権限、到達経路、限定した再現結果、影響、重大度の理由、修正案、回帰test条件を示す。推測・設計懸念・明示されたtrusted制約は欠陥と分ける。
4. 根拠の出典と更新日を付ける。codeは対象commitの`git log -1 --format=%cI -- PATH`等で最終変更日を確認する。外部資料は一次情報を使い、更新日がなければ未記載と確認日を記す。
5. 該当する設計レビュー結論を既存lifecycleに従って記し、未解決事項を列挙する。全項目を判定し、`git status --short`でsourceに監査変更がないことを確認して終了する。調査終了は欠陥修正やrepository全体の安全性保証を意味しない。
6. 最終応答には報告書と補助fixtureのpath、重要な結論、残る未確認条件を簡潔に示す。
