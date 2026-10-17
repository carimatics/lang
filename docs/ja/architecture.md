<!-- translation-of: ../architecture.md; canonical-language: en -->

# アーキテクチャbaseline

[English](../architecture.md)

ステータス：RFC 0001–0005による**受理済み設計baseline**（2026-10-17）。実装は未完成のprototypeです。受理は安定化を意味しません。意味論はRFCに従い、以下の広範なプロダクト方針は実装前に追加RFCを必要とします。

## 目標

- WindowsとmacOSでのnative性能と低レベル制御。
- Tracing GCを必須とせず、既定でmemory-safeかつdata-race-safeであること。
- Application codeに適した、生産性の高いexpression-oriented language。
- 1つのsemantic frontendを使用したAOT executable／libraryとembeddable script。
- 同一process内での効率的なnative-to-script／script-to-native call。
- 1つのfrontend ABIに互換性を固定せず、将来複数source languageを追加できる経路。
- 優れたdiagnostics、build speed、debugging、IDE support、reproducibility。

## 初期実装の非目標

- すべてのCPU architectureまたはOSをsupportすること。
- Native language ABIをstableにすること。
- 任意のnative codeを安全にhot reloadすること。
- 透過的なdistributed execution。
- Core languageにtracing GCを含めること。
- 最初のfrontendとcomponent boundaryが安定する前に複数source languageを実装すること。
- Local／Git-based dependency workflowがreproducibleになる前にpackage registryを作ること。

## プロダクトモデル

明示的な2つのprogramming layerを持つ1つの言語から開始します。

- **Safe application layer:** Value-oriented API、null safety、algebraic data type、pattern matching、type inference、trait、closure、structured concurrency、必要な場合のmanaged shared reference。
- **Systems layer:** Allocator、raw pointer、layout control、C ABI、atomic、SIMD、inline assembly／intrinsic、platform APIを `unsafe` またはcapability-gated interfaceの背後に置く。

これにより、相互運用を1つのtype system内に保ちます。2つ目の言語は、安全性を弱めることなく同じcomponent modelとconformance suiteへloweringできる場合にのみ追加します。

## コンパイラパイプライン

```text
source text
    │
    ▼
lossless syntax tree ───────────── formatter / IDE recovery
    │
    ▼
resolved HIR ───────────────────── names / modules / desugaring
    │
    ▼
typed HIR ──────────────────────── inference / traits / effects
    │
    ▼
MIR ────────────────────────────── ownership / destruction / async lowering
    │
    ├──────────────► bytecode ───► verifier ─► embedded VM
    │
    ├──────────────► native SSA ─► Cranelift/LLVM ─► object/linker
    │
    └──────────────► interface IR / metadata ───────► bindings and plugins
```

型付きMIRをsemantic contractとします。Backend固有のoptimizationは異なっても構いませんが、観測可能な動作、destruction order、panic／error behavior、integer semantics、concurrency ruleは異なってはいけません。

受理済み[RFC 0002](rfcs/0002-prototype-control-flow.md)は `i64` parameter／result、wrapping addition、signed comparison、mutable local、`if`／`else`、`while`、明示的 `return`、左から右の評価順序を定義します。Join／back-edgeをまたぐvalueはtyped block argumentで運びます。受理済み[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)は字句規則、module／call、aggregate／pattern、portable bytecode、上限付きframe、script／REPL behaviorを追加します。Diagnostics、lossless parsing、source typing／lowering、portable artifact、上限付きVM実行、初期native scalar conformance、script、transactional REPLは実装済みです。Formatterとparser／module-query LSP統合は未完了です。

2026-10-17実装：`lang-syntax` はUTF-8 inputを検証し、不正encodingや先頭BOMにはsnapshotを作成しません。Input errorはraw byte範囲を保持します。Lexerは受け入れた各snapshotを、空白、comment、不正token、不正Unicode scalarも含む、連続した完全なtoken列として保持します。符号付きdecimalのoverflow、不正token、comment内も含むNUL、閉じていないblock comment、直接作成したsnapshotのBOM違反を報告します。Source byte、token、nested-comment depth、診断数の上限を超えると `SRC0011` でanalysisを終了します。Commentは反復処理で走査します。13件のlexer testが正確な範囲、復元、決定的な混合Unicode input、resource境界を検証します。

Parserはsyntax node、real-token参照、zero-widthなmissing-syntax leafをimmutableなarenaへ構築します。File／expression別のentry pointで受理済み文法全体を実装し、treeをsemantic HIRと分離します。Postfix field access、左結合の加算、連鎖しない比較、named call、record-headの曖昧性解消、pattern shapeをtreeへ明示します。Delimiterを考慮するrecoveryが外側の閉じbraceと後続宣言を保持します。`SRC0012` 診断は曖昧でないdelimiter挿入だけにmachine-applicable fixを付けます。Syntax node数2,000,000、tree／parser depth 256（内部再帰上限も兼ねる）、comma-list要素数256、診断数4,096、work 10,000,000を上限とします。Workはrecoveryを含むtoken indexing／consumption、node開始、missing-syntax挿入を課金します。Resource超過はerrorを返し、部分的な成功treeを返しません。復元、clone、dropは再帰的な所有nodeではなくarena参照を使います。18件のparser testがsyntax snapshot、全体文法fixtureとその全prefix、1,000件の決定的な不正input、正確なdepth／list境界、recovery、fix preview、source identityを検証します。`source_parser` fuzz targetはbyte ingestion、両parser entry point、復元、診断spanを検証します。Stable Rustのinstrumentationなしの10,000-run smokeは完了しましたが、coverage-guided／sanitizer evidenceを示すものではありません。Instrumented campaignはrepository rootで `cargo +nightly fuzz run source_parser` を別途実行します。Semantic resolution／typingとsource実行は後続の段階です。

2026-10-17実装：`lang-hir` はsyntax非依存のRFC 0002 function arenaを受け取り、local nameを解決し、analysisにerrorがない場合だけread-onlyなtyped HIRを返します。Parameterはimmutableで、宣言はinitializerの後からlexical block内だけで可視です。別branchを含むfunction全体での名前再利用を拒否します。Type checkingはinteger operand／value、boolean condition、到達可能な全exitの明示returnを強制します。`SRC0002`–`SRC0007` をprimary／related span付きで生成し、signed comparisonにも `i64` operand checkを適用します。到達不能statementもanalysisし、structured warningを返します。Primitive APIはstructured warning型を保持し、source program全体のcheckingは受理済み `SRC0019` を使います。全nodeにsource-snapshot byte span、localには表示名／宣言spanを保持します。Caller指定のwork／local／depth／diagnostic limit、内部再帰上限、arena参照検査、cycle検出、source／name-span検証でanalysis boundaryを保護します。この段階ではliteral decode／range診断、字句規則、module loading、source call、aggregate、serialized HIR契約を導入しません。

同じく2026-10-17実装：`lang-semantics::lowering` はこのHIRを既存typed scalar MIRへ変換し、明示work／value／block limitと最終MIR validationを適用します。Expressionを左から右へloweringし、変化したbranch valueをtyped block argumentでjoinし、可視mutable valueをloop-header parameterで運びます。Early returnで到達不能joinを作らず、到達不能statementからinstructionを生成しません。CFG構築後にsymbolic IDをMIRが要求する連続definition順へremapします。合成jumpを含む全生成instruction／terminatorで、snapshot byte spanを既存の1始まりMIR line／Unicode-scalar columnへ変換します。構造化inputのintegration testはVM／JIT arithmetic、両branch path、nested branch／loop state、early return、fuel超過、bytecode span保持、native JIT／AOT-object source mappingを扱います。Parser接続済みsource loweringと新call／aggregateは別gateです。Untrusted deployment前にはprototype MIR validator自体のwork／allocation hardeningも必要です。

### Source name resolution

2026-10-17実装：`lang-semantics::modules` はhost側のsource loadingとmodule namespaceを提供します。Hostがroot fileと明示的なsource-root directoryを指定し、読み込み前にcanonical pathがroot内にあることを確認します。Child pathは宣言階層（`math.lang`、次に `math/inner.lang`）に従い、processのworking directoryに依存しません。Readはfile別／全体byte budgetの残量と検出用1 byteまでに制限します。合計byte／token／node、予定module数、function／definition／import数、診断数、resolution workに上限があります。Host宣言と `use` aliasはimport数の上限を共有します。不正UTF-8はraw input範囲を保持し、有効なsource上の失敗はstructured spanを保持します。Source表示pathはUTF-8を必須とし、非可逆変換によるaliasを作りません。

