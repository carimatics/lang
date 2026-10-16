<!-- translation-of: ../architecture.md; canonical-language: en -->

# アーキテクチャbaseline

[English](../architecture.md)

ステータス：RFC 0001–0005による**受理済み設計baseline**（2026-10-17）。実装は未完成のprototypeです。受理は安定化を意味しません。意味論はRFCに従い、以下の広範なプロダクト方針は実装前に追加RFCを必要とします。

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

受理済み[RFC 0002](rfcs/0002-prototype-control-flow.md)は、最初のsource subsetとして `i64` parameter／result、wrapping addition、signed comparison、mutable local、`if`／`else`、`while`、明示的 `return`、左から右の評価順序を定義します。Join／back-edgeをまたぐlive valueはtyped block argumentへloweringします。受理済み[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)は、字句規則とmodule／call、aggregate／pattern、portable bytecode、script／REPLの拡張を定義します。Diagnostics、lossless tokenization、構造化inputのprimitive HIR analysis、primitive HIR-to-MIR loweringは実装済みですが、text parsing、parser統合、formatterは未実装です。

2026-10-17実装：`lang-syntax` はUTF-8 inputを検証し、不正encodingや先頭BOMにはsnapshotを作成しません。Input errorはraw byte範囲を保持します。Lexerは受け入れた各snapshotを、空白、comment、不正token、不正Unicode scalarも含む、連続した完全なtoken列として保持します。符号付きdecimalのoverflow、不正token、comment内も含むNUL、閉じていないblock comment、直接作成したsnapshotのBOM違反を報告します。Source byte、token、nested-comment depth、診断数の上限を超えると `SRC0011` でanalysisを終了します。Commentは反復処理で走査します。13件のtestが正確な範囲、復元、決定的な混合Unicode input、resource境界を検証します。Lossless treeとparser recoveryは別の残作業です。

2026-10-17実装：`lang-hir` はsyntax非依存のRFC 0002 function arenaを受け取り、local nameを解決し、analysisにerrorがない場合だけread-onlyなtyped HIRを返します。Parameterはimmutableで、宣言はinitializerの後からlexical block内だけで可視です。別branchを含むfunction全体での名前再利用を拒否します。Type checkingはinteger operand／value、boolean condition、到達可能な全exitの明示returnを強制します。`SRC0002`–`SRC0007` をprimary／related span付きで生成し、signed comparisonにも `i64` operand checkを適用します。到達不能statementもanalysisし、structured warningを返します。Warningのstable source codeは字句／tooling拡張の決定を待ちます。全nodeにsource-snapshot byte span、localには表示名／宣言spanを保持します。Caller指定のwork／local／depth／diagnostic limit、内部再帰上限、arena参照検査、cycle検出、source／name-span検証でanalysis boundaryを保護します。この段階ではliteral decode／range診断、字句規則、module loading、source call、aggregate、serialized HIR契約を導入しません。

同じく2026-10-17実装：`lang-semantics::lowering` はこのHIRを既存typed scalar MIRへ変換し、明示work／value／block limitと最終MIR validationを適用します。Expressionを左から右へloweringし、変化したbranch valueをtyped block argumentでjoinし、可視mutable valueをloop-header parameterで運びます。Early returnで到達不能joinを作らず、到達不能statementからinstructionを生成しません。CFG構築後にsymbolic IDをMIRが要求する連続definition順へremapします。合成jumpを含む全生成instruction／terminatorで、snapshot byte spanを既存の1始まりMIR line／Unicode-scalar columnへ変換します。構造化inputのintegration testはVM／JIT arithmetic、両branch path、nested branch／loop state、early return、fuel超過、bytecode span保持、native JIT／AOT-object source mappingを扱います。Parser接続済みsource loweringと新call／aggregateは別gateです。Untrusted deployment前にはprototype MIR validator自体のwork／allocation hardeningも必要です。

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

