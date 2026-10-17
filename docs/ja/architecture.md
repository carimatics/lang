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

受理済み[RFC 0002](rfcs/0002-prototype-control-flow.md)は `i64` parameter／result、wrapping addition、signed comparison、mutable local、`if`／`else`、`while`、明示的 `return`、左から右の評価順序を定義します。Join／back-edgeをまたぐvalueはtyped block argumentで運びます。受理済み[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)は字句規則、module／call、aggregate／pattern、portable bytecode、上限付きframe、script／REPL behaviorを追加します。Diagnostics、lossless parsing、source typing／lowering、portable artifact、上限付きVM実行、初期native scalar conformance、script、transactional REPLは実装済みです。Formatterとparser／module-query LSP prototypeも実装／検証済みです。

2026-10-17実装：`lang-syntax` はUTF-8 inputを検証し、不正encodingや先頭BOMにはsnapshotを作成しません。Input errorはraw byte範囲を保持します。Lexerは受け入れた各snapshotを、空白、comment、不正token、不正Unicode scalarも含む、連続した完全なtoken列として保持します。符号付きdecimalのoverflow、不正token、comment内も含むNUL、閉じていないblock comment、直接作成したsnapshotのBOM違反を報告します。Source byte、token、nested-comment depth、診断数の上限を超えると `SRC0011` でanalysisを終了します。Commentは反復処理で走査します。13件のlexer testが正確な範囲、復元、決定的な混合Unicode input、resource境界を検証します。

Parserはsyntax node、real-token参照、zero-widthなmissing-syntax leafをimmutableなarenaへ構築します。File／expression別のentry pointで受理済み文法全体を実装し、treeをsemantic HIRと分離します。Postfix field access、左結合の加算、連鎖しない比較、named call、record-headの曖昧性解消、pattern shapeをtreeへ明示します。Delimiterを考慮するrecoveryが外側の閉じbraceと後続宣言を保持します。`SRC0012` 診断は曖昧でないdelimiter挿入だけにmachine-applicable fixを付けます。Syntax node数2,000,000、tree／parser depth 256（内部再帰上限も兼ねる）、comma-list要素数256、診断数4,096、work 10,000,000を上限とします。Workはrecoveryを含むtoken indexing／consumption、node開始、missing-syntax挿入を課金します。Resource超過はerrorを返し、部分的な成功treeを返しません。復元、clone、dropは再帰的な所有nodeではなくarena参照を使います。18件のparser testがsyntax snapshot、全体文法fixtureとその全prefix、1,000件の決定的な不正input、正確なdepth／list境界、recovery、fix preview、source identityを検証します。`source_parser` fuzz targetはbyte ingestion、両parser entry point、復元、診断spanを検証します。Stable Rustのinstrumentationなしの10,000-run smokeは完了しましたが、coverage-guided／sanitizer evidenceを示すものではありません。Instrumented campaignはrepository rootで `cargo +nightly fuzz run source_parser` を別途実行します。後続の段階がsemantic resolution／typingをsource実行へ接続します。

2026-10-17実装：`lang-hir` はsyntax非依存のRFC 0002 function arenaを受け取り、local nameを解決し、analysisにerrorがない場合だけread-onlyなtyped HIRを返します。Parameterはimmutableで、宣言はinitializerの後からlexical block内だけで可視です。別branchを含むfunction全体での名前再利用を拒否します。Type checkingはinteger operand／value、boolean condition、到達可能な全exitの明示returnを強制します。`SRC0002`–`SRC0007` をprimary／related span付きで生成し、signed comparisonにも `i64` operand checkを適用します。到達不能statementもanalysisし、structured warningを返します。Primitive APIはstructured warning型を保持し、source program全体のcheckingは受理済み `SRC0019` を使います。全nodeにsource-snapshot byte span、localには表示名／宣言spanを保持します。Caller指定のwork／local／depth／diagnostic limit、内部再帰上限、arena参照検査、cycle検出、source／name-span検証でanalysis boundaryを保護します。この段階ではliteral decode／range診断、字句規則、module loading、source call、aggregate、serialized HIR契約を導入しません。

同じく2026-10-17実装：`lang-semantics::lowering` はこのHIRを既存typed scalar MIRへ変換し、明示work／value／block limitと最終MIR validationを適用します。Expressionを左から右へloweringし、変化したbranch valueをtyped block argumentでjoinし、可視mutable valueをloop-header parameterで運びます。Early returnで到達不能joinを作らず、到達不能statementからinstructionを生成しません。CFG構築後にsymbolic IDをMIRが要求する連続definition順へremapします。合成jumpを含む全生成instruction／terminatorで、snapshot byte spanを既存の1始まりMIR line／Unicode-scalar columnへ変換します。構造化inputのintegration testはVM／JIT arithmetic、両branch path、nested branch／loop state、early return、fuel超過、bytecode span保持、native JIT／AOT-object source mappingを扱います。後述の完全なsource loweringとbudget付きunit verifierがparser接続済みcall／aggregateを扱います。Legacy scalar validatorはtrusted-input境界を維持します。

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

Unit検証は反復処理のnominal依存走査、圧縮predecessor list、反復処理のreverse-postorder immediate dominatorを使います。構造上限だけでdataflow workも制限できるとは仮定せず、CFG／operand／dominatorの再訪問を課金します。共有 `Budget` はdecoder／verifierを通算計数し、既定でwork 10,000,000、要求bufferの累計128 MiB、要求vector要素の累計8,000,000をtemporary bufferも含めて制限します。解放してもbudgetは回復しません。Name byte／比較、source-line index、table要素の初期化、metadata／type／signature／member訪問、instruction、operand、edge、ancestor走査も課金します。Buffer割り当て前にchecked arithmeticとfallible reservationを使います。Caller所有のraw unit storageはverifierのallocation計数から除き、decoderは自身のbufferを同じbudgetへ課金する必要があります。15件のtestが上限と正確なlayout、span、owned制約、独立した経路除去oracleとの1,536件のSSA dominance照合を検証します。Rust object layoutはwire formatではなく、後述のportable codecがschema 1のbyte encodingを実装します。

### Source program lowering

Source生成unitは `lang_codegen_clif::unit` により既存native profileへ接続します。要求unit limitを再検証し、変換buffer／workへ課金し、symbolic registerを旧連続順へ再配置し、明示host IDを対応付け、immutableなbyte spanをline／scalar-column debug metadataへ変換します。部分的・複数fileにまたがるfunction mapは位置を黙って失わず拒否します。未対応signature／operationはbackend codeの割り当て・公開前に拒否し、新guest callとcopy aggregateはnative後続作業までVM側の責務です。全宣言host importをJIT compile前にregistryでpreflightします。元のnative compile／旧validation APIはtrusted-input境界を保持し、adapter budgetによるnative compiler時間／memory制限は主張しません。9件のsource conformance testは演算／CFGのVM・JIT一致、signature確認付き引数なし呼出し、bytecode／JIT／AOTでのspan、未対応case、typed import／owned identity、metadata／budget failureを扱います。Tier-1条件付きC harnessがparsed-source objectをdirect／response-file両modeでlink／実行します。2026-10-17のlocal evidenceはmacOS AArch64であり、同じWindows経路はCI実行を要します。

2026-10-17実装：`lang_semantics::program_lowering` はimmutableなtyped source HIRを検証済みcompilation unitへ変換します。Argument／payloadの左から右の評価、recordの記述順評価と宣言順格納、scrutineeの1回評価、順序付き副作用なしpattern test、case guard後のchecked enum projection、arm-local bindingを保持します。Exhaustivenessにより最終armを無条件にでき、汎用runtime match-failure経路は生成しません。定数trueの構造的branchでwarningのみのcovered armをverifier上到達可能に保ちます。Joinはtyped block argumentを使い、loop headerはaggregate／boolを含む可視mutable valueを運びます。Dead statementは命令を生成しません。合成jumpを含むfunction、全instruction、全terminatorにsemantic byte spanを保持し、fileを読まずsnapshot本文をコピーします。構築と最終検証はwork／allocation／element budgetとunit全体register／block／instruction countを共有します。8件のparsed-source testで契約と拒否境界を検証します。後述の実行testでsource VMと初期native scalar conformanceを完了します。

### Compilation-unit execution

2026-10-17実装のvirtual-source基盤：`modules::load_virtual` はhost指定root byteをfile読込と同じ通算module／source／token／node budgetとraw encoding検証でparseします。Temporary fileを作らずrootを再読込せず、child fileには明示的canonical source rootを要求します。`check_expression_function` は1つのreturnだけを持ちlocal／parameter／patternがなく、unit内から呼ばれないprivate root `__repl_eval_*` wrapperだけの結果型を推論します。他の宣言には明示result型検証を維持し、最終signatureは検証済みexpression factsから得ます。`lower_library` はscript entryの自動選択を止めます。8件のtestはvirtual UTF-8 snapshot、権限、nested module、上限、推論と不正wrapperの制約、通常typing、library entry policyを検証します。後述のtransactional sessionがこれらのAPIを使います。

2026-10-17実装：`lang_vm::unit` は検証済みportable programを明示 `Vec` frameで実行し、guest callにRustの再帰を使いません。各frameは全registerの最大cell数の検証済み合計を1つのflat bufferへ割り当て、offsetとparallel-copy schedulingはregister metadataとします。Recordはzero header、enumはcase tagと最大payloadまでのzero paddingを持ちます。Aggregate構築／projection、argument、return、CFG copyを課金済みstorage内で処理します。Parallel copyは上書き前にfan-outを処理し、cycleを1 cellずつ回転させます。Result storageは最終frame bufferを再利用します。Typed embedding valueは実行前に厳密なsize、bool、nonzero owned index、record header、enum tag／padding、nested memberを検証します。既存owned identityは論理resource indexの転送であり、新resource managerではありません。

安全な既定limitはfuel 1,000,000、host call 1,000、frame 256、live cell 1,000,000です。Embeddingは代替値やunbounded実行を明示設定できます。Interruption、deadline、instruction fuelの順に確認します。Instruction／terminatorはfuel 1を消費し、guest callは続いてcontrol、予定frame depth、live cellを割り当て・entry前に再確認します。拒否したcallのfuelは消費済みとし、未entry calleeをtraceへ含めません。初期frame／cell拒否はfuel 0です。Host callはcontrol／budgetを再確認し、trustedかつnon-preemptibleなcallbackへ入る直前に課金します。未使用を含む全宣言importをqualified nameから厳密なtyped registry登録へguest副作用前に解決します。Failureはguest frameを解放し、reason、counter、fault span、内側から順のcall siteを返します。先行host副作用は保持します。Trace自体の割り当ても失敗可能とし、失敗時は不完全であることを明示します。Limitは論理guest cellを数え、callback割り当てやnative実行は対象外です。

17件のsource→canonical wire→VM testはcall、2,000段recursion、mutual recursion、aggregate／pattern、評価順、frame解放、厳密なfailed-call／host計数、control優先順、deadline、import、source stackを検証します。7件のhand-built unit testはparallel-copy oracleとの324実行比較、canonical value failure、最大enum cell課金、owned identity、`InvalidVariantProjection` を含みます。Parsed scalar conformanceもこのVMと元のscalar VM・JITを比較します。Decoder fuzz targetは受理entryをfuel 128、host call 0、frame 8、cell 4,096で実行し、instrumentationなしの10,000-run smokeが成功しました。Coverage／sanitizer evidenceは主張しません。後述のCLI／REPLとeditor統合がsource向け成果を完成させます。

### Portable bytecode

2026-10-17実装：`lang_bytecode::unit` は検証済みMIR unitをregister programへ昇格し、受理済みRFC 0006のportable schema 1を実装します。明示的なlittle-endian codecが `LANGBC\0\0`、schema／flags／payload長、source／definition／import／function table、typed register、全12 operation、terminator、byte span、任意のentryを書き込みます。Global tableをcanonicalにsortし、対応する全type／call／import／span／entry IDを再配置します。Field／case／block／instruction順は保持します。Decodeはnoncanonical順序、未知version／flags／tag、不正UTF-8／bool、長さ／trailing-data不一致、不正typed unitをprogram返却前に拒否します。既存prototype／component formatは維持します。

Codecの既定artifact上限は64 MiBです。Fallible reservation前にcountと残入力の最小量を検証し、source／register／block／instructionの合計を通算します。Raw stringとoutput byteは共有128 MiBの要求byte budgetへ、structured vectorはさらに8,000,000要素のbudgetへ課金します。Decodeとverificationは1つの `Budget` を使います。Encodeは要求limitでcallerのprogramを再検証し、失敗可能なbudget付き比較でsortし、正確な出力長を割り当て前に計算し、2回目のpassで明示fieldを書き込みます。Verifier workに加え、readerのword／tag、最大1 KiBごとのUTF-8検証chunk、両passのwriter field、sort／index memberを課金します。11件のtestは固定88-byte golden、全opcode、global ID再配置、非連番register／CFG round trip、全切断、不正input、canonical reencode、通算budgetを含みます。`portable_bytecode_decoder` はraw byteとheader以降を変異させた有効payloadをfuzzし、compileとinstrumentationなしの10,000-run smokeが完了しました。Compilation-unit VMがこのartifactを利用します。このsmokeでinstrumented fuzz coverageは主張しません。

### 解決済みownership型の性質

2026-10-17実装：source bindingはRFC 0007のloan／shared／weak／arena prefixと有限の `core` 型群をcompilation単位のconstructor arenaへ解決します。構造的interningにより各annotationのspanとconstructor初出のspanを保持し、childはparentより前に置き、nominal参照の前方参照を許可します。未知core path、不正arity、user generic specializationには `SRC0024` を出します。この表現はin-memory HIRであり、serialized formatやABIを導入しません。

`lang_hir::types::analyze` はCopy／drop、推移的なloan／arena／allocatorの有無、private型の由来を計算します。反復inline dependency探索でindirectionのない再帰layoutを拒否し、上限付き単調worklistで再帰する間接graphへ性質を伝播します。Copy／dropはfieldとOption／Resultを通じて伝播し、owning indirectionは独自の性質を持ちながらstorage／privacy制約を伝播します。通常loan／allocator authorityの格納、result型のescape、arenaを含むshared／weak payload、未対応Map key、public signatureへのprivate型露出を拒否します。ID、source span、member shape、declaration／member／depth／work／diagnostic上限、解析中に要求したallocation byteの通算を検証します。このallocation会計はcaller所有のHIR／source snapshot、allocator bookkeeping／fragmentation、固定の緊急errorを除き、process memory limitではありません。

Source接続済みの11 testはconstructor identity／span、全core error型、間接再帰とinline循環、推移的storage／privacy error、不正foreign HIR、work／allocationの正確な境界、400定義のchain、独立した構造的Copy oracleと比較する200生成型を含みます。解決済みHIRのfuzz targetにも上限付き不正constructor graphを追加しました。型の性質の取得はbodyのmove／loan／region validityやlayoutを保証しません。両program-check entry pointは実行可能な `TypedProgram` の生成前に拡張profileを明示的に拒否します。Core constructor／operation checking、move／loan／region flow、ownership MIR、実行は未完了で、有限core callのbindingは後述します。

CLIとrevision管理付きtoolingはtype-limit診断とsource snapshotを保持し、2件のintegration testで編集後の復帰と実行用lowering前の拒否も検証します。全fmt／Clippy／test／doc baselineが成功しました（554 test）。Lock済みfuzz workspaceがcompileし、解決済みHIR targetのinstrumentationなし10,000-run smokeが完了しました。Instrumented coverageのevidenceではありません。

情報源：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、`hir`／`semantics` 実装、source test、fuzz target。2026-10-17 JST更新／確認。実装事実と上限の記録であり、追加の言語判断ではありません。

### 解決済みownership body

