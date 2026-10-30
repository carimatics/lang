<!-- translation-of: ../../rfcs/0011-ownership-artifacts.md; canonical-language: en -->

# RFC 0011: Portable ownership artifacts

[English](../../rfcs/0011-ownership-artifacts.md)

- Status: implemented
- Authors: Project contributors
- Created: 2026-10-18
- Last updated: 2026-10-30
- Tracking issue: [実装Todo](../milestone-2.md) のM2-A01/A02/A03とN01
- Supersedes: なし。RFC 0006のschema 1と並行してownership schema 2を追加
- Superseded by:

拡張（2026-10-19）：[RFC 0014](0014-explicit-source-panic.md)は明示的Panicを含むunitにschema 3を選びます。本書のschema 2 profileは閉じたままbyte互換性を保ち、33命令tagと37runtime core tagは変えません。

## 現在の実装状態

CLIのdisk admissionを明確化（2026-10-30 JST）: artifact入力は通常fileに限定し、Unixではnonblocking openしたhandleを有界readの前に検査します。FIFO/deviceは無期限に待たずI/O入力として拒否します。既存byte上限・診断span・schema・memory内decoder APIは変更しません。

2026-10-20 JSTに[M2 capabilityと検証根拠](../milestone-2-capabilities.md)と照合済みです。本RFCの範囲はimplementedであり、stabilizedではありません。以下の当初計画、decision時点の記述、日付付きcheckpointはrollout履歴として読みます。そこで別作業としていたnative／SDK／platformの現在状態は上記ガイドを参照してください。今回発見したRFC 0012の論理debugger検証不足は同ガイドの訂正で解消し、本RFCの非目標や規範的要求は変更しません。

## Summary

RFC 0007／0008／0009のownership program用にportable schema 2を追加します。Schema 1とCopy encodingは維持します。Backendに依存しないownership declaration、place、typed CFG操作を明示的に保存し、load時に全proofとfailure-flow planを独立に再構築します。Live valueやruntime authorityは保存しません。実験段階のartifact契約であり、安定したserialized MIRやnative／component ABIではありません。

## Motivation

`9d9d2ac`時点のsource VMはowning Bytes／Text、入れ子collection、shared／weak、字句的arena値を実行できますが、`lang-bytecode::unit`はschema 1とCopyの`VerifiedUnit`だけに対応します。Ownershipの`BodyTable`、loan／region proof、failure elaborationは別に存在します。Rust enumをbyte列とみなしたりschema 1のtagをそのまま拡張すると、受理済み互換性境界を変えてしまいます。2026-10-18 JSTに検証したrepository上の観測であり、性能結果は推定しません。

## Goals

- 受理済みの有限ownership型／操作profileと元の全source spanをround-tripする。
- 不正な構造／semanticsを持つ非信頼入力を、host bindingやguest effectの前に拒否する。
- 直接VM実行とnative AOTで、独立検証済みのsemantic contractを共有する。
- 実装前に互換性、数値tag、field順序、決定的encoding、resource上限を決める。

## Non-goals

Live heap／session snapshot、IRの代わりのsource埋め込み、native machine code、proof certificate、allocator callbackの保存、安定したMIR／native ABI、新source構文、一般generics、component／package形式、圧縮、新たなfilesystem／network authorityは対象外です。Schema 2だけで未対応native操作やindirect REPL previewは有効になりません。

## Guide-level design

[Bytes fixture](../../../examples/scripts/owned_bytes.lang)のようなowning scriptを、将来は一度compileしてsourceを再parseせずloadできます。埋め込んだsource snapshotは診断用だけで、pathはfile accessを許可しません。Load時にunit全体を検証します。実行には引き続き明示的allocator／host-import authority、新しいruntime予算、対応するentry／result契約が必要です。

Copy compilationはschema 1を出力し続けます。Ownership compilationはA02／A03で本契約を実装してからschema 2を出力します。旧loaderはversion 2を明示拒否し、新general loaderはversion別verifierへ振り分けます。Schema-1専用APIは従来の狭い契約を維持できます。不正version-2 payloadをversion 1やsourceとして再試行してはいけません。移行はsourceから再buildし、headerだけを書き換える方法は無効です。

