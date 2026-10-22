<!-- translation-of: README.md; canonical-language: en -->

# lang

[English](README.md)

`lang` は、新しいシステムプログラミング言語と、その完全な開発ツールチェーンを作るためのプレアルファ研究プロジェクトです。実装はRustで開始し、言語・コンパイラ・ブートストラップ手順が自己再生成できるほど安定した段階でセルフホスティングへ移行することを目指します。

現在は意味論の定義とアーキテクチャの検証を進めています。安定した構文、ABI、パッケージ形式、リリース予定はまだ存在しません。

現機能snapshot（2026-10-20 JST検証）：所有値・collection・shared／arena・C境界・VM／native実行・実験SDKと両hostのdebug検証を実装しています。[M2 capability guideと全出口監査](docs/ja/milestone-2-capabilities.md)に手順・証拠・制約をまとめています。

M3 task execution checkpoint（2026-10-21 JST 検証）: `cargo run -- run examples/scripts/tasks.lang` と `cargo run --example native-structured-tasks` は 42 を出力します。VM と実験段階の native API は owned Text/Bytes、record、whole Option/Result move、同期 owned call、同期 call/borrowed payload をまたぐ通常 loan も実行します。`cargo run -- run tests/fixtures/task_owned_buffers.lang` は 71、`cargo run -- run tests/fixtures/task_sync_loans.lang` は 12 を出力します。対応 profile を VM/JIT/別プロセスのリンク済み object fixture で検証します。`tests/fixtures/task_owned_pipeline.lang` の owned Vec/Map/shared/weak pipeline は suspension と取消をまたいで 42 を返し、compiler-known get/read loan と iterative cleanup に対応します。`cargo run -- run tests/fixtures/task_owned_arena.lang` も 42 を出力し、owned arena cycle を中断越しに保持し、checked read と descendant join 後に退役します。arena entry は invocation allocator 権限を捕捉して引数 temporary を終えますが、別 binding の allocator/read loan は suspension 越しを引き続き拒否します。複合 `tests/fixtures/task_owned_integrated.lang` は owned enum payload、再帰、collection、shared/weak、arena storage、入れ子 join を実行して 42 を返し、失敗版では panic −42 と順序付き cleanup 証拠を保持します。Task 実行は有限 service-work 上限を task/call/view 間で共有し、分離した return cell を解放まで計上します。`cargo run -- run --service-work 0 tests/fixtures/task_owned_buffers.lang` は数値付き拒否を報告して owner を退役します。Source/MIR は owned async signature の再帰 Send/Share を証明し、VM/native は引数/結果の managed move、owning await、未回収結果の逆公開順退役を実行します。`cargo run -- run tests/fixtures/task_owned_transfer.lang` と `cargo run -- run tests/fixtures/task_owned_transfer_all.lang` は 42 を返し、15 差分テストと 13 linked-object profile で所有権と有限 control を検証します。channel/source actor authority、foreign service、task artifact/native package、async REPL submission は M3 の残作業です。[実装 Todo](docs/ja/milestone-3.md) に checkpoint と残り scope を記録しています。

Channel 実行 checkpoint（2026-10-22 JST 検証）: `tests/fixtures/task_channel*.lang` 全7 program が VM CLI で42を返し、backpressure、Receiver 循環/再検査、Shared/Weak と Vec 内 Receiver、Shared Sender の publication/置換、混合取消、Text を実行します。source 26件で有限 protocol quota、loop 再利用、513段の iterative 退役、拡張6 fixture の全 fuel/work/trace/cell/provider/interruption prefix、cleanup panic/report 打切りと primary failure 保持を検証し、全 inventory は live allocation 0 へ退役します。message は独立 Send/storable proof を要し、source/MIR は payload 再利用を拒否します。native parity は下記に記録し、全 M3 gate は未完です。[実装 Todo](docs/ja/milestone-3.md) に検証証拠と残る作業を記録します。

