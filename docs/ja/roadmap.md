<!-- translation-of: ../roadmap.md; canonical-language: en -->

# 実装ロードマップ

[English](../roadmap.md)

このロードマップはrisk reductionの順に並んでいます。Milestoneはcalendar commitmentではなくexit criteriaです。

## タスク管理

未規定のsource、ABI、package、serialized behaviorを導入する実装taskは、そのbehavior実装前に受理済みRFCを必要とします。Decision taskはownerのdisposition記録で完了し、設計が拒否された場合は依存実装を完了扱いにせず再計画します。各成果に応じたcompile-pass／fail、不正入力／resource-limit、VM／native差分testを実行します。非信頼環境へ公開する前にdecoder／verifierのCPU・allocation budgetが必要です。

各milestone内のtaskはdependencyを考慮した優先度順に並べます。すべての `P0` taskがcheck済みで、exit evidenceが記録されるまでmilestoneはexitできません。未完了の `P1` taskはexit前に移動先とcostを明示的に再設定し、暗黙に消してはいけません。Check済みtaskは完了です。前のmilestoneで完了したprototype groundworkだけでは、後続milestoneのより広いtaskを完了扱いにしません。

Costは経過時間の約束ではなく相対的なeffort pointです。`1`はごく小さい変更、`2`は小、`3`は中、`5`は大、`8`は非常に大です。未完了taskはそれぞれ独立して検証可能な1つの成果を持ち、それより大きなprogramはdependencyまたはacceptance boundaryで分割します。進捗は `完了point / 全point × 100` を小数第1位へ丸めて算出します。長期見積りはplanning valueであり、そのmilestoneがcurrentになったとき、またはscopeやevidenceが変わったときに再見積りします。

2026-10-16のroadmap全体監査で、3つの構造的問題を修正しました。Source frontendを必要とするproduction作業がdesign baselineへ誤って含まれていたこと、複数の独立してship可能な成果を1つにまとめたtaskがあったこと、後続milestoneで必要なregistry、bootstrap、interoperability作業が欠けていたことです。改訂後のlistは、その監査時点で完了済みだったevidence 80 ptを維持し、production hardeningを依存先のmilestoneへ移し、大きな複合項目を1 taskとして扱わず、隠れていた作業を明示しました。Component workstreamのfollow-up監査では、in-memory compatibility negotiation、scalar execution-prototype integration、backend-neutral resource-adapter behavior、boundary-cost measurementを分離しました。従来の単一cross-mode scalar／resource taskはこれらのacceptance boundaryを混在させ、Milestone 3のproduction IDL／adapter implementationとも重複していました。Schema 18後の監査では、native debug-metadata実装とcross-platform debug-quality validationがbackend選定のprerequisiteとして本文にだけ記載されていることを発見したため、backend選定前の5 pt／3 pt taskとして明示しました。Host-value follow-up監査では、host-value identity bridge、general instantiation、VM／JIT／AOT resource／failure conformanceという3つのacceptance boundaryをなお混在させていた別の8 pt項目を分割しました。分割後も元の8 pt総計を維持し、各成果を独立してcheckできるようにしました。Repository ownerのacceptanceを必要としたdecision taskは、ownerがacceptするまで未完了のままevidence prerequisiteの後ろへ配置しました。Checklistはdependency順を記録するものであり、外部gateをlocalで実行可能とみなすものではありません。Owner decisionまたはremote runnerが利用できない場合は、優先度を変更せず次のunblocked P0へ進みます。

同日のRFC readiness監査では、別のplanning gapも修正しました。RFC 0003はowner decision待ちとして並んでいましたが、review outcomeは当時stable serialization、host-value／cross-mode adapter conformance、lock integration、native Windows component execution、より広いcost evidenceをblockerとして明記していました。これらの成果はRFC 0003 decision前の明示的なprototype taskとなり、production IDL、adapter、package managerは引き続きMilestone 3／4の責務とします。

同日のnative evidence監査では、3 ptのWindows component項目が2つのacceptance boundary、すなわちlocalで検証可能なMSVC direct／response-file test integrationと、外部で観測するnative runner evidenceを混在させていることを確認しました。この項目を完了済み1 ptのimplementation taskと当時未完了だった2 ptのevidence taskへ分割し、総costを維持しながら、利用できないremote evidenceがlocal作業を隠さないようにしました。

続くRFC consistency監査では、RFC 0003だけに記載されていた2つのdecision blocker、すなわちshared MIR／backend contractを通るtyped resource loweringと、代表的multi-component application evidenceを確認しました。これらを8 pt／5 ptのMilestone 0 prototype taskとして明示します。Milestone 3のproduction component IDL／adapter implementationを置き換えるものではありません。Production package-manager I/OはMilestone 4の作業であり、RFC 0003 designを判断するprerequisiteではありません。

最初のtyped-resource slice後、8 ptのlowering taskを次のacceptance boundaryで分割し、総costを維持しました。完了済み3 pt taskはnominal MIR／bytecode typingとCranelift JIT／AOT machine laneをcoverし、2番目の5 pt taskはtyped VM value、adapter統合済みownership transfer／cleanup、native link／run conformanceをcoverします。

2番目のtyped-resource sliceは、規定したprototype boundaryで完了しました。Typed VM valueとadapter統合済みVM／JIT executionは、macOS／Windows AOT direct／response-file native link／run testとnominal owner laneを共有します。GitHub Actions run 38932964398が対応するWindows execution／cleanup outcomeを記録し、下記の独立した2 pt evidence taskを完了しました。

