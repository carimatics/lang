<!-- translation-of: ../../../docs/rfcs/0003-versioned-component-contracts.md; canonical-language: en -->

# RFC 0003：Version付きcomponent contractとcapability import

[English](../../../docs/rfcs/0003-versioned-component-contracts.md)

- Status：proposed
- Authors：Project contributors
- Created：2026-10-16
- Last updated：2026-10-16
- Tracking issue：
- Supersedes：
- Superseded by：

## 概要

Package identity、scalar function import／export、各importが要求するcapabilityを記述する、backend非依存でversion付きのcomponent contractを定義します。Embedding固有のhost-function identifierはportable metadataの外に置きます。実行前にcontractを明示的なcapability grant、論理import binding、型付きhost-function registryに対して検証し、検証済みwrapperまたはstructured errorを生成します。

実行可能prototypeは `i64 -> i64` host importをsupportし、`i64`／`bool` scalar function typeを記録し、boundedな実験的schema-1 scalar-metadata codecと、明示的なshared／exclusive borrow tokenを持つgeneration tag付きowned-resource tableを提供します。このcodecはdesign evidenceであり、stableなserialized formatでもWebAssembly Interface Types（WIT）実装でもありません。

## 動機

数値 `HostFunctionId` registryは型付きcallbackを実証しますが、portable component identity、名前付きimport／export、compatibility version、authority requirementを表現しません。またprocess-localな数値IDをcomponent metadataへ入れると、portableであるべきcontractが1つのembedding instanceへ結合されます。

Resource handle、canonical lowering、stable C embedding APIを固定する前に、component boundaryの狭い実行可能modelが必要です。本提案は最初の分離を確立します。Portable declarationがrequirementを命名し、embedderはその名前をlocal host registrationへ独立してmapし、authorityをgrantします。

## 目標

- Metadata schema evolutionとcomponent package versioningを区別する。
- Component declarationをMIR、VM、JIT、AOT、process-local host IDから独立させる。
- すべてのhost importに明示的なcapabilityを要求する。
- 実行前にname、duplicate declaration、supported signature、grant、binding、registrationを検証する。
- 成功したvalidationをresolved local bindingを持つ別typeとして保持する。
- Rust referenceがboundaryを越えることに依存せず、stale、repeatedly released、illegal borrowed resource handleを拒否する。
- Runtime handle layoutを公開せず、canonical scalar／resource call representationとnormal-path ownership transferを規定する。
- Trust boundaryでstructuredかつdeterministicなfailureを返す。

## 非目標

- Serialized IDLまたはwire encodingをstable化すること。
- WIT source／binary compatibilityを主張すること。
- String、list、record、variant、option、result、async functionを定義すること。
- Live resource handleをpackage metadataへserializeすること、またはruntime handle bit layoutをstable化すること。
- Ownershipがcomponent boundaryを越えた後のpanic、unwind、cancellation、destructor-failure behaviorを定義すること。
- C embedding ABI、native calling convention、symbol namingをstable化すること。
- この段階でVM、JIT、AOT pathを通してcomponent exportを実行すること。
- Semantic-version compatibility rangeまたはdependency resolutionを定義すること。

## Guide-level design

Component declarationは概念上、次を含みます。

```text
schema = 1
package = example:counter@1.2.3

import read-clock: (i64) -> i64 requires clock.monotonic
export run: (i64) -> i64
```

提案するresource extensionは各call positionでownershipを明示します。

```text
resource timer
import inspect: (borrow<timer>) -> i64 requires clock.read
export adopt: (own<timer>) -> i64
```

`inspect` はcall-scoped loanを受け取り、`adopt` はtransfer commit成功時にcallerのownerをconsumeします。このnotationは説明用であり、最終的なIDL syntaxを選択するものではありません。

Declarationは `HostFunctionId` を含みません。Embedderは次を別に構成します。

```text
grant clock.monotonic
bind read-clock = host-function 7
```

Schema 1を理解でき、declarationが構造的にvalidで、すべてのbindingが宣言済みimportを指し、要求されたcapabilityがすべてgrantされ、すべてのimportにbindingがあり、bound host functionがsupport済みの型付きregistrationとして存在する場合にのみvalidationが成功します。得られるvalidated valueはportable declarationとresolved local IDを保持します。

