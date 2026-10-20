<!-- translation-of: README.md; canonical-language: en -->

# lang

[English](README.md)

`lang` は、新しいシステムプログラミング言語と、その完全な開発ツールチェーンを作るためのプレアルファ研究プロジェクトです。実装はRustで開始し、言語・コンパイラ・ブートストラップ手順が自己再生成できるほど安定した段階でセルフホスティングへ移行することを目指します。

現在は意味論の定義とアーキテクチャの検証を進めています。安定した構文、ABI、パッケージ形式、リリース予定はまだ存在しません。

明示的なsource panic（2026-10-19）：RFC 0008の`core::panic::<T>(code: i64)`は復帰せず、符号付きcode、元の呼び出しspan、guest stack、owner cleanupをVM／nativeで保持します。[例](examples/analysis/explicit_panic_owned.lang)と[version付き契約](docs/ja/rfcs/0014-explicit-source-panic.md)を参照してください。Cを含まない明示的panicのartifact unitはownership schema 3、従来のownership unitはschema 2を使います。C対応unitは明示的panicも含めschema 4を選びます。引き続き実験的な実装です。

C対応artifact transport（2026-10-20）：[RFC 0017](docs/ja/rfcs/0017-c-aware-ownership-artifacts.md)でownership schema 4を追加します。有限aggregate署名と明示的panicを含むHost／C、Guest／CExport宣言全体を独立decode後も保持します。Sourceから復元したfixtureは明示VM／native context経由で実C codeを呼び出し、CLIは消費量ゼロでC実行を拒否します。Native配布にはP02のinstall証拠があります。

Native compiler record（2026-10-20）：[RFC 0018](docs/ja/rfcs/0018-native-compiler-records.md)で元object byte／target／symbol／source rangeをtransportし、正確なportable意味論artifactとの対応を検査します。Rustの`native_record::Record` APIはdecodeを制限し、live addressやcapabilityを保持しません。`Record::prepare_copy`と`program::PreparedObject::bind`で、JITと同じcanonical値検査を持つimage保持付きCopy呼出しを行います。Ownershipは`owned::RecordedObject::from_record`で検証済みplanを再構築します。Macのlinked fixtureは生成元破棄後に意味論／記録fileを読み直し、record／enum呼出し、managed Textのservice work、明示的panicを保持し、起動時にsource分析も機械語生成も行いません。Copy nativeは既存のtrustedな同期・非計量契約を維持します。通常production runtimeは下記で実装しました。保存済みC起動は下記で実装し、SDK組立は後述のとおり実装し、公開linkは後述のとおり実装し、両host clean installにはP02の証拠があります。

通常owned native runtime（2026-10-20）：[RFC 0019](docs/ja/rfcs/0019-owned-native-entry-runtime.md)で`lang_codegen_clif::owned::runtime::execute_entry`を追加します。Compile／link済みownership program、明示的な引数なしi64 entry、cleanup Instance、元の`Allocator<P>`を渡します。C exportとproduction managed primitivesを共有し、C contextやtest runtimeは不要です。Configで有限frame／cell（既定64／65,536）、準備予算、controlを指定します。失敗時もusage、function／span／panic code、guest stack、cleanup reportを保持し、host storage完了失敗も別途返します。正常な失敗後はretryでき、host panicは元Instanceを永続poisonします。10 sourceで全managed familyとpanic、全fuel／work／provider拒否位置、frame／cell／interrupt／deadline、保存済みlinked起動と解放を検証します。`cargo test --test native_owned_boundary production_owned_entry`で確認できます。公開SDK linkは後述のとおり実装し、両hostの隔離installにはP02の証拠があります。

Raw C context support（2026-10-19）：Rust埋め込みAPIの`lang_codegen_clif::c_abi::context::ContextSupport`は、登録済みProgram向けのprofile 1 create／destroy／last_error symbolを生成します。実C callerでdescriptorをコピーしたopaque contextを作成し、整数のerror summaryを照会・破棄できます。すべての呼び出しとcontext破棄が終わるまでsupport・登録code・allocator・callback user dataを保持してください。Native source exportの明示embedding入口は下記のF15です。C対応object公開はF05に維持します。

Native C source export（2026-10-19 JST確認）：`lang_codegen_clif::c_abi::exports::NativeExports`と信頼する`ExportRuntime` factoryを`ContextSupport`／`CCompiledProgram`と共に保持すると、実C callerからsource increment／panic、bool／Unit、C callbackを実行できます。生成machine codeがsource bodyを実行し、callbackへ元のopaque contextを渡します。各contextが実C providerと累積quotaを保持し、失敗時はcaller resultを変更しません。`cargo test --test c_abi imports::native::exports`で実行・再入・quota／OOM・host panicを検証します。Factoryは型付きstorage／cleanupを実装する信頼されたhost部品です。後続のF16で再利用可能なnative runtimeとaggregate／owner bodyを接続しました（下記）。Linked C object／standalone公開はF05に残ります。

