<!-- translation-of: ../milestone-4.md -->

# Milestone 4 作業ボード

[English](../milestone-4.md)

更新: 2026-10-29 JST。M4 は active。Goal 開始は clean main／origin/main `af9988f`、remote `git@github.com:carimatics/lang.git`。PR #9–#16 は merge 済み。tooling PR #16 は `49a0fe8` として merge し、main を fast-forward して clean、ahead／behind 0/0 を確認した。現在の `codex/m4-editor-navigation` はその main から作成し、PR base は `carimatics/lang:main`。upstream は origin/codex/m4-editor-navigation、[PR #17](https://github.com/carimatics/lang/pull/17) で、既存 working-tree／user 変更はなかった。C01–C04 の navigation／references／completion／token を一貫した成果として扱う。

**16 done · 1 doing · 23 todo · 68 remaining pt**

C03 実行 checkpoint、2026-10-29 18:35 JST: 重点 25 件、fmt、警告なし全 Clippy、source 固定の全 baseline（**166 suite／2,445 成功／0 失敗**）と doc が成功。completion 5 fixture は公開／空 namespace、解決済み import／enum case、型 context、宣言 record field、lexical scope、元 UTF-16 edit の適用、古い version、部分 prefix を返さない件数／出力上限拒否を検証した。一時的な通常 parser／binder overlay は I/O を行わず、probe fact を公開しない。未対応 receiver の field は推測しない。残る条件は code 証拠と最新 head CI／PR lifecycle。C03 は review 済み 3 pt の doing に保ち、見積追加はない。C02 head `569260b` は CI `40845584529`（18 job 成功、documentation-only は意図した skip）と debugger `40845584137`（3/3）に成功した。

**次: M4-C03 — scope／privacy／type 対応 completion と UTF-16／stale 検証（doing、3 pt）。** C02 source `569260b` は重点 57 件、逆 UTF-16 unit、全 baseline（166 suite／2,440 test）、fmt／Clippy／doc に成功した。authority 文面の拒否回帰を直して全 baseline を再実行した。最終 3 pt を消化し、追加 +11、消化 46、残り 68 pt。C03 は通常 parser／binder を使う不完全入力の実験、scope／型候補の限定、上限付き text edit／protocol fixture を既存 3 pt で進め、新 delivery／見積を追加しない。最新 head CI と editor branch の PR lifecycle は継続する。