Representative-application taskも規定したprototype boundaryで完了しました。Host-orchestrated fixtureは2-provider negotiation、deterministic lock生成、exact-lock replay、instance publication、scalar／typed-resource VM／JIT call、guest-trap cleanupをcomposeします。Cleanなschema-20 macOS baselineは1,000 operationを31 sample実行し、median 67.592 us、p95 68.459 us、RSD 1.109%を記録しました。CleanなWindows baselineは同じrowでmedian 236.804 us、p95 266.366 us、RSD 6.201%を記録し、production generated dispatch stubだけが別作業として残ります。

2026-10-17のrepository reviewで、source ownership／core-library構文とproduction component profileの開始gate欠落を明確にし、既存copy-value profileのnative compilationをownership-aware AOTから分離しました。Milestone 2に未完了21 pt、Milestone 3に未完了5 ptを追加し、完了済み298 ptは維持します。全体進捗の低下は隠れていた作業の明示によるものです。Source／core-library基盤を代表的workload／self-hostingより先に整備し、C embeddingのstable化はconformance／resource enforcement確認後に行います。

このrepository stateに対して2026-10-17（JST）に確認した進捗snapshot：

| Milestone | 完了 | 総計 | 進捗 |
| --- | ---: | ---: | ---: |
| 0 — design baseline | 209 pt | 209 pt | **100.0%** |
| 1 — frontendとbytecodeのvertical slice（完了） | 89 pt | 89 pt | **100.0%** |
| 2 — ownershipとnative AOT | 21 pt | 113 pt | 18.6% |
| 3 — concurrencyとembedding | 0 pt | 96 pt | 0.0% |
| 4 — developer toolchain | 0 pt | 63 pt | 0.0% |
| 5 — ecosystemとcompatibility | 0 pt | 65 pt | 0.0% |
| 6 — セルフホスティング | 0 pt | 74 pt | 0.0% |
| 7 — 追加frontend | 0 pt | 55 pt | 0.0% |
| **全体** | **319 pt** | **764 pt** | **41.8%** |

## Milestone 0：設計baseline

**進捗：209 / 209 pt（100.0%）。**

### Workstream別内訳

この表はMilestone 0の全checklist itemをそれぞれ1回だけ集計したreporting viewであり、scopeを追加するものではありません。

| Workstream | 完了 | 総計 | 進捗 |
| --- | ---: | ---: | ---: |
| Project framingとsource semantics | 10 pt | 10 pt | 100.0% |
| Cross-mode compiler／runtime semantics | 21 pt | 21 pt | 100.0% |
| Resource control、ownership、concurrency | 30 pt | 30 pt | 100.0% |
| Component／resource contract | 84 pt | 84 pt | 100.0% |
| Platform／backend evidence | 62 pt | 62 pt | 100.0% |
| Architecture-baseline disposition | 2 pt | 2 pt | 100.0% |
| **Milestone 0** | **209 pt** | **209 pt** | **100.0%** |

このsnapshot時点の完了状態：