2026-10-17実装：bindingは通常borrow／dereference式、place assignment、字句的arena blockを解決済みHIRへ接続します。`PlaceId` は既存expression arenaを参照し、可視local root、field chain、明示dereferenceを許可します。Temporary valueとmodule itemは `SRC0021` で拒否し、未解決local rootには `SRC0002` を維持します。Read／exclusive loanの種別と全source spanを保持します。Arena blockはallocator式を解決した後に専用のcompiler-created ownerをbindし、block内だけで公開します。既存のfunction全体の名前一意性と予約root規則に従います。到達不能bodyでも名前解決します。

6件のsource testでplace shape／span、字句的arena identity／visibility、temporary拒否、到達不能codeの診断、既存expression／local／block／depth budget、constructed annotationがなくてもCopy専用checkerで拒否することを検証します。これは名前／place解決であり、mutability、ownership transfer、loan duration、allocator authority、region escape flow、cleanupの妥当性のevidenceではありません。`CheckError::OwnershipProfile` と防御的なMIR lowering errorで、未解決のsemantic義務が実行artifactへ進むのを防ぎます。Core callは後述するsignature bindingを使い、実行用typingで引き続き停止します。このsliceのfull baselineは成功し（560 test）、borrow／dereference付きforeign HIR inputを加えたlock済みfuzz workspaceもcompileしました。

情報源：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、解決済みHIR、binding／checking／lowering実装、test。2026-10-17 JST更新／確認。

### 有限core callのsignature

2026-10-17実装：`lang_hir::core` はRFC 0007が要求する全34 operationと、backend非依存の固定signature templateを識別します。Parameter順、具体type argumentの制約、valueとread／exclusive loan型、operation固有result error、内部arena-owner parameterを保持します。Result metadataは通常value、execution-instanceのallocator loan、引数ownerに結び付く即時match専用のprovisional loan、ownerに結び付くarena参照を区別します。Borrowを含むcore Option／Result templateを通常のuser storage型としてinternしません。

Source bindingは `::<...>` 構文を含む修飾済み `core`／`root::core` callを、具体type use、記述順の引数、callee／argument／call spanを持つ `CoreCall` へ解決します。未知operation、type argumentの過不足、user generic callには `SRC0024`、値arityの不一致には `SRC0014` を出します。具体type argumentは既存の上限付きnominal／constructor解決とexpression／work limitを使います。従来のgeneric-call `UnsupportedSyntax` guardは除去し、Copy専用checkerとMIR loweringは実行artifact生成前に引き続きcore callを拒否します。2件のcatalog testで全signatureを独立したRFC表の期待値と照合し、由来／制約metadataを検証します。追加3件のsource testと更新した到達不能名testで全operation、arity、root修飾、identity／span／順序、limitを検証します。全fmt／Clippy／test／doc baselineが成功し（565 test）、core-call inputを追加したlock済みforeign-HIR fuzz targetもcompileしました。

完了したのはoperation識別とarity／signature metadataです。Operand型の適合、callでのtype-argument storage／Map key／shared payload制約、推論borrow、result loan duration、core case／Unitの完全なtyping、runtime operation、allocator認可、公開allocator／FFI ABIは未実装です。Core value／pattern bindingは後述し、残る検査と実行pathはMilestone 2の未完了作業です。情報源：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、core／解決済みHIR、source binding／checking／lowering、test、fuzz target。2026-10-17 JST更新／確認。

### Core valueとpatternのbinding

2026-10-17実装：core catalogはOption／Resultと6つのerror familyの全22 caseを、正確なpayload templateと所属familyを含めて識別します。Source bindingはcore case identity、入れ子payload、括弧の有無、source path／value／pattern spanを、user nominal IDと区別した専用HIR nodeへ保持します。Zero-payload caseは括弧を省略し、payload付きcaseは正確なshapeを要求します。`core::Unit {}` は式とpatternで空のcore recordです。空payloadのpattern listは既存parserが拒否し、他のcore shape errorには `SRC0015` を出します。Caseのtype argumentは期待するannotation／parameter／result／field文脈またはmatch scrutineeから暗黙に得る方針を維持し、明示 `::<...>` case specializationには `SRC0024` を出します。

独立したcatalog test 1件で全family／case／payloadをRFC 0007と照合し、6件のsource testで全caseの式／pattern、入れ子error／sum、Unit、不正shape、user／core identity分離、pattern scope、expression／pattern／depth budget、現実的なallocator／Vec／Result／read-place binding fixtureを検証します。全572 test、fmt／Clippy／doc、lock済みfuzz compileが成功しました。Foreign-HIR fuzz targetにはcore Unit／caseと不正core patternを追加しました。これはbinding／shapeのみを保証し、expected-type推論、payload型の適合、exhaustiveness、move、loan／region、cleanup、MIR／VM／native実行、ownership wire supportは未完了です。Copy専用checkerと防御的lowering／coverage境界は実行artifact生成前にcore nodeを拒否します。情報源：受理済み[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)のshape規則、受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、core／解決済みHIR、binding／checking／lowering、test／fixture、fuzz target。2026-10-17 JST更新／確認。

### Ownership bodyの型解析

2026-10-17実装：`lang_hir::program::analyze_bodies` はRFC 0006のbody checkerを再利用し、読み取り専用の `BodyTypes` を生成します。期待型を注釈付きlocalの初期化／再代入、function result／argument、record field、入れ子case payloadへ渡します。GenericなSome／Ok／Errには曖昧でない期待family引数を要求し、monomorphic error caseは自身のfamilyを推論します。Unit、通常loan／dereference型、loan fieldの1段projection、place assignment型、call専用の推論borrow、exclusive loanからのshared reborrowを扱います。Non-copy match payloadのbindingにはread-loan型を与え、compiler既知のprovisional loan payloadはloan種別を維持します。Call factsは具体化したparameter型とresult由来を保持します。Core callでoperandの型適合と、推移的なstorable／Map key／shared payloadのtype-argument制約を検査します。

既存の反復coverage engineはcore Unitと全Option／Result／error caseを扱い、入れ子payload、loan projection、上限付きmissing-pattern witness、冗長armの警告に対応します。Opaque core valueはwildcard／bindingによるcoverageを許可します。`BodyTypes::types()` の型arenaは解決済みsourceの元prefixを維持し、provisional loan sumを含むintern済みcompiler生成型を追記します。`BodyTypes::source()` は元の解決済み入力を保持し、推論しただけの一時型をuser annotationへ混入させません。Source型の性質の解析とinstantiationはtype work／要求allocationの通算会計、既定65,536 instance上限、depth 64を共有します。Body／coverage workとarena数は別途制限し、`type_display_bytes` と `witness_bytes` は型診断とcoverage出力をそれぞれ既定65,536 byteへ制限します。対象は要求した型解析bufferであり、compiler／process全体のmemoryではありません。

共通frontendはconstructed typeを宣言するprogram、またはownership／core構文を使うprogramでこの段階を実行し、constructed型注釈のない構文も対象にします。Metadata検証後の上限付きscanはCopy checkerとwork budgetを共有します。Source診断はCLIとrevision管理付きtoolingへ公開します。型解析が成功すると後述の初期化／moveと字句的loan passへ進み、実行へのguardは維持します。Arena-ownerの型解析と入れ子reference cellの由来は後述のfollow-upで完成し、valueのarena brand flow、drop／layout elaboration、ownership MIR／VM／native実行、wire encodingは未完了です。`BodyTypes` から実行可能な `TypedProgram` は生成できず、focused testでtype compatibilityと次passによるuse-after-move programの拒否を区別します。型解析の成功はallocator／host権限を付与しません。

12件のbody testでsourceから全31非arena operation signatureと全22 core case、allocator／Vec／read fixture、context伝播、明示／推論loan型、入れ子exhaustiveness、独立したCopy checkerとの一致、source不変性／再解析、正確なbudget／出力上限、不正foreign ID／placeを検証します。2件のCLI／query testで診断とsnapshot／cache復帰を確認します。全fmt／Clippy／test／doc baselineが成功しました（586 test）。Lock済みforeign-HIR fuzz targetはcompileし、type-fact ID／source保持を確認するinstrumentationなし10,000-run smokeが完了しました。Instrumented coverageは主張しません。情報源：受理済み[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)、受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、HIR type／body／coverage実装、source／CLI／query test、fuzz target。2026-10-17 JST更新／確認。

### 字句的arena ownerの型

2026-10-17実装：typed arenaにfunction IDとlocal IDをkeyとする内部 `ArenaOwner(ArenaOwnerId)` constructorを追加しました。Arena statementはowner宣言前にallocator loanを検査し、ownerを字句的body内だけでbindし、owner scopeを離れた後にbodyのreturn flowを伝播します。この型はopaque、non-Copyで、teardownが必要かつ自身のarena brandを持ちます。解決済みsourceの型validatorはcompiler専用constructorを拒否し、sourceのannotation、parameter、storage、specializationには型名を書けません。解決済み入力は変更しません。

Arenaの3 signatureは、第1引数のowner identityから内部owner parameterを具体化し、明示loanと推論したread-loan aliasにも対応します。引数式は記述順に1回ずつ検査し、call factsには具体的なread／exclusive owner loanとresult由来を保持します。不正allocator／owner型には `SRC0014`／`SRC0023`、shared loanをexclusive parameterへ渡す場合にはcore／guest callとも `SRC0021` を出します。生成するowner／loan型は既存のtype instance／depth／work／要求allocation上限へ課金します。8件のsource／foreign-input testで1 fixture内の全3 operation、入れ子／function間のidentity分離、aliasと明示loan、不正authority／kind、解決済みowner型／public parameterの禁止、local IDのmetadata検証、正確なbudget、型解析だけでは実行できない境界を確認します。全594 test、fmt／Clippy／doc、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功しました。不正owner constructorとarena statementもfuzz inputに追加しました。

これは宣言siteのidentityと型のfactsです。Valueのregion由来、cross-region拒否、active loan conflict、動的activation／generation確認、allocationと逆順のteardown、allocator lifetime／認可、MIR／VM／native operationは未完了です。Testではcross-region readの型適合を意図的に維持しながら、実行guardを越えられないことを確認します。実行を有効化する前にregion flowで拒否する必要があります。情報源：受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)、受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、解決済み／typed HIRと型検証、source／foreign-input testとarena fixture、fuzz target。2026-10-17 JST更新／確認。

### Definite initializationとmove

2026-10-17実装：`lang_hir::moves::analyze` は `BodyTypes` を受け取り、各localを未宣言／初期化済み／move済み／到達経路の一部でmove済みとして追跡します。By-value argumentとrecord／case payloadは記述順にnon-Copy ownerをconsumeし、Copy値、loan forwarding、推論borrow argument、match scrutineeはownerを暗黙にconsumeしません。Whole localをmoveした後もmutable localを再初期化でき、`local = identity(local)` も許可します。Field replacementはRHS評価前とpublication前にdestinationを確認し、RHSでaggregateをmoveした後のpartial再構築を拒否します。Non-Copy fieldの抽出とloanからのmoveも拒否します。Fallible operationは成功／error両経路でowning inputをconsumeします。Use／初期化／back-edge failureには寄与したmove span付き `SRC0020`、禁止されたpartial／borrowed／arena-owner moveには `SRC0023` を出します。構造化violationはmessage textと独立にfunction／local identityとcategoryを保持します。

Branch／match joinはjoinへ到達する経路だけを含めます。到達不能statementとcoverageで冗長と証明したarmにmove effectはなく、名前／型は先の段階で検査します。Loop back-edgeはheaderの初期化categoryを復元する必要があり、0回iterationのexitにもcondition評価のeffectを残します。Coverageは冗長arm IDをこのpassとMIR loweringへ共有します。これにより、網羅的return armの後の冗長fallthrough armがmissing-return errorや不要なMIR continuationを作るCopy profileの問題も修正しました。VM／JIT regressionでscalar behaviorを確認します。

成功時はtyped input／sourceを保持した読み取り専用 `MoveCheckedBodies` とlocal access recordを生成します。保証するのは初期化／local consumption検査のみです。Active loan conflict、mutability、reborrow duration、region由来／escape、alias validity、drop／layout、ownership MIR／wire／runtime／native実行は別途必要です。Testではunusedな字句的loanがliveの間に初期化済みownerをmoveする例のmove段階での成功を意図的に維持し、後続loan passによる拒否と実行guardが閉じていることを確認します。共通frontendはguard前にmove failureとmove-limit診断をCLI／query snapshotへ公開します。Move factsから実行用programを生成するAPIはありません。

既定でfunction／local数、depth（256）、work（10,000,000）、diagnostic（4,096）、要求buffer通算（128 MiB）を制限します。Byte会計はstate snapshot、scope、access、function table、violation、diagnostic textを含み、caller所有のtyped data、allocator overhead／fragmentation、固定の緊急errorは除きます。15件のmove testでtransfer／再初期化、branch／match／loop edgeとreturn、評価順序、error経路、partial move、正確なbudget／snapshot identity、残るloan境界、成功／拒否の両方を含む独立boolean-state oracleと比較する80生成branch programを検証します。2件のCLI／query testと1件のVM／JIT regressionも追加しました。全612 test、fmt／Clippy／doc、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功しました。Foreign-HIR targetはmove／access factsも検査し、instrumented coverageは主張しません。情報源：受理済み[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)、受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、typed HIR／move／coverage／lowering実装、source／CLI／query／native test、fuzz target。2026-10-17 JST更新／確認。

### 字句的なwhole-owner loan

2026-10-17実装：`lang_hir::loans::analyze` は `MoveCheckedBodies` を受け取り、local／parameter／temporary ownerとinstance allocator authorityに対する単層の通常loanを検査します。名前付きloanは未使用でも上書きされてもenclosing blockの末尾までliveです。Callは記述順にargument loanを保持してcall終了時に解放し、入れ子call中も外側argument loanを維持します。Read loanはshared readを許可し、exclusive loanにはmutable local／place、compiler生成arena owner、または既存exclusive loanが必要です。Owning parameter bindingはimmutableのままです。Field pathはowner全体のconflict identityを保守的に共有します。Shared reborrowはparent readを許可し、exclusive reborrowはparent accessを凍結します。Loan変数の再代入はbindingへアクセスし、それ以前の字句的loanと子loanを保持します。Ownerやparentより長いscopeへのloan代入は拒否します。構造化された `SRC0021` のaccess／mutability／back-edgeと `SRC0022` のlifetime failureはfunction／owner／categoryと関連source spanを保持します。

Matchはnon-Copy scrutineeをborrowし、字句的field loanを生成します。Compiler既知の暫定core readはloanをbindするarmでだけowner loanを維持し、失敗／未使用armではbodyの前に解放します。そのためarena read loanは同じarena owner経由のallocation／replacementを排除します。Branch joinは到達するloan／由来の集合をunionし、return経路とcoverageが証明した冗長armを除外します。Loop back-edgeはheaderのactive loanとlocal reference状態を復元する必要があり、body localとcallだけのloanは比較前に終了します。これは保守的な字句的解析であり、path correlationや非字句的なlifetime短縮は行いません。Runtimeのborrow終了／cleanup順序やshared alias publicationの検証は扱いません。

成功時は外部から構築できない読み取り専用の `LoanCheckedBodies` を生成し、sourceを保持したloan identityとparent linkを持ちます。後述する入れ子reference cellの拡張は、`&&T`、`&mut &T` と関連expression型に対する初期の未完了gateを置き換えます。Value／callを介した推移的arena brand、動的shared alias、ownership layout／MIR／cleanup、runtime operation、bytecode、native実行は別途必要です。Scope番号は解析上の入れ子levelであり、runtime generationやwire identityではありません。CLI／query adapterはloan診断とresource failureを公開し、loan factsが成功しても実行guardを維持します。

