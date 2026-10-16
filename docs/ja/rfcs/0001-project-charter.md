<!-- translation-of: ../../../docs/rfcs/0001-project-charter.md; canonical-language: en -->

# RFC 0001：プロジェクト憲章と互換性ポリシー

[English](../../../docs/rfcs/0001-project-charter.md)

- ステータス：proposed
- 著者：プロジェクトコントリビューター
- 作成日：2026-10-15
- 最終更新日：2026-10-15
- トラッキングIssue：
- 置き換えるRFC：
- 後継RFC：

## 概要

このRFCは、`lang` の初期実装における意思決定の枠組みを定めます。初期のプロダクト境界、プラットフォームTier、互換性領域、設計上の不変条件、実験的な動作を言語仕様上の約束に変える前に必要な証拠を定義します。

最初の実行可能なマイルストーンは、意図的に小さくした縦断スライスです。検証済みの1つの型付きMIR関数が、検証済みregister bytecode VMとCranelift native backendを通じて、同一プロセス内で等価な結果を生成しなければなりません。

## 動機

プロジェクトのビジョンは、言語意味論、メモリ安全性、ネイティブコード、スクリプト、埋め込み、並行処理、デバッグ、パッケージ管理、最終的なセルフホスティングに及びます。これらを同時に実装すると、失敗原因の特定が難しくなり、プロトタイプ上の偶発的な仕様が互換性制約になります。

プロジェクトには、以下を区別する安定した方法が必要です。

- 長期目標と現在の保証。
- 言語意味論とbackendの実装詳細。
- ソース、bytecode、native ABI、component ABI、パッケージの互換性。
- 実測結果と設計仮説。
- 信頼済みnative extension境界とcapability制限付きplugin境界。

これは言語進化、コンパイラ動作、runtime相互運用性、リリースポリシーを一体として管理するため、ライブラリやツール規約だけではこの境界を提供できません。

## 目標

- 複数のソース言語ではなく、一貫した1つの初期言語を定義する。
- 同じ意味モデル内で、低レベルなシステムプログラミングと生産的なアプリケーションプログラミングへの道を維持する。
- 共通言語機能について、VM実行とnative実行の観測可能な等価性を要求する。
- 同一プロセスへの埋め込みと双方向host callを第一級のアーキテクチャ制約にする。
- メモリ安全を既定としつつ、unsafe操作を明示的かつ監査可能にする。
- 互換性領域を定義し、それぞれに意図的なバージョン管理を要求する。
- 性能やergonomicsの主張を受け入れる前に、プロトタイプ、差分テスト、実測を要求する。
- macOS AArch64とWindows x86-64を初期Tier 1アーキテクチャペアにする。
- 最初のコンパイラにセルフホスティングを要求せず、再現可能なセルフホスティングへの道を維持する。

## 非目標

- 最終的な表層構文の選定。
- 完全な所有権、trait、effect、並行処理、例外モデルの承認。
- ソース、native、component、bytecode ABIの安定化。
- 最初のfrontendとcomponent境界が実証される前の複数言語frontend対応。
- 本番スクリプト環境でのJITコンパイル必須化。
- 将来のすべてのライブラリ抽象化に対するGC非使用の保証。
- プレアルファ期間中のリリース日、長期サポート期間、セマンティックバージョニング保証の約束。
- 現在の算術プロトタイプを承認済み言語意味論として扱うこと。

## ガイドレベル設計

プロジェクトでは、ユーザーが依存できる対象を伝えるためにRFCステータスと互換性ラベルを使用します。

RFCは以下の状態を使用します。

- `proposed`：議論中であり、互換性の約束はない。
- `accepted`：リポジトリ所有者が承認済みだが、実装は未完成の場合がある。
- `implemented`：意図したインターフェースで承認済み設計を利用でき、検証に合格している。
- `stabilized`：ユーザーが文書化された互換性ポリシーに依存できる。
- `withdrawn`：採用せず終了した。
- `superseded`：明示されたRFCに置き換えられた。

互換性に影響する各RFCは、影響を受ける領域を特定します。

