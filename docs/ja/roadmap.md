<!-- translation-of: ../roadmap.md; canonical-language: en -->

# 実装ロードマップ

[English](../roadmap.md)

このロードマップはrisk reductionの順に並んでいます。Milestoneはcalendar commitmentではなくexit criteriaです。

## タスク管理

各milestone内のtaskは優先度順に並べます。`P0` taskはそのmilestoneのexitをblockし、`P1` taskは必要なfollow-upですがP0 blockerより優先しません。Check済みtaskは完了です。前のmilestoneで完了したprototype groundworkだけでは、後続milestoneのより広いtaskを完了扱いにしません。

Costは経過時間の約束ではなく相対的なeffort pointです。`1`はごく小さい変更、`2`は小、`3`は中、`5`は大、`8`は非常に大、`13`は複数の連携変更にまたがる規模、`21`はprogram規模の作業を表します。進捗は `完了point / 全point × 100` を小数第1位へ丸めて算出します。Scopeやevidenceが変わったときは見積りと完了状態を更新します。

Resource-handle prototype後の再検証で、残り5 ptのcomponent taskが3つの独立したrisk class、すなわちuntrusted byteのbounded decoding、compatibility／canonical-resource semantics、cross-mode conformance／measurementを混在させていることが判明しました。RFC 0003もそれぞれを別のblocking evidenceとして列挙しています。現在はMilestone 0のprototypeとして5、5、8 ptへ分割し、Milestone 3にはより広いproduction IDL／stable embedding implementationを残します。

このrepository stateに対して2026-10-16 15:14 JSTに確認した進捗snapshot：

| Milestone | 完了 | 総計 | 進捗 |
| --- | ---: | ---: | ---: |
| 0 — design baseline（現在） | 80 pt | 151 pt | **53.0%** |
| 1 — frontendとbytecodeのvertical slice | 0 pt | 52 pt | 0.0% |
| 2 — ownershipとnative AOT | 0 pt | 53 pt | 0.0% |
| 3 — concurrencyとembedding | 0 pt | 55 pt | 0.0% |
| 4 — developer toolchain | 0 pt | 58 pt | 0.0% |
| 5 — ecosystemとcompatibility | 0 pt | 55 pt | 0.0% |
| 6 — セルフホスティング | 0 pt | 50 pt | 0.0% |
| 7 — 追加frontend | 0 pt | 39 pt | 0.0% |
| **全体** | **80 pt** | **513 pt** | **15.6%** |

## Milestone 0：設計baseline

**進捗：80 / 151 pt（53.0%）。**

### 優先度順のタスク

