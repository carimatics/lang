<!-- translation-of: ../architecture.md; canonical-language: en -->

# アーキテクチャ提案

[English](../architecture.md)

ステータス：**提案中の設計baseline**。個々の意味論にはRFCの承認とprototypeが必要です。

## 目標

- WindowsとmacOSでのnative性能と低レベル制御。
- Tracing GCを必須とせず、既定でmemory-safeかつdata-race-safeであること。
- Application codeに適した、生産性の高いexpression-oriented language。
- 1つのsemantic frontendを使用したAOT executable／libraryとembeddable script。
- 同一process内での効率的なnative-to-script／script-to-native call。
- 1つのfrontend ABIに互換性を固定せず、将来複数source languageを追加できる経路。
- 優れたdiagnostics、build speed、debugging、IDE support、reproducibility。

## 初期実装の非目標

- すべてのCPU architectureまたはOSをsupportすること。
- Native language ABIをstableにすること。
- 任意のnative codeを安全にhot reloadすること。
- 透過的なdistributed execution。
- Core languageにtracing GCを含めること。
- 最初のfrontendとcomponent boundaryが安定する前に複数source languageを実装すること。
- Local／Git-based dependency workflowがreproducibleになる前にpackage registryを作ること。

## プロダクトモデル

明示的な2つのprogramming layerを持つ1つの言語から開始します。

- **Safe application layer:** Value-oriented API、null safety、algebraic data type、pattern matching、type inference、trait、closure、structured concurrency、必要な場合のmanaged shared reference。
- **Systems layer:** Allocator、raw pointer、layout control、C ABI、atomic、SIMD、inline assembly／intrinsic、platform APIを `unsafe` またはcapability-gated interfaceの背後に置く。

これにより、相互運用を1つのtype system内に保ちます。2つ目の言語は、安全性を弱めることなく同じcomponent modelとconformance suiteへloweringできる場合にのみ追加します。

## コンパイラパイプライン

```text
source text
    │
    ▼
lossless syntax tree ───────────── formatter / IDE recovery
    │
    ▼
resolved HIR ───────────────────── names / modules / desugaring
    │
    ▼
typed HIR ──────────────────────── inference / traits / effects
    │
    ▼
MIR ────────────────────────────── ownership / destruction / async lowering
    │
    ├──────────────► bytecode ───► verifier ─► embedded VM
    │
    ├──────────────► native SSA ─► Cranelift/LLVM ─► object/linker
    │
    └──────────────► interface IR / metadata ───────► bindings and plugins
```

型付きMIRをsemantic contractとします。Backend固有のoptimizationは異なっても構いませんが、観測可能な動作、destruction order、panic／error behavior、integer semantics、concurrency ruleは異なってはいけません。

[RFC 0002](rfcs/0002-prototype-control-flow.md)は、実装済みMIR subsetの最初のsource interpretationとして、`i64` parameter／result、wrapping addition、signed comparison、mutable local、`if`／`else`、`while`、明示的 `return` を提案します。Left-to-right evaluationを仕様化し、join／back-edgeをまたいでliveなvalueをtyped block argumentでloweringします。このRFCはrepository ownerのdecisionに向けてreadyですがproposedのままです。Parser、HIR、source diagnostics、formatter、source-to-MIR loweringはまだ実装されていません。

### Incremental architecture

最初の非自明なfrontendからdependency-tracked query systemを使用します。各queryはdeterministicで、個別に計測できる必要があります。

- Fileをparseする
- Resource-boundedなcompile-time constructを展開する
- Module／import graphをresolveする
- Nameをresolveする
- Typeとeffectをinfer／checkする
- Ownershipとexclusivityをcheckする
- MIRをlower／optimizeする
- Unit単位でbytecodeまたはnative codeを生成する

Hashとtiming explanationを永続化し、build toolがunitを再buildした理由を回答できるようにします。Source fileとpackage manifestを明示的なquery inputとして扱います。

## Memory model仮説

この設計では、memory safety、一般的なshared object graph、単純な構文、deterministic destruction、zero overhead、GCなしをすべて同時に最大化できないという基本的トレードオフを認識する必要があります。

次のmodelをprototypeします。

- 通常の `T` はdeterministic destructionを持つowned valueである。
- Moveは推論し、use-after-moveをrejectする。
- 自明でないvalueのfunction parameterは、曖昧さがなければ既定でnon-owning readとする。
- Mutable accessはexclusiveかつlexicalとする。通常のcodeでlifetime parameterを書かせない。
- Non-escaping borrowは推論され、明示的なowning abstractionなしには保存できない。
- `shared T` は明示的なatomic reference-counted referenceであり、`weak T` でcycleを切る。
- Region／arenaにより、graph、compiler、game、request-scoped dataをbulk lifetime managementできるようにする。
- Closureはcaptureをborrow、move、shareのどれで行うかを表す。
- Raw pointer、union、unchecked indexing、layout cast、foreign ownershipは狭い `unsafe` boundaryの背後に置く。
- Standard libraryはallocator-aware containerを公開し、後にno-runtime／freestanding subsetを許可する。

[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)は、完了済みgraph-policy比較をdecision-readyなproposalへまとめます。Plain valueは単一ownerを持ち、`shared`／`weak` がreference countingを可視化し、cycleを閉じるstrong shared edgeはmutation前に失敗し、lexical arenaがtracing collectionを必須にせず意図的なcycleを所有します。Repository ownerがdispositionを記録するまではproposedのままです。

Prototypeが必要な未解決問題には、borrowを伴うasync suspension／cancellation、self-referential value、cancellation中のdestruction、atomic `shared` reference、cycle preflight、arena storageのruntime／memory costがあります。

