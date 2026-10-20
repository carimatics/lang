<!-- canonical-language: en; english-source: ../milestone-2-capabilities.md -->

# Milestone 2 capability guide

[English](../milestone-2-capabilities.md) · [実装Todo](milestone-2.md) · [元の完了条件](roadmap.md#milestone-2ownershipとnative-aot)

更新・検証：**2026-10-20 JST**。M2の元の18完了条件には実行／設計の証拠がそろっています。Z01でこの最終guideと監査を公開します。実装は引き続き実験的で、accepted／implementedのRFCもstabilizedな契約ではありません。履歴上の**113/113 gate weight**は受け入れ範囲の達成を示し、時間・生産性・正確な工数見積もりを意味しません。

## 完結したsource programを実行する

固定されたRust **1.99.0** toolchainを使います。[複合program](../../examples/conformance/owned_pipeline.lang)はowner移動、collection、shared／weak、arena、再帰、cleanupを組み合わせます。

```bash
cargo run -- check examples/conformance/owned_pipeline.lang
cargo run -- run examples/conformance/owned_pipeline.lang
cargo run -- compile --output target/m2-owned.lbc examples/conformance/owned_pipeline.lang
cargo run -- run --artifact target/m2-owned.lbc
cargo run -- repl
```

2つの実行はいずれも**42**を表示してexit 0になります。Program結果は表示値でありprocess終了codeではなく、compile／runtime失敗はexit 1、usage errorはexit 2です。Artifact出力先は新規pathにしてください。REPLでは定義を保持し、guest storageは評価ごとに解放します。Module、構文、診断、format、stdio LSPは[M1 guide](milestone-1.md)で説明しています。

## 利用できる動作

| 項目 | M2で実装した動作 |
| --- | --- |
| Ownership | Owner引数／戻り値とmove、射影read／replacement、HIR／MIRの独立move・loan・初期化・region検証、再帰guest call |
| 失敗とcleanup | 通常／return／error／panic時の決定的cleanup、戻り値保護、明示的な`core::drop`と符号付き`core::panic`、source span・guest stack・cleanup report |
| Core値 | Unit、有限Option／Result特殊化、失敗可能な割当を伴うBytes／Text、検査付きbyteアクセス、UTF-8変換、clone、厳密な等値比較 |
| Collection | Allocator対応Vec／Map、owner／入れ子payload、検査付きread／mutation、move／失敗cleanup。Map keyはi64か厳密なUTF-8 byte列のText |
| Shared／weak | 明示retain／release、read、replacement、weak upgrade／失効、入れ子containerを通るstrong cycle拒否とrollback |
| Lexical arena | Brand付き割当／read／replacement、同一region内cycle、escape／cross-region拒否、generation検査、決定的teardown |
| VM／REPL | Portable owned実行、上限付きservice work、現core／indirect値のresult preview。CLIはallocator権限を付与し、host importは登録しない |
| Native code | macOS AArch64／Windows x86-64のrelocatable AOT objectにaccepted Copy全profileとowned source bodyを出力。Production managed runtime、保存済み記録、source不要の起動 |
| C境界 | 明示unsafe import／export、有限Copy aggregate、生成C11 header、status／out-parameter転送、opaque context、実allocator／import grant、実C caller |
| 配布 | 一致する実験SDKの組立、上限付きpackage検証、公開`link-native`、隔離producer／consumer install、source不要のCopy／owned／C実行 |
| Debug | DWARF／CodeViewのsource位置、両native hostで実LLDB／CDBのbreakpoint・step・物理backtraceを実証 |

Accepted言語は有限の組込み特殊化を使います。一般のuser generic、trait、closure、並行実行、任意のC ABI layoutを追加するものではありません。通常loan・allocator権限・arena ownerは引き続き格納不可です。Source検査とdecodeしたartifact検証は、それぞれのtrust boundaryを守ります。

## Native executableを作る

macOSではnative linker／C compiler、Windowsでは互換C11 compilerを含むx86-64 MSVC developer environmentを使います。Python 3と記録に完全一致するRust toolchainが必要です。

```bash
python3 tools/build-native-sdk.py --output target/m2-sdk
cargo run -- native --output target/m2-program examples/conformance/owned_pipeline.lang
cargo run -- link-native --sdk target/m2-sdk --package target/m2-program --output target/m2-application
./target/m2-application
```

Windowsでは必要に応じて`python`を使い、`--output target/m2-application.exe`を選んで`.\target\m2-application.exe`を実行します。全出力先は新規pathにしてください。既定startupは42を表示します。SDK組立はCargoを使い、install後のlinkはCargoやrepository探索なしで同梱artifactと記録に完全一致するrustc release／commit／hostを使います。Link後のexecutableは元source・SDK／program directoryが不要ですが、system libraryは外部依存として残ります。

C libraryは生成mainではなく、明示的な信頼するRust hostと別途compileしたC objectを必要とします。[SDK手順とstartup template](native-sdk.md)、[C source例](../../examples/analysis/c_owned_exports.lang)、[実C統合test](../../tests/c_abi.rs)を参照してください。既定CLIのVM実行はeffect前にC unitを拒否します。Metadataのdecode／hash照合はC capabilityを付与せず、native codeの真正性も保証しません。

## Resourceとembeddingの契約

Owned実行はcallerの元allocator、cleanup Instance、明示controlを使います。通常native entryの既定値は**64 frames／65,536 cells**で、準備／storageとfuel／service workのpolicyは個別に設定できる上限です。失敗reportはusageと元source contextを保持します。正常な失敗後はretryでき、host panicは元Instanceをpoisonします。Result公開前にguest storageを解放します。

Native Copyはtrustedな同期・**非計量**ABIを使い、実行controlの設定を拒否します。Native callbackは途中で強制停止できません。Rust／C hostは全呼出しとcontext teardownが終わるまで元code image・library・callback・providerを保持し、scope内C clientをescapeさせてはいけません。これは信頼するnative embeddingです。[通常entry](../../crates/codegen-clif/src/owned/runtime.rs)、[scope付きSDK runtime](../../crates/codegen-clif/src/sdk/runtime.rs)、[RFC 0020](rfcs/0020-native-sdk-and-startup.md)を参照してください。

Portable Copyはschema 1、通常ownershipはschema 2、明示panic ownershipはschema 3、C対応ownershipはschema 4です。Native recordは元object byte／targetを保持し、独立検査した意味論と対応付けます。Build／target／toolchain不一致や未対応formatは一致するSDKでの再buildが必要で、build間のRust ABI安定性は約束しません。

## Debugと性能の証拠

Exact product `1496b8b`の[両host run 39413674593](https://github.com/carimatics/lang/actions/runs/39413674593)は実production SDKを使うdebugger実行に成功しました。2026-10-20のmacOS LLDB記録は**11:16:39 UTC**、Windows CDBは**11:20:07 UTC**です。

- Copy：2行目のsource breakpoint、3行目へのsource step、2／6／9行のcaller。
- Owned：8行目のbreakpoint、native PCの前進、実Rust runtime caller。
- 両profile：通常実行とdebugger実行で42を返し、正常終了。

[Probe](../../tools/check-native-debugger.py)はsourceを保持し、一致するSDK／rustcでdebug情報付きの手動linkを行います。既定`link-native`はこのdebug sidecarを公開しません。Owned continuationは物理native／runtime frameを示し、native debugger内の完全な論理guest stackではありません。Source変数位置、完全な論理continuation step、VM／native混在stack、DAPは後続toolingの範囲です。Runtimeのguest error stackを縮小するものではありません。

[性能captureと再現手順](benchmarks.md#milestone-2-sourcerustc-harness)は、各hostで3つの代表source programを同等の最適化Rust／C実装と比較します。macOS captureは`ecc0fd1`、Windowsは`dced632`です。これらの小さなworkloadでは検査付きsource実行は数百〜数千倍遅く、同等性能には達していません。Memoryはprocess全体、file sizeは文書記載の範囲で測定しています。これは先行revisionの不変の観測で、`1496b8b`や本guideの計測ではありません。Scaling・電源状態の制御・一般的な性能を示す証拠ではありません。

## 元の全exit gateの監査

[Board](milestone-2.md)は成果ごとのcommit証拠と見積もり履歴を保持します。この監査は元の全受け入れ境界を維持します。

| Gate | 必要な成果 | 実装・検証の証拠 |
| --- | --- | --- |
| G01 | Source ownership／core profileのaccept | [RFC 0007](rfcs/0007-source-ownership-and-core-values.md)、[RFC 0009](rfcs/0009-whole-sum-ownership-extraction.md) |
| G02 | Cleanup／allocator／unsafe／FFI契約のaccept | [RFC 0008](rfcs/0008-cleanup-allocators-and-c-boundaries.md)、[明示panic](rfcs/0014-explicit-source-panic.md) |
| G03 | Typed ownership・move・lexical loan | [MIR move](../../crates/mir/tests/ownership_moves.rs)、[loan解析](../../crates/mir/tests/ownership_loans.rs)、[source適合性](../../tests/source_conformance.rs) |
| G04 | Shared／weak保持・cycle preflight／publication・rollback／race | [Shared engine](../../crates/runtime/tests/shared.rs)、[型付きstorage](../../crates/runtime/tests/shared_storage.rs)、[managed graph](../../crates/runtime/tests/managed_shared.rs)、[native owner境界](../../tests/native_owned_boundary.rs) |
| G05 | Arena・escape／stale検査・teardown | [独立region解析](../../tests/mir_regions.rs)、[call summary](../../tests/mir_region_calls.rs)、[arena実行](../../tests/arena_calls.rs)、[managed arena](../../crates/runtime/tests/managed_arena.rs) |
| G06 | 通常／error／panic時の決定的drop | [Source cleanup](../../tests/ownership_cleanup.rs)、[failure CFG](../../tests/failure_flow.rs)、[実native entry](../../tests/support/native_entry_runtime.rs) |
| G07 | Text／Bytes・UTF-8・検査付きアクセス・allocator保持 | [Buffer](../../crates/runtime/tests/buffer.rs)、[cleanup](../../tests/buffer_cleanup.rs)、[owned実行](../../tests/owned_execution.rs)、[native境界](../../tests/native_owned_boundary.rs) |
| G08 | Unit／Option／Resultと必要な有限特殊化 | [Core MIR](../../crates/mir/tests/ownership_core.rs)、[source／VM適合性](../../tests/source_conformance.rs)、[C aggregate例](../../tests/c_abi.rs) |
| G09 | Allocator対応Vec・move／loan／growth／失敗適合性 | [Vector engine](../../crates/runtime/tests/vector.rs)、[owned source](../../examples/scripts/vec_owned.lang)、[複合production entry](../../tests/support/native_entry_runtime.rs) |
| G10 | Allocator対応Map・厳密key／replacement／失敗適合性 | [Map engine](../../crates/runtime/tests/map.rs)、[owned source](../../examples/scripts/map_owned.lang)、[複合program](../../examples/conformance/owned_pipeline.lang) |
| G11 | 狭いunsafe境界とFFI validity | [C境界検査](../../tests/c_abi.rs)、[context契約](rfcs/0008-cleanup-allocators-and-c-boundaries.md)、[scope付きruntime](../../crates/codegen-clif/src/sdk/runtime.rs) |
| G12 | C import／exportと生成header | [実C test](../../tests/c_abi.rs)、[実export](../../tests/support/c_exports_runtime.rs)、[保存済みobject](../../tests/support/c_object_records.rs) |
| G13 | Accepted Copy全profileのnative実行 | [Copy適合性](../../tests/source_conformance.rs)、[native program](../../tests/native_program.rs)、[link済compiler test](../../tests/native_compiler.rs) |
| G14 | Owned source objectと両host native link／run | [Native owner境界](../../tests/native_owned_boundary.rs)、[source不要AOT](../../tests/native_compiler.rs)、下記両host Q02 |
| G15 | Linker／startup／runtime packaging・production error | [Package検証](../../tests/native_packages.rs)、[link拒否／atomicity](../../tests/native_link.rs)、[SDK組立](../../tests/native_sdk.rs)、下記隔離install |
| G16 | 代表VM／native差分適合性suite | [複合fixture](../../examples/conformance/owned_pipeline.lang)、[production entry比較](../../tests/support/native_entry_runtime.rs)、[実C比較](../../tests/support/c_exports_runtime.rs)、両host Q02 |
| G17 | 同等Rust／C実装に対するnative workload計測 | [B02／B03 captureとB04監査](benchmarks.md#milestone-2-sourcerustc-harness)、上記の制約を維持 |
| G18 | 両hostのsource-span debug metadata出力・検証 | [Source全体PDB回帰](../../tests/native_debug_windows.rs)、[link済DWARF回帰](../../tests/native_compiler.rs)、[実LLDB／CDB run](https://github.com/carimatics/lang/actions/runs/39413674593) |

以下のcross-host記録はすべて2026-10-20 JST検証です。

| 証拠 | 正確な範囲 |
| --- | --- |
| [隔離install 39370391958](https://github.com/carimatics/lang/actions/runs/39370391958)、`c54af31` | 両hostで別producer／consumer job。Checkout／Cargo／repository入力なし、private入力削除後にCopy／owned／実C実行 |
| [Q02 macOS 39376049840](https://github.com/carimatics/lang/actions/runs/39376049840)、`1d0c0dc`、[Windows 39373854894](https://github.com/carimatics/lang/actions/runs/39373854894)、`3db077f` | 必須native host適合性検証。両headの実装／test／fixture／Cargo／toolchainのGit blobは一致 |
| [Windows全baseline 39398395224](https://github.com/carimatics/lang/actions/runs/39398395224)、`2297a07` | 後続unwind修正より前のfmt・C／linker・Clippy・全test・rustdoc。`1496b8b`のWindows全baselineを主張しない |
| [現productの限定native run 39413674593](https://github.com/carimatics/lang/actions/runs/39413674593)、`1496b8b` | 新規SDK、Windows metadata／unwind reader、macOS link後address回帰、両host実debugger実行 |
| Local `1496b8b` baseline | 1,593 tests／95 suites成功、失敗0、手動計測ignore 1件。fmt・警告error化Clippy・rustdoc・全11 fuzz targetのcompile／lint成功 |

これはtestと記録済runであり、coverage-guided fuzz campaign・形式的安全証明・安定配布・性能保証ではありません。最終文書の成果でcompiler／runtime動作は変わらず、repository整合性検証はboardへ記録します。

## 参照元と日時

Project判断：RFC 0007の最終更新は**2026-10-18**、RFC 0008は**2026-10-19**、RFC 0020は**2026-10-20**。他のlink先実装／test／boardは記載したrevisionで**2026-10-20 JST**に確認しました。Benchmark captureは各記録の日時とrevisionを保持します。本guideで新しい外部調査は行っていません。

Windows修正は[Microsoft x64 exception／unwind仕様](https://learn.microsoft.com/en-us/cpp/build/exception-handling-x64?view=msvc-170)に基づき、最終更新**2026-05-21**、検証**2026-10-20 JST**です。上でsourceの解釈、project判断、実行証拠を区別しています。Accept後の設計変更は別の改訂／変更taskで扱います。

