<!-- translation-of: ../../rfcs/0025-bounded-channels-and-backpressure.md -->

# RFC 0025: Bounded channels and backpressure

[English](../../rfcs/0025-bounded-channels-and-backpressure.md)

- Status: accepted
- Authors: project contributors
- Created: 2026-10-21
- Last updated: 2026-10-21
- Tracking issue: M3-C01, M3-C02, M3-C03, M3-C04, M3-C05, M3-C06, M3-A02
- Supersedes: none
- Superseded by: none

## Summary

C06 型 checkpoint（2026-10-21 JST）: 予約された opaque `core::ChannelSetup<T>`、`core::Sender<T>`、`core::Receiver<T>` の identity を source/HIR と独立検証する MIR に保持します。既存 graph で有限な Send/Share 依存を別々に導出し、Sender の両 capability は message の Send、Receiver は Share 不可、Setup は両方不可、shared/weak は target の両 capability を要します。全 role は non-Copy で退役が必要であり、未使用 message constructor も Send を要します。重点11件は再帰/宣言順 graph、alias、不正 message/arity、move、偽造 MIR、proof resource の上限境界/1 未満、未対応 profile の admission を検証します。schema-2/3/4 writer は constructor byte 前に拒否し、実行 layout、VM/native channel admission、両 target の native package は未使用 endpoint も guest/provider 副作用前に拒否します。これは型 capability の証明であり channel 実行ではありません。具体 call/error/MIR operation、suspension loan proof、guest allocator/cell 統合、VM/native fixture は C03/C04 に残り、wire/C layout や M3 gate は完了しません。

C02 trusted-host runtime checkpoint（2026-10-21 JST）: `runtime::channel::Hub<T>` は有限 MPSC、opaque setup/sender/receiver 権限、検証済み累積 endpoint identity、pending/commit 済み値の ownership、FIFO sender 先頭予約、private readiness ticket を実行します。seal/last-producer closure は commit 済み値を drain し、abort は unlink 後に既存 Session/Cleaner で FIFO 退役し、panic quarantine と有限 report を維持します。共有 ServiceWork は preparation、clone、attempt、commit、実 publication 検査を課金し、取消/wakeup/必須退役は非計量です。重点17件で owned allocation identity、late arrival/2空きslot、wake/commit 前後の取消、動的循環の再検査、旧/foreign claim、metadata/position/count 上限、pool 再利用、最古 deadlock 起点、即時/待機転送の全 work prefix を検証します。`cargo run --example bounded-channels` は42、payload [20,22]、frame [1,0]、live 0、取消は [9,7] の退役、typed deadlock は cleanup 前の起点記録を確認します。有限の trusted Rust host profile であり、guest allocator/cell に基づく reservation、独立 source/MIR endpoint capability と loan proof、具体 core call/error、VM/native channel adapter、persisted/foreign profile は C03/C04 と監査する前提に残ります。M3 gate は未完です。

C02 adapter 決定（2026-10-21 JST、実装前）: 既存 scheduler と clone した invocation ServiceWork 上に型付き trusted-host channel Hub を追加します。opaque Setup/Sender/Receiver key は Hub の brand と role/live ticket で検証し、累積の channel/handle preparation 上限で index 再利用/ABA を防ぎます。queue storage と waiter pool は公開前に有限予約します。owned host payload は commit 済み entry と task ごとの pending operation を move し、拒否/取消では同じ owner を返すか退役し、source binding を復元しません。trusted payload visitor は実際の strong receiver edge を列挙して有限 cycle 検査を行い、Sendable/cycle-free bit を受け取りません。明示 abort は値を FIFO で除いてから呼出側の既存 cleanup Session/Cleaner に渡し、失敗後も続けます。C03 受理前に source-owned reservation、allocator/cell 計上、独立 Send/Share/loan proof、具体 source/error/MIR form を接続します。host interface だけで source/serialized/foreign 権限を与えません。host Rust storage は Hub の channel/slot/handle/waiter 上限で有限化し、guest heap 統合の完成は主張しません。

C05 実装 checkpoint（2026-10-21 JST）: driver は invocation/task に紐付く opaque WaitId ticket と検証済み global ordinal を発行します。check_park は事前検査、park は dispatch 解放または pending 取消時の終了、wake は正確な parked activation を検証して既存 ready task の後ろに一度追加し ticket を即失効させます。Parked task は live のまま adapter 所有 frame を保持し、取消/child failure は descendant-first cleanup acknowledge 前に readiness を失効させます。kernel 26 テストで境界、active dispatch 維持、不正/foreign/duplicate/stale wake、storage を増やさない256回再利用、最終 u64 ordinal/枯渇を確認します。`cargo run --example parked-tasks` は 42、実 frame cleanup [1,0]、全park後取消の cleanup [2,1,0]、live task 0 を検証します。kernel は guest frame/message や park ごとの storage を allocation しません。channel readiness、registration/payload custody、publication、deadlock 診断、source/MIR/VM/native channel 操作は C02/C03/C04 に残り、adapter は value cleanup 前に登録を unlink します。channel profile と M3 gate は未完です。

