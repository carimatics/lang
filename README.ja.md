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
- [全ファイルreview、2026-10-17](docs/ja/review-2026-10-17.md)
- [RFCプロセスとテンプレート](docs/ja/rfcs/0000-template.md)
- [RFC 0001：プロジェクト憲章と互換性ポリシー](docs/ja/rfcs/0001-project-charter.md)
- [RFC 0002：Prototype integer／structured control-flow semantics](docs/ja/rfcs/0002-prototype-control-flow.md)
- [RFC 0003：Version付きcomponent contractとcapability import](docs/ja/rfcs/0003-versioned-component-contracts.md)
- [RFC 0004：Owned value、明示的共有、cyclic graph storage](docs/ja/rfcs/0004-owned-values-and-cyclic-graphs.md)
- [RFC 0005：Structured task、cancellation、isolated service](docs/ja/rfcs/0005-structured-tasks-and-isolated-services.md)
- [RFC 0006：Source frontendとportable VM program（implemented）](docs/ja/rfcs/0006-source-frontend-and-portable-programs.md)
- [RFC 0007：Sourceのownershipとcore value（accepted）](docs/ja/rfcs/0007-source-ownership-and-core-values.md)
- [RFC 0008：Cleanup、allocator authority、C boundary（accepted）](docs/ja/rfcs/0008-cleanup-allocators-and-c-boundaries.md)
- [RFC 0009：Option／Result全体のownership取り出し（accepted）](docs/ja/rfcs/0009-whole-sum-ownership-extraction.md)
- [RFC 0010：所有値を返すREPL結果のpreview（accepted・current-profile REPL実装済み）](docs/ja/rfcs/0010-owned-repl-previews.md)
- [RFC 0011：Portable ownership artifact（implemented）](docs/ja/rfcs/0011-ownership-artifacts.md)
- [RFC 0012：Native ownershipの継続とmanaged frame（accepted）](docs/ja/rfcs/0012-native-ownership-continuations.md)
- [RFC 0013：Service byte-work budget（accepted・実装予定）](docs/ja/rfcs/0013-service-byte-work-budget.md)
- [コントリビューションガイド](CONTRIBUTING.ja.md)

## 開発

Milestone 1のsource pipelineは複数fileのmodule、型付きfunctionと再帰、copy-only record／enum、網羅的pattern、検証済みportable bytecode、上限付きVM実行に対応しています。Source spanはanalysis、lowering、artifact、runtime stackまで保持されます。Formatterとstdio LSP prototypeは同じlossless parserとrevision付きmodule／診断databaseを使います。

必要な環境は `rust-toolchain.toml` で固定したRust `1.99.0` とrustfmt／Clippyです。補助tool versionが必要な場合は `mise` を使います。

```bash
cargo run -- check examples/analysis/ownership.lang
cargo run -- check examples/analysis/extraction.lang
cargo run -- run examples/scripts/answer.lang
cargo run -- run examples/scripts/bytes.lang
cargo run -- run examples/scripts/owned_bytes.lang
cargo run -- compile --output target/owned-bytes.lbc examples/scripts/owned_bytes.lang
cargo run -- run --artifact target/owned-bytes.lbc
cargo run -- run examples/analysis/extraction.lang
cargo run -- run examples/scripts/utf8.lang
cargo run -- run examples/scripts/text.lang
cargo run -- run examples/scripts/owned_calls.lang
cargo run -- run examples/scripts/cleanup_calls.lang
cargo run -- run examples/scripts/owned_replace.lang
cargo run -- run examples/scripts/owned_aggregate.lang
cargo run -- run examples/scripts/arena.lang
cargo run -- run examples/scripts/arena_owned.lang
cargo run -- run examples/scripts/shared.lang
cargo run -- run examples/scripts/shared_owned.lang
cargo run -- run examples/scripts/vec.lang
cargo run -- run examples/scripts/vec_owned.lang
cargo run -- run examples/scripts/map.lang
cargo run -- run examples/scripts/map_owned.lang
cargo run -- run examples/scripts/nested_collections.lang
cargo run -- run examples/scripts/nested_cycle.lang
cargo run -- run examples/scripts/nested_arena.lang
cargo run -- run examples/scripts/nested_mutation.lang
cargo run -- repl
cargo run --quiet -- repl < examples/repl/owned.repl
cargo run --quiet -- lsp
```

