<!-- translation-of: ../roadmap.md; canonical-language: en -->

# 実装ロードマップ

[English](../roadmap.md)

このロードマップはrisk reductionの順に並んでいます。Milestoneはcalendar commitmentではなくexit criteriaです。

## タスク管理

各milestone内のtaskはdependencyを考慮した優先度順に並べます。すべての `P0` taskがcheck済みで、exit evidenceが記録されるまでmilestoneはexitできません。未完了の `P1` taskはexit前に移動先とcostを明示的に再設定し、暗黙に消してはいけません。Check済みtaskは完了です。前のmilestoneで完了したprototype groundworkだけでは、後続milestoneのより広いtaskを完了扱いにしません。

Costは経過時間の約束ではなく相対的なeffort pointです。`1`はごく小さい変更、`2`は小、`3`は中、`5`は大、`8`は非常に大です。未完了taskはそれぞれ独立して検証可能な1つの成果を持ち、それより大きなprogramはdependencyまたはacceptance boundaryで分割します。進捗は `完了point / 全point × 100` を小数第1位へ丸めて算出します。長期見積りはplanning valueであり、そのmilestoneがcurrentになったとき、またはscopeやevidenceが変わったときに再見積りします。

2026-10-16のroadmap全体監査で、3つの構造的問題を修正しました。Source frontendを必要とするproduction作業がdesign baselineへ誤って含まれていたこと、複数の独立してship可能な成果を1つにまとめたtaskがあったこと、後続milestoneで必要なregistry、bootstrap、interoperability作業が欠けていたことです。改訂後のlistは、その監査時点で完了済みだったevidence 80 ptを維持し、production hardeningを依存先のmilestoneへ移し、大きな複合項目を1 taskとして扱わず、隠れていた作業を明示しました。Component workstreamのfollow-up監査では、in-memory compatibility negotiation、scalar execution-prototype integration、backend-neutral resource-adapter behavior、boundary-cost measurementを分離しました。従来の単一cross-mode scalar／resource taskはこれらのacceptance boundaryを混在させ、Milestone 3のproduction IDL／adapter implementationとも重複していました。Schema 18後の監査では、native debug-metadata実装とcross-platform debug-quality validationがbackend選定のprerequisiteとして本文にだけ記載されていることを発見したため、backend選定前の5 pt／3 pt taskとして明示しました。Repository ownerのacceptanceを必要とするdecision taskは未完了のままevidence prerequisiteの後ろへ配置します。Checklistはdependency順を記録するものであり、外部gateをlocalで実行可能とみなすものではありません。Owner decisionまたはremote runnerが利用できない場合は、優先度を変更せず次のunblocked P0へ進みます。

同日のRFC readiness監査では、別のplanning gapも修正しました。RFC 0003はowner decision待ちとして並んでいましたが、review outcomeはstable serialization、host-value／cross-mode adapter conformance、lock integration、native Windows component execution、より広いcost evidenceをblockerとして明記しています。これらの成果をRFC 0003 decision前の明示的なprototype taskとして追加し、production IDL、adapter、package managerは引き続きMilestone 3／4の責務とします。

このrepository stateに対して2026-10-16 20:56 JSTに確認した進捗snapshot：

| Milestone | 完了 | 総計 | 進捗 |
| --- | ---: | ---: | ---: |
| 0 — design baseline（現在） | 145 pt | 196 pt | **74.0%** |
| 1 — frontendとbytecodeのvertical slice | 0 pt | 65 pt | 0.0% |
| 2 — ownershipとnative AOT | 0 pt | 74 pt | 0.0% |
| 3 — concurrencyとembedding | 0 pt | 86 pt | 0.0% |
| 4 — developer toolchain | 0 pt | 63 pt | 0.0% |
| 5 — ecosystemとcompatibility | 0 pt | 65 pt | 0.0% |
| 6 — セルフホスティング | 0 pt | 74 pt | 0.0% |
| 7 — 追加frontend | 0 pt | 55 pt | 0.0% |
| **全体** | **145 pt** | **678 pt** | **21.4%** |

## Milestone 0：設計baseline

**進捗：145 / 196 pt（74.0%）。**

### Workstream別内訳

この表はMilestone 0の全checklist itemをそれぞれ1回だけ集計したreporting viewであり、scopeを追加するものではありません。

| Workstream | 完了 | 総計 | 進捗 |
| --- | ---: | ---: | ---: |
| Project framingとsource semantics | 8 pt | 10 pt | 80.0% |
| Cross-mode compiler／runtime semantics | 21 pt | 21 pt | 100.0% |
| Resource control、ownership、concurrency | 26 pt | 30 pt | 86.7% |
| Component／resource contract | 42 pt | 71 pt | 59.2% |
| Platform／backend evidence | 48 pt | 62 pt | 77.4% |
| Architecture-baseline disposition | 0 pt | 2 pt | 0.0% |
| **Milestone 0** | **145 pt** | **196 pt** | **74.0%** |

このsnapshot時点の実行状態：

- **Unblocked local P0：** RFC 0003のstable versioned component serialization、cross-mode host-value adapter conformance、lockfile／package-selection prototype、拡張boundary-cost evidence。
- **外部gate：** RFC 0002、RFC 0004、RFC 0005のrepository-owner decisionと、remote Windows object-link-run、benchmark、component-execution evidence。
- **依存gate：** 追加したevidence prerequisiteの完了後にRFC 0003をdecisionし、Windows evidenceの記録後に初期native backendを選定し、その他すべてのprerequisite完了後にRFC 0001をdecisionする。

### 優先度順のタスク