RFC 0005/0021/0022/0024 に対し、同一 invocation 内の有限 multiple-producer, single-consumer channel を定めます。full send は未commit payload を所有して待ち、成功 enqueue で FIFO の1位置に所有権を commit します。receive は最古の commit 済み payload を一度だけ move します。close、取消、resource 拒否、closed invocation の停止に明示結果と必須 cleanup を定めます。Sender 権限から receiver/service state を参照できません。

本書は protocol の決定であり、実装済み channel profile や安定 host ABI ではありません。C02 は runtime protocol、C03 は独立検証する source/MIR と VM 操作、C04 は生成 native/linked-object parity を担当します。具体的 reserved-core call catalog は C03 実装前に下記へ記録しています。別 task 言語や未検証の同期 call で suspension を隠しません。

## Motivation

`224c1f2` では owned async 引数/結果を実行できますが、production scheduler の待機は child join のみで channel park/wake はありません。`semantics::ServiceModel` は呼出側指定 Sendable と非有限 mailbox を使う以前の意味論実験です。どちらも有限 production channel を実装しません。RFC 0005 は full-mailbox 方針を未決とし、受理済み message の FIFO、owner isolation、取消時の破棄を要求します。

## Goals

- 有限 capacity、commit 済み値の FIFO、blocked sender の FIFO admission。
- preparation、waiting、publication、receive、retirement の全段階で payload owner を1つに保つこと。
- VM/native 共通の close/取消/失敗優先順位と計上。
- backend code と独立した source/MIR capability、effect、loan proof。

## Non-goals

capacity zero の rendezvous、複数 consumer、select、timer、priority、broadcast、detached task、OS thread、invocation 間 handle、remote transport、公開 C/component channel ABI。actor state projection や汎用 future/closure interface は定めません。

## Guide-level design

以下は protocol trace であり、実行可能な source spelling ではありません。

```text
capacity = 1
send(A)           -> committed at position 0
send(B)           -> waiting; B remains owned by the sending operation
receive()         -> A; oldest sender becomes eligible
resume send(B)    -> committed at position 1
close receiver    -> seal sends; keep committed values available
receive()         -> B
receive()         -> closed
```

send operand は評価時に通常 source move 解析で消費します。closed send は所有 payload を失敗値に返し、move 済み source 変数を復元しません。送信 task の取消では pending payload を退役します。enqueue commit 後の sender 取消は message を取り消しません。

source 構築は RFC 0009 の whole-value extraction と borrowed match を使い、owning tuple/field destructuring を仮定しません。構築 protocol は唯一の local setup 権限を使い、有限 storage を作り、sender ticket を明示発行し、setup を唯一の receiver に消費します。Setup は一時的 producer keepalive を1つ持ち、変換時に解放します。sender を発行していない receiver は既に send-closed です。setup の破棄は channel を abort します。構築 semantics をここで選び、具体的な名前/signature/失敗 sum は実装前の call catalog で決めます。

## Reference semantics

### Source catalog and typed MIR

C03 catalog 決定（2026-10-21 JST）: **実装前に RFC 0001 の実験的委任で採択**します。既存の予約 `core` namespace、明示型引数、`await` 構文を使います。channel message の T は Send と storable を独立証明し、data のみを抽出する `channel_unsent` は既存の任意の storable T を受け付けます。通常の同期構築/検査に suspension を隠さず、send/receive は async 関数内の直接 `await` を要し、spawn、future としての保存、同期 call を認めません。

| Source call (explicit `::<T>`) | Parameters | Result | Effect |
| --- | --- | --- | --- |
| `core::channel_new` | `&core::Allocator`, `i64` capacity | `core::Result<core::ChannelSetup<T>,core::ChannelError>` | synchronous construction |
| `core::channel_sender` | `&core::ChannelSetup<T>` | `core::Sender<T>` | synchronous explicit mint |
| `core::channel_clone` | `&core::Sender<T>` | `core::Sender<T>` | synchronous explicit clone |
| `core::channel_receiver` | `core::ChannelSetup<T>` | `core::Receiver<T>` | synchronous consuming conversion |
| `core::channel_seal` | `&mut core::Receiver<T>` | `core::Unit` | synchronous idempotent seal |
| `core::channel_send` | `&core::Sender<T>`, `T` | `core::SendResult<T>` | direct async intrinsic |
| `core::channel_receive` | `&mut core::Receiver<T>` | `core::Option<T>` | direct async intrinsic |
| `core::channel_unsent` | `core::SendResult<T>` | `T` | synchronous whole-value extraction |