2026-10-17実装：`lang-query` はbackend非依存のin-memory graph／cache coreです。Hostはopaque key、immutableな `Arc` value、完全な明示dependency集合を渡します。Input editは推移的consumerをinvalidateし、無関係なcacheを保持します。Query publication ticketはgraph identity、query generation、dependency revisionを記録し、edit、新しいwork、publication、removal、key再利用後に古い結果で現在のvalueを置換させません。Dependency置換で古いreverse edgeを残しません。Key closeはconsumerをinvalidateし、incoming referenceを上限付きtombstoneとして保持するためreopenでも古いworkを復活させません。Cycle、node／edge／work limit、revision overflowは全bookkeepingをstate変更前に検証します。Traversalは反復的かつdeterministicです。上限対象はgraph bookkeepingであり、hostのanalysis-value memoryやcodeではありません。構造化input HIRのintegration testで診断cache再利用と古い結果の拒否を示します。Query recipeの自動実行、全input／dependency検出、hash／timing永続化、parser／module統合、LSP adapter／transportは未実装であり、graph coreだけでtooling milestoneの広いtaskを完了しません。

## Memory model baseline

この設計では、memory safety、一般的なshared object graph、単純な構文、deterministic destruction、zero overhead、GCなしをすべて同時に最大化できないという基本的トレードオフを認識する必要があります。

受理済み所有権のcoreはRFC 0004で規定します。以下のparameter passing推論、closure capture構文、unsafe操作、allocator API、freestanding profileは追加検討の方針であり、受理済み規則を増やすものではありません。

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

受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)は、plain valueの単独所有、明示的 `shared`／`weak`、mutation前のstrong shared cycle拒否、意図的なcycleのlexical arena格納を要求します。これはsource／runtime実装の責務であり、実験modelがproduction ownership checkerであるという意味ではありません。

残る実装evidenceにはnested cleanup、partial initialization、self-reference rejection、atomic sharing／cycle preflight／arenaのruntime・memory costがあります。RFC 0005はsuspensionをまたぐすべてのborrowを既に拒否します。その緩和には新しいRFCが必要です。

### Cyclic graph実験

`crates/semantics` のgraph modelでは、2つのshared-cycle policyの下で、明示的なshared／weak ownershipとarena-scoped storageを比較します。

- Shared nodeはexternal rootを1つ持って開始する。Strong edgeはtargetをretainし、weak edgeはretainしない。
- Defaultの `Reject` policyは、shared-reference cycleを閉じるstrong edgeをstructured diagnostic `SEM0018` でrejectする。同じcycleをweak edgeで閉じることは許可し、expired weak edgeのupgradeは `SEM0021` を報告する。
- 実験的な `CollectUnreachable` policyはstrong cycleを許可する。External rootをdropした後、残るすべてのlive rootからstrong edgeをtraceし、到達不能なshared nodeを逆生成順にreleaseする。Weak edgeはtargetをliveに保たない。
- 1つのarena内のnodeは、arenaがlifetimeを所有するため任意のcycleを形成できる。Arena間、またはarena storageとshared storage間のedgeは `SEM0017` を報告する。
- Arenaをdropするとnodeを逆生成順に正確に1回releaseする。その後にstale node handleを使うと `SEM0012` を報告する。
- すべてのoperationがsource spanを持ち、関連するallocationまたはedge spanが存在する場合はdiagnosticに保持する。

RFC 0004は `Reject`、weak back-edge、lexical arenaを受理しています。`CollectUnreachable` は未受理の代替案です。同期graph実験はruntime reference-count実装ではなく、weak upgradeはlivenessを確認しますが新しいretained ownerを返しません。Atomic publication、thread safety、destructor failure、production weak-reference orderingもmodel化していません。これらは専用実装とconformance testが必要です。

レビュー結果：**production実装に向けてneeds prototype/measurement**。受理済みstrict policyの再決定ではありません。Milestone 2でsource checking、shared／weak lifecycle、cycle-preflight atomicity、arena lifetime validationを扱います。

### Async borrow／cancellation実験