- [x] **P0 — Project charter、goal／non-goal、terminology、RFC process、compatibility labelを草案化する。** — 3 pt
- [x] **P0 — Typed `i64` MIR → bytecode／VM vertical sliceを構築してverifyする。** — 5 pt
- [x] **P0 — 同じtyped MIRをCranelift JITとhost-native object emissionへloweringする。** — 5 pt
- [x] **P0 — Typed host registration／callbackをVM、JIT、AOT object contract間でvalidateする。** — 3 pt
- [x] **P0 — Typed CFGのreachability、dominance、branch、back-edgeを各execution modeでvalidateする。** — 5 pt
- [x] **P0 — Typed block argument／loop valueをMIR、bytecode、VM、JIT、AOT間で一貫して運ぶ。** — 3 pt
- [x] **P0 — 共通linker request、tier-1 target policy、Windows MSVC workflow、host非依存COFF contractを確立する。** — 5 pt
- [x] **P0 — macOS AArch64のlink／run evidenceを記録し、direct／response-file pathを実行する。** — 3 pt
- [x] **P0 — Deterministic VM instruction、host-call、value-slot limitをprototypeする。** — 5 pt
- [x] **P0 — Cooperative interruptionとabsolute execution deadlineをprototypeする。** — 3 pt
- [x] **P0 — Version付きscalar component metadataと明示的にcapability-gatedなimportをtyped host registryに対してvalidateする。** — 5 pt
- [x] **P0 — Generation tag付きowned／borrowed resource handleとdeterministic lifecycle validationをprototypeする。** — 3 pt
- [x] **P0 — Strict／less-restrictiveなasync-borrow／cancellation policyをprototypeする。** — 3 pt
- [x] **P0 — Cyclic graph ownershipのstrict／tracing alternativeをprototypeする。** — 3 pt
- [x] **P0 — Shared-serviceのowner-isolated／exclusive-capability policyをprototypeする。** — 2 pt
- [x] **P0 — RFC 0002にdecision-readyなsource-level integer／structured-control-flow semanticsを草案化する。** — 5 pt
- [x] **P0 — Version付きbenchmark harnessを構築し、compile、runtime、startup、artifact-size baselineを記録する。** — 8 pt
- [x] **P0 — Deterministic logical-heap profileとlinked-process memory baselineを記録する。** — 5 pt
- [x] **P0 — Variable-size host allocationをdirect Rust、VM、JIT、macOS AOT、Windows object contractで実行する。** — 3 pt
- [x] **P0 — AArch64 generated-code inspectionと、現在のnegative native debug-information baselineを記録する。** — 3 pt
- [ ] **P0 — RFC 0002を判断し、acceptance、修正要求、rejectionのいずれかを記録する。** — 2 pt
- [ ] **P0 — Windows x86-64 CIでobject-link-run成功evidenceを記録する。** — 3 pt
- [ ] **P0 — Clean Windows benchmark baselineを記録し、macOS baselineと比較して解釈する。** — 5 pt
- [x] **P0 — Byte／count／depth／string limit、location-bearing diagnostics、malformed fixture、fuzz targetを持つbounded schema-1 component encoder／decoderをprototypeする。** — 5 pt
- [x] **P0 — RFC 0003でcanonical scalar／resource representationとownership transferを規定する。** — 5 pt
- [x] **P0 — RFC 0003でcomponent boundaryのpanic／destructor failure behaviorを規定する。** — 3 pt
- [x] **P0 — RFC 0003でpackage／interface version compatibility negotiationを規定する。** — 3 pt
- [x] **P0 — In-memory candidate setとexact lock inputに対するdeterministicなpackage／interface negotiationをprototypeする。** — 5 pt
- [x] **P0 — 1つのvalidated schema-1 scalar import／exportをVM／JIT／AOT prototypeへ接続し、同等のoutcomeを記録する。** — 3 pt
- [x] **P0 — Canonical resourceのpreflight／commit、borrow scoping、cleanup ledger、instance poisoningをbackend-neutralなadapter traceでprototypeする。** — 5 pt
- [x] **P0 — Metadata validation／negotiation、scalar dispatch、resource lifecycleのcostを別々のbenchmark seriesとして計測し、その解釈をRFC 0003へ記録する。** — 5 pt
- [ ] **P0 — Normalized requirement、digest、resource identity、failure recordを対象とするstable versioned component serializationをprototypeし、規定する。** — 8 pt
- [ ] **P0 — General component instantiationをprototypeし、canonical host-value resource／failure adapterをVM、JIT、AOTへ接続して同等のoutcomeを記録する。** — 8 pt
- [ ] **P0 — Production package managerを実装せず、in-memory resolverに対するdeterministicなlockfile／package-selection integrationをprototypeする。** — 5 pt
- [ ] **P0 — 同じconformance fixtureのnative Windows component executionを記録する。** — 3 pt
- [ ] **P0 — Component-boundary cost evidenceをinstantiation、failure／destructor path、memory、code sizeへ拡張し、その解釈をRFC 0003へ記録する。** — 5 pt
- [ ] **P0 — RFC 0003を判断し、acceptance、修正要求、rejectionのいずれかを記録する。** — 3 pt
- [x] **P0 — 完了済みexperimentからdecision-readyなownership／cyclic-graph RFCを草案化する。** — 5 pt
- [x] **P0 — 完了済みexperimentからdecision-readyなasync-cancellation／shared-service concurrency RFCを草案化する。** — 5 pt
- [ ] **P0 — Ownership／cyclic-graph RFCを判断する。** — 2 pt
- [ ] **P0 — Async-cancellation／shared-service concurrency RFCを判断する。** — 2 pt
- [x] **P0 — Exact requested payloadを含め、variable-size allocation callbackを計時／profileする。** — 5 pt
- [x] **P0 — Allocator overhead／fragmentationを含む代表的なallocation workloadを計測する。** — 5 pt
- [x] **P0 — Cross-mode benchmark setへ代表的なCFG workloadをもう1つ追加する。** — 3 pt
- [x] **P0 — 代表的なAOT object／link済みartifactにsource span駆動のnative debug metadataを実装し、macOS AArch64で検証する。** — 5 pt
- [x] **P0 — 同じ代表artifactについて、同等のWindows x86-64 CodeView／PDB／source-mapping evidenceを記録する。** — 3 pt
- [ ] **P0 — 完成したcross-platform performance、debug-quality、integration evidenceから初期native backendを選択し、decisionを記録する。** — 3 pt
- [ ] **P0 — Component、resource-control、backend-evidence prerequisiteの完了後にRFC 0001を判断し、acceptance、修正要求、rejectionのいずれかを記録する。** — 2 pt

