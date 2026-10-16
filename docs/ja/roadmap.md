<!-- translation-of: ../roadmap.md; canonical-language: en -->

# 実装ロードマップ

[English](../roadmap.md)

このロードマップはrisk reductionの順に並んでいます。Milestoneはcalendar commitmentではなくexit criteriaです。

## Milestone 0：設計baseline

現在の進捗：最初の型付き関数は、検証済みbytecodeとCranelift JIT prototypeの両方で実行でき、同じMIR loweringからhost-native object artifactを生成できます。最小の型付き `i64` host-function registration、通常のscript-to-host callback、allocation-backed callbackは両方のexecution modeで実行でき、object outputはcallbackをimportとして表現します。VMはopt-inのdeterministic instruction-fuel／host-call budget、allocation前のvalue-slot limit、cooperativeなone-way interruption flag、絶対monotonic deadlineを合成できるようになりました。明示的なbranch、jump、return terminatorを持つ型付きbasic blockをMIR、bytecode、VM、JIT、AOTで共有し、validatorがtargetの存在、reachability、SSA dominanceを強制し、fuel-bounded back-edge testがnon-terminationを検証します。型付きblock parameterとedge argumentがtest済みcountdown back-edgeを介して `i64` を運び、bounded sum workloadはcounterとwrapping accumulatorの両方をloopで運びます。MIRとbytecodeのvalidatorはentry-block制約とedge argumentのarity／typeを強制し、VMとJITの結果が一致し、AOT object emissionも成功します。初期の共通 `SystemLinker` driverが順序付きlink requestを検証し、host toolchainのcommand flavorを選択してstructured failureを報告し、macOS AArch64 integrationはadd、minimum、通常のhost-callback、host-allocation、accumulator、conditional-sum objectをこのdriver経由でlinkして実行します。現在のcleanなmacOS AArch64 [schema 11 benchmark baseline](benchmarks.md)は、add、minimum、loop-carried countdown、2-value accumulator、conditional-sumのlowering、JIT／AOT compilation、code／object／link済みexecutable size、direct／VM／JIT call、通常、single-allocation、64-allocation-burstのhost callback、unset-flag interruption-check／期限前deadline比較、system linking、single-launchとbatched warm-cache AOT process startupを扱います。3つすべてのmemory-model stress caseに実行可能な厳格modelがあります。すなわち、no-borrow-across-suspension ruleとdeterministic cancellation cleanup、明示的なshared／weak graphとarena-scoped cyclic graph、moved messageを使うactor-isolated shared mutable serviceです。Generated-code inspectionでは、2 instructionのAArch64 add、compare-and-branch minimum、host callback用のposition-independentなGOT importを確認しました。RFC 0001はproposedのままであり、より制限の少ない代替案、より広いembedding limitとnative enforcement、production linker hardeningとstartup／runtime packaging、Windows x86-64でのlink、execution、benchmark、CI evidence、source allocation semanticsと代表的なallocation／memory workload、提案済みsource-level control-flow semanticsのacceptance／implementation、追加CFG workload、heap／debug-quality measurement、backend決定は未完了です。

CFG移行後の再検査では、minimumのcode／object sizeとinstruction sequenceが変わらないことを確認しました。`SystemLinker` は `Cc` と `MsvcLink` のcommand flavorをmodel化して対応するhost defaultを選択し、ordered prefix argument付きの明示driverをsupportし、完全なcommandを公開してstructured execution failureへ保持します。Ordered library search pathとlogical library nameをflavor固有argumentへloweringし、validationが空のobject path、search path、nameを拒否します。明示的なresponse-file linkは、`Cc` にGNU-escaped UTF-8、`MsvcLink` にWindows-escaped BOM-prefixed UTF-16LEを使用します。Focused testがserializationとvalidationを検証し、macOS AArch64 integrationがdirect linkとresponse-file linkの両方を実行します。Toolchain discoveryとより広いtarget policyは未完了です。Target指定object emissionは明示的なtripleからISAを構築し、host非依存testが `x86_64-pc-windows-msvc` COFF outputのformat、architecture、export、importを検証します。cfg-gated MSVC direct／response-file object-link-execute testと完全SHAでpinしたWindows Server 2025 workflowを構成しましたが、このcommitにはWindows runnerの結果が記録されていないため、Windowsでのlinkとexecution evidenceは未完了です。

初期host-toolchain discoveryは、deterministicなcandidate順で `PATH` を探索し、non-executable Unix fileを拒否し、探索したpathとcandidateをstructuredに報告して、macOS integrationを駆動します。cfg-gated MSVC integrationは、developer environmentが `PATH` を初期化した後に同じdiscoveryを使用します。初期化済みenvironmentの外側にあるVisual Studio、SDK、compiler installationのdiscoveryは未完了です。

