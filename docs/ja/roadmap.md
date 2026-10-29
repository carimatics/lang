<!-- translation-of: ../roadmap.md; canonical-language: en -->

# 実装ロードマップ

[English](../roadmap.md)

このロードマップはrisk reductionの順に並んでいます。Milestoneはcalendar commitmentではなくexit criteriaです。

## 完了済みM2とM3

M2 delivery記録: **検証済み133件 · 進行中0件 · 残り0件（暫定0 pt）。**

M2は完了済みです。18/18ゲート、過去のゲート重み113/113です。[完了ボード](milestone-2.md)に成果と見積もり収支、[機能ガイド](milestone-2-capabilities.md)に実際のsource/VM/native/C/SDK/debugの根拠と制約を記録しています。この件数はM2のもので、将来の全作業ではありません。

**M3は完了しました:** [完了board](milestone-3.md)は**79 done · 0 doing · 0 todo · 0 remaining pt**、**15/15 gate・96/96 gate weight**です。[機能ガイドと全出口監査](milestone-3-capabilities.md)に実行command・両host証拠・制約をまとめました。`9e78f2d`のmacOS全7段階（153 suite / 2,321成功）と[Windows全CI](https://github.com/carimatics/lang/actions/runs/40471721760)（153 suite / 2,306成功）が失敗0で成功し、旧C client移行も両方で成功しました。SDK隔離起動7 profile、X01 plugin、Z01 cost/M2 parity証拠を保持します。最終Z02の2 ptを消化し追加/再見積り0、台帳は**110 + 99 − 209 = 0**です。P0/P1の先送りはありません。

公開C v1と既存生成clientはstabilized RFC 0033に従います。source/native language ABIとprivate SDK/保存formatの独立互換境界は実験段階です。M2の完了記録・過去のbenchmarkは未変更で、M3の計測は別sourceの有限観測として記録しています。

## M3完了後の作業

M3 は完了済みです。M4 の [作業ボード](milestone-4.md) は 42 delivery、**27 done · 1 doing · 14 todo · 42 remaining pt**。editor 最終 head `5cdc422` の両 host CI／debugger 成功と PR #18 merge（`0a15fb5`）を含め G1–G6 を検証済みです。D01 source `8421298` が `lang dap` の保持 VM source-breakpoint／stack／source 実行を実装し、D02 source `ced8e56` の VM step／inspection が全 local baseline に成功し、D03R Windows debugger 前提 `99a20de` は native workflow 40907199433（3/3）に成功し、D03 production native source `39473b1` は workflow 40918693919（3/3）に成功し、D04B source `67f62fa`がtrusted live mixed bridgeを実装し、D04 source `759bae0`がnative-debug CIの3環境に成功し、D05 lifecycle監査がdoingです。native／混在 debug 全体（G7）、profiling（G8）、署名 installer（G9）、最終 journey／機能 guide は引き続き必要です。

PR #9–#18 は merge 済みです。local main／origin/main ref は `0a15fb5` で一致し、ahead／behind 0/0。現在の `codex/m4-debug-adapter` は clean・upstream 同期済み `codex/m4-editor-checked-edits` head `5cdc422` から作成し、既存 user 変更はありません。記録 PR base は #18 merge 後に `carimatics/lang:main` へ移行しました。upstream は `origin/codex/m4-debug-adapter`、[draft PR #19](https://github.com/carimatics/lang/pull/19) の D01 両 host CI／debugger は成功し、現在の証拠と残る条件は board に記録しています。

保守やレビューのために完了済みM2ボードを拡大しません。修正はscopeを限定したchecklistに記録し、code、文書、検証を同じ変更にまとめます。新たな言語／ABI／形式の選択には引き続き明示的RFC判断が必要ですが、実験段階の委任により個別owner承認待ちは不要です。

[2026-10-28のrepositoryレビュー](review-2026-10-28.md)で、古いRFC follow-upを完了済みM2／M3証拠と照合し、保守修正を別途記録しています。日付付きcheckpointに残る「Windows未検証」などの過去記述で完了済み成果を再開しません。M4～M7のgate重みは変更せず、残作業の見積もりとも扱いません。

## タスク管理

未規定のsource、ABI、package、serialized behaviorを導入する実装taskは、そのbehavior実装前に受理済みRFCを必要とします。Decision taskはownerまたは[RFC 0001の実験段階の委任](rfcs/0001-project-charter.md#実験段階のrfc判断)に従うagentのdisposition記録で完了し、設計が拒否された場合は依存実装を完了扱いにせず再計画します。各成果に応じたcompile-pass／fail、不正入力／resource-limit、VM／native差分testを実行します。非信頼環境へ公開する前にdecoder／verifierのCPU・allocation budgetが必要です。

各milestone内のtaskはdependencyを考慮した優先度順に並べます。すべての `P0` taskがcheck済みで、exit evidenceが記録されるまでmilestoneはexitできません。未完了の `P1` taskはexit前に移動先とcostを明示的に再設定し、暗黙に消してはいけません。Check済みtaskは完了です。前のmilestoneで完了したprototype groundworkだけでは、後続milestoneのより広いtaskを完了扱いにしません。

以下のmilestone checklistは相対的な受入条件の重みを保持します。`1`はごく小、`2`は小、`3`は中、`5`は大、`8`は非常に大です。重み付きの達成率は `完了した重み / 全重み × 100` を小数第1位へ丸めます。この粗い重みは経過時間や実装残作業を予測しません。Milestoneがcurrentになったとき、またはscope／evidenceが変わったときに、長期見積もりを積み上げで見直します。進行中milestoneごとに別の実装Todoを使い、独立検証可能な残り1〜3 ptのtaskと、明示的なscope／見積もり変更を記録します。

2026-10-29（JST）に確認した exit-gate weight の snapshot です。受入条件の重みと残作業見積りは別であり、現在の収支は live M4 board に記録します。完了済み M2／M3 の記録は維持します。

| Milestone | 完了条件の重み | 条件の総重み | 条件達成率 |
| --- | ---: | ---: | ---: |
| 0 — design baseline | 209 pt | 209 pt | **100.0%** |
| 1 — frontendとbytecodeのvertical slice（完了） | 89 pt | 89 pt | **100.0%** |
| 2 — ownershipとnative AOT | 113 pt | 113 pt | 100.0% |
| 3 — concurrencyとembedding（完了） | 96 pt | 96 pt | 100.0% |
| 4 — developer toolchain | 39 pt | 63 pt | 61.9% |
| 5 — ecosystemとcompatibility | 0 pt | 65 pt | 0.0% |
| 6 — セルフホスティング | 0 pt | 74 pt | 0.0% |
| 7 — 追加frontend | 0 pt | 55 pt | 0.0% |
| **全体** | **546 pt** | **764 pt** | **71.5%** |

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

**進捗: 96 / 96 pt（100.0%）。**

開始条件は充足済みです。Milestone 2 は両実行経路の所有権/cleanup semantics を提供し、リポジトリレビューも完了しました。RFC 0003/0005 を受理済み意味論の基準とし、prototype で production IDL/scheduler/sandbox を完了扱いにしません。[完了した実装 Todo](milestone-3.md)で範囲と見積りを追跡します。source VM の既存 frame/live-cell 上限を task、native code、host callback 全体へ拡張します。

- [x] **P0 — RFC 0005を緩和せず、残るscheduler／channel／backpressure／error contract、send／share導出、source async構文を受理する。** — 5 pt
- [x] **P0 — Production component IDL／wire profileとembedding ABIを受理し、compatibility、typed failure、callback reentrancy、RFC 0003へのcancellation対応を決める。** — 5 pt
- [x] **P0 — Structured task lifetimeとdeterministic cancellation cleanupを実装する。** — 8 pt
- [x] **P0 — Bounded backpressure／cancellation behavior付きchannelを実装する。** — 5 pt
- [x] **P0 — Actor isolationとaccept済みdata-race capability ruleを強制する。** — 8 pt
- [x] **P0 — Version付きcomponent IDLとbounded serializationを実装する。** — 8 pt
- [x] **P0 — Canonical owned／borrowed resource adapterを実装する。** — 8 pt
- [x] **P0 — Capability-gated component importを強制する。** — 5 pt
- [x] **P0 — Typed error付きmixed native／VM call／callbackをsupportする。** — 8 pt
- [x] **P0 — Native／VM boundaryを越えてstack trace／cancellationを保持する。** — 5 pt
- [x] **P0 — Host-callback accountingを含むheap／allocation limitを強制する。** — 5 pt
- [x] **P0 — Recursion-depth／live-task limitを強制する。** — 5 pt
- [x] **P0 — VM、JIT、AOT、callback boundary全体でcooperative interruptionを強制する。** — 8 pt
- [x] **P0 — 両tier-1 platformでcross-modeのownership／failure／capability／resource-limit conformanceを確認後、C embedding APIとgenerated bindingをstable化する。** — 8 pt
- [x] **P1 — Trusted-native／verified-script plugin exampleを提供する。** — 5 pt

Exit：Native hostがuser scriptをloadし、typed APIを公開し、scriptをcallし、callbackを受け取り、IPCなしでcapabilityを強制できる。

## Milestone 4：developer toolchain

**進捗：39 / 63 pt（61.9%）。**

有効化監査（2026-10-29 JST）: 現在の task、実行 checkpoint、積上げ baseline 103 pt は [M4 board](milestone-4.md)で管理します。[RFC 0034](rfcs/0034-package-workspaces-and-cli-diagnostics.md)が manifest／diagnostic と最初の local package 契約を採択しました。厳密な検査、許可 local dependency の実行、lock／offline replay、create／build／run／test fixture の順に進めてから広範な tooling へ進みます。installer は M5 registry より先に toolchain binary を配布するため、trust root、署名 release metadata 検証、atomic update／failure recovery の採択設計を download／install 前に必要とします。M5 signing は registry package 公開／provenance を対象とします。元の九つの gate と weight を維持し、63 pt を残作業予測には使いません。

順序の契約（2026-10-28 JST に明確化、有効化時も維持）: 最初の package fixture で、採択 manifest／diagnostic 契約に従う package 作成、local dependency 1 件、build／run／test、完全一致 offline lock replay を扱います。依存 I/O 前に package source 権限と checksum／拒否動作を定義し、M5 公開 registry を仮定しません。editor／debug は既存 parse、format、query 無効化、native／VM source mapping を再利用します。広範な tooling 基盤より先に次の利用者 fixture と失敗経路を実証します。installer 検証は download／install の前提であり、registry 公開と信頼方針は M5 です。有効化だけではこれらの実装 gate は完了しません。

- [x] **P0 — Version付きpackage／workspace manifest、target condition、lockfile互換性、machine-readable CLI diagnostic契約を受理する。** — 5 pt
- [x] **P0 — Deterministic dependency resolution、lockfile、checksum、offline modeを実装する。** — 8 pt
- [x] **P0 — 既存の`lang run` frontendをpackage対応build／run／test orchestrationへ拡張し、受理済みstructured diagnostic出力を提供する。** — 5 pt
- [x] **P0 — Package、fmt、lint、doc commandをCLIへ統合する。** — 5 pt
- [x] **P0 — LSP navigation、completion、semantic tokenを実装する。** — 8 pt
- [x] **P0 — 安全なrename／code actionを実装する。** — 8 pt
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

