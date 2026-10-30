<!-- translation-of: ../../rfcs/0006-source-frontend-and-portable-programs.md; canonical-language: en; reviewed: 2026-10-20 -->

# RFC 0006：Source frontendとportable VM program

[English](../../rfcs/0006-source-frontend-and-portable-programs.md)

- ステータス: implemented
- Authors: Codex（repository ownerのreview向け）
- 作成日: 2026-10-17
- 最終更新日: 2026-10-30
- Tracking issue: `docs/roadmap.md` のMilestone 1
- Supersedes: なし。RFC 0002を拡張
- Superseded by: なし

## 現在の範囲

disk入力の強制を明確化（2026-10-30 JST）: root source・import module・editorのdisk snapshotは通常fileだけを受理します。Unixではnonblocking open後のhandleを検査し、byte上限を適用する前にFIFO/device入力がwriterを待つことを防ぎます。root内の通常fileへの認可済みsymlinkは引き続き利用でき、overlayはhostから受け取る不変bytesです。I/O拒否を追加するもので、source構文やpath権限は変えません。

2026-10-28 JST照合。M1のfrontend／portable Copy profileとVM／nativeへの全loweringは実装済みです。[M2](../milestone-2-capabilities.md)がnative call／aggregateを完成させ、[M3](../milestone-3-capabilities.md)が別途規定したtask／component profileを追加します。RFC 0007、0008、0022は字句／core／foreign／task機能を拡張します。本書の初期字句一覧はM1基準であり、現在のkeyword全体ではありません。以下の過去のscalar-native制約や受理checkpointで完了済みgateを再開しません。

## 概要

Milestone 1の契約を定めます。字句規則、module／call、copy専用record／variantと網羅的pattern、portable register-bytecode encoding、上限付きVM frame、transactional REPL sessionです。RFC 0002のwrapping addition、signed comparison、statementの順序、明示的returnは維持します。Repository ownerは2026-10-17にこのRFCを受理し、3つのsource設計gateとbytecode／REPLの前提を満たしました。実装とMilestone 1 exit auditは2026-10-17に完了しました。

この実装済みproject設計契約は、以下のexampleと互換性fixtureを規定します。実装はstabilizationや他言語に関する主張ではありません。

## 動機

実装前の課題は次のとおりでした。受理済みgrammarには `IDENT`、comment、encoding、signed-literal境界の未規定部分があります。Call、module、aggregateは意図的に対象外です。現在のbytecode `Program` は1 functionを表し、portable encodingはありません。VMにはguest call stackがありません。Lexer、CLI、backend内で個別にこれらを決めると意味が不整合になります。1つの意味論契約によりfrontend、VM、formatter、editor prototypeで同じevidenceを共有できます。

## 目標

- 小さな複数function／複数fileのscriptを有用な診断付きで実行する。
- Record、variant、網羅的patternをtype-check／実行する。
- Typed HIR、MIR、bytecode、failureまでsource snapshotとspanを保持する。
- 不正なsource／artifactを拒否し、side effect前にcompiler／VMの処理量を制限する。
- Rust layoutとnative calling conventionから独立した厳密なversion付きartifactを維持する。

## 非目標

Generic、method、closure、string、float、暗黙変換、再帰data type、参照、source内resource生成、destructor、例外、async、package／registry I/O、stable native ABI、production sandboxの保証は対象外です。新しいcall／aggregate操作のnative loweringはMilestone 2が担当し、実装まではstructured unsupported-feature errorを返します。既存RFC 0002のnative differential conformanceは引き続き必要です。

## 利用者向け設計

```text
mod math;
use root::math::increment;

struct Pair { left: i64, right: i64 }
enum Answer { Missing, Found(Pair) }

fn total(answer: Answer) -> i64 {
    match answer {
        Answer::Missing => { return 0; },
        Answer::Found(Pair { left: a, right: b }) => {
            return a + b;
        },
    }
}

fn main() -> i64 {
    return total(Answer::Found(Pair { left: increment(20), right: 21 }));
}
```

`math.lang` に `pub fn increment(value: i64) -> i64 { return value + 1; }` を記述します。Root fileを実行すると `42` を表示します。Function signatureとlocal typeは明示します。`let mut name: Type = expression;` はこのRFCのすべての型で使用でき、parameterとpattern bindingはimmutableです。Assignmentはlocal value全体を置き換えます。Field assignment構文はありません。

## 参照意味論

### 字句契約

