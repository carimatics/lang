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

本proposalはnormal-path scalar／resource transfer、panic／trap／destructor-failure behavior、deterministicなpackage／interface compatibility negotiationも規定します。これらのadapter、cleanup、failure、negotiation ruleは規定済みですが、まだ実装されておらずstable ABIとしてacceptされていません。

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
- Ownership transfer後のbackend-neutralなpanic／trap containmentと、deterministicかつat-most-onceなdestructor-failure behaviorを規定する。
- SemVer claimをstructural compatibilityの証明として扱わず、deterministicなpackage／interface version negotiationを規定する。
- Trust boundaryでstructuredかつdeterministicなfailureを返す。

## 非目標

- Serialized IDLまたはwire encodingをstable化すること。
- WIT source／binary compatibilityを主張すること。
- String、list、record、variant、option、result、async functionを定義すること。
- Live resource handleをpackage metadataへserializeすること、またはruntime handle bit layoutをstable化すること。
- Process abortからrecoverすること、native unwind／foreign exceptionがcomponent boundaryを越えることを許すこと、またはasync cancellationを定義すること。
- C embedding ABI、native calling convention、symbol namingをstable化すること。
- この段階でVM、JIT、AOT pathを通してcomponent exportを実行すること。
- Network越しにdependencyをresolveすること、registryを選ぶこと、source-package dependency graphをsolveすること、または最初のcontractで1 package内のinterfaceを独立versioningすること。

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

`inspect` はcall-scoped loanを受け取り、`adopt` はtransfer commit成功時にcallerのownerをconsumeします。その後 `adopt` がpanicしても、callerはownerを取り戻しません。Callee codeが別の場所へmove済みでなければ、callee cleanup ledgerがdestroyします。Destructor failureは1回だけreportし、残るcleanupを続行してaffected instanceをpoisonします。このnotationは説明用であり、最終的なIDL syntaxを選択するものではありません。

Interface importは概念上、package／interfaceとnormalized version requirementの両方を指定します。

```text
import clocks from example:time/clocks
  version >=1.2.0,<2.0.0
  requires members { now: () -> i64 requires clock.read }
```

Serialized contractはこの説明用range syntaxではなく、exact requirementまたは明示的lower／upper boundを格納します。Instantiation時にembedderはversion boundとすべてのrequired memberを満たすauthorized providerのうち、最も高いversionを選びます。Exact content digestへのlock bindingがある場合はそのsearchより優先します。SemVerがmatchしてもsignature、ownership mode、resource identity、capabilityが変わっていればfailureです。

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

Versioned encoding候補は `LCMP` を再利用し、little-endian `u32` schema version `2` と1-byte document-kind discriminatorを続けます。Contract kind `0` はpackage key、型付きprerelease／build identifierを持つ完全なsemantic version、完全interface key順にsortしたinterface requirement、key順にsortしたexact lock、完全package／interface／resource key順にsortしたcanonical resource-type identityを格納します。Requirement tag `0` はbuild-sensitiveなexact version、tag `1` はinclusive minimum／exclusive maximum versionとcanonicalな `0`／`1` prerelease-policy byteを持ちます。各requirementの後にはmember name順にsortしたrequired memberが続きます。Memberはname、順序付きparameter／result list、exact capabilityを格納し、value-type tagは `i64` が `0`、`bool` が `1`、`own<T>` が `2`、`borrow<T>` が `3` で、resource tagの後に完全なresource-type keyが続きます。Borrowed resultは拒否され、すべてのnominal resource valueはcontract resource sectionで宣言されたidentityを参照しなければなりません。Digest tag `0` はexactly 32 byteのSHA-256を持ちます。Failure-report kind `1` はfully redactedを示すmessage policy `0` またはembedding-sanitized transportを示す `1`、1つのprimary record、順序付きcount-prefixed suppressed-record list、canonicalな `0`／`1` poisoned flagを格納します。各recordはcategory（`0` panic、`1` trap、`2` destructor）、phase（`0` call、`1` cleanup）、diagnostic identity（`0` boundary panic、`1` boundary trap、`2` destructor failure、`3` instance poisoned）、canonicalなmessage-presence byteと、presentの場合はbounded stringを格納します。Validationは各diagnosticのcategoryとdiagnostic固有phaseを固定し、suppressed recordをcleanup phaseに限定し、redacted policy下のmessageを禁止し、destructor failure／instance-poisoned diagnosticにpoisoningを要求します。すべてのcount／stringはlittle-endian `u32` length prefixを持ち、`ComponentCodecLimits` でboundedです。Semantic-version componentはすべてlittle-endian `u64` で、nested member valueにはdepth 5が必要です。Constructionはencoding前にsemanticsをvalidateし、各decoderはbounded parsing後にvalidationを繰り返し、malformed header、tag、boolean、UTF-8、truncation、trailing byte、limitをbyte offset付きで報告します。Collection sortingによりcontract encodingは1つのcanonical formとなり、parameter／result／suppressed-record順序はsignificantなままです。Check-in済みgolden fixture、round-trip／semantic-validation／malformed-input test、deterministic arbitrary-byte corpus、専用libFuzzer targetが両document kindをcoverします。Schema 2はRFC acceptanceまでcompatibility promiseではなく提案中prototypeです。

### Canonical scalar／resource call representation

