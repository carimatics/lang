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

現在の設計仮説は、完全なビジョンより意図的に範囲を絞っています。

1. まず1つの言語を作ります。最初から2つの言語を作るのではなく、安全な高レベルコードと明示的な低レベル機能を同じ意味モデルで提供します。
2. すべての実行モードで、構文解析、型検査、所有権解析、型付き中間表現を共有します。
3. 共通MIRから、スクリプト向けのポータブルなバイトコードVMと、AOT向けのネイティブオブジェクトコードへloweringします。
4. VMの意味論、デバッグ情報、埋め込みAPIが安定してからJITコンパイルを追加します。
5. バージョン付きインターフェース記述とcanonical component ABIを定義します。レイアウトが判明している場合は直接内部呼び出しを使用し、独立してバージョン管理される境界では生成されたthunk／handleを使用します。
6. 信頼済みネイティブプラグインと、capabilityで制限されたスクリプト／WebAssemblyプラグインを別のデプロイ層として扱います。
7. セルフホスティングは、初期実装を制約する条件ではなく、再現可能なブートストラップのマイルストーンとして扱います。

根拠と未解決のトレードオフは、[アーキテクチャ提案](docs/ja/architecture.md)に記載しています。

## ドキュメント

- [言語・エコシステム調査](docs/ja/research.md)
- [Prototype benchmark evidence](docs/ja/benchmarks.md)
- [アーキテクチャ提案](docs/ja/architecture.md)
- [実装ロードマップ](docs/ja/roadmap.md)
- [RFCプロセスとテンプレート](docs/ja/rfcs/0000-template.md)
- [RFC 0001：プロジェクト憲章と互換性ポリシー](docs/ja/rfcs/0001-project-charter.md)
- [RFC 0002：Prototype integer／structured control-flow semantics](docs/ja/rfcs/0002-prototype-control-flow.md)
- [コントリビューションガイド](CONTRIBUTING.ja.md)

## 開発

必要な環境：

- `rust-toolchain.toml` で固定されたRust `1.99.0`
- 固定ツールチェーンから導入される `rustfmt` とClippy
- リポジトリの補助ツールバージョンを利用する場合は `mise`

現在のVM/native等価性プロトタイプを実行します。

```bash
cargo run -- 20 22
```

このcommandは、検証済みの型付きMIR関数を構築し、検証済みregister bytecodeへloweringしてVMで実行し、同じMIRをCranelift JITでcompileして結果を比較します。wrapping `i64` 加算はプロトタイプ動作であり、承認済み言語意味論ではありません。

Workspace testでは、最小のembedding経路も検証します。backend非依存のhost-function IDから型付き `extern "C" fn(i64) -> i64` 登録を解決し、同じMIR host callをVMとCranelift JITの両方で実行します。VMは合成可能なopt-in instruction-fuel／host-call budgetに加えて、value-slot limit、optionalな共有one-way interruption flag、絶対monotonic deadlineを公開します。成功したlimit付きexecutionは、両方の消費countと静的なregister-slot footprintを報告します。各bytecode instructionとterminatorは1 fuel unit、実行に入る各callbackは1 host-call unitを消費し、どちらの動的limitも対応するside effectの前に確認します。Validationとarity checkの後、VMはregister fileを割り当てる前に、register countがslot limitを超えるprogramを拒否します。Interruption／deadline checkはregister-file allocation前とinstruction、terminator、callback boundaryで行い、structured errorが両方の消費countを報告します。同じsafe pointで両方のconditionが成立する場合はinterruptionを優先します。Callbackはinterruptionをrequestしたりdeadlineを超過したりでき、そのcallback後の次のsafe pointで観測します。既存convenience functionはunbounded、non-interruptible、deadlineなしのままです。Slot countはbyte単位のheap limitではなく、callbackのallocationを含みません。このprototypeはstable ABIを定義せず、aggregate value、error transport、panic containment、capability、heap／recursion／task limit、callbackのpreemptive interruption、native／JIT resource enforcementはまだ提供しません。