### Evidenceと現在の状態

最初のcomponent-contract sliceはportable declarationをembedding-local bindingから分離しました。Schema-1 in-memory metadataはpackage identity／versionと名前付きscalar import／exportを記録します。各importがcapabilityを宣言し、別のbinding tableが論理nameをprocess-localな `HostFunctionId` へmapします。Validationはunknown schema、invalid／duplicate name、import／export collision、unsupported import type、undeclared／missing binding、grant不足、unregistered host functionを拒否します。Unauthorized componentがhost inventoryをprobeできないようcapability checkをregistry lookupより前に行い、成功時にはresolved bindingを持つ別のwrapperを返します。7件のruntime testがaccepted pathと主要なrejection／ordering contractをcoverします。

実験的schema-1 `LCMP` codecはscalar declaration subsetをdeterministically encodeし、設定可能なtotal-byte、declaration-count、structural-depth、string-length limitの下でuntrusted byteをdecodeします。Malformed magic、payload前のunsupported schema、truncation、invalid UTF-8、unknown scalar tag、trailing byte、limit超過をstructured byte-offset errorで拒否します。Round-trip、encoder／decoder limit、malformed fixture、deterministic arbitrary-byte corpus、locked fuzz-target build、1,000-run libFuzzer smoke executionがpassしました。このformatはunstableなfixtureのままであり、stable contract serialization／compatibility negotiationはRFC 0003の未解決事項です。

Component runtimeはgeneration tag付きowned handleと明示的なshared／exclusive borrow tokenもprototypeしました。Released slotはgenerationを進め、最小のfree slotをdeterministically再利用するため、stale ownerはreplacementをaliasできず、generation exhaustionではwrapせずslotをretireします。Shared borrowは共存し、exclusive borrow／mutable accessはexclusivityを強制し、active borrowはreleaseをblockし、borrow end／releaseのrepeatはstructuredに失敗します。6件のfocused testがslot reuse、stale owner／borrow、double release／end、conflict、mutation、deterministic reuse、generation retirement、borrow-ID exhaustionをcoverします。

[RFC 0003](rfcs/0003-versioned-component-contracts.md)は最初のnormal-path canonical call subsetを規定しました。64-bit `i64`、32-bit normalized `bool`、per-instance unsigned 32-bit adapter indexで表す `own<T>`／`borrow<T>` resourceです。Index zeroはinvalidで、boundary indexをinstance内で再利用しないため、internal generation tag付きslotを再利用してもstale numeric copyが後続resourceをaliasしません。Whole-list preflightはownership mutationより前に、stale、wrong-type、duplicate-owned、owned／borrowed alias、active-lend、capacity、reservation、index-exhaustion failureを拒否します。成功するowner-transfer commitはallocation、callback、観測可能な中間stepを一切行わず、freshなdestination ownerをinstallしてsource indexを永続的にstaleにします。Borrowはcall-scopedのままで、borrowed resultは除外します。このproposalはproject decisionとWIT／Canonical ABI source factを分離し、RFC statusをproposedのまま維持します。

同じRFCはRust／native unwindを公開せず、post-commit failure behaviorも規定しました。Panic／通常trapはownershipをrollbackしません。明示的cleanup ledgerはnested borrowをcloseしてから、残るframe-owned resourceをacquisitionと逆順でdestroyします。Destructorはuser codeより前にownerをconsumeし、at most onceで実行し、failure後もdeterministicなbest-effort cleanupを続けます。先行panic／trapをprimaryに保ち、destructor failureをsuppressed順で追加します。Cleanなcomponent failureではrevalidate済みinstanceをreuseできますが、destructor、contain済みhost-callback、invariant、incomplete-cleanup failureはinstanceをpoisonします。Designはstableなlogical diagnostic identityを記録し、cleanup reentrancyを禁止し、process abort／foreign unwindをtrusted-native limitationとして扱い、VM／JIT／AOTの同一outcomeを要求します。WebAssembly Canonical ABI sourceは2026-10-13最終変更です。Rust公式のpanic、`catch_unwind`、FFI-unwinding documentationは更新日の記載がなく、2026-10-16に確認しました。Failure-record encoding、host-value／cross-mode adapter conformance、stable serializationは未完了です。