### Cyclic graph実験

`crates/semantics` のgraph modelでは、2つのshared-cycle policyの下で、明示的なshared／weak ownershipとarena-scoped storageを比較します。

- Shared nodeはexternal rootを1つ持って開始する。Strong edgeはtargetをretainし、weak edgeはretainしない。
- Defaultの `Reject` policyは、shared-reference cycleを閉じるstrong edgeをstructured diagnostic `SEM0018` でrejectする。同じcycleをweak edgeで閉じることは許可し、expired weak edgeのupgradeは `SEM0021` を報告する。
- 実験的な `CollectUnreachable` policyはstrong cycleを許可する。External rootをdropした後、残るすべてのlive rootからstrong edgeをtraceし、到達不能なshared nodeを逆生成順にreleaseする。Weak edgeはtargetをliveに保たない。
- 1つのarena内のnodeは、arenaがlifetimeを所有するため任意のcycleを形成できる。Arena間、またはarena storageとshared storage間のedgeは `SEM0017` を報告する。
- Arenaをdropするとnodeを逆生成順に正確に1回releaseする。その後にstale node handleを使うと `SEM0012` を報告する。
- すべてのoperationがsource spanを持ち、関連するallocationまたはedge spanが存在する場合はdiagnosticに保持する。

どちらのshared-cycle policyも承認済みsemanticsではありません。RFC 0004はstrict policy、weak back-edge、lexical arenaを提案し、別名のtraced heapを将来RFC用に保留します。Collector policyはrootから到達可能な間はcycleがliveのままで、最後のrootが消えると回収されることの実行可能evidenceのままです。Collectorは意図的に同期的で、root dropごとにstrong edgeを全traceします。Incremental／concurrent collection、finalization、atomic、thread safety、destructor failure、weak-reference orderingはmodel化していません。Library規約だけではstale-handle rejectionを保証できず、loweringを通じて同じdiagnostic contractを維持できません。

レビュー結果：**RFC 0004でready for decision**。Reject-and-weak、trace-and-collect、arena teardown behaviorが実行可能であり、RFCがsemantic／cost tradeoffを記録します。Production benchmark、最終surface spelling、optimization、allocator policy、将来のopt-in traced heapは、未規定semanticsではなくimplementation／follow-up workです。

### Async borrow／cancellation実験

`crates/semantics` の実行可能modelでは、意図的に厳格なbaseline仮説と、明示的な1つのより制限の少ないpolicyを比較します。

- Task-owned valueはsuspensionを越えてtask frame内でliveのままにする。
- Defaultの `RejectAll` policyは、suspension pointでactiveなshared／exclusive borrowをstructured diagnostic `SEM0007` でrejectし、suspension前に終了したborrowをacceptする。
- 実験的な `AllowSharedFrameBorrows` policyはtask-owned valueのactiveなshared borrowがsuspensionを越えることを許可する一方、exclusive borrowは引き続き `SEM0007` でrejectする。
- Cancellationはtaskがsuspendedである間だけ観測する。
- 実験的policyでは、resume後もshared borrowを維持し、cancellation時はactive borrowを終了してからtask-owned valueを逆生成順に正確に1回dropする。
- すべてのoperationがsource spanを持ち、failureではboundaryとそれを越えたborrowの両方を示す。

どちらのpolicyも承認済み意味論ではありません。新しい比較は、frame-residentなshared borrowがsuspend／resumeを越えて存続し、exclusive accessを弱めずにcancellation cleanup前にrevokeできることのevidenceです。Address stability、pinning、suspensionを越えるexclusive borrow、task frame外へescapeするborrowはmodel化していません。Library規約だけではcompilerのborrow ruleを強制できず、決定を先送りすると未解決のcancellation／self-reference制約がMIR loweringへ流入します。後続prototypeではnested scope、partial initialization、destructor failure、panic containment、self-referential task frame、VM／native trace、frame borrow維持のruntime costを扱う必要があります。

レビュー結果：**needs prototype/measurement**。Shared frame-resident caseは実行可能になりました。blocking questionは、pinned／exclusive borrowがsuspensionを越えられるか、source semanticsがframe residencyとaddress stabilityをどう証明するか、destructor failure時にcleanupがどう動作するかです。Surface syntaxとscheduler APIは任意のfollow-upです。

## Error／effect model

- Recoverable errorはtyped result valueを使用し、surface sugarとして `throws E`／propagation syntaxを提供する。
- Panicはinvariant violationを表し、component／FFI boundaryを越えない。
- Native unwindingをstable ABIの一部にしない。
- Effectはまず、`async`、`unsafe`、host capability、場合によってはallocationなど、安全性やcompositionを変える動作だけを追跡する。Prototypeでergonomic valueが証明されるまでgeneral effect systemを導入しない。
- Cancellationはstructuredでtask scopeを伝播し、destructorとcleanup blockを予測可能に実行する。

## Concurrency model

1つの抽象を強制せず、複数layerを組み合わせます。

- Application code向けのstructured task、cancellation、deadline、async I/O。
- Mutable service state向けのactor isolationとmessage passing。
- Data parallelism向けのparallel iteratorとscoped fork／join。
- Systems code向けの明示的atomic、lock、channel、unsafe synchronization。
- Ownershipとmutabilityからsend／share capabilityを導出し、data raceを静的にrejectする。

Schedulerは交換可能なruntime serviceとします。Libraryはglobal executorを前提とせず、embedded hostがschedulerをdriveまたは統合できる必要があります。

### Shared mutable service実験

