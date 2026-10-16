<!-- translation-of: ../research.md; canonical-language: en -->

# 言語・エコシステム調査

[English](../research.md)

調査基準時刻：**2026-10-15 22:08 JST**。

このドキュメントは、現在の言語動向を要約し、このプロジェクトに必要な設計上の知見を抽出したものです。調査における人気度はエコシステムの根拠として扱い、言語機構が技術的に優れている証明としては扱いません。

## 開発者の評価

[2026 Stack Overflow Developer Surveyの言語データセット](https://survey.stackoverflow.co/2026/technology/data/da-language)は14,099人の回答者を対象としています。「Desired」は今後その言語を使いたい回答者、「Admired」は使用経験があり今後も使いたい回答者を示します。

| 言語 | 2026年の使用率 | Desired | Admired | このプロジェクトに対する示唆 |
| --- | ---: | ---: | ---: | --- |
| Rust | 18.0% | 26% | 67% | 安全なnative性能への強い需要と、十分な導入実績がある |
| C++ | 24.3% | 16% | 46% | 複雑さがあってもnative ecosystemと制御能力は重要である |
| C | 21.8% | 14% | 46% | 安定したC ABIは依然として普遍的な相互運用baselineである |
| Kotlin | 10.8% | 9% | 43% | 生産性の高い構文とtoolingには意味のある利用者層がある |
| Zig | 2.8% | 8% | 58% | ecosystemは小さいが、明示的なsystems designへの関心が強い |
| Swift | 5.9% | 5% | 49% | value semanticsと親しみやすいnative開発には魅力がある |
| Mojo | usage tableでは強調されていない | 2% | 45% | Python風の使いやすさ、ownership、native性能の組み合わせに初期の関心がある |

この調査はself-selectedであり、ecosystem全体を対象としています。意味論そのものではなく、ergonomics、tooling、interoperabilityの優先順位を決めるために使用します。

Rustは、検証済みの参照先として最も近い言語です。[2025 State of Rust Survey results](https://blog.rust-lang.org/2026/03/02/2025-State-Of-Rust-Survey-results/)は企業での採用継続を報告する一方、遅いcompile／storage使用量、debugging、言語の複雑化への懸念も示しています。[2025 compiler performance survey](https://blog.rust-lang.org/2025/09/10/rust-compiler-performance-survey-2025-results/)では、build performanceの平均満足度が6/10であり、compile bottleneckの診断が難しいこと、build時間を減らすためだけにsource変更が必要になることへの不満が報告されています。これらは本プロジェクトの直接的な要件になります。compile-time observability、incremental architecture、debugger品質を、言語が成熟した後まで先送りしてはいけません。

## 関連する言語設計

### Rust

[Rust Bookのreference-cycle章](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html)は、`Rc<T>` cycleが到達不能valueをretainし得ることと、`Weak<T>` がtargetのlivenessに寄与しないことを説明します。これはreference countingをleak-freeなgraph managementとして扱わず、shared ownershipとweak back-edgeを明示する根拠です。

採用するもの：

- Affine ownership、決定的destruction、明示的なunsafe境界、強力なdata-race prevention。
- Trait、algebraic data type、pattern matching、zero-cost generics、強力なpackage／build workflow。
- 言語体験の一部としてのcompile-fail testingとdiagnostics。

改善するもの：

- Lexical inference、value semantics、non-escaping borrowにより、通常のapplication codeからlifetime syntaxを排除する。
- 日常的なAPIの既定解として、高度なgeneric技法やmacro expansionを要求しない。
- Incremental compilation、build理由の説明、mixed-mode debugging、stable interfaceを最初から設計する。

### KotlinとKotlin/Native

[Kotlin/Native](https://kotlinlang.org/docs/native-overview.html)は、表現力の高い言語がLLVMを利用し、VMなしでbinaryを生成できることを示しています。[Memory-manager documentation](https://kotlinlang.org/docs/native-memory-manager.html)はshared heapとrootからのtracing collectionを規定するため、そのruntime modelはmandatory GCなしというcore objectiveを満たしません。

Null safety、expression-oriented syntax、extension function、exhaustive matching、coroutine、親しみやすいgenerics、IDE品質を採用します。JVM風のobject allocationを普遍的な抽象としては採用しません。

### Swift

[Swift memory safety](https://docs.swift.org/latest/documentation/the-swift-programming-language/memorysafety/)とownershipに関する取り組みは、exclusivity checkingとvalue-oriented APIの価値を示しています。[Swift ARC guide](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/LanguageGuide/AutomaticReferenceCounting.md)はstrong-reference cycleとweak／unowned relationshipを説明します。ARCは明示的borrowingより習得しやすい一方、retain／release trafficが増え、cycleへの規則が必要で、性能コストを隠す可能性があります。

Swiftはvalue semanticsと人間工学的なaccess ruleの根拠として使用します。Reference countingは、コストとcycle動作が型から分かる明示的なshared-reference typeを通じてのみ使用します。

### Zig

[Zig](https://ziglang.org/learn/overview/)は、明示的allocator、予測可能なcontrol flow、C interoperability、cross compilation、高速なdevelopment buildの有用な参照先です。公式ドキュメント自身が完全にmemory-safeな言語ではないと明記しています。そのためtoolingとexplicitnessの参照先であり、目標とするsafety modelではありません。

### Mojo

[Mojo ownership](https://docs.modular.com/mojo/manual/values/ownership/)は、single ownership、`read`、`mut`、`var`、`ref` のargument conventionと、value semanticsを既定で組み合わせています。Rustより通常の構文量を減らしてownershipを提示できる可能性を支持しますが、言語とecosystemはまだ発展中です。

### Hylo

[Hylo](https://hylo-lang.org/introduction/)は、本プロジェクトのmemory-model目標に最も直接的な研究参照です。Mutable value semantics、AOT native code、in-place mutation、safe by default、source modelにlifetime annotationがないことを掲げています。ただし成熟度とecosystemはproduction viabilityの証拠にはならないため、機構を独立してprototypeし、計測する必要があります。

### PonyとKoka

[Pony reference capabilities](https://tutorial.ponylang.org/capabilities/guarantees.html)は、aliasing guaranteeによりactor communicationをdata-race-freeにできることを示しています。[Koka](https://github.com/koka-lang/koka)はtyped effectとPerceus reference-count analysisを示しています。どちらも意味論上有用な参照ですが、全面的には採用しません。PonyはGCを使用し、Kokaのfunctional／effect-oriented modelは目標とするKotlin風surfaceから離れています。

## コンパイラ・相互運用基盤

### LLVM、Cranelift、MLIR

[LLVM IR](https://llvm.org/docs/LangRef.html)は、成熟したoptimization、object format、debug information、Windows／macOS target supportへの最有力経路です。[LLVM ORC](https://llvm.org/docs/ORCv2.html)は、in-process JITの構築とhost symbol integrationを提供します。

[Wasmtime経由のCranelift](https://docs.wasmtime.dev/stability-platform-support.html)はx86-64とAArch64をサポートし、AOTまたはJIT modeで動作できます。高速なcode generationとRust-native integrationを提供しますが、optimization surfaceはLLVMより小さいものです。

[MLIR](https://mlir.llvm.org/docs/LangRef/)は、拡張可能なdialect、高レベル／target-specific representation、dialect間のconversion、1つのmodule内で複数dialectが共存する仕組みを提供します。将来のmulti-language substrateとして有力です。一方、stage 0には推奨しません。最初の言語意味論を検証する前に、大きなC++ dependency、追加のversioning surface、大量のinfrastructureが必要になるためです。

推奨backend導入順序：

1. Backend-neutralなtyped MIRとbytecode VMを定義する。
2. 高速なiterationのため、Craneliftでnative object generationをprototypeする。
3. 代表的benchmarkによって統合コストが正当化された場合のみ、LLVM release backendを追加または置換する。
4. 2つ目のfrontendまたはdomain-specific optimizationが実在する段階でMLIRを再評価する。

### WebAssembly Component Model

[WebAssembly Component Model](https://component-model.bytecodealliance.org/design/why-component-model.html)は、WIT interfaceと[Canonical ABI](https://component-model.bytecodealliance.org/advanced/canonical-abi.html)を使用して、言語非依存のcontractを定義します。Version付きcross-language componentとsandbox指向pluginの現時点で最良の参照です。

このプロジェクトではinterface modelとcapability disciplineを取り入れます。ただし、すべてのcomponent boundaryでzero overheadを約束しません。String、list、resource、validation、isolationにはlowering、check、copy、handleが必要になる場合があります。信頼済みでstatic linkされるcodeは、whole-program validation後にspecialized direct callを使用できます。

## Tooling標準

- コンパイラと同じincremental query database上に[LSP 3.18](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/)を実装する。
- Editorが言語固有debug UIを必要としないよう、[Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/)を実装する。
- macOSではDWARF、WindowsではCodeView／PDBを出力し、別にVM source mapとmixed-stack modelを保持する。
- Formatterのsyntax treeをlosslessにし、parser recoveryをLSPと共有する。

## 追加すべき強い要件

元の要件には、次を追加する必要があります。

1. **文書化された言語仕様とmemory model。** Safetyをcompiler内の暗黙知に依存させない。
2. **安定したC ABI interoperability。** C header／importはOSと既存libraryへの実用的なbridgeである。
3. **Version付きcomponent ABIとIDL。** Native source-language ABIをplugin compatibilityの約束にしない。
4. **Plugin securityとcapability。** In-process executionはIPC overheadを除く一方、process isolationも失う。
5. **Resource limitとcancellation。** Scriptにはfuel／time、memory、recursion、task、host-call limitが必要である。
6. **説明可能な高速incremental build。** どのunitがなぜrebuildされ、compile timeがどこで消費されたかを把握できるようにする。
7. **Compatibility policyとedition。** Syntax、package、bytecode、ABI、serialized IRを独立してversion管理する。
8. **Reproducible／hermetic build。** Lockfile、checksum、offline mode、provenance、deterministic outputはsupply-chain要件である。
9. **APIとしてのdiagnostics。** Stable error code、span、fix-it、machine-readable output、localization-ready messageがIDEとCIを支える。
10. **完全なunsafe／FFI model。** Pointer provenance、aliasing、atomic、unwinding、callback、ownership transferを仕様化する。
11. **Compile-time execution limit。** Macroとconstant evaluationをdeterministic、cacheable、resource-boundedにする。
12. **Profilingとobservability。** Runtime profile、allocation trace、VM／native mixed stack、compiler timing dataをdeveloper experienceに含める。
13. **Fuzzingとconformance suite。** 複数backendと将来のfrontendにdifferential／compatibility testingが必要である。
14. **Documentation generationと検索可能なpackage metadata。** 言語ecosystemには最初のpublic packageからAPI docsが必要である。
15. **Migration tooling。** Ecosystemを分断せず進化させるため、editionとautomatic fixerが必要である。

## 情報源一覧

| 情報源 | 公開／更新日 | 確認記録 |
| --- | --- | --- |
| [Stack Overflow Developer Survey 2026 language data](https://survey.stackoverflow.co/2026/technology/data/da-language) | Dataset year 2026、pageの正確なtimestampは非公開 | 2026-10-15確認 |
| [Stack Overflow Developer Survey 2026 usage data](https://survey.stackoverflow.co/2026/technology/data/languages) | Dataset year 2026、pageの正確なtimestampは非公開 | 2026-10-15確認 |
| [2025 State of Rust Survey Results](https://blog.rust-lang.org/2026/03/02/2025-State-Of-Rust-Survey-results/) | 2026-03-02 | 2026-10-15確認 |
| [Rust compiler performance survey 2025](https://blog.rust-lang.org/2025/09/10/rust-compiler-performance-survey-2025-results/) | 2025-09-10 | 2026-10-15確認 |
| [Rust Book：Reference Cycles Can Leak Memory](https://doc.rust-lang.org/book/ch15-06-reference-cycles.html) | Source最終変更2025-10-03 | 2026-10-16確認 |
| [Kotlin/Native overview](https://kotlinlang.org/docs/native-overview.html) | 2026-09-01 | 2026-10-15確認 |
| [Kotlin/Native memory management](https://kotlinlang.org/docs/native-memory-manager.html) | Living documentation、日付は非公開 | 2026-10-16確認 |
| [Zig overview](https://ziglang.org/learn/overview/) | pageに記載なし | 2026-10-15確認 |
| [Mojo ownership](https://docs.modular.com/mojo/manual/values/ownership/) | pageに記載なし | 2026-10-15確認 |
| [Hylo introduction](https://hylo-lang.org/introduction/) | pageに記載なし | 2026-10-15確認 |
| [Swift memory safety](https://docs.swift.org/latest/documentation/the-swift-programming-language/memorysafety/) | Living documentation、日付は非公開 | 2026-10-15確認 |
| [Swift Book：Automatic Reference Counting](https://github.com/swiftlang/swift-book/blob/main/TSPL.docc/LanguageGuide/AutomaticReferenceCounting.md) | Source最終変更2026-01-09 | 2026-10-16確認 |
| [MLIR language reference](https://mlir.llvm.org/docs/LangRef/) | Living documentation、日付は非公開 | 2026-10-15確認 |
| [LLVM ORC design](https://llvm.org/docs/ORCv2.html) | Living documentation、日付は非公開 | 2026-10-15確認 |
| [Wasmtime platform support](https://docs.wasmtime.dev/stability-platform-support.html) | Living documentation、日付は非公開 | 2026-10-15確認 |
| [WebAssembly Component Model rationale](https://component-model.bytecodealliance.org/design/why-component-model.html) | Living documentation、日付は非公開 | 2026-10-15確認 |
| [LSP 3.18 specification](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/) | Version 3.18.0、2026-06-04 | 2026-10-15確認 |
| [Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/) | Specification 1.71.0、日付は非公開 | 2026-10-15確認 |
| [GitHub Docs：Building and testing Rust](https://docs.github.com/en/actions/tutorials/build-and-test-code/rust) | Living documentation、日付は非公開 | 2026-10-16確認 |
| [GitHub Docs：Secure use reference](https://docs.github.com/en/actions/security-for-github-actions/security-guides/security-hardening-for-github-actions) | Living documentation、日付は非公開 | 2026-10-16確認。read-only token permissionと完全SHAのaction pinに使用 |
| [GitHub Windows Server 2025 runner image](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-Readme.md) | Image version 20261004.281.1、page更新timestampは非公開 | 2026-10-16確認 |
| [actions/checkout v7.0.1 commit](https://github.com/actions/checkout/commit/3d3c42e5aac5ba805825da76410c181273ba90b1) | 2026-07-17 | 2026-10-16に `v7` tagの参照先として確認 |
| [ilammy/msvc-dev-cmd v1 commit](https://github.com/ilammy/msvc-dev-cmd/commit/0b201ec74fa43914dc39ae48a89fd1d8cb592756) | 2024-01-01 | 2026-10-16に `v1` tagの参照先として確認 |
| [GCC：Overall Options](https://gcc.gnu.org/onlinedocs/gcc/Overall-Options.html) | Living documentation、日付は非公開 | 2026-10-16確認。`@file`、whitespace separation、quoting、escaping、recursive expansionを定義 |
| [MSVC LINK：linker response fileの指定](https://learn.microsoft.com/en-us/cpp/build/reference/at-specify-a-linker-response-file?view=msvc-170) | 2022-05-18 | 2026-10-16確認 |
| [rustc linker response-file implementation](https://github.com/rust-lang/rust/blob/364c4856ae0eb8e5b058a2c6699749d1e3efa7ef/compiler/rustc_codegen_ssa/src/back/link.rs#L2100) | File最終変更2026-10-08 | 2026-10-16確認。GNU／Windows escapingとMSVC UTF-16LE encodingの先例として使用 |
| [GitHub Docs：workflow artifactによるdataの保存と共有](https://docs.github.com/en/actions/tutorials/store-and-share-data) | Living documentation、日付は非公開 | 2026-10-16確認。workflow-artifact uploadとretention設定に使用 |
| [actions/upload-artifact v7.0.2 commit](https://github.com/actions/upload-artifact/commit/cf430e030ddbb5b0abf93d22962f4752f3646cd9) | 2026-10-07 | 2026-10-16にlatest release tagの参照先として確認 |

公開更新時刻のないLiving Documentationについては、CDN responseやsearch indexから日付を推測せず、意図的に「非公開」と記載しています。
