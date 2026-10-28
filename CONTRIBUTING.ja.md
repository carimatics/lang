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

### 個人用Codex設定

更新日: 2026-10-28 JST。

Codexは、信頼済みcheckoutの `.codex/config.toml` をプロジェクト設定として読み込みます。このリポジトリでは、このファイルを**Git管理対象外・ignore対象**にし、リポジトリ固有の好み、ローカルMCP接続先、個別に承認したツール設定を、commitに含めず各自で保存できます。シェルの承認は、別途ignoreされる `.codex/rules/user.rules` に保存します。

Git管理する [`.codex/config.example.toml`](.codex/config.example.toml) は共有可能な初期値です。新しいcheckoutでは、個人用configが存在しない場合だけ `.codex/config.toml` へコピーし、そのコピーを調整してください。既存ファイルは保持します。例のファイルは自動で読み込まれたりマージされたりしません。例が後日更新されたら内容を確認し、必要な初期値を個人用configへ手動で反映してください。cloneやworktreeごとにローカル設定が必要です。例の任意のRustRover接続先はコメントアウトし、新しいcheckoutでそのローカルサーバーを必須にしません。

移行前のcheckoutでは、**変更をpullする前、またはファイルの追跡状態が異なるブランチへswitchする前**に、`.codex/config.toml` を作業ツリー外へバックアップしてください。移行では追跡済みのコピーを削除するため、更新によって未変更のファイルが削除されたり、ローカル変更があると停止したりします。また、古いブランチへのswitchではignore対象のコピーが上書きされる場合があります。更新後に個人用configを復元してください。更新を進めるために設定を破棄してはいけません。今回の移行を実行するcheckoutでは、個人用ファイルを保持し、Git indexの項目だけを削除します。他のcheckoutではバックアップと復元が必要です。

`git ls-files -- .codex/config.toml`（出力なし）、`git check-ignore -v .codex/config.toml`（`.codex/.gitignore` のルール）、`git status --short`（configの変更なし）で確認できます。すでに追跡済みのファイルは、`.gitignore` への追加だけでは管理対象外になりません。個人の承認を例に含めたり、実際のconfigを強制addしたりしてはいけません。ファイルの保存だけで実行中セッションへの再読み込みを確認できたことにはなりません。必要に応じて新しいセッションで有効な設定を確認してください。