Module graphはparsed tree、compilation内のmodule／item ID、表示名、宣言span、canonicalなimport targetを保持します。全宣言名を収集してからimportを解決します。Item種別は同じnamespaceを共有し、aliasは最後のsegmentを名前に使い、private importを再exportできません。Resolutionは横断する各module／item境界を検証し、module自身のprivate childへの参照を許可し、enum経由でcaseを公開します。同一canonical fileの重複とancestor cycleは再読み込み前に拒否します。Alias依存は明示stackを使い、再訪問も課金します。Workは宣言、path segment、enum-case候補、alias依存の訪問、ancestor検査を課金します。17件のlocal testでforward／階層path、alias、privacy、循環、名前重複、不正input、通算budget、Unix symlink境界を検証します。Linux専用testは、macOS filesystemが作成時に拒否する非UTF-8 filenameも検証します。Namespace graphの成功はbody typingの完了を意味しません。

2026-10-17実装：`lang-semantics::binding` はこのgraphと元のsnapshotをbackend非依存の `lang-hir::resolved` arenaへ変換します。Bodyより前に宣言IDを割り当て、forward／mutual guest callとnominal参照を可能にします。Primitive／nominalな型注釈、host call、enum case、record／field構文、全arm bodyを解決します。Local valueはmodule valueを隠し、宣言はinitializerの後からblock内でのみ可視です。Parameterとpattern bindingはimmutableで、別branch／armを含むfunction全体でlocal／parameter／binding名を一意とします。公開aggregateのfield／payloadを含むpublic signatureでprivate type宣言を拒否します。型使用箇所のspan、field／argumentの記述順、call site、block closer、enum-case構文の括弧有無をresolved IRへ保持します。これはtyping向けの名前解決済みinputであり、実行可能／serialized artifactではありません。

Bindingの既定上限はunit全体でlocal／expression／pattern／blockが各1,000,000、診断数4,096、depth 256（内部上限も兼ねる）、work 10,000,000です。訪問する宣言、型使用、block、statement、expression、pattern、arm、local宣言、namespace要求を課金します。Namespace解決は、実際に訪問したpath segmentとenum候補も同じbudgetへ課金します。課金済みtoken indexで先頭triviaをsemantic spanから除き、comment prefixの反復走査を避けます。CSTは元の全byteを保持します。Small-budget testはdeterministicな厳密境界とcall／triviaによる追加workを検証します。17件のtestでfixture変換、注釈／source span、名前／scope、mutability、public signature、forward call、enum構文の保持、resource上限、layout統合、foreign snapshotを検証します。名前の失敗が1つでもあればresolved programを返しません。Expression／patternの型、aggregate layout／循環、exhaustiveness、return経路の完全性は後続検証です。Loweringや実行の前にresolved programをこれらの検証へ通す必要があります。

2026-10-17実装：`lang-hir::layout` はguest valueの割り当て前にresolvedなcopy専用nominal定義を検証します。ID、元source／name span、member名の一意性、enumが空でないこと、参照を検査します。反復処理の依存stackでrecordやenum payloadを通る循環を拒否し、Rustの再帰を使いません。Scalarは1 cell、recordは1＋全field、enumは1＋最大payloadのcell数です。Scalarと空recordのdepthは1です。Checked arithmeticで、既定の定義数4,096、field／case／payload member数256、type depth 64、valueあたり4,096 cell、診断数4,096、work 10,000,000を強制します。Workは定義、名前、参照、依存訪問、case訪問、再訪問を含むfield／payload集計を課金します。10件のHIR testと2件のparsed-source統合testで、depth 64／65とcell 4096／4097の境界、明示的な深いstack、overflow、循環、不正metadata、small budgetを検証します。Shape失敗ではlayoutなしの診断を返し、input／limit失敗ではstructured errorを返します。Layout成功はexpression／pattern typing、exhaustiveness、実行可能programの成立を意味しません。

2026-10-17実装：`lang-hir::program::check_program` はresolved source HIRを受け取り、metadata、scope、signature、arena参照を検証してからread-onlyな `TypedProgram` を構築します。Source arenaはtreeとし、循環、node共有／orphan、foreign span、不可視local、不正signatureをloweringへ渡しません。Nominal layout検証を呼び出し、primitive／nominal callとvalue、recordの完全なfield集合、enum payloadの構文／arity、field projection、pattern shape／binding、全arm body、return経路の完全性を検証します。Factsはexpression／local／patternの型、field index、記述順から宣言順へのrecord対応、正規化したpattern field順、block return flowを保持します。`SRC0005`／`SRC0006` は元の文脈を維持し、新しい型不一致に `SRC0014`、shapeに `SRC0015`、missing witnessに `SRC0016`、covered-arm warningに `SRC0017`、unreachable-statement warningに `SRC0019` を使います。Warningはtypingの成功を妨げず、errorが1つでもあればtyped programを返しません。

Exhaustivenessはbool／record／enumの直積と、言及された整数literal＋補集合の代表値へconstructor specializationを適用します。Searchとwitness rendererは明示stackを使います。全columnがwildcardのmatrix rowがあれば、残る直積の値を列挙せずcoverageを確定します。Missing witnessはfunctionのmoduleから有効なpathを使います。Type checkingはmetadata／node／operand／signature parameter／field候補の訪問を10,000,000-step budgetへ課金します。Exhaustivenessは別の10,000,000-step budgetへstate、row／cellの訪問、member copy、constructor、related arm label、witness fragmentを毎回課金します。既定ではarenaを各1,000,000、宣言／listを前述の上限、再帰を256、診断数を4,096、witness出力を65,536 byteに制限します。Limit超過は `SRC0011` を返し、誤ってexhaustiveと判断しません。24件のsource統合testは、独立した有限domain coverage maskとの1,458件の照合、256-field bool直積、moduleをまたぐwitness修復、不正IR／budgetを含みます。`typed_source_hir` は独立したresolved-IR境界をfuzzし、targetのcompileとinstrumentationなしの10,000-run smokeが完了しました。Coverage-guided／sanitizer evidenceは主張しません。SourceのMIR／VM／native統合、portable artifact、CLI／toolingは後続段階です。

### Compilation units and validation

2026-10-17実装：`lang_mir::unit` はRFC 0006のcompilation-unit契約を定義します。Source本文／相対pathとbyte span、nominal定義、typed host import、名前付きfunction／register／block、任意の `main() -> i64` entryを含みます。Primitive定数／演算、named call、record構築／projection、enum構築／test／checked payload projection、内部整数比較を扱います。`.verify` はname／type／layout、signature、source境界、register定義、dominance、reachability、block argument、ownership制約を検証後だけimmutableな `VerifiedUnit` を返します。Checked enum projectionはcase guardの証明なしでも有効なIRであり、runtime case不一致でtrapします。Source lowering、compilation-unit VM、scalar native adapterはこの表現へ接続済みです。既存の単一function APIとone-block owned-resource fixtureは維持します。

Unit検証は反復処理のnominal依存走査、圧縮predecessor list、反復処理のreverse-postorder immediate dominatorを使います。構造上限だけでdataflow workも制限できるとは仮定せず、CFG／operand／dominatorの再訪問を課金します。共有 `Budget` はdecoder／verifierを通算計数し、既定でwork 10,000,000、要求bufferの累計128 MiB、要求vector要素の累計8,000,000をtemporary bufferも含めて制限します。解放してもbudgetは回復しません。Name byte／比較、source-line index、table要素の初期化、metadata／type／signature／member訪問、instruction、operand、edge、ancestor走査も課金します。Buffer割り当て前にchecked arithmeticとfallible reservationを使います。Caller所有のraw unit storageはverifierのallocation計数から除き、decoderは自身のbufferを同じbudgetへ課金する必要があります。15件のtestが上限と正確なlayout、span、owned制約、独立した経路除去oracleとの1,536件のSSA dominance照合を検証します。Rust object layoutはwire formatではなく、schema 1のbyte encodingは別の残作業です。

