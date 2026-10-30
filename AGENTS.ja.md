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
  - `docs/architecture-history.md` と `docs/ja/architecture-history.md`
  - `docs/architecture-m3-history.md` と `docs/ja/architecture-m3-history.md`
  - `docs/roadmap.md` と `docs/ja/roadmap.md`
  - `docs/roadmap-history.md` と `docs/ja/roadmap-history.md`
  - `docs/benchmarks.md` と `docs/ja/benchmarks.md`
  - `docs/milestone-1.md` と `docs/ja/milestone-1.md`
  - `docs/milestone-2.md` と `docs/ja/milestone-2.md`
  - `docs/milestone-3.md` と `docs/ja/milestone-3.md`
  - `docs/milestone-4.md` と `docs/ja/milestone-4.md`
  - `docs/milestone-3-history.md` と `docs/ja/milestone-3-history.md`
  - `docs/milestone-2-capabilities.md` と `docs/ja/milestone-2-capabilities.md`
  - `docs/milestone-3-capabilities.md` と `docs/ja/milestone-3-capabilities.md`
  - `docs/milestone-4-capabilities.md` と `docs/ja/milestone-4-capabilities.md`
  - `docs/native-sdk.md` と `docs/ja/native-sdk.md`
  - `docs/review-2026-10-17.md` と `docs/ja/review-2026-10-17.md`
  - `docs/review-2026-10-20.md` と `docs/ja/review-2026-10-20.md`
  - `docs/review-2026-10-27.md` と `docs/ja/review-2026-10-27.md`
  - `docs/review-2026-10-28.md` と `docs/ja/review-2026-10-28.md`
  - `docs/spec-improvement-study.md` と `docs/ja/spec-improvement-study.md`
  - `docs/rfcs/0000-template.md` と `docs/ja/rfcs/0000-template.md`
  - `docs/rfcs/` の各番号付きRFCと `docs/ja/rfcs/` の同名ミラー
  - `.agents/skills/language-design-rfc/SKILL.md` と `docs/ja/codex/language-design-rfc.md`

## 最初に読むもの

- コンパイラ構造の変更を提案する前に、`README.md`、`docs/architecture.md`、`docs/roadmap.md` を読んでください。
- 言語意味論、メモリ規則、並行処理、ABI、シリアライズ形式を変更する前に、関連する承認済みRFCを読んでください。
- 言語機能の提案とレビューには `language-design-rfc` Skillを使用してください。
- ユーザーの変更を保持し、各patchを依頼された作業に限定してください。

## ブランチとpull requestの作業手順

方針更新日: 2026-10-28 JST。