Package／interface negotiationはmetadata-schema selection／package-manager resolutionから分離して規定しています。Runtimeは現在、そのmodel全体をin-memoryでprototypeします。Normalized full-SemVer valueはprerelease／build identifierを保持し、exact requirementはbuild metadataを含み、bounded intervalはSemVer precedenceで比較し、prereleaseには明示的opt-inを要求します。Capability grantをprovider inventory validationより先にcheckし、exact lockは指定されたidentity、version、opaque digestをfallbackなしで要求します。Unlocked candidateはexact package／interface identity、version、structural projection checkを通過してからdeterministicにhighest-precedenceを選択します。Projection checkは順序付きscalar type、nominalな `own`／`borrow` resource type、exact capabilityを扱い、providerのadditive memberを許可し、rejectionをdeterministicな順序で報告します。Same-precedenceのbuild／digest tieはambiguity errorで、結果はimmutable validated bindingです。14個のfocused testがordering、exact lock、non-fallback、structural fallback／rejection、capability ordering、resource ownership mode、ambiguity、malformed declarationを検証します。これは新metadata schema、stable digest encoding、package-manager integration、VM／JIT／AOT instantiation pathではなく、引き続きin-memory prototypeです。

1つの実行可能なscalar conformance fixtureは、schema-1 metadata、capability grant、logical import bindingから開始し、typed host registryに対してvalidateし、得られたimmutable contractからMIR host callとexport symbolを導出します。同じfunctionはinteger extremeを含むwrapping-double caseを実行し、VMとCranelift JITで同等のoutcomeになります。Validated fixtureはhost-native objectもemitし、その `run` exportとresolved host importをmacOS AArch64でdirect／response-file linker pathの両方からlink／実行して同じ結果を生成します。これは1つのscalar import／export接続を実証しますが、general component instantiation、non-scalar value、resource、failure cleanup、stable serialization、remote Windows executionを実装するものではありません。

Component runtimeは、stale copyを決してaliasしない非ゼロかつ単調増加のboundary indexを持つbackend-neutralなcanonical-resource adapter traceも公開します。Call全体のpreflightは、handleを公開したりownershipを変更したりせずcommit storageを予約しながら、resource identity、ownership mode、duplicate／owned-borrow alias、active lend、poison state、destinationのlive／index capacityをvalidateします。Infallibleなcommit pathはallocationもcallbackも行いません。Dynamic borrowはchildからparent scopeの順に閉じ、frame-owned resourceはdestructor callback前にremoveしてacquisitionと逆順にcleanupし、独立したdestructor failureはdeterministicな順序で継続し、callback／invariant／destructor failureは以後のentryをpoisonします。7件のfocused testがfailed-preflight atomicity、alias rejection、indexを再利用しないtransfer／return、nested borrow order、post-commit non-rollback、ordered failureを伴うreverse cleanup、capacity rejection、poison enforcementをcoverします。これはopaqueなstate-machine traceのままであり、host valueやVM／JIT／AOT resource callには未接続で、stable failure encodingやcancellationも決定しません。

Cleanなschema-18 macOS AArch64 [CFG／allocation baseline](../../benchmarks/results/2026-10-16-macos-aarch64-b5ddb9b.json)は、56個の一意なtiming row、4個の正確なcallback profile、8個のcomponent-boundary row、3個のprocess-memory distributionを記録します。Iterative Fibonacciはcountdownと2個のwrapping sequence valueを型付きblock argumentで運び、direct Rust／VM／JIT medianは6.510 ns／1.231 us／7.463 ns、JIT codeは48 byte、AOT objectは400 byteです。維持されたallocation traceは18,102,042 peak live requested byte、21,020,672 byteのmedian resident delta、2,918,630 byteのallocator-overhead／fragmentation合算推定値を記録します。これらはperformance／memory budgetではなくplatform固有の方向的resultです。Remote Windows executionは未完了です。

現在の進捗：最初の型付き関数は、検証済みbytecodeとCranelift JIT prototypeの両方で実行でき、同じMIR loweringからhost-native object artifactを生成できます。最小の型付き `i64` host-function registration、通常のscript-to-host callback、allocation-backed callbackは両方のexecution modeで実行でき、object outputはcallbackをimportとして表現します。VMはopt-inのdeterministic instruction-fuel／host-call budget、allocation前のvalue-slot limit、cooperativeなone-way interruption flag、絶対monotonic deadlineを合成できるようになりました。明示的なbranch、jump、return terminatorを持つ型付きbasic blockをMIR、bytecode、VM、JIT、AOTで共有し、validatorがtargetの存在、reachability、SSA dominanceを強制し、fuel-bounded back-edge testがnon-terminationを検証します。型付きblock parameterとedge argumentはcountdownで1 value、bounded sumで2 value、iterative Fibonacciで3 valueを運び、VMとJITの結果が一致し、AOT object emissionも成功します。初期の共通 `SystemLinker` driverが順序付きlink requestを検証し、host toolchainのcommand flavorを選択してstructured failureを報告し、macOS AArch64 integrationはadd、minimum、Fibonacci、通常のhost-callback、host-allocation、accumulator、conditional-sum objectをこのdriver経由でlinkして実行します。現在のcleanなmacOS AArch64 [schema 18 benchmark baseline](benchmarks.md)は、56個のgeneral timing row、4個の正確なcallback heap profile、3個の別process memory distribution、code／object／executable size、startup／link row、個別にlabel付けした4個のcomponent-boundary cost seriesを記録します。Remote Windows execution／benchmark evidenceはまだ記録していません。3つすべてのmemory-model stress caseに実行可能な厳格modelがあります。すなわち、no-borrow-across-suspension ruleとdeterministic cancellation cleanup、明示的なshared／weak graphとarena-scoped cyclic graph、moved messageを使うactor-isolated shared mutable serviceです。Generated-code inspectionでは、2 instructionのAArch64 add、compare-and-branch minimum、host callback用のposition-independentなGOT importを確認しました。Source-mapped DWARF／dSYMとCodeView／PDB evidenceは、代表的な両tier-1 artifactをcoverするようになりました。RFC 0001はproposedのままであり、より制限の少ない代替案、より広いembedding limitとnative enforcement、production linker hardeningとstartup／runtime packaging、Windows x86-64でのlink、execution、benchmark、CI evidence、source allocation semantics、提案済みsource-level control-flow semanticsのacceptance／implementation、source-language local、unwind metadata、optimized stepping、JIT debugger registration、backend決定は未完了です。

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

