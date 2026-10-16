<!-- translation-of: ../../../docs/rfcs/0005-structured-tasks-and-isolated-services.md; canonical-language: en -->

# RFC 0005：Structured task、cancellation、isolated service

[English version](../../../docs/rfcs/0005-structured-tasks-and-isolated-services.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-16
- Last updated: 2026-10-17
- Tracking issue:
- Supersedes:
- Superseded by:

## 概要

初期concurrency／cancellation baselineを定義します。

- Child taskはscope付きであり、全childがcomplete、またはcancel／joinされるまでparent scopeはexitできません。
- Cancellationはcooperativeでtask treeへ伝播し、任意codeをpreemptせず明示的suspension／cancellation pointで観測します。
- Task frameはsuspension中もinitialized valueを所有し、borrow終了後に逆initialization順で正確に1回destroyします。
- 初期言語ではshared／exclusive borrowのどちらもsuspension pointをまたげません。そのsuspensionは `concurrency.borrow-across-suspension`（prototypeでは `SEM0007`）で拒否します。
- Mutable service stateはowner taskへ隔離します。Shareable service handleはsendable payloadを1つのmailboxへmoveできますが、stateを直接mutationできません。
- Accepted messageはmailbox enqueue順で処理します。Cancellationはactive handler boundaryまで待ち、queued payloadを順番にdiscardし、service stateを1回destroyして、後続sendを拒否します。
- Base modelはgeneral shared mutable stateもimplicit global executorも公開しません。

正確なtask／service API spellingとscheduler policyは暫定です。Lifetime、isolation、ordering、cancellation、cleanup、cross-mode ruleはaccepted semantic commitmentです。

## 動機

Async loweringはvalueの配置、borrow終了、cancellationによるdestructionを決めます。Service concurrencyはshared handleがmutation authorityを持つか、message payloadがtask間でaliasできるかを決めます。これらを延期するとsource lowering、MIR、VM、native execution、embeddingが互換性のないsafety ruleを獲得します。

Swift structured concurrencyは、child-task lifetime、cancellation、priority、deadlineがtask tree／scoped joinから恩恵を受ける一次資料上のevidenceです。Pony reference capabilityは、aliasing guaranteeがactor communicationをdata-race-freeにできるevidenceです。これらは提案をinformしますが決定しません。

Repository prototypeはstrict／less-restrictive policyを比較します。`RejectAll` は全borrow-across-suspensionを拒否します。`AllowSharedFrameBorrows` はshared frame borrowをsuspend／resume越しに維持し、cancellation cleanup前に終了します。`OwnerIsolated` はservice mutationを1 ownerへ制限し、`ExclusiveCapability` はhandle holderが単一dynamic capability下でmutationできます。代替案は実行できますが、pinning、address stability、lock fairness、poisoning、deadlock、backpressure、許容costを確立しません。Strictな組合せは、これらのhidden contractなしで一貫した初期baselineを形成します。

Library-only task／actorはborrow termination、compiler-derived sendability、owner isolation、at-most-once cleanup、backend-equivalent diagnosticを強制できません。

## 目標

- Scoped task lifetime／cooperative cancellationを定義する。
- Task-frame valueのownership／destructionを定義する。
- 初期言語ではborrowをsuspended frameに持ち込まない。
- Mutable serviceをactor-isolatedにし、message payload transferを明示する。
- Ambient runtime checkだけに頼らずtypeからsend／share capabilityをderiveする。
- VM／native modeでordering、cancellation、cleanupをobservableかつ同一に規定する。
- Source span／stable logical diagnosticを維持する。
- Embedded hostがschedulerを提供／driveできるようにする。

## 非目標

- 最終的な `async`、task-group、service、mailbox、deadline、select syntax。
- Borrow-across-suspension、self-referential／pinned task frame、generator、stackful coroutine。
- Lock、read／write guard、atomic、memory ordering、transactional memory、general shared mutable object。
- Scheduler fairness、work stealing、thread count、priority inheritance、real-time guarantee、distributed actor。
- Mailbox backpressure policy、supervision tree、restart policy、persistence、remote transport。
- 完全なpanic／destructor-failure transport。Component-boundary failureはRFC 0003に従います。

## Guide-level design

Spellingは例示です。Structured scopeが全childを所有します。

```text
async fn load_page(urls: List<Url>) -> Result<Page, Error> {
    task_scope tasks {
        let header = tasks.spawn(fetch_header(urls[0]));
        let body = tasks.spawn(fetch_body(urls[1]));
        return Page(await header, await body);
    } // 未完了childをcancel／joinしてからexit
}
```

Borrowは `await` 前に終了する必要があります。

```text
let view = data.read();
consume(view);
end view;
await next_event(); // accepted
```

```text
error[concurrency.borrow-across-suspension]: borrow is live at suspension
  --> worker.lang:9:1
   |
 7 | let view = data.read()
   |            ----------- borrow begins here
 9 | await next_event()
   | ^^^^^^^^^^^^^^^^^^ suspension would retain the borrow
   |
   = help: end the borrow before `await` or move owned data into the task
```

Serviceはmutable stateを所有し、handleはmoved payloadをsendします。

```text
service Counter {
    state value: i64 = 0;

    on Add(amount: i64) {
        value = value + amount;
    }
}

let counter = spawn service Counter();
counter.send(Add(move amount));
// `amount` はunavailable。Ownerがenqueue順で `Add` を処理する。
```

`counter` 経由のdirect mutationは `concurrency.service-isolation`（`SEM0031`）で拒否します。有効なhandleからのcancellation requestはcooperativeです。Active handlerがboundaryへ達し、queued messageをenqueue順でdiscard／dropし、stateを1回dropして、future sendをunavailableとして失敗させます。

## Reference semantics

### Structured taskとsuspension

1. 明示的detached host／root task以外のtaskはparentを1つ持ちます。通常のchild作成はlexical task scope内で行います。
2. Scope exitは全childを待ちます。Normal exitはjoinし、exceptional／cancellation exitは先にcancellationをrequestしてからjoinします。Childがdestroy済みparent frameへaccessすることはありません。
3. Task frameは別の場所へmoveしていない正常initialized localをすべて所有します。Suspensionはowned valueを維持します。
4. Shared／exclusive borrowがliveな間のsuspensionはinvalidです。Rejectionはsuspension／borrow originを示します。Suspension前に終了したborrowはvalidです。
5. このruleはaddress-stability／pinning promiseを避けます。Suspended frameはsource behaviorを変えずmemory内でmoveできます。
6. Child result transferはowned resultをmoveするかtyped errorを返します。未観測child failureはtask-scope APIに従いsibling workをcancelします。正確なerror aggregationはerror RFCで扱います。

### Cancellationとcleanup

1. Cancellationはasynchronous destructionではなくidempotent requestです。Cancelling scopeからunfinished descendantへ伝播します。
2. Taskは `await`、explicit yield、mailbox receive、timer／deadline wait、explicit cancellation checkへ入る前またはsuspend中にcancellationを観測します。Point間のuser codeはpreemptしません。
3. 実行中callback／handlerはcancellation cleanup前に次のsemantic boundaryへ到達します。Native signal／exception injectionはlanguage mechanismではありません。
4. Frameをdestroyする前に、runtimeはcompiler-managed borrow scopeをcloseし、child scopeをjoin／cancelし、残るowned valueを逆initialization順で正確に1回destroyします。
5. Cleanupはsuccess、typed error、panic containment、deadline cancellation、explicit cancellationで実行します。Failure ordering／suppressionはdeterministicでなければなりません。正確なerror encodingは別作業です。
6. Deadlineは観測時にcancellationをrequestするscheduler inputです。Wall-clock accuracy／callback preemptionは保証しません。

### Isolated serviceとmessage

1. Serviceにはstateへのexclusive mutable accessを持つowner taskが1つあります。Shareable handleが伝えるのはenqueue／cancellation-request authorityだけです。
2. Non-ownerのdirect state mutationは `concurrency.service-isolation` で失敗します。Identityが既知ならproposed source type systemがexecution前に拒否し、validated dynamic boundaryもcheckを維持します。
3. Sendはpayloadをconsumeします。Successful send後のreuseはuse-after-moveです。Payloadはlive borrow、arena reference、non-sendable host handle、mutable aliasを含まず、全fieldが再帰的にsendableな場合にsendableです。
4. Compilerが `Send` をderiveし、safe sourceは手動assertできません。`shared T` は、`T` の公開operationが後続 `Share`／synchronization ruleを満たす場合だけsendableです。このRFCはarbitrary shared mutationをsendableにしません。
5. Successful enqueueは1つのservice mailbox positionでlinearizeします。Ownerはaccepted messageをそのpositionのFIFOで処理します。同時sender間のschedulingはdeterministicではありませんが、記録済みenqueue orderは全backendで同じ意味を持ちます。
6. Message handlerはhandler中payloadを所有し、success／failure時に正確に1回destroyまたはmoveします。
7. Cancellationはcurrent owner handler boundaryまで待ちます。その後serviceをunavailableにし、queued messageをFIFOでdiscardし、各queued payload／service stateを1回destroyし、後続send／receive／mutationを拒否します。
8. Mailbox capacity／backpressureはhost／runtime configurationです。Full sendがsuspend、fail、明示policy適用のどれかは後続RFCが規定します。Unbounded implicit growthはsemantic guaranteeではありません。

### Unsafe、embedding、constant execution

Sendable value／service handleを構築する `unsafe`／host codeは、同じownership、alias、liveness invariantを確立する必要があります。Native callbackはowner service operationへ入らずisolated stateをmutationできません。Untrusted componentはresource limit付きの明示的task、timer、service capabilityを受け取り、ambient scheduler／thread accessを得ません。

Constant evaluationはruntime task／serviceを作りません。将来のdeterministic compile-time schedulerには別のcapability／resource designが必要です。

## Execution mode

Typed MIRがshared contractです。Task-scope enter／exit、spawn、suspension、cancellation check／request、join、message move／enqueue／dequeue、service-owner mutation、cleanup edgeをsource span付きで記録します。Validationはlive borrow-across-suspension、escaping child handle、use-after-send、non-sendable payload、non-owner mutationを拒否します。

Bytecode VMは1 host thread上でcooperativeにscheduleできます。Native AOT／JITはthread／event loopを使えます。同じordered external event／enqueue orderに対し、全modeがvalue、diagnostic、cancellation observation point、ownership move、cleanup order、service availabilityで一致する必要があります。Thread選択／timingはobservable guaranteeではありません。

Embedderはschedulerをpoll／block／integrateできます。Component interfaceはRFC 0003 adapter経由でowned／sendable resourceを渡します。Raw task frame、waker、mailbox pointer、service stateはstable ABI valueではありません。

## ABIとcompatibility

Task-frame layout、coroutine state number、waker、executor queue、mailbox representation、thread mappingはunstable implementation detailです。Typed MIR、bytecode、package capability declaration、component adapterは独立してversioningします。

`async` の追加／削除、payload sendability変更、service operation isolation変更、cancellation point変更はcompatibility-sensitiveです。Public metadataは最終的にasync／sendability／isolation effectを記録する必要があります。Migration toolingはbehaviorが一意の場合だけ明示的move／scopeを追加でき、childをsilentにdetachしたりstateをshared-mutableにしたりできません。

## Diagnosticとtooling

- Diagnosticはsuspension／send／mutation spanと関連borrow、move、owner、active-handler spanを維持します。
- Stable identityには `concurrency.borrow-across-suspension`、`concurrency.payload-not-sendable`、`concurrency.payload-moved`、`concurrency.service-isolation`、`concurrency.service-unavailable` を含みます。
- Formatterはtask／service boundaryを維持し、LSP hoverは `async`、`Send`、owner isolation、cancellation pointを表示します。
- Linterはdocumented lifetimeのないdetached task、ignored child result、cancellation checkのないloop、potentially unbounded mailboxを報告します。証明できないscheduler behaviorは主張しません。
- DAP viewはraw runtime addressを公開せずtask ancestry／state、cancellation request／observation、mailbox position、service ownerを表示します。

## Performanceとcost model

Structured scopeにはchild bookkeeping／joinが必要です。Suspensionはlive owned valueをframeへ保存しますが、strict policy下ではlive borrow tokenを保存しません。Message sendはmoveとmailbox synchronizationを行い、multithread runtimeのservice handle／cancellation flagにはatomicまたはscheduler-synchronized stateが必要です。

Languageはscheduler、latency、allocation、throughput budgetを保証しません。Implementationはobservable pointを維持しつつ、non-suspending async callのframe削除、mailbox batching、single-thread embedding specialization、redundant cancellation check削除ができます。

Stabilization前に、spawn／join、suspend／resume、cancellation fan-out／cleanup、single／many senderでのmessage throughput／latency、queued-message cancellation、frame／mailbox peak memory、owner-isolated／experimental exclusive-capability pathをVM、macOS AArch64、Windows x86-64で計測します。

## Securityとtrust boundary

Untrusted bytecode、task metadata、handle、message、host wakeupをvalidateします。Limitはlive task、descendant、frame byte、mailbox message／byte、total send、deadline、host callbackを扱います。Counter exhaustion／malformed state transitionはside effect前にfail closedします。Cancellationは新authorityを与えずcleanupを迂回しません。

Payload sendabilityはownership commit前にcheckします。Enqueue failureではsenderがpayloadを所有したままです。Successful enqueueでsenderの旧valueがunavailableになります。Service cancellation／queue disposalはmailbox invariantが部分更新された状態でarbitrary codeを呼びません。

## 代替案

### 変更しない

Async ownership／service isolationをopenのままにするとcoroutine lowering／runtime schedulingがaccidental semanticsになります。

### Library／tooling solution

Libraryはfuture／actorを公開できますが、no-borrow-at-suspension、compiler-derived sendability、lexical child lifetime、backend間で同一のcleanupを強制できません。Lintはdynamic ownership transferに不十分です。

### Shared frame borrowとexclusive mutation capability

最も強い競合prototypeはshared task-frame borrow-across-suspensionを許可し、有効service-handle holderが1つのmutation capabilityを取得できます。Copyを減らしlock-like APIをsupportできますが、stable frame residency／pinning、lock scheduling、fairness、poisoning、deadlock、reentrancy、cancellation contractが必要です。Cost／ruleが確立していないため、このRFCはstrict baselineを選び、prototypeを将来revision用に維持します。

### Single-threaded event loopのみ

Single event loopは単純でfreestanding／embedded subsetに有用ですが、parallel task executionや複数host threadから使うservice handleを表現できません。Semanticsのimplementationにはなれますが唯一のexecution strategyにはしません。

### Detached futureをdefaultにする

Unscoped futureはspawnを簡単にしますが、childがborrowed／owned parent contextより長生きでき、cancellation／deadline propagationを弱めます。Explicit detached taskはownedな`'static`-like／capability requirementの背後で存在できますがdefaultではありません。

## Implementation plan

1. Source／HIR task scope、suspension point、messageへのmove、service-owner operation、compile-pass／fail fixtureを追加します。
2. No-live-borrow-at-suspension、scoped child handle、`Send` derivation、isolationのtyped MIR operation／validationを追加します。
3. 既存task／service modelをlimit／stable diagnostic付きの1つのdeterministic reference interpreterへ一般化します。
4. VM scheduling／cleanup traceを実装し、同じordered-event fixtureでJIT／AOT差分実行します。
5. Runtime layoutを公開せず明示的scheduler／service capabilityとRFC 0003 resource adapterを統合します。
6. Malformed task state、cancellation race、enqueue commit、queue disposal、handle generationをfuzzします。
7. 上記runtime／memory benchmarkを両tier-1 targetで記録します。
8. Stabilization前にformatter、linter、LSP、DAP、guide、migration supportを追加します。

## Test plan

- Compile-pass：await前にborrow終了、owned localがsuspensionを生存、scoped child join、sendがpayloadをmove、ownerがserviceをmutation。
- Compile-fail：shared／exclusive borrow-across-await、child handle escape、payload reuse、non-sendable payload、non-owner mutation。
- Runtime：各pointでのcancellation、child propagation／join、reverse cleanup、FIFO accepted message、active-handler boundary、queued payload disposal、later-send rejection。
- Differential：VM／JIT／AOTのvalue、event、diagnostic、cancellation point、queue order、cleanup trace。
- Property／fuzz：at-most-once destruction、child escapeなし、send後payload aliasなし、non-owner mutationなし、idempotent cancellation、bounded resource accounting。
- Performance／platform：上記cost seriesをmacOS AArch64／Windows x86-64で計測。

## 未解決事項

Accepted strict initial policyにblocking semantic questionはありません。Repository ownerは2026-10-17にacceptしました。

Optional follow-upはexact surface API、detached-task requirement、mailbox backpressure、supervision／restart、static executor specialization、pinned shared-frame borrow／explicit mutation capabilityを別RFCにするかどうかです。

Decision：Repository ownerにより **2026-10-17にaccepted**。

## 情報源

- [Swift Evolution SE-0304: Structured Concurrency](https://github.com/swiftlang/swift-evolution/blob/main/proposals/0304-structured-concurrency.md)、source最終変更2026-07-29、2026-10-16確認。Scoped child-task lifetime／cancellation propagationを規定します。
- [Pony Tutorial: Reference Capability Guarantees](https://tutorial.ponylang.org/capabilities/guarantees.html)、公開更新日のないliving documentation、2026-10-16確認。Data-race-free actor communicationに使うaliasing guaranteeを説明します。
- `crates/semantics/src/lib.rs`、async-borrow／cancellation model、repository commit `809fee1e09c279d4865811fc86a0972d3d6d37ab`、2026-10-16 commit。
- `crates/semantics/src/service.rs`、isolated／exclusive-capability service model、repository commit `8c473960e2c27a567b61606598c802c9eec9d30e`、2026-10-16 commit。
- [RFC 0004](0004-owned-values-and-cyclic-graphs.md)、owned／shared／arena foundation、最終更新2026-10-16。
