<!-- translation-of: ../../../docs/rfcs/0004-owned-values-and-cyclic-graphs.md; canonical-language: en -->

# RFC 0004: Owned value、明示的共有、cyclic graph storage

[English version](../../../docs/rfcs/0004-owned-values-and-cyclic-graphs.md)

- Status: accepted
- Authors: Project contributors
- Created: 2026-10-16
- Last updated: 2026-10-28
- Tracking issue:
- Supersedes:
- Superseded by:

## 現在の範囲

**2026-10-27 JST確認。** このRFCは受理済み所有権方針です。具体的なsource constructor/core APIとallocator契約はRFC 0007–0009で実装し、M2機能guideに記録しています。logical-task Send/Shareは別途RFC 0024で実装し、すべてのmanaged/host型のOS-thread Send/Syncは保証しません。将来のparallel executorの順序、静的acyclicity注釈、traced heapは別の設計作業です。

[M3完了監査](../milestone-3-capabilities.md)と[今回のレビュー](../review-2026-10-27.md)が現在の証跡です。実装sourceに公開更新日は埋め込まれていません。日付付きcheckpointと元の受理判断は当時の記録として保持します。

## 概要

初期source-language ownership／cyclic-graph policyを定義します。

- Plainな `T` は単一ownerを持ち、暗黙copyではなくmoveし、deterministicにdestroyされます。
- Non-escapingなshared／exclusive borrowは、userがlifetime parameterを書かずに推論されます。
- `shared T` は明示的なatomic reference-counted ownerです。Cloneは明示的です。
- `weak T` はtargetをretainせず、upgradeは明示的に失敗し得ます。
- Cycleを閉じるstrong `shared` edgeの公開は、mutation前にstable diagnostic identity `ownership.strong-cycle`（prototypeでは `SEM0018`）で失敗します。
- Lexicalな `arena` が、任意のcycleを形成できるgraph nodeを所有します。Arena referenceはarena regionからescape／crossできず、arena exitはlive nodeを逆allocation順で正確に1回destroyします。
- Base runtimeはtracing garbage collectorを必須にしません。将来のopt-in traced heapには別RFCと別typeが必要です。

正確なconstructor／parameter-modifier spellingは暫定ですが、ownership category、cycle policy、failure atomicity、execution-mode equivalenceはaccepted semantic commitmentです。

## 動機

この言語は、無制限なshared object graph、deterministic destruction、見えないruntime cost、単純なsyntax、tracing collection不要を同時に最大化できません。Tradeoffを未規定にすると、frontend、MIR、VM、native runtime、component adapterが互換性のないownership behaviorを獲得します。

Reference countingだけでは不十分です。Rustは `Rc<T>` cycleがleakし得ることと、non-owningな `Weak<T>` でcycleを切ることを説明しています。Swift ARCもstrong-reference cycleの解消にweak／unowned relationshipを要求します。Kotlin/Nativeは最も強い競合design、すなわちrootからtraceするshared heapを示します。これらはsource factであり、project decisionではありません。

Repositoryの実行可能graph modelは、2つのpolicyについてlocal evidenceを提供します。`Reject` はcycleを閉じるstrong edgeを拒否し、weak back-edgeを許可します。`CollectUnreachable` はstrong cycleを許可し、root dropごとに残るrootから同期的にtraceします。後者はreclamation behaviorを実証しますが、許容可能なproduction latency、memory、finalization、concurrency behaviorを確立しません。Arena-scoped cycleも、deterministicな逆順teardownとstale-handle拒否付きで実行できます。

Library-only conventionではmove／use rule、arena referenceのescape、全shared-edge mutationの同じpreflight、typed MIRと全backendを通じたstable diagnosticを強制できません。

## 目標

- Owned、borrowed、shared、weak、arena-scoped lifetime categoryを定義する。
- Mandatory tracing collectorなしでdeterministic destructionを維持する。
- Reference-counting／cycle behaviorをsource type上で可視にする。
- Cycleを形成するstrong shared edgeをobservable mutation前に拒否する。
- 意図的なcyclic dataを明示的でboundedなarena lifetime内で扱う。
- Bytecode VM、JIT、AOT、embedding pathの同一outcomeを規定する。
- Source spanとstableなlogical diagnostic identityを維持する。
- Handle／foreign codeのcompatibility／trust-boundary ruleを定義する。

## 非目標

