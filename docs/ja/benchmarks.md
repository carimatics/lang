<!-- translation-of: ../benchmarks.md; canonical-language: en -->

# Benchmark evidence

[English](../benchmarks.md)

これらのmeasurementはpre-alpha prototypeの設計証拠です。言語のperformance guarantee、release target、最終backend選定ではありません。

## Milestone 2 source/Rust/C harness

更新・検証日: **2026-10-20 JST**。[`tools/m2-bench`](../../tools/m2-bench/)は実際のsource compiler、元のC-export object、installed release SDKのproduction runtimeを使用します。後述の過去のschema20 prototypeとは独立しています。private result formatは`lang-m2-benchmark` version 1です。

3 workloadで同じ上限付きの非負入力と整数結果を使います。

| Workload | 処理 | 独立した結果検証 |
| --- | --- | --- |
| `scalar` | 加算、値に依存する分岐、loop。中間値はすべてi64範囲内 | Python漸化式と固定の小入力 |
| `buffers` | Bytes/byte vectorとVec/i64 vectorを伸ばし、両方を読み加算 | 250 byteの値の折り返しを含む閉形式の和 |
| `regions` | shared payload、weak observer、retained owner、arena payloadを確保し、読み出し・退役・weak失効を確認 | `n * (n + 1)` |

計測前に`n = 0, 1, 7, 64, 250, 251, 1000`の**21組のsource/Rust/C/oracle一致**を必須とします。欠落・重複・誤った結果は拒否します。生きているsource context内でRust hostを意図的にunwindさせ、allocator退役後に次のcontextを正常に登録・実行することも検証します。これらは正しさの検証であり、性能の証拠ではありません。

```bash
python3 tools/build-native-sdk.py --release --output /tmp/m2-sdk
python3 -m unittest discover -s tools/m2-bench -p 'test_*.py'
python3 tools/m2-bench/run.py --sdk /tmp/m2-sdk --output /tmp/m2-parity --parity-only
python3 tools/m2-bench/run.py --sdk /tmp/m2-sdk --output /tmp/m2-measure --samples 15 --repeats 20 --input 1000
```

未使用のoutput pathを使います。Windowsではx64 MSVC developer環境が必要で、環境に応じて`python3`を`python`に読み替えます。SDKはnative tier-1 hostと固定rustc identityに一致させます。`M2 source Rust C benchmark` CIは`codex/bench-parity-*`でparityを検証し、`codex/bench-macos-*`と`codex/bench-windows-*`では一方のnative計測hostを選択します。これらのbranchは従来のprototype／isolated-install workflowを起動しません。

parity executableは3実装を含みます。**計測するsource・Rust・C executableは別々**で、reference側には言語objectもSDK/runtimeもlinkしません。3つともRustの計測／JSON driverを使います。`c`はCのworkload実装を表し、そのstartup／sizeにも共通driverを含みます。Rust host/referenceは`-Copt-level=3`、edition 2024、unwind、LTOなし、CはmacOSで`-O3`、MSVCで`/O2 /MD`です。source objectは現compilerのCranelift既定値（PIC、明示的な`opt_level`上書きなし）を使います。現実装と最適化済みreferenceの比較であり、optimizerの成熟度が同等とは主張しません。

`preparation_ns`はsource record検証／decode、artifact準備／binding、C context作成をprocessごとに一度記録します。`elapsed_ns / repeats`は再利用するartifact/contextへの呼び出しで、**呼び出しごとのruntime admission・準備・確保・退役を含み**、3回のwarmupは計時外です。各実装で同じC dispatch/statusとchecksum loopも含みます。入出力を観測可能にし、Cのallocation callbackにはvolatileな間接呼び出しを使い、Rustのbuffer／region ownerは`black_box`を通します。

専用の`startup`起動ではworkloadもwarmupも実行せず、初期化から正常終了までの新規process wall timeを記録します。OS/file cacheはwarm状態です。親側wall timeにはprocess起動と出力取得を含みます。workloadのprocess wall timeはmacOSで`/usr/bin/time`自体も含み、**起動latencyではありません**。memoryはworkload childごとのmacOS maximum RSS（`time -l`、bytes）またはWindows `PeakWorkingSetSize`（K32）で、変動するbaselineは差し引きません。sizeはlink済みexecutable全体と元のsource objectで、system dynamic libraryのsizeは加算しません。

sourceには要求live memory 256 MiB、cumulative memory 1 GiB、allocation 65,536件、8 frames、65,536 cells、fuel 100,000,000、cleanup records 4,096の明示的有限limitを設定し、host call/import grantは与えません。元のproviderで要求bytes・確保・解放を数え、context破棄後にzeroへ戻ることを必須とします。Cも同じ追跡付きheap callbackを使用します。Rustは標準`Vec`、`Rc`/`Weak`、lexicalな`Vec<Box<_>>` regionをRAIIで管理しweak失効を確認します。Rustのheap counterは**未計測**です（zero欄はallocationがないという意味ではありません）。Cは幾何級数で伸びるbuffer、明示的な単一thread参照count、lexicalなregion退役を使います。policy・layout・検証・参照count同期・allocator追跡のoverheadには差があり、同等なのはworkloadの振る舞いです。region testは並列利用や汎用arena実装の比較ではありません。

provider heap counterは当該guest/C providerを通る確保が対象で、Rust runtime／準備用metadataのすべてやprocess全体は数えません。process比較にはOSのpeak memory sampleを使います。

reportには各sample、min/median/max/mean/stdev、実装順の巡回、compiler command/version、SDK ID、source/harness hash、clean/dirty revision、host、入力、executable sizeを保存します。計測でも毎回checksumとallocator収支を検証します。local smoke runはharnessの検証であり、G17の完了には引き続き**両方**のtier-1 hostのnative計測と再現可能な解釈report（B02–B04）が必要です。