Text例は説明専用です。以下で説明するrepository-local binary codecは実験用fixture formatであり、最終的なstable component serializationを選択するものではありません。

Component resourceは別のin-memory tableを使います。Owned handleはslot／generationを含み、untrusted codeが複製するinteger handleをmodel化するため意図的にCopy可能です。Componentはsharedまたはexclusive borrowed handleを要求できます。Shared borrowは共存でき、exclusive borrowには他のlive borrowがないことを要求し、mutationを許可するborrowed handleはexclusiveだけです。Ownerがresourceをreleaseする前にすべてのborrowを終了する必要があります。Releaseはhost valueを返し、slot generationを進め、再利用可能な最小slotを利用可能にします。Copyされたold ownerの再利用、borrowの二重終了、二重release、終了後borrowの使用はstructured stale-handle errorになります。

## Reference semantics

`ComponentMetadata` はschema version、package namespace、package name、3-part component version、import、exportを含みます。各function declarationはMIR／backend typeではなくcomponent固有のscalar typeを使います。各importはさらに1つのcapabilityを命名します。`ComponentImportBindings` は論理import nameをprocess-localな `HostFunctionId` valueへmapし、`CapabilityGrants` はこのinstanceへ供給されたauthorityを保持します。

Prototype identifierは保守的なASCII subsetを使います。Package／function identifierはsingle hyphenで区切られたlowercase alphanumeric segmentからなり、各segmentはlowercase letterで始まります。Capability identifierはdotで区切られた1個以上の同種identifierからなります。これはproject-localなprototype grammarであり、完全なWIT identifier grammarについての主張ではありません。

Validationはdeterministicで、次の順序で行います。

1. 後続fieldを解釈する前にunsupported metadata schemaを拒否する。
2. Package、import、export、capability nameを検証し、duplicate import、duplicate export、import／export collision、current registryがsupportしないimport signatureを拒否する。
3. 宣言済みimportを命名しないembedding bindingを拒否する。
4. 宣言capabilityが存在しない最初のimportを拒否する。Unauthorized componentがvalidationを使ってregistered functionを発見できないよう、host lookupより前に行う。
5. 各importに1つのbindingを要求し、local host functionが未登録のbindingを拒否する。
6. Metadataとresolved local bindingを持つ `ValidatedComponentMetadata` を返す。

Schema 1が検証できるimportは `i64 -> i64` のみです。これはcurrent host registryが持つshapeがそれだけだからです。Contract type systemをregistry method nameで定義しないためmetadata typeは `bool` を記録しますが、同じ型のregistry entryとexecution pathが存在するまで `bool -> bool` importは拒否します。この段階のexportはdeclarationのみです。

実験的scalar-metadata encodingは `LCMP` で始まり、little-endian `u32` schema version `1`、length-prefixed UTF-8 package namespace／name、3つのlittle-endian `u32` package-version part、length-prefixed import／export sectionが続きます。各functionは1-byte scalar tag（`i64` は `0`、`bool` は `1`）を格納し、importはさらにlength-prefixed capabilityを格納します。Encodingはdeclaration orderを保持します。`ComponentCodecLimits` はallocation前にtotal byte、sectionごとのdeclaration数、structural depth、stringごとのbyte数を制限します。Decoderはpayloadより先にunsupported schemaを拒否し、invalid magic／UTF-8、unknown scalar tag、truncation、trailing byte、すべてのlimit超過をbyte offset付きstructured errorで拒否します。Decode結果は未validateの `ComponentMetadata` であり、semantic／capability validationは別の必須stepです。

### Canonical scalar／resource call representation

**Source fact。** WITはunique ownershipを持つresource handleとborrowed handleを区別します。Owned handleを渡すとunique ownershipがtransferされ、borrowed handleはcall durationだけの一時loanです。WebAssembly Component Model Canonical ABIは両方のhandle kindを32-bit boundary indexへloweringし、handleのlift／drop時にresource identityとactive lendをcheckします。そのreference tableはremove済みentryが残したholeを再利用します。Scalar loweringでは `s64` に64-bit laneを使い、boolean loweringは `0` または `1` を生成し、liftingは `0` をfalse、すべてのnon-zero integerをtrueとして扱います。