- 最終的なparameter-modifier、constructor、graph-container API spelling。
- Suspensionをまたぐborrow、cancellation cleanup、actor／service isolation、atomic、scheduling、lock semantics。これらはconcurrency RFCで扱います。
- Self-referential movable valueまたはpinning。
- General tracing collector、cycle collector、finalizer、object resurrection model。
- Production allocatorの選定、allocation／retain-release／cycle-check performanceの保証。
- RFC 0003のcomponent resource representation変更、またはstable ABIでのsource heap handle公開。
- Panic payload、destructor-failure transport、完全なerror／effect syntaxの定義。

## Guide-level design

以下のspellingは例示です。このRFCはtype categoryとbehaviorをnormativeにし、syntax-focused revisionがconstructor／mutation APIを調整できます。

```text
record Node {
    name: String,
    parent: weak Node?,
    children: List<shared Node>,
}

let root: shared Node = shared(Node(name: "root"));
let child: shared Node = shared(Node(name: "child"));

root.children.push(child.clone()); // 明示的なstrong ownership
child.parent = weak(root);         // retainしないback-edge

if let some(parent) = child.parent.upgrade() {
    print(parent.name);
}
```

Typeはstrong peer edgeを意図的に保存できますが、cycleを閉じるedgeはfield、reference count、graph metadataを変更する前に失敗します。

```text
record Peer { peer: shared Peer? }

let left: shared Peer = shared(Peer());
let right: shared Peer = shared(Peer());
left.peer = right.clone();
right.peer = left.clone(); // reject：`left` からすでに `right` へ到達する
```

```text
error[ownership.strong-cycle]: strong shared edge would create a cycle
  --> peers.lang:6:1
   |
 6 | right.peer = left.clone()
   | ^^^^^^^^^^^^^^^^^^^^^^^^^ cycle-closing write
   |
   = related: existing strong path reaches `right`
   = help: use `weak(left)` or place the graph in an `arena`
```

意図的なcyclic topologyを持つgraphはlexical arenaを使います。

```text
arena graph {
    let left = graph.new(Node(name: "left"));
    let right = graph.new(Node(name: "right"));
    left.next = right;
    right.next = left;
} // `right`、次に`left`を正確に1回destroy
```

Arena referenceはreturn、global／`shared` valueへの保存、より長寿命なclosureへのcapture、別arenaへの送信、arena exit後のdereferenceができません。Compilerがescapeを証明できればsource errorを報告します。Validated MIR、bytecode、dynamic embedding boundaryはdefense in depthとしてregion／generation checkを維持します。

## Reference semantics

### Owned valueとborrow

1. Plainなnon-copy `T` には正確に1つのownerがあります。Assignment、owning parameterへのargument passing、return、captureは、明示的clone operationを選ばない限りvalueをmoveします。
2. Move済みvalueのread／mutationを拒否します。Partial-move semanticsを後続RFCが定義するまで、partially initialized aggregateのmoveを拒否します。
3. Ownerが正常にinitializedされたscopeを離れるとdestructionが起きます。Localとaggregate fieldは逆initialization順でdestroyされます。
4. Shared borrowはreadと他のshared borrowとのaliasを許可します。Exclusive borrowはmutationを許可し、他の全live borrowを排除します。Safe codeはownerのmove／destroy後にborrowを観測できません。
5. 通常のnon-escaping borrowはlexical inferenceを使います。Borrowのstore、return、その他のescapeには後続の明示的owning abstractionが必要です。このRFCはuser-written lifetime parameterを導入しません。
6. どのborrowがsuspension／task boundaryをまたげるかはasync／concurrency RFCが決めます。

### Shared／weak ownership

1. `shared T` はheap allocationへのstrong referenceを1つ所有します。別のstrong owner作成は明示的で、atomic strong countを増やします。最後のstrong ownerをreleaseするとuser destruction前にallocationをunavailableにし、resurrectionを禁止します。
2. `weak T` はliveな `shared T` からのみ作成します。Livenessには寄与しません。`upgrade` はallocationがliveな場合だけ、新しいstrong ownerを含むoptional／result-like successを返します。
3. Strong edgeは、別のshared allocationから到達できるstored `shared` ownerです。`from` から `to` への新規／置換strong edgeを公開する前に、runtimeは両endpointをvalidateし、`to` から `from` へ既存strong pathがあるかcheckします。
4. 公開がcycleを閉じる場合、destination field、strong count、edge metadata、その他のownership stateを変更する前に `ownership.strong-cycle` で失敗します。Diagnosticはwrite spanと、利用可能なら既存pathのrelated spanを保持します。
5. Weak edgeはcycle rejectionのreachabilityに含めず、targetをretainしません。Expired weak edgeのupgradeはallocationをreviveせず、`ownership.expired-weak`（prototypeでは `SEM0021`）を報告します。
6. Shared-edge replacementは1つのobservable operationとしてcommitします。Validationと必要なreservationを先に行い、その後、user callbackを挟まずに新edge／countの追加と旧edge／countの削除を可視化します。
7. Shared mutation authorityの取得方法はconcurrency RFCが定義できますが、全implementationはcycle preflight後に成功publicationをlinearizeし、同等のordered executionで同じsuccess／failure outcomeを生成する必要があります。