`crates/semantics` のservice modelでは、厳格なactor-isolation仮説と、明示的な1つのshared-mutation policyを比較します。

- Defaultの `OwnerIsolated` policyでは、1つのowner taskだけがservice stateへ直接mutable accessを持つ。Shared handleはmessageをenqueueできますが、別taskからのdirect mutationは `SEM0031` を報告する。
- 実験的な `ExclusiveCapability` policyでは、有効なhandleを持つ任意のtaskがactive mutationのないときにmutationを開始できる。Handleを持たないtaskはrejectし、同時beginはactive mutation spanを伴う既存のstructured mutation-conflict diagnosticを報告する。
- Sendはpayloadをmailboxへmoveする。Non-sendable payloadは `SEM0028`、成功したmove後の再利用は `SEM0027` を報告する。
- Ownerは引き続きmessageをFIFO順にprocessする。有効な任意のhandleからのcancellation requestは、実験的policyでのnon-owner mutationを含め、active mutationが終了するまでdeferする。
- Cancellationはqueued messageをFIFO順にdiscardし、queued payloadとservice stateをそれぞれ正確に1回dropしてから、後続sendを `SEM0024` でrejectする。
- Operationはstable diagnostic code `SEM0022`〜`SEM0035` のもとでprimaryおよび関連source spanを保持する。

どちらのmutation policyも承認済みsemanticsではありません。Exclusive-capability pathはlock-likeな代替案の実行可能evidenceですが、read／write capability、atomic、lock scheduling、fairness、priority inversion、poisoning、deadlock、reentrancy、panic behavior、mailbox backpressure、distributed serviceはmodel化していません。より単純な代替案はshareable handleを持たないsingle-threaded event loopです。Library-only actorではsendabilityやisolationを言語levelのpropertyとして強制できません。

レビュー結果：**needs prototype/measurement**。Actor isolationと1つのexclusive-capability代替案は実行可能になりました。Blocking questionは、send／share／mutation capabilityをどう導出するか、lock schedulingとfailureがcancellation／backpressureとどう組み合わさるか、VM／native executionが同等のorderingとcostを維持するかです。Surface syntaxとstandard actor APIは任意のfollow-upです。

## AOTとscript実行

### Bytecode first

AST interpreterではなく、typed register-based bytecode VMを使用します。

- VMは実行前にbytecodeを検証する。
- Instructionは有用な範囲でlanguage-level typeとsource locationを保持する。
- Native codeとscript codeの双方で、同じruntimeがstring、collection、shared reference、task primitive、panic／error boundaryを実装する。
- Profiling後にinline cacheとspecializationを追加できる。
- JIT compilationはoptionalであり、stable bytecode semanticsの後に実装する。

### Embedding API

実装言語に関係なくstable C ABI embedding layerを提供します。

- Runtimeとisolated script contextの作成／破棄。
- Memoryからverified bytecode／componentをloadする。
- Native function、type、constant、capabilityをregisterする。
- Validated function handleでexportをcallする。
- Generated bindingまたはopaque handleでvalueを変換する。
- Memory、instruction／fuel、recursion、task、wall-clock limitを設定する。
- Executionをinterruptし、structured diagnosticとstack traceを受け取る。

Call-site validationとgenerated thunkをcacheします。Callはin-processのままでIPCを回避しますが、boundary checkとrepresentation conversionは実在するcostであり、benchmarkする必要があります。

[RFC 0003](rfcs/0003-versioned-component-contracts.md)は、最初の実行可能なcomponent-contract sliceを提案します。Runtimeは現在、名前付きscalar import／export、保守的identifier、明示的capability requirement、embedderが供給する論理name-to-host-function bindingを持つschema-versioned package metadataをvalidateします。Portable metadataはprocess-localな `HostFunctionId` valueを含みません。Validationは構造的conflictとunsupported import typeを拒否し、undeclared importへのbindingを拒否し、host lookupより前にcapability grantをcheckし、すべてのimportをtyped registryに対してresolveし、resolved local bindingを持つvalidated wrapperを返します。Deterministicな実験的 `LCMP` schema-1 codecはscalar declaration subsetをencodeし、明示的なtotal-byte、declaration-count、structural-depth、string-length limitの下でuntrusted byteをdecodeします。Malformed-input errorはbyte offsetを持ち、libFuzzer targetがdecoderを実行します。これは選定済みcomponent serializationではなくunstableなfixtureであり、WIT実装、component-IDL resource declaration、canonical ABI、component exportのexecution pathでもありません。これらは提案中boundaryのblocking partとして残ります。

Component runtimeにはin-memoryのgeneration tag付きresource tableもあります。Copy可能なowned handleはuntrusted callerが複製できるinteger handleをmodel化します。Resourceをreleaseすると最小のfree slotを再利用する前にgenerationを進め、generation exhaustionの場合はslotをretireするため、stale copyはreplacementをaliasできません。Shared borrowは共存でき、exclusive borrowにはactive borrowがないことを要求し、mutable accessにはliveなexclusive tokenを要求し、releaseはすべてのlive borrowを拒否します。Borrow tokenはmonotonicで明示的に終了します。Stale token、repeated end、repeated release、conflicting borrow、identifier exhaustionはstructured errorです。これはtable-local lifecycle checkを実証するもので、serialized handle layout、cross-component ownership transfer、destructor behavior、thread safety、canonical resource loweringではありません。