`crates/semantics` の実行可能modelはRFC 0005のstrict baselineと、未受理の制限を緩めたpolicyを比較します。

- Task-owned valueはsuspensionを越えてtask frame内でliveのままにする。
- Defaultの `RejectAll` policyは、suspension pointでactiveなshared／exclusive borrowをstructured diagnostic `SEM0007` でrejectし、suspension前に終了したborrowをacceptする。
- 実験的な `AllowSharedFrameBorrows` policyはtask-owned valueのactiveなshared borrowがsuspensionを越えることを許可する一方、exclusive borrowは引き続き `SEM0007` でrejectする。
- Cancellationはtaskがsuspendedである間だけ観測する。
- 実験的policyでは、resume後もshared borrowを維持し、cancellation時はactive borrowを終了してからtask-owned valueを逆生成順に正確に1回dropする。
- すべてのoperationがsource spanを持ち、failureではboundaryとそれを越えたborrowの両方を示す。

RFC 0005は `RejectAll` を受理し、`AllowSharedFrameBorrows` は実験のままです。Modelはsuspension時だけcancellationを観測し、仕様のtask-tree runtimeや冪等なcancellation-request APIではありません。Nested scope、partial initialization、destructor failure、panic containment、同等なVM／native cleanup traceは残る実装作業です。

レビュー結果：**runtime実装に向けてneeds prototype/measurement**。Suspensionをまたぐpinned／shared／exclusive borrowは受理済みbaselineから除外されています。実装者が独自の挙動を選べる未決事項ではありません。

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

`crates/semantics` のservice modelはRFC 0005のowner-isolated baselineと、未受理のshared-mutation policyを比較します。

- Defaultの `OwnerIsolated` policyでは、1つのowner taskだけがservice stateへ直接mutable accessを持つ。Shared handleはmessageをenqueueできますが、別taskからのdirect mutationは `SEM0031` を報告する。
- 実験的な `ExclusiveCapability` policyでは、有効なhandleを持つ任意のtaskがactive mutationのないときにmutationを開始できる。Handleを持たないtaskはrejectし、同時beginはactive mutation spanを伴う既存のstructured mutation-conflict diagnosticを報告する。
- Sendはpayloadをmailboxへmoveする。Non-sendable payloadは `SEM0028`、成功したmove後の再利用は `SEM0027` を報告する。
- Ownerは引き続きmessageをFIFO順にprocessする。有効な任意のhandleからのcancellation requestは、実験的policyでのnon-owner mutationを含め、active mutationが終了するまでdeferする。
- Cancellationはqueued messageをFIFO順にdiscardし、queued payloadとservice stateをそれぞれ正確に1回dropしてから、後続sendを `SEM0024` でrejectする。
- Operationはstable diagnostic code `SEM0022`〜`SEM0035` のもとでprimaryおよび関連source spanを保持する。

RFC 0005は `OwnerIsolated` を受理し、`ExclusiveCapability` は未受理のlock-likeな代替案です。Modelはsend／share導出、task tree、繰り返しの冪等cancellation request、backpressure、reentrancy、panic cleanup、runtime schedulingを備えていません。Model diagnosticsは実験evidenceであり、完全なsource／scheduler contractではありません。

レビュー結果：**runtime実装に向けてneeds prototype/measurement**。Milestone 3では受理済みisolation／cancellationを実装し、未規定のchannel／scheduler／error詳細をsource behaviorとして公開する前に決定します。

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