**Project decision。** 最初のcanonical call subsetは `i64`、`bool`、`own<resource T>`、`borrow<resource T>` です。`i64` は1つの64-bit laneでtwo's-complement bitを保持します。`bool` は1つのunsigned 32-bit laneを使い、lowering時はcanonicalに `0`／`1` とし、lifting時は `0` をfalse、non-zero valueをtrueとします。Resource valueは1つのcomponent instanceにscopeされたadapter-owned tableへのunsigned 32-bit indexを使います。Index `0` はinvalidとし、それ以降のboundary indexはそのinstanceのlifetimeを通じて再利用もwrapもせずmonotonicにallocateします。設定済みallocation limitまたは残存index namespaceのexhaustionはstructured failureです。これはuntrustedなstale numeric copyが後続resourceをaliasできないようにするため、Canonical ABI reference tableのfree-list reuseと意図的に異なります。Adapterはinternal storage slotを再利用できますが、そのgeneration tag付きownerと再利用しないboundary indexは別物です。Adapter entryはvalidated resource-type identityを保持し、runtimeのgeneration tag付きowner／borrow tokenへresolveします。Boundary index、internal slot、generation、borrow identifier、host addressは相互変換せず、package metadataへserializeしません。Canonical `borrow<T>` が規定するのはlifetime／ownershipだけです。各operationがruntime tableのshared／exclusive tokenのどちらを取得するかは、後続memory／concurrency RFCで判断します。

各callはownershipを変更する前に、argument list全体へpreflightを行います。Preflightはfunction signature、capability grant、resource type identity、live generation、要求ownership mode、active lend、duplicate owned argument、1つのresourceをowned／borrowedの両方として提示するalias、設定済みlive／total-index limit、destination capacityをvalidateします。Borrowだけを繰り返すaliasは、後続memory／concurrency policyが要求access modeを許す場合に限って受け入れられます。Preflightはすべてのdestination entry／backing storageをreserveしますが、handleをpublishせずresource／ownership stateを変更しません。このためallocation／reservation failureでもcallerは不変です。Normal-path transferは次のruleに従います。

1. `own<T>` parameterのloweringは、caller adapterからlive ownerをexactly onceでremoveし、1つのtransfer commitの下でcallee adapterへfresh ownerをinsertします。Commitはreserved destination entryを使い、allocation、user code、callback、destructor invocationを行わず、failureになりません。Component-boundary operationはintermediate stateを観測できません。Boundary indexを再利用しないため、callerのold numeric indexのcopyは永続的にstaleです。Borrowed、wrong-type、already moved、actively lent、duplicate、borrow-aliased ownerはcommit前に拒否します。
2. `borrow<T>` parameterのloweringはownershipをcallerに残し、dynamic call scopeへ紐づくtokenを作成します。Calleeはそのtokenをrelease／transfer／returnできず、scope後まで保持できません。Nested callはchild scope内だけでreborrowでき、すべてのborrowはparent scope終了前に終わります。
3. `own<T>` resultのliftingはcalleeからcallerへ対称なpreflight／commitを行います。Destination ownerのinstall成功時だけcalleeのnumeric indexがstaleになります。
4. Borrowed resultはloanを証明するcall scopeより長く生存するため、最初のcanonical subsetから除外します。Owned boundary handleのdropはownerをexactly onceでconsumeします。Destructor invocation／failure behaviorは別のpanic／destructor taskで規定します。

Transfer commit前のfailureではownershipを変更しません。Adapterはpreflight reservation／transfer commitがactiveな間、callback reentrancyを防止する必要があります。Commit後のpanic、trap、cancellation、callback reentrancy、destructor failureには後続failure-boundary ruleが必要であり、ここでは推測しません。VM、JIT、AOT adapterは同じpreflight／table operationを呼び出す必要があります。Whole-program direct-call specializationはそれらのcheckとのobservational equivalenceを証明した場合だけ有効です。