Runtimeはschema-1 serialization／package-manager resolutionから独立したpackage／interface compatibility negotiationもprototypeします。Normalized semantic versionはprerelease／build identifierを保持し、numeric prerelease identifierをmachine wordへ制限しません。Exact requirementは完全なvalueを比較し、intervalはSemVer precedenceと明示的なprerelease policyを使用します。Capability grantはprovider inventory validationより先にcheckします。Exact lock inputは1つのidentity、version、opaque digestをfallbackなしで要求し、それ以外ではexact keyとrequired-memberのstructural projectionでcandidateをfilterしてからdeterministicにhighest-precedenceを選択します。Projectionは順序付きscalar type、nominalなowned／borrowed resource type、exact capabilityを含み、要求されていないprovider memberを許可します。Same-precedenceのbuild／digest tieはambiguousで、resolution成功時はimmutable bindingを返します。これはin-memoryのdesign prototypeであり、stable metadata／digest encoding、package-manager integration、VM／JIT／AOT instantiation pathではありません。

実行可能なschema-1 scalar fixtureは、validated component metadataを3つすべてのprototype execution pathへ接続します。Capability／logical-name bindingをtyped host registryに対してvalidateしてから、選択した `i64 -> i64` import／export pairをMIRへloweringします。VM／JITはinteger extremeを含む同じfunctionを同等のwrapping resultで実行します。同じvalidated fixtureがhost-native objectをemitし、そのexportとresolved host importをmacOS AArch64のdirect／response-file pathでlink／実行します。これは1つのscalar contractに対する狭いdifferential evidenceであり、general component loader、canonical resource adapter、failure boundary、stable serialization、cross-platform component runtimeではありません。

Backend-neutralなcanonical-resource adapter traceにより、runtime handle layoutを公開せずRFC 0003のownership／failure orderingを実行可能にしました。Instanceごとのboundary indexは非ゼロかつ単調増加で、再利用しません。Call全体のpreflightはすべてのresource argumentとdestination limitをvalidateし、storageを予約してから、allocationもcallbackも行わないcommitでownerをmoveまたはcall-scoped borrowを公開します。Cleanupはnested borrowを内側から閉じ、残るframe ownerをremoveしてからacquisitionと逆順にdestructorを呼び、独立したdestructor failure後も継続し、先行するpanic／trapをprimaryのまま保ち、callback／invariant／destructor failure後にcalleeをpoisonします。このprototypeはopaque resource identityとsimulated destructor callbackを使用します。Host `ResourceTable` valueやVM／JIT／AOT resource loweringには未接続で、stable failure serializationやasynchronous cancellationを定義しません。

VM prototypeは、合成可能な3つのopt-in limitであるinstruction fuel、host-call budget、value-slot limitに加えて、optionalな共有one-way interruption flagと絶対monotonic deadlineを実装します。各bytecode instructionとterminatorは1 fuel unit、実行に入る各host callbackは1 host-call unitを消費します。Validationとarity checkはmetering前に行い、各動的limitは対応するside effectの前に確認します。VMはregister fileを割り当てる前に、検証済みprogramのregister countをvalue-slot limitと比較します。すでにsetされたinterruptionまたは期限切れdeadlineはそのallocation前にexecutionを停止します。以降のsafe pointは各instruction／terminatorの開始時と、実行に入るcallback直前のboundaryです。Structured termination errorは観測したsafe pointでのinstruction／host-call consumptionを報告し、両conditionが成立する場合はinterruptionを優先します。Callbackはflagをsetしたりdeadlineを超過したりできますが、その内部処理は完了し、次のVM safe pointでconditionを観測します。Limit付きAPIはvalue、両方の消費count、静的なslot footprintを返し、既存のconvenience APIはunbounded、non-interruptible、deadlineなしのままです。非停止back-edge test、有限loop-carried countdown test、zero-budget callback test、過小なslot-limit test、pre-execution／callback-triggered interruption test、pre-execution／running-loop deadline testにより、loop停止、callback抑止、allocation前の拒否、safe-point accountingを確認しています。Slotはbackendが定義するVM register entryでありbyte countではなく、host callbackのallocationを計上しません。これらのcontrolはheap、recursion、task、callback内部のpreemption、native／JIT executionには未対応であるため、完全なsandboxではなくembedding interfaceの証拠です。

初期の2つの型付き `i64` callbackがhost所有allocationを検証します。1つ目は1回の `Box` allocate／read／drop round tripを行います。2つ目は異なるwrapping valueを持つ64個のboxを作成し、512 payload byteすべてを同時に保持して合計した後にdropします。VM／JITはRust実装へdispatchし、link済みAOT objectは対応するCの `malloc`／read／`free` 実装をimportします。macOSのdirect／response-file integrationは両wrapperを実行します。Benchmark schema 8はsingle-allocation callbackのdirect Rust／VM／JIT latencyを計測し、schema 11はburst用の同じ3 rowを追加します。Callback allocation workはprocess creation／loadingに埋もれるため、process-startup rowは変更しません。これにより、3つすべてのexecution pathがboundedなallocation-backed operationのために型付きboundaryを越えられることを確認し、より単純でないVM／JIT boundary-cost workloadを得られます。これはlanguage heap、allocator ABI、ownership rule、garbage collector、memory limiter、allocator-overhead account、代表的application workloadではないため、それらのdesign／costに関する論点は未解決です。

Benchmark schema 9は `vm_countdown_interruptible_call` を追加します。`vm_countdown_call` と同じ検証済みcountdown program／input sequenceを、unsetの共有interruption flag付きで実行します。2つのrowの比較は、requestがないときにVM safe-point checkを有効化するincremental acquire-load costを分離します。Interruption latencyやnative／JIT enforcementは計測しません。Clean schema 14 macOS AArch64 resultでは、通常／unset interruption付きVM countdown medianが1.611 us／1.641 usでした。29.868 ns（1.854%）の増加はinterruption rowの15.866% RSDより小さいため、このsampleは測定可能なregressionを示さず、overhead budgetも確立しません。Windows CI contractは構成済みですが、remote Windows resultはまだ記録していません。

