<!-- translation-of: AGENTS.md; canonical-language: en; informational-only: true -->

# リポジトリ指示

この指示はリポジトリ全体に適用されます。

## ドキュメント言語

- 実装判断とAI向け指示では英語ドキュメントを正本とします。
- 日本語ドキュメントは人間が読むためのミラーであり、追加要件を導入してはいけません。
- `AGENTS.ja.md` は参考用の翻訳です。Codexはこれを別の指示ソースとして扱ってはいけません。
- 翻訳ペアは同じ変更で両方を更新してください。見出し、リンク、例、表、ステータスラベル、情報源の日付を同等に保ってください。
- 翻訳ペアは以下です。
  - `README.md` と `README.ja.md`
  - `AGENTS.md` と `AGENTS.ja.md`
  - `CONTRIBUTING.md` と `CONTRIBUTING.ja.md`
  - `docs/research.md` と `docs/ja/research.md`
  - `docs/architecture.md` と `docs/ja/architecture.md`
  - `docs/roadmap.md` と `docs/ja/roadmap.md`
  - `docs/benchmarks.md` と `docs/ja/benchmarks.md`
  - `docs/rfcs/0000-template.md` と `docs/ja/rfcs/0000-template.md`
  - `docs/rfcs/` の各番号付きRFCと `docs/ja/rfcs/` の同名ミラー
  - `.agents/skills/language-design-rfc/SKILL.md` と `docs/ja/codex/language-design-rfc.md`

## 最初に読むもの

- コンパイラ構造の変更を提案する前に、`README.md`、`docs/architecture.md`、`docs/roadmap.md` を読んでください。
- 言語意味論、メモリ規則、並行処理、ABI、シリアライズ形式を変更する前に、関連する承認済みRFCを読んでください。
- 言語機能の提案とレビューには `language-design-rfc` Skillを使用してください。
- ユーザーの変更を保持し、各patchを依頼された作業に限定してください。

## 調査

- Web調査の前に、現在の日付とタイムゾーンを確認してください。
- 仕様、公式ドキュメント、プロジェクトmaintainer、標準化団体、元の調査データセットを優先してください。
- 各情報源の公開日または更新日を記録してください。Living Documentに公開日がない場合は、その旨と確認日を記録してください。
- 情報源に基づく事実、プロジェクトの決定、設計仮説を分離してください。

## Rust

- `rust-toolchain.toml` で固定されたstable RustとRust 2024 editionの規約を使用してください。
- `cargo fmt` でformatし、Clippyでlintし、警告をエラーとして扱ってください。
- `unsafe` を避けてください。必要な場合はblockを狭く保ち、安全性不変条件を文書化し、その境界を実行するテストを追加してください。
- 複数の意味を持つ文字列やbooleanより、明示的なdomain typeと構造化診断を優先してください。
- frontendの意味論を特定のcode generation backendから独立させてください。
- `Cargo.lock` を手動編集しないでください。

## コンパイラアーキテクチャ

- syntax、HIR、MIR、bytecode、native debug metadataを通してsource spanを保持してください。
- lossless syntax dataをsemantic IRから分離してください。
- 型付きMIRをAOT実行とscript実行で共有する意味契約として扱ってください。
- frontend APIからLLVM、Cranelift、VM、platform固有typeを公開しないでください。
- serialized IR、bytecode、package metadata、plugin interfaceをversion付きformatとして扱い、trust boundaryで検証してください。
- 最初のfrontend、共有ABI、conformance suiteが安定するまで、2つ目のsource languageを追加しないでください。

## 検証

最初に関連する最小限のcheckを実行し、code changeを完了する前に次のbaseline全体を実行してください。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

動作変更には焦点を絞ったテストを追加してください。コンパイラ作業では、parser snapshot、compile-pass test、compile-fail diagnostics、MIR／bytecode snapshot、VM／native differential test、fuzzing、platform integration testを適切に組み合わせてください。

## ドキュメントとRFC

- 動作またはアーキテクチャの変更と同じ変更でドキュメントを更新してください。
- ユーザーまたは長期互換性に影響する変更では `docs/rfcs/0000-template.md` を使用してください。
- RFC 0001のlifecycle label（proposed、accepted、implemented、stabilized、withdrawn、superseded）を使用してください。受理は実装・安定化を意味しません。拒否された提案はdecision rationaleを添えてwithdrawnと記録してください。
- 構文、ABI、package metadata、serialized artifactの変更には移行と互換性への影響を含めてください。

## セキュリティ

- credential、token、private key、実際のuser dataをcommitしないでください。
- scriptとplugin codeは既定で信頼しません。filesystem、network、environment、process、native FFI、host APIへのアクセスには明示的なcapabilityを要求してください。
- native in-process pluginをsandboxとして扱わないでください。信頼済みcodeだけに使用してください。
- 実行前にbytecode、component metadata、package manifest、外部inputを検証してください。