- InputはBOMなしUTF-8です。Unicodeも改行も正規化しません。不正UTF-8は該当するraw byte範囲を持つinput error `SRC0010` とし、架空のvalid-source spanへ変換しません。Comment内を含めNULは不正です。Valid sourceはimmutableな `lang-diagnostics` snapshotへ保存します。
- Identifierは `[A-Za-z_][A-Za-z0-9_]*` です。ただし単独の `_` はwildcard patternに予約します。非ASCII textはcomment内のみ許可し、identifierには許可しません。Keywordはcase-sensitiveな完全一致の `fn let mut i64 bool if else while return mod use pub root extern struct enum match true false` です。他のwordはidentifierで、暗黙の将来予約語はありません。
- 空白はspace、tab、LF、CRです。Compilerのline counterを進めるのはLFだけで、CRLFは1回進みます。単独CRは同じcompiler診断行内の空白です。Byte spanは半開区間、human columnはUnicode scalar数、IDE columnはUTF-16 code unit数です。LSP adapterは単独CRを含むprotocolの改行規則でbyte spanを独立変換するため、editorの行番号はcompilerの行番号のcopyではなくLSPに従います。
- `//` commentはLFまたはEOFの直前で終わります。`/* ... */` commentは最大depth 256までnestできます。両方ともtriviaです。閉じていないblock commentは開始delimiterからEOFまでのspanで `SRC0009` を報告します。Depth超過は `SRC0011` とし、panicせずanalysisを終了します。
- Punctuationは `(` `)` `{` `}` `:` `;` `,` `.` `=` `+` `<` `->` `::` `=>` です。一致する2文字tokenを優先します。Minusの直後にASCII digitがある場合はsigned decimal tokenの一部です。単独の減算／unary negation operatorはありません。
- Decimal tokenは `-?[0-9]+` です。先頭のzeroは許可しますがoctalの意味はありません。`-0` はzeroです。直後がidentifier characterの場合、digit／identifierの連続全体を1つの不正token（`SRC0008`）にします。Separatorや他の整数基数は対象外です。Range checkはsigned token全体へ `SRC0001` を報告し、wrapしません。
- 他のUnicode scalarは、上記のmalformed-number規則を除き、それぞれ1つのinvalid token（`SRC0008`）です。`- 1` はminusを報告します。`a-1` はidentifierとsigned decimalへtokenizeされ、syntax checkingで失敗します。Comment、空白、invalid token、missing-token recovery nodeをlossless treeへ保持します。実在tokenのtextを連結すると、元のvalid UTF-8 byte列を厳密に再現する必要があります。

### Grammar拡張

RFC 0002のgrammarを維持し、以下を追加します（`?`／`*` はsource tokenではなくgrammar表記です）。

```text
file        := item*
item        := "pub"? (function | record | variant | module) | import | host
function    := "fn" IDENT "(" (parameter ("," parameter)* ","?)? ")" "->" type block
parameter   := IDENT ":" type
module      := "mod" IDENT ";"
import      := "use" path ";"
host        := "extern" "fn" IDENT "(" IDENT ":" "i64" ")" "->" "i64" ";"
record      := "struct" IDENT "{" (IDENT ":" type ("," IDENT ":" type)* ","?)? "}"
variant     := "enum" IDENT "{" case ("," case)* ","? "}"
case        := IDENT ("(" type ("," type)* ","? ")")?
path        := ("root" "::")? IDENT ("::" IDENT)*
type        := "i64" | "bool" | path
primary     := SIGNED_DECIMAL | "true" | "false" | path
             | "(" expression ")" | call | construction | field
call        := path "(" (expression ("," expression)* ","?)? ")"
construction:= path "{" (IDENT ":" expression ("," IDENT ":" expression)* ","?)? "}"
field       := primary "." IDENT
statement  += "match" expression "{" (pattern "=>" block ",")* "}"
pattern     := "_" | IDENT | SIGNED_DECIMAL | "true" | "false"
             | path "{" (IDENT ":" pattern ("," IDENT ":" pattern)* ","?)? "}"
             | path ("(" pattern ("," pattern)* ","? ")")?
```

Local declarationのRFC 0002における `"i64"` annotationを `type` へ置き換えます。この表記の `field` は左結合postfix chainを意味し、parserの左再帰を要求しません。Call先は名前付きfunctionまたはenum caseのみで、任意valueではありません。Field accessはadditionより強く結合します。裸のenum-case pathはpayloadなしcaseの生成／matchであり、payloadなしcaseの括弧は拒否します。Patternの裸の単一identifierは常にbindingです。Payload caseには括弧が必要です。Record literal／patternは全fieldを任意の記述順でそれぞれ1回列挙します。Shorthand、spread、guard、range、or-patternはありません。最後を含め全match armに末尾commaが必要です。既存parameter listにはoptional trailing commaを追加します。空enumは拒否し、空recordはinhabitedなcopy valueです。