- `source`：構文解析、名前解決、型付け、観測可能なプログラム動作。
- `bytecode`：bytecode encoding、verifier規則、VM動作。
- `native-abi`：単一toolchain release内のcalling conventionとdata layout。
- `component-abi`：独立してversion管理されるinterface、handle、生成adapter。
- `package`：manifest、依存関係解決、lockfile、registry protocol。
- `tooling`：診断code、formatter出力、LSP/DAP動作、機械可読出力。
- `bootstrap`：compiler stageの入力、出力、再現性要件。

プレアルファ期間中、承認済みRFCが明示的に範囲を限定しない限り、すべての領域は不安定です。それでも、リポジトリ内fixtureを無効にする変更にはテスト、ドキュメント、migration noteが必要です。

## 参照意味論

このRFCはユーザー向け構文や完全な言語意味論を定義しません。後続RFCに対して以下の不変条件を定めます。

- 共通の型付きMIRを、bytecodeとnative loweringが使用する意味境界にする。
- すべてのbackendがMIRを利用する前に検証する。
- 安全なソースプログラムでは、ユーザーコードに未検査メモリアクセスを要求しない。
- unsafeなcompiler/runtimeコードは、明示的な不変条件とテストを持つ狭いinterfaceの背後に隔離する。
- 評価順序、整数overflow、destruction、panic、unwinding、cancellationは、専用RFCで承認されるまで未指定とする。
- backend最適化は、承認済みの観測可能な動作を変更してはならない。
- capability検査とtrust boundaryは意味入力であり、任意のdeployment documentationではない。
- compile-time executionを導入する場合、明示的なcapability modelとresource modelを持たせる。

プロトタイプの `i64` wrapping加算は、VM/native差分テストを決定的にするためだけに選択しています。将来の既定overflow意味論を決定するものではありません。

## 実行モード

初期実行モデルは1つの意味入力と複数のloweringを持ちます。

- bytecode VMは、検証済みMIRからloweringされた検証済みregister bytecodeを利用する。
- native実行ではCraneliftを使い、lowering、host calling convention、同一プロセス実行を検証する。
- 本番AOT経路では、bundled linkerが正当化されるまでobject fileを出力し、system linkerを使用する。
- startup time、memory、security、debugging動作を測定するまで、JITは開発・埋め込み実験とする。
- 埋め込みhost callとcallbackは、最終的に型なしraw pointerではなく、型付き登録と生成adapterを利用する。
- WebAssembly/component実行は将来のsandbox tierであり、最初のプロトタイプには必須でない。

現在のembedding実験では、各host functionにbackend非依存のMIR identifierを割り当て、runtimeごとのregistryから型付き `extern "C" fn(i64) -> i64` を解決します。VMはそのregistry経由でdispatchし、Craneliftは同じ登録済み関数をJIT moduleへimportします。VMは合成可能なopt-in instruction-fuel／host-call budget、value-slot limit、共有one-way interruption flag、絶対monotonic deadlineも試作します。各instruction／terminatorと実行に入るcallbackは、それぞれのcounterで1 unitを消費し、各動的limitは対応するside effectの前に確認します。Validationとarity checkの後、検証済みregister countをregister-file allocation前に確認します。すでにsetされたinterruptionまたは期限切れdeadlineはそのallocation前に停止し、以降のconditionはinstruction、terminator、pre-callback safe pointで観測して、structured errorに両方の消費counterを含め、interruptionを優先します。Callback-triggered interruptionとdeadline expirationはcooperativeであり、そのcallback完了後にのみ観測します。成功したlimit付きexecutionは両方のcountと静的slot footprintを報告します。既存convenience APIはunbounded、non-interruptible、deadlineなしのままで、native／JIT executionに同等のenforcementはありません。Slotはbyte単位のheap chargeではなくVM register entryであり、host callbackのallocationはlimit外です。これはstable ABIではなくtest scaffoldingです。Aggregate value、error transport、panic containment、capability、heap／recursion／task limit、callbackのpreemptive interruption、serialized interfaceは未指定かつ未実装のままです。