Native C runtime（2026-10-20 JST実装・確認）：`lang_codegen_clif::c_abi::runtime::NativeFactory::new(&compiled, &artifact, limits)`を`NativeExports`へ渡すと、fixture独自のruntimeを実装せずに型付きnative exportを実行できます。[Record／入れ子sum](examples/analysis/c_aggregate_exports.lang)、[全core error／空record](examples/analysis/c_copy_export_matrix.lang)、[owner付き再帰・C callback・panic](examples/analysis/c_owned_exports.lang)の例があります。I64／bool／Unit／recordを混ぜた256引数も扱います。Bodyは生成machine codeで実行し、Bytes／Text／Vec／Map／shared／arenaとguest callには共有storage／loan／cleanup primitivesを使います。全guest領域を元のC allocatorで計上・解放し、cleanup後に検証済みresultだけを公開します。`cargo test --test c_abi imports::native::exports`で、実C caller・VMとの11 source比較・全fuel／provider失敗prefixを検証します。これはJIT／対応linked image向けの明示Rust embedding APIです。既定CLIのC実行は拒否し、RFC 0017のschema 4では権限を付与せずC識別を保存できます。Linked C objectと両hostの境界検証はF05–F07で記録済みです。保存済み記録からのC起動は下記で実装し、公開SDK linkは後述のとおり実装し、隔離installにはP02の証拠があります。

保存済みC object起動（2026-10-20 JST）：`CObjectArtifact::into_recorded`でRFC 0018の記録を保存し、decode済みの検証済み宣言から`c_abi::Artifact`を再構築、`RecordedObject::prepare_c`でC interfaceとprivate bridge symbolを復元します。元objectのimageを従来のunsafe契約でbindし、`NativeFactory`と`ObjectExports`で元opaque context、allocator、明示import grant、cleanupを保って実行します。起動時のsource分析・機械語生成・JITは不要です。全managed family、aggregate／256引数、有限Copy全case、owner付き再帰／callback／panic、fuel／割当失敗、両object形式と予算を検証します。`cargo test --test c_abi persisted_c_object`で確認できます。SDK組立は後述のとおり実装し、完全一致toolchainのlinkは後述のとおり実装し、両host隔離installにはP02の証拠があります。

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
- [RFC 0016：間接値のREPL結果プレビュー（implemented・実装済み）](docs/ja/rfcs/0016-indirect-repl-previews.md)
- [RFC 0017：C対応ownership artifact（implemented・実装済み）](docs/ja/rfcs/0017-c-aware-ownership-artifacts.md)
- [RFC 0018：Native compiler record（implemented・実装済み）](docs/ja/rfcs/0018-native-compiler-records.md)
- [RFC 0019：通常owned native entry runtime（implemented・実装済み）](docs/ja/rfcs/0019-owned-native-entry-runtime.md)
- [RFC 0020：実験段階のnative SDKとscope付き起動（accepted・受理済み）](docs/ja/rfcs/0020-native-sdk-and-startup.md)
- [RFC 0011：Portable ownership artifact（implemented）](docs/ja/rfcs/0011-ownership-artifacts.md)
- [RFC 0012：Native ownershipの継続とmanaged frame（accepted）](docs/ja/rfcs/0012-native-ownership-continuations.md)
- [RFC 0013：Service byte-work budget（accepted・実装予定）](docs/ja/rfcs/0013-service-byte-work-budget.md)
- [コントリビューションガイド](CONTRIBUTING.ja.md)

## 開発

RFC 0008の`core::drop::<T>(value) -> core::Unit`でownerを即座に破棄し、Copyの元の値を維持します。Loan／allocator特殊化は`SRC0024`、借用中ownerのmoveは`SRC0021`、破棄済みownerの使用は`SRC0020`で拒否します。[Bytes](examples/analysis/explicit_drop.lang)と[shared／weak](examples/analysis/explicit_drop_shared.lang)の例はVM／nativeで42を返します。Dropを既存cleanup命令へ下げてownership schema 2で往復でき、source `core::panic`はF13に残ります。2026-10-19 JST確認。

Milestone 1のsource pipelineは複数fileのmodule、型付きfunctionと再帰、copy-only record／enum、網羅的pattern、検証済みportable bytecode、上限付きVM実行に対応しています。Source spanはanalysis、lowering、artifact、runtime stackまで保持されます。Formatterとstdio LSP prototypeは同じlossless parserとrevision付きmodule／診断databaseを使います。