既定でfunction（4,096）、local／生成loanまたはprojection fact（各1,000,000）、depth（256）、work（10,000,000）、diagnostic（4,096）、要求buffer通算（128 MiB）を制限します。会計はmetadata／dependency、reference／active-stateのcopy／join、scope、diagnostic textを含み、caller所有input、allocator overhead／fragmentation、固定の緊急errorを除きます。初期sliceの21件のsource testで字句的conflict／mutability、重複field、推論／明示call期間、reborrow／再代入、scope escape、branch／match／loop flow、暫定read、arena、source identity、正確なbudget、当初未完了だった入れ子referenceの境界を検証します。64生成permission sequenceが成功／拒否の両方を含む独立oracleと一致します。2件のCLI／query testで診断、上限、revision更新後の復帰、保持snapshotを検証します。この初期sliceでは全635 testのfmt／Clippy／doc baseline、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功し、instrumented coverageは主張しません。情報源：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、typed HIR／move／loan／frontend実装、source／CLI／query test、foreign-HIR fuzz target。2026-10-17 JST更新／確認。

### 入れ子のreference cellの由来

2026-10-17実装：loan解析はreference cellとその参照先を区別します。Parameter chainの各levelに独立したsymbolic identityを与え、local reference cellへのstoreは追跡する内容を更新し、destinationが複数cellを指し得る場合はweak updateを使います。各loanはowner parentと経由したcellの両方について、上限付きのソート済み推移的dependency集合を保持します。Shared cellを経由したloan loadはexclusive authorityを取得できず、有効accessは各reference layerの権限の共通部分になります。Dependencyは派生loanがliveの間のcell置換を防ぎ、owner／parent／cellのlifetimeを検査します。Destination評価ではinactive projection factを作るため、`**p = **p + 1` のようなRHS readを許可し、access／borrowのpublication時に経路全体を再検証します。Projectionはactive borrowではなく、同じfact数／work／allocation budgetを消費します。

推論argumentはreference cell自体のborrowとloan値のforwardingを区別します。Callはsignatureの権限の共通部分に従って到達可能な参照先を予約し、calleeへ入る前に凍結済みinner loanやargument間のaliasを検出します。Coreの暫定readはtemporary argument loanの終了後も存続するcell dependencyを保持し、未使用／error armでは解放します。RFC 0007の通常loanのescape禁止に従い、user functionはcaller所有のreference cellへloanをpublishできません（`SRC0022`）。有効な入れ子exclusive経路からscalarへ書き込むことは許可します。Callを越えたloan exportやlifetime関係の推論は導入しません。従来の入れ子referenceの未完了項目は削除し、有効な入れ子経路はchecked loan factを生成します。Valueのarena brand／escape flow、runtime alias検査、ownership MIR／cleanup／layout、serialization、実行は独立したgateとして残ります。

追加11件のsource testで多段read／write、cellの再代入／由来、派生loanによる凍結、推論／明示の入れ子call、到達可能alias、caller cellへのescape、暫定dependency、graph invariant、正確なresource上限を検証します。権限の共通部分を計算するoracleで、深さ1〜5の生成read／write program 124件を検証し、全layerがexclusiveの成功write 5件を含みます。各dependency IDはloanより前にあり、任意のowner parentを含みます。追加2件のCLI／query testでsource診断とrevision-safeなparameter spanを確認します。全648 test、fmt／Clippy／doc、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功しました。Foreign-HIR targetはsource factsに加えdependency IDも検証し、instrumented coverageは主張しません。情報源：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)のgrammarとloan／region規則、move／loan実装、source／CLI／query test、fuzz target。2026-10-17 JST更新／確認。

### Valueのarena brandと字句的escape

2026-10-17実装：`lang_hir::regions::analyze` は `LoanCheckedBodies` を受け取り、元のsourceと先行factsを保持し、expressionごとの保守的なbrand集合を記録します。Local brandはfunction／字句的owner identityを使います。Caller由来はopaqueで複数runtime regionを表し得るため、新しいlocal arenaへの権限にはなりません。通常のlocal値はstorage scopeが推移的な全brandに収まる間tokenを保持できます。構造化された `SRC0022` 診断はescapeするstoreと別arenaのread／allocation payload／replacementを拒否し、関連spanはarena／callerの由来を示します。Shared／weak storageとfunction resultも、先行type制約と並ぶ防御境界として検査します。宣言siteのbrandはruntime generation、stale token検証、teardownの証拠ではありません。

このpassはinline record fieldとsum payloadの由来を保持し、通常／入れ子reference cellのaliasを追跡し、field／loan storeやmutable collection projectionで実際のdestination ownerを更新します。Core Vec／Map operationは要素の可能なbrandを保守的にunionし、pop／removeでcollectionが空であることやbrandの消去を証明しません。Allocation／read／replacementでarena authorityの一致を検査し、同一regionの再帰payloadを許可します。Coreの暫定resultとpatternは参照先locationを保持します。Internしたabstract valueと安定したscratch rootにより、loopは後続iterationのeffectも含む保守的な固定点へ収束します。到達するbranch／match経路のみをjoinし、冗長armやdead statementにeffectはありません。成功時は読み取り専用 `RegionCheckedBodies` を構築し、実行への昇格APIは持ちません。

Guest callのbrand置換とmutation effectは後述のfollow-upで完成します。Region factsはownership MIR／layout／drop elaboration、VM／native／core実行、wire encoding、動的shared alias、arena runtime generation／teardownを検証しません。Frontendは証明済みregion failureとresource limitを公開した後も実行guardを維持します。

既定でfunction（4,096）、local／abstract value／place（各1,000,000）、depth（256）、work（10,000,000）、diagnostic（4,096）、要求buffer通算（128 MiB）を制限し、canonical value、path／root、state join／copy、call summary、expression facts、diagnostic textを会計に含めます。Input facts、allocator overhead／fragmentation、固定の緊急errorを除きます。初期15件のsource testで同一regionのcycle、由来、別regionのauthority／payload、opaque caller、field／pattern精度、入れ子alias、collection storage／projection、loop収束、dead path、正確なbudget／snapshot identity、当初未完了だったguest call境界を検証します。20生成lexical-store programが独立scope oracleと一致し、成功／拒否を10件ずつ含みます。2件のCLI／query testで診断／上限、revision更新後の復帰、保持arena spanを確認します。Loan regression 1件で `vec_get_mut` の暫定resultが具体化されたexclusive result kindを保持するよう修正し、parent accessを凍結しつつ要素writeを許可します。このsliceでは全666 test、fmt／Clippy／doc、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功し、instrumented coverageは主張しません。情報源：受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)、受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、typed HIR／loan／region／frontend実装、source／CLI／query test、foreign-HIR fuzz target。2026-10-17 JST更新／確認。

### Guest callのregion summary

2026-10-17実装：region解析はguest callのeffectへ実際のcaller brandを置換し、borrowed record field、sum payload、collection、入れ子reference cellを扱います。`CallerOrigin` はparameter identity、dereference数、型付きinline projectionを記録し、collection由来には上述の保守的なelement抽象化を維持します。Mutable parameterのsummaryは最終storage値と途中で起こり得るstoreを表します。全inputとreplacementを一つのcall前snapshotから準備してからoutputをpublishし、複数mutable parameter間のswapや依存を維持します。Calleeが後で値を消す場合も、store制約でcallerの実際のdestination owner lifetimeを検査します。Source診断はcall span、arena由来、寄与したcalleeのstore／call span一つを保持します。

再帰／相互再帰functionを二つの独立した最小固定点で扱います。第一段階は既知の通常returnがない状態から開始し、可能な通常returnとborrowed storageの最終内容を計算します。第二段階はその安定したoutputを使い、終了しないcallの前のstoreも含めて途中の全store制約を増やします。これにより、read-only再帰にwriteを捏造せず、通常returnしないことを証明したcallの後を解析せずに、再帰clearによる確実な消去を維持します。Return経路はlocal cleanupの前に記録し、借用した外部storageだけをexportします。Source診断は収束後に出力し、resource上限は計算中も常に有効です。Canonicalな型付きtemplate、caller input path、summary node／比較、繰り返す全解析visitを既存のwork／allocation／value上限で制限します。

Guest region callの未完了APIは削除しました。読み取り専用 `FunctionRegions` はcaller projectionとparameterの可能なmutationを公開し、成功した `RegionCheckedBodies` も実行可能MIRを構築できません。通常loanのcaller cellへのexport制約はloan passで維持し、user resultにarena brandを含めることも引き続き禁止します。Shared aliasのruntime検査、検証済みownership MIR、cleanup／layout、VM／nativeでのsource operation実行、ownership wire encoding、runtime arena generation／teardownとのsource統合は独立した要件です。

追加14件のsource testで直接／推移的call store、一時write後のclear、最終値のstrong update、inline field／sum精度、間接target、collection effect、call前swap、再帰clear、相互再帰write、通常returnしない再帰、再解析／source保持、正確なsummary budgetを検証します。20生成programは二つのguest callを経由し、独立lexical-scope oracleと一致して成功／拒否を10件ずつ含みます。追加2件のCLI／query testでcall／callee診断の由来とrevision更新後の復帰を検証します。全682 test、fmt／Clippy／doc、lock済みfuzz compile、instrumentationなし10,000-run HIR smokeが成功しました。Foreign-HIR targetはcaller projection型とmutation spanも検証し、instrumented coverageは主張しません。情報源：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)のloan／regionとcall規則、typed region／summary実装、source／CLI／query test、fuzz target。2026-10-17 JST更新／確認。

### In-memory ownership MIR types

2026-10-17実装：`lang-mir::unit::ownership` はfrontend／backendに依存せずRFC 0004／0007のnominal／constructed type graphを表現します。`lang-semantics::ownership_types::lower` は、変更不可のregion検査済みHIRを独立検証するgraphへ変換し、10種すべてのbuiltin、有限constructor、shared／exclusive loan、compiler生成のloan sum、lexical arena owner symbolを保持します。Definition／constructed indexとfunction／local owner symbolはsource identityを維持し、変更不可source snapshotと全definition／member／type-use spanをsource IDを付け替えてcopyします。変換のためにdisplay pathを読み直すことはありません。

Verifierはsource／name／span metadata、nominal／constructed参照、前方を参照しないconstructed argument、member上限、名前重複、空でないenum、正確なMap key分類、public／private公開、推移的storage制約を検査します。明示stackによる走査でinline再帰型を拒否し、indirectionを介する再帰を許可します。上限付き単調work queueでinline storageを介するCopy／dropと、全referentを介するloan／arena／allocator／owner／private属性を計算します。Core read-resultのOption／Resultはstack valueにloanを保持できますが、nominal／heap storageは通常loan、allocator authority、owner symbolを拒否します。Shared／weak storageは推移的なarena brandも拒否します。Source annotationとして記述できる型の制約はfrontendの責務です。

構築と検証はwork、要求allocation byte、elementの累積budgetを共有し、source／definition／member／constructed／depthにも上限を設けます。13件のfocused testでsource／MIR属性の一致、ownerとprovisional read型、snapshot／span保持と再変換、壊れたgraph／metadata／storage入力、正確なresource境界、400 definitionのchain、独立Copy／drop oracleで照合する200件の生成型graphを検証します。`ownership_mir_types` fuzz targetはlock済みmanifestでcompileし、instrumentationなし10,000-run smokeが成功しました。このsmokeはcoverage-guided fuzzのevidenceではありません。

`VerifiedTypes` はgraphのshape／属性と、後述の型同値を証明します。生のtable indexは論理参照であり、wire tagではありません。Owner symbolはruntime authorityを与えません。次の宣言／place stageで対応する宣言と型付きpathを検査しますが、実行可能instruction／CFG検証、initialization／move／loan／region遷移、layout、cleanup、core operation、VM／native lowering、ownership serializationは個別要件として残ります。実行可能な `VerifiedUnit` への変換は提供せず、既存Copy profileとbytecode schema 1は変更しません。この前提作業だけではHIR／MIR ownershipやarenaのoutcomeは完了しません。

