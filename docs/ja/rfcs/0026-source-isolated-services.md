<!-- translation-of: ../../rfcs/0026-source-isolated-services.md -->

# RFC 0026: Source isolated service

[English](../../rfcs/0026-source-isolated-services.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-22
- Last updated: 2026-10-22
- Tracking issue: M3-A02, M3-A07, M3-A08, M3-C07, M3-A03, M3-A04
- Supersedes: なし。RFC 0005/0024 が要求する具体 service catalog を補完
- Superseded by: なし

## Summary

A03 VM checkpoint（2026-10-22 JST）: 明示 private VM profile は Receiver/ServiceOwner・入れ子 container・shared/weak producer handle・再帰 state を含む、独立に証明した完全な Send/storable state/message を受理します。既存 backing が実 state Reservation を1個所有します。有界 Hub publication walk が現在の committed queue/state を読み、初期/whole replacement は候補 state だけを置き換え、再開 send は wake 後に再検査します。循環 publication/資源拒否は commit 前の ownership を保ち、置換拒否は旧 bytes/generation を保って拒否候補を既存 caller cleanup で一度消費します。別 owner/edge registry や scheduler は追加しません。handler read provenance/取消境界・closed identity・typed Copy 回復・queue-before-state 反復退役も維持します。A04 の native 実行は下記のとおりで、ordinary layout・REPL・wire・C/foreign・package は副作用前拒否を維持します。

A04 native service 境界（2026-10-22 JST 選択、実装前）: 明示した generated-task API は検証済み service layout を選び、完全な Send/storable role・core call・task MIR を独立検査します。PrepareService/ServiceSend/ServiceReceive を既存 capture/send/receive native primitive に lower し、同期 state/read/replace/finish/cancel は共通の検査付き service を使います。ServiceNew は provider 副作用前の VM と同じ位置で capacity と state cell を事前検査します。演算・制御フロー・continuation/失敗選択は生成 native code のままとし、VM/MIR evaluator fallback は使いません。JIT と別リンク object は同じ private exact-build ABI を使い、primitive ABI・source 構文・永続 schema は追加しません。ordinary API・wire/C/foreign・native-package の受理は維持します。conformance catalog は基本 service 14 fixture と4段の再帰 prefix fixture を含みます。全 prefix は小さい graph で検証し、64段では深い退役と control 上限の境界を検証します。別リンク object は既存分を含め40 profile です。 A04 signoff 前に全 differential control/custody/report と linked-object 証拠を必要とします。

実行証拠（2026-10-22 JST）: state CLI fixture 6個が42を返します。重点85件は実/nested/再帰 state edge、即時と wake 後 commit 前の循環拒否/回復、置換候補の循環と旧 state/generation を保つ work 上限/1 不足、shared/weak producer authority、64段の cleanup、選択した全有限 control/provider/割込み prefix、実 quota、ordered nested cleanup panic/打切り report と inventory 0 を検証します。全 baseline の signoff と A07/A08/C07 の日付付き履歴は英日実装 Todo に記録します。この checkpoint から安定化・service native 受理・M3 gate 完了は導きません。

invocation 内で隔離した service の opaque setup・owner・producer role、明示 core 操作、handler 境界を受理します。唯一の owner だけが state を read/置換でき、producer は enqueue と取消要求ができます。typed MIR は role・move・loan・suspension を独立検証します。受理は実行や安定化の証拠ではありません。

初期 state interface は既存 managed shared/arena interface に倣い、検証済み shared read と state 全体置換を使います。exclusive state projection、暗黙 clone、actor 宣言構文、closure、第一級関数値は追加しません。state/message は Receiver/ServiceOwner を含む、独立証明した Send/storable family を許容し、循環を防ぐ publication 検査を必須とします。

## Motivation

通常の async state と Receiver の組合せではこの契約を供給できません。[composition 実験](../../experiments/2026-10-22-service-composition.lang)で state Bytes(1) を Bytes(42) に置換し、Bytes(7) を queue に残して owner task を取り消すと、既存 provider の退役 trace は **[1,42,7]**、結果42、live allocation 0 です。通常の publication 逆順は新 state を queue より先に退役します。単独 Sender には service 取消権限もありません。

これは managed ownership 境界を分ける正しさの証拠であり、throughput/ergonomics benchmark ではありません。通常 task cleanup は維持します。library は受理した service 操作を合成できますが、独立 source/MIR role/loan proof や queue-before-state 退役境界は作れません。

## Goals

- private owner state、message 専用 producer 権限、再帰 Send、通常 move。
- 有限 FIFO message、service 取消時の現在 handler 完了、queue-before-state 退役。
- 共有する有限 accounting、検証済み動的 identity、VM/native/linked-object の一致。

## Non-goals

OS thread 実行、detached task、trait/自己申告 Send、mutable state projection、自動 state copy、暗黙 handler、公開 native/C layout、persisted task/service format。

## Guide-level design

受理した spelling です。A03 private VM profile は独立に証明した Send/storable state 契約とこの lifecycle を実行します。

```text
struct Counter { total:i64 }

fn add(owner:&mut core::ServiceOwner<Counter,i64>, amount:i64)->core::Unit {
    let mut total:i64=0;
    match core::service_read::<Counter,i64>(owner) {
        core::Result::Ok(state) => { total=state.total; },
        core::Result::Err(error) => { return core::panic::<core::Unit>(-6); },
    }
    return core::result_unwrap::<core::Unit,core::ServiceError>(
        core::service_replace::<Counter,i64>(owner,Counter { total:total+amount }));
}

async fn serve(input:core::ServiceOwner<Counter,i64>)->i64 {
    let mut owner:core::ServiceOwner<Counter,i64>=input;
    let mut first:i64=core::option_unwrap::<i64>(
        await core::service_receive::<Counter,i64>(&mut owner));
    let mut added:core::Unit=add(&mut owner,first);
    let mut second:i64=core::option_unwrap::<i64>(
        await core::service_receive::<Counter,i64>(&mut owner));
    let mut added_again:core::Unit=add(&mut owner,second);
    let mut answer:i64=0;
    match core::service_read::<Counter,i64>(&owner) {
        core::Result::Ok(state) => { answer=state.total; },
        core::Result::Err(error) => { return core::panic::<i64>(-6); },
    }
    return answer;
}
```

caller は Counter(0)・capacity 2 で ServiceSetup を作り、handle を mint、setup を owner に消費、serve(owner) を spawn し、handle 経由で20と22を send、scope 内 task を await して42を得ます。producer は handle で read/replace や state field projection ができません。明示 binding の read は置換・finish・receive・全 suspension 前に終了します。通常 loan の async worker への引渡しは禁止のままです。

## Reference semantics

### Types and core catalog

全名を core に予約します。S/M は明示型引数であり、未使用 constructor を含め独立に Send/storable を要します。Setup/Owner は2引数、Handle は M だけを持ち、S の知識や projection を許可しません。

| Constructor | Copy | Send | Share | Ownership |
| --- | --- | --- | --- | --- |
| ServiceSetup<S,M> | no | no | no | unique な未 claim state/mailbox owner |
| ServiceOwner<S,M> | no | S と M が Send | no | unique state/receive 権限 |
| ServiceHandle<M> | no | M が Send | M が Send | producer/取消 ticket。queued/state の strong ownership なし |
| ServiceError | yes | yes | yes | 通常 data |

有効な closed identity の mint/clone は closed handle を増やせますが、state/message 受入を復活させず、累積 handle admission を消費します。opaque 3 role はすべて退役が必要です。再帰導出は既存の有限 graph を使い、Handle の M は strong ownership traversal では phantom です。Shared/Weak 内 Handle は capability fact だけでなく実 identity 検証を要します。Owner は Share ではなく Shared/Weak に隠せません。

ServiceError の canonical data case は 0 InvalidCapacity、1 Allocation(AllocError)、2 Unavailable、3 NoActiveHandler、4 InvalidPublication です。case 自体は権限を持ちません。

| Core call | Result | Effect |
| --- | --- | --- |
| service_new<S,M>(&Allocator,i64,S) | Result<ServiceSetup<S,M>,ServiceError> | 同期。初期 state を消費 |
| service_handle<S,M>(&ServiceSetup<S,M>) | ServiceHandle<M> | 同期の明示 mint |
| service_clone<M>(&ServiceHandle<M>) | ServiceHandle<M> | 同期の明示 clone |
| service_owner<S,M>(ServiceSetup<S,M>) | ServiceOwner<S,M> | 同期の unique 変換 |
| service_read<S,M>(&ServiceOwner<S,M>) | Result<&S,ServiceError> | 同期の検証済み read |
| service_replace<S,M>(&mut ServiceOwner<S,M>,S) | Result<Unit,ServiceError> | 同期の全体置換 |
| service_finish<S,M>(&mut ServiceOwner<S,M>) | Unit | 同期の handler 境界 |
| service_cancel<M>(&ServiceHandle<M>) | Unit | 同期の冪等 request |
| service_send<M>(&ServiceHandle<M>,M) | SendResult<M> | direct await 専用 |
| service_receive<S,M>(&mut ServiceOwner<S,M>) | Option<M> | direct await 専用 |

引数は左から一度ずつ評価します。明示型 arity・storage・capability 失敗は source type span を使います。構築は非正 capacity を InvalidCapacity、回復可能な guest allocation 失敗を Allocation とします。失敗しても通常の moved core 引数として初期 S を消費し一度退役し、元 binding を復元しません。control/proof/metadata quota 失敗は構造化した execution/preparation failure であり ServiceError ではありません。identity 公開/ownership commit 前に全 fallible resource を事前検査します。

Send は RFC 0025 の Sent/Closed(M)/InvalidPublication(M)、結果全体の unsent 抽出、元 payload custody、moved local を復元しない規則を再利用します。receive/send は現在 task の操作で child Task claim ではありません。bare call、intrinsic の spawn、同期 caller は INVALID_TASK です。cancel は request であり、完了 acknowledge や暗黙 Task cancel/join ではありません。

### Owner authority, reads and replacement

unique Setup/Owner token が state と receive 権限を所有します。Owner の move は権限を一度移し、state clone や2つ目の owner task は作りません。同期 helper は同じ segment 内で caller の owner を borrow できます。通常 task loan で別 activation へ渡せません。

Some(M) の receive 成功が1 handler を開始します。次の receive は前 handler を finish してから FIFO の次 message を poll します。明示 finish と退役も handler を終了し、idle/closed の明示 finish は冪等です。idle の read/replace は NoActiveHandler、closed 後は Unavailable を返します。active handler は全通常 loan 終了後にのみ yield/await できます。service 取消待ちでも先取りせず、finish・次 receive・owner 退役に達します。structured task 取消/失敗は RFC 0005/0022 に従い、同じ queue-before-state 境界で service を退役します。

read は正確な owner/state generation に結ぶ通常 shared loan で、provisional Result binding と検証済み origin を持ちます。既存 core read 規則で借用 Result を直ちに match し、通常 result_unwrap の T を通常 loan に特殊化できません。owning S や mutable loan にはなりません。束縛済み read は lexical で、通常 borrow 規則により置換/finish/receive/move/drop を防ぎます。compiler-created call loan は受理した境界で終了しますが、束縛済み read を早期終了しません。

置換は通常 move 規則で candidate S を消費します。active owner と read が残っていないことを要します。commit 前に state 内 Receiver/ServiceOwner を含む prospective な実 strong receiver edge を、現在 commit 済み message/state ownership に対して検査します。循環拒否は InvalidPublication を返し、旧 state を変えず candidate を一度退役します。Unavailable/idle の置換も candidate を退役します。既存 replacement service が新しい sole owner を確立してから旧 state を退役し、cleanup failure は既存 primary/secondary 規則を保持します。置換時の退役順で service 最終退役の queue-before-state 境界は変えません。

初期 mutation interface は state read/全体置換です。将来の exclusive descendant projection は独自の決定と全 mutation/publication の検証を要し、safe source は producer や cast で到達できません。

### Cancellation, closure and retirement

取消 request は一度 latch し、新 message の受入を止め、parked operation を起こし、待機せず返ります。既存 blocked send は RFC 0025 の Closed(M) で回復します。active handler がなければ registration を unlink し、commit 済み message を FIFO、state を一度退役してから request を返します。active handler があれば境界まで state/read generation を有効に保ち、境界で unavailable として同じ drain を実行します。既存 active handler の read/置換は境界前まで有効で、request 後に新 handler は開始しません。有効な closed handle の反復 request は no-op です。

shutdown 後の receive は None、state 操作は Unavailable です。last-producer closure は受理 message を通常通り drain し、最後の handler 境界または None の receive で state を finish/退役します。Setup/Owner の drop、owner task 取消/失敗、invocation abort は unlink 後、queue の FIFO 退役を state より先に実行します。Handle drop は queued/state 値を所有/直接退役せず、最後の producer の bookkeeping により owner を ready にできます。

handler に返った message は通常の owner local であり queue item ではありません。通常 move/scope cleanup に従い、service inventory に重複登録しません。blocked send の未 commit payload は sender の pending owner のままです。queue-before-state は commit 済み queue custody の規則で、無関係な caller local や pending payload の順序ではありません。全 move と実退役で owner source span を保ちます。

既存 iterative mixed drain、必須非計量 cleanup、panic quarantine、report 打切り、primary-failure 優先を使います。queue の子と state の退役まで親 header を保持し、子が最後の producer を drop しても同じです。publication/queue invariant の commit 途中に callback を実行しません。

### Independent MIR and trust boundaries

同期8操作に typed core operation を追加します。PrepareService { destination, endpoint, role, state_type, message_type }、ServiceSend { authority,payload }、ServiceReceive { authority } を追加し、state_type は Owner receive にのみ保持します。既存の有限 prepared namespace を service 専用 role/kind で再利用し、channel/place 権限を代用できません。独立 MIR は正確な role・canonical type・effect・入力初期化・capture の単一消費・escape/再利用禁止・payload move・read origin/generation・suspension 時の通常 loan 不在を検証します。private な invocation 内権限だけを capture し、suspension 前に一時 endpoint loan を終了します。不正な未使用 operation/metadata も proof を省略できません。

lossless syntax と operation/loan/move/state origin を保ちます。旧 ordinary layout、REPL、C/foreign、schema-2/3/4 codec、native-package は未使用 constructor を含む service metadata を guest/provider 副作用前に拒否します。A02 は semantics を独立証明します。A07/A08/C07 は state・handler/取消・typed Copy の前提を供給し、A03 は明示 private VM profile に完全な Send state/publication/control を接続します。A04 は parity 確認後にだけ生成 native 実行を許容します。受理だけでは profile を拡張しません。

既存 Hub/managed channel backing に service role/status と unique token が所有する実 state Reservation 1個を追加します。commit/pending message の唯一の owner は Hub のままです。実 guest/state/queue/shared storage の graph edge を検証済み read-only context で読み、第二の owner registry、cached edge inventory の信用、caller の capability/acyclic flag は導入しません。prospective 全体置換の循環検査はその state だけを差替え、再開した pending send は現在 graph を再検査します。Shared/Weak 内 Handle の publication/置換は明示 service context で検証し、strong receive edge がなくても実 live identity を検証します。

有限 channel/slot/handle/waiter・preparation・frame/cell・guest allocation/heap・retained-result limit は service object、実 state storage、closed identity、pending operation を含みます。scheduler/ServiceWork を再利用し、ambient executor、detached state、再帰 host-stack drain を加えません。

## Execution modes

VM、生成 native continuation、exact-build linked object は値・event・origin・全 usage・取消/closure・failure/report 順・実退役を一致させます。native callback は typed storage/scheduling service を実行し MIR 命令を解釈しません。両 tier-1 host と production embedding は M3 の要件です。trusted Rust service model や JIT のみの example では不足します。

## ABI and compatibility

従来未知だった予約 core 名と拒否していた service program を拡張します。既存の有効な通常 task/channel program は cleanup と論理 service-work event を維持します。compiler/preparation byte accounting は実構造体を数え続け、検証済み metadata 拡張で増加し得るため、更新時に有限 preparation limit の調整が必要な場合があります。全体置換・request 専用取消・handler 境界は source 契約であり backend の偶然ではありません。公開 ABI、source edition、artifact tag、package field、C 表現は割り当てません。persisted service metadata は独立 version の明示的 format 決定、検証、migration test を要します。

## Diagnostics and tooling

構造化した type/arity、use-after-move、invalid loan、INVALID_TASK 診断を再利用します。既知 producer handle を owner/state 権限に使う場合 SRC0029（SERVICE_ISOLATION）を加え、operation と handle/type origin を示します。旧 semantic model の SEM0031 は別 identity のままです。suspension は SRC0028 と live read/loan origin を示します。独立 MIR は typed error/span を保持し、runtime の誤 brand/role/generation/type は構造化 execution 拒否であって ServiceError data の偽造にはしません。

既存 generic call/parser recovery と formatter 構文を使い、LSP は同じ source/MIR analysis を使います。debug 表示は raw address ではなく role/state/取消 status を示します。別の緩い analysis mode、一般 mutable state fix-it、新 DAP 実装は含みません。

## Performance and cost model

publication 前に有限 mailbox storage、state reservation 1個、identity descriptor を準備します。move/handle は deep clone を意味しません。read は検証済み generation/loan metadata を保持し、全体置換は新 S を構築して旧値を退役します。exclusive field mutation より高価な可能性があり、ergonomics・throughput・code size・scalability の利点は主張しません。

RFC 0025 の capacity・mint/clone・send/receive attempt/commit 課金と、RFC 0013 の実 managed observation/typed replacement/publication event を再利用します。固定 role/generation/handler flag bookkeeping に payload-copy event はありません。明示 finish・取消通知・wake/unlink・必須退役は非計量 bookkeeping で、guest fuel/poll 境界は維持します。両 backend は副作用前に同じ課金を使います。安定化前に代表的 state 置換、queue 取消、receiver-edge 検査、retained memory を測定します。

## Security and trust boundaries

untrusted code は role identity の構築、Send/Share の自己申告、producer state projection、通常 loan の転送ができません。実 service・shared publication・foreign 境界で動的検証が必要です。graph work・state/queue storage・metadata は commit 前に上限を検査します。private な trusted native profile は sandbox や OS-thread safety の主張ではなく、将来の component/plugin admission も明示 capability grant と versioned interface を要します。

## Alternatives

変更なしでは再現した取消/退役の不足と取消 handle の欠如が残ります。async function/Sender の library は managed state 境界を強制できませんが、検証済み primitive の library は service を合成できます。最も強い対案は組込み actor 宣言/static-handler function catalog で、loop を隠し handler 形状を選べますが、新しい宣言/effect/lowering 規則が必要で今回の source authority に不要です。mutable state loan は柔軟ですが、全 mutation managed service の transitive publication hook が必要です。Copy 限定の state/message は単純ですが、受理済み owned/再帰 Send を落とします。選択した全体置換はこれら family を維持し、publication を一つの検証境界に集約します。

## Implementation plan

A02 は constructor fact/core catalog、producer/owner 診断、通常 read/move/suspension proof、独立 MIR と旧/backend の副作用前拒否を供給します。A07/A08/C07 の前提後、A03 は完全な Send state/message edge・prospective publication・control/quota/refusal 統合を実行します。A04 は生成 native/別リンク parity を証明します。RFC 0005/0024、architecture、英日 board/roadmap を証拠と同時に更新します。gate、両 host embedding、測定、最終 capability guide は別の受入条件です。

## Test plan

Copy/owned/nested/recursive state/message、helper read、owner への合法 move、producer clone を compile-pass とします。handle の state projection/read/replace、owner 重複消費、nested loan/arena/allocator/Task の非 Send、arity 不正、置換/finish/receive/await にまたがる束縛 read を compile-fail にします。MIR の role/type/capture 再利用/loan origin を偽造し、proof budget の正確な上限/1未満、未使用 metadata、全旧 artifact/profile 拒否を確認します。

20+22=42、full/empty FIFO/backpressure、pending send の回復、idle/active/parked と反復取消、境界前の handler yield、取消前 state 置換、closure/return/task failure、受信済み/queue custody、queue-before-state trace を実行します。Receiver/ServiceOwner の state/message、prospective state cycle、再開時再検査、shared producer publication を含めます。全 control/provider/interruption 位置、実 quota、panic/quarantine/report 打切り、深い iterative cleanup、inventory 0 を VM/native/linked object で比較します。全 fuzz target の compile/lint を行い、実際に実行した場合のみ fuzz campaign を主張します。両 host/ABI migration test と安定化前測定も維持します。

## Open questions

この private VM profile の意味論を妨げる未決事項はありません。managed loan generation の保持・backing 内の state Reservation 1個・Hub wake/unlink・closed identity の寿命・prospective non-Share state 検査・queue-before-state mixed retirement を、owner registry の追加や退役ごとの allocation なしで検証します。service native/別リンク parity、代表的 cost、両 host、将来の persisted/foreign 表現は未検証で、受理から推測しません。

決定: **RFC 0001 の実験段階委任により accepted、2026-10-22 JST**。明示 role、通常 loan/move、state 全体 publication、観測可能な handler 境界により、新 source 言語・無制限 mutable projection・interpreter fallback なしで RFC 0005 の isolation/取消を保ちます。composition 実験は ownership 境界が必要な理由を示します。上記仮説は実装証拠として扱わず記録します。

## Sources

- RFC [0005](../../rfcs/0005-structured-tasks-and-isolated-services.md)、[0013](../../rfcs/0013-service-byte-work-budget.md)、[0022](../../rfcs/0022-source-structured-tasks.md)、[0024](../../rfcs/0024-send-share-and-service-authority.md)、[0025](../../rfcs/0025-bounded-channels-and-backpressure.md): project の意味論契約。最終更新は0005/0013/0024/0025 **2026-10-22**、0022 **2026-10-21**、**2026-10-22 JST** 確認。外部の事実/性能主張の根拠にはしていません。
- [composition 実験](../../experiments/2026-10-22-service-composition.lang): **2026-10-22 JST** 作成/実行、基盤 f882074/14f1af7。一時 integration probe が既存 tests/support/channel_fixtures.rs provider を使い、結果42、buffer 退役 [1,42,7]、live inventory 0、実験1件成功を記録しました。probe は service conformance suite ではありません。
- crates/hir/src/core.rs、crates/hir/src/types.rs、crates/mir/src/unit/ownership/、crates/runtime/src/channel/managed.rs、既存 shared/arena view と task 退役: code に更新日の埋込みはなく、**2026-10-22 JST** 確認。C04 全体検証の証拠は[実装 Todo](../milestone-3.md)に記録します。