A03で`lang compile [--source-root DIR] --output ARTIFACT SOURCE`と`lang run --artifact [--fuel N] [--host-calls N] [--frames N] [--cells N] ARTIFACT`を追加します。Compileは通常のscript entryを要求し、検証後にだけ出力を公開します。既存の`lang run SOURCE`はsource実行を維持し、artifactのloadは明示指定し、埋め込みpathを辿りません。Entryなしのlibrary artifactはembedding API機能に留めます。ここでcommandを規定しますが、A01では実装しません。

出力保護の訂正をRFC 0001の実験段階の委任で受理（2026-10-28 JST）：compileは、間接的なmoduleとpathの別名を含む**読み込んだ全source file**のいずれかを指す出力先を、公開前に拒否します。入口fileだけを保護すると、正常なcompileでimport先のmoduleを破壊できました。判定にはfile解析loaderの正規化済みidentityを使い、artifactから渡されたdisplay pathに権限を与えません。それ以外の既存artifact出力はatomicな置換を維持します。Source入力を出力先に指定していた利用側は別の出力先を選びます。Source構文、artifactのbyte列、ABIは変更しません。敵対的なfilesystemの並行置換はこのCLI検査の保証外です。`tests/artifact_cli.rs`で入口／間接module、別名、source byteの保存、正常なartifact置換を検証します。

## Reference semantics

### Envelope and primitive encoding

既存の16-byte envelopeを使います。8 byteの`LANGBC\0\0`、little-endian `u16` version **2**、`u16` flags **0**、`u32` payload byte長の順です。未知version／flags、切り詰め、長さ不一致、末尾余剰byteを拒否し、拡張sectionを黙って読み飛ばしません。

Count、長さ、index、offset、IDはすべてlittle-endian `u32`で、host `usize`との変換を検査します。符号付きliteralはlittle-endianの2の補数`i64`、tagとbooleanは`u8`で、booleanは0／1だけです。Vectorはcountの後に要素、stringはbyte長の後に終端記号なしの厳密なUTF-8 byteを置きます。Name／source／pathの妥当性規則はschema 1を引き継ぎます。Source pathは`/`区切りの正規化された相対display pathで、filesystem authorityにはなりません。

`Span`はsource index、開始byte offset、終了byte offsetの3つの`u32`です。Schema 1の省略可能なinstruction spanと異なり、既存raw ownership契約に合わせて必須とし、範囲とUTF-8境界を検証します。`TypeUse`はtypeとspanの順です。Optional IDは不在のtag 0、またはtag 1と`u32` IDで、他のtagを拒否します。

Rust discriminant、pointer、platform layout、native padding、registry address、暗黙operandは含めません。以下の数値割当はRust宣言順序とは独立した規範です。

### Payload and table identity

Payload fieldは次の順です。

1. Source：`(path string, source-text string)`のvector。
2. Nominal definition：`(name, public bool, span, kind)`のvector。Kind 0はfield vectorを持つrecordで、各fieldは`(name, span, TypeUse)`。Kind 1はcase vectorを持つenumで、各caseは`(name, span, payload TypeUse vector)`。
3. Constructed type：以下のtagを使う`(constructor, span)`のvector。
4. Import：`(name, span, parameter TypeUse vector, result TypeUse)`のvector。現行の受理済みtrusted host-import signature `(i64) -> i64`を維持し、C-FFI import表現は追加しない。
5. Function declaration：`(name, public bool, span, signature span, parameter local-ID vector, result TypeUse, local vector)`のvector。各localは`(name, span, local-kind tag, TypeUse)`で、kind 0／1／2／3はparameter／mutable／pattern／arena-owner。
6. Function place：declarationごとに1件の`(value TypeUse vector, place vector)`。Placeは`(origin register, root, projection vector, span)`。Root tag 0／1はlocal／value ID。Projection tag 0はdereference、1は続くfield ordinalを表し、各projectionの後にspanを置く。
7. Function body：declarationごとに1件の`(loan-count u32, prepared-place-count u32, block vector)`。Blockは`(parameter-register vector, instruction vector, terminator, block span)`で、block 0がentry。Countは論理identityの個数で、activeなruntime registrationではない。
8. Optional entry function ID。存在するなら引数なし・result i64が必須。不在なら検証済みlibrary unitにできるがscript実行はできない。実行はartifact内のnameでなく検証済みIDを使う。

全vectorの順序とIDの意味を保持します。共有verifierが要求するname／pathの一意性を検証し、constructed引数をそのconstructed parentより前へ置きます。Field／case／block／instruction／initialization順序も保持します。Schema 1と異なり、schema 2はglobal tableを**sortせず**、等価なconstructed typeを採番し直しません。順序付きraw tableのencodingは一意ですが、等価でも順序が違うprogramのbyte一致は要求しません。Encode／decode／re-encodeはbyte単位で一致させ、順序なしhost mapのiterationを保存してはいけません。