`core::ChannelError` は有限 Copy sum で、case 順は `InvalidCapacity`、`Allocation(core::AllocError)` です。不正 capacity は storage allocation 前に返し、allocation 拒否では setup/endpoint を公開しません。channel/slot/handle/waiter/cell/control 上限と identity 枯渇は execution failure であり、構築成功や架空の domain error にしません。`core::SendResult<T>` の case 順は `Sent`、`Closed(T)`、`InvalidPublication(T)` で、通常の inline sum と同様に T から Copy/drop/Send/Share を導出します。この結果は data であって endpoint 権限ではなく、既存 core-sum 規則で case を構築/match できます。`channel_unsent` は結果全体を consume してどちらかの失敗 case の唯一 payload を move し、Sent なら既存の wrong-case unwrap 方針で panic します。owning partial move を追加しません。receive は commit 済み owner に Some(T)、drain 済み closed にだけ None を返します。

直接の `await core::channel_send::<T>(...)` は一度評価して SendResult<T>、`await core::channel_receive::<T>(...)` は Option<T> を返します。現在 task の操作であり child Task claim ではないため、child await の `Result<_,TaskError>` wrapper を付けません。現在 task の取消は terminal で pending owner を退役し、Closed、None、架空の T を返しません。即時完了でも suspension/cancellation 境界です。裸の call、intrinsic の `spawn`、同期関数からの使用は INVALID_TASK です。message 適格性/arity/storage/型失敗は既存 ownership/type diagnostic identity を使って宣言/call 型 span を保ち、use-after-move と active-loan 診断も既存 identity を維持します。

producer の規定例は `let mut outcome: core::SendResult<core::Bytes> = await core::channel_send::<core::Bytes>(&sender, payload);` です。borrowed match で Sent/Closed/InvalidPublication を区別し、match 終了後に `core::channel_unsent::<core::Bytes>(outcome)` で未送信 owner を回収できます。これは source 契約の sketch であり実行証拠ではありません。次の C03 fixture は実際に capacity-one storage を構築し、producer backpressure と receive を通じて owned payload を move して42を返し、全 owner を退役する必要があります。

lowering は権限 capture と suspension を MIR に独立して明示します。構築/mint/clone/変換/seal と結果全体の抽出の typed channel operation に加え、`PrepareChannel { destination: PreparedId, endpoint: Register, role, message_type }`、`ChannelSend { authority: PreparedId, payload: Register }`、`ChannelReceive { authority: PreparedId }` を追加します。既存の有限 prepared-destination namespace を channel kind を区別して再利用し、prepared replacement/place を endpoint と偽装できません。prepare は正確な shared Sender/exclusive Receiver loan を検証し、invocation 内 endpoint 権限だけを capture します。compiler-created 引数 loan は ChannelSend/ChannelReceive 前に終了し、明示 binding の loan は lexical lifetime を保って境界で生存していれば拒否します。独立 MIR は role/type/effect、初期化、capture の単一 consumer、escape/再利用禁止、payload consume、通常 loan の不存在を検証し、不正な未使用 operation/metadata も proof を省略できません。全 failure path で prepared authority を破棄し、guest value にはできません。pending continuation は destination、next/failure edge、実際の source operation span を保ちます。

task profile は同一 invocation の通常同期 helper と async activation に channel service を供給し、同期 helper は構築/mint/seal ができますが中断はできません。source channel operation は entry が同期でも task 実行 profile を選択します。metadata だけの旧 ownership API は明示拒否を維持し、endpoint 名だけで scheduler を付与しません。native、linked-object、REPL、C/foreign、persisted admission は対応 delivery が profile を証明するまで未対応とします。第2言語や native callback 内の隠れた VM 解釈は認めません。

capture の一意性は動的 preparation ごとに適用します。consumer または discard が prepared slot を空にした後、loop は同じ静的 PrepareChannel を再実行できますが、前の captured instance は再使用できません。

非公開 capture は引数の一時値 cleanup、suspension、取消の間、endpoint の protocol lifetime を保持します。一時 source endpoint の退役で captured operation を早期に close/失効させてはいけません。consumer/discard/失敗は waiter を unlink してから保持を解放し、通常の guest loan や payload の第2 owner にはしません。VM 統合の admission 前にこの lifetime を検証します。