**Source fact。** WITはunique ownershipを持つresource handleとborrowed handleを区別します。Owned handleを渡すとunique ownershipがtransferされ、borrowed handleはcall durationだけの一時loanです。WebAssembly Component Model Canonical ABIは両方のhandle kindを32-bit boundary indexへloweringし、handleのlift／drop時にresource identityとactive lendをcheckします。そのreference tableはremove済みentryが残したholeを再利用します。Scalar loweringでは `s64` に64-bit laneを使い、boolean loweringは `0` または `1` を生成し、liftingは `0` をfalse、すべてのnon-zero integerをtrueとして扱います。

**Project decision。** 最初のcanonical call subsetは `i64`、`bool`、`own<resource T>`、`borrow<resource T>` です。`i64` は1つの64-bit laneでtwo's-complement bitを保持します。`bool` は1つのunsigned 32-bit laneを使い、lowering時はcanonicalに `0`／`1` とし、lifting時は `0` をfalse、non-zero valueをtrueとします。Resource valueは1つのcomponent instanceにscopeされたadapter-owned tableへのunsigned 32-bit indexを使います。Index `0` はinvalidとし、それ以降のboundary indexはそのinstanceのlifetimeを通じて再利用もwrapもせずmonotonicにallocateします。設定済みallocation limitまたは残存index namespaceのexhaustionはstructured failureです。これはuntrustedなstale numeric copyが後続resourceをaliasできないようにするため、Canonical ABI reference tableのfree-list reuseと意図的に異なります。Adapterはinternal storage slotを再利用できますが、そのgeneration tag付きownerと再利用しないboundary indexは別物です。Adapter entryはvalidated resource-type identityを保持し、runtimeのgeneration tag付きowner／borrow tokenへresolveします。Boundary index、internal slot、generation、borrow identifier、host addressは相互変換せず、package metadataへserializeしません。Canonical `borrow<T>` が規定するのはlifetime／ownershipだけです。各operationがruntime tableのshared／exclusive tokenのどちらを取得するかは、後続memory／concurrency RFCで判断します。

各callはownershipを変更する前に、argument list全体へpreflightを行います。Preflightはfunction signature、capability grant、resource type identity、live generation、要求ownership mode、active lend、duplicate owned argument、1つのresourceをowned／borrowedの両方として提示するalias、設定済みlive／total-index limit、destination capacityをvalidateします。Borrowだけを繰り返すaliasは、後続memory／concurrency policyが要求access modeを許す場合に限って受け入れられます。Preflightはすべてのdestination entry／backing storageをreserveしますが、handleをpublishせずresource／ownership stateを変更しません。このためallocation／reservation failureでもcallerは不変です。Normal-path transferは次のruleに従います。

1. `own<T>` parameterのloweringは、caller adapterからlive ownerをexactly onceでremoveし、1つのtransfer commitの下でcallee adapterへfresh ownerをinsertします。Commitはreserved destination entryを使い、allocation、user code、callback、destructor invocationを行わず、failureになりません。Component-boundary operationはintermediate stateを観測できません。Boundary indexを再利用しないため、callerのold numeric indexのcopyは永続的にstaleです。Borrowed、wrong-type、already moved、actively lent、duplicate、borrow-aliased ownerはcommit前に拒否します。
2. `borrow<T>` parameterのloweringはownershipをcallerに残し、dynamic call scopeへ紐づくtokenを作成します。Calleeはそのtokenをrelease／transfer／returnできず、scope後まで保持できません。Nested callはchild scope内だけでreborrowでき、すべてのborrowはparent scope終了前に終わります。
3. `own<T>` resultのliftingはcalleeからcallerへ対称なpreflight／commitを行います。Destination ownerのinstall成功時だけcalleeのnumeric indexがstaleになります。
4. Borrowed resultはloanを証明するcall scopeより長く生存するため、最初のcanonical subsetから除外します。Owned boundary handleのdropはownerをexactly onceでconsumeし、destructor invocation／failureは下記boundary ruleに従います。

Transfer commit前のfailureではownershipを変更しません。Adapterはpreflight reservation／transfer commitがactiveな間、callback reentrancyを防止する必要があります。Commit後のpanic、trap、callback reentrancy、destructor failureは下記boundary ruleに従い、async cancellationは未解決のままです。VM、JIT、AOT adapterは同じpreflight／table operationを呼び出す必要があります。Whole-program direct-call specializationはそれらのcheckとのobservational equivalenceを証明した場合だけ有効です。

**未解決hypothesis。** Per-instance adapter tableはVM／native behaviorをdeterministicにし、internal handle layoutのleakを防ぐはずですが、validation、reservation、monotonicに消費するindex namespace、table mutation、callback costはsource factではなくmeasurement対象です。

`ResourceTable<T>` はhost valueを所有します。`OwnedResourceHandle` は `(slot: u32, generation: u64)` です。`BorrowedResourceHandle` はさらにmonotonicに割り当てられたborrow identifierと `Shared`／`Exclusive` kindを持ちます。Released slotは再利用前にgenerationをincrementします。Generationを進められないslotはretireします。Free slotはslot昇順で選び、reuseをdeterministicにします。Borrow identifierは再利用せず、exhaustionはerrorです。

Resource operationは最初にslot occupancy／generationをvalidateします。Shared-borrow creationはactive exclusive borrowを拒否します。Exclusive-borrow creationはすべてのactive borrowを拒否します。Mutable accessはsharedまたはinactive tokenを拒否します。Borrow終了はそのtokenだけをremoveし、repeated endはstaleです。Releaseはactive borrowを拒否し、valueをexactly onceでremove／returnし、copy済みownerをすべてstaleにします。したがってCopy可能representationはcopy可能ownershipを意味しません。

