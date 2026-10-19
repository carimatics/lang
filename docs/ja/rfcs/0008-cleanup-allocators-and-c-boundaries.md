<!-- translation-of: ../../rfcs/0008-cleanup-allocators-and-c-boundaries.md; canonical-language: en -->

# RFC 0008：Cleanup、allocator authority、C boundary

[English](../../rfcs/0008-cleanup-allocators-and-c-boundaries.md)

- ステータス: accepted
- 著者: Project contributors
- 作成日: 2026-10-17
- 最終更新日: 2026-10-19
- Tracking issue:
- Supersedes:
- Superseded by:

## 概要

受理済みRFC 0004／0007を前提として、Milestone 2に残るcleanup／allocator／unsafe／FFIのdecision gateを定義します。Project ownerは2026-10-17 JSTにこの判断を受理しました。受理は実装／stabilizationの完了を意味しません。

Recoverable errorは明示的typed Result、全正常／failure edgeのcleanupはcompilerがelaborateするdeterministic処理、guest allocationはbudget付きfallible操作、C境界は生成したstatus／out-parameter形式とします。Unsafe sourceは信頼するforeign callを明示するだけで、move／loan／region／artifact検証を無効にしません。最初のC profileはadapter経由のscalarと有限Copy aggregateを扱い、source専用native layoutを保持します。範囲外のC APIは生成contractを実装するC shimで接続できます。

## 動機

Return valueのownership、panic cleanup、allocator lifetime、foreign memory validity、callback containmentが未規定のままでは、owned Text／Bytes／containerを全modeで安全に実行できません。現在のshared engineは実際のownership遷移を実装しましたが、abortし得るhost Rust Arc allocationを使い、recoverable guest-allocation契約をまだ満たしません。既存C harnessはnative objectのlinkを実証し、公開source C ABIを実証しません。

Runtime libraryだけではcompilerのdrop flag、source unsafe authority、cleanup失敗後の部分return value公開を決められません。この提案は公開interfaceの実装前に結果を定めます。Ownership bytecode encodingと汎用VM／component embeddingは別途versionを定める作業です。

## 目標

- Native unwindを言語semanticsにせず、正確に1回のcleanupとfailure優先順位を定義する。
- Guest allocationごとに明示authority、lifetime、budget、recoverable failure経路を設ける。
- 両tier-1 targetの狭いC import／export profileとgenerated header／validation義務を規定する。
- RFC 0006 Copy programとRFC 0007のownership／型表面、明示Result matchを維持する。
- Native unsafeをtyped validated adapterへ閉じ込め、VM／nativeでguest value／drop traceを一致させる。

## 非目標

- User-defined destructor宣言、例外、catch／throw、`?`、closure、async cancellation実装、raw source pointer、pointer arithmetic、variadic、C++例外、任意のC layout。
- Text／Bytes／Vec／Map／shared／weak／arena tokenのC valueとしての直接受け渡し、source native layoutのstable化。
- 汎用bytecode loading／VM lifecycle API、production component IDL／adapter生成、package metadata format、native codeのsandbox保証。
- 完全なownership bytecode schema。新命令のserialize前には受理済みversioned encodingが必要で、schema 1は維持する。

## 利用者向け設計

Recoverable errorは通常の `core::Result<T, E>` valueです。Errを明示returnし、暗黙propagationは挿入しません。`core::drop::<T>(value) -> core::Unit` は意図した時点でownerをconsumeし、`core::panic::<T>(code: i64) -> T` はcompilerが認識するdiverging操作です。この2つの有限generic intrinsicをRFC 0007のlistへ追加し、user genericや任意panic payloadは導入しません。

```text
fn forward(value: core::Text) -> core::Text {
    return value; // 結果を移譲してから他の初期化済みlocalをcleanupする。
}

fn fail() -> i64 {
    return core::panic::<i64>(7);
}
```

C表記は明示的とします。Lexerは `extern` 後の厳密なquoted marker `"C"` を認識し、一般のstring literalは導入しません。Importは `extern "C" fn name(parameters) -> type;`、exportはroot moduleの `pub extern "C" fn name(parameters) -> type { ... }` です。C import callは `unsafe { statements }` 内に置き、実行hostがそのforeign bindingを別途grantしなければなりません。Blockは通常の字句的cleanupを持つstatement scopeで、unsafe expression／raw pointer／unsafe declarationは追加しません。

生成C declarationは `uint32_t` statusを返し、source resultを最後のout-parameterへ書き込みます。例えばsource export `increment(value: i64) -> i64` は次の形になります。

```c
uint32_t lang_increment_<signature_hash>(lang_context *context,
                                       int64_t value, int64_t *out);
```

実際のidentifierはangle bracketではなくhexadecimal digitを含みます。Headerは具体的identifierと可読aliasを供給します。Status 0はreturnとcleanupの全成功を意味し、非0ならresult bufferを変更しません。Callerはcontextからcopied error summaryを取得でき、panic／language valueがCを越えてunwindすることはありません。

## 参照セマンティクス

### Cleanupとfailure優先順位