- [x] **P0 — Project charter、goal／non-goal、terminology、RFC process、compatibility labelを草案化する。** — 3 pt
- [ ] **P0 — RFC 0001／RFC 0002を判断し、acceptance、修正要求、rejectionのいずれかを記録する。** — 3 pt
- [x] **P0 — Host registration／callbackを含むtyped `i64` MIR → bytecode／VM → Cranelift JIT／AOT vertical sliceを構築する。** — 13 pt
- [x] **P0 — Typed CFG validationと、代表的なprototype branch、back-edge、loop-carried valueをVM／JIT／AOT差分coverage付きで追加する。** — 8 pt
- [x] **P0 — 共通system-linker path、macOS AArch64 link-and-run integration、Windows MSVC workflow、host非依存COFF contractを確立する。** — 8 pt
- [ ] **P0 — Windows x86-64 CIのlink／run成功evidenceと、macOSと比較できるclean benchmark baselineを記録する。** — 8 pt
- [x] **P0 — Deterministic VM fuel、host-call／value-slot limit、cooperative interruption、absolute deadlineをprototypeする。** — 8 pt
- [x] **P0 — Version付きscalar component metadataと明示的にcapability-gatedなimportをtyped host registryに対してvalidateする。** — 5 pt
- [x] **P0 — Generation tag付きowned／borrowed resource handleとdeterministic lifecycle validationをprototypeする。** — 3 pt
- [ ] **P0 — Byte／count／depth／string limit、location-bearing diagnostics、malformed fixture、fuzz targetを持つbounded schema-1 component encoder／decoderをprototypeする。** — 5 pt
- [ ] **P0 — Scalar／resource canonical representation、package-version compatibility negotiation、ownership transfer、panic／destructor failure ruleをRFC 0003で仕様化する。** — 5 pt
- [ ] **P0 — 1つのvalidated scalar import／exportとresource lifecycleをVM／JIT／AOT differential conformance／boundary-cost benchmarkへ通し、RFC 0003をdecision-readyにする。** — 8 pt
- [ ] **P0 — Heap、recursion、task、native／JIT executionとcallback behaviorへresource enforcementを拡張する。** — 13 pt
- [x] **P0 — Async cancellation、cyclic graph、shared mutable serviceについて、厳格／制限の少ない代替案をprototypeする。** — 8 pt
- [x] **P0 — RFC 0002でsource-level integer／structured-control-flow semanticsをdecision-readyな状態まで草案化する。** — 5 pt
- [x] **P0 — Version付きbenchmark harnessをschema 14まで構築し、compile、runtime、startup、code size、logical heap lifetime、process memoryを含むcleanなmacOS AArch64 baselineを記録する。** — 13 pt
- [ ] **P0 — 完了したcross-platform performance、debug-quality、integration evidenceから初期native backendを選定し、decisionを記録する。** — 3 pt
- [x] **P1 — Direct Rust、VM、JIT、macOS AOT、Windows object contractでvariable-size host-allocation executionを追加する。** — 3 pt
- [ ] **P1 — Variable-size allocation callbackのtiming／正確なrequested-payload profileを追加し、次のclean baselineを記録する。** — 5 pt
- [ ] **P1 — Allocator-overhead／fragmentation evidenceを含む代表的allocation／memory workloadと追加CFG workloadを加える。** — 8 pt
- [x] **P1 — AArch64 generated-code inspectionと現在の負のnative debug-information baselineを記録する。** — 3 pt
- [ ] **P1 — macOS AArch64／Windows x86-64でsource-span native debug metadataを実装・検証する。** — 8 pt
- [ ] **P1 — Linker discovery、startup／runtime packaging、production error handlingをhardeningする。** — 5 pt

### Evidenceと現在の状態

最初のcomponent-contract sliceはportable declarationをembedding-local bindingから分離しました。Schema-1 in-memory metadataはpackage identity／versionと名前付きscalar import／exportを記録します。各importがcapabilityを宣言し、別のbinding tableが論理nameをprocess-localな `HostFunctionId` へmapします。Validationはunknown schema、invalid／duplicate name、import／export collision、unsupported import type、undeclared／missing binding、grant不足、unregistered host functionを拒否します。Unauthorized componentがhost inventoryをprobeできないようcapability checkをregistry lookupより前に行い、成功時にはresolved bindingを持つ別のwrapperを返します。7件のruntime testがaccepted pathと主要なrejection／ordering contractをcoverします。

Component runtimeはgeneration tag付きowned handleと明示的なshared／exclusive borrow tokenもprototypeしました。Released slotはgenerationを進め、最小のfree slotをdeterministically再利用するため、stale ownerはreplacementをaliasできず、generation exhaustionではwrapせずslotをretireします。Shared borrowは共存し、exclusive borrow／mutable accessはexclusivityを強制し、active borrowはreleaseをblockし、borrow end／releaseのrepeatはstructuredに失敗します。6件のfocused testがslot reuse、stale owner／borrow、double release／end、conflict、mutation、deterministic reuse、generation retirement、borrow-ID exhaustionをcoverします。[RFC 0003](rfcs/0003-versioned-component-contracts.md)はproposedのままで、serialized resource representation、cross-component ownership transfer、compatibility negotiation、canonical lowering、component execution、boundary-cost evidenceを明示的に未完了としています。