A04 source `cf6cf5c` は fmt、全 Clippy／doc、checked opt-level 1 全 test（**158 suite／2,384 成功／0 失敗**）、重点 75 test に成功。最終 head `75a2041` は [全 CI](https://github.com/carimatics/lang/actions/runs/40810065213)（全 19 job、18 成功、意図した文書専用 skip）と [debugger 実行](https://github.com/carimatics/lang/actions/runs/40810065278)（3/3）に成功した。[PR #13](https://github.com/carimatics/lang/pull/13) は `58527c2` として merge、main を fast-forward して clean・同期済み、ahead／behind 0/0。workspace branch 作成と現在の PR／upstream は上記に記録した。既存の user 変更はなかった。

A04B source `9cf5f7c` は [全 CI](https://github.com/carimatics/lang/actions/runs/40800668325)（全 19 job、18 成功、意図した文書専用 skip）と [debugger 実行](https://github.com/carimatics/lang/actions/runs/40800668297)（3/3）に成功。command fixture は macOS 12/12、Windows 11/11。独立した checked opt-level 1 local 全 baseline は **158 suite／2,376 成功／0 失敗** で完了し、fmt／Clippy／doc も成功。[PR #12](https://github.com/carimatics/lang/pull/12) は `2cd06e2` として merge し、local main tip は ahead／behind 0/0 で一致する。以前の command baseline／PR lifecycle メモはこれで完了。

A03 の検証は完了。修正 source `117a2ab`、最終 head `6e11460` で、local fmt、警告なし全 Clippy、assertion／overflow check を有効にした checked opt-level 1 全 workspace test（**157 suite／2,364 成功／0 失敗**）、doc が成功した。最終 head の [全 CI](https://github.com/carimatics/lang/actions/runs/40788344128) は期待した 19 job（18 成功、意図した文書専用 skip）、[debugger 実行](https://github.com/carimatics/lang/actions/runs/40788344085) は 3/3 成功。root／fuzz fmt、全 fuzz-bin Clippy、instrumentation なし metadata fuzz smoke 10,000 回も成功し、coverage／sanitizer campaign は主張しない。[PR #11](https://github.com/carimatics/lang/pull/11) は `3f06a70` として merge、local `main` は clean・同期済み、ahead／behind 0/0。以下の過去の未完了 API 検証メモをこの記録で更新する。

A01 の修正済み source は `9e5b0ce`。2026-10-29 02:24 JST の local 検証で fmt、警告なし Clippy、assertion／overflow check を有効にした checked opt-level 1 の全 test（**154 suite／2,330 成功／0 失敗**）、doc が成功。manifest／CLI は 7/7、consistency は 6/6 成功し、従来の test 名／結果は未変更。最新 head `f3d0a52` の [全 CI](https://github.com/carimatics/lang/actions/runs/40736268875)（18 成功、意図した文書専用 skip）と [native debugger 実行](https://github.com/carimatics/lang/actions/runs/40736267899)（全 3 環境）が成功した。[PR #9](https://github.com/carimatics/lang/pull/9) は 02:43 JST に `42e6045` として merge。local `main` を fast-forward し、clean、ahead／behind 0/0 を確認した。

A02R／A02S／A02 は修正 source `b81633e` で実装・検証済み（2026-10-29 JST）。宣言した実 dependency が portable wire replay／VM とプログラム全体の native JIT で 42 を返す。分離 root、共有 nominal type、元の可視性 span、identity 重複、停止しない FIFO／逸脱拒否、編集／削除／rename 後の不変入力が成功した。取得は成功／失敗のどちらでも全 directory handle を閉じ、project rename 成功 test は Windows と macOS の両方で通る。

local 検証: source 13/13、graph 9/9、manifest／CLI 7/7、consistency 6/6。fmt、警告なしの全 Clippy、assertion／overflow check を有効にした checked opt-level 1 全 workspace test（**156 suite／2,352 成功／0 失敗**）、doc が成功した。独立 fuzz workspace も、Cargo が `4611c86` で依存 lock を再生成した後、`cargo check --manifest-path fuzz/Cargo.toml --locked --offline --bins` が成功した。

最終 head `c0784fc` の [全 CI](https://github.com/carimatics/lang/actions/runs/40779348154) は期待した全 19 job が存在し、18 成功、source 変更のため文書専用 job が意図した skip。[native debugger 実行](https://github.com/carimatics/lang/actions/runs/40779348160)も全 3 環境で成功した。[PR #10](https://github.com/carimatics/lang/pull/10) は 07:34 JST に `260e67a` として merge。local `main` を fast-forward し、clean、ahead／behind 0/0 を確認した。fuzz fmt と警告なし locked／offline Clippy も成功。G2–G9 は未完了。

A01 入力修正は Unix の nonblocking open と開いた handle の metadata を使い、FIFO／device／directory を `PKG0005`／exit 1 で拒否する。計画 point や完了 delivery は増やさず、全検証と merge は上記のとおり完了した。

## 終了 gate

[roadmap](roadmap.md#milestone-4developer-toolchain)の元の採択 weight 63 pt を維持する。**検証済み 4/9 gate、23/63 gate weight（36.5%）。** 設計採択は実装 gate の完了を意味しない。P0／P1 はすべて scope 内に残し、延期はない。

| Gate | 優先度 | 成果 | Weight | 状態 |
| --- | --- | --- | ---: | --- |
| G1 | P0 | manifest／workspace、target、lock 互換性、CLI 診断の採択 | 5 | done |
| G2 | P0 | 決定的依存解決、checksum、lock、offline replay | 8 | done |
| G3 | P0 | package 対応 build／run／test と構造化診断 | 5 | done |
| G4 | P0 | package／fmt／lint／doc の CLI 統合 | 5 | done |
| G5 | P0 | LSP navigation／completion／semantic token | 8 | pending |
| G6 | P0 | 安全な rename と code action | 8 | pending |
| G7 | P0 | native／VM 混在 stack を持つ DAP adapter | 8 | pending |
| G8 | P1 | compile／runtime profile、allocation trace、rebuild 説明 | 8 | pending |
| G9 | P0 | 署名付き install／update／channel tool と復旧 | 8 | pending |

## Delivery

`pt` は残りの計画 effort で、経過時間ではない。各 1–3 pt 行に観測可能な成果を定める。実装 doing は最大一つ。Evidence は完了後の code commit hash だけを記録する。依存関係は順序を定めるが、gate の終了監査を代替しない。

| ID | State | pt | Needs | Gates | 観測可能な受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M4-A01 | done | 0 | — | G1 | RFC 0034 を採択し、上限付き valid／invalid manifest を実 JSON CLI 診断で検査 | `9e5b0ce` |
| M4-A02R | done | 0 | M4-A01 | G2,G3 | capability で制限した local dependency graph、決定的順序、循環／逸脱／version／target／resource 拒否 | `aae5242` |
| M4-A02S | done | 0 | M4-A02R | G2,G3 | 上限付き source input を一度固定し directory handle を解放、両 host で編集／rename 後の実行を保持 | `b81633e` |
| M4-A02 | done | 0 | M4-A02S | G2,G3 | 分離した package module root と不変 source access を統合、実際の宣言 dependency import を実行 | `b81633e` |
| M4-A03 | done | 0 | M4-A02 | G2,G3 | canonical checksum lock fixture を offline replay、従来 output を書き換えず変更 source を拒否 | `117a2ab` |
| M4-A04B | done | 0 | M4-A03 | G2,G3 | package／local dependency 作成、実 package build／run、locked／offline replay と失敗時保護 generation | `9cf5f7c` |
| M4-A04 | done | 0 | M4-A04B | G2,G3 | 宣言 test-entry scope、共有 VM profile、失敗 test／source／JSON 診断 fixture | `cf6cf5c` |
| M4-A05W | done | 0 | M4-A04 | G2,G3 | 固定 workspace catalogue／schema 2 整合性と、実 sibling build／run／test 実行 | `183d393` |
| M4-A05 | done | 0 | M4-A05W | G2,G3 | workspace member 選択と、両 supported target の完全一致 target condition lock | `fc032f5` |
| M4-A06 | done | 0 | M4-A05 | G2,G3 | 配置変更した locked package から native executable を二度 build、両 host で固定 input digest 比較 | `d66c764` |
| M4-B01 | done | 0 | M4-A04 | G4 | 決定的 package source list／archive と逸脱／変更 input の拒否 | `b150762` |
| M4-B02 | done | 0 | M4-A04 | G4 | package fmt／check が CST comment を保持し、不正／古い edit を拒否 | `5010d45` |
| M4-B03 | done | 0 | M4-B02 | G4 | lint rule 採択、安定 code、package lint pass／fail／JSON fixture | `99fabc4` |
| M4-B04 | done | 0 | M4-B03 | G4 | 解決済み link、source 位置、失敗時 output 保護を持つ package API doc 生成 | `4f62c78` |
| M4-C01 | done | 0 | M4-A05 | G5,G6 | editor symbol／snapshot／edit 契約を採択し、実 package dependency を navigate | `cd37106` |
| M4-C02 | done | 0 | M4-C01 | G5 | package overlay、edit、watched-file refresh を跨ぐ definition／reference | `569260b` |
| M4-C03 | doing | 3 | M4-C02 | G5 | scope／privacy／type に従う completion、UTF-16 と stale result test | — |
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
| 103 pt | +11 pt | −46 pt | 68 pt |

**103 + 11 − 46 = 68 pt**

名前空間の前提監査、2026-10-29 JST: 既存 `ModuleGraph::root()` は常に `ModuleId(0)`。path provider だけでは依存ごとの package-root 名前空間や共有 dependency identity の安全な再利用を提供できない。元の A02 を、許可 graph（新規 A02R、3 pt）と名前空間／source 実行の統合（task list を見直した A02、残り 3 pt）へ分割する。純追加 +3 pt、全 37 delivery。A01 と A02R は前提二つとなるため、水平基盤を増やす前に A02 で次の source 実行 fixture を示す。この監査による gate 完了や延期はない。

snapshot lifecycle 監査、2026-10-29 JST: Windows CI は dependency fixture の実行に成功したが、生存 directory handle により project rename を拒否した。入力 snapshot は取得 handle の寿命から独立させる必要がある。上限付き inventory と handle 解放の A02S（3 pt）を追加し、rename 成功 test を保持する。A02 は固定 provider 統合と両 host 検証が残る（3 pt）ため、doing を A02S に移す。A03 はこの inventory を利用するので、canonical serialization、checksum、replay、atomic publication の task を見直し、残見積を 3 から 2 pt に減らす。純追加 +2 pt、累計 +5 pt、全 38 delivery。dependency source fixture はすでに両 mode で実行済みであり、実行証拠に続く修正である。gate 完了や延期はない。

CLI 成果監査、2026-10-29 JST、実装前: 採択した transaction profile で実行可能な成果二つが明確になった。新規 A04B（3 pt）は scaffold／既存 file 互換、全 profile の build／run、不変 generation 公開を、42 を返す実 dependency で示す。A04 は残る test-entry 選択、複数 profile の戻り値検査、構造化した失敗／span fixture を見直して 3 pt とする。公開処理や共有 engine 作業を元の見積に隠さず、元の複合 CLI 成果を縦の delivery に分割する。純追加 +3 pt、累計 +8 pt、全 39 delivery。これと別に、修正 source の両 host CI が API 成果を検証した A03 の最終 2 pt を消化する。CLI 二行は同じ一貫した command 実装 branch とし、lock wire API を review 済み依存とする。独立 local API baseline／PR lifecycle は source を固定した checkout で継続する。gate 完了や延期はない。

## Delivery note と roadmap review

B03 実行 checkpoint、2026-10-29 15:55 JST、現在 worktree: 重点 110 件、source 固定の全 baseline（**163 suite／2,419 成功／0 失敗**）、fmt、警告なし全 Clippy、doc が成功した。実 CLI 6 fixture が解決済み未読 binding、元 UTF-8 span、既存 warning、dependency／test 重複除去、拒否、main 不要／無実行解析、lock／artifact bytes 保持を検証し、library 境界は一意診断 65,536 件丁度を受理して次の一件を拒否した。formatter head `5010d45` の Windows core-1 job `123305175897` で slash 固定文字列比較の test 仮定を検出したため、同じ厳密 span 条件を保持して native Path 比較へ修正し、新 lint test にも適用した。修正後の path／formatter／lint／consistency 19 件と Clippy は local で成功。残る条件は code 証拠と修正最新 head の両 host CI／PR lifecycle。B03 は 3 pt の doing に保つ。

B02 実行 checkpoint、2026-10-29 15:44 JST、現在 worktree: formatter fixture 全 7 件と archive／command／consistency 回帰 29 件が成功。comment 保持、dependency／test 整形、旧 lock 保持、古い manifest／source 拒否、read-only file、staging 失敗時 cleanup、保持 root の rename、Unix permission 保持を実行確認した。fmt、警告なし全 Clippy、source 固定の全 test（**162 suite／2,412 成功／0 失敗**）と doc が成功。初回 baseline は RFC 0037 の索引追加前に compile した README snapshot を検出したが、検証を弱めず修正後の全実行に成功した。残る条件は code 証拠と一貫した tooling branch の最新 head 両 host CI。B02 は review 済み 3 pt の doing に保つ。

B01 実行 checkpoint、2026-10-29 15:03 JST、現在 worktree: archive 5/5、既存 command 18/18、manifest 7/7、consistency 6/6 に成功し、全 Clippy は警告なし。信頼した展開後に元 lock で実 sibling の run／test を replay した。独立した Python 標準 `tarfile` reader でも通常 file、固定 uid／gid／mtime／mode／owner metadata、descriptor schema 1 を確認した。root／fuzz Cargo lock は tar 0.4.46／filetime を Cargo が生成し、fuzz-bin check も成功。production source を固定した全 test（161 suite／2,405 成功／0 失敗）と doc が成功した。残る条件は code 証拠と両 host の最新 head CI／PR lifecycle。B01 は doing に保ち、見積追加はない。

A06 Windows 修正、2026-10-29 JST: head `fd11166` の [Windows native job](https://github.com/carimatics/lang/actions/runs/40820496903/job/123271717757) で実 native fixture が失敗。移動後 locked replay は `build.json` の変更を検出し、output を保持して拒否した。RFC 0020 で修正前に MSVC `/Brepro` の明示指定を採択し、cache replay 前に native package file／直接 link digest を比較して PE timestamp を記録する。timestamp／linker metadata は修正後の Windows test 成功まで仮説。重点 local 検査／全 baseline と最新 head CI を再実行する。A06 は review 済み 3 pt の doing に保ち、delivery 完了や新見積を主張しない。

A06 実行 checkpoint、2026-10-29 14:15 JST、source `d66c764`: fmt、警告なし全 Clippy、全 test（**160 suite／2,400 成功／0 失敗**）、doc と修正後の重点 14 件に成功（当初の重点検証は 51 件）。配置変更した二度の native build は executable 全体の digest が一致し、42 を返す。実 native CLI receipt、locked／offline replay、SDK／program／source 削除後の起動、stale／SDK／rustc 拒否、Unix 実行 permission の cache 検査が成功した。配置変更前後の元 package input も一致し、startup path と Mach-O OSO path に RFC 0020 の修正を要した。残る条件は実 Windows 再現性、最新 head CI と branch merge。この checkpoint では A06 や追加 exit gate を完了としない。

workspace scope 監査、2026-10-29 JST、code 前: schema 1 は実 package input だけを持ち、input の役割を流用せず workspace catalogue の root／未選択 manifest を固定できない。上の probe が不足する整合性 input を確認した。明示選択、固定 catalogue、schema 2 checksum／互換性、実 sibling 実行 fixture の A05W（3 pt）を追加する。既存 target filter 統合と両 target replay／拒否 fixture の残 task を見直し、A05 を 3 から 2 pt に再見積する。純追加 +2 pt、累計 +10 pt、全 40 delivery。これとは別に A04 の最終 3 pt は消化済み。両方の実行可能な成果を維持し、gate 完了や延期はない。

A04 完了: `cf6cf5c` が `_package`、通常の可視性／nominal identity、元 UTF-8 診断で固定した宣言 test root を実行する。manifest 順、fail-fast 時の保持、test 0 件 library、entry 再利用、entry 単位 fuel、ownership／task／isolated-service profile、実際の累積 64 MiB output 拒否が成功。上記の local 全 baseline と両 host CI／debugger 検査が成功した。実行 probe に基づき RFC／fixture の例を既存の加算と符号付き literal に修正した。見積追加なしで最終 3 pt を消化。workspace 選択と native package 再現性が残るので G2／G3 は未完了。A05 は scope／整合性監査から始める。

checkout 監査、2026-10-29 JST: 以前は A04B／A04 を同じ branch に置く計画だった。native baseline の source fingerprint を保持するため、command checkout を固定したまま、依存する test-entry 名前空間／実行の成果を既存の元 checkout へ分ける。二つの観測可能な成果の依存関係であり、delivery や前提を追加せず、見積は不変。CPU を使う build／test は順次実行する。A04B の最終 head の両 host CI が command 成果を証明したため、実装 doing は A04 のみ。独立した command baseline／PR lifecycle は実際の完了まで継続する。

A04B 完了: `9cf5f7c` が実 package 作成／dependency build／run、locked／offline replay、全三 VM profile、完全一致 generation 再利用、古い input／改変／resource／runtime 拒否、元 UTF-8 診断、file 明示形式との互換性を検証する。両 host CI と三つの debugger 環境が成功。Windows は最終 lock 失敗時の旧 output 保持と stage 除去、macOS は表示 path の置換後も保持 directory へ公開することを検証する。見積追加なしで最終 3 pt を消化。名前空間選択、結果／control 処理、output／失敗 fixture を見直し、A04 は 3 pt を維持する。採択済み `_package` edge と既存 engine で実装する。G2／G3 は workspace／native package 証拠が残るため未完了。

マージ前の A01 入力監査 checklist:

- [x] 通常ファイル以外の manifest 入力を待機せず拒否し、構造化 `PKG0005`／exit 1 を維持する。
- [x] 監視付きの実 FIFO CLI 回帰 test を追加し、重点 7 test と fmt／Clippy を通す。
- [x] 全 test／doc を完了し、修正 source を commit して証拠を更新する。
- [x] 最新 head の全 CI／debugger check を検証し、merge と base branch 同期を行う。

- A01 完了: [RFC 0034](rfcs/0034-package-workspaces-and-cli-diagnostics.md) が最初の package／tooling profile を採択した。`lang package check-manifest` が実行可能な実験であり、重複／未知／null／型／version／path／target／件数／byte 上限／通常ファイル以外の拒否と構造化 exit 1／2 を含む。必須 local baseline 四つが成功し、修正済み source evidence は `9e5b0ce`（初回 delivery `d1f2667`）。A01 の最終 3 pt を消化し、新規前提 A02R（+3 pt）と区別する。マージ前の入力修正による見積追加はない。完了は G1 のみで、source dependency、offline lock、editor、debugger、installer gate は未完了。
- A02R 完了: 不変の原 manifest／entry byte、整列した完全一致 local edge、diamond identity 共有、循環／名前／version／target／resource 拒否、256 package 境界が成功。監視付きの FIFO path 4 ケースも停止せず終了し、directory 専用 canonicalize と capability 相対 file admission が親／symlink 逸脱を拒否する。最終見積 3 pt を消化し、新規追加はない。次の実装は A02 の dependency source 実行で、その後 A03 checksum／offline replay、A04 create／build／run／test と続く。追加の水平前提は選択しない。
- A02S／A02 完了: `b81633e` が上限付き source inventory の固定、取得失敗 cache、handle 解放、不変 provider と分離 dependency 名前空間の統合を実装する。両 host の実行と project rename が成功した。各行の最終 3 pt（計 6 pt）を従来の追加と分けて消化し、見積追加はない。Windows が検出した未公開 identity import と保持 handle の欠陥は test を弱めず修正した。Cargo は独立 fuzz lock も同期した。初期の全実行は source 編集と重なり SDK build-ID check で失敗したが、上記の固定 source 最終 baseline は全 native-image test に成功し、その無効 checkpoint を置き換える。
- A03 契約 review: source hash だけでは、取得済みの二つの tree 間で directory alias を付け替えた変更を検出できない。RFC 0034 は整列した directory／entry／test binding を canonical lock record と package checksum に含め、binding 変更の拒否 fixture を定めた。lock 実装や既存 lock artifact がない段階で採択した修正であり、serialization、replay、publication task を見直した A03 は 2 pt を維持する。
- A03 API 完了: `117a2ab` が両 host の全 CI と三つの debugger 環境に成功。canonical fixture、独立 SHA-256／UTF-8 順、固定した配置変更後の dependency replay、変更／未使用／binding／compiler／target／不正／resource 拒否、物理 identity admission、失敗時保護 lock 公開を検証した。Windows が検出した fixture checkout の CRLF は明示 LF 属性で修正し、CRLF lock 拒否は厳密に保つ。A04B の +3 pt scope 監査とは別に最終 2 pt を消化する。独立した local 全 test／doc と最終文書 head の CI は branch merge 条件に残り、local 完了や PR merge はまだ主張しない。
- A04B 互換性 checkpoint、2026-10-29 JST: Windows CI と初回 local baseline が、source なし `run` の usage exit 2 と service-work help 3 形式という二つの古い期待値を検出した。RFC 0034 は既定 package manifest の選択と第 4 help 形式を意図して追加している。更新した検査は全ての不正な file 明示 exit 2 を維持し、manifest 不在の exit 1、42 を返す既定 manifest run、祖先検索の拒否を検証する。CLI／service-work／package-command／consistency は 53/53 成功。失敗 baseline は診断証拠として保持し、修正後の全 baseline と最新 head CI は引き続き必要。この修正による delivery や見積の追加はない。
- roadmap 監査: P1 profiling を含む元の九つの gate は、新規開発者 journey に引き続き必要。M4 を有効化したので roadmap から live board を参照する。署名 installer 検証は install／download より先に行う。registry distribution は M5 に保ち、暗黙に供給したり削除したりしない。
- 出典: repository roadmap／architecture／RFC を 2026-10-29 JST に確認。source file に埋込み更新日はない。RFC 0034 更新日 2026-10-29。checkpoint は有限の local 証拠で、両 host／milestone 全体の主張ではない。