1. 初期化に成功したowning placeごとにruntime drop obligationを1つ持ちます。Moveで移譲し、move元にはdropを生成しません。Copy valueにowning obligationはありません。Partial move／initializationは引き続きRFC 0007で拒否します。
2. 字句scopeのlocalは、成功した初期化の逆順でcleanupします。初期化済みfunction parameterは宣言順で外側scopeを構成し、その逆順でcleanupします。Conditional initializationは実際に初期化したpathだけを記録し、再初期化はその時点の新obligationを作ります。
3. Aggregate field／payloadは実際の初期化の逆順でcleanupし、recordの記述順と宣言順が異なる場合も同じです。必要な順序metadataを保持し、物理layoutから推測しません。Aggregate構築は一時fieldのobligationを移譲し、失敗時は完成済みtemporaryだけを逆順cleanupします。ArenaはRFC 0004どおりaccessを取り消してからallocationの逆順でnodeをcleanupします。
4. Assignmentはdestination、RHS、置換検証、新obligationのcommit、旧valueのcleanupの順です。検証失敗時は旧destinationを保持し、incoming moved ownerはatomic publicationの外でcleanupします。Shared edge preflight／publication中にuser callbackは呼びません。
5. Return expressionを先に評価します。Non-copy resultを未公開return slotへmoveし、退出scopeをすべてcleanupします。全成功後にだけcallerへpublishし、cleanupが失敗すれば未公開resultもcleanupしてfailureを返します。Callerはそのownershipを受け取りません。
6. Typed Errのreturnは通常のvalue returnです。明示panic、verifier保護下のruntime fault、fuel／frame／cell／allocation limit、containしたhost-call failureはfailure cleanup edgeへ進みます。既存host effectはrollbackしません。Compiler既知のborrow登録を終了してからownerをcleanupし、safe codeでlive loanのownerをmoveできません。
7. このprofileは暗黙user destructorを介して別guest bodyを実行しません。Compiler生成owning-type cleanupと明示登録したtrusted host-resource cleanupはfailureを報告できます。Invocation前にobligationを消費済みにし、失敗しても独立obligationを継続し、retry／resurrectionはしません。
8. 元のexecution failureをprimaryとし、それがなければ最初のcleanup failureをprimaryとします。後続は順序付きsecondary recordです。Entry前にreport bufferを事前確保し、defaultは256 recordです。Overflowではtruncation flag／countを残して全cleanupを継続し、buffer確保失敗はcleanup省略でなくentry拒否にします。
9. Cleanup成功の通常guest panicはinstanceをpoisonしません。Cleanup failureまたはcontainしたhost panicはinstanceをpoisonし、host teardownまで後続guest entryをeffect前に拒否します。Host abort、不正foreign unwind、host memory corruptionはcontainment外です。Destructor／panic-payload containment failureで無限再帰せず、失敗host objectをquarantineまたは意図的leakし、制限を記録して独立bookkeepingを継続します。
10. Native backendはfailure／status／cleanup flowを明示loweringし、platform unwindをsource semanticsにしません。Cleanup bookkeepingは既存obligationを完了するfuelを消費したり、新guest storageを確保したりしません。通常allocation／frame構築で上限付き反復retirement／drop metadataを確保します。外部からのprocess killにcleanup保証はありません。

Non-copyの `core::drop` はmove後に同じcleanupを実行し、後のuseは `SRC0020` です。Copy引数はcopyをdropし、元valueは無効にしません。Loan tokenのdropは字句的lifetimeを短縮せず、unsupported specializationとして拒否します。`core::panic` はi64 code、元span、guest call stackをstructured failureへ保持し、source catcherは導入しません。Flow checkはintrinsicがreturnしないことを認識します。

### Allocator authorityとaccounting

実行instanceはentry前に明示allocator authorityとallocation limitを受け取ります。`core::allocator()` はそのcompiler既知read loanで、他allocatorへのambient accessはありません。Authority不足はguest code、foreign symbol lookup、callbackより先にentry preflightを失敗させます。

各owning allocationはallocator identity、要求size／alignment、live generationを保持します。Growthは元allocatorを使い、Text-from-Bytesはstorage／allocatorをそのまま移譲し、別allocatorへのclone／conversionは明示します。Authorityとcallback contextは全allocation／cleanupより長く生存し、guest valueへ保存／偽造できません。Live allocation中のhostによるrevokeはstructured in-use errorとなり、hostがteardown責任を保持します。

Memory要求前にchecked size arithmetic、i64 length表現、power-of-two alignment、per-request／live／cumulative byte上限、allocation count、type／region validity、metadata capacityを検証します。現core型のalignmentは最大4096で、それ以上／不正な要求はcallback前に拒否します。Size／count overflowは `AllocError::Overflow`、設定上限は `Limit`、allocation／reserve失敗は `OutOfMemory` です。操作に必要なguest-controlled dataとownership／frame／collection metadataはfallible allocationまたは事前確保した有限poolを使います。Guest entry前のtrusted host／compiler context allocationはhost OOMでabortし得ますが、guest core操作内のinfallible allocationを認めるものではありません。

Host primitiveは `allocate(context, size: u64, alignment: u64) -> pointer-or-null` と対応する `deallocate(context, pointer, size, alignment)` です。非zero要求の成功は要求以上の書込可能・一意所有・aligned storageを返し、nullはrecoverable OOMです。Zero-sized guest allocationはcallback不要でもidentity／accountingを保持します。Deallocationは元の厳密な要求で正確に1回行い、失敗／unwind／guest pointer保持／同一guest instanceへのreentryを禁止します。このtrusted foreign precondition違反をinteger handle検証で安全にはできません。