### Types

| Type tag | Payload |
| --- | --- |
| 0 | i64、なし |
| 1 | bool、なし |
| 2 | nominal definition ID |
| 3 | constructed type ID |

| Constructor tag | Payload |
| --- | --- |
| 0 | builtin tag |
| 1 | Option：type |
| 2 | Result：success type、error type |
| 3 | Vec：element type |
| 4 | Map：key type、value type |
| 5 | shared：payload type |
| 6 | weak：payload type |
| 7 | arena reference：payload type |
| 8 | ordinary loan：loan-kind tag、target type |
| 9 | 論理arena owner：function ID、local ID |

Builtin tag 0–9は順にUnit、Text、Bytes、Allocator、AllocError、BoundsError、BufferError、Utf8Error、OwnershipError、ArenaErrorです。Loan-kind tag 0／1はshared／exclusiveです。いずれもtypeと字句的identityの記述であり、Allocator型やarena-owner型にallocator instanceやregion authorityを含めません。RFC 0007のstorage、Map key、再帰、signature、loan、arena escape制約を維持します。具体的core型引数のspanも保持し、独立に検証します。

### Instructions and cleanup ranges

Instructionは`(result-register vector, opcode u8, operands, span)`です。以下のIDはすべて`u32`、`vector`はcount付きvectorです。Verifierは保存されたresult listを信用せず、result個数／typeを検証します。

| Opcode | Meaning | Operands in order |
| --- | --- | --- |
| 0 | I64 | i64 literal |
| 1 | Bool | bool |
| 2 | Unit | なし |
| 3 | Add | left register、right register |
| 4 | LessThan | left register、right register |
| 5 | EqualI64 | left register、right register |
| 6 | Copy | value register |
| 7 | Move | value register |
| 8 | CopyPlace | place ID |
| 9 | MovePlace | place ID |
| 10 | Initialize | local ID、value register |
| 11 | Replace | place ID、value register |
| 12 | PreparePlace | prepared-place ID、place ID |
| 13 | ReplacePrepared | prepared-place ID、value register |
| 14 | DiscardPrepared | prepared-place ID |
| 15 | DropValue | value register |
| 16 | DropLocal | local ID |
| 17 | DropLocalIfInitialized | local ID |
| 18 | DropValueIfAvailable | value register |
| 19 | Cleanup | cleanup-step vector |
| 20 | Borrow | loan ID、loan-kind tag、place ID |
| 21 | EndBorrow | loan ID、end-mode tag |
| 22 | ArenaEnter | owner local ID、allocator register |
| 23 | ArenaExit | owner local ID |
| 24 | Call | function ID、argument-register vector |
| 25 | Import | import ID、argument-register vector |
| 26 | CoreCall | core-operation tag、型引数TypeUse vector、argument-register vector、optional result-loan ID |
| 27 | Record | definition ID、field-register vector、initialization-order ordinal vector |
| 28 | Variant | case ordinal、payload-register vector |
| 29 | IsCase | value register、case ordinal |
| 30 | CopyCaseField | value register、case ordinal、field ordinal、guard register |
| 31 | BorrowCaseField | loan ID、value register、case ordinal、field ordinal、guard register |
| 32 | UnpackVariant | value register、case ordinal、guard register |

End-mode tag 0／1はrequired／if-activeです。Cleanup stepは`(target tag, target ID, span)`で、target tag 0／1／2／3はlocal／value／loan／arena-owner localを選びます。Cleanup rangeはobligationを表し、実破棄は引き続きRFC 0008の動的な逆初期化順序に従い、ownerより先にregistrationを終了します。Prepared destination、record初期化permutation、値全体のsum取り出し、provisional core loanは既存の独立検証済みsemanticsを維持します。保存されたlistでcleanup優先順位を上書きしたり、ownerが初期化済みだと宣言することはできません。

RFC 0009の取り出しを含むcore-operation tagを次の表で固定します。