Benchmark schema 10は `vm_countdown_deadline_call` を追加し、harness準備の1時間後に設定した絶対monotonic deadline付きで同じbytecode／inputを実行します。Controlなしrowとの比較は、deadlineが期限前のままのsteady-state clock-check costを分離します。Expiration latency、scheduler accuracy、callback preemption、native／JIT enforcementは計測しません。Clean schema 14 macOS AArch64 resultでは、通常／期限前deadline付きVM countdown medianが1.611 us／3.320 usで、1.709 us、106.071%増加しました。Windows CIはこのrowを要求しますが、remote Windows resultはまだ記録していません。

Benchmark schema 11はschema 10 fieldをすべて維持しながら、64-allocation burst用のdirect Rust／VM／JIT rowを追加します。過去のclean macOS AArch64 resultは44行すべてが一意であることを検証し、burst medianとして782.624 ns／962.329 ns／786.270 nsを記録します。Windows CIはburst 3 rowすべてを維持し、macOS／Windows AOT integrationは対応するimport callbackを実行しますが、remote Windows benchmark resultはまだ記録していません。

Benchmark schema 12は44個のtiming rowを維持し、scopeが `host_callback_requested_payload_bytes` の独立した `heap_measurements` sectionを追加します。安全なmeasuring heapがtimed loopの外側で同じgeneric single／burst allocation workloadを実行し、allocation count、要求payload byte合計、同時にliveだったpayload byteのpeak、完了後のlive payload byteを記録します。TestとWindows CIはsingle workloadに `(1, 8, 8, 0)`、burstに `(64, 512, 512, 0)` を要求します。これはdirect／VM／JIT callback pathで共有される決定的なlogical-payload evidenceであり、allocator metadata、fragmentation、stack、generated code、VM／JIT内部allocation、process RSSではありません。過去のclean macOS AArch64 schema 12 resultは両profileと44個すべての一意なtiming rowを記録しました。Schema 13はそのcontractを維持し、remote Windows evidenceは未完了です。

Benchmark schema 13はschema 12のtiming、artifact、heap fieldをすべて維持し、`process_memory_measurement_scope`、`process_memory_metric`、`process_memory_measurements` を追加します。Timing完了後、harnessはallocationを行わないlink済みAOT executableをsampleごとに1回起動し、OSのpeak process-resident metricをbytesで要約します。macOSは `/usr/bin/time -l` 経由で `ru_maxrss`、WindowsはWindows PowerShell経由で `System.Diagnostics.Process.PeakWorkingSet64` を読み取ります。両metricにはprivate／shared resident pageとloader／runtime stateが含まれるため、値はallocator-overheadや代表的applicationのmeasurementではなくplatform固有のprocess envelopeです。Unit testはdistribution renderingとmacOS parserを検証し、native smoke runはmacOS pathを検証し、Windows CIは正で順序付けされた31-sample distributionを要求します。過去のclean schema 13 macOS AArch64 baselineは31 sampleすべてが1,671,168 byte、RSD 0.000%であることを記録し、remote Windows evidenceは未記録です。

Benchmark schema 14はschema 13の全fieldを維持し、allocation-churn callbackのdirect Rust／VM／JIT timing rowと3つ目の正確なrequested-payload heap profileを追加します。このcallbackは64個のboxed `i64` valueをallocateし、1つおきのallocationをconsumeし、その32個の穴をreplacementで埋めてから、最後の64個をconsumeします。決定的profileは `(96 allocations, 768 total bytes, 512 peak live bytes, 0 live bytes afterward)` です。VM／JIT差分testはinteger extremeを含むwrapping resultを扱い、macOS AArch64 direct／response-file integrationは対応するC `malloc`／`free` importを実行し、host非依存Windows COFF validationは `lang_host_i64_3` を検証し、cfg-gated MSVC integrationとWindows workflowは同じcontractを強制します。Cleanな31-sample macOS AArch64 runは47個すべての一意なtiming row、3つすべての正確なheap profile、direct Rust／VM／JIT churn medianの1.192 us／1.348 us／1.173 us、1,654,784〜1,671,168 byteのlink済みprocess peak-memory distributionを記録します。これはvariable-sizeまたは代表的application workloadを主張せずにlifetime-interleaving evidenceを追加します。Allocator metadata／fragmentationとremote Windows executionは未完了です。

Benchmark schema 16はschema 15の全fieldを維持し、4つ目のallocation-boundary callbackのdirect Rust／VM／JIT timing rowと正確なrequested-payload heap profileを追加します。このcallbackはpayloadが8〜128 byteへ増える16個のboxed `i64` sliceをallocateし、要求した1,088 payload byteすべてを同時に保持して、各allocationに保存した1 valueをfoldします。VM／JIT差分testはinteger extremeを含むwrapping resultを扱い、macOS AArch64 direct／response-file integrationは対応するC `malloc`／`free` importを実行し、host非依存Windows COFF validationは `lang_host_i64_4` を検証し、cfg-gated MSVC integrationとWindows workflowも同じcontractを強制します。Cleanな31-sample macOS AArch64 resultはdirect Rust／VM／JIT medianの215.438 ns／397.544 ns／218.453 nsと、正確なprofile `(16 allocations, 1,088 total bytes, 1,088 peak live bytes, 0 live bytes afterward)` を記録します。これはsource-language allocation semantics、allocator metadata／fragmentation、代表的workloadを主張せずにvariable-size boundaryのcostとlogical-payload evidenceを確立します。Remote Windows executionは未完了です。