- **完了済みdisposition：** Repository ownerは2026-10-17にRFC 0003をacceptし、続けてRFC 0001をacceptしました。RFC 0002、RFC 0004、RFC 0005も同日にaccept済みです。
- **完了済みarchitecture decision：** Cranelift 0.136.3をinitial native JIT／AOT backendとします。Frontend／artifact contractはbackend-neutralのまま維持し、後続release evidenceが正当化するまでLLVMをdeferします。
- **Milestone 1 exit：** 全P0／P1成果が完了し、[機能ガイド](milestone-1.md)へ利用方法、evidence、残る制限を記録しました。次の実装scopeはMilestone 2です。

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
- [x] **P0 — RFC 0002を判断し、acceptance、修正要求、rejectionのいずれかを記録する。** — 2 pt
- [x] **P0 — Windows x86-64 CIでobject-link-run成功evidenceを記録する。** — 3 pt
- [x] **P0 — Clean Windows benchmark baselineを記録し、macOS baselineと比較して解釈する。** — 5 pt
- [x] **P0 — Byte／count／depth／string limit、location-bearing diagnostics、malformed fixture、fuzz targetを持つbounded schema-1 component encoder／decoderをprototypeする。** — 5 pt
- [x] **P0 — RFC 0003でcanonical scalar／resource representationとownership transferを規定する。** — 5 pt
- [x] **P0 — RFC 0003でcomponent boundaryのpanic／destructor failure behaviorを規定する。** — 3 pt
- [x] **P0 — RFC 0003でpackage／interface version compatibility negotiationを規定する。** — 3 pt
- [x] **P0 — In-memory candidate setとexact lock inputに対するdeterministicなpackage／interface negotiationをprototypeする。** — 5 pt
- [x] **P0 — 1つのvalidated schema-1 scalar import／exportをVM／JIT／AOT prototypeへ接続し、同等のoutcomeを記録する。** — 3 pt
- [x] **P0 — Canonical resourceのpreflight／commit、borrow scoping、cleanup ledger、instance poisoningをbackend-neutralなadapter traceでprototypeする。** — 5 pt
- [x] **P0 — Metadata validation／negotiation、scalar dispatch、resource lifecycleのcostを別々のbenchmark seriesとして計測し、その解釈をRFC 0003へ記録する。** — 5 pt
- [x] **P0 — Normalized requirement、digest、resource identity、failure recordを対象とするstable versioned component serializationをprototypeし、規定する。** — 8 pt
- [x] **P0 — Opaqueなcanonical resource identityをgeneration-check済みhost valueへ接続し、atomic publication、borrow resolution、rollback、exactly-once cleanupを実装する。** — 2 pt
- [x] **P0 — Validated contract、negotiated binding、instanceごとのadapter stateからgeneral component instantiationをprototypeする。** — 3 pt
- [x] **P0 — 同じhost-value resource／failure fixtureをVM、JIT、AOT adapterで実行し、同等のoutcomeを記録する。** — 3 pt
- [x] **P0 — Production package managerを実装せず、in-memory resolverに対するdeterministicなlockfile／package-selection integrationをprototypeする。** — 5 pt
- [x] **P0 — 同じscalar／resource-failure conformance fixtureをWindows MSVCのdirect／response-file AOT test pathへ接続する。** — 1 pt
- [x] **P0 — Component-boundary cost evidenceをinstantiation、failure／destructor path、memory、code sizeへ拡張し、その解釈をRFC 0003へ記録する。** — 5 pt
- [x] **P0 — Nominal canonical `own<T>` typeを定義し、linear-use／zero-index checkを伴うstraight-line ownerをMIR、bytecode、Cranelift JIT、AOT object contract全体で運ぶ。** — 3 pt
- [x] **P0 — そのtyped ownerをtyped VM valueとadapter統合済みVM／JIT、およびmacOS AOT direct／response-file native link／runのownership-transfer／guest-trap cleanup conformanceで実行し、対応するWindows pathを構成する。** — 5 pt
- [x] **P0 — Negotiation、exact-lock replay、instantiation、scalar／resource call、failure cleanupにまたがる代表的multi-component application fixtureを1つ構成・計測する。** — 5 pt
- [x] **P0 — Component-conformance pathのnative Windows execution evidenceを記録する。** — 2 pt
- [x] **P0 — RFC 0003を判断し、acceptance、修正要求、rejectionのいずれかを記録する。** — 3 pt
- [x] **P0 — 完了済みexperimentからdecision-readyなownership／cyclic-graph RFCを草案化する。** — 5 pt
- [x] **P0 — 完了済みexperimentからdecision-readyなasync-cancellation／shared-service concurrency RFCを草案化する。** — 5 pt
- [x] **P0 — Ownership／cyclic-graph RFCを判断する。** — 2 pt
- [x] **P0 — Async-cancellation／shared-service concurrency RFCを判断する。** — 2 pt
- [x] **P0 — Exact requested payloadを含め、variable-size allocation callbackを計時／profileする。** — 5 pt
- [x] **P0 — Allocator overhead／fragmentationを含む代表的なallocation workloadを計測する。** — 5 pt
- [x] **P0 — Cross-mode benchmark setへ代表的なCFG workloadをもう1つ追加する。** — 3 pt
- [x] **P0 — 代表的なAOT object／link済みartifactにsource span駆動のnative debug metadataを実装し、macOS AArch64で検証する。** — 5 pt
- [x] **P0 — 同じ代表artifactについて、同等のWindows x86-64 CodeView／PDB／source-mapping evidenceを記録する。** — 3 pt
- [x] **P0 — 完成したcross-platform performance、debug-quality、integration evidenceから初期native backendを選択し、decisionを記録する。** — 3 pt
- [x] **P0 — Component、resource-control、backend-evidence prerequisiteの完了後にRFC 0001を判断し、acceptance、修正要求、rejectionのいずれかを記録する。** — 2 pt

### Evidenceと現在の状態

2026-10-17確認：RFC 0001–0005は受理済みですが、source compilationとproduction runtime／toolchain実装は未完成です。上の完了taskはprototypeの成果であり、production機能の完成ではありません。

- MIR → bytecode／VMとCranelift JIT／AOTはtest済みscalar／CFGと直線形owned-resource subsetを共有します。Source spanはnative mappingに加えてbytecode loweringでも保持します。
- Bounded schema-1／2 codec、要求全体のcapability preflight、negotiation／lock replay、resource adapter、host-value cleanup、failure-report fixtureがあります。完全なsandbox、production package manager、汎用component executionではありません。
- macOS AArch64 testと[Windows run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398)はdirect／response-file native executionを記録します。Debug inspectionは[architecture](architecture.md)から参照できます。
- 計測履歴は[benchmark evidence](benchmarks.md)へ集約します。過去のclean schema-20値は2026-10-17のJIT memory回収修正前のため、現在のcompile／application budgetには両tier-1 platformで新しいclean runが必要です。
- RFC 0004／0005はstrict ownership／concurrency policyを受理しています。Semantic modelは一部のtraceのみを扱い、reference counting、task tree、scheduler、cancellation-request実装を完成させるものではありません。

Exit達成：受理済み設計baseline、cross-mode prototype、initial backend evidence。以後のmilestoneでsource-level／production conformanceを満たします。

## Milestone 1：frontendとbytecodeのvertical slice（完了）

**進捗：89 / 89 pt（100.0%）。**

Ownerは2026-10-17に[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)を受理し、commit `031575e` に英日両方のdispositionを記録しました。受理済みの5つの設計gateが16 ptの完了分で、字句規則、module／call、aggregate／pattern、bytecode schema 1、上限付きframe、script／REPL behaviorを対象にします。Lossless parsing、module loading／resolution、source全体のtyping、nominal layout、pattern、source MIR、portable artifact、上限付きVM実行、初期scalar native conformanceは実装済みです。CLI／REPL、formatter、LSP統合も実装／検証済みです。