[RFC 0003](rfcs/0003-versioned-component-contracts.md)は、最初の実行可能なcomponent-contract sliceを規定します。Runtimeは現在、名前付きscalar import／export、保守的identifier、明示的capability requirement、embedderが供給する論理name-to-host-function bindingを持つschema-versioned package metadataをvalidateします。Portable metadataはprocess-localな `HostFunctionId` valueを含みません。Validationは構造的conflictとunsupported import typeを拒否し、undeclared importへのbindingを拒否し、host lookupより前にcapability grantをcheckし、すべてのimportをtyped registryに対してresolveし、resolved local bindingを持つvalidated wrapperを返します。Deterministicな実験的 `LCMP` schema-1 codecはscalar declaration subsetをencodeし、明示的なtotal-byte、declaration-count、structural-depth、string-length limitの下でuntrusted byteをdecodeします。Malformed-input errorはbyte offsetを持ち、libFuzzer targetがdecoderを実行します。このschema-1 codecはaccepted candidate schema-2 serializationではなくunstableなfixtureであり、WIT実装、component-IDL resource declaration、canonical ABI、component exportのexecution pathでもありません。後述するprototypeがaccepted candidate serializationと選定したresource／adapter pathをcoverし、production IDL／generated adapterは後続作業です。

別のcandidate `LCMP` schema-2 envelopeは、contract document（`kind 0`）とversioned failure-report document（`kind 1`）を定義するようになりました。Contractはpackage key／完全なsemantic version、sort済みnormalized interface requirement／required-member projection、SHA-256 tag付きexact lock、sort済みcanonical resource-type identityをdeterministically encodeします。Required memberはparameter／result順序、exact capability、完全なnominal resource key付きの `i64`、`bool`、`own<T>`、`borrow<T>` value typeを保持します。Failure reportは1つのprimary recordと順序付きsuppressed cleanup failureを保持し、poisoned stateをencodeし、固定category／phase／diagnostic tagを使い、message redactionとembedding-sanitized transportを区別します。Policy明示conversionはcleanup invariant trap／destructor-failure順序を保持しながらruntime `BoundaryFailure` recordをprojectします。Constructionはinvalid identifier／capability、duplicate entry／member name、invalid interval、未宣言resource参照、borrowed result、inconsistent failure classification、cleanup以外のsuppressed record、unsafe redaction state、poisonされていないdestructor failure、orphan lock、lock-version mismatchを拒否します。Bounded decoderはwrong schema／document kind、malformed tag／boolean、truncation、invalid UTF-8、trailing byte、byte／item／depth／string limit超過をbyte offset付きで拒否します。Check-in済みgolden encoding、round-trip／canonical-order test、semantic validation test、deterministic arbitrary-byte coverage、専用fuzz targetが両layoutを保護します。Accepted RFC 0003はschema 2をimplementation baselineとして採用しますが、stable public production formatではなくversion管理されたdesign evidenceのままです。

Component runtimeにはin-memoryのgeneration tag付きresource tableもあります。Copy可能なowned handleはuntrusted callerが複製できるinteger handleをmodel化します。Resourceをreleaseすると最小のfree slotを再利用する前にgenerationを進め、generation exhaustionの場合はslotをretireするため、stale copyはreplacementをaliasできません。Shared borrowは共存でき、exclusive borrowにはactive borrowがないことを要求し、mutable accessにはliveなexclusive tokenを要求し、releaseはすべてのlive borrowを拒否します。Borrow tokenはmonotonicで明示的に終了します。Stale token、repeated end、repeated release、conflicting borrow、identifier exhaustionはstructured errorです。これはtable-local lifecycle checkを実証するもので、serialized handle layout、cross-component ownership transfer、destructor behavior、thread safety、canonical resource loweringではありません。

Runtimeはschema-1 serialization／production package-manager resolutionから独立したpackage／interface compatibility negotiationもprototypeします。Normalized semantic versionはprerelease／build identifierを保持し、numeric prerelease identifierをmachine wordへ制限しません。Exact requirementは完全なvalueを比較し、intervalはSemVer precedenceと明示的なprerelease policyを使用します。Capability grantはprovider inventory validationより先にcheckします。Exact lock inputは1つのidentity、version、opaque digestをfallbackなしで要求し、それ以外ではexact keyとrequired-memberのstructural projectionでcandidateをfilterしてからdeterministicにhighest-precedenceを選択します。Projectionは順序付きscalar type、nominalなowned／borrowed resource type、exact capabilityを含み、要求されていないprovider memberを許可します。Same-precedenceのbuild／digest tieはambiguousで、resolution成功時はimmutable bindingを返します。Canonicalなin-memory lockfileはcompleteなexact SHA-256 selectionを記録し、fallbackなしでreplayし、embedded contract lockとreconcileし、選択済みcontract／bindingをfreshなinstance stateへ直接moveします。On-disk lockfile syntax、registry access、artifact download、publication workflowはまだ定義しません。