Benchmark schema 17はschema 16を維持し、受け入れ済みsource semanticsを導入せずにallocator size classとlifetime fragmentationを実行する、別processのcompiler-like allocation traceを追加します。24〜4,096 byteの12 size classにまたがる16,384個のretained payloadをallocateし、3個ごとに1個を解放して異sizeのreplacementで穴を埋め、retained graphと2,048〜5,632 byteの2,048個のtransient bufferを重ねます。全payload byteをtouchします。Harnessは正確なrequested-payload accountingと、同じexecutableのinterleaved control／workload peak-resident sampleを記録します。Median workload-minus-control deltaからpeak live requested payloadを引いたsaturating valueをallocator-overhead／fragmentation合算推定値として報告します。CleanなmacOS AArch64 runは23,894 allocations、19,183,210 total requested byte、18,102,042 peak live requested byte、21,037,056 byteのmedian resident delta、2,935,014 byteの推定値を記録します。Page rounding、allocator retention、loader state、OS working-set policyは分離できず、この推定値は方向的かつplatform固有であり、memory budgetやallocator選定ではありません。

Benchmark schema 18はschema 17の全fieldを維持し、追加の代表的CFG workloadとしてiterative Fibonacciを追加します。このloopはsigned countdownと2個のwrapping sequence valueを3個の型付きblock argumentで運び、back-edgeでsequence valueを入れ替え、非正inputにはzeroを返します。差分testはwrapping overflowを含むVM／JIT equivalenceを扱い、macOS AArch64 direct／response-file integrationはAOT objectを実行し、host非依存Windows COFF validationはそのexportを検証し、cfg-gated MSVC integrationとWindows workflowも同じcontractを維持します。Cleanな31-sample macOS AArch64 runは56個の一意なtiming rowを記録し、direct Rust／VM／JIT medianは6.510 ns／1.231 us／7.463 ns、生成artifactは48 byteのJIT codeと400-byte objectです。これは新しいMIR operationを追加せず、受け入れ済みsource-level semanticsを主張せずにloop-carried CFG evidenceを拡張します。Remote Windows executionは未完了です。

### Plugin trust tier

1. **Trusted native:** AOT object／static library、specialized call、最大性能、sandboxの保証なし。
2. **Verified script:** Capabilityとresource limitを持つbytecode VM。
3. **Portable sandboxed component:** 絶対的なcall overheadよりportabilityとisolationが重要な場合のWebAssembly Component Model／WIT-compatible boundary。

User-authored pluginは既定でtier 2または3を使用します。Native pluginは明示的なtrust decisionです。

## Component ABIと複数言語

Native source-language ABIをcompatibility promiseとして使用しません。WITに着想を得た独立interface languageを定義します。

- Record、variant、enum、flag、resource／handle、list、string、option、result、future、stream、function。
- 明示的なownership transferとborrowed／owned parameter。
- Package／interface semantic versionとcompatibility check。
- Capability importとexported world。
- Native code、VM bytecode、C、WebAssembly向けgenerated adapter。

Whole-program build内では、compilerがcompatible layoutとownershipを証明した後、adapterをdirect callへspecializeできます。独立してversion管理されるcomponentではcanonical boundaryを維持します。

将来のfrontendは最初に固有HIRへloweringし、coreが意味論を忠実に表現できる場合だけshared typed MIRへloweringします。安全に表現できないfrontend固有機能は、core MIRを無制限に拡張せず、component interfaceを通じてloweringします。

## Native backend

### 初期target

- Tier 1 prototype：macOS AArch64とWindows x86-64。
- 次のtarget：macOS x86-64とWindows AArch64。
- Object format：Mach-OとCOFF／PE。
- Custom ABIより先にC ABI interoperabilityを実装する。

### Backend戦略

- MIR lowering、calling convention、高速なiterationを検証するため、Cranelift object／JIT prototypeから開始する。
- Backend APIを狭く、target-neutralに保つ。
- 代表的programでrelease runtime、compile latency、code size、debug品質、platform integrationを計測する。
- Craneliftがrelease performance、optimization、debug要件を満たせない場合にLLVM backendを追加する。
- 最初はsystem linkerを使用し、reproducibilityとinstallation dataが正当化した場合のみbundled LLDを評価する。