Cyclic-graph実験は、defaultの `Reject` shared-cycle policyと、明示的な `CollectUnreachable` 代替案を比較します。Collectorはstrong cycleを許可し、root dropごとに残るlive rootからstrong edgeをtraceし、いずれかのrootから到達できる間はcycleを維持し、最後のrootが消えると逆生成順に回収します。Weak edgeはtargetをretainしません。[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)は、この比較をdecision-readyなproposalへまとめます。すなわち、単一ownerのplain value、明示的なatomic `shared`／non-retaining `weak` reference、cycleを閉じるstrong edgeのmutation前拒否、意図的なcycle用のlexical arena、mandatory tracing collectorなしというpolicyです。正確なconvenience syntaxを暫定に保ちながら、failure atomicity、deterministic arena teardown、cross-mode equivalence、compatibility、diagnostic、trust boundary、alternative、段階的implementation、observable testを規定します。RFCはproposedのまま **ready for decision** を記録し、production cost benchmarkと将来の別名traced heapはaccepted behaviorではなくfollow-up workです。

Shared-service実験は、defaultの `OwnerIsolated` mutation policyと、明示的な `ExclusiveCapability` 代替案を比較します。代替案では、有効なhandleを持つ任意のtaskが単一のactive capabilityを保持している間にmutationでき、handleのないtaskと同時mutatorはstructured diagnosticでrejectされ、cancellationはnon-owner mutationの境界までcleanupを待ちます。これはlock-likeな競合designをacceptせずに実行可能にします。Read／write／atomic capability、capability derivation、scheduling fairness、priority inversion、poisoning、deadlock、reentrancy、panic handling、backpressure、VM／native trace、runtime costは未完了です。

Host-allocation boundary実験は、single-allocation callbackと、64個のbox／512 payload byteを同時に保持してから合計・dropするburst callbackから始まりました。VM／JIT差分testはRustの `Box` callbackを使用し、macOS AArch64 direct／response-file AOT integrationと構成済みWindows MSVC pathは、別objectがimportする対応するCの `malloc`／`free` callbackを使用します。Schema 8はsingle-allocation callbackのdirect Rust／VM／JIT runtime rowを追加し、schema 11はAOT link／startup setを変えずにburst用の同じ3 rowを追加します。Windows CIは6 rowすべてを検証します。Clean schema 11 macOS AArch64 baselineのsingle-allocation medianはdirect 12.272 ns、VM 190.573 ns、JIT 14.123 nsで、burst medianは782.624 ns、962.329 ns、786.270 nsです。Source allocation semantics、ownership、allocator／collector policy、memory limit、allocator-overhead／peak-memory accounting、より広い代表的workload、remote Windows evidence、cross-platform costは未完了です。

VM resource-control prototypeは、optionalなinstruction-fuel／host-call budget、value-slot limit、共有one-way interruption flag、絶対monotonic deadlineを合成できるようになりました。Instruction／terminatorと実行に入るcallbackはそれぞれのcounterを消費し、対応するside effectの前にlimitを確認し、検証済みregister countはregister-file allocation前に確認します。すでにsetされたinterruptionまたは期限切れdeadlineはそのallocation前に停止し、以降のconditionはinstruction、terminator、pre-callback safe pointで観測して、structured errorに両方の消費counterを含め、interruptionを優先します。Callback-triggered requestまたはdeadline overrunはそのcallback完了後に観測します。成功したlimit付きexecutionは両方の動的countと静的slot footprintを報告します。Zero host-call budgetはcallbackを抑止し、過小なslot limitはexecutionを拒否し、どちらもstructured errorを返します。既存convenience APIはunbounded、non-interruptible、deadlineなしのままです。Heap、recursion、task、callback内部のpreemption、byte単位accounting、native／JIT enforcementは未完了です。

[RFC 0002](rfcs/0002-prototype-control-flow.md)は、実装済みinteger／CFG prototypeへ提案中のsource-level interpretationを与えます。Wrapping `i64` addition、signed comparison、left-to-right evaluation、mutable local、structured `if`／`else`／`while`、明示的return、typed block argumentを介したlive valueのloweringを定義します。Focused RFCはowner decisionに向けてreadyですがproposedのままです。Parser、HIR、formatter、source diagnostics、source-to-MIR implementationは未完了です。

Schema 9 harnessは、既存VM countdown rowの隣にunset-flagの `vm_countdown_interruptible_call` rowを追加し、input sequence／bytecodeを変えずにincremental safe-point acquire-load costを直接計測できるようになりました。過去のclean schema 14 runは通常／interruption-enabled VM countdown medianとして1.611 us／1.641 usを記録します。29.868 ns（1.854%）の増加はinterruption rowの15.866% RSDより小さいため、測定可能なregressionを示さず、overhead budgetも確立しません。Schema 18がこのrowを維持し、現在のCI contractです。Remote Windows resultはまだ記録していません。

Schema 10 harnessは、harness準備の1時間後に設定した絶対monotonic deadline付きで同じcountdown bytecode／inputを実行する `vm_countdown_deadline_call` を追加します。Deadlineが期限前のままのsteady-state clock-check costを分離します。過去のclean schema 14 macOS AArch64 runの通常／期限前deadline付きVM countdown medianは1.611 us／3.320 us、1.709 usまたは106.071%の増加でした。Schema 18／Windows CIはこのrowを維持しますが、remote Windows resultはまだ記録していません。