| Tag | Operation | Tag | Operation | Tag | Operation |
| --- | --- | --- | --- | --- | --- |
| 0 | Allocator | 13 | VecNew | 26 | SharedRead |
| 1 | TextFromUtf8 | 14 | VecLen | 27 | SharedReplace |
| 2 | TextClone | 15 | VecPush | 28 | WeakNew |
| 3 | TextLen | 16 | VecPop | 29 | WeakClone |
| 4 | TextBytes | 17 | VecGet | 30 | WeakUpgrade |
| 5 | TextEqual | 18 | VecGetMut | 31 | ArenaNew |
| 6 | TextConcat | 19 | MapNew | 32 | ArenaRead |
| 7 | BytesNew | 20 | MapLen | 33 | ArenaReplace |
| 8 | BytesClone | 21 | MapInsert | 34 | OptionUnwrap |
| 9 | BytesLen | 22 | MapGet | 35 | ResultUnwrap |
| 10 | BytesGet | 23 | MapRemove | 36 | ResultUnwrapErr |
| 11 | BytesPush | 24 | SharedNew | — | — |
| 12 | BytesSet | 25 | SharedClone | — | — |

Terminatorは`(tag u8, operands, span)`で、0はreturn(value register)、1はjump(target)、2はbranch(condition register, then target, else target)、3はunreachable(operandなし)です。Targetは`(block ID, argument-register vector)`です。Unreachableへ到達したらcontainされたinvariant failureとし、残るcleanupを省略する許可にはしません。すべての階層で未知tagをerrorにします。

### Verification, effects and authority

まず上限付きraw tableをdecodeします。実行可能artifactを返す前に、ownership type／declaration／placeとtyped CFG／SSA、move obligation、loan activity、推移的reference provenance／access、region identityと再帰call effect、完全なfailure／return cleanupを独立検証します。Failure CFG、layout／slot plan、graph inspection、retirement準備は検証済み契約から再生成します。Proof boolean、dominator、hash、liveness／region summary、実行可能host pool、failure-flow certificateを保存／信用しません。Raw tableだけを返すdecoderは、非実行可能な状態をtype／APIで明示しなければなりません。

埋め込んだsource textを再parseしてproofにしてはいけません。Type ID、span、declaration、core signature、初期化permutation、cleanup range、loan終了、arena-owner symbolを個別検証します。Source snapshotに別の正しいprogramがあっても、偽造ownership操作を拒否します。Import binding、allocator authority、動的execution／host上限は明示的なinstantiation入力のままで、検証成功だけでは付与しません。Decode／検証中のguest allocation、provider呼び出し、host callback、filesystem readは禁止し、上限付きcompiler dataのhost allocationは許可します。

算術、result match、borrow、panic code、cleanup順序、中断、poisonはRFC 0007／0008／0009のままです。Constant evaluation、scheduling、atomics、callback、data raceの新規則は追加しません。

### Resource limits

Schema 1の既定上限を維持します。Artifact 64 MiB、source当たり4 MiB、全source 16 MiB、source 256件、definition／function／import各4096件、field／case／parameter各256件、type depth 64、value cell 4096、register／place／block／instruction／loan ID／prepared ID各1,000,000件です。Constructed typeは最大65,536件とします。Decode／検証のrequested allocationは**128 MiB**、累積elementは**8,000,000**、charged workはload全体で**10,000,000**を既定とします。共有verifierの追加の明示的region／dataflow上限も適用します。

Reservation／traversalの**前に**残入力の最低長、count上限、累積element、checked arithmetic、requested byte、workを検査します。Decodeと全proof stageでload全体のledgerを共有し、phase resetで予算を再付与してはいけません。保持する入力copy、string／vector、type／CFG／provenance matrix、反復worklist、固定点の再訪、生成failure dataを数えます。呼び出し元所有の入力byteは除外し、requested allocation byteは後で解放しても計上するためresident memory量とは異なります。Encodeにも独立したoutput／一時allocation／work上限を設け、byteを公開する前にraw入力を検証します。失敗／cancel経路で部分的host bufferの解放を確認し、枯渇時はstructured errorとし、部分的実行可能unitを返しません。

共有verifierの全visitとdataflow set要素の再訪に加え、decode／encodeした各scalar field／tag、訪問した各vector要素／operand、最大1 KiBごとのUTF-8検証chunkを課金します。Encodeのsize算出／writeの両passもworkを消費します。追加課金操作を公開して厳密な小予算境界を試し、byte／count上限で反復work会計を代用しません。

設定可能なadmission予算であり、throughput／memory実測ではありません。既存APIへ累積admissionを統合できなければ、loader実装前にその前提作業を公開しA02を再見積もりします。各stageに新予算を与えて黙って代用してはいけません。