2026-10-17の見積り89 ptを維持します。Bytecode／REPL受理を2 pt／1 ptのtaskとして明示し、実装scopeを削っていません。Primitive typingを構造化input HIR 3 ptとparser接続済みsource／call conformance 5 pt、loweringをprimitive HIR／MIR 2 ptとparser接続／拡張source conformance 3 ptへ分割しました。基盤と広いparser接続成果は別々に計上し、最終監査で統合を検証します。

Incremental toolingの8 ptは共有graph／cache core 3 pt、protocol診断／位置変換1 pt、parser／module queryとLSP prototype統合4 ptに分割し、すべて完了しました。Lossless tokenization 3 ptとsyntax-tree構築／recovery 5 ptも完了し、89 ptの総計と全scopeを維持します。

- [x] **P0 — Stable code、label、fix-itを持つstructured source diagnosticsを確立する。** — 5 pt
- [x] **P0 — Identifier／keyword、空白／comment、signed-literal tokenization、source encoding、不正token診断をfixture付きで定めるRFC 0002の字句仕様を受理する。** — 3 pt
- [x] **P0 — 上限付きUTF-8 ingestion、lossless tokenization、literal／comment診断、source spanを実装する。** — 3 pt
- [x] **P0 — Lossless syntax treeを構築し、error recoveryとsource spanを保持して受理済み文法をparseする。** — 5 pt
- [x] **P0 — RFC 0002を超えるsource module／function callのvisibility、cycle、entry point、host call、call frame、recursion limitの意味論を受理する。** — 5 pt
- [x] **P0 — 明示的source root内のmoduleを上限付きで読み込み、可視性／循環を検証してitem／import namespaceを解決する。** — 3 pt
- [x] **P0 — Public-signature可視性を含むparsed local／pattern scopeとtype pathをsource HIRへ解決する。** — 2 pt
- [x] **P0 — 構造化したRFC 0002 function inputをspan保持HIRへ解決／type-checkし、上限付きanalysisと診断testを追加する。** — 3 pt
- [x] **P0 — Parsed sourceをHIRへ接続し、受理済みcall signatureを含むsource function／control flowをcompile-pass／fail fixtureでtype-checkする。** — 5 pt
- [x] **P0 — Record／variant／pattern／exhaustivenessの意味論、typed MIR／value表現、互換性fixtureを受理する。** — 5 pt
- [x] **P0 — Copy専用nominal定義のshapeを検証し、再帰型の循環を拒否し、checkedなvalue-cell／depth layoutと不正input testを実装する。** — 3 pt
- [x] **P0 — Record／variant構築、field、pattern、exhaustivenessをsource HIRへtype-checkする。** — 5 pt
- [x] **P0 — 受理済みprimitive HIRをCFG block argument／source map付きvalidated typed MIRへloweringし、VM／JIT differential testを追加する。** — 2 pt
- [x] **P0 — 受理済みcall／aggregateとsource-span conformanceを含むparser接続済みHIR-to-MIR loweringを完成させる。** — 3 pt
- [x] **P0 — Portable bytecode schemaと上限付きdecoding／verificationの契約を受理する。** — 2 pt
- [x] **P0 — Prototypeをencoding、decoding、verification付きversioned register bytecodeへ昇格させる。** — 5 pt
- [x] **P0 — 受理済みsource call、call frame、return、bounded recursionをMIR／bytecode／VMへ実装しfailure／stack testを追加する。** — 8 pt
- [x] **P0 — Typed host registration付きでsource-language subsetをVM実行する。** — 5 pt
- [x] **P0 — Script entry／outputとtransactional REPL state／error契約を受理する。** — 1 pt
- [x] **P0 — Deterministicなscript／REPL entry pointを提供する。** — 3 pt
- [x] **P1 — Formatterとparser／formatter round-trip testを実装する。** — 5 pt
- [x] **P1 — 推移的invalidation、古い結果の拒否、構造化HIR診断cache testを持つ上限付きrevisioned query graph／cacheを実装する。** — 3 pt
- [x] **P1 — Protocolの改行、related label、位置testを備え、structured diagnosticを上限付きUTF-16 LSP dataへ変換する。** — 1 pt
- [x] **P1 — 不完全source診断、protocol transport／capability、revision-safe publicationを含め、parser／module queryをLSP prototypeへ統合する。** — 4 pt

### 実装evidenceの履歴

以下は各commit時点の記録であり、当時の残作業は末尾の完了監査で更新します。

2026-10-17（JST）検証済みevidence：commit `a4f135c` は `lang-diagnostics` にimmutableなsource snapshot、UTF-8検証済みspan、Unicode／UTF-16位置、受理済み `SRC0001`–`SRC0007` identity、primary／secondary label、note、atomic fix previewを追加しました。5つの集中testが位置、古い／別mapのsnapshot、code identity、複数fileの表示、edit競合、制御文字のescapeを検証します。全体のfmt／Clippy／test／doc baselineは成功しています。Parser接続済み診断conformanceは実装作業として残ります。Commit `40a56cb` は英日RFC 0006提案と互換性fixture表を追加し、commit `031575e` はその後のownerの明示受理を記録しました。89 ptの計画はrecord／variant／call／toolingの全scopeを維持します。