RFC 0008のC import／export宣言は、source checkから独立検証済みownership MIRまで型付きC識別を保持します。呼出しにはlexicalな`unsafe`が必要で、通常の型・move・loan・cleanup規則を維持します。不正なC署名／呼出しには`SRC0025`、従来`unsafe`という名前には移行`SRC0024`を示します。明示Rust context／binding APIで[scalar／Unit](examples/analysis/c_scalar_imports.lang)と[finite Copy aggregate import](examples/analysis/c_aggregate_imports.lang)をVM／native実行できます。Record、enum、入れ子Option／Result、core errorはfield単位でC値と変換し、実context allocatorを使用します。生成C11 headerでcallback署名も検査します。`cargo test --test c_abi imports::aggregate_c_imports_execute_records_and_sums_from_source`で確認できます。Raw C context factoryとnative source exportの明示embedding入口は実装済みです（上記F15）。Aggregate／owner bodyの明示native exportは下記F16で検証済みです。C object公開と完全なnative packagingは未完成です。C対応artifactはschema 4を使い、bindingやcapabilityを保存せずC識別を保持します。DecodeだけではC実行権限を与えません。既定CLI／従来実行入口は引き続き作用前にC unit全体を拒否します。i64引数1個の従来`extern fn` host importの動作は維持します。2026-10-20 JST確認。

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