実行可能なschema-1 scalar fixtureは、validated component metadataを3つすべてのprototype execution pathへ接続します。Capability／logical-name bindingをtyped host registryに対してvalidateしてから、選択した `i64 -> i64` import／export pairをMIRへloweringします。VM／JITはinteger extremeを含む同じfunctionを同等のwrapping resultで実行します。同じvalidated fixtureがhost-native objectをemitし、そのexportとresolved host importをmacOS AArch64のdirect／response-file pathでlink／実行します。これは1つのscalar contractに対する狭いdifferential evidenceであり、general component loader、canonical resource adapter、failure boundary、stable serialization、cross-platform component runtimeではありません。

Backend-neutralなcanonical-resource adapter traceにより、runtime handle layoutを公開せずRFC 0003のownership／failure orderingを実行可能にしました。Instanceごとのboundary indexは非ゼロかつ単調増加で、再利用しません。Call全体のpreflightはすべてのresource argumentとdestination limitをvalidateし、storageを予約してから、allocationもcallbackも行わないcommitでownerをmoveまたはcall-scoped borrowを公開します。Cleanupはnested borrowを内側から閉じ、残るframe ownerをremoveしてからacquisitionと逆順にdestructorを呼び、独立したdestructor failure後も継続し、先行するpanic／trapをprimaryのまま保ち、callback／invariant／destructor failure後にcalleeをpoisonします。共有host-value bridgeはopaque identityをallocateしてgeneration-check済み `ResourceTable<T>` ownerへmapし、canonical publication failure時はidentityを再利用せずvalueをrollbackし、runtime borrow operationをresolveし、cleanupがactual valueをexactly onceでconsumeできるようにします。General instantiationはinstanceを公開する前にcanonical順のschema-2 requirementをすべてnegotiateし、immutable bindingとfreshでboundedなadapter stateを持つinstanceを生成します。Conformance fixtureはVM、JIT、macOS AOTのscalar resultを同じowned transfer、guest-trap cleanup、redacted failure projectionへ通し、value、state、report outcomeが一致します。後続の直線形nominal `own<T>` laneはMIR、bytecode、VM、JIT、tier-1 AOTで動作しますが、汎用resource control flowとreturn ownership adapterは未実装です。Asynchronous cancellationも未完了です。

VM prototypeは、合成可能な3つのopt-in limitであるinstruction fuel、host-call budget、value-slot limitに加えて、optionalな共有one-way interruption flagと絶対monotonic deadlineを実装します。各bytecode instructionとterminatorは1 fuel unit、実行に入る各host callbackは1 host-call unitを消費します。Validationとarity checkはmetering前に行い、各動的limitは対応するside effectの前に確認します。VMはregister fileを割り当てる前に、検証済みprogramのregister countをvalue-slot limitと比較します。すでにsetされたinterruptionまたは期限切れdeadlineはそのallocation前にexecutionを停止します。以降のsafe pointは各instruction／terminatorの開始時と、実行に入るcallback直前のboundaryです。Structured termination errorは観測したsafe pointでのinstruction／host-call consumptionを報告し、両conditionが成立する場合はinterruptionを優先します。Callbackはflagをsetしたりdeadlineを超過したりできますが、その内部処理は完了し、次のVM safe pointでconditionを観測します。Limit付きAPIはvalue、両方の消費count、静的なslot footprintを返し、既存のconvenience APIはunbounded、non-interruptible、deadlineなしのままです。非停止back-edge test、有限loop-carried countdown test、zero-budget callback test、過小なslot-limit test、pre-execution／callback-triggered interruption test、pre-execution／running-loop deadline testにより、loop停止、callback抑止、allocation前の拒否、safe-point accountingを確認しています。Slotはbackendが定義するVM register entryでありbyte countではなく、host callbackのallocationを計上しません。これらのcontrolはheap、recursion、task、callback内部のpreemption、native／JIT executionには未対応であるため、完全なsandboxではなくembedding interfaceの証拠です。