`lang-codegen-clif` は同じMIRからobject artifactを生成し、compiled functionをexportし、host callbackをimportとして残すこともできます。Host-native pathに加えて、callerは明示的なtarget tripleを選択できます。Host非依存testは `x86_64-pc-windows-msvc` COFF objectを生成し、そのarchitecture、format、exportされたfunction、callback importを検証します。Root crateは、object、library search path、logical libraryの順序を保持し、空でないinputを検証する初期 `LinkRequest`／`SystemLinker` layerを提供します。Linkerは非MSVC hostで `-o`、`-L`、`-l` を使う `cc`、MSVC hostで `/OUT:`、`/LIBPATH:`、`.lib` nameを使う `link.exe` を選択します。Callerは代わりに明示的なdriver、command flavor、ordered prefix argumentを選択できます。Linkerは実行前に完全なcommandを公開し、structuredなspawn、exit-status、missing-output failureへそのcommandを含めます。Callerは明示的なresponse-file pathも選択できます。`Cc` argumentにはGNU escaping付きUTF-8、`MsvcLink` argumentにはWindows escaping付きBOM-prefixed UTF-16LEを使用します。macOS AArch64 integration testは、共通pathを介してdirect linkとresponse-file linkの両方を実行します。Windows x86-64 MSVC integration testは、`cl.exe` でC harnessをcompileし、両modeでadd、minimum、host-callback、2-value loop-accumulator、conditional-sum objectをlinkして実行し、完全SHAでpinしたWindows Server 2025 CI workflowで走るよう構成されています。このcommitにはWindows runnerの結果が記録されていないため、Windowsでのlink／executionとbenchmark evidenceは、toolchain discovery、より広いtarget policy、startup code、stable native ABI、debug metadataとともに未完了です。

2-value accumulator prototypeは、countdown valueとwrapping sumの両方をback-edgeのblock parameterで運びます。Conditional-sum prototypeはそのloop内にbranchを追加し、別のblock parameter pairを介してadd／skip pathをmergeします。VM／JIT differential testはnon-positive input、両conditional path、bounded positive loopを扱い、macOS AArch64 direct／response-file integrationは両AOT objectを実行し、host非依存Windows testはexportされたCOFF symbolを検証します。これらのprototypeはsource-level loop syntaxやallocation semanticsを定義せずにMIR CFG evidenceを広げます。

`SystemLinker::discover_host` は、host flavorのcandidate（`cc`、`clang`、`gcc`、またはMSVCの `link.exe`）を `PATH` から探索し、Unixではnon-executable fileをskipし、探索したpathとcandidateをstructured errorで報告します。Platform integration testは、発見したexecutableを使用します。初期化済みtoolchain environmentの外側にあるVisual Studio、SDK、compiler installationのdiscoveryは未完了です。

`LinkTarget` は、`aarch64-apple-darwin` と `x86_64-pc-windows-msvc` に対する初期tier-1 linker policyを一元化します。各exact tripleをlinker flavor、default program、ordered discovery candidateへ対応付け、未supportのtripleをstructuredに拒否し、`SystemLinker` による明示targetの構築とdiscoveryを駆動します。Object-format testとcfg-gatedの両platform integrationは、この共有policyを使用します。追加targetのpolicyは未完了です。

Schema 9 benchmark harnessは、add、minimum、通常のhost-callback、host-allocation callback workloadに加えて、block parameter、内部branch、merge、back-edgeを持つbounded countdown／2-value accumulator／conditional-sum loopを計測します。Unset interruption flag付きの2つ目のVM countdown rowを追加し、そのsafe-point-check costを既存のuninterruptible VM rowと直接比較できるようにします。また、macOSの `cc` とWindows MSVCの `link.exe` という両tier-1 hostでsystem linking、linked executable size、single-launch process startup、設定可能なbatched startupを計測します。CleanなmacOS AArch64 schema 9 baselineでは通常のVM countdownが1.588 us、unset interruption flag付きが1.592 usで、medianは3.409 ns、0.215%増加しました。このbaselineはdeadline controlより前のもので、deadline check costは計測していません。Remote Windows resultはまだ記録していません。Windows CIは文書化済みparameterでschema 9を実行し、clean-checkout metadata、target identity、interruption／allocation／conditional-loop／platform row、batch size、正のexecutable sizeを検証して、JSONを30日間workflow artifactとして保持します。

Typed MIR、bytecode verifier、VM、JIT、object pathは、signed `i64` comparisonと明示的control-flow graphも共有します。2引数minimum functionはentry blockから個別のreturn blockへ分岐し、焦点化されたjump testが明示的edgeを検証します。型付きblock parameterとedge argumentは、専用phi instructionなしでphiに相当するmergeを表現します。MIRとbytecodeのvalidationは、invalid target、unreachable block、entry-block predecessorまたはparameter、edge argumentのarity／type mismatch、useをdominateしないvalueを拒否します。Countdown workloadはback-edgeを介して `i64` を運び、VMとJITで同じ結果を生成し、object pathもこれをemitします。Bytecode back-edge testは、instruction fuelが非停止loopを停止することを証明します。Allocationとsource-level control-flow semanticsは未実装です。