出典：受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)、[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[MIR型verifier](../../crates/mir/src/unit/ownership.rs)、[source adapter](../../crates/semantics/src/ownership_types.rs)、対応test、fuzz target。更新／検証日：2026-10-17 JST。Graph storageと解析algorithmは実装上の選択であり、serialized format、heap ABI、performance測定の主張は追加しません。

### Ownership MIR declarations

2026-10-17実装：検証済みownership型は `canonical` と `equivalent` のqueryを提供します。構造的に同じconstructed entryは、検証済みargumentを介して最初の同値entryへ正規化します。Nominal definition、shared／exclusive loan kind、function／local arena owner symbolは別々のidentityを維持します。元のtable entryとspanは保持します。固定サイズでfallible reserveするopen-address tableでhash probeごとに課金し、key全体を比較するため、衝突で型を混同したりwork上限を回避したりできません。Hash値とRust layoutは実装詳細であり、serialized identityではありません。

`lang-mir::unit::ownership::declarations` はfunction／importの名前とspan、signature／localの型参照、parameterとlocalの正確な対応、parameter bindingの不変性、public／private公開、arena owner symbolに対応するfunction／local宣言とowner kind／typeを独立検証します。User resultは推移的loan、arena brand、allocator authority、hidden ownerを拒否します。Parameterは入れ子の通常loanとallocator loanを許可しますが、compiler既知のloan入りstack sumはuser signatureから除外します。通常localはarena ownerを装ったり、allocator authorityを値で格納したりできません。Local数は累積register quotaを使い、型／宣言の全検査で同じwork／allocation／element budgetを共有します。

`lang-semantics::ownership_types::lower_declarations` はregion検査済みsource function、import、型推論済み／型注釈付きlocalを変換し、function／local ID、binding kind、名前、snapshot、byte spanを保持します。`VerifiedDeclarations::check_call` と `check_import` は、明示的な引数型を検証済み同値判定で比較し、宣言されたresult型を返します。Borrow推論、move、liveness／region／capability遷移の検証、code実行は行いません。既存host importは正確に `(i64) -> i64` のままで、この宣言APIからallocator callbackや提案中のC boundaryを推論しません。

追加13件のfocused testで、独立に展開したkeyと比較する80件の生成型同値graph、同じhash bucketへ配置する32個の異なるkey、nominal／loan／ownerの区別、正確なbudget、不正なparameter／name／span／type／owner入力、禁止signature、scalar host call、source再変換、異なるfunctionで同じlocal indexを使うsource arenaを確認します。拡張fuzz targetは同値、宣言、再検証、引数検査を扱い、lock済みcompile／Clippyとinstrumentationなし10,000-run smokeが成功しました。Coverage-guidedの主張は行いません。これらのwrapperは実行できません。次のplace stageがsourceのrecord／dereference pathを扱いますが、対応するMilestone 2 outcomeの完了には、typed instruction／CFGとownership遷移、cleanup／layout、core／runtimeのVM／native統合、ownership wire encodingが必要です。

出典：受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)、[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[型同値](../../crates/mir/src/unit/ownership.rs)、[宣言verifier](../../crates/mir/src/unit/ownership/declarations.rs)、[source adapter](../../crates/semantics/src/ownership_types.rs)、test、fuzz target。更新／検証日：2026-10-17 JST。Interningとmetadataの方式は実装上の選択であり、新たな言語規則、公開heap／C ABI、serialization、performance測定は主張しません。

### Ownership MIR places

2026-10-17実装：`lang-mir::unit::ownership::places` はfunction内のlocal／value root、明示的な通常loan dereference、record field projectionを検証します。全typed value descriptor、place origin、root、field、projection span、導出したresult型を検証済み宣言／型と照合します。Shared／weak／arena tokenのraw dereferenceは許可せず、enum payloadをrecord fieldとして扱わず、MIR field projectionでloanを暗黙dereferenceしません。Value descriptorが提供するのは型とsource originであり、検証済みSSA定義や実行済みloadではありません。

静的access queryはCopy read、whole-value move、禁止されるnon-Copyのpartial／borrowed move、型が一致するreplacement、shared／exclusive borrowを区別します。Owning parameterとpattern bindingはimmutableです。Shared loanを一度通ると、その後に内側のexclusive loanがあってもpathはread-onlyのままです。Arena ownerはcore authorityのためのexclusive borrowを許可しますが、move／replacementを拒否します。Opaque allocator authorityも通常値としてmove／replaceできません。Value rootのbindingはimmutableですが、通常exclusive loanのreferentへのaccessは許可され得ます。これらのpermissionはloanが正しく生成されたことを前提とし、active loan／cell provenance、initialization、lifetime、region flow、borrow escape安全性は証明しません。

`lang-semantics::ownership_types::places::lower` はregion検査済みsourceのlocal／field／dereference式を正規化し、expression index、snapshot、byte spanを保持します。Sourceの自動field dereferenceは明示stepへ変換し、入れ子のdereferenceではshared／exclusiveの全pathを保持します。Temporary loan式はtyped value rootを使います。Verifierはprojection後の型を独立に導出し、検証済み同値判定でsource type descriptorと比較します。正規化とqueryはmetadataを生成し、guestのread／write／move／allocation、loan lifetime、cleanupには実行可能な遷移が必要です。

追加14 testで、独立permission-intersection oracleと比較する60通りのnested loan path、Copyとpartial／borrowed moveの区別、parameter／pattern／valueの不変性、authority操作、不正なroot／field／type／origin／span、正確な累積budget、source field／temporary／nested loan／arenaの正規化と再変換を確認します。構築と全type／declaration／place verifierでwork／allocation／element budgetを共有し、localとvalue descriptorには累積register上限、place数とprojection depthには個別上限を設けます。拡張 `ownership_mir_types` fuzz targetは不正place、再検証、access queryを含み、lock済みtargetのClippyとinstrumentationなし10,000-run smokeが成功しました。Coverage-guidedの主張は行いません。

`VerifiedPlaces` は実行可能MIR unitやmemory-access capabilityではありません。次のbody stageがtyped instruction／value定義とCFG／dominanceを扱いますが、initialization／move／loan／region dataflow、完全なsource／pattern lowering、cleanup／layout、core／runtime operation、VM／native実行、ownership serializationが必要です。ここで扱う通常source place profileだけではHIR／MIRやarenaのoutcomeは完了しません。出典：受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)、[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[place verifier](../../crates/mir/src/unit/ownership/places.rs)、[source normalizer](../../crates/semantics/src/ownership_types/places.rs)、test、fuzz target。更新／検証日：2026-10-17 JST。表現、interning、budget粒度は実装上の選択であり、新たな公開ABI、wire tag、performanceは主張しません。

### Typed ownership instructions and CFG

2026-10-17実装：`lang-mir::unit::ownership::bodies` はscalar／Unit value、Copy／move、place load、local初期化／replacement、明示drop、borrow生成／終了、arena入退場、guest／scalar host call、record、nominal／core variantを表現します。Instructionは0個、1個、複数のresult slotを定義します。Whole-variant unpackは全payloadを生成し、payloadのCopy／borrowとは区別します。Option／Result／errorの論理case ordinalは、このRust IR内で受理済みsource caseを表すものであり、ownership wire tagを追加しません。Sourceのnon-Copy matchは引き続きborrowし、内部whole-value unpackでsourceのpartial moveを許可しません。Shared／weak／container／text／byteのcore operation loweringは個別作業です。

構造verifierは検証済みtype／declaration／placeを使い、result／operand型、静的access制約、call／constructor arity、loan生成identity、arena owner／allocator分類、block parameter、型付きedge／return、単一定義、到達可能性、entry制約、定義の支配関係を検証します。Typed place originはloadではなくmetadataです。未使用origin slotは未定義でも構いませんが、実際のoperandとvalue-root依存には支配する定義が必要です。通常unitとownership unitは同じ上限付きdominator algorithmを共有します。必須loan endには支配する生成が必要です。条件付きendは一部pathでのみ生成されるloanを指定でき、有効状態の正しさは次のpassで検査します。

Payload操作には、同じvalueとcaseを調べる `IsCase` が生成したguard valueを渡します。そのguardを使うbranchには別々のtrue／false successorが必要で、true successorのpredecessorはそのbranchだけであり、payload accessを支配する必要があります。この保守的な構造証明でfalse／合流pathからの侵入を排除します。Variant検査は所有する不変SSA valueまたは通常shared loanを受け取ります。Payload lifetime、alias凍結、owner liveness、途中の無効化がないことは、ownership dataflowの責務です。

追加13 testで、独立した経路交差dominator oracleと比較する120個の生成CFG、non-Copy loop引数、result／definition／edgeの不正、local／value-root依存、borrow／arena命令、guard付きpayload Copy／borrow／unpack、22種すべてのcore case shape、record／call／import、正確な累積budget、source接続のowned-return組み立てfixtureを確認します。Fixtureは検査済みreturnを1件手動で組み立て、自動source-body loweringは主張しません。新しい `ownership_mir_bodies` test-input generatorはlock済みcompile／Clippyとinstrumentationなし10,000-run smokeを通過し、coverage-guidedの主張は行いません。Work／allocation／element budgetはmetadataとbody検証で累積し、block／instruction／loan数にも上限を設けます。

`TypedBodies` はinstruction型とCFG構造だけを証明し、実行可能なCopy profileの `VerifiedUnit` には変換できません。構造testは型が合う二重moveを受け入れますが、次のinitialization／consumption stageで拒否します。有効loanと推移的cell依存、間接alias／region／call effect、RHSのeffectをまたいで準備したassignment destination、cleanup／failure順序、layout、完全なsource／core lowering、等価なVM／native実行、受理済みownership serializationが必要です。このstageだけではMilestone 2のownership／arena／core outcomeは完了しません。

出典：受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)、[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[body表現／verifier](../../crates/mir/src/unit/ownership/bodies.rs)、[構造test](../../crates/mir/tests/ownership_bodies.rs)、source組み立てtest、fuzz target。更新／検証日：2026-10-17 JST。CFG／storage／guard表現は実装上の選択であり、公開cleanup／allocator／C ABI、serialized layout、performance測定は追加しません。

### Ownership MIR initialization and moves

2026-10-17実装：`lang-mir::unit::ownership::moves::analyze` はlocal storageとtyped SSA valueを、未初期化、初期化済み、一部pathでのみ初期化済みの3状態で独立追跡します。すべてのparameter localは初期化済みで開始し、他のstorageとvalue descriptorは未初期化で開始します。Read／borrowには確実なavailabilityが必要で、non-Copy transferはsourceを消費し、constructor／call operandは記述順に移します。Mutable local全体のreplacementでmove済みcellを再初期化でき、通常initializationはliveの可能性があるdestinationを拒否します。Ownerを持つSSA再定義は保持storageを暗黙に上書きできません。Copy引数／値のdropはsourceを維持し、local dropとarena exitはstorageを未初期化へ戻します。

Branch引数は選択されたedgeだけでtransferします。全引数を読み取り／消費してからdestination parameterを再束縛し、non-Copyのself-edge swapを許可しながら、同じowner入力の重複を拒否します。上限付き単調work queueでpath状態を合流します。DFS feedback edgeから非循環の初回iteration baselineを作り、一意のnatural-loop entryを持たないcycleも含め、feedback時に全localとdrop対象SSA slotの状態復元を要求します。Copy SSA temporaryはowner復元の対象ではなく、通常loanのactivation／provenanceは次のstageの責務です。Blockのentry状態、以前の消費の可能性があるspan、return時のcleanup obligationを読み取り専用factとして保持します。

`Budget::with_usage` で、変更不可のtyped stageが実際に使ったwork／allocation／element counterを再開してflow解析します。State matrixには個別の累積cell上限を設け、走査／copy／joinのworkと明示bufferを計上します。追加12 testで二重moveと診断span、未初期化storage／borrow、再初期化、branch joinと条件付きcleanup obligation、選択edge transfer、non-Copy swap、loop状態復元、irreducible cycle、arena activation、正確なresource境界、独立single-owner oracleと比較する160件の生成操作列を確認します。既存source owned-return組み立てfixtureもこのstageを通り、残るowner obligationがないことを確認します。拡張したlock済みbody fuzz targetはClippyとinstrumentationなし10,000-run smokeを通過しました。Coverage-guidedのevidenceではありません。

`MoveCheckedBodies` は引き続き実行できません。Return時にlive／maybe-live ownerをcleanup対象として報告できますが、このstageはobligationを記録し、未決定cleanup policyを選択・挿入しません。Dereferenceにはroot reference／cellの初期化を要求しますが、有効loan、推移的cell依存、参照先aliasの妥当性、escape／region／call effect、RHSのeffectをまたぐ準備済みdestination、allocator／cleanup／layout／runtime contractは未証明です。完全なsource／core loweringとVM／native／wire統合はMilestone 2作業として残ります。このstageだけではHIR／MIRやarenaのoutcomeは完了しません。

出典：受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)、[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[CFG状態解析](../../crates/mir/src/unit/ownership/moves.rs)、[test](../../crates/mir/tests/ownership_moves.rs)、累積budget API、source fixture、fuzz target。更新／検証日：2026-10-17 JST。Lattice、feedback edgeの方法、診断witnessは実装上の選択であり、新たなsource rule、公開cleanup ABI、artifact encoding、performance測定は主張しません。

### Prepared ownership assignment destinations

2026-10-17実装：ownership MIRは `PreparePlace` と `ReplacePrepared` を分離し、評価済みdestinationを使わない経路には `DiscardPrepared` を用意します。Function内の `PreparedPlaceId` で、source valueやloanとは独立したcapture siteを識別します。PrepareはRHSのeffectより前にdestination pathを評価し、publicationはreference cellを読み直さずcapture済みpathを使います。RHS評価中のexclusive loanはactivateしません。既存 `Replace` はpath解決とpublicationを一度に行うprimitiveとして残し、間にRHSのeffectが入るsource loweringには明示prepareを使います。

構造verifierは各captureのplace／site／spanを保持し、IDの一意性と範囲、累積capture上限を検査し、prepareがすべてのpublication／discardに先行して支配することを要求します。静的place mutability、authority制限、replacement型の互換性、resultが0件であることも検証します。Value-root依存はprepare時のSSA useであり、publicationの明示operandは評価済みRHSです。これは内部のin-memory contractであり、ownership artifact encodingではありません。

Move解析は各captureを独立したnon-Copy／drop不要のstate slotとして追跡します。Local全体のprepareは再初期化のため未初期化storageを許可し、projection／間接destinationは評価時にrootの初期化を要求します。Publication／discardでcaptureを一度だけ消費し、すべてのreturnは各pathのpending captureを処理し、loop feedbackはcapture状態を復元します。RHS loopの外でprepareしたdestinationは、そのloop中にpendingのまま変化せず保持できます。Publicationでlocal全体の初期化を復元し、間接publicationでは以前のroot reference cellを読み直しません。後続provenance stageはreplacementの前に、captureしたownerと推移的reference-cell依存を検証します。

追加10 testで不正capture ID／型／permission／arity、同一blockとbranch間の支配関係、二重消費、failure pathのdiscard、loopの再captureとpending RHS loop、間接root、正確な累積budget、replay、独立した経路消費oracleと比較する81個の生成diamondを確認します。追加1件のsource組み立てfixtureは検査済みsource metadataを保持し、literal RHSとpublicationの前にprepareを配置します。これは手動組み立てであり、自動source-body loweringや実行のevidenceではありません。拡張したlock済みbody fuzz targetは3操作を扱い、Clippyとinstrumentationなし10,000-run smokeを通り、coverage-guidedの主張は行いません。

出典：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)のassignment評価順序、[body verifier](../../crates/mir/src/unit/ownership/bodies.rs)、[move解析](../../crates/mir/src/unit/ownership/moves.rs)、[capture test](../../crates/mir/tests/ownership_prepared.rs)、[source fixture](../../crates/semantics/tests/ownership_types.rs)。更新／検証日：2026-10-17 JST。Capture表現は実装上の選択です。有効loan／alias／region／call、cleanup／allocator／layout contract、core／runtime lowering、VM／native等価性、ownership serializationは未証明であり、この前提作業だけでMilestone 2 outcomeは完了しません。

### Explicit MIR loan activation and end state

2026-10-17実装：`lang-mir::unit::ownership::loan_activity::analyze` は変更不可のmove検査済みbodyを受け取り、明示的な各 `LoanId` をinactive、active、一部の到達pathでのみactiveの3状態で独立追跡します。Placeとcase payloadのborrow生成でidentityをactivateします。Required endには確実なactivityが必要で、conditional endは冪等に条件付きactivityを解消します。通常returnのすべてのpathで明示loanを終了する必要があります。これはRFC 0007のlexical lifetimeに基づく内部MIRの義務であり、未決定のpanic／error cleanupやdestructor ABIを選択しません。

上限付き単調CFG伝播でblock-entryのactivity factを保持します。Move解析と同じ課金済みDFS feedback-edge走査を使い、irreducible cycleでもfeedbackと初回iteration baselineを比較し、外側loanがloop中にactiveのまま変化せず保持されることを許可し、不均衡なactivation／end状態を拒否します。Resource計上はmove stageの実際のwork／allocation／element使用量を再開します。Baselineと固定点matrixには個別の累積cell上限を設け、queue／scratch bufferを事前確保します。状態違反の診断は失敗operation／returnのspanと、関連する生成またはloop-header spanを保持します。

追加10 testでrequired／冪等／conditional end、joinと通常return、条件付き生成、均衡／不均衡loop、irreducible cycle、guard付きcase loan、範囲／replay、正確な累積budget、独立した具体pathのend oracleと比較する81個の生成diamondを確認します。新しい検査済みsource組み立てfixtureはnamed lexical loanの明示begin／read／end列を供給します。これはsource metadataの手動組み立てであり、自動body loweringではありません。Lock済みbody fuzz targetも成功したactivity解析を再検証し、Clippyとinstrumentationなし10,000-run smokeを通りました。Coverage-guidedのevidenceは主張しません。

`ActivityBodies` は実行できず、完全なloan-validity proofとは区別します。参照先解決、alias排他性やparent freezing、推移的reference-cell依存、暗黙caller-parameter loanの検証、終了済み参照のaccess拒否、古いloop参照が再activateした生成site identityを使わないことは検証しません。型付きuse-after-end fixtureはこのactivity stageを通り、後続provenance stageで拒否します。準備済みdestinationもRHS effect後のowner／依存の再検証が必要です。Region／call effect、cleanup／allocator／layout contract、完全なsource／core／runtime lowering、VM／native実行、ownership serializationはMilestone 2作業として残ります。

出典：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[activity解析](../../crates/mir/src/unit/ownership/loan_activity.rs)、[共通CFG走査](../../crates/mir/src/unit/ownership/flow.rs)、[test](../../crates/mir/tests/ownership_loan_activity.rs)、[source fixture](../../crates/semantics/tests/ownership_types.rs)。更新／検証日：2026-10-17 JST。Activity latticeとproof stage表現は実装上の選択であり、この前提作業だけでownership／arena／core outcomeは完了しません。

### Ownership MIR reference provenance and access

2026-10-17実装：`lang-mir::unit::ownership::loans::analyze` は変更不可のactivity検査済みbodyを受け取り、抽象的なlocal／SSA／caller所有cellに対して通常referenceを検証します。Loanを含むinline stack sumはpayload cellを分けて保持します。各reference identityは可能な参照先と推移的parent／cell依存を持ち、callerのloan chainには異なるcompiler専用identityとpointee cellを与えます。これは解析symbolであり、runtime address／generation／serialized IDではありません。

Read、write、move、破棄、動的SSA定義でwhole-owner loanの排他性を検査します。Shared readは共存でき、exclusive accessには正しいreference pathが必要で、live reborrowは競合するparent accessをfreezeします。依存loanがactiveのままparentを終了すると失敗します。Referenceの無効化はlocal／SSAのmove、initialization、replacement、sum構築／unpack、選択edgeでの同時引数transfer、条件付きjoinを通じて追跡します。Referenceはpathごとに異なるactive loanを指せますが、lifetime終了pathは個別の無効化factに記録します。Loopの生成siteを再activateしても古い格納referenceは復活しません。構造化errorはoperationと関連loanのspanを保持します。