実行可能なhost-value bridgeは非ゼロかつ単調増加のopaqueな `ResourceIdentity` valueをallocateし、各identityを共有 `ResourceTable<T>` 内の1つのgeneration-check済み `OwnedResourceHandle` へmapします。Canonical adapter entryが受け取るのはopaque identityだけです。Host valueとcanonical ownerのpublishはcallerから見てatomicです。Adapterがentryを拒否した場合、bridgeはinsert済みvalueをreleaseしますが、消費したidentityを再利用しません。Borrow operationはmapped ownerをruntime table経由でresolveし、canonical cleanupはidentityをbridgeへ返すことでactual valueをexactly onceでremove／returnします。このprototypeはidentity／lifecycleの接続をinstantiation／backend executionから独立して確立します。

General instantiationはvalidated schema-2 contractをconsumeし、instance stateを公開する前に、canonical順の各requirementをcomplete provider inventory、明示的capability grant、optional exact lockに対してnegotiateします。最初のfailureはrequirement keyを識別し、structured negotiation causeを保持します。成功時は元contract、immutableなvalidated binding、instanceごとのlimitで構成したfreshな `CanonicalResourceTable` をownします。Contract selectionとadapter-state creationは、後述のcross-mode fixtureより前の独立してtest可能なstageのままです。

最初のcross-mode conformance fixtureはvalidated component callが生成したscalar resultをactual host valueとして使用します。VM、JIT、macOS AOTのresultを、同一のschema-2 instantiation、owned publication／transfer、guest-trap cleanup、exactly-once host-value release、redacted failure-report projectionへ通します。Direct／response-file AOT linkはVM／JITと同じvalue、reusable state、report outcomeになります。これはbackendが生成したdataを囲むhost-side canonical-adapter equivalenceを実証しますが、MIR、bytecode、native ABIへresource type／operationを追加するものではありません。

### Boundary panicとdestructor failure

**Source fact。** WebAssembly Component Model Canonical ABIはresource handleをtableからremoveし、active lendを拒否してからowned handleのdestructorをinvokeします。Rustはrecover可能なunwindとrecover不能なabort panic strategyをsupportします。`std::panic::catch_unwind` がcatchするのはunwindするRust panicだけで、aborting panicはcatchせず、RustのFFI ruleは任意のforeign exceptionをportableにcatchする方法を提供しません。これらはimplementation constraintであり、component contractではありません。

**Project decision。** Rust panic payload、native exception、backend unwindがstable component boundaryを越えることはありません。Result typeを追加した後も、declared error resultは通常valueでありboundary panicではありません。Backend-neutral failure modelは概念上、1つのprimary record、0個以上のordered suppressed record、reusable／poisoned instance stateを持ちます。各recordはstable category（`panic`、`trap`、`destructor`）、phase（`call`／`cleanup`）、logical diagnostic identity（`component.boundary.panic`、`component.boundary.trap`、`component.boundary.destructor-failure`）、policyでsanitizeしたoptional messageを持ちます。Poison済みinstanceへのreentryは `component.instance-poisoned` を使います。Numeric encoding／message transportはversioned ABIの未完了作業です。

Ownership／cleanupは次のruleに従います。

1. Transfer commit前のfailureはcaller ownershipを保持します。成功したcommit後のfailureはownershipをrollbackしません。Calleeがresourceをすでにobserve、mutate、store、moveした可能性があるため、caller indexはstaleのままです。
2. User codeを実行する前に、adapterはすべてのtransferred owner、temporary owner、dynamic borrow scopeについてcleanup ledgerを作成します。後続moveのpublish時には両方のledgerをatomically更新します。Persistent component stateへ正常にstoreしたowner、またはcallerへ正常にreturnしたownerはcurrent frame ledgerから外れます。
3. Normal／failed exitのどちらでも、owned cleanup開始前にchild borrow scopeをinnermostからoutermostの順でcloseします。残るframe-owned resourceはacquisitionと逆順でdestroyします。作成済みでもcallerへcommitしていないowned resultはcallee-ownedのままcleanupします。
4. Compiler-generated language panic／guest trapは明示的なbackend-neutral failure pathへloweringします。VM bytecodeは明示的なtrap／cleanup control flowを使います。JIT／AOTはinternal landing padまたはstatus returnを使えますが、Rust／C++／platform unwindはadapterからescapeできません。`catch_unwind` はunwind用にcompileされた既知のRust host callbackをcontainするために使えますが、semantic mechanismではなく、aborting panicをrecoverできません。Opaqueなcaught panic payloadはdestructorがpanicする可能性があるため、そのdispose自体もcontainします。Native callback registrationはforeign exceptionをadapter entryより前にcatchするよう要求し、containmentを保証できないcallbackはtrusted-native tierだけに属します。
5. Component panic／通常のguest trap後にinstanceをreuseできるのは、cleanup ledgerが空で、dynamic borrowが残らず、adapter invariantを再validateできた場合だけです。Adapter invariant violation、incomplete cleanup、contain済みhost-callback panic、destructor failureはinstanceをpoisonし、以降のentryは `instance-poisoned` で失敗しますがhost processは生存します。Process abortはこのrecoverable contractの範囲外であり、trusted-native limitationとして残ります。