Record literalとcondition blockの曖昧さを避けるため、`if`／`while`／`match` のhead expressionでは括弧のないrecord constructionを禁止します。括弧内なら使用できます。通常のname、call、field chain、comparisonは制限しません。`match` はstatementでありvalue expressionではありません。到達可能なfunction exitには引き続き `return` が必要です。

### Module、name、visibility

Callerはroot source fileと明示的なsource-root directoryを渡します。Loaderが読めるのはcanonical source root以下のfileだけです。Rootの `mod math;` は `math.lang`、そのmoduleの `mod inner;` は `math/inner.lang` をloadします。Pathはmodule階層に従い、processのworking directoryには依存しません。Analysis前にmissing file、root外のcanonical path、重複canonical file、module cycleを拒否し、symlink cycleを再帰的に辿りません。File readはhostが提供するcompiler inputであり、scriptへのruntime capability付与ではありません。

各moduleはbody解決前に全item signatureを収集するため、前方function参照と相互再帰を許可します。全item kindは1つのmodule namespaceを共有し、重複item／import aliasはerrorです。Import aliasはpathの最後のsegmentを使用し、rename／re-exportできません。Import-alias cycleはerrorです。Unqualified pathは現在のmodule内、`root::` pathはrootから解決します。Local valueをmodule valueより優先しますが、local valueをcallできません。Private itemは宣言module内のみ可視です。別moduleへのtraversalには、跨ぐ各child／item boundaryの `pub` が必要ですが、現在のmoduleは自身のprivate child moduleへアクセスできます。Public record／enumは全field／caseを公開します。Private typeを含むpublic signatureは拒否します。

RFC 0002のlexical local scope、宣言前使用の禁止、function全体でのlocal／parameter名再利用禁止を維持します。別arm間を含めpattern bindingにも同じ規則を適用し、bindingはarm block内だけに存在します。Module itemはこのlocal-name禁止の対象外です。HIRはcompilation内でstableなmodule／item／local／type IDを割り当て、表示名とsource spanを保持します。これらのIDはpersistされたcompiler APIではありません。

Root script entryはparameterなしの厳密な `fn main() -> i64` です。Libraryはentryなしでanalysisできますが、実行時はmissing-entry診断です。別moduleの `main` は通常のcodeです。Top-level statementや初期化side effectは許可しません。

### Type、aggregate、pattern、評価

`bool` をsource typeへ追加し、`true`／`false` を導入します。Conditionは `bool` を要求し、addition／comparisonはRFC 0002の `i64` 規則を維持します。Call、assignment、return、field、payloadはnominal typeの厳密一致を要求し、coercion、overload、宣言型推論はありません。新しい一般型不一致は `SRC0014`、既存RFC 0002の文脈は `SRC0005`／`SRC0006` を維持します。Return pathは直接returnする場合、両if branchがreturnする場合、網羅的matchの全armがreturnする場合に完結します。Loopだけではreturnを証明しません。

Record／enumは定義moduleとnameで識別するnominal typeです。Field／payloadには `i64`、`bool`、他のrecord／enumのみ格納できます。Enum payload経由を含む全再帰type cycleを拒否します。Enum valueは宣言済みcaseをちょうど1つと、その位置付きpayloadを持ちます。すべてcopy valueであり、assign、引数渡し、match、returnで他のvalueをmove／destroyしません。Resource identity、borrow、alias、destructor、言語から見えるheap allocationはありません。実装は内部storageを上限付きでallocateできますが、後述のaccountingが必要です。既存のnominal `own<T>` MIR valueをこれらのsource aggregateへ含めることはできません。

Call argumentは左から右へ評価してからcalleeへ入ります。Record fieldは記述順に評価し、宣言順へ格納します。Enum payloadは左から右へ評価します。Field accessはreceiverを1回評価します。Matchはscrutineeを1回評価し、armをsource順にside effectなしで試し、最初に一致したarmのbindingをcopyして実行します。Literalはsigned integer／booleanの完全一致、wildcard／bindingは期待型の全valueへ一致します。Record patternはfieldの積、case patternは該当caseとpayload patternへ一致します。到達不能armを含め全bodyをtype-checkします。

Exhaustivenessはarm patternの和集合がscrutinee typeの全inhabited valueをcoverすることであり、満たさなければmissing witness付き `SRC0016` です。先行armに覆われるarmはwarning `SRC0017` です。Integer domainを記述されたliteral値と補集合の代表1値へ分割します。Booleanは2 constructor、recordは1 product constructor、enumは宣言caseを使用します。Field columnへconstructor specializationを再帰適用します。これにより全 `i64` を列挙せずnested／product coverageを定義します。各specializationを設定可能なwork budgetへ課金し、超過時は `SRC0011` とし、誤って網羅済みとしません。Validな網羅的sourceからruntime match-failure pathを生成しません。