Native channel checkpoint（2026-10-22 JST 検証）: `cargo run --example native-structured-tasks -- tests/fixtures/task_channels.lang` は42を出力し、残る channel 6 fixture も42を返します。VM/native の厳密差分8件で値・event・usage・failure/cleanup・resource cap・全 control/provider/interruption prefix を検証します。別リンク object 20 profile には channel 7 fixture と fuel/work prefix を含みます。明示的で検証済みの非公開 task API と trusted exact-build object binding を使い、serialized/C/package admission の拒否を維持します。full baseline は125 suite / 1,893件 / 失敗0 / 既存 timing ignore 1件で成功し、fmt・Clippy・rustdoc・全11 fuzz target の compile/lint も成功しました。fuzz campaign は主張しません。 証拠は[実装 Todo](docs/ja/milestone-3.md)に記録します。

A03 VM checkpoint（2026-10-22 JST 検証）: 明示 private service profile は、Receiver/ServiceOwner・入れ子 container・再帰 state を含む、独立に証明した Send/storable state と message を実行します。既存 Hub backing に実 state Reservation を1個保持します。publication は committed queue と実 state を読み、wake 後の send を再検査し、whole-state replacement では候補 state だけを置き換えます。循環 publication は payload を返し、拒否した置換は旧 state/generation を保って候補を一度退役します。shared/weak producer handle は実 authority を検証し、message 型引数は strong ownership に対して phantom のままです。handler 境界の取消・queue-before-state 退役・closed identity・typed Copy 回復も維持します。state CLI fixture 6個が42を返し、control/quota/provider/割込みと nested panic/打切りのテストは、64段の state cleanup を含め inventory 0 を保ちます。A04 の native 実行は下記に記録します。ordinary layout・REPL・wire・C/foreign・package は拒否を維持し、全 M3 gate は未完です。

A04 native 実装境界（2026-10-22 JST）: 明示 private task compiler は検査付き service type/layout を選択し、service capture/send/receive を既存 native primitive に lower します。state/read/replace/finish/cancel は共通 Machine service を使い、ServiceNew は VM と同じ境界で provider 副作用前に capacity と state cell を事前検査します。演算・分岐・continuation/失敗選択は生成 code のままです。JIT と別リンク object は private exact-build ABI を維持し、VM/MIR evaluator fallback・新 primitive ABI・永続 schema は追加しません。共通 conformance catalog は再帰 state・typed Copy 回復・取消を含む service 14 profile を扱い、全 control/provider/割込み/report と別リンクの signoff は実装 Todo に記録します。ordinary layout・REPL・wire・C/foreign・native package は拒否を維持します。両 host conformance・production embedding・全 M3 gate は引き続き必要です。 `cargo run --example native-structured-tasks -- tests/fixtures/task_service_state_replace.lang` は42を返します。

C06 型 checkpoint（2026-10-21 JST）: opaque `core::ChannelSetup<T>`、`core::Sender<T>`、`core::Receiver<T>` の identity を source/HIR と独立検証する MIR に保持します。既存 graph で有限な Send/Share 依存を別々に導出し、Sender の両 capability は message の Send、Receiver は Share 不可、Setup は両方不可、shared/weak は target の両 capability を要します。全 role は non-Copy で退役が必要であり、未使用 message constructor も Send を要します。重点11件で再帰/宣言順 graph、alias、不正 message/arity、move、偽造 MIR、proof/profile resource 境界を検証します。schema-2/3/4 writer、通常/旧 task layout、native package は副作用前の拒否を維持します。上記の明示的 VM/native task profile は実行を拡張しますが、wire/C layout や M3 gate は完了しません。

IDL v1 検査 checkpoint（2026-10-22 JST 検証）: `cargo run --example component-idl -- tests/fixtures/component_counter.idl.json` で完全な宣言文書を検査します。有界な検査器は source span を保持し、未使用宣言の不正、循環、borrow/effect の競合、不正 projection を拒否して宣言順序を正規化します。循環型の拒否例は両方の位置を伴う `IDL0004` を報告します。I02 の実装であり、codec・生成 binding・source/MIR component call・authority・production embedding は[実装 Todo](docs/ja/milestone-3.md)に残ります。