Schema 11 harnessは64-allocation burst用のdirect Rust／VM／JIT rowを追加します。過去のcleanな31-sample macOS AArch64 runは44行すべてが一意であることと782.624 ns／962.329 ns／786.270 nsのburst medianを記録し、macOS AOT／host非依存Windows COFF testは新callback importを扱います。Windows CIはschema 18で3 rowすべてを維持しますが、remote Windows resultはまだ記録していません。

Schema 12 harnessは44個のtiming rowを維持し、host allocation callbackに対する決定的なrequested-payload heap profileを追加します。Single workloadは1 allocation、total／peak live 8 byte、完了後0 byteを記録し、burstは64 allocation、total／peak live 512 byte、完了後0 byteを記録します。Cleanな31-sample macOS AArch64 runは両方の正確なprofile、44個すべての一意なtiming row、burst median 768.283 ns／946.601 ns／771.625 nsを記録します。TestとWindows CIは正確な値とscopeを強制します。これは証拠を過大評価せず、最初のlogical heap-lifetime measurement gapを閉じます。Schema 13は最初のlocal process envelopeを提供しましたが、allocator metadata／fragmentation、VM／JIT内部allocation、代表的application allocation、remote Windows evidenceは未完了です。

Schema 13 harnessはschema 12 fieldをすべて維持し、allocationを行わないlink済みAOT harnessに対するprocess-level peak resident-memory distributionを追加します。各sampleは別processを起動します。macOSは `/usr/bin/time -l` 経由で `ru_maxrss`、WindowsはPowerShell経由で `Process.PeakWorkingSet64` を報告します。両方ともbytes単位のOS固有process envelopeであり、shared pageとloader／runtime stateを含みます。Unit testとnative smoke runがmacOS parser／distributionを検証し、Windows CIは正で順序付けされた31-sample rowを要求します。これはallocator-overheadや代表的workload evidenceを主張せず、最初のprocess-level peak-memory mechanismを実装します。過去のclean schema 13 macOS AArch64 resultは31 sampleすべてが1,671,168 byte、RSD 0.000%であることを記録し、Windows baselineは未記録です。

Schema 14 harnessはschema 13の全fieldを維持し、allocation-churn callbackのdirect Rust／VM／JIT timing rowと正確なrequested-payload heap profileを追加します。決定的sequenceは64個のboxed `i64` valueをallocateし、1つおきのallocationを解放し、32個の穴をreplacementで埋めてから、最後の64個をconsumeして、`(96 allocations, 768 total bytes, 512 peak live bytes, 0 live bytes afterward)` を生成します。VM／JIT差分testはwrapping resultを扱い、macOS AArch64 direct／response-file integrationは対応するC `malloc`／`free` importを実行し、host非依存Windows COFF validationは `lang_host_i64_3` を検証し、cfg-gated MSVC integrationとWindows workflowはcontractを維持します。Cleanな31-sample macOS AArch64 runは47個すべての一意なtiming row、3つすべての正確なheap profile、direct Rust／VM／JIT churn medianの1.192 us／1.348 us／1.173 us、1,654,784〜1,671,168 byteのlink済みprocess peak memoryを記録します。Variable-size／代表的workload、allocator overhead／fragmentation、remote Windows executionは未完了です。

Schema 16 harnessはvariable-size allocation callbackのdirect Rust／VM／JIT timingを追加します。このcallbackは8〜128 payload byteの16個のboxed `i64` sliceを保持し、要求した1,088 payload byteを同時にliveにして、決定的なwrapping foldを返します。Cleanな31-sample macOS AArch64 runのmedianは215.438 ns／397.544 ns／218.453 nsで、正確なprofileは `(16 allocations, 1,088 total bytes, 1,088 peak live bytes, 0 live bytes afterward)` です。VM／JIT差分testはinteger extremeを扱い、macOS AArch64 direct／response-file AOT integrationは対応するC importを実行し、host非依存Windows COFF validationは `lang_host_i64_4` を検証し、cfg-gated MSVC pathとWindows workflowもcontractを維持します。

Schema 17 harnessはcompiler-likeなmixed-lifetime allocation traceと、正確なrequested-payload accounting、interleaved control／workload process-memory sampleを追加します。CleanなmacOS AArch64 resultは23,894 allocations、19,183,210 total requested byte、18,102,042 peak live requested byte、21,037,056 byteのmedian resident delta、2,935,014 byteのallocator-overhead／fragmentation合算推定値を記録します。Traceは12 size class、部分解放、異size replacement、重なり合うtransient bufferを実行します。Source-language heap、production allocator選定、分離可能なmetadata／fragmentation accountingを主張せずに、計画した代表的allocation evidenceを完了します。Remote Windows executionは未完了です。

Schema 18 harnessはschema 17の全fieldを維持し、iterative Fibonacciのlowering、JIT compilation、AOT emission、direct／VM／JIT runtime、artifact-size evidenceを追加します。Workloadは既存MIR operationを使い、3個のloop-carried valueをback-edgeで入れ替えます。CleanなmacOS AArch64 resultは56個すべての一意なtiming row、Fibonacci medianのdirect 6.510 ns／VM 1.231 us／JIT 7.463 ns、48-byte JIT body、400-byte object、17,168-byte link済みharnessを記録します。VM／JIT差分test、macOS direct／response-file execution、host非依存Windows COFF validation、cfg-gated MSVC path、schema 18 Windows CIがcontractを維持します。Source semanticsをacceptせずに、計画した追加CFG evidenceを完了します。Remote Windows executionは未完了です。