Owned resourceのdropは、user destructor codeをinvokeする前にboundary ownerをconsumedとmarkしてlookupからremoveします。Destructorをattemptするのはat most onceです。Panic／trapでもhandleをresurrectせず、自動retryせず、external side effectをrollbackしません。Cleanup中はcomponent-boundary reentrancyを禁止し、reentry attemptはdestructor failureとしてrecordします。Destructorが失敗した場合、cleanupはfailureをrecordし、残る独立ledger entryを同じdeterministic orderで処理し続けます。先行failureがなければ最初のdestructor failureがprimaryになります。先行するpanic／trapがあれば元のfailureをprimaryに保ち、destructor failureをsuppressed recordとして追加します。追加destructor failureもcleanup順でsuppressedにします。完了後にinstanceをpoisonします。

**未解決hypothesis。** 明示的cleanup ledgerとdestructorごとのcontainmentによりhost processを維持しながらleak／double destructionを避けられるはずですが、code-size、steady-state、failure-path、native landing-pad costにはimplementation／cross-mode measurementが必要です。

### Package／interface compatibility negotiation

**Source fact。** WIT package nameはnamespace、package name、optionalなfull SemVer versionを含み、qualified `use` pathはそのpackage内のinterfaceを指定します。SemVer 2.0.0はrelease済みversionのcontentを変更しないよう要求し、incompatibleなpublic-API changeにmajor-version incrementを割り当て、`0.y.z` をunstableなinitial developmentとして扱い、prereleaseのprecedenceを対応するnormal versionより低くし、precedence比較ではbuild metadataを無視します。Version number／rangeだけでは、2つの具体的interface declarationがstructurally compatibleであることを証明しません。

**Project decision。** Metadata schema negotiation、package selection、interface compatibilityを別々にcheckします。

- Metadata schemaは引き続きexactなsupported integerです。Schema 1はexact package versionを記録し続けますがinterface requirementを表現できません。下記fieldの追加にはnew schemaが必要で、既存 `LCMP` byteを再解釈しません。
- `PackageKey` はexactかつcase-sensitiveな `(namespace, package)` pairです。`InterfaceKey` はexact interface nameを追加します。Nominalな `ResourceTypeKey` はexact resource nameを追加しますが、package versionを意図的に除外します。Providerがversion requirementを通過した後だけ比較することで、別のpackage／interface／resource nameを同一視せず、compatible release間で1つのresource lineageを維持します。初期modelではすべてのinterfaceがpackageのSemVer versionを継承し、interfaceの独立versioningは後続へ送ります。
- Provider declarationはexact SemVer version、immutable package-content digest、exportするinterface declarationを持ちます。Consumerは `Exact(version)` requirement、またはrequired member projectionとともにnormalized `Interval { min_inclusive, max_exclusive, allow_prerelease }` を持ちます。`Exact` はprerelease／build metadataを含む完全なSemVer valueを比較し、`Interval` はbuild metadataがcandidateをorderしないSemVer precedenceを使います。Serialized formはこれらのfieldを直接格納し、textual range grammarの解釈差をruntime間に生じさせません。
- Toolingはstableな `1.0.0` 以降のdependencyを同じmajor内のintervalへdefault loweringし、`0.y.z` dependencyを `Exact` へdefault loweringします。より広いzero-major／prerelease intervalにはauthorの明示的opt-inが必要です。Prerelease candidateは、`Exact` がそのprereleaseを指定するかintervalがprereleaseを明示的に許可しない限りrequirementを満たしません。Build metadataはprecedenceでは無視しますが、selected identityには保持します。

Instantiationはprovider inventoryを公開したりVM／JIT／AOT call siteをpatchしたりする前に、次のdeterministic algorithmを実行します。

1. Consumer contractとすべてのcapability grantをvalidateします。Required grantを持たないcomponentはcandidate lookupより先に既存capability errorを受け取ります。
2. Embedding／lock dataがexact content digestを指定する場合、exact package／interface identity、version、digestを要求します。Mismatchはterminalであり、negotiationは暗黙にfallbackしません。
3. それ以外ではregistered providerをexact `PackageKey`／`InterfaceKey`、次にnormalized SemVer requirement／prerelease policyでfilterします。
4. Consumerのrequired projectionを各candidateに対してstructurally validateします。すべてのrequired member name、parameter／result order、scalar type、resource type identity、`own`／`borrow` mode、capabilityがexactにmatchする必要があります。Unknown schema constructはfail closedです。Providerはvalidated projectionから到達できない追加memberを公開できます。
5. 残るcandidateからSemVer precedenceが最も高いものを選びます。そのprecedenceで異なるcandidateのbuild metadata／content digestが異なる場合、registration／map orderで選ばずambiguityを返します。
6. Selected exact version、digest、required-member mapping、capability、adapter ruleを持つimmutableな `ValidatedInterfaceBinding` を生成します。すべてのexecution modeがこのbindingをconsumeします。Registry mutation／rebindingにはfresh validationが必要で、running instanceはfailure後に別providerへfall throughしません。

したがってcompatibility promiseは2層です。SemVer boundはconsumerが検討を許すreleaseを表し、structural projectionはselected releaseが期待するobservable contractを引き続き提供することを証明します。Unrequested exportの追加は既存consumerとcompatibleです。Required memberのremove／rename、parameter／result orderまたはtypeの変更、resource identity／ownership modeの変更、matched memberのcapability変更はversion labelにかかわらずincompatibleです。Package-manager resolutionは利用可能なartifact／lock digestを決めます。Runtime negotiationはnetwork／registry accessを行いません。

