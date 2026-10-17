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
- [Milestone 1機能ガイド](docs/ja/milestone-1.md)
- [RFCプロセスとテンプレート](docs/ja/rfcs/0000-template.md)
- [RFC 0001：プロジェクト憲章と互換性ポリシー](docs/ja/rfcs/0001-project-charter.md)
- [RFC 0002：Prototype integer／structured control-flow semantics](docs/ja/rfcs/0002-prototype-control-flow.md)
- [RFC 0003：Version付きcomponent contractとcapability import](docs/ja/rfcs/0003-versioned-component-contracts.md)
- [RFC 0004：Owned value、明示的共有、cyclic graph storage](docs/ja/rfcs/0004-owned-values-and-cyclic-graphs.md)
- [RFC 0005：Structured task、cancellation、isolated service](docs/ja/rfcs/0005-structured-tasks-and-isolated-services.md)
- [RFC 0006：Source frontendとportable VM program（implemented）](docs/ja/rfcs/0006-source-frontend-and-portable-programs.md)
- [コントリビューションガイド](CONTRIBUTING.ja.md)

## 開発

Milestone 1のsource pipelineは複数fileのmodule、型付きfunctionと再帰、copy-only record／enum、網羅的pattern、検証済みportable bytecode、上限付きVM実行に対応しています。Source spanはanalysis、lowering、artifact、runtime stackまで保持されます。Formatterとstdio LSP prototypeは同じlossless parserとrevision付きmodule／診断databaseを使います。

必要な環境は `rust-toolchain.toml` で固定したRust `1.99.0` とrustfmt／Clippyです。補助tool versionが必要な場合は `mise` を使います。

```bash
cargo run -- run examples/scripts/answer.lang
cargo run -- repl
cargo run --quiet -- lsp
```

Scriptは `42` を出力してexit 0です。Compile／runtime errorはexit 1、usage errorはexit 2で、scriptの結果はexit codeではありません。`--source-root DIR` がfile authorityを選び、省略時はscriptのroot fileの親を使い、REPLにはfile-module authorityを与えません。両commandは `--fuel`、`--host-calls`、`--frames`、`--cells` に対応します。CLIはhost importを登録しません。

REPLは検証に成功したitem定義をcommitし、submissionごとに1つのexpressionを新しいVM counterで評価します。Scalarまたはqualified aggregate valueを表示し、複数行入力と `:reset`／`:quit` に対応します。失敗したsubmissionは以前の定義を保持し、host副作用はrollbackしません。

`cargo build` 後、LSP clientに `target/debug/lang lsp` を設定します。Stdio profileはinitialize／shutdown／exit、UTF-16位置、full document同期、version付き診断、watched-file refresh、canonicalな4-space document formattingに対応します。Initializeで明示するlocal workspace rootは最大1つで、省略時はopen bufferだけを利用します。Code action、completion、navigation、debugger統合は後続作業です。[Architectureとprotocol情報源](docs/ja/architecture.md#diagnosticslspdebugging)を参照してください。

共有APIは `lang::frontend`、`lang::repl::Session`、`lang_semantics::database`、`lang_syntax::format`、MIR／bytecode／VM／Cranelift crateの `unit` moduleです。Compiler／codec／cache budgetを設定できます。VM既定値はfuel 1,000,000、host call 1,000、guest frame 256、live value cell 1,000,000です。Interruption／deadlineはguest safe pointで作用し、trusted native callbackはpreemptできず、unwindは禁止です。

初期native adapterはscalar／CFG profileと既存straight-line owned fixtureに対応します。Source guest callとaggregateはMilestone 2までunsupported-feature errorを返します。`cargo run -- 20 22` は以前のscalar VM／JIT比較を保持します。初期native backendはCranelift 0.136.3です。Native linker／debug evidenceと過去のplatform計測は[architecture](docs/ja/architecture.md)、[benchmarks](docs/ja/benchmarks.md)、[roadmap](docs/ja/roadmap.md)へ記録しています。2026-10-17のlocal source-conformance実行環境はmacOS AArch64で、Windows実行にはCI evidenceが必要です。

Code変更の確定前に次を実行します。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

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

