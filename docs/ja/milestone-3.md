<!-- translation-of: ../milestone-3.md -->

# Milestone 3 delivery board

[English](../milestone-3.md) · [Exit gates](../roadmap.md#milestone-3-concurrency-and-embedding)

更新日: **2026-10-27 JST**。開始チェックポイント: `984557d`。M2 とリポジトリレビューは完了済みです。

**79 done · 0 doing · 0 todo · 0 remaining pt**

**次: none — M3の全deliveryと全exit gateが完了し、次milestoneは未開始です。**

Z02完了（2026-10-27 JST、実行source `9e78f2d`）: 全15 gateの規範契約と実装/検証を個別監査し、[日英機能guide](milestone-3-capabilities.md)に実行command・所有権/失敗/authority・公開C v1互換性・SDK version境界・cost証拠・制約をまとめました。CLI task/channel/service/owned移動、保存artifact、native channel、VM/JIT pluginと有限work拒否を実行確認しました。両host全適合と旧client移行、SDK隔離起動、未変更M2証拠を保持しています。**完了: 1 delivery / 2 pt消化、追加/再見積り: 0 pt**。照合は**2 + 0 − 2 = 0 pt**、**79行 / 79 done / 0 doing / 0 todo**、最終台帳**110 + 99 − 209 = 0**です。全P0/P1を完了し先送りはなく、M4以降は未開始です。以前のcheckpointはリンク先の履歴に保持します。

最終guide/監査の成果commitは`3dbf7d0`です。2026-10-27 18:19 JSTにrepository consistency全5件、日英15 gate/実行command一致、全参照先と`git diff --check`を確認しました。M2 benchmark source/過去結果は未変更です。

[過去のcheckpoint・見積り監査・検証証跡](milestone-3-history.md)を別文書に保持します。この完了記録へ新規maintenance作業を追加せず、現在の[レビュー](review-2026-10-27.md)や次のactive milestoneで追跡します。

## Deliveries

見積りは残作業量であり経過時間ではありません。各行は観測可能な 1〜3 pt の作業です。後続行は実装前に実際のインターフェースに照らして再確認します。`G01`〜`G15` はロードマップのゲート順です。Evidence 欄には完了した実装のコミットだけを記載します。

| ID | State | Remaining | Needs | Gates | Observable acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M3-T01 | done | 0 | — | G01 | FIFO ドライバー、親子関係、join、取消、失敗、上限付き admission の規則を採択 | `8ab751b` |
| M3-T02 | done | 0 | M3-T01 | G03,G12 | ホスト駆動の上限付きタスク木で join、協調取消、子から親への retirement を実行 | `a543d23` |
| M3-T03 | done | 0 | M3-T01 | G01,G03 | source async 契約を採択し lossless parse/recovery/format で task 構文を保持 | `0655939` |
| M3-T09 | done | 0 | M3-T03 | G03 | async call と scope 付き task handle を bind/type し、逸出と不正 async C surface を拒否 | `390e3ca` |
| M3-T10 | done | 0 | M3-T09 | G03 | suspension を跨ぐ shared/exclusive loan を両方の起点の診断付きで拒否 | `9f7012d` |
| M3-T11 | done | 0 | M3-T10 | G03 | Task/TaskError と async MIR 宣言を検証し旧実行/artifact profile を拒否 | `ae9404d` |
| M3-T04 | done | 0 | M3-T11 | G03 | typed MIR の suspension、task scope、所有権 edge を独立検証 | `700391d` |
| M3-T05 | done | 0 | M3-T02,M3-T04 | G03 | 共通 task driver 経由で source spawn/yield/join fixture を VM 実行 | `2f58084` |
| M3-T06 | done | 0 | M3-T05 | G03,G13 | 同じ fixture を native continuation で実行し event 一致を検証 | `8d6c283` |
| M3-T07 | done | 0 | M3-T12,M3-T13,M3-T14,M3-T15,M3-T16 | G03 | 所有 local、入れ子の取消、主/副失敗、exactly-once cleanup を検証 | `8e72ea7` |
| M3-T12 | done | 0 | M3-T06 | G03,G13 | owned task 実行前に join 後の local cleanup と公開逆順の result retirement を保証する | `29badaa` |
| M3-T13 | done | 0 | M3-T12 | G03,G13 | owned flat buffer/record/whole sum の move、replacement、有界 cleanup report を VM/native で実行 | `f75738a` |
| M3-T14 | done | 0 | M3-T13 | G03 | task 実行区間内の同期 call をまたぐ通常 shared/exclusive loan と borrowed payload を実行 | `642e4be` |
| M3-T15 | done | 0 | M3-T14 | G03 | 中断/取消される owned Vec/Map/shared/weak pipeline を既存 managed service で VM/native 実行 | `c1b7887` |
| M3-T16 | done | 0 | M3-T15 | G03 | region-safe loan と descendant join 後の retirement を持つ owned arena task scope を VM/native 実行 | `59a5070` |
| M3-T08 | done | 0 | M3-T07 | G12,G13 | 全実行経路で task/frame/fuel の共有上限を副作用前に適用 | `fdfc755` |
| M3-C01 | done | 0 | M3-T08,M3-A01,M3-A05 | G01,G04 | 有界 channel、FIFO wakeup、背圧、所有権 commit、取消 error を採択 | `2a13940` |
| M3-C05 | done | 0 | M3-C01,M3-T08 | G04,G13 | Execute bounded task park/wake with branded wait tickets, FIFO resumption, cancellation revocation and stale-wake checks | `72e759c` |
| M3-C02 | done | 0 | M3-C01,M3-C05 | G04 | 有界 channel の送受信、close、待機者取消 fixture を実行 | `4056cce` |
| M3-C06 | done | 0 | M3-C02,M3-A06 | G01,G04,G05 | Decide/check opaque channel role types and distinct Send/Share independently in source/MIR; refuse legacy layouts/artifacts | `e16e537` |
| M3-C03 | done | 0 | M3-C02,M3-C06,M3-A05 | G04 | VM で source channel fixture を実行し payload 再利用を拒否する | `11769c1` |
| M3-C04 | done | 0 | M3-C03,M3-T06 | G04 | native 実行で channel の値・失敗・cleanup を一致させる | `f882074` |
| M3-C07 | done | 0 | M3-C04,M3-A08 | G04,G05,G13 | typed Copy aggregate/sum message の staging/回復・inventory と通常 channel VM/native/別リンク・service VM conformance を実行 | `7d1a71a` |
| M3-A01 | done | 0 | M3-T09 | G01,G05 | 再帰的 Send/Share 導出と source service 権限規則を採択 | `9b2b68c` |
| M3-A06 | done | 0 | M3-A01,M3-T11 | G05 | Derive Send/Share independently in source/MIR and check owned async signatures with compile-pass/fail evidence | `d3b33e3` |
| M3-A05 | done | 0 | M3-A06,M3-T08 | G03,G05,G13 | Execute owned async arguments/results with move publication, consuming await, retained-result disposal and controls on VM/native | `ea822bd` |
| M3-A02 | done | 0 | M3-A01,M3-A06,M3-C03 | G05 | actor 所有権、move、送信不能 capture を診断付きで検査 | `8944645` |
| M3-A07 | done | 0 | M3-A02,M3-C03 | G05 | 実 state storage/read/全体置換と有限 control で owned service FIFO source を VM 実行 | `d90de9b` |
| M3-A08 | done | 0 | M3-A07 | G05 | handler 境界の service 取消、pending send 回復、FIFO queue-before-state 退役を実行 | `4223d7b` |
| M3-A03 | done | 0 | M3-A07,M3-A08,M3-C07 | G05 | owner 分離した service state、FIFO message、取消時の破棄を実行 | `8d945b0` |
| M3-A04 | done | 0 | M3-A03,M3-C04 | G05 | native で actor 分離と破棄 trace を一致 | `cfd9641` |
| M3-I01 | done | 0 | — | G02 | production IDL/wire/embedding profile、error、再入、取消を採択 | `1f477fa` |
| M3-I02 | done | 0 | M3-I01 | G06 | version 付き IDL を parse/check し有界診断と不正 schema を検証 | `6816a74` |
| M3-I03 | done | 0 | M3-I02 | G06 | LCMP3 contract/report・LCVL1/LCSG1 value/signature codec を独立 canonical/不正/budget vector で検証 | `228daf2` |
| M3-I09 | done | 0 | M3-I02,M3-I03 | G01,G06,G07 | opaque nominal component resource の型・能力・move を検証し未対応実行を拒否 | `13db0a6` |
| M3-I06 | done | 0 | M3-I02,M3-I03,M3-I09 | G01,G06,G07 | component binding map と resource-key/call/signature/effect/loan facts を source/MIR で独立検証し旧 profile 拒否を維持 | `0646497` |
| M3-I04 | done | 0 | M3-I03,M3-I06 | G06,G14 | typed native-host/script fixture 用の C/guest binding を生成 | `adb2e03` |
| M3-I05 | done | 0 | M3-I03 | G06 | interface/version 交渉と非互換 metadata を副作用前に検証 | `6544afe` |
| M3-P01 | done | 0 | M3-I04,M3-I05 | G08 | typed import の明示 grant を必須とし欠落/不一致 capability を拒否 | `ff94b34` |
| M3-R01 | done | 0 | M3-I04,M3-P01,M3-R04,M3-R05 | G07 | 実 context の resource index と whole-call preflight/commit を検証・実行し、commit 前拒否時に owner を保持 | `f3b086c` |
| M3-R02 | done | 0 | M3-R01,M3-E05 | G07 | borrow scope、generation、所有権移転、poison を適用 | `2adfeb8` |
| M3-R06 | done | 0 | M3-R02 | G07 | 全 call の native import 所有引数を元 custody と必須退役で実行 | `8914a50` |
| M3-R03 | done | 0 | M3-R02,M3-R06 | G07 | VM/native の error、取消、不正入力で resource cleanup を一致 | `0e7b8ae` |
| M3-R04 | done | 0 | M3-I04,M3-P01 | G07 | 独立 binding 済み source resource 引数/結果を実 allocator/provider/code 所有権・必須 cleanup とともに VM/native で実行し、旧 profile 拒否を維持 | f06bd95 |
| M3-R05 | done | 0 | M3-I04,M3-P01,M3-R04 | G07 | rich 所有値の source 引数/結果を既存 VM/native engine で transport し、入れ子 owner・元 allocator/code 所有権・staging cleanup・結果 escrow を実行 | `3a3bbc4` |
| M3-I10 | done | 0 | M3-I06,M3-R03 | G06,G09,G14 | schema 5 の task/channel/service/component field・tag・proof・limit・互換 vector を受理 | `51459cb` |
| M3-I11 | done | 0 | M3-I10 | G06,G09,G14 | 独立検証する task MIR を encode/decode し最初の保存 source task fixture を VM/生成 native で実行 | `669cc4e` |
| M3-I07 | done | 0 | M3-I06,M3-R03,M3-I10,M3-I11 | G06,G09,G14 | task/channel/service/component artifact の versioned profile を受理し、独立検証と旧 vector を保って実行する | `a0501f4` |
| M3-P02 | done | 0 | M3-P01,M3-I05,M3-I07 | G08 | metadata/adapter 作業量を制限し不正 import を副作用前に拒否 | `fc4bd48` |
| M3-E06 | done | 0 | M3-P01,M3-R05,M3-R06 | G07,G09 | 元 native 登録/code owner を共有保持し handle 解放後の resource 退役と保存 source VM/native 実行を検証 | `2c26241` |
| M3-E07 | done | 0 | M3-E06,M3-I07,M3-R05 | G07,G09 | 元 resource table/allocator と holder 権限・lifecycle を保持し scoped program から連続保存 source root を VM/native で実行 | `39c9265` |
| M3-E08 | done | 0 | M3-R03,M3-E07,M3-P01 | G07,G09 | 元table/allocatorでC resource index・typed移動/借用/結果/drop・holder単位のcloseを実行 | `75ca09c` |
| M3-E09 | done | 0 | M3-E08,M3-E03 | G07,G09 | C callbackの永続keep/takeとprovider closeを元owner/destructor・loan/poison/cleanup規則で実行 | `7ba6a45` |
| M3-E01 | done | 0 | M3-I04,M3-P01,M3-I07,M3-E06,M3-E07,M3-E08,M3-E09,M3-E04,M3-L04,M3-I15,M3-E15,M3-E16 | G09 | 全32操作のC entryを公開し、対応installed modeで生成typed host callを実行 | `40158d6` |
| M3-E15 | done | 0 | M3-I04,M3-I07,M3-E04,M3-L04 | G09 | 検査済みtask root export要求をbindingし生成Cのtask呼出し・拒否・cleanupを実行 | `ae90a88` |
| M3-E16 | done | 0 | M3-I15,M3-E15,M3-L04 | G09 | 厳密SDK所有native imageをbindingしC gatewayからinstalled AOTを実行 | `42b27ba` |
| M3-E10 | done | 0 | M3-E08,M3-P02 | G09,G12 | child entry統合前にscoped VM/JIT source呼び出し間で元fuel/host-call/service-workの累積計量を実行 | `879a534` |
| M3-E11 | done | 0 | M3-E08,M3-E10 | G09,G12 | 元handle・limit・統計・provider副作用を保つscoped allocator viewでVM/JIT sourceを実行 | `93f9b52` |
| M3-E12 | done | 0 | M3-E07,M3-E10,M3-E11 | G09,G12 | frame pool・arena・inspector・shared heap・retirementの準備失敗時に元課金を保持し、有限予算のVM/JIT child callbackを実行 | `44823d6` |
| M3-E13 | done | 0 | M3-E07,M3-E10,M3-E12 | G09,G12 | 元taskのidentity・取消・lifecycleを借用し、task再admissionなしで実同期VM/JIT callbackを実行 | `be604da` |
| M3-E14 | done | 0 | M3-E07,M3-E12,M3-E13 | G09 | 再開するcomponent検証で元preparation ledger・失敗prefix・固有proof cost・一度だけのentry課金を維持 | `078e9b4` |
| M3-E02 | done | 0 | M3-E08,M3-E07,M3-E06,M3-E10,M3-E11,M3-E12,M3-E13,M3-E14,M3-E03 | G09 | script から native host を呼び入れ子 guest callback を受信 | `082d1de` |
| M3-E03 | done | 0 | M3-E06,M3-E07,M3-E14,M3-R03 | G09 | typed failure、元の context、再入規則、poison を mode 間で維持 | 52b20e0 |
| M3-E04 | done | 0 | M3-E03,M3-T07,M3-E09 | G10 | callback を跨いで logical mixed stack、取消、cleanup を維持 | `8227a74` |
| M3-E05 | done | 0 | M3-I04,M3-P01,M3-R01 | G07,G09 | canonical transport・実直接 Resource backing 変更・元 allocator/context・scope の完全 cleanup により VM/native source から保持済み native member を実行 | `a13272b` |
| M3-L01 | done | 0 | M3-E03 | G11 | callback の確保を含む heap/allocation の共有上限を適用 | `85e7dce` |
| M3-L02 | done | 0 | M3-T08,M3-E03 | G12 | guest/host を跨ぐ再帰/frame/live-task 合算上限を適用 | `599d7ee` |
| M3-L03 | done | 0 | M3-E04,M3-T08,M3-I12 | G13 | VM/JIT/AOT と callback 境界で interruption を一貫して観測 | `c7daed1` |
| M3-L04 | done | 0 | M3-L01,M3-L02,M3-L03,M3-P02 | G11,G12,G13 | 敵対的な資源枯渇でも必須 cleanup を完了 | `d5ee99a` |
| M3-I08 | done | 0 | M3-I07,M3-E04,M3-P02,M3-I13,M3-I14,M3-I15 | G08,G09,G14 | 両hostでinstalled task/component startupを実検証する | `8f41f44` |
| M3-I12 | done | 0 | M3-I07,M3-E04,M3-P02 | G08,G09 | Emit/bind checked component objects and execute scalar/resource VM/JIT/AOT callbacks with original task authority | `47093ef` |
| M3-I13 | done | 0 | M3-I07,M3-I12 | G08,G09,G14 | Persist/revalidate native task/component records and bind exact linked code without code generation | `8ba2c43` |
| M3-I14 | done | 0 | M3-I13,M3-T08 | G09,G14 | Publish/link installed task packages with exact SDK v2 and original control/cleanup | `042fc38` |
| M3-I15 | done | 0 | M3-I13,M3-I14,M3-P02 | G08,G09,G14 | 明示binding/grantと元callback/cleanupを保つinstalled component packageを公開・リンクする | `8b89aae` |
| M3-B01 | done | 0 | M3-L04,M3-A04,M3-I08,M3-E01 | G14 | 両 tier-1 host で所有権/失敗/capability/資源適合試験に合格 | 19a29c0 |
| M3-B02 | done | 0 | M3-B01 | G14 | C embedding API と生成 binding を移行試験付きで安定化 | 9e78f2d |
| M3-X01 | done | 0 | M3-E01 | G15 | 権限を明示する trusted-native/verified-script の実行可能例を提供 | `8a3ae2e` |
| M3-Z01 | done | 0 | M3-E01,M3-L04 | G03,G04,G05 | spawn/join/channel/取消/メモリ費用を記録し M2 workload の証拠を維持 | `c9ffd19` |
| M3-Z02 | done | 0 | M3-X01,M3-Z01,M3-B02 | G01,G02,G03,G04,G05,G06,G07,G08,G09,G10,G11,G12,G13,G14,G15 | 全ゲートを監査し command、証拠、制限を含む日英 capability guide を公開 | 3dbf7d0 |

## Progress and estimate changes

最終照合: 開始時110 pt + 明示追加/再見積り99 pt − 完了時の最後の見積り209 pt = 残り0 pt。79 deliveryが完了し、doing/todoはありません。今回の履歴分離で見積り・gate・証拠commitを変更していません。個別scope監査と計上根拠は[履歴](milestone-3-history.md#progress-and-estimate-changes)に保持します。

**110 + 99 − 209 = 0 pt**

| 開始時残り | 純追加/再見積り | 完了時消化 | 残り |
| ---: | ---: | ---: | ---: |
| 110 pt | +99 pt | −209 pt | 0 pt |

## Exit audit

**15/15 gate完了、96/96 gate weight。** `9e78f2d`の実装と両host検証、I08の両host isolated SDK 7 profile、stabilized RFC 0033の旧C client移行、X01のnative/script plugin、Z01の有限cost/M2 parityを照合しました。[全15行の監査表と利用guide](milestone-3-capabilities.md#全exit監査)と[機械記録](../evidence/2026-10-27-m3-exit-9e78f2d.json)に各条件と具体的suiteを対応付けます。G01/G02は受理契約、G03–G13は実source/VM/JIT/AOTと原scopeの所有権・失敗・capability・resource/複合control、G14は両host全適合後のC互換性、G15は実行例で完了しました。header cross-compileだけでの代用やP0/P1の先送りはありません。public C v1の安定性はprivate SDK/native language ABIへ拡張しません。

## Sources

- [M3機能guide](milestone-3-capabilities.md)と[出口記録](../evidence/2026-10-27-m3-exit-9e78f2d.json): 2026-10-27。
- [両host適合・旧client移行](../evidence/2026-10-27-m3-migration-9e78f2d.json): 2026-10-27。
- [元の検証日・情報源・見積り監査](milestone-3-history.md): 2026-10-20–27の記録を原文のまま保存。