### Host callとguest frame

`extern fn next(value: i64) -> i64;` は明示的に提供されるhost importを宣言します。Hostはqualified declaration nameを既存の型付き `HostFunctionRegistry` registrationへbindします。全宣言importを厳密な1引数 `i64` signatureでbindしてからguest codeを実行します。宣言だけではfilesystem、environment、process、network、その他の権限を与えません。CLI／REPLのdefault registryは空です。Native callbackはtrustedかつnon-preemptibleのままで、この構文やRFCで `extern "C"` を跨ぐpanic transportを追加しません。

Guest callはRustの再帰callではなく、VMが管理するframe vectorを使用します。各frameはcallee、register、現在のblock／instruction、return destination、call-site spanを記録します。Callee開始前にargumentをcopyします。Returnはresultをcallerへcopyしてframeを解放します。Entryのdepthは1です。Frame／register allocationやcallee／hostのeffect前に、interruption／deadline、frame-depth limit、total live value-cell limit、該当fuel／host-call budgetを確認します。既存のinterruption優先規則を維持します。Guest call instructionは1 fuel unitで、callee instructionとreturnは通常のcostです。隠れた追加call／return fuelは課金しません。Tail-call eliminationでcounterやstack traceを変えてはいけません。

Metering順序はprototypeを維持します。Controlと利用可能instruction fuelを確認してからinstructionを1回課金します。Guest callは続いてcontrolを再確認し、予定frame depth、その後live cellを確認し、失敗時はallocationもcallee entryも行いませんがinstruction fuelは課金済みのままです。Host callはcontrolとhost-call budgetを再確認し、callbackへ入る直前に1 host callを課金します。拒否されたcallbackではinstruction fuelを消費しますがhost callは消費しません。初期entry frame／cell拒否のfuel消費はzeroで、一度も入っていないcalleeはstack traceへ含めません。例えばframe limit 1でmainがconstant `1` の後に `leaf(1)` をcallすると、call siteでfuel消費2、host call 0として失敗します。

Scalar／owned-resource cellは各1、recordは1にfieldの再帰countを加算し、enum registerは1に最大case payload countを加算して予約します。Allocation前にoverflow-safeなcount確認を行います。Register copyはこれらの静的最大chargeを超えられません。Frame limitはentryを含み、直接／相互再帰に適用します。Failureはstable runtime reason、消費counter、fault span、最内frameから順のguest stackとcallerのcall-site spanを返します。Failureでcopy専用guest frameをすべて解放しますが、既存host side effectは取り消しません。

安全なscript／REPL defaultはfuel 1,000,000 unit、host call 1,000回、live frame 256、live value cell 1,000,000です。明示的なembedding APIは別limitや既存のunbounded convenience behaviorを選べます。Execution errorはsource valueではありません。Host callbackのbyte-based accounting、production heap sandbox、native interruption、async／task limitは保証しません。

### Typed MIRとbytecode契約

Nominal type definition、名前付きhost import、function、optional entry IDを含むbackend非依存compilation unitを導入します。既存の単一function APIはunitのwrapperにできますが、古いin-memory constructorはwire formatではありません。Shared typed contractにboolean constant、guest call、record construction／field projection、enum construction／case test／checked payload projection、literal pattern向け内部integer equalityを追加します。Assignment join／loop back-edgeはtyped block argumentを維持します。Copy専用aggregateをblock argument／callでtyped valueとして渡します。Frontend ASTやbackend typeをMIRへ混入させません。

Validationは全ID／signatureを解決し、重複定義を拒否し、非循環で上限付きのtype layout、SSA definition／dominance、reachability、call arity／type、block-argument type、field／case範囲、return type、source-map shape／rangeを確認します。既存owned-resource linearity制約を維持し、新しいcopy操作はowned-resource inputを拒否し、guest callはowned resourceを含むsignatureを拒否します。これにより別のownership契約を導入せず、既存straight-line owner fixtureを維持します。Checked enum-payload projectionはruntime caseが異なると `InvalidVariantProjection` でtrapします。正しく型付けされたsource loweringは選択されたcase path内にだけ生成します。手動構築した検証済みunitではこの定義済みtrapが発生し得ます。

### Portable bytecode schema 1

新しい非互換artifact familyです。8 byteのmagic `LANGBC\0\0` の後にlittle-endian `u16` schema `1`、`u16` flags `0`、`u32` payload lengthが続きます。未知version／flag、切断input、length不一致、末尾byteを拒否します。以下のcount、length、index、offsetはすべてlittle-endian `u32`、signed constantはlittle-endian two's-complement `i64`、tag／booleanは `u8`（booleanは0／1のみ）です。Vectorはcountとelement列、stringはbyte lengthと厳密なUTF-8 byte列で、NUL terminatorなしです。Identifier／name stringは空文字と埋込NULを拒否します。Source textも別途length-prefixを持ち、改行は許可しますがNULは許可しません。Rust discriminant、pointer、native padding、varint、compression、host registry addressをencodingに使用しません。

