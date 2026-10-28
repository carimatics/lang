<!-- translation-of: ../../rfcs/0021-bounded-structured-task-driver.md -->

# RFC 0021: 上限付き構造化タスクドライバー

[English](../../rfcs/0021-bounded-structured-task-driver.md)

- Status: implemented
- Authors: Project contributors
- Created: 2026-10-20
- Last updated: 2026-10-28
- Tracking issue: M3-T01, M3-T02
- Supersedes:
- Superseded by:

## 現在の範囲

2026-10-28 JST照合。Kernel、source adapter、parked channel、子孫のcleanup、元taskでの同期callback scopeは実装済みで、[M3終了監査](../milestone-3-capabilities.md)に含まれます。以前は別途必要だったsource、Send／Share、channel、service、embeddingの判断はRFC 0022～0027が提供します。実行は1 invocation内のlogical taskのままです。以下の日付付き実装前提は当時のcheckpointを示し、M3の残作業ではありません。

E13 current-task借用の判断（2026-10-25 JST、RFC 0001実験段階の委任で受理）: 同期native callbackは、元Schedulerのactively running taskをcheckedな排他viewとして借用します。viewは元のbrand付きTaskId・parent/origin・取消要求・usage/limitを読み、取消要求は元Schedulerの同じtaskへ委譲します。root開始・task admission・counter複製・dispatch/finish/cleanup権限は与えません。入場でinvocation/slot/active/runningを検証し、ready/waiting/cleaning/completed/foreign taskは拒否します。nested reborrowは同じtask/stateを参照し、drop/失敗/unwindで元taskを完了または再dispatchしません。既存kernel transitionは不変です。ID/取消flagのコピーや新しいrootではlifecycleを共有できず、viewの代案として採用しません。次に既存VM/native frame executionへ接続し、同期childは元logical taskに属しtaskを再admitせず、元taskを閉じずに自身のscopeだけをcleanupします。実sourceの非root task callback・取消・反復・usage不増・全owner/loan cleanupで検証するまでE13は未完了です。task exportの拒否とE02/E03/E04/L04・公開Cの条件を維持し、syntax/永続形式は変更しません。未検証の実装仮定は、既存frame driverと元task lifecycleを別task生成なしに接続できることです。

E13同期call scopeの補足判断（2026-10-25 JST、実装前に委任受理）: 借用taskの実行adapterは、元Schedulerをnew/start/dispatchせず、元CurrentTaskとcall-localなready/running/cleanup/complete phaseだけを持ちます。既存Backend/frame executorを共有し、logical TaskId・task usage・取消/closing状態は元viewを参照します。scopeのfinish/cleanupはそのscopeだけを閉じ、元taskとready queueを完了・再dispatchしません。suspending/task exportは引き続き拒否します。native importは元task viewを借用し、ネストした同期callへ渡します。scope内の一時的なframe-record索引はlogical task identityと区別し、観測eventは元identityを使います。新しいlogical task・identity registry・source evaluatorは設けません。実sourceの非root callbackで、この分離と取消時のscope cleanup/親task継続を検証します。

## Summary

C02 統合 checkpoint（2026-10-21 JST）: channel Hub は C05 の private ticket と準備済み global FIFO 上で readiness/custody を持ちます。`Scheduler::is_quiescent` は未完 root があり active/ready dispatch がない状態を返し、closed host driver は tree 取消前に最古 Hub operation を typed ChannelDeadlock として記録します。実 send/receive/full-send/close/取消例は42または記録済み失敗を返し、descendant-first frame cleanup 後に live 0 になります。source/MIR/VM/native channel 受理は RFC 0025 の別段階で、外部 event 権限を推論しません。

C05 実装 checkpoint（2026-10-21 JST）: driver は invocation/task に紐付く opaque WaitId ticket と検証済み global ordinal を発行します。check_park は事前検査、park は dispatch 解放または pending 取消時の終了、wake は正確な parked activation を検証して既存 ready task の後ろに一度追加し ticket を即失効させます。Parked task は live のまま adapter 所有 frame を保持し、取消/child failure は descendant-first cleanup acknowledge 前に readiness を失効させます。kernel 26 テストで境界、active dispatch 維持、不正/foreign/duplicate/stale wake、storage を増やさない256回再利用、最終 u64 ordinal/枯渇を確認します。`cargo run --example parked-tasks` は 42、実 frame cleanup [1,0]、全park後取消の cleanup [2,1,0]、live task 0 を検証します。kernel は guest frame/message や park ごとの storage を allocation しません。channel readiness、registration/payload custody、publication、deadlock 診断、source/MIR/VM/native channel 操作は C02/C03/C04 に残り、adapter は value cleanup 前に登録を unlink します。channel profile と M3 gate は未完です。