guest endpoint storage は非公開の managed token であり C/native 公開 layout ではありません。副作用前に invocation/allocator identity、role、canonical message type、live channel/ticket と枯渇を検証します。Hub の authority/registration state と既存 owner/loan ledger を再利用し、有限 message-type descriptor table は metadata であって value owner の別 registry ではありません。guest payload は既存 Reservation storage と mixed retirement を使います。setup 公開前に実際の guest queue storage/cell を準備し、累積 endpoint/registration metadata を解放まで計上し、blocked/result storage は owner の move または物理退役まで計上します。source/guest inspection に read-only invocation allocator/shared-heap context を明示渡しして実際の強い receive edge を列挙し、cache した edge inventory や呼出側の Send/cycle-free flag では代用できません。host convenience visitor を理由に source admission の検査を省略できません。失敗し得る storage/publication/control 検証は全て atomic owner transition 前に行い、reservation 初期化前に queue custody を公開しません。空 slot reservation も計上し、必須 abort/取消/drain は既存の非計量 Session/Cleaner と quarantine/report 規則を保ちます。

最古 pending operation の span/ordinal を持つ型付き ChannelDeadlock execution reason を追加し、取消/cleanup 前に primary として保持します。trust-boundary 拒否は channel identity/role/type error に構造化対応し、quota/control failure は既存の数値 resource 証拠に対応します。backend string で error を分類しません。ChannelError、SendResult、operation、private token metadata に schema-2/3/4 tag や公開 C/package 表現を割り当てず、旧 writer/reader/layout API は未使用でも拒否します。将来の persisted revision は別の互換性判断と migration test を要します。

根拠: 名前付き result sum は Boolean flag、入れ子 Result encoding、owning field destructuring なしに2つの owner を保つ send 拒否を表します。capture/commit の分離で両 proof に loan 境界を明示し、単なる同期 CoreCall に suspension を隠しません。library は操作を組み合わせられますが proof を供給できません。既存 Reservation experiment は Bytes を Hub 経由で実行し、通常 [20,22]、取消 [22,20] の退役、合計42、live allocation 0 を確認しますが、queue slot/metadata admission、source operation、endpoint-bearing publication、native parity は証明しません。これらの統合、費用、両 host 等価性は未検証の実装義務です。この決定だけで delivery/gate は完了しません。

### Types, authority and lifetime

| Authority | Copy | Send | Share | Operations |
| --- | --- | --- | --- | --- |
| Setup<T> | no | no | no | mint sender; consume into receiver; abort on retirement |
| Sender<T> | no | T is Send | T is Send | explicit clone, send, retire |
| Receiver<T> | no | T is Send | no | receive, seal, retire |

source 名は `core::ChannelSetup<T>`、`core::Sender<T>`、`core::Receiver<T>` とし、型引数はそれぞれ正確に1つです。予約された opaque constructor であり、field、case、literal、暗黙変換を持たず、似た名前の user record は別型です。source と MIR は未使用宣言、canonical alias、再帰 nominal/container graph を含め、特性を独立導出します。未使用 constructor でも message 型 T は Send と既存 storage/region 検証を要します。3 role とも non-Copy で退役が必要です。Setup は Send/Share とも不可、Sender の Send/Share は T の Send のみに依存し、Receiver の Send は T の Send に依存して Share は常に不可です。handle の暗黙 clone はありません。Receiver の clone/share で第2 consumer を作れません。

C06 constructor 決定（2026-10-21 JST、RFC 0001 の委任に基づき採択）: 既存の有限型 graph で単調な non-Send/non-Share fact を個別に伝播します。通常 aggregate/container は各 capability を個別に伝播し、shared/weak のどちらの capability にも target の両 capability を要します。Sender は message の non-Send 依存を両拒否へ変換しますが message の non-Share は伝播しません。Receiver は加えて non-Share、Setup は両拒否を seed します。storage/privacy/region fact は全型引数を通じて独立に伝播します。再帰した Send 可 message と `Sender<Receiver<i64>>` を許容する一方、`shared Receiver<i64>` は Send/Share とも不可で channel message にできません。既存 loan/arena/allocator/Task 除外を維持します。work/storage/depth 枯渇は拒否し、capability を許可する fallback にしません。これは型 identity と導出のみの決定であり、具体 channel call、protocol error sum、typed MIR operation は source 実行前の C03 follow-up を要します。wire tag、C layout、実行 endpoint layout は割り当てず、旧 profile は未使用 channel metadata も副作用前に拒否します。未検証の仮定は C03 guest-storage 統合と C04 native parity であり、この決定はどちらの完成も主張しません。

Receiver/setup は queued payload storage を排他的に所有します。Sender は検証済み identity/closed-state metadata のみを保持し、receiver 退役後の queued value を生存させません。各 operation は完了/取消まで検証済み endpoint metadata を一時 pin します。count、generation、invocation identity を検証し、枯渇は副作用前に閉じて失敗します。数値 index だけでは権限を与えません。setup の keepalive 終了後、in-flight producer ticket を含む最後の sender が終わると将来の send を seal します。commit 済み値は受信できます。