## Execution modes

直接source VMとloadしたschema 2は同じ検証済みownership／failure契約と対応操作profileを使います。Native AOTはwire encodingと独立にその契約を使い、object layoutとmachine conventionはN01–N05の別作業です。到達可能な全function／importを検証し、未対応実行をeffect前に拒否します。JIT、native REPL、WebAssembly／component実行の実装済み主張は追加しません。Runtime service準備は従来どおり上限付きでguest entry前に行います。

## ABI and compatibility

影響domainは`bytecode`と`tooling`（明示的compile／load commandとstructured artifact error）です。Source構文／typing、native／component／package ABIは維持します。Schema 1のbyte fixture、sort、検証、encodingはbyte単位で不変です。Version 2には独自の順序付きtable規則があり、scalarだけのbodyでもschema 1との互換性はありません。将来の非互換tag／field順序／semanticsには明示的schema決定と拒否／移行fixtureを必要とし、version 2へ未合意opcodeを追加しません。Pre-alphaでの受理は安定化ではありません。同じsource／toolchainからartifactを再buildし、binary translatorは要求しません。

## Diagnostics and tooling

Version／flags／tag／切り詰め／overflow／UTF-8／budget／validation失敗を、byte offset付きstructured artifact errorにします。上限付きspan検証後は可能な範囲で元source位置も示します。不正binary入力にsource `SRC` codeを新設せず、invalid spanを信用して診断を描画しません。Runtime stackと将来のnative debug metadata用に完全なsource snapshotを保持します。Parser、formatter、LSP、REPLのsource grammarは維持します。CLIの成功は検証後に公開し、filesystem出力失敗はhost errorのままです。

## Performance and cost model

保存するのはcompiler metadataと埋め込みsourceであり、heap snapshotではありません。Loadは上限付きtableを確保して高価なdataflow／region proofを再実行するため、trusted proof cacheにはなりません。Table順序の保持でcanonical graph採番を避けます。Byte単位の再現性は同じ順序付きtableに適用し、等価な全source programには要求しません。Startup、size、memory、runtimeの利点は主張せず、A03と既存benchmark行で計測してから判断します。

## Security and trust boundaries

出所や拡張子にかかわらずartifactを非信頼入力として扱います。長さ、name、debug path、論理owner、妥当なCRC／hash風dataでcapabilityを付与したりproofを代替できません。Executable／native pointer payload、native plugin load、暗黙process／environment／network accessはありません。Decode／検証上限でtrusted host callbackをsandbox化できるわけではありません。既存allocation／C-boundary契約を維持し、本RFCは`unsafe`を要求しません。

## Alternatives

- **変更しない：** ownershipをsource専用に保つと、受理済みartifact／conformance成果が未完了のままになる。
- **Library／toolingや埋め込みsourceの再実行：** 再compileでsource behaviorは維持できるが、ownership IR round-tripにならず偽造IRを独立検証できない。Debug textは情報用に留める。
- **Schema 1をそのまま拡張：** codecの再利用は増えるが、旧version検査が誤解を招き、非互換type／operation tagを重ねる。明示versionを選ぶ。
- **Elaborated bytecodeとproof certificateを保存：** 再計算を一部減らせる可能性はあるが、派生cleanup／layout／proof内部までtrusted formatを広げ、なお独立検査が必要。Raw semantic tableだけを保持する。
- **Rust enum／serde layoutの単純snapshot：** 初期仕様を減らせるが、保存dataを実装layoutと暗黙enum順序へ結び付ける。明示的な有限field／tagで互換性境界を小さくする。

## Implementation plan

1. A01：日英文書で本決定をreview／受理する。受理は設計依存作業の開始だけを認める。
2. A02：最初に累積budget admissionを監査／prototypeし、独立前提作業を実装前に公開する。上限付き明示type／place／body codec、不正入力coverage、fuzz targetを実装し、raw decode結果は非実行可能に保つ。
3. A03：loadしたunitを独立再検証してinstantiateする。Compile／load／script実行を接続し、artifact基盤をさらに広げる前に最初のowned Bytes artifactを実証する。VM結果、error、cleanup／loan behavior、spanを直接source実行と比較する。
4. N01以降のnative／platform行：共有された検証済み契約を使い、既存の差分／ABI／platform／性能／debug証拠を揃える。Capability文書は実証した動作だけを更新する。

## Test plan