初期tier-1 linker policyを明示しました。Backend-neutralな `LinkTarget` valueがexactな `aarch64-apple-darwin` と `x86_64-pc-windows-msvc` tripleを所有し、command flavor、default program、ordered discovery candidateへ対応付けます。明示targetの構築／discovery、COFF object test、両platform integrationがこのmappingを共有し、未supportのtripleはstructuredに失敗します。追加targetのpolicy、installation discovery、startup／runtime inputは未完了です。

Schema 3 harnessは、addとminimumに加えてbounded countdown back-edgeのlowering、JIT／AOT compilation、direct／VM／JIT call、artifact size、system linkingを計測します。新fieldとrename済みlink seriesは明示的にversion管理します。CleanなmacOS AArch64 schema 3 baselineを記録しました。Windows CIは対応JSONを検証して30日間uploadしますが、remote runner結果はまだ記録していません。Allocation、より広いCFG coverage、Windows benchmarkの解釈は未完了です。

Schema 4 harnessは、両tier-1 evidence pathでlink済みAOT executable sizeと反復warm-cache process startupを記録します。このrowをsystem-link timeから分離し、process creation、loader処理、小さなharness、output collection、terminationを含むことを文書化します。CleanなmacOS AArch64 resultは16,992-byte executableと1.511 ms median startupを記録しましたが、49.824 msの外れ値によりstartup RSDが276.388%になったため、budget設定前にmethodを改善する必要があります。Windows CIは両rowと正のexecutable sizeを検証します。Memory、debug quality、allocation、より広いCFG coverage、remote Windows evidence、cross-platform interpretationは未完了です。

Schema 5 harnessはsingle-launch startup rowを維持し、連続launch全体のlaunch当たり経過時間を報告する設定可能なbatched rowを追加します。これにより、tail-sensitiveなlatency evidenceとnoiseの影響を受けにくいamortized estimateを分離します。CleanなmacOS AArch64 resultは1.487 msのsingle medianと1.499 msのbatched medianをほぼ同じに保ちつつ、10-launch batchのRSDを256.272%から4.688%へ下げました。Windows CIは設定済みbatch sizeと両rowを検証します。Remote Windows comparisonと残りのevidence gapは未完了です。

Schema 6 harnessは、2-value accumulatorをlowering、JIT／AOT compilation、direct／VM／JIT call、生成code／object size、system linking、link済みexecutable size、両startup rowへ追加します。4-object link setを反映するためplatform seriesをrenameし、Windows CIは新しいaccumulator／platform fieldを検証します。CleanなmacOS AArch64 resultは全30 rowと17,056-byteのlink済みharnessを記録しました。Remote Windows evidenceは、allocation、追加のsource-level control flow、memory、debug quality、backend decisionの作業とともに未完了です。

Schema 6後のconditional-sum CFG prototypeは、bounded accumulator loopへ内部branchを追加し、add／skip pathをblock parameterでmergeしてから共有back-edgeを取ります。VM／JIT differential testは両pathを扱い、macOS AArch64 direct／response-file integrationは5番目のAOT objectを実行し、host非依存Windows x86-64 COFF validationはそのexportを検証します。cfg-gated MSVC integrationも同じlink-and-run pathを反映します。Source-level control-flow semanticsと追加CFG／allocation workloadは未完了です。

Schema 7 harnessはconditional-sum workloadをlowering、JIT／AOT compilation、direct／VM／JIT call、generated-code／object size、拡張された5-object system-link／startup setで計測します。6つのconditional-sum rowを追加し、そのsetに合わせてplatform seriesとlink済みexecutable fieldをrenameします。Windows CIはschema 7 contractを検証します。CleanなmacOS AArch64 resultは全36 row、44-byteのJIT body、400-byteのobject、17,104-byteのlink済みharnessを記録しました。Source-level semantics、allocation／memory workload、debug-quality evidence、追加CFG coverage、remote Windows execution／benchmark evidenceは未完了です。

Async semantics実験は、厳格なdefaultの `RejectAll` suspension policyと、明示的な `AllowSharedFrameBorrows` 代替案を比較します。代替案はshared task-frame borrowをsuspend／resume越しに維持し、exclusive borrowは引き続き `SEM0007` でrejectし、cancellationがowned valueをdropする前にactiveなshared borrowを終了します。これはsemanticsをacceptせずに、より制限の少ないdesignの論点を狭めます。Pinned／exclusive borrow、source-level frame-residency proof、destructor failure、VM／native trace、runtime costは未完了です。