### Source program lowering

Source生成unitは `lang_codegen_clif::unit` により既存native profileへ接続します。要求unit limitを再検証し、変換buffer／workへ課金し、symbolic registerを旧連続順へ再配置し、明示host IDを対応付け、immutableなbyte spanをline／scalar-column debug metadataへ変換します。部分的・複数fileにまたがるfunction mapは位置を黙って失わず拒否します。未対応signature／operationはbackend codeの割り当て・公開前に拒否し、新guest callとcopy aggregateはnative後続作業までVM側の責務です。全宣言host importをJIT compile前にregistryでpreflightします。元のnative compile／旧validation APIはtrusted-input境界を保持し、adapter budgetによるnative compiler時間／memory制限は主張しません。9件のsource conformance testは演算／CFGのVM・JIT一致、signature確認付き引数なし呼出し、bytecode／JIT／AOTでのspan、未対応case、typed import／owned identity、metadata／budget failureを扱います。Tier-1条件付きC harnessがparsed-source objectをdirect／response-file両modeでlink／実行します。2026-10-17のlocal evidenceはmacOS AArch64であり、同じWindows経路はCI実行を要します。

2026-10-17実装：`lang_semantics::program_lowering` はimmutableなtyped source HIRを検証済みcompilation unitへ変換します。Argument／payloadの左から右の評価、recordの記述順評価と宣言順格納、scrutineeの1回評価、順序付き副作用なしpattern test、case guard後のchecked enum projection、arm-local bindingを保持します。Exhaustivenessにより最終armを無条件にでき、汎用runtime match-failure経路は生成しません。定数trueの構造的branchでwarningのみのcovered armをverifier上到達可能に保ちます。Joinはtyped block argumentを使い、loop headerはaggregate／boolを含む可視mutable valueを運びます。Dead statementは命令を生成しません。合成jumpを含むfunction、全instruction、全terminatorにsemantic byte spanを保持し、fileを読まずsnapshot本文をコピーします。構築と最終検証はwork／allocation／element budgetとunit全体register／block／instruction countを共有します。8件のparsed-source testで契約と拒否境界を検証します。後述の実行testでsource VMと初期native scalar conformanceを完了します。

### Compilation-unit execution

2026-10-17実装のvirtual-source基盤：`modules::load_virtual` はhost指定root byteをfile読込と同じ通算module／source／token／node budgetとraw encoding検証でparseします。Temporary fileを作らずrootを再読込せず、child fileには明示的canonical source rootを要求します。`check_expression_function` は1つのreturnだけを持ちlocal／parameter／patternがなく、unit内から呼ばれないprivate root `__repl_eval_*` wrapperだけの結果型を推論します。他の宣言には明示result型検証を維持し、最終signatureは検証済みexpression factsから得ます。`lower_library` はscript entryの自動選択を止めます。8件のtestはvirtual UTF-8 snapshot、権限、nested module、上限、推論と不正wrapperの制約、通常typing、library entry policyを検証します。後述のtransactional sessionがこれらのAPIを使います。

2026-10-17実装：`lang_vm::unit` は検証済みportable programを明示 `Vec` frameで実行し、guest callにRustの再帰を使いません。各frameは全registerの最大cell数の検証済み合計を1つのflat bufferへ割り当て、offsetとparallel-copy schedulingはregister metadataとします。Recordはzero header、enumはcase tagと最大payloadまでのzero paddingを持ちます。Aggregate構築／projection、argument、return、CFG copyを課金済みstorage内で処理します。Parallel copyは上書き前にfan-outを処理し、cycleを1 cellずつ回転させます。Result storageは最終frame bufferを再利用します。Typed embedding valueは実行前に厳密なsize、bool、nonzero owned index、record header、enum tag／padding、nested memberを検証します。既存owned identityは論理resource indexの転送であり、新resource managerではありません。

安全な既定limitはfuel 1,000,000、host call 1,000、frame 256、live cell 1,000,000です。Embeddingは代替値やunbounded実行を明示設定できます。Interruption、deadline、instruction fuelの順に確認します。Instruction／terminatorはfuel 1を消費し、guest callは続いてcontrol、予定frame depth、live cellを割り当て・entry前に再確認します。拒否したcallのfuelは消費済みとし、未entry calleeをtraceへ含めません。初期frame／cell拒否はfuel 0です。Host callはcontrol／budgetを再確認し、trustedかつnon-preemptibleなcallbackへ入る直前に課金します。未使用を含む全宣言importをqualified nameから厳密なtyped registry登録へguest副作用前に解決します。Failureはguest frameを解放し、reason、counter、fault span、内側から順のcall siteを返します。先行host副作用は保持します。Trace自体の割り当ても失敗可能とし、失敗時は不完全であることを明示します。Limitは論理guest cellを数え、callback割り当てやnative実行は対象外です。

17件のsource→canonical wire→VM testはcall、2,000段recursion、mutual recursion、aggregate／pattern、評価順、frame解放、厳密なfailed-call／host計数、control優先順、deadline、import、source stackを検証します。7件のhand-built unit testはparallel-copy oracleとの324実行比較、canonical value failure、最大enum cell課金、owned identity、`InvalidVariantProjection` を含みます。Parsed scalar conformanceもこのVMと元のscalar VM・JITを比較します。Decoder fuzz targetは受理entryをfuel 128、host call 0、frame 8、cell 4,096で実行し、instrumentationなしの10,000-run smokeが成功しました。Coverage／sanitizer evidenceは主張しません。CLI／REPLとeditor統合は別成果として残ります。

### Portable bytecode

2026-10-17実装：`lang_bytecode::unit` は検証済みMIR unitをregister programへ昇格し、受理済みRFC 0006のportable schema 1を実装します。明示的なlittle-endian codecが `LANGBC\0\0`、schema／flags／payload長、source／definition／import／function table、typed register、全12 operation、terminator、byte span、任意のentryを書き込みます。Global tableをcanonicalにsortし、対応する全type／call／import／span／entry IDを再配置します。Field／case／block／instruction順は保持します。Decodeはnoncanonical順序、未知version／flags／tag、不正UTF-8／bool、長さ／trailing-data不一致、不正typed unitをprogram返却前に拒否します。既存prototype／component formatは維持します。

Codecの既定artifact上限は64 MiBです。Fallible reservation前にcountと残入力の最小量を検証し、source／register／block／instructionの合計を通算します。Raw stringとoutput byteは共有128 MiBの要求byte budgetへ、structured vectorはさらに8,000,000要素のbudgetへ課金します。Decodeとverificationは1つの `Budget` を使います。Encodeは要求limitでcallerのprogramを再検証し、失敗可能なbudget付き比較でsortし、正確な出力長を割り当て前に計算し、2回目のpassで明示fieldを書き込みます。Verifier workに加え、readerのword／tag、最大1 KiBごとのUTF-8検証chunk、両passのwriter field、sort／index memberを課金します。11件のtestは固定88-byte golden、全opcode、global ID再配置、非連番register／CFG round trip、全切断、不正input、canonical reencode、通算budgetを含みます。`portable_bytecode_decoder` はraw byteとheader以降を変異させた有効payloadをfuzzし、compileとinstrumentationなしの10,000-run smokeが完了しました。Portable artifactの成立であり、source実行やinstrumented fuzz coverageは主張しません。

### Source formatter

2026-10-17実装：`lang_syntax::format` は実際のparsed token ownerとdelimiter文脈を使ってformatします。宣言、statement block、match body、record value／patternを区別し、delimiterごとの4-space indentation、operator／注釈のspace、1行1 statement／member／宣言を適用します。Optional comma、literalの表記、実comment／tokenの全byteは維持します。不正treeは `PreservedInvalid` と元snapshot全体を返し、有効treeは末尾LF付きの冪等なtextを生成します。反復passとdelimiter stack、失敗可能buffer、16 MiB output上限、node／child／token／indentation／text-chunkを課金する10,000,000 work limitを使います。Lookaheadを1回index化しtriviaの反復走査を避けます。9件のsyntax testは完全なsemantic CST shape／comment round trip、scalar fixtureの全comment挿入境界、不正input保持、resource／snapshot failureを検証します。2件のsource testはformat後の複数file aggregateのVM結果／counterとscalar VM／JIT behaviorを検証します。Source parser fuzz targetも有効outputを再parse・再formatしnode／token保持を検証し、instrumentationなしの10,000-run smokeが成功しました。単独CLI formatter commandはMilestone 4の統合作業です。

