<!-- translation-of: ../roadmap.md; canonical-language: en -->

# 実装ロードマップ

[English](../roadmap.md)

このロードマップはrisk reductionの順に並んでいます。Milestoneはcalendar commitmentではなくexit criteriaです。

## 現在の作業と完了済みM2

**検証済み133件 · 進行中0件 · 残り0件（暫定0 pt）。**

M2は完了済みです。18/18ゲート、過去のゲート重み113/113です。[完了ボード](milestone-2.md)に成果と見積もり収支、[機能ガイド](milestone-2-capabilities.md)に実際のsource/VM/native/C/SDK/debugの根拠と制約を記録しています。この件数はM2のもので、将来の全作業ではありません。

**M3 は進行中です。** [実装 Todo](milestone-3.md)は **58 done · 1 doing · 14 todo · 43 remaining pt**です。E14は`078e9b4`（2026-10-25 15:46 JST）で完了し、再開proofが元ledgerで失敗prefix/固有costを保持し、ledger checkpointで最初のentryを一度だけ受理します。canonical scopeも元ledgerの識別を保持します。重点60件・budget scope 3件・runtime単体171件、入力固定の **2,255件**のbaseline、fmt/Clippy/doc、全15 fuzz targetのcompile/lintが成功し、830入力の前後hashが一致しました。**消化3 pt・追加/再見積り0 pt**、73行、台帳 **110 + 91 − 158 = 43**、gate **18/96 weight・3/15**。残る公開C result escrow/拒否の監査と不足fixtureに **M3-E02（3 pt）**を選択します。公開操作は28個でquery/完全table未公開、E03/E04/L04・E09・E01全32操作・installed/P1・両host・最終guideを維持します。2026-10-25 JST更新。

E08のcaller resource退役・E10の計量・E11の元allocator view・E02のprivate child実行にはsourceの証拠があります。公開全32操作には検証済みcallbackの完成・永続provider lifetime・合算control・native AOT/SDK統合が必要です。**E02 → E03 → E09 → E04**の後にcontrol/I08を完了し、E01を公開します。B01の両host適合にもE01完了が必要です。元table/allocator・明示accounting・source/MIR・grant・code ownerを再利用します。

## 進行中 milestone の実行

M3 の source/task/channel/service 契約は受理済みで、対応する VM/native profile で実行します。RFC 0027 の IDL/codec・I09 の opaque 型/拒否 proof・I06 の独立 source/MIR binding・I04 の生成・I05 の rich 交渉・P01 の typed grant/live binding 受理を、記録した scope で実装しました。R01/R02/R03/R04/R05/E05/R06 により実 resource 引数/結果・直接 loan・native backing view・instance lifecycle を実行します。I10/I11/I07 により schema 5 の決定と保存 task/channel/service/component 実行を実装しました。P02 で root/canonical adapter の元 service-work 計量を実装しました。E06 で登録 owner の保持を実装しました。E07でscoped program/保持resource接続を実装しました。native-registry checkpointで明示C grantと元owner/contextによるcallbackを接続しました。次は **E02 → E03 → E09 → E04**、共有controlとI08、続いて完全なE01公開とB01/B02へ進みます。canonical graph/signature・version policy・既存 code-owner lifetime pattern を再利用し、prototype metadata だけを live binding と扱いません。残工数の積み上げ見積りと96点の exit-gate weight は別に扱います。M2 計測を維持し、Z01 で実 concurrency/boundary cost を計測します。

保守やレビューのために完了済みM2ボードを拡大しません。修正はscopeを限定したchecklistに記録し、code、文書、検証を同じ変更にまとめます。新たな言語／ABI／形式の選択には引き続き明示的RFC判断が必要ですが、実験段階の委任により個別owner承認待ちは不要です。

## タスク管理

