<!-- translation-of: ../roadmap.md; canonical-language: en -->

# 実装ロードマップ

[English](../roadmap.md)

このロードマップはrisk reductionの順に並んでいます。Milestoneはcalendar commitmentではなくexit criteriaです。

## Milestone 0：設計baseline

- Goal、non-goal、terminologyを承認する。
- RFC processとcompatibility labelを確立する。
- Cyclic graph、async borrow／cancellation、shared mutable serviceという3つのmemory-model stress caseをprototypeする。
- Typed functionをbytecodeとnative loweringの両方でprototypeする。
- 初期tier-1 architecture pairとlinker strategyを選定する。

Exit：prototype subsetの意味論が文書化され、初期backend選定を支持するbenchmark evidenceがある。

## Milestone 1：frontendとbytecodeのvertical slice

- Error recoveryとsource spanを持つlossless parser。
- Name resolution、module、primitive type、function、record、variant、pattern matching。
- Stable codeとfix-itを持つstructured diagnostics。
- Validationを持つtyped MIR。
- Register bytecode、verifier、VM、REPL／script runner、host function registration。
- Formatterとparser／formatter round-trip test。
- LSP prototypeと共有する初期incremental query graph。

Exit：小さなprogramをscriptとして実行でき、deterministic behaviorと有用なIDE diagnosticsがある。

## Milestone 2：ownershipとnative AOT

- Owned value、move、lexical borrow、deterministic destruction、狭いunsafe operation。
- Allocator-aware standard container。
- C ABI import／exportとgenerated header。
- macOS AArch64とWindows x86-64向けCranelift object generation。
- VM／native differential execution suite。
- Breakpointとstack traceに十分なnative debug metadata。

Exit：代表的なcommand-line programがVM semanticsと一致し、同等のRust／C実装に対してperformanceが計測されている。

## Milestone 3：concurrencyとembedding

- Structured task、cancellation、channel、actor isolation、data-race capability rule。
- Stable C embedding APIとgenerated binding。
- Version付きcomponent IDL、resource handle、capability import。
- Mixed native／VM call、callback、error、stack trace、cancellation。
- Script resource limitとinterruption。
- Trusted native／verified script pluginのexample。

Exit：Native hostがuser scriptをloadし、typed APIを公開し、scriptをcallし、callbackを受け取り、IPCなしでcapabilityを強制できる。

## Milestone 4：developer toolchain

- Navigation、completion、rename、semantic token、code actionに対応した完全なLSP機能。
- Mixed native／VM stackを持つDAP adapter。
- `lang` build／package／test／fmt／lint／doc command。
- Deterministic resolution、checksum、offline mode、target conditionを持つmanifest／lockfile。
- Compile-time／runtime profiling、allocation trace、rebuild explanation。
- `rustup` と同じ役割を持つinstall／update／channel tool。

Exit：新しいdeveloperが1つのtoolをinstallし、packageを作成し、editorで開発し、test／debugし、reproducible binaryを生成できる。

## Milestone 5：ecosystemとcompatibility

- Edition、deprecation policy、migration／fix tool、compatibility test matrix。
- Signed release manifestとpackage provenance。
- Documentation hostingとpackage metadata search。
- Upload受付前のpublic registry threat model。
- Sandboxed portable plugin向けWebAssembly component target。
- 公開benchmarkとdebug品質基準によるLLVM release backendの評価。

Exit：Source、package、bytecode、component consumerを暗黙的に破壊せず言語を進化させられる。

## Milestone 6：セルフホスティング

- Self-hosting subsetを定義してfreezeする。
- 新言語でfrontendとcore compiler stageを実装する。
- Stage 0 Rust compilerでstage 1をbuildし、stage 1でstage 2をbuildする。
- 両tier-1 platformでreproducible outputを比較する。
- Bootstrap trustとdisaster-recovery procedureを文書化する。

Exit：Stage 2がstage 0と同じconformance suiteに合格し、文書化されたbootstrap chainからsupport対象toolchainをbuildできる。

## Milestone 7：追加frontend

- 明確に別のhigh-level／domain-specific languageが、library、profile、syntax sugarでは満たせない要件を持つかreviewする。
- Frontend-specific HIRを維持し、coreが忠実に表現できる意味論だけをshared MIRへloweringする。
- Memory、effect、runtime modelが異なる場合はcomponent ABIを使用する。
- Interoperabilityをstableと宣言する前に、cross-language debugging、package metadata、conformance testを追加する。

Exit：2つ目のfrontendが明確なuser valueを追加し、最初の言語の安全性を弱めず、shared IRを無関係な意味論の保守不能な集合にしない。