独立したschema-1 byteと旧／新loaderのversion拒否を固定します。手作業のschema-2 scalar／Bytes vectorでheader／tag／field順序を厳密検証し、33 opcode、37 core操作、全type constructorをinactive再帰shape／CFG引数も含めround-tripします。Table順序とrecord初期化順序を保持したdecode／re-encodeのbyte一致を確認します。

未知tag、不正boolean、count／offset overflow、各境界の切り詰め、不正UTF-8／span、identity重複、前方constructed参照、不正result list／entry signatureを拒否します。妥当fixtureのmove、loan終了／custody、prepared destination、cleanup obligation、guard、cross-arena store／return、再帰call effectを偽造し、source textで救済できないことを検証します。Phase境界をまたぐ厳密なload全体予算とallocation失敗時の解放を試し、任意host importは実行せず、上限付きdecode／検証をfuzzします。

A03はBytes／Text／Vec／Map／shared／arena、provider OOM、fuel、中断について、直接／loadしたsourceの結果と元spanのfailure／cleanup traceを比較します。Native／macOS／Windowsと最終benchmark／debugの受入は既存行に残し、codec testで閉じません。

## Open questions

実装済み形式に未解決のblockerはありません。Load全体のledger、load／直接実行、source span、cleanupはA02／A03と完了済み[M2監査](../milestone-2-capabilities.md)で検証済みです。Native／platformとbenchmark／debug gateは記録されたrevisionで完了していますが、将来の性能を保証する観測ではありません。上記2026-10-28の出力保護訂正により、読み込んだsourceと同じ出力先には別のpathが必要です。形式契約を変える前にRFCを改訂します。

## Decision

2026-10-18 JST、**実験段階の委任に基づき受理**しました。RFC 0001と受理済みownership／cleanup契約に整合すると判断しました。Schema 1を維持し、明示的な有限tagで既存ownership 33 opcodeとcore 37操作を網羅し、runtime authorityを保存せず、独立検証を必須にしています。順序付きraw tableの保持により新たなgraph canonicalization要件を避けます。日英heading、link、tag網羅、repository整合性を確認しました。完了するのはA01の決定だけで、codec、loader、native backendやexit gateの実装完了ではありません。前述の累積budgetと実行等価性の仮定はA02／A03の検証要件として残します。契約を変える実装の前に本RFCを明示改訂します。

## Sources

2026-10-18 JST実装記録（A02／A03）：schema 2、独立検証、明示的compile／load command、検証済みentryによるVM dispatchを実装しました。最初のload済みBytes fixtureは`0`を返します。Service 10 fixtureで全provider拒否位置／fuel prefixと中断／再利用を比較し、CLI testで両version、metadata／entry拒否、入出力上限、削除した複数fileのsnapshotを検証します。`implemented`は本format／現VM契約に対する状態であり、安定化やnative／platform exit gateの完了ではありません。

- Project決定：[RFC 0001の委任／互換性](0001-project-charter.md)と[RFC 0006 schema 1](0006-source-frontend-and-portable-programs.md)は2026-10-17 JST更新、[RFC 0008 cleanup／authority](0008-cleanup-allocators-and-c-boundaries.md)は2026-10-17 JST更新、[RFC 0007](0007-source-ownership-and-core-values.md)と[RFC 0009](0009-whole-sum-ownership-extraction.md)は2026-10-18 JST更新。
- 設計時点の証拠：[schema-1 codec](../../../crates/bytecode/src/unit/codec.rs)、[ownership type](../../../crates/mir/src/unit/ownership.rs)、[place](../../../crates/mir/src/unit/ownership/places.rs)、[body](../../../crates/mir/src/unit/ownership/bodies.rs)、[core signature](../../../crates/mir/src/unit/ownership/core/mod.rs)、[必須region／failure proof](../../../crates/mir/src/unit/ownership/failure.rs)、[source analysis pipeline](../../../crates/semantics/src/analysis.rs)。`9d9d2ac`時点で2026-10-18 JST検証。数値wire割当は本project決定に由来し、外部標準ではありません。
- 現実装：[schema-2 codec](../../../crates/bytecode/src/ownership.rs)、[compile／load dispatch](../../../src/artifact.rs)、[CLI](../../../src/main.rs)、[検証済みentryの実行](../../../crates/vm/src/unit/owned.rs)、[CLI test](../../../tests/artifact_cli.rs)、[差分実行](../../../tests/support/artifact_execution.rs)。2026-10-18 JST更新・検証。