Reference cellの借用と、その内容へのaccessは別操作として扱います。Nested dereferenceはcellへ到達するためのguardを保持し、guest callはすべての入れ子の引数referenceに対するshared／exclusive permissionの交差を検査します。Caller所有reference cellへ通常のlocal loanは格納できません。Prepare済みdestinationはRHS effectをまたいで評価済み参照先とguardを保持し、publicationはそのsnapshotを使って終了済み依存を拒否します。直接fieldへのpublicationでもRHS effect後にparent storageの初期化を再検査します。このcheckerはallocatorやhost capabilityを付与しません。

上限付き単調伝播は共通CFG走査を使い、irreducible cycleを含むfeedback edgeで、reference cell、支配するSSA／header value、active loan descriptor、capture済みdestinationの復元を検証します。均衡loopはfresh reference temporaryを定義でき、外側referenceをself-edge parameterで変化させず保持できます。密なbit matrixには累積word上限、topologyには累積node上限とpathごとのdepth上限を設けます。Work、要求buffer、temporary query set、elementはactivity stageの実際の計上を再開します。これはcapacity上限付き解析であり、performance測定ではありません。

追加17 testに、独立owner-interval oracleと比較する96 program、nested／caller cell、parent freezingと早期end、条件付きreference、payload view、古いloop alias、prepare済みdestination、均衡／irreducible loop、正確な累積上限／depth、replay、70 loanのreference／target bit境界を含みます。追加のregressionはparent storageのdrop後にprepare済み直接fieldをreplaceする操作を拒否します。既存3件の検査済みsource組み立てfixtureもprovenance検査を通り、以前activityだけを通っていたuse-after-end fixtureはこのstageで拒否します。拡張したlock済みbody fuzz targetはClippyとinstrumentationなし10,000-run smokeを通り、coverage-guidedの主張は行いません。

`LoanCheckedBodies` は実行できません。後述のcore operationと自動source adapterでtyped core callとRFC 0007の完全なbody loweringを提供します。独立MIRのarena brand／escapeとguest mutation effect、受理済みcleanup／allocator契約の実装、owned layout、VM／native runtime統合、ownership serializationはMilestone 2の独立要件として残ります。これらの独立した解析factだけでarena／coreや実行用ownership成果は完了しません。

出典：受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)、[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[provenance／access checker](../../crates/mir/src/unit/ownership/loans/mod.rs)、[test](../../crates/mir/tests/ownership_loans.rs)、[prepare済みfield regression](../../crates/mir/tests/ownership_prepared.rs)、[source組み立てfixture](../../crates/semantics/tests/ownership_types.rs)。更新／検証日：2026-10-17 JST。Cell topology、bit matrix、無効化表現は受理済みprofile内の実装上の選択です。

### Ownership MIR core-call contracts

2026-10-17実装：`lang_mir::unit::ownership::core` は受理済みRFC 0007の全34 operationを独立に規定します。`CoreCall` は具体type argumentのspanと記述したoperand順を保持します。Typed body検証は正確な引数／結果形状、storable／map-key／shared-payload制約、必須result-loan identity、arena operationの具体的な字句的owner authorityを確認します。Frontendの網羅的adapterはHIR型をMIR APIへ公開せずsource operation identityを対応付けます。HIR／MIR testはどちらかの実装から期待値を生成せず、独立した共通RFC signature fixtureを使います。

Move解析はoperationがfailureを返す場合もowning inputを消費します。Provenance解析は借用payloadの成功後もcontainer／owner loanを保持し、推移的parent依存を追跡し、map keyは結果と独立に解放します。Core readは論理的なprovisional loan予約を作り、実際の返却参照は成功payloadだけに存在します。失敗／未使用pathも予約を終了し、取得済みleaseがあれば解放します。生のprovisional sumは検査、一致case guard下のprojection、unpack、破棄だけを許可し、通常storage、forward、入れ子化、block引数を拒否します。Instance allocatorは独立した借用host cellであり、factがallocator capabilityを付与したりallocatorを呼んだりすることはありません。

読み取り専用のcall factはresult由来と具体arena ownerを記録します。上限付き固定点計算で `needs_instance_allocator()` を再帰guest callerへ伝播し、後続の実行adapterがeffect前にwhole-call preflightできるようにします。これは実行adapterへの要件であり、preflightやguest allocation engineの実装ではありません。11件のMIR testに、独立に具体化した102 signature case、全31 non-arena operationのtype／move／activity／provenance解析、arena new／read／replaceをつなぐfixture、guard付き結果のlifetime、再帰requirement、不正call、正確な累積上限を含みます。2件のsource testで全operation対応とspanを保持したsource由来allocator callの明示組み立てを確認し、完全な自動body loweringは未完了です。上限付きownership-body fuzzerもcore callとallocator-loan pathを生成します。

出典：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)、[MIR契約](../../crates/mir/src/unit/ownership/core/mod.rs)、[独立signature fixture](../../tests/fixtures/core-signatures.txt)、[MIR test](../../crates/mir/tests/ownership_core.rs)、[source adapter／test](../../crates/semantics/tests/ownership_types.rs)。更新／検証日：2026-10-17 JST。受理済みprofile内の解析を実装し、runtime ownership、arena brand、cleanup、allocator budget、C boundary、layout、version付きserializationはそれぞれ独立した完了gateを維持します。

### Automatic ownership body lowering

2026-10-17実装：`lang_semantics::ownership_types::bodies::lower` はregion検証済みsource bodyを受け取り、独立に検証するownership MIRを自動生成します。Type／declaration／place adapterと命令生成は累積work／allocation／element budgetを共有します。Source expression identityとspanを保持し、compiler生成のborrow valueとassignment placeには型と上限を持つmetadata slotを割り当てます。APIは実行できない `TypedBodies` を返し、手作業で命令を組み立てずに既存のmove／loan activity／provenance解析へ渡せます。

現在の経路はowned localとreturn、nominal／core constructor、記述順のrecord／call引数、明示／暗黙place borrow、exclusive-to-shared／exclusive-to-exclusive reborrow、guest／scalar host／core call、RHS前のdestination準備を行うassignment、if／while CFG、arena開始／終了をlowerします。Call一時loanはcall後に、字句的loanは正常scope／return edgeで終了します。条件付きlocal／SSA discardは、移動済み値を許容しながらauthority／active loanの検証を維持し、loop localのstorageを復元し、返却するSSA ownerを保持します。これらは正常flowのstorage／ownership factです。実初期化順に従う破棄、return cleanupの失敗、全exceptional edgeは、実行前にRFC 0008のcleanup elaborationを必要とします。

初期9件のsource testでowned returnの消費、recordの評価／並べ替え、入れ子／暗黙引数borrow、一時loan後の変更、branch／loopの移動済みかもしれないlocal、allocator要件、scalar host import、nominal／core sum、arena owner call／return時の終了、条件付きdiscardのactive loan拒否、source span／入力保持、正確な累積上限を確認します。Ownership-body fuzzerも条件付きdiscardを生成します。後述のmatch／arm拡張でRFC 0007のfrontend body経路を完成させます。独立MIRのarena brand／guest effect検証、owned layout、allocation authority、failure cleanup、serialization、VM／native実行は未完了であり、arena／core実行は独立した完了gateを維持します。

出典：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)、[自動lowerer](../../crates/semantics/src/ownership_types/bodies.rs)、[source統合test](../../crates/semantics/tests/ownership_types.rs)、[ownership命令](../../crates/mir/src/unit/ownership/bodies.rs)。更新／検証日：2026-10-17 JST。この変更では実行用／serialize用ownership profileを追加しません。

### Ownership match arms and provisional loans

2026-10-18 JST実装：自動loweringでRFC 0007の全pattern形式、入れ子match、借用scrutineeを扱います。Pattern全体を検査してからbindingを公開し、non-Copy scrutineeを保持し、不一致時は次のarmを試す前に一時pattern viewを終了します。Copy leafは借用view経由で読み、affineなshared／exclusive loan bindingには参照またはguard付きprovisional payload全体を渡します。Loan引数は両kindともcallerから暗黙copy／moveせずreborrowします。Provisional result全体のbindingは `SRC0020` で拒否するため、名前付きcaseをmatchしてpayloadをbindしてください。

Loanをbindする成功armはcore leaseと引数の祖先loanをbody終了まで保持します。失敗／未使用armはCopy bindingを保持しつつ、view、生result、provisional lease、一時parent loanをbody前に解放します。Map keyはcall後に独立して終了します。継続するarmは合流前にbinding scopeをcleanupし、loop feedbackは同じstorage／loan状態へ戻します。借用／入れ子参照scrutineeのpermissionは全dereferenceを通して保持します。

Ownership MIRはserialized Copy profileから独立した `bodies::Term` を持ち、`Unreachable` を追加します。網羅的matchのfallthroughは架空の結果を作らず、この終端経路を使います。Move解析は両exit kindのcleanup義務を記録し、loan activityはどちらのexitでも全loan終了を要求します。今後の実行adapterは `Unreachable` 到達時にinvariant faultを封じ込める必要があります。従来のCopy termは明示的に変換でき、この変更でownership wire encoding、実行用layout、schema変更は導入しません。

追加9件のsource testと完全なarena fixtureの拡張で、non-Copy record、可変core payload、未使用／Copy payloadの解放、map key、shared read、loop、借用／入れ子参照binding、生result拒否、正確な上限、fault exit義務を確認します。Test専用Copy evaluatorで、入れ子nominal／core patternとloop内scalar matchについて独立に定めた43件の返却値／fault結果を照合し、ownership runtime実行は実装しません。これらで受理済みsource profileのHIR／MIR owned／move／lexical borrow解析経路を完成させます。Driverの実行profileは引き続きCopyのみで、cleanup順序／失敗、owned layout／allocation、独立MIRのarena brand／effect、serialization、VM／native統合はそれぞれの要件として残ります。