実行モード間に意図的な差を設ける場合、RFC、必要に応じた診断、その境界を示すテストが必要です。

## ABIと互換性

プレアルファ期間中、安定したnative ABIは保証しません。producerとconsumerを同時にbuildする内部呼び出しでは、toolchain privateなlayoutを使用できます。

独立してversion管理されるコードでは、以下を定義するcomponent boundaryを使用します。

- canonicalなscalarおよびaggregate表現。
- 所有権移動とdestruction責任。
- error、panic、cancellation動作。
- string、collection、resource handle、callback表現。
- capability宣言とtrust level。
- version negotiationとadapter生成。

bytecodeとserialized MIRを永続化または配布する前に、明示的なformat versionを持たせます。未知のversionと不正な入力はfail closedにします。editionはソース互換性を維持できますが、component ABI contractを暗黙に変更しません。

## 診断とツール

compiler diagnosticは、テストまたはeditor integrationで使用された段階から安定したidentifierを使用します。人間向けの文言は機械可読codeとは独立して変更できます。

最初のfrontendは、formatterとLSPに必要なsource spanとrecovery情報を保持しなければなりません。formatter idempotence、parser/formatter round trip、VM/native差分実行、debugger source mappingは、それらを導入するmilestoneのrelease criteriaです。

自動化を意図したCLI commandは、安定化前にversion付きstructured-output modeを提供します。英語ドキュメントを正本とし、日本語ドキュメントは同じ要件を複製しますが、AI toolの第2のinstruction sourceにはしません。

## 性能とコストモデル

代表的なworkloadに対するbenchmarkと生成コード検査がない限り、zero-costの主張は受け入れません。測定では以下を分離します。

- parse、analysis、optimization、code generation、linkの時間。
- VM startup、JIT compilation、AOT compilation、steady-state execution。
- direct call、VM call、host call、callback、component adapter。
- owned value、推論borrow、atomic shared reference、arena、allocator traffic。
- debug buildとrelease build。
- macOS AArch64とWindows x86-64の結果。

benchmarkにはtoolchain version、commit identifier、hardware、OS、command line、varianceを記録します。プロトタイプ測定は意思決定に利用しますが、後続RFCでユーザー向けcost modelを定義しない限り、言語保証にはなりません。

## セキュリティとtrust boundary

ソーステキスト、package metadata、bytecode、component description、debug data、plugin inputは、明示的に信頼が確立されない限りuntrustedです。

- MIRとbytecodeは、実行またはnative loweringの前に検証する。
- VMはbytecodeを検証し、side effect前にprototype instruction-fuel／host-call budget、allocation前にregister-slot cap、明示的safe pointでcooperative interruptionまたは絶対deadlineを強制できる。より広いboundsとresource limitは段階的作業として残る。
- native pluginはtrusted codeであり、明示的なopt-inを要求する。
- untrusted pluginはcapability制限付きscriptまたはWebAssembly tierを使用する。
- 承認済み表現がない限り、panicとunwindingはFFIまたはcomponent boundaryを越えない。
- platform APIが厳密に限定された遷移を必要とする場合を除き、JIT memoryをwrite可能かつexecute可能な状態に同時にしない。
- parser、verifier、package、componentへの入力をuntrusted ecosystemへ公開する前に、fuzzingとdenial-of-service testを行う。

## 代替案

### 憲章を作らない

architecture documentから直接機能を実装する方法です。architecture proposalは承認権限、互換性領域、証拠要件を定義しないため採用しません。

### ライブラリまたはtooling policyのみ

repository conventionで不安定APIをlabel付けできます。しかしVM/native等価性、unsafe境界、ABI進化、editionはtoolchain全体の意味論に影響するため不十分です。

### LLVM互換native ABIを最初に安定化する

native-first設計ではC相互運用性を主要境界とし、後からscriptingを追加できます。初期code generationは単純になりますが、platform ABI detailが言語モデルになる危険があり、安全な同一プロセスpluginのversion管理が難しくなります。

### bytecode言語だけを先に作る