### Script CLI

2026-10-17実装のscript CLI：`lang run FILE` は共通 `lang::frontend` analysis／既定compile limitを使い、`main() -> i64` を検証して、空のtyped host registryでverified unitを実行します。Hostは `--source-root` がなければfileの親をsource rootに選びます。Resource診断はsnapshotを保持し、raw encoding失敗はspanを捏造せずstable code／byte rangeを報告します。成功時は符号付き10進数と改行を1つ出力してexit 0、compile／runtime失敗はstderrとexit 1、usage errorはexit 2です。Runtime errorはcounterとentry済みsource frameを保持します。VM-limit flagと従来の整数2つのprototypeも利用できます。10件のCLI／frontend testはこれらの契約、複数fileのaggregate、entry、不正input、typed-import拒否、limitを検証します。

### Transactional REPL

2026-10-17実装：`lang::repl::Session` は1つのitemまたはexpressionを受け付けます。候補定義全体をanalysisし、library MIR検証後にだけitemをcommitします。Errorは以前のtext定義を保持します。Expressionは衝突しない生成wrapperを選び、新しいVM counterで実行し、scalarまたはqualified name・宣言順fieldのaggregate valueを出力します。Mutable valueを永続化しません。`:reset` は定義を消去し、`:quit` またはEOFで終了します。CLIは追加byteだけを走査してnested commentと括弧／波括弧を追跡し、割り当て前に入力上限を確認し、不完全EOFを診断します。TTYにはprimary／continuation promptを出し、pipe入力には出しません。処理済みsubmission error後も通常quit／EOFはexit 0、不完全EOFと致命的IO／入力上限失敗はexit 1です。File moduleには `--source-root` を要求し、CLIのhost importは空です。

元submissionは独立immutable snapshotです。Segment mapでprimary／related label、fix edit、fault位置、全guest call siteを合成sourceから元snapshotへ戻し、runtime表示で生成wrapper名／行を隠します。各reportがsourceと失敗runtimeのunitを所有するため、後続submissionやresetで位置が無効になりません。Sessionにはtext定義だけを保持します。出力は反復処理とし、既定上限は65,536 byteです。線形byte走査で既存の全identifierより長いwrapper名を生成し、走査／生成byteを設定済みparse-work枠へ課金します。各compiler phaseも個別の設定limitを維持します。Guest／display失敗でhost副作用をrollbackしません。16件のtestはtransaction、item数、canonical display、source／fix／stack対応、fresh budget、host副作用、file権限、不正UTF-8／comment、上限、buffering、pipe CLIの回復／reset／EOFを検証します。Local TTY smokeでも継続入力とquitを確認しました。

### Incremental architecture

2026-10-17実装：immutableな `SourceMap` のclone／includeは `Arc<SourceFile>` snapshotと既存identityを共有し、新editには引き続きglobal unique IDを割り当てます。Moduleは `Arc<SyntaxTree>` を保持します。`load_preparsed` は明示read権限の下でcanonical identityを解決し、そのparser結果を渡すhost `ModuleProvider` を受け取ります。Module格納前に同じparse policyと通算source／token／node budgetを検証し、parser-limit snapshotを保持し、同じcanonical fileは2回目のparse要求前に検出します。Module index化では従来の宣言／alias workに加えてnode／path-child訪問を課金します。6件のtestは同じpointerのtree／source再利用、duplicate preflight、通算budget、不正／foreign input、policy不一致、診断snapshot保持を検証します。Query recipeの自動実行とtransportは最後の統合scopeです。

2026-10-17実装：`lang_semantics::database` は `lang-query` 上でinput → parser → module → semantic診断のrecipeを自動実行します。Module構築は `load_preparsed` 経由でcached CSTを使い、semantic analysisは名前解決とsource全体のtypingを実行し、typed HIRを重複保持せずimmutableな診断／snapshotを保持します。通常editはreverse consumerをinvalidateし、無関係なentryを維持します。新しいvirtual fileを含めopen overlayをdiskより優先します。Disk読込は上限付きで明示canonical rootを要求し、rootなしではfileを読みません。Watch／discovery epochはmissing fileやsymlink identity変更を含むmodule発見を保守的にinvalidateし、無関係なparser snapshotは再利用します。Closeはoverlay inputを捨て、共有poolを解放するためparser cacheをinvalidateします。Closed dependencyは必要時にdiskから再読込します。

独立したprepare／compute／publish APIでdatabase外のworkを可能にし、graph ticketでedit、close／reopen、新しいwork、別graphの結果によるcache置換を拒否します。Host cacheの既定limitは既知file 256、input byte 16 MiB、parser node 2,000,000、module index node 2,000,000、診断text 16 MiB、graph node 4,096／edge 65,536／graph work 10,000,000です。各compiler phaseも個別limitを維持します。Cache countにcallerが保持する過去snapshot／jobは含めず、そのlifetimeはhostが管理します。12件のtestは実tree再利用、選択的invalidation、stale publication拒否、不完全sourceの元診断、close／disk fallback、missing／watched／virtual module回復、読込権限／symlink preflight、resource境界、edit／close後の回復を検証します。LSPのdocument／protocol adapterが最後の統合作業です。

最初の非自明なfrontendからdependency-tracked query systemを使用します。各queryはdeterministicで、個別に計測できる必要があります。

- Fileをparseする
- Resource-boundedなcompile-time constructを展開する
- Module／import graphをresolveする
- Nameをresolveする
- Typeとeffectをinfer／checkする
- Ownershipとexclusivityをcheckする
- MIRをlower／optimizeする
- Unit単位でbytecodeまたはnative codeを生成する

Hashとtiming explanationを永続化し、build toolがunitを再buildした理由を回答できるようにします。Source fileとpackage manifestを明示的なquery inputとして扱います。

2026-10-17実装：`lang-query` はbackend非依存のin-memory graph／cache coreです。Hostはopaque key、immutableな `Arc` value、完全な明示dependency集合を渡します。Input editは推移的consumerをinvalidateし、無関係なcacheを保持します。Query publication ticketはgraph identity、query generation、dependency revisionを記録し、edit、新しいwork、publication、removal、key再利用後に古い結果で現在のvalueを置換させません。Dependency置換で古いreverse edgeを残しません。Key closeはconsumerをinvalidateし、incoming referenceを上限付きtombstoneとして保持するためreopenでも古いworkを復活させません。Cycle、node／edge／work limit、revision overflowは全bookkeepingをstate変更前に検証します。Traversalは反復的かつdeterministicです。上限対象はgraph bookkeepingであり、hostのanalysis-value memoryやcodeではありません。構造化input HIRのintegration testで診断cache再利用と古い結果の拒否を示します。Query recipeの自動実行、全input／dependency検出、hash／timing永続化、parser／module統合、LSP adapter／transportは未実装であり、graph coreだけでtooling milestoneの広いtaskを完了しません。

## Memory model baseline

この設計では、memory safety、一般的なshared object graph、単純な構文、deterministic destruction、zero overhead、GCなしをすべて同時に最大化できないという基本的トレードオフを認識する必要があります。

受理済み所有権のcoreはRFC 0004で規定します。以下のparameter passing推論、closure capture構文、unsafe操作、allocator API、freestanding profileは追加検討の方針であり、受理済み規則を増やすものではありません。

- 通常の `T` はdeterministic destructionを持つowned valueである。
- Moveは推論し、use-after-moveをrejectする。
- 自明でないvalueのfunction parameterは、曖昧さがなければ既定でnon-owning readとする。
- Mutable accessはexclusiveかつlexicalとする。通常のcodeでlifetime parameterを書かせない。
- Non-escaping borrowは推論され、明示的なowning abstractionなしには保存できない。
- `shared T` は明示的なatomic reference-counted referenceであり、`weak T` でcycleを切る。
- Region／arenaにより、graph、compiler、game、request-scoped dataをbulk lifetime managementできるようにする。
- Closureはcaptureをborrow、move、shareのどれで行うかを表す。
- Raw pointer、union、unchecked indexing、layout cast、foreign ownershipは狭い `unsafe` boundaryの背後に置く。
- Standard libraryはallocator-aware containerを公開し、後にno-runtime／freestanding subsetを許可する。