handle を含むだけで payload を拒否しません。receiver/setup、queue、container、shared storage を通る強い所有循環を enqueue 前に拒否し、自分の mailbox への receiver 送信も含めます。Sender identity edge は queued payload を所有しません。既存の有限 publication 検査と mixed retirement を拡張し、呼出側指定の Sendable/cycle-free bit を信頼しません。非循環 handle 転送、alias、再帰 nominal 型は検証済みの動作を維持します。失敗時の incoming payload は operation の所有下に残ります。

### Capacity, preparation and commit

capacity は channel/slot/allocation 上限内の検証済み正数です。zero/negative/overflow は allocation/publication 前に拒否します。host は invocation allocator/scheduler 権限と有限 channel/slot/handle/waiter 上限を明示供給し、source metadata は権限を与えません。preparation で queue storage と有限 wait bookkeeping を予約します。1 logical task の pending channel operation は最大1つで、隠れた child task を作りません。

引数は左から右に一度だけ評価します。型、endpoint identity/generation、利用可能性、publication graph、destination storage を共有上限下で事前検証してから原子的な所有権移行を行います。non-Copy payload は既存 reservation と初期化 metadata を move し、token を bit-copy しません。commit 前の allocation/control 失敗は何も公開せず、初期化済み値を failure cleanup 下に保ちます。closed/invalid-publication send は元の owned payload と型付き protocol reason を返します。control/resource 枯渇は実行失敗であり、送信成功や取消値ではありません。

commit 中に guest fuel、allocation、interruption、callback 境界を入れません。send commit は pending owner を除き、初期化済み queue entry を1つ追加し、enqueue 位置を記録します。receive commit は最古の queue owner を除いて destination を1つ初期化し、span、allocator identity、推移 ownership を保ちます。二重 owner や owner 不在の隙間を観測できません。cleanup callback は commit 外で実行します。中断/取消された task が保持する受信値は task の値として計上/退役し、queue item として再退役しません。

### Backpressure and scheduling

full send は有限 waiter を登録して task を park します。empty/open receive は唯一の receiver operation を park します。登録には検証済み task identity、continuation/origin、全体順の registration ordinal を保持します。guest loan や native stack frame は保持しません。invocation storage は未commit payload を転送/退役まで初期化済み・計上済みに保ちます。

blocked sender は channel 内の登録順 FIFO で eligible になります。commit できるのは waitlist 先頭だけで、物理 storage に空きがあっても新 sender は既存 waiter の後ろに並びます。空いた slot は最古の生存 eligible sender が commit/登録解除するまでその sender に属します。その移行後も空きがあれば次の先頭を eligible にします。wake は既存 global ready queue の末尾へ一度だけ追加し、guest code を再帰実行しません。reserved turn の取消では unlink 後に次 waiter を eligible にします。receive は commit 済み値または終端 close を観測可能になると wake します。待機中に別 task が到達可能な所有関係を変える可能性があるため、parked operation が eligible になったら publication/endpoint 状態を再検証します。waitlist scan は有限かつ admission 後の allocation 不要とし、有限 link/index と既存 task identity 検証を使います。これは順序の保証であり、実時間公平性や channel 間の実行順を保証しません。

send/receive は即完了できる場合も明示 async effect と potential suspension/cancellation point です。blocked でなければ現在の dispatch 内で完了でき、追加 task/yield を必須にしません。source/MIR は実際の operation 境界で生存する全通常 loan を拒否します。compiler-created 引数 temporary は endpoint 権限を private operation ticket に捕捉後、境界前に終了でき、既存 allocator capture と同様です。別 binding の loan は通常の lexical lifetime を維持します。guest reference を suspension 越しに渡しません。具体 core catalog はこの境界を両 proof に明示し、backend 特例にしません。

### Close, cancellation and stalled execution

receiver seal は冪等で、新 send を拒否し、waiting send に未commit payload 付き Closed を返し、commit 済み値の FIFO drain を許します。queue が空になると receive は T なしの Closed を返します。last-producer closure も同じ drain 動作です。receiver/setup 退役は abort で、最初に receive 権限を無効化して queue を切り離し、commit 済み payload を FIFO で一度だけ退役します。cleanup 失敗/report truncation 後も継続します。pending send は commit 済み queue item ではなく、Closed return または取消 cleanup まで各 operation が所有します。