現在の進捗：最初の型付き関数は、検証済みbytecodeとCranelift JIT prototypeの両方で実行でき、同じMIR loweringからhost-native object artifactを生成できます。最小の型付き `i64` host-function registration、通常のscript-to-host callback、allocation-backed callbackは両方のexecution modeで実行でき、object outputはcallbackをimportとして表現します。VMはopt-inのdeterministic instruction-fuel／host-call budget、allocation前のvalue-slot limit、cooperativeなone-way interruption flag、絶対monotonic deadlineを合成できるようになりました。明示的なbranch、jump、return terminatorを持つ型付きbasic blockをMIR、bytecode、VM、JIT、AOTで共有し、validatorがtargetの存在、reachability、SSA dominanceを強制し、fuel-bounded back-edge testがnon-terminationを検証します。型付きblock parameterとedge argumentがtest済みcountdown back-edgeを介して `i64` を運び、bounded sum workloadはcounterとwrapping accumulatorの両方をloopで運びます。MIRとbytecodeのvalidatorはentry-block制約とedge argumentのarity／typeを強制し、VMとJITの結果が一致し、AOT object emissionも成功します。初期の共通 `SystemLinker` driverが順序付きlink requestを検証し、host toolchainのcommand flavorを選択してstructured failureを報告し、macOS AArch64 integrationはadd、minimum、通常のhost-callback、host-allocation、accumulator、conditional-sum objectをこのdriver経由でlinkして実行します。現在のcleanなmacOS AArch64 [schema 14 benchmark baseline](benchmarks.md)は、add、minimum、loop-carried countdown、2-value accumulator、conditional-sumのlowering、JIT／AOT compilation、code／object／link済みexecutable size、direct／VM／JIT call、通常、single-allocation、64-allocation-burst、allocation-churnのhost callback、3つのrequested-payload heap profile、unset-flag interruption-check／期限前deadline比較、system linking、single-launch／batched warm-cache AOT process startup、1,654,784〜1,671,168 byteの31個のlink済みprocess peak-memory sampleを扱います。Remote Windows evidenceはまだ記録していません。3つすべてのmemory-model stress caseに実行可能な厳格modelがあります。すなわち、no-borrow-across-suspension ruleとdeterministic cancellation cleanup、明示的なshared／weak graphとarena-scoped cyclic graph、moved messageを使うactor-isolated shared mutable serviceです。Generated-code inspectionでは、2 instructionのAArch64 add、compare-and-branch minimum、host callback用のposition-independentなGOT importを確認しました。RFC 0001はproposedのままであり、より制限の少ない代替案、より広いembedding limitとnative enforcement、production linker hardeningとstartup／runtime packaging、Windows x86-64でのlink、execution、benchmark、CI evidence、source allocation semanticsと代表的なallocation／memory workload、提案済みsource-level control-flow semanticsのacceptance／implementation、追加CFG workload、allocator-overhead measurement、native debug-metadata implementation、cross-platform debug-quality validation、backend決定は未完了です。

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

Host-allocation boundary実験は、single-allocation callbackと、64個のbox／512 payload byteを同時に保持してから合計・dropするburst callbackから始まりました。VM／JIT差分testはRustの `Box` callbackを使用し、macOS AArch64 direct／response-file AOT integrationと構成済みWindows MSVC pathは、別objectがimportする対応するCの `malloc`／`free` callbackを使用します。Schema 8はsingle-allocation callbackのdirect Rust／VM／JIT runtime rowを追加し、schema 11はAOT link／startup setを変えずにburst用の同じ3 rowを追加します。Windows CIは6 rowすべてを検証します。Clean schema 11 macOS AArch64 baselineのsingle-allocation medianはdirect 12.272 ns、VM 190.573 ns、JIT 14.123 nsで、burst medianは782.624 ns、962.329 ns、786.270 nsです。Source allocation semantics、ownership、allocator／collector policy、memory limit、allocator-overhead／peak-memory accounting、より広い代表的workload、remote Windows evidence、cross-platform costは未完了です。

VM resource-control prototypeは、optionalなinstruction-fuel／host-call budget、value-slot limit、共有one-way interruption flag、絶対monotonic deadlineを合成できるようになりました。Instruction／terminatorと実行に入るcallbackはそれぞれのcounterを消費し、対応するside effectの前にlimitを確認し、検証済みregister countはregister-file allocation前に確認します。すでにsetされたinterruptionまたは期限切れdeadlineはそのallocation前に停止し、以降のconditionはinstruction、terminator、pre-callback safe pointで観測して、structured errorに両方の消費counterを含め、interruptionを優先します。Callback-triggered requestまたはdeadline overrunはそのcallback完了後に観測します。成功したlimit付きexecutionは両方の動的countと静的slot footprintを報告します。Zero host-call budgetはcallbackを抑止し、過小なslot limitはexecutionを拒否し、どちらもstructured errorを返します。既存convenience APIはunbounded、non-interruptible、deadlineなしのままです。Heap、recursion、task、callback内部のpreemption、byte単位accounting、native／JIT enforcementは未完了です。