### Arena-scoped graph

1. `arena name { ... }` は一意のlexical ownership regionと唯一のarena ownerを導入します。Allocationはhidden region identityでbrandされたnon-owning referenceを返します。
2. Arena nodeは同じlive arena内のnodeとの任意のstrong cycleを含められます。Arena edgeはnodeごとのreference countを維持しません。
3. 異なるarena間、またはarena storageとshared heap間のedgeは、後続RFCが明示的owned copy／component-resource adapterを定義しない限り `ownership.cross-region-edge`（prototypeでは `SEM0017`）で拒否します。
4. Arena referenceはarena regionより長生きせず、外へescapeできません。Region identityは推論され、通常のsourceでlifetime parameterとして書きません。
5. Arena exitは新しいaccessを最初に禁止し、その後、graph topologyに関係なく、正常にinitializedされた全live nodeを逆allocation順で正確に1回destroyします。
6. Stale／forged arena referenceがvalidated dynamic boundaryへ到達すると、`ownership.unavailable-arena-reference`（prototypeでは `SEM0012`）で拒否します。後続allocationをaliasしません。Numeric slotを再利用できる箇所ではgeneration tag付きhandleがimplementation要件です。
7. User destructorはarena nodeをresurrectしたり、exit中regionの外へarena referenceを公開したりできません。Panic／destructor failure orderingは別のerror-semantics decisionですが、全backendはcompiler-mandated ownership bookkeepingを一貫して継続する必要があります。

### Constant execution、unsafe、foreign code

Constant evaluationは、全allocation／destructionがevaluatorのresource limit内で完結する場合だけowned value／arenaをsupportできます。Deterministic allocation／atomic behaviorを規定するまで、constant executionでは `shared`／`weak` operationを拒否します。

Raw pointer／foreign referenceは `shared`、`weak`、arena referenceではありません。Safe ownership valueを構築する `unsafe` codeは、同じuniqueness、liveness、cycle registration、region invariantを確立する必要があります。Foreign codeはruntimeの外側でtracked strong edgeをmutationし、そのobjectをsafeな `shared T` として公開できません。

## Execution mode

Typed MIRが共通semantic contractです。Implementation planはmove、borrow begin／end、shared clone／release、weak downgrade／upgrade、checked strong-edge publication、arena enter／allocate／exitのbackend-neutral operationを追加します。MIR／bytecode validationはmalformed region flow、use-after-move、対応しないborrow scope、type不整合なownership operationをexecution／native lowering前に拒否します。

VMはこれらのoperationを直接、またはshared runtime経由で実装します。JIT／AOTは証明済みretain／release／cycle checkをinlineできますが、optimizationはobservable destruction order、weak-upgrade result、cycle rejection、diagnostic identity、failure atomicityを変更できません。Native unwindはsemantic mechanismではありません。

Embedding APIはhost addressではなくopaqueなgeneration-checked valueを公開します。RFC 0003の `own<T>`／`borrow<T>` component resourceが受理済みだが公開安定化前のcross-component ownership baselineであり、sourceの `shared`、`weak`、arena referenceをserializeしたり直接component boundaryへ渡したりしません。WebAssembly implementationは同じlogical outcomeを持つadapterを使います。

## ABIとcompatibility

このRFCはsource-level semantic categoryを導入し、stable native layoutは導入しません。Reference-count header、edge metadata、arena allocation strategy、generation widthはimplementation detailです。Native ABI、bytecode、typed MIR、debug metadata、serialized component contractは独立してversioningします。

Public field／parameterをowned、borrowed、`shared`、`weak`、arena-scoped form間で変更するとsource／behaviorへ影響します。Public interfaceへ入った時点でpackage metadataはnormalized ownership categoryを記録する必要があります。Edition migrationはbehaviorが一意の場合だけ明示的clone／downgrade operationを挿入できます。Strong edgeをsilentにweak edgeへ置換したりdataをarenaへ移動したりしてはいけません。