登録/commit 前に取消を確認します。parked operation の取消は登録を unlink/無効化し、reservation を解除してから必須 frame cleanup に進むため、古い wake は再開できません。cancelled sender は未commit payload を一度だけ退役します。cancelled receiver の frame は唯一の receiver を退役し、その handle の退役完了前に queued message を abort します。取消より先に観測した commit は維持し、receiver が既に所有する可能性のある message を rollback しません。source task 取消は RFC 0022 の terminal/TaskError 境界を保ち、通常 Closed return へ変換しません。trusted host protocol adapter は未commit payload 付き Cancelled を返せますが、返却か退役のどちらか一方だけを行います。

現在の closed invocation で ready/active task がなく、未完 task と channel waiter があれば決定的な ChannelDeadlock 実行失敗にします。成功、invariant panic、無期限 busy loop としません。最古の pending 登録を起点とし、cleanup 前に記録して tree 取消を要求し、全登録/owner を必須 cleanup で drain します。join-only ancestor で blocked descendant を隠しません。外部 polling/timer/host wake 権限を将来追加する場合、この規則を変える前に production embedding の決定を要し、ambient な外部 event を仮定しません。

RFC 0008/0021 の primary failure と poison 規則を維持します。先行する execution panic/control failure を primary に保ち、cleanup failure は secondary、取消/cleanup 優先順位は変えません。channel 内 commit 済み payload の破棄は FIFO、通常 frame owner は実際の初期化の逆順です。A03 は service queue を private owner state より先に破棄することを証明します。独立 channel 間の新しい全体順は約束しません。

### Accounting and protocol errors

queue entry、endpoint metadata、registration、pending/受信値、保持 tombstone は解放まで invocation の実 allocator/論理上限に計上します。予約済み queue slot は空でも preparation から storage 解放まで各1 live value cell として計上し、pending/result reservation は同じ storage を二重計上せず既存 cell 計上を使います。累積 metadata/ordinal 枯渇も上限で制限し、capacity は heap/cell/handle/task 上限超過を許しません。cleanup/取消 bookkeeping は新 guest allocation を要さず、RFC 0013 に従って非計量・非preemptive とします。

service-work を明示拡張します。capacity サイズの queue storage preparation 前に capacity 単位、明示 mint/clone/seal 前に1単位、send/receive attempt ごとに1単位、send/receive commit 前に1単位を課金します。resume/wake だけで新しい attempt/commit 単位を再課金せず、失敗した precommit 検査でも消費済み work は残します。既存 graph/element 検査課金を維持します。waitlist unlink、wake 通知、必須 closure/retirement は bookkeeping であり、新 guest service event ではありません。課金対象の副作用前の work/fuel/interruption 拒否でも cleanup が所有権を回収できます。全 backend で同じ数値 usage と拒否起点を公開します。これは project の費用規則であり、性能測定ではありません。

typed protocol 拒否は Closed と InvalidPublication を区別し、失敗 send の payload を保持します。invalid capacity/allocation は構築拒否です。foreign/stale/wrong-role identity、型不一致、receiver の二重 pending は構造化した trust-boundary failure です。channel/slot/handle/waiter/control 枯渇と ChannelDeadlock は execution failure です。具体 diagnostic variant/core error sum は実装前に記録し、backend string の分類を許しません。

## Execution modes

独立 source/MIR proof に具体 message/handle 型、move、終了 loan、async effect、pending continuation、commit/failure/取消 cleanup、source span を保持します。共有 invocation driver が protocol/service を持ち、VM dispatch と native 生成 continuation は同じ検証済み操作を使います。native callback は storage/scheduling service のみを行い、MIR 評価や VM fallback をしません。別リンク object を含め、値、enqueue/wakeup 順、起点、counter、report、poison、物理退役を比較します。未対応 profile は guest/provider 副作用や artifact 公開前に拒否します。

型の parse や旧意味論モデルの成功だけで channel を source execution に昇格しません。constant execution は未対応です。REPL、C import/export、foreign callback、invocation 間 handle は採択済み production profile の実装まで既存拒否を維持します。OS-thread Send/Sync を推論しません。

## ABI and compatibility

本 follow-up は RFC 0005 の有限 full-send 方針と RFC 0024 の具体 endpoint capability を選び、通常 move/loan と owner-isolation を維持します。既存の同期/task program の動作と event 費用は変えません。後続 core catalog は reserved name と既存 call 構文を使うか、必要な構文変更を明示記録し、新しい partial-move pattern を仮定しません。

C layout、公開 native ABI、serialized MIR/bytecode tag、native-package profile は割り当てません。旧 schema-2/3/4 writer/reader/runtime layout は未使用を含む channel metadata/操作を拒否し、opaque token fallback を推論しません。永続実行前に serialized task/channel revision と migration test を要します。新 handle を legacy host/foreign type 経由で持ち出せません。

## Diagnostics and tooling

