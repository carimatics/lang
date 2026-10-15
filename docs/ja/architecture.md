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

Prototypeが必要な未解決問題には、cyclic graphのergonomics、borrowを伴うasync suspension／cancellation、self-referential value、cancellation中のdestruction、atomic `shared` referenceのcost modelがあります。

### Cyclic graph実験

`crates/semantics` のgraph modelでは、意図的に明示的な2つのownership戦略を比較します。

- Shared nodeはexternal rootを1つ持って開始する。Strong edgeはtargetをretainし、weak edgeはretainしない。
- Shared-reference cycleを閉じるstrong edgeはstructured diagnostic `SEM0018` でrejectする。同じcycleをweak edgeで閉じることは許可し、expired weak edgeのupgradeは `SEM0021` を報告する。
- 1つのarena内のnodeは、arenaがlifetimeを所有するため任意のcycleを形成できる。Arena間、またはarena storageとshared storage間のedgeは `SEM0017` を報告する。
- Arenaをdropするとnodeを逆生成順に正確に1回releaseする。その後にstale node handleを使うと `SEM0012` を報告する。
- すべてのoperationがsource spanを持ち、関連するallocationまたはedge spanが存在する場合はdiagnosticに保持する。

この厳格なmodelは暗黙のreference-counted cycleを避けてregion boundaryを可視化しますが、surface syntax、atomic、thread safety、destructor failure、production reclamation algorithmは定義しません。最も強い競合designはstrong cycleを許可してtracingまたはcycle collectionを追加する方式であり、より単純な代替案はarena-only graph storageです。Library規約だけではstale-handle rejectionを保証できず、loweringを通じて同じdiagnostic contractを維持できません。

レビュー結果：**needs prototype/measurement**。Blocking questionは、shared cycleをstaticまたはdynamicのどちらでrejectするか、generation-tagged arena handleが必要か、両戦略がallocation costとdestruction observabilityへどう影響するかです。Surface syntaxとgraph-container APIは任意のfollow-upです。

### Async borrow／cancellation実験

`crates/semantics` の実行可能modelでは、意図的に厳格なbaseline仮説を検証します。

- Task-owned valueはsuspensionを越えてtask frame内でliveのままにする。
- Suspension pointでactiveなshared／exclusive borrowはstructured diagnostic `SEM0007` でrejectし、suspension前に終了したborrowはacceptする。
- Cancellationはtaskがsuspendedである間だけ観測する。
- Completionとcancellationでは、task-owned valueを逆生成順に正確に1回dropする。
- すべてのoperationがsource spanを持ち、failureではboundaryとそれを越えたborrowの両方を示す。

これはより単純な代替案であり、承認済み意味論ではありません。最も強い競合designは、選択されたframe-resident borrowまたはpinned borrowがsuspensionを越えることを許可します。Library規約だけではcompilerのborrow ruleを強制できず、決定を先送りすると未解決のcancellation／self-reference制約がMIR loweringへ流入します。後続prototypeではこれらの代替案を比較し、nested scope、partial initialization、destructor failure、panic containment、self-referential task frame、VM／native traceを扱う必要があります。

レビュー結果：**needs prototype/measurement**。blocking questionは、borrowがsuspensionを越えられるか、task-frame ownershipとpinningがどう相互作用するか、destructor failure時にcleanupがどう動作するかです。Surface syntaxとscheduler APIは任意のfollow-upです。

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

`crates/semantics` のservice modelでは、厳格なactor-isolation仮説を検証します。

- 1つのowner taskだけがservice stateへ直接mutable accessを持つ。Shared handleはmessageをenqueueできますが、別taskからのdirect mutationは `SEM0031` を報告する。
- Sendはpayloadをmailboxへmoveする。Non-sendable payloadは `SEM0028`、成功したmove後の再利用は `SEM0027` を報告する。
- OwnerはmessageをFIFO順にprocessする。有効な任意のhandleからのcancellation requestは、active mutationが終了するまでdeferする。
- Cancellationはqueued messageをFIFO順にdiscardし、queued payloadとservice stateをそれぞれ正確に1回dropしてから、後続sendを `SEM0024` でrejectする。
- Operationはstable diagnostic code `SEM0022`〜`SEM0035` のもとでprimaryおよび関連source spanを保持する。