実行可能なpackage-selection prototypeは、contract requirementとruntime instantiationの間にcanonicalなin-memory lockfile artifactを置きます。Unlocked passは全requirementについてexact SHA-256 version／digest entryをinterface-key順に1つずつ記録します。Replayは同じconsumer package／versionとcompleteかつorphanのないrequirement coverageを要求し、external entryはembedded contract lockと一致しなければなりません。Replay entryはすべてresolver既存のexact-lock pathを使うため、provider不在／digest mismatchではcompatible-version fallbackなしに失敗します。成功selectionはcontract／immutable bindingをownし、再negotiationせずfreshでboundedなinstance stateを生成できます。これはstable on-disk syntax、registry protocol、artifact fetch、publication behaviorを意図的に定義しません。

**未解決hypothesis。** Projection matchingはsafetyを弱めずadditive provider evolutionを許すはずですが、declaration size、digest／fingerprint strategy、candidate-set scaling、明示的zero-major／prerelease requirementのergonomicsにはprototype／registry／tooling measurementが必要です。

## Execution mode

Contract／resource-table validationとpackage／interface negotiationはbackend非依存で、VM、JIT、AOT executionを選ぶ前に実行します。すべてのmodeが同じimmutable `ValidatedInterfaceBinding` をconsumeし、同じpreflight、adapter-table transfer、dynamic-scope borrow、cleanup-ledger order、failure precedence、instance-poisoning ruleを適用します。Prototypeはnormalized version requirementを表現し、providerをselect／exact lockし、general schema-2 instance stateを生成し、VM／JIT／macOS AOT scalar resultを囲むhost-side resource／failure adapter fixtureを1つ実行するようになりました。Resource-aware VM／JIT call siteのpatch、MIR／bytecode／native ABIを通じたtyped resource lowering、backend landing pad周辺のboundary enforcementは未完了です。将来のexecution pathはunchecked metadataを独立に再解釈したり、code patching後に再negotiationしたりしてはいけません。

## ABIと互換性

Metadata schema versionとpackage versionは別の意味を持ちます。Schema versionはcontractのshapeとvalidation ruleを選択します。Package versionは宣言されたcomponent revisionを識別します。Schema 1は正確なsupported schema valueを要求し、package versionを記録しますが解釈しません。

`HostFunctionId` はembedding-local bindingであり、portable metadataから意図的に除外します。Resource slot、generation、borrow identifierはruntime-local validation dataであり、portable representationではありません。提案するunsigned 32-bit canonical resource indexはadapter-table keyであり、runtime handle layoutでもserializable identityでもありません。Rust panic payload、native exception object、backend unwind metadataもportableではありません。Schema 1とその `LCMP` fixtureはexact package versionだけをencodeし続けます。Candidate schema 2はruntime handle／unwind payloadをserializeせず、normalized requirement、interface projection、content-digest algorithm、resource identity、failure reportの明示的なrepresentationを提供します。Rust layout、enum discriminant、in-memory map order、symbol name、説明用text syntaxはunstableなままです。Boundaryをstableと呼ぶ前に、accepted RFC revisionで候補のtag、ordering、compatibility rule、message policyをratifyまたはreviseする必要があります。

## Diagnosticsとtooling

Codecはbyte、declaration-count、nesting-depth、string-length limit、malformed header／length／UTF-8／scalar tag、unsupported schema、trailing byteについてstructured errorを返します。すべてのcodec errorがbyte offsetを含みます。Contract validationは別にinvalid identifier、duplicate name、namespace collision、unsupported function type、過剰／不足binding、missing capability、unknown host functionについてstructured error variantを返します。Negotiationはunsupported schema、unresolved import、invalid requirement、compatible versionなし、prerelease拒否、structural member mismatch、lock-digest mismatch、ambiguous providerを区別します。Authorized embedding diagnosticsはpackage／interface、candidate version、member、mismatch dimensionを識別し、untrusted componentにはcapability validation前にprovider-inventory detailを返しません。Resource operationはstale owned／borrowed handle、borrow conflict、in-use release、shared-borrow mutation、table capacity、boundary-index exhaustion、destination-reservation failure、borrow-identifier exhaustionを区別します。Boundary failureはRust panic payload／native exception objectを公開せず、stable category、phase、diagnostic identity、ordered suppressed failure、poisoned stateを公開します。Human-readable messageはlocation、論理name、handle identityを含み、callerはtextをparseせずvariantをmatchできます。

Toolingはembedding configurationを診断する場合を除き、local numeric host IDではなくportable import nameを表示するべきです。Resolution outputはnormalized requirement、selected exact version／digest、lockがselectionを強制したか、各rejected candidateの最初のstructural incompatibilityを表示するべきです。Decode後のsemantic failureをbyte rangeへmapする作業は将来課題です。

## Performanceとcost model

Contract validationはduplicate／lookup checkにhash set／ordered mapを使い、平均してdeclaration／binding数に対してlinearです。Owned stringとresolved-binding mapを成功したvalidationごとに1回allocateします。Indexed identity lookup後のnegotiationは各candidate／required projectionを調べ、registry orderに依存せずmaximumを選びます。Candidate count、projection size、digest verificationをboundする必要があります。Resource lookupはslotによりconstant timeで、shared-borrow tracking／deterministic free-slot selectionはordered setを使い、それぞれのset sizeに対してlogarithmicです。Candidate filtering、structural projection、digest verification、call-site thunk generation、canonical conversion、cross-boundary resource dispatch、cleanup-ledger maintenance、panic-containment barrier、landing pad、destructor cleanupはstable化前のbenchmark／memory／code-size requirementとして残ります。