C05 adapter 決定（2026-10-21 JST、実装前）: `check_park` は running task と invocation 全体の検証済み wait ordinal を adapter 公開前に事前検査します。`park` は allocation-free で Parked に移って `Park::Waiting(WaitId)` を返し、取消/child failure で終了する場合は ticket を発行せず `Park::Closing` を返します。opaque/non-Copy ticket は invocation-branded TaskId と唯一の ordinal を持ち、`wake(&WaitId)` は一致する parked task を ready 末尾へ一度追加して ticket を即失効させます。foreign、duplicate、取消済み、旧 generation の wake は queue/task 状態を変えず構造化失敗します。取消は Parked を child Waiting と同様に settle し、descendant join 後に cleanup します。wake 後の取消は既存 Ready/Running 境界規則に従います。ordinal 枯渇は active state を変える前に拒否します。admitted task ごとに1つの parked state で metadata を有限化し、新 allocation、guest frame、payload inventory、外部 event 権限を追加しません。channel adapter が operation span と registration custody を保持し、C02 が readiness と closed invocation の全停止を扱います。

C01 follow-up 決定（2026-10-21 JST）: [RFC 0025](0025-bounded-channels-and-backpressure.md) でこの invocation driver 上の channel park/wake と FIFO admission を受理しました。C05 は有限の invocation/task branded wait identity、allocation-free park/wake、二重 enqueue のない stale/duplicate wake 拒否、cleanup 前の取消時失効、安全な wait generation 枯渇を追加します。C02 が channel readiness/commit を担当し、kernel は guest message の所有/評価や MIR 解釈をしません。global ready FIFO と descendant join 後の cleanup を維持します。C01 決定時の committed driver は child-join 待機のみで、上記 C05 が有限 readiness 拡張を実装します。

RFC 0005 の下で、バックエンドに依存しないホスト駆動の scheduler kernel を追加します。kernel は親子関係とスケジュール状態、実行 adapter は guest frame、値、診断を所有します。明示的な root は 1 つで、同時に active な dispatch は最大 1 つです。ready task を FIFO で実行し、adapter が意味的境界を報告します。scope の終了は子を join し、取消/失敗では未完了の子孫へ取消要求を伝播します。guest cleanup は別の dispatch とし、実際の frame cleanup 後に確認します。

最初の production task driver の契約を決めます。source async 構文は [RFC 0022](0022-source-structured-tasks.md) で採択しました。Send/Share、channel、component 取消、公開 C embedding API は別の M3 判断です。この RFC の採択だけでは G01 も構造化 source 実行も完了しません。

実装 checkpoint（2026-10-21 JST）: `crates/runtime/src/task.rs` に kernel を実装しました。18 件の focused test で状態/上限/cleanup 契約を検証し、cleanup entry 前と実行中の取消も含めています。`cargo run --example structured-tasks` は実際の host 所有 frame で通常終了と入れ子取消を実行します（42、cleanup 2,1,0、live task 0）。全 fmt/Clippy/test/rustdoc baseline が成功し、96 suite、1,613 test、失敗 0、既存の timing ignore 1 件です。source/VM/native task 実行と公開 embedding は後述の別作業です。

## Motivation

`crates/semantics` の accepted prototype は単一 task の borrow と破棄をモデル化します。子のスケジュール、scope join、task admission の制限、実際の VM/native frame の駆動はありません。M2 の明示的 managed continuation と failure cleanup を、その private layout の解釈や Rust destructor の実行時機に依存せず dispatch する必要があります。

## Goals

- 明示的な scheduler 権限、上限付き admission、同一順序の event に対する決定的挙動。
- 協調取消、未完了の子の逸出禁止、子孫から親への cleanup。
- VM/JIT/AOT/host continuation に共通の契約。
- foreign、不正 handle、違法な遷移を変更前に検証。

## Non-goals

- 最初の実装には source 構文、coroutine、channel、service、thread、timer、外部 wakeup を含めません。
- fairness/latency 保証や任意の trusted host/native code の強制停止。
- 安定 Rust/C ABI、task handle のシリアライズ、guest allocator 計測、kernel 単独の sandbox 保証。

## Guide-level design

host は live/total task 上限を指定して scheduler を構築し、root を起動して dispatch を駆動します。resume では adapter が frame にアクセスし、直接の子の spawn、yield、join、finish を行います。cleanup dispatch では guest cleanup とその確認だけを行います。どちらの dispatch も active な間は `dispatch` の再入を拒否します。