## Diagnosticとtooling

- Parser／formatterは `shared`、`weak`、`arena` formをdeterministicに維持します。
- Move／borrow diagnosticはmove／borrow originとinvalidな後続useを示します。
- Cycle、cross-region、expired-weak、stale-arena diagnosticはHIR、MIR、bytecode、VM、native debug metadataを通じてstable logical identityとsource spanを維持します。
- LSP reference／hoverはownership categoryとclone、weak upgrade、arena boundaryの関与を表示します。
- Linterは不要なshared ownership、即座にexpireするweak value、acyclicでowned storageを使えるarena valueをwarningします。証明できないruntime cycleを主張してはいけません。
- DAP viewはraw addressをdefaultで公開せず、owned value、strong count、weak handle、arena identity、unavailable handleを区別します。
- Documentation generationはpublic signatureのownership categoryをrenderし、cycle-forming diagnosticをweak／arena guidanceへlinkします。

## Performanceとcost model

Owned move／inferred borrowにreference countは不要です。`shared` clone／releaseは、whole-program analysisが安全に除去しない限りatomic reference-count trafficを行います。`weak` storageにはliveness metadataとupgrade checkが必要です。

Strict shared-cycle policyは、安全だと証明できないpublicationごとに、到達可能なstrong vertex／edge数に比例するworst-case graph traversalを持ちます。Implementationはtopology cache、ownership-tree summary、strong fieldを持たないtypeのspecialization、static proofを利用できますが、cycleの可能性が残るcheckを省略できません。これらのoptimization／invalidation costはlanguage guaranteeではありません。

Arena allocationはbump-likeにできますが、保証するのはbulk lifetimeと逆allocation-order destructionだけです。Allocation complexity、reuse、fragmentation、deallocation batchingはimplementation detailです。

現在のprototypeはoutcomeを実証しますがproduction costを実証しません。Stabilization前に、owned move、shared clone／release、weak upgrade、複数graph sizeでのacyclic checked-edge insertion、拒否されるcycle insertion、arena allocation／teardown、peak memory、allocator／metadata overheadをVM／native executionで別々に計測する必要があります。このRFCはlatency／memory budgetを確立しません。

## Securityとtrust boundary

Source、MIR、bytecode、component input、numeric handle、foreign referenceは各boundaryでuntrustedです。Validatorはdereference前にtype、region、generation、operation orderingをcheckします。Cycle preflight／edge commitはuser callbackを呼びません。Countのinteger overflow、generation exhaustion、allocation failure、malformed region flow、resource limitはstale valueをaliasせずfail closedします。

Untrusted pluginはnative address／ambient allocator accessを受け取りません。Capability policyがscript／component executionのallocation／graph-size limitを制御します。Native pluginはtrustedで明示的opt-inが必要です。Reference counting／arenaはnative pluginをsafeにしません。

## 代替案

### 変更しない

Ownershipをarchitecture hypothesisのままにすると、parser、MIR、runtime、component workが互換性のないmove、destruction、cycle behaviorを作れます。Milestoneのdesign-baseline要件を満たしません。

### Library／tooling solution

Smart-pointer／arena libraryは有用なAPIを公開できますが、use-after-move、borrow exclusivity、arena escape、backend-equivalent cleanup、完全なedge registrationをsafe-language invariantにはできません。Lintはdynamic cycleを完全には検出できません。

### Tracing shared heap

最も強い競合designはKotlin/Nativeのようなtraced shared heapです。Unreachable strong cycleを自然に回収し、edge publicationごとのpath checkを避けますが、collection trigger、tracing work、pause／concurrency policy、weak／finalizer ordering、より大きなmandatory runtime、nondeterministicなreclamation timingを導入します。`CollectUnreachable` prototypeはsemanticsを示しますが、許容可能なproduction collectorを示しません。このRFCはbase `shared` representationとしてtracing collectionを退けます。将来の明示的traced typeは可能です。

### Arena-only graph

Owned value／arenaだけをsupportする案は単純で、cyclic lifetimeを明示しますが、周囲のarenaより先にnodeをreleaseすべき独立shared DAGを表現できません。明示的shared ownershipが必要なservice／cacheにもbulk lifetimeを強制します。完全なdesignではなく、有用なrestricted／freestanding subsetです。

### Reference-counted cycleを許可してwarningする