## Securityとtrust boundary

Component metadata、capability declaration、version requirement、content digest、provider registration、binding configurationは、それぞれのboundaryでuntrusted inputです。Executionまたはlinker／JIT patchingより前にvalidation／negotiationを完了しなければなりません。Authorityはembedderが供給するgrantから生じ、componentがcapabilityを宣言するだけでは得られません。Unauthorized componentへinventoryを露出しないよう、capability failureをprovider／host-registry lookupより先に評価します。Package／interface nameはfallbackなしでcanonicalかつcase-sensitiveに比較し、prerelease admissionは明示的とし、digest algorithm／lengthを比較前にvalidateし、lock mismatchまたはsame-precedence ambiguityではdowngradeせずfail closedにします。

Scalar codecはallocation前に設定済みlimitをcheckし、malformed inputをdeterministically拒否し、semantic validationとは独立してfuzz targetを持ちます。Validated wrapper／resource tableが証明するのは実装済みcheckだけです。Copy可能handleをuntrusted tokenとして扱い、すべてのoperationでgeneration／active-borrow stateを再validateし、instance内でboundary indexを再利用しません。Stale numeric valueの所持からauthorityを導出しません。Preflightはmutation前にowned aliasを拒否し、transfer commit内ではuser codeを実行しません。Panic payload／native exception objectはuntrustedであり、boundaryを越えてverbatimに反映しません。Message exposureはembedder policyです。Poisoningにより、external stateをpartialにした可能性があるdestructor、host-callback、invariant failure後のreentryを防ぎます。Prototypeはbackend-neutral cleanup ledgerを実装しますが、live resourceをserializeせず、native callbackをsandbox化せず、callback workをpreemptせず、heap limitをenforceせず、backend landing padをinstallせず、native pluginを安全にしません。Aborting panic／foreign exceptionはrecoverable in-process contractの範囲外です。

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

### Failure時にownershipをrollbackするかprocessをabortする

Panic後にtransferred ownerをcallerへrollbackする方法は便利に見えますが、callee codeがresourceをmutate、store、moveした後にはunsoundです。すべてのpanicでprocessをabortする方法は単純で、native `panic=abort` では不可避ですが、embedded-script isolation goalに反します。最初のdestructor failureでcleanupを止める方法はsecondary failureを避けますが、後続frame-owned resourceをleakします。本提案はcommit済みownershipを維持し、deterministicなbest-effort cleanupを実行し、suppressed destructor failureを記録してaffected instanceをpoisonします。

### SemVerだけをtrustするかexact versionだけを要求する

In-range SemVer labelをdeclaration checkなしでtrustする方法は高速ですが、mislabelされたreleaseがsignature、ownership、capabilityを暗黙に変更できます。Exact-version-only matchingはより安全ですが、compatible patchごとにlockstep churnが起きます。Version intentなしのstructural matchingはauthorがsupportすると選んでいないshapeを受け入れます。本提案は明示的normalized SemVer requirementとstructural projectionの両方を要求し、optionalなexact lock digestを組み合わせます。

## 実装計画

1. Schema-1 in-memory scalar metadata、明示的capability grant、分離されたimport binding、host-registry resolution、structured validation errorを実装・test済み。
2. Generation tag付きowned／borrowed resource handleを実装・testし、stale handle、repeated release、repeated borrow end、borrow conflictをdeterministically拒否済み。
3. Deterministicな実験的schema-1 scalar-metadata encoder／decoderを実装・test済み。Byte／count／depth／string limit、byte-offset diagnostics、malformed fixture、libFuzzer targetを備える。Unsupported schema versionはpayloadより先に拒否し、stable serialization／forward-compatibility policyは未決定のまま残す。
4. 1つのvalidated schema-1 scalar contractをVM／JIT execution、macOS AArch64 AOT object emission、direct／response-file link、process executionへ接続して実装・test済み。General schema-2 instantiationは全requirementをnegotiateしてからimmutable binding／freshでboundedなadapter stateを公開します。Typed resource callはMIR／backend ABI外のままです。
5. Adapter-table `own<T>`／`borrow<T>` resource index、call全体のpreflight／reservation、allocation／callbackなしのcommit、再利用しないboundary index、nested call-scoped borrow、generation-check済みhost-value identity linkageのbackend-neutral traceを実装・test済み。Exact source-level scalar／boolean canonical conversionとtyped backend resource loweringは未完了です。
6. Backend-neutralなcommit後非rollback、deterministic borrow／owner cleanup ledger、callback前removeとat-most-once destructor、ordered primary／suppressed failure、再利用可能なguest panic／trap exit、host-callback／invariant／destructor failure後のconservative poisoningを実装・test済み。Host-side VM／JIT／macOS-AOT fixtureは同等のcleanup／serialized-failure outcomeを記録します。Backend landing pad、foreign-exception policy enforcement、panic-payload disposal、asynchronous cancellationは未完了です。
7. Exact package／interface keyとversion-free resource key、normalized full-SemVer exact／interval requirement、明示的prerelease policy、structural required-member projection、fallbackなしのexact lock digest、deterministicなhighest-version selection、same-precedence ambiguity rejectionを実装・test済み。Canonicalなin-memory lockfileはcomplete selectionを記録し、fallbackなしでexact lockをreplayし、embedded lockとreconcileし、選択済みbindingをinstantiationへ渡します。Stable on-disk lockfile syntaxとproduction package-manager I/Oは未完了です。
8. Candidate schema-2 `LCMP` contract／failure-report documentを実装・test済み。Contractはcanonical package／resource identity、完全なsemantic version、normalized exact／interval requirement、structural required-member projection、SHA-256 tag付きexact lockをcoverする。Failure reportは固定category／phase／diagnostic tag、primary record、順序付きcleanup-phase suppressed record、poisoned state、明示的redacted／sanitized message policyをcoverする。Runtime `BoundaryFailure` recordからのpolicy明示projectionは、unsanitized messageを公開せずcleanup invariant trap／destructor failureを保持する。Orderがsemanticでない箇所のdeterministic sorting、check-in済みgolden encoding、明示的byte／item／depth／string limit、byte-offset diagnostics、semantic validation、arbitrary-byte test、専用fuzz targetが両layoutをcoverする。Stable serialization選定前にowner acceptanceとforward-compatibility policyが引き続き必要。
9. Scalar metadata validation、3-candidate negotiation、direct／validated scalar dispatch、generation tag付きtable lifecycle、canonical owner／borrow traceについて、最初の4-series boundary-cost harnessとcleanなmacOS AArch64 baselineを実装済み。General instantiation、lockfile replay、failure／destructor path、component memory／code size、host-value／cross-mode adapter、remote Windows executionの拡張cost evidenceは未完了です。