`compile --output ARTIFACT SOURCE`はsource／format検証後にCopy schema 1またはownership schema 2／3／4（2は通常、3は明示的panic、4はC対応）を出力します。`run --artifact ARTIFACT`は独立検証後に明示的entry IDを実行し、埋め込んだmodule snapshotによりsource file削除後も診断を保持します。上のBytes commandは`0`を表示します。Binary入力は64 MiBを上限とし、compile成功後にtemporary file経由で出力を公開します。Entryなしのlibrary artifactはscript実行できません。CLIはowned実行へallocator authorityを与え、host importは登録せず、artifact入力と`--source-root`の併用を拒否します。現owned adapterは到達するhost callをpreflightで拒否し、未使用宣言は実行しません。現在のVM profileの実装であり、native／platformと組合せconformanceは別作業です。[Artifact実行](docs/ja/architecture.md#ownership-artifact-cli-and-execution)を参照してください。2026-10-20 JST更新・確認。

Sourceのshared／weak値で入れ子Vec／Mapと再帰的shared shapeを含むstorable値の明示clone、checked read、replacement、weak生成／clone／upgrade、cleanupを実行できます。`shared.lang` は `9`、`shared_owned.lang` はstrong cycleを拒否し、borrowed guest call越しに入れ子owning nodeを読んで `13` を表示します。Weak ownerはpayload失効後もmetadataを保持します。Live readはalias経由の置換を禁止し、publication失敗時はincoming ownerを解放します。間接値previewは[RFC 0016](docs/ja/rfcs/0016-indirect-repl-previews.md)で実装しました。Native実行とVM／nativeの組合せconformanceは個別gateを維持します。[Source shared実行](docs/ja/architecture.md#source-shared-and-weak-execution)を参照してください。2026-10-20 JST更新・確認。

Source Vecで入れ子collection、shared／weak値、同一regionのarena referenceを含むstorable値のnew／len／push／popと、検証済みshared／mutable element readを実行できます。`vec.lang` は `9`、`vec_owned.lang` はowning pairのgrowth、buffer変更、field置換、値のpop、guest call越しのreadを経て `12` を表示します。Live element loan中はpush／popを禁止し、push失敗時は評価済みowning引数をcleanupします。間接値previewは[RFC 0016](docs/ja/rfcs/0016-indirect-repl-previews.md)で実装しました。Native実行とVM／nativeの組合せconformanceは各gateを維持します。[Source Vec実行](docs/ja/architecture.md#source-vec-execution)を参照してください。2026-10-20 JST更新・確認。

Source Mapでi64またはUTF-8 byte単位の厳密なText keyと、storableな入れ子valueのnew／len／insert／get／removeを実行できます。`map.lang` は `49`、`map_owned.lang` はowning pairのgrowth、等価Text keyの置換、guest call越しのread、value削除を経て `33` を表示します。置換で元keyを保持し、旧valueを返します。拒否時は評価済みincoming ownerをcleanupし、live read中はmutationを禁止します。間接値previewは[RFC 0016](docs/ja/rfcs/0016-indirect-repl-previews.md)で実装しました。Native実行とVM／nativeの組合せconformanceは各gateを維持します。[Source Map実行](docs/ja/architecture.md#source-map-execution)を参照してください。2026-10-20 JST更新・確認。

入れ子serviceを接続する [example](docs/ja/architecture.md#nested-service-source-execution) は `8`（shared→Vec→Map越しのclone／weak／read）、`21`（container越しの循環拒否）、`4`（arena→Map→Vecの同一region循環）、`6`（入れ子mutable置換／削除）を表示します。Strong edgeとarena brandをcontainer越しに検査し、元spanを維持して反復cleanupします。Ordinary loan、allocator authority、arena ownerは格納不可で、別arena・shared heap間のedge、region越しの生存を拒否します。2026-10-18 JST更新・確認。

`check` はCopyまたはRFC 0007／0009 ownership sourceを該当するHIR／MIR解析で検証し、成功時はexit 0です。Guest codeは実行せず、`main` やhost bindingを必要としません。`run` はCopy profileと、allocator authority、Bytes new／len／get／set／push／clone、所有権を消費するUTF-8変換、Text byte長／clone／concat／to-bytes／equality、入れ子Bytes／Text／Copy record／enumとOption／Result構築・取り出し、lexical loan、cleanupを扱う初期ownership profileに対応します。Answer scriptは `42`、`bytes.lang`／`owned_bytes.lang` は `0`、取り出し／変更exampleは `1`、`utf8.lang` はbyte長の `3`、`text.lang` はcloneと変換後の独立性を確認して `2` を表示します。`owned_aggregate.lang` は入れ子ownerを移動し、投影したreferenceを読んで `9` を表示します。表示前にstorageを解放します。取り出しはcaseを確認し、不一致ならsource位置付きguest panicを報告します。未対応命令／payloadはpreflightで拒否します。

Ownership profileはCopy／owned parameterとresult、borrowed parameterを持つguest functionを自己再帰／相互再帰も含めて実行します。`owned_calls.lang` はBytesの再帰的な移譲とborrowed callを経て `1`、`cleanup_calls.lang` は入れ子aggregate／callのcleanupを経て `13` を表示します。`owned_replace.lang` はowner全体／fieldの置換とscalar loopを経て `3` を表示します。代入先をRHSより先に確定し、新しい値を公開してから古い値をcleanupします。返却値をcallee cleanup成功まで保護し、失敗時は実際に入ったsource call stackを示します。2026-10-18 JST確認。[Call](docs/ja/architecture.md#owned-guest-callとreturn)、[failure exit](docs/ja/architecture.md#owned-failure-exitのconformance)、[置換の根拠](docs/ja/architecture.md#prepared-ownership-replacement)を参照してください。

Compile／runtime errorはexit 1、usage errorはexit 2で、scriptの結果はexit codeではありません。`--source-root DIR` がfile authorityを選び、省略時はcheck／scriptのroot fileの親を使い、REPLにはfile-module authorityを与えません。`run` と `repl` は `--fuel`、`--service-work`、`--host-calls`、`--frames`、`--cells` に対応します。REPLはcurrent-profile ownership定義／式とBytes／Text、Vec、キー順Map、shared内容、weak生存状態、入れ子resultの上限付きpreviewを扱い、評価ごとにfreshなallocatorとcleanup instanceを使います。Arena参照の領域外への返却は禁止のままです。CLIはhost importを登録しません。検証日：2026-10-20 JST。

Embedding APIの `lang_vm::unit::owned::execute_preview` は検証済みIDでnamedな引数なしownership functionを実行し、Bytes／Text／入れ子resultを上限付きで表示し、全guest storage解放後にstringを返します。[Source library](examples/analysis/repl_preview.lang) は `cargo test --test owned_execution preview::named_owned` で実行します。Frontendも、検証済みの生成ownership wrapperのresultを通常のHIR／MIR検証越しに推論し、`cargo test --test owned_execution inference::` でその結果を実行します。Embedding sessionは `Session::submit_with_allocator` でownership式のauthorityを明示し、従来の `submit` はallocator authorityを与えません。定義は実行せずcommitし、callerはallocatorの累積quotaを意図的に再利用できます。[Preview](docs/ja/architecture.md#owned-result-previews)、[推論](docs/ja/architecture.md#ownership-expression-inference)、[sessionの根拠](docs/ja/architecture.md#ownership-repl-sessions)。2026-10-18 JST更新・確認。

REPLは検証に成功したitem定義をcommitし、submissionごとに1つのexpressionを新しいVM counterで評価します。Scalar、qualified aggregate、current-profile owned previewを表示し、複数行入力と `:reset`／`:quit` に対応します。保持するのはsource定義だけで、全guest resultは表示前に解放します。失敗したsubmissionは以前の定義と元sourceの診断／stackを保持し、host副作用はrollbackしません。到達可能な未対応ownership操作／resultはprovider callback前に拒否し、後続indirect-service previewはgateを維持します。

`cargo build` 後、LSP clientに `target/debug/lang lsp` を設定します。Stdio profileはinitialize／shutdown／exit、UTF-16位置、full document同期、version付き診断、watched-file refresh、canonicalな4-space document formattingに対応します。Initializeで明示するlocal workspace rootは最大1つで、省略時はopen bufferだけを利用します。Code action、completion、navigation、debugger統合は後続作業です。[Architectureとprotocol情報源](docs/ja/architecture.md#diagnosticslspdebugging)を参照してください。

共有host APIには `lang::frontend`、`lang::artifact`、`lang::repl::Session`、`lang_semantics::analysis`、`lang_semantics::database`、`lang_syntax::format`、MIR／bytecode／VM／Cranelift crateの `unit` moduleがあります。Source解析は明示的なCopy／ownership programを返し、ownership variantは検証済みHIR／MIRを保持します。Compiler／codec／cache budgetを設定できます。VM既定値はfuel 1,000,000、host call 1,000、guest frame 256、live value cell 1,000,000です。Interruption／deadlineはguest safe pointで作用し、trusted native callbackはpreemptできず、unwindは禁止です。

`lang_codegen_clif::program` のunit全体native APIは、guest call、boolean interface、record／enum、patternを含むRFC 0006 Copy profileを扱います。Ownership-aware native loweringと明示C adapterは下記で実装済みで、production native SDK packagingはMilestone 2の残作業です。`cargo run -- 20 22` は以前のscalar VM／JIT比較を保持します。初期native backendはCranelift 0.136.3です。Native linker／debug evidenceと過去のplatform計測は[architecture](docs/ja/architecture.md)、[benchmarks](docs/ja/benchmarks.md)、[roadmap](docs/ja/roadmap.md)へ記録しています。2026-10-17のlocal source-conformance実行環境はmacOS AArch64で、Windows実行にはCI evidenceが必要です。

Code変更の確定前に次を実行します。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
cargo doc --workspace --no-deps
```

Rustのnative embedding境界で [sourceのBytes引数／返り値](examples/analysis/native_owned_boundary.lang)、[ownerを渡すguest call](examples/analysis/native_owned_calls.lang)、[record](examples/analysis/native_owned_records.lang)、[sumのpayload全体抽出](examples/analysis/native_owned_sums.lang)、[借用fieldの変更](examples/analysis/native_owned_loans.lang)、allocator／buffer／collection／shared／weak／字句arena service（既存の [shared](examples/scripts/shared_owned.lang)、[Map](examples/scripts/map_owned.lang)、[Vec](examples/scripts/vec_owned.lang)、[Text](examples/scripts/text.lang) と [UTF-8](examples/scripts/utf8.lang) の例）、[入れ子arena ownerとguest call越しのread](examples/analysis/native_arena_regions.lang)、[ownerの再open](examples/analysis/native_arena_reuse.lang) が生成機械コードで実行できます。Scalar／Copyの制御、place projection、通常のloan、準備済み置換、CFG引数の同時転送もlowerします。VMとnative adapterはinstrumented controlとusageを共有し、nativeの報告にもloan／escrowのcell数と元の失敗位置を保持します。`cargo test --test native_owned_boundary` でVM／native比較、失敗時cleanup、admission、poison、compile budgetを検証できます。対応Mach-O／COFF codeとメモリ上のmetadataを出力し、macOSのstandalone owned programはdirect／response-fileでlink／run検証済みで、Source compileと生成startupは後述し、SDK配布は残作業です。[境界の対応範囲](docs/ja/architecture.md#native-whole-ownerの境界)を参照してください。2026-10-19 JST更新・検証。

Owned VMとcanonical native profileで、現在のallocator、Bytes／Text、Vec／Map、shared／weak、arenaの全serviceに有限のservice-work予算を適用できます。Invocation共通counterがguest call、loan、service viewへ伝わり、型付きcopy／grow、UTF-8 scan、Map lookup、managed payloadの検査を決定的なeventで計量します。拒否時はlimit／used／requestedの数値を保持し、必須cleanupを課金せずownerを解放します。[Bufferの例](examples/analysis/native_buffer_work.lang) は引き続き37単位で67を返し、[managed Text payloadの例](examples/analysis/native_managed_text_work.lang) はVec／shared／arenaのviewを通ります。両tier-1 hostのnative object実行は下記で実証し、production driverのpackagingにはP02のinstall証拠があります。2026-10-19 JST確認。

Trusted Rust APIのObjectArtifact::bindはlinked imageのownerを保持し、JITと同じ検証付きowned gatewayを利用します。Symbol解決前にhost targetと累積compile budgetを検査し、symbol欠落は構造化errorで返します。実際のmacOS linked imageでVec／Text／shared／arena混在例を実行し、workの全prefixでVMと一致します。Unsafe bindingには、同じartifact／toolchainからlinkしたcodeと、ownerの寿命中の有効性が必要です。下記の開発用standalone driverで両tier-1 targetを実証し、clean-install packagingにはP02のinstall証拠があります。2026-10-19 JST確認。

macOS AArch64とWindows x86-64 MSVCの[standalone conformance test](tests/support/native_static.rs)は、同じ31 source unitをhostごとにdirect／response-file経由で62 executableへlinkし、空白を含むpathも検証します。Work／fuelの全prefix、allocation拒否、割り込み、frame／cell上限、panic、cleanupをVMと比較します。[Copy payload制御の例](examples/analysis/native_owned_control.lang)はnominal record／enum、managed Vec、boolのcall、所有権付きreturnを組み合わせます。516f663の[Windows全workflow run 39198008813](https://github.com/carimatics/lang/actions/runs/39198008813)でnative testと後続prototype benchmarkの両方が成功しました。開発用driverは対応metadataを再生成・照合し、link済み機械関数を呼びます。SDK組立は後述のとおり実装し、公開linkは後述のとおり実装し、隔離installにはP02の証拠があります。実行：cargo test --test native_owned_boundary standalone_owned_object -- --nocapture。2026-10-19 JST確認。

CLIのsource、保存artifact、REPLで `--service-work N` を使えます。省略時は無制限、0は無償の操作だけを許し、範囲はu64です。上限はartifactへ保存せず実行時に設定し、REPLでは投稿ごとにresetします。Runtime errorにはservice workのusageと拒否のlimit／used／requestedを表示します。引数不正はexit 2、scriptの予算拒否はexit 1、REPLはruntime error後も継続します。CLI実行はVMで、native実行への置換を意味しません。2026-10-19 JST検証。

```bash
cargo run --bin lang -- run --service-work 37 examples/analysis/native_buffer_work.lang
```

### Native program出力

2026-10-20 JST更新・検証：`lang native` はsource unit全体を `aarch64-apple-darwin` または `x86_64-pc-windows-msvc` 向けにcompileします（既定：compiler host）。

```bash
cargo run --bin lang -- native --output /tmp/lang-owned-native examples/scripts/owned_bytes.lang
cargo run --bin lang -- native --target x86_64-pc-windows-msvc --output /tmp/lang-c-native examples/analysis/c_owned_exports.lang
```

新しいoutput directoryを指定します。`package.json`、独立に検証した `program.lbc`／`program.native`、元の `program.o`／`program.obj`、canonicalな `bindings.rs` を出力します。通常のCopy／ownership programには実際の引数なしi64 mainを起動する `main.rs` を、C unitには既定entryを持たない `program.h` を追加します。完全な出力だけをatomicに公開し、既存destinationを保持します。

生成startupはproduction runtime APIを使ってscriptの値を表示し、元のobjectのbinding前にSDK identityを照合します。[Compiler test](tests/native_compiler.rs)はsource削除後にこれらの出力と実C callerをbuild・実行します。公開 `link-native` は後述のとおり実装し、隔離installはaccepted [RFC 0020](docs/ja/rfcs/0020-native-sdk-and-startup.md)に沿って検証済みです。SDKは完全一致するRust toolchainとnative linkerを必要とし、C hostにはC11も必要です。

### Native package検証API

2026-10-20 JST更新・検証：`native::manifest::SdkSnapshot::open` と `ProgramSnapshot::open` はRFC 0020のdataを検証し、後続の信頼済みlinkに使うprivate copyを保持します。重複／未知field・role、不正path、予算、SDK／build／toolchain tag、hash、意味論／native／objectの不一致を拒否します。SDKは64 KiB bufferでstreamコピーし、program dataはbindingやguest実行なしに独立decodeとplan検査を行います。

Callerはsource rootと信頼済みprivate staging parentを選び、toolが使う間はsnapshot ownerを保持してfileを変更しません。Snapshot成功後は元入力を削除でき、失敗／drop時は所有するstaging directoryだけを削除します。Native fileとRust host glueは信頼済み入力であり、hashは対応検査です。このAPIはrustcを起動せず、applicationをlinkしません。公開 `link-native` は後述のとおり実装し、両host隔離installにはP02の証拠があります。[検証test](tests/native_packages.rs)を参照してください。`native_manifests` はfilesystem／native権限を持たない上限付きparser fuzz targetです。

### Scoped native SDK runtime API

2026-10-20 JST更新・検証：`lang_codegen_clif::sdk::runtime` は元のlinked imageに対するunsafeな `execute_copy`、`execute_owned`、`with_c` APIを提供します。Symbol解決前にhost target／buildを検査し、上限付きsemantic／native記録を独立decodeしてproductionのbinding／runtime primitiveを使います。Callerは正確に対応するimageと全依存物を保持します。起動時にはsource compile、code生成／JIT、guest命令のinterpretを行いません。

Copyは同期unmetered ABIのため、未発火のinterruption／deadlineを含め設定済みcontrolをすべて拒否します。Ownershipはcallerの実際のallocator、cleanup Instance、controlを使い、usage、poison、元のfailure reportとsemantic source byteを保持します。C bindingは明示closure内に限られ、unwind時を含めclosure終了前にowner thread上で全callを戻し、全contextを破棄します。Unsafe callerはraw client／contextの持ち出しを防ぎ、実際のC設定、provider、import grantを供給します。Metadataは権限を与えません。[runtime実装](crates/codegen-clif/src/sdk/runtime.rs)と[sourceなしの実行／guard／状態test](tests/native_compiler.rs)を参照してください。公開SDK linkは後述のとおり実装し、両host隔離installにはP02の証拠があります。

### Native SDK assembly

2026-10-20 JST更新・確認：`python3 tools/build-native-sdk.py --output SDK` で新しい対応host SDKをbuild・組立します。Releaseには `--release` を加えます。Cacheの依存variant一覧ではなく、成功した正確なCargo artifact／build-script出力からRust／proc-macro libraryとnative support archiveを選びます。Compiler、scope付きruntime、共通C header、Copy／owned／C起動例、日英の説明、上限付き `sdk.json` を収録します。Production検証APIでprivate copyをstream／hash検証してからexclusive公開し、compilerの実行権限を保持します。[SDK build／利用方法](docs/ja/native-sdk.md)と[組立／sourceなしtest](tests/native_sdk.rs)を参照してください。公開 `link-native` は後述のとおり実装し、両host隔離installにはP02の証拠があります。

### Native executable linking

2026-10-20 JST更新・確認：`lang link-native --sdk SDK --package PROGRAM --output EXE` は対応SDK／program全体をsnapshot・検証し、実rustc release／commit／hostを照合して元objectを配布libraryとlinkします。Compile成功後だけ新executableを公開し、Cargo／registry／repository依存探索は行いません。Install済みcompilerは `RUSTC` で選択でき、host glueはRust 2024とpanic unwindを使います。

Copy／通常owned packageは生成mainを使います。C libraryは `--host HOST.rs` と必要に応じて繰返し `--object HOST.o` を要求します。Hostはコピーした `bindings.rs` の隣に置く単一Rust rootで、信頼済みC objectと明示context／allocator／import grantを使い、C mainを合成しません。Link後はsource／SDK／program directoryが不要です。UTF-8 response引数で空白を保持し、tool不一致、入力／診断上限、link失敗、並行公開は既存出力を保ちます。[SDK説明](docs/ja/native-sdk.md)、[link実装](src/native/link.rs)、[公開CLI／拒否／atomicity test](tests/native_link.rs)を参照してください。両host隔離installにはP02の証拠がありますが、packagingやQ／B／G全体の判定は別です。

### Native SDK isolated installation

2026-10-20 JST更新・確認：[隔離install CI](.github/workflows/native-sdk-install.yml) はmacOS AArch64／Windows x86-64でSDKを作るjobと利用jobを分け、SDKとprivate Python probeだけをtarで渡して実行権限を保持します。利用jobはrepoをcheckoutせず、元の.git／Cargo manifest／src／crates／targetがないことを確認します。[Probe](tools/check-native-sdk-install.py) は対応Rust／C toolchainを使い、CargoをPATHから除去して空のCARGO_HOMEでCopy／owned／実Cをcompile／linkします。Private source／SDK／programを削除後に実行し、C allocator／contextの作成・破棄も検証してJSON証拠を保存します。

両host隔離の検証（2026-10-20 JST）：完全一致head `c54af31` の [run 39370391958](https://github.com/carimatics/lang/actions/runs/39370391958) は2026-10-20 04:44:42 UTCに成功しました。Mac consumer `118458816864`／Windows consumer `118458816960` のJSONで、両方の元repo／target入力12 pathの不在、Cargoなし、空のCARGO_HOME、privateなlink入力削除、Copy／owned／C全3件の成功を照合しました。P02は完了し、G15を完了し、RFC 0020をimplementedとしました。対応Windows [run 39369585929](https://github.com/carimatics/lang/actions/runs/39369585929)（head `991768e`）のfmt／C・linker検証／Clippy／全test／rustdocは2026-10-20 04:47:51 UTCまでに全成功しました。`c54af31` との差分は転送workflowのみです。Prototype benchmarkは実行中で、workflow全体や性能gateの成功とは数えません。Q／B／Gと最終guideは未完了です。GitHubの[hosted runner仕様](https://docs.github.com/en/actions/using-github-hosted-runners/about-github-hosted-runners)はjobごとの新VMを説明します（今回のrunnerはsingle-CPU例外ではありません）。更新日表示はなく2026-10-20 JST確認です。

Local probeだけでは両host隔離の証拠になりません。対応する別jobの結果を[Todo](docs/ja/milestone-2.md)に記録しました。Q／B／Gと最終guideはそれぞれの完了証拠を必要とします。Artifact転送の権限保持はGitHub公式 [upload-artifact](https://github.com/actions/upload-artifact/tree/cf430e030ddbb5b0abf93d22962f4752f3646cd9#permission-loss)／[download-artifact](https://github.com/actions/download-artifact/tree/9000827ccba6bdab643e8b6fd33ac0654aef8333#maintaining-file-permissions) に従います。Living READMEに更新日表示はなく、2026-10-20 JST確認です。

### Combined source conformance

2026-10-20 JST更新・確認：[複合source](examples/conformance/owned_pipeline.lang) はVecからMapへのowner移動・置換、Text／Bytesを持つaggregate、再帰guest call、shared／weak、arenaのowned collectionと入れ子readを組み合わせます。RFC 0004どおりshared→arena格納をCrossRegionで拒否し、拒否したcloneの解放、arena scope終了後のweak失効も確認します。通常entryは42、同じ処理をarena内でpanicさせるvariantは−42です。

[実production entry比較](tests/support/native_entry_runtime.rs) はVMと値・usage・元span／stack／reportを照合し、両variantの全fuel／service-work prefix、provider割当て拒否位置、frame／cell境界で元Instanceとallocatorの解放を確認します。[Source-native test](tests/native_compiler.rs) は元source削除後の実AOTで両variantを実行します。[C runtime比較](tests/support/c_exports_runtime.rs) と[保存C object比較](tests/support/c_object_records.rs) では通常variantを含む11 sourceを実C caller／元allocatorで検証します。これらは代表的conformanceで、性能やdebuggerの証拠とは別です。

このfixtureで見つかったpanic時のallocator loan解析拒否を修正しました。Typed MIRが末尾Panic＋Unreachableだけを許可した経路では、必須failure retirementがpanic記録後にloan／ownerを解放します。通常return、bare Unreachable、成功経路へ継続する不正Panicの拒否は[MIR回帰test](crates/mir/tests/ownership_loan_activity.rs)で維持します。既存の受理済みpanic意味論の修正であり、source／ABI／serialized formatの変更はありません。

両tier-1 conformance確認（2026-10-20 JST）：[macOS run 39376049840](https://github.com/carimatics/lang/actions/runs/39376049840)、job `118474323072`／head `1d0c0dc` は全workflow成功（2026-10-20 05:48:01 UTC）。[Windows run 39373854894](https://github.com/carimatics/lang/actions/runs/39373854894)、job `118467465176`／head `3db077f` はfmt／C・linker検証／Clippy／全test／rustdoc成功（05:49:05 UTCまで）です。差分はmacOS起動条件と文書だけで、実装・test・fixture・Cargo／toolchainのGit blobは同一です。複合production entry、実AOT、実C／保存C object、panic loan回帰testが両targetで有効であり、Mac logの各成功行も照合しました。Q02／G16を完了します。Windowsのprototype benchmarkは実行中で、workflow全体の成功やRust／C性能比較の証拠とは数えません。

Platform conformance CI：[macOS workflow](.github/workflows/macos-aarch64.yml) は `codex/q02-*` のpushでも起動し、[Windows](.github/workflows/windows-x86_64.yml) は `codex/m2-*` で起動します。検証headの差分がworkflow／planning文書のみなら、結果を組み合わせる前に実装・test・fixture byteが同一であることを明示照合します。Q02の両native hostの結果は[Todo](docs/ja/milestone-2.md)に記録します。

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