Payload fieldは以下の順です。

1. Source vector：正規化した相対display path string、その後にsource-text string。Pathは `/` を使い、空／`.`／`..` segmentを含まず、file-read権限を与えません。Text byteを保持し、decode時にfileを再読込しません。
2. Nominal definition vector：qualified name string、kind `0` recordまたは `1` enum。Record bodyは `(field-name string, type)` vector、enum bodyは `(case-name string, payload-type vector)` vectorです。各scope内でnameは一意、enumは1 case以上です。
3. Host-import vector：qualified name string、parameter-type vector、result type。Schema 1はここで `(i64) -> i64` だけを許可します。参照はimport indexを使用し、instantiation時にhostが再bindします。
4. Function vector：qualified name string、parameter-type vector、result type、register-type vector、optional function span、block vector。Entry argumentはsignature順で先頭registerに置きます。各blockはparameter-register vector、instruction vector、1 terminatorを持ちます。Block zeroがentryで、block parameterもpredecessorも持ちません。
5. Optional entry：なしはmarker `0`、ありは `1` とfunction indexです。他のmarkerは不正です。

Typeはtag `0` = `i64`、`1` = `bool`、`2` とnominal resource ID = `own<T>`、`3` とdefinition index = record、`4` とdefinition index = enumです。参照definitionは対応kindと一致する必要があります。Live resource valueはserializeしません。Spanはなしがmarker `0`、ありが `1` とsource index／start／end byte offsetで、UTF-8境界を検証します。Source生成function／instruction／terminatorは常にspanを持ち、手動fixtureでは省略できます。

Instructionはopcode、以下のoperand、optional spanの順です。Register／function／import／definition／case／field operandはすべて `u32` です。列挙したoperandはすべて存在し、暗黙destinationや推論typeをserializeしません。

| Opcode | 意味 | Operandの順序 |
| --- | --- | --- |
| 0 | i64 constant | destination, i64 value |
| 1 | wrapping add | destination, left, right |
| 2 | signed less-than | destination, left, right |
| 3 | host call | destination, import, argument |
| 4 | bool constant | destination, u8 value |
| 5 | guest call | destination, function, argument-register vector |
| 6 | record construct | destination, definition, field-register vector in declaration order |
| 7 | record field | destination, record register, field index |
| 8 | enum construct | destination, definition, case index, payload-register vector |
| 9 | enum case test | destination, enum register, case index |
| 10 | checked enum field | destination, enum register, case index, payload index |
| 11 | internal i64 equality | destination, left, right |

Terminatorはtag `0` とreturn register、`1` とblock target、または `2` とcondition register／then-target／else-targetの後にoptional spanです。Block targetはblock indexとargument-register vectorです。未知tag／opcodeはerrorです。Canonical encodingではfunction／type／importをqualified ASCII name順、sourceを正規化path順に並べ、indexを対応してremapします。Field／case／block／instruction順は維持します。重複name／pathは拒否します。Decodeは構造limitとtyped unit全体を検証してから実行可能artifactを返します。Encode／decode／re-encodeで同じcanonical byte列になる必要があり、非canonical順序はindexの意味を黙って変更せず拒否します。

### Compilerとdecoderの上限

Default compile limitはsourceごと4 MiB、合計source 16 MiB、module／file 256、token 1,000,000、syntax node 2,000,000、syntax／comment depth 256、function／nominal definition／importそれぞれ4,096、parameter／field／case／payload fieldそれぞれ256、type depth 64、valueごと4,096 cell、unit合計register／block／instructionそれぞれ1,000,000です。Exhaustiveness／verifier work budgetはそれぞれ10,000,000 charged stepです。Work stepは、訪問したpattern-specialization state、またはverifier instruction／operand／CFG edge／dataflow-set memberで、再訪も毎回課金します。実装は追加課金する操作を文書化し、小budgetでtestします。構造上限で繰返しdataflow処理の課金を代替しません。

Decoderはさらにartifact全体を64 MiBへ制限します。Reserveや参照追跡の前にbyte length、残input最小量、count、checked arithmetic、depth、累積allocation、work budgetを確認します。個別countの上限だけでは不十分で、累積element数とallocation budget（defaultでdecoder／verifier bufferの要求量128 MiB、caller所有inputを除外）を課金します。過大requestはallocation前に拒否します。Limitは明示設定できます。超過はerrorであり、malformed-source recoveryや部分成功artifactにはしません。Frontend APIもCLIと同じlimit設定を受け取り、library callerが偶然checkを迂回しないようにします。

