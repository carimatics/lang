<!-- translation-of: README.md; canonical-language: en -->

# lang

[English](README.md)

`lang` は、新しいシステムプログラミング言語と、その完全な開発ツールチェーンを作るためのプレアルファ研究プロジェクトです。実装はRustで開始し、言語・コンパイラ・ブートストラップ手順が自己再生成できるほど安定した段階でセルフホスティングへ移行することを目指します。

現在は意味論の定義とアーキテクチャの検証を進めています。安定した構文、ABI、パッケージ形式、リリース予定はまだ存在しません。

## ビジョン

この言語では、以下を両立させます。

- システム開発とアプリケーション開発に適したネイティブ性能。
- トレーシングGCを必須としないメモリ安全性。
- Kotlinに近い生産性の高い表現力：Null安全、代数的データ型、パターンマッチ、ジェネリクス、トレイト、拡張指向API、クロージャ、構造化並行処理。
- 監査可能なunsafe境界を通じた、ポインタ、アロケータ、SIMD、OS API、外部関数への明示的アクセス。
- AOTコンパイルと、同一プロセス内で相互呼び出しできる埋め込み可能なスクリプトランタイム。
- 診断、formatter、linter、テスト、パッケージ／ビルド管理、LSP、DAP、ドキュメント、ツールチェーン導入を含む第一級のツール群。
- 単一言語フロントエンドから開始し、共通のインターフェースモデルとコンパイラパイプラインを共有する複数のドメイン特化フロントエンドへ発展できる構造。

## 初期方針

受理済みarchitecture baselineは、完全なビジョンより意図的に範囲を絞っています。未実装の拡張には引き続き個別RFCが必要です。

1. まず1つの言語を作ります。最初から2つの言語を作るのではなく、安全な高レベルコードと明示的な低レベル機能を同じ意味モデルで提供します。
2. すべての実行モードで、構文解析、型検査、所有権解析、型付き中間表現を共有します。
3. 共通MIRから、スクリプト向けのポータブルなバイトコードVMと、AOT向けのネイティブオブジェクトコードへloweringします。
4. VMの意味論、デバッグ情報、埋め込みAPIが安定してからJITコンパイルを追加します。
5. バージョン付きインターフェース記述とcanonical component ABIを定義します。レイアウトが判明している場合は直接内部呼び出しを使用し、独立してバージョン管理される境界では生成されたthunk／handleを使用します。
6. 信頼済みネイティブプラグインと、capabilityで制限されたスクリプト／WebAssemblyプラグインを別のデプロイ層として扱います。
7. セルフホスティングは、初期実装を制約する条件ではなく、再現可能なブートストラップのマイルストーンとして扱います。

根拠と未解決のトレードオフは、[アーキテクチャbaseline](docs/ja/architecture.md)に記載しています。

## ドキュメント

- [言語・エコシステム調査](docs/ja/research.md)
- [Prototype benchmark evidence](docs/ja/benchmarks.md)
- [アーキテクチャbaseline](docs/ja/architecture.md)
- [実装ロードマップ](docs/ja/roadmap.md)
- [RFCプロセスとテンプレート](docs/ja/rfcs/0000-template.md)
- [RFC 0001：プロジェクト憲章と互換性ポリシー](docs/ja/rfcs/0001-project-charter.md)
- [RFC 0002：Prototype integer／structured control-flow semantics](docs/ja/rfcs/0002-prototype-control-flow.md)
- [RFC 0003：Version付きcomponent contractとcapability import](docs/ja/rfcs/0003-versioned-component-contracts.md)
- [RFC 0004：Owned value、明示的共有、cyclic graph storage](docs/ja/rfcs/0004-owned-values-and-cyclic-graphs.md)
- [RFC 0005：Structured task、cancellation、isolated service](docs/ja/rfcs/0005-structured-tasks-and-isolated-services.md)
- [RFC 0006：Source frontendとportable VM program（accepted）](docs/ja/rfcs/0006-source-frontend-and-portable-programs.md)
- [コントリビューションガイド](CONTRIBUTING.ja.md)

## 開発