[RFC 0002](rfcs/0002-prototype-control-flow.md)は、実装済みinteger／CFG prototypeへ提案中のsource-level interpretationを与えます。Wrapping `i64` addition、signed comparison、left-to-right evaluation、mutable local、structured `if`／`else`／`while`、明示的return、typed block argumentを介したlive valueのloweringを定義します。Focused RFCはowner decisionに向けてreadyですがproposedのままです。Parser、HIR、formatter、source diagnostics、source-to-MIR implementationは未完了です。

Schema 9 harnessは、既存VM countdown rowの隣にunset-flagの `vm_countdown_interruptible_call` rowを追加し、input sequence／bytecodeを変えずにincremental safe-point acquire-load costを直接計測できるようになりました。現在のclean schema 14 runはこのrowを維持し、通常／interruption-enabled VM countdown medianとして1.611 us／1.641 usを記録します。29.868 ns（1.854%）の増加はinterruption rowの15.866% RSDより小さいため、測定可能なregressionを示さず、overhead budgetも確立しません。Schema 14が現在のCI contractです。Remote Windows resultはまだ記録していません。

Schema 10 harnessは、harness準備の1時間後に設定した絶対monotonic deadline付きで同じcountdown bytecode／inputを実行する `vm_countdown_deadline_call` を追加します。Deadlineが期限前のままのsteady-state clock-check costを分離します。Clean schema 14 macOS AArch64 runはこのrowを維持し、通常／期限前deadline付きVM countdown medianは1.611 us／3.320 us、1.709 usまたは106.071%の増加でした。Windows CIはこのrowを要求しますが、remote Windows resultはまだ記録していません。

Schema 11 harnessは64-allocation burst用のdirect Rust／VM／JIT rowを追加します。過去のcleanな31-sample macOS AArch64 runは44行すべてが一意であることと782.624 ns／962.329 ns／786.270 nsのburst medianを記録し、macOS AOT／host非依存Windows COFF testは新callback importを扱います。Windows CIはschema 14で3 rowすべてを維持しますが、remote Windows resultはまだ記録していません。

Schema 12 harnessは44個のtiming rowを維持し、host allocation callbackに対する決定的なrequested-payload heap profileを追加します。Single workloadは1 allocation、total／peak live 8 byte、完了後0 byteを記録し、burstは64 allocation、total／peak live 512 byte、完了後0 byteを記録します。Cleanな31-sample macOS AArch64 runは両方の正確なprofile、44個すべての一意なtiming row、burst median 768.283 ns／946.601 ns／771.625 nsを記録します。TestとWindows CIは正確な値とscopeを強制します。これは証拠を過大評価せず、最初のlogical heap-lifetime measurement gapを閉じます。Schema 13は最初のlocal process envelopeを提供しましたが、allocator metadata／fragmentation、VM／JIT内部allocation、代表的application allocation、remote Windows evidenceは未完了です。

Schema 13 harnessはschema 12 fieldをすべて維持し、allocationを行わないlink済みAOT harnessに対するprocess-level peak resident-memory distributionを追加します。各sampleは別processを起動します。macOSは `/usr/bin/time -l` 経由で `ru_maxrss`、WindowsはPowerShell経由で `Process.PeakWorkingSet64` を報告します。両方ともbytes単位のOS固有process envelopeであり、shared pageとloader／runtime stateを含みます。Unit testとnative smoke runがmacOS parser／distributionを検証し、Windows CIは正で順序付けされた31-sample rowを要求します。これはallocator-overheadや代表的workload evidenceを主張せず、最初のprocess-level peak-memory mechanismを実装します。過去のclean schema 13 macOS AArch64 resultは31 sampleすべてが1,671,168 byte、RSD 0.000%であることを記録し、Windows baselineは未記録です。