SRC0020 move、SRC0023 storage/authority、SRC0027 async effect、SRC0028 live loan の identity と operation/loan 起点を維持します。必要な proof/adapter が完成するまで SRC0026 または構造化 unavailable-profile error を維持します。新しい有限 channel error identity は実装前に記録します。既存 CST formatter/LSP は同じ resolved call/effect fact を使って comment/span を保存し、別の緩い解析 mode を設けません。将来 debug view は role、logical channel identity、queue state、待機起点を表示し、address を表示しません。DAP/linter の完成は主張しません。

## Performance and cost model

admission 前に queue storage と wait metadata を有限化し、暗黙 mailbox を増大させません。endpoint clone は検証済み metadata を retain し、message を clone しません。enqueue/dequeue は reservation を move し、payload graph を deep-copy しません。publication 検証は既存 work/storage 上限で stored ownership edge を走査し得ます。取消/退役は iterative にします。定数時間、latency、throughput、allocation-free を保証しません。Z01 で両 tier-1 host の queue/保持 payload peak、send/receive/wakeup/取消 work、代表的 M2 費用を測定します。

## Security and trust boundaries

untrusted source は Send/Share を自己宣言せず、endpoint/task identity の偽造、receiver 権限 clone、publication 検査回避、ambient scheduler/allocator/host 権限の取得をできません。untrusted MIR と将来 wire metadata を独立再検証します。count/generation overflow や traversal 枯渇で unchecked fallback を許しません。trusted adapter は pending owner を保持し、実際の退役後だけ cleanup を acknowledge します。native provider/callback の既存 trust/no-unwind 契約を維持し、native-process sandbox にはしません。

## Alternatives

- 変更なしでは有限 source channel と actor service が未実装で、旧モデルでは不足します。
- library は primitive 実装後に service を構成できますが、compiler-visible suspension、独立 move/loan proof、trusted commit semantics は単独で追加できません。
- 非有限 queue は blocked send を省けても有限 memory/backpressure 要求を満たしません。fail-fast Full は簡単ですが、選択した waiting producer 契約を満たしません。将来の明示 try-send は default を変えず併存できます。
- rendezvous、multi-consumer/select、並列 lock-free queue はより強い競合設計で、fairness/取消/memory-order 契約を要します。直列 invocation の FIFO から暗黙に導かず延期します。
- 暗黙 Copy endpoint や2つの owning pattern binding は retain/authority 費用を隠すか RFC 0009 を変えます。明示 setup/clone/whole move で現行規則を維持します。

## Implementation plan

C02 representation audit（2026-10-21 JST）: C06 を C03 前の新たな前提として明示し、concrete role 型名/constructor、独立 source/MIR Send/Share/storage proof、legacy profile 拒否を担当します。現 source/MIR の閉じた型族は Send/Share を同値としており、この同値を Receiver に流用できません。C03 は source fixture 前に正確な core call/error/MIR catalog と guest storage/control 統合を行います。C02 完了3ptとは別に残作業3ptを追加し、実装 Todo に収支を記録します。

1. C01: protocol を受理し、関連 RFC/architecture を改訂して決定と実装/gate 状態を区別します。
2. C05 で有限の checked task park/wake、待機 identity 再利用、park/wake 前後の取消を実行し、C02 の前提を閉じます。これは channel fixture に必要な kernel の不足であり、汎用 executor framework にはしません。C02 を有限 storage、typed ownership commit、取消に照らして再監査し、さらに独立した不足前提があれば実装前に明示します。send/receive、close、blocked-sender 取消、必須破棄を通る trusted host fixture で 42 を返します。後続 adapter が使う runtime service とし、別の意味論モデル framework にしません。
3. C03 前に正確な core catalog、constructor graph 規則、diagnostic/error variant、typed MIR 操作を本書または採択 follow-up に記録します。source/独立 MIR/VM を繋ぎ、次の source channel fixture と payload 再利用失敗を実行します。段階的な admission が初期拒否する場合も、受理した handle-bearing/publication 条件を維持します。
4. C04: 生成 native と linked-object の値、suspension、失敗起点/usage、退役 parity。A02/A03/A04 は具体 actor authority、state isolation、取消時の queue-before-state 破棄を担当します。
5. 既存 embedding、resource/adversarial、両 host conformance、serialized/package、費用、最終 guide の delivery を維持します。決定や host fixture で gate を完了扱いにしません。

## Test plan

capacity one/exact/max/refusal、late arrival を含む sender FIFO、複数 slot 解放、closed drain、last sender、setup 放棄、receiver abort、wake 済み取消、stale/foreign/wrong-role identity、二重 wake、count/generation/ordinal 枯渇、全停止 deadlock を検証します。owned record/sum/collection/shared/weak と非循環 endpoint-bearing 値を含め、staging、commit、receive、return、retirement の全 payload を追跡します。直接/推移 strong publication cycle を原子的に拒否します。nested scope、commit 前後の取消、panic/cleanup panic/report truncation、service queue-before-state 破棄も検証します。