VM-only実装はscriptingを早く提供できます。より単純ですが、1つの意味表現でnative systems performanceとprocess境界のないembedded scriptを支えるという中心要件の検証を先送りします。

## 実装計画

2026-10-16時点のprototype状況：step 1〜3は、local `i64` vertical sliceについて完了しています。Step 3では型付き登録、VM／native差分実行、重複登録の拒否、未知関数の拒否、deterministic VM instruction fuel、host-call budget、allocation前のvalue-slot cap、cooperative interruption、絶対monotonic deadlineを扱いますが、stable embedding boundaryまたはcomponent boundaryは完成していません。Typed MIRとbytecodeは、branch、jump、return terminatorを持つ明示的basic blockを使用し、validationがtarget、reachability、SSA dominanceを検証し、VM fuelがtest済みback-edge loopを停止します。型付きblock parameterとedge argumentがphiに相当するmergeをmodel化し、test済みcountdown back-edgeを介して `i64` を運びます。Bounded sum workloadはdescending counterとwrapping accumulatorの両方をloopで運び、conditional-sum workloadは共有back-edgeの前でbranchしてadd／skip pathをmergeします。Validationはentry-block parameter、edge argumentのarity／type mismatch、dominateしないvalueを拒否し、VMとJITの結果が一致し、AOT pathも3つすべてのloop functionをemitします。Source allocationは未完了です。[RFC 0002](0002-prototype-control-flow.md)はdecision-readyな提案中のsource-level integer／structured-control-flow semanticsを提供しますが、parser、HIR、formatter、source diagnostics、source-to-MIR loweringは未実装です。Cranelift backendは、native hostまたは明示的なtarget triple向けに、exportされたMIR functionとimportされたhost callbackを持つobject artifactを生成できます。Host非依存testは `x86_64-pc-windows-msvc` のx86-64 COFF formatと期待するadd、accumulator、conditional-sum、callback export／import symbolを検証しますが、Windowsでのlinkとexecutionは未完了です。初期の共通 `SystemLinker` driverが順序付きrequestを検証し、非MSVC hostでは `cc -o`、MSVC hostでは `link.exe /OUT:` を選択し、明示的なdriver／flavor設定を許可してstructured failureを報告します。macOS AArch64 integration testはadd、minimum CFG、通常のcallback、host-allocation、accumulator、conditional-sum objectをこのdriver経由でlinkして、生成されたC harnessを実行します。Production driver hardeningとstable native ABIは未完了です。Step 4には、async borrowing／cancellation、cyclic graph、shared mutable serviceという3つすべてのstress caseについて実行可能な厳格modelがあります。Step 5には、add、minimum、loop-carried countdown、2-value accumulator、conditional-sumのbytecode lowering、Cranelift JIT／AOT compilation、生成code、object、link済みexecutable size、VM／native call、通常、single-allocation、64-allocation-burstのhost callback、unset-flag interruption-check／期限前deadline比較、system linking、single-launchとbatched warm-cache AOT process startupに関するcleanなmacOS AArch64 [schema 11 benchmark baseline](../benchmarks.md)があります。記録したAArch64 generated-code inspectionでは、意図した2-instruction add、compare-and-branch minimum、position-independentなhost importを確認しました。Windows x86-64でのlink、execution、benchmark、CI evidence、追加のcontrol-flowと代表的なallocation／memory workload、より広いembedding limit、native enforcement、より制限の少ないmemory-model代替案は未完了です。

CFG移行後の再検査では、minimumのcode／object sizeとinstruction sequenceが変わらないことを確認しました。`SystemLinker` は `Cc` と `MsvcLink` のcommand flavorをmodel化して対応するhost defaultを選択し、ordered prefix argument付きの明示driverをsupportし、完全なcommandを公開してstructured execution failureへ保持します。Ordered library search pathとlogical library nameをflavor固有argumentへloweringし、validationが空のobject path、search path、nameを拒否します。明示的なresponse-file linkは、`Cc` にGNU-escaped UTF-8、`MsvcLink` にWindows-escaped BOM-prefixed UTF-16LEを使用し、focused serialization testとmacOS direct／response-file executionが成功しています。Toolchain discoveryとより広いtarget policyは未完了です。cfg-gated Windows x86-64 MSVC direct／response-file object-link-execute testと完全SHAでpinしたWindows Server 2025 CI workflowを構成しましたが、このcommitにはrunner結果が記録されていません。