## 初期boundary-cost evidence

Cleanなschema-15 macOS AArch64 [baseline](../../../benchmarks/results/2026-10-16-macos-aarch64-f163409.json)は、commit `f163409c00d94e08cd636357e58143134e02f568` から31 sampleを記録します。Median／p95 costは、1-import／1-export scalar metadataのclone／validationが286.833／316.292 ns、3 providerのvalidation／selectionが842.584／926.250 ns、direct scalar callbackが0.958／1.031 ns、logical-import lookup＋host-registry lookup＋callbackが11.451／11.695 ns、generation tag付きowner insert／releaseが10.661／12.452 ns、insert／shared-borrow／read／end／releaseが22.731／24.259 ns、canonical owner preflight／commit／persistが412.334／442.417 ns、canonical borrow preflight／commit／scope cleanupが347.250／396.250 nsです。

比率より比較boundaryが重要です。現在のAPIはmetadataをconsumeしてowned immutable bindingを生成するため、metadata validationはcloneを含みます。Negotiationはconsumerと全providerをvalidateし、選択したprojectionをcloneします。Validated dispatch rowは操作ごとに2つのmap lookupを行い、cached direct-call specializationではありません。Canonical rowはpreflight allocation／reservation、trace／cleanup-ledger construction、単調増加するboundary-index vectorを含みますが、table rowはinternal generation tag付きslotを再利用します。したがって、これらの値は正確なprototype pathについてのevidenceであり、stable ABI budgetやoptimized VM／JIT／AOT adapterに関する主張ではありません。General instantiation、host-value integration、failure／destructor path、component memory／code size、asynchronous cancellation、remote Windows executionは対象外です。

## テスト計画

- Accepted schema-1 metadataとresolved local bindingのunit test。
- Unknown schema、invalid name、duplicate declaration、import／export collision、unsupported signatureのnegative test。
- Missing capabilityがunknown host registrationより先に報告されることを証明するtest。
- Missing、duplicate、undeclared embedding bindingのtest。
- Deterministic scalar-metadata encoding、明示的なbyte／declaration-count／nesting-depth／string-length limit、byte-offset diagnosticsのround-trip／malformed-fixture test。
- Bounded decoder向けlibFuzzer targetとdeterministic arbitrary-byte smoke corpus。
- Schema-2候補contract testはcanonical ordering、exact golden encoding、full-SemVer exact／interval requirement、SHA-256 tag付きlock、resource identity、duplicate／orphan／mismatched-lock rejection、すべてのparser bound、malformed header、arbitrary byteをcoverする。別のfuzz targetがdecoderを実行する。
- VM／JIT差分testとmacOS AArch64 AOT link／run integrationは同じvalidated schema-1 scalar contractを使い、host-side resource／failure fixtureがgeneral instantiation後に3 resultすべてを囲みます。将来のtestはnon-scalar callをbackend ABIへloweringし、remote Windows executionを追加します。
- Generation-safe reuse、stale owner／borrow、repeated release／end、shared／exclusive conflict、mutation、deterministic reuse、identifier exhaustionのresource-handle test。
- Backend-neutral adapter testはwrong-type／duplicate／borrow-aliased owner拒否、再利用しないboundary index、ownershipを変えないdestination-limit failure、allocation／callbackなしのcommit、owner transfer／return、call-scoped／nested borrow、deterministic trace、host-value linkage、host-side VM／JIT／macOS-AOT equivalenceをcoverします。将来のtestはexact scalar lane、boolean normalization、stale／active-lend／total-index exhaustion injection、borrowed result禁止、typed backend resource callをcoverします。
- Backend-neutral failure testはpre-commit preservation、post-commit非rollback、inner-to-outer borrow closure後のreverse-order cleanup、moved-owner ledger update、callback前remove destructor、ordered suppressed failureを伴うcleanup継続、再利用可能なguest panic／trap exit、poisoned host-callback／destructor exit、later entry拒否、現backend間の同等なhost-side failure projectionをcoverします。将来のtestはuncommitted result、callback reentrancy、incomplete-cleanup injection、panic-payload disposal、foreign unwind拒否、asynchronous cancellation、backend landing padをcoverします。
- In-memory negotiation／lockfile testはbuild-sensitive exact／bounded requirement、prerelease opt-in、arbitrary-precision numeric prerelease precedence、locked digest成功／mismatch、registration order非依存のhighest compatible selection、same-precedence ambiguity、additive provider member、function／resource-ownership／capability mismatch、capability-before-inventory ordering、canonical complete lock generation、exact non-fallback replayをcoverします。将来のtooling／integration testはzero-major default、compatible-version resource lineage／rename／identity mismatch、残るstructural dimension、stable file I/O、typed VM／JIT／AOT同一bindingをcoverします。
- 将来のtyped cross-mode resource call／capability-denial conformance test。
- Schema-15 benchmark testはmetadata-validation、interface-negotiation、scalar-dispatch、resource-lifecycle seriesの分離を強制します。将来のbenchmarkはgeneral instantiation、lockfile replay、cached／specialized dispatch、host-value／VM／JIT／AOT conversion、failure／destructor path、component memory／code size、remote Windows executionをcoverします。

