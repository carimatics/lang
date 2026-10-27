<!-- translation-of: README.md; canonical-language: en -->

# lang

[English](README.md)

`lang` は、新しいシステムプログラミング言語と、その完全な開発ツールチェーンを作るためのプレアルファ研究プロジェクトです。実装はRustで開始し、言語・コンパイラ・ブートストラップ手順が自己再生成できるほど安定した段階でセルフホスティングへ移行することを目指します。

source構文・native language ABI・private SDK/package formatは実験段階です。対応する公開component C v1と既存生成clientは、両host適合と旧client移行後、[RFC 0033](docs/ja/rfcs/0033-component-v1-compatibility.md)に従い安定化しました。

**Milestone 0–3のexit gateは完了しました。** [M3機能ガイドと全15 gate監査](docs/ja/milestone-3-capabilities.md)に、structured task・有限channel・isolated service・typed native/script plugin・公開C API・installed SDKのcommand、証拠、制約を記載しています。[M3 board](docs/ja/milestone-3.md)は全79 deliveryの完了記録です。[M2機能ガイド](docs/ja/milestone-2-capabilities.md)と過去のbenchmark証拠も保持しています。

2026-10-27 JST検証: source `9e78f2d`のmacOS全7段階（153 suite / 2,321成功）と[Windows全CI](https://github.com/carimatics/lang/actions/runs/40471721760)（153 suite / 2,306成功）が失敗0で成功しました。両hostのcheckout/CargoなしSDK起動も7 profileに成功しています。将来milestoneはroadmapに残り、M4はまだ開始していません。

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
- [Benchmark evidence: M2 source/Rust/Cと過去のprototype](docs/ja/benchmarks.md)
- [アーキテクチャbaseline](docs/ja/architecture.md)
- [実装ロードマップ](docs/ja/roadmap.md)
- [Milestone 1機能ガイド](docs/ja/milestone-1.md)
- [Milestone 2 capability guide・全出口監査](docs/ja/milestone-2-capabilities.md)
- [Milestone 3 完了した実装記録](docs/ja/milestone-3.md)
- [全ファイルreview、2026-10-17](docs/ja/review-2026-10-17.md)
- [リポジトリ全体レビュー、2026-10-20](docs/ja/review-2026-10-20.md)
- [リポジトリ全体レビュー、2026-10-27](docs/ja/review-2026-10-27.md)
- [M3アーキテクチャ履歴](docs/ja/architecture-m3-history.md)
- [M3実装・見積り履歴](docs/ja/milestone-3-history.md)
- [RFCプロセスとテンプレート](docs/ja/rfcs/0000-template.md)
- [RFC 0001：プロジェクト憲章と互換性ポリシー（accepted）](docs/ja/rfcs/0001-project-charter.md)
- [RFC 0002：Prototype integer／structured control-flow semantics（implemented）](docs/ja/rfcs/0002-prototype-control-flow.md)
- [RFC 0003：Version付きcomponent contractとcapability import（accepted）](docs/ja/rfcs/0003-versioned-component-contracts.md)
- [RFC 0004: Owned value、明示的共有、cyclic graph storage（accepted）](docs/ja/rfcs/0004-owned-values-and-cyclic-graphs.md)
- [RFC 0005：Structured task、cancellation、isolated service（accepted）](docs/ja/rfcs/0005-structured-tasks-and-isolated-services.md)
- [RFC 0006：Source frontendとportable VM program（implemented）](docs/ja/rfcs/0006-source-frontend-and-portable-programs.md)
- [RFC 0007：Sourceのownershipとcore value（implemented）](docs/ja/rfcs/0007-source-ownership-and-core-values.md)
- [RFC 0008：Cleanup、allocator authority、C boundary（implemented）](docs/ja/rfcs/0008-cleanup-allocators-and-c-boundaries.md)
- [RFC 0009：Option／Result全体からの所有権取り出し（implemented）](docs/ja/rfcs/0009-whole-sum-ownership-extraction.md)
- [RFC 0010: 所有値を返すREPL結果のpreview（implemented）](docs/ja/rfcs/0010-owned-repl-previews.md)
- [RFC 0011: Portable ownership artifacts（implemented）](docs/ja/rfcs/0011-ownership-artifacts.md)
- [RFC 0012: Native ownershipの継続とmanaged frame（implemented）](docs/ja/rfcs/0012-native-ownership-continuations.md)
- [RFC 0013: Service byte-work budget（implemented）](docs/ja/rfcs/0013-service-byte-work-budget.md)
- [RFC 0014: 明示的なソースpanic（implemented）](docs/ja/rfcs/0014-explicit-source-panic.md)
- [RFC 0015: Private C native object bridge（implemented）](docs/ja/rfcs/0015-c-native-object-bridge.md)
- [RFC 0016: 間接値の REPL 結果プレビュー（implemented）](docs/ja/rfcs/0016-indirect-repl-previews.md)
- [RFC 0017: Cに対応したownership artifact（implemented）](docs/ja/rfcs/0017-c-aware-ownership-artifacts.md)
- [RFC 0018: Native compiler record（implemented）](docs/ja/rfcs/0018-native-compiler-records.md)
- [RFC 0019: 通常owned native entry runtime（implemented）](docs/ja/rfcs/0019-owned-native-entry-runtime.md)
- [RFC 0020: 実験段階のnative SDKとscope付き起動（implemented）](docs/ja/rfcs/0020-native-sdk-and-startup.md)
- [RFC 0021: 上限付き構造化タスクドライバー（implemented）](docs/ja/rfcs/0021-bounded-structured-task-driver.md)
- [RFC 0022: Source の構造化タスク（implemented）](docs/ja/rfcs/0022-source-structured-tasks.md)
- [RFC 0023: Task MIR の境界（implemented）](docs/ja/rfcs/0023-task-mir-boundaries.md)
- [RFC 0024: 再帰 Send/Share と service 権限（implemented）](docs/ja/rfcs/0024-send-share-and-service-authority.md)
- [RFC 0025: 有限 channel と backpressure（implemented）](docs/ja/rfcs/0025-bounded-channels-and-backpressure.md)

- [RFC 0026: Source isolated service（implemented）](docs/ja/rfcs/0026-source-isolated-services.md)
- [RFC 0027: Production component IDL・wire・embedding profile（implemented）](docs/ja/rfcs/0027-production-component-interface.md)
- [RFC 0028: Task と component MIR の直列化（implemented）](docs/ja/rfcs/0028-task-component-artifacts.md)
- [RFC 0029: Component adapter work と invocation 計量（implemented）](docs/ja/rfcs/0029-component-adapter-work.md)
- [RFC 0030: Native task/component record（implemented）](docs/ja/rfcs/0030-native-task-component-records.md)
- [RFC 0031: Installed native task package（implemented）](docs/ja/rfcs/0031-installed-native-task-packages.md)
- [RFC 0032: 明示host binding付きinstalled component package（implemented）](docs/ja/rfcs/0032-installed-component-packages.md)
- [RFC 0033: Component C v1の互換性と移行（stabilized）](docs/ja/rfcs/0033-component-v1-compatibility.md)
- [コントリビューションガイド](CONTRIBUTING.ja.md)

## 開発

`rust-toolchain.toml`で固定したRust **1.99.0**とrustfmt/Clippyを使います。[M2機能guide](docs/ja/milestone-2-capabilities.md)と[M3機能guide](docs/ja/milestone-3-capabilities.md)に実装動作・完了条件・制約を記録しています。`cargo run -- run examples/scripts/tasks.lang`で実装済みsource asyncを実行し、42を返します。[M3 board](docs/ja/milestone-3.md)は完了した79 deliveryの記録です。

```bash
cargo run -- check examples/conformance/owned_pipeline.lang
cargo run -- run examples/conformance/owned_pipeline.lang
cargo run -- compile --output target/owned.lbc examples/conformance/owned_pipeline.lang
cargo run -- run --artifact target/owned.lbc
cargo run -- repl
cargo run --quiet -- repl < examples/repl/owned.repl
cargo run --quiet -- lsp
```

複合programは42を表示します。出力先は新規pathにしてください。CLIはowned実行にallocator権限を付与し、host importを登録せず、C実行はeffect前に拒否します。REPLは定義を保持し、評価ごとにguest counter／storageを新しく用意してcore／indirect値の上限付きpreviewを返します。

Code変更を完了する前に次を実行します。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

### Native program出力

`cargo run -- native --output PROGRAM SOURCE`は元object、portable意味論／native record、検証済plan、`bindings.rs`と必要な`main.rs`／`program.h`を公開します。通常Copy／owned startupは引数なしi64 entryを使い、C libraryは明示hostを要します。[Producer test](tests/native_compiler.rs)を参照してください。

Task packageはprogram manifest version 2、schema-5意味論とLANGNAT-2 objectを使います。component packageはprogram version 3と明示host bindingを使い、現SDK manifest version 3が対応します。旧copy/owned/C/task profileの受理規則も維持します。実両hostの7 profile隔離起動は[M3完了guide](docs/ja/milestone-3-capabilities.md)と[RFC 0031](docs/ja/rfcs/0031-installed-native-task-packages.md)・[0032](docs/ja/rfcs/0032-installed-component-packages.md)に記録しています（2026-10-27 JST確認）。

### Native package検証API

`lang::native`は所有snapshot内で上限付きmanifest、path、hash、target／build／toolchain対応、意味論／native recordを検証します。Toolの使用中はsnapshotを不変に保ちます。Hashは完全性／対応の検査で、native入力は信頼対象です。[Package test](tests/native_packages.rs)を参照してください。

### Scoped native SDK runtime API

Unsafeな`lang_codegen_clif::sdk::runtime::{execute_copy, execute_owned, execute_tasks, with_c}`は元linked imageをbindし、起動時にsource compiler・JIT・guest interpreterを使いません。Ownedはallocator/Instance/control/failureを保持し、task startupはcallerのallocator/Control/runner上限と元task cleanup/failureを保持します。Copyは同期/非計量でcontrol設定を拒否します。C context/callback/dependencyは元binding寿命に収めます。[runtime](crates/codegen-clif/src/sdk/runtime.rs)・[RFC 0020](docs/ja/rfcs/0020-native-sdk-and-startup.md)・[RFC 0031](docs/ja/rfcs/0031-installed-native-task-packages.md)を参照してください。

### Native SDK assembly

`python3 tools/build-native-sdk.py --output SDK`は正確なCargo出力から新規の対応host SDKを組み立て、compiler／runtime／依存物・C header・template・日英説明を収録します。Releaseには`--release`を追加し、`panic=unwind`が必要です。[SDK手順](docs/ja/native-sdk.md)と[組立test](tests/native_sdk.rs)を参照してください。

### Native executable linking

`lang link-native --sdk SDK --package PROGRAM --output EXE`はrustc release／commit／hostの完全一致を確認し、Cargo・registry・repository探索なしで同梱の元codeをlinkします。Cは`--host HOST.rs`、必要なら繰り返し`--object HOST.o`と明示context／provider／import権限を要します。出力先は新規pathにし、失敗時は既存出力を保持します。[全手順](docs/ja/milestone-2-capabilities.md#native-executableを作る)と[link test](tests/native_link.rs)を参照してください。

### Native SDK isolated installation

`c54af31`の[run 39370391958](https://github.com/carimatics/lang/actions/runs/39370391958)は、両native hostの別producer／consumerでconsumer側checkout／Cargo／repository入力なしの隔離installを実証します。Private source／SDK／program削除後にCopy／owned／実Cを実行します。[Workflow](.github/workflows/native-sdk-install.yml)と[probe](tools/check-native-sdk-install.py)を参照してください。System library／toolchainは外部依存です。

### Combined source conformance

[複合program](examples/conformance/owned_pipeline.lang)は入れ子collection、owner移動、shared／weak、arena、再帰call、cleanupを実行して42を返し、失敗variantは−42でpanicします。[Production entry比較](tests/support/native_entry_runtime.rs)、[実AOT](tests/native_compiler.rs)、[実C runtime](tests/support/c_exports_runtime.rs)で値・usage・source失敗・cleanup／拒否境界を照合します。

両host適合性、不変のRust／C計測、実LLDB／CDB実行は、[最終監査](docs/ja/milestone-2-capabilities.md)にそれぞれ異なるrevisionの証拠があります。既定linkはdebug sidecarを公開しません。Owned debugの実証は物理native／runtime frameまでで、論理continuation step・source変数・VM／native混在stackは後続範囲です。

## 設計プロセス

構文、型意味論、メモリ安全性、並行処理、ABI、パッケージ互換性、プラグイン境界を変更する場合はRFCが必要です。リポジトリSkillの `$language-design-rfc` を使用するか、`docs/ja/rfcs/0000-template.md` から開始してください。

RFCがモード固有の制限を明示的に定めない限り、実装ではバイトコードbackendとネイティブbackendの観測可能な動作を等価に保つ必要があります。

## レビュー範囲と現在の制限

2026-10-17レビューではJIT memory回収、bytecode span保持、CodeView machine type、bounded decoding、semantic-version validation、要求全体のcapability preflight、Rust adapter cleanupを強化しました。Linker response fileは新しいpathを必要とし、既存fileとsymlinkを切り詰めず拒否します。反復呼び出しでは新しいresponse-file pathを選ぶか、自分の古いfileを明示的に削除してください。

過去のprototype benchmark captureは改変せず保持し、JIT memory回収修正前のものです。M2のsource／Rust／C captureは記載した別revisionの実験です。現在の性能を解釈する際は[benchmark evidence](docs/ja/benchmarks.md)を参照してください。直接のnative host callbackはtrustedでありunwindは禁止です。Sandboxではありません。

## プロジェクト状態

- 成熟度：研究／プレアルファ
- 検証済みnative host：macOS AArch64、Windows x86-64（実験段階）
- 初期実装言語：Rust
- ライセンス：未選定