Live byteは成功し現在所有する要求storageと必須guest-controlled metadataを数え、cumulative requested byteはallocatorがnullを返しても認可済みnonzero要求を数えます。Callback前にlive／count budgetをtransactionalにreserveし、失敗時はliveを返却してもcumulative要求は戻しません。Releaseはlive byte／countだけを返却します。Peak live byteとrequest／allocation／deallocation countを記録し、RSS／fragmentationは主張しません。Defaultはlive requested 64 MiB、cumulative 256 MiB、live allocation 65,536で、hostは明示的に変更できます。Project defaultでありperformance保証ではありません。

Container growthはpublication前に置換storageを確保／初期化し、allocation／copy／validation失敗時は元collectionを保持します。評価済みowning argumentはRFC 0007どおりconsume／cleanupします。Shared publication失敗時はlockを出て準備storageをreleaseします。Allocator／resource callbackはgraph／instance lock外で実行します。Lock外でreserveし、再取得後にtopology／livenessを再検証して再訪workを計上し、atomicにpublishまたは拒否します。Topology変更後の古いpreflight結果は認可になりません。

### Unsafe authorityとC-valid type

`unsafe` を新しい予約語とし、従来nameは `SRC0024` migration対象です。Blockはforeign-call義務を明示するだけで、ownership／borrow／region／capability／integer／artifact検証を無効にしません。Untrusted script／pluginのdefaultではforeign／unsafe authorityを拒否します。Trusted hostが正規化binding／signature／contract digestを明示grantし、source-rootや通常host APIの権限をFFIへ昇格させません。Importから自動dynamic-library／path／symbol loadingは起こりません。初期profileではforeign callのsuspend、adapterを越えるthrow／unwind、同じguest instanceへのreentryを禁止します。

C-valid source typeは `i64`、`bool`、`core::Unit`、有限でinline再帰のないCopy record／enumとCopy Option／Resultです。通常loan、arena brand、shared／weak owner、Allocator、Text／Bytes／Vec／Mapとそれを含むrecordはC signatureで拒否し、通常function内部では利用できます。Raw source pointer変換やownership-handle C表現は追加せず、汎用resource／value embeddingはMilestone 3です。Finite profile外はC shimを要求し、platform layoutの推測やsource安全性の無効化はしません。

### 生成C ABI profile 1

対象は64-bit native pointerを持つmacOS AArch64／Windows x86-64とC11 headerです。C compiler／platform ABIがC struct alignmentを決め、adapterはfield単位に変換しsource／VM／Rust layoutをmemcpyしません。Headerはdescriptorからtargetごとのsize／alignment／offset assertionを生成し、両tier-1 hostの実C compilerで検証します。

| Source型 | C表現 |
| --- | --- |
| `i64` | `int64_t` |
| `bool` | 厳密に0／1の `uint8_t` |
| Unit／空record | Reserved zero byteを1つ持つ1-byte generated struct。Unit resultはout-parameterなし |
| Copy record | 再帰的C-valid fieldのgenerated struct。C予約語collisionを避けるfield prefix付き |
| Copy enum | `uint32_t tag` とgenerated payload unionのstruct。Caseは宣言順、未使用union byte／明示reserved fieldはzero |

Scalar parameterはvalue、aggregate parameterはconst generated C structへのpointer、resultは最後のwritable pointerで、Unit resultだけpointerなしです。Guest functionは `lang_context *context` を先頭に持ち、`uint32_t` statusを返します。Import callbackだけcontext直後に `void *user` を追加し、export wrapperは追加しません。Incoming bufferはcall全体で正しいalignment／object extent／read-write validityを持ち、result storageはinput／contextと重複してはいけません。不明tag、不正bool／unit／reserved byte、context provenance／version、count／size overflowをguest effect／output write前に確認します。通常C paddingはsource fieldとして解釈せず初期化も要求しません。Adapterはoutgoing bufferをzero初期化してfieldを書きます。最初のcaseを示すtag 0は有効で、未知tagは拒否します。

未使用enum union byteはactive case payload structの全 `sizeof` より後の領域を指し、struct内部の通常paddingは含みません。Zero-payload caseは1-byte zero reserved表現を使います。Callerは実在し妥当なC object addressとexclusive writable output storageを供給する責任を持ちます。Runtimeは妥当addressの供給後にnull／alignment／type／tag／context identityを確認できても、任意native addressの妥当性は証明できません。Native caller／callbackはtrustedで、borrowed bufferからsafe source ownershipを偽造してはいけません。Import adapterはC-valid inputをcall-local C storageへcopyし、成功outputを検証／copyします。Importのfailure statusはstructured runtime foreign failureと通常cleanupとなり、recoverable domain errorはstatus 0と妥当なCopy Resultで表現します。未知foreign statusは `ffi.invalid-value` で、source enum discriminantにはしません。