初期の2つの型付き `i64` callbackがhost所有allocationを検証します。1つ目は1回の `Box` allocate／read／drop round tripを行います。2つ目は異なるwrapping valueを持つ64個のboxを作成し、512 payload byteすべてを同時に保持して合計した後にdropします。VM／JITはRust実装へdispatchし、link済みAOT objectは対応するCの `malloc`／read／`free` 実装をimportします。macOSのdirect／response-file integrationは両wrapperを実行します。Benchmark schema 8はsingle-allocation callbackのdirect Rust／VM／JIT latencyを計測し、schema 11はburst用の同じ3 rowを追加します。Callback allocation workはprocess creation／loadingに埋もれるため、process-startup rowは変更しません。これにより、3つすべてのexecution pathがboundedなallocation-backed operationのために型付きboundaryを越えられることを確認し、より単純でないVM／JIT boundary-cost workloadを得られます。これはlanguage heap、allocator ABI、ownership rule、garbage collector、memory limiter、allocator-overhead account、代表的application workloadではないため、それらのdesign／costに関する論点は未解決です。

Schemaごとの過去のtiming、allocation、process-memory結果は、現在のarchitecture要件ではなく[benchmark evidence](benchmarks.md)へ集約します。Native Windows evidenceはschema 20とrun 38932964398に記録済みです。これらは2026-10-17のJIT memory回収修正前の記録であり、現在のcompile／application性能を主張する前に新しいclean計測が必要です。

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

- 最初の実装のinitial native JIT／AOT backendとしてCraneliftを使用する。
- Backend APIを狭く、target-neutralに保つ。
- 代表的programでrelease runtime、compile latency、code size、debug品質、platform integrationを計測する。
- Craneliftがrelease performance、optimization、debug要件を満たせない場合にLLVM backendを追加する。
- 最初はsystem linkerを使用し、reproducibilityとinstallation dataが正当化した場合のみbundled LLDを評価する。

**Decision（2026-10-17）：** Workspace lockfileでresolveされるCranelift 0.136.3を、JIT／AOT両方のinitial native backendとします。これはimplementation baselineであり、stable native ABI、恒久的に排他的なbackend、language-level performance guaranteeではありません。Frontend、typed MIR、source map、linker request、artifact interfaceはbackend-neutralのまま維持し、代表的release workloadで未達のoptimization、target、debug要件が明らかになった場合のみLLVM backendを後から追加します。

| Criterion | 記録済みevidence | Disposition |
| --- | --- | --- |
| Semantic／ABI integration | Differential VM／JIT coverageと、integer、CFG、callback、scalar component、1つのtyped-owner laneに対するmacOS／Windows direct／response-file AOT execution | Initial backendとして十分 |
| Tier-1 portability | Target-neutral linker policy／明示tripleを使うnative macOS AArch64／Windows x86-64 object-link-run CI | Prototype Tier-1両targetについて十分 |
| Debug quality | 代表artifactに対するsource-mapped Mach-O DWARF／dSYMとCOFF CodeView／PDB inspection | 最初の実装として十分。Source-language local、unwind metadata、optimized stepping、JIT debugger registrationは後続criterion |
| Performance／memory evidence | 過去のclean schema-20 macOS／Windows baseline | 方向的evidenceのみ。収録後にJIT memory回収を修正したため、現在のcompile／application性能の主張には新しいclean計測が必要 |
| Architecture cost | 既存 `codegen-clif` crateはfrontend APIへCranelift typeを公開せず、JIT／object emissionをすでに提供 | 現時点で第2backendを導入するより実装／保守riskが低い |