受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)は、plain valueの単独所有、明示的 `shared`／`weak`、mutation前のstrong shared cycle拒否、意図的なcycleのlexical arena格納を要求します。これはsource／runtime実装の責務であり、実験modelがproduction ownership checkerであるという意味ではありません。

残る実装evidenceにはnested cleanup、partial initialization、self-reference rejection、atomic sharing／cycle preflight／arenaのruntime・memory costがあります。RFC 0005はsuspensionをまたぐすべてのborrowを既に拒否します。その緩和には新しいRFCが必要です。

### Cyclic graph実験

`crates/semantics` のgraph modelでは、2つのshared-cycle policyの下で、明示的なshared／weak ownershipとarena-scoped storageを比較します。

- Shared nodeはexternal rootを1つ持って開始する。Strong edgeはtargetをretainし、weak edgeはretainしない。
- Defaultの `Reject` policyは、shared-reference cycleを閉じるstrong edgeをstructured diagnostic `SEM0018` でrejectする。同じcycleをweak edgeで閉じることは許可し、expired weak edgeのupgradeは `SEM0021` を報告する。
- 実験的な `CollectUnreachable` policyはstrong cycleを許可する。External rootをdropした後、残るすべてのlive rootからstrong edgeをtraceし、到達不能なshared nodeを逆生成順にreleaseする。Weak edgeはtargetをliveに保たない。
- 1つのarena内のnodeは、arenaがlifetimeを所有するため任意のcycleを形成できる。Arena間、またはarena storageとshared storage間のedgeは `SEM0017` を報告する。
- Arenaをdropするとnodeを逆生成順に正確に1回releaseする。その後にstale node handleを使うと `SEM0012` を報告する。
- すべてのoperationがsource spanを持ち、関連するallocationまたはedge spanが存在する場合はdiagnosticに保持する。

RFC 0004は `Reject`、weak back-edge、lexical arenaを受理しています。`CollectUnreachable` は未受理の代替案です。同期graph実験はruntime reference-count実装ではなく、weak upgradeはlivenessを確認しますが新しいretained ownerを返しません。Atomic publication、thread safety、destructor failure、production weak-reference orderingもmodel化していません。これらは専用実装とconformance testが必要です。

レビュー結果：**production実装に向けてneeds prototype/measurement**。受理済みstrict policyの再決定ではありません。Milestone 2でsource checking、shared／weak lifecycle、cycle-preflight atomicity、arena lifetime validationを扱います。

### Async borrow／cancellation実験

`crates/semantics` の実行可能modelはRFC 0005のstrict baselineと、未受理の制限を緩めたpolicyを比較します。

- Task-owned valueはsuspensionを越えてtask frame内でliveのままにする。
- Defaultの `RejectAll` policyは、suspension pointでactiveなshared／exclusive borrowをstructured diagnostic `SEM0007` でrejectし、suspension前に終了したborrowをacceptする。
- 実験的な `AllowSharedFrameBorrows` policyはtask-owned valueのactiveなshared borrowがsuspensionを越えることを許可する一方、exclusive borrowは引き続き `SEM0007` でrejectする。
- Cancellationはtaskがsuspendedである間だけ観測する。
- 実験的policyでは、resume後もshared borrowを維持し、cancellation時はactive borrowを終了してからtask-owned valueを逆生成順に正確に1回dropする。
- すべてのoperationがsource spanを持ち、failureではboundaryとそれを越えたborrowの両方を示す。

RFC 0005は `RejectAll` を受理し、`AllowSharedFrameBorrows` は実験のままです。Modelはsuspension時だけcancellationを観測し、仕様のtask-tree runtimeや冪等なcancellation-request APIではありません。Nested scope、partial initialization、destructor failure、panic containment、同等なVM／native cleanup traceは残る実装作業です。

レビュー結果：**runtime実装に向けてneeds prototype/measurement**。Suspensionをまたぐpinned／shared／exclusive borrowは受理済みbaselineから除外されています。実装者が独自の挙動を選べる未決事項ではありません。

## Error／effect model

- Recoverable errorはtyped result valueを使用し、surface sugarとして `throws E`／propagation syntaxを提供する。
- Panicはinvariant violationを表し、component／FFI boundaryを越えない。
- Native unwindingをstable ABIの一部にしない。
- Effectはまず、`async`、`unsafe`、host capability、場合によってはallocationなど、安全性やcompositionを変える動作だけを追跡する。Prototypeでergonomic valueが証明されるまでgeneral effect systemを導入しない。
- Cancellationはstructuredでtask scopeを伝播し、destructorとcleanup blockを予測可能に実行する。

## Concurrency model

1つの抽象を強制せず、複数layerを組み合わせます。

- Application code向けのstructured task、cancellation、deadline、async I/O。
- Mutable service state向けのactor isolationとmessage passing。
- Data parallelism向けのparallel iteratorとscoped fork／join。
- Systems code向けの明示的atomic、lock、channel、unsafe synchronization。
- Ownershipとmutabilityからsend／share capabilityを導出し、data raceを静的にrejectする。

Schedulerは交換可能なruntime serviceとします。Libraryはglobal executorを前提とせず、embedded hostがschedulerをdriveまたは統合できる必要があります。

### Shared mutable service実験

`crates/semantics` のservice modelはRFC 0005のowner-isolated baselineと、未受理のshared-mutation policyを比較します。

- Defaultの `OwnerIsolated` policyでは、1つのowner taskだけがservice stateへ直接mutable accessを持つ。Shared handleはmessageをenqueueできますが、別taskからのdirect mutationは `SEM0031` を報告する。
- 実験的な `ExclusiveCapability` policyでは、有効なhandleを持つ任意のtaskがactive mutationのないときにmutationを開始できる。Handleを持たないtaskはrejectし、同時beginはactive mutation spanを伴う既存のstructured mutation-conflict diagnosticを報告する。
- Sendはpayloadをmailboxへmoveする。Non-sendable payloadは `SEM0028`、成功したmove後の再利用は `SEM0027` を報告する。
- Ownerは引き続きmessageをFIFO順にprocessする。有効な任意のhandleからのcancellation requestは、実験的policyでのnon-owner mutationを含め、active mutationが終了するまでdeferする。
- Cancellationはqueued messageをFIFO順にdiscardし、queued payloadとservice stateをそれぞれ正確に1回dropしてから、後続sendを `SEM0024` でrejectする。
- Operationはstable diagnostic code `SEM0022`〜`SEM0035` のもとでprimaryおよび関連source spanを保持する。

RFC 0005は `OwnerIsolated` を受理し、`ExclusiveCapability` は未受理のlock-likeな代替案です。Modelはsend／share導出、task tree、繰り返しの冪等cancellation request、backpressure、reentrancy、panic cleanup、runtime schedulingを備えていません。Model diagnosticsは実験evidenceであり、完全なsource／scheduler contractではありません。

レビュー結果：**runtime実装に向けてneeds prototype/measurement**。Milestone 3では受理済みisolation／cancellationを実装し、未規定のchannel／scheduler／error詳細をsource behaviorとして公開する前に決定します。

## AOTとscript実行

### Bytecode first

AST interpreterではなく、typed register-based bytecode VMを使用します。

- VMは実行前にbytecodeを検証する。
- Instructionは有用な範囲でlanguage-level typeとsource locationを保持する。
- Native codeとscript codeの双方で、同じruntimeがstring、collection、shared reference、task primitive、panic／error boundaryを実装する。
- Profiling後にinline cacheとspecializationを追加できる。
- JIT compilationはoptionalであり、stable bytecode semanticsの後に実装する。

### Embedding API

実装言語に関係なくstable C ABI embedding layerを提供します。

- Runtimeとisolated script contextの作成／破棄。
- Memoryからverified bytecode／componentをloadする。
- Native function、type、constant、capabilityをregisterする。
- Validated function handleでexportをcallする。
- Generated bindingまたはopaque handleでvalueを変換する。
- Memory、instruction／fuel、recursion、task、wall-clock limitを設定する。
- Executionをinterruptし、structured diagnosticとstack traceを受け取る。