Symbolは `lang_<root_name>_<signature_digest>` とし、32-byte SHA-256全体を64桁lowercase hexで使います。公開root nameはASCII identifier最大128 byteです。Hash入力は厳密な9 byte `LANGCABI\0`、little-endian u16 profile 1、u8 target（1 macOS AArch64、2 Windows x86-64）、u8 direction（0 import callback、1 export wrapper）、qualified function name、parameter vector、result typeの順です。Stringはlittle-endian u32 byte lengthとUTF-8本文、vectorはu32 countとelementです。Type tagはu8で0 i64、1 bool、2 Unit、3 record、4 enumです。Recordはqualified nameと(field-name string,type) vector、enumはqualified nameと(case-name string,payload type vector) vectorをencodeします。Copy Option／Resultはqualified core nameと特殊化済みcase payload全体を使います。Inline typeは有限で、reserve／hash前にdescriptor 4 MiB、type depth 64、parameter／field／case／payload各256へ制限します。これはABI fingerprint入力でsource-value／artifact formatではありません。Generated type identifierは `LANGCTYPE\0`、同じprofile／target、type encodingのdigestを使い、別headerのtype名collisionを防ぎます。

Headerは具体symbolと任意の可読aliasを定義します。Alias重複、source／runtime symbol collision、競合import signature、digest collisionはobject publication前に拒否します。不適合な旧headerは別symbolを参照してlink／bindingで失敗します。Artifact digestは `LANGCART\0`、profile／target、full function descriptor byte stringを辞書順sortしたvectorのSHA-256です。Artifact固有support symbolもこのfull digestを含み、複数headerをincludeするときは可読support aliasを無効化できます。これらはcomponent ABI／source native layoutと独立し、任意artifact byteを認証しません。

Artifact固有supportはcontext create／destroy／copied error-summary queryだけで、任意source／bytecode loadingやplugin登録は提供しません。Createは明示allocator descriptor、checked limit、そのartifactの宣言import binding／capability grantを受け取り、公開前にrequest全体をpreflightします。Contextは1つのartifact digest／allocator instanceに結び付き、guest reentry／poisonをeffect前に拒否します。Destroyはpointerをconsumeし、その後のnative利用はcaller lifetime契約違反です。任意dangling addressを検証できるという保証ではなく、numeric serializeもsupported formatにしません。Descriptor field順は以下のとおりで、各設定structの先頭はu32 `version=1, size=sizeof(struct)` とし、不明version／size／flagを拒否します。

| Descriptor | version／size後のfield | 両targetのsize／alignment |
| --- | --- | --- |
| Allocator | `void *user; void *(*allocate)(void *, uint64_t, uint64_t); void (*deallocate)(void *, void *, uint64_t, uint64_t);` | 32 / 8 |
| Limits | u64 `live_bytes, cumulative_bytes, live_allocations, frames, cells, host_calls, fuel`、u32 `cleanup_records, reserved` | 72 / 8 |
| Import binding | `const char *name; uint64_t name_bytes; uint8_t signature_digest[32]; void (*function)(void); void *user; uint64_t flags;` | 80 / 8 |
| Program config | const allocator／limits／import-binding descriptor pointer、u64 `import_count`、u8 `artifact_digest[32]`、u64 `flags` | 80 / 8 |

Allocator／limits pointerと両allocator callbackはnon-null、import pointerはcount 0時だけnull可です。Import nameはnonemptyでNULを含まない厳密UTF-8 qualified nameで、NUL終端は必須ではありません。Import `function` はnon-nullで、消去したfunction-pointer型は一致するgenerated callback型へだけ戻し、data pointerを経由しません。Import flag bit 0はnative FFIの明示grant、他bit、全config flag、limits reserved fieldはzeroです。Duplicate、宣言と厳密に一致するsignature／digest、全grantを含む全descriptorをsymbol／callback利用前に検証します。Importは最大4096、cumulative name byteは4 MiBです。Host bufferは記載extentを持つ実C objectでなければならず、adapterは検証dataをcopyしcaller所有descriptor pointerを保持しません。Allocator user context／function実装は規定lifetimeを保持します。

Limitsのdefaultは前述allocation上限とRFC 0006のframe 256、cell／fuel 1,000,000、host call 1,000、cleanup record 256です。Zero budgetは有効で該当操作を拒否します。Fuel／host_callsの `UINT64_MAX` だけが明示unboundedを意味します。Frames／cells／live-allocation limitは実装のchecked native address／metadata capacityへ収まり、cleanup recordは最大4096で、非対応capacityはcreate時に拒否します。Support形状は `u32 create(const config *, context **out)`、`u32 destroy(context *)`、`u32 last_error(context *, error_summary *out)` で、各実symbolにartifact digestを含みます。Create失敗はoutを変えず、descriptor検証はallocator callbackより先です。汎用stable embedding APIではなくartifact固有契約です。

Statusはu32値で `0 success`、`1 invalid argument/context/ABI`、`2 guest panic`、`3 runtime failure`、`4 cleanup failure`、`5 capability denied`、`6 poisoned instance` とし、Rust enum layoutではありません。Error summaryは妥当なcaller outputへcopyし、順に `int64_t panic_code; uint32_t status, source, start, end, cleanup_count, flags;` を持ち、両targetでsize 32／alignment 8です。Sourceはartifact source-table index、start／endは半開byte offsetで、`UINT32_MAX` とzero offsetはspanなしです。Flag bit 0はcleanup report切り詰め、他bitはzero、primary guest panic以外のpanic_codeはzeroです。Full stack／secondary recordはhost structured diagnostic interfaceに保持し、summary copyでguest storageは確保しません。成功guest entryは前のerrorをclearし、失敗はprimaryを記録し、拒否reentry／poisonは既存primaryを上書きしません。Query失敗はoutputを変えません。Destroyはvalid contextを先にrevokeしてbest-effort teardownし、cleanup失敗statusでも消費したcontextを再利用可能にはしません。