## 実行モード

VMは受理されたMilestone 1操作をすべて実装します。JIT／AOTはscalar RFC 0002 source differential coverageを維持し、未supportのcall／aggregateをcode publication前に拒否します。Milestone 2で完全なloweringと等価性を追加します。この段階的supportは明示制約であり、異なるarithmetic／pattern behaviorを許すものではありません。Host importはtrusted typed callback boundaryを維持します。WebAssembly mappingやcomponent ABI変更はありません。Constant evaluationも引き続き対象外です。

## ABIと互換性

既存RFC 0002 programの意味は維持します。`bool` storage、call、module、record／enum宣言、match、予約語はpre-release source profile内の追加です。移行対象の実装済みsource lexerはありません。実装後の意味変更にはRFC 0001の互換性processが必要です。Nominal definitionはcompilation-unit identityであり、stable native memory layoutではありません。Bytecode schema 1は他versionを拒否し、将来の非互換encodingは新schemaと明示的な拒否／migration fixtureを使用します。Component schema番号を流用せず、serialized MIRの安定性を約束しません。

## Diagnosticsとtooling

受理済み文脈で `SRC0001`–`SRC0007` を維持します。新codeは `SRC0008` invalid token、`SRC0009` unterminated comment、`SRC0010` invalid source encoding／BOM、`SRC0011` resource limit、`SRC0012` expected syntax、`SRC0013` module／import／visibility error、`SRC0014` extended type mismatch、`SRC0015` invalid aggregate／pattern shape、`SRC0016` non-exhaustive match、`SRC0017` unreachable arm、`SRC0018` missing／invalid script entry、`SRC0019` unreachable statementです。Runtime／artifact errorはsource codeとは独立したstructured enumです。Valid sourceの全diagnosticにprimary spanを付け、元の宣言／signature／競合armをrelated labelへ保持します。曖昧さのないmissing-delimiter fixのみmachine-applicableとし、推測したexpression／branchはspeculativeです。

Recoveryはnested delimiterを尊重し、semicolon、現在のblockのclosing brace、次のitem boundaryで同期して必ず前進します。Nested expressionの修復で外側のclosing braceを消費しません。不完全fileと後続の回復済みitemをtoken／textとsyntax snapshotで検証します。Formatterは実在commentと不正textを保持し、4-space indent、binary operatorの両側のspace、1行1 declaration／statementを使用します。Idempotentでsemantic parse構造を保持し、不正tokenを破棄してinvalid fileをformatしません。Incremental databaseはimmutable source snapshotと依存revisionをparse／analysis結果のkeyにし、editor／LSP prototypeと共有し、edit時にreverse module dependencyをinvalidateします。Document closeで公開diagnosticをclearし、古いsnapshotの結果で新revisionを上書きしません。Completionやproduction LSP serverはprototypeの必須項目ではありません。

## Performanceとcost model

Source resolutionはmodule名をindexし、type layoutは非循環かつ上限付きです。Aggregate copyは上限付きvalue sizeに比例し、field access／enum case testはdestructorを実行しません。Bytecodeはregister-basedで実行前に検証します。これらは設計上のcost boundであり、計測済みthroughput保証ではありません。性能を主張する前にlexing、parsing、resolution／type checking、lowering、decode／verification、frame call、aggregate copyを個別計測します。この提案は新benchmark結果を主張しません。

## Securityとtrust boundary

Untrusted inputには明示的なcompile／decode／verification budgetとmalformed-input／fuzz testを適用します。Source pathやdebug nameはfilesystem accessを許可せず、明示的なroot配下だけをsource loadします。Host importはeffect前の明示的typed bindingを要求します。VMは上限付きexplicit frame／value storageを使用します。Trusted callbackは独立してblock、allocate、abortでき、ここでのlimitはそれらをsandbox guest codeへ変えません。新しい `unsafe`、native plugin load、network権限、process access、component-resource ownership規則は導入しません。

## 代替案

- **変更しない：** 直接MIR構築を続けますが、既存のsource-script exit条件を満たせません。
- **Library／tooling layer：** 外部言語やmacro library経由でsourceを変換するとname／type／pattern意味論が重複し、共有frontend契約を失います。
- **より豊富な競合設計：** Unicode正規化identifier、型推論付きimmutable binding、expression match、generic、再帰boxed ADT、任意host signatureは表現力を高め得ますが、未解決のownership／正規化／error／ABI決定が必要です。暗黙の実装判断にせずfollow-upとします。
- **より単純な設計：** Aggregate／guest callなしの単一file `i64` functionは有用な中間sliceですが、現在のMilestone 1 scopeを完了しません。Record／variant／module／call taskやexit evidenceの代替にできません。