Milestone 1ではbackend非依存の `lang-diagnostics`／`lang-syntax`／`lang-hir` crateを追加しました。DiagnosticsはimmutableなUTF-8 source snapshot、検証済みbyte span、Unicode／UTF-16位置、label、note、atomic fix previewを提供します。RFC 0006 lexer／parserはUTF-8 inputを検証し、符号付きdecimal literalをdecodeし、宣言、control flow、call、aggregate、patternをarena方式のlossless treeへparseします。Trivia、不正token、missing-syntax nodeを保持し、後続の宣言へ復帰します。Byte、token、node、depth、list要素、診断数、workに上限があります。HIR analysisは構造化したRFC 0002 function inputを解決／type-checkし、lexical local scope、function全体での名前一意性、immutable parameter、`i64`／内部 `bool`、mutation、condition type、明示return経路を検証します。Spanを保持し、循環／budget超過inputを拒否します。`cargo test -p lang-diagnostics -p lang-syntax -p lang-hir` で集中検証できます。Source実行は未実装です。

`lang-semantics::modules` は明示されたcanonical source root内のmoduleをparseして読み込み、共有item namespaceを収集し、可視性を検証しながらimportとmodule／enum-case pathを解決します。Root外への参照、同一canonical fileの重複、module／aliasの循環、名前重複、resource上限超過を拒否します。`lang-semantics::binding` はparsed local／pattern scopeとtype pathを `lang-hir::resolved` へ解決し、public signature内のprivate typeを拒否し、宣言、型注釈、式、pattern、control flowのsource spanを保持します。`cargo test -p lang-semantics --test modules --test binding` で検証できます。Source実行は実装の残作業です。

`lang-hir::layout` はguest割り当て前にnominal循環とmember shapeを検証し、copy valueのcell数／depthを上限付きで計算します。既定値はtype depth 64、valueあたり4,096 cellで、enum storageは最大payloadを予約します。`cargo test -p lang-hir --test nominal_layouts` でlayout境界と不正inputを検証できます。

`lang-hir::program` はresolved sourceのfunction、call、aggregate操作、pattern、上限付きexhaustivenessを検証してread-onlyなtyped programを生成します。Missing witnessとunreachable arm／statementを報告し、到達不能bodyも検証します。`cargo test -p lang-semantics --test program_typing` でsourceのcompile-pass／fail、有限domain oracle、不正IRを検証できます。MIR／VM実行とsource CLI entry pointは未実装です。

`lang_mir::unit` はsource byte／span、call、nominal value、SSA dominance、上限付きallocation／work計数を含むcompilation unitを定義／検証します。`cargo test -p lang-mir --test units` で検証できます。Source loweringとVM／native統合は未完了で、unitのdata structureはserialized formatではありません。

`lang_bytecode::unit` は明示的なversion／opcode、ID再配置、UTF-8／span検証、decoder／verifier共通のresource budgetを持つcanonical schema 1 artifactをencode／decodeします。`cargo test -p lang-bytecode --test unit_codec` で検証できます。固定golden fixtureと不正input／fuzz検証を含み、source VM実行は未完了です。

`lang-semantics::lowering` は成功したprimitive HIRを検証済みtyped MIRへ変換します。Branch assignmentはtyped block parameterでmergeし、loopはback-edgeを越えてmutable valueを運び、early returnと到達不能statementは受理済みbehaviorを保持します。Instruction／terminator spanを既存bytecodeとnative debug metadataへ対応付けます。`cargo test --test hir_lowering` で構造化inputのVM／JIT differential execution、host-object debug mapping、lowering budgetを検証できます。これらはHIR-to-MIR経路のevidenceであり、end-to-end source compilationではありません。

`lang-query` はincremental graph／cacheの共有coreとして、immutable input revision、明示query dependency、推移的invalidation、editやclose／reopen後の古いanalysis結果を拒否するpublication ticketを提供します。Graphのnode、edge、bookkeeping workに上限を設け、失敗をstate変更前に検証します。`cargo test -p lang-query` はcache分離、古い／別graphの結果、dependency置換、cycle、limit、実際の構造化input HIR診断cacheを検証します。Adapterは参照する全dependencyの宣言が必要です。Compiler queryの自動実行、parser／module dependency検出、LSP transportは未実装です。