情報源: [OpenAI Config basics](https://developers.openai.com/codex/config-basic)。プロジェクト設定のパス、優先順位、信頼済みプロジェクトの要件を記載しています。公開・更新日の記載なし。確認日: 2026-10-28 JST。実際のconfigをignoreし、例を配布することは、このリポジトリの方針です。

## ブランチ作成からマージまで

方針更新日: 2026-10-28 JST。[リポジトリの作業ルール](AGENTS.ja.md#ブランチとpull-requestの作業手順)は、単一のプロンプトとCodex Goalで進める一連の関連taskに適用します。

1. 開始時のブランチ、remote／upstream、作業ツリーの状態を記録します（`git branch --show-current`、`git remote -v`、`git status --short --branch`）。既存の変更を保持してください。対象remoteをfetchして履歴の分岐を確認し、破壊的なresetは行わないでください。detached HEADの場合やremote／baseを特定できない場合は、編集前に解決してください。
2. Git管理対象ファイルを変更する前に、まとまりのある成果のためのブランチを作成します。例: `git switch -c codex/describe-the-outcome`。分岐元が `main` 以外でも、そのブランチをPRのbaseとして記録します。関連taskはこのブランチで継続してください。独立してレビュー可能な成果や依存関係に理由がある場合だけブランチを分け、Goalの個々のtaskやdelivery行ごとには分けません。
3. 対象範囲の変更を実装し、翻訳ペアと関連するdelivery evidenceを更新して、[必須のローカルcheck](#必須チェック)を実行します。差分を確認し、意味のある単位でcommitして、対象remoteへ作業ブランチをpushしてください。`main` で編集・stage・commitしたり、baseブランチへ作業変更を直接pushしたりしてはいけません。
4. 記録したbaseと作業ブランチのheadを指定して、remoteにpull requestを作成します。成果、関連RFCまたはRFCが不要な理由、検証結果を説明してください。delivery evidenceで使用するcommit hashが保持されるマージ方式を選び、そのhashをbaseの履歴に残す必要がある場合はmerge commitを優先してください。
5. PRの最新head SHAを確認し、適用対象のCI checkがすべて成功するまで待ちます。期待するworkflowが実際に実行されたことを確認してください。待機中、失敗、キャンセル、想定外のcheck欠落があればマージできません。既存の文書化されたpath／job条件によってcheckが適用対象外になる場合は、必要に応じて理由を記録します。ローカル検証や以前のSHAのCI結果では不十分です。headが変わったら、新しいheadを再確認してください。
6. 失敗の原因を調査し、作業ブランチで根本原因を修正してpushし、検証を繰り返します。失敗するテストのskip・削除、assertionやCI検証範囲の弱体化、CIを成功させるだけのhackは禁止です。一時的な実行基盤の障害は、診断を裏付ける根拠がある場合だけ再試行します。必須レビューなどの保護設定も迂回せず満たしてください。
7. 条件を満たしたらPRを通じてマージし、remoteのPRが `merged` になったことを確認します。remoteをfetchし、ローカルのbaseブランチへswitchして、対応するremoteブランチまでfast-forwardします（記録したbaseが `origin` の `main` なら、例: `git merge --ff-only origin/main`）。履歴が分岐している場合は、ローカル作業を保持して原因を解決し、強制resetしないでください。完了した各作業ブランチでこの手順を実行し、依存関係がある場合はその順序に従ってください。
8. 作業単位全体の終了時には、手順1で記録したブランチへ戻ります。fetchしてローカルとremoteの先端を比較してください。upstreamが意図したremoteブランチなら、`git rev-list --left-right --count HEAD...@{upstream}` の両方の値が0である必要があります。`git status --short --branch` で未commitの作業変更がなく、作業前のユーザーの変更が保持されていることを確認してください。PR、CI結果、マージ確認、同期結果を報告します。未解決の手順がある場合は作業を保持し、完了を宣言せず、未完了の条件を具体的に報告してください。

### 既存ルールとの関係

- delivery行と意味のあるcommitは引き続き計画・evidenceの単位であり、それぞれにブランチを作る必要はありません。1つのブランチに複数の関連deliveryを含められ、理由があれば1つのGoalで複数のブランチを使えます。
- 以下の文書だけの計画変更に対する例外は、**ローカル**検証の重複を減らすものです。PR CIは免除されず、適用対象のcheckが成功する前にマージしてはいけません。
- RFC 0001の設計判断の委任は引き続き有効です。RFCの受理はブランチ、PR、CI、マージの要件を代替せず、これらの要件によって定型的なownerへの再確認を追加することもありません。
- remoteでマージ済みのcommitへ `main` を更新することは同期であり、`main` 上で変更を作成する行為ではありません。常にユーザーの変更を保持し、権限、競合、履歴の分岐は解決するか明示的に報告してください。作業を破棄して問題を隠してはいけません。

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

### CIの実行と所要時間

更新日: 2026-10-28 JST。

`CI` workflowをpull requestと `main` へのpushの共通入口とし、手動dispatchは常に全検証を選びます。変更分類は、変更された全pathが既知の開発者向け文書または個人設定の例である場合だけ軽量経路を許可します。対象はrootのREADME／AGENTS／CONTRIBUTINGの翻訳ペア、`docs/` 配下のMarkdown、`.codex/config.example.toml`、2つの `.codex` ignoreファイルです。SDKに埋め込む `docs/native-sdk.md` と日本語版は常に全検証を必要とします。Code、実行例、その他の設定、JSON／schema、CI変更、未知のpathも全検証です。Git比較はNUL区切りの名前とrename前後の両pathを使用し、履歴不足、不正event、空の比較でも全検証を選びます。PRはmerge baseから、pushは `before` と `after` の間を比較します。

文書だけの変更はUbuntuで既存の `cargo test --locked --test repository_consistency` を実行し、tier-1 native runnerを占有しません。それ以外は既存のOS matrixとテスト範囲を保持したmacOS、Windows、隔離SDK workflowを呼び出します。これらのworkflowは引き続き直接dispatchして全検証でき、branch pushによるPRテストの重複実行をなくします。常に起動する **CI result** jobは、選択したjobの成功と非選択jobの想定したskipだけを受け入れ、失敗、cancel、job欠落、不正なscopeでは成功しません。共通gateにはこの固定checkを使い、適用対象の変更では別途起動するnative-debug workflowも引き続き確認します。分類とgateは `tools/ci/test_routing.py` で、実際のGit履歴、rename、PR baseの分岐を含めて検証します。

Windowsでは `python tools/ci/run_tests.py <workspace|core-1|core-2|native|owned>` を使い、Cargo targetを5つの独立したgroupで実行します。`workspace` はrootの `lang` package以外の全memberを、全targetを有効にして検証します。`core-1` と `core-2` は並べ替えたrootの非native integration targetを交互に分担し、`core-1` はrootのlibrary／binary／example／benchmark targetも担当します。`owned` は `native_owned_boundary` と `native_task_execution`、`native` は残りの `native_*` integration targetを担当します。macOSでは `python3 tools/ci/run_tests.py <core|native-all>` を使い、`core` にWindowsの3つのcore group、`native-all` に `native` と `owned` をまとめます。両macOSバージョンで計4 jobにすると、このrepositoryで観測したmacOSの同時実行5枠に収まります。最初の6 jobでの実験では、1つのtest group全体にrunner待ちが発生しました。Cargo metadataからtargetを取得するため、新しいテストも自動的に含まれます。`--dry-run` で実行commandを表示できます。分割のvalidatorは `python3 -m unittest discover -s tools/ci -p 'test_*.py'` で両構成を検証します。

Tier-1 CIではrunnerに `--optimized` を渡します。Cargoのdevとtestの**両方**をopt-level 1、debuginfo 2とし、debug assertionとoverflow checkを明示的に有効にします。環境設定は内部のCargo buildにも伝わり、native fixtureが選択する正確なgraphを維持します。Release SDK／benchmarkの設定は変更しません。ローカルで再現する場合は、例えば `python3 tools/ci/run_tests.py owned --optimized` を実行し、Cargoの既定profileを使う場合はこのflagを省略します。各commandは失敗時も所要時間を表示します。単独でbuildするRust conformance hostもopt-level 1とし、assertionとoverflow checkを有効にしたまま、既存のMSVC entry stack設定を保持します。Guestの上限、failure prefix、テストケースは削減しません。

Group全体で、**各OS**の `cargo test --locked --workspace --all-targets --all-features` と同じ検証範囲を維持します。各実行ファイル内は従来どおりlibtestで実行し、process内のfixture再利用を保ちます。Format、Clippy、documentation、validator確認はWindowsの `workspace` またはmacOSの `core` で各OSにつき1回実行し、fuzz lintは両macOSで実行します。WindowsのC ABI／linker testは全suite前の重複実行を除き、core groupで1回実行します（101件の重複実行を削減）。`cargo test` が対象実行ファイルをbuildするため、別の `--no-run` は不要です。完全なschema 20 Windows benchmarkは独立runnerで並列実行し、sample数、assertion、artifact uploadを維持します。SDK隔離とdebugger workflowは役割の異なるend-to-end検証として維持します。

Rust依存関係cacheはtoolchain、Cargo manifest／lockfile、build環境をkeyに含め、OSバージョンとgroupも明示的に分離します。Workspaceの生成物やテスト結果は再利用せず、各groupは毎回テストを実行します。Cache missでも正常に動作します。Action revisionはSwatinem/rust-cache v2.9.3（2026-10-20公開）に固定します。依存関係の復元ではテスト実行時間は省けません。Runnerを増やすことでcompileの一部重複とrunner使用時間を許容し、完了までの経過時間を短縮します。さらに分割する前にcache復元／保存の負荷とgroup間の偏りを測定します。

最適化テストのcacheは独立した `checked-opt1` keyを使用します。SDK producerは固定toolchainを選んだ後でrelease build依存関係をcacheします。隔離consumerには引き続きcheckout、build cache、repositoryのテスト実行ファイルを置きません。文書jobには専用のLinux依存関係cacheを設けます。Cache内容を、選択したcheckの実行の代わりにはしません。

最初の最適化後の全CIは[17分19秒](https://github.com/carimatics/lang/actions/runs/40644725990)で成功しましたが、[2回目](https://github.com/carimatics/lang/actions/runs/40647186045)はWindows coreの依存関係cacheにhitしても19分24秒でした。2つの直列buildに4分01秒と9分33秒かかり、テスト実行は675.38秒から210.88秒へ短縮していました。この実測したcompileのボトルネックに対応して、上記のWindows core groupを追加分割しました。macOSのgroupを増やすと、観測済みのrunner待ちが再発します。実際の[文書専用PR run](https://github.com/carimatics/lang/actions/runs/40646288908)は1分32秒で成功しました。いずれも個別の実測であり、性能保証ではありません。情報源のrun完了・確認日: 2026-10-28 JST。

2026-10-28 JSTの追加調査では、従来のshard構成で直近7件のWindows実行の中央値は18分08秒、macOSも7件で16分54秒でした。代表的な [Windows run](https://github.com/carimatics/lang/actions/runs/40628127920)では依存関係cacheにhitしていても、ownedのテストstep 913秒のうち855.82秒がテスト実行でした。ローカルのM1 Maxで `calls::control::managed_work_profiles_match_vm_at_finite_boundaries` を各3回実行した中央値は、既定test profileで31.19秒、opt-level 1と両checkを有効にした構成で2.51秒でした。最適化側の初回buildには約3分03秒かかっています。1テストのローカル比較であり、CI全体の高速化率の予測ではありません。さらにshardを増やしたりテストごとのprocess実行へ変更したりする前に、cold／warmの全CI、job時間の合計、失敗を比較してください。情報源: link先のGitHub run（2026-10-28 JST完了）と [Cargo profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)（更新日非公表、2026-10-28 JST確認）。

実装した最適化 `owned` partitionも2026-10-28 JSTにローカルで検証し、所有権境界の77テストとtaskの12テストがすべて通過しました。既存の1件のignored testは変更していません。単独conformance hostを含む実行時間は71.01秒と17.93秒、command全体は128.18秒でした。より広いpartitionと内部build経路の動作を確認したもので、platform互換性の根拠は引き続きremoteのtier-1検証です。文書変更の分類も、共通CI変更を取り込む前に実装branch向けの実際の文書専用PRで検証します。

2026-10-28 JSTの調査では、成功済みの基準commit `b62c5fa` を使用しました。[Windows run](https://github.com/carimatics/lang/actions/runs/40538531740)はrun作成から完了まで51分36秒（テスト36分22秒、benchmark 7分28秒、事前ABI／linkerのbuildとテスト2分12秒）、[macOS run](https://github.com/carimatics/lang/actions/runs/40538531591)は29分49秒でした。Windowsで最長のテスト実行ファイル `native_owned_boundary` は10分35秒でした。Hosted runnerでの個別の観測値であり、性能保証ではありません。Failure-prefix case、OS検証範囲、debug assertion、benchmark sampleは削減していません。

最初の[Windows実験](https://github.com/carimatics/lang/actions/runs/40551325087)（`e72e49e`）は依存関係cache hitなしで18分39秒で通過し、全suiteの2,307件のテスト名と結果が基準と完全一致しました。その後、実測3分35秒の `native_task_execution` を `native` から短い `owned` へ移し、負荷を均しました。[最初のmacOS実験](https://github.com/carimatics/lang/actions/runs/40551325072)では前述のrunner待ちが判明しました。両実験の確認日は2026-10-28 JSTです。

情報源：[Cargo test reference](https://doc.rust-lang.org/cargo/commands/cargo-test.html)（複数test targetの直列実行を説明）、[GitHub matrix documentation](https://docs.github.com/en/actions/using-jobs/using-a-matrix-for-your-jobs)、[rust-cache README](https://github.com/Swatinem/rust-cache/tree/v2.9.3)。ページ更新日は非公表、2026-10-28 JST確認。基準runの完了日は2026-10-28 JSTです。

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