Rust `Rc`／Swift ARCが示す馴染みのあるdesignです。Dynamic cycle-preflight costを避けますが、safeで到達可能に見えるoperationがunreachable graphを無期限にretainできます。Warningはdynamic mutationを完全には扱えません。このRFCはno-silent-cycle guaranteeを選び、そのvisible runtime checkを受け入れます。

## Implementation plan

Policyは受理済みですが、例示はsource grammarの受理ではありません。Milestone 2ではsource ownership／core-library profileと残るcleanup／FFI／allocator contractの受理を記録してから依存実装へ進みます。既存graph modelは比較evidenceであり、production retain／releaseやatomic cycle publicationではありません。

1. 別途受理したsource profileに従い、source／HIR ownership categoryと、move、borrow conflict、escaping arena reference、cross-region edgeのcompile-fail testを追加します。
2. Typed MIR ownership／region operationとvalidationを追加し、source span／stable diagnostic identityを維持します。
3. 既存 `crates/semantics` graph modelを、generation tag付きhandle／resource limitを持つruntime-facing checked-edge／lexical-arena prototypeへ一般化します。
4. 同じfixtureをbytecode VM／native executionへlowerし、event trace、destruction order、weak upgrade、全failure outcomeを比較します。
5. Source heap layout／raw handleを公開せず、source ownershipをRFC 0003 adapterへ統合します。
6. Malformed MIR／bytecode operationとrandomized graph sequenceをfuzzし、generation／count exhaustion、failed-publication atomicityを扱います。
7. Stabilization前に、上記runtime／memory benchmarkをmacOS AArch64／Windows x86-64で記録します。
8. Accepted policyを実装するようguide、formatter、linter、LSP、DAP、migration toolingを更新します。将来のsemantic changeには新しいRFC dispositionが必要です。

## Test plan

- Compile-pass：owned move into return、inferred read borrow、exclusive mutation、明示的shared clone、weak parent、same-arena cycle、deterministic arena exit。
- Compile-fail：use after move、overlapping exclusive／shared borrow、escaping arena reference、cross-arena edge、arena-to-shared storage、arenaより長寿命なclosure。
- Runtime：拒否されたstrong cycleがfield／countを変更しない、weak edgeがback-edgeを切る、expired upgradeがreviveしない、last strong releaseが正確に1回destroyする、arena cycleが逆allocation順で正確に1回destroyする、stale generationがreuseをaliasしない。
- Differential：success／failure fixtureでVM／JIT／AOTのvalue、event trace、destruction order、diagnostic、source spanが一致する。
- Boundary：component adapterが明示的 `own`／`borrow` conversionを受け入れ、raw shared／weak／arena handleを拒否する。Host callbackはedge preflightを迂回できない。
- Property／fuzz：random operationがnonnegative count、live strong cycleなし、region separation、stale aliasなし、at-most-once destruction、failure atomicityを維持する。
- Performance：performance sectionのgraph-size series／allocation-memory profileを両tier-1 targetで計測する。

## 未解決事項

Accepted policyにblocking semantic questionはありません。Repository ownerは2026-10-17にacceptしました。

Constructor／core APIの表記とallocator権限は実装済みRFC 0007／0008で決定済みです。任意の将来検討は静的acyclicity注釈、将来のOS-thread並列executorでのweak upgrade順序、別名を持つtraced heapです。Logical taskのSend／ShareはRFC 0024で対応済みですが、host thread全般の安全性を意味しません。

Decision：Repository ownerにより **2026-10-17にaccepted**。

## 情報源

- [The Rust Programming Language, “Reference Cycles Can Leak Memory”](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html)、source最終変更2025-10-03、2026-10-16確認。`Rc<T>` cycleがleakし、`Weak<T>` はtargetをretainしないことを説明します。
- [The Swift Programming Language, “Automatic Reference Counting”](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/LanguageGuide/AutomaticReferenceCounting.md)、source最終変更2026-01-09、2026-10-16確認。Strong-reference cycleとweak／unowned relationshipを説明します。
- [Kotlin/Native memory management](https://kotlinlang.org/docs/native-memory-manager.html)、公開更新日のないliving documentation、2026-10-16確認。Shared heap、root-based tracing、concurrent mark-and-sweep collection、weak-reference processingを説明します。
- `crates/semantics/src/graph.rs`、strict／tracing／arena graph prototype、repository commit `d4925b8c1d97a49916d6c3f7cd2274f9672e1c50`、2026-10-16 commit。
- [RFC 0003](0003-versioned-component-contracts.md)、source／component ownership boundary、最終更新2026-10-16。