```text
root resumes → spawn left → spawn right → join left
left resumes → yield
right resumes → finish(success)
left resumes → finish(success)
right cleanup → acknowledge
left cleanup → acknowledge
root resumes → read left/right results → finish(success)
root cleanup → acknowledge → root complete
```

結果は adapter の storage に保存します。scheduler の成功と guest の `Result::Err` は別で、後者は通常の typed value です。terminal panic/control/adapter failure は単に join して回復できません。

## Reference semantics

### Admission and identity

1 invocation に root は 1 つです。active に実行中の task だけが直接の子を spawn でき、detach はありません。handle は private invocation identity と単調増加 slot を持ちます。invocation 内で再利用せず、guest は構築/シリアライズできません。slot が同じでも別 scheduler の handle は無効です。

root/child/queue entry の公開前に live と累積 task 上限を確認します。root/running/suspended/joining/cleaning は live に数えます。拒否時に slot を消費しません。total 上限は完了後に保持する親子/結果 metadata と走査量も制限します。metadata/queue の有界容量を開始前に fallible 確保します。guest frame/result の確保には adapter の別の上限を適用します。

### Scheduling and join

状態は ready、running、直接の子の待機、全子の join、cleanup-ready、cleaning、complete です。ready と cleanup-ready は同じ FIFO queue を使い、task ごとの位置は最大 1 つです。spawn/yield は末尾へ追加します。完了済みの子への join でも queue に実行を譲ります。子の cleanup 確認後にだけ待機中の親を再開します。

`finish(success)` は通常の scope 終了を開始し、直接の子すべての complete を待ちます。未完了作業を捨てません。子が live な間は親 frame を cleanup できません。自分、兄弟、祖先、foreign task の join は状態変更前に拒否します。子の結果は検証済み move/retirement まで adapter が所有し、scheduler は clone しません。

### Cancellation and failures

取消要求は冪等で、未完了の子孫へ伝播します。dispatch entry、yield、join、adapter の明示 check で観測します。実行中の resume/cleanup callback は強制停止しません。実行中に届いた要求は次の境界で反映します。取消保留中の通常成功は cancelled になり、明示的 terminal instruction failure は主原因を維持します。成功した子の join 待ちの親も取消可能です。

子が terminal failure で完了したら未完了の兄弟へ取消要求し、親を次の境界で failure closure に移します。親自身の cleanup 後に上へ伝播します。親の明示的 terminal failure が主原因です。それ以外は全子完了後、spawn 順で最初の失敗した直接の子を cancellation/success より優先します。adapter は失敗の起点と副次/抑制失敗を保持します。typed guest error は terminal failure ではありません。

取消/失敗の終了では、取消した子孫を join してから親を cleanup します。すでに完了した子はそのままです。admission された task ごとに cleanup を一度だけ dispatch し、guest fuel は消費しません。adapter が RFC 0008 の初期化逆順 cleanup を実行し、任意の cleanup failure ID を返します。cleanup failure は成功を置き換えますが、既存 failure/cancellation は主原因として保持し、adapter が cleanup failure を副原因に記録します。確認後にだけ complete にして live slot を解放します。queue/report の打切りや guest control 枯渇で cleanup を省略できません。

kernel が検証するのは scheduling 遷移で、guest 型/loan/capability/結果 move/instruction fuel ではありません。VM/native adapter が独立して検証します。特に RFC 0005 に従い suspension 時の live shared/exclusive loan をすべて拒否します。

## Execution modes

kernel 自身は MIR/VM/native code を呼びません。driver が resume を準備済み continuation、cleanup を failure/retirement 経路へ対応させます。同じ順序の操作に対して、全 mode で親子関係、dispatch 順、取消観測、結果、cleanup 順を一致させます。最初の host fixture は kernel を実行します。source/VM/native fixture は T03〜T07 の独立した作業です。

host callback は暗黙の scheduler 権限を取得できず、active dispatch に再入できません。認可済み callback による取消要求や scheduling は後続 component profile が定義します。定数評価には scheduler はありません。WebAssembly component と multithread executor には契約を保つ別 adapter が必要です。

## ABI and compatibility

状態、queue layout、handle、failure ID は private な process 内 Rust interface です。この作業では source/MIR/bytecode/native ABI/package/C ABI を変更しません。将来の source task 操作には明示的 version 付き MIR/artifact 対応が必要で、旧 artifact に暗黙の task を追加しません。frame pointer/private handle を component value にしません。FIFO/観測/失敗規則を変える場合は RFC 改訂と差分移行 fixture が必要です。

## Diagnostics and tooling

