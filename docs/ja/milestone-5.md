<!-- canonical-language: en; japanese-translation: ja/milestone-5.md -->

# Milestone 5 delivery board

[English](../milestone-5.md)

2026-10-30 JST、`3b676c5`から開始。[ロードマップ](roadmap.md#milestone-5ecosystemとcompatibility)のM5全範囲（P1の2ゲートを含む）を追跡する。M4のinstallerとlocal packageは前提条件であり、registry完了を意味しない。

**7 done · 1 doing · 32 todo · 96 remaining pt**

**次: M5-C01 — identity・namespace・authority・濫用・攻撃者のregistry脅威モデルを採択する（3 pt）。**

B04・G2完了（2026-10-31 10:59 JST確認）: code `62e742c`と[保持した証拠](../evidence/2026-10-31-m5-source-migration.json)で、実旧producer・現VM・link済みnativeの両host journeyを検証した。固定`2f905c7`の全baselineは180 suite・2,553 test、fmt・厳密Clippy・rustdocが成功し、4つのchild exitすべてが0、source fingerprintも不変。logは`/private/tmp/lang-m5-fixes-target/m5-b04-validation/`に保持する。最終head `ab5cda8`のCI `41156030921`（対象18 job、full routingにより文書専用のみ意図的省略）とnative-debug `41156030861`（3 host job）が成功。[PR #31](https://github.com/carimatics/lang/pull/31)を`46cb6a1`としてmergeし、元の`main`はcleanかつ`origin/main`と同期済み（ahead 0 / behind 0）。以下の日付付き未完了checkpointは履歴であり、現在の阻害条件ではない。

C01範囲監査（2026-10-31 JST）: 採択脅威モデルと検証可能な信頼境界に3 ptを維持する。C02はupload前のwire format・上限・復旧・security受入ケースとして別に3 ptを維持する。実装deliveryのdoingはない。branch `codex/m5-registry-contract`は`main`（`46cb6a1`）からclean状態で作成し、checkoutは`/private/tmp/lang-m5-registry`、PR baseは`git@github.com:carimatics/lang.git`の`main`。delivery追加・見積り変更なし。P1の両成果を含む8つのopenゲートをすべて維持する。

B04受入監査（2026-10-31 10:34 JST確認）: [保持した証拠](../evidence/2026-10-31-m5-source-migration.json)はmacOS 15/26・Windows 2025の各6 fixtureについて、実旧source遷移5件、現VM・link済みnative実行、厳密な編集適用、再実行no-opを示す。CLI・公開・拒否要件は実装済みで、残作業はlocal全baseline・最新head CI・PR公開。これらのcheckを省かず2 → 1 ptへ再見積りする。B04はdoing、G2はopenを維持し、固定`2f905c7`のtest processは稼働中、fmt・厳密Clippyは成功している。

B04再開checkpoint（2026-10-31 10:26 JST確認）: `62e742c`のCI `41154299004`はWindowsを含む3 hostのSDK producer移行journeyと隔離SDK install jobに成功し、残りのCI jobは実行中。local `5480e14` baselineに終了結果はなく、session 60960とdriver・Cargo・test processは消失し、logは10:17 JSTで停止している。中断試行は`/private/tmp/lang-m5-fixes-target/m5-b04-validation-5480e14-interrupted/`に保持し、途中の645 test成功を全baseline成功に数えない。作業branchを`2f2e2b9`までfast-forwardし、新たに固定するcheckpointで全baselineを実行する。local全検証と最新head CIが成功するまでB04はdoing・2 ptを維持する。

B04 CI修正（2026-10-31 JST）: run `41109397767`はmacOSの移行journeyに成功したが、WindowsではGitの自動CRLF変換が固定旧lockのbytesを変え、実行前に拒否した。一時checkoutで`core.autocrlf=true`の不一致と`false`の保持hash一致を再現した。旧worktree作成時だけ変換を無効にし、厳密なlock検査は維持する。このCIだけの修正は`5480e14`から作った`/private/tmp/lang-m5-migration-ci`に隔離し、元worktreeでRust baselineの入力を不変に保って継続する。同branch・PRに公開し、baseline終了後に元worktreeをfast-forwardする。失敗CIをgate完了に数えず、deliveryの範囲・見積りも変更しない。

B04 host checkpoint（2026-10-31 03:42 JST確認）: `tools/check-source-migration.py`はlock不変で固定旧compilerをbuildし、元fixture 6件を実行、実際のmigration CLIを適用し、現VMと対応SDKでlinkしたnative結果を検査した。実遷移5件は保持した`shared`・`weak`、wrapping parameter、再帰、末尾comma、loop・名前衝突・UTF-8 comment・CRLF保持を検証し、変更不要の6件目でno-opを別途検査する。macOS localですべて成功し、source・compiler・lock・SDK・実行fileのhashと59 commandの観測を`/private/tmp/lang-m5-b04-journey/report.json`に保持する。既存SDK producer matrixにも同journeyを追加し、macOS 15/26・Windows 2025のreportを保持する。Windows・最終全baseline・最新head CIは未完了で、B04はdoing・2 ptを維持する。

B04実行checkpoint（2026-10-31 03:32 JST確認）: `lang migrate`はprofile・edition・rootを明示必須とし、既定でpreview、新しく証明した単一source置換を共通公開境界で適用する。CLI fixtureは元bytesを変更せずpreviewし、適用後42を実行して、再実行がno-opになる。移行test 15件は旧受入・証明に加え、適用・stale bytes/identity・異なるauthority・readonly/alias・usage拒否・cwd基準/絶対source pathを検証し、既存fix 11/11・format 7/7も成功。この実行可能な統合checkpointを受け、残見積りを3から2 ptへ下げる。残る受入条件は固定旧producer・現VM・対応SDK nativeの両host journey、最終全baseline、最新head CI。このcheckpointはdelivery完了ではなく、同じ一貫したbranchでB04だけをdoingとする。

B05完了（2026-10-31 03:17 JST確認）: code `7f41dd0`は実在する旧`shared`・`weak` sourceを受入れ、元token offsetと型付き解決構造を証明し、rewrite後sourceは42を返す一方、通常compileは元sourceを拒否する。integration test 8件・proof test 2件で、固定producerの受入11ケース、生成scalar実行24件、wrapping加算、燃料切れ保持、衝突・trivia・CRLF、再実行no-op、意味の反例、引上げ不可の再帰上限を含む厳密な資源境界を検証。source固定全baselineの179 suite・2,546 test、fmt・厳密Clippy・rustdocが成功し、全child exitは0、全入力fingerprintは不変。logは`/private/tmp/lang-m5-fixes-target/m5-b05-validation/`に保持する。同headのCI `41101729179`は対象18 jobすべて成功（full routingにより文書専用のみ意図的省略）、native-debug `41101727954`も3 host jobすべて成功。[PR #31](https://github.com/carimatics/lang/pull/31)は関連するB04継続用にdraftを維持し、後続headもその公開check成功を必要とする。

branch `codex/m5-historical-source-migration`は`codex/m5-checked-source-fixes`の`791237e`からclean状態で作成し、checkoutは`/private/tmp/lang-m5-migration`、初期PR baseは`git@github.com:carimatics/lang.git`の同前提branch。PR #30のmerge確認後、`6ea2cd1`へfast-forwardし、PR baseを`main`へ変更した。元worktreeは`main`上でcleanかつ`origin/main`と同期済み（ahead 0 / behind 0）。

公開checkpoint（2026-10-31 JST）: code `01b425e`のCI `41085682122`（18 job成功、full routingにより文書専用のみ意図的省略）とnative-debug `41085682104`が成功。全local baselineは178 suite・2,536 test、fmt、厳密Clippy、rustdocが成功し、全child exitは0、source fingerprintは不変。logは`/private/tmp/lang-m5-fixes-target/m5-b03-validation-authorized/`に保持する。最初のsandbox実行はHTTPS fixture 2件のloopback待受を拒否したため、同一の固定sourceで承認済み実行を行った。失敗・取消runを成功とは数えない。最終head `791237e`もCI `41094820142`（対象18 job）とnative-debug `41094820280`が成功し、[PR #30](https://github.com/carimatics/lang/pull/30)を`6ea2cd1`としてmerge、local `main`を同じremote tipへfast-forwardした。

B03完了（2026-10-31 JST確認）: `lang fix`はpreviewが既定で、`--apply`はformatterの固定入力検証・全件staging・file単位置換を再利用し、非公開の検証済みplanを再計算する。entry・testの適用前後VM実行、古いsource・依存・manifestと異なるauthority拒否、readonly・alias拒否、staging失敗cleanup、正確な適用済みprefix、再実行no-op、CLI診断、適用成功reportのsize境界を検証。取得inventory外のhardlinkが既存alias禁止要件の漏れを露出したため、両live検証でopen済みfileのlink数を確認し、RFC 0036を日英で明確化した。実装と両host CIは完了し、最終文書公開は上記の別条件として追跡する。B04は旧source移行統合を維持する。

B02 local完了（2026-10-31 JST確認）: 実装`2a3aca1`は元spanを合成し、各rename後にentryと全宣言testを証明する。source固定全baselineの178 suite・2,529 test、fmt、厳密Clippy、rustdocが成功し、実終了codeを`/private/tmp/lang-m5-fixes-target/m5-validation/`に保持した。修正`5e2b593`はPOSIX専用test期待値を厳密native pathへ置換し、対象preview・整合性・fmt・Clippyが成功。公開には最新head CIが必要で、検証付き適用と旧source移行は別のB03・B05・B04成果とする。

B01 checkpoint（2026-10-30 JST）: [RFC 0044](rfcs/0044-checked-source-migration.md)でpreview既定のfix、検証付き公開、明示的な旧scalar profileを採択。保持した旧`shared`・`weak` sourceは42を返し、手動alpha-rename後も現VMで42を返してnative出力を生成する。移行commandとnative実行journeyは未実装。B01検証は日英整合性7/7成功、`1ab1201`としてcommit済み。作業branch `codex/m5-checked-source-fixes`は`main`（`3b676c5`）から作成し、PR baseを`git@github.com:carimatics/lang.git`の`main`として記録。その後PR #29の前提commit `8a43f76`へfast-forwardした。checkoutは`/private/tmp/lang-m5-fixes`。元worktreeはcleanを保ち、同PRのCIを待つ。

A02公開checkpoint（2026-10-31 JST確認）: 最新head `8a43f76`のCI `41072332781`とnative-debug `41072332783`（attempt 2）が成功した後、[PR #29](https://github.com/carimatics/lang/pull/29)を`1cdad09`としてmergeした。full routingにより文書専用jobだけを意図的に省略。元worktreeは`main`へ復帰し、cleanかつ`origin/main`と同期済み（ahead 0 / behind 0）。remoteは`git@github.com:carimatics/lang.git`。A01・A02のlocal baselineは2,522 test、fmt、Clippy、分離targetのrustdocが成功。[Draft PR #30](https://github.com/carimatics/lang/pull/30)で別branchの検証付きfix previewを追跡し、その全local baselineは成功、修正headのCIは未確定。M5の9ゲートはopenを維持する。

## Exit gates

B05 budget監査（2026-10-31 JST）: 再利用するscalar検査内部の独立task構文分類3回をすべて計上する。各scanは共有budgetからphase許容量全体を予約し、新しい小source回帰testで不十分な総量budgetを拒否する。最初の`a55cda0`全体検証はこの修正のため意図的に中断し、`/private/tmp/lang-m5-fixes-target/m5-b05-validation-a55cda0-interrupted/`に保持した。成功baselineとは数えず、修正後の固定commitで全体を再実行する。既存の課金要件を満たす修正で、新規delivery・見積り変更なし。

| Gate | 必須成果 | State |
| --- | --- | --- |
| G1 | edition・非推奨方針・互換性表 | done |
| G2 | edition移行と機械適用可能な修正 | done |
| G3 | upload前のregistry脅威モデル採択 | open |
| G4 | storage・index・download・immutable version | open |
| G5 | 認証publish・yank・owner操作とaudit | open |
| G6 | 署名release manifestとpackage provenance | open |
| G7 | portable WebAssembly component plugin target | open |
| G8 | Wasm sandboxとcapability適合 | open |
| G9 | 文書hostingとmetadata検索（P1） | open |
| G10 | 公開LLVM release backend評価（P1） | open |

ゲート完了: **2/10; 13/65 historical gate weight**。以下は残作業見積りであり、実時間やロードマップのゲート重みではない。

## Deliveries

| ID | State | Remaining | Needs | Gates | 観測可能な受入条件 | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M5-A01 | done | 0 | — | G1 | edition・非推奨方針と領域別互換性表を採択 | `b654ae7` |
| M5-A02 | done | 0 | M5-A01 | G1 | CLIで互換性を表示し未知editionと旧consumer境界を検証 | `a473cc0` |
| M5-B01 | done | 0 | M5-A01 | G2 | 移行レシピ・適用可能性・書込みトランザクション契約を採択 | `1ab1201` |
| M5-B02 | done | 0 | M5-B01 | G2 | 元spanを保持した実ソース修正previewと意味再検査 | `2a3aca1` |
| M5-B03 | done | 0 | M5-B02 | G2 | CLI修正適用とstale・競合・部分書込み拒否fixture | `01b425e` |
| M5-B05 | done | 0 | M5-B01 | G2 | 旧scalar構文を受入れfresh renameを証明し実在する旧sourceをpreview | `7f41dd0` |
| M5-B04 | done | 0 | M5-B03,M5-B05 | G2 | edition移行CLIと両hostの移行前後実行journey | `62e742c` |
| M5-C01 | doing | 3 | M5-A01 | G3 | identity・namespace・authority・濫用・攻撃者の脅威モデル | — |
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

**112 + 1 − 17 = 96 pt**

| 基準残作業 | 純追加・再見積り | 完了時に除去 | 残作業 |
| ---: | ---: | ---: | ---: |
| 112 pt | +1 pt | −17 pt | 96 pt |

## Delivery noteとロードマップ監査

- B04完了（2026-10-31 JST）: local全baseline・最終head CI・PR merge確認後に最終1 ptを退役。退役見積りは17 pt、純追加は+1 ptを維持する。C01だけをdoing（3 pt）とする。新規作業・範囲削減なし。G2完了、8ゲートはopen。

- B04受入監査・再見積り（2026-10-31 10:34 JST）: 実旧source・現VM・link済みnativeのjourneyが両hostで成功し、report・source hashも独立に照合した。残る最終検証・公開を2から1 ptへ下げ、純追加・再見積りは+3 − 1 − 1 = +1 ptとする。B04は未完了のため退役見積り16 ptは維持する。delivery・gate・受入条件の削除なし。

- B04 checkpoint・再見積り（2026-10-31 JST）: CLIと非公開の単一source公開adapterで移行fixtureを実行できた。残作業は両hostの旧producer・現VM・native journeyと最終検証のため、受入条件を削除せず3 → 2 ptへ下げる。純追加・再見積りは+3 − 1 = +2 ptとし、B04はdoingなので退役見積り16 ptは変えない。新規delivery追加なし。

- B05完了（2026-10-31 JST）: `7f41dd0`の全local・同head両host検証後、最後の3 ptを退役。追加・再見積りなし。次はB04で、上記統合監査後も3 ptを維持する。明示CLI・検証付き適用・実旧producerと現VM・nativeのjourneyが両hostで成功するまでG2はopen。他の8 openゲートとP1の両成果も必須を維持する。

- B03完了（2026-10-31 JST）: code `01b425e`、全local baseline 2,536/2,536とfmt・Clippy・rustdoc、両host CIが成功。最終3 ptを退役し、追加・再見積りなし。B05・B04の旧source受入、検証付き移行CLI、両hostでの実旧producer・現VM・native journeyが成功するまでG2をopenとする。次はB05。他の全ゲートとP1成果も必須を維持する。

- 旧source受入準備（2026-10-31 JST、delivery退役なし）: 11件の[固定producer probe](../evidence/2026-10-31-m5-scalar-admission.json)でblock scopeと関数全体の名前一意性、不変parameter、明示local型・`mut`、localによるcallable隠蔽、到達不能名の検証、必要なreturn経路、前方再帰、任意の`else`、parameter・argument末尾commaを確定した。RFC 0044の日英版に既存要件を記録。B05で予定する解決名・型の証明が必要であることを確認し、追加前提・3 ptの見積り変更なし。B03は全local検証の終了まで唯一のdoing行を維持する。

- B02 local完了（2026-10-31 JST）: 実装`2a3aca1`のsource固定全baseline（2,529 test、fmt、厳密Clippy、rustdoc）が成功。修正`5e2b593`はnative pathの厳密比較を維持し、対象検証が成功。最後の3 ptを退役し、追加・再見積りなし。G2はopen。最新head CI・PR mergeは公開条件として未完了。B03だけをdoingとし、新規planのCLI・共通writerを`codex/m5-checked-source-fixes`上の一貫した続きとして進める。

- B01完了（2026-10-30 JST）: `1ab1201`でRFC 0044を実験段階委任により採択し、未検証の実装前提を明示した。日英repository consistencyは7/7成功。validatorがNext markerの変更を検出したため既存markerへ戻した。B01の最終2 ptを除去し、別に監査した前提+3 ptは維持する。G2は未完了、doingはB02のみ。

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