Symbolとdisassemblyを検査するため、どちらかのinspection targetをhost-native object fileへ書き出します。

```bash
cargo run --release --bin prototype-object -- add /tmp/prototype-add.o
cargo run --release --bin prototype-object -- min /tmp/prototype-min.o
cargo run --release --bin prototype-object -- host-callback /tmp/prototype-host-callback.o
```

このcommandはadd、signed-minimum、host-callback targetについて、exported symbol、生成function-code size、object全体のsizeを報告します。Linkerは実行しません。

実行可能なownership実験は、`cargo test -p lang-semantics` で個別に実行できます。Async modelは厳格な `RejectAll` suspension policyと、exclusive-borrow rejection／deterministic cancellation cleanupを維持しながらshared task-frame borrowを許可する明示的な `AllowSharedFrameBorrows` 代替案を比較します。Graph modelはdefaultの `Reject` shared-cycle policy、実験的な `CollectUnreachable` tracing policy、arena-scoped cyclic storageを比較します。Service modelは `OwnerIsolated` mutationと `ExclusiveCapability` 代替案を比較しつつ、sendable payloadのmove、FIFO mailbox、deferred cancellation cleanupを維持します。これらのmodelは設計証拠を記録するだけで、承認済み言語意味論ではありません。

クリーンなrelease buildからprototype backend measurement harnessを実行します。

```bash
cargo run --release --bin prototype-bench -- \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000 \
  --startup-iterations 10
```

Harnessは、commitとdirty state、toolchain、host、system-linker metadata、生成されたJIT code／AOT object size、link済みexecutable size、lowering、JIT／AOT compilation、direct／VM／JIT call、host callback、control flow、linking、process startupのdistribution statisticsを含むversion付きJSONを出力します。Schema 9はinterruption-enabled VM countdown row、schema 8はhost-allocation callback row、schema 7はconditional-sum loop、schema 6は2-value accumulator、schema 5はbatched startup、schema 4はsingle-launch startupとexecutable size、schema 3はcountdownを追加しました。Platform seriesはlink済みworkload setが増えるとrenameします。Consumerは `schema_version` で分岐する必要があり、以前のJSON fileはimmutableなevidenceとして残します。これらのmeasurementは承認済みperformance guaranteeではなく、prototype evidenceです。静かなmachineでのみresultを記録し、正確なcommandとraw JSONを保持します。

Allocation boundary prototypeは、2つ目の型付きhost callbackを追加し、VM／JITのhost registryで1回の `Box<i64>` allocate／read／drop round tripを実行します。Link済みC harnessはAOTで対応する `malloc`／read／`free` operationを行います。macOS AArch64ではdirect linkとresponse-file linkの両方で実行し、cfg-gated Windows MSVC pathも同じ動作を行うよう構成しています。Cleanなschema 9 baselineはRust callbackのdirect Rust／VM／JIT medianとして12.068 ns、189.585 ns、13.931 nsを記録し、既存AOT link／startup setは変更しません。これは既存host-call boundaryを越えるallocation plumbingだけを分離して検証するものです。Source-languageのallocation／ownershipを定義せず、runtime allocator／collectorを選定せず、memory limitを課さず、peak memoryも計測しません。これらの論点は未完了です。

共通チェック：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

現在のworkspaceには、prototypeで実際に使う `lang-mir`、`lang-bytecode`、`lang-vm`、`lang-codegen-clif`、型付きhost-function registryを持つ `lang-runtime`、実行可能な設計modelを持つ `lang-semantics` だけが含まれます。`docs/ja/architecture.md` に示す追加の境界は、対応する実装とともに導入します。

## 設計プロセス

構文、型意味論、メモリ安全性、並行処理、ABI、パッケージ互換性、プラグイン境界を変更する場合はRFCが必要です。リポジトリSkillの `$language-design-rfc` を使用するか、`docs/ja/rfcs/0000-template.md` から開始してください。

RFCがモード固有の制限を明示的に定めない限り、実装ではバイトコードbackendとネイティブbackendの観測可能な動作を等価に保つ必要があります。

## プロジェクト状態

- 成熟度：研究／プレアルファ
- 対応予定ホストプラットフォーム：最初はmacOSとWindows
- 初期実装言語：Rust
- ライセンス：未選定