Commit `f8d7370` は `lang-syntax` のUTF-8 ingestion、lossless tokenization、13件のconformance／limit testを実装しました。整数境界、不正token、comment、Unicode、NUL、正確なsource範囲、決定的な復元、resource上限を検証しています。全体のfmt／Clippy／test／doc baselineは成功しています。

Commit `c301665` は受理済み文法のarena方式lossless parsingを完了しました。File／expression別のentry point、missing-syntax node、delimiterを考慮したrecovery、保守的なfix-it、node／depth／list／診断数／workの上限を実装しました。18件のparser testでsyntax shape snapshot、fixtureの全prefix、決定的な不正input、resource境界、brace保持、後続itemへの復帰を検証します。Source-parser fuzz targetはlocked fuzz workspaceでcompileでき、instrumentationなしの10,000-run smokeも完了しました。Instrumentedなcoverage-guided evidenceは主張しません。全体のfmt／Clippy／test／doc baselineは成功しています。Semantic resolution／typing、formatter、LSP統合、source実行は別の未完了成果です。

Commit `da778cc` は受理済みsubsetの `lang-hir` analysisと13件の構造化input conformance／limit testを追加しました。Lexical localの解決、function全体での名前再利用／parameter assignmentの拒否、primitive type／condition／returnの強制、診断spanの保持を、字句規則を選ばず実装しています。全体のfmt／Clippy／test／doc baselineは成功しています。Parser診断とtext-to-HIR compile-pass／fail coverageは未完了です。

Commit `a198518` は受理済みprimitive HIR-to-MIR sliceとしてbranch join、loop-carried mutable state、early return、連続SSA renumbering、source map、構築limit、最終MIR validationを完成させました。8件の構造化input integration testでscalar極値、branch path、nested loop state、VM／JIT等価性、VM fuel超過、bytecode span、JIT／AOT-object debug mappingを検証します。全体のfmt／Clippy／test／doc baselineは成功しています。Parser接続済みexecution、parsed sourceのnative link／run evidence、guest call、aggregateを提供するものではなく、それらのtaskはRFC 0006受理後も未完了です。

Commit `8dafdeb` は `lang-query` に10件のgraph testと1件の実際の構造化input HIR-cache integration testを追加しました。Dependency置換、diamond invalidation、無関係なcacheの再利用、古い／別graphの結果の拒否、close／reopen、cycle、node／edge／work limit、部分的bookkeeping変更を伴わないrevision overflowを検証します。全体のfmt／Clippy／test／doc baselineは成功しています。Graphは宣言されたdependencyとhost提供のimmutable valueのみを扱い、compiler recipeの自動実行とLSP接続済みsource経路は未完了です。Roadmap reviewは89 ptの全計画を維持し、core／integration境界を明示します。