初期host-toolchain discoveryは、deterministicなcandidate順で `PATH` を探索し、Unixではexecutable fileを要求し、探索したpathとcandidateをstructured errorで報告します。macOS integrationは発見したdriverを使用し、cfg-gated MSVC integrationもdeveloper environmentが `PATH` を初期化した後に同じdiscoveryを使用します。初期化済みenvironment外のinstallation discoveryは未完了です。

初期tier-1 linker strategyは、backend-neutralな `LinkTarget` でcode化しました。Exactな `aarch64-apple-darwin` と `x86_64-pc-windows-msvc` tripleを、それぞれのcommand flavor、default program、ordered discovery candidateへ対応付けます。明示targetのlinker構築／discovery、両platform integration、cross-target COFF testがこの共有mappingを使用し、未supportのtripleはstructuredに失敗します。追加targetのpolicy、installation discovery、startup／runtime inputは未完了です。

Schema 3 benchmarkは、型付きcountdown back-edgeをlowering、JIT／AOT compilation、direct／VM／JIT call、artifact size、両tier-1 system-link pathへ追加します。Schema bumpは、追加fieldとlink-series renameを明示的に記録します。CleanなmacOS AArch64 schema 3 resultを記録しました。Windows CIはschema、clean-checkout metadata、target identity、拡張済みlink rowを検証してからJSONを30日間保持します。検査済みWindows resultがないため、これはまだcross-platform evidenceではありません。

Schema 4は、両tier-1 evidence pathへlink済みAOT executable sizeと反復warm-cache process-startup measurementを追加します。Process startupはsystem-link timeと分離したままで、process creation、loader処理、小さなharness、output collection、terminationを含みます。Operating-system cacheをflushせず、完全なruntime distributionを表すものでもありません。CleanなmacOS AArch64 resultは16,992-byte executableと1.511 ms median startupを記録しましたが、49.824 msの外れ値によりstartup RSDが276.388%になったため、budget設定前にmethodologyを改善する必要があります。Windows CIは新しいrowとsize fieldを検証しますが、remote Windows evidenceは未完了です。

Schema 5はsingle-launch startup rowを維持し、sampleが連続launch当たりの経過時間を報告する設定可能なbatched rowを追加します。両rowを維持することで、tail-sensitiveなindividual latencyとnoiseの影響を受けにくいamortized estimateを分離します。CleanなmacOS AArch64 resultは、1.487 msと1.499 msという近いmedianを維持しながら、10-launch batchのRSDを256.272%から4.688%へ下げました。Batch sizeはmachine-readable envelopeの一部で、Windows CIは両rowを検証しますが、remote Windows evidenceは未完了です。

Schema 6は、2-value accumulatorをbytecode lowering、JIT／AOT compilation、direct／VM／JIT call、生成code／object size、system linking、link済みexecutable size、両startup rowへ追加します。4-object link setに合わせてplatform seriesをrenameし、Windows CIは新しいaccumulator／platform fieldを検証します。CleanなmacOS AArch64 resultは全30 rowと17,056-byteのlink済みharnessを記録しましたが、remote Windows evidenceは未完了です。このimplementation evidenceによってRFCのproposed statusは変わりません。

Schema 6後のconditional-sum prototypeは、bounded loop内でbranchし、add／skip pathをblock parameterでmergeしてから共有back-edgeを取ります。VM／JIT differential test、macOS AArch64 direct／response-file link-and-run integration、host非依存Windows x86-64 COFF symbol validation、cfg-gated MSVC pathが新しいfunctionを扱います。そのsource-level semanticsとallocationは未完了であり、RFCはproposedのままです。