## 実装計画

1. Repository ownerの明示受理を記録します。2026-10-17に完了しました。新しいsource／wire behaviorをこの受理済み契約に沿って実装します。
2. Diagnostic crateとinput limitを使いlossless tokenization／parser recoveryを実装し、字句／syntax snapshotを追加します。
3. Module loading／resolved HIR、primitive typing、nominal layout、pattern checking、exhaustivenessを独立してtest可能なsliceで追加します。
4. Typed HIRを検証済みcompilation-unit MIRへloweringします。合成joinとcall siteのspanを保持し、scalar VM／JIT source differential testを維持します。
5. Bytecode／VMをtyped call／aggregate、explicit frame、cell accounting、failure、stack testで拡張します。Schema-1 encoding／decodingとbyte fixtureは受理済み契約に対してのみ追加します。
6. `lang run <root-file>` と `lang repl` を公開します。Script成功時はsigned decimal resultと改行を1つ表示します。Exit 0は実行成功、1はcompile／runtime failure、2はCLI usage不正です。Diagnosticはstderrへ出し、script returnをprocess exit codeにはしません。
7. REPLはsubmissionごとに完全なitemまたはexpressionを1つ受け付けます（brace／parenthesisの対応で継続入力できます）。Itemはanalysis全体の成功後にのみcommitし、errorで以前の定義を変えません。Expressionはcommit済み定義を参照する合成functionでfreshなexecution budgetを使って実行し、canonical valueを表示します。Trusted host side effectはrollbackしません。Top-level mutable stateはありません。重複itemは `:reset` までerrorです。`:reset` は定義とdiagnosticをclearし、`:quit` またはEOFで終了し、不完全EOFはsubmissionを診断します。Hostが明示source rootを渡さなければfile moduleは利用できません。非interactive inputではpromptを表示しません。Canonical aggregate表示はqualified type／case名、宣言順record field、comma-space separator、再帰的なscalar表示を使用し、booleanは `true`／`false` です。
8. Formatter、incremental dependency invalidation、editor／LSP diagnostic adapterを追加します。RoadmapのP1 taskを黙って延期せず完了します。Milestone 1の最終guideは必要実装とexit evidenceがすべて成功した後に作成します。

## Test計画

以下の互換性要件は実装済みであり、今後も回帰検証の対象です。この表は必要な挙動を示し、後続のevidence表が実行可能なsuiteを対応付けます。

| 領域 | Input／action | 必須結果 |
| --- | --- | --- |
| 字句 | `return -9223372036854775808;` | 範囲内negative literal 1つ |
| 字句 | `9223372036854775808` / `-9223372036854775809` | `SRC0001`、token全体のspan |
| 字句 | `a+-1`, `a-1`, `- 1`, `12abc` | 順にnegative literalの加算、syntax error、invalid minus、単一malformed number |
| 字句 | Nested `/* α /* b */ c */`、CRLF、EOF comment | Lossless byteと後続位置の正確性 |
| 字句 | BOM、不正UTF-8、NUL、Unicode identifier | 正確なencoding／token error、panicなし |
| Recovery | 後続code前の `;`、`)`、`}` 欠落 | 上限付きrecovery、tokenと後続diagnosticの保持 |
| Resolution | 前方call、相互再帰、private import、symlink escape | 前2つは解決、後2つは実行前拒否 |
| Scalar | 極値を含むRFC 0002 add／min／countdown／conditional sum | 正しい結果とscalar VM／JIT differential一致 |
| Record | Host callを伴う記述field順変更、重複／欠落field | 記述順評価、shape error |
| Pattern | 全enum case、nested boolean product、wildcardなしinteger literal | 網羅、全product時のみ網羅、missing integer witness |
| Call | 直接／相互再帰、arity／type不一致、厳密なframe／cell／fuel境界 | 正しい結果またはguard対象effect前のstructured error |
| Metering | Frame limit 1でconstant後guest call、host budget 0でconstant後host call | 両方fuel 2、callee frame／callbackなし、host count 0 |
| Stack | 3 guest frame後のhost／VM failure | 最内からのfault／call-site span、消費count維持 |
| Bytecode | Golden byte、re-encode、未知tag／version、切断／count bomb | Stable schema byte、allocation急増なしの上限付き拒否 |
| REPL | Valid定義、invalid定義、後続call、reset、不完全EOF | Transactional定義、fresh budget、deterministic表示 |
| Tooling | 2回format、依存edit、source close、古いdiagnostic完了 | Idempotence、対象限定invalidation、clear／最新diagnostic |