現在のprototypeは、同じ検証済みMIR loweringからin-process JIT codeまたはobject artifactを生成します。Object artifactは、Craneliftまたはobject-libraryの型をcallerへ公開せず、byte、exportされたfunction symbol、生成function-code sizeを提供します。Object emissionはnative host ISAを使用することも、明示的なtarget tripleからISAを構築することもできます。Host非依存testは `x86_64-pc-windows-msvc` outputをx86-64 COFFとしてparseし、exportされたMIR functionと未定義のhost-callback importを検証します。これはcross-target object構築を証明しますが、Windowsでのlinkまたはexecutionを証明するものではありません。`Branch`、`Jump`、`Return` terminatorを持つ型付きbasic blockが、MIR、検証済みbytecode、VM、Craneliftで共有する最初の明示的CFGを提供します。型付きblock parameterとedge argumentは、専用phi instructionなしでphiに相当するmergeを表現します。Validationでは、すべてのtargetが存在すること、すべてのblockがreachableであること、entry blockにpredecessorもblock parameterもないこと、edge argumentのarityと型がtarget parameterに一致すること、各SSA definitionがuseをdominateすることを要求します。Entry block以外へのback-edgeは有効です。Countdown workloadはback-edgeを介して `i64` を運び、VMとJITの実行結果が一致し、object pathも同じfunctionをemitします。VMは動的に実行した各instructionとterminatorを課金するため、fuelは非停止loopをdeterministicに停止します。Root crateは、target-neutralで順序を保持する `LinkRequest` と初期host `SystemLinker` adapterを所有します。Requestはobject、library search path、logical libraryの順序を保持し、空値を拒否します。Adapterは `Cc` と `MsvcLink` のcommand flavorをmodel化し、search pathとlibraryを `-L`／`-l` または `/LIBPATH:`／`.lib` にloweringし、対応するhost driverとoutput syntaxを選択し、明示的なdriver、flavor、ordered prefix argumentも許可します。完全なcommandを公開して、structuredなspawn、process-failure、missing-output diagnosticへそのcommandを保持します。明示的なresponse-file linkは、lowering済み `Cc` argumentをGNU-escaped UTF-8、`MsvcLink` argumentをWindows-escaped BOM-prefixed UTF-16LEとしてserializeします。Validationは、unsafeなoutput／object path conflict、non-Unicode argument、NUL、newlineをwrite前に拒否します。macOS AArch64 integration coverageはdirect linkとresponse-file linkの両方を実行します。cfg-gated Windows x86-64 MSVC testは、`cl.exe` と `link.exe` を使って両方のobject-link-execute pathを再現します。Windows Server 2025 workflowはx64 developer environmentを構成し、read-only permissionと完全SHAでpinしたactionを使って全baselineを実行します。このtestは現在のmacOS baselineでは実行されず、このcommitにはremote workflow結果が記録されていません。Aggregate value、allocation、source-level control-flow semantics、toolchain discoveryやより広いtarget別policyなどのproduction linker hardening、startup code、stable ABI adapter、debug metadata、記録済みWindows link／execution結果、Windows benchmarkは個別の作業として残っています。

CleanなmacOS AArch64の[debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)では、commit `013da31f9f0fb752aad5ec887f8cf5a5ecf1650d` 時点のadd、minimum、host-callback Mach-O objectにexternal function／import symbolがある一方、`__debug_*` section、DWARF compile unit、line table、source variable、parameter、localはいずれも見つかりませんでした。そのinspection以降、typed MIRは構造を検証したsource file、function span、各instruction／terminatorに1つずつのsource spanを保持できるようになりました。Cranelift loweringは各operationの前にopaqueなsource locationを設定し、生成されたmachine-code offset mapをJIT／AOT artifactのbackend-neutralなsource rangeへ戻します。Mach-O serializationはこれらのrangeをrelocation付きDWARF v4 compile unit、subprogram entry、line tableへ変換します。代表的なadd、minimum、host-callback MIR fixtureが通常のobject CLIとmacOS AArch64 integration pathを駆動し、testは3 objectすべてを `dwarfdump` で検証し、direct／response-file linkを実行し、両dSYMを作成してlink後のdebug informationを検証します。Cleanなcommit済みinspectionの記録は残っています。Unwind metadata、Windows CodeView／PDB、JIT debugger registration、optimized stepping、将来のsource-language frontendは個別のacceptance workであり、backend選定ではそれらのcost／qualityを考慮する必要があります。

Host linker discoveryは、deterministicなcandidate順で `PATH` を探索し、Unixではexecutable fileを要求し、driverがない場合は探索したpathとcandidate nameを含むstructured diagnosticを返します。macOS integrationは発見したdriverを使用し、cfg-gated MSVC integrationもdeveloper environmentが `PATH` を初期化した後に同じdiscoveryを検証します。そのenvironment外にあるVisual Studio、SDK、compilerのinstallation discoveryは個別の作業として残っています。

Root crateは、初期tier-1 linker policyをbackend-neutralな `LinkTarget` domain typeとして表現します。Exactな `aarch64-apple-darwin` と `x86_64-pc-windows-msvc` tripleがcommand flavor、default program、ordered `PATH` candidateを決定し、未supportのtripleはhost policyを暗黙に継承せずstructured errorになります。明示targetのlinker構築、discovery、platform integration、cross-target COFF testがこのmappingを共有します。別targetのsupportには明示的なpolicy追加が必要です。Installation discoveryとtarget固有のstartup／runtime inputは別layerのままです。

Schema 3 benchmarkは、両方の初期tier-1 hostでsystem-link measurementにこのpolicyを使用します。計測対象MIRへ、back-edgeを介して `i64` block parameterを運ぶbounded countdown loopを追加し、platform C harnessを事前compileし、link済みexecutableを1回検証してから、compiler setupとlinker discoveryを含めずに `cc` または `link.exe` の反復呼び出しを計測します。Windows CIはmachine-readable envelopeを検証して期限付きartifactとしてuploadします。Benchmark valueがproject evidenceになるのは、cleanなrunner結果を検査して記録した後だけです。

Schema 4は同じbackend-neutral link pathを維持し、link済みexecutableのbyte sizeと反復process-startup rowを追加します。Startup measurementは、1回のvalidation run後にoperating-system cacheをflushせず、process creation、platform loader処理、小さなAOT harness、output collection、terminationを含めます。これにより、測定したwarm-cache process costをcold boot、loader-only cost、完全なruntime distribution sizeから区別します。CleanなmacOS AArch64 resultを記録しましたが、49.824 msの外れ値と276.388%のstartup RSDがあるため、budget設定前にmethodologyを改善する必要があり、Windows evidenceも未完了です。

