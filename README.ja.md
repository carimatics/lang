<!-- translation-of: README.md; canonical-language: en -->

# lang

[English](README.md)

`lang` は、新しいシステムプログラミング言語と、その完全な開発ツールチェーンを作るためのプレアルファ研究プロジェクトです。実装はRustで開始し、言語・コンパイラ・ブートストラップ手順が自己再生成できるほど安定した段階でセルフホスティングへ移行することを目指します。

source構文・native language ABI・private SDK/package formatは実験段階です。対応する公開component C v1と既存生成clientは、両host適合と旧client移行後、[RFC 0033](docs/ja/rfcs/0033-component-v1-compatibility.md)に従い安定化しました。

**Milestone 0–4のexit gateは完了しました。** [M4能力guide](docs/ja/milestone-4-capabilities.md)に検証済みの開発workflowを記載し、[ボード](docs/ja/milestone-4.md)に全44 deliveryの完了を記録しています。 [M3機能ガイドと全15 gate監査](docs/ja/milestone-3-capabilities.md)に、structured task・有限channel・isolated service・typed native/script plugin・公開C API・installed SDKのcommand、証拠、制約を記載しています。[M3 board](docs/ja/milestone-3.md)は全79 deliveryの完了記録です。[M2機能ガイド](docs/ja/milestone-2-capabilities.md)と過去のbenchmark証拠も保持しています。