Call-site validationとgenerated thunkをcacheします。Callはin-processのままでIPCを回避しますが、boundary checkとrepresentation conversionは実在するcostであり、benchmarkする必要があります。

[RFC 0003](rfcs/0003-versioned-component-contracts.md)は、最初の実行可能なcomponent-contract sliceを規定します。Runtimeは現在、名前付きscalar import／export、保守的identifier、明示的capability requirement、embedderが供給する論理name-to-host-function bindingを持つschema-versioned package metadataをvalidateします。Portable metadataはprocess-localな `HostFunctionId` valueを含みません。Validationは構造的conflictとunsupported import typeを拒否し、undeclared importへのbindingを拒否し、host lookupより前にcapability grantをcheckし、すべてのimportをtyped registryに対してresolveし、resolved local bindingを持つvalidated wrapperを返します。Deterministicな実験的 `LCMP` schema-1 codecはscalar declaration subsetをencodeし、明示的なtotal-byte、declaration-count、structural-depth、string-length limitの下でuntrusted byteをdecodeします。Malformed-input errorはbyte offsetを持ち、libFuzzer targetがdecoderを実行します。このschema-1 codecはaccepted candidate schema-2 serializationではなくunstableなfixtureであり、WIT実装、component-IDL resource declaration、canonical ABI、component exportのexecution pathでもありません。後述するprototypeがaccepted candidate serializationと選定したresource／adapter pathをcoverし、production IDL／generated adapterは後続作業です。

別のcandidate `LCMP` schema-2 envelopeは、contract document（`kind 0`）とversioned failure-report document（`kind 1`）を定義するようになりました。Contractはpackage key／完全なsemantic version、sort済みnormalized interface requirement／required-member projection、SHA-256 tag付きexact lock、sort済みcanonical resource-type identityをdeterministically encodeします。Required memberはparameter／result順序、exact capability、完全なnominal resource key付きの `i64`、`bool`、`own<T>`、`borrow<T>` value typeを保持します。Failure reportは1つのprimary recordと順序付きsuppressed cleanup failureを保持し、poisoned stateをencodeし、固定category／phase／diagnostic tagを使い、message redactionとembedding-sanitized transportを区別します。Policy明示conversionはcleanup invariant trap／destructor-failure順序を保持しながらruntime `BoundaryFailure` recordをprojectします。Constructionはinvalid identifier／capability、duplicate entry／member name、invalid interval、未宣言resource参照、borrowed result、inconsistent failure classification、cleanup以外のsuppressed record、unsafe redaction state、poisonされていないdestructor failure、orphan lock、lock-version mismatchを拒否します。Bounded decoderはwrong schema／document kind、malformed tag／boolean、truncation、invalid UTF-8、trailing byte、byte／item／depth／string limit超過をbyte offset付きで拒否します。Check-in済みgolden encoding、round-trip／canonical-order test、semantic validation test、deterministic arbitrary-byte coverage、専用fuzz targetが両layoutを保護します。Accepted RFC 0003はschema 2をimplementation baselineとして採用しますが、stable public production formatではなくversion管理されたdesign evidenceのままです。

Component runtimeにはin-memoryのgeneration tag付きresource tableもあります。Copy可能なowned handleはuntrusted callerが複製できるinteger handleをmodel化します。Resourceをreleaseすると最小のfree slotを再利用する前にgenerationを進め、generation exhaustionの場合はslotをretireするため、stale copyはreplacementをaliasできません。Shared borrowは共存でき、exclusive borrowにはactive borrowがないことを要求し、mutable accessにはliveなexclusive tokenを要求し、releaseはすべてのlive borrowを拒否します。Borrow tokenはmonotonicで明示的に終了します。Stale token、repeated end、repeated release、conflicting borrow、identifier exhaustionはstructured errorです。これはtable-local lifecycle checkを実証するもので、serialized handle layout、cross-component ownership transfer、destructor behavior、thread safety、canonical resource loweringではありません。

Runtimeはschema-1 serialization／production package-manager resolutionから独立したpackage／interface compatibility negotiationもprototypeします。Normalized semantic versionはprerelease／build identifierを保持し、numeric prerelease identifierをmachine wordへ制限しません。Exact requirementは完全なvalueを比較し、intervalはSemVer precedenceと明示的なprerelease policyを使用します。Capability grantはprovider inventory validationより先にcheckします。Exact lock inputは1つのidentity、version、opaque digestをfallbackなしで要求し、それ以外ではexact keyとrequired-memberのstructural projectionでcandidateをfilterしてからdeterministicにhighest-precedenceを選択します。Projectionは順序付きscalar type、nominalなowned／borrowed resource type、exact capabilityを含み、要求されていないprovider memberを許可します。Same-precedenceのbuild／digest tieはambiguousで、resolution成功時はimmutable bindingを返します。Canonicalなin-memory lockfileはcompleteなexact SHA-256 selectionを記録し、fallbackなしでreplayし、embedded contract lockとreconcileし、選択済みcontract／bindingをfreshなinstance stateへ直接moveします。On-disk lockfile syntax、registry access、artifact download、publication workflowはまだ定義しません。

実行可能なschema-1 scalar fixtureは、validated component metadataを3つすべてのprototype execution pathへ接続します。Capability／logical-name bindingをtyped host registryに対してvalidateしてから、選択した `i64 -> i64` import／export pairをMIRへloweringします。VM／JITはinteger extremeを含む同じfunctionを同等のwrapping resultで実行します。同じvalidated fixtureがhost-native objectをemitし、そのexportとresolved host importをmacOS AArch64のdirect／response-file pathでlink／実行します。これは1つのscalar contractに対する狭いdifferential evidenceであり、general component loader、canonical resource adapter、failure boundary、stable serialization、cross-platform component runtimeではありません。

Backend-neutralなcanonical-resource adapter traceにより、runtime handle layoutを公開せずRFC 0003のownership／failure orderingを実行可能にしました。Instanceごとのboundary indexは非ゼロかつ単調増加で、再利用しません。Call全体のpreflightはすべてのresource argumentとdestination limitをvalidateし、storageを予約してから、allocationもcallbackも行わないcommitでownerをmoveまたはcall-scoped borrowを公開します。Cleanupはnested borrowを内側から閉じ、残るframe ownerをremoveしてからacquisitionと逆順にdestructorを呼び、独立したdestructor failure後も継続し、先行するpanic／trapをprimaryのまま保ち、callback／invariant／destructor failure後にcalleeをpoisonします。共有host-value bridgeはopaque identityをallocateしてgeneration-check済み `ResourceTable<T>` ownerへmapし、canonical publication failure時はidentityを再利用せずvalueをrollbackし、runtime borrow operationをresolveし、cleanupがactual valueをexactly onceでconsumeできるようにします。General instantiationはinstanceを公開する前にcanonical順のschema-2 requirementをすべてnegotiateし、immutable bindingとfreshでboundedなadapter stateを持つinstanceを生成します。Conformance fixtureはVM、JIT、macOS AOTのscalar resultを同じowned transfer、guest-trap cleanup、redacted failure projectionへ通し、value、state、report outcomeが一致します。後続の直線形nominal `own<T>` laneはMIR、bytecode、VM、JIT、tier-1 AOTで動作しますが、汎用resource control flowとreturn ownership adapterは未実装です。Asynchronous cancellationも未完了です。

VM prototypeは、合成可能な3つのopt-in limitであるinstruction fuel、host-call budget、value-slot limitに加えて、optionalな共有one-way interruption flagと絶対monotonic deadlineを実装します。各bytecode instructionとterminatorは1 fuel unit、実行に入る各host callbackは1 host-call unitを消費します。Validationとarity checkはmetering前に行い、各動的limitは対応するside effectの前に確認します。VMはregister fileを割り当てる前に、検証済みprogramのregister countをvalue-slot limitと比較します。すでにsetされたinterruptionまたは期限切れdeadlineはそのallocation前にexecutionを停止します。以降のsafe pointは各instruction／terminatorの開始時と、実行に入るcallback直前のboundaryです。Structured termination errorは観測したsafe pointでのinstruction／host-call consumptionを報告し、両conditionが成立する場合はinterruptionを優先します。Callbackはflagをsetしたりdeadlineを超過したりできますが、その内部処理は完了し、次のVM safe pointでconditionを観測します。Limit付きAPIはvalue、両方の消費count、静的なslot footprintを返し、既存のconvenience APIはunbounded、non-interruptible、deadlineなしのままです。非停止back-edge test、有限loop-carried countdown test、zero-budget callback test、過小なslot-limit test、pre-execution／callback-triggered interruption test、pre-execution／running-loop deadline testにより、loop停止、callback抑止、allocation前の拒否、safe-point accountingを確認しています。Slotはbackendが定義するVM register entryでありbyte countではなく、host callbackのallocationを計上しません。これらのcontrolはheap、recursion、task、callback内部のpreemption、native／JIT executionには未対応であるため、完全なsandboxではなくembedding interfaceの証拠です。

