<!-- translation-of: ../../../docs/rfcs/0001-project-charter.md; canonical-language: en -->

# RFC 0001：プロジェクト憲章と互換性ポリシー

[English](../../../docs/rfcs/0001-project-charter.md)

- ステータス：accepted
- 著者：プロジェクトコントリビューター
- 作成日：2026-10-15
- 最終更新日：2026-10-17
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
- 受理済みRFCの範囲外のprototype挙動を言語意味論として扱うこと。

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
- 評価／overflowはRFC 0002、component failure boundaryはRFC 0003、ownership／destructionはRFC 0004、structured cancellation／isolationはRFC 0005の受理済み規則に従う。その範囲外の詳細は未規定であり、追加RFCを必要とする。
- backend最適化は、承認済みの観測可能な動作を変更してはならない。
- capability検査とtrust boundaryは意味入力であり、任意のdeployment documentationではない。
- compile-time executionを導入する場合、明示的なcapability modelとresource modelを持たせる。

RFC 0002は最初のsource subsetのwrapping `i64` 加算を受理しています。将来の数値型や演算のoverflowを規定するものではありません。

## 実行モード

初期実行モデルは1つの意味入力と複数のloweringを持ちます。

- bytecode VMは、検証済みMIRからloweringされた検証済みregister bytecodeを利用する。
- native実行ではCraneliftを使い、lowering、host calling convention、同一プロセス実行を検証する。
- 本番AOT経路では、bundled linkerが正当化されるまでobject fileを出力し、system linkerを使用する。
- startup time、memory、security、debugging動作を測定するまで、JITは開発・埋め込み実験とする。
- 埋め込みhost callとcallbackは、最終的に型なしraw pointerではなく、型付き登録と生成adapterを利用する。
- WebAssembly/component実行は将来のsandbox tierであり、最初のプロトタイプには必須でない。

当初のscalar embedding実験では、各host functionにbackend非依存のMIR identifierを割り当て、runtimeごとのregistryから型付き `extern "C" fn(i64) -> i64` を解決します。VMはそのregistry経由でdispatchし、Craneliftは同じ登録済み関数をJIT moduleへimportします。VMは合成可能なopt-in instruction-fuel／host-call budget、value-slot limit、共有one-way interruption flag、絶対monotonic deadlineも試作します。各instruction／terminatorと実行に入るcallbackは、それぞれのcounterで1 unitを消費し、各動的limitは対応するside effectの前に確認します。Validationとarity checkの後、検証済みregister countをregister-file allocation前に確認します。すでにsetされたinterruptionまたは期限切れdeadlineはそのallocation前に停止し、以降のconditionはinstruction、terminator、pre-callback safe pointで観測して、structured errorに両方の消費counterを含め、interruptionを優先します。Callback-triggered interruptionとdeadline expirationはcooperativeであり、そのcallback完了後にのみ観測します。成功したlimit付きexecutionは両方のcountと静的slot footprintを報告します。Legacy scalar convenience APIはunbounded、non-interruptible、deadlineなしのままで、native／JIT executionに同等のenforcementはありません。Slotはbyte単位のheap chargeではなくVM register entryであり、host callbackのallocationはlimit外です。このscalar実験はstable ABIではなくtest scaffoldingです。現在はRFC 0003がcomponentのownership／failure boundaryとcandidate serializationを規定し、実行可能なprototypeがあります。RFC 0006はsource aggregate、portable bytecode、既定のfuel／host-call／frame／live-cell上限を持つVMを実装しています。Backend全体のproduction heap／task accounting、豊富なcomponent call、callbackのpreemptive interruptionは実装範囲外です。対応範囲は現在の[機能ガイド](../milestone-1.md)を参照してください。

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

Milestone 0はprototype／設計の範囲で完了しています。RFC 0001–0005は2026-10-17に受理され、検証済みMIR、VM／JIT差分実行、tier-1 AOT link／run、component／resource／failure fixture、native debug format evidenceがinitial backend decisionを支えます。Source frontendの実装やABI安定化を意味しません。