## 未解決事項

- Serialized contractはWITを直接採用するか、compatible subsetをembedするか、WIT adapter付きproject-specific encodingを使うべきか。
- Toolingはどのsource-level range syntaxをnormalized exact／interval representationへloweringするべきか。
- Interfaceの独立versioningは、いつpackage-inherited versionを置き換えるべきか。
- 初期scalar／resource subsetの次にどのrich valueを追加し、どれを後続schemaへ送るべきか。
- Capability namespace、delegation、attenuation、ambient host policyをどうcomposeするべきか。
- Validated contractはdirect function pointer、immutable registry reference、generation-checked registry handleのどれを保持するべきか。
- Candidate failure-record tag／redacted-sanitized message policyを変更なしでacceptするべきか。また各embeddingはどのsanitizerを適用するべきか。

レビュー結果：**needs prototype/measurement**。Bounded scalar-metadata decoding、capability-gated binding、table-local resource lifecycle、normal-path canonical scalar／resource transfer、panic／destructor failure-boundary rule、deterministicなpackage／interface compatibility negotiationは規定済みです。In-memory compatibility negotiation、exact lock selection／deterministic lockfile replay、1つのvalidated schema-1 scalar VM／JIT／macOS-AOT path、backend-neutralなcanonical-resource transfer／borrow／cleanup／poisoning trace、host-value identity／lifecycle bridge、general schema-2 instantiation、host-side VM／JIT／macOS-AOT resource／failure adapter conformance、candidate schema-2 contract／failure-report document、最初の4-series macOS boundary-cost baselineは実行可能かつtest済みです。このRFCをstable component contractとしてacceptする前に、production package-manager I/O、candidate serializationのowner acceptance／forward-compatibility disposition、remote Windows component execution、より広いinstantiation／failure-path／memory／code-size cost evidenceがblocking workとして残ります。

## 情報源

- WebAssembly Component Model「Why the Component Model?」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/design/why-component-model.html>
- WebAssembly Component Model「WIT」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/design/wit.html>
- WebAssembly Component Model「Worlds」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/design/worlds.html>
- WebAssembly Component Model「Canonical ABI」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://component-model.bytecodealliance.org/advanced/canonical-abi.html>
- WebAssembly Component Model `design/mvp/WIT.md`、公式specification source、commit `d6a17ea9f828f44c4659d70c35822ab53d2a8a1d` で2026-09-15最終変更、2026-10-16確認。<https://github.com/WebAssembly/component-model/blob/d6a17ea9f828f44c4659d70c35822ab53d2a8a1d/design/mvp/WIT.md>
- WebAssembly Component Model `design/mvp/CanonicalABI.md`、公式specification source、commit `2e7895c76905cd3fb7e6be4ac632258972b1f688` で2026-10-13最終変更、2026-10-16確認。<https://github.com/WebAssembly/component-model/blob/2e7895c76905cd3fb7e6be4ac632258972b1f688/design/mvp/CanonicalABI.md>
- Semantic Versioning 2.0.0、公式versioned specification、公開日・page更新日の記載なし、2026-10-16確認。<https://semver.org/spec/v2.0.0.html>
- The Rust Reference「Panic」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://doc.rust-lang.org/reference/panic.html>
- Rust standard library `std::panic::catch_unwind`、公式living API documentation、page更新日の記載なし、2026-10-16確認。<https://doc.rust-lang.org/std/panic/fn.catch_unwind.html>
- The Rustonomicon「FFI and unwinding」、公式living documentation、公開日・更新日の記載なし、2026-10-16確認。<https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-unwinding>
- `docs/architecture.md`、提案中のembedding／component architecture、最終更新2026-10-16。
- `docs/roadmap.md`、Milestone 0 component-boundary taskとcurrent evidence、最終更新2026-10-16。
- `crates/runtime/src/lib.rs`、型付きhost-function registry prototype、最終更新2026-10-16。
- `crates/runtime/src/component.rs`、schema-1 component validation prototype、最終更新2026-10-16。
- `crates/runtime/src/component/serialization.rs`、candidate schema-2 contract／failure-report serialization、最終更新2026-10-16。
- `fuzz/fuzz_targets/versioned_component_contract_decoder.rs` と `fuzz/fuzz_targets/versioned_component_failure_report_decoder.rs`、bounded schema-2 decoder fuzz target、最終更新2026-10-16。
- `crates/runtime/src/component/resource.rs`、generation tag付きresource-table prototype、最終更新2026-10-16。