Schema 5はそのsingle-launch rowを維持し、設定可能なbatched startup rowを追加します。各sampleは要求数のlaunchを順番に実行してlaunch当たりの経過時間を報告し、single-launch rowをtail-sensitiveなevidenceとして維持しながら、1回の遅延したlaunchがsampleへ与える影響を小さくします。2つのrowは意味的に別であり、batchingはamortized process-launch costを推定しますが、individual startup latencyを置き換えません。CleanなmacOS AArch64 resultでは、1.487 msと1.499 msという近いmedianを維持しながら、startup RSDがsingle launchの256.272%から10-launch batchの4.688%へ下がりました。Windows comparisonは未完了です。

より広いCFG prototypeは、descending counterとwrapping accumulatorという2つの `i64` valueを、同じback-edgeのblock parameterで運びます。Conditional-sum variantはloop内でbranchし、current valueをaddまたはskipして、両pathをblock parameterでmergeした後に共有back-edgeを取ります。MIR validation、bytecode execution、Cranelift JITはnon-positive input、両conditional path、bounded positive loopで一致します。Native object emissionはmacOS direct／response-file link-and-run integrationとhost非依存Windows x86-64 COFF symbol validationで検証し、cfg-gated MSVC integrationも同じexecution pathを反映します。これはMIR evidenceにとどまり、source-level loop semanticsとcleanなconditional-workload benchmark evidenceは未完了です。

Schema 6は、accumulator workloadをbytecode lowering、JIT／AOT compilation、direct／VM／JIT call、生成code／object size、tier-1 system-link／startup setへ追加します。Link setがadd、minimum、countdown、accumulator objectを含むようになったため、platform seriesとlink済みexecutable fieldをrenameします。CleanなmacOS AArch64 resultは新しいrowとfieldを記録し、Windows CIも同じcontractを検証しますが、remote Windows comparisonは未完了です。

Schema 7はconditional-sum workloadを同じdimensionへ追加し、その5番目のobjectをtier-1 system-link／startup setへ加えます。拡張されたsetに合わせてplatform seriesとlink済みexecutable fieldを再度renameし、Windows CIはschema 7のconditional-loop／platform contractを検証します。CleanなmacOS AArch64 resultは全36 rowと17,104-byteのlink済みharnessを記録しました。Remote Windows comparisonは未完了です。

## Diagnostics、LSP、debugging

Diagnosticsは、stable code、primary／secondary span、note、fix-it、machine-readable fieldを持つstructured valueです。

LSPはcompilerのparser、resolver、type checker、source map、incremental query databaseを再利用します。不完全またはinvalidなprogramでもresponsiveである必要があります。

Debug stackは3つのlayerを持ちます。

- Native debug metadata：macOSではDWARF、WindowsではCodeView／PDB。
- VM debug metadata：bytecode／source map、lexical scope、variable location、async task frame。
- Native frameとVM frameを統合し、mixed breakpointをsupportし、optimized／unavailable valueを正直に表示するDAP adapter。

Source spanとstable symbol／function identifierをすべてのlowering stageで維持します。Debuggabilityをbackend acceptance criteriaに含めます。

## Toolchain UX

名称が決まるまでは、仮の統合commandを1つ使用します。

```text
lang new       create a package
lang build     build incrementally
lang run       build and run, or execute a script
lang test      run unit, integration, and documentation tests
lang fmt       canonical formatting
lang lint      semantic and style diagnostics
lang doc       generate API documentation
lang lsp       start the language server
lang debug     start or connect the DAP adapter
lang package   manage dependencies and publication
lang toolchain install|list|default
```

Public registryより前に、package managerはmanifest、exact lockfile、checksum、offline mode、source replacement／mirror、feature resolution、target condition、reproducible build metadataを必要とします。

## セルフホスティング

1. Stage 0 Rust compilerが承認済みcore semantics全体を実装する。
2. 意図的に小さなself-hosting subsetを新言語で実装する。
3. Stage 0でstage 1をbuildし、stage 1でstage 2をbuildする。
4. Reproducible settingでstage 1とstage 2のoutputを比較する。
5. 複数platformでconformance／reproducibility checkに合格するまで、Rust compilerをbootstrap referenceとして残す。

Rust compilerとself-hosted compilerで意味論をforkさせません。両方が同じconformance suite、serialized test case、component ABI fixtureを使用します。

## 提案するRust workspace境界

実装が現れた段階でのみ、次のcrateを作成します。

```text
crates/
  syntax        lossless tree, parser, source map
  diagnostics   stable diagnostic model
  hir           resolution and typed high-level IR
  semantics     inference, traits, effects, ownership
  mir           typed mid-level IR and verification
  bytecode      format, encoder, decoder, verifier
  vm            embedded runtime and interpreter
  codegen       backend-neutral lowering interface
  codegen-clif  Cranelift backend
  runtime       native runtime services
  component     IDL, canonical ABI, bindings
  lsp           language server
  dap           debug adapter
  cli           toolchain command
```

Cyclic crate dependencyを避けます。Data-oriented IR crateはCLI、LSP、VM、backend crateへ依存してはいけません。

## 必須検証

- Parser／formatter round tripとmalformed-input recovery。
- Stable error codeを持つcompile-pass／compile-fail diagnostics。
- MIR／bytecode verifier test。
- Interpreter／native differential test。
- Cross-version ABI／package compatibility fixture。
- Parser、manifest、bytecode、debug data、component metadataへのfuzzing。
- 実用的な範囲でRust実装へのMiri／sanitizer run。
- Windows／macOS CIと代表的benchmark。
- Self-hostingを主張する前のbootstrap stage比較。