## 実行mode

Typed HIR／MIRはbackend非依存で初期化順、loan／region、drop obligation、failure edge、allocator identity、spanを保持します。同じinput／grant／limitならVM／JIT／AOTでvalue、初期化／move state、cleanup順、failure優先順位、publication atomicity、allocator結果を一致させます。C profile 1はunit専用call規約を変更しません。Pure guest fixtureは全mode、C header／symbol／pointer検証はnative adapterで実行し、VMはtyped host binding経由で同値性を確認します。汎用VM-to-C export APIの実装は主張しません。

Scriptのfuel／call／frame／cell defaultはRFC 0006のままです。Native interruptionを暗黙追加せず、hostはuninstrumented native実行の制限を明示し、allocation／capability sandboxとして提示しません。Ownership operationをschema 1へserializeせず、正確なversioned encodingはencoder／decoder変更前の別判断です。Constant execution、async cancellation、Wasm mapping、component resource layoutは変更なし／延期です。

### Cを区別するIRとartifactの段階導入

F01の実装前となる2026-10-19 JSTに、実験的委譲のもとでこの判断を受理します。HIRとownership MIRはimport ABI（`Host`／`C`）とfunction ABI（`Guest`／`CExport`）を明示的に保持します。名前、引数数、visibility、`i64` signatureをABI identityの代わりにはしません。従来host familyは正確な`(i64) -> i64`と従来のgrant／実行規則を維持します。C importは型付きparameter／result宣言を持ち、C exportは追加C wrapperを要求しますが、guest間callは従来のprivate規約を維持します。Public C exportはroot moduleの関数です。独立ownership MIR verifierでも有限CopyのC-valid signatureを検証し、偽造loan／owner／再帰signatureを拒否します。Source検査だけには依存しません。

Unsafe statementの初期化、move、loan、region、failure、cleanup解析は通常のlexical blockと同じです。Lexical markerを使うのはsourceのforeign call義務だけで、capabilityを与えたり通常の検査を無効にしたりしません。C interfaceを持つunitは全valueがCopyでもownership MIR経路へ進み、Copy profileでABI identityを消しません。型付きbindingとnative adapterがunit全体を扱えるまでは、binding／guest allocation／guest effectに先立って、実行とnative object公開を未対応C interfaceとして拒否します。従来hostやVMへのfallbackは認めません。

[RFC 0011](0011-ownership-artifacts.md)のschema 2にはC-FFI表現を含めないと明示されています。Schema 1／2のbyte、tag、flag、意味を維持します。EncoderはC import／C exportを見つけた場合、artifact公開前に妥当性確認済みlocation付きの構造化unsupported-profile errorで拒否し、ABI markerを省いたり通常host importとして符号化したりしません。旧形式のdecode結果は明示的にHost／Guestです。将来portableなC interface artifactを追加する場合は、encoding変更前に別のversion／互換性判断が必要です。これは受理済みartifact範囲を保持する判断であり、source C実行、native C wrapper、typed host bindingを通すVM同値性はMilestone 2の必須作業に維持します。上で非対象とした汎用VM-to-C export APIは追加しません。

理由と代替案：scalar host familyの流用はgrantを混同し、受理済みsignatureも表現できません。Backendだけのside tableへABI metadataを置くと共通の検証境界が失われます。現時点のwire version追加はC consumerより先に形式を固定し、schema 2の暗黙拡張は既存契約に反します。まずin-memory ABI identityとcodecの明示的拒否を実装します。この判断からC callのcost、pointer安全性、性能は推定しません。C pointer／context／capability preflight、正規descriptor／header、実adapterはF02-F07で別途検証します。