Schema 7 harnessはconditional-sum workloadをlowering、JIT／AOT compilation、direct／VM／JIT call、artifact size、5-object tier-1 system-link／startup setで扱います。Windows CIは対応するschema contractを検証します。CleanなmacOS AArch64 resultは全36 rowと17,104-byteのlink済みharnessを記録しました。Remote Windows evidenceは未完了であり、このprototype evidenceによってRFCのproposed statusは変わりません。

Async semantics prototypeは、厳格なdefaultの `RejectAll` suspension policyと、明示的な `AllowSharedFrameBorrows` 代替案を比較します。代替案はshared task-frame borrowをsuspend／resume越しに運び、exclusive borrowに対する `SEM0007` rejectionを維持し、cancellationがowned valueを破棄する前にactiveなshared borrowを終了します。これはより制限の少ない1つのdesignに関するevidenceであり、accept済みのownership／cancellation semanticsではありません。Pinning、suspensionを越えるexclusive borrow、source-level frame-residency proof、destructor failure、VM／native trace等価性、runtime costは未完了であり、review outcomeは引き続き **needs prototype/measurement** です。

Cyclic-graph prototypeは、defaultの `Reject` shared-cycle policyと、明示的な `CollectUnreachable` 代替案を比較します。Collectorはstrong cycleを許可し、root dropごとにliveなexternal rootからstrong edgeをtraceし、weak edgeではretainせず、到達不能なcycleを逆生成順に回収します。これにより最も強い競合graph designを実行可能にしますが、accepted済みのownership／reclamation semanticsではなく、同期的なmodelにとどまります。Production collectionのtrigger／latency、incremental／concurrent collection、allocation／peak-memory cost、finalization、weak-reference ordering、destruction observabilityは未完了であり、review outcomeは引き続き **needs prototype/measurement** です。

Shared-service prototypeは、defaultの `OwnerIsolated` mutation policyと、明示的な `ExclusiveCapability` 代替案を比較します。有効なhandleのholderは単一のactive mutation capabilityを取得でき、handleを持たないtaskまたは同時mutatorはstructured diagnosticでrejectされ、cancellationはnon-owner mutationが終了するまでservice stateのdropを待ちます。これにより1つのlock-likeな競合designを実行可能にしますが、accepted済みのconcurrency semanticsではありません。Read／write／atomic capability、capability derivation、scheduling fairness、priority inversion、poisoning、deadlock、reentrancy、panic handling、backpressure、VM／native trace等価性、runtime costは未完了であり、review outcomeは引き続き **needs prototype/measurement** です。

Host-allocation boundary prototypeは、single-allocation host functionと、64個のbox／512 payload byteを同時に保持してから合計・dropする2つ目のallocation functionを持つようになりました。VM／JIT差分testはRustの `Box` callbackをregisterし、macOS AArch64 direct／response-file AOT integrationと構成済みWindows MSVC pathは、別々のC `malloc`／`free` importを提供します。Schema 8はsingle-allocation callbackのdirect Rust／VM／JIT latency rowを追加し、schema 11はAOT process-startup setを変えずにburst用の同じ3 rowを追加します。Clean schema 12 macOS AArch64 resultのsingle-allocation medianはそれぞれ11.857 ns／187.240 ns／13.471 ns、burst medianは768.283 ns／946.601 ns／771.625 nsです。これは既存execution boundaryを越えて戻り値が等価になることを示し、より単純でないboundary-cost workloadを可能にしますが、source-languageのallocation／ownership semanticsではありません。Allocator／collector policy、failure／resource-limit behavior、allocator-overhead／process-level peak-memory accounting、より広い代表的workload、remote Windows evidenceは未完了です。RFCはproposedのままであり、review outcomeは引き続き **needs prototype/measurement** です。