Cyclic-graph実験は、defaultの `Reject` shared-cycle policyと、明示的な `CollectUnreachable` 代替案を比較します。Collectorはstrong cycleを許可し、root dropごとに残るlive rootからstrong edgeをtraceし、いずれかのrootから到達できる間はcycleを維持し、最後のrootが消えると逆生成順に回収します。Weak edgeはtargetをretainしません。これにより、最も強い競合designをacceptせずに実行可能にします。Production collectionのtrigger／latency、incremental／concurrent behavior、allocation／peak-memory cost、finalization、weak-reference ordering、destruction observabilityは未完了です。

Shared-service実験は、defaultの `OwnerIsolated` mutation policyと、明示的な `ExclusiveCapability` 代替案を比較します。代替案では、有効なhandleを持つ任意のtaskが単一のactive capabilityを保持している間にmutationでき、handleのないtaskと同時mutatorはstructured diagnosticでrejectされ、cancellationはnon-owner mutationの境界までcleanupを待ちます。これはlock-likeな競合designをacceptせずに実行可能にします。Read／write／atomic capability、capability derivation、scheduling fairness、priority inversion、poisoning、deadlock、reentrancy、panic handling、backpressure、VM／native trace、runtime costは未完了です。

Host-allocation boundary実験は、single-allocation callbackと、64個のbox／512 payload byteを同時に保持してから合計・dropするburst callbackを持つようになりました。VM／JIT差分testはRustの `Box` callbackを使用し、macOS AArch64 direct／response-file AOT integrationと構成済みWindows MSVC pathは、別objectがimportする対応するCの `malloc`／`free` callbackを使用します。Schema 8はsingle-allocation callbackのdirect Rust／VM／JIT runtime rowを追加し、schema 11はAOT link／startup setを変えずにburst用の同じ3 rowを追加します。Windows CIは6 rowすべてを検証します。Clean schema 11 macOS AArch64 baselineのsingle-allocation medianはdirect 12.272 ns、VM 190.573 ns、JIT 14.123 nsで、burst medianは782.624 ns、962.329 ns、786.270 nsです。Source allocation semantics、ownership、allocator／collector policy、memory limit、allocator-overhead／peak-memory accounting、より広い代表的workload、remote Windows evidence、cross-platform costは未完了です。

VM resource-control prototypeは、optionalなinstruction-fuel／host-call budget、value-slot limit、共有one-way interruption flag、絶対monotonic deadlineを合成できるようになりました。Instruction／terminatorと実行に入るcallbackはそれぞれのcounterを消費し、対応するside effectの前にlimitを確認し、検証済みregister countはregister-file allocation前に確認します。すでにsetされたinterruptionまたは期限切れdeadlineはそのallocation前に停止し、以降のconditionはinstruction、terminator、pre-callback safe pointで観測して、structured errorに両方の消費counterを含め、interruptionを優先します。Callback-triggered requestまたはdeadline overrunはそのcallback完了後に観測します。成功したlimit付きexecutionは両方の動的countと静的slot footprintを報告します。Zero host-call budgetはcallbackを抑止し、過小なslot limitはexecutionを拒否し、どちらもstructured errorを返します。既存convenience APIはunbounded、non-interruptible、deadlineなしのままです。Heap、recursion、task、callback内部のpreemption、byte単位accounting、native／JIT enforcementは未完了です。

[RFC 0002](rfcs/0002-prototype-control-flow.md)は、実装済みinteger／CFG prototypeへ提案中のsource-level interpretationを与えます。Wrapping `i64` addition、signed comparison、left-to-right evaluation、mutable local、structured `if`／`else`／`while`、明示的return、typed block argumentを介したlive valueのloweringを定義します。Focused RFCはowner decisionに向けてreadyですがproposedのままです。Parser、HIR、formatter、source diagnostics、source-to-MIR implementationは未完了です。

Schema 9 harnessは、既存VM countdown rowの隣にunset-flagの `vm_countdown_interruptible_call` rowを追加し、input sequence／bytecodeを変えずにincremental safe-point acquire-load costを直接計測できるようになりました。現在のclean schema 11 runはこのrowを維持し、通常／interruption-enabled VM countdown medianとして1.602 us／1.616 usを記録します。14.056 ns（0.878%）の差は両rowの4.793%／22.662% RSDより小さいため、測定可能なregressionを示さず、overhead budgetも確立しません。Schema 11が現在のCI contractです。Remote Windows resultはまだ記録していません。

Schema 10 harnessは、harness準備の1時間後に設定した絶対monotonic deadline付きで同じcountdown bytecode／inputを実行する `vm_countdown_deadline_call` を追加します。Deadlineが期限前のままのsteady-state clock-check costを分離します。Clean schema 11 macOS AArch64 runはこのrowを維持し、通常／期限前deadline付きVM countdown medianは1.602 us／3.324 us、1.723 usまたは107.558%の増加でした。Windows CIはこのrowを要求しますが、remote Windows resultはまだ記録していません。