実装前のcleanなmacOS AArch64 [AOT debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)は、元のnegative baselineを確立しました。Commit済みの[source-mapped DWARF inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md)は、commit `31e182ac27e324048597a5ebd0fda4d2d4934395` から代表的なadd、minimum、host-callback objectを再生成します。構造を検証したsource-file、function-span、instructionごとのspan、terminator-span dataがbackend-neutralなJIT／AOT machine-code rangeとrelocation付きDWARF v4 compile-unit、subprogram、line-table dataまで到達します。3 objectすべてが `dwarfdump --verify` をpassし、direct／response-file executableがlink／実行され、生成した両dSYMもfixture pathと3 functionを保持したままverificationをpassします。Commit済みの[Windows x86-64 CodeView／PDB inspection](../../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md)は、commit `465b0d66ba6ea61a0d4b26201e6ab043f2f15f9d` から同じ代表COFF objectをemitし、C13 symbol、type、line、relocationをvalidateし、LLDでcross-target linkしてPE／PDB identityを検証し、LLDBで3 functionすべてをfixtureへ解決します。これらのcaptureにより、計画済みの2つのsource-mapping taskは完了しました。Native Windows runtime／CI／benchmark evidence、source-language local、unwind metadata、JIT debugger registration、optimized steppingは個別の作業です。

Exit：上記の全P0 taskがcheck済みであり、source semantics、component contract、ownership／cyclic-graph policy、async-cancellation／shared-service policyにevidence-backedなRFC dispositionがあり、macOS／Windows evidenceが記録済みの初期backend選定を支持し、RFC 0001がその結果となるarchitecture baselineを記録している。

## Milestone 1：frontendとbytecodeのvertical slice

**進捗：0 / 65 pt（0.0%）。** Milestone 0のprototypeはgroundworkであり、これらのtaskには完全なsource-driven vertical sliceが必要です。

- [ ] **P0 — Stable code、label、fix-itを持つstructured source diagnosticsを確立する。** — 5 pt
- [ ] **P0 — Error recoveryとsource spanを持つlossless parserを実装する。** — 8 pt
- [ ] **P0 — Scope、name resolution、module loadingを実装する。** — 5 pt
- [ ] **P0 — Primitive expression、function、structured control flowをtype-checkする。** — 8 pt
- [ ] **P0 — Record、variant、pattern、exhaustivenessをtype-checkする。** — 8 pt
- [ ] **P0 — Spanを保持しながらresolved／typed sourceをHIR経由でvalidated typed MIRへloweringする。** — 5 pt
- [ ] **P0 — Prototypeをencoding、decoding、verification付きversioned register bytecodeへ昇格させる。** — 5 pt
- [ ] **P0 — Typed host registration付きでsource-language subsetをVM実行する。** — 5 pt
- [ ] **P0 — Deterministicなscript／REPL entry pointを提供する。** — 3 pt
- [ ] **P1 — Formatterとparser／formatter round-trip testを実装する。** — 5 pt
- [ ] **P1 — LSP prototypeと共有する初期incremental query graphを構築する。** — 8 pt

Exit：小さなprogramをscriptとして実行でき、deterministic behaviorと有用なIDE diagnosticsがある。

## Milestone 2：ownershipとnative AOT

**進捗：0 / 74 pt（0.0%）。**

- [ ] **P0 — Typed HIR／MIRでowned value、move、lexical borrow checkingを実装する。** — 8 pt
- [ ] **P0 — Deterministic dropをelaborateし、return、error、panic pathのdestruction behaviorを定義する。** — 8 pt
- [ ] **P0 — 狭いunsafe boundaryを定義し、FFI validity ruleを強制する。** — 5 pt
- [ ] **P0 — C ABI import／exportとgenerated headerを実装する。** — 8 pt
- [ ] **P0 — macOS AArch64／Windows x86-64向けsource-program object generationを完成させる。** — 8 pt
- [ ] **P0 — Linker discovery、startup／runtime packaging、production error handlingをhardeningする。** — 5 pt
- [ ] **P0 — 代表的なVM／native differential conformance suiteを構築する。** — 8 pt
- [ ] **P0 — 代表的native workloadを同等のRust／C実装に対して計測する。** — 8 pt
- [ ] **P1 — Allocator-aware standard containerを実装する。** — 8 pt
- [ ] **P1 — 両tier-1 platformでsource-span native debug metadataをemit／validateする。** — 8 pt

Exit：代表的なcommand-line programがVM semanticsと一致し、同等のRust／C実装に対してperformanceが計測されている。

## Milestone 3：concurrencyとembedding

**進捗：0 / 86 pt（0.0%）。**

- [ ] **P0 — Structured task lifetimeとdeterministic cancellation cleanupを実装する。** — 8 pt
- [ ] **P0 — Bounded backpressure／cancellation behavior付きchannelを実装する。** — 5 pt
- [ ] **P0 — Actor isolationとaccept済みdata-race capability ruleを強制する。** — 8 pt
- [ ] **P0 — C embedding APIとgenerated bindingをstable化する。** — 8 pt
- [ ] **P0 — Version付きcomponent IDLとbounded serializationを実装する。** — 8 pt
- [ ] **P0 — Canonical owned／borrowed resource adapterを実装する。** — 8 pt
- [ ] **P0 — Capability-gated component importを強制する。** — 5 pt
- [ ] **P0 — Typed error付きmixed native／VM call／callbackをsupportする。** — 8 pt
- [ ] **P0 — Native／VM boundaryを越えてstack trace／cancellationを保持する。** — 5 pt
- [ ] **P0 — Host-callback accountingを含むheap／allocation limitを強制する。** — 5 pt
- [ ] **P0 — Recursion-depth／live-task limitを強制する。** — 5 pt
- [ ] **P0 — VM、JIT、AOT、callback boundary全体でcooperative interruptionを強制する。** — 8 pt
- [ ] **P1 — Trusted-native／verified-script plugin exampleを提供する。** — 5 pt

