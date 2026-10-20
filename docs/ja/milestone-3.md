<!-- translation-of: ../milestone-3.md -->

# Milestone 3 delivery board

[English](../milestone-3.md) · [Exit gates](../roadmap.md#milestone-3-concurrency-and-embedding)

更新日: **2026-10-21 JST**。開始チェックポイント: `984557d`。M2 とリポジトリレビューは完了済みです。

**2 done · 1 doing · 38 todo · 110 remaining pt**

**次: M3-T03 → source async 構文を採択し、lossless parser/formatter fixture を実行します。**

現在の作業: **M3-T03**。直近の実行検証: **2026-10-21 JST**。[RFC 0022](rfcs/0022-source-structured-tasks.md) を採択し、lossless async/task grammar、recovery、formatter fixture が成功。追加 14 件で優先順位/comment/上限、file/virtual/cached module/CLI/transactional REPL/LSP format・診断の未対応 async admission 拒否を検証しました。全 baseline は **96 suite / 成功 1,627 / 失敗 0 / 既存 timing ignore 1**。fmt、warnings-denied Clippy、rustdoc、全 11 fuzz target の compile/lint も成功。候補 source は parse に成功し、SRC0012 の代わりに SRC0026 を報告します。残る受入条件: 実装 commit と証拠公開。binding/型/loan 証明は T09/T10 であり、source task の実行と M3 ゲートは未完了です。

**今回: T02 完了（−3 pt）、source analysis 2 作業を明示（+5 pt）。** 前回残り **108 + 5 − 3 = 110 pt**。全 **41 行**、累積消化 **5 pt** と追加見積り **+5 pt** は別に記録します。

## Deliveries

見積りは残作業量であり経過時間ではありません。各行は観測可能な 1〜3 pt の作業です。後続行は実装前に実際のインターフェースに照らして再確認します。`G01`〜`G15` はロードマップのゲート順です。Evidence 欄には完了した実装のコミットだけを記載します。

| ID | State | Remaining | Needs | Gates | Observable acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M3-T01 | done | 0 | — | G01 | FIFO ドライバー、親子関係、join、取消、失敗、上限付き admission の規則を採択 | `8ab751b` |
| M3-T02 | done | 0 | M3-T01 | G03,G12 | ホスト駆動の上限付きタスク木で join、協調取消、子から親への retirement を実行 | `a543d23` |
| M3-T03 | doing | 3 | M3-T01 | G01,G03 | source async 契約を採択し lossless parse/recovery/format で task 構文を保持 | — |
| M3-T09 | todo | 3 | M3-T03 | G03 | async call と scope 付き task handle を bind/type し、逸出と不正 async C surface を拒否 | — |
| M3-T10 | todo | 2 | M3-T09 | G03 | suspension を跨ぐ shared/exclusive loan を両方の起点の診断付きで拒否 | — |
| M3-T04 | todo | 3 | M3-T10 | G03 | typed MIR の suspension、task scope、所有権 edge を独立検証 | — |
| M3-T05 | todo | 3 | M3-T02,M3-T04 | G03 | 共通 task driver 経由で source spawn/yield/join fixture を VM 実行 | — |
| M3-T06 | todo | 3 | M3-T05 | G03,G13 | 同じ fixture を native continuation で実行し event 一致を検証 | — |
| M3-T07 | todo | 3 | M3-T06 | G03 | 所有 local、入れ子の取消、主/副失敗、exactly-once cleanup を検証 | — |
| M3-T08 | todo | 3 | M3-T07 | G12,G13 | 全実行経路で task/frame/fuel の共有上限を副作用前に適用 | — |
| M3-C01 | todo | 2 | M3-T05 | G01,G04 | 有界 channel、FIFO wakeup、背圧、所有権 commit、取消 error を採択 | — |
| M3-C02 | todo | 3 | M3-C01 | G04 | 有界 channel の送受信、close、待機者取消 fixture を実行 | — |
| M3-C03 | todo | 3 | M3-C02 | G04 | source channel fixture を VM 実行し payload 再利用を拒否 | — |
| M3-C04 | todo | 3 | M3-C03,M3-T06 | G04 | native で channel の値、失敗、cleanup を一致 | — |
| M3-A01 | todo | 2 | M3-T09 | G01,G05 | 再帰的 Send/Share 導出と source service 権限規則を採択 | — |
| M3-A02 | todo | 3 | M3-A01 | G05 | actor 所有権、move、送信不能 capture を診断付きで検査 | — |
| M3-A03 | todo | 3 | M3-A02,M3-C03 | G05 | owner 分離した service state、FIFO message、取消時の破棄を実行 | — |
| M3-A04 | todo | 3 | M3-A03,M3-C04 | G05 | native で actor 分離と破棄 trace を一致 | — |
| M3-I01 | todo | 3 | — | G02 | production IDL/wire/embedding profile、error、再入、取消を採択 | — |
| M3-I02 | todo | 3 | M3-I01 | G06 | version 付き IDL を parse/check し有界診断と不正 schema を検証 | — |
| M3-I03 | todo | 3 | M3-I02 | G06 | scalar/record/variant/sequence を独立した上限付き encode/decode | — |
| M3-I04 | todo | 3 | M3-I03 | G06,G14 | typed native-host/script fixture 用の C/guest binding を生成 | — |
| M3-I05 | todo | 2 | M3-I03 | G06 | interface/version 交渉と非互換 metadata を副作用前に検証 | — |
| M3-R01 | todo | 3 | M3-I04 | G07 | 実 adapter で resource preflight/commit/rollback を実行 | — |
| M3-R02 | todo | 3 | M3-R01 | G07 | borrow scope、generation、所有権移転、poison を適用 | — |
| M3-R03 | todo | 3 | M3-R02 | G07 | VM/native の error、取消、不正入力で resource cleanup を一致 | — |
| M3-P01 | todo | 2 | M3-I04 | G08 | typed import の明示 grant を必須とし欠落/不一致 capability を拒否 | — |
| M3-P02 | todo | 3 | M3-P01,M3-I05 | G08 | metadata/adapter 作業量を制限し不正 import を副作用前に拒否 | — |
| M3-E01 | todo | 3 | M3-I04,M3-P01 | G09 | native host が検証済み script を読み込み生成 typed API を呼び出し | — |
| M3-E02 | todo | 3 | M3-E01 | G09 | script から native host を呼び入れ子 guest callback を受信 | — |
| M3-E03 | todo | 3 | M3-E02,M3-R03 | G09 | typed failure、元の context、再入規則、poison を mode 間で維持 | — |
| M3-E04 | todo | 3 | M3-E03,M3-T07 | G10 | callback を跨いで logical mixed stack、取消、cleanup を維持 | — |
| M3-L01 | todo | 3 | M3-E03 | G11 | callback の確保を含む heap/allocation の共有上限を適用 | — |
| M3-L02 | todo | 3 | M3-T08,M3-E03 | G12 | guest/host を跨ぐ再帰/frame/live-task 合算上限を適用 | — |
| M3-L03 | todo | 3 | M3-E04,M3-T08 | G13 | VM/JIT/AOT と callback 境界で interruption を一貫して観測 | — |
| M3-L04 | todo | 3 | M3-L01,M3-L02,M3-L03,M3-P02 | G11,G12,G13 | 敵対的な資源枯渇でも必須 cleanup を完了 | — |
| M3-B01 | todo | 3 | M3-L04,M3-A04 | G14 | 両 tier-1 host で所有権/失敗/capability/資源適合試験に合格 | — |
| M3-B02 | todo | 3 | M3-B01 | G14 | C embedding API と生成 binding を移行試験付きで安定化 | — |
| M3-X01 | todo | 2 | M3-B02 | G15 | 権限を明示する trusted-native/verified-script の実行可能例を提供 | — |
| M3-Z01 | todo | 3 | M3-B01 | G03,G04,G05 | spawn/join/channel/取消/メモリ費用を記録し M2 workload の証拠を維持 | — |
| M3-Z02 | todo | 2 | M3-X01,M3-Z01 | G01,G02,G03,G04,G05,G06,G07,G08,G09,G10,G11,G12,G13,G14,G15 | 全ゲートを監査し command、証拠、制限を含む日英 capability guide を公開 | — |

## Progress and estimate changes

開始監査（2026-10-20 JST）: 39 作業、初期 **110 pt**。積上げの暫定予測です。ロードマップの **96 pt** は過去に付与したゲート重みです。開始時点では完了による消化も追加作業もありませんでした。範囲が変われば実装前に再見積りします。新しい source 実行 fixture のない前提作業が 2 件続いたら順序を再確認します。T02/T03 を際限のない scheduler framework にしません。

T01 は `8ab751b` で完了しました。RFC 0021 を採択し board の独立検証を追加しました。semantic model 29/29 と repository consistency 5/5 が成功。2 pt を消化し追加/再見積りはありません。ロードマップ監査: 全 15 ゲートを維持し、判断だけでは完了を証明しません。

T02 は `a543d23` で完了（2026-10-21 JST）。有界 FIFO kernel、invocation 別 handle、直接の子の join、live/total admission、協調取消、別 dispatch で確認する cleanup を実装しました。focused test 18 件で 512 階層の取消、境界位置の探索、失敗順、拒否時の非変更、cleanup entry の取消を検証します。host example は 42 を返し、通常/入れ子取消の frame を 2,1,0 の順に破棄し live task は 0 です。全 baseline は **96 suite / 成功 1,613 / 失敗 0 / 既存 timing ignore 1**、suite 時間合計 **439.11s**。fmt、Clippy（**1.08s**）、rustdoc（**26.32s**）も成功。RFC/index 更新後の文書整合性も成功。T02 の最後の残見積り 3 pt を消化します。

source scope 監査（2026-10-21 JST）: 初期 T03 は言語判断/grammar と独立した 2 つの source 証明をまとめていました。実際の frontend には resolved typing と lexical loan の別解析があり、その後に独立した MIR 検証があります。構文受入だけで scoped handle の寿命や suspension の borrow 禁止は証明できません。元の受入条件をすべて維持し、T03 の 3 pt を判断/lossless syntax に残して、実装前に **T09（3 pt）** と **T10（2 pt）** を明示します。純追加 **+5 pt**、行数 **39 → 41**。T04 は T10、A01 は T09 の型付き handle に依存します。残作業の新しい見積りであり、完了作業や T02 の遡及増額ではありません。

前提 2 件後のロードマップ/順序監査: T01/T02 で最初の host 成果を達成しましたが G03 は未完了です。全 15 ゲートと P1 作業を維持します。kernel/IDL 基盤の拡張を止め、**T03 → T09 → T10 → T04 → T05** で最初の実 source VM fixture、その後 T06 native parity へ進みます。新しい source/ABI/format の選択には引き続き accepted 判断が必要です。milestone の重みや出口条件は削減しません。

**110 + 5 − 5 = 110 pt**

| Initial remaining | Net additions/re-estimates | Retired at completion | Remaining |
| ---: | ---: | ---: | ---: |
| 110 pt | +5 pt | −5 pt | 110 pt |

## Exit audit

**0/15 ゲート完了、ゲート重み 0/96。** 作業の完了とゲートの完了は別です。G01 には task 構文、channel、Send/Share の決定が必要です。G14 には macOS AArch64 と Windows x86-64 で実際の cross-mode 適合試験が必要です。P1 作業も M3 に残します。

## Sources

- [ロードマップ](roadmap.md)、[architecture](architecture.md)、[完了済み M2 board](milestone-2.md)、[レビュー](review-2026-10-20.md): 最終更新 2026-10-20、`984557d` で 2026-10-20 JST に確認。
- [RFC 0005](rfcs/0005-structured-tasks-and-isolated-services.md)、最終更新 2026-10-21 と [RFC 0003](rfcs/0003-versioned-component-contracts.md)、最終更新 2026-10-17。契約を引き続き遵守し、prototype は production 完了の証拠にしません。
- [RFC 0021](rfcs/0021-bounded-structured-task-driver.md)、最終更新 2026-10-21。kernel、test、example を `a543d23` で 2026-10-21 JST に確認。source scope 監査は同じ checkpoint の `crates/hir/src/resolved.rs`、`crates/hir/src/program/`、`crates/hir/src/loans.rs`、`crates/semantics/src/analysis.rs` に基づきます。