初期の2つの型付き `i64` callbackがhost所有allocationを検証します。1つ目は1回の `Box` allocate／read／drop round tripを行います。2つ目は異なるwrapping valueを持つ64個のboxを作成し、512 payload byteすべてを同時に保持して合計した後にdropします。VM／JITはRust実装へdispatchし、link済みAOT objectは対応するCの `malloc`／read／`free` 実装をimportします。macOSのdirect／response-file integrationは両wrapperを実行します。Benchmark schema 8はsingle-allocation callbackのdirect Rust／VM／JIT latencyを計測し、schema 11はburst用の同じ3 rowを追加します。Callback allocation workはprocess creation／loadingに埋もれるため、process-startup rowは変更しません。これにより、3つすべてのexecution pathがboundedなallocation-backed operationのために型付きboundaryを越えられることを確認し、より単純でないVM／JIT boundary-cost workloadを得られます。これはlanguage heap、allocator ABI、ownership rule、garbage collector、memory limiter、allocator-overhead account、代表的application workloadではないため、それらのdesign／costに関する論点は未解決です。

Schemaごとの過去のtiming、allocation、process-memory結果は、現在のarchitecture要件ではなく[benchmark evidence](benchmarks.md)へ集約します。Native Windows evidenceはschema 20とrun 38932964398に記録済みです。これらは2026-10-17のJIT memory回収修正前の記録であり、現在のcompile／application性能を主張する前に新しいclean計測が必要です。

### Plugin trust tier

1. **Trusted native:** AOT object／static library、specialized call、最大性能、sandboxの保証なし。
2. **Verified script:** Capabilityとresource limitを持つbytecode VM。
3. **Portable sandboxed component:** 絶対的なcall overheadよりportabilityとisolationが重要な場合のWebAssembly Component Model／WIT-compatible boundary。

User-authored pluginは既定でtier 2または3を使用します。Native pluginは明示的なtrust decisionです。

## Component ABIと複数言語

Native source-language ABIをcompatibility promiseとして使用しません。WITに着想を得た独立interface languageを定義します。

- Record、variant、enum、flag、resource／handle、list、string、option、result、future、stream、function。
- 明示的なownership transferとborrowed／owned parameter。
- Package／interface semantic versionとcompatibility check。
- Capability importとexported world。
- Native code、VM bytecode、C、WebAssembly向けgenerated adapter。

Whole-program build内では、compilerがcompatible layoutとownershipを証明した後、adapterをdirect callへspecializeできます。独立してversion管理されるcomponentではcanonical boundaryを維持します。

将来のfrontendは最初に固有HIRへloweringし、coreが意味論を忠実に表現できる場合だけshared typed MIRへloweringします。安全に表現できないfrontend固有機能は、core MIRを無制限に拡張せず、component interfaceを通じてloweringします。

## Native backend

### 初期target

- Tier 1 prototype：macOS AArch64とWindows x86-64。
- 次のtarget：macOS x86-64とWindows AArch64。
- Object format：Mach-OとCOFF／PE。
- Custom ABIより先にC ABI interoperabilityを実装する。

### Backend戦略

- 最初の実装のinitial native JIT／AOT backendとしてCraneliftを使用する。
- Backend APIを狭く、target-neutralに保つ。
- 代表的programでrelease runtime、compile latency、code size、debug品質、platform integrationを計測する。
- Craneliftがrelease performance、optimization、debug要件を満たせない場合にLLVM backendを追加する。
- 最初はsystem linkerを使用し、reproducibilityとinstallation dataが正当化した場合のみbundled LLDを評価する。

**Decision（2026-10-17）：** Workspace lockfileでresolveされるCranelift 0.136.3を、JIT／AOT両方のinitial native backendとします。これはimplementation baselineであり、stable native ABI、恒久的に排他的なbackend、language-level performance guaranteeではありません。Frontend、typed MIR、source map、linker request、artifact interfaceはbackend-neutralのまま維持し、代表的release workloadで未達のoptimization、target、debug要件が明らかになった場合のみLLVM backendを後から追加します。

| Criterion | 記録済みevidence | Disposition |
| --- | --- | --- |
| Semantic／ABI integration | Differential VM／JIT coverageと、integer、CFG、callback、scalar component、1つのtyped-owner laneに対するmacOS／Windows direct／response-file AOT execution | Initial backendとして十分 |
| Tier-1 portability | Target-neutral linker policy／明示tripleを使うnative macOS AArch64／Windows x86-64 object-link-run CI | Prototype Tier-1両targetについて十分 |
| Debug quality | 代表artifactに対するsource-mapped Mach-O DWARF／dSYMとCOFF CodeView／PDB inspection | 最初の実装として十分。Source-language local、unwind metadata、optimized stepping、JIT debugger registrationは後続criterion |
| Performance／memory evidence | 過去のclean schema-20 macOS／Windows baseline | 方向的evidenceのみ。収録後にJIT memory回収を修正したため、現在のcompile／application性能の主張には新しいclean計測が必要 |
| Architecture cost | 既存 `codegen-clif` crateはfrontend APIへCranelift typeを公開せず、JIT／object emissionをすでに提供 | 現時点で第2backendを導入するより実装／保守riskが低い |

現在のbackendは、Cranelift／object-library typeをcallerへ公開せず、同じvalidated MIR loweringからin-process JIT codeまたはobject artifactを生成します。Typed block、明示terminator、block parameter、source span、import、exportはVM、JIT、AOT path全体で同じbackend-neutral contractを共有します。共通 `SystemLinker` は `Cc`／`MsvcLink` をmodel化し、順序付きrequestをvalidateし、direct／response-file invocationをsupportしてstructured failureを記録します。macOS integrationと[Windows run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398)は両link modeをnative実行し、cleanな[schema-20 benchmark evidence](benchmarks.md)はTier-1両platformを記録します。Production installation discovery、startup／runtime packaging、stable ABI adapter、追加targetは別作業のままです。

実装前のcleanなmacOS AArch64 [debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)は、元のnegative baselineを確立しました。Commit済みの[source-mapped DWARF inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md)は、typed MIRが構造を検証したsource file、function span、各instruction／terminatorに1つずつのsource spanを保持し、Craneliftがそれらをbackend-neutralなJIT／AOT machine-code rangeとして維持し、Mach-O serializationがrelocation付きDWARF v4 compile unit、subprogram entry、line tableを出力することを実証します。代表的なadd、minimum、host-callback objectは `dwarfdump --verify` をpassし、direct／response-file executableはlink／実行され、生成した両dSYMもfixture pathと3 functionを保持したままverificationをpassします。Commit済みの[Windows x86-64 CodeView／PDB inspection](../../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md)は、同等のC13 symbol、type、line table、relocationをCOFFでvalidateし、LLDでPE／PDB pairへlinkし、LLDBでfunction-to-source resolutionを検証します。これはnative Windows runではなくcross-target debug-format evidenceです。Source-language local、unwind metadata、JIT debugger registration、optimized stepping、将来のsource-language frontendは個別のacceptance workであり、backend選定ではそれらのcost／qualityを考慮する必要があります。

Host linker discoveryは、deterministicなcandidate順で `PATH` を探索し、Unixではexecutable fileを要求し、driverがない場合は探索したpathとcandidate nameを含むstructured diagnosticを返します。macOS integrationは発見したdriverを使用し、cfg-gated MSVC integrationもdeveloper environmentが `PATH` を初期化した後に同じdiscoveryを検証します。そのenvironment外にあるVisual Studio、SDK、compilerのinstallation discoveryは個別の作業として残っています。

