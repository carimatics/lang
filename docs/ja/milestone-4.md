<!-- translation-of: ../milestone-4.md -->

# Milestone 4 作業ボード

[English](../milestone-4.md)

更新: 2026-10-29 JST。M4 は active。開始 branch: `main`、upstream `origin/main`、remote `git@github.com:carimatics/lang.git`。`af9988f` で clean。作業 branch: `codex/m4-package-toolchain`、PR base: `carimatics/lang:main`。

**0 done · 1 doing · 36 todo · 106 remaining pt**

**次: M4-A01 — package 契約を採択し、厳密な manifest 検査を実行する。**

checkpoint 確認時刻 2026-10-29 01:09 JST: macOS AArch64 で workspace fmt、警告なしの全 target／全 feature Clippy が成功。現行実装の厳密 manifest／実 JSON CLI test は **6/6 成功、ignored なし**。全 `cargo test --workspace --all-targets --all-features` は実行中（Cargo PID 12333、実行 session 29818）で、直近の回収済み小計は **116 suite／1,695 成功／0 失敗**。native integration は完了し、残りの workspace suite を実行している。local 生成 log は `target/m4-baseline-test.log`（ignored）。まだ dependency を読んだり実行したりしない。A01 の残条件は全 workspace test／doc の完了、最新 board 監査への consistency 再実行、code／evidence commit。commit と PR はまだない。この checkpoint は delivery 完了ではない。

## 終了 gate