このmodelはsurface syntax、scheduler thread、atomic ordering、mailbox backpressure、fairness、panic behavior、distributed serviceを定義しません。最も強い競合designはlockまたはatomic capabilityを通じてshared mutable stateを公開する方式であり、より単純な代替案はshareable handleを持たないsingle-threaded event loopです。Library-only actorではsendabilityやisolationを言語levelのpropertyとして強制できません。

レビュー結果：**needs prototype/measurement**。Blocking questionは、send／share capabilityをどう導出するか、cancellationがbackpressureやhandler failureとどう相互作用するか、VM／native executionが同等のorderingとcostを維持するかです。Surface syntaxとstandard actor APIは任意のfollow-upです。

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

VM prototypeは、狭いresource-control layerとしてopt-inのinstruction-fuel budgetを実装します。各bytecode instructionとterminatorが1 unitを消費します。Validationとarity checkはmetering前に行い、host callbackはそのinstructionのfuel消費後にだけ実行します。Limit付きAPIはvalueと消費fuelの両方を返し、既存のconvenience APIはunboundedのままです。非停止back-edge testと有限loop-carried countdown testにより、このbudgetがbytecode loopを扱うことを確認しています。Memory、recursion、task、wall-clock time、外部interruption、native／JIT executionは未対応であるため、これは完全なsandboxではなくembedding interfaceの証拠です。

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

現在のprototypeは、同じ検証済みMIR loweringからin-process JIT codeまたはobject artifactを生成します。Object artifactは、Craneliftまたはobject-libraryの型をcallerへ公開せず、byte、exportされたfunction symbol、生成function-code sizeを提供します。Object emissionはnative host ISAを使用することも、明示的なtarget tripleからISAを構築することもできます。Host非依存testは `x86_64-pc-windows-msvc` outputをx86-64 COFFとしてparseし、exportされたMIR functionと未定義のhost-callback importを検証します。これはcross-target object構築を証明しますが、Windowsでのlinkまたはexecutionを証明するものではありません。`Branch`、`Jump`、`Return` terminatorを持つ型付きbasic blockが、MIR、検証済みbytecode、VM、Craneliftで共有する最初の明示的CFGを提供します。型付きblock parameterとedge argumentは、専用phi instructionなしでphiに相当するmergeを表現します。Validationでは、すべてのtargetが存在すること、すべてのblockがreachableであること、entry blockにpredecessorもblock parameterもないこと、edge argumentのarityと型がtarget parameterに一致すること、各SSA definitionがuseをdominateすることを要求します。Entry block以外へのback-edgeは有効です。Countdown workloadはback-edgeを介して `i64` を運び、VMとJITの実行結果が一致し、object pathも同じfunctionをemitします。VMは動的に実行した各instructionとterminatorを課金するため、fuelは非停止loopをdeterministicに停止します。Root crateは、target-neutralでobject順序を保持する `LinkRequest` と初期host `SystemLinker` adapterを所有します。このadapterはrequestを検証し、hostの `cc` またはordered prefix argument付きの明示driverを起動し、完全なcommandを公開して、structuredなspawn、process-failure、missing-output diagnosticへそのcommandを保持します。macOS AArch64 integration coverageとlink benchmarkは、この共通driverを使用します。これは完全なCFGでも完全なAOT pathでもありません。Aggregate value、allocation、source-level control-flow semantics、toolchain discoveryとtarget別policy、response file、library、search pathなどのproduction linker hardening、startup code、stable ABI adapter、debug metadata、Windowsでのlink、execution、benchmark、CI coverageは個別の作業として残っています。

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