Root crateは、初期tier-1 linker policyをbackend-neutralな `LinkTarget` domain typeとして表現します。Exactな `aarch64-apple-darwin` と `x86_64-pc-windows-msvc` tripleがcommand flavor、default program、ordered `PATH` candidateを決定し、未supportのtripleはhost policyを暗黙に継承せずstructured errorになります。明示targetのlinker構築、discovery、platform integration、cross-target COFF testがこのmappingを共有します。別targetのsupportには明示的なpolicy追加が必要です。Installation discoveryとtarget固有のstartup／runtime inputは別layerのままです。

過去のbackend実験と当時のevidence不足は[benchmark evidence](benchmarks.md)に記録します。受理済みsource規則や完了済みWindows baselineを未決定へ戻すものではありません。

## Diagnostics、LSP、debugging

Diagnosticsは、stable code、primary／secondary span、note、fix-it、machine-readable fieldを持つstructured valueです。

2026-10-17実装：`lang-diagnostics` はimmutableなUTF-8 snapshot、全mapを通じて異なるsnapshot ID、文字境界を検証する半開byte span、1始まりのUnicode scalar位置、IDE adapter向け0始まりのUTF-16 columnを管理します。Diagnosticはprimary labelを1つ必須とし、別fileのsecondary label、note、machine-applicable／speculativeを区別したfixを追加できます。Fix previewは全参照snapshotを検証し、重複や曖昧なeditを拒否してから変更後のtextを返し、fileは変更しません。Plain-text表示はdeterministicで、制御文字とdirectional-formatting文字をescapeします。RFC 0002の `SRC0001`–`SRC0007` を表現し、構造化HIRは `SRC0002`–`SRC0007` を生成します。Literal／parser側の診断生成とtext接続済みdiagnostic conformanceは未実装です。このcrateはsource grammarやserialized diagnostic formatを導入しません。

LSPはcompilerのparser、resolver、type checker、source map、incremental query databaseを再利用します。不完全またはinvalidなprogramでもresponsiveである必要があります。

2026-10-17実装：`lang-lsp` は既存structured diagnostic modelをUTF-16 client profileのLSP diagnostic dataへ変換します。Source byte spanをcompiler line番号とは独立して変換します。LSPはLF、CRLF、単独CRを認識し、0始まりのUTF-16位置とexclusiveなrange endを使います。CRLF途中のbyte境界は前の行のcontent endへ正規化します。Adapterはsource spanと座標上限を検証し、source code／severityとrelated labelを保持し、primary-label詳細／noteをplain diagnostic textへ含めます。Hostが検証済みdocument URIを渡し、source表示名からURIやfilesystem accessを推測しません。明示source-byte、label-count、note-count、output-byte limitで変換を制限します。Fixは将来のcode-action adapter向けにcompiler diagnostic側へ残します。LSP prototypeを主張する前にclient capability filter、UTF-16 negotiation、JSON serialization／transport、document version／close notification、parser／module query、shared-graph publicationの統合が必要です。

Protocol情報源：Microsoft LSP 3.18の[text document](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/textDocuments.md)、[position](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/position.md)、[range](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/range.md)、[diagnostic](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/types/diagnostic.md)、[base integer bound](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/specification.md#baseTypes)。いずれも更新日を本文に掲載しないliving documentで、2026-10-17 JSTに公式repositoryを確認しました。外部protocolの位置／data要件に従うadapterであり、resource limitと段階的統合はprojectの実装判断です。

Debug stackは3つのlayerを持ちます。

- Native debug metadata：macOSではDWARF、WindowsではCodeView／PDB。
- VM debug metadata：bytecode／source map、lexical scope、variable location、async task frame。
- Native frameとVM frameを統合し、mixed breakpointをsupportし、optimized／unavailable valueを正直に表示するDAP adapter。

Source spanとstable symbol／function identifierをすべてのlowering stageで維持します。Debuggabilityをbackend acceptance criteriaに含めます。

## Toolchain UX

名称が決まるまでは、仮の統合commandを1つ使用します。

```text
lang new       create a package
lang build     build incrementally
lang run       build and run, or execute a script
lang test      run unit, integration, and documentation tests
lang fmt       canonical formatting
lang lint      semantic and style diagnostics
lang doc       generate API documentation
lang lsp       start the language server
lang debug     start or connect the DAP adapter
lang package   manage dependencies and publication
lang toolchain install|list|default
```

Public registryより前に、package managerはmanifest、exact lockfile、checksum、offline mode、source replacement／mirror、feature resolution、target condition、reproducible build metadataを必要とします。

## セルフホスティング

1. Stage 0 Rust compilerが承認済みcore semantics全体を実装する。
2. 意図的に小さなself-hosting subsetを新言語で実装する。
3. Stage 0でstage 1をbuildし、stage 1でstage 2をbuildする。
4. Reproducible settingでstage 1とstage 2のoutputを比較する。
5. 複数platformでconformance／reproducibility checkに合格するまで、Rust compilerをbootstrap referenceとして残す。

Rust compilerとself-hosted compilerで意味論をforkさせません。両方が同じconformance suite、serialized test case、component ABI fixtureを使用します。

## 提案するRust workspace境界

実装が現れた段階でのみ、次のcrateを作成します。

```text
crates/
  syntax        lossless tree, parser, source map
  diagnostics   stable diagnostic model
  hir           resolution and typed high-level IR
  semantics     inference, traits, effects, ownership
  mir           typed mid-level IR and verification
  bytecode      format, encoder, decoder, verifier
  vm            embedded runtime and interpreter
  codegen       backend-neutral lowering interface
  codegen-clif  Cranelift backend
  runtime       native runtime services
  component     IDL, canonical ABI, bindings
  lsp           language server
  dap           debug adapter
  cli           toolchain command
```

Cyclic crate dependencyを避けます。Data-oriented IR crateはCLI、LSP、VM、backend crateへ依存してはいけません。

## 実装レビュー（2026-10-17）

現状の実装はMIR source mapをbytecodeとnative rangeで保持し、実際のCodeView parameter／result typeを記録し、NUL関数名とhost-import symbol衝突を拒否し、compiled function破棄までJIT executable mappingを所有します。狭いunsafe releaseはprivate code pointerとownerをborrowするinvokeに基づき、error pathでも未公開mappingを解放します。

Component validationは直接構築した不正SemVerを各boundaryで確認し、borrowed resultを拒否します。すべてのinterface capabilityをprovider／lock lookup前にpreflightします。両schemaのdecoderはstorage予約前に宣言collectionが残りpayloadへ収まるか確認します。これはartifact認証ではなく、将来loaderが実byteを検証するまでは選択digestは供給されたmetadataです。

Rust adapter cleanupはdestructor error表示／破棄、panic payload破棄からのpanicを封じ込め、独立cleanupを継続し、active call放棄時は両instanceをpoisonします。放棄時にはhost callbackなしでhost destructorを実行できず、hostが隔離valueをteardownする必要があります。再帰的panic payload破棄では2つ目のpayloadを意図的にleakしえます。Abort-mode panic、foreign unwind、直接native callbackはこのcontainmentの対象外です。

Response fileは排他的に作成・保持し、retryでは新しいpathを使用します。既存file／symlinkは上書きしません。Linker設定とnative callbackはtrusted host inputであり、script capabilityではありません。

情報源：lock済みCranelift JIT 0.136.3の[memory解放contract](https://docs.rs/cranelift-jit/0.136.3/cranelift_jit/struct.JITModule.html#method.free_memory)を2026-10-17にinstalled versioned sourceで確認（公開／更新日記載なし）。Microsoft [CodeView型定義](https://github.com/microsoft/microsoft-pdb/blob/master/include/cvinfo.h)は公開日表示のないliving sourceで、2026-10-17確認。Repository実装と回帰testは2026-10-17更新。

## 必須検証

- Parser／formatter round tripとmalformed-input recovery。
- Stable error codeを持つcompile-pass／compile-fail diagnostics。
- MIR／bytecode verifier test。
- Interpreter／native differential test。
- Cross-version ABI／package compatibility fixture。
- Parser、manifest、bytecode、debug data、component metadataへのfuzzing。
- 実用的な範囲でRust実装へのMiri／sanitizer run。
- Windows／macOS CIと代表的benchmark。
- Self-hostingを主張する前のbootstrap stage比較。