[roadmap](roadmap.md#milestone-4developer-toolchain)の元の採択 weight 63 pt を維持する。**検証済み 0/9 gate、0/63 gate weight。** 設計採択は実装 gate の完了を意味しない。P0／P1 はすべて scope 内に残し、延期はない。

| Gate | 優先度 | 成果 | Weight | 状態 |
| --- | --- | --- | ---: | --- |
| G1 | P0 | manifest／workspace、target、lock 互換性、CLI 診断の採択 | 5 | pending |
| G2 | P0 | 決定的依存解決、checksum、lock、offline replay | 8 | pending |
| G3 | P0 | package 対応 build／run／test と構造化診断 | 5 | pending |
| G4 | P0 | package／fmt／lint／doc の CLI 統合 | 5 | pending |
| G5 | P0 | LSP navigation／completion／semantic token | 8 | pending |
| G6 | P0 | 安全な rename と code action | 8 | pending |
| G7 | P0 | native／VM 混在 stack を持つ DAP adapter | 8 | pending |
| G8 | P1 | compile／runtime profile、allocation trace、rebuild 説明 | 8 | pending |
| G9 | P0 | 署名付き install／update／channel tool と復旧 | 8 | pending |

## Delivery

`pt` は残りの計画 effort で、経過時間ではない。各 1–3 pt 行に観測可能な成果を定める。実装 doing は最大一つ。Evidence は完了後の code commit hash だけを記録する。依存関係は順序を定めるが、gate の終了監査を代替しない。

| ID | State | pt | Needs | Gates | 観測可能な受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M4-A01 | doing | 3 | — | G1 | RFC 0034 を採択し、上限付き valid／invalid manifest を実 JSON CLI 診断で検査 | — |
| M4-A02R | todo | 3 | M4-A01 | G2,G3 | capability で制限した local dependency graph、決定的順序、循環／逸脱／version／target／resource 拒否 | — |
| M4-A02 | todo | 3 | M4-A02R | G2,G3 | 分離した package module root と不変 source access を統合、実際の宣言 dependency import を実行 | — |
| M4-A03 | todo | 3 | M4-A02 | G2,G3 | canonical checksum lock fixture を offline replay、従来 output を書き換えず変更 source を拒否 | — |
| M4-A04 | todo | 3 | M4-A03 | G2,G3 | package と local dependency を作成し package build／run／test と失敗 test fixture | — |
| M4-A05 | todo | 3 | M4-A04 | G2,G3 | workspace member 選択と、両 supported target の完全一致 target condition lock | — |
| M4-A06 | todo | 3 | M4-A05 | G2,G3 | 配置変更した locked package から native executable を二度 build、両 host で固定 input digest 比較 | — |
| M4-B01 | todo | 3 | M4-A04 | G4 | 決定的 package source list／archive と逸脱／変更 input の拒否 | — |
| M4-B02 | todo | 2 | M4-A04 | G4 | package fmt／check が CST comment を保持し、不正／古い edit を拒否 | — |
| M4-B03 | todo | 3 | M4-B02 | G4 | lint rule 採択、安定 code、package lint pass／fail／JSON fixture | — |
| M4-B04 | todo | 3 | M4-B03 | G4 | 解決済み link、source 位置、失敗時 output 保護を持つ package API doc 生成 | — |
| M4-C01 | todo | 3 | M4-A05 | G5,G6 | editor symbol／snapshot／edit 契約を採択し、実 package dependency を navigate | — |
| M4-C02 | todo | 3 | M4-C01 | G5 | package overlay、edit、watched-file refresh を跨ぐ definition／reference | — |
| M4-C03 | todo | 3 | M4-C02 | G5 | scope／privacy／type に従う completion、UTF-16 と stale result test | — |
| M4-C04 | todo | 3 | M4-C03 | G5 | Unicode、不正 source、document lifecycle に耐える semantic token と refresh | — |
| M4-C05 | todo | 3 | M4-C02 | G6 | 衝突／privacy 検査と version 付き atomic edit を持つ複数 file prepare／rename | — |
| M4-C06 | todo | 3 | M4-C05 | G6 | 採択 code action が検証済み fix を生成し stale／曖昧 edit を拒否 | — |
| M4-C07 | todo | 2 | M4-C04,M4-C06 | G5,G6 | editor package の end-to-end fixture と protocol／resource／refusal coverage | — |
| M4-D01 | todo | 3 | M4-A06 | G7 | DAP launch／control／stack／source 契約採択、VM source breakpoint で停止 | — |
| M4-D02 | todo | 3 | M4-D01 | G7 | pause／authority 上限付き VM stepping、frame、local、evaluation | — |
| M4-D03 | todo | 3 | M4-D02 | G7 | 両 tier-1 host で native debugger launch と source breakpoint | — |
| M4-D04 | todo | 3 | M4-D03 | G7 | native／VM 混在 stack fixture で call 順、span、失敗 cleanup 保持 | — |
| M4-D05 | todo | 3 | M4-D04 | G7 | DAP disconnect／restart／cancel／不正 input と両 host session 証拠 | — |
| M4-E01 | todo | 3 | M4-A06 | G8 | profile／trace schema 採択、上限付き compiler phase cost 出力 | — |
| M4-E02 | todo | 3 | M4-E01 | G8 | sampling／測定限界を明示する source 対応 VM／native runtime profile | — |
| M4-E03 | todo | 3 | M4-E02 | G8 | 実 allocator counter と整合し buffer 上限を持つ allocation／retirement trace | — |
| M4-E04 | todo | 3 | M4-E03 | G8 | rebuild 説明が source／dependency／target／toolchain input の変更を特定 | — |
| M4-F01 | todo | 3 | M4-A01 | G9 | installer trust root、署名 metadata、channel／version 選択、atomic 復旧設計採択 | — |
| M4-F02 | todo | 3 | M4-F01 | G9 | install 前に署名、期限／rollback／target／digest を検証、改ざん拒否 | — |
| M4-F03 | todo | 3 | M4-F02 | G9 | isolated root に検証済み tool を install、失敗／中断で active version 保持 | — |
| M4-F04 | todo | 3 | M4-F03 | G9 | atomic activation／復旧付き channel update／select／list／uninstall | — |
| M4-F05 | todo | 3 | M4-F04 | G9 | 両 host で署名 release／distribution fixture を install／update | — |
| M4-Z01 | todo | 3 | M4-B01,M4-B04,M4-C07,M4-D05,M4-E04,M4-F05 | G1,G2,G3,G4,G5,G6,G7,G8,G9 | macOS の新規開発者 install／create／editor／test／debug／再現可能 binary journey | — |
| M4-Z02 | todo | 3 | M4-Z01 | G1,G2,G3,G4,G5,G6,G7,G8,G9 | Windows の同一 journey と全 baseline、差異の原因解決 | — |
| M4-Z03 | todo | 2 | M4-Z02 | G1,G2,G3,G4,G5,G6,G7,G8,G9 | 全 roadmap 終了条件を最新 code／両 host 証拠／限界に照らして監査 | — |
| M4-Z04 | todo | 2 | M4-Z03 | G1,G2,G3,G4,G5,G6,G7,G8,G9 | 実行コマンドと最終照合を持つ日英 M4 capability guide 公開 | — |
| M4-Z05 | todo | 2 | M4-Z04 | G1,G2,G3,G4,G5,G6,G7,G8,G9 | 最新 head CI、PR merge、開始 branch 同期、clean な最終完了監査 | — |

## 見積照合

有効化監査、2026-10-29 JST: 観測可能な delivery 36 件、baseline 103 pt。下から積み上げた暫定見積であり、従来の gate weight 63 pt は変更しない。未実装行の実際の受入 task が明確になったら再監査する。前提作業と scope 増大は実装前に公開し、機械的に同じ見積を維持しない。完了した M2／M3 履歴は閉じたまま保持する。

| Baseline remaining | Net added/re-estimated | Retired on completion | Remaining |
| ---: | ---: | ---: | ---: |
| 103 pt | +3 pt | −0 pt | 106 pt |

**103 + 3 − 0 = 106 pt**

名前空間の前提監査、2026-10-29 JST: 既存 `ModuleGraph::root()` は常に `ModuleId(0)`。path provider だけでは依存ごとの package-root 名前空間や共有 dependency identity の安全な再利用を提供できない。元の A02 を、許可 graph（新規 A02R、3 pt）と名前空間／source 実行の統合（task list を見直した A02、残り 3 pt）へ分割する。純追加 +3 pt、全 37 delivery。A01 と A02R は前提二つとなるため、水平基盤を増やす前に A02 で次の source 実行 fixture を示す。この監査による gate 完了や延期はない。

## Delivery note と roadmap review

- A01 作業中: [RFC 0034](rfcs/0034-package-workspaces-and-cli-diagnostics.md) が採択するのは最初の package／tooling profile のみ。厳密な manifest 検査が実行可能な実験である。この checkpoint で source dependency、offline lock、editor、debugger、installer の gate 完了は主張しない。
- 次の縦の slice: A02 で実 dependency import を実行、A03 で checksum／offline 失敗経路、A04 で create／build／run／test を公開する。新しい source 実行 fixture のない前提 delivery が二つ続いたら順序を再検討する。
- roadmap 監査: P1 profiling を含む元の九つの gate は、新規開発者 journey に引き続き必要。M4 を有効化したので roadmap から live board を参照する。署名 installer 検証は install／download より先に行う。registry distribution は M5 に保ち、暗黙に供給したり削除したりしない。
- 出典: repository roadmap／architecture／RFC を 2026-10-29 JST に確認。source file に埋込み更新日はない。RFC 0034 更新日 2026-10-29。checkpoint は有限の local 証拠で、両 host／milestone 全体の主張ではない。