VM resource-control prototypeは、optionalなhost-call budget、value-slot limit、共有one-way interruption flag、絶対monotonic deadlineをinstruction fuelと合成します。実行直前の各callbackは、そのbytecode instructionがfuelを消費した後に1 host-call unitを消費します。どちらかのcounterを使い切ると、そのcounterがguardするside effectの前にstructured errorを返します。検証済みregister countは、register fileを割り当てる前にvalue-slot limitと比較します。すでにsetされたinterruptionまたは期限切れdeadlineはallocation前に停止し、以降のconditionはinstruction、terminator、pre-callback safe pointで観測して、structured errorに両方の動的countを含め、interruptionを優先します。Callback-triggered requestまたはdeadline overrunは意図的にそのcallback完了後に観測します。成功したlimit付きexecutionは両方の動的countと静的slot footprintを報告します。Testはzero host-call budget、過小なslot limit、pre-execution interruption、callback-triggered interruption、pre-execution deadline expiration、running back-edgeを停止するdeadlineを扱います。Callback内部の処理はopaqueな1 unitであり、slotはbyte数を測定せずheapを扱いません。Heap memory、recursion、task、callback内部のpreemption、native／JIT enforcementは未完了です。これはaccepted sandbox semanticsを定義せずに、blockingなresource-control questionをさらに狭めるものです。Review outcomeは引き続き **needs prototype/measurement** です。

Benchmark schema 9は、既存VM countdown rowの隣に `vm_countdown_interruptible_call` を追加します。同一bytecode／inputをunsetの共有interruption flag付きで実行し、有効化したsafe pointのacquire-load costを直接計測できるようにします。現在のclean schema 12 macOS AArch64 runでは通常／interruption-enabled VM countdown medianが1.606 us／1.603 usでした。3.424 ns（0.213%）の減少は両rowの10.380%／0.263% RSDより小さいため、測定可能なregressionを示さず、overhead budgetも確立しません。Schema 12が現在のCI contractです。Remote evidenceは未完了です。

Benchmark schema 10は、harness準備の1時間後に設定した絶対monotonic deadline付きで同一bytecode／inputを実行する `vm_countdown_deadline_call` を追加します。Deadlineが期限前のままのsteady-state clock-check costを直接計測できます。Clean schema 12 macOS AArch64 runはこのrowを維持し、通常／期限前deadline付きVM countdown medianは1.606 us／3.314 us、1.708 usまたは106.379%の増加でした。Windows CIはこのrowを要求しますが、remote evidenceは未完了です。

Benchmark schema 11はschema 10 fieldをすべて維持しながら、64-allocation burst用のdirect Rust／VM／JIT rowを追加します。過去のcleanな31-sample macOS AArch64 runは44行すべてが一意であることと782.624 ns／962.329 ns／786.270 nsのburst medianを記録し、macOS AOT integrationはimport済みcallbackを実行し、host非依存testはそのWindows COFF importを検証します。Windows CIはschema 12で3 rowを維持しますが、remote Windows evidenceは未完了です。

Benchmark schema 12は44個のtiming rowを維持し、同じcallback workloadにbackend非依存のrequested-payload heap profileを追加します。安全なaccountingは `(allocation count, total bytes, peak live bytes, live bytes after)` をsingle allocationで `(1, 8, 8, 0)`、burstで `(64, 512, 512, 0)` と記録し、testとWindows CIが値を強制します。Cleanな31-sample macOS AArch64 runは両方の正確なprofileと44個すべての一意なrowを記録します。これはlogical payload lifetime／cleanupを測定するもので、allocator metadata、fragmentation、VM／JIT内部、process RSSではありません。Remote Windows evidence、allocator／process peak measurement、代表的allocation workloadは未完了であるため、review outcomeは引き続き **needs prototype/measurement** です。

CleanなmacOS AArch64の[AOT debug-information inspection](../../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)では、commit `013da31f9f0fb752aad5ec887f8cf5a5ecf1650d` からadd、minimum、host-callback objectを再生成しました。すべてexternal symbolを保持しますが、`__debug_*` section、DWARF compile unit、line entry、source variable、parameter、localはいずれも含みません。これはbackendをacceptせずに現在のdebug-quality floorを確立します。Source-span mapping、link済み／dSYM output、unwind metadata、Windows CodeView／PDB、JIT debugger registration、optimized stepping、代表的source programでのvalidationは未完了です。したがってreview outcomeは引き続き **needs prototype/measurement** です。