source compile-pass/fail で endpoint capability、明示 clone、unique receiver、payload 再利用、invalid effect、live-loan suspension 拒否を証明し、不正 raw MIR で独立再検証します。VM/native/linked-object 差分で該当する全 fuel/work/provider/interruption prefix と queue/handle/waiter/frame/cell/heap/task 上限境界/1 未満を確認します。profile 実装時に raw graph/wire fuzz mutation を加え、compile を fuzz campaign としません。exit/stabilization 前に両 host conformance と Z01 測定を行います。

## Open questions

上記 C03 catalog に source name/error sum/MIR form を実装前に記録しました。新 owner registry なしで managed publication/retirement を endpoint edge に拡張できること、admission 済み wait link により wake/取消を allocation-free にできること、setup/clone/result spelling が現行 whole-value 規則で使えること、代表的費用と両 host 適合は未検証です。採択済み catalog を変更する場合は実装前に英日を明示改訂します。この決定だけから実装・安定・性能結果は導きません。

判断: **RFC 0001 の実験委任で accepted、2026-10-21 JST**。有限 MPSC、唯一の receive 権限、明示 Send/Share、非commit payload の custody、原子的な公開、FIFO admission と既存 cleanup を組み合わせて厳密な所有/loan 規則を維持します。capacity-one の trace 実験では、単純 wake で A,C,B と追越しが起き、先頭予約で A,B,C を維持しました。wake 後の B 取消、commit 後の C sender 取消、seal/abort の5 trace も各 payload の owner と一度限りの返却/退役を検証しました。これは protocol 実験であり production channel の実行証拠ではありません。上記仮定と concrete core catalog は未検証/未実装のまま追跡します。

## Sources

- [RFC 0001](0001-project-charter.md): 最終更新 2026-10-17、同日の owner 実験委任、2026-10-21 JST 確認。[RFC 0005](0005-structured-tasks-and-isolated-services.md)、最終更新 2026-10-21: strict ownership、accepted-message FIFO、取消、以前未決の backpressure 方針。
- [RFC 0021](0021-bounded-structured-task-driver.md)、[RFC 0022](0022-source-structured-tasks.md)、[RFC 0024](0024-send-share-and-service-authority.md)、最終更新 2026-10-21: scheduler、task effect/loan、logical Send/Share。[RFC 0009](0009-whole-sum-ownership-extraction.md)、最終更新 2026-10-20: owning partial move のない whole-value extraction。
- [RFC 0007](0007-source-ownership-and-core-values.md) と [RFC 0013](0013-service-byte-work-budget.md)、最終更新 2026-10-21、2026-10-21 JST 確認: checked publication、allocator 権限捕捉、共有 service-work/非計量 retirement。
- `crates/runtime/src/task.rs`、`task/runner/`、`crates/semantics/src/service.rs`、source/core/MIR 境界を `224c1f2`、2026-10-21 JST に確認。コードに更新日埋込みはありません。その checkpoint の driver は readiness wait state を持たず、旧モデルは有限 runtime の証拠ではありません。これらは repository の事実です。上記 MPSC/role/FIFO admission/seal/abort/費用/deadlock は project decision、ergonomics/性能/reuse は仮説で、外部言語の主張は用いません。

- C05 実装証拠: `crates/runtime/src/task.rs`、`task/tests/wait.rs`、`examples/parked-tasks.rs`。コードに更新日埋込みはなく、2026-10-21 JST に macOS AArch64 で確認/実行しました。wait ticket は private identity と ordinal を持ち、kernel は guest message を所有せず、新 channel Source/ABI/wire の証拠にはしません。

- C02 実装証拠: `crates/runtime/src/channel.rs`、`channel/`、`task.rs` の quiescence query、`examples/bounded-channels.rs`。コードに更新日埋込みはなく、2026-10-21 JST に macOS AArch64 で確認/実行しました。native callback panic 時の unknown payload は RFC 0008 の既存 quarantine 契約を保ち、source/guest heap の完成や fuzz campaign は主張しません。

- C03 reservation experiment: `examples/bounded-guest-channels.rs`。コードに更新日埋込みはなく、2026-10-21 JST に macOS AArch64 で実行しました。検証済みの正確な Bytes reservation、invocation allocator、既存 Hub/Session を使い、合計42、owning storage identity の維持、通常/取消の退役、live allocation 0 は experiment の証拠ですが、完全な source/queue-slot/metadata/native admission ではありません。source catalog と MIR capture protocol は project の決定であり、外部言語の主張は使用しません。