構造化 error は busy/no active dispatch、foreign/invalid handle、wrong active task/phase、non-child join、duplicate root、cancellation pending、live/total limit、容量準備失敗を区別します。拒否は状態を変えません。adapter が検証済みの任意 MIR source span を起点として保持します。read-only status/parent/取消情報を公開します。formatter/LSP/source 診断、DAP 親子表示は後続作業です。

## Performance and cost model

invocation ごとに metadata/queue 容量を準備し、保持 entry は total task 上限以下とします。準備後の遷移に追加確保は不要です。初期実装は親子/状態の走査で join/取消を解決してよく、定数時間/throughput は保証しません。安定化前に total-task 規模と取消 fan-out を測定します。旧単一 task prototype から性能を推測しません。実 adapter を追加するまで M2 実行経路は変わりません。

## Security and trust boundaries

scheduler は trusted host facility であり、untrusted bytecode verifier ではありません。不正操作は副作用前に拒否し、private handle が invocation を区別します。上限は task 数と metadata を制限しますが、任意 callback の CPU/メモリは制限しません。adapter が共有 execution control を poll し spawn 前に frame を admission し、control 枯渇後も必須 cleanup を実行します。native process 内 sandbox を主張しません。fallible metadata 準備の失敗は root 公開前に報告します。

## Alternatives

- **変更なし:** prototype は構造化木の寿命や実 frame の駆動を提供しません。
- **library-only futures:** adapter 内では利用可能ですが、guest 所有権/loan の独立検証や等価な native cleanup を提供しません。
- **work-stealing multithread executor:** 並列化できますが、最初の source fixture より前に同期/wakeup/計測契約が必要です。accepted semantics の下で後続実装できます。
- **子を即時に完了まで実行:** 単純ですが suspension、同時 live admission の制限、queue 内の兄弟の取消を表せず、選択した成果を満たしません。

## Implementation plan

1. M3-T02: 有界 kernel と実際の host 駆動 spawn/yield/join/cancel/cleanup fixture。
2. M3-T03/T09/T10/T04: source 構文の判断/parse、task 型と suspension 所有権の検査、独立検証 typed MIR。
3. M3-T05/T06: VM、次に compiled native continuation で source fixture の event/span を一致。
4. M3-T07/T08: 実所有 frame cleanup と合算 execution control。kernel event だけでゲートを完了しません。

## Test plan

FIFO dispatch、通常/異常 scope 終了、入れ子、完了済み子の join、active resume/cleanup 中の取消、失敗順、cleanup failure、一度だけの確認を試験します。wrong invocation/parent/phase/再入は状態変更なしで拒否します。live/total 上限の境界と完了後の slot 保持を検証し、host fixture を実行可能な example にします。差分/source/ABI/fuzz/platform 検証は後続 board に従い、kernel 試験だけでは M3 完了にしません。

## Open questions

実装済みkernelとsource adapterにblockerはありません。Source構文、Send／Share、channel、service、production embeddingはRFC 0022～0027で決定済みで、[M3適合性検証](../milestone-3-capabilities.md)が予約、取消、元taskのcallback scopeを検証しています。走査／dispatchのcostはworkload依存の観測であり、並列executorやlatencyの保証ではありません。以下の判断は以前の受理時点の状態です。

判断: **RFC 0001 の実験段階の委任で accepted、2026-10-20 JST**。明示 driver は RFC 0005 を保ち、検証済み guest cleanup と scheduling を分離します。個別 owner 承認待ちは不要です。source 構文は RFC 0022 で採択し、channel/Send/Share と production embedding profile は未決定です。

## Sources

- [RFC 0005](0005-structured-tasks-and-isolated-services.md)、最終更新 2026-10-21: accepted task lifetime/取消規則と後続判断との対応。
- [RFC 0008](0008-cleanup-allocators-and-c-boundaries.md)、最終更新 2026-10-20: 実装済み cleanup/failure boundary。
- [RFC 0012](0012-native-ownership-continuations.md)、最終更新 2026-10-20: 実装済み native managed continuation。
- `crates/semantics/src/lib.rs`、`crates/runtime/src/execution.rs`、[architecture](../architecture.md) を `984557d` で 2026-10-20 JST に確認。単一 task semantic model の library test 29 件は同日に成功しました。baseline であり kernel 実装の証拠ではありません。
- 上記 scheduler の選択は project decision です。新たな外部言語/性能の主張はありません。

- C05 実装証拠: `crates/runtime/src/task.rs`、`task/tests/wait.rs`、`examples/parked-tasks.rs`。コードに更新日埋込みはなく、2026-10-21 JST に macOS AArch64 で確認/実行しました。wait ticket は private identity と ordinal を持ち、kernel は guest message を所有せず、新 channel Source/ABI/wire の証拠にはしません。
