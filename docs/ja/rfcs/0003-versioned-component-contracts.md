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

実行可能prototypeは `i64 -> i64` host importをsupportし、`i64`／`bool` scalar function typeを記録し、明示的なshared／exclusive borrow tokenを持つgeneration tag付きowned-resource tableを提供します。これはin-memoryのdesign probeであり、stableなserialized formatでもWebAssembly Interface Types（WIT）実装でもありません。

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
- Trust boundaryでstructuredかつdeterministicなfailureを返す。

## 非目標

- Serialized IDLまたはwire encodingをstable化すること。
- WIT source／binary compatibilityを主張すること。
- String、list、record、variant、option、result、async functionを定義すること。
- Serialized resource-handle layout、cross-component ownership transfer、destructor、lifetimeをstable化すること。
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

Declarationは `HostFunctionId` を含みません。Embedderは次を別に構成します。

```text
grant clock.monotonic
bind read-clock = host-function 7
```

Schema 1を理解でき、declarationが構造的にvalidで、すべてのbindingが宣言済みimportを指し、要求されたcapabilityがすべてgrantされ、すべてのimportにbindingがあり、bound host functionがsupport済みの型付きregistrationとして存在する場合にのみvalidationが成功します。得られるvalidated valueはportable declarationとresolved local IDを保持します。

Text例は説明専用です。このRFCはfile syntaxまたはserializationを選択しません。

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

`ResourceTable<T>` はhost valueを所有します。`OwnedResourceHandle` は `(slot: u32, generation: u64)` です。`BorrowedResourceHandle` はさらにmonotonicに割り当てられたborrow identifierと `Shared`／`Exclusive` kindを持ちます。Released slotは再利用前にgenerationをincrementします。Generationを進められないslotはretireします。Free slotはslot昇順で選び、reuseをdeterministicにします。Borrow identifierは再利用せず、exhaustionはerrorです。

Resource operationは最初にslot occupancy／generationをvalidateします。Shared-borrow creationはactive exclusive borrowを拒否します。Exclusive-borrow creationはすべてのactive borrowを拒否します。Mutable accessはsharedまたはinactive tokenを拒否します。Borrow終了はそのtokenだけをremoveし、repeated endはstaleです。Releaseはactive borrowを拒否し、valueをexactly onceでremove／returnし、copy済みownerをすべてstaleにします。したがってCopy可能representationはcopy可能ownershipを意味しません。

## Execution mode

Contract／resource-table validationはbackend非依存で、VM、JIT、AOT executionを選ぶ前に実行します。Prototypeはimportを `HostFunctionRegistry` に対してresolveし、runtime crate内でresource lifecycleを直接実行します。VM moduleのinstantiate、JIT call-site patch、AOT import／resource lowering、export callはまだ行いません。将来のexecution pathはunchecked metadataを独立に再解釈せず、同じvalidated contract／resource ruleをconsumeしなければなりません。

## ABIと互換性

Metadata schema versionとpackage versionは別の意味を持ちます。Schema versionはcontractのshapeとvalidation ruleを選択します。Package versionは宣言されたcomponent revisionを識別します。Schema 1は正確なsupported schema valueを要求し、package versionを記録しますが解釈しません。

`HostFunctionId` はembedding-local bindingであり、portable metadataから意図的に除外します。Resource slot、generation、borrow identifierはruntime-local validation dataであり、選択済みwire representationではありません。このprototypeのRust layout、enum discriminant、in-memory map order、symbol name、handle bit layout、説明用text syntaxはいずれもstable formatではありません。Boundaryをstableと呼ぶ前に、後続RFCでvalidated serialization、compatibility negotiation、canonical lowering rule、serialized resource-handle representationを選択する必要があります。

## Diagnosticsとtooling

Contract validationはunsupported schema、invalid identifier、duplicate name、namespace collision、unsupported function type、過剰／不足binding、missing capability、unknown host functionについてstructured error variantを返します。Resource operationはstale owned／borrowed handle、borrow conflict、in-use release、shared-borrow mutation、table capacity、borrow-identifier exhaustionを区別します。Human-readable messageは論理nameまたはhandle identityを含み、callerはtextをparseせずvariantをmatchできます。

将来のdecoderは同じsemantic failureへbyte offsetまたはsource spanを付与するべきです。Toolingはembedding configurationを診断する場合を除き、local numeric host IDではなくportable import nameを表示するべきです。

## Performanceとcost model

