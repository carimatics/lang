<!-- translation-of: ../milestone-3.md -->

# Milestone 3 delivery board

[English](../milestone-3.md) · [Exit gates](../roadmap.md#milestone-3-concurrency-and-embedding)

更新日: **2026-10-21 JST**。開始チェックポイント: `984557d`。M2 とリポジトリレビューは完了済みです。

**10 done · 1 doing · 32 todo · 93 remaining pt**

**次: M3-T07 → owned task local を実行し、入れ子の取消/失敗 cleanup を検証します。**

現在の作業: **M3-T07**。直近の実行検証: **`29badaa`、2026-10-21 08:18 JST**。`cargo run -- run examples/scripts/task_result_retirement.lang` は 42 を出力します。source/MIR テストは通常/入れ子 return 終了で loan → join → 当該 scope の通常 local cleanup の順序を維持し、VM/native region テストは遅い sibling の全 join まで結果を保持して公開逆順に retire します。全 baseline: **107 suites / 1,715 passed / 0 failed / 1 existing timing ignore**。fmt、warnings-denied Clippy、rustdoc、全 11 fuzz target の compile/lint が通過しました。T07 の残条件は実際の owned local と同期 owned-result move、実行区間内の通常 loan、retirement 前の入れ子 cancel/join、primary/secondary 失敗 report、exactly-once cleanup です。一般 async Send/Share transfer は A01、保持 result cell を含む完全な共有 control は T08 に残します。owned task 実行と task artifact/native package は副作用前の拒否を維持し、M3 exit gate は未完了です。

**今回: T12 完了（−2 pt）、追加 task/再見積りはありません。** 直前残り **95 − 2 = 93 pt**。全 **43 行**、累積消化 **26 pt**、先に公開した T12 前提を含む純追加見積り **+9 pt** です。

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
| M3-T07 | doing | 3 | M3-T12 | G03 | 所有 local、入れ子の取消、主/副失敗、exactly-once cleanup を検証 | — |
| M3-T12 | done | 0 | M3-T06 | G03,G13 | owned task 実行前に join 後の local cleanup と公開逆順の result retirement を保証する | `29badaa` |
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

MIR 表現監査（2026-10-21 JST）: T04 の従来 3 pt は未実装の型/effect 表現境界と task 命令/制御フロー証明を含んでいました。既存 ownership MIR は型、宣言、place、body、move/loan/region/cleanup を分けて検証し、`e4b17d0` に Task/async effect はありません。実装前に **T11（2 pt）** を追加し、採択した [RFC 0023](rfcs/0023-task-mir-boundaries.md) に従い既存 ownership graph/宣言を拡張して旧実行/artifact 昇格を明示拒否します。T04 の元の命令/scope/suspension/所有権の受入条件と 3 pt を維持し、T11 に依存させます。追加 **+2 pt**、行数 **41 → 42**、累積追加 **+7 pt**。消化はありません。別 task 言語/framework や channel/IDL の前提を追加せず、T11 は compile 実験で再監査し **T11 → T04 → T05 → T06** を source fixture へ進めます。

T11 を `ae9404d` で完了（2026-10-21 JST）: RFC 0023 を採択し、既存 ownership MIR graph に Task/TaskError、canonical property、ABI から独立した async effect を追加しました。hidden/入れ子/borrowed claim、不正 signature/local、偽装 effect span、async C export を独立検証で拒否し、通常 call query に async を消去させません。型/宣言の成功で task body は昇格しません。旧 place/body/runtime layout は未使用 task metadata も拒否し、汎用 token layout の fallback で task 表現を割り当てません。schema 2/3/4 writer は出力前に Task/TaskError/effect を拒否し、reader の同期 default と tag を維持します。追加 14 件は MIR 宣言 11 件、private codec 境界 2 件、layout 1 件です。MIR/bytecode の関連 15 suite・196 件、native compiler 適合 12/12 が通過し、厳密な SDK identity/mismatch 拒否を維持しました。最終 baseline 全体で production input を固定し、**100 suite / 成功 1,666 / 失敗 0 / 既存 timing ignore 1**、suite 時間合計 **462.67s**。fmt、warnings-denied Clippy、rustdoc、全 11 fuzz target の compile/lint が成功しました。fuzz campaign や source task 実行は主張しません。T11 の最後の 2 pt を消化します。先行して追加した表現の前提 +2 pt は完了に隠さず純追加 +7 pt に保持します。

T11 roadmap 監査: T04 の元の命令/scope/suspension/所有権・cleanup 成果と 3 pt を維持します。実装前に RFC 0023 へ具体的な命令/continuation 設計を記録し、**T04 → T05 → T06** で source VM 実行と compiled native parity を進めます。型だけの証明や旧 profile 拒否は G03 や他の出口ゲートの証明ではありません。全 15 gate、P1 成果、channel、Send/Share、production embedding を M3 に維持し、別 task 言語や scheduler/IDL framework を追加しません。

T04 を `700391d` で完了（2026-10-21 JST）: 実装前に RFC 0023 へ具体的な命令/continuation と cancellation plan の判断を記録しました。既存 ownership MIR の task entry が有界 lexical scope、local/prepared destination/CFG 引数経由の起点集合、move-only claim、suspension loan、return escrow を独立検証します。既存 ownership move/loan/arena region 証明も引き続き必要です。async failure plan は取消 entry と primary failure 記録を区別し、loan 終了、scope cancel/join、owner retirement、escrow 破棄の順を明示します。静的 plan であり guest cleanup の実行ではありません。旧 frame/VM/native/codec API は scalar async 証明も副作用/wire 出力前に拒否します。追加 19 件は raw MIR 17 件、private schema 2/3/4 writer 1 件、cross-adapter 境界 1 件です。入力を固定した最終 baseline は **102 suite / 成功 1,685 / 失敗 0 / 既存 timing ignore 1**、suite 時間合計 **471.29s**。fmt、warnings-denied Clippy、rustdoc、全 11 fuzz target の compile/lint が成功しました。既存 raw MIR fuzz target に task 命令/metadata の mutation を追加し、fuzz campaign や source task 実行は主張しません。先行する全体実行で native CLI 起動時の NotFound が 1 件ありましたが、production input や SDK 検査を変えず単独再実行と最終全 baseline が成功しました。T04 の最後の 3 pt を消化し、追加/再見積りはありません。

T04 roadmap 監査: 検証済み MIR 契約と、既存 source lowerer の Task 拒否・同期実行境界を照合しました。T05 は元の 3 pt の source fixture 成果を維持し、task 対応 source lowering と、証明を既存有界 driver へ接続する VM 経路を追加し、その後 T06 で同じ continuation を compile します。実際の owned local 取消/cleanup と共有 control は T07/T08 に残します。この source fixture 前に scheduler/IDL framework や表現の前提を追加せず、残り条件が増えたら再監査します。全 15 gate、全 P1 作業、channel、Send/Share、production embedding を M3 の要件に維持します。

T05 を `2f58084` で完了（2026-10-21 JST）: TaskProgram で source claim/loan 証明、task 対応 lowering、独立 MIR 検証を接続しました。明示 VM adapter が RFC 0021 driver 経由で既存 failure plan を実行し、通常 call、spawn/yield/check、消費 join、型付き取消、loop 再入場を区別する scope、子の terminal failure、保護された return escrow を扱います。managed layout/frame storage を共有し、task 証明を要する entry は payload 0 の TaskClaim marker と invocation 内 TaskId sidecar を使い、frame cleanup は型付き result reservation だけを分離します。子 frame admission 前に task/frame/cell を検査し、source/MIR/preparation budget も有界です。CLI check/run と LSP が接続済み task source を受け入れ、`cargo run -- run examples/scripts/tasks.lang` は **42** を出力し、実行 fixture で **admitted 3 / live task 0**、**live guest allocation 0** を確認しました。旧同期 API、task artifact/native package、async REPL submission の明示拒否を維持します。初期の到達可能 local は finite Copy/Task とし、owned/loan/foreign task 実行と finite service/共有 control の完成は後続作業です。追加 13 件は source-to-MIR 1 件と source/VM/layout/境界 12 件で、CLI/LSP の動作も更新しました。fuel/trace の各 prefix、task/frame 上限、provider の実呼出しごとの拒否で live task/storage が残らず、取消時機、sync/direct-await call、panic 起点、loop scope の破棄も成功しました。production input を固定した全 baseline は **104 suite / 成功 1,698 / 失敗 0 / 既存 timing ignore 1**、suite 時間合計 **467.18s**。fmt、warnings-denied Clippy、rustdoc、全 11 fuzz target の compile/lint が成功しました。fuzz campaign や native task 適合は主張しません。T05 の最後の 3 pt を消化し、追加/再見積りはありません。

T05 roadmap 監査: 最初の source VM fixture が実行できたため、G03/G12/G13 完了判定前に **T06 → T07 → T08** を進めます。`crates/codegen-clif/src/owned.rs` と `src/native.rs` を確認し、既存 native 経路は TaskProgram/async metadata を拒否する一方、検証済み MIR、managed task layout、scope identity、result storage が揃ったことを確認しました。T06 は元の 3 pt の compiled continuation と値/event 一致の成果を維持し、VM/MIR fallback なしに生成 code を実行する必要があります。初期 native fixture で serialized task revision や package/startup 公開を決めたことにはしません。明示 format 判断と native/owned/loan/foreign/共有 control の受入条件を維持し、次の実行 fixture 前に scheduler/IDL 基盤を追加しません。全 15 gate、全 P1 作業、channel、Send/Share、production embedding を M3 に残します。

**110 + 9 − 26 = 93 pt**

T06 を `8d6c283` で完了（2026-10-21 JST）: 実装前に RFC 0023 へ native 境界を記録しました。invocation/frame/claim/result 状態と scheduling/storage service を共有し、VM/native に別の resume/cleanup entry を持ちます。Cranelift が演算、分岐、失敗/取消 CFG、検査付き continuation 選択を生成し、実行時 MIR evaluator や VM 依存はありません。source fixture は VM/native ともに 42 を出力します。integration test 12 件で値/event/使用量/割り当て状態、取消、有限 record/enum と field 更新、通常 call、loop scope identity、子/同期 panic 起点、全 fuel/trace prefix、task/frame/cell/preparation 上限、各 provider 拒否を比較します。別途リンクした実行形式では join/aggregate/cancel/panic profile、再生成 object/compiler/SDK byte の一致、全 fuel prefix を検証し、JIT compile/invoke API を呼びません。private 境界 2 テストで callback 前の範囲外 PC 拒否と source 証拠付き callback panic の封じ込めを確認します。受理した storage/task はすべて解放します。全 baseline: **106 suites / 1,712 passed / 0 failed / 1 existing timing ignore**、完了 suite 時間合計 **471.21 s**。fmt、warnings-denied workspace Clippy **6.24 s**、rustdoc **26.34 s**、全 11 fuzz target の compile/lint **3.11 s** が通過しました。fuzz campaign、性能主張、より広い platform/native package 完了ではありません。T06 の 3 pt を消化し、作業追加/再見積りはありません。

T06 roadmap 監査: G03/G12/G13 の完了前に **T07 → T08** へ進みます。task `Frame::transfer`、finite profile の admission、既存 VM ownership state、native production managed primitive、`Reservation::move_root_into`、retirement/cleanup context を確認しました。T07 は元の 3 pt の観測可能な成果を維持し、残条件は実際の managed ownership による対応 owned local と同期 owned-result move、owner/escrow retirement 前の入れ子取消 join、失敗時の順序付き primary/secondary report と exactly-once cleanup です。現在の Copy transfer/forget 経路は ownership の代用になりません。次の owned source fixture で既存 managed service/cleanup 機構を再利用し、別の観測可能な前提が必要なら実装前に公開/再見積りします。一般 async transfer は A01、完全な共有 control は T08 に残します。全 15 gate、全 P1、channel、Send/Share、production embedding を M3 に維持します。

T07 前提監査（2026-10-21 JST）: `03311f8` の lowerer は task scope body/return の Cleanup を TaskScopeExit より前に出し、runtime pending join は後続 child の join 前に先行 result を破棄しています。Copy-only 実行では owner callback がなく、所有値対応時に RFC 0022 の join → 公開逆順 result retirement → ordinary local cleanup を満たせません。owned source fixture は check に成功して実行が副作用前の Unsupported で止まることを再現しました。所有値を受理する前に必要な独立の正しさ修正を **T12（2 pt）** として公開し、**T06 → T12 → T07 → T08** に変更します。T12 は新しい source/MIR 順序テストと VM/native の実行時 region 解放テストを成果とし、scheduler/IDL 基盤は追加しません。T07 の owned move/loan/cancel/primary-secondary/exactly-once 契約と全 M3 gate は維持します。追加 **+2 pt**、**42 → 43 行**、net **+7 → +9 pt**、完了による消化はありません。

T12 を `29badaa` で完了（2026-10-21 JST）: source の暗黙 task join で loan/reference/claim を先に終え、当該 scope の通常 local cleanup を join 後に lower します。return は result を一度保護して内側 scope の join/cleanup から外側へ進みます。runtime は事前確保した scope/result link で公開順を保持し、全 join 後に公開逆順で未回収 result を retire します。await 済みの穴、loop の scope identity、失敗/取消/abort も同じ inventory を使い、backend の RetireFailure より前に処理します。新規 3 テストは source/MIR の通常/入れ子 return、VM/native の公開順両方向と遅い sibling を検証します。修正前の provider region は先行結果から解放されていました。実行 fixture は 42 を返し、既存 task 24 テスト、JIT/別途リンク object、fuel/trace/provider 拒否も通過しました。全 baseline: **107 suites / 1,715 passed / 0 failed / 1 existing timing ignore**、完了 suite 時間合計 **474.84 s**。fmt、warnings-denied Clippy（最終 cached workspace pass **0.78 s**）、rustdoc **31.05 s**、全 11 fuzz target compile/lint **1.51 s** が通過しました。fuzz campaign や性能主張ではありません。先に追加した T12 の 2 pt を消化し、追加作業/再見積りはありません。

T12 roadmap 監査: **T07 → T08** を再開します。`tests/fixtures/task_owned.lang` は解析/lowering を通りますが実行前の拒否を維持しており、owner token を Copy/forget で処理してはいけません。次は既存 managed move/retirement/cleanup 機構を使い、実際の owned local/同期 result/通常 loan と入れ子取消、primary/secondary report、exactly-once cleanup を実行で検証します。今回確認した retained result の logical live-cell 計上（`Machine::retired` は分離 result cell も frame と一緒に引いている）は既存 T08 の共有 control 条件に残し、新規 task として数えません。一般 async transfer は A01、artifact/production embedding は各判断に残します。全 15 gate、全 P1、channel、Send/Share、production embedding を M3 に維持し、owned source fixture より先に scheduler/IDL 基盤を増やしません。

| Initial remaining | Net additions/re-estimates | Retired at completion | Remaining |
| ---: | ---: | ---: | ---: |
| 110 pt | +9 pt | −26 pt | 93 pt |

## Exit audit

**0/15 ゲート完了、ゲート重み 0/96。** 作業の完了とゲートの完了は別です。source async 構文は採択済みですが、G01 には channel/backpressure と Send/Share の決定が残っています。G14 には macOS AArch64 と Windows x86-64 で実際の cross-mode 適合試験が必要です。P1 作業も M3 に残します。

## Sources

- 開始時の参照: `984557d` 時点の[ロードマップ](roadmap.md)、[architecture](architecture.md)、[完了済み M2 board](milestone-2.md)、[レビュー](review-2026-10-20.md)。当時の版の最終更新は 2026-10-20、確認日は 2026-10-20 JST です。T09 の roadmap/architecture 再確認は 2026-10-21 JST です。
- [RFC 0005](rfcs/0005-structured-tasks-and-isolated-services.md)、最終更新 2026-10-21 と [RFC 0003](rfcs/0003-versioned-component-contracts.md)、最終更新 2026-10-17。契約を引き続き遵守し、prototype は production 完了の証拠にしません。
- [RFC 0021](rfcs/0021-bounded-structured-task-driver.md)、最終更新 2026-10-21。kernel、test、example を `a543d23` で 2026-10-21 JST に確認。source scope 監査は同じ checkpoint の `crates/hir/src/resolved.rs`、`crates/hir/src/program/`、`crates/hir/src/loans.rs`、`crates/semantics/src/analysis.rs` に基づきます。
- [RFC 0022](rfcs/0022-source-structured-tasks.md)、最終更新 2026-10-21。syntax/module admission/REPL/CLI・LSP fixture を `0655939` で 2026-10-21 JST に確認。grammar/formatter 契約と現在の実行制限は同 RFC に区別して記録しています。

- T09 の根拠: [RFC 0022](rfcs/0022-source-structured-tasks.md)、`crates/hir/src/tasks.rs`、`crates/semantics/tests/tasks.rs`。最終更新 2026-10-21。`390e3ca` を 2026-10-21 JST に検証しました。
- T10 の根拠: [RFC 0022](rfcs/0022-source-structured-tasks.md)、`crates/hir/src/loans.rs`、`crates/hir/src/tasks.rs`、`crates/semantics/tests/task_loans.rs`。最終更新 2026-10-21、`9f7012d` を 2026-10-21 JST に確認しました。MIR 境界監査は `crates/mir/src/unit/ownership/declarations.rs` と `bodies.rs` に基づきます。埋込み更新日の記載はなく、同じ checkpoint/日付で確認しました。
- [RFC 0023](rfcs/0023-task-mir-boundaries.md)、最終更新 2026-10-21。採択した in-memory task MIR/layout/cleanup 契約と初期 source VM 実装。native/owned task 実行と serialized task 形式は未証明です。
- T11 の根拠: [RFC 0023](rfcs/0023-task-mir-boundaries.md)、`crates/mir/src/unit/ownership.rs`、`ownership/declarations.rs`、`crates/bytecode/src/ownership/`、`crates/runtime/src/layout.rs`。最終更新 2026-10-21、`ae9404d` を 2026-10-21 JST に確認/検証しました。
- T04 の根拠: [RFC 0023](rfcs/0023-task-mir-boundaries.md)、`crates/mir/src/unit/ownership/tasks.rs`、`ownership/failure.rs`、`crates/mir/tests/tasks.rs`、`tests/task_mir_boundary.rs`。code の埋込み更新日はなく、`700391d` を 2026-10-21 JST に確認/検証しました。T05 の順序は同じ checkpoint の source lowerer と VM 境界に基づきます。
- T05 の根拠: [RFC 0022](rfcs/0022-source-structured-tasks.md)、[RFC 0023](rfcs/0023-task-mir-boundaries.md)、`crates/semantics/src/analysis.rs`、`ownership_types/bodies.rs`、`crates/runtime/src/layout/`、`crates/vm/src/unit/tasks/`、`tests/task_execution.rs`、`examples/scripts/tasks.lang`。code の埋込み更新日はなく、`2f58084` を 2026-10-21 JST に確認/実行/検証しました。T06 監査は同じ checkpoint の native codegen/公開境界に基づきます。
- T06 の根拠: [RFC 0023](rfcs/0023-task-mir-boundaries.md)、`crates/runtime/src/task/runner/`、`crates/codegen-clif/src/tasks.rs`、`tasks/object.rs`、`tasks/tests.rs`、`tests/native_task_execution.rs`、`tests/support/task_static.rs`、`examples/native-structured-tasks.rs`。code の埋込み更新日はなく、`8d6c283` を 2026-10-21 07:29 JST に macOS AArch64 で確認/実行/検証しました。T07 監査も同じ時点の ownership move/cleanup interface に基づきます。
- T12 の根拠: [RFC 0022](rfcs/0022-source-structured-tasks.md)、[RFC 0023](rfcs/0023-task-mir-boundaries.md)、`crates/semantics/src/ownership_types/bodies.rs`、`crates/runtime/src/task/runner/`、`crates/vm/src/unit/tasks/machine.rs`、`tests/task_retirement.rs`、`tests/fixtures/task_owned.lang`、`examples/scripts/task_result_retirement.lang`。RFC は最終更新 2026-10-21、code は埋込み更新日なし、`29badaa` を 2026-10-21 08:18 JST に確認/実行/検証しました。owned fixture は静的証明、provider region の実行は finite Copy task の証拠です。