Production codec checkpoint（2026-10-22 JST）: `cargo test --test component_wire` で LCMP schema 3 contract/report・alias 解決済み LCSG signature digest・LCVL data-only frame を独立 vector と照合します。rich value は record/case/parameter 順序・厳密 tag・UTF-8 を保持し、単一 preparation session で bytes/work/elements/requested buffer を累積計上します。resource を含む schema は後続の実 call context を要し、data-only API は拒否します。schema-2 bytes は一致を維持します。source/MIR component proof・生成 binding・runtime grant/resource/callback は[実装 Todo](docs/ja/milestone-3.md)に残ります。

Opaque resource 型 checkpoint（2026-10-22 JST）: `cargo test --test component_resource_types` で source/独立 MIR の nominal `core::ComponentResource<Tag>` 所有権、move/drop、Send/Share 不許可、元 span、旧 profile の拒否を検証します。tag は通常 data として扱えます。検証済み component binding/call と実 resource 実行は[実装 Todo](docs/ja/milestone-3.md)に残ります。

Component binding checkpoint（2026-10-22 JST）: `cargo test --test component_bindings` で version 付き source-name/MIR-ID map と独立 IDL→MIR resource/type/effect 照合、未使用/偽造 metadata、厳密 quota を検証します。`lang::frontend::component::analyze` は contract・code・検証済み map を保持する受動的 unit を返します。実 provider/transfer/embedding call は[実装 Todo](docs/ja/milestone-3.md)に残ります。

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
- [Milestone 2 capability guide・全出口監査](docs/ja/milestone-2-capabilities.md)
- [Milestone 3 進行中の実装 Todo](docs/ja/milestone-3.md)
- [全ファイルreview、2026-10-17](docs/ja/review-2026-10-17.md)
- [リポジトリ全体レビュー、2026-10-20](docs/ja/review-2026-10-20.md)
- [RFCプロセスとテンプレート](docs/ja/rfcs/0000-template.md)
- [RFC 0001：プロジェクト憲章と互換性ポリシー（accepted）](docs/ja/rfcs/0001-project-charter.md)
- [RFC 0002：Prototype integer／structured control-flow semantics（accepted）](docs/ja/rfcs/0002-prototype-control-flow.md)
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
- [RFC 0022: Source の構造化タスク（accepted）](docs/ja/rfcs/0022-source-structured-tasks.md)
- [RFC 0023: Task MIR の境界（accepted）](docs/ja/rfcs/0023-task-mir-boundaries.md)
- [RFC 0024: 再帰 Send/Share と service 権限（accepted）](docs/ja/rfcs/0024-send-share-and-service-authority.md)
- [RFC 0025: 有限 channel と backpressure（accepted）](docs/ja/rfcs/0025-bounded-channels-and-backpressure.md)

- [RFC 0026: Source isolated service（accepted）](docs/ja/rfcs/0026-source-isolated-services.md)
- [RFC 0027: Production component IDL・wire・embedding profile（accepted）](docs/ja/rfcs/0027-production-component-interface.md)
- [コントリビューションガイド](CONTRIBUTING.ja.md)

## 開発

`rust-toolchain.toml` で固定した Rust **1.99.0** と rustfmt/Clippy を使います。[M2 capability guide](docs/ja/milestone-2-capabilities.md)に実装動作・18 完了条件・制約、[M3 実装 Todo](docs/ja/milestone-3.md)に現在の作業と見積り変更を記録します。`cargo run --example structured-tasks` で最初の明示的 host task driver を実行できます。source async は実装中です。

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

### Native package検証API

`lang::native`は所有snapshot内で上限付きmanifest、path、hash、target／build／toolchain対応、意味論／native recordを検証します。Toolの使用中はsnapshotを不変に保ちます。Hashは完全性／対応の検査で、native入力は信頼対象です。[Package test](tests/native_packages.rs)を参照してください。

### Scoped native SDK runtime API

Unsafeな`lang_codegen_clif::sdk::runtime::{execute_copy, execute_owned, with_c}`は元linked imageをbindし、起動時にsource compiler・JIT・guest interpreterを使いません。Ownedはallocator・Instance・control・失敗情報を保持し、Copyは同期・非計量でcontrolを拒否します。C context・callback・依存物は元bindingの生存期間内に保ちます。[Runtime](crates/codegen-clif/src/sdk/runtime.rs)と[RFC 0020](docs/ja/rfcs/0020-native-sdk-and-startup.md)を参照してください。

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