2026-10-19 JST確認の証拠：[ownership宣言verifier](../../../crates/mir/src/unit/ownership/declarations.rs)と[scalar-host契約test](../../../crates/mir/tests/ownership_declarations.rs)はi64引数／result各1個を許し、bool resultを拒否します。[schema 2の順序付きimport encoding](0011-ownership-artifacts.md#payload-and-table-identity)にABI discriminatorはありません。C／Rust layoutの仮定ではなく、これらのrepository契約に基づく追記です。各経路の有効化前に、malformed input、旧byte互換性、static source診断、unit全体の拒否の証拠を追加します。

## ABIと互換性

影響domainはsource、tooling、native-abi、bootstrap、将来のownership bytecodeです。`unsafe` name migrationを除きRFC 0006 Copy meaning／golden byteを保持し、既存 `extern fn name(i64) -> i64` typed host宣言はquoted C markerと区別して従来behaviorを保持します。Foreign宣言を通常host importへ、または逆方向へ暗黙bindしません。

C profile 1は独立番号の新adapter契約で、全APIのstabilizationではありません。Scalar width、bool／tag validity、ownership、status、descriptor version、digest入力、failure-output policyの変更には新profile／versionとmigration fixtureが必要です。Headerにtoolchain version、target、full digest、profileを記録し、Cargo.lock手動編集／Rust-layout serializationはしません。Component negotiationとpackage manifestはそれぞれの受理済み規則を維持し、2つのsource moduleのlinkから独立package互換性を保証しません。

## 診断とtooling

受理済みownership／loan／type／reserved-name categoryには `SRC0020`–`SRC0024` を再利用し、unsafe／foreign authority不足と不正C signatureに `SRC0025` を予約します。Resource limitは `SRC0011` です。Runtime identityは `runtime.panic`、`runtime.cleanup-failed`、`allocation.limit`、`allocation.out-of-memory`、`allocation.overflow`、`ffi.invalid-value`、`ffi.failure` と元fault／call／cleanup spanを保持します。FFIはcapability不足、不正value、symbol／signature不一致、containしたcallback failureを文字列だけによらず区別します。

Lossless parser／formatterは厳密C marker、unsafe scope、intrinsic type argumentを保持します。LSPはcurrent revisionで検証／publishし、foreign／unsafe要求、allocator／ownership annotationを表示します。自動fixでunsafe挿入、allocator変更、ownership弱化、cleanup failureの握り潰しはしません。Native debugは明示cleanup／failure rangeを保持し、hidden adapterの架空source declarationは作りません。汎用DAP／linterは予定milestoneの作業です。

## Performanceとcost model

このRFCは判断とtest義務を供給し、計測結果は主張しません。Drop flag／順序metadata、status check、allocator bookkeeping、context preflight、C変換にはcostがあります。全program解析で冗長処理を除去できてもfailure／destruction結果は変えません。Metadata事前確保はentry／allocationへcostを移し、fallible growthは一時的に旧／新storageを同時保持するため、releaseまで両方liveへ計上します。C aggregate pointer adapterはsource Copy型でもcopyし得ます。Digest／header assertionはbuild workで、runtime performance保証ではありません。

Compile latency／object／header size、allocation／growth成功失敗、正常／error／panic cleanup、graph teardown、scalar／aggregate C call、peak live byte、代表的source workloadのRust／C比較を両tier-1 hostで計測します。Allocator／request／RSS計上範囲を分けて報告します。現shared engine／copy-native testはprerequisiteであり、このprofile全体のperformance証拠ではありません。

## Securityとtrust boundary

Source／IR／artifact／handleは各境界でuntrustedとして検証します。Allocator callback、native C、実C caller addressはlifetime／no-unwind／nonreentry義務を持つtrusted inputです。Capability preflightをnative lookup／callbackより先に行い、name／path／digestから任意artifact byteを認証しません。Counter／size／generation枯渇はfail closedで、stale handleは再利用にaliasしません。Cleanupで新authorityを与えず、topology lock中にuser callbackを呼びません。Unsafe sourceはverifierをoverrideできず、native process隔離はhostの責任です。

## 代替案

- **変更なし：** Host Rust cleanup／global allocationと個別C harnessを継続する。第2gateとownership-aware production exitは未完了のまま。
- **Library／toolingだけ：** Allocation／C関数をwrapperで供給できても、全MIR edgeのreturn-slot publication、drop flag、unsafe scope、failure cleanupは決められない。
- **最も強力な競合design：** Native exception／unwind、無制限raw pointer／C layout／variadic、暗黙global allocation。Interopは広がるが、最初の安全なowning profileより前にplatform unwind／pointer validity／effect／resource authorityの契約が増える。将来の明示profileで拡張できる。
- **より単純な境界：** 全panic／allocation failureでabortしinteger関数だけ公開する。Cleanup／変換は減るが、受理済みfallible core／containerと完全なCopy aggregate／native目標を満たさず、内部中間buildに限り利用できる。

## 実装計画

1. Owner dispositionを記録済み：RFC 0008をacceptedとして作業を続ける明示指示に従い、2026-10-17 JSTに受理しました。第2roadmap decision gateは完了し、implementation gateは別途維持します。
2. Ownership HIR／MIRの初期化／drop／failure flow、checked allocator authority／metadata reserve、backend-neutral cleanup／error stateを実装し、全spanとCopy behaviorを維持する。
3. Shared runtimeのhost Arc／storage戦略をfallible guest allocator契約へ適応してからsource operationを公開し、arena／core／containerのlifetime／failure経路を追加する。Graph lock内でguest callbackを呼ばない。
4. Unsafe／C構文、厳密generated descriptor／header fixture、typed import／export adapter、context検証を実装する。MacOS AArch64／Windows x86-64実compilerで規定size／alignment／offset／symbolを検証し、未対応形状はeffect／publication前に拒否する。
5. Serialize変更前にownership bytecode encodingを決定／version化し、VM／native ownership／cleanup／allocator／call差分、link／run、source debug、代表的Rust／C計測、対訳capability guideを実装する。

## Test計画

| 分野 | 必要なevidence |
| --- | --- |
| Cleanup | 実初期化の逆順、branch／loop flag、moved owner除外、aggregate temporary、return escrow、明示drop、typed Err、panicと全runtime-limit edge |
| Failure | Primary／secondary順、report overflowでも全cleanup継続、未公開result cleanup、host panic／destructor containment、poison／reentry／teardown、native unwind不使用 |
| Allocation | Authority不足、live／cumulative／count／alignment厳密境界、zero要求、元allocator、checked i64／size overflow、data／metadataの各reserveへのOOM注入、transactional growth／publication |
| C validity | 全許可scalar／record／enum／Copy Option／Result、空record、不明tag／bool／reserved byte、valid-object host fixtureでのnull／misalignment／overlap、ownership／borrow signature拒否 |
| Header／platform | 実C compilerのstatic assertion／call、固定fixture／digest／symbol、両tier-1 hostのdirect／response-file link／run、旧header／signature collision拒否 |
| Authority | Unsafe blockはbindingをgrantしない、grant不足でlookup／callbackなし、全import preflight、host／foreignの取り違え／source-rootからFFIへの昇格拒否 |
| Cross-mode／trust | VM／JIT／AOTで同じvalue／cleanup／fault span／allocator trace、不正ownership IR／artifact／handle、deterministic fault injection、fuzzと代表costの範囲を正確に報告 |

## 未解決事項

Ownerは明示cleanup／status flow、recoverable budget付きguest allocation、有限generated C boundaryを受理しました。規定descriptor／layout／digestはimplemented前に実fixtureで確認し、汎用embedding APIと厳密ownership bytecodeは個別の受理済みversioned契約を必要とします。暗黙に補ってはいけません。このprofileにuser-defined destructorやraw-pointer source escape hatchはありません。

Review outcome：**cleanup／allocator／unsafe／C-validity semanticsを2026-10-17 JSTに受理済み**。完全なgenerated descriptor fixture、implementation、platform conformance、measurement、stabilizationは主張しません。

F03の製品実装evidenceを2026-10-19 JST確認：[descriptor／layout／header生成](../../../crates/runtime/src/c_abi.rs)は従来scalar vectorを変えず、このprofileからPython struct／hashlibで直接計算した[独立aggregate／type／artifact vector](../../../benchmarks/fixtures/c-abi-profile-1-aggregate-fingerprints.json)とも一致します。[Compiler test](../../../tests/c_abi.rs)で両target・両方向のdescriptor、正確なC field／payload配置、callback／aggregate／Unit署名、headerの再読込み／共有型、曖昧なalias、resource上限、暗黙builtin payloadを含むsource／MIR／展開後Cの一致したdepth境界を検証します。Host C compilerとfreestanding Clangのtarget検査は生成headerの実装evidenceであり、F02 context／事前検査とF04-F07の実行／platform gateは引き続き必要です。SHA-256にはRustCrypto sha2 0.11.1を使います（package metadataを2026-10-19確認。Cargo metadataに公開／更新時刻の記載なし）。

F10検証evidenceを2026-10-19 JST確認：[共有事前検査](../../../crates/runtime/src/c_abi/validation.rs)は必要な初期化済みfield／reserved／unused unionだけを読みます。Native adapterは未初期化C paddingを含むRust u8 sliceを作らず、実native objectの生存／access有効性は前述のcaller／adapterの義務です。[Fixture](../../../tests/support/c_validation.rs)では通常paddingへのread自体を拒否し、tag zero／未知tag、bool／Unit、active case全体sizeofと実union extentの区別、read前のaddress全体検査、output alias／null／short／alignment／overflow、共有workとinterruption／deadline／read failureを確認します。[生成header test](../../../tests/c_abi.rs)もhostと両Clang targetでunion sizeofを独立検査します。F02のcontext evidenceは後述し、F04-F07のadapter／platform実行は未完了です。

F02共通context gate（2026-10-19 JST実装・確認）：全descriptorのversion／size／flags、必須pointer、count／UTF-8名／合計4 MiB、全importの重複・欠落・署名・明示native FFI grant、backend metadata容量とcleanup上限を確認してからsnapshotを所有します。Caller descriptorや名前への参照は残さず、code/user pointerの生存義務を維持します。実code ownerを保持するProgram登録と個別allocator identityで、同一ABIの別programを拒否します。Mutable state取得前のtry-borrowで再入を拒否し、既存cleanup Instanceのpoisonと実行制御／Usage／ServiceWorkを共有します。ImportはF10検査後にhost callを1回計上し、非zero statusをguest panic／enumに変換せず、最初のfailureを保持し成功resultを検証します。Rust body／callback／reader panicとpanic payload destructorを上限付きでcontainします。後続cleanup失敗でもprimaryとcallsite spanを保ち、切捨て分を含むcleanup件数を32-byte integer-only summaryへcopyします。通常guest panicは再利用可能で、cleanup／host panicはpoisonし、再入／poison拒否では前のprimaryを保持します。Destroyはteardown前に失効します。[実装](../../../crates/runtime/src/c_abi/context.rs)と[16件のfailure/state fixture](../../../tests/support/c_context.rs)を参照してください。Raw C read、正確なcallback型への復元／呼出し、foreign allocator bridge、VM/native adapterとcaller output公開はF04/F05、実platform検証はF06/F07に残ります。C実行／object／schema 2の拒否は維持します。

F11のscalar C source実行（2026-10-19 JST実装・確認）：Rust embedding向けの明示入口`lang_vm::unit::owned::execute_entry_c`と`lang_codegen_clif::owned::CCompiledProgram`が、共通context Session越しにscalar／Unit importを実行します。C対応native関数はRFC 0012のprivate revision 2を使い、通常のrevision 1 JIT／objectはsymbolとC unit全体の拒否を維持します。型付きnative thunkが、u8 bool、call-local Unit pointer、out-result、最大256引数を含む生成C callback署名へ正確に復元します。Function pointerはcode pointerのままです。Native CAllocatorはRustで読む前に全byteを初期化し、元のallocator／user／正確なrequestで解放します。実accounting identityをcontextへ一度だけ結び付け、再構築による累積quotaのresetを拒否します。Source受入れはeffect前に実code owner、C metadata全体、grant、実allocator、control／workを照合します。C compiler産fixtureをsource VM／nativeから呼び42／59、256引数で32,896を返し、全有限fuel／work境界、foreign／未知status、不正bool、OOM、interruption／deadline、別owner／allocator交換を検証します。失敗時はsource main resultを公開しません。[名前付きsource](../../../examples/analysis/c_scalar_imports.lang)、[C fixture](../../../tests/support/c_imports.c)、[VM／native fixture](../../../tests/support/c_imports.rs)、[native wrapper](../../../crates/codegen-clif/src/owned/c_imports.rs)を参照してください。明示context／binding APIと信頼されたnative fixture runtimeであり、汎用aggregate import、生成raw C context factory／export、C対応object公開、完全なnative packaging／platform検証はF04-F07／P02に残します。既定CLI／従来実行入口とschema 2 encodingは引き続きC unit全体を拒否し、外部C profile 1のdescriptor／digest／headerも不変です。名前付きfixtureは`cargo test --test c_abi imports::named_scalar_c_example_returns_59_on_vm_and_native`で実行します。

2026-10-17 JST確認のprototype evidence：[layout fixture](../../../benchmarks/fixtures/c-abi-profile-1-layout.c)はApple Clang 21.0.0（`clang-2100.3.34.2`）で `clang --target=arm64-apple-macos -std=c11 -Wall -Wextra -Werror -fsyntax-only` と、同じcommandの `--target=x86_64-pc-windows-msvc` をpassしました。[Fingerprint vector](../../../benchmarks/fixtures/c-abi-profile-1-fingerprints.json)は両direction／targetのscalar descriptor byte／SHA-256を記録します。提案fixtureであり、compiler生成header、native Windows／MSVC実行、source C interoperabilityではなく、上記implementation gateは残ります。

## 情報源

F09 source dropのevidence（2026-10-19 JST更新・確認）：[Bytes](../../../examples/analysis/explicit_drop.lang)と[最後のstrong／weak期限切れ](../../../examples/analysis/explicit_drop_shared.lang)で受理済みgeneric dropをVM／native実行します。HIR署名／型診断でCopyの元の値・affine move・lexical loanを維持し、source専用dropを既存DropValue／Unitへ下げ、閉じたruntime core tableは拡張しません。既存schema 2のencode／decodeから両fixtureを実行できます。即時破棄、有限control／provider拒否、cleanup失敗時の一度だけの消費を[対象fixture](../../../tests/support/native_drop.rs)で検証し、両sourceを共通standalone target matrixにも追加します。Source panic・動的code／非復帰・version／互換性判断はF13に残し、dropではformat／native packet revisionを変更しません。

F04実装evidence（2026-10-19 JST更新・確認）：明示VM／contextとnative revision 2のimport APIでfinite Copy aggregateに対応しました。[Field単位の転送](../../../crates/runtime/src/c_abi/transfer.rs)は実allocatorを使い、初期化済みの呼出し中C objectをmanaged storageと分離します。正確なnative thunkはRust byte access再開前に通常paddingを初期化し、不正な正規fieldはF10拒否用に保持します。[実C fixture](../../../tests/support/c_imports.rs)はrecord／空record、enum、入れ子Option／Result／error、scalar result、256混合引数、全有限fuel／work境界、scratch OOMを検証します。[名前付きsource](../../../examples/analysis/c_aggregate_imports.lang)は両modeで84を返します。Raw C factory／export、C object公開、完全native packaging／platform、ownerとCの複合conformanceは未完了で、既定CLI／schema 2のC拒否は不変です。既存の受理済みprofileを実装し、source構文・C digest・private helper／packet revisionを追加しません。

- [RFC 0004](0004-owned-values-and-cyclic-graphs.md)：受理済みlifetime／cycle／arena規則。[RFC 0007](0007-source-ownership-and-core-values.md)：受理済みsource／core／allocator型表面。ともに2026-10-17 JST最終更新、同日確認。
- [RFC 0003](0003-versioned-component-contracts.md)：component failure／ownership境界。[RFC 0006](0006-source-frontend-and-portable-programs.md)：Copy semantics／resource limit／schema 1。ともに2026-10-17 JST最終更新、同日確認。
- [Architecture](../architecture.md)、[roadmap](../roadmap.md)、`crates/runtime/src/shared.rs` とtest（`ec16f8a`）、native copy backend／test（`926fb3b`）、ownership grammar（`eb3b16f`）：実装と残作業のevidence。2026-10-17 JST更新、同日確認。
- `benchmarks/fixtures/c-abi-profile-1-layout.c`、`c-abi-profile-1-fingerprints.json`：提案evidence、2026-10-17 JST更新／確認。Layout主張は上記Clang cross-target検証で支え、native Windows／MSVC検証は未実施です。他のsemanticsはproject提案で、外部言語やperformanceの新知見ではありません。
