<!-- translation-language: ja; canonical-language: en -->

# 15 — shared・weak・arenaストレージ

作成・対象確認: 2026-10-30 JST。依頼種別: 所有者が許可したOSSの防御目的のセキュリティ監査。英語版を正本とし、この日本語版は同内容を表す。

## 目的と許可範囲

対象は `carimatics/lang` の以下の境界に限定する。欠陥の有無、影響条件、修正方針、最小の回帰検証を調べる。実装・RFCの変更、公開、第三者への連絡は依頼に含めない。架空データとlocal fixtureだけを用い、外部システム・本番データ・認証情報には触れない。この書面だけで着手でき、別の調査結果は必要ない。

固定対象コミット: `067e1ee200d903eb97663d6157024e63f538782c`。M3完了後のコミット済みM4実装を含む。このコミット以後の変更や、別セッションの未コミット変更について安全性を主張しない。対象objectがなければ通常の権限規則に従ってauthorized remoteから取得するか、不足前提として報告し、黙ってHEADへ切り替えない。

## この単位に分ける理由

共有所有権と循環graphでは線形owner以外の同一性・寿命検査が必要だから。

## 対象revisionの入力

- `crates/runtime/src/shared.rs`
- `crates/runtime/src/shared/`
- `crates/runtime/src/arena.rs`
- `crates/runtime/src/arena/`
- `crates/runtime/src/inspection.rs`
- `tests/arena_calls.rs`
- `crates/runtime/tests/arena.rs`
- `crates/runtime/tests/managed_arena.rs`
- `crates/runtime/tests/shared.rs`
- `crates/runtime/tests/shared_storage.rs`
- `crates/runtime/tests/managed_shared.rs`
- `crates/runtime/tests/inspection.rs`

関連契約（英語正本）:

- `docs/rfcs/0004-owned-values-and-cyclic-graphs.md`
- `docs/rfcs/0007-source-ownership-and-core-values.md`
- `docs/rfcs/0008-cleanup-allocators-and-c-boundaries.md`
- `docs/rfcs/0016-indirect-repl-previews.md`

## 隔離した作業環境

開始時に日時を確認し、適用されるAGENTS.mdとその継続指示を読む。元checkoutのbranch・remote/upstream・変更状態を記録する。下のPOSIX shell例はrepository checkoutから実行する。WindowsではOSの一時領域に一意なdirectoryを作る同等手順を使う。fixtureと報告はsourceの外に置く。

```sh
audit_root=$(mktemp -d "${TMPDIR:-/tmp}/lang-review-15.XXXXXX")
git worktree add --detach "$audit_root/source" 067e1ee200d903eb97663d6157024e63f538782c
export CARGO_TARGET_DIR="$audit_root/target"
cd "$audit_root/source"
```

自分のworktreeとtargetだけを使用し、他のcheckout・branch・index・個人設定は編集しない。managed fileを変更せず、追加の検証harnessは `$audit_root/fixtures/` に置く。cleanupする場合も自分の一時物だけに限定し、最終報告が読める状態を保つ。test並列度はCPU・memory・diskの余力に合わせる。pathの隔離はhost資源が無制限であることを意味しない。

## 調査項目

関連RFCの英語正本を先に読み、期待する不変条件を記録してから入口から副作用までcodeを追う。必要な呼出し先だけ範囲を広げ、対象外の問題は根拠付き候補として記録する。設計レビューにはrepositoryのlanguage-design-rfc手順を適用するが、この依頼で設計や実装を変更しない。

1. strong/weak count、世代枯渇、stale identity、最後のrelease後のupgradeを確認する。
2. 別arenaへのstoreと入れ子所有権を追跡し、brand検査、ちょうど一度のretirement、循環走査の上限を確認する。
3. 小さな循環graphと確保/drop失敗を使い、期限切れownerやpreview経由でlive resourceへアクセスできないことを確認する。

## 対象を絞った検証

以下は対象worktreeで実行する最初の検証であり、成功だけで安全性を結論しない。各確認項目を既存testへ対応付け、欠けたものだけ小さなlocal harnessで補う。生成入力・反復回数・memory・subprocess時間を有限に制限し、外部timeoutと終了確認を付ける。testがゼロ件、platform条件で省略、依存やtool不足の場合は未検証として記録する。

```sh
cargo test --locked -p lang-runtime --test shared --test shared_storage --test managed_shared --test arena --test managed_arena --test inspection
```

tool・network・承認の規則を守る。自動安全性チェックで拒否された操作は停止し、表示理由と未確認範囲を記録する。目的を偽装して再試行せず、独立に許可された読取り・分析だけを続ける。

## 成果物と完了条件

1. `$audit_root/report.md` に日本語の報告書を作り、対象commit・OS/architecture・toolchain・確認日時を記録する。
2. 調査項目ごとに、確認したcode/契約、実行commandと結果、確認済み欠陥／今回問題なし／未検証を表にする。test名・件数・timeout・省略条件を明記する。
3. 欠陥ごとにfileと行、期待契約、必要な攻撃者権限、到達経路、限定した再現結果、影響、重大度の理由、修正案、回帰test条件を示す。推測・設計懸念・明示されたtrusted制約は欠陥と分ける。
4. 根拠の出典と更新日を付ける。codeは対象commitの`git log -1 --format=%cI -- PATH`等で最終変更日を確認する。外部資料は一次情報を使い、更新日がなければ未記載と確認日を記す。
5. 該当する設計レビュー結論を既存lifecycleに従って記し、未解決事項を列挙する。全項目を判定し、`git status --short`でsourceに監査変更がないことを確認して終了する。調査終了は欠陥修正やrepository全体の安全性保証を意味しない。
6. 最終応答には報告書と補助fixtureのpath、重要な結論、残る未確認条件を簡潔に示す。