**未解決hypothesis。** Per-instance adapter tableはVM／native behaviorをdeterministicにし、internal handle layoutのleakを防ぐはずですが、validation、reservation、monotonicに消費するindex namespace、table mutation、callback costはsource factではなくmeasurement対象です。

`ResourceTable<T>` はhost valueを所有します。`OwnedResourceHandle` は `(slot: u32, generation: u64)` です。`BorrowedResourceHandle` はさらにmonotonicに割り当てられたborrow identifierと `Shared`／`Exclusive` kindを持ちます。Released slotは再利用前にgenerationをincrementします。Generationを進められないslotはretireします。Free slotはslot昇順で選び、reuseをdeterministicにします。Borrow identifierは再利用せず、exhaustionはerrorです。

Resource operationは最初にslot occupancy／generationをvalidateします。Shared-borrow creationはactive exclusive borrowを拒否します。Exclusive-borrow creationはすべてのactive borrowを拒否します。Mutable accessはsharedまたはinactive tokenを拒否します。Borrow終了はそのtokenだけをremoveし、repeated endはstaleです。Releaseはactive borrowを拒否し、valueをexactly onceでremove／returnし、copy済みownerをすべてstaleにします。したがってCopy可能representationはcopy可能ownershipを意味しません。

## Execution mode

Contract／resource-table validationはbackend非依存で、VM、JIT、AOT executionを選ぶ前に実行します。Canonical callはすべてのmodeで同じpreflight、adapter-table transfer、dynamic-scope borrow ruleを使います。Prototypeはimportを `HostFunctionRegistry` に対してresolveし、runtime crate内でresource lifecycleを直接実行します。VM moduleのinstantiate、JIT call-site patch、adapter tableの実装、AOT import／resource lowering、export callはまだ行いません。将来のexecution pathはunchecked metadataを独立に再解釈せず、同じvalidated contract／resource ruleをconsumeしなければなりません。

## ABIと互換性

Metadata schema versionとpackage versionは別の意味を持ちます。Schema versionはcontractのshapeとvalidation ruleを選択します。Package versionは宣言されたcomponent revisionを識別します。Schema 1は正確なsupported schema valueを要求し、package versionを記録しますが解釈しません。

`HostFunctionId` はembedding-local bindingであり、portable metadataから意図的に除外します。Resource slot、generation、borrow identifierはruntime-local validation dataであり、portable representationではありません。提案するunsigned 32-bit canonical resource indexはadapter-table keyであり、runtime handle layoutでもserializable identityでもありません。このprototypeの `LCMP` scalar fixture、Rust layout、enum discriminant、in-memory map order、symbol name、説明用text syntaxはいずれもunstableです。Boundaryをstableと呼ぶ前に、accepted revisionでstable serialization、compatibility negotiation、resource-type identity encoding、failure-boundary ruleを選択する必要があります。

## Diagnosticsとtooling

Codecはbyte、declaration-count、nesting-depth、string-length limit、malformed header／length／UTF-8／scalar tag、unsupported schema、trailing byteについてstructured errorを返します。すべてのcodec errorがbyte offsetを含みます。Contract validationは別にinvalid identifier、duplicate name、namespace collision、unsupported function type、過剰／不足binding、missing capability、unknown host functionについてstructured error variantを返します。Resource operationはstale owned／borrowed handle、borrow conflict、in-use release、shared-borrow mutation、table capacity、boundary-index exhaustion、destination-reservation failure、borrow-identifier exhaustionを区別します。Human-readable messageはlocation、論理name、handle identityを含み、callerはtextをparseせずvariantをmatchできます。

Toolingはembedding configurationを診断する場合を除き、local numeric host IDではなくportable import nameを表示するべきです。Decode後のsemantic failureをbyte rangeへmapする作業は将来課題です。

## Performanceとcost model

Contract validationはduplicate／lookup checkにhash set／ordered mapを使い、平均してdeclaration／binding数に対してlinearです。Owned stringとresolved-binding mapを成功したvalidationごとに1回allocateします。Resource lookupはslotによりconstant timeで、shared-borrow tracking／deterministic free-slot selectionはordered setを使い、それぞれのset sizeに対してlogarithmicです。Call-site thunk generation、canonical representation conversion、cross-boundary resource dispatch、destructor overheadはこのprototypeの範囲外であり、stable化前のbenchmark requirementとして残ります。