出典：[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17 JST。[Match lowerer](../../crates/semantics/src/ownership_types/bodies/matching.rs)、[ownership制御フロー](../../crates/mir/src/unit/ownership/bodies/term.rs)、[source test](../../crates/semantics/tests/ownership_types.rs)、[独立oracle](../../crates/semantics/tests/support/ownership_pattern_oracle.rs)の更新／検証日：2026-10-18 JST。

### Cleanup runtimeとfailure reporting

実装日：2026-10-18 JST。`lang_runtime::cleanup`は上限付きentry frameで実際のruntime payloadを所有します。Slot metadata、lexical／call scope、failure bufferはentry前にfallibleな確保を行います。Intrusive listで宣言順とは独立して初期化成功順を記録します。Moveはsourceの義務を外し、再初期化は新しい義務を末尾に追加します。置換はframe stateを検査してincoming ownerをcommitしてから、切り離した旧値のtrusted cleanupを呼びます。生成adapterはcommit前にprepare済みdestination、loan、shared publicationも検証する必要があります。

Parameterは各activationの最外scopeに置きます。入れ子のguest callは同じentry frame内で別の宣言slotを使います。`return_to`とentryの`finish`はowning resultを終了scopeの外に保留し、ownerより先にcompiler既知のborrow registrationを終了して、cleanup成功後にだけresultを公開します。Cleanup失敗時は独立した義務を処理してから未公開resultも破棄します。明示drop、通常scope exit、failure exit、hostによる中断のいずれもcleanup呼び出し前に義務を消費し、失敗したcleanupは再試行しません。型付き`Err`は通常の返却値です。

Execution failureをprimaryとし、それがなければ最初のcleanup failureをprimaryにします。後続recordは事前確保buffer（既定で合計256 record）内で順序とsource spanを保持します。Overflow時はtruncation countを増やし、cleanupを続行します。Cleanup failureまたはcontain済みhost panicは`Instance`をpoisonして次のentryを拒否しますが、cleanupに成功したguest panicはpoisonしません。Trusted callbackには可変payloadを渡してpanicをcontainし、payload自体の破棄も別にcontainします。Callbackがpanicした場合は部分処理済みvalueを意図的にleakし、panic payloadの破棄中に再度panicした場合も再帰的にunwindせず隔離します。Reportはこの隔離数を記録し、cleanup中に診断文字列を確保しません。

Runtime testは、実際の初期化／再初期化順、条件分岐／loop slot、move元の除外、transactional replacement、入れ子call、返却値保留、全modeled execution failure区分、上限付きreportのoverflow、metadata確保失敗、panic containment、poison／再入、実際のshared owner解放を検証します。独立したlistベースoracleで256 stepの生成列を100件照合します。このengineはruntimeのcleanup遷移を提供しますが、source ownership実行にはproduction ownership／aggregate実行の統合、fallible guest allocatorのaccounting、VM／native adapterが引き続き必要です。Host Rustのmetadata確保はguest allocatorの権限ではなく、serialized formatの変更もありません。

出典：[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17。[Cleanup engine](../../crates/runtime/src/cleanup.rs)と[16 runtime test](../../crates/runtime/tests/cleanup.rs)の更新／検証日：2026-10-18 JST。

### 生成MIR cleanup range

実装／検証日：2026-10-18 JST。Ownershipの自動loweringは、生成するlexical／temporary cleanup範囲ごとに`Op::Cleanup`を出力します。Rangeはlocal、SSA value、loan registration、arena ownerの対象と、各actionのsource spanを保持します。返却するowning registerは終了rangeから除外します。個別の明示operationも利用できますが、生成するlocal cleanupは宣言逆順の実行listではなくなり、古いlocalの再初期化でruntimeのcleanup位置が変わります。

Typed verifierは対象のidentity／type、入れ子の各span、loan定義とregister dominance、provisional resultのdiscard、void resultのarity、累積work／allocation／element上限を検査します。Move解析は条件付きdiscardの遷移を適用します。Activity解析は指定registrationを終了し、provenance解析はowner storageのdiscard前にその終了を検証します。`CleanupStep::instruction`は静的解析用の展開です。Ownership実行consumerはその展開順を物理破棄順に使わず、rangeを一単位として処理する必要があります。既存のserialized Copy instruction schemaは変えず、新variantはin-memory ownership MIRに属します。

`lang_runtime::cleanup::MirLayout`はtyped functionのlocal、value、registrationを上限付きentry frameのcellへ対応付けます。Offsetを検査し、Copy valueにowning cellは作りません。`Frame::cleanup_mir`はcallback前にrange全体とcell kindを検査し、確保をせず生存中の義務を選択して、選択したregistrationを終了し、ownerを動的な初期化逆順で処理します。Vacant cellや重複targetで二重cleanupは起きません。選択外のcaller valueは生存を維持します。このmappingはguest allocatorの権限や実行用payload layoutを与えず、adapterにはmove／loan／regionのproofと実行preflightが引き続き必要です。

追加5 testはrangeのpreflight／選択、不正なMIR／runtime target、入れ子source span、verifierの厳密な予算境界、sourceからruntimeへの接続を検証します。Transfer／cleanupに限定したinterpreterで、source生成の全検査済みMIRと実際のshared resourceを使います。Localをmoveして再初期化すると、宣言逆順なら`1, 2, 3`となる値を`3, 1, 2`の順でcleanupします。Owning return成功時はshared valueを保持し、cleanup failureを注入した場合は独立したcleanup後に未公開resultも解放します。このtestは汎用ownership VMではありません。Production aggregate cleanupと実行adapter、owned layout／fallible allocation、production VM／native adapterは未完了です。Workspace全868 testとfmt／Clippy／docが成功し、fuzz targetのformat／locked Clippyと拡張body targetの非instrumented smoke 10,000回も成功しました。

出典：[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17。[MIR cleanup契約](../../crates/mir/src/unit/ownership/bodies.rs)、[source lowering](../../crates/semantics/src/ownership_types/bodies.rs)、[runtime mapping](../../crates/runtime/src/cleanup/mir.rs)、[source統合test](../../tests/ownership_cleanup.rs)、[runtime test](../../crates/runtime/tests/cleanup.rs)の更新／検証日：2026-10-18 JST。

### Record構築とfield cleanup順

実装／検証日：2026-10-18 JST。Ownershipの`Op::Record`はoperandをfieldの宣言順に保持し、宣言field ordinalをsource初期化順に並べた完全なpermutationの`initialization_order`を追加します。自動loweringはfieldを記述順に評価しながらこのpermutationを記録します。Typed verifierは累積work／allocation／element上限の下で、ordinalの不足／過剰、重複、範囲外を拒否します。空recordのpermutationは空です。位置引数によるvariant payloadのoperandは既に初期化順を保持しています。Serialized Copy schemaは変更せず、in-memory ownership命令を構築するconsumerはpermutationを指定する必要があります。

`lang_runtime::cleanup::RecordOrder`は検証済みownership record命令からだけmetadataを借り、逆順のfieldとinitializer spanを提供します。確保を行わない`drain`はeffect前にstorage arityを検査し、各owning fieldを消費してからcleanupへ渡し、`None`で表すCopy／vacant cellを飛ばします。Drainを途中でdropしても未訪問fieldはcallerの所有に残るため、生成cleanupは個別failureをcontain／報告し、最後まで処理する必要があります。`Frame::take_owner`は事前予約したaggregate／call storageへpayloadを移し、borrow registrationのexportを拒否します。生成adapterはownerを取り出す前に転送全体を検証し、その後のfallibleな処理より前にincomingの義務を引き受ける必要があります。

追加6 testは、3 fieldのsource記述順全6通り、宣言順の物理配置後のaggregate返却／caller cleanup、Copy fieldを含む入れ子recordと同じnominal typeの値ごとに異なる構築順、部分的field drainとiterator途中drop、空record、不正order metadata、initializer span、厳密な予算境界、owner exportのguardを検証します。Record／transferに限定したinterpreterで、source生成の全検査済みMIRを通して実際のshared valueを移し、test用aggregate storageはentry前に予約します。Aggregate構築と所有権移転を通した順序保持を実証しますが、汎用owned payload layoutやproduction ownership VMは提供しません。production実行adapterへのfield cleanup統合、guest allocatorのaccounting、VM／native conformanceは引き続き必要です。

出典：[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17。[Ownership record契約](../../crates/mir/src/unit/ownership/bodies.rs)、[source lowering](../../crates/semantics/src/ownership_types/bodies.rs)、[record順序runtime helper](../../crates/runtime/src/cleanup/mir.rs)、[record統合test](../../tests/record_cleanup.rs)、[runtime transfer test](../../crates/runtime/tests/cleanup.rs)の更新／検証日：2026-10-18 JST。

### Contextを共有するfield cleanup報告

実装／検証日：2026-10-18 JST。`lang_runtime::cleanup::Cleaner`は借用した`Context`を通して生成aggregate cleanupを支援します。`Instance::enter_with_context`と`enter_cleaner`は通常entryと同じmetadata／report bufferを事前確保し、既存2引数callbackとの互換性を維持します。`Context::field`は切り離したchildを消費してからtrusted cleanupを呼び、callbackとpayload破棄のpanicを個別にcontainして、全failureを親の上限付きreportへ記録します。Childのinitializer spanは一時的に親の位置を上書きし、その後元に戻ります。Contextは所有frame slotとexit reasonを保持します。`Context::fail`は兄弟cleanupを終了せずcontainer failureを報告できます。

`RecordOrder::cleanup_fields`は個別cleanupが失敗してもfield drainを最後まで処理します。戻り値はstorage shapeの妥当性を報告し、cleanup failureは共有contextへ残るため、frameはresult公開を拒否してinstanceをpoisonします。元のexecution failureをprimaryにし、順序付きfield failureを後続に置き、それがなければ最初のfield failureをprimaryにします。Buffer overflow時は省略record数を数えてcleanupを続行します。Fieldのpanic隔離もroot valueと同じ上限付きdisposal方針を使います。Contextはchild用の別report、診断文字列、fuel課金、guest allocationを追加しません。

生成adapterはnormal allocation／frame setup中に反復retirement用storageを予約する必要があります。Contextはowned payload shapeを推測せず、failure後にtraversal storageを予約しません。追加6 testは、report共有と親span復元、3 record上限bufferと元のfuel failureに対する300 field failure、callback／destructor／悪意あるpayloadのpanic、2万段のhost所有shapeを事前確保した1 itemのworklistで処理するケースを含みます。検証を限定したtest interpreterを通して、source生成の全検査済みrecord MIRと実際のshared ownerを接続します。事前予約した反復worklistで、構築順に従う入れ子field failureを各initializer span付きで報告し、cleanup失敗時は全field failureを保持しながら未公開record resultも破棄します。Workspace全880 test、fmt／Clippy、fuzz targetのformat／locked Clippyが成功しました。Source allocation／retirement予約、実行用owned layout、production VM／native adapterとconformanceは未完了です。

出典：[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17。[Cleanup context](../../crates/runtime/src/cleanup/context.rs)、[entry統合](../../crates/runtime/src/cleanup.rs)、[record helper](../../crates/runtime/src/cleanup/mir.rs)、[runtime test](../../crates/runtime/tests/cleanup.rs)、[source統合test](../../tests/record_cleanup.rs)の更新／検証日：2026-10-18 JST。

### Ownership MIRのfailure経路

実装／検証日：2026-10-18 JST。`lang_mir::unit::ownership::failure::elaborate`はloan検査済みownership bodyから明示的なstatus CFGを生成します。Entryはparameterを宣言順に受け入れる前に、権限／frame／cell／metadata上限をpreflightします。各guest命令とterminatorに1つのcontrol／fuel guardを置き、生成cleanup、registration終了、prepare済みdestinationのdiscardにはguest fuelを課金しません。命令resultは成功時だけ初期化し、status failureは共通cleanup経路へ進みます。型付きguest `Err`は通常の成功した実行結果です。Branchと同時edge transferは元のsource identity、register argument、spanを保持します。

Returnは末尾の生成cleanup rangeより前に未公開resultを保留し、captureをdiscardしてactivationを処理し、全cleanup成功後にだけ公開します。到達不能terminatorは生成tail cleanupより先にcontainするinvariant failureを記録するため、destructor failureが元のfaultを置き換えません。共通failure経路は原因の記録／保持、prepare済みdestinationのdiscard、active registration／ownerのcleanup、未公開resultのdiscard、failure伝播を明示します。`Cause::Reported`はcallee／cleanupのreportを維持し、reportがないのに既存reportを主張した場合はinvariant failureになります。Interrupt／deadline、fuel／host-call／frame／cell／allocation上限、counter overflow、host failure／panic、invariantの区分を維持します。

Runtimeの`Frame`には対応する段階的exit操作を追加します。`stage_return`、`begin_failure`、`retire_exit`、`discard_result`、callerへの公開／伝播、entryの完了です。Retirementは冪等で、消費済み義務を再試行しません。Entry全体のabortやhostによる中断でも、calleeのresultをcaller ownerより先に処理します。Stagingはcaller storageを拒否し、pending exit中は新scopeを拒否します。既存のentry一括完了も互換性を維持し、`ExecutionFailure`は共有MIRのdomain typeへ移してruntimeからのre-exportを維持します。

共通source解析はこのCFGを`OwnershipProgram::failure_flow`に保持し、`mir()`は引き続き検査済みsource bodyを返します。構築は累積work／allocation／element課金を再開し、合計node上限を追加します。Resource failureは元のroot source spanを持つ`SRC0011`を維持し、Copyのみの検査にはownership limitを適用しません。生成graphはprivateな検証済みin-memory dataであり、serialized ownership formatや実行用payload／ABI定義ではありません。

追加11 testでは、検証を限定したtransfer backendと実際のshared ownerを生成CFGへ接続します。Fuelゼロ／枯渇と厳密な課金、返却値保留、cleanup failure、旧値／incoming ownerを保持する置換失敗、teardown前のcapture discard、caller引数を消費しないentry拒否、branch／loop、到達不能経路のinvariant優先、入れ子caller伝播、staging中断、厳密な予算と決定的replayを検証します。Body fuzz targetもelaboration成功時のreplayとtarget上限検査を追加します。Production owned layout、source allocation／retirement予約、全opcodeの実行、VM／native adapterとconformanceは引き続き必要です。

検証：workspace全891 testと必須fmt／Clippy／doc baselineが成功しました。Fuzz targetのformat／全bin locked Clippyと、拡張body targetの非instrumented smoke 10,000回も成功しました。

出典：[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)と[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17。[Failure elaboration](../../crates/mir/src/unit/ownership/failure.rs)、[共通解析](../../crates/semantics/src/analysis.rs)、[runtime exit](../../crates/runtime/src/cleanup.rs)、[CFG統合test](../../tests/failure_flow.rs)、[runtime test](../../crates/runtime/tests/cleanup.rs)の更新／検証日：2026-10-18 JST。

### Fallibleなinstance allocator基盤

実装／検証日：2026-10-18 JST。`lang_runtime::allocation::Allocator<P>`は、明示的なtrusted providerを一意なallocator identityへ結び付けます。Entry adapterはguest実行やcallback前に権限の欠落／失効を拒否できます。各live handleはallocator identity、wrapしないgeneration、元の正確なsize／alignmentを保持し、別allocator、解放済み、再利用済みhandleを拒否します。Live allocationがある間の権限取り消しは`InUse`を返して権限を維持します。Teardownはproviderを破棄する前に残存storageを解放します。

`Request`はcallback前にi64／host sizeへの表現可能性、要素sizeの乗算、最大4096の2の冪alignmentを検査します。Limitは個別request、live／cumulative requested bytes、live allocation数、alignmentを別々に制限します。既定値は個別request／liveが64 MiB、cumulativeが256 MiB、live allocationが65,536件です。Locator slotとfree listはtrustedなinstance setup中にfallibleに事前確保し、guestのallocation／releaseは表を拡張せず定数時間でslotを管理します。Layout adapterには引き続きtype／regionの検証と、guest制御のdata／header／retirement allocationすべての明示requestが必要です。`check`はdata／metadataを合わせたplanを課金せずpreflightし、後続request失敗時のrollbackはadapterが担当します。

認可済みrequestはprovider呼び出し前にlive／count予算を予約します。OOMはlive／countを返金しますが、cumulative requested bytesと試行数は維持します。Quota拒否ではcallbackも課金も行いません。Zero-size requestもproviderを呼ばずにidentity、generation、allocation数のaccountingを保持します。確保成功時はpeak requested storage／countを更新します。統計が示すのはrequested storageであり、hostのpadding、fragmentation、process RSSではありません。短すぎる／alignment不正のprovider regionは一度だけ返却して公開前に拒否し、foreign pointerのvalidity、uniqueness、lifetimeは引き続きtrusted preconditionです。

`resize`は元のproviderでreplacement storageを確保／copyし、旧値と新値が同時に生存する予算を要求して、新handleを公開してから元の正確なrequestで旧storageを解放します。失敗時は元のhandle／dataを保持します。別allocatorへのcopyは明示操作です。Safe Rustの`System` providerは余裕を加えたbyte vectorをfallibleに確保し、連続したaligned viewを公開します。Unsafe blockやguest経路でのinfallibleなmetadata確保は追加しません。汎用region／provider interfaceは内部runtime基盤であり、生成するraw C allocator bridgeやserialized handle formatではありません。

追加16 testは、alignment 4096までのaligned／disjoint storage、zero-sizeの権限、全quotaとarithmetic／counter／generation overflow、別allocator／古いhandle、決定的OOMと不正provider storage、metadataを合わせたpreflight／rollback、失敗時に元の値を維持するgrow、明示的allocator clone、元のpointer／size／alignmentによる正確な解放、provider lifetimeを検証します。独立したledgerは、OOM注入とslot再利用を含む256 stepのallocation／release生成列を100件照合します。Production owned payload layout、source core operation adapter、shared／arenaのallocation変換、retirement予約、VM／native実行、C callbackは引き続き未完了です。

検証：workspace全907 testと必須fmt／Clippy／doc baselineが成功しました。Fuzz targetのformatと全bin locked Clippyも成功しました。

出典：[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17。[Allocator基盤](../../crates/runtime/src/allocation.rs)と[allocator test](../../crates/runtime/tests/allocation.rs)の更新／検証日：2026-10-18 JST。

### Managed owned layoutと予約済みretirement storage

実装／検証日：2026-10-18 JST。`lang_runtime::layout::Layouts::compile`は、独立に検証済みのownership型からprivateなmanaged実行用layoutとdrop descriptorを生成します。依存関係の反復走査はinlineのrecord／enum／Option／Result構造を辿り、再帰的なcollection／shared型も含め、owning／reference indirectionで止まります。Size、padding、alignment、metadata、workの算術を検査し、表現不能なlayoutをguest entry前に拒否します。CompileはMIRの累積work／allocation／element予算を再開します。Descriptorはfield／type spanを保持し、frontendへbackend／platform型を持ち込みません。

Managed scalarは明示sizeを使い、recordは宣言順のoffsetを保持し、enumはtagと最大のaligned active payloadを予約します。Core Copy errorは実際のpayload storageを含み、UTF-8 errorはi64 offsetを、allocationを含むerrorはallocation-error tagを保持します。Indirect valueはprivateな32-byte logical tokenを使います。Token layoutはpointer、allocator、liveness、loan、regionの権限を与えません。これらはruntime内部のlayoutであり、C／native calling conventionやownership artifact encodingではありません。

`Reservation`はpayload storageと、inline aggregateすべての初期化順／span状態および上限付き反復work領域をまとめてpreflightし、fallibleに確保します。Metadataはpayload公開前に確保し、後続dataのOOM時はmetadataを解放しつつcumulative request課金を維持します。両regionを明示的に初期化します。入れ子aggregateの状態範囲は重ならず、variantは最大caseの必要量を予約し、Copy fieldにはdrop workを作りません。必要storageは通常構築中にinstance allocatorへ課金し、execution failure後に初めて要求することはありません。

Opaque locationはallocation generationに結び付きます。`initialize_from_mir`は検査済みrecord／variant命令から構築順とinitializer spanを直接取り込み、同一の検証済み型グラフとresult typeの一致を要求します。Iteratorを使い、guest経路で一時vectorを作りません。Active enum caseをorder状態へ結び付け、未初期化の入れ子aggregate、不正case、重複／範囲外order、不正span、別allocationのlocationをretirement前に拒否します。この構築helperによってcallerのmove／loan／regionやownership transferのproofを省略できるわけではありません。

`begin_retirement`は義務を1つも返す前にactive shape全体を検査します。予約済みLIFO work領域でfieldを実際の初期化逆順に辿り、Copy holeを飛ばし、initializer spanを持つindirect tokenを返します。各leafはcallback前に消費し、明示的な完了確認によってleaf処理中の次の走査やstorage解放を防ぎます。Retirement開始後の変更も拒否します。`Reservation::cleanup`は切り離したtoken bytesをentryの共有`Context`へ渡し、独立したfailure／panicをcontainして報告し、走査を続け、全drain後にpayload／order／work storageを解放します。対応するowning serviceはtokenを検証／消費し、indirect descendantも事前予約済みmetadataで処理する必要があります。

追加10 testは、有限core型とerror payload、物理offset／alignment、data／retirementを合わせた予算と決定的OOMのrollback、実際のshared ownerを使うsource record全6構築順、入れ子record／variantの状態とCopy hole、enum caseの変更／不正値、別allocation／再利用前の構築location、厳密な累積compiler予算とreplay、指数的に展開するlayoutのoverflow、cleanup中に確保しない1,024段のinline shapeを検証します。共有Contextとの統合では元のfuel failureを維持し、各leafのcleanup failureまたはcontainしたpanicをsource順に報告し、実際のownerを一度だけ解放して両予約regionを解放します。Ownership typeのfuzz targetも、決定的layout replay、fieldの範囲／alignment、metadataの範囲を検査します。

Fixtureのtokenはhost test registryで実際のshared ownerへ解決します。Productionのtoken／service adapter、動的collection／shared／arenaのpayloadとretirement統合、全owning opcodeの実行、VM／native loweringとconformanceは引き続き必要です。今回のruntime storage／descriptor実装によってdeterministic dropやsource core-library成果が完了したとはしません。

検証：workspace全917 testと必須fmt／Clippy／doc baselineが成功しました。Fuzz targetのformat／全bin locked Clippyと、拡張ownership type targetの非instrumented smoke 10,000回も成功しました。このsmokeはsanitizerやcoverage-guided fuzzingのevidenceではありません。

出典：[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)と[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17。[Layout compiler](../../crates/runtime/src/layout.rs)、[storage予約／retirement](../../crates/runtime/src/layout/storage.rs)、[source／runtime統合test](../../tests/owned_layout.rs)、[ownership type fuzz target](../../fuzz/fuzz_targets/ownership_mir_types.rs)の更新／検証日：2026-10-18 JST。

### Managedな有限core value

実装／検証日：2026-10-18 JST。Managed reservationに、i64、Bool、Unit、有限core error全6系統を追加確保なしで型検査して読み書きする `read_immediate`／`write_immediate` を追加しました。Errorのcase順はsource MIRに一致し、allocationを含むerrorは入れ子のAllocErrorを、Utf8Errorはi64 payload全体を保持します。読み出しではBoolの非canonical byte、不正な外側error tag、不正な入れ子allocation tagを拒否します。書き込みは変更前に型とbyte範囲全体を検証し、paddingと非active error payloadを初期化し、型不一致時はstorageを保持します。Record末尾ちょうどにあるsize 0のUnit fieldも扱えます。

`case` はOption／Result／nominal／errorのdiscriminantを検証し、`Immediate::payload` はCopyなerror payloadだけを返します。`prepare_case` は未公開variant destinationのinline data全体と構築metadataをclearし、provider allocationなしで検査済みcaseを選択します。その後adapterがpayloadを移譲し、実際の初期化順を記録します。未構築のowning aggregateはretirement preflightで拒否し、構築済みResult／OptionはactiveなBytes tokenだけを本番buffer serviceでretireします。Locationは構造上のoffsetであり、読み出し、移譲、再利用はsourceのmove／loan／region検査と実行ledgerで許可する必要があります。再利用先をprepareする前には元のownerをretireします。

Runtime 5 testで、全core error caseと入れ子allocation payloadの明示vector、i64境界値、Boolの不正byte全254値、size 0のfield境界、別allocationのlocationとretirement状態、入れ子sum往復128回、不正tag、拒否時の書き込み保持、不変のallocation会計、active Bytesの正確な1回のcleanupを検証します。検査済みsourceとの統合testでは、入れ子Result／Option／BufferError／AllocErrorのcaseを生成MIRと直接照合します。このprivate managed codecはserialized artifactやC ABIではなく、単独でownership VM／native実行を有効にするものではありません。

出典：[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)と[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17。[Managed value access](../../crates/runtime/src/layout/storage/value.rs)、[runtime test](../../crates/runtime/tests/managed_values.rs)、[検査済みsource統合](../../tests/owned_layout.rs)の更新／検証日：2026-10-18 JST。

### Allocatorを使うBytes／Textとtoken cleanup

実装／検証日：2026-10-18 JST。`lang_runtime::buffer`は明示的instance allocatorを通してRFC 0007のBytes／Text runtime operationを提供します。Bufferの24-byte length／capacity／kind headerとpayload capacityをalignment 8の1つのfallible allocationへ置き、headerとcapacityの両方をrequested storageへ計上します。空の値も実際のheader allocationとallocator権限を保持します。Lengthは非負i64に収め、負／範囲外indexを拒否し、byte変更は0..=255以外の整数を切り詰めや部分変更なしで拒否します。

`Bytes::take_text`はUTF-8を検証し、domainの成功／失敗の両方でBytesを消費します。最初の不正sequenceのbyte offsetを報告し、成功時は確保／copyなしで同じstorage／allocator／generationを移譲します。不正入力は一度だけ解放します。Text lengthはbyte数、等価性は正確なUTF-8 byte列で判定し、連結時に正規化しません。明示的host byte入力には`from_slice`を使い、source literal構文は変更しません。回復可能なallocation／bounds／byte／UTF-8 resultと、allocator権限、古いhandle、storageのfaultを区別します。Runtime preflight拒否時は、外側のfailure cleanupが処理できるようownerを保持します。

Pushは元のallocatorで幾何的に増やすreplacement capacityをprepareし、copy／公開前に旧値と新値が同時に生存するrequested storageを予約します。失敗時は元のhandle、length、bytesを維持し、cumulativeの試行課金も維持します。Clone、TextからBytesへのcopy、連結は独立ownerを作ります。同一instance内のmethodは一時guest vectorなしで検査済み範囲をcopyし、明示的な別allocator向けmethodはsource ownerを検証してtarget権限へ課金します。その後のgrow／releaseでもresult固有のallocatorを維持します。

Privateな32-byte managed tokenは、検査するallocator／slot／generation identityとbuffer kindを保持します。変換時はreserved bit、元のrequest size／alignment、headerの範囲とkindを検査し、Text tokenの受け入れではUTF-8も検査します。Preflight成功後にだけtoken bytesを消費し、cleanupはtrusted deallocation前に義務を消費します。これはruntime内部dataであり、artifact encoding、sourceのownership権限、foreign pointer表現ではありません。Move／loan／type proofは引き続き必要で、token bytesのcopyでは代替できません。Rust owner wrapperも暗黙のRust破棄ではなく、entry ledgerによる明示cleanupを使います。

`buffer::cleanup_token`は検査済みBytes／Text leafをinstance allocatorで直接解決します。予約済みaggregate cleanup engineは、従来のshared-owner test registryを使わず実際のbufferを処理できるようになりました。Source生成record／enum layoutは実際のinitializer順／spanとCopy fieldを保持し、共有Contextを使って処理を続け、親のpayload／retirement regionより先にbuffer storageを解放します。Aggregate storageのpreflight失敗時は、評価済みtemporaryを生成failure cleanup経路が処理できる状態に保ちます。

追加13 testは、全buffer operation、不正index／byte、空の値の権限、UTF-8境界と消費するerror、同一storageの移譲、growのOOM／quota時の保持、正確なrequested-byte課金、別allocatorへの明示clone／変換／連結、tokenのkind／別allocator／reserved bit／古いhandleの拒否、実際のsource record／enum cleanupを検証します。独立byte-vector／UTF-8 oracleで128 stepのedit生成列を100件照合します。新しい`core_buffers` fuzz targetは上限付きoperation列、allocation-limitの結果、byte／UTF-8 oracle、破損tokenの拒否を検査します。

検証：workspace全930 testと必須fmt／Clippy／doc baselineが成功しました。Fuzz targetのformat／全bin locked Clippyと、`core_buffers`の非instrumented smoke 10,000回も成功しました。Smokeはsanitizerやcoverage-guidedのevidenceではありません。

Production ownership VM／native dispatch、managedなUnit／Option／Resultのmarshalling、動的Vec／Map／shared／arenaのpayloadとretirement adapter、serialized ownership artifact、cross-mode conformanceは引き続き必要です。既存の`lang run`／REPL実行境界は変えず、このruntime serviceだけでText／Bytes source-operation成果が完了したとはしません。

出典：[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)と[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17。[Buffer service](../../crates/runtime/src/buffer.rs)、[allocator token／range helper](../../crates/runtime/src/allocation.rs)、[runtime test](../../crates/runtime/tests/buffer.rs)、[source cleanup統合](../../tests/buffer_cleanup.rs)、[buffer fuzz target](../../fuzz/fuzz_targets/core_buffers.rs)の更新／検証日：2026-10-18 JST。

### Source analysis profileとdriver境界

実装／検証日：2026-10-18 JST。`lang_hir::program::analyze_program`は検査済みCopy program、またはprivateなregion検査済みHIR bodyを保持するownership programを返します。互換entry pointの`check_program`と`check_expression_function`は引き続きCopy execution profileを要求します。成功したownership解析は保持され、再実行したり未対応の言語機能として報告したりしません。

`lang_semantics::analysis::analyze`がdriver／query共通のentry pointです。検査済みownership HIRをtyped MIRへloweringし、move、loan activity、provenanceを検査して、HIR／MIR両方のproofを`Program::Ownership`に保持します。Copy programは既存の`TypedProgram`表現を維持し、ownership MIRのlimitは適用されません。Ownership MIRのresource failureは元のroot module spanを持つ`SRC0011`診断になります。Driverはremap後のMIR source IDから元のsource IDを推測しません。内部不変条件のerrorは構造化されたanalysis failureとして保持します。

`lang check [--source-root DIR] FILE`はコードを実行せず、名前解決と該当するsource解析を行います。Libraryの検査には`main`関数もhost関数のbindingも不要です。成功時は無出力で終了status 0、source／analysis failureは1、不正なcommand usageは2を返します。[Ownership example](../../examples/analysis/ownership.lang)でこの経路を確認できます。Query database snapshotも検査に成功したownership sourceをvalidとして報告し、既存のrevision／dependency publication検査と上限付きdiagnostics cacheを維持します。

Frontendの`Analysis.result`は共通の`Program` enumを保持するようになりました。Copy `TypedProgram`が必要なcallerは`as_copy`でvariantを選択するか、`frontend::copy_executable`で構造化されたexecution-profile errorを取得します。既存のCopy MIR／bytecode lowering APIは変更しません。`lang run`とREPLはcompileまたはitem publicationの前にこの境界を適用し、拒否されたownership REPL itemはcommit済みsessionを変更しません。Ownership解析proofはまだ実行可能layout、allocation、cleanupを許可せず、serialized formatの変更もありません。

検証では、HIR／MIR解析結果の保持、元sourceを指すresource診断、library／複数fileの検査、query cacheの無効化とstale ticket、REPL拒否時のtransactionを対象に7 testを追加しました。CLI／REPL／queryのfocused test 63件とworkspace全847 test、およびformat、Clippy、documentation生成、fuzz targetのformat／Clippyが成功しました。独立したexampleも`cargo run -- check examples/analysis/ownership.lang`で成功しています。

出典：[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)と[RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)の最終更新日：2026-10-17。[共通analysis](../../crates/semantics/src/analysis.rs)、[frontend](../../src/frontend.rs)、[CLI](../../src/main.rs)、[query test](../../crates/semantics/tests/database.rs)、[CLI test](../../tests/cli.rs)、[REPL test](../../tests/repl.rs)の更新／検証日：2026-10-18 JST。

### Source formatter

2026-10-17に実装したRFC 0007構文拡張：lossless token／CSTで `&T`／`&mut T`、`shared T`／`weak T`／`arena T`、入れ子の組み込みtype argument、`::<...>` call、place borrow、括弧付きdereference、place assignment、allocatorを渡す字句的arena blockを表現します。Generic末尾の `>` は別tokenのままで、`<` 比較と符号付き整数のbehaviorを維持します。Prefix／type argumentの再帰とrecoveryは既存node／depth／list／work上限に従い、formatterはgeneric delimiterと比較を区別して新tokenの全境界間のcommentを保持します。

従来の名前位置のownership予約語、`core` というdeclaration／import／local／pattern bindingには受理済み `SRC0024` を出し、予約module名はfile IOより前に拒否します。通常record fieldの `core` は許可します。Source bindingは全CST nodeを課金してから上記type／body／core-call構文を解決し、borrowed primitiveやownership operationをCopyの挙動へ落としません。受理済み `SRC0020`–`SRC0024` の登録は全semantic checkの実装を意味しません。10件のsyntax／migration testでprecedence、ownership fixture全prefix、recovery／resource上限、comment保持のformatter／CST往復、migration、初期のspan付き拒否境界（後に解決済みHIRへ置換）を検証します。完全なbody ownership checking／MIR、borrow validity、arena escape flow、core operation、実行は未完了です。

情報源：受理済み[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、syntax／diagnostic／module／binding実装とtestは2026-10-17 JST更新／確認。受理済みsource profileの実装sliceで、新しい言語判断やownership解析の完了ではありません。

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

2026-10-17実装：`lang-query` はbackend非依存のin-memory graph／cache coreです。Hostはopaque key、immutableな `Arc` value、完全な明示dependency集合を渡します。Input editは推移的consumerをinvalidateし、無関係なcacheを保持します。Query publication ticketはgraph identity、query generation、dependency revisionを記録し、edit、新しいwork、publication、removal、key再利用後に古い結果で現在のvalueを置換させません。Dependency置換で古いreverse edgeを残しません。Key closeはconsumerをinvalidateし、incoming referenceを上限付きtombstoneとして保持するためreopenでも古いworkを復活させません。Cycle、node／edge／work limit、revision overflowは全bookkeepingをstate変更前に検証します。Traversalは反復的かつdeterministicです。上限対象はgraph bookkeepingであり、hostのanalysis-value memoryやcodeではありません。構造化input HIRのintegration testで診断cache再利用と古い結果の拒否を示します。Source databaseがquery recipe自動実行とparser／module dependencyをLSP serverへ接続します。Hash／timing永続化は将来の作業で、graph coreだけではcompiler統合を提供しません。

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

### Shared ownership runtime

2026-10-17実装：`lang-runtime::shared` は実際にretainする `Strong` owner、payloadをretainしない `Weak` token、retainを伴うweak upgrade、追跡するstrong／weak payload field、checked whole-payload replacement、反復的reclamationを提供します。1つのtopology mutexでreference-count遷移、liveness確認、cycle preflight、publicationを線形化します。これはlockを使うatomic実装で、lock-freeを主張しません。Handleは非公開のheap provenanceとgenerationを持ち、generation枯渇時はslotを廃止します。Weak tokenはmetadataを保持してもpayloadをliveに保ちません。Read guardはretainしたleaseとreader登録を持ち、どのaliasのreadも置換を防ぎ、Rust hostが元ownerをdropしてもguardを無効化しません。Leaseもruntimeのreference-count上限へ計上します。

Publicationは全endpointと累積node／slot／edge／live-payload-byte上限を検証し、繰り返すtraversalのworkを制限し、変更前にmetadata／retirement scratchを確保します。拒否した置換はowning inputをそのまま返し、lockを出てからcleanupできます。成功時は新field／countをすべてpublishしてから除去fieldをreleaseします。Strong cycle／expired weakはRFC 0004のlogical identityを保持し、cycle errorはwriteと既存edgeの関連byte spanを保持します。Preflight／publication／reclamation中にuser callbackは呼びません。Payloadはopaque byteで、全ownership fieldをedgeとして追跡します。Retirementは先にaccessを取り消し、供給された初期化順の逆順でfieldを処理し、再帰dropや新規allocationを使わず事前確保済みstorageで反復処理します。

16件のfocused testでretain付きupgrade、alias read、weak back-edge、publication失敗時のsnapshot／count／span、heap provenance、limit／count overflow、slot再利用／枯渇、poison quarantine、15,001 nodeの反復reclamation、独立した推移閉包／count oracleと比較する1,000回の混合publication、64回の逆向き同時publication競合、128回のupgrade／last-release競合を検証します。Deterministic／stress testであり、memory orderingのmodel checkingやsanitizer／fuzz campaignではありません。独自unsafe atomicを使わずRustのmutex同期契約に従います。

このserviceでruntime retain／releaseとatomic publicationのscopeを完了します。Source type／loan lowering、VM／native ownership operation、guest destructor／error／panic cleanup、source arena統合、allocator callback、source heap ABI統合はMilestone 2の個別作業として残ります。現状はhost Rust allocationを使い、fallible Vec reserveを確認する一方、Arc allocationはhost OOM時にabortし得ます。Live payload計上はprocess／allocator全体のmemory計上ではありません。Source契約を公開する前に、別途受理するallocator／cleanup profileへ適応または置換する必要があります。Poisonしたstateはfail closedとし、Rust hostは他のRust owner同様token／guardを意図的にleakできます。

情報源：受理済み[RFC 0004](rfcs/0004-owned-values-and-cyclic-graphs.md)／[RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、runtime実装とtestは2026-10-17 JST更新／確認。Build／test evidenceにはpinしたstableの `rustc 1.99.0 (b940084d7 2026-09-28)` を使い、2026-10-17 JSTに確認しました。外部API文書のinspectionは主張しません。Lock、storage、capacity戦略は実装判断で、performance budgetは主張しません。

### Runtime arena storage

2026-10-17実装：`lang-runtime::arena` は単独の `Arena` owner、retainせずcopyできる `Reference` token、追跡するedgeを持つopaque byte payload、arena全体のread loan、検査付きallocation／replacementを提供します。同じarena内のself-edgeや任意のcycleにnode reference countは不要です。Publicationはarena間、runtime間、shared、weakのedgeをinput消費前に拒否し、失敗時はinputを変更せず返します。Replacementはnode identityとallocation順を維持し、古いpayloadをstate mutexの外でcleanupできるよう返します。Mutex内でuser callbackは実行しません。

非公開runtime provenance、arena slot generation、検証したnode indexにより、stale handleが再利用したstorageを参照することを防ぎます。公開 `Identity` fieldは信頼しない論理識別子であり、wire formatやaddress authorityではありません。Resolveにはowning arenaが必要です。Provenance counterの枯渇はfail closedとし、generationが枯渇したslotは永久にretireします。登録済みreadは別のnode reference経由も含めarena全体のallocation、replacement、通常closeを禁止します。信頼するRust hostはread guardを保持したままownerをdropできます。その場合は終了後のguard accessを拒否し、保持したsnapshotで既に返したRust sliceの安全性を維持します。古いguardは再利用したgenerationのreader登録を解除できません。このhost側の処理はsource loanのlexical arena外へのescapeを許可するものではありません。

終了時はregionをdetachしてaccessを無効にした後、追加allocation不要の `Teardown` iteratorを返します。Iteratorは初期化済みnodeをallocationの逆順で1回ずつ返します。途中のdrainを破棄した場合やhost codeのunwind時も、残るbufferを同じ順序で解放し、cleanupからlock外でheapへ再入できます。`retired_nodes` は論理的retirement数であり、guest destructor呼び出し数ではありません。Read／retired snapshotは最後のstorage ownerのdropまたはcallerへのpayload返却までresident payload／byte／edge計上に残ります。そのためreplacementはhostが保持する古いbufferを控除せず、旧値と新値の合計peakを予約します。Live arena、arena slot、live node、publication work、readerにも上限を設けます。Poisonしたstateはaccess／publicationを拒否します。Poisonしたbudgetは新規publicationを拒否しますが、解放は継続できます。

22件のfocused testで、同一regionのcycle、reference copy、provenanceとstale／forged handle、read conflict、逆順／部分teardown、slot／count枯渇、poison隔離、失敗時に変更されないinput／span／count、保持storageの上限、独立payload／edge oracleで照合する1,000件の生成graph sequence、128件のread解放／close race、20,001 nodeのcycleの反復drain、再入cleanupとhost unwindを検証します。これらはdeterministic／stress testであり、guest destructorやVM／nativeのtrace等価性を示すものではありません。

このengineはhost Rust storageを使います。Fallible Vec metadata reserveを確認する一方、Arc allocationはhost OOM時にabortし得ます。Resident byte計上にはmetadata capacity、allocator overhead、caller所有のinput、callerへ返却済みpayloadを含めず、process memory sandboxではありません。信頼するhostはguardやdetachしたpayloadを意図的にforgetできますが、engineが保持するstorageは設定上限の計上に残ります。Source allocator callback、typed layout、cleanup／error／panicの挙動、検証済みownership MIR／bytecode、VM／native arena operation、serialized ownership formatはMilestone 2の個別作業として残ります。この実装だけではarena outcomeは完了しません。

出典：受理済み[RFC 0004](../rfcs/0004-owned-values-and-cyclic-graphs.md)、[RFC 0007](../rfcs/0007-source-ownership-and-core-values.md)、[arena実装](../../crates/runtime/src/arena.rs)、[境界test](../../crates/runtime/src/arena/tests.rs)、[統合test](../../crates/runtime/tests/arena.rs)。更新／検証日：2026-10-17 JST。Storage、lock、quotaの粒度は実装上の選択であり、allocator ABI、performance budget、追加の言語決定は主張しません。

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

Backend-neutralなcanonical-resource adapter traceにより、runtime handle layoutを公開せずRFC 0003のownership／failure orderingを実行可能にしました。Instanceごとのboundary indexは非ゼロかつ単調増加で、再利用しません。Call全体のpreflightはすべてのresource argumentとdestination limitをvalidateし、storageを予約してから、allocationもcallbackも行わないcommitでownerをmoveまたはcall-scoped borrowを公開します。Cleanupはnested borrowを内側から閉じ、残るframe ownerをremoveしてからacquisitionと逆順にdestructorを呼び、独立したdestructor failure後も継続し、先行するpanic／trapをprimaryのまま保ち、callback／invariant／destructor failure後にcalleeをpoisonします。共有host-value bridgeはopaque identityをallocateしてgenerationとtable provenanceを検証した `ResourceTable<T>` ownerへmapし、canonical publication failure時はidentityを再利用せずvalueをrollbackし、runtime borrow operationをresolveし、cleanupがactual valueをexactly onceでconsumeできるようにします。General instantiationはinstanceを公開する前にcanonical順のschema-2 requirementをすべてnegotiateし、immutable bindingとfreshでboundedなadapter stateを持つinstanceを生成します。Conformance fixtureはVM、JIT、macOS AOTのscalar resultを同じowned transfer、guest-trap cleanup、redacted failure projectionへ通し、value、state、report outcomeが一致します。後続の直線形nominal `own<T>` laneはMIR、bytecode、VM、JIT、tier-1 AOTで動作しますが、汎用resource control flowとreturn ownership adapterは未実装です。Asynchronous cancellationも未完了です。

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

実装前のcleanなmacOS AArch64 [debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)は、元のnegative baselineを確立しました。Commit済みの[source-mapped DWARF inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md)は、typed MIRが構造を検証したsource file、function span、各instruction／terminatorに1つずつのsource spanを保持し、Craneliftがそれらをbackend-neutralなJIT／AOT machine-code rangeとして維持し、Mach-O serializationがrelocation付きDWARF v4 compile unit、subprogram entry、line tableを出力することを実証します。代表的なadd、minimum、host-callback objectは `dwarfdump --verify` をpassし、direct／response-file executableはlink／実行され、生成した両dSYMもfixture pathと3 functionを保持したままverificationをpassします。Commit済みの[Windows x86-64 CodeView／PDB inspection](../../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md)は、同等のC13 symbol、type、line table、relocationをCOFFでvalidateし、LLDでPE／PDB pairへlinkし、LLDBでfunction-to-source resolutionを検証します。これはnative Windows runではなくcross-target debug-format evidenceです。Source-language local、unwind metadata、JIT debugger registration、optimized steppingは個別のacceptance workであり、backend選定ではそれらのcost／qualityを考慮する必要があります。

Host linker discoveryは、deterministicなcandidate順で `PATH` を探索し、Unixではexecutable fileを要求し、driverがない場合は探索したpathとcandidate nameを含むstructured diagnosticを返します。macOS integrationは発見したdriverを使用し、cfg-gated MSVC integrationもdeveloper environmentが `PATH` を初期化した後に同じdiscoveryを検証します。そのenvironment外にあるVisual Studio、SDK、compilerのinstallation discoveryは個別の作業として残っています。

Root crateは、初期tier-1 linker policyをbackend-neutralな `LinkTarget` domain typeとして表現します。Exactな `aarch64-apple-darwin` と `x86_64-pc-windows-msvc` tripleがcommand flavor、default program、ordered `PATH` candidateを決定し、未supportのtripleはhost policyを暗黙に継承せずstructured errorになります。明示targetのlinker構築、discovery、platform integration、cross-target COFF testがこのmappingを共有します。別targetのsupportには明示的なpolicy追加が必要です。Installation discoveryとtarget固有のstartup／runtime inputは別layerのままです。

過去のbackend実験と当時のevidence不足は[benchmark evidence](benchmarks.md)に記録します。受理済みsource規則や完了済みWindows baselineを未決定へ戻すものではありません。

### Compilation unit全体のcopy-value lowering

2026-10-17実装：`lang-codegen-clif::program` は検証済みcompilation unitからRFC 0006のcopy-value命令すべてをJIT codeまたは単一native objectへloweringします。全guest functionをbodyより先に宣言し、前方／相互再帰、bool parameter／result、入れ子のrecord／enum、field projection、順序付きpattern、aggregateのloop joinをsupportします。引数と結果は内部専用のpointer／cell規約で受け渡し、stable source ABIやC export interfaceとしては扱いません。Nominal identity、bool／tag／header／paddingの妥当性、enum非active領域のゼロ初期化を保持します。Block edgeでは全operandを読んでからdestinationへ書き込みます。不正なenum projectionはstatusを返し、後続の副作用を実行する前にguest call全体へ伝播します。

Compilationは累積budgetでMIRを再検証し、全host bindingを事前確認し、展開後のcell copyと一時bufferを計上します。Owned register、深さ64を超えるlayout、outgoing argumentを含め1 MiBを超えるframeはnative compilerの明示的なcapacity limitとして拒否します。Cranelift内部のallocationは信頼するcompiler境界です。JIT呼び出し時にはcanonical input bufferを検証し、call全体で実行codeのownerを保持し、ownerのdropで実行用mappingを解放します。Native recursionとhost callbackは信頼する同期実行で、VMのfuel／frame／interruption制限は適用しません。

Native rangeは複数moduleを含む元のsource IDとinstruction／terminatorのbyte spanを保持します。この新しい内部規約に対応する完全なsource-level DWARF／CodeView出力は、Milestone 2の独立したdebug taskです。`tests/native_program.rs` はportable bytecode往復、VM／JIT結果一致、再帰、複合型の入力／結果、literal／structural pattern、評価順序、self-edgeでの入れ替え、不正projection／入力、compile budget／frame capacity、Mach-O／COFF object、およびcfgで対象を絞ったdirect／response-file object link／runを検証します。macOS AArch64実行はlocalで確認済みで、新しいWindows x86-64実行evidenceは未取得です。

情報源：受理済み[RFC 0006](rfcs/0006-source-frontend-and-portable-programs.md)、MIR layout／verifier、native実装／testはすべて2026-10-17 JST更新／確認。Lock済みCranelift 0.136.3のinstall済みsourceでstack-slot、memory-flag、module、実行memory解放APIを確認しました。各source fileに公開／更新日の明記はなく、確認日は2026-10-17 JSTです。Cell layoutとcapacity limitはprojectの実装判断です。

## Diagnostics、LSP、debugging

Diagnosticsは、stable code、primary／secondary span、note、fix-it、machine-readable fieldを持つstructured valueです。

2026-10-17実装：`lang-diagnostics` はimmutableなUTF-8 snapshot、全mapを通じて異なるsnapshot ID、文字境界を検証する半開byte span、1始まりのUnicode scalar位置、IDE adapter向け0始まりのUTF-16 columnを管理します。Diagnosticはprimary labelを1つ必須とし、別fileのsecondary label、note、machine-applicable／speculativeを区別したfixを追加できます。Fix previewは全参照snapshotを検証し、重複や曖昧なeditを拒否してから変更後のtextを返し、fileは変更しません。Plain-text表示はdeterministicで、制御文字とdirectional-formatting文字をescapeします。RFC 0002の `SRC0001`–`SRC0007` を表現し、構造化HIRは `SRC0002`–`SRC0007` を生成します。Source frontendはliteral／parser診断生成と `SRC0019` までのtext接続済みdiagnostic conformanceを提供します。このcrateはsource grammarやserialized diagnostic formatを導入しません。

LSPはcompilerのparser、resolver、type checker、source map、incremental query databaseを再利用します。不完全またはinvalidなprogramでもresponsiveである必要があります。

2026-10-17実装：`lang-lsp` は既存structured diagnostic modelをUTF-16 client profileのLSP diagnostic dataへ変換します。Source byte spanをcompiler line番号とは独立して変換します。LSPはLF、CRLF、単独CRを認識し、0始まりのUTF-16位置とexclusiveなrange endを使います。CRLF途中のbyte境界は前の行のcontent endへ正規化します。Adapterはsource spanと座標上限を検証し、source code／severityとrelated labelを保持し、primary-label詳細／noteをplain diagnostic textへ含めます。Hostが検証済みdocument URIを渡し、source表示名からURIやfilesystem accessを推測しません。明示source-byte、label-count、note-count、output-byte limitで変換を制限します。Fixは将来のcode-action adapter向けにcompiler diagnostic側へ残します。

2026-10-17実装：`lang lsp` は共有databaseを使う同期stdio LSP 3.18 prototypeです。Initialize／initialized／shutdown／exitを実装し、必須のUTF-16を明示して返し、full document同期とdocument formattingをadvertiseします。Related informationと診断versionはclient capabilityに従います。増加するdocument versionはquery評価／publication前にimmutable overlayを更新し、古いversionは無視します。Databaseはedit、close／reopen、新しいwork後の古いjobを拒否します。Closeは診断をclearしてoverlay authorityを解放し、権限がある場合は残るrootをdiskからrefreshします。Open済みimported fileは親moduleの文脈を使い、closed dependencyの診断はopen rootのmodule宣言へ付け、対応clientには元位置をrelated informationで示します。Missing moduleはopen bufferまたはwatched-file通知で回復します。Dynamic watcher登録は対応clientにだけ要求します。

Initializeはcanonicalなlocal source rootを1つ許可し、省略時はfilesystemを読みません。Local file URIを検証／percent-decodeし、nonlocal authority、NUL、parent traversal、root外／symlink escapeはsource読込み前に拒否します。Stdio framingはASCII CRLF headerとUTF-8 byte長を使い、重複／不正lengthとtruncated frameを拒否し、payload割当て前にheader 8 KiB、message 16 MiBを上限とします。JSON parserのnesting limitも維持します。不正JSONと非対応charsetはparse errorを返し、曖昧なframingはconnectionを終了します。出力serializationもframe書込み前に制限します。JSON-RPC batch、completion、navigation、code action、DAPは非対応です。Formattingはcanonicalな4-spaceのwhole-document editを返し、不正inputを保持します。12件のintegration testで実stdio process、lifecycle／capability、UTF-16、version／close、module overlay／watcher、input／authority境界、framing、不正JSON、formattingを検証します。

Transport／lifecycle情報源：Microsoft公式の[LSP 3.18 specification](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/specification.md)、[initialize](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/general/initialize.md)、[didChange](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/lsp/3.18/textDocument/didChange.md)、[shutdown](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_includes/messages/3.18/shutdown.md)、[exit](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_includes/messages/3.18/exit.md)。更新日／公開日を記載しないliving documentで、2026-10-17 JST確認です。[JSON-RPC 2.0](https://www.jsonrpc.org/specification)は2010-03-26公開、2013-01-04最終更新で、2026-10-17 JST確認です。Protocol要件は情報源に基づく事実で、選択したfeature、同期scheduling、routing、resource capはprojectの実装判断です。

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

続くrepository全体のreviewでは、runtime resource handleを発行元tableへ結び付け、すべての公開database queryでcache lookup／disk read前にsource-root権限を検証し、optionのような相対linker filenameをdirect／response-file commandの両方で保持するよう修正しました。過大なdocument openを拒否してもfile slotを消費しません。許可済みopen overlayはdisk削除／置換後も利用でき、close時にその権限を失います。再現条件、検証範囲、結果は[日付付きreview](review-2026-10-17.md)を参照してください。

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
