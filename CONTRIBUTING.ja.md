<!-- translation-of: CONTRIBUTING.md; canonical-language: en -->

# コントリビューション

[English](CONTRIBUTING.md)

このプロジェクトは調査・ブートストラップ段階です。広範なscaffoldより、小さくテスト可能なvertical sliceを優先します。

## セットアップ

Rustは `rust-toolchain.toml` で固定されています。`mise` を使用する場合は、次のコマンドで補助ツールを導入してください。

```bash
mise install
```

次のコマンドでcheckoutを検証します。

```bash
cargo test --workspace --all-targets --all-features
```

## 必須チェック

変更を提出する前に、次を実行してください。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

## RFCが必要な場合

次のいずれかを変更する場合は、`docs/rfcs/0000-template.md` から開始してください。

- 構文またはsource compatibility
- 型推論、generics、trait、effect、error semantics
- ownership、borrowing、region、reference counting、destruction、`unsafe`
- concurrency、cancellation、atomic、memory model
- FFI、calling convention、component interface、plugin capability、ABI stability
- bytecode、serialized IR、package manifest、lockfile、registry behavior
- toolchain compatibility、edition、deprecation、migration policy

RFCでは、承認済み要件と仮説を区別し、代替案、コスト、移行、診断、実装段階、テスト戦略を含める必要があります。

## テスト戦略

観測可能でbackend横断のテストを優先します。

- Parser／formatter round-trip test
- Compile-pass／compile-fail diagnostic test
- Type／ownership checker unit test
- MIR／bytecode verification test
- VMとnative backendのdifferential execution test
- 独立してbuildされたcomponent間のABI／embedding test
- Parser、decoder、verifierへのproperty testとfuzzing
- Windows／macOS integration test
- Compile time、runtime、memory、binary size benchmark

## Pull request

- 無関係なrefactorは分離してください。
- ユーザーから見える動作とアーキテクチャへの影響を説明してください。
- 関連RFCへlinkするか、RFCが不要な理由を記載してください。
- 実装と同時にテストとドキュメントを含めてください。
- 新しい不変条件を文書化せずに、安全性checkを弱めてbenchmarkを改善してはいけません。