## Securityとtrust boundary

Component metadata、capability declaration、binding configurationは、それぞれのboundaryでuntrusted inputです。Executionまたはlinker／JIT patchingより前にvalidationを完了しなければなりません。Authorityはembedderが供給するgrantから生じ、componentがcapabilityを宣言するだけでは得られません。Unauthorized componentへhost inventoryを露出しないよう、capability failureをhost-registry lookupより先に評価します。

Scalar codecはallocation前に設定済みlimitをcheckし、malformed inputをdeterministically拒否し、semantic validationとは独立してfuzz targetを持ちます。Validated wrapper／resource tableが証明するのは実装済みcheckだけです。Copy可能handleをuntrusted tokenとして扱い、すべてのoperationでgeneration／active-borrow stateを再validateし、instance内でboundary indexを再利用しません。Stale numeric valueの所持からauthorityを導出しません。Preflightはmutation前にowned aliasを拒否し、transfer commit内ではuser codeを実行しません。このprototypeはresourceをserializeせず、native callbackをsandbox化せず、callback workをpreemptせず、heap limitをenforceせず、failureをまたいでdestructorを実行せず、native pluginを安全にしません。Panic／unwind behaviorは最終的なcomponent boundaryで表現・封じ込める必要があります。

## 代替案

### 変更しない

Prototype codeへ数値host IDを直接渡し続けます。より小さい一方、portable contract、compatibility identity、明示的authorityを表現できず、process-local detailが長期boundaryへ漏れることを促します。

### Libraryまたはtooling solution

Runtime validationを変更せず、build toolingでad hoc binding tableを生成します。Ergonomicsは改善できますが、malformedまたはmismatched metadataが1つのauthoritative trust-boundary checkなしでruntimeへ到達します。

### WITとWebAssembly Component Modelを直ちに採用する

WIT package、world、resource、Canonical ABIを最初の実装として使います。既存のcross-language interface modelと明示的なimport／export contractを提供するため、これは最も強い競合designです。ProjectはWebAssemblyを唯一のcomponent representationとして選択しておらず、lowering対象のrich-value／resource runtimeもなく、direct native／script executionが1つのcontractを共有する必要があるため、まだ選択しません。Prototypeは互換性を主張せず、後からWITへmapできる概念を意図的に保持します。

### Flat C ABIを先にstable化する

C-compatibleなscalar callとsymbol nameだけを公開します。Current AOT objectにはより単純ですが、package-level versioning、capability requirement、rich value、resource ownership、共有VM／native component modelを提供しません。

### Raw slot indexまたはhost referenceを使う

Raw indexはより小さい一方、slot reuse後にold copyが別resourceを暗黙に指せます。Rust referenceはlocal borrowをencodeしますが、component dataから再構築された場合やABI callをまたいで保持された場合にtrustできません。Table-checked borrow tokenを持つgeneration tag付きinteger handleはhost addressを公開せず明示的なvalidationを維持します。

## 実装計画

1. Schema-1 in-memory scalar metadata、明示的capability grant、分離されたimport binding、host-registry resolution、structured validation errorを実装・test済み。
2. Generation tag付きowned／borrowed resource handleを実装・testし、stale handle、repeated release、repeated borrow end、borrow conflictをdeterministically拒否済み。
3. Deterministicな実験的schema-1 scalar-metadata encoder／decoderを実装・test済み。Byte／count／depth／string limit、byte-offset diagnostics、malformed fixture、libFuzzer targetを備える。Unsupported schema versionはpayloadより先に拒否し、stable serialization／forward-compatibility policyは未決定のまま残す。
4. 1つのvalidated contractをVM、JIT、AOT instantiationへ接続し、import／export差分testを追加する。
5. 最初のcanonical call subsetを提案済み。64-bit `i64`、32-bit `bool`、preflight済みnormal-path ownership transferを持つadapter-table `own<T>`／`borrow<T>` resource indexを規定する。Implementation／cross-mode conformanceは未完了。
6. Panic／error／destructor containmentとpackage／interface compatibility negotiationを規定する。
7. Stable boundaryをacceptする前にvalidation、instantiation、adapter conversion、steady-state call overheadをbenchmarkする。