Schema 14 harnessはschema 13の全fieldを維持し、allocation-churn callbackのdirect Rust／VM／JIT timing rowと正確なrequested-payload heap profileを追加します。決定的sequenceは64個のboxed `i64` valueをallocateし、1つおきのallocationを解放し、32個の穴をreplacementで埋めてから、最後の64個をconsumeして、`(96 allocations, 768 total bytes, 512 peak live bytes, 0 live bytes afterward)` を生成します。VM／JIT差分testはwrapping resultを扱い、macOS AArch64 direct／response-file integrationは対応するC `malloc`／`free` importを実行し、host非依存Windows COFF validationは `lang_host_i64_3` を検証し、cfg-gated MSVC integrationとWindows workflowはcontractを維持します。Cleanな31-sample macOS AArch64 runは47個すべての一意なtiming row、3つすべての正確なheap profile、direct Rust／VM／JIT churn medianの1.192 us／1.348 us／1.173 us、1,654,784〜1,671,168 byteのlink済みprocess peak memoryを記録します。Variable-size／代表的workload、allocator overhead／fragmentation、remote Windows executionは未完了です。

未benchmarkのvariable-size allocation callbackは8〜128 payload byteの16個のboxed `i64` sliceを保持し、要求した1,088 payload byteを同時にliveにして、決定的なwrapping foldを返します。VM／JIT差分testはinteger extremeを扱い、macOS AArch64 direct／response-file AOT integrationは対応するC importを実行し、host非依存Windows COFF validationは `lang_host_i64_4` を検証し、cfg-gated MSVC pathも同じlink-and-run coverageを再現します。Timingと正確なrequested-payload profiling、代表的allocation workload、allocator overhead／fragmentation、remote Windows executionは未完了です。

CleanなmacOS AArch64の[AOT debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)では、commit `013da31f9f0fb752aad5ec887f8cf5a5ecf1650d` からadd、minimum、host-callback objectを再生成しました。Linker-visible symbolは残りますが、3 objectすべてに `__debug_*` section、DWARF compile unit、line entry、source variable、parameter、localはありません。これはcriterionを完了するものではなく、負のdebug-quality evidenceを提供します。Source-span mapping、link済み／dSYM output、unwind metadata、Windows CodeView／PDB、JIT debugger registration、optimized stepping、代表的source programでのvalidationは未完了です。

Exit：prototype subsetの意味論が文書化され、初期backend選定を支持するbenchmark evidenceがある。

## Milestone 1：frontendとbytecodeのvertical slice

**進捗：0 / 52 pt（0.0%）。** Milestone 0のprototypeはgroundworkであり、これらのtaskには完全なsource-driven vertical sliceが必要です。

- [ ] **P0 — Error recoveryとsource spanを持つlossless parserを実装する。** — 8 pt
- [ ] **P0 — Name resolution、module、primitive type、function、record、variant、pattern matchingを実装する。** — 13 pt
- [ ] **P0 — Stable codeとfix-itを持つstructured diagnosticsを追加する。** — 5 pt
- [ ] **P0 — Source semanticsをvalidation付きtyped MIRへ統合する。** — 5 pt
- [ ] **P0 — Source program向けregister bytecode、verifier、VM、REPL／script runner、host-function registrationを完成させる。** — 8 pt
- [ ] **P1 — Formatterとparser／formatter round-trip testを実装する。** — 5 pt
- [ ] **P1 — LSP prototypeと共有する初期incremental query graphを構築する。** — 8 pt

Exit：小さなprogramをscriptとして実行でき、deterministic behaviorと有用なIDE diagnosticsがある。

## Milestone 2：ownershipとnative AOT

**進捗：0 / 53 pt（0.0%）。**

- [ ] **P0 — Owned value、move、lexical borrow、deterministic destruction、狭いunsafe operationを実装する。** — 13 pt
- [ ] **P0 — C ABI import／exportとgenerated headerを実装する。** — 8 pt
- [ ] **P0 — macOS AArch64／Windows x86-64向けCranelift object generationを完成させる。** — 8 pt
- [ ] **P0 — 代表的なVM／native differential execution suiteを構築する。** — 8 pt
- [ ] **P1 — Allocator-aware standard containerを実装する。** — 8 pt
- [ ] **P1 — Breakpointとstack traceに十分なnative debug metadataを提供する。** — 8 pt

Exit：代表的なcommand-line programがVM semanticsと一致し、同等のRust／C実装に対してperformanceが計測されている。

## Milestone 3：concurrencyとembedding

**進捗：0 / 55 pt（0.0%）。**