| 検証範囲 | 実行可能なevidence（repository相対path） |
| --- | --- |
| 字句境界とrecovery | `crates/syntax/tests/lexing.rs`、`parsing.rs`、`fixtures/accepted.lang` |
| 権限、resolution、typed aggregate／pattern | `crates/semantics/tests/modules.rs`、`binding.rs`、`program_typing.rs`、`database.rs`、`crates/hir/tests/nominal_layouts.rs` |
| Source loweringとscalar nativeの一致 | `crates/semantics/tests/program_lowering.rs`、`tests/source_conformance.rs`、`hir_lowering.rs` |
| Call、metering、stack、aggregate実行 | `tests/source_vm.rs`、`crates/vm/tests/units.rs` |
| MIRとportable wire検証 | `crates/mir/tests/units.rs`、`crates/bytecode/tests/unit_codec.rs`、`fixtures/unit-v1-main-42.hex` |
| CLIとtransactional REPL | `tests/cli.rs`、`repl.rs`、`crates/semantics/tests/virtual_programs.rs` |
| Formattingとeditor lifecycle | `crates/syntax/tests/formatting.rs`、`tests/formatter.rs`、`lsp.rs`、`crates/semantics/tests/database.rs`、`crates/query/tests/hir_cache.rs` |

Parser／HIR／bytecodeのfuzz target、明示depth／count／work／allocation-limit test、diagnostic compile-pass／fail fixture、構造snapshot、macOS／Windows workspace CIがあります。Parser fixtureとgolden bytecodeは再現可能なseedです。Fuzz binaryのcompile／smoke実行はinstrumented fuzz campaignではありません。Native testは対応するscalar source subsetを扱い、非対応の新native操作を明示的に拒否します。現在の検証詳細とplatform上の制約は[repository review](../review-2026-10-17.md)に記録し、以下の当初完了evidenceは過去の記録として維持します。

## 未決事項

Decision：repository ownerは「RFC 0006 を受理します」と明示し、**2026-10-17に受理**しました。ASCII identifier、copy専用の非循環aggregate、statement match、明示VM limit、schema 1、transactional REPL stateを含む完全なMilestone 1 source／execution profileを受理したものです。このprofileにblocking design decisionは残っていません。

任意の将来検討はUnicode識別子、通常のimmutable binding、import alias、通常の戻り値型推論、式match、利用者定義generics／再帰value型です。有限ownership／coreと拡張foreign profileはRFC 0007／0008で追加済みです。本RFCのCopy profile全体のnative対応は[M2監査](../milestone-2-capabilities.md)で完了しており、未解決の前提ではありません。将来拡張は明示的な契約を必要とします。

受理前のreview outcomeは **ready for decision** でした。現在は **implemented** で、2026-10-17に497-test fmt／Clippy／test／doc baselineとMilestone 1完了監査で検証しました。Evidenceは `b5b83b7`（portable codec）、`cb3312d`（source lowering）、`b1964e7`（scalar native conformance）、`d041598`（上限付きVM）、`bbc1f3d`（transactional REPL）、`2d3a3ff`（formatter）、`d419df9`（query database）、`402a5dc`（stdio LSP）です。Stabilizationは主張しません。

## 情報源

- [Milestone 1完了監査](../roadmap-history.md#完了監査2026-10-17)、実装evidence。最終更新／確認2026-10-17 JST。
- `docs/roadmap.md`、scopeと依存gate。最終更新2026-10-17 JST、commit `f596e2c`。
- `docs/rfcs/0001-project-charter.md`、受理済みgovernance、backend制約、互換性。最終変更2026-10-17 JST、確認2026-10-17 JST。
- `docs/rfcs/0002-prototype-control-flow.md`、受理済みscalar／CFG grammarとcode。最終変更2026-10-17 JST、確認2026-10-17 JST。
- `docs/architecture.md` と `crates/diagnostics/src/lib.rs`、source-snapshot／diagnostic実装。最終更新2026-10-17 JST、commit `a4f135c`。
- `crates/mir/src/lib.rs` と `crates/bytecode/src/lib.rs`、現行typed prototype。最終更新2026-10-17 03:31 JST、commit `98bbb08`。確認2026-10-17 JST。
- `crates/vm/src/lib.rs`、現行typed VM。最終更新2026-10-16 23:23 JST、commit `b2ccdaf`。確認2026-10-17 JST。
- `crates/runtime/src/lib.rs`、typed host registry。最終更新2026-10-16 15:05 JST、commit `a3e07f6`。確認2026-10-17 JST。

- Microsoft LSP 3.18の[text-document位置／改行規則](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/textDocuments.md)。更新日非掲載のliving specification。確認2026-10-17 JST。

LSP位置mappingは外部protocol要件に従います。数値limit、grammar、wire tag、段階的backend supportは受理済みproject decisionであり、計測済みbest-practice保証ではありません。比較性能の主張は導入しません。