## テスト計画

- Accepted schema-1 metadataとresolved local bindingのunit test。
- Unknown schema、invalid name、duplicate declaration、import／export collision、unsupported signatureのnegative test。
- Missing capabilityがunknown host registrationより先に報告されることを証明するtest。
- Missing、duplicate、undeclared embedding bindingのtest。
- Deterministic scalar-metadata encoding、明示的なbyte／declaration-count／nesting-depth／string-length limit、byte-offset diagnosticsのround-trip／malformed-fixture test。
- Bounded decoder向けlibFuzzer targetとdeterministic arbitrary-byte smoke corpus。
- 同じvalidated contractを使う将来のVM／JIT／AOT差分test。
- Generation-safe reuse、stale owner／borrow、repeated release／end、shared／exclusive conflict、mutation、deterministic reuse、identifier exhaustionのresource-handle test。
- Exact scalar lane、boolean normalization、wrong-type／stale／duplicate／borrow-aliased owner拒否、boundary indexの非再利用／exhaustion、ownership不変のreservation failure、atomicかつnon-reentrantなnormal-path owner transfer、call-scoped／nested borrow、borrowed result禁止、VM／JIT／AOT同一結果に関する将来のcanonical-call test。
- 将来のcross-mode resource lifecycle／capability-denial conformance test。
- Validation、instantiation、conversion、repeated callの将来benchmark。

## 未解決事項

- Serialized contractはWITを直接採用するか、compatible subsetをembedするか、WIT adapter付きproject-specific encodingを使うべきか。
- Embedderはどのpackage-version compatibility rule／negotiation algorithmを適用するべきか。
- 初期scalar／resource subsetの次にどのrich valueを追加し、どれを後続schemaへ送るべきか。
- Capability namespace、delegation、attenuation、ambient host policyをどうcomposeするべきか。
- Validated contractはdirect function pointer、immutable registry reference、generation-checked registry handleのどれを保持するべきか。

レビュー結果：**needs prototype/measurement**。Bounded scalar-metadata decoding、capability-gated binding、table-local resource lifecycle、normal-path canonical scalar／resource transfer ruleは規定済みですが、このRFCをstable component contractとしてacceptする前に、stable serializationの選定、指定済みadapter-table loweringのimplementation／VM／JIT／AOT conformance、compatibility negotiation、panic／destructor behavior、boundary-cost evidenceがblocking workとして残ります。

## 情報源

- WebAssembly Component Model「Why the Component Model?」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/design/why-component-model.html>
- WebAssembly Component Model「WIT」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/design/wit.html>
- WebAssembly Component Model「Worlds」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/design/worlds.html>
- WebAssembly Component Model「Canonical ABI」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/advanced/canonical-abi.html>
- WebAssembly Component Model `design/mvp/WIT.md`、公式specification source、commit `d6a17ea9f828f44c4659d70c35822ab53d2a8a1d` で2026-09-15最終変更、2026-10-16確認。<https://github.com/WebAssembly/component-model/blob/d6a17ea9f828f44c4659d70c35822ab53d2a8a1d/design/mvp/WIT.md>
- WebAssembly Component Model `design/mvp/CanonicalABI.md`、公式specification source、commit `2e7895c76905cd3fb7e6be4ac632258972b1f688` で2026-10-13最終変更、2026-10-16確認。<https://github.com/WebAssembly/component-model/blob/2e7895c76905cd3fb7e6be4ac632258972b1f688/design/mvp/CanonicalABI.md>
- `docs/architecture.md`、提案中のembedding／component architecture、最終更新2026-10-16。
- `docs/roadmap.md`、Milestone 0 component-boundary taskとcurrent evidence、最終更新2026-10-16。
- `crates/runtime/src/lib.rs`、型付きhost-function registry prototype、最終更新2026-10-16。
- `crates/runtime/src/component.rs`、schema-1 component validation prototype、最終更新2026-10-16。
- `crates/runtime/src/component/resource.rs`、generation tag付きresource-table prototype、最終更新2026-10-16。