Commit `b4a6f24` は `lang-lsp` の診断data変換と4件の集中testを追加しました。UTF-16／surrogate幅、LF／CRLF／単独CR改行、CRLF途中の境界、EOF range、related location、severity／code保持、欠けたsnapshot／URI、source／label／note／output limitを検証します。[Architecture](architecture.md#diagnosticslspdebugging)に公式LSP 3.18の情報源と2026-10-17の確認日を記録しました。Living documentに公開／更新日は掲載されていません。全体のfmt／Clippy／test／doc baselineは成功しています。これはdata adapterであり、serverやwire codecではありません。残る4 ptの統合taskはRFC 0006受理後もparser／module query、capability、transport、document lifecycle、revision-safe publicationを維持します。

Commit `eadf992` は明示rootのcanonical module loading、compilation内のID、namespace収集、可視性を検証するmodule／enum-case path、forward alias、反復処理によるalias循環拒否を追加しました。17件のlocal testで階層、privacy、循環、重複、parser／encoding失敗、通算budget、宣言／work上限、Unix symlink境界を検証します。Linux専用caseは非UTF-8 filenameを検証します。全体のfmt／Clippy／test／doc baselineは成功しています。元のscope／resolution／loading 5 ptを、検証済みmodule成果3 ptと、parsed local／pattern scope、type path、public-signature可視性2 ptへ分割しました。89 ptの総計とsource統合scopeを保持します。Namespace graphはtyped bodyやsource実行の完了を示しません。

Commit `844aad0` はparsed scope、nominal type path、forward／mutual call、host import、record／enum／pattern名をbackend非依存の `lang-hir::resolved` arenaへ解決します。注釈／expression／pattern／control-flow spanと記述順の評価順序を保持し、function全体の名前再利用／immutable assignmentを拒否し、public-signatureのtype可視性を検証します。15件のtestでこれらの成果、syntax fixture全体の変換、正確なwork境界、resource上限、snapshot identityを検証します。全体のfmt／Clippy／test／doc baseline（319 test）は成功しています。完了したのはsourceの名前解決であり、expression／pattern typing、nominal layout／循環、exhaustiveness、return経路の完全性は元のtyping taskに残ります。Resolved programは実行可能artifactではありません。

Commit `1d5fa88` は `lang-hir` に上限付きnominal layout analysisを追加しました。ID／name／spanの検証、member重複／空enumの拒否、反復処理の循環検出、record cell／enum最大payload cellのchecked計算、guest割り当て前のdepth／work／構造上限を実装します。10件のHIR testと2件のparsed-source testで循環、overflow、depth 64／65とcell 4096／4097の境界、明示的な深いstack、不正input、small budgetを検証します。全体のbaseline（331 test）は成功しています。元のaggregate-typing 8 ptを、検証済みlayout成果3 ptと、aggregate操作／pattern typing・exhaustiveness 5 ptへ分割しました。全scopeと89 ptの総計を保持し、layoutによるtyped bodyや実行の成立は主張しません。

Commit `ef95dc0` はprimitive／control flow、guest／host call、厳密なnominal aggregate／field、pattern binding／shape、return経路、上限付きexhaustivenessのsource全体HIR typingを完了しました。成功後だけimmutableなtype／field／order／flow factsを公開し、warningではartifactを維持し、semantic errorや不正IRをloweringへ渡しません。24件の統合testはcompile-pass／fail source、有限domain oracleとの1,458件の照合、256-field直積、moduleから有効なmissing witness、不正ID／tree／scope／budget境界を含みます。両fuzz targetはcompileでき、新しい `typed_source_hir` targetのinstrumentationなしの10,000-run smokeも完了しました。Sanitizer／coverage-guided evidenceは主張しません。全体のbaseline（355 test）は成功しています。元のsource／call typingとaggregate／pattern typing成果を完了し、MIR loweringやsource実行の完了は主張しません。

Commit `e6b6987` は受理済みcompilation-unit MIR modelとbudget付きverifierを追加しました。Source-byte metadata、全必須operation、nominal layout、SSA／dominance、CFG argument／reachability、signature、entry検証、既存owned制約を扱います。15件のtestは独立したdominance oracleとの1,536件の照合、正確なlayout／resource境界、source位置、通算allocation／work計数を含みます。全体のbaseline（370 test）は成功しています。残るlowering、bytecode、VM taskの共有prerequisiteであり、未完了の統合にchecklist点を付けません。次のscopeは、decoder／verifierの共通通算budgetを使うcanonical schema 1 encode／decodeです。

Commit `b5b83b7` はportable schema 1 bytecodeのencode／decodeと共通budgetによるverificationを完了しました。11件のtestは88-byte固定fixture、全12 operation、global tableのcanonical sortとID再配置、byte一致reencode、切断／不正input、通算byte／count／allocation／work limitを検証します。Decoder fuzz targetのcompileとinstrumentationなしの10,000-run smokeも完了しました。全体のfmt／Clippy／test／doc baseline（381 test）は成功しています。Portable artifactの成果5 ptを完了し、実行は別成果として残します。

### 完了監査（2026-10-17）

Milestone 1の全24成果が完了し、P1 toolingの両成果を含め **89 / 89 pt** です。この結果のためにscopeを削除／移動していません。Roadmap全体は **298 / 738 pt（40.4%）** で、Milestone 2–7の未完了scopeは維持しています。

Commit `402a5dc` が最後のLSP統合4 ptを完了します。12件のprotocol／stdio integration testはlifecycle／capability、UTF-16 range、不完全source、増加／古いversion、close／reopenと消去、親文脈のmodule overlay、closed dependencyのrouting、watched file、local-root権限、不正framing／JSON、出力上限／回復、canonical formattingを検証します。`d419df9` はparser／module／診断query自動実行と古いpublication拒否を12件のdatabase testで別途検証します。macOS AArch64でfmt／Clippy／test／doc baselineの **497 test** がすべて成功しています。

Source実行のevidenceは `cb3312d`（typed source→verified MIR）、`b1964e7`（9件のscalar VM／JIT／native conformance testとlocal direct／response-file AOT link／run）、`d041598`（portable VM、17件のsource→wire実行test、独立した同時copy比較324回を含む7件のunit test）です。`77287e7` と `bbc1f3d` がscriptとtransactional REPLを完了し、10件のCLI test、16件のsession／CLI test、local TTY smokeで検証します。`2d3a3ff` がformatterを完了し、9件のCST／comment／冪等性testと2件のsource VM／native round tripで検証します。Parsing、resolution、typing、layout、portable schema-1 verificationは上記の過去evidenceへ記録しています。

受理済み[RFC 0002](rfcs/0002-prototype-control-flow.md)と実装済み[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)が、この完了profileを定義します（最終更新2026-10-17 JST）。Native guest call／aggregateはMilestone 2まで明示的に非対応です。Production ownership／concurrencyと完全なdeveloper toolingは後続scopeです。Local source-native evidenceから新しいWindows CI run、sanitizer campaign、coverage-guided fuzz結果、stable ABI、production sandboxを主張しません。

最終的な英日[機能ガイド](milestone-1.md)へ実装済みlanguage、CLI／REPL／editor workflow、embedding API、上限、evidence、後続scopeを記録しました。実装要件を代替／縮小せず、このexit audit後のdocumentation成果物を完了します。

Exit確認済み：小さなprogramをscriptとして実行でき、deterministic behaviorと有用なIDE diagnosticsがあります。

## Milestone 2：ownershipとnative AOT

**進捗：21 / 113 pt（18.6%）。**

開始条件：RFC 0006の実装済みcopy-value profileとcompatibility fixtureを基準とします。RFC 0004のstate modelはsource構文、production allocation、ABIを規定しません。対応する設計決定を受理してから依存する実装を進め、core-library sliceはRFCで最小要件が定まった時点で再見積もりします。

- [x] **P0 — Sourceのownership／borrow／arena profile、Copy分類、最小core-library型表面（text／byte、collection利用、必要なgeneric構文を含む）を受理する。** — 5 pt
- [ ] **P0 — 未規定のerror／panic cleanup、unsafe／FFI validity、allocator contractを公開interface実装前に受理する。** — 5 pt
- [ ] **P0 — Typed HIR／MIRでowned value、move、lexical borrow checkingを実装する。** — 8 pt
- [x] **P0 — RFC 0004のshared／weak retain-release、retainを伴うweak upgrade、atomic cycle-preflight／publicationを実装しrollback／race testを追加する。** — 8 pt
- [ ] **P0 — RFC 0004のlexical arena、escape拒否、stale-reference確認、deterministic teardownをcompile-fail／runtime test付きで実装する。** — 5 pt
- [ ] **P0 — 受理済みのreturn／error／panic cleanup規則に従ってdeterministic dropをelaborateする。** — 8 pt
- [ ] **P0 — RFC 0007のText／Bytes source operation、UTF-8検証、checked index、allocatorを保持する変換を実装する。** — 5 pt
- [ ] **P0 — Core source operationに必要なRFC 0007のUnit／Option／Resultと有限built-in specializationを実装する。** — 3 pt
- [ ] **P0 — RFC 0007のallocator-aware Vec source operationをmove／loan／growth／failure conformance付きで実装する。** — 5 pt
- [ ] **P0 — RFC 0007のallocator-aware Map source operationを厳密key equalityとreplacement／failure conformance付きで実装する。** — 3 pt
- [ ] **P0 — 狭いunsafe boundaryを定義し、FFI validity ruleを強制する。** — 5 pt
- [ ] **P0 — C ABI import／exportとgenerated headerを実装する。** — 8 pt
- [x] **P0 — RFC 0006のcopy-value profile全体（guest call、boolean interface、record、enum、pattern）をdifferential fixture付きでnative codeへlowerする。** — 8 pt
- [ ] **P0 — macOS AArch64／Windows x86-64向けownership-aware source-program object generationとnative link／run conformanceを完成させる。** — 8 pt
- [ ] **P0 — Linker discovery、startup／runtime packaging、production error handlingをhardeningする。** — 5 pt
- [ ] **P0 — 代表的なVM／native differential conformance suiteを構築する。** — 8 pt
- [ ] **P0 — 代表的native workloadを同等のRust／C実装に対して計測する。** — 8 pt
- [ ] **P1 — 両tier-1 platformでsource-span native debug metadataをemit／validateする。** — 8 pt

Exit：代表的なcommand-line programがVM semanticsと一致し、同等のRust／C実装に対してperformanceが計測されている。

2026-10-17 JSTに確認した実装evidence：commit `926fb3b` はRFC 0006のcopy-value全12命令を扱う `lang-codegen-clif::program` のunit全体JIT／object loweringを追加しました。10件のfocused testでcanonical wire往復、前方／相互call、boolと入れ子aggregateのinterface、structural／literal pattern、loop join／self-edge swap、host評価順序／preflight、不正入力／projection、compile budget／frame capacity、複数fileのspan、Mach-O／COFF生成、macOSでのdirect／response-file link／runを検証します。Fmt／Clippy／test／docのfull baselineは515 testすべてpassしました。従来のscalar／owned compatibility adapterも維持します。新しいWindows native実行、ownership-aware object、公開C ABI／header、完全なsource debug format、production packaging、代表的performance／conformanceは、それぞれの未完了成果として残ります。

このscope後のroadmap確認：113 pt総計と全decision gateは引き続き適切です。今回の受理済みcopy-value実装はsource ownershipの判断に依存せず、その完了によってownership／core-libraryやproduction AOTのtaskを完了扱いにはしません。暗黙のacceptance／stabilizationは記録しません。

Commit `a607ed2` は[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)と日本語対訳を追加しました。Copy分類、字句的loan／region、明示shared／weak／arena構文、allocator-awareなtext／byte／collectionの有限型表面を定めます。Ownerが2026-10-17 JSTに明示受理し、`31ab7ac` に記録したことで最初の5 pt decision gateが完了しました。Cleanup／allocator／unsafe／FFI契約とownership bytecode encodingは別判断のままです。Link／対訳metadata確認とrepository-consistency testはpassしました。受理済みの有限surfaceをText／Bytes（5）、Unit／Option／Result／組み込みspecialization（3）、Vec（5）、Map（3）の独立して検証可能な4成果へ再見積もりしました。従来8+8 ptの総計とsource／platform／conformance／measurementの全scopeを保持します。次は残り契約を定義し、source／allocator統合の早計な完了主張を避けながら受理済みRFC 0004のruntime serviceを進めます。

Commit `ec16f8a` はruntime shared-ownership engineを追加しました。実際のretain付きowner／upgrade、追跡strong／weak field、generation／provenance確認、aliasを考慮するread lease、mutexで線形化するwhole-payload cycle preflight／publicationを実装します。16件のfocused testに、独立した推移閉包／count oracleとの1,000比較、64回の逆向きpublication競合、128回のweak upgrade／last-release競合、15,001 nodeの反復reclamation、rollback／limit／枯渇／poison境界を含みます。Fmt／Clippy／test／docのfull baselineは531 testすべてpassしました。Runtime lifecycle／publicationだけを完了とし、source／MIR／VM／native ownership統合、arena、guest cleanup、公開allocator／FFI契約は未完了です。現engineはabortし得るArc allocationを含むhost Rust storageを使い、未決のsource allocatorのrecoverable-OOM契約やsandboxを保証しません。Roadmap確認では総113 ptと残る全outcomeを維持します。次のdecision scopeはcleanup／allocator／unsafe／FFIで、source ownership analysisはRFC 0007の受理により独立して開始可能です。

Source grammarのfollow-up（2026-10-17 JST）：lossless parser／formatterで受理済みRFC 0007のloan／ownership type、入れ子の組み込みtype argumentと `::<...>` call、place borrow／dereference／assignment、allocatorを渡すarena blockを実装しました。追加10 testでownership fixture全prefix、syntax／CST／comment往復、limit／recovery、`SRC0024` 予約名migration、旧source bindingへ進む前のspan付き拒否を検証し、full baselineは541 testすべてpassしました。これは未完了8 ptのHIR／MIR ownership outcome内のprerequisite進捗であり、move／loan、arena escape安全性、組み込み型検査、実行、ownership wire supportの完了ではありません。Roadmap確認では完了21/113 ptと残る全decision／implementation／measurement gateを維持します。次はcleanup／allocator／unsafe／FFI契約をowner判断可能な形へ起草し、受理済みRFC 0007のownership-aware source解析を独立して進めます。

## Milestone 3：concurrencyとembedding

**進捗：0 / 96 pt（0.0%）。**

開始条件：Milestone 2が両実行pathのownership／cleanup semanticsを提供します。RFC 0003／0005を受理済み意味論の基準とし、そのprototypeでproduction IDL／scheduler／sandboxを完了扱いにしません。Source VMには既にframe／live-cell上限があり、このmilestoneでtask、native code、host callback全体へaccountingを拡張します。

- [ ] **P0 — RFC 0005を緩和せず、残るscheduler／channel／backpressure／error contract、send／share導出、source async構文を受理する。** — 5 pt
- [ ] **P0 — Production component IDL／wire profileとembedding ABIを受理し、compatibility、typed failure、callback reentrancy、RFC 0003へのcancellation対応を決める。** — 5 pt
- [ ] **P0 — Structured task lifetimeとdeterministic cancellation cleanupを実装する。** — 8 pt
- [ ] **P0 — Bounded backpressure／cancellation behavior付きchannelを実装する。** — 5 pt
- [ ] **P0 — Actor isolationとaccept済みdata-race capability ruleを強制する。** — 8 pt
- [ ] **P0 — Version付きcomponent IDLとbounded serializationを実装する。** — 8 pt
- [ ] **P0 — Canonical owned／borrowed resource adapterを実装する。** — 8 pt
- [ ] **P0 — Capability-gated component importを強制する。** — 5 pt
- [ ] **P0 — Typed error付きmixed native／VM call／callbackをsupportする。** — 8 pt
- [ ] **P0 — Native／VM boundaryを越えてstack trace／cancellationを保持する。** — 5 pt
- [ ] **P0 — Host-callback accountingを含むheap／allocation limitを強制する。** — 5 pt
- [ ] **P0 — Recursion-depth／live-task limitを強制する。** — 5 pt
- [ ] **P0 — VM、JIT、AOT、callback boundary全体でcooperative interruptionを強制する。** — 8 pt
- [ ] **P0 — 両tier-1 platformでcross-modeのownership／failure／capability／resource-limit conformanceを確認後、C embedding APIとgenerated bindingをstable化する。** — 8 pt
- [ ] **P1 — Trusted-native／verified-script plugin exampleを提供する。** — 5 pt

Exit：Native hostがuser scriptをloadし、typed APIを公開し、scriptをcallし、callbackを受け取り、IPCなしでcapabilityを強制できる。

## Milestone 4：developer toolchain

**進捗：0 / 63 pt（0.0%）。**

- [ ] **P0 — Version付きpackage／workspace manifest、target condition、lockfile互換性、machine-readable CLI diagnostic契約を受理する。** — 5 pt
- [ ] **P0 — Deterministic dependency resolution、lockfile、checksum、offline modeを実装する。** — 8 pt
- [ ] **P0 — 既存の`lang run` frontendをpackage対応build／run／test orchestrationへ拡張し、受理済みstructured diagnostic出力を提供する。** — 5 pt
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

開始条件：必要なtext／byte、collection、allocation、module、host-I/O primitiveをRust compilerが両tier-1 platform向けに既にcompileできること。Subsetのfreezeはevidenceに基づく互換性判断であり、移植中に欠けたstage-0 semanticsを新設する許可ではありません。

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

開始条件：最初のfrontend、共通ABI、conformance suiteがstableであること。追加frontendには受理済みRFCが必要であり、最初の言語に残る未解決のsemanticsを迂回する手段にはしません。

- [ ] **P0 — Library、profile、syntax sugarでは満たせない要件を別frontendが提供することを実証する。** — 5 pt
- [ ] **P0 — Frontend semanticsとshared-MIR／component boundaryを規定する。** — 5 pt
- [ ] **P0 — Frontend固有のparsing、HIR、semantic analysisを実装する。** — 8 pt
- [ ] **P0 — 忠実に表現できるsemanticsだけをshared typed MIRへloweringする。** — 8 pt
- [ ] **P0 — Memory、effect、runtime modelが異なる箇所にcomponent adapterを実装する。** — 8 pt
- [ ] **P0 — Cross-language package metadata／version resolutionを追加する。** — 5 pt
- [ ] **P0 — Differential cross-language conformance testを追加する。** — 8 pt
- [ ] **P0 — Cross-language stack trace、source mapping、debugger integrationを追加する。** — 8 pt

Exit：2つ目のfrontendが明確なuser valueを追加し、最初の言語の安全性を弱めず、shared IRを無関係な意味論の保守不能な集合にしない。

