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

Code変更を提出する前に、次を実行してください。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

文書だけの計画follow-upは `cargo test --test repository_consistency` を実行してください。変更のない実装の成功済みfull baselineを再利用し、code変更、失敗、未解決の懸念がある場合に広いcheckを再実行します。

## 成果の計画

進行中milestoneの実装Todoを使います。[M2実装Todo](docs/ja/milestone-2.md)は完了済みで、履歴として保持します。次のmilestoneを始める前に依存関係を確認し、次の成果を観測可能な1〜3 ptのtaskへ分けます。doingの実装taskは最大1件とし、doneの前に実装evidenceと検証を記録してください。レビューや保守作業は、点数の付与やM2の再開をせず、報告内のscopeを限定したchecklistで管理できます。

計画と対訳は実装と同じ変更で更新し、計画だけの別commitは必須ではありません。新たな前提作業はcodeの前に計画へ反映します。従来の完了条件と実装sliceの完了は分けます。新たなソース実行evidenceが増えない前提作業が2件続いたら、次のvertical sliceを見直します。現在の契約はRFC／architectureに、日付付きの実装checkpointはTodoやレビュー履歴に記録します。Commitごとに履歴本文や根拠のない同じ見積もり判断を繰り返しません。

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

実験的な初期開発では、[RFC 0001の判断委任方針](docs/ja/rfcs/0001-project-charter.md#実験段階のrfc判断)に従い、coding agentがある程度妥当と判断できるRFCを個別のユーザー承認なしで受理できます。判断と残る不確実性を記録し、後から設計変更を実装する前にRFCを明示的に改訂してください。受理は実装・安定化と区別します。

## テスト戦略

`cargo test --test repository_consistency` はroadmap集計、日英task状態、RFC status／更新日、READMEの全RFC indexと現在のstatusを検証します。計画変更後に実行してください。両tier-1 CI workflowで全baselineと別fuzz workspaceのformat確認を実行し、macOS workflowでは全fuzz binaryをwarning禁止でlintします。Fuzz targetまたは共有dependency変更後は `cargo fmt --manifest-path fuzz/Cargo.toml --check` と `cargo clippy --locked --manifest-path fuzz/Cargo.toml --bins -- -D warnings` を実行してください。Fuzz targetのcompileはfuzz campaignではなく、長時間fuzzingは明示的な検証作業として残ります。

関連するnative変更では、両hostで実LLDB／CDBのsource stepと論理stack検証も起動します。Metadataだけのテストはこの根拠を代替しません。Debugger validatorは `python3 -m unittest discover -s tools -p 'test_*.py'`、benchmark validatorは `python3 -m unittest discover -s tools/m2-bench -p 'test_*.py'` で検証し、後者は通常のmacOS CIにも含めます。

Tier-1 AOT integrationは `tests/fixtures/aot_harness.c` を共有します。最適化したC callbackはunsigned演算とbit保持変換でRustのwrapping behaviorに合わせ、通常入力に加えて `i64` 境界値、allocation sum、overflowするFibonacciを検証します。

macOS workflowは `macos-26` と `macos-15` の両ARM64 runnerで全baselineを実行し、Intel runner上でcfg-gated native testが黙ってskipされないよう実際のarchitectureを確認します。Native debugger検証、隔離SDKインストール、source／Rust／C benchmarkも両macOSバージョンを対象とし、Windowsは引き続き `windows-2025` x86-64 MSVCを使用します。Matrix jobはfail-fastを無効にし、あるOSの失敗で別OSの検証がcancelされないようにします。情報源：[GitHub runner-image catalog](https://github.com/actions/runner-images/blob/main/README.md)（2026-09-17更新、2026-10-27確認）。新workflowはremote run成功前には実行evidenceになりません。

SDK artifact、debugger cache／evidence、benchmark artifactはrunner labelを含め、2つのmacOS buildを分離します。隔離SDK consumerは自身のOSとtarget向けにbuildされたartifactを取得します。これは各OSでのインストールを検証するもので、OSバージョン間のbinary互換性検証ではありません。Benchmark workflowの `macos` selectorは両macOSバージョン、`windows` はWindowsのみ、`both` は3環境すべてを実行します。

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