Source arenaでCopy値、Bytes／Text、inline aggregate、同一region referenceを持つ入れ子Vec／Mapのallocation、checked read、replacement、lexical teardownを実行できます。`arena.lang` は値の置換と読み取り後に `9`、`arena_owned.lang` はowning Bytes payloadを持つnodeのcycleを辿って `5` を表示します。Region再enterでは新しいgenerationを使います。変更／exit前にreadを終了し、teardownでnodeはallocationの逆順、各fieldは初期化の逆順に解放します。[Source arena実行](docs/ja/architecture.md#source-arena-execution)を参照してください。2026-10-18 JST更新・確認。

`compile --output ARTIFACT SOURCE`はsource／format検証後にCopy schema 1またはownership schema 2を出力します。`run --artifact ARTIFACT`は独立検証後に明示的entry IDを実行し、埋め込んだmodule snapshotによりsource file削除後も診断を保持します。上のBytes commandは`0`を表示します。Binary入力は64 MiBを上限とし、compile成功後にtemporary file経由で出力を公開します。Entryなしのlibrary artifactはscript実行できません。CLIはowned実行へallocator authorityを与え、host importは登録せず、artifact入力と`--source-root`の併用を拒否します。現owned adapterは到達するhost callをpreflightで拒否し、未使用宣言は実行しません。現在のVM profileの実装であり、native／platformと組合せconformanceは別作業です。[Artifact実行](docs/ja/architecture.md#ownership-artifact-cli-and-execution)を参照してください。2026-10-18 JST更新・確認。

Sourceのshared／weak値で入れ子Vec／Mapと再帰的shared shapeを含むstorable値の明示clone、checked read、replacement、weak生成／clone／upgrade、cleanupを実行できます。`shared.lang` は `9`、`shared_owned.lang` はstrong cycleを拒否し、borrowed guest call越しに入れ子owning nodeを読んで `13` を表示します。Weak ownerはpayload失効後もmetadataを保持します。Live readはalias経由の置換を禁止し、publication失敗時はincoming ownerを解放します。Indirect result preview、native実行、VM／nativeの組合せconformanceは個別gateを維持します。[Source shared実行](docs/ja/architecture.md#source-shared-and-weak-execution)を参照してください。2026-10-18 JST更新・確認。

Source Vecで入れ子collection、shared／weak値、同一regionのarena referenceを含むstorable値のnew／len／push／popと、検証済みshared／mutable element readを実行できます。`vec.lang` は `9`、`vec_owned.lang` はowning pairのgrowth、buffer変更、field置換、値のpop、guest call越しのreadを経て `12` を表示します。Live element loan中はpush／popを禁止し、push失敗時は評価済みowning引数をcleanupします。Indirect preview、native実行、VM／nativeの組合せconformanceは各gateを維持します。[Source Vec実行](docs/ja/architecture.md#source-vec-execution)を参照してください。2026-10-18 JST更新・確認。

Source Mapでi64またはUTF-8 byte単位の厳密なText keyと、storableな入れ子valueのnew／len／insert／get／removeを実行できます。`map.lang` は `49`、`map_owned.lang` はowning pairのgrowth、等価Text keyの置換、guest call越しのread、value削除を経て `33` を表示します。置換で元keyを保持し、旧valueを返します。拒否時は評価済みincoming ownerをcleanupし、live read中はmutationを禁止します。Indirect preview、native実行、VM／nativeの組合せconformanceは各gateを維持します。[Source Map実行](docs/ja/architecture.md#source-map-execution)を参照してください。2026-10-18 JST更新・確認。

入れ子serviceを接続する [example](docs/ja/architecture.md#nested-service-source-execution) は `8`（shared→Vec→Map越しのclone／weak／read）、`21`（container越しの循環拒否）、`4`（arena→Map→Vecの同一region循環）、`6`（入れ子mutable置換／削除）を表示します。Strong edgeとarena brandをcontainer越しに検査し、元spanを維持して反復cleanupします。Ordinary loan、allocator authority、arena ownerは格納不可で、別arena・shared heap間のedge、region越しの生存を拒否します。2026-10-18 JST更新・確認。

`check` はCopyまたはRFC 0007／0009 ownership sourceを該当するHIR／MIR解析で検証し、成功時はexit 0です。Guest codeは実行せず、`main` やhost bindingを必要としません。`run` はCopy profileと、allocator authority、Bytes new／len／get／set／push／clone、所有権を消費するUTF-8変換、Text byte長／clone／concat／to-bytes／equality、入れ子Bytes／Text／Copy record／enumとOption／Result構築・取り出し、lexical loan、cleanupを扱う初期ownership profileに対応します。Answer scriptは `42`、`bytes.lang`／`owned_bytes.lang` は `0`、取り出し／変更exampleは `1`、`utf8.lang` はbyte長の `3`、`text.lang` はcloneと変換後の独立性を確認して `2` を表示します。`owned_aggregate.lang` は入れ子ownerを移動し、投影したreferenceを読んで `9` を表示します。表示前にstorageを解放します。取り出しはcaseを確認し、不一致ならsource位置付きguest panicを報告します。未対応命令／payloadはpreflightで拒否します。

Ownership profileはCopy／owned parameterとresult、borrowed parameterを持つguest functionを自己再帰／相互再帰も含めて実行します。`owned_calls.lang` はBytesの再帰的な移譲とborrowed callを経て `1`、`cleanup_calls.lang` は入れ子aggregate／callのcleanupを経て `13` を表示します。`owned_replace.lang` はowner全体／fieldの置換とscalar loopを経て `3` を表示します。代入先をRHSより先に確定し、新しい値を公開してから古い値をcleanupします。返却値をcallee cleanup成功まで保護し、失敗時は実際に入ったsource call stackを示します。2026-10-18 JST確認。[Call](docs/ja/architecture.md#owned-guest-callとreturn)、[failure exit](docs/ja/architecture.md#owned-failure-exitのconformance)、[置換の根拠](docs/ja/architecture.md#prepared-ownership-replacement)を参照してください。

Compile／runtime errorはexit 1、usage errorはexit 2で、scriptの結果はexit codeではありません。`--source-root DIR` がfile authorityを選び、省略時はcheck／scriptのroot fileの親を使い、REPLにはfile-module authorityを与えません。`run` と `repl` は `--fuel`、`--host-calls`、`--frames`、`--cells` に対応します。REPLはcurrent-profile ownership定義／式とBytes／Text／入れ子resultの上限付きpreviewを扱い、評価ごとにfreshなallocatorとcleanup instanceを使います。CLIはhost importを登録しません。検証日：2026-10-18 JST。

Embedding APIの `lang_vm::unit::owned::execute_preview` は検証済みIDでnamedな引数なしownership functionを実行し、Bytes／Text／入れ子resultを上限付きで表示し、全guest storage解放後にstringを返します。[Source library](examples/analysis/repl_preview.lang) は `cargo test --test owned_execution preview::named_owned` で実行します。Frontendも、検証済みの生成ownership wrapperのresultを通常のHIR／MIR検証越しに推論し、`cargo test --test owned_execution inference::` でその結果を実行します。Embedding sessionは `Session::submit_with_allocator` でownership式のauthorityを明示し、従来の `submit` はallocator authorityを与えません。定義は実行せずcommitし、callerはallocatorの累積quotaを意図的に再利用できます。[Preview](docs/ja/architecture.md#owned-result-previews)、[推論](docs/ja/architecture.md#ownership-expression-inference)、[sessionの根拠](docs/ja/architecture.md#ownership-repl-sessions)。2026-10-18 JST更新・確認。

REPLは検証に成功したitem定義をcommitし、submissionごとに1つのexpressionを新しいVM counterで評価します。Scalar、qualified aggregate、current-profile owned previewを表示し、複数行入力と `:reset`／`:quit` に対応します。保持するのはsource定義だけで、全guest resultは表示前に解放します。失敗したsubmissionは以前の定義と元sourceの診断／stackを保持し、host副作用はrollbackしません。到達可能な未対応ownership操作／resultはprovider callback前に拒否し、後続indirect-service previewはgateを維持します。

`cargo build` 後、LSP clientに `target/debug/lang lsp` を設定します。Stdio profileはinitialize／shutdown／exit、UTF-16位置、full document同期、version付き診断、watched-file refresh、canonicalな4-space document formattingに対応します。Initializeで明示するlocal workspace rootは最大1つで、省略時はopen bufferだけを利用します。Code action、completion、navigation、debugger統合は後続作業です。[Architectureとprotocol情報源](docs/ja/architecture.md#diagnosticslspdebugging)を参照してください。

共有host APIには `lang::frontend`、`lang::artifact`、`lang::repl::Session`、`lang_semantics::analysis`、`lang_semantics::database`、`lang_syntax::format`、MIR／bytecode／VM／Cranelift crateの `unit` moduleがあります。Source解析は明示的なCopy／ownership programを返し、ownership variantは検証済みHIR／MIRを保持します。Compiler／codec／cache budgetを設定できます。VM既定値はfuel 1,000,000、host call 1,000、guest frame 256、live value cell 1,000,000です。Interruption／deadlineはguest safe pointで作用し、trusted native callbackはpreemptできず、unwindは禁止です。

`lang_codegen_clif::program` のunit全体native APIは、guest call、boolean interface、record／enum、patternを含むRFC 0006 Copy profileを扱います。Ownership-aware native lowering、公開C adapter、production native packagingはMilestone 2の残作業です。`cargo run -- 20 22` は以前のscalar VM／JIT比較を保持します。初期native backendはCranelift 0.136.3です。Native linker／debug evidenceと過去のplatform計測は[architecture](docs/ja/architecture.md)、[benchmarks](docs/ja/benchmarks.md)、[roadmap](docs/ja/roadmap.md)へ記録しています。2026-10-17のlocal source-conformance実行環境はmacOS AArch64で、Windows実行にはCI evidenceが必要です。

Code変更の確定前に次を実行します。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

Rustのnative embedding境界で [sourceのBytes引数／返り値](examples/analysis/native_owned_boundary.lang)、[ownerを渡すguest call](examples/analysis/native_owned_calls.lang)、[record](examples/analysis/native_owned_records.lang)、[sumのpayload全体抽出](examples/analysis/native_owned_sums.lang)、[借用fieldの変更](examples/analysis/native_owned_loans.lang)、allocator／buffer／collection／shared／weak／字句arena service（既存の [shared](examples/scripts/shared_owned.lang)、[Map](examples/scripts/map_owned.lang)、[Vec](examples/scripts/vec_owned.lang)、[Text](examples/scripts/text.lang) と [UTF-8](examples/scripts/utf8.lang) の例）、[入れ子arena ownerとguest call越しのread](examples/analysis/native_arena_regions.lang)、[ownerの再open](examples/analysis/native_arena_reuse.lang) が生成機械コードで実行できます。Scalar／Copyの制御、place projection、通常のloan、準備済み置換、CFG引数の同時転送もlowerします。共通service-work admissionのprimitiveは利用でき、有限のowned-service予算はN05のservice計量完了までguest effect前に拒否します。VMとnative adapterはinstrumented controlとusageを共有し、nativeの報告にもloan／escrowのcell数と元の失敗位置を保持します。`cargo test --test native_owned_boundary` でVM／native比較、失敗時cleanup、admission、poison、compile budgetを検証できます。対応Mach-O／COFF codeとメモリ上のmetadataを出力し、exit conformance全体、objectのlink／実行、CLI driverは残作業です。[境界の対応範囲](docs/ja/architecture.md#native-whole-ownerの境界)を参照してください。2026-10-19 JST更新・検証。

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