現在のbackendは、Cranelift／object-library typeをcallerへ公開せず、同じvalidated MIR loweringからin-process JIT codeまたはobject artifactを生成します。Typed block、明示terminator、block parameter、source span、import、exportはVM、JIT、AOT path全体で同じbackend-neutral contractを共有します。共通 `SystemLinker` は `Cc`／`MsvcLink` をmodel化し、順序付きrequestをvalidateし、direct／response-file invocationをsupportしてstructured failureを記録します。macOS integrationと[Windows run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398)は両link modeをnative実行し、cleanな[schema-20 benchmark evidence](benchmarks.md)はTier-1両platformを記録します。Production installation discovery、startup／runtime packaging、stable ABI adapter、追加targetは別作業のままです。

実装前のcleanなmacOS AArch64 [debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)は、元のnegative baselineを確立しました。Commit済みの[source-mapped DWARF inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md)は、typed MIRが構造を検証したsource file、function span、各instruction／terminatorに1つずつのsource spanを保持し、Craneliftがそれらをbackend-neutralなJIT／AOT machine-code rangeとして維持し、Mach-O serializationがrelocation付きDWARF v4 compile unit、subprogram entry、line tableを出力することを実証します。代表的なadd、minimum、host-callback objectは `dwarfdump --verify` をpassし、direct／response-file executableはlink／実行され、生成した両dSYMもfixture pathと3 functionを保持したままverificationをpassします。Commit済みの[Windows x86-64 CodeView／PDB inspection](../../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md)は、同等のC13 symbol、type、line table、relocationをCOFFでvalidateし、LLDでPE／PDB pairへlinkし、LLDBでfunction-to-source resolutionを検証します。これはnative Windows runではなくcross-target debug-format evidenceです。Source-language local、unwind metadata、JIT debugger registration、optimized stepping、将来のsource-language frontendは個別のacceptance workであり、backend選定ではそれらのcost／qualityを考慮する必要があります。

Host linker discoveryは、deterministicなcandidate順で `PATH` を探索し、Unixではexecutable fileを要求し、driverがない場合は探索したpathとcandidate nameを含むstructured diagnosticを返します。macOS integrationは発見したdriverを使用し、cfg-gated MSVC integrationもdeveloper environmentが `PATH` を初期化した後に同じdiscoveryを検証します。そのenvironment外にあるVisual Studio、SDK、compilerのinstallation discoveryは個別の作業として残っています。

Root crateは、初期tier-1 linker policyをbackend-neutralな `LinkTarget` domain typeとして表現します。Exactな `aarch64-apple-darwin` と `x86_64-pc-windows-msvc` tripleがcommand flavor、default program、ordered `PATH` candidateを決定し、未supportのtripleはhost policyを暗黙に継承せずstructured errorになります。明示targetのlinker構築、discovery、platform integration、cross-target COFF testがこのmappingを共有します。別targetのsupportには明示的なpolicy追加が必要です。Installation discoveryとtarget固有のstartup／runtime inputは別layerのままです。

過去のbackend実験と当時のevidence不足は[benchmark evidence](benchmarks.md)に記録します。受理済みsource規則や完了済みWindows baselineを未決定へ戻すものではありません。

## Diagnostics、LSP、debugging

Diagnosticsは、stable code、primary／secondary span、note、fix-it、machine-readable fieldを持つstructured valueです。

2026-10-17実装：`lang-diagnostics` はimmutableなUTF-8 snapshot、全mapを通じて異なるsnapshot ID、文字境界を検証する半開byte span、1始まりのUnicode scalar位置、IDE adapter向け0始まりのUTF-16 columnを管理します。Diagnosticはprimary labelを1つ必須とし、別fileのsecondary label、note、machine-applicable／speculativeを区別したfixを追加できます。Fix previewは全参照snapshotを検証し、重複や曖昧なeditを拒否してから変更後のtextを返し、fileは変更しません。Plain-text表示はdeterministicで、制御文字とdirectional-formatting文字をescapeします。RFC 0002の `SRC0001`–`SRC0007` を表現し、構造化HIRは `SRC0002`–`SRC0007` を生成します。Literal／parser側の診断生成とtext接続済みdiagnostic conformanceは未実装です。このcrateはsource grammarやserialized diagnostic formatを導入しません。

