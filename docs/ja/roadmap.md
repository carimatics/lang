<!-- translation-of: ../roadmap.md; canonical-language: en -->

# 実装ロードマップ

[English](../roadmap.md)

このロードマップはrisk reductionの順に並んでいます。Milestoneはcalendar commitmentではなくexit criteriaです。

## 現在のM2実装状況

**検証済み65件 · 進行中0件 · 残り36件（暫定91 pt）。**

現在のTodo、evidence、残作業見積もりは [M2実装Todo](milestone-2.md) を参照してください（2026-10-18 JST更新）。受入条件の数値が最後に動いてから、計画済みfollow-up設計判断を含む39件の成果を検証しました。C02（`1271ad8`）で登録付きshared／exclusive Vec element loan、pop／Copy transfer、元spanと実retirement順を保持する全体／field replacementを追加しました。追加11 testでconstructor metadata予算／OOM、stale／不正lease、serial枯渇、abort、旧payload cleanup失敗／panic、独立oracleと比較する96回の混合操作を検証しました。全体baselineの79 suite・1196 testとfmt／Clippy／docが成功し、**残り94 → 91 pt**、done 65行、計101行です。Vecの基盤2件C01／C02が完了し、前提C02／V09も完了した次のC03でsource fixtureの実行を必須にします。未対応の入れ子Vec payloadをshared／arena操作へ副作用前に通さないservice別preflightが必要であり、現在の単一global closureでVecを一律有効化できません。最初の実source実験まではC03を暫定3 ptとします。入れ子統合全体はV13／V17とnative N04の前提C07に維持します。Native、collection、provisional-read／preview、artifact全体のevidenceは引き続き必要で、新exit gateは完了扱いにしません。

従来の **完了条件18件中5件完了** は維持します。その **34 / 113 pt** は履歴上の受入条件の重みであり、作業量予測や日々の実装進捗ではありません。2026-10-18の監査で隠れた前提作業と粗すぎるchecklistを明確にし、113 ptの作業量見積もりは使用を取りやめました。完了成果と残作業を分けて追跡し、後付けのeffort完成率は示しません。

## タスク管理