- 単一のプロンプト、またはCodex Goalで進める一連の関連taskを1つの作業単位として、この手順を適用してください。開始前に元のブランチ、remote／upstream、既存の作業ツリーの変更を記録し、ユーザーの変更を保持してください。
- `main` ではGit管理対象ファイルの編集、stage、commitを行ってはいけません。Git管理に追加するファイル、削除、追跡済み生成ファイルも対象です。これらの変更前に作業ブランチを作成してください。Codexが作成するブランチには、内容を判別できる `codex/*` 形式の名前を付けてください。
- ブランチは、まとまりがありレビュー可能な成果を単位とします。関連taskは同じブランチで進め、プロンプトの継続、Goalの個々のtask、delivery行、commitごとに機械的に作成しないでください。成果や依存関係に理由があれば、複数のブランチを作成できます。各ブランチについて、分岐元ブランチとremoteリポジトリをpull requestのbaseとして記録してください。
- 作業ブランチの実装が完了したら、関連するローカルcheckを実行し、変更をcommit・pushして、記録したbaseブランチへのpull requestを作成してください。依頼範囲内のこの一連の操作は、定型的な再確認を求めず完遂してください。実際の権限制約とリポジトリの保護設定は尊重してください。
- マージ前に、最新のhead commitに対する適用対象のpull request CI checkがすべて成功したことを確認してください。待機中、失敗、キャンセル、想定外のcheck欠落は成功ではありません。既存の意図的なpath／job条件は引き続き適用し、checkの省略理由を確認してください。説明できない欠落を成功と扱ってはいけません。ローカルcheckや古いcommitの成功結果で、この条件を代替してはいけません。
- CIが失敗したら原因を調査・修正し、修正をpushしてCIを再確認してください。成功させるためだけに、失敗するテストのskip・削除、assertionやCI検証範囲の弱体化、本質的でない場当たり的なhackを行ってはいけません。根本原因を解消し、一時的な実行基盤の障害を再試行する場合も、診断の根拠を持ってください。
- CI成功とリポジトリの要件充足後に、remoteのpull requestを通じてマージしてください。pull requestのマージ完了を確認してからfetchし、ローカルのbaseブランチをremoteのマージ後の状態へ更新してください。マージ済みremote commitとの `main` の同期は許可します。`main` での直接編集、ローカルmerge commit、直接pushはこの手順に含めません。
- 作業単位の完了を宣言する前に、開始時に記録したブランチへswitchし、そのremoteをfetchして、ローカルとremoteの先端が一致し、ahead／behindのcommitも未commitの作業変更もないことを確認してください。可能な限りfast-forwardで更新し、同期させるためにユーザーの変更を破棄したり、分岐した履歴を強制resetしたりしてはいけません。
- 権限、CI、競合、remote設定の欠落、無関係なローカル変更によって完遂できない場合は、作業を保持し、ブランチ、pull request、未解決の条件を報告してください。マージや同期が完了したとは報告しないでください。具体的な手順と既存ルールとの関係は [CONTRIBUTING.ja.md](CONTRIBUTING.ja.md#ブランチ作成からマージまで) を参照してください。

## リポジトリ単位の承認管理

方針更新日: 2026-10-28 JST。

- このリポジトリの作業で付与された永続的な承認は、原則としてリポジトリ単位で管理し、承認の種類ごとに保存先を分けてください。ユーザーが承認した範囲を保持し、一度限りの承認を永続ルールや、より広いコマンド・ツール・アプリ・サーバーの許可へ拡張してはいけません。
- シェルコマンドの承認は、対応する `prefix_rule` 構文を使って `.codex/rules/user.rules` に保存してください。この個人用ファイルはGit管理対象外のままにしてください。アプリやMCPの承認設定を `.rules` ファイルへ記述してはいけません。
- アプリやMCPツールの承認は、リポジトリの `.codex/config.toml` に保存し、`apps.<id>.tools.<tool>.approval_mode` や `mcp_servers.<id>.tools.<tool>.approval_mode` など、文書化されたツール別の `approval_mode` 設定を使用してください。アプリ全体やサーバー全体の許可よりも、承認された個々のツールの設定を優先してください。
- 承認ダイアログがユーザー共通設定へ保存する場合もあります。承認を永続化した後は、実際の保存先を確認してください。新たに保存されたリポジトリ限定の承認は適切なリポジトリ内ファイルへ移し、ローカルの設定を確認してからユーザー共通設定側の重複を削除してください。無関係な設定や既存の共通承認は保持し、過去のグローバル承認がこのリポジトリ専用だと推測してはいけません。ユーザー共通設定の編集に必要な権限制約を尊重し、完了できなかった移行は報告してください。
- `.codex/config.toml` はcheckoutごとの個人用リポジトリ設定としてGit管理対象外・ignore対象に保ってください。内容を保持し、強制addしてはいけません。共有可能な初期値は `.codex/config.example.toml` に置きます。この例はCodexに自動で読み込まれません。configが存在しない場合だけ例から個人用configを作成し、既存の設定や承認を上書きしてはいけません。セットアップと移行は[個人用リポジトリ設定](CONTRIBUTING.ja.md#個人用codex設定)を参照してください。
- 個人の承認設定をcommitや共有用の例に含めてはいけません。ユーザーがリポジトリ共通の方針として明示的に意図した場合に限り、承認の初期値をcommitしてください。認証情報やtokenを含めてはいけません。
- プロジェクト内のconfigとrulesを読み込むには、そのプロジェクトが信頼済みである必要があります。この適用範囲は設定を読み込む条件であり、GitHubツールの操作対象リポジトリやコマンドのremote接続先を制限するものではありません。引き続き、taskで許可されたリポジトリを明示的に操作対象としてください。
- 承認管理の完了を報告する前に、保存先・構文・意図した適用範囲を検証し、新たに作成したリポジトリ限定の承認がユーザー共通設定に残っていないことを確認してください。永続的なローカル設定は未commitのtask変更とは分けて報告し、ルールを保存しただけで実行中セッションへの再読み込みまで確認できたと報告してはいけません。

参照元: [OpenAI Rules](https://developers.openai.com/codex/rules)、[Config basics](https://developers.openai.com/codex/config-basic)、[Configuration Reference](https://developers.openai.com/codex/config-reference)。公開・更新日の記載なし。確認日: 2026-10-28 JST。

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
- 実験的な初期開発の間は、[RFC 0001](docs/ja/rfcs/0001-project-charter.md)に記録した2026-10-17 JSTのownerの委任を適用し、ある程度妥当と判断できるRFCを個別のユーザー承認なしで受理し、判断理由と未検証の仮定を記録して作業を続けてください。
- 受理済み設計を変更して実装する前に、対象RFCの改訂またはfollow-upで変更理由と互換性への影響を記録し、英日対訳を同時に更新してください。
- 構文、ABI、package metadata、serialized artifactの変更には移行と互換性への影響を含めてください。

## 進行中milestoneの追跡

- milestoneを開始するときは、そのmilestoneの実装Todoを使います。[M2実装Todo](docs/ja/milestone-2.md)は完了記録であり、新規作業のqueueではありません。レビューや保守作業はレビュー報告内のscopeを限定したchecklistで管理できます。完了済みmilestoneの再開や、通常の修正への架空のeffort point付与は不要です。
- 進行中milestoneの実装前に、次の成果を観測可能な1〜3 ptの単位へ分け、1行をdoingにします。同時進行の実装成果は最大1件です。新たな前提作業は実装前に明示して再見積もりし、残taskを確認せず見積もりを妥当と繰り返し判断しません。Roadmapの大きな完了条件の重みは、実装残作業の予測ではありません。
- 計画開始から10分経ってもpatchまたは実行可能な実験がなければ、設計を続ける前にtaskを狭めて最小限の有用な実験を実行します。抽象化／hardeningの追加は、選択した成果または受理済み要件に必要な場合に限ります。
- 成果ごとに実装／検証evidence付きで英日Todoを更新します。Evidence欄はcode commit hashのみとし、検証詳細は成果noteへ記録します。前提作業の完了と完了条件の達成を分離します。横方向の基盤追加より次のソース実行fixtureを優先し、実行fixtureが増えない前提作業が2件続いたら順序を見直します。
- doingの行が作業sessionをまたいで未完了なら、Todo冒頭に最後の実行可能なcheckpoint、検証時刻、残る完了条件を表示します。Checkpointは完了成果に数えません。完了した作業と新たに追加した作業を別々に報告し、項目が記載した成果を超えて膨らむ場合は、実装を追加する前に再監査します。
- 計画更新は行と現在の件数を中心に簡潔にします。監査本文はscope、依存、見積もり、完了判断が変わった場合に追加します。現在の契約はRFC／architectureに、日付付きの実装checkpointはTodoやレビュー履歴に記録します。Code、evidence、計画は同じ変更で更新し、計画だけの別commitは必須ではありません。文書だけの計画更新では `cargo test --test repository_consistency` を実行し、変更のないcompiler baseline全体の再実行ではなく、この既存validatorを使います。
- Todoの見積もり収支「導入時の残り ＋ 追加・再見積もりの差分合計 − 完了で取り除いた見積もり ＝ 残り」を更新します。完了成果と新たに可視化した作業を分けて報告します。行をdoneにするときは直前の残り見積もりを取り除き、追加分は明示的なscope／見積もり監査がある場合のみ変更します。新しい行を最初のcommitでdoneにする場合も、監査に見積もりを残し、追加分と完了分の両方に含めます。点数は計画の単位であり、稼働時間の実測ではありません。

## セキュリティ

- credential、token、private key、実際のuser dataをcommitしないでください。
- scriptとplugin codeは既定で信頼しません。filesystem、network、environment、process、native FFI、host APIへのアクセスには明示的なcapabilityを要求してください。
- native in-process pluginをsandboxとして扱わないでください。信頼済みcodeだけに使用してください。
- 実行前にbytecode、component metadata、package manifest、外部inputを検証してください。