未規定のsource、ABI、package、serialized behaviorを導入する実装taskは、そのbehavior実装前に受理済みRFCを必要とします。Decision taskはownerまたは[RFC 0001の実験段階の委任](rfcs/0001-project-charter.md#実験段階のrfc判断)に従うagentのdisposition記録で完了し、設計が拒否された場合は依存実装を完了扱いにせず再計画します。各成果に応じたcompile-pass／fail、不正入力／resource-limit、VM／native差分testを実行します。非信頼環境へ公開する前にdecoder／verifierのCPU・allocation budgetが必要です。

各milestone内のtaskはdependencyを考慮した優先度順に並べます。すべての `P0` taskがcheck済みで、exit evidenceが記録されるまでmilestoneはexitできません。未完了の `P1` taskはexit前に移動先とcostを明示的に再設定し、暗黙に消してはいけません。Check済みtaskは完了です。前のmilestoneで完了したprototype groundworkだけでは、後続milestoneのより広いtaskを完了扱いにしません。

以下のmilestone checklistは相対的な受入条件の重みを保持します。`1`はごく小、`2`は小、`3`は中、`5`は大、`8`は非常に大です。重み付きの達成率は `完了した重み / 全重み × 100` を小数第1位へ丸めます。この粗い重みは経過時間や実装残作業を予測しません。Milestoneがcurrentになったとき、またはscope／evidenceが変わったときに、長期見積もりを積み上げで見直します。進行中milestoneごとに別の実装Todoを使い、独立検証可能な残り1〜3 ptのtaskと、明示的なscope／見積もり変更を記録します。

2026-10-24（JST）に確認した exit-gate weight の snapshot です。受入条件の重みと残作業見積りは別であり、現在の積み上げ残数は上記の M3 実装 Todo を参照してください。M2 の完了記録は維持します。

| Milestone | 完了条件の重み | 条件の総重み | 条件達成率 |
| --- | ---: | ---: | ---: |
| 0 — design baseline | 209 pt | 209 pt | **100.0%** |
| 1 — frontendとbytecodeのvertical slice（完了） | 89 pt | 89 pt | **100.0%** |
| 2 — ownershipとnative AOT | 113 pt | 113 pt | 100.0% |
| 3 — concurrencyとembedding | 18 pt | 96 pt | 18.8% |
| 4 — developer toolchain | 0 pt | 63 pt | 0.0% |
| 5 — ecosystemとcompatibility | 0 pt | 65 pt | 0.0% |
| 6 — セルフホスティング | 0 pt | 74 pt | 0.0% |
| 7 — 追加frontend | 0 pt | 55 pt | 0.0% |
| **全体** | **429 pt** | **764 pt** | **56.2%** |

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


### 完了根拠

Milestone 0で完了したのは受理済み設計baselineとprototypeであり、後のproduction実装ではありません。[過去の監査](roadmap-history.md#evidenceと現在の状態)に元の根拠とbenchmarkの留保を保持しています。現在のsource/runtime機能は以下のM1/M2で示します。

Exit達成: 設計baselineの受理、cross-mode prototype、初期backendの根拠。

## Milestone 1：frontendとbytecodeのvertical slice（完了）

**進捗：89 / 89 pt（100.0%）。**

Ownerは2026-10-17に[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)を受理し、commit `031575e` に英日両方のdispositionを記録しました。受理済みの5つの設計gateが16 ptの完了分で、字句規則、module／call、aggregate／pattern、bytecode schema 1、上限付きframe、script／REPL behaviorを対象にします。Lossless parsing、module loading／resolution、source全体のtyping、nominal layout、pattern、source MIR、portable artifact、上限付きVM実行、初期scalar native conformanceは実装済みです。CLI／REPL、formatter、LSP統合も実装／検証済みです。

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


### 完了根拠

M1の24成果はすべて完了し、ゲート重みは89/89です。[M1ガイド](milestone-1.md)に2026-10-17のsource/VM/CLI/REPL/formatter/LSP受け入れ根拠、[M2ガイド](milestone-2-capabilities.md)にその後の所有権と全native対応を記録しています。M1時点のnative制約は過去の状態です。

Exit検証済み: 小さなプログラムを決定的にscriptとして実行でき、有用なIDE診断を得られます。

## Milestone 2：ownershipとnative AOT

**Milestone 2完了：18/18 gate、履歴上の重み113 / 113 pt（100.0%）、最終capability guide／全出口監査Z01も完了。**

**完了成果記録: [M2ボードと見積もり収支](milestone-2.md)。** 以下のchecklistは元の全受け入れ条件を保持し、M2の残作業はありません。

開始条件：RFC 0006の実装済みcopy-value profileとcompatibility fixtureを基準とします。RFC 0004のstate modelだけでは欠けていたsource／core、cleanup、allocator、C境界の受理済み要件は、現在RFC 0007／0008で規定しています。新たな設計決定が必要なbehaviorは受理後に実装します。実装Todoに2026-10-18の残作業積み上げ見直しを記録しました。

- [x] **P0 — Sourceのownership／borrow／arena profile、Copy分類、最小core-library型表面（text／byte、collection利用、必要なgeneric構文を含む）を受理する。** — 5 pt
- [x] **P0 — 未規定のerror／panic cleanup、unsafe／FFI validity、allocator contractを公開interface実装前に受理する。** — 5 pt
- [x] **P0 — Typed HIR／MIRでowned value、move、lexical borrow checkingを実装する。** — 8 pt
- [x] **P0 — RFC 0004のshared／weak retain-release、retainを伴うweak upgrade、atomic cycle-preflight／publicationを実装しrollback／race testを追加する。** — 8 pt
- [x] **P0 — RFC 0004のlexical arena、escape拒否、stale-reference確認、deterministic teardownをcompile-fail／runtime test付きで実装する。** — 5 pt
- [x] **P0 — 受理済みのreturn／error／panic cleanup規則に従ってdeterministic dropをelaborateする。** — 8 pt
- [x] **P0 — RFC 0007のText／Bytes source operation、UTF-8検証、checked index、allocatorを保持する変換を実装する。** — 5 pt
- [x] **P0 — Core source operationに必要なRFC 0007のUnit／Option／Resultと有限built-in specializationを実装する。** — 3 pt
- [x] **P0 — RFC 0007のallocator-aware Vec source operationをmove／loan／growth／failure conformance付きで実装する。** — 5 pt
- [x] **P0 — RFC 0007のallocator-aware Map source operationを厳密key equalityとreplacement／failure conformance付きで実装する。** — 3 pt
- [x] **P0 — 狭いunsafe boundaryを定義し、FFI validity ruleを強制する。** — 5 pt
- [x] **P0 — C ABI import／exportとgenerated headerを実装する。** — 8 pt
- [x] **P0 — RFC 0006のcopy-value profile全体（guest call、boolean interface、record、enum、pattern）をdifferential fixture付きでnative codeへlowerする。** — 8 pt
- [x] **P0 — macOS AArch64／Windows x86-64向けownership-aware source-program object generationとnative link／run conformanceを完成させる。** — 8 pt
- [x] **P0 — Linker discovery、startup／runtime packaging、production error handlingをhardeningする。** — 5 pt
- [x] **P0 — 代表的なVM／native differential conformance suiteを構築する。** — 8 pt
- [x] **P0 — 代表的native workloadを同等のRust／C実装に対して計測する。** — 8 pt
- [x] **P1 — 両tier-1 platformでsource-span native debug metadataをemit／validateする。** — 8 pt

レビューによる訂正（2026-10-20、解消済み）：旧G18根拠に不足していたRFC 0012のowned論理debugger呼び出し列を実装し、[両host run 39427222132](https://github.com/carimatics/lang/actions/runs/39427222132)（73dca1d）で検証しました。[更新したguide](milestone-2-capabilities.md)に3段の論理frameと親のcall siteの根拠を記録しました。

G18検証（2026-10-20 JST）：exact product `1496b8b`の[両host実debugger検証](https://github.com/carimatics/lang/actions/runs/39413674593)が成功しました。LLDB／CDBでCopyの2→3行step・2／6／9行caller、ownedのinstruction step／実runtime caller、42／正常終了をassertします。対応SDKによる手動debug linkでsourceを保持し、source変数・論理continuation step・CLIのdebug sidecar自動出力はこの証拠の範囲外です。[最終日英guide](milestone-2-capabilities.md)で全18 gateを監査し、Z01も完了しました。

Exit：代表的なcommand-line programがVM semanticsと一致し、同等のRust／C実装に対してperformanceが計測されている。

## Milestone 3：concurrencyとembedding

**進捗：18 / 96 pt（18.8%）。**

開始条件は充足済みです。Milestone 2 は両実行経路の所有権/cleanup semantics を提供し、リポジトリレビューも完了しました。RFC 0003/0005 を受理済み意味論の基準とし、prototype で production IDL/scheduler/sandbox を完了扱いにしません。[進行中の実装 Todo](milestone-3.md)で範囲と見積りを追跡します。source VM の既存 frame/live-cell 上限を task、native code、host callback 全体へ拡張します。

- [x] **P0 — RFC 0005を緩和せず、残るscheduler／channel／backpressure／error contract、send／share導出、source async構文を受理する。** — 5 pt
- [x] **P0 — Production component IDL／wire profileとembedding ABIを受理し、compatibility、typed failure、callback reentrancy、RFC 0003へのcancellation対応を決める。** — 5 pt
- [ ] **P0 — Structured task lifetimeとdeterministic cancellation cleanupを実装する。** — 8 pt
- [ ] **P0 — Bounded backpressure／cancellation behavior付きchannelを実装する。** — 5 pt
- [ ] **P0 — Actor isolationとaccept済みdata-race capability ruleを強制する。** — 8 pt
- [x] **P0 — Version付きcomponent IDLとbounded serializationを実装する。** — 8 pt
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
- [ ] **P0 — `rustup` と同じ役割を持つinstall／update／channel toolを構築する。** — 8 pt

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