現在の依存順実装taskは[roadmap](../roadmap.md)、schemaごとの計測履歴とraw artifact linkは[benchmark記録](../benchmarks.md)を正とします。以前のWindows evidence不足やRFC提案中という記述は過去revisionの状態であり、現在のdecision gateではありません。過去のJIT／application計測は2026-10-17のJIT memory回収修正前のため、現在の性能を主張する前に再計測が必要です。

## Initial native backend decision

**Project decision（2026-10-17）：initial JIT／AOT backendとしてCraneliftを選定します。** 完了したmacOS AArch64／Windows x86-64 evidenceは、共有MIRについてVM／JIT equivalence、native object emission、direct／response-file system link、process execution、host callback、typed component／resource cleanup、source-mapped DWARF／dSYMおよびCodeView／PDB artifact、schema-20 performance／memory measurementを実行します。最初のfrontend／ABIがstableになる前に第2backendを追加すべきblockerは確認されませんでした。

既存backendは両execution modeをすでに提供し、backend-neutralなMIR、artifact、source-map、linker interfaceの背後に隔離されているため、このdecisionは現在の実装／保守riskを最小化します。Cranelift固有ABIをstableと約束せず、現在のbenchmark値を保証せず、LLVMを恒久的にrejectしません。代表的release workloadで、Cranelift pathでは合理的に解決できないoptimization、target support、debug quality、integration要件が判明した場合にLLVMまたは別backendを再評価します。

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
- WIT互換性を早計に主張せず、受理済みRFC 0003 profileを将来のWebAssembly component targetとどう相互運用させるか。
- Production heap／live-task accountingをnative code、source VMのframe／live-cell上限、host callback全体でどう合成するか。RFC 0006は既にguest recursionの上限を定義しており、cross-mode accountingはMilestone 3に残ります。

Decision：RFC 0003のacceptance後、repository ownerが**2026-10-17にaccept**しました。このacceptanceはproject charter、compatibility policy、evidence requirement、Craneliftをinitial JIT／AOT backendとする結果的なarchitecture baselineを承認します。未実装のsource、ABI、package、serialized-format detailをstabilizeするものではありません。命名とWindows AArch64の時期は任意のfollow-upです。

## 情報源

- Repository-owner acceptance、2026-10-17 02:25 JSTに記録。
- `docs/research.md`、プロジェクト調査、最終更新 2026-10-15 22:29 JST。
- `docs/architecture.md`、initial-backend decision／accepted設計baseline、最終更新 2026-10-17 02:25 JST。
- `docs/roadmap.md`、実装milestoneとMilestone 0完了、最終更新 2026-10-17 02:25 JST。
- `crates/vm/src/lib.rs`、deadline-enabled VM resource-control prototype、最終更新 2026-10-16 11:20 JST。
- `src/lib.rs`、allocation burst／churn／variable-size boundary prototype、最終更新 2026-10-16 14:12 JST。
- `src/bin/prototype-bench.rs`、schema-20 cross-platform benchmark harness、最終更新 2026-10-17。
- `benchmarks/results/2026-10-16-macos-aarch64-6a2caf8.json`、clean schema 14 macOS AArch64 evidence、2026-10-16 14:02:20 JSTに計測。
- `benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md`、macOS AArch64 AOT debug-information inspection、2026-10-16 12:06:01 JSTに取得。
- `benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md`、macOS AArch64 source-mapped DWARF inspection、2026-10-16 19:41:57 JSTに取得。
- `benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md`、Windows x86-64 source-mapped CodeView／PDB inspection、2026-10-16 20:19:36 JSTに取得。
- `benchmarks/results/2026-10-16-macos-aarch64-de248a1.json`、clean schema-20 macOS AArch64 evidence、2026-10-16 23:34:00 JSTに計測。
- `benchmarks/results/2026-10-17-windows-x86_64-7c37d5d.json`、[GitHub Actions run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398)によるclean schema-20 Windows x86-64 evidence、2026-10-17 01:45:50 JSTに計測、runは2026-10-17完了。
- Cranelift 0.136.3 documentation、公式package documentation／公開更新日のないliving documentation、2026-10-17確認：https://docs.rs/cranelift-codegen/0.136.3/
- Cranelift JIT documentation、公開更新日のないliving documentation、2026-10-15確認：https://docs.rs/cranelift-jit/0.136.3/