- [ ] **P0 — Structured task、cancellation、channel、actor isolation、data-race capability ruleを実装する。** — 13 pt
- [ ] **P0 — C embedding APIとgenerated bindingをstable化する。** — 8 pt
- [ ] **P0 — Version付きcomponent IDL、resource handle、capability importを実装する。** — 13 pt
- [ ] **P0 — Mixed native／VM call、callback、error、stack trace、cancellationをsupportする。** — 8 pt
- [ ] **P0 — Script resource limitとinterruptionを強制する。** — 8 pt
- [ ] **P1 — Trusted-native／verified-script plugin exampleを提供する。** — 5 pt

Exit：Native hostがuser scriptをloadし、typed APIを公開し、scriptをcallし、callbackを受け取り、IPCなしでcapabilityを強制できる。

## Milestone 4：developer toolchain

**進捗：0 / 58 pt（0.0%）。**

- [ ] **P0 — `lang` build／package／test／fmt／lint／doc commandを実装する。** — 8 pt
- [ ] **P0 — Deterministic resolution、checksum、offline mode、target conditionを持つmanifest／lockfileを実装する。** — 13 pt
- [ ] **P0 — LSPのnavigation、completion、rename、semantic token、code actionを完成させる。** — 13 pt
- [ ] **P0 — Mixed native／VM stackを持つDAP adapterを実装する。** — 8 pt
- [ ] **P1 — Compile-time／runtime profiling、allocation trace、rebuild explanationを追加する。** — 8 pt
- [ ] **P1 — `rustup` と同じ役割を持つinstall／update／channel toolを構築する。** — 8 pt

Exit：新しいdeveloperが1つのtoolをinstallし、packageを作成し、editorで開発し、test／debugし、reproducible binaryを生成できる。

## Milestone 5：ecosystemとcompatibility

**進捗：0 / 55 pt（0.0%）。**

- [ ] **P0 — Edition／deprecation policyを定義し、migration／fix toolとcompatibility matrixを構築する。** — 13 pt
- [ ] **P0 — Upload受付前にpublic-registry threat modelを完成させる。** — 8 pt
- [ ] **P0 — Signed release manifestとpackage provenanceを実装する。** — 8 pt
- [ ] **P0 — Sandboxed portable plugin向けWebAssembly component targetを追加する。** — 13 pt
- [ ] **P1 — Documentation hostingとpackage-metadata searchを提供する。** — 5 pt
- [ ] **P1 — 公開benchmarkとdebug-quality criteriaを使ってLLVM release backendを評価する。** — 8 pt

Exit：Source、package、bytecode、component consumerを暗黙的に破壊せず言語を進化させられる。

## Milestone 6：セルフホスティング

**進捗：0 / 50 pt（0.0%）。**

- [ ] **P0 — Self-hosting subsetを定義してfreezeする。** — 5 pt
- [ ] **P0 — 新言語でfrontendとcore compiler stageを実装する。** — 21 pt
- [ ] **P0 — Stage 0 Rust compilerでstage 1をbuildし、stage 1でstage 2をbuildできるようにする。** — 13 pt
- [ ] **P0 — 両tier-1 platformでreproducible outputを比較する。** — 8 pt
- [ ] **P1 — Bootstrap trustとdisaster-recovery procedureを文書化する。** — 3 pt

Exit：Stage 2がstage 0と同じconformance suiteに合格し、文書化されたbootstrap chainからsupport対象toolchainをbuildできる。

## Milestone 7：追加frontend

**進捗：0 / 39 pt（0.0%）。**

- [ ] **P0 — Library、profile、syntax sugarでは満たせない要件を、別のhigh-level／domain-specific frontendが提供することを実証する。** — 5 pt
- [ ] **P0 — Frontend-specific HIRを実装し、coreが意味論を忠実に表現できる場合だけshared MIRへloweringする。** — 13 pt
- [ ] **P0 — Memory、effect、runtime modelが異なる場合はcomponent ABIを使用する。** — 8 pt
- [ ] **P0 — Interoperabilityをstableと宣言する前に、cross-language debugging、package metadata、conformance testを追加する。** — 13 pt

Exit：2つ目のfrontendが明確なuser valueを追加し、最初の言語の安全性を弱めず、shared IRを無関係な意味論の保守不能な集合にしない。