LSPはcompilerのparser、resolver、type checker、source map、incremental query databaseを再利用します。不完全またはinvalidなprogramでもresponsiveである必要があります。

2026-10-17実装：`lang-lsp` は既存structured diagnostic modelをUTF-16 client profileのLSP diagnostic dataへ変換します。Source byte spanをcompiler line番号とは独立して変換します。LSPはLF、CRLF、単独CRを認識し、0始まりのUTF-16位置とexclusiveなrange endを使います。CRLF途中のbyte境界は前の行のcontent endへ正規化します。Adapterはsource spanと座標上限を検証し、source code／severityとrelated labelを保持し、primary-label詳細／noteをplain diagnostic textへ含めます。Hostが検証済みdocument URIを渡し、source表示名からURIやfilesystem accessを推測しません。明示source-byte、label-count、note-count、output-byte limitで変換を制限します。Fixは将来のcode-action adapter向けにcompiler diagnostic側へ残します。LSP prototypeを主張する前にclient capability filter、UTF-16 negotiation、JSON serialization／transport、document version／close notification、parser／module query、shared-graph publicationの統合が必要です。

Protocol情報源：Microsoft LSP 3.18の[text document](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/textDocuments.md)、[position](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/position.md)、[range](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/range.md)、[diagnostic](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/diagnostic.md)、[base integer bound](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/specification.md#baseTypes)。いずれも更新日を本文に掲載しないliving documentで、2026-10-17 JSTに公式repositoryを確認しました。外部protocolの位置／data要件に従うadapterであり、resource limitと段階的統合はprojectの実装判断です。

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

## 実装レビュー（2026-10-17）

現状の実装はMIR source mapをbytecodeとnative rangeで保持し、実際のCodeView parameter／result typeを記録し、NUL関数名とhost-import symbol衝突を拒否し、compiled function破棄までJIT executable mappingを所有します。狭いunsafe releaseはprivate code pointerとownerをborrowするinvokeに基づき、error pathでも未公開mappingを解放します。

Component validationは直接構築した不正SemVerを各boundaryで確認し、borrowed resultを拒否します。すべてのinterface capabilityをprovider／lock lookup前にpreflightします。両schemaのdecoderはstorage予約前に宣言collectionが残りpayloadへ収まるか確認します。これはartifact認証ではなく、将来loaderが実byteを検証するまでは選択digestは供給されたmetadataです。

Rust adapter cleanupはdestructor error表示／破棄、panic payload破棄からのpanicを封じ込め、独立cleanupを継続し、active call放棄時は両instanceをpoisonします。放棄時にはhost callbackなしでhost destructorを実行できず、hostが隔離valueをteardownする必要があります。再帰的panic payload破棄では2つ目のpayloadを意図的にleakしえます。Abort-mode panic、foreign unwind、直接native callbackはこのcontainmentの対象外です。

Response fileは排他的に作成・保持し、retryでは新しいpathを使用します。既存file／symlinkは上書きしません。Linker設定とnative callbackはtrusted host inputであり、script capabilityではありません。

情報源：lock済みCranelift JIT 0.136.3の[memory解放contract](https://docs.rs/cranelift-jit/0.136.3/cranelift_jit/struct.JITModule.html#method.free_memory)を2026-10-17にinstalled versioned sourceで確認（公開／更新日記載なし）。Microsoft [CodeView型定義](https://github.com/microsoft/microsoft-pdb/blob/master/include/cvinfo.h)は公開日表示のないliving sourceで、2026-10-17確認。Repository実装と回帰testは2026-10-17更新。

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
