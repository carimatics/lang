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
- [アーキテクチャ提案](docs/ja/architecture.md)
- [実装ロードマップ](docs/ja/roadmap.md)
- [RFCプロセスとテンプレート](docs/ja/rfcs/0000-template.md)
- [RFC 0001：プロジェクト憲章と互換性ポリシー](docs/ja/rfcs/0001-project-charter.md)
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

Workspace testでは、最小のembedding経路も検証します。backend非依存のhost-function IDから型付き `extern "C" fn(i64) -> i64` 登録を解決し、同じMIR host callをVMとCranelift JITの両方で実行します。このprototypeはstable ABIを定義せず、aggregate value、error transport、panic containment、capability、resource limitはまだ提供しません。

実行可能なownership実験は、`cargo test -p lang-semantics` で個別に実行できます。Async modelはsource span、structured diagnostic、borrow exclusivity、suspension、cancellation、deterministic destructionを扱います。Graph modelは明示的なshared／weak edgeとarena-scoped cyclic storageを比較します。Service modelはactor-isolated mutation、sendable payloadのmove、FIFO mailbox、deferred cancellation cleanupを検証します。これらのmodelは設計証拠を記録するだけで、承認済み言語意味論ではありません。

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