Exit：Native hostがuser scriptをloadし、typed APIを公開し、scriptをcallし、callbackを受け取り、IPCなしでcapabilityを強制できる。

## Milestone 4：developer toolchain

**進捗：0 / 63 pt（0.0%）。**

- [ ] **P0 — `lang` build、run、test orchestrationを実装する。** — 5 pt
- [ ] **P0 — Package／workspace manifestとtarget conditionを定義する。** — 5 pt
- [ ] **P0 — Deterministic dependency resolution、lockfile、checksum、offline modeを実装する。** — 8 pt
- [ ] **P0 — Package、fmt、lint、doc commandをCLIへ統合する。** — 5 pt
- [ ] **P0 — LSP navigation、completion、semantic tokenを実装する。** — 8 pt
- [ ] **P0 — 安全なrename／code actionを実装する。** — 8 pt
- [ ] **P0 — Mixed native／VM stackを持つDAP adapterを実装する。** — 8 pt
- [ ] **P1 — Compile-time／runtime profiling、allocation trace、rebuild explanationを追加する。** — 8 pt
- [ ] **P1 — `rustup` と同じ役割を持つinstall／update／channel toolを構築する。** — 8 pt

Exit：新しいdeveloperが1つのtoolをinstallし、packageを作成し、editorで開発し、test／debugし、reproducible binaryを生成できる。

## Milestone 5：ecosystemとcompatibility

**進捗：0 / 65 pt（0.0%）。**

- [ ] **P0 — Edition、deprecation policy、compatibility matrixを定義する。** — 5 pt
- [ ] **P0 — Edition migration／machine-applicable fix toolを構築する。** — 8 pt
- [ ] **P0 — Upload受付前にpublic-registry threat modelを完成させる。** — 8 pt
- [ ] **P0 — Registry package storage、indexing、download、immutable-version ruleを実装する。** — 5 pt
- [ ] **P0 — Authenticated publish／yank／owner operationとaudit trailを実装する。** — 5 pt
- [ ] **P0 — Signed release manifestとpackage provenanceを実装する。** — 8 pt
- [ ] **P0 — Portable plugin向けWebAssembly component targetを追加する。** — 8 pt
- [ ] **P0 — WebAssembly plugin sandbox／capability conformanceをvalidateする。** — 5 pt
- [ ] **P1 — Documentation hostingとpackage-metadata searchを提供する。** — 5 pt
- [ ] **P1 — 公開benchmarkとdebug-quality criteriaを使ってLLVM release backendを評価する。** — 8 pt

Exit：Source、package、bytecode、component consumerを暗黙的に破壊せず言語を進化させられる。

## Milestone 6：セルフホスティング

**進捗：0 / 74 pt（0.0%）。**

- [ ] **P0 — Self-hosting subsetを定義してfreezeする。** — 5 pt
- [ ] **P0 — 新言語でlossless syntax、parsing、diagnosticsを実装する。** — 8 pt
- [ ] **P0 — 新言語でresolution、type checking、typed HIRを実装する。** — 8 pt
- [ ] **P0 — 新言語でMIR lowering／verificationを実装する。** — 8 pt
- [ ] **P0 — 新言語で必要なbytecode／native code-generation pathを実装する。** — 8 pt
- [ ] **P0 — Bootstrapに必要な最小package／compiler driverを実装する。** — 5 pt
- [ ] **P0 — Stage 0 Rust compilerでstage 1をbuildする。** — 8 pt
- [ ] **P0 — 未追跡のstage 0 behaviorを使わず、stage 1でstage 2をbuildする。** — 8 pt
- [ ] **P0 — Stage 0、1、2に同じconformance suiteを実行する。** — 5 pt
- [ ] **P0 — 両tier-1 platformでreproducible outputを比較する。** — 8 pt
- [ ] **P1 — Bootstrap trustとdisaster-recovery procedureを文書化する。** — 3 pt

Exit：Stage 2がstage 0と同じconformance suiteに合格し、文書化されたbootstrap chainからsupport対象toolchainをbuildできる。

## Milestone 7：追加frontend

**進捗：0 / 55 pt（0.0%）。**

- [ ] **P0 — Library、profile、syntax sugarでは満たせない要件を別frontendが提供することを実証する。** — 5 pt
- [ ] **P0 — Frontend semanticsとshared-MIR／component boundaryを規定する。** — 5 pt
- [ ] **P0 — Frontend固有のparsing、HIR、semantic analysisを実装する。** — 8 pt
- [ ] **P0 — 忠実に表現できるsemanticsだけをshared typed MIRへloweringする。** — 8 pt
- [ ] **P0 — Memory、effect、runtime modelが異なる箇所にcomponent adapterを実装する。** — 8 pt
- [ ] **P0 — Cross-language package metadata／version resolutionを追加する。** — 5 pt
- [ ] **P0 — Differential cross-language conformance testを追加する。** — 8 pt
- [ ] **P0 — Cross-language stack trace、source mapping、debugger integrationを追加する。** — 8 pt

Exit：2つ目のfrontendが明確なuser valueを追加し、最初の言語の安全性を弱めず、shared IRを無関係な意味論の保守不能な集合にしない。