B01の正しさの証拠（検証日 **2026-10-20 JST**）：[CI run 39382631742](https://github.com/carimatics/lang/actions/runs/39382631742)で両native hostのcleanな`fffe86c`が成功しました。[macOS parity report](../../benchmarks/results/2026-10-20-m2-parity-macos-fffe86c.json)と[Windows parity report](../../benchmarks/results/2026-10-20-m2-parity-windows-fffe86c.json)はそれぞれ21組のoracle一致とunwind退役成功、対応release SDK ID、検証したharness hash、独立したreference link commandを保持します。canonical source hashの照合ではWindows checkoutの改行差を許容します。**このparity reportにtiming／startup観測はありません**。入力欄は予定した計測設定です。B02–B04は未完了です。

### macOS native capture (B02)

検証日 **2026-10-20 JST**。[CI run 39383945072 / job 118499215009](https://github.com/carimatics/lang/actions/runs/39383945072/job/118499215009)のcleanな`ecc0fd1`が成功しました。**2026-10-20 07:03:45 UTC**に記録した[全native report](../../benchmarks/results/2026-10-20-m2-macos-ecc0fd1.json)には**workload 135観測と専用startup 45観測**を保存しています。各case 15 samples、`n=1000`、sample当たり20 calls、計時外warmup 3回です。先に必須の21組parity／unwind gateが通っています。保存dataから全checksum・allocator退役・case/sample identity・集計値・SDK ID・harness hash・reference linkの分離を再検証しました。

Hostは **macOS 15.7.9、arm64、logical CPU 5、Python 3.14.7**です。processor欄は`arm`のみで、chip modelは未記録です。sourceはrelease SDK `06794c1ec70d06992a0e9f7c5714d05d32e8e1b5c029beac78062fbe8a32bad1`を使います。harnessとproduction／reference codeはB01と同一です。reportにflag・rustc/C compiler version・commandを保存し、測定境界は前述のとおりです。

| Workload | 実装 | 中央値 µs/call | Runtime RSD | Artifact/context準備 ms | RSS中央値 MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| `scalar` | source | 1484.383 | 4.68% | 1.554 | 27.328 |
| `scalar` | rust | 1.004 | 4.70% | — | 1.344 |
| `scalar` | c | 1.012 | 8.86% | — | 1.328 |
| `buffers` | source | 6661.158 | 1.76% | 1.521 | 27.469 |
| `buffers` | rust | 1.981 | 5.71% | — | 1.375 |
| `buffers` | c | 1.883 | 5.03% | — | 1.359 |
| `regions` | source | 18913.673 | 0.86% | 1.532 | 33.766 |
| `regions` | rust | 44.850 | 4.47% | — | 1.344 |
| `regions` | c | 30.492 | 4.67% | — | 1.328 |

Runtimeは反復call時間の中央値を20で割った値で、RSDはsample標準偏差／平均です。準備はsource processごとに一度の値で、callごとのruntime処理は含めません。referenceにはSDK/context準備がありません。RSSはworkload process全体で、baselineは差し引きません。

| 実装 | 新規process起動中央値 ms | RSD |
| --- | ---: | ---: |
| source | 4.085 | 19.02% |
| rust | 2.277 | 30.52% |
| c | 2.493 | 27.03% |

Startupはworkloadなし、新規process、warmなOS/file cache、初期化から正常終了までを測ります。RSDが19–31%あるため小さな差は不安定です。link済みexecutable全体のsizeはsource **2,283,096 B**, rust **493,976 B**, c **493,168 B**、元のsource objectは **27,912 B**です。

3 caseともsource pipeline時間は最適化済みreferenceより大きい結果です。宣言したruntime check、callごとの準備、ownership serviceを含みます。CPU scheduling／power stateとchip modelは記録しておらず、異なるmachine全体の性能は示しません。**G17完了には引き続きB03のWindows計測とB04の両hostの解釈が必要です。**

### Windows native capture (B03)

検証日 **2026-10-20 JST**。[CI run 39385041497 / job 118502756853](https://github.com/carimatics/lang/actions/runs/39385041497/job/118502756853)のcleanな`dced632`が成功しました（job完了2026-10-20 07:17:54 UTC）。**07:17:49 UTC**記録の[全native report](../../benchmarks/results/2026-10-20-m2-windows-dced632.json)は、改行をLFに正規化して全JSON dataを保持しています。21組parity／unwind gate成功後に、各case 15 samples、`n=1000`、20 calls、計時外warmup 3回で**workload 135観測と専用startup 45観測**を記録しました。全checksum・allocator退役・sample/case identity・集計値・SDK／harness identity・reference linkの分離を独立に検証しました。

Hostは **Windows Server 2025、build 10.0.26100、x86-64、logical CPU 2、Python 3.14.8**、processor identifierは`AMD64 Family 26 Model 2 Stepping 1, AuthenticAMD`です（chip brand／power stateは未記録）。release SDK `1a19e49debefcffaec8ab17b288c2a575c528c1f9a22c64d1493960e086f6602`とB01から変更していないharnessで、MSVC `/O2 /MD`、Rust `opt-level=3`を使います。compiler version／commandとcanonicalなLF／CRLF同等のharness hashを保持しています。このrunはWindowsだけを選びます。

| Workload | 実装 | 中央値 µs/call | Runtime RSD | Artifact/context準備 ms | Peak working set中央値 MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| `scalar` | source | 1900.895 | 11.53% | 2.603 | 12.953 |
| `scalar` | rust | 1.165 | 3.10% | — | 3.426 |
| `scalar` | c | 0.415 | 20.03% | — | 3.438 |
| `buffers` | source | 8872.875 | 6.51% | 2.610 | 13.145 |
| `buffers` | rust | 4.375 | 33.39% | — | 3.543 |
| `buffers` | c | 2.620 | 10.80% | — | 3.445 |
| `regions` | source | 20966.035 | 4.75% | 2.602 | 13.605 |
| `regions` | rust | 74.030 | 21.90% | — | 3.426 |
| `regions` | c | 60.055 | 20.32% | — | 3.441 |

各workload processで実際の非zeroな **K32 PeakWorkingSetSize** を取得し、親が保存したpeakも同じ読み値であることを確認しました。Windowsのworking set peakであり、macOSのRSSとは区別します。runtime／準備の測定境界と、Rust heap counterが未計測であるscopeは前述のとおりです。

| 実装 | 新規process起動中央値 ms | RSD |
| --- | ---: | ---: |
| source | 7.896 | 6.97% |
| rust | 4.566 | 18.88% |
| c | 4.929 | 23.17% |

専用startupではworkloadを実行せず、3実装とも共通のRust計測／JSON driverを含みます。startup RSDは約7–23%で、このrunのRust／C workload RSDは最大33.39%です。link済みexecutable全体のsizeはsource **1,849,344 B**, rust **154,624 B**, c **153,088 B**、元のsource objectは **29,687 B**です。

3 caseともsource pipeline時間は最適化済みreferenceより大きい結果です。checked C-export embeddingとruntime serviceの計測であり、optimizer／accounting policyは異なります。**G17完了前に、B04で両hostのコスト・ばらつき・memory定義・再現手順を解釈する必要があります。**

## 2026-10-17レビュー後の解釈

`representative_application` はsyntheticなhost orchestrationです。公開instanceの交渉済みbindingからdispatchせず、別の固定scalar／resource fixtureを実行します。実際のprovider実装の選択と呼び出し、生成adapterのend-to-end costは後続conformance／measurement課題です。

保存済みcaptureはすべてJIT executable-memory回収修正前です。従来はcompiled functionを破棄してもfinalized mappingがleakしていました。このためconstruction sampleと代表的applicationでは反復中にmappingが蓄積しえます。Timingは記録commitの過去evidenceであり、修正済み実装の計測ではありません。Harnessはconstruction timingからoutput破棄を除外しますが、sample間のmapping解放によってprocess状態が変わります。現在のcompile／application性能・memory budgetを主張する前にmacOS／Windowsのclean結果を再収録し、過去raw fileは改変しないでください。Separate-process memory rowは記載されたchild workloadを計測し、parentのJIT leakを計測しません。

情報源：`crates/codegen-clif/src/lib.rs` とsuccess／early-exit時解放の回帰test（2026-10-17更新）、lock済みCranelift JIT 0.136.3の `JITModule::free_memory`／`SystemMemoryProvider::drop`（installed upstream sourceを2026-10-17確認、公開／更新日記載なし）。受理済みinitial backend選定はRFC 0001へ記録しており、timing値だけから推論するものではありません。

## 現在のWindows x86-64 schema 20 baseline

現在の[Windows baseline](../../benchmarks/results/2026-10-17-windows-x86_64-7c37d5d.json)は、[GitHub Actions run 38932964398](https://github.com/carimatics/lang/actions/runs/38932964398)がclean commit `7c37d5d1a8d7ac7da5653e9d6931d4759a3a5fa7` から2026-10-17 01:45:50 JSTに記録しました。Windows Server 2025 runnerは、AMD64 Family 25 Model 1 CPU上のMicrosoft Windows 10.0.26100.33438を報告しました。Harness schema 20、固定されたRust 1.99.0 MSVC toolchain、31 sample、1,000 lowering／component operation、100,000 runtime operation、startup batch当たり10 launchを使用しました。Artifactは `git_dirty: false` を報告します。同じrunでformat、lint、全test suite、documentation、MSVC direct／response-file object-link-run test、typed component-resource cleanup conformance、benchmark validation、artifact uploadがすべてpassしました。

選択したcross-platform rowは次のとおりです。

| Measurement | Windows median | Windows p95 | Windows RSD | Windows／macOS median比 |
| --- | ---: | ---: | ---: | ---: |
| 6-object system link | 74.068 ms | 121.479 ms | 20.896% | 2.026× |
| 10-launch batched AOT startup | 7.444 ms | 8.986 ms | 6.832% | 4.972× |
| VM Fibonacci call | 3.526 us | 3.621 us | 3.565% | 2.845× |
| JIT Fibonacci call | 13.240 ns | 15.684 ns | 5.459% | 1.469× |
| 代表的component application | 236.804 us | 266.366 us | 6.201% | 3.503× |

Windowsのpeak-process-memory rowはすべて正で順序付けされ、31個の別processに基づきます。

| Process row | Median | p95 | RSD |
| --- | ---: | ---: | ---: |
| Allocation-free link済みAOT harness | 3,207,168 byte | 3,211,264 byte | 0.050% |
| Benchmark control | 6,242,304 byte | 6,246,400 byte | 0.124% |
| 代表的allocation workload | 26,189,824 byte | 26,198,016 byte | 0.032% |
| 4,096個のretained component instance | 14,241,792 byte | 14,245,888 byte | 0.025% |

代表的workloadはmacOSと同じ正確なlogical profile、すなわち23,894 allocation、19,183,210 total requested byte、18,102,042 peak live requested byte、終了後live byte 0を維持します。Windowsのcontrol-adjusted median resident deltaは19,947,520 byte、macOSでは21,053,440 byte（0.947×）で、対応するallocator-overhead／fragmentation合算推定値は1,845,478 byte／2,951,398 byteです。これらの比率はbackendだけの比較ではなく、system-levelの方向的evidenceです。Run間でCPU、OS、object format、linker、process loader、OS固有のpeak-working-set metricが異なります。また、linkのRSD 20.896%と代表的applicationのRSD 6.201%により、これらのmedianを安定したbudgetとして扱うことはできません。

## 現在のmacOS AArch64 schema 20 representative-application baseline

現在の[baseline](../../benchmarks/results/2026-10-16-macos-aarch64-de248a1.json)は、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 23:34:00 JSTにclean commit `de248a126e2e7b20e7be2df083b104a6c2e438df` から記録しました。Harness schema 20、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0、31 sample、component construction／lifecycle／application sampleごとに1,000 operation、その他のrowにはschema 19から維持したparameterを使用しました。Artifactは `git_dirty: false`、14個のcomponent-boundary row、schema 19から維持した全fieldを報告します。

Schema 20は既存component-boundary seriesへ1 rowを追加します。

| Series | Measurement | Sample当たりのoperation数 | Median | p95 | RSD |
| --- | --- | ---: | ---: | ---: | ---: |
| Representative application | 2-provider negotiation、lock生成／replay、instantiation、scalar／resource execution、failure cleanup | 1,000 | 67.592 us | 68.459 us | 1.109% |

`component_application_negotiate_lock_replay_instantiate_execute_cleanup` は、独立して選択される2つのprovider interface、deterministicなunlocked lock生成、exact-lock replay、fresh instance publication、scalar／nominal owned-resource VM／JIT call、再利用可能なguest-trap cleanupを1 operationへcomposeします。Check-in済みapplication testは2個のbinding／2個のexact lock entryを要求し、scalar、resource、cleanupの全pathがfixture value 42を保持することを要求します。

このrowはpackage selection、fresh instance construction、bytecode lowering、JIT compilation、adapter setup、cleanupを含み、isolated call-overhead budgetではなくend-to-endの方向的evidenceです。Fixtureはhost-orchestratedです。Validated bindingがcallをgateしますが、production generated dispatch stubはまだmaterializeしません。上記Windows baselineは、同じschema-20 rowと維持されたschema-19 contractすべてをnative MSVC runnerでvalidateします。

## 過去のmacOS AArch64 schema 19 component-boundary baseline

現在の[baseline](../../benchmarks/results/2026-10-16-macos-aarch64-76c8f81.json)は、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 22:51:15 JSTにclean commit `76c8f81516be5eb4cd77ab063b3ecd3935d16e58` から記録しました。Harness schema 19、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0、31 sample、component construction／lifecycle sampleごとに1,000 operation、その他のrowにはschema 18から維持したparameterを使用しました。

Schema 19は56個のgeneral timing rowを維持し、5個のcomponent-boundary rowを追加します。

| Series | Measurement | Sample当たりのoperation数 | Median | p95 | RSD |
| --- | --- | ---: | ---: | ---: | ---: |
| Instantiation | Lockなしschema-2 contractをnegotiateしてinstantiate | 1,000 | 1.226 us | 1.352 us | 3.802% |
| Instantiation | Complete exact lockfileをreplayしてinstanceを作成 | 1,000 | 1.358 us | 1.384 us | 0.934% |
| Host-value lifecycle | Owned host valueを1つpublish／transfer／release | 1,000 | 507.875 ns | 559.375 ns | 3.630% |
| Failure cleanup | Instantiate、transfer、guest trap cleanup、redacted report projection | 1,000 | 3.529 us | 3.619 us | 1.675% |
| Failure cleanup | Host valueをreleaseし、destructor failure／poisoningを記録してredacted reportへproject | 1,000 | 524.959 ns | 545.417 ns | 2.084% |

Unlocked／locked rowはいずれもrequired projectionをvalidateします。Lock replayはさらにcompleteなexact package／version／digest coverageをvalidateし、freshなadapter stateを作る前にcanonical lock artifactを構築します。Guest-trap rowはgeneral instantiationを2回実行するため、cleanup costだけとして読んではいけません。Destructor rowは構築済みadapter tableから始まり、publication、transfer、cleanup failure、poisoning、projectionをより狭く分離します。どちらもtyped resourceをMIR、bytecode、JIT、native ABIへloweringしません。

別child processはexact lock済みinstanceを4,096個保持します。Peak-resident medianは10,108,928 byte、変更のないcontrolは2,064,384 byteで、process全体の差は8,044,544 byteです。p95は10,174,464 byte、RSDは0.268%です。このenvelopeはloader state、allocator behavior、複製されたowned contract／binding、adapter container、measurement-process overheadを含みます。Instanceごとのlanguage memory guaranteeではなく、単純な除算による正規化も行いません。

Scalar `i64 -> i64` component functionのJIT bodyは40 byte、AOT objectは416 byteです。Encode済みschema-2 benchmark contractは271 byte、redacted failure reportは19 byteです。Machine-code sizeはhost registry、adapter／resource implementation、runtime support、final linkを除外します。Document sizeはserialization evidenceであり、executable code sizeではありません。後続のWindows schema-20 baselineはnative component executionを記録し、これらのmeasurement contractを維持します。

## 過去のschema 18 macOS AArch64 CFG／representative-allocation baseline

以前の[baseline](../../benchmarks/results/2026-10-16-macos-aarch64-b5ddb9b.json)は、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 19:17:44 JSTにclean commit `b5ddb9b6ed0e8339935c55df2a138233209f13d0` から記録しました。Harness schema 18、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0、31 sample、sampleごとに1,000回のlowering／validation／negotiation／adapter operationと100,000回のruntime／callback／scalar-dispatch／table-lifecycle operationを使用しました。

Schema 18はschema 17の全fieldを維持し、代表的CFG workloadとしてiterative Fibonacciを追加します。このloopはcountdownと2個のwrapping sequence valueを型付きblock argumentで運びます。維持したcross-mode rowは次のとおりです。

| Execution path | Sampleごとのoperation | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Direct Rust Fibonacci | 100,000 | 6.510 ns | 6.747 ns | 1.442% |
| VM Fibonacci | 100,000 | 1.231 us | 1.238 us | 0.349% |
| JIT Fibonacci | 100,000 | 7.463 ns | 8.593 ns | 4.483% |

JIT bodyは48 byte、AOT objectは400 byte、link済み6-workload harnessは17,168 byteです。Schemaはcompiler-likeなmixed-lifetime allocation workloadも維持します。すなわち、12 size classにまたがる16,384個のretained payload、3個ごとの解放、異sizeのreplacement、重なり合う2,048個のtransient bufferです。全payload byteをtouchし、正確なlogical profileは `(23,894 allocations, 19,183,210 total requested bytes, 18,102,042 peak live requested bytes, 0 live bytes afterward)` のままです。

同じbenchmark executableをinterleavedなcontrol／workload pairで起動するため、loaderとbinary structureは2つのdistribution間で共通です。

| Process measurement | Minimum | Median | p95 | Maximum | RSD |
| --- | ---: | ---: | ---: | ---: | ---: |
| Benchmark control | 2,048,000 B | 2,064,384 B | 2,064,384 B | 2,064,384 B | 0.140% |
| Representative allocation workload | 23,068,672 B | 23,085,056 B | 23,085,056 B | 23,085,056 B | 0.035% |

Median workload-minus-control resident deltaは21,020,672 byteです。Peak live requested payloadを引くとallocator-overhead／fragmentation合算推定値は2,918,630 byteです。計算は0でsaturateし、allocator metadata、size-class slack、retained page、page rounding、真のexternal fragmentationを分離できません。このtrace、allocator、OS、binaryに対するevidenceであり、language memory budgetではありません。

維持したvariable-size callback rowは次のとおりです。

| Execution path | Sampleごとのoperation | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Direct Rust variable-size allocation callback | 100,000 | 216.019 ns | 219.115 ns | 0.714% |
| VM variable-size allocation callback | 100,000 | 397.814 ns | 402.769 ns | 0.635% |
| JIT variable-size allocation callback | 100,000 | 218.037 ns | 221.265 ns | 0.590% |

正確なcallback profileは `(16 allocations, 1,088 total bytes, 1,088 peak live bytes, 0 live bytes afterward)` のままです。維持したsingle／burst／churn profileと合わせ、schema 18には56個のgeneral timing rowと4個のcallback requested-payload profileがあります。

このrunで維持したcomponent-boundary rowは次のとおりです。

| Series | Measurement | Sampleごとのoperation | Median | p95 | RSD |
| --- | --- | ---: | ---: | ---: | ---: |
| Metadata validation | 1 import／1 exportのschema-1 scalar contractをcloneしてvalidate | 1,000 | 277.916 ns | 304.083 ns | 4.407% |
| Interface negotiation | 3 providerをvalidateし、互換memberのうちhighest versionを選択 | 1,000 | 824.958 ns | 866.750 ns | 2.276% |
| Scalar dispatch | Direct wrapping callback | 100,000 | 0.930 ns | 0.932 ns | 0.521% |
| Scalar dispatch | Validated logical-import lookup、host-registry lookup、callback | 100,000 | 11.322 ns | 11.536 ns | 1.443% |
| Resource lifecycle | Generation tag付きownerのinsert／release | 100,000 | 14.209 ns | 14.781 ns | 1.795% |
| Resource lifecycle | Owner insert、shared borrow／read／end、release | 100,000 | 26.504 ns | 27.220 ns | 1.426% |
| Resource lifecycle | 2 adapter間で交互に行うcanonical owner preflight／commit／persist | 1,000 | 367.666 ns | 454.000 ns | 7.847% |
| Resource lifecycle | Canonical borrow preflight／commitとcall-scope cleanup | 1,000 | 341.542 ns | 435.834 ns | 11.015% |

これらのrowは意図的に異なる問いへ答えます。Metadata validationはconsumeされるmetadataのcloneとimmutable resolved bindingの構築を含みます。Negotiationはconsumerと3 providerをvalidateし、version／structural filteringを行い、選択bindingを構築します。Validated scalar rowはoperationごとにlogical-name lookupとhost-registry lookupを行います。Direct rowとの差であるmedian 10.392 nsは、この正確なin-memory pathのevidenceであり、stable dispatch budgetではありません。`ResourceTable` rowは再利用可能なgeneration tag付きstorageを実行し、canonical rowはpreflight allocation／reservation、trace／cleanup-ledger構築、単調増加するboundary-index vectorも含みます。VM／JIT／AOT resource lowering、host-value integration、failure／destructor path、component code size／memory、remote Windows behavior、代表的applicationは測定しません。

## 過去のschema 14 macOS AArch64 baseline

この過去の[baseline](../../benchmarks/results/2026-10-16-macos-aarch64-6a2caf8.json)は、macOS 27.0.1を実行するApple M1 Max上で、2026-10-16 14:02:20 JSTにclean commit `6a2caf827de5b3a28746708cc36178c86a514a45` から記録しました。Release buildで、harness schema 14、固定されたRust 1.99.0 toolchain、Apple Clang 21.0.0を使用しました。

Schema 14は47個すべてのtiming rowと3つの決定的なhost-callback requested-payload heap profileを記録します。Single callbackは `(1, 8, 8, 0)`、burstは `(64, 512, 512, 0)`、churn sequenceは `(96, 768, 512, 0)` です。Direct Rust／VM／JIT churn medianは1.192 us／1.348 us／1.173 usです。Link済みAOT process-memory distributionは31 sampleで、minimumが1,654,784 byte、median／p95／maximumが1,671,168 byteです。Heap profileはlogical payload lifetimeを測定し、process metricはallocator overheadではなくprocess全体のresident envelopeです。

```bash
./target/release/prototype-bench \
  --samples 31 \
  --lowering-iterations 1000 \
  --runtime-iterations 100000 \
  --startup-iterations 10
```

各rowはoperation当たりのnanosecondを示します。RSDは31 sample全体のrelative population standard deviationです。

| Measurement | Sample当たりのoperation数 | Median | p95 | RSD |
| --- | ---: | ---: | ---: | ---: |
| Bytecode lowering：wrapping `i64` add | 1,000 | 387.583 ns | 467.875 ns | 6.639% |
| Bytecode lowering：signed minimum | 1,000 | 1.067 us | 1.184 us | 4.369% |
| Bytecode lowering：countdown back-edge | 1,000 | 1.724 us | 1.980 us | 5.461% |
| Bytecode lowering：2-value accumulator | 1,000 | 1.786 us | 1.942 us | 3.314% |
| Bytecode lowering：conditional sum | 1,000 | 3.211 us | 3.473 us | 3.893% |
| Cranelift JIT compilation：wrapping `i64` add | 1 | 19.500 us | 32.584 us | 66.655% |
| Cranelift JIT compilation：signed minimum | 1 | 22.917 us | 30.000 us | 19.507% |
| Cranelift JIT compilation：countdown back-edge | 1 | 26.708 us | 35.334 us | 12.975% |
| Cranelift JIT compilation：2-value accumulator | 1 | 30.791 us | 36.000 us | 7.686% |
| Cranelift JIT compilation：conditional sum | 1 | 40.500 us | 47.916 us | 7.178% |
| Cranelift AOT object emission：wrapping `i64` add | 1 | 13.459 us | 16.833 us | 22.039% |
| Cranelift AOT object emission：signed minimum | 1 | 17.834 us | 21.042 us | 5.882% |
| Cranelift AOT object emission：countdown back-edge | 1 | 22.584 us | 51.541 us | 57.765% |
| Cranelift AOT object emission：2-value accumulator | 1 | 26.583 us | 35.459 us | 15.450% |
| Cranelift AOT object emission：conditional sum | 1 | 36.250 us | 43.375 us | 10.384% |
| macOS system link：事前compile済みC harnessとadd／minimum／countdown／accumulator／conditional-sum object | 1 | 39.705 ms | 42.020 ms | 3.349% |
| macOS AOT process startup：single launch | 1 | 1.923 ms | 3.496 ms | 232.018% |
| macOS AOT process startup：10-launch batch、launch当たり | 10 | 1.822 ms | 1.988 ms | 5.362% |
| Direct Rust wrapping `i64` add call | 100,000 | 0.930 ns | 1.019 ns | 4.162% |
| VM wrapping `i64` add call | 100,000 | 183.391 ns | 186.630 ns | 1.243% |
| JIT wrapping `i64` add call | 100,000 | 1.901 ns | 2.065 ns | 3.281% |
| Direct Rust signed minimum call | 100,000 | 0.930 ns | 0.993 ns | 2.147% |
| VM signed minimum call | 100,000 | 505.068 ns | 509.653 ns | 0.493% |
| JIT signed minimum call | 100,000 | 2.640 ns | 2.878 ns | 2.821% |
| Direct Rust countdown call | 100,000 | 0.960 ns | 1.064 ns | 4.144% |
| VM countdown call | 100,000 | 1.611 us | 1.622 us | 0.383% |
| unset interruption flag付きVM countdown call | 100,000 | 1.641 us | 1.947 us | 15.866% |
| 期限前deadline付きVM countdown call | 100,000 | 3.320 us | 3.369 us | 0.614% |
| JIT countdown call | 100,000 | 12.846 ns | 13.716 ns | 2.466% |
| Direct Rust 2-value accumulator call | 100,000 | 1.705 ns | 1.753 ns | 0.855% |
| VM 2-value accumulator call | 100,000 | 1.665 us | 1.678 us | 0.363% |
| JIT 2-value accumulator call | 100,000 | 12.790 ns | 13.716 ns | 2.246% |
| Direct Rust conditional-sum call | 100,000 | 5.605 ns | 5.738 ns | 0.997% |
| VM conditional-sum call | 100,000 | 3.046 us | 3.073 us | 1.002% |
| JIT conditional-sum call | 100,000 | 20.011 ns | 21.299 ns | 2.295% |
| Direct Rust host callback | 100,000 | 0.930 ns | 0.979 ns | 1.865% |
| VM host callback | 100,000 | 176.513 ns | 181.394 ns | 1.109% |
| JIT host callback | 100,000 | 2.797 ns | 3.333 ns | 5.898% |
| Direct Rust host-allocation callback | 100,000 | 11.931 ns | 13.015 ns | 3.752% |
| VM host-allocation callback | 100,000 | 188.068 ns | 191.508 ns | 1.018% |
| JIT host-allocation callback | 100,000 | 13.631 ns | 14.070 ns | 1.723% |
| Direct Rust 64-allocation-burst callback | 100,000 | 775.515 ns | 1.269 us | 20.514% |
| VM 64-allocation-burst callback | 100,000 | 959.024 ns | 968.330 ns | 0.544% |
| JIT 64-allocation-burst callback | 100,000 | 789.842 ns | 825.818 ns | 6.002% |
| Direct Rust allocation-churn callback | 100,000 | 1.192 us | 1.373 us | 4.901% |
| VM allocation-churn callback | 100,000 | 1.348 us | 1.373 us | 0.861% |
| JIT allocation-churn callback | 100,000 | 1.173 us | 1.184 us | 0.388% |

別のlink済みAOT process-memory distributionは、minimumが1,654,784 byte（1.578 MiB）、median／p95／maximumが1,671,168 byte（1.594 MiB）、meanが1,670,639.484 byte、RSDが0.173%です。16,384-byteの幅は、このhost上の小さなallocation-free harnessについてのevidenceであり、普遍的なmemory floorではありません。

Craneliftのcompiled code bufferにはaddが8 byte、minimumが20 byte、countdownが20 byte、accumulatorが36 byte、conditional sumが44 byte、host callbackが40 byte含まれました。生成されたadd、minimum、countdown、accumulator、conditional-sum objectは352、368、376、392、400 byteでした。Link済みAOT harnessは17,104 byteでした。JIT code sizeにはobject-file structure、relocation data、unwind/debug information、runtime support、linker outputは含まれません。Object sizeにはMach-O structureが含まれますが、C harnessと最終executableは含まれません。Executable sizeにはその両方とplatform linkerのdefaultが含まれますが、完全なruntime distributionは含まれません。

## Schema 19 formatとmigration

Schema 19はschema 18の全fieldを維持します。`component_boundary_measurements` は8行から13行へ増え、`instantiation`、`host_value_lifecycle`、`failure_cleanup` seriesを追加します。`process_memory_measurements` は `benchmark_process_component_instances_peak_resident_memory` を追加し、`component_instance_memory_profile.retained_instance_count` とexact row nameでその内容を記述します。`generated_code_bytes.jit_component_scalar`、`object_file_bytes.aot_component_scalar`、新しい `component_document_bytes` objectは、意味を区別してcomponent artifact sizeを報告します。Windows workflowは全新規row／series、正のartifact size、exact 4,096 retained instance、control medianを上回るcomponent-process medianを要求します。Consumerは `schema_version` で分岐する必要があり、schema 1〜18 JSON fileはimmutable evidenceのままです。

## Schema 18 formatとmigration

Schema 18はschema 17の全fieldを維持し、bytecode lowering、JIT compilation、AOT object emission、direct Rust／VM／JIT runtimeという6個のFibonacci rowを追加します。`jit_fibonacci` と `aot_fibonacci` のartifact sizeも追加します。Link済みworkload setが増えるため、system-link、single-process startup、batched startup、link済みprocess memory、executable-sizeの名前は `countdown` の後に `fibonacci` を挿入します。CleanなmacOS AArch64 resultは56個すべての一意なtiming row、4個の正確なcallback profile、8個のcomponent row、3個のprocess-memory rowを含みます。Consumerは `schema_version` で分岐する必要があり、schema 1〜17 JSON fileはimmutable evidenceのままです。

Workloadはsigned countdownと型付きblock argumentで運ぶ2個のsequence valueを使って、wrapping Fibonacciを反復計算します。既存MIR operationを使って3-value loop stateとback-edge argumentの入れ替えを実行し、source-language semanticsを追加もacceptもしません。VM／JIT差分testはwrapping overflowを含み、macOS direct／response-file integrationはAOT objectを実行し、host非依存Windows COFF validationはexportを検証し、cfg-gated MSVC pathとWindows workflowもcontractを維持します。

## Schema 17 formatとmigration

Schema 17はschema 16のtiming、artifact、callback-heap、component-boundary、link済みAOT process-memory fieldをすべて維持します。正確なrequested-payload count、paired control／workload process rowへの参照、median resident delta、`estimated_allocator_overhead_and_fragmentation_bytes_at_median` を持つ `representative_allocation_workload` を追加します。推定値は `max(0, workload median - control median - peak live requested payload)` です。`process_memory_measurement_scope` は `separate_process_peak_resident_memory` となり、process arrayは維持したlink済みAOT rowに加えて `benchmark_process_control_peak_resident_memory` と `benchmark_process_representative_allocation_peak_resident_memory` を含みます。

Workloadはsize classとlifetimeを意図的に混在させ、要求した全byteをtouchします。Bounded callbackよりcompiler allocation pressureを代表しますが、synthetic traceのままです。推定値はmetadata、internal／external fragmentation、retained page、page roundingを合算し、個別に帰属できません。最初のclean macOS AArch64 resultが上記representative-allocation baselineです。Consumerは `schema_version` で分岐する必要があり、schema 1〜16 JSON fileはimmutable evidenceのままです。

## Schema 16 formatとmigration

Schema 16はschema 15の全fieldを維持し、`rust_host_allocation_variable_call`、`vm_host_allocation_variable_call`、`jit_host_allocation_variable_call` を追加します。Generic heap pathは `(allocation_count, total_allocated_bytes, peak_live_bytes, live_bytes_after)` が `(16, 1088, 1088, 0)` の `host_allocation_variable_call` を追加します。Harness testとWindows workflowは3個すべてのtiming rowと正確なprofileを要求します。

最初のclean macOS AArch64 resultは過去のvariable-allocation baselineとして残します。Consumerは `schema_version` で分岐する必要があり、schema 1〜15 JSON fileはimmutable evidenceのままです。Schema 16はallocator overhead／fragmentationを分離せず、代表的application evidenceを提供せず、測定対象のprototype pathをstable ABIやperformance commitmentにしません。

## Schema 15 formatとmigration

Schema 15は `component_boundary_measurements` を追加します。これは `measurements` と同じdistribution fieldに必須の `series` valueを加えたrowのarrayです。最初のseriesは `metadata_validation`、`interface_negotiation`、`scalar_dispatch`、`resource_lifecycle` です。8個のexact row nameはharness testとWindows workflowがcheckします。既存のartifact、heap、process-memory、47個のgeneral timing rowはshapeを変更しません。

最初のclean macOS AArch64 resultは過去のcomponent-boundary baselineとして残します。Consumerは `schema_version` で分岐する必要があり、schema 1〜14 JSON fileはimmutable evidenceのままです。Schema 15は測定対象のprototype pathをstable ABIやperformance commitmentにしません。

## Schema 14 formatとmigration

Schema 14はschema 13のtiming、artifact、heap、process-memory fieldをすべて維持し、`rust_host_allocation_churn_call`、`vm_host_allocation_churn_call`、`jit_host_allocation_churn_call` を追加します。各operationは64個のboxed `i64` valueをallocateし、1つおきのallocationをconsumeし、解放済みslotへ32個のreplacementをallocateしてから、最後の64個をconsumeします。同じgeneric heap pathは `(allocation_count, total_allocated_bytes, peak_live_bytes, live_bytes_after)` が `(96, 768, 512, 0)` の `host_allocation_churn_call` を追加します。

これらのrowはfree／reallocate interleavingを伴う固定sizeのsynthetic host-callback sequenceを計測します。Allocator metadata、fragmentation、reuse policyを分離せず、代表的applicationのevidenceではありません。VM／JIT差分test、macOS direct／response-file AOT integration、host非依存Windows COFF validation、cfg-gated MSVC pathがcallback／import contractを扱います。Cleanな31-sample macOS AArch64 runは47個の一意なtiming row、3つすべての正確なheap profile、direct Rust／VM／JIT churn medianの1.192 us／1.348 us／1.173 us、1,654,784〜1,671,168 byteのlink済みprocess peak memoryを記録します。現在のWindows CIは正確なchurn row／profileを維持しつつschema 20を要求します。Consumerは `schema_version` で分岐する必要があり、schema 1〜13 JSON fileはimmutableなevidenceとして残します。

## Schema 13 formatとmigration

Schema 13はschema 12のtiming、artifact、heap fieldをすべて維持し、`process_memory_measurement_scope`、`process_memory_metric`、`process_memory_measurements` を追加します。Scopeは `linked_aot_executable_process` です。唯一のrowである `aot_process_peak_resident_memory_add_min_countdown_sum_to_sum_below` は、link済みAOT executableをsampleごとに1回起動した後、`bytes/process`、`sample_count`、minimum、median、p95、maximum、mean、relative standard deviationを記録します。

MetricはmacOSで `macos_rusage_ru_maxrss_bytes`、Windows MSVCで `windows_process_peak_working_set_bytes` です。前者は `/usr/bin/time -l` からparseし、後者は各計測対象processが終了前に `K32GetProcessMemoryInfo` で `PROCESS_MEMORY_COUNTERS.PeakWorkingSetSize` をself-reportします。両方ともprivate／shared resident pageとloader／runtime stateを含むplatform固有のprocess envelopeです。Consumerは `schema_version` で分岐する必要があり、schema 1〜12 JSON fileはimmutableなevidenceとして残します。過去のclean schema 13 macOS AArch64 resultは31 sampleすべてが1,671,168 byte、RSD 0.000%であることを記録します。現在のschema 20 Windows CIはこの維持rowに加えて正で順序付けされたcontrol／workload／component distributionを要求しますが、remote resultはまだ検査・commitしていません。

## Schema 12 formatとmigration

Schema 12はschema 11のtiming row／artifact fieldをすべて維持しながら、`heap_measurement_scope` と `heap_measurements` を追加します。Scopeは `host_callback_requested_payload_bytes` です。各profileは `allocation_count`、`total_allocated_bytes`、`peak_live_bytes`、`live_bytes_after` を記録します。Harnessはtimed loopの外側で、安全なmeasuring heapを通じて同じgeneric single／burst callback workloadを実行します。

正確なsingle profileは `(1, 8, 8, 0)`、burst profileは `(64, 512, 512, 0)` です。これらの値は要求された `i64` payloadとlogical cleanupだけを扱います。Allocator metadata／fragmentation、stack、generated code、VM／JIT内部allocation、process RSSは除外します。Consumerは `schema_version` で分岐する必要があり、schema 1〜11 JSON fileはimmutableなevidenceとして残します。過去のschema 12 resultは両profileと44個すべての一意なtiming rowを記録しました。Schema 13／14とWindows CIは同じheap contractを維持しますが、remote resultはまだ検査・commitしていません。

## Schema 11 formatとmigration

Schema 11は `rust_host_allocation_burst_call`、`vm_host_allocation_burst_call`、`jit_host_allocation_burst_call` を追加します。各operationは64個の異なる `Box<i64>` valueを作成し、512 payload byteすべてを固定長arrayで同時に保持し、wrapping sumを計算してからdropします。`black_box` は各boxとload済みvalueをoptimizationからopaqueにします。VM／JIT rowは同じregister済みcallbackを囲む型付きhost-call dispatchを含み、direct Rust rowがhost側baselineを提供します。Harnessはwarm-up／計測前に3つすべてのresultを検証します。

既存AOT link／startup seriesは変更せず、macOSと構成済みWindowsのintegration testが、64個の `malloc` allocationを同時に保持してから読み取り・解放する対応C callbackを実行します。これらのrowはallocator metadata、stack上のpointer array、process peak RSS、failure behavior、language heap、代表的applicationを計測しません。Schema 11はschema 10の既存row／artifact fieldをすべて維持します。Consumerは `schema_version` で分岐する必要があり、schema 1〜10 JSON fileはimmutableなevidenceとして残します。過去のclean schema 11 resultは44行すべてが一意であることを記録し、direct Rust／VM／JIT burst medianは782.624 ns／962.329 ns／786.270 nsです。RSDは0.331%／11.822%／0.378%で、VMのmaximum 1.620 usがp95 967.234 nsにもかかわらずRSDを押し上げているため、それらの値はperformance budgetではなく方向的evidenceのままです。Windows Server 2025 workflowは現在schema 20を要求し、burst 3 rowすべてを維持します。

## Schema 10 formatとmigration

Schema 10は `vm_countdown_deadline_call` を追加します。`vm_countdown_call` と同じalternating positive／immediate-return inputと検証済みcountdown bytecodeを実行しますが、`ExecutionControl` を介してharness準備の1時間後に設定した絶対monotonic deadlineを渡します。Harnessはwarm-up／計測前にresultの等価性を検証し、deadlineはcallごとに再計算せず全sampleで共有します。

このrowは、deadlineが期限前のままdeadline checkを有効化するsteady-state costを計測します。Expiration latency、scheduler accuracy、callback overrun、callback preemption、native／JIT enforcementは計測しません。Schema 10は既存row／artifact fieldをすべて変更しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜9 JSON fileはimmutableなevidenceとして残します。過去のclean schema 14 resultでは、通常のVM countdown medianが1.611 us、期限前deadline付きが3.320 usで、1.709 us、106.071%増加しました。0.383%／0.614%のRSDを持つこのpaired medianは方向性のevidenceであり、overhead budgetではありません。現在のschema-20 Windows workflowがこのrowを維持します。

## Schema 9 formatとmigration

Schema 9は `vm_countdown_interruptible_call` を追加します。`vm_countdown_call` と同じalternating positive／immediate-return inputと同じ検証済みcountdown bytecodeを実行しますが、`ExecutionControl` を介して共有されたunset interruption flagを渡します。現在のschema 13 artifactでは両rowがVMのoptional-control branchを含み、新rowは実行した各instruction／terminatorでflagのacquire loadも行います。Harnessはwarm-up／計測前にresultの等価性を検証します。

このrowは、requestが発生しないときにcooperative interruptionを有効化するsteady-state costを計測します。Interruption latency、cross-thread wake-up、set済みflag、callback preemption、native／JIT enforcementは計測しません。Schema 9はlowering、compilation、artifact-size、host-callback、link、startup fieldをすべて変更しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜8 JSON fileはimmutableなevidenceとして残します。過去のclean schema 14 resultでは、通常のVM countdown medianが1.611 us、unset interruption付きが1.641 usで、29.868 ns、1.854%増加しました。これはinterruption rowの15.866% RSDより小さいため、この1つのpaired microbenchmarkはinterruption-check regressionを測定できず、overhead budgetも確立しません。現在のschema-20 Windows workflowがこのrowを維持します。

## Schema 8 formatとmigration

Schema 8は `rust_host_allocation_call`、`vm_host_allocation_call`、`jit_host_allocation_call` を追加します。各operationは1回のRust `Box` allocate／read／drop round tripを通して `i64` を渡します。`black_box` によりallocated boxと戻り値をoptimizationからopaqueにします。VM／JIT rowは同じregister済みcallbackを囲む既存の型付きhost-call dispatchを含み、direct Rust rowがhost側baselineを提供します。Harnessはwarm-up／計測前に3つすべての結果を検証します。

既存のAOT system-link、executable-size、process-startup seriesは変更しません。Launchされたharness内の1回のallocationはprocess creation／loader処理に埋もれるためです。別のplatform integration testが、link済みAOT objectを通して対応するC `malloc`／read／`free` callbackを実行します。したがって新rowが計測するのは、狭いhost-owned allocationとVM／JIT boundary costです。Language heap、allocator policy、peak memory、allocation failure、collection、代表的なapplication behaviorは計測しません。Consumerは `schema_version` で分岐する必要があり、schema 1〜7 JSON fileはimmutableなevidenceとして残します。CleanなmacOS AArch64 schema 8 resultを上に記録しました。

Windows Server 2025 workflowは当初、変更のないplatform measurementに加えてschema 8と3つすべてのsingle-allocation rowを要求しました。Remote schema 8 resultはまだ検査もcommitもしておらず、workflowは現在、上記のschema 20を要求します。

## Schema 7 formatとmigration

Schema 7は `sum_below` conditional-sum workloadをbytecode lowering、JIT compilation、AOT object emission、direct／VM／JIT call、generated-code size、object sizeへ追加します。Runtime sampleは、skipとaddの両pathを実行して `528` を返す `(64, 33)` と、immediate-return pathで `0` を返す `(-7, 33)` を交互に使用します。Harnessは計測前に `(-7, 33)`、`(0, 33)`、`(1, 1)`、`(1, 2)`、`(64, 33)` についてdirect／VM／JITの等価性を確認します。

Tier-1 link／process-startup setにも5番目のconditional-sum objectを追加します。そのためschema 7は3つのplatform rowを `system_link_add_min_countdown_sum_to_sum_below`、`aot_process_startup_add_min_countdown_sum_to_sum_below`、`aot_process_startup_batch_add_min_countdown_sum_to_sum_below` へrenameし、executable-size fieldを `aot_add_min_countdown_sum_to_sum_below` へrenameします。また、`jit_sum_below`／`aot_sum_below` size fieldと6つのconditional-sum measurement rowを追加します。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜6 JSON fileはimmutableなevidenceとして残します。

CleanなmacOS AArch64 schema 7 resultを上に記録しました。Windows evidence pathは存在しますが、remote schema 7 resultはまだ検査もcommitもしていません。

## Schema 6 formatとmigration

Schema 6は2-value `sum_to` accumulator workloadをbytecode lowering、JIT compilation、AOT object emission、direct／VM／JIT call、generated-code size、object sizeへ追加します。Runtime sampleはinput `64` と `-7` を再利用し、64-iteration loop後に `2080`、immediate-return pathで `0` を返します。Harnessは計測前に `-7`、`0`、`1`、`64` についてdirect／VM／JITの等価性を確認します。

Tier-1 link／process-startup setにもaccumulator objectを追加します。そのためschema 6は3つのplatform rowを `system_link_add_min_countdown_sum_to`、`aot_process_startup_add_min_countdown_sum_to`、`aot_process_startup_batch_add_min_countdown_sum_to` へrenameし、executable-size fieldを `aot_add_min_countdown_sum_to` へrenameします。また、`jit_sum_to`／`aot_sum_to` size fieldと6つのaccumulator measurement rowを追加します。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜5 JSON fileはimmutableなevidenceとして残します。

CleanなmacOS AArch64 schema 6 resultを上に記録しました。Windows evidence pathは存在しますが、remote schema 6 resultはまだ検査もcommitもしていません。

## Schema 5 formatとmigration

Schema 5はtop-levelの `startup_iterations` fieldと `aot_process_startup_batch_add_min_countdown` measurementを追加します。既存のsingle-launch rowはsample当たり1 operationのままです。Batched rowは各sample内で設定数のprocess launchを順番に実行し、経過時間をその回数で割ります。Defaultは10です。これは1回のscheduling delayの影響を受けにくいamortized launch-cost estimateを提供し、single-launch rowはtail-sensitiveなlatency observationとして残します。

2つのrowは異なる問いに答えるため、mergeしてはいけません。Batchingはloader timeを分離せず、host noiseを除去せず、individual-latency percentileを置き換えません。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜4 JSON fileはimmutableなevidenceとして残します。

CleanなmacOS AArch64 schema 5 resultを上に記録しました。Windows evidence pathは存在しますが、remote schema 5 resultはまだ検査もcommitもしていません。

## Schema 4 formatとmigration

Schema 4は `executable_file_bytes.aot_add_min_countdown` と `aot_process_startup_add_min_countdown` measurementを追加します。Executable-size fieldは初期tier-1 hostでは正のbyte count、それ以外では `null` です。Consumerは `schema_version` で分岐する必要があり、既存のschema 1〜3 JSON fileはimmutableなevidenceとして残します。

Startup rowは、反復するprocess creation、dynamic loading、add／minimum／countdown checkの実行、output collection、process terminationを計測します。Harnessは計測前にlinkとexecutionを1回検証し、operating-system cacheをflushしません。そのため、これはcold-boot latencyではなくwarm-cache process-startup measurementです。Executableには事前compile済みC harnessとplatform linkerのdefaultが含まれますが、完全なruntime distributionやinstallation-size measurementではありません。

CleanなmacOS AArch64 schema 4 resultを上に記録しました。Windows evidence pathは存在しますが、remote schema 4 resultはまだ検査もcommitもしていません。

## Schema 3 formatとmigration

Schema 3は、bounded countdown workloadをbytecode lowering、JIT compilation、AOT object emission、direct／VM／JIT call、artifact size、system-link setへ追加します。Runtime sampleは、`i64` block parameterをback-edgeの周囲で運んでzeroへ到達するinput `64` と、immediate-return pathを取る `-7` を交互に使用します。Harnessは計測前に、`-7`、`0`、`1`、`64` についてdirect／VM／JITの等価性を確認します。

Schema 3は `jit_countdown` と `aot_countdown` size field、6つのcountdown measurement rowを追加します。また、`system_link_add_min` を `system_link_add_min_countdown` へrenameします。Consumerはこれらのnameをmergeせず、`schema_version` で分岐する必要があります。既存のschema 1／2 JSON fileはimmutableなhistorical evidenceとして残します。

## Windows x86-64 evidence path

Schema 7 harnessは、Windows x86-64 MSVCでconditional-sum lowering／compilation／runtime row、rename済みsystem-link／process-startup row、`executable_file_bytes.aot_add_min_countdown_sum_to_sum_below` を記録します。C harnessを `cl.exe` でcompileし、tier-1の `link.exe` を `LinkTarget` 経由で発見し、計測前にlinkとexecutionを1回検証します。Schema 8はそれらのplatform fieldと31／1,000／100,000／10 parameterを維持し、3つのsingle-allocation runtime rowを追加します。Schema 9は `vm_countdown_interruptible_call`、schema 10は `vm_countdown_deadline_call`、schema 11は3つのallocation-burst row、schema 12は正確な2つのheap profile、schema 13はlink済みprocess peak-memory distribution、schema 14はallocation-churn evidence、schema 15はcomponent-boundary series、schema 16はvariable-size allocation timing／profile evidence、schema 17はrepresentative allocation／process-memory evidence、schema 18はFibonacci CFG timing／artifact evidence、schema 19はcomponent lifecycle／artifact／process-memory evidence、schema 20はrepresentative-application evidenceを追加し、現在Windows Server 2025 workflowはschema 20を要求します。Workflowはclean-checkout metadata、`windows`／`x86_64` identity、interruption、deadline、conditional-sum、Fibonacci、single-allocation、allocation-burst、allocation-churn、variable-size allocation、正確なrepresentative-workload accounting、4個すべてのprocess-memory row、14個すべてのcomponent-boundary row、startup batch size、正のFibonacci／component／executable sizeを検証してから、そのJSONを30日間uploadします。

上記のcommit済みschema-20 Windows baselineが、このpathの記録済みoutputです。GitHub Actions run 38932964398は11分55秒で全workflow stepを完了し、artifactをuploadしました。そのartifactはこのrepositoryに保持しています。Platform metricは交換可能ではないため、Windows resultはmacOS baselineを置き換えず補完します。

## 生成コード検査

Addとhost-callback objectをclean commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad` から再生成し、2026-10-16 00:25 JSTにmacOS AArch64 hostで検査しました。Signed-minimum objectはclean commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483` から00:43 JSTに最初に再生成し、その後、明示的CFGへの移行後にclean commit `f6eb016798c237412b59057fa32257ca2bc5d016` から01:08 JSTに再生成しました。

| Prototype | Export | Import | 生成body | Relocation |
| --- | --- | --- | --- | --- |
| Wrapping `i64` add | `_prototype_add` | なし | `add x0, x0, x1`; `ret` | なし |
| Signed `i64` minimum | `_prototype_min` | なし | `cmp`、conditional `b.lt`、else側の `mov`、2つの `ret` path | なし |
| 型付きhost callback | `_prototype_host_callback` | `_lang_host_i64_0` | Pointer authentication付きframe、GOT load、間接 `blr`、authenticated return | `_lang_host_i64_0` に対する `GOTLDP` と `GOTLDPOFF` |

3つすべてのfileは64-bit ARM64 Mach-O objectです。Add prototypeはrelocationなしの8-byte、2-instruction bodyへloweringされています。Minimumはrelocationなしの20-byte compareと実際のconditional branchへloweringされています。Less-than pathは最初のargumentを `x0` に保持し、もう一方のpathは `x1` をreturn registerへmoveします。明示的CFGへの移行後の再検査でも、同じcode／object size、symbol、instruction sequence、空のrelocation tableが得られました。Callbackはlinkerが解決するexternal importのままであり、position-independentなGOT addressingを使用します。そのbodyは32 bytesです。これは意図したarithmetic、control-flow、export／import lowering boundaryを確認します。Loop lowering、block parameter、loop-carried value、allocation、unwind behavior、他targetは評価していません。

完全なcommandと、取得した `file`、`nm`、`otool` の出力は[add／callback transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、[CFG移行前のminimum transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md)、[明示的CFGのminimum transcript](../../benchmarks/inspection/2026-10-16-macos-aarch64-f6eb016-cfg-min.md)に保存しています。

実装前のcleanな[debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)は、元のnegative baselineを記録しています。Commit済みの[source-mapped DWARF inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md)は、commit `31e182ac27e324048597a5ebd0fda4d2d4934395` から代表的なadd、minimum、host-callback objectを再生成します。3 objectすべてがrelocation付きDWARF v4 compile-unit、subprogram、source-line dataを持った状態で `dwarfdump --verify` をpassし、direct／response-file executableがlink／実行され、生成した両dSYMもfixture pathと3 functionを保持したままverificationをpassします。Commit済みの[Windows x86-64 CodeView／PDB inspection](../../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md)は、commit `465b0d66ba6ea61a0d4b26201e6ab043f2f15f9d` から同じ代表COFF objectをemitし、C13 record／relocationをvalidateし、LLDでPE／PDB artifactへlinkし、LLDBでlink済みsource mappingを検証します。これらは計画済みtier-1 source-mapping criterionを満たしますが、native Windows execution、source-language local、unwind metadata、JIT debugger registration、optimized steppingを完了したとは主張しません。

## 解釈と制限

- Harnessは計測前に、minimumの両方の結果、countdown／accumulatorのimmediate-return／loop path、conditional sumのimmediate-return／add／skip pathを含め、VM、JIT、direct Rustのresultが等価であることを確認します。
- 2-value accumulatorはdescending counterとwrapping sumを同じback-edgeで運びます。Direct、VM、JIT medianは1.705 ns、1.665 us、12.790 nsです。このbounded MIR workloadは有用なdifferential evidenceですが、代表的なprogram benchmarkではありません。
- Conditional-sum prototypeは両方のbranch結果とimmediate-return pathを扱います。Direct、VM、JIT medianは5.605 ns、3.046 us、20.011 nsです。これはより広いCFG evidenceですが、代表的なprogram benchmarkではありません。
- Schema 18 Fibonacci workloadは30回のloop iterationと非正inputのimmediate returnを交互に実行しながら、3個のloop-carried valueを入れ替えます。Direct／VM／JIT medianは6.510 ns／1.231 us／7.463 nsです。認識しやすいstateful loopをcross-mode setへ追加しますが、代表的application performanceではなくbounded microbenchmarkのままです。
- Single-allocation host-callback medianはdirectが11.931 ns、VMが188.068 ns、JITが13.631 nsです。VM minus directは176.137 ns（1,476.297%）、JIT minus directは1.700 ns（14.249%）です。3 rowのRSDは3.752%／1.018%／1.723%で、このrunについてstableで限定的なboundary-cost evidenceを提供しますが、1回の `Box<i64>` round tripは代表的なallocator／memory workloadではありません。
- 64-allocation-burst medianはdirectが775.515 ns、VMが959.024 ns、JITが789.842 nsです。VM minus directは183.509 ns（23.663%）、JIT minus directは14.327 ns（1.847%）です。Direct／VM／JIT rowのRSDは20.514%／0.544%／6.002%であるため、resultは一般的なboundary-overhead budgetではなく方向的evidenceです。
- 96-allocation churn medianはdirectが1.192 us、VMが1.348 us、JITが1.173 usです。VM minus directは156.544 ns（13.137%）、JIT minus directは-18.203 ns（-1.528%）です。各rowのRSDは4.901%／0.861%／0.388%です。負のJIT deltaはnoise-sensitiveなmicrobenchmark evidenceであり、boundaryがallocation costを除去するという主張ではありません。
- Schema 12はtimed loopの外側で同じgeneric callback workloadをprofileします。Single callbackは1 allocation、total／peak live payload 8 byte、終了後live byte 0を記録し、burstは64 allocation、total／peak live payload 512 byte、終了後0を記録します。Scopeは意図的に `host_callback_requested_payload_bytes` としており、logical payload lifetime／cleanupを確認しますが、allocator metadata／fragmentation、stack、generated code、VM／JIT内部allocation、process RSSは除外します。
- Schema 13はallocation-freeのlink済みAOT harnessに対する別processのpeak-memory samplingを導入しました。過去のschema 14 macOS runは `ru_maxrss` を通じてminimum 1,654,784 byte、median／p95／maximum 1,671,168 byte、RSD 0.173%を報告します。現在のmacOS schema-20 runはFibonacciをlink setに維持してmedian 1,671,168 byteを報告し、Windows schema-20 runは `PeakWorkingSetSize` でmedian 3,207,168 byteを報告します。個別rowは代表的allocation推定値とcomponent-instance evidenceを維持します。AOT rowはallocation-free process envelopeのままであり、2つのOS metricは独立して解釈する必要があります。
- VM rowには、呼び出しごとのbytecode verificationとregister allocationが含まれます。Interpreter dispatchだけでなく、現在のend-to-end embedding APIを計測しています。
- JIT rowにはchecked Rust invocation wrapperが含まれます。Direct Rust rowは限定的なbaselineであり、代表的なprogramの予測ではありません。
- Object-emission rowにはhost ISA構築、MIR lowering、compilation、object serializationが含まれます。Schema 2のsystem-link rowはharness、add、minimum objectを扱い、schema 3はcountdown、schema 6はaccumulator、schema 7はconditional sumを追加します。いずれもC harnessを事前compileし、platform driverとsystem linker（macOSでは `cc`、Windows MSVCでは `link.exe`）を計測します。
- Schema 4のprocess-startup rowには、process creation、loader処理、小さなharness workload、output collection、終了待ちが含まれます。Operating-system cacheをflushせず、loader timeも分離しません。Executable-size fieldにはplatform C harnessとdefault linker outputが含まれます。
- Schema 5はsingle-launch rowを維持し、設定可能なbatched rowを追加します。Batched per-operation valueは、1回の遅延したlaunchがsampleへ与える影響を小さくしますが、individual startup latencyの代わりではなくamortized throughput estimateです。
- JIT add／minimum compilationのRSDは66.655%／19.507%です。Conditional-sum JIT／AOT medianは40.500 us／36.250 us、RSDは7.178%／10.384%です。これらの値は方向性の証拠として有用ですが、compile-time variationはstable performance budgetには依然として大きすぎます。
- Single-launch AOT startup medianは1.923 ms、p95は3.496 msですが、RSDは232.018%です。10-launch batched rowは1.822 ms median、1.988 ms p95をRSD 5.362%で報告します。Batchingはamortized estimateに対するhost-noiseの影響を大きく下げ、single rowはtail-sensitiveなlatencyを引き続き示します。
- 64-iteration back-edgeとimmediate returnを交互に実行すると、countdownのJIT／VM medianは12.846 ns／1.611 us、accumulatorは12.790 ns／1.665 usになります。Conditional-sum input `(64, 33)` と `(-7, 33)` を交互に実行すると、JIT／VM medianは20.011 ns／3.046 usになります。これらのbounded microbenchmarkは代表的なcontrol-flow performanceを予測しません。約1 nanosecondのdirect add／countdown／host-call rowはtimer-resolution baselineであり、信頼できるthroughput claimではありません。
- このbaselineは、より広い証拠を収集する間、prototypeでCraneliftを継続使用することを支持します。Milestone 0のbackend決定を完了するものではありません。

Cleanなnative Windows x86-64 schema-20 resultとmacOSとの比較は記録済みです。完了済みのmacOS AArch64 DWARF／dSYMおよびWindows-target CodeView／PDB source-mapping criterionと合わせ、初期native-backend decisionに計画したcross-platform evidence prerequisiteを完了しました。Schema 20が維持するschema 17 allocation traceもsyntheticなcompiler-like proxyであり、合算推定値はallocator metadataとfragmentationを分離できません。Batched startup rowは方向的なamortized evidenceには適しますがtail-latency guaranteeではなく、executable-size resultもrelease packaging budgetではなくharness artifactです。

## 以前のbaseline

[Schema 14 result](../../benchmarks/results/2026-10-16-macos-aarch64-6a2caf8.json)はcommit `6a2caf827de5b3a28746708cc36178c86a514a45` から2026-10-16 14:02:20 JSTに計測され、component-boundary seriesより前のresultであるためschema 15に置き換えられます。[Schema 13 result](../../benchmarks/results/2026-10-16-macos-aarch64-11a1af7.json)はcommit `11a1af7cac32625ebfad5a6bde921ac5382f320d` から2026-10-16 13:00:19 JSTに計測され、allocation-churn timing／profile evidenceより前のresultであるためschema 14に置き換えられます。[Schema 12 result](../../benchmarks/results/2026-10-16-macos-aarch64-fa9a69b.json)はcommit `fa9a69b8650b6a2f4b15ea645ab0e18f522a8790` から2026-10-16 12:24:45 JSTに計測され、process-level peak-memory distributionより前のresultであるためschema 13に置き換えられます。[Schema 11 result](../../benchmarks/results/2026-10-16-macos-aarch64-664edb9.json)、[Schema 10 result](../../benchmarks/results/2026-10-16-macos-aarch64-133172b.json)、[Schema 9 result](../../benchmarks/results/2026-10-16-macos-aarch64-e390a5d.json)、[Schema 8 result](../../benchmarks/results/2026-10-16-macos-aarch64-6034dc8.json)、[Schema 7 result](../../benchmarks/results/2026-10-16-macos-aarch64-0865167.json)、[Schema 6 result](../../benchmarks/results/2026-10-16-macos-aarch64-bad17cf.json)、[Schema 5 result](../../benchmarks/results/2026-10-16-macos-aarch64-180af85.json)、[Schema 4 result](../../benchmarks/results/2026-10-16-macos-aarch64-78fd3ed.json)、[Schema 3 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc216b2.json)、[Schema 2 result](../../benchmarks/results/2026-10-16-macos-aarch64-fc86889.json)、[Schema 1 AOT／link result](../../benchmarks/results/2026-10-16-macos-aarch64-0808ce7.json)、[以前のJIT-only result](../../benchmarks/results/2026-10-16-macos-aarch64-b55848c.json)はharness履歴として残します。

## 情報源

- [現在のraw macOS AArch64 schema 20 result](../../benchmarks/results/2026-10-16-macos-aarch64-de248a1.json)、2026-10-16 23:34:00 JSTに計測。
- [過去のraw macOS AArch64 schema 19 result](../../benchmarks/results/2026-10-16-macos-aarch64-76c8f81.json)、2026-10-16 22:51:15 JSTに計測。
- [過去のraw macOS AArch64 schema 18 result](../../benchmarks/results/2026-10-16-macos-aarch64-b5ddb9b.json)、2026-10-16 19:17:44 JSTに計測。
- [過去のraw macOS AArch64 schema 17 result](../../benchmarks/results/2026-10-16-macos-aarch64-aac146d.json)、2026-10-16 19:01:45 JSTに計測。
- [過去のraw macOS AArch64 schema 16 result](../../benchmarks/results/2026-10-16-macos-aarch64-7f2482a.json)、2026-10-16 18:42:15 JSTに計測。
- [過去のraw macOS AArch64 schema 15 result](../../benchmarks/results/2026-10-16-macos-aarch64-f163409.json)、2026-10-16 18:00:07 JSTに計測。
- [過去のraw macOS AArch64 schema 14 result](../../benchmarks/results/2026-10-16-macos-aarch64-6a2caf8.json)、2026-10-16 14:02:20 JSTに計測。
- `crates/vm/src/lib.rs`、deadline-enabled VM resource control、最終更新 2026-10-16 11:20 JST。
- `src/lib.rs`、cross-mode CFG／allocation-callback prototype、最終更新 2026-10-16 19:15:05 JST。
- `src/component_application_conformance.rs`、代表的multi-component application fixture、最終更新2026-10-16 23:29 JST。
- `src/bin/prototype-bench.rs`、schema 20 CFG／allocation／component／process-memory harness、最終更新 2026-10-16 23:29 JST。
- [macOS AArch64 generated-object inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-f2559b8.md)、2026-10-16 00:25 JSTに取得。
- [macOS AArch64 signed-minimum inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-dfc65f5-min.md)、2026-10-16 00:43:31 JSTに取得。
- [macOS AArch64 AOT debug-information inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-013da31-debug.md)、2026-10-16 12:06:01 JSTに取得。
- [macOS AArch64 source-mapped DWARF inspection](../../benchmarks/inspection/2026-10-16-macos-aarch64-31e182a-debug.md)、2026-10-16 19:41:57 JSTに取得。
- [Windows x86-64 source-mapped CodeView／PDB inspection](../../benchmarks/inspection/2026-10-16-windows-x86_64-465b0d6-debug.md)、2026-10-16 20:19:36 JSTに取得。
- Repository commit `76c8f81516be5eb4cd77ab063b3ecd3935d16e58`、2026-10-16にcommit。
- Repository commit `b5ddb9b6ed0e8339935c55df2a138233209f13d0`、2026-10-16にcommit。
- Repository commit `aac146dad79cc02518c809d0bb203559799f72db`、2026-10-16にcommit。
- Repository commit `7f2482a52c11354d29c61e68608f85a1f7c9611c`、2026-10-16にcommit。
- Repository commit `0865167bfe75331a29e2d4ee9e509c9d17b4b4f1`、2026-10-16にcommit。
- Repository commit `bad17cf2a42a5fc92b83f1aaebdc68e3ca2cb7a6`、2026-10-16にcommit。
- Repository commit `180af8502341c4ed00d90a658193cfa14b5ce1da`、2026-10-16にcommit。
- Repository commit `78fd3edff8d5dbeb38ff501999f7094a604404bd`、2026-10-16にcommit。
- Repository commit `fc216b26cdd8cdbd77f8e78e598f9a2143514bd3`、2026-10-16にcommit。
- Repository commit `fc86889fed093a910d575ac47eca66239a7b47e8`、2026-10-16にcommit。
- Repository commit `f2559b8046d4533907f34e93fe53c12bdb5b3cad`、2026-10-16にcommit。
- Repository commit `dfc65f5c0f944abccaf24555ba3f6dbe77017483`、2026-10-16にcommit。
- Raw resultに記録されたRust compiler metadata。Compiler commit dateは2026-09-28。

- macOS 27.0.1にinstallされたmacOS `time(1)`／`getrusage(2)` manual page。公開・更新日は提示されず、2026-10-16確認。後者は `ru_maxrss` の単位をbytesと規定します。
- Microsoft Learnの [`GetProcessMemoryInfo`](https://learn.microsoft.com/en-us/windows/win32/api/psapi/nf-psapi-getprocessmemoryinfo) と [`PROCESS_MEMORY_COUNTERS`](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters)。2018-12-05公開、2024-02-22 19:54 UTC最終更新、2026-10-17確認。`K32GetProcessMemoryInfo` を文書化し、`PeakWorkingSetSize` をprocessのpeak working-set size（bytes）と定義します。