Schema 11 harnessは64-allocation burst用のdirect Rust／VM／JIT rowを追加します。Cleanな31-sample macOS AArch64 runは44行すべてが一意であることと782.624 ns／962.329 ns／786.270 nsのburst medianを記録し、macOS AOT／host非依存Windows COFF testは新callback importを扱います。Windows CIは新しい3 rowすべてを要求しますが、remote Windows schema 11 resultはまだ記録していません。

- Goal、non-goal、terminologyを承認する。
- RFC processとcompatibility labelを確立する。
- Cyclic graph、async borrow／cancellation、shared mutable serviceという3つのmemory-model stress caseをprototypeする。
- Typed functionをbytecodeとnative loweringの両方でprototypeする。
- 初期tier-1 architecture pairとlinker strategyを選定する。

Exit：prototype subsetの意味論が文書化され、初期backend選定を支持するbenchmark evidenceがある。

## Milestone 1：frontendとbytecodeのvertical slice

- Error recoveryとsource spanを持つlossless parser。
- Name resolution、module、primitive type、function、record、variant、pattern matching。
- Stable codeとfix-itを持つstructured diagnostics。
- Validationを持つtyped MIR。
- Register bytecode、verifier、VM、REPL／script runner、host function registration。
- Formatterとparser／formatter round-trip test。
- LSP prototypeと共有する初期incremental query graph。

Exit：小さなprogramをscriptとして実行でき、deterministic behaviorと有用なIDE diagnosticsがある。

## Milestone 2：ownershipとnative AOT

- Owned value、move、lexical borrow、deterministic destruction、狭いunsafe operation。
- Allocator-aware standard container。
- C ABI import／exportとgenerated header。
- macOS AArch64とWindows x86-64向けCranelift object generation。
- VM／native differential execution suite。
- Breakpointとstack traceに十分なnative debug metadata。

Exit：代表的なcommand-line programがVM semanticsと一致し、同等のRust／C実装に対してperformanceが計測されている。

## Milestone 3：concurrencyとembedding

- Structured task、cancellation、channel、actor isolation、data-race capability rule。
- Stable C embedding APIとgenerated binding。
- Version付きcomponent IDL、resource handle、capability import。
- Mixed native／VM call、callback、error、stack trace、cancellation。
- Script resource limitとinterruption。
- Trusted native／verified script pluginのexample。

Exit：Native hostがuser scriptをloadし、typed APIを公開し、scriptをcallし、callbackを受け取り、IPCなしでcapabilityを強制できる。

## Milestone 4：developer toolchain

- Navigation、completion、rename、semantic token、code actionに対応した完全なLSP機能。
- Mixed native／VM stackを持つDAP adapter。
- `lang` build／package／test／fmt／lint／doc command。
- Deterministic resolution、checksum、offline mode、target conditionを持つmanifest／lockfile。
- Compile-time／runtime profiling、allocation trace、rebuild explanation。
- `rustup` と同じ役割を持つinstall／update／channel tool。

Exit：新しいdeveloperが1つのtoolをinstallし、packageを作成し、editorで開発し、test／debugし、reproducible binaryを生成できる。

## Milestone 5：ecosystemとcompatibility

- Edition、deprecation policy、migration／fix tool、compatibility test matrix。
- Signed release manifestとpackage provenance。
- Documentation hostingとpackage metadata search。
- Upload受付前のpublic registry threat model。
- Sandboxed portable plugin向けWebAssembly component target。
- 公開benchmarkとdebug品質基準によるLLVM release backendの評価。

Exit：Source、package、bytecode、component consumerを暗黙的に破壊せず言語を進化させられる。

## Milestone 6：セルフホスティング

- Self-hosting subsetを定義してfreezeする。
- 新言語でfrontendとcore compiler stageを実装する。
- Stage 0 Rust compilerでstage 1をbuildし、stage 1でstage 2をbuildする。
- 両tier-1 platformでreproducible outputを比較する。
- Bootstrap trustとdisaster-recovery procedureを文書化する。

Exit：Stage 2がstage 0と同じconformance suiteに合格し、文書化されたbootstrap chainからsupport対象toolchainをbuildできる。

## Milestone 7：追加frontend

- 明確に別のhigh-level／domain-specific languageが、library、profile、syntax sugarでは満たせない要件を持つかreviewする。
- Frontend-specific HIRを維持し、coreが忠実に表現できる意味論だけをshared MIRへloweringする。
- Memory、effect、runtime modelが異なる場合はcomponent ABIを使用する。
- Interoperabilityをstableと宣言する前に、cross-language debugging、package metadata、conformance testを追加する。

Exit：2つ目のfrontendが明確なuser valueを追加し、最初の言語の安全性を弱めず、shared IRを無関係な意味論の保守不能な集合にしない。