Contract validationはduplicate／lookup checkにhash set／ordered mapを使い、平均してdeclaration／binding数に対してlinearです。Owned stringとresolved-binding mapを成功したvalidationごとに1回allocateします。Resource lookupはslotによりconstant timeで、shared-borrow tracking／deterministic free-slot selectionはordered setを使い、それぞれのset sizeに対してlogarithmicです。Call-site thunk generation、canonical representation conversion、cross-boundary resource dispatch、destructor overheadはこのprototypeの範囲外であり、stable化前のbenchmark requirementとして残ります。

## Securityとtrust boundary

Component metadata、capability declaration、binding configurationは、それぞれのboundaryでuntrusted inputです。Executionまたはlinker／JIT patchingより前にvalidationを完了しなければなりません。Authorityはembedderが供給するgrantから生じ、componentがcapabilityを宣言するだけでは得られません。Unauthorized componentへhost inventoryを露出しないよう、capability failureをhost-registry lookupより先に評価します。

Validated wrapper／resource tableが証明するのは実装済みcheckだけです。Copy可能handleをuntrusted tokenとして扱い、すべてのoperationでgeneration／active-borrow stateを再validateします。Stale numeric valueの所持からauthorityを導出しません。このprototypeはnative callbackをsandbox化せず、callback workをpreemptせず、heap limitをenforceせず、serialized byte streamをvalidateせず、failureをまたいでdestructorを実行せず、native pluginを安全にしません。Panic／unwind behaviorは最終的なcomponent boundaryで表現・封じ込める必要があります。

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
3. Size／depth limit、source location、forward-compatibility ruleを持つbounded serialized component format／decoderを定義する。
4. 1つのvalidated contractをVM、JIT、AOT instantiationへ接続し、import／export差分testを追加する。
5. Canonical rich-value lowering、C embedding handle、panic／error containment、compatibility negotiationを仕様化する。
6. Stable boundaryをacceptする前にvalidation、instantiation、adapter conversion、steady-state call overheadをbenchmarkする。

## テスト計画

- Accepted schema-1 metadataとresolved local bindingのunit test。
- Unknown schema、invalid name、duplicate declaration、import／export collision、unsupported signatureのnegative test。
- Missing capabilityがunknown host registrationより先に報告されることを証明するtest。
- Missing、duplicate、undeclared embedding bindingのtest。
- 将来のdecoder fuzzingと明示的なbyte、declaration-count、nesting-depth、string-length limit。
- 同じvalidated contractを使う将来のVM／JIT／AOT差分test。
- Generation-safe reuse、stale owner／borrow、repeated release／end、shared／exclusive conflict、mutation、deterministic reuse、identifier exhaustionのresource-handle test。
- 将来のcross-mode resource lifecycle／capability-denial conformance test。
- Validation、instantiation、conversion、repeated callの将来benchmark。

## 未解決事項

- Serialized contractはWITを直接採用するか、compatible subsetをembedするか、WIT adapter付きproject-specific encodingを使うべきか。
- Embedderはどのpackage-version compatibility rule／negotiation algorithmを適用するべきか。
- 最初のstable schemaにはどのrich value／resource ownership typeを含めるべきか。
- Capability namespace、delegation、attenuation、ambient host policyをどうcomposeするべきか。
- Validated contractはdirect function pointer、immutable registry reference、generation-checked registry handleのどれを保持するべきか。

レビュー結果：**needs prototype/measurement**。Scalar metadata、capability-gated binding、table-localなowned／borrowed resource lifecycleは実行可能になりましたが、このRFCをstable component contractとしてacceptする前に、serialized resource representation／transfer、bounded decoding、VM／JIT／AOT instantiation、canonical lowering、compatibility negotiation、destructor／failure behavior、boundary-cost evidenceがblocking workとして残ります。

## 情報源

- WebAssembly Component Model「Why the Component Model?」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/design/why-component-model.html>
- WebAssembly Component Model「WIT」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/design/wit.html>
- WebAssembly Component Model「Worlds」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/design/worlds.html>
- WebAssembly Component Model「Canonical ABI」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/advanced/canonical-abi.html>
- `docs/architecture.md`、提案中のembedding／component architecture、最終更新2026-10-16。
- `docs/roadmap.md`、Milestone 0 component-boundary taskとcurrent evidence、最終更新2026-10-16。
- `crates/runtime/src/lib.rs`、型付きhost-function registry prototype、最終更新2026-10-16。
- `crates/runtime/src/component.rs`、schema-1 component validation prototype、最終更新2026-10-16。
- `crates/runtime/src/component/resource.rs`、generation tag付きresource-table prototype、最終更新2026-10-16。