未規定のsource、ABI、package、serialized behaviorを導入する実装taskは、そのbehavior実装前に受理済みRFCを必要とします。Decision taskはownerまたは[RFC 0001の実験段階の委任](rfcs/0001-project-charter.md#実験段階のrfc判断)に従うagentのdisposition記録で完了し、設計が拒否された場合は依存実装を完了扱いにせず再計画します。各成果に応じたcompile-pass／fail、不正入力／resource-limit、VM／native差分testを実行します。非信頼環境へ公開する前にdecoder／verifierのCPU・allocation budgetが必要です。

各milestone内のtaskはdependencyを考慮した優先度順に並べます。すべての `P0` taskがcheck済みで、exit evidenceが記録されるまでmilestoneはexitできません。未完了の `P1` taskはexit前に移動先とcostを明示的に再設定し、暗黙に消してはいけません。Check済みtaskは完了です。前のmilestoneで完了したprototype groundworkだけでは、後続milestoneのより広いtaskを完了扱いにしません。

以下のmilestone checklistは相対的な受入条件の重みを保持します。`1`はごく小、`2`は小、`3`は中、`5`は大、`8`は非常に大です。重み付きの達成率は `完了した重み / 全重み × 100` を小数第1位へ丸めます。この粗い重みは経過時間や実装残作業を予測しません。Milestoneがcurrentになったとき、またはscope／evidenceが変わったときに、長期見積もりを積み上げで見直します。進行中のM2では、実装Todoで独立検証可能な残り1〜3 ptのtaskに分解します。

2026-10-16のroadmap全体監査で、3つの構造的問題を修正しました。Source frontendを必要とするproduction作業がdesign baselineへ誤って含まれていたこと、複数の独立してship可能な成果を1つにまとめたtaskがあったこと、後続milestoneで必要なregistry、bootstrap、interoperability作業が欠けていたことです。改訂後のlistは、その監査時点で完了済みだったevidence 80 ptを維持し、production hardeningを依存先のmilestoneへ移し、大きな複合項目を1 taskとして扱わず、隠れていた作業を明示しました。Component workstreamのfollow-up監査では、in-memory compatibility negotiation、scalar execution-prototype integration、backend-neutral resource-adapter behavior、boundary-cost measurementを分離しました。従来の単一cross-mode scalar／resource taskはこれらのacceptance boundaryを混在させ、Milestone 3のproduction IDL／adapter implementationとも重複していました。Schema 18後の監査では、native debug-metadata実装とcross-platform debug-quality validationがbackend選定のprerequisiteとして本文にだけ記載されていることを発見したため、backend選定前の5 pt／3 pt taskとして明示しました。Host-value follow-up監査では、host-value identity bridge、general instantiation、VM／JIT／AOT resource／failure conformanceという3つのacceptance boundaryをなお混在させていた別の8 pt項目を分割しました。分割後も元の8 pt総計を維持し、各成果を独立してcheckできるようにしました。Repository ownerのacceptanceを必要としたdecision taskは、ownerがacceptするまで未完了のままevidence prerequisiteの後ろへ配置しました。Checklistはdependency順を記録するものであり、外部gateをlocalで実行可能とみなすものではありません。Owner decisionまたはremote runnerが利用できない場合は、優先度を変更せず次のunblocked P0へ進みます。

同日のRFC readiness監査では、別のplanning gapも修正しました。RFC 0003はowner decision待ちとして並んでいましたが、review outcomeは当時stable serialization、host-value／cross-mode adapter conformance、lock integration、native Windows component execution、より広いcost evidenceをblockerとして明記していました。これらの成果はRFC 0003 decision前の明示的なprototype taskとなり、production IDL、adapter、package managerは引き続きMilestone 3／4の責務とします。

同日のnative evidence監査では、3 ptのWindows component項目が2つのacceptance boundary、すなわちlocalで検証可能なMSVC direct／response-file test integrationと、外部で観測するnative runner evidenceを混在させていることを確認しました。この項目を完了済み1 ptのimplementation taskと当時未完了だった2 ptのevidence taskへ分割し、総costを維持しながら、利用できないremote evidenceがlocal作業を隠さないようにしました。

続くRFC consistency監査では、RFC 0003だけに記載されていた2つのdecision blocker、すなわちshared MIR／backend contractを通るtyped resource loweringと、代表的multi-component application evidenceを確認しました。これらを8 pt／5 ptのMilestone 0 prototype taskとして明示します。Milestone 3のproduction component IDL／adapter implementationを置き換えるものではありません。Production package-manager I/OはMilestone 4の作業であり、RFC 0003 designを判断するprerequisiteではありません。

最初のtyped-resource slice後、8 ptのlowering taskを次のacceptance boundaryで分割し、総costを維持しました。完了済み3 pt taskはnominal MIR／bytecode typingとCranelift JIT／AOT machine laneをcoverし、2番目の5 pt taskはtyped VM value、adapter統合済みownership transfer／cleanup、native link／run conformanceをcoverします。

2番目のtyped-resource sliceは、規定したprototype boundaryで完了しました。Typed VM valueとadapter統合済みVM／JIT executionは、macOS／Windows AOT direct／response-file native link／run testとnominal owner laneを共有します。GitHub Actions run 38932964398が対応するWindows execution／cleanup outcomeを記録し、下記の独立した2 pt evidence taskを完了しました。

Representative-application taskも規定したprototype boundaryで完了しました。Host-orchestrated fixtureは2-provider negotiation、deterministic lock生成、exact-lock replay、instance publication、scalar／typed-resource VM／JIT call、guest-trap cleanupをcomposeします。Cleanなschema-20 macOS baselineは1,000 operationを31 sample実行し、median 67.592 us、p95 68.459 us、RSD 1.109%を記録しました。CleanなWindows baselineは同じrowでmedian 236.804 us、p95 266.366 us、RSD 6.201%を記録し、production generated dispatch stubだけが別作業として残ります。

2026-10-17のrepository reviewで、source ownership／core-library構文とproduction component profileの開始gate欠落を明確にし、既存copy-value profileのnative compilationをownership-aware AOTから分離しました。Milestone 2に未完了21 pt、Milestone 3に未完了5 ptを追加し、完了済み298 ptは維持します。全体進捗の低下は隠れていた作業の明示によるものです。Source／core-library基盤を代表的workload／self-hostingより先に整備し、C embeddingのstable化はconformance／resource enforcement確認後に行います。

2026-10-18（JST）に確認した受入条件の重みのsnapshotです。全体の計算は履歴上の重みの集計であり、全体作業量の見積もりではありません。現在のM2実装状況は上記とリンク先のTodoを参照してください。

| Milestone | 完了条件の重み | 条件の総重み | 条件達成率 |
| --- | ---: | ---: | ---: |
| 0 — design baseline | 209 pt | 209 pt | **100.0%** |
| 1 — frontendとbytecodeのvertical slice（完了） | 89 pt | 89 pt | **100.0%** |
| 2 — ownershipとnative AOT | 34 pt | 113 pt | 30.1% |
| 3 — concurrencyとembedding | 0 pt | 96 pt | 0.0% |
| 4 — developer toolchain | 0 pt | 63 pt | 0.0% |
| 5 — ecosystemとcompatibility | 0 pt | 65 pt | 0.0% |
| 6 — セルフホスティング | 0 pt | 74 pt | 0.0% |
| 7 — 追加frontend | 0 pt | 55 pt | 0.0% |
| **全体** | **332 pt** | **764 pt** | **43.5%** |

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

**完了条件：18件中5件完了、履歴上の重み：34 / 113 pt（30.1%）。**

**実装Todo：[現在の完了成果、次のtask、残作業見積もり](milestone-2.md)。** 現在の作業queueはこちらで管理し、以下のchecklistは従来の受入条件を保持します。

開始条件：RFC 0006の実装済みcopy-value profileとcompatibility fixtureを基準とします。RFC 0004のstate modelだけでは欠けていたsource／core、cleanup、allocator、C境界の受理済み要件は、現在RFC 0007／0008で規定しています。新たな設計決定が必要なbehaviorは受理後に実装します。実装Todoに2026-10-18の残作業積み上げ見直しを記録しました。

- [x] **P0 — Sourceのownership／borrow／arena profile、Copy分類、最小core-library型表面（text／byte、collection利用、必要なgeneric構文を含む）を受理する。** — 5 pt
- [x] **P0 — 未規定のerror／panic cleanup、unsafe／FFI validity、allocator contractを公開interface実装前に受理する。** — 5 pt
- [x] **P0 — Typed HIR／MIRでowned value、move、lexical borrow checkingを実装する。** — 8 pt
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

### 実装evidence履歴

以下は各commit時点の記録です。「113 ptの見積もりは引き続き妥当」という繰り返しの判断は2026-10-18の [進捗管理監査](milestone-2.md#従来の数値が止まった理由) で置き換えました。履歴evidenceとして保持し、今後の通常報告は長文追記ではなく現在のTodo行を更新します。

Ownerの判断（2026-10-17 JST）：RFC 0008をacceptedとして作業を続ける明示指示に従い、[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)を受理しました。Cleanup／allocator／unsafe／C-validity decision gateは完了し、5 ptを加算してM2は26 / 113 pt（23.0%）、全18成果中4成果完了となりました。受理により規定実装を進めますが、cleanup、guest allocation、unsafe／FFI強制、C adapter、platform conformanceは完了扱いにしません。以下の過去follow-upは各commit時点の状態を記録しており、RFC 0008のproposed／受理待ちという記述は履歴です。現在のstatusは受理済みRFCとこの判断記録を参照してください。

2026-10-17 JSTに確認した実装evidence：commit `926fb3b` はRFC 0006のcopy-value全12命令を扱う `lang-codegen-clif::program` のunit全体JIT／object loweringを追加しました。10件のfocused testでcanonical wire往復、前方／相互call、boolと入れ子aggregateのinterface、structural／literal pattern、loop join／self-edge swap、host評価順序／preflight、不正入力／projection、compile budget／frame capacity、複数fileのspan、Mach-O／COFF生成、macOSでのdirect／response-file link／runを検証します。Fmt／Clippy／test／docのfull baselineは515 testすべてpassしました。従来のscalar／owned compatibility adapterも維持します。新しいWindows native実行、ownership-aware object、公開C ABI／header、完全なsource debug format、production packaging、代表的performance／conformanceは、それぞれの未完了成果として残ります。

このscope後のroadmap確認：113 pt総計と全decision gateは引き続き適切です。今回の受理済みcopy-value実装はsource ownershipの判断に依存せず、その完了によってownership／core-libraryやproduction AOTのtaskを完了扱いにはしません。暗黙のacceptance／stabilizationは記録しません。

Commit `a607ed2` は[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)と日本語対訳を追加しました。Copy分類、字句的loan／region、明示shared／weak／arena構文、allocator-awareなtext／byte／collectionの有限型表面を定めます。Ownerが2026-10-17 JSTに明示受理し、`31ab7ac` に記録したことで最初の5 pt decision gateが完了しました。Cleanup／allocator／unsafe／FFI契約とownership bytecode encodingは別判断のままです。Link／対訳metadata確認とrepository-consistency testはpassしました。受理済みの有限surfaceをText／Bytes（5）、Unit／Option／Result／組み込みspecialization（3）、Vec（5）、Map（3）の独立して検証可能な4成果へ再見積もりしました。従来8+8 ptの総計とsource／platform／conformance／measurementの全scopeを保持します。次は残り契約を定義し、source／allocator統合の早計な完了主張を避けながら受理済みRFC 0004のruntime serviceを進めます。

Commit `ec16f8a` はruntime shared-ownership engineを追加しました。実際のretain付きowner／upgrade、追跡strong／weak field、generation／provenance確認、aliasを考慮するread lease、mutexで線形化するwhole-payload cycle preflight／publicationを実装します。16件のfocused testに、独立した推移閉包／count oracleとの1,000比較、64回の逆向きpublication競合、128回のweak upgrade／last-release競合、15,001 nodeの反復reclamation、rollback／limit／枯渇／poison境界を含みます。Fmt／Clippy／test／docのfull baselineは531 testすべてpassしました。Runtime lifecycle／publicationだけを完了とし、source／MIR／VM／native ownership統合、arena、guest cleanup、公開allocator／FFI契約は未完了です。現engineはabortし得るArc allocationを含むhost Rust storageを使い、未決のsource allocatorのrecoverable-OOM契約やsandboxを保証しません。Roadmap確認では総113 ptと残る全outcomeを維持します。次のdecision scopeはcleanup／allocator／unsafe／FFIで、source ownership analysisはRFC 0007の受理により独立して開始可能です。

Source grammarのfollow-up（2026-10-17 JST）：lossless parser／formatterで受理済みRFC 0007のloan／ownership type、入れ子の組み込みtype argumentと `::<...>` call、place borrow／dereference／assignment、allocatorを渡すarena blockを実装しました。追加10 testでownership fixture全prefix、syntax／CST／comment往復、limit／recovery、`SRC0024` 予約名migration、旧source bindingへ進む前のspan付き拒否を検証し、full baselineは541 testすべてpassしました。これは未完了8 ptのHIR／MIR ownership outcome内のprerequisite進捗であり、move／loan、arena escape安全性、組み込み型検査、実行、ownership wire supportの完了ではありません。Roadmap確認では完了21/113 ptと残る全decision／implementation／measurement gateを維持します。次はcleanup／allocator／unsafe／FFI契約をowner判断可能な形へ起草し、受理済みRFC 0007のownership-aware source解析を独立して進めます。

Decision gateの準備（2026-10-17 JST）：提案[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)と日本語対訳はcleanup／failure優先順位とreturn escrow、recoverable budget付きguest allocation、unsafe／capability規則、有限generated C status／header／descriptor契約を定義します。提案layout assertionはApple Clang 21で両tier-1 target tripleにpassし、4つのscalar descriptor／SHA-256 vectorを独立検証しました。提案／cross-target fixtureでありsource実装やnative Windows実行ではありません。第2の5 pt gateは明示owner dispositionまで未完了です。Ownership wire encodingは別途必要な判断で、grammar／source解析はRFC 0007に従って独立に進められます。Reviewでは総113 ptと元のownership／core／platform／conformance／performanceの全deliverableを維持します。

Source typeのfollow-up（2026-10-17 JST）：commit `81fab15` はRFC 0007のtype constructorをspan保持・構造的interning付きHIRへ解決し、再帰する間接型の上限付きCopy／drop／storage／privacy解析を追加しました。11件のsource testで受理済み型群、不正storage／return／specialization、source identity、不正HIR、正確な上限、反復depth、独立した生成型oracleを検証し、2件のCLI／query integration testでsource診断と復帰を保持します。解決済みHIRのfuzz targetがcompileし、instrumentationなしの10,000-run smokeが完了しました。全554 testとfmt／Clippy／docのfull baselineがpassしました。両program-check entry pointは拡張profileの実行用typing前に意図的に停止し、move／loan／region flow、core operation typing、ownership MIR／lowering／実行、cleanup、wire encodingは未完了です。Reviewでは21/113 pt、元の総113 pt、残る全gateを維持します。型の性質は未完了HIR／MIR ownership outcome内の前提進捗です。RFC 0008の明示owner判断を待つ間、RFC 0007に従ってsource body／placeとownership flowの実装を進めます。

Source bodyのfollow-up（2026-10-17 JST）：commit `f6e0edb` は通常loan、明示dereference、place assignmentを既存expression arena内の専用place IDへbindし、字句的なcompiler-created arena ownerも追加しました。6件のsource testで正確なroot／span、temporary拒否、owner visibility／名前規則、到達不能codeの診断、通算expression／local／block／depth上限、ownership annotationがなくても実行用checkで拒否する境界を検証します。全560 test、fmt／Clippy／doc、lock済みfuzz compileが成功しました。Bindingが保証するのは名前とplace shapeのみで、mutability、move、loan flow、allocator authority、region escape、core signature、cleanup、ownership MIR／VM／native動作は未完了です。Reviewでは完了21/113 ptと元のoutcome／見積もりを維持します。次は受理済み有限core signatureとownership body checkingを接続し、RFC 0008の判断とownership serializationは独立gateとして残します。

Core signatureのfollow-up（2026-10-17 JST）：commit `9342175` はRFC 0007が要求する全34 operationを、固定source signature template、type-argument制約、value／instance allocator／provisional loan／arena result由来とともに識別します。Source callは具体type argumentをbindし、callee／argument spanと記述順を保持します。不正pathとtype／value arityには受理済みsource診断を出し、generic-callのunsupported-syntax guardを置換しました。独立したsignature表／由来の2 testと追加3 source testが、全565 testのfmt／Clippy／doc baselineおよびlock済みfuzz compileとともに成功しました。Operand型／規則の強制、loan／region flow、core case／Unit構築とpattern、operation、実行用loweringは未完了で、compiler／runtime／ABI gateは維持します。Reviewでは21/113 ptと元の総計／outcomeを維持し、この前提作業を未完了HIR／core実装scope内へ記録します。次はcore value／case構文のbindingとownership body typing／flowを実装し、RFC 0008はowner判断待ちとして残します。

Core valueのfollow-up（2026-10-17 JST）：commit `0115a46` はUnit空recordのvalue／patternと、全22 Option／Result／error caseを、閉じたfamily identity、正確なpayload／括弧shape、入れ子payload、source span付きでbindし、疑似user nominal IDを導入しません。独立したcase catalog test 1件と6件のsource testで全case、入れ子scope／error、parserとbindingの拒否境界、identity分離、budget、allocator／Vec／Result／read-place fixtureを検証します。全572 testのfmt／Clippy／doc baselineとlock済みfuzz compileが成功しました。Expected-context推論、payload型の適合、exhaustiveness、ownership flow、実際のcore operation、cleanup、実行用lowering、ownership serializationは未完了で、HIR／MIR 8 ptもUnit／Option／Result 3 ptも完了扱いにしません。Roadmap reviewでは21/113 pt、元の全outcome、独立したRFC 0008／wire decision gateを維持します。受理済みgrammarとcore名が解決済みHIRに揃ったため、次の実装scopeはexpected-type body解析とmove／loan／region checkingです。Binding testからruntimeやABIの完了を推定しません。

Body typeのfollow-up（2026-10-17 JST）：commit `8eda046` は既存checkerを再利用して双方向body解析とcore exhaustivenessを実装します。読み取り専用の `BodyTypes` は、解決済みsourceを保持しつつ、具体化したcall signature／由来と独立したtyped arenaを保持します。期待文脈、Unit／sum／error、通常loan／dereference／place型、推論argument borrow、non-copy patternのloan型、core type-argument制約を扱います。12件のbody testで全31非arena signature／全22 case、独立したCopy checkerとの一致、再解析／source保持、不正foreign input、正確なinstantiation／work／allocation／出力上限、型解析のみと実行の境界を検証し、2件のCLI／query testで診断公開と復帰を確認します。全586 test、fmt／Clippy／doc、lock済みfuzz compile、instrumentationなしの10,000-run HIR smokeが成功しました。Reviewでは21/113 ptと元の全outcomeを維持します。Arena-ownerの型解析、初期化／move／partial move、mutabilityと字句的loan／region flow、ownership MIR／cleanup、core実行、wire format、native／platform／debug／conformance／performanceは未完了です。型解析だけのfactsは実行やallocator権限を付与しません。次はbody typingへ字句的arena ownerを表現し、検証済みMIRに必要なownership dataflowを実装します。RFC 0008のowner判断とownership serializationは独立したdecision gateとして残します。

Arena-owner型のfollow-up（2026-10-17 JST）：commit `7213ae3` はfunction／local identityをkeyとするcompiler専用の宣言site owner型、allocator loan検査、ownerの字句的scope／return flow、arenaの3 signatureで具体化したowner authorityを追加します。全34 core operation signatureをbody解析で具体化できるようになりました。8件のsource／foreign-input testで全arena fixture、入れ子／function identity、明示loan／alias、不正allocator／owner／loan kind、source owner型／parameterの禁止、不正ID、正確なbudget、残るcross-region／実行境界を検証します。全594 testのfmt／Clippy／doc baseline、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功しました。Reviewでは21/113 ptと元の全outcomeを維持します。静的owner identityはvalue region由来、loan validity、runtime generation／staleness、teardown、ownership MIR、実行を証明しないため、arena 5 ptとHIR／MIR 8 ptは未完了です。次は分岐／loop edgeのdefinite initialization／move dataflowを実装し、字句的loan／region flowと検証済みMIR transitionへ進めます。RFC 0008の判断、serialization、platform／debug／conformance／performance gateは独立して残します。

初期化／moveのfollow-up（2026-10-17 JST）：commit `9263966` はtyped source bodyに上限付きdefinite initialization／whole-local consumption解析を追加し、実行用に昇格せずsource／access factsを保持します。記述順の評価、再初期化、partial／borrowed move、fallible inputのconsumption、継続経路のbranch／match join、loop header復元／condition exitを検査します。Coverageが冗長と証明したarmをreturn解析とCopy MIR loweringへ共有し、誤ったmissing-return／continuationを修正しました。15件のmove testに独立state oracleと比較する80生成branch programを含み、2件のCLI／query testと1件のVM／JIT regressionも追加しました。全612 test、fmt／Clippy／doc、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功しました。Reviewでは21/113完了ptと元の全outcomeを維持します。Activeな字句的loan、mutability／reborrow規則、value region由来／escape、ownership MIR／cleanup、source／runtime統合は未完了です。Move factsだけではloan validityや実行を保証できないため、HIR／MIR 8 ptとarena 5 ptは未完了のままです。次は字句的loan／mutability解析を実装し、region／手続き間flowと検証済みMIR transitionへ進めます。RFC 0008のowner判断、ownership serialization、platform／debug／conformance／performance gateは独立して残します。

字句的loanのfollow-up（2026-10-17 JST）：commit `7e3d43d` はmove検査後に上限付きwhole-owner loan／mutability解析を追加し、owning parameterのimmutable性、推論／明示call loan、unused loanの字句的期間、reborrowによるparent凍結、reference変数の再代入、scope escape検査、継続経路のjoin、loop header復元を扱います。Non-Copy matchのfield loanとcompiler既知の暫定readはarmごとにowner authorityを維持／解放し、arena readとmutationの競合も検査します。Constructed型注釈のないownership／core構文もmetadata検証後の上限付きscanを介してbody検査へ進みます。21件のsource testに独立oracleと比較する64 permission sequenceを含み、2件のCLI／query testで診断／上限とrevision-safe snapshotを検証します。全635 test、fmt／Clippy／doc、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功しました。Reviewでは21/113完了ptと元の全outcomeを維持します。入れ子reference cellの由来はchecked bodyの証拠を生成しない明示的な未完了gateです。Valueの推移的arena brandと手続き間flow、ownership MIR／layout／cleanup、core／runtime実行、動的alias検査も未完了のため、HIR／MIR 8 ptとarena 5 ptは未完了のままです。次は入れ子reference cellへloan由来を拡張し、arena brand／escape flowと検証済みMIR transitionへ進めます。RFC 0008の判断、ownership serialization、platform／debug／conformance／performance gateは独立して残します。

入れ子referenceのfollow-up（2026-10-17 JST）：commit `83b5e36` はreference cellの未完了gateを、検査済みlocal／parameter chain、権限の共通部分、間接store、上限付きの推移的owner／cell dependency factへ置き換えます。Inactive projectionは早すぎる凍結を起こさずdestination→RHSの評価を維持し、publication時に経路全体を再検証します。入れ子call argumentは到達可能な参照先を予約し、暫定core resultはcall temporary終了後も存続するcell guardを保持します。RFC 0007の非escape規則に従い、user functionからcaller所有cellへのloan exportを拒否し、有効な入れ子scalar writeは許可します。追加11件のsource testに独立した権限oracleと比較する124生成read／write pathを含み、2件のCLI／query testで診断とrevision-safeなparameter由来を検証します。全648 test、fmt／Clippy／doc、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功しました。Reviewでは21/113完了ptと元の全outcomeを維持します。Loan factsは実行を許可せず、valueのarena brand／escape flowと手続き間region authority、ownership MIR／layout／cleanup／core／runtime統合は未完了です。HIR／MIR 8 ptとarena 5 ptは未完了のままです。次はvalue、constructor、local、patternのarena brandを追跡し、call／escape authorityと検証済みMIR transitionへ進めます。RFC 0008の判断、ownership serialization、platform／debug／conformance／performance gateは独立して残します。

Value regionのfollow-up（2026-10-17 JST）：commit `3e6563a` はloan検査後に上限付きの関数内arena-brand flowを追加し、field／sumの精度、入れ子reference cellのalias、core collection storage／projection、同一regionの循環payload、別arenaのauthority検査、字句的escape診断、canonicalなloop固定点を扱います。Caller由来はopaqueで、collectionは削除後も可能なbrandを保守的に保持します。Regionを含むguest callは明示的に未完了とし、手続き間の置換／effectを実装するまでchecked bodyや実行の証拠を付与しません。15件のsource testに独立scope oracleと比較する20 lexical-store programを含み、2件のCLI／query testでsource診断、上限、revision-safeなarena由来を検証します。追加loan test 1件でparent凍結を維持しつつ `vec_get_mut` の暫定mutable authorityを修正しました。全666 test、fmt／Clippy／doc、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功しました。Reviewでは21/113完了ptと元の全outcomeを維持します。HIR／MIR 8 ptとarena 5 ptには手続き間region authority、検証済みownership MIR、runtime generation／stalenessとteardown、source実行の統合が引き続き必要です。次はguest callのbrand置換とborrowed storage mutationのsummaryを実装し、再帰effectとcaller lifetimeを検査してから検証済みMIR transitionへ進めます。RFC 0008の判断、ownership serialization、platform／debug／conformance／performance gateは独立して残します。

Guest regionのfollow-up（2026-10-17 JST）：commit `b39b96a` は型付きcaller projection、call前snapshot、borrowed storageの最終値summary、途中のstore制約によって未完了call gateを削除します。独立した上限付き最小固定点で再帰／相互再帰callの通常returnと全可能storeをモデル化し、writeを捏造せず、strong clearと通常returnしないcall前の検査を維持します。Call診断はcaller／arena／callee由来を保持します。追加14件のsource testに二つのguest callを介して独立scope oracleと比較する20 programを含み、2件のCLI／query testで診断spanとrevision更新後の復帰を確認します。再解析、source保持、正確なsummary work／allocation上限が、全682 testのfmt／Clippy／doc baseline、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeとともに成功しました。Reviewでは21/113完了ptと元の全outcomeを維持します。Source region解析はguest effectまで扱うようになりましたが、ownership MIR／layout／drop elaboration、core／runtimeのVM／native統合、arena generation／staleness／teardownは未完了です。HIR／MIR 8 ptとarena 5 ptは未完了のままです。次はRFC 0004に従うruntime arena storage、generation／provenance検証、決定的な逆allocation順teardownを実装し、適用される受理済みcontractに従ってownership MIRとcleanupへ接続します。RFC 0008の判断、ownership serialization、platform／debug／conformance／performance gateは独立して残します。

Runtime arenaのfollow-up（2026-10-17 JST）：commit `02f349d` は単独arena owner、retainせずcopyできるnode reference、同一regionのcyclic payload、generation／provenance／index検査、arena全体のread登録、失敗時にinputを維持するatomicなallocation／replacement、access無効化後の追加allocation不要な逆順drainを実装します。保持するread／retired snapshotは旧値と新値のreplacement peakも含めresident storage quotaに残り、古いreaderは再利用したregionへ影響しません。22件のfocused testに、独立oracleと比較する1,000件の生成graph sequence、128件のclose／read解放race、20,001 nodeのcycleの反復teardown、再入host unwind、limit／exhaustion／poison／failure境界を含みます。全704 testのfmt／Clippy／doc baselineが成功しました。Reviewでは21/113完了pt、元の113 pt総計、残る全outcomeを維持します。Arena 5 ptには検証済みownership MIRとsource VM／native統合、typed layout、guest cleanup／allocator適応が必要であり、このhost engineだけでは完了しません。Arc allocationはabortし得て、resident byte quotaはmetadata／caller所有storageを除外し、論理retirement数はguest destructorのevidenceではありません。次は受理済みRFC 0004／0007に従う内部ownership MIR contractとvalidationを構築します。Cleanup／allocator／FFI（RFC 0008）とserialized ownership encodingは、それぞれの公開contractを実装する前の独立した判断として維持します。Platform／debug／conformance／performance gateは変更しません。出典：受理済みRFC、実装／test、[architecture evidence](architecture.md#runtime-arena-storage)。更新／検証日：2026-10-17 JST。

Ownership MIR型のfollow-up（2026-10-17 JST）：commit `c6847a6` はbackend／frontend非依存のメモリ上ownership type graphと、変更不可のregion検査済みHIRを受け取るsource adapterを追加します。Nominal／constructed identity、lexical owner symbol、変更不可snapshot、付け替えたbyte spanを保持し、独立したMIR検証でgraph参照、inline再帰、storage／privacy／Map制約、上限付きの推移的Copy／drop／loan／arena／allocator／owner属性を確認します。Compiler生成のread-loan sumを表現できますが、heap／nominal fieldへの格納は許可しません。13件のfocused testに、独立Copy／drop oracleで照合する200件の生成graph、400 definitionのchain、source／MIR一致と再変換、不正入力、構築と検証の正確な累積budgetを含みます。全717 testとfmt／Clippy／docが成功し、新たなlock済みfuzz targetもClippyとinstrumentationなし10,000-run smokeを通りました。Coverage-guided fuzzの主張は行いません。Reviewでは21/113 pt、元の総計、残る全outcomeを維持します。型の事実だけではbody宣言／signature、instruction／place／CFGのownership遷移、layout、cleanup、core operation、VM／native実行を検証できません。この前提作業だけではHIR／MIR 8 pt、arena 5 pt、coreのoutcomeは完了しません。次は明示move／loan遷移を含む検証済みownership function／place／CFG contractを構築し、callable型の比較に先立つconstructed typeの同値判定も加え、その後source loweringへ接続します。RFC 0008のowner判断とownership wire encodingは独立した決定として残し、Copy bytecode schema 1とplatform／debug／conformance／performance gateは変更しません。出典：受理済みRFC 0004／0007、実装／test／fuzz target、[architecture evidence](architecture.md#in-memory-ownership-mir-types)。更新／検証日：2026-10-17 JST。

Ownership MIR宣言のfollow-up（2026-10-17 JST）：commit `97672e4` は、nominal、loan kind、owner symbolのidentityを保つbudget付きconstructed type正規化／同値判定、独立検証するfunction／import／local宣言、明示引数型の検査を追加します。Region検査済みsourceのloweringはfunction／local index、binding kind、名前、spanを保持します。Owner symbolは対応するfunction／localのowner宣言へ正確に解決する必要があり、signature検証はescapeするloan／arena／allocator／owner型、public interface中のprivate型、user parameter中のcompiler専用loan sumを拒否します。既存host importはscalarのままです。追加13 testに、80件の展開key同値oracle、同一hash bucketの異なる32 key、不正な宣言／call境界、正確な累積budget、source再変換、異なるfunctionのarena identityを含みます。全730 testのfmt／Clippy／doc baselineが成功しました。Docには古い生成済みMIR metadataの再構築が必要でしたが、source側の回避策は不要でした。拡張したlock済みfuzz targetはClippyとinstrumentationなし10,000-run smokeを通過し、coverage-guidedの主張は行いません。Reviewでは21/113完了pt、113 pt総計、残る全outcomeを維持します。これらの宣言と型検査は、実行可能body、move／loan／region flow、allocator、cleanupのauthorityを与えないため、HIR／MIR 8 pt、arena 5 pt、core／runtime統合outcomeは完了しません。次はtyped ownership placeと明示instruction／CFG遷移を検証し、そのcontractに従って検査済みsource bodyをlowerします。RFC 0008の判断、ownership serialization、source実行、platform／debug／conformance／performanceの全gateは独立して残します。出典：受理済みRFC 0004／0007、実装／test／fuzz target、[architecture evidence](architecture.md#ownership-mir-declarations)。更新／検証日：2026-10-17 JST。

Ownership MIR placeのfollow-up（2026-10-17 JST）：commit `51759b3` は、独立検証するfunction内local／value root、明示的な通常loan dereference、record field、source／type origin、静的access queryを追加します。Copy read、whole move、partial／borrowed move拒否、型が一致するreplacement、nested shared／exclusive permissionの交差、authorityのborrowとmove／replacementの区別をmemory accessなしで表現します。Region検査済みsourceの正規化はexpression indexとbyte spanを保持し、自動field dereferenceを展開し、temporary loan rootを維持します。追加14 testに60通りのpermission oracle path、不正metadata／root／projection、正確な累積上限、source再変換、nested／temporary／arena fixtureを含みます。全744 test、fmt／Clippy／doc、lock済みfuzz Clippy、instrumentationなし10,000-run smokeが成功しました。Docには再度生成済みMIR metadataの再構築が必要でしたが、source側の回避策やcoverage-guidedの主張はありません。Reviewでは21/113完了pt、元の総計、残る全outcomeを維持します。Typed value descriptorはinstruction定義ではなく、静的permissionはloanの正しい生成を前提とし、initialization、有効なloan／cell provenance、lifetime、region／escape flow、実行を証明しません。そのためHIR／MIR 8 ptとarena 5 pt、core／runtime統合は未完了です。次は明示ownership instructionとCFG／value定義を定めて検証し、合成pattern／payload pathも含むsource loweringとownership dataflowへ接続します。RFC 0008の判断、ownership wire encoding、cleanup／layout、VM／nativeのsource実行、platform／debug／conformance／performance gateは独立して残します。出典：受理済みRFC 0004／0007、実装／test／fuzz target、[architecture evidence](architecture.md#ownership-mir-places)。更新／検証日：2026-10-17 JST。

Ownership MIR instruction／CFGのfollow-up（2026-10-17 JST）：commit `a3c5dce` はtyped scalar／Unit／aggregate／core case constructor、Copy／move／place操作、initialization／replacement／drop、borrow begin／end identity、arena入退場、call／import、CFG block／引数を追加します。独立した構造検証で、operation／result型、placeのvalue-root useを含む定義と支配関係、到達可能な型付きedge、loan生成site、payload Copy／borrow／whole unpack前の一致するcase-test true-edge guardを確認します。既存Copy verifierも同じ上限付きdominator algorithmを共有し、ownership wire tagは追加しません。追加13 testに120件の独立CFG dominator oracle、22種すべてのcore case shape、non-Copy loop引数、不正operation／definition／guard、正確なbudget、手動で組み立てるsource owned-return fixtureを含みます。全757 testとfmt／Clippy／docが成功しました。新しいlock済みbody fuzz targetもClippyとinstrumentationなし10,000-run smokeを通り、coverage-guidedの主張は行いません。Reviewでは21/113完了pt、元の総計、残る全outcomeを維持します。`TypedBodies` は実行できず、ownership-flowによる拒否を待つ型付き二重moveを意図的に受け入れます。自動の全source-body lowering、core operation実装、意味論的なVM／native等価性は未完了です。そのためHIR／MIR 8 ptとarena 5 ptは未完了のままです。次はtyped edge transferとloop状態復元を伴う独立CFG initialization／affine consumption解析を実装し、その後loan／cell／region／call flow、RHSのeffectをまたぐassignment destination準備、完全なsource loweringへ進みます。RFC 0008の判断、cleanup／layout、ownership serialization、runtime／source統合、platform／debug／conformance／performance gateは独立して残します。出典：受理済みRFC 0004／0007、実装／test／fuzz target、[architecture evidence](architecture.md#typed-ownership-instructions-and-cfg)。更新／検証日：2026-10-17 JST。

Ownership MIR initialization／moveのfollow-up（2026-10-17 JST）：commit `9cf81bd` はdefinite initialization、local／SSAのaffine consumption、選択edgeでの同時引数transfer、再初期化、reducible／irreducible cycleのloop状態復元を独立したCFG解析で検証します。変更不可の結果はblock-entry状態、以前の消費の可能性があるspan、return時のlive／maybe-live ownerのcleanup obligationを保持します。Resource計上はtyped stageの実際の累積使用量を再開し、state matrixにも個別の上限を設けます。追加12 testに、独立single-owner oracleと比較する160件の生成操作列、branch join、non-Copy swap、arena activation、診断、正確なbudget境界を含み、source owned-return組み立てfixtureもmove検査を通ります。全769 testとfmt／Clippy／docの全基準が成功しました。拡張したlock済みbody fuzz targetもClippyとinstrumentationなし10,000-run smokeを通り、coverage-guidedの主張は行いません。Reviewでは21/113完了pt、全18 outcome、元の総計を維持します。`MoveCheckedBodies` は実行できず、reference cellの初期化検査は有効loanや参照先aliasの妥当性を証明せず、cleanup obligationの記録もcleanupを実装しません。HIR／MIR 8 ptとarena 5 ptは未完了です。次はRHSのeffectをまたぐassignment destination準備も含め、MIRの有効loanと推移的reference-cell依存を検証し、その後region／call effectと完全なsource loweringへ進みます。RFC 0008の判断、cleanup／allocator／layout contract、ownership serialization、core／runtimeのVM／native統合、platform／debug／conformance／performance gateは独立して残します。出典：受理済みRFC 0004／0007、実装／test／fuzz target、[architecture evidence](architecture.md#ownership-mir-initialization-and-moves)。更新／検証日：2026-10-17 JST。

Prepared destinationのfollow-up（2026-10-17 JST）：commit `9a57680` はownership MIRのdestination capture、replacement publication、破棄を分離し、RHSのeffectによって元のreference cellを読み直す必要が生じない表現にします。Verifierはcapture identity／shape、静的replacement permission／型、capture siteの支配関係、累積上限を検査します。Move解析はcaptureの確実なavailability、一度だけの消費、return時の処理完了、RHS loopをまたぐpending destinationを含むloop状態復元を独立検証します。追加10 testに81件の独立した経路消費diamond oracleを含み、追加1件の検査済みsource組み立てfixtureは自動loweringや実行を主張せずdestination-before-RHS順序を保持します。全780 testとfmt／Clippy／docが成功し、拡張したlock済みbody fuzz targetもClippyとinstrumentationなし10,000-run smokeを通りました。Coverage-guidedの主張はありません。Reviewでは21/113完了pt、18 outcome、元の全acceptance境界を維持します。Captureのlifecycleは有効loan、推移的reference-cell依存、RHS effect後の参照先aliasの妥当性、region／call effectを証明しません。HIR／MIR 8 ptとarena 5 ptは未完了です。次は明示capture destinationに対してMIRのloan／provenanceを検証し、その後完全なsource／core loweringとruntime統合へ進みます。RFC 0008の判断、cleanup／allocator／layout contract、ownership serialization、VM／native／platform／debug conformance、performance測定は独立gateとして残します。出典：受理済みRFC 0007、実装／test／fuzz target、[architecture evidence](architecture.md#prepared-ownership-assignment-destinations)。更新／検証日：2026-10-17 JST。

MIR loan activityのfollow-up（2026-10-17 JST）：commit `fd5f050` はmove検査後に明示loanのactivation／end状態を独立検証します。Required endには確実なactivityが必要で、conditional endは条件付きactivityを冪等に解消し、通常returnは全明示loanを終了し、feedback edgeはreducible／irreducible cycleでheader状態を復元します。Move／activity解析は同じbudget付きDFS走査を共有します。変更不可のfactはblock-entry activityを保持し、累積計上はmove stageの実際の使用量を再開します。追加10 testに81件の具体path end oracle、条件付き生成／end、guard付きpayload loan、loop、診断、replay、正確なresource上限を含み、新しい検査済みsource組み立てfixtureはlexical begin／read／endを供給します。全791 testとfmt／Clippy／docが成功しました。Docは生成済みMIR metadataの再構築後に成功し、source側の回避策はありません。拡張したlock済みbody fuzz targetもClippyとinstrumentationなし10,000-run smokeを通り、coverage-guidedの主張は行いません。Reviewでは21/113完了pt、18 outcome、元の全acceptance境界を維持します。`ActivityBodies` は実行できず、activation状態は参照先／aliasの妥当性、推移的cell依存、parent freezing、caller-parameter loanの妥当性、終了済み／古いloop参照の拒否を証明しません。型付きuse-after-end fixtureはprovenance stageで拒否すべき対象として明示的に残します。そのためHIR／MIR 8 ptとarena 5 ptは未完了です。次はMIRのreference provenanceと依存を解決し、RHS effect後のcapture destinationも含むaccess／parent／終了済み参照の規則を検証し、その後region／call effectと完全なsource／core／runtime loweringへ接続します。RFC 0008の判断、cleanup／allocator／layout contract、ownership serialization、VM／native／platform／debug conformance、performance測定は独立gateとして残します。出典：受理済みRFC 0007、実装／test／fuzz target、[architecture evidence](architecture.md#explicit-mir-loan-activation-and-end-state)。更新／検証日：2026-10-17 JST。

MIR reference provenanceのfollow-up（2026-10-17 JST）：commit `4c6c13e` はloan activityの後に独立したcell／target／依存解析を追加し、local／SSA／caller reference chain、whole-owner aliasの排他性、parent freezing、依存end、move／sum／joinを通じた無効化、条件付きreference、loopでの古い生成site identityを扱います。Prepare済みdestinationは評価済み参照先を保持し、RHS effect後の終了済みguardを拒否します。直接fieldのpublicationにもparent storageの初期化が必要です。Nested cellの借用とpointee accessを区別し、guest callは入れ子の引数referenceを通じたshared／exclusive permissionの交差を検証します。Caller所有reference cellへlocal loanは格納できません。追加17 testに96件の独立owner-interval oracle program、正確な累積／depth上限、reducible／irreducible loop、payload view、70 loanのbit境界を含み、追加1件のprepare済みfield regressionも成功します。検査済みsource組み立てfixtureは3件すべてこのstageを通り、以前activityだけを通っていたuse-after-end fixtureは拒否します。全809 testとfmt／Clippy／docが成功しました。Lock済みbody fuzz targetもClippyとinstrumentationなし10,000-run smokeを通り、coverage-guidedの主張は行いません。Reviewでは21/113完了pt、18 outcome、元の全acceptance境界を維持します。`LoanCheckedBodies` は実行できず、core operationとprovisional／arena result contract、arena／call brand effect、自動の完全なsource lowering、cleanup／layout、sourceのVM／native統合は未完了です。そのためHIR／MIR 8 pt、arena 5 pt、core outcomeは未完了のままです。次はこれらのresultを表すため、受理済みcore callのtyped MIR contractを追加し、その後arena／call effectを検証して完全なcontractへ検査済みsource bodyをloweringします。RFC 0008の判断、allocator／cleanup／FFI interface、ownership serialization、platform／debug／conformance、performance測定は独立gateとして残します。出典：受理済みRFC 0004／0007、実装／test／fuzz target、[architecture evidence](architecture.md#ownership-mir-reference-provenance-and-access)。更新／検証日：2026-10-17 JST。

MIR core callのfollow-up（2026-10-17 JST）：commit `4619937` はRFC 0007の全34 operationの独立契約、provisional result-loan由来、具体arena-owner fact、再帰instance-allocator要件を追加しました。11件のMIR testで独立に具体化した102 signature、全31 non-arena operationの解析経路、arena new／read／replaceをつなぐflowを確認します。2件のsource testでoperation adapterとsource由来allocator callの明示組み立てを確認します。Fmt／Clippy／test／docのfull baselineは822 testすべてpassし、fuzz workspaceのformatと全binのlocked Clippyもpassしました。Body targetの非instrumented 10,000回smoke実行も完了しました。Coverage campaignとは主張しません。

このscope後のroadmap確認：M2は26 / 113 pt、全18成果中4成果完了のままです。次の実装対象は、既存type／place／move／loan契約を使うchecked sourceからMIR bodyへの自動変換とsourceの統合evidenceです。Arena brand／effect検証、cleanupとfallible allocation、owned layout、serialization、実行可能なcore operation、残るplatform／conformance／performance gateを明示的に維持します。113 ptの見積もりと受け入れ境界は妥当です。実験段階の委任で通常のRFC承認待ちをなくし、実装要件は維持します。

自動ownership bodyのfollow-up（2026-10-17 JST）：commit `39bb20e` は、owned local／return、nominal／core constructor、明示／暗黙／reborrow引数、guest／host／core call、prepare付きassignment、if／while flow、arena開始／終了について、手作業の命令組み立てをchecked sourceからtyped MIRへの自動変換に置き換えました。条件付きlocal／SSA discardはactive loan検証を維持して移動済み／maybe-live storageを正規化し、返却するSSA ownerを保持します。追加9件のsource testが独立したmove／activity／provenance解析を通り、構築／検証の正確な累積上限も確認します。Fmt／Clippy／test／docのfull baselineは831 testすべてpassし、fuzzのformat／全bin locked Clippyと非instrumented body smoke 10,000回もpassしました。

このscope後のroadmap確認：26 / 113 pt、全18成果中4成果完了を維持します。次のbody統合対象はmatch patternのloweringとprovisional core readの成功／失敗armごとのlifetime処理です。現在、到達可能なmatchは明示的に拒否します。実初期化順の破棄とexceptional cleanupはRFC 0008の独立したdrop成果であり、独立arena brand／effect検証、実行用ownership layout、allocation、serialization、backend／platform作業も未完了です。既存gateと113 pt見積もりは引き続き妥当です。正常flowの自動変換は具体的な統合進捗であり、これらの広い成果を完了扱いにはしません。

Ownership frontendの完了（2026-10-18 JST）：commit `ad40b7a` は、借用／入れ子参照scrutineeとprovisional core-result armを含むRFC 0007のpattern／match自動loweringを完成させました。Pattern検査後にbindingを作り、不一致viewと未使用／失敗core leaseをarm body前に終了し、成功loan bindingの祖先関係を保持し、Map keyはcall後に終了します。Shared／exclusive loan引数はreborrowします。Provisional result全体のbindingは内部sumを公開せず `SRC0020` を報告します。Ownership専用の `Unreachable` exitでもmove義務と全loan終了を維持し、serialized Copy schemaは変更しません。追加9件のsource testと拡張arena fixtureが全解析経路を通り、独立Copy oracleは43件の返却値／faultを照合します。Fmt／Clippy／test／docのfull baselineは840 testすべてpassし、fuzz format／全bin locked Clippyと非instrumented body smoke 10,000回もpassしました。

8 ptのHIR／MIR成果の完了確認：受理済みsource type／declaration／place loweringと全expression／statement／pattern形式が、独立検証するtyped CFG、definite initialization／affine move、prepare済みdestination、明示loan activity、推移的reference provenanceへつながりました。既存source compile-pass／fail、type／CFG／resource-limit、stale alias／parent freezing、生成oracleのsuiteと、新しいmatch統合testをfrontend解析のevidenceとします。この成果を完了にし、M2は34 / 113 pt（30.1%）、全18成果中5成果完了、roadmap全体は332 / 764 pt（43.5%）となります。RFC 0007全体のimplementedやowned実行の有効化は主張せず、driverの実行profileはCopyのみです。Arena統合／独立MIR brand・effect、実初期化順／failure cleanup、fallible guest allocation／owned layout、core runtime operation、version付きownership artifact、unsafe／C、backend／platform／performanceは独立gateを維持します。113 ptの見積もりは引き続き妥当です。次はdriverのownership解析interfaceを統合し、続いてowned program実行に必要なcleanup／layout／allocation契約を実装します。

Ownership解析のdriver統合（2026-10-18 JST）：commit `d25706c` は、成功したownership HIR／MIR proofを`lang_semantics::analysis`、共通frontend、revisionを検査するquery jobに保持します。`lang check [--source-root DIR] FILE`で両source profileを検査でき、`main`やhost bindingのないlibraryも対象です。[Ownership example](../../examples/analysis/ownership.lang)もこのcommandで成功します。Ownership MIRのresource limitは元sourceを指す`SRC0011`診断になります。`lang run`とtransactional REPLの実行境界は明示的にCopyのみです。追加7 testでこれらの経路、source span、cache無効化／stale ticket、拒否itemの非publicationを検証しました。Workspace全847 test、format、Clippy、documentation生成、fuzz format／全bin locked Clippy、独立exampleが成功しました。Evidence：[architecture](architecture.md#source-analysis-profileとdriver境界)とリンク先の実装／testの更新／検証日：2026-10-18 JST。RFC 0007／0008はacceptedを維持し、最終更新日は2026-10-17です。

見直し：完了済みownership frontend解析をuserとeditor callerへ提供しましたが、別のM2成果は完了していません。M2は34 / 113 pt（30.1%）、全18成果中5成果、roadmap全体は332 / 764 pt（43.5%）を維持します。現在のscope、依存関係、113 ptの見積もりは引き続き妥当です。次は実際の初期化／再初期化順とfailure exitを保持するcleanup elaborationを実装し、続いてowned layoutとfallibleなinstance allocationを実行可能core operationへ接続します。正常flowのstorage-discard命令と非実行の解析proofだけではこれらの成果を満たしません。Ownership artifact、独立したarena effect、backend／platform conformanceと残るgateは未完了です。

Cleanup runtime統合の基盤（2026-10-18 JST）：commit `c437dc1` は`lang_runtime::cleanup`を追加し、実際のpayloadを所有して、上限付きlexical／call scope内で初期化成功の逆順を追跡します。Moveは義務を移転し、再初期化は末尾へ追加し、置換は新しいownerをcommitしてから旧値をcleanupします。Entryと入れ子callのreturnはcleanup成功までowning resultを保留し、失敗時は未公開resultもcleanupします。Ownerより先にborrow registrationを終了し、失敗後も独立したcleanupを続けます。順序付きreportは既定で256 recordを事前確保しoverflow数を記録し、cleanup failure／contain済みhost panicはinstanceをpoisonします。Panicしたcallbackのvalueと悪意あるsecondary panic payloadは再帰的unwindをせず隔離します。16 runtime testで実際のshared owner解放、入れ子call、limit／failure／poison挙動と、独立oracleによる256 stepの生成列100件を検証しました。Workspace全863 test、fmt／Clippy／doc、fuzz format／全bin locked Clippyが成功しました。Evidence：[cleanup architecture](architecture.md#cleanup-runtimeとfailure-reporting)、[engine](../../crates/runtime/src/cleanup.rs)、[test](../../crates/runtime/tests/cleanup.rs)の更新／検証日：2026-10-18 JST。RFC 0008の最終更新日：2026-10-17。

見直し：source／MIRのscopeとfailure exitがこのruntime遷移を駆動し、aggregate field／temporaryの実構築順を保持し、実行adapterで受理済み意味論を実証するまでdeterministic drop成果は未完了です。Engineのfallibleなhost metadata確保はguest allocatorのaccounting完了を意味しません。現在のscope／依存関係と113 pt見積もりを維持し、M2は34 / 113 pt（30.1%）、全18成果中5成果、全体は332 / 764 pt（43.5%）のままです。次はscope／activation境界、aggregate初期化順、生成failure exitを含むownership MIRのcleanup elaborationをこのengineへ接続し、続いてowned layout／fallible core allocationとVM／native実行を実装します。既存Copy互換性と、残るarena、ownership artifact、unsafe／C、packaging、conformance、platform／debug、performanceのgateも引き続き必要です。

生成cleanup rangeの接続（2026-10-18 JST）：commit `6216b43` はownershipの自動loweringを変更し、個別source spanを持つlexical／temporary `Op::Cleanup` rangeを出力して、返却owning registerをrange外に保持します。独立したtype／move／activity／provenance検査で、累積上限の下で対象と遷移を検証します。`MirLayout`と`Frame::cleanup_mir`が対象を宣言済みruntime cellに対応付け、effect前にrange全体をpreflightし、Copy／vacant valueを除外してregistrationを終了し、metadata確保をせず選択ownerを動的な初期化順でcleanupします。追加5 testには、source生成MIRのtransfer／cleanupに限定したinterpreterと実際のshared valueを使った検証を含みます。再初期化後は宣言逆順の`1, 2, 3`ではなく`3, 1, 2`でcleanupし、cleanup failure時は未公開owning resultを解放します。不正target／span、verifierの厳密な予算境界、重複target、range preflight失敗も対象です。Workspace全868 test、fmt／Clippy／doc、fuzz format／全bin locked Clippyと、拡張body targetの非instrumented smoke 10,000回が成功しました。Evidence：[cleanup range architecture](architecture.md#生成mir-cleanup-range)とリンク先実装／testの更新／検証日：2026-10-18 JST。RFC 0008の最終更新日：2026-10-17。

見直し：source生成のcleanup rangeがruntime engineへ接続されましたが、検証を限定したtest interpreterによってownership VM実行やdeterministic dropのconformanceが完了したとはしません。M2は34 / 113 pt（30.1%）、全18成果中5成果、全体332 / 764 pt（43.5%）を維持し、scope、依存関係、113 pt見積もりは引き続き妥当です。次はownership MIRにaggregate field／temporaryの実構築順を保持し、生成runtime failure exitをelaborateして、続いてowned layout／fallible allocationとproduction VM／native adapterを実装します。独立arena effect、version付きownership artifactと、残るunsafe／C、packaging、conformance、platform／debug、performanceのgateは未完了です。

Record構築順の保持（2026-10-18 JST）：commit `a984e5e` は、ownershipの`Op::Record`へ検証済み初期化permutationを追加し、物理operandは宣言順を維持します。Source loweringは記述順を記録し、不正な長さ／重複／ordinalはverifierの厳密な累積上限の下で失敗します。Runtimeの`RecordOrder`は検査済みconstructor metadataを借り、initializer spanと確保なしの逆順field drainを提供します。消費済みfieldは再試行できず、iterationを止めても未訪問fieldの所有は残ります。`Frame::take_owner`は検証／予約済みaggregate storageへの転送を支援し、registrationのexportを拒否します。追加6 testは、source生成MIRのrecord／transferに限定したinterpreterで実際のshared resourceを使います。3 fieldの記述順全6通り、入れ子／Copy field、同じnominal typeでconstructor順が異なる2つの値、aggregate返却／caller cleanup、部分drain、空record、不正metadata、span、厳密な予算境界、export guardを検証しました。Workspace全874 test、fmt／Clippy／doc、fuzz format／全bin locked Clippyと、拡張record／body targetの非instrumented smoke 10,000回が成功しました。Evidence：[record cleanup architecture](architecture.md#record構築とfield-cleanup順)とリンク先実装／testの更新／検証日：2026-10-18 JST。RFC 0008の最終更新日：2026-10-17。

見直し：recordのfield順序がnominal layoutとは独立して構築／move／返却を通して保持されました。このmetadata統合部分は完了しましたが、deterministic dropやownership実行の成果は未完了です。生成failure exit、production cleanupでの順序付きfield failure報告、owned layout／fallible guest allocation、VM／native adapterが残ります。M2は34 / 113 pt（30.1%）、全18成果中5成果、全体332 / 764 pt（43.5%）を維持し、scope、依存関係、113 pt見積もりは引き続き妥当です。次は入れ子fieldのfailure後も処理／報告を続けるfailure pathとcleanup contextを実装し、続いてfallible allocationとowned実行を接続します。独立arena、ownership artifact、unsafe／C、packaging、differential／platform／debug、performanceのgateも引き続き必要です。

入れ子cleanup failureの報告（2026-10-18 JST）：commit `9678072` は生成／登録済み`Cleaner`向けに共有する上限付き`Context`を追加し、通常callbackの互換性を維持します。Child cleanupは呼び出し前に義務を消費し、callback／payloadのpanicを個別にcontainして、fieldのinitializer spanと親slot／reasonを保持し、child用reportを確保せず順序付きfailureを追加します。`RecordOrder::cleanup_fields`は最後までdrainを続け、execution failureをprimaryに維持し、overflow時は省略record数を数え、cleanup failureでinstanceをpoisonします。追加6 testは、3 recordのbufferで300 fieldが失敗するケース、親spanの復元、悪意あるpanicのdisposal、事前予約した1 work itemで2万段のhost所有shapeを反復処理するケースを含みます。検証を限定したsource生成MIRのrecord interpreterでも、予約済みの反復traversal storageを使い、実際のshared owner解放、入れ子initializer診断、未公開resultのcleanupからの全failureを検証しました。Workspace全880 test、fmt／Clippy／doc、fuzz format／全bin locked Clippyが成功しました。Evidence：[context architecture](architecture.md#contextを共有するfield-cleanup報告)とリンク先実装／testの更新／検証日：2026-10-18 JST。RFC 0008の最終更新日：2026-10-17。

見直し：runtimeの報告境界は入れ子failureに対応しましたが、生成guest failure edgeとproduction ownership実行は未実装です。Deterministic drop成果は未完了とし、M2は34 / 113 pt（30.1%）、全18成果中5成果、全体332 / 764 pt（43.5%）を維持します。現在のscope／依存関係と113 pt見積もりは引き続き妥当です。次はownership MIRのruntime failure cleanup経路を生成し、続いてowned layoutとnormal allocation時のretirement予約をfallible guest allocationおよびVM／native実行へ接続します。予約はguest effect前に完了する必要があり、failure後にcleanup用traversal storageを代替確保してはいけません。独立arena effect、version付きownership artifact、unsafe／C、packaging、differential／platform／debug、performanceのgateも引き続き必要です。

Ownership failure CFGのelaboration（2026-10-18 JST）：commit `3c3131f` はloan検査済みMIRの後にprivateなstatus CFGを追加し、共通source解析へ保持します。Entryのpreflightはparameter受け入れより前に行い、guest命令／terminatorには厳密なfuel guardを置き、生成cleanupには課金しません。Returnは末尾cleanupより前にresultを保留します。明示failure経路は原因の保持／記録、prepare済みdestinationのdiscard、activationの処理、未公開resultのdiscard、failure伝播を行います。到達不能経路はcleanupより先にinvariant failureを記録します。Runtimeの段階的exitはcaller／calleeの所有を維持し、再試行を避け、entry全体のabort時はpending callee resultをcaller ownerより先に処理します。型付きErrは通常の成功resultとし、元source spanとCopy検査の互換性を維持します。追加11 testで実際のshared ownerと生成経路を使い、厳密なfuel、置換失敗、entry拒否、branch／loop、invariant優先、入れ子伝播／中断、replay、厳密なresource上限を検証しました。Workspace全891 test、fmt／Clippy／doc、fuzz format／全bin locked Clippyと、拡張bodyの非instrumented smoke 10,000回が成功しました。Evidence：[failure architecture](architecture.md#ownership-mirのfailure経路)とリンク先実装／testの更新／検証日：2026-10-18 JST。RFC 0006／0008の最終更新日：2026-10-17。

見直し：compilerがfailure／return制御フローを生成し、runtimeが対応するcleanup遷移を提供するようになりました。実行用owned layout、normal allocation時のretirement予約、全value／operationをつなぐproduction実行を完成させるまで、deterministic drop成果は未完了とします。M2は34 / 113 pt（30.1%）、全18成果中5成果、全体332 / 764 pt（43.5%）を維持し、scope／依存関係と113 pt見積もりは引き続き妥当です。次はowned value layout／drop descriptorと、retirement metadata予約を伴うfallible instance allocationを実装し、続いてcore operationとVM／native実行を接続します。独立arena effect、version付きownership artifact、unsafe／C、packaging、differential／platform／debug、performanceのgateも引き続き必要です。検証を限定したtest backendによってownership VMや両platformのnative conformanceが完了したとはしません。

Fallibleなinstance allocator基盤（2026-10-18 JST）：commit `d7f85de` は明示的provider権限、一意なallocator identityとwrapしないgeneration handle、size／alignmentと個別request／live／cumulative／count上限の検査、事前確保した定数時間のlocator管理、`InUse`を伴う権限取り消し、元の正確なrequestによるteardownを追加します。認可済みOOM試行はlive／countを返金し、cumulative課金を維持します。Zero-size allocationもcallbackなしでidentity／countを保持します。Growは元のallocatorでprepare／copyしてから公開し、失敗時は元のownerを保持します。別allocatorへのcloneは明示操作です。Safe Rustのsystem providerはalignment 4096までの連続aligned storageを提供します。追加16 testでこれらの遷移、不正provider storage、data／metadataのpreflightとrollback、counter枯渇、正確なpointer／size／alignmentによる解放、provider lifetimeを検証しました。独立ledgerはOOMとslot再利用を含む256 stepの生成列を100件照合します。Workspace全907 test、fmt／Clippy／doc、fuzz formatと全bin locked Clippyが成功しました。Evidence：[allocator architecture](architecture.md#fallibleなinstance-allocator基盤)とリンク先実装／testの更新／検証日：2026-10-18 JST。RFC 0008の最終更新日：2026-10-17。

見直し：今回は正確なstorageを扱うallocator基盤です。Source layout adapterには引き続きtype／regionの検証、guest制御のdata／header／retirement allocationすべてのrequest、部分planのrollback、生成cleanup／failure CFGへの接続が必要です。既存shared／arena storageの変換も未完了です。残る13成果を未完了のまま維持し、M2は34 / 113 pt（30.1%）、全18成果中5成果、全体332 / 764 pt（43.5%）とします。Scope／依存関係と113 pt見積もりは引き続き妥当です。次はこのallocatorを使うowned value layout／drop descriptorとnormal allocation時のretirement予約を実装し、続いてcore operationとproduction VM／native実行を接続します。独立arena effect、version付きownership artifact、unsafe／C callback、packaging、differential／platform／debug、performanceのgateも引き続き必要です。

Managed owned layoutとretirement予約（2026-10-18 JST）：commit `f68a4da` は検証済みownership型からprivateなsize／alignment、field／variant、drop descriptorを反復的かつ累積予算内で生成します。Record offsetは宣言順とし、値ごとのmetadataは実際の初期化順、active enum case、initializer spanを保持します。Payload、order状態、反復work storageを公開前にまとめてpreflight／予約し、dataのOOM時はmetadataをrollbackします。検査済みMIRの構築順／spanを一時vectorなしで取り込みます。Retirement前にactive shape全体を検査し、Copy holeを飛ばし、各leafをcallback前に消費して、共有cleanup Contextで個別error／panic後も続行し、最後に両regionを解放します。追加10 testはcore／error layout、test token registryで実際のshared ownerへ接続するsource record全6構築順、入れ子variant、case変更、別allocation／古いlocation、厳密な予算／replay、指数的overflow、cleanup中に確保しない1,024段のinline shapeを検証します。Workspace全917 test、fmt／Clippy／doc、fuzz format／全bin locked Clippy、拡張type targetの非instrumented smoke 10,000回が成功しました。Evidence：[owned layout architecture](architecture.md#managed-owned-layoutと予約済みretirement-storage)とリンク先実装／testの更新／検証日：2026-10-18 JST。RFC 0007／0008の最終更新日：2026-10-17。Smokeはsanitizer／coverage-guidedのevidenceではありません。

見直し：owned inline layoutとnormal allocation時のretirement storageを実装しましたが、indirect tokenのlayoutはliveness／region／ownership権限を与えません。Production service adapterにはtokenの検証／消費と、動的descendant用retirement容量の事前予約が必要です。Deterministic dropとsource core-library成果は未完了とし、M2は34 / 113 pt（30.1%）、全18成果中5成果、全体332 / 764 pt（43.5%）を維持します。残る13成果、scope／依存関係、113 pt見積もりは引き続き妥当です。次はallocatorを使うBytes／Text storageとallocatorを維持する変換をproduction token adapterへ接続し、続いて動的collection／shared／arenaのretirementと全owned opcodeの実行を進めます。独立arena effect、version付きownership artifact、VM／native conformance、unsafe／C callback、packaging、platform／debug、performanceのgateも引き続き必要です。

Allocatorを使うBytes／Text runtime（2026-10-18 JST）：commit `82ed11b` はcheckedなbyte index／変更、allocatorを維持するgrow、UTF-8変換、正確なbyte length／等価性、clone、TextからBytesへのcopy、連結を実装します。Header／capacityも実際のfallibleなinstance storageを使います。UTF-8変換はdomainの成功／失敗の両方でBytesを消費し、成功時は確保なしで元のstorage／allocator／generationを維持します。GrowのOOM／上限到達では元のownerとcumulativeの試行課金を維持し、別allocatorへのcopyはtargetへ明示的に課金します。Private tokenの受け入れではidentity、generation、kind、reserved bit、header／request範囲、Textのvalidityを検証します。Production用Bytes／Text cleanup serviceはmanaged aggregateのleafを直接解決し、deallocation前に消費します。追加13 testは、128 stepのbyte／UTF-8 oracle生成列100件と、host token registryなしで実際のbufferを解放するsource由来record／enum cleanupを含みます。Workspace全930 test、fmt／Clippy／doc、fuzz format／全bin locked Clippy、新しい`core_buffers` targetの非instrumented smoke 10,000回が成功しました。Smokeはsanitizer／coverage-guidedのevidenceではありません。Evidence：[buffer architecture](architecture.md#allocatorを使うbytestextとtoken-cleanup)とリンク先実装／testの更新／検証日：2026-10-18 JST。RFC 0007／0008の最終更新日：2026-10-17。

見直し：Bytes／Text runtimeとleaf cleanup adapterは受理済みoperationを実装しましたが、source実行にはmanagedなUnit／Option／Resultのmarshallingとproduction ownership VM／native dispatchが必要です。Text／Bytesとdeterministic drop成果は未完了とし、M2は34 / 113 pt（30.1%）、全18成果中5成果、全体332 / 764 pt（43.5%）を維持します。残る13成果、scope／依存関係、113 pt見積もりは引き続き妥当です。次は有限result／aggregate valueとowned実行経路を接続してsource Bytes／Text programを実行できるようにし、続いてVec／Map／shared／arenaのpayload／retirement adapterと残りのoperationを追加します。独立arena effect、version付きownership artifact、native／platform conformance、unsafe／C callback、packaging、debug、代表的performanceのgateも引き続き必要です。既存の`lang run`／REPLによるownership拒否が現在のdriver境界です。

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