1. 最小の型付きMIR、bytecode lowering、verifier、VM、Cranelift JIT prototypeを追加する。
2. 開発host上で、小さな型付き関数をVMとnative executionに通して差分テストする。
3. 型付きhost-function registrationとscript-to-host callbackを追加し、embedded runtimeからcompiled functionを呼び出す。
4. 提案中のownership modelで、cyclic graph、async borrowing/cancellation、shared mutable serviceを試作する。
5. macOS AArch64とWindows x86-64でbackend、call overhead、code size、compile timeを記録する。
6. プロトタイプ証拠に基づき、arithmetic、ownership、effects/errors、concurrency、embedding、component ABIの焦点を絞ったRFCを作成する。
7. blocking question解決後に、この憲章をacceptまたは改訂する。

## テスト計画

- MIR validation、bytecode verification、VM execution、backend error handlingをunit testする。
- 同一の入力表をVMとnative codeで実行し、結果を比較する。
- deserialize導入前に、不正なMIRとbytecodeのテストを追加する。
- instruction数とtype数の増加に合わせてproperty testとfuzz testを追加する。
- 実装済み `i64` host callとcallbackについて、VM／native等価性、重複登録の拒否、未知関数の拒否をテストする。Panic containmentとcapability rejectionは、それらのmechanism導入時に追加する。
- Semantics実験では、stable diagnostic code、primary／secondary source span、deterministic event trace、正確なdestruction回数をテストする。どちらもaccepted behaviorとして提示せず、厳格policyとより制限の少ないpolicyのtraceを比較する。
- CIでformatting、警告を拒否するClippy、workspace全test、documentation buildを実行する。
- 縦断スライスをportableと宣言する前に、macOS AArch64とWindows x86-64 CIを追加する。
- benchmark入力とraw metadataを要約結果とともに保存する。

## 未解決事項

- `lang` に代わる恒久的なプロジェクト名と言語名は何か。
- 最初の公開pre-1.0 releaseで、どの互換性保証を開始するか。
- Windows AArch64を最初のnative backend milestoneの前と後のどちらでTier 1に加えるか。
- canonical ABI実験の出発点として、どのcomponent modelを利用するか。
- Byte単位のheap、recursion、task budgetをprototype instruction-fuel／host-call-budget／value-slot-limit／cooperative-interruption／absolute-deadline interfaceとどう合成するか。
- Craneliftをprototype backendにとどめずinitial backendとして選定するために、どの測定をblockingとするか。

レビュー結果：**needs prototype/measurement**。blocking questionはcomponent boundary、現在のVM counter、register-slot cap、cooperative interruption、absolute deadlineを超えるresource control、cross-platform／backend measurementです。命名とWindows AArch64の時期は、このRFCに対する任意のfollow-upです。

## 情報源

- `docs/research.md`、プロジェクト調査、最終更新 2026-10-15 22:29 JST。
- `docs/architecture.md`、設計baseline案、最終更新 2026-10-16 12:30 JST。
- `docs/roadmap.md`、実装milestone、最終更新 2026-10-16 12:30 JST。
- `crates/vm/src/lib.rs`、deadline-enabled VM resource-control prototype、最終更新 2026-10-16 11:20 JST。
- `src/lib.rs`、64-allocation burst boundary prototype、最終更新 2026-10-16 11:42 JST。
- `src/bin/prototype-bench.rs`、schema 12 requested-payload heap measurement harness、最終更新 2026-10-16 12:18 JST。
- `benchmarks/results/2026-10-16-macos-aarch64-fa9a69b.json`、clean schema 12 macOS AArch64 evidence、2026-10-16 12:24:45 JSTに計測。
- `benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md`、macOS AArch64 AOT debug-information inspection、2026-10-16 12:06:01 JSTに取得。
- Cranelift documentation、公開更新日のないliving documentation、2026-10-15確認：https://docs.rs/cranelift-codegen/0.136.3/
- Cranelift JIT documentation、公開更新日のないliving documentation、2026-10-15確認：https://docs.rs/cranelift-jit/0.136.3/
