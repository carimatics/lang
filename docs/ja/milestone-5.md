<!-- canonical-language: en; japanese-translation: ja/milestone-5.md -->

# Milestone 5 delivery board

[English](../milestone-5.md)

2026-10-30 JST、`3b676c5`から開始。[ロードマップ](roadmap.md#milestone-5ecosystemとcompatibility)のM5全範囲（P1の2ゲートを含む）を追跡する。M4のinstallerとlocal packageは前提条件であり、registry完了を意味しない。

**2 done · 1 doing · 37 todo · 110 remaining pt**

**次: M5-B01 — 検証付きfixと旧仕様移行recipeを採択する。**

B01 checkpoint（2026-10-30 JST）: [RFC 0044](rfcs/0044-checked-source-migration.md)でpreview既定のfix、検証付き公開、明示的な旧scalar profileを採択。保持した旧`shared`・`weak` sourceは42を返し、手動alpha-rename後も現VMで42を返してnative出力を生成する。移行commandとnative実行journeyは未実装。残る受入条件は日英整合性検証とcommit。作業branch `codex/m5-checked-source-fixes`は`main`（`3b676c5`）から作成し、PR baseを`git@github.com:carimatics/lang.git`の`main`として記録。その後PR #29の前提commit `8a43f76`へfast-forwardした。checkoutは`/private/tmp/lang-m5-fixes`。元worktreeはcleanを保ち、同PRのCIを待つ。

A02 checkpoint（2026-10-30 JST確認）: A01・A02でRFC 0043を実装した。code `a473cc0`で実互換性CLI、focused test 45件、fmt、Clippy、workspace test 2,522件、分離targetのrustdocが成功。両host CI `41064851495`とnative-debug CI `41064851345`も同codeで成功した。[PR #29](https://github.com/carimatics/lang/pull/29)のbaseは`main`。この文書記録の公開には最新head checkとremote mergeが残る。開始branchはcleanな`main`、追跡先`origin/main`、remoteは`git@github.com:carimatics/lang.git`、作業branchは`codex/m5-editions-and-migration`。ユーザー変更なし。M5は継続中で9ゲートがopen。

## Exit gates

| Gate | 必須成果 | State |
| --- | --- | --- |
| G1 | edition・非推奨方針・互換性表 | done |
| G2 | edition移行と機械適用可能な修正 | open |
| G3 | upload前のregistry脅威モデル採択 | open |
| G4 | storage・index・download・immutable version | open |
| G5 | 認証publish・yank・owner操作とaudit | open |
| G6 | 署名release manifestとpackage provenance | open |
| G7 | portable WebAssembly component plugin target | open |
| G8 | Wasm sandboxとcapability適合 | open |
| G9 | 文書hostingとmetadata検索（P1） | open |
| G10 | 公開LLVM release backend評価（P1） | open |

ゲート完了: **1/10; 5/65 historical gate weight**。以下は残作業見積りであり、実時間やロードマップのゲート重みではない。

## Deliveries

| ID | State | Remaining | Needs | Gates | 観測可能な受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M5-A01 | done | 0 | — | G1 | edition・非推奨方針と領域別互換性表を採択 | `b654ae7` |
| M5-A02 | done | 0 | M5-A01 | G1 | CLIで互換性を表示し未知editionと旧consumer境界を検証 | `a473cc0` |
| M5-B01 | doing | 2 | M5-A01 | G2 | 移行レシピ・適用可能性・書込みトランザクション契約を採択 | — |
| M5-B02 | todo | 3 | M5-B01 | G2 | 元spanを保持した実ソース修正previewと意味再検査 | — |
| M5-B03 | todo | 3 | M5-B02 | G2 | 修正適用とstale・競合・部分書込み拒否fixture | — |
| M5-B05 | todo | 3 | M5-B01 | G2 | 旧scalar構文を受入れfresh renameを証明し実在する旧sourceをpreview | — |
| M5-B04 | todo | 3 | M5-B03,M5-B05 | G2 | edition移行CLIと両hostの移行前後実行journey | — |
| M5-C01 | todo | 3 | M5-A01 | G3 | identity・namespace・authority・濫用・攻撃者の脅威モデル | — |
| M5-C02 | todo | 3 | M5-C01 | G3 | upload前にprotocol・上限・復旧・security受入ケースを採択 | — |
| M5-D01 | todo | 3 | M5-C02 | G4 | 上限付き未信頼archiveの受入とtraversal・重複・bomb拒否 | — |
| M5-D02 | todo | 3 | M5-D01 | G4 | 内容address方式のstorageとimmutable version公開transaction | — |
| M5-D03 | todo | 3 | M5-D02 | G4 | versioned indexとidentity・checksumを検証したdownload | — |
| M5-D04 | todo | 3 | M5-D03 | G4 | registry依存を解決して両hostで実行 | — |
| M5-D05 | todo | 3 | M5-D04 | G4 | lock・offline replay・公開中断・並行immutable検証 | — |
| M5-E01 | todo | 3 | M5-C02 | G5 | 認証identity・credential取扱い・scope付き認可 | — |
| M5-E02 | todo | 3 | M5-E01,M5-D02 | G5 | owner・競合制約付きpublish・yank・unyank | — |
| M5-E03 | todo | 3 | M5-E02 | G5 | owner追加・削除・移管と最後のowner・credential失効fixture | — |
| M5-E04 | todo | 3 | M5-E03 | G5 | 永続audit trail・復旧・権限外操作のend-to-end検証 | — |
| M5-F01 | todo | 3 | M5-C02 | G6 | package信頼root・provenance・署名・freshness契約を採択 | — |
| M5-F02 | todo | 3 | M5-F01,M5-D02 | G6 | 署名package release manifestと検証可能provenanceを生成 | — |
| M5-F03 | todo | 3 | M5-F02,M5-D03 | G6 | 消費前に信頼・artifact結合・期限・replayを検証 | — |
| M5-F04 | todo | 3 | M5-F03 | G6 | 鍵更新・失効・改竄と署名packageの両host journey | — |
| M5-G01 | todo | 3 | M5-A01 | G7 | Wasm component lowering・interface対応・runtime profileを採択 | — |
| M5-G02 | todo | 3 | M5-G01 | G7 | scalarソースpluginを検証可能componentにcompileし実行 | — |
| M5-G03 | todo | 3 | M5-G02 | G7 | 型付きhost import・result/error・resource adapterの往復 | — |
| M5-G04 | todo | 3 | M5-G03 | G7 | package・CLIでportable component生成と両host plugin load | — |
| M5-G05 | todo | 3 | M5-G04 | G7 | 採択plugin profileのVM・native・Wasm適合 | — |
| M5-H01 | todo | 3 | M5-G01,M5-C02 | G8 | sandbox capability方針とresource上限失敗契約を採択 | — |
| M5-H02 | todo | 3 | M5-G03,M5-H01 | G8 | ambient filesystem・network・environment・process・FFIを拒否 | — |
| M5-H03 | todo | 3 | M5-H02 | G8 | memory・fuel・stack・host call上限とcancel・cleanup | — |
| M5-H04 | todo | 3 | M5-H03,M5-G05 | G8 | 不正component・escape・DoS corpusと両host適合 | — |
| M5-I01 | todo | 2 | M5-C02 | G9 | 文書hosting・検索metadata・未信頼content方針を採択 | — |
| M5-I02 | todo | 3 | M5-I01,M5-D03 | G9 | sanitizedなversion結合package文書を公開しfixture実行 | — |
| M5-I03 | todo | 3 | M5-I02 | G9 | 上限付きmetadata検索・version/yank可視性・攻撃query検証 | — |
| M5-J01 | todo | 2 | M5-A01 | G10 | LLVM評価corpus・debug基準・再現環境を定義 | — |
| M5-J02 | todo | 3 | M5-J01 | G10 | corpus用に比較可能LLVM release backend実験を構築 | — |
| M5-J03 | todo | 3 | M5-J02 | G10 | compile・runtime・startup・size測定と実debugger動作確認 | — |
| M5-J04 | todo | 2 | M5-J03 | G10 | benchmark artifactと限界付き採用判断を公開 | — |
| M5-Z01 | todo | 3 | M5-A02,M5-B04,M5-D05,M5-E04,M5-F04,M5-H04,M5-I03,M5-J04 | G1,G2,G3,G4,G5,G6,G7,G8,G9,G10 | 署名ecosystem・移行・pluginの連続journeyを両hostで実行 | — |
| M5-Z02 | todo | 3 | M5-Z01 | G1,G2,G3,G4,G5,G6,G7,G8,G9,G10 | 全exit条件を監査し日英文のcapability guideを公開 | — |

## 見積り照合

**112 + 3 − 5 = 110 pt**

| 基準残作業 | 純追加・再見積り | 完了時に除去 | 残作業 |
| ---: | ---: | ---: | ---: |
| 112 pt | +3 pt | −5 pt | 110 pt |

## Delivery noteとロードマップ監査

- B01範囲監査（2026-10-30 JST）: 実在する旧仕様からの遷移には、現editionのLNT0001 fixに加え、上限付きscalar readerと解決名の等価証明が必要。実装前にB05を3 ptで明示する（純追加+3、計40 delivery）。受入条件は実在する旧`shared`・`weak`のpreview。B04はB03・B05後のCLI統合、検証付き適用、両host VM・native実行用に3 ptを維持する。B02の3 ptは宣言test captureと総量proof課金を含み、B03の3 ptは既存公開guardを再利用する。この範囲監査でゲートを削除・完了しない。

- 開始監査（2026-10-30 JST）: 元の10ゲートすべてを維持し、P1の先送りなし。積上げ39件、112 pt。upload受入より先にsecurityを設計する。registry署名信頼はRFC 0042のtoolchain配布と別契約。Wasmはcontainer生成だけでなくソースpluginを実行する。LLVMゲートは根拠に基づく評価であり、採用は必須ではない。
- 順序: 互換性方針 → 実行互換性fixture → 移行・修正 → 認証・署名registry消費 → portable plugin実行・sandbox → hosting・検索とbackend評価 → 両host連続journey・exit監査。独立ゲートも維持する。実装doingは最大1行、前提を追加する前に再監査する。
- 移行を実演する目的だけで新editionや破壊的構文を作らない。採択済みレシピと意味のある移行前後実行証拠を使い、未対応の遷移は拒否する。新format・protocolは実装前に独立したRFC採択を要する。
- 完了には日英`milestone-5-capabilities.md`、正確なCI head証拠、PR merge、開始branchへの復帰・同期を要する。この開始記録はexitゲート完了ではない。

出典: repositoryのroadmap・architecture・採択済みRFC。2026-10-30 JST確認。更新日を持たないソースファイルの公開日は推測しない。


A01完了（2026-10-30 JST）: 日英RFC 0043を委任採択し、既存editionのみを受入れる方針、非推奨手順、領域別互換性を定義。repository consistency 7/7成功。最終見積り2 ptを除去し、追加・再見積りなし。ロードマップの全10ゲートは引き続き必要。

B01準備（2026-10-30 JST、delivery完了への変更なし）: 2 moduleのstandalone sourceと、同じbytesにedition-2026 manifestを付けたpackageはどちらも42を返した。これはpackage化であり、edition間移行の証拠ではなく、これだけでG2を完了できない。実stdio LSPは`unused`だけを`_unused`へ変更するversion付き`LNT0001` quickfixを返し、適用後もinitializerを保持して42を返し、lintが解消した。次のfix CLIでは既存の意味rename証明を再利用する。現editor snapshotはroot entryを扱うため、B02では宣言testのcoverageを明示的に扱う必要がある。契約案はB01レビューまで採択RFC集合の外に置く。見積り・exitゲートの変更なし。

A02完了（2026-10-30 JST）: 実装`a473cc0`は上限付きhuman・JSON互換性reportを公開し、manifestと有限edition受入を共有する。保持した直接終了codeでfmt・Clippy、全workspace test 2,522件（177 suite、失敗0）を確認した。最初のrustdocは古い依存metadataを参照して`NativeContext`・`Method`のAPI不一致になったが、同じsourceが分離targetの`cargo doc --workspace --no-deps`で成功した。detached再試行では観測用一時directoryの消失も確認し、永続する`/private/tmp`で解消した。logと実終了codeはignoreされた`target/m5-validation/`に保持し、観測timeoutは成功と数えない。実装headのCI `41064851495`とnative-debug `41064851345`は旧consumer境界を含め成功。3 ptを除去し、追加なし。G1を完了（gate重み5/65）とし、他9ゲートと全P1成果は維持する。文書公開もそのPR最新headのcheck成功後にmergeする。

B01の歴史的移行証拠: [保持したprobe](../evidence/2026-10-30-m5-legacy-source-probe.json)は架空の後継editionではなく、実在する`69b1c4d`（2026-10-17）のRFC 0006 sourceを使う。`shared`・`weak`識別子はlockを固定した旧compilerで42を返すが、現compilerでは`SRC0024`が4件出る。その旧libraryから生成した315 byteのartifactは現compilerでも42を返す。これを旧profileから2026への実移行fixtureとする。B01は明示profile受入を定義し、capture・renameの前提作業を実装前に明示・再見積りする。通常compilerの現予約語拒否は維持する。この実験はG2完了ではない。