`lang-lsp` は既存のstructured diagnosticをUTF-16位置のLSP 3.18 diagnostic data modelへ変換します。ProtocolのLF／CRLF／CR改行、surrogate-pair幅、EOF range、stable code／severity、host提供URIを使う複数fileのrelated locationを扱います。`cargo test -p lang-lsp` で変換とresource limitを検証できます。このdata adapterにはserver、position-encoding negotiation、JSON-RPC transport、revision付きpublication、code actionはまだなく、統合作業として残ります。公式specificationの情報源と確認日を[architecture](docs/ja/architecture.md#diagnosticslspdebugging)へ記録しています。

必要な環境：

- `rust-toolchain.toml` で固定されたRust `1.99.0`
- 固定ツールチェーンから導入される `rustfmt` とClippy
- リポジトリの補助ツールバージョンを利用する場合は `mise`

現在のVM/native等価性プロトタイプを実行します。

```bash
cargo run -- 20 22
```

このcommandは、検証済みの型付きMIR関数を構築し、検証済みregister bytecodeへloweringしてVMで実行し、同じMIRをCranelift JITでcompileして結果を比較します。wrapping `i64` 加算は受理済みRFC 0002と一致しますが、source frontendは未実装です。Cranelift 0.136.3をinitial native JIT／AOT backendとして選定し、decisionと再評価triggerを[architecture](docs/ja/architecture.md#backend戦略)へ記録しました。

Workspace testでは、最小のembedding経路も検証します。backend非依存のhost-function IDから型付き `extern "C" fn(i64) -> i64` 登録を解決し、同じMIR host callをVMとCranelift JITの両方で実行します。VMは合成可能なopt-in instruction-fuel／host-call budgetに加えて、value-slot limit、optionalな共有one-way interruption flag、絶対monotonic deadlineを公開します。成功したlimit付きexecutionは、両方の消費countと静的なregister-slot footprintを報告します。各bytecode instructionとterminatorは1 fuel unit、実行に入る各callbackは1 host-call unitを消費し、どちらの動的limitも対応するside effectの前に確認します。Validationとarity checkの後、VMはregister fileを割り当てる前に、register countがslot limitを超えるprogramを拒否します。Interruption／deadline checkはregister-file allocation前とinstruction、terminator、callback boundaryで行い、structured errorが両方の消費countを報告します。同じsafe pointで両方のconditionが成立する場合はinterruptionを優先します。Callbackはinterruptionをrequestしたりdeadlineを超過したりでき、そのcallback後の次のsafe pointで観測します。既存convenience functionはunbounded、non-interruptible、deadlineなしのままです。Slot countはbyte単位のheap limitではなく、callbackのallocationを含みません。このprototypeはstable ABIを定義せず、aggregate value、error transport、panic containment、capability、heap／recursion／task limit、callbackのpreemptive interruption、native／JIT resource enforcementはまだ提供しません。

`lang-codegen-clif` は同じMIRからobject artifactを生成し、compiled functionをexportし、host callbackをimportとして残すこともできます。Host-native pathに加えてcallerは明示的なtarget tripleを選択でき、host非依存testは `x86_64-pc-windows-msvc` COFF構造とsource駆動CodeView recordをvalidateします。Root crateのtarget-neutralな `LinkRequest`／`SystemLinker` は順序付きinputを維持し、`cc`／`link.exe` をsupportし、structured failureを公開し、明示的なGNU UTF-8またはWindows UTF-16LE response fileをserializeします。macOS AArch64 integrationと[Windows x86-64 run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398)は、共通pathでdirect／response-file linkの両方を実行します。Production installation discovery、startup／runtime packaging、追加target policy、stable native ABIは別作業のままです。

実装前のcleanなmacOS AArch64 [debug-information inspection](benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)は、元のnegative baselineを記録しています。その後のcommit済み[source-mapped DWARF inspection](benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md)は、代表的なadd、minimum、host-callback Mach-O objectが、構造を検証したMIR source map由来のrelocation付きDWARF v4 compile-unit、subprogram、line-table dataを含むことを検証します。3 objectすべてが `dwarfdump --verify` をpassし、direct／response-file executableがlink／実行され、生成した両dSYMもfixture pathと3 functionを保持したままverificationをpassします。Commit済みの[Windows x86-64 CodeView／PDB inspection](benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md)は、同等のCOFF objectにあるC13 record／relocationをvalidateし、LLDでcross-targetのPE／PDB artifactへlinkし、LLDBで同じfunctionをfixtureへ解決します。このinspectionはcross-targetであり、別のWindows CI runがnative execution evidenceを提供します。Source-language local、JIT debugger registration、unwind metadata、optimized steppingは個別の作業として残ります。

2-value accumulator prototypeは、countdown valueとwrapping sumの両方をback-edgeのblock parameterで運びます。Conditional-sum prototypeはそのloop内にbranchを追加し、別のblock parameter pairを介してadd／skip pathをmergeします。VM／JIT differential testはnon-positive input、両conditional path、bounded positive loopを扱い、macOS AArch64 direct／response-file integrationは両AOT objectを実行し、host非依存Windows testはexportされたCOFF symbolを検証します。これらのprototypeはsource-level loop syntaxやallocation semanticsを定義せずにMIR CFG evidenceを広げます。

`SystemLinker::discover_host` は、host flavorのcandidate（`cc`、`clang`、`gcc`、またはMSVCの `link.exe`）を `PATH` から探索し、Unixではnon-executable fileをskipし、探索したpathとcandidateをstructured errorで報告します。Platform integration testは、発見したexecutableを使用します。初期化済みtoolchain environmentの外側にあるVisual Studio、SDK、compiler installationのdiscoveryは未完了です。

`LinkTarget` は、`aarch64-apple-darwin` と `x86_64-pc-windows-msvc` に対する初期tier-1 linker policyを一元化します。各exact tripleをlinker flavor、default program、ordered discovery candidateへ対応付け、未supportのtripleをstructuredに拒否し、`SystemLinker` による明示targetの構築とdiscoveryを駆動します。Object-format testとcfg-gatedの両platform integrationは、この共有policyを使用します。追加targetのpolicyは未完了です。

Cleanなschema-20 [macOS AArch64](benchmarks/results/2026-10-16-macos-aarch64-de248a1.json)／[Windows x86-64](benchmarks/results/2026-10-17-windows-x86_64-7c37d5d.json) benchmarkは、56個のgeneral timing row、4個の正確なcallback heap profile、4個のprocess-memory distribution、14個のcomponent-boundary rowを維持します。代表的applicationは1,000 operationを31 sample実行し、macOSでmedian 67.592 us／RSD 1.109%、Windowsでmedian 236.804 us／RSD 6.201%を記録します。Fixtureはhost-orchestratedでlowering／JIT compilationを含むため、resultはisolated call overheadやstable cross-platform budgetではなく方向的application evidenceです。

Typed MIR、bytecode verifier、VM、JIT、object pathは、signed `i64` comparisonと明示的control-flow graphも共有します。2引数minimum functionはentry blockから個別のreturn blockへ分岐し、焦点化されたjump testが明示的edgeを検証します。型付きblock parameterとedge argumentは、専用phi instructionなしでphiに相当するmergeを表現します。MIRとbytecodeのvalidationは、invalid target、unreachable block、entry-block predecessorまたはparameter、edge argumentのarity／type mismatch、useをdominateしないvalueを拒否します。Countdownは1個の `i64`、bounded sumはcounterとaccumulator、iterative Fibonacciはcounterと2個のsequence valueをback-edgeで運び、Fibonacciではそれらを入れ替えます。VM／JIT差分testとAOT object pathは3 workloadすべてを扱います。Bytecode back-edge testは、instruction fuelが非停止loopを停止することを証明します。Allocationとsource-level control-flow semanticsは未実装です。

Symbolとdisassemblyを検査するため、どちらかのinspection targetをhost-native object fileへ書き出します。

```bash
cargo run --release --bin prototype-object -- add /tmp/prototype-add.o
cargo run --release --bin prototype-object -- min /tmp/prototype-min.o
cargo run --release --bin prototype-object -- host-callback /tmp/prototype-host-callback.o
```

このcommandはadd、signed-minimum、host-callback targetについて、exported symbol、生成function-code size、object全体のsizeを報告します。Linkerは実行しません。

実行可能なownership実験は、`cargo test -p lang-semantics` で個別に実行できます。Async modelは厳格な `RejectAll` suspension policyと、exclusive-borrow rejection／deterministic cancellation cleanupを維持しながらshared task-frame borrowを許可する明示的な `AllowSharedFrameBorrows` 代替案を比較します。Graph modelはdefaultの `Reject` shared-cycle policy、実験的な `CollectUnreachable` tracing policy、arena-scoped cyclic storageを比較します。Service modelは `OwnerIsolated` mutationと `ExclusiveCapability` 代替案を比較しつつ、sendable payloadのmove、FIFO mailbox、deferred cancellation cleanupを維持します。RFC 0004／0005はstrict policyを受理しています。Modelは未完成のevidenceでありproduction ownership／concurrency実装ではなく、制限を緩めた代替案は未受理です。

クリーンなrelease buildからprototype backend measurement harnessを実行します。

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000 \
  --startup-iterations 10
```

Harnessは、commitとdirty state、toolchain、host、system-linker metadata、生成されたJIT code／AOT object size、link済みexecutable size、requested-payload heap profile、process peak-memory distribution、lowering、JIT／AOT compilation、direct／VM／JIT call、host callback、control flow、linking、process startupのdistribution statisticsを含むversion付きJSONを出力します。Schema 20は2-provider negotiation、deterministic lock生成、exact-lock replay、instantiation、scalar／typed-resource VM／JIT call、guest-trap cleanupをcomposeする代表的application rowを1つ追加し、schema 19はcomponent instantiation、lockfile replay、host-value／failure cleanup、retained-instance process memory、component artifact／document sizeを追加し、schema 18はiterative-Fibonacci CFG timing／artifact evidence、schema 17は代表的なmixed-lifetime allocation traceとpaired process-memory evidence、schema 16はvariable-size allocation timing／heap evidence、schema 15は個別にlabel付けしたcomponent metadata-validation、interface-negotiation、scalar-dispatch、resource-lifecycle seriesを追加しました。それ以前のschemaは[benchmark evidence](docs/ja/benchmarks.md)に記録したallocation-churn、process-memory、heap、allocation、control-flow、startup rowを追加します。Platform seriesはlink済みworkload setが増えるとrenameします。Consumerは `schema_version` で分岐する必要があり、以前のJSON fileはimmutableなevidenceとして残します。これらのmeasurementは承認済みperformance guaranteeではなく、prototype evidenceです。静かなmachineでのみresultを記録し、正確なcommandとraw JSONを保持します。

Allocation boundary prototypeはsingle、64-box burst、free／reallocate churn、16-slice variable-size callbackと、対応するVM／JIT pathおよびmacOS／Windows AOT contractを持ちます。Schema 17は4個の正確なrequested-payload profileを維持しつつ、より大きなcompiler-like process workloadを追加しました。これは多数のsmall long-lived graph nodeを確保し、3分の1を解放して異sizeで置換し、さらに大きなtransient bufferを確保します。全payload byteをtouchし、同じbenchmark executableのpaired launchからcontrol／workload peak-resident distributionを得ます。現在のschema 20 macOS runは18,102,042 peak live requested byte、21,053,440 byteのmedian resident delta、2,951,398 byteのoverhead／fragmentation合算推定値を記録します。この推定値はOS／allocator固有でpage roundingとallocator retentionを含み、metadataとfragmentationを分離できず、source-language allocation、ownership、collector policy、memory limitも定義しません。

共通チェック：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

現在のworkspaceには、prototypeで実際に使う `lang-mir`、`lang-bytecode`、`lang-vm`、`lang-codegen-clif`、型付きhost-function registry、boundedな実験的schema-1 scalar codec、schema-2候補のcontract／failure-report codec、scalar component-contract validator、generation tag付きresource tableを持つ `lang-runtime`、実行可能な設計modelを持つ `lang-semantics` だけが含まれます。`docs/ja/architecture.md` に示す追加の境界は、対応する実装とともに導入します。

## 設計プロセス

構文、型意味論、メモリ安全性、並行処理、ABI、パッケージ互換性、プラグイン境界を変更する場合はRFCが必要です。リポジトリSkillの `$language-design-rfc` を使用するか、`docs/ja/rfcs/0000-template.md` から開始してください。

RFCがモード固有の制限を明示的に定めない限り、実装ではバイトコードbackendとネイティブbackendの観測可能な動作を等価に保つ必要があります。

## レビュー範囲と現在の制限

2026-10-17レビューではJIT memory回収、bytecode span保持、CodeView machine type、bounded decoding、semantic-version validation、要求全体のcapability preflight、Rust adapter cleanupを強化しました。Linker response fileは新しいpathを必要とし、既存fileとsymlinkを切り詰めず拒否します。反復呼び出しでは新しいresponse-file pathを選ぶか、自分の古いfileを明示的に削除してください。

過去のbenchmark captureは改変せず保持し、JIT memory回収修正前のものです。現在の性能を解釈する際は[benchmark evidence](docs/ja/benchmarks.md)を参照してください。直接のnative host callbackはtrustedでありunwindは禁止です。Sandboxではありません。

## プロジェクト状態

- 成熟度：研究／プレアルファ
- 対応予定ホストプラットフォーム：最初はmacOSとWindows
- 初期実装言語：Rust
- ライセンス：未選定