2026-10-27 JST検証: source `9e78f2d`のmacOS全7段階（153 suite / 2,321成功）と[Windows全CI](https://github.com/carimatics/lang/actions/runs/40471721760)（153 suite / 2,306成功）が失敗0で成功しました。両hostのcheckout/CargoなしSDK起動も7 profileに成功しています。M4は2026-10-30 JSTに完了し、local 2,514 test、両host journey、最終head CI `41023185791`に成功しました。PR #26はmerge済みで開始mainも同期しました。[M4 guide](docs/ja/milestone-4-capabilities.md)を参照してください。

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
4. 既存のCranelift JITを、各profileの明示的な実行制御に従って開発・embedding・VM／native適合性検証に使います。JITのdebugger登録と広範な最適化は別作業です。
5. バージョン付きインターフェース記述とcanonical component ABIを定義します。レイアウトが判明している場合は直接内部呼び出しを使用し、独立してバージョン管理される境界では生成されたthunk／handleを使用します。
6. 信頼済みネイティブプラグインと、capabilityで制限されたスクリプト／WebAssemblyプラグインを別のデプロイ層として扱います。
7. セルフホスティングは、初期実装を制約する条件ではなく、再現可能なブートストラップのマイルストーンとして扱います。

根拠と未解決のトレードオフは、[アーキテクチャbaseline](docs/ja/architecture.md)に記載しています。

## ドキュメント

- [M4開発toolchain機能guide](docs/ja/milestone-4-capabilities.md): package workflow、editor／debugger、profile、署名導入、両hostの証拠と制限。

- [言語・エコシステム調査](docs/ja/research.md)
- [Benchmark evidence: M2 source/Rust/Cと過去のprototype](docs/ja/benchmarks.md)
- [アーキテクチャbaseline](docs/ja/architecture.md)
- [実装ロードマップ](docs/ja/roadmap.md)
- [Milestone 1機能ガイド](docs/ja/milestone-1.md)
- [Milestone 2 capability guide・全出口監査](docs/ja/milestone-2-capabilities.md)
- [Milestone 3 完了した実装記録](docs/ja/milestone-3.md)
- [Milestone 4 完了した実装ボード](docs/ja/milestone-4.md)
- [Milestone 5 進行中の実装ボード](docs/ja/milestone-5.md)
- [全ファイルreview、2026-10-17](docs/ja/review-2026-10-17.md)
- [リポジトリ全体レビュー、2026-10-20](docs/ja/review-2026-10-20.md)
- [リポジトリ全体レビュー、2026-10-27](docs/ja/review-2026-10-27.md)
- [リポジトリ全体レビュー、2026-10-28](docs/ja/review-2026-10-28.md)
- [リポジトリ全体レビュー、2026-10-30](docs/ja/review-2026-10-30.md)
- [仕様改善調査：Goose・メモリドメイン・GraalVM](docs/ja/spec-improvement-study.md)
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
- [RFC 0034: パッケージワークスペースと CLI 診断（implemented）](docs/ja/rfcs/0034-package-workspaces-and-cli-diagnostics.md)
- [RFC 0035: 決定的な source package snapshot（implemented）](docs/ja/rfcs/0035-source-package-snapshots.md)
- [RFC 0036: 検証付き package formatting（implemented）](docs/ja/rfcs/0036-checked-package-formatting.md)
- [RFC 0037: package lint 診断（implemented）](docs/ja/rfcs/0037-package-lint-diagnostics.md)
- [RFC 0038: package API 文書（implemented）](docs/ja/rfcs/0038-package-api-documentation.md)
- [RFC 0039: package editor snapshot と検証付き edit（implemented）](docs/ja/rfcs/0039-package-editor-snapshots.md)
- [RFC 0040: 上限付き debug adapter session（implemented）](docs/ja/rfcs/0040-bounded-debug-adapter-sessions.md)
- [RFC 0041: 上限付きtoolchain profile（implemented）](docs/ja/rfcs/0041-bounded-toolchain-profiles.md)
- [RFC 0042: 署名付きtoolchain installation（implemented）](docs/ja/rfcs/0042-signed-toolchain-installation.md)
- [RFC 0043: Editionと互換性方針（implemented）](docs/ja/rfcs/0043-editions-and-compatibility.md)
- [RFC 0044: 検証付きsource修正と旧仕様からの移行（implemented）](docs/ja/rfcs/0044-checked-source-migration.md)
- [RFC 0045: Public registryの脅威モデル（accepted）](docs/ja/rfcs/0045-registry-threat-model.md)
- [RFC 0046: Registry protocol・受入・復旧（accepted）](docs/ja/rfcs/0046-registry-protocol-and-recovery.md)
- [RFC 0047: レジストリ権限レコードと最新状態での認可（implemented）](docs/ja/rfcs/0047-registry-authority-records.md)
- [RFC 0048: レジストリ世代と不変公開（implemented）](docs/ja/rfcs/0048-registry-state-and-publication.md)
- [RFC 0049: 固定package graphへのregistry依存統合（accepted）](docs/ja/rfcs/0049-registry-package-integration.md)
- [RFC 0050: Registry cache世代とoffline replay（implemented）](docs/ja/rfcs/0050-registry-cache-and-offline-replay.md)
- [RFC 0051: 認証されたyank・unyank transaction（accepted）](docs/ja/rfcs/0051-registry-yank-transactions.md)
- [コントリビューションガイド](CONTRIBUTING.ja.md)

`cargo run -- toolchain verify --trust-root root.json --metadata channel.json --channel stable --message-format json` は現在のhost向けの上限付き署名metadataを検証し、JSON概要を出力します。rootは別経路で信頼を確認してください。この読取専用検査は新規trust contextを使い、既存installのrollback floorを参照せず、payloadの読取やinstallも行いません。[RFC 0042](docs/ja/rfcs/0042-signed-toolchain-installation.md)を参照してください。

`cargo run -- toolchain init --root target/tools --trust-root root.json` は明示的に信頼したrootを使って新しい分離installationを初期化します。`cargo run -- toolchain install --root target/tools --source distribution --channel stable` はlocal署名distributionからinstallし、active identityをJSONで報告します。activation前にcompiler／SDKの正確なbytesとidentityを検査し、転送失敗で認証済みmetadata floorが進む場合もactive版を保持します。`cargo run -- toolchain identity` はこのcompiler自身のversion、target、build IDを報告します。sourceには明示的に信頼したHTTPS baseも指定でき、TLS検証、redirect／復号なし、有限の転送上限を要求します。署名current bundleへは`toolchain update --root target/tools --source distribution --channel stable`、確認には`list`／`which --root target/tools`、既存bundle選択には`select --root target/tools --channel stable --version 0.1.0`、検証済みcompilerへの引数転送には`run --root target/tools -- run --manifest-path app/Lang.json`を使います。`uninstall`はinactiveのchannel／versionを指定し、`recover --root target/tools`はfloorをresetせず既知の中断処理を解決します。

配布作成者は`cargo run --example toolchain-release -- --sdk target/sdk --output distribution --trust-root root.json --channel stable --version 0.1.0 --sequence 1 --issued UNIX_SECONDS --expires UNIX_SECONDS`を実行し、別途用意した署名情報をstdinへ渡せます。入力はschema-1 JSONで、`signing_keys`に小文字16進Ed25519 seed（各64文字）の配列を指定します。時刻のplaceholderはRFC 0042を満たすUnix秒へ置き換えます。秘密情報をcommand引数、環境変数、logに含めないでください。producerは既存SDKと署名を検証し、private stagingで通常installerを実行してから、channel metadataとcontent-addressed objectだけを含む新規destinationをatomicに公開します。鍵生成やtrust確立は行いません。両hostのinstall／update／失敗journeyは`bf14faf`で検証済みで、M4の完全な新規開発者journeyは`0b6591d`で検証し、[能力guide](docs/ja/milestone-4-capabilities.md)に記録しています。

`cargo run -- lint --manifest-path target/my_app/Lang.json --deny-warnings --message-format json` は guest code を実行せず、lock／artifact を書かずに選択 package と宣言 test を検査します。`LNT0001` は未読 binding を報告し、`_` で始まる名前はこの rule を抑制します。既存 warning は code を保持します。`--deny-warnings` を省くと warning は失敗にしません。[RFC 0037](docs/ja/rfcs/0037-package-lint-diagnostics.md) を参照してください。

`cargo run -- doc --manifest-path target/my_app/Lang.json --locked --offline` は単独で閲覧できる static API reference を `target/lang/<lock-digest>/doc/index.html` に公開します。public signature と type link は検査済み identity を使い、隣接 `///` text と元 source panel を escape します。source panel は private 実装 text も含みます。生成失敗時は前回 lock／output を保持します。[RFC 0038](docs/ja/rfcs/0038-package-api-documentation.md) を参照してください。

`cargo run -- lsp` は明示 workspace に `Lang.json` があると package definition／references を使います。initialize option の `manifestPath`、`package`、`target` で package を選択し、open source overlay と監視変更で一つの許可 epoch を更新します。dependency item、case、parameter、local、field の元 UTF-16 位置を保持します。dirty manifest は保存／refresh 後に semantic navigation を使います。source 権限と edit 前提は [RFC 0039](docs/ja/rfcs/0039-package-editor-snapshots.md) を参照してください。

package completion は公開 module／import／enum 名、scope 内 binding、宣言 receiver 型から取得した field を返します。不完全な識別子は上限付きのメモリ内通常 parser／binder probe で扱い、text edit は元 UTF-16 範囲と epoch／version data を保持します。未対応 receiver 式の field は推測しません。

full semantic token は固定 legend と現在の binding index を使い、不正 source には字句 fallback を使います。CR／LF で分割し、非重複 UTF-16 delta と epoch result ID を返します。semantic-token refresh 対応を宣言した client には source／overlay／watch 無効化後に refresh を要求し、range／delta request は未対応です。

prepare／rename は dependency file を含む source 識別子の検証 edit を提案します。編集する全 file が open・version 付きであることと、client の `workspaceEdit.documentChanges`、複数 file では `transactional`／`textOnlyTransactional` の failure handling が必要です。許可済み全 input bytes の再確認、置換後の再解析、resolved HIR 全体と型の対応検証を行い、衝突、import alias、module、外部 ABI 宣言、古い input を拒否します。提案を disk に直接書き込みません。

literal code action は同じ input／version guard と package linter の rule を再利用し、client が宣言した kind family に限定して提供します。`source.format` は source 位置に依存しない HIR／型の証明後に文書全体を整形し、`quickfix` は一意に選択した未読 binding を rename 証明で `_` prefix にします。initializer と代入の効果を保持し、guest code を実行せず、曖昧／衝突する fix は edit を返しません。

## 開発

`cargo run -- compatibility --message-format json` はprojectを読まずに対応source editionと独立した互換性方針を表示します。editionは`2026`のみで、packageには明示が必要です。公開component C v1は安定化していますが、source・bytecode・package/tooling format・private native linkageはそれぞれの実験契約を維持します。[RFC 0043](docs/ja/rfcs/0043-editions-and-compatibility.md)が非推奨と将来移行の要件を定義し、[M5進捗](docs/ja/milestone-5.md)で実装状況を確認できます。

`cargo run -- fmt --manifest-path target/my_app/Lang.json --check` は既存 CST formatter の要変更 file を無書込みで報告します。`--check` を省くと、選択 dependency／test を含め検証済みの source を file 単位で置換します。元 comment／span と permission を保持し、stale／不正 input は公開前に拒否します。source 変更後の古い lock は stale に保ち、通常 build／run で再生成します。file 単位公開の境界は [RFC 0036](docs/ja/rfcs/0036-checked-package-formatting.md) を参照してください。

`cargo run -- fix --manifest-path target/my_app/Lang.json --message-format json` は選択entryと全宣言testの検証付き`LNT0001` renameをpreviewします。`--apply`を付けると証明を再計算して置換し、initializer・assignment・comment・無関係なbytesを保持します。依存は検査しますが編集しません。reportは対象root・file、元byte span、変更・適用件数を含み、保存したreportは適用入力にできません。`--locked`は操作前のlockを検証し、source編集後もlockは変更せずstaleに保ち、通常build／runで再生成します。公開中は協調writerの排他的書込み時間帯が必要です。全入力を再検証してfile単位でatomic置換しますが、後半I/O失敗時は適用済みprefixを報告しrollbackしません。[RFC 0044](docs/ja/rfcs/0044-checked-source-migration.md)を参照してください。

`cargo run -- migrate --from-profile rfc0006-scalar --to-edition 2026 --source-root target/legacy --message-format json target/legacy/main.lang` は単一の旧scalar sourceの移行をpreviewします。`--apply`を付けると新しいsnapshotを検証してそのfileを置換します。`FILE`はcwd基準または絶対pathで、明示root内に限定し、reportにはroot相対pathを使います。有限profileはprivateな`i64`／`bool`関数と検証付き識別子renameを受け入れ、未対応構文・stale入力・filesystem aliasを拒否します。commentと無関係なbytesを保持し、manifest・lockは作成も変更もしません。適用は上記の公開境界を使います。旧producer・現VM・link済みnativeの全journeyはmacOSとWindowsで成功しています。[保持した証拠](docs/evidence/2026-10-31-m5-source-migration.json)と[B04完了記録](docs/ja/milestone-5.md)を参照してください。

`cargo run -- package --manifest-path target/my_app/Lang.json --list` は予定する固定 source snapshot を無書込みで一覧表示します。`--list` を省くと、元 manifest／source byte、`Lang.lock`、version 付き descriptor の決定的な非圧縮 TAR を公開します。`--locked --offline` は完全一致 replay を検証します。profile 1 は directory alias を拒否し、信頼しない archive の展開や registry 公開は行いません。[RFC 0035](docs/ja/rfcs/0035-source-package-snapshots.md) を参照してください。

`rust-toolchain.toml`で固定したRust **1.99.0**とrustfmt/Clippyを使います。[M2機能guide](docs/ja/milestone-2-capabilities.md)と[M3機能guide](docs/ja/milestone-3-capabilities.md)に実装動作・完了条件・制約を記録しています。`cargo run -- run examples/scripts/tasks.lang`で実装済みsource asyncを実行し、42を返します。[M3 board](docs/ja/milestone-3.md)は完了した79 deliveryの記録です。

M4 の最初のコマンドは `cargo run -- package check-manifest --message-format json examples/packages/hello/Lang.json`。RFC 0034 の上限付き manifest を dependency を読まずに検証します。成功時は出力なし、エラーは stderr の JSON line（exit 1、usage は exit 2）です。host の `package::graph::Graph::resolve` API は明示 manifest の directory 内で完全一致 local dependency graph を snapshot 化し、edge を整列して package／edge／source 上限を適用します。`package::source::analyze` は分離した package root を共有 frontend へ接続し、`cargo test --test package_source` が実 local-dependency fixture を VM と native JIT で実行して両方 42 を返します。`package::lock::Lock::capture` は固定 inventory と解決した entry／test／directory binding を hash 化します。`Lock::check` は厳密な canonical lock を offline replay し、`Lock::publish` は別途渡した directory capability で atomic に置換します。`cargo test --test package_lock` が配置変更、古い input／binding の拒否、失敗時の公開保護を検証します。

`cargo run -- new target/my_app` は完成した新規 package を排他的に作成します。`cargo run -- build --manifest-path target/my_app/Lang.json` は検証済み portable artifact を生成し、`cargo run -- run --manifest-path target/my_app/Lang.json --locked --offline` は既存 VM profile で実行します。manifest option がなければ package command は `./Lang.json` だけを使います。build 成功時は出力なし、run 成功時は i64 の結果を出力して exit 0 です。`target/lang/<lock-sha256>/build/` の不変 generation を先に公開して最後に `Lang.lock` を更新し、失敗時は従来 output を保持します。JSON 診断は元 source の byte span を保持します。file 明示 command は引き続き使えます。workspace command は `--package NAME` で明示 member を選択します。`build --native --sdk SDK` は対応 SDK で再現可能な native executable と固定 build receipt を生成します。

`cargo run -- test --manifest-path target/my_app/Lang.json` は宣言 test を manifest 順に実行します。test 内の `mod _package;` は package の公開 API を参照し、0 を返せば成功です。固定 source scope、元 span、既存 Copy／ownership／task／service engine を使い、entry ごとに実行 control を初期化します。成功時は出力なし、最初の source／runtime／非 0 失敗は exit 1。test 0 件でも main を要求せず package を型検査します。test artifact は同じ lock key の `test/` generation に置き、全 test 成功後だけ公開します。test にも `--locked --offline` を適用できます。

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

