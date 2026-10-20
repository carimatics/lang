<!-- translation-of: ../milestone-3.md -->

# Milestone 3 delivery board

[English](../milestone-3.md) · [Exit gates](../roadmap.md#milestone-3-concurrency-and-embedding)

更新日: **2026-10-21 JST**。開始チェックポイント: `984557d`。M2 とリポジトリレビューは完了済みです。

**5 done · 1 doing · 35 todo · 102 remaining pt**

**次: M3-T04 → source VM fixture 前に task MIR の scope/suspension/所有権 edge を独立検証します。**

現在の作業: **M3-T04**。直近の実行検証: **`9f7012d`、2026-10-21 03:29 JST**。source claim と lexical suspension loan の証明を実行昇格なしで結合しました。追加 11 件で明示/暗黙 point での両 loan 種、一時引数、return/失敗 edge、reborrow/unsafe、branch/loop/pattern 状態、資源上限、旧 MIR の拒否を検証しました。全 baseline は **98 suite / 成功 1,652 / 失敗 0 / 既存 timing ignore 1**。fmt、warnings-denied Clippy、rustdoc、11 fuzz target の compile/lint も成功。T04 の残る受入条件: 独立 task MIR の effect/type/scope、suspension continuation、所有権/cleanup edge、不正入力検証。必要な MIR 証明と execution adapter の接続までは SRC0026 を維持します。source task 実行と M3 ゲートは未完了です。

**今回: T10 完了（−2 pt）、新しい作業/見積りの追加なし。** 前回残り **104 − 2 = 102 pt**。全 **41 行**、累積消化 **13 pt** と追加見積り **+5 pt** は別に記録します。

## Deliveries

見積りは残作業量であり経過時間ではありません。各行は観測可能な 1〜3 pt の作業です。後続行は実装前に実際のインターフェースに照らして再確認します。`G01`〜`G15` はロードマップのゲート順です。Evidence 欄には完了した実装のコミットだけを記載します。

| ID | State | Remaining | Needs | Gates | Observable acceptance | Evidence |
| --- | --- | ---: | --- | --- | --- | --- |
| M3-T01 | done | 0 | — | G01 | FIFO ドライバー、親子関係、join、取消、失敗、上限付き admission の規則を採択 | `8ab751b` |
| M3-T02 | done | 0 | M3-T01 | G03,G12 | ホスト駆動の上限付きタスク木で join、協調取消、子から親への retirement を実行 | `a543d23` |
| M3-T03 | done | 0 | M3-T01 | G01,G03 | source async 契約を採択し lossless parse/recovery/format で task 構文を保持 | `0655939` |
| M3-T09 | done | 0 | M3-T03 | G03 | async call と scope 付き task handle を bind/type し、逸出と不正 async C surface を拒否 | `390e3ca` |
| M3-T10 | done | 0 | M3-T09 | G03 | suspension を跨ぐ shared/exclusive loan を両方の起点の診断付きで拒否 | `9f7012d` |
| M3-T04 | doing | 3 | M3-T10 | G03 | typed MIR の suspension、task scope、所有権 edge を独立検証 | — |
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

T03 は `0655939` で完了（2026-10-21 JST）。RFC 0022 を採択し、厳密な task keyword と専用 async/scope/prefix/statement CST node、有界 recovery、優先順位、空白だけを変える formatter を実装しました。`async: T` は予約 parameter 名として局所診断し、`async fn` は後続宣言の回復境界として保持します。14 件の回帰試験で task 構文、comment/span、keyword 移行、深さ上限、file/virtual/cached module/CLI/REPL/LSP の未対応 profile 拒否を検証しました。LSP format は SRC0026 を維持し、REPL の拒否は既存定義を保持します。候補 fixture は構文 error から、parse 成功と明示的な未対応 profile 診断へ変わりました。全 baseline は **96 suite / 成功 1,627 / 失敗 0 / 既存 timing ignore 1**、suite 時間合計 **430.74s**。fmt、Clippy（**39.56s**）、rustdoc（**36.94s**）、11 fuzz target の compile/lint（**3.22s**）も成功。fuzz target の compile は fuzz campaign の実行ではありません。日英 metadata/code 例/local reference と repository consistency も成功。T03 の最後の残見積り 3 pt を消化し、追加は +5 pt、行数は 41 のままです。

T03 ロードマップ監査: source async 判断は G01 の一部です。channel/backpressure、Send/Share、production embedding、source 証明、cross-mode 実行を引き続き要し、全 15 ゲートを未完了のまま維持します。次は T09 → T10 → T04 → T05 → T06 の source/VM/native 経路です。accepted format 拡張と適合証拠なしに Task を既存 C/serialized profile に公開したり artifact/startup 作業を消化したりしません。T09 は明示分割した 3 pt の型/scope 成果を維持し、今回の構文作業で新たな前提/見積りは追加しません。

T09 を `390e3ca` で完了（2026-10-21 JST）: resolved async effect、Task/TaskError 型、await の Result 型、call/move/consume 診断、有界 lexical scope provenance を実装しました。Task claim の scope escape、borrow/match、container/入れ子格納、parameter/result/C surface を拒否します。既存 finite C 検査でも TaskError を拒否します。初期 transfer profile は scalar/finite Copy 値に限定し、owned Send/Share は A01 です。owned 値自体が送信不能という主張ではありません。Result unwrap の明示 generic は既存構文に従い、opaque Task の match は既存 loan semantics に従います。追加 14 件で branch/loop、raw ID/span、work/allocation/diagnostic 上限、実行/loan 証明との分離を検証しました。既存 file/virtual/cached module と CLI/REPL/LSP は transactional な拒否を維持し、妥当な source も SRC0026 を受けます。全 baseline は **97 suite / 成功 1,641 / 失敗 0 / 既存 timing ignore 1**、suite 時間合計 **437.84s**。fmt、warnings-denied Clippy、rustdoc、全 11 fuzz target の compile/lint が成功しました。raw HIR fuzz generator に async effect、Task/TaskError、spawn/await を加えましたが、fuzz campaign の実施は主張しません。T09 の最後の 3 pt を消化し、追加/再見積りはありません。

T09 roadmap 監査: 全 15 gate と P1 成果は引き続き必要です。既存 lexical loan engine と有界 task driver を使い **T10 → T04 → T05 → T06** を進めます。次の source fixture 前に scheduler/IDL 基盤を追加しません。source task 実行、独立 MIR 証明、native parity は未証明です。

T10 を `9f7012d` で完了（2026-10-21 JST）: lexical loan engine が task expression/scope を扱い、await/yield/cancel_check/暗黙 join で生存 shared/exclusive loan を SRC0028 と loan 起点付きで拒否します。制御フローの合流で状態が捨てられる前に return/失敗 edge を検査し、終了する lexical loan を自身の join 前に終え、入れ子 join では外側 loan を維持します。spawn/cancel は suspension point ではありません。TaskLoans に不変の source claim/loan 証明を結合します。妥当な source も SRC0026 を受け、pure async と Task 値を使う source の両方を旧 ownership MIR 境界で拒否します。追加 11 件と既存の関連 47 件で規則、call 引数順、一時 loan の終了、reborrow、unsafe、branch/loop/pattern、source identity、work/allocation/depth/loan/diagnostic 上限を検証しました。全 baseline は **98 suite / 成功 1,652 / 失敗 0 / 既存 timing ignore 1**、suite 時間合計 **440.07s**。fmt、warnings-denied Clippy、rustdoc、全 11 fuzz target の compile/lint が成功しました。fuzz campaign や source task 実行は主張しません。T10 の最後の 2 pt を消化し、追加/再見積りはありません。

source 前提 T09/T10 の 2 件完了後の roadmap 監査: source claim/loan は必要ですが G03 の証明ではありません。既存 MIR の ownership declaration/body 検証は別の境界です。T04 は HIR 証明を MIR 証明として扱わず、実際の task scope/suspension/所有権 edge と独立検証を追加します。最初の source VM fixture と native parity へ **T04 → T05 → T06** を進め、間に scheduler/IDL framework を追加しません。全 15 gate、全 P1 成果、現在の transfer/ABI/format 制限を維持します。

**110 + 5 − 13 = 102 pt**

| Initial remaining | Net additions/re-estimates | Retired at completion | Remaining |
| ---: | ---: | ---: | ---: |
| 110 pt | +5 pt | −13 pt | 102 pt |

## Exit audit

**0/15 ゲート完了、ゲート重み 0/96。** 作業の完了とゲートの完了は別です。source async 構文は採択済みですが、G01 には channel/backpressure と Send/Share の決定が残っています。G14 には macOS AArch64 と Windows x86-64 で実際の cross-mode 適合試験が必要です。P1 作業も M3 に残します。

## Sources

- 開始時の参照: `984557d` 時点の[ロードマップ](roadmap.md)、[architecture](architecture.md)、[完了済み M2 board](milestone-2.md)、[レビュー](review-2026-10-20.md)。当時の版の最終更新は 2026-10-20、確認日は 2026-10-20 JST です。T09 の roadmap/architecture 再確認は 2026-10-21 JST です。
- [RFC 0005](rfcs/0005-structured-tasks-and-isolated-services.md)、最終更新 2026-10-21 と [RFC 0003](rfcs/0003-versioned-component-contracts.md)、最終更新 2026-10-17。契約を引き続き遵守し、prototype は production 完了の証拠にしません。
- [RFC 0021](rfcs/0021-bounded-structured-task-driver.md)、最終更新 2026-10-21。kernel、test、example を `a543d23` で 2026-10-21 JST に確認。source scope 監査は同じ checkpoint の `crates/hir/src/resolved.rs`、`crates/hir/src/program/`、`crates/hir/src/loans.rs`、`crates/semantics/src/analysis.rs` に基づきます。
- [RFC 0022](rfcs/0022-source-structured-tasks.md)、最終更新 2026-10-21。syntax/module admission/REPL/CLI・LSP fixture を `0655939` で 2026-10-21 JST に確認。grammar/formatter 契約と現在の実行制限は同 RFC に区別して記録しています。

- T09 の根拠: [RFC 0022](rfcs/0022-source-structured-tasks.md)、`crates/hir/src/tasks.rs`、`crates/semantics/tests/tasks.rs`。最終更新 2026-10-21。`390e3ca` を 2026-10-21 JST に検証しました。
- T10 の根拠: [RFC 0022](rfcs/0022-source-structured-tasks.md)、`crates/hir/src/loans.rs`、`crates/hir/src/tasks.rs`、`crates/semantics/tests/task_loans.rs`。最終更新 2026-10-21、`9f7012d` を 2026-10-21 JST に確認しました。MIR 境界監査は `crates/mir/src/unit/ownership/declarations.rs` と `bodies.rs` に基づきます。埋込み更新日の記載はなく、同じ checkpoint/日付で確認しました。
